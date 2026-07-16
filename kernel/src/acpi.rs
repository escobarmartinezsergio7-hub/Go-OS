use alloc::format;
use alloc::string::String;
use core::arch::{asm, global_asm};
use core::ptr;
use core::slice;
use core::sync::atomic::{AtomicU8, Ordering};

const RSDP_SIGNATURE: [u8; 8] = *b"RSD PTR ";
const FADT_SIGNATURE: u32 = u32::from_le_bytes(*b"FACP");
const DSDT_SIGNATURE: u32 = u32::from_le_bytes(*b"DSDT");

const ACPI_ADDRESS_SPACE_SYSTEM_MEMORY: u8 = 0;
const ACPI_ADDRESS_SPACE_SYSTEM_IO: u8 = 1;

const PM1_STS_WAK: u16 = 1 << 15;
const PM1_CNT_SCI_EN: u16 = 1;
const PM1_CNT_SLP_TYP_SHIFT: u16 = 10;
const PM1_CNT_SLP_TYP_MASK: u16 = 0x7 << PM1_CNT_SLP_TYP_SHIFT;
const PM1_CNT_SLP_EN: u16 = 1 << 13;

const S3_TRAMPOLINE_PHYS: u64 = 0x8000;
const S3_TRAMPOLINE_MAX_BYTES: usize = 4096;
const S3_WAKE_STACK_BYTES: usize = 16 * 1024;
const FACS_SIGNATURE: u32 = u32::from_le_bytes(*b"FACS");
const FACS_FIRMWARE_WAKING_VECTOR_OFF: usize = 12;
const FACS_FLAGS_OFF: usize = 20;
const FACS_X_FIRMWARE_WAKING_VECTOR_OFF: usize = 24;
const FACS_OSPM_FLAGS_OFF: usize = 36;
const FACS_64BIT_WAKE_SUPPORTED_F: u32 = 1 << 1;
const FACS_OSPM_64BIT_WAKE_F: u32 = 1;

static S3_RESUME_SEEN: AtomicU8 = AtomicU8::new(0);

#[repr(C)]
struct S3JumpBuffer {
    rbx: u64,
    rbp: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rsp: u64,
    rip: u64,
}

#[repr(align(16))]
struct S3WakeStack([u8; S3_WAKE_STACK_BYTES]);

#[unsafe(no_mangle)]
static mut S3_JUMP_BUFFER: S3JumpBuffer = S3JumpBuffer {
    rbx: 0,
    rbp: 0,
    r12: 0,
    r13: 0,
    r14: 0,
    r15: 0,
    rsp: 0,
    rip: 0,
};

static mut S3_WAKE_STACK: S3WakeStack = S3WakeStack([0; S3_WAKE_STACK_BYTES]);

unsafe extern "C" {
    fn s3_set_resume_context() -> u64;
    fn s3_longjmp_resume() -> !;

    static s3_wake_trampoline_start: u8;
    static s3_wake_trampoline_end: u8;
    static s3_wake_trampoline_cr3: u8;
    static s3_wake_trampoline_stack: u8;
    static s3_wake_trampoline_entry: u8;
    static s3_wake_trampoline_x64_entry: u8;
}

global_asm!(
    r#"
.intel_syntax noprefix
.section .text

.global s3_set_resume_context
s3_set_resume_context:
    lea rdx, [rip + S3_JUMP_BUFFER]
    mov qword ptr [rdx + 0], rbx
    mov qword ptr [rdx + 8], rbp
    mov qword ptr [rdx + 16], r12
    mov qword ptr [rdx + 24], r13
    mov qword ptr [rdx + 32], r14
    mov qword ptr [rdx + 40], r15
    mov qword ptr [rdx + 48], rsp
    lea rax, [rip + .Ls3_resume_context_return]
    mov qword ptr [rdx + 56], rax
    xor eax, eax
    ret
.Ls3_resume_context_return:
    mov eax, 1
    ret

.global s3_longjmp_resume
s3_longjmp_resume:
    lea rdx, [rip + S3_JUMP_BUFFER]
    mov rbx, qword ptr [rdx + 0]
    mov rbp, qword ptr [rdx + 8]
    mov r12, qword ptr [rdx + 16]
    mov r13, qword ptr [rdx + 24]
    mov r14, qword ptr [rdx + 32]
    mov r15, qword ptr [rdx + 40]
    mov rsp, qword ptr [rdx + 48]
    jmp qword ptr [rdx + 56]

.equ S3_TRAMP_BASE, 0x8000
.equ S3_TRAMP_PROT_ABS, S3_TRAMP_BASE + (s3_wake_trampoline_prot - s3_wake_trampoline_start)
.equ S3_TRAMP_LONG_ABS, S3_TRAMP_BASE + (s3_wake_trampoline_long - s3_wake_trampoline_start)
.equ S3_TRAMP_CR3_ABS, S3_TRAMP_BASE + (s3_wake_trampoline_cr3 - s3_wake_trampoline_start)
.equ S3_TRAMP_STACK_ABS, S3_TRAMP_BASE + (s3_wake_trampoline_stack - s3_wake_trampoline_start)
.equ S3_TRAMP_ENTRY_ABS, S3_TRAMP_BASE + (s3_wake_trampoline_entry - s3_wake_trampoline_start)
.equ S3_TRAMP_GDT_PTR_ABS, S3_TRAMP_BASE + (s3_wake_trampoline_gdt_ptr - s3_wake_trampoline_start)

.code16
.global s3_wake_trampoline_start
s3_wake_trampoline_start:
    cli
    cld
    mov al, 0xA0
    out 0x80, al
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7c00

    in al, 0x92
    or al, 0x02
    out 0x92, al

    lgdt [S3_TRAMP_GDT_PTR_ABS]
    mov al, 0xA1
    out 0x80, al

    mov eax, cr0
    or eax, 0x1
    mov cr0, eax
    push 0x18
    push S3_TRAMP_PROT_ABS
    retf

.code32
s3_wake_trampoline_prot:
    mov al, 0xA2
    out 0x80, al
    mov ax, 0x20
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    mov eax, dword ptr [S3_TRAMP_CR3_ABS]
    mov cr3, eax

    mov eax, cr4
    or eax, 0x20
    mov cr4, eax

    mov ecx, 0xC0000080
    rdmsr
    or eax, 0x100
    wrmsr

    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax

    push 0x08
    push S3_TRAMP_LONG_ABS
    retf

.code64
s3_wake_trampoline_long:
    mov al, 0xA3
    out 0x80, al
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    mov rsp, qword ptr [S3_TRAMP_STACK_ABS]
    mov rax, qword ptr [S3_TRAMP_ENTRY_ABS]
    jmp rax

.global s3_wake_trampoline_x64_entry
s3_wake_trampoline_x64_entry:
    cli
    cld
    mov al, 0xB0
    out 0x80, al
    mov rax, qword ptr [S3_TRAMP_CR3_ABS]
    mov cr3, rax
    mov rsp, qword ptr [S3_TRAMP_STACK_ABS]
    mov al, 0xB1
    out 0x80, al
    mov rax, qword ptr [S3_TRAMP_ENTRY_ABS]
    jmp rax

.align 8
.global s3_wake_trampoline_cr3
s3_wake_trampoline_cr3:
    .quad 0
.global s3_wake_trampoline_stack
s3_wake_trampoline_stack:
    .quad 0
.global s3_wake_trampoline_entry
s3_wake_trampoline_entry:
    .quad 0

.align 8
s3_wake_trampoline_gdt:
    .quad 0
    .quad 0x00AF9A000000FFFF
    .quad 0x00AF92000000FFFF
    .quad 0x00CF9A000000FFFF
    .quad 0x00CF92000000FFFF
s3_wake_trampoline_gdt_end:
s3_wake_trampoline_gdt_ptr:
    .word s3_wake_trampoline_gdt_end - s3_wake_trampoline_gdt - 1
    .long S3_TRAMP_BASE + (s3_wake_trampoline_gdt - s3_wake_trampoline_start)

.global s3_wake_trampoline_end
s3_wake_trampoline_end:
"#
);

#[repr(C, packed)]
struct AcpiRsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    _reserved: [u8; 3],
}

#[derive(Clone, Copy)]
struct AcpiRegister {
    space: u8,
    address: u64,
}

#[derive(Clone, Copy)]
struct AcpiS3Context {
    fadt_phys: u64,
    dsdt_phys: u64,
    facs_phys: u64,
    smi_cmd: u32,
    acpi_enable: u8,
    pm1a_evt: AcpiRegister,
    pm1b_evt: Option<AcpiRegister>,
    pm1a_cnt: AcpiRegister,
    pm1b_cnt: Option<AcpiRegister>,
    slp_typa: u8,
    slp_typb: u8,
}

pub fn s3_status_line() -> String {
    match discover_s3_context() {
        Ok(ctx) => format!(
            "ACPI S3 OK: FADT={:#x} DSDT={:#x} FACS={:#x} wake={} PM1a_CNT={:#x}/{} PM1b_CNT={} SLP_TYPa={} SLP_TYPb={}",
            ctx.fadt_phys,
            ctx.dsdt_phys,
            ctx.facs_phys,
            facs_wake_mode_name(&ctx),
            ctx.pm1a_cnt.address,
            register_space_name(ctx.pm1a_cnt.space),
            match ctx.pm1b_cnt {
                Some(reg) => format!("{:#x}/{}", reg.address, register_space_name(reg.space)),
                None => String::from("none"),
            },
            ctx.slp_typa,
            ctx.slp_typb
        ),
        Err(err) => format!("ACPI S3 no disponible: {}", err),
    }
}

pub fn try_suspend_to_ram() -> Result<(), &'static str> {
    let ctx = discover_s3_context()?;
    enable_acpi_if_needed(&ctx)?;
    prepare_wake_trampoline_and_facs(&ctx)?;
    clear_wake_status(&ctx);

    let timer_paused = crate::interrupts::suspend_irq_timer();
    let resume_path = unsafe { s3_set_resume_context() };
    if resume_path != 0 {
        finish_resume_path(&ctx, timer_paused);
        return Ok(());
    }

    flush_cpu_caches_for_sleep();
    write_sleep_enable(&ctx);

    // If the platform really entered S3, execution resumes here after wake and
    // WAK_STS is normally set. If not set after a short wait, treat it as a
    // firmware refusal so the GUI can fall back to soft suspend.
    let mut woke = false;
    let mut spins = 0usize;
    while spins < 6_000_000 {
        if wake_status_set(&ctx) {
            woke = true;
            break;
        }
        crate::hal::pause();
        spins += 1;
    }

    clear_wake_status(&ctx);
    clear_facs_wake_vector(&ctx);
    if timer_paused {
        crate::interrupts::resume_irq_timer();
    }

    if woke {
        Ok(())
    } else {
        Err("SLP_EN written, but firmware did not enter/wake from S3")
    }
}

fn finish_resume_path(ctx: &AcpiS3Context, timer_paused: bool) {
    unsafe { crate::hal::outb(0x80, 0xB2); }
    S3_RESUME_SEEN.store(1, Ordering::SeqCst);
    crate::interrupts::load_current_idt();
    clear_wake_status(ctx);
    clear_facs_wake_vector(ctx);
    if timer_paused {
        crate::interrupts::resume_irq_timer();
    }
    crate::privilege::init_privilege_layers_uefi_lite();
    crate::interrupts::load_current_idt();
    crate::interrupts::enable_irqs();
    unsafe { crate::hal::outb(0x80, 0xB3); }
}

fn register_space_name(space: u8) -> &'static str {
    match space {
        ACPI_ADDRESS_SPACE_SYSTEM_MEMORY => "mem",
        ACPI_ADDRESS_SPACE_SYSTEM_IO => "io",
        _ => "unknown",
    }
}

fn prepare_wake_trampoline_and_facs(ctx: &AcpiS3Context) -> Result<(), &'static str> {
    let cr3 = read_cr3();
    if cr3 > u32::MAX as u64 {
        return Err("S3 wake trampoline requires CR3 below 4GiB");
    }

    copy_wake_trampoline()?;
    patch_wake_trampoline(cr3, s3_wake_stack_top(), s3_longjmp_resume as *const () as u64);
    write_facs_wake_vector(ctx)?;
    S3_RESUME_SEEN.store(0, Ordering::SeqCst);
    Ok(())
}

fn s3_wake_stack_top() -> u64 {
    let base = unsafe { core::ptr::addr_of!(S3_WAKE_STACK.0) as *const u8 as u64 };
    (base + S3_WAKE_STACK_BYTES as u64) & !0xFu64
}

fn trampoline_size() -> usize {
    let start = unsafe { &s3_wake_trampoline_start as *const u8 as usize };
    let end = unsafe { &s3_wake_trampoline_end as *const u8 as usize };
    end.saturating_sub(start)
}

fn trampoline_offset(sym: *const u8) -> usize {
    let base = unsafe { &s3_wake_trampoline_start as *const u8 as usize };
    let addr = sym as usize;
    addr.saturating_sub(base)
}

fn copy_wake_trampoline() -> Result<(), &'static str> {
    let size = trampoline_size();
    if size == 0 || size > S3_TRAMPOLINE_MAX_BYTES {
        return Err("S3 wake trampoline size invalid");
    }
    unsafe {
        let src = &s3_wake_trampoline_start as *const u8;
        let dst = S3_TRAMPOLINE_PHYS as *mut u8;
        core::ptr::copy_nonoverlapping(src, dst, size);
        core::ptr::write_bytes(dst.add(size), 0, S3_TRAMPOLINE_MAX_BYTES - size);
    }
    core::sync::atomic::fence(Ordering::SeqCst);
    Ok(())
}

fn patch_wake_trampoline(cr3: u64, stack_top: u64, entry: u64) {
    unsafe {
        let base = S3_TRAMPOLINE_PHYS as *mut u8;
        let cr3_ptr = base.add(trampoline_offset(&s3_wake_trampoline_cr3 as *const u8)) as *mut u64;
        let stack_ptr = base.add(trampoline_offset(&s3_wake_trampoline_stack as *const u8)) as *mut u64;
        let entry_ptr = base.add(trampoline_offset(&s3_wake_trampoline_entry as *const u8)) as *mut u64;
        ptr::write_unaligned(cr3_ptr, cr3);
        ptr::write_unaligned(stack_ptr, stack_top);
        ptr::write_unaligned(entry_ptr, entry);
    }
    core::sync::atomic::fence(Ordering::SeqCst);
}

fn x64_wake_vector_phys() -> u64 {
    S3_TRAMPOLINE_PHYS
        + trampoline_offset(unsafe { &s3_wake_trampoline_x64_entry as *const u8 }) as u64
}

fn read_cr3() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

fn flush_cpu_caches_for_sleep() {
    unsafe {
        asm!("wbinvd", options(nostack, preserves_flags));
        crate::hal::outb(0x80, 0xC3);
    }
}

fn facs_len(facs_phys: u64) -> Option<usize> {
    if facs_phys == 0 {
        return None;
    }
    let sig = unsafe { ptr::read_unaligned(facs_phys as *const u32) };
    if sig != FACS_SIGNATURE {
        return None;
    }
    let len = unsafe { ptr::read_unaligned((facs_phys + 4) as *const u32) } as usize;
    if len < 32 || len > 4096 {
        return None;
    }
    Some(len)
}

fn facs_flags(facs_phys: u64, len: usize) -> u32 {
    if len < FACS_FLAGS_OFF + 4 {
        return 0;
    }
    unsafe { ptr::read_unaligned((facs_phys + FACS_FLAGS_OFF as u64) as *const u32) }
}

fn facs_supports_x64_wake(ctx: &AcpiS3Context, len: usize) -> bool {
    if len < FACS_X_FIRMWARE_WAKING_VECTOR_OFF + 8 || len < FACS_OSPM_FLAGS_OFF + 4 {
        return false;
    }
    (facs_flags(ctx.facs_phys, len) & FACS_64BIT_WAKE_SUPPORTED_F) != 0
}

fn facs_wake_mode_name(ctx: &AcpiS3Context) -> &'static str {
    let Some(len) = facs_len(ctx.facs_phys) else {
        return "no-facs";
    };
    if facs_supports_x64_wake(ctx, len) {
        "x64"
    } else {
        "legacy32"
    }
}

fn write_facs_wake_vector(ctx: &AcpiS3Context) -> Result<(), &'static str> {
    let len = facs_len(ctx.facs_phys).ok_or("FACS not found or invalid")?;
    if S3_TRAMPOLINE_PHYS > u32::MAX as u64 {
        return Err("S3 wake trampoline is above 32-bit Firmware_Waking_Vector range");
    }

    let use_x64 = facs_supports_x64_wake(ctx, len);
    unsafe {
        let fw = (ctx.facs_phys + FACS_FIRMWARE_WAKING_VECTOR_OFF as u64) as *mut u32;
        ptr::write_unaligned(fw, S3_TRAMPOLINE_PHYS as u32);

        if len >= FACS_X_FIRMWARE_WAKING_VECTOR_OFF + 8 {
            let xfw = (ctx.facs_phys + FACS_X_FIRMWARE_WAKING_VECTOR_OFF as u64) as *mut u64;
            ptr::write_unaligned(xfw, if use_x64 { x64_wake_vector_phys() } else { 0 });
        }

        if len >= FACS_OSPM_FLAGS_OFF + 4 {
            let flags_ptr = (ctx.facs_phys + FACS_OSPM_FLAGS_OFF as u64) as *mut u32;
            let flags = ptr::read_unaligned(flags_ptr);
            let next_flags = if use_x64 {
                flags | FACS_OSPM_64BIT_WAKE_F
            } else {
                flags & !FACS_OSPM_64BIT_WAKE_F
            };
            ptr::write_unaligned(flags_ptr, next_flags);
        }
    }
    core::sync::atomic::fence(Ordering::SeqCst);
    Ok(())
}

fn clear_facs_wake_vector(ctx: &AcpiS3Context) {
    let Some(len) = facs_len(ctx.facs_phys) else {
        return;
    };
    unsafe {
        let fw = (ctx.facs_phys + FACS_FIRMWARE_WAKING_VECTOR_OFF as u64) as *mut u32;
        ptr::write_unaligned(fw, 0);
        if len >= FACS_X_FIRMWARE_WAKING_VECTOR_OFF + 8 {
            let xfw = (ctx.facs_phys + FACS_X_FIRMWARE_WAKING_VECTOR_OFF as u64) as *mut u64;
            ptr::write_unaligned(xfw, 0);
        }
        if len >= FACS_OSPM_FLAGS_OFF + 4 {
            let flags_ptr = (ctx.facs_phys + FACS_OSPM_FLAGS_OFF as u64) as *mut u32;
            let flags = ptr::read_unaligned(flags_ptr);
            ptr::write_unaligned(flags_ptr, flags & !FACS_OSPM_64BIT_WAKE_F);
        }
    }
    core::sync::atomic::fence(Ordering::SeqCst);
}

fn discover_s3_context() -> Result<AcpiS3Context, &'static str> {
    let rsdp = find_rsdp().ok_or("RSDP not found")?;
    let revision = unsafe { ptr::read_unaligned(ptr::addr_of!((*rsdp).revision)) };
    let xsdt = unsafe { ptr::read_unaligned(ptr::addr_of!((*rsdp).xsdt_address)) };
    let rsdt = unsafe { ptr::read_unaligned(ptr::addr_of!((*rsdp).rsdt_address)) };

    let fadt_phys = if revision >= 2 && xsdt != 0 {
        find_table_in_xsdt(xsdt, FADT_SIGNATURE)
    } else {
        find_table_in_rsdt(rsdt as u64, FADT_SIGNATURE)
    }
    .ok_or("FADT/FACP not found")?;

    parse_fadt_for_s3(fadt_phys)
}

fn find_rsdp() -> Option<*const AcpiRsdp> {
    if let Some(p) = find_rsdp_uefi() {
        return Some(p);
    }
    scan_for_rsdp(0xE0000, 0x20000)
}

fn find_rsdp_uefi() -> Option<*const AcpiRsdp> {
    let acpi20_guid = uefi::Guid::from_bytes([
        0x71, 0xe8, 0x68, 0x88, 0xf1, 0xe4, 0xd3, 0x11,
        0xbc, 0x22, 0x00, 0x80, 0xc7, 0x3c, 0x88, 0x81,
    ]);
    let acpi10_guid = uefi::Guid::from_bytes([
        0x30, 0x2d, 0x9d, 0xeb, 0x88, 0x2d, 0xd3, 0x11,
        0x9a, 0x16, 0x00, 0x90, 0x27, 0x3f, 0xc1, 0x4d,
    ]);

    uefi::system::with_config_table(|entries| {
        for entry in entries {
            if entry.guid == acpi20_guid {
                let ptr = entry.address as *const AcpiRsdp;
                if validate_rsdp(ptr) {
                    return Some(ptr);
                }
            }
        }
        for entry in entries {
            if entry.guid == acpi10_guid {
                let ptr = entry.address as *const AcpiRsdp;
                if validate_rsdp(ptr) {
                    return Some(ptr);
                }
            }
        }
        None
    })
}

fn scan_for_rsdp(base: u64, length: u64) -> Option<*const AcpiRsdp> {
    let mut addr = base;
    while addr + 20 <= base + length {
        let ptr = addr as *const AcpiRsdp;
        if validate_rsdp(ptr) {
            return Some(ptr);
        }
        addr += 16;
    }
    None
}

fn validate_rsdp(ptr: *const AcpiRsdp) -> bool {
    if ptr.is_null() {
        return false;
    }
    let sig = unsafe { ptr::read_unaligned(ptr::addr_of!((*ptr).signature)) };
    if sig != RSDP_SIGNATURE {
        return false;
    }
    checksum_ok(ptr as *const u8, 20)
}

fn table_len(addr: u64) -> Option<usize> {
    if addr == 0 {
        return None;
    }
    let len = unsafe { ptr::read_unaligned((addr + 4) as *const u32) } as usize;
    if len < 36 || len > 1024 * 1024 {
        return None;
    }
    Some(len)
}

fn table_signature(addr: u64) -> Option<u32> {
    if addr == 0 {
        return None;
    }
    Some(unsafe { ptr::read_unaligned(addr as *const u32) })
}

fn table_valid(addr: u64, signature: u32) -> bool {
    table_signature(addr) == Some(signature)
        && table_len(addr)
            .map(|len| checksum_ok(addr as *const u8, len))
            .unwrap_or(false)
}

fn checksum_ok(ptr: *const u8, len: usize) -> bool {
    if ptr.is_null() || len == 0 {
        return false;
    }
    let bytes = unsafe { slice::from_raw_parts(ptr, len) };
    bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b)) == 0
}

fn find_table_in_xsdt(xsdt_phys: u64, signature: u32) -> Option<u64> {
    if !table_valid(xsdt_phys, u32::from_le_bytes(*b"XSDT")) {
        return None;
    }
    let total_len = table_len(xsdt_phys)?;
    let entry_count = (total_len - 36) / 8;
    let entries_base = xsdt_phys + 36;
    let mut i = 0usize;
    while i < entry_count {
        let table_addr = unsafe {
            ptr::read_unaligned((entries_base + (i * 8) as u64) as *const u64)
        };
        if table_valid(table_addr, signature) {
            return Some(table_addr);
        }
        i += 1;
    }
    None
}

fn find_table_in_rsdt(rsdt_phys: u64, signature: u32) -> Option<u64> {
    if !table_valid(rsdt_phys, u32::from_le_bytes(*b"RSDT")) {
        return None;
    }
    let total_len = table_len(rsdt_phys)?;
    let entry_count = (total_len - 36) / 4;
    let entries_base = rsdt_phys + 36;
    let mut i = 0usize;
    while i < entry_count {
        let table_addr = unsafe {
            ptr::read_unaligned((entries_base + (i * 4) as u64) as *const u32)
        } as u64;
        if table_valid(table_addr, signature) {
            return Some(table_addr);
        }
        i += 1;
    }
    None
}

fn parse_fadt_for_s3(fadt_phys: u64) -> Result<AcpiS3Context, &'static str> {
    if !table_valid(fadt_phys, FADT_SIGNATURE) {
        return Err("FADT checksum/signature invalid");
    }
    let len = table_len(fadt_phys).ok_or("FADT length invalid")?;

    let facs_phys = choose_u64_field(fadt_phys, len, 36, 132);
    let dsdt_phys = choose_u64_field(fadt_phys, len, 40, 140);
    if dsdt_phys == 0 || !table_valid(dsdt_phys, DSDT_SIGNATURE) {
        return Err("DSDT not found or invalid");
    }

    let smi_cmd = read_u32_at(fadt_phys, len, 48).unwrap_or(0);
    let acpi_enable = read_u8_at(fadt_phys, len, 52).unwrap_or(0);
    let pm1_evt_len = read_u8_at(fadt_phys, len, 88).unwrap_or(4);
    let pm1_cnt_len = read_u8_at(fadt_phys, len, 89).unwrap_or(2);

    if pm1_evt_len < 4 {
        return Err("FADT PM1 event block length invalid");
    }
    if pm1_cnt_len < 2 {
        return Err("FADT PM1 control block length invalid");
    }

    let pm1a_evt = choose_register(fadt_phys, len, 56, 148)
        .ok_or("PM1a event block missing")?;
    let pm1b_evt = choose_register(fadt_phys, len, 60, 160);
    let pm1a_cnt = choose_register(fadt_phys, len, 64, 172)
        .ok_or("PM1a control block missing")?;
    let pm1b_cnt = choose_register(fadt_phys, len, 68, 184);
    let (slp_typa, slp_typb) = find_s3_sleep_type(dsdt_phys)?;

    Ok(AcpiS3Context {
        fadt_phys,
        dsdt_phys,
        facs_phys,
        smi_cmd,
        acpi_enable,
        pm1a_evt,
        pm1b_evt,
        pm1a_cnt,
        pm1b_cnt,
        slp_typa,
        slp_typb,
    })
}

fn choose_u64_field(base: u64, len: usize, legacy_off: usize, x_off: usize) -> u64 {
    let legacy = read_u32_at(base, len, legacy_off).unwrap_or(0) as u64;
    let extended = read_u64_at(base, len, x_off).unwrap_or(0);
    if extended != 0 { extended } else { legacy }
}

fn choose_register(base: u64, len: usize, legacy_off: usize, gas_off: usize) -> Option<AcpiRegister> {
    if let Some(reg) = read_gas_at(base, len, gas_off) {
        if reg.address != 0
            && (reg.space == ACPI_ADDRESS_SPACE_SYSTEM_IO
                || reg.space == ACPI_ADDRESS_SPACE_SYSTEM_MEMORY)
        {
            return Some(reg);
        }
    }
    let legacy = read_u32_at(base, len, legacy_off).unwrap_or(0);
    if legacy == 0 {
        None
    } else {
        Some(AcpiRegister {
            space: ACPI_ADDRESS_SPACE_SYSTEM_IO,
            address: legacy as u64,
        })
    }
}

fn read_gas_at(base: u64, len: usize, off: usize) -> Option<AcpiRegister> {
    if off + 12 > len {
        return None;
    }
    let space = read_u8_at(base, len, off)?;
    let address = read_u64_at(base, len, off + 4)?;
    Some(AcpiRegister { space, address })
}

fn read_u8_at(base: u64, len: usize, off: usize) -> Option<u8> {
    if off + 1 > len {
        return None;
    }
    Some(unsafe { ptr::read_unaligned((base + off as u64) as *const u8) })
}

fn read_u16_at(base: u64, len: usize, off: usize) -> Option<u16> {
    if off + 2 > len {
        return None;
    }
    Some(unsafe { ptr::read_unaligned((base + off as u64) as *const u16) })
}

fn read_u32_at(base: u64, len: usize, off: usize) -> Option<u32> {
    if off + 4 > len {
        return None;
    }
    Some(unsafe { ptr::read_unaligned((base + off as u64) as *const u32) })
}

fn read_u64_at(base: u64, len: usize, off: usize) -> Option<u64> {
    if off + 8 > len {
        return None;
    }
    Some(unsafe { ptr::read_unaligned((base + off as u64) as *const u64) })
}

fn find_s3_sleep_type(dsdt_phys: u64) -> Result<(u8, u8), &'static str> {
    let len = table_len(dsdt_phys).ok_or("DSDT length invalid")?;
    let bytes = unsafe { slice::from_raw_parts(dsdt_phys as *const u8, len) };
    let mut i = 36usize;
    while i + 4 < bytes.len() {
        if &bytes[i..i + 4] == b"_S3_" {
            if let Some((a, b)) = parse_s3_package(bytes, i + 4) {
                return Ok(((a & 0x7) as u8, (b & 0x7) as u8));
            }
        }
        i += 1;
    }
    Err("_S3 package not found in DSDT")
}

fn parse_s3_package(bytes: &[u8], offset: usize) -> Option<(u64, u64)> {
    let mut pos = offset;
    let scan_end = bytes.len().min(offset.saturating_add(16));
    while pos < scan_end {
        if bytes.get(pos).copied()? == 0x12 {
            let mut cur = pos + 1;
            let (_pkg_len, pkg_len_bytes) = parse_aml_pkg_length(bytes, cur)?;
            cur += pkg_len_bytes;
            let _element_count = *bytes.get(cur)?;
            cur += 1;
            let (slp_a, used_a) = parse_aml_integer(bytes, cur)?;
            cur += used_a;
            let (slp_b, _used_b) = parse_aml_integer(bytes, cur)?;
            return Some((slp_a, slp_b));
        }
        pos += 1;
    }
    None
}

fn parse_aml_pkg_length(bytes: &[u8], offset: usize) -> Option<(usize, usize)> {
    let lead = *bytes.get(offset)? as usize;
    let byte_count = lead >> 6;
    if byte_count == 0 {
        return Some((lead & 0x3F, 1));
    }

    let mut len = lead & 0x0F;
    let mut i = 0usize;
    while i < byte_count {
        let b = *bytes.get(offset + 1 + i)? as usize;
        len |= b << (4 + i * 8);
        i += 1;
    }
    Some((len, 1 + byte_count))
}

fn parse_aml_integer(bytes: &[u8], offset: usize) -> Option<(u64, usize)> {
    let op = *bytes.get(offset)?;
    match op {
        0x00 => Some((0, 1)),
        0x01 => Some((1, 1)),
        0x0A => Some((*bytes.get(offset + 1)? as u64, 2)),
        0x0B => {
            let lo = *bytes.get(offset + 1)? as u16;
            let hi = *bytes.get(offset + 2)? as u16;
            Some((((hi << 8) | lo) as u64, 3))
        }
        0x0C => {
            if offset + 5 > bytes.len() {
                return None;
            }
            let v = unsafe { ptr::read_unaligned(bytes.as_ptr().add(offset + 1) as *const u32) };
            Some((v as u64, 5))
        }
        0x0E => {
            if offset + 9 > bytes.len() {
                return None;
            }
            let v = unsafe { ptr::read_unaligned(bytes.as_ptr().add(offset + 1) as *const u64) };
            Some((v, 9))
        }
        0xFF => Some((u64::MAX, 1)),
        _ => None,
    }
}

fn enable_acpi_if_needed(ctx: &AcpiS3Context) -> Result<(), &'static str> {
    if (read_register_u16(ctx.pm1a_cnt) & PM1_CNT_SCI_EN) != 0 {
        return Ok(());
    }
    if ctx.smi_cmd == 0 || ctx.acpi_enable == 0 {
        return Err("ACPI mode is disabled and FADT has no SMI enable command");
    }

    unsafe { crate::hal::outb(ctx.smi_cmd as u16, ctx.acpi_enable) };
    let mut spins = 0usize;
    while spins < 1_000_000 {
        if (read_register_u16(ctx.pm1a_cnt) & PM1_CNT_SCI_EN) != 0 {
            return Ok(());
        }
        crate::hal::pause();
        spins += 1;
    }
    Err("ACPI enable command did not set SCI_EN")
}

fn clear_wake_status(ctx: &AcpiS3Context) {
    write_register_u16(ctx.pm1a_evt, PM1_STS_WAK);
    if let Some(reg) = ctx.pm1b_evt {
        write_register_u16(reg, PM1_STS_WAK);
    }
}

fn wake_status_set(ctx: &AcpiS3Context) -> bool {
    (read_register_u16(ctx.pm1a_evt) & PM1_STS_WAK) != 0
        || ctx
            .pm1b_evt
            .map(|reg| (read_register_u16(reg) & PM1_STS_WAK) != 0)
            .unwrap_or(false)
}

fn write_sleep_enable(ctx: &AcpiS3Context) {
    let pm1a = read_register_u16(ctx.pm1a_cnt);
    let sleep_a = (pm1a & !PM1_CNT_SLP_TYP_MASK)
        | (((ctx.slp_typa as u16) & 0x7) << PM1_CNT_SLP_TYP_SHIFT)
        | PM1_CNT_SLP_EN;
    write_register_u16(ctx.pm1a_cnt, sleep_a);

    if let Some(reg) = ctx.pm1b_cnt {
        let pm1b = read_register_u16(reg);
        let sleep_b = (pm1b & !PM1_CNT_SLP_TYP_MASK)
            | (((ctx.slp_typb as u16) & 0x7) << PM1_CNT_SLP_TYP_SHIFT)
            | PM1_CNT_SLP_EN;
        write_register_u16(reg, sleep_b);
    }
}

fn read_register_u16(reg: AcpiRegister) -> u16 {
    match reg.space {
        ACPI_ADDRESS_SPACE_SYSTEM_IO => unsafe { crate::hal::inw(reg.address as u16) },
        ACPI_ADDRESS_SPACE_SYSTEM_MEMORY => unsafe { (reg.address as *const u16).read_volatile() },
        _ => 0,
    }
}

fn write_register_u16(reg: AcpiRegister, value: u16) {
    match reg.space {
        ACPI_ADDRESS_SPACE_SYSTEM_IO => unsafe { crate::hal::outw(reg.address as u16, value) },
        ACPI_ADDRESS_SPACE_SYSTEM_MEMORY => unsafe { (reg.address as *mut u16).write_volatile(value) },
        _ => {}
    }
}
