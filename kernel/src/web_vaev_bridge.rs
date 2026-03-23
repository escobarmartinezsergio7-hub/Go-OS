#[cfg(feature = "vaev_bridge")]
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

const VAEV_BRIDGE_TEXT_MAX: usize = 2 * 1024 * 1024;
const VAEV_BRIDGE_MAX_LINES: usize = 640;
const VAEV_BRIDGE_FRAME_MAX_PIXELS: usize = 2 * 1024 * 1024;
const VAEV_BRIDGE_FRAME_MAX_BYTES: usize = VAEV_BRIDGE_FRAME_MAX_PIXELS * 4;

#[derive(Clone)]
pub enum VaevInputEvent {
    Click { x: u32, y: u32 },
    Scroll { delta: i32 },
    Key { key: String },
    Text { text: String },
    Back,
    Forward,
    Reload,
}

impl VaevInputEvent {
    fn to_query(&self) -> String {
        match self {
            Self::Click { x, y } => format!("type=click&x={}&y={}", x, y),
            Self::Scroll { delta } => format!("type=scroll&delta={}", delta),
            Self::Key { key } => {
                format!("type=key&key={}", url_encode_component(key.as_str()))
            }
            Self::Text { text } => {
                format!("type=text&text={}", url_encode_component(text.as_str()))
            }
            Self::Back => String::from("type=back"),
            Self::Forward => String::from("type=forward"),
            Self::Reload => String::from("type=reload"),
        }
    }
}

pub fn feature_enabled() -> bool {
    cfg!(feature = "vaev_bridge")
}

pub fn input_enabled() -> bool {
    cfg!(all(
        feature = "vaev_bridge",
        any(not(feature = "vaev_external"), vaev_external_unavailable)
    ))
}

pub fn binding_mode() -> &'static str {
    #[cfg(not(feature = "vaev_bridge"))]
    {
        "disabled"
    }
    #[cfg(all(
        feature = "vaev_bridge",
        feature = "vaev_external",
        not(vaev_external_unavailable)
    ))]
    {
        "external-libvaevbridge"
    }
    #[cfg(all(
        feature = "vaev_bridge",
        any(not(feature = "vaev_external"), vaev_external_unavailable)
    ))]
    {
        "integrated-shim"
    }
}

pub fn fetch_and_render<F: FnMut()>(
    url: &str,
    pump: &mut F,
) -> crate::web_servo_bridge::ServoBridgeRender {
    #[cfg(not(feature = "vaev_bridge"))]
    {
        fallback_fetch(
            url,
            pump,
            "Vaev bridge no compilado (feature 'vaev_bridge' OFF).",
        )
    }
    #[cfg(feature = "vaev_bridge")]
    {
        fetch_and_render_with_vaev(url, pump)
    }
}

pub fn dispatch_input<F: FnMut()>(
    event: VaevInputEvent,
    pump: &mut F,
) -> crate::web_servo_bridge::ServoBridgeRender {
    #[cfg(not(feature = "vaev_bridge"))]
    {
        let _ = event;
        let _ = pump;
        crate::web_servo_bridge::ServoBridgeRender {
            output: None,
            note: Some(String::from(
                "Vaev input no disponible (feature 'vaev_bridge' OFF).",
            )),
            surface: None,
        }
    }
    #[cfg(feature = "vaev_bridge")]
    {
        dispatch_input_with_vaev(event, pump)
    }
}

fn fallback_fetch<F: FnMut()>(
    url: &str,
    pump: &mut F,
    reason: &str,
) -> crate::web_servo_bridge::ServoBridgeRender {
    let output = crate::web_engine::fetch_and_render(url, pump);
    let surface = output
        .as_ref()
        .and_then(crate::web_servo_bridge::builtin_surface_from_output);
    crate::web_servo_bridge::ServoBridgeRender {
        output,
        note: Some(String::from(reason)),
        surface,
    }
}

#[cfg(feature = "vaev_bridge")]
extern "C" {
    fn vaev_bridge_is_ready() -> i32;
    fn vaev_bridge_render_text(
        url_ptr: *const u8,
        url_len: usize,
        out_ptr: *mut u8,
        out_cap: usize,
        out_len: *mut usize,
    ) -> i32;
}

#[cfg(all(
    feature = "vaev_bridge",
    any(not(feature = "vaev_external"), vaev_external_unavailable)
))]
fn bridge_input_text(
    input_ptr: *const u8,
    input_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    crate::vaevbridge_shim::vaev_bridge_input(input_ptr, input_len, out_ptr, out_cap, out_len)
}

#[cfg(all(
    feature = "vaev_bridge",
    feature = "vaev_external",
    not(vaev_external_unavailable)
))]
fn bridge_input_text(
    _input_ptr: *const u8,
    _input_len: usize,
    _out_ptr: *mut u8,
    _out_cap: usize,
    _out_len: *mut usize,
) -> i32 {
    -127
}

#[cfg(all(
    feature = "vaev_bridge",
    any(not(feature = "vaev_external"), vaev_external_unavailable)
))]
fn bridge_copy_last_frame_rgba(
    out_ptr: *mut u8,
    out_cap: usize,
    out_width: *mut u32,
    out_height: *mut u32,
    out_len: *mut usize,
) -> i32 {
    crate::vaevbridge_shim::vaev_bridge_copy_last_frame_rgba(
        out_ptr,
        out_cap,
        out_width,
        out_height,
        out_len,
    )
}

#[cfg(all(
    feature = "vaev_bridge",
    feature = "vaev_external",
    not(vaev_external_unavailable)
))]
fn bridge_copy_last_frame_rgba(
    _out_ptr: *mut u8,
    _out_cap: usize,
    _out_width: *mut u32,
    _out_height: *mut u32,
    _out_len: *mut usize,
) -> i32 {
    -127
}

#[cfg(feature = "vaev_bridge")]
struct BridgeTextResponse {
    payload: String,
    truncated: bool,
}

#[cfg(feature = "vaev_bridge")]
fn read_text_payload<F: FnMut(), C>(pump: &mut F, call: C) -> Result<BridgeTextResponse, String>
where
    C: FnOnce(*mut u8, usize, *mut usize) -> i32,
{
    let mut text = Vec::new();
    text.resize(VAEV_BRIDGE_TEXT_MAX, 0);
    let mut out_len = 0usize;

    pump();
    let rc = call(text.as_mut_ptr(), text.len(), &mut out_len as *mut usize);
    pump();

    if rc < 0 {
        return Err(format!("vaev bridge rc={}", rc));
    }
    if out_len == 0 {
        return Err(String::from("vaev bridge sin contenido"));
    }
    if out_len > text.len() {
        return Err(String::from("vaev bridge devolvio longitud invalida"));
    }

    text.truncate(out_len);
    let payload = core::str::from_utf8(text.as_slice())
        .map_err(|_| String::from("vaev bridge devolvio texto no UTF-8"))?;
    Ok(BridgeTextResponse {
        payload: String::from(payload),
        truncated: rc > 0,
    })
}

#[cfg(feature = "vaev_bridge")]
fn fetch_text_from_bridge<F: FnMut()>(url: &str, pump: &mut F) -> Result<BridgeTextResponse, String> {
    read_text_payload(pump, |out_ptr, out_cap, out_len| unsafe {
        vaev_bridge_render_text(url.as_ptr(), url.len(), out_ptr, out_cap, out_len)
    })
}

#[cfg(feature = "vaev_bridge")]
fn fetch_text_from_input<F: FnMut()>(
    event: &VaevInputEvent,
    pump: &mut F,
) -> Result<BridgeTextResponse, String> {
    let query = event.to_query();
    read_text_payload(pump, |out_ptr, out_cap, out_len| {
        bridge_input_text(query.as_ptr(), query.len(), out_ptr, out_cap, out_len)
    })
}

#[cfg(feature = "vaev_bridge")]
fn copy_last_frame_from_bridge<F: FnMut()>(
    pump: &mut F,
) -> Result<Option<crate::web_servo_bridge::ServoBridgeSurface>, String> {
    let mut rgba = Vec::new();
    rgba.resize(VAEV_BRIDGE_FRAME_MAX_BYTES, 0);
    let mut out_width = 0u32;
    let mut out_height = 0u32;
    let mut out_len = 0usize;

    pump();
    let rc = bridge_copy_last_frame_rgba(
        rgba.as_mut_ptr(),
        rgba.len(),
        &mut out_width as *mut u32,
        &mut out_height as *mut u32,
        &mut out_len as *mut usize,
    );
    pump();

    if rc == -127 || rc == -2 {
        return Ok(None);
    }
    if rc < 0 {
        return Err(format!("vaev frame rc={}", rc));
    }
    if out_width == 0 || out_height == 0 || out_len == 0 {
        return Ok(None);
    }

    let pixels_len = (out_width as usize)
        .checked_mul(out_height as usize)
        .ok_or_else(|| String::from("vaev frame dimension overflow"))?;
    if pixels_len == 0 || pixels_len > VAEV_BRIDGE_FRAME_MAX_PIXELS {
        return Err(String::from("vaev frame fuera de limite"));
    }

    let expected = pixels_len
        .checked_mul(4)
        .ok_or_else(|| String::from("vaev frame overflow bytes"))?;
    if out_len < expected || expected > rgba.len() {
        return Err(String::from("vaev frame incompleto"));
    }

    let mut pixels = Vec::new();
    pixels.resize(pixels_len, 0);
    let mut src = 0usize;
    for dst in pixels.iter_mut() {
        let r = rgba[src] as u32;
        let g = rgba[src + 1] as u32;
        let b = rgba[src + 2] as u32;
        *dst = (r << 16) | (g << 8) | b;
        src += 4;
    }

    Ok(Some(crate::web_servo_bridge::ServoBridgeSurface {
        source: format!("vaev-bridge-rgba-{}", binding_mode()),
        width: out_width,
        height: out_height,
        pixels,
    }))
}

#[cfg(feature = "vaev_bridge")]
fn fetch_and_render_with_vaev<F: FnMut()>(
    url: &str,
    pump: &mut F,
) -> crate::web_servo_bridge::ServoBridgeRender {
    let ready = unsafe { vaev_bridge_is_ready() };
    if ready <= 0 {
        return fallback_fetch(
            url,
            pump,
            "Vaev bridge no listo; usando motor builtin.",
        );
    }

    let payload = match fetch_text_from_bridge(url, pump) {
        Ok(v) => v,
        Err(reason) => {
            let detail = format!("Vaev bridge fallo ({}). Fallback builtin.", reason);
            return fallback_fetch(url, pump, detail.as_str());
        }
    };

    let output = parse_vaev_text_payload(url, payload.payload.as_str());
    let mut note = format!(
        "renderizado por Vaev bridge embebido (bridge={}).",
        binding_mode()
    );
    if payload.truncated {
        note.push_str(" texto truncado.");
    }

    let surface = match copy_last_frame_from_bridge(pump) {
        Ok(Some(surface)) => Some(surface),
        Ok(None) => crate::web_servo_bridge::builtin_surface_from_output(&output),
        Err(err) => {
            note.push(' ');
            note.push_str(format!("frame bridge no disponible ({})", err).as_str());
            crate::web_servo_bridge::builtin_surface_from_output(&output)
        }
    };

    crate::web_servo_bridge::ServoBridgeRender {
        output: Some(output),
        note: Some(note),
        surface,
    }
}

#[cfg(feature = "vaev_bridge")]
fn dispatch_input_with_vaev<F: FnMut()>(
    event: VaevInputEvent,
    pump: &mut F,
) -> crate::web_servo_bridge::ServoBridgeRender {
    let ready = unsafe { vaev_bridge_is_ready() };
    if ready <= 0 {
        return crate::web_servo_bridge::ServoBridgeRender {
            output: None,
            note: Some(String::from("Vaev bridge no listo para input.")),
            surface: None,
        };
    }

    if !input_enabled() {
        return crate::web_servo_bridge::ServoBridgeRender {
            output: None,
            note: Some(String::from(
                "Vaev input bridge no disponible en modo external-lib actual.",
            )),
            surface: None,
        };
    }

    let payload = match fetch_text_from_input(&event, pump) {
        Ok(v) => v,
        Err(reason) => {
            return crate::web_servo_bridge::ServoBridgeRender {
                output: None,
                note: Some(format!("Vaev input fallo ({})", reason)),
                surface: None,
            };
        }
    };

    let output = parse_vaev_text_payload("about:blank", payload.payload.as_str());
    let mut note = format!("input procesado por Vaev bridge (bridge={}).", binding_mode());
    if payload.truncated {
        note.push_str(" texto truncado.");
    }

    let surface = match copy_last_frame_from_bridge(pump) {
        Ok(Some(surface)) => Some(surface),
        Ok(None) => crate::web_servo_bridge::builtin_surface_from_output(&output),
        Err(err) => {
            note.push(' ');
            note.push_str(format!("frame bridge no disponible ({})", err).as_str());
            crate::web_servo_bridge::builtin_surface_from_output(&output)
        }
    };

    crate::web_servo_bridge::ServoBridgeRender {
        output: Some(output),
        note: Some(note),
        surface,
    }
}

fn parse_vaev_text_payload(request_url: &str, payload: &str) -> crate::web_engine::BrowserRenderOutput {
    let mut status = String::from("Done (Vaev)");
    let mut title: Option<String> = None;
    let mut final_url = String::from(request_url);
    let mut lines: Vec<String> = Vec::new();
    let mut in_body = false;

    for raw in payload.lines() {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim();

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
                if lines.len() < VAEV_BRIDGE_MAX_LINES {
                    lines.push(String::from(v.trim_start()));
                }
                continue;
            }
            in_body = true;
        }

        if lines.len() >= VAEV_BRIDGE_MAX_LINES {
            break;
        }
        lines.push(String::from(line));
    }

    if lines.is_empty() {
        lines.push(String::from("Vaev bridge: respuesta vacia."));
    }

    crate::web_engine::BrowserRenderOutput {
        final_url,
        status,
        title,
        lines,
        surface: None,
    }
}

fn hex_upper(n: u8) -> char {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    HEX[(n & 0x0F) as usize] as char
}

fn url_encode_component(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        let keep = (b >= b'A' && b <= b'Z')
            || (b >= b'a' && b <= b'z')
            || (b >= b'0' && b <= b'9')
            || b == b'-'
            || b == b'_'
            || b == b'.'
            || b == b'~';
        if keep {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex_upper(b >> 4));
            out.push(hex_upper(b));
        }
    }
    out
}
