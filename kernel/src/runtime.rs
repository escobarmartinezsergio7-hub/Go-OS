use crate::{framebuffer, input, interrupts, memory, privilege, process, scheduler, syscall, timer, ui};
use crate::framebuffer::FramebufferInfo;
use crate::hal::pause;
use crate::input::{RuntimeInput, RuntimeKey};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use uefi::runtime::ResetType;
use uefi::Status;

const POLL_TICK_MS: u64 = 10;
const POLL_DELAY_SPINS_IDLE: usize = 80_000;
const POLL_DELAY_SPINS_ACTIVE: usize = 3_000;
const POLL_DELAY_SPINS_EVENT: usize = 1_000;
const PROCESS_DISPATCH_BUDGET_POLLING: usize = 1;
const PROCESS_DISPATCH_BUDGET_IRQ: usize = 4;
const IRQ_TIMER_HZ_DEFAULT: u32 = 100;
const IRQ_TIMER_HZ_MIN: u32 = 60;
const IRQ_TIMER_HZ_MAX: u32 = 240;
// Hardware startup can take longer than expected before first/second IRQ tick is observable.
const IRQ_STARTUP_SPINS: usize = 80_000_000;
// Runtime loop can iterate much faster than IRQ frequency; avoid false "stalled" fallback.
const IRQ_STALL_CYCLES: usize = 5_000_000;
// Enable IRQ path when explicitly requested (boot irq). Runtime still auto-fallbacks
// to polling if startup probe fails or stalls.
const ENABLE_EXPERIMENTAL_IRQ_PATH: bool = true;
// APIC timer routing is unstable on some real hardware in current build.
// Keep enabled so systems where PIC/PIT only emits one startup IRQ can still run in IRQ mode.
const ENABLE_APIC_TIMER_FALLBACK: bool = true;
const ENABLE_EXPERIMENTAL_PRIVILEGE_LAYERS: bool = true;
const RUNTIME_MODE_SWITCH_NONE: u8 = 0;
const RUNTIME_MODE_SWITCH_POLLING: u8 = 1;
const RUNTIME_MODE_SWITCH_IRQ: u8 = 2;

static RUNTIME_MODE_SWITCH_REQUEST: AtomicU8 = AtomicU8::new(RUNTIME_MODE_SWITCH_NONE);
static RUNTIME_UEFI_ACTIVE: AtomicBool = AtomicBool::new(false);
static IRQ_TIMER_HZ_OVERRIDE: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMode {
    Polling,
    IrqSafe,
}

pub fn request_runtime_mode(mode: RuntimeMode) {
    let value = match mode {
        RuntimeMode::Polling => RUNTIME_MODE_SWITCH_POLLING,
        RuntimeMode::IrqSafe => RUNTIME_MODE_SWITCH_IRQ,
    };
    RUNTIME_MODE_SWITCH_REQUEST.store(value, Ordering::SeqCst);
}

pub fn runtime_uefi_active() -> bool {
    RUNTIME_UEFI_ACTIVE.load(Ordering::SeqCst)
}

pub fn set_runtime_uefi_active(active: bool) {
    RUNTIME_UEFI_ACTIVE.store(active, Ordering::SeqCst);
}

#[inline]
fn sanitize_irq_timer_hz(hz: u32) -> u32 {
    hz.clamp(IRQ_TIMER_HZ_MIN, IRQ_TIMER_HZ_MAX)
}

pub fn set_irq_timer_target_hz(hz: Option<u32>) {
    let value = hz.map(sanitize_irq_timer_hz).unwrap_or(0);
    IRQ_TIMER_HZ_OVERRIDE.store(value, Ordering::SeqCst);
}

pub fn irq_timer_target_hz() -> u32 {
    let override_hz = IRQ_TIMER_HZ_OVERRIDE.load(Ordering::SeqCst);
    if override_hz == 0 {
        IRQ_TIMER_HZ_DEFAULT
    } else {
        sanitize_irq_timer_hz(override_hz)
    }
}

fn take_runtime_mode_request() -> Option<RuntimeMode> {
    match RUNTIME_MODE_SWITCH_REQUEST.swap(RUNTIME_MODE_SWITCH_NONE, Ordering::SeqCst) {
        RUNTIME_MODE_SWITCH_POLLING => Some(RuntimeMode::Polling),
        RUNTIME_MODE_SWITCH_IRQ => Some(RuntimeMode::IrqSafe),
        _ => None,
    }
}

// Service runtime mode switch requests from non-runtime loops (e.g. compositor loop in main.rs).
// Returns the resulting IRQ mode state.
pub fn service_mode_switch_non_runtime(current_irq_mode: bool) -> bool {
    let Some(requested) = take_runtime_mode_request() else {
        return current_irq_mode;
    };

    match requested {
        RuntimeMode::Polling => {
            interrupts::disable_irqs();
            interrupts::quiesce_firmware_apic();
            timer::init_polling(POLL_TICK_MS);
            privilege::linux_real_slice_configure_soft_preempt(true, 2048);
            process::reset_irq_preempt_hints();
            false
        }
        RuntimeMode::IrqSafe => {
            if ENABLE_EXPERIMENTAL_IRQ_PATH && try_start_irq_timer(false) {
                privilege::linux_real_slice_set_soft_preempt(false);
                true
            } else {
                interrupts::disable_irqs();
                interrupts::quiesce_firmware_apic();
                timer::init_polling(POLL_TICK_MS);
                privilege::linux_real_slice_configure_soft_preempt(true, 2048);
                process::reset_irq_preempt_hints();
                false
            }
        }
    }
}

#[inline]
fn ascii_lower(byte: u8) -> u8 {
    if byte >= b'A' && byte <= b'Z' {
        byte + 32
    } else {
        byte
    }
}

fn is_boot_command(cmd: &[u8]) -> bool {
    if cmd.len() < 4 {
        return false;
    }
    let boot = b"boot";
    let mut i = 0usize;
    while i < 4 {
        if ascii_lower(cmd[i]) != boot[i] {
            return false;
        }
        i += 1;
    }
    cmd.len() == 4 || cmd[4] == b' '
}

#[inline]
fn irq_trace(enabled: bool, msg: &'static str) {
    if enabled {
        ui::terminal_system_message(msg);
    }
}

fn try_start_irq_timer(trace: bool) -> bool {
    fn observe_irq_periodic(spins_first: usize, spins_second: usize) -> bool {
        let start_tick = timer::ticks();
        let start_irq = interrupts::irq0_count();
        let mut i = 0usize;
        while i < spins_first {
            let first_tick = timer::ticks().saturating_sub(start_tick);
            let first_irq = interrupts::irq0_count().saturating_sub(start_irq);
            if first_tick >= 1 || first_irq >= 1 {
                let mid_tick = timer::ticks();
                let mid_irq = interrupts::irq0_count();
                let mut j = 0usize;
                while j < spins_second {
                    let second_tick = timer::ticks().saturating_sub(mid_tick);
                    let second_irq = interrupts::irq0_count().saturating_sub(mid_irq);
                    if second_tick >= 1 || second_irq >= 1 {
                        return true;
                    }
                    pause();
                    j += 1;
                }
                return false;
            }
            pause();
            i += 1;
        }
        false
    }

    let irq_hz = irq_timer_target_hz();

    irq_trace(trace, "IRQ ARM STAGE 1/7: CLI");
    interrupts::disable_irqs();
    irq_trace(trace, "IRQ ARM STAGE 2/7: APIC QUIESCE");
    interrupts::quiesce_firmware_apic();
    irq_trace(trace, "IRQ ARM STAGE 3/7: IDT IRQ");
    let _ = interrupts::init_with_irq0();
    irq_trace(trace, "IRQ ARM STAGE 4/7: PIC REMAP");
    interrupts::remap_pic_for_timer_irq();
    irq_trace(trace, "IRQ ARM STAGE 5/7: PIT CONFIG");
    timer::configure_pit(irq_hz);
    irq_trace(trace, "IRQ ARM STAGE 6/7: STI");
    interrupts::enable_irqs();
    irq_trace(trace, "IRQ ARM STAGE 7/7: PROBE(>=2)");
    if observe_irq_periodic(IRQ_STARTUP_SPINS, IRQ_STARTUP_SPINS) {
        irq_trace(trace, "IRQ ARM OK: PIT TICKS RECIBIDOS");
        return true;
    }
    irq_trace(trace, "IRQ ARM FAIL: SIN TICKS INICIALES");

    if ENABLE_APIC_TIMER_FALLBACK {
        // APIC timer fallback for hardware where legacy PIC/PIT IRQ0 is not routed.
        irq_trace(trace, "IRQ APIC STAGE 1/5: CLI");
        interrupts::disable_irqs();
        irq_trace(trace, "IRQ APIC STAGE 2/5: APIC QUIESCE");
        interrupts::quiesce_firmware_apic();
        irq_trace(trace, "IRQ APIC STAGE 3/5: IDT IRQ");
        let _ = interrupts::init_with_irq0();
        irq_trace(trace, "IRQ APIC STAGE 4/5: START TIMER");
        timer::set_irq_tick_hz(irq_hz);
        if !interrupts::start_apic_timer_irq(irq_hz) {
            interrupts::quiesce_firmware_apic();
            irq_trace(trace, "IRQ APIC FAIL: START TIMER");
            return false;
        }
        irq_trace(trace, "IRQ APIC STAGE 5/5: STI+PROBE(>=2)");
        interrupts::enable_irqs();
        if observe_irq_periodic(IRQ_STARTUP_SPINS, IRQ_STARTUP_SPINS) {
            irq_trace(trace, "IRQ APIC OK: TICKS RECIBIDOS");
            return true;
        }
        irq_trace(trace, "IRQ APIC FAIL: SIN TICKS");
        interrupts::disable_irqs();
        interrupts::quiesce_firmware_apic();
    }

    interrupts::disable_irqs();
    interrupts::quiesce_firmware_apic();
    false
}

pub fn enter_runtime(
    framebuffer_info: FramebufferInfo,
    mem_stats: memory::MemoryStats,
    requested_mode: RuntimeMode,
) -> ! {
    RUNTIME_MODE_SWITCH_REQUEST.store(RUNTIME_MODE_SWITCH_NONE, Ordering::SeqCst);
    RUNTIME_UEFI_ACTIVE.store(false, Ordering::SeqCst);
    interrupts::disable_irqs();
    framebuffer::init(framebuffer_info);
    framebuffer::clear(framebuffer::rgb(0, 0, 0));
    let _ = framebuffer::enable_backbuffer();

    scheduler::init_demo();
    crate::worker_pool::init();

    // Auto-init SMP: discover CPUs + per-core scheduler + bootstrap APs
    crate::smp::discover_cpus();
    crate::per_core::init();
    crate::smp::bootstrap_aps();
    let mut mode = RuntimeMode::Polling;
    let staged_boot_irq_switch = requested_mode == RuntimeMode::IrqSafe;
    let mut irq_fallback_note: Option<&'static str> = None;
    timer::init_polling(POLL_TICK_MS);
    if staged_boot_irq_switch {
        if !ENABLE_EXPERIMENTAL_IRQ_PATH {
            irq_fallback_note = Some("IRQ MODE DISABLED -> POLLING");
        } else {
            irq_fallback_note = Some("BOOT IRQ: STAGED SWITCH (UI READY -> ARM IRQ)");
        }
    }
    if mode == RuntimeMode::IrqSafe {
        privilege::linux_real_slice_set_soft_preempt(false);
    } else {
        privilege::linux_real_slice_configure_soft_preempt(true, 2048);
    }

    syscall::init();
    process::init_user_space();
    process::reset_irq_preempt_hints();
    ui::terminal_reset(mode == RuntimeMode::IrqSafe);
    if let Some(note) = irq_fallback_note {
        ui::terminal_system_message(note);
    }
    if ENABLE_EXPERIMENTAL_PRIVILEGE_LAYERS {
        privilege::init_privilege_layers();
        ui::terminal_system_message("PRIV: GDT+TSS+SYSCALL READY");
    } else {
        ui::terminal_system_message("PRIV: HW LAYERS DISABLED (SAFE MODE)");
        ui::terminal_system_message("USE: PRIV / PRIV NEXT");
    }
    if staged_boot_irq_switch && ENABLE_EXPERIMENTAL_IRQ_PATH {
        ui::terminal_system_message("BOOT IRQ: REQUEST -> IRQ SAFE");
        request_runtime_mode(RuntimeMode::IrqSafe);
    }

    let mut running = true;
    let mut alt_theme = false;
    let mut last_render_tick = 0u64;
    let mut display_tick = 0u64;
    let mut process_tick = 0u64;
    let mut last_source_tick = 0u64;
    let mut pending_display_ticks = 0u64;
    let mut pending_process_ticks = 0u64;
    let mut stall_cycles = 0usize;

    loop {
        let mut force_render = false;
        let mut had_input = false;
        let mut command_fastpath_dispatched = false;

        while let Some(event) = input::poll_input() {
            had_input = true;
            match event {
                RuntimeInput::Key(key) => match key {
                    RuntimeKey::Esc => input::reboot_via_keyboard_controller(),
                    RuntimeKey::F1 => {
                        running = !running;
                        if running {
                            ui::terminal_system_message("STATE -> RUNNING");
                        } else {
                            ui::terminal_system_message("STATE -> PAUSED");
                        }
                        force_render = true;
                    }
                    RuntimeKey::F2 => {
                        alt_theme = !alt_theme;
                        force_render = true;
                    }
                    _ => {}
                },
                RuntimeInput::Char(ch) => {
                    if !running && mode == RuntimeMode::Polling && ch == ' ' {
                        display_tick = display_tick.saturating_add(1);
                        scheduler::on_tick(display_tick);
                    } else {
                        ui::terminal_input_char(ch);
                    }
                    force_render = true;
                }
                RuntimeInput::Backspace => {
                    ui::terminal_backspace();
                    force_render = true;
                }
                RuntimeInput::Enter => {
                    let mut cmd = [0u8; ui::TERM_MAX_INPUT];
                    let n = ui::terminal_copy_input_trim(&mut cmd);
                    ui::terminal_commit_input_line();
                    if n > 0 {
                        syscall::enqueue_command(&cmd[..n]);
                        // Keep boot mode switches out of this inline path so the UI can
                        // flush diagnostics before entering IRQ transition code.
                        if !is_boot_command(&cmd[..n]) {
                            process_tick = process_tick.saturating_add(1);
                            process::on_tick_core(0, process_tick);
                            command_fastpath_dispatched = true;
                        } else {
                            ui::terminal_system_message("BOOT CMD DEFERRED -> NEXT TICK");
                        }
                    }
                    force_render = true;
                }
                _ => {}
            }
        }

        if let Some(requested) = take_runtime_mode_request() {
            match requested {
                RuntimeMode::Polling => {
                    interrupts::disable_irqs();
                    timer::init_polling(POLL_TICK_MS);
                    mode = RuntimeMode::Polling;
                    privilege::linux_real_slice_configure_soft_preempt(true, 2048);
                    process::reset_irq_preempt_hints();
                    stall_cycles = 0;
                    last_source_tick = 0;
                    pending_display_ticks = 0;
                    pending_process_ticks = 0;
                    ui::terminal_system_message("RUNTIME SWITCH -> POLLING");
                    force_render = true;
                }
                RuntimeMode::IrqSafe => {
                    if ENABLE_EXPERIMENTAL_IRQ_PATH && try_start_irq_timer(true) {
                        mode = RuntimeMode::IrqSafe;
                        privilege::linux_real_slice_set_soft_preempt(false);
                        stall_cycles = 0;
                        last_source_tick = timer::ticks();
                        ui::terminal_system_message("RUNTIME SWITCH -> IRQ SAFE");
                    } else {
                        interrupts::disable_irqs();
                        timer::init_polling(POLL_TICK_MS);
                        mode = RuntimeMode::Polling;
                        privilege::linux_real_slice_configure_soft_preempt(true, 2048);
                        process::reset_irq_preempt_hints();
                        stall_cycles = 0;
                        last_source_tick = 0;
                        pending_display_ticks = 0;
                        pending_process_ticks = 0;
                        ui::terminal_system_message("RUNTIME SWITCH FAILED -> POLLING");
                    }
                    force_render = true;
                }
            }
        }

        let tick = match mode {
            RuntimeMode::Polling => timer::on_tick(),
            RuntimeMode::IrqSafe => timer::ticks(),
        };

        if mode == RuntimeMode::IrqSafe {
            if tick == last_source_tick {
                stall_cycles = stall_cycles.saturating_add(1);
                if stall_cycles > IRQ_STALL_CYCLES {
                    // Automatic fallback to stable polling mode.
                    interrupts::disable_irqs();
                    timer::init_polling(POLL_TICK_MS);
                    mode = RuntimeMode::Polling;
                    privilege::linux_real_slice_configure_soft_preempt(true, 2048);
                    process::reset_irq_preempt_hints();
                    ui::terminal_system_message("IRQ STALLED -> FALLBACK POLLING");
                    last_source_tick = 0;
                    pending_display_ticks = 0;
                    pending_process_ticks = 0;
                    last_render_tick = 0;
                    stall_cycles = 0;
                    continue;
                }
            } else {
                stall_cycles = 0;
            }
        }

        if running {
            let delta = tick.saturating_sub(last_source_tick);
            pending_display_ticks = pending_display_ticks.saturating_add(delta);
            pending_process_ticks = pending_process_ticks.saturating_add(delta);
            if pending_display_ticks > 0 {
                pending_display_ticks -= 1;
                display_tick = display_tick.saturating_add(1);
                scheduler::on_tick(display_tick);
            }
        }
        last_source_tick = tick;

        let pulled_irq_preempts = if mode == RuntimeMode::IrqSafe {
            process::sync_irq_preempt_hints()
        } else {
            0
        };
        let dispatch_budget = if mode == RuntimeMode::IrqSafe {
            PROCESS_DISPATCH_BUDGET_IRQ
        } else {
            PROCESS_DISPATCH_BUDGET_POLLING
        };

        syscall::set_runtime_state(display_tick, running, mode == RuntimeMode::IrqSafe);
        let mut process_dispatches = 0usize;
        while running && process_dispatches < dispatch_budget && pending_process_ticks > 0 {
            pending_process_ticks -= 1;
            process_tick = process_tick.saturating_add(1);
            process::on_tick_core(0, process_tick);
            process_dispatches += 1;
        }
        if running && had_input && !command_fastpath_dispatched && process_dispatches < dispatch_budget {
            process_tick = process_tick.saturating_add(1);
            process::on_tick_core(0, process_tick);
            process_dispatches += 1;
        }
        if mode == RuntimeMode::IrqSafe && running && pulled_irq_preempts > 0 && process_dispatches < dispatch_budget {
            let mut remaining = (pulled_irq_preempts as usize).min(dispatch_budget - process_dispatches);
            while remaining > 0 {
                process_tick = process_tick.saturating_add(1);
                process::on_tick_core(0, process_tick);
                process_dispatches += 1;
                remaining -= 1;
            }
        }

        // Process per-core jobs (BSP = core 0)
        crate::per_core::tick(0);

        if force_render || display_tick != last_render_tick {
            let snap = scheduler::snapshot();
            let dispatches = snap.dispatches.saturating_add(process::dispatches());
            ui::draw_desktop(
                display_tick,
                mem_stats.conventional_bytes() / (1024 * 1024),
                dispatches,
                interrupts::irq0_count(),
                alt_theme,
                running,
                mode == RuntimeMode::IrqSafe,
            );
            framebuffer::present();
            last_render_tick = display_tick;
        }

        // Adaptive delay: keep idle power low but reduce interactive latency.
        let spins = if had_input || force_render {
            POLL_DELAY_SPINS_EVENT
        } else if running {
            POLL_DELAY_SPINS_ACTIVE
        } else {
            POLL_DELAY_SPINS_IDLE
        };

        let mut i = 0usize;
        while i < spins {
            pause();
            i += 1;
        }
    }
}

// UEFI runtime path: keep Boot Services alive so USB keyboards work via UEFI input.
//
// This is intentionally "dev-mode": no ExitBootServices, no PIC/PIT, no IRQ remap.
// It exists to unblock real-hardware testing before xHCI/HID is implemented.
pub fn enter_runtime_uefi(framebuffer_info: FramebufferInfo, mem_stats: memory::MemoryStats) -> ! {
    RUNTIME_MODE_SWITCH_REQUEST.store(RUNTIME_MODE_SWITCH_NONE, Ordering::SeqCst);
    RUNTIME_UEFI_ACTIVE.store(true, Ordering::SeqCst);
    framebuffer::init(framebuffer_info);
    framebuffer::clear(framebuffer::rgb(0, 0, 0));
    let _ = framebuffer::enable_backbuffer();

    scheduler::init_demo();
    crate::worker_pool::init();

    // Auto-init SMP: discover CPUs + per-core scheduler + bootstrap APs
    crate::smp::discover_cpus();
    crate::per_core::init();
    let smp_aps = crate::smp::bootstrap_aps();
    timer::init_polling(POLL_TICK_MS);
    privilege::linux_real_slice_configure_soft_preempt(true, 2048);

    syscall::init();
    process::init_user_space();
    process::reset_irq_preempt_hints();
    ui::terminal_reset(false);
    ui::terminal_system_message("BOOT: UEFI MODE (BootServices alive)");
    ui::terminal_system_message("INPUT: UEFI keyboard (USB OK)");
    ui::terminal_system_message("NOTE: use 'boot' for true bare metal (needs PS/2 or legacy USB)");
    ui::terminal_system_message(
        alloc::format!("SMP: {} CPUs, {} APs online, per-core scheduler active",
            crate::smp::cpu_count(), smp_aps).as_str()
    );
    if ENABLE_EXPERIMENTAL_PRIVILEGE_LAYERS {
        privilege::init_privilege_layers_uefi_lite();
        let phase = privilege::current_phase();
        ui::terminal_system_message(
            alloc::format!("PRIV: UEFI init (lite) phase={} bridge_ready={}", phase, privilege::syscall_bridge_ready()).as_str()
        );
        ui::terminal_system_message("PRIV: UEFI MSR no aplicado. Usa 'priv init msr' para forzar.");
    }

    let mut running = true;
    let mut alt_theme = false;
    let mut mode = RuntimeMode::Polling;
    let mut last_render_tick = 0u64;
    let mut display_tick = 0u64;
    let mut process_tick = 0u64;
    let mut last_source_tick = 0u64;
    let mut pending_display_ticks = 0u64;
    let mut pending_process_ticks = 0u64;
    let mut stall_cycles = 0usize;

    loop {
        let mut force_render = false;
        let mut had_input = false;
        let mut command_fastpath_dispatched = false;

        while let Some(event) = input::poll_input_uefi() {
            had_input = true;
            match event {
                RuntimeInput::Key(key) => match key {
                    RuntimeKey::Esc => {
                        uefi::runtime::reset(ResetType::COLD, Status::SUCCESS, None);
                    }
                    RuntimeKey::F1 => {
                        running = !running;
                        if running {
                            ui::terminal_system_message("STATE -> RUNNING");
                        } else {
                            ui::terminal_system_message("STATE -> PAUSED");
                        }
                        force_render = true;
                    }
                    RuntimeKey::F2 => {
                        alt_theme = !alt_theme;
                        force_render = true;
                    }
                    _ => {}
                },
                RuntimeInput::Char(ch) => {
                    if !running && ch == ' ' {
                        display_tick = display_tick.saturating_add(1);
                        scheduler::on_tick(display_tick);
                    } else {
                        ui::terminal_input_char(ch);
                    }
                    force_render = true;
                }
                RuntimeInput::Backspace => {
                    ui::terminal_backspace();
                    force_render = true;
                }
                RuntimeInput::Enter => {
                    let mut cmd = [0u8; ui::TERM_MAX_INPUT];
                    let n = ui::terminal_copy_input_trim(&mut cmd);
                    ui::terminal_commit_input_line();
                    if n > 0 {
                        syscall::enqueue_command(&cmd[..n]);
                        process_tick = process_tick.saturating_add(1);
                        process::on_tick_core(0, process_tick);
                        command_fastpath_dispatched = true;
                    }
                    force_render = true;
                }
                _ => {}
            }
        }

        if let Some(requested) = take_runtime_mode_request() {
            match requested {
                RuntimeMode::Polling => {
                    interrupts::disable_irqs();
                    timer::init_polling(POLL_TICK_MS);
                    mode = RuntimeMode::Polling;
                    privilege::linux_real_slice_configure_soft_preempt(true, 2048);
                    process::reset_irq_preempt_hints();
                    stall_cycles = 0;
                    last_source_tick = 0;
                    pending_display_ticks = 0;
                    pending_process_ticks = 0;
                    ui::terminal_system_message("RUNTIME MODE: POLLING (UEFI)");
                }
                RuntimeMode::IrqSafe => {
                    // UEFI runtime keeps BootServices/input path alive; do not rewire PIC/APIC here.
                    interrupts::disable_irqs();
                    timer::init_polling(POLL_TICK_MS);
                    mode = RuntimeMode::Polling;
                    privilege::linux_real_slice_configure_soft_preempt(true, 2048);
                    process::reset_irq_preempt_hints();
                    stall_cycles = 0;
                    last_source_tick = 0;
                    pending_display_ticks = 0;
                    pending_process_ticks = 0;
                    ui::terminal_system_message("RUNTIME IRQ NO SOPORTADO EN UEFI -> POLLING");
                }
            }
            force_render = true;
        }

        let tick = match mode {
            RuntimeMode::Polling => timer::on_tick(),
            RuntimeMode::IrqSafe => timer::ticks(),
        };

        if mode == RuntimeMode::IrqSafe {
            if tick == last_source_tick {
                stall_cycles = stall_cycles.saturating_add(1);
                if stall_cycles > IRQ_STALL_CYCLES {
                    interrupts::disable_irqs();
                    timer::init_polling(POLL_TICK_MS);
                    mode = RuntimeMode::Polling;
                    privilege::linux_real_slice_configure_soft_preempt(true, 2048);
                    process::reset_irq_preempt_hints();
                    ui::terminal_system_message("IRQ STALLED -> FALLBACK POLLING (UEFI)");
                    last_source_tick = 0;
                    pending_display_ticks = 0;
                    pending_process_ticks = 0;
                    last_render_tick = 0;
                    stall_cycles = 0;
                    continue;
                }
            } else {
                stall_cycles = 0;
            }
        }

        // Preserve the same "one visible tick per loop" pacing used in the bare-metal runtime.
        if running {
            let delta = tick.saturating_sub(last_source_tick);
            pending_display_ticks = pending_display_ticks.saturating_add(delta);
            pending_process_ticks = pending_process_ticks.saturating_add(delta);
            if pending_display_ticks > 0 {
                pending_display_ticks -= 1;
                display_tick = display_tick.saturating_add(1);
                scheduler::on_tick(display_tick);
            }
        }
        last_source_tick = tick;

        let pulled_irq_preempts = if mode == RuntimeMode::IrqSafe {
            process::sync_irq_preempt_hints()
        } else {
            0
        };
        let dispatch_budget = if mode == RuntimeMode::IrqSafe {
            PROCESS_DISPATCH_BUDGET_IRQ
        } else {
            PROCESS_DISPATCH_BUDGET_POLLING
        };

        syscall::set_runtime_state(display_tick, running, mode == RuntimeMode::IrqSafe);
        let mut process_dispatches = 0usize;
        while running && process_dispatches < dispatch_budget && pending_process_ticks > 0 {
            pending_process_ticks -= 1;
            process_tick = process_tick.saturating_add(1);
            process::on_tick_core(0, process_tick);
            process_dispatches += 1;
        }
        if running && had_input && !command_fastpath_dispatched && process_dispatches < dispatch_budget {
            process_tick = process_tick.saturating_add(1);
            process::on_tick_core(0, process_tick);
            process_dispatches += 1;
        }
        if mode == RuntimeMode::IrqSafe && running && pulled_irq_preempts > 0 && process_dispatches < dispatch_budget {
            let mut remaining = (pulled_irq_preempts as usize).min(dispatch_budget - process_dispatches);
            while remaining > 0 {
                process_tick = process_tick.saturating_add(1);
                process::on_tick_core(0, process_tick);
                process_dispatches += 1;
                remaining -= 1;
            }
        }

        // Process per-core jobs (BSP = core 0)
        crate::per_core::tick(0);

        if force_render || display_tick != last_render_tick {
            let snap = scheduler::snapshot();
            let dispatches = snap.dispatches.saturating_add(process::dispatches());
            ui::draw_desktop(
                display_tick,
                mem_stats.conventional_bytes() / (1024 * 1024),
                dispatches,
                interrupts::irq0_count(),
                alt_theme,
                running,
                mode == RuntimeMode::IrqSafe,
            );
            framebuffer::present();
            last_render_tick = display_tick;
        }

        // Keep the exact same delay knobs to avoid re-tuning perceived smoothness.
        let spins = if had_input || force_render {
            POLL_DELAY_SPINS_EVENT
        } else {
            POLL_DELAY_SPINS_ACTIVE
        };

        let mut i = 0usize;
        while i < spins {
            pause();
            i += 1;
        }
    }
}
