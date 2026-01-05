//! MotorView - Renders a single motor using embedded-graphics
//!
//! Framework-specific: uses embedded-graphics + embedded-layout

use crate::ui::view_models::MotorViewModel;
use embedded_graphics::{
    mono_font::{MonoTextStyle, MonoTextStyleBuilder, ascii::FONT_10X20},
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

        // Redraw marker in a single batched iterator (erase old + draw new) to avoid flicker
        self.draw_angle_indicator(target, self.previous_angle, view_model.angle, initial)?;

        // Draw angle value with background set to avoid flicker (single draw)
        let text_style: MonoTextStyle<'_, Rgb565> = MonoTextStyleBuilder::new()
            .font(&FONT_10X20)
            .text_color(Rgb565::WHITE)
            .background_color(Rgb565::BLACK)
            .build();
        let angle_text = view_model.angle_text(view_model.angle);
        Text::new(
            angle_text.as_str(),
            Point::new(self.center_x - 25, label_y + 25),
            text_style,
        )
        .draw(target)?;

        self.previous_angle = Some(view_model.angle);
        Ok(())
    }

    /// Draw a circular angle indicator with position marker
    fn draw_angle_indicator<D>(
        &self,
        target: &mut D,
        previous_angle: Option<u16>,
        new_angle: u16,
        initial: bool,
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

        let marker_pixels = |angle: u16, color: Rgb565| {
            let radians = ((angle as f32 - 90.0) * core::f32::consts::PI) / 180.0;
            let marker_x = self.center_x + (25.0 * radians.cos()) as i32;
            let marker_y = self.center_y + (25.0 * radians.sin()) as i32;
            Circle::new(Point::new(marker_x - 5, marker_y - 5), 10)
                .into_styled(PrimitiveStyle::with_fill(color))
                .pixels()
        };

        match previous_angle {
            Some(old) => {
                // Batch erase (bg) + draw (fg) in one iterator to minimize visible flicker
                target.draw_iter(
                    marker_pixels(old, Rgb565::BLACK)
                        .chain(marker_pixels(new_angle, self.color)),
                )?;
            }
            None => {
                target.draw_iter(marker_pixels(new_angle, self.color))?;
            }
        }

        Ok(())
    }
}
