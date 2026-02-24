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

pub fn reset_mouse_uefi() {
    use uefi::proto::console::pointer::Pointer;
    use uefi::boot::{OpenProtocolParams, OpenProtocolAttributes};
    
    let handles = uefi::boot::find_handles::<Pointer>().unwrap_or_default();
    crate::println("Detecting USB Mice (Pointer Protocol)...");
    
    if handles.is_empty() {
        crate::println("  WARNING: No Pointer handles found!");
        return;
    }
    
    let agent = uefi::boot::image_handle();
    let mut count = 0usize;
    
    for (i, h) in handles.iter().enumerate() {
        if i >= 4 { break; }
        let params = OpenProtocolParams { handle: *h, agent, controller: None };
        
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
}

/// Returns (dx, dy, wheel_delta, left_button, right_button) from any available pointing device.
/// For tablet devices, dx/dy may be large absolute values.
pub fn poll_mouse_uefi() -> Option<(i32, i32, i32, bool, bool)> {
    use uefi::proto::console::pointer::Pointer;
    
    let count = MOUSE_COUNT.load(core::sync::atomic::Ordering::Acquire);
    
    for i in 0..count {
        let addr = MOUSE_PTRS[i].load(core::sync::atomic::Ordering::Acquire);
        if addr == 0 { continue; }
        
        let ptr = unsafe { &mut *(addr as *mut Pointer) };
        
        match ptr.read_state() {
            Ok(Some(state)) => {
                let dx = state.relative_movement[0];
                let dy = state.relative_movement[1];
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
