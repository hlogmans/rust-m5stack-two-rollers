#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use defmt_rtt as _;
use m5_minimal::{info, warn};
use m5_minimal::hardware::Board;
use m5_minimal::hardware::TouchPoint;
use m5_minimal::helpers::TelemetrySender;
use esp_backtrace as _; // provides panic handler with backtrace via esp-println

use alloc::boxed::Box;
use embassy_executor::Spawner;
use embassy_futures::select;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use m5_minimal::display::Display;
use m5_minimal::filters::MotorValueFilter;


// Global channels for Motor A telemetry and commands
static ANGLE_A_CH: Watch<CriticalSectionRawMutex, u16, 8> = Watch::new();
static SPEED_A_CH: Watch<CriticalSectionRawMutex, f32, 4> = Watch::new();
static MOTOR_A_CMD: Channel<CriticalSectionRawMutex, m5_minimal::hardware::MotorCommand, 4> = Channel::new();

// Business logic: reset triggers per motor
static MOTOR_A_RESET: Channel<CriticalSectionRawMutex, (), 1> = Channel::new();
static MOTOR_B_RESET: Channel<CriticalSectionRawMutex, (), 1> = Channel::new();

// Display gets angle updates for both motors
static ANGLE_B_CH: Watch<CriticalSectionRawMutex, u16, 8> = Watch::new();
static SPEED_B_CH: Watch<CriticalSectionRawMutex, f32, 4> = Watch::new();
static MOTOR_B_CMD: Channel<CriticalSectionRawMutex, m5_minimal::hardware::MotorCommand, 4> = Channel::new();

// Touch events
static TOUCH_CH: Watch<CriticalSectionRawMutex, TouchPoint, 4> = Watch::new();

// Screen navigation
static SCREEN_CH: Watch<CriticalSectionRawMutex, m5_minimal::ui::Screen, 4> = Watch::new();
static COUNTDOWN_CH: Watch<CriticalSectionRawMutex, u8, 4> = Watch::new();

// Panic handling is provided by esp-backtrace (print-uart feature) for stack traces

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    // Start embassy executor timer FIRST - needed for any async operations
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    // Initialize all hardware (power, display, and two Roller485 motors on separate I2C buses)
    let display_buffer: &'static mut [u8; 512] = Box::leak(Box::new([0_u8; 512]));
    let board = Board::init(
        peripherals.I2C0,
        peripherals.GPIO12,
        peripherals.GPIO11,
        peripherals.SPI2,
        peripherals.GPIO37,
        peripherals.GPIO36,
        peripherals.GPIO3,
        peripherals.GPIO35,
        peripherals.GPIO15,
        display_buffer,
        peripherals.I2C1,
        peripherals.GPIO9, // Port B SDA
        peripherals.GPIO8, // Port B SCL
        Some(TelemetrySender::from_watch(&ANGLE_A_CH)),   // Angle via Watch (display needs latest)
        Some(TelemetrySender::from_watch(&SPEED_A_CH)),   // Speed via Watch (reset handler needs latest)
        Some(TelemetrySender::from_watch(&ANGLE_B_CH)),   // Motor B angle for display
        Some(TelemetrySender::from_watch(&SPEED_B_CH)),   // Motor B speed for diagnostics
        &MOTOR_A_CMD,
        &MOTOR_B_CMD,
    )
    .await;

    // Spawn display task to render angle updates
    let display = board.display;
    if let Err(e) = spawner.spawn(run_display(display)) {
        warn!("Spawn display failed: {:?}", e);
    }

    // Spawn navigation task to manage screen transitions
    if let Err(e) = spawner.spawn(run_navigation()) {
        warn!("Spawn navigation failed: {:?}", e);
    }

    // Extract motors and touch
    let motor_a = board.roller_a;
    let motor_b = board.roller_b;
    let touch = board.touch;

    // Spawn motor background task with speed filtering
    let speed_filter_a = Some(MotorValueFilter::new(0.75, 0.3, 0.05));
    if let Err(e) = spawner.spawn(run_motor_background("A", motor_a.clone(), speed_filter_a)) {
        warn!("Spawn motor A failed: {:?}", e);
    }

    let speed_filter_b = Some(MotorValueFilter::new(0.75, 0.3, 0.05));
    if let Err(e) = spawner.spawn(run_motor_background("B", motor_b.clone(), speed_filter_b)) {
        warn!("Spawn motor B failed: {:?}", e);
    }
    
    // Spawn motor reset handlers (business logic)
    if let Err(e) = spawner.spawn(run_motor_reset_handler_a(motor_a)) {
        warn!("Spawn motor A reset handler failed: {:?}", e);
    }

    if let Err(e) = spawner.spawn(run_motor_reset_handler_b(motor_b)) {
        warn!("Spawn motor B reset handler failed: {:?}", e);
    }
    
    // Wrap touch in SharedFT6336 with telemetry sender via Watch
    let shared_touch = m5_minimal::hardware::SharedFT6336::new(
        touch,
        Some(TelemetrySender::from_watch(&TOUCH_CH)),
    );
    
    // Spawn touch reader background task
    if let Err(e) = spawner.spawn(run_touch_reader(shared_touch)) {
        warn!("Spawn touch reader failed: {:?}", e);
    }
    
    // Spawn touch event handler (business logic) - responds to Press events
    if let Err(e) = spawner.spawn(run_touch_handler()) {
        warn!("Spawn touch handler failed: {:?}", e);
    }

    info!("Motor A (0x65) and Motor B (0x64) on shared I2C1 Port B, Touch on I2C0");

    // Idle loop
    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}

/// Task: run the motor background polling and command processing
#[embassy_executor::task(pool_size = 2)]
async fn run_motor_background(
    name: &'static str,
    motor: m5_minimal::hardware::SharedRoller485<m5_minimal::hardware::RollerI2cDevice>,
    speed_filter: Option<MotorValueFilter>,
) {
    info!("Motor {} background task starting", name);
    motor.run_background_task(speed_filter).await;
}

/// Task: handle motor A reset to zero (business logic)
#[embassy_executor::task]
async fn run_motor_reset_handler_a(
    motor: m5_minimal::hardware::SharedRoller485<m5_minimal::hardware::RollerI2cDevice>,
) {
    info!("Motor A reset handler starting");

    let mut angle_receiver = ANGLE_A_CH
        .receiver()
        .expect("Need angle receiver for reset handler");
    let mut speed_receiver = SPEED_A_CH
        .receiver()
        .expect("Need speed receiver for reset handler");

    loop {
        MOTOR_A_RESET.receive().await;

        info!("Reset A to zero requested");

        if angle_receiver.get().await == 0 {
            info!("A already at zero");
            continue;
        }

        motor.send_command(m5_minimal::hardware::MotorCommand::SetPosition(0)).await;

        info!("Waiting A movement start...");
        let _ = speed_receiver.changed_and(|v| v.abs() > 0.2f32).await;

        info!("Waiting A movement stop...");
        let _ = speed_receiver.changed_and(|v| *v == 0.0f32).await;

        Timer::after(Duration::from_millis(100)).await;
        motor.send_command(m5_minimal::hardware::MotorCommand::SetReading).await;

        info!("Reset A complete");
    }
}

/// Task: handle motor B reset to zero (business logic)
#[embassy_executor::task]
async fn run_motor_reset_handler_b(
    motor: m5_minimal::hardware::SharedRoller485<m5_minimal::hardware::RollerI2cDevice>,
) {
    info!("Motor B reset handler starting");

    let mut angle_receiver = ANGLE_B_CH
        .receiver()
        .expect("Need angle receiver for reset handler B");
    let mut speed_receiver = SPEED_B_CH
        .receiver()
        .expect("Need speed receiver for reset handler B");

    loop {
        MOTOR_B_RESET.receive().await;

        info!("Reset B to zero requested");

        if angle_receiver.get().await == 0 {
            info!("B already at zero");
            continue;
        }

        motor.send_command(m5_minimal::hardware::MotorCommand::SetPosition(0)).await;

        info!("Waiting B movement start...");
        let _ = speed_receiver.changed_and(|v| v.abs() > 0.2f32).await;

        info!("Waiting B movement stop...");
        let _ = speed_receiver.changed_and(|v| *v == 0.0f32).await;

        Timer::after(Duration::from_millis(100)).await;
        motor.send_command(m5_minimal::hardware::MotorCommand::SetReading).await;

        info!("Reset B complete");
    }
}

/// Task: render angle on the display using embedded-graphics
#[embassy_executor::task]
async fn run_display(mut display: Display<m5_minimal::hardware::CoreS3Display<'static>>) {
    use m5_minimal::ui::{SplashView, Screen};
    use embedded_graphics::prelude::*;
    use embedded_graphics::pixelcolor::Rgb565;
    
    info!("Display task: waiting for screen navigation");
    
    let mut screen_receiver = SCREEN_CH
        .receiver()
        .expect("Could not register screen receiver");
    let mut countdown_receiver = COUNTDOWN_CH
        .receiver()
        .expect("Could not register countdown receiver");
    
    // Create views
    let splash = SplashView::new(320, 240);
    
    let mut current_screen = Screen::Splash;
    let mut last_a: u16 = 0;
    let mut last_b: u16 = 0;
    let mut last_countdown: u8 = 4;

    let mut receiver_a = ANGLE_A_CH
        .receiver()
        .expect("Could not register receiver A");
    let mut receiver_b = ANGLE_B_CH
        .receiver()
        .expect("Could not register receiver B");

    // Initialize splash screen
    let driver = display.driver_mut();
    let _ = driver.clear(Rgb565::BLACK);
    let _ = splash.init(driver);
    let _ = splash.update_countdown(driver, last_countdown);

    loop {
        // Wait for either screen change, countdown update, or angle update
        let event = select::select4(
            screen_receiver.changed(),
            countdown_receiver.changed(),
            receiver_a.changed(),
            receiver_b.changed(),
        ).await;
        
        match event {
            select::Either4::First(screen) => {
                // Screen changed
                if screen != current_screen {
                    current_screen = screen;
                    
                    match current_screen {
                        Screen::Splash => {
                            let driver = display.driver_mut();
                            let _ = driver.clear(Rgb565::BLACK);
                            let _ = splash.init(driver);
                            let _ = splash.update_countdown(driver, last_countdown);
                        }
                        Screen::Dashboard => {
                            display.init_angle_display();
                            display.update_dual_angles(last_a, last_b);
                        }
                    }
                }
            }
            select::Either4::Second(countdown) => {
                // Countdown updated
                last_countdown = countdown;
                if current_screen == Screen::Splash {
                    let _ = splash.update_countdown(display.driver_mut(), last_countdown);
                }
            }
            select::Either4::Third(a) => {
                last_a = a;
                if current_screen == Screen::Dashboard {
                    display.update_dual_angles(last_a, last_b);
                }
            }
            select::Either4::Fourth(b) => {
                last_b = b;
                if current_screen == Screen::Dashboard {
                    display.update_dual_angles(last_a, last_b);
                }
            }
        }
    }
}

/// Task: Manage screen navigation (splash -> dashboard)
#[embassy_executor::task]
async fn run_navigation() {
    use m5_minimal::ui::Screen;
    
    info!("Navigation task: starting splash screen");
    
    // Initialize with Splash screen
    SCREEN_CH.sender().send(Screen::Splash);
    
    // Countdown from 4 to 1 seconds
    for countdown in (1..=4).rev() {
        COUNTDOWN_CH.sender().send(countdown);
        Timer::after(Duration::from_secs(1)).await;
    }
    
    // Switch to Dashboard
    info!("Navigation: switching to dashboard");
    SCREEN_CH.sender().send(Screen::Dashboard);
    
    // Navigation task complete - screen stays on dashboard
    loop {
        Timer::after(Duration::from_secs(3600)).await;
    }
}

/// Task: Read touch events via SharedFT6336
#[embassy_executor::task]
async fn run_touch_reader(
    shared_touch: m5_minimal::hardware::SharedFT6336<
        esp_hal::i2c::master::I2c<'static, esp_hal::Blocking>,
        4,
    >,
) {
    info!("Touch reader task starting...");

    // Run the background task for continuous polling and telemetry
    shared_touch.run_background_task().await
}

/// Business logic: Respond to touch events
#[embassy_executor::task]
async fn run_touch_handler() {
    info!("Touch handler task starting...");
    let mut receiver = TOUCH_CH
        .receiver()
        .expect("Need touch receiver for handler");

    // Button hitboxes
    // Motor A button: left gauge area
    const BTN_A_X1: u16 = 20;
    const BTN_A_X2: u16 = 140;
    const BTN_A_Y1: u16 = 200;
    const BTN_A_Y2: u16 = 235;

    // Motor B button: right gauge area
    const BTN_B_X1: u16 = 180;
    const BTN_B_X2: u16 = 300;
    const BTN_B_Y1: u16 = 200;
    const BTN_B_Y2: u16 = 235;

    loop {
        let point = receiver.changed().await;
        match point.event {
            m5_minimal::hardware::TouchEvent::Press => {
                info!("TOUCH PRESS at ({}, {})", point.x, point.y);

                let in_btn_a = point.x >= BTN_A_X1 && point.x <= BTN_A_X2 && point.y >= BTN_A_Y1 && point.y <= BTN_A_Y2;
                let in_btn_b = point.x >= BTN_B_X1 && point.x <= BTN_B_X2 && point.y >= BTN_B_Y1 && point.y <= BTN_B_Y2;

                if in_btn_a {
                    info!("Trigger reset Motor A");
                    let _ = MOTOR_A_RESET.try_send(());
                } else if in_btn_b {
                    info!("Trigger reset Motor B");
                    let _ = MOTOR_B_RESET.try_send(());
                } else {
                    info!("Touch outside reset buttons");
                }
            }
            m5_minimal::hardware::TouchEvent::Contact => {
                // Only log contact occasionally to avoid spam
                // info!("Touch contact: x={}, y={}", point.x, point.y);
            }
            m5_minimal::hardware::TouchEvent::Release => {
                info!("TOUCH RELEASE");
            }
        }
    }
}
