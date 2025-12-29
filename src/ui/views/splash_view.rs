//! SplashView - Startup screen with countdown
//!
//! Framework-specific: uses embedded-graphics

use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::{FONT_10X20, FONT_6X10}},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};

use crate::ui::buttons::ButtonSpec;
use crate::ui::screen_trait::{ScreenController, ScreenEvent};
use crate::hardware::CoreS3Display;

/// Splash screen view (framework-specific rendering)
pub struct SplashView {
    /// Display bounds
    bounds: Rectangle,
    /// Current countdown value
    countdown: u8,
}

impl SplashView {
    /// Create a new splash view for the given display size
    pub fn new(width: u32, height: u32) -> Self {
        let bounds = Rectangle::new(Point::zero(), Size::new(width, height));
        Self { bounds, countdown: 4 }
    }

    /// Initialize the splash screen (draw static elements)
    fn init<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        // Clear background to dark blue
        target.fill_solid(&self.bounds, Rgb565::new(0, 0, 8))?;

        // Draw title
        let title_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
        Text::with_alignment(
            "M5Stack CoreS3",
            Point::new(160, 60),
            title_style,
            Alignment::Center,
        )
        .draw(target)?;

        // Draw subtitle
        let subtitle_style = MonoTextStyle::new(&FONT_6X10, Rgb565::CSS_LIGHT_GRAY);
        Text::with_alignment(
            "Dual Motor Controller",
            Point::new(160, 80),
            subtitle_style,
            Alignment::Center,
        )
        .draw(target)?;

        // Draw version info
        Text::with_alignment(
            "v1.0.0",
            Point::new(160, 95),
            subtitle_style,
            Alignment::Center,
        )
        .draw(target)?;

        // Draw decorative circle
        Circle::new(Point::new(135, 110), 50)
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_DODGER_BLUE, 3))
            .draw(target)?;

        Ok(())
    }

    /// Update the countdown timer
    fn update_countdown<D>(&self, target: &mut D, seconds_left: u8) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        // Clear countdown area
        let clear_area = Rectangle::new(Point::new(100, 170), Size::new(120, 30));
        target.fill_solid(&clear_area, Rgb565::new(0, 0, 8))?;

        // Draw countdown text
        let mut buffer = heapless::String::<32>::new();
        use core::fmt::Write;
        let _ = write!(&mut buffer, "Starting in {}...", seconds_left);

        let countdown_style = MonoTextStyle::new(&FONT_10X20, Rgb565::CSS_YELLOW);
        Text::with_alignment(
            &buffer,
            Point::new(160, 185),
            countdown_style,
            Alignment::Center,
        )
        .draw(target)?;

        Ok(())
    }
}

impl ScreenController for SplashView {
    type Driver = CoreS3Display<'static>;

    fn open(&mut self, display: &mut Self::Driver) -> Result<&[ButtonSpec], <Self::Driver as DrawTarget>::Error> {
        self.init(display)?;
        self.update_countdown(display, self.countdown)?;
        Ok(&[])
    }

    fn update(&mut self, display: &mut Self::Driver, event: ScreenEvent) -> Result<(), <Self::Driver as DrawTarget>::Error> {
        match event {
            ScreenEvent::Countdown(value) => {
                self.countdown = value;
                self.update_countdown(display, value)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn close(&mut self, _display: &mut Self::Driver) -> Result<(), <Self::Driver as DrawTarget>::Error> {
        Ok(())
    }

    fn buttons(&self) -> &[ButtonSpec] {
        &[]
    }
}

