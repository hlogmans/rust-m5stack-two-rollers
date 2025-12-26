//! MotorView - Renders a single motor using embedded-graphics
//!
//! Framework-specific: uses embedded-graphics + embedded-layout

use crate::ui::view_models::MotorViewModel;
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle},
    text::Text,
};
use micromath::F32Ext;

/// Visual representation of a motor (framework-specific)
pub struct MotorView {
    /// Center X position for this motor's display
    pub center_x: i32,
    /// Center Y position for this motor's display
    pub center_y: i32,
    /// Color for the angle indicator
    pub color: Rgb565,

    /// Previous angle rendered (for optimization)
    previous_angle: Option<u16>,
}

impl MotorView {
    /// Create a new motor view at the specified position
    pub fn new(center_x: i32, center_y: i32, color: Rgb565) -> Self {
        Self {
            center_x,
            center_y,
            color,
            previous_angle: None,
        }
    }

    /// Render the motor view model to a drawable target
    pub fn render<D>(
        &mut self,
        target: &mut D,
        view_model: &MotorViewModel,
        initial: bool,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        let label_y = self.center_y - 80;
        if initial {
            let label_style = MonoTextStyle::new(&FONT_10X20, Rgb565::CSS_GRAY);

            // Draw motor name label

            Text::new(
                view_model.name,
                Point::new(self.center_x - 30, label_y),
                label_style,
            )
            .draw(target)?;
        }

        // Erase previous indicator by redrawing in background color
        if let Some(old_angle) = self.previous_angle {
            let text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::BLACK);
            let angle_text = view_model.angle_text(old_angle);
            Text::new(
                angle_text.as_str(),
                Point::new(self.center_x - 25, label_y + 25),
                text_style,
            )
            .draw(target)?;
                    // Draw circular angle indicator
            self.draw_angle_indicator(target, old_angle, initial, true)?; // remove
        }

        // Draw angle value
        let text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
        let angle_text = view_model.angle_text(view_model.angle);
        Text::new(
            angle_text.as_str(),
            Point::new(self.center_x - 25, label_y + 25),
            text_style,
        )
        .draw(target)?;



        // Draw circular angle indicator
        self.draw_angle_indicator(target, view_model.angle, initial, false)?; // draw

        self.previous_angle = Some(view_model.angle);
        Ok(())
    }

    /// Draw a circular angle indicator with position marker
    fn draw_angle_indicator<D>(
        &self,
        target: &mut D,
        angle: u16,
        initial: bool,
        remove: bool
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        if initial {
            // Outer circle (track)
            Circle::new(Point::new(self.center_x - 30, self.center_y - 30), 60)
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_DIM_GRAY, 2))
                .draw(target)?;
        }

        // Position marker based on angle (0° = right, clockwise)
        let radians = ((angle as f32 - 90.0) * core::f32::consts::PI) / 180.0;
        let marker_x = self.center_x + (25.0 * radians.cos()) as i32;
        let marker_y = self.center_y + (25.0 * radians.sin()) as i32;

        let color_to_use = if remove {
            Rgb565::BLACK
        } else {
            self.color
        };

        Circle::new(Point::new(marker_x - 5, marker_y - 5), 10)
            .into_styled(PrimitiveStyle::with_fill(color_to_use))
            .draw(target)?;

        Ok(())
    }
}
