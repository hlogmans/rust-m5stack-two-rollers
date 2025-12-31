//! Hardware abstraction layer for M5Stack CoreS3
//!
//! This module provides a clean separation between hardware-specific code
//! and business logic. All low-level hardware initialization and interaction
//! is encapsulated here.

pub mod power;
pub mod roller485;
pub mod touch;
pub mod aw9523;
pub mod display;

use alloc::boxed::Box;
use core::cell::RefCell;

use self::display::Display;
use crate::helpers::TelemetrySender;
use crate::{info, warn};
use critical_section::Mutex;
use embassy_time::Timer;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::time::Rate;
use embedded_hal_bus::i2c::CriticalSectionDevice;

pub use roller485::{Roller485, SharedRoller485, MotorCommand};
pub use touch::{FT6336, TouchEvent, TouchPoint, read_touch_data, SharedFT6336};

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

type ManagedI2cBus = Mutex<RefCell<I2c<'static, esp_hal::Blocking>>>;
type ManagedI2cDevice = CriticalSectionDevice<'static, I2c<'static, esp_hal::Blocking>>;
pub type RollerI2cDevice = ManagedI2cDevice;

/// Represents the initialized M5Stack CoreS3 hardware
pub struct Board<'a> {
    pub display: Display<CoreS3Display<'a>>,
    pub roller_a: SharedRoller485<ManagedI2cDevice>,
    pub roller_b: SharedRoller485<ManagedI2cDevice>,
    pub touch: FT6336<I2c<'a, esp_hal::Blocking>>,
    _i2c1_bus: &'static ManagedI2cBus,
}

impl<'a> Board<'a>
where
    'a: 'static,
{
    /// Initialize all hardware components of the M5Stack CoreS3
    /// 
    /// This function handles:
    /// - Power management (AXP2101)
    /// - Display backlight
    /// - Display controller (ILI9342C)
    /// - Two Roller485 motors on I2C1 (Grove Port B)
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
    /// * `i2c1` - I2C1 peripheral (Port B) on GPIO9 (SDA) / GPIO8 (SCL)
    /// * `i2c1_sda` - Port B SDA pin
    /// * `i2c1_scl` - Port B SCL pin
    /// * `angle_sender_a` - Optional channel-agnostic sender for motor A (0x65) angle updates
    /// * `speed_sender_a` - Optional channel-agnostic sender for motor A speed updates
    /// * `angle_sender_b` - Optional channel-agnostic sender for motor B (0x64) angle updates
    /// * `speed_sender_b` - Optional channel-agnostic sender for motor B speed updates
    /// * `command_channel_a` - Channel for motor A commands
    /// * `command_channel_b` - Channel for motor B commands
    /// 
    /// Both motors share the same I2C1 bus on Port B pins; addresses:
    /// - Motor A: 0x65
    /// - Motor B: 0x64
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
        angle_sender_a: Option<TelemetrySender<u16, 8>>,
        speed_sender_a: Option<TelemetrySender<f32, 4>>,
        angle_sender_b: Option<TelemetrySender<u16, 8>>,
        speed_sender_b: Option<TelemetrySender<f32, 4>>,
        command_channel_a: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
        command_channel_b: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
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
        
        // Grove I2C bus (Port B) - Motors at 0x65 (A) and 0x64 (B)
        info!("Initializing I2C1 for Grove port B (motors)...");
        let i2c1_bus: I2c<'static, esp_hal::Blocking> = I2c::new(
            i2c1,
            I2cConfig::default().with_frequency(Rate::from_khz(400)),
        )
        .expect("Failed to create I2C1")
        .with_sda(i2c1_sda)
        .with_scl(i2c1_scl);
        let i2c1_shared: &'static ManagedI2cBus = Box::leak(Box::new(Mutex::new(RefCell::new(i2c1_bus))));

        info!("Initializing Roller485 Motor A at address 0x65 on I2C1 (Port B)...");
        let mut roller_a = Roller485::new_with_address(
            CriticalSectionDevice::new(i2c1_shared),
            0x65,
        );
        let _ = roller_a.init();

        info!("Initializing Roller485 Motor B at address 0x64 on I2C1 (Port B)...");
        let mut roller_b = Roller485::new_with_address(
            CriticalSectionDevice::new(i2c1_shared),
            0x64,
        );
        let _ = roller_b.init();

        // Scan I2C1 bus to confirm motor addresses respond
        let mut scan_proxy = CriticalSectionDevice::new(i2c1_shared);
        scan_i2c_bus(&mut scan_proxy, "I2C1 (motors)");
        
        // Use I2C0 for touch controller on GPIO12/11
        info!("Initializing FT6336 touch controller on I2C0...");
        let touch = FT6336::new(i2c0_released);
        info!("Touch controller ready at address 0x38");
        
        // Wrap motors in SharedRoller485 for async-safe sharing
        let roller_a = SharedRoller485::new(
            roller_a,
            angle_sender_a,
            speed_sender_a,
            command_channel_a,
        );

        let roller_b = SharedRoller485::new(
            roller_b,
            angle_sender_b,
            speed_sender_b,
            command_channel_b,
        );
        
        Self { 
            display, 
            roller_a,
            roller_b,
            touch,
            _i2c1_bus: i2c1_shared,
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
