//! Logging macros backed by `defmt` so output decodes and colors correctly in the monitor.
//!
//! Note: format strings must follow `defmt` formatting rules (implementing `Format`).

/// Log an info message via `defmt::info!`.
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        defmt::info!($($arg)*);
    };
}

/// Log a warning message via `defmt::warn!`.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        defmt::warn!($($arg)*);
    };
}

/// Log a debug message via `defmt::debug!`.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        defmt::debug!($($arg)*);
    };
}
