//! Screen navigation types
//!
//! Defines which screen is currently active in the UI

/// The active screen in the UI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Splash screen (startup)
    Splash,
    /// Main dashboard
    Dashboard,
}

impl Default for Screen {
    fn default() -> Self {
        Self::Splash
    }
}
