use crate::pci::{read_bar, read_config, PciDevice};
use crate::println;

const VENDOR_INTEL: u16 = 0x8086;

// Known Intel Wi-Fi PCI device IDs extracted from Netwtw6e.INF
const DEVICE_AX210: u16 = 0x2725;
const DEVICE_BE200_BE202: u16 = 0x272B;
const DEVICE_AX211_BE201_FAMILY: u16 = 0xA840;
const DEVICE_AX211_FAMILY_1: u16 = 0x51F0;
const DEVICE_AX211_FAMILY_2: u16 = 0x51F1;
const DEVICE_AX211_FAMILY_3: u16 = 0x54F0;
const DEVICE_AX211_FAMILY_4: u16 = 0x7A70;
const DEVICE_AX211_FAMILY_5: u16 = 0x7AF0;
const DEVICE_AX211_FAMILY_6: u16 = 0x7E40;
const DEVICE_AX211_FAMILY_7: u16 = 0x7F70;

const WIFI_STATUS_NOT_DETECTED: &str = "No detectado";
const WIFI_STATUS_UNKNOWN_ID: &str = "Detectado (ID no mapeado, fase1)";
const WIFI_STATUS_PHASE1_FW: &str = "Detectado (fase1 nativa, firmware pendiente)";
const WIFI_STATUS_PHASE1_READY: &str = "Detectado (fase1 nativa, datapath listo)";

const MAX_SSID_LEN: usize = 32;
const MAX_PSK_LEN: usize = 64;
const MAX_SCAN_RESULTS: usize = 16;

#[derive(Clone, Copy)]
struct WifiProfile {
    ssid: [u8; MAX_SSID_LEN],
    ssid_len: usize,
    psk: [u8; MAX_PSK_LEN],
    psk_len: usize,
    secure: bool,
}

impl WifiProfile {
    const fn empty() -> Self {
        Self {
            ssid: [0; MAX_SSID_LEN],
            ssid_len: 0,
            psk: [0; MAX_PSK_LEN],
            psk_len: 0,
            secure: false,
        }
    }
}

#[derive(Clone, Copy)]
pub struct WifiProfileInfo {
    pub ssid: [u8; MAX_SSID_LEN],
    pub ssid_len: usize,
    pub secure: bool,
}

impl WifiProfileInfo {
    pub fn ssid_str(&self) -> &str {
        core::str::from_utf8(&self.ssid[..self.ssid_len]).unwrap_or("<invalid-ssid>")
    }
}

#[derive(Clone, Copy)]
pub struct WifiScanEntry {
    pub ssid: [u8; MAX_SSID_LEN],
    pub ssid_len: usize,
    pub rssi_dbm: i8,
    pub channel: u8,
    pub secure: bool,
    pub valid: bool,
}

impl WifiScanEntry {
    const fn empty() -> Self {
        Self {
            ssid: [0; MAX_SSID_LEN],
            ssid_len: 0,
            rssi_dbm: 0,
            channel: 0,
            secure: false,
            valid: false,
        }
    }

    pub fn ssid_str(&self) -> &str {
        core::str::from_utf8(&self.ssid[..self.ssid_len]).unwrap_or("<invalid-ssid>")
    }
}

pub struct IntelWifiDevice {
    pub pci: PciDevice,
    pub mmio_base: Option<u64>,
    pub command_reg: u16,
    pub revision_id: u8,
    pub subsystem_vendor_id: u16,
    pub subsystem_device_id: u16,
}

pub static mut GLOBAL_INTEL_WIFI: Option<IntelWifiDevice> = None;
pub static mut WIFI_STATUS: &str = WIFI_STATUS_NOT_DETECTED;
static mut WIFI_PROFILE: Option<WifiProfile> = None;
static mut WIFI_CONNECTED: bool = false;
static mut WIFI_CONNECTED_SSID: [u8; MAX_SSID_LEN] = [0; MAX_SSID_LEN];
static mut WIFI_CONNECTED_SSID_LEN: usize = 0;
static mut LAST_SCAN_STATUS: &str = "Sin escaneo";
static mut LAST_SCAN_RESULTS: [WifiScanEntry; MAX_SCAN_RESULTS] =
    [WifiScanEntry::empty(); MAX_SCAN_RESULTS];
static mut LAST_SCAN_COUNT: usize = 0;

fn wifi_model_name(device_id: u16) -> Option<&'static str> {
    match device_id {
        DEVICE_AX210 => Some("Intel AX210"),
        DEVICE_BE200_BE202 => Some("Intel BE200/BE202"),
        DEVICE_AX211_BE201_FAMILY => Some("Intel AX211/BE201 family"),
        DEVICE_AX211_FAMILY_1
        | DEVICE_AX211_FAMILY_2
        | DEVICE_AX211_FAMILY_3
        | DEVICE_AX211_FAMILY_4
        | DEVICE_AX211_FAMILY_5
        | DEVICE_AX211_FAMILY_6
        | DEVICE_AX211_FAMILY_7 => Some("Intel AX211/AX411 family"),
        _ => None,
    }
}

fn firmware_hint_for_device(device_id: u16) -> Option<&'static str> {
    match device_id {
        DEVICE_AX210 => Some("iwlwifi-ty-a0-gf-a0-*.ucode"),
        DEVICE_BE200_BE202 => Some("iwlwifi-gl-c0-fm-c0-*.ucode"),
        DEVICE_AX211_BE201_FAMILY
        | DEVICE_AX211_FAMILY_1
        | DEVICE_AX211_FAMILY_2
        | DEVICE_AX211_FAMILY_3
        | DEVICE_AX211_FAMILY_4
        | DEVICE_AX211_FAMILY_5
        | DEVICE_AX211_FAMILY_6
        | DEVICE_AX211_FAMILY_7 => Some("iwlwifi-so-a0-gf-a0-*.ucode"),
        _ => None,
    }
}

fn copy_ascii<const N: usize>(dst: &mut [u8; N], text: &str, empty_ok: bool) -> Result<usize, &'static str> {
    if text.is_empty() && !empty_ok {
        return Err("Texto vacio.");
    }
    if text.len() > N {
        return Err("Texto demasiado largo.");
    }
    for b in text.bytes() {
        if !b.is_ascii() || b == 0 {
            return Err("Solo ASCII simple esta soportado por ahora.");
        }
    }
    dst.fill(0);
    for (i, b) in text.bytes().enumerate() {
        dst[i] = b;
    }
    Ok(text.len())
}

fn clear_scan_results() {
    unsafe {
        LAST_SCAN_RESULTS = [WifiScanEntry::empty(); MAX_SCAN_RESULTS];
        LAST_SCAN_COUNT = 0;
    }
}

pub fn init(device: PciDevice) {
    if device.vendor_id != VENDOR_INTEL {
        return;
    }

    let command_reg = unsafe { read_config(device.bus, device.slot, device.func, 0x04) as u16 };
    let class_rev = unsafe { read_config(device.bus, device.slot, device.func, 0x08) };
    let revision_id = (class_rev & 0xFF) as u8;
    let subsystem = unsafe { read_config(device.bus, device.slot, device.func, 0x2C) };
    let subsystem_vendor_id = (subsystem & 0xFFFF) as u16;
    let subsystem_device_id = (subsystem >> 16) as u16;

    // Read BAR only for inventory/reporting; do not touch MMIO registers yet.
    let mmio_base = unsafe { read_bar(device.bus, device.slot, device.func, 0) };

    let model = wifi_model_name(device.device_id).unwrap_or("Intel WiFi (unknown ID)");
    let fw_hint = firmware_hint_for_device(device.device_id);

    println(
        alloc::format!(
            "Intel WiFi: Detected {} [8086:{:04X}] @ {}:{}.{}",
            model,
            device.device_id,
            device.bus,
            device.slot,
            device.func
        )
        .as_str(),
    );
    println(
        alloc::format!(
            "Intel WiFi: rev={:#04x} cmd={:#06x} subsys={:04X}:{:04X}",
            revision_id,
            command_reg,
            subsystem_vendor_id,
            subsystem_device_id
        )
        .as_str(),
    );
    if let Some(bar) = mmio_base {
        println(alloc::format!("Intel WiFi: BAR0 MMIO = {:#x}", bar).as_str());
    } else {
        println("Intel WiFi: BAR0 MMIO unavailable.");
    }
    if let Some(hint) = fw_hint {
        println(alloc::format!("Intel WiFi: Firmware esperado: {}", hint).as_str());
    } else {
        println("Intel WiFi: Firmware hint unavailable for this PCI ID.");
    }

    unsafe {
        // Keep probe non-invasive until a full native Wi-Fi driver is implemented.
        // Do not enable bus master/MMIO here on real hardware.
        GLOBAL_INTEL_WIFI = Some(IntelWifiDevice {
            pci: device,
            mmio_base,
            command_reg,
            revision_id,
            subsystem_vendor_id,
            subsystem_device_id,
        });
        WIFI_STATUS = if fw_hint.is_some() {
            WIFI_STATUS_PHASE1_FW
        } else {
            WIFI_STATUS_UNKNOWN_ID
        };
    }

    println("Intel WiFi: Base driver initialized (phase1 safe probe).");
    println("Intel WiFi: Windows .sys/.inf package cannot run directly in this kernel.");
}

pub fn is_present() -> bool {
    unsafe { GLOBAL_INTEL_WIFI.is_some() }
}

pub fn is_data_path_ready() -> bool {
    // Phase 1 currently exposes PCI probing + metadata only.
    // TX/RX rings, firmware load and 802.11 state machine are pending.
    false
}

pub fn get_model_name() -> Option<&'static str> {
    unsafe {
        GLOBAL_INTEL_WIFI
            .as_ref()
            .and_then(|dev| wifi_model_name(dev.pci.device_id))
    }
}

pub fn get_status() -> &'static str {
    unsafe { WIFI_STATUS }
}

pub fn scan_networks() -> &'static str {
    clear_scan_results();

    if !is_present() {
        unsafe {
            LAST_SCAN_STATUS = "No hay adaptador WiFi Intel detectado.";
        }
        return unsafe { LAST_SCAN_STATUS };
    }

    if !is_data_path_ready() {
        unsafe {
            LAST_SCAN_STATUS = "Escaneo no disponible: datapath WiFi pendiente (fase2).";
            if WIFI_STATUS == WIFI_STATUS_PHASE1_READY {
                WIFI_STATUS = WIFI_STATUS_PHASE1_FW;
            }
        }
        return unsafe { LAST_SCAN_STATUS };
    }

    unsafe {
        LAST_SCAN_STATUS = "Escaneo completado: 0 redes detectadas.";
    }
    unsafe { LAST_SCAN_STATUS }
}

pub fn get_last_scan_status() -> &'static str {
    unsafe { LAST_SCAN_STATUS }
}

pub fn get_last_scan_count() -> usize {
    unsafe { LAST_SCAN_COUNT }
}

pub fn get_scan_entry(index: usize) -> Option<WifiScanEntry> {
    unsafe {
        if index < LAST_SCAN_COUNT {
            Some(LAST_SCAN_RESULTS[index])
        } else {
            None
        }
    }
}

pub fn configure_profile(ssid: &str, psk: &str) -> Result<&'static str, &'static str> {
    let mut profile = WifiProfile::empty();
    profile.ssid_len = copy_ascii(&mut profile.ssid, ssid, false)?;
    profile.psk_len = copy_ascii(&mut profile.psk, psk, true)?;
    profile.secure = profile.psk_len > 0;

    unsafe {
        WIFI_PROFILE = Some(profile);
        WIFI_CONNECTED = false;
        WIFI_CONNECTED_SSID = [0; MAX_SSID_LEN];
        WIFI_CONNECTED_SSID_LEN = 0;
        if WIFI_STATUS == WIFI_STATUS_NOT_DETECTED {
            WIFI_STATUS = WIFI_STATUS_UNKNOWN_ID;
        }
    }
    Ok("Perfil guardado.")
}

pub fn clear_profile() -> &'static str {
    unsafe {
        WIFI_PROFILE = None;
        WIFI_CONNECTED = false;
        WIFI_CONNECTED_SSID = [0; MAX_SSID_LEN];
        WIFI_CONNECTED_SSID_LEN = 0;
    }
    "Perfil WiFi eliminado."
}

pub fn get_profile_info() -> Option<WifiProfileInfo> {
    unsafe {
        WIFI_PROFILE.map(|p| WifiProfileInfo {
            ssid: p.ssid,
            ssid_len: p.ssid_len,
            secure: p.secure,
        })
    }
}

pub fn has_profile() -> bool {
    unsafe { WIFI_PROFILE.is_some() }
}

pub fn connect_profile() -> &'static str {
    if !is_present() {
        return "No hay adaptador WiFi Intel detectado.";
    }

    let profile = unsafe { WIFI_PROFILE };
    let Some(profile) = profile else {
        return "No hay perfil configurado. Usa: wifi connect <ssid> <clave>";
    };

    if !is_data_path_ready() {
        unsafe {
            WIFI_CONNECTED = false;
            WIFI_CONNECTED_SSID = [0; MAX_SSID_LEN];
            WIFI_CONNECTED_SSID_LEN = 0;
            WIFI_STATUS = WIFI_STATUS_PHASE1_FW;
        }
        return "Conexion pendiente: driver WiFi fase1 (scan/connect reales en fase2).";
    }

    unsafe {
        WIFI_CONNECTED = true;
        WIFI_CONNECTED_SSID = profile.ssid;
        WIFI_CONNECTED_SSID_LEN = profile.ssid_len;
        WIFI_STATUS = WIFI_STATUS_PHASE1_READY;
    }
    "WiFi conectado (modo experimental)."
}

pub fn disconnect() -> &'static str {
    unsafe {
        WIFI_CONNECTED = false;
        WIFI_CONNECTED_SSID = [0; MAX_SSID_LEN];
        WIFI_CONNECTED_SSID_LEN = 0;
        if GLOBAL_INTEL_WIFI.is_some() {
            WIFI_STATUS = WIFI_STATUS_PHASE1_FW;
        }
    }
    "WiFi desconectado."
}

pub fn is_connected() -> bool {
    unsafe { WIFI_CONNECTED }
}

pub fn connected_ssid() -> Option<([u8; MAX_SSID_LEN], usize)> {
    unsafe {
        if WIFI_CONNECTED && WIFI_CONNECTED_SSID_LEN > 0 {
            Some((WIFI_CONNECTED_SSID, WIFI_CONNECTED_SSID_LEN))
        } else {
            None
        }
    }
}

pub fn firmware_hint() -> Option<&'static str> {
    unsafe {
        GLOBAL_INTEL_WIFI
            .as_ref()
            .and_then(|dev| firmware_hint_for_device(dev.pci.device_id))
    }
}

pub fn get_pci_location() -> Option<(u8, u8, u8)> {
    unsafe {
        GLOBAL_INTEL_WIFI
            .as_ref()
            .map(|dev| (dev.pci.bus, dev.pci.slot, dev.pci.func))
    }
}

pub fn get_pci_ids() -> Option<(u16, u16, u16, u16)> {
    unsafe {
        GLOBAL_INTEL_WIFI.as_ref().map(|dev| {
            (
                dev.pci.vendor_id,
                dev.pci.device_id,
                dev.subsystem_vendor_id,
                dev.subsystem_device_id,
            )
        })
    }
}

pub fn get_revision() -> Option<u8> {
    unsafe { GLOBAL_INTEL_WIFI.as_ref().map(|dev| dev.revision_id) }
}

pub fn get_command_reg() -> Option<u16> {
    unsafe { GLOBAL_INTEL_WIFI.as_ref().map(|dev| dev.command_reg) }
}

pub fn get_mmio_base() -> Option<u64> {
    unsafe { GLOBAL_INTEL_WIFI.as_ref().and_then(|dev| dev.mmio_base) }
}
