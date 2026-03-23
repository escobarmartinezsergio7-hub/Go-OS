#[cfg(feature = "litehtml_bridge")]
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

const LITEHTML_BRIDGE_TEXT_MAX: usize = 2 * 1024 * 1024;
const LITEHTML_BRIDGE_MAX_LINES: usize = 1024;

pub fn feature_enabled() -> bool {
    cfg!(feature = "litehtml_bridge")
}

pub fn binding_mode() -> &'static str {
    #[cfg(not(feature = "litehtml_bridge"))]
    {
        "disabled"
    }
    #[cfg(all(
        feature = "litehtml_bridge",
        feature = "litehtml_external",
        not(litehtml_external_unavailable)
    ))]
    {
        "external-liblitehtmlbridge"
    }
    #[cfg(all(
        feature = "litehtml_bridge",
        any(not(feature = "litehtml_external"), litehtml_external_unavailable)
    ))]
    {
        "integrated-shim"
    }
}

pub fn fetch_and_render<F: FnMut()>(
    url: &str,
    pump: &mut F,
) -> crate::web_servo_bridge::ServoBridgeRender {
    #[cfg(not(feature = "litehtml_bridge"))]
    {
        fallback_fetch(
            url,
            pump,
            "LiteHTML bridge no compilado (feature 'litehtml_bridge' OFF).",
        )
    }
    #[cfg(feature = "litehtml_bridge")]
    {
        fetch_and_render_with_litehtml(url, pump)
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

#[cfg(feature = "litehtml_bridge")]
extern "C" {
    fn litehtml_bridge_is_ready() -> i32;
    fn litehtml_bridge_render_text(
        url_ptr: *const u8,
        url_len: usize,
        out_ptr: *mut u8,
        out_cap: usize,
        out_len: *mut usize,
    ) -> i32;
}

#[cfg(feature = "litehtml_bridge")]
struct BridgeTextResponse {
    payload: String,
    truncated: bool,
}

#[cfg(feature = "litehtml_bridge")]
fn read_text_payload<F: FnMut(), C>(pump: &mut F, call: C) -> Result<BridgeTextResponse, String>
where
    C: FnOnce(*mut u8, usize, *mut usize) -> i32,
{
    let mut text = Vec::new();
    text.resize(LITEHTML_BRIDGE_TEXT_MAX, 0);
    let mut out_len = 0usize;

    pump();
    let rc = call(text.as_mut_ptr(), text.len(), &mut out_len as *mut usize);
    pump();

    if rc < 0 {
        return Err(format!("litehtml bridge rc={}", rc));
    }
    if out_len == 0 {
        return Err(String::from("litehtml bridge sin contenido"));
    }
    if out_len > text.len() {
        return Err(String::from("litehtml bridge devolvio longitud invalida"));
    }

    text.truncate(out_len);
    let payload = core::str::from_utf8(text.as_slice())
        .map_err(|_| String::from("litehtml bridge devolvio texto no UTF-8"))?;
    Ok(BridgeTextResponse {
        payload: String::from(payload),
        truncated: rc > 0,
    })
}

#[cfg(feature = "litehtml_bridge")]
fn fetch_text_from_bridge<F: FnMut()>(
    url: &str,
    pump: &mut F,
) -> Result<BridgeTextResponse, String> {
    read_text_payload(pump, |out_ptr, out_cap, out_len| unsafe {
        litehtml_bridge_render_text(url.as_ptr(), url.len(), out_ptr, out_cap, out_len)
    })
}

#[cfg(feature = "litehtml_bridge")]
fn parse_litehtml_text_payload(
    request_url: &str,
    payload: &str,
) -> crate::web_engine::BrowserRenderOutput {
    let mut status = String::from("Done");
    let mut title: Option<String> = None;
    let mut final_url = String::from(request_url.trim());
    let mut lines: Vec<String> = Vec::new();
    let mut in_lines = false;

    for raw in payload.lines() {
        let line = raw.trim_end();
        if line == "---" {
            in_lines = true;
            continue;
        }
        if let Some(rest) = line.strip_prefix("STATUS:") {
            status = String::from(rest.trim());
            continue;
        }
        if let Some(rest) = line.strip_prefix("TITLE:") {
            let t = rest.trim();
            if !t.is_empty() {
                title = Some(String::from(t));
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("FINAL_URL:") {
            let u = rest.trim();
            if !u.is_empty() {
                final_url = String::from(u);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("LINE:") {
            if lines.len() < LITEHTML_BRIDGE_MAX_LINES {
                lines.push(String::from(rest.trim_start()));
            }
            continue;
        }
        if in_lines && !line.trim().is_empty() && lines.len() < LITEHTML_BRIDGE_MAX_LINES {
            lines.push(String::from(line.trim()));
        }
    }

    if lines.is_empty() {
        lines.push(String::from(
            "LiteHTML bridge: renderer sin lineas visuales.",
        ));
    }

    crate::web_engine::BrowserRenderOutput {
        final_url,
        status,
        title,
        lines,
        surface: None,
    }
}

#[cfg(feature = "litehtml_bridge")]
fn fetch_and_render_with_litehtml<F: FnMut()>(
    url: &str,
    pump: &mut F,
) -> crate::web_servo_bridge::ServoBridgeRender {
    let ready = unsafe { litehtml_bridge_is_ready() };
    if ready <= 0 {
        return fallback_fetch(
            url,
            pump,
            "LiteHTML bridge no listo; usando motor builtin.",
        );
    }

    let payload = match fetch_text_from_bridge(url, pump) {
        Ok(v) => v,
        Err(reason) => {
            return fallback_fetch(
                url,
                pump,
                format!("LiteHTML bridge fallo ({}). Fallback builtin.", reason).as_str(),
            );
        }
    };

    let mut output = parse_litehtml_text_payload(url, payload.payload.as_str());
    if payload.truncated {
        output
            .lines
            .push(String::from("[LiteHTML] payload truncado por limite de buffer."));
    }
    let surface = crate::web_servo_bridge::builtin_surface_from_output(&output);
    crate::web_servo_bridge::ServoBridgeRender {
        output: Some(output),
        note: Some(format!(
            "renderizado por LiteHTML bridge embebido (bridge={}).",
            binding_mode()
        )),
        surface,
    }
}
