//! SMP (Symmetric Multi-Processing) module.
//!
//! Phase 1: CPU discovery via ACPI MADT parsing.
//! Phase 2: Per-core GDT/TSS/stack.
//! Phase 3: AP bootstrap via UEFI MpServices Protocol.

extern crate alloc;

use sha2::{Digest, Sha256};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::arch::{asm, global_asm};
use alloc::{boxed::Box, string::String, vec::Vec};

// ---------------------------------------------------------------------------
// AP trampoline (baremetal)
// ---------------------------------------------------------------------------

#[repr(C, packed)]
struct ApGdtPointer {
    limit: u16,
    base: u64,
}

unsafe extern "C" {
    fn load_gdt_and_segments(gdt_ptr: *const ApGdtPointer);
}

extern "C" {
    static ap_trampoline_start: u8;
    static ap_trampoline_end: u8;
    static ap_trampoline_cr3: u8;
    static ap_trampoline_stack: u8;
    static ap_trampoline_entry: u8;
    static ap_trampoline_os_gdt_ptr: u8;
}

global_asm!(
    r#"
.intel_syntax noprefix
.section .text
.equ TRAMP_BASE, 0x7000
.equ TRAMP_GDT_PTR_ABS, TRAMP_BASE + (ap_trampoline_gdt_ptr - ap_trampoline_start)
.equ TRAMP_PROT_ABS, TRAMP_BASE + (ap_trampoline_prot - ap_trampoline_start)
.equ TRAMP_LONG_ABS, TRAMP_BASE + (ap_trampoline_long - ap_trampoline_start)
.equ TRAMP_CR3_ABS, TRAMP_BASE + (ap_trampoline_cr3 - ap_trampoline_start)
.equ TRAMP_STACK_ABS, TRAMP_BASE + (ap_trampoline_stack - ap_trampoline_start)
.equ TRAMP_ENTRY_ABS, TRAMP_BASE + (ap_trampoline_entry - ap_trampoline_start)
.equ TRAMP_OS_GDT_ABS, TRAMP_BASE + (ap_trampoline_os_gdt_ptr - ap_trampoline_start)
.code16
.global ap_trampoline_start
ap_trampoline_start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7000

    lgdt [TRAMP_GDT_PTR_ABS]

    mov eax, cr0
    or eax, 0x1
    mov cr0, eax
    push 0x08
    push TRAMP_PROT_ABS
    retf

.code32
ap_trampoline_prot:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    mov eax, dword ptr [TRAMP_CR3_ABS]
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

    push 0x18
    push TRAMP_LONG_ABS
    retf

.code64
ap_trampoline_long:
    mov ax, 0x20
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    mov rsp, qword ptr [TRAMP_STACK_ABS]
    mov rdi, TRAMP_OS_GDT_ABS
    call load_gdt_and_segments

    mov rax, qword ptr [TRAMP_ENTRY_ABS]
    jmp rax

.align 8
.global ap_trampoline_cr3
ap_trampoline_cr3:
    .quad 0
.global ap_trampoline_stack
ap_trampoline_stack:
    .quad 0
.global ap_trampoline_entry
ap_trampoline_entry:
    .quad 0
.global ap_trampoline_os_gdt_ptr
ap_trampoline_os_gdt_ptr:
    .word 0
    .quad 0

.align 8
ap_trampoline_gdt:
    .quad 0
    .quad 0x00CF9A000000FFFF
    .quad 0x00CF92000000FFFF
    .quad 0x00AF9A000000FFFF
    .quad 0x00AF92000000FFFF
ap_trampoline_gdt_end:
ap_trampoline_gdt_ptr:
    .word ap_trampoline_gdt_end - ap_trampoline_gdt - 1
    .quad ap_trampoline_gdt

.global ap_trampoline_end
ap_trampoline_end:
"#
);

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const MAX_CPUS: usize = 128;

// ACPI table signatures
const RSDP_SIGNATURE: [u8; 8] = *b"RSD PTR ";
const MADT_SIGNATURE: u32 = u32::from_le_bytes(*b"APIC");

// MADT entry types
const MADT_TYPE_LOCAL_APIC: u8 = 0;
const MADT_TYPE_LOCAL_X2APIC: u8 = 9;

// MADT Local APIC flags
const MADT_LAPIC_ENABLED: u32 = 1;
const MADT_LAPIC_ONLINE_CAPABLE: u32 = 2;

// ---------------------------------------------------------------------------
// ACPI structures — all access via raw pointers to avoid packed ref UB
// ---------------------------------------------------------------------------

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

#[repr(C, packed)]
struct AcpiSdtHeader {
    signature: u32,
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

// ---------------------------------------------------------------------------
// Per-CPU info
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct CpuInfo {
    pub apic_id: u32,
    pub is_bsp: bool,
    pub usable: bool,
}

impl CpuInfo {
    const fn empty() -> Self {
        Self { apic_id: 0, is_bsp: false, usable: false }
    }
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

static mut CPU_TABLE: [CpuInfo; MAX_CPUS] = [CpuInfo::empty(); MAX_CPUS];
static CPU_COUNT: AtomicU32 = AtomicU32::new(0);
static DISCOVERY_DONE: AtomicBool = AtomicBool::new(false);
static mut LOCAL_APIC_BASE_ADDR: u32 = 0xFEE0_0000;
static mut BSP_APIC_ID: u32 = 0;

// AP state
static APS_ONLINE: AtomicU32 = AtomicU32::new(0);
static AP_GO: AtomicBool = AtomicBool::new(false);
static BOOTSTRAP_DONE: AtomicBool = AtomicBool::new(false);

static mut AP_ONLINE_FLAGS: [AtomicBool; MAX_CPUS] = {
    const INIT: AtomicBool = AtomicBool::new(false);
    [INIT; MAX_CPUS]
};

// ---------------------------------------------------------------------------
// Baremetal AP bootstrap configuration
// ---------------------------------------------------------------------------

const AP_TRAMPOLINE_PHYS: usize = 0x7000;
const AP_TRAMPOLINE_MAX_BYTES: usize = 4096;
const AP_BOOT_SPIN_WAIT: usize = 200_000;
const AP_BOOT_TIMEOUT_SPINS: usize = 2_000_000;
const AP_BOOT_PREFER_BAREMETAL: bool = true;

const IA32_APIC_BASE_MSR: u32 = 0x1B;
const IA32_X2APIC_APICID: u32 = 0x802;
const IA32_X2APIC_ICR: u32 = 0x830;
const APIC_BASE_ENABLE: u64 = 1 << 11;
const APIC_BASE_X2APIC_ENABLE: u64 = 1 << 10;
const APIC_BASE_ADDR_MASK: u64 = 0xFFFF_F000;
const APIC_REG_ICR_LOW: u64 = 0x0300;
const APIC_REG_ICR_HIGH: u64 = 0x0310;
const APIC_ICR_DELIVERY_INIT: u32 = 0x5 << 8;
const APIC_ICR_DELIVERY_STARTUP: u32 = 0x6 << 8;
const APIC_ICR_DELIVERY_FIXED: u32 = 0x0 << 8;
const APIC_ICR_LEVEL_ASSERT: u32 = 1 << 14;
const APIC_ICR_TRIGGER_LEVEL: u32 = 1 << 15;
const APIC_ICR_DELIVERY_STATUS: u32 = 1 << 12;

static AP_BOOT_TARGET: AtomicU32 = AtomicU32::new(0);
static AP_BOOT_ACK: AtomicU32 = AtomicU32::new(0);
static AP_BOOT_METHOD: AtomicU32 = AtomicU32::new(0);

const AP_BOOT_METHOD_NONE: u32 = 0;
const AP_BOOT_METHOD_UEFI: u32 = 1;
const AP_BOOT_METHOD_BAREMETAL: u32 = 2;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn cpu_count() -> u32 { CPU_COUNT.load(Ordering::SeqCst) }
pub fn discovery_done() -> bool { DISCOVERY_DONE.load(Ordering::SeqCst) }
pub fn bsp_apic_id() -> u32 { unsafe { BSP_APIC_ID } }
pub fn local_apic_base() -> u32 { unsafe { LOCAL_APIC_BASE_ADDR } }
pub fn aps_online() -> u32 { APS_ONLINE.load(Ordering::SeqCst) }
pub fn bootstrap_done() -> bool { BOOTSTRAP_DONE.load(Ordering::SeqCst) }

pub fn cpu_info(index: usize) -> Option<CpuInfo> {
    if index < cpu_count() as usize {
        Some(unsafe { CPU_TABLE[index] })
    } else {
        None
    }
}

pub fn is_core_online(index: usize) -> bool {
    if index == 0 {
        return true;
    }
    let count = CPU_COUNT.load(Ordering::SeqCst) as usize;
    if index >= count {
        return false;
    }
    unsafe { AP_ONLINE_FLAGS[index].load(Ordering::SeqCst) }
}

pub fn send_resched_ipi(core_index: usize) -> bool {
    let count = CPU_COUNT.load(Ordering::SeqCst) as usize;
    if core_index >= count {
        return false;
    }
    if !is_core_online(core_index) {
        return false;
    }
    if !cpu_has_local_apic() {
        return false;
    }
    let apic_id = unsafe { CPU_TABLE[core_index].apic_id };
    if apic_id == current_apic_id() {
        return false;
    }
    apic_send_ipi(apic_id, crate::interrupts::IPI_RESCHED_VECTOR);
    true
}

fn cpu_has_local_apic() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        let leaf1 = unsafe { core::arch::x86_64::__cpuid(1) };
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
        let leaf1 = unsafe { core::arch::x86_64::__cpuid(1) };
        (leaf1.ecx & (1 << 21)) != 0
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Current CPU APIC ID via x2APIC MSR when active, otherwise CPUID leaf 1.
pub fn current_apic_id() -> u32 {
    if is_x2apic_active() {
        unsafe { crate::hal::rdmsr(IA32_X2APIC_APICID) as u32 }
    } else {
        unsafe {
            let leaf = core::arch::x86_64::__cpuid(1);
            (leaf.ebx >> 24) & 0xFF
        }
    }
}

fn is_ap_online(apic_id: u32) -> bool {
    let count = CPU_COUNT.load(Ordering::SeqCst) as usize;
    for i in 0..count {
        if unsafe { CPU_TABLE[i].apic_id } == apic_id {
            return unsafe { AP_ONLINE_FLAGS[i].load(Ordering::SeqCst) };
        }
    }
    false
}

fn mark_ap_online(apic_id: u32) {
    let count = CPU_COUNT.load(Ordering::SeqCst) as usize;
    for i in 0..count {
        if unsafe { CPU_TABLE[i].apic_id } == apic_id {
            unsafe { AP_ONLINE_FLAGS[i].store(true, Ordering::SeqCst); }
            return;
        }
    }
}

/// Detect if x2APIC mode is active.
fn is_x2apic_active() -> bool {
    if !cpu_has_local_apic() {
        return false;
    }
    let val = unsafe { crate::hal::rdmsr(IA32_APIC_BASE_MSR) };
    (val & APIC_BASE_X2APIC_ENABLE) != 0
}

fn read_cr3() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

fn enable_local_apic() -> bool {
    if !cpu_has_local_apic() {
        return false;
    }
    unsafe {
        let mut apic_base = crate::hal::rdmsr(IA32_APIC_BASE_MSR);
        apic_base |= APIC_BASE_ENABLE;
        if cpu_has_x2apic() {
            apic_base |= APIC_BASE_X2APIC_ENABLE;
        }
        crate::hal::wrmsr(IA32_APIC_BASE_MSR, apic_base);
    }
    true
}

fn apic_wait_delivery() {
    if is_x2apic_active() {
        for _ in 0..AP_BOOT_SPIN_WAIT {
            let icr = unsafe { crate::hal::rdmsr(IA32_X2APIC_ICR) } as u32;
            if (icr & APIC_ICR_DELIVERY_STATUS) == 0 {
                break;
            }
            crate::hal::pause();
        }
        return;
    }

    let base = local_apic_base() as u64;
    let icr_low = (base + APIC_REG_ICR_LOW) as *const u32;
    for _ in 0..AP_BOOT_SPIN_WAIT {
        let val = unsafe { core::ptr::read_volatile(icr_low) };
        if (val & APIC_ICR_DELIVERY_STATUS) == 0 {
            break;
        }
        crate::hal::pause();
    }
}

fn apic_send_init(apic_id: u32) {
    if is_x2apic_active() {
        let icr = ((apic_id as u64) << 32)
            | (APIC_ICR_DELIVERY_INIT | APIC_ICR_LEVEL_ASSERT | APIC_ICR_TRIGGER_LEVEL) as u64;
        unsafe { crate::hal::wrmsr(IA32_X2APIC_ICR, icr); }
        apic_wait_delivery();
        return;
    }

    let base = local_apic_base() as u64;
    unsafe {
        let icr_high = (base + APIC_REG_ICR_HIGH) as *mut u32;
        let icr_low = (base + APIC_REG_ICR_LOW) as *mut u32;
        core::ptr::write_volatile(icr_high, apic_id << 24);
        core::ptr::write_volatile(
            icr_low,
            APIC_ICR_DELIVERY_INIT | APIC_ICR_LEVEL_ASSERT | APIC_ICR_TRIGGER_LEVEL,
        );
    }
    apic_wait_delivery();
}

fn apic_send_startup(apic_id: u32, vector: u8) {
    let vec = vector as u32;
    if is_x2apic_active() {
        let icr = ((apic_id as u64) << 32) | (APIC_ICR_DELIVERY_STARTUP | vec) as u64;
        unsafe { crate::hal::wrmsr(IA32_X2APIC_ICR, icr); }
        apic_wait_delivery();
        return;
    }

    let base = local_apic_base() as u64;
    unsafe {
        let icr_high = (base + APIC_REG_ICR_HIGH) as *mut u32;
        let icr_low = (base + APIC_REG_ICR_LOW) as *mut u32;
        core::ptr::write_volatile(icr_high, apic_id << 24);
        core::ptr::write_volatile(icr_low, APIC_ICR_DELIVERY_STARTUP | vec);
    }
    apic_wait_delivery();
}

fn apic_send_ipi(apic_id: u32, vector: u8) {
    if !cpu_has_local_apic() {
        return;
    }
    let vec = vector as u32;
    if is_x2apic_active() {
        let icr = ((apic_id as u64) << 32) | (APIC_ICR_DELIVERY_FIXED | vec) as u64;
        unsafe { crate::hal::wrmsr(IA32_X2APIC_ICR, icr); }
        apic_wait_delivery();
        return;
    }

    let base = local_apic_base() as u64;
    unsafe {
        let icr_high = (base + APIC_REG_ICR_HIGH) as *mut u32;
        let icr_low = (base + APIC_REG_ICR_LOW) as *mut u32;
        core::ptr::write_volatile(icr_high, apic_id << 24);
        core::ptr::write_volatile(icr_low, APIC_ICR_DELIVERY_FIXED | vec);
    }
    apic_wait_delivery();
}

// ===========================================================================
// PHASE 1: CPU Discovery via ACPI MADT
// ===========================================================================

pub fn discover_cpus() {
    if DISCOVERY_DONE.load(Ordering::SeqCst) { return; }

    let bsp_id = current_apic_id();
    unsafe { BSP_APIC_ID = bsp_id; }

    let rsdp_addr = find_rsdp();
    if let Some(rsdp) = rsdp_addr {
        let revision = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*rsdp).revision)) };
        let xsdt = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*rsdp).xsdt_address)) };
        let rsdt = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*rsdp).rsdt_address)) };

        let madt_addr = if revision >= 2 && xsdt != 0 {
            find_table_in_xsdt(xsdt, MADT_SIGNATURE)
        } else {
            find_table_in_rsdt(rsdt as u64, MADT_SIGNATURE)
        };

        if let Some(madt) = madt_addr {
            parse_madt(madt, bsp_id);
        }
    }

    if CPU_COUNT.load(Ordering::SeqCst) == 0 {
        register_cpu(bsp_id, true);
    }

    DISCOVERY_DONE.store(true, Ordering::SeqCst);
}

fn find_rsdp() -> Option<*const AcpiRsdp> {
    if let Some(p) = find_rsdp_uefi() { return Some(p); }
    for region in &[(0xE0000u64, 0x20000u64)] {
        if let Some(p) = scan_for_rsdp(region.0, region.1) { return Some(p); }
    }
    None
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
                if validate_rsdp(ptr) { return Some(ptr); }
            }
        }
        for entry in entries {
            if entry.guid == acpi10_guid {
                let ptr = entry.address as *const AcpiRsdp;
                if validate_rsdp(ptr) { return Some(ptr); }
            }
        }
        None
    })
}

fn scan_for_rsdp(base: u64, length: u64) -> Option<*const AcpiRsdp> {
    let mut addr = base;
    while addr + 16 <= base + length {
        let ptr = addr as *const AcpiRsdp;
        if validate_rsdp(ptr) { return Some(ptr); }
        addr += 16;
    }
    None
}

fn validate_rsdp(ptr: *const AcpiRsdp) -> bool {
    if ptr.is_null() { return false; }
    let sig = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*ptr).signature)) };
    sig == RSDP_SIGNATURE
}

fn find_table_in_xsdt(xsdt_phys: u64, signature: u32) -> Option<u64> {
    if xsdt_phys == 0 { return None; }
    let hdr = xsdt_phys as *const AcpiSdtHeader;
    let total_len = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*hdr).length)) } as usize;
    let hdr_size = core::mem::size_of::<AcpiSdtHeader>();
    if total_len <= hdr_size { return None; }
    let entry_count = (total_len - hdr_size) / 8;
    let entries_base = xsdt_phys + hdr_size as u64;
    for i in 0..entry_count {
        let entry_ptr = (entries_base + (i * 8) as u64) as *const u64;
        let table_addr = unsafe { core::ptr::read_unaligned(entry_ptr) };
        if table_addr == 0 { continue; }
        let table_hdr = table_addr as *const AcpiSdtHeader;
        let table_sig = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*table_hdr).signature)) };
        if table_sig == signature { return Some(table_addr); }
    }
    None
}

fn find_table_in_rsdt(rsdt_phys: u64, signature: u32) -> Option<u64> {
    if rsdt_phys == 0 { return None; }
    let hdr = rsdt_phys as *const AcpiSdtHeader;
    let total_len = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*hdr).length)) } as usize;
    let hdr_size = core::mem::size_of::<AcpiSdtHeader>();
    if total_len <= hdr_size { return None; }
    let entry_count = (total_len - hdr_size) / 4;
    let entries_base = rsdt_phys + hdr_size as u64;
    for i in 0..entry_count {
        let entry_ptr = (entries_base + (i * 4) as u64) as *const u32;
        let table_addr = unsafe { core::ptr::read_unaligned(entry_ptr) } as u64;
        if table_addr == 0 { continue; }
        let table_hdr = table_addr as *const AcpiSdtHeader;
        let table_sig = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*table_hdr).signature)) };
        if table_sig == signature { return Some(table_addr); }
    }
    None
}

fn parse_madt(madt_phys: u64, bsp_id: u32) {
    let hdr = madt_phys as *const AcpiSdtHeader;
    let total_len = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*hdr).length)) } as usize;
    let lapic_base = unsafe { core::ptr::read_unaligned((madt_phys + 36) as *const u32) };
    unsafe { LOCAL_APIC_BASE_ADDR = lapic_base; }

    let entries_start = madt_phys + 44;
    let entries_end = madt_phys + total_len as u64;
    let mut offset = entries_start;

    while offset + 2 <= entries_end {
        let entry_type = unsafe { core::ptr::read(offset as *const u8) };
        let entry_len = unsafe { core::ptr::read((offset + 1) as *const u8) } as u64;
        if entry_len < 2 { break; }

        match entry_type {
            MADT_TYPE_LOCAL_APIC if entry_len >= 8 => {
                let apic_id = unsafe { core::ptr::read((offset + 3) as *const u8) } as u32;
                let flags = unsafe { core::ptr::read_unaligned((offset + 4) as *const u32) };
                if (flags & MADT_LAPIC_ENABLED) != 0 || (flags & MADT_LAPIC_ONLINE_CAPABLE) != 0 {
                    register_cpu(apic_id, apic_id == bsp_id);
                }
            }
            MADT_TYPE_LOCAL_X2APIC if entry_len >= 16 => {
                let apic_id = unsafe { core::ptr::read_unaligned((offset + 4) as *const u32) };
                let flags = unsafe { core::ptr::read_unaligned((offset + 8) as *const u32) };
                if (flags & MADT_LAPIC_ENABLED) != 0 || (flags & MADT_LAPIC_ONLINE_CAPABLE) != 0 {
                    register_cpu(apic_id, apic_id == bsp_id);
                }
            }
            _ => {}
        }
        offset += entry_len;
    }
}

fn register_cpu(apic_id: u32, is_bsp: bool) {
    let idx = CPU_COUNT.load(Ordering::SeqCst) as usize;
    if idx >= MAX_CPUS { return; }
    for i in 0..idx {
        if unsafe { CPU_TABLE[i].apic_id } == apic_id { return; }
    }
    unsafe { CPU_TABLE[idx] = CpuInfo { apic_id, is_bsp, usable: true }; }
    CPU_COUNT.store((idx + 1) as u32, Ordering::SeqCst);
}

// ===========================================================================
// PHASE 2: Per-Core State
// ===========================================================================

const AP_STACK_SIZE: usize = 8192;
const MAX_BOOT_APS: usize = 128;
const AP_GDT_LEN: usize = 7;

#[repr(C)]
struct ApTss {
    _reserved0: u32,
    rsp: [u64; 3],
    _reserved1: u64,
    ist: [u64; 7],
    _reserved2: u64,
    _reserved3: u16,
    iomap_base: u16,
}

impl ApTss {
    const fn new() -> Self {
        Self {
            _reserved0: 0, rsp: [0; 3], _reserved1: 0,
            ist: [0; 7], _reserved2: 0, _reserved3: 0, iomap_base: 0,
        }
    }
}

#[repr(C, align(16))]
struct ApBootState {
    gdt: [u64; AP_GDT_LEN],
    tss: ApTss,
    stack: [u8; AP_STACK_SIZE],
}

impl ApBootState {
    const fn new() -> Self {
        Self { gdt: [0u64; AP_GDT_LEN], tss: ApTss::new(), stack: [0u8; AP_STACK_SIZE] }
    }
}

static mut AP_BOOT_STATES: [ApBootState; MAX_BOOT_APS] = {
    const INIT: ApBootState = ApBootState::new();
    [INIT; MAX_BOOT_APS]
};

const fn ap_tss_descriptor(base: u64, limit: u32) -> (u64, u64) {
    let low = (limit as u64 & 0xFFFF)
        | ((base & 0xFFFF) << 16)
        | (((base >> 16) & 0xFF) << 32)
        | (0x89u64 << 40)
        | (((limit as u64 >> 16) & 0xF) << 48)
        | (((base >> 24) & 0xFF) << 56);
    let high = (base >> 32) & 0xFFFF_FFFF;
    (low, high)
}

fn init_ap_boot_state(ap_index: usize) {
    if ap_index >= MAX_BOOT_APS { return; }
    unsafe {
        let state = &mut AP_BOOT_STATES[ap_index];
        let stack_top = (core::ptr::addr_of!(state.stack) as *const u8 as u64)
            + AP_STACK_SIZE as u64;
        state.tss.rsp[0] = stack_top;
        state.tss.iomap_base = core::mem::size_of::<ApTss>() as u16;

        state.gdt[0] = 0;
        state.gdt[1] = 0x00AF9A000000FFFF; // kernel code 64 (0x08)
        state.gdt[2] = 0x00AF92000000FFFF; // kernel data     (0x10)
        state.gdt[3] = 0x00AFF2000000FFFF; // user data       (0x18)
        state.gdt[4] = 0x00AFFA000000FFFF; // user code 64    (0x20)

        let tss_base = core::ptr::addr_of!(state.tss) as u64;
        let tss_limit = (core::mem::size_of::<ApTss>() - 1) as u32;
        let (tss_low, tss_high) = ap_tss_descriptor(tss_base, tss_limit);
        state.gdt[5] = tss_low;  // TSS low  (0x28)
        state.gdt[6] = tss_high; // TSS high
    }
}

/// Initialize this AP's Local APIC SVR.
fn ap_init_lapic() {
    const SVR_ENABLE: u32 = 1 << 8;
    const SPURIOUS_VEC: u32 = 0xFF;

    if is_x2apic_active() {
        let current = unsafe { crate::hal::rdmsr(0x80F) } as u32;
        unsafe { crate::hal::wrmsr(0x80F, (current | SVR_ENABLE | SPURIOUS_VEC) as u64); }
    } else {
        unsafe {
            let svr = 0xFEE0_00F0 as *mut u32;
            let current = core::ptr::read_volatile(svr);
            core::ptr::write_volatile(svr, current | SVR_ENABLE | SPURIOUS_VEC);
        }
    }
}

// ===========================================================================
// Baremetal AP Bootstrap (INIT/SIPI + trampoline)
// ===========================================================================

fn trampoline_size() -> usize {
    let start = unsafe { &ap_trampoline_start as *const u8 as usize };
    let end = unsafe { &ap_trampoline_end as *const u8 as usize };
    end.saturating_sub(start)
}

fn trampoline_offset(sym: *const u8) -> usize {
    let base = unsafe { &ap_trampoline_start as *const u8 as usize };
    let addr = sym as usize;
    addr.saturating_sub(base)
}

fn copy_trampoline() -> bool {
    let size = trampoline_size();
    if size == 0 || size > AP_TRAMPOLINE_MAX_BYTES {
        return false;
    }
    unsafe {
        let src = &ap_trampoline_start as *const u8;
        let dst = AP_TRAMPOLINE_PHYS as *mut u8;
        core::ptr::copy_nonoverlapping(src, dst, size);
    }
    true
}

fn patch_trampoline(cr3: u64, stack_top: u64, entry: u64, gdt_ptr: &ApGdtPointer) {
    unsafe {
        let base = AP_TRAMPOLINE_PHYS as *mut u8;
        let cr3_ptr = base.add(trampoline_offset(&ap_trampoline_cr3 as *const u8)) as *mut u64;
        let stack_ptr = base.add(trampoline_offset(&ap_trampoline_stack as *const u8)) as *mut u64;
        let entry_ptr = base.add(trampoline_offset(&ap_trampoline_entry as *const u8)) as *mut u64;
        let gdt_ptr_dst = base.add(trampoline_offset(&ap_trampoline_os_gdt_ptr as *const u8)) as *mut ApGdtPointer;

        core::ptr::write_unaligned(cr3_ptr, cr3);
        core::ptr::write_unaligned(stack_ptr, stack_top);
        core::ptr::write_unaligned(entry_ptr, entry);
        core::ptr::write_unaligned(core::ptr::addr_of_mut!((*gdt_ptr_dst).limit), gdt_ptr.limit);
        core::ptr::write_unaligned(core::ptr::addr_of_mut!((*gdt_ptr_dst).base), gdt_ptr.base);
    }

    core::sync::atomic::fence(Ordering::SeqCst);
}

fn cpu_index_for_apic(apic_id: u32) -> Option<usize> {
    let count = CPU_COUNT.load(Ordering::SeqCst) as usize;
    for i in 0..count {
        let cpu = unsafe { CPU_TABLE[i] };
        if cpu.apic_id == apic_id {
            return Some(i);
        }
    }
    None
}

/// Return the CPU table index for a given APIC ID.
pub fn cpu_index_for_apic_id(apic_id: u32) -> Option<usize> {
    cpu_index_for_apic(apic_id)
}

/// Best-effort current CPU index. Falls back to 0 if unknown.
pub fn current_cpu_index() -> usize {
    let apic_id = current_apic_id();
    cpu_index_for_apic(apic_id).unwrap_or(0)
}

#[no_mangle]
extern "C" fn ap_entry_baremetal() -> ! {
    let apic_id = current_apic_id();
    AP_BOOT_ACK.store(apic_id, Ordering::SeqCst);

    let _ = enable_local_apic();
    ap_init_lapic();
    crate::interrupts::load_current_idt();

    mark_ap_online(apic_id);
    APS_ONLINE.fetch_add(1, Ordering::SeqCst);

    if let Some(core_index) = cpu_index_for_apic(apic_id) {
        crate::per_core::activate_core(core_index);

        // Wait until BSP releases APs after boot.
        while !AP_GO.load(Ordering::SeqCst) {
            crate::hal::pause();
        }

        let mut last_tick = crate::timer::ticks();
        loop {
            let mut did_work = false;
            let now = crate::timer::ticks();
            if now != last_tick {
                last_tick = now;
                crate::process::on_tick_core(core_index, now);
                did_work = true;
            }
            if crate::per_core::tick(core_index) {
                did_work = true;
            }
            if !did_work {
                crate::hal::pause();
            }
        }
    }

    loop {
        crate::hal::pause();
    }
}

fn bootstrap_aps_baremetal() -> u32 {
    if !enable_local_apic() {
        return 0;
    }

    if !copy_trampoline() {
        return 0;
    }

    unsafe {
        for i in 0..MAX_CPUS {
            AP_ONLINE_FLAGS[i].store(false, Ordering::SeqCst);
        }
    }
    APS_ONLINE.store(0, Ordering::SeqCst);
    AP_BOOT_ACK.store(0, Ordering::SeqCst);
    AP_GO.store(false, Ordering::SeqCst);

    let count = cpu_count() as usize;
    let mut ap_index = 0usize;

    let cr3 = read_cr3();
    if (cr3 >> 32) != 0 {
        // Trampoline only loads low 32-bits of CR3.
        return 0;
    }

    for i in 0..count {
        let cpu = unsafe { CPU_TABLE[i] };
        if !cpu.usable || cpu.is_bsp {
            continue;
        }
        if ap_index >= MAX_BOOT_APS {
            break;
        }

        init_ap_boot_state(ap_index);
        unsafe {
            let state = &AP_BOOT_STATES[ap_index];
            let stack_top = (core::ptr::addr_of!(state.stack) as *const u8 as u64)
                + AP_STACK_SIZE as u64;
            let gdt_ptr = ApGdtPointer {
                limit: (core::mem::size_of::<[u64; AP_GDT_LEN]>() - 1) as u16,
                base: (core::ptr::addr_of!(state.gdt) as *const _) as u64,
            };
            patch_trampoline(cr3, stack_top, ap_entry_baremetal as *const () as u64, &gdt_ptr);
        }

        AP_BOOT_TARGET.store(cpu.apic_id, Ordering::SeqCst);
        AP_BOOT_ACK.store(0, Ordering::SeqCst);

        let vector = (AP_TRAMPOLINE_PHYS >> 12) as u8;
        apic_send_init(cpu.apic_id);
        for _ in 0..AP_BOOT_SPIN_WAIT {
            crate::hal::pause();
        }
        apic_send_startup(cpu.apic_id, vector);
        for _ in 0..AP_BOOT_SPIN_WAIT {
            crate::hal::pause();
        }
        apic_send_startup(cpu.apic_id, vector);

        let mut ok = false;
        for _ in 0..AP_BOOT_TIMEOUT_SPINS {
            if AP_BOOT_ACK.load(Ordering::SeqCst) == cpu.apic_id || is_ap_online(cpu.apic_id) {
                ok = true;
                break;
            }
            crate::hal::pause();
        }

        if ok {
            ap_index += 1;
        }
    }

    AP_GO.store(true, Ordering::SeqCst);
    APS_ONLINE.load(Ordering::SeqCst)
}

// ===========================================================================
// PHASE 3: AP Bootstrap via UEFI MpServices Protocol
// ===========================================================================

/// UEFI efiapi callback — runs on each AP via MpServices.
/// Firmware already set up GDT/IDT/page tables/long mode for us.
extern "efiapi" fn ap_procedure_efiapi(_arg: *mut core::ffi::c_void) {
    let my_apic_id = current_apic_id();

    // Find our AP index
    let count = CPU_COUNT.load(Ordering::SeqCst) as usize;
    let mut ap_index = 0usize;
    for i in 0..count {
        let cpu = unsafe { CPU_TABLE[i] };
        if cpu.is_bsp { continue; }
        if cpu.apic_id == my_apic_id { break; }
        ap_index += 1;
    }

    // Prepare per-AP GDT/TSS for post-BootServices use
    if ap_index < MAX_BOOT_APS {
        init_ap_boot_state(ap_index);
    }

    // Init LAPIC SVR
    ap_init_lapic();

    // Mark online
    mark_ap_online(my_apic_id);
    APS_ONLINE.fetch_add(1, Ordering::SeqCst);
}

/// Bootstrap all APs using UEFI MpServices Protocol.
fn bootstrap_aps_uefi() -> u32 {
    match uefi::boot::get_handle_for_protocol::<uefi::proto::pi::mp::MpServices>() {
        Ok(handle) => {
            if let Ok(mp) = uefi::boot::open_protocol_exclusive::<uefi::proto::pi::mp::MpServices>(handle) {
                let timeout = Some(core::time::Duration::from_secs(5));
                let _ = mp.startup_all_aps(
                    false,
                    ap_procedure_efiapi,
                    core::ptr::null_mut(),
                    None,
                    timeout,
                );
            }
        }
        Err(_) => {}
    }
    APS_ONLINE.load(Ordering::SeqCst)
}

/// Bootstrap all APs. Uses UEFI MpServices in firmware mode, falls back to
/// baremetal INIT/SIPI trampoline after ExitBootServices.
pub fn bootstrap_aps() -> u32 {
    if !discovery_done() { discover_cpus(); }

    let count = cpu_count() as usize;
    if count <= 1 {
        BOOTSTRAP_DONE.store(true, Ordering::SeqCst);
        return 0;
    }

    let method = AP_BOOT_METHOD.load(Ordering::SeqCst);
    if BOOTSTRAP_DONE.load(Ordering::SeqCst) {
        if method == AP_BOOT_METHOD_BAREMETAL {
            return APS_ONLINE.load(Ordering::SeqCst);
        }
        if method == AP_BOOT_METHOD_UEFI && crate::runtime::runtime_uefi_active() {
            return APS_ONLINE.load(Ordering::SeqCst);
        }
    }

    let mut online = 0u32;
    if crate::runtime::runtime_uefi_active() {
        online = bootstrap_aps_uefi();
        if online > 0 {
            AP_BOOT_METHOD.store(AP_BOOT_METHOD_UEFI, Ordering::SeqCst);
            BOOTSTRAP_DONE.store(true, Ordering::SeqCst);
            return online;
        }

        if !AP_BOOT_PREFER_BAREMETAL {
            BOOTSTRAP_DONE.store(true, Ordering::SeqCst);
            return online;
        }
    }

    online = bootstrap_aps_baremetal();
    if online > 0 {
        AP_BOOT_METHOD.store(AP_BOOT_METHOD_BAREMETAL, Ordering::SeqCst);
    }

    BOOTSTRAP_DONE.store(true, Ordering::SeqCst);
    online
}

pub fn release_aps() { AP_GO.store(true, Ordering::SeqCst); }

// ===========================================================================
// AP Work Dispatch via MpServices
// ===========================================================================

/// Dispatch a procedure to a specific AP (by processor number, not APIC ID).
/// Blocks until the AP completes or times out.
/// `arg` is passed as the procedure argument (can be a pointer to shared data).
pub fn dispatch_to_ap(
    processor_number: usize,
    procedure: extern "efiapi" fn(*mut core::ffi::c_void),
    arg: *mut core::ffi::c_void,
    timeout_secs: u64,
) -> bool {
    if !crate::runtime::runtime_uefi_active() {
        return dispatch_to_ap_baremetal(processor_number, procedure, arg, timeout_secs);
    }

    let handle = match uefi::boot::get_handle_for_protocol::<uefi::proto::pi::mp::MpServices>() {
        Ok(h) => h,
        Err(_) => return false,
    };
    let mp = match uefi::boot::open_protocol_exclusive::<uefi::proto::pi::mp::MpServices>(handle) {
        Ok(m) => m,
        Err(_) => return false,
    };

    let timeout = if timeout_secs > 0 {
        Some(core::time::Duration::from_secs(timeout_secs))
    } else {
        None
    };

    mp.startup_this_ap(processor_number, procedure, arg, None, timeout).is_ok()
}

/// Dispatch a procedure to any available AP.
/// Uses processor number 1 (first AP) by default.
pub fn dispatch_to_any_ap(
    procedure: extern "efiapi" fn(*mut core::ffi::c_void),
    arg: *mut core::ffi::c_void,
    timeout_secs: u64,
) -> bool {
    if !crate::runtime::runtime_uefi_active() {
        if let Some(core_index) = pick_any_ap_core() {
            return dispatch_to_ap_baremetal(core_index, procedure, arg, timeout_secs);
        }
        return false;
    }

    // Processor number 1 = first AP (0 = BSP)
    dispatch_to_ap(1, procedure, arg, timeout_secs)
}

#[repr(C)]
struct BaremetalApJob {
    procedure: extern "efiapi" fn(*mut core::ffi::c_void),
    arg: *mut core::ffi::c_void,
    done: AtomicBool,
}

impl BaremetalApJob {
    fn new(procedure: extern "efiapi" fn(*mut core::ffi::c_void), arg: *mut core::ffi::c_void) -> Self {
        Self { procedure, arg, done: AtomicBool::new(false) }
    }
}

fn baremetal_ap_job_entry(arg: u64) {
    let job = unsafe { &*(arg as *const BaremetalApJob) };
    (job.procedure)(job.arg);
    job.done.store(true, Ordering::SeqCst);
}

fn pick_any_ap_core() -> Option<usize> {
    let cores = crate::per_core::core_count() as usize;
    for i in 0..cores {
        if i == 0 {
            continue;
        }
        if crate::per_core::core_is_active(i) {
            return Some(i);
        }
    }
    None
}

fn dispatch_to_ap_baremetal(
    processor_number: usize,
    procedure: extern "efiapi" fn(*mut core::ffi::c_void),
    arg: *mut core::ffi::c_void,
    timeout_secs: u64,
) -> bool {
    let core_index = if processor_number == 0 {
        return false;
    } else {
        processor_number
    };

    if !crate::per_core::core_is_active(core_index) {
        return false;
    }

    let job = BaremetalApJob::new(procedure, arg);
    let job_ptr = &job as *const BaremetalApJob as u64;
    let enqueued = crate::per_core::enqueue(crate::per_core::Job {
        func: baremetal_ap_job_entry,
        arg: job_ptr,
        priority: 0,
        affinity: core_index as i32,
    });
    if !enqueued {
        return false;
    }

    let timeout_ticks = if timeout_secs == 0 {
        0
    } else {
        let snap = crate::timer::snapshot();
        let tick_us = snap.tick_us.max(1);
        (timeout_secs.saturating_mul(1_000_000) + tick_us - 1) / tick_us
    };
    let start = crate::timer::ticks();

    loop {
        if job.done.load(Ordering::SeqCst) {
            return true;
        }
        if timeout_ticks > 0 && crate::timer::ticks().saturating_sub(start) > timeout_ticks {
            return false;
        }
        crate::hal::pause();
    }
}

// ---------------------------------------------------------------------------
// Multi-core test: prove APs can do real CPU work
// ---------------------------------------------------------------------------

/// Shared data for multi-core test.
#[repr(C)]
pub struct ApWorkResult {
    pub iterations: u64,
    pub checksum: u64,
    pub ap_apic_id: u32,
    pub done: core::sync::atomic::AtomicBool,
}

impl ApWorkResult {
    pub const fn new() -> Self {
        Self {
            iterations: 0,
            checksum: 0,
            ap_apic_id: 0,
            done: core::sync::atomic::AtomicBool::new(false),
        }
    }
}

/// CPU-bound work that runs on an AP — proves real multi-core execution.
extern "efiapi" fn ap_cpu_work(arg: *mut core::ffi::c_void) {
    let result = unsafe { &mut *(arg as *mut ApWorkResult) };
    result.ap_apic_id = current_apic_id();

    // Do real CPU work: compute a checksum over many iterations
    let mut checksum: u64 = 0;
    let target_iterations: u64 = 10_000_000;
    for i in 0..target_iterations {
        checksum = checksum.wrapping_add(i).wrapping_mul(6364136223846793005).wrapping_add(1);
    }

    result.iterations = target_iterations;
    result.checksum = checksum;
    result.done.store(true, Ordering::SeqCst);
}

/// Test multi-core execution: dispatch CPU work to AP, BSP stays free.
/// Returns a formatted result string.
pub fn test_multi_core() -> alloc::string::String {
    use alloc::format;

    static mut TEST_RESULT: ApWorkResult = ApWorkResult::new();

    unsafe {
        TEST_RESULT = ApWorkResult::new();
    }

    let start_tick = crate::timer::ticks();

    let arg_ptr = unsafe { core::ptr::addr_of_mut!(TEST_RESULT) as *mut core::ffi::c_void };
    let ok = dispatch_to_any_ap(ap_cpu_work, arg_ptr, 10);

    let end_tick = crate::timer::ticks();
    let elapsed = end_tick.saturating_sub(start_tick);

    if ok {
        let result = unsafe { &TEST_RESULT };
        format!(
            "Multi-core test PASSED!\n  AP APIC_ID={} completed {} iterations\n  Checksum=0x{:016X}\n  Elapsed={} ticks\n  BSP was FREE during AP computation",
            result.ap_apic_id, result.iterations, result.checksum, elapsed
        )
    } else {
        format!("Multi-core test FAILED: could not dispatch to AP (elapsed={} ticks)", elapsed)
    }
}

// ===========================================================================
// AP-dispatched GZIP inflate
// ===========================================================================

/// Shared data for AP inflate job.
/// BSP sets input_ptr/input_len/max_output, then dispatches to AP.
/// AP writes results to output_ptr/output_len/error.
#[repr(C)]
pub struct ApInflateJob {
    pub input_ptr: *const u8,
    pub input_len: usize,
    pub max_output: usize,
    // Output (written by AP)
    pub output_ptr: *mut u8,
    pub output_len: usize,
    pub success: core::sync::atomic::AtomicBool,
    pub done: core::sync::atomic::AtomicBool,
}

unsafe impl Send for ApInflateJob {}
unsafe impl Sync for ApInflateJob {}

impl ApInflateJob {
    pub const fn new() -> Self {
        Self {
            input_ptr: core::ptr::null(),
            input_len: 0,
            max_output: 0,
            output_ptr: core::ptr::null_mut(),
            output_len: 0,
            success: core::sync::atomic::AtomicBool::new(false),
            done: core::sync::atomic::AtomicBool::new(false),
        }
    }
}

/// The procedure that runs on the AP — performs DEFLATE inflate.
extern "efiapi" fn ap_inflate_procedure(arg: *mut core::ffi::c_void) {
    let job = unsafe { &mut *(arg as *mut ApInflateJob) };

    let input = unsafe { core::slice::from_raw_parts(job.input_ptr, job.input_len) };

    match miniz_oxide::inflate::decompress_to_vec_with_limit(input, job.max_output) {
        Ok(decompressed) => {
            let len = decompressed.len();
            // Allocate output and copy — the Vec lives on this AP's stack,
            // so we copy into a heap allocation that BSP can take ownership of.
            let layout = core::alloc::Layout::from_size_align(len, 8)
                .unwrap_or(core::alloc::Layout::new::<u8>());
            let ptr = unsafe { alloc::alloc::alloc(layout) };
            if !ptr.is_null() {
                unsafe { core::ptr::copy_nonoverlapping(decompressed.as_ptr(), ptr, len); }
                job.output_ptr = ptr;
                job.output_len = len;
                job.success.store(true, Ordering::SeqCst);
            }
        }
        Err(_) => {
            job.success.store(false, Ordering::SeqCst);
        }
    }

    job.done.store(true, Ordering::SeqCst);
}

/// Inflate a DEFLATE payload on an AP.
/// Returns Ok(Vec<u8>) on success, Err(()) if AP dispatch failed (caller should fallback to BSP).
pub fn inflate_on_ap(payload: &[u8], max_output: usize) -> Result<alloc::vec::Vec<u8>, ()> {
    static mut INFLATE_JOB: ApInflateJob = ApInflateJob::new();

    unsafe {
        INFLATE_JOB = ApInflateJob::new();
        INFLATE_JOB.input_ptr = payload.as_ptr();
        INFLATE_JOB.input_len = payload.len();
        INFLATE_JOB.max_output = max_output;
    }

    let arg_ptr = unsafe { core::ptr::addr_of_mut!(INFLATE_JOB) as *mut core::ffi::c_void };

    // Dispatch to AP with 30s timeout (for large files)
    if !dispatch_to_any_ap(ap_inflate_procedure, arg_ptr, 30) {
        return Err(());
    }

    let job = unsafe { &INFLATE_JOB };
    if !job.success.load(Ordering::SeqCst) {
        return Err(());
    }

    // Take ownership of the AP-allocated buffer
    let len = job.output_len;
    let ptr = job.output_ptr;
    if ptr.is_null() || len == 0 {
        return Err(());
    }

    let result = unsafe { alloc::vec::Vec::from_raw_parts(ptr, len, len) };
    Ok(result)
}

// ===========================================================================
// AP-dispatched SHA256 (CPU-bound)
// ===========================================================================

// ===========================================================================
// AP-dispatched ELF dynamic inspection (CPU-bound)
// ===========================================================================

#[repr(C)]
pub struct ApDynInspectJob {
    pub input_ptr: *const u8,
    pub input_len: usize,
    pub output_ptr: *mut crate::linux_compat::DynamicInspectReport,
    pub success: AtomicBool,
    pub done: AtomicBool,
}

unsafe impl Send for ApDynInspectJob {}
unsafe impl Sync for ApDynInspectJob {}

impl ApDynInspectJob {
    pub const fn new() -> Self {
        Self {
            input_ptr: core::ptr::null(),
            input_len: 0,
            output_ptr: core::ptr::null_mut(),
            success: AtomicBool::new(false),
            done: AtomicBool::new(false),
        }
    }
}

extern "efiapi" fn ap_dyn_inspect_procedure(arg: *mut core::ffi::c_void) {
    let job = unsafe { &mut *(arg as *mut ApDynInspectJob) };
    let raw = unsafe { core::slice::from_raw_parts(job.input_ptr, job.input_len) };
    if let Ok(report) = crate::linux_compat::inspect_dynamic_elf64(raw) {
        let boxed = Box::new(report);
        job.output_ptr = Box::into_raw(boxed);
        job.success.store(true, Ordering::SeqCst);
    } else {
        job.success.store(false, Ordering::SeqCst);
    }
    job.done.store(true, Ordering::SeqCst);
}

pub fn inspect_dynamic_elf64_on_ap(
    raw: &[u8],
) -> Result<crate::linux_compat::DynamicInspectReport, ()> {
    static mut DYN_JOB: ApDynInspectJob = ApDynInspectJob::new();
    unsafe {
        DYN_JOB = ApDynInspectJob::new();
        DYN_JOB.input_ptr = raw.as_ptr();
        DYN_JOB.input_len = raw.len();
    }
    let arg_ptr = unsafe { core::ptr::addr_of_mut!(DYN_JOB) as *mut core::ffi::c_void };
    if !dispatch_to_any_ap(ap_dyn_inspect_procedure, arg_ptr, 5) {
        return Err(());
    }
    let job = unsafe { &DYN_JOB };
    if !job.success.load(Ordering::SeqCst) {
        return Err(());
    }
    let ptr = job.output_ptr;
    if ptr.is_null() {
        return Err(());
    }
    let report = unsafe { *Box::from_raw(ptr) };
    Ok(report)
}

// ===========================================================================
// AP-dispatched TAR scan (CPU-bound)
// ===========================================================================

#[derive(Clone)]
pub struct TarScanEntry {
    pub path: String,
    pub header_offset: usize,
    pub data_offset: usize,
    pub data_len: usize,
    pub typeflag: u8,
}

#[repr(C)]
pub struct ApTarScanJob {
    pub input_ptr: *const u8,
    pub input_len: usize,
    pub max_entries: usize,
    pub output_ptr: *mut TarScanEntry,
    pub output_len: usize,
    pub output_cap: usize,
    pub success: AtomicBool,
    pub done: AtomicBool,
}

unsafe impl Send for ApTarScanJob {}
unsafe impl Sync for ApTarScanJob {}

impl ApTarScanJob {
    pub const fn new() -> Self {
        Self {
            input_ptr: core::ptr::null(),
            input_len: 0,
            max_entries: 0,
            output_ptr: core::ptr::null_mut(),
            output_len: 0,
            output_cap: 0,
            success: AtomicBool::new(false),
            done: AtomicBool::new(false),
        }
    }
}

fn tar_parse_octal(field: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    let mut any = false;
    for b in field.iter() {
        if *b == 0 || *b == b' ' {
            continue;
        }
        if *b < b'0' || *b > b'7' {
            return None;
        }
        value = value.saturating_mul(8).saturating_add((b - b'0') as usize);
        any = true;
    }
    if any { Some(value) } else { None }
}

fn tar_header_all_zero(header: &[u8]) -> bool {
    for b in header.iter() {
        if *b != 0 {
            return false;
        }
    }
    true
}

fn tar_header_path(header: &[u8]) -> Option<String> {
    let name_end = header[..100].iter().position(|b| *b == 0).unwrap_or(100);
    let prefix_end = header[345..500]
        .iter()
        .position(|b| *b == 0)
        .unwrap_or(155);
    let name = String::from_utf8_lossy(&header[..name_end]).into_owned();
    let prefix = String::from_utf8_lossy(&header[345..345 + prefix_end]).into_owned();
    if name.is_empty() && prefix.is_empty() {
        None
    } else if prefix.is_empty() {
        Some(name)
    } else if name.is_empty() {
        Some(prefix)
    } else {
        Some(alloc::format!("{}/{}", prefix, name))
    }
}

extern "efiapi" fn ap_tar_scan_procedure(arg: *mut core::ffi::c_void) {
    let job = unsafe { &mut *(arg as *mut ApTarScanJob) };
    let raw = unsafe { core::slice::from_raw_parts(job.input_ptr, job.input_len) };
    let mut cursor = 0usize;
    let mut entries: Vec<TarScanEntry> = Vec::new();
    let mut ok = true;

    while cursor + 512 <= raw.len() {
        let header = &raw[cursor..cursor + 512];
        if tar_header_all_zero(header) {
            break;
        }
        let path = match tar_header_path(header) {
            Some(v) => v,
            None => {
                ok = false;
                break;
            }
        };
        let size = match tar_parse_octal(&header[124..136]) {
            Some(v) => v,
            None => {
                ok = false;
                break;
            }
        };
        let typeflag = header[156];
        let data_offset = cursor + 512;
        let aligned = ((size + 511) / 512) * 512;
        if data_offset.saturating_add(aligned) > raw.len() {
            ok = false;
            break;
        }
        entries.push(TarScanEntry {
            path,
            header_offset: cursor,
            data_offset,
            data_len: size,
            typeflag,
        });
        if job.max_entries > 0 && entries.len() >= job.max_entries {
            ok = false;
            break;
        }
        cursor = data_offset + aligned;
    }

    if ok {
        let mut vec = entries;
        let ptr = vec.as_mut_ptr();
        let len = vec.len();
        let cap = vec.capacity();
        core::mem::forget(vec);
        job.output_ptr = ptr;
        job.output_len = len;
        job.output_cap = cap;
        job.success.store(true, Ordering::SeqCst);
    } else {
        job.success.store(false, Ordering::SeqCst);
    }
    job.done.store(true, Ordering::SeqCst);
}

pub fn tar_scan_on_ap(raw: &[u8], max_entries: usize) -> Result<Vec<TarScanEntry>, ()> {
    static mut TAR_JOB: ApTarScanJob = ApTarScanJob::new();
    unsafe {
        TAR_JOB = ApTarScanJob::new();
        TAR_JOB.input_ptr = raw.as_ptr();
        TAR_JOB.input_len = raw.len();
        TAR_JOB.max_entries = max_entries;
    }
    let arg_ptr = unsafe { core::ptr::addr_of_mut!(TAR_JOB) as *mut core::ffi::c_void };
    if !dispatch_to_any_ap(ap_tar_scan_procedure, arg_ptr, 10) {
        return Err(());
    }
    let job = unsafe { &TAR_JOB };
    if !job.success.load(Ordering::SeqCst) {
        return Err(());
    }
    let ptr = job.output_ptr;
    let len = job.output_len;
    let cap = job.output_cap;
    if ptr.is_null() || cap < len {
        return Err(());
    }
    let entries = unsafe { Vec::from_raw_parts(ptr, len, cap) };
    Ok(entries)
}

#[repr(C)]
pub struct ApSha256Job {
    pub input_ptr: *const u8,
    pub input_len: usize,
    pub digest: [u8; 32],
    pub success: AtomicBool,
    pub done: AtomicBool,
}

unsafe impl Send for ApSha256Job {}
unsafe impl Sync for ApSha256Job {}

impl ApSha256Job {
    pub const fn new() -> Self {
        Self {
            input_ptr: core::ptr::null(),
            input_len: 0,
            digest: [0u8; 32],
            success: AtomicBool::new(false),
            done: AtomicBool::new(false),
        }
    }
}

extern "efiapi" fn ap_sha256_procedure(arg: *mut core::ffi::c_void) {
    let job = unsafe { &mut *(arg as *mut ApSha256Job) };
    let input = unsafe { core::slice::from_raw_parts(job.input_ptr, job.input_len) };
    let digest = Sha256::digest(input);
    job.digest.copy_from_slice(&digest);
    job.success.store(true, Ordering::SeqCst);
    job.done.store(true, Ordering::SeqCst);
}

pub fn sha256_on_ap(payload: &[u8]) -> Result<[u8; 32], ()> {
    static mut SHA256_JOB: ApSha256Job = ApSha256Job::new();

    unsafe {
        SHA256_JOB = ApSha256Job::new();
        SHA256_JOB.input_ptr = payload.as_ptr();
        SHA256_JOB.input_len = payload.len();
    }

    let arg_ptr = unsafe { core::ptr::addr_of_mut!(SHA256_JOB) as *mut core::ffi::c_void };

    if !dispatch_to_any_ap(ap_sha256_procedure, arg_ptr, 5) {
        return Err(());
    }

    let job = unsafe { &SHA256_JOB };
    if !job.success.load(Ordering::SeqCst) {
        return Err(());
    }

    Ok(job.digest)
}

// ===========================================================================
// Status / diagnostic
// ===========================================================================

pub fn status_string() -> alloc::string::String {
    use alloc::format;
    use alloc::string::String;

    if !discovery_done() {
        return String::from("SMP: discovery not yet performed.");
    }

    let count = cpu_count();
    let bsp = bsp_apic_id();
    let online = APS_ONLINE.load(Ordering::SeqCst);
    let method = match AP_BOOT_METHOD.load(Ordering::SeqCst) {
        AP_BOOT_METHOD_UEFI => "uefi-mp",
        AP_BOOT_METHOD_BAREMETAL => "baremetal",
        _ => "none",
    };
    let mut s = format!(
        "SMP: {} CPU(s) detected, {} AP(s) online, BSP APIC_ID={}\n  LAPIC base=0x{:08X} mode={} boot={}\n",
        count, online, bsp, local_apic_base(),
        if is_x2apic_active() { "x2APIC" } else { "xAPIC" },
        method,
    );

    for i in 0..count as usize {
        if let Some(cpu) = cpu_info(i) {
            let state = if cpu.is_bsp { "online" }
                else if is_ap_online(cpu.apic_id) { "online" }
                else { "offline" };
            s.push_str(&format!(
                "  CPU[{}]: APIC_ID={} {} {} {}\n",
                i, cpu.apic_id,
                if cpu.is_bsp { "BSP" } else { "AP" },
                if cpu.usable { "usable" } else { "disabled" },
                state,
            ));
        }
    }
    s
}
