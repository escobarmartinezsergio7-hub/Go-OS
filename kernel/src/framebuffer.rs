use core::ptr;

const BACKBUFFER_CAPACITY: usize = 64 * 1024 * 1024;

#[repr(align(64))]
struct AlignedBackbuffer([u8; BACKBUFFER_CAPACITY]);

static mut BACKBUFFER: AlignedBackbuffer = AlignedBackbuffer([0; BACKBUFFER_CAPACITY]);

#[derive(Clone, Copy)]
pub enum PixelLayout {
    Rgb,
    Bgr,
    Unknown,
}

#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    pub base: *mut u8,
    pub size: usize,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub layout: PixelLayout,
}

#[derive(Clone, Copy)]
struct Framebuffer {
    front_base: *mut u8,
    draw_base: *mut u8,
    size: usize,
    width: usize,
    height: usize,
    stride: usize,
    layout: PixelLayout,
    backbuffer_enabled: bool,
}

impl Framebuffer {
    const fn empty() -> Self {
        Self {
            front_base: ptr::null_mut(),
            draw_base: ptr::null_mut(),
            size: 0,
            width: 0,
            height: 0,
            stride: 0,
            layout: PixelLayout::Unknown,
            backbuffer_enabled: false,
        }
    }
}

static mut FB: Framebuffer = Framebuffer::empty();

pub fn init(info: FramebufferInfo) {
    unsafe {
        FB = Framebuffer {
            front_base: info.base,
            draw_base: info.base,
            size: info.size,
            width: info.width,
            height: info.height,
            stride: info.stride,
            layout: info.layout,
            backbuffer_enabled: false,
        };
    }
}

pub fn dimensions() -> (usize, usize) {
    unsafe { (FB.width, FB.height) }
}

pub fn enable_backbuffer() -> bool {
    unsafe {
        if FB.size == 0 || FB.front_base.is_null() || FB.size > BACKBUFFER_CAPACITY {
            FB.draw_base = FB.front_base;
            FB.backbuffer_enabled = false;
            return false;
        }

        let back_ptr = core::ptr::addr_of_mut!(BACKBUFFER.0) as *mut u8;
        ptr::copy_nonoverlapping(FB.front_base as *const u8, back_ptr, FB.size);

        FB.draw_base = back_ptr;
        FB.backbuffer_enabled = true;
        true
    }
}

pub fn backbuffer_enabled() -> bool {
    unsafe { FB.backbuffer_enabled }
}

pub fn draw_base() -> *mut u8 {
    unsafe { FB.draw_base }
}

pub fn present() {
    unsafe {
        if !FB.backbuffer_enabled || FB.front_base.is_null() || FB.draw_base.is_null() {
            return;
        }

        ptr::copy_nonoverlapping(FB.draw_base as *const u8, FB.front_base, FB.size);
    }
}

#[inline]
fn write_pixel_raw(offset: usize, r: u8, g: u8, b: u8) {
    unsafe {
        if offset + 3 >= FB.size {
            return;
        }

        let ptr = FB.draw_base.add(offset);
        if FB.backbuffer_enabled {
            match FB.layout {
                PixelLayout::Rgb => {
                    ptr.write(r);
                    ptr.add(1).write(g);
                    ptr.add(2).write(b);
                    ptr.add(3).write(0);
                }
                PixelLayout::Bgr | PixelLayout::Unknown => {
                    ptr.write(b);
                    ptr.add(1).write(g);
                    ptr.add(2).write(r);
                    ptr.add(3).write(0);
                }
            }
        } else {
            match FB.layout {
                PixelLayout::Rgb => {
                    ptr.write_volatile(r);
                    ptr.add(1).write_volatile(g);
                    ptr.add(2).write_volatile(b);
                    ptr.add(3).write_volatile(0);
                }
                PixelLayout::Bgr | PixelLayout::Unknown => {
                    ptr.write_volatile(b);
                    ptr.add(1).write_volatile(g);
                    ptr.add(2).write_volatile(r);
                    ptr.add(3).write_volatile(0);
                }
            }
        }
    }
}

pub fn clear(color: u32) {
    let (r, g, b) = split_rgb(color);
    let (w, h) = dimensions();
    rect(0, 0, w, h, rgb(r, g, b));
}

pub fn pixel(x: usize, y: usize, color: u32) {
    unsafe {
        if x >= FB.width || y >= FB.height {
            return;
        }

        let (r, g, b) = split_rgb(color);
        let byte_index = (y * FB.stride + x) * 4;
        write_pixel_raw(byte_index, r, g, b);
    }
}

pub fn rect(x: usize, y: usize, w: usize, h: usize, color: u32) {
    if w == 0 || h == 0 {
        return;
    }

    unsafe {
        let max_x = x.saturating_add(w).min(FB.width);
        let max_y = y.saturating_add(h).min(FB.height);
        if max_x <= x || max_y <= y {
            return;
        }

        // Fast path used by current runtime: backbuffer CPU rendering.
        if FB.backbuffer_enabled {
            let (r, g, b) = split_rgb(color);
            let packed = match FB.layout {
                PixelLayout::Rgb => (r as u32) | ((g as u32) << 8) | ((b as u32) << 16),
                PixelLayout::Bgr | PixelLayout::Unknown => {
                    (b as u32) | ((g as u32) << 8) | ((r as u32) << 16)
                }
            };

            let span = max_x - x;
            let mut yy = y;
            while yy < max_y {
                let row_off = (yy * FB.stride + x) * 4;
                let mut dst = FB.draw_base.add(row_off) as *mut u32;
                let mut i = 0usize;
                while i < span {
                    dst.write(packed);
                    dst = dst.add(1);
                    i += 1;
                }
                yy += 1;
            }
            return;
        }

        // Slow fallback path (front-buffer/direct write).
        let (r, g, b) = split_rgb(color);
        let mut yy = y;
        while yy < max_y {
            let mut xx = x;
            while xx < max_x {
                let byte_index = (yy * FB.stride + xx) * 4;
                write_pixel_raw(byte_index, r, g, b);
                xx += 1;
            }
            yy += 1;
        }
    }
}

pub fn blit(x: usize, y: usize, w: usize, h: usize, buffer: &[u32]) {
    if w == 0 || h == 0 || buffer.len() < w * h {
        return;
    }

    unsafe {
        let max_x = x.saturating_add(w).min(FB.width);
        let max_y = y.saturating_add(h).min(FB.height);
        
        let mut yy = y;
        while yy < max_y {
            let win_y = yy - y;
            let span = max_x - x;
            if span == 0 { yy += 1; continue; }
            
            let fb_off = (yy * FB.stride + x) * 4;
            let win_off = win_y * w;
            
            let dst = FB.draw_base.add(fb_off);
            let src = buffer.as_ptr().add(win_off);

            if FB.backbuffer_enabled {
                 let mut i = 0usize;
                 while i < span {
                     let color = *src.add(i);
                     let (r, g, b) = split_rgb(color);
                     let ptr = dst.add(i * 4);
                     match FB.layout {
                         PixelLayout::Rgb => {
                             ptr.write(r);
                             ptr.add(1).write(g);
                             ptr.add(2).write(b);
                             ptr.add(3).write(0);
                         }
                         PixelLayout::Bgr | PixelLayout::Unknown => {
                             ptr.write(b);
                             ptr.add(1).write(g);
                             ptr.add(2).write(r);
                             ptr.add(3).write(0);
                         }
                     }
                     i += 1;
                 }
            } else {
                 let mut i = 0usize;
                 while i < span {
                     let color = *src.add(i);
                     let (r, g, b) = split_rgb(color);
                     let ptr = dst.add(i * 4);
                     match FB.layout {
                         PixelLayout::Rgb => {
                             ptr.write_volatile(r);
                             ptr.add(1).write_volatile(g);
                             ptr.add(2).write_volatile(b);
                             ptr.add(3).write_volatile(0);
                         }
                         PixelLayout::Bgr | PixelLayout::Unknown => {
                             ptr.write_volatile(b);
                             ptr.add(1).write_volatile(g);
                             ptr.add(2).write_volatile(r);
                             ptr.add(3).write_volatile(0);
                         }
                     }
                     i += 1;
                 }
            }
            yy += 1;
        }
    }
}

pub fn digit_7seg(x: usize, y: usize, scale: usize, value: u8, color: u32) {
    let scale = scale.max(1);
    let t = scale;
    let w = scale * 6;
    let h = scale * 10;

    let segments = match value {
        0 => 0b0111111,
        1 => 0b0000110,
        2 => 0b1011011,
        3 => 0b1001111,
        4 => 0b1100110,
        5 => 0b1101101,
        6 => 0b1111101,
        7 => 0b0000111,
        8 => 0b1111111,
        9 => 0b1101111,
        _ => 0,
    };

    // a
    if (segments & (1 << 0)) != 0 {
        rect(x + t, y, w - (2 * t), t, color);
    }
    // b
    if (segments & (1 << 1)) != 0 {
        rect(x + w - t, y + t, t, (h / 2) - t, color);
    }
    // c
    if (segments & (1 << 2)) != 0 {
        rect(x + w - t, y + (h / 2), t, (h / 2) - t, color);
    }
    // d
    if (segments & (1 << 3)) != 0 {
        rect(x + t, y + h - t, w - (2 * t), t, color);
    }
    // e
    if (segments & (1 << 4)) != 0 {
        rect(x, y + (h / 2), t, (h / 2) - t, color);
    }
    // f
    if (segments & (1 << 5)) != 0 {
        rect(x, y + t, t, (h / 2) - t, color);
    }
    // g
    if (segments & (1 << 6)) != 0 {
        rect(x + t, y + (h / 2) - (t / 2), w - (2 * t), t, color);
    }
}

pub fn draw_u64_digits(x: usize, y: usize, scale: usize, value: u64, color: u32) {
    let mut buf = [0u8; 20];
    let mut n = value;
    let mut count = 0usize;

    if n == 0 {
        buf[0] = 0;
        count = 1;
    } else {
        while n > 0 && count < buf.len() {
            buf[count] = (n % 10) as u8;
            n /= 10;
            count += 1;
        }
    }

    let digit_w = scale.max(1) * 6;
    let spacing = scale.max(1) * 2;

    let mut i = 0;
    while i < count {
        let d = buf[count - 1 - i];
        digit_7seg(x + i * (digit_w + spacing), y, scale, d, color);
        i += 1;
    }
}

fn glyph_5x7(ch: char) -> [u8; 7] {
    let c = if ch.is_ascii_lowercase() {
        ((ch as u8) - b'a' + b'A') as char
    } else {
        ch
    };

    match c {
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0E],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1F],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x12, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],

        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x14, 0x04, 0x04, 0x04, 0x1F],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1E, 0x01, 0x01, 0x0E, 0x01, 0x01, 0x1E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x10, 0x1E, 0x01, 0x01, 0x1E],
        '6' => [0x0E, 0x10, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x01, 0x0E],

        ' ' => [0, 0, 0, 0, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 0x0C, 0x0C],
        ',' => [0, 0, 0, 0, 0x0C, 0x0C, 0x08],
        ':' => [0, 0x0C, 0x0C, 0, 0x0C, 0x0C, 0],
        ';' => [0, 0x0C, 0x0C, 0, 0x0C, 0x0C, 0x08],
        '-' => [0, 0, 0, 0x1F, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0x1F],
        '=' => [0, 0x1F, 0, 0x1F, 0, 0, 0],
        '+' => [0, 0x04, 0x04, 0x1F, 0x04, 0x04, 0],
        '/' => [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10],
        '\\' => [0x10, 0x08, 0x08, 0x04, 0x02, 0x02, 0x01],
        '>' => [0x10, 0x08, 0x04, 0x02, 0x04, 0x08, 0x10],
        '<' => [0x01, 0x02, 0x04, 0x08, 0x04, 0x02, 0x01],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '[' => [0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E],
        ']' => [0x0E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0E],
        '?' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0, 0x04],
        '!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0, 0x04],
        '#' => [0x0A, 0x1F, 0x0A, 0x0A, 0x1F, 0x0A, 0],
        '*' => [0, 0x0A, 0x04, 0x1F, 0x04, 0x0A, 0],
        '"' => [0x0A, 0x0A, 0, 0, 0, 0, 0],
        '\'' => [0x04, 0x04, 0, 0, 0, 0, 0],
        '@' => [0x0E, 0x11, 0x17, 0x15, 0x17, 0x10, 0x0E],
        _ => [0x1F, 0x11, 0x15, 0x15, 0x11, 0x11, 0x1F],
    }
}

pub fn draw_char_5x7(x: usize, y: usize, ch: char, color: u32) {
    let glyph = glyph_5x7(ch);
    let mut row = 0usize;
    while row < glyph.len() {
        let bits = glyph[row];
        let mut col = 0usize;
        while col < 5 {
            let mask = 1 << (4 - col);
            if (bits & mask) != 0 {
                pixel(x + col, y + row, color);
            }
            col += 1;
        }
        row += 1;
    }
}

pub fn draw_text_5x7(x: usize, y: usize, text: &str, color: u32) {
    draw_text_5x7_bytes(x, y, text.as_bytes(), color);
}

pub fn draw_text_5x7_bytes(x: usize, y: usize, text: &[u8], color: u32) {
    let mut cx = x;
    let mut cy = y;
    let mut i = 0usize;
    while i < text.len() {
        let b = text[i];
        if b == b'\n' {
            cx = x;
            cy += 8;
            i += 1;
            continue;
        }
        let ch = if b.is_ascii() { b as char } else { '?' };
        draw_char_5x7(cx, cy, ch, color);
        cx += 6;
        i += 1;
    }
}

pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

fn split_rgb(color: u32) -> (u8, u8, u8) {
    (
        ((color >> 16) & 0xFF) as u8,
        ((color >> 8) & 0xFF) as u8,
        (color & 0xFF) as u8,
    )
}
