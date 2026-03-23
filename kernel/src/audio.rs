// Intel High Definition Audio (HDA) Controller Driver
// Supports PCM playback via codec DAC → Pin Widget output path.

use crate::pci::{PciDevice, read_bar, enable_bus_master};
use crate::println;
use alloc::vec::Vec;
use alloc::string::String;

// ─── Init Log (visible from GUI) ─────────────────────────────────────────────
static mut INIT_LOG: Option<String> = None;

fn hda_log(msg: &str) {
    println(msg);
    unsafe {
        let log = INIT_LOG.get_or_insert_with(String::new);
        if !log.is_empty() {
            log.push('\n');
        }
        log.push_str(msg);
    }
}

/// Return the full HDA init log for display in the GUI.
pub fn init_log() -> String {
    unsafe { INIT_LOG.clone().unwrap_or_default() }
}

// ─── HDA Controller MMIO Registers ───────────────────────────────────────────
const REG_GCAP: u32     = 0x00; // Global Capabilities
const REG_VMIN: u32     = 0x02; // Minor Version
const REG_VMAJ: u32     = 0x03; // Major Version
const REG_GCTL: u32     = 0x08; // Global Control
const REG_WAKEEN: u32   = 0x0C; // Wake Enable
const REG_STATESTS: u32 = 0x0E; // State Change Status
const REG_INTCTL: u32   = 0x20; // Interrupt Control
const REG_INTSTS: u32   = 0x24; // Interrupt Status

// CORB registers
const REG_CORBLBASE: u32 = 0x40;
const REG_CORBUBASE: u32 = 0x44;
const REG_CORBWP: u32    = 0x48; // Write Pointer
const REG_CORBRP: u32    = 0x4A; // Read Pointer
const REG_CORBCTL: u32   = 0x4C;
const REG_CORBSIZE: u32  = 0x4E;

// RIRB registers
const REG_RIRBLBASE: u32 = 0x50;
const REG_RIRBUBASE: u32 = 0x54;
const REG_RIRBWP: u32    = 0x58;
const REG_RINTCNT: u32   = 0x5A;
const REG_RIRBCTL: u32   = 0x5C;
const REG_RIRBSIZE: u32  = 0x5E;
const REG_RIRBSTS: u32   = 0x5D;

// Stream Descriptor base (output stream 0)
// Input streams start at 0x80, output streams after them.
// GCAP tells us how many input streams there are.
const SD_BASE_OFFSET: u32 = 0x80; // we compute dynamically
const SD_SIZE: u32 = 0x20; // each stream descriptor is 0x20 bytes

// Stream descriptor register offsets (relative to SD base)
const SD_CTL:   u32 = 0x00; // 3 bytes: Control
const SD_STS:   u32 = 0x03; // 1 byte: Status
const SD_LPIB:  u32 = 0x04; // Link Position in Buffer
const SD_CBL:   u32 = 0x08; // Cyclic Buffer Length
const SD_LVI:   u32 = 0x0C; // Last Valid Index (u16)
const SD_FMT:   u32 = 0x12; // Format (u16)
const SD_BDPL:  u32 = 0x18; // Buffer Descriptor List Pointer (Low)
const SD_BDPU:  u32 = 0x1C; // Buffer Descriptor List Pointer (High)

// GCTL bits
const GCTL_CRST: u32 = 1 << 0; // Controller Reset

// CORB/RIRB control bits
const CORBCTL_RUN: u8 = 1 << 1;
const RIRBCTL_RUN: u8 = 1 << 1;

// Stream control bits
const SD_CTL_RUN:   u32 = 1 << 0;
const SD_CTL_IOCE:  u32 = 1 << 2;
const SD_CTL_STRIPE: u32 = 0; // no stripe
const SD_CTL_STREAM_SHIFT: u32 = 20; // stream number in bits [23:20]

// HDA Verb helpers
const VERB_GET_PARAM: u32        = 0xF00_00;
const VERB_SET_STREAM_FMT: u32   = 0x200_00;
const VERB_SET_CHAN_STREAM: u32   = 0x706_00;
const VERB_SET_PIN_WIDGET: u32   = 0x707_00;
const VERB_SET_EAPD: u32         = 0x70C_00;
const VERB_SET_AMP_GAIN: u32     = 0x300_00;
const VERB_SET_POWER_STATE: u32  = 0x705_00;
const VERB_SET_CONVERTER_FMT: u32= 0x200_00;
const VERB_GET_CONN_LIST: u32    = 0xF02_00;

// Parameter IDs
const PARAM_VENDOR_ID: u32    = 0x00;
const PARAM_NODE_COUNT: u32   = 0x04;
const PARAM_FN_GROUP_TYPE: u32= 0x05;
const PARAM_AUDIO_WIDGET_CAP: u32 = 0x09;
const PARAM_CONN_LIST_LEN: u32= 0x0E;
const PARAM_OUT_AMP_CAP: u32  = 0x12;

// Widget types (from Audio Widget Capabilities parameter bits [23:20])
const WIDGET_TYPE_OUTPUT: u32 = 0x0; // Audio Output (DAC)
const WIDGET_TYPE_INPUT: u32  = 0x1; // Audio Input (ADC)
const WIDGET_TYPE_MIXER: u32  = 0x2;
const WIDGET_TYPE_SELECTOR: u32 = 0x3;
const WIDGET_TYPE_PIN: u32    = 0x4; // Pin Complex
const WIDGET_TYPE_POWER: u32  = 0x5;
const WIDGET_TYPE_BEEP: u32   = 0x7;

// Pin default config: default device field bits [23:20]
const PIN_DEV_LINE_OUT: u32   = 0x0;
const PIN_DEV_SPEAKER: u32    = 0x1;
const PIN_DEV_HP_OUT: u32     = 0x2;

// ─── BDL Entry ───────────────────────────────────────────────────────────────
#[derive(Copy, Clone)]
#[repr(C, align(128))]
struct BdlEntry {
    address: u64,
    length: u32,
    ioc: u32, // bit 0 = interrupt on completion
}

// ─── Static Buffers ──────────────────────────────────────────────────────────
// We use static buffers because we need stable physical addresses for DMA.
// In a UEFI environment, virtual == physical for identity-mapped memory.

const CORB_ENTRIES: usize = 256;
const RIRB_ENTRIES: usize = 256;
const BDL_ENTRIES: usize = 32;
const PCM_BUFFER_SAMPLES: usize = 48000 * 2 * 4; // ~4 seconds of 48kHz stereo 16-bit
const PCM_BUFFER_BYTES: usize = PCM_BUFFER_SAMPLES * 2; // 16-bit samples

#[repr(C, align(128))]
struct CorbBuffer([u32; CORB_ENTRIES]);

#[repr(C, align(128))]
struct RirbBuffer([[u32; 2]; RIRB_ENTRIES]); // each entry is response + response_ex

#[repr(C, align(128))]
struct BdlBuffer([BdlEntry; BDL_ENTRIES]);

#[repr(C, align(4096))]
struct PcmBuffer([u8; PCM_BUFFER_BYTES]);

static mut CORB_BUF: CorbBuffer = CorbBuffer([0u32; CORB_ENTRIES]);
static mut RIRB_BUF: RirbBuffer = RirbBuffer([[0u32; 2]; RIRB_ENTRIES]);
static mut BDL_BUF: BdlBuffer = BdlBuffer([BdlEntry { address: 0, length: 0, ioc: 0 }; BDL_ENTRIES]);
static mut PCM_BUF: PcmBuffer = PcmBuffer([0u8; PCM_BUFFER_BYTES]);

// ─── HDA Controller State ────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum HdaState {
    Uninitialized,
    Ready,
    Playing,
    Error,
}

pub struct HdaController {
    pub state: HdaState,
    mmio_base: u64,
    codec_id: u8,
    dac_nid: u16,
    pin_nid: u16,
    mixer_nid: u16,
    num_input_streams: u8,
    num_output_streams: u8,
    output_stream_index: u8, // which output stream descriptor to use
    corb_wp: u16,
    rirb_rp: u16,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    // Playback state
    pub pcm_total_bytes: usize,
    pub pcm_written: usize,
    pub volume: u8, // 0..127
}

pub static mut GLOBAL_HDA: HdaController = HdaController {
    state: HdaState::Uninitialized,
    mmio_base: 0,
    codec_id: 0,
    dac_nid: 0,
    pin_nid: 0,
    mixer_nid: 0,
    num_input_streams: 0,
    num_output_streams: 0,
    output_stream_index: 0,
    corb_wp: 0,
    rirb_rp: 0,
    sample_rate: 48000,
    channels: 2,
    bits_per_sample: 16,
    pcm_total_bytes: 0,
    pcm_written: 0,
    volume: 100,
};

// ─── MMIO Helpers ────────────────────────────────────────────────────────────

unsafe fn mmio_read32(base: u64, offset: u32) -> u32 {
    let ptr = (base + offset as u64) as *const u32;
    core::ptr::read_volatile(ptr)
}

unsafe fn mmio_write32(base: u64, offset: u32, value: u32) {
    let ptr = (base + offset as u64) as *mut u32;
    core::ptr::write_volatile(ptr, value);
}

unsafe fn mmio_read16(base: u64, offset: u32) -> u16 {
    let ptr = (base + offset as u64) as *const u16;
    core::ptr::read_volatile(ptr)
}

unsafe fn mmio_write16(base: u64, offset: u32, value: u16) {
    let ptr = (base + offset as u64) as *mut u16;
    core::ptr::write_volatile(ptr, value);
}

unsafe fn mmio_read8(base: u64, offset: u32) -> u8 {
    let ptr = (base + offset as u64) as *const u8;
    core::ptr::read_volatile(ptr)
}

unsafe fn mmio_write8(base: u64, offset: u32, value: u8) {
    let ptr = (base + offset as u64) as *mut u8;
    core::ptr::write_volatile(ptr, value);
}

// ─── Delay ───────────────────────────────────────────────────────────────────

fn delay_us(us: u64) {
    // Simple spin delay. In UEFI we have boot services stall, but
    // post-ExitBootServices we just spin. This is approximate.
    for _ in 0..us * 100 {
        core::hint::spin_loop();
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

pub fn init(device: PciDevice) {
    let bar0 = unsafe { read_bar(device.bus, device.slot, device.func, 0) };
    let base = match bar0 {
        Some(addr) if addr != 0 => addr,
        _ => {
            hda_log("HDA: Failed to find BAR0.");
            unsafe { GLOBAL_HDA.state = HdaState::Error; }
            return;
        }
    };

    hda_log(alloc::format!(
        "HDA: BAR0={:#010x} vendor={:#06x} device={:#06x}",
        base, device.vendor_id, device.device_id
    ).as_str());

    // Enable bus mastering for DMA
    unsafe { enable_bus_master(device.bus, device.slot, device.func); }

    // Also enable memory space access
    unsafe {
        let cmd = crate::pci::read_config(device.bus, device.slot, device.func, 0x04);
        if (cmd & 0x02) == 0 {
            crate::pci::write_config(device.bus, device.slot, device.func, 0x04, cmd | 0x06);
        }
    }

    let hda = unsafe { &mut GLOBAL_HDA };
    hda.mmio_base = base;

    // Step 1: Reset controller
    hda_log("HDA: Step 1 - Reset...");
    if !unsafe { hda_reset_controller(base) } {
        hda_log("HDA: FAIL - Reset timeout.");
        hda.state = HdaState::Error;
        return;
    }
    hda_log("HDA: Step 1 - OK");

    // Step 2: Read capabilities
    let gcap = unsafe { mmio_read32(base, REG_GCAP) };
    hda.num_input_streams = ((gcap >> 8) & 0x0F) as u8;
    hda.num_output_streams = ((gcap >> 12) & 0x0F) as u8;
    let _num_bss = ((gcap >> 3) & 0x1F) as u8;
    let _64bit_ok = (gcap & 0x01) != 0;

    hda_log(alloc::format!(
        "HDA: Step 2 - GCAP={:#010x} in={} out={} 64bit={}",
        gcap, hda.num_input_streams, hda.num_output_streams, _64bit_ok
    ).as_str());

    if hda.num_output_streams == 0 {
        hda_log("HDA: WARN - 0 output streams, trying 1");
        hda.num_output_streams = 1;
    }

    // Step 3: Setup CORB/RIRB
    hda_log("HDA: Step 3 - CORB/RIRB...");
    if !unsafe { hda_setup_corb_rirb(hda) } {
        hda_log("HDA: FAIL - CORB/RIRB setup.");
        hda.state = HdaState::Error;
        return;
    }
    hda_log("HDA: Step 3 - OK");

    // Step 4: Discover codecs
    let statests = unsafe { mmio_read16(base, REG_STATESTS) };
    hda_log(alloc::format!(
        "HDA: Step 4 - STATESTS={:#06x}", statests
    ).as_str());

    if statests == 0 {
        hda_log("HDA: FAIL - No codecs (STATESTS=0).");
        hda.state = HdaState::Error;
        return;
    }

    // Find first codec
    for i in 0..15u8 {
        if (statests & (1 << i)) != 0 {
            hda.codec_id = i;
            hda_log(alloc::format!("HDA: Codec {} found", i).as_str());
            break;
        }
    }

    // Step 5: Walk codec to find DAC and output pin
    hda_log("HDA: Step 5 - Finding output path...");
    if !unsafe { hda_discover_output_path(hda) } {
        hda_log("HDA: FAIL - No DAC/Pin path found.");
        hda.state = HdaState::Error;
        return;
    }

    hda_log(alloc::format!(
        "HDA: Step 5 - OK: DAC=nid{} PIN=nid{} MIX=nid{}",
        hda.dac_nid, hda.pin_nid, hda.mixer_nid
    ).as_str());

    // Step 6: Configure the output path
    hda_log("HDA: Step 6 - Configure output...");
    unsafe { hda_configure_output(hda); }

    hda.state = HdaState::Ready;
    hda_log("HDA: Driver ready.");
}

/// Play raw PCM samples. Data must be 16-bit signed, little-endian.
/// `sample_rate`: typically 44100 or 48000
/// `channels`: 1 (mono) or 2 (stereo)
pub fn play_pcm(data: &[u8], sample_rate: u32, channels: u16) {
    let hda = unsafe { &mut GLOBAL_HDA };
    if hda.state != HdaState::Ready && hda.state != HdaState::Playing {
        return;
    }

    hda.sample_rate = sample_rate;
    hda.channels = channels;
    hda.bits_per_sample = 16;
    hda.pcm_total_bytes = data.len();
    hda.pcm_written = 0;

    // Copy PCM data to DMA buffer
    let copy_len = data.len().min(PCM_BUFFER_BYTES);
    unsafe {
        PCM_BUF.0[..copy_len].copy_from_slice(&data[..copy_len]);
        // Zero rest
        if copy_len < PCM_BUFFER_BYTES {
            PCM_BUF.0[copy_len..].fill(0);
        }
    }
    hda.pcm_written = copy_len;

    // Setup stream and start playback
    unsafe { hda_start_playback(hda, copy_len); }
    hda.state = HdaState::Playing;
}

/// Stop playback.
pub fn stop() {
    let hda = unsafe { &mut GLOBAL_HDA };
    if hda.state == HdaState::Playing {
        unsafe { hda_stop_playback(hda); }
        hda.state = HdaState::Ready;
    }
}

/// Check if the driver is currently playing audio.
pub fn is_playing() -> bool {
    let hda = unsafe { &GLOBAL_HDA };
    hda.state == HdaState::Playing
}

/// Check if the driver is initialized and ready.
pub fn is_ready() -> bool {
    let hda = unsafe { &GLOBAL_HDA };
    hda.state == HdaState::Ready || hda.state == HdaState::Playing
}

/// Get current playback position in bytes (approximate from LPIB register).
pub fn playback_position() -> usize {
    let hda = unsafe { &GLOBAL_HDA };
    if hda.state != HdaState::Playing {
        return 0;
    }
    let sd_base = unsafe { hda_output_sd_base(hda) };
    let lpib = unsafe { mmio_read32(hda.mmio_base, sd_base + SD_LPIB) };
    lpib as usize
}

/// Return a status string for the HDA driver.
pub fn status_text() -> &'static str {
    let hda = unsafe { &GLOBAL_HDA };
    match hda.state {
        HdaState::Uninitialized => "No inicializado",
        HdaState::Ready => "Listo",
        HdaState::Playing => "Reproduciendo",
        HdaState::Error => "Error",
    }
}

// ─── Internal Implementation ─────────────────────────────────────────────────

unsafe fn hda_reset_controller(base: u64) -> bool {
    // Clear CRST to enter reset
    let gctl = mmio_read32(base, REG_GCTL);
    mmio_write32(base, REG_GCTL, gctl & !GCTL_CRST);

    // Wait for CRST to read 0
    for _ in 0..1000 {
        delay_us(100);
        if (mmio_read32(base, REG_GCTL) & GCTL_CRST) == 0 {
            break;
        }
    }

    delay_us(1000);

    // Set CRST to exit reset
    let gctl = mmio_read32(base, REG_GCTL);
    mmio_write32(base, REG_GCTL, gctl | GCTL_CRST);

    // Wait for CRST to read 1
    for _ in 0..1000 {
        delay_us(100);
        if (mmio_read32(base, REG_GCTL) & GCTL_CRST) != 0 {
            // Wait additional time for codecs to enumerate
            delay_us(5000);
            return true;
        }
    }

    false
}

unsafe fn hda_setup_corb_rirb(hda: &mut HdaController) -> bool {
    let base = hda.mmio_base;

    // ── CORB ──
    // Stop CORB
    mmio_write8(base, REG_CORBCTL, 0);
    delay_us(100);

    // Set CORB size to 256 entries (size register value = 2)
    mmio_write8(base, REG_CORBSIZE, 0x02);

    // Set CORB base address (we use identity-mapped addresses)
    let corb_phys = &CORB_BUF as *const CorbBuffer as u64;
    mmio_write32(base, REG_CORBLBASE, corb_phys as u32);
    mmio_write32(base, REG_CORBUBASE, (corb_phys >> 32) as u32);

    // Reset CORB read pointer
    mmio_write16(base, REG_CORBRP, 0x8000); // set CORBRPRST
    delay_us(100);
    mmio_write16(base, REG_CORBRP, 0x0000); // clear CORBRPRST
    delay_us(100);

    // Set CORB write pointer to 0
    mmio_write16(base, REG_CORBWP, 0);
    hda.corb_wp = 0;

    // Start CORB
    mmio_write8(base, REG_CORBCTL, CORBCTL_RUN);
    delay_us(100);

    // ── RIRB ──
    // Stop RIRB
    mmio_write8(base, REG_RIRBCTL, 0);
    delay_us(100);

    // Set RIRB size to 256
    mmio_write8(base, REG_RIRBSIZE, 0x02);

    // Set RIRB base address
    let rirb_phys = &RIRB_BUF as *const RirbBuffer as u64;
    mmio_write32(base, REG_RIRBLBASE, rirb_phys as u32);
    mmio_write32(base, REG_RIRBUBASE, (rirb_phys >> 32) as u32);

    // Reset RIRB write pointer
    mmio_write16(base, REG_RIRBWP, 0x8000); // set RIRBWPRST
    delay_us(100);
    hda.rirb_rp = 0;

    // Start RIRB
    mmio_write8(base, REG_RIRBCTL, RIRBCTL_RUN);
    delay_us(100);

    true
}

/// Send a codec command via CORB and wait for response via RIRB.
unsafe fn hda_send_verb(hda: &mut HdaController, nid: u16, verb: u32) -> u32 {
    let base = hda.mmio_base;
    let codec = hda.codec_id as u32;

    // Build the CORB entry: [codec(28:31)] [nid(20:27)] [verb(0:19)]
    let cmd = (codec << 28) | ((nid as u32) << 20) | (verb & 0xFFFFF);

    // Write to next CORB slot
    hda.corb_wp = (hda.corb_wp + 1) % (CORB_ENTRIES as u16);
    CORB_BUF.0[hda.corb_wp as usize] = cmd;

    // Update write pointer
    mmio_write16(base, REG_CORBWP, hda.corb_wp);

    // Wait for RIRB response
    for _ in 0..10000 {
        delay_us(10);
        let rirb_wp = mmio_read16(base, REG_RIRBWP);
        if rirb_wp != hda.rirb_rp {
            hda.rirb_rp = (hda.rirb_rp + 1) % (RIRB_ENTRIES as u16);
            let response = RIRB_BUF.0[hda.rirb_rp as usize][0];
            // Clear RIRB interrupt status
            mmio_write8(base, REG_RIRBSTS, 0x05);
            return response;
        }
    }

    // Timeout
    0
}

unsafe fn hda_discover_output_path(hda: &mut HdaController) -> bool {
    // Get root node to find AFG (Audio Function Group)
    let root_param = hda_send_verb(hda, 0, VERB_GET_PARAM | PARAM_NODE_COUNT);
    let start_nid = ((root_param >> 16) & 0xFF) as u16;
    let total_nodes = (root_param & 0xFF) as u16;

    if total_nodes == 0 {
        return false;
    }

    // Find AFG node
    let mut afg_nid: u16 = 0;
    for nid in start_nid..(start_nid + total_nodes) {
        let fg_type = hda_send_verb(hda, nid, VERB_GET_PARAM | PARAM_FN_GROUP_TYPE);
        if (fg_type & 0xFF) == 0x01 {
            // Audio Function Group
            afg_nid = nid;

            // Power on the AFG
            hda_send_verb(hda, nid, VERB_SET_POWER_STATE | 0x00); // D0
            delay_us(1000);
            break;
        }
    }

    if afg_nid == 0 {
        return false;
    }

    // Enumerate widgets in AFG
    let afg_param = hda_send_verb(hda, afg_nid, VERB_GET_PARAM | PARAM_NODE_COUNT);
    let widget_start = ((afg_param >> 16) & 0xFF) as u16;
    let widget_count = (afg_param & 0xFF) as u16;

    let mut dac_nid: u16 = 0;
    let mut pin_nid: u16 = 0;
    let mut mixer_nid: u16 = 0;
    let mut dac_candidates: Vec<u16> = Vec::new();
    let mut pin_candidates: Vec<u16> = Vec::new();
    let mut mixer_candidates: Vec<u16> = Vec::new();

    for nid in widget_start..(widget_start + widget_count) {
        let wcap = hda_send_verb(hda, nid, VERB_GET_PARAM | PARAM_AUDIO_WIDGET_CAP);
        let wtype = (wcap >> 20) & 0xF;

        match wtype {
            WIDGET_TYPE_OUTPUT => {
                dac_candidates.push(nid);
            }
            WIDGET_TYPE_PIN => {
                // Check pin default configuration for output capability
                let pin_cfg = hda_send_verb(hda, nid, 0xF1C_00); // GET_CONFIG_DEFAULT
                let default_device = (pin_cfg >> 20) & 0xF;
                // Accept line out, speaker, HP out
                if default_device == PIN_DEV_LINE_OUT
                    || default_device == PIN_DEV_SPEAKER
                    || default_device == PIN_DEV_HP_OUT
                {
                    pin_candidates.push(nid);
                }
            }
            WIDGET_TYPE_MIXER => {
                mixer_candidates.push(nid);
            }
            _ => {}
        }
    }

    // Pick first DAC and first output pin
    if let Some(&d) = dac_candidates.first() {
        dac_nid = d;
    }
    if let Some(&p) = pin_candidates.first() {
        pin_nid = p;
    }
    if let Some(&m) = mixer_candidates.first() {
        mixer_nid = m;
    }

    if dac_nid == 0 || pin_nid == 0 {
        return false;
    }

    hda.dac_nid = dac_nid;
    hda.pin_nid = pin_nid;
    hda.mixer_nid = mixer_nid;

    true
}

unsafe fn hda_configure_output(hda: &mut HdaController) {
    // Power on DAC
    hda_send_verb(hda, hda.dac_nid, VERB_SET_POWER_STATE | 0x00);
    delay_us(500);

    // Power on Pin
    hda_send_verb(hda, hda.pin_nid, VERB_SET_POWER_STATE | 0x00);
    delay_us(500);

    // Power on mixer if present
    if hda.mixer_nid != 0 {
        hda_send_verb(hda, hda.mixer_nid, VERB_SET_POWER_STATE | 0x00);
        delay_us(500);
    }

    // Enable pin output
    // Pin Widget Control: OUT enable (bit 6)
    hda_send_verb(hda, hda.pin_nid, VERB_SET_PIN_WIDGET | 0x40);
    delay_us(100);

    // Enable EAPD if supported (some codecs need this for speakers)
    hda_send_verb(hda, hda.pin_nid, VERB_SET_EAPD | 0x02);
    delay_us(100);

    // Set output amplifier gain on Pin to max (0dB)
    // Verb 0x3: SET_AMP_GAIN_MUTE
    // Bits: [15] output, [14:13] left+right, [12] not muted, [6:0] gain
    hda_send_verb(hda, hda.pin_nid, 0x3B0_7F); // output, L+R, unmute, max gain
    delay_us(100);

    // Set DAC output amp to max
    hda_send_verb(hda, hda.dac_nid, 0x3B0_7F);
    delay_us(100);

    // Set mixer amp if present
    if hda.mixer_nid != 0 {
        hda_send_verb(hda, hda.mixer_nid, 0x3B0_7F);
        delay_us(100);
    }
}

/// Compute the stream format register value for the given parameters.
fn hda_stream_format(sample_rate: u32, channels: u16, bits: u16) -> u16 {
    // FMT register layout:
    // [15]    = 0 (PCM)
    // [14]    = Base rate: 0 = 48kHz, 1 = 44.1kHz
    // [13:11] = Sample rate multiplier (0=x1, 1=x2, 2=x3, 3=x4)
    // [10:8]  = Sample rate divisor (0=/1, 1=/2, ..., 7=/8)
    // [7:4]   = Bits per sample: 000=8, 001=16, 010=20, 011=24, 100=32
    // [3:0]   = Number of channels minus 1

    let base = if sample_rate == 44100 { 1u16 << 14 } else { 0u16 };

    let bits_field: u16 = match bits {
        8 => 0,
        16 => 1,
        20 => 2,
        24 => 3,
        32 => 4,
        _ => 1, // default 16-bit
    };

    let channels_field = (channels.saturating_sub(1)) & 0x0F;

    base | (bits_field << 4) | channels_field
}

unsafe fn hda_output_sd_base(hda: &HdaController) -> u32 {
    // Output stream descriptors start after input streams
    let input_count = hda.num_input_streams as u32;
    SD_BASE_OFFSET + (input_count + hda.output_stream_index as u32) * SD_SIZE
}

unsafe fn hda_start_playback(hda: &mut HdaController, data_len: usize) {
    let base = hda.mmio_base;
    let sd_base = hda_output_sd_base(hda);

    // Stop stream first
    let ctl = mmio_read32(base, sd_base + SD_CTL) & 0x00FFFFFF;
    mmio_write32(base, sd_base + SD_CTL, ctl & !SD_CTL_RUN);
    delay_us(100);

    // Reset stream
    mmio_write32(base, sd_base + SD_CTL, 0x01); // SRST
    delay_us(100);
    for _ in 0..1000 {
        if (mmio_read32(base, sd_base + SD_CTL) & 0x01) != 0 {
            break;
        }
        delay_us(10);
    }
    mmio_write32(base, sd_base + SD_CTL, 0x00); // clear SRST
    delay_us(100);
    for _ in 0..1000 {
        if (mmio_read32(base, sd_base + SD_CTL) & 0x01) == 0 {
            break;
        }
        delay_us(10);
    }

    // Clear status bits
    mmio_write8(base, sd_base + SD_STS, 0x1C);

    // Setup BDL — single entry pointing to entire PCM buffer
    let pcm_phys = &PCM_BUF as *const PcmBuffer as u64;
    let bdl_entries_used = 1usize;

    BDL_BUF.0[0] = BdlEntry {
        address: pcm_phys,
        length: data_len as u32,
        ioc: 1, // interrupt on completion
    };

    // Set BDL address
    let bdl_phys = &BDL_BUF as *const BdlBuffer as u64;
    mmio_write32(base, sd_base + SD_BDPL, bdl_phys as u32);
    mmio_write32(base, sd_base + SD_BDPU, (bdl_phys >> 32) as u32);

    // Set Cyclic Buffer Length
    mmio_write32(base, sd_base + SD_CBL, data_len as u32);

    // Set Last Valid Index
    mmio_write16(base, sd_base + SD_LVI, (bdl_entries_used - 1) as u16);

    // Set stream format
    let fmt = hda_stream_format(hda.sample_rate, hda.channels, hda.bits_per_sample);
    mmio_write16(base, sd_base + SD_FMT, fmt);

    // Configure DAC to use stream tag 1
    let stream_tag: u32 = 1;
    // Set converter stream/channel: verb 0x706, payload = (stream_tag << 4) | channel
    hda_send_verb(hda, hda.dac_nid, VERB_SET_CHAN_STREAM | ((stream_tag << 4) & 0xFF));
    delay_us(100);

    // Set converter format on the DAC widget too
    hda_send_verb(hda, hda.dac_nid, VERB_SET_CONVERTER_FMT | (fmt as u32));
    delay_us(100);

    // Start the stream: set RUN bit, stream tag in bits [23:20]
    let ctl_val = SD_CTL_RUN | SD_CTL_IOCE | (stream_tag << SD_CTL_STREAM_SHIFT);
    mmio_write32(base, sd_base + SD_CTL, ctl_val & 0x00FFFFFF);

    // Enable global interrupt control
    let intctl = mmio_read32(base, REG_INTCTL);
    let stream_index = hda.num_input_streams as u32 + hda.output_stream_index as u32;
    mmio_write32(base, REG_INTCTL, intctl | (1 << 31) | (1 << stream_index));
}

unsafe fn hda_stop_playback(hda: &mut HdaController) {
    let base = hda.mmio_base;
    let sd_base = hda_output_sd_base(hda);

    // Clear RUN bit
    let ctl = mmio_read32(base, sd_base + SD_CTL) & 0x00FFFFFF;
    mmio_write32(base, sd_base + SD_CTL, ctl & !SD_CTL_RUN);
    delay_us(100);

    // Clear status
    mmio_write8(base, sd_base + SD_STS, 0x1C);

    // Silence: reset DAC stream/channel
    hda_send_verb(hda, hda.dac_nid, VERB_SET_CHAN_STREAM | 0x00);
}
