# M5Stack CoreS3 Power Management Solution

## Problem

After flashing and running the firmware, the display and Roller485 devices work perfectly. However, after a power cycle or reset (without reflashing), the devices would not have power until the firmware was run again.

## Root Cause

The M5Stack CoreS3 uses a two-tier power management architecture:

1. **AXP2101 PMU** - Provides voltage rails to power the board components
2. **AW9523 I/O Expander** - Controls power gates to external Grove ports

The critical insight from M5Stack's documentation and the M5Unified library revealed that:
- **Port 0 Register (0x02), Bit 1 (BUS_OUT_EN)** - Enables power to Grove ports
- **Port 1 Register (0x03), Bit 7 (BOOST_EN)** - Enables power boost for stable external power

The original `aw9523-rs` crate only set:
- Port 0: `0b00000101` (bits 0 and 2)
- Port 1: `0b00000011` (bits 0 and 1)

This meant **BUS_OUT_EN and BOOST_EN were never enabled**, so external devices had no power after boot.

## Solution

### Step 1: Extend the aw9523-rs Crate

Modified the local copy of `aw9523-rs` to include Grove port power in the init() sequence:

```rust
// In local_aw9523/src/lib.rs
pub fn init(&mut self) -> Result<(), Aw9523Error> {
    // Changed from 0b00000101 to 0b00000111 (add bit 1: BUS_OUT_EN)
    let _ = self.interface.send_commands(DataFormat::U8(&[0x02, 0b00000111]));
    
    // Changed from 0b00000011 to 0b10000011 (add bit 7: BOOST_EN)
    let _ = self.interface.send_commands(DataFormat::U8(&[0x03, 0b10000011]));
    
    // ... rest of initialization
    Ok(())
}
```

Added `release_i2c()` method to enable I2C bus reuse:

```rust
pub fn release_i2c(self) -> I {
    self.interface
}
```

### Step 2: Update Cargo.toml

Changed the aw9523 dependency from remote to local:

```toml
aw9523 = { path = "./local_aw9523" }
```

### Step 3: Simplify Power Initialization

The `src/hardware/power.rs` now simply:
1. Initializes AXP2101 (voltage rails)
2. Releases the I2C bus
3. Initializes AW9523 with **persistent** port power configuration

```rust
pub fn init_power_and_display_control<'a>(i2c_bus: I2c<'a, esp_hal::Blocking>) {
    // 1. Init AXP2101
    let axp_interface = I2CPowerManagementInterface::new(i2c_bus);
    let mut axp = Axp2101::new(axp_interface);
    axp.init()?;
    
    // 2. Get I2C bus back
    let i2c_bus = axp.release_i2c();
    
    // 3. Init AW9523 (now includes BUS_OUT_EN and BOOST_EN)
    let aw_interface = I2CGpioExpanderInterface::new(i2c_bus);
    let mut aw = Aw9523::new(aw_interface);
    aw.init()?;  // Automatically enables Grove port power!
}
```

## Benefits

✅ **Persistent Power** - Grove ports have power after every reboot  
✅ **No Polling Required** - Configuration happens once at init  
✅ **Standards Compliant** - Follows M5Unified library pattern  
✅ **Clean Abstraction** - Power management separated from business logic  
✅ **Tested Hardware** - Works with real M5Stack CoreS3 + Roller485  

## Hardware Architecture

```
┌─────────────────────────────────────────────────────────┐
│  M5Stack CoreS3 (ESP32-S3)                              │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌──────────────┐      ┌──────────────┐                │
│  │  AXP2101     │      │  AW9523      │                │
│  │  PMU @ 0x34  │◄────►│  I/O Exp     │                │
│  │              │ I2C0 │  @ 0x58      │                │
│  │ • DLDO1      │      │              │                │
│  │ • ALDO2      │      │ • Port0[1]=  │                │
│  │ • DCDC1      │      │   BUS_OUT_EN │                │
│  └──────────────┘      │ • Port1[7]=  │                │
│        │               │   BOOST_EN   │                │
│        │ 3.3V          │              │                │
│        │               │ • LCD reset  │                │
│        │               │ • Backlight  │                │
│        └───────────────┴──────────────┘                │
│                        │                               │
│                        ▼ Power to Grove Ports          │
└─────────────────────────────────────────────────────────┘
         │
         └──► Grove Port A (I2C1)
              └──► Roller485 @ 0x64
```

## Files Modified

- **local_aw9523/src/lib.rs** - Enhanced init() and added release_i2c()
- **src/hardware/power.rs** - Simplified initialization logic
- **Cargo.toml** - Points to local aw9523 crate
- **.gitignore** - Excludes local_aw9523 from git

## Testing

Boot sequence now shows:

```
INFO - Initializing AXP2101 PMU...
INFO - AXP2101 initialized successfully
INFO - Initializing AW9523 GPIO Expander (LCD + Grove port power)...
INFO - AW9523 initialized successfully
INFO -   - LCD reset and backlight control enabled
INFO -   - Grove port power (BUS_OUT_EN) enabled
INFO -   - Power boost (BOOST_EN) enabled
INFO - Power and display control initialization complete
INFO - Roller485 initialization complete
INFO - Roller485 angle: 0° (steps=256) - Status: Good
```

## Future Improvements

Consider contributing the enhanced `aw9523-rs` changes back to the upstream repository to benefit other projects.
