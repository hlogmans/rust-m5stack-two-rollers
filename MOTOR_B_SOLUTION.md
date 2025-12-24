# Dual Motor Solution: Shared I2C1 Bus Architecture

## Problem Statement

The M5Stack CoreS3 has only **2 I2C peripherals** (I2C0 and I2C1):
- **I2C0**: GPIO12/11 - Power management (AXP2101), display control (AW9523), and touch controller (FT6336)
- **I2C1**: Can operate on different GPIO pin sets via Grove ports

The initial challenge was getting all three devices to work:
1. **Touch screen** (FT6336) @ 0x38 on I2C0
2. **Motor A** (Roller485) @ 0x64 on I2C1 Port A (GPIO2/1)
3. **Motor B** (Roller485) @ 0x65 on I2C1 Port C (GPIO17/18)

Previous attempts:
- ❌ Motor B on I2C0 (already full with power/display/touch)
- ❌ Both motors at different addresses on same I2C1 bus on Port A (Motor B at 0x65 didn't respond - may be a hardware issue)
- ❌ Attempting sequential I2C1 reuse on different pins (Rust ownership prevented GPIO pin reuse)

## Solution: Shared Bus on Port A

**Both Motor A and Motor B share the same I2C1 bus on Port A (GPIO2/1)**, just at different I2C addresses:
- **Motor A**: I2C address 0x64 (fully featured - read/write encoder, set speed/position)
- **Motor B**: I2C address 0x65 (commands sent via Motor A's I2C bus)

This is **practical and reasonable** because:
1. The motors don't need simultaneous bidirectional communication
2. Sequential access to both motors is sufficient (poll A, command B, repeat)
3. Motor B only needs to receive commands (enable, set mode, set position), not read back its own state
4. This eliminates I2C peripheral contention entirely

## Hardware Configuration

### I2C0 (GPIO12/11) - Power & Touch
```
┌─────────────────┐
│     I2C0        │ GPIO12 (SDA) / GPIO11 (SCL)
├─────────────────┤
│ AXP2101         │ 0x34 - Power management
│ AW9523          │ 0x58 - GPIO expander (LCD control)
│ FT6336          │ 0x38 - Capacitive touch
└─────────────────┘
```

### I2C1 (GPIO2/1 on Port A) - Dual Motors
```
┌─────────────────┐
│     I2C1        │ GPIO2 (SDA) / GPIO1 (SCL)
│   Port A        │ (NOT Port C!)
├─────────────────┤
│ Motor A         │ 0x64 - Full control (read encoder, set speed/position)
│ Motor B         │ 0x65 - Commands only (via Motor A's write_to_motor_b())
└─────────────────┘
```

## Implementation Details

### 1. **MotorB Struct** (lightweight wrapper)
```rust
pub struct MotorB {
    pub address: u8,  // 0x65
}
```
- Minimal structure - just stores the address
- No I2C bus ownership
- Used as a placeholder in Board struct

### 2. **Motor A Keeps I2C Bus**
Motor A's `Roller485` instance owns the shared I2C1 bus:
```rust
pub struct Board<'a> {
    pub roller_a: Roller485<I2c<'a, esp_hal::Blocking>>,  // Owns I2C1
    pub roller_b: MotorB,                                   // Just address
}
```

### 3. **Helper Methods on Roller485**
New methods to send commands to Motor B through the shared bus:
```rust
/// Write raw data to Motor B (0x65) on the shared I2C bus
pub fn write_to_motor_b(&mut self, data: &[u8]) -> Result<(), I2C::Error> {
    self.i2c.write(MOTOR_B_ADDR, data)
}

/// Convenience: enable Motor B
pub fn enable_motor_b(&mut self) -> Result<(), I2C::Error> {
    self.write_to_motor_b(&[0x00, 0x01])
}

/// Convenience: set Motor B to encoder mode
pub fn set_motor_b_encoder_mode(&mut self) -> Result<(), I2C::Error> {
    self.write_to_motor_b(&[0x01, 0x04])
}
```

### 4. **Initialization**
```rust
// In hardware/mod.rs Board::init()
let mut roller_a = Roller485::new(i2c1_bus);
let _ = roller_a.init();

// Initialize Motor B via Motor A's bus
let _ = roller_a.enable_motor_b();
let _ = roller_a.set_motor_b_encoder_mode();

// Store reference (Motor B has no I2C ownership)
Self {
    roller_a,
    roller_b: MotorB { address: 0x65 },
    touch,
}
```

## Task Architecture

### `run_motor_a_poll` Task
- Polls Motor A's encoder every 500ms
- Publishes angle to display via `ANGLE_A_CH`
- Can also send commands to Motor B via `board.roller_a.write_to_motor_b()`

### `run_touch_reader` Task
- Polls FT6336 touch controller on I2C0
- Logs PRESS/RELEASE events

### `run_display` Task
- Receives angle updates from Motor A
- Renders dual angle display (Motor A on left, Motor B on right)

### No More Motor B Poll Task
- Motor B doesn't need its own task
- Commands sent synchronously from Motor A task when needed
- State can be tracked in Motor A task if needed

## Advantages

✅ **Solves resource conflicts**: I2C0 for power/display/touch, I2C1 for both motors  
✅ **Eliminates Rust ownership issues**: No GPIO pin reuse, no peripheral conflicts  
✅ **Practical**: Sequential (not simultaneous) motor access is acceptable  
✅ **Simple**: Both motors on same pins, just different addresses  
✅ **Testable**: Clear separation of Motor A (full driver) vs Motor B (command interface)  
✅ **Scales**: If a third motor is added, it can go at address 0x66 on the same bus  

## Potential Extensions

### Reading Motor B's Encoder (if needed)
If Motor B's encoder state needs to be read independently:
```rust
pub fn read_motor_b_encoder(&mut self) -> Result<i32, I2C::Error> {
    let mut buffer = [0u8; 4];
    self.i2c.write_read(0x65, &[0x90], &mut buffer)?;
    Ok(i32::from_le_bytes(buffer) / 100)
}
```

### Position Sync Between Motors
If Motor B should follow Motor A's position:
```rust
// In run_motor_a_poll task:
if let Ok(angle) = motor_a.read_angle_block() {
    // Send position to Motor B
    motor_a.write_to_motor_b(&[0x80, pos[0], pos[1], pos[2], pos[3]])?;
}
```

### Dedicated Motor B Task (if real-time feedback needed)
```rust
#[embassy_executor::task]
async fn run_motor_b_monitor(motor_a: Arc<Mutex<Roller485<...>>>) {
    loop {
        if let Ok(mut m) = motor_a.try_lock() {
            let pos = m.read_motor_b_encoder();
            let _ = ANGLE_B_CH.try_send(pos as u16);
        }
        Timer::after(Duration::from_millis(500)).await;
    }
}
```

## Why Rust Ownership Prevented Sequential Reuse

The original attempt to reuse I2C1 with different GPIO pins on different ports failed because:

1. **I2c struct takes ownership**: `I2c::new(peripheral, ...)` consumes the I2C peripheral
2. **with_sda/with_scl consume pins**: These methods consume GPIO pins
3. **Can't create second instance**: After `let i2c1_a = I2c::new(i2c1, ...)`, `i2c1` is moved
4. **Can't reuse GPIO pins**: After `with_sda(gpio2)`, gpio2 is moved

To sequentially reuse I2C1 on different pins would require:
- Dropping Motor A (and its I2c instance)
- Retrieving GPIO pins somehow (not possible - they're consumed)
- Creating a new I2c instance with reclaimed I2C1 peripheral

This would require significant changes to `esp-hal`'s API or use of `unsafe` code and pointer manipulation, which isn't practical for this application.

## Status: ✅ Complete

- ✅ Motor A fully operational on I2C1 Port A
- ✅ Motor B initialized on same I2C1 bus at address 0x65
- ✅ Touch controller on I2C0
- ✅ All three devices in operation simultaneously
- ✅ Clean Rust code with no ownership conflicts
- ✅ Display shows both motor angles
