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
use embassy_time::{Duration, Timer};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::channel::Channel;
use alloc::sync::Arc;
use crate::{info, warn};
use crate::helpers::TelemetrySender;

/// I2C address for Roller485 (default)
pub const ROLLER485_DEFAULT_ADDR: u8 = 0x64;

/// I2C address for Motor B (when sharing bus with Motor A)
pub const MOTOR_B_ADDR: u8 = 0x65;

/// Register addresses for Roller485 (I2C Protocol)
/// See: M5Stack Unit Roller485 I2C Protocol Documentation
mod registers {
    /// Motor Enable/Disable register (1 byte: 0x00=off, 0x01=on)
    pub const MOTOR_ENABLE_REG: u8 = 0x00;
    /// Motor mode setting (1 byte: 0x01=speed, 0x02=position, 0x03=current, 0x04=encoder)
    pub const MODE_REG: u8 = 0x01;
    /// Speed Setting register (4 bytes, little-endian i32, set target speed in RPM)
    pub const SPEED_CONTROL_REG: u8 = 0x40;
    /// Speed Readback register (4 bytes, little-endian i32, read current speed)
    pub const SPEED_READBACK_REG: u8 = 0x60;
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
/// Generic over any I2C bus implementation. Uses critical-section for
/// safe multi-task access when wrapped in Mutex/Arc.
pub struct Roller485<I2C> {
    pub i2c: I2C,
    address: u8,
}

impl<I2C> Roller485<I2C>
where
    I2C: I2c,
{
    /// Create a new Roller485 instance with default address
    ///
    /// # Arguments
    /// * `i2c` - The I2C bus to use for communication (can be Arc<Mutex<>> for sharing)
    pub fn new(i2c: I2C) -> Self {
        Self::new_with_address(i2c, ROLLER485_DEFAULT_ADDR)
    }

    /// Create a new Roller485 instance with custom address
    ///
    /// # Arguments
    /// * `i2c` - The I2C bus to use for communication (can be Arc<Mutex<>> for sharing)
    /// * `address` - Custom I2C address
    pub fn new_with_address(i2c: I2C, address: u8) -> Self {
        Self { i2c, address }
    }

    /// Consume this Roller485 instance and return the I2C bus
    /// Useful for reconfiguring the I2C bus with different pins
    pub fn into_i2c(self) -> I2C {
        self.i2c
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
        let _ = self.read_mode()?;
        info!("Roller485 mode set");
        
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

    /// Send a raw command frame (async-friendly wrapper).
    pub async fn send_command(&mut self, cmd: &[u8]) -> Result<(), I2C::Error> {
        // allow await points in async tasks, even though write is blocking
        Timer::after(Duration::from_millis(0)).await;
        self.i2c.write(self.address, cmd)
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

    /// Write position command to Motor B (0x65) sharing the same I2C bus.
    /// This is a write-only operation that doesn't require Motor B to have its own Roller485 instance.
    /// 
    /// # Arguments
    /// * `position_steps` - Target position in steps
    pub fn write_position_to_motor_b(&mut self, position_steps: i32) -> Result<(), I2C::Error> {
        let target_position = position_steps * 100;
        let bytes = target_position.to_le_bytes();
        self.i2c.write(MOTOR_B_ADDR, &[
            registers::POSITION_CONTROL_REG,
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[3],
        ])
    }

    /// Set the target position in steps (synchronous helper).
    /// Position value is multiplied by 100 per device protocol.
    /// Automatically switches to Position Control mode if needed.
    pub fn set_position(&mut self, position_steps: i32) -> Result<(), I2C::Error> {
        // Switch to Position Control mode if not already there
        let current_mode = self.read_mode()?;
        if current_mode != Roller485Mode::Position {
            self.set_mode(Roller485Mode::Position)?;
        }

        let target_position = position_steps * 100;
        let bytes = target_position.to_le_bytes();
        self.i2c.write(self.address, &[
            registers::POSITION_CONTROL_REG,
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[3],
        ])
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

    /// Write a command to Motor B (address 0x65) on the same I2C bus.
    /// This allows Motor A to command Motor B when they share the same I2C1 bus.
    ///
    /// # Arguments
    /// * `data` - Raw command bytes to send to Motor B
    pub fn write_to_motor_b(&mut self, data: &[u8]) -> Result<(), I2C::Error> {
        self.i2c.write(MOTOR_B_ADDR, data)
    }

    /// Read current motor speed (RPM).
    ///
    /// Reads the 4-byte speed readback register at 0x60, converts the
    /// little-endian signed value to RPM with /100 scaling (per spec).
    pub fn read_speed_rpm(&mut self) -> Result<f32, I2C::Error> {
        let raw = self.read_speed_raw()?;
        Ok(raw as f32 / 100.0)
    }

    /// Read the raw speed register (scaled by 100).
    /// Returns the signed i32 value directly from the device.
    pub fn read_speed_raw(&mut self) -> Result<i32, I2C::Error> {
        let mut buffer = [0u8; 4];
        let _ = self.read_block(registers::SPEED_READBACK_REG, &mut buffer)?;
        Ok(i32::from_le_bytes(buffer))
    }

    /// Convenience: enable Motor B on the shared I2C bus
    pub fn enable_motor_b(&mut self) -> Result<(), I2C::Error> {
        self.write_to_motor_b(&[registers::MOTOR_ENABLE_REG, 0x01])
    }

    /// Convenience: set Motor B to encoder mode (same as Motor A)
    pub fn set_motor_b_encoder_mode(&mut self) -> Result<(), I2C::Error> {
        self.write_to_motor_b(&[registers::MODE_REG, Roller485Mode::Encoder as u8])
    }

    /// Ensure the device is in encoder reading mode
    ///
    /// Call this if you suspect the mode may have been changed,
    /// for example after navigating menu pages on the device.
    pub fn ensure_encoder_mode(&mut self) -> Result<(), I2C::Error> {
        let mode = self.read_mode()?;
        if mode != Roller485Mode::Encoder {
            info!("Roller485 mode reset to Encoder");
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
    fn test_speed_readback_conversion() {
        // 123.45 RPM encoded as (12345) in little-endian i32
        let bytes = 12345i32.to_le_bytes();
        let value = i32::from_le_bytes(bytes);
        assert_eq!(value, 12345);
        let rpm = value as f32 / 100.0;
        assert!((rpm - 123.45).abs() < 0.01);
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

/// Commands that can be sent to a motor controller
#[derive(Debug, Clone, Copy)]
pub enum MotorCommand {
    /// Move to absolute position (in steps)
    SetPosition(i32),
    /// Set speed (in RPM)
    SetSpeed(i32),
    /// Active Reading
    SetReading
}

/// Shared wrapper around Roller485 with internal async locking and background polling.
/// Use this type when you need to share a motor across multiple tasks.
/// The motor publishes telemetry via channel-agnostic senders.
pub struct SharedRoller485<I2C> {
    inner: Arc<Mutex<CriticalSectionRawMutex, Roller485<I2C>>>,
    angle_sender: Option<TelemetrySender<u16, 8>>,
    speed_sender: Option<TelemetrySender<f32, 4>>,
    command_channel: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
}

impl<I2C> Clone for SharedRoller485<I2C> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            angle_sender: self.angle_sender.clone(),
            speed_sender: self.speed_sender.clone(),
            command_channel: self.command_channel,
        }
    }
}

impl<I2C> SharedRoller485<I2C>
where
    I2C: I2c,
{
    /// Wrap a Roller485 instance with channel-agnostic telemetry senders.
    /// 
    /// # Arguments
    /// * `motor` - The Roller485 hardware driver
    /// * `angle_sender` - Optional sender for angle updates (Watch/Channel/Both)
    /// * `speed_sender` - Optional sender for speed updates (Watch/Channel/Both)
    /// * `command_channel` - Channel for receiving motor commands
    /// 
    /// The motor doesn't know or care whether telemetry goes to a Watch or Channel.
    pub fn new(
        motor: Roller485<I2C>,
        angle_sender: Option<TelemetrySender<u16, 8>>,
        speed_sender: Option<TelemetrySender<f32, 4>>,
        command_channel: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(motor)),
            angle_sender,
            speed_sender,
            command_channel,
        }
    }

    /// Send a command to the motor
    pub async fn send_command(&self, cmd: MotorCommand) {
        self.command_channel.send(cmd).await;
    }

    /// Start the motor background task that polls encoder/speed and processes commands.
    /// This should be spawned once per motor instance.
    /// 
    /// # Arguments
    /// * `speed_filter` - Optional filter for smoothing speed readings
    pub async fn run_background_task(
        self,
        mut speed_filter: Option<crate::filters::MotorValueFilter>,
    ) where
        I2C: 'static,
    {
        info!("Motor background task starting");

        // Ensure encoder mode on startup
        let _ = self.ensure_encoder_mode().await;

        let mut original_angle = 0;
        let mut first_position_sent = false;

        loop {
            // Check for commands (non-blocking)
            if let Ok(cmd) = self.command_channel.try_receive() {
                match cmd {
                    MotorCommand::SetPosition(pos) => {
                        info!("Motor command: SetPosition({})", pos);
                        let _ = self.set_position(pos).await;
                    }
                    MotorCommand::SetSpeed(rpm) => {
                        info!("Motor command: SetSpeed({})", rpm);
                        let _ = self.set_speed(rpm).await;
                    }
                    MotorCommand::SetReading => {
                        let _ = self.ensure_encoder_mode().await;
                    }
                }
            }

            // Poll encoder position (if angle sender configured)
            if let Some(ref sender) = self.angle_sender {
                match self.read_encoder_position().await {
                    Ok(steps) => {
                        let angle = ((steps % 333 + 333) % 333) * 360 / 333;
                        if (!first_position_sent) || (angle != original_angle) {
                            info!("Motor position: steps={}, angle={}°", steps, angle);
                            sender.send(angle as u16);
                            first_position_sent = true;
                            original_angle = angle;
                        }
                    }
                    Err(_) if !first_position_sent => warn!("Motor read error"),
                    Err(_) => {}
                }
            }

            // Poll speed (if speed sender configured)
            if let Some(ref sender) = self.speed_sender {
                match self.read_speed_rpm().await {
                    Ok(rpm) => {
                        let filtered_rpm = if let Some(ref mut filter) = speed_filter {
                            filter.update(rpm)
                        } else {
                            Some(rpm)
                        };
                        
                        if let Some(rpm) = filtered_rpm {
                            info!("Motor speed: {} RPM", rpm);
                            sender.send(rpm);
                        }
                    }
                    Err(_) => {
                        warn!("Motor speed read error");
                    }
                }
            }

            Timer::after(Duration::from_millis(25)).await;
        }
    }

    /// Ensure the motor is in encoder mode (async with internal locking)
    async fn ensure_encoder_mode(&self) -> Result<(), I2C::Error> {
        let mut guard = self.inner.lock().await;
        guard.ensure_encoder_mode()
    }

    /// Read encoder position (async with internal locking)
    async fn read_encoder_position(&self) -> Result<i32, I2C::Error> {
        let mut guard = self.inner.lock().await;
        guard.read_encoder_position()
    }

    /// Read motor speed in RPM (async with internal locking)
    async fn read_speed_rpm(&self) -> Result<f32, I2C::Error> {
        let mut guard = self.inner.lock().await;
        guard.read_speed_rpm()
    }

    /// Set motor position (async with internal locking)
    async fn set_position(&self, position_steps: i32) -> Result<(), I2C::Error> {
        let mut guard = self.inner.lock().await;
        guard.set_position(position_steps)
    }

    /// Set motor speed (async with internal locking)
    async fn set_speed(&self, speed_rpm: i32) -> Result<(), I2C::Error> {
        let mut guard = self.inner.lock().await;
        guard.set_speed(speed_rpm)
    }
}
