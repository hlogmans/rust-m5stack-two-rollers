#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use alloc::boxed::Box;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embassy_sync::channel::Channel;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use log::{info, warn};
use m5_minimal::hardware::Board;
use m5_minimal::display::Display;

// Global channel to stream angle updates from motor task to display task
static ANGLE_A_CH: Channel<CriticalSectionRawMutex, u16, 8> = Channel::new();
static ANGLE_B_CH: Channel<CriticalSectionRawMutex, u16, 8> = Channel::new();

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

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
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    info!("Embassy initialized!");

    // Start embassy executor timer FIRST - needed for any async operations
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

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
        peripherals.GPIO2,
        peripherals.GPIO1,
        peripherals.GPIO17,
        peripherals.GPIO18,
    )
    .await;

    // Spawn display task to render angle updates
    let display = board.display;
    spawner.must_spawn(run_display(display));

    // Move single motor into task (I2C1)
    let motor_a = board.roller_a;
    let motor_b = board.roller_b;

    spawner.must_spawn(run_motor_a(motor_a));
    spawner.must_spawn(run_motor_b(motor_b));

    // Idle loop
    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}
/// Task: drive motor B on I2C0 (Port C GPIO17/18) at address 0x65
#[embassy_executor::task]
async fn run_motor_b(mut motor: m5_minimal::hardware::Roller485<esp_hal::i2c::master::I2c<'static, esp_hal::Blocking>>) {
    info!("Motor B: start (I2C0 Port C, addr=0x65)");
    let mut angle: i32 = 180;

    // Set initial speed
    let _ = motor.set_speed(30);

    loop {
        // Step 45 degrees backward each 1.5s
        angle = (angle + 315) % 360; // -45 modulo 360
        let steps = (angle * 333 / 360) as i32;
        if let Err(e) = motor.set_position(steps) {
            warn!("Motor B position error: {:?}", e);
        } else {
            info!("Motor B -> {}° (steps={})", angle, steps);
            let _ = ANGLE_B_CH.try_send(angle as u16);
        }
        Timer::after(Duration::from_millis(1500)).await;
    }
}

/// Task: drive motor A on I2C1 (Grove PORT.A GPIO2/1) at address 0x64
#[embassy_executor::task]
async fn run_motor_a(mut motor: m5_minimal::hardware::Roller485<esp_hal::i2c::master::I2c<'static, esp_hal::Blocking>>) {
    info!("Motor A: start (I2C1, addr=0x64)");
    let mut angle: i32 = 0;

    // Set initial speed
    let _ = motor.set_speed(20);

    loop {
        // Step 30 degrees forward each 2s
        angle = (angle + 30) % 360;
        let steps = (angle * 333 / 360) as i32;
        if let Err(e) = motor.set_position(steps) {
            warn!("Motor A position error: {:?}", e);
        } else {
            info!("Motor A -> {}° (steps={})", angle, steps);
            // Stream current angle to display task
            let _ = ANGLE_A_CH.try_send(angle as u16);
        }
        Timer::after(Duration::from_secs(2)).await;
    }
}

/// Task: render angle on the display using embedded-graphics
#[embassy_executor::task]
async fn run_display(mut display: Display<m5_minimal::hardware::CoreS3Display<'static>>) {
    info!("Display task: init UI");
    display.init_angle_display();

    let mut last_a: u16 = 0;
    let mut last_b: u16 = 0;

    loop {
        if let Ok(a) = ANGLE_A_CH.try_receive() {
            last_a = a;
        }
        if let Ok(b) = ANGLE_B_CH.try_receive() {
            last_b = b;
        }
        display.update_dual_angles(last_a, last_b);
        Timer::after(Duration::from_millis(100)).await;
    }
}
