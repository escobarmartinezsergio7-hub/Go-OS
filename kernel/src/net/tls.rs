use alloc::sync::Arc;
use core::convert::TryFrom;
use core::time::Duration;
use rustls::{ClientConfig, RootCertStore};
use rustls::pki_types::{ServerName, UnixTime};

use crate::println;

use rustls::client::UnbufferedClientConnection;
use rustls::time_provider::TimeProvider;
use rustls::unbuffered::ConnectionState;

const TLS_ALPN_H2: &[u8] = b"h2";
const TLS_ALPN_HTTP11: &[u8] = b"http/1.1";

#[derive(Debug)]
pub struct KernelTimeProvider;

const TLS_FALLBACK_UNIX_SECS: u64 = 1_767_225_600; // 2026-01-01 00:00:00 UTC

fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = month as i32;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era as i64) * 146_097 + (doe as i64) - 719_468
}

fn unix_seconds_from_uefi_time(time: &uefi::runtime::Time) -> Option<u64> {
    let year = time.year();
    if year < 1970 {
        return None;
    }

    let mut secs = days_from_civil(year as i32, time.month(), time.day())
        .checked_mul(86_400)?
        .checked_add((time.hour() as i64) * 3_600)?
        .checked_add((time.minute() as i64) * 60)?
        .checked_add(time.second() as i64)?;

    // UEFI reports local time with an optional UTC offset in minutes.
    if let Some(offset_minutes) = time.time_zone() {
        secs = secs.checked_sub((offset_minutes as i64) * 60)?;
    }

    if secs < 0 {
        None
    } else {
        Some(secs as u64)
    }
}

fn fallback_unix_seconds() -> u64 {
    TLS_FALLBACK_UNIX_SECS.saturating_add(crate::timer::ticks() / 1000)
}

impl TimeProvider for KernelTimeProvider {
    fn current_time(&self) -> Option<UnixTime> {
        let seconds = uefi::runtime::get_time()
            .ok()
            .and_then(|t| unix_seconds_from_uefi_time(&t))
            .unwrap_or_else(fallback_unix_seconds);
        Some(UnixTime::since_unix_epoch(Duration::from_secs(seconds)))
    }
}

pub enum HandshakeStatus {
    Done,
    InProgress,
    Error,
}

pub struct TlsConnection {
    pub conn: UnbufferedClientConnection,
    incoming_tls: [u8; 8192],
    incoming_len: usize,
}

impl TlsConnection {
    pub fn new(hostname: &str) -> Option<Self> {
        let mut root_store = RootCertStore::empty();
        root_store.extend(
            webpki_roots::TLS_SERVER_ROOTS
                .iter()
                .cloned()
        );

        let provider = rustls_rustcrypto::provider();
        
        let mut config = ClientConfig::builder_with_details(
            Arc::new(provider),
            Arc::new(KernelTimeProvider)
        )
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(root_store)
        .with_no_client_auth();
        // Offer HTTP/2 first, then HTTP/1.1 fallback via ALPN.
        config.alpn_protocols = alloc::vec![
            TLS_ALPN_H2.to_vec(),
            TLS_ALPN_HTTP11.to_vec(),
        ];
            
        let server_name = match ServerName::try_from(hostname) {
            Ok(n) => n.to_owned(),
            Err(_) => {
                println("TLS: Invalid hostname");
                return None;
            }
        };

        match UnbufferedClientConnection::new(Arc::new(config), server_name) {
            Ok(conn) => Some(TlsConnection {
                conn,
                incoming_tls: [0u8; 8192],
                incoming_len: 0,
            }),
            Err(e) => {
                println(alloc::format!("TLS: Creation failed -> {:?}", e).as_str());
                None
            }
        }
    }

    pub fn selected_alpn(&self) -> Option<&[u8]> {
        self.conn.alpn_protocol()
    }

    pub fn selected_alpn_label(&self) -> &'static str {
        match self.selected_alpn() {
            Some(p) if p == TLS_ALPN_H2 => "h2",
            Some(p) if p == TLS_ALPN_HTTP11 => "http/1.1",
            Some(_) => "other",
            None => "none",
        }
    }

    fn pull_tls_from_socket(&mut self, socket: &mut smoltcp::socket::tcp::Socket) -> usize {
        if !socket.can_recv() || self.incoming_len >= self.incoming_tls.len() {
            return 0;
        }

        match socket.recv_slice(&mut self.incoming_tls[self.incoming_len..]) {
            Ok(len) if len > 0 => {
                self.incoming_len += len;
                len
            }
            _ => 0,
        }
    }

    fn discard_tls_prefix(&mut self, discard: usize) {
        if discard == 0 {
            return;
        }
        if discard >= self.incoming_len {
            self.incoming_len = 0;
            return;
        }
        let end = self.incoming_len;
        self.incoming_tls.copy_within(discard..end, 0);
        self.incoming_len -= discard;
    }

    pub fn process_handshake(&mut self, socket: &mut smoltcp::socket::tcp::Socket) -> HandshakeStatus {
        let mut tx_buf = [0u8; 4096];

        for _ in 0..16 {
            let pulled = self.pull_tls_from_socket(socket);
            let status = self
                .conn
                .process_tls_records(&mut self.incoming_tls[..self.incoming_len]);
            let discarded = status.discard;
            let mut should_continue = false;
            let mut outcome: Option<HandshakeStatus> = None;

            match status.state {
                Ok(ConnectionState::EncodeTlsData(mut state)) => {
                     match state.encode(&mut tx_buf) {
                        Ok(len) => {
                            if len > 0 {
                                match socket.send_slice(&tx_buf[..len]) {
                                    Ok(sent) if sent == len => {}
                                    _ => outcome = Some(HandshakeStatus::InProgress),
                                }
                            }
                        }
                        Err(e) => {
                            println(alloc::format!("TLS: encode failed -> {:?}", e).as_str());
                            outcome = Some(HandshakeStatus::Error);
                        }
                    }
                    should_continue = true;
                }
                Ok(ConnectionState::TransmitTlsData(state)) => {
                     state.done();
                     should_continue = true;
                }
                Ok(ConnectionState::BlockedHandshake) => {
                     if pulled == 0 && discarded == 0 {
                         outcome = Some(HandshakeStatus::InProgress);
                     }
                     should_continue = true;
                }
                Ok(ConnectionState::WriteTraffic(_)) | Ok(ConnectionState::ReadTraffic(_)) => {
                    outcome = Some(HandshakeStatus::Done);
                }
                Ok(ConnectionState::Closed) | Ok(ConnectionState::PeerClosed) => {
                    outcome = Some(HandshakeStatus::Error);
                }
                Ok(_) => {}
                Err(e) => {
                    println(alloc::format!("TLS: handshake failed -> {:?}", e).as_str());
                    outcome = Some(HandshakeStatus::Error);
                }
            }

            self.discard_tls_prefix(discarded);
            if let Some(done) = outcome {
                return done;
            }
            if should_continue {
                continue;
            }
            if pulled == 0 && discarded == 0 {
                return HandshakeStatus::InProgress;
            }
        }

        HandshakeStatus::InProgress
    }
    
    pub fn write(&mut self, socket: &mut smoltcp::socket::tcp::Socket, data: &[u8]) -> usize {
        let mut tx_buf = [0u8; 4096];
        let status = self.conn.process_tls_records(&mut []);
        match status.state {
            Ok(ConnectionState::WriteTraffic(mut state)) => {
                match state.encrypt(data, &mut tx_buf) {
                    Ok(len) => {
                         if let Ok(sent) = socket.send_slice(&tx_buf[..len]) {
                             if sent == len {
                                 return data.len();
                             }
                         }
                         0
                    }
                    Err(_) => 0
                }
            }
            _ => 0
        }
    }
    
    pub fn read(&mut self, socket: &mut smoltcp::socket::tcp::Socket, out_buf: &mut [u8]) -> usize {
        let mut total_read = 0;

        for _ in 0..16 {
            let pulled = self.pull_tls_from_socket(socket);
            if self.incoming_len == 0 && pulled == 0 {
                break;
            }

            let status = self
                .conn
                .process_tls_records(&mut self.incoming_tls[..self.incoming_len]);
            let discarded = status.discard;
            let mut should_break = false;

            match status.state {
                Ok(ConnectionState::ReadTraffic(mut state)) => {
                    while let Some(record) = state.next_record() {
                        if let Ok(record) = record {
                            let payload = record.payload;
                            let available = out_buf.len().saturating_sub(total_read);
                            if available == 0 {
                                break;
                            }
                            let to_copy = core::cmp::min(available, payload.len());
                            out_buf[total_read..total_read + to_copy]
                                .copy_from_slice(&payload[..to_copy]);
                            total_read += to_copy;
                        }
                    }
                }
                Ok(ConnectionState::EncodeTlsData(mut state)) => {
                    let mut tx_buf = [0u8; 4096];
                    if let Ok(len) = state.encode(&mut tx_buf) {
                        if len > 0 {
                            let _ = socket.send_slice(&tx_buf[..len]);
                        }
                    }
                }
                Ok(ConnectionState::TransmitTlsData(state)) => {
                    state.done();
                }
                Ok(ConnectionState::BlockedHandshake)
                | Ok(ConnectionState::WriteTraffic(_))
                | Ok(ConnectionState::PeerClosed)
                | Ok(ConnectionState::Closed) => should_break = true,
                Ok(_) => {}
                Err(_) => should_break = true,
            }

            self.discard_tls_prefix(discarded);
            if should_break {
                break;
            }
            if discarded == 0 && pulled == 0 {
                break;
            }
            if total_read >= out_buf.len() {
                break;
            }
        }
        total_read
    }
}
