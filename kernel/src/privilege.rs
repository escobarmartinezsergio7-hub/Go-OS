use core::arch::global_asm;

use crate::{hal, interrupts, syscall};

const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;
const COM1_PORT: u16 = 0x3F8;

pub const KERNEL_CS: u16 = 0x08;
pub const KERNEL_DS: u16 = 0x10;
pub const USER_DS: u16 = 0x18;
pub const USER_CS: u16 = 0x20;

pub const PHASE_OFF: u8 = 0;
pub const PHASE_GDT_TSS: u8 = 1;
pub const PHASE_USER_GATES: u8 = 2;
pub const PHASE_SYSCALL_MSR: u8 = 3;
pub const PHASE_CPL3_OK: u8 = 4;

pub const CPL3_TEST_UNKNOWN: u8 = 0;
pub const CPL3_TEST_PASS: u8 = 1;
pub const CPL3_TEST_FAIL: u8 = 2;
pub const CPL3_TEST_SKIPPED_SAFE: u8 = 3;
const LINUX_REAL_SLICE_SOFT_QUANTUM_DEFAULT: u64 = 2048;

#[derive(Clone, Copy)]
pub struct LinuxRealSliceReport {
    pub calls: u64,
    pub context_valid: bool,
    pub still_active: bool,
}

#[derive(Clone, Copy)]
pub struct LinuxRealContext {
    pub rax: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub r10: u64,
    pub r11: u64,
    pub r8: u64,
    pub r9: u64,
    pub rsp: u64,
    pub rip: u64,
    pub rflags: u64,
}

const GDT_LEN: usize = 16; // Room for UEFI entries + our entries
const KSTACK_SIZE: usize = 32 * 1024;
const USTACK_SIZE: usize = 32 * 1024;

#[repr(C, packed)]
struct GdtPointer {
    limit: u16,
    base: u64,
}

#[repr(C, packed)]
struct Tss64 {
    _reserved0: u32,
    rsp: [u64; 3],
    _reserved1: u64,
    ist: [u64; 7],
    _reserved2: u64,
    _reserved3: u16,
    iomap_base: u16,
}

impl Tss64 {
    const fn new() -> Self {
        Self {
            _reserved0: 0,
            rsp: [0; 3],
            _reserved1: 0,
            ist: [0; 7],
            _reserved2: 0,
            _reserved3: 0,
            iomap_base: 0,
        }
    }
}

#[repr(align(16))]
struct Stack<const N: usize>([u8; N]);

static mut GDT: [u64; GDT_LEN] = [0; GDT_LEN];
static mut TSS: Tss64 = Tss64::new();
static mut KERNEL_STACK: Stack<KSTACK_SIZE> = Stack([0; KSTACK_SIZE]);
static mut USER_STACK: Stack<USTACK_SIZE> = Stack([0; USTACK_SIZE]);

static mut PHASE: u8 = PHASE_OFF;
static mut UEFI_INIT_STEP: u8 = 0;
static mut CPL3_TEST_STATE: u8 = CPL3_TEST_UNKNOWN;
static mut SERIAL_READY: bool = false;

#[unsafe(no_mangle)]
static mut PRIV_TRACE_MARK: u64 = 0;

// Shared with asm syscall entry and CPL3 smoke test.
#[unsafe(no_mangle)]
static mut SYSCALL_KERNEL_STACK_TOP: u64 = 0;
#[unsafe(no_mangle)]
static mut SYSCALL_SAVED_USER_RSP: u64 = 0;
#[unsafe(no_mangle)]
static mut SYSCALL_ARG4: u64 = 0;
#[unsafe(no_mangle)]
static mut SYSCALL_ARG5: u64 = 0;

#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_ACTIVE: u8 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_FORCE_YIELD: u8 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_ENTRY: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_STACK: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_TLS: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_BUDGET: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_CALLS: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_IRQ_PREEMPTS: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_RETURN_RSP: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_RETURN_RIP: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_SOFT_PREEMPT: u8 = 1;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_SOFT_PREEMPTS: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_SOFT_STEP_COUNT: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_SOFT_QUANTUM: u64 = LINUX_REAL_SLICE_SOFT_QUANTUM_DEFAULT;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_FAULTED: u8 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_FAULT_VECTOR: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_FAULT_ERROR: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_FAULT_RIP: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_SLICE_FAULT_PREEMPTS: u64 = 0;

#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_VALID: u8 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_RAX: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_RCX: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_RBX: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_RBP: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_R12: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_R13: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_R14: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_R15: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_RDI: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_RSI: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_RDX: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_R10: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_R11: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_R8: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_R9: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_RSP: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_RIP: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CTX_RFLAGS: u64 = 0;

#[unsafe(no_mangle)]
static mut LINUX_REAL_CALLER_RBX: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CALLER_RBP: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CALLER_R12: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CALLER_R13: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CALLER_R14: u64 = 0;
#[unsafe(no_mangle)]
static mut LINUX_REAL_CALLER_R15: u64 = 0;

#[unsafe(no_mangle)]
static mut CPL3_TEST_USER_RSP: u64 = 0;
#[unsafe(no_mangle)]
static mut CPL3_TEST_RETURN_RSP: u64 = 0;
#[unsafe(no_mangle)]
static mut CPL3_TEST_RETURN_RIP: u64 = 0;
#[unsafe(no_mangle)]
static mut CPL3_TEST_FLAG: u64 = 0;

unsafe extern "C" {
    fn load_gdt_and_segments(gdt_ptr: *const GdtPointer);
    fn syscall_entry_asm();
    fn user_int80_stub();
    fn user_return_gate_stub();
    fn run_cpl3_test_asm() -> u64;
    fn linux_real_slice_enter_asm();
}

global_asm!(
    r#"
.global load_gdt_and_segments
load_gdt_and_segments:
    lgdt [rdi]
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    push 0x08
    lea rax, [rip + .Lreload_done]
    push rax
    retfq
.Lreload_done:
    mov ax, 0x28
    ltr ax
    ret

.global syscall_entry_asm
syscall_entry_asm:
    // Minimal single-core stack switch for SYSCALL path.
    mov [rip + SYSCALL_SAVED_USER_RSP], rsp
    mov rsp, [rip + SYSCALL_KERNEL_STACK_TOP]

    push rcx
    push r11
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    push rdi
    push rsi
    push rdx
    push r10
    push r8
    push r9
    sub rsp, 8

    mov rdi, rax         // id
    mov rsi, [rsp + 48]  // a0
    mov rdx, [rsp + 40]  // a1
    mov rcx, [rsp + 32]  // a2
    mov r8,  [rsp + 24]  // a3
    mov rax, [rsp + 16]  // a4 (saved r8)
    mov [rip + SYSCALL_ARG4], rax
    mov rax, [rsp + 8]   // a5 (saved r9)
    mov [rip + SYSCALL_ARG5], rax

    cmp byte ptr [rip + LINUX_REAL_SLICE_ACTIVE], 0
    je .Lsys_skip_entry_ctx

    mov byte ptr [rip + LINUX_REAL_CTX_VALID], 1
    mov [rip + LINUX_REAL_CTX_RAX], rdi
    mov rbx, [rsp + 112]
    mov [rip + LINUX_REAL_CTX_RCX], rbx
    mov rbx, [rsp + 96]
    mov [rip + LINUX_REAL_CTX_RBX], rbx
    mov rbx, [rsp + 88]
    mov [rip + LINUX_REAL_CTX_RBP], rbx
    mov rbx, [rsp + 80]
    mov [rip + LINUX_REAL_CTX_R12], rbx
    mov rbx, [rsp + 72]
    mov [rip + LINUX_REAL_CTX_R13], rbx
    mov rbx, [rsp + 64]
    mov [rip + LINUX_REAL_CTX_R14], rbx
    mov rbx, [rsp + 56]
    mov [rip + LINUX_REAL_CTX_R15], rbx
    mov rbx, [rsp + 48]
    mov [rip + LINUX_REAL_CTX_RDI], rbx
    mov rbx, [rsp + 40]
    mov [rip + LINUX_REAL_CTX_RSI], rbx
    mov rbx, [rsp + 32]
    mov [rip + LINUX_REAL_CTX_RDX], rbx
    mov rbx, [rsp + 24]
    mov [rip + LINUX_REAL_CTX_R10], rbx
    mov rbx, [rsp + 104]
    mov [rip + LINUX_REAL_CTX_R11], rbx
    mov rbx, [rsp + 16]
    mov [rip + LINUX_REAL_CTX_R8], rbx
    mov rbx, [rsp + 8]
    mov [rip + LINUX_REAL_CTX_R9], rbx
    mov rbx, [rip + SYSCALL_SAVED_USER_RSP]
    mov [rip + LINUX_REAL_CTX_RSP], rbx
    mov rbx, [rsp + 112]
    mov [rip + LINUX_REAL_CTX_RIP], rbx
    mov rbx, [rsp + 104]
    mov [rip + LINUX_REAL_CTX_RFLAGS], rbx
.Lsys_skip_entry_ctx:
    call syscall_hw_dispatch

.Lsys_after_dispatch:
    cmp byte ptr [rip + LINUX_REAL_SLICE_ACTIVE], 0
    je .Lsys_normal_return

    mov rbx, [rip + LINUX_REAL_SLICE_CALLS]
    add rbx, 1
    mov [rip + LINUX_REAL_SLICE_CALLS], rbx

    cmp byte ptr [rip + LINUX_REAL_SLICE_FORCE_YIELD], 0
    jne .Lsys_try_yield
    mov rcx, [rip + LINUX_REAL_SLICE_BUDGET]
    cmp rbx, rcx
    jb .Lsys_normal_return
    mov byte ptr [rip + LINUX_REAL_SLICE_FORCE_YIELD], 1

.Lsys_try_yield:
    cmp byte ptr [rip + LINUX_REAL_SLICE_FORCE_YIELD], 0
    je .Lsys_normal_return

    cmp byte ptr [rip + LINUX_REAL_SLICE_ACTIVE], 0
    je .Lsys_yield_noctx

    mov byte ptr [rip + LINUX_REAL_CTX_VALID], 1
    mov [rip + LINUX_REAL_CTX_RAX], rax
    mov rbx, [rsp + 112]
    mov [rip + LINUX_REAL_CTX_RCX], rbx

    mov rbx, [rsp + 96]
    mov [rip + LINUX_REAL_CTX_RBX], rbx
    mov rbx, [rsp + 88]
    mov [rip + LINUX_REAL_CTX_RBP], rbx
    mov rbx, [rsp + 80]
    mov [rip + LINUX_REAL_CTX_R12], rbx
    mov rbx, [rsp + 72]
    mov [rip + LINUX_REAL_CTX_R13], rbx
    mov rbx, [rsp + 64]
    mov [rip + LINUX_REAL_CTX_R14], rbx
    mov rbx, [rsp + 56]
    mov [rip + LINUX_REAL_CTX_R15], rbx
    mov rbx, [rsp + 48]
    mov [rip + LINUX_REAL_CTX_RDI], rbx
    mov rbx, [rsp + 40]
    mov [rip + LINUX_REAL_CTX_RSI], rbx
    mov rbx, [rsp + 32]
    mov [rip + LINUX_REAL_CTX_RDX], rbx
    mov rbx, [rsp + 24]
    mov [rip + LINUX_REAL_CTX_R10], rbx
    mov rbx, [rsp + 104]
    mov [rip + LINUX_REAL_CTX_R11], rbx
    mov rbx, [rsp + 16]
    mov [rip + LINUX_REAL_CTX_R8], rbx
    mov rbx, [rsp + 8]
    mov [rip + LINUX_REAL_CTX_R9], rbx

    mov rbx, [rip + SYSCALL_SAVED_USER_RSP]
    mov [rip + LINUX_REAL_CTX_RSP], rbx
    mov rbx, [rsp + 112]
    mov [rip + LINUX_REAL_CTX_RIP], rbx
    mov rbx, [rsp + 104]
    mov [rip + LINUX_REAL_CTX_RFLAGS], rbx

.Lsys_yield_noctx:
    mov byte ptr [rip + LINUX_REAL_SLICE_FORCE_YIELD], 0
    mov rsp, [rip + LINUX_REAL_SLICE_RETURN_RSP]
    jmp qword ptr [rip + LINUX_REAL_SLICE_RETURN_RIP]

.Lsys_normal_return:
    add rsp, 8
    pop r9
    pop r8
    pop r10
    pop rdx
    pop rsi
    pop rdi
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    pop r11
    pop rcx

    mov rsp, [rip + SYSCALL_SAVED_USER_RSP]
    sysretq

.global user_int80_stub
user_int80_stub:
    // INT 0x80 path (uses interrupt gate and iretq).
    push rcx
    push r11
    push rdi
    push rsi
    push rdx
    push r10
    push r8
    push r9
    sub rsp, 8

    mov rdi, rax         // id
    mov rsi, [rsp + 48]  // a0
    mov rdx, [rsp + 40]  // a1
    mov rcx, [rsp + 32]  // a2
    mov r8,  [rsp + 24]  // a3
    mov rax, [rsp + 16]  // a4 (saved r8)
    mov [rip + SYSCALL_ARG4], rax
    mov rax, [rsp + 8]   // a5 (saved r9)
    mov [rip + SYSCALL_ARG5], rax
    call syscall_hw_dispatch

    add rsp, 8
    pop r9
    pop r8
    pop r10
    pop rdx
    pop rsi
    pop rdi
    pop r11
    pop rcx
    iretq

.global user_return_gate_stub
user_return_gate_stub:
    mov qword ptr [rip + CPL3_TEST_FLAG], 1
    mov rsp, [rip + CPL3_TEST_RETURN_RSP]
    jmp [rip + CPL3_TEST_RETURN_RIP]

.global cpl3_test_user_entry
cpl3_test_user_entry:
    mov eax, 2            // SYS_GET_TICK
    xor edi, edi
    xor esi, esi
    xor edx, edx
    xor r10d, r10d
    syscall
    int 0x81
1:
    jmp 1b

.global run_cpl3_test_asm
run_cpl3_test_asm:
    mov qword ptr [rip + CPL3_TEST_FLAG], 0
    mov [rip + CPL3_TEST_RETURN_RSP], rsp
    lea rax, [rip + .Lcpl3_return]
    mov [rip + CPL3_TEST_RETURN_RIP], rax

    push 0x1b // USER_DS | RPL3
    mov rax, [rip + CPL3_TEST_USER_RSP]
    push rax
    push 0x202
    push 0x23 // USER_CS | RPL3
    lea rax, [rip + cpl3_test_user_entry]
    push rax
    iretq

.Lcpl3_return:
    mov rax, [rip + CPL3_TEST_FLAG]
    ret

.global linux_real_slice_enter_asm
linux_real_slice_enter_asm:
    mov [rip + LINUX_REAL_CALLER_RBX], rbx
    mov [rip + LINUX_REAL_CALLER_RBP], rbp
    mov [rip + LINUX_REAL_CALLER_R12], r12
    mov [rip + LINUX_REAL_CALLER_R13], r13
    mov [rip + LINUX_REAL_CALLER_R14], r14
    mov [rip + LINUX_REAL_CALLER_R15], r15

    mov [rip + LINUX_REAL_SLICE_RETURN_RSP], rsp
    lea rax, [rip + .Llinux_real_slice_return]
    mov [rip + LINUX_REAL_SLICE_RETURN_RIP], rax

    cmp byte ptr [rip + LINUX_REAL_CTX_VALID], 0
    jne .Llinux_real_slice_resume

    mov rax, [rip + LINUX_REAL_SLICE_TLS]
    test rax, rax
    jz .Llinux_real_slice_start
    mov ecx, 0xC0000100
    mov rdx, rax
    shr rdx, 32
    wrmsr

.Llinux_real_slice_start:
    push 0x1b
    mov rax, [rip + LINUX_REAL_SLICE_STACK]
    push rax
    mov rax, 0x202
    cmp byte ptr [rip + LINUX_REAL_SLICE_SOFT_PREEMPT], 0
    je .Llinux_real_slice_start_flags_done
    or rax, 0x100
.Llinux_real_slice_start_flags_done:
    push rax
    push 0x23
    mov rax, [rip + LINUX_REAL_SLICE_ENTRY]
    push rax
    iretq

.Llinux_real_slice_resume:
    mov rax, [rip + LINUX_REAL_SLICE_TLS]
    test rax, rax
    jz .Llinux_resume_no_tls
    mov ecx, 0xC0000100
    mov rdx, rax
    shr rdx, 32
    wrmsr
.Llinux_resume_no_tls:
    mov rax, [rip + LINUX_REAL_CTX_RSP]
    mov rcx, [rip + LINUX_REAL_CTX_RFLAGS]
    cmp byte ptr [rip + LINUX_REAL_SLICE_SOFT_PREEMPT], 0
    je .Llinux_resume_flags_clear_tf
    or rcx, 0x100
    jmp .Llinux_resume_flags_ready
.Llinux_resume_flags_clear_tf:
    and rcx, -257
.Llinux_resume_flags_ready:
    mov r11, [rip + LINUX_REAL_CTX_RIP]
    push 0x1b
    push rax
    push rcx
    push 0x23
    push r11

    mov rax, [rip + LINUX_REAL_CTX_RAX]
    mov rcx, [rip + LINUX_REAL_CTX_RCX]
    mov rbx, [rip + LINUX_REAL_CTX_RBX]
    mov rbp, [rip + LINUX_REAL_CTX_RBP]
    mov r12, [rip + LINUX_REAL_CTX_R12]
    mov r13, [rip + LINUX_REAL_CTX_R13]
    mov r14, [rip + LINUX_REAL_CTX_R14]
    mov r15, [rip + LINUX_REAL_CTX_R15]
    mov rdi, [rip + LINUX_REAL_CTX_RDI]
    mov rsi, [rip + LINUX_REAL_CTX_RSI]
    mov rdx, [rip + LINUX_REAL_CTX_RDX]
    mov r10, [rip + LINUX_REAL_CTX_R10]
    mov r11, [rip + LINUX_REAL_CTX_R11]
    mov r8,  [rip + LINUX_REAL_CTX_R8]
    mov r9,  [rip + LINUX_REAL_CTX_R9]
    iretq

.Llinux_real_slice_return:
    mov rbx, [rip + LINUX_REAL_CALLER_RBX]
    mov rbp, [rip + LINUX_REAL_CALLER_RBP]
    mov r12, [rip + LINUX_REAL_CALLER_R12]
    mov r13, [rip + LINUX_REAL_CALLER_R13]
    mov r14, [rip + LINUX_REAL_CALLER_R14]
    mov r15, [rip + LINUX_REAL_CALLER_R15]
    ret
"#
);

#[unsafe(no_mangle)]
extern "C" fn syscall_hw_dispatch(id: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let (a4, a5) = unsafe { (SYSCALL_ARG4, SYSCALL_ARG5) };
    if unsafe { LINUX_REAL_SLICE_ACTIVE != 0 } && syscall::linux_shim_active() {
        // While Linux runreal shim is active, route raw CPU SYSCALL numbers to Linux ABI shim.
        let result = syscall::linux_shim_invoke(id, a0, a1, a2, a3, a4, a5) as u64;
        unsafe {
            if LINUX_REAL_SLICE_ACTIVE != 0 && !syscall::linux_shim_active() {
                // Process exited: force return to kernel without keeping resumable context.
                LINUX_REAL_SLICE_ACTIVE = 0;
                LINUX_REAL_SLICE_FORCE_YIELD = 1;
                LINUX_REAL_CTX_VALID = 0;
            }
        }
        return result;
    }
    // Thread slot 0 is the shell userspace thread in current runtime model.
    syscall::invoke(0, id as usize, a0, a1, a2, a3)
}

const fn tss_descriptor(base: u64, limit: u32) -> (u64, u64) {
    let low = (limit as u64 & 0xFFFF)
        | ((base & 0xFFFF) << 16)
        | (((base >> 16) & 0xFF) << 32)
        | (0x89u64 << 40)
        | (((limit as u64 >> 16) & 0xF) << 48)
        | (((base >> 24) & 0xFF) << 56);

    let high = (base >> 32) & 0xFFFF_FFFF;
    (low, high)
}

fn user_stack_top() -> u64 {
    unsafe {
        let base = (core::ptr::addr_of!(USER_STACK.0) as *const u8) as u64;
        base + USTACK_SIZE as u64
    }
}

fn enable_user_fpu_sse() {
    unsafe {
        let mut cr0: u64 = 0;
        core::arch::asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
        // Enable x87/SSE in user mode: clear EM/TS, keep MP set.
        cr0 &= !(1 << 2); // EM
        cr0 &= !(1 << 3); // TS
        cr0 |= 1 << 1; // MP
        core::arch::asm!("mov cr0, {}", in(reg) cr0, options(nomem, nostack, preserves_flags));

        let mut cr4: u64 = 0;
        core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
        cr4 |= 1 << 9; // OSFXSR
        cr4 |= 1 << 10; // OSXMMEXCPT
        core::arch::asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack, preserves_flags));

        core::arch::asm!("fninit", options(nostack));
        let mxcsr: u32 = 0x1F80;
        core::arch::asm!("ldmxcsr [{}]", in(reg) &mxcsr, options(readonly, nostack));
    }
}

fn phase1_prepare_gdt_tss() {
    enable_user_fpu_sse();
    unsafe {
        let kstack_base = (core::ptr::addr_of!(KERNEL_STACK.0) as *const u8) as u64;
        let kstack_top = kstack_base + KSTACK_SIZE as u64;

        SYSCALL_KERNEL_STACK_TOP = kstack_top;
        CPL3_TEST_USER_RSP = user_stack_top();

        TSS.rsp[0] = kstack_top;
        TSS.iomap_base = core::mem::size_of::<Tss64>() as u16;

        GDT[0] = 0;
        GDT[1] = 0x00AF9A000000FFFF; // kernel code 64
        GDT[2] = 0x00AF92000000FFFF; // kernel data
        GDT[3] = 0x00AFF2000000FFFF; // user data
        GDT[4] = 0x00AFFA000000FFFF; // user code 64

        let tss_base = (core::ptr::addr_of!(TSS) as *const _) as u64;
        let tss_limit = (core::mem::size_of::<Tss64>() - 1) as u32;
        let (tss_low, tss_high) = tss_descriptor(tss_base, tss_limit);
        GDT[5] = tss_low;
        GDT[6] = tss_high;
    }
}

fn phase2_install_user_gates() {
    let int80 = user_int80_stub as *const () as usize as u64;
    let ret_gate = user_return_gate_stub as *const () as usize as u64;
    interrupts::install_user_gate(0x80, int80);
    interrupts::install_user_gate(0x81, ret_gate);
    interrupts::load_current_idt();
}

fn phase3_configure_syscall_msrs() {
    unsafe {
        let lstar = syscall_entry_asm as *const () as usize as u64;

        let mut efer = hal::rdmsr(IA32_EFER);
        efer |= 1; // SCE
        hal::wrmsr(IA32_EFER, efer);

        // SYSCALL entry: CS=KERNEL_CS, SS=KERNEL_DS.
        // SYSRET return: CS=(STAR[63:48] + 16)=USER_CS, SS=(STAR[63:48] + 8)=USER_DS.
        let star_hi_base = (USER_DS as u64).saturating_sub(8);
        let star = (star_hi_base << 48) | ((KERNEL_CS as u64) << 32);
        hal::wrmsr(IA32_STAR, star);
        hal::wrmsr(IA32_LSTAR, lstar);
        // Clear IF and TF on SYSCALL entry so single-step fallback stays in user mode only.
        hal::wrmsr(IA32_FMASK, (1 << 9) | (1 << 8));
    }
}

fn commit_gdt_tss_now() {
    unsafe {
        let ptr = GdtPointer {
            limit: (core::mem::size_of::<[u64; GDT_LEN]>() - 1) as u16,
            base: (core::ptr::addr_of!(GDT) as *const _) as u64,
        };
        load_gdt_and_segments(&ptr as *const GdtPointer);
    }
}

fn phase4_run_cpl3_smoke_test() -> bool {
    // Safe default path: do not execute risky CPL3 transition automatically.
    // This keeps GUI/runtime stable when stepping phases from shell.
    let ok = false;
    unsafe {
        CPL3_TEST_STATE = if ok {
            CPL3_TEST_PASS
        } else {
            CPL3_TEST_SKIPPED_SAFE
        };
    }
    true
}

pub fn run_cpl3_test_unsafe_now() -> bool {
    // Manual, risky path: use only for explicit debugging sessions.
    commit_gdt_tss_now();
    let ok = unsafe { run_cpl3_test_asm() } != 0;
    unsafe {
        CPL3_TEST_STATE = if ok { CPL3_TEST_PASS } else { CPL3_TEST_FAIL };
    }
    ok
}

pub fn status_word() -> u64 {
    unsafe { (PHASE as u64) | ((CPL3_TEST_STATE as u64) << 8) }
}

pub fn current_phase() -> u8 {
    unsafe { PHASE }
}

pub fn syscall_bridge_ready() -> bool {
    current_phase() >= PHASE_SYSCALL_MSR
}

pub fn uefi_init_step() -> u8 {
    unsafe { UEFI_INIT_STEP }
}

pub fn linux_real_slice_reset() {
    unsafe {
        let irq_preempt_active = syscall::runtime_irq_mode_active() && interrupts::irq0_count() > 0;
        LINUX_REAL_SLICE_ACTIVE = 0;
        LINUX_REAL_SLICE_FORCE_YIELD = 0;
        LINUX_REAL_SLICE_ENTRY = 0;
        LINUX_REAL_SLICE_STACK = 0;
        LINUX_REAL_SLICE_TLS = 0;
        LINUX_REAL_SLICE_BUDGET = 0;
        LINUX_REAL_SLICE_CALLS = 0;
        LINUX_REAL_SLICE_IRQ_PREEMPTS = 0;
        LINUX_REAL_SLICE_RETURN_RSP = 0;
        LINUX_REAL_SLICE_RETURN_RIP = 0;
        LINUX_REAL_SLICE_SOFT_PREEMPT = 1;
        LINUX_REAL_SLICE_SOFT_PREEMPTS = 0;
        LINUX_REAL_SLICE_SOFT_STEP_COUNT = 0;
        LINUX_REAL_SLICE_SOFT_QUANTUM = 512;
        LINUX_REAL_SLICE_FAULTED = 0;
        LINUX_REAL_SLICE_FAULT_VECTOR = 0;
        LINUX_REAL_SLICE_FAULT_ERROR = 0;
        LINUX_REAL_SLICE_FAULT_RIP = 0;
        LINUX_REAL_SLICE_FAULT_PREEMPTS = 0;
        LINUX_REAL_CTX_VALID = 0;
        LINUX_REAL_CTX_RAX = 0;
        LINUX_REAL_CTX_RCX = 0;
        LINUX_REAL_CTX_RBX = 0;
        LINUX_REAL_CTX_RBP = 0;
        LINUX_REAL_CTX_R12 = 0;
        LINUX_REAL_CTX_R13 = 0;
        LINUX_REAL_CTX_R14 = 0;
        LINUX_REAL_CTX_R15 = 0;
        LINUX_REAL_CTX_RDI = 0;
        LINUX_REAL_CTX_RSI = 0;
        LINUX_REAL_CTX_RDX = 0;
        LINUX_REAL_CTX_R10 = 0;
        LINUX_REAL_CTX_R11 = 0;
        LINUX_REAL_CTX_R8 = 0;
        LINUX_REAL_CTX_R9 = 0;
        LINUX_REAL_CTX_RSP = 0;
        LINUX_REAL_CTX_RIP = 0;
        LINUX_REAL_CTX_RFLAGS = 0;
        LINUX_REAL_CALLER_RBX = 0;
        LINUX_REAL_CALLER_RBP = 0;
        LINUX_REAL_CALLER_R12 = 0;
        LINUX_REAL_CALLER_R13 = 0;
        LINUX_REAL_CALLER_R14 = 0;
        LINUX_REAL_CALLER_R15 = 0;
    }
}

pub fn linux_real_slice_irq_preempts() -> u64 {
    unsafe { LINUX_REAL_SLICE_IRQ_PREEMPTS }
}

pub fn linux_real_slice_soft_preempt_supported() -> bool {
    true
}

pub fn linux_real_slice_set_soft_preempt(enabled: bool) {
    unsafe {
        LINUX_REAL_SLICE_SOFT_PREEMPT = if enabled { 1 } else { 0 };
        LINUX_REAL_SLICE_SOFT_STEP_COUNT = 0;
        if enabled && LINUX_REAL_SLICE_SOFT_QUANTUM == 0 {
            LINUX_REAL_SLICE_SOFT_QUANTUM = LINUX_REAL_SLICE_SOFT_QUANTUM_DEFAULT;
        }
    }
}

pub fn linux_real_slice_configure_soft_preempt(enabled: bool, quantum: usize) {
    unsafe {
        LINUX_REAL_SLICE_SOFT_PREEMPT = if enabled { 1 } else { 0 };
        LINUX_REAL_SLICE_SOFT_STEP_COUNT = 0;
        let q = (quantum as u64).max(1).min(1_000_000);
        LINUX_REAL_SLICE_SOFT_QUANTUM = q;
    }
}

pub fn linux_real_slice_soft_preempt_enabled() -> bool {
    unsafe { LINUX_REAL_SLICE_SOFT_PREEMPT != 0 }
}

pub fn linux_real_slice_soft_preempts() -> u64 {
    unsafe { LINUX_REAL_SLICE_SOFT_PREEMPTS }
}

pub fn linux_real_slice_fault_preempts() -> u64 {
    unsafe { LINUX_REAL_SLICE_FAULT_PREEMPTS }
}

pub fn linux_real_slice_take_fault() -> Option<(u64, u64, u64)> {
    unsafe {
        if LINUX_REAL_SLICE_FAULTED == 0 {
            return None;
        }
        let out = (
            LINUX_REAL_SLICE_FAULT_VECTOR,
            LINUX_REAL_SLICE_FAULT_ERROR,
            LINUX_REAL_SLICE_FAULT_RIP,
        );
        LINUX_REAL_SLICE_FAULTED = 0;
        Some(out)
    }
}

pub fn linux_real_slice_request_yield() {
    unsafe {
        LINUX_REAL_SLICE_FORCE_YIELD = 1;
    }
}

pub fn linux_real_context_valid() -> bool {
    unsafe { LINUX_REAL_CTX_VALID != 0 }
}

pub fn linux_real_context_snapshot() -> Option<LinuxRealContext> {
    unsafe {
        if LINUX_REAL_CTX_VALID == 0 {
            return None;
        }
        Some(LinuxRealContext {
            rax: LINUX_REAL_CTX_RAX,
            rcx: LINUX_REAL_CTX_RCX,
            rbx: LINUX_REAL_CTX_RBX,
            rbp: LINUX_REAL_CTX_RBP,
            r12: LINUX_REAL_CTX_R12,
            r13: LINUX_REAL_CTX_R13,
            r14: LINUX_REAL_CTX_R14,
            r15: LINUX_REAL_CTX_R15,
            rdi: LINUX_REAL_CTX_RDI,
            rsi: LINUX_REAL_CTX_RSI,
            rdx: LINUX_REAL_CTX_RDX,
            r10: LINUX_REAL_CTX_R10,
            r11: LINUX_REAL_CTX_R11,
            r8: LINUX_REAL_CTX_R8,
            r9: LINUX_REAL_CTX_R9,
            rsp: LINUX_REAL_CTX_RSP,
            rip: LINUX_REAL_CTX_RIP,
            rflags: LINUX_REAL_CTX_RFLAGS,
        })
    }
}

pub fn linux_real_context_restore(ctx: &LinuxRealContext) {
    unsafe {
        LINUX_REAL_CTX_RAX = ctx.rax;
        LINUX_REAL_CTX_RCX = ctx.rcx;
        LINUX_REAL_CTX_RBX = ctx.rbx;
        LINUX_REAL_CTX_RBP = ctx.rbp;
        LINUX_REAL_CTX_R12 = ctx.r12;
        LINUX_REAL_CTX_R13 = ctx.r13;
        LINUX_REAL_CTX_R14 = ctx.r14;
        LINUX_REAL_CTX_R15 = ctx.r15;
        LINUX_REAL_CTX_RDI = ctx.rdi;
        LINUX_REAL_CTX_RSI = ctx.rsi;
        LINUX_REAL_CTX_RDX = ctx.rdx;
        LINUX_REAL_CTX_R10 = ctx.r10;
        LINUX_REAL_CTX_R11 = ctx.r11;
        LINUX_REAL_CTX_R8 = ctx.r8;
        LINUX_REAL_CTX_R9 = ctx.r9;
        LINUX_REAL_CTX_RSP = ctx.rsp;
        LINUX_REAL_CTX_RIP = ctx.rip;
        LINUX_REAL_CTX_RFLAGS = ctx.rflags;
        LINUX_REAL_CTX_VALID = 1;
    }
}

pub fn linux_real_slice_set_tls(tls_tcb_addr: u64) {
    unsafe {
        LINUX_REAL_SLICE_TLS = tls_tcb_addr;
    }
}

pub fn linux_real_slice_discard_resume_context() {
    unsafe {
        LINUX_REAL_CTX_VALID = 0;
        LINUX_REAL_SLICE_FORCE_YIELD = 0;
    }
}

pub fn linux_real_slice_run(
    entry: u64,
    stack_ptr: u64,
    tls_tcb_addr: u64,
    call_budget: usize,
) -> LinuxRealSliceReport {
    let mut report = LinuxRealSliceReport {
        calls: 0,
        context_valid: false,
        still_active: false,
    };

    if !syscall_bridge_ready() || entry == 0 || stack_ptr == 0 {
        return report;
    }

    unsafe {
        if LINUX_REAL_CTX_VALID == 0
            || LINUX_REAL_SLICE_ENTRY != entry
            || LINUX_REAL_SLICE_STACK != stack_ptr
        {
            LINUX_REAL_CTX_VALID = 0;
            LINUX_REAL_SLICE_ENTRY = entry;
            LINUX_REAL_SLICE_STACK = stack_ptr;
            LINUX_REAL_SLICE_TLS = tls_tcb_addr;
        } else if tls_tcb_addr != 0 {
            LINUX_REAL_SLICE_TLS = tls_tcb_addr;
        }

        LINUX_REAL_SLICE_ACTIVE = 1;
        LINUX_REAL_SLICE_FORCE_YIELD = 0;
        LINUX_REAL_SLICE_BUDGET = (call_budget.max(1).min(4096)) as u64;
        LINUX_REAL_SLICE_CALLS = 0;
        LINUX_REAL_SLICE_SOFT_STEP_COUNT = 0;

        linux_real_slice_enter_asm();
        // Real-slice must be considered active only while guest CPL3 is executing.
        // Keeping this flag set between slices allows unrelated userspace syscalls/IRQs
        // to be mistaken as guest events and can deadlock/freeze runtime on real hardware.
        LINUX_REAL_SLICE_ACTIVE = 0;

        report.calls = LINUX_REAL_SLICE_CALLS;
        report.context_valid = LINUX_REAL_CTX_VALID != 0;
        report.still_active = syscall::linux_shim_active();
    }

    report
}

pub fn advance_phase() -> u8 {
    unsafe {
        match PHASE {
            PHASE_OFF => {
                phase1_prepare_gdt_tss();
                // Real-slice relies on USER_CS/USER_DS selectors from this GDT.
                // Keep safe mode by skipping CPL3 smoke test, but always commit GDT/TSS.
                commit_gdt_tss_now();
                PHASE = PHASE_GDT_TSS;
            }
            PHASE_GDT_TSS => {
                phase2_install_user_gates();
                PHASE = PHASE_USER_GATES;
            }
            PHASE_USER_GATES => {
                phase3_configure_syscall_msrs();
                PHASE = PHASE_SYSCALL_MSR;
            }
            PHASE_SYSCALL_MSR => {
                if phase4_run_cpl3_smoke_test() {
                    PHASE = PHASE_CPL3_OK;
                }
            }
            _ => {}
        }
        PHASE
    }
}

pub fn init_privilege_layers() {
    // Keep existing API: run all phases in order.
    while current_phase() < PHASE_CPL3_OK {
        let before = current_phase();
        let after = advance_phase();
        if after == before {
            break;
        }
    }
}

fn cpu_has_syscall() -> bool {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let leaf = core::arch::x86_64::__cpuid(0x8000_0001);
        (leaf.edx & (1 << 11)) != 0
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// UEFI-safe base initialization: EXTENDS the active UEFI GDT rather than replacing it.
/// Uses sgdt to read the current GDT, copies all entries, adds our kernel/user/TSS entries,
/// then does lgdt with the extended GDT. UEFI entries are preserved for BootServices compatibility.
pub fn init_privilege_layers_uefi_lite() {
    unsafe {
        UEFI_INIT_STEP = 1;
        enable_user_fpu_sse();

        // Set up kernel stack for ring transitions
        UEFI_INIT_STEP = 2;
        let kstack_base = (core::ptr::addr_of!(KERNEL_STACK.0) as *const u8) as u64;
        let kstack_top = kstack_base + KSTACK_SIZE as u64;
        SYSCALL_KERNEL_STACK_TOP = kstack_top;
        CPL3_TEST_USER_RSP = user_stack_top();

        // Prepare TSS
        TSS.rsp[0] = kstack_top;
        TSS.iomap_base = core::mem::size_of::<Tss64>() as u16;

        // Read the current (UEFI) GDT via sgdt
        UEFI_INIT_STEP = 3;
        let mut current_gdt = GdtPointer { limit: 0, base: 0 };
        core::arch::asm!(
            "sgdt [{}]",
            in(reg) &mut current_gdt as *mut GdtPointer,
            options(nostack, preserves_flags)
        );

        // Copy ALL existing UEFI GDT entries into our larger GDT array
        UEFI_INIT_STEP = 4;
        let uefi_entry_count = ((current_gdt.limit as usize + 1) / 8).min(GDT_LEN);
        let uefi_gdt_ptr = current_gdt.base as *const u64;
        for i in 0..uefi_entry_count {
            GDT[i] = core::ptr::read_volatile(uefi_gdt_ptr.add(i));
        }
        // Zero remaining entries
        for i in uefi_entry_count..GDT_LEN {
            GDT[i] = 0;
        }

        // Overwrite/add our entries at the standard offsets.
        // Selectors 0x08 and 0x10 are standard kernel CS/DS in long mode;
        // UEFI uses compatible descriptors here, so overwriting is safe.
        UEFI_INIT_STEP = 5;
        GDT[1] = 0x00AF9A000000FFFF; // kernel code 64  (selector 0x08)
        GDT[2] = 0x00AF92000000FFFF; // kernel data      (selector 0x10)
        GDT[3] = 0x00AFF2000000FFFF; // user data         (selector 0x18)
        GDT[4] = 0x00AFFA000000FFFF; // user code 64     (selector 0x20)

        let tss_base = (core::ptr::addr_of!(TSS) as *const _) as u64;
        let tss_limit = (core::mem::size_of::<Tss64>() - 1) as u32;
        let (tss_low, tss_high) = tss_descriptor(tss_base, tss_limit);
        GDT[5] = tss_low;  // TSS low  (selector 0x28)
        GDT[6] = tss_high; // TSS high

        // CRITICAL SECTION: disable interrupts, swap GDT, load TSS, reload IDT
        UEFI_INIT_STEP = 6;
        core::arch::asm!("cli", options(nostack, preserves_flags));

        // lgdt with our extended GDT (includes all UEFI entries + our entries)
        UEFI_INIT_STEP = 7;
        let ptr = GdtPointer {
            limit: (core::mem::size_of::<[u64; GDT_LEN]>() - 1) as u16,
            base: (core::ptr::addr_of!(GDT) as *const _) as u64,
        };
        core::arch::asm!(
            "lgdt [{}]",
            in(reg) &ptr as *const GdtPointer,
            options(nostack, preserves_flags)
        );

        // ltr — load TSS
        UEFI_INIT_STEP = 8;
        core::arch::asm!(
            "mov ax, 0x28",
            "ltr ax",
            options(nostack)
        );

        PHASE = PHASE_GDT_TSS;

        // Install user interrupt gates + reload IDT
        UEFI_INIT_STEP = 9;
        phase2_install_user_gates();
        PHASE = PHASE_USER_GATES;

        // Re-enable interrupts
        UEFI_INIT_STEP = 10;
        core::arch::asm!("sti", options(nostack, preserves_flags));
    }
}

/// UEFI-safe syscall MSR configuration (separate step).
/// Returns false if CPU reports no SYSCALL support.
pub fn init_privilege_layers_uefi_msr() -> bool {
    if !cpu_has_syscall() {
        return false;
    }
    unsafe {
        phase3_configure_syscall_msrs();
        PHASE = PHASE_SYSCALL_MSR;
    }
    true
}

/// UEFI-safe initialization: configures GDT + TSS + user gates + SYSCALL MSRs.
/// Does NOT reload current CS/DS/SS segments (which crashes in UEFI mode).
/// Only does: lgdt (register GDT) + ltr (load TSS) + SYSCALL MSRs.
/// When real-slice does iretq, CPU loads user CS/SS from our GDT.
/// When syscall fires, STAR MSR resolves kernel CS/SS from our GDT.
pub fn init_privilege_layers_uefi() {
    init_privilege_layers_uefi_lite();
    let _ = init_privilege_layers_uefi_msr();
}
