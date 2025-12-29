//! Views - Rendering layer (framework-specific)
//!
//! Views know HOW to render ViewModels using a specific UI framework.
//! Currently uses embedded-graphics + embedded-layout.

mod dashboard_view;
mod motor_view;
mod splash_view;

pub use dashboard_view::DashboardView;
pub use motor_view::MotorView;
pub use splash_view::SplashView;
