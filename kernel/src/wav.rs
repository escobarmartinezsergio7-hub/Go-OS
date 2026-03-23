// WAV/RIFF File Parser
// Supports uncompressed PCM WAV files (format tag = 1)
// and G.711 encoded WAV files (format tag = 6 a-law, 7 mu-law).

use alloc::vec::Vec;

/// Parsed WAV file header information.
pub struct WavFile<'a> {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub data: &'a [u8],
}

/// Decoded WAV that owns its PCM data (used for G.711 → PCM conversion).
pub struct WavFileOwned {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub data: Vec<u8>,
}

// ─── G.711 mu-law → 16-bit PCM decode ──────────────────────────────────────

fn ulaw_to_pcm16(u_val: u8) -> i16 {
    // ITU-T G.711 mu-law decoding
    let u_val = !u_val;
    let sign = if (u_val & 0x80) != 0 { -1i16 } else { 1i16 };
    let exponent = ((u_val >> 4) & 0x07) as i16;
    let mantissa = (u_val & 0x0F) as i16;
    let magnitude = ((mantissa << 1) | 0x21) << (exponent + 2);
    let sample = sign * (magnitude - 0x21);
    sample.max(-32768).min(32767)
}

// ─── G.711 a-law → 16-bit PCM decode ───────────────────────────────────────

fn alaw_to_pcm16(a_val: u8) -> i16 {
    // ITU-T G.711 A-law decoding
    let a_val = a_val ^ 0x55;
    let sign = if (a_val & 0x80) != 0 { -1i16 } else { 1i16 };
    let exponent = ((a_val >> 4) & 0x07) as u32;
    let mantissa = (a_val & 0x0F) as i16;

    let sample = if exponent == 0 {
        (mantissa << 4) | 0x08
    } else {
        ((mantissa << 4) | 0x108) << (exponent - 1)
    };
    (sign * sample).max(-32768).min(32767)
}

/// Decode G.711 data (mu-law or a-law) to 16-bit PCM.
fn decode_g711(data: &[u8], is_alaw: bool) -> Vec<u8> {
    let mut pcm = Vec::with_capacity(data.len() * 2);
    for &byte in data {
        let sample = if is_alaw {
            alaw_to_pcm16(byte)
        } else {
            ulaw_to_pcm16(byte)
        };
        let le = sample.to_le_bytes();
        pcm.push(le[0]);
        pcm.push(le[1]);
    }
    pcm
}

const FORMAT_PCM: u16 = 1;
const FORMAT_ALAW: u16 = 6;
const FORMAT_MULAW: u16 = 7;

/// Parse a WAV file from a byte slice.
/// Returns `None` if the file is not a valid PCM WAV.
pub fn parse_wav(buf: &[u8]) -> Option<WavFile<'_>> {
    if buf.len() < 44 {
        return None;
    }

    // Check RIFF header
    if &buf[0..4] != b"RIFF" {
        return None;
    }

    // Check WAVE format
    if &buf[8..12] != b"WAVE" {
        return None;
    }

    // Find "fmt " chunk
    let mut pos = 12usize;
    let mut sample_rate: u32 = 0;
    let mut channels: u16 = 0;
    let mut bits_per_sample: u16 = 0;
    let mut fmt_found = false;
    let mut format_tag: u16 = 0;

    while pos + 8 <= buf.len() {
        let chunk_id = &buf[pos..pos + 4];
        let chunk_size = u32::from_le_bytes([
            buf[pos + 4],
            buf[pos + 5],
            buf[pos + 6],
            buf[pos + 7],
        ]) as usize;

        if chunk_id == b"fmt " {
            if pos + 8 + 16 > buf.len() {
                return None;
            }

            let data = &buf[pos + 8..];

            format_tag = u16::from_le_bytes([data[0], data[1]]);
            if format_tag != FORMAT_PCM {
                // Not PCM — check if G.711 (handled separately via parse_wav_any)
                return None;
            }

            channels = u16::from_le_bytes([data[2], data[3]]);
            sample_rate = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            // bytes_per_sec at [8..12], block_align at [12..14]
            bits_per_sample = u16::from_le_bytes([data[14], data[15]]);
            fmt_found = true;
        }

        if chunk_id == b"data" && fmt_found {
            let data_start = pos + 8;
            let data_end = (data_start + chunk_size).min(buf.len());

            if data_start >= buf.len() {
                return None;
            }

            return Some(WavFile {
                sample_rate,
                channels,
                bits_per_sample,
                data: &buf[data_start..data_end],
            });
        }

        // Advance to next chunk (chunks are word-aligned)
        pos += 8 + chunk_size;
        if chunk_size % 2 != 0 {
            pos += 1; // padding byte
        }
    }

    // "data" chunk not found — try returning with fmt info if we at least found fmt
    if fmt_found {
        // If we found fmt but no data chunk, the remaining bytes after fmt are data
        // This handles some non-standard WAV files
        return None;
    }

    None
}

/// Parse a WAV file, supporting PCM, mu-law, and a-law formats.
/// For G.711 formats, returns a WavFileOwned with the decoded PCM data.
/// For PCM, returns None in the owned variant (caller should use parse_wav instead).
pub fn parse_wav_any(buf: &[u8]) -> Option<WavFileOwned> {
    if buf.len() < 44 {
        return None;
    }

    if &buf[0..4] != b"RIFF" {
        return None;
    }
    if &buf[8..12] != b"WAVE" {
        return None;
    }

    let mut pos = 12usize;
    let mut sample_rate: u32 = 0;
    let mut channels: u16 = 0;
    let mut bits_per_sample: u16 = 0;
    let mut fmt_found = false;
    let mut format_tag: u16 = 0;

    while pos + 8 <= buf.len() {
        let chunk_id = &buf[pos..pos + 4];
        let chunk_size = u32::from_le_bytes([
            buf[pos + 4],
            buf[pos + 5],
            buf[pos + 6],
            buf[pos + 7],
        ]) as usize;

        if chunk_id == b"fmt " {
            if pos + 8 + 16 > buf.len() {
                return None;
            }
            let data = &buf[pos + 8..];
            format_tag = u16::from_le_bytes([data[0], data[1]]);
            channels = u16::from_le_bytes([data[2], data[3]]);
            sample_rate = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            bits_per_sample = u16::from_le_bytes([data[14], data[15]]);
            fmt_found = true;
        }

        if chunk_id == b"data" && fmt_found {
            let data_start = pos + 8;
            let data_end = (data_start + chunk_size).min(buf.len());

            if data_start >= buf.len() {
                return None;
            }

            let raw_data = &buf[data_start..data_end];

            match format_tag {
                FORMAT_PCM => {
                    return Some(WavFileOwned {
                        sample_rate,
                        channels,
                        bits_per_sample,
                        data: raw_data.to_vec(),
                    });
                }
                FORMAT_ALAW => {
                    let pcm_data = decode_g711(raw_data, true);
                    return Some(WavFileOwned {
                        sample_rate,
                        channels,
                        bits_per_sample: 16,
                        data: pcm_data,
                    });
                }
                FORMAT_MULAW => {
                    let pcm_data = decode_g711(raw_data, false);
                    return Some(WavFileOwned {
                        sample_rate,
                        channels,
                        bits_per_sample: 16,
                        data: pcm_data,
                    });
                }
                _ => {
                    return None;
                }
            }
        }

        pos += 8 + chunk_size;
        if chunk_size % 2 != 0 {
            pos += 1;
        }
    }

    None
}

/// Calculate the duration in milliseconds of a WAV file.
pub fn duration_ms(wav: &WavFile<'_>) -> u32 {
    if wav.sample_rate == 0 || wav.channels == 0 || wav.bits_per_sample == 0 {
        return 0;
    }
    let bytes_per_sample = (wav.bits_per_sample / 8) as u32;
    let bytes_per_second = wav.sample_rate * wav.channels as u32 * bytes_per_sample;
    if bytes_per_second == 0 {
        return 0;
    }
    ((wav.data.len() as u64 * 1000) / bytes_per_second as u64) as u32
}

/// Calculate duration for owned WAV.
pub fn duration_ms_owned(wav: &WavFileOwned) -> u32 {
    if wav.sample_rate == 0 || wav.channels == 0 || wav.bits_per_sample == 0 {
        return 0;
    }
    let bytes_per_sample = (wav.bits_per_sample / 8) as u32;
    let bytes_per_second = wav.sample_rate * wav.channels as u32 * bytes_per_sample;
    if bytes_per_second == 0 {
        return 0;
    }
    ((wav.data.len() as u64 * 1000) / bytes_per_second as u64) as u32
}

/// Format a duration in milliseconds as "MM:SS".
pub fn format_time(ms: u32) -> alloc::string::String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    alloc::format!("{:02}:{:02}", mins, secs)
}
