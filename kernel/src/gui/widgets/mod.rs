use super::window::Window;
use super::{Rect, Event};

pub trait Widget {
    fn draw(&self, window: &mut Window, rect: Rect);
    fn handle_event(&mut self, event: Event) -> bool;
}

pub mod terminal;
pub mod button;
pub mod taskbar;
