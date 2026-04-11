//! Saga executor — multi-step provisioning with compensation on failure.
//!
//! Executes saga steps in order. On failure at any step, compensates all
//! previously completed steps in reverse order. Uses the existing
//! SagaResult and SagaProgress types from tatara-core.

use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, error, info};

use tatara_core::domain::saga::SagaResult;

/// A single step in a saga.
#[async_trait]
pub trait SagaStep: Send + Sync {
    /// Step name for logging.
    fn name(&self) -> &str;

    /// Execute the forward action. Returns output for potential compensation.
    async fn execute(&self) -> Result<serde_json::Value>;

    /// Compensate (undo) this step using the output from execute.
    async fn compensate(&self, output: &serde_json::Value) -> Result<()>;
}

/// Executes a sequence of saga steps with compensation.
pub struct SagaExecutor {
    steps: Vec<Box<dyn SagaStep>>,
}

impl SagaExecutor {
    pub fn new(steps: Vec<Box<dyn SagaStep>>) -> Self {
        Self { steps }
    }

    /// Execute all steps in order. On failure, compensate in reverse.
    pub async fn run(&self) -> SagaResult {
        let mut completed: Vec<(usize, serde_json::Value)> = Vec::new();

        for (i, step) in self.steps.iter().enumerate() {
            debug!(step = step.name(), index = i, "saga: executing step");

            match step.execute().await {
                Ok(output) => {
                    info!(step = step.name(), "saga: step completed");
                    completed.push((i, output));
                }
                Err(e) => {
                    error!(
                        step = step.name(),
                        error = %e,
                        "saga: step failed — compensating"
                    );

                    // Compensate in reverse order
                    let mut compensation_errors = Vec::new();
                    for (j, output) in completed.iter().rev() {
                        let comp_step = &self.steps[*j];
                        debug!(step = comp_step.name(), "saga: compensating");
                        if let Err(comp_err) = comp_step.compensate(output).await {
                            error!(
                                step = comp_step.name(),
                                error = %comp_err,
                                "saga: compensation failed"
                            );
                            compensation_errors.push(format!(
                                "{}: {}",
                                comp_step.name(),
                                comp_err
                            ));
                        }
                    }

                    return SagaResult::Compensated {
                        failed_step: step.name().to_string(),
                        error: e.to_string(),
                        steps_completed: completed.len(),
                        compensations_run: completed.len() - compensation_errors.len(),
                        compensation_errors,
                    };
                }
            }
        }

            SagaResult::Completed {
            steps_run: self.steps.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct SuccessStep {
        name: String,
        log: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl SagaStep for SuccessStep {
        fn name(&self) -> &str {
            &self.name
        }
        async fn execute(&self) -> Result<serde_json::Value> {
            self.log.lock().unwrap().push(format!("exec:{}", self.name));
            Ok(serde_json::json!({"step": self.name}))
        }
        async fn compensate(&self, _output: &serde_json::Value) -> Result<()> {
            self.log
                .lock()
                .unwrap()
                .push(format!("comp:{}", self.name));
            Ok(())
        }
    }

    struct FailStep {
        name: String,
        log: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl SagaStep for FailStep {
        fn name(&self) -> &str {
            &self.name
        }
        async fn execute(&self) -> Result<serde_json::Value> {
            self.log.lock().unwrap().push(format!("exec:{}", self.name));
            Err(anyhow::anyhow!("step failed"))
        }
        async fn compensate(&self, _output: &serde_json::Value) -> Result<()> {
            self.log
                .lock()
                .unwrap()
                .push(format!("comp:{}", self.name));
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_all_success() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let executor = SagaExecutor::new(vec![
            Box::new(SuccessStep { name: "a".into(), log: log.clone() }),
            Box::new(SuccessStep { name: "b".into(), log: log.clone() }),
            Box::new(SuccessStep { name: "c".into(), log: log.clone() }),
        ]);

        let result = executor.run().await;
        assert!(matches!(result, SagaResult::Completed { steps_run: 3 }));
        assert_eq!(*log.lock().unwrap(), vec!["exec:a", "exec:b", "exec:c"]);
    }

    #[tokio::test]
    async fn test_fail_at_step_2_compensates_step_1() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let executor = SagaExecutor::new(vec![
            Box::new(SuccessStep { name: "a".into(), log: log.clone() }),
            Box::new(FailStep { name: "b".into(), log: log.clone() }),
            Box::new(SuccessStep { name: "c".into(), log: log.clone() }),
        ]);

        let result = executor.run().await;
        assert!(matches!(result, SagaResult::Compensated { .. }));
        // Should execute a, then b fails, then compensate a
        // c should never execute
        let events = log.lock().unwrap().clone();
        assert!(events.contains(&"exec:a".to_string()));
        assert!(events.contains(&"exec:b".to_string()));
        assert!(!events.contains(&"exec:c".to_string()));
        assert!(events.contains(&"comp:a".to_string()));
    }

    #[tokio::test]
    async fn test_empty_saga() {
        let executor = SagaExecutor::new(vec![]);
        let result = executor.run().await;
        assert!(matches!(result, SagaResult::Completed { steps_run: 0 }));
    }

    #[tokio::test]
    async fn test_single_step_success() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let executor = SagaExecutor::new(vec![
            Box::new(SuccessStep { name: "only".into(), log: log.clone() }),
        ]);

        let result = executor.run().await;
        assert!(matches!(result, SagaResult::Completed { steps_run: 1 }));
    }

    #[tokio::test]
    async fn test_first_step_fails_no_compensation() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let executor = SagaExecutor::new(vec![
            Box::new(FailStep { name: "first".into(), log: log.clone() }),
            Box::new(SuccessStep { name: "second".into(), log: log.clone() }),
        ]);

        let result = executor.run().await;
        assert!(matches!(result, SagaResult::Compensated { steps_completed: 0, .. }));
        // No compensation needed — nothing completed before failure
        let events = log.lock().unwrap().clone();
        assert!(!events.contains(&"comp:first".to_string()));
    }
}
