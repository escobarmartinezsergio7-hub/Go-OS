#![no_std]
#![no_main]

use core::panic::PanicInfo;

unsafe extern "C" {
    fn cpp_clear_screen();
    fn cpp_set_color(fg: u8, bg: u8);
    fn cpp_putc(c: i8);
    fn cpp_print(s: *const i8);
    fn cpp_println(s: *const i8);
    fn cpp_keyboard_poll() -> i8;
    fn cpp_pci_scan_brief();
    fn cpp_reboot();
}

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    unsafe { cpp_clear_screen() };
    unsafe { cpp_set_color(0x0B, 0x00) };
    println_cstr(b"ReduxOS Starter\0");
    println_cstr(b"Rust kernel + C++ drivers + Ruby tooling\0");
    println_cstr(b"Type 'help' to show commands.\0");

    shell_loop()
}

fn shell_loop() -> ! {
    let mut input = [0u8; 128];
    let mut len = 0usize;

    print_prompt();

    loop {
        let key = unsafe { cpp_keyboard_poll() } as u8;
        if key == 0 {
            cpu_idle();
            continue;
        }

        match key {
            b'\n' => {
                unsafe { cpp_putc(b'\n' as i8) };
                handle_command(&input[..len]);
                len = 0;
                print_prompt();
            }
            8 => {
                if len > 0 {
                    len -= 1;
                    unsafe { cpp_putc(8) };
                }
            }
            _ => {
                if key >= b' ' && len < input.len() - 1 {
                    input[len] = key;
                    len += 1;
                    unsafe { cpp_putc(key as i8) };
                }
            }
        }
    }
}

fn handle_command(cmd: &[u8]) {
    if cmd.is_empty() {
        return;
    }

    if eq(cmd, b"help") {
        println_cstr(b"Commands:\0");
        println_cstr(b"  help    - Show this help\0");
        println_cstr(b"  about   - Show system info\0");
        println_cstr(b"  clear   - Clear screen\0");
        println_cstr(b"  pci     - Scan PCI devices (brief)\0");
        println_cstr(b"  echo X  - Print text\0");
        println_cstr(b"  ruby    - Ruby toolchain hint\0");
        println_cstr(b"  apps    - App packaging hint\0");
        println_cstr(b"  reboot  - Reboot machine\0");
        return;
    }

    if eq(cmd, b"about") {
        println_cstr(b"ReduxOS Starter v0.1\0");
        println_cstr(b"Kernel: Rust no_std\0");
        println_cstr(b"Drivers: C++ VGA/Keyboard/PCI\0");
        println_cstr(b"Tools: Ruby RPX + redux_get\0");
        return;
    }

    if eq(cmd, b"clear") {
        unsafe {
            cpp_clear_screen();
        }
        return;
    }

    if eq(cmd, b"pci") {
        unsafe {
            cpp_pci_scan_brief();
        }
        return;
    }

    if starts_with(cmd, b"echo ") {
        let payload = &cmd[5..];
        print_slice(payload);
        newline();
        return;
    }

    if eq(cmd, b"ruby") {
        println_cstr(b"Use host tools:\0");
        println_cstr(b"ruby tools/redux_get.rb update\0");
        println_cstr(b"ruby tools/redux_get.rb install hello-redux\0");
        return;
    }

    if eq(cmd, b"apps") {
        println_cstr(b"Sample app sources:\0");
        println_cstr(b"apps/hello_redux/main.rml\0");
        println_cstr(b"apps/hello_redux/main.rdx\0");
        return;
    }

    if eq(cmd, b"reboot") {
        unsafe {
            cpp_reboot();
        }
        return;
    }

    print_cstr(b"Unknown command: \0");
    print_slice(cmd);
    newline();
}

fn print_prompt() {
    print_cstr(b"redux> \0");
}

fn print_slice(data: &[u8]) {
    let mut buf = [0u8; 129];
    let max = core::cmp::min(data.len(), 128);
    let mut i = 0;
    while i < max {
        buf[i] = data[i];
        i += 1;
    }
    buf[i] = 0;

    unsafe {
        cpp_print(buf.as_ptr() as *const i8);
    }
}

fn newline() {
    unsafe { cpp_putc(b'\n' as i8) };
}

fn print_cstr(bytes: &[u8]) {
    unsafe { cpp_print(bytes.as_ptr() as *const i8) };
}

fn println_cstr(bytes: &[u8]) {
    unsafe { cpp_println(bytes.as_ptr() as *const i8) };
}

fn cpu_idle() {
    unsafe {
        core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}

fn eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

fn starts_with(a: &[u8], prefix: &[u8]) -> bool {
    if prefix.len() > a.len() {
        return false;
    }

    let mut i = 0;
    while i < prefix.len() {
        if a[i] != prefix[i] {
            return false;
        }
        i += 1;
    }
    true
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe { cpp_set_color(0x0F, 0x04) }; // white on red
    println_cstr(b"KERNEL PANIC\0");

    loop {
        cpu_idle();
    }
}
