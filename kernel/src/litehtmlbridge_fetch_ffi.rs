#![cfg(feature = "litehtml_bridge")]

use alloc::string::String;
use core::cmp::min;
use core::ptr;
use core::slice;
use core::str;

fn normalize_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let lower = trimmed
        .as_bytes()
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect::<String>();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        String::from(trimmed)
    } else {
        alloc::format!("https://{}", trimmed)
    }
}

fn split_http_response(raw: &str) -> (String, String) {
    if !raw.starts_with("HTTP/") {
        return (String::from("200 OK"), String::from(raw));
    }

    let (head, body) = if let Some(idx) = raw.find("\r\n\r\n") {
        (&raw[..idx], &raw[idx + 4..])
    } else if let Some(idx) = raw.find("\n\n") {
        (&raw[..idx], &raw[idx + 2..])
    } else {
        (raw, "")
    };

    let status_line = head
        .lines()
        .next()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("HTTP error");

    (String::from(status_line), String::from(body))
}

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

#[unsafe(no_mangle)]
pub extern "C" fn redux_litehtml_fetch_raw(
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
        // SAFETY: pointer/len validated above.
        let bytes = slice::from_raw_parts(url_ptr, url_len);
        match str::from_utf8(bytes) {
            Ok(v) => v,
            Err(_) => return -2,
        }
    };

    let normalized = normalize_url(req_url);
    if normalized.is_empty() {
        return -3;
    }

    let mut pump = || {};
    let Some(raw) = crate::net::http_get_request(normalized.as_str(), &mut pump) else {
        return -4;
    };

    let (status, body) = split_http_response(raw.as_str());
    let mut payload = String::new();
    payload.push_str("STATUS: ");
    payload.push_str(sanitize_inline(status.as_str()).as_str());
    payload.push('\n');
    payload.push_str("FINAL_URL: ");
    payload.push_str(sanitize_inline(normalized.as_str()).as_str());
    payload.push('\n');
    payload.push_str("---\n");
    payload.push_str(body.as_str());

    let bytes = payload.as_bytes();
    let copy_len = min(bytes.len(), out_cap.saturating_sub(1));

    unsafe {
        // SAFETY: output pointer/cap validated above.
        ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr, copy_len);
        *out_ptr.add(copy_len) = 0;
        ptr::write(out_len, copy_len);
    }

    if copy_len < bytes.len() { 1 } else { 0 }
}
