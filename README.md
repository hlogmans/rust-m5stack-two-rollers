M5Stack Dual Motor + Touch Demo with Embassy Async

A demonstration project for embedded hardware abstraction, HAL patterns, and async/await concurrency using Embassy on an ESP32-S3.

## Hardware Setup

### M5Stack CoreS3 (ESP32-S3)
- **Display**: ILI9342C 320×240 over SPI2
- **Power**: AXP2101 on I2C0
- **GPIO Expander**: AW9523 (LCD control) on I2C0
- **Touch**: FT6336 (capacitive) on I2C0
- **Motor A**: Roller485 @ 0x64 on I2C1 Port A (Grove)
- **Motor B**: Roller485 @ 0x65 on I2C1 Port A (shared bus with A)

### I2C Pinouts

```
I2C0 (GPIO12 SDA / GPIO11 SCL):
  ├─ AXP2101     @ 0x34  [Power Management]
  ├─ AW9523      @ 0x58  [GPIO Expander - display control]
  └─ FT6336      @ 0x38  [Capacitive Touch Controller]

I2C1 (GPIO2 SDA / GPIO1 SCL) - Port A:
  ├─ Motor A     @ 0x64  [Full control - encoder read, speed/position]
  └─ Motor B     @ 0x65  [Commands via Motor A's bus]
```

## Features

✅ **Dual Motor Control**: Both Roller485 motors on same I2C bus at different addresses  
✅ **Touch Input**: FT6336 capacitive touch with press/release/contact detection  
✅ **Display Output**: Real-time dual angle display (0-359°)  
✅ **Async Tasks**: Embassy-based concurrency with channels for angle updates  
✅ **HAL Abstraction**: Clean separation of hardware drivers and business logic  
✅ **Power Management**: AXP2101 initialization and display backlight control  

## Building

### Debug Build
```bash
cargo build
```

### Release Build + Flash + Monitor
```bash
cargo run --release
# Or manually:
espflash flash --monitor --chip esp32s3 --port /dev/ttyUSB0 \
  target/xtensa-esp32s3-none-elf/release/m5-minimal
```

## Architecture

### Hardware Layer (`src/hardware/`)

- **`mod.rs`**: Board initialization, peripheral setup, I2C bus configuration
- **`power.rs`**: AXP2101 (power management) and AW9523 (GPIO expander) driver
- **`roller485.rs`**: Roller485 stepper motor controller - supports multiple addresses on same bus
- **`touch.rs`**: FT6336 capacitive touch controller with event types
- **`display.rs`**: ILI9342C display over SPI with embedded-graphics integration

### Business Layer (`src/business/`)

- **`angle_controller.rs`**: Motor sync and position following logic
- **`evaluator.rs`**: Angle-based evaluation (color coding for UI feedback)

### Main Application (`src/bin/main.rs`)

Tasks:
1. **`run_motor_a_poll`**: Poll Motor A encoder, publish angle updates
2. **`run_touch_reader`**: Poll FT6336 for touch events
3. **`run_display`**: Render real-time angle display using embedded-graphics

Inter-task Communication:
- `ANGLE_A_CH`: Motor A angle updates (u16 degrees)
- `ANGLE_B_CH`: Motor B angle updates (u16 degrees)

## Key Design Decisions

### Shared I2C1 Bus for Both Motors

Instead of attempting sequential reuse of I2C1 on different GPIO pins (which Rust ownership prevents), both motors operate at different I2C addresses on the same bus:

- **Motor A** (0x64): Full driver with encoder reading, speed control, position control
- **Motor B** (0x65): Lightweight wrapper - commands sent via Motor A's I2C bus

This is practical because:
- Simultaneous motor communication isn't required
- Sequential access (poll A, command B) is sufficient
- No GPIO pin reuse or I2C peripheral contention
- Simple to extend (third motor at 0x66 on same bus)

See [MOTOR_B_SOLUTION.md](MOTOR_B_SOLUTION.md) for technical details.

### RS485 Option (Alternative)

If you prefer to avoid I2C pin/peripheral constraints entirely, you can move one or both Roller485 units to RS485. This is viable if your Roller485 units expose an RS485 two-wire A/B interface (many do), and you add an RS485 transceiver to the CoreS3.

- Hardware:
  - Add a Grove RS485 transceiver (e.g. M5Stack RS485 Unit) or a MAX485 breakout.
  - Connect to a free UART on the CoreS3 (e.g. `UART1`) and a GPIO for RE/DE (direction control) if required by the transceiver.
  - Daisy-chain both motors on the same A/B differential pair; each motor should have a unique device ID/address in the RS485 protocol.

- Pros:
  - Robust, long-distance, multi-drop bus; no I2C pin remapping needed.
  - Single UART can address multiple motors sequentially.

- Cons:
  - Requires an extra hardware transceiver and new wiring.
  - Needs a UART-based driver for the Roller485 RS485 protocol (framing, addressing, CRC if present).

- Software approach:
  - Add a UART+RS485 driver under `src/hardware/` that implements: open UART, manage RE/DE, frame encode/decode, per-motor addressing.
  - Embassy tasks can share the UART by sending sequential commands (no simultaneous bus access).
  - Keep I2C0 for PMU/AW9523/touch; keep I2C1 free or for other sensors.

If you want, I can scaffold a minimal `rs485` driver and a `roller485_rs485` facade that mirrors the existing I2C API (enable, set mode, set speed/position, read encoder if supported).

### Async Concurrency with Embassy

- Single executor on CPU 0
- All operations use `async/await` for cooperative multitasking
- Channels for safe inter-task communication (no shared mutable state)
- Timer-driven task scheduling (100-500ms polling intervals)

### HAL Abstraction

- Generic `Roller485<I2C>` driver over `embedded_hal::i2c::I2c` trait
- Display abstraction layer for SPI-based rendering
- Power management separated from board initialization
- Touch driver with event-based interface

## Testing

Unit tests for core functionality:

```bash
# Test angle conversion and position wrapping
cargo test --lib hardware::roller485::tests

# Run all tests
cargo test --lib
```

## Future Enhancements

- [ ] Motor B encoder reading (if needed)
- [ ] Position sync between motors
- [ ] Real-time status display (mode, RPM, diagnostics)
- [ ] CI/CD pipeline for formatting and linting
- [ ] Hardware wiring diagram
- [ ] Property-based tests for angle calculations

## Documentation

- [MOTOR_B_SOLUTION.md](MOTOR_B_SOLUTION.md) - Detailed explanation of dual motor architecture
- [DUAL_MOTOR_ARCHITECTURE.md](DUAL_MOTOR_ARCHITECTURE.md) - Alternative approaches explored
- [POWER_SOLUTION.md](POWER_SOLUTION.md) - Power management details
- [agents.md](agents.md) - Project goals and requirements

## License

Educational/demonstration project
