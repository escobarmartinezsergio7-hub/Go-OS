pub mod compositor;
pub mod window;
pub mod widgets;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < (self.x + self.width as i32) &&
        p.y >= self.y && p.y < (self.y + self.height as i32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color(pub u32);

impl Color {
    pub const BLACK: Color = Color(0x000000);
    pub const WHITE: Color = Color(0xFFFFFF);
    pub const RED: Color = Color(0xFF0000);
    pub const GREEN: Color = Color(0x00FF00);
    pub const BLUE: Color = Color(0x0000FF);
    
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(((r as u32) << 16) | ((g as u32) << 8) | (b as u32))
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Mouse(MouseEvent),
    Keyboard(KeyboardEvent),
}

#[derive(Clone, Copy, Debug)]
pub struct MouseEvent {
    pub x: i32,
    pub y: i32,
    pub left_down: bool,
    pub right_down: bool,
    pub wheel_delta: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialKey {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
pub struct KeyboardEvent {
    pub key: Option<char>, // Simplified
    pub special: Option<SpecialKey>,
    pub down: bool,
}
