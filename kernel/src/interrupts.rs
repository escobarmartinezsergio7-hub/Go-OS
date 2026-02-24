use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicU64, Ordering};

use crate::hal::{cli, outb, sti};

#[derive(Clone, Copy)]
#[repr(C, packed)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    zero: u32,
}

impl IdtEntry {
    const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            zero: 0,
        }
    }

    fn from_handler(addr: u64, selector: u16) -> Self {
        Self::from_handler_with_attr(addr, selector, 0x8E)
    }

    fn from_handler_with_attr(addr: u64, selector: u16, type_attr: u8) -> Self {
        Self {
            offset_low: (addr & 0xFFFF) as u16,
            selector,
            ist: 0,
            type_attr,
            offset_mid: ((addr >> 16) & 0xFFFF) as u16,
            offset_high: ((addr >> 32) & 0xFFFF_FFFF) as u32,
            zero: 0,
        }
    }
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

#[derive(Clone, Copy)]
pub struct IdtSummary {
    pub initialized: bool,
    pub base: u64,
    pub limit: u16,
    pub sample_handler: u64,
}

impl IdtSummary {
    const fn empty() -> Self {
        Self {
            initialized: false,
            base: 0,
            limit: 0,
            sample_handler: 0,
        }
    }
}

static mut IDT: [IdtEntry; 256] = [IdtEntry::missing(); 256];
static mut SUMMARY: IdtSummary = IdtSummary::empty();
static IRQ0_COUNT: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" {
    fn default_interrupt_stub();
    fn irq0_stub();
}

global_asm!(
    r#"
.global default_interrupt_stub
default_interrupt_stub:
    cli
1:
    hlt
    jmp 1b

.global irq0_stub
irq0_stub:
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov rbx, rsp
    and rsp, -16
    call irq0_rust
    mov rsp, rbx

    cmp byte ptr [rip + LINUX_REAL_SLICE_ACTIVE], 0
    je .Lirq0_restore

    mov rax, [rsp + 128] // interrupted CS from hardware frame
    and eax, 3
    cmp eax, 3
    jne .Lirq0_restore

    mov byte ptr [rip + LINUX_REAL_CTX_VALID], 1

    mov rax, [rsp + 0]
    mov [rip + LINUX_REAL_CTX_RAX], rax
    mov rax, [rsp + 8]
    mov [rip + LINUX_REAL_CTX_RCX], rax
    mov rax, [rsp + 24]
    mov [rip + LINUX_REAL_CTX_RBX], rax
    mov rax, [rsp + 32]
    mov [rip + LINUX_REAL_CTX_RBP], rax
    mov rax, [rsp + 88]
    mov [rip + LINUX_REAL_CTX_R12], rax
    mov rax, [rsp + 96]
    mov [rip + LINUX_REAL_CTX_R13], rax
    mov rax, [rsp + 104]
    mov [rip + LINUX_REAL_CTX_R14], rax
    mov rax, [rsp + 112]
    mov [rip + LINUX_REAL_CTX_R15], rax
    mov rax, [rsp + 48]
    mov [rip + LINUX_REAL_CTX_RDI], rax
    mov rax, [rsp + 40]
    mov [rip + LINUX_REAL_CTX_RSI], rax
    mov rax, [rsp + 16]
    mov [rip + LINUX_REAL_CTX_RDX], rax
    mov rax, [rsp + 72]
    mov [rip + LINUX_REAL_CTX_R10], rax
    mov rax, [rsp + 80]
    mov [rip + LINUX_REAL_CTX_R11], rax
    mov rax, [rsp + 56]
    mov [rip + LINUX_REAL_CTX_R8], rax
    mov rax, [rsp + 64]
    mov [rip + LINUX_REAL_CTX_R9], rax

    mov rax, [rsp + 120]
    mov [rip + LINUX_REAL_CTX_RIP], rax
    mov rax, [rsp + 136]
    mov [rip + LINUX_REAL_CTX_RFLAGS], rax
    mov rax, [rsp + 144]
    mov [rip + LINUX_REAL_CTX_RSP], rax

    mov rax, [rip + LINUX_REAL_SLICE_IRQ_PREEMPTS]
    add rax, 1
    mov [rip + LINUX_REAL_SLICE_IRQ_PREEMPTS], rax

    mov byte ptr [rip + LINUX_REAL_SLICE_FORCE_YIELD], 0
    mov rsp, [rip + LINUX_REAL_SLICE_RETURN_RSP]
    jmp qword ptr [rip + LINUX_REAL_SLICE_RETURN_RIP]

.Lirq0_restore:
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    iretq
"#
);

#[unsafe(no_mangle)]
extern "C" fn irq0_rust() {
    IRQ0_COUNT.fetch_add(1, Ordering::SeqCst);
    crate::timer::irq_tick();

    unsafe {
        // PIC EOI for IRQ0.
        outb(0x20, 0x20);
    }
}

#[inline]
fn current_cs() -> u16 {
    let cs: u16;
    unsafe {
        asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
    }
    cs
}

pub fn init_skeleton() -> IdtSummary {
    let handler = default_interrupt_stub as *const () as usize as u64;
    let code_selector = current_cs();

    unsafe {
        let mut i = 0;
        while i < 256 {
            IDT[i] = IdtEntry::from_handler(handler, code_selector);
            i += 1;
        }

        SUMMARY = IdtSummary {
            initialized: true,
            base: (core::ptr::addr_of!(IDT) as *const _) as u64,
            limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
            sample_handler: handler,
        };

        SUMMARY
    }
}

pub fn init_with_irq0() -> IdtSummary {
    let mut s = init_skeleton();
    let code_selector = current_cs();

    unsafe {
        let irq_addr = irq0_stub as *const () as usize as u64;
        IDT[32] = IdtEntry::from_handler(irq_addr, code_selector);

        let ptr = IdtPointer {
            limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: (core::ptr::addr_of!(IDT) as *const _) as u64,
        };

        asm!("lidt [{}]", in(reg) &ptr, options(readonly, nostack));

        s.sample_handler = irq_addr;
        SUMMARY = s;
    }

    s
}

pub fn install_user_gate(vector: u8, handler_addr: u64) {
    let code_selector = current_cs();
    unsafe {
        IDT[vector as usize] = IdtEntry::from_handler_with_attr(handler_addr, code_selector, 0xEE);
    }
}

pub fn load_current_idt() {
    unsafe {
        let ptr = IdtPointer {
            limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: (core::ptr::addr_of!(IDT) as *const _) as u64,
        };
        asm!("lidt [{}]", in(reg) &ptr, options(readonly, nostack));
    }
}

pub fn remap_pic_for_timer_irq() {
    unsafe {
        // ICW1
        outb(0x20, 0x11);
        outb(0xA0, 0x11);
        // ICW2: remap IRQs 0..15 to vectors 32..47
        outb(0x21, 0x20);
        outb(0xA1, 0x28);
        // ICW3
        outb(0x21, 0x04);
        outb(0xA1, 0x02);
        // ICW4
        outb(0x21, 0x01);
        outb(0xA1, 0x01);

        // Mask all except IRQ0 (timer) on master; mask all on slave
        outb(0x21, 0xFE);
        outb(0xA1, 0xFF);
    }
}

pub fn enable_irqs() {
    sti();
}

pub fn disable_irqs() {
    cli();
}

pub fn irq0_count() -> u64 {
    IRQ0_COUNT.load(Ordering::SeqCst)
}

pub fn summary() -> IdtSummary {
    unsafe { SUMMARY }
}
