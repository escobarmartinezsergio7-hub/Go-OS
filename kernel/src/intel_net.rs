use crate::pci::{PciDevice, read_bar};
use crate::println;
use alloc::vec::Vec;
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;

// Intel Vendor ID
const VENDOR_INTEL: u16 = 0x8086;

// Device IDs
const DEVICE_I226_V: u16 = 0x125B;
const DEVICE_I226_LM: u16 = 0x125C;
const DEVICE_I225_V: u16 = 0x15F3;
const DEVICE_I225_LM: u16 = 0x15F2;

// Registers
const REG_CTRL: u32 = 0x00000;
const REG_STATUS: u32 = 0x00008;
const REG_CTRL_EXT: u32 = 0x00018;
const REG_ICR: u32 = 0x01500; // Interrupt Cause Read
const REG_IMS: u32 = 0x01508; // Interrupt Mask Set
const REG_RCTL: u32 = 0x00100; // Receive Control
const REG_RXCTRL: u32 = 0x03000; // Receive Path Control
const REG_TCTL: u32 = 0x00400; // Transmit Control
const REG_TIPG: u32 = 0x00410; // Transmit IPG

const REG_RDBAL: u32 = 0x0C000; // RX Descriptor Base Low
const REG_RDBAH: u32 = 0x0C004; // RX Descriptor Base High
const REG_RDLEN: u32 = 0x0C008; // RX Descriptor Length
const REG_SRRCTL: u32 = 0x0C00C; // Split and Replication RX Control
const REG_RDH: u32 = 0x0C010;   // RX Descriptor Head
const REG_RDT: u32 = 0x0C018;   // RX Descriptor Tail

const REG_TDBAL: u32 = 0x0E000; // TX Descriptor Base Low
const REG_TDBAH: u32 = 0x0E004; // TX Descriptor Base High
const REG_TDLEN: u32 = 0x0E008; // TX Descriptor Length
const REG_TDH: u32 = 0x0E010;   // TX Descriptor Head
const REG_TDT: u32 = 0x0E018;   // TX Descriptor Tail

const REG_RAL: u32 = 0x05400; // Receive Address Low
const REG_RAH: u32 = 0x05404; // Receive Address High
const REG_GPRC: u32 = 0x04074; // Good Packets Received Count
const REG_GPTC: u32 = 0x04080; // Good Packets Transmitted Count

const RING_SIZE: usize = 64; // Number of descriptors

const REG_IMC: u32 = 0x0150C;
const REG_RXDCTL: u32 = 0x0C028;
const REG_TXDCTL: u32 = 0x0E028;

const CTRL_SLU: u32 = 1 << 6;
const CTRL_RST: u32 = 1 << 26;
const CTRL_EXT_DRV_LOAD: u32 = 1 << 28;
const STATUS_LU: u32 = 1 << 1;

const RAH_AV: u32 = 1 << 31;

const RXDCTL_ENABLE: u32 = 1 << 25;
const TXDCTL_ENABLE: u32 = 1 << 25;

const RCTL_EN: u32 = 1 << 1;
const RCTL_MPE: u32 = 1 << 4;
const RCTL_BAM: u32 = 1 << 15;
const RCTL_SECRC: u32 = 1 << 26;
const RXCTRL_RXEN: u32 = 1 << 0;
const SRRCTL_DESCTYPE_ADV_ONEBUF: u32 = 0x0200_0000;
const SRRCTL_BSIZEPKT_SHIFT: u32 = 10;

const ADV_RX_STAT_DD: u32 = 1 << 0;
const ADV_RX_STAT_EOP: u32 = 1 << 1;
const RX_MIN_FRAME_LEN: usize = 14;
const RX_MAX_FRAME_LEN: usize = 2048;

const TCTL_EN: u32 = 1 << 1;
const TCTL_PSP: u32 = 1 << 3;
const TCTL_CT: u32 = 0x10 << 4;
const TCTL_COLD: u32 = 0x40 << 12;

const TX_CMD_EOP: u8 = 1 << 0;
const TX_CMD_IFCS: u8 = 1 << 1;
const TX_CMD_RS: u8 = 1 << 3;

pub static mut RX_COUNT: u64 = 0;
pub static mut TX_COUNT: u64 = 0;

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct IntelDescriptor {
    addr: u64,
    length: u16,
    cso: u8,
    cmd: u8,
    status: u8,
    css: u8,
    special: u16,
}

pub struct IntelNetDevice {
    pub pci: PciDevice,
    pub mmio_base: u64,
    pub mac_addr: [u8; 6],
    
    rx_ring_phys: u64,
    rx_ring: *mut IntelDescriptor,
    rx_buffers: Vec<u64>,
    rx_cur: usize,

    tx_ring_phys: u64,
    tx_ring: *mut IntelDescriptor,
    tx_buffers: Vec<u64>,
    tx_cur: usize,
}

impl IntelNetDevice {
    pub unsafe fn read_reg(&self, offset: u32) -> u32 {
        let ptr = (self.mmio_base + offset as u64) as *const u32;
        core::ptr::read_volatile(ptr)
    }

    pub unsafe fn write_reg(&self, offset: u32, val: u32) {
        let ptr = (self.mmio_base + offset as u64) as *mut u32;
        core::ptr::write_volatile(ptr, val);
    }

    pub unsafe fn reset(&self) {
        // Reset device
        let ctrl = self.read_reg(REG_CTRL);
        self.write_reg(REG_CTRL, ctrl | CTRL_RST);
        
        // Wait for reset to complete
        uefi::boot::stall(10000);
        
        // Force Link Up (Set Link Up / SLU bit)
        let ctrl = self.read_reg(REG_CTRL);
        self.write_reg(REG_CTRL, ctrl | CTRL_SLU);

        // Signal that an OS driver has taken ownership.
        let ctrl_ext = self.read_reg(REG_CTRL_EXT);
        self.write_reg(REG_CTRL_EXT, ctrl_ext | CTRL_EXT_DRV_LOAD);
    }

    pub unsafe fn is_link_up(&self) -> bool {
        let status = self.read_reg(REG_STATUS);
        (status & STATUS_LU) != 0
    }
}

fn parse_rx_length(desc: &IntelDescriptor) -> Option<usize> {
    // Advanced one-buffer RX write-back interpretation.
    // bytes[8..12] => status_error, bytes[12..14] => length
    let adv_status_error =
        (desc.length as u32) | ((desc.cso as u32) << 16) | ((desc.cmd as u32) << 24);
    let adv_len = (desc.status as usize) | ((desc.css as usize) << 8);
    let adv_valid = (adv_status_error & ADV_RX_STAT_DD) != 0
        && (adv_status_error & ADV_RX_STAT_EOP) != 0
        && (RX_MIN_FRAME_LEN..=RX_MAX_FRAME_LEN).contains(&adv_len);
    if adv_valid { Some(adv_len) } else { None }
}

pub static mut GLOBAL_INTEL_NET: Option<IntelNetDevice> = None;

pub fn init(device: PciDevice) {
    if device.vendor_id != VENDOR_INTEL { return; }

    println("Intel Net: Initializing Hardware...");

    unsafe {
        // Ensure MMIO + bus mastering are enabled for DMA/register access.
        let cmd = crate::pci::read_config(device.bus, device.slot, device.func, 0x04);
        crate::pci::write_config(device.bus, device.slot, device.func, 0x04, cmd | 0x0006);
    }

    let mmio = match unsafe { read_bar(device.bus, device.slot, device.func, 0) } {
        Some(m) => m,
        None => { println("Intel Net: Error - BAR0 failed."); return; }
    };

    let mut dev = unsafe {
        // Setup DMA Descriptors
        let rx_ring_phys = crate::memory::allocate_dma_page32().expect("RX Ring DMA failed");
        let tx_ring_phys = crate::memory::allocate_dma_page32().expect("TX Ring DMA failed");
        
        let rx_ring = rx_ring_phys as *mut IntelDescriptor;
        let tx_ring = tx_ring_phys as *mut IntelDescriptor;
        
        // Clear rings using volatile writes
        for i in 0..RING_SIZE {
            core::ptr::write_volatile(rx_ring.add(i), core::mem::zeroed());
            core::ptr::write_volatile(tx_ring.add(i), core::mem::zeroed());
        }

        IntelNetDevice {
            pci: device,
            mmio_base: mmio,
            mac_addr: [0; 6],
            rx_ring_phys,
            rx_ring,
            rx_buffers: Vec::with_capacity(RING_SIZE),
            rx_cur: 0,
            tx_ring_phys,
            tx_ring,
            tx_buffers: Vec::with_capacity(RING_SIZE),
            tx_cur: 0,
        }
    };

    unsafe {
        dev.reset();

        // Some platforms clear command bits across device reset; force MMIO + bus mastering again.
        let cmd_after_reset = crate::pci::read_config(device.bus, device.slot, device.func, 0x04);
        crate::pci::write_config(device.bus, device.slot, device.func, 0x04, cmd_after_reset | 0x0006);

        // Read MAC from hardware registers.
        let ral = dev.read_reg(REG_RAL);
        let rah = dev.read_reg(REG_RAH);
        dev.mac_addr[0] = (ral & 0xFF) as u8;
        dev.mac_addr[1] = ((ral >> 8) & 0xFF) as u8;
        dev.mac_addr[2] = ((ral >> 16) & 0xFF) as u8;
        dev.mac_addr[3] = ((ral >> 24) & 0xFF) as u8;
        dev.mac_addr[4] = (rah & 0xFF) as u8;
        dev.mac_addr[5] = ((rah >> 8) & 0xFF) as u8;

        let mac_invalid = dev.mac_addr.iter().all(|&b| b == 0) || dev.mac_addr.iter().all(|&b| b == 0xFF);
        if mac_invalid {
            // Deterministic local-admin MAC fallback if NIC didn't expose a valid one.
            dev.mac_addr = [0x02, 0x52, 0x45, device.bus, device.slot, device.func];
            println("Intel Net: Warning - invalid HW MAC, using fallback MAC.");
        }

        // Program MAC and mark address as valid (RAH.AV).
        let ral_prog = (dev.mac_addr[0] as u32)
            | ((dev.mac_addr[1] as u32) << 8)
            | ((dev.mac_addr[2] as u32) << 16)
            | ((dev.mac_addr[3] as u32) << 24);
        let rah_prog = (dev.mac_addr[4] as u32) | ((dev.mac_addr[5] as u32) << 8) | RAH_AV;
        dev.write_reg(REG_RAL, ral_prog);
        dev.write_reg(REG_RAH, rah_prog);

        // Populate RX buffers/descriptors before enabling RX queue.
        for i in 0..RING_SIZE {
            let buf_phys = crate::memory::allocate_dma_page32().expect("RX Buffer DMA failed");
            dev.rx_buffers.push(buf_phys);
            let desc = IntelDescriptor {
                addr: buf_phys,
                length: 0,
                cso: 0,
                cmd: 0,
                status: 0,
                css: 0,
                special: 0,
            };
            core::ptr::write_volatile(dev.rx_ring.add(i), desc);
        }

        // Init RX ring/registers.
        dev.write_reg(REG_RCTL, 0); // Disable
        dev.write_reg(REG_RDBAL, dev.rx_ring_phys as u32);
        dev.write_reg(REG_RDBAH, (dev.rx_ring_phys >> 32) as u32);
        dev.write_reg(REG_RDLEN, (RING_SIZE * 16) as u32);
        // Use one-buffer advanced RX descriptors with 2KiB packet buffer.
        dev.write_reg(
            REG_SRRCTL,
            SRRCTL_DESCTYPE_ADV_ONEBUF | ((2048u32 >> SRRCTL_BSIZEPKT_SHIFT) & 0x7F),
        );
        dev.write_reg(REG_RDH, 0);
        dev.write_reg(REG_RDT, 0);

        let rxdctl = dev.read_reg(REG_RXDCTL);
        dev.write_reg(REG_RXDCTL, rxdctl | RXDCTL_ENABLE);
        let mut rx_wait = 0;
        while rx_wait < 100 && (dev.read_reg(REG_RXDCTL) & RXDCTL_ENABLE) == 0 {
            uefi::boot::stall(1000);
            rx_wait += 1;
        }

        // Enable RX: EN | MPE | BAM | SECRC.
        dev.write_reg(REG_RCTL, RCTL_EN | RCTL_MPE | RCTL_BAM | RCTL_SECRC);
        let rxctrl = dev.read_reg(REG_RXCTRL);
        dev.write_reg(REG_RXCTRL, rxctrl | RXCTRL_RXEN);
        // Advertise the full RX ring after queue/rx path is enabled.
        dev.write_reg(REG_RDT, (RING_SIZE - 1) as u32);
        // Clear TX descriptors.
        for i in 0..RING_SIZE {
            core::ptr::write_volatile(dev.tx_ring.add(i), core::mem::zeroed());
        }

        // Init TX ring/registers.
        dev.write_reg(REG_TCTL, 0); // Disable
        dev.write_reg(REG_TDBAL, dev.tx_ring_phys as u32);
        dev.write_reg(REG_TDBAH, (dev.tx_ring_phys >> 32) as u32);
        dev.write_reg(REG_TDLEN, (RING_SIZE * 16) as u32);
        dev.write_reg(REG_TDH, 0);
        dev.write_reg(REG_TDT, 0);

        let txdctl = dev.read_reg(REG_TXDCTL);
        dev.write_reg(REG_TXDCTL, txdctl | TXDCTL_ENABLE);
        let mut tx_wait = 0;
        while tx_wait < 100 && (dev.read_reg(REG_TXDCTL) & TXDCTL_ENABLE) == 0 {
            uefi::boot::stall(1000);
            tx_wait += 1;
        }

        // Typical legacy defaults used by Intel sample drivers.
        dev.write_reg(REG_TIPG, 10 | (8 << 10) | (12 << 20));

        // Disable all interrupts
        dev.write_reg(REG_IMC, 0xFFFF_FFFF);
        let _ = dev.read_reg(REG_ICR); // Clear any pending causes.

        // Enable TX with collision defaults (CT/COLD).
        dev.write_reg(REG_TCTL, TCTL_EN | TCTL_PSP | TCTL_CT | TCTL_COLD);

        GLOBAL_INTEL_NET = Some(dev);

        // Wait a bit for link
        let mut timeout = 0;
        while timeout < 100 {
            if let Some(ref d) = GLOBAL_INTEL_NET {
                if d.is_link_up() {
                    println("Intel Net: Link is UP.");
                    break;
                }
            }
            uefi::boot::stall(10000);
            timeout += 1;
        }
    }

    println("Intel Net: Ready.");
}

pub fn is_link_up() -> bool {
    unsafe {
        GLOBAL_INTEL_NET.as_ref().map(|d| d.is_link_up()).unwrap_or(false)
    }
}

#[derive(Clone, Copy)]
pub struct IntelNetDiag {
    pub pci_cmd: u32,
    pub status: u32,
    pub ctrl: u32,
    pub ctrl_ext: u32,
    pub rxctrl: u32,
    pub rctl: u32,
    pub rxdctl: u32,
    pub rdh: u32,
    pub rdt: u32,
    pub rdlen: u32,
    pub tctl: u32,
    pub txdctl: u32,
    pub tdh: u32,
    pub tdt: u32,
    pub tdlen: u32,
    pub ims: u32,
    pub imc: u32,
    pub srrctl: u32,
    pub gprc: u32,
    pub gptc: u32,
    pub rx_cur: usize,
    pub tx_cur: usize,
    pub rx_desc_addr: u64,
    pub rx_desc_length: u16,
    pub rx_desc_status: u8,
    pub rx_desc_cso: u8,
    pub rx_desc_cmd: u8,
    pub rx_desc_css: u8,
    pub rx_desc_special: u16,
}

pub fn get_diagnostics() -> Option<IntelNetDiag> {
    unsafe {
        GLOBAL_INTEL_NET.as_ref().map(|dev| {
            let rx_desc = core::ptr::read_volatile(dev.rx_ring.add(dev.rx_cur));
            IntelNetDiag {
                pci_cmd: crate::pci::read_config(dev.pci.bus, dev.pci.slot, dev.pci.func, 0x04),
                status: dev.read_reg(REG_STATUS),
                ctrl: dev.read_reg(REG_CTRL),
                ctrl_ext: dev.read_reg(REG_CTRL_EXT),
                rxctrl: dev.read_reg(REG_RXCTRL),
                rctl: dev.read_reg(REG_RCTL),
                rxdctl: dev.read_reg(REG_RXDCTL),
                rdh: dev.read_reg(REG_RDH),
                rdt: dev.read_reg(REG_RDT),
                rdlen: dev.read_reg(REG_RDLEN),
                tctl: dev.read_reg(REG_TCTL),
                txdctl: dev.read_reg(REG_TXDCTL),
                tdh: dev.read_reg(REG_TDH),
                tdt: dev.read_reg(REG_TDT),
                tdlen: dev.read_reg(REG_TDLEN),
                ims: dev.read_reg(REG_IMS),
                imc: dev.read_reg(REG_IMC),
                srrctl: dev.read_reg(REG_SRRCTL),
                gprc: dev.read_reg(REG_GPRC),
                gptc: dev.read_reg(REG_GPTC),
                rx_cur: dev.rx_cur,
                tx_cur: dev.tx_cur,
                rx_desc_addr: rx_desc.addr,
                rx_desc_length: rx_desc.length,
                rx_desc_status: rx_desc.status,
                rx_desc_cso: rx_desc.cso,
                rx_desc_cmd: rx_desc.cmd,
                rx_desc_css: rx_desc.css,
                rx_desc_special: rx_desc.special,
            }
        })
    }
}

pub struct IntelPhy;

impl Device for IntelPhy {
    type RxToken<'a> = IntelRxToken where Self: 'a;
    type TxToken<'a> = IntelTxToken<'a> where Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        unsafe {
            if let Some(ref mut dev) = GLOBAL_INTEL_NET {
                let desc = core::ptr::read_volatile(dev.rx_ring.add(dev.rx_cur));
                if let Some(len) = parse_rx_length(&desc) {
                    let buf_phys = dev.rx_buffers[dev.rx_cur];

                    let new_desc = IntelDescriptor {
                        addr: buf_phys,
                        length: 0,
                        cso: 0,
                        cmd: 0,
                        status: 0,
                        css: 0,
                        special: 0,
                    };
                    core::ptr::write_volatile(dev.rx_ring.add(dev.rx_cur), new_desc);
                    
                    dev.write_reg(REG_RDT, dev.rx_cur as u32);
                    dev.rx_cur = (dev.rx_cur + 1) % RING_SIZE;

                    let mut data = alloc::vec![0u8; len];
                    core::ptr::copy_nonoverlapping(buf_phys as *const u8, data.as_mut_ptr(), len);

                    RX_COUNT += 1;

                    let rx = IntelRxToken(data);
                    let tx = IntelTxToken { dev };
                    return Some((rx, tx));
                }
            }
        }
        None
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        unsafe {
            if let Some(ref mut dev) = GLOBAL_INTEL_NET {
                return Some(IntelTxToken { dev });
            }
        }
        None
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1500;
        caps.medium = Medium::Ethernet;
        caps
    }
}

pub struct IntelRxToken(Vec<u8>);
impl RxToken for IntelRxToken {
    fn consume<R, F>(self, f: F) -> R where F: FnOnce(&mut [u8]) -> R {
        let mut buffer = self.0;
        f(&mut buffer)
    }
}

pub struct IntelTxToken<'a> {
    dev: &'a mut IntelNetDevice,
}

impl<'a> TxToken for IntelTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R where F: FnOnce(&mut [u8]) -> R {
        let mut buffer = alloc::vec![0u8; len];
        let result = f(&mut buffer);
        unsafe {
            let dev = self.dev;
            let td_phys = crate::memory::allocate_dma_page32().expect("Temp TX DMA failed");
            core::ptr::copy_nonoverlapping(buffer.as_ptr(), td_phys as *mut u8, len);
            
            let cur = dev.tx_cur;
            let next_tdt = (dev.tx_cur + 1) % RING_SIZE;
            // Intel TX Descriptor layout
            // cmd: EOP(0) | IFCS(1) | RS(3)
            let desc = IntelDescriptor {
                addr: td_phys,
                length: len as u16,
                cso: 0,
                cmd: TX_CMD_EOP | TX_CMD_IFCS | TX_CMD_RS,
                status: 0,
                css: 0,
                special: 0,
            };
            
            core::ptr::write_volatile(dev.tx_ring.add(cur), desc);
            
            dev.tx_cur = next_tdt;
            dev.write_reg(REG_TDT, next_tdt as u32);
            TX_COUNT += 1;

            // Wait for RS (Report Status)
        }
        result
    }
}

pub fn get_model_name() -> Option<&'static str> {
    unsafe {
        GLOBAL_INTEL_NET.as_ref().map(|dev| {
            match dev.pci.device_id {
                DEVICE_I226_V => "Intel I226-V (2.5GbE)",
                DEVICE_I226_LM => "Intel I226-LM (2.5GbE)",
                DEVICE_I225_V => "Intel I225-V (2.5GbE)",
                DEVICE_I225_LM => "Intel I225-LM (2.5GbE)",
                _ => "Intel Ethernet",
            }
        })
    }
}

pub fn get_mac_address() -> Option<[u8; 6]> {
    unsafe { GLOBAL_INTEL_NET.as_ref().map(|d| d.mac_addr) }
}
