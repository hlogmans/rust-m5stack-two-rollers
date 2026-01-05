//! Splash screen root: orchestrates the splash view lifecycle

use embedded_graphics::prelude::DrawTarget;

use crate::hardware::CoreS3Display;
use crate::ui::buttons::ButtonSpec;
use crate::ui::screen_trait::{ScreenController, ScreenEvent};
use crate::ui::views::SplashView;

pub struct SplashScreen {
    view: SplashView,
}

impl SplashScreen {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            view: SplashView::new(width, height),
        }
    }
}

impl ScreenController for SplashScreen {
    type Driver = CoreS3Display<'static>;

    fn open(
        &mut self,
        display: &mut Self::Driver,
    ) -> Result<&[ButtonSpec], <Self::Driver as DrawTarget>::Error> {
        self.view.init(display)?;
        self.view.update_countdown(display, self.view.countdown())?;
        Ok(self.view.buttons())
    }

    fn update(
        &mut self,
        display: &mut Self::Driver,
        event: ScreenEvent,
    ) -> Result<(), <Self::Driver as DrawTarget>::Error> {
        if let ScreenEvent::Countdown(value) = event {
            self.view.set_countdown(value);
            self.view.update_countdown(display, value)?;
        }
        Ok(())
    }

    fn close(
        &mut self,
        _display: &mut Self::Driver,
    ) -> Result<(), <Self::Driver as DrawTarget>::Error> {
        Ok(())
    }

    fn buttons(&self) -> &[ButtonSpec] {
        self.view.buttons()
    }
}
