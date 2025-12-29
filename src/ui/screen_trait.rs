//! Screen lifecycle trait
//!
//! Every screen must implement this trait to participate in the display service lifecycle.

use embedded_graphics::{pixelcolor::Rgb565, prelude::DrawTarget};
use crate::ui::buttons::ButtonSpec;

/// Events that can trigger screen updates
#[derive(Copy, Clone, Debug)]
pub enum ScreenEvent {
    /// Motor A angle changed
    AngleA(u16),
    /// Motor B angle changed
    AngleB(u16),
    /// Countdown timer changed
    Countdown(u8),
}

/// Screen lifecycle trait - all screens must implement this
pub trait ScreenController {
    /// Concrete display driver type for this screen
    type Driver: DrawTarget<Color = Rgb565>;

    /// Called when the screen becomes active
    /// Returns the list of buttons this screen uses (empty slice if none)
    fn open(&mut self, display: &mut Self::Driver) -> Result<&[ButtonSpec], <Self::Driver as DrawTarget>::Error>;

    /// Called when the screen should update in response to an event
    fn update(&mut self, display: &mut Self::Driver, event: ScreenEvent) -> Result<(), <Self::Driver as DrawTarget>::Error>;

    /// Called when the screen is being closed (before navigation to another screen)
    fn close(&mut self, display: &mut Self::Driver) -> Result<(), <Self::Driver as DrawTarget>::Error>;

    /// Get the current button layout (for re-registration if needed)
    fn buttons(&self) -> &[ButtonSpec];
}
