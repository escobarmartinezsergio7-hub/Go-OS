use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::framebuffer::{self, rgb, FramebufferInfo, PixelLayout};
use crate::fs::FileSystem;
use crate::input::{self, RuntimeInput, RuntimeKey};

use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams};
use uefi::fs::FileSystem as UefiFileSystem;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::block::BlockIO;
use uefi::CString16;
use uefi::Handle;

const LOGICAL_SECTOR_SIZE: usize = 512;
const MAX_UEFI_BLOCK_SIZE: usize = 4096;
const RESERVED_SECTORS: u16 = 32;
const FAT_COUNT: u8 = 2;
const FAT32_EOC: u32 = 0x0FFF_FFFF;
const ROOT_CLUSTER: u32 = 2;
const MIN_INSTALL_SECTORS: u32 = 131_072; // 64 MiB
const RESIZE_STEP_MIB: u32 = 128;
const RESIZE_STEP_SECTORS: u32 = RESIZE_STEP_MIB * 2048;
const DUAL_BOOT_TARGET_MIB: u32 = 16 * 1024;
const DUAL_MIN_BOOT_MIB: u32 = 8 * 1024;
const DUAL_MIN_DATA_MIB: u32 = 1024;
const DUAL_BOOT_TARGET_SECTORS: u32 = DUAL_BOOT_TARGET_MIB * 2048;
const DUAL_MIN_BOOT_SECTORS: u32 = DUAL_MIN_BOOT_MIB * 2048;
const DUAL_MIN_DATA_SECTORS: u32 = DUAL_MIN_DATA_MIB * 2048;
const RUNTIME_COPY_MAX_BYTES: usize = 256 * 1024 * 1024;
const SERVORT_COPY_MAX_BYTES: usize = 160 * 1024 * 1024;
const PAYLOAD_MAX_BYTES: usize = 256 * 1024 * 1024;
const EMBEDDED_LINUXRT_BUNDLE_MAGIC: &[u8; 6] = b"RLTB1\0";
const EMBEDDED_LINUXRT_BUNDLE: &[u8] = include_bytes!(env!("REDUX_LINUXRT_BUNDLE"));
const REQUIRED_RUNTIME_LIBFFMPEG_SHORT: &str = "RTB0422.SO";
const REQUIRED_RUNTIME_LIBFFMPEG_SOURCE: &str = "opt/simplenote/libffmpeg.so";
const REQUIRED_RUNTIME_LIBFFMPEG_LEAF: &str = "libffmpeg.so";
const STATUS_OK: u32 = 0x9DE5A8;
const STATUS_WARN: u32 = 0xFFD48A;
const STATUS_ERR: u32 = 0xFF9F9F;
const GPT_MAX_ENTRY_BYTES: usize = 8 * 1024 * 1024;
const GPT_BASIC_DATA_TYPE_GUID_LE: [u8; 16] = [
    0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99,
    0xC7,
];
const GPT_EFI_SYSTEM_TYPE_GUID_LE: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9,
    0x3B,
];

#[derive(Clone, Copy, Default)]
struct MbrPartition {
    boot: u8,
    part_type: u8,
    start_lba: u32,
    total_sectors: u32,
}

impl MbrPartition {
    fn is_used(&self) -> bool {
        self.part_type != 0 && self.total_sectors != 0
    }

    fn end_exclusive(&self) -> u32 {
        self.start_lba.saturating_add(self.total_sectors)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PartitionScheme {
    Mbr,
    Gpt,
}

#[derive(Clone)]
struct InternalDisk {
    handle: Handle,
    block_size: usize,
    total_logical_sectors: u64,
    scheme: PartitionScheme,
    partitions: Vec<MbrPartition>,
}

#[derive(Clone, Copy)]
struct TargetRef {
    disk_idx: usize,
    part_idx: usize,
}

#[derive(Clone, Copy)]
struct ArmedPartitionCreate {
    disk_idx: usize,
    part_idx: usize,
    start_lba: u32,
    total_sectors: u32,
}

struct CreatePartitionResult {
    message: String,
    boot_start_lba: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RuntimeBucket {
    Lib,
    Lib64,
    UsrLib,
    UsrLib64,
    Bin,
    Etc,
    UsrBin,
}

#[derive(Clone)]
struct RuntimeInstallFile {
    short_name: [u8; 11],
    source_path: String,
    bucket: RuntimeBucket,
    content: Vec<u8>,
}

#[derive(Clone)]
struct ServortInstallFile {
    short_name: [u8; 11],
    source_path: String,
    declared_size: u32,
    source_fat: Option<ServortFatSource>,
    content: Vec<u8>,
}

#[derive(Clone, Copy)]
struct ServortFatSource {
    volume_index: Option<usize>,
    start_cluster: u32,
}

#[derive(Clone, Copy)]
struct RuntimeFileLayout {
    short_name: [u8; 11],
    bucket: RuntimeBucket,
    first_cluster: u32,
    cluster_count: u32,
    size: u32,
}

#[derive(Clone, Copy)]
struct ServortFileLayout {
    short_name: [u8; 11],
    first_cluster: u32,
    cluster_count: u32,
    size: u32,
}

#[derive(Clone)]
struct GrubInstallAssets {
    efi_payload: Vec<u8>,
    config_payload: Vec<u8>,
}

#[derive(Clone, Copy)]
struct ClusterChainLayout {
    first_cluster: u32,
    cluster_count: u32,
}

#[derive(Clone, Copy)]
struct DirEntryLayout {
    short_name: [u8; 11],
    attr: u8,
    first_cluster: u32,
    size: u32,
}

pub enum InstallerResult {
    Skipped,
    Installed,
    Failed,
}

pub fn run() -> InstallerResult {
    let fb = match capture_framebuffer_info() {
        Some(v) => v,
        None => return InstallerResult::Skipped,
    };

    framebuffer::init(fb);
    framebuffer::enable_backbuffer();
    draw_bootstrap_progress("INITIALIZING", 2, "FRAMEBUFFER READY");
    crate::println("Preboot installer: progress 2%");

    draw_bootstrap_progress("LOADING PAYLOAD", 8, "READING EFI IMAGE");
    crate::println("Preboot installer: load payload");
    let payload = match load_bootx64_payload() {
        Ok(bytes) => bytes,
        Err(err) => {
            crate::println("Preboot installer: payload error");
            crate::println(err);
            draw_and_wait_error(err);
            return InstallerResult::Failed;
        }
    };
    draw_bootstrap_progress("LOADING PAYLOAD", 42, "PAYLOAD READY");
    crate::println("Preboot installer: progress 42%");

    draw_bootstrap_progress("LOADING LINUXRT", 50, "READING EMBEDDED BUNDLE");
    let mut runtime_error: Option<String> = None;
    let mut runtime_files = match load_runtime_from_embedded_bundle() {
        Ok(files) => files,
        Err(err) => {
            runtime_error = Some(String::from(err));
            Vec::new()
        }
    };
    ensure_required_runtime_files(&mut runtime_files);
    let runtime_total_bytes = runtime_files
        .iter()
        .fold(0usize, |acc, file| acc.saturating_add(file.content.len()));
    crate::println("Preboot installer: runtime files loaded (fast)");
    crate::println_num(runtime_files.len() as u64);
    crate::println("Preboot installer: runtime bytes loaded");
    crate::println_num(runtime_total_bytes as u64);
    draw_bootstrap_progress("LOADING LINUXRT", 72, "BUNDLE PARSED");

    draw_bootstrap_progress("LOADING SERVORT", 75, "READING SERVO LAUNCHER");
    let mut servort_files = load_servort_from_boot_fs();
    let servort_total_bytes = servort_files
        .iter()
        .fold(0usize, |acc, file| acc.saturating_add(servort_file_size_bytes(file)));
    crate::println("Preboot installer: servort files loaded");
    crate::println_num(servort_files.len() as u64);
    crate::println("Preboot installer: servort bytes loaded");
    crate::println_num(servort_total_bytes as u64);

    draw_bootstrap_progress("LOADING BOOT MANAGER", 78, "CONFIGURING BOOT FLOW");
    // GRUB payload on some media was built with a memdisk prefix and fails to
    // detect real ESP devices. Keep ReduxEFI as the installed boot manager.
    let grub_assets: Option<GrubInstallAssets> = None;
    let grub_enabled = grub_assets.is_some();
    crate::println("Preboot installer: optional GRUB payload");
    crate::println_num(if grub_enabled { 1 } else { 0 });

    draw_bootstrap_progress("SCANNING DISKS", 84, "DISCOVERING INTERNAL TARGETS");
    crate::println("Preboot installer: discover disks");
    let mut disks = discover_internal_disks();
    let mut targets = collect_targets(&disks);
    crate::println("Preboot installer: disks discovered");
    crate::println_num(disks.len() as u64);
    crate::println("Preboot installer: targets discovered");
    crate::println_num(targets.len() as u64);
    draw_bootstrap_progress("SCANNING DISKS", 96, "READY TO ENTER UI");
    crate::println("Preboot installer: enter ui loop");
    let mut selected = 0usize;
    let mut armed = false;
    let mut partition_create_armed: Option<ArmedPartitionCreate> = None;

    let mut status = if let Some(err) = runtime_error.as_ref() {
        format!("ERROR: LINUXRT BUNDLE FAILED: {}", err)
    } else if runtime_files.is_empty() {
        String::from("ERROR: LINUXRT NOT EMBEDDED IN BUILD. RUN MAKE UEFI. INSTALL BLOCKED.")
    } else if disks.is_empty() {
        String::from("NO INTERNAL DISKS DETECTED. PRESS ESC TO SKIP.")
    } else if targets.is_empty() {
        String::from("NO PARTITIONS. PRESS C TO CREATE 16 GIB FAT32 BOOT + REST EXFAT DATA.")
    } else {
        let boot_manager = if grub_enabled { "GRUB" } else { "REDUXEFI" };
        format!(
            "READY. PAYLOAD {} KB. LINUXRT {} FILES. SERVORT {} FILES. BOOTMGR {}. SELECT TARGET + ENTER.",
            payload.len() / 1024,
            runtime_files.len(),
            servort_files.len(),
            boot_manager
        )
    };
    let mut status_color = if runtime_error.is_some() || runtime_files.is_empty() {
        STATUS_ERR
    } else {
        STATUS_WARN
    };

    loop {
        draw_screen(
            disks.as_slice(),
            targets.as_slice(),
            selected,
            armed,
            status.as_str(),
            status_color,
            payload.len(),
        );
        framebuffer::present();

        if let Some(event) = input::poll_input_uefi() {
            match event {
                RuntimeInput::Key(RuntimeKey::Esc) => return InstallerResult::Skipped,
                RuntimeInput::Char(ch) => match ch {
                    'n' | 'N' => {
                        armed = false;
                        partition_create_armed = None;
                        if !targets.is_empty() {
                            selected = (selected + 1) % targets.len();
                            status = format!("SELECTED TARGET {}.", selected + 1);
                            status_color = STATUS_WARN;
                        }
                    }
                    'p' | 'P' => {
                        armed = false;
                        partition_create_armed = None;
                        if !targets.is_empty() {
                            selected = if selected == 0 { targets.len() - 1 } else { selected - 1 };
                            status = format!("SELECTED TARGET {}.", selected + 1);
                            status_color = STATUS_WARN;
                        }
                    }
                    '1'..='9' => {
                        armed = false;
                        partition_create_armed = None;
                        let idx = (ch as u8 - b'1') as usize;
                        if idx < targets.len() {
                            selected = idx;
                            status = format!("SELECTED TARGET {}.", selected + 1);
                            status_color = STATUS_WARN;
                        }
                    }
                    '+' | '=' => {
                        armed = false;
                        partition_create_armed = None;
                        match resize_selected_partition(
                            &mut disks,
                            targets.as_slice(),
                            selected,
                            true,
                        ) {
                            Ok(msg) => {
                                status = msg;
                                status_color = STATUS_OK;
                                targets = collect_targets(&disks);
                                refresh_selection(&mut selected, targets.len());
                            }
                            Err(err) => {
                                status = String::from(err);
                                status_color = STATUS_ERR;
                            }
                        }
                    }
                    '-' => {
                        armed = false;
                        partition_create_armed = None;
                        match resize_selected_partition(
                            &mut disks,
                            targets.as_slice(),
                            selected,
                            false,
                        ) {
                            Ok(msg) => {
                                status = msg;
                                status_color = STATUS_OK;
                                targets = collect_targets(&disks);
                                refresh_selection(&mut selected, targets.len());
                            }
                            Err(err) => {
                                status = String::from(err);
                                status_color = STATUS_ERR;
                            }
                        }
                    }
                    'c' | 'C' => {
                        armed = false;
                        if disks.is_empty() {
                            status = String::from("NO INTERNAL DISK AVAILABLE FOR CREATE.");
                            status_color = STATUS_ERR;
                            continue;
                        }

                        let selected_target = if selected < targets.len() {
                            Some(targets[selected])
                        } else {
                            None
                        };
                        let selected_part = selected_target.map(|t| disks[t.disk_idx].partitions[t.part_idx]);
                        let disk_idx = current_disk_index(&targets, selected, &disks);
                        let create_result = if let (Some(armed_create), Some(target), Some(part)) =
                            (partition_create_armed, selected_target, selected_part)
                        {
                            if armed_create.disk_idx == target.disk_idx
                                && armed_create.part_idx == target.part_idx
                                && armed_create.start_lba == part.start_lba
                                && armed_create.total_sectors == part.total_sectors
                            {
                                create_dual_partitions_from_selected(&mut disks, target)
                            } else {
                                Err("SELECTED TARGET CHANGED. PRESS C AGAIN TO ARM SAFE PARTITION SPLIT.")
                            }
                        } else if let (Some(target), Some(part)) = (selected_target, selected_part) {
                            if !is_install_target_partition_type(part.part_type) {
                                partition_create_armed = Some(ArmedPartitionCreate {
                                    disk_idx: target.disk_idx,
                                    part_idx: target.part_idx,
                                    start_lba: part.start_lba,
                                    total_sectors: part.total_sectors,
                                });
                                status = format!(
                                    "ARMED: PRESS C AGAIN TO ERASE ONLY TARGET {} ({} MIB) AND SPLIT IT INTO FAT32 BOOT + EXFAT DATA.",
                                    selected + 1,
                                    part.total_sectors as u64 / 2048
                                );
                                status_color = STATUS_ERR;
                                continue;
                            }
                            create_partition_on_disk(&mut disks, disk_idx)
                        } else {
                            create_partition_on_disk(&mut disks, disk_idx)
                        };
                        match create_result {
                            Ok(created) => {
                                partition_create_armed = None;
                                status = created.message;
                                status_color = STATUS_OK;
                                targets = collect_targets(&disks);
                                selected = find_target_by_start_lba(
                                    targets.as_slice(),
                                    &disks,
                                    disk_idx,
                                    created.boot_start_lba,
                                )
                                .unwrap_or_else(|| targets.len().saturating_sub(1));
                            }
                            Err(err) => {
                                partition_create_armed = selected_target.map(|target| {
                                    let part = disks[target.disk_idx].partitions[target.part_idx];
                                    ArmedPartitionCreate {
                                        disk_idx: target.disk_idx,
                                        part_idx: target.part_idx,
                                        start_lba: part.start_lba,
                                        total_sectors: part.total_sectors,
                                    }
                                });
                                if let Some(part) = selected_part {
                                    status = format!(
                                        "{} PRESS C AGAIN TO ERASE ONLY SELECTED PARTITION {} ({} MIB), NOT THE WHOLE DISK.",
                                        err,
                                        selected + 1,
                                        part.total_sectors as u64 / 2048
                                    );
                                } else {
                                    status = format!("CREATE ERROR: {}", err);
                                }
                                status_color = STATUS_ERR;
                            }
                        }
                    }
                    'r' | 'R' => {
                        armed = false;
                        partition_create_armed = None;
                        draw_bootstrap_progress("RESCAN", 35, "REFRESHING DISKS + LINUXRT + SERVORT");
                        disks = discover_internal_disks();
                        targets = collect_targets(&disks);
                        runtime_error = None;
                        runtime_files = match load_runtime_from_embedded_bundle() {
                            Ok(files) => files,
                            Err(err) => {
                                runtime_error = Some(String::from(err));
                                Vec::new()
                            }
                        };
                        ensure_required_runtime_files(&mut runtime_files);
                        servort_files = load_servort_from_boot_fs();
                        refresh_selection(&mut selected, targets.len());

                        status = if let Some(err) = runtime_error.as_ref() {
                            format!("ERROR: LINUXRT BUNDLE FAILED: {}", err)
                        } else if runtime_files.is_empty() {
                            String::from("ERROR: LINUXRT NOT EMBEDDED IN BUILD.")
                        } else if disks.is_empty() {
                            String::from("NO INTERNAL DISKS DETECTED.")
                        } else if targets.is_empty() {
                            String::from("RELOADED. NO PARTITIONS; PRESS C TO CREATE 16 GIB FAT32 BOOT + REST EXFAT DATA.")
                        } else {
                            format!(
                                "RELOADED. C CREATES IN FREE SPACE OR SPLITS ONLY SELECTED DATA TARGET. SERVORT FILES={}.",
                                servort_files.len()
                            )
                        };
                        status_color = if runtime_error.is_some() || runtime_files.is_empty() {
                            STATUS_ERR
                        } else {
                            STATUS_WARN
                        };
                    }
                    _ => {}
                },
                RuntimeInput::Enter => {
                    partition_create_armed = None;
                    if let Some(err) = runtime_error.as_ref() {
                        status = format!("INSTALL BLOCKED: LINUXRT BUNDLE FAILED ({})", err);
                        status_color = STATUS_ERR;
                        continue;
                    }
                    if runtime_files.is_empty() {
                        status = String::from(
                            "INSTALL BLOCKED: LINUXRT IS REQUIRED. REBUILD IMAGE (MAKE UEFI).",
                        );
                        status_color = STATUS_ERR;
                        continue;
                    }

                    if targets.is_empty() {
                        status = String::from("NO INSTALL TARGET. CREATE A PARTITION FIRST WITH C.");
                        status_color = STATUS_ERR;
                        continue;
                    }

                    let target = targets[selected];
                    let disk = &disks[target.disk_idx];
                    let part = disk.partitions[target.part_idx];
                    let paired_data_start_lba =
                        paired_data_partition_after_boot(disk, target.part_idx).map(|p| p.start_lba as u64);

                    if !is_install_target_partition_type(part.part_type) {
                        armed = false;
                        status = format!(
                            "TARGET {} TYPE {:02X} IS DATA/OTHER. PRESS C TWICE TO SPLIT ONLY THIS PARTITION, OR SELECT FAT32/EFI.",
                            selected + 1,
                            part.part_type
                        );
                        status_color = STATUS_ERR;
                        continue;
                    }

                    if !armed {
                        armed = true;
                        status = format!(
                            "DANGER: ENTER AGAIN TO FACTORY-RESET TARGET {} AND INSTALL.",
                            selected + 1
                        );
                        status_color = STATUS_ERR;
                        continue;
                    }


                    status = String::from("INSTALLING [0%] PREPARING TARGET. DO NOT POWER OFF.");
                    status_color = STATUS_WARN;
                    draw_screen(
                        disks.as_slice(),
                        targets.as_slice(),
                        selected,
                        armed,
                        status.as_str(),
                        status_color,
                        payload.len(),
                    );
                    framebuffer::present();

                    let mut install_last_percent = 0u8;
                    let mut install_last_detail = String::new();
                    let mut install_progress = |percent: u8, detail: &str| {
                        let pct = core::cmp::min(100u8, percent);
                        if pct == install_last_percent && install_last_detail.as_str() == detail {
                            return;
                        }
                        install_last_percent = pct;
                        install_last_detail.clear();
                        install_last_detail.push_str(detail);
                        status = format!("INSTALLING [{}%] {}. DO NOT POWER OFF.", pct, detail);
                        status_color = STATUS_WARN;
                        draw_screen(
                            disks.as_slice(),
                            targets.as_slice(),
                            selected,
                            armed,
                            status.as_str(),
                            status_color,
                            payload.len(),
                        );
                        framebuffer::present();
                    };

                    match install_to_partition(
                        disk.handle,
                        part.start_lba as u64,
                        part.total_sectors as u64,
                        paired_data_start_lba,
                        payload.as_slice(),
                        grub_assets.as_ref(),
                        runtime_files.as_slice(),
                        servort_files.as_slice(),
                        &mut install_progress,
                    ) {
                        Ok(()) => {
                            unsafe {
                                crate::fat32::GLOBAL_FAT.unmount();
                            }
                            if runtime_files.is_empty() {
                                status = String::from(
                                    "INSTALL COMPLETE. BOOT + SYSTEM COPIED. LINUXRT NOT EMBEDDED IN BUILD.",
                                );
                            } else {
                                status = format!(
                                    "INSTALL COMPLETE. BOOT + SYSTEM + LINUXRT({}) + SERVORT({}) COPIED.",
                                    runtime_files.len(),
                                    servort_files.len()
                                );
                            }
                            if grub_enabled {
                                status.push_str(" GRUB MENU ENABLED.");
                            }
                            status_color = STATUS_OK;
                            draw_screen(
                                disks.as_slice(),
                                targets.as_slice(),
                                selected,
                                false,
                                status.as_str(),
                                status_color,
                                payload.len(),
                            );
                            framebuffer::present();
                            boot::stall(900_000);
                            return InstallerResult::Installed;
                        }
                        Err(err) => {
                            armed = false;
                            status = format!("INSTALL ERROR: {}", err);
                            status_color = STATUS_ERR;
                        }
                    }
                }
                _ => {}
            }
        }

        boot::stall(4_000);
    }
}

fn draw_and_wait_error(err: &str) {
    crate::println("Preboot installer: fatal error");
    crate::println(err);
    loop {
        framebuffer::clear(rgb(10, 14, 24));
        framebuffer::rect(0, 0, 1600, 72, rgb(36, 14, 14));
        framebuffer::draw_text_5x7(24, 22, "PREBOOT INSTALLER ERROR", rgb(255, 220, 220));
        framebuffer::draw_text_5x7(24, 96, err, STATUS_ERR);
        framebuffer::draw_text_5x7(24, 120, "PRESS ESC TO CONTINUE TO SHELL.", rgb(230, 230, 230));
        framebuffer::present();

        if let Some(RuntimeInput::Key(RuntimeKey::Esc)) = input::poll_input_uefi() {
            return;
        }

        boot::stall(4_000);
    }
}

fn draw_bootstrap_status(msg: &str) {
    let (w, h) = framebuffer::dimensions();
    framebuffer::clear(rgb(8, 12, 20));
    framebuffer::rect(0, 0, w, 72, rgb(12, 34, 70));
    framebuffer::draw_text_5x7(
        24,
        22,
        "ZENOX OS PREBOOT INSTALLER",
        rgb(220, 235, 255),
    );
    framebuffer::draw_text_5x7(24, 110, msg, rgb(196, 214, 241));
    framebuffer::draw_text_5x7(24, h.saturating_sub(38), "PLEASE WAIT...", rgb(170, 190, 218));
    framebuffer::present();
}

fn draw_bootstrap_progress(stage: &str, percent: u8, detail: &str) {
    let p = core::cmp::min(percent, 100);
    let msg = format!("{} [{}%] {}", stage, p, detail);
    draw_bootstrap_status(msg.as_str());
}

fn draw_screen(
    disks: &[InternalDisk],
    targets: &[TargetRef],
    selected: usize,
    armed: bool,
    status: &str,
    status_color: u32,
    payload_len: usize,
) {
    let (w, h) = framebuffer::dimensions();
    framebuffer::clear(rgb(10, 14, 24));

    framebuffer::rect(0, 0, w, 72, rgb(12, 34, 70));
    framebuffer::draw_text_5x7(
        24,
        20,
        "ZENOX OS GRAPHICAL INSTALLER (PRE-BOOT)",
        rgb(220, 235, 255),
    );
    framebuffer::draw_text_5x7(
        24,
        40,
        "CAN RESIZE + CREATE PARTITIONS, THEN INSTALL SYSTEM BOOT PACKAGE",
        rgb(168, 198, 235),
    );

    let panel_x = 20usize;
    let panel_y = 90usize;
    let panel_w = w.saturating_sub(40);
    let panel_h = h.saturating_sub(180);
    framebuffer::rect(panel_x, panel_y, panel_w, panel_h, rgb(20, 28, 46));
    framebuffer::rect(panel_x, panel_y, panel_w, 28, rgb(26, 44, 74));
    framebuffer::draw_text_5x7(panel_x + 12, panel_y + 10, "INSTALL TARGETS", rgb(229, 239, 255));

    let info = format!(
        "PAYLOAD {} KB   DISKS {}   TARGETS {}",
        payload_len / 1024,
        disks.len(),
        targets.len()
    );
    framebuffer::draw_text_5x7(panel_x + 190, panel_y + 10, info.as_str(), rgb(180, 210, 250));

    if targets.is_empty() {
        if disks.is_empty() {
            framebuffer::draw_text_5x7(
                panel_x + 14,
                panel_y + 48,
                "NO INTERNAL DISKS DETECTED.",
                STATUS_ERR,
            );
        } else {
            framebuffer::draw_text_5x7(
                panel_x + 14,
                panel_y + 48,
                "NO PARTITIONS FOUND. PRESS C TO CREATE 16 GIB FAT32 BOOT + REST EXFAT DATA.",
                STATUS_WARN,
            );
        }
    } else {
        let line_h = 18usize;
        let max_lines = ((panel_h.saturating_sub(88)) / line_h).max(1);
        let count = core::cmp::min(targets.len(), max_lines);

        for i in 0..count {
            let tref = targets[i];
            let disk = &disks[tref.disk_idx];
            let part = disk.partitions[tref.part_idx];
            let y = panel_y + 40 + i * line_h;

            if i == selected {
                framebuffer::rect(
                    panel_x + 8,
                    y - 2,
                    panel_w.saturating_sub(16),
                    14,
                    rgb(44, 82, 138),
                );
            }

            let mib = part.total_sectors as u64 / 2048;
            let scheme = match disk.scheme {
                PartitionScheme::Mbr => "MBR",
                PartitionScheme::Gpt => "GPT",
            };
            let role = partition_role_label(part.part_type);
            let line = format!(
                "{}. DISK {} PART {}  {} {:02X} {}  START {}  SIZE {} MIB",
                i + 1,
                tref.disk_idx + 1,
                tref.part_idx + 1,
                scheme,
                part.part_type,
                role,
                part.start_lba,
                mib
            );
            let fg = if i == selected { rgb(255, 255, 255) } else { rgb(196, 214, 241) };
            framebuffer::draw_text_5x7(panel_x + 14, y, line.as_str(), fg);
        }

        if targets.len() > count {
            let more = format!("{} MORE TARGETS NOT SHOWN", targets.len() - count);
            framebuffer::draw_text_5x7(panel_x + 14, panel_y + panel_h - 46, more.as_str(), rgb(170, 180, 196));
        }
    }

    let help_y = panel_y + panel_h - 78;
    framebuffer::draw_text_5x7(
        panel_x + 12,
        help_y,
        "N/P MOVE  +/- RESIZE  C CREATE/SPLIT  R RELOAD  1-9 SELECT  ENTER INSTALL  ESC SKIP",
        rgb(191, 209, 236),
    );
    framebuffer::draw_text_5x7(
        panel_x + 12,
        help_y + 14,
        "C USES FREE SPACE; ON DATA/OTHER, C AGAIN ERASES ONLY SELECTED PARTITION. STEP = 128 MIB.",
        rgb(170, 190, 218),
    );

    if armed {
        framebuffer::draw_text_5x7(
            panel_x + 12,
            help_y + 30,
            "CONFIRMATION ARMED: ENTER AGAIN WILL FACTORY-RESET THE SELECTED PARTITION.",
            STATUS_ERR,
        );
    }

    framebuffer::rect(0, h.saturating_sub(56), w, 56, rgb(13, 19, 32));
    framebuffer::draw_text_5x7(20, h.saturating_sub(38), status, status_color);
}

fn discover_internal_disks() -> Vec<InternalDisk> {
    let mut out = Vec::new();
    let handles = boot::find_handles::<BlockIO>().unwrap_or_default();
    crate::println("Preboot installer: block handles found");
    crate::println_num(handles.len() as u64);

    let mut sorted_handles: Vec<Handle> = handles.iter().copied().collect();
    sorted_handles.sort_unstable();

    for handle in sorted_handles.iter().copied() {
        let Some((block_size, total_logical_sectors)) = describe_physical_disk(handle) else {
            continue;
        };

        crate::println("Preboot installer: accepted physical disk sectors");
        crate::println_num(total_logical_sectors);

        crate::println("Preboot installer: probing disk MBR");
        let mut sector0 = [0u8; LOGICAL_SECTOR_SIZE];
        let mbr_partitions = if read_sector_from_uefi_handle(handle, 0, &mut sector0)
            && sector0[510] == 0x55
            && sector0[511] == 0xAA
        {
            parse_mbr_partitions(&sector0)
        } else {
            [MbrPartition::default(); 4]
        };

        let is_gpt = mbr_partitions.iter().any(|p| p.part_type == 0xEE);
        if is_gpt {
            crate::println("Preboot installer: GPT protective MBR detected");
            let gpt_partitions = match parse_gpt_partitions(handle, block_size, total_logical_sectors) {
                Ok(v) => v,
                Err(_) => {
                    crate::println("Preboot installer: GPT parse failed");
                    Vec::new()
                }
            };
            crate::println("Preboot installer: GPT partitions parsed");
            crate::println_num(gpt_partitions.len() as u64);

            out.push(InternalDisk {
                handle,
                block_size,
                total_logical_sectors,
                scheme: PartitionScheme::Gpt,
                partitions: gpt_partitions,
            });
            crate::println("Preboot installer: accepted disk");
            continue;
        }

        out.push(InternalDisk {
            handle,
            block_size,
            total_logical_sectors,
            scheme: PartitionScheme::Mbr,
            partitions: mbr_partitions.to_vec(),
        });
        crate::println("Preboot installer: accepted disk");
    }

    out
}

fn current_boot_device_handle() -> Option<Handle> {
    let params = OpenProtocolParams {
        handle: boot::image_handle(),
        agent: boot::image_handle(),
        controller: None,
    };

    let loaded = unsafe {
        boot::open_protocol::<LoadedImage>(params, OpenProtocolAttributes::GetProtocol)
    }
    .ok()?;

    loaded.device()
}

fn handle_is_removable(handle: Handle) -> Option<bool> {
    let blk = boot::open_protocol_exclusive::<BlockIO>(handle).ok()?;
    Some(blk.media().is_removable_media())
}

fn describe_physical_disk(handle: Handle) -> Option<(usize, u64)> {
    let params = OpenProtocolParams {
        handle,
        agent: boot::image_handle(),
        controller: None,
    };

    let blk = unsafe {
        boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol)
    }
    .ok()?;

    let media = blk.media();
    if !media.is_media_present() {
        return None;
    }
    if media.is_removable_media() {
        return None;
    }
    if media.is_logical_partition() {
        return None;
    }
    if media.is_read_only() {
        return None;
    }

    let block_size = media.block_size() as usize;
    if block_size < LOGICAL_SECTOR_SIZE
        || block_size > MAX_UEFI_BLOCK_SIZE
        || (block_size % LOGICAL_SECTOR_SIZE) != 0
    {
        return None;
    }

    let total_blocks = media.last_block().saturating_add(1);
    if total_blocks == 0 {
        return None;
    }

    let total_logical_sectors = total_blocks.saturating_mul(block_size as u64) / LOGICAL_SECTOR_SIZE as u64;
    if total_logical_sectors < MIN_INSTALL_SECTORS as u64 {
        return None;
    }

    Some((block_size, total_logical_sectors))
}

fn parse_mbr_partitions(sector0: &[u8; LOGICAL_SECTOR_SIZE]) -> [MbrPartition; 4] {
    let mut out = [MbrPartition::default(); 4];

    let mut i = 0usize;
    while i < 4 {
        let off = 446 + i * 16;
        let boot = sector0[off];
        let part_type = sector0[off + 4];

        let mut lba_b = [0u8; 4];
        lba_b.copy_from_slice(&sector0[off + 8..off + 12]);
        let start_lba = u32::from_le_bytes(lba_b);

        let mut sz_b = [0u8; 4];
        sz_b.copy_from_slice(&sector0[off + 12..off + 16]);
        let total_sectors = u32::from_le_bytes(sz_b);

        out[i] = MbrPartition {
            boot,
            part_type,
            start_lba,
            total_sectors,
        };
        i += 1;
    }

    out
}

#[derive(Clone)]
struct GptMeta {
    lba_mul: u64,
    primary_header_lba_native: u64,
    backup_header_lba_native: u64,
    first_usable_lba_native: u64,
    last_usable_lba_native: u64,
    primary_entry_lba_native: u64,
    backup_entry_lba_native: u64,
    entry_count: usize,
    entry_size: usize,
    header_size: usize,
    primary_header_sector: [u8; LOGICAL_SECTOR_SIZE],
    backup_header_sector: [u8; LOGICAL_SECTOR_SIZE],
    entries: Vec<u8>,
}

fn load_gpt_meta(disk: &InternalDisk) -> Result<GptMeta, &'static str> {
    if disk.scheme != PartitionScheme::Gpt {
        return Err("DISK IS NOT GPT.");
    }
    if disk.block_size == 0 || (disk.block_size % LOGICAL_SECTOR_SIZE) != 0 {
        return Err("GPT BLOCK SIZE INVALID.");
    }
    let lba_mul = (disk.block_size / LOGICAL_SECTOR_SIZE) as u64;
    if lba_mul == 0 {
        return Err("GPT LBA SCALE INVALID.");
    }

    let mut primary_header = [0u8; LOGICAL_SECTOR_SIZE];
    if !read_sector_from_uefi_handle(disk.handle, lba_mul, &mut primary_header) {
        return Err("GPT PRIMARY HEADER READ FAILED.");
    }
    if &primary_header[0..8] != b"EFI PART" {
        return Err("GPT PRIMARY HEADER SIGNATURE INVALID.");
    }

    let header_size = read_u32_le(&primary_header, 12).ok_or("GPT HEADER SIZE MISSING.")? as usize;
    if header_size < 92 || header_size > LOGICAL_SECTOR_SIZE {
        return Err("GPT HEADER SIZE INVALID.");
    }

    let primary_header_lba_native =
        read_u64_le(&primary_header, 24).ok_or("GPT CURRENT LBA MISSING.")?;
    let backup_header_lba_native =
        read_u64_le(&primary_header, 32).ok_or("GPT BACKUP LBA MISSING.")?;
    let first_usable_lba_native =
        read_u64_le(&primary_header, 40).ok_or("GPT FIRST_USABLE_LBA MISSING.")?;
    let last_usable_lba_native =
        read_u64_le(&primary_header, 48).ok_or("GPT LAST_USABLE_LBA MISSING.")?;
    let primary_entry_lba_native =
        read_u64_le(&primary_header, 72).ok_or("GPT ENTRY LBA MISSING.")?;
    let entry_count = read_u32_le(&primary_header, 80).ok_or("GPT ENTRY COUNT MISSING.")? as usize;
    let entry_size = read_u32_le(&primary_header, 84).ok_or("GPT ENTRY SIZE MISSING.")? as usize;

    if entry_count == 0 || entry_size < 128 || entry_size > 1024 {
        return Err("GPT ENTRY GEOMETRY INVALID.");
    }
    let entry_bytes = entry_count
        .checked_mul(entry_size)
        .ok_or("GPT ENTRY BYTES OVERFLOW.")?;
    if entry_bytes == 0 || entry_bytes > GPT_MAX_ENTRY_BYTES {
        return Err("GPT ENTRY ARRAY TOO LARGE.");
    }

    let entries = read_gpt_entry_array(disk.handle, primary_entry_lba_native, lba_mul, entry_bytes)?;

    let mut backup_header = [0u8; LOGICAL_SECTOR_SIZE];
    let mut backup_entry_lba_native = 0u64;
    let backup_header_lba_512 = backup_header_lba_native.saturating_mul(lba_mul);
    if read_sector_from_uefi_handle(disk.handle, backup_header_lba_512, &mut backup_header)
        && &backup_header[0..8] == b"EFI PART"
    {
        backup_entry_lba_native = read_u64_le(&backup_header, 72).unwrap_or(0);
    }

    if backup_entry_lba_native == 0 {
        let entry_array_native = (entry_bytes as u64)
            .saturating_add(disk.block_size as u64 - 1)
            / disk.block_size as u64;
        backup_entry_lba_native = backup_header_lba_native.saturating_sub(entry_array_native);
        backup_header = primary_header;
    }

    Ok(GptMeta {
        lba_mul,
        primary_header_lba_native,
        backup_header_lba_native,
        first_usable_lba_native,
        last_usable_lba_native,
        primary_entry_lba_native,
        backup_entry_lba_native,
        entry_count,
        entry_size,
        header_size,
        primary_header_sector: primary_header,
        backup_header_sector: backup_header,
        entries,
    })
}

fn read_gpt_entry_array(
    handle: Handle,
    entry_lba_native: u64,
    lba_mul: u64,
    entry_bytes: usize,
) -> Result<Vec<u8>, &'static str> {
    if entry_bytes == 0 {
        return Err("GPT ENTRY BYTES INVALID.");
    }
    let sectors = (entry_bytes + LOGICAL_SECTOR_SIZE - 1) / LOGICAL_SECTOR_SIZE;

    let mut raw = Vec::new();
    raw.resize(entry_bytes, 0);

    let mut i = 0usize;
    while i < sectors {
        let mut sec = [0u8; LOGICAL_SECTOR_SIZE];
        let lba512 = entry_lba_native
            .saturating_mul(lba_mul)
            .saturating_add(i as u64);
        if !read_sector_from_uefi_handle(handle, lba512, &mut sec) {
            return Err("GPT ENTRY ARRAY READ FAILED.");
        }
        let dst = i * LOGICAL_SECTOR_SIZE;
        let copy_len = core::cmp::min(LOGICAL_SECTOR_SIZE, entry_bytes.saturating_sub(dst));
        if copy_len > 0 {
            raw[dst..dst + copy_len].copy_from_slice(&sec[0..copy_len]);
        }
        i += 1;
    }

    Ok(raw)
}

fn write_gpt_entry_array(
    handle: Handle,
    entry_lba_native: u64,
    lba_mul: u64,
    entries: &[u8],
) -> Result<(), &'static str> {
    if entries.is_empty() {
        return Err("GPT ENTRY ARRAY EMPTY.");
    }
    let sectors = (entries.len() + LOGICAL_SECTOR_SIZE - 1) / LOGICAL_SECTOR_SIZE;

    let mut i = 0usize;
    while i < sectors {
        let mut sec = [0u8; LOGICAL_SECTOR_SIZE];
        let src = i * LOGICAL_SECTOR_SIZE;
        let copy_len = core::cmp::min(LOGICAL_SECTOR_SIZE, entries.len().saturating_sub(src));
        if copy_len > 0 {
            sec[0..copy_len].copy_from_slice(&entries[src..src + copy_len]);
        }

        let lba512 = entry_lba_native
            .saturating_mul(lba_mul)
            .saturating_add(i as u64);
        if !write_sector_to_uefi_handle(handle, lba512, &sec) {
            return Err("GPT ENTRY ARRAY WRITE FAILED.");
        }
        i += 1;
    }

    Ok(())
}

fn gpt_used_entries_sorted(meta: &GptMeta) -> Vec<(usize, u64)> {
    let mut out = Vec::<(usize, u64)>::new();

    let mut i = 0usize;
    while i < meta.entry_count {
        let off = i * meta.entry_size;
        let end = off + meta.entry_size;
        if end > meta.entries.len() {
            break;
        }
        let entry = &meta.entries[off..end];
        if !gpt_entry_is_used(entry) {
            i += 1;
            continue;
        }

        let first = match read_u64_le(entry, 32) {
            Some(v) => v,
            None => {
                i += 1;
                continue;
            }
        };
        let last = match read_u64_le(entry, 40) {
            Some(v) => v,
            None => {
                i += 1;
                continue;
            }
        };
        if first == 0 || last < first {
            i += 1;
            continue;
        }
        if first < meta.first_usable_lba_native || last > meta.last_usable_lba_native {
            i += 1;
            continue;
        }
        out.push((i, first));
        i += 1;
    }

    out.sort_by(|a, b| {
        if a.1 != b.1 {
            return a.1.cmp(&b.1);
        }
        a.0.cmp(&b.0)
    });
    out
}

fn gpt_entry_is_used(entry: &[u8]) -> bool {
    if entry.len() < 128 {
        return false;
    }
    !entry[0..16].iter().all(|b| *b == 0)
}

fn gpt_find_empty_entry(meta: &GptMeta) -> Option<usize> {
    let mut i = 0usize;
    while i < meta.entry_count {
        let off = i * meta.entry_size;
        let end = off + meta.entry_size;
        if end > meta.entries.len() {
            return None;
        }
        if !gpt_entry_is_used(&meta.entries[off..end]) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn gpt_entry_mut<'a>(meta: &'a mut GptMeta, idx: usize) -> Result<&'a mut [u8], &'static str> {
    if idx >= meta.entry_count {
        return Err("GPT ENTRY INDEX OUT OF RANGE.");
    }
    let off = idx
        .checked_mul(meta.entry_size)
        .ok_or("GPT ENTRY OFFSET OVERFLOW.")?;
    let end = off
        .checked_add(meta.entry_size)
        .ok_or("GPT ENTRY END OVERFLOW.")?;
    if end > meta.entries.len() {
        return Err("GPT ENTRY BUFFER OUT OF RANGE.");
    }
    Ok(&mut meta.entries[off..end])
}

fn gpt_write_entry(
    meta: &mut GptMeta,
    idx: usize,
    start_lba_native: u64,
    total_lba_native: u64,
) -> Result<(), &'static str> {
    gpt_write_entry_typed(
        meta,
        idx,
        start_lba_native,
        total_lba_native,
        GPT_BASIC_DATA_TYPE_GUID_LE,
        "ZENOX OS",
    )
}

fn gpt_write_entry_typed(
    meta: &mut GptMeta,
    idx: usize,
    start_lba_native: u64,
    total_lba_native: u64,
    type_guid_le: [u8; 16],
    name: &str,
) -> Result<(), &'static str> {
    if start_lba_native == 0 || total_lba_native == 0 {
        return Err("GPT ENTRY RANGE INVALID.");
    }
    let last_lba_native = start_lba_native
        .checked_add(total_lba_native - 1)
        .ok_or("GPT ENTRY LAST LBA OVERFLOW.")?;
    if start_lba_native < meta.first_usable_lba_native || last_lba_native > meta.last_usable_lba_native {
        return Err("GPT ENTRY OUTSIDE USABLE RANGE.");
    }

    let entry = gpt_entry_mut(meta, idx)?;
    for b in entry.iter_mut() {
        *b = 0;
    }

    entry[0..16].copy_from_slice(&type_guid_le);

    let mut unique = [0u8; 16];
    unique[0..8].copy_from_slice(&start_lba_native.to_le_bytes());
    unique[8..16].copy_from_slice(&last_lba_native.to_le_bytes());
    unique[15] ^= (idx as u8).wrapping_mul(37);
    if unique.iter().all(|b| *b == 0) {
        unique[0] = 1;
    }
    entry[16..32].copy_from_slice(&unique);

    entry[32..40].copy_from_slice(&start_lba_native.to_le_bytes());
    entry[40..48].copy_from_slice(&last_lba_native.to_le_bytes());
    entry[48..56].copy_from_slice(&0u64.to_le_bytes());
    gpt_write_name(entry, name);

    Ok(())
}

fn gpt_write_name(entry: &mut [u8], name: &str) {
    if entry.len() < 128 {
        return;
    }
    let mut off = 56usize;
    for ch in name.bytes() {
        if off + 2 > 128 {
            break;
        }
        entry[off] = ch;
        entry[off + 1] = 0;
        off += 2;
    }
}

fn gpt_write_tables(handle: Handle, meta: &mut GptMeta) -> Result<(), &'static str> {
    let entry_crc = crc32_ieee(&meta.entries);

    set_u64_le(&mut meta.primary_header_sector, 24, meta.primary_header_lba_native)?;
    set_u64_le(&mut meta.primary_header_sector, 32, meta.backup_header_lba_native)?;
    set_u64_le(&mut meta.primary_header_sector, 40, meta.first_usable_lba_native)?;
    set_u64_le(&mut meta.primary_header_sector, 48, meta.last_usable_lba_native)?;
    set_u64_le(&mut meta.primary_header_sector, 72, meta.primary_entry_lba_native)?;
    set_u32_le(&mut meta.primary_header_sector, 80, meta.entry_count as u32)?;
    set_u32_le(&mut meta.primary_header_sector, 84, meta.entry_size as u32)?;
    set_u32_le(&mut meta.primary_header_sector, 88, entry_crc)?;
    set_u32_le(&mut meta.primary_header_sector, 16, 0)?;
    let primary_crc = crc32_ieee(&meta.primary_header_sector[0..meta.header_size]);
    set_u32_le(&mut meta.primary_header_sector, 16, primary_crc)?;

    set_u64_le(&mut meta.backup_header_sector, 24, meta.backup_header_lba_native)?;
    set_u64_le(&mut meta.backup_header_sector, 32, meta.primary_header_lba_native)?;
    set_u64_le(&mut meta.backup_header_sector, 40, meta.first_usable_lba_native)?;
    set_u64_le(&mut meta.backup_header_sector, 48, meta.last_usable_lba_native)?;
    set_u64_le(&mut meta.backup_header_sector, 72, meta.backup_entry_lba_native)?;
    set_u32_le(&mut meta.backup_header_sector, 80, meta.entry_count as u32)?;
    set_u32_le(&mut meta.backup_header_sector, 84, meta.entry_size as u32)?;
    set_u32_le(&mut meta.backup_header_sector, 88, entry_crc)?;
    set_u32_le(&mut meta.backup_header_sector, 16, 0)?;
    let backup_crc = crc32_ieee(&meta.backup_header_sector[0..meta.header_size]);
    set_u32_le(&mut meta.backup_header_sector, 16, backup_crc)?;

    write_gpt_entry_array(handle, meta.primary_entry_lba_native, meta.lba_mul, &meta.entries)?;
    write_gpt_entry_array(handle, meta.backup_entry_lba_native, meta.lba_mul, &meta.entries)?;

    let primary_header_lba_512 = meta.primary_header_lba_native.saturating_mul(meta.lba_mul);
    if !write_sector_to_uefi_handle(handle, primary_header_lba_512, &meta.primary_header_sector) {
        return Err("GPT PRIMARY HEADER WRITE FAILED.");
    }

    let backup_header_lba_512 = meta.backup_header_lba_native.saturating_mul(meta.lba_mul);
    if !write_sector_to_uefi_handle(handle, backup_header_lba_512, &meta.backup_header_sector) {
        return Err("GPT BACKUP HEADER WRITE FAILED.");
    }

    Ok(())
}

fn sectors512_to_native_exact(sectors_512: u32, lba_mul: u64) -> Result<u64, &'static str> {
    if lba_mul == 0 {
        return Err("INVALID GPT LBA SCALE.");
    }
    let s = sectors_512 as u64;
    if s % lba_mul != 0 {
        return Err("PARTITION GEOMETRY IS NOT NATIVE-LBA ALIGNED.");
    }
    Ok(s / lba_mul)
}

fn round_down_to_multiple_u32(value: u32, align: u32) -> u32 {
    if align <= 1 {
        return value;
    }
    value - (value % align)
}

fn set_u32_le(buf: &mut [u8], off: usize, value: u32) -> Result<(), &'static str> {
    if off + 4 > buf.len() {
        return Err("BUFFER WRITE OUT OF RANGE.");
    }
    buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn set_u64_le(buf: &mut [u8], off: usize, value: u64) -> Result<(), &'static str> {
    if off + 8 > buf.len() {
        return Err("BUFFER WRITE OUT OF RANGE.");
    }
    buf[off..off + 8].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        let mut i = 0;
        while i < 8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320u32 & mask);
            i += 1;
        }
    }
    !crc
}

fn parse_gpt_partitions(
    handle: Handle,
    block_size: usize,
    total_logical_sectors: u64,
) -> Result<Vec<MbrPartition>, &'static str> {
    if block_size == 0 || (block_size % LOGICAL_SECTOR_SIZE) != 0 {
        return Err("GPT BLOCK SIZE INVALID.");
    }
    let lba_mul = (block_size / LOGICAL_SECTOR_SIZE) as u64;
    if lba_mul == 0 {
        return Err("GPT LBA SCALE INVALID.");
    }

    let mut header = [0u8; LOGICAL_SECTOR_SIZE];
    if !read_sector_from_uefi_handle(handle, lba_mul, &mut header) {
        return Err("GPT HEADER READ FAILED.");
    }

    if &header[0..8] != b"EFI PART" {
        return Err("GPT HEADER SIGNATURE INVALID.");
    }

    let first_usable_lba_native = read_u64_le(&header, 40).ok_or("GPT FIRST_USABLE_LBA MISSING.")?;
    let last_usable_lba_native = read_u64_le(&header, 48).ok_or("GPT LAST_USABLE_LBA MISSING.")?;
    let entry_lba_native = read_u64_le(&header, 72).ok_or("GPT ENTRY LBA MISSING.")?;
    let entry_count = read_u32_le(&header, 80).ok_or("GPT ENTRY COUNT MISSING.")? as usize;
    let entry_size = read_u32_le(&header, 84).ok_or("GPT ENTRY SIZE MISSING.")? as usize;

    if entry_count == 0 || entry_size < 128 || entry_size > 1024 {
        return Err("GPT ENTRY GEOMETRY INVALID.");
    }

    let scan_entries = core::cmp::min(entry_count, 256);
    let scan_bytes = scan_entries
        .checked_mul(entry_size)
        .ok_or("GPT ENTRY BYTES OVERFLOW.")?;
    let scan_sectors = (scan_bytes + LOGICAL_SECTOR_SIZE - 1) / LOGICAL_SECTOR_SIZE;
    if scan_sectors == 0 {
        return Err("GPT ENTRY SCAN SIZE INVALID.");
    }

    let mut raw = Vec::new();
    raw.resize(scan_sectors * LOGICAL_SECTOR_SIZE, 0);

    let mut i = 0usize;
    while i < scan_sectors {
        let mut sec = [0u8; LOGICAL_SECTOR_SIZE];
        let lba512 = entry_lba_native
            .saturating_mul(lba_mul)
            .saturating_add(i as u64);
        if !read_sector_from_uefi_handle(handle, lba512, &mut sec) {
            return Err("GPT ENTRY ARRAY READ FAILED.");
        }
        let dst = i * LOGICAL_SECTOR_SIZE;
        raw[dst..dst + LOGICAL_SECTOR_SIZE].copy_from_slice(&sec);
        i += 1;
    }

    let mut out = Vec::new();
    let mut idx = 0usize;
    while idx < scan_entries {
        let off = idx * entry_size;
        let end = off + entry_size;
        if end > raw.len() {
            break;
        }
        let entry = &raw[off..end];

        // Zero type GUID means entry is unused.
        if entry[0..16].iter().all(|b| *b == 0) {
            idx += 1;
            continue;
        }

        let first_native = read_u64_le(entry, 32).ok_or("GPT FIRST LBA MISSING.")?;
        let last_native = read_u64_le(entry, 40).ok_or("GPT LAST LBA MISSING.")?;
        if first_native == 0 || last_native < first_native {
            idx += 1;
            continue;
        }
        if first_native < first_usable_lba_native || last_native > last_usable_lba_native {
            idx += 1;
            continue;
        }

        let total_native = last_native.saturating_sub(first_native).saturating_add(1);
        let first = first_native.saturating_mul(lba_mul);
        let total = total_native.saturating_mul(lba_mul);

        if first > u32::MAX as u64 || total > u32::MAX as u64 {
            idx += 1;
            continue;
        }
        if first.saturating_add(total) > total_logical_sectors {
            idx += 1;
            continue;
        }

        let mut part_type = 0xEE;
        if entry.len() >= 16 && entry[0..16] == GPT_EFI_SYSTEM_TYPE_GUID_LE {
            part_type = 0xEF;
        } else if entry.len() >= 16 && entry[0..16] == GPT_BASIC_DATA_TYPE_GUID_LE {
            part_type = 0x07;
        }

        out.push(MbrPartition {
            boot: 0,
            part_type,
            start_lba: first as u32,
            total_sectors: total as u32,
        });

        idx += 1;
    }

    out.sort_by(|a, b| a.start_lba.cmp(&b.start_lba));
    Ok(out)
}

fn encode_mbr_entry(entry: &MbrPartition) -> [u8; 16] {
    if !entry.is_used() {
        return [0u8; 16];
    }

    let mut out = [0u8; 16];
    out[0] = if entry.boot == 0x80 { 0x80 } else { 0x00 };
    out[1] = 0xFE;
    out[2] = 0xFF;
    out[3] = 0xFF;
    out[4] = entry.part_type;
    out[5] = 0xFE;
    out[6] = 0xFF;
    out[7] = 0xFF;
    out[8..12].copy_from_slice(&entry.start_lba.to_le_bytes());
    out[12..16].copy_from_slice(&entry.total_sectors.to_le_bytes());
    out
}

fn write_mbr_table(handle: Handle, partitions: &[MbrPartition; 4]) -> Result<(), &'static str> {
    let mut sector0 = [0u8; LOGICAL_SECTOR_SIZE];
    let _ = read_sector_from_uefi_handle(handle, 0, &mut sector0);

    let mut i = 0usize;
    while i < 4 {
        let off = 446 + i * 16;
        let raw = encode_mbr_entry(&partitions[i]);
        sector0[off..off + 16].copy_from_slice(&raw);
        i += 1;
    }

    sector0[510] = 0x55;
    sector0[511] = 0xAA;

    if !write_sector_to_uefi_handle(handle, 0, &sector0) {
        return Err("FAILED TO WRITE MBR TABLE.");
    }

    Ok(())
}

fn mbr_slots_from_disk(disk: &InternalDisk) -> Result<[MbrPartition; 4], &'static str> {
    if disk.scheme != PartitionScheme::Mbr {
        return Err("GPT PARTITION EDITING NOT SUPPORTED YET.");
    }
    if disk.partitions.len() != 4 {
        return Err("INVALID MBR TABLE STATE.");
    }

    let mut out = [MbrPartition::default(); 4];
    let mut i = 0usize;
    while i < 4 {
        out[i] = disk.partitions[i];
        i += 1;
    }
    Ok(out)
}

fn collect_targets(disks: &[InternalDisk]) -> Vec<TargetRef> {
    let mut out = Vec::new();

    let mut d = 0usize;
    while d < disks.len() {
        let disk = &disks[d];
        let disk_limit = core::cmp::min(disk.total_logical_sectors, u32::MAX as u64) as u32;

        let mut p = 0usize;
        while p < disk.partitions.len() {
            let part = disk.partitions[p];
            if part.is_used()
                && is_visible_partition_for_selection(disk.scheme, part.part_type)
                && part.start_lba >= 1
                && part.total_sectors >= MIN_INSTALL_SECTORS
                && part.end_exclusive() <= disk_limit
            {
                out.push(TargetRef { disk_idx: d, part_idx: p });
            }
            p += 1;
        }

        d += 1;
    }

    out.sort_by(|a, b| {
        if a.disk_idx != b.disk_idx {
            return a.disk_idx.cmp(&b.disk_idx);
        }

        let pa = disks[a.disk_idx].partitions[a.part_idx].start_lba;
        let pb = disks[b.disk_idx].partitions[b.part_idx].start_lba;
        pa.cmp(&pb)
    });

    out
}

fn is_visible_partition_for_selection(scheme: PartitionScheme, part_type: u8) -> bool {
    if part_type == 0 {
        return false;
    }
    if scheme == PartitionScheme::Mbr && part_type == 0xEE {
        return false;
    }
    true
}

fn is_install_target_partition_type(part_type: u8) -> bool {
    part_type == 0x0B || part_type == 0x0C || part_type == 0xEF
}

fn partition_role_label(part_type: u8) -> &'static str {
    if is_install_target_partition_type(part_type) {
        "BOOT"
    } else if part_type == 0x07 {
        "DATA"
    } else {
        "OTHER"
    }
}

fn paired_data_partition_after_boot(disk: &InternalDisk, boot_part_idx: usize) -> Option<MbrPartition> {
    let boot_part = *disk.partitions.get(boot_part_idx)?;
    if !boot_part.is_used() || !is_install_target_partition_type(boot_part.part_type) {
        return None;
    }

    let expected_start = boot_part.end_exclusive();
    disk.partitions
        .iter()
        .copied()
        .find(|part| part.is_used() && part.part_type == 0x07 && part.start_lba == expected_start)
}

fn refresh_selection(selected: &mut usize, len: usize) {
    if len == 0 {
        *selected = 0;
    } else if *selected >= len {
        *selected = len - 1;
    }
}

fn current_disk_index(targets: &[TargetRef], selected: usize, disks: &[InternalDisk]) -> usize {
    if targets.is_empty() {
        return 0;
    }
    if selected < targets.len() {
        return targets[selected].disk_idx;
    }
    core::cmp::min(targets[0].disk_idx, disks.len().saturating_sub(1))
}

fn find_target_by_start_lba(
    targets: &[TargetRef],
    disks: &[InternalDisk],
    disk_idx: usize,
    start_lba: u64,
) -> Option<usize> {
    let mut i = 0usize;
    while i < targets.len() {
        let target = targets[i];
        if target.disk_idx == disk_idx {
            let part = disks[target.disk_idx].partitions[target.part_idx];
            if part.start_lba as u64 == start_lba {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn resize_selected_partition(
    disks: &mut [InternalDisk],
    targets: &[TargetRef],
    selected: usize,
    grow: bool,
) -> Result<String, &'static str> {
    if targets.is_empty() {
        return Err("NO TARGET SELECTED.");
    }
    if selected >= targets.len() {
        return Err("INVALID TARGET SELECTION.");
    }

    let t = targets[selected];
    let mut disk = disks[t.disk_idx].clone();
    if disk.scheme != PartitionScheme::Mbr {
        return Err("GPT RESIZE NOT IMPLEMENTED YET. USE EXISTING GPT PARTITIONS.");
    }
    let mut part = disk.partitions[t.part_idx];

    if !part.is_used() {
        return Err("TARGET PARTITION IS EMPTY.");
    }

    if grow {
        let disk_limit = core::cmp::min(disk.total_logical_sectors, u32::MAX as u64) as u32;
        let mut next_start = disk_limit;

        let mut i = 0usize;
        while i < disk.partitions.len() {
            if i != t.part_idx {
                let p = disk.partitions[i];
                if p.is_used() && p.start_lba > part.start_lba && p.start_lba < next_start {
                    next_start = p.start_lba;
                }
            }
            i += 1;
        }

        let current_end = part.end_exclusive();
        if current_end >= next_start {
            return Err("NO FREE SPACE AFTER PARTITION TO GROW.");
        }

        let available = next_start - current_end;
        let inc = core::cmp::min(RESIZE_STEP_SECTORS, available);
        if inc == 0 {
            return Err("NO SPACE AVAILABLE FOR GROW.");
        }

        part.total_sectors = part.total_sectors.saturating_add(inc);
    } else {
        if part.total_sectors <= MIN_INSTALL_SECTORS {
            return Err("PARTITION ALREADY AT MIN SIZE (64 MIB).");
        }

        let max_dec = part.total_sectors - MIN_INSTALL_SECTORS;
        let dec = core::cmp::min(RESIZE_STEP_SECTORS, max_dec);
        if dec == 0 {
            return Err("PARTITION CANNOT SHRINK FURTHER.");
        }

        part.total_sectors = part.total_sectors.saturating_sub(dec);
    }

    disk.partitions[t.part_idx] = part;
    let mbr = mbr_slots_from_disk(&disk)?;
    write_mbr_table(disk.handle, &mbr)?;
    disks[t.disk_idx] = disk;

    let new_mib = part.total_sectors as u64 / 2048;
    let msg = if grow {
        format!(
            "RESIZE OK: DISK {} PART {} GREW TO {} MIB (+{} MIB).",
            t.disk_idx + 1,
            t.part_idx + 1,
            new_mib,
            RESIZE_STEP_MIB
        )
    } else {
        format!(
            "RESIZE OK: DISK {} PART {} SHRANK TO {} MIB (-{} MIB).",
            t.disk_idx + 1,
            t.part_idx + 1,
            new_mib,
            RESIZE_STEP_MIB
        )
    };

    Ok(msg)
}

fn create_partition_on_disk(disks: &mut [InternalDisk], disk_idx: usize) -> Result<CreatePartitionResult, &'static str> {
    if disk_idx >= disks.len() {
        return Err("INVALID DISK INDEX.");
    }

    let mut disk = disks[disk_idx].clone();
    if disk.scheme == PartitionScheme::Gpt {
        return create_dual_gpt_partitions_on_disk(disks, disk_idx);
    }

    let mut empty_slots = Vec::new();
    let mut i = 0usize;
    while i < disk.partitions.len() {
        if !disk.partitions[i].is_used() {
            empty_slots.push(i);
        }
        i += 1;
    }

    if empty_slots.len() < 2 {
        return Err("NEED TWO FREE MBR SLOTS FOR 16 GIB FAT32 BOOT + EXFAT DATA.");
    }
    let boot_slot = empty_slots[0];
    let data_slot = empty_slots[1];

    let (gap_start, gap_end) = find_largest_free_gap(&disk)?;
    let aligned_start = align_up_u32(gap_start, 2048);
    if aligned_start >= gap_end {
        return Err("FREE GAP TOO SMALL AFTER ALIGNMENT.");
    }

    let size = gap_end - aligned_start;
    if size < MIN_INSTALL_SECTORS {
        return Err("LARGEST FREE SPACE IS SMALLER THAN 64 MIB.");
    }
    let (boot_sectors, data_sectors) = choose_dual_partition_sectors(size)?;
    let data_start = aligned_start
        .checked_add(boot_sectors)
        .ok_or("DUAL PARTITION START OVERFLOW.")?;

    disk.partitions[boot_slot] = MbrPartition {
        boot: 0,
        part_type: 0x0C,
        start_lba: aligned_start,
        total_sectors: boot_sectors,
    };
    disk.partitions[data_slot] = MbrPartition {
        boot: 0,
        part_type: 0x07,
        start_lba: data_start,
        total_sectors: data_sectors,
    };

    let mbr = mbr_slots_from_disk(&disk)?;
    write_mbr_table(disk.handle, &mbr)?;
    format_exfat_partition(disk.handle, data_start as u64, data_sectors as u64, "ZENOX DATA")?;
    disks[disk_idx] = disk;

    Ok(CreatePartitionResult {
        message: format!(
            "CREATE OK: DISK {} PART {} FAT32 BOOT ({} MIB) + PART {} EXFAT DATA ({} MIB).",
            disk_idx + 1,
            boot_slot + 1,
            boot_sectors as u64 / 2048,
            data_slot + 1,
            data_sectors as u64 / 2048
        ),
        boot_start_lba: aligned_start as u64,
    })
}

fn create_dual_partitions_from_selected(
    disks: &mut [InternalDisk],
    target: TargetRef,
) -> Result<CreatePartitionResult, &'static str> {
    if target.disk_idx >= disks.len() {
        return Err("INVALID DISK INDEX.");
    }
    if target.part_idx >= disks[target.disk_idx].partitions.len() {
        return Err("INVALID PARTITION INDEX.");
    }

    if disks[target.disk_idx].scheme == PartitionScheme::Gpt {
        create_dual_gpt_partitions_from_selected(disks, target)
    } else {
        create_dual_mbr_partitions_from_selected(disks, target)
    }
}

fn create_dual_mbr_partitions_from_selected(
    disks: &mut [InternalDisk],
    target: TargetRef,
) -> Result<CreatePartitionResult, &'static str> {
    let mut disk = disks[target.disk_idx].clone();
    if target.part_idx >= disk.partitions.len() {
        return Err("INVALID MBR PARTITION INDEX.");
    }

    let selected = disk.partitions[target.part_idx];
    if !selected.is_used() {
        return Err("SELECTED PARTITION IS EMPTY.");
    }
    if selected.total_sectors < MIN_INSTALL_SECTORS {
        return Err("SELECTED PARTITION IS TOO SMALL.");
    }

    let mut data_slot = None;
    let mut i = 0usize;
    while i < disk.partitions.len() {
        if i != target.part_idx && !disk.partitions[i].is_used() {
            data_slot = Some(i);
            break;
        }
        i += 1;
    }
    let data_slot = data_slot.ok_or("NEED ONE FREE MBR SLOT TO SPLIT SELECTED PARTITION.")?;

    let (boot_sectors, data_sectors) = choose_dual_partition_sectors(selected.total_sectors)?;
    let data_start = selected
        .start_lba
        .checked_add(boot_sectors)
        .ok_or("SELECTED PARTITION SPLIT OVERFLOW.")?;

    disk.partitions[target.part_idx] = MbrPartition {
        boot: 0,
        part_type: 0x0C,
        start_lba: selected.start_lba,
        total_sectors: boot_sectors,
    };
    disk.partitions[data_slot] = MbrPartition {
        boot: 0,
        part_type: 0x07,
        start_lba: data_start,
        total_sectors: data_sectors,
    };

    let mbr = mbr_slots_from_disk(&disk)?;
    write_mbr_table(disk.handle, &mbr)?;
    format_exfat_partition(disk.handle, data_start as u64, data_sectors as u64, "ZENOX DATA")?;
    disks[target.disk_idx] = disk;

    Ok(CreatePartitionResult {
        message: format!(
            "SPLIT OK: ONLY DISK {} PART {} WAS ERASED. FAT32 BOOT {} MIB + EXFAT DATA {} MIB.",
            target.disk_idx + 1,
            target.part_idx + 1,
            boot_sectors as u64 / 2048,
            data_sectors as u64 / 2048
        ),
        boot_start_lba: selected.start_lba as u64,
    })
}

fn create_dual_gpt_partitions_from_selected(
    disks: &mut [InternalDisk],
    target: TargetRef,
) -> Result<CreatePartitionResult, &'static str> {
    let disk = disks[target.disk_idx].clone();
    if target.part_idx >= disk.partitions.len() {
        return Err("INVALID GPT PARTITION INDEX.");
    }

    let selected = disk.partitions[target.part_idx];
    if !selected.is_used() {
        return Err("SELECTED GPT PARTITION IS EMPTY.");
    }

    let mut meta = load_gpt_meta(&disk)?;
    let selected_start_native = sectors512_to_native_exact(selected.start_lba, meta.lba_mul)?;
    let selected_total_native = sectors512_to_native_exact(selected.total_sectors, meta.lba_mul)?;
    let selected_last_native = selected_start_native
        .checked_add(selected_total_native.saturating_sub(1))
        .ok_or("SELECTED GPT PARTITION OVERFLOW.")?;

    let mut selected_entry_idx = None;
    let mut empty_entry_idx = None;
    let mut i = 0usize;
    while i < meta.entry_count {
        let off = i
            .checked_mul(meta.entry_size)
            .ok_or("GPT ENTRY OFFSET OVERFLOW.")?;
        let end = off
            .checked_add(meta.entry_size)
            .ok_or("GPT ENTRY END OVERFLOW.")?;
        if end > meta.entries.len() {
            break;
        }
        let entry = &meta.entries[off..end];
        if !gpt_entry_is_used(entry) {
            if empty_entry_idx.is_none() {
                empty_entry_idx = Some(i);
            }
            i += 1;
            continue;
        }

        let first = read_u64_le(entry, 32).ok_or("GPT ENTRY FIRST LBA MISSING.")?;
        let last = read_u64_le(entry, 40).ok_or("GPT ENTRY LAST LBA MISSING.")?;
        if first == selected_start_native && last == selected_last_native {
            selected_entry_idx = Some(i);
        }
        i += 1;
    }

    let selected_entry_idx =
        selected_entry_idx.ok_or("SELECTED GPT PARTITION ENTRY NOT FOUND.")?;
    let empty_entry_idx =
        empty_entry_idx.ok_or("NEED ONE FREE GPT ENTRY TO SPLIT SELECTED PARTITION.")?;

    let (boot_512, data_512) = choose_dual_partition_sectors(selected.total_sectors)?;
    let boot_native = sectors512_to_native_exact(boot_512, meta.lba_mul)?;
    let data_native = sectors512_to_native_exact(data_512, meta.lba_mul)?;
    let data_start_native = selected_start_native
        .checked_add(boot_native)
        .ok_or("GPT SELECTED DATA START OVERFLOW.")?;
    if data_start_native
        .checked_add(data_native)
        .ok_or("GPT SELECTED DATA END OVERFLOW.")?
        .saturating_sub(1)
        > selected_last_native
    {
        return Err("GPT SELECTED SPLIT DOES NOT FIT.");
    }

    gpt_write_entry_typed(
        &mut meta,
        selected_entry_idx,
        selected_start_native,
        boot_native,
        GPT_EFI_SYSTEM_TYPE_GUID_LE,
        "ZENOX OS",
    )?;
    gpt_write_entry_typed(
        &mut meta,
        empty_entry_idx,
        data_start_native,
        data_native,
        GPT_BASIC_DATA_TYPE_GUID_LE,
        "ZENOX DATA",
    )?;
    gpt_write_tables(disk.handle, &mut meta)?;

    let data_start_512 = data_start_native
        .checked_mul(meta.lba_mul)
        .ok_or("GPT DATA START CONVERSION OVERFLOW.")?;
    let data_total_512 = data_native
        .checked_mul(meta.lba_mul)
        .ok_or("GPT DATA SIZE CONVERSION OVERFLOW.")?;
    format_exfat_partition(disk.handle, data_start_512, data_total_512, "ZENOX DATA")?;

    let refreshed = parse_gpt_partitions(disk.handle, disk.block_size, disk.total_logical_sectors)
        .unwrap_or_else(|_| {
            let mut fallback = disk.partitions.clone();
            fallback[target.part_idx] = MbrPartition {
                boot: 0,
                part_type: 0xEF,
                start_lba: selected.start_lba,
                total_sectors: boot_512,
            };
            fallback.push(MbrPartition {
                boot: 0,
                part_type: 0x07,
                start_lba: data_start_512 as u32,
                total_sectors: data_512,
            });
            fallback.sort_by(|a, b| a.start_lba.cmp(&b.start_lba));
            fallback
        });

    disks[target.disk_idx] = InternalDisk {
        handle: disk.handle,
        block_size: disk.block_size,
        total_logical_sectors: disk.total_logical_sectors,
        scheme: PartitionScheme::Gpt,
        partitions: refreshed,
    };

    Ok(CreatePartitionResult {
        message: format!(
            "SPLIT OK: ONLY GPT TARGET {} WAS ERASED. FAT32 BOOT {} MIB + EXFAT DATA {} MIB.",
            target.part_idx + 1,
            boot_512 as u64 / 2048,
            data_512 as u64 / 2048
        ),
        boot_start_lba: selected.start_lba as u64,
    })
}

fn create_dual_partitions_on_whole_disk(
    disks: &mut [InternalDisk],
    disk_idx: usize,
) -> Result<CreatePartitionResult, &'static str> {
    if disk_idx >= disks.len() {
        return Err("INVALID DISK INDEX.");
    }
    if disks[disk_idx].scheme == PartitionScheme::Gpt {
        create_dual_gpt_partitions_on_whole_disk(disks, disk_idx)
    } else {
        create_dual_mbr_partitions_on_whole_disk(disks, disk_idx)
    }
}

fn create_dual_mbr_partitions_on_whole_disk(
    disks: &mut [InternalDisk],
    disk_idx: usize,
) -> Result<CreatePartitionResult, &'static str> {
    let mut disk = disks[disk_idx].clone();
    let disk_limit = core::cmp::min(disk.total_logical_sectors, u32::MAX as u64) as u32;
    let aligned_start = 2048u32;
    if disk_limit <= aligned_start {
        return Err("DISK TOO SMALL FOR 8 GIB FAT32 BOOT + EXFAT DATA.");
    }

    let total = disk_limit.saturating_sub(aligned_start);
    let (boot_sectors, data_sectors) = choose_dual_partition_sectors(total)?;
    let data_start = aligned_start
        .checked_add(boot_sectors)
        .ok_or("DUAL PARTITION START OVERFLOW.")?;

    let mut partitions = [MbrPartition::default(); 4];
    partitions[0] = MbrPartition {
        boot: 0,
        part_type: 0x0C,
        start_lba: aligned_start,
        total_sectors: boot_sectors,
    };
    partitions[1] = MbrPartition {
        boot: 0,
        part_type: 0x07,
        start_lba: data_start,
        total_sectors: data_sectors,
    };

    write_mbr_table(disk.handle, &partitions)?;
    format_exfat_partition(disk.handle, data_start as u64, data_sectors as u64, "ZENOX DATA")?;

    let mut refreshed = Vec::new();
    refreshed.extend_from_slice(&partitions);
    disk.partitions = refreshed;
    disks[disk_idx] = disk;

    Ok(CreatePartitionResult {
        message: format!(
            "FORMAT OK: DISK {} ERASED. FAT32 BOOT {} MIB + EXFAT DATA {} MIB.",
            disk_idx + 1,
            boot_sectors as u64 / 2048,
            data_sectors as u64 / 2048
        ),
        boot_start_lba: aligned_start as u64,
    })
}

fn create_dual_gpt_partitions_on_whole_disk(
    disks: &mut [InternalDisk],
    disk_idx: usize,
) -> Result<CreatePartitionResult, &'static str> {
    let disk = disks[disk_idx].clone();
    let mut meta = load_gpt_meta(&disk)?;
    if meta.entry_count < 2 {
        return Err("GPT NEEDS TWO ENTRIES FOR 16 GIB FAT32 BOOT + EXFAT DATA.");
    }

    for b in meta.entries.iter_mut() {
        *b = 0;
    }

    let align_native = core::cmp::max(1, 2048u64 / meta.lba_mul.max(1));
    let boot_start_native = align_up_u64(meta.first_usable_lba_native, align_native);
    let disk_end_native = meta.last_usable_lba_native.saturating_add(1);
    if disk_end_native <= boot_start_native {
        return Err("GPT DISK TOO SMALL AFTER ALIGNMENT.");
    }

    let total_512 = disk_end_native
        .saturating_sub(boot_start_native)
        .checked_mul(meta.lba_mul)
        .ok_or("GPT DISK SIZE OVERFLOW.")?;
    if total_512 > u32::MAX as u64 {
        return Err("GPT DISK TOO LARGE FOR PREBOOT DUAL CREATOR.");
    }

    let (boot_512, data_512) = choose_dual_partition_sectors(total_512 as u32)?;
    let boot_native = sectors512_to_native_exact(boot_512, meta.lba_mul)?;
    let data_native = sectors512_to_native_exact(data_512, meta.lba_mul)?;
    let data_start_native = boot_start_native
        .checked_add(boot_native)
        .ok_or("GPT DATA START OVERFLOW.")?;
    if data_start_native
        .checked_add(data_native)
        .ok_or("GPT DATA END OVERFLOW.")?
        > disk_end_native
    {
        return Err("GPT DUAL LAYOUT DOES NOT FIT.");
    }

    gpt_write_entry_typed(
        &mut meta,
        0,
        boot_start_native,
        boot_native,
        GPT_EFI_SYSTEM_TYPE_GUID_LE,
        "ZENOX OS",
    )?;
    gpt_write_entry_typed(
        &mut meta,
        1,
        data_start_native,
        data_native,
        GPT_BASIC_DATA_TYPE_GUID_LE,
        "ZENOX DATA",
    )?;
    gpt_write_tables(disk.handle, &mut meta)?;

    let boot_start_512 = boot_start_native
        .checked_mul(meta.lba_mul)
        .ok_or("GPT BOOT START CONVERSION OVERFLOW.")?;
    let data_start_512 = data_start_native
        .checked_mul(meta.lba_mul)
        .ok_or("GPT DATA START CONVERSION OVERFLOW.")?;
    let data_total_512 = data_native
        .checked_mul(meta.lba_mul)
        .ok_or("GPT DATA SIZE CONVERSION OVERFLOW.")?;
    format_exfat_partition(disk.handle, data_start_512, data_total_512, "ZENOX DATA")?;

    let refreshed = parse_gpt_partitions(disk.handle, disk.block_size, disk.total_logical_sectors)
        .unwrap_or_else(|_| {
            let mut fallback = Vec::new();
            fallback.push(MbrPartition {
                boot: 0,
                part_type: 0xEF,
                start_lba: boot_start_512 as u32,
                total_sectors: boot_512,
            });
            fallback.push(MbrPartition {
                boot: 0,
                part_type: 0x07,
                start_lba: data_start_512 as u32,
                total_sectors: data_512,
            });
            fallback
        });

    disks[disk_idx] = InternalDisk {
        handle: disk.handle,
        block_size: disk.block_size,
        total_logical_sectors: disk.total_logical_sectors,
        scheme: PartitionScheme::Gpt,
        partitions: refreshed,
    };

    Ok(CreatePartitionResult {
        message: format!(
            "FORMAT OK: GPT DISK {} ERASED. FAT32 BOOT {} MIB + EXFAT DATA {} MIB.",
            disk_idx + 1,
            boot_512 as u64 / 2048,
            data_512 as u64 / 2048
        ),
        boot_start_lba: boot_start_512,
    })
}

fn choose_dual_partition_sectors(total: u32) -> Result<(u32, u32), &'static str> {
    let min_total = DUAL_MIN_BOOT_SECTORS
        .checked_add(DUAL_MIN_DATA_SECTORS)
        .ok_or("DUAL SIZE OVERFLOW.")?;
    if total < min_total {
        return Err("FREE SPACE TOO SMALL FOR 8 GIB FAT32 BOOT + EXFAT DATA.");
    }

    let mut boot = if total >= DUAL_BOOT_TARGET_SECTORS.saturating_add(DUAL_MIN_DATA_SECTORS) {
        DUAL_BOOT_TARGET_SECTORS
    } else {
        total.saturating_sub(DUAL_MIN_DATA_SECTORS)
    };
    boot = round_down_to_multiple_u32(boot, 2048);
    if boot < DUAL_MIN_BOOT_SECTORS {
        boot = DUAL_MIN_BOOT_SECTORS;
    }
    if total.saturating_sub(boot) < DUAL_MIN_DATA_SECTORS {
        boot = total.saturating_sub(DUAL_MIN_DATA_SECTORS);
        boot = round_down_to_multiple_u32(boot, 2048);
    }
    let data = total.saturating_sub(boot);
    if boot < DUAL_MIN_BOOT_SECTORS || data < DUAL_MIN_DATA_SECTORS {
        return Err("FREE SPACE TOO SMALL FOR 8 GIB FAT32 BOOT + EXFAT DATA.");
    }
    Ok((boot, data))
}

fn align_up_u64(value: u64, align: u64) -> u64 {
    if align <= 1 {
        return value;
    }
    let rem = value % align;
    if rem == 0 {
        value
    } else {
        value.saturating_add(align - rem)
    }
}

fn create_dual_gpt_partitions_on_disk(disks: &mut [InternalDisk], disk_idx: usize) -> Result<CreatePartitionResult, &'static str> {
    let disk = disks[disk_idx].clone();
    let mut meta = load_gpt_meta(&disk)?;

    let mut empty = Vec::new();
    let mut idx = 0usize;
    while idx < meta.entry_count {
        let off = idx * meta.entry_size;
        let end = off + meta.entry_size;
        if end > meta.entries.len() {
            break;
        }
        if !gpt_entry_is_used(&meta.entries[off..end]) {
            empty.push(idx);
            if empty.len() >= 2 {
                break;
            }
        }
        idx += 1;
    }
    if empty.len() < 2 {
        return Err("NEED TWO FREE GPT ENTRIES FOR 16 GIB FAT32 BOOT + EXFAT DATA.");
    }

    let align_native = core::cmp::max(1, 2048u64 / meta.lba_mul.max(1));
    let used = gpt_used_entries_sorted(&meta);
    let mut best_start = 0u64;
    let mut best_end = 0u64;
    let mut cursor = align_up_u64(meta.first_usable_lba_native, align_native);

    for (_entry_idx, first) in used.iter().copied() {
        if first > cursor {
            let gap_start = cursor;
            let gap_end = first;
            if gap_end.saturating_sub(gap_start) > best_end.saturating_sub(best_start) {
                best_start = gap_start;
                best_end = gap_end;
            }
        }
        let entry = gpt_entry_mut(&mut meta, _entry_idx)?;
        let last = read_u64_le(entry, 40).ok_or("GPT ENTRY LAST LBA MISSING.")?;
        if last.saturating_add(1) > cursor {
            cursor = align_up_u64(last.saturating_add(1), align_native);
        }
    }

    let tail_end = meta.last_usable_lba_native.saturating_add(1);
    if tail_end > cursor
        && tail_end.saturating_sub(cursor) > best_end.saturating_sub(best_start)
    {
        best_start = cursor;
        best_end = tail_end;
    }

    if best_end <= best_start {
        return Err("NO GPT FREE SPACE AVAILABLE.");
    }

    let gap_native = best_end.saturating_sub(best_start);
    let gap_512 = gap_native
        .checked_mul(meta.lba_mul)
        .ok_or("GPT GAP SIZE OVERFLOW.")?;
    if gap_512 > u32::MAX as u64 {
        return Err("GPT FREE GAP TOO LARGE FOR PREBOOT DUAL CREATOR.");
    }
    let (boot_512, data_512) = choose_dual_partition_sectors(gap_512 as u32)?;
    let boot_native = sectors512_to_native_exact(boot_512, meta.lba_mul)?;
    let data_native = sectors512_to_native_exact(data_512, meta.lba_mul)?;
    let boot_start_512 = best_start
        .checked_mul(meta.lba_mul)
        .ok_or("GPT BOOT START CONVERSION OVERFLOW.")?;
    let data_start_native = best_start
        .checked_add(boot_native)
        .ok_or("GPT DATA START OVERFLOW.")?;
    if data_start_native
        .checked_add(data_native)
        .ok_or("GPT DATA END OVERFLOW.")?
        > best_end
    {
        return Err("GPT DUAL LAYOUT DOES NOT FIT.");
    }

    gpt_write_entry_typed(
        &mut meta,
        empty[0],
        best_start,
        boot_native,
        GPT_EFI_SYSTEM_TYPE_GUID_LE,
        "ZENOX OS",
    )?;
    gpt_write_entry_typed(
        &mut meta,
        empty[1],
        data_start_native,
        data_native,
        GPT_BASIC_DATA_TYPE_GUID_LE,
        "ZENOX DATA",
    )?;
    gpt_write_tables(disk.handle, &mut meta)?;

    let data_start_512 = data_start_native
        .checked_mul(meta.lba_mul)
        .ok_or("GPT DATA START CONVERSION OVERFLOW.")?;
    let data_total_512 = data_native
        .checked_mul(meta.lba_mul)
        .ok_or("GPT DATA SIZE CONVERSION OVERFLOW.")?;
    format_exfat_partition(disk.handle, data_start_512, data_total_512, "ZENOX DATA")?;

    let refreshed = parse_gpt_partitions(disk.handle, disk.block_size, disk.total_logical_sectors)
        .unwrap_or_else(|_| {
            let mut fallback = disk.partitions.clone();
            fallback.push(MbrPartition {
                boot: 0,
                part_type: 0xEF,
                start_lba: (best_start * meta.lba_mul) as u32,
                total_sectors: boot_512,
            });
            fallback.push(MbrPartition {
                boot: 0,
                part_type: 0x07,
                start_lba: data_start_512 as u32,
                total_sectors: data_512,
            });
            fallback
        });

    disks[disk_idx] = InternalDisk {
        handle: disk.handle,
        block_size: disk.block_size,
        total_logical_sectors: disk.total_logical_sectors,
        scheme: PartitionScheme::Gpt,
        partitions: refreshed,
    };

    Ok(CreatePartitionResult {
        message: format!(
            "CREATE OK: GPT DISK {} FAT32 BOOT {} MIB + EXFAT DATA {} MIB.",
            disk_idx + 1,
            boot_512 as u64 / 2048,
            data_512 as u64 / 2048
        ),
        boot_start_lba: boot_start_512,
    })
}

fn find_largest_free_gap(disk: &InternalDisk) -> Result<(u32, u32), &'static str> {
    let limit = core::cmp::min(disk.total_logical_sectors, u32::MAX as u64) as u32;
    if limit <= 4096 {
        return Err("DISK TOO SMALL.");
    }

    let mut used = Vec::<(u32, u32)>::new();
    let mut i = 0usize;
    while i < disk.partitions.len() {
        let p = disk.partitions[i];
        if p.is_used() {
            let start = p.start_lba;
            let end = p.end_exclusive();
            if start < limit && end > start {
                used.push((start, core::cmp::min(end, limit)));
            }
        }
        i += 1;
    }

    used.sort_by(|a, b| a.0.cmp(&b.0));

    let mut best_start = 0u32;
    let mut best_end = 0u32;

    let mut cursor = 2048u32; // 1 MiB alignment baseline.
    for (start, end) in used {
        if start > cursor {
            let gap = start - cursor;
            if gap > best_end.saturating_sub(best_start) {
                best_start = cursor;
                best_end = start;
            }
        }
        if end > cursor {
            cursor = end;
        }
    }

    if limit > cursor {
        let gap = limit - cursor;
        if gap > best_end.saturating_sub(best_start) {
            best_start = cursor;
            best_end = limit;
        }
    }

    if best_end <= best_start {
        return Err("NO FREE SPACE AVAILABLE.");
    }

    Ok((best_start, best_end))
}

fn align_up_u32(value: u32, align: u32) -> u32 {
    if align <= 1 {
        return value;
    }
    let rem = value % align;
    if rem == 0 {
        value
    } else {
        value.saturating_add(align - rem)
    }
}

fn load_bootx64_payload() -> Result<Vec<u8>, &'static str> {
    draw_bootstrap_progress("LOADING PAYLOAD", 10, "OPENING LOADEDIMAGE PROTOCOL");
    crate::println("Preboot installer: open LoadedImage protocol");

    let params = OpenProtocolParams {
        handle: boot::image_handle(),
        agent: boot::image_handle(),
        controller: None,
    };

    let loaded = unsafe {
        boot::open_protocol::<LoadedImage>(params, OpenProtocolAttributes::GetProtocol)
    }
    .map_err(|_| "LOADED IMAGE PROTOCOL NOT AVAILABLE.")?;

    let (base, size64) = loaded.info();
    if base.is_null() || size64 == 0 {
        return Err("LOADED IMAGE BASE/SIZE INVALID.");
    }

    let size = usize::try_from(size64).map_err(|_| "LOADED IMAGE SIZE CONVERSION FAILED.")?;
    draw_bootstrap_progress("LOADING PAYLOAD", 14, "INSPECTING EFI IMAGE");
    crate::println("Preboot installer: loaded image bytes");
    crate::println_num(size as u64);

    let inspect_len = core::cmp::min(size, 256 * 1024 * 1024);
    let image = unsafe { core::slice::from_raw_parts(base as *const u8, inspect_len) };
    draw_bootstrap_progress("LOADING PAYLOAD", 18, "RECONSTRUCTING PE PAYLOAD");
    match reconstruct_pe_payload(image) {
        Ok(payload) => {
            draw_bootstrap_progress("LOADING PAYLOAD", 40, "RECONSTRUCTION OK");
            crate::println("Preboot installer: payload reconstructed bytes");
            crate::println_num(payload.len() as u64);
            return Ok(payload);
        }
        Err(err) => {
            crate::println("Preboot installer: payload reconstruct error");
            crate::println(err);
            crate::println("Preboot installer: payload reconstruct failed; fs fallback");
        }
    }

    draw_bootstrap_progress("LOADING PAYLOAD", 30, "FALLBACK: READING FROM BOOT FS");
    load_payload_from_boot_fs()
}

fn load_payload_from_boot_fs() -> Result<Vec<u8>, &'static str> {
    let parent_image = boot::image_handle();
    let fs_proto = boot::get_image_file_system(parent_image)
        .map_err(|_| "BOOT FILESYSTEM NOT AVAILABLE FOR PAYLOAD READ.")?;
    let mut fs = UefiFileSystem::new(fs_proto);

    let candidates = [
        (uefi::cstr16!("\\EFI\\BOOT\\BOOTX64.EFI"), "\\EFI\\BOOT\\BOOTX64.EFI"),
        (uefi::cstr16!("\\BOOTX64.EFI"), "\\BOOTX64.EFI"),
        (
            uefi::cstr16!("\\EFI\\REDUX\\BOOTX64.EFI"),
            "\\EFI\\REDUX\\BOOTX64.EFI",
        ),
    ];

    for (idx, (path, label)) in candidates.iter().enumerate() {
        let ui_pct = core::cmp::min(39u8, 31u8.saturating_add((idx as u8).saturating_mul(3)));
        draw_bootstrap_progress("LOADING PAYLOAD", ui_pct, "FALLBACK: READING BOOT FS PATH");
        crate::println("Preboot installer: payload fallback trying");
        crate::println(label);

        if let Ok(bytes) = fs.read(*path) {
            draw_bootstrap_progress(
                "LOADING PAYLOAD",
                core::cmp::min(39u8, ui_pct.saturating_add(1)),
                "FALLBACK: VALIDATING IMAGE",
            );
            if bytes.len() >= 4096
                && bytes.len() <= PAYLOAD_MAX_BYTES
                && bytes.len() >= 2
                && bytes[0] == b'M'
                && bytes[1] == b'Z'
            {
                crate::println("Preboot installer: payload from fs bytes");
                crate::println_num(bytes.len() as u64);
                return Ok(bytes);
            }
            crate::println("Preboot installer: payload fallback candidate invalid");
        } else {
            crate::println("Preboot installer: payload fallback read failed");
        }
    }

    Err("PAYLOAD READ FAILED (LOADED IMAGE + FS FALLBACK).")
}

fn runtime_bucket_dir_name(bucket: RuntimeBucket) -> &'static str {
    match bucket {
        RuntimeBucket::Lib => "LIB",
        RuntimeBucket::Lib64 => "LIB64",
        RuntimeBucket::UsrLib => "USR\\LIB",
        RuntimeBucket::UsrLib64 => "USR\\LIB64",
        RuntimeBucket::Bin => "BIN",
        RuntimeBucket::Etc => "ETC",
        RuntimeBucket::UsrBin => "USR\\BIN",
    }
}

fn runtime_bucket_from_source_path(source_path: &str) -> RuntimeBucket {
    let mut normalized = String::with_capacity(source_path.len());
    for b in source_path.bytes() {
        if b == b'\\' {
            normalized.push('/');
        } else if b.is_ascii_uppercase() {
            normalized.push((b + 32) as char);
        } else {
            normalized.push(b as char);
        }
    }

    if normalized.starts_with("usr/lib64/") || normalized.contains("/usr/lib64/") {
        RuntimeBucket::UsrLib64
    } else if normalized.starts_with("usr/lib/") || normalized.contains("/usr/lib/") {
        RuntimeBucket::UsrLib
    } else if normalized.starts_with("usr/bin/") || normalized.contains("/usr/bin/") {
        RuntimeBucket::UsrBin
    } else if normalized.starts_with("lib64/") || normalized.contains("/lib64/") {
        RuntimeBucket::Lib64
    } else if normalized.starts_with("bin/") || normalized.contains("/bin/") {
        RuntimeBucket::Bin
    } else if normalized.starts_with("etc/") || normalized.contains("/etc/") {
        RuntimeBucket::Etc
    } else {
        RuntimeBucket::Lib
    }
}

fn normalize_runtime_source_path(source_path: &str) -> String {
    let mut normalized = String::with_capacity(source_path.len());
    for b in source_path.bytes() {
        if b == b'\\' {
            normalized.push('/');
        } else if b.is_ascii_uppercase() {
            normalized.push((b + 32) as char);
        } else {
            normalized.push(b as char);
        }
    }
    while normalized.starts_with('/') {
        normalized.remove(0);
    }
    normalized
}

fn runtime_source_leaf(source_path: &str) -> &str {
    let mut start = 0usize;
    for (idx, b) in source_path.bytes().enumerate() {
        if b == b'/' || b == b'\\' {
            start = idx + 1;
        }
    }
    &source_path[start..]
}

fn try_read_runtime_file_from_boot_fs(short_name_text: &str, bucket: RuntimeBucket) -> Option<Vec<u8>> {
    let parent_image = boot::image_handle();
    let fs_proto = boot::get_image_file_system(parent_image).ok()?;
    let mut fs = UefiFileSystem::new(fs_proto);
    let path = format!(
        "\\LINUXRT\\{}\\{}",
        runtime_bucket_dir_name(bucket),
        short_name_text
    );
    let bytes = read_boot_fs_path(&mut fs, path.as_str()).ok()?;
    if bytes.is_empty() || bytes.len() > u32::MAX as usize {
        return None;
    }
    Some(bytes)
}

fn ensure_required_runtime_files(runtime_files: &mut Vec<RuntimeInstallFile>) {
    let required_short = match to_short_name_11(REQUIRED_RUNTIME_LIBFFMPEG_SHORT) {
        Some(v) => v,
        None => return,
    };
    let required_source_norm = normalize_runtime_source_path(REQUIRED_RUNTIME_LIBFFMPEG_SOURCE);

    let alias_idx = runtime_files
        .iter()
        .position(|f| f.bucket == RuntimeBucket::Lib && f.short_name == required_short);
    let source_idx = runtime_files
        .iter()
        .position(|f| normalize_runtime_source_path(f.source_path.as_str()) == required_source_norm);
    if alias_idx.is_some() && source_idx.is_some() {
        return;
    }

    let fallback_idx = source_idx.or_else(|| {
        runtime_files.iter().position(|f| {
            runtime_source_leaf(f.source_path.as_str())
                .eq_ignore_ascii_case(REQUIRED_RUNTIME_LIBFFMPEG_LEAF)
        })
    });

    let mut content = fallback_idx.map(|idx| runtime_files[idx].content.clone());
    if content.is_none() {
        content = try_read_runtime_file_from_boot_fs(
            REQUIRED_RUNTIME_LIBFFMPEG_SHORT,
            RuntimeBucket::Lib,
        );
    }
    let Some(bytes) = content else {
        crate::println("Preboot installer: warning missing required runtime libffmpeg source.");
        return;
    };
    if bytes.is_empty() || bytes.len() > u32::MAX as usize {
        return;
    }

    if let Some(idx) = alias_idx {
        runtime_files[idx].bucket = RuntimeBucket::Lib;
        runtime_files[idx].source_path = String::from(REQUIRED_RUNTIME_LIBFFMPEG_SOURCE);
        if runtime_files[idx].content.is_empty() {
            runtime_files[idx].content = bytes;
        }
        return;
    }

    runtime_files.push(RuntimeInstallFile {
        short_name: required_short,
        source_path: String::from(REQUIRED_RUNTIME_LIBFFMPEG_SOURCE),
        bucket: RuntimeBucket::Lib,
        content: bytes,
    });
}

fn parse_runtime_manifest_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    if trimmed.eq_ignore_ascii_case("LINUXRT INSTALL") {
        return None;
    }

    let split = trimmed.find("<-")?;
    let left = trimmed[..split].trim();
    let right = trimmed[split + 2..].trim();
    if right.is_empty() {
        return None;
    }

    let mut parts = left.split_whitespace();
    let _index = parts.next()?;
    let short = parts.next()?.trim();
    if short.is_empty() {
        return None;
    }
    Some((String::from(short), String::from(right)))
}

fn to_short_name_11(name: &str) -> Option<[u8; 11]> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (stem_raw, ext_raw) = if let Some(dot) = trimmed.rfind('.') {
        (&trimmed[..dot], &trimmed[dot + 1..])
    } else {
        (trimmed, "")
    };
    if stem_raw.is_empty() {
        return None;
    }

    let mut out = [b' '; 11];
    let mut stem_len = 0usize;
    for b in stem_raw.bytes() {
        if stem_len >= 8 {
            break;
        }
        let c = b.to_ascii_uppercase();
        // Si es válido lo dejamos, si no, lo convertimos en un guion bajo '_'
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'~' || c == b'$' {
            out[stem_len] = c;
        } else {
            out[stem_len] = b'_'; 
        }
        stem_len += 1;
    }

    let mut ext_len = 0usize;
    for b in ext_raw.bytes() {
        if ext_len >= 3 {
            break;
        }
        let c = b.to_ascii_uppercase();
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
            out[8 + ext_len] = c;
        } else {
            out[8 + ext_len] = b'_';
        }
        ext_len += 1;
    }

    Some(out)
}

fn short_name_to_string(short_name: [u8; 11]) -> String {
    let mut stem_end = 8usize;
    while stem_end > 0 && short_name[stem_end - 1] == b' ' {
        stem_end -= 1;
    }
    let mut ext_end = 3usize;
    while ext_end > 0 && short_name[8 + ext_end - 1] == b' ' {
        ext_end -= 1;
    }

    let mut out = String::new();
    if stem_end > 0 {
        let mut i = 0usize;
        while i < stem_end {
            out.push(short_name[i] as char);
            i += 1;
        }
    }
    if ext_end > 0 {
        out.push('.');
        let mut i = 0usize;
        while i < ext_end {
            out.push(short_name[8 + i] as char);
            i += 1;
        }
    }
    out
}

fn read_boot_fs_path(fs: &mut UefiFileSystem, path: &str) -> Result<Vec<u8>, &'static str> {
    let cpath = CString16::try_from(path).map_err(|_| "BOOT FS PATH UCS2 INVALID.")?;
    fs.read(cpath.as_ref()).map_err(|_| "BOOT FS FILE READ FAILED.")
}

fn ascii_lower_owned(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_uppercase() {
            out.push((b + 32) as char);
        } else {
            out.push(b as char);
        }
    }
    out
}

fn servort_candidate_score(name: &str) -> u32 {
    let lower = ascii_lower_owned(name.trim());
    let is_bin = lower.ends_with(".bin") || lower.ends_with(".bin;1");
    if !is_bin {
        return 0;
    }

    if lower == "svrt0001.bin" || lower == "svrt0001.bin;1" {
        100
    } else if lower.contains("svrt0001") {
        95
    } else if lower.starts_with("svrt") {
        90
    } else if lower.contains("servort") {
        85
    } else if lower.contains("servo") {
        80
    } else if lower.starts_with("surt") {
        70
    } else {
        40
    }
}

fn servort_file_size_bytes(file: &ServortInstallFile) -> usize {
    if !file.content.is_empty() {
        file.content.len()
    } else {
        file.declared_size as usize
    }
}

fn scan_runtime_bucket_dir(
    fs: &mut UefiFileSystem,
    dir_path: &str,
    source_prefix: &str,
    bucket: RuntimeBucket,
    out: &mut Vec<RuntimeInstallFile>,
    total_bytes: &mut usize,
) {
    // 1. Avisar que estamos intentando abrir la carpeta
    crate::println(format!("Buscando carpeta: {}", dir_path).as_str());

    let cdir = match CString16::try_from(dir_path) {
        Ok(v) => v,
        Err(_) => {
            crate::println("-> ERROR: Ruta invalida (CString16)");
            return;
        }
    };

    let iter = match fs.read_dir(cdir.as_ref()) {
        Ok(v) => {
            crate::println("-> OK: Carpeta encontrada y abierta");
            v
        },
        Err(_) => {
            crate::println("-> ERROR: No se pudo abrir la carpeta (quiza no existe en el medio origen)");
            return;
        }
    };

    for entry in iter {
        let Ok(info) = entry else { continue; };
        if info.is_directory() { continue; }

        let file_name: String = String::from(info.file_name());
        if file_name.is_empty() { continue; }

        // 2. Avisar qué archivo detectó el firmware UEFI
        crate::println(format!("  [Archivo]: {}", file_name).as_str());

        let Some(short_name) = to_short_name_11(file_name.as_str()) else {
            crate::println("    -> Ignorado: fallo la validacion to_short_name_11");
            continue;
        };

        let duplicate = out
            .iter()
            .any(|existing| existing.short_name == short_name && existing.bucket == bucket);
        if duplicate {
            crate::println("    -> Ignorado: duplicado");
            continue;
        }

        let full_path = format!("{}\\{}", dir_path, file_name.as_str());
        
        // 3. Avisar si la lectura del archivo fue exitosa
        let bytes = match read_boot_fs_path(fs, full_path.as_str()) {
            Ok(v) => {
                crate::println(format!("    -> Leidos {} bytes", v.len()).as_str());
                v
            },
            Err(_) => {
                crate::println("    -> ERROR: Fallo la lectura del archivo");
                continue;
            }
        };

        if bytes.is_empty() || bytes.len() > u32::MAX as usize {
            crate::println("    -> Ignorado: tamano de archivo invalido");
            continue;
        }
        if total_bytes.saturating_add(bytes.len()) > RUNTIME_COPY_MAX_BYTES {
            crate::println("    -> ALTO: Se alcanzo el limite maximo de RAM para la copia");
            return;
        }

        *total_bytes += bytes.len();
        out.push(RuntimeInstallFile {
            short_name,
            source_path: format!("{}/{}", source_prefix, ascii_lower_owned(file_name.as_str())),
            bucket,
            content: bytes,
        });
    }
}

fn load_runtime_from_boot_fs_fallback_scan(fs: &mut UefiFileSystem) -> Vec<RuntimeInstallFile> {
    let mut out = Vec::new();
    let mut total_bytes = 0usize;

    scan_runtime_bucket_dir(
        fs,
        "\\LINUXRT\\LIB",
        "lib",
        RuntimeBucket::Lib,
        &mut out,
        &mut total_bytes,
    );
    scan_runtime_bucket_dir(
        fs,
        "\\LINUXRT\\LIB64",
        "lib64",
        RuntimeBucket::Lib64,
        &mut out,
        &mut total_bytes,
    );
    scan_runtime_bucket_dir(
        fs,
        "\\LINUXRT\\USR\\LIB",
        "usr/lib",
        RuntimeBucket::UsrLib,
        &mut out,
        &mut total_bytes,
    );
    scan_runtime_bucket_dir(
        fs,
        "\\LINUXRT\\USR\\LIB64",
        "usr/lib64",
        RuntimeBucket::UsrLib64,
        &mut out,
        &mut total_bytes,
    );
    scan_runtime_bucket_dir(
        fs,
        "\\LINUXRT\\BIN",
        "bin",
        RuntimeBucket::Bin,
        &mut out,
        &mut total_bytes,
    );
    scan_runtime_bucket_dir(
        fs,
        "\\LINUXRT\\ETC",
        "etc",
        RuntimeBucket::Etc,
        &mut out,
        &mut total_bytes,
    );
    scan_runtime_bucket_dir(
        fs,
        "\\LINUXRT\\USR\\BIN",
        "usr/bin",
        RuntimeBucket::UsrBin,
        &mut out,
        &mut total_bytes,
    );

    out
}

fn load_runtime_from_fs(fs: &mut UefiFileSystem) -> Vec<RuntimeInstallFile> {
    let manifest_bytes = match read_boot_fs_path(fs, "\\LINUXRT\\RTBASE.LST") {
        Ok(v) => v,
        Err(_) => return load_runtime_from_boot_fs_fallback_scan(fs),
    };
    let manifest_text = match core::str::from_utf8(manifest_bytes.as_slice()) {
        Ok(v) => v,
        Err(_) => return load_runtime_from_boot_fs_fallback_scan(fs),
    };

    let mut out = Vec::new();
    let mut total_bytes = 0usize;
    for line in manifest_text.lines() {
        let Some((short_name_text, source_path)) = parse_runtime_manifest_line(line) else {
            continue;
        };
        let Some(short_name) = to_short_name_11(short_name_text.as_str()) else {
            continue;
        };

        let bucket = runtime_bucket_from_source_path(source_path.as_str());
        let src_path = format!(
            "\\LINUXRT\\{}\\{}",
            runtime_bucket_dir_name(bucket),
            short_name_text
        );
        let bytes = match read_boot_fs_path(fs, src_path.as_str()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if bytes.is_empty() {
            continue;
        }
        if bytes.len() > u32::MAX as usize {
            continue;
        }
        if total_bytes.saturating_add(bytes.len()) > RUNTIME_COPY_MAX_BYTES {
            break;
        }

        let duplicate = out.iter().any(|existing: &RuntimeInstallFile| {
            existing.short_name == short_name && existing.bucket == bucket
        });
        if duplicate {
            continue;
        }

        total_bytes += bytes.len();
        out.push(RuntimeInstallFile {
            short_name,
            source_path,
            bucket,
            content: bytes,
        });
    }

    if out.is_empty() {
        load_runtime_from_boot_fs_fallback_scan(fs)
    } else {
        out
    }
}

fn runtime_find_dir_on_global_fat(
    fat: &mut crate::fat32::Fat32,
    parent_cluster: u32,
    name: &str,
) -> Option<u32> {
    use crate::fs::FileType;
    let entries = fat.read_dir_entries(parent_cluster).ok()?;
    for entry in entries.iter() {
        if !entry.valid || entry.file_type != FileType::Directory {
            continue;
        }
        if entry.matches_name(name) || entry.full_name().eq_ignore_ascii_case(name) {
            return Some(if entry.cluster >= 2 {
                entry.cluster
            } else {
                fat.root_cluster
            });
        }
    }
    None
}

fn runtime_find_file_on_global_fat(
    fat: &mut crate::fat32::Fat32,
    parent_cluster: u32,
    name: &str,
) -> Option<crate::fs::DirEntry> {
    use crate::fs::FileType;
    let entries = fat.read_dir_entries(parent_cluster).ok()?;
    for entry in entries.iter() {
        if !entry.valid || entry.file_type != FileType::File {
            continue;
        }
        if entry.matches_name(name) || entry.full_name().eq_ignore_ascii_case(name) {
            return Some(*entry);
        }
    }
    None
}

fn runtime_read_entry_on_global_fat(
    fat: &mut crate::fat32::Fat32,
    entry: &crate::fs::DirEntry,
) -> Option<Vec<u8>> {
    if entry.size == 0 {
        return None;
    }
    let mut out = vec![0u8; entry.size as usize];
    let len = fat
        .read_file_sized(entry.cluster, entry.size as usize, out.as_mut_slice())
        .ok()?;
    if len == 0 {
        return None;
    }
    out.truncate(len);
    Some(out)
}

fn runtime_bucket_cluster_on_global_fat(
    fat: &mut crate::fat32::Fat32,
    linuxrt_root: u32,
    bucket: RuntimeBucket,
) -> Option<u32> {
    match bucket {
        RuntimeBucket::Lib => runtime_find_dir_on_global_fat(fat, linuxrt_root, "LIB"),
        RuntimeBucket::Lib64 => runtime_find_dir_on_global_fat(fat, linuxrt_root, "LIB64"),
        RuntimeBucket::Bin => runtime_find_dir_on_global_fat(fat, linuxrt_root, "BIN"),
        RuntimeBucket::Etc => runtime_find_dir_on_global_fat(fat, linuxrt_root, "ETC"),
        RuntimeBucket::UsrLib => {
            let usr = runtime_find_dir_on_global_fat(fat, linuxrt_root, "USR")?;
            runtime_find_dir_on_global_fat(fat, usr, "LIB")
        }
        RuntimeBucket::UsrLib64 => {
            let usr = runtime_find_dir_on_global_fat(fat, linuxrt_root, "USR")?;
            runtime_find_dir_on_global_fat(fat, usr, "LIB64")
        }
        RuntimeBucket::UsrBin => {
            let usr = runtime_find_dir_on_global_fat(fat, linuxrt_root, "USR")?;
            runtime_find_dir_on_global_fat(fat, usr, "BIN")
        }
    }
}

fn load_runtime_from_fat_instance(fat: &mut crate::fat32::Fat32) -> Vec<RuntimeInstallFile> {
    use crate::fs::FileType;

    let root = fat.root_cluster;
    let entries = match fat.read_dir_entries(root) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let linuxrt_root = entries
        .iter()
        .find(|entry| {
            entry.valid
                && entry.file_type == FileType::Directory
                && (entry.matches_name("LINUXRT")
                    || entry.full_name().eq_ignore_ascii_case("LINUXRT"))
        })
        .map(|entry| if entry.cluster >= 2 { entry.cluster } else { root });
    let Some(linuxrt_root) = linuxrt_root else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut total_bytes = 0usize;

    if let Some(manifest_entry) = runtime_find_file_on_global_fat(fat, linuxrt_root, "RTBASE.LST") {
        if let Some(manifest_bytes) = runtime_read_entry_on_global_fat(fat, &manifest_entry) {
            if let Ok(manifest_text) = core::str::from_utf8(manifest_bytes.as_slice()) {
                for line in manifest_text.lines() {
                    let Some((short_name_text, source_path)) = parse_runtime_manifest_line(line) else {
                        continue;
                    };
                    let Some(short_name) = to_short_name_11(short_name_text.as_str()) else {
                        continue;
                    };
                    let bucket = runtime_bucket_from_source_path(source_path.as_str());
                    let Some(bucket_cluster) =
                        runtime_bucket_cluster_on_global_fat(fat, linuxrt_root, bucket)
                    else {
                        continue;
                    };
                    let Some(file_entry) =
                        runtime_find_file_on_global_fat(fat, bucket_cluster, short_name_text.as_str())
                    else {
                        continue;
                    };
                    let Some(bytes) = runtime_read_entry_on_global_fat(fat, &file_entry) else {
                        continue;
                    };
                    if bytes.len() > u32::MAX as usize {
                        continue;
                    }
                    if total_bytes.saturating_add(bytes.len()) > RUNTIME_COPY_MAX_BYTES {
                        break;
                    }
                    if out.iter().any(|existing: &RuntimeInstallFile| {
                        existing.short_name == short_name && existing.bucket == bucket
                    }) {
                        continue;
                    }

                    total_bytes += bytes.len();
                    out.push(RuntimeInstallFile {
                        short_name,
                        source_path,
                        bucket,
                        content: bytes,
                    });
                }
            }
        }
    }

    if !out.is_empty() {
        return out;
    }

    let buckets = [
        (RuntimeBucket::Lib, "lib"),
        (RuntimeBucket::Lib64, "lib64"),
        (RuntimeBucket::UsrLib, "usr/lib"),
        (RuntimeBucket::UsrLib64, "usr/lib64"),
        (RuntimeBucket::Bin, "bin"),
        (RuntimeBucket::Etc, "etc"),
        (RuntimeBucket::UsrBin, "usr/bin"),
    ];

    for (bucket, source_prefix) in buckets.iter().copied() {
        let Some(bucket_cluster) = runtime_bucket_cluster_on_global_fat(fat, linuxrt_root, bucket)
        else {
            continue;
        };
        let entries = match fat.read_dir_entries(bucket_cluster) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::File {
                continue;
            }
            let file_name = entry.full_name();
            let Some(short_name) = to_short_name_11(file_name.as_str()) else {
                continue;
            };
            if out.iter().any(|existing: &RuntimeInstallFile| {
                existing.short_name == short_name && existing.bucket == bucket
            }) {
                continue;
            }
            let Some(bytes) = runtime_read_entry_on_global_fat(fat, entry) else {
                continue;
            };
            if bytes.len() > u32::MAX as usize {
                continue;
            }
            if total_bytes.saturating_add(bytes.len()) > RUNTIME_COPY_MAX_BYTES {
                return out;
            }

            total_bytes += bytes.len();
            out.push(RuntimeInstallFile {
                short_name,
                source_path: format!("{}/{}", source_prefix, ascii_lower_owned(file_name.as_str())),
                bucket,
                content: bytes,
            });
        }
    }

    out
}

fn load_runtime_from_embedded_bundle() -> Result<Vec<RuntimeInstallFile>, &'static str> {
    let data = EMBEDDED_LINUXRT_BUNDLE;
    if data.len() < 10 {
        return Err("EMBEDDED LINUXRT BUNDLE TOO SMALL.");
    }
    if &data[0..6] != EMBEDDED_LINUXRT_BUNDLE_MAGIC {
        return Err("EMBEDDED LINUXRT BUNDLE MAGIC INVALID.");
    }
    draw_bootstrap_progress("LOADING LINUXRT", 56, "BUNDLE HEADER OK");

    let mut count_raw = [0u8; 4];
    count_raw.copy_from_slice(&data[6..10]);
    let entry_count = u32::from_le_bytes(count_raw) as usize;
    crate::println("Preboot installer: embedded linuxrt entries");
    crate::println_num(entry_count as u64);
    if entry_count == 0 {
        return Err("EMBEDDED LINUXRT BUNDLE IS EMPTY.");
    }

    let mut cursor = 10usize;
    let mut out = Vec::new();
    let mut total_bytes = 0usize;
    let mut last_ui_pct = 0u8;

    let mut idx = 0usize;
    while idx < entry_count {
        if cursor + 18 > data.len() {
            return Err("EMBEDDED LINUXRT BUNDLE TRUNCATED (HEADER).");
        }

        let mut short_name = [b' '; 11];
        short_name.copy_from_slice(&data[cursor..cursor + 11]);
        cursor += 11;

        let bucket = match data[cursor] {
            0 => RuntimeBucket::Lib,
            1 => RuntimeBucket::Lib64,
            2 => RuntimeBucket::UsrLib,
            3 => RuntimeBucket::UsrLib64,
            4 => RuntimeBucket::Bin,
            5 => RuntimeBucket::Etc,
            6 => RuntimeBucket::UsrBin,
            _ => RuntimeBucket::Lib,
        };
        cursor += 1;

        let mut source_len_raw = [0u8; 2];
        source_len_raw.copy_from_slice(&data[cursor..cursor + 2]);
        let source_len = u16::from_le_bytes(source_len_raw) as usize;
        cursor += 2;

        let mut content_len_raw = [0u8; 4];
        content_len_raw.copy_from_slice(&data[cursor..cursor + 4]);
        let content_len = u32::from_le_bytes(content_len_raw) as usize;
        cursor += 4;

        let end = match cursor
            .checked_add(source_len)
            .and_then(|v| v.checked_add(content_len))
        {
            Some(v) => v,
            None => return Err("EMBEDDED LINUXRT BUNDLE OVERFLOW."),
        };
        if end > data.len() {
            return Err("EMBEDDED LINUXRT BUNDLE TRUNCATED (CONTENT).");
        }

        let source_path_bytes = &data[cursor..cursor + source_len];
        cursor += source_len;
        let source_path = match core::str::from_utf8(source_path_bytes) {
            Ok(v) => String::from(v),
            Err(_) => short_name_to_string(short_name),
        };

        let content_bytes = &data[cursor..cursor + content_len];
        cursor += content_len;
        if content_bytes.is_empty() {
            idx += 1;
            continue;
        }
        if total_bytes.saturating_add(content_bytes.len()) > RUNTIME_COPY_MAX_BYTES {
            crate::println("Preboot installer: embedded linuxrt hit runtime max bytes");
            break;
        }
        if out.iter().any(|existing: &RuntimeInstallFile| {
            existing.short_name == short_name && existing.bucket == bucket
        }) {
            idx += 1;
            continue;
        }

        total_bytes += content_bytes.len();
        out.push(RuntimeInstallFile {
            short_name,
            source_path,
            bucket,
            content: content_bytes.to_vec(),
        });

        if entry_count > 0 {
            let pct = ((idx + 1) * 100 / entry_count) as u8;
            if pct >= last_ui_pct.saturating_add(10) || idx + 1 == entry_count {
                last_ui_pct = pct;
                let ui_pct = 56u8.saturating_add(pct / 5);
                let detail = format!(
                    "PARSED {} / {} FILES ({} KB)",
                    idx + 1,
                    entry_count,
                    total_bytes / 1024
                );
                draw_bootstrap_progress("LOADING LINUXRT", ui_pct, detail.as_str());
            }
        }

        idx += 1;
    }

    if out.is_empty() {
        return Err("EMBEDDED LINUXRT BUNDLE HAS NO VALID FILES.");
    }

    crate::println("Preboot installer: embedded linuxrt parsed files");
    crate::println_num(out.len() as u64);
    crate::println("Preboot installer: embedded linuxrt parsed bytes");
    crate::println_num(total_bytes as u64);
    Ok(out)
}

fn load_runtime_from_global_fat(allow_removable: bool) -> Vec<RuntimeInstallFile> {
    let mut volumes = crate::fat32::Fat32::detect_uefi_fat_volumes();
    if !allow_removable {
        volumes.retain(|v| !v.removable);
    }
    if !volumes.is_empty() {
        if allow_removable {
            volumes.sort_by_key(|v| (!v.removable, v.index));
        } else {
            volumes.sort_by_key(|v| v.index);
        }
        for volume in volumes.iter() {
            let mut probe_fat = crate::fat32::Fat32::new();
            if probe_fat.mount_uefi_fat_volume(volume.index).is_err() {
                continue;
            }
            let runtime = load_runtime_from_fat_instance(&mut probe_fat);
            if !runtime.is_empty() {
                return runtime;
            }
        }
    }

    let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
    if !fat.init() {
        return Vec::new();
    }

    if !allow_removable {
        let Some(handle) = fat.uefi_block_handle else {
            return Vec::new();
        };
        if handle_is_removable(handle) != Some(false) {
            return Vec::new();
        }
    }

    load_runtime_from_fat_instance(fat)
}

fn load_runtime_from_boot_device_only() -> Vec<RuntimeInstallFile> {
    let parent_image = boot::image_handle();
    if let Ok(fs_proto) = boot::get_image_file_system(parent_image) {
        let mut fs = UefiFileSystem::new(fs_proto);
        let runtime = load_runtime_from_fs(&mut fs);
        if !runtime.is_empty() {
            return runtime;
        }
    }

    Vec::new()
}

fn load_servort_from_fs(fs: &mut UefiFileSystem) -> Vec<ServortInstallFile> {
    let mut out = Vec::new();
    let Some(short_name) = to_short_name_11("SVRT0001.BIN") else {
        return out;
    };

    let dir_candidates = ["\\SERVORT", "\\servort", "\\LINUXRT\\SERVORT", "\\linuxrt\\servort"];
    let mut best_score = 0u32;
    let mut best_path: Option<String> = None;
    let mut best_bytes: Option<Vec<u8>> = None;

    let direct_candidates = [
        "\\SERVORT\\SVRT0001.BIN",
        "\\SERVORT\\svrt0001.bin",
        "\\servort\\SVRT0001.BIN",
        "\\servort\\svrt0001.bin",
        "\\LINUXRT\\SERVORT\\SVRT0001.BIN",
        "\\LINUXRT\\SERVORT\\svrt0001.bin",
        "\\linuxrt\\servort\\SVRT0001.BIN",
        "\\linuxrt\\servort\\svrt0001.bin",
    ];
    for path in direct_candidates.iter().copied() {
        let bytes = match read_boot_fs_path(fs, path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if bytes.is_empty() || bytes.len() > u32::MAX as usize {
            continue;
        }
        if bytes.len() > SERVORT_COPY_MAX_BYTES {
            continue;
        }
        if bytes.len() < 4 || &bytes[0..4] != b"\x7FELF" {
            continue;
        }
        let file_name = path.rsplit('\\').next().unwrap_or(path);
        let score = servort_candidate_score(file_name).saturating_add(20);
        if best_bytes.is_none() || score > best_score {
            best_score = score;
            best_path = Some(String::from(path));
            best_bytes = Some(bytes);
        }
    }

    for dir_path in dir_candidates.iter().copied() {
        let cdir = match CString16::try_from(dir_path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let iter = match fs.read_dir(cdir.as_ref()) {
            Ok(v) => v,
            Err(_) => continue,
        };

        for entry in iter {
            let Ok(info) = entry else { continue; };
            if info.is_directory() {
                continue;
            }
            let file_name = String::from(info.file_name());
            if file_name.trim().is_empty() {
                continue;
            }

            let score = servort_candidate_score(file_name.as_str());
            if score == 0 {
                continue;
            }

            let full_path = format!("{}\\{}", dir_path, file_name.as_str());
            let bytes = match read_boot_fs_path(fs, full_path.as_str()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if bytes.is_empty() || bytes.len() > u32::MAX as usize {
                continue;
            }
            if bytes.len() > SERVORT_COPY_MAX_BYTES {
                crate::println("Preboot installer: servort candidate exceeds copy budget.");
                continue;
            }
            if bytes.len() < 4 || &bytes[0..4] != b"\x7FELF" {
                continue;
            }

            let replace = if best_bytes.is_none() || score > best_score {
                true
            } else if score == best_score {
                bytes.len() > best_bytes.as_ref().map(|v| v.len()).unwrap_or(0)
            } else {
                false
            };
            if replace {
                best_score = score;
                best_path = Some(full_path);
                best_bytes = Some(bytes);
            }
        }
    }

    if let Some(bytes) = best_bytes {
        crate::println("Preboot installer: servort launcher selected");
        if let Some(path) = best_path.as_ref() {
            crate::println(path.as_str());
        }
        crate::println("Preboot installer: servort launcher loaded bytes");
        crate::println_num(bytes.len() as u64);
        let declared_size = bytes.len() as u32;
        out.push(ServortInstallFile {
            short_name,
            source_path: String::from("/SERVORT/SVRT0001.BIN"),
            declared_size,
            source_fat: None,
            content: bytes,
        });
    }

    out
}

fn load_servort_from_fs_handle(parent_image: Handle, handle: Handle) -> Vec<ServortInstallFile> {
    use uefi::proto::media::fs::SimpleFileSystem;

    if let Ok(fs_proto) = boot::open_protocol_exclusive::<SimpleFileSystem>(handle) {
        let mut fs = UefiFileSystem::new(fs_proto);
        return load_servort_from_fs(&mut fs);
    }

    let params = OpenProtocolParams {
        handle,
        agent: parent_image,
        controller: None,
    };
    let fs_proto = match unsafe {
        boot::open_protocol::<SimpleFileSystem>(params, OpenProtocolAttributes::GetProtocol)
    } {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut fs = UefiFileSystem::new(fs_proto);
    load_servort_from_fs(&mut fs)
}

fn load_servort_from_fat_instance(
    fat: &mut crate::fat32::Fat32,
    volume_index: Option<usize>,
) -> Vec<ServortInstallFile> {
    use crate::fs::FileType;

    let mut out = Vec::new();
    let Some(short_name) = to_short_name_11("SVRT0001.BIN") else {
        return out;
    };

    let root = fat.root_cluster;
    let mut dir_candidates: [Option<(u32, &str)>; 2] = [None, None];

    if let Some(servort_root) = runtime_find_dir_on_global_fat(fat, root, "SERVORT") {
        dir_candidates[0] = Some((servort_root, "/SERVORT"));
    }
    if let Some(linuxrt_root) = runtime_find_dir_on_global_fat(fat, root, "LINUXRT") {
        if let Some(servort_dir) = runtime_find_dir_on_global_fat(fat, linuxrt_root, "SERVORT") {
            dir_candidates[1] = Some((servort_dir, "/LINUXRT/SERVORT"));
        }
    }

    let mut best_score = 0u32;
    let mut best_path: Option<String> = None;
    let mut best_entry: Option<crate::fs::DirEntry> = None;

    for candidate in dir_candidates.iter().copied().flatten() {
        let (dir_cluster, source_prefix) = candidate;
        let entries = match fat.read_dir_entries(dir_cluster) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for entry in entries.iter() {
            if !entry.valid || entry.file_type != FileType::File {
                continue;
            }
            let file_name = entry.full_name();
            if file_name.trim().is_empty() {
                continue;
            }
            let score = servort_candidate_score(file_name.as_str());
            if score == 0 {
                continue;
            }
            if entry.size == 0
                || entry.size as usize > SERVORT_COPY_MAX_BYTES
                || entry.size as usize > u32::MAX as usize
            {
                continue;
            }
            let mut magic = [0u8; 4];
            let Ok(magic_len) = fat.read_file_range(entry.cluster, entry.size as usize, 0, &mut magic) else {
                continue;
            };
            if magic_len < 4 || magic != *b"\x7FELF" {
                continue;
            }

            let replace = if best_entry.is_none() || score > best_score {
                true
            } else if score == best_score {
                entry.size > best_entry.as_ref().map(|v| v.size).unwrap_or(0)
            } else {
                false
            };
            if replace {
                best_score = score;
                best_path = Some(format!("{}/{}", source_prefix, file_name.as_str()));
                best_entry = Some(*entry);
            }
        }
    }

    if let Some(entry) = best_entry {
        crate::println("Preboot installer: servort launcher selected from FAT");
        if let Some(path) = best_path.as_ref() {
            crate::println(path.as_str());
        }
        crate::println("Preboot installer: servort launcher size bytes");
        crate::println_num(entry.size as u64);
        out.push(ServortInstallFile {
            short_name,
            source_path: best_path.unwrap_or_else(|| String::from("/SERVORT/SVRT0001.BIN")),
            declared_size: entry.size,
            source_fat: Some(ServortFatSource {
                volume_index,
                start_cluster: entry.cluster,
            }),
            content: Vec::new(),
        });
    }

    out
}

fn load_servort_from_global_fat(allow_removable: bool) -> Vec<ServortInstallFile> {
    let mut volumes = crate::fat32::Fat32::detect_uefi_fat_volumes();
    if !allow_removable {
        volumes.retain(|v| !v.removable);
    }
    if !volumes.is_empty() {
        if allow_removable {
            volumes.sort_by_key(|v| (!v.removable, v.index));
        } else {
            volumes.sort_by_key(|v| v.index);
        }
        for volume in volumes.iter() {
            let mut probe_fat = crate::fat32::Fat32::new();
            if probe_fat.mount_uefi_fat_volume(volume.index).is_err() {
                continue;
            }
            let files = load_servort_from_fat_instance(&mut probe_fat, Some(volume.index));
            if !files.is_empty() {
                return files;
            }
        }
    }

    let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
    if !fat.init() {
        return Vec::new();
    }

    if !allow_removable {
        let Some(handle) = fat.uefi_block_handle else {
            return Vec::new();
        };
        if handle_is_removable(handle) != Some(false) {
            return Vec::new();
        }
    }

    load_servort_from_fat_instance(fat, None)
}

fn load_servort_from_boot_fs() -> Vec<ServortInstallFile> {
    let files = load_servort_from_global_fat(true);
    if !files.is_empty() {
        return files;
    }

    let parent_image = boot::image_handle();
    if let Ok(fs_proto) = boot::get_image_file_system(parent_image) {
        let mut fs = UefiFileSystem::new(fs_proto);
        let files = load_servort_from_fs(&mut fs);
        if !files.is_empty() {
            return files;
        }
    }

    use uefi::proto::media::fs::SimpleFileSystem;
    let handles = match boot::find_handles::<SimpleFileSystem>() {
        Ok(v) => v,
        Err(_) => return load_servort_from_global_fat(true),
    };

    let boot_handle = current_boot_device_handle();
    if let Some(handle) = boot_handle {
        let files = load_servort_from_fs_handle(parent_image, handle);
        if !files.is_empty() {
            return files;
        }
    }

    for pass in 0..3usize {
        for handle in handles.iter().copied() {
            if Some(handle) == boot_handle {
                continue;
            }
            let removable = handle_is_removable(handle);
            let allow = match pass {
                0 => removable == Some(true),
                1 => removable == Some(false),
                _ => true,
            };
            if !allow {
                continue;
            }

            let files = load_servort_from_fs_handle(parent_image, handle);
            if !files.is_empty() {
                return files;
            }
        }
    }

    load_servort_from_global_fat(true)
}

fn load_runtime_from_boot_fs() -> Vec<RuntimeInstallFile> {
    use uefi::proto::media::fs::SimpleFileSystem;

    let parent_image = boot::image_handle();

    if let Some(boot_handle) = current_boot_device_handle() {
        if handle_is_removable(boot_handle) == Some(false) {
            if let Ok(fs_proto) = boot::get_image_file_system(parent_image) {
                let mut fs = UefiFileSystem::new(fs_proto);
                let runtime = load_runtime_from_fs(&mut fs);
                if !runtime.is_empty() {
                    return runtime;
                }
            }
        }
    }

    let handles = match boot::find_handles::<SimpleFileSystem>() {
        Ok(v) => v,
        Err(_) => return load_runtime_from_global_fat(false),
    };

    for handle in handles.iter().copied() {
        if handle_is_removable(handle) != Some(false) {
            continue;
        }

        if let Ok(fs_proto) = boot::open_protocol_exclusive::<SimpleFileSystem>(handle) {
            let mut fs = UefiFileSystem::new(fs_proto);
            let runtime = load_runtime_from_fs(&mut fs);
            if !runtime.is_empty() {
                return runtime;
            }
            continue;
        }

        let params = OpenProtocolParams {
            handle,
            agent: parent_image,
            controller: None,
        };
        let fs_proto = match unsafe {
            boot::open_protocol::<SimpleFileSystem>(params, OpenProtocolAttributes::GetProtocol)
        } {
            Ok(v) => v,
            Err(_) => continue,
        };
        let mut fs = UefiFileSystem::new(fs_proto);
        let runtime = load_runtime_from_fs(&mut fs);
        if !runtime.is_empty() {
            return runtime;
        }
    }

    load_runtime_from_global_fat(false)
}

fn load_runtime_from_boot_fs_with_retry(attempts: usize, stall_us: usize) -> Vec<RuntimeInstallFile> {
    let tries = core::cmp::max(1usize, attempts);
    for attempt in 0..tries {
        let runtime = load_runtime_from_boot_fs();
        if !runtime.is_empty() || attempt + 1 >= tries {
            return runtime;
        }
        boot::stall(stall_us);
    }
    Vec::new()
}

fn default_grub_config_payload() -> Vec<u8> {
    b"set timeout=8\r\n\
set default=0\r\n\
\r\n\
menuentry \"Zenox OS\" {\r\n\
    if search --no-floppy --file --set=reduxroot /EFI/BOOT/REDUX64.EFI; then\r\n\
        chainloader ($reduxroot)/EFI/BOOT/REDUX64.EFI\r\n\
        boot\r\n\
    fi\r\n\
    if search --no-floppy --file --set=reduxroot /EFI/BOOT/BOOTX64.EFI; then\r\n\
        chainloader ($reduxroot)/EFI/BOOT/BOOTX64.EFI\r\n\
        boot\r\n\
    fi\r\n\
}\r\n\
\r\n\
menuentry \"Windows Boot Manager\" {\r\n\
    if search --no-floppy --file --set=winroot /EFI/Microsoft/Boot/bootmgfw.redux.bak.efi; then\r\n\
        chainloader ($winroot)/EFI/Microsoft/Boot/bootmgfw.redux.bak.efi\r\n\
        boot\r\n\
    fi\r\n\
    if search --no-floppy --file --set=winroot /EFI/Microsoft/Boot/bootmgfw.efi; then\r\n\
        chainloader ($winroot)/EFI/Microsoft/Boot/bootmgfw.efi\r\n\
        boot\r\n\
    fi\r\n\
}\r\n\
\r\n\
menuentry \"UEFI Firmware Settings\" {\r\n\
    fwsetup\r\n\
}\r\n"
        .to_vec()
}

fn load_optional_grub_assets_from_boot_fs() -> Option<GrubInstallAssets> {
    let parent_image = boot::image_handle();
    let fs_proto = boot::get_image_file_system(parent_image).ok()?;
    let mut fs = UefiFileSystem::new(fs_proto);

    let efi_candidates = [
        "\\EFI\\GRUB\\GRUBX64.EFI",
        "\\EFI\\GRUB\\grubx64.efi",
        "\\EFI\\ZENOX\\GRUBX64.EFI",
        "\\EFI\\ZENOX\\grubx64.efi",
        "\\EFI\\GOOS\\GRUBX64.EFI",
        "\\EFI\\GOOS\\grubx64.efi",
        "\\EFI\\REDUXOS\\GRUBX64.EFI",
        "\\EFI\\REDUXOS\\grubx64.efi",
        "\\GRUBX64.EFI",
        "\\grubx64.efi",
    ];
    let mut efi_payload = None;
    for path in efi_candidates.iter() {
        let Ok(bytes) = read_boot_fs_path(&mut fs, path) else {
            continue;
        };
        if bytes.len() < 4096 || bytes.len() > 64 * 1024 * 1024 {
            continue;
        }
        if bytes.len() < 2 || bytes[0] != b'M' || bytes[1] != b'Z' {
            continue;
        }
        efi_payload = Some(bytes);
        break;
    }
    let efi_payload = efi_payload?;

    Some(GrubInstallAssets {
        efi_payload,
        // Always use installer-managed config to avoid stale media configs
        // with fixed paths that break on different partition layouts.
        config_payload: default_grub_config_payload(),
    })
}

fn build_runtime_manifest_content(runtime_files: &[RuntimeInstallFile]) -> Vec<u8> {
    if runtime_files.is_empty() {
        return Vec::new();
    }

    let mut text = String::from("LINUXRT INSTALL\r\n");
    for (idx, file) in runtime_files.iter().enumerate() {
        let short = short_name_to_string(file.short_name);
        text.push_str(
            format!("{:04} {} <- {}\r\n", idx + 1, short, file.source_path.as_str()).as_str(),
        );
    }
    text.into_bytes()
}

fn dir_cluster_count_for_entries(entry_count: usize, cluster_size: usize) -> Result<u32, &'static str> {
    if cluster_size < 32 {
        return Err("CLUSTER SIZE TOO SMALL FOR DIRECTORY ENTRIES.");
    }

    let entries_per_cluster = cluster_size / 32;
    if entries_per_cluster == 0 {
        return Err("DIRECTORY ENTRY CAPACITY INVALID.");
    }

    let needed_entries = entry_count.saturating_add(1);
    let clusters = (needed_entries + entries_per_cluster - 1) / entries_per_cluster;
    Ok(core::cmp::max(1, clusters) as u32)
}

fn allocate_cluster_chain(next_cluster: &mut u32, cluster_count: u32) -> Result<ClusterChainLayout, &'static str> {
    if cluster_count == 0 {
        return Err("CLUSTER CHAIN COUNT INVALID.");
    }

    let first_cluster = *next_cluster;
    let next = first_cluster
        .checked_add(cluster_count)
        .ok_or("CLUSTER INDEX OVERFLOW.")?;
    *next_cluster = next;

    Ok(ClusterChainLayout {
        first_cluster,
        cluster_count,
    })
}

fn read_u16_le(buf: &[u8], off: usize) -> Option<u16> {
    if off + 2 > buf.len() {
        return None;
    }
    Some(u16::from_le_bytes([buf[off], buf[off + 1]]))
}

fn read_u32_le(buf: &[u8], off: usize) -> Option<u32> {
    if off + 4 > buf.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        buf[off],
        buf[off + 1],
        buf[off + 2],
        buf[off + 3],
    ]))
}

fn read_u64_le(buf: &[u8], off: usize) -> Option<u64> {
    if off + 8 > buf.len() {
        return None;
    }
    Some(u64::from_le_bytes([
        buf[off],
        buf[off + 1],
        buf[off + 2],
        buf[off + 3],
        buf[off + 4],
        buf[off + 5],
        buf[off + 6],
        buf[off + 7],
    ]))
}

fn reconstruct_pe_payload(image: &[u8]) -> Result<Vec<u8>, &'static str> {
    draw_bootstrap_progress("LOADING PAYLOAD", 20, "VALIDATING PE STRUCTURE");
    if image.len() < 0x100 {
        return Err("PE IMAGE TOO SMALL.");
    }
    if image[0] != b'M' || image[1] != b'Z' {
        return Err("MZ SIGNATURE MISSING.");
    }

    let lfanew = read_u32_le(image, 0x3C).ok_or("PE HEADER OFFSET MISSING.")? as usize;
    if lfanew + 24 > image.len() {
        return Err("PE HEADER OUT OF RANGE.");
    }

    let sig = read_u32_le(image, lfanew).ok_or("PE SIGNATURE MISSING.")?;
    if sig != 0x0000_4550 {
        return Err("PE SIGNATURE INVALID.");
    }

    let section_count = read_u16_le(image, lfanew + 6).ok_or("PE SECTION COUNT MISSING.")? as usize;
    let size_of_optional =
        read_u16_le(image, lfanew + 20).ok_or("PE OPTIONAL SIZE MISSING.")? as usize;

    let optional_off = lfanew + 24;
    if optional_off + size_of_optional > image.len() {
        return Err("PE OPTIONAL HEADER OUT OF RANGE.");
    }

    let size_of_headers =
        read_u32_le(image, optional_off + 60).ok_or("PE SIZE_OF_HEADERS MISSING.")? as usize;

    let section_table_off = optional_off + size_of_optional;
    let section_table_len = section_count
        .checked_mul(40)
        .ok_or("PE SECTION TABLE OVERFLOW.")?;
    if section_table_off + section_table_len > image.len() {
        return Err("PE SECTION TABLE OUT OF RANGE.");
    }

    let mut file_len = core::cmp::max(size_of_headers, section_table_off + section_table_len);

    let mut i = 0usize;
    let mut last_scan_pct = 0u8;
    while i < section_count {
        let sh = section_table_off + i * 40;
        let virtual_addr = read_u32_le(image, sh + 12).ok_or("SECTION VIRTUAL ADDR MISSING.")? as usize;
        let size_raw = read_u32_le(image, sh + 16).ok_or("SECTION RAW SIZE MISSING.")? as usize;
        let ptr_raw = read_u32_le(image, sh + 20).ok_or("SECTION RAW PTR MISSING.")? as usize;

        if size_raw != 0 && virtual_addr < image.len() {
            let available = image.len() - virtual_addr;
            let copy_len = core::cmp::min(size_raw, available);
            let end = ptr_raw
                .checked_add(copy_len)
                .ok_or("PE RAW LENGTH OVERFLOW.")?;
            if end > file_len {
                file_len = end;
            }
        }
        if section_count > 0 {
            let scan_pct = ((i + 1) * 100 / section_count) as u8;
            if scan_pct >= last_scan_pct.saturating_add(20) || i + 1 == section_count {
                last_scan_pct = scan_pct;
                let ui_pct = 20u8.saturating_add(scan_pct / 4);
                draw_bootstrap_progress("LOADING PAYLOAD", ui_pct, "SCANNING PE SECTIONS");
            }
        }
        i += 1;
    }

    if file_len == 0 || file_len > PAYLOAD_MAX_BYTES {
        return Err("PE RECONSTRUCTED FILE SIZE INVALID.");
    }

    let mut out = Vec::new();
    out.resize(file_len, 0);
    draw_bootstrap_progress("LOADING PAYLOAD", 30, "BUILDING RECONSTRUCTED IMAGE");

    let hdr_copy = core::cmp::min(core::cmp::min(size_of_headers, image.len()), out.len());
    if hdr_copy > 0 {
        out[0..hdr_copy].copy_from_slice(&image[0..hdr_copy]);
    }

    let mut s = 0usize;
    let mut last_copy_pct = 0u8;
    while s < section_count {
        let sh = section_table_off + s * 40;
        let virtual_addr = read_u32_le(image, sh + 12).ok_or("SECTION VIRTUAL ADDR MISSING.")? as usize;
        let size_raw = read_u32_le(image, sh + 16).ok_or("SECTION RAW SIZE MISSING.")? as usize;
        let ptr_raw = read_u32_le(image, sh + 20).ok_or("SECTION RAW PTR MISSING.")? as usize;

        if size_raw == 0 || virtual_addr >= image.len() || ptr_raw >= out.len() {
            s += 1;
            continue;
        }

        let src_avail = image.len() - virtual_addr;
        let dst_avail = out.len() - ptr_raw;
        let copy_len = core::cmp::min(size_raw, core::cmp::min(src_avail, dst_avail));

        if copy_len > 0 {
            out[ptr_raw..ptr_raw + copy_len]
                .copy_from_slice(&image[virtual_addr..virtual_addr + copy_len]);
        }

        if section_count > 0 {
            let copy_pct = ((s + 1) * 100 / section_count) as u8;
            if copy_pct >= last_copy_pct.saturating_add(20) || s + 1 == section_count {
                last_copy_pct = copy_pct;
                let ui_pct = 30u8.saturating_add(copy_pct / 6);
                draw_bootstrap_progress("LOADING PAYLOAD", ui_pct, "COPYING PE SECTIONS");
            }
        }
        s += 1;
    }

    if out.len() < 2 || out[0] != b'M' || out[1] != b'Z' {
        return Err("PE RECONSTRUCTION FAILED.");
    }

    Ok(out)
}

fn map_install_progress(start: u8, end: u8, step: usize, total: usize) -> u8 {
    if end <= start {
        return start;
    }
    if total == 0 {
        return end;
    }
    let bounded = core::cmp::min(step, total);
    let span = (end - start) as usize;
    let delta = (bounded.saturating_mul(span) + (total / 2)) / total;
    start.saturating_add(delta as u8)
}

fn install_to_partition<F>(
    disk_handle: Handle,
    partition_start_lba: u64,
    partition_total_sectors: u64,
    paired_data_start_lba: Option<u64>,
    payload: &[u8],
    grub_assets: Option<&GrubInstallAssets>,
    runtime_files: &[RuntimeInstallFile],
    servort_files: &[ServortInstallFile],
    progress: &mut F,
) -> Result<(), &'static str>
where
    F: FnMut(u8, &str),
{
    progress(1, "VALIDATING INPUT");
    if payload.is_empty() {
        return Err("PAYLOAD IS EMPTY.");
    }
    if payload.len() > u32::MAX as usize {
        return Err("PAYLOAD TOO LARGE.");
    }

    let total_sectors = partition_total_sectors;
    if total_sectors < MIN_INSTALL_SECTORS as u64 {
        return Err("TARGET TOO SMALL (MIN 64 MIB).");
    }
    if total_sectors > u32::MAX as u64 {
        return Err("TARGET TOO LARGE FOR THIS FORMATTER.");
    }

    let mut sectors_per_cluster = choose_sectors_per_cluster(total_sectors);
    let (sectors_per_fat, data_start_rel, cluster_count) = loop {
        let (spf, data_lba, clusters) = compute_layout(total_sectors, sectors_per_cluster)?;
        if clusters >= 65_525 {
            break (spf, data_lba, clusters);
        }
        if sectors_per_cluster == 1 {
            return Err("FAT32 REQUIRES A LARGER TARGET PARTITION.");
        }
        sectors_per_cluster /= 2;
    };
    progress(6, "PLANNING FAT32 LAYOUT");

    let cluster_size = sectors_per_cluster as usize * LOGICAL_SECTOR_SIZE;
    let runtime_enabled = !runtime_files.is_empty();
    let servort_enabled = !servort_files.is_empty();
    let grub_enabled = grub_assets.is_some();
    let boot_payload = if let Some(grub) = grub_assets {
        grub.efi_payload.as_slice()
    } else {
        payload
    };
    let grub_config_payload = grub_assets.map(|grub| grub.config_payload.as_slice());

    let startup_content = b"\\EFI\\BOOT\\BOOTX64.EFI\r\n";
    let mut config_text = format!(
        "[zenox]\r\ninstalled=1\r\nautoboot=gui\r\nboot_start_lba={}\r\nboot_size_sectors={}\r\n",
        partition_start_lba,
        partition_total_sectors
    );
    if let Some(data_start_lba) = paired_data_start_lba {
        config_text.push_str(format!("data_start_lba={}\r\n", data_start_lba).as_str());
    }
    config_text.push_str("boot_efi=\\EFI\\BOOT\\BOOTX64.EFI\r\n");
    let config_content = config_text.into_bytes();
    let mut readme_text = String::from(
        "Zenox OS installed on internal storage.\r\nBoot path: \\EFI\\BOOT\\BOOTX64.EFI\r\n",
    );
    if grub_enabled {
        readme_text.push_str("Boot manager: GRUB.\r\n");
    }
    readme_text.push_str(format!("Boot LBA: {}\r\n", partition_start_lba).as_str());
    if let Some(data_start_lba) = paired_data_start_lba {
        readme_text.push_str(format!("Data LBA: {}\r\n", data_start_lba).as_str());
    }
    let readme_content = readme_text.into_bytes();

    let mut runtime_lib_count = 0usize;
    let mut runtime_lib64_count = 0usize;
    let mut runtime_usr_lib_count = 0usize;
    let mut runtime_usr_lib64_count = 0usize;
    let mut runtime_bin_count = 0usize;
    let mut runtime_etc_count = 0usize;
    let mut runtime_usr_bin_count = 0usize;
    for runtime in runtime_files.iter() {
        if runtime.content.is_empty() {
            return Err("RUNTIME FILE IS EMPTY.");
        }
        if runtime.content.len() > u32::MAX as usize {
            return Err("RUNTIME FILE TOO LARGE.");
        }
        match runtime.bucket {
            RuntimeBucket::Lib => runtime_lib_count += 1,
            RuntimeBucket::Lib64 => runtime_lib64_count += 1,
            RuntimeBucket::UsrLib => runtime_usr_lib_count += 1,
            RuntimeBucket::UsrLib64 => runtime_usr_lib64_count += 1,
            RuntimeBucket::Bin => runtime_bin_count += 1,
            RuntimeBucket::Etc => runtime_etc_count += 1,
            RuntimeBucket::UsrBin => runtime_usr_bin_count += 1,
        }
    }
    progress(10, "ANALYZING RUNTIME FILES");

    for servort in servort_files.iter() {
        let size = servort_file_size_bytes(servort);
        if size == 0 {
            return Err("SERVORT FILE IS EMPTY.");
        }
        if size > u32::MAX as usize {
            return Err("SERVORT FILE TOO LARGE.");
        }
    }
    progress(11, "ANALYZING SERVORT FILES");

    let runtime_manifest_content = build_runtime_manifest_content(runtime_files);
    let runtime_manifest_clusters = if runtime_manifest_content.is_empty() {
        0
    } else {
        cluster_count_for_bytes(runtime_manifest_content.len(), cluster_size)
    };

    let root_dir_entries = 4usize
        + if runtime_enabled { 1 } else { 0 }
        + if servort_enabled { 1 } else { 0 }
        + if grub_enabled { 1 } else { 0 };
    let efi_dir_entries = 1usize + if grub_enabled { 1 } else { 0 };
    let efi_grub_dir_entries = if grub_enabled { 1usize } else { 0usize };
    let boot_dir_entries = 1usize + if grub_enabled { 2 } else { 0 };
    let linuxrt_root_dir_entries = if runtime_enabled { 6usize } else { 0usize };
    let linuxrt_usr_dir_entries = if runtime_enabled { 3usize } else { 0usize };
    let servort_root_dir_entries = if servort_enabled {
        servort_files.len()
    } else {
        0usize
    };

    let root_dir_clusters = dir_cluster_count_for_entries(root_dir_entries, cluster_size)?;
    let efi_dir_clusters = dir_cluster_count_for_entries(efi_dir_entries, cluster_size)?;
    let efi_grub_dir_clusters = if grub_enabled {
        dir_cluster_count_for_entries(efi_grub_dir_entries, cluster_size)?
    } else {
        0
    };
    let boot_dir_clusters = dir_cluster_count_for_entries(boot_dir_entries, cluster_size)?;
    let linuxrt_root_dir_clusters = if runtime_enabled {
        dir_cluster_count_for_entries(linuxrt_root_dir_entries, cluster_size)?
    } else {
        0
    };
    let linuxrt_lib_dir_clusters = if runtime_enabled {
        dir_cluster_count_for_entries(runtime_lib_count, cluster_size)?
    } else {
        0
    };
    let linuxrt_lib64_dir_clusters = if runtime_enabled {
        dir_cluster_count_for_entries(runtime_lib64_count, cluster_size)?
    } else {
        0
    };
    let linuxrt_bin_dir_clusters = if runtime_enabled {
        dir_cluster_count_for_entries(runtime_bin_count, cluster_size)?
    } else {
        0
    };
    let linuxrt_etc_dir_clusters = if runtime_enabled {
        dir_cluster_count_for_entries(runtime_etc_count, cluster_size)?
    } else {
        0
    };
    let linuxrt_usr_dir_clusters = if runtime_enabled {
        dir_cluster_count_for_entries(linuxrt_usr_dir_entries, cluster_size)?
    } else {
        0
    };
    let linuxrt_usr_lib_dir_clusters = if runtime_enabled {
        dir_cluster_count_for_entries(runtime_usr_lib_count, cluster_size)?
    } else {
        0
    };
    let linuxrt_usr_lib64_dir_clusters = if runtime_enabled {
        dir_cluster_count_for_entries(runtime_usr_lib64_count, cluster_size)?
    } else {
        0
    };
    let linuxrt_usr_bin_dir_clusters = if runtime_enabled {
        dir_cluster_count_for_entries(runtime_usr_bin_count, cluster_size)?
    } else {
        0
    };
    let servort_root_dir_clusters = if servort_enabled {
        dir_cluster_count_for_entries(servort_root_dir_entries, cluster_size)?
    } else {
        0
    };

    let mut next_cluster = ROOT_CLUSTER;
    let root_dir = allocate_cluster_chain(&mut next_cluster, root_dir_clusters)?;
    let efi_dir = allocate_cluster_chain(&mut next_cluster, efi_dir_clusters)?;
    let efi_grub_dir = if grub_enabled {
        Some(allocate_cluster_chain(&mut next_cluster, efi_grub_dir_clusters)?)
    } else {
        None
    };
    let boot_dir = allocate_cluster_chain(&mut next_cluster, boot_dir_clusters)?;

    let mut linuxrt_root_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_lib_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_lib64_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_bin_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_etc_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_usr_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_usr_lib_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_usr_lib64_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_usr_bin_dir: Option<ClusterChainLayout> = None;
    let mut servort_root_dir: Option<ClusterChainLayout> = None;
    if runtime_enabled {
        linuxrt_root_dir = Some(allocate_cluster_chain(
            &mut next_cluster,
            linuxrt_root_dir_clusters,
        )?);
        linuxrt_lib_dir = Some(allocate_cluster_chain(
            &mut next_cluster,
            linuxrt_lib_dir_clusters,
        )?);
        linuxrt_lib64_dir = Some(allocate_cluster_chain(
            &mut next_cluster,
            linuxrt_lib64_dir_clusters,
        )?);
        linuxrt_bin_dir = Some(allocate_cluster_chain(
            &mut next_cluster,
            linuxrt_bin_dir_clusters,
        )?);
        linuxrt_etc_dir = Some(allocate_cluster_chain(
            &mut next_cluster,
            linuxrt_etc_dir_clusters,
        )?);
        linuxrt_usr_dir = Some(allocate_cluster_chain(
            &mut next_cluster,
            linuxrt_usr_dir_clusters,
        )?);
        linuxrt_usr_lib_dir = Some(allocate_cluster_chain(
            &mut next_cluster,
            linuxrt_usr_lib_dir_clusters,
        )?);
        linuxrt_usr_lib64_dir = Some(allocate_cluster_chain(
            &mut next_cluster,
            linuxrt_usr_lib64_dir_clusters,
        )?);
        linuxrt_usr_bin_dir = Some(allocate_cluster_chain(
            &mut next_cluster,
            linuxrt_usr_bin_dir_clusters,
        )?);
    }
    if servort_enabled {
        servort_root_dir = Some(allocate_cluster_chain(
            &mut next_cluster,
            servort_root_dir_clusters,
        )?);
    }

    let boot_efi_clusters = cluster_count_for_bytes(boot_payload.len(), cluster_size);
    let startup_clusters = cluster_count_for_bytes(startup_content.len(), cluster_size);
    let config_clusters = cluster_count_for_bytes(config_content.len(), cluster_size);
    let readme_clusters = cluster_count_for_bytes(readme_content.len(), cluster_size);
    let boot_efi_file = allocate_cluster_chain(&mut next_cluster, boot_efi_clusters)?;
    let startup_file = allocate_cluster_chain(&mut next_cluster, startup_clusters)?;
    let config_file = allocate_cluster_chain(&mut next_cluster, config_clusters)?;
    let readme_file = allocate_cluster_chain(&mut next_cluster, readme_clusters)?;
    let redux_efi_file = if grub_enabled {
        let redux_clusters = cluster_count_for_bytes(payload.len(), cluster_size);
        Some(allocate_cluster_chain(&mut next_cluster, redux_clusters)?)
    } else {
        None
    };
    let grub_cfg_file = if let Some(cfg) = grub_config_payload {
        let grub_cfg_clusters = cluster_count_for_bytes(cfg.len(), cluster_size);
        Some(allocate_cluster_chain(&mut next_cluster, grub_cfg_clusters)?)
    } else {
        None
    };
    let efi_grub_cfg_file = if let Some(cfg) = grub_config_payload {
        let grub_cfg_clusters = cluster_count_for_bytes(cfg.len(), cluster_size);
        Some(allocate_cluster_chain(&mut next_cluster, grub_cfg_clusters)?)
    } else {
        None
    };
    let root_grub_cfg_file = if let Some(cfg) = grub_config_payload {
        let root_cfg_clusters = cluster_count_for_bytes(cfg.len(), cluster_size);
        Some(allocate_cluster_chain(&mut next_cluster, root_cfg_clusters)?)
    } else {
        None
    };

    let runtime_manifest_file = if runtime_manifest_clusters > 0 {
        Some(allocate_cluster_chain(
            &mut next_cluster,
            runtime_manifest_clusters,
        )?)
    } else {
        None
    };

    let mut runtime_layouts = Vec::new();
    for runtime in runtime_files.iter() {
        let file_clusters = cluster_count_for_bytes(runtime.content.len(), cluster_size);
        let chain = allocate_cluster_chain(&mut next_cluster, file_clusters)?;
        runtime_layouts.push(RuntimeFileLayout {
            short_name: runtime.short_name,
            bucket: runtime.bucket,
            first_cluster: chain.first_cluster,
            cluster_count: chain.cluster_count,
            size: runtime.content.len() as u32,
        });
    }
    let mut servort_layouts = Vec::new();
    for servort in servort_files.iter() {
        let file_size = servort_file_size_bytes(servort);
        let file_clusters = cluster_count_for_bytes(file_size, cluster_size);
        let chain = allocate_cluster_chain(&mut next_cluster, file_clusters)?;
        servort_layouts.push(ServortFileLayout {
            short_name: servort.short_name,
            first_cluster: chain.first_cluster,
            cluster_count: chain.cluster_count,
            size: file_size as u32,
        });
    }
    progress(14, "ALLOCATING CLUSTER CHAINS");

    if runtime_layouts.len() != runtime_files.len() {
        return Err("RUNTIME FILE LAYOUT COUNT MISMATCH.");
    }
    if servort_layouts.len() != servort_files.len() {
        return Err("SERVORT FILE LAYOUT COUNT MISMATCH.");
    }

    let last_used_cluster = next_cluster
        .checked_sub(1)
        .ok_or("CLUSTER INDEX UNDERFLOW.")?;
    let max_available_cluster = cluster_count as u32 + 1;
    if last_used_cluster > max_available_cluster {
        return Err("NOT ENOUGH SPACE FOR PAYLOAD + LINUXRT + SERVORT.");
    }

    progress(16, "CLEARING TARGET PARTITION");
    clear_reserved_partition_sectors(disk_handle, partition_start_lba, progress, 16, 24)?;
    progress(25, "WRITING FAT32 BOOT SECTORS");
    write_boot_sector(
        disk_handle,
        partition_start_lba,
        total_sectors as u32,
        sectors_per_cluster,
        sectors_per_fat,
    )?;
    write_fsinfo_sector(disk_handle, partition_start_lba + 1)?;
    write_boot_sector_backup(
        disk_handle,
        partition_start_lba + 6,
        total_sectors as u32,
        sectors_per_cluster,
        sectors_per_fat,
    )?;
    write_fsinfo_sector(disk_handle, partition_start_lba + 7)?;

    let fat_start = partition_start_lba + RESERVED_SECTORS as u64;
    let data_start = partition_start_lba + data_start_rel;
    clear_initial_fat_sectors(
        disk_handle,
        fat_start,
        sectors_per_fat,
        max_available_cluster,
        progress,
        26,
        36,
    )?;
    progress(37, "INITIALIZING FAT TABLE");

    write_fat_entry(disk_handle, fat_start, sectors_per_fat, 0, 0x0FFF_FFF8)?;
    write_fat_entry(disk_handle, fat_start, sectors_per_fat, 1, FAT32_EOC)?;
    progress(40, "BUILDING FAT CLUSTER CHAINS");
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        root_dir.first_cluster,
        root_dir.cluster_count,
    )?;
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        efi_dir.first_cluster,
        efi_dir.cluster_count,
    )?;
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        boot_dir.first_cluster,
        boot_dir.cluster_count,
    )?;
    if let Some(dir) = efi_grub_dir {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            dir.first_cluster,
            dir.cluster_count,
        )?;
    }
    progress(48, "FAT CHAINS: SYSTEM DIRECTORIES READY");

    if let Some(dir) = linuxrt_root_dir {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            dir.first_cluster,
            dir.cluster_count,
        )?;
    }
    if let Some(dir) = linuxrt_lib_dir {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            dir.first_cluster,
            dir.cluster_count,
        )?;
    }
    if let Some(dir) = linuxrt_lib64_dir {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            dir.first_cluster,
            dir.cluster_count,
        )?;
    }
    if let Some(dir) = linuxrt_bin_dir {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            dir.first_cluster,
            dir.cluster_count,
        )?;
    }
    if let Some(dir) = linuxrt_etc_dir {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            dir.first_cluster,
            dir.cluster_count,
        )?;
    }
    if let Some(dir) = linuxrt_usr_dir {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            dir.first_cluster,
            dir.cluster_count,
        )?;
    }
    if let Some(dir) = linuxrt_usr_lib_dir {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            dir.first_cluster,
            dir.cluster_count,
        )?;
    }
    if let Some(dir) = linuxrt_usr_lib64_dir {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            dir.first_cluster,
            dir.cluster_count,
        )?;
    }
    if let Some(dir) = linuxrt_usr_bin_dir {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            dir.first_cluster,
            dir.cluster_count,
        )?;
    }
    progress(52, "FAT CHAINS: LINUXRT DIRECTORIES READY");
    if let Some(dir) = servort_root_dir {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            dir.first_cluster,
            dir.cluster_count,
        )?;
        progress(54, "FAT CHAINS: SERVORT DIRECTORY READY");
    }

    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        boot_efi_file.first_cluster,
        boot_efi_file.cluster_count,
    )?;
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        startup_file.first_cluster,
        startup_file.cluster_count,
    )?;
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        config_file.first_cluster,
        config_file.cluster_count,
    )?;
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        readme_file.first_cluster,
        readme_file.cluster_count,
    )?;
    progress(58, "FAT CHAINS: CORE FILES READY");
    if let Some(file) = redux_efi_file {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            file.first_cluster,
            file.cluster_count,
        )?;
    }
    if let Some(file) = grub_cfg_file {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            file.first_cluster,
            file.cluster_count,
        )?;
    }
    if let Some(file) = efi_grub_cfg_file {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            file.first_cluster,
            file.cluster_count,
        )?;
    }
    if let Some(file) = root_grub_cfg_file {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            file.first_cluster,
            file.cluster_count,
        )?;
    }
    if let Some(file) = runtime_manifest_file {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            file.first_cluster,
            file.cluster_count,
        )?;
    }
    let runtime_chain_total = runtime_layouts.len();
    let mut runtime_chain_done = 0usize;
    for runtime in runtime_layouts.iter() {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            runtime.first_cluster,
            runtime.cluster_count,
        )?;
        runtime_chain_done += 1;
        let pct = map_install_progress(60, 68, runtime_chain_done, runtime_chain_total);
        let detail = format!(
            "FAT CHAINS: LINUXRT FILES {}/{}",
            runtime_chain_done, runtime_chain_total
        );
        progress(pct, detail.as_str());
    }
    let servort_chain_total = servort_layouts.len();
    let mut servort_chain_done = 0usize;
    for servort in servort_layouts.iter() {
        write_chain_entries(
            disk_handle,
            fat_start,
            sectors_per_fat,
            servort.first_cluster,
            servort.cluster_count,
        )?;
        servort_chain_done += 1;
        let pct = map_install_progress(68, 69, servort_chain_done, servort_chain_total);
        let detail = format!(
            "FAT CHAINS: SERVORT FILES {}/{}",
            servort_chain_done, servort_chain_total
        );
        progress(pct, detail.as_str());
    }
    progress(69, "PREPARING DIRECTORY TABLES");

    let mut root_entries = Vec::new();
    root_entries.push(DirEntryLayout {
        short_name: *b"EFI        ",
        attr: 0x10,
        first_cluster: efi_dir.first_cluster,
        size: 0,
    });
    root_entries.push(DirEntryLayout {
        short_name: *b"STARTUP NSH",
        attr: 0x20,
        first_cluster: startup_file.first_cluster,
        size: startup_content.len() as u32,
    });
    root_entries.push(DirEntryLayout {
        short_name: *b"ZENOXOS INI",
        attr: 0x20,
        first_cluster: config_file.first_cluster,
        size: config_content.len() as u32,
    });
    root_entries.push(DirEntryLayout {
        short_name: *b"README  TXT",
        attr: 0x20,
        first_cluster: readme_file.first_cluster,
        size: readme_content.len() as u32,
    });
    if let Some(dir) = linuxrt_root_dir {
        root_entries.push(DirEntryLayout {
            short_name: *b"LINUXRT    ",
            attr: 0x10,
            first_cluster: dir.first_cluster,
            size: 0,
        });
    }
    if let Some(dir) = servort_root_dir {
        root_entries.push(DirEntryLayout {
            short_name: *b"SERVORT    ",
            attr: 0x10,
            first_cluster: dir.first_cluster,
            size: 0,
        });
    }
    if let (Some(file), Some(cfg)) = (root_grub_cfg_file, grub_config_payload) {
        root_entries.push(DirEntryLayout {
            short_name: *b"GRUB    CFG",
            attr: 0x20,
            first_cluster: file.first_cluster,
            size: cfg.len() as u32,
        });
    }

    let mut efi_entries = Vec::new();
    efi_entries.push(DirEntryLayout {
        short_name: *b"BOOT       ",
        attr: 0x10,
        first_cluster: boot_dir.first_cluster,
        size: 0,
    });
    if let Some(dir) = efi_grub_dir {
        efi_entries.push(DirEntryLayout {
            short_name: *b"GRUB       ",
            attr: 0x10,
            first_cluster: dir.first_cluster,
            size: 0,
        });
    }
    let mut boot_entries = Vec::new();
    boot_entries.push(DirEntryLayout {
        short_name: *b"BOOTX64 EFI",
        attr: 0x20,
        first_cluster: boot_efi_file.first_cluster,
        size: boot_payload.len() as u32,
    });
    if let Some(file) = redux_efi_file {
        boot_entries.push(DirEntryLayout {
            short_name: *b"REDUX64 EFI",
            attr: 0x20,
            first_cluster: file.first_cluster,
            size: payload.len() as u32,
        });
    }
    if let (Some(file), Some(cfg)) = (grub_cfg_file, grub_config_payload) {
        boot_entries.push(DirEntryLayout {
            short_name: *b"GRUB    CFG",
            attr: 0x20,
            first_cluster: file.first_cluster,
            size: cfg.len() as u32,
        });
    }
    let mut efi_grub_entries = Vec::new();
    if let (Some(file), Some(cfg)) = (efi_grub_cfg_file, grub_config_payload) {
        efi_grub_entries.push(DirEntryLayout {
            short_name: *b"GRUB    CFG",
            attr: 0x20,
            first_cluster: file.first_cluster,
            size: cfg.len() as u32,
        });
    }
    let mut servort_entries = Vec::new();
    for servort in servort_layouts.iter() {
        servort_entries.push(DirEntryLayout {
            short_name: servort.short_name,
            attr: 0x20,
            first_cluster: servort.first_cluster,
            size: servort.size,
        });
    }

    write_directory_chain(
        disk_handle,
        data_start,
        sectors_per_cluster,
        root_dir,
        root_entries.as_slice(),
    )?;
    write_directory_chain(
        disk_handle,
        data_start,
        sectors_per_cluster,
        efi_dir,
        efi_entries.as_slice(),
    )?;
    if let Some(dir) = efi_grub_dir {
        write_directory_chain(
            disk_handle,
            data_start,
            sectors_per_cluster,
            dir,
            efi_grub_entries.as_slice(),
        )?;
    }
    write_directory_chain(
        disk_handle,
        data_start,
        sectors_per_cluster,
        boot_dir,
        boot_entries.as_slice(),
    )?;
    if let Some(dir) = servort_root_dir {
        write_directory_chain(
            disk_handle,
            data_start,
            sectors_per_cluster,
            dir,
            servort_entries.as_slice(),
        )?;
    }
    progress(78, "DIRECTORY STRUCTURES WRITTEN");

    if let (
        Some(rt_root_dir),
        Some(rt_lib_dir),
        Some(rt_lib64_dir),
        Some(rt_bin_dir),
        Some(rt_etc_dir),
        Some(rt_usr_dir),
        Some(rt_usr_lib_dir),
        Some(rt_usr_lib64_dir),
        Some(rt_usr_bin_dir),
    ) = (
        linuxrt_root_dir,
        linuxrt_lib_dir,
        linuxrt_lib64_dir,
        linuxrt_bin_dir,
        linuxrt_etc_dir,
        linuxrt_usr_dir,
        linuxrt_usr_lib_dir,
        linuxrt_usr_lib64_dir,
        linuxrt_usr_bin_dir,
    ) {
        let mut rt_root_entries = Vec::new();
        rt_root_entries.push(DirEntryLayout {
            short_name: *b"LIB        ",
            attr: 0x10,
            first_cluster: rt_lib_dir.first_cluster,
            size: 0,
        });
        rt_root_entries.push(DirEntryLayout {
            short_name: *b"LIB64      ",
            attr: 0x10,
            first_cluster: rt_lib64_dir.first_cluster,
            size: 0,
        });
        rt_root_entries.push(DirEntryLayout {
            short_name: *b"BIN        ",
            attr: 0x10,
            first_cluster: rt_bin_dir.first_cluster,
            size: 0,
        });
        rt_root_entries.push(DirEntryLayout {
            short_name: *b"ETC        ",
            attr: 0x10,
            first_cluster: rt_etc_dir.first_cluster,
            size: 0,
        });
        rt_root_entries.push(DirEntryLayout {
            short_name: *b"USR        ",
            attr: 0x10,
            first_cluster: rt_usr_dir.first_cluster,
            size: 0,
        });
        if let Some(file) = runtime_manifest_file {
            rt_root_entries.push(DirEntryLayout {
                short_name: *b"RTBASE  LST",
                attr: 0x20,
                first_cluster: file.first_cluster,
                size: runtime_manifest_content.len() as u32,
            });
        }

        let usr_entries = [
            DirEntryLayout {
                short_name: *b"LIB        ",
                attr: 0x10,
                first_cluster: rt_usr_lib_dir.first_cluster,
                size: 0,
            },
            DirEntryLayout {
                short_name: *b"LIB64      ",
                attr: 0x10,
                first_cluster: rt_usr_lib64_dir.first_cluster,
                size: 0,
            },
            DirEntryLayout {
                short_name: *b"BIN        ",
                attr: 0x10,
                first_cluster: rt_usr_bin_dir.first_cluster,
                size: 0,
            },
        ];

        let mut rt_lib_entries = Vec::new();
        let mut rt_lib64_entries = Vec::new();
        let mut rt_bin_entries = Vec::new();
        let mut rt_etc_entries = Vec::new();
        let mut rt_usr_lib_entries = Vec::new();
        let mut rt_usr_lib64_entries = Vec::new();
        let mut rt_usr_bin_entries = Vec::new();
        for runtime in runtime_layouts.iter() {
            let entry = DirEntryLayout {
                short_name: runtime.short_name,
                attr: 0x20,
                first_cluster: runtime.first_cluster,
                size: runtime.size,
            };
            match runtime.bucket {
                RuntimeBucket::Lib => rt_lib_entries.push(entry),
                RuntimeBucket::Lib64 => rt_lib64_entries.push(entry),
                RuntimeBucket::UsrLib => rt_usr_lib_entries.push(entry),
                RuntimeBucket::UsrLib64 => rt_usr_lib64_entries.push(entry),
                RuntimeBucket::Bin => rt_bin_entries.push(entry),
                RuntimeBucket::Etc => rt_etc_entries.push(entry),
                RuntimeBucket::UsrBin => rt_usr_bin_entries.push(entry),
            }
        }

        write_directory_chain(
            disk_handle,
            data_start,
            sectors_per_cluster,
            rt_root_dir,
            rt_root_entries.as_slice(),
        )?;
        write_directory_chain(
            disk_handle,
            data_start,
            sectors_per_cluster,
            rt_usr_dir,
            usr_entries.as_slice(),
        )?;
        write_directory_chain(
            disk_handle,
            data_start,
            sectors_per_cluster,
            rt_lib_dir,
            rt_lib_entries.as_slice(),
        )?;
        write_directory_chain(
            disk_handle,
            data_start,
            sectors_per_cluster,
            rt_lib64_dir,
            rt_lib64_entries.as_slice(),
        )?;
        write_directory_chain(
            disk_handle,
            data_start,
            sectors_per_cluster,
            rt_bin_dir,
            rt_bin_entries.as_slice(),
        )?;
        write_directory_chain(
            disk_handle,
            data_start,
            sectors_per_cluster,
            rt_etc_dir,
            rt_etc_entries.as_slice(),
        )?;
        write_directory_chain(
            disk_handle,
            data_start,
            sectors_per_cluster,
            rt_usr_lib_dir,
            rt_usr_lib_entries.as_slice(),
        )?;
        write_directory_chain(
            disk_handle,
            data_start,
            sectors_per_cluster,
            rt_usr_lib64_dir,
            rt_usr_lib64_entries.as_slice(),
        )?;
        write_directory_chain(
            disk_handle,
            data_start,
            sectors_per_cluster,
            rt_usr_bin_dir,
            rt_usr_bin_entries.as_slice(),
        )?;
    }
    progress(84, "LINUXRT DIRECTORIES WRITTEN");

    progress(86, "WRITING FILE DATA");
    write_cluster_chain_data(
        disk_handle,
        data_start,
        sectors_per_cluster,
        boot_efi_file.first_cluster,
        boot_efi_file.cluster_count,
        boot_payload,
    )?;
    progress(88, "WRITING CORE FILES");
    write_cluster_chain_data(
        disk_handle,
        data_start,
        sectors_per_cluster,
        startup_file.first_cluster,
        startup_file.cluster_count,
        startup_content,
    )?;
    write_cluster_chain_data(
        disk_handle,
        data_start,
        sectors_per_cluster,
        config_file.first_cluster,
        config_file.cluster_count,
        config_content.as_slice(),
    )?;
    write_cluster_chain_data(
        disk_handle,
        data_start,
        sectors_per_cluster,
        readme_file.first_cluster,
        readme_file.cluster_count,
        readme_content.as_slice(),
    )?;
    progress(91, "CORE FILES WRITTEN");
    if let Some(file) = redux_efi_file {
        write_cluster_chain_data(
            disk_handle,
            data_start,
            sectors_per_cluster,
            file.first_cluster,
            file.cluster_count,
            payload,
        )?;
    }
    if let (Some(file), Some(cfg)) = (grub_cfg_file, grub_config_payload) {
        write_cluster_chain_data(
            disk_handle,
            data_start,
            sectors_per_cluster,
            file.first_cluster,
            file.cluster_count,
            cfg,
        )?;
    }
    if let (Some(file), Some(cfg)) = (efi_grub_cfg_file, grub_config_payload) {
        write_cluster_chain_data(
            disk_handle,
            data_start,
            sectors_per_cluster,
            file.first_cluster,
            file.cluster_count,
            cfg,
        )?;
    }
    if let (Some(file), Some(cfg)) = (root_grub_cfg_file, grub_config_payload) {
        write_cluster_chain_data(
            disk_handle,
            data_start,
            sectors_per_cluster,
            file.first_cluster,
            file.cluster_count,
            cfg,
        )?;
    }
    if let Some(file) = runtime_manifest_file {
        write_cluster_chain_data(
            disk_handle,
            data_start,
            sectors_per_cluster,
            file.first_cluster,
            file.cluster_count,
            runtime_manifest_content.as_slice(),
        )?;
    }
    let runtime_data_total = runtime_layouts.len();
    let mut runtime_data_done = 0usize;
    let runtime_data_end_pct = if servort_layouts.is_empty() { 99 } else { 97 };
    for (runtime_layout, runtime_file) in runtime_layouts.iter().zip(runtime_files.iter()) {
        write_cluster_chain_data(
            disk_handle,
            data_start,
            sectors_per_cluster,
            runtime_layout.first_cluster,
            runtime_layout.cluster_count,
            runtime_file.content.as_slice(),
        )?;
        runtime_data_done += 1;
        let pct = map_install_progress(92, runtime_data_end_pct, runtime_data_done, runtime_data_total);
        let detail = format!(
            "COPYING LINUXRT FILES {}/{}",
            runtime_data_done, runtime_data_total
        );
        progress(pct, detail.as_str());
    }
    let servort_data_total = servort_layouts.len();
    let mut servort_data_done = 0usize;
    for (servort_layout, servort_file) in servort_layouts.iter().zip(servort_files.iter()) {
        let servort_copy_index = servort_data_done + 1;
        let start_detail = format!(
            "COPYING SERVORT FILES {}/{} (0%)",
            servort_copy_index, servort_data_total
        );
        progress(98, start_detail.as_str());

        if !servort_file.content.is_empty() {
            write_cluster_chain_data(
                disk_handle,
                data_start,
                sectors_per_cluster,
                servort_layout.first_cluster,
                servort_layout.cluster_count,
                servort_file.content.as_slice(),
            )?;
        } else if let Some(source_fat) = servort_file.source_fat {
            let mut last_percent = 255u8;
            let mut source_progress = |copied: usize, total: usize| {
                let pct = if total == 0 {
                    100u8
                } else {
                    ((copied.saturating_mul(100) / total).min(100)) as u8
                };
                if copied < total && pct == last_percent {
                    return;
                }
                last_percent = pct;
                let detail = format!(
                    "COPYING SERVORT FILES {}/{} ({}%)",
                    servort_copy_index, servort_data_total, pct
                );
                progress(98, detail.as_str());
            };
            write_cluster_chain_data_from_fat_source(
                disk_handle,
                data_start,
                sectors_per_cluster,
                servort_layout.first_cluster,
                servort_layout.cluster_count,
                source_fat,
                servort_layout.size as usize,
                &mut source_progress,
            )?;
        } else {
            return Err("SERVORT SOURCE DATA UNAVAILABLE.");
        }
        servort_data_done += 1;
        let pct = map_install_progress(98, 99, servort_data_done, servort_data_total);
        let detail = format!(
            "COPYING SERVORT FILES {}/{}",
            servort_data_done, servort_data_total
        );
        progress(pct, detail.as_str());
    }
    progress(100, "INSTALLATION FINISHED");

    Ok(())
}

fn choose_sectors_per_cluster(total_sectors: u64) -> u8 {
    let total_mib = total_sectors / 2048;
    if total_mib >= 32 * 1024 {
        64
    } else if total_mib >= 16 * 1024 {
        32
    } else if total_mib >= 8 * 1024 {
        16
    } else if total_mib >= 512 {
        8
    } else {
        1
    }
}

fn compute_layout(total_sectors: u64, sectors_per_cluster: u8) -> Result<(u32, u64, u64), &'static str> {
    if sectors_per_cluster == 0 {
        return Err("INVALID CLUSTER SIZE.");
    }

    let mut sectors_per_fat = 1u32;
    for _ in 0..24 {
        let overhead = RESERVED_SECTORS as u64 + (FAT_COUNT as u64 * sectors_per_fat as u64);
        if total_sectors <= overhead {
            return Err("TARGET LAYOUT OVERFLOW.");
        }

        let data_sectors = total_sectors - overhead;
        let clusters = data_sectors / sectors_per_cluster as u64;
        let needed = (((clusters + 2) * 4 + (LOGICAL_SECTOR_SIZE as u64 - 1))
            / LOGICAL_SECTOR_SIZE as u64) as u32;

        if needed == sectors_per_fat {
            let data_start = RESERVED_SECTORS as u64 + (FAT_COUNT as u64 * sectors_per_fat as u64);
            return Ok((sectors_per_fat, data_start, clusters));
        }
        sectors_per_fat = needed.max(1);
    }

    Err("FAILED TO CONVERGE FAT LAYOUT.")
}

fn exfat_choose_sectors_per_cluster_shift(total_sectors: u64) -> u8 {
    let total_mib = total_sectors / 2048;
    if total_mib >= 32 * 1024 {
        7 // 64 KiB
    } else if total_mib >= 1024 {
        6 // 32 KiB
    } else if total_mib >= 256 {
        5 // 16 KiB
    } else {
        3 // 4 KiB
    }
}

fn exfat_compute_layout(
    total_sectors: u64,
    sectors_per_cluster: u32,
) -> Result<(u32, u32, u32, u32), &'static str> {
    if total_sectors < 65_536 || sectors_per_cluster == 0 {
        return Err("EXFAT TARGET TOO SMALL.");
    }

    let fat_offset = 24u32;
    let mut fat_length = 1u32;
    for _ in 0..24 {
        let heap_unaligned = fat_offset
            .checked_add(fat_length)
            .ok_or("EXFAT LAYOUT OVERFLOW.")?;
        let cluster_heap_offset = align_up_u32(heap_unaligned, sectors_per_cluster);
        if total_sectors <= cluster_heap_offset as u64 {
            return Err("EXFAT TARGET LAYOUT OVERFLOW.");
        }
        let cluster_count = ((total_sectors - cluster_heap_offset as u64)
            / sectors_per_cluster as u64) as u32;
        if cluster_count < 16 {
            return Err("EXFAT CLUSTER COUNT TOO SMALL.");
        }
        let needed_fat = (((cluster_count as u64 + 2) * 4 + LOGICAL_SECTOR_SIZE as u64 - 1)
            / LOGICAL_SECTOR_SIZE as u64) as u32;
        if needed_fat == fat_length {
            return Ok((fat_offset, fat_length, cluster_heap_offset, cluster_count));
        }
        fat_length = needed_fat.max(1);
    }

    Err("EXFAT LAYOUT DID NOT CONVERGE.")
}

fn exfat_boot_checksum(sectors: &[[u8; LOGICAL_SECTOR_SIZE]; 11]) -> u32 {
    let mut checksum = 0u32;
    let mut absolute = 0usize;
    for sector in sectors.iter() {
        for byte in sector.iter() {
            if absolute != 106 && absolute != 107 && absolute != 112 {
                checksum = checksum.rotate_right(1).wrapping_add(*byte as u32);
            }
            absolute += 1;
        }
    }
    checksum
}

fn exfat_write_boot_region(
    handle: Handle,
    base_lba: u64,
    partition_offset: u64,
    volume_length: u64,
    fat_offset: u32,
    fat_length: u32,
    cluster_heap_offset: u32,
    cluster_count: u32,
    root_cluster: u32,
    sectors_per_cluster_shift: u8,
    percent_in_use: u8,
) -> Result<(), &'static str> {
    let mut sectors = [[0u8; LOGICAL_SECTOR_SIZE]; 11];
    let boot = &mut sectors[0];
    boot[0] = 0xEB;
    boot[1] = 0x76;
    boot[2] = 0x90;
    boot[3..11].copy_from_slice(b"EXFAT   ");
    boot[0x40..0x48].copy_from_slice(&partition_offset.to_le_bytes());
    boot[0x48..0x50].copy_from_slice(&volume_length.to_le_bytes());
    boot[0x50..0x54].copy_from_slice(&fat_offset.to_le_bytes());
    boot[0x54..0x58].copy_from_slice(&fat_length.to_le_bytes());
    boot[0x58..0x5C].copy_from_slice(&cluster_heap_offset.to_le_bytes());
    boot[0x5C..0x60].copy_from_slice(&cluster_count.to_le_bytes());
    boot[0x60..0x64].copy_from_slice(&root_cluster.to_le_bytes());
    boot[0x64..0x68].copy_from_slice(&0x2026_0605u32.to_le_bytes());
    boot[0x68..0x6A].copy_from_slice(&0x0100u16.to_le_bytes());
    boot[0x6A..0x6C].copy_from_slice(&0u16.to_le_bytes());
    boot[0x6C] = 9;
    boot[0x6D] = sectors_per_cluster_shift;
    boot[0x6E] = 1;
    boot[0x6F] = 0x80;
    boot[0x70] = percent_in_use;

    let mut i = 0usize;
    while i < 11 {
        sectors[i][510] = 0x55;
        sectors[i][511] = 0xAA;
        i += 1;
    }

    let checksum = exfat_boot_checksum(&sectors);
    for (idx, sector) in sectors.iter().enumerate() {
        if !write_sector_to_uefi_handle(handle, base_lba + idx as u64, sector) {
            return Err("EXFAT BOOT REGION WRITE FAILED.");
        }
    }

    let mut checksum_sector = [0u8; LOGICAL_SECTOR_SIZE];
    let mut off = 0usize;
    while off + 4 <= LOGICAL_SECTOR_SIZE {
        checksum_sector[off..off + 4].copy_from_slice(&checksum.to_le_bytes());
        off += 4;
    }
    if !write_sector_to_uefi_handle(handle, base_lba + 11, &checksum_sector) {
        return Err("EXFAT BOOT CHECKSUM WRITE FAILED.");
    }
    Ok(())
}

fn exfat_upcase_checksum(data: &[u8]) -> u32 {
    let mut checksum = 0u32;
    for byte in data.iter() {
        checksum = checksum.rotate_right(1).wrapping_add(*byte as u32);
    }
    checksum
}

fn build_exfat_upcase_table() -> Vec<u8> {
    let mut out = Vec::new();
    out.resize(65_536 * 2, 0);
    let mut code = 0u32;
    while code <= 0xFFFF {
        let mapped = if code >= b'a' as u32 && code <= b'z' as u32 {
            code - 32
        } else {
            code
        } as u16;
        let off = code as usize * 2;
        out[off..off + 2].copy_from_slice(&mapped.to_le_bytes());
        code += 1;
    }
    out
}

fn exfat_name_hash(name: &str) -> u16 {
    let mut hash = 0u16;
    for b in name.bytes() {
        let ch = if b.is_ascii_lowercase() { b.to_ascii_uppercase() } else { b };
        for byte in [ch, 0u8] {
            hash = hash.rotate_right(1).wrapping_add(byte as u16);
        }
    }
    hash
}

fn exfat_entry_set_checksum(entries: &[u8]) -> u16 {
    let mut checksum = 0u16;
    let mut i = 0usize;
    while i < entries.len() {
        if i != 2 && i != 3 {
            checksum = checksum.rotate_right(1).wrapping_add(entries[i] as u16);
        }
        i += 1;
    }
    checksum
}

fn exfat_write_utf16_ascii(dst: &mut [u8], text: &str, max_chars: usize) -> usize {
    let mut count = 0usize;
    for b in text.bytes() {
        if count >= max_chars {
            break;
        }
        let off = count * 2;
        if off + 2 > dst.len() {
            break;
        }
        dst[off] = b;
        dst[off + 1] = 0;
        count += 1;
    }
    count
}

fn exfat_push_directory_entry_set(
    image: &mut [u8],
    entry_index: &mut usize,
    name: &str,
    first_cluster: u32,
    data_length: u64,
) -> Result<(), &'static str> {
    if *entry_index + 3 > image.len() / 32 {
        return Err("EXFAT ROOT DIRECTORY FULL.");
    }

    let base = *entry_index * 32;
    let set = &mut image[base..base + 96];
    set.fill(0);

    set[0] = 0x85;
    set[1] = 2;
    set[4..6].copy_from_slice(&0x10u16.to_le_bytes());

    set[32] = 0xC0;
    set[33] = 0x03; // allocation possible + no FAT chain
    let name_len = name.bytes().count().min(15);
    set[35] = name_len as u8;
    set[36..38].copy_from_slice(&exfat_name_hash(name).to_le_bytes());
    set[40..48].copy_from_slice(&data_length.to_le_bytes());
    set[52..56].copy_from_slice(&first_cluster.to_le_bytes());
    set[56..64].copy_from_slice(&data_length.to_le_bytes());

    set[64] = 0xC1;
    exfat_write_utf16_ascii(&mut set[66..96], name, 15);

    let checksum = exfat_entry_set_checksum(set);
    set[2..4].copy_from_slice(&checksum.to_le_bytes());

    *entry_index += 3;
    Ok(())
}

fn exfat_write_fat_entry(
    handle: Handle,
    fat_start: u64,
    cluster: u32,
    value: u32,
) -> Result<(), &'static str> {
    let fat_offset = cluster as u64 * 4;
    let lba = fat_start + fat_offset / LOGICAL_SECTOR_SIZE as u64;
    let off = (fat_offset % LOGICAL_SECTOR_SIZE as u64) as usize;
    if off + 4 > LOGICAL_SECTOR_SIZE {
        return Err("EXFAT FAT OFFSET INVALID.");
    }

    let mut sector = [0u8; LOGICAL_SECTOR_SIZE];
    if !read_sector_from_uefi_handle(handle, lba, &mut sector) {
        return Err("EXFAT FAT READ FAILED.");
    }
    sector[off..off + 4].copy_from_slice(&value.to_le_bytes());
    if !write_sector_to_uefi_handle(handle, lba, &sector) {
        return Err("EXFAT FAT WRITE FAILED.");
    }
    Ok(())
}

fn format_exfat_partition(
    handle: Handle,
    partition_start_lba: u64,
    partition_total_sectors: u64,
    label: &str,
) -> Result<(), &'static str> {
    if partition_total_sectors < DUAL_MIN_DATA_SECTORS as u64 {
        return Err("EXFAT DATA PARTITION TOO SMALL.");
    }

    let spc_shift = exfat_choose_sectors_per_cluster_shift(partition_total_sectors);
    let sectors_per_cluster = 1u32 << spc_shift;
    let cluster_size = sectors_per_cluster as usize * LOGICAL_SECTOR_SIZE;
    let (fat_offset, fat_length, cluster_heap_offset, cluster_count) =
        exfat_compute_layout(partition_total_sectors, sectors_per_cluster)?;

    let root_cluster = 2u32;
    let bitmap_cluster = 3u32;
    let bitmap_bytes = ((cluster_count as usize) + 7) / 8;
    let bitmap_clusters = cluster_count_for_bytes(bitmap_bytes, cluster_size);
    let upcase_cluster = bitmap_cluster + bitmap_clusters;
    let upcase = build_exfat_upcase_table();
    let upcase_checksum = exfat_upcase_checksum(upcase.as_slice());
    let upcase_clusters = cluster_count_for_bytes(upcase.len(), cluster_size);
    let desktop_cluster = upcase_cluster + upcase_clusters;
    let downloads_cluster = desktop_cluster + 1;
    let documents_cluster = downloads_cluster + 1;
    let images_cluster = documents_cluster + 1;
    let videos_cluster = images_cluster + 1;
    let last_allocated_cluster = videos_cluster;
    if last_allocated_cluster > cluster_count.saturating_add(1) {
        return Err("EXFAT DATA PARTITION HAS TOO FEW CLUSTERS.");
    }

    let used_clusters = last_allocated_cluster.saturating_sub(1);
    let percent_in_use = if cluster_count == 0 {
        0
    } else {
        ((used_clusters as u64 * 100) / cluster_count as u64).min(100) as u8
    };

    let zero = [0u8; LOGICAL_SECTOR_SIZE];
    let clear_sectors = 24u64
        .saturating_add(fat_length as u64)
        .saturating_add((used_clusters as u64 + 2) * sectors_per_cluster as u64);
    let mut i = 0u64;
    while i < clear_sectors.min(partition_total_sectors) {
        if !write_sector_to_uefi_handle(handle, partition_start_lba + i, &zero) {
            return Err("EXFAT CLEAR FAILED.");
        }
        i += 1;
    }

    exfat_write_boot_region(
        handle,
        partition_start_lba,
        partition_start_lba,
        partition_total_sectors,
        fat_offset,
        fat_length,
        cluster_heap_offset,
        cluster_count,
        root_cluster,
        spc_shift,
        percent_in_use,
    )?;
    exfat_write_boot_region(
        handle,
        partition_start_lba + 12,
        partition_start_lba,
        partition_total_sectors,
        fat_offset,
        fat_length,
        cluster_heap_offset,
        cluster_count,
        root_cluster,
        spc_shift,
        percent_in_use,
    )?;

    let fat_start = partition_start_lba + fat_offset as u64;
    let data_start = partition_start_lba + cluster_heap_offset as u64;
    exfat_write_fat_entry(handle, fat_start, 0, 0xFFFF_FFF8)?;
    exfat_write_fat_entry(handle, fat_start, 1, 0xFFFF_FFFF)?;
    let mut cluster = root_cluster;
    while cluster <= last_allocated_cluster {
        exfat_write_fat_entry(handle, fat_start, cluster, 0xFFFF_FFFF)?;
        cluster += 1;
    }

    let mut bitmap = Vec::new();
    bitmap.resize(bitmap_bytes, 0);
    let mut used = 0u32;
    while used < used_clusters {
        let byte_idx = (used / 8) as usize;
        let bit = (used % 8) as u8;
        if byte_idx < bitmap.len() {
            bitmap[byte_idx] |= 1u8 << bit;
        }
        used += 1;
    }
    write_cluster_chain_data(
        handle,
        data_start,
        sectors_per_cluster as u8,
        bitmap_cluster,
        bitmap_clusters,
        bitmap.as_slice(),
    )?;
    write_cluster_chain_data(
        handle,
        data_start,
        sectors_per_cluster as u8,
        upcase_cluster,
        upcase_clusters,
        upcase.as_slice(),
    )?;

    let mut root = Vec::new();
    root.resize(cluster_size, 0);
    let mut entry_idx = 0usize;
    root[entry_idx * 32] = 0x81;
    root[entry_idx * 32 + 20..entry_idx * 32 + 24].copy_from_slice(&bitmap_cluster.to_le_bytes());
    root[entry_idx * 32 + 24..entry_idx * 32 + 32].copy_from_slice(&(bitmap_bytes as u64).to_le_bytes());
    entry_idx += 1;

    root[entry_idx * 32] = 0x82;
    root[entry_idx * 32 + 4..entry_idx * 32 + 8].copy_from_slice(&upcase_checksum.to_le_bytes());
    root[entry_idx * 32 + 20..entry_idx * 32 + 24].copy_from_slice(&upcase_cluster.to_le_bytes());
    root[entry_idx * 32 + 24..entry_idx * 32 + 32].copy_from_slice(&(upcase.len() as u64).to_le_bytes());
    entry_idx += 1;

    let label_len = label.bytes().count().min(11);
    root[entry_idx * 32] = 0x83;
    root[entry_idx * 32 + 1] = label_len as u8;
    exfat_write_utf16_ascii(&mut root[entry_idx * 32 + 2..entry_idx * 32 + 32], label, label_len);
    entry_idx += 1;

    exfat_push_directory_entry_set(
        root.as_mut_slice(),
        &mut entry_idx,
        "Desktop",
        desktop_cluster,
        cluster_size as u64,
    )?;
    exfat_push_directory_entry_set(
        root.as_mut_slice(),
        &mut entry_idx,
        "Downloads",
        downloads_cluster,
        cluster_size as u64,
    )?;
    exfat_push_directory_entry_set(
        root.as_mut_slice(),
        &mut entry_idx,
        "Documents",
        documents_cluster,
        cluster_size as u64,
    )?;
    exfat_push_directory_entry_set(
        root.as_mut_slice(),
        &mut entry_idx,
        "Images",
        images_cluster,
        cluster_size as u64,
    )?;
    exfat_push_directory_entry_set(
        root.as_mut_slice(),
        &mut entry_idx,
        "Videos",
        videos_cluster,
        cluster_size as u64,
    )?;

    write_cluster_chain_data(
        handle,
        data_start,
        sectors_per_cluster as u8,
        root_cluster,
        1,
        root.as_slice(),
    )?;

    let empty_dir = Vec::new();
    write_cluster_chain_data(
        handle,
        data_start,
        sectors_per_cluster as u8,
        desktop_cluster,
        1,
        empty_dir.as_slice(),
    )?;
    write_cluster_chain_data(
        handle,
        data_start,
        sectors_per_cluster as u8,
        downloads_cluster,
        1,
        empty_dir.as_slice(),
    )?;
    write_cluster_chain_data(
        handle,
        data_start,
        sectors_per_cluster as u8,
        documents_cluster,
        1,
        empty_dir.as_slice(),
    )?;
    write_cluster_chain_data(
        handle,
        data_start,
        sectors_per_cluster as u8,
        images_cluster,
        1,
        empty_dir.as_slice(),
    )?;
    write_cluster_chain_data(
        handle,
        data_start,
        sectors_per_cluster as u8,
        videos_cluster,
        1,
        empty_dir.as_slice(),
    )?;

    Ok(())
}

fn cluster_count_for_bytes(len: usize, cluster_size: usize) -> u32 {
    let needed = len.max(1);
    ((needed + cluster_size - 1) / cluster_size) as u32
}

fn clear_reserved_partition_sectors<F>(
    handle: Handle,
    partition_start_lba: u64,
    progress: &mut F,
    start_pct: u8,
    end_pct: u8,
) -> Result<(), &'static str>
where
    F: FnMut(u8, &str),
{
    let zero = [0u8; LOGICAL_SECTOR_SIZE];
    let total = RESERVED_SECTORS as usize;
    let mut last_pct = start_pct;
    progress(last_pct, "CLEARING RESERVED REGION");
    for i in 0..RESERVED_SECTORS as u64 {
        if !write_sector_to_uefi_handle(handle, partition_start_lba + i, &zero) {
            return Err("FAILED TO CLEAR RESERVED REGION.");
        }
        let pct = map_install_progress(start_pct, end_pct, (i + 1) as usize, total);
        if pct != last_pct {
            last_pct = pct;
            progress(last_pct, "CLEARING RESERVED REGION");
        }
    }
    Ok(())
}

fn clear_initial_fat_sectors<F>(
    handle: Handle,
    fat_start: u64,
    sectors_per_fat: u32,
    max_cluster: u32,
    progress: &mut F,
    start_pct: u8,
    end_pct: u8,
) -> Result<(), &'static str>
where
    F: FnMut(u8, &str),
{
    let sectors_to_clear =
        (((max_cluster as u64 + 16) * 4 + (LOGICAL_SECTOR_SIZE as u64 - 1))
            / LOGICAL_SECTOR_SIZE as u64)
            .max(8);
    let max_clear = core::cmp::min(sectors_to_clear, sectors_per_fat as u64);

    let zero = [0u8; LOGICAL_SECTOR_SIZE];
    let total = max_clear as usize * FAT_COUNT as usize;
    let mut done = 0usize;
    let mut last_pct = start_pct;
    progress(last_pct, "CLEARING FAT REGION");
    for fat_copy in 0..FAT_COUNT {
        let base = fat_start + fat_copy as u64 * sectors_per_fat as u64;
        for i in 0..max_clear {
            if !write_sector_to_uefi_handle(handle, base + i, &zero) {
                return Err("FAILED TO CLEAR FAT REGION.");
            }
            done += 1;
            let pct = map_install_progress(start_pct, end_pct, done, total);
            if pct != last_pct {
                last_pct = pct;
                progress(last_pct, "CLEARING FAT REGION");
            }
        }
    }
    Ok(())
}

fn write_chain_entries(
    handle: Handle,
    fat_start: u64,
    sectors_per_fat: u32,
    first_cluster: u32,
    count: u32,
) -> Result<(), &'static str> {
    let mut i = 0u32;
    while i < count {
        let cluster = first_cluster + i;
        let value = if i + 1 == count { FAT32_EOC } else { cluster + 1 };
        write_fat_entry(handle, fat_start, sectors_per_fat, cluster, value)?;
        i += 1;
    }
    Ok(())
}

fn build_directory_cluster_image(
    entries: &[DirEntryLayout],
    chain: ClusterChainLayout,
    cluster_size: usize,
) -> Result<Vec<u8>, &'static str> {
    let total_bytes = (chain.cluster_count as usize)
        .checked_mul(cluster_size)
        .ok_or("DIRECTORY IMAGE SIZE OVERFLOW.")?;
    if total_bytes == 0 || cluster_size < 32 {
        return Err("DIRECTORY IMAGE SIZE INVALID.");
    }

    let capacity = total_bytes / 32;
    if entries.len().saturating_add(1) > capacity {
        return Err("DIRECTORY ENTRY OVERFLOW.");
    }

    let mut image = Vec::new();
    image.resize(total_bytes, 0);

    for (idx, entry) in entries.iter().enumerate() {
        let off = idx * 32;
        write_dir_entry(
            &mut image[off..off + 32],
            entry.short_name,
            entry.attr,
            entry.first_cluster,
            entry.size,
        );
    }

    Ok(image)
}

fn write_directory_chain(
    handle: Handle,
    data_start: u64,
    sectors_per_cluster: u8,
    chain: ClusterChainLayout,
    entries: &[DirEntryLayout],
) -> Result<(), &'static str> {
    let cluster_size = sectors_per_cluster as usize * LOGICAL_SECTOR_SIZE;
    let image = build_directory_cluster_image(entries, chain, cluster_size)?;
    write_cluster_chain_data(
        handle,
        data_start,
        sectors_per_cluster,
        chain.first_cluster,
        chain.cluster_count,
        image.as_slice(),
    )
}

fn write_cluster_chain_data(
    handle: Handle,
    data_start: u64,
    sectors_per_cluster: u8,
    first_cluster: u32,
    clusters: u32,
    content: &[u8],
) -> Result<(), &'static str> {
    let cluster_size = sectors_per_cluster as usize * LOGICAL_SECTOR_SIZE;
    let mut offset = 0usize;

    for i in 0..clusters {
        let cluster = first_cluster + i;
        let remaining = content.len().saturating_sub(offset);
        let chunk_len = core::cmp::min(cluster_size, remaining);
        let chunk = &content[offset..offset + chunk_len];
        write_single_cluster(handle, data_start, sectors_per_cluster, cluster, chunk)?;
        offset += chunk_len;
    }

    Ok(())
}

fn write_cluster_chain_data_from_fat_reader(
    handle: Handle,
    data_start: u64,
    sectors_per_cluster: u8,
    first_cluster: u32,
    clusters: u32,
    source_start_cluster: u32,
    source_size: usize,
    source_fat: &mut crate::fat32::Fat32,
    on_progress: &mut impl FnMut(usize, usize),
) -> Result<(), &'static str> {
    if source_start_cluster < 2 {
        return Err("SERVORT SOURCE CLUSTER INVALID.");
    }
    let cluster_size = sectors_per_cluster as usize * LOGICAL_SECTOR_SIZE;
    let total_capacity = (clusters as usize)
        .checked_mul(cluster_size)
        .ok_or("SERVORT DESTINATION SIZE OVERFLOW.")?;
    if source_size == 0 || source_size > total_capacity {
        return Err("SERVORT SOURCE SIZE MISMATCH.");
    }

    let max_chunk = 32 * 1024 * 1024usize;
    let mut chunk_bytes = core::cmp::min(source_size, max_chunk);
    chunk_bytes = (chunk_bytes / cluster_size).max(1) * cluster_size;
    chunk_bytes = chunk_bytes.clamp(cluster_size, max_chunk);
    let mut chunk = vec![0u8; chunk_bytes];
    let mut partial_cluster = vec![0u8; cluster_size];

    on_progress(0, source_size);
    let mut offset = 0usize;
    while offset < source_size {
        let target = core::cmp::min(chunk.len(), source_size - offset);
        let got = source_fat.read_file_range(
            source_start_cluster,
            source_size,
            offset,
            &mut chunk[..target],
        )?;
        if got == 0 {
            return Err("SERVORT SOURCE READ STALLED.");
        }

        let mut local_off = 0usize;
        while local_off < got {
            let file_off = offset + local_off;
            let cluster_index = file_off / cluster_size;
            if cluster_index >= clusters as usize {
                return Err("SERVORT DESTINATION CLUSTER RANGE EXCEEDED.");
            }
            let cluster = first_cluster + cluster_index as u32;
            let in_cluster_off = file_off % cluster_size;
            let take = core::cmp::min(got - local_off, cluster_size - in_cluster_off);

            if in_cluster_off == 0 && take == cluster_size {
                write_single_cluster(
                    handle,
                    data_start,
                    sectors_per_cluster,
                    cluster,
                    &chunk[local_off..local_off + take],
                )?;
            } else {
                partial_cluster.fill(0);
                partial_cluster[in_cluster_off..in_cluster_off + take]
                    .copy_from_slice(&chunk[local_off..local_off + take]);
                write_single_cluster(
                    handle,
                    data_start,
                    sectors_per_cluster,
                    cluster,
                    partial_cluster.as_slice(),
                )?;
            }

            local_off += take;
        }

        offset += got;
        on_progress(offset, source_size);
    }

    Ok(())
}

fn write_cluster_chain_data_from_fat_source(
    handle: Handle,
    data_start: u64,
    sectors_per_cluster: u8,
    first_cluster: u32,
    clusters: u32,
    source: ServortFatSource,
    source_size: usize,
    on_progress: &mut impl FnMut(usize, usize),
) -> Result<(), &'static str> {
    if let Some(volume_index) = source.volume_index {
        let mut fat = crate::fat32::Fat32::new();
        fat.mount_uefi_fat_volume(volume_index)
            .map_err(|_| "FAILED TO MOUNT SERVORT SOURCE VOLUME.")?;
        return write_cluster_chain_data_from_fat_reader(
            handle,
            data_start,
            sectors_per_cluster,
            first_cluster,
            clusters,
            source.start_cluster,
            source_size,
            &mut fat,
            on_progress,
        );
    }

    let fat = unsafe { &mut crate::fat32::GLOBAL_FAT };
    if !fat.init() {
        return Err("FAILED TO INIT GLOBAL FAT FOR SERVORT SOURCE.");
    }
    write_cluster_chain_data_from_fat_reader(
        handle,
        data_start,
        sectors_per_cluster,
        first_cluster,
        clusters,
        source.start_cluster,
        source_size,
        fat,
        on_progress,
    )
}

fn write_single_cluster(
    handle: Handle,
    data_start: u64,
    sectors_per_cluster: u8,
    cluster: u32,
    content: &[u8],
) -> Result<(), &'static str> {
    if cluster < 2 {
        return Err("INVALID CLUSTER.");
    }

    let cluster_lba = data_start + (cluster as u64 - 2) * sectors_per_cluster as u64;
    for sec in 0..sectors_per_cluster as usize {
        let mut sector = [0u8; LOGICAL_SECTOR_SIZE];
        let src_start = sec * LOGICAL_SECTOR_SIZE;
        if src_start < content.len() {
            let src_end = core::cmp::min(src_start + LOGICAL_SECTOR_SIZE, content.len());
            let len = src_end - src_start;
            sector[0..len].copy_from_slice(&content[src_start..src_end]);
        }

        if !write_sector_to_uefi_handle(handle, cluster_lba + sec as u64, &sector) {
            return Err("CLUSTER WRITE FAILED.");
        }
    }

    Ok(())
}

fn write_dir_entry(entry: &mut [u8], short_name: [u8; 11], attr: u8, cluster: u32, size: u32) {
    if entry.len() < 32 {
        return;
    }

    for b in entry.iter_mut() {
        *b = 0;
    }
    entry[0..11].copy_from_slice(&short_name);
    entry[11] = attr;

    let hi = ((cluster >> 16) & 0xFFFF) as u16;
    let lo = (cluster & 0xFFFF) as u16;
    entry[20..22].copy_from_slice(&hi.to_le_bytes());
    entry[26..28].copy_from_slice(&lo.to_le_bytes());
    entry[28..32].copy_from_slice(&size.to_le_bytes());
}

fn write_fat_entry(
    handle: Handle,
    fat_start: u64,
    sectors_per_fat: u32,
    cluster: u32,
    value: u32,
) -> Result<(), &'static str> {
    let fat_offset = cluster as u64 * 4;
    let rel_lba = fat_offset / LOGICAL_SECTOR_SIZE as u64;
    let off = (fat_offset % LOGICAL_SECTOR_SIZE as u64) as usize;
    if off + 4 > LOGICAL_SECTOR_SIZE {
        return Err("FAT OFFSET OUT OF RANGE.");
    }

    for fat_copy in 0..FAT_COUNT {
        let lba = fat_start + fat_copy as u64 * sectors_per_fat as u64 + rel_lba;
        let mut sector = [0u8; LOGICAL_SECTOR_SIZE];
        if !read_sector_from_uefi_handle(handle, lba, &mut sector) {
            return Err("FAT READ FAILED.");
        }

        let mut prev = [0u8; 4];
        prev.copy_from_slice(&sector[off..off + 4]);
        let old = u32::from_le_bytes(prev);
        let new = (old & 0xF000_0000) | (value & 0x0FFF_FFFF);
        sector[off..off + 4].copy_from_slice(&new.to_le_bytes());

        if !write_sector_to_uefi_handle(handle, lba, &sector) {
            return Err("FAT WRITE FAILED.");
        }
    }

    Ok(())
}

fn write_boot_sector(
    handle: Handle,
    lba: u64,
    total_sectors_32: u32,
    sectors_per_cluster: u8,
    sectors_per_fat_32: u32,
) -> Result<(), &'static str> {
    write_boot_sector_backup(
        handle,
        lba,
        total_sectors_32,
        sectors_per_cluster,
        sectors_per_fat_32,
    )
}

fn write_boot_sector_backup(
    handle: Handle,
    lba: u64,
    total_sectors_32: u32,
    sectors_per_cluster: u8,
    sectors_per_fat_32: u32,
) -> Result<(), &'static str> {
    let mut sector = [0u8; LOGICAL_SECTOR_SIZE];
    sector[0] = 0xEB;
    sector[1] = 0x58;
    sector[2] = 0x90;
    sector[3..11].copy_from_slice(b"ZENOXOS ");
    sector[11..13].copy_from_slice(&(LOGICAL_SECTOR_SIZE as u16).to_le_bytes());
    sector[13] = sectors_per_cluster;
    sector[14..16].copy_from_slice(&RESERVED_SECTORS.to_le_bytes());
    sector[16] = FAT_COUNT;
    sector[17..19].copy_from_slice(&0u16.to_le_bytes());
    sector[19..21].copy_from_slice(&0u16.to_le_bytes());
    sector[21] = 0xF8;
    sector[22..24].copy_from_slice(&0u16.to_le_bytes());
    sector[24..26].copy_from_slice(&63u16.to_le_bytes());
    sector[26..28].copy_from_slice(&255u16.to_le_bytes());
    sector[28..32].copy_from_slice(&0u32.to_le_bytes());
    sector[32..36].copy_from_slice(&total_sectors_32.to_le_bytes());
    sector[36..40].copy_from_slice(&sectors_per_fat_32.to_le_bytes());
    sector[40..42].copy_from_slice(&0u16.to_le_bytes());
    sector[42..44].copy_from_slice(&0u16.to_le_bytes());
    sector[44..48].copy_from_slice(&ROOT_CLUSTER.to_le_bytes());
    sector[48..50].copy_from_slice(&1u16.to_le_bytes());
    sector[50..52].copy_from_slice(&6u16.to_le_bytes());
    sector[64] = 0x80;
    sector[66] = 0x29;
    sector[67..71].copy_from_slice(&0x2026_0217u32.to_le_bytes());
    sector[71..82].copy_from_slice(b"ZENOX OS   ");
    sector[82..90].copy_from_slice(b"FAT32   ");
    sector[510] = 0x55;
    sector[511] = 0xAA;

    if !write_sector_to_uefi_handle(handle, lba, &sector) {
        return Err("BOOT SECTOR WRITE FAILED.");
    }
    Ok(())
}

fn write_fsinfo_sector(handle: Handle, lba: u64) -> Result<(), &'static str> {
    let mut fsinfo = [0u8; LOGICAL_SECTOR_SIZE];
    fsinfo[0..4].copy_from_slice(&0x4161_5252u32.to_le_bytes());
    fsinfo[484..488].copy_from_slice(&0x6141_7272u32.to_le_bytes());
    fsinfo[488..492].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    fsinfo[492..496].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    fsinfo[510] = 0x55;
    fsinfo[511] = 0xAA;

    if !write_sector_to_uefi_handle(handle, lba, &fsinfo) {
        return Err("FSINFO WRITE FAILED.");
    }
    Ok(())
}

fn read_sector_from_uefi_handle(
    handle: Handle,
    lba: u64,
    buffer: &mut [u8; LOGICAL_SECTOR_SIZE],
) -> bool {
    let params = OpenProtocolParams {
        handle,
        agent: boot::image_handle(),
        controller: None,
    };

    let blk = match unsafe { boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol) } {
        Ok(p) => p,
        Err(_) => return false,
    };

    let (media_id, last_block, block_size) = {
        let media = blk.media();
        if !media.is_media_present() {
            return false;
        }
        (media.media_id(), media.last_block(), media.block_size() as usize)
    };

    if block_size < LOGICAL_SECTOR_SIZE
        || block_size > MAX_UEFI_BLOCK_SIZE
        || (block_size % LOGICAL_SECTOR_SIZE) != 0
    {
        return false;
    }

    let byte_offset = match lba.checked_mul(LOGICAL_SECTOR_SIZE as u64) {
        Some(v) => v,
        None => return false,
    };
    let block_lba = byte_offset / block_size as u64;
    let offset = (byte_offset % block_size as u64) as usize;
    if block_lba > last_block {
        return false;
    }

    let mut scratch = [0u8; MAX_UEFI_BLOCK_SIZE];
    if blk.read_blocks(media_id, block_lba, &mut scratch[0..block_size]).is_err() {
        return false;
    }
    buffer.copy_from_slice(&scratch[offset..offset + LOGICAL_SECTOR_SIZE]);
    true
}

fn write_sector_to_uefi_handle(handle: Handle, lba: u64, buffer: &[u8; LOGICAL_SECTOR_SIZE]) -> bool {
    let params = OpenProtocolParams {
        handle,
        agent: boot::image_handle(),
        controller: None,
    };

    let mut blk = match unsafe { boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol) } {
        Ok(p) => p,
        Err(_) => return false,
    };

    let (media_id, last_block, block_size) = {
        let media = blk.media();
        if !media.is_media_present() {
            return false;
        }
        (media.media_id(), media.last_block(), media.block_size() as usize)
    };

    if block_size < LOGICAL_SECTOR_SIZE
        || block_size > MAX_UEFI_BLOCK_SIZE
        || (block_size % LOGICAL_SECTOR_SIZE) != 0
    {
        return false;
    }

    let byte_offset = match lba.checked_mul(LOGICAL_SECTOR_SIZE as u64) {
        Some(v) => v,
        None => return false,
    };
    let block_lba = byte_offset / block_size as u64;
    let offset = (byte_offset % block_size as u64) as usize;
    if block_lba > last_block {
        return false;
    }

    let mut scratch = [0u8; MAX_UEFI_BLOCK_SIZE];
    if block_size != LOGICAL_SECTOR_SIZE || offset != 0 {
        if blk.read_blocks(media_id, block_lba, &mut scratch[0..block_size]).is_err() {
            return false;
        }
    }
    scratch[offset..offset + LOGICAL_SECTOR_SIZE].copy_from_slice(buffer);

    blk.write_blocks(media_id, block_lba, &scratch[0..block_size]).is_ok()
}

fn capture_framebuffer_info() -> Option<FramebufferInfo> {
    let handle = boot::get_handle_for_protocol::<GraphicsOutput>().ok()?;
    let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(handle).ok()?;

    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    let stride = mode.stride();
    let layout = match mode.pixel_format() {
        PixelFormat::Rgb => PixelLayout::Rgb,
        PixelFormat::Bgr => PixelLayout::Bgr,
        _ => PixelLayout::Unknown,
    };

    let mut fb = gop.frame_buffer();
    let base = fb.as_mut_ptr();
    let size = fb.size();

    Some(FramebufferInfo {
        base,
        size,
        width,
        height,
        stride,
        layout,
    })
}
