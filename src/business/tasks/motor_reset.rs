use embassy_executor::task;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
    watch::Watch,
};
use embassy_time::{Duration, Timer};

use crate::hardware::MotorCommand;
use crate::info;

/// Task: handle motor reset to zero via command channel (decoupled from hardware)
///
/// Business logic for homing/reset: sends position commands and monitors feedback
/// through channels/watches, independent of the actual motor driver.
#[task(pool_size = 2)]
pub async fn run_motor_reset_handler(
    name: &'static str,
    cmd_ch: &'static Channel<CriticalSectionRawMutex, MotorCommand, 4>,
    angle_watch: &'static Watch<CriticalSectionRawMutex, u16, 8>,
    speed_watch: &'static Watch<CriticalSectionRawMutex, f32, 4>,
    reset_ch: &'static Channel<CriticalSectionRawMutex, (), 1>,
) {
    info!("Motor {} reset handler starting", name);

    let mut angle_receiver = angle_watch
        .receiver()
        .expect("Need angle receiver for reset handler");
    let mut speed_receiver = speed_watch
        .receiver()
        .expect("Need speed receiver for reset handler");

    loop {
        reset_ch.receive().await;

        info!("Reset {} to zero requested", name);

        if angle_receiver.get().await == 0 {
            info!("{} already at zero", name);
            continue;
        }

        cmd_ch.send(MotorCommand::SetPosition(0)).await;

        info!("Waiting {} movement start...", name);
        let _ = speed_receiver.changed_and(|v| v.abs() > 0.2f32).await;

        info!("Waiting {} movement stop...", name);
        let _ = speed_receiver.changed_and(|v| *v == 0.0f32).await;

        Timer::after(Duration::from_millis(100)).await;
        cmd_ch.send(MotorCommand::SetReading).await;

        info!("Reset {} complete", name);
    }
}
