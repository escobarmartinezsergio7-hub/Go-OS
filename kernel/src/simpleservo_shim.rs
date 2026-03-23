#![cfg(feature = "servo_bridge")]

use alloc::string::String;
use core::cmp::min;
use core::ptr;
use core::slice;
use core::str;

const DEMO_FRAME_W: u32 = 96;
const DEMO_FRAME_H: u32 = 72;

fn sanitize_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

fn hash_seed(text: &str) -> u32 {
    let mut h = 2166136261u32;
    for b in text.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

fn push_hex_byte(out: &mut String, value: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(HEX[(value >> 4) as usize] as char);
    out.push(HEX[(value & 0x0F) as usize] as char);
}

fn append_demo_frame(out: &mut String, seed: u32, checker: bool) {
    out.push_str("FRAME_MODE: surface\n");
    out.push_str("FRAME_SIZE: 96x72\n");
    out.push_str("FRAME_ENCODING: RGBA32-HEX\n");

    let w = DEMO_FRAME_W as usize;
    let h = DEMO_FRAME_H as usize;
    for y in 0..h {
        out.push_str("FRAME_DATA: ");
        for x in 0..w {
            let r: u8;
            let g: u8;
            let b: u8;
            if checker {
                let cell = ((x / 8) + (y / 8)) & 1;
                if cell == 0 {
                    r = 0x2A;
                    g = 0x4C;
                    b = 0x7E;
                } else {
                    r = 0xD9;
                    g = 0xE8;
                    b = 0xFF;
                }
            } else {
                let fx = ((x as u32).saturating_mul(255) / (w as u32).max(1)) as u8;
                let fy = ((y as u32).saturating_mul(255) / (h as u32).max(1)) as u8;
                let skew = ((seed.rotate_left(((x + y) as u32) & 7)) & 0xFF) as u8;
                r = fx ^ ((seed & 0x5F) as u8);
                g = fy ^ (((seed >> 8) & 0x5F) as u8);
                b = fx.wrapping_add(fy).wrapping_add(skew >> 2);
            }
            push_hex_byte(out, r);
            push_hex_byte(out, g);
            push_hex_byte(out, b);
            push_hex_byte(out, 0xFF);
        }
        out.push('\n');
    }
}

fn build_servo_payload(request_url: &str, page: Option<crate::web_engine::BrowserRenderOutput>) -> String {
    let mut out = String::new();
    match page {
        Some(rendered) => {
            out.push_str("STATUS: ");
            out.push_str(sanitize_inline(rendered.status.as_str()).as_str());
            out.push('\n');

            if let Some(title) = rendered.title {
                let t = sanitize_inline(title.as_str());
                if !t.trim().is_empty() {
                    out.push_str("TITLE: ");
                    out.push_str(t.as_str());
                    out.push('\n');
                }
            }

            out.push_str("FINAL_URL: ");
            out.push_str(sanitize_inline(rendered.final_url.as_str()).as_str());
            out.push('\n');
            out.push_str("FRAME_MODE: text\n");
            out.push_str("---\n");

            for line in rendered.lines.iter().take(640) {
                out.push_str("LINE: ");
                out.push_str(sanitize_inline(line.as_str()).as_str());
                out.push('\n');
            }
        }
        None => {
            out.push_str("STATUS: Error\n");
            out.push_str("FINAL_URL: ");
            out.push_str(sanitize_inline(request_url).as_str());
            out.push('\n');
            out.push_str("FRAME_MODE: text\n");
            out.push_str("---\n");
            out.push_str("LINE: Servo shim: builtin renderer devolvio vacio.\n");
        }
    }
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn simpleservo_bridge_is_ready() -> i32 {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn simpleservo_bridge_render_text(
    url_ptr: *const u8,
    url_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    if url_ptr.is_null() || out_ptr.is_null() || out_len.is_null() || out_cap == 0 {
        return -1;
    }

    let req_url = unsafe {
        // SAFETY: pointers/len are validated by caller contract above.
        let bytes = slice::from_raw_parts(url_ptr, url_len);
        match str::from_utf8(bytes) {
            Ok(v) => v.trim(),
            Err(_) => return -2,
        }
    };

    if req_url.is_empty() {
        return -3;
    }

    let mut pump = || {};
    let rendered = crate::web_engine::fetch_and_render(req_url, &mut pump);
    let payload = build_servo_payload(req_url, rendered);
    let payload_bytes = payload.as_bytes();
    let copy_len = min(payload_bytes.len(), out_cap.saturating_sub(1));

    unsafe {
        // SAFETY: out buffer is validated by caller contract and copy_len is bounded by out_cap.
        ptr::copy_nonoverlapping(payload_bytes.as_ptr(), out_ptr, copy_len);
        *out_ptr.add(copy_len) = 0;
        ptr::write(out_len, copy_len);
    }

    if payload_bytes.len() > copy_len {
        1
    } else {
        0
    }
}
