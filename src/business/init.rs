//! Business logic task orchestration.
//!
//! Wires up business-owned Embassy tasks (motor tests).
//! The hardware layer's SharedRoller485::run_background_task() handles command processing.

use embassy_executor::Spawner;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
};

use crate::business::tasks::run_motor_test;
use crate::hardware::MotorCommand;

/// Initialization errors for business tasks.
#[derive(Debug)]
pub enum InitError {
    /// Spawning the motor test task failed.
    SpawnMotorTest,
}

/// Start business-owned tasks (motor test only).
///
/// Note: Command execution is handled by the hardware layer's
/// SharedRoller485::run_background_task(), which is spawned from main.rs.
pub fn init(
    spawner: &Spawner,
    _motor_a_cmd: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
    motor_b_cmd: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
    motor_b_reset: &'static Channel<CriticalSectionRawMutex, (), 1>,
) -> Result<(), InitError> {
    // Simple test task for Motor B
    spawner
        .spawn(run_motor_test("B", motor_b_cmd, motor_b_reset))
        .map_err(|_| InitError::SpawnMotorTest)?;

    Ok(())
}