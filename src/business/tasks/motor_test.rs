use embassy_executor::task;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
};
use embassy_time::{Duration, Timer};

use crate::hardware::MotorCommand;
use crate::info;

/// Task: run motor test sequence via command channel (decoupled from hardware)
///
/// Sends test commands through a channel instead of directly accessing motor hardware.
/// This allows the same test logic to work with any motor driver.
#[task(pool_size = 2)]
pub async fn run_motor_test(
    name: &'static str,
    cmd_ch: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
    reset_ch: &'static Channel<CriticalSectionRawMutex, (), 1>,
) {
    loop {
        reset_ch.receive().await;
        info!("Motor {} test task starting", name);
        let mut speed = 10000i32;

        for _ in 0..10 {
            cmd_ch.send(MotorCommand::SetSpeed(speed)).await;
            speed *= -2;

            // a next value in the reset_ch will interrupt the test
            // so, which is earlier, the timeout or a reset?
            let timeout_fut = Timer::after(Duration::from_millis(500));
            let reset_fut = reset_ch.receive();
            let result = embassy_futures::select::select(timeout_fut, reset_fut).await;
            if result.is_first() {
                // timeout completed, continue to next speed
                continue;
            } else {
                // reset received, abort test
                info!("Motor {} test task aborted by reset", name);
                break;
            }
        }

        cmd_ch.send(MotorCommand::SetSpeed(0)).await;
        cmd_ch.send(MotorCommand::SetReading).await;
        info!("Motor {} test task complete", name);
    }
}
