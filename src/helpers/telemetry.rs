//! Channel-agnostic telemetry sender abstraction
//!
//! Provides a unified interface for sending telemetry data that abstracts over
//! Embassy's Watch (latest-value) and Channel (all-values) primitives.
//!
//! This is similar to a ISender<T> interface in C# - the sender doesn't know
//! or care about the underlying delivery semantics.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::watch::Watch;

/// Channel-agnostic telemetry sender that abstracts over Watch and Channel
///
/// # Semantics
/// - **Watch**: Receivers get the latest value only, can miss intermediate updates
/// - **Channel**: All values are queued, no updates are missed (until full)
/// - **Both**: Send to both Watch and Channel simultaneously
///
/// # Example
/// ```ignore
/// // Create sender for angle updates via Watch
/// let sender = TelemetrySender::from_angle_watch(&ANGLE_WATCH);
/// sender.send(180); // Motor doesn't know it's a Watch
///
/// // Or use Channel for logging all values
/// let sender = TelemetrySender::from_angle_channel(&ANGLE_CHANNEL);
/// sender.send(180); // Same API, different semantics
/// ```
#[derive(Clone)]
pub enum TelemetrySender<T, const N: usize>
where
    T: Copy + 'static,
{
    /// Watch sender - latest value only, receivers can miss intermediate updates
    Watch(embassy_sync::watch::Sender<'static, CriticalSectionRawMutex, T, N>),
    /// Channel sender - all values queued, no updates are missed
    Channel(&'static Channel<CriticalSectionRawMutex, T, N>),
    /// Both - send to both Watch and Channel simultaneously
    Both(
        embassy_sync::watch::Sender<'static, CriticalSectionRawMutex, T, N>,
        &'static Channel<CriticalSectionRawMutex, T, N>,
    ),
}

impl<T, const N: usize> TelemetrySender<T, N>
where
    T: Copy + 'static,
{
    /// Send a value via the configured channel(s)
    /// Watch sends are always non-blocking, Channel sends use try_send
    pub fn send(&self, value: T) {
        match self {
            TelemetrySender::Watch(sender) => {
                let _ = sender.send(value);
            }
            TelemetrySender::Channel(channel) => {
                let _ = channel.try_send(value);
            }
            TelemetrySender::Both(watch_sender, channel) => {
                let _ = watch_sender.send(value);
                let _ = channel.try_send(value);
            }
        }
    }

    /// Create telemetry sender from a Watch channel
    pub fn from_watch(watch: &'static Watch<CriticalSectionRawMutex, T, N>) -> Self {
        TelemetrySender::Watch(watch.sender())
    }

    /// Create telemetry sender from a Channel
    pub fn from_channel(channel: &'static Channel<CriticalSectionRawMutex, T, N>) -> Self {
        TelemetrySender::Channel(channel)
    }

    /// Create telemetry sender that sends to both Watch and Channel
    pub fn from_both(
        watch: &'static Watch<CriticalSectionRawMutex, T, N>,
        channel: &'static Channel<CriticalSectionRawMutex, T, N>,
    ) -> Self {
        TelemetrySender::Both(watch.sender(), channel)
    }
}
