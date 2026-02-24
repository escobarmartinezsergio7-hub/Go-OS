use linked_list_allocator::LockedHeap;
use core::sync::atomic::{AtomicUsize, Ordering};
use uefi::mem::memory_map::MemoryMap;

const MIB: usize = 1024 * 1024;
const PAGE_BYTES: usize = 4096;
const HEAP_MIN_MIB: usize = 64;
const HEAP_MAX_MIB: usize = 4096;
const HEAP_STEP_MIB: usize = 64;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();
static HEAP_SIZE_BYTES: AtomicUsize = AtomicUsize::new(0);
static HEAP_RESERVED_BYTES: AtomicUsize = AtomicUsize::new(0);

pub struct HeapReservation {
    bytes: usize,
}

impl Drop for HeapReservation {
    fn drop(&mut self) {
        release_heap_reservation(self.bytes);
    }
}

fn conventional_memory_mib() -> Option<usize> {
    let map = uefi::boot::memory_map(uefi::mem::memory_map::MemoryType::LOADER_DATA).ok()?;
    let mut conventional_pages: u64 = 0;
    for desc in map.entries() {
        if desc.ty == uefi::mem::memory_map::MemoryType::CONVENTIONAL {
            conventional_pages = conventional_pages.saturating_add(desc.page_count);
        }
    }
    let bytes = conventional_pages.saturating_mul(PAGE_BYTES as u64);
    Some((bytes / MIB as u64) as usize)
}

fn round_down_mib(value_mib: usize, step_mib: usize) -> usize {
    if step_mib == 0 {
        return value_mib;
    }
    (value_mib / step_mib) * step_mib
}

fn pick_heap_target_mib() -> usize {
    let conventional_mib = conventional_memory_mib().unwrap_or(512);
    // Reserve roughly 50% of detected conventional RAM for heap, with hard caps.
    let quarter = conventional_mib / 2;
    let clamped = quarter.clamp(HEAP_MIN_MIB, HEAP_MAX_MIB);
    round_down_mib(clamped, HEAP_STEP_MIB).max(HEAP_MIN_MIB)
}

pub fn heap_size_bytes() -> usize {
    HEAP_SIZE_BYTES.load(Ordering::Relaxed)
}

pub fn heap_reserved_bytes() -> usize {
    HEAP_RESERVED_BYTES.load(Ordering::Relaxed)
}

fn release_heap_reservation(bytes: usize) {
    if bytes == 0 {
        return;
    }
    let mut current = HEAP_RESERVED_BYTES.load(Ordering::Acquire);
    loop {
        let next = current.saturating_sub(bytes);
        match HEAP_RESERVED_BYTES.compare_exchange_weak(
            current,
            next,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

pub fn try_reserve_heap(bytes: usize, headroom_bytes: usize) -> Option<HeapReservation> {
    if bytes == 0 {
        return Some(HeapReservation { bytes: 0 });
    }

    let total = heap_size_bytes();
    if total == 0 {
        return None;
    }
    let usable = total.saturating_sub(headroom_bytes);
    if bytes > usable {
        return None;
    }

    let mut current = HEAP_RESERVED_BYTES.load(Ordering::Acquire);
    loop {
        let next = current.checked_add(bytes)?;
        if next > usable {
            return None;
        }
        match HEAP_RESERVED_BYTES.compare_exchange_weak(
            current,
            next,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return Some(HeapReservation { bytes }),
            Err(observed) => current = observed,
        }
    }
}

pub fn init_heap() {
    let mut target_mib = pick_heap_target_mib();
    let mut selected: Option<(usize, usize)> = None;

    while target_mib >= HEAP_MIN_MIB {
        let heap_size = target_mib * MIB;
        let pages = heap_size / PAGE_BYTES;
        if let Ok(ptr) = uefi::boot::allocate_pages(
            uefi::boot::AllocateType::AnyPages,
            uefi::mem::memory_map::MemoryType::LOADER_DATA,
            pages,
        ) {
            selected = Some((ptr.as_ptr() as usize, heap_size));
            break;
        }

        if target_mib == HEAP_MIN_MIB {
            break;
        }
        target_mib = target_mib.saturating_sub(HEAP_STEP_MIB).max(HEAP_MIN_MIB);
    }

    // Final small fallbacks for heavily fragmented systems.
    if selected.is_none() {
        for mib in [32usize, 16usize, 8usize] {
            let heap_size = mib * MIB;
            let pages = heap_size / PAGE_BYTES;
            if let Ok(ptr) = uefi::boot::allocate_pages(
                uefi::boot::AllocateType::AnyPages,
                uefi::mem::memory_map::MemoryType::LOADER_DATA,
                pages,
            ) {
                selected = Some((ptr.as_ptr() as usize, heap_size));
                break;
            }
        }
    }

    let (heap_ptr, heap_size) = selected.expect("Failed to allocate heap pages");
    unsafe {
        ALLOCATOR.lock().init(heap_ptr as *mut u8, heap_size);
    }
    HEAP_SIZE_BYTES.store(heap_size, Ordering::Relaxed);
    HEAP_RESERVED_BYTES.store(0, Ordering::Relaxed);
}
