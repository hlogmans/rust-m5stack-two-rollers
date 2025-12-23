//! Power management abstraction for M5Stack CoreS3
//!
//! The M5Stack CoreS3 uses an AXP2101 power management IC to control
//! various power rails, and an AW9523 I/O expander to control LCD reset,
//! backlight enable, and Grove port power enables.
//!
//! Hardware configuration:
//! - AXP2101 I2C Address: 0x34
//! - AW9523 I2C Address: 0x58  
//! - I2C SDA: GPIO12
//! - I2C SCL: GPIO11
//! 
//! AW9523 GPIO functions (from M5Unified source):
//! - Port 0 (register 0x02):
//!   - Bit 1 (0b00000010): BUS_OUT_EN - enables power to Grove ports
//!   - Bit 5 (0b00100000): USB_OTG_EN - enables USB OTG power
//! - Port 1 (register 0x03):
//!   - Bit 7 (0b10000000): BOOST_EN - enables power boost
//!
//! Reference: https://github.com/m5stack/M5Unified/blob/master/src/utility/Power_Class.cpp

use axp2101::{Axp2101, I2CPowerManagementInterface};
use aw9523::{Aw9523, I2CGpioExpanderInterface};
use esp_hal::i2c::master::I2c;
use log::{info, error};

/// Initialize power management, GPIO expander, display, and Grove port power
///
/// This function implements the M5Stack CoreS3 power initialization sequence:
/// 1. Initialize AXP2101 PMU (power management unit)
/// 2. Release I2C bus from AXP2101  
/// 3. Initialize AW9523 I/O expander (LCD control + Grove port power)
///
/// # Arguments
/// * `i2c_bus` - I2C bus on GPIO12/11 (any lifetime)
pub fn init_power_and_display_control<'a>(i2c_bus: I2c<'a, esp_hal::Blocking>) {
    info!("Initializing AXP2101 PMU...");
    
    // Initialize AXP2101 with interface wrapper
    let axp_interface = I2CPowerManagementInterface::new(i2c_bus);
    let mut axp = Axp2101::new(axp_interface);
    
    match axp.init() {
        Ok(_) => info!("AXP2101 initialized successfully"),
        Err(e) => error!("AXP2101 initialization failed (continuing): {:?}", e),
    }
    
    // Release I2C bus from AXP2101 to configure AW9523
    let i2c_bus = axp.release_i2c();
    
    info!("Initializing AW9523 GPIO Expander (LCD + Grove port power)...");
    
    // Initialize AW9523 with the I2C interface
    let aw_interface = I2CGpioExpanderInterface::new(i2c_bus);
    let mut aw = Aw9523::new(aw_interface);
    
    match aw.init() {
        Ok(_) => {
            info!("AW9523 initialized successfully");
            info!("  - LCD reset and backlight control enabled");
            info!("  - Grove port power (BUS_OUT_EN) enabled");
            info!("  - Power boost (BOOST_EN) enabled");
        }
        Err(e) => error!("AW9523 initialization failed: {:?}", e),
    }
    
    info!("Power and display control initialization complete");
}
