use crate::pci::PciDevice;
use crate::virtio::{VirtioDevice, VIRTIO_STATUS_ACKNOWLEDGE, VIRTIO_STATUS_DRIVER, VIRTIO_STATUS_DRIVER_OK, VIRTIO_STATUS_FAILED};
use crate::println;
use crate::memory;

// VirtIO Block Request Type
const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;

#[repr(C, packed)]
struct VirtioBlkReq {
    type_: u32,
    reserved: u32,
    sector: u64,
}

// VirtQueue Structures
#[repr(C, packed)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C, packed)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 16], // Small queue for now
    used_event: u16,
}

#[repr(C, packed)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C, packed)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 16],
    avail_event: u16,
}

// Flag constants
const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

// Static driver state
static mut BLOCK_DEVICE: Option<VirtioBlockDriver> = None;

struct VirtioBlockDriver {
    dev: VirtioDevice,
    queue_desc: *mut VirtqDesc,
    queue_avail: *mut VirtqAvail,
    queue_used: *mut VirtqUsed,
    idx: u16, // Driver's index for avail ring
    q_size: u16,
    msg_buffer: *mut u8, // Page for headers/status
}

impl VirtioBlockDriver {
    unsafe fn request(&mut self, sector: u64, buffer: &mut [u8], is_write: bool) -> bool {
        let req = self.msg_buffer as *mut VirtioBlkReq;
        (*req).type_ = if is_write { VIRTIO_BLK_T_OUT } else { VIRTIO_BLK_T_IN };
        (*req).reserved = 0;
        (*req).sector = sector;
        
        let bounce_buffer = self.msg_buffer.add(16);
        
        if is_write {
            // Copy user buffer to bounce buffer
            core::ptr::copy_nonoverlapping(buffer.as_ptr(), bounce_buffer, 512);
        }
        
        // Setup Descriptors
        let desc = self.queue_desc;
        
        // 1. Header
        (*desc.add(0)).addr = req as u64;
        (*desc.add(0)).len = 16;
        (*desc.add(0)).flags = VRING_DESC_F_NEXT;
        (*desc.add(0)).next = 1;
        
        // 2. Data
        (*desc.add(1)).addr = bounce_buffer as u64;
        (*desc.add(1)).len = 512;
        // If Write: Device reads from buffer (No Write Flag)
        // If Read: Device writes to buffer (Write Flag)
        (*desc.add(1)).flags = if is_write { VRING_DESC_F_NEXT } else { VRING_DESC_F_NEXT | VRING_DESC_F_WRITE };
        (*desc.add(1)).next = 2;
        
        // 3. Status
        let status_ptr = self.msg_buffer.add(16 + 512);
        *status_ptr = 0xFF;
        (*desc.add(2)).addr = status_ptr as u64;
        (*desc.add(2)).len = 1;
        (*desc.add(2)).flags = VRING_DESC_F_WRITE;
        (*desc.add(2)).next = 0;
        
        // Update Available Ring
        let avail = self.queue_avail;
        (*avail).ring[self.idx as usize % self.q_size as usize] = 0; // Head of chain is Desc 0
        
        // Memory Barrier
        
        self.idx = self.idx.wrapping_add(1);
        (*avail).idx = self.idx;
        
        // Notify
        self.dev.notify_queue(0);
        
        // Poll for completion (Spin wait with timeout)
        let used = self.queue_used;
        let start_tick = crate::timer::ticks();
        loop {
            let used_idx_ptr = core::ptr::addr_of!((*used).idx);
            let used_idx = core::ptr::read_volatile(used_idx_ptr);
            if used_idx == self.idx {
                break;
            }
            
            // 1 second timeout (assuming 100Hz or 1000Hz, we'll check >= 1000 ticks)
            if crate::timer::ticks().wrapping_sub(start_tick) > 1000 {
                println("VirtIO Block: Request Timeout!");
                return false;
            }
            
            core::hint::spin_loop();
        }
        
        // Check status
        let status = core::ptr::read_volatile(status_ptr);
        if status == 0 {
            if !is_write {
                 // Copy bounce buffer to user buffer
                 core::ptr::copy_nonoverlapping(bounce_buffer, buffer.as_mut_ptr(), 512);
            }
            return true;
        }
        println("VirtIO Block: Request Failed status != 0");
        false
    }

    pub fn read_sector(&mut self, sector: u64, buffer: &mut [u8]) -> bool {
        unsafe { self.request(sector, buffer, false) }
    }

    pub fn write_sector(&mut self, sector: u64, buffer: &[u8]) -> bool {
        // Cast const slice to mut slice for signature matching (internal/unsafe)
        // Ideally change signature but for now cast
        let ptr = buffer.as_ptr() as *mut u8;
        let len = buffer.len();
        let mut_slice = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
        unsafe { self.request(sector, mut_slice, true) }
    }
}


pub fn init(pci_dev: PciDevice) {
    if let Some(dev) = VirtioDevice::new(pci_dev) {
        println("VirtIO Block: Found.");
        
        unsafe { crate::pci::enable_bus_master(pci_dev.bus, pci_dev.slot, pci_dev.func); }
        
        dev.reset();
        dev.add_status(VIRTIO_STATUS_ACKNOWLEDGE);
        dev.add_status(VIRTIO_STATUS_DRIVER);
        
        // Use Frame Allocator to get a page for the queue
        // We look for a page, assume it succeeds for this prototype.
        if let Some(frame_addr) = memory::alloc_frame() {
             unsafe {
                // Layout for Queue Size 16:
                // Desc: 16 * 16 = 256 bytes @ Offset 0
                // Avail: 6 + 2*16 = 38 bytes @ Offset 256
                // Used:  6 + 8*16 = 134 bytes @ Offset 4096 (Alignment usually required)
                // Actually, Legacy VirtIO requires Used ring to be on 4096 byte boundary?
                // The formula is: align(Avail + sizeof(Avail), 4096)
                
                // Let's use 2 pages? Or just one since it fits if we ignore strict alignment?
                // QEMU is lenient usually, but spec says 4k alignment for Used.
                // 16 descriptors is very small.
                // Page 1: Desc + Avail
                // Page 2: Used
                
                let _desc_ptr = frame_addr as *mut VirtqDesc;
                let _avail_ptr = (frame_addr + 0x400) as *mut VirtqAvail; // Arbitrary offset
                
                // Alloc another page for Used ring to be safe on alignment
                 let frame2 = memory::alloc_frame().unwrap();
                 let _used_ptr = frame2 as *mut VirtqUsed;
                 
                  // Alloc page for headers
                 let frame3 = memory::alloc_frame().unwrap();
                 
                 // Configure PFN
                 // Legacy: Address is PFN (page frame number) of the start of the region?
                 // Wait, legacy writes PFN to port. It expects physically contiguous.
                 // If we split internal rings, we can't use standard legacy setup easily.
                 // Standard legacy setup: 
                 // Write PFN to VIRTIO_REG_QUEUE_ADDRESS.
                 // The device calculates:
                 // Desc = PFN * 4096
                 // Avail = Desc + size
                 // Used = align(Avail_end, 4096)
                 
                 // So we MUST have contiguous memory if Used pushes past page boundary.
                 // For Queue Size 16:
                 // Desc (256) + Avail (38) = 294 bytes.
                 // Padded to 4096.
                 // Used (134).
                 // Total < 8192. 
                 // So if we give it PFN, it expects Used at PFN*4096 + 4096.
                 // So we need 2 contiguous pages.
                 // memory::alloc_frame does not guarantee contiguous if called twice.
                 // But in early boot (linear alloc), it usually is. 
                 
                 // HACK: Check if frame2 == frame1 + 4096. If not, panic.
                 if frame2 != frame_addr + 4096 {
                     println("VirtIO Block: Failed to allocate contiguous queue pages!");
                     return;
                 }
                 
                 // Setup Queue 0
                 dev.select_queue(0);
                 
                 let q_size = dev.get_queue_size();
                 if q_size == 0 {
                     println("VirtIO Block: Queue 0 has size 0!");
                     return;
                 }
                 
                 // Debug print queue size
                 crate::println("VirtIO Queue Size found (raw):");
                 // crate::utils::print_hex(q_size as u64); // Not available

                 // Legacy Layout Requirements:
                 // 1. Descriptors: 16 bytes * q_size
                 // 2. Available: 6 bytes + 2 * q_size
                 // 3. Padding to 4096
                 // 4. Used: 6 bytes + 8 * q_size
                 
                 let desc_size = (q_size as usize) * 16;
                 let avail_size = 6 + (q_size as usize) * 2;
                 let used_size = 6 + (q_size as usize) * 8;
                 
                 // Calculate size of first part (Desc + Avail)
                 let part1_size = desc_size + avail_size;
                 
                 // Must align part1 end to 4096 for Used ring start
                 let part1_pages = (part1_size + 4095) / 4096;
                 
                 // Used ring pages
                 let used_pages = (used_size + 4095) / 4096;
                 
                 let total_pages = part1_pages + used_pages;
                 
                 // Alloc first page to get start
                 let base_addr = match memory::alloc_frame() {
                     Some(a) => a,
                     None => return,
                 };
                 
                 // Alloc remaining pages and verify contiguous
                 for i in 1..total_pages {
                     let next = memory::alloc_frame();
                     match next {
                         Some(addr) => {
                             if addr != base_addr + (i as u64 * 4096) {
                                 println("VirtIO Block: Failed to alloc contiguous memory (fragmented).");
                                 // Ideally we should free here, but we don't have free implementation yet.
                                 return;
                             }
                         }
                         None => {
                             println("VirtIO Block: OOM during queue alloc.");
                             return;
                         }
                     }
                 }
                 
                 // Pointers
                 let desc_ptr = base_addr as *mut VirtqDesc;
                 let avail_ptr = (base_addr + desc_size as u64) as *mut VirtqAvail;
                 let used_ptr = (base_addr + (part1_pages as u64 * 4096)) as *mut VirtqUsed;
                 
                 dev.set_queue_pfn((base_addr / 4096) as u32);
                 
                 let driver = VirtioBlockDriver {
                     dev: dev,
                     queue_desc: desc_ptr,
                     queue_avail: avail_ptr,
                     queue_used: used_ptr,
                     idx: 0,
                     q_size: q_size,
                     msg_buffer: frame3 as *mut u8,
                 };
                 
                 driver.dev.add_status(VIRTIO_STATUS_DRIVER_OK);
                 
                 BLOCK_DEVICE = Some(driver);
                 println("VirtIO Block: Initialized & Ready.");
             }
        } else {
             println("VirtIO Block: Failed to allocate memory.");
        }
    }
}
pub fn write(lba: u64, buffer: &[u8]) -> bool {
    unsafe {
        if let Some(driver) = &mut BLOCK_DEVICE {
            return driver.write_sector(lba, buffer);
        }
    }
    false
}

pub fn read(lba: u64, buffer: &mut [u8]) -> bool {
    unsafe {
        if let Some(driver) = &mut BLOCK_DEVICE {
            // println("VirtIO Block: Reading LBA...");
            return driver.read_sector(lba, buffer);
        }
    }
    false
}
