use uefi::mem::memory_map::{MemoryDescriptor, MemoryMap, MemoryType};
use uefi::{boot, Status};

pub const PAGE_SIZE: u64 = 4096;
const MAX_REGIONS: usize = 128;

#[derive(Clone, Copy)]
struct Region {
    start_phys: u64,
    pages: u64,
    next_page: u64,
}

impl Region {
    const fn empty() -> Self {
        Self {
            start_phys: 0,
            pages: 0,
            next_page: 0,
        }
    }

    fn available(&self) -> bool {
        self.next_page < self.pages
    }

    fn alloc_one(&mut self) -> Option<u64> {
        if !self.available() {
            return None;
        }

        let addr = self.start_phys + self.next_page.saturating_mul(PAGE_SIZE);
        self.next_page = self.next_page.saturating_add(1);
        Some(addr)
    }
}

#[derive(Clone, Copy)]
pub struct MemoryStats {
    pub regions: u64,
    pub total_pages: u64,
    pub conventional_pages: u64,
    pub reserved_pages: u64,
    pub largest_conventional_pages: u64,
    pub conventional_regions: u64,
}

impl MemoryStats {
    const fn empty() -> Self {
        Self {
            regions: 0,
            total_pages: 0,
            conventional_pages: 0,
            reserved_pages: 0,
            largest_conventional_pages: 0,
            conventional_regions: 0,
        }
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_pages.saturating_mul(PAGE_SIZE)
    }

    pub fn conventional_bytes(&self) -> u64 {
        self.conventional_pages.saturating_mul(PAGE_SIZE)
    }
}

#[derive(Clone, Copy)]
pub struct AllocatorState {
    pub tracked_regions: usize,
    pub allocations: u64,
    pub failed_allocations: u64,
}

impl AllocatorState {
    const fn empty() -> Self {
        Self { tracked_regions: 0, allocations: 0, failed_allocations: 0 }
    }
}

struct FrameAllocator {
    regions: [Region; MAX_REGIONS],
    region_count: usize,
    cursor: usize,
    allocations: u64,
    failed_allocations: u64,
}

impl FrameAllocator {
    const fn new() -> Self {
        Self {
            regions: [Region::empty(); MAX_REGIONS],
            region_count: 0,
            cursor: 0,
            allocations: 0,
            failed_allocations: 0,
        }
    }

    fn reset(&mut self) {
        self.regions = [Region::empty(); MAX_REGIONS];
        self.region_count = 0;
        self.cursor = 0;
        self.allocations = 0;
        self.failed_allocations = 0;
    }

    fn add_region(&mut self, start_phys: u64, pages: u64) {
        if pages == 0 || self.region_count >= MAX_REGIONS {
            return;
        }

        self.regions[self.region_count] = Region {
            start_phys,
            pages,
            next_page: 0,
        };
        self.region_count += 1;
    }

    fn alloc_frame(&mut self) -> Option<u64> {
        if self.region_count == 0 {
            self.failed_allocations = self.failed_allocations.saturating_add(1);
            return None;
        }

        let mut scanned = 0;
        while scanned < self.region_count {
            let idx = self.cursor % self.region_count;
            // Try to alloc from current cursor
            if let Some(addr) = self.regions[idx].alloc_one() {
                self.allocations = self.allocations.saturating_add(1);
                return Some(addr);
            }
            
            // Move to next region only if current failed
            self.cursor = (self.cursor + 1) % self.region_count;
            scanned += 1;
        }

        self.failed_allocations = self.failed_allocations.saturating_add(1);
        None
    }

    fn state(&self) -> AllocatorState {
        AllocatorState {
            tracked_regions: self.region_count,
            allocations: self.allocations,
            failed_allocations: self.failed_allocations,
        }
    }
}

static mut STATS: MemoryStats = MemoryStats::empty();
static mut ALLOCATOR: FrameAllocator = FrameAllocator::new();

fn analyze_map<'a, I>(entries: I) -> (MemoryStats, FrameAllocator)
where
    I: Iterator<Item = &'a MemoryDescriptor>,
{
    let mut stats = MemoryStats::empty();
    let mut allocator = FrameAllocator::new();

    for desc in entries {
        let pages = desc.page_count;
        stats.regions = stats.regions.saturating_add(1);
        stats.total_pages = stats.total_pages.saturating_add(pages);

        if desc.ty == MemoryType::CONVENTIONAL {
            stats.conventional_pages = stats.conventional_pages.saturating_add(pages);
            stats.conventional_regions = stats.conventional_regions.saturating_add(1);
            if pages > stats.largest_conventional_pages {
                stats.largest_conventional_pages = pages;
            }

            // Skip lower memory to reduce collisions with firmware/legacy ranges.
            if desc.phys_start >= 0x10_0000 {
                allocator.add_region(desc.phys_start, pages);
            }
        } else {
            stats.reserved_pages = stats.reserved_pages.saturating_add(pages);
        }
    }

    (stats, allocator)
}

fn set_state(stats: MemoryStats, allocator: FrameAllocator) {
    unsafe {
        STATS = stats;
        ALLOCATOR = allocator;
    }
}

pub fn init_from_uefi() -> Result<MemoryStats, Status> {
    let map = match boot::memory_map(MemoryType::LOADER_DATA) {
        Ok(m) => m,
        Err(e) => return Err(e.status()),
    };

    let (stats, allocator) = analyze_map(map.entries());
    set_state(stats, allocator);

    Ok(stats)
}

pub fn init_from_existing_map<M: MemoryMap>(map: &M) -> MemoryStats {
    let (stats, allocator) = analyze_map(map.entries());
    set_state(stats, allocator);
    stats
}

pub fn stats() -> MemoryStats {
    unsafe { STATS }
}

pub fn alloc_frame() -> Option<u64> {
    unsafe { ALLOCATOR.alloc_frame() }
}

pub fn allocate_dma_page() -> Option<u64> {
    // Allocate a single 4KB page for DMA use
    alloc_frame()
}

pub fn allocate_dma_page32() -> Option<u64> {
    // Some PCIe devices are more reliable when DMA descriptors/buffers stay below 4GiB.
    const MAX_TRIES: usize = 512;
    for _ in 0..MAX_TRIES {
        let addr = alloc_frame()?;
        if addr <= 0xFFFF_F000 {
            return Some(addr);
        }
    }
    None
}

pub fn allocator_state() -> AllocatorState {
    unsafe { ALLOCATOR.state() }
}
