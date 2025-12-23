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
// Steps channel: A's encoder position (steps, post /100 conversion)
static A_STEPS_CH: Channel<CriticalSectionRawMutex, i32, 8> = Channel::new();
// Offset channel: Motor B's initial offset relative to Motor A
static OFFSET_CH: Channel<CriticalSectionRawMutex, i32, 1> = Channel::new();

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

    // Extract motors
    let motor_a = board.roller_a;
    let motor_b = board.roller_b;

    // Poll A's encoder; have B follow A's position
    spawner.must_spawn(run_motor_a_poll(motor_a));
    spawner.must_spawn(run_motor_b_follow(motor_b));
    
    info!("Motor A on I2C1 Port A, Motor B on I2C0 Port C - Touch temporarily disabled");

    // Idle loop
    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}

/// Task: Motor B follows Motor A's encoder position once per second
#[embassy_executor::task]
async fn run_motor_b_follow(mut motor: m5_minimal::hardware::Roller485<esp_hal::i2c::master::I2c<'static, esp_hal::Blocking>>) {
    info!("Motor B FOLLOW: start (I2C0 Port C, addr=0x65)");
    // Ensure position control is available when setting position
    let _ = motor.set_speed(1);

    let mut last_steps: i32 = 0;
    let mut prev_logged_steps: i32 = i32::MIN;
    let mut offset: i32 = 0;
    let mut offset_synced = false;

    // Read Motor B's current position to calculate offset on first run
    if let Ok(b_steps) = motor.read_encoder_position() {
        info!("Motor B initial position: {} steps", b_steps);
    }

    loop {
        // Receive offset from A's task (only once at startup)
        if !offset_synced {
            if let Ok(a_offset) = OFFSET_CH.try_receive() {
                // Calculate B's offset relative to A
                if let Ok(b_steps) = motor.read_encoder_position() {
                    offset = b_steps - a_offset;
                    offset_synced = true;
                    info!("Motor B offset calculated: {} (B={} - A={})", offset, b_steps, a_offset);
                }
            }
        }

        // Try to receive latest steps from A
        if let Ok(steps) = A_STEPS_CH.try_receive() {
            last_steps = steps;
        }

        // Apply A's last position to B, accounting for offset
        let target_steps = last_steps + offset;
        if let Err(e) = motor.set_position(target_steps) {
            warn!("Motor B follow error: {:?}", e);
        } else {
            // Update display with an angle derived from steps
            let angle = ((target_steps % 333 + 333) % 333) * 360 / 333;
            if target_steps != prev_logged_steps {
                info!("Motor B -> follow steps={} ({}°)", target_steps, angle);
                prev_logged_steps = target_steps;
            }
            let _ = ANGLE_B_CH.try_send(angle as u16);
        }

        Timer::after(Duration::from_millis(25)).await;
    }
}

/// Task: poll Motor A's encoder and publish steps and angle
#[embassy_executor::task]
async fn run_motor_a_poll(mut motor: m5_minimal::hardware::Roller485<esp_hal::i2c::master::I2c<'static, esp_hal::Blocking>>) {
    info!("Motor A POLL: start (I2C1, addr=0x64)");
    // Ensure encoder mode for stable reads
    let _ = motor.ensure_encoder_mode();

    let mut prev_logged_steps: i32 = i32::MIN;
    let mut offset_sent = false;

    loop {
        match motor.read_encoder_position() {
            Ok(steps) => {
                // Send initial offset once for Motor B to sync
                if !offset_sent {
                    let _ = OFFSET_CH.try_send(steps);
                    offset_sent = true;
                    info!("Motor A initial offset sent: {} steps", steps);
                }

                // Publish steps for follower
                let _ = A_STEPS_CH.try_send(steps);
                // Also send angle to display
                let angle = ((steps % 333 + 333) % 333) * 360 / 333;
                let _ = ANGLE_A_CH.try_send(angle as u16);
                if steps != prev_logged_steps {
                    info!("Motor A steps={} ({}°)", steps, angle);
                    prev_logged_steps = steps;
                }
            }
            Err(e) => warn!("Motor A read error: {:?}", e),
        }
        Timer::after(Duration::from_millis(500)).await;
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
