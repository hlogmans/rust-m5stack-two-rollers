//! Touch controller driver for FT6336 on M5Stack CoreS3
//!
//! The FT6336 is a capacitive touch controller connected via I2C at address 0x38.
//! This module provides both a driver struct for exclusive I2C access and a standalone
//! function for reading touch data via a shared I2C reference.

use embedded_hal::i2c::I2c;
use crate::{debug, info};

/// I2C address of the FT6336 touch controller
const FT6336_ADDR: u8 = 0x38;

/// FT6336 register addresses
mod registers {
    pub const TOUCH_POINTS: u8 = 0x02;
    // First touch point starts at 0x03
    pub const P1_XH: u8 = 0x03;  // Event flags + X high bits
    pub const _P1_XL: u8 = 0x04;  // X low bits, not used
    pub const _P1_YH: u8 = 0x05;  // Y high bits, not used
    pub const _P1_YL: u8 = 0x06;  // Y low bits, not used
}

/// Touch event type
#[derive(Debug, Clone, Copy)]
pub enum TouchEvent {
    /// Touch press
    Press,
    /// Touch release
    Release,
    /// Touch contact (ongoing)
    Contact,
}

/// Represents a single touch point
#[derive(Debug, Clone, Copy)]
pub struct TouchPoint {
    /// X coordinate (0-319 for 320-pixel wide display)
    pub x: u16,
    /// Y coordinate (0-239 for 240-pixel tall display)
    pub y: u16,
    /// Touch event type
    pub event: TouchEvent,
}

/// FT6336 touch controller driver
pub struct FT6336<I2C> {
    i2c: I2C,
}

impl<I2C> FT6336<I2C>
where
    I2C: I2c,
{
    /// Create a new FT6336 driver instance
    pub fn new(i2c: I2C) -> Self {
        Self { i2c }
    }

    /// Read touch data from the controller
    ///
    /// Returns the number of touch points detected and their coordinates.
    /// Returns Ok(None) if no touch is detected.
    pub fn read_touch(&mut self) -> Result<Option<TouchPoint>, I2C::Error> {
        // Read touch point count register
        let mut status = [0u8; 1];
        self.i2c.write_read(FT6336_ADDR, &[registers::TOUCH_POINTS], &mut status)?;
        
        let touch_count = status[0] & 0x0F;
        
        if touch_count == 0 {
            return Ok(None);
        }

        // Read first touch point data (4 bytes starting at 0x03)
        let mut touch_data = [0u8; 4];
        self.i2c.write_read(FT6336_ADDR, &[registers::P1_XH], &mut touch_data)?;

        // Parse event type from high 2 bits of first byte
        let event_byte = touch_data[0];
        let event_type = (event_byte >> 6) & 0x03;
        
        let event = match event_type {
            0 => TouchEvent::Press,
            1 => TouchEvent::Release,
            2 => TouchEvent::Contact,
            _ => TouchEvent::Press,
        };

        // Extract X coordinate: bits [3:0] of byte 0 are X[11:8], byte 1 is X[7:0]
        let x = (((touch_data[0] & 0x0F) as u16) << 8) | (touch_data[1] as u16);
        
        // Extract Y coordinate: bits [3:0] of byte 2 are Y[11:8], byte 3 is Y[7:0]
        let y = (((touch_data[2] & 0x0F) as u16) << 8) | (touch_data[3] as u16);

        debug!("Touch detected: x={}, y={}, event={:?}", x, y, event);

        Ok(Some(TouchPoint { x, y, event }))
    }
}

/// Read touch data from the FT6336 controller
///
/// # Arguments
/// * `i2c` - I2C bus to communicate on
///
/// Returns the number of touch points detected and their coordinates.
/// Returns Ok(None) if no touch is detected.
pub fn read_touch_data<I2C>(i2c: &mut I2C) -> Result<Option<TouchPoint>, I2C::Error>
where
    I2C: I2c,
{
    // Read touch point count register
    let mut status = [0u8; 1];
    match i2c.write_read(FT6336_ADDR, &[registers::TOUCH_POINTS], &mut status) {
        Ok(()) => {
            debug!("Touch: read status OK, byte=0x{:02x}", status[0]);
        }
        Err(e) => {
            info!("Touch: I2C error reading status");
            return Err(e);
        }
    }
    
    let touch_count = status[0] & 0x0F;
    
    if touch_count == 0 {
        return Ok(None);
    }

    info!("Touch: {} point(s) detected", touch_count);

    // Read first touch point data (4 bytes starting at 0x03)
    let mut touch_data = [0u8; 4];
    match i2c.write_read(FT6336_ADDR, &[registers::P1_XH], &mut touch_data) {
        Ok(()) => {
            debug!("Touch data: {:?}", touch_data);
        }
        Err(e) => {
            info!("Touch: I2C error reading data");
            return Err(e);
        }
    }

    // Parse event type from high 2 bits of first byte
    let event_byte = touch_data[0];
    let event_type = (event_byte >> 6) & 0x03;
    
    let event = match event_type {
        0 => TouchEvent::Press,
        1 => TouchEvent::Release,
        2 => TouchEvent::Contact,
        _ => TouchEvent::Press,
    };

    // Extract X coordinate: bits [3:0] of byte 0 are X[11:8], byte 1 is X[7:0]
    let x = (((touch_data[0] & 0x0F) as u16) << 8) | (touch_data[1] as u16);
    
    // Extract Y coordinate: bits [3:0] of byte 2 are Y[11:8], byte 3 is Y[7:0]
    let y = (((touch_data[2] & 0x0F) as u16) << 8) | (touch_data[3] as u16);

    info!("Touch: x={}, y={}", x, y);

    Ok(Some(TouchPoint { x, y, event }))
}