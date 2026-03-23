use crate::pci::PciDevice;
use crate::virtio::{VirtioDevice, VIRTIO_STATUS_ACKNOWLEDGE, VIRTIO_STATUS_DRIVER, VIRTIO_STATUS_DRIVER_OK, VIRTIO_STATUS_FAILED};
use crate::virtio::queue::VirtQueue;
use crate::println;
use alloc::vec::Vec;
use core::cell::RefCell;

// VirtIO Net Header
#[repr(C, packed)]
struct VirtioNetHeader {
    flags: u8,
    gso_type: u8,
    hdr_len: u16,
    gso_size: u16,
    csum_start: u16,
    csum_offset: u16,
}

pub struct VirtioNetDriver {
    dev: VirtioDevice,
    rx_queue: VirtQueue,
    tx_queue: VirtQueue,
    mac: [u8; 6],
    // Buffers for RX. 
    // In a real driver, we'd have a pool of pages.
    // Here we just keep track of buffers we gave to RX queue.
    // For simplicity, we allocate new buffers when refilling? 
    // Or we use a fixed set of static buffers?
    // Let's use a slab of buffers.
    rx_buffers: Vec<Vec<u8>>, 
}

impl VirtioNetDriver {
    pub fn new(pci_dev: PciDevice) -> Option<Self> {
        let dev = VirtioDevice::new(pci_dev)?;
        
        dev.reset();
        dev.add_status(VIRTIO_STATUS_ACKNOWLEDGE);
        dev.add_status(VIRTIO_STATUS_DRIVER);
        
        // Negotiate Features
        // We want VIRTIO_NET_F_MAC (bit 5) usually.
        // Bit 5 = 1 << 5 = 32
        let features = dev.get_features();
        if (features & (1 << 5)) != 0 {
             dev.set_features(1 << 5);
        } else {
             dev.set_features(0);
        }

        let rx = VirtQueue::new(&dev, 0)?;
        let tx = VirtQueue::new(&dev, 1)?;
        
        dev.add_status(VIRTIO_STATUS_DRIVER_OK);
        
        let mut mac = [0u8; 6];
        for i in 0..6 {
            mac[i] = dev.read_config_byte(i as u16);
        }
        
        let mut drv = Self {
            dev,
            rx_queue: rx,
            tx_queue: tx,
            mac,
            rx_buffers: Vec::new(),
        };

        // Fill RX queue
        drv.refill_rx();
        
        Some(drv)
    }

    fn refill_rx(&mut self) {
        // Populate RX queue with buffers
        // We use 1500 byte buffers + header
        // Header is 10 bytes (legacy) or 12 (modern)? 
        // Legacy header is 10 bytes.
        // We need 2 descriptors: Header (10) + Packet (1514)
        // Or 1 descriptor if we merge them?
        // Let's use 1 descriptor of size 1524?
        // But the device writes header fields...
        // Let's safe side: 2048 bytes buffer, use single descriptor?
        // Device writes header at start of buffer.
        
        while self.rx_queue.available_space() > 0 {
            // Alloc a buffer
            let mut vec = Vec::with_capacity(2048);
            unsafe { vec.set_len(2048); }
            
            // Add to queue
            // We pass slice.
            unsafe {
                if let Some(_idx) = self.rx_queue.add_buf(None, &vec, true) { // Write = true (Device writes)
                     self.rx_buffers.push(vec);
                } else {
                    break;
                }
            }
        }
        self.rx_queue.notify(&self.dev);
    }

    pub fn transmit(&mut self, packet: &[u8]) {
        // virtio-net requires a header before the packet.
        // In legacy, we just zero it?
        // We need 1 buffer for header, 1 for packet.
        let header = VirtioNetHeader {
            flags: 0,
            gso_type: 0,
            hdr_len: 0,
            gso_size: 0,
            csum_start: 0,
            csum_offset: 0,
        };
        
        let header_slice = unsafe {
            core::slice::from_raw_parts(&header as *const _ as *const u8, core::mem::size_of::<VirtioNetHeader>())
        };

        unsafe {
             self.tx_queue.add_buf(Some(header_slice), packet, false); // Write = false (Device reads)
        }
        self.tx_queue.notify(&self.dev);
        
        // Reclaim TX buffers?
        // We assume synchronous or fire-and-forget for now?
        // Eventually we need to pop used TX buffers to free descriptors.
        self.process_tx_cleanup();
    }
    
    pub fn process_tx_cleanup(&mut self) {
        unsafe {
            while let Some(_) = self.tx_queue.pop_used() {
                // Just recycling descriptors currently
            }
        }
    }

    pub fn receive(&mut self) -> Option<Vec<u8>> {
        unsafe {
            if let Some((_desc_id, len)) = self.rx_queue.pop_used() {
                // We got a packet!
                // Problem: We need to know WHICH buffer it was to return it.
                // Our simple VirtQueue implementation recycles descriptors internally 
                // but doesn't give us back the "cookie" or index easily to map to rx_buffers.
                // This is a limitation of the simple queue.
                
                // For this prototype, we just treat rx_buffers as a FIFO matching the specific order?
                // Real VirtIO drivers use 'id' field in descriptor as index into a buffer array.
                // Our 'add_buf' returns 'head' index. And 'pop_used' returns 'id'.
                // So if we track (head_id -> buffer_index), we can find it.
                
                // HACK: Since we assume FIFO for now (simplicity):
                if !self.rx_buffers.is_empty() {
                    let buf = self.rx_buffers.remove(0);
                    // Actual length include header (10 bytes)
                    // The payload starts at offset 10.
                    let header_len = core::mem::size_of::<VirtioNetHeader>();
                    if len as usize > header_len {
                         let payload_len = len as usize - header_len;
                         let mut packet = Vec::with_capacity(payload_len);
                         packet.extend_from_slice(&buf[header_len..header_len+payload_len]);
                         
                         // Refill
                         self.refill_rx();
                         return Some(packet);
                    }
                }
                
                self.refill_rx();
            }
        }
        None
    }
    
    pub fn mac_address(&self) -> [u8; 6] {
        self.mac
    }
}

// Global Network Driver
pub static mut GLOBAL_NET: Option<VirtioNetDriver> = None;

pub fn init(pci_dev: PciDevice) {
    if let Some(drv) = VirtioNetDriver::new(pci_dev) {
        crate::println(&alloc::format!("VirtIO Net: Initialized. MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", 
            drv.mac[0], drv.mac[1], drv.mac[2], drv.mac[3], drv.mac[4], drv.mac[5]));
            
        unsafe { GLOBAL_NET = Some(drv); }
    } else {
        println("VirtIO Net: Failed to initialize.");
    }
}
