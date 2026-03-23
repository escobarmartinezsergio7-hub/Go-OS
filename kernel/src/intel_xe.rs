use crate::pci::{PciDevice, read_bar};
use crate::println;

// Intel Vendor ID
const VENDOR_INTEL: u16 = 0x8086;

// BAR Offsets for Graphics
// BAR0 is usually MMIO (GTTMMAD) - 16MB
const BAR_GTTMMAD: u8 = 0;

// Engine Offsets (Gen12+)
const BCS_BASE: u32 = 0x22000; // Blitter Command Streamer

// Registers relative to Engine Base
const RING_TAIL: u32 = 0x00;
const RING_HEAD: u32 = 0x04;
const RING_START: u32 = 0x08;
const RING_CTL: u32 = 0x0C;

// Commands
const MI_BATCH_BUFFER_END: u32 = 0x05000000;
const XY_SRC_COPY_BLT_CMD: u32 = 0x53000000;

// GTT (Graphics Translation Table) Constants
const GTT_OFFSET: u32 = 0x40000;
const GTT_PAGE_SIZE: u32 = 4096;
const GTT_PTE_PRESENT: u64 = 0x01;

pub struct GttManager {
    mmio_base: u64,
}

impl GttManager {
    pub fn new(mmio_base: u64) -> Self {
        Self { mmio_base }
    }

    pub unsafe fn map(&self, gpu_addr: u64, phys_addr: u64) {
        let pte_index = (gpu_addr / GTT_PAGE_SIZE as u64) as usize;
        let pte_addr = (self.mmio_base + GTT_OFFSET as u64 + (pte_index * 8) as u64) as *mut u64;
        
        // Basic PTE: Physical Address | Present
        let pte_value = (phys_addr & !0xFFF) | GTT_PTE_PRESENT;
        *pte_addr = pte_value;
    }
}

pub struct CommandStreamer {
    pub base: u32,
    pub mmio_base: u64,
}

impl CommandStreamer {
    pub fn new(mmio_base: u64, engine_base: u32) -> Self {
        Self { base: engine_base, mmio_base }
    }

    pub unsafe fn write_tail(&self, tail: u32) {
        let reg = (self.mmio_base + (self.base + RING_TAIL) as u64) as *mut u32;
        *reg = tail;
    }

    pub unsafe fn read_head(&self) -> u32 {
        let reg = (self.mmio_base + (self.base + RING_HEAD) as u64) as *const u32;
        *reg
    }
}

pub struct RingBuffer {
    streamer: CommandStreamer,
    cpu_ptr: *mut u32,
    phys_addr: u64,
    size: u32,
    tail: u32,
}

impl RingBuffer {
    pub unsafe fn new(mmio_base: u64, engine_base: u32) -> Self {
        let size = 4096; // 1 page
        let phys_addr = crate::memory::alloc_frame().expect("Failed to alloc Ring Buffer");
        let cpu_ptr = phys_addr as *mut u32;

        // Reset Ring
        for i in 0..(size/4) {
            *cpu_ptr.add(i as usize) = 0;
        }

        let mut ring = Self {
            streamer: CommandStreamer::new(mmio_base, engine_base),
            cpu_ptr,
            phys_addr,
            size,
            tail: 0,
        };

        ring.init_hardware();
        ring
    }

    unsafe fn init_hardware(&mut self) {
        let base = self.streamer.mmio_base + self.streamer.base as u64;
        
        // 1. Disable Ring
        let ctl_reg = (base + RING_CTL as u64) as *mut u32;
        *ctl_reg = 0;

        // 2. Set Start
        let start_reg = (base + RING_START as u64) as *mut u32;
        *start_reg = self.phys_addr as u32; 

        // 3. Set Head/Tail
        let head_reg = (base + RING_HEAD as u64) as *mut u32;
        *head_reg = 0;
        let tail_reg = (base + RING_TAIL as u64) as *mut u32;
        *tail_reg = 0;

        // 4. Set Length and Enable
        // Bits 12-20: Size in pages minus 1
        let size_val = (self.size / 4096) - 1;
        *ctl_reg = (size_val << 12) | 0x01; // Enable bit
    }

    pub unsafe fn write(&mut self, data: u32) {
        // Simple wrap-around logic
        *self.cpu_ptr.add((self.tail / 4) as usize) = data;
        self.tail = (self.tail + 4) % self.size;
    }

    pub unsafe fn submit(&mut self) {
        self.streamer.write_tail(self.tail);
    }
}

static mut BCS_RING: Option<RingBuffer> = None;
static mut GTT: Option<GttManager> = None;

pub fn init(device: PciDevice) {
    if device.vendor_id != VENDOR_INTEL {
        return;
    }

    println("Intel Xe: Controller Found.");
    unsafe { crate::pci::enable_bus_master(device.bus, device.slot, device.func); }
    
    let mmio_base = unsafe { read_bar(device.bus, device.slot, device.func, BAR_GTTMMAD) };
    
    if let Some(addr) = mmio_base {
        println("Intel Xe: GTTMMAD (MMIO) Initialized.");
        
        unsafe {
            GTT = Some(GttManager::new(addr));
            BCS_RING = Some(RingBuffer::new(addr, BCS_BASE));
        }

        println("Intel Xe: Hardware Acceleration Ready (GTT + BCS Ring).");
    }
}

pub fn blit(src_phys: u64, dst_phys: u64, x: u32, y: u32, w: u32, h: u32, pitch: u32) -> bool {
    unsafe {
        if let Some(ref mut ring) = BCS_RING {
            // XY_SRC_COPY_BLT (Gen12 command)
            // Note: In real Gen12, we must use Global GTT offsets.
            // For this pilot, we assume physical mapping is 1:1 or handled via GTT.
            
            ring.write(XY_SRC_COPY_BLT_CMD | 8); // Command + Length (10 dwords total, index starts at 0, so 8 means 10)
            ring.write(0xCC << 16 | 0x03 << 24 | (pitch as u32)); // ROP and pitch
            ring.write((y << 16) | x); // Dst Y, X
            ring.write((y + h) << 16 | (x + w)); // Dst Y2, X2
            ring.write(dst_phys as u32); // Dst Addr Low
            ring.write((dst_phys >> 32) as u32); // Dst Addr High
            ring.write(pitch as u32); // Src Pitch
            ring.write(src_phys as u32); // Src Addr Low
            ring.write((src_phys >> 32) as u32); // Src Addr High
            ring.write(MI_BATCH_BUFFER_END);
            
            ring.submit();
            return true;
        }
    }
    false
}
