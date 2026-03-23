use crate::memory;
use crate::println;
use crate::virtio::VirtioDevice;
use core::mem::size_of;

// VirtQueue Structures
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct VirtqAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; 32], // Fixed size 32 for simplicity
    pub used_event: u16,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct VirtqUsedElem {
    pub id: u32,
    pub len: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct VirtqUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtqUsedElem; 32],
    pub avail_event: u16,
}

pub const VRING_DESC_F_NEXT: u16 = 1;
pub const VRING_DESC_F_WRITE: u16 = 2;

pub struct VirtQueue {
    queue_idx: u16,
    desc: *mut VirtqDesc,
    avail: *mut VirtqAvail,
    used: *mut VirtqUsed,
    q_size: u16,
    last_used_idx: u16,
    free_head: u16,
    num_free: u16,
}

impl VirtQueue {
    pub fn new(dev: &VirtioDevice, idx: u16) -> Option<Self> {
        dev.select_queue(idx);
        let q_size = dev.get_queue_size();
        if q_size == 0 {
            crate::println(&alloc::format!("VirtQueue: Queue {} has size 0", idx));
            return None;
        }
        
        // Ensure we support 32
        if q_size < 32 {
            crate::println(&alloc::format!("VirtQueue: Queue {} size {} too small (need 32)", idx, q_size));
            return None;
        }

        // Allocate memory. 
        // Layout:
        // Desc: 16 * 32 = 512 bytes
        // Avail: 6 + 2*32 = 70 bytes
        // Used: 6 + 8*32 = 262 bytes
        // Total < 4096. One page is enough if Used alignment (4096) isn't strict OR if we manually align.
        // Legacy requires Used to be aligned to 4096. 
        // So we need 2 pages. Page 1: Desc + Avail. Page 2: Used.

        let frame1 = memory::alloc_frame().expect("VirtQueue OOM");
        let frame2 = memory::alloc_frame().expect("VirtQueue OOM");

        // Verify contiguous (legacy requirement hack)
        if frame2 != frame1 + 4096 {
             crate::println("VirtQueue: Failed to allocate contiguous pages");
             return None;
        }

        unsafe {
            let desc_ptr = frame1 as *mut VirtqDesc;
            let avail_ptr = (frame1 + 512) as *mut VirtqAvail; // Offset 512
            let used_ptr = frame2 as *mut VirtqUsed;

            // Init Descriptors (free list)
            for i in 0..32 {
                 (*desc_ptr.add(i)).next = (i + 1) as u16;
                 (*desc_ptr.add(i)).flags = 0;
            }
            // Last one
            (*desc_ptr.add(31)).next = 0; 

            // Init Avail/Used
            (*avail_ptr).flags = 0;
            (*avail_ptr).idx = 0;
            (*used_ptr).flags = 0;
            (*used_ptr).idx = 0;

            dev.set_queue_pfn((frame1 / 4096) as u32);

            Some(Self {
                queue_idx: idx,
                desc: desc_ptr,
                avail: avail_ptr,
                used: used_ptr,
                q_size: 32, // limit to 32 even if device has more
                last_used_idx: 0,
                free_head: 0,
                num_free: 32,
            })
        }
    }

    pub fn available_space(&self) -> u16 {
        self.num_free
    }

    // Add a buffer to the Available ring.
    // Returns descriptor index used, or None if full.
    // Currently supports chain of 2 descriptors: Header (buffer1) + Packet (buffer2)
    // If buffer1 is None, only uses buffer2 (1 descriptor).
    pub unsafe fn add_buf(&mut self, buffer1: Option<&[u8]>, buffer2: &[u8], is_write: bool) -> Option<u16> {
        let needed = if buffer1.is_some() { 2 } else { 1 };
        if self.num_free < needed {
            return None;
        }

        let head = self.free_head;
        let mut curr = head;
        
        // Descriptor 1 (Optional Header)
        if let Some(buf1) = buffer1 {
             let desc = self.desc.add(curr as usize);
             (*desc).addr = buf1.as_ptr() as u64;
             (*desc).len = buf1.len() as u32;
             (*desc).flags = if is_write { VRING_DESC_F_WRITE } else { 0 };
             (*desc).flags |= VRING_DESC_F_NEXT;
             
             let next = (*desc).next;
             curr = next;
        }

        // Descriptor 2 (Data)
        let desc = self.desc.add(curr as usize);
        (*desc).addr = buffer2.as_ptr() as u64;
        (*desc).len = buffer2.len() as u32;
        (*desc).flags = if is_write { VRING_DESC_F_WRITE } else { 0 };
        // No NEXT for last one
        
        // Update free list
        self.num_free -= needed;
        self.free_head = (*desc).next; // New head is what used to be next of tail

        // Add to Available Ring
        let avail_idx = (*self.avail).idx;
        (*self.avail).ring[avail_idx as usize % 32] = head;
        
        // Memory Barrier
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        
        (*self.avail).idx = avail_idx.wrapping_add(1);

        Some(head)
    }
    
    // Check for used buffers
    // Returns (desc_index, length)
    pub unsafe fn pop_used(&mut self) -> Option<(u32, u32)> {
        let used_idx = (*self.used).idx;
        if used_idx == self.last_used_idx {
            return None;
        }

        let elem = (*self.used).ring[self.last_used_idx as usize % 32];
        self.last_used_idx = self.last_used_idx.wrapping_add(1);

        // Recycle descriptors? 
        // For now user must manually free them or we need a way to track chain length to free.
        // Simplified: We assume 1-2 descriptors and just recycle them here?
        // Actually, 'elem.id' is the head descriptor index.
        // We need to walk the chain from 'elem.id' and add back to free list.
        
        let head = elem.id as u16;
        let length = elem.len;
        
        // Helper to put back in free list
        // This logic heavily depends on how we allocated. 
        // If we just put 'head' back to free_head, we need to find tail of 'head' chain.
        // But we lost the 'next' links when we initialized? No, 'next' is preserved in the used descriptors?
        // Wait, the device doesn't modify 'next' of descriptors.
        
        // Find tail of chain starting at head
        let mut tail = head;
        let mut count = 1;
        while ((*self.desc.add(tail as usize)).flags & VRING_DESC_F_NEXT) != 0 {
             tail = (*self.desc.add(tail as usize)).next;
             count += 1;
        }
        
        // Link tail -> old free head
        (*self.desc.add(tail as usize)).next = self.free_head;
        // New free head -> head
        self.free_head = head;
        self.num_free += count;

        Some((elem.id, length))
    }

    pub fn notify(&self, dev: &VirtioDevice) {
        dev.notify_queue(self.queue_idx);
    }
}
