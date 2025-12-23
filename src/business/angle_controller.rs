//! Angle Controller
//!
//! Business logic for managing an angle value (0-360 degrees).
//! This abstraction separates the business logic from the hardware,
//! allowing the angle to be sourced from different inputs in the future
//! (external sensors, network, etc.).

/// Controller for managing angle values (0-360 degrees)
pub struct AngleController {
    /// Current angle in degrees (0-360)
    angle: u16,
    /// Increment step per update
    increment: u16,
}

impl AngleController {
    /// Create a new AngleController starting at 0 degrees
    ///
    /// # Arguments
    /// * `increment` - The number of degrees to increment on each update
    ///
    /// # Example
    /// ```ignore
    /// let controller = AngleController::new(1); // increment by 1 degree
    /// ```
    pub fn new(increment: u16) -> Self {
        Self {
            angle: 0,
            increment,
        }
    }

    /// Update the angle by incrementing it
    ///
    /// The angle wraps around at 360 degrees back to 0.
    pub fn update(&mut self) {
        self.angle = (self.angle + self.increment) % 360;
    }

    /// Get the current angle value
    pub fn angle(&self) -> u16 {
        self.angle
    }

    /// Set the angle to a specific value
    ///
    /// # Arguments
    /// * `angle` - The new angle value (will be wrapped to 0-359)
    pub fn set_angle(&mut self, angle: u16) {
        self.angle = angle % 360;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_controller() {
        let controller = AngleController::new(1);
        assert_eq!(controller.angle(), 0);
    }

    #[test]
    fn test_update_increments() {
        let mut controller = AngleController::new(5);
        controller.update();
        assert_eq!(controller.angle(), 5);
        controller.update();
        assert_eq!(controller.angle(), 10);
    }

    #[test]
    fn test_wraps_at_360() {
        let mut controller = AngleController::new(10);
        controller.set_angle(355);
        controller.update();
        assert_eq!(controller.angle(), 5);
    }

    #[test]
    fn test_set_angle() {
        let mut controller = AngleController::new(1);
        controller.set_angle(180);
        assert_eq!(controller.angle(), 180);
    }

    #[test]
    fn test_set_angle_wraps() {
        let mut controller = AngleController::new(1);
        controller.set_angle(370);
        assert_eq!(controller.angle(), 10);
    }
}
