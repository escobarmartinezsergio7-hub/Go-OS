#[cfg(feature = "servo_bridge")]
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

const SERVO_BRIDGE_TEXT_MAX: usize = 2 * 1024 * 1024;
const SERVO_API_ADAPTER_SPIN_STEPS: usize = 3;
const SERVO_SURFACE_PREVIEW_W: u32 = 320;
const SERVO_SURFACE_PREVIEW_H: u32 = 200;
const SERVO_FRAME_MAX_BYTES: usize = 8 * 1024 * 1024;
const BUILTIN_SURFACE_PREVIEW_W: u32 = 640;
const BUILTIN_SURFACE_PREVIEW_H: u32 = 360;

pub struct ServoBridgeSurface {
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

pub struct ServoBridgeRender {
    pub output: Option<crate::web_engine::BrowserRenderOutput>,
    pub note: Option<String>,
    pub surface: Option<ServoBridgeSurface>,
}

impl ServoBridgeRender {
    fn fallback<F: FnMut()>(url: &str, pump: &mut F, reason: &str) -> Self {
        Self {
            output: crate::web_engine::fetch_and_render(url, pump),
            note: Some(String::from(reason)),
            surface: None,
        }
    }
}

pub fn builtin_surface_from_output(
    output: &crate::web_engine::BrowserRenderOutput,
) -> Option<ServoBridgeSurface> {
    if let Some(surface) = output.surface.as_ref() {
        return Some(ServoBridgeSurface {
            source: surface.source.clone(),
            width: surface.width,
            height: surface.height,
            pixels: surface.pixels.clone(),
        });
    }

    let width = BUILTIN_SURFACE_PREVIEW_W;
    let height = BUILTIN_SURFACE_PREVIEW_H;
    let total = (width as usize).saturating_mul(height as usize);
    if total == 0 {
        return None;
    }

    let mut pixels = Vec::new();
    pixels.resize(total, 0xF8FBFF);
    fill_preview_surface(
        width,
        height,
        pixels.as_mut_slice(),
        output.title.as_deref(),
        output.lines.as_slice(),
    );

    Some(ServoBridgeSurface {
        source: String::from("builtin-htmlcssjs-subset"),
        width,
        height,
        pixels,
    })
}

pub fn fetch_and_render<F: FnMut()>(url: &str, pump: &mut F) -> ServoBridgeRender {
    #[cfg(feature = "servo_bridge")]
    {
        fetch_and_render_with_servo(url, pump)
    }
    #[cfg(not(feature = "servo_bridge"))]
    {
        ServoBridgeRender::fallback(
            url,
            pump,
            "Servo bridge no compilado (feature 'servo_bridge' OFF).",
        )
    }
}

pub fn feature_enabled() -> bool {
    cfg!(feature = "servo_bridge")
}

pub fn api_profile() -> &'static str {
    #[cfg(not(feature = "servo_bridge"))]
    {
        "disabled"
    }
    #[cfg(feature = "servo_bridge")]
    {
        "servo::Servo + webview::WebView adapter v1 (doc.servo.org/servo)"
    }
}

pub fn binding_mode() -> &'static str {
    #[cfg(not(feature = "servo_bridge"))]
    {
        "disabled"
    }
    #[cfg(all(
        feature = "servo_bridge",
        feature = "servo_external",
        not(servo_external_unavailable)
    ))]
    {
        "external-libsimpleservo"
    }
    #[cfg(all(
        feature = "servo_bridge",
        any(not(feature = "servo_external"), servo_external_unavailable)
    ))]
    {
        "integrated-shim"
    }
}

#[cfg(feature = "servo_bridge")]
extern "C" {
    fn simpleservo_bridge_is_ready() -> i32;
    fn simpleservo_bridge_render_text(
        url_ptr: *const u8,
        url_len: usize,
        out_ptr: *mut u8,
        out_cap: usize,
        out_len: *mut usize,
    ) -> i32;
}

#[cfg(feature = "servo_bridge")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ServoApiStage {
    Created,
    Loaded,
    EventLoopSpun,
    Painted,
}

#[cfg(feature = "servo_bridge")]
struct ServoWebView {
    url: String,
    stage: ServoApiStage,
}

#[cfg(feature = "servo_bridge")]
struct ServoApiSession {
    is_ready: bool,
    event_loop_ticks: u32,
}

#[cfg(feature = "servo_bridge")]
impl ServoApiSession {
    fn build() -> Result<Self, &'static str> {
        let ready = unsafe { simpleservo_bridge_is_ready() };
        if ready <= 0 {
            return Err("servo bridge no listo");
        }
        Ok(Self {
            is_ready: true,
            event_loop_ticks: 0,
        })
    }

    fn new_webview(&mut self, url: &str) -> Result<ServoWebView, &'static str> {
        if !self.is_ready {
            return Err("servo bridge no inicializado");
        }
        if url.trim().is_empty() {
            return Err("url vacia");
        }
        Ok(ServoWebView {
            url: String::from(url.trim()),
            stage: ServoApiStage::Loaded,
        })
    }

    fn spin_event_loop<F: FnMut()>(&mut self, pump: &mut F, webview: &mut ServoWebView) {
        if !self.is_ready {
            return;
        }
        self.event_loop_ticks = self.event_loop_ticks.saturating_add(1);
        pump();
        webview.stage = ServoApiStage::EventLoopSpun;
    }

    fn paint_webview_text<F: FnMut()>(
        &mut self,
        webview: &mut ServoWebView,
        pump: &mut F,
    ) -> Result<String, String> {
        if !self.is_ready {
            return Err(String::from("servo bridge no inicializado"));
        }
        if webview.stage == ServoApiStage::Created {
            return Err(String::from("webview sin carga"));
        }

        let payload = fetch_text_from_legacy_bridge(webview.url.as_str(), pump)?;
        webview.stage = ServoApiStage::Painted;
        Ok(payload)
    }
}

#[cfg(feature = "servo_bridge")]
fn fetch_text_from_legacy_bridge<F: FnMut()>(url: &str, pump: &mut F) -> Result<String, String> {
    pump();
    let mut text = Vec::new();
    text.resize(SERVO_BRIDGE_TEXT_MAX, 0);
    let mut out_len = 0usize;

    let rc = unsafe {
        simpleservo_bridge_render_text(
            url.as_ptr(),
            url.len(),
            text.as_mut_ptr(),
            text.len(),
            &mut out_len as *mut usize,
        )
    };
    pump();

    if rc != 0 {
        return Err(format!("servo bridge rc={}", rc));
    }
    if out_len == 0 {
        return Err(String::from("servo bridge sin contenido"));
    }
    if out_len > text.len() {
        return Err(String::from("servo bridge devolvio longitud invalida"));
    }

    text.truncate(out_len);
    let payload = core::str::from_utf8(text.as_slice())
        .map_err(|_| String::from("servo bridge devolvio texto no UTF-8"))?;
    Ok(String::from(payload))
}

#[cfg(feature = "servo_bridge")]
fn fetch_and_render_with_servo<F: FnMut()>(url: &str, pump: &mut F) -> ServoBridgeRender {
    let mut session = match ServoApiSession::build() {
        Ok(v) => v,
        Err(_) => {
            return ServoBridgeRender::fallback(
                url,
                pump,
                "Servo bridge no listo; usando motor builtin.",
            );
        }
    };

    let mut webview = match session.new_webview(url) {
        Ok(v) => v,
        Err(_) => {
            return ServoBridgeRender::fallback(
                url,
                pump,
                "Servo webview invalida; fallback builtin.",
            );
        }
    };

    for _ in 0..SERVO_API_ADAPTER_SPIN_STEPS {
        session.spin_event_loop(pump, &mut webview);
    }

    let payload = match session.paint_webview_text(&mut webview, pump) {
        Ok(v) => v,
        Err(reason) => {
            return ServoBridgeRender::fallback(
                url,
                pump,
                format!("Servo paint fallo ({}). Fallback builtin.", reason).as_str(),
            );
        }
    };

    let (rendered, surface) = parse_servo_text_payload(url, payload.as_str());
    ServoBridgeRender {
        output: Some(rendered),
        note: Some(format!(
            "renderizado por adaptador Rust Servo/WebView ({}, bridge={}).",
            api_profile(),
            binding_mode()
        )),
        surface,
    }
}

fn parse_servo_text_payload(
    request_url: &str,
    payload: &str,
) -> (crate::web_engine::BrowserRenderOutput, Option<ServoBridgeSurface>) {
    let mut status = String::from("Done (Servo)");
    let mut title: Option<String> = None;
    let mut final_url = String::from(request_url);
    let mut lines: Vec<String> = Vec::new();
    let mut frame_mode: Option<String> = None;
    let mut frame_size: Option<(u32, u32)> = None;
    let mut frame_encoding: Option<String> = None;
    let mut frame_hex_chunks: Vec<String> = Vec::new();
    let mut frame_hex_chars: usize = 0;

    let mut in_body = false;
    for raw in payload.lines() {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim();
        if let Some(v) = trimmed.strip_prefix("FRAME_MODE:") {
            let value = v.trim();
            if !value.is_empty() {
                frame_mode = Some(String::from(value));
            }
            continue;
        }
        if let Some(v) = trimmed.strip_prefix("FRAME_SIZE:") {
            if let Some((w, h)) = parse_frame_size(v.trim()) {
                frame_size = Some((w, h));
            }
            continue;
        }
        if let Some(v) = trimmed.strip_prefix("FRAME_ENCODING:") {
            let value = v.trim();
            if !value.is_empty() {
                frame_encoding = Some(String::from(value));
            }
            continue;
        }
        if let Some(v) = trimmed.strip_prefix("FRAME_DATA:") {
            let chunk = v.trim();
            if !chunk.is_empty() {
                let next_len = frame_hex_chars.saturating_add(chunk.len());
                if next_len <= SERVO_FRAME_MAX_BYTES.saturating_mul(2) {
                    frame_hex_chunks.push(String::from(chunk));
                    frame_hex_chars = next_len;
                }
            }
            continue;
        }

        if !in_body {
            if trimmed.is_empty() || trimmed == "---" {
                in_body = true;
                continue;
            }
            if let Some(v) = trimmed.strip_prefix("STATUS:") {
                let value = v.trim();
                if !value.is_empty() {
                    status = String::from(value);
                }
                continue;
            }
            if let Some(v) = trimmed.strip_prefix("TITLE:") {
                let value = v.trim();
                if !value.is_empty() {
                    title = Some(String::from(value));
                }
                continue;
            }
            if let Some(v) = trimmed.strip_prefix("FINAL_URL:") {
                let value = v.trim();
                if !value.is_empty() {
                    final_url = String::from(value);
                }
                continue;
            }
            if let Some(v) = trimmed.strip_prefix("LINE:") {
                lines.push(String::from(v.trim_start()));
                continue;
            }

            in_body = true;
        }

        if lines.len() >= 512 {
            break;
        }
        lines.push(String::from(line));
    }

    if lines.is_empty() {
        lines.push(String::from("Servo bridge: respuesta vacia."));
    }

    let output = crate::web_engine::BrowserRenderOutput {
        final_url,
        status,
        title: title.clone(),
        lines,
        surface: None,
    };

    let surface = build_surface_from_payload(
        title.as_deref(),
        output.lines.as_slice(),
        frame_mode.as_deref(),
        frame_size,
        frame_encoding.as_deref(),
        frame_hex_chunks.as_slice(),
    );

    (output, surface)
}

fn parse_frame_size(value: &str) -> Option<(u32, u32)> {
    let mut parts = value.split('x');
    let w_str = parts.next()?.trim();
    let h_str = parts.next()?.trim();
    if parts.next().is_some() {
        return None;
    }
    let w = w_str.parse::<u32>().ok()?;
    let h = h_str.parse::<u32>().ok()?;
    if w == 0 || h == 0 || w > 1920 || h > 1080 {
        return None;
    }
    Some((w, h))
}

fn build_surface_from_payload(
    title: Option<&str>,
    lines: &[String],
    frame_mode: Option<&str>,
    frame_size: Option<(u32, u32)>,
    frame_encoding: Option<&str>,
    frame_hex_chunks: &[String],
) -> Option<ServoBridgeSurface> {
    let (width, height) = frame_size.unwrap_or((SERVO_SURFACE_PREVIEW_W, SERVO_SURFACE_PREVIEW_H));
    let total = (width as usize).saturating_mul(height as usize);
    if total == 0 || total > (1920usize * 1080usize) {
        return None;
    }

    let mode = frame_mode.unwrap_or("none");
    let encoding = frame_encoding.unwrap_or("");

    if mode.eq_ignore_ascii_case("none") || mode.eq_ignore_ascii_case("text") {
        return None;
    }

    if mode.eq_ignore_ascii_case("surface")
        || mode.eq_ignore_ascii_case("real")
        || mode.eq_ignore_ascii_case("raster")
    {
        if encoding.eq_ignore_ascii_case("RGBA32-HEX")
            || encoding.eq_ignore_ascii_case("RGBA-HEX")
            || encoding.eq_ignore_ascii_case("RGBA")
        {
            if let Some(mut pixels) = decode_rgba_hex_frame(width, height, frame_hex_chunks) {
                // Subtle header strip for readability in browser window.
                draw_header_strip(width, pixels.as_mut_slice());
                return Some(ServoBridgeSurface {
                    source: String::from("servo-rgba-surface"),
                    width,
                    height,
                    pixels,
                });
            }
        }
    }

    let mut pixels = Vec::new();
    pixels.resize(total, 0xFFFFFF);
    if mode.eq_ignore_ascii_case("checker") || mode.eq_ignore_ascii_case("test") {
        fill_checker_surface(width, height, pixels.as_mut_slice());
        return Some(ServoBridgeSurface {
            source: String::from("servo-frame-checker"),
            width,
            height,
            pixels,
        });
    }

    if mode.eq_ignore_ascii_case("preview") {
        fill_preview_surface(width, height, pixels.as_mut_slice(), title, lines);
        return Some(ServoBridgeSurface {
            source: String::from("servo-preview-surface"),
            width,
            height,
            pixels,
        });
    }

    None
}

fn draw_header_strip(width: u32, pixels: &mut [u32]) {
    let w = width as usize;
    let rows = 20usize;
    for y in 0..rows {
        for x in 0..w {
            let idx = y.saturating_mul(w).saturating_add(x);
            if idx >= pixels.len() {
                continue;
            }
            // Blend toward dark slate to keep top strip readable.
            let px = pixels[idx];
            let r = ((px >> 16) & 0xFF) as u8;
            let g = ((px >> 8) & 0xFF) as u8;
            let b = (px & 0xFF) as u8;
            let nr = ((r as u16 + 0x20u16) / 3) as u8;
            let ng = ((g as u16 + 0x36u16) / 3) as u8;
            let nb = ((b as u16 + 0x4Fu16) / 3) as u8;
            pixels[idx] = ((nr as u32) << 16) | ((ng as u32) << 8) | (nb as u32);
        }
    }
}

fn decode_rgba_hex_frame(width: u32, height: u32, chunks: &[String]) -> Option<Vec<u32>> {
    let expected_px = (width as usize).saturating_mul(height as usize);
    if expected_px == 0 || expected_px > (1920usize * 1080usize) {
        return None;
    }
    let expected_bytes = expected_px.saturating_mul(4);
    if expected_bytes > SERVO_FRAME_MAX_BYTES {
        return None;
    }

    let bytes = decode_hex_chunks(chunks, expected_bytes)?;
    if bytes.len() < expected_bytes {
        return None;
    }

    let mut pixels = Vec::new();
    pixels.resize(expected_px, 0);
    for i in 0..expected_px {
        let base = i.saturating_mul(4);
        let r = bytes[base];
        let g = bytes[base + 1];
        let b = bytes[base + 2];
        pixels[i] = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
    }
    Some(pixels)
}

fn decode_hex_chunks(chunks: &[String], expected_bytes: usize) -> Option<Vec<u8>> {
    if expected_bytes == 0 || expected_bytes > SERVO_FRAME_MAX_BYTES {
        return None;
    }
    let mut out = Vec::new();
    out.reserve(expected_bytes);
    let mut hi: Option<u8> = None;

    for chunk in chunks.iter() {
        for b in chunk.bytes() {
            if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
                continue;
            }
            let nib = hex_nibble(b)?;
            if let Some(h) = hi.take() {
                out.push((h << 4) | nib);
                if out.len() >= expected_bytes {
                    return Some(out);
                }
            } else {
                hi = Some(nib);
            }
        }
    }

    if hi.is_some() {
        return None;
    }
    Some(out)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn fill_checker_surface(width: u32, height: u32, pixels: &mut [u32]) {
    let w = width as usize;
    let h = height as usize;
    for y in 0..h {
        for x in 0..w {
            let idx = y.saturating_mul(w).saturating_add(x);
            if idx >= pixels.len() {
                continue;
            }
            let cell = ((x / 20) + (y / 20)) & 1;
            pixels[idx] = if cell == 0 { 0x2A4C7E } else { 0xD9E8FF };
        }
    }
}

fn fill_preview_surface(
    width: u32,
    height: u32,
    pixels: &mut [u32],
    title: Option<&str>,
    lines: &[String],
) {
    let w = width as usize;
    let h = height as usize;
    for y in 0..h {
        for x in 0..w {
            let idx = y.saturating_mul(w).saturating_add(x);
            if idx >= pixels.len() {
                continue;
            }
            pixels[idx] = if y < 28 { 0x20364F } else { 0xF8FBFF };
        }
    }

    let title_text = match title {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => "Servo Preview",
    };
    draw_text_surface(width, height, pixels, 8, 10, title_text, 0xFFFFFF);

    let mut y = 34usize;
    for line in lines.iter().take(80) {
        if y + 8 >= h {
            break;
        }
        let txt = line.trim();
        if txt.is_empty() {
            y = y.saturating_add(8);
            continue;
        }
        let color = if txt.contains("http://") || txt.contains("https://") {
            0x2357C8
        } else {
            0x121820
        };
        draw_text_surface(
            width,
            height,
            pixels,
            8,
            y,
            trim_surface_line(txt, ((w.saturating_sub(16)) / 6).max(1)).as_str(),
            color,
        );
        y = y.saturating_add(9);
    }
}

fn trim_surface_line(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if text.len() <= max_chars {
        return String::from(text);
    }
    let mut out = String::new();
    for b in text.bytes().take(max_chars.saturating_sub(3)) {
        out.push(if b.is_ascii() { b as char } else { '?' });
    }
    out.push_str("...");
    out
}

fn draw_text_surface(
    width: u32,
    height: u32,
    pixels: &mut [u32],
    x: usize,
    y: usize,
    text: &str,
    color: u32,
) {
    let mut cx = x;
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            break;
        }
        draw_char_surface(width, height, pixels, cx, y, ch, color);
        cx = cx.saturating_add(6);
        if cx + 5 >= width as usize {
            break;
        }
    }
}

fn draw_char_surface(
    width: u32,
    height: u32,
    pixels: &mut [u32],
    x: usize,
    y: usize,
    ch: char,
    color: u32,
) {
    let glyph = crate::font::glyph_5x7(if ch.is_ascii() { ch } else { '?' });
    for (row, bits) in glyph.iter().enumerate() {
        let py = y.saturating_add(row);
        if py >= height as usize {
            continue;
        }
        for col in 0..5usize {
            let mask = 1 << (4 - col);
            if (*bits & mask) == 0 {
                continue;
            }
            let px = x.saturating_add(col);
            if px >= width as usize {
                continue;
            }
            let idx = py.saturating_mul(width as usize).saturating_add(px);
            if idx < pixels.len() {
                pixels[idx] = color;
            }
        }
    }
}
