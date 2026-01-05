//! UI layer initialization and task spawning.
//!
//! Manages all display-related setup including DisplayService, navigation,
//! and touch event handling.

use embassy_executor::Spawner;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    watch::Watch,
};

use crate::ui::{DisplayService, Screen};

/// Errors during UI initialization.
#[derive(Debug)]
pub enum InitError {
    /// Failed to spawn display service task.
    SpawnDisplayService,
    /// Failed to spawn navigation task.
    SpawnNavigation,
}

/// Initialize all UI-related tasks.
///
/// # Arguments
/// - `spawner`: Embassy task spawner
/// - `display_service`: Initialized DisplayService
/// - `screen_watch`: Screen state channel
/// - `countdown_watch`: Splash screen countdown channel
/// - `angle_a_watch`: Motor A angle updates
/// - `angle_b_watch`: Motor B angle updates
/// - `touch`: Initialized touch controller
///
/// # Returns
/// - `Ok(())` on success
/// - `Err(InitError)` if any task fails to spawn
pub fn init_display_service(
    spawner: &Spawner,
    display_service: DisplayService,
    screen_watch: &'static Watch<CriticalSectionRawMutex, Screen, 4>,
    countdown_watch: &'static Watch<CriticalSectionRawMutex, u8, 4>,
    angle_a_watch: &'static Watch<CriticalSectionRawMutex, u16, 8>,
    angle_b_watch: &'static Watch<CriticalSectionRawMutex, u16, 8>,
) -> Result<(), InitError> {
    spawner
        .spawn(run_display_service(
            display_service,
            screen_watch,
            countdown_watch,
            angle_a_watch,
            angle_b_watch,
        ))
        .map_err(|_| InitError::SpawnDisplayService)?;

    Ok(())
}

/// Initialize navigation task (splash -> dashboard).
pub fn init_navigation(
    spawner: &Spawner,
    screen_watch: &'static Watch<CriticalSectionRawMutex, Screen, 4>,
    countdown_watch: &'static Watch<CriticalSectionRawMutex, u8, 4>,
) -> Result<(), InitError> {
    spawner
        .spawn(run_navigation(screen_watch, countdown_watch))
        .map_err(|_| InitError::SpawnNavigation)?;

    Ok(())
}

/// Task: Display service - manages screens, navigation, and button registration
#[embassy_executor::task]
async fn run_display_service(
    service: DisplayService,
    screen_watch: &'static Watch<CriticalSectionRawMutex, Screen, 4>,
    countdown_watch: &'static Watch<CriticalSectionRawMutex, u8, 4>,
    angle_a_watch: &'static Watch<CriticalSectionRawMutex, u16, 8>,
    angle_b_watch: &'static Watch<CriticalSectionRawMutex, u16, 8>,
) {
    service
        .run(screen_watch, countdown_watch, angle_a_watch, angle_b_watch)
        .await;
}

/// Task: Manage screen navigation (splash -> dashboard)
#[embassy_executor::task]
async fn run_navigation(
    screen_watch: &'static Watch<CriticalSectionRawMutex, Screen, 4>,
    countdown_watch: &'static Watch<CriticalSectionRawMutex, u8, 4>,
) {
    use defmt::info;
    use embassy_time::{Duration, Timer};

    info!("Navigation task: starting splash screen");

    // Initialize with Splash screen
    screen_watch.sender().send(Screen::Splash);

    // Countdown from 4 to 1 seconds
    for countdown in (1..=4).rev() {
        countdown_watch.sender().send(countdown);
        Timer::after(Duration::from_secs(1)).await;
    }

    // Switch to Dashboard
    info!("Navigation: switching to dashboard");
    screen_watch.sender().send(Screen::Dashboard);

    // Navigation task complete - screen stays on dashboard
    loop {
        Timer::after(Duration::from_secs(3600)).await;
    }
}
