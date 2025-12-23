//! Power management abstraction for M5Stack CoreS3
//!
//! The M5Stack CoreS3 uses an AXP2101 power management IC to control
//! various power rails, including the display backlight.
//!
//! Hardware configuration:
//! - I2C Address: 0x34
//! - I2C SDA: GPIO12
//! - I2C SCL: GPIO11
//! - Display backlight: DLDO1 @ 3.3V

use axp2101_dd::LdoId;
use embassy_time::Timer;
use esp_hal::i2c::master::I2c;
use esp_hal::peripherals::{GPIO11, GPIO12, I2C0};
use log::info;

/// Initialize power management and enable display backlight
///
/// This function:
/// 1. Sets up I2C communication with the AXP2101
/// 2. Configures DLDO1 to 3.3V for display backlight
/// 3. Enables the backlight
///
/// # Arguments
/// * `i2c0` - I2C0 peripheral
/// * `sda` - GPIO12 for I2C SDA
/// * `scl` - GPIO11 for I2C SCL
pub async fn init_power_and_backlight(i2c0: I2C0<'_>, sda: GPIO12<'_>, scl: GPIO11<'_>) {
    info!("Initializing I2C bus for power management...");
    let i2c_bus = I2c::new(
        i2c0,
        esp_hal::i2c::master::Config::default()
            .with_frequency(esp_hal::time::Rate::from_hz(400_000)),
    )
    .expect("Failed to create I2C")
    .with_sda(sda)
    .with_scl(scl)
    .into_async();

    info!("Creating AXP2101 instance...");
    let mut axp = axp2101_dd::Axp2101Async::new(i2c_bus);

    info!("Configuring DLDO1 for display backlight (3.3V)...");
    
    // Try to set DLDO1 voltage with error handling
    info!("Setting DLDO1 voltage...");
    match axp.set_ldo_voltage_mv(LdoId::Dldo1, 3300).await {
        Ok(_) => {
            info!("DLDO1 voltage set to 3.3V");
            
            Timer::after_millis(10).await;
            
            // Try to enable DLDO1
            info!("Enabling DLDO1...");
            match axp.set_ldo_enable(LdoId::Dldo1, true).await {
                Ok(_) => info!("DLDO1 enabled - backlight should be on"),
                Err(e) => {
                    log::error!("Failed to enable DLDO1: {:?}", e);
                    info!("Continuing without backlight...");
                }
            }
        }
        Err(e) => {
            log::error!("Failed to set DLDO1 voltage: {:?}", e);
            info!("AXP2101 communication error, continuing without backlight initialization...");
            // Even if AXP2101 fails, the display might still work if it has external power
        }
    }
    
    info!("Power management initialization complete");
}
