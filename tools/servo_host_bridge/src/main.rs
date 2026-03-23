use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Response, Server, StatusCode};

#[derive(Debug, Clone)]
struct BridgeConfig {
    servo_bin: String,
    config_dir: String,
    frame_path: String,
    frame_width: u32,
    frame_height: u32,
    timeout_ms: u64,
}

#[derive(Debug, Clone, Default)]
struct SharedState {
    running: bool,
    bind_addr: String,
    servo_bin: String,
    config_dir: String,
    frame_path: String,
    window_size: String,
    current_url: String,
    title: String,
    history: Vec<String>,
    history_index: usize,
    last_error: Option<String>,
    last_input: Option<String>,
    last_frame: Option<Vec<u8>>,
    last_frame_width: u32,
    last_frame_height: u32,
    last_render_ms: u64,
}

#[derive(Serialize)]
struct StatusPayload {
    ok: bool,
    backend: &'static str,
    mode: &'static str,
    running: bool,
    bind_addr: String,
    servo_bin: String,
    config_dir: String,
    frame_path: String,
    window_size: String,
    current_url: String,
    title: String,
    history_len: usize,
    history_index: usize,
    frame_ready: bool,
    frame_width: u32,
    frame_height: u32,
    last_render_ms: u64,
    last_error: Option<String>,
    last_input: Option<String>,
    routes: Vec<&'static str>,
}

fn parse_arg(args: &[String], key: &str, default: &str) -> String {
    let mut idx = 0usize;
    while idx < args.len() {
        if args[idx] == key && idx + 1 < args.len() {
            return args[idx + 1].clone();
        }
        idx += 1;
    }
    String::from(default)
}

fn parse_u64_arg(args: &[String], key: &str, default: u64) -> u64 {
    parse_arg(args, key, "").trim().parse::<u64>().ok().unwrap_or(default)
}

fn parse_size_arg(args: &[String], key: &str, default_w: u32, default_h: u32) -> (u32, u32) {
    let raw = parse_arg(args, key, "");
    let value = raw.trim();
    if value.is_empty() {
        return (default_w, default_h);
    }

    let sep = if value.contains('x') {
        'x'
    } else if value.contains('X') {
        'X'
    } else if value.contains(',') {
        ','
    } else {
        return (default_w, default_h);
    };

    let mut parts = value.split(sep);
    let w = parts
        .next()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(default_w);
    let h = parts
        .next()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(default_h);
    if parts.next().is_some() {
        return (default_w, default_h);
    }
    if w == 0 || h == 0 {
        return (default_w, default_h);
    }
    (w, h)
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
        return String::from(trimmed);
    }
    format!("https://{}", trimmed)
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
        (String::from(path), map)
    } else {
        (String::from(url), BTreeMap::new())
    }
}

fn set_running(shared: &Arc<Mutex<SharedState>>, running: bool) {
    if let Ok(mut s) = shared.lock() {
        s.running = running;
    }
}

fn set_error(shared: &Arc<Mutex<SharedState>>, message: String) {
    if let Ok(mut s) = shared.lock() {
        s.last_error = Some(message);
    }
}

fn set_last_input(shared: &Arc<Mutex<SharedState>>, message: String) {
    if let Ok(mut s) = shared.lock() {
        s.last_input = Some(message);
    }
}

fn snapshot_status(shared: &Arc<Mutex<SharedState>>) -> StatusPayload {
    if let Ok(s) = shared.lock() {
        StatusPayload {
            ok: true,
            backend: "servo-host",
            mode: "headless-exit-image",
            running: s.running,
            bind_addr: s.bind_addr.clone(),
            servo_bin: s.servo_bin.clone(),
            config_dir: s.config_dir.clone(),
            frame_path: s.frame_path.clone(),
            window_size: s.window_size.clone(),
            current_url: s.current_url.clone(),
            title: s.title.clone(),
            history_len: s.history.len(),
            history_index: s.history_index,
            frame_ready: s.last_frame.is_some(),
            frame_width: s.last_frame_width,
            frame_height: s.last_frame_height,
            last_render_ms: s.last_render_ms,
            last_error: s.last_error.clone(),
            last_input: s.last_input.clone(),
            routes: vec!["/status", "/open?url=...", "/frame", "/input?type=..."],
        }
    } else {
        StatusPayload {
            ok: false,
            backend: "servo-host",
            mode: "headless-exit-image",
            running: false,
            bind_addr: String::new(),
            servo_bin: String::new(),
            config_dir: String::new(),
            frame_path: String::new(),
            window_size: String::new(),
            current_url: String::new(),
            title: String::new(),
            history_len: 0,
            history_index: 0,
            frame_ready: false,
            frame_width: 0,
            frame_height: 0,
            last_render_ms: 0,
            last_error: Some(String::from("state lock poisoned")),
            last_input: None,
            routes: vec!["/status", "/open?url=...", "/frame", "/input?type=..."],
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

fn choose_default_servo_bin() -> String {
    if let Ok(v) = std::env::var("SERVO_BIN") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return String::from(trimmed);
        }
    }

    let pinned = "/Users/mac/Desktop/servo/target/release/servo";
    if Path::new(pinned).exists() {
        return String::from(pinned);
    }

    String::from("servo")
}

fn ppm_next_token(bytes: &[u8], idx: &mut usize) -> Option<String> {
    while *idx < bytes.len() {
        let b = bytes[*idx];
        if b == b'#' {
            while *idx < bytes.len() && bytes[*idx] != b'\n' {
                *idx += 1;
            }
            continue;
        }
        if b.is_ascii_whitespace() {
            *idx += 1;
            continue;
        }
        break;
    }
    if *idx >= bytes.len() {
        return None;
    }

    let start = *idx;
    while *idx < bytes.len() {
        let b = bytes[*idx];
        if b == b'#' || b.is_ascii_whitespace() {
            break;
        }
        *idx += 1;
    }
    core::str::from_utf8(&bytes[start..*idx])
        .ok()
        .map(String::from)
}

fn parse_ppm_p6_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut idx = 0usize;
    let magic = ppm_next_token(bytes, &mut idx)?;
    if magic != "P6" {
        return None;
    }
    let width = ppm_next_token(bytes, &mut idx)?.parse::<u32>().ok()?;
    let height = ppm_next_token(bytes, &mut idx)?.parse::<u32>().ok()?;
    let max = ppm_next_token(bytes, &mut idx)?.parse::<u32>().ok()?;
    if width == 0 || height == 0 || max == 0 || max > 255 {
        return None;
    }
    Some((width, height))
}

fn render_with_servo(config: &BridgeConfig, url: &str) -> Result<Vec<u8>, String> {
    fs::create_dir_all(config.config_dir.as_str())
        .map_err(|e| format!("config-dir create failed: {}", e))?;

    if let Some(parent) = Path::new(config.frame_path.as_str()).parent() {
        fs::create_dir_all(parent).map_err(|e| format!("frame dir create failed: {}", e))?;
    }
    let _ = fs::remove_file(config.frame_path.as_str());

    let mut cmd = Command::new(config.servo_bin.as_str());
    cmd.arg("--headless")
        .arg("--exit")
        .arg("--config-dir")
        .arg(config.config_dir.as_str())
        .arg("--window-size")
        .arg(format!("{}x{}", config.frame_width, config.frame_height))
        .arg("--output")
        .arg(config.frame_path.as_str())
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("spawn servo failed ({}): {}", config.servo_bin, e))?;

    let deadline = Instant::now()
        .checked_add(Duration::from_millis(config.timeout_ms))
        .unwrap_or_else(Instant::now);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let mut stderr_text = String::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        let _ = stderr.read_to_string(&mut stderr_text);
                    }
                    let detail = stderr_text.trim();
                    if detail.is_empty() {
                        return Err(format!("servo exited with {}", status));
                    }
                    return Err(format!("servo exited with {}: {}", status, detail));
                }
                break;
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("servo render timeout ({} ms)", config.timeout_ms));
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(format!("servo wait failed: {}", e)),
        }
    }

    let bytes = fs::read(config.frame_path.as_str())
        .map_err(|e| format!("frame read failed ({}): {}", config.frame_path, e))?;
    if parse_ppm_p6_dimensions(bytes.as_slice()).is_none() {
        return Err(String::from("servo output is not valid PPM P6"));
    }
    Ok(bytes)
}

fn apply_render(
    shared: &Arc<Mutex<SharedState>>,
    config: &BridgeConfig,
    url: &str,
    push_history: bool,
    target_history_index: Option<usize>,
) -> Result<(u32, u32, u64), String> {
    let started = Instant::now();
    let frame = render_with_servo(config, url)?;
    let (w, h) = parse_ppm_p6_dimensions(frame.as_slice())
        .ok_or_else(|| String::from("cannot parse frame dimensions"))?;
    let elapsed = started.elapsed().as_millis() as u64;

    let mut s = shared
        .lock()
        .map_err(|_| String::from("state lock poisoned"))?;
    s.current_url = String::from(url);
    s.title = format!("Servo Host Bridge - {}", url);
    s.last_error = None;
    s.last_frame = Some(frame);
    s.last_frame_width = w;
    s.last_frame_height = h;
    s.last_render_ms = elapsed;

    if push_history {
        let truncate_from = s.history_index.saturating_add(1);
        if truncate_from < s.history.len() {
            s.history.truncate(truncate_from);
        }
        s.history.push(String::from(url));
        s.history_index = s.history.len().saturating_sub(1);
    } else if let Some(idx) = target_history_index {
        if idx < s.history.len() {
            s.history_index = idx;
        }
    }

    Ok((w, h, elapsed))
}

fn serve_http(bind_addr: String, shared: Arc<Mutex<SharedState>>, config: BridgeConfig) {
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
        let mut should_quit = false;

        let response = match (method, path.as_str()) {
            (Method::Get, "/status") => {
                let payload = snapshot_status(&shared);
                let body = serde_json::to_string_pretty(&payload)
                    .unwrap_or_else(|_| String::from("{\"ok\":false,\"error\":\"json encode\"}"));
                json_response(200, body.as_str())
            }
            (Method::Get, "/open") | (Method::Post, "/open") => {
                if let Some(url_raw) = query.get("url") {
                    let normalized = normalize_target_url(url_raw.as_str());
                    if normalized.is_empty() {
                        json_response(400, "{\"ok\":false,\"error\":\"missing url\"}")
                    } else {
                        match apply_render(&shared, &config, normalized.as_str(), true, None) {
                            Ok((w, h, took_ms)) => json_response(
                                200,
                                format!(
                                    "{{\"ok\":true,\"url\":{},\"frame\":\"{}x{}\",\"render_ms\":{}}}",
                                    serde_json::to_string(normalized.as_str())
                                        .unwrap_or_else(|_| String::from("\"\"")),
                                    w,
                                    h,
                                    took_ms
                                )
                                .as_str(),
                            ),
                            Err(err) => {
                                set_error(&shared, err.clone());
                                json_response(
                                    500,
                                    format!("{{\"ok\":false,\"error\":{}}}", serde_json::to_string(err.as_str()).unwrap_or_else(|_| String::from("\"render failed\""))).as_str(),
                                )
                            }
                        }
                    }
                } else {
                    json_response(400, "{\"ok\":false,\"error\":\"use /open?url=...\"}")
                }
            }
            (Method::Get, "/frame") | (Method::Post, "/frame") => {
                let frame = shared
                    .lock()
                    .ok()
                    .and_then(|s| s.last_frame.as_ref().cloned());
                if let Some(bytes) = frame {
                    bytes_response(200, "image/x-portable-pixmap", bytes)
                } else {
                    json_response(503, "{\"ok\":false,\"error\":\"frame not ready\"}")
                }
            }
            (Method::Get, "/input") | (Method::Post, "/input") => {
                let input_type = query
                    .get("type")
                    .map(|s| s.trim().to_ascii_lowercase())
                    .unwrap_or_default();
                if input_type.is_empty() {
                    json_response(400, "{\"ok\":false,\"error\":\"missing type\"}")
                } else {
                    set_last_input(&shared, input_type.clone());
                    if input_type == "back" || input_type == "forward" || input_type == "reload" {
                        let nav = {
                            let lock = shared.lock();
                            if let Ok(s) = lock {
                                if input_type == "reload" {
                                    if s.current_url.trim().is_empty() {
                                        Err(String::from("no current url"))
                                    } else {
                                        Ok((s.history_index, s.current_url.clone()))
                                    }
                                } else if input_type == "back" {
                                    if s.history_index == 0 || s.history.is_empty() {
                                        Err(String::from("no back history"))
                                    } else {
                                        let target_idx = s.history_index - 1;
                                        Ok((target_idx, s.history[target_idx].clone()))
                                    }
                                } else if s.history_index + 1 >= s.history.len() {
                                    Err(String::from("no forward history"))
                                } else {
                                    let target_idx = s.history_index + 1;
                                    Ok((target_idx, s.history[target_idx].clone()))
                                }
                            } else {
                                Err(String::from("state lock poisoned"))
                            }
                        };

                        match nav {
                            Ok((target_idx, target_url)) => {
                                match apply_render(
                                    &shared,
                                    &config,
                                    target_url.as_str(),
                                    false,
                                    Some(target_idx),
                                ) {
                                    Ok((w, h, took_ms)) => json_response(
                                        200,
                                        format!(
                                            "{{\"ok\":true,\"type\":\"{}\",\"url\":{},\"frame\":\"{}x{}\",\"render_ms\":{}}}",
                                            input_type,
                                            serde_json::to_string(target_url.as_str())
                                                .unwrap_or_else(|_| String::from("\"\"")),
                                            w,
                                            h,
                                            took_ms
                                        )
                                        .as_str(),
                                    ),
                                    Err(err) => {
                                        set_error(&shared, err.clone());
                                        json_response(
                                            500,
                                            format!(
                                                "{{\"ok\":false,\"type\":\"{}\",\"error\":{}}}",
                                                input_type,
                                                serde_json::to_string(err.as_str())
                                                    .unwrap_or_else(|_| String::from("\"render failed\""))
                                            )
                                            .as_str(),
                                        )
                                    }
                                }
                            }
                            Err(err) => json_response(
                                409,
                                format!(
                                    "{{\"ok\":false,\"type\":\"{}\",\"error\":{}}}",
                                    input_type,
                                    serde_json::to_string(err.as_str())
                                        .unwrap_or_else(|_| String::from("\"nav failed\""))
                                )
                                .as_str(),
                            ),
                        }
                    } else if input_type == "click"
                        || input_type == "scroll"
                        || input_type == "key"
                        || input_type == "text"
                    {
                        json_response(
                            200,
                            format!(
                                "{{\"ok\":true,\"type\":\"{}\",\"note\":\"no-op in screenshot mode\"}}",
                                input_type
                            )
                            .as_str(),
                        )
                    } else {
                        json_response(
                            400,
                            "{\"ok\":false,\"error\":\"unsupported input type\",\"usage\":\"/input?type=back|forward|reload|click|scroll|key|text\"}",
                        )
                    }
                }
            }
            (Method::Get, "/quit") | (Method::Post, "/quit") => {
                set_running(&shared, false);
                should_quit = true;
                json_response(200, "{\"ok\":true,\"queued\":\"quit\"}")
            }
            _ => text_response(
                404,
                "servo_host_bridge routes: /status, /open?url=..., /frame, /input?type=..., /quit",
            ),
        };

        let _ = request.respond(response);
        if should_quit {
            break;
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bind_addr = parse_arg(&args, "--bind", "127.0.0.1:37810");
    let start_url = parse_arg(&args, "--url", "https://example.com");
    let servo_bin_arg = parse_arg(&args, "--servo-bin", "");
    let servo_bin = if servo_bin_arg.trim().is_empty() {
        choose_default_servo_bin()
    } else {
        servo_bin_arg
    };
    let config_dir = parse_arg(&args, "--config-dir", "/tmp/servo_host_bridge_config");
    let frame_path = parse_arg(&args, "--frame", "/tmp/servo_host_bridge_frame.ppm");
    let timeout_ms = parse_u64_arg(&args, "--timeout-ms", 25000);
    let (frame_width, frame_height) = parse_size_arg(&args, "--size", 1280, 720);

    let config = BridgeConfig {
        servo_bin: servo_bin.clone(),
        config_dir: config_dir.clone(),
        frame_path: frame_path.clone(),
        frame_width,
        frame_height,
        timeout_ms,
    };

    let shared = Arc::new(Mutex::new(SharedState {
        running: true,
        bind_addr: bind_addr.clone(),
        servo_bin,
        config_dir,
        frame_path,
        window_size: format!("{}x{}", frame_width, frame_height),
        current_url: String::from(start_url.as_str()),
        title: String::from("Servo Host Bridge"),
        history: Vec::new(),
        history_index: 0,
        last_error: None,
        last_input: None,
        last_frame: None,
        last_frame_width: 0,
        last_frame_height: 0,
        last_render_ms: 0,
    }));

    let initial = normalize_target_url(start_url.as_str());
    if !initial.is_empty() {
        if let Err(err) = apply_render(&shared, &config, initial.as_str(), true, None) {
            set_error(&shared, format!("initial render failed: {}", err));
        }
    }

    serve_http(bind_addr, shared, config);
}
