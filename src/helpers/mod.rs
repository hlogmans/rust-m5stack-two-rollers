//! Generic helper utilities for async embedded systems
//!
//! This module contains reusable abstractions that are not specific to any hardware.

pub mod memory;
pub mod telemetry;

pub use memory::{get_heap_stats, get_psram_size, print_memory_diagnostics, print_memory_stats};
pub use telemetry::TelemetrySender;
