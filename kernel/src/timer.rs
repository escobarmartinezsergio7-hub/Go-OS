use core::sync::atomic::{AtomicI32, AtomicI64, AtomicU64, Ordering};

use crate::hal::outb;

#[derive(Clone, Copy)]
pub struct TickSnapshot {
    pub ticks: u64,
    pub tick_us: u64,
    pub uptime_ms: u64,
}

static TICKS: AtomicU64 = AtomicU64::new(0);
static TICK_US: AtomicU64 = AtomicU64::new(10_000);
static WALL_CLOCK_BASE_UNIX_MS: AtomicI64 = AtomicI64::new(1_780_531_200_000);
static WALL_CLOCK_BASE_TICKS: AtomicU64 = AtomicU64::new(0);
static WALL_CLOCK_TZ_OFFSET_MINUTES: AtomicI32 = AtomicI32::new(-360);

fn elapsed_ms_since(base_ticks: u64) -> u64 {
    let ticks = TICKS.load(Ordering::SeqCst);
    let tick_us = TICK_US.load(Ordering::SeqCst);
    ticks.saturating_sub(base_ticks).saturating_mul(tick_us) / 1000
}

fn preserve_wall_clock_around_tick_reset() {
    let now_ms = wall_clock_unix_millis();
    WALL_CLOCK_BASE_UNIX_MS.store(now_ms, Ordering::SeqCst);
    WALL_CLOCK_BASE_TICKS.store(0, Ordering::SeqCst);
}

pub fn init_polling(tick_ms: u64) {
    preserve_wall_clock_around_tick_reset();
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

    set_irq_tick_hz(safe_hz);

    unsafe {
        // Channel 0, lobyte/hibyte, mode 3, binary.
        outb(0x43, 0x36);
        outb(0x40, (divisor & 0x00FF) as u8);
        outb(0x40, ((divisor >> 8) & 0x00FF) as u8);
    }
}

pub fn set_irq_tick_hz(hz: u32) {
    let safe_hz = hz.clamp(18, 1000);
    preserve_wall_clock_around_tick_reset();
    TICKS.store(0, Ordering::SeqCst);
    TICK_US.store(1_000_000u64 / safe_hz as u64, Ordering::SeqCst);
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

pub fn wall_clock_unix_millis() -> i64 {
    let base_ms = WALL_CLOCK_BASE_UNIX_MS.load(Ordering::SeqCst);
    let base_ticks = WALL_CLOCK_BASE_TICKS.load(Ordering::SeqCst);
    base_ms.saturating_add(elapsed_ms_since(base_ticks) as i64)
}

pub fn wall_clock_timezone_offset_minutes() -> i32 {
    WALL_CLOCK_TZ_OFFSET_MINUTES.load(Ordering::SeqCst)
}

pub fn set_wall_clock_unix_millis(unix_ms: i64, timezone_offset_minutes: i32) {
    WALL_CLOCK_TZ_OFFSET_MINUTES.store(timezone_offset_minutes, Ordering::SeqCst);
    WALL_CLOCK_BASE_UNIX_MS.store(unix_ms, Ordering::SeqCst);
    WALL_CLOCK_BASE_TICKS.store(TICKS.load(Ordering::SeqCst), Ordering::SeqCst);
}

pub fn set_wall_clock_timezone_offset_minutes(timezone_offset_minutes: i32) {
    WALL_CLOCK_TZ_OFFSET_MINUTES.store(timezone_offset_minutes, Ordering::SeqCst);
}

pub fn set_wall_clock_from_local_unix_seconds(local_seconds: i64, timezone_offset_minutes: i32) {
    let offset_ms = (timezone_offset_minutes as i64).saturating_mul(60_000);
    set_wall_clock_unix_millis(
        local_seconds.saturating_mul(1000).saturating_sub(offset_ms),
        timezone_offset_minutes,
    );
}
