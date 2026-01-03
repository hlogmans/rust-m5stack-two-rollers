//! Display Service - manages screen lifecycle and navigation
//!
//! This service:
//! - Owns the display hardware
//! - Manages the active screen and handles navigation
//! - Automatically registers/unregisters buttons when screens change
//! - Routes events to the active screen

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::Watch;
use embassy_futures::select;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::DrawTarget;

use crate::business::input;
use crate::ui::screen::Screen;
use crate::ui::{ScreenController, ScreenEvent};
use crate::ui::views::{SplashView, DashboardView};
use crate::ui::view_models::DashboardViewModel;
use crate::hardware::CoreS3Display;
use crate::info;

/// Display wrapper with MVVM architecture support
///
/// Combines hardware driver with view model management for easy screen updates.
pub struct Display<D: DrawTarget<Color = Rgb565>> {
    driver: D,
    view_model: DashboardViewModel,
    view: DashboardView,
}

impl<D: DrawTarget<Color = Rgb565>> Display<D> {
    /// Create a new display wrapper with MVVM support
    pub fn new(driver: D, width: u32, height: u32) -> Self {
        let view_model = DashboardViewModel::new();
        let view = DashboardView::new(width, height);
        Self {
            driver,
            view_model,
            view,
        }
    }

    /// Get a mutable reference to the underlying driver for custom rendering
    pub fn driver_mut(&mut self) -> &mut D {
        &mut self.driver
    }

    /// Initialize UI (render static elements)
    pub fn init_angle_display(&mut self) {
        info!("Initializing dashboard UI (MVVM)");
        let _ = self.view.init(&mut self.driver);

        let _ = self.view.update(&mut self.driver, &self.view_model, true);
    }

    /// Update display with dual motor angles (MVVM pattern)
    pub fn update_dual_angles(&mut self, angle_a: u16, angle_b: u16) {
        // Update ViewModel (business/presentation state)
        self.view_model.update_motor_a_angle(angle_a);
        self.view_model.update_motor_b_angle(angle_b);

        // Render ViewModel to screen (View layer)
        let _ = self.view.update(&mut self.driver, &self.view_model, false);
    }

    /// Get mutable reference to view model (for future extensions)
    pub fn view_model_mut(&mut self) -> &mut DashboardViewModel {
        &mut self.view_model
    }
}

/// Active screen wrapper - enum of all possible screens
enum ActiveScreen {
    Splash(SplashView),
    Dashboard(DashboardView),
}

impl ScreenController for ActiveScreen {
    type Driver = CoreS3Display<'static>;

    fn open(&mut self, display: &mut Self::Driver) -> Result<&[crate::ui::buttons::ButtonSpec], <Self::Driver as embedded_graphics::prelude::DrawTarget>::Error> {
        match self {
            ActiveScreen::Splash(view) => view.open(display),
            ActiveScreen::Dashboard(view) => view.open(display),
        }
    }

    fn update(&mut self, display: &mut Self::Driver, event: ScreenEvent) -> Result<(), <Self::Driver as embedded_graphics::prelude::DrawTarget>::Error> {
        match self {
            ActiveScreen::Splash(view) => ScreenController::update(view, display, event),
            ActiveScreen::Dashboard(view) => ScreenController::update(view, display, event),
        }
    }

    fn close(&mut self, display: &mut Self::Driver) -> Result<(), <Self::Driver as embedded_graphics::prelude::DrawTarget>::Error> {
        match self {
            ActiveScreen::Splash(view) => view.close(display),
            ActiveScreen::Dashboard(view) => view.close(display),
        }
    }

    fn buttons(&self) -> &[crate::ui::buttons::ButtonSpec] {
        match self {
            ActiveScreen::Splash(view) => view.buttons(),
            ActiveScreen::Dashboard(view) => view.buttons(),
        }
    }
}

/// The DisplayService manages screen lifecycle and rendering
pub struct DisplayService {
    display: Display<CoreS3Display<'static>>,
    splash: SplashView,
    dashboard: DashboardView,
}

impl DisplayService {
    /// Create a new display service
    pub fn new(display: Display<CoreS3Display<'static>>) -> Self {
        Self {
            display,
            splash: SplashView::new(320, 240),
            dashboard: DashboardView::new(320, 240),
        }
    }

    /// Run the display service task
    pub async fn run(
        mut self,
        screen_watch: &'static Watch<CriticalSectionRawMutex, Screen, 4>,
        countdown_watch: &'static Watch<CriticalSectionRawMutex, u8, 4>,
        angle_a_watch: &'static Watch<CriticalSectionRawMutex, u16, 8>,
        angle_b_watch: &'static Watch<CriticalSectionRawMutex, u16, 8>,
    ) {
        info!("DisplayService: starting");

        let mut screen_rx = screen_watch
            .receiver()
            .expect("Failed to create screen receiver");
        let mut countdown_rx = countdown_watch
            .receiver()
            .expect("Failed to create countdown receiver");
        let mut angle_a_rx = angle_a_watch
            .receiver()
            .expect("Failed to create angle A receiver");
        let mut angle_b_rx = angle_b_watch
            .receiver()
            .expect("Failed to create angle B receiver");

        // Start with splash screen
        let mut current_screen = Screen::Splash;
        let mut active_screen = ActiveScreen::Splash(core::mem::replace(
            &mut self.splash,
            SplashView::new(320, 240),
        ));
        
        // Open initial screen
        if let Ok(buttons) = active_screen.open(self.display.driver_mut()) {
            input::set_buttons(buttons);
        }

        loop {
            // Wait for any event
            let event = select::select4(
                screen_rx.changed(),
                countdown_rx.changed(),
                angle_a_rx.changed(),
                angle_b_rx.changed(),
            ).await;

            match event {
                select::Either4::First(new_screen) => {
                    if new_screen != current_screen {
                        info!("DisplayService: navigating to {:?}", new_screen);
                        
                        // Close current screen and clear buttons
                        let _ = active_screen.close(self.display.driver_mut());
                        input::clear_buttons();
                        
                        // Switch to new screen
                        current_screen = new_screen;
                        active_screen = match current_screen {
                            Screen::Splash => {
                                let view = core::mem::replace(&mut self.splash, SplashView::new(320, 240));
                                ActiveScreen::Splash(view)
                            }
                            Screen::Dashboard => {
                                let view = core::mem::replace(&mut self.dashboard, DashboardView::new(320, 240));
                                ActiveScreen::Dashboard(view)
                            }
                        };
                        
                        // Open new screen and register its buttons
                        if let Ok(buttons) = active_screen.open(self.display.driver_mut()) {
                            input::set_buttons(buttons);
                        }
                    }
                }
                select::Either4::Second(countdown) => {
                    let _ = active_screen.update(
                        self.display.driver_mut(),
                        ScreenEvent::Countdown(countdown)
                    );
                }
                select::Either4::Third(angle_a) => {
                    let _ = active_screen.update(
                        self.display.driver_mut(),
                        ScreenEvent::AngleA(angle_a)
                    );
                }
                select::Either4::Fourth(angle_b) => {
                    let _ = active_screen.update(
                        self.display.driver_mut(),
                        ScreenEvent::AngleB(angle_b)
                    );
                }
            }
        }
    }
}

