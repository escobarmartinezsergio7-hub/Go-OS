use core::arch::asm;
use crate::memory::{alloc_frame, PAGE_SIZE};

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    pub const fn empty() -> Self { Self(0) }
    
    pub fn is_present(&self) -> bool { (self.0 & 1) != 0 }
    
    pub fn set_present(&mut self, present: bool) {
        if present { self.0 |= 1; } else { self.0 &= !1; }
    }
    
    pub fn set_writable(&mut self, writable: bool) {
        if writable { self.0 |= 1 << 1; } else { self.0 &= !(1 << 1); }
    }
    
    pub fn set_user(&mut self, user: bool) {
        if user { self.0 |= 1 << 2; } else { self.0 &= !(1 << 2); }
    }
    
    pub fn addr(&self) -> u64 {
        self.0 & 0x000FFFFFFFFFF000
    }
    
    pub fn set_addr(&mut self, addr: u64) {
        self.0 = (self.0 & 0xFFF0000000000FFF) | (addr & 0x000FFFFFFFFFF000);
    }
    
    pub fn set_flags(&mut self, flags: u64) {
        self.0 = (self.0 & 0xFFFFFFFFFFFFF000) | (flags & 0xFFF);
    }
    
    pub fn raw(&self) -> u64 {
        self.0
    }
    
    pub fn set_raw(&mut self, raw: u64) {
        self.0 = raw;
    }
}

#[repr(C, align(4096))]
pub struct PageTable {
    pub entries: [PageTableEntry; 512],
}

impl PageTable {
    pub const fn empty() -> Self {
        Self { entries: [PageTableEntry::empty(); 512] }
    }
}

pub static mut KERNEL_CR3: u64 = 0;

pub fn init() {
    unsafe {
        KERNEL_CR3 = get_current_cr3();
    }
}

pub fn get_current_cr3() -> u64 {
    let mut value: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

pub unsafe fn set_cr3(cr3: u64) {
    asm!("mov cr3, {}", in(reg) cr3, options(nostack, preserves_flags));
}

pub fn switch_to_process_cr3(cr3: Option<u64>) {
    let target_cr3 = cr3.unwrap_or(unsafe { KERNEL_CR3 });
    if get_current_cr3() != target_cr3 {
        unsafe { set_cr3(target_cr3); }
    }
}

/// Crea un nuevo PML4 base copiando el PML4 actual (asumiendo que es el del Kernel/UEFI)
/// y forzando que todas esas entradas sean de Supervisor (aislando el kernel de Ring 3).
pub fn create_process_pml4() -> Option<u64> {
    let pml4_phys = alloc_frame()?;
    let pml4 = unsafe { &mut *(pml4_phys as *mut PageTable) };
    
    let current_cr3 = get_current_cr3() & 0x000FFFFFFFFFF000;
    let current_pml4 = unsafe { &*(current_cr3 as *const PageTable) };
    
    for i in 0..512 {
        let mut entry = current_pml4.entries[i];
        if entry.is_present() {
            // Forzar que las entradas heredadas del kernel sean de Supervisor (U=0)
            // para que Ring 3 no pueda leer la memoria identidad del sistema.
            entry.set_user(false);
        }
        pml4.entries[i] = entry;
    }
    
    Some(pml4_phys)
}

fn get_or_alloc_table(parent_entry: &mut PageTableEntry) -> Option<&mut PageTable> {
    if !parent_entry.is_present() {
        let frame = alloc_frame()?;
        let table = unsafe { &mut *(frame as *mut PageTable) };
        *table = PageTable::empty();
        
        parent_entry.set_addr(frame);
        parent_entry.set_present(true);
        parent_entry.set_writable(true);
        parent_entry.set_user(true); // Siempre permitir User en niveles intermedios
    }
    Some(unsafe { &mut *(parent_entry.addr() as *mut PageTable) })
}

/// Mapea una dirección virtual a una física en el PML4 dado.
pub fn map_page(pml4_phys: u64, virt: u64, phys: u64, user: bool, writable: bool) -> Result<(), &'static str> {
    let p4_idx = ((virt >> 39) & 0177) as usize;
    let p3_idx = ((virt >> 30) & 0177) as usize;
    let p2_idx = ((virt >> 21) & 0177) as usize;
    let p1_idx = ((virt >> 12) & 0177) as usize;
    
    let pml4 = unsafe { &mut *(pml4_phys as *mut PageTable) };
    let pdpt = get_or_alloc_table(&mut pml4.entries[p4_idx]).ok_or("OOM en PDPT")?;
    let pd = get_or_alloc_table(&mut pdpt.entries[p3_idx]).ok_or("OOM en PD")?;
    let pt = get_or_alloc_table(&mut pd.entries[p2_idx]).ok_or("OOM en PT")?;
    
    let entry = &mut pt.entries[p1_idx];
    entry.set_addr(phys);
    entry.set_present(true);
    entry.set_writable(writable);
    entry.set_user(user);
    
    Ok(())
}
