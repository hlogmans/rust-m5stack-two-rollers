//! Business logic task orchestration.
//!
//! Wires up business-owned Embassy tasks including motor background processing,
//! command handling, and reset handlers.

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
    /// Spawning a motor background task failed.
    SpawnMotorBackground,
    /// Spawning the motor reset handler failed.
    SpawnMotorResetHandler,
}

/// Initialize all business-owned motor tasks.
///
/// This includes:
/// - Motor background processing (command handler + encoder polling)
/// - Motor reset handlers for A/B
/// - Motor test task (Motor B only)
///
/// # Arguments
/// - `spawner`: Embassy task spawner
/// - `motor_a`: Motor A shared device
/// - `motor_b`: Motor B shared device
/// - `motor_a_cmd`: Command channel for Motor A
/// - `motor_b_cmd`: Command channel for Motor B
/// - `motor_a_reset`: Reset trigger channel for Motor A
/// - `motor_b_reset`: Reset trigger channel for Motor B
/// - `angle_a_watch`: Motor A angle telemetry channel
/// - `speed_a_watch`: Motor A speed telemetry channel
/// - `speed_b_watch`: Motor B speed telemetry channel
///
/// # Returns
/// - `Ok(())` on success
/// - `Err(InitError)` if any task fails to spawn
pub fn init_motors(
    spawner: &Spawner,
    motor_a: &'static crate::hardware::SharedRoller485<crate::hardware::RollerI2cDevice>,
    motor_b: &'static crate::hardware::SharedRoller485<crate::hardware::RollerI2cDevice>,
    motor_a_cmd: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
    motor_b_cmd: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
    motor_a_reset: &'static Channel<CriticalSectionRawMutex, (), 1>,
    motor_b_reset: &'static Channel<CriticalSectionRawMutex, (), 1>,
    angle_a_watch: &'static embassy_sync::watch::Watch<CriticalSectionRawMutex, u16, 8>,
    speed_a_watch: &'static embassy_sync::watch::Watch<CriticalSectionRawMutex, f32, 4>,
    _speed_b_watch: &'static embassy_sync::watch::Watch<CriticalSectionRawMutex, f32, 4>,
) -> Result<(), InitError> {
    use crate::business::tasks::run_motor_reset_handler;

    // Spawn Motor A background task (listens to commands, polls encoder, reports telemetry)
    spawner
        .spawn(run_motor_a_background(motor_a.clone()))
        .map_err(|_| InitError::SpawnMotorBackground)?;

    // Spawn Motor B background task (listens to commands, polls encoder, reports telemetry)
    spawner
        .spawn(run_motor_b_background(motor_b.clone()))
        .map_err(|_| InitError::SpawnMotorBackground)?;

    // Spawn motor reset handler for Motor A
    spawner
        .spawn(run_motor_reset_handler(
            "A",
            motor_a_cmd,
            angle_a_watch,
            speed_a_watch,
            motor_a_reset,
        ))
        .map_err(|_| InitError::SpawnMotorResetHandler)?;

    // Spawn simple test task for Motor B
    spawner
        .spawn(run_motor_test("B", motor_b_cmd, motor_b_reset))
        .map_err(|_| InitError::SpawnMotorTest)?;

    Ok(())
}

/// Task: Motor A background processing (command handler + encoder polling)
#[embassy_executor::task]
async fn run_motor_a_background(
    motor: crate::hardware::SharedRoller485<crate::hardware::RollerI2cDevice>,
) {
    motor.run_background_task("A", None).await
}

/// Task: Motor B background processing (command handler + encoder polling)
#[embassy_executor::task]
async fn run_motor_b_background(
    motor: crate::hardware::SharedRoller485<crate::hardware::RollerI2cDevice>,
) {
    motor.run_background_task("B", None).await
}

/// Backward compatibility: Old init function signature.
/// 
/// Kept for compatibility with existing code. Use init_motors() for new code.
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