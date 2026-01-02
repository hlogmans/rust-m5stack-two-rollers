//! Business logic layer
//! 
//! This module contains the application's business logic, separated from
//! hardware abstractions. This allows for easier testing and modularity.

pub mod angle_controller;
pub mod evaluator;
pub mod input;
pub mod init;
pub mod tasks;

pub use angle_controller::AngleController;
pub use evaluator::{EvaluationStatus, Evaluator, ThresholdEvaluator};
pub use init::{init, InitError};
pub use tasks::{run_motor_background, run_motor_reset_handler, run_motor_test};
