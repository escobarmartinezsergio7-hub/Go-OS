#![cfg(feature = "vaev_bridge")]

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::min;
use core::ptr;
use core::slice;
use core::str;

struct ShimState {
    history: Vec<String>,
    history_index: usize,
    last_url: String,
    last_payload: String,
    last_frame_width: u32,
    last_frame_height: u32,
    last_frame_rgba: Vec<u8>,
}

impl ShimState {
    fn new() -> Self {
        Self {
            history: Vec::new(),
            history_index: 0,
            last_url: String::new(),
            last_payload: String::from(
                "STATUS: Ready\nFINAL_URL: about:blank\n---\nLINE: Vaev shim listo. Abre una URL primero.\n",
            ),
            last_frame_width: 0,
            last_frame_height: 0,
            last_frame_rgba: Vec::new(),
        }
    }
}

static mut VAEV_SHIM_STATE: Option<ShimState> = None;

fn state_mut() -> &'static mut ShimState {
    unsafe {
        if VAEV_SHIM_STATE.is_none() {
            VAEV_SHIM_STATE = Some(ShimState::new());
        }
        VAEV_SHIM_STATE.as_mut().unwrap()
    }
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

fn build_vaev_payload(
    request_url: &str,
    page: Option<&crate::web_engine::BrowserRenderOutput>,
    note: Option<&str>,
) -> String {
    let mut out = String::new();
    match page {
        Some(rendered) => {
            out.push_str("STATUS: ");
            out.push_str(sanitize_inline(rendered.status.as_str()).as_str());
            out.push('\n');

            if let Some(title) = rendered.title.as_ref() {
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
            out.push_str("---\n");
            out.push_str("LINE: Vaev shim: renderer sin contenido.\n");
        }
    }

    if let Some(note) = note {
        if !note.trim().is_empty() {
            out.push_str("LINE: ");
            out.push_str(sanitize_inline(note).as_str());
            out.push('\n');
        }
    }

    out
}

fn render_page(url: &str) -> Option<crate::web_engine::BrowserRenderOutput> {
    let mut pump = || {};
    crate::web_engine::fetch_and_render(url, &mut pump)
}

fn surface_to_rgba_bytes(surface: &crate::web_servo_bridge::ServoBridgeSurface) -> Vec<u8> {
    let mut out = Vec::new();
    out.resize(surface.pixels.len().saturating_mul(4), 0);
    let mut idx = 0usize;
    for pixel in surface.pixels.iter() {
        let r = ((pixel >> 16) & 0xFF) as u8;
        let g = ((pixel >> 8) & 0xFF) as u8;
        let b = (pixel & 0xFF) as u8;
        out[idx] = r;
        out[idx + 1] = g;
        out[idx + 2] = b;
        out[idx + 3] = 0xFF;
        idx += 4;
    }
    out
}

fn frame_from_output(output: &crate::web_engine::BrowserRenderOutput) -> Option<(u32, u32, Vec<u8>)> {
    let surface = crate::web_servo_bridge::builtin_surface_from_output(output)?;
    let rgba = surface_to_rgba_bytes(&surface);
    Some((surface.width, surface.height, rgba))
}

fn push_history_entry(state: &mut ShimState, url: &str) {
    if url.trim().is_empty() {
        return;
    }

    if !state.history.is_empty() {
        let keep_len = state
            .history_index
            .saturating_add(1)
            .min(state.history.len());
        if keep_len < state.history.len() {
            state.history.truncate(keep_len);
        }

        if state
            .history
            .last()
            .map(|last| last.as_str() == url)
            .unwrap_or(false)
        {
            state.history_index = state.history.len().saturating_sub(1);
            return;
        }
    }

    state.history.push(String::from(url));
    state.history_index = state.history.len().saturating_sub(1);
}

fn current_url(state: &ShimState) -> Option<String> {
    if !state.history.is_empty() {
        let idx = state.history_index.min(state.history.len().saturating_sub(1));
        return state.history.get(idx).cloned();
    }
    if !state.last_url.trim().is_empty() {
        return Some(state.last_url.clone());
    }
    None
}

fn apply_render_result(
    state: &mut ShimState,
    request_url: &str,
    rendered: Option<crate::web_engine::BrowserRenderOutput>,
    push_history: bool,
    note: Option<&str>,
) {
    let final_url = rendered
        .as_ref()
        .map(|page| page.final_url.clone())
        .unwrap_or_else(|| String::from(request_url));

    state.last_payload = build_vaev_payload(request_url, rendered.as_ref(), note);
    state.last_url = final_url.clone();

    if let Some(page) = rendered.as_ref() {
        if let Some((width, height, rgba)) = frame_from_output(page) {
            state.last_frame_width = width;
            state.last_frame_height = height;
            state.last_frame_rgba = rgba;
        } else {
            state.last_frame_width = 0;
            state.last_frame_height = 0;
            state.last_frame_rgba.clear();
        }
    } else {
        state.last_frame_width = 0;
        state.last_frame_height = 0;
        state.last_frame_rgba.clear();
    }

    if push_history {
        push_history_entry(state, final_url.as_str());
    }
}

fn copy_text_to_out(payload: &str, out_ptr: *mut u8, out_cap: usize, out_len: *mut usize) -> i32 {
    let payload_bytes = payload.as_bytes();
    let copy_len = min(payload_bytes.len(), out_cap.saturating_sub(1));

    unsafe {
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

fn ascii_lower(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        out.push(b.to_ascii_lowercase() as char);
    }
    out
}

fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn url_decode_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'+' {
            out.push(' ');
            i += 1;
            continue;
        }
        if b == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push(((hi << 4) | lo) as char);
                i += 3;
                continue;
            }
        }
        out.push(b as char);
        i += 1;
    }
    out
}

enum ShimInputEvent {
    Click { x: i32, y: i32 },
    Scroll { delta: i32 },
    Key { key: String },
    Text { text: String },
    Back,
    Forward,
    Reload,
    Unknown,
}

fn parse_input_event(raw_query: &str) -> ShimInputEvent {
    let query = raw_query
        .trim()
        .trim_start_matches("input?")
        .trim_start_matches('?');

    let mut event_type = String::new();
    let mut x_val: Option<i32> = None;
    let mut y_val: Option<i32> = None;
    let mut delta_val: Option<i32> = None;
    let mut key_val: Option<String> = None;
    let mut text_val: Option<String> = None;

    for chunk in query.split('&') {
        if chunk.is_empty() {
            continue;
        }
        let mut kv = chunk.splitn(2, '=');
        let key = ascii_lower(kv.next().unwrap_or(""));
        let value = url_decode_component(kv.next().unwrap_or(""));

        if key == "type" {
            event_type = ascii_lower(value.as_str());
        } else if key == "x" {
            x_val = value.trim().parse::<i32>().ok();
        } else if key == "y" {
            y_val = value.trim().parse::<i32>().ok();
        } else if key == "delta" {
            delta_val = value.trim().parse::<i32>().ok();
        } else if key == "key" {
            key_val = Some(value);
        } else if key == "text" {
            text_val = Some(value);
        }
    }

    match event_type.as_str() {
        "click" => match (x_val, y_val) {
            (Some(x), Some(y)) => ShimInputEvent::Click { x, y },
            _ => ShimInputEvent::Unknown,
        },
        "scroll" => ShimInputEvent::Scroll {
            delta: delta_val.unwrap_or(120),
        },
        "key" => ShimInputEvent::Key {
            key: key_val.unwrap_or_else(|| String::from("Enter")),
        },
        "text" => ShimInputEvent::Text {
            text: text_val.unwrap_or_default(),
        },
        "back" => ShimInputEvent::Back,
        "forward" => ShimInputEvent::Forward,
        "reload" => ShimInputEvent::Reload,
        _ => ShimInputEvent::Unknown,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn vaev_bridge_is_ready() -> i32 {
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn vaev_bridge_render_text(
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
        let bytes = slice::from_raw_parts(url_ptr, url_len);
        match str::from_utf8(bytes) {
            Ok(v) => v.trim(),
            Err(_) => return -2,
        }
    };

    if req_url.is_empty() {
        return -3;
    }

    let rendered = render_page(req_url);
    let state = state_mut();
    apply_render_result(state, req_url, rendered, true, None);
    copy_text_to_out(state.last_payload.as_str(), out_ptr, out_cap, out_len)
}

#[unsafe(no_mangle)]
pub extern "C" fn vaev_bridge_input(
    input_ptr: *const u8,
    input_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    if input_ptr.is_null() || out_ptr.is_null() || out_len.is_null() || out_cap == 0 {
        return -1;
    }

    let raw_input = unsafe {
        let bytes = slice::from_raw_parts(input_ptr, input_len);
        match str::from_utf8(bytes) {
            Ok(v) => v.trim(),
            Err(_) => return -2,
        }
    };

    let event = parse_input_event(raw_input);
    let state = state_mut();
    let current = current_url(state);

    let (target_url, note) = match event {
        ShimInputEvent::Back => {
            if !state.history.is_empty() && state.history_index > 0 {
                state.history_index -= 1;
                (
                    state.history.get(state.history_index).cloned(),
                    String::from("Vaev shim: back."),
                )
            } else {
                (current.clone(), String::from("Vaev shim: no hay historial atras."))
            }
        }
        ShimInputEvent::Forward => {
            if !state.history.is_empty() && state.history_index + 1 < state.history.len() {
                state.history_index += 1;
                (
                    state.history.get(state.history_index).cloned(),
                    String::from("Vaev shim: forward."),
                )
            } else {
                (
                    current.clone(),
                    String::from("Vaev shim: no hay historial adelante."),
                )
            }
        }
        ShimInputEvent::Reload => (current.clone(), String::from("Vaev shim: reload.")),
        ShimInputEvent::Click { x, y } => (
            current.clone(),
            format!("Vaev shim: click ({}, {}) recibido.", x, y),
        ),
        ShimInputEvent::Scroll { delta } => (
            current.clone(),
            format!("Vaev shim: scroll {} recibido.", delta),
        ),
        ShimInputEvent::Key { key } => (
            current.clone(),
            format!("Vaev shim: key '{}' recibido.", sanitize_inline(key.as_str())),
        ),
        ShimInputEvent::Text { text } => (
            current.clone(),
            format!("Vaev shim: text '{}' recibido.", sanitize_inline(text.as_str())),
        ),
        ShimInputEvent::Unknown => (
            current,
            String::from("Vaev shim: input no reconocido."),
        ),
    };

    if let Some(url) = target_url {
        let rendered = render_page(url.as_str());
        apply_render_result(state, url.as_str(), rendered, false, Some(note.as_str()));
    } else {
        apply_render_result(
            state,
            "about:blank",
            None,
            false,
            Some("Vaev shim: abre una URL primero."),
        );
    }

    copy_text_to_out(state.last_payload.as_str(), out_ptr, out_cap, out_len)
}

#[unsafe(no_mangle)]
pub extern "C" fn vaev_bridge_copy_last_frame_rgba(
    out_ptr: *mut u8,
    out_cap: usize,
    out_width: *mut u32,
    out_height: *mut u32,
    out_len: *mut usize,
) -> i32 {
    if out_ptr.is_null()
        || out_width.is_null()
        || out_height.is_null()
        || out_len.is_null()
        || out_cap == 0
    {
        return -1;
    }

    let state = state_mut();
    if state.last_frame_width == 0 || state.last_frame_height == 0 || state.last_frame_rgba.is_empty() {
        unsafe {
            ptr::write(out_width, 0);
            ptr::write(out_height, 0);
            ptr::write(out_len, 0);
        }
        return -2;
    }

    let bytes = state.last_frame_rgba.as_slice();
    let copy_len = min(bytes.len(), out_cap);

    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr, copy_len);
        ptr::write(out_width, state.last_frame_width);
        ptr::write(out_height, state.last_frame_height);
        ptr::write(out_len, copy_len);
    }

    if bytes.len() > copy_len {
        1
    } else {
        0
    }
}
