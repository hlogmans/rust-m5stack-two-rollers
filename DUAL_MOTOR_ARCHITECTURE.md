# Dual Motor Architecture - Design Challenge & Solution

## Current Status
- ✅ Single motor (Motor A) working on I2C1 (Grove PORT.A) at address 0x64
- ❌ Dual motor support not yet implemented due to I2C bus ownership model in esp-hal

## The Problem: I2C Bus Ownership

### Root Cause
The `Roller485` driver struct takes **ownership** of the I2C bus:

```rust
pub struct Roller485<I2C> {
    i2c: I2C,    // Takes ownership
    address: u8,
}
```

In Rust's ownership model, only one struct can own the I2C bus. When `roller_a` owns `i2c1_bus`, it's impossible for `roller_b` to also own it.

### Why This Matters
The Roller485 protocol requires the ability to communicate with multiple devices on the same I2C bus by varying the slave address (0x64 for Motor A, 0x65 for Motor B). However, the current driver design forces one-to-one coupling between a motor driver and an I2C instance.

## Available Solutions

### Option 1: Refactor Roller485 to Use I2C References ⭐ RECOMMENDED
Instead of owning the I2C bus, make the driver accept a mutable reference:

```rust
pub struct Roller485<'i, I2C: I2c> {
    i2c: &'i mut I2C,  // Borrow instead of own
    address: u8,
}

impl<'i, I2C: I2c> Roller485<'i, I2C> {
    pub fn new(i2c: &'i mut I2C) -> Self {
        Self::new_with_address(i2c, 0x64)
    }
}
```

**Pros:**
- Minimal refactoring (~20 lines changed)
- Allows sequential access to motors (one at a time)
- No additional dependencies

**Cons:**
- Motors cannot run truly concurrently with embedded_hal_async
- Requires careful lifetime management in spawned tasks
- May have borrowing issues with Embassy tasks

**Implementation Notes:**
- Update all `self.i2c.write()` to `self.i2c.write()` (unchanged interface)
- Update Board::init() to pass `&mut i2c1_bus` to each motor
- Tasks must serialize access (use a Mutex or mutex-like guard if parallelism needed)

### Option 2: Use embedded-hal-bus (Bus Manager Pattern)
The `embedded-hal-bus` crate provides abstractions for sharing I2C buses:

```rust
use embedded_hal_bus::i2c::BusManager;

// Create a bus manager (allocation-free)
let bus_manager = BusManager::<I2c<'static, Blocking>>::new(i2c1_bus);

// Acquire independent I2C interfaces
let motor_a_bus = bus_manager.acquire_i2c();
let motor_b_bus = bus_manager.acquire_i2c();

let roller_a = Roller485::new(motor_a_bus);
let roller_b = Roller485::new_with_address(motor_b_bus, 0x65);
```

**Pros:**
- No refactoring of Roller485 needed
- Built for this exact use case
- Handles bus arbitration automatically

**Cons:**
- `embedded-hal-bus` v0.3 doesn't have a suitable `I2c` manager (it has SPI, but I2C support is limited)
- May need to use `critical-section` or `Mutex` based managers
- Adds dependency on synchronization primitives

**Status:** Requires further research on available APIs in `embedded-hal-bus` v0.3

### Option 3: Arc<Mutex<I2C>> Wrapper
Wrap the I2C bus in a thread-safe reference:

```rust
use alloc::sync::Arc;
use core::cell::RefCell;
use critical_section::Mutex;

type SharedI2C = Arc<Mutex<I2c<'static, Blocking>>>;

pub struct Roller485<I2C> {
    i2c: SharedI2C,
    address: u8,
}
```

**Pros:**
- Allows true concurrent access
- Clear ownership semantics

**Cons:**
- Adds synchronization overhead
- Requires ESP-IDF or RTOS for proper Mutex
- Overkill for simple sequential access
- Changes the type signature significantly

## Recommendation: Option 1 (Refactor for References)

### Why Option 1?
1. **Simplicity**: Only ~30 lines of code changes in `roller485.rs`
2. **No dependencies**: Uses only existing embedded-hal traits
3. **Performance**: No synchronization overhead
4. **Correctness**: Lifetime system ensures safe access
5. **Flexibility**: Can upgrade to Mutex/channels later if needed

### Implementation Plan

#### Step 1: Update Roller485 struct
```rust
pub struct Roller485<'i, I2C> {
    i2c: &'i mut I2C,
    address: u8,
}
```

#### Step 2: Update Board struct
```rust
pub struct Board<'a> {
    pub display: Display<CoreS3Display<'a>>,
    pub roller_a: Roller485<'a, I2c<'a, Blocking>>,
    pub roller_b: Roller485<'a, I2c<'a, Blocking>>,
}
```

#### Step 3: Update Board::init()
```rust
let mut i2c1_bus = I2c::new(...);

// Sequential initialization
let mut roller_a = Roller485::new(&mut i2c1_bus);
let _ = roller_a.init();

let mut roller_b = Roller485::new_with_address(&mut i2c1_bus, 0x65);
let _ = roller_b.init();

// Return motors (both borrow i2c1_bus for lifetime 'a)
Self { display, roller_a, roller_b }
```

#### Step 4: Update embassy tasks
With references, the motors will need serialized access. Options:
- Move I2C bus into a shared task that coordinates motor commands
- Use a command queue/message passing pattern
- Accept sequential-only motor control (simpler)

## Testing Strategy

### Phase 1: Single Motor (Current)
✅ Already working
- Motor A at 0x64 on I2C1
- Encoder reading functional
- Position control working
- Speed adjustment working

### Phase 2: Dual Motor Initialization
Test both motors initialize without errors:
1. Refactor Roller485 for references
2. Initialize Motor A (0x64)
3. Initialize Motor B (0x65)
4. Verify both report correct modes

### Phase 3: Dual Motor Control (Sequential)
Test that commands can be sent to either motor:
1. Motor A: 30° steps, 2s interval
2. Motor B: 45° backward, 1.5s interval
3. Verify both respond correctly

### Phase 4: Enhanced Concurrency (Future)
If parallel motor control is needed:
1. Implement message-passing (Embassy channel)
2. Create motor controller task that manages I2C access
3. Send commands from other tasks via channels

## Hardware Configuration Notes

### I2C Buses
- **I2C0** (GPIO12 SDA, GPIO11 SCL): AXP2101 PMU + AW9523 GPIO expander
- **I2C1** (GPIO2 SDA, GPIO1 SCL): Grove PORT.A (where both Roller485 motors are connected)

### Roller485 Configuration
- **Motor A**: I2C Address 0x64 (default)
- **Motor B**: I2C Address 0x65 (must be set on device)

### Wiring
Both motors appear to be on the same Grove PORT.A (I2C1) with different addresses.

## Next Steps

1. Refactor `Roller485<'i, I2C>` struct to accept `&'i mut I2C` instead of owned I2C
2. Update all method implementations to work with mutable references
3. Update `Board::init()` to pass mutable references
4. Update embassy task signatures and motor spawning logic
5. Test dual motor initialization and basic movement
6. Consider implementing a command queue if parallel control is needed

## References

- [M5Stack Roller485 Protocol Documentation](https://docs.m5stack.com/)
- [Embassy Async Runtime Patterns](https://embassy.dev/)
- [embedded-hal I2C Trait](https://docs.rs/embedded-hal/latest/embedded_hal/i2c/trait.I2c.html)
- [Rust Ownership & Borrowing](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html)
