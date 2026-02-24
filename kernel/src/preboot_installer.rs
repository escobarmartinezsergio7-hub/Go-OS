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
const RUNTIME_COPY_MAX_BYTES: usize = 256 * 1024 * 1024;
const PAYLOAD_MAX_BYTES: usize = 256 * 1024 * 1024;
const EMBEDDED_LINUXRT_BUNDLE_MAGIC: &[u8; 6] = b"RLTB1\0";
const EMBEDDED_LINUXRT_BUNDLE: &[u8] = include_bytes!(env!("REDUX_LINUXRT_BUNDLE"));
const DEFAULT_APP_MAIN_RML: &[u8] = include_bytes!("../../apps/hello_redux/main.rml");
const DEFAULT_APP_MAIN_RDX: &[u8] = include_bytes!("../../apps/hello_redux/main.rdx");
const STATUS_OK: u32 = 0x9DE5A8;
const STATUS_WARN: u32 = 0xFFD48A;
const STATUS_ERR: u32 = 0xFF9F9F;
const GPT_MAX_ENTRY_BYTES: usize = 8 * 1024 * 1024;
const GPT_BASIC_DATA_TYPE_GUID_LE: [u8; 16] = [
    0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99,
    0xC7,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum RuntimeBucket {
    Lib,
    Lib64,
    UsrLib,
    UsrLib64,
}

#[derive(Clone)]
struct RuntimeInstallFile {
    short_name: [u8; 11],
    source_path: String,
    bucket: RuntimeBucket,
    content: Vec<u8>,
}

#[derive(Clone, Copy)]
struct RuntimeFileLayout {
    short_name: [u8; 11],
    bucket: RuntimeBucket,
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
    let runtime_total_bytes = runtime_files
        .iter()
        .fold(0usize, |acc, file| acc.saturating_add(file.content.len()));
    crate::println("Preboot installer: runtime files loaded (fast)");
    crate::println_num(runtime_files.len() as u64);
    crate::println("Preboot installer: runtime bytes loaded");
    crate::println_num(runtime_total_bytes as u64);
    draw_bootstrap_progress("LOADING LINUXRT", 72, "BUNDLE PARSED");

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

    let mut status = if let Some(err) = runtime_error.as_ref() {
        format!("ERROR: LINUXRT BUNDLE FAILED: {}", err)
    } else if runtime_files.is_empty() {
        String::from("ERROR: LINUXRT NOT EMBEDDED IN BUILD. RUN MAKE UEFI. INSTALL BLOCKED.")
    } else if disks.is_empty() {
        String::from("NO INTERNAL DISKS DETECTED. PRESS ESC TO SKIP.")
    } else if targets.is_empty() {
        String::from("NO PARTITIONS. PRESS C TO CREATE ONE IN FREE SPACE.")
    } else {
        let boot_manager = if grub_enabled { "GRUB" } else { "REDUXEFI" };
        format!(
            "READY. PAYLOAD {} KB. LINUXRT {} FILES. BOOTMGR {}. SELECT TARGET + ENTER.",
            payload.len() / 1024,
            runtime_files.len(),
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
                        if !targets.is_empty() {
                            selected = (selected + 1) % targets.len();
                            status = format!("SELECTED TARGET {}.", selected + 1);
                            status_color = STATUS_WARN;
                        }
                    }
                    'p' | 'P' => {
                        armed = false;
                        if !targets.is_empty() {
                            selected = if selected == 0 { targets.len() - 1 } else { selected - 1 };
                            status = format!("SELECTED TARGET {}.", selected + 1);
                            status_color = STATUS_WARN;
                        }
                    }
                    '1'..='9' => {
                        armed = false;
                        let idx = (ch as u8 - b'1') as usize;
                        if idx < targets.len() {
                            selected = idx;
                            status = format!("SELECTED TARGET {}.", selected + 1);
                            status_color = STATUS_WARN;
                        }
                    }
                    '+' | '=' => {
                        armed = false;
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

                        let disk_idx = current_disk_index(&targets, selected, &disks);
                        match create_partition_on_disk(&mut disks, disk_idx) {
                            Ok(msg) => {
                                status = msg;
                                status_color = STATUS_OK;
                                targets = collect_targets(&disks);
                                selected = targets.len().saturating_sub(1);
                            }
                            Err(err) => {
                                status = String::from(err);
                                status_color = STATUS_ERR;
                            }
                        }
                    }
                    'r' | 'R' => {
                        armed = false;
                        draw_bootstrap_progress("RESCAN", 35, "REFRESHING DISKS + LINUXRT");
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
                        refresh_selection(&mut selected, targets.len());

                        status = if let Some(err) = runtime_error.as_ref() {
                            format!("ERROR: LINUXRT BUNDLE FAILED: {}", err)
                        } else if runtime_files.is_empty() {
                            String::from("ERROR: LINUXRT NOT EMBEDDED IN BUILD.")
                        } else if disks.is_empty() {
                            String::from("NO INTERNAL DISKS DETECTED.")
                        } else if targets.is_empty() {
                            String::from("RELOADED. NO PARTITIONS; PRESS C TO CREATE.")
                        } else {
                            String::from("RELOADED DISK + PARTITION TABLES.")
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

                    if !armed {
                        armed = true;
                        status = format!(
                            "DANGER: ENTER AGAIN TO FACTORY-RESET TARGET {} AND INSTALL.",
                            selected + 1
                        );
                        status_color = STATUS_ERR;
                        continue;
                    }

                    let target = targets[selected];
                    let disk = &disks[target.disk_idx];
                    let part = disk.partitions[target.part_idx];


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
                        payload.as_slice(),
                        grub_assets.as_ref(),
                        runtime_files.as_slice(),
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
                                    "INSTALL COMPLETE. BOOT + SYSTEM + LINUXRT COPIED ({} FILES).",
                                    runtime_files.len()
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
        "REDUXOS PREBOOT INSTALLER",
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
        "REDUXOS GRAPHICAL INSTALLER (PRE-BOOT)",
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
                "NO PARTITIONS FOUND. PRESS C TO CREATE IN FREE SPACE.",
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
            let line = match disk.scheme {
                PartitionScheme::Mbr => format!(
                    "{}. DISK {} PART {}  TYPE {:02X}  START {}  SIZE {} MIB",
                    i + 1,
                    tref.disk_idx + 1,
                    tref.part_idx + 1,
                    part.part_type,
                    part.start_lba,
                    mib
                ),
                PartitionScheme::Gpt => format!(
                    "{}. DISK {} PART {}  TYPE GPT  START {}  SIZE {} MIB",
                    i + 1,
                    tref.disk_idx + 1,
                    tref.part_idx + 1,
                    part.start_lba,
                    mib
                ),
            };
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
        "N/P MOVE  +/- RESIZE  C CREATE  R RELOAD  1-9 SELECT  ENTER INSTALL  ESC SKIP",
        rgb(191, 209, 236),
    );
    framebuffer::draw_text_5x7(
        panel_x + 12,
        help_y + 14,
        "GPT+MBR INSTALL + RESIZE + CREATE. STEP = 128 MIB.",
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

    let boot_device = current_boot_device_handle();
    let mut best_non_boot: Option<(Handle, usize, u64)> = None;
    let mut best_boot: Option<(Handle, usize, u64)> = None;

    for handle in handles.iter().copied() {
        let Some((block_size, total_logical_sectors)) = describe_physical_disk(handle) else {
            continue;
        };

        let is_boot_device = boot_device == Some(handle);
        let slot = if is_boot_device {
            &mut best_boot
        } else {
            &mut best_non_boot
        };

        let should_replace = match slot {
            Some((_, _, prev_total)) => total_logical_sectors > *prev_total,
            None => true,
        };
        if should_replace {
            *slot = Some((handle, block_size, total_logical_sectors));
        }
    }

    let selected = best_non_boot.or(best_boot);
    let Some((handle, block_size, total_logical_sectors)) = selected else {
        return out;
    };

    crate::println("Preboot installer: selected disk sectors");
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
        return out;
    }

    out.push(InternalDisk {
        handle,
        block_size,
        total_logical_sectors,
        scheme: PartitionScheme::Mbr,
        partitions: mbr_partitions.to_vec(),
    });
    crate::println("Preboot installer: accepted disk");

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

    entry[0..16].copy_from_slice(&GPT_BASIC_DATA_TYPE_GUID_LE);

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
    gpt_write_name(entry, "REDUXOS");

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

        out.push(MbrPartition {
            boot: 0,
            part_type: 0xEE,
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

fn create_partition_on_disk(disks: &mut [InternalDisk], disk_idx: usize) -> Result<String, &'static str> {
    if disk_idx >= disks.len() {
        return Err("INVALID DISK INDEX.");
    }

    let mut disk = disks[disk_idx].clone();
    if disk.scheme != PartitionScheme::Mbr {
        return Err("GPT CREATE NOT IMPLEMENTED YET. INSTALL TO EXISTING GPT PARTITIONS.");
    }

    let mut empty_slot = None;
    let mut i = 0usize;
    while i < disk.partitions.len() {
        if !disk.partitions[i].is_used() {
            empty_slot = Some(i);
            break;
        }
        i += 1;
    }

    let Some(slot) = empty_slot else {
        return Err("NO FREE MBR SLOT. MAX 4 PRIMARY PARTITIONS.");
    };

    let (gap_start, gap_end) = find_largest_free_gap(&disk)?;
    let aligned_start = align_up_u32(gap_start, 2048);
    if aligned_start >= gap_end {
        return Err("FREE GAP TOO SMALL AFTER ALIGNMENT.");
    }

    let size = gap_end - aligned_start;
    if size < MIN_INSTALL_SECTORS {
        return Err("LARGEST FREE SPACE IS SMALLER THAN 64 MIB.");
    }

    disk.partitions[slot] = MbrPartition {
        boot: 0,
        part_type: 0x0C,
        start_lba: aligned_start,
        total_sectors: size,
    };

    let mbr = mbr_slots_from_disk(&disk)?;
    write_mbr_table(disk.handle, &mbr)?;
    disks[disk_idx] = disk;

    Ok(format!(
        "CREATE OK: DISK {} PART {} CREATED ({} MIB).",
        disk_idx + 1,
        slot + 1,
        size as u64 / 2048
    ))
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
    } else if normalized.starts_with("lib64/") || normalized.contains("/lib64/") {
        RuntimeBucket::Lib64
    } else {
        RuntimeBucket::Lib
    }
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
        RuntimeBucket::UsrLib => {
            let usr = runtime_find_dir_on_global_fat(fat, linuxrt_root, "USR")?;
            runtime_find_dir_on_global_fat(fat, usr, "LIB")
        }
        RuntimeBucket::UsrLib64 => {
            let usr = runtime_find_dir_on_global_fat(fat, linuxrt_root, "USR")?;
            runtime_find_dir_on_global_fat(fat, usr, "LIB64")
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
menuentry \"ReduxOS\" {\r\n\
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
    payload: &[u8],
    grub_assets: Option<&GrubInstallAssets>,
    runtime_files: &[RuntimeInstallFile],
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
    let grub_enabled = grub_assets.is_some();
    let boot_payload = if let Some(grub) = grub_assets {
        grub.efi_payload.as_slice()
    } else {
        payload
    };
    let grub_config_payload = grub_assets.map(|grub| grub.config_payload.as_slice());

    let startup_content = b"\\EFI\\BOOT\\BOOTX64.EFI\r\n";
    let config_content =
        b"[reduxos]\r\ninstalled=1\r\nautoboot=gui\r\nboot_efi=\\EFI\\BOOT\\BOOTX64.EFI\r\n";
    let readme_content = if grub_enabled {
        b"ReduxOS installed on internal storage.\r\nBoot manager: GRUB.\r\nBoot path: \\EFI\\BOOT\\BOOTX64.EFI\r\n"
            .as_slice()
    } else {
        b"ReduxOS installed on internal storage.\r\nBoot path: \\EFI\\BOOT\\BOOTX64.EFI\r\n"
            .as_slice()
    };
    let app_main_rml_content = DEFAULT_APP_MAIN_RML;
    let app_main_rdx_content = DEFAULT_APP_MAIN_RDX;

    let mut runtime_lib_count = 0usize;
    let mut runtime_lib64_count = 0usize;
    let mut runtime_usr_lib_count = 0usize;
    let mut runtime_usr_lib64_count = 0usize;
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
        }
    }
    progress(10, "ANALYZING RUNTIME FILES");

    let runtime_manifest_content = build_runtime_manifest_content(runtime_files);
    let runtime_manifest_clusters = if runtime_manifest_content.is_empty() {
        0
    } else {
        cluster_count_for_bytes(runtime_manifest_content.len(), cluster_size)
    };

    let root_dir_entries = 11usize + if runtime_enabled { 1 } else { 0 } + if grub_enabled { 1 } else { 0 };
    let efi_dir_entries = 1usize + if grub_enabled { 1 } else { 0 };
    let efi_grub_dir_entries = if grub_enabled { 1usize } else { 0usize };
    let boot_dir_entries = 1usize + if grub_enabled { 2 } else { 0 };
    let quick_access_dir_entries = 0usize;
    let linuxrt_root_dir_entries = if runtime_enabled { 4usize } else { 0usize };
    let linuxrt_usr_dir_entries = if runtime_enabled { 2usize } else { 0usize };

    let root_dir_clusters = dir_cluster_count_for_entries(root_dir_entries, cluster_size)?;
    let efi_dir_clusters = dir_cluster_count_for_entries(efi_dir_entries, cluster_size)?;
    let efi_grub_dir_clusters = if grub_enabled {
        dir_cluster_count_for_entries(efi_grub_dir_entries, cluster_size)?
    } else {
        0
    };
    let boot_dir_clusters = dir_cluster_count_for_entries(boot_dir_entries, cluster_size)?;
    let quick_access_dir_clusters =
        dir_cluster_count_for_entries(quick_access_dir_entries, cluster_size)?;
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

    let mut next_cluster = ROOT_CLUSTER;
    let root_dir = allocate_cluster_chain(&mut next_cluster, root_dir_clusters)?;
    let efi_dir = allocate_cluster_chain(&mut next_cluster, efi_dir_clusters)?;
    let efi_grub_dir = if grub_enabled {
        Some(allocate_cluster_chain(&mut next_cluster, efi_grub_dir_clusters)?)
    } else {
        None
    };
    let boot_dir = allocate_cluster_chain(&mut next_cluster, boot_dir_clusters)?;
    let desktop_dir = allocate_cluster_chain(&mut next_cluster, quick_access_dir_clusters)?;
    let downloads_dir = allocate_cluster_chain(&mut next_cluster, quick_access_dir_clusters)?;
    let documents_dir = allocate_cluster_chain(&mut next_cluster, quick_access_dir_clusters)?;
    let images_dir = allocate_cluster_chain(&mut next_cluster, quick_access_dir_clusters)?;
    let videos_dir = allocate_cluster_chain(&mut next_cluster, quick_access_dir_clusters)?;

    let mut linuxrt_root_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_lib_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_lib64_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_usr_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_usr_lib_dir: Option<ClusterChainLayout> = None;
    let mut linuxrt_usr_lib64_dir: Option<ClusterChainLayout> = None;
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
    }

    let boot_efi_clusters = cluster_count_for_bytes(boot_payload.len(), cluster_size);
    let startup_clusters = cluster_count_for_bytes(startup_content.len(), cluster_size);
    let config_clusters = cluster_count_for_bytes(config_content.len(), cluster_size);
    let readme_clusters = cluster_count_for_bytes(readme_content.len(), cluster_size);
    let app_main_rml_clusters = cluster_count_for_bytes(app_main_rml_content.len(), cluster_size);
    let app_main_rdx_clusters = cluster_count_for_bytes(app_main_rdx_content.len(), cluster_size);
    let boot_efi_file = allocate_cluster_chain(&mut next_cluster, boot_efi_clusters)?;
    let startup_file = allocate_cluster_chain(&mut next_cluster, startup_clusters)?;
    let config_file = allocate_cluster_chain(&mut next_cluster, config_clusters)?;
    let readme_file = allocate_cluster_chain(&mut next_cluster, readme_clusters)?;
    let app_main_rml_file = allocate_cluster_chain(&mut next_cluster, app_main_rml_clusters)?;
    let app_main_rdx_file = allocate_cluster_chain(&mut next_cluster, app_main_rdx_clusters)?;
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
    progress(14, "ALLOCATING CLUSTER CHAINS");

    if runtime_layouts.len() != runtime_files.len() {
        return Err("RUNTIME FILE LAYOUT COUNT MISMATCH.");
    }

    let last_used_cluster = next_cluster
        .checked_sub(1)
        .ok_or("CLUSTER INDEX UNDERFLOW.")?;
    let max_available_cluster = cluster_count as u32 + 1;
    if last_used_cluster > max_available_cluster {
        return Err("NOT ENOUGH SPACE FOR PAYLOAD + LINUXRT.");
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
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        desktop_dir.first_cluster,
        desktop_dir.cluster_count,
    )?;
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        downloads_dir.first_cluster,
        downloads_dir.cluster_count,
    )?;
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        documents_dir.first_cluster,
        documents_dir.cluster_count,
    )?;
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        images_dir.first_cluster,
        images_dir.cluster_count,
    )?;
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        videos_dir.first_cluster,
        videos_dir.cluster_count,
    )?;
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
    progress(52, "FAT CHAINS: LINUXRT DIRECTORIES READY");

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
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        app_main_rml_file.first_cluster,
        app_main_rml_file.cluster_count,
    )?;
    write_chain_entries(
        disk_handle,
        fat_start,
        sectors_per_fat,
        app_main_rdx_file.first_cluster,
        app_main_rdx_file.cluster_count,
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
        short_name: *b"REDUXOS INI",
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
    root_entries.push(DirEntryLayout {
        short_name: *b"DESKTOP    ",
        attr: 0x10,
        first_cluster: desktop_dir.first_cluster,
        size: 0,
    });
    root_entries.push(DirEntryLayout {
        short_name: *b"DOWNLOAD   ",
        attr: 0x10,
        first_cluster: downloads_dir.first_cluster,
        size: 0,
    });
    root_entries.push(DirEntryLayout {
        short_name: *b"DOCUMENT   ",
        attr: 0x10,
        first_cluster: documents_dir.first_cluster,
        size: 0,
    });
    root_entries.push(DirEntryLayout {
        short_name: *b"IMAGES     ",
        attr: 0x10,
        first_cluster: images_dir.first_cluster,
        size: 0,
    });
    root_entries.push(DirEntryLayout {
        short_name: *b"VIDEOS     ",
        attr: 0x10,
        first_cluster: videos_dir.first_cluster,
        size: 0,
    });
    root_entries.push(DirEntryLayout {
        short_name: *b"MAIN    RML",
        attr: 0x20,
        first_cluster: app_main_rml_file.first_cluster,
        size: app_main_rml_content.len() as u32,
    });
    root_entries.push(DirEntryLayout {
        short_name: *b"MAIN    RDX",
        attr: 0x20,
        first_cluster: app_main_rdx_file.first_cluster,
        size: app_main_rdx_content.len() as u32,
    });
    if let Some(dir) = linuxrt_root_dir {
        root_entries.push(DirEntryLayout {
            short_name: *b"LINUXRT    ",
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
    let empty_entries: [DirEntryLayout; 0] = [];

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
    write_directory_chain(
        disk_handle,
        data_start,
        sectors_per_cluster,
        desktop_dir,
        empty_entries.as_slice(),
    )?;
    write_directory_chain(
        disk_handle,
        data_start,
        sectors_per_cluster,
        downloads_dir,
        empty_entries.as_slice(),
    )?;
    write_directory_chain(
        disk_handle,
        data_start,
        sectors_per_cluster,
        documents_dir,
        empty_entries.as_slice(),
    )?;
    write_directory_chain(
        disk_handle,
        data_start,
        sectors_per_cluster,
        images_dir,
        empty_entries.as_slice(),
    )?;
    write_directory_chain(
        disk_handle,
        data_start,
        sectors_per_cluster,
        videos_dir,
        empty_entries.as_slice(),
    )?;
    progress(78, "DIRECTORY STRUCTURES WRITTEN");

    if let (
        Some(rt_root_dir),
        Some(rt_lib_dir),
        Some(rt_lib64_dir),
        Some(rt_usr_dir),
        Some(rt_usr_lib_dir),
        Some(rt_usr_lib64_dir),
    ) = (
        linuxrt_root_dir,
        linuxrt_lib_dir,
        linuxrt_lib64_dir,
        linuxrt_usr_dir,
        linuxrt_usr_lib_dir,
        linuxrt_usr_lib64_dir,
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
        ];

        let mut rt_lib_entries = Vec::new();
        let mut rt_lib64_entries = Vec::new();
        let mut rt_usr_lib_entries = Vec::new();
        let mut rt_usr_lib64_entries = Vec::new();
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
        config_content,
    )?;
    write_cluster_chain_data(
        disk_handle,
        data_start,
        sectors_per_cluster,
        readme_file.first_cluster,
        readme_file.cluster_count,
        readme_content,
    )?;
    write_cluster_chain_data(
        disk_handle,
        data_start,
        sectors_per_cluster,
        app_main_rml_file.first_cluster,
        app_main_rml_file.cluster_count,
        app_main_rml_content,
    )?;
    write_cluster_chain_data(
        disk_handle,
        data_start,
        sectors_per_cluster,
        app_main_rdx_file.first_cluster,
        app_main_rdx_file.cluster_count,
        app_main_rdx_content,
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
        let pct = map_install_progress(92, 99, runtime_data_done, runtime_data_total);
        let detail = format!(
            "COPYING LINUXRT FILES {}/{}",
            runtime_data_done, runtime_data_total
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
    sector[3..11].copy_from_slice(b"REDUXOS ");
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
    sector[71..82].copy_from_slice(b"REDUXOS    ");
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
