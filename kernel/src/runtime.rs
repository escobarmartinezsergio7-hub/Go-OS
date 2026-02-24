use crate::{framebuffer, input, interrupts, memory, privilege, process, scheduler, syscall, timer, ui};
use crate::framebuffer::FramebufferInfo;
use crate::hal::pause;
use crate::input::{RuntimeInput, RuntimeKey};
use uefi::runtime::ResetType;
use uefi::Status;

const POLL_TICK_MS: u64 = 10;
const POLL_DELAY_SPINS_IDLE: usize = 80_000;
const POLL_DELAY_SPINS_ACTIVE: usize = 3_000;
const POLL_DELAY_SPINS_EVENT: usize = 1_000;
const IRQ_TIMER_HZ: u32 = 100;
const IRQ_STARTUP_SPINS: usize = 600_000;
const IRQ_STALL_CYCLES: usize = 200;
// Keep IRQ path disabled by default on real hardware because some systems
// hang before we can confirm stable timer interrupts.
const ENABLE_EXPERIMENTAL_IRQ_PATH: bool = false;
const ENABLE_EXPERIMENTAL_PRIVILEGE_LAYERS: bool = true;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMode {
    Polling,
    IrqSafe,
}

fn try_start_irq_timer() -> bool {
    interrupts::disable_irqs();
    let _ = interrupts::init_with_irq0();
    interrupts::remap_pic_for_timer_irq();
    timer::configure_pit(IRQ_TIMER_HZ);
    interrupts::enable_irqs();

    let start_tick = timer::ticks();
    let start_irq = interrupts::irq0_count();

    let mut i = 0usize;
    while i < IRQ_STARTUP_SPINS {
        if timer::ticks() != start_tick || interrupts::irq0_count() != start_irq {
            return true;
        }
        pause();
        i += 1;
    }

    // Failed to observe IRQ activity within startup window.
    interrupts::disable_irqs();
    false
}

pub fn enter_runtime(
    framebuffer_info: FramebufferInfo,
    mem_stats: memory::MemoryStats,
    requested_mode: RuntimeMode,
) -> ! {
    interrupts::disable_irqs();
    framebuffer::init(framebuffer_info);
    framebuffer::clear(framebuffer::rgb(0, 0, 0));
    let _ = framebuffer::enable_backbuffer();

    scheduler::init_demo();
    let mut mode = requested_mode;
    let mut irq_fallback_note: Option<&'static str> = None;
    if mode == RuntimeMode::IrqSafe {
        if !ENABLE_EXPERIMENTAL_IRQ_PATH {
            mode = RuntimeMode::Polling;
            timer::init_polling(POLL_TICK_MS);
            irq_fallback_note = Some("IRQ MODE DISABLED -> FALLBACK POLLING");
        } else if !try_start_irq_timer() {
            mode = RuntimeMode::Polling;
            timer::init_polling(POLL_TICK_MS);
            irq_fallback_note = Some("IRQ START FAILED -> FALLBACK POLLING");
        }
    } else {
        timer::init_polling(POLL_TICK_MS);
    }

    syscall::init();
    process::init_user_space();
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

    let mut running = true;
    let mut alt_theme = false;
    let mut last_render_tick = 0u64;
    let mut display_tick = 0u64;
    let mut last_source_tick = 0u64;
    let mut pending_source_ticks = 0u64;
    let mut stall_cycles = 0usize;

    loop {
        let mut force_render = false;
        let mut had_input = false;

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
                        // Fast-path: dispatch userspace right now to reduce command echo latency.
                        process::on_tick(display_tick.saturating_add(1));
                    }
                    force_render = true;
                }
                _ => {}
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
                    ui::terminal_system_message("IRQ STALLED -> FALLBACK POLLING");
                    last_source_tick = 0;
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
            pending_source_ticks = pending_source_ticks.saturating_add(delta);
            if pending_source_ticks > 0 {
                pending_source_ticks -= 1;
                display_tick = display_tick.saturating_add(1);
                scheduler::on_tick(display_tick);
            }
        }
        last_source_tick = tick;

        syscall::set_runtime_state(display_tick, running, mode == RuntimeMode::IrqSafe);
        process::on_tick(display_tick);

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
    framebuffer::init(framebuffer_info);
    framebuffer::clear(framebuffer::rgb(0, 0, 0));
    let _ = framebuffer::enable_backbuffer();

    scheduler::init_demo();
    timer::init_polling(POLL_TICK_MS);

    syscall::init();
    process::init_user_space();
    ui::terminal_reset(false);
    ui::terminal_system_message("BOOT: UEFI MODE (BootServices alive)");
    ui::terminal_system_message("INPUT: UEFI keyboard (USB OK)");
    ui::terminal_system_message("NOTE: use 'boot' for true bare metal (needs PS/2 or legacy USB)");

    let mut running = true;
    let mut alt_theme = false;
    let mut last_render_tick = 0u64;
    let mut display_tick = 0u64;
    let mut last_source_tick = 0u64;
    let mut pending_source_ticks = 0u64;

    loop {
        let mut force_render = false;
        let mut had_input = false;

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
                        process::on_tick(display_tick.saturating_add(1));
                    }
                    force_render = true;
                }
                _ => {}
            }
        }

        let tick = timer::on_tick();

        // Preserve the same "one visible tick per loop" pacing used in the bare-metal runtime.
        if running {
            let delta = tick.saturating_sub(last_source_tick);
            pending_source_ticks = pending_source_ticks.saturating_add(delta);
            if pending_source_ticks > 0 {
                pending_source_ticks -= 1;
                display_tick = display_tick.saturating_add(1);
                scheduler::on_tick(display_tick);
            }
        }
        last_source_tick = tick;

        syscall::set_runtime_state(display_tick, running, false);
        process::on_tick(display_tick);

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
                false,
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
