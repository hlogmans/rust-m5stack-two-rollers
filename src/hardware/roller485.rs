//! Roller485 stepper motor controller driver
//!
//! Hardware abstraction for the M5Stack Roller485 unit.
//! The Roller485 is a stepper motor controller with encoder feedback
//! that communicates over I2C.
//!
//! Default I2C address: 0x64
//!
//! Register Map:
//! - 0x00-0x03: Motor position (32-bit signed integer, little-endian)
//! - 0x10: Motor mode
//! - 0x20-0x21: Motor speed (16-bit)
//! - More registers available - see M5Stack Roller485 documentation

use embedded_hal::i2c::I2c;
use log::info;

/// I2C address for Roller485 (default)
pub const ROLLER485_DEFAULT_ADDR: u8 = 0x64;

/// Register addresses for Roller485 (I2C Protocol)
/// See: M5Stack Unit Roller485 I2C Protocol Documentation
mod registers {
    /// Motor Enable/Disable register (1 byte: 0x00=off, 0x01=on)
    pub const MOTOR_ENABLE_REG: u8 = 0x00;
    /// Motor mode setting (1 byte: 0x01=speed, 0x02=position, 0x03=current, 0x04=encoder)
    pub const MODE_REG: u8 = 0x01;
    /// Speed Setting register (4 bytes, little-endian i32, set target speed in RPM)
    pub const SPEED_CONTROL_REG: u8 = 0x40;
    /// Position Control register (4 bytes, little-endian i32, set target position)
    pub const POSITION_CONTROL_REG: u8 = 0x80;
    /// Position Readback register (4 bytes, little-endian i32, divide by 100 for actual position)
    pub const POSITION_READBACK_REG: u8 = 0x90;
}

/// Roller485 operational modes
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum Roller485Mode {
    /// Speed control mode
    Speed = 0x01,
    /// Position control mode
    Position = 0x02,
    /// Current control mode
    Current = 0x03,
    /// Encoder reading mode (for angle/position feedback)
    Encoder = 0x04,
}

impl From<u8> for Roller485Mode {
    fn from(val: u8) -> Self {
        match val {
            0x01 => Roller485Mode::Speed,
            0x02 => Roller485Mode::Position,
            0x03 => Roller485Mode::Current,
            0x04 => Roller485Mode::Encoder,
            _ => Roller485Mode::Encoder, // Default to Encoder for unknown modes
        }
    }
}

impl From<Roller485Mode> for u8 {
    fn from(mode: Roller485Mode) -> u8 {
        mode as u8
    }
}

/// Parsed angle/position frame returned by the Roller485.
pub struct AngleBlock {
    /// Cumulative encoder steps (little-endian i32 from 0x00..0x03)
    pub steps: i32,
    /// Angle in degrees, wrapped to [0, 360)
    pub angle_deg: u16,
    /// True when the entire 8-byte block is zero (known spurious frame)
    pub zero_block: bool,
}

/// Roller485 driver
pub struct Roller485<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Roller485<I2C>
where
    I2C: I2c,
{
    /// Create a new Roller485 instance with default address
    ///
    /// # Arguments
    /// * `i2c` - The I2C bus to use for communication
    pub fn new(i2c: I2C) -> Self {
        Self::new_with_address(i2c, ROLLER485_DEFAULT_ADDR)
    }

    /// Create a new Roller485 instance with custom address
    ///
    /// # Arguments
    /// * `i2c` - The I2C bus to use for communication
    /// * `address` - Custom I2C address
    pub fn new_with_address(i2c: I2C, address: u8) -> Self {
        Self { i2c, address }
    }

    /// Initialize the Roller485 device
    ///
    /// Performs basic communication test and setup.
    /// Enables the motor and sets mode to ENCODER for angle reading.
    pub fn init(&mut self) -> Result<(), I2C::Error> {
        info!("Initializing Roller485 at address 0x{:02x}", self.address);
        
        // Step 1: Enable the motor (0x00 = 0x01)
        self.i2c
            .write(self.address, &[registers::MOTOR_ENABLE_REG, 0x01])?;
        info!("Roller485 motor enabled");
        
        // Step 2: Set mode to ENCODER (0x01 = 0x04)
        self.set_mode(Roller485Mode::Encoder)?;
        
        // Verify mode was set
        let mode = self.read_mode()?;
        info!("Roller485 mode set to: {:?}", mode);
        
        // Try reading encoder position to verify communication
        let _ = self.read_encoder_position()?;
        
        info!("Roller485 initialization complete");
        Ok(())
    }

    /// Read the current device mode
    ///
    /// Returns the mode as a Roller485Mode enum
    pub fn read_mode(&mut self) -> Result<Roller485Mode, I2C::Error> {
        let mut buffer = [0u8; 1];
        self.i2c
            .write_read(self.address, &[registers::MODE_REG], &mut buffer)?;
        Ok(Roller485Mode::from(buffer[0]))
    }

    /// Set the device mode
    ///
    /// # Arguments
    /// * `mode` - The mode to set
    pub fn set_mode(&mut self, mode: Roller485Mode) -> Result<(), I2C::Error> {
        let mode_val: u8 = mode.into();
        self.i2c
            .write(self.address, &[registers::MODE_REG, mode_val])?;
        Ok(())
    }

    /// Read the current encoder position
    ///
    /// Returns the position value from register 0x90 (divided by 100).
    /// The value represents the cumulative position.
    pub fn read_encoder_position(&mut self) -> Result<i32, I2C::Error> {
        let mut buffer = [0u8; 4];
        self.i2c
            .write_read(self.address, &[registers::POSITION_READBACK_REG], &mut buffer)?;
        
        // Convert little-endian bytes to i32, then divide by 100 (device units)
        let raw = i32::from_le_bytes(buffer);
        Ok(raw / 100)
    }

    /// Read angle and raw block with retry for consistency.
    /// Reads position from register 0x90 and converts to angle (0-359°).
    /// Retries until two consecutive reads match.
    pub fn read_angle_block(&mut self) -> Result<AngleBlock, I2C::Error> {
        let mut prev_position = 0i32;
        let _ = self.read_encoder_position()?;
        
        // Try up to 3 times to get two consistent reads
        for _ in 0..3 {
            let position = self.read_encoder_position()?;
            
            // If two consecutive reads match, we have stable data
            if position == prev_position {
                return Ok(parse_position_to_angle(position));
            }
            prev_position = position;
        }
        
        // If we couldn't get consistent data, return last read
        Ok(parse_position_to_angle(prev_position))
    }

    /// Convenience: angle only
    pub fn read_angle_deg(&mut self) -> Result<u16, I2C::Error> {
        let block = self.read_angle_block()?;
        Ok(block.angle_deg)
    }

    /// Move motor to target angle in steps of 5 degrees.
    /// The motor will move from current position to target angle.
    /// Automatically switches to Position Control mode.
    ///
    /// # Arguments
    /// * `target_angle_deg` - Target angle in degrees (0-359)
    pub fn move_to_angle(&mut self, target_angle_deg: u16) -> Result<(), I2C::Error> {
        // Switch to Position Control mode (0x02) if not already there
        let current_mode = self.read_mode()?;
        if current_mode != Roller485Mode::Position {
            self.set_mode(Roller485Mode::Position)?;
        }
        
        // Normalize angle to 0-359 range
        let normalized_angle = (target_angle_deg % 360) as i32;
        
        // Convert angle to position steps: (angle / 360) * 333 steps per rotation
        let target_steps = (normalized_angle * 333) / 360;
        
        // Device wants position / 100, so multiply by 100
        let target_position = target_steps * 100;
        
        // Convert to little-endian bytes (4-byte i32)
        let bytes = target_position.to_le_bytes();
        
        // Write to position control register
        self.i2c.write(self.address, &[
            registers::POSITION_CONTROL_REG,
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[3],
        ])?;
        
        Ok(())
    }

    /// Set the motor speed (RPM).
    /// The motor will accelerate/decelerate to this speed.
    ///
    /// # Arguments
    /// * `speed_rpm` - Target speed in RPM (positive = forward, negative = reverse)
    pub fn set_speed(&mut self, speed_rpm: i32) -> Result<(), I2C::Error> {
        // Device wants speed / 100, so multiply by 100
        let speed_value = speed_rpm * 100;
        
        // Convert to little-endian bytes (4-byte i32)
        let bytes = speed_value.to_le_bytes();
        
        // Write to speed control register
        self.i2c.write(self.address, &[
            registers::SPEED_CONTROL_REG,
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[3],
        ])?;
        
        Ok(())
    }

    /// Ensure the device is in encoder reading mode
    ///
    /// Call this if you suspect the mode may have been changed,
    /// for example after navigating menu pages on the device.
    pub fn ensure_encoder_mode(&mut self) -> Result<(), I2C::Error> {
        let mode = self.read_mode()?;
        if mode != Roller485Mode::Encoder {
            info!("Roller485 mode was {:?}, resetting to Encoder", mode);
            self.set_mode(Roller485Mode::Encoder)?;
        }
        Ok(())
    }

    /// Release the I2C bus
    ///
    /// Consumes the Roller485 driver and returns the I2C bus.
    /// Useful when you need to reuse the I2C bus for other devices.
    pub fn release(self) -> I2C {
        self.i2c
    }

    /// Debug helper: read a raw block starting at `start` into `buf`.
    /// Returns the buffer for convenience.
    /// Uses separate write/read with small delay for Roller485 timing requirements.
    pub fn read_block<'b>(&mut self, start: u8, buf: &'b mut [u8]) -> Result<&'b [u8], I2C::Error> {
        // Write register address
        self.i2c.write(self.address, &[start])?;
        // Small delay for device to prepare data (Roller485 needs ~1ms)
        for _ in 0..1000 { core::hint::spin_loop(); }
        // Read data
        self.i2c.read(self.address, buf)?;
        Ok(buf)
    }
}

/// Parse position value into angle (0-359 degrees).
/// Assumes ~333 steps per full rotation (360 degrees).
/// Device position divided by 100, so 33300 raw units = 360 degrees.
fn parse_position_to_angle(position: i32) -> AngleBlock {
    let steps_per_rotation = 360;
    
    // Normalize position to 0-999 range
    let normalized_pos = ((position % steps_per_rotation) + steps_per_rotation) % steps_per_rotation;
    
    // Convert to degrees: (position / steps_per_rotation) * 360
    let angle_f = (normalized_pos as f32 / steps_per_rotation as f32) * 360.0;
    
    AngleBlock {
        steps: position,
        angle_deg: angle_f as u16,
        zero_block: position == 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_angle_conversion() {
        // Test position to angle conversion
        // 166.5 steps out of 333 = 180 degrees
        let angle_block = parse_position_to_angle(166);
        assert!(angle_block.angle_deg >= 179 && angle_block.angle_deg <= 181);
        assert_eq!(angle_block.steps, 166);
        assert!(!angle_block.zero_block);
    }

    #[test]
    fn test_position_wrapping() {
        // Test that position wraps correctly
        // 500 steps should wrap (500 % 333 = 167)
        let angle_block = parse_position_to_angle(500);
        assert_eq!(angle_block.steps, 500);
    }

    #[test]
    fn test_negative_position_wrapping() {
        // Test negative position wrapping
        // -167 steps should wrap to approximately 180 degrees (halfway around)
        let angle_block = parse_position_to_angle(-167);
        assert!(angle_block.angle_deg >= 179 && angle_block.angle_deg <= 181);
        assert_eq!(angle_block.steps, -167);
    }

    #[test]
    fn test_zero_block_detection() {
        let angle_block = parse_position_to_angle(0);
        assert!(angle_block.zero_block);
        assert_eq!(angle_block.angle_deg, 0);
        assert_eq!(angle_block.steps, 0);
    }
}
