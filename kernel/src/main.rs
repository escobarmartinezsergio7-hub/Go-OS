#![no_std]
#![no_main]
#![allow(static_mut_refs)]
#![allow(dead_code)]

extern crate alloc;

mod framebuffer;
mod font;
mod hal;
mod input;
mod interrupts;
mod memory;
mod process;
mod privilege;
mod runtime;
mod scheduler;
mod syscall;
mod timer;
mod ui;
mod usermode;
mod pci;
mod virtio;
mod nvme;
mod xhci;
mod audio;
pub mod net;
mod intel_xe;
pub mod intel_net;
pub mod intel_wifi;
mod quota;
mod fs;
mod fat32;
mod allocator;
mod gui;
mod preboot_installer;
mod web_engine;
mod web_servo_bridge;
mod web_vaev_bridge;
#[cfg(all(
    feature = "servo_bridge",
    any(not(feature = "servo_external"), servo_external_unavailable)
))]
mod simpleservo_shim;
#[cfg(all(
    feature = "vaev_bridge",
    any(not(feature = "vaev_external"), vaev_external_unavailable)
))]
mod vaevbridge_shim;
mod ruby_runtime;
mod linux_compat;

use core::fmt::Write;
use core::panic::PanicInfo;
use core::str;
use alloc::string::String;
use alloc::vec::Vec;

use framebuffer::{FramebufferInfo, PixelLayout};
use uefi::mem::memory_map::MemoryType;
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::proto::console::text::{Key, ScanCode};
use uefi::runtime::ResetType;
use uefi::CString16;

const LOOP_STALL_US: usize = 10_000;
const LINE_MAX: usize = 128;
const UEFI_LOAD_OPTION_ACTIVE: u32 = 0x0000_0001;

// Compatibility shim: desktop currently runs on simple polling path.
pub fn desktop_irq_timer_active() -> bool {
    false
}

#[entry]
fn efi_main() -> Status {
    if uefi::helpers::init().is_err() {
        return Status::ABORTED;
    }

    // UEFI starts a 5-minute watchdog per loaded image. Disable it so
    // long-running GUI/installer sessions don't reboot unexpectedly.
    let _ = uefi::boot::set_watchdog_timer(0, 0, None);

    clear_screen();
    
    allocator::init_heap();
    maybe_auto_register_installed_boot_option();
    maybe_ensure_redux_boot_priority();

    // Run preboot installer while UEFI storage/input stack is still pristine.
    // Custom PCI/NVMe init can interfere with firmware BlockIO protocols.
    let installer_result = if should_skip_preboot_installer() {
        preboot_installer::InstallerResult::Skipped
    } else {
        preboot_installer::run()
    };
    println("Kernel stage: installer returned.");

    match installer_result {
        preboot_installer::InstallerResult::Installed => {
            reset_global_fat_mount_state();
            println("Preboot installer: install completed.");
            match ensure_installed_boot_option_registered() {
                Ok(msg) => println(msg.as_str()),
                Err(err) => println(alloc::format!("UEFI boot option: {}", err).as_str()),
            }
            match force_windows_boot_manager_to_redux() {
                Ok(msg) => println(msg.as_str()),
                Err(err) => println(alloc::format!("UEFI fallback hook: {}", err).as_str()),
            }
            println("Installer: rebooting now...");
            uefi::boot::stall(500_000);
            uefi::runtime::reset(ResetType::COLD, Status::SUCCESS, None);
        }
        preboot_installer::InstallerResult::Failed => {
            println("Preboot installer: finished with errors.");
        }
        preboot_installer::InstallerResult::Skipped => {
            println("Preboot installer: skipped.");
        }
    }
    if matches!(installer_result, preboot_installer::InstallerResult::Skipped)
        && should_show_boot_selector()
    {
        maybe_handle_boot_selector();
    }
    println("");

    let mem_status = memory::init_from_uefi();
    let idt = interrupts::init_skeleton();
    timer::init_polling(1); // 1ms per tick for GUI-based polling
    scheduler::init_demo();
    pci::scan();
    
    // Init network
    net::init();
    
    quota::init();
    quota::test_quota();

    println("ReduxOS UEFI Kernel - Phase 1+");
    println("x86_64 + OVMF | Rust no_std");
    println("");

    // If installer completed (or user skipped), continue directly to runtime GUI.
    // This avoids the "stuck screen" perception where VGA stays on installer UI
    // while shell prompt is only visible on serial.
    if matches!(installer_result, preboot_installer::InstallerResult::Skipped) {
        if mem_status.is_ok() {
            println("Kernel stage: auto-launch GUI mode after installer.");
            uefi::boot::stall(300_000);
            start_gui_mode();
        } else {
            println("Kernel stage: memory init failed; staying in shell.");
        }
    }

    match mem_status {
        Ok(stats) => {
            with_stdout(|out| {
                let _ = writeln!(
                    out,
                    "Memory map: regions={} conventional={} MiB",
                    stats.regions,
                    stats.conventional_bytes() / (1024 * 1024)
                );
            });
        }
        Err(status) => {
            with_stdout(|out| {
                let _ = writeln!(out, "Memory map init failed: {:?}", status);
            });
        }
    }

    with_stdout(|out| {
        let _ = writeln!(
            out,
            "IDT skeleton: init={} limit={} base={:#x}",
            idt.initialized,
            idt.limit,
            idt.base
        );
    });

    println("Type 'help' to list commands.");
    println("Use 'boot' for bare metal runtime (PS/2), 'boot uefi' for USB keyboards.");
    println("Use 'boot irq' for experimental IRQ timer mode (auto fallback).");
    println("");

    shell_loop(0)
}

fn boot_media_has_install_marker() -> bool {
    let Some(current) = current_boot_device_handle() else {
        return false;
    };

    // On removable media (USB), always allow entering the preboot installer.
    // The installed marker is only trusted for internal boots.
    if handle_is_removable(current) == Some(true) {
        return false;
    }

    handle_has_installed_redux_marker(current)
}

fn should_skip_preboot_installer() -> bool {
    if boot_media_has_install_marker() {
        println("Preboot installer: installed media marker detected, skipping installer.");
        return true;
    }

    let Some(current) = current_boot_device_handle() else {
        return false;
    };
    if handle_is_removable(current) == Some(false) {
        println("Preboot installer: internal boot detected, skipping installer.");
        return true;
    }

    false
}

fn should_show_boot_selector() -> bool {
    if boot_media_has_install_marker() {
        // If we are already running from the installed Redux volume,
        // continue directly to GUI instead of presenting another selector.
        return false;
    }
    let Some(current) = current_boot_device_handle() else {
        return false;
    };
    handle_is_removable(current) == Some(false)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BootSelectorChoice {
    BootRedux,
    BootLinuxGuest,
    BootOtherOs,
}

fn maybe_handle_boot_selector() {
    let current_handle = current_boot_device_handle();
    let installed_handle = find_installed_redux_handle(current_handle);
    let has_linux_guest = detect_linux_guest_boot_target(current_handle, installed_handle).is_some();
    let has_other_os = detect_other_os_boot_target(current_handle, installed_handle).is_some()
        || find_windows_boot_option_id().ok().flatten().is_some();

    if installed_handle.is_none() && !has_other_os && !has_linux_guest {
        return;
    }

    clear_screen();
    println("ReduxOS Boot Manager");
    if installed_handle.is_some() {
        println("1) Iniciar ReduxOS instalado");
    } else {
        println("1) Iniciar ReduxOS");
    }
    let mut next_option = 2u8;
    if has_linux_guest {
        println(alloc::format!("{} ) Iniciar Linux guest (apps Linux reales)", next_option).as_str());
        next_option = next_option.saturating_add(1);
    }
    if has_other_os {
        println(alloc::format!("{} ) Iniciar otro sistema operativo", next_option).as_str());
    }
    if next_option > 2 || has_other_os {
        let max_opt = if has_other_os { next_option } else { next_option.saturating_sub(1) };
        println(alloc::format!("Pulsa 1-{} (Enter=1).", max_opt).as_str());
    } else {
        println("Pulsa 1 (Enter=1).");
    }

    let choice = read_boot_selector_choice(has_linux_guest, has_other_os);
    match choice {
        BootSelectorChoice::BootRedux => {
            if installed_handle.is_some() {
                println("Arranque: ReduxOS instalado...");
                match launch_installed_redux(current_handle) {
                    Ok(path) => println(alloc::format!("Arranque regresó desde {}.", path).as_str()),
                    Err(err) => {
                        println(alloc::format!("No se pudo arrancar instalado: {}", err).as_str());
                        uefi::boot::stall(2_500_000);
                    }
                }
                println("Continuando con medio actual...");
                uefi::boot::stall(400_000);
            }
            clear_screen();
        }
        BootSelectorChoice::BootLinuxGuest => {
            println("Arranque: Linux guest...");
            match launch_linux_guest_boot(current_handle, installed_handle) {
                Ok(path) => println(alloc::format!("Arranque regresó desde {}.", path).as_str()),
                Err(err) => {
                    println(alloc::format!("No se pudo arrancar Linux guest: {}", err).as_str());
                    uefi::boot::stall(2_500_000);
                }
            }
            println("Continuando con medio actual...");
            uefi::boot::stall(400_000);
            clear_screen();
        }
        BootSelectorChoice::BootOtherOs => {
            println("Arranque: otro sistema operativo...");
            match launch_other_os_boot(current_handle, installed_handle) {
                Ok(path) => println(alloc::format!("Arranque regresó desde {}.", path).as_str()),
                Err(err) => {
                    println(alloc::format!("No se pudo arrancar otro SO: {}", err).as_str());
                    uefi::boot::stall(2_500_000);
                }
            }
            println("Continuando con medio actual...");
            uefi::boot::stall(400_000);
            clear_screen();
        }
    }
}

fn maybe_auto_register_installed_boot_option() {
    let Some(current) = current_boot_device_handle() else {
        return;
    };
    if handle_is_removable(current) != Some(true) {
        return;
    }
    match ensure_installed_boot_option_registered() {
        Ok(msg) => println(msg.as_str()),
        Err(err) => println(alloc::format!("UEFI boot option: {}", err).as_str()),
    }
}

fn read_boot_selector_choice(has_linux_guest: bool, has_other_os: bool) -> BootSelectorChoice {
    let mut waited_ticks = 0usize;
    let timeout_ticks = 1000usize; // ~10s at 10ms polling
    loop {
        if let Some(event) = poll_input_event() {
            match event {
                InputEvent::Char('1') | InputEvent::Enter | InputEvent::Escape => {
                    return BootSelectorChoice::BootRedux
                }
                InputEvent::Char('2') => {
                    if has_linux_guest {
                        return BootSelectorChoice::BootLinuxGuest;
                    }
                    if has_other_os {
                        return BootSelectorChoice::BootOtherOs;
                    }
                }
                InputEvent::Char('3') if has_linux_guest && has_other_os => {
                    return BootSelectorChoice::BootOtherOs
                }
                _ => {}
            }
        }
        if waited_ticks >= timeout_ticks {
            return BootSelectorChoice::BootRedux;
        }
        uefi::boot::stall(10_000);
        waited_ticks = waited_ticks.saturating_add(1);
    }
}

fn current_boot_device_handle() -> Option<uefi::Handle> {
    use uefi::boot;
    use uefi::proto::loaded_image::LoadedImage;

    let loaded = boot::open_protocol_exclusive::<LoadedImage>(boot::image_handle()).ok()?;
    loaded.device()
}

fn read_file_from_fs_handle(handle: uefi::Handle, path: &uefi::CStr16) -> Option<Vec<u8>> {
    use uefi::boot;
    use uefi::fs::FileSystem as UefiFileSystem;
    use uefi::proto::media::fs::SimpleFileSystem;

    let fs_proto = boot::open_protocol_exclusive::<SimpleFileSystem>(handle).ok()?;
    let mut fs = UefiFileSystem::new(fs_proto);
    fs.read(path).ok()
}

fn handle_is_removable(handle: uefi::Handle) -> Option<bool> {
    use uefi::boot;
    use uefi::proto::media::block::BlockIO;

    let blk = boot::open_protocol_exclusive::<BlockIO>(handle).ok()?;
    Some(blk.media().is_removable_media())
}

fn handle_has_installed_redux_marker(handle: uefi::Handle) -> bool {
    if let Some(bytes) = read_file_from_fs_handle(handle, uefi::cstr16!("\\REDUXOS.INI")) {
        let text = core::str::from_utf8(bytes.as_slice()).unwrap_or("");
        if text.contains("installed=1") || text.contains("INSTALLED=1") {
            return true;
        }
    }

    if let Some(bytes) = read_file_from_fs_handle(handle, uefi::cstr16!("\\README.TXT")) {
        let text = core::str::from_utf8(bytes.as_slice()).unwrap_or("");
        if text.contains("ReduxOS installed on internal storage.") {
            return true;
        }
    }

    false
}

fn reconnect_storage_controllers() {
    use uefi::boot;
    use uefi::proto::media::block::BlockIO;

    let Ok(handles) = boot::find_handles::<BlockIO>() else {
        return;
    };

    for handle in handles.iter().copied() {
        let _ = boot::connect_controller(handle, None, None, true);
    }
}

fn select_boot_path_candidate<'a>(
    handle: uefi::Handle,
    candidates: &'a [(&'a uefi::CStr16, &'a str)],
) -> Option<(&'a uefi::CStr16, &'a str)> {
    candidates
        .iter()
        .find(|(path, _)| read_file_from_fs_handle(handle, *path).is_some())
        .copied()
}

fn find_installed_redux_handle(exclude: Option<uefi::Handle>) -> Option<uefi::Handle> {
    use uefi::boot;
    use uefi::proto::media::fs::SimpleFileSystem;

    let handles = boot::find_handles::<SimpleFileSystem>().ok()?;
    let mut fallback = None;

    for handle in handles.iter().copied() {
        if Some(handle) == exclude {
            continue;
        }
        if !handle_has_installed_redux_marker(handle) {
            continue;
        }
        if handle_is_removable(handle) == Some(false) {
            return Some(handle);
        }
        if fallback.is_none() {
            fallback = Some(handle);
        }
    }

    fallback
}

fn find_installed_redux_handle_with_retry(exclude: Option<uefi::Handle>) -> Option<uefi::Handle> {
    if let Some(handle) = find_installed_redux_handle(exclude) {
        return Some(handle);
    }

    reconnect_storage_controllers();
    uefi::boot::stall(150_000);
    find_installed_redux_handle(exclude)
}

fn is_probably_efi_image(bytes: &[u8]) -> bool {
    bytes.len() >= 4096 && bytes.len() <= 64 * 1024 * 1024 && bytes[0] == b'M' && bytes[1] == b'Z'
}

fn load_grub_payload_for_fallback(source_handle: uefi::Handle) -> Result<Vec<u8>, String> {
    let target_candidates = [
        uefi::cstr16!("\\EFI\\GRUB\\GRUBX64.EFI"),
        uefi::cstr16!("\\EFI\\GRUB\\grubx64.efi"),
    ];

    for path in target_candidates.iter() {
        if let Some(bytes) = read_file_from_fs_handle(source_handle, *path) {
            if is_probably_efi_image(bytes.as_slice()) {
                return Ok(bytes);
            }
        }
    }

    let redux_present =
        read_file_from_fs_handle(source_handle, uefi::cstr16!("\\EFI\\BOOT\\REDUX64.EFI")).is_some();
    if redux_present {
        if let Some(bytes) =
            read_file_from_fs_handle(source_handle, uefi::cstr16!("\\EFI\\BOOT\\BOOTX64.EFI"))
        {
            if is_probably_efi_image(bytes.as_slice()) {
                return Ok(bytes);
            }
        }
    }

    use uefi::boot;
    use uefi::fs::FileSystem as UefiFileSystem;

    let fs_proto = boot::get_image_file_system(boot::image_handle())
        .map_err(|err| alloc::format!("volumen de arranque no disponible: {:?}", err))?;
    let mut fs = UefiFileSystem::new(fs_proto);
    let boot_media_candidates = [
        uefi::cstr16!("\\EFI\\GRUB\\GRUBX64.EFI"),
        uefi::cstr16!("\\EFI\\GRUB\\grubx64.efi"),
        uefi::cstr16!("\\EFI\\REDUXOS\\GRUBX64.EFI"),
        uefi::cstr16!("\\EFI\\REDUXOS\\grubx64.efi"),
        uefi::cstr16!("\\GRUBX64.EFI"),
        uefi::cstr16!("\\grubx64.efi"),
    ];
    for path in boot_media_candidates.iter() {
        let Ok(bytes) = fs.read(*path) else {
            continue;
        };
        if is_probably_efi_image(bytes.as_slice()) {
            return Ok(bytes);
        }
    }

    Err(String::from("no se encontro GRUBX64.EFI para aplicar fallback de arranque"))
}

fn load_redux_payload_for_fallback(source_handle: uefi::Handle) -> Result<Vec<u8>, String> {
    let candidates = [
        uefi::cstr16!("\\EFI\\BOOT\\REDUX64.EFI"),
        uefi::cstr16!("\\EFI\\BOOT\\BOOTX64.EFI"),
        uefi::cstr16!("\\EFI\\REDUXOS\\BOOTX64.EFI"),
    ];

    for path in candidates.iter() {
        if let Some(bytes) = read_file_from_fs_handle(source_handle, *path) {
            if is_probably_efi_image(bytes.as_slice()) {
                return Ok(bytes);
            }
        }
    }

    Err(String::from(
        "no se encontro REDUX64/BOOTX64 en instalacion interna para fallback",
    ))
}

fn build_forced_grub_config_payload(redux_path: &str, windows_path: Option<&str>) -> Vec<u8> {
    let mut cfg = alloc::format!(
        "set timeout=8\r\n\
set default=0\r\n\
\r\n\
menuentry \"ReduxOS\" {{\r\n\
    if search --no-floppy --file --set=reduxroot {}; then\r\n\
        chainloader ($reduxroot){}\r\n\
        boot\r\n\
    fi\r\n\
    if search --no-floppy --file --set=reduxroot /EFI/BOOT/BOOTX64.EFI; then\r\n\
        chainloader ($reduxroot)/EFI/BOOT/BOOTX64.EFI\r\n\
        boot\r\n\
    fi\r\n\
}}\r\n",
        redux_path, redux_path
    );

    if let Some(win_path) = windows_path {
        cfg.push_str(
            alloc::format!(
                "\r\n\
menuentry \"Windows 11\" {{\r\n\
    if search --no-floppy --file --set=winroot {}; then\r\n\
        chainloader ($winroot){}\r\n\
        boot\r\n\
    fi\r\n\
}}\r\n",
                win_path, win_path
            )
            .as_str(),
        );
    }

    cfg.into_bytes()
}

fn write_forced_grub_config(
    fs: &mut uefi::fs::FileSystem,
    redux_path: &str,
    windows_path: Option<&str>,
) -> Result<(), String> {
    let cfg = build_forced_grub_config_payload(redux_path, windows_path);

    for dir in [
        uefi::cstr16!("\\EFI\\GRUB"),
        uefi::cstr16!("\\EFI\\BOOT"),
        uefi::cstr16!("\\boot\\grub"),
        uefi::cstr16!("\\BOOT\\GRUB"),
    ] {
        let _ = fs.create_dir_all(dir);
    }

    let cfg_candidates = [
        uefi::cstr16!("\\EFI\\GRUB\\GRUB.CFG"),
        uefi::cstr16!("\\EFI\\GRUB\\grub.cfg"),
        uefi::cstr16!("\\EFI\\BOOT\\GRUB.CFG"),
        uefi::cstr16!("\\EFI\\BOOT\\grub.cfg"),
        uefi::cstr16!("\\GRUB.CFG"),
        uefi::cstr16!("\\grub.cfg"),
        uefi::cstr16!("\\boot\\grub\\grub.cfg"),
        uefi::cstr16!("\\BOOT\\GRUB\\GRUB.CFG"),
    ];

    for path in cfg_candidates.iter() {
        let _ = fs.write(*path, cfg.as_slice());
    }

    Ok(())
}

fn collect_internal_simplefs_handles() -> Vec<uefi::Handle> {
    use uefi::boot;
    use uefi::proto::media::fs::SimpleFileSystem;

    let Ok(handles) = boot::find_handles::<SimpleFileSystem>() else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for handle in handles.iter().copied() {
        if handle_is_removable(handle) == Some(true) {
            continue;
        }
        out.push(handle);
    }
    out
}

fn should_seed_bootstrap_assets_on_handle(
    handle: uefi::Handle,
    current_handle: Option<uefi::Handle>,
    installed_handle: uefi::Handle,
    hook_handle: uefi::Handle,
) -> bool {
    if Some(handle) == current_handle || handle == installed_handle || handle == hook_handle {
        return true;
    }

    let markers = [
        uefi::cstr16!("\\EFI\\GRUB\\GRUB.CFG"),
        uefi::cstr16!("\\EFI\\GRUB\\grub.cfg"),
        uefi::cstr16!("\\EFI\\BOOT\\GRUB.CFG"),
        uefi::cstr16!("\\EFI\\BOOT\\grub.cfg"),
        uefi::cstr16!("\\boot\\grub\\grub.cfg"),
        uefi::cstr16!("\\BOOT\\GRUB\\GRUB.CFG"),
        uefi::cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.efi"),
        uefi::cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.redux.bak.efi"),
        uefi::cstr16!("\\EFI\\BOOT\\BOOTX64.EFI"),
    ];

    markers
        .iter()
        .any(|path| read_file_from_fs_handle(handle, *path).is_some())
}

fn force_windows_boot_manager_to_redux() -> Result<String, String> {
    use uefi::boot;
    use uefi::fs::FileSystem as UefiFileSystem;
    use uefi::proto::media::fs::SimpleFileSystem;

    let current_handle = current_boot_device_handle();
    let installed_handle = find_installed_redux_handle_with_retry(current_handle)
        .ok_or_else(|| String::from("no se detecto instalacion interna para fallback Microsoft"))?;
    if handle_is_removable(installed_handle) == Some(true) {
        return Err(String::from(
            "fallback Microsoft cancelado: instalacion detectada como removible",
        ));
    }
    let hook_handle = find_windows_boot_handle(current_handle, Some(installed_handle))
        .unwrap_or(installed_handle);
    if handle_is_removable(hook_handle) == Some(true) {
        return Err(String::from("fallback Microsoft cancelado: ESP de hook removible"));
    }

    let redux_payload = load_redux_payload_for_fallback(installed_handle)?;

    let fs_proto = boot::open_protocol_exclusive::<SimpleFileSystem>(hook_handle)
        .map_err(|err| alloc::format!("SimpleFS destino no disponible: {:?}", err))?;
    let mut fs = UefiFileSystem::new(fs_proto);

    fs.create_dir_all(uefi::cstr16!("\\EFI\\Microsoft\\Boot"))
        .map_err(|err| alloc::format!("creando \\EFI\\Microsoft\\Boot: {:?}", err))?;
    fs.create_dir_all(uefi::cstr16!("\\EFI\\BOOT"))
        .map_err(|err| alloc::format!("creando \\EFI\\BOOT: {:?}", err))?;

    let win_loader = uefi::cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.efi");
    let win_backup = uefi::cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.redux.bak.efi");

    let mut backup_created = false;
    let mut backup_exists = fs
        .try_exists(win_backup)
        .map_err(|err| alloc::format!("leyendo backup de Windows: {:?}", err))?;
    let loader_exists = fs
        .try_exists(win_loader)
        .map_err(|err| alloc::format!("leyendo bootmgfw.efi: {:?}", err))?;

    if loader_exists && !backup_exists {
        fs.rename(win_loader, win_backup)
            .map_err(|err| alloc::format!("respaldando bootmgfw.efi: {:?}", err))?;
        backup_exists = true;
        backup_created = true;
    }

    fs.write(win_loader, redux_payload.as_slice())
        .map_err(|err| alloc::format!("escribiendo fallback bootmgfw.efi: {:?}", err))?;
    fs.write(uefi::cstr16!("\\EFI\\BOOT\\REDUX64.EFI"), redux_payload.as_slice())
        .map_err(|err| alloc::format!("escribiendo \\EFI\\BOOT\\REDUX64.EFI: {:?}", err))?;
    fs.write(uefi::cstr16!("\\EFI\\BOOT\\BOOTX64.EFI"), redux_payload.as_slice())
        .map_err(|err| alloc::format!("escribiendo \\EFI\\BOOT\\BOOTX64.EFI: {:?}", err))?;

    // Seed fallback artifacts on likely boot-related internal volumes too,
    // so firmware has consistent paths across internal ESPs.
    let all_internal = collect_internal_simplefs_handles();
    for handle in all_internal {
        if !should_seed_bootstrap_assets_on_handle(handle, current_handle, installed_handle, hook_handle) {
            continue;
        }
        let Ok(proto) = boot::open_protocol_exclusive::<SimpleFileSystem>(handle) else {
            continue;
        };
        let mut hfs = UefiFileSystem::new(proto);
        let _ = hfs.create_dir_all(uefi::cstr16!("\\EFI\\BOOT"));
        let _ = hfs.write(uefi::cstr16!("\\EFI\\BOOT\\REDUX64.EFI"), redux_payload.as_slice());
        let _ = hfs.write(uefi::cstr16!("\\EFI\\BOOT\\BOOTX64.EFI"), redux_payload.as_slice());
    }

    if backup_created {
        Ok(String::from(
            "Fallback Microsoft aplicado: bootmgfw.efi ahora abre ReduxEFI (backup creado).",
        ))
    } else if backup_exists {
        Ok(String::from(
            "Fallback Microsoft aplicado: bootmgfw.efi ahora abre ReduxEFI (backup ya existia en ESP).",
        ))
    } else {
        Ok(String::from(
            "Fallback Microsoft aplicado: bootmgfw.efi ahora abre ReduxEFI (ESP sin backup previo).",
        ))
    }
}

fn handle_has_any_path(handle: uefi::Handle, candidates: &[(&uefi::CStr16, &'static str)]) -> bool {
    for (path, _) in candidates.iter() {
        if read_file_from_fs_handle(handle, *path).is_some() {
            return true;
        }
    }
    false
}

fn detect_other_os_boot_target(
    current_handle: Option<uefi::Handle>,
    installed_redux: Option<uefi::Handle>,
) -> Option<uefi::Handle> {
    use uefi::boot;
    use uefi::proto::media::fs::SimpleFileSystem;

    let all_candidates: [(&uefi::CStr16, &'static str); 8] = [
        (
            uefi::cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.redux.bak.efi"),
            "\\EFI\\Microsoft\\Boot\\bootmgfw.redux.bak.efi",
        ),
        (uefi::cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.efi"), "\\EFI\\Microsoft\\Boot\\bootmgfw.efi"),
        (uefi::cstr16!("\\EFI\\MICROSOFT\\BOOT\\BOOTMGFW.EFI"), "\\EFI\\MICROSOFT\\BOOT\\BOOTMGFW.EFI"),
        (uefi::cstr16!("\\EFI\\ubuntu\\shimx64.efi"), "\\EFI\\ubuntu\\shimx64.efi"),
        (uefi::cstr16!("\\EFI\\ubuntu\\grubx64.efi"), "\\EFI\\ubuntu\\grubx64.efi"),
        (uefi::cstr16!("\\EFI\\debian\\grubx64.efi"), "\\EFI\\debian\\grubx64.efi"),
        (uefi::cstr16!("\\EFI\\fedora\\shimx64.efi"), "\\EFI\\fedora\\shimx64.efi"),
        (uefi::cstr16!("\\EFI\\fedora\\grubx64.efi"), "\\EFI\\fedora\\grubx64.efi"),
    ];
    let current_handle_candidates: [(&uefi::CStr16, &'static str); 6] = [
        (
            uefi::cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.redux.bak.efi"),
            "\\EFI\\Microsoft\\Boot\\bootmgfw.redux.bak.efi",
        ),
        (uefi::cstr16!("\\EFI\\ubuntu\\shimx64.efi"), "\\EFI\\ubuntu\\shimx64.efi"),
        (uefi::cstr16!("\\EFI\\ubuntu\\grubx64.efi"), "\\EFI\\ubuntu\\grubx64.efi"),
        (uefi::cstr16!("\\EFI\\debian\\grubx64.efi"), "\\EFI\\debian\\grubx64.efi"),
        (uefi::cstr16!("\\EFI\\fedora\\shimx64.efi"), "\\EFI\\fedora\\shimx64.efi"),
        (uefi::cstr16!("\\EFI\\fedora\\grubx64.efi"), "\\EFI\\fedora\\grubx64.efi"),
    ];

    let handles = boot::find_handles::<SimpleFileSystem>().ok()?;
    let mut fallback = None;

    for handle in handles.iter().copied() {
        if Some(handle) == installed_redux {
            continue;
        }
        if handle_has_installed_redux_marker(handle) {
            continue;
        }
        let candidates = if Some(handle) == current_handle {
            current_handle_candidates.as_slice()
        } else {
            all_candidates.as_slice()
        };
        if !handle_has_any_path(handle, candidates) {
            continue;
        }
        if handle_is_removable(handle) == Some(false) {
            return Some(handle);
        }
        if fallback.is_none() {
            fallback = Some(handle);
        }
    }

    fallback
}

fn find_windows_boot_handle(
    current_handle: Option<uefi::Handle>,
    installed_redux: Option<uefi::Handle>,
) -> Option<uefi::Handle> {
    use uefi::boot;
    use uefi::proto::media::fs::SimpleFileSystem;

    let windows_candidates = [
        uefi::cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.redux.bak.efi"),
        uefi::cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.efi"),
        uefi::cstr16!("\\EFI\\MICROSOFT\\BOOT\\BOOTMGFW.EFI"),
    ];

    let handles = boot::find_handles::<SimpleFileSystem>().ok()?;
    let mut fallback = None;

    for handle in handles.iter().copied() {
        if Some(handle) == current_handle || Some(handle) == installed_redux {
            continue;
        }
        if !handle_has_any_path(
            handle,
            &[
                (windows_candidates[0], ""),
                (windows_candidates[1], ""),
                (windows_candidates[2], ""),
            ],
        ) {
            continue;
        }
        if handle_is_removable(handle) == Some(false) {
            return Some(handle);
        }
        if fallback.is_none() {
            fallback = Some(handle);
        }
    }

    fallback
}

fn start_image_from_handle<'a>(
    handle: uefi::Handle,
    image_candidates: &[(&uefi::CStr16, &'a str)],
) -> core::result::Result<&'a str, String> {
    use uefi::boot::{self, LoadImageSource};
    use uefi::proto::BootPolicy;
    use uefi::proto::device_path::DevicePath;
    use uefi::proto::device_path::build;

    let parent_image = boot::image_handle();
    let mut last_error = String::from("no se encontro ejecutable EFI en destino");

    'candidate: for (path_cstr, path_label) in image_candidates.iter() {
        let mut path_vec: Vec<u8> = Vec::new();
        let full_path = {
            let Ok(device_path_proto) = boot::open_protocol_exclusive::<DevicePath>(handle) else {
                last_error = String::from("no se pudo abrir DevicePath del volumen destino");
                continue 'candidate;
            };

            let file_node = build::media::FilePath { path_name: *path_cstr };
            let mut builder = build::DevicePathBuilder::with_vec(&mut path_vec);
            for node in device_path_proto.node_iter() {
                builder = match builder.push(&node) {
                    Ok(next) => next,
                    Err(_) => {
                        last_error = String::from("fallo construyendo DevicePath");
                        continue 'candidate;
                    }
                };
            }
            builder = match builder.push(&file_node) {
                Ok(next) => next,
                Err(_) => {
                    last_error = String::from("fallo agregando archivo EFI al DevicePath");
                    continue 'candidate;
                }
            };
            match builder.finalize() {
                Ok(path) => path,
                Err(_) => {
                    last_error = String::from("fallo finalizando DevicePath");
                    continue 'candidate;
                }
            }
        };

        let image_handle = match boot::load_image(
            parent_image,
            LoadImageSource::FromDevicePath {
                device_path: full_path,
                boot_policy: BootPolicy::ExactMatch,
            },
        ) {
            Ok(h) => h,
            Err(err) => {
                last_error = alloc::format!("LoadImage fallo: {:?}", err);
                continue;
            }
        };

        match boot::start_image(image_handle) {
            Ok(()) => return Ok(*path_label),
            Err(err) => {
                last_error = alloc::format!("StartImage fallo: {:?}", err);
            }
        }
    }

    Err(last_error)
}

fn launch_installed_redux(current_handle: Option<uefi::Handle>) -> core::result::Result<&'static str, String> {
    let Some(handle) = find_installed_redux_handle(current_handle) else {
        return Err(String::from("no se detecto ReduxOS instalado en otro volumen"));
    };

    let candidates: [(&uefi::CStr16, &'static str); 6] = [
        (uefi::cstr16!("\\EFI\\GRUB\\GRUBX64.EFI"), "\\EFI\\GRUB\\GRUBX64.EFI"),
        (uefi::cstr16!("\\EFI\\GRUB\\grubx64.efi"), "\\EFI\\GRUB\\grubx64.efi"),
        (uefi::cstr16!("\\EFI\\BOOT\\BOOTX64.EFI"), "\\EFI\\BOOT\\BOOTX64.EFI"),
        (uefi::cstr16!("\\EFI\\boot\\bootx64.efi"), "\\EFI\\boot\\bootx64.efi"),
        (uefi::cstr16!("\\EFI\\REDUXOS\\BOOTX64.EFI"), "\\EFI\\REDUXOS\\BOOTX64.EFI"),
        (uefi::cstr16!("\\EFI\\reduxos\\bootx64.efi"), "\\EFI\\reduxos\\bootx64.efi"),
    ];
    start_image_from_handle(handle, candidates.as_slice())
}

fn launch_other_os_boot(
    current_handle: Option<uefi::Handle>,
    installed_redux: Option<uefi::Handle>,
) -> core::result::Result<&'static str, String> {
    use uefi::boot;
    use uefi::proto::media::fs::SimpleFileSystem;

    let all_candidates: [(&uefi::CStr16, &'static str); 8] = [
        (
            uefi::cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.redux.bak.efi"),
            "\\EFI\\Microsoft\\Boot\\bootmgfw.redux.bak.efi",
        ),
        (uefi::cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.efi"), "\\EFI\\Microsoft\\Boot\\bootmgfw.efi"),
        (uefi::cstr16!("\\EFI\\MICROSOFT\\BOOT\\BOOTMGFW.EFI"), "\\EFI\\MICROSOFT\\BOOT\\BOOTMGFW.EFI"),
        (uefi::cstr16!("\\EFI\\ubuntu\\shimx64.efi"), "\\EFI\\ubuntu\\shimx64.efi"),
        (uefi::cstr16!("\\EFI\\ubuntu\\grubx64.efi"), "\\EFI\\ubuntu\\grubx64.efi"),
        (uefi::cstr16!("\\EFI\\debian\\grubx64.efi"), "\\EFI\\debian\\grubx64.efi"),
        (uefi::cstr16!("\\EFI\\fedora\\shimx64.efi"), "\\EFI\\fedora\\shimx64.efi"),
        (uefi::cstr16!("\\EFI\\fedora\\grubx64.efi"), "\\EFI\\fedora\\grubx64.efi"),
    ];
    let current_handle_candidates: [(&uefi::CStr16, &'static str); 6] = [
        (
            uefi::cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.redux.bak.efi"),
            "\\EFI\\Microsoft\\Boot\\bootmgfw.redux.bak.efi",
        ),
        (uefi::cstr16!("\\EFI\\ubuntu\\shimx64.efi"), "\\EFI\\ubuntu\\shimx64.efi"),
        (uefi::cstr16!("\\EFI\\ubuntu\\grubx64.efi"), "\\EFI\\ubuntu\\grubx64.efi"),
        (uefi::cstr16!("\\EFI\\debian\\grubx64.efi"), "\\EFI\\debian\\grubx64.efi"),
        (uefi::cstr16!("\\EFI\\fedora\\shimx64.efi"), "\\EFI\\fedora\\shimx64.efi"),
        (uefi::cstr16!("\\EFI\\fedora\\grubx64.efi"), "\\EFI\\fedora\\grubx64.efi"),
    ];

    let handles = boot::find_handles::<SimpleFileSystem>()
        .map_err(|err| alloc::format!("no hay volúmenes SimpleFS: {:?}", err))?;
    let mut last_error = String::from("no se encontro cargador EFI de otro sistema");

    // Prefer internal volumes first, then removable.
    for pass in 0..2 {
        for handle in handles.iter().copied() {
            if Some(handle) == installed_redux {
                continue;
            }
            if handle_has_installed_redux_marker(handle) {
                continue;
            }
            let removable = handle_is_removable(handle).unwrap_or(false);
            if (pass == 0 && removable) || (pass == 1 && !removable) {
                continue;
            }

            let candidates = if Some(handle) == current_handle {
                current_handle_candidates.as_slice()
            } else {
                all_candidates.as_slice()
            };

            match start_image_from_handle(handle, candidates) {
                Ok(path) => return Ok(path),
                Err(err) => last_error = err,
            }
        }
    }

    if let Some(win_id) = find_windows_boot_option_id()? {
        if let Some(redux_id) = find_redux_boot_option_id()? {
            let _ = ensure_boot_order_contains(redux_id);
        }
        write_boot_next(win_id)?;
        println(
            alloc::format!(
                "Otro SO fallback: BootNext=Boot{:04X} (Windows). Reiniciando...",
                win_id
            )
            .as_str(),
        );
        uefi::boot::stall(300_000);
        uefi::runtime::reset(ResetType::COLD, Status::SUCCESS, None);
    }

    Err(last_error)
}

fn detect_linux_guest_boot_target(
    current_handle: Option<uefi::Handle>,
    installed_redux: Option<uefi::Handle>,
) -> Option<uefi::Handle> {
    use uefi::boot;
    use uefi::proto::media::fs::SimpleFileSystem;

    let candidates: [(&uefi::CStr16, &'static str); 12] = [
        (uefi::cstr16!("\\EFI\\LINUX\\BOOTX64.EFI"), "\\EFI\\LINUX\\BOOTX64.EFI"),
        (uefi::cstr16!("\\EFI\\LINUX\\LINUX.EFI"), "\\EFI\\LINUX\\LINUX.EFI"),
        (uefi::cstr16!("\\EFI\\LINUX\\VMLINUZ.EFI"), "\\EFI\\LINUX\\VMLINUZ.EFI"),
        (uefi::cstr16!("\\EFI\\BOOT\\LINUX.EFI"), "\\EFI\\BOOT\\LINUX.EFI"),
        (uefi::cstr16!("\\EFI\\BOOT\\VMLINUZ.EFI"), "\\EFI\\BOOT\\VMLINUZ.EFI"),
        (uefi::cstr16!("\\LINUX\\BOOTX64.EFI"), "\\LINUX\\BOOTX64.EFI"),
        (uefi::cstr16!("\\LINUX\\LINUX.EFI"), "\\LINUX\\LINUX.EFI"),
        (uefi::cstr16!("\\BOOT\\VMLINUZ.EFI"), "\\BOOT\\VMLINUZ.EFI"),
        (uefi::cstr16!("\\boot\\vmlinuz.efi"), "\\boot\\vmlinuz.efi"),
        (uefi::cstr16!("\\boot\\linux.efi"), "\\boot\\linux.efi"),
        (uefi::cstr16!("\\EFI\\SYSTEMD\\SYSTEMD-BOOTX64.EFI"), "\\EFI\\SYSTEMD\\SYSTEMD-BOOTX64.EFI"),
        (uefi::cstr16!("\\EFI\\systemd\\systemd-bootx64.efi"), "\\EFI\\systemd\\systemd-bootx64.efi"),
    ];

    let handles = boot::find_handles::<SimpleFileSystem>().ok()?;
    let mut fallback = None;
    for handle in handles.iter().copied() {
        if Some(handle) == installed_redux {
            continue;
        }
        // Avoid selecting the same Redux-installed volume as "Linux guest".
        if handle_has_installed_redux_marker(handle) {
            continue;
        }
        if Some(handle) == current_handle && !handle_has_any_path(handle, candidates.as_slice()) {
            continue;
        }
        if !handle_has_any_path(handle, candidates.as_slice()) {
            continue;
        }
        if handle_is_removable(handle) == Some(false) {
            return Some(handle);
        }
        if fallback.is_none() {
            fallback = Some(handle);
        }
    }
    fallback
}

fn launch_linux_guest_boot(
    current_handle: Option<uefi::Handle>,
    installed_redux: Option<uefi::Handle>,
) -> core::result::Result<&'static str, String> {
    use uefi::boot;
    use uefi::proto::media::fs::SimpleFileSystem;

    let all_candidates: [(&uefi::CStr16, &'static str); 12] = [
        (uefi::cstr16!("\\EFI\\LINUX\\BOOTX64.EFI"), "\\EFI\\LINUX\\BOOTX64.EFI"),
        (uefi::cstr16!("\\EFI\\LINUX\\LINUX.EFI"), "\\EFI\\LINUX\\LINUX.EFI"),
        (uefi::cstr16!("\\EFI\\LINUX\\VMLINUZ.EFI"), "\\EFI\\LINUX\\VMLINUZ.EFI"),
        (uefi::cstr16!("\\EFI\\BOOT\\LINUX.EFI"), "\\EFI\\BOOT\\LINUX.EFI"),
        (uefi::cstr16!("\\EFI\\BOOT\\VMLINUZ.EFI"), "\\EFI\\BOOT\\VMLINUZ.EFI"),
        (uefi::cstr16!("\\LINUX\\BOOTX64.EFI"), "\\LINUX\\BOOTX64.EFI"),
        (uefi::cstr16!("\\LINUX\\LINUX.EFI"), "\\LINUX\\LINUX.EFI"),
        (uefi::cstr16!("\\BOOT\\VMLINUZ.EFI"), "\\BOOT\\VMLINUZ.EFI"),
        (uefi::cstr16!("\\boot\\vmlinuz.efi"), "\\boot\\vmlinuz.efi"),
        (uefi::cstr16!("\\boot\\linux.efi"), "\\boot\\linux.efi"),
        (uefi::cstr16!("\\EFI\\SYSTEMD\\SYSTEMD-BOOTX64.EFI"), "\\EFI\\SYSTEMD\\SYSTEMD-BOOTX64.EFI"),
        (uefi::cstr16!("\\EFI\\systemd\\systemd-bootx64.efi"), "\\EFI\\systemd\\systemd-bootx64.efi"),
    ];

    let handles = boot::find_handles::<SimpleFileSystem>()
        .map_err(|err| alloc::format!("no hay volúmenes SimpleFS: {:?}", err))?;
    let mut last_error = String::from("no se encontro loader EFI para Linux guest");

    for pass in 0..2 {
        for handle in handles.iter().copied() {
            if Some(handle) == installed_redux {
                continue;
            }
            if handle_has_installed_redux_marker(handle) {
                continue;
            }
            let removable = handle_is_removable(handle).unwrap_or(false);
            if (pass == 0 && removable) || (pass == 1 && !removable) {
                continue;
            }

            let candidates = if Some(handle) == current_handle {
                all_candidates.as_slice()
            } else {
                all_candidates.as_slice()
            };
            match start_image_from_handle(handle, candidates) {
                Ok(path) => return Ok(path),
                Err(err) => last_error = err,
            }
        }
    }

    Err(last_error)
}

fn extract_boot_option_description(data: &[u8]) -> Option<String> {
    if data.len() < 8 {
        return None;
    }
    let mut off = 6usize;
    let mut units: Vec<u16> = Vec::new();
    while off + 1 < data.len() {
        let unit = u16::from_le_bytes([data[off], data[off + 1]]);
        off += 2;
        if unit == 0 {
            break;
        }
        units.push(unit);
    }

    let mut out = String::new();
    for decoded in core::char::decode_utf16(units.into_iter()) {
        match decoded {
            Ok(ch) => out.push(ch),
            Err(_) => out.push('?'),
        }
    }
    Some(out)
}

fn contains_ascii_utf16_case_insensitive(data: &[u8], needle_lower: &str) -> bool {
    let mut text = String::new();
    let mut i = 0usize;
    while i + 1 < data.len() {
        let unit = u16::from_le_bytes([data[i], data[i + 1]]);
        i += 2;
        if (0x20..=0x7e).contains(&unit) {
            text.push((unit as u8 as char).to_ascii_lowercase());
        } else {
            text.push(' ');
        }
    }
    text.contains(needle_lower)
}

fn read_boot_option_variable(id: u16) -> Result<Option<Vec<u8>>, String> {
    let vendor = uefi::runtime::VariableVendor::GLOBAL_VARIABLE;
    let name = CString16::try_from(alloc::format!("Boot{:04X}", id).as_str())
        .map_err(|_| String::from("nombre Boot#### invalido"))?;

    match uefi::runtime::get_variable_boxed(name.as_ref(), &vendor) {
        Ok((bytes, _attrs)) => Ok(Some(bytes.to_vec())),
        Err(err) => {
            if err.status() == Status::NOT_FOUND {
                Ok(None)
            } else {
                Err(alloc::format!(
                    "leyendo Boot{:04X}: {:?}",
                    id,
                    err.status()
                ))
            }
        }
    }
}

fn find_windows_boot_option_id() -> Result<Option<u16>, String> {
    let mut ids = read_boot_order()?;
    if ids.is_empty() {
        let vendor = uefi::runtime::VariableVendor::GLOBAL_VARIABLE;
        for key_result in uefi::runtime::variable_keys() {
            let key = key_result
                .map_err(|err| alloc::format!("iterando variables UEFI: {:?}", err.status()))?;
            if key.vendor != vendor {
                continue;
            }
            let name: String = String::from(&key.name);
            if let Some(id) = parse_boot_option_id_from_name(name.as_str()) {
                ids.push(id);
            }
        }
    }

    let path_needle = "bootmgfw.efi";
    for id in ids {
        let Some(raw) = read_boot_option_variable(id)? else {
            continue;
        };

        let desc = extract_boot_option_description(raw.as_slice()).unwrap_or_default();
        let desc_lower = desc.to_ascii_lowercase();
        if desc_lower.contains("windows") || desc_lower.contains("microsoft") {
            return Ok(Some(id));
        }
        if contains_ascii_utf16_case_insensitive(raw.as_slice(), path_needle) {
            return Ok(Some(id));
        }
    }

    Ok(None)
}

fn find_redux_boot_option_id() -> Result<Option<u16>, String> {
    let mut ids = read_boot_order()?;
    if ids.is_empty() {
        let vendor = uefi::runtime::VariableVendor::GLOBAL_VARIABLE;
        for key_result in uefi::runtime::variable_keys() {
            let key = key_result
                .map_err(|err| alloc::format!("iterando variables UEFI: {:?}", err.status()))?;
            if key.vendor != vendor {
                continue;
            }
            let name: String = String::from(&key.name);
            if let Some(id) = parse_boot_option_id_from_name(name.as_str()) {
                ids.push(id);
            }
        }
    }

    for id in ids {
        let Some(raw) = read_boot_option_variable(id)? else {
            continue;
        };

        let desc = extract_boot_option_description(raw.as_slice()).unwrap_or_default();
        let desc_lower = desc.to_ascii_lowercase();
        if desc_lower.contains("redux") {
            return Ok(Some(id));
        }

        if contains_ascii_utf16_case_insensitive(raw.as_slice(), "redux64.efi")
            || contains_ascii_utf16_case_insensitive(raw.as_slice(), "\\efi\\reduxos\\bootx64.efi")
        {
            return Ok(Some(id));
        }

        if contains_ascii_utf16_case_insensitive(raw.as_slice(), "\\efi\\boot\\bootx64.efi")
            && !desc_lower.contains("windows")
            && !desc_lower.contains("microsoft")
            && !desc_lower.contains("ubuntu")
            && !desc_lower.contains("debian")
            && !desc_lower.contains("fedora")
        {
            return Ok(Some(id));
        }
    }

    Ok(None)
}

fn maybe_ensure_redux_boot_priority() {
    let Ok(Some(redux_id)) = find_redux_boot_option_id() else {
        return;
    };
    let _ = ensure_boot_order_contains(redux_id);
}

fn ensure_installed_boot_option_registered() -> Result<String, String> {
    let current_handle = current_boot_device_handle();
    let target_handle = match find_installed_redux_handle(current_handle) {
        Some(handle) => handle,
        None => {
            reconnect_storage_controllers();
            uefi::boot::stall(150_000);
            find_installed_redux_handle(current_handle)
                .ok_or_else(|| String::from("no se detecto instalacion interna de ReduxOS"))?
        }
    };

    let path_candidates: [(&uefi::CStr16, &'static str); 6] = [
        (uefi::cstr16!("\\EFI\\GRUB\\GRUBX64.EFI"), "\\EFI\\GRUB\\GRUBX64.EFI"),
        (uefi::cstr16!("\\EFI\\GRUB\\grubx64.efi"), "\\EFI\\GRUB\\grubx64.efi"),
        (uefi::cstr16!("\\EFI\\BOOT\\BOOTX64.EFI"), "\\EFI\\BOOT\\BOOTX64.EFI"),
        (uefi::cstr16!("\\EFI\\boot\\bootx64.efi"), "\\EFI\\boot\\bootx64.efi"),
        (uefi::cstr16!("\\EFI\\REDUXOS\\BOOTX64.EFI"), "\\EFI\\REDUXOS\\BOOTX64.EFI"),
        (uefi::cstr16!("\\EFI\\reduxos\\bootx64.efi"), "\\EFI\\reduxos\\bootx64.efi"),
    ];

    let selected = match select_boot_path_candidate(target_handle, path_candidates.as_slice()) {
        Some(path) => path,
        None => {
            reconnect_storage_controllers();
            uefi::boot::stall(150_000);
            select_boot_path_candidate(target_handle, path_candidates.as_slice())
                .ok_or_else(|| String::from("no se encontro BOOTX64.EFI en la instalacion interna"))?
        }
    };

    let file_dp = build_file_device_path_bytes(target_handle, selected.0)?;
    if let Some(existing_id) = find_existing_boot_option_for_path(file_dp.as_slice())? {
        ensure_boot_order_contains(existing_id)?;
        let _ = write_boot_next(existing_id);
        return Ok(alloc::format!(
            "Entrada UEFI existente lista: Boot{:04X} ({})",
            existing_id,
            selected.1
        ));
    }

    let free_id = find_free_boot_option_id()?;
    let load_option = build_boot_load_option("ReduxOS", file_dp.as_slice(), &[])?;
    write_boot_option_variable(free_id, load_option.as_slice())?;
    ensure_boot_order_contains(free_id)?;
    let _ = write_boot_next(free_id);

    Ok(alloc::format!(
        "Entrada UEFI creada: Boot{:04X} ({})",
        free_id,
        selected.1
    ))
}

fn build_file_device_path_bytes(
    handle: uefi::Handle,
    path: &uefi::CStr16,
) -> Result<Vec<u8>, String> {
    use uefi::boot;
    use uefi::proto::device_path::DevicePath;
    use uefi::proto::device_path::build;

    let mut path_vec: Vec<u8> = Vec::new();
    let full_path = {
        let device_path_proto = boot::open_protocol_exclusive::<DevicePath>(handle)
            .map_err(|err| alloc::format!("DevicePath no disponible: {:?}", err))?;

        let file_node = build::media::FilePath { path_name: path };
        let mut builder = build::DevicePathBuilder::with_vec(&mut path_vec);
        for node in device_path_proto.node_iter() {
            builder = builder
                .push(&node)
                .map_err(|_| String::from("fallo construyendo DevicePath"))?;
        }
        builder = builder
            .push(&file_node)
            .map_err(|_| String::from("fallo agregando FilePath al DevicePath"))?;
        builder
            .finalize()
            .map_err(|_| String::from("fallo finalizando DevicePath"))?
    };

    Ok(full_path.as_bytes().to_vec())
}

fn build_boot_load_option(
    description: &str,
    file_path_list: &[u8],
    optional_data: &[u8],
) -> Result<Vec<u8>, String> {
    let path_len = u16::try_from(file_path_list.len())
        .map_err(|_| String::from("DevicePath demasiado largo para EFI_LOAD_OPTION"))?;
    let desc = CString16::try_from(description)
        .map_err(|_| String::from("descripcion invalida para EFI_LOAD_OPTION"))?;

    let mut data = Vec::new();
    data.extend_from_slice(&UEFI_LOAD_OPTION_ACTIVE.to_le_bytes());
    data.extend_from_slice(&path_len.to_le_bytes());
    for unit in desc.to_u16_slice_with_nul().iter() {
        data.extend_from_slice(&unit.to_le_bytes());
    }
    data.extend_from_slice(file_path_list);
    data.extend_from_slice(optional_data);
    Ok(data)
}

fn parse_boot_option_id_from_name(name: &str) -> Option<u16> {
    if name.len() != 8 || !name.starts_with("Boot") {
        return None;
    }
    u16::from_str_radix(&name[4..], 16).ok()
}

fn extract_boot_option_file_path(data: &[u8]) -> Option<&[u8]> {
    if data.len() < 6 {
        return None;
    }
    let file_path_len = u16::from_le_bytes([data[4], data[5]]) as usize;
    let mut off = 6usize;
    while off + 1 < data.len() {
        if data[off] == 0 && data[off + 1] == 0 {
            off += 2;
            break;
        }
        off += 2;
    }
    if off > data.len() || off + file_path_len > data.len() {
        return None;
    }
    Some(&data[off..off + file_path_len])
}

fn find_existing_boot_option_for_path(file_path: &[u8]) -> Result<Option<u16>, String> {
    let vendor = uefi::runtime::VariableVendor::GLOBAL_VARIABLE;

    for key_result in uefi::runtime::variable_keys() {
        let key = key_result
            .map_err(|err| alloc::format!("iterando variables UEFI: {:?}", err.status()))?;
        if key.vendor != vendor {
            continue;
        }
        let name: String = String::from(&key.name);
        let Some(id) = parse_boot_option_id_from_name(name.as_str()) else {
            continue;
        };

        let entry = match uefi::runtime::get_variable_boxed(key.name.as_ref(), &vendor) {
            Ok((bytes, _attrs)) => bytes,
            Err(_) => continue,
        };
        let Some(existing_path) = extract_boot_option_file_path(entry.as_ref()) else {
            continue;
        };
        if existing_path == file_path {
            return Ok(Some(id));
        }
    }

    Ok(None)
}

fn find_free_boot_option_id() -> Result<u16, String> {
    let vendor = uefi::runtime::VariableVendor::GLOBAL_VARIABLE;
    let mut used = Vec::new();

    for key_result in uefi::runtime::variable_keys() {
        let key = key_result
            .map_err(|err| alloc::format!("iterando variables UEFI: {:?}", err.status()))?;
        if key.vendor != vendor {
            continue;
        }
        let name: String = String::from(&key.name);
        if let Some(id) = parse_boot_option_id_from_name(name.as_str()) {
            used.push(id);
        }
    }

    for candidate in 0u16..=u16::MAX {
        if !used.iter().any(|id| *id == candidate) {
            return Ok(candidate);
        }
    }

    Err(String::from("sin identificador libre para Boot####"))
}

fn read_boot_order() -> Result<Vec<u16>, String> {
    let vendor = uefi::runtime::VariableVendor::GLOBAL_VARIABLE;
    let (raw, _attrs) = match uefi::runtime::get_variable_boxed(uefi::cstr16!("BootOrder"), &vendor) {
        Ok(v) => v,
        Err(err) => {
            if err.status() == Status::NOT_FOUND {
                return Ok(Vec::new());
            }
            return Err(alloc::format!("leyendo BootOrder: {:?}", err.status()));
        }
    };

    let mut order = Vec::new();
    let mut i = 0usize;
    while i + 1 < raw.len() {
        order.push(u16::from_le_bytes([raw[i], raw[i + 1]]));
        i += 2;
    }
    Ok(order)
}

fn write_boot_order(order: &[u16]) -> Result<(), String> {
    let vendor = uefi::runtime::VariableVendor::GLOBAL_VARIABLE;
    let attrs = uefi::runtime::VariableAttributes::NON_VOLATILE
        | uefi::runtime::VariableAttributes::BOOTSERVICE_ACCESS
        | uefi::runtime::VariableAttributes::RUNTIME_ACCESS;

    let mut data = Vec::new();
    for id in order.iter() {
        data.extend_from_slice(&id.to_le_bytes());
    }

    uefi::runtime::set_variable(uefi::cstr16!("BootOrder"), &vendor, attrs, data.as_slice())
        .map_err(|err| alloc::format!("escribiendo BootOrder: {:?}", err.status()))
}

fn ensure_boot_order_contains(id: u16) -> Result<(), String> {
    let mut order = read_boot_order()?;
    order.retain(|existing| *existing != id);
    order.insert(0, id);
    write_boot_order(order.as_slice())
}

fn write_boot_next(id: u16) -> Result<(), String> {
    let vendor = uefi::runtime::VariableVendor::GLOBAL_VARIABLE;
    let attrs = uefi::runtime::VariableAttributes::NON_VOLATILE
        | uefi::runtime::VariableAttributes::BOOTSERVICE_ACCESS
        | uefi::runtime::VariableAttributes::RUNTIME_ACCESS;
    let data = id.to_le_bytes();
    uefi::runtime::set_variable(uefi::cstr16!("BootNext"), &vendor, attrs, &data)
        .map_err(|err| alloc::format!("escribiendo BootNext: {:?}", err.status()))
}

fn write_boot_option_variable(id: u16, data: &[u8]) -> Result<(), String> {
    let vendor = uefi::runtime::VariableVendor::GLOBAL_VARIABLE;
    let attrs = uefi::runtime::VariableAttributes::NON_VOLATILE
        | uefi::runtime::VariableAttributes::BOOTSERVICE_ACCESS
        | uefi::runtime::VariableAttributes::RUNTIME_ACCESS;
    let name = CString16::try_from(alloc::format!("Boot{:04X}", id).as_str())
        .map_err(|_| String::from("nombre Boot#### invalido"))?;

    uefi::runtime::set_variable(name.as_ref(), &vendor, attrs, data)
        .map_err(|err| alloc::format!("escribiendo Boot{:04X}: {:?}", id, err.status()))
}

fn reset_global_fat_mount_state() {
    unsafe {
        crate::fat32::GLOBAL_FAT.unmount();
    }
}

fn shell_loop(mut current_cluster: u32) -> ! {
    let fs_state = unsafe { &mut crate::fat32::GLOBAL_FAT };
    let mut line = [0u8; LINE_MAX];
    let mut len = 0usize;

    prompt();


    loop {
        let tick = timer::on_tick();
        scheduler::on_tick(tick);

        if let Some(event) = poll_input_event() {
            match event {
                InputEvent::Char(ch) => {
                    if ch.is_ascii() {
                        if len < LINE_MAX - 1 {
                            line[len] = ch as u8;
                            len += 1;
                            print_char(ch);
                        }
                    }
                }
                InputEvent::Backspace => {
                    if len > 0 {
                        len -= 1;
                        backspace_echo();
                    }
                }
                InputEvent::Enter => {
                    println("");
                    let cmd = core::str::from_utf8(&line[..len]).unwrap_or("").trim();
                    handle_command(cmd, fs_state, &mut current_cluster);
                    len = 0;
                    prompt();
                }
                InputEvent::Escape => {
                    println("");
                    println("Use command 'reboot' to restart.");
                    prompt();
                }
            }
        }

        uefi::boot::stall(LOOP_STALL_US);
    }
}

fn handle_command(cmd: &str, fat: &mut crate::fat32::Fat32, current_cluster: &mut u32) {
    if cmd.is_empty() {
        return;
    }

    if handle_fs_command(cmd, fat, current_cluster) {
        return;
    }

    if cmd == "help" {
        println("Commands:");
        println("  help           - show this help");
        println("  about          - system info");
        println("  clear          - clear screen");
        println("  mem            - memory map stats");
        println("  alloc          - allocate one 4KiB frame");
        println("  idt            - IDT skeleton info");
        println("  tick           - timer/uptime info");
        println("  sched          - scheduler stats");
        println("  step           - run +100 virtual ticks");
        println("  format         - format virtual disk as FAT32");
        println("  boot           - exit boot services and start stable polling runtime");
        println("  boot uefi      - start GUI without ExitBootServices (UEFI input: USB OK)");
        println("  boot irq       - start experimental PIT/IRQ runtime (auto fallback)");
        println("                   runtime shell runs in user-space via syscalls");
        println("  echo <text>    - print text");
        println("  panic          - panic test");
        println("  reboot         - reboot VM");
        println("  gui            - enter windowed desktop mode");
        println("  installer      - open graphical pre-boot installer");
        println("  disks          - list UEFI BlockIO devices (USB/NVMe/HDD)");
        println("  vols           - list mountable FAT32 volumes");
        println("  mount <n>      - mount FAT32 from BlockIO device index in 'disks'");
        println("  doom           - chainload external UEFI DOOM image (from USB)");
        println("  shell          - chainload external UEFI Shell image (SHELLX64.EFI)");
        println("  linux guest    - chainload Linux guest EFI loader (ruta 2: compat Linux real)");
        println("  net            - show network transport/IP status");
        println("  net dhcp       - switch to DHCP mode and request dynamic IP");
        println("  net static     - apply default static IP profile");
        println("  net static <ip> <prefijo> <gateway> - apply custom static IP");
        println("  net mode       - show current IP mode");
        println("  net https <on|off|status> - HTTPS compatibility mode");
        println("  net diag       - dump Intel Ethernet RX/TX registers");
        println("  wifi           - show Intel WiFi native driver status");
        println("  wifi scan      - scan WiFi networks (phase2 for real RF scan)");
        println("  wifi connect <ssid> <clave> - save profile and connect");
        println("  wifi disconnect - disconnect WiFi");
        println("  wifi failover <ethernet|wifi|status> - set automatic priority");
        return;
    }

    if cmd == "installer" {
        let result = preboot_installer::run();
        println("Kernel stage: installer returned.");
        match result {
            preboot_installer::InstallerResult::Installed => {
                reset_global_fat_mount_state();
                println("Preboot installer: install completed.");
                match ensure_installed_boot_option_registered() {
                    Ok(msg) => println(msg.as_str()),
                    Err(err) => println(alloc::format!("UEFI boot option: {}", err).as_str()),
                }
                println("Installer: rebooting now...");
                uefi::boot::stall(500_000);
                uefi::runtime::reset(ResetType::COLD, Status::SUCCESS, None);
            }
            preboot_installer::InstallerResult::Failed => {
                println("Preboot installer: finished with errors.");
            }
            preboot_installer::InstallerResult::Skipped => {
                println("Preboot installer: skipped.");
                println("Kernel stage: auto-launch GUI mode after installer.");
                uefi::boot::stall(300_000);
                start_gui_mode();
            }
        }
        return;
    }

    if cmd == "linux guest" || cmd == "lguest" {
        let current_handle = current_boot_device_handle();
        let installed_handle = find_installed_redux_handle(current_handle);
        println("Arranque: Linux guest...");
        match launch_linux_guest_boot(current_handle, installed_handle) {
            Ok(path) => println(alloc::format!("Arranque regresó desde {}.", path).as_str()),
            Err(err) => {
                println(alloc::format!("No se pudo arrancar Linux guest: {}", err).as_str());
                println("Rutas buscadas: \\EFI\\LINUX\\BOOTX64.EFI, \\EFI\\BOOT\\LINUX.EFI, \\boot\\vmlinuz.efi");
            }
        }
        return;
    }

    if cmd == "gui" {
        start_gui_mode();
    }

    if cmd == "doom" {
        match launch_doom_uefi() {
            Ok(path) => println(alloc::format!("DOOM: ejecucion finalizada ({})", path).as_str()),
            Err(err) => {
                println(alloc::format!("DOOM: {}", err).as_str());
                if doom_error_requires_shell(err.as_str()) {
                    println("DOOM: intentando abrir UEFI Shell + script DOOM automaticamente...");
                    match launch_doom_via_shell() {
                        Ok(path) => println(
                            alloc::format!("UEFI Shell (auto-DOOM): sesion finalizada ({})", path)
                                .as_str(),
                        ),
                        Err(shell_err) => {
                            println(alloc::format!("UEFI Shell (auto-DOOM): {}", shell_err).as_str())
                        }
                    }
                }
            }
        }
        return;
    }

    if cmd == "shell" {
        match launch_uefi_shell() {
            Ok(path) => println(alloc::format!("UEFI Shell: sesion finalizada ({})", path).as_str()),
            Err(err) => println(alloc::format!("UEFI Shell: {}", err).as_str()),
        }
        return;
    }

    if cmd == "about" {
        println("ReduxOS Phase 1 kernel prototype");
        println("Includes: memory + idt + timer + scheduler + syscall table");
        println("Runtime path: EBS + PIT IRQ + GOP desktop + userspace shell");
        return;
    }

    if let Some(args_raw) = cmd.strip_prefix("wifi ") {
        let args = args_raw.trim();

        if args == "scan" {
            let status = crate::intel_wifi::scan_networks();
            println(alloc::format!("WiFi: {}", status).as_str());
            let count = crate::intel_wifi::get_last_scan_count();
            if count == 0 {
                println("WiFi: no hay redes en la lista de escaneo.");
            } else {
                for i in 0..count {
                    if let Some(entry) = crate::intel_wifi::get_scan_entry(i) {
                        println(
                            alloc::format!(
                                "WiFi[{}]: '{}' RSSI={}dBm CH={} {}",
                                i,
                                entry.ssid_str(),
                                entry.rssi_dbm,
                                entry.channel,
                                if entry.secure { "secure" } else { "open" }
                            )
                            .as_str(),
                        );
                    }
                }
            }
            return;
        }

        if let Some(rest) = args.strip_prefix("connect ") {
            let mut parts = rest.trim().splitn(2, ' ');
            let ssid = parts.next().unwrap_or("").trim();
            let psk = parts.next().unwrap_or("").trim();
            if ssid.is_empty() {
                println("Usage: wifi connect <ssid> <clave>");
                return;
            }
            match crate::intel_wifi::configure_profile(ssid, psk) {
                Ok(msg) => println(alloc::format!("WiFi: {}", msg).as_str()),
                Err(err) => {
                    println(alloc::format!("WiFi: {}", err).as_str());
                    return;
                }
            }
            let result = crate::intel_wifi::connect_profile();
            println(alloc::format!("WiFi: {}", result).as_str());
            return;
        }

        if args == "disconnect" {
            println(alloc::format!("WiFi: {}", crate::intel_wifi::disconnect()).as_str());
            return;
        }

        if args == "profile" {
            if let Some(profile) = crate::intel_wifi::get_profile_info() {
                println(
                    alloc::format!(
                        "WiFi: perfil SSID='{}' secure={}",
                        profile.ssid_str(),
                        if profile.secure { "yes" } else { "no" }
                    )
                    .as_str(),
                );
            } else {
                println("WiFi: sin perfil configurado.");
            }
            return;
        }

        if args == "profile clear" {
            println(alloc::format!("WiFi: {}", crate::intel_wifi::clear_profile()).as_str());
            return;
        }

        if let Some(policy) = args.strip_prefix("failover ") {
            let mode = policy.trim();
            if mode.eq_ignore_ascii_case("ethernet") {
                crate::net::set_failover_policy_ethernet_first();
                println("WiFi: failover policy -> EthernetFirst");
                return;
            }
            if mode.eq_ignore_ascii_case("wifi") {
                crate::net::set_failover_policy_wifi_first();
                println("WiFi: failover policy -> WifiFirst");
                return;
            }
            if mode.eq_ignore_ascii_case("status") {
                println(alloc::format!("WiFi: failover policy -> {}", crate::net::get_failover_policy()).as_str());
                return;
            }
            println("Usage: wifi failover <ethernet|wifi|status>");
            return;
        }

        println("Usage: wifi <scan|connect|disconnect|profile|profile clear|failover>");
        return;
    }

    if cmd == "net" || cmd.starts_with("net ") {
        let args = cmd.strip_prefix("net").unwrap_or("").trim();

        if !args.is_empty() {
            let mut parts = args.split_whitespace();
            let sub = parts.next().unwrap_or("");

            if sub.eq_ignore_ascii_case("dhcp") {
                println(alloc::format!("Net: {}", crate::net::set_dhcp_mode()).as_str());
                return;
            }

            if sub.eq_ignore_ascii_case("static") {
                let ip_arg = parts.next();
                let prefix_arg = parts.next();
                let gw_arg = parts.next();
                let extra_arg = parts.next();

                if ip_arg.is_none() && prefix_arg.is_none() && gw_arg.is_none() && extra_arg.is_none() {
                    println(alloc::format!("Net: {}", crate::net::use_default_static_ipv4()).as_str());
                    return;
                }

                if let (Some(ip), Some(prefix), Some(gw), None) = (ip_arg, prefix_arg, gw_arg, extra_arg) {
                    match crate::net::set_static_ipv4_from_text(ip, prefix, gw) {
                        Ok(msg) => println(alloc::format!("Net: {}", msg).as_str()),
                        Err(err) => println(alloc::format!("Net: {}", err).as_str()),
                    }
                    return;
                }

                println("Usage: net static <ip> <prefijo> <gateway>");
                return;
            }

            if sub.eq_ignore_ascii_case("mode") {
                println(alloc::format!("Net: modo -> {}", crate::net::get_network_mode()).as_str());
                return;
            }

            if sub.eq_ignore_ascii_case("https") {
                let mode = parts.next().unwrap_or("status");
                if mode.eq_ignore_ascii_case("on") {
                    println(alloc::format!("Net: {}", crate::net::set_https_mode_proxy()).as_str());
                    return;
                }
                if mode.eq_ignore_ascii_case("off") {
                    println(alloc::format!("Net: {}", crate::net::set_https_mode_disabled()).as_str());
                    return;
                }
                if mode.eq_ignore_ascii_case("status") {
                    println(alloc::format!("Net: HTTPS mode -> {}", crate::net::get_https_mode()).as_str());
                    return;
                }
                println("Usage: net https <on|off|status>");
                return;
            }

            if sub.eq_ignore_ascii_case("diag") {
                if let Some(diag) = crate::intel_net::get_diagnostics() {
                    let rxq_en = (diag.rxdctl & 0x0200_0000) != 0;
                    let txq_en = (diag.txdctl & 0x0200_0000) != 0;
                    let link = (diag.status & 0x0000_0002) != 0;

                    println(
                        alloc::format!(
                            "NetDiag: PCI_CMD={:#010x} STATUS={:#010x} CTRL={:#010x} CTRL_EXT={:#010x}",
                            diag.pci_cmd, diag.status, diag.ctrl, diag.ctrl_ext
                        )
                        .as_str(),
                    );
                    println(
                        alloc::format!(
                            "NetDiag: RX RXCTRL={:#010x} RCTL={:#010x} RXDCTL={:#010x} RDH={} RDT={} RDLEN={} enabled={} ",
                            diag.rxctrl, diag.rctl, diag.rxdctl, diag.rdh, diag.rdt, diag.rdlen, rxq_en
                        )
                        .as_str(),
                    );
                    println(
                        alloc::format!(
                            "NetDiag: TX TCTL={:#010x} TXDCTL={:#010x} TDH={} TDT={} TDLEN={} enabled={}",
                            diag.tctl, diag.txdctl, diag.tdh, diag.tdt, diag.tdlen, txq_en
                        )
                        .as_str(),
                    );
                    println(
                        alloc::format!(
                            "NetDiag: IMS={:#010x} IMC={:#010x} LinkUp={} rx_cur={} tx_cur={}",
                            diag.ims, diag.imc, link, diag.rx_cur, diag.tx_cur
                        )
                        .as_str(),
                    );
                    println(
                        alloc::format!(
                            "NetDiag: SRRCTL={:#010x} HW_GPRC={} HW_GPTC={}",
                            diag.srrctl, diag.gprc, diag.gptc
                        )
                        .as_str(),
                    );
                    println(
                        alloc::format!(
                            "NetDiag: RXDESC[cur] addr={:#x} len={} status={:#04x} cso={:#04x} cmd={:#04x} css={:#04x} special={:#06x}",
                            diag.rx_desc_addr,
                            diag.rx_desc_length,
                            diag.rx_desc_status,
                            diag.rx_desc_cso,
                            diag.rx_desc_cmd,
                            diag.rx_desc_css,
                            diag.rx_desc_special
                        )
                        .as_str(),
                    );
                } else {
                    println("NetDiag: Intel Ethernet no inicializado.");
                }
                return;
            }

            println("Usage: net [dhcp|static|static <ip> <prefijo> <gateway>|mode|https|diag]");
            return;
        }

        let active = crate::net::get_active_transport();
        let dhcp_status = unsafe { crate::net::DHCP_STATUS };
        let (s_ip, s_prefix, s_gw) = crate::net::get_static_ipv4_config();
        println(alloc::format!("Net: Transporte activo -> {}", active).as_str());
        println(alloc::format!("Net: Failover policy -> {}", crate::net::get_failover_policy()).as_str());
        println(alloc::format!("Net: Modo IP -> {}", crate::net::get_network_mode()).as_str());
        println(alloc::format!("Net: HTTPS mode -> {}", crate::net::get_https_mode()).as_str());
        println(alloc::format!("Net: Estado IP -> {}", dhcp_status).as_str());
        println(
            alloc::format!(
                "Net: Perfil fija -> {}.{}.{}.{}/{} gw {}.{}.{}.{}",
                s_ip[0], s_ip[1], s_ip[2], s_ip[3], s_prefix, s_gw[0], s_gw[1], s_gw[2], s_gw[3]
            )
            .as_str(),
        );
        if let Some(ip) = crate::net::get_ip_address() {
            println(alloc::format!("Net: IP -> {}", ip).as_str());
        } else {
            println("Net: IP -> (sin asignar)");
        }
        if let Some(gw) = crate::net::get_gateway() {
            println(alloc::format!("Net: Gateway -> {}", gw).as_str());
        } else {
            println("Net: Gateway -> (none)");
        }

        if crate::intel_net::get_model_name().is_some() {
            let link = crate::intel_net::is_link_up();
            let (rx, tx) = crate::net::get_packet_stats();
            println(alloc::format!("Net: Ethernet link -> {}", if link { "UP" } else { "DOWN" }).as_str());
            println(alloc::format!("Net: Ethernet packets RX={} TX={}", rx, tx).as_str());
        }

        if crate::intel_wifi::is_present() {
            println(
                alloc::format!(
                    "Net: WiFi -> {} | datapath={}",
                    crate::intel_wifi::get_status(),
                    if crate::intel_wifi::is_data_path_ready() { "ready" } else { "pending" }
                )
                .as_str(),
            );
            if let Some((ssid, len)) = crate::intel_wifi::connected_ssid() {
                let ssid_str = core::str::from_utf8(&ssid[..len]).unwrap_or("<invalid-ssid>");
                println(alloc::format!("Net: WiFi conectado a '{}'", ssid_str).as_str());
            }
        }
        return;
    }

    if cmd == "wifi" {
        if !crate::intel_wifi::is_present() {
            println("WiFi: no Intel WiFi device detected.");
            return;
        }

        let model = crate::intel_wifi::get_model_name().unwrap_or("Intel WiFi (unknown)");
        println(alloc::format!("WiFi: model -> {}", model).as_str());
        println(alloc::format!("WiFi: status -> {}", crate::intel_wifi::get_status()).as_str());
        println(
            alloc::format!(
                "WiFi: datapath ready -> {}",
                if crate::intel_wifi::is_data_path_ready() { "yes" } else { "no (phase1)" }
            )
            .as_str(),
        );
        if let Some(hint) = crate::intel_wifi::firmware_hint() {
            println(alloc::format!("WiFi: firmware hint -> {}", hint).as_str());
        } else {
            println("WiFi: firmware hint -> (no hint for this device ID)");
        }
        if let Some((bus, slot, func)) = crate::intel_wifi::get_pci_location() {
            println(alloc::format!("WiFi: pci -> {}:{}.{}", bus, slot, func).as_str());
        }
        if let Some((ven, dev, subven, subdev)) = crate::intel_wifi::get_pci_ids() {
            println(
                alloc::format!(
                    "WiFi: ids -> {:04X}:{:04X} subsys {:04X}:{:04X}",
                    ven,
                    dev,
                    subven,
                    subdev
                )
                .as_str(),
            );
        }
        if let Some(rev) = crate::intel_wifi::get_revision() {
            println(alloc::format!("WiFi: revision -> {:#04x}", rev).as_str());
        }
        if let Some(cmd_reg) = crate::intel_wifi::get_command_reg() {
            println(alloc::format!("WiFi: PCI CMD -> {:#06x}", cmd_reg).as_str());
        }
        if let Some(mmio) = crate::intel_wifi::get_mmio_base() {
            println(alloc::format!("WiFi: BAR0 MMIO -> {:#x}", mmio).as_str());
        } else {
            println("WiFi: BAR0 MMIO -> unavailable");
        }
        if let Some(profile) = crate::intel_wifi::get_profile_info() {
            println(
                alloc::format!(
                    "WiFi: perfil -> '{}' secure={}",
                    profile.ssid_str(),
                    if profile.secure { "yes" } else { "no" }
                )
                .as_str(),
            );
        } else {
            println("WiFi: perfil -> (none)");
        }
        if let Some((ssid, len)) = crate::intel_wifi::connected_ssid() {
            let ssid_str = core::str::from_utf8(&ssid[..len]).unwrap_or("<invalid-ssid>");
            println(alloc::format!("WiFi: conectado -> {}", ssid_str).as_str());
        } else {
            println("WiFi: conectado -> no");
        }
        println(alloc::format!("WiFi: last scan -> {}", crate::intel_wifi::get_last_scan_status()).as_str());
        println(alloc::format!("WiFi: failover policy -> {}", crate::net::get_failover_policy()).as_str());
        return;
    }

    if cmd == "clear" {
        clear_screen();
        return;
    }

    if cmd == "mem" {
        let stats = memory::stats();
        let alloc = memory::allocator_state();
        let heap_bytes = allocator::heap_size_bytes() as u64;
        let heap_reserved = allocator::heap_reserved_bytes() as u64;
        with_stdout(|out| {
            let _ = writeln!(out, "Memory statistics:");
            let _ = writeln!(out, "  regions:              {}", stats.regions);
            let _ = writeln!(out, "  total pages:          {}", stats.total_pages);
            let _ = writeln!(out, "  conventional pages:   {}", stats.conventional_pages);
            let _ = writeln!(out, "  reserved pages:       {}", stats.reserved_pages);
            let _ = writeln!(
                out,
                "  heap reservado:       {} MiB ({} bytes)",
                heap_bytes / (1024 * 1024),
                heap_bytes
            );
            let _ = writeln!(
                out,
                "  heap reservado tarea: {} MiB ({} bytes)",
                heap_reserved / (1024 * 1024),
                heap_reserved
            );
            let _ = writeln!(
                out,
                "  largest conventional: {} pages ({} MiB)",
                stats.largest_conventional_pages,
                (stats.largest_conventional_pages * memory::PAGE_SIZE) / (1024 * 1024)
            );
            let _ = writeln!(
                out,
                "  conventional regions tracked: {}",
                alloc.tracked_regions
            );
        });
        return;
    }

    if cmd == "alloc" {
        match memory::alloc_frame() {
            Some(addr) => {
                let alloc = memory::allocator_state();
                with_stdout(|out| {
                    let _ = writeln!(
                        out,
                        "Allocated frame @ {:#x} (allocations={}, failed={})",
                        addr,
                        alloc.allocations,
                        alloc.failed_allocations
                    );
                });
            }
            None => println("Allocator has no free tracked frames."),
        }
        return;
    }

    if cmd == "idt" {
        let s = interrupts::summary();
        with_stdout(|out| {
            let _ = writeln!(out, "IDT skeleton:");
            let _ = writeln!(out, "  initialized: {}", s.initialized);
            let _ = writeln!(out, "  base:        {:#x}", s.base);
            let _ = writeln!(out, "  limit:       {}", s.limit);
            let _ = writeln!(out, "  handler:     {:#x}", s.sample_handler);
        });
        return;
    }

    if cmd == "tick" {
        let t = timer::snapshot();
        with_stdout(|out| {
            let _ = writeln!(out, "Timer:");
            let _ = writeln!(out, "  ticks:     {}", t.ticks);
            let _ = writeln!(out, "  tick_us:   {}", t.tick_us);
            let _ = writeln!(out, "  uptime_ms: {}", t.uptime_ms);
        });
        return;
    }

    if cmd == "sched" {
        let s = scheduler::snapshot();
        with_stdout(|out| {
            let _ = writeln!(out, "Scheduler:");
            let _ = writeln!(out, "  tick={} dispatches={} cursor={}", s.tick, s.dispatches, s.cursor);
            let _ = writeln!(out, "  tasks={}", s.task_count);

            let mut i = 0;
            while i < s.task_count {
                let t = s.tasks[i];
                let _ = writeln!(
                    out,
                    "    [{}] {} runs={} period={} max={} active={}",
                    i,
                    t.name,
                    t.runs,
                    t.period_ticks,
                    t.max_runs,
                    t.active
                );
                i += 1;
            }
        });
        return;
    }

    if cmd == "step" {
        let mut i = 0;
        while i < 100 {
            let t = timer::on_tick();
            scheduler::on_tick(t);
            i += 1;
        }
        println("Ran 100 virtual scheduler ticks (diagnostic). Use 'sched' to inspect.");
        return;
    }

    if cmd == "boot uefi" {
        enter_runtime_uefi();
    }

    if cmd == "boot" {
        enter_runtime_kernel(runtime::RuntimeMode::Polling);
    }

    if cmd == "boot irq" {
        enter_runtime_kernel(runtime::RuntimeMode::IrqSafe);
    }

    if let Some(text) = cmd.strip_prefix("echo ") {
        println(text);
        return;
    }

    if cmd == "panic" {
        panic!("panic requested by user command");
    }

    if cmd == "reboot" {
        println("Rebooting...");
        uefi::runtime::reset(ResetType::COLD, Status::SUCCESS, None);
    }

    if cmd == "format" {
        println("Formatting virtual disk as FAT32...");
        format_disk();
        println("Format complete. Use 'ls' to verify.");
        return;
    }

    with_stdout(|out| {
        let _ = writeln!(out, "Unknown command: {}", cmd);
    });
}

pub(crate) fn launch_doom_uefi() -> core::result::Result<&'static str, String> {
    use uefi::boot::{self, LoadImageSource};
    use uefi::proto::device_path::build;
    use uefi::proto::device_path::DevicePath;
    use uefi::proto::BootPolicy;

    let parent_image = boot::image_handle();
    let image_candidates: [(&uefi::CStr16, &'static str); 13] = [
        (uefi::cstr16!("\\EFI\\DOOM\\DOOMX64.EFI"), "\\EFI\\DOOM\\DOOMX64.EFI"),
        (uefi::cstr16!("\\EFI\\DOOM\\BOOTX64.EFI"), "\\EFI\\DOOM\\BOOTX64.EFI"),
        (uefi::cstr16!("\\EFI\\DOOM\\DOOM.EFI"), "\\EFI\\DOOM\\DOOM.EFI"),
        (uefi::cstr16!("\\EFI\\DOOM\\doomx64.efi"), "\\EFI\\DOOM\\doomx64.efi"),
        (uefi::cstr16!("\\EFI\\DOOM\\doom.efi"), "\\EFI\\DOOM\\doom.efi"),
        (uefi::cstr16!("\\DOOM\\DOOMX64.EFI"), "\\DOOM\\DOOMX64.EFI"),
        (uefi::cstr16!("\\DOOM\\DOOM.EFI"), "\\DOOM\\DOOM.EFI"),
        (uefi::cstr16!("\\DOOM\\doomx64.efi"), "\\DOOM\\doomx64.efi"),
        (uefi::cstr16!("\\DOOM\\doom.efi"), "\\DOOM\\doom.efi"),
        (uefi::cstr16!("\\DOOMX64.EFI"), "\\DOOMX64.EFI"),
        (uefi::cstr16!("\\doomx64.efi"), "\\doomx64.efi"),
        (uefi::cstr16!("\\DOOM.EFI"), "\\DOOM.EFI"),
        (uefi::cstr16!("\\doom.efi"), "\\doom.efi"),
    ];

    let device_handles = collect_preferred_launch_handles();
    if device_handles.is_empty() {
        return Err(String::from(
            "no hay dispositivo de arranque disponible para lanzar DOOM",
        ));
    }

    let mut last_error = String::from("no se encontro ejecutable UEFI de Doom");

    for handle in device_handles.iter() {
        'candidate: for (path_cstr, path_label) in image_candidates.iter() {
            let mut path_vec: Vec<u8> = Vec::new();
            let full_path = {
                let Ok(device_path_proto) = boot::open_protocol_exclusive::<DevicePath>(*handle)
                else {
                    continue 'candidate;
                };

                let file_node = build::media::FilePath { path_name: *path_cstr };
                let mut builder = build::DevicePathBuilder::with_vec(&mut path_vec);
                for node in device_path_proto.node_iter() {
                    builder = match builder.push(&node) {
                        Ok(next) => next,
                        Err(_) => continue 'candidate,
                    };
                }
                builder = match builder.push(&file_node) {
                    Ok(next) => next,
                    Err(_) => continue 'candidate,
                };
                match builder.finalize() {
                    Ok(path) => path,
                    Err(_) => continue 'candidate,
                }
            };

            let image_handle = match boot::load_image(
                parent_image,
                LoadImageSource::FromDevicePath {
                    device_path: full_path,
                    boot_policy: BootPolicy::ExactMatch,
                },
            ) {
                Ok(h) => h,
                Err(err) => {
                    last_error = alloc::format!("LoadImage fallo: {:?}", err);
                    continue;
                }
            };

            match boot::start_image(image_handle) {
                Ok(()) => {
                    println("DOOM: la aplicacion termino y regreso al shell.");
                    return Ok(*path_label);
                }
                Err(err) => {
                    let status_raw = err.status().0;
                    if status_raw == usize::MAX {
                        last_error = alloc::format!(
                            "StartImage fallo: {:?}. Este DOOM.EFI requiere UEFI Shell (SHELLX64.EFI).",
                            err
                        );
                    } else {
                        last_error = alloc::format!("StartImage fallo: {:?}", err);
                    }
                }
            }
        }
    }

    println("DOOM: no encontre ejecutable UEFI utilizable.");
    println("Copia uno de estos archivos a tu USB:");
    println("  \\EFI\\DOOM\\DOOMX64.EFI");
    println("  \\EFI\\DOOM\\BOOTX64.EFI");
    println("  \\EFI\\DOOM\\DOOM.EFI");
    println("  \\EFI\\DOOM\\doom.efi");
    println("  \\DOOM\\DOOMX64.EFI");
    println("  \\DOOMX64.EFI");
    println("Tambien coloca tu WAD en \\DOOM\\ (ej. doom1.wad o freedoom1.wad).");
    Err(last_error)
}

pub(crate) fn doom_error_requires_shell(err: &str) -> bool {
    err.contains("requiere UEFI Shell")
        || err.contains("Status(18446744073709551615)")
        || err.contains("LoadImage fallo")
        || err.contains("NOT_FOUND")
}

fn collect_preferred_launch_handles() -> Vec<uefi::Handle> {
    use uefi::boot;
    use uefi::proto::loaded_image::LoadedImage;
    use uefi::proto::media::fs::SimpleFileSystem;

    let mut handles: Vec<uefi::Handle> = Vec::new();

    if let Ok(loaded) = boot::open_protocol_exclusive::<LoadedImage>(boot::image_handle()) {
        if let Some(device) = loaded.device() {
            handles.push(device);
        }
    }

    let mounted = unsafe { crate::fat32::GLOBAL_FAT.uefi_block_handle };
    if let Some(device) = mounted {
        if !handles.iter().any(|h| *h == device) {
            handles.push(device);
        }
    }

    if handles.is_empty() {
        if let Ok(fs_handles) = boot::find_handles::<SimpleFileSystem>() {
            for handle in fs_handles.iter() {
                handles.push(*handle);
            }
        }
    }

    handles
}

fn launch_uefi_shell_internal(load_options: Option<&str>) -> core::result::Result<&'static str, String> {
    use uefi::boot::{self, LoadImageSource};
    use uefi::proto::device_path::build;
    use uefi::proto::device_path::DevicePath;
    use uefi::proto::BootPolicy;
    use uefi::proto::loaded_image::LoadedImage;
    use uefi::proto::media::fs::SimpleFileSystem;
    use uefi::CString16;

    let parent_image = boot::image_handle();
    let image_candidates: [(&uefi::CStr16, &'static str); 10] = [
        (uefi::cstr16!("\\EFI\\TOOLS\\SHELLX64.EFI"), "\\EFI\\TOOLS\\SHELLX64.EFI"),
        (uefi::cstr16!("\\EFI\\TOOLS\\shellx64.efi"), "\\EFI\\TOOLS\\shellx64.efi"),
        (uefi::cstr16!("\\EFI\\SHELL\\SHELLX64.EFI"), "\\EFI\\SHELL\\SHELLX64.EFI"),
        (uefi::cstr16!("\\EFI\\SHELL\\shellx64.efi"), "\\EFI\\SHELL\\shellx64.efi"),
        (uefi::cstr16!("\\EFI\\BOOT\\SHELLX64.EFI"), "\\EFI\\BOOT\\SHELLX64.EFI"),
        (uefi::cstr16!("\\EFI\\BOOT\\shellx64.efi"), "\\EFI\\BOOT\\shellx64.efi"),
        (uefi::cstr16!("\\EFI\\SHELLX64.EFI"), "\\EFI\\SHELLX64.EFI"),
        (uefi::cstr16!("\\EFI\\shellx64.efi"), "\\EFI\\shellx64.efi"),
        (uefi::cstr16!("\\SHELLX64.EFI"), "\\SHELLX64.EFI"),
        (uefi::cstr16!("\\shellx64.efi"), "\\shellx64.efi"),
    ];

    let fs_handles = match boot::find_handles::<SimpleFileSystem>() {
        Ok(handles) => handles,
        Err(err) => return Err(alloc::format!("no hay volúmenes SimpleFS: {:?}", err)),
    };

    let mut last_error = String::from("no se encontro ejecutable UEFI Shell");
    let load_options_owned = match load_options {
        Some(text) => match CString16::try_from(text) {
            Ok(s) => Some(s),
            Err(_) => return Err(String::from("opciones de shell invalidas para UCS-2")),
        },
        None => None,
    };

    for handle in fs_handles.iter() {
        'candidate: for (path_cstr, path_label) in image_candidates.iter() {
            let mut path_vec: Vec<u8> = Vec::new();
            let full_path = {
                let Ok(device_path_proto) = boot::open_protocol_exclusive::<DevicePath>(*handle)
                else {
                    continue 'candidate;
                };

                let file_node = build::media::FilePath { path_name: *path_cstr };
                let mut builder = build::DevicePathBuilder::with_vec(&mut path_vec);
                for node in device_path_proto.node_iter() {
                    builder = match builder.push(&node) {
                        Ok(next) => next,
                        Err(_) => continue 'candidate,
                    };
                }
                builder = match builder.push(&file_node) {
                    Ok(next) => next,
                    Err(_) => continue 'candidate,
                };
                match builder.finalize() {
                    Ok(path) => path,
                    Err(_) => continue 'candidate,
                }
            };

            let image_handle = match boot::load_image(
                parent_image,
                LoadImageSource::FromDevicePath {
                    device_path: full_path,
                    boot_policy: BootPolicy::ExactMatch,
                },
            ) {
                Ok(h) => h,
                Err(err) => {
                    last_error = alloc::format!("LoadImage fallo: {:?}", err);
                    continue;
                }
            };

            if let Some(options) = load_options_owned.as_ref() {
                let mut loaded_image =
                    match boot::open_protocol_exclusive::<LoadedImage>(image_handle) {
                        Ok(proto) => proto,
                        Err(err) => {
                            last_error = alloc::format!("LoadedImage fallo: {:?}", err);
                            let _ = boot::unload_image(image_handle);
                            continue;
                        }
                    };
                let options_size = core::mem::size_of_val(options.as_slice_with_nul());
                unsafe {
                    loaded_image.set_load_options(
                        options.as_ptr().cast::<u8>(),
                        options_size as u32,
                    );
                }
            }

            match boot::start_image(image_handle) {
                Ok(()) => {
                    println("UEFI Shell: sesion terminada y regreso al shell.");
                    return Ok(*path_label);
                }
                Err(err) => {
                    let _ = boot::unload_image(image_handle);
                    let status_raw = err.status().0;
                    if status_raw == usize::MAX {
                        last_error = alloc::format!(
                            "StartImage fallo: {:?}. SHELLX64.EFI devolvio error interno.",
                            err
                        );
                    } else {
                        last_error = alloc::format!("StartImage fallo: {:?}", err);
                    }
                }
            }
        }
    }

    println("UEFI Shell: no encontre ejecutable utilizable.");
    println("Copia SHELLX64.EFI en una de estas rutas de tu USB:");
    println("  \\EFI\\TOOLS\\SHELLX64.EFI");
    println("  \\EFI\\SHELL\\SHELLX64.EFI");
    println("  \\EFI\\BOOT\\SHELLX64.EFI");
    println("  \\SHELLX64.EFI");
    Err(last_error)
}

pub(crate) fn launch_uefi_shell() -> core::result::Result<&'static str, String> {
    // Avoid automatic STARTUP.NSH execution (can relaunch boot entry and bounce USB state).
    launch_uefi_shell_internal(Some("-nostartup -nointerrupt -noversion"))
}

pub(crate) fn launch_doom_via_shell() -> core::result::Result<&'static str, String> {
    // Run deterministic script and skip STARTUP.NSH side effects.
    launch_uefi_shell_internal(Some(
        "-nostartup -nointerrupt -noversion -exit \\EFI\\TOOLS\\DOOMAUTO.NSH",
    ))
}

pub(crate) fn restore_gui_after_external_app() -> bool {
    // Some UEFI apps switch GOP mode. Re-capture framebuffer before GUI repaints.
    uefi::boot::stall(20_000);
    let Some(info) = capture_framebuffer_info() else {
        return false;
    };
    framebuffer::init(info);
    let _ = framebuffer::enable_backbuffer();
    input::reset_mouse_uefi();
    true
}

fn fat_label_to_string(label: &[u8; 11]) -> String {
    let mut end = label.len();
    while end > 0 && label[end - 1] == b' ' {
        end -= 1;
    }

    if end == 0 {
        return String::from("NO_LABEL");
    }

    let mut out = String::new();
    for b in &label[..end] {
        let ch = if b.is_ascii_graphic() || *b == b' ' {
            *b as char
        } else {
            '?'
        };
        out.push(ch);
    }
    out
}

fn handle_fs_command(cmd: &str, fat: &mut crate::fat32::Fat32, current_cluster: &mut u32) -> bool {
    use crate::fs::FileSystem;

    if cmd == "disks" {
        let devices = crate::fat32::Fat32::detect_uefi_block_devices();
        if devices.is_empty() {
            println("No UEFI BlockIO storage devices detected.");
            return true;
        }

        println("Detected BlockIO devices:");
        for dev in devices.iter() {
            let media = if dev.removable { "USB" } else { "NVME/HDD" };
            let scope = if dev.logical_partition { "part" } else { "disk" };
            with_stdout(|out| {
                let _ = writeln!(
                    out,
                    "  [{}] {} {}  {} MiB",
                    dev.index,
                    media,
                    scope,
                    dev.total_mib
                );
            });
        }
        println("Use 'mount <index>' to probe and mount FAT32 on a listed device.");
        return true;
    }

    if cmd == "vols" {
        let volumes = crate::fat32::Fat32::detect_uefi_fat_volumes();
        if volumes.is_empty() {
            println("No FAT32 volumes detected on UEFI BlockIO (USB/NVMe/HDD).");
            return true;
        }

        println("Detected FAT32 volumes:");
        for vol in volumes.iter() {
            let media = if vol.removable { "USB" } else { "NVME/HDD" };
            let scope = if vol.logical_partition { "part" } else { "disk" };
            let label = fat_label_to_string(&vol.volume_label);
            with_stdout(|out| {
                let _ = writeln!(
                    out,
                    "  [{}] {} {}  {} MiB  label='{}'  start_lba={}",
                    vol.index,
                    media,
                    scope,
                    vol.total_mib,
                    label,
                    vol.partition_start
                );
            });
        }
        return true;
    }

    if let Some(raw_idx) = cmd.strip_prefix("mount ") {
        let idx = match raw_idx.trim().parse::<usize>() {
            Ok(v) => v,
            Err(_) => {
                println("Usage: mount <index>   (see 'disks').");
                return true;
            }
        };

        match fat.mount_uefi_block_device(idx) {
            Ok(vol) => {
                *current_cluster = vol.root_cluster;
                let label = fat_label_to_string(&vol.volume_label);
                let media = if vol.removable { "USB" } else { "NVME/HDD" };
                with_stdout(|out| {
                    let _ = writeln!(
                        out,
                        "Mounted volume [{}]: {} label='{}' root_cluster={} start_lba={}",
                        vol.index,
                        media,
                        label,
                        vol.root_cluster,
                        vol.partition_start
                    );
                });
            }
            Err(e) => {
                println(e);
            }
        }

        return true;
    }

    if cmd == "ls" {
        // Try init if not already done
        if fat.init_status != crate::fat32::InitStatus::Success {
             fat.init();
        }
        
        // If still not ready, just give error
        if fat.init_status != crate::fat32::InitStatus::Success {
            println("Filesystem not available. Use 'disks' and then 'mount <n>'.");
            return true;
        }
        
        if *current_cluster == 0 { *current_cluster = fat.root_cluster; }

        if let Ok(entries) = fat.read_dir_entries(*current_cluster) {
            println("Files:");
            
            // Print Volume Label only at root
            if *current_cluster == fat.root_cluster && fat.volume_label[0] != 0 {
                let label = core::str::from_utf8(&fat.volume_label).unwrap_or("UNKNOWN");
                with_stdout(|out| {
                        let _ = writeln!(out, "  [VOL ] {}", label);
                });
            }

            let mut count = 0;
            for entry in entries.iter() {
                if entry.valid {
                    let name = entry.full_name();
                    let type_str = match entry.file_type {
                        crate::fs::FileType::Directory => "DIR ",
                        crate::fs::FileType::File => "FILE",
                    };
                    with_stdout(|out| {
                        let _ = writeln!(out, "  [{}] {} ({} bytes)", type_str, name.as_str(), entry.size);
                    });
                    count += 1;
                }
            }
            if count == 0 {
                println("  (No files found)");
            }
        } else {
            println("Failed to read directory.");
        }
        return true;
    }

    if let Some(dir_name) = cmd.strip_prefix("cd ") {
         // Init check
         if fat.bytes_per_sector == 0 {
             if !fat.init() { println("FAT32 Init Failed"); return true; }
             *current_cluster = fat.root_cluster;
         }

         let target = dir_name.trim();
         
         // Handle ".."
         if target == ".." {
             // FAT32 usually has ".." entry in subdirs.
             // If we are at root, do nothing.
             if *current_cluster == fat.root_cluster {
                 return true;
             }
             
             // Scan current dir for ".."
             if let Ok(entries) = fat.read_dir_entries(*current_cluster) {
                 for entry in entries.iter() {
                     if entry.matches_name("..") {
                         // "." and ".." clusters often 0 for root in some implementations,
                         // or the actual root cluster number.
                         if entry.cluster == 0 {
                             *current_cluster = fat.root_cluster;
                         } else {
                             *current_cluster = entry.cluster;
                         }
                         return true;
                     }
                 }
                 // If no ".." found (weird), maybe just go root?
                 *current_cluster = fat.root_cluster; 
             }
             return true;
         }

         // Handle Normal Dir
         if let Ok(entries) = fat.read_dir_entries(*current_cluster) {
             let mut found = false;
             for entry in entries.iter() {
                 if entry.valid && entry.file_type == crate::fs::FileType::Directory {
                     if entry.matches_name(target) {
                         *current_cluster = if entry.cluster == 0 {
                             fat.root_cluster
                         } else {
                             entry.cluster
                         };
                         found = true;
                         break;
                     }
                 }
             }
             if !found {
                 println("Directory not found.");
             }
         }
         return true;
    }

    if let Some(filename) = cmd.strip_prefix("cat ") {
        if fat.bytes_per_sector == 0 {
             if !fat.init() { println("FAT32 Init Failed"); return true; }
             *current_cluster = fat.root_cluster;
        }

        if let Ok(entries) = fat.read_dir_entries(*current_cluster) {
            let mut found = false;
            for entry in entries.iter() {
                if entry.valid && entry.matches_name(filename.trim()) {
                    let target = (entry.size as usize).min(16 * 1024);
                    let mut buffer = Vec::new();
                    buffer.resize(target, 0);

                    match fat.read_file_sized(entry.cluster, target, &mut buffer) {
                        Ok(len) => {
                            println("Content:");
                            let s = core::str::from_utf8(&buffer[0..len]).unwrap_or("<binary>");
                            println(s);
                            if (entry.size as usize) > target {
                                println("[output truncated]");
                            }
                        }
                        Err(_) => println("Read error."),
                    }
                    found = true;
                    break;
                }
            }
            if !found {
                println("File not found.");
            }
        }
        return true;
    }
    
    false
}

fn format_disk() {
    println("Formatting Disk with FAT32 BPB...");
    // Construct minimal FAT32 BPB (Sector 0)
    let mut sector = [0u8; 512];
    
    // Jump Code: EB 58 90
    sector[0] = 0xEB; sector[1] = 0x58; sector[2] = 0x90;
    
    // OEM Name: "REDUXOS "
    sector[3] = b'R'; sector[4] = b'E'; sector[5] = b'D'; sector[6] = b'U'; 
    sector[7] = b'X'; sector[8] = b'O'; sector[9] = b'S'; sector[10] = b' ';
    
    // Bytes per Sector: 512 (0x0200)
    sector[11] = 0x00; sector[12] = 0x02;
    
    // Sectors per Cluster: 1
    sector[13] = 1;
    
    // Reserved Sectors: 32 (0x0020)
    sector[14] = 0x20; sector[15] = 0x00;
    
    // FATs: 2
    sector[16] = 2;
    
    // Root Entries: 0 (FAT32)
    sector[17] = 0; sector[18] = 0;
    
    // Small Sectors: 0 (Use Large)
    sector[19] = 0; sector[20] = 0;
    
    // Media: F8 (Fixed)
    sector[21] = 0xF8;
    
    // Sectors per FAT (16): 0
    sector[22] = 0; sector[23] = 0;
    
    // Sectors per Track: 32 (0x0020)
    sector[24] = 0x20; sector[25] = 0x00;
    
    // Heads: 16 (0x0010)
    sector[26] = 0x10; sector[27] = 0x00;
    
    // Hidden Sectors: 0
    sector[28] = 0; sector[29] = 0; sector[30] = 0; sector[31] = 0;
    
    // Large Sectors: 131072 (64MB) -> 0x00020000
    sector[32] = 0x00; sector[33] = 0x00; sector[34] = 0x02; sector[35] = 0x00;
    
    // Sectors per FAT (32): 1024 (0x00000400)
    // 131072 clusters * 4 bytes = 512KB. 512KB / 512 = 1024 sectors.
    sector[36] = 0x00; sector[37] = 0x04; sector[38] = 0x00; sector[39] = 0x00;
    
    // Flags: 0
    sector[40] = 0; sector[41] = 0;
    
    // Version: 0
    sector[42] = 0; sector[43] = 0;
    
    // Root Cluster: 2
    sector[44] = 2; sector[45] = 0; sector[46] = 0; sector[47] = 0;
    
    // FS Info Sector: 1
    sector[48] = 1; sector[49] = 0;
    
    // Backup Boot: 6
    sector[50] = 6; sector[51] = 0;
    
    // Drive Number: 0x80
    sector[64] = 0x80;
    
    // Signature: 0x29
    sector[66] = 0x29;
    
    // Volume ID: 0x12345678
    sector[67] = 0x78; sector[68] = 0x56; sector[69] = 0x34; sector[70] = 0x12;
    
    // Volume Label: "NO NAME    "
    let label = b"NO NAME    ";
    for i in 0..11 { sector[71+i] = label[i]; }
    
    // FS Type: "FAT32   "
    let fstype = b"FAT32   ";
    for i in 0..8 { sector[82+i] = fstype[i]; }
    
    // Boot Signature: 0xAA55
    sector[510] = 0x55; sector[511] = 0xAA;
    
    use crate::virtio::block;
    if block::write(0, &sector) {
        println("Format: Wrote MBR/BPB to Sector 0.");
    } else {
        println("Format: Failed to write Sector 0.");
    }
    
    // Clean FAT1 (Sector 32 .. 32+1024)
    let mut fat_sector = [0u8; 512];
    // Entry 0
    fat_sector[0] = 0xF8; fat_sector[1] = 0xFF; fat_sector[2] = 0xFF; fat_sector[3] = 0x0F;
    // Entry 1
    fat_sector[4] = 0xFF; fat_sector[5] = 0xFF; fat_sector[6] = 0xFF; fat_sector[7] = 0x0F; 
    // Entry 2 (Root Dir) - End of Chain (0x0FFFFFFF)
    fat_sector[8] = 0xFF; fat_sector[9] = 0xFF; fat_sector[10] = 0xFF; fat_sector[11] = 0x0F;
    
    // Write detailed FAT sector
    // Start of FAT1 = Reserved = 32.
    block::write(32, &fat_sector);
    
    // Clean the root directory cluster (cluster 2 = sector 1088)
    // Data starts at sector 32 + (2 * 1024) = 2080
    // Cluster 2 LBA = 2080
    let root_lba = 2080u64;
    let mut root_sector = [0u8; 512];
    
    // Create sample directory entries
    // Entry 1: "README.TXT" file (123 bytes, cluster 3)
    root_sector[0..11].copy_from_slice(b"README  TXT");
    root_sector[11] = 0x20; // Archive attribute
    root_sector[26] = 3; root_sector[27] = 0; // Cluster low word
    root_sector[28] = 123; root_sector[29] = 0; root_sector[30] = 0; root_sector[31] = 0; // Size 123 bytes
    
    // Entry 2: "DOCS" directory (cluster 4)
    root_sector[32..43].copy_from_slice(b"DOCS       ");
    root_sector[43] = 0x10; // Directory attribute
    root_sector[58] = 4; root_sector[59] = 0; // Cluster low word
    root_sector[60] = 0; root_sector[61] = 0; root_sector[62] = 0; root_sector[63] = 0; // Size 0 for dir
    
    // Entry 3: "TEST.TXT" file (456 bytes, cluster 5)
    root_sector[64..75].copy_from_slice(b"TEST    TXT");
    root_sector[75] = 0x20; // Archive attribute
    root_sector[90] = 5; root_sector[91] = 0; // Cluster low word
    root_sector[92] = 200; root_sector[93] = 1; root_sector[94] = 0; root_sector[95] = 0; // Size 456 bytes (0x1C8)
    
    block::write(root_lba, &root_sector);
    
    // Update FAT to mark clusters 3, 4, 5 as end-of-chain
    let mut fat_update = [0u8; 512];
    // Read current FAT sector
    block::read(32, &mut fat_update);
    
    // Cluster 3 (README.TXT) - End of Chain
    fat_update[12] = 0xFF; fat_update[13] = 0xFF; fat_update[14] = 0xFF; fat_update[15] = 0x0F;
    // Cluster 4 (DOCS dir) - End of Chain
    fat_update[16] = 0xFF; fat_update[17] = 0xFF; fat_update[18] = 0xFF; fat_update[19] = 0x0F;
    // Cluster 5 (TEST.TXT) - End of Chain
    fat_update[20] = 0xFF; fat_update[21] = 0xFF; fat_update[22] = 0xFF; fat_update[23] = 0x0F;
    
    block::write(32, &fat_update);
    
    println("Format: Created sample files (README.TXT, DOCS/, TEST.TXT)");
}

fn enter_runtime_kernel(mode: runtime::RuntimeMode) -> ! {
    println("Preparing runtime handoff...");

    let fb = match capture_framebuffer_info() {
        Some(info) => info,
        None => {
            println("Failed to capture GOP framebuffer. Rebooting...");
            uefi::runtime::reset(ResetType::COLD, Status::ABORTED, None);
        }
    };

    println("Exiting boot services...");

    // After boot-services handoff we run bare metal; keep IRQs off until
    // our own IDT/PIC/timer path is installed.
    interrupts::disable_irqs();
    let mmap = unsafe { uefi::boot::exit_boot_services(MemoryType::LOADER_DATA) };
    let stats = memory::init_from_existing_map(&mmap);

    runtime::enter_runtime(fb, stats, mode)
}

fn enter_runtime_uefi() -> ! {
    println("Preparing UEFI runtime (Boot Services remain active)...");

    let fb = match capture_framebuffer_info() {
        Some(info) => info,
        None => {
            println("Failed to capture GOP framebuffer. Rebooting...");
            uefi::runtime::reset(ResetType::COLD, Status::ABORTED, None);
        }
    };

    // memory::init_from_uefi() already ran during Phase 1 init; reuse cached stats.
    let stats = memory::stats();

    runtime::enter_runtime_uefi(fb, stats)
}

fn capture_framebuffer_info() -> Option<FramebufferInfo> {
    let handle = match uefi::boot::get_handle_for_protocol::<GraphicsOutput>() {
        Ok(h) => h,
        Err(_) => return None,
    };

    let mut gop = match uefi::boot::open_protocol_exclusive::<GraphicsOutput>(handle) {
        Ok(g) => g,
        Err(_) => return None,
    };

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

#[derive(Clone, Copy)]
enum InputEvent {
    Char(char),
    Enter,
    Backspace,
    Escape,
}

fn poll_input_event() -> Option<InputEvent> {
    uefi::system::with_stdin(|input| match input.read_key().ok().flatten() {
        Some(Key::Printable(c16)) => {
            let ch: char = c16.into();
            match ch {
                '\r' | '\n' => Some(InputEvent::Enter),
                '\u{8}' => Some(InputEvent::Backspace),
                _ => Some(InputEvent::Char(ch)),
            }
        }
        Some(Key::Special(ScanCode::ESCAPE)) => Some(InputEvent::Escape),
        _ => None,
    })
}

mod utils;

fn with_stdout<F>(f: F)
where
    F: Fn(&mut uefi::proto::console::text::Output),
{
    uefi::system::with_stdout(f);
}

pub fn println(msg: &str) {
    with_stdout(|out| {
        let _ = writeln!(out, "{}", msg);
    });
}

pub fn print(msg: &str) {
    with_stdout(|out| {
        let _ = write!(out, "{}", msg);
    });
}

pub fn println_num(n: u64) {
    with_stdout(|out| {
        let _ = writeln!(out, "{}", n);
    });
}

pub fn println_hex(n: u64) {
    with_stdout(|out| {
        let _ = writeln!(out, "{:#x}", n);
    });
}

fn prompt() {
    with_stdout(|out| {
        let _ = write!(out, "redux> ");
    });
}

pub fn print_char(ch: char) {
    with_stdout(|out| {
        let _ = write!(out, "{}", ch);
    });
}

fn backspace_echo() {
    with_stdout(|out| {
        let _ = write!(out, "\u{8} \u{8}");
    });
}
fn start_gui_mode() -> ! {
    let fb_info = match capture_framebuffer_info() {
        Some(info) => info,
        None => {
            println("Failed to capture GOP for GUI. Returning to shell...");
            loop { core::hint::spin_loop(); }
        }
    };

    println("Entering Desktop Mode...");
    framebuffer::init(fb_info);
    framebuffer::enable_backbuffer();

    let (width, height) = framebuffer::dimensions();
    input::reset_mouse_uefi();
    let mut compositor = gui::compositor::Compositor::new(width, height);
    
    // Create Desktop UI
    let _term_win_id = compositor.create_window("Terminal Shell", 100, 100, 800, 500);

    println("Entering Desktop Mode...");
    with_stdout(|out| {
        let _ = writeln!(out, "Resolution: {}x{}", width, height);
    });

    let mut _frame_count: u64 = 0;

    let mut current_mouse_x = compositor.mouse_pos.x;
    let mut current_mouse_y = compositor.mouse_pos.y;

    if !framebuffer::enable_backbuffer() {
        println("ERROR: Backbuffer failed! Capacity exceeded?");
    }

    loop {
        _frame_count += 1;
        
        // Timer update: the loop handles roughly 1ms per iteration due to uefi::boot::stall(1000)
        let tick = timer::on_tick();
        scheduler::on_tick(tick);
        
        // 1. Poll input (Keyboard)
        while let Some(input_event) = input::poll_input_uefi() {
            use crate::gui::{Event, KeyboardEvent, SpecialKey};
            let gui_event = match input_event {
                input::RuntimeInput::Char(ch) => Some(Event::Keyboard(KeyboardEvent {
                    key: Some(ch),
                    special: None,
                    down: true,
                })),
                input::RuntimeInput::Enter => Some(Event::Keyboard(KeyboardEvent {
                    key: Some('\n'),
                    special: None,
                    down: true,
                })),
                input::RuntimeInput::Backspace => Some(Event::Keyboard(KeyboardEvent {
                    key: Some('\x08'),
                    special: None,
                    down: true,
                })),
                input::RuntimeInput::Key(input::RuntimeKey::Esc) => Some(Event::Keyboard(
                    KeyboardEvent {
                        key: Some('\x1b'),
                        special: None,
                        down: true,
                    },
                )),
                input::RuntimeInput::Key(input::RuntimeKey::Up) => Some(Event::Keyboard(
                    KeyboardEvent {
                        key: None,
                        special: Some(SpecialKey::Up),
                        down: true,
                    },
                )),
                input::RuntimeInput::Key(input::RuntimeKey::Down) => Some(Event::Keyboard(
                    KeyboardEvent {
                        key: None,
                        special: Some(SpecialKey::Down),
                        down: true,
                    },
                )),
                input::RuntimeInput::Key(input::RuntimeKey::Left) => Some(Event::Keyboard(
                    KeyboardEvent {
                        key: None,
                        special: Some(SpecialKey::Left),
                        down: true,
                    },
                )),
                input::RuntimeInput::Key(input::RuntimeKey::Right) => Some(Event::Keyboard(
                    KeyboardEvent {
                        key: None,
                        special: Some(SpecialKey::Right),
                        down: true,
                    },
                )),
                _ => None,
            };
            
            if let Some(e) = gui_event {
                compositor.handle_event(e);
            }
        }

        // 2. Poll Mouse (USB/UEFI)
        while let Some((dx, dy, wheel_delta, left_btn, right_btn)) = input::poll_mouse_uefi() {
            current_mouse_x = current_mouse_x.saturating_add(dx);
            current_mouse_y = current_mouse_y.saturating_add(dy);
            current_mouse_x = current_mouse_x.clamp(0, width as i32 - 1);
            current_mouse_y = current_mouse_y.clamp(0, height as i32 - 1);

            use crate::gui::{Event, MouseEvent};
            compositor.handle_event(Event::Mouse(MouseEvent {
                x: current_mouse_x,
                y: current_mouse_y,
                left_down: left_btn,
                right_down: right_btn,
                wheel_delta,
            }));
        }
        
        crate::net::poll();

        // 4. Paint
        compositor.paint();

        // 5. Heartbeat (Blinking dot in corner to show system is alive)
        if _frame_count % 30 < 15 {
            framebuffer::rect(0, 0, 8, 8, 0x00FF00); // Green heartbeat
        }
        current_mouse_x = compositor.mouse_pos.x;
        current_mouse_y = compositor.mouse_pos.y;
        
        // Yield to firmware services; improves USB input stability on real hardware.
        uefi::boot::stall(1000);
    }
}

fn clear_screen() {
    with_stdout(|out| {
        let _ = out.clear();
    });
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println("");
    println("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    println("KERNEL PANIC!");
    if let Some(location) = info.location() {
        println(&alloc::format!("Location: {}:{}:{}", location.file(), location.line(), location.column()));
    }
    println(&alloc::format!("Message: {}", info.message()));
    println("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    
    loop {
        core::hint::spin_loop();
    }
}
