use crate::gui::{Rect, Event, Color};
use super::Widget;
use super::Window;
use super::button::Button;
use alloc::string::String;
use alloc::vec::Vec;

/// Maximum number of pinned items allowed.
const MAX_PINNED: usize = 32;
/// How many pinned icons are visible without scrolling.
pub const PINNED_VISIBLE: usize = 6;
/// Size of each pinned icon square (px).
pub const PINNED_ICON_SIZE: i32 = 34;
/// Gap between pinned icons.
pub const PINNED_GAP: i32 = 2;
/// Width of scroll arrow buttons.
pub const PINNED_ARROW_W: i32 = 16;
/// Width of the settings gear icon area.
pub const SETTINGS_ICON_W: i32 = 28;
/// Width of the clock area.
pub const CLOCK_W: i32 = 80;
/// Total right-side area (arrows + 6 icons + settings + clock + padding).
pub const TRAY_TOTAL_W: i32 =
    PINNED_ARROW_W + PINNED_VISIBLE as i32 * (PINNED_ICON_SIZE + PINNED_GAP) + PINNED_ARROW_W
    + 4 + SETTINGS_ICON_W + 4 + CLOCK_W + 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PinnedItemKind {
    File,
    Directory,
    App,
    Audio,
    Image,
    Archive,
    Executable,
}

#[derive(Clone)]
pub struct PinnedItem {
    pub label: String,
    pub kind: PinnedItemKind,
    pub cluster: u32,
    pub size: u32,
    /// For apps launched from Start menu: the command string.
    pub app_command: Option<String>,
    /// FAT32 device index — needed for USB vs internal partition.
    pub device_index: Option<usize>,
}

pub struct Taskbar {
    pub rect: Rect,
    pub start_button: Button,
    pub start_menu_open: bool,
    pub pinned_items: Vec<PinnedItem>,
    pub pinned_scroll: usize,
    /// Index of pinned item currently hovered (-1 = none).
    pub pinned_hover_index: i32,
}

impl Taskbar {
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        let height = 40;
        let rect = Rect::new(0, (screen_height - height) as i32, screen_width, height);

        Self {
            rect,
            start_button: Button::new("START", 5, 5, 80, 30),
            start_menu_open: false,
            pinned_items: Vec::new(),
            pinned_scroll: 0,
            pinned_hover_index: -1,
        }
    }

    /// Pin an item to the taskbar. Returns true if added.
    pub fn pin_item(&mut self, item: PinnedItem) -> bool {
        if self.pinned_items.len() >= MAX_PINNED {
            return false;
        }
        // Avoid duplicates by label + device
        if self.pinned_items.iter().any(|p| p.label == item.label && p.device_index == item.device_index) {
            return false;
        }
        self.pinned_items.push(item);
        true
    }

    /// Unpin an item by index.
    pub fn unpin_item(&mut self, index: usize) {
        if index < self.pinned_items.len() {
            self.pinned_items.remove(index);
            if self.pinned_scroll > 0 && self.pinned_scroll >= self.pinned_items.len() {
                self.pinned_scroll = self.pinned_items.len().saturating_sub(1);
            }
        }
    }

    /// X position where the tray area starts (right-aligned).
    pub fn tray_start_x(&self) -> i32 {
        (self.rect.width as i32 - TRAY_TOTAL_W).max(90)
    }

    /// Whether there are more items to scroll right.
    pub fn can_scroll_right(&self) -> bool {
        self.pinned_scroll + PINNED_VISIBLE < self.pinned_items.len()
    }

    /// Whether there are items to scroll left.
    pub fn can_scroll_left(&self) -> bool {
        self.pinned_scroll > 0
    }

    pub fn scroll_left(&mut self) {
        if self.pinned_scroll > 0 {
            self.pinned_scroll -= 1;
        }
    }

    pub fn scroll_right(&mut self) {
        if self.can_scroll_right() {
            self.pinned_scroll += 1;
        }
    }

    /// Returns the maximum number of minimized tabs that fit before the tray area.
    pub fn max_minimized_tabs(&self) -> usize {
        let tabs_start = 90i32;
        let available = (self.tray_start_x() - tabs_start - 8).max(0);
        (available / 120) as usize
    }
}

impl Widget for Taskbar {
    fn draw(&self, window: &mut Window, _rect: Rect) {
        // Draw Taskbar Background
        let bg_color = Color(0x222222);
        window.fill_rect(Rect::new(0, 0, self.rect.width, self.rect.height), bg_color);

        // Draw Top Border
        let border_color = Color(0x444444);
        window.fill_rect(Rect::new(0, 0, self.rect.width, 1), border_color);

        // Draw Start Button (relative to taskbar)
        let mut btn_rect = self.start_button.rect;
        btn_rect.y = 5;
        self.start_button.draw(window, btn_rect);
    }

    fn handle_event(&mut self, event: Event) -> bool {
        self.start_button.handle_event(event)
    }
}
