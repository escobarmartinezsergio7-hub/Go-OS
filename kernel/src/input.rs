use crate::hal::{cli, hlt, inb, outb, pause};
use uefi::proto::console::text::{Key, ScanCode};

#[derive(Clone, Copy)]
pub enum RuntimeKey {
    Esc,
    F1,
    F2,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy)]
pub enum RuntimeInput {
    Key(RuntimeKey),
    Char(char),
    Enter,
    Backspace,
    Mouse { dx: i32, dy: i32, btn: bool },
}

static mut SHIFT_DOWN: bool = false;

fn decode_ascii(scancode: u8, shift: bool) -> Option<char> {
    const MAP: [char; 58] = [
        '\0', '\x1b', '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '-', '=', '\x08',
        '\t', 'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p', '[', ']', '\n', '\0', 'a',
        's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', ';', '\'', '`', '\0', '\\', 'z', 'x', 'c',
        'v', 'b', 'n', 'm', ',', '.', '/', '\0', '*', '\0', ' ',
    ];
    const MAP_SHIFT: [char; 58] = [
        '\0', '\x1b', '!', '@', '#', '$', '%', '^', '&', '*', '(', ')', '_', '+', '\x08',
        '\t', 'Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P', '{', '}', '\n', '\0', 'A',
        'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', ':', '"', '~', '\0', '|', 'Z', 'X', 'C',
        'V', 'B', 'N', 'M', '<', '>', '?', '\0', '*', '\0', ' ',
    ];

    let idx = scancode as usize;
    if idx >= MAP.len() {
        return None;
    }

    let ch = if shift { MAP_SHIFT[idx] } else { MAP[idx] };
    if ch == '\0' {
        None
    } else {
        Some(ch)
    }
}

pub fn poll_input() -> Option<RuntimeInput> {
    let status = unsafe { inb(0x64) };
    if (status & 0x01) == 0 {
        return None;
    }

    let scancode = unsafe { inb(0x60) };

    // Shift press/release.
    match scancode {
        0x2A | 0x36 => {
            unsafe { SHIFT_DOWN = true };
            return None;
        }
        0xAA | 0xB6 => {
            unsafe { SHIFT_DOWN = false };
            return None;
        }
        _ => {}
    }

    // Ignore other key release events.
    if (scancode & 0x80) != 0 {
        return None;
    }

    match scancode {
        0x01 => Some(RuntimeInput::Key(RuntimeKey::Esc)),
        0x3B => Some(RuntimeInput::Key(RuntimeKey::F1)),
        0x3C => Some(RuntimeInput::Key(RuntimeKey::F2)),
        0x0E => Some(RuntimeInput::Backspace),
        0x1C => Some(RuntimeInput::Enter),
        _ => {
            let shift = unsafe { SHIFT_DOWN };
            decode_ascii(scancode, shift).map(RuntimeInput::Char)
        }
    }
}

// UEFI keyboard input (USB works here). Only valid while Boot Services are active.
pub fn poll_input_uefi() -> Option<RuntimeInput> {
    uefi::system::with_stdin(|input| match input.read_key().ok().flatten() {
        Some(Key::Printable(c16)) => {
            let ch: char = c16.into();
            match ch {
                '\r' | '\n' => Some(RuntimeInput::Enter),
                '\u{8}' => Some(RuntimeInput::Backspace),
                _ => Some(RuntimeInput::Char(ch)),
            }
        }
        Some(Key::Special(sc)) => match sc {
            ScanCode::ESCAPE => Some(RuntimeInput::Key(RuntimeKey::Esc)),
            ScanCode::FUNCTION_1 => Some(RuntimeInput::Key(RuntimeKey::F1)),
            ScanCode::FUNCTION_2 => Some(RuntimeInput::Key(RuntimeKey::F2)),
            ScanCode::UP => Some(RuntimeInput::Key(RuntimeKey::Up)),
            ScanCode::DOWN => Some(RuntimeInput::Key(RuntimeKey::Down)),
            ScanCode::LEFT => Some(RuntimeInput::Key(RuntimeKey::Left)),
            ScanCode::RIGHT => Some(RuntimeInput::Key(RuntimeKey::Right)),
            _ => None,
        },
        _ => None,
    })
}

/// Raw pointers to opened Pointer protocols. Kept alive for the entire session.
static MOUSE_PTRS: [core::sync::atomic::AtomicUsize; 4] = [
    core::sync::atomic::AtomicUsize::new(0),
    core::sync::atomic::AtomicUsize::new(0),
    core::sync::atomic::AtomicUsize::new(0),
    core::sync::atomic::AtomicUsize::new(0),
];
static MOUSE_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

// ---------------------------------------------------------------------------
// EFI_ABSOLUTE_POINTER_PROTOCOL — raw implementation for touchpads
// UEFI spec: Section 12.7 — EFI Absolute Pointer Protocol
// GUID: 8D59D32B-C655-4AE9-9B15-F25904992A43
// ---------------------------------------------------------------------------

#[repr(C)]
struct AbsolutePointerMode {
    abs_min_x: u64,
    abs_min_y: u64,
    abs_min_z: u64,
    abs_max_x: u64,
    abs_max_y: u64,
    abs_max_z: u64,
    attributes: u32,
}

#[repr(C)]
struct AbsolutePointerState {
    current_x: u64,
    current_y: u64,
    current_z: u64,
    active_buttons: u32,
}

#[repr(C)]
struct RawAbsolutePointerProtocol {
    reset: unsafe extern "efiapi" fn(
        this: *mut RawAbsolutePointerProtocol,
        extended: bool,
    ) -> uefi::Status,
    get_state: unsafe extern "efiapi" fn(
        this: *mut RawAbsolutePointerProtocol,
        state: *mut AbsolutePointerState,
    ) -> uefi::Status,
    _wait_for_input: *mut core::ffi::c_void,
    mode: *const AbsolutePointerMode,
}

unsafe impl uefi::Identify for RawAbsolutePointerProtocol {
    const GUID: uefi::Guid = uefi::Guid::from_bytes([
        0x2B, 0xD3, 0x59, 0x8D, 0x55, 0xC6, 0xE9, 0x4A,
        0x9B, 0x15, 0xF2, 0x59, 0x04, 0x99, 0x2A, 0x43,
    ]);
}

impl uefi::proto::Protocol for RawAbsolutePointerProtocol {}

static ABS_PTRS: [core::sync::atomic::AtomicUsize; 4] = [
    core::sync::atomic::AtomicUsize::new(0),
    core::sync::atomic::AtomicUsize::new(0),
    core::sync::atomic::AtomicUsize::new(0),
    core::sync::atomic::AtomicUsize::new(0),
];
static ABS_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

// USB mouse cursor movement (linear).
const SIMPLE_POINTER_SPEED_NUM: i32 = 10;
const SIMPLE_POINTER_SPEED_DEN: i32 = 4;

/// Last absolute X/Y from touchpad — needed to compute deltas.
static mut ABS_LAST_X: i64 = -1;
static mut ABS_LAST_Y: i64 = -1;

/// Screen resolution for absolute→relative mapping.
static mut SCREEN_W: u32 = 1920;
static mut SCREEN_H: u32 = 1080;

pub fn set_screen_dimensions(w: u32, h: u32) {
    unsafe {
        SCREEN_W = w;
        SCREEN_H = h;
    }
}

pub fn reset_mouse_uefi() {
    use uefi::proto::console::pointer::Pointer;
    use uefi::boot::{OpenProtocolParams, OpenProtocolAttributes};

    let handles = uefi::boot::find_handles::<Pointer>().unwrap_or_default();
    crate::println("Detecting USB Mice (Pointer Protocol)...");

    if handles.is_empty() {
        crate::println("  WARNING: No Pointer handles found!");
    }

    let agent = uefi::boot::image_handle();
    let mut count = 0usize;

    for (i, h) in handles.iter().enumerate() {
        if i >= 4 {
            break;
        }
        let params = OpenProtocolParams {
            handle: *h,
            agent,
            controller: None,
        };

        match unsafe {
            uefi::boot::open_protocol::<Pointer>(params, OpenProtocolAttributes::GetProtocol)
        } {
            Ok(mut scoped) => {
                let _ = scoped.reset(false);
                let raw: *mut Pointer = &mut *scoped as *mut Pointer;
                MOUSE_PTRS[i].store(raw as usize, core::sync::atomic::Ordering::Release);
                core::mem::forget(scoped);
                crate::println("  - Mouse device opened persistently.");
                count += 1;
            }
            Err(_) => {
                crate::println("  - Handle found but open failed.");
            }
        }
    }
    MOUSE_COUNT.store(count, core::sync::atomic::Ordering::Release);

    // Now detect AbsolutePointer devices (touchpads / touchscreens)
    reset_absolute_pointer_uefi();
}

fn reset_absolute_pointer_uefi() {
    use uefi::boot::{OpenProtocolAttributes, OpenProtocolParams};

    crate::println("Detecting Touchpad (AbsolutePointer Protocol)...");

    let handles = uefi::boot::find_handles::<RawAbsolutePointerProtocol>().unwrap_or_default();

    if handles.is_empty() {
        crate::println("  No AbsolutePointer handles found.");
        return;
    }

    let agent = uefi::boot::image_handle();
    let mut abs_count = 0usize;

    for h in handles.iter() {
        if abs_count >= 4 {
            break;
        }

        let params = OpenProtocolParams {
            handle: *h,
            agent,
            controller: None,
        };

        match unsafe {
            uefi::boot::open_protocol::<RawAbsolutePointerProtocol>(
                params,
                OpenProtocolAttributes::GetProtocol,
            )
        } {
            Ok(scoped) => {
                // Get raw pointer and keep it alive
                let raw_ptr = &*scoped as *const RawAbsolutePointerProtocol
                    as *mut RawAbsolutePointerProtocol;
                // Reset device
                let _ = unsafe { ((*raw_ptr).reset)(raw_ptr, false) };

                // Log mode info
                if !unsafe { (*raw_ptr).mode }.is_null() {
                    let mode = unsafe { &*(*raw_ptr).mode };
                    crate::println(
                        alloc::format!(
                            "  - Touchpad: range X[{}..{}] Y[{}..{}]",
                            mode.abs_min_x,
                            mode.abs_max_x,
                            mode.abs_min_y,
                            mode.abs_max_y
                        )
                        .as_str(),
                    );
                }

                ABS_PTRS[abs_count]
                    .store(raw_ptr as usize, core::sync::atomic::Ordering::Release);
                core::mem::forget(scoped);
                abs_count += 1;
            }
            Err(_) => {
                crate::println("  - AbsolutePointer handle found but open failed.");
            }
        }
    }

    ABS_COUNT.store(abs_count, core::sync::atomic::Ordering::Release);
    if abs_count > 0 {
        crate::println(
            alloc::format!("  Opened {} AbsolutePointer device(s).", abs_count).as_str(),
        );
    }
    unsafe {
        ABS_LAST_X = -1;
        ABS_LAST_Y = -1;
    }
}

/// Returns (dx, dy, wheel_delta, left_button, right_button) from any available pointing device.
/// Checks both SimplePointer (USB mouse) and AbsolutePointer (touchpad) protocols.
pub fn poll_mouse_uefi() -> Option<(i32, i32, i32, bool, bool)> {
    // 1. Try SimplePointer first (USB mice — fast, low latency)
    if let Some(result) = poll_simple_pointer() {
        return Some(result);
    }
    // 2. Try AbsolutePointer (laptop touchpads)
    if let Some(result) = poll_absolute_pointer() {
        return Some(result);
    }
    None
}

fn poll_simple_pointer() -> Option<(i32, i32, i32, bool, bool)> {
    use uefi::proto::console::pointer::Pointer;

    let count = MOUSE_COUNT.load(core::sync::atomic::Ordering::Acquire);

    for i in 0..count {
        let addr = MOUSE_PTRS[i].load(core::sync::atomic::Ordering::Acquire);
        if addr == 0 {
            continue;
        }

        let ptr = unsafe { &mut *(addr as *mut Pointer) };

        match ptr.read_state() {
            Ok(Some(state)) => {
                let dx = scale_simple_pointer_delta(state.relative_movement[0]);
                let dy = scale_simple_pointer_delta(state.relative_movement[1]);
                let wheel_delta = state.relative_movement[2];
                let left_btn = state.button[0];
                let right_btn = state.button[1];
                return Some((dx, dy, wheel_delta, left_btn, right_btn));
            }
            _ => {}
        }
    }

    None
}

fn scale_simple_pointer_delta(delta: i32) -> i32 {
    if delta == 0 {
        return 0;
    }

    let abs = delta.abs() as i64;
    let scaled_abs = abs
        .saturating_mul(SIMPLE_POINTER_SPEED_NUM as i64)
        .saturating_div(SIMPLE_POINTER_SPEED_DEN as i64);
    let scaled = scaled_abs.min(i32::MAX as i64) as i32;
    if delta < 0 { -scaled } else { scaled }
}

fn poll_absolute_pointer() -> Option<(i32, i32, i32, bool, bool)> {
    let count = ABS_COUNT.load(core::sync::atomic::Ordering::Acquire);
    if count == 0 {
        return None;
    }

    for i in 0..count {
        let addr = ABS_PTRS[i].load(core::sync::atomic::Ordering::Acquire);
        if addr == 0 {
            continue;
        }

        let proto = addr as *mut RawAbsolutePointerProtocol;
        let mut state = AbsolutePointerState {
            current_x: 0,
            current_y: 0,
            current_z: 0,
            active_buttons: 0,
        };

        let status = unsafe { ((*proto).get_state)(proto, &mut state) };
        if status.is_error() {
            continue;
        }

        // Read mode for coordinate range
        let mode_ptr = unsafe { (*proto).mode };
        if mode_ptr.is_null() {
            continue;
        }
        let mode = unsafe { &*mode_ptr };
        let range_x = (mode.abs_max_x.saturating_sub(mode.abs_min_x)).max(1) as i64;
        let range_y = (mode.abs_max_y.saturating_sub(mode.abs_min_y)).max(1) as i64;

        // Button mapping: bit 0 = touch/left, bit 1 = alt/right
        let touch_active = (state.active_buttons & 0x01) != 0;
        let left_btn = touch_active;
        let right_btn = (state.active_buttons & 0x02) != 0;

        // ── Finger lift detection ──
        // When touch is not active (finger lifted), reset baseline so the next
        // touch starts fresh without a huge delta jump.
        if !touch_active {
            unsafe {
                ABS_LAST_X = -1;
                ABS_LAST_Y = -1;
            }
            // Still return the event so button-up reaches compositor
            return Some((0, 0, 0, false, right_btn));
        }

        // Map absolute coordinates to screen pixels
        let screen_w = unsafe { SCREEN_W } as i64;
        let screen_h = unsafe { SCREEN_H } as i64;
        let norm_x = (state.current_x as i64 - mode.abs_min_x as i64) * screen_w / range_x;
        let norm_y = (state.current_y as i64 - mode.abs_min_y as i64) * screen_h / range_y;

        let (raw_dx, raw_dy) = unsafe {
            if ABS_LAST_X < 0 || ABS_LAST_Y < 0 {
                // First reading after finger down — set baseline, no movement
                ABS_LAST_X = norm_x;
                ABS_LAST_Y = norm_y;
                (0i32, 0i32)
            } else {
                let dx = (norm_x - ABS_LAST_X) as i32;
                let dy = (norm_y - ABS_LAST_Y) as i32;
                ABS_LAST_X = norm_x;
                ABS_LAST_Y = norm_y;
                (dx, dy)
            }
        };

        // ── Clamp max delta (prevent cursor fling on residual jumps) ──
        let clamped_dx = raw_dx.clamp(-50, 50);
        let clamped_dy = raw_dy.clamp(-50, 50);

        // ── Jitter deadzone: ignore micro-movements when finger rests ──
        let abs_sum = clamped_dx.abs() + clamped_dy.abs();
        let (dx, dy) = if abs_sum <= 1 {
            (0, 0)
        } else {
            // ── Acceleration curve ──
            let accel = |d: i32| -> i32 {
                let a = d.abs();
                let factor = if a <= 3 {
                    1.0f32
                } else if a <= 10 {
                    1.5
                } else {
                    2.5
                };
                let result = (d as f32 * factor) as i32;
                result.clamp(-60, 60)
            };
            (accel(clamped_dx), accel(clamped_dy))
        };

        // Always return the event — never drop zero-delta ticks
        return Some((dx, dy, 0, left_btn, right_btn));
    }

    None
}

pub fn reboot_via_keyboard_controller() -> ! {
    cli();

    // Wait until the keyboard controller input buffer is clear.
    let mut spins = 0usize;
    while spins < 200_000 {
        let status = unsafe { inb(0x64) };
        if (status & 0x02) == 0 {
            break;
        }
        pause();
        spins += 1;
    }

    unsafe {
        // Pulse CPU reset line via i8042.
        outb(0x64, 0xFE);
    }

    loop {
        hlt();
    }
}
