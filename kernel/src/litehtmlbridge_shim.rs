#![cfg(feature = "litehtml_bridge")]

use alloc::string::String;
use core::cmp::min;
use core::ptr;
use core::slice;
use core::str;

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

fn build_litehtml_payload(
    request_url: &str,
    page: Option<crate::web_engine::BrowserRenderOutput>,
) -> String {
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
            out.push_str("---\n");

            for line in rendered.lines.iter().take(1024) {
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
            out.push_str("---\n");
            out.push_str("LINE: LiteHTML shim: renderer interno devolvio vacio.\n");
        }
    }
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn litehtml_bridge_is_ready() -> i32 {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn litehtml_bridge_render_text(
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
    let payload = build_litehtml_payload(req_url, rendered);
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
