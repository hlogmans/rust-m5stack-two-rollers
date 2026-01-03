/// Display abstraction for M5Stack CoreS3
/// 
/// Hardware initialization and driver management.
/// UI rendering is handled by the `ui` module (MVVM architecture).
///
/// The M5CoreS3 uses an ILI9342C display controller (320x240) connected via SPI.
///
/// Pin configuration:
/// - MOSI: GPIO37
/// - SCK: GPIO36
/// - CS: GPIO3
/// - DC: GPIO35 (Data/Command)
/// - RST: GPIO15 (Reset)
/// - Backlight: AXP2101 (I2C 0x34) DLDO1

use crate::info;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use mipidsi::interface::SpiInterface;
use mipidsi::{models::ILI9342CRgb565, Builder};

// M5CoreS3 display dimensions
pub const W: u32 = 320;
pub const H: u32 = 240;

/// Raw peripherals required to build the display for the M5CoreS3
pub struct DisplayPeripherals<'a> {
    pub spi2: esp_hal::peripherals::SPI2<'a>,
    pub gpio_mosi: esp_hal::peripherals::GPIO37<'a>,  // M5CoreS3 SPI MOSI
    pub gpio_sck: esp_hal::peripherals::GPIO36<'a>,   // M5CoreS3 SPI SCK
    pub gpio_cs: esp_hal::peripherals::GPIO3<'a>,     // M5CoreS3 LCD CS
    pub gpio_dc: esp_hal::peripherals::GPIO35<'a>,    // M5CoreS3 LCD DC
    pub gpio_rst: esp_hal::peripherals::GPIO15<'a>,   // M5CoreS3 LCD RST
}

/// Initialize the ILI9342C display.
/// Returns the raw mipidsi display driver for the M5CoreS3.
#[allow(clippy::large_stack_frames)]
pub fn init<'a>(
    pins: DisplayPeripherals<'a>,
    buffer: &'a mut [u8; 512],
) -> mipidsi::Display<
    SpiInterface<'a, ExclusiveDevice<Spi<'a, esp_hal::Blocking>, Output<'a>, embedded_hal_bus::spi::NoDelay>, Output<'a>>,
    ILI9342CRgb565,
    Output<'a>,
> {
    info!("Initializing SPI...");
    let spi = Spi::new(
        pins.spi2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(40))
            .with_mode(Mode::_0),
    )
    .expect("Failed to create SPI")
    .with_mosi(pins.gpio_mosi)
    .with_sck(pins.gpio_sck);

    info!("Initializing GPIO pins...");
    let cs = Output::new(pins.gpio_cs, Level::High, OutputConfig::default());
    let spi_device = ExclusiveDevice::new_no_delay(spi, cs).expect("Failed to create SPI device");

    // Prepare reset pin and force a clean reset after PMU init
    let mut rst = Output::new(pins.gpio_rst, Level::Low, OutputConfig::default());
    let mut delay = Delay::new();
    info!("Forcing LCD reset (LOW 20ms -> HIGH 120ms)...");
    rst.set_low();
    delay.delay_millis(20);
    rst.set_high();
    delay.delay_millis(120);

    // DC pin and display interface
    let dc = Output::new(pins.gpio_dc, Level::Low, OutputConfig::default());
    // The display interface uses the provided temporary buffer for transfers.
    info!("Creating display interface...");
    let di = SpiInterface::new(spi_device, dc, buffer);

    info!("Initializing ILI9342C display controller...");
    let display = Builder::new(ILI9342CRgb565, di)
        .reset_pin(rst)
        .color_order(mipidsi::options::ColorOrder::Rgb)
        .invert_colors(mipidsi::options::ColorInversion::Normal)
        .init(&mut delay)
        .expect("Failed to initialize display");

    info!("Display initialization complete");
    display
}


