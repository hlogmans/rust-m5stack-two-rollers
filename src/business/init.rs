//! Business logic task orchestration.
//!
//! Wires up business-owned Embassy tasks (motor command processors, reset, tests).
//! Tasks communicate via channels, not hardware objects, for full decoupling.

use embassy_executor::Spawner;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
};

use crate::business::tasks::{
    run_motor_background,
    run_motor_test,
};
use crate::hardware::MotorCommand;

/// Initialization errors for business tasks.
#[derive(Debug)]
pub enum InitError {
    /// Spawning motor command processor task failed (name included for clarity).
    SpawnMotorCommandProcessor(&'static str),
    /// Spawning the motor test task failed.
    SpawnMotorTest,
}

/// Start business-owned tasks (motor command processors + motor test).
///
/// This function takes command channels instead of motor hardware objects,
/// allowing the business logic to be completely decoupled from the hardware
/// layer. The hardware layer listens to these channels and executes commands.
pub fn init(
    spawner: &Spawner,
    motor_a_cmd: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
    motor_b_cmd: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
    motor_b_reset: &'static Channel<CriticalSectionRawMutex, (), 1>,
) -> Result<(), InitError> {
    // Command processor for Motor A
    spawner
        .spawn(run_motor_background("A", motor_a_cmd))
        .map_err(|_| InitError::SpawnMotorCommandProcessor("A"))?;

    // Command processor for Motor B
    spawner
        .spawn(run_motor_background("B", motor_b_cmd))
        .map_err(|_| InitError::SpawnMotorCommandProcessor("B"))?;

    // Simple test task for Motor B
    spawner
        .spawn(run_motor_test("B", motor_b_cmd, motor_b_reset))
        .map_err(|_| InitError::SpawnMotorTest)?;

    Ok(())
}