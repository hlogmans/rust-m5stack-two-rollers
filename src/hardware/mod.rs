//! Hardware abstraction layer for M5Stack CoreS3
//!
//! This module provides a clean separation between hardware-specific code
//! and business logic. All low-level hardware initialization and interaction
//! is encapsulated here.

pub mod power;
pub mod roller485;

use crate::display::{self, Display};
use embassy_time::Timer;
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::time::Rate;
use log::info;

pub use roller485::Roller485;

/// Type alias for the display driver used on M5Stack CoreS3
pub type CoreS3Display<'a> = mipidsi::Display<
    mipidsi::interface::SpiInterface<
        'a,
        embedded_hal_bus::spi::ExclusiveDevice<
            esp_hal::spi::master::Spi<'a, esp_hal::Blocking>,
            esp_hal::gpio::Output<'a>,
            embedded_hal_bus::spi::NoDelay,
        >,
        esp_hal::gpio::Output<'a>,
    >,
    mipidsi::models::ILI9342CRgb565,
    esp_hal::gpio::Output<'a>,
>;

/// Represents the initialized M5Stack CoreS3 hardware
pub struct Board<'a> {
    pub display: Display<CoreS3Display<'a>>,
    pub roller485: Roller485<I2c<'a, esp_hal::Blocking>>,
}

impl<'a> Board<'a> {
    /// Initialize all hardware components of the M5Stack CoreS3
    /// 
    /// This function handles:
    /// - Power management (AXP2101)
    /// - Display backlight
    /// - Display controller (ILI9342C)
    /// 
    /// Note: This function takes ownership of all used peripherals.
    /// For other peripherals (like TIMG0), keep references before calling init.
    /// 
    /// # Arguments
    /// * `i2c0` - I2C0 peripheral (for AXP2101)
    /// * `sda` - GPIO12 for I2C SDA
    /// * `scl` - GPIO11 for I2C SCL
    /// * `spi2` - SPI2 peripheral
    /// * `gpio_mosi` - GPIO37 for SPI MOSI
    /// * `gpio_sck` - GPIO36 for SPI SCK
    /// * `gpio_cs` - GPIO3 for SPI CS
    /// * `gpio_dc` - GPIO35 for display DC
    /// * `gpio_rst` - GPIO15 for display RST
    /// * `display_buffer` - Buffer for display SPI transfers (min 512 bytes)
    /// * `i2c1` - I2C1 peripheral (Port A/Grove) on GPIO2 (SDA) / GPIO1 (SCL)
    /// 
    /// # Returns
    /// A `Board` struct containing all initialized hardware
    pub async fn init(
        i2c0: esp_hal::peripherals::I2C0<'a>,
        sda: esp_hal::peripherals::GPIO12<'a>,
        scl: esp_hal::peripherals::GPIO11<'a>,
        spi2: esp_hal::peripherals::SPI2<'a>,
        gpio_mosi: esp_hal::peripherals::GPIO37<'a>,
        gpio_sck: esp_hal::peripherals::GPIO36<'a>,
        gpio_cs: esp_hal::peripherals::GPIO3<'a>,
        gpio_dc: esp_hal::peripherals::GPIO35<'a>,
        gpio_rst: esp_hal::peripherals::GPIO15<'a>,
        display_buffer: &'a mut [u8; 512],
        i2c1: esp_hal::peripherals::I2C1<'a>,
        gpio_ext_sda: esp_hal::peripherals::GPIO2<'a>,
        gpio_ext_scl: esp_hal::peripherals::GPIO1<'a>,
    ) -> Self {
        info!("Initializing M5Stack CoreS3 hardware...");

        // Create I2C0 bus on GPIO12/11 for AXP2101 and AW9523
        let i2c_bus = I2c::new(
            i2c0,
            I2cConfig::default().with_frequency(Rate::from_khz(100)),
        )
        .expect("Failed to create I2C0")
        .with_sda(sda)
        .with_scl(scl);

        // Initialize power management (AXP2101) and display control (AW9523)
        power::init_power_and_display_control(i2c_bus);
        
        // Small delay for power stabilization
        Timer::after_millis(100).await;
        
        // Initialize display
        info!("Initializing display...");
        let disp_pins = display::DisplayPeripherals {
            spi2,
            gpio_mosi,
            gpio_sck,
            gpio_cs,
            gpio_dc,
            gpio_rst,
        };
        let display = display::init(disp_pins, display_buffer);
        
                // External Grove I2C bus on Port A (GPIO2 SDA, GPIO1 SCL)
                info!("Initializing I2C1 for Grove Port A...");
                let i2c1_bus = I2c::new(
                        i2c1,
                        I2cConfig::default().with_frequency(Rate::from_khz(100)),
                )
                .expect("Failed to create I2C1")
                .with_sda(gpio_ext_sda)
                .with_scl(gpio_ext_scl);

                // Initialize Roller485 on Grove Port A
                let mut roller485 = Roller485::new(i2c1_bus);
          if let Err(e) = roller485.init() {
            log::error!("Failed to initialize Roller485: {:?}", e);
        }
        
        info!("M5Stack CoreS3 hardware initialization complete");
        
        Self { display, roller485 }
    }
}
