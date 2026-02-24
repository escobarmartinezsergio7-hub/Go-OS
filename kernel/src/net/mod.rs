use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::format;
use core::str;
use miniz_oxide::inflate::{decompress_to_vec, decompress_to_vec_zlib};
use smoltcp::phy::{Device, DeviceCapabilities, Medium};
use smoltcp::time::Instant;
use smoltcp::iface::{Interface, Config, SocketSet};
use smoltcp::socket::{tcp, dhcpv4, dns};
use smoltcp::wire::{EthernetAddress, IpCidr, Ipv4Address, IpAddress};
use smoltcp::iface::SocketStorage;

use crate::println;
pub mod tls;

const DHCP_STATUS_INACTIVE: &str = "Inactivo";
const DHCP_STATUS_SEARCHING: &str = "Buscando...";
const DHCP_STATUS_CONFIGURED: &str = "Configurado";
const DHCP_STATUS_NO_LINK: &str = "Sin enlace";
const DHCP_STATUS_STATIC: &str = "IP Fija";
const NET_TRANSPORT_NONE: &str = "Sin interfaz";
const NET_TRANSPORT_INTEL_ETH: &str = "Intel Ethernet";
const NET_TRANSPORT_INTEL_WIFI: &str = "Intel WiFi";
const NET_TRANSPORT_VIRTIO: &str = "VirtIO Ethernet";
const FAILOVER_ETHERNET_FIRST: &str = "EthernetFirst";
const FAILOVER_WIFI_FIRST: &str = "WifiFirst";
const NET_MODE_DHCP: &str = "DHCP";
const NET_MODE_STATIC: &str = "Static";
const HTTPS_MODE_PROXY: &str = "CompatProxy";
const HTTPS_MODE_DISABLED: &str = "Disabled";
const HTTPS_PROXY_BASE: &str = "http://r.jina.ai/";
const HTTPS_PROXY_HOST: &str = "r.jina.ai";
const HTTP_ACCEPT_ENCODING_VALUE: &str = "gzip, deflate, identity";
const HTTP_CACHE_MAX_ENTRIES: usize = 16;
const HTTP_CACHE_MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const HTTP_COOKIE_MAX_ENTRIES: usize = 64;
const HTTP_CONN_POOL_MAX_ENTRIES: usize = 4;
const HTTP_CONN_POOL_IDLE_TICKS: u64 = 3_000;
const HTTP2_TLS_POOL_MAX_ENTRIES: usize = 2;
const HTTP2_TLS_POOL_IDLE_TICKS: u64 = 4_000;
const HTTP_RETRY_MAX_ATTEMPTS: usize = 3;
const HTTP_RETRY_BASE_BACKOFF_TICKS: u64 = 25;
const HTTP_RETRY_MAX_BACKOFF_TICKS: u64 = 800;
const DNS_SERVER_LIMIT: usize = 1;

// Default networking mode at boot.
// `false` = start in DHCP mode automatically.
const NET_USE_STATIC_IPV4: bool = false;
const STATIC_IPV4_ADDR: [u8; 4] = [192, 168, 100, 50];
const STATIC_IPV4_PREFIX_LEN: u8 = 24;
const STATIC_IPV4_GATEWAY: [u8; 4] = [192, 168, 100, 1];
const STATIC_DNS_SERVERS: [[u8; 4]; 2] = [[192, 168, 100, 1], [1, 1, 1, 1]];

pub enum ReduxPhy {
    Virtio(VirtioPhy),
    Intel(crate::intel_net::IntelPhy),
}

impl Device for ReduxPhy {
    type RxToken<'a> = ReduxRxToken<'a>;
    type TxToken<'a> = ReduxTxToken<'a>;

    fn receive(&mut self, timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        match self {
            Self::Virtio(v) => v.receive(timestamp).map(|(rx, tx)| (ReduxRxToken::Virtio(rx), ReduxTxToken::Virtio(tx))),
            Self::Intel(i) => i.receive(timestamp).map(|(rx, tx)| (ReduxRxToken::Intel(rx), ReduxTxToken::Intel(tx))),
        }
    }

    fn transmit(&mut self, timestamp: Instant) -> Option<Self::TxToken<'_>> {
        match self {
            Self::Virtio(v) => v.transmit(timestamp).map(ReduxTxToken::Virtio),
            Self::Intel(i) => i.transmit(timestamp).map(ReduxTxToken::Intel),
        }
    }

    fn capabilities(&self) -> DeviceCapabilities {
        match self {
            Self::Virtio(v) => v.capabilities(),
            Self::Intel(i) => i.capabilities(),
        }
    }
}

pub enum ReduxRxToken<'a> {
    Virtio(VirtioRxToken),
    Intel(crate::intel_net::IntelRxToken),
    _Dummy(&'a ()),
}

impl<'a> smoltcp::phy::RxToken for ReduxRxToken<'a> {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        match self {
            Self::Virtio(rx) => rx.consume(f),
            Self::Intel(rx) => rx.consume(f),
            _ => unsafe { core::hint::unreachable_unchecked() },
        }
    }
}

pub enum ReduxTxToken<'a> {
    Virtio(VirtioTxToken),
    Intel(crate::intel_net::IntelTxToken<'a>),
    _Dummy(&'a ()),
}

impl<'a> smoltcp::phy::TxToken for ReduxTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        match self {
            Self::Virtio(tx) => tx.consume(len, f),
            Self::Intel(tx) => tx.consume(len, f),
            _ => unsafe { core::hint::unreachable_unchecked() },
        }
    }
}

pub struct VirtioPhy;

impl Device for VirtioPhy {
    type RxToken<'a> = VirtioRxToken where Self: 'a;
    type TxToken<'a> = VirtioTxToken where Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        unsafe {
            if let Some(ref mut drv) = crate::virtio::net::GLOBAL_NET {
                if let Some(packet) = drv.receive() {
                    return Some((VirtioRxToken(packet), VirtioTxToken));
                }
            }
        }
        None
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(VirtioTxToken)
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1500;
        caps.medium = Medium::Ethernet;
        caps
    }
}

pub struct VirtioRxToken(Vec<u8>);

impl smoltcp::phy::RxToken for VirtioRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = self.0;
        f(&mut buffer)
    }
}

pub struct VirtioTxToken;

impl smoltcp::phy::TxToken for VirtioTxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = alloc::vec![0u8; len];
        let result = f(&mut buffer);
        unsafe {
            if let Some(ref mut drv) = crate::virtio::net::GLOBAL_NET {
                drv.transmit(&buffer);
            }
        }
        result
    }
}

// Global Stack State
pub static mut SOCKETS: Option<SocketSet<'static>> = None;
pub static mut IFACE: Option<Interface> = None;
pub static mut DHCP_HANDLE: Option<smoltcp::iface::SocketHandle> = None;
pub static mut DNS_HANDLE: Option<smoltcp::iface::SocketHandle> = None;
pub static mut DHCP_STATUS: &str = DHCP_STATUS_INACTIVE;
pub static mut IPV4_GATEWAY: Option<IpAddress> = None;
pub static mut ACTIVE_TRANSPORT: &str = NET_TRANSPORT_NONE;
pub static mut FAILOVER_POLICY: &str = FAILOVER_ETHERNET_FIRST;
static mut USE_STATIC_IPV4_RUNTIME: bool = NET_USE_STATIC_IPV4;
static mut STATIC_IPV4_ADDR_RUNTIME: [u8; 4] = STATIC_IPV4_ADDR;
static mut STATIC_IPV4_PREFIX_RUNTIME: u8 = STATIC_IPV4_PREFIX_LEN;
static mut STATIC_IPV4_GATEWAY_RUNTIME: [u8; 4] = STATIC_IPV4_GATEWAY;
static mut STATIC_DNS_SERVERS_RUNTIME: [[u8; 4]; 2] = STATIC_DNS_SERVERS;
static mut HTTPS_PROXY_ENABLED: bool = false;
static mut DHCP_LAST_RESET_TICK: u64 = 0;
static mut WIFI_AUTOCONNECT_LAST_TICK: u64 = 0;
static mut HTTP_CACHE: Vec<HttpCacheEntry> = Vec::new();
static mut HTTP_COOKIE_JAR: Vec<HttpCookieEntry> = Vec::new();
static mut HTTP_CONN_POOL: Vec<HttpConnPoolEntry> = Vec::new();

#[derive(Clone)]
struct HttpCacheEntry {
    url: String,
    etag: Option<String>,
    last_modified: Option<String>,
    response_bytes: Vec<u8>,
    stored_at_ticks: u64,
}

#[derive(Clone)]
struct HttpCookieEntry {
    name: String,
    value: String,
    domain: String,
    path: String,
    secure: bool,
    host_only: bool,
    expires_at_ticks: Option<u64>,
}

#[derive(Clone, Default)]
struct HttpRequestHints {
    cookie_header: Option<String>,
    if_none_match: Option<String>,
    if_modified_since: Option<String>,
}

#[derive(Clone, Default)]
struct ParsedHttpHeaders {
    status_code: Option<u16>,
    status_line: Option<String>,
    headers: Vec<(String, String)>,
    body_offset: usize,
}

#[derive(Clone)]
struct HttpConnPoolEntry {
    handle: smoltcp::iface::SocketHandle,
    host: String,
    port: u16,
    is_https: bool,
    use_https_proxy: bool,
    last_used_ticks: u64,
}

fn default_ethernet_transport() -> &'static str {
    if crate::intel_net::get_model_name().is_some() {
        NET_TRANSPORT_INTEL_ETH
    } else {
        NET_TRANSPORT_VIRTIO
    }
}

fn refresh_active_transport() {
    let ethernet_up = if crate::intel_net::get_model_name().is_some() {
        crate::intel_net::is_link_up()
    } else {
        true
    };
    let wifi_up = crate::intel_wifi::is_data_path_ready() && crate::intel_wifi::is_connected();
    let ethernet_transport = default_ethernet_transport();

    let selected = unsafe {
        if FAILOVER_POLICY == FAILOVER_WIFI_FIRST {
            if wifi_up {
                NET_TRANSPORT_INTEL_WIFI
            } else if ethernet_up {
                ethernet_transport
            } else {
                NET_TRANSPORT_NONE
            }
        } else if ethernet_up {
            ethernet_transport
        } else if wifi_up {
            NET_TRANSPORT_INTEL_WIFI
        } else {
            NET_TRANSPORT_NONE
        }
    };

    unsafe {
        ACTIVE_TRANSPORT = selected;
    }
}

fn ipv4_from_octets(octets: [u8; 4]) -> Ipv4Address {
    Ipv4Address::new(octets[0], octets[1], octets[2], octets[3])
}

fn update_dns_servers(
    sockets: &mut SocketSet<'_>,
    dns_handle: smoltcp::iface::SocketHandle,
    servers: &[IpAddress],
) {
    let mut safe_servers = Vec::with_capacity(DNS_SERVER_LIMIT);
    for server in servers.iter() {
        if safe_servers.len() >= DNS_SERVER_LIMIT {
            break;
        }
        safe_servers.push(*server);
    }
    if safe_servers.is_empty() {
        safe_servers.push(Ipv4Address::new(8, 8, 8, 8).into());
    }
    sockets
        .get_mut::<dns::Socket>(dns_handle)
        .update_servers(&safe_servers);
}

fn reset_ipv4_runtime(iface: &mut Interface) {
    iface.update_ip_addrs(|ip_addrs| {
        ip_addrs.clear();
        let cidr: IpCidr = IpCidr::new(Ipv4Address::new(0, 0, 0, 0).into(), 0);
        ip_addrs.push(cidr).unwrap();
    });
    let _ = iface.routes_mut().remove_default_ipv4_route();
    unsafe {
        IPV4_GATEWAY = None;
    }
}

fn apply_static_ipv4_runtime(iface: &mut Interface) {
    let (static_ip, prefix, gateway) = unsafe {
        (
            ipv4_from_octets(STATIC_IPV4_ADDR_RUNTIME),
            STATIC_IPV4_PREFIX_RUNTIME,
            ipv4_from_octets(STATIC_IPV4_GATEWAY_RUNTIME),
        )
    };

    iface.update_ip_addrs(|ip_addrs| {
        ip_addrs.clear();
        let cidr: IpCidr = IpCidr::new(static_ip.into(), prefix);
        ip_addrs.push(cidr).unwrap();
    });
    let _ = iface.routes_mut().remove_default_ipv4_route();
    if iface.routes_mut().add_default_ipv4_route(gateway.into()).is_err() {
        println("Net: Static gateway route update failed.");
    }
    unsafe {
        IPV4_GATEWAY = Some(gateway.into());
    }
}

fn static_dns_servers_runtime() -> Vec<IpAddress> {
    let mut dns_servers = Vec::with_capacity(2);
    unsafe {
        for server in STATIC_DNS_SERVERS_RUNTIME.iter() {
            dns_servers.push(ipv4_from_octets(*server).into());
        }
    }
    dns_servers
}

fn default_dns_servers() -> Vec<IpAddress> {
    alloc::vec![Ipv4Address::new(8, 8, 8, 8).into()]
}

fn parse_ipv4_octets(text: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut idx = 0usize;

    for part in text.split('.') {
        if idx >= 4 || part.is_empty() {
            return None;
        }
        let value = part.parse::<u16>().ok()?;
        if value > 255 {
            return None;
        }
        out[idx] = value as u8;
        idx += 1;
    }

    if idx == 4 { Some(out) } else { None }
}

fn maybe_autoconnect_wifi(now_ticks: u64, ethernet_up: bool) {
    if !crate::intel_wifi::is_present()
        || !crate::intel_wifi::is_data_path_ready()
        || !crate::intel_wifi::has_profile()
    {
        return;
    }

    let wifi_first = unsafe { FAILOVER_POLICY == FAILOVER_WIFI_FIRST };
    let should_use_wifi = wifi_first || !ethernet_up;

    if should_use_wifi {
        unsafe {
            if !crate::intel_wifi::is_connected()
                && now_ticks.saturating_sub(WIFI_AUTOCONNECT_LAST_TICK) >= 300
            {
                let _ = crate::intel_wifi::connect_profile();
                WIFI_AUTOCONNECT_LAST_TICK = now_ticks;
            }
        }
    } else if crate::intel_wifi::is_connected() {
        let _ = crate::intel_wifi::disconnect();
    }
}

pub fn init() {
    println("Net: Initializing Stack...");
    
    let mut phy = if unsafe { crate::intel_net::GLOBAL_INTEL_NET.is_some() } {
        println("Net: Using Native Intel I225/I226 PHY.");
        ReduxPhy::Intel(crate::intel_net::IntelPhy)
    } else {
        println("Net: Using VirtIO PHY.");
        ReduxPhy::Virtio(VirtioPhy)
    };
    refresh_active_transport();

    if crate::intel_wifi::is_present() {
        if crate::intel_wifi::is_data_path_ready() {
            println("Net: Intel WiFi datapath is ready.");
        } else {
            println("Net: Intel WiFi detected, but datapath is not ready yet (phase1).");
            println("Net: Traffic remains on Ethernet for now.");
        }
    }

    let mut mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
    if let Some(intel_mac) = crate::intel_net::get_mac_address() {
        mac = intel_mac;
    } else if let Some(virtio_mac) = unsafe { crate::virtio::net::GLOBAL_NET.as_ref().map(|drv| drv.mac_address()) } {
        mac = virtio_mac;
    }
    let mut config = Config::new(EthernetAddress(mac).into());
    config.random_seed = 0x12345678;
    
    let mut iface = Interface::new(config, &mut phy, Instant::from_millis(0));
    
    // Start from 0.0.0.0 and wait for DHCP lease.
    reset_ipv4_runtime(&mut iface);

    // Pre-allocate socket storage
    let mut storage = alloc::vec::Vec::with_capacity(8);
    for _ in 0..8 { storage.push(SocketStorage::EMPTY); }
    let storage_static = alloc::boxed::Box::leak(storage.into_boxed_slice());
    let mut sockets = SocketSet::new(&mut storage_static[..]);

    let dhcp = dhcpv4::Socket::new();
    let dhcp_handle = Some(sockets.add(dhcp));

    let dns_servers = alloc::vec![Ipv4Address::new(8, 8, 8, 8).into()];
    let dns_servers_static: &'static [IpAddress] = alloc::boxed::Box::leak(dns_servers.into_boxed_slice());
    
    let mut queries = alloc::vec::Vec::with_capacity(4);
    for _ in 0..4 { queries.push(None); }
    let queries_static = alloc::boxed::Box::leak(queries.into_boxed_slice());
    
    let dns_socket = dns::Socket::new(dns_servers_static, &mut queries_static[..]); 
    let dns_handle = sockets.add(dns_socket);

    let startup_status = unsafe {
        if USE_STATIC_IPV4_RUNTIME {
            apply_static_ipv4_runtime(&mut iface);
            let static_dns = static_dns_servers_runtime();
            update_dns_servers(&mut sockets, dns_handle, &static_dns);
            DHCP_STATUS_STATIC
        } else {
            let fallback_dns = default_dns_servers();
            update_dns_servers(&mut sockets, dns_handle, &fallback_dns);
            DHCP_STATUS_SEARCHING
        }
    };
    
    unsafe {
        DHCP_HANDLE = dhcp_handle;
        DNS_HANDLE = Some(dns_handle);
        IFACE = Some(iface);
        SOCKETS = Some(sockets);
        DHCP_STATUS = startup_status;
    }

    if unsafe { USE_STATIC_IPV4_RUNTIME } {
        let (static_ip, prefix, gateway) = unsafe {
            (
                ipv4_from_octets(STATIC_IPV4_ADDR_RUNTIME),
                STATIC_IPV4_PREFIX_RUNTIME,
                ipv4_from_octets(STATIC_IPV4_GATEWAY_RUNTIME),
            )
        };
        println("Net: Initialized (Static IPv4 mode).");
        println(alloc::format!("Net: IP fija -> {}/{}", static_ip, prefix).as_str());
        println(alloc::format!("Net: Gateway -> {}", gateway).as_str());
    } else {
        println("Net: Initialized (DHCP in background).");
    }
}

pub fn poll() {
    unsafe {
        if let (Some(iface), Some(sockets)) = (&mut IFACE, &mut SOCKETS) {
            let ethernet_up = if crate::intel_net::GLOBAL_INTEL_NET.is_some() {
                crate::intel_net::is_link_up()
            } else {
                true
            };

            let now_ticks = crate::timer::ticks();
            maybe_autoconnect_wifi(now_ticks, ethernet_up);
            refresh_active_transport();

            let mut phy = if crate::intel_net::GLOBAL_INTEL_NET.is_some() {
                ReduxPhy::Intel(crate::intel_net::IntelPhy)
            } else {
                ReduxPhy::Virtio(VirtioPhy)
            };

            let timestamp = Instant::from_millis(now_ticks as i64 * 10);
            iface.poll(timestamp, &mut phy, sockets);

            let active_transport = ACTIVE_TRANSPORT;
            if active_transport == NET_TRANSPORT_NONE {
                DHCP_STATUS = DHCP_STATUS_NO_LINK;
                return;
            }

            if active_transport == NET_TRANSPORT_INTEL_ETH
                && crate::intel_net::GLOBAL_INTEL_NET.is_some()
                && !ethernet_up
            {
                DHCP_STATUS = DHCP_STATUS_NO_LINK;
                return;
            }

            if USE_STATIC_IPV4_RUNTIME {
                DHCP_STATUS = DHCP_STATUS_STATIC;
                return;
            }
            
            // Background DHCP Management
            if let Some(dhcp_handle) = DHCP_HANDLE {
                if DHCP_STATUS == DHCP_STATUS_INACTIVE
                    || DHCP_STATUS == DHCP_STATUS_NO_LINK
                    || DHCP_STATUS == DHCP_STATUS_STATIC
                {
                    DHCP_STATUS = DHCP_STATUS_SEARCHING;
                }
                let event = sockets.get_mut::<dhcpv4::Socket>(dhcp_handle).poll();
                if let Some(event) = event {
                    match event {
                        dhcpv4::Event::Configured(config) => {
                            println("Net: DHCP Configured!");
                            DHCP_STATUS = DHCP_STATUS_CONFIGURED;
                            println(alloc::format!("Net: IP -> {}", config.address).as_str());
                            
                            iface.update_ip_addrs(|addrs| {
                                addrs.clear();
                                let cidr: IpCidr = IpCidr::Ipv4(config.address);
                                addrs.push(cidr).unwrap();
                            });

                            let _ = iface.routes_mut().remove_default_ipv4_route();
                            if let Some(router) = config.router {
                                println(alloc::format!("Net: Gateway -> {}", router).as_str());
                                if iface.routes_mut().add_default_ipv4_route(router.into()).is_err() {
                                    println("Net: DHCP Gateway route update failed.");
                                }
                                IPV4_GATEWAY = Some(router.into());
                            } else {
                                IPV4_GATEWAY = None;
                            }

                            // Update DNS servers from DHCP
                            if let Some(dns_handle) = DNS_HANDLE {
                                let mut servers = alloc::vec::Vec::new();
                                for server in config.dns_servers.iter() {
                                    if !server.is_unspecified() {
                                        servers.push((*server).into());
                                    }
                                }
                                if servers.is_empty() {
                                    servers.push(Ipv4Address::new(8, 8, 8, 8).into()); // Fallback to Google
                                }
                                update_dns_servers(sockets, dns_handle, &servers);
                                println("Net: DNS Servers Updated.");
                            }
                        }
                        dhcpv4::Event::Deconfigured => {
                            println("Net: DHCP lease lost, retrying...");
                            DHCP_STATUS = DHCP_STATUS_SEARCHING;
                            reset_ipv4_runtime(iface);
                        }
                    }
                }

                if DHCP_STATUS == DHCP_STATUS_SEARCHING {
                    if now_ticks.saturating_sub(DHCP_LAST_RESET_TICK) > 2000 {
                        sockets.get_mut::<dhcpv4::Socket>(dhcp_handle).reset();
                        DHCP_LAST_RESET_TICK = now_ticks;
                    }
                }
            }
        }
    }
}

const NET_BLOCKING_LOOP_STALL_US: usize = 1_000;
const NET_BLOCKING_TIMEOUT_TICKS: u64 = 5_000;

fn starts_with_ignore_ascii_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .map(|head| head.eq_ignore_ascii_case(prefix))
        .unwrap_or(false)
}

fn build_https_proxy_url(url: &str) -> String {
    let mut out = String::from(HTTPS_PROXY_BASE);
    out.push_str(url);
    out
}

fn extract_url_host(url: &str) -> Option<&str> {
    let without_scheme = if starts_with_ignore_ascii_case(url, "http://") {
        &url[7..]
    } else if starts_with_ignore_ascii_case(url, "https://") {
        &url[8..]
    } else {
        url
    };

    let authority = match without_scheme.find('/') {
        Some(idx) => &without_scheme[..idx],
        None => without_scheme,
    };
    let host = match authority.find(':') {
        Some(idx) => &authority[..idx],
        None => authority,
    };

    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

fn is_https_proxy_url(url: &str) -> bool {
    extract_url_host(url)
        .map(|host| host.eq_ignore_ascii_case(HTTPS_PROXY_HOST))
        .unwrap_or(false)
}

fn parse_url(url: &str) -> Option<(String, u16, &str)> {
    let url = if starts_with_ignore_ascii_case(url, "http://") {
        &url[7..]
    } else if starts_with_ignore_ascii_case(url, "https://") {
        &url[8..]
    } else {
        url
    };
    
    let (addr_part, path) = match url.find('/') {
        Some(idx) => (&url[..idx], &url[idx..]),
        None => (url, "/"),
    };
    
    let (host, port) = match addr_part.find(':') {
        Some(idx) => (String::from(&addr_part[..idx]), addr_part[idx+1..].parse().ok()?),
        None => (String::from(addr_part), 80),
    };
    
    Some((host, port, path))
}

fn http_wait_ticks_with_ui(pump_ui: &mut impl FnMut(), wait_ticks: u64) {
    if wait_ticks == 0 {
        return;
    }
    let start = crate::timer::ticks();
    while crate::timer::ticks().saturating_sub(start) < wait_ticks {
        pump_ui();
        crate::timer::on_tick();
        uefi::boot::stall(NET_BLOCKING_LOOP_STALL_US);
    }
}

fn http_retry_backoff_ticks(attempt: usize) -> u64 {
    let shift = core::cmp::min(attempt as u32, 6);
    let ticks = HTTP_RETRY_BASE_BACKOFF_TICKS.saturating_mul(1u64 << shift);
    core::cmp::min(ticks, HTTP_RETRY_MAX_BACKOFF_TICKS)
}

fn http_should_retry_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

fn http_status_has_no_body(status: Option<u16>) -> bool {
    match status {
        Some(code) if (100..200).contains(&code) => true,
        Some(204) | Some(304) => true,
        _ => false,
    }
}

fn http_header_contains_token(value: &str, token: &str) -> bool {
    value
        .split(',')
        .any(|part| part.trim().eq_ignore_ascii_case(token))
}

fn http_response_allows_keepalive(parsed: &ParsedHttpHeaders) -> bool {
    match header_first(parsed.headers.as_slice(), "connection") {
        Some(conn) => !http_header_contains_token(conn, "close"),
        None => true,
    }
}

fn http_pool_prune(sockets: &mut SocketSet<'_>, now_ticks: u64) {
    unsafe {
        let mut i = 0usize;
        while i < HTTP_CONN_POOL.len() {
            let stale = now_ticks.saturating_sub(HTTP_CONN_POOL[i].last_used_ticks) > HTTP_CONN_POOL_IDLE_TICKS;
            let mut drop_entry = stale;
            if !drop_entry {
                let socket = sockets.get_mut::<tcp::Socket>(HTTP_CONN_POOL[i].handle);
                drop_entry = !socket.is_open() || !socket.is_active();
            }
            if drop_entry {
                let removed = HTTP_CONN_POOL.remove(i);
                sockets.remove(removed.handle);
            } else {
                i += 1;
            }
        }
    }
}

fn http_pool_take_reusable_socket(
    sockets: &mut SocketSet<'_>,
    host: &str,
    port: u16,
    is_https: bool,
    use_https_proxy: bool,
    now_ticks: u64,
) -> Option<smoltcp::iface::SocketHandle> {
    if is_https && !use_https_proxy {
        return None;
    }

    http_pool_prune(sockets, now_ticks);
    unsafe {
        let mut selected = None;
        for (idx, entry) in HTTP_CONN_POOL.iter().enumerate() {
            if entry.port == port
                && entry.is_https == is_https
                && entry.use_https_proxy == use_https_proxy
                && entry.host.eq_ignore_ascii_case(host)
            {
                selected = Some(idx);
                break;
            }
        }
        let idx = selected?;
        let entry = HTTP_CONN_POOL.remove(idx);
        let reusable = {
            let socket = sockets.get_mut::<tcp::Socket>(entry.handle);
            socket.is_open() && socket.is_active() && socket.may_send()
        };
        if reusable {
            Some(entry.handle)
        } else {
            sockets.remove(entry.handle);
            None
        }
    }
}

fn http_pool_store_socket(
    sockets: &mut SocketSet<'_>,
    handle: smoltcp::iface::SocketHandle,
    host: &str,
    port: u16,
    is_https: bool,
    use_https_proxy: bool,
    now_ticks: u64,
) {
    if is_https && !use_https_proxy {
        sockets.remove(handle);
        return;
    }

    let is_reusable = {
        let socket = sockets.get_mut::<tcp::Socket>(handle);
        socket.is_open() && socket.is_active() && socket.may_send()
    };
    if !is_reusable {
        sockets.remove(handle);
        return;
    }

    http_pool_prune(sockets, now_ticks);
    unsafe {
        let mut i = 0usize;
        while i < HTTP_CONN_POOL.len() {
            let same_endpoint = HTTP_CONN_POOL[i].port == port
                && HTTP_CONN_POOL[i].is_https == is_https
                && HTTP_CONN_POOL[i].use_https_proxy == use_https_proxy
                && HTTP_CONN_POOL[i].host.eq_ignore_ascii_case(host);
            if same_endpoint {
                let removed = HTTP_CONN_POOL.remove(i);
                if removed.handle != handle {
                    sockets.remove(removed.handle);
                }
            } else {
                i += 1;
            }
        }

        if HTTP_CONN_POOL.len() >= HTTP_CONN_POOL_MAX_ENTRIES {
            let removed = HTTP_CONN_POOL.remove(0);
            if removed.handle != handle {
                sockets.remove(removed.handle);
            }
        }

        HTTP_CONN_POOL.push(HttpConnPoolEntry {
            handle,
            host: String::from(host),
            port,
            is_https,
            use_https_proxy,
            last_used_ticks: now_ticks,
        });
    }
}

fn ascii_lowercase(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        out.push((b.to_ascii_lowercase()) as char);
    }
    out
}

fn find_http_header_end(raw: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0usize;
    while i + 3 < raw.len() {
        if raw[i] == b'\r' && raw[i + 1] == b'\n' && raw[i + 2] == b'\r' && raw[i + 3] == b'\n' {
            return Some((i, i + 4));
        }
        i += 1;
    }
    let mut j = 0usize;
    while j + 1 < raw.len() {
        if raw[j] == b'\n' && raw[j + 1] == b'\n' {
            return Some((j, j + 2));
        }
        j += 1;
    }
    None
}

fn parse_http_headers(raw: &[u8]) -> ParsedHttpHeaders {
    let mut parsed = ParsedHttpHeaders::default();
    let Some((head_end, body_offset)) = find_http_header_end(raw) else {
        return parsed;
    };
    parsed.body_offset = body_offset;

    let head_bytes = &raw[..head_end];
    let head = String::from_utf8_lossy(head_bytes).into_owned();
    let mut lines = head.lines();
    if let Some(status_line) = lines.next() {
        let status_trimmed = status_line.trim();
        parsed.status_line = Some(String::from(status_trimmed));
        let mut parts = status_trimmed.split_whitespace();
        let _proto = parts.next();
        parsed.status_code = parts.next().and_then(|s| s.parse::<u16>().ok());
    }
    for line in lines {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        if let Some((k, v)) = l.split_once(':') {
            parsed
                .headers
                .push((ascii_lowercase(k.trim()), String::from(v.trim())));
        }
    }
    parsed
}

fn header_first<'a>(headers: &'a [(String, String)], key: &str) -> Option<&'a str> {
    let key_lower = ascii_lowercase(key);
    for (k, v) in headers.iter() {
        if *k == key_lower {
            return Some(v.as_str());
        }
    }
    None
}

fn header_values<'a>(headers: &'a [(String, String)], key: &str) -> Vec<&'a str> {
    let key_lower = ascii_lowercase(key);
    let mut out = Vec::new();
    for (k, v) in headers.iter() {
        if *k == key_lower {
            out.push(v.as_str());
        }
    }
    out
}

fn http_cookie_domain_matches(host: &str, cookie_domain: &str, host_only: bool) -> bool {
    let host_l = ascii_lowercase(host);
    let dom_l = ascii_lowercase(cookie_domain);
    if host_only {
        return host_l == dom_l;
    }
    if host_l == dom_l {
        return true;
    }
    host_l.ends_with(format!(".{}", dom_l).as_str())
}

fn http_cookie_path_matches(request_path: &str, cookie_path: &str) -> bool {
    if cookie_path.is_empty() || cookie_path == "/" {
        return true;
    }
    if !request_path.starts_with(cookie_path) {
        return false;
    }
    if request_path.len() == cookie_path.len() {
        return true;
    }
    cookie_path.ends_with('/') || request_path.as_bytes()[cookie_path.len()] == b'/'
}

fn http_cookie_default_path(request_path: &str) -> String {
    if !request_path.starts_with('/') {
        return String::from("/");
    }
    if request_path == "/" {
        return String::from("/");
    }
    let bytes = request_path.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        if bytes[i] == b'/' {
            if i == 0 {
                return String::from("/");
            }
            return String::from(&request_path[..i]);
        }
    }
    String::from("/")
}

fn http_parse_set_cookie(
    value: &str,
    request_host: &str,
    request_path: &str,
    is_https: bool,
    now_ticks: u64,
) -> Option<HttpCookieEntry> {
    let mut parts = value.split(';');
    let first = parts.next()?.trim();
    let (name_raw, value_raw) = first.split_once('=')?;
    let name = name_raw.trim();
    if name.is_empty() {
        return None;
    }

    let mut cookie = HttpCookieEntry {
        name: String::from(name),
        value: String::from(value_raw.trim()),
        domain: ascii_lowercase(request_host),
        path: http_cookie_default_path(request_path),
        secure: false,
        host_only: true,
        expires_at_ticks: None,
    };

    for attr in parts {
        let token = attr.trim();
        if token.is_empty() {
            continue;
        }
        if token.eq_ignore_ascii_case("secure") {
            cookie.secure = true;
            continue;
        }
        if token.eq_ignore_ascii_case("httponly") {
            continue;
        }
        if let Some((k, v)) = token.split_once('=') {
            let key = ascii_lowercase(k.trim());
            let val = v.trim();
            match key.as_str() {
                "domain" => {
                    let normalized = val.trim_start_matches('.');
                    if normalized.is_empty() {
                        return None;
                    }
                    if !http_cookie_domain_matches(request_host, normalized, false) {
                        return None;
                    }
                    cookie.domain = ascii_lowercase(normalized);
                    cookie.host_only = false;
                }
                "path" => {
                    if !val.is_empty() && val.starts_with('/') {
                        cookie.path = String::from(val);
                    }
                }
                "max-age" => {
                    if let Ok(seconds) = val.parse::<i64>() {
                        if seconds <= 0 {
                            cookie.expires_at_ticks = Some(now_ticks);
                        } else {
                            let ttl = (seconds as u64).saturating_mul(100);
                            cookie.expires_at_ticks = Some(now_ticks.saturating_add(ttl));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if cookie.secure && !is_https {
        return None;
    }
    Some(cookie)
}

fn http_cookie_prune_expired(now_ticks: u64) {
    unsafe {
        let mut i = 0usize;
        while i < HTTP_COOKIE_JAR.len() {
            let expired = HTTP_COOKIE_JAR[i]
                .expires_at_ticks
                .map(|t| now_ticks >= t)
                .unwrap_or(false);
            if expired {
                HTTP_COOKIE_JAR.remove(i);
            } else {
                i += 1;
            }
        }
    }
}

fn http_cookie_store(cookie: HttpCookieEntry) {
    unsafe {
        let mut replace_idx = None;
        for (idx, existing) in HTTP_COOKIE_JAR.iter().enumerate() {
            if existing.name == cookie.name
                && existing.domain == cookie.domain
                && existing.path == cookie.path
            {
                replace_idx = Some(idx);
                break;
            }
        }

        if let Some(idx) = replace_idx {
            HTTP_COOKIE_JAR[idx] = cookie;
            return;
        }

        if HTTP_COOKIE_JAR.len() >= HTTP_COOKIE_MAX_ENTRIES {
            HTTP_COOKIE_JAR.remove(0);
        }
        HTTP_COOKIE_JAR.push(cookie);
    }
}

fn http_collect_cookie_header(host: &str, path: &str, is_https: bool, now_ticks: u64) -> Option<String> {
    http_cookie_prune_expired(now_ticks);
    let mut parts: Vec<String> = Vec::new();
    unsafe {
        for cookie in HTTP_COOKIE_JAR.iter() {
            if cookie.secure && !is_https {
                continue;
            }
            if !http_cookie_domain_matches(host, cookie.domain.as_str(), cookie.host_only) {
                continue;
            }
            if !http_cookie_path_matches(path, cookie.path.as_str()) {
                continue;
            }
            parts.push(format!("{}={}", cookie.name, cookie.value));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

fn http_ingest_set_cookie_headers(
    headers: &[(String, String)],
    request_host: &str,
    request_path: &str,
    is_https: bool,
    now_ticks: u64,
) {
    let set_cookie_values = header_values(headers, "set-cookie");
    for value in set_cookie_values.into_iter() {
        if let Some(cookie) = http_parse_set_cookie(value, request_host, request_path, is_https, now_ticks) {
            http_cookie_store(cookie);
        }
    }
}

fn http_cache_lookup_index(url: &str) -> Option<usize> {
    unsafe {
        for (idx, entry) in HTTP_CACHE.iter().enumerate() {
            if entry.url == url {
                return Some(idx);
            }
        }
    }
    None
}

fn http_cache_request_hints(url: &str, host: &str, path: &str, is_https: bool, now_ticks: u64) -> HttpRequestHints {
    let mut hints = HttpRequestHints::default();
    hints.cookie_header = http_collect_cookie_header(host, path, is_https, now_ticks);
    if let Some(idx) = http_cache_lookup_index(url) {
        unsafe {
            let entry = &HTTP_CACHE[idx];
            hints.if_none_match = entry.etag.clone();
            hints.if_modified_since = entry.last_modified.clone();
        }
        println("Net: HTTP cache conditional revalidate enabled.");
    }
    hints
}

fn http_cache_get_response(url: &str) -> Option<Vec<u8>> {
    let idx = http_cache_lookup_index(url)?;
    unsafe { Some(HTTP_CACHE[idx].response_bytes.clone()) }
}

fn http_cache_store_response(url: &str, parsed: &ParsedHttpHeaders, response: &[u8], now_ticks: u64) {
    if response.len() > HTTP_CACHE_MAX_RESPONSE_BYTES {
        return;
    }
    let cache_control = header_first(parsed.headers.as_slice(), "cache-control")
        .map(|v| ascii_lowercase(v))
        .unwrap_or_else(String::new);
    if cache_control.contains("no-store") {
        return;
    }

    let entry = HttpCacheEntry {
        url: String::from(url),
        etag: header_first(parsed.headers.as_slice(), "etag").map(String::from),
        last_modified: header_first(parsed.headers.as_slice(), "last-modified").map(String::from),
        response_bytes: response.to_vec(),
        stored_at_ticks: now_ticks,
    };

    unsafe {
        if let Some(idx) = http_cache_lookup_index(url) {
            HTTP_CACHE[idx] = entry;
            println("Net: HTTP cache updated.");
            return;
        }
        if HTTP_CACHE.len() >= HTTP_CACHE_MAX_ENTRIES {
            HTTP_CACHE.remove(0);
        }
        HTTP_CACHE.push(entry);
        println("Net: HTTP cache stored.");
    }
}

fn http_decode_chunked_body(body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < body.len() {
        let line_end_rel = body[i..]
            .windows(2)
            .position(|w| w == b"\r\n")
            .or_else(|| body[i..].iter().position(|b| *b == b'\n'))?;
        let line_end = if body.get(i + line_end_rel + 1) == Some(&b'\n')
            && body.get(i + line_end_rel) == Some(&b'\r')
        {
            i + line_end_rel
        } else {
            i + line_end_rel
        };
        let line_bytes = if body.get(line_end) == Some(&b'\r') {
            &body[i..line_end]
        } else {
            &body[i..=line_end]
        };
        let line_text = str::from_utf8(line_bytes).ok()?.trim();
        let size_hex = line_text.split(';').next().unwrap_or("").trim();
        let chunk_size = usize::from_str_radix(size_hex, 16).ok()?;

        let after_line = if body.get(line_end) == Some(&b'\r') {
            line_end + 2
        } else {
            line_end + 1
        };

        if chunk_size == 0 {
            return Some(out);
        }
        let chunk_end = after_line.saturating_add(chunk_size);
        if chunk_end > body.len() {
            return None;
        }
        out.extend_from_slice(&body[after_line..chunk_end]);
        i = chunk_end;
        if i + 1 < body.len() && body[i] == b'\r' && body[i + 1] == b'\n' {
            i += 2;
        } else if i < body.len() && body[i] == b'\n' {
            i += 1;
        }
    }
    Some(out)
}

fn http_decode_gzip_body(body: &[u8]) -> Option<Vec<u8>> {
    if body.len() < 18 {
        return None;
    }
    if body[0] != 0x1F || body[1] != 0x8B || body[2] != 0x08 {
        return None;
    }
    let flg = body[3];
    let mut idx = 10usize;
    if (flg & 0x04) != 0 {
        if idx + 2 > body.len() {
            return None;
        }
        let xlen = (body[idx] as usize) | ((body[idx + 1] as usize) << 8);
        idx = idx.saturating_add(2).saturating_add(xlen);
    }
    if (flg & 0x08) != 0 {
        while idx < body.len() && body[idx] != 0 {
            idx += 1;
        }
        idx = idx.saturating_add(1);
    }
    if (flg & 0x10) != 0 {
        while idx < body.len() && body[idx] != 0 {
            idx += 1;
        }
        idx = idx.saturating_add(1);
    }
    if (flg & 0x02) != 0 {
        idx = idx.saturating_add(2);
    }
    if idx >= body.len().saturating_sub(8) {
        return None;
    }
    let deflate_stream = &body[idx..body.len() - 8];
    decompress_to_vec(deflate_stream).ok()
}

fn http_decode_deflate_body(body: &[u8]) -> Option<Vec<u8>> {
    decompress_to_vec_zlib(body).ok().or_else(|| decompress_to_vec(body).ok())
}

fn http_decode_entity_body(parsed: &ParsedHttpHeaders, body: &[u8]) -> Option<(Vec<u8>, bool)> {
    let mut decoded = body.to_vec();
    let mut changed = false;

    if let Some(te) = header_first(parsed.headers.as_slice(), "transfer-encoding") {
        let te_lower = ascii_lowercase(te);
        if te_lower.contains("chunked") {
            let chunked = http_decode_chunked_body(decoded.as_slice())?;
            decoded = chunked;
            changed = true;
        }
    }

    if let Some(ce) = header_first(parsed.headers.as_slice(), "content-encoding") {
        let ce_lower = ascii_lowercase(ce);
        if ce_lower.contains("gzip") {
            let plain = http_decode_gzip_body(decoded.as_slice())?;
            decoded = plain;
            changed = true;
        } else if ce_lower.contains("deflate") {
            let plain = http_decode_deflate_body(decoded.as_slice())?;
            decoded = plain;
            changed = true;
        }
    }

    Some((decoded, changed))
}

fn http_rebuild_response(
    parsed: &ParsedHttpHeaders,
    body: &[u8],
    strip_transfer_encoding: bool,
    strip_content_encoding: bool,
) -> Vec<u8> {
    let status_line = parsed
        .status_line
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("HTTP/1.1 200 OK");
    let mut out = Vec::new();
    out.extend_from_slice(status_line.as_bytes());
    out.extend_from_slice(b"\r\n");

    for (k, v) in parsed.headers.iter() {
        if strip_transfer_encoding && *k == "transfer-encoding" {
            continue;
        }
        if strip_content_encoding && *k == "content-encoding" {
            continue;
        }
        if *k == "content-length" {
            continue;
        }
        out.extend_from_slice(k.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(v.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    let len_line = format!("content-length: {}", body.len());
    out.extend_from_slice(len_line.as_bytes());
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(body);
    out
}

fn http_postprocess_response(
    effective_url: &str,
    request_host: &str,
    request_path: &str,
    is_https: bool,
    mut response: Vec<u8>,
    now_ticks: u64,
) -> Vec<u8> {
    let parsed = parse_http_headers(response.as_slice());
    if parsed.body_offset == 0 || parsed.status_line.is_none() {
        return response;
    }

    http_ingest_set_cookie_headers(
        parsed.headers.as_slice(),
        request_host,
        request_path,
        is_https,
        now_ticks,
    );

    if parsed.status_code == Some(304) {
        if let Some(cached) = http_cache_get_response(effective_url) {
            println("Net: HTTP cache hit (304 -> cached response).");
            return cached;
        }
        return response;
    }

    let body = response.get(parsed.body_offset..).unwrap_or(&[]);
    if let Some((decoded_body, changed)) = http_decode_entity_body(&parsed, body) {
        if changed {
            println("Net: HTTP body decoded (chunked/compressed).");
            response = http_rebuild_response(&parsed, decoded_body.as_slice(), true, true);
        }
    }

    let parsed_after = parse_http_headers(response.as_slice());
    if parsed_after.status_code == Some(200) {
        http_cache_store_response(effective_url, &parsed_after, response.as_slice(), now_ticks);
    }

    response
}

fn http_read_http1_response(
    iface: &mut Interface,
    sockets: &mut SocketSet<'_>,
    handle: smoltcp::iface::SocketHandle,
    pump_ui: &mut impl FnMut(),
    timeout_ticks: u64,
) -> Option<(Vec<u8>, bool)> {
    let start_read = crate::timer::ticks();
    let mut response = Vec::new();

    let mut header_parsed = false;
    let mut body_offset = 0usize;
    let mut status_no_body = false;
    let mut chunked = false;
    let mut content_length: Option<usize> = None;
    let mut keepalive_allowed = false;

    loop {
        pump_ui();
        crate::timer::on_tick();
        let timestamp = Instant::from_millis(crate::timer::ticks() as i64 * 10);
        let mut phy = if unsafe { crate::intel_net::GLOBAL_INTEL_NET.is_some() } {
            ReduxPhy::Intel(crate::intel_net::IntelPhy)
        } else {
            ReduxPhy::Virtio(VirtioPhy)
        };
        iface.poll(timestamp, &mut phy, sockets);

        let mut bytes_read = 0usize;
        let socket_is_open = {
            let socket = sockets.get_mut::<tcp::Socket>(handle);
            if socket.can_recv() {
                if let Ok(read_len) = socket.recv(|data| {
                    response.extend_from_slice(data);
                    (data.len(), data.len())
                }) {
                    bytes_read = read_len;
                }
            }
            socket.is_open()
        };

        if !header_parsed {
            let parsed = parse_http_headers(response.as_slice());
            if parsed.body_offset != 0 && parsed.status_line.is_some() {
                header_parsed = true;
                body_offset = parsed.body_offset;
                status_no_body = http_status_has_no_body(parsed.status_code);
                chunked = header_first(parsed.headers.as_slice(), "transfer-encoding")
                    .map(|v| ascii_lowercase(v).contains("chunked"))
                    .unwrap_or(false);
                content_length = header_first(parsed.headers.as_slice(), "content-length")
                    .and_then(|v| v.trim().parse::<usize>().ok());
                let delimit_known = status_no_body || chunked || content_length.is_some();
                keepalive_allowed = delimit_known && http_response_allows_keepalive(&parsed);
            }
        }

        if header_parsed {
            let body = response.get(body_offset..).unwrap_or(&[]);
            let complete = if status_no_body {
                true
            } else if chunked {
                http_decode_chunked_body(body).is_some()
            } else if let Some(len) = content_length {
                body.len() >= len
            } else {
                false
            };
            if complete {
                return Some((response, keepalive_allowed));
            }
        }

        if !socket_is_open {
            if response.is_empty() {
                return None;
            }
            return Some((response, false));
        }

        if crate::timer::ticks().saturating_sub(start_read) > timeout_ticks {
            if response.is_empty() {
                return None;
            }
            return Some((response, false));
        }

        if bytes_read == 0 {
            pump_ui();
            uefi::boot::stall(NET_BLOCKING_LOOP_STALL_US);
        }
    }
}

const HTTP2_CLIENT_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
const HTTP2_FRAME_DATA: u8 = 0x0;
const HTTP2_FRAME_HEADERS: u8 = 0x1;
const HTTP2_FRAME_SETTINGS: u8 = 0x4;
const HTTP2_FRAME_PING: u8 = 0x6;
const HTTP2_FRAME_GOAWAY: u8 = 0x7;
const HTTP2_FRAME_WINDOW_UPDATE: u8 = 0x8;
const HTTP2_FRAME_CONTINUATION: u8 = 0x9;
const HTTP2_FRAME_RST_STREAM: u8 = 0x3;
const HTTP2_FLAG_END_STREAM: u8 = 0x1;
const HTTP2_FLAG_ACK: u8 = 0x1;
const HTTP2_FLAG_END_HEADERS: u8 = 0x4;
const HTTP2_FLAG_PADDED: u8 = 0x8;
const HTTP2_FLAG_PRIORITY: u8 = 0x20;
const HTTP2_DEFAULT_STREAM_WINDOW: i64 = 65_535;
const HTTP2_WINDOW_UPDATE_STEP: u32 = 16 * 1024;
const HTTP2_WINDOW_UPDATE_LOW_WATERMARK: i64 = 16 * 1024;
const HTTP2_DEFAULT_MAX_CONCURRENT_STREAMS: u32 = 100;
const HTTP2_ERR_PROTOCOL: u32 = 0x1;
const HTTP2_ERR_FLOW_CONTROL: u32 = 0x3;
const HTTP2_ERR_STREAM_CLOSED: u32 = 0x5;
const HTTP2_ERR_REFUSED_STREAM: u32 = 0x7;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Http2StreamState {
    Idle,
    Open,
    HalfClosedLocal,
    HalfClosedRemote,
    Closed,
}

struct Http2StreamCollector {
    status: Option<u16>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    partial_header_block: Vec<u8>,
    state: Http2StreamState,
    local_initiated: bool,
    recv_window: i64,
    consumed_since_update: u32,
    reset_error_code: Option<u32>,
}

impl Http2StreamCollector {
    fn new(initial_window: i64, local_initiated: bool) -> Self {
        Self {
            status: None,
            headers: Vec::new(),
            body: Vec::new(),
            partial_header_block: Vec::new(),
            state: Http2StreamState::Idle,
            local_initiated,
            recv_window: initial_window,
            consumed_since_update: 0,
            reset_error_code: None,
        }
    }

    fn is_closed(&self) -> bool {
        self.state == Http2StreamState::Closed
    }

    fn can_receive_response_headers(&self) -> bool {
        matches!(
            self.state,
            Http2StreamState::Open | Http2StreamState::HalfClosedLocal
        )
    }

    fn can_receive_response_data(&self) -> bool {
        self.can_receive_response_headers()
    }

    fn mark_local_headers_sent(&mut self, end_stream: bool) {
        self.state = if end_stream {
            Http2StreamState::HalfClosedLocal
        } else {
            Http2StreamState::Open
        };
    }

    fn mark_remote_end_stream(&mut self) {
        self.state = match self.state {
            Http2StreamState::Open => Http2StreamState::HalfClosedRemote,
            Http2StreamState::HalfClosedLocal => Http2StreamState::Closed,
            Http2StreamState::HalfClosedRemote => Http2StreamState::Closed,
            Http2StreamState::Closed => Http2StreamState::Closed,
            Http2StreamState::Idle => Http2StreamState::Closed,
        };
    }

    fn force_close(&mut self) {
        self.state = Http2StreamState::Closed;
    }
}

struct Http2ResponseCollector {
    target_stream_id: u32,
    streams: BTreeMap<u32, Http2StreamCollector>,
    partial_header_stream: u32,
    dynamic_table: HpackDynamicTable,
    outbound_dynamic_table: HpackDynamicTable,
    use_hpack_huffman: bool,
    connection_closed: bool,
    next_local_stream_id: u32,
    highest_local_stream_id: u32,
    conn_recv_window: i64,
    conn_consumed_since_update: u32,
    peer_conn_send_window: i64,
    peer_initial_stream_send_window: i64,
    peer_max_concurrent_streams: u32,
    peer_stream_send_windows: BTreeMap<u32, i64>,
}

impl Http2ResponseCollector {
    fn new() -> Self {
        Self {
            target_stream_id: 0,
            streams: BTreeMap::new(),
            partial_header_stream: 0,
            dynamic_table: HpackDynamicTable::new(),
            outbound_dynamic_table: HpackDynamicTable::new(),
            use_hpack_huffman: true,
            connection_closed: false,
            next_local_stream_id: 1,
            highest_local_stream_id: 0,
            conn_recv_window: HTTP2_DEFAULT_STREAM_WINDOW,
            conn_consumed_since_update: 0,
            peer_conn_send_window: HTTP2_DEFAULT_STREAM_WINDOW,
            peer_initial_stream_send_window: HTTP2_DEFAULT_STREAM_WINDOW,
            peer_max_concurrent_streams: HTTP2_DEFAULT_MAX_CONCURRENT_STREAMS,
            peer_stream_send_windows: BTreeMap::new(),
        }
    }

    fn create_stream(&mut self, stream_id: u32, local_initiated: bool) -> &mut Http2StreamCollector {
        let initial_window = HTTP2_DEFAULT_STREAM_WINDOW;
        self.streams
            .entry(stream_id)
            .or_insert_with(|| Http2StreamCollector::new(initial_window, local_initiated))
    }

    fn stream_mut(&mut self, stream_id: u32) -> Option<&mut Http2StreamCollector> {
        self.streams.get_mut(&stream_id)
    }

    fn active_local_streams(&self) -> usize {
        self.streams
            .values()
            .filter(|s| s.local_initiated && !s.is_closed())
            .count()
    }

    fn can_open_local_stream(&self) -> bool {
        (self.active_local_streams() as u32) < self.peer_max_concurrent_streams
    }

    fn open_local_get_stream(&mut self, end_stream: bool) -> Option<u32> {
        if !self.can_open_local_stream() {
            return None;
        }
        let stream_id = self.next_local_stream_id;
        if stream_id == 0 || (stream_id & 1) == 0 {
            return None;
        }
        self.next_local_stream_id = self.next_local_stream_id.saturating_add(2);
        self.highest_local_stream_id = core::cmp::max(self.highest_local_stream_id, stream_id);

        let stream = self.create_stream(stream_id, true);
        stream.mark_local_headers_sent(end_stream);
        self.peer_stream_send_windows
            .entry(stream_id)
            .or_insert(self.peer_initial_stream_send_window);

        if self.target_stream_id == 0 {
            self.target_stream_id = stream_id;
        }
        Some(stream_id)
    }

    fn mark_stream_closed(&mut self, stream_id: u32) {
        let stream = self.create_stream(stream_id, (stream_id & 1) == 1);
        stream.force_close();
    }

    fn target_stream_closed(&self) -> bool {
        if self.connection_closed {
            return true;
        }
        if self.target_stream_id == 0 {
            return false;
        }
        self.streams
            .get(&self.target_stream_id)
            .map(|s| s.is_closed())
            .unwrap_or(false)
    }

    fn target_body_is_empty(&self) -> bool {
        self.streams
            .get(&self.target_stream_id)
            .map(|s| s.body.is_empty())
            .unwrap_or(true)
    }

    fn target_status(&self) -> Option<u16> {
        self.streams
            .get(&self.target_stream_id)
            .and_then(|s| s.status)
    }

    fn target_headers(&self) -> Option<&[(String, String)]> {
        self.streams
            .get(&self.target_stream_id)
            .map(|s| s.headers.as_slice())
    }

    fn target_body(&self) -> Option<&[u8]> {
        self.streams
            .get(&self.target_stream_id)
            .map(|s| s.body.as_slice())
    }
}

const HPACK_DYNAMIC_TABLE_DEFAULT_MAX: usize = 4096;
const HPACK_DYNAMIC_TABLE_ABSOLUTE_MAX: usize = 64 * 1024;

#[derive(Clone)]
struct HpackDynamicEntry {
    name: String,
    value: String,
    size: usize,
}

#[derive(Clone)]
struct HpackDynamicTable {
    entries: Vec<HpackDynamicEntry>,
    size: usize,
    max_size: usize,
}

impl HpackDynamicTable {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            size: 0,
            max_size: HPACK_DYNAMIC_TABLE_DEFAULT_MAX,
        }
    }

    fn set_max_size(&mut self, max_size: usize) {
        self.max_size = core::cmp::min(max_size, HPACK_DYNAMIC_TABLE_ABSOLUTE_MAX);
        self.evict_to_fit();
    }

    fn evict_to_fit(&mut self) {
        while self.size > self.max_size {
            if let Some(last) = self.entries.pop() {
                self.size = self.size.saturating_sub(last.size);
            } else {
                self.size = 0;
                break;
            }
        }
    }

    fn insert(&mut self, name: String, value: String) {
        let entry_size = name.len().saturating_add(value.len()).saturating_add(32);
        if entry_size > self.max_size {
            self.entries.clear();
            self.size = 0;
            return;
        }

        while self.size.saturating_add(entry_size) > self.max_size {
            if let Some(last) = self.entries.pop() {
                self.size = self.size.saturating_sub(last.size);
            } else {
                self.size = 0;
                break;
            }
        }

        self.entries.insert(
            0,
            HpackDynamicEntry {
                name,
                value,
                size: entry_size,
            },
        );
        self.size = self.size.saturating_add(entry_size);
    }

    fn get(&self, absolute_index: u32) -> Option<(&str, &str)> {
        if absolute_index <= 61 {
            return None;
        }
        let dyn_index = (absolute_index as usize).checked_sub(62)?;
        let entry = self.entries.get(dyn_index)?;
        Some((entry.name.as_str(), entry.value.as_str()))
    }
}

fn http2_reason_phrase(code: u16) -> &'static str {
    match code {
        200 => "OK",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    }
}

fn http2_build_frame(
    frame_type: u8,
    flags: u8,
    stream_id: u32,
    payload: &[u8],
    out: &mut Vec<u8>,
) -> bool {
    if payload.len() > 0x00FF_FFFF {
        return false;
    }
    let len = payload.len() as u32;
    out.push(((len >> 16) & 0xFF) as u8);
    out.push(((len >> 8) & 0xFF) as u8);
    out.push((len & 0xFF) as u8);
    out.push(frame_type);
    out.push(flags);
    out.push(((stream_id >> 24) & 0x7F) as u8);
    out.push(((stream_id >> 16) & 0xFF) as u8);
    out.push(((stream_id >> 8) & 0xFF) as u8);
    out.push((stream_id & 0xFF) as u8);
    out.extend_from_slice(payload);
    true
}

fn http2_queue_window_update(
    pending_tx_frames: &mut Vec<Vec<u8>>,
    stream_id: u32,
    increment: u32,
) {
    if increment == 0 || increment > 0x7FFF_FFFF {
        return;
    }
    let payload = [
        ((increment >> 24) & 0x7F) as u8,
        ((increment >> 16) & 0xFF) as u8,
        ((increment >> 8) & 0xFF) as u8,
        (increment & 0xFF) as u8,
    ];
    let mut frame = Vec::new();
    if http2_build_frame(
        HTTP2_FRAME_WINDOW_UPDATE,
        0,
        stream_id,
        payload.as_slice(),
        &mut frame,
    ) {
        pending_tx_frames.push(frame);
    }
}

fn http2_queue_rst_stream(
    pending_tx_frames: &mut Vec<Vec<u8>>,
    stream_id: u32,
    error_code: u32,
) {
    if stream_id == 0 {
        return;
    }
    let payload = [
        ((error_code >> 24) & 0xFF) as u8,
        ((error_code >> 16) & 0xFF) as u8,
        ((error_code >> 8) & 0xFF) as u8,
        (error_code & 0xFF) as u8,
    ];
    let mut frame = Vec::new();
    if http2_build_frame(
        HTTP2_FRAME_RST_STREAM,
        0,
        stream_id,
        payload.as_slice(),
        &mut frame,
    ) {
        pending_tx_frames.push(frame);
    }
}

fn http2_replenish_connection_window_if_needed(
    response: &mut Http2ResponseCollector,
    force: bool,
    pending_tx_frames: &mut Vec<Vec<u8>>,
) {
    let should_send = response.conn_consumed_since_update >= HTTP2_WINDOW_UPDATE_STEP
        || response.conn_recv_window <= HTTP2_WINDOW_UPDATE_LOW_WATERMARK
        || force;
    if !should_send || response.conn_consumed_since_update == 0 {
        return;
    }
    let increment = response.conn_consumed_since_update;
    response.conn_recv_window = response.conn_recv_window.saturating_add(increment as i64);
    response.conn_consumed_since_update = 0;
    http2_queue_window_update(pending_tx_frames, 0, increment);
}

fn http2_replenish_stream_window_if_needed(
    stream: &mut Http2StreamCollector,
    stream_id: u32,
    force: bool,
    pending_tx_frames: &mut Vec<Vec<u8>>,
) {
    let should_send = stream.consumed_since_update >= HTTP2_WINDOW_UPDATE_STEP
        || stream.recv_window <= HTTP2_WINDOW_UPDATE_LOW_WATERMARK
        || force;
    if !should_send || stream.consumed_since_update == 0 {
        return;
    }
    let increment = stream.consumed_since_update;
    stream.recv_window = stream.recv_window.saturating_add(increment as i64);
    stream.consumed_since_update = 0;
    http2_queue_window_update(pending_tx_frames, stream_id, increment);
}

fn http2_handle_peer_window_update(
    response: &mut Http2ResponseCollector,
    stream_id: u32,
    increment: u32,
) {
    if stream_id == 0 {
        response.peer_conn_send_window = response
            .peer_conn_send_window
            .saturating_add(increment as i64);
        return;
    }
    let win = response
        .peer_stream_send_windows
        .entry(stream_id)
        .or_insert(response.peer_initial_stream_send_window);
    *win = win.saturating_add(increment as i64);
}

fn http2_apply_peer_settings(response: &mut Http2ResponseCollector, payload: &[u8]) {
    let mut idx = 0usize;
    while idx + 6 <= payload.len() {
        let id = ((payload[idx] as u16) << 8) | payload[idx + 1] as u16;
        let value = ((payload[idx + 2] as u32) << 24)
            | ((payload[idx + 3] as u32) << 16)
            | ((payload[idx + 4] as u32) << 8)
            | payload[idx + 5] as u32;
        idx += 6;

        // SETTINGS_INITIAL_WINDOW_SIZE
        if id == 0x4 {
            if value > 0x7FFF_FFFF {
                response.connection_closed = true;
                return;
            }
            let next = value as i64;
            let delta = next.saturating_sub(response.peer_initial_stream_send_window);
            response.peer_initial_stream_send_window = next;
            for (_, win) in response.peer_stream_send_windows.iter_mut() {
                *win = win.saturating_add(delta);
            }
        } else if id == 0x3 {
            response.peer_max_concurrent_streams = value;
        }
    }
}

fn hpack_encode_prefixed_integer(out: &mut Vec<u8>, first_prefix: u8, prefix_bits: u8, value: u32) {
    let max_prefix = (1u32 << prefix_bits) - 1;
    if value < max_prefix {
        out.push(first_prefix | value as u8);
        return;
    }

    out.push(first_prefix | max_prefix as u8);
    let mut remaining = value - max_prefix;
    while remaining >= 128 {
        out.push(((remaining as u8) & 0x7F) | 0x80);
        remaining >>= 7;
    }
    out.push(remaining as u8);
}

fn hpack_encode_string_no_huffman(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    hpack_encode_prefixed_integer(out, 0x00, 7, bytes.len() as u32);
    out.extend_from_slice(bytes);
}

fn hpack_encode_string_huffman(out: &mut Vec<u8>, value: &str) {
    let mut encoded = Vec::new();
    let mut bit_buffer: u64 = 0;
    let mut bit_count: u32 = 0;

    for &byte in value.as_bytes().iter() {
        let (nbits, code) = HPACK_HUFFMAN_CODES[byte as usize];
        bit_buffer = (bit_buffer << nbits) | code as u64;
        bit_count = bit_count.saturating_add(nbits as u32);

        while bit_count >= 8 {
            let shift = bit_count - 8;
            encoded.push(((bit_buffer >> shift) & 0xFF) as u8);
            bit_count -= 8;
            if bit_count == 0 {
                bit_buffer = 0;
            } else {
                bit_buffer &= (1u64 << bit_count) - 1;
            }
        }
    }

    if bit_count > 0 {
        let pad_bits = 8 - bit_count;
        let padded = (bit_buffer << pad_bits) | ((1u64 << pad_bits) - 1);
        encoded.push((padded & 0xFF) as u8);
    }

    hpack_encode_prefixed_integer(out, 0x80, 7, encoded.len() as u32);
    out.extend_from_slice(encoded.as_slice());
}

fn hpack_encode_string(out: &mut Vec<u8>, value: &str, use_huffman: bool) {
    if use_huffman {
        hpack_encode_string_huffman(out, value);
    } else {
        hpack_encode_string_no_huffman(out, value);
    }
}

fn hpack_encode_indexed_header(out: &mut Vec<u8>, index: u32) {
    hpack_encode_prefixed_integer(out, 0x80, 7, index);
}

fn hpack_find_header_exact_index(
    dynamic: &HpackDynamicTable,
    name: &str,
    value: &str,
) -> Option<u32> {
    for idx in 1..=61u32 {
        if let Some((n, v)) = hpack_static_table(idx) {
            if n == name && v == value {
                return Some(idx);
            }
        }
    }
    for (i, entry) in dynamic.entries.iter().enumerate() {
        if entry.name == name && entry.value == value {
            return Some(62u32.saturating_add(i as u32));
        }
    }
    None
}

fn hpack_find_header_name_index(dynamic: &HpackDynamicTable, name: &str) -> Option<u32> {
    for idx in 1..=61u32 {
        if let Some((n, _)) = hpack_static_table(idx) {
            if n == name {
                return Some(idx);
            }
        }
    }
    for (i, entry) in dynamic.entries.iter().enumerate() {
        if entry.name == name {
            return Some(62u32.saturating_add(i as u32));
        }
    }
    None
}

fn hpack_encode_literal_header(
    out: &mut Vec<u8>,
    dynamic: &mut HpackDynamicTable,
    name: &str,
    value: &str,
    incremental_indexing: bool,
    use_huffman: bool,
) {
    let name_index = hpack_find_header_name_index(dynamic, name).unwrap_or(0);
    if incremental_indexing {
        hpack_encode_prefixed_integer(out, 0x40, 6, name_index);
        if name_index == 0 {
            hpack_encode_string(out, name, use_huffman);
        }
        hpack_encode_string(out, value, use_huffman);
        dynamic.insert(String::from(name), String::from(value));
    } else {
        hpack_encode_prefixed_integer(out, 0x00, 4, name_index);
        if name_index == 0 {
            hpack_encode_string(out, name, use_huffman);
        }
        hpack_encode_string(out, value, use_huffman);
    }
}

fn hpack_should_index_request_header(name: &str) -> bool {
    matches!(
        name,
        ":authority" | "accept" | "accept-language" | "accept-encoding" | "user-agent"
    )
}

fn hpack_encode_request_header(
    out: &mut Vec<u8>,
    dynamic: &mut HpackDynamicTable,
    name: &str,
    value: &str,
    use_huffman: bool,
) {
    if let Some(index) = hpack_find_header_exact_index(dynamic, name, value) {
        hpack_encode_indexed_header(out, index);
        return;
    }
    hpack_encode_literal_header(
        out,
        dynamic,
        name,
        value,
        hpack_should_index_request_header(name),
        use_huffman,
    );
}

fn hpack_encode_request_headers(
    dynamic: &mut HpackDynamicTable,
    host: &str,
    path: &str,
    is_https: bool,
    use_huffman: bool,
    hints: &HttpRequestHints,
) -> Vec<u8> {
    let mut out = Vec::new();

    hpack_encode_request_header(&mut out, dynamic, ":method", "GET", use_huffman);
    hpack_encode_request_header(
        &mut out,
        dynamic,
        ":scheme",
        if is_https { "https" } else { "http" },
        use_huffman,
    );
    hpack_encode_request_header(&mut out, dynamic, ":authority", host, use_huffman);
    if path == "/" {
        hpack_encode_request_header(&mut out, dynamic, ":path", "/", use_huffman);
    } else {
        hpack_encode_request_header(&mut out, dynamic, ":path", path, use_huffman);
    }
    hpack_encode_request_header(&mut out, dynamic, "accept", "*/*", use_huffman);
    hpack_encode_request_header(
        &mut out,
        dynamic,
        "accept-language",
        "en-US,en;q=0.9,es;q=0.8",
        use_huffman,
    );
    hpack_encode_request_header(
        &mut out,
        dynamic,
        "accept-encoding",
        HTTP_ACCEPT_ENCODING_VALUE,
        use_huffman,
    );
    hpack_encode_request_header(&mut out, dynamic, "user-agent", "ReduxOS/0.2 h2", use_huffman);
    if let Some(cookie) = hints.cookie_header.as_ref() {
        hpack_encode_request_header(&mut out, dynamic, "cookie", cookie.as_str(), use_huffman);
    }
    if let Some(etag) = hints.if_none_match.as_ref() {
        hpack_encode_request_header(&mut out, dynamic, "if-none-match", etag.as_str(), use_huffman);
    }
    if let Some(modified) = hints.if_modified_since.as_ref() {
        hpack_encode_request_header(
            &mut out,
            dynamic,
            "if-modified-since",
            modified.as_str(),
            use_huffman,
        );
    }
    out
}

const HPACK_HUFFMAN_CODES: [(u8, u32); 257] = [
    (13, 0x1ff8),
    (23, 0x7fffd8),
    (28, 0xfffffe2),
    (28, 0xfffffe3),
    (28, 0xfffffe4),
    (28, 0xfffffe5),
    (28, 0xfffffe6),
    (28, 0xfffffe7),
    (28, 0xfffffe8),
    (24, 0xffffea),
    (30, 0x3ffffffc),
    (28, 0xfffffe9),
    (28, 0xfffffea),
    (30, 0x3ffffffd),
    (28, 0xfffffeb),
    (28, 0xfffffec),
    (28, 0xfffffed),
    (28, 0xfffffee),
    (28, 0xfffffef),
    (28, 0xffffff0),
    (28, 0xffffff1),
    (28, 0xffffff2),
    (30, 0x3ffffffe),
    (28, 0xffffff3),
    (28, 0xffffff4),
    (28, 0xffffff5),
    (28, 0xffffff6),
    (28, 0xffffff7),
    (28, 0xffffff8),
    (28, 0xffffff9),
    (28, 0xffffffa),
    (28, 0xffffffb),
    (6, 0x14),
    (10, 0x3f8),
    (10, 0x3f9),
    (12, 0xffa),
    (13, 0x1ff9),
    (6, 0x15),
    (8, 0xf8),
    (11, 0x7fa),
    (10, 0x3fa),
    (10, 0x3fb),
    (8, 0xf9),
    (11, 0x7fb),
    (8, 0xfa),
    (6, 0x16),
    (6, 0x17),
    (6, 0x18),
    (5, 0x0),
    (5, 0x1),
    (5, 0x2),
    (6, 0x19),
    (6, 0x1a),
    (6, 0x1b),
    (6, 0x1c),
    (6, 0x1d),
    (6, 0x1e),
    (6, 0x1f),
    (7, 0x5c),
    (8, 0xfb),
    (15, 0x7ffc),
    (6, 0x20),
    (12, 0xffb),
    (10, 0x3fc),
    (13, 0x1ffa),
    (6, 0x21),
    (7, 0x5d),
    (7, 0x5e),
    (7, 0x5f),
    (7, 0x60),
    (7, 0x61),
    (7, 0x62),
    (7, 0x63),
    (7, 0x64),
    (7, 0x65),
    (7, 0x66),
    (7, 0x67),
    (7, 0x68),
    (7, 0x69),
    (7, 0x6a),
    (7, 0x6b),
    (7, 0x6c),
    (7, 0x6d),
    (7, 0x6e),
    (7, 0x6f),
    (7, 0x70),
    (7, 0x71),
    (7, 0x72),
    (8, 0xfc),
    (7, 0x73),
    (8, 0xfd),
    (13, 0x1ffb),
    (19, 0x7fff0),
    (13, 0x1ffc),
    (14, 0x3ffc),
    (6, 0x22),
    (15, 0x7ffd),
    (5, 0x3),
    (6, 0x23),
    (5, 0x4),
    (6, 0x24),
    (5, 0x5),
    (6, 0x25),
    (6, 0x26),
    (6, 0x27),
    (5, 0x6),
    (7, 0x74),
    (7, 0x75),
    (6, 0x28),
    (6, 0x29),
    (6, 0x2a),
    (5, 0x7),
    (6, 0x2b),
    (7, 0x76),
    (6, 0x2c),
    (5, 0x8),
    (5, 0x9),
    (6, 0x2d),
    (7, 0x77),
    (7, 0x78),
    (7, 0x79),
    (7, 0x7a),
    (7, 0x7b),
    (15, 0x7ffe),
    (11, 0x7fc),
    (14, 0x3ffd),
    (13, 0x1ffd),
    (28, 0xffffffc),
    (20, 0xfffe6),
    (22, 0x3fffd2),
    (20, 0xfffe7),
    (20, 0xfffe8),
    (22, 0x3fffd3),
    (22, 0x3fffd4),
    (22, 0x3fffd5),
    (23, 0x7fffd9),
    (22, 0x3fffd6),
    (23, 0x7fffda),
    (23, 0x7fffdb),
    (23, 0x7fffdc),
    (23, 0x7fffdd),
    (23, 0x7fffde),
    (24, 0xffffeb),
    (23, 0x7fffdf),
    (24, 0xffffec),
    (24, 0xffffed),
    (22, 0x3fffd7),
    (23, 0x7fffe0),
    (24, 0xffffee),
    (23, 0x7fffe1),
    (23, 0x7fffe2),
    (23, 0x7fffe3),
    (23, 0x7fffe4),
    (21, 0x1fffdc),
    (22, 0x3fffd8),
    (23, 0x7fffe5),
    (22, 0x3fffd9),
    (23, 0x7fffe6),
    (23, 0x7fffe7),
    (24, 0xffffef),
    (22, 0x3fffda),
    (21, 0x1fffdd),
    (20, 0xfffe9),
    (22, 0x3fffdb),
    (22, 0x3fffdc),
    (23, 0x7fffe8),
    (23, 0x7fffe9),
    (21, 0x1fffde),
    (23, 0x7fffea),
    (22, 0x3fffdd),
    (22, 0x3fffde),
    (24, 0xfffff0),
    (21, 0x1fffdf),
    (22, 0x3fffdf),
    (23, 0x7fffeb),
    (23, 0x7fffec),
    (21, 0x1fffe0),
    (21, 0x1fffe1),
    (22, 0x3fffe0),
    (21, 0x1fffe2),
    (23, 0x7fffed),
    (22, 0x3fffe1),
    (23, 0x7fffee),
    (23, 0x7fffef),
    (20, 0xfffea),
    (22, 0x3fffe2),
    (22, 0x3fffe3),
    (22, 0x3fffe4),
    (23, 0x7ffff0),
    (22, 0x3fffe5),
    (22, 0x3fffe6),
    (23, 0x7ffff1),
    (26, 0x3ffffe0),
    (26, 0x3ffffe1),
    (20, 0xfffeb),
    (19, 0x7fff1),
    (22, 0x3fffe7),
    (23, 0x7ffff2),
    (22, 0x3fffe8),
    (25, 0x1ffffec),
    (26, 0x3ffffe2),
    (26, 0x3ffffe3),
    (26, 0x3ffffe4),
    (27, 0x7ffffde),
    (27, 0x7ffffdf),
    (26, 0x3ffffe5),
    (24, 0xfffff1),
    (25, 0x1ffffed),
    (19, 0x7fff2),
    (21, 0x1fffe3),
    (26, 0x3ffffe6),
    (27, 0x7ffffe0),
    (27, 0x7ffffe1),
    (26, 0x3ffffe7),
    (27, 0x7ffffe2),
    (24, 0xfffff2),
    (21, 0x1fffe4),
    (21, 0x1fffe5),
    (26, 0x3ffffe8),
    (26, 0x3ffffe9),
    (28, 0xffffffd),
    (27, 0x7ffffe3),
    (27, 0x7ffffe4),
    (27, 0x7ffffe5),
    (20, 0xfffec),
    (24, 0xfffff3),
    (20, 0xfffed),
    (21, 0x1fffe6),
    (22, 0x3fffe9),
    (21, 0x1fffe7),
    (21, 0x1fffe8),
    (23, 0x7ffff3),
    (22, 0x3fffea),
    (22, 0x3fffeb),
    (25, 0x1ffffee),
    (25, 0x1ffffef),
    (24, 0xfffff4),
    (24, 0xfffff5),
    (26, 0x3ffffea),
    (23, 0x7ffff4),
    (26, 0x3ffffeb),
    (27, 0x7ffffe6),
    (26, 0x3ffffec),
    (26, 0x3ffffed),
    (27, 0x7ffffe7),
    (27, 0x7ffffe8),
    (27, 0x7ffffe9),
    (27, 0x7ffffea),
    (27, 0x7ffffeb),
    (28, 0xffffffe),
    (27, 0x7ffffec),
    (27, 0x7ffffed),
    (27, 0x7ffffee),
    (27, 0x7ffffef),
    (27, 0x7fffff0),
    (26, 0x3ffffee),
    (30, 0x3fffffff),
];

fn hpack_huffman_lookup_symbol(prefix: u32, bit_len: u8) -> Option<u16> {
    let mut sym = 0usize;
    while sym < HPACK_HUFFMAN_CODES.len() {
        let (len, code) = HPACK_HUFFMAN_CODES[sym];
        if len == bit_len && code == prefix {
            return Some(sym as u16);
        }
        sym += 1;
    }
    None
}

fn hpack_decode_huffman(raw: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(raw.len());
    let mut acc: u64 = 0;
    let mut bit_len: u8 = 0;

    for byte in raw.iter().copied() {
        if bit_len > 56 {
            return None;
        }
        acc = (acc << 8) | byte as u64;
        bit_len = bit_len.saturating_add(8);

        loop {
            let mut matched = false;
            let mut l = 5u8;
            while l <= 30 {
                if bit_len < l {
                    l += 1;
                    continue;
                }

                let mask = (1u64 << l) - 1;
                let prefix = ((acc >> (bit_len - l)) & mask) as u32;
                if let Some(sym) = hpack_huffman_lookup_symbol(prefix, l) {
                    if sym == 256 {
                        return None;
                    }
                    out.push(sym as u8);
                    bit_len -= l;
                    if bit_len == 0 {
                        acc = 0;
                    } else {
                        let keep_mask = (1u64 << bit_len) - 1;
                        acc &= keep_mask;
                    }
                    matched = true;
                    break;
                }
                l += 1;
            }

            if !matched {
                break;
            }
        }
    }

    // RFC7541: remaining bits must be <= 7 and all ones (EOS padding).
    if bit_len > 7 {
        return None;
    }
    if bit_len > 0 {
        let pad_mask = (1u64 << bit_len) - 1;
        if (acc & pad_mask) != pad_mask {
            return None;
        }
    }

    Some(out)
}

fn hpack_decode_prefixed_integer(buf: &[u8], idx: &mut usize, prefix_bits: u8) -> Option<u32> {
    let first = *buf.get(*idx)?;
    let mask = (1u8 << prefix_bits) - 1;
    let mut value = (first & mask) as u32;
    *idx += 1;
    if value < mask as u32 {
        return Some(value);
    }

    let mut shift = 0u32;
    loop {
        let b = *buf.get(*idx)?;
        *idx += 1;
        value = value.checked_add(((b & 0x7F) as u32) << shift)?;
        if (b & 0x80) == 0 {
            break;
        }
        shift = shift.saturating_add(7);
        if shift > 28 {
            return None;
        }
    }
    Some(value)
}

fn hpack_decode_string(buf: &[u8], idx: &mut usize) -> Option<String> {
    let first = *buf.get(*idx)?;
    let huffman = (first & 0x80) != 0;
    let len = hpack_decode_prefixed_integer(buf, idx, 7)? as usize;
    let end = idx.checked_add(len)?;
    if end > buf.len() {
        return None;
    }
    let raw = &buf[*idx..end];
    *idx = end;

    if huffman {
        let decoded = hpack_decode_huffman(raw)?;
        return Some(String::from_utf8_lossy(decoded.as_slice()).into_owned());
    }

    Some(String::from_utf8_lossy(raw).into_owned())
}

fn hpack_static_table(index: u32) -> Option<(&'static str, &'static str)> {
    // RFC 7541 Appendix A (1..=61)
    match index {
        1 => Some((":authority", "")),
        2 => Some((":method", "GET")),
        3 => Some((":method", "POST")),
        4 => Some((":path", "/")),
        5 => Some((":path", "/index.html")),
        6 => Some((":scheme", "http")),
        7 => Some((":scheme", "https")),
        8 => Some((":status", "200")),
        9 => Some((":status", "204")),
        10 => Some((":status", "206")),
        11 => Some((":status", "304")),
        12 => Some((":status", "400")),
        13 => Some((":status", "404")),
        14 => Some((":status", "500")),
        15 => Some(("accept-charset", "")),
        16 => Some(("accept-encoding", "gzip, deflate")),
        17 => Some(("accept-language", "")),
        18 => Some(("accept-ranges", "")),
        19 => Some(("accept", "")),
        20 => Some(("access-control-allow-origin", "")),
        21 => Some(("age", "")),
        22 => Some(("allow", "")),
        23 => Some(("authorization", "")),
        24 => Some(("cache-control", "")),
        25 => Some(("content-disposition", "")),
        26 => Some(("content-encoding", "")),
        27 => Some(("content-language", "")),
        28 => Some(("content-length", "")),
        29 => Some(("content-location", "")),
        30 => Some(("content-range", "")),
        31 => Some(("content-type", "")),
        32 => Some(("cookie", "")),
        33 => Some(("date", "")),
        34 => Some(("etag", "")),
        35 => Some(("expect", "")),
        36 => Some(("expires", "")),
        37 => Some(("from", "")),
        38 => Some(("host", "")),
        39 => Some(("if-match", "")),
        40 => Some(("if-modified-since", "")),
        41 => Some(("if-none-match", "")),
        42 => Some(("if-range", "")),
        43 => Some(("if-unmodified-since", "")),
        44 => Some(("last-modified", "")),
        45 => Some(("link", "")),
        46 => Some(("location", "")),
        47 => Some(("max-forwards", "")),
        48 => Some(("proxy-authenticate", "")),
        49 => Some(("proxy-authorization", "")),
        50 => Some(("range", "")),
        51 => Some(("referer", "")),
        52 => Some(("refresh", "")),
        53 => Some(("retry-after", "")),
        54 => Some(("server", "")),
        55 => Some(("set-cookie", "")),
        56 => Some(("strict-transport-security", "")),
        57 => Some(("transfer-encoding", "")),
        58 => Some(("user-agent", "")),
        59 => Some(("vary", "")),
        60 => Some(("via", "")),
        61 => Some(("www-authenticate", "")),
        _ => None,
    }
}

fn hpack_lookup_header(index: u32, dynamic: &HpackDynamicTable) -> Option<(String, String)> {
    if index == 0 {
        return None;
    }
    if let Some((n, v)) = hpack_static_table(index) {
        return Some((String::from(n), String::from(v)));
    }
    dynamic
        .get(index)
        .map(|(n, v)| (String::from(n), String::from(v)))
}

fn hpack_lookup_name(index: u32, dynamic: &HpackDynamicTable) -> Option<String> {
    if index == 0 {
        return None;
    }
    if let Some((n, _)) = hpack_static_table(index) {
        return Some(String::from(n));
    }
    dynamic.get(index).map(|(n, _)| String::from(n))
}

fn hpack_decode_header_block(
    block: &[u8],
    dynamic: &mut HpackDynamicTable,
    out_headers: &mut Vec<(String, String)>,
    out_status: &mut Option<u16>,
) {
    let mut idx = 0usize;
    while idx < block.len() {
        let byte = block[idx];

        // Dynamic table size update: 001xxxxx
        if (byte & 0xE0) == 0x20 {
            let Some(new_size) = hpack_decode_prefixed_integer(block, &mut idx, 5) else {
                break;
            };
            dynamic.set_max_size(new_size as usize);
            continue;
        }

        // Indexed header field: 1xxxxxxx
        if (byte & 0x80) != 0 {
            let Some(index) = hpack_decode_prefixed_integer(block, &mut idx, 7) else {
                break;
            };
            if let Some((name, value)) = hpack_lookup_header(index, dynamic) {
                if name == ":status" {
                    if let Ok(code) = value.parse::<u16>() {
                        *out_status = Some(code);
                    }
                } else if !name.starts_with(':') {
                    out_headers.push((name, value));
                }
            }
            continue;
        }

        // Literal header field:
        // 01xxxxxx (incremental indexing) or 0000xxxx (without indexing) or 0001xxxx (never indexed)
        let (incremental, prefix) = if (byte & 0xC0) == 0x40 {
            (true, 6u8)
        } else {
            (false, 4u8)
        };
        let Some(name_index) = hpack_decode_prefixed_integer(block, &mut idx, prefix) else {
            break;
        };

        let name = if name_index == 0 {
            match hpack_decode_string(block, &mut idx) {
                Some(text) => text,
                None => continue,
            }
        } else {
            match hpack_lookup_name(name_index, dynamic) {
                Some(n) => n,
                None => continue,
            }
        };

        let value = match hpack_decode_string(block, &mut idx) {
            Some(text) => text,
            None => continue,
        };

        if name == ":status" {
            if let Ok(code) = value.parse::<u16>() {
                *out_status = Some(code);
            }
        } else {
            if !name.starts_with(':') {
                out_headers.push((name.clone(), value.clone()));
            }
        }

        if incremental {
            dynamic.insert(name, value);
        }
    }
}

fn http2_drain_frames(
    input: &mut Vec<u8>,
    response: &mut Http2ResponseCollector,
    pending_tx_frames: &mut Vec<Vec<u8>>,
) {
    loop {
        if input.len() < 9 {
            return;
        }

        let len = ((input[0] as usize) << 16) | ((input[1] as usize) << 8) | (input[2] as usize);
        if input.len() < 9 + len {
            return;
        }

        let frame_type = input[3];
        let flags = input[4];
        let stream_id = ((input[5] as u32 & 0x7F) << 24)
            | ((input[6] as u32) << 16)
            | ((input[7] as u32) << 8)
            | (input[8] as u32);
        let payload = &input[9..9 + len];

        match frame_type {
            HTTP2_FRAME_SETTINGS => {
                if stream_id != 0 {
                    response.connection_closed = true;
                } else if (flags & HTTP2_FLAG_ACK) == 0 {
                    http2_apply_peer_settings(response, payload);
                    let mut ack = Vec::new();
                    let _ = http2_build_frame(
                        HTTP2_FRAME_SETTINGS,
                        HTTP2_FLAG_ACK,
                        0,
                        &[],
                        &mut ack,
                    );
                    pending_tx_frames.push(ack);
                }
            }
            HTTP2_FRAME_PING => {
                if stream_id != 0 {
                    response.connection_closed = true;
                } else if (flags & HTTP2_FLAG_ACK) == 0 {
                    if payload.len() == 8 {
                        let mut ack = Vec::new();
                        let _ = http2_build_frame(
                            HTTP2_FRAME_PING,
                            HTTP2_FLAG_ACK,
                            0,
                            payload,
                            &mut ack,
                        );
                        pending_tx_frames.push(ack);
                    } else {
                        response.connection_closed = true;
                    }
                }
            }
            HTTP2_FRAME_HEADERS => {
                if stream_id == 0 {
                    response.connection_closed = true;
                } else {
                    if !response.streams.contains_key(&stream_id) {
                        if (stream_id & 1) == 0 {
                            http2_queue_rst_stream(
                                pending_tx_frames,
                                stream_id,
                                HTTP2_ERR_REFUSED_STREAM,
                            );
                            response.mark_stream_closed(stream_id);
                        } else {
                            response.connection_closed = true;
                        }
                        input.drain(..9 + len);
                        continue;
                    }
                    let mut start = 0usize;
                    let mut end = payload.len();
                    if (flags & HTTP2_FLAG_PADDED) != 0 {
                        if payload.is_empty() {
                            response.connection_closed = true;
                            input.drain(..9 + len);
                            continue;
                        }
                        let pad = payload[0] as usize;
                        start = 1;
                        if pad <= payload.len().saturating_sub(start) {
                            end = payload.len().saturating_sub(pad);
                        } else {
                            response.connection_closed = true;
                            input.drain(..9 + len);
                            continue;
                        }
                    }
                    if (flags & HTTP2_FLAG_PRIORITY) != 0 {
                        start = start.saturating_add(5);
                    }
                    if start > end || end > payload.len() {
                        response.connection_closed = true;
                        input.drain(..9 + len);
                        continue;
                    }

                    if response.partial_header_stream != 0
                        && response.partial_header_stream != stream_id
                    {
                        response.connection_closed = true;
                    } else {
                        let mut complete_header_block = None;
                        let continuing_same_stream = response.partial_header_stream == stream_id;
                        let mut stream_error = false;
                        {
                            let Some(stream) = response.stream_mut(stream_id) else {
                                response.connection_closed = true;
                                input.drain(..9 + len);
                                continue;
                            };
                            if !stream.can_receive_response_headers() {
                                stream.force_close();
                                stream_error = true;
                            } else {
                                if !continuing_same_stream {
                                    stream.partial_header_block.clear();
                                }
                                if start < end {
                                    stream
                                        .partial_header_block
                                        .extend_from_slice(&payload[start..end]);
                                }
                                if (flags & HTTP2_FLAG_END_HEADERS) != 0 {
                                    complete_header_block =
                                        Some(core::mem::take(&mut stream.partial_header_block));
                                }
                                if (flags & HTTP2_FLAG_END_STREAM) != 0 {
                                    stream.mark_remote_end_stream();
                                }
                            }
                        }
                        if stream_error {
                            http2_queue_rst_stream(
                                pending_tx_frames,
                                stream_id,
                                HTTP2_ERR_STREAM_CLOSED,
                            );
                            response.partial_header_stream = 0;
                            input.drain(..9 + len);
                            continue;
                        }

                        if let Some(header_block) = complete_header_block {
                            let mut parsed_headers = Vec::new();
                            let mut parsed_status = None;
                            hpack_decode_header_block(
                                header_block.as_slice(),
                                &mut response.dynamic_table,
                                &mut parsed_headers,
                                &mut parsed_status,
                            );
                            if let Some(stream) = response.stream_mut(stream_id) {
                                if let Some(code) = parsed_status {
                                    stream.status = Some(code);
                                }
                                stream.headers.extend(parsed_headers);
                            }
                            response.partial_header_stream = 0;
                        } else {
                            response.partial_header_stream = stream_id;
                        }
                    }
                }
            }
            HTTP2_FRAME_CONTINUATION => {
                if stream_id == 0 || stream_id != response.partial_header_stream {
                    response.connection_closed = true;
                } else {
                    let mut complete_header_block = None;
                    {
                        let Some(stream) = response.stream_mut(stream_id) else {
                            response.connection_closed = true;
                            input.drain(..9 + len);
                            continue;
                        };
                        stream.partial_header_block.extend_from_slice(payload);
                        if (flags & HTTP2_FLAG_END_HEADERS) != 0 {
                            complete_header_block =
                                Some(core::mem::take(&mut stream.partial_header_block));
                        }
                    }
                    if let Some(header_block) = complete_header_block {
                        let mut parsed_headers = Vec::new();
                        let mut parsed_status = None;
                        hpack_decode_header_block(
                            header_block.as_slice(),
                            &mut response.dynamic_table,
                            &mut parsed_headers,
                            &mut parsed_status,
                        );
                        if let Some(stream) = response.stream_mut(stream_id) {
                            if let Some(code) = parsed_status {
                                stream.status = Some(code);
                            }
                            stream.headers.extend(parsed_headers);
                        }
                        response.partial_header_stream = 0;
                    }
                }
            }
            HTTP2_FRAME_DATA => {
                if stream_id == 0 {
                    response.connection_closed = true;
                } else {
                    if !response.streams.contains_key(&stream_id) {
                        if (stream_id & 1) == 0 {
                            http2_queue_rst_stream(
                                pending_tx_frames,
                                stream_id,
                                HTTP2_ERR_REFUSED_STREAM,
                            );
                            response.mark_stream_closed(stream_id);
                        } else {
                            response.connection_closed = true;
                        }
                        input.drain(..9 + len);
                        continue;
                    }
                    let mut start = 0usize;
                    let mut end = payload.len();
                    let mut flow_controlled_len = payload.len() as u32;

                    if (flags & HTTP2_FLAG_PADDED) != 0 {
                        if payload.is_empty() {
                            response.connection_closed = true;
                            input.drain(..9 + len);
                            continue;
                        }
                        let pad = payload[0] as usize;
                        start = 1;
                        if pad <= payload.len().saturating_sub(start) {
                            end = payload.len().saturating_sub(pad);
                        } else {
                            response.connection_closed = true;
                            input.drain(..9 + len);
                            continue;
                        }
                        flow_controlled_len = payload.len().saturating_sub(1) as u32;
                    }

                    if flow_controlled_len > 0 {
                        response.conn_recv_window =
                            response.conn_recv_window.saturating_sub(flow_controlled_len as i64);
                        response.conn_consumed_since_update = response
                            .conn_consumed_since_update
                            .saturating_add(flow_controlled_len);
                    }

                    let mut force_window_update = false;
                    let mut stream_flow_error = false;
                    {
                        let Some(stream) = response.stream_mut(stream_id) else {
                            response.connection_closed = true;
                            input.drain(..9 + len);
                            continue;
                        };
                        if !stream.can_receive_response_data() {
                            stream.force_close();
                            http2_queue_rst_stream(
                                pending_tx_frames,
                                stream_id,
                                HTTP2_ERR_STREAM_CLOSED,
                            );
                            stream_flow_error = true;
                        }
                        if flow_controlled_len > 0 {
                            stream.recv_window =
                                stream.recv_window.saturating_sub(flow_controlled_len as i64);
                            stream.consumed_since_update = stream
                                .consumed_since_update
                                .saturating_add(flow_controlled_len);
                        }
                        if start < end && end <= payload.len() && !stream_flow_error {
                            stream.body.extend_from_slice(&payload[start..end]);
                        }
                        if (flags & HTTP2_FLAG_END_STREAM) != 0 {
                            stream.mark_remote_end_stream();
                            force_window_update = true;
                        }
                        if stream.recv_window < 0 {
                            stream.force_close();
                            http2_queue_rst_stream(
                                pending_tx_frames,
                                stream_id,
                                HTTP2_ERR_FLOW_CONTROL,
                            );
                        }
                        http2_replenish_stream_window_if_needed(
                            stream,
                            stream_id,
                            force_window_update,
                            pending_tx_frames,
                        );
                    }

                    if response.conn_recv_window < 0 {
                        response.connection_closed = true;
                    }
                    http2_replenish_connection_window_if_needed(
                        response,
                        force_window_update,
                        pending_tx_frames,
                    );
                }
            }
            HTTP2_FRAME_RST_STREAM => {
                if stream_id == 0 {
                    response.connection_closed = true;
                } else {
                    if !response.streams.contains_key(&stream_id) {
                        response.connection_closed = true;
                        input.drain(..9 + len);
                        continue;
                    }
                    let mut err_code = None;
                    if payload.len() >= 4 {
                        err_code = Some(
                            ((payload[0] as u32) << 24)
                                | ((payload[1] as u32) << 16)
                                | ((payload[2] as u32) << 8)
                                | payload[3] as u32,
                        );
                    }
                    if let Some(stream) = response.stream_mut(stream_id) {
                        stream.force_close();
                        stream.reset_error_code = err_code;
                    }
                }
            }
            HTTP2_FRAME_GOAWAY => {
                response.connection_closed = true;
            }
            HTTP2_FRAME_WINDOW_UPDATE => {
                if payload.len() != 4 {
                    response.connection_closed = true;
                } else {
                    let increment = (((payload[0] as u32) << 24)
                        | ((payload[1] as u32) << 16)
                        | ((payload[2] as u32) << 8)
                        | payload[3] as u32)
                        & 0x7FFF_FFFF;
                    if increment == 0 {
                        if stream_id == 0 {
                            response.connection_closed = true;
                        } else if let Some(stream) = response.stream_mut(stream_id) {
                            stream.force_close();
                        } else {
                            response.connection_closed = true;
                        }
                    } else {
                        if stream_id != 0 && !response.streams.contains_key(&stream_id) {
                            response.connection_closed = true;
                            input.drain(..9 + len);
                            continue;
                        }
                        http2_handle_peer_window_update(response, stream_id, increment);
                    }
                }
            }
            _ => {}
        }

        input.drain(..9 + len);
    }
}

fn http2_build_synthesized_http_response_bytes(response: &Http2ResponseCollector) -> Vec<u8> {
    let status = response.target_status().unwrap_or(200);
    let reason = http2_reason_phrase(status);
    let mut out = alloc::format!("HTTP/2 {} {}\r\n", status, reason).into_bytes();
    if let Some(headers) = response.target_headers() {
        for (name, value) in headers.iter() {
            if name.is_empty() {
                continue;
            }
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(b": ");
            out.extend_from_slice(value.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
    }
    out.extend_from_slice(b"\r\n");
    if let Some(body) = response.target_body() {
        out.extend_from_slice(body);
    }
    out
}

fn http2_send_get_requests(
    tls: &mut crate::net::tls::TlsConnection,
    socket: &mut tcp::Socket,
    response: &mut Http2ResponseCollector,
    host: &str,
    paths: &[&str],
    is_https: bool,
    request_hints: &HttpRequestHints,
) -> bool {
    let mut request = Vec::new();
    request.extend_from_slice(HTTP2_CLIENT_PREFACE);

    // SETTINGS: disable server push (id=0x2, value=0)
    let settings_payload = [0x00u8, 0x02, 0x00, 0x00, 0x00, 0x00];
    if !http2_build_frame(HTTP2_FRAME_SETTINGS, 0, 0, &settings_payload, &mut request) {
        return false;
    }

    let mut opened = 0usize;
    for path in paths.iter().copied() {
        if !response.can_open_local_stream() {
            break;
        }
        let Some(stream_id) = response.open_local_get_stream(true) else {
            break;
        };
        let use_huffman = response.use_hpack_huffman;
        let header_block = hpack_encode_request_headers(
            &mut response.outbound_dynamic_table,
            host,
            path,
            is_https,
            use_huffman,
            request_hints,
        );
        if !http2_build_frame(
            HTTP2_FRAME_HEADERS,
            HTTP2_FLAG_END_HEADERS | HTTP2_FLAG_END_STREAM,
            stream_id,
            header_block.as_slice(),
            &mut request,
        ) {
            response.mark_stream_closed(stream_id);
            break;
        }
        opened += 1;
    }

    if opened == 0 {
        return false;
    }
    tls.write(socket, request.as_slice()) == request.len()
}

fn http_get_request_bytes_with_timeout_once(
    url: &str,
    pump_ui: &mut impl FnMut(),
    timeout_ticks: u64,
) -> Option<Vec<u8>> {
    // Very simple HTTP 1.0 Client (Blocking)
    // URL ignored for now, always connects to 1.1.1.1 (Cloudflare) or similar
    // to prove connectivity.
    
    unsafe {
        if IFACE.is_none() || SOCKETS.is_none() {
            println("Net: Stack not initialized.");
            return None;
        }
        
        let iface = IFACE.as_mut().unwrap();
        let sockets = SOCKETS.as_mut().unwrap();
        
        let is_https = starts_with_ignore_ascii_case(url, "https://");
        let use_https_proxy = is_https && is_https_proxy_enabled() && !is_https_proxy_url(url);
        let effective_url_storage = if use_https_proxy {
            build_https_proxy_url(url)
        } else {
            String::from(url)
        };
        let effective_url = effective_url_storage.as_str();

        let Some((host, port, path)) = parse_url(effective_url) else {
            println("Net: Invalid URL format. Use http://domain.com/ or http://1.2.3.4/");
            return None;
        };
        
        // If using native HTTPS, default port 443 if not specified.
        let port = if port == 80 && is_https && !use_https_proxy { 443 } else { port };
        let request_hints = http_cache_request_hints(
            effective_url,
            host.as_str(),
            path,
            is_https && !use_https_proxy,
            crate::timer::ticks(),
        );

        let mut reused_pooled_socket = false;
        let handle = if let Some(existing) = http_pool_take_reusable_socket(
            sockets,
            host.as_str(),
            port,
            is_https,
            use_https_proxy,
            crate::timer::ticks(),
        ) {
            reused_pooled_socket = true;
            println("Net: HTTP keep-alive socket reused.");
            existing
        } else {
            // Resolve Host
            let remote_addr = if let Ok(ip) = host.parse::<Ipv4Address>() {
                ip
            } else {
                // Use DNS
                let dns_handle = DNS_HANDLE.expect("DNS not initialized");
                println(&alloc::format!("Net: Resolving {}...", host));
                
                let query_handle = {
                    let dns_socket = sockets.get_mut::<dns::Socket>(dns_handle);
                    dns_socket.start_query(iface.context(), &host, smoltcp::wire::DnsQueryType::A).ok()?
                };

                let start_dns = crate::timer::ticks();
                let mut resolved_ip = None;
                while crate::timer::ticks() - start_dns < timeout_ticks {
                    pump_ui();
                    crate::timer::on_tick();
                    let timestamp = Instant::from_millis(crate::timer::ticks() as i64 * 10);
                    let mut phy = if crate::intel_net::GLOBAL_INTEL_NET.is_some() {
                        ReduxPhy::Intel(crate::intel_net::IntelPhy)
                    } else {
                        ReduxPhy::Virtio(VirtioPhy)
                    };
                    iface.poll(timestamp, &mut phy, sockets);

                    let dns_socket = sockets.get_mut::<dns::Socket>(dns_handle);
                    match dns_socket.get_query_result(query_handle) {
                        Ok(addrs) => {
                            for addr in addrs {
                                if let smoltcp::wire::IpAddress::Ipv4(ip) = addr {
                                    resolved_ip = Some(ip);
                                    break;
                                }
                            }
                            if resolved_ip.is_some() { break; }
                        },
                        Err(dns::GetQueryResultError::Pending) => {
                            pump_ui();
                            uefi::boot::stall(NET_BLOCKING_LOOP_STALL_US);
                            continue;
                        }
                        Err(_) => {
                            println("Net: DNS Resolution Failed");
                            return None;
                        }
                    }

                    pump_ui();
                    uefi::boot::stall(NET_BLOCKING_LOOP_STALL_US);
                }
                
                resolved_ip?
            };

            let rx_buffer = alloc::vec![0u8; 4096];
            let tx_buffer = alloc::vec![0u8; 4096];
            // Must leak to get 'static lifetime for now
            let rx_static = alloc::boxed::Box::leak(rx_buffer.into_boxed_slice());
            let tx_static = alloc::boxed::Box::leak(tx_buffer.into_boxed_slice());

            let socket = tcp::Socket::new(
                tcp::SocketBuffer::new(&mut rx_static[..]),
                tcp::SocketBuffer::new(&mut tx_static[..]),
            );
            let handle = sockets.add(socket);
            let socket = sockets.get_mut::<tcp::Socket>(handle);
            
            crate::println(&alloc::format!("Net: Connecting to {}:{}...", remote_addr, port));
            
            if let Err(_e) = socket.connect(iface.context(), (remote_addr, port), 49152 + (crate::timer::ticks() % 10000) as u16) {
                println("Net: Connect failed");
                sockets.remove(handle);
                return None;
            }
            
            // Blocking loop to connect
            let start = crate::timer::ticks();
            loop {
                pump_ui();
                crate::timer::on_tick();
                let timestamp = Instant::from_millis(crate::timer::ticks() as i64 * 10);
                let mut phy = if crate::intel_net::GLOBAL_INTEL_NET.is_some() {
                    ReduxPhy::Intel(crate::intel_net::IntelPhy)
                } else {
                    ReduxPhy::Virtio(VirtioPhy)
                };
                iface.poll(timestamp, &mut phy, sockets);
                
                let (may_send, is_active) = {
                    let socket = sockets.get_mut::<tcp::Socket>(handle);
                    (socket.may_send(), socket.is_active())
                };
                if may_send {
                    break;
                }
                if !is_active {
                    println("Net: Connect failed");
                    sockets.remove(handle);
                    return None;
                }
                
                if crate::timer::ticks() - start > timeout_ticks {
                    println("Net: Connect Timeout");
                    sockets.remove(handle);
                    return None;
                }
    
                pump_ui();
                uefi::boot::stall(NET_BLOCKING_LOOP_STALL_US);
            }
            handle
        };
    
            let mut response: Vec<u8> = Vec::new();
            let mut keepalive_reusable = false;
            let host_header = if (is_https && port != 443) || (!is_https && port != 80) {
                alloc::format!("{}:{}", host, port)
            } else {
                host.clone()
            };
            let connection_header = if is_https && !use_https_proxy {
                "close"
            } else {
                "keep-alive"
            };
            // Browser-like request headers improve compatibility with modern sites/CDN/WAFs.
            let mut req = alloc::format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36 ReduxOS/0.2\r\nAccept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8\r\nAccept-Language: en-US,en;q=0.9,es;q=0.8\r\nAccept-Encoding: {}\r\nCache-Control: no-cache\r\nPragma: no-cache\r\nConnection: {}\r\n",
                path,
                host_header,
                HTTP_ACCEPT_ENCODING_VALUE,
                connection_header,
            );
            if connection_header == "keep-alive" {
                req.push_str("Keep-Alive: timeout=20, max=8\r\n");
            }
            if let Some(cookie) = request_hints.cookie_header.as_ref() {
                req.push_str("Cookie: ");
                req.push_str(cookie.as_str());
                req.push_str("\r\n");
            }
            if let Some(etag) = request_hints.if_none_match.as_ref() {
                req.push_str("If-None-Match: ");
                req.push_str(etag.as_str());
                req.push_str("\r\n");
            }
            if let Some(modified) = request_hints.if_modified_since.as_ref() {
                req.push_str("If-Modified-Since: ");
                req.push_str(modified.as_str());
                req.push_str("\r\n");
            }
            req.push_str("\r\n");
    
             if is_https && !use_https_proxy {
                 crate::println("Net: Initializing TLS...");
                 crate::println(
                     alloc::format!("Net: TLS root CA store -> {}", webpki_roots::TLS_SERVER_ROOTS.len()).as_str()
                 );
                 let mut tls = match crate::net::tls::TlsConnection::new(&host) {
                     Some(t) => t,
                     None => {
                         sockets.remove(handle);
                         return None;
                     }
                 };
    
                 // Handshake Loop
                 let start_handshake = crate::timer::ticks();
                 loop {
                     pump_ui();
                     crate::timer::on_tick();
                     let timestamp = Instant::from_millis(crate::timer::ticks() as i64 * 10);
                     let mut phy = if crate::intel_net::GLOBAL_INTEL_NET.is_some() {
                         ReduxPhy::Intel(crate::intel_net::IntelPhy)
                     } else {
                         ReduxPhy::Virtio(VirtioPhy)
                     };
                     iface.poll(timestamp, &mut phy, sockets);
                     
                     let socket = sockets.get_mut::<tcp::Socket>(handle);
                     if !socket.is_active() { break; }
                     
                     match tls.process_handshake(socket) {
                         crate::net::tls::HandshakeStatus::Done => break,
                         crate::net::tls::HandshakeStatus::Error => {
                             println("Net: TLS Handshake Failed.");
                             sockets.remove(handle);
                             return None;
                         }
                         crate::net::tls::HandshakeStatus::InProgress => {}
                     }
                     
                     if crate::timer::ticks() - start_handshake > timeout_ticks {
                         println("Net: TLS Handshake Timeout");
                         sockets.remove(handle);
                         return None;
                     }
                     pump_ui();
                     uefi::boot::stall(NET_BLOCKING_LOOP_STALL_US);
                 }
                 
                 crate::println("Net: TLS Handshake Success!");
                 crate::println(
                     alloc::format!("Net: TLS ALPN -> {}", tls.selected_alpn_label()).as_str()
                 );

                 let alpn_is_h2 = tls
                     .selected_alpn()
                     .map(|p| p == b"h2")
                     .unwrap_or(false);

                 if alpn_is_h2 {
                     crate::println("Net: HTTP/2 enabled (ALPN h2).");
                     let mut stream_response = Http2ResponseCollector::new();

                     {
                         let socket = sockets.get_mut::<tcp::Socket>(handle);
                         if !http2_send_get_requests(
                             &mut tls,
                             socket,
                             &mut stream_response,
                             &host,
                            &[path],
                            is_https,
                             &request_hints,
                         ) {
                             println("Net: HTTP/2 request send failed.");
                             sockets.remove(handle);
                             return None;
                         }
                     }

                     let start_read = crate::timer::ticks();
                     let mut tls_read_buf = [0u8; 2048];
                     let mut frame_input = Vec::new();
                     loop {
                         pump_ui();
                         crate::timer::on_tick();
                         let timestamp = Instant::from_millis(crate::timer::ticks() as i64 * 10);
                         let mut phy = if crate::intel_net::GLOBAL_INTEL_NET.is_some() {
                             ReduxPhy::Intel(crate::intel_net::IntelPhy)
                         } else {
                             ReduxPhy::Virtio(VirtioPhy)
                         };
                         iface.poll(timestamp, &mut phy, sockets);

                         let mut pending_tx = Vec::new();
                         {
                             let socket = sockets.get_mut::<tcp::Socket>(handle);
                             if !socket.is_active() {
                                 break;
                             }
                             let read_len = tls.read(socket, &mut tls_read_buf);
                             if read_len > 0 {
                                 frame_input.extend_from_slice(&tls_read_buf[..read_len]);
                             }
                         }

                         if !frame_input.is_empty() {
                             http2_drain_frames(
                                 &mut frame_input,
                                 &mut stream_response,
                                 &mut pending_tx,
                             );
                         }

                         if !pending_tx.is_empty() {
                             let socket = sockets.get_mut::<tcp::Socket>(handle);
                             for frame in pending_tx.iter() {
                                 let _ = tls.write(socket, frame.as_slice());
                             }
                         }

                         if stream_response.target_stream_closed() {
                             break;
                         }

                         if crate::timer::ticks() - start_read > timeout_ticks {
                             println("Net: HTTP/2 read timeout.");
                             break;
                         }
                         pump_ui();
                         uefi::boot::stall(NET_BLOCKING_LOOP_STALL_US);
                     }

                     if stream_response.target_body_is_empty() {
                         println("Net: HTTP/2 returned empty body; fallback to HTTP/1.1 parser path.");
                     }
                     response = http2_build_synthesized_http_response_bytes(&stream_response);
                 } else {
                     let socket = sockets.get_mut::<tcp::Socket>(handle);
                     tls.write(socket, req.as_bytes());

                     // TLS Read Loop (HTTP/1.1 over TLS)
                     let start_read = crate::timer::ticks();
                     let mut read_buf = [0u8; 1024];
                     loop {
                         pump_ui();
                         crate::timer::on_tick();
                         let timestamp = Instant::from_millis(crate::timer::ticks() as i64 * 10);
                         let mut phy = if crate::intel_net::GLOBAL_INTEL_NET.is_some() {
                             ReduxPhy::Intel(crate::intel_net::IntelPhy)
                         } else {
                             ReduxPhy::Virtio(VirtioPhy)
                         };
                         iface.poll(timestamp, &mut phy, sockets);

                         let socket = sockets.get_mut::<tcp::Socket>(handle);
                         if !socket.is_active() { break; }

                         let len = tls.read(socket, &mut read_buf);
                         if len > 0 {
                             response.extend_from_slice(&read_buf[..len]);
                         }

                         if crate::timer::ticks() - start_read > timeout_ticks {
                             break;
                         }
                         pump_ui();
                         uefi::boot::stall(NET_BLOCKING_LOOP_STALL_US);
                     }
                 }
            } else {
                 if use_https_proxy {
                     crate::println("Net: HTTPS compatibility proxy enabled.");
                 }
                 if reused_pooled_socket {
                     println("Net: HTTP request using pooled keep-alive socket.");
                 }
                 crate::println(&alloc::format!("Net: Connected! Sending GET {}...", path));
                 let send_ok = {
                     let socket = sockets.get_mut::<tcp::Socket>(handle);
                     socket.can_send() && socket.send_slice(req.as_bytes()).is_ok()
                 };
                 if !send_ok {
                     println("Net: HTTP send failed.");
                     sockets.remove(handle);
                     return None;
                 }

                 if let Some((plain_response, can_reuse_socket)) =
                     http_read_http1_response(iface, sockets, handle, pump_ui, timeout_ticks)
                 {
                     response = plain_response;
                     keepalive_reusable = can_reuse_socket;
                 }
            }
        
        println("Net: Request complete.");

        if keepalive_reusable {
            http_pool_store_socket(
                sockets,
                handle,
                host.as_str(),
                port,
                is_https,
                use_https_proxy,
                crate::timer::ticks(),
            );
            println("Net: HTTP keep-alive socket returned to pool.");
        } else {
            sockets.remove(handle);
        }
        
        if response.is_empty() {
            None
        } else {
            Some(http_postprocess_response(
                effective_url,
                host.as_str(),
                path,
                is_https && !use_https_proxy,
                response,
                crate::timer::ticks(),
            ))
        }
    }
}

pub fn http_get_request_bytes_with_timeout(
    url: &str,
    pump_ui: &mut impl FnMut(),
    timeout_ticks: u64,
) -> Option<Vec<u8>> {
    let mut attempt = 0usize;
    while attempt < HTTP_RETRY_MAX_ATTEMPTS {
        let response = http_get_request_bytes_with_timeout_once(url, pump_ui, timeout_ticks);
        match response {
            Some(bytes) => {
                let parsed = parse_http_headers(bytes.as_slice());
                let retryable = parsed
                    .status_code
                    .map(http_should_retry_status)
                    .unwrap_or(false);
                if retryable && attempt + 1 < HTTP_RETRY_MAX_ATTEMPTS {
                    let backoff = http_retry_backoff_ticks(attempt);
                    let status = parsed.status_code.unwrap_or(0);
                    println(
                        format!(
                            "Net: HTTP retry {}/{} after status {} (backoff={} ticks).",
                            attempt + 1,
                            HTTP_RETRY_MAX_ATTEMPTS - 1,
                            status,
                            backoff
                        )
                        .as_str(),
                    );
                    http_wait_ticks_with_ui(pump_ui, backoff);
                    attempt += 1;
                    continue;
                }
                return Some(bytes);
            }
            None => {
                if attempt + 1 < HTTP_RETRY_MAX_ATTEMPTS {
                    let backoff = http_retry_backoff_ticks(attempt);
                    println(
                        format!(
                            "Net: HTTP retry {}/{} after network failure (backoff={} ticks).",
                            attempt + 1,
                            HTTP_RETRY_MAX_ATTEMPTS - 1,
                            backoff
                        )
                        .as_str(),
                    );
                    http_wait_ticks_with_ui(pump_ui, backoff);
                    attempt += 1;
                    continue;
                }
                return None;
            }
        }
    }
    None
}

pub fn http_get_request_bytes(url: &str, pump_ui: &mut impl FnMut()) -> Option<Vec<u8>> {
    http_get_request_bytes_with_timeout(url, pump_ui, NET_BLOCKING_TIMEOUT_TICKS)
}

pub fn http_get_request_with_timeout(
    url: &str,
    pump_ui: &mut impl FnMut(),
    timeout_ticks: u64,
) -> Option<String> {
    let bytes = http_get_request_bytes_with_timeout(url, pump_ui, timeout_ticks)?;
    Some(String::from_utf8_lossy(bytes.as_slice()).into_owned())
}

pub fn http_get_request(url: &str, pump_ui: &mut impl FnMut()) -> Option<String> {
    let bytes = http_get_request_bytes(url, pump_ui)?;
    Some(String::from_utf8_lossy(bytes.as_slice()).into_owned())
}

pub fn get_ip_address() -> Option<IpAddress> {
    unsafe {
        IFACE.as_ref().and_then(|iface| {
            iface
                .ip_addrs()
                .iter()
                .map(|cidr| cidr.address())
                .find(|addr| matches!(addr, IpAddress::Ipv4(ipv4) if !ipv4.is_unspecified()))
        })
    }
}

pub fn get_gateway() -> Option<IpAddress> {
    unsafe { IPV4_GATEWAY }
}

pub fn get_active_transport() -> &'static str {
    unsafe { ACTIVE_TRANSPORT }
}

pub fn get_failover_policy() -> &'static str {
    unsafe { FAILOVER_POLICY }
}

pub fn get_network_mode() -> &'static str {
    unsafe {
        if USE_STATIC_IPV4_RUNTIME {
            NET_MODE_STATIC
        } else {
            NET_MODE_DHCP
        }
    }
}

pub fn get_https_mode() -> &'static str {
    if is_https_proxy_enabled() {
        HTTPS_MODE_PROXY
    } else {
        HTTPS_MODE_DISABLED
    }
}

pub fn is_https_proxy_enabled() -> bool {
    unsafe { HTTPS_PROXY_ENABLED }
}

pub fn get_static_ipv4_config() -> ([u8; 4], u8, [u8; 4]) {
    unsafe {
        (
            STATIC_IPV4_ADDR_RUNTIME,
            STATIC_IPV4_PREFIX_RUNTIME,
            STATIC_IPV4_GATEWAY_RUNTIME,
        )
    }
}

pub fn set_failover_policy_ethernet_first() {
    unsafe {
        FAILOVER_POLICY = FAILOVER_ETHERNET_FIRST;
    }
    refresh_active_transport();
}

pub fn set_failover_policy_wifi_first() {
    unsafe {
        FAILOVER_POLICY = FAILOVER_WIFI_FIRST;
    }
    refresh_active_transport();
}

pub fn set_https_mode_proxy() -> &'static str {
    unsafe {
        HTTPS_PROXY_ENABLED = true;
    }
    "Compatibilidad HTTPS por proxy activada."
}

pub fn set_https_mode_disabled() -> &'static str {
    unsafe {
        HTTPS_PROXY_ENABLED = false;
    }
    "Compatibilidad HTTPS por proxy desactivada."
}

pub fn set_dhcp_mode() -> &'static str {
    unsafe {
        USE_STATIC_IPV4_RUNTIME = false;

        if let (Some(iface), Some(sockets), Some(dhcp_handle), Some(dns_handle)) =
            (&mut IFACE, &mut SOCKETS, DHCP_HANDLE, DNS_HANDLE)
        {
            reset_ipv4_runtime(iface);

            let dns_servers = default_dns_servers();
            update_dns_servers(sockets, dns_handle, &dns_servers);

            sockets.get_mut::<dhcpv4::Socket>(dhcp_handle).reset();
            DHCP_LAST_RESET_TICK = crate::timer::ticks();
            DHCP_STATUS = if ACTIVE_TRANSPORT == NET_TRANSPORT_NONE {
                DHCP_STATUS_NO_LINK
            } else {
                DHCP_STATUS_SEARCHING
            };
            return "DHCP habilitado. Buscando IP...";
        }
    }
    "DHCP habilitado (se aplicara cuando la red se inicialice)."
}

pub fn set_static_ipv4(ip: [u8; 4], prefix: u8, gateway: [u8; 4]) -> Result<&'static str, &'static str> {
    if prefix == 0 || prefix > 32 {
        return Err("Prefijo invalido. Usa un valor entre 1 y 32.");
    }

    unsafe {
        USE_STATIC_IPV4_RUNTIME = true;
        STATIC_IPV4_ADDR_RUNTIME = ip;
        STATIC_IPV4_PREFIX_RUNTIME = prefix;
        STATIC_IPV4_GATEWAY_RUNTIME = gateway;
        STATIC_DNS_SERVERS_RUNTIME = STATIC_DNS_SERVERS;

        if let (Some(iface), Some(sockets), Some(dns_handle)) = (&mut IFACE, &mut SOCKETS, DNS_HANDLE) {
            apply_static_ipv4_runtime(iface);

            let dns_servers = static_dns_servers_runtime();
            update_dns_servers(sockets, dns_handle, &dns_servers);
            DHCP_STATUS = DHCP_STATUS_STATIC;
        }

        if let (Some(sockets), Some(dhcp_handle)) = (&mut SOCKETS, DHCP_HANDLE) {
            sockets.get_mut::<dhcpv4::Socket>(dhcp_handle).reset();
            DHCP_LAST_RESET_TICK = crate::timer::ticks();
        }
    }

    Ok("IP fija aplicada.")
}

pub fn use_default_static_ipv4() -> &'static str {
    if set_static_ipv4(STATIC_IPV4_ADDR, STATIC_IPV4_PREFIX_LEN, STATIC_IPV4_GATEWAY).is_ok() {
        "IP fija por defecto aplicada."
    } else {
        "No se pudo aplicar IP fija por defecto."
    }
}

pub fn set_static_ipv4_from_text(
    ip_text: &str,
    prefix_text: &str,
    gateway_text: &str,
) -> Result<&'static str, &'static str> {
    let ip = parse_ipv4_octets(ip_text).ok_or("IP invalida. Usa formato: a.b.c.d")?;
    let prefix = prefix_text
        .parse::<u8>()
        .ok()
        .filter(|v| *v > 0 && *v <= 32)
        .ok_or("Prefijo invalido. Usa un valor entre 1 y 32.")?;
    let gateway = parse_ipv4_octets(gateway_text).ok_or("Gateway invalido. Usa formato: a.b.c.d")?;
    set_static_ipv4(ip, prefix, gateway)
}

pub fn get_packet_stats() -> (u64, u64) {
    unsafe {
        (crate::intel_net::RX_COUNT, crate::intel_net::TX_COUNT)
    }
}
