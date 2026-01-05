//! DashboardViewModel - Overall application UI state
//!
//! Framework-agnostic: holds state for the entire dashboard

use super::MotorViewModel;

/// ViewModel for the main dashboard showing both motors
#[derive(Debug, Clone, Copy)]
pub struct DashboardViewModel {
    /// Motor A state
    pub motor_a: MotorViewModel,
    /// Motor B state
    pub motor_b: MotorViewModel,
    /// Application title
    pub title: &'static str,
    /// Subtitle
    pub subtitle: &'static str,
}

impl DashboardViewModel {
    /// Create a new dashboard view model
    pub fn new() -> Self {
        Self {
            motor_a: MotorViewModel::new("Motor A"),
            motor_b: MotorViewModel::new("Motor B"),
            title: "M5Stack CoreS3",
            subtitle: "Dual Motor Control",
        }
    }

    /// Update motor A angle
    pub fn update_motor_a_angle(&mut self, angle: u16) {
        self.motor_a.update_angle(angle);
    }

    /// Update motor B angle
    pub fn update_motor_b_angle(&mut self, angle: u16) {
        self.motor_b.update_angle(angle);
    }

    /// Update motor A speed
    pub fn update_motor_a_speed(&mut self, speed: f32) {
        self.motor_a.update_speed(speed);
    }

    /// Update motor B speed
    pub fn update_motor_b_speed(&mut self, speed: f32) {
        self.motor_b.update_speed(speed);
    }
}

impl Default for DashboardViewModel {
    fn default() -> Self {
        Self::new()
    }
}
