//! UI Button specifications
//!
//! Views define their buttons here as `ButtonSpec`s with geometry.

/// Logical button identifiers used by business logic
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ButtonId {
    ZeroA,
    ZeroB,
}

/// A rectangular button on screen
#[derive(Copy, Clone, Debug)]
pub struct ButtonSpec {
    pub id: ButtonId,
    pub x1: u16,
    pub y1: u16,
    pub x2: u16,
    pub y2: u16,
}

impl ButtonSpec {
    pub const fn rect(id: ButtonId, x: u16, y: u16, w: u16, h: u16) -> Self {
        Self { id, x1: x, y1: y, x2: x + w, y2: y + h }
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x1 && x <= self.x2 && y >= self.y1 && y <= self.y2
    }
}

/// Buttons for the dashboard screen
pub fn dashboard_buttons() -> [ButtonSpec; 2] {
    [
        // ZERO A button: Rectangle::new(Point::new(20, 200), Size::new(120, 35))
        ButtonSpec::rect(ButtonId::ZeroA, 20, 200, 120, 35),
        // ZERO B button: Rectangle::new(Point::new(180, 200), Size::new(120, 35))
        ButtonSpec::rect(ButtonId::ZeroB, 180, 200, 120, 35),
    ]
}

/// No buttons on splash screen at the moment
pub fn splash_buttons() -> [ButtonSpec; 0] { [] }
