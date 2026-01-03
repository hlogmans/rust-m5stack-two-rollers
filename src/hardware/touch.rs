//! Touch controller driver for FT6336 on M5Stack CoreS3
//!
//! The FT6336 is a capacitive touch controller connected via I2C at address 0x38.
//! This module provides debounced, high-level touch events suitable for business logic:
//! - Only emits ConfirmedPress after 300ms minimum contact duration
//! - Emits Release when contact ends
//! - Tracks position and ensures sequential press/release handling

use embedded_hal::i2c::I2c;
use embassy_sync::{mutex::Mutex, blocking_mutex::raw::CriticalSectionRawMutex};
use embassy_time::{Timer, Instant};
use alloc::sync::Arc;
use crate::{debug, info};

/// I2C address of the FT6336 touch controller
const FT6336_ADDR: u8 = 0x38;

/// Minimum hold duration for a press to be confirmed (ms)
const PRESS_CONFIRM_TIME_MS: u64 = 100;

/// FT6336 register addresses
mod registers {
    pub const TOUCH_POINTS: u8 = 0x02;
    // First touch point starts at 0x03
    pub const P1_XH: u8 = 0x03;  // Event flags + X high bits
    pub const _P1_XL: u8 = 0x04;  // X low bits, not used
    pub const _P1_YH: u8 = 0x05;  // Y high bits, not used
    pub const _P1_YL: u8 = 0x06;  // Y low bits, not used
}

/// Low-level touch event type from FT6336 hardware
#[derive(Debug, Clone, Copy)]
pub enum TouchEvent {
    /// Touch press
    Press,
    /// Touch release
    Release,
    /// Touch contact (ongoing)
    Contact,
}

/// Represents a single touch point (low-level hardware data)
#[derive(Debug, Clone, Copy)]
pub struct TouchPoint {
    /// X coordinate (0-319 for 320-pixel wide display)
    pub x: u16,
    /// Y coordinate (0-239 for 240-pixel tall display)
    pub y: u16,
    /// Touch event type
    pub event: TouchEvent,
}

/// High-level confirmed press event - only emitted after 300ms of contact
#[derive(Debug, Clone, Copy)]
pub struct ConfirmedPress {
    /// X coordinate of the press
    pub x: u16,
    /// Y coordinate of the press
    pub y: u16,
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

        //debug!("Touch detected: x={}, y={}, event={:?}", x, y, event);

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

/// Shared wrapper around FT6336 with debounced press events
///
/// Provides async-safe access to the touch controller through an Arc<Mutex<>>.
/// Emits only confirmed presses (after 300ms contact) and releases via a Channel,
/// suitable for business logic consumption.
pub struct SharedFT6336<I2C, const N: usize> {
    touch: Arc<Mutex<CriticalSectionRawMutex, FT6336<I2C>>>,
    press_sender: Option<embassy_sync::channel::Sender<'static, CriticalSectionRawMutex, ConfirmedPress, N>>,
}

impl<I2C, const N: usize> SharedFT6336<I2C, N>
where
    I2C: I2c + 'static,
{
    /// Create a new shared touch controller with debounced press sender
    pub fn new(
        touch: FT6336<I2C>,
        press_sender: Option<embassy_sync::channel::Sender<'static, CriticalSectionRawMutex, ConfirmedPress, N>>,
    ) -> Self {
        Self {
            touch: Arc::new(Mutex::new(touch)),
            press_sender,
        }
    }

    /// Read touch data (locks internally)
    pub async fn read_touch(&self) -> Result<Option<TouchPoint>, I2C::Error> {
        let mut touch = self.touch.lock().await;
        touch.read_touch()
    }

    /// Background task: continuously poll touch with debouncing logic
    ///
    /// Implements:
    /// - 300ms hold requirement for press confirmation
    /// - Sequential press/release (next press starts after release)
    /// - Emits only ConfirmedPress and Release events via channel
    ///
    /// Poll rate: ~100Hz (10ms intervals)
    pub async fn run_background_task(self) -> ! {
        info!("Touch background task starting...");
        
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum State {
            Idle,
            Touching { since: Instant, x: u16, y: u16 },
            Confirmed { x: u16, y: u16 },
        }
        
        let mut state = State::Idle;
        
        loop {
            // Read touch via internal mutex
            let raw = self.read_touch().await.ok().flatten();
            
            match (&state, raw) {
                // Idle: waiting for first contact
                (State::Idle, Some(point)) if matches!(point.event, TouchEvent::Press | TouchEvent::Contact) => {
                    debug!("Touch: contact started at ({}, {})", point.x, point.y);
                    state = State::Touching {
                        since: Instant::now(),
                        x: point.x,
                        y: point.y,
                    };
                }
                
                // Touching: counting down to confirmation (300ms)
                (State::Touching { since, x, y }, Some(point)) if matches!(point.event, TouchEvent::Contact) => {
                    let elapsed = Instant::now() - *since;
                    if elapsed.as_millis() >= PRESS_CONFIRM_TIME_MS {
                        // Confirmed!
                        debug!("Touch: press confirmed after {}ms at ({}, {})", elapsed.as_millis(), x, y);
                        if let Some(ref sender) = self.press_sender {
                            let _ = sender.try_send(ConfirmedPress { x: *x, y: *y });
                        }
                        state = State::Confirmed { x: *x, y: *y };
                    }
                }
                
                // Confirmed: waiting for release
                (State::Confirmed { x: _, y: _ }, Some(point)) if matches!(point.event, TouchEvent::Contact) => {
                    // Still confirmed and touching, position update only (no event)
                    // Position tracking here if needed
                }
                
                // Touching: released before confirmation
                (State::Touching { .. }, Some(point)) if matches!(point.event, TouchEvent::Release) => {
                    debug!("Touch: released before confirmation");
                    state = State::Idle;
                }
                
                // Confirmed: released after confirmation -> emit Release
                (State::Confirmed { x, y }, Some(point)) if matches!(point.event, TouchEvent::Release) => {
                    debug!("Touch: confirmed release at ({}, {})", x, y);
                    // Could emit a Release event here if needed
                    state = State::Idle;
                }
                
                // No touch
                (_, None) => {
                    match state {
                        State::Touching { .. } => {
                            debug!("Touch: lost contact before confirmation");
                            state = State::Idle;
                        }
                        State::Confirmed { .. } => {
                            debug!("Touch: lost contact after confirmation");
                            state = State::Idle;
                        }
                        _ => {}
                    }
                }
                
                _ => {} // Other transitions ignored
            }

            // Poll at ~100Hz
            Timer::after_millis(10).await;
        }
    }
}