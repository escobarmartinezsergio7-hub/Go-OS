use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

const MAX_REDIRECTS: usize = 6;
const MAX_RENDER_LINES: usize = 480;
const MAX_LINE_WIDTH: usize = 108;
const NATIVE_SURFACE_W: u32 = 800;
const NATIVE_SURFACE_H: u32 = 420;
const NATIVE_MAX_TOKENS: usize = 4096;
const READER_PROXY_BASE: &str = "http://r.jina.ai/http://";
const READER_PROXY_HOST: &str = "r.jina.ai";
static NATIVE_RENDER_ENABLED: AtomicBool = AtomicBool::new(true);

#[derive(Clone, Copy, PartialEq, Eq)]
enum NativeTextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone)]
struct CssRule {
    selector: String,
    hide: bool,
    uppercase: bool,
    lowercase: bool,
    preserve_whitespace: bool,
    block: Option<bool>,
    text_color: Option<u32>,
    background_color: Option<u32>,
    bold: bool,
    indent_left_px: Option<u16>,
    text_align: Option<NativeTextAlign>,
}

#[derive(Clone)]
struct TagState {
    tag: String,
    id: Option<String>,
    class_attr: Option<String>,
    hide: bool,
    uppercase: bool,
    lowercase: bool,
    preserve_whitespace: bool,
    block: bool,
    link: Option<String>,
}

#[derive(Clone)]
struct JsResult {
    title_override: Option<String>,
    body_override: Option<String>,
    body_append: Vec<String>,
    logs: Vec<String>,
    unsupported_count: usize,
}

impl JsResult {
    fn new() -> Self {
        Self {
            title_override: None,
            body_override: None,
            body_append: Vec::new(),
            logs: Vec::new(),
            unsupported_count: 0,
        }
    }
}

struct ParsedHttp {
    status_line: Option<String>,
    status_code: Option<u16>,
    headers: Vec<(String, String)>,
    body: String,
}

pub struct BrowserRenderOutput {
    pub final_url: String,
    pub status: String,
    pub title: Option<String>,
    pub lines: Vec<String>,
    pub surface: Option<BrowserRenderSurface>,
}

#[derive(Clone)]
pub struct BrowserRenderSurface {
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

pub fn set_native_render_enabled(enabled: bool) {
    NATIVE_RENDER_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_native_render_enabled() -> bool {
    NATIVE_RENDER_ENABLED.load(Ordering::Relaxed)
}

fn to_ascii_sanitized(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii() {
            out.push(ch);
        } else {
            out.push(' ');
        }
    }
    out
}

fn ascii_lower_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        out.push(b.to_ascii_lowercase() as char);
    }
    out
}

fn find_subslice(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() || start >= haystack.len() || needle.len() > haystack.len() {
        return None;
    }
    let end = haystack.len().saturating_sub(needle.len());
    let mut i = start;
    while i <= end {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_byte(bytes: &[u8], needle: u8, start: usize) -> Option<usize> {
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn extract_tag_blocks(source_ascii: &str, tag: &str) -> (String, Vec<String>) {
    let bytes = source_ascii.as_bytes();
    let mut lower = Vec::with_capacity(bytes.len());
    for b in bytes {
        lower.push(b.to_ascii_lowercase());
    }
    let tag_lower = ascii_lower_str(tag).into_bytes();

    let mut cleaned = String::with_capacity(source_ascii.len());
    let mut blocks = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let open_match = if i + 1 + tag_lower.len() <= lower.len() && lower[i] == b'<' {
            let start = i + 1;
            let end = start + tag_lower.len();
            if &lower[start..end] == tag_lower.as_slice() {
                if end < lower.len() {
                    let next = lower[end];
                    next.is_ascii_whitespace() || next == b'>' || next == b'/'
                } else {
                    true
                }
            } else {
                false
            }
        } else {
            false
        };

        if open_match {
            let Some(open_end) = find_byte(bytes, b'>', i) else {
                break;
            };
            let content_start = open_end + 1;

            let mut close_pos = None;
            let mut j = content_start;
            while j + 2 + tag_lower.len() <= lower.len() {
                if lower[j] == b'<' && lower[j + 1] == b'/' {
                    let name_start = j + 2;
                    let name_end = name_start + tag_lower.len();
                    if &lower[name_start..name_end] == tag_lower.as_slice() {
                        let mut k = name_end;
                        while k < lower.len() && lower[k].is_ascii_whitespace() {
                            k += 1;
                        }
                        if k < lower.len() && lower[k] == b'>' {
                            close_pos = Some(j);
                            break;
                        }
                    }
                }
                j += 1;
            }

            if let Some(close_at) = close_pos {
                blocks.push(String::from(&source_ascii[content_start..close_at]));
                cleaned.push('\n');
                if let Some(close_end) = find_byte(bytes, b'>', close_at) {
                    i = close_end + 1;
                    continue;
                }
                break;
            }
        }

        cleaned.push(bytes[i] as char);
        i += 1;
    }

    while i < bytes.len() {
        cleaned.push(bytes[i] as char);
        i += 1;
    }

    (cleaned, blocks)
}

fn extract_first_tag_text(source_ascii: &str, tag: &str) -> Option<String> {
    let lower = ascii_lower_str(source_ascii);
    let open_pat = format!("<{}", tag);
    let close_pat = format!("</{}>", tag);

    let open_idx = lower.find(open_pat.as_str())?;
    let open_end_rel = lower[open_idx..].find('>')?;
    let content_start = open_idx + open_end_rel + 1;
    let close_rel = lower[content_start..].find(close_pat.as_str())?;
    let content_end = content_start + close_rel;
    Some(String::from(source_ascii[content_start..content_end].trim()))
}

fn parse_http_response(raw: &str) -> ParsedHttp {
    let ascii = to_ascii_sanitized(raw);
    if !ascii.starts_with("HTTP/") {
        return ParsedHttp {
            status_line: None,
            status_code: None,
            headers: Vec::new(),
            body: ascii,
        };
    }

    let (head, body) = if let Some(idx) = ascii.find("\r\n\r\n") {
        (&ascii[..idx], &ascii[idx + 4..])
    } else if let Some(idx) = ascii.find("\n\n") {
        (&ascii[..idx], &ascii[idx + 2..])
    } else {
        (ascii.as_str(), "")
    };

    let mut lines = head.lines();
    let status = lines.next().map(|s| String::from(s.trim()));
    let status_code = status
        .as_ref()
        .and_then(|s| {
            let mut parts = s.split_whitespace();
            let _http = parts.next()?;
            parts.next()?.parse::<u16>().ok()
        });

    let mut headers = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((ascii_lower_str(k.trim()), String::from(v.trim())));
        }
    }

    ParsedHttp {
        status_line: status,
        status_code,
        headers,
        body: String::from(body),
    }
}

fn header_value<'a>(parsed: &'a ParsedHttp, key: &str) -> Option<&'a str> {
    let key_lower = ascii_lower_str(key);
    for (k, v) in parsed.headers.iter() {
        if *k == key_lower {
            return Some(v.as_str());
        }
    }
    None
}

fn resolve_redirect_url(base_url: &str, location: &str) -> String {
    let loc = location.trim();
    let loc_lower = ascii_lower_str(loc);
    if loc_lower.starts_with("http://") || loc_lower.starts_with("https://") {
        return String::from(loc);
    }

    if loc.starts_with("//") {
        if let Some(scheme_pos) = base_url.find("://") {
            let scheme = &base_url[..scheme_pos];
            return format!("{}:{}", scheme, loc);
        }
        return format!("http:{}", loc);
    }

    let Some(scheme_pos) = base_url.find("://") else {
        return String::from(loc);
    };
    let scheme = &base_url[..scheme_pos];
    let rest = &base_url[scheme_pos + 3..];
    let (authority, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };

    if loc.starts_with('/') {
        return format!("{}://{}{}", scheme, authority, loc);
    }

    let base_dir = match path.rfind('/') {
        Some(idx) => &path[..idx + 1],
        None => "/",
    };
    format!("{}://{}{}{}", scheme, authority, base_dir, loc)
}

fn starts_with_ignore_ascii_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .map(|head| head.eq_ignore_ascii_case(prefix))
        .unwrap_or(false)
}

fn extract_url_host(url: &str) -> Option<&str> {
    let without_scheme = if starts_with_ignore_ascii_case(url, "http://") {
        &url[7..]
    } else if starts_with_ignore_ascii_case(url, "https://") {
        &url[8..]
    } else {
        url
    };

    let authority = match without_scheme.find('/') {
        Some(idx) => &without_scheme[..idx],
        None => without_scheme,
    };
    let host = match authority.find(':') {
        Some(idx) => &authority[..idx],
        None => authority,
    };

    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

fn is_reader_proxy_url(url: &str) -> bool {
    extract_url_host(url)
        .map(|host| host.eq_ignore_ascii_case(READER_PROXY_HOST))
        .unwrap_or(false)
}

fn build_reader_proxy_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if starts_with_ignore_ascii_case(trimmed, "http://") {
        return Some(format!("{}{}", READER_PROXY_BASE, &trimmed[7..]));
    }
    if starts_with_ignore_ascii_case(trimmed, "https://") {
        return Some(format!("{}{}", READER_PROXY_BASE, &trimmed[8..]));
    }
    None
}

fn should_try_reader_proxy(url: &str) -> bool {
    let trimmed = url.trim();
    if trimmed.is_empty() || is_reader_proxy_url(trimmed) {
        return false;
    }
    starts_with_ignore_ascii_case(trimmed, "http://")
        || starts_with_ignore_ascii_case(trimmed, "https://")
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn strip_angle_markup_fragments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'<' {
            if let Some(close_rel) = text[i + 1..].find('>') {
                let close = i + 1 + close_rel;
                if close > i + 1 {
                    let inside = text[i + 1..close].trim();
                    if !inside.is_empty() {
                        let first = inside.as_bytes()[0];
                        if first.is_ascii_alphabetic()
                            || first == b'/'
                            || first == b'!'
                            || first == b'?'
                        {
                            i = close + 1;
                            continue;
                        }
                    }
                }
            }
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    out
}

fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    for ch in text.chars() {
        if ch.is_ascii_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    String::from(out.trim())
}

fn parse_css_color_name(name: &str) -> Option<u32> {
    match name.trim() {
        "black" => Some(0x000000),
        "white" => Some(0xFFFFFF),
        "red" => Some(0xFF0000),
        "green" => Some(0x008000),
        "blue" => Some(0x0000FF),
        "yellow" => Some(0xFFFF00),
        "cyan" | "aqua" => Some(0x00FFFF),
        "magenta" | "fuchsia" => Some(0xFF00FF),
        "gray" | "grey" => Some(0x808080),
        "silver" => Some(0xC0C0C0),
        "maroon" => Some(0x800000),
        "navy" => Some(0x000080),
        "purple" => Some(0x800080),
        "teal" => Some(0x008080),
        "olive" => Some(0x808000),
        "orange" => Some(0xFFA500),
        "brown" => Some(0x8B4513),
        "transparent" => Some(0xF6F8FC),
        _ => None,
    }
}

fn parse_css_u8(token: &str) -> Option<u8> {
    let t = token.trim();
    if let Some(percent) = t.strip_suffix('%') {
        let p = percent.trim().parse::<u16>().ok()?.min(100);
        let v = ((p as u32).saturating_mul(255) / 100) as u8;
        return Some(v);
    }
    t.parse::<u16>().ok().map(|v| v.min(255) as u8)
}

fn parse_css_color_value(value: &str) -> Option<u32> {
    let v = ascii_lower_str(value.trim());
    let vv = v.as_str();

    if let Some(hex) = vv.strip_prefix('#') {
        if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            let rr = (r << 4) | r;
            let gg = (g << 4) | g;
            let bb = (b << 4) | b;
            return Some(((rr as u32) << 16) | ((gg as u32) << 8) | (bb as u32));
        }
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(((r as u32) << 16) | ((g as u32) << 8) | (b as u32));
        }
    }

    if let Some(inner) = vv.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let mut it = inner.split(',');
        let r = parse_css_u8(it.next()?)?;
        let g = parse_css_u8(it.next()?)?;
        let b = parse_css_u8(it.next()?)?;
        return Some(((r as u32) << 16) | ((g as u32) << 8) | (b as u32));
    }
    if let Some(inner) = vv.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
        let mut it = inner.split(',');
        let r = parse_css_u8(it.next()?)?;
        let g = parse_css_u8(it.next()?)?;
        let b = parse_css_u8(it.next()?)?;
        return Some(((r as u32) << 16) | ((g as u32) << 8) | (b as u32));
    }

    parse_css_color_name(vv)
}

fn parse_css_px_value(value: &str) -> Option<u16> {
    let vv = ascii_lower_str(value.trim());
    let vv = vv.trim();
    if vv.is_empty() {
        return None;
    }

    let num = if let Some(px) = vv.strip_suffix("px") {
        px.trim().parse::<u16>().ok()?
    } else {
        vv.parse::<u16>().ok()?
    };
    Some(num.min(640))
}

fn parse_css_first_box_px(value: &str) -> Option<u16> {
    let mut first = value.split_whitespace();
    parse_css_px_value(first.next()?.trim())
}

fn parse_css_text_align(value: &str) -> Option<NativeTextAlign> {
    match ascii_lower_str(value.trim()).as_str() {
        "center" => Some(NativeTextAlign::Center),
        "right" | "end" => Some(NativeTextAlign::Right),
        "left" | "start" => Some(NativeTextAlign::Left),
        _ => None,
    }
}

fn css_property_value(style: &str, name: &str) -> Option<String> {
    for decl in style.split(';') {
        let Some((k, v)) = decl.split_once(':') else {
            continue;
        };
        if ascii_lower_str(k.trim()) == name {
            let value = v.trim();
            if !value.is_empty() {
                return Some(String::from(value));
            }
        }
    }
    None
}

fn parse_css_rules(blocks: &[String]) -> Vec<CssRule> {
    let mut rules = Vec::new();
    for block in blocks {
        let lower = ascii_lower_str(block);
        let mut cursor = 0usize;
        while let Some(open) = lower[cursor..].find('{') {
            let open_idx = cursor + open;
            let Some(close_rel) = lower[open_idx + 1..].find('}') else {
                break;
            };
            let close_idx = open_idx + 1 + close_rel;
            let selectors = block[cursor..open_idx].split(',');
            let body = lower[open_idx + 1..close_idx].trim();

            let hide = body.contains("display:none") || body.contains("visibility:hidden");
            let uppercase = body.contains("text-transform:uppercase");
            let lowercase = body.contains("text-transform:lowercase");
            let preserve_whitespace = body.contains("white-space:pre");
            let text_color = css_property_value(body, "color")
                .and_then(|v| parse_css_color_value(v.as_str()));
            let background_color = css_property_value(body, "background-color")
                .or_else(|| css_property_value(body, "background"))
                .and_then(|v| parse_css_color_value(v.as_str()));
            let indent_left_px = css_property_value(body, "margin-left")
                .and_then(|v| parse_css_px_value(v.as_str()))
                .or_else(|| {
                    css_property_value(body, "padding-left")
                        .and_then(|v| parse_css_px_value(v.as_str()))
                })
                .or_else(|| {
                    css_property_value(body, "margin")
                        .and_then(|v| parse_css_first_box_px(v.as_str()))
                })
                .or_else(|| {
                    css_property_value(body, "padding")
                        .and_then(|v| parse_css_first_box_px(v.as_str()))
                });
            let text_align = css_property_value(body, "text-align")
                .and_then(|v| parse_css_text_align(v.as_str()));
            let bold = css_property_value(body, "font-weight")
                .map(|v| {
                    let vv = ascii_lower_str(v.as_str());
                    vv.contains("bold")
                        || vv.trim().parse::<u16>().map(|n| n >= 600).unwrap_or(false)
                })
                .unwrap_or(false);
            let block_mode = if body.contains("display:block")
                || body.contains("display:list-item")
                || body.contains("display:flex")
            {
                Some(true)
            } else if body.contains("display:inline") || body.contains("display:contents") {
                Some(false)
            } else {
                None
            };

            if hide
                || uppercase
                || lowercase
                || preserve_whitespace
                || block_mode.is_some()
                || text_color.is_some()
                || background_color.is_some()
                || bold
                || indent_left_px.is_some()
                || text_align.is_some()
            {
                for selector in selectors {
                    let sel = selector.trim();
                    if !sel.is_empty() {
                        rules.push(CssRule {
                            selector: String::from(sel),
                            hide,
                            uppercase,
                            lowercase,
                            preserve_whitespace,
                            block: block_mode,
                            text_color,
                            background_color,
                            bold,
                            indent_left_px,
                            text_align,
                        });
                    }
                }
            }

            cursor = close_idx + 1;
        }
    }
    rules
}

fn attr_value(tag_content: &str, attr_name: &str) -> Option<String> {
    let lower = ascii_lower_str(tag_content);
    let pattern = format!("{}=", ascii_lower_str(attr_name));
    let pos = lower.find(pattern.as_str())?;
    let mut i = pos + pattern.len();
    let bytes = tag_content.as_bytes();
    if i >= bytes.len() {
        return None;
    }

    if bytes[i] == b'"' || bytes[i] == b'\'' {
        let quote = bytes[i];
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i] != quote {
            i += 1;
        }
        return Some(String::from(&tag_content[start..i]));
    }

    let start = i;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
        i += 1;
    }
    Some(String::from(&tag_content[start..i]))
}

fn parse_inline_style_full(style: &str) -> (bool, bool, bool, bool, Option<bool>) {
    let lower = ascii_lower_str(style);
    let hide = lower.contains("display:none") || lower.contains("visibility:hidden");
    let uppercase = lower.contains("text-transform:uppercase");
    let lowercase = lower.contains("text-transform:lowercase");
    let preserve_whitespace = lower.contains("white-space:pre");
    let block_mode = if lower.contains("display:block")
        || lower.contains("display:list-item")
        || lower.contains("display:flex")
    {
        Some(true)
    } else if lower.contains("display:inline") || lower.contains("display:contents") {
        Some(false)
    } else {
        None
    };
    (hide, uppercase, lowercase, preserve_whitespace, block_mode)
}

fn parse_inline_visual_style(style: &str) -> (Option<u32>, Option<u32>, bool) {
    let lower = ascii_lower_str(style);
    let text_color = css_property_value(lower.as_str(), "color")
        .and_then(|v| parse_css_color_value(v.as_str()));
    let background_color = css_property_value(lower.as_str(), "background-color")
        .or_else(|| css_property_value(lower.as_str(), "background"))
        .and_then(|v| parse_css_color_value(v.as_str()));
    let bold = css_property_value(lower.as_str(), "font-weight")
        .map(|v| {
            let vv = ascii_lower_str(v.as_str());
            vv.contains("bold")
                || vv.trim().parse::<u16>().map(|n| n >= 600).unwrap_or(false)
        })
        .unwrap_or(false);
    (text_color, background_color, bold)
}

fn parse_inline_layout_style(style: &str) -> (Option<u16>, Option<NativeTextAlign>) {
    let lower = ascii_lower_str(style);
    let indent_left_px = css_property_value(lower.as_str(), "margin-left")
        .and_then(|v| parse_css_px_value(v.as_str()))
        .or_else(|| {
            css_property_value(lower.as_str(), "padding-left")
                .and_then(|v| parse_css_px_value(v.as_str()))
        })
        .or_else(|| {
            css_property_value(lower.as_str(), "margin")
                .and_then(|v| parse_css_first_box_px(v.as_str()))
        })
        .or_else(|| {
            css_property_value(lower.as_str(), "padding")
                .and_then(|v| parse_css_first_box_px(v.as_str()))
        });
    let text_align = css_property_value(lower.as_str(), "text-align")
        .and_then(|v| parse_css_text_align(v.as_str()));
    (indent_left_px, text_align)
}

fn class_list_has(classes: Option<&str>, class_name: &str) -> bool {
    let Some(classes) = classes else {
        return false;
    };
    let target = ascii_lower_str(class_name);
    for c in classes.split_whitespace() {
        if ascii_lower_str(c) == target {
            return true;
        }
    }
    false
}

fn css_simple_selector_matches(simple_selector: &str, tag: &str, id: Option<&str>, class_attr: Option<&str>) -> bool {
    let sel = simple_selector.trim();
    if sel.is_empty() {
        return false;
    }

    if sel == "*" {
        return true;
    }

    let mut tag_part = "";
    let mut id_part: Option<&str> = None;
    let mut class_part: Option<&str> = None;

    if let Some(hash) = sel.find('#') {
        tag_part = &sel[..hash];
        let after = &sel[hash + 1..];
        if let Some(dot) = after.find('.') {
            id_part = Some(&after[..dot]);
            class_part = Some(&after[dot + 1..]);
        } else {
            id_part = Some(after);
        }
    } else if let Some(dot) = sel.find('.') {
        tag_part = &sel[..dot];
        class_part = Some(&sel[dot + 1..]);
    } else if sel.starts_with('.') {
        class_part = Some(&sel[1..]);
    } else if sel.starts_with('#') {
        id_part = Some(&sel[1..]);
    } else {
        tag_part = sel;
    }

    if !tag_part.is_empty() && tag_part != "*" && ascii_lower_str(tag_part) != ascii_lower_str(tag) {
        return false;
    }

    if let Some(id_sel) = id_part {
        let Some(id_val) = id else {
            return false;
        };
        if ascii_lower_str(id_sel) != ascii_lower_str(id_val) {
            return false;
        }
    }

    if let Some(class_sel) = class_part {
        if !class_list_has(class_attr, class_sel) {
            return false;
        }
    }

    true
}

fn css_rule_matches(
    rule: &CssRule,
    tag: &str,
    id: Option<&str>,
    class_attr: Option<&str>,
    stack: &[TagState],
) -> bool {
    let selector = rule.selector.trim();
    if selector.is_empty() {
        return false;
    }

    // Phase-2: support descendant and direct-child (>) selector chains
    // using the current open-tag stack as ancestor context.
    let normalized = selector.replace('>', " > ");
    let mut tokens: Vec<&str> = Vec::new();
    for tok in normalized.split_whitespace() {
        if !tok.is_empty() {
            tokens.push(tok);
        }
    }
    if tokens.is_empty() {
        return false;
    }

    let mut cur_idx = tokens.len();
    while cur_idx > 0 && tokens[cur_idx - 1] == ">" {
        cur_idx -= 1;
    }
    if cur_idx == 0 {
        return false;
    }

    if !css_simple_selector_matches(tokens[cur_idx - 1], tag, id, class_attr) {
        return false;
    }

    let mut ancestor_limit = stack.len();
    let mut require_direct_parent = false;
    let mut i = cur_idx.saturating_sub(1);
    while i > 0 {
        i -= 1;
        let tok = tokens[i];
        if tok == ">" {
            require_direct_parent = true;
            continue;
        }

        if require_direct_parent {
            if ancestor_limit == 0 {
                return false;
            }
            let parent_idx = ancestor_limit - 1;
            let parent = &stack[parent_idx];
            if !css_simple_selector_matches(
                tok,
                parent.tag.as_str(),
                parent.id.as_deref(),
                parent.class_attr.as_deref(),
            ) {
                return false;
            }
            ancestor_limit = parent_idx;
            require_direct_parent = false;
        } else {
            let mut found = None;
            let mut search = ancestor_limit;
            while search > 0 {
                search -= 1;
                let ancestor = &stack[search];
                if css_simple_selector_matches(
                    tok,
                    ancestor.tag.as_str(),
                    ancestor.id.as_deref(),
                    ancestor.class_attr.as_deref(),
                ) {
                    found = Some(search);
                    break;
                }
            }
            let Some(found_idx) = found else {
                return false;
            };
            ancestor_limit = found_idx;
        }
    }

    !require_direct_parent
}

fn active_hidden(stack: &[TagState]) -> bool {
    for state in stack.iter().rev() {
        if state.hide {
            return true;
        }
    }
    false
}

fn active_uppercase(stack: &[TagState]) -> bool {
    for state in stack.iter().rev() {
        if state.uppercase {
            return true;
        }
    }
    false
}

fn active_lowercase(stack: &[TagState]) -> bool {
    for state in stack.iter().rev() {
        if state.lowercase {
            return true;
        }
    }
    false
}

fn active_preserve_whitespace(stack: &[TagState]) -> bool {
    for state in stack.iter().rev() {
        if state.preserve_whitespace {
            return true;
        }
    }
    false
}

fn active_link(stack: &[TagState]) -> Option<&str> {
    for state in stack.iter().rev() {
        if let Some(link) = state.link.as_ref() {
            return Some(link.as_str());
        }
    }
    None
}

fn push_line(lines: &mut Vec<String>, line: &str) {
    if lines.len() >= MAX_RENDER_LINES {
        return;
    }
    if line.trim().is_empty() {
        if lines.last().map(|s| s.trim().is_empty()).unwrap_or(false) {
            return;
        }
        lines.push(String::new());
        return;
    }
    lines.push(String::from(line.trim()));
}

fn push_line_raw(lines: &mut Vec<String>, line: &str) {
    if lines.len() >= MAX_RENDER_LINES {
        return;
    }
    if line.is_empty() {
        if lines.last().map(|s| s.is_empty()).unwrap_or(false) {
            return;
        }
        lines.push(String::new());
        return;
    }
    lines.push(String::from(line));
}

fn looks_like_noise_line(line: &str) -> bool {
    let text = line.trim();
    if text.is_empty() || text.starts_with('[') {
        return false;
    }

    let len = text.len();
    if len < 24 {
        return false;
    }

    let mut alpha = 0usize;
    let mut spaces = 0usize;
    let mut punct = 0usize;
    let mut angle = 0usize;

    for b in text.bytes() {
        if b.is_ascii_alphabetic() {
            alpha += 1;
        } else if b.is_ascii_whitespace() {
            spaces += 1;
        } else {
            punct += 1;
            if b == b'<' || b == b'>' {
                angle += 1;
            }
        }
    }

    if len > 96 && spaces <= 1 {
        return true;
    }
    if angle >= 4 && alpha * 100 / len < 55 {
        return true;
    }
    if len > 80 && punct * 100 / len > 42 && alpha * 100 / len < 45 {
        return true;
    }

    let lower = ascii_lower_str(text);
    if lower.contains("function(")
        || lower.contains("function ")
        || lower.contains("document.")
        || lower.contains("window.")
        || lower.contains("google.")
        || lower.contains(".innerhtml")
        || lower.contains("google.k")
        || lower.contains("spdx-license-identifier")
        || lower.contains("closure library authors")
        || lower.contains("\\x")
        || lower.contains("\\u00")
        || lower.contains("||")
        || lower.contains("&&")
        || lower.starts_with("var ")
        || lower.starts_with("let ")
        || lower.starts_with("const ")
        || (lower.contains("function") && lower.contains("return"))
        || (lower.contains("var ") && lower.contains('=') && lower.contains(';'))
        || text.matches(';').count() >= 3
    {
        return true;
    }

    false
}

fn looks_like_js_payload(text: &str) -> bool {
    let lower = ascii_lower_str(text.trim());
    if lower.is_empty() {
        return false;
    }

    lower.contains("<script")
        || lower.contains("</script")
        || lower.contains("<style")
        || lower.contains("function(")
        || lower.contains("function ")
        || lower.contains("window.")
        || lower.contains("document.")
        || lower.contains("google.")
        || lower.contains("\\x")
        || lower.contains("\\u00")
        || lower.starts_with("var ")
        || lower.starts_with("let ")
        || lower.starts_with("const ")
        || (lower.contains('=') && (lower.contains("||") || lower.contains("&&")))
}

fn sanitize_render_lines(lines: &mut Vec<String>) {
    let mut out = Vec::with_capacity(lines.len());
    for line in lines.iter() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !out.last().map(|s: &String| s.is_empty()).unwrap_or(false) {
                out.push(String::new());
            }
            continue;
        }

        if looks_like_noise_line(trimmed) {
            continue;
        }

        out.push(String::from(trimmed));
        if out.len() >= MAX_RENDER_LINES {
            break;
        }
    }

    if out.is_empty() {
        out.push(String::from("(Documento sin contenido visible)"));
    }

    *lines = out;
}

fn strip_non_render_blocks(source: &str) -> String {
    let mut cur = String::from(source);
    for _ in 0..2 {
        let (next, _) = extract_tag_blocks(cur.as_str(), "script");
        cur = next;
        let (next, _) = extract_tag_blocks(cur.as_str(), "style");
        cur = next;
        let (next, _) = extract_tag_blocks(cur.as_str(), "noscript");
        cur = next;
        let (next, _) = extract_tag_blocks(cur.as_str(), "template");
        cur = next;
    }
    cur
}

fn flush_current_line(lines: &mut Vec<String>, current_line: &mut String, preserve_whitespace: bool) {
    if current_line.is_empty() {
        return;
    }
    if preserve_whitespace {
        push_line_raw(lines, current_line.as_str());
    } else {
        push_line(lines, current_line.as_str());
    }
    current_line.clear();
}

fn apply_case_transform(text: &str, uppercase: bool, lowercase: bool) -> String {
    if lowercase {
        return ascii_lower_str(text);
    }
    if uppercase {
        return ascii_lower_str(text).to_ascii_uppercase();
    }
    String::from(text)
}

fn append_wrapped_word(lines: &mut Vec<String>, current_line: &mut String, word: &str) {
    if word.is_empty() {
        return;
    }
    if current_line.is_empty() {
        current_line.push_str(word);
        return;
    }
    if current_line.len() + 1 + word.len() > MAX_LINE_WIDTH {
        push_line(lines, current_line.as_str());
        current_line.clear();
        current_line.push_str(word);
        return;
    }
    current_line.push(' ');
    current_line.push_str(word);
}

fn append_visible_text(
    raw_text: &str,
    stack: &[TagState],
    lines: &mut Vec<String>,
    current_line: &mut String,
) {
    if raw_text.is_empty() || active_hidden(stack) {
        return;
    }

    let preserve_whitespace = active_preserve_whitespace(stack);
    let mut text = decode_entities(raw_text);
    if !preserve_whitespace {
        text = strip_angle_markup_fragments(text.as_str());
    }
    text = apply_case_transform(
        text.as_str(),
        active_uppercase(stack),
        active_lowercase(stack),
    );

    if let Some(link) = active_link(stack) {
        if preserve_whitespace {
            if !text.is_empty() {
                text.push(' ');
                text.push('<');
                text.push_str(link);
                text.push('>');
            }
        } else {
            let mut collapsed = collapse_spaces(text.as_str());
            if collapsed.is_empty() {
                return;
            }
            collapsed.push_str(" <");
            collapsed.push_str(link);
            collapsed.push('>');
            for word in collapsed.split_whitespace() {
                append_wrapped_word(lines, current_line, word);
            }
            return;
        }
    }

    if preserve_whitespace {
        for ch in text.chars() {
            match ch {
                '\r' => {}
                '\n' => {
                    flush_current_line(lines, current_line, true);
                }
                '\t' => {
                    current_line.push_str("    ");
                    if current_line.len() >= MAX_LINE_WIDTH {
                        flush_current_line(lines, current_line, true);
                    }
                }
                _ => {
                    current_line.push(ch);
                    if current_line.len() >= MAX_LINE_WIDTH {
                        flush_current_line(lines, current_line, true);
                    }
                }
            }
        }
        return;
    }

    let collapsed = collapse_spaces(text.as_str());
    if collapsed.is_empty() {
        return;
    }
    for word in collapsed.split_whitespace() {
        append_wrapped_word(lines, current_line, word);
    }
}

fn extract_first_string_literal(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let q = bytes[i];
        if q == b'"' || q == b'\'' {
            let start = i + 1;
            i = start;
            while i < bytes.len() {
                if bytes[i] == q && bytes[i.saturating_sub(1)] != b'\\' {
                    return Some(String::from(&text[start..i]));
                }
                i += 1;
            }
            return None;
        }
        i += 1;
    }
    None
}

fn extract_call_string_arg(text: &str) -> Option<String> {
    let open = text.find('(')?;
    let close = find_matching_delim(text, open, b'(', b')')?;
    if close <= open {
        return None;
    }
    extract_first_string_literal(text[open + 1..close].trim())
}

fn escape_html_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn find_matching_delim(text: &str, open_idx: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = text.as_bytes();
    if open_idx >= bytes.len() || bytes[open_idx] != open {
        return None;
    }

    let mut depth = 0usize;
    let mut in_quote = 0u8;
    let mut i = open_idx;
    while i < bytes.len() {
        let b = bytes[i];

        if in_quote != 0 {
            if b == in_quote && bytes[i.saturating_sub(1)] != b'\\' {
                in_quote = 0;
            }
            i += 1;
            continue;
        }

        if b == b'"' || b == b'\'' || b == b'`' {
            in_quote = b;
            i += 1;
            continue;
        }

        if b == open {
            depth += 1;
        } else if b == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(i);
            }
        }

        i += 1;
    }
    None
}

fn split_js_statements(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut in_quote = 0u8;
    let mut paren = 0usize;
    let mut brace = 0usize;
    let mut bracket = 0usize;

    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];

        if in_quote != 0 {
            if b == in_quote && bytes[i.saturating_sub(1)] != b'\\' {
                in_quote = 0;
            }
            i += 1;
            continue;
        }

        match b {
            b'"' | b'\'' | b'`' => in_quote = b,
            b'(' => paren += 1,
            b')' => paren = paren.saturating_sub(1),
            b'{' => brace += 1,
            b'}' => brace = brace.saturating_sub(1),
            b'[' => bracket += 1,
            b']' => bracket = bracket.saturating_sub(1),
            b';' if paren == 0 && brace == 0 && bracket == 0 => {
                let piece = source[start..i].trim();
                if !piece.is_empty() {
                    out.push(String::from(piece));
                }
                start = i + 1;
            }
            _ => {}
        }

        i += 1;
    }

    if start < source.len() {
        let tail = source[start..].trim();
        if !tail.is_empty() {
            out.push(String::from(tail));
        }
    }

    out
}

fn extract_js_function_body(statement: &str) -> Option<String> {
    let open = statement.find('{')?;
    let close = find_matching_delim(statement, open, b'{', b'}')?;
    if close <= open {
        return None;
    }
    Some(String::from(statement[open + 1..close].trim()))
}

fn extract_assignment_string(statement: &str) -> Option<(bool, String)> {
    if let Some(idx) = statement.find("+=") {
        let value = extract_first_string_literal(statement[idx + 2..].trim())?;
        return Some((true, value));
    }
    let eq = statement.find('=')?;
    let value = extract_first_string_literal(statement[eq + 1..].trim())?;
    Some((false, value))
}

fn extract_call_string_arg_and_rest(statement: &str, prefix: &str) -> Option<(String, String)> {
    let mut idx = prefix.len();
    while idx < statement.len() && statement.as_bytes()[idx].is_ascii_whitespace() {
        idx += 1;
    }
    if idx >= statement.len() || statement.as_bytes()[idx] != b'(' {
        return None;
    }

    let close = find_matching_delim(statement, idx, b'(', b')')?;
    if close <= idx {
        return None;
    }
    let arg = extract_first_string_literal(statement[idx + 1..close].trim())?;
    let rest = statement[close + 1..].trim();
    Some((arg, String::from(rest)))
}

#[derive(Clone)]
struct DomOpenElem {
    tag: String,
    id: Option<String>,
    class_attr: Option<String>,
    open_start: usize,
    open_end: usize,
}

fn selector_matches_dom_stack(
    selector: &str,
    tag: &str,
    id: Option<&str>,
    class_attr: Option<&str>,
    stack: &[DomOpenElem],
) -> bool {
    let normalized = selector.trim().replace('>', " > ");
    let mut tokens: Vec<&str> = Vec::new();
    for tok in normalized.split_whitespace() {
        if !tok.is_empty() {
            tokens.push(tok);
        }
    }
    if tokens.is_empty() {
        return false;
    }

    let mut cur_idx = tokens.len();
    while cur_idx > 0 && tokens[cur_idx - 1] == ">" {
        cur_idx -= 1;
    }
    if cur_idx == 0 {
        return false;
    }

    if !css_simple_selector_matches(tokens[cur_idx - 1], tag, id, class_attr) {
        return false;
    }

    let mut ancestor_limit = stack.len();
    let mut require_direct_parent = false;
    let mut i = cur_idx.saturating_sub(1);
    while i > 0 {
        i -= 1;
        let tok = tokens[i];
        if tok == ">" {
            require_direct_parent = true;
            continue;
        }

        if require_direct_parent {
            if ancestor_limit == 0 {
                return false;
            }
            let parent_idx = ancestor_limit - 1;
            let parent = &stack[parent_idx];
            if !css_simple_selector_matches(
                tok,
                parent.tag.as_str(),
                parent.id.as_deref(),
                parent.class_attr.as_deref(),
            ) {
                return false;
            }
            ancestor_limit = parent_idx;
            require_direct_parent = false;
        } else {
            let mut found = None;
            let mut search = ancestor_limit;
            while search > 0 {
                search -= 1;
                let ancestor = &stack[search];
                if css_simple_selector_matches(
                    tok,
                    ancestor.tag.as_str(),
                    ancestor.id.as_deref(),
                    ancestor.class_attr.as_deref(),
                ) {
                    found = Some(search);
                    break;
                }
            }
            let Some(found_idx) = found else {
                return false;
            };
            ancestor_limit = found_idx;
        }
    }

    !require_direct_parent
}

fn first_matching_element_bounds(
    source: &str,
    selector: &str,
) -> Option<(usize, usize, usize, usize)> {
    let selector = selector.trim();
    if selector.is_empty() {
        return None;
    }

    let bytes = source.as_bytes();
    let mut stack: Vec<DomOpenElem> = Vec::new();

    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }

        let Some(tag_end) = find_byte(bytes, b'>', i) else {
            break;
        };
        let tag_inner = source[i + 1..tag_end].trim();
        if tag_inner.is_empty() {
            i = tag_end + 1;
            continue;
        }

        let tag_inner_lower = ascii_lower_str(tag_inner);
        if tag_inner_lower.starts_with("!--") {
            i = tag_end + 1;
            continue;
        }

        let is_close = tag_inner.starts_with('/');
        let tag_text = if is_close {
            tag_inner[1..].trim()
        } else {
            tag_inner
        };

        let mut parts = tag_text.splitn(2, char::is_whitespace);
        let tag_name = ascii_lower_str(parts.next().unwrap_or(""));
        let attrs = parts.next().unwrap_or("").trim();
        if tag_name.is_empty() {
            i = tag_end + 1;
            continue;
        }

        let self_closing = tag_inner.ends_with('/')
            || matches!(tag_name.as_str(), "br" | "img" | "input" | "meta" | "link" | "hr");

        if is_close {
            if let Some(pos) = stack.iter().rposition(|s| s.tag == tag_name) {
                let open = stack.remove(pos);
                if selector_matches_dom_stack(
                    selector,
                    open.tag.as_str(),
                    open.id.as_deref(),
                    open.class_attr.as_deref(),
                    &stack,
                ) {
                    return Some((open.open_start, open.open_end, open.open_end + 1, i));
                }
            }
        } else {
            let id = attr_value(attrs, "id");
            let class_attr = attr_value(attrs, "class");

            if self_closing {
                if selector_matches_dom_stack(
                    selector,
                    tag_name.as_str(),
                    id.as_deref(),
                    class_attr.as_deref(),
                    &stack,
                ) {
                    return Some((i, tag_end, tag_end + 1, tag_end + 1));
                }
            } else {
                stack.push(DomOpenElem {
                    tag: tag_name,
                    id,
                    class_attr,
                    open_start: i,
                    open_end: tag_end,
                });
            }
        }

        i = tag_end + 1;
    }

    None
}

fn replace_element_inner_html(source: &mut String, selector: &str, new_html: &str) -> bool {
    let Some((_open_start, _open_end, inner_start, inner_end)) =
        first_matching_element_bounds(source.as_str(), selector)
    else {
        return false;
    };
    source.replace_range(inner_start..inner_end, new_html);
    true
}

fn append_element_inner_html(source: &mut String, selector: &str, new_html: &str) -> bool {
    let Some((_open_start, _open_end, inner_start, inner_end)) =
        first_matching_element_bounds(source.as_str(), selector)
    else {
        return false;
    };

    let mut combined = String::from(&source[inner_start..inner_end]);
    combined.push_str(new_html);
    source.replace_range(inner_start..inner_end, combined.as_str());
    true
}

fn set_element_text_content(source: &mut String, selector: &str, text: &str, append: bool) -> bool {
    let escaped = escape_html_text(text);
    if append {
        append_element_inner_html(source, selector, escaped.as_str())
    } else {
        replace_element_inner_html(source, selector, escaped.as_str())
    }
}

fn extract_selector_attr(source: &str, selector: &str, attr: &str) -> Option<String> {
    let (open_start, open_end, _inner_start, _inner_end) =
        first_matching_element_bounds(source, selector)?;
    if open_end <= open_start + 1 {
        return None;
    }

    let tag_inner = source[open_start + 1..open_end].trim();
    let mut parts = tag_inner.splitn(2, char::is_whitespace);
    let _tag = parts.next()?;
    let attrs = parts.next().unwrap_or("").trim();
    attr_value(attrs, attr)
}

fn run_js_source(source: &str, dom_source: &mut String, result: &mut JsResult, depth: usize) {
    if depth > 6 {
        result.unsupported_count += 1;
        return;
    }

    let statements = split_js_statements(source);
    for stmt in statements {
        let statement = stmt.trim();
        if statement.is_empty() {
            continue;
        }

        let lower = ascii_lower_str(statement);

        if lower.starts_with("window.addeventlistener(")
            || lower.starts_with("document.addeventlistener(")
        {
            let event = extract_first_string_literal(statement)
                .map(|s| ascii_lower_str(s.as_str()))
                .unwrap_or(String::new());
            if matches!(event.as_str(), "load" | "domcontentloaded" | "click") {
                if let Some(body) = extract_js_function_body(statement) {
                    run_js_source(body.as_str(), dom_source, result, depth + 1);
                } else {
                    result.unsupported_count += 1;
                }
            }
            continue;
        }

        if lower.starts_with("window.onload") || lower.starts_with("document.onload") {
            if let Some(body) = extract_js_function_body(statement) {
                run_js_source(body.as_str(), dom_source, result, depth + 1);
            } else {
                result.unsupported_count += 1;
            }
            continue;
        }

        if lower.starts_with("document.title") && lower.contains('=') {
            if let Some(text) = extract_first_string_literal(statement) {
                result.title_override = Some(text.clone());
                let escaped = escape_html_text(text.as_str());
                let _ = replace_element_inner_html(dom_source, "title", escaped.as_str());
            } else {
                result.unsupported_count += 1;
            }
            continue;
        }

        if lower.starts_with("document.body.innerhtml") && lower.contains('=') {
            if let Some((append, text)) = extract_assignment_string(statement) {
                if append {
                    let _ = append_element_inner_html(dom_source, "body", text.as_str());
                } else {
                    let _ = replace_element_inner_html(dom_source, "body", text.as_str());
                }
            } else {
                result.unsupported_count += 1;
            }
            continue;
        }

        if (lower.starts_with("document.body.innertext")
            || lower.starts_with("document.body.textcontent"))
            && lower.contains('=')
        {
            if let Some((append, text)) = extract_assignment_string(statement) {
                if !set_element_text_content(dom_source, "body", text.as_str(), append) {
                    result.body_override = Some(text);
                }
            } else {
                result.unsupported_count += 1;
            }
            continue;
        }

        if lower.starts_with("document.write") {
            if let Some(text) = extract_call_string_arg(statement) {
                if text.len() > 2048 || looks_like_js_payload(text.as_str()) {
                    result.unsupported_count += 1;
                    continue;
                }

                if !append_element_inner_html(dom_source, "body", text.as_str()) {
                    result.body_append.push(text);
                }
            } else {
                result.unsupported_count += 1;
            }
            continue;
        }

        if lower.starts_with("console.log") {
            if let Some(text) = extract_call_string_arg(statement) {
                result.logs.push(format!("[console] {}", text));
            } else {
                result.unsupported_count += 1;
            }
            continue;
        }

        let mut handled_selector = false;

        if lower.starts_with("document.getelementbyid") {
            if let Some((id, rest)) =
                extract_call_string_arg_and_rest(statement, "document.getElementById")
            {
                let selector = format!("#{}", id);
                let rest_lower = ascii_lower_str(rest.as_str());

                if rest_lower.starts_with(".innerhtml") && rest_lower.contains('=') {
                    if let Some((append, text)) = extract_assignment_string(rest.as_str()) {
                        if append {
                            let _ = append_element_inner_html(dom_source, selector.as_str(), text.as_str());
                        } else {
                            let _ = replace_element_inner_html(dom_source, selector.as_str(), text.as_str());
                        }
                        handled_selector = true;
                    }
                } else if (rest_lower.starts_with(".innertext")
                    || rest_lower.starts_with(".textcontent"))
                    && rest_lower.contains('=')
                {
                    if let Some((append, text)) = extract_assignment_string(rest.as_str()) {
                        let _ = set_element_text_content(dom_source, selector.as_str(), text.as_str(), append);
                        handled_selector = true;
                    }
                } else if rest_lower.starts_with(".addeventlistener(") {
                    let event = extract_first_string_literal(rest.as_str())
                        .map(|s| ascii_lower_str(s.as_str()))
                        .unwrap_or(String::new());
                    if matches!(event.as_str(), "load" | "domcontentloaded" | "click") {
                        if let Some(body) = extract_js_function_body(rest.as_str()) {
                            run_js_source(body.as_str(), dom_source, result, depth + 1);
                            handled_selector = true;
                        }
                    }
                } else if rest_lower.starts_with(".onclick") {
                    if let Some(body) = extract_js_function_body(rest.as_str()) {
                        run_js_source(body.as_str(), dom_source, result, depth + 1);
                        handled_selector = true;
                    } else if let Some((_, code)) = extract_assignment_string(rest.as_str()) {
                        run_js_source(code.as_str(), dom_source, result, depth + 1);
                        handled_selector = true;
                    }
                } else if rest_lower.starts_with(".click()") {
                    if let Some(handler) = extract_selector_attr(dom_source.as_str(), selector.as_str(), "onclick") {
                        run_js_source(handler.as_str(), dom_source, result, depth + 1);
                        handled_selector = true;
                    }
                }
            }
        }

        if !handled_selector && lower.starts_with("document.queryselector") {
            if let Some((selector, rest)) =
                extract_call_string_arg_and_rest(statement, "document.querySelector")
            {
                let rest_lower = ascii_lower_str(rest.as_str());

                if rest_lower.starts_with(".innerhtml") && rest_lower.contains('=') {
                    if let Some((append, text)) = extract_assignment_string(rest.as_str()) {
                        if append {
                            let _ = append_element_inner_html(dom_source, selector.as_str(), text.as_str());
                        } else {
                            let _ = replace_element_inner_html(dom_source, selector.as_str(), text.as_str());
                        }
                        handled_selector = true;
                    }
                } else if (rest_lower.starts_with(".innertext")
                    || rest_lower.starts_with(".textcontent"))
                    && rest_lower.contains('=')
                {
                    if let Some((append, text)) = extract_assignment_string(rest.as_str()) {
                        let _ = set_element_text_content(dom_source, selector.as_str(), text.as_str(), append);
                        handled_selector = true;
                    }
                } else if rest_lower.starts_with(".style.display") && rest_lower.contains('=') {
                    if let Some((_, mode)) = extract_assignment_string(rest.as_str()) {
                        if ascii_lower_str(mode.as_str()) == "none" {
                            let _ = replace_element_inner_html(dom_source, selector.as_str(), "");
                        }
                        handled_selector = true;
                    }
                } else if rest_lower.starts_with(".addeventlistener(") {
                    let event = extract_first_string_literal(rest.as_str())
                        .map(|s| ascii_lower_str(s.as_str()))
                        .unwrap_or(String::new());
                    if matches!(event.as_str(), "load" | "domcontentloaded" | "click") {
                        if let Some(body) = extract_js_function_body(rest.as_str()) {
                            run_js_source(body.as_str(), dom_source, result, depth + 1);
                            handled_selector = true;
                        }
                    }
                } else if rest_lower.starts_with(".onclick") {
                    if let Some(body) = extract_js_function_body(rest.as_str()) {
                        run_js_source(body.as_str(), dom_source, result, depth + 1);
                        handled_selector = true;
                    } else if let Some((_, code)) = extract_assignment_string(rest.as_str()) {
                        run_js_source(code.as_str(), dom_source, result, depth + 1);
                        handled_selector = true;
                    }
                } else if rest_lower.starts_with(".click()") {
                    if let Some(handler) = extract_selector_attr(dom_source.as_str(), selector.as_str(), "onclick") {
                        run_js_source(handler.as_str(), dom_source, result, depth + 1);
                        handled_selector = true;
                    }
                }
            }
        }

        if handled_selector {
            continue;
        }

        result.unsupported_count += 1;
    }
}

fn run_js_minimal(scripts: &[String], dom_source: &mut String) -> JsResult {
    let mut result = JsResult::new();
    for script in scripts {
        run_js_source(script.as_str(), dom_source, &mut result, 0);
    }
    result
}

fn parse_html_to_lines(source_ascii: &str, css_rules: &[CssRule]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut stack: Vec<TagState> = Vec::new();
    let mut current_line = String::new();
    let mut ol_counters: Vec<usize> = Vec::new();
    let bytes = source_ascii.as_bytes();

    let mut i = 0usize;
    let mut text_start = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }

        // Emit text before tag.
        if text_start < i {
            let raw_text = &source_ascii[text_start..i];
            append_visible_text(raw_text, &stack, &mut lines, &mut current_line);
        }

        let Some(tag_end) = find_byte(bytes, b'>', i) else {
            break;
        };
        let tag_inner = source_ascii[i + 1..tag_end].trim();
        let tag_inner_lower = ascii_lower_str(tag_inner);

        if tag_inner_lower.starts_with("!--") {
            // Skip comments.
            i = tag_end + 1;
            text_start = i;
            continue;
        }

        let is_close = tag_inner.starts_with('/');
        let tag_text = if is_close {
            tag_inner[1..].trim()
        } else {
            tag_inner
        };

        let mut parts = tag_text.splitn(2, char::is_whitespace);
        let tag_name = ascii_lower_str(parts.next().unwrap_or(""));
        let attrs = parts.next().unwrap_or("").trim();
        let self_closing = tag_inner.ends_with('/') || tag_name == "br" || tag_name == "img";

        let default_block_tag = matches!(
            tag_name.as_str(),
            "html"
                | "body"
                | "main"
                | "section"
                | "article"
                | "header"
                | "footer"
                | "nav"
                | "div"
                | "p"
                | "ul"
                | "ol"
                | "li"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "pre"
                | "br"
                | "table"
                | "thead"
                | "tbody"
                | "tr"
                | "th"
                | "td"
        );

        if is_close {
            let mut closing_block = default_block_tag;
            let mut closing_pre = tag_name == "pre";
            if let Some(pos) = stack.iter().rposition(|s| s.tag == tag_name) {
                closing_block = closing_block || stack[pos].block;
                closing_pre = closing_pre || stack[pos].preserve_whitespace;
                stack.truncate(pos);
            }
            if closing_block || tag_name == "br" {
                flush_current_line(&mut lines, &mut current_line, closing_pre);
            }
            if tag_name == "ol" && !ol_counters.is_empty() {
                ol_counters.pop();
            }
            if matches!(tag_name.as_str(), "tr" | "table") {
                flush_current_line(&mut lines, &mut current_line, false);
            }
            if matches!(tag_name.as_str(), "p" | "div" | "section" | "article" | "li")
                && lines.last().map(|s| !s.is_empty()).unwrap_or(false)
            {
                push_line_raw(&mut lines, "");
            }
        } else {
            let id = attr_value(attrs, "id");
            let classes = attr_value(attrs, "class");
            let inline_style = attr_value(attrs, "style");
            let href = if tag_name == "a" {
                attr_value(attrs, "href")
            } else {
                None
            };

            let mut hide = false;
            let mut uppercase = matches!(tag_name.as_str(), "h1" | "h2" | "h3");
            let mut lowercase = false;
            let mut preserve_whitespace = matches!(tag_name.as_str(), "pre" | "textarea");
            let mut block_mode: Option<bool> = None;
            for rule in css_rules {
                if css_rule_matches(
                    rule,
                    tag_name.as_str(),
                    id.as_deref(),
                    classes.as_deref(),
                    &stack,
                ) {
                    hide = hide || rule.hide;
                    uppercase = uppercase || rule.uppercase;
                    lowercase = lowercase || rule.lowercase;
                    preserve_whitespace = preserve_whitespace || rule.preserve_whitespace;
                    if let Some(mode) = rule.block {
                        block_mode = Some(mode);
                    }
                }
            }
            if let Some(inline) = inline_style {
                let (inline_hide, inline_uppercase, inline_lowercase, inline_preserve, inline_block) =
                    parse_inline_style_full(inline.as_str());
                hide = hide || inline_hide;
                uppercase = uppercase || inline_uppercase;
                lowercase = lowercase || inline_lowercase;
                preserve_whitespace = preserve_whitespace || inline_preserve;
                if inline_block.is_some() {
                    block_mode = inline_block;
                }
            }

            let is_block_tag = block_mode.unwrap_or(default_block_tag);
            if is_block_tag {
                flush_current_line(
                    &mut lines,
                    &mut current_line,
                    active_preserve_whitespace(&stack),
                );
            }

            if tag_name == "br" {
                flush_current_line(
                    &mut lines,
                    &mut current_line,
                    active_preserve_whitespace(&stack),
                );
                if lines.last().map(|s| !s.is_empty()).unwrap_or(false) {
                    push_line_raw(&mut lines, "");
                }
            }

            if tag_name == "hr" {
                flush_current_line(&mut lines, &mut current_line, false);
                push_line_raw(&mut lines, "----------------------------------------");
            }

            let self_hidden = hide || active_hidden(&stack);
            if tag_name == "img" && !self_hidden {
                let alt = attr_value(attrs, "alt");
                let src = attr_value(attrs, "src");
                let img_line = if let Some(a) = alt {
                    format!("[Imagen] {}", a)
                } else if let Some(s) = src {
                    format!("[Imagen] {}", s)
                } else {
                    String::from("[Imagen]")
                };
                push_line(&mut lines, img_line.as_str());
            }

            if tag_name == "li" && !self_hidden {
                if let Some(counter) = ol_counters.last_mut() {
                    *counter += 1;
                    let marker = format!("{}. ", *counter);
                    if current_line.is_empty() {
                        current_line.push_str(marker.as_str());
                    } else {
                        append_wrapped_word(&mut lines, &mut current_line, marker.trim());
                    }
                } else if current_line.is_empty() {
                    current_line.push_str("- ");
                } else {
                    append_wrapped_word(&mut lines, &mut current_line, "-");
                }
            }

            if matches!(tag_name.as_str(), "td" | "th") && !self_hidden && !current_line.is_empty() {
                current_line.push_str(" | ");
            }

            if tag_name == "ol" && !self_hidden {
                ol_counters.push(0);
            }

            if !self_closing {
                stack.push(TagState {
                    tag: tag_name.clone(),
                    id,
                    class_attr: classes,
                    hide,
                    uppercase,
                    lowercase,
                    preserve_whitespace,
                    block: is_block_tag,
                    link: href,
                });
            }
        }

        i = tag_end + 1;
        text_start = i;
    }

    if text_start < source_ascii.len() {
        let raw_text = &source_ascii[text_start..];
        append_visible_text(raw_text, &stack, &mut lines, &mut current_line);
    }

    flush_current_line(
        &mut lines,
        &mut current_line,
        active_preserve_whitespace(&stack),
    );

    if lines.is_empty() {
        lines.push(String::from("(Documento sin contenido visible)"));
    }
    if lines.len() > MAX_RENDER_LINES {
        lines.truncate(MAX_RENDER_LINES);
        lines.push(String::from("[Salida truncada]"));
    }
    lines
}

#[derive(Clone)]
struct NativeTagContext {
    tag: String,
    block: bool,
    heading_level: u8,
    link_active: bool,
    preserve_whitespace: bool,
    hidden: bool,
    uppercase: bool,
    lowercase: bool,
    text_color: Option<u32>,
    background_color: Option<u32>,
    bold: bool,
    indent_left_px: u16,
    text_align: Option<NativeTextAlign>,
}

enum NativeToken {
    Block {
        depth: u8,
        tag: String,
        background_color: Option<u32>,
        text_color: Option<u32>,
        indent_left_px: u16,
        text_align: NativeTextAlign,
    },
    Text {
        depth: u8,
        heading_level: u8,
        link: bool,
        bold: bool,
        text_color: Option<u32>,
        background_color: Option<u32>,
        indent_left_px: u16,
        text_align: NativeTextAlign,
        text: String,
    },
    Break,
}

fn native_default_block_tag(tag: &str) -> bool {
    matches!(
        tag,
        "html"
            | "body"
            | "main"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "nav"
            | "div"
            | "p"
            | "ul"
            | "ol"
            | "li"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "pre"
            | "br"
            | "table"
            | "thead"
            | "tbody"
            | "tr"
            | "th"
            | "td"
            | "figure"
            | "figcaption"
            | "form"
            | "aside"
    )
}

fn native_active_hidden(stack: &[NativeTagContext]) -> bool {
    stack.iter().rev().any(|ctx| ctx.hidden)
}

fn native_active_link(stack: &[NativeTagContext]) -> bool {
    stack.iter().rev().any(|ctx| ctx.link_active)
}

fn native_active_heading_level(stack: &[NativeTagContext]) -> u8 {
    for ctx in stack.iter().rev() {
        if ctx.heading_level > 0 {
            return ctx.heading_level;
        }
    }
    0
}

fn native_active_preserve_whitespace(stack: &[NativeTagContext]) -> bool {
    stack.iter().rev().any(|ctx| ctx.preserve_whitespace)
}

fn native_active_uppercase(stack: &[NativeTagContext]) -> bool {
    stack.iter().rev().any(|ctx| ctx.uppercase)
}

fn native_active_lowercase(stack: &[NativeTagContext]) -> bool {
    stack.iter().rev().any(|ctx| ctx.lowercase)
}

fn native_active_text_color(stack: &[NativeTagContext]) -> Option<u32> {
    for ctx in stack.iter().rev() {
        if let Some(c) = ctx.text_color {
            return Some(c);
        }
    }
    None
}

fn native_active_background_color(stack: &[NativeTagContext]) -> Option<u32> {
    for ctx in stack.iter().rev() {
        if let Some(c) = ctx.background_color {
            return Some(c);
        }
    }
    None
}

fn native_active_bold(stack: &[NativeTagContext]) -> bool {
    stack.iter().rev().any(|ctx| ctx.bold)
}

fn native_active_indent_left(stack: &[NativeTagContext]) -> u16 {
    let mut total = 0u16;
    for ctx in stack.iter() {
        total = total.saturating_add(ctx.indent_left_px);
    }
    total.min(320)
}

fn native_active_text_align(stack: &[NativeTagContext]) -> NativeTextAlign {
    for ctx in stack.iter().rev() {
        if let Some(align) = ctx.text_align {
            return align;
        }
    }
    NativeTextAlign::Left
}

fn native_current_depth(stack: &[NativeTagContext]) -> u8 {
    let depth = stack.iter().filter(|ctx| ctx.block).count();
    depth.min(12) as u8
}

fn native_wrap_words(text: &str, max_chars: usize) -> Vec<String> {
    let limit = max_chars.max(8);
    let mut out = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if word.is_empty() {
            continue;
        }

        if word.len() > limit {
            if !current.is_empty() {
                out.push(current.clone());
                current.clear();
            }
            let mut start = 0usize;
            while start < word.len() {
                let end = (start + limit).min(word.len());
                out.push(String::from(&word[start..end]));
                start = end;
            }
            continue;
        }

        if current.is_empty() {
            current.push_str(word);
        } else if current.len().saturating_add(1).saturating_add(word.len()) <= limit {
            current.push(' ');
            current.push_str(word);
        } else {
            out.push(current.clone());
            current.clear();
            current.push_str(word);
        }
    }

    if !current.is_empty() {
        out.push(current);
    }

    if out.is_empty() {
        out.push(String::new());
    }

    out
}

fn native_push_text_tokens(raw_text: &str, stack: &[NativeTagContext], tokens: &mut Vec<NativeToken>) {
    if raw_text.is_empty() || native_active_hidden(stack) || tokens.len() >= NATIVE_MAX_TOKENS {
        return;
    }

    let preserve = native_active_preserve_whitespace(stack);
    let mut text = decode_entities(raw_text);
    if !preserve {
        text = collapse_spaces(text.as_str());
    }
    text = apply_case_transform(
        text.as_str(),
        native_active_uppercase(stack),
        native_active_lowercase(stack),
    );
    text = strip_angle_markup_fragments(text.as_str());
    if text.trim().is_empty() {
        return;
    }

    let depth = native_current_depth(stack);
    let link = native_active_link(stack);
    let heading_level = native_active_heading_level(stack);
    let text_color = native_active_text_color(stack);
    let background_color = native_active_background_color(stack);
    let bold = native_active_bold(stack);
    let indent_left_px = native_active_indent_left(stack);
    let text_align = native_active_text_align(stack);

    if preserve {
        for line in text.lines() {
            if tokens.len() >= NATIVE_MAX_TOKENS {
                break;
            }
            let clean = to_ascii_sanitized(line);
            if !clean.trim().is_empty() {
                tokens.push(NativeToken::Text {
                    depth,
                    heading_level,
                    link,
                    bold,
                    text_color,
                    background_color,
                    indent_left_px,
                    text_align,
                    text: clean,
                });
            }
            tokens.push(NativeToken::Break);
        }
        return;
    }

    let clean = to_ascii_sanitized(text.trim());
    if clean.is_empty() {
        return;
    }
    tokens.push(NativeToken::Text {
        depth,
        heading_level,
        link,
        bold,
        text_color,
        background_color,
        indent_left_px,
        text_align,
        text: clean,
    });
}

fn native_parse_tokens(source_ascii: &str, css_rules: &[CssRule]) -> Vec<NativeToken> {
    let mut tokens = Vec::new();
    let bytes = source_ascii.as_bytes();
    let mut stack: Vec<NativeTagContext> = Vec::new();
    let mut css_stack: Vec<TagState> = Vec::new();
    let mut i = 0usize;
    let mut text_start = 0usize;

    while i < bytes.len() && tokens.len() < NATIVE_MAX_TOKENS {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }

        if text_start < i {
            native_push_text_tokens(&source_ascii[text_start..i], &stack, &mut tokens);
        }

        let Some(tag_end) = find_byte(bytes, b'>', i) else {
            break;
        };
        let tag_inner = source_ascii[i + 1..tag_end].trim();
        let tag_inner_lower = ascii_lower_str(tag_inner);

        if tag_inner_lower.starts_with("!--") {
            i = tag_end + 1;
            text_start = i;
            continue;
        }

        let is_close = tag_inner.starts_with('/');
        let tag_text = if is_close {
            tag_inner[1..].trim()
        } else {
            tag_inner
        };

        let mut parts = tag_text.splitn(2, char::is_whitespace);
        let tag_name = ascii_lower_str(parts.next().unwrap_or(""));
        let attrs = parts.next().unwrap_or("").trim();
        let self_closing = tag_inner.ends_with('/') || tag_name == "br" || tag_name == "img";
        let default_block = native_default_block_tag(tag_name.as_str());

        if is_close {
            if let Some(pos) = stack.iter().rposition(|ctx| ctx.tag == tag_name) {
                stack.truncate(pos);
            }
            if let Some(pos) = css_stack.iter().rposition(|ctx| ctx.tag == tag_name) {
                css_stack.truncate(pos);
            }
            if default_block {
                tokens.push(NativeToken::Break);
            }
            if matches!(tag_name.as_str(), "p" | "div" | "section" | "article" | "li") {
                tokens.push(NativeToken::Break);
            }
        } else {
            let id = attr_value(attrs, "id");
            let classes = attr_value(attrs, "class");
            let inline_style = attr_value(attrs, "style");
            let (inline_hide, inline_uppercase, inline_lowercase, inline_preserve, inline_block) =
                match inline_style.as_ref() {
                    Some(s) => parse_inline_style_full(s.as_str()),
                    None => (false, false, false, false, None),
                };
            let (inline_text_color, inline_background_color, inline_bold) = match inline_style.as_ref() {
                Some(s) => parse_inline_visual_style(s.as_str()),
                None => (None, None, false),
            };
            let (inline_indent_left_px, inline_text_align) = match inline_style.as_ref() {
                Some(s) => parse_inline_layout_style(s.as_str()),
                None => (None, None),
            };

            let mut hide = inline_hide;
            let mut block_mode = inline_block;
            let mut preserve_whitespace = inline_preserve || tag_name == "pre" || tag_name == "textarea";
            let mut uppercase = inline_uppercase;
            let mut lowercase = inline_lowercase;
            let mut text_color = None;
            let mut background_color = None;
            let mut bold = inline_bold;
            let mut indent_left_px = None;
            let mut text_align = None;
            for rule in css_rules {
                if css_rule_matches(
                    rule,
                    tag_name.as_str(),
                    id.as_deref(),
                    classes.as_deref(),
                    css_stack.as_slice(),
                ) {
                    hide = hide || rule.hide;
                    preserve_whitespace = preserve_whitespace || rule.preserve_whitespace;
                    uppercase = uppercase || rule.uppercase;
                    lowercase = lowercase || rule.lowercase;
                    if rule.block.is_some() {
                        block_mode = rule.block;
                    }
                    if let Some(c) = rule.text_color {
                        text_color = Some(c);
                    }
                    if let Some(c) = rule.background_color {
                        background_color = Some(c);
                    }
                    bold = bold || rule.bold;
                    if let Some(px) = rule.indent_left_px {
                        indent_left_px = Some(px);
                    }
                    if let Some(align) = rule.text_align {
                        text_align = Some(align);
                    }
                }
            }
            if inline_text_color.is_some() {
                text_color = inline_text_color;
            }
            if inline_background_color.is_some() {
                background_color = inline_background_color;
            }
            if inline_indent_left_px.is_some() {
                indent_left_px = inline_indent_left_px;
            }
            if inline_text_align.is_some() {
                text_align = inline_text_align;
            }

            let heading_level = match tag_name.as_str() {
                "h1" => 1,
                "h2" => 2,
                "h3" => 3,
                "h4" => 4,
                "h5" => 5,
                "h6" => 6,
                _ => 0,
            };
            let link_active = tag_name == "a";
            let block = block_mode.unwrap_or(default_block);
            let hidden = hide || native_active_hidden(&stack);
            let depth = native_current_depth(&stack);
            let effective_indent = native_active_indent_left(&stack)
                .saturating_add(indent_left_px.unwrap_or(0))
                .min(320);
            let effective_align = text_align.unwrap_or_else(|| native_active_text_align(&stack));

            if block && !hidden {
                tokens.push(NativeToken::Block {
                    depth,
                    tag: tag_name.clone(),
                    background_color,
                    text_color,
                    indent_left_px: effective_indent,
                    text_align: effective_align,
                });
            }

            if tag_name == "br" {
                tokens.push(NativeToken::Break);
            } else if tag_name == "img" && !hidden {
                let alt = attr_value(attrs, "alt");
                let src = attr_value(attrs, "src");
                let text = if let Some(a) = alt {
                    format!("[img] {}", a.trim())
                } else if let Some(s) = src {
                    format!("[img] {}", s.trim())
                } else {
                    String::from("[img]")
                };
                tokens.push(NativeToken::Text {
                    depth: depth.saturating_add(1),
                    heading_level: 0,
                    link: false,
                    bold,
                    text_color,
                    background_color,
                    indent_left_px: effective_indent,
                    text_align: effective_align,
                    text,
                });
            }

            if !self_closing {
                css_stack.push(TagState {
                    tag: tag_name.clone(),
                    id: id.clone(),
                    class_attr: classes.clone(),
                    hide,
                    uppercase: false,
                    lowercase: false,
                    preserve_whitespace,
                    block,
                    link: None,
                });
                stack.push(NativeTagContext {
                    tag: tag_name,
                    block,
                    heading_level,
                    link_active,
                    preserve_whitespace,
                    hidden,
                    uppercase,
                    lowercase,
                    text_color,
                    background_color,
                    bold,
                    indent_left_px: indent_left_px.unwrap_or(0).min(320),
                    text_align,
                });
            }
        }

        i = tag_end + 1;
        text_start = i;
    }

    if text_start < source_ascii.len() && tokens.len() < NATIVE_MAX_TOKENS {
        native_push_text_tokens(&source_ascii[text_start..], &stack, &mut tokens);
    }

    tokens
}

fn native_draw_char(
    width: usize,
    height: usize,
    pixels: &mut [u32],
    x: usize,
    y: usize,
    ch: char,
    color: u32,
) {
    let glyph = crate::font::glyph_5x7(if ch.is_ascii() { ch } else { '?' });
    for (row, bits) in glyph.iter().enumerate() {
        let py = y.saturating_add(row);
        if py >= height {
            continue;
        }
        for col in 0..5usize {
            if (*bits & (1 << (4 - col))) == 0 {
                continue;
            }
            let px = x.saturating_add(col);
            if px >= width {
                continue;
            }
            let idx = py.saturating_mul(width).saturating_add(px);
            if idx < pixels.len() {
                pixels[idx] = color;
            }
        }
    }
}

fn native_draw_text(
    width: usize,
    height: usize,
    pixels: &mut [u32],
    x: usize,
    y: usize,
    text: &str,
    color: u32,
    bold: bool,
) {
    let mut cx = x;
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            break;
        }
        native_draw_char(width, height, pixels, cx, y, ch, color);
        if bold {
            native_draw_char(width, height, pixels, cx.saturating_add(1), y, ch, color);
        }
        cx = cx.saturating_add(6);
        if cx + 5 >= width {
            break;
        }
    }
}

fn native_fill_rect(
    width: usize,
    height: usize,
    pixels: &mut [u32],
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    color: u32,
) {
    let x_end = x.saturating_add(w).min(width);
    let y_end = y.saturating_add(h).min(height);
    for py in y..y_end {
        for px in x..x_end {
            let idx = py.saturating_mul(width).saturating_add(px);
            if idx < pixels.len() {
                pixels[idx] = color;
            }
        }
    }
}

fn render_html_native_surface(
    title: Option<&str>,
    body_source: &str,
    css_rules: &[CssRule],
) -> BrowserRenderSurface {
    let width = NATIVE_SURFACE_W as usize;
    let height = NATIVE_SURFACE_H as usize;
    let mut pixels = Vec::new();
    pixels.resize(width.saturating_mul(height), 0xF6F8FC);

    // Header and gutter.
    native_fill_rect(width, height, pixels.as_mut_slice(), 0, 0, width, 24, 0x1C2F4A);
    native_fill_rect(width, height, pixels.as_mut_slice(), 0, 24, 8, height.saturating_sub(24), 0xE2E8F2);

    let header_title = match title {
        Some(t) if !t.trim().is_empty() => t.trim(),
        _ => "ReduxOS Native WebEngine",
    };
    native_draw_text(
        width,
        height,
        pixels.as_mut_slice(),
        10,
        8,
        header_title,
        0xFFFFFF,
        true,
    );

    let tokens = native_parse_tokens(body_source, css_rules);
    let mut y = 30usize;
    let max_y = height.saturating_sub(10);

    for token in tokens.iter() {
        if y >= max_y {
            break;
        }
        match token {
            NativeToken::Break => {
                y = y.saturating_add(6);
            }
            NativeToken::Block {
                depth,
                tag,
                background_color,
                text_color,
                indent_left_px,
                text_align,
            } => {
                let indent = 12usize
                    .saturating_add((*depth as usize).saturating_mul(14))
                    .saturating_add(*indent_left_px as usize)
                    .min(width.saturating_sub(10));
                if let Some(bg) = background_color {
                    native_fill_rect(
                        width,
                        height,
                        pixels.as_mut_slice(),
                        indent,
                        y.saturating_sub(1),
                        width.saturating_sub(indent).saturating_sub(4),
                        10,
                        *bg,
                    );
                }
                native_fill_rect(
                    width,
                    height,
                    pixels.as_mut_slice(),
                    indent.saturating_sub(6),
                    y.saturating_add(2),
                    3,
                    8,
                    0x8EA1B8,
                );
                native_draw_text(
                    width,
                    height,
                    pixels.as_mut_slice(),
                    match text_align {
                        NativeTextAlign::Left => indent,
                        NativeTextAlign::Center => {
                            let label_w = tag.len().saturating_add(2).saturating_mul(6);
                            let avail = width.saturating_sub(indent).saturating_sub(8);
                            if avail > label_w {
                                indent.saturating_add((avail - label_w) / 2)
                            } else {
                                indent
                            }
                        }
                        NativeTextAlign::Right => {
                            let label_w = tag.len().saturating_add(2).saturating_mul(6);
                            width.saturating_sub(8).saturating_sub(label_w).max(indent)
                        }
                    },
                    y,
                    format!("<{}>", tag).as_str(),
                    text_color.unwrap_or(0x4A5D73),
                    false,
                );
                y = y.saturating_add(10);
            }
            NativeToken::Text {
                depth,
                heading_level,
                link,
                bold,
                text_color,
                background_color,
                indent_left_px,
                text_align,
                text,
            } => {
                let indent = 18usize
                    .saturating_add((*depth as usize).saturating_mul(14))
                    .saturating_add(*indent_left_px as usize)
                    .min(width.saturating_sub(10));
                let avail_px = width.saturating_sub(indent).saturating_sub(8);
                let max_chars = (avail_px / 6).max(8);
                let wrapped = native_wrap_words(text.as_str(), max_chars);
                let mut color = if *link {
                    0x1F5FD6
                } else if *heading_level > 0 {
                    0x111E2E
                } else {
                    0x1E2732
                };
                if let Some(c) = text_color {
                    color = *c;
                }
                let text_bold = *bold || *heading_level > 0;
                for line in wrapped {
                    if y >= max_y {
                        break;
                    }
                    let line_w = line.len().saturating_mul(6).saturating_add(1);
                    let draw_x = match text_align {
                        NativeTextAlign::Left => indent,
                        NativeTextAlign::Center => {
                            if avail_px > line_w {
                                indent.saturating_add((avail_px - line_w) / 2)
                            } else {
                                indent
                            }
                        }
                        NativeTextAlign::Right => {
                            width.saturating_sub(8).saturating_sub(line_w).max(indent)
                        }
                    };
                    if let Some(bg) = background_color {
                        native_fill_rect(
                            width,
                            height,
                            pixels.as_mut_slice(),
                            draw_x.saturating_sub(1),
                            y.saturating_sub(1),
                            line_w
                                .saturating_add(2)
                                .min(width.saturating_sub(draw_x).saturating_sub(2)),
                            if *heading_level > 0 { 11 } else { 9 },
                            *bg,
                        );
                    }
                    native_draw_text(
                        width,
                        height,
                        pixels.as_mut_slice(),
                        draw_x,
                        y,
                        line.as_str(),
                        color,
                        text_bold,
                    );
                    y = y.saturating_add(if *heading_level > 0 { 11 } else { 9 });
                }
            }
        }
    }

    BrowserRenderSurface {
        source: String::from("native-dom-layout-raster-v1"),
        width: NATIVE_SURFACE_W,
        height: NATIVE_SURFACE_H,
        pixels,
    }
}

fn render_html_document(html_raw: &str) -> (Option<String>, Vec<String>, Option<BrowserRenderSurface>) {
    let html_ascii = to_ascii_sanitized(html_raw);
    let (without_style, style_blocks) = extract_tag_blocks(html_ascii.as_str(), "style");
    let css_rules = parse_css_rules(&style_blocks);
    let (without_script, script_blocks) = extract_tag_blocks(without_style.as_str(), "script");
    let mut dom_source = without_script.clone();

    let total_script_bytes: usize = script_blocks.iter().map(|s| s.len()).sum();
    let skip_js_runtime = script_blocks.len() > 24 || total_script_bytes > (96 * 1024);
    let js = if skip_js_runtime {
        JsResult::new()
    } else {
        run_js_minimal(&script_blocks, &mut dom_source)
    };
    dom_source = strip_non_render_blocks(dom_source.as_str());
    let default_title = extract_first_tag_text(dom_source.as_str(), "title");

    let lower = ascii_lower_str(dom_source.as_str());
    let body_source = if let Some(start) = lower.find("<body") {
        let body_open_end = lower[start..]
            .find('>')
            .map(|v| start + v + 1)
            .unwrap_or(start);
        if let Some(body_close_rel) = lower[body_open_end..].find("</body>") {
            &dom_source[body_open_end..body_open_end + body_close_rel]
        } else {
            &dom_source[body_open_end..]
        }
    } else {
        dom_source.as_str()
    };

    let mut lines = if let Some(text) = js.body_override.as_ref() {
        text.lines().map(|l| String::from(l.trim())).filter(|l| !l.is_empty()).collect()
    } else {
        parse_html_to_lines(body_source, &css_rules)
    };

    for extra in js.body_append {
        let mut clean = collapse_spaces(&decode_entities(extra.as_str()));
        if !clean.is_empty() {
            clean = String::from(clean.trim());
            if !looks_like_js_payload(clean.as_str()) {
                lines.push(clean);
            }
        }
    }

    if !js.logs.is_empty() {
        lines.push(String::new());
        lines.push(String::from("[JS Console]"));
        for log in js.logs {
            lines.push(log);
        }
    }
    sanitize_render_lines(&mut lines);

    let title = if js.title_override.is_some() {
        js.title_override
    } else {
        default_title
    };

    let surface = if is_native_render_enabled() {
        Some(render_html_native_surface(
            title.as_deref(),
            body_source,
            &css_rules,
        ))
    } else {
        None
    };

    (title, lines, surface)
}

fn render_plain_text(text: &str) -> Vec<String> {
    let ascii = to_ascii_sanitized(text);
    let mut out = Vec::new();
    for line in ascii.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            out.push(String::from(trimmed));
        } else if !out.last().map(|s| s.is_empty()).unwrap_or(false) {
            out.push(String::new());
        }
        if out.len() >= MAX_RENDER_LINES {
            break;
        }
    }
    if out.is_empty() {
        out.push(String::from("(Sin contenido)"));
    }
    out
}

fn fetch_with_redirects(
    start_url: &str,
    pump_ui: &mut impl FnMut(),
) -> Option<(ParsedHttp, String, usize)> {
    let mut current_url = String::from(start_url.trim());
    let mut redirects = 0usize;

    loop {
        let raw = crate::net::http_get_request(current_url.as_str(), pump_ui)?;
        let parsed = parse_http_response(raw.as_str());

        let status = parsed.status_code.unwrap_or(0);
        if matches!(status, 301 | 302 | 303 | 307 | 308)
            && redirects < MAX_REDIRECTS
            && header_value(&parsed, "location").is_some()
        {
            let location = header_value(&parsed, "location").unwrap_or("");
            current_url = resolve_redirect_url(current_url.as_str(), location);
            redirects += 1;
            continue;
        }

        return Some((parsed, current_url, redirects));
    }
}

fn response_blocked_for_reader(parsed: &ParsedHttp) -> bool {
    if matches!(parsed.status_code.unwrap_or(0), 401 | 403 | 429 | 451 | 503) {
        return true;
    }

    let body = ascii_lower_str(parsed.body.as_str());
    body.contains("securitycompromiseerror")
        || body.contains("anonymous access to domain")
        || body.contains("access denied")
        || body.contains("request blocked")
        || body.contains("captcha")
        || body.contains("too many requests")
}

fn rendered_lines_unusable(lines: &[String]) -> bool {
    let mut meaningful = 0usize;
    for line in lines {
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        if text.starts_with("[HTTP]") || text.starts_with("[TLS]") || text.starts_with("[Render]") {
            continue;
        }
        if text == "(Sin contenido)"
            || text == "(Documento sin contenido visible)"
        {
            continue;
        }
        meaningful += 1;
        if meaningful >= 2 {
            return false;
        }
    }
    true
}

fn render_parsed_response(
    parsed: &ParsedHttp,
) -> (Option<String>, Vec<String>, Option<BrowserRenderSurface>) {
    let content_type = header_value(parsed, "content-type").unwrap_or("");
    let looks_html = content_type.contains("text/html")
        || content_type.contains("application/xhtml+xml")
        || parsed.body.contains("<html")
        || parsed.body.contains("<HTML")
        || parsed.body.contains("<body")
        || parsed.body.contains("<!DOCTYPE")
        || parsed.body.contains("<svg");

    if looks_html {
        render_html_document(parsed.body.as_str())
    } else {
        (None, render_plain_text(parsed.body.as_str()), None)
    }
}

pub fn fetch_and_render(url: &str, pump_ui: &mut impl FnMut()) -> Option<BrowserRenderOutput> {
    let base_url = String::from(url.trim());
    if base_url.is_empty() {
        return None;
    }

    // Native route first: direct fetch without host/bridge dependency.
    let _ = crate::net::set_https_mode_disabled();
    let show_tls_banner = starts_with_ignore_ascii_case(base_url.as_str(), "https://")
        && crate::net::is_https_proxy_enabled();

    let mut used_reader_proxy = false;
    let mut reader_note: Option<String> = None;
    let (mut parsed, mut final_url, mut redirects) =
        if let Some((parsed, final_url, redirects)) = fetch_with_redirects(base_url.as_str(), pump_ui)
        {
            (parsed, final_url, redirects)
        } else if should_try_reader_proxy(base_url.as_str()) {
            let proxy_url = build_reader_proxy_url(base_url.as_str())?;
            let (proxy_parsed, _proxy_final, proxy_redirects) =
                fetch_with_redirects(proxy_url.as_str(), pump_ui)?;
            used_reader_proxy = true;
            reader_note = Some(String::from(
                "[Render] fetch directo fallo; usando fallback reader-proxy.",
            ));
            (proxy_parsed, String::from(base_url.as_str()), proxy_redirects)
        } else {
            return None;
        };

    let (mut title, mut lines, mut surface) = render_parsed_response(&parsed);

    if should_try_reader_proxy(base_url.as_str()) && !used_reader_proxy {
        let blocked = response_blocked_for_reader(&parsed);
        let unusable = rendered_lines_unusable(lines.as_slice());
        if blocked || unusable {
            if let Some(proxy_url) = build_reader_proxy_url(base_url.as_str()) {
                if let Some((proxy_parsed, _proxy_final, proxy_redirects)) =
                    fetch_with_redirects(proxy_url.as_str(), pump_ui)
                {
                    let (proxy_title, proxy_lines, proxy_surface) =
                        render_parsed_response(&proxy_parsed);
                    let proxy_usable = !rendered_lines_unusable(proxy_lines.as_slice());
                    if blocked || proxy_usable {
                        parsed = proxy_parsed;
                        final_url = String::from(base_url.as_str());
                        redirects = proxy_redirects;
                        title = if proxy_title.is_some() {
                            proxy_title
                        } else {
                            title
                        };
                        lines = proxy_lines;
                        surface = proxy_surface;
                        used_reader_proxy = true;
                        reader_note = Some(if blocked {
                            String::from(
                                "[Render] pagina bloqueada; fallback reader-proxy aplicado.",
                            )
                        } else {
                            String::from(
                                "[Render] fallback reader-proxy para evitar salida ilegible.",
                            )
                        });
                    }
                }
            }
        }
    }

    if let Some(note) = reader_note {
        let mut prefix = Vec::new();
        prefix.push(note);
        prefix.push(String::new());
        prefix.extend(lines);
        lines = prefix;
    }

    if show_tls_banner {
        let mut prefix = Vec::new();
        prefix.push(String::from(
            "[TLS] modo compatibilidad activo: certificados/SNI validados por proxy.",
        ));
        prefix.push(String::new());
        prefix.extend(lines);
        lines = prefix;
    }

    if redirects > 0 {
        let mut prefix = Vec::new();
        prefix.push(format!("[HTTP] redirects seguidos: {}", redirects));
        prefix.push(format!("[HTTP] URL final: {}", final_url.as_str()));
        prefix.push(String::new());
        prefix.extend(lines);
        lines = prefix;
    }

    let status = if used_reader_proxy {
        String::from("Done (Reader fallback)")
    } else {
        parsed.status_line.unwrap_or(String::from("Done"))
    };
    Some(BrowserRenderOutput {
        final_url,
        status,
        title,
        lines,
        surface,
    })
}
