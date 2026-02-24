use crate::framebuffer::{self, rgb};

const TOP_PANEL_H: usize = 64;
const TASKBAR_H: usize = 52;
const INDICATOR_W: usize = 44;
const INDICATOR_H: usize = 28;
const INDICATOR_X_BASE: usize = 60;
const INDICATOR_Y_OFFSET: usize = 12;

pub const TERM_MAX_INPUT: usize = 72;
const TERM_MAX_LINES: usize = 18;
const TERM_MAX_COLS: usize = 72;
const TERM_LINE_HEIGHT: usize = 9;

#[derive(Clone, Copy)]
struct ThemePalette {
    bg_desktop: u32,
    bg_top: u32,
    bg_left_win: u32,
    bg_left_title: u32,
    bg_right_win: u32,
    bg_right_title: u32,
    bg_taskbar: u32,
    fg_text: u32,
    fg_muted: u32,
    fg_input: u32,
}

const THEME_DEFAULT: ThemePalette = ThemePalette {
    bg_desktop: 0x121A28,
    bg_top: 0x0A121E,
    bg_left_win: 0x1A2438,
    bg_left_title: 0x243A5C,
    bg_right_win: 0x221E34,
    bg_right_title: 0x3A2C54,
    bg_taskbar: 0x080E18,
    fg_text: 0xDAE7FF,
    fg_muted: 0x8FA8C8,
    fg_input: 0xFFE29A,
};

const THEME_ALT: ThemePalette = ThemePalette {
    bg_desktop: 0x221D16,
    bg_top: 0x1A140D,
    bg_left_win: 0x3B2C1A,
    bg_left_title: 0x6A4E22,
    bg_right_win: 0x35261B,
    bg_right_title: 0x5D3F2A,
    bg_taskbar: 0x17100A,
    fg_text: 0xFBEAD0,
    fg_muted: 0xC5A981,
    fg_input: 0xFFE29A,
};

#[derive(Clone, Copy)]
struct TerminalState {
    input: [u8; TERM_MAX_INPUT],
    input_len: usize,
    lines: [[u8; TERM_MAX_COLS]; TERM_MAX_LINES],
    line_lens: [usize; TERM_MAX_LINES],
    head: usize,
    count: usize,
}

impl TerminalState {
    const fn new() -> Self {
        Self {
            input: [0; TERM_MAX_INPUT],
            input_len: 0,
            lines: [[0; TERM_MAX_COLS]; TERM_MAX_LINES],
            line_lens: [0; TERM_MAX_LINES],
            head: 0,
            count: 0,
        }
    }
}

static mut TERMINAL: TerminalState = TerminalState::new();

#[inline]
fn palette(alt_theme: bool) -> ThemePalette {
    if alt_theme {
        THEME_ALT
    } else {
        THEME_DEFAULT
    }
}

fn push_line_bytes(bytes: &[u8]) {
    unsafe {
        let idx = TERMINAL.head;
        let n = bytes.len().min(TERM_MAX_COLS);
        let mut i = 0usize;
        while i < n {
            TERMINAL.lines[idx][i] = bytes[i];
            i += 1;
        }
        TERMINAL.line_lens[idx] = n;
        TERMINAL.head = (TERMINAL.head + 1) % TERM_MAX_LINES;
        if TERMINAL.count < TERM_MAX_LINES {
            TERMINAL.count += 1;
        }
    }
}

fn push_line(text: &str) {
    push_line_bytes(text.as_bytes());
}

fn clear_lines_only() {
    unsafe {
        TERMINAL.lines = [[0; TERM_MAX_COLS]; TERM_MAX_LINES];
        TERMINAL.line_lens = [0; TERM_MAX_LINES];
        TERMINAL.head = 0;
        TERMINAL.count = 0;
    }
}

pub fn terminal_reset(irq_mode: bool) {
    unsafe {
        TERMINAL = TerminalState::new();
    }
    push_line("REDUX TERMINAL READY");
    push_line("USER SPACE SHELL: ONLINE");
    if irq_mode {
        push_line("TIMER MODE: IRQ");
    } else {
        push_line("TIMER MODE: POLLING");
    }
}

pub fn terminal_clear_lines() {
    clear_lines_only();
}

pub fn terminal_system_message(msg: &str) {
    push_line(msg);
}

pub fn terminal_system_message_bytes(bytes: &[u8]) {
    push_line_bytes(bytes);
}

pub fn terminal_input_char(ch: char) {
    if !ch.is_ascii() || ch == '\n' || ch == '\r' {
        return;
    }
    unsafe {
        if TERMINAL.input_len < TERM_MAX_INPUT {
            TERMINAL.input[TERMINAL.input_len] = ch as u8;
            TERMINAL.input_len += 1;
        }
    }
}

pub fn terminal_backspace() {
    unsafe {
        if TERMINAL.input_len > 0 {
            TERMINAL.input_len -= 1;
        }
    }
}

pub fn terminal_clear_input() {
    unsafe {
        TERMINAL.input_len = 0;
    }
}

fn trimmed_input_bounds() -> (usize, usize) {
    unsafe {
        let mut start = 0usize;
        while start < TERMINAL.input_len && TERMINAL.input[start] == b' ' {
            start += 1;
        }
        let mut end = TERMINAL.input_len;
        while end > start && TERMINAL.input[end - 1] == b' ' {
            end -= 1;
        }
        (start, end)
    }
}

pub fn terminal_copy_input_trim(out: &mut [u8]) -> usize {
    let (start, end) = trimmed_input_bounds();
    if end <= start || out.is_empty() {
        return 0;
    }

    unsafe {
        let n = (end - start).min(out.len());
        let mut i = 0usize;
        while i < n {
            out[i] = TERMINAL.input[start + i];
            i += 1;
        }
        n
    }
}

fn push_prompt_line() {
    let mut line = [0u8; TERM_MAX_COLS];
    let mut n = 0usize;
    line[n] = b'>';
    n += 1;
    line[n] = b' ';
    n += 1;

    unsafe {
        let copy = TERMINAL.input_len.min(TERM_MAX_COLS.saturating_sub(n));
        let mut i = 0usize;
        while i < copy {
            line[n + i] = TERMINAL.input[i];
            i += 1;
        }
        n += copy;
    }
    push_line_bytes(&line[..n]);
}

pub fn terminal_commit_input_line() {
    let (_, end) = trimmed_input_bounds();
    if end > 0 {
        push_prompt_line();
    }
    terminal_clear_input();
}

fn draw_static_shell(w: usize, h: usize, p: ThemePalette) {
    framebuffer::rect(0, 0, w, h, p.bg_desktop);
    framebuffer::rect(0, 0, w, TOP_PANEL_H, p.bg_top);

    let left_w = (w as f32 * 0.62) as usize;
    let right_w = w.saturating_sub(left_w + 24);

    framebuffer::rect(12, 84, left_w, h.saturating_sub(160), p.bg_left_win);
    framebuffer::rect(12, 84, left_w, 30, p.bg_left_title);

    framebuffer::rect(left_w + 24, 84, right_w, h.saturating_sub(160), p.bg_right_win);
    framebuffer::rect(left_w + 24, 84, right_w, 30, p.bg_right_title);

    framebuffer::rect(0, h.saturating_sub(TASKBAR_H), w, TASKBAR_H, p.bg_taskbar);
}

fn draw_top_section(p: ThemePalette, irq_mode: bool, running: bool) {
    framebuffer::draw_text_5x7(20, 10, "REDUX DESKTOP", p.fg_text);
    if irq_mode {
        framebuffer::draw_text_5x7(20, 24, "TIMER: IRQ", p.fg_muted);
    } else {
        framebuffer::draw_text_5x7(20, 24, "TIMER: POLLING", p.fg_muted);
    }
    if running {
        framebuffer::draw_text_5x7(180, 24, "STATE: RUN", rgb(120, 255, 150));
    } else {
        framebuffer::draw_text_5x7(180, 24, "STATE: PAUSE", rgb(255, 182, 73));
    }
}

fn draw_terminal_panel(w: usize, h: usize, p: ThemePalette, ticks: u64) {
    let left_w = (w as f32 * 0.62) as usize;
    let term_x = 20usize;
    let term_y = 118usize;
    let term_w = left_w.saturating_sub(16);
    let term_h = h.saturating_sub(182);

    framebuffer::rect(term_x, term_y, term_w, term_h, 0x0C121D);
    framebuffer::draw_text_5x7(term_x + 8, term_y + 8, "TERMINAL", p.fg_muted);

    let content_x = term_x + 8;
    let content_y = term_y + 22;
    let max_visible = term_h.saturating_sub(40) / TERM_LINE_HEIGHT;

    unsafe {
        let count = TERMINAL.count;
        let visible = count.min(max_visible);
        let skip = count.saturating_sub(visible);
        let oldest = if count == TERM_MAX_LINES { TERMINAL.head } else { 0 };

        let mut line_no = 0usize;
        while line_no < visible {
            let seq = skip + line_no;
            let idx = (oldest + seq) % TERM_MAX_LINES;
            let len = TERMINAL.line_lens[idx];
            if len > 0 {
                framebuffer::draw_text_5x7_bytes(
                    content_x,
                    content_y + line_no * TERM_LINE_HEIGHT,
                    &TERMINAL.lines[idx][..len],
                    p.fg_text,
                );
            }
            line_no += 1;
        }

        let input_y = term_y + term_h.saturating_sub(16);
        framebuffer::draw_text_5x7(content_x, input_y, "> ", p.fg_input);
        framebuffer::draw_text_5x7_bytes(
            content_x + 12,
            input_y,
            &TERMINAL.input[..TERMINAL.input_len],
            p.fg_input,
        );

        if ((ticks / 8) & 1) == 0 {
            let cursor_x = content_x + 12 + TERMINAL.input_len * 6;
            framebuffer::rect(cursor_x, input_y + 7, 5, 1, p.fg_input);
        }
    }
}

fn draw_right_panel(w: usize, _h: usize, p: ThemePalette) {
    let left_w = (w as f32 * 0.62) as usize;
    let x = left_w + 34;
    let y = 120usize;
    framebuffer::draw_text_5x7(x, y, "INPUT", p.fg_muted);
    framebuffer::draw_text_5x7(x, y + 16, "KEYBOARD: TYPE + ENTER", p.fg_text);
    framebuffer::draw_text_5x7(x, y + 28, "SHELL: HELP STATUS PS", p.fg_text);
    framebuffer::draw_text_5x7(x, y + 40, "F1 PAUSE  F2 THEME", p.fg_text);
    framebuffer::draw_text_5x7(x, y + 52, "ESC REBOOT", p.fg_text);
}

fn draw_metrics(
    w: usize,
    h: usize,
    ticks: u64,
    dispatches: u64,
    irq_count: u64,
    mem_mib: u64,
    p: ThemePalette,
) {
    let slot_y = 40;
    let slot_h = 20;
    let slot_w = (w / 3).saturating_sub(40).max(120);
    let x_left = 24;
    let x_mid = w / 3;
    let x_right = (w * 2) / 3;

    framebuffer::rect(x_left, slot_y, slot_w, slot_h, p.bg_top);
    framebuffer::rect(x_mid, slot_y, slot_w, slot_h, p.bg_top);
    framebuffer::rect(x_right, slot_y, slot_w, slot_h, p.bg_top);
    framebuffer::draw_u64_digits(x_left, 42, 1, ticks, rgb(80, 220, 255));
    framebuffer::draw_u64_digits(x_mid, 42, 1, dispatches, rgb(120, 255, 150));
    framebuffer::draw_u64_digits(x_right, 42, 1, irq_count, rgb(255, 200, 90));

    framebuffer::rect(20, h.saturating_sub(44), (w / 4).max(120), 36, p.bg_taskbar);
    framebuffer::draw_u64_digits(20, h.saturating_sub(40), 2, mem_mib, rgb(255, 170, 120));
}

fn draw_activity_indicator(w: usize, h: usize, ticks: u64, running: bool) {
    let travel = w.saturating_sub(120);
    let x = INDICATOR_X_BASE + ((ticks as usize) % travel.max(1));
    let y = h.saturating_sub(TASKBAR_H) + INDICATOR_Y_OFFSET;
    let indicator = if running {
        rgb(64, 166, 255)
    } else {
        rgb(255, 182, 73)
    };
    framebuffer::rect(x, y, INDICATOR_W, INDICATOR_H, indicator);
}

pub fn draw_desktop(
    ticks: u64,
    mem_mib: u64,
    dispatches: u64,
    irq_count: u64,
    alt_theme: bool,
    running: bool,
    irq_mode: bool,
) {
    let (w, h) = framebuffer::dimensions();
    if w == 0 || h == 0 {
        return;
    }

    let p = palette(alt_theme);
    draw_static_shell(w, h, p);
    draw_top_section(p, irq_mode, running);
    draw_terminal_panel(w, h, p, ticks);
    draw_right_panel(w, h, p);
    draw_metrics(w, h, ticks, dispatches, irq_count, mem_mib, p);
    draw_activity_indicator(w, h, ticks, running);
}
pub fn for_each_line<F>(mut f: F)
where
    F: FnMut(&[u8]),
{
    unsafe {
        let count = TERMINAL.count;
        let visible = count.min(TERM_MAX_LINES);
        let skip = count.saturating_sub(visible);
        let oldest = if count == TERM_MAX_LINES { TERMINAL.head } else { 0 };

        let mut line_no = 0usize;
        while line_no < visible {
            let seq = skip + line_no;
            let idx = (oldest + seq) % TERM_MAX_LINES;
            let len = TERMINAL.line_lens[idx];
            f(&TERMINAL.lines[idx][..len]);
            line_no += 1;
        }
    }
}

pub fn with_input<F>(f: F)
where
    F: FnOnce(&[u8]),
{
    unsafe {
        f(&TERMINAL.input[..TERMINAL.input_len]);
    }
}
