//! Framework abstraction layer
//!
//! This module isolates framework-specific dependencies.
//! To switch UI frameworks, replace this module.

// Currently using embedded-graphics + embedded-layout
// If switching to Slint/LVGL/etc, replace these exports

pub use embedded_graphics;
pub use embedded_layout;
