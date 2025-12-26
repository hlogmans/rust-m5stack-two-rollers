//! MotorViewModel - Presentation state for a single motor
//!
//! Framework-agnostic: can be used with any UI framework

/// ViewModel for a single motor's display state
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotorViewModel {
    /// Motor identifier (e.g., "A", "B")
    pub name: &'static str,
    /// Current angle in degrees (0-360)
    pub angle: u16,
    /// Current speed in RPM (for future extension)
    pub speed: f32,
    /// Motor enabled state
    pub enabled: bool,
}

impl MotorViewModel {
    /// Create a new motor view model
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            angle: 0,
            speed: 0.0,
            enabled: true,
        }
    }

    /// Update angle value
    pub fn update_angle(&mut self, angle: u16) {
        self.angle = angle % 360;
    }

    /// Update speed value
    pub fn update_speed(&mut self, speed: f32) {
        self.speed = speed;
    }

    /// Get formatted angle string (e.g., "180°")
    pub fn angle_text(&self, value: u16) -> heapless::String<16> {
        use core::fmt::Write;
        let mut s = heapless::String::new();
        let _ = write!(s, "{:3}°", value);
        s
    }

    /// Get formatted speed string (e.g., "12.5 RPM")
    pub fn speed_text(&self) -> heapless::String<16> {
        use core::fmt::Write;
        let mut s = heapless::String::new();
        let _ = write!(s, "{:.1} RPM", self.speed);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_angle_wrapping() {
        let mut vm = MotorViewModel::new("A");
        vm.update_angle(370);
        assert_eq!(vm.angle, 10);
    }

    #[test]
    fn test_angle_text_formatting() {
        let mut vm = MotorViewModel::new("A");
        vm.update_angle(45);
        assert_eq!(vm.angle_text(45).as_str(), " 45°");
    }
}
