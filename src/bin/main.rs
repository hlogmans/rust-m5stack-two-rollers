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
use m5_minimal::hardware::{Board, ConfirmedPress};
use m5_minimal::helpers::TelemetrySender;
use m5_minimal::helpers::{print_memory_diagnostics, print_memory_stats};
use m5_minimal::{info, warn}; // provides panic handler with backtrace via esp-println
use m5_minimal::business;
use m5_minimal::ui;

use alloc::boxed::Box;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use m5_minimal::business::input;

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

// Touch events - now a Channel for confirmed presses only
static TOUCH_CH: Channel<CriticalSectionRawMutex, ConfirmedPress, 8> = Channel::new();

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

    // Clone motors BEFORE display moves into DisplayService
    let motor_a = Box::leak(Box::new(board.roller_a.clone()));
    let motor_b = Box::leak(Box::new(board.roller_b.clone()));

    // Extract touch and display
    let touch = board.touch;
    let display_service = m5_minimal::ui::DisplayService::new(board.display);

    // === UI Layer Initialization ===
    if let Err(e) = ui::init_display_service(
        &spawner,
        display_service,
        &SCREEN_CH,
        &COUNTDOWN_CH,
        &ANGLE_A_CH,
        &ANGLE_B_CH,
    ) {
        warn!("UI init_display_service failed: {:?}", e);
    }

    if let Err(e) = ui::init_navigation(&spawner, &SCREEN_CH, &COUNTDOWN_CH) {
        warn!("UI init_navigation failed: {:?}", e);
    }

    // Wrap touch in SharedFT6336 with debounced press channel and spawn background task
    let shared_touch = m5_minimal::hardware::SharedFT6336::new(
        touch,
        Some(TOUCH_CH.sender()),
    );

    if let Err(e) = m5_minimal::hardware::init_touch(&spawner, shared_touch) {
        warn!("Hardware init_touch failed: {:?}", e);
    }

    // === Business Layer Initialization ===
    if let Err(e) = business::init_motors(
        &spawner,
        motor_a,
        motor_b,
        &MOTOR_A_CMD,
        &MOTOR_B_CMD,
        &MOTOR_A_RESET,
        &MOTOR_B_RESET,
        &ANGLE_A_CH,
        &SPEED_A_CH,
        &SPEED_B_CH,
    ) {
        warn!("Business init_motors failed: {:?}", e);
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

