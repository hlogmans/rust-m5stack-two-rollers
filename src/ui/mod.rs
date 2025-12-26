//! User Interface Module
//!
//! MVVM architecture with framework abstraction for easy UI framework switching.
//!
//! Structure:
//! - `view_models/`: ViewModels hold presentation state (framework-agnostic)
//! - `views/`: Views render ViewModels to screen (framework-specific)
//! - `framework/`: Framework abstraction layer (currently embedded-layout)

pub mod view_models;
pub mod views;
pub mod framework;

pub use view_models::{DashboardViewModel, MotorViewModel};
pub use views::DashboardView;
