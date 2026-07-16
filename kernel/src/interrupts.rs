use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::__cpuid;

use crate::hal::{cli, inb, outb, sti};

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

#[inline]
fn idt_entry_handler_addr(entry: &IdtEntry) -> u64 {
    (entry.offset_low as u64)
        | ((entry.offset_mid as u64) << 16)
        | ((entry.offset_high as u64) << 32)
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
static APIC_TIMER_MODE: AtomicU8 = AtomicU8::new(0);
static APIC_TIMER_BASE: AtomicU64 = AtomicU64::new(0);
static PIC_TIMER_ARMED: AtomicBool = AtomicBool::new(false);
static IRQ_TIMER_SUSPEND_DEPTH: AtomicU32 = AtomicU32::new(0);
static SAVED_APIC_LVT_TIMER: AtomicU32 = AtomicU32::new(0);
static SAVED_PIC_IMR: AtomicU8 = AtomicU8::new(0xFF);

const APIC_MODE_NONE: u8 = 0;
const APIC_MODE_XAPIC: u8 = 1;
const APIC_MODE_X2APIC: u8 = 2;

const APIC_TIMER_VECTOR: u8 = 33;
pub const IPI_RESCHED_VECTOR: u8 = 0xF0;
const IA32_APIC_BASE_MSR: u32 = 0x1B;
const IA32_X2APIC_EOI: u32 = 0x80B;
const IA32_X2APIC_SVR: u32 = 0x80F;
const IA32_X2APIC_LVT_TIMER: u32 = 0x832;
const IA32_X2APIC_INITIAL_COUNT: u32 = 0x838;
const IA32_X2APIC_DIVIDE: u32 = 0x83E;
const APIC_BASE_ENABLE: u64 = 1 << 11;
const APIC_BASE_X2APIC_ENABLE: u64 = 1 << 10;
const APIC_BASE_ADDR_MASK: u64 = 0xFFFF_F000;
const APIC_REG_EOI: u64 = 0x00B0;
const APIC_REG_SVR: u64 = 0x00F0;
const APIC_REG_LVT_TIMER: u64 = 0x0320;
const APIC_REG_INITIAL_COUNT: u64 = 0x0380;
const APIC_REG_DIVIDE: u64 = 0x03E0;
const APIC_SVR_ENABLE: u32 = 1 << 8;
const APIC_SVR_SPURIOUS_VECTOR: u32 = 0xFF;
const APIC_LVT_PERIODIC: u32 = 1 << 17;
const APIC_LVT_MASKED: u32 = 1 << 16;
const APIC_DIVIDE_BY_16: u32 = 0x3;
const ALLOW_XAPIC_TIMER_MMIO: bool = false;

unsafe extern "C" {
    fn default_interrupt_stub();
    fn de_stub();
    fn ud_stub();
    fn nm_stub();
    fn ts_stub();
    fn np_stub();
    fn ss_stub();
    fn gp_stub();
    fn pf_stub();
    fn mf_stub();
    fn ac_stub();
    fn xm_stub();
    fn debug_stub();
    fn irq0_stub();
    fn ipi_resched_stub();
}

global_asm!(
    r#"
.global default_interrupt_stub
default_interrupt_stub:
    cli
1:
    hlt
    jmp 1b

.global de_stub
de_stub:
    mov r15, 0
    jmp linux_fault_noerr_entry

.global ud_stub
ud_stub:
    mov r15, 6
    jmp linux_fault_noerr_entry

.global nm_stub
nm_stub:
    mov r15, 7
    jmp linux_fault_noerr_entry

.global ts_stub
ts_stub:
    mov r15, 10
    jmp linux_fault_err_entry

.global np_stub
np_stub:
    mov r15, 11
    jmp linux_fault_err_entry

.global ss_stub
ss_stub:
    mov r15, 12
    jmp linux_fault_err_entry

.global gp_stub
gp_stub:
    mov r15, 13
    jmp linux_fault_err_entry

.global pf_stub
pf_stub:
    mov r15, 14
    jmp linux_fault_err_entry

.global mf_stub
mf_stub:
    mov r15, 16
    jmp linux_fault_noerr_entry

.global ac_stub
ac_stub:
    mov r15, 17
    jmp linux_fault_err_entry

.global xm_stub
xm_stub:
    mov r15, 19
    jmp linux_fault_noerr_entry

.global linux_fault_noerr_entry
linux_fault_noerr_entry:
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

    cmp byte ptr [rip + LINUX_REAL_SLICE_ACTIVE], 0
    je .Lfault_halt
    mov rax, [rsp + 128] // interrupted CS from hardware frame
    and eax, 3
    cmp eax, 3
    je .Lfault_noerr_user

    // Fault while real-slice is active but exception happened in CPL0 path.
    // Do not halt entire GUI: surface fault and return to kernel caller.
    mov byte ptr [rip + LINUX_REAL_SLICE_FAULTED], 1
    mov [rip + LINUX_REAL_SLICE_FAULT_VECTOR], r15
    mov qword ptr [rip + LINUX_REAL_SLICE_FAULT_ERROR], 0
    mov rax, [rsp + 120]
    mov [rip + LINUX_REAL_SLICE_FAULT_RIP], rax
    mov byte ptr [rip + LINUX_REAL_CTX_VALID], 0

    mov rax, [rip + LINUX_REAL_SLICE_FAULT_PREEMPTS]
    add rax, 1
    mov [rip + LINUX_REAL_SLICE_FAULT_PREEMPTS], rax

    mov byte ptr [rip + LINUX_REAL_SLICE_FORCE_YIELD], 0
    mov rsp, [rip + LINUX_REAL_SLICE_RETURN_RSP]
    jmp qword ptr [rip + LINUX_REAL_SLICE_RETURN_RIP]

.Lfault_noerr_user:

    mov byte ptr [rip + LINUX_REAL_SLICE_FAULTED], 1
    mov [rip + LINUX_REAL_SLICE_FAULT_VECTOR], r15
    mov qword ptr [rip + LINUX_REAL_SLICE_FAULT_ERROR], 0

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
    mov [rip + LINUX_REAL_SLICE_FAULT_RIP], rax
    mov rax, [rsp + 136]
    mov [rip + LINUX_REAL_CTX_RFLAGS], rax
    mov rax, [rsp + 144]
    mov [rip + LINUX_REAL_CTX_RSP], rax

    mov rax, [rip + LINUX_REAL_SLICE_FAULT_PREEMPTS]
    add rax, 1
    mov [rip + LINUX_REAL_SLICE_FAULT_PREEMPTS], rax

    mov byte ptr [rip + LINUX_REAL_SLICE_FORCE_YIELD], 0
    mov rsp, [rip + LINUX_REAL_SLICE_RETURN_RSP]
    jmp qword ptr [rip + LINUX_REAL_SLICE_RETURN_RIP]

.global linux_fault_err_entry
linux_fault_err_entry:
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

    cmp byte ptr [rip + LINUX_REAL_SLICE_ACTIVE], 0
    je .Lfault_halt
    mov rax, [rsp + 136] // interrupted CS from hardware frame (+errcode)
    and eax, 3
    cmp eax, 3
    je .Lfault_err_user

    // Fault while real-slice is active but exception happened in CPL0 path.
    // Do not halt entire GUI: surface fault and return to kernel caller.
    mov byte ptr [rip + LINUX_REAL_SLICE_FAULTED], 1
    mov [rip + LINUX_REAL_SLICE_FAULT_VECTOR], r15
    mov rax, [rsp + 120]
    mov [rip + LINUX_REAL_SLICE_FAULT_ERROR], rax
    mov rax, [rsp + 128]
    mov [rip + LINUX_REAL_SLICE_FAULT_RIP], rax
    mov byte ptr [rip + LINUX_REAL_CTX_VALID], 0

    mov rax, [rip + LINUX_REAL_SLICE_FAULT_PREEMPTS]
    add rax, 1
    mov [rip + LINUX_REAL_SLICE_FAULT_PREEMPTS], rax

    mov byte ptr [rip + LINUX_REAL_SLICE_FORCE_YIELD], 0
    mov rsp, [rip + LINUX_REAL_SLICE_RETURN_RSP]
    jmp qword ptr [rip + LINUX_REAL_SLICE_RETURN_RIP]

.Lfault_err_user:

    mov byte ptr [rip + LINUX_REAL_SLICE_FAULTED], 1
    mov [rip + LINUX_REAL_SLICE_FAULT_VECTOR], r15
    mov rax, [rsp + 120]
    mov [rip + LINUX_REAL_SLICE_FAULT_ERROR], rax

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

    mov rax, [rsp + 128]
    mov [rip + LINUX_REAL_CTX_RIP], rax
    mov [rip + LINUX_REAL_SLICE_FAULT_RIP], rax
    mov rax, [rsp + 144]
    mov [rip + LINUX_REAL_CTX_RFLAGS], rax
    mov rax, [rsp + 152]
    mov [rip + LINUX_REAL_CTX_RSP], rax

    mov rax, [rip + LINUX_REAL_SLICE_FAULT_PREEMPTS]
    add rax, 1
    mov [rip + LINUX_REAL_SLICE_FAULT_PREEMPTS], rax

    mov byte ptr [rip + LINUX_REAL_SLICE_FORCE_YIELD], 0
    mov rsp, [rip + LINUX_REAL_SLICE_RETURN_RSP]
    jmp qword ptr [rip + LINUX_REAL_SLICE_RETURN_RIP]

.Lfault_halt:
    cli
1:
    hlt
    jmp 1b

.global debug_stub
debug_stub:
    // Ensure TF is cleared in kernel context to avoid recursive #DB in handler/kernel path.
    pushfq
    pop rax
    and rax, -257
    push rax
    popfq

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

    cmp byte ptr [rip + LINUX_REAL_SLICE_ACTIVE], 0
    je .Ldbg_restore
    cmp byte ptr [rip + LINUX_REAL_SLICE_SOFT_PREEMPT], 0
    je .Ldbg_restore

    mov rax, [rsp + 128] // interrupted CS from hardware frame
    and eax, 3
    cmp eax, 3
    jne .Ldbg_restore

    mov rax, [rip + LINUX_REAL_SLICE_SOFT_STEP_COUNT]
    add rax, 1
    mov [rip + LINUX_REAL_SLICE_SOFT_STEP_COUNT], rax
    mov rcx, [rip + LINUX_REAL_SLICE_SOFT_QUANTUM]
    test rcx, rcx
    je .Ldbg_preempt
    cmp rax, rcx
    jb .Ldbg_restore

.Ldbg_preempt:
    mov qword ptr [rip + LINUX_REAL_SLICE_SOFT_STEP_COUNT], 0
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

    mov rax, [rip + LINUX_REAL_SLICE_SOFT_PREEMPTS]
    add rax, 1
    mov [rip + LINUX_REAL_SLICE_SOFT_PREEMPTS], rax

    mov byte ptr [rip + LINUX_REAL_SLICE_FORCE_YIELD], 0
    mov rsp, [rip + LINUX_REAL_SLICE_RETURN_RSP]
    jmp qword ptr [rip + LINUX_REAL_SLICE_RETURN_RIP]

.Ldbg_restore:
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

    // Kernel preempt: only for CPL=0.
    mov rax, [rsp + 128] // interrupted CS from hardware frame
    and eax, 3
    cmp eax, 0
    jne .Lirq0_kernel_preempt_done
    mov rdi, rsp
    call process_irq_preempt_arm
    test al, al
    je .Lirq0_kernel_preempt_done
    lea rax, [rip + kernel_preempt_trampoline]
    mov [rsp + 120], rax
.Lirq0_kernel_preempt_done:

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

.global ipi_resched_stub
ipi_resched_stub:
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
    call ipi_resched_rust
    mov rsp, rbx

    // Kernel preempt: only for CPL=0.
    mov rax, [rsp + 128] // interrupted CS from hardware frame
    and eax, 3
    cmp eax, 0
    jne .Lipi_resched_done
    mov rdi, rsp
    call process_irq_preempt_arm
    test al, al
    je .Lipi_resched_done
    lea rax, [rip + kernel_preempt_trampoline]
    mov [rsp + 120], rax
.Lipi_resched_done:

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

.global kernel_preempt_trampoline
kernel_preempt_trampoline:
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
    call process_irq_preempt_resume_rip
    mov rsp, rbx
    mov [rsp - 8], rax

    mov rbx, rsp
    and rsp, -16
    call process_thread_yield
    mov rsp, rbx

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
    jmp qword ptr [rsp - 128]
"#
);

#[unsafe(no_mangle)]
extern "C" fn irq0_rust() {
    let apic_timer_active = APIC_TIMER_MODE.load(Ordering::SeqCst) != APIC_MODE_NONE;
    let pic_timer_active = PIC_TIMER_ARMED.load(Ordering::SeqCst);
    if apic_timer_active || pic_timer_active {
        IRQ0_COUNT.fetch_add(1, Ordering::SeqCst);
        crate::timer::irq_tick();
        crate::process::irq_preempt_signal();
    }
    apic_eoi_if_present();

    unsafe {
        // Send EOI to both PICs so spurious IRQ7/IRQ15 and slave lines
        // never remain in-service.
        outb(0xA0, 0x20);
        outb(0x20, 0x20);
    }
}

#[unsafe(no_mangle)]
extern "C" fn ipi_resched_rust() {
    apic_eoi_if_present();
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
    let de_handler = de_stub as *const () as usize as u64;
    let debug_handler = debug_stub as *const () as usize as u64;
    let ud_handler = ud_stub as *const () as usize as u64;
    let nm_handler = nm_stub as *const () as usize as u64;
    let ts_handler = ts_stub as *const () as usize as u64;
    let np_handler = np_stub as *const () as usize as u64;
    let ss_handler = ss_stub as *const () as usize as u64;
    let gp_handler = gp_stub as *const () as usize as u64;
    let pf_handler = pf_stub as *const () as usize as u64;
    let mf_handler = mf_stub as *const () as usize as u64;
    let ac_handler = ac_stub as *const () as usize as u64;
    let xm_handler = xm_stub as *const () as usize as u64;
    let code_selector = current_cs();

    unsafe {
        let mut i = 0;
        while i < 256 {
            IDT[i] = IdtEntry::from_handler(handler, code_selector);
            i += 1;
        }
        // Keep debug vector wired even without IRQ mode so soft-preempt works in polling runtime.
        IDT[0] = IdtEntry::from_handler(de_handler, code_selector);
        // Keep NMI non-fatal in runtime mode; treat as debug-style trap and iret.
        IDT[2] = IdtEntry::from_handler(debug_handler, code_selector);
        IDT[1] = IdtEntry::from_handler(debug_handler, code_selector);
        // Recover user-mode real-slice faults instead of halting whole GUI.
        IDT[6] = IdtEntry::from_handler(ud_handler, code_selector);
        IDT[7] = IdtEntry::from_handler(nm_handler, code_selector);
        IDT[10] = IdtEntry::from_handler(ts_handler, code_selector);
        IDT[11] = IdtEntry::from_handler(np_handler, code_selector);
        IDT[12] = IdtEntry::from_handler(ss_handler, code_selector);
        IDT[13] = IdtEntry::from_handler(gp_handler, code_selector);
        IDT[14] = IdtEntry::from_handler(pf_handler, code_selector);
        IDT[16] = IdtEntry::from_handler(mf_handler, code_selector);
        IDT[17] = IdtEntry::from_handler(ac_handler, code_selector);
        IDT[19] = IdtEntry::from_handler(xm_handler, code_selector);

        SUMMARY = IdtSummary {
            initialized: true,
            base: (core::ptr::addr_of!(IDT) as *const _) as u64,
            limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
            sample_handler: debug_handler,
        };

        SUMMARY
    }
}

pub fn init_with_irq0() -> IdtSummary {
    let mut s = init_skeleton();
    let code_selector = current_cs();

    unsafe {
        let debug_addr = debug_stub as *const () as usize as u64;
        let irq_addr = irq0_stub as *const () as usize as u64;
        let ipi_addr = ipi_resched_stub as *const () as usize as u64;
        IDT[1] = IdtEntry::from_handler(debug_addr, code_selector);
        // Route all external interrupt vectors through a safe IRQ stub so
        // firmware-routed vectors never fall into the default halt handler.
        let mut vec = 32usize;
        while vec < 256 {
            IDT[vec] = IdtEntry::from_handler(irq_addr, code_selector);
            vec += 1;
        }
        IDT[IPI_RESCHED_VECTOR as usize] = IdtEntry::from_handler(ipi_addr, code_selector);

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
    APIC_TIMER_MODE.store(APIC_MODE_NONE, Ordering::SeqCst);
    APIC_TIMER_BASE.store(0, Ordering::SeqCst);
    PIC_TIMER_ARMED.store(true, Ordering::SeqCst);
    unsafe {
        // Route external interrupts to legacy PIC (IMCR), away from APIC path.
        // This is required on some real hardware to keep PIC/PIT mode stable.
        outb(0x22, 0x70);
        let imcr = inb(0x23);
        outb(0x23, imcr | 0x01);

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

pub fn irq_timer_source_armed() -> bool {
    PIC_TIMER_ARMED.load(Ordering::SeqCst) || APIC_TIMER_MODE.load(Ordering::SeqCst) != APIC_MODE_NONE
}

pub fn irq_timer_source_name() -> &'static str {
    if PIC_TIMER_ARMED.load(Ordering::SeqCst) {
        "pic-pit"
    } else {
        match APIC_TIMER_MODE.load(Ordering::SeqCst) {
            APIC_MODE_X2APIC => "apic-x2apic",
            APIC_MODE_XAPIC => "apic-xapic",
            _ => "none",
        }
    }
}

pub fn suspend_irq_timer() -> bool {
    let depth = IRQ_TIMER_SUSPEND_DEPTH.fetch_add(1, Ordering::SeqCst);
    if depth > 0 {
        return true;
    }

    let mode = APIC_TIMER_MODE.load(Ordering::SeqCst);
    if mode == APIC_MODE_X2APIC {
        unsafe {
            let lvt = crate::hal::rdmsr(IA32_X2APIC_LVT_TIMER) as u32;
            SAVED_APIC_LVT_TIMER.store(lvt, Ordering::SeqCst);
            crate::hal::wrmsr(
                IA32_X2APIC_LVT_TIMER,
                (lvt | APIC_LVT_MASKED) as u64,
            );
        }
        return true;
    }

    if mode == APIC_MODE_XAPIC {
        let base = APIC_TIMER_BASE.load(Ordering::SeqCst);
        if base != 0 {
            unsafe {
                let lvt_ptr = (base + APIC_REG_LVT_TIMER) as *mut u32;
                let lvt = lvt_ptr.read_volatile();
                SAVED_APIC_LVT_TIMER.store(lvt, Ordering::SeqCst);
                lvt_ptr.write_volatile(lvt | APIC_LVT_MASKED);
            }
            return true;
        }
    }

    if PIC_TIMER_ARMED.load(Ordering::SeqCst) {
        unsafe {
            let imr = inb(0x21);
            SAVED_PIC_IMR.store(imr, Ordering::SeqCst);
            outb(0x21, imr | 0x01); // mask IRQ0
        }
        return true;
    }

    IRQ_TIMER_SUSPEND_DEPTH.store(0, Ordering::SeqCst);
    false
}

pub fn resume_irq_timer() {
    let depth = IRQ_TIMER_SUSPEND_DEPTH.load(Ordering::SeqCst);
    if depth == 0 {
        return;
    }
    if IRQ_TIMER_SUSPEND_DEPTH.fetch_sub(1, Ordering::SeqCst) != 1 {
        return;
    }

    let mode = APIC_TIMER_MODE.load(Ordering::SeqCst);
    if mode == APIC_MODE_X2APIC {
        let lvt = SAVED_APIC_LVT_TIMER.load(Ordering::SeqCst);
        if lvt != 0 {
            unsafe { crate::hal::wrmsr(IA32_X2APIC_LVT_TIMER, lvt as u64); }
        }
        return;
    }

    if mode == APIC_MODE_XAPIC {
        let base = APIC_TIMER_BASE.load(Ordering::SeqCst);
        if base != 0 {
            let lvt = SAVED_APIC_LVT_TIMER.load(Ordering::SeqCst);
            unsafe {
                let lvt_ptr = (base + APIC_REG_LVT_TIMER) as *mut u32;
                lvt_ptr.write_volatile(lvt);
            }
        }
        return;
    }

    if PIC_TIMER_ARMED.load(Ordering::SeqCst) {
        unsafe {
            let imr = SAVED_PIC_IMR.load(Ordering::SeqCst);
            outb(0x21, imr);
        }
    }
}

pub fn summary() -> IdtSummary {
    unsafe { SUMMARY }
}

pub fn linux_soft_preempt_debug_ready() -> bool {
    let expected = debug_stub as *const () as usize as u64;
    unsafe {
        if !SUMMARY.initialized {
            return false;
        }
        idt_entry_handler_addr(&IDT[1]) == expected
    }
}

pub fn linux_soft_preempt_debug_ensure() -> bool {
    let expected = debug_stub as *const () as usize as u64;
    unsafe {
        if !SUMMARY.initialized {
            let _ = init_skeleton();
        }

        if idt_entry_handler_addr(&IDT[1]) != expected {
            let code_selector = current_cs();
            IDT[1] = IdtEntry::from_handler(expected, code_selector);
        }

        let ptr = IdtPointer {
            limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: (core::ptr::addr_of!(IDT) as *const _) as u64,
        };
        asm!("lidt [{}]", in(reg) &ptr, options(readonly, nostack));

        SUMMARY.initialized = true;
        SUMMARY.base = ptr.base;
        SUMMARY.limit = ptr.limit;

        idt_entry_handler_addr(&IDT[1]) == expected
    }
}

fn apic_eoi_if_present() {
    if !cpu_has_local_apic() {
        return;
    }
    unsafe {
        let apic_base = crate::hal::rdmsr(IA32_APIC_BASE_MSR);
        if (apic_base & APIC_BASE_ENABLE) == 0 {
            return;
        }
        // x2APIC EOI via MSR is safe even when we are in PIC/PIT mode.
        // Keep this enabled to avoid interrupt storms if firmware leaves x2APIC routing active.
        if (apic_base & APIC_BASE_X2APIC_ENABLE) != 0 {
            crate::hal::wrmsr(IA32_X2APIC_EOI, 0);
            return;
        }
        // In PIC/PIT mode avoid xAPIC MMIO EOI touches because APIC MMIO may be unmapped.
        if APIC_TIMER_MODE.load(Ordering::SeqCst) == APIC_MODE_NONE {
            return;
        }
        let base = apic_base & APIC_BASE_ADDR_MASK;
        if base != 0 {
            ((base + APIC_REG_EOI) as *mut u32).write_volatile(0);
        }
    }
}

fn apic_initial_count_for_hz(hz: u32) -> u32 {
    let safe_hz = hz.clamp(18, 1000);
    // Keep first IRQ latency short enough for startup probe on slower timer buses.
    (25_000_000u32 / safe_hz).clamp(20_000, 250_000)
}

fn cpu_has_local_apic() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        let leaf1 = unsafe { __cpuid(1) };
        (leaf1.edx & (1 << 9)) != 0
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

fn cpu_has_x2apic() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        let leaf1 = unsafe { __cpuid(1) };
        (leaf1.ecx & (1 << 21)) != 0
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

pub fn quiesce_firmware_apic() {
    let apic_mode = APIC_TIMER_MODE.load(Ordering::SeqCst);
    let apic_base_saved = APIC_TIMER_BASE.load(Ordering::SeqCst);
    let pic_timer_was_armed = PIC_TIMER_ARMED.load(Ordering::SeqCst);

    unsafe {
        if pic_timer_was_armed {
            let imr = inb(0x21);
            SAVED_PIC_IMR.store(imr, Ordering::SeqCst);
            outb(0x21, imr | 0x01);
            outb(0xA1, 0xFF);
        } else {
            let imr = inb(0x21);
            outb(0x21, imr | 0x01);
        }
    }

    APIC_TIMER_MODE.store(APIC_MODE_NONE, Ordering::SeqCst);
    APIC_TIMER_BASE.store(0, Ordering::SeqCst);
    PIC_TIMER_ARMED.store(false, Ordering::SeqCst);
    // Keep this mostly as software quiesce to avoid risky APIC_BASE toggles.
    // If firmware is already in x2APIC mode, masking timer via MSR is safe and helps
    // prevent stale LAPIC timer/spurious activity during PIC/PIT runtime.
    if !cpu_has_local_apic() {
        return;
    }
    unsafe {
        let apic_base = crate::hal::rdmsr(IA32_APIC_BASE_MSR);
        if (apic_base & APIC_BASE_ENABLE) == 0 {
            return;
        }
        if apic_mode == APIC_MODE_X2APIC {
            crate::hal::wrmsr(IA32_X2APIC_INITIAL_COUNT, 0);
            crate::hal::wrmsr(
                IA32_X2APIC_LVT_TIMER,
                (APIC_LVT_MASKED | APIC_TIMER_VECTOR as u32) as u64,
            );
            crate::hal::wrmsr(IA32_X2APIC_EOI, 0);
            return;
        }
        if apic_mode == APIC_MODE_XAPIC && apic_base_saved != 0 {
            let lvt_ptr = (apic_base_saved + APIC_REG_LVT_TIMER) as *mut u32;
            lvt_ptr.write_volatile(APIC_LVT_MASKED | APIC_TIMER_VECTOR as u32);
            ((apic_base_saved + APIC_REG_INITIAL_COUNT) as *mut u32).write_volatile(0);
            apic_eoi_if_present();
            return;
        }
        if (apic_base & APIC_BASE_X2APIC_ENABLE) != 0 {
            crate::hal::wrmsr(IA32_X2APIC_INITIAL_COUNT, 0);
            crate::hal::wrmsr(
                IA32_X2APIC_LVT_TIMER,
                (APIC_LVT_MASKED | APIC_TIMER_VECTOR as u32) as u64,
            );
            crate::hal::wrmsr(IA32_X2APIC_EOI, 0);
        }
    }
}

pub fn start_apic_timer_irq(hz: u32) -> bool {
    if !cpu_has_local_apic() {
        return false;
    }

    PIC_TIMER_ARMED.store(false, Ordering::SeqCst);
    let initial = apic_initial_count_for_hz(hz);
    unsafe {
        // Route external interrupts back to APIC (IMCR) when APIC mode is selected.
        outb(0x22, 0x70);
        let imcr = inb(0x23);
        outb(0x23, imcr & !0x01);

        let mut apic_base = crate::hal::rdmsr(IA32_APIC_BASE_MSR);
        apic_base |= APIC_BASE_ENABLE;
        if cpu_has_x2apic() {
            apic_base |= APIC_BASE_X2APIC_ENABLE;
        }
        crate::hal::wrmsr(IA32_APIC_BASE_MSR, apic_base);
        let apic_base = crate::hal::rdmsr(IA32_APIC_BASE_MSR);

        if (apic_base & APIC_BASE_X2APIC_ENABLE) != 0 {
            let svr = crate::hal::rdmsr(IA32_X2APIC_SVR) as u32;
            let svr_new = (svr & !0xFF) | APIC_SVR_SPURIOUS_VECTOR | APIC_SVR_ENABLE;
            crate::hal::wrmsr(IA32_X2APIC_SVR, svr_new as u64);
            crate::hal::wrmsr(IA32_X2APIC_DIVIDE, APIC_DIVIDE_BY_16 as u64);
            crate::hal::wrmsr(
                IA32_X2APIC_LVT_TIMER,
                (APIC_LVT_PERIODIC | APIC_TIMER_VECTOR as u32) as u64,
            );
            crate::hal::wrmsr(IA32_X2APIC_INITIAL_COUNT, initial as u64);
            APIC_TIMER_BASE.store(0, Ordering::SeqCst);
            APIC_TIMER_MODE.store(APIC_MODE_X2APIC, Ordering::SeqCst);
        } else {
            if !ALLOW_XAPIC_TIMER_MMIO {
                return false;
            }
            let base = apic_base & APIC_BASE_ADDR_MASK;
            if base == 0 {
                return false;
            }

            let svr_ptr = (base + APIC_REG_SVR) as *mut u32;
            let svr = svr_ptr.read_volatile();
            svr_ptr.write_volatile((svr & !0xFF) | APIC_SVR_SPURIOUS_VECTOR | APIC_SVR_ENABLE);
            ((base + APIC_REG_DIVIDE) as *mut u32).write_volatile(APIC_DIVIDE_BY_16);
            ((base + APIC_REG_LVT_TIMER) as *mut u32)
                .write_volatile(APIC_LVT_PERIODIC | APIC_TIMER_VECTOR as u32);
            ((base + APIC_REG_INITIAL_COUNT) as *mut u32).write_volatile(initial);
            APIC_TIMER_BASE.store(base, Ordering::SeqCst);
            APIC_TIMER_MODE.store(APIC_MODE_XAPIC, Ordering::SeqCst);
        }

        // Mask legacy PIC lines when APIC timer drives periodic ticks.
        outb(0x21, 0xFF);
        outb(0xA1, 0xFF);
    }

    true
}
