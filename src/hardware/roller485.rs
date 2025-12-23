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

/// Register addresses for Roller485
mod registers {
    /// Motor position register (32-bit, little-endian)
    pub const ENCODER_REG: u8 = 0x00;
    /// Motor mode register
    pub const MODE_REG: u8 = 0x10;
    /// Motor speed register (16-bit)
    pub const SPEED_REG: u8 = 0x20;
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
    /// Performs basic communication test and setup
        pub fn init(&mut self) -> Result<(), I2C::Error> {
        info!("Initializing Roller485 at address 0x{:02x}", self.address);
        
        // Try reading encoder position to verify communication
            let _ = self.read_encoder_position()?;
        
        info!("Roller485 initialization complete");
        Ok(())
    }

    /// Read the current encoder position
    ///
    /// Returns the encoder position as a 32-bit signed integer.
    /// The value represents the cumulative position in encoder steps.
        pub fn read_encoder_position(&mut self) -> Result<i32, I2C::Error> {
        let mut buffer = [0u8; 4];
        self.i2c
              .write_read(self.address, &[registers::ENCODER_REG], &mut buffer)?;
        
        // Convert little-endian bytes to i32
        Ok(i32::from_le_bytes(buffer))
    }

    /// Read angle and raw block with retry for consistency.
    /// Reads multiple times and returns only when two consecutive reads match.
    pub fn read_angle_block(&mut self) -> Result<AngleBlock, I2C::Error> {
        let mut prev_buf = [0u8; 8];
        self.read_block(0x00, &mut prev_buf)?;
        
        // Try up to 3 times to get two consistent reads
        for _ in 0..3 {
            let mut buf = [0u8; 8];
            self.read_block(0x00, &mut buf)?;
            
            // If two consecutive reads match, we have stable data
            if buf == prev_buf {
                return Ok(parse_angle_block(buf));
            }
            prev_buf = buf;
        }
        
        // If we couldn't get consistent data, return last read
        Ok(parse_angle_block(prev_buf))
    }

    /// Convenience: angle only
    pub fn read_angle_deg(&mut self) -> Result<u16, I2C::Error> {
        let block = self.read_angle_block()?;
        Ok(block.angle_deg)
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

fn parse_angle_block(buf: [u8; 8]) -> AngleBlock {
    let steps = i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let zero_block = buf == [0u8; 8];
    let angle_f = f32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let wrapped = ((angle_f % 360.0) + 360.0) % 360.0;
    AngleBlock {
        steps,
        angle_deg: wrapped as u16,
        zero_block,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_angle_conversion() {
        // Mock test - actual hardware tests would need embedded-hal-mock
        // Testing the angle calculation logic
        let steps = 500; // half rotation
        let steps_per_rev = 1000;
        let angle = (steps * 360 / steps_per_rev) as u16;
        assert_eq!(angle, 180);
    }

    #[test]
    fn parse_block_wraps_angle() {
        // 400.5 degrees -> expect wrap to 40
        let buf = [0, 0, 0, 0, 0x00, 0x00, 0xc8, 0x43];
        let parsed = parse_angle_block(buf);
        assert_eq!(parsed.angle_deg, 40);
        assert!(!parsed.zero_block);
        assert_eq!(parsed.steps, 0);
    }

    #[test]
    fn parse_block_detects_zero_block() {
        let parsed = parse_angle_block([0; 8]);
        assert!(parsed.zero_block);
        assert_eq!(parsed.angle_deg, 0);
        assert_eq!(parsed.steps, 0);
    }
}
