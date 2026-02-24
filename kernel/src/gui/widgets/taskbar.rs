use crate::gui::{Rect, Event, Color};
use super::Widget;
use super::Window;
use super::button::Button;

pub struct Taskbar {
    pub rect: Rect,
    pub start_button: Button,
    pub start_menu_open: bool,
}

impl Taskbar {
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        let height = 40;
        let rect = Rect::new(0, (screen_height - height) as i32, screen_width, height);
        
        Self {
            rect,
            start_button: Button::new("START", 5, 5, 80, 30),
            start_menu_open: false,
        }
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
