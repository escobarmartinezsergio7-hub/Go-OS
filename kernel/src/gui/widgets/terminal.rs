use super::Widget;
use crate::gui::{Rect, Event, Color};
use crate::gui::window::Window;
use crate::ui; // Access to TERMINAL static

pub struct TerminalWidget {
    rect: Rect,
}

impl TerminalWidget {
    pub fn new(rect: Rect) -> Self {
        Self { rect }
    }
}

impl Widget for TerminalWidget {
    fn draw(&self, window: &mut Window, rect: Rect) {
        // Fill background
        window.fill_rect(rect, Color(0x0C121D));

        let start_x = rect.x + 8;
        let mut y = rect.y + 8;

        // Draw Lines
        ui::for_each_line(|bytes| {
             window.draw_text(start_x as u32, y as u32, bytes, Color(0xDAE7FF));
             y += 9;
        });

        // Draw Input
        ui::with_input(|input| {
             window.draw_text(start_x as u32, y as u32, b"> ", Color(0xFFE29A));
             window.draw_text((start_x + 12) as u32, y as u32, input, Color(0xFFE29A));
             
             // Cursor (simple)
             let cursor_x = start_x + 12 + (input.len() as i32 * 6);
             window.fill_rect(Rect::new(cursor_x, y + 7, 5, 1), Color(0xFFE29A));
        });
    }

    fn handle_event(&mut self, event: Event) -> bool {
        // Pass event to ui::terminal_input_char etc.
        match event {
            Event::Keyboard(ke) => {
                 if ke.down {
                     if let Some(ch) = ke.key {
                         ui::terminal_input_char(ch);
                         return true;
                     }
                 }
            }
            _ => {}
        }
        false
    }
}
