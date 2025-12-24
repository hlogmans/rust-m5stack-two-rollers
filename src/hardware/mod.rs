//! Hardware abstraction layer for M5Stack CoreS3
//!
//! This module provides a clean separation between hardware-specific code
//! and business logic. All low-level hardware initialization and interaction
//! is encapsulated here.

pub mod power;
pub mod roller485;
pub mod touch;

use crate::display::{self, Display};
use crate::{info, warn};
use embassy_time::Timer;
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::time::Rate;

pub use roller485::Roller485;
pub use touch::{FT6336, TouchEvent, TouchPoint, read_touch_data};

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

/// Simple wrapper for Motor B that holds its address but shares I2C with Motor A
pub struct MotorB {
    pub address: u8,
}

/// Represents the initialized M5Stack CoreS3 hardware
pub struct Board<'a> {
    pub display: Display<CoreS3Display<'a>>,
    pub roller_a: Roller485<I2c<'a, esp_hal::Blocking>>,
    pub roller_b: MotorB,
    pub touch: FT6336<I2c<'a, esp_hal::Blocking>>,
}

impl<'a> Board<'a> {
    /// Initialize all hardware components of the M5Stack CoreS3
    /// 
    /// This function handles:
    /// - Power management (AXP2101)
    /// - Display backlight
    /// - Display controller (ILI9342C)
    /// - Roller485 motor on I2C1 (Grove Port A)
    /// 
    /// Note: This function takes ownership of all used peripherals.
    /// For other peripherals (like TIMG0), keep references before calling init.
    /// 
    /// # Arguments
    /// * `i2c0` - I2C0 peripheral (for AXP2101, then Motor A)
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
    /// * `gpio_ext_sda` - GPIO2 for I2C1 SDA (Motor A)
    /// * `gpio_ext_scl` - GPIO1 for I2C1 SCL (Motor A)
    /// 
    /// Both Motor A (address 0x64) and Motor B (address 0x65) share the same
    /// I2C1 bus on Port A pins for simplicity.
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
        i2c1_sda: impl esp_hal::gpio::interconnect::PeripheralOutput<'a>,
        i2c1_scl: impl esp_hal::gpio::interconnect::PeripheralOutput<'a>,
    ) -> Self {
        info!("Initializing M5Stack CoreS3 hardware...");

        // Create I2C0 bus on GPIO12/11 for AXP2101 and AW9523
        let i2c0_bus = I2c::new(
            i2c0,
            I2cConfig::default().with_frequency(Rate::from_khz(400)),
        )
        .expect("Failed to create I2C0")
        .with_sda(sda)
        .with_scl(scl);

        // Initialize power management (AXP2101) and display control (AW9523).
        // This returns the bus back after configuration so we can optionally scan it.
        let mut i2c0_released = power::init_power_and_display_control(i2c0_bus);

        // Optional: scan I2C0 and log responding addresses (helps diagnose ACK issues)
        scan_i2c_bus(&mut i2c0_released, "I2C0");
        
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
        
        // Grove I2C bus (selected pins) - Motors
        info!("Initializing I2C1 for Grove port (motors)...");
        // Motor A at address 0x64 on I2C1
        info!("Initializing Roller485 Motor A at address 0x64 on I2C1...");
        let i2c1_bus = I2c::new(
            i2c1,
            I2cConfig::default().with_frequency(Rate::from_khz(400)),
        )
        .expect("Failed to create I2C1")
        .with_sda(i2c1_sda)
        .with_scl(i2c1_scl);

        let mut roller_a = Roller485::new(i2c1_bus);
        let _ = roller_a.init();
        // Scan I2C1 bus to confirm motor addresses respond
        scan_i2c_bus(&mut roller_a.i2c, "I2C1 (motors)");
        
        // Motor B at address 0x65 - commands via Motor A's shared I2C1 bus
        // Both motors are on the same I2C bus (Port A pins) but at different addresses
        info!("Initializing Motor B at address 0x65 on shared I2C1 bus...");
        let _ = roller_a.enable_motor_b();
        let _ = roller_a.set_motor_b_encoder_mode();
        info!("Motor B initialized at address 0x65");
        
        // Use I2C0 for touch controller on GPIO12/11
        info!("Initializing FT6336 touch controller on I2C0...");
        let touch = FT6336::new(i2c0_released);
        info!("Touch controller ready at address 0x38");
        
        Self { 
            display, 
            roller_a, 
            roller_b: MotorB { address: 0x65 },
            touch 
        }
    }
}

/// Scan an I2C bus for responding device addresses and log them.
fn scan_i2c_bus<I2C>(bus: &mut I2C, label: &str)
where
    I2C: embedded_hal::i2c::I2c,
{
    info!("Scanning {} for I2C devices...", label);
    let mut found = 0u8;
    for addr in 0x08..=0x77 {
        // Try a zero-length write; if device ACKs, it's present.
        match bus.write(addr, &[]) {
            Ok(()) => {
                found = found.wrapping_add(1);
                info!("{}: found device at 0x{:02x}", label, addr);
            }
            Err(_) => {
                // Ignore NACK/other errors; continue scanning
            }
        }
    }
    if found == 0 {
        warn!("{}: no I2C devices responded", label);
    } else {
        info!("{}: {} device(s) responded", label, found);
    }
}
