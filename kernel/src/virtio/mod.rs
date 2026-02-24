use crate::hal::{inb, inl, outb, outl, outw, inw};
use crate::pci::{self, PciDevice};
use crate::println;

pub mod block;
mod input;
pub mod net;
pub mod queue;

// Legacy VirtIO Header Offsets (IO Space)
const VIRTIO_REG_HOST_FEATURES: u16 = 0x00;
const VIRTIO_REG_GUEST_FEATURES: u16 = 0x04;
const VIRTIO_REG_QUEUE_ADDRESS: u16 = 0x08;
const VIRTIO_REG_QUEUE_SIZE: u16 = 0x0C;
const VIRTIO_REG_QUEUE_SELECT: u16 = 0x0E;
const VIRTIO_REG_QUEUE_NOTIFY: u16 = 0x10;
const VIRTIO_REG_DEVICE_STATUS: u16 = 0x12;
const VIRTIO_REG_ISR_STATUS: u16 = 0x13;

// Status bits
const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_FAILED: u8 = 128;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;

pub struct VirtioDevice {
    io_base: u16,
    irq: u8,
}

impl VirtioDevice {
    pub fn new(pci_dev: PciDevice) -> Option<Self> {
        // Read BAR0 to get I/O base
        let bar0 = unsafe { pci::read_config(pci_dev.bus, pci_dev.slot, pci_dev.func, 0x10) };
        // Check if I/O space (bit 0 set)
        if (bar0 & 1) == 0 {
            println("VirtIO: BAR0 is not I/O space");
            return None;
        }

        let io_base = (bar0 & !3) as u16;
        let irq_line = unsafe { pci::read_config(pci_dev.bus, pci_dev.slot, pci_dev.func, 0x3C) } as u8;

        Some(Self { io_base, irq: irq_line })
    }

    pub fn reset(&self) {
        unsafe { outb(self.io_base + VIRTIO_REG_DEVICE_STATUS, 0) };
    }

    pub fn set_status(&self, status: u8) {
        unsafe { outb(self.io_base + VIRTIO_REG_DEVICE_STATUS, status) };
    }
    
    pub fn add_status(&self, status: u8) {
        let old = unsafe { inb(self.io_base + VIRTIO_REG_DEVICE_STATUS) };
        unsafe { outb(self.io_base + VIRTIO_REG_DEVICE_STATUS, old | status) };
    }

    pub fn get_status(&self) -> u8 {
        unsafe { inb(self.io_base + VIRTIO_REG_DEVICE_STATUS) }
    }

    pub fn get_features(&self) -> u32 {
        unsafe { inl(self.io_base + VIRTIO_REG_HOST_FEATURES) }
    }
    
    pub fn set_features(&self, features: u32) {
        unsafe { outl(self.io_base + VIRTIO_REG_GUEST_FEATURES, features) };
    }

    pub fn read_config_byte(&self, offset: u16) -> u8 {
        unsafe { inb(self.io_base + 20 + offset) }
    }
    
    pub fn select_queue(&self, idx: u16) {
        unsafe { outw(self.io_base + VIRTIO_REG_QUEUE_SELECT, idx) };
    }
    
    pub fn set_queue_pfn(&self, pfn: u32) {
        unsafe { outl(self.io_base + VIRTIO_REG_QUEUE_ADDRESS, pfn) };
    }
    
    pub fn notify_queue(&self, idx: u16) {
        unsafe { outw(self.io_base + VIRTIO_REG_QUEUE_NOTIFY, idx) };
    }
    
    pub fn get_queue_size(&self) -> u16 {
        unsafe { inw(self.io_base + VIRTIO_REG_QUEUE_SIZE) }
    }
}

pub fn probe(device: PciDevice) {
    if device.device_id >= 0x1040 {
        let _legacy_id = device.device_id - 0x1040 + 0x1000;
        println("VirtIO: Modern device found (unsupported).");
        // If we forced legacy, it should have a legacy ID (0x1000-0x103F).
        // If it still shows up as 0x1040+, it means it's strictly modern or transitional.
        // QEMU disable-modern=on should force ID 0x1000..
    }

    match device.device_id {
        0x1001 => block::init(device),
        0x1000 => net::init(device),
        0x1002 => input::init(device),
        _ => {
            // Check if it's a transitional device with a different ID?
            // Usually 0x1000-0x103F are the ones we care about for legacy I/O.
             println("Unknown VirtIO device ID (legacy)");
        }
    }
}
