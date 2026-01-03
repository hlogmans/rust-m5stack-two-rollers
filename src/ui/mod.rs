//! User Interface Module
//!
//! MVVM architecture with framework abstraction for easy UI framework switching.
//!
//! Structure:
//! - `view_models/`: ViewModels hold presentation state (framework-agnostic)
//! - `views/`: Views render ViewModels to screen (framework-specific)
//! - `framework/`: Framework abstraction layer (currently embedded-layout)
//! - `screen`: Navigation types
//! - `init`: Task spawning and orchestration

pub mod view_models;
pub mod views;
pub mod framework;
pub mod screen;
pub mod buttons;
pub mod screen_trait;
pub mod display_service;
pub mod init;

pub use screen_trait::{ScreenController, ScreenEvent};
pub use display_service::DisplayService;
pub use init::{init_display_service, init_navigation, init_touch_reader};

pub use view_models::{DashboardViewModel, MotorViewModel};
pub use views::{DashboardView, SplashView};
pub use screen::Screen;
