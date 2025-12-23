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
use m5_minimal::display;
use axp2101_dd::Axp2101Async;

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

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    // Initialize power management (AXP2101) and enable backlight
    info!("Creating shared I2C bus on GPIO12 (SDA) and GPIO11 (SCL)...");
    let i2c_bus = esp_hal::i2c::master::I2c::new(
        peripherals.I2C0,
        esp_hal::i2c::master::Config::default()
            .with_frequency(esp_hal::time::Rate::from_hz(400_000)),
    )
    .expect("Failed to create I2C")
    .with_sda(peripherals.GPIO12)
    .with_scl(peripherals.GPIO11)
    .into_async();
    
    info!("Initializing AXP2101 power management IC...");
    let mut axp = Axp2101Async::new(i2c_bus);
    
    info!("Enabling display backlight via DLDO1...");
    // Enable DLDO1 to 3.3V for display backlight
    match axp.set_ldo_voltage_mv(axp2101_dd::LdoId::Dldo1, 3300).await {
        Ok(_) => info!("DLDO1 voltage set to 3.3V"),
        Err(e) => {
            log::error!("Failed to set DLDO1 voltage: {:?}", e);
            info!("Continuing anyway...");
        }
    }
    Timer::after_millis(10).await;
    
    match axp.set_ldo_enable(axp2101_dd::LdoId::Dldo1, true).await {
        Ok(_) => info!("DLDO1 enabled"),
        Err(e) => {
            log::error!("Failed to enable DLDO1: {:?}", e);
            info!("Continuing anyway...");
        }
    }
    Timer::after_millis(100).await;
    info!("Backlight initialization complete");

    // Initialize display with direct GPIO pins (based on esp-bsp)
    let mut buffer = [0_u8; 512];
    let disp_pins = display::DisplayPeripherals {
        spi2: peripherals.SPI2,
        gpio_mosi: peripherals.GPIO37,
        gpio_sck: peripherals.GPIO36,
        gpio_cs: peripherals.GPIO3,
        gpio_dc: peripherals.GPIO35,
        gpio_rst: peripherals.GPIO15,
    };
    let mut display = display::init(disp_pins, &mut buffer);
    
    display.clear_color(display::colors::green());
    info!("Display initialized and set to red");

    // TODO: Spawn some tasks
    let _ = spawner;

    loop {
        info!("Hello world!");
        Timer::after(Duration::from_secs(1)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v~1.0/examples
}
