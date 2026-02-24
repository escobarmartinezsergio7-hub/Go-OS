use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use tiny_http::{Header, Method, Response, Server, StatusCode};
use wry::{http::Request, WebView, WebViewBuilder};

#[cfg(target_os = "macos")]
use objc::{sel, sel_impl};

#[derive(Clone)]
enum UserEvent {
    OpenUrl(String),
    EvalScript(String),
    CaptureFrame(Sender<Result<FrameSnapshot, String>>),
    Quit,
}

#[derive(Clone)]
struct FrameSnapshot {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
struct SharedState {
    running: bool,
    bind_addr: String,
    current_url: String,
    title: String,
    last_error: Option<String>,
    last_ipc: Option<String>,
}

#[derive(Serialize)]
struct StatusPayload {
    ok: bool,
    backend: &'static str,
    mode: &'static str,
    running: bool,
    bind_addr: String,
    current_url: String,
    title: String,
    last_error: Option<String>,
    last_ipc: Option<String>,
}

fn parse_arg(args: &[String], key: &str, default: &str) -> String {
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == key {
            if i + 1 < args.len() {
                return args[i + 1].clone();
            }
        }
        i += 1;
    }
    default.to_string()
}

fn normalize_target_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.contains("://")
        || trimmed.starts_with("about:")
        || trimmed.starts_with("data:")
        || trimmed.starts_with("file:")
    {
        return trimmed.to_string();
    }
    format!("https://{}", trimmed)
}

fn set_error(shared: &Arc<Mutex<SharedState>>, msg: String) {
    if let Ok(mut s) = shared.lock() {
        s.last_error = Some(msg);
    }
}

fn set_running(shared: &Arc<Mutex<SharedState>>, running: bool) {
    if let Ok(mut s) = shared.lock() {
        s.running = running;
    }
}

fn set_current_url(shared: &Arc<Mutex<SharedState>>, url: String) {
    if let Ok(mut s) = shared.lock() {
        s.current_url = url;
    }
}

fn set_title(shared: &Arc<Mutex<SharedState>>, title: String) {
    if let Ok(mut s) = shared.lock() {
        s.title = title;
    }
}

fn set_last_ipc(shared: &Arc<Mutex<SharedState>>, msg: String) {
    if let Ok(mut s) = shared.lock() {
        s.last_ipc = Some(msg);
    }
}

fn snapshot_status(shared: &Arc<Mutex<SharedState>>) -> StatusPayload {
    if let Ok(s) = shared.lock() {
        StatusPayload {
            ok: true,
            backend: "webkit",
            mode: "host-eventloop-wry",
            running: s.running,
            bind_addr: s.bind_addr.clone(),
            current_url: s.current_url.clone(),
            title: s.title.clone(),
            last_error: s.last_error.clone(),
            last_ipc: s.last_ipc.clone(),
        }
    } else {
        StatusPayload {
            ok: false,
            backend: "webkit",
            mode: "host-eventloop-wry",
            running: false,
            bind_addr: String::new(),
            current_url: String::new(),
            title: String::new(),
            last_error: Some(String::from("state lock poisoned")),
            last_ipc: None,
        }
    }
}

fn json_response(status: u16, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut resp = Response::from_string(body.to_string()).with_status_code(StatusCode(status));
    if let Ok(header) = Header::from_bytes("Content-Type", "application/json; charset=utf-8") {
        resp.add_header(header);
    }
    resp
}

fn text_response(status: u16, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut resp = Response::from_string(body.to_string()).with_status_code(StatusCode(status));
    if let Ok(header) = Header::from_bytes("Content-Type", "text/plain; charset=utf-8") {
        resp.add_header(header);
    }
    resp
}

fn decode_url_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h1 = bytes[i + 1];
                let h2 = bytes[i + 2];
                let v1 = (h1 as char).to_digit(16);
                let v2 = (h2 as char).to_digit(16);
                if let (Some(a), Some(b)) = (v1, v2) {
                    out.push((a * 16 + b) as u8 as char);
                    i += 3;
                } else {
                    out.push('%');
                    i += 1;
                }
            }
            b => {
                out.push(b as char);
                i += 1;
            }
        }
    }
    out
}

fn parse_query_map(url: &str) -> (String, BTreeMap<String, String>) {
    if let Some((path, query)) = url.split_once('?') {
        let mut map = BTreeMap::new();
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            if let Some((k, v)) = pair.split_once('=') {
                map.insert(decode_url_component(k), decode_url_component(v));
            } else {
                map.insert(decode_url_component(pair), String::new());
            }
        }
        (path.to_string(), map)
    } else {
        (url.to_string(), BTreeMap::new())
    }
}

fn json_string_literal(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| String::from("\"\""))
}

fn parse_i32_param(query: &BTreeMap<String, String>, key: &str, default: i32) -> i32 {
    query
        .get(key)
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(default)
}

fn build_input_script(query: &BTreeMap<String, String>) -> Result<String, String> {
    let input_type = query
        .get("type")
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_default();

    if input_type.is_empty() {
        return Err(String::from("missing type"));
    }

    match input_type.as_str() {
        "back" => Ok(String::from("history.back();")),
        "forward" => Ok(String::from("history.forward();")),
        "reload" => Ok(String::from("location.reload();")),
        "scroll" => {
            let delta = parse_i32_param(query, "delta", 120);
            Ok(format!("window.scrollBy(0, {});", delta))
        }
        "click" => {
            let x = parse_i32_param(query, "x", 0);
            let y = parse_i32_param(query, "y", 0);
            Ok(format!(
                "(function(){{const el=document.elementFromPoint({},{}) ;if(el){{el.click();return true;}}return false;}})();",
                x, y
            ))
        }
        "key" => {
            let key = query.get("key").cloned().unwrap_or_else(|| String::from("Enter"));
            let key_js = json_string_literal(key.as_str());
            Ok(format!(
                "(function(){{const key={};const target=document.activeElement||document.body;['keydown','keypress','keyup'].forEach((name)=>target.dispatchEvent(new KeyboardEvent(name,{{key:key,bubbles:true,cancelable:true}})));return true;}})();",
                key_js
            ))
        }
        "text" => {
            let text = query.get("text").cloned().unwrap_or_default();
            if text.is_empty() {
                return Err(String::from("missing text"));
            }
            let text_js = json_string_literal(text.as_str());
            Ok(format!(
                "(function(){{const text={};const el=document.activeElement||document.body;if(!el){{return false;}}if('value' in el){{const value=String(el.value||'');const start=(typeof el.selectionStart==='number')?el.selectionStart:value.length;const end=(typeof el.selectionEnd==='number')?el.selectionEnd:value.length;el.value=value.slice(0,start)+text+value.slice(end);const caret=start+text.length;if(typeof el.setSelectionRange==='function'){{el.setSelectionRange(caret,caret);}}el.dispatchEvent(new Event('input',{{bubbles:true}}));el.dispatchEvent(new Event('change',{{bubbles:true}}));return true;}}el.textContent=String(el.textContent||'')+text;return true;}})();",
                text_js
            ))
        }
        other => Err(format!("unsupported input type: {}", other)),
    }
}

fn send_proxy_event(proxy: &EventLoopProxy<UserEvent>, event: UserEvent) -> Result<(), String> {
    proxy.send_event(event).map_err(|e| e.to_string())
}

fn bytes_response(
    status: u16,
    content_type: &str,
    body: Vec<u8>,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut resp = Response::from_data(body).with_status_code(StatusCode(status));
    if let Ok(header) = Header::from_bytes("Content-Type", content_type.as_bytes()) {
        resp.add_header(header);
    }
    resp
}

fn encode_ppm_p6(frame: &FrameSnapshot) -> Vec<u8> {
    let mut out = format!("P6\n{} {}\n255\n", frame.width, frame.height).into_bytes();
    out.extend_from_slice(frame.rgb.as_slice());
    out
}

fn request_frame_snapshot(
    proxy: &EventLoopProxy<UserEvent>,
    timeout_ms: u64,
) -> Result<FrameSnapshot, String> {
    let (tx, rx) = mpsc::channel::<Result<FrameSnapshot, String>>();
    send_proxy_event(proxy, UserEvent::CaptureFrame(tx))?;
    match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(String::from("frame capture timeout")),
        Err(RecvTimeoutError::Disconnected) => Err(String::from("frame capture disconnected")),
    }
}

#[cfg(target_os = "macos")]
fn capture_webview_frame(webview: &WebView) -> Result<FrameSnapshot, String> {
    use wry::WebViewExtMacOS;

    const NS_BITMAP_FORMAT_ALPHA_FIRST: usize = 1 << 0;
    const NS_BITMAP_FORMAT_32BIT_LITTLE_ENDIAN: usize = 1 << 9;
    const NS_BITMAP_FORMAT_32BIT_BIG_ENDIAN: usize = 1 << 11;

    unsafe {
        let wk = webview.webview();
        if wk.is_null() {
            return Err(String::from("wkwebview handle is null"));
        }

        let _: () = objc::msg_send![wk, displayIfNeeded];
        let bounds: cocoa::foundation::NSRect = objc::msg_send![wk, bounds];
        let rep: cocoa::base::id = objc::msg_send![wk, bitmapImageRepForCachingDisplayInRect: bounds];
        if rep.is_null() {
            return Err(String::from("bitmapImageRepForCachingDisplayInRect failed"));
        }

        let _: () = objc::msg_send![wk, cacheDisplayInRect: bounds toBitmapImageRep: rep];

        let width: isize = objc::msg_send![rep, pixelsWide];
        let height: isize = objc::msg_send![rep, pixelsHigh];
        let bits_per_sample: isize = objc::msg_send![rep, bitsPerSample];
        let samples_per_pixel: isize = objc::msg_send![rep, samplesPerPixel];
        let bytes_per_row: isize = objc::msg_send![rep, bytesPerRow];
        let bitmap_format: usize = objc::msg_send![rep, bitmapFormat];
        let data_ptr: *const u8 = objc::msg_send![rep, bitmapData];

        if width <= 0 || height <= 0 {
            return Err(String::from("invalid snapshot size"));
        }
        if bits_per_sample != 8 {
            return Err(format!("unsupported bits_per_sample={}", bits_per_sample));
        }
        if samples_per_pixel < 3 {
            return Err(format!("unsupported samples_per_pixel={}", samples_per_pixel));
        }
        if bytes_per_row <= 0 || data_ptr.is_null() {
            return Err(String::from("bitmap buffer unavailable"));
        }

        let width_u = width as usize;
        let height_u = height as usize;
        let spp = samples_per_pixel as usize;
        let row_stride = bytes_per_row as usize;
        let total_len = row_stride
            .checked_mul(height_u)
            .ok_or_else(|| String::from("frame size overflow"))?;

        let src = std::slice::from_raw_parts(data_ptr, total_len);
        let mut rgb = Vec::with_capacity(width_u.saturating_mul(height_u).saturating_mul(3));

        let alpha_first = (bitmap_format & NS_BITMAP_FORMAT_ALPHA_FIRST) != 0;
        let little_32 = (bitmap_format & NS_BITMAP_FORMAT_32BIT_LITTLE_ENDIAN) != 0;
        let big_32 = (bitmap_format & NS_BITMAP_FORMAT_32BIT_BIG_ENDIAN) != 0;

        for y in 0..height_u {
            let row_start = y.saturating_mul(row_stride);
            for x in 0..width_u {
                let px = row_start + x.saturating_mul(spp);
                if px + spp > src.len() {
                    return Err(String::from("frame buffer truncated"));
                }

                let (r, g, b) = if spp == 3 {
                    (src[px], src[px + 1], src[px + 2])
                } else if spp >= 4 {
                    if alpha_first {
                        if little_32 {
                            // Little-endian ARGB word -> BGRA bytes.
                            (src[px + 2], src[px + 1], src[px])
                        } else {
                            // Big-endian ARGB word -> ARGB bytes.
                            (src[px + 1], src[px + 2], src[px + 3])
                        }
                    } else if little_32 {
                        // Little-endian RGBA word -> ABGR bytes.
                        (src[px + 3], src[px + 2], src[px + 1])
                    } else if big_32 {
                        // Big-endian RGBA word -> RGBA bytes.
                        (src[px], src[px + 1], src[px + 2])
                    } else {
                        // Common fallback (RGBA).
                        (src[px], src[px + 1], src[px + 2])
                    }
                } else {
                    return Err(format!("unsupported channel count={}", spp));
                };

                rgb.push(r);
                rgb.push(g);
                rgb.push(b);
            }
        }

        Ok(FrameSnapshot {
            width: width_u as u32,
            height: height_u as u32,
            rgb,
        })
    }
}

#[cfg(not(target_os = "macos"))]
fn capture_webview_frame(_webview: &WebView) -> Result<FrameSnapshot, String> {
    Err(String::from("/frame snapshot only supported on macOS WKWebView"))
}

fn serve_http(bind_addr: String, shared: Arc<Mutex<SharedState>>, proxy: EventLoopProxy<UserEvent>) {
    let server = match Server::http(bind_addr.clone()) {
        Ok(s) => s,
        Err(e) => {
            set_error(
                &shared,
                format!("http bind failed on {}: {}", bind_addr.as_str(), e),
            );
            return;
        }
    };

    for request in server.incoming_requests() {
        let method = request.method().clone();
        let (path, query) = parse_query_map(request.url());

        let response = match (method, path.as_str()) {
            (Method::Get, "/status") => {
                let payload = snapshot_status(&shared);
                let body = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| {
                    String::from("{\"ok\":false,\"error\":\"json encode failed\"}")
                });
                json_response(200, body.as_str())
            }
            (Method::Get, "/open") | (Method::Post, "/open") => {
                if let Some(url) = query.get("url") {
                    let normalized = normalize_target_url(url);
                    if normalized.is_empty() {
                        json_response(400, "{\"ok\":false,\"error\":\"missing url\"}")
                    } else {
                        match send_proxy_event(&proxy, UserEvent::OpenUrl(normalized)) {
                            Ok(()) => json_response(200, "{\"ok\":true,\"queued\":\"open\"}"),
                            Err(msg) => json_response(
                                500,
                                format!(
                                    "{{\"ok\":false,\"error\":\"event loop not reachable: {}\"}}",
                                    msg
                                )
                                .as_str(),
                            ),
                        }
                    }
                } else {
                    json_response(400, "{\"ok\":false,\"error\":\"use /open?url=...\"}")
                }
            }
            (Method::Get, "/eval") | (Method::Post, "/eval") => {
                if let Some(js) = query.get("js") {
                    match send_proxy_event(&proxy, UserEvent::EvalScript(js.clone())) {
                        Ok(()) => json_response(200, "{\"ok\":true,\"queued\":\"eval\"}"),
                        Err(msg) => json_response(
                            500,
                            format!(
                                "{{\"ok\":false,\"error\":\"event loop not reachable: {}\"}}",
                                msg
                            )
                            .as_str(),
                        ),
                    }
                } else {
                    json_response(400, "{\"ok\":false,\"error\":\"use /eval?js=...\"}")
                }
            }
            (Method::Get, "/input") | (Method::Post, "/input") => {
                match build_input_script(&query) {
                    Ok(script) => match send_proxy_event(&proxy, UserEvent::EvalScript(script)) {
                        Ok(()) => json_response(200, "{\"ok\":true,\"queued\":\"input\"}"),
                        Err(msg) => json_response(
                            500,
                            format!("{{\"ok\":false,\"error\":\"{}\"}}", msg).as_str(),
                        ),
                    },
                    Err(msg) => json_response(
                        400,
                        format!(
                            "{{\"ok\":false,\"error\":\"{}\",\"usage\":\"/input?type=back|forward|reload|scroll&delta=120|click&x=10&y=10|key&key=Enter|text&text=hola\"}}",
                            msg
                        )
                        .as_str(),
                    ),
                }
            }
            (Method::Get, "/frame") | (Method::Post, "/frame") => {
                match request_frame_snapshot(&proxy, 3500) {
                    Ok(frame) => bytes_response(200, "image/x-portable-pixmap", encode_ppm_p6(&frame)),
                    Err(err) => json_response(
                        503,
                        format!("{{\"ok\":false,\"error\":\"{}\"}}", err).as_str(),
                    ),
                }
            }
            (Method::Get, "/quit") | (Method::Post, "/quit") => {
                let _ = proxy.send_event(UserEvent::Quit);
                json_response(200, "{\"ok\":true,\"queued\":\"quit\"}")
            }
            _ => text_response(
                404,
                "wry_host_bridge routes: /status, /open?url=..., /eval?js=..., /input?type=..., /frame, /quit",
            ),
        };

        let _ = request.respond(response);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bind_addr = parse_arg(&args, "--bind", "127.0.0.1:37810");
    let start_url = parse_arg(&args, "--url", "https://example.com");

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("ReduxOS Wry Host Bridge")
        .build(&event_loop)
        .expect("failed to create Wry window");

    let shared = Arc::new(Mutex::new(SharedState {
        running: true,
        bind_addr: bind_addr.clone(),
        current_url: start_url.clone(),
        title: String::from("ReduxOS Wry Host Bridge"),
        last_error: None,
        last_ipc: None,
    }));

    let shared_for_ipc = shared.clone();
    let shared_for_title = shared.clone();

    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    ))]
    let builder = WebViewBuilder::new(&window);

    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    )))]
    let builder = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window
            .default_vbox()
            .expect("linux gtk container not available");
        WebViewBuilder::new_gtk(vbox)
    };

    let webview = builder
        .with_url(start_url.as_str())
        .with_initialization_script(
            r#"
                window.__reduxWryBridge = {
                  ping: () => window.ipc.postMessage("ping")
                };
                window.addEventListener("load", () => {
                  window.ipc.postMessage("loaded:" + window.location.href);
                });
            "#,
        )
        .with_ipc_handler(move |req: Request<String>| {
            set_last_ipc(&shared_for_ipc, req.body().clone());
        })
        .with_document_title_changed_handler(move |title| {
            set_title(&shared_for_title, title);
        })
        .build()
        .expect("failed to build webview");

    let shared_http = shared.clone();
    thread::spawn(move || serve_http(bind_addr, shared_http, proxy));

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(_) => {}
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                set_running(&shared, false);
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(true),
                ..
            } => {
                let _ = webview.evaluate_script("document.title = document.title;");
            }
            Event::UserEvent(UserEvent::OpenUrl(url)) => {
                match webview.load_url(url.as_str()) {
                    Ok(()) => {
                        set_current_url(&shared, url.clone());
                        set_title(&shared, format!("ReduxOS Wry Host Bridge - {}", url));
                    }
                    Err(e) => set_error(&shared, format!("load_url failed: {}", e)),
                }
            }
            Event::UserEvent(UserEvent::EvalScript(js)) => {
                if let Err(e) = webview.evaluate_script(js.as_str()) {
                    set_error(&shared, format!("evaluate_script failed: {}", e));
                }
            }
            Event::UserEvent(UserEvent::CaptureFrame(reply_tx)) => {
                let result = capture_webview_frame(&webview);
                if let Err(err) = result.as_ref() {
                    set_error(&shared, format!("capture frame failed: {}", err));
                }
                let _ = reply_tx.send(result);
            }
            Event::UserEvent(UserEvent::Quit) => {
                set_running(&shared, false);
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}
