//! Dashboard screen root: couples view + view model + buttons

use embedded_graphics::prelude::DrawTarget;

use crate::hardware::CoreS3Display;
use crate::ui::buttons::ButtonSpec;
use crate::ui::screen_trait::{ScreenController, ScreenEvent};
use crate::ui::view_models::DashboardViewModel;
use crate::ui::views::DashboardView;

pub struct DashboardScreen {
    view: DashboardView,
    view_model: DashboardViewModel,
}

impl DashboardScreen {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            view: DashboardView::new(width, height),
            view_model: DashboardViewModel::new(),
        }
    }
}

impl ScreenController for DashboardScreen {
    type Driver = CoreS3Display<'static>;

    fn open(
        &mut self,
        display: &mut Self::Driver,
    ) -> Result<&[ButtonSpec], <Self::Driver as DrawTarget>::Error> {
        self.view.init(display, &self.view_model)?;
        Ok(self.view.buttons())
    }

    fn update(
        &mut self,
        display: &mut Self::Driver,
        event: ScreenEvent,
    ) -> Result<(), <Self::Driver as DrawTarget>::Error> {
        match event {
            ScreenEvent::DashboardModel(vm) => {
                self.view_model = vm;
                self.view.update(display, &self.view_model, false)?;
            }
            ScreenEvent::AngleA(angle) => {
                self.view_model.update_motor_a_angle(angle);
                self.view.update(display, &self.view_model, false)?;
            }
            ScreenEvent::AngleB(angle) => {
                self.view_model.update_motor_b_angle(angle);
                self.view.update(display, &self.view_model, false)?;
            }
            ScreenEvent::Countdown(_) => {}
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
