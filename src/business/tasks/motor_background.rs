use embassy_executor::task;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
};

use crate::hardware::MotorCommand;
use crate::info;

/// Task: monitor background motor status and apply filtering (decoupled from hardware)
///
/// This task listens for incoming commands on the motor command channel and would
/// apply filtering/conditioning before processing. Currently passes through.
/// In a real scenario, this could apply speed ramping, smoothing, etc.
#[task(pool_size = 2)]
pub async fn run_motor_background(
    name: &'static str,
    cmd_ch: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
) {
    info!("Motor {} background task starting", name);
    // Listen for commands on the channel
    // In a full implementation, this would apply filtering and transformation
    loop {
        let cmd = cmd_ch.receive().await;
        info!("Motor {} background got command: {:?}", name, cmd);
        // Process/filter command here if needed
        // For now, just log it
    }
}
