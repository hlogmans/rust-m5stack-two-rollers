#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use defmt_rtt as _;
use esp_backtrace as _;
use m5_minimal::hardware::Board;
use m5_minimal::hardware::TouchPoint;
use m5_minimal::helpers::TelemetrySender;
use m5_minimal::helpers::{print_memory_diagnostics, print_memory_stats};
use m5_minimal::{info, warn}; // provides panic handler with backtrace via esp-println

use alloc::boxed::Box;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use m5_minimal::business::input;
use m5_minimal::filters::MotorValueFilter;
use m5_minimal::ui::DisplayService;

// Global channels for Motor A telemetry and commands
static ANGLE_A_CH: Watch<CriticalSectionRawMutex, u16, 8> = Watch::new();
static SPEED_A_CH: Watch<CriticalSectionRawMutex, f32, 4> = Watch::new();
static MOTOR_A_CMD: Channel<CriticalSectionRawMutex, m5_minimal::hardware::MotorCommand, 4> =
    Channel::new();

// Business logic: reset triggers per motor
static MOTOR_A_RESET: Channel<CriticalSectionRawMutex, (), 1> = Channel::new();
static MOTOR_B_RESET: Channel<CriticalSectionRawMutex, (), 1> = Channel::new();

// Display gets angle updates for both motors
static ANGLE_B_CH: Watch<CriticalSectionRawMutex, u16, 8> = Watch::new();
static SPEED_B_CH: Watch<CriticalSectionRawMutex, f32, 4> = Watch::new();
static MOTOR_B_CMD: Channel<CriticalSectionRawMutex, m5_minimal::hardware::MotorCommand, 4> =
    Channel::new();

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
    let (timg0, peripherals) = m5_minimal::hardware::split_peripherals(peripherals);
    let timg0 = TimerGroup::new(timg0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");
    
    // Print initial memory statistics
    print_memory_diagnostics();

    // Initialize all hardware (power, display, and two Roller485 motors on separate I2C buses)
    let display_buffer: &'static mut [u8; 512] = Box::leak(Box::new([0_u8; 512]));
    let board = Board::init(
        peripherals,
        display_buffer,
        Some(TelemetrySender::from_watch(&ANGLE_A_CH)), // Angle via Watch (display needs latest)
        Some(TelemetrySender::from_watch(&SPEED_A_CH)), // Speed via Watch (reset handler needs latest)
        Some(TelemetrySender::from_watch(&ANGLE_B_CH)), // Motor B angle for display
        Some(TelemetrySender::from_watch(&SPEED_B_CH)), // Motor B speed for diagnostics
        &MOTOR_A_CMD,
        &MOTOR_B_CMD,
    )
    .await;

    // Spawn DisplayService to manage screens and rendering
    let display_service = DisplayService::new(board.display);
    if let Err(e) = spawner.spawn(run_display_service(
        display_service,
        &SCREEN_CH,
        &COUNTDOWN_CH,
        &ANGLE_A_CH,
        &ANGLE_B_CH,
    )) {
        warn!("Spawn display service failed: {:?}", e);
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
    if let Err(e) = spawner.spawn(run_motor_reset_handler(
        "A",
        motor_a,
        &ANGLE_A_CH,
        &SPEED_A_CH,
        &MOTOR_A_RESET,
    )) {
        warn!("Spawn motor A reset handler failed: {:?}", e);
    }

    if let Err(e) = spawner.spawn(run_motor_test(
        "B",
        motor_b,
        // &ANGLE_B_CH,
        // &SPEED_B_CH,
        &MOTOR_B_RESET,
    )) {
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

    // Spawn button router and action handler (business logic)
    if let Err(e) = spawner.spawn(input::run_button_router(&TOUCH_CH)) {
        warn!("Spawn button router failed: {:?}", e);
    }
    if let Err(e) = spawner.spawn(input::run_button_actions(&MOTOR_A_RESET, &MOTOR_B_RESET)) {
        warn!("Spawn button actions failed: {:?}", e);
    }

    info!("Motor A (0x65) and Motor B (0x64) on shared I2C1 Port B, Touch on I2C0");

    // Idle loop with periodic memory monitoring
    let mut counter = 0;
    loop {
        Timer::after(Duration::from_secs(1)).await;
        
        // Print memory stats every 10 seconds
        counter += 1;
        if counter >= 10 {
            print_memory_stats();
            counter = 0;
        }
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
    motor.run_background_task(name, speed_filter).await;
}

/// Task: run the motor for three seconds - generic for A/B
#[embassy_executor::task(pool_size = 2)]
async fn run_motor_test(
    name: &'static str,
    motor: m5_minimal::hardware::SharedRoller485<m5_minimal::hardware::RollerI2cDevice>,
    reset_ch: &'static Channel<CriticalSectionRawMutex, (), 1>,
) {
    loop {
        reset_ch.receive().await;
        info!("Motor {} test task starting", name);
        let mut speed = 10000i32;

        for _ in 0..10 {
            motor
                .send_command(m5_minimal::hardware::MotorCommand::SetSpeed(speed))
                .await;
            speed *= -2; 
            Timer::after(Duration::from_millis(500)).await;
        }   

        motor
            .send_command(m5_minimal::hardware::MotorCommand::SetSpeed(0))
            .await;
        motor.send_command(m5_minimal::hardware::MotorCommand::SetReading)
            .await;
        info!("Motor {} test task complete", name);
    }
}

/// Task: handle motor reset to zero (business logic) - generic for A/B
#[embassy_executor::task(pool_size = 2)]
async fn run_motor_reset_handler(
    name: &'static str,
    motor: m5_minimal::hardware::SharedRoller485<m5_minimal::hardware::RollerI2cDevice>,
    angle_watch: &'static Watch<CriticalSectionRawMutex, u16, 8>,
    speed_watch: &'static Watch<CriticalSectionRawMutex, f32, 4>,
    reset_ch: &'static Channel<CriticalSectionRawMutex, (), 1>,
) {
    info!("Motor {} reset handler starting", name);

    let mut angle_receiver = angle_watch
        .receiver()
        .expect("Need angle receiver for reset handler");
    let mut speed_receiver = speed_watch
        .receiver()
        .expect("Need speed receiver for reset handler");

    loop {
        reset_ch.receive().await;

        info!("Reset {} to zero requested", name);

        if angle_receiver.get().await == 0 {
            info!("{} already at zero", name);
            continue;
        }

        motor
            .send_command(m5_minimal::hardware::MotorCommand::SetPosition(0))
            .await;

        info!("Waiting {} movement start...", name);
        let _ = speed_receiver.changed_and(|v| v.abs() > 0.2f32).await;

        info!("Waiting {} movement stop...", name);
        let _ = speed_receiver.changed_and(|v| *v == 0.0f32).await;

        Timer::after(Duration::from_millis(100)).await;
        motor
            .send_command(m5_minimal::hardware::MotorCommand::SetReading)
            .await;

        info!("Reset {} complete", name);
    }
}

/// Task: Display service - manages screens, navigation, and button registration
#[embassy_executor::task]
async fn run_display_service(
    service: DisplayService,
    screen_watch: &'static Watch<CriticalSectionRawMutex, m5_minimal::ui::Screen, 4>,
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

// touch handler removed in favor of business::input routing
