
const AW9523_ADDRESS: u8 = 0x58;

// AW9523 Register Addresses
const REG_OUTPUT_PORT0: u8 = 0x02;
const REG_OUTPUT_PORT1: u8 = 0x03;
const REG_CONFIG_PORT0: u8 = 0x04;
const REG_CONFIG_PORT1: u8 = 0x05;
#[allow(dead_code)]
const REG_INT_PORT0: u8 = 0x06;
#[allow(dead_code)]
const REG_INT_PORT1: u8 = 0x07;
#[allow(dead_code)]
const REG_ID: u8 = 0x10;
const REG_CTL: u8 = 0x11;
const REG_LED_MODE: u8 = 0x12;
#[allow(dead_code)]
const REG_LED_DIM: u8 = 0x13;

// Pin assignments for M5CoreS3
const PIN_LCD_RST: u8 = 0;      // P0_0: LCD Reset
const PIN_LCD_BL: u8 = 1;       // P0_1: LCD Backlight  
#[allow(dead_code)]
const PIN_BUS_OUT_EN: u8 = 1;   // P0_1: Bus output enable (also controls backlight)
const PIN_GROVE_POWER: u8 = 7;  // P1_7: Grove port power rail (5V)
#[allow(dead_code)]
const PIN_BOOST_EN: u8 = 7;     // P1_7: Also boost enable

pub enum DataFormat<'a> {
    /// Slice of unsigned bytes
    U8(&'a [u8]),
}

#[derive(Debug)]
pub enum Aw9523Error {
    NotSupported,
    InvalidArgument,
    ReadError,
    WriteError,
}

pub trait Aw9523ReadWrite {
    fn send_commands(&mut self, cmd: DataFormat<'_>) -> Result<(), Aw9523Error>;
    fn read_register(&mut self, reg: u8) -> Result<u8, Aw9523Error>;
    fn write_register(&mut self, reg: u8, value: u8) -> Result<(), Aw9523Error>;
}

pub struct Aw9523<I> {
    interface: I,
}

// https://github.com/m5stack/M5CoreS3/blob/main/src/AXP2101.cpp
impl<I> Aw9523<I>
where
    I: Aw9523ReadWrite,
{
    // Create a new AW9523 interface
    pub fn new(interface: I) -> Self {
        Self { interface }
    }

    /// Modify a single bit in a register (read-modify-write)
    fn modify_bit(&mut self, reg: u8, bit: u8, set: bool) -> Result<(), Aw9523Error> {
        let mut value = self.interface.read_register(reg)?;
        if set {
            value |= 1 << bit;
        } else {
            value &= !(1 << bit);
        }
        self.interface.write_register(reg, value)
    }

    /// Initialize AW9523 with default configuration for M5CoreS3:
    /// - LCD backlight and reset control enabled
    /// - Grove port power enabled (5V rail)
    /// - GPIO mode (non-LED mode)
    pub fn init(&mut self) -> Result<(), Aw9523Error> {
        // Control register: push-pull mode
        self.interface
            .write_register(REG_CTL, 0b00010000)?;

        // LED mode: all pins in GPIO mode (0xFF = all GPIO, not LED)
        self.interface
            .write_register(REG_LED_MODE, 0xFF)?;

        // Port configs: keep only used pins as outputs (0 = output, 1 = input)
        self.interface
            .write_register(REG_CONFIG_PORT0, 0b00011000)?;
        self.interface
            .write_register(REG_CONFIG_PORT1, 0b00001100)?;

        // Seed output states before toggling individual rails via helpers
        self.interface
            .write_register(REG_OUTPUT_PORT0, 0b00000111)?;
        self.interface
            .write_register(REG_OUTPUT_PORT1, 0b10000011)?;

        // Apply named helpers for clarity and future reuse
        self.set_lcd_reset(false)?;
        self.set_lcd_backlight(true)?;
        self.set_grove_power(true)?;

        Ok(())
    }

    /// Set Grove port power rail (5V) and return self for method chaining
    /// 
    /// # Arguments
    /// * `enable` - true to enable 5V power to Grove ports, false to disable
    pub fn set_grove_power(&mut self, enable: bool) -> Result<&mut Self, Aw9523Error> {
        self.modify_bit(REG_OUTPUT_PORT1, PIN_GROVE_POWER, enable)?;
        Ok(self)
    }

    /// Set LCD backlight state and return self for method chaining
    /// 
    /// # Arguments
    /// * `enable` - true to turn backlight on, false to turn off
    pub fn set_lcd_backlight(&mut self, enable: bool) -> Result<&mut Self, Aw9523Error> {
        self.modify_bit(REG_OUTPUT_PORT0, PIN_LCD_BL, enable)?;
        Ok(self)
    }

    /// Set LCD reset state and return self for method chaining
    /// 
    /// # Arguments
    /// * `reset` - true to hold reset low, false to release reset (high)
    pub fn set_lcd_reset(&mut self, reset: bool) -> Result<&mut Self, Aw9523Error> {
        self.modify_bit(REG_OUTPUT_PORT0, PIN_LCD_RST, !reset)?;
        Ok(self)
    }

    /// Release the I2C interface from this AW9523 instance
    pub fn into_i2c(self) -> I {
        self.interface
    }
}

pub struct I2CInterface<I2C> {
    i2c: I2C,
    addr: u8,
}

impl<I2C> I2CInterface<I2C>
where
    I2C: embedded_hal::i2c::I2c,
{
    /// Create new I2C interface for communication with a display driver
    pub fn new(i2c: I2C, addr: u8) -> Self {
        Self { i2c, addr }
    }

    /// Release the I2C bus from this interface
    pub fn release_i2c(self) -> I2C {
        self.i2c
    }
}

// Implement Aw9523ReadWrite for I2CInterface
impl<I> Aw9523ReadWrite for I2CInterface<I>
where
    I: embedded_hal::i2c::I2c,
{
    /// Send commands over I2C to AW9523
    fn send_commands(&mut self, cmd: DataFormat<'_>) -> Result<(), Aw9523Error> {
        let mut data_buf = [0];

        match cmd {
            DataFormat::U8(data) => {
                self.i2c
                    .write_read(self.addr, &[data[0]], &mut data_buf)
                    .map_err(|_| Aw9523Error::WriteError)?;
                self.i2c
                    .write(self.addr, data)
                    .map_err(|_| Aw9523Error::WriteError)
            }
        }
    }

    /// Read a single register from AW9523
    fn read_register(&mut self, reg: u8) -> Result<u8, Aw9523Error> {
        let mut buffer = [0u8; 1];
        self.i2c
            .write_read(self.addr, &[reg], &mut buffer)
            .map_err(|_| Aw9523Error::ReadError)?;
        Ok(buffer[0])
    }

    /// Write a single register to AW9523
    fn write_register(&mut self, reg: u8, value: u8) -> Result<(), Aw9523Error> {
        self.i2c
            .write(self.addr, &[reg, value])
            .map_err(|_| Aw9523Error::WriteError)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct I2CGpioExpanderInterface(());

impl I2CGpioExpanderInterface {
    pub fn new<I>(i2c: I) -> I2CInterface<I>
    where
        I: embedded_hal::i2c::I2c,
    {
        Self::new_custom_address(i2c, AW9523_ADDRESS)
    }

    /// Create a new I2C interface with a custom address.
    pub fn new_custom_address<I>(i2c: I, address: u8) -> I2CInterface<I>
    where
        I: embedded_hal::i2c::I2c,
    {
        I2CInterface::new(i2c, address)
    }
}
