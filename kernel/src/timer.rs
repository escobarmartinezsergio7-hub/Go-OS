use core::sync::atomic::{AtomicU64, Ordering};

use crate::hal::outb;

#[derive(Clone, Copy)]
pub struct TickSnapshot {
    pub ticks: u64,
    pub tick_us: u64,
    pub uptime_ms: u64,
}

static TICKS: AtomicU64 = AtomicU64::new(0);
static TICK_US: AtomicU64 = AtomicU64::new(10_000);

pub fn init_polling(tick_ms: u64) {
    TICKS.store(0, Ordering::SeqCst);
    TICK_US.store(tick_ms.max(1).saturating_mul(1000), Ordering::SeqCst);
}

pub fn on_tick() -> u64 {
    TICKS.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn irq_tick() {
    TICKS.fetch_add(1, Ordering::SeqCst);
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::SeqCst)
}

pub fn configure_pit(hz: u32) {
    let safe_hz = hz.clamp(18, 1000);
    let divisor: u16 = (1_193_182u32 / safe_hz) as u16;

    TICKS.store(0, Ordering::SeqCst);
    TICK_US.store(1_000_000u64 / safe_hz as u64, Ordering::SeqCst);

    unsafe {
        // Channel 0, lobyte/hibyte, mode 3, binary.
        outb(0x43, 0x36);
        outb(0x40, (divisor & 0x00FF) as u8);
        outb(0x40, ((divisor >> 8) & 0x00FF) as u8);
    }
}

pub fn snapshot() -> TickSnapshot {
    let ticks = TICKS.load(Ordering::SeqCst);
    let tick_us = TICK_US.load(Ordering::SeqCst);
    TickSnapshot {
        ticks,
        tick_us,
        uptime_ms: ticks.saturating_mul(tick_us) / 1000,
    }
}
