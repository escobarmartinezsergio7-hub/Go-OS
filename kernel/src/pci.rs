use crate::hal::{outl, inl};
use crate::println;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
}

pub unsafe fn write_config(bus: u8, slot: u8, func: u8, offset: u8, value: u32) {
    let address = 0x80000000
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | (offset as u32 & 0xFC);

    outl(CONFIG_ADDRESS, address);
    outl(CONFIG_DATA, value);
}

pub unsafe fn read_config(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let address = 0x80000000
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | (offset as u32 & 0xFC);

    outl(CONFIG_ADDRESS, address);
    inl(CONFIG_DATA)
}

pub unsafe fn read_bar(bus: u8, slot: u8, func: u8, index: u8) -> Option<u64> {
    // BARs are at offset 0x10 + (index * 4)
    let offset = 0x10 + (index * 4);
    let bar_low = read_config(bus, slot, func, offset);
    
    // Check for 64-bit BAR
    let bar_type = (bar_low >> 1) & 0x03;
    let _prefetch = (bar_low >> 3) & 0x01;
    
    // Check if MMIO or IO
    if (bar_low & 0x01) == 0x01 {
        // IO Space
        // Mask out low 2 bits
        return Some((bar_low & !0x03) as u64);
    } else {
        // Memory Space
        if bar_type == 0x02 { // 64-bit
            let bar_high = read_config(bus, slot, func, offset + 4);
            let addr = ((bar_high as u64) << 32) | (bar_low & !0x0F) as u64;
            return Some(addr);
        } else {
            // 32-bit
            return Some((bar_low & !0x0F) as u64);
        }
    }
}

pub unsafe fn enable_bus_master(bus: u8, slot: u8, func: u8) {
    let cmd = read_config(bus, slot, func, 0x04);
    if (cmd & 0x04) == 0 {
        write_config(bus, slot, func, 0x04, cmd | 0x04);
    }
}

pub fn scan() {
    println("Scanning PCI bus...");
    
    for bus in 0..=255 {
        for slot in 0..32 {
            let vendor_id = unsafe { read_config(bus, slot, 0, 0x00) as u16 };
            if vendor_id == 0xFFFF {
                continue;
            }

            let device_id = unsafe { (read_config(bus, slot, 0, 0x00) >> 16) as u16 };
            let header_type = unsafe { (read_config(bus, slot, 0, 0x0C) >> 16) as u8 };
            
            check_function(bus, slot, 0, vendor_id, device_id);

            // Multi-function device?
            if (header_type & 0x80) != 0 {
                for func in 1..8 {
                    let vid = unsafe { read_config(bus, slot, func, 0x00) as u16 };
                    if vid != 0xFFFF {
                        let did = unsafe { (read_config(bus, slot, func, 0x00) >> 16) as u16 };
                        check_function(bus, slot, func, vid, did);
                    }
                }
            }
        }
    }
}

fn check_function(bus: u8, slot: u8, func: u8, vendor_id: u16, device_id: u16) {
    let class_rev = unsafe { read_config(bus, slot, func, 0x08) };
    let class_code = ((class_rev >> 24) & 0xFF) as u8;
    let sub_class = ((class_rev >> 16) & 0xFF) as u8;

    if vendor_id == 0x1AF4 {
        crate::println("Found VirtIO Device (1AF4)");
         // device_id 0x1000..0x103F for legacy, 0x1040+ for modern
        crate::virtio::probe(PciDevice {
            bus,
            slot,
            func,
            vendor_id,
            device_id,
        });
    } else if class_code == 0x01 && sub_class == 0x08 {
        crate::println("Found NVMe Controller");
        crate::nvme::init(PciDevice { bus, slot, func, vendor_id, device_id });
    } else if class_code == 0x0C && sub_class == 0x03 {
        crate::println("Found xHCI (USB 3.0) Controller");
        crate::xhci::init(PciDevice { bus, slot, func, vendor_id, device_id });
    } else if class_code == 0x04 && sub_class == 0x03 {
        crate::println("Found Intel HDA Audio Controller");
        crate::audio::init(PciDevice { bus, slot, func, vendor_id, device_id });
    } else if vendor_id == 0x8086 && class_code == 0x03 {
        // Intel Display Controller (VGA/3D)
        crate::println("Found Intel Graphics Controller");
        crate::intel_xe::init(PciDevice { bus, slot, func, vendor_id, device_id });
    } else if vendor_id == 0x8086 && class_code == 0x02 && sub_class == 0x80 {
        // Intel Wireless Network Controller
        crate::println("Found Intel Wireless Controller");
        crate::intel_wifi::init(PciDevice { bus, slot, func, vendor_id, device_id });
    } else if vendor_id == 0x8086 && class_code == 0x02 && sub_class == 0x00 {
        // Intel Ethernet Controller
        crate::println("Found Intel Ethernet Controller");
        crate::intel_net::init(PciDevice { bus, slot, func, vendor_id, device_id });
    }
}
