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
    business::{Evaluator, ThresholdEvaluator},
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
            peripherals.I2C1,
            peripherals.GPIO2,
            peripherals.GPIO1,
        )
        .await;

    // Application logic starts here - hardware is fully initialized
    let mut display = board.display;
    let mut roller485 = board.roller485;
    
    // Initialize display with circles (once)
    display.init_angle_display();
    info!("Board ready!");

    // Create evaluator: < 100 = green, > 300 = red, otherwise yellow
    let evaluator = ThresholdEvaluator::new(100, 300);

    // TODO: Spawn some tasks
    let _ = spawner;

    let mut loop_counter: u32 = 0;
    let mut last_valid_angle: u16 = 0;
    let mut last_steps: Option<i32> = None;
    let mut last_reported_angle: Option<u16> = None;

        // Main application loop - read Roller485 angle and display
    loop {
            // Periodically ensure we're in encoder mode (in case user switched modes on device)
            if loop_counter % 500 == 0 {
                if let Err(e) = roller485.ensure_encoder_mode() {
                    log::warn!("Failed to verify/set encoder mode: {:?}", e);
                }
            }

            // Read angle and detect zero-blocks (spurious reads)
            let block = match roller485.read_angle_block() {
                Ok(v) => v,
                Err(e) => {
                    log::error!("Failed to read Roller485: {:?}", e);
                    Timer::after(Duration::from_millis(100)).await;
                    continue;
                }
            };

            // Ignore all-zero frames outright
            if block.zero_block {
                Timer::after(Duration::from_millis(50)).await;
                continue;
            }

            // Calculate angle difference (accounting for wraparound)
            let angle_diff = if last_valid_angle == 0 {
                0 // First read
            } else {
                let diff = (block.angle_deg as i32 - last_valid_angle as i32).abs();
                if diff > 180 {
                    360 - diff
                } else {
                    diff
                }
            };

            // Only accept new angles that are:
            // 1. First reading (last_valid_angle == 0), OR
            // 2. Within reasonable range (< 45° change), OR  
            // 3. Steps changed significantly (actual movement)
            let steps_changed = last_steps.map_or(true, |prev| (block.steps - prev).abs() > 10);
            let angle_reasonable = angle_diff < 45;
            
            if last_valid_angle == 0 || angle_reasonable || steps_changed {
                last_valid_angle = block.angle_deg;
                last_steps = Some(block.steps);
            }

            let angle_to_use = last_valid_angle;
        
        // Evaluate the angle and get the corresponding color
        let status = evaluator.evaluate(angle_to_use);
        let color = status.to_color();
        
        // Update only the text with evaluated color (efficient, no flicker)
        display.update_angle_text(angle_to_use, color);
        
        if last_reported_angle != Some(angle_to_use) || loop_counter % 200 == 0 {
            info!(
                "Roller485 angle: {}° (steps={}) - Status: {:?}",
                angle_to_use, block.steps, status
            );
            last_reported_angle = Some(angle_to_use);
        }

            // Slow down raw dumps to avoid log noise
            loop_counter = loop_counter.wrapping_add(1);
        if loop_counter % 400 == 0 {
            let mut buf = [0u8; 8];
            if let Ok(raw) = roller485.read_block(0x00, &mut buf) {
                log::info!("Raw[0x00..]= {:02x?}", raw);
            }
            }
        
            // Wait 50ms for smooth updates (20 updates/second)
        Timer::after(Duration::from_millis(50)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v~1.0/examples
}
