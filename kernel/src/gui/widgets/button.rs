use alloc::string::String;
use crate::gui::{Rect, Point, Event, Color};
use super::Widget;
use super::Window;

pub struct Button {
    pub text: String,
    pub rect: Rect,
    pub bg_color: Color,
    pub text_color: Color,
    pub hover: bool,
    pub pressed: bool,
}

impl Button {
    pub fn new(text: &str, x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            text: String::from(text),
            rect: Rect::new(x, y, width, height),
            bg_color: Color(0x333333),
            text_color: Color(0xFFFFFF),
            hover: false,
            pressed: false,
        }
    }
}

impl Widget for Button {
    fn draw(&self, window: &mut Window, rect: Rect) {
        let mut color = self.bg_color;
        if self.pressed {
            color = Color(0x111111);
        } else if self.hover {
            color = Color(0x555555);
        }

        // Draw Background
        for y in 0..rect.height {
            for x in 0..rect.width {
                window.draw_pixel((rect.x + x as i32) as u32, (rect.y + y as i32) as u32, color);
            }
        }

        // Draw Border
        let border_color = if self.hover { Color(0x00AAFF) } else { Color(0x777777) };
        for x in 0..rect.width {
            window.draw_pixel((rect.x + x as i32) as u32, rect.y as u32, border_color);
            window.draw_pixel((rect.x + x as i32) as u32, (rect.y + rect.height as i32 - 1) as u32, border_color);
        }
        for y in 0..rect.height {
            window.draw_pixel(rect.x as u32, (rect.y + y as i32) as u32, border_color);
            window.draw_pixel((rect.x + rect.width as i32 - 1) as u32, (rect.y + y as i32) as u32, border_color);
        }

        // Draw Text (Centered)
        let text_bytes = self.text.as_bytes();
        let text_width = text_bytes.len() as u32 * 6;
        let text_height = 8;
        
        let tx = rect.x + (rect.width as i32 - text_width as i32) / 2;
        let ty = rect.y + (rect.height as i32 - text_height as i32) / 2;
        
        window.draw_text(tx as u32, ty as u32, text_bytes, self.text_color);
    }

    fn handle_event(&mut self, event: Event) -> bool {
        match event {
            Event::Mouse(m) => {
                let p = Point { x: m.x, y: m.y };
                let was_hover = self.hover;
                self.hover = self.rect.contains(p);
                
                if self.hover {
                    if m.left_down {
                        self.pressed = true;
                    } else {
                        if self.pressed {
                            // Clicked!
                            self.pressed = false;
                            return true; // Event handled (and action would happen)
                        }
                    }
                } else {
                    self.pressed = false;
                }
                
                return was_hover != self.hover || self.pressed;
            }
            _ => false,
        }
    }
}
