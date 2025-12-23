//! Value evaluator
//!
//! Provides evaluation logic for determining the status (color) of a value.
//! This abstraction allows for flexible evaluation criteria that can be
//! configured or changed without modifying the display logic.

use embedded_graphics::pixelcolor::Rgb565;

/// Status/color result of an evaluation
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EvaluationStatus {
    /// Good status - typically green
    Good,
    /// Warning status - typically yellow
    Warning,
    /// Critical status - typically red
    Critical,
}

impl EvaluationStatus {
    /// Convert status to RGB565 color
    pub fn to_color(&self) -> Rgb565 {
        match self {
            EvaluationStatus::Good => Rgb565::new(0, 63, 0),     // Green
            EvaluationStatus::Warning => Rgb565::new(63, 63, 0), // Yellow
            EvaluationStatus::Critical => Rgb565::new(31, 0, 0), // Red
        }
    }
}

/// Trait for evaluating a value and returning its status
pub trait Evaluator {
    /// Evaluate a value and return its status
    fn evaluate(&self, value: u16) -> EvaluationStatus;
}

/// Simple threshold-based evaluator
///
/// Evaluates values based on two thresholds:
/// - value < good_threshold: Good (green)
/// - value > critical_threshold: Critical (red)
/// - otherwise: Warning (yellow)
pub struct ThresholdEvaluator {
    good_threshold: u16,
    critical_threshold: u16,
}

impl ThresholdEvaluator {
    /// Create a new threshold evaluator
    ///
    /// # Arguments
    /// * `good_threshold` - Values below this are considered good
    /// * `critical_threshold` - Values above this are considered critical
    ///
    /// # Example
    /// ```ignore
    /// let evaluator = ThresholdEvaluator::new(100, 300);
    /// // < 100 = green, > 300 = red, 100-300 = yellow
    /// ```
    pub fn new(good_threshold: u16, critical_threshold: u16) -> Self {
        Self {
            good_threshold,
            critical_threshold,
        }
    }
}

impl Evaluator for ThresholdEvaluator {
    fn evaluate(&self, value: u16) -> EvaluationStatus {
        if value < self.good_threshold {
            EvaluationStatus::Good
        } else if value > self.critical_threshold {
            EvaluationStatus::Critical
        } else {
            EvaluationStatus::Warning
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threshold_evaluator_good() {
        let evaluator = ThresholdEvaluator::new(100, 300);
        assert_eq!(evaluator.evaluate(50), EvaluationStatus::Good);
        assert_eq!(evaluator.evaluate(99), EvaluationStatus::Good);
    }

    #[test]
    fn test_threshold_evaluator_warning() {
        let evaluator = ThresholdEvaluator::new(100, 300);
        assert_eq!(evaluator.evaluate(100), EvaluationStatus::Warning);
        assert_eq!(evaluator.evaluate(200), EvaluationStatus::Warning);
        assert_eq!(evaluator.evaluate(300), EvaluationStatus::Warning);
    }

    #[test]
    fn test_threshold_evaluator_critical() {
        let evaluator = ThresholdEvaluator::new(100, 300);
        assert_eq!(evaluator.evaluate(301), EvaluationStatus::Critical);
        assert_eq!(evaluator.evaluate(350), EvaluationStatus::Critical);
    }
}
