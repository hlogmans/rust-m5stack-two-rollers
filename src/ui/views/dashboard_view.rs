//! DashboardView - Main UI rendering
//!
//! Framework-specific: uses embedded-graphics + embedded-layout

use super::MotorView;
use crate::ui::view_models::DashboardViewModel;
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Rectangle, PrimitiveStyle},
    text::{Alignment, Text},
};

/// Main dashboard view (framework-specific rendering)
pub struct DashboardView {
    /// Display bounds
    bounds: Rectangle,
    /// Motor A rendering view
    motor_a_view: MotorView,
    /// Motor B rendering view
    motor_b_view: MotorView,
}

impl DashboardView {
    /// Create a new dashboard view for the given display size
    pub fn new(width: u32, height: u32) -> Self {
        let bounds = Rectangle::new(Point::zero(), Size::new(width, height));

        // Position motor views side-by-side
        let motor_a_view = MotorView::new(80, 160, Rgb565::CSS_LIGHT_BLUE);
        let motor_b_view = MotorView::new(240, 160, Rgb565::CSS_LIGHT_CORAL);

        Self {
            bounds,
            motor_a_view,
            motor_b_view,
        }
    }

    /// Initialize the dashboard (draw static elements)
    pub fn init<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        // Clear background
        target.fill_solid(&self.bounds, Rgb565::BLACK)?;

        let style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

        // Draw title
        Text::with_alignment(
            "M5Stack CoreS3",
            Point::new(160, 20),
            style,
            Alignment::Center,
        )
        .draw(target)?;

        // Draw subtitle
        Text::with_alignment(
            "Dual Motor Control",
            Point::new(160, 40),
            style,
            Alignment::Center,
        )
        .draw(target)?;

        Ok(())
    }

    /// Update the dashboard with new view model state
    pub fn update<D>(
        &mut self,
        target: &mut D,
        view_model: &DashboardViewModel,
        initial: bool,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        if initial {
            // Clear content area (keep header)
            let content_area = Rectangle::new(Point::new(0, 60), Size::new(320, 180));
            target.fill_solid(&content_area, Rgb565::BLACK)?;
        }

        // Render motor A
        self.motor_a_view.render(target, &view_model.motor_a, initial)?;

        // Render motor B
        self.motor_b_view.render(target, &view_model.motor_b, initial)?;

        // Draw reset buttons only on initial render (after motors so they're on top)
        if initial {
            let btn_style = PrimitiveStyle::with_fill(Rgb565::CSS_BLUE);
            let btn_style_border = PrimitiveStyle::with_stroke(Rgb565::CSS_YELLOW, 2);
            let btn_text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

            let btn_a = Rectangle::new(Point::new(20, 200), Size::new(120, 35));
            btn_a.into_styled(btn_style).draw(target)?;
            btn_a.into_styled(btn_style_border).draw(target)?;
            Text::with_alignment(
                "ZERO A",
                Point::new(80, 222),
                btn_text_style,
                Alignment::Center,
            )
            .draw(target)?;

            let btn_b = Rectangle::new(Point::new(180, 200), Size::new(120, 35));
            btn_b.into_styled(btn_style).draw(target)?;
            btn_b.into_styled(btn_style_border).draw(target)?;
            Text::with_alignment(
                "ZERO B",
                Point::new(240, 222),
                btn_text_style,
                Alignment::Center,
            )
            .draw(target)?;
        }

        Ok(())
    }
}
