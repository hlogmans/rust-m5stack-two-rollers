This application is building on M5Stack devices.
I have a M5CoreS3 as controller (esp32s3), and two Roller485 devices.

The purpose of this application is to demonstrate embedded hardware, HAL abstractions, async/await with Embassy.

Goals is clear separation of business logic and underlying hardware, so abstractions are key.

Use a formal way of putting structures in separate files with corresponding code.
Try to include documentation and tests.


## Hardware Topology
- Controller: M5Stack CoreS3 (ESP32-S3)
- Display: ILI9342C over SPI2 (GPIO37 MOSI, GPIO36 SCK, GPIO3 CS, GPIO35 DC, GPIO15 RST)
- PMU: AXP2101 on I2C0 (0x34)
- GPIO Expander: AW9523 on I2C0 (0x58) controlling LCD reset/backlight and power rails
- Grove Port A: I2C1 (GPIO2 SDA, GPIO1 SCL) → Roller485 Motor A at 0x64
- Port C: I2C0 remapped (GPIO17 SDA, GPIO18 SCL) → Roller485 Motor B at 0x65

## HAL Structure
- `src/hardware/mod.rs`: Board initialization (power, display, I2C buses) and bus scanning
- `src/hardware/power.rs`: AXP2101 and AW9523 setup (LCD reset, backlight, port power)
- `src/hardware/roller485.rs`: Roller485 I2C driver (enable, mode, speed, position, encoder)
- `src/display.rs`: Display facade with simple UI helpers (circles and text)
- `src/business/`: Place higher-level business logic (controllers, evaluators)

## Concurrency
- Embassy executor with separate tasks for display and each motor
- Channels (`embassy-sync`) for passing angle updates to the display

## Running
1. Build debug:
	- `cargo build`
2. Build + flash + monitor (release):
	- `cargo run --release`
	- Or select the serial port with `espflash` and run `espflash flash --monitor --chip esp32s3 --port <port> target/xtensa-esp32s3-none-elf/release/m5-minimal`

## Tests (planned)
- Add unit tests for `roller485` packing format using an `embedded-hal`-compatible mock I2C.
- Add property checks for angle-to-step conversion in business layer.

## Next Steps
- Integrate `business/angle_controller.rs` to drive both motors via shared logic.
- Extend display to show status (mode/RPM) per motor and basic diagnostics.
- Document wiring with a diagram and add CI for formatting/linting.

