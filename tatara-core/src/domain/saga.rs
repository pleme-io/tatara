//! Saga pattern types for multi-step provisioning with compensation.
//!
//! Based on Garcia-Molina & Salem 1987: each step in a distributed
//! transaction has a compensating action. On failure, compensations
//! execute in reverse order, ensuring no dangling resources.
//!
//! This module defines result/tracking types only.
//! The async saga executor lives in tatara-engine (which has tokio).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Result of executing a saga.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SagaResult {
    /// All steps completed successfully.
    Completed { steps_run: usize },
    /// A step failed. Compensations were executed.
    Compensated {
        failed_step: String,
        error: String,
        steps_completed: usize,
        compensations_run: usize,
        compensation_errors: Vec<String>,
    },
}

impl SagaResult {
    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }

    pub fn is_compensated(&self) -> bool {
        matches!(self, Self::Compensated { .. })
    }
}

impl fmt::Display for SagaResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Completed { steps_run } => {
                write!(f, "saga completed ({steps_run} steps)")
            }
            Self::Compensated {
                failed_step,
                error,
                compensations_run,
                compensation_errors,
                ..
            } => {
                write!(
                    f,
                    "saga failed at '{}': {}. {} compensations run",
                    failed_step, error, compensations_run
                )?;
                if !compensation_errors.is_empty() {
                    write!(f, " ({} compensation errors)", compensation_errors.len())?;
                }
                Ok(())
            }
        }
    }
}

/// Tracks the progress of a saga during execution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SagaProgress {
    /// Steps completed so far.
    pub completed_steps: Vec<String>,
    /// Current step being executed (if any).
    pub current_step: Option<String>,
    /// Whether the saga is in compensation mode.
    pub compensating: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_saga_result_completed() {
        let result = SagaResult::Completed { steps_run: 5 };
        assert!(result.is_completed());
        assert!(!result.is_compensated());
        assert_eq!(format!("{result}"), "saga completed (5 steps)");
    }

    #[test]
    fn test_saga_result_compensated() {
        let result = SagaResult::Compensated {
            failed_step: "start_driver".to_string(),
            error: "wasmtime not found".to_string(),
            steps_completed: 3,
            compensations_run: 3,
            compensation_errors: vec![],
        };
        assert!(result.is_compensated());
        assert!(format!("{result}").contains("start_driver"));
    }

    #[test]
    fn test_saga_progress_default() {
        let progress = SagaProgress::default();
        assert!(progress.completed_steps.is_empty());
        assert!(!progress.compensating);
    }
}
