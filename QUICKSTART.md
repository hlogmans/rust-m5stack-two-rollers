# Quick Start Guide

## Building & Running

### Prerequisites
- Rust with esp32s3 target: `rustup target add xtensa-esp32s3-none-elf`
- espflash: `cargo install espflash`
- ESP-IDF tools installed
- M5Stack CoreS3 connected via USB

### Build Debug
```bash
cd m5-minimal
cargo build
```

### Build + Flash + Monitor (Release)
```bash
cargo run --release
# OR manually select port:
espflash flash --monitor --chip esp32s3 --port /dev/cu.usbmodem112201 \
  target/xtensa-esp32s3-none-elf/release/m5-minimal
```

## What You'll See

### Serial Output
```
INFO - Embassy initialized!
INFO - Initializing M5Stack CoreS3 hardware...
INFO - Initializing Roller485 Motor A at address 0x64 on I2C1 Port A...
INFO - Initializing Motor B at address 0x65 on shared I2C1 bus...
INFO - Initializing FT6336 touch controller on I2C0...
INFO - Motor A POLL: start (I2C1, addr=0x64)
INFO - Motor A steps=... (angle°)
```

### Device Display
- **Left Circle**: Motor A angle (0-359°)
- **Right Circle**: Motor B angle
- **Responsive** to motor rotation

### Touch Interaction
- **Press**: `TOUCH PRESS: x=131, y=156`
- **Release**: `TOUCH RELEASE`

## Key Components

### Motor A: Full Control
```rust
let mut motor_a = Roller485::new(i2c1_bus);
motor_a.init();  // Enable, set to encoder mode
motor_a.read_angle_block()?;  // Get angle
```

### Motor B: Via Motor A
```rust
motor_a.enable_motor_b()?;
motor_a.set_motor_b_encoder_mode()?;
motor_a.write_to_motor_b(&[0x40, ...])?;  // Raw commands
```

### Touch Controller
```rust
let mut touch = FT6336::new(i2c0_bus);
if let Ok(Some(point)) = touch.read_touch() {
    match point.event {
        TouchEvent::Press => println!("x={}, y={}", point.x, point.y),
        TouchEvent::Release => println!("Released"),
        _ => {}
    }
}
```

## Architecture

```
┌─────────────────────────────────┐
│   M5Stack CoreS3 (ESP32-S3)     │
├─────────────────────────────────┤
│  I2C0 (GPIO12/11)   I2C1 (GPIO2/1)
│                                  │
├─ AXP2101 (0x34)   ├─ Motor A (0x64)
├─ AW9523 (0x58)    ├─ Motor B (0x65)
├─ FT6336 (0x38)    │
│                    │
└─────────────────────────────────┘
         SPI2: ILI9342C Display
```

## Debugging

### Check Device Responses
```bash
# In monitor output, look for:
INFO - I2C0: found device at 0x38  # Touch
INFO - Motor A steps=... (°)        # Motor A working
INFO - TOUCH PRESS: ...             # Touch working
```

### If Motor B Doesn't Respond
- Verify physical connection to Port A (not Port C)
- Check that Motor A initializes first
- Monitor log should show "Motor B initialized at 0x65"

### If Touch Not Responding
- Ensure FT6336 is on I2C0 (GPIO12/11), not I2C1
- Check display backlight works (AW9523 must be initialized)
- Look for `found device at 0x38` in I2C0 scan

## File Structure

```
src/
├─ bin/main.rs               # Tasks and app logic
├─ lib.rs                    # Module exports
├─ hardware/
│  ├─ mod.rs                 # Board init
│  ├─ power.rs               # AXP2101, AW9523
│  ├─ roller485.rs           # Motor driver
│  ├─ touch.rs               # FT6336 driver
│  └─ display.rs             # ILI9342C driver
├─ business/
│  ├─ angle_controller.rs    # Motor sync logic
│  └─ evaluator.rs           # Angle-based feedback
└─ display.rs                # UI rendering

Documentation:
├─ README.md                 # Project overview
├─ MOTOR_B_SOLUTION.md       # Dual motor architecture
├─ SOLUTION_SUMMARY.md       # This session's solution
├─ DUAL_MOTOR_ARCHITECTURE.md # Alternative approaches
└─ POWER_SOLUTION.md         # Power management details
```

## Common Tasks

### Reading Motor A Angle
```rust
// In a task:
match motor_a.read_angle_block() {
    Ok(block) if !block.zero_block => {
        let angle = block.angle_deg;  // 0-359
        let _ = ANGLE_A_CH.try_send(angle);
    }
    _ => {}
}
```

### Sending Command to Motor B
```rust
// Via Motor A's I2C bus:
motor_a.write_to_motor_b(&[
    0x80,           // Position control register
    pos[0], pos[1], // Position (little-endian i32)
    pos[2], pos[3],
])?;
```

### Updating Display
```rust
display.update_dual_angles(
    motor_a_angle,  // u16: 0-359
    motor_b_angle,  // u16: 0-359
);
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| Compilation error: "cannot find..." | Run `cargo build` to check all dependencies |
| Device not found on flash | Check USB cable, try `/dev/cu.usbmodem*` port |
| No serial output | Monitor might be blocked - press CTRL+C and retry |
| Motor A not responding | Check I2C1 on GPIO2/1, verify 0x64 in device list |
| Motor B not working | Ensure Motor A initializes first, check address 0x65 |
| Touch coordinates weird | Display is 320×240, x/y should be within bounds |
| Display blank | Check backlight - AW9523 must be initialized first |

## References

- [M5Stack CoreS3 Docs](https://docs.m5stack.com/en/core/coress3)
- [Roller485 Manual](https://docs.m5stack.com/en/unit/roller485)
- [FT6336 Datasheet](https://www.focaltech-systems.com/)
- [ESP-HAL Documentation](https://docs.esp-rs.org/esp-hal/)
- [Embassy Async Runtime](https://embassy.dev/)
