//! Generic helper utilities for async embedded systems
//!
//! This module contains reusable abstractions that are not specific to any hardware.

pub mod telemetry;

pub use telemetry::TelemetrySender;
