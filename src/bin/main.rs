#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use log::info;
use m5_minimal::{
    business::{AngleController, Evaluator, ThresholdEvaluator},
    hardware::Board,
};

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
    // generator version: 1.1.0

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    info!("Embassy initialized!");

    // Start embassy executor timer FIRST - needed for any async operations
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    // Initialize all M5Stack CoreS3 hardware (power, display, etc.)
    let mut display_buffer = [0_u8; 512];
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
        &mut display_buffer,
    )
    .await;

    // Application logic starts here - hardware is fully initialized
    let mut display = board.display;
    
    // Initialize display with circles (once)
    display.init_angle_display();
    info!("Board ready!");

    // Create angle controller for business logic
    let mut angle_controller = AngleController::new(1); // increment by 1 degree

    // Create evaluator: < 100 = green, > 300 = red, otherwise yellow
    let evaluator = ThresholdEvaluator::new(100, 300);

    // TODO: Spawn some tasks
    let _ = spawner;

    // Main application loop - slowly increment angle and display
    loop {
        // Update business logic
        angle_controller.update();
        let current_angle = angle_controller.angle();
        
        // Evaluate the angle and get the corresponding color
        let status = evaluator.evaluate(current_angle);
        let color = status.to_color();
        
        // Update only the text with evaluated color (efficient, no flicker)
        display.update_angle_text(current_angle, color);
        
        info!("Angle: {}° - Status: {:?}", current_angle, status);
        
        // Wait 50ms for smooth animation (20 updates/second)
        Timer::after(Duration::from_millis(50)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v~1.0/examples
}
