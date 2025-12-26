//! ViewModels - Presentation state (framework-agnostic)
//!
//! ViewModels hold the state for UI rendering but don't know HOW to render.
//! They can be tested without any graphics framework.

mod motor_view_model;
mod dashboard_view_model;

pub use motor_view_model::MotorViewModel;
pub use dashboard_view_model::DashboardViewModel;
