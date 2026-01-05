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
use crate::ui::screens::{SplashScreen, DashboardScreen};
use crate::hardware::CoreS3Display;
use crate::info;

/// Display wrapper with MVVM architecture support
///
/// Combines hardware driver with view model management for easy screen updates.
pub struct Display<D: DrawTarget<Color = Rgb565>> {
    driver: D,
}

impl<D: DrawTarget<Color = Rgb565>> Display<D> {
    /// Create a new display wrapper with MVVM support
    pub fn new(driver: D, width: u32, height: u32) -> Self {
        let _ = (width, height); // retain signature for existing callers
        Self { driver }
    }

    /// Get a mutable reference to the underlying driver for custom rendering
    pub fn driver_mut(&mut self) -> &mut D {
        &mut self.driver
    }
}

/// Active screen wrapper - enum of all possible screens
enum ActiveScreen {
    Splash(SplashScreen),
    Dashboard(DashboardScreen),
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
    splash: SplashScreen,
    dashboard: DashboardScreen,
}

impl DisplayService {
    /// Create a new display service
    pub fn new(display: Display<CoreS3Display<'static>>) -> Self {
        Self {
            display,
            splash: SplashScreen::new(320, 240),
            dashboard: DashboardScreen::new(320, 240),
        }
    }

    /// Run the display service task
    pub async fn run(
        mut self,
        screen_watch: &'static Watch<CriticalSectionRawMutex, Screen, 4>,
        countdown_watch: &'static Watch<CriticalSectionRawMutex, u8, 4>,
        angle_a_watch: &'static Watch<CriticalSectionRawMutex, u16, 8>,
        angle_b_watch: &'static Watch<CriticalSectionRawMutex, u16, 8>,
        dashboard_view_model_watch: &'static Watch<CriticalSectionRawMutex, crate::ui::DashboardViewModel, 2>,
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
        let mut dashboard_vm_rx = dashboard_view_model_watch
            .receiver()
            .expect("Failed to create dashboard view model receiver");

        // Start with splash screen
        let mut current_screen = Screen::Splash;
        let mut active_screen = ActiveScreen::Splash(core::mem::replace(
            &mut self.splash,
            SplashScreen::new(320, 240),
        ));
        
        // Open initial screen
        if let Ok(buttons) = active_screen.open(self.display.driver_mut()) {
            input::set_buttons(buttons);
        }

        loop {
            // Wait for any event
            let event = select::select5(
                screen_rx.changed(),
                countdown_rx.changed(),
                angle_a_rx.changed(),
                angle_b_rx.changed(),
                dashboard_vm_rx.changed(),
            ).await;

            match event {
                select::Either5::First(new_screen) => {
                    if new_screen != current_screen {
                        info!("DisplayService: navigating to {:?}", new_screen);
                        
                        // Close current screen and clear buttons
                        let _ = active_screen.close(self.display.driver_mut());
                        input::clear_buttons();
                        
                        // Switch to new screen
                        current_screen = new_screen;
                        active_screen = match current_screen {
                            Screen::Splash => {
                                let screen = core::mem::replace(&mut self.splash, SplashScreen::new(320, 240));
                                ActiveScreen::Splash(screen)
                            }
                            Screen::Dashboard => {
                                let screen = core::mem::replace(&mut self.dashboard, DashboardScreen::new(320, 240));
                                ActiveScreen::Dashboard(screen)
                            }
                        };
                        
                        // Open new screen and register its buttons
                        if let Ok(buttons) = active_screen.open(self.display.driver_mut()) {
                            input::set_buttons(buttons);
                        }
                    }
                }
                select::Either5::Second(countdown) => {
                    let _ = active_screen.update(
                        self.display.driver_mut(),
                        ScreenEvent::Countdown(countdown)
                    );
                }
                select::Either5::Third(angle_a) => {
                    let _ = active_screen.update(
                        self.display.driver_mut(),
                        ScreenEvent::AngleA(angle_a)
                    );
                }
                select::Either5::Fourth(angle_b) => {
                    let _ = active_screen.update(
                        self.display.driver_mut(),
                        ScreenEvent::AngleB(angle_b)
                    );
                }
                select::Either5::Fifth(vm) => {
                    let _ = active_screen.update(
                        self.display.driver_mut(),
                        ScreenEvent::DashboardModel(vm)
                    );
                }
            }
        }
    }
}

