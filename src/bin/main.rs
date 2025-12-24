#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use alloc::boxed::Box;
use alloc::sync::Arc;
use embassy_executor::Spawner;
use embassy_futures::select;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use log::{info, warn};
use m5_minimal::display::Display;
use m5_minimal::filters::MotorValueFilter;
use m5_minimal::hardware::Board;

/// Reset state for Motor A
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ResetState {
    Requested,
    StartMoving,
    Moving,
    Complete,
}

// Global channel to stream angle updates from motor task to display task
static ANGLE_A_CH: Watch<CriticalSectionRawMutex, u16, 8> = Watch::new();
static ANGLE_B_CH: Watch<CriticalSectionRawMutex, u16, 8> = Watch::new();

static SPEED_A_CH: Watch<CriticalSectionRawMutex, f32, 4> = Watch::new();

// channel to track/reset Motor A state (requested -> moving -> complete)
static MOTOR_A_RESET: Channel<CriticalSectionRawMutex, bool, 1> = Channel::new();

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
        peripherals.GPIO9, // Port B SDA
        peripherals.GPIO8, // Port B SCL
    )
    .await;

    // Spawn display task to render angle updates
    let display = board.display;
    spawner.must_spawn(run_display(display));

    // Extract motors and touch
    let motor_a = Arc::new(Mutex::new(board.roller_a));
    //let _motor_b = board.roller_b; // Motor B is on same bus as A, commands sent via A's I2C
    let touch = board.touch;

    // Poll Motor A (Motor B shares the same I2C1 bus, commanded via Motor A's i2c field)
    spawner.must_spawn(run_touch_reader(touch));
    spawner.must_spawn(run_motor_a_reader(motor_a.clone()));
    spawner.must_spawn(run_motor_a_reset(motor_a));

    info!("Motor A and Motor B on shared I2C1 Port A, Touch on I2C0");

    // Idle loop
    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}

/// Task: read Motor A encoder and publish angle updates
#[embassy_executor::task]
async fn run_motor_a_reader(
    motor: Arc<
        Mutex<
            CriticalSectionRawMutex,
            m5_minimal::hardware::Roller485<esp_hal::i2c::master::I2c<'static, esp_hal::Blocking>>,
        >,
    >,
) {
    info!("Motor A reader: start (I2C1, addr=0x64)");

    // Ensure encoder mode once on startup
    {
        let mut guard = motor.lock().await;
        let _ = guard.ensure_encoder_mode();
    }

    let mut original_angle = 0;
    let mut first_position_sent = false;
    let angle_sender = ANGLE_A_CH.sender();
    let speed_sender = SPEED_A_CH.sender();

    let mut speed_filter = MotorValueFilter::new(0.2, 0.3, 0.05);

    loop {
        {
            let mut guard = motor.lock().await;
            match guard.read_encoder_position() {
                Ok(steps) => {
                    let angle = ((steps % 333 + 333) % 333) * 360 / 333;
                    if (!first_position_sent) || (angle != original_angle) {
                        info!("Motor position: steps={}, angle={}°", steps, angle);
                        let _ = angle_sender.send(angle as u16);
                        first_position_sent = true;
                        original_angle = angle;
                    }
                }
                Err(e) if !first_position_sent => warn!("Motor read error: {:?}", e),
                Err(_) => {}
            }
            match guard.read_speed_rpm() {
                Ok(rpm) => {
                    if let Some(rpm) = speed_filter.update(rpm) {
                        info!("Motor speed: {:.2} RPM", rpm);
                        let _ = speed_sender.send(rpm);
                    }
                }
                Err(e) => {
                    warn!("Motor speed read error: {:?}", e);
                }
            }
        }

        Timer::after(Duration::from_millis(100)).await;
    }
}

/// Task: handle Motor A reset-to-zero flow
#[embassy_executor::task]
async fn run_motor_a_reset(
    motor: Arc<
        Mutex<
            CriticalSectionRawMutex,
            m5_minimal::hardware::Roller485<esp_hal::i2c::master::I2c<'static, esp_hal::Blocking>>,
        >,
    >,
) {
    info!("Motor A reset: start (I2C1, addr=0x64)");

    let mut reset_state = ResetState::Complete;
    let mut angle_receiver = ANGLE_A_CH
        .receiver()
        .expect("Could not register angle receiver for Motor A reset");

    let mut speed_receiver = SPEED_A_CH
        .receiver()
        .expect("Could not register speed receiver for Motor A reset");

    loop {
        // Process reset requests, this is not a state, but just a trigger

        // nu transformeren zonder state machine
        MOTOR_A_RESET.receive().await; // only true values are sent ;-)

        if angle_receiver.get().await == 0 {
            // already at zero, skip
            continue;
        }

        // we need to move, so lets move to position 0
        let mut guard = motor.lock().await;
        let _ = guard.set_position(0);

        info!("Waiting for speed to reach nonZero...");
        let _ = speed_receiver.changed_and(|v| v.abs() > 0.2f32).await;

        info!("Waiting for speed to reach 0...");
        let _ = speed_receiver.changed_and(|v| *v == 0.0f32).await;

        Timer::after(Duration::from_millis(100)).await;

        let mut guard = motor.lock().await;
        let _ = guard.ensure_encoder_mode();

        info!("Motor A reset to zero complete.");

    }
}

/// Task: render angle on the display using embedded-graphics
#[embassy_executor::task]
async fn run_display(mut display: Display<m5_minimal::hardware::CoreS3Display<'static>>) {
    info!("Display task: init UI");
    display.init_angle_display();

    let mut last_a: u16 = 0;
    let mut last_b: u16 = 0;

    let mut receiver_a = ANGLE_A_CH
        .receiver()
        .expect("Could not register receiver A");
    let mut receiver_b = ANGLE_B_CH
        .receiver()
        .expect("Could not register receiver B");

    loop {
        // Wait for either channel to receive a value
        let event = select::select(receiver_a.changed(), receiver_b.changed()).await;
        match event {
            select::Either::First(a) => {
                last_a = a;
            }
            select::Either::Second(b) => {
                last_b = b;
            }
        }

        display.update_dual_angles(last_a, last_b);
    }
}

/// Task: Read touch events from FT6336 and log them
#[embassy_executor::task]
async fn run_touch_reader(
    mut touch: m5_minimal::hardware::FT6336<esp_hal::i2c::master::I2c<'static, esp_hal::Blocking>>,
) {
    info!("Touch reader task starting...");

    loop {
        if let Ok(Some(point)) = touch.read_touch() {
            match point.event {
                m5_minimal::hardware::TouchEvent::Press => {
                    info!("TOUCH PRESS: x={}, y={}", point.x, point.y);
                    let _ = MOTOR_A_RESET.try_send(true);
                }
                m5_minimal::hardware::TouchEvent::Contact => {
                    // Only log contact occasionally to avoid spam
                    // info!("Touch contact: x={}, y={}", point.x, point.y);
                }
                m5_minimal::hardware::TouchEvent::Release => {
                    info!("TOUCH RELEASE");
                    // now we want motor A to go to potition 0
                }
            }
        }

        // Poll touch at ~100Hz
        Timer::after(Duration::from_millis(10)).await;
    }
}
