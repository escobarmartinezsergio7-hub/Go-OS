use crate::pci::{PciDevice, read_bar};
use crate::println;
use crate::memory;

// NVMe Register Offsets
const REG_CAP: usize = 0x00;     // Controller Capabilities
const REG_CC: usize = 0x14;      // Controller Configuration  
const REG_CSTS: usize = 0x1C;    // Controller Status
const REG_AQA: usize = 0x24;     // Admin Queue Attributes
const REG_ASQ: usize = 0x28;     // Admin Submission Queue Base
const REG_ACQ: usize = 0x30;     // Admin Completion Queue Base

// Doorbell offsets (after CAP.DSTRD calculation)
const REG_DOORBELL_BASE: usize = 0x1000;

// NVMe Command Opcodes
const NVME_CMD_CREATE_IO_CQ: u8 = 0x05;
const NVME_CMD_CREATE_IO_SQ: u8 = 0x01;
const NVME_CMD_READ: u8 = 0x02;

// Controller Configuration bits
const CC_EN: u32 = 1 << 0;
const CC_IOSQES: u32 = 6 << 16;  // I/O Submission Queue Entry Size (2^6 = 64 bytes)
const CC_IOCQES: u32 = 4 << 20;  // I/O Completion Queue Entry Size (2^4 = 16 bytes)

// Controller Status bits
const CSTS_RDY: u32 = 1 << 0;

#[repr(C)]
#[derive(Copy, Clone)]
struct NvmeCommand {
    opcode: u8,
    flags: u8,
    command_id: u16,
    nsid: u32,
    _rsvd: [u32; 2],
    metadata: u64,
    prp1: u64,
    prp2: u64,
    cdw10: u32,
    cdw11: u32,
    cdw12: u32,
    cdw13: u32,
    cdw14: u32,
    cdw15: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct NvmeCompletion {
    result: u32,
    _rsvd: u32,
    sq_head: u16,
    sq_id: u16,
    command_id: u16,
    status: u16,
}

static mut NVME_CONTROLLER: Option<NvmeController> = None;

struct NvmeController {
    mmio_base: u64,
    admin_sq: *mut NvmeCommand,
    admin_cq: *mut NvmeCompletion,
    io_sq: *mut NvmeCommand,
    io_cq: *mut NvmeCompletion,
    admin_sq_tail: u16,
    io_sq_tail: u16,
    data_buffer: *mut u8,
}

impl NvmeController {
    unsafe fn read_reg(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.mmio_base + offset as u64) as *const u32)
    }

    unsafe fn write_reg(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.mmio_base + offset as u64) as *mut u32, value);
    }

    unsafe fn wait_ready(&self, ready: bool, timeout_ms: u32) -> bool {
        for _ in 0..(timeout_ms * 10) {
            let csts = self.read_reg(REG_CSTS);
            if ((csts & CSTS_RDY) != 0) == ready {
                return true;
            }
            uefi::boot::stall(100); // 100µs
        }
        false
    }

    unsafe fn submit_admin_cmd(&mut self, cmd: NvmeCommand) -> bool {
        // Write command to submission queue
        core::ptr::write_volatile(self.admin_sq.add(self.admin_sq_tail as usize), cmd);
        
        // Ring doorbell
        let old_tail = self.admin_sq_tail;
        self.admin_sq_tail = (self.admin_sq_tail + 1) % 64;
        self.write_reg(REG_DOORBELL_BASE, self.admin_sq_tail as u32);
        
        // Wait for completion (polling)
        for _ in 0..1000 {
            let cqe = core::ptr::read_volatile(self.admin_cq.add(old_tail as usize));
            if cqe.command_id == cmd.command_id {
                if (cqe.status & 0xFE) != 0 {
                    return false; // Error
                }
                return true; // Success
            }
            uefi::boot::stall(100);
        }
        false
    }

    unsafe fn submit_io_read(&mut self, lba: u64, buffer: *mut u8) -> bool {
        let mut cmd: NvmeCommand = core::mem::zeroed();
        cmd.opcode = NVME_CMD_READ;
        cmd.nsid = 1; // Namespace 1
        cmd.command_id = self.io_sq_tail;
        cmd.prp1 = buffer as u64;
        cmd.cdw10 = (lba & 0xFFFFFFFF) as u32;
        cmd.cdw11 = (lba >> 32) as u32;
        cmd.cdw12 = 0; // Read 1 block (512 bytes)

        // Write command to I/O submission queue
        core::ptr::write_volatile(self.io_sq.add(self.io_sq_tail as usize), cmd);
        
        // Ring I/O SQ doorbell (offset 0x1000 + (2 * qid * doorbell_stride))
        let old_tail = self.io_sq_tail;
        self.io_sq_tail = (self.io_sq_tail + 1) % 64;
        self.write_reg(REG_DOORBELL_BASE + 8, self.io_sq_tail as u32);
        
        // Wait for completion
        for _ in 0..1000 {
            let cqe = core::ptr::read_volatile(self.io_cq.add(old_tail as usize));
            if cqe.command_id == cmd.command_id {
                // Ring I/O CQ doorbell
                self.write_reg(REG_DOORBELL_BASE + 12, (old_tail + 1) as u32);
                return (cqe.status & 0xFE) == 0;
            }
            uefi::boot::stall(100);
        }
        false
    }
}

pub fn init(device: PciDevice) {
    unsafe {
        let bar0 = read_bar(device.bus, device.slot, device.func, 0);
        
        if let Some(addr) = bar0 {
            println("NVMe: Initializing controller...");
            
            // Allocate queue memory
            let admin_sq = memory::allocate_dma_page().unwrap() as *mut NvmeCommand;
            let admin_cq = memory::allocate_dma_page().unwrap() as *mut NvmeCompletion;
            let io_sq = memory::allocate_dma_page().unwrap() as *mut NvmeCommand;
            let io_cq = memory::allocate_dma_page().unwrap() as *mut NvmeCompletion;
            let data_buffer = memory::allocate_dma_page().unwrap() as *mut u8;

            let mut ctrl = NvmeController {
                mmio_base: addr,
                admin_sq,
                admin_cq,
                io_sq,
                io_cq,
                admin_sq_tail: 0,
                io_sq_tail: 0,
                data_buffer,
            };

            // 1. Disable controller
            ctrl.write_reg(REG_CC, 0);
            if !ctrl.wait_ready(false, 5000) {
                println("NVMe: Controller disable timeout");
                return;
            }

            // 2. Configure admin queues
            ctrl.write_reg(REG_AQA, 0x003F003F); // 64 entries each
            ctrl.write_reg(REG_ASQ, (admin_sq as u64 & 0xFFFFFFFF) as u32);
            ctrl.write_reg(REG_ASQ + 4, ((admin_sq as u64) >> 32) as u32);
            ctrl.write_reg(REG_ACQ, (admin_cq as u64 & 0xFFFFFFFF) as u32);
            ctrl.write_reg(REG_ACQ + 4, ((admin_cq as u64) >> 32) as u32);

            // 3. Enable controller
            ctrl.write_reg(REG_CC, CC_EN | CC_IOSQES | CC_IOCQES);
            if !ctrl.wait_ready(true, 5000) {
                println("NVMe: Controller enable timeout");
                return;
            }

            // 4. Create I/O Completion Queue
            let mut cmd: NvmeCommand = core::mem::zeroed();
            cmd.opcode = NVME_CMD_CREATE_IO_CQ;
            cmd.command_id = 1;
            cmd.prp1 = io_cq as u64;
            cmd.cdw10 = (1 << 16) | 63; // QID=1, Size=64
            cmd.cdw11 = 1; // Physically contiguous

            if !ctrl.submit_admin_cmd(cmd) {
                println("NVMe: Failed to create I/O CQ");
                return;
            }

            // 5. Create I/O Submission Queue
            cmd = core::mem::zeroed();
            cmd.opcode = NVME_CMD_CREATE_IO_SQ;
            cmd.command_id = 2;
            cmd.prp1 = io_sq as u64;
            cmd.cdw10 = (1 << 16) | 63; // QID=1, Size=64
            cmd.cdw11 = (1 << 16) | 1; // CQID=1, Physically contiguous

            if !ctrl.submit_admin_cmd(cmd) {
                println("NVMe: Failed to create I/O SQ");
                return;
            }

            NVME_CONTROLLER = Some(ctrl);
            println("NVMe: Initialized successfully");
        } else {
            println("NVMe: Failed to find BAR0.");
        }
    }
}

pub fn read(lba: u64, buffer: &mut [u8]) -> bool {
    unsafe {
        if let Some(ctrl) = &mut NVME_CONTROLLER {
            if ctrl.submit_io_read(lba, ctrl.data_buffer) {
                core::ptr::copy_nonoverlapping(ctrl.data_buffer, buffer.as_mut_ptr(), 512);
                return true;
            }
        }
        false
    }
}
