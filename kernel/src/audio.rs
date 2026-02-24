use crate::pci::{PciDevice, read_bar};
use crate::println;

pub fn init(device: PciDevice) {
    let bar0 = unsafe { read_bar(device.bus, device.slot, device.func, 0) };
    if let Some(_) = bar0 {
        println("Audio (HDA): Initialized (Stub). MMIO Base found.");
    } else {
        println("Audio: Failed to find BAR0.");
    }
}
