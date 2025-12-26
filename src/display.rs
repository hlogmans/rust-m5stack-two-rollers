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

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use crate::info;
use crate::ui::{DashboardViewModel, DashboardView};
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
/// Returns a DrawTarget that can be used with embedded-graphics.
#[allow(clippy::large_stack_frames)]
pub fn init<'a>(
    pins: DisplayPeripherals<'a>,
    buffer: &'a mut [u8; 512],
) -> Display<
    mipidsi::Display<
        SpiInterface<'a, ExclusiveDevice<Spi<'a, esp_hal::Blocking>, Output<'a>, embedded_hal_bus::spi::NoDelay>, Output<'a>>,
        ILI9342CRgb565,
        Output<'a>,
    >,
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
    
    // Wrap in our high-level Display facade with MVVM support
    Display::new(display, W, H)
}

/// Display wrapper with MVVM architecture support
pub struct Display<D: DrawTarget<Color = Rgb565>> {
    driver: D,
    view_model: DashboardViewModel,
    view: DashboardView,
}

impl<D: DrawTarget<Color = Rgb565>> Display<D> {
    /// Create a new display wrapper with MVVM support
    pub fn new(driver: D, width: u32, height: u32) -> Self {
        let view_model = DashboardViewModel::new();
        let view = DashboardView::new(width, height);
        Self {
            driver,
            view_model,
            view,
        }
    }

    /// Initialize UI (render static elements)
    pub fn init_angle_display(&mut self) {
        info!("Initializing dashboard UI (MVVM)");
        let _ = self.view.init(&mut self.driver);

        let _ = self.view.update(&mut self.driver, &self.view_model, true);
    }

    /// Update display with dual motor angles (MVVM pattern)
    pub fn update_dual_angles(&mut self, angle_a: u16, angle_b: u16) {
        // Update ViewModel (business/presentation state)
        self.view_model.update_motor_a_angle(angle_a);
        self.view_model.update_motor_b_angle(angle_b);

        // Render ViewModel to screen (View layer)
        let _ = self.view.update(&mut self.driver, &self.view_model, false);
    }

    /// Get mutable reference to view model (for future extensions)
    pub fn view_model_mut(&mut self) -> &mut DashboardViewModel {
        &mut self.view_model
    }
}
