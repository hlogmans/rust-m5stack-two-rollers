//! Business logic layer
//! 
//! This module contains the application's business logic, separated from
//! hardware abstractions. This allows for easier testing and modularity.

pub mod angle_controller;
pub mod evaluator;

pub use angle_controller::AngleController;
pub use evaluator::{EvaluationStatus, Evaluator, ThresholdEvaluator};
