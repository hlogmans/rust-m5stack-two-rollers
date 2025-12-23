/// Display abstraction for M5CoreS3
///
/// This module provides a hardware abstraction for the M5CoreS3 display,
/// following the principle of separating business logic from hardware details.
///
/// The M5CoreS3 uses an ILI9342C display controller (320x240) connected via SPI.
///
/// Pin configuration (based on esp-bsp):
/// - MOSI: GPIO37
/// - SCK: GPIO36
/// - MISO: GPIO35 (Note: same as DC, but MISO not used for display)
/// - CS: GPIO3
/// - DC: GPIO35 (Data/Command)
/// - RST: GPIO15 (Reset)
/// - Backlight: AXP2101 (I2C 0x34) DLDO1

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::mono_font::{ascii::FONT_10X20, MonoTextStyle};
use embedded_graphics::text::{Alignment, Text};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use mipidsi::interface::SpiInterface;
use mipidsi::{models::ILI9342CRgb565, Builder};
use core::fmt::Write;

// M5CoreS3 display dimensions
pub const W: i32 = 320;
pub const H: i32 = 240;

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
    log::info!("Initializing SPI...");
    let spi = Spi::new(
        pins.spi2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(40))
            .with_mode(Mode::_0),
    )
    .expect("Failed to create SPI")
    .with_mosi(pins.gpio_mosi)
    .with_sck(pins.gpio_sck);

    log::info!("Initializing GPIO pins...");
    let cs = Output::new(pins.gpio_cs, Level::High, OutputConfig::default());
    let spi_device = ExclusiveDevice::new_no_delay(spi, cs).expect("Failed to create SPI device");

    let dc = Output::new(pins.gpio_dc, Level::Low, OutputConfig::default());
    let rst = Output::new(pins.gpio_rst, Level::Low, OutputConfig::default());

    // The display interface uses the provided temporary buffer for transfers.
    log::info!("Creating display interface...");
    let di = SpiInterface::new(spi_device, dc, buffer);

    let mut delay = Delay::new();

    log::info!("Initializing ILI9342C display controller...");
    let display = Builder::new(ILI9342CRgb565, di)
        .reset_pin(rst)
        .color_order(mipidsi::options::ColorOrder::Bgr)
        .invert_colors(mipidsi::options::ColorInversion::Inverted)
        .init(&mut delay)
        .expect("Failed to initialize display");

    log::info!("Display initialization complete");
    // Wrap in our high-level Display facade
    Display { inner: display }
}

/// Simple high-level facade hiding low-level embedded-graphics usage.
pub struct Display<D: DrawTarget<Color = Rgb565>> {
    inner: D,
}

impl<D: DrawTarget<Color = Rgb565>> Display<D> {
    /// Fill the screen with a color.
    pub fn clear_color(&mut self, color: Rgb565) {
        let area = Rectangle::new(Point::zero(), Size::new(W as u32, H as u32));
        let _ = area.into_styled(PrimitiveStyle::with_fill(color))
            .draw(&mut self.inner);
    }

    /// Draw a circle with a stroke at the specified position
    ///
    /// # Arguments
    /// * `center` - The center point of the circle
    /// * `diameter` - The diameter of the circle
    /// * `stroke_color` - The color of the circle stroke
    /// * `stroke_width` - The width of the stroke
    pub fn draw_circle(&mut self, center: Point, diameter: u32, stroke_color: Rgb565, stroke_width: u32) {
        let style = PrimitiveStyleBuilder::new()
            .stroke_color(stroke_color)
            .stroke_width(stroke_width)
            .build();
        
        let _ = Circle::new(
            Point::new(center.x - diameter as i32 / 2, center.y - diameter as i32 / 2),
            diameter,
        )
        .into_styled(style)
        .draw(&mut self.inner);
    }

    /// Draw centered text at the specified position
    ///
    /// # Arguments
    /// * `text` - The text to display
    /// * `position` - The center position for the text
    /// * `color` - The text color
    pub fn draw_centered_text(&mut self, text: &str, position: Point, color: Rgb565) {
        let style = MonoTextStyle::new(&FONT_10X20, color);
        
        let _ = Text::with_alignment(
            text,
            position,
            style,
            Alignment::Center,
        )
        .draw(&mut self.inner);
    }

    /// Initialize the angle display: draw the background and circles once
    ///
    /// Call this once at startup before calling update_angle_text
    pub fn init_angle_display(&mut self) {
        // Clear screen to black
        self.clear_color(colors::black());
        
        // Calculate center of screen
        let center = Point::new(W / 2, H / 2);
        
        // Draw outer circle (diameter: 180 pixels)
        self.draw_circle(center, 180, colors::cyan(), 4);
        
        // Draw inner circle (diameter: 140 pixels)
        self.draw_circle(center, 140, colors::blue(), 2);
    }

    /// Update only the angle text in the center (efficient, no flicker)
    ///
    /// # Arguments
    /// * `angle` - The angle value to display (0-360)
    /// * `color` - The color to use for the text
    pub fn update_angle_text(&mut self, angle: u16, color: Rgb565) {
        // Calculate center of screen
        let center = Point::new(W / 2, H / 2);
        
        // Clear text area with a black rectangle (60x30 pixels)
        let text_area = Rectangle::new(
            Point::new(center.x - 30, center.y - 15),
            Size::new(60, 30),
        );
        let _ = text_area.into_styled(PrimitiveStyle::with_fill(colors::black()))
            .draw(&mut self.inner);
        
        // Format angle text
        let mut buffer = heapless::String::<8>::new();
        let _ = write!(buffer, "{}°", angle);
        
        // Draw the angle value centered with the specified color
        self.draw_centered_text(&buffer, center, color);
    }

    /// Expose screen size for convenience.
    pub fn size(&self) -> (i32, i32) {
        (W, H)
    }
}

/// Color helper functions
pub mod colors {
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::prelude::RgbColor;

    pub fn red() -> Rgb565 {
        Rgb565::RED
    }

    pub fn green() -> Rgb565 {
        Rgb565::GREEN
    }

    pub fn blue() -> Rgb565 {
        Rgb565::BLUE
    }

    pub fn white() -> Rgb565 {
        Rgb565::WHITE
    }

    pub fn black() -> Rgb565 {
        Rgb565::BLACK
    }

    pub fn cyan() -> Rgb565 {
        Rgb565::CYAN
    }
}
