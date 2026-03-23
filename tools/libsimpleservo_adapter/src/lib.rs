#![no_std]

use core::fmt::{self, Write};
use core::panic::PanicInfo;
use core::ptr;
use core::slice;
use core::str;

const PAYLOAD_STACK_CAP: usize = 16 * 1024;
const URL_MAX_BYTES: usize = 1024;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

struct FixedBuf<'a> {
    out: &'a mut [u8],
    len: usize,
}

impl<'a> FixedBuf<'a> {
    fn new(out: &'a mut [u8]) -> Self {
        Self { out, len: 0 }
    }

    fn push_bytes(&mut self, src: &[u8]) {
        if src.is_empty() || self.len >= self.out.len() {
            return;
        }
        let free = self.out.len().saturating_sub(self.len);
        let take = src.len().min(free);
        self.out[self.len..self.len + take].copy_from_slice(&src[..take]);
        self.len += take;
    }

    fn push_ascii_sanitized(&mut self, src: &[u8]) {
        for &b in src.iter().take(URL_MAX_BYTES) {
            let c = if b.is_ascii_graphic() || b == b' ' { b } else { b'?' };
            self.push_bytes(&[c]);
            if self.len >= self.out.len() {
                break;
            }
        }
    }

    fn as_used(&self) -> &[u8] {
        &self.out[..self.len]
    }
}

impl Write for FixedBuf<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_bytes(s.as_bytes());
        Ok(())
    }
}

fn default_url() -> &'static [u8] {
    b"about:blank"
}

fn sanitize_url_from_ffi(url_ptr: *const u8, url_len: usize, out: &mut FixedBuf<'_>) {
    if url_ptr.is_null() || url_len == 0 {
        out.push_bytes(default_url());
        return;
    }

    let raw = unsafe { slice::from_raw_parts(url_ptr, url_len.min(URL_MAX_BYTES)) };
    // Prefer UTF-8 if possible; otherwise sanitize byte-by-byte.
    if let Ok(text) = str::from_utf8(raw) {
        out.push_bytes(text.trim().as_bytes());
        if out.len == 0 {
            out.push_bytes(default_url());
        }
        return;
    }
    out.push_ascii_sanitized(raw);
    if out.len == 0 {
        out.push_bytes(default_url());
    }
}

#[no_mangle]
pub extern "C" fn simpleservo_bridge_is_ready() -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn simpleservo_bridge_render_text(
    url_ptr: *const u8,
    url_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    if out_ptr.is_null() || out_len.is_null() || out_cap == 0 {
        return -1;
    }

    let mut payload = [0u8; PAYLOAD_STACK_CAP];
    let mut w = FixedBuf::new(payload.as_mut_slice());
    let mut url_buf = [0u8; URL_MAX_BYTES];
    let mut url = FixedBuf::new(url_buf.as_mut_slice());
    sanitize_url_from_ffi(url_ptr, url_len, &mut url);

    let _ = writeln!(w, "STATUS: Done (External Servo Adapter)");
    let _ = writeln!(w, "TITLE: Servo Adapter");
    let _ = write!(w, "FINAL_URL: ");
    w.push_bytes(url.as_used());
    let _ = writeln!(w);
    let _ = writeln!(w, "FRAME_MODE: preview");
    let _ = writeln!(w, "FRAME_SIZE: 640x360");
    let _ = writeln!(w, "---");
    let _ = writeln!(w, "LINE: External libsimpleservo adapter linked.");
    let _ = write!(w, "LINE: URL ");
    w.push_bytes(url.as_used());
    let _ = writeln!(w);
    let _ = writeln!(w, "LINE: Este adapter usa preview surface (bridge ABI v1).");
    let _ = writeln!(
        w,
        "LINE: Sustituye este lib por uno con Servo embebido real cuando este listo."
    );

    let used = w.as_used();
    if used.is_empty() || used.len() > out_cap {
        unsafe {
            ptr::write(out_len, 0);
        }
        return -2;
    }

    unsafe {
        ptr::copy_nonoverlapping(used.as_ptr(), out_ptr, used.len());
        ptr::write(out_len, used.len());
    }

    0
}
