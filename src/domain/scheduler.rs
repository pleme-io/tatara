use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use super::evaluation::Evaluator;
use super::state_store::StateStore;
use crate::client::executor::Executor;

/// Runs the scheduling loop: evaluates pending jobs and dispatches allocations.
pub struct Scheduler {
    evaluator: Evaluator,
    executor: Arc<Executor>,
    eval_interval: Duration,
}

impl Scheduler {
    pub fn new(store: Arc<StateStore>, executor: Arc<Executor>, eval_interval_secs: u64) -> Self {
        Self {
            evaluator: Evaluator::new(store),
            executor,
            eval_interval: Duration::from_secs(eval_interval_secs),
        }
    }

    /// Run the scheduler loop until cancelled.
    pub async fn run(&self) -> Result<()> {
        info!("Scheduler started (interval: {:?})", self.eval_interval);
        let mut interval = tokio::time::interval(self.eval_interval);

        loop {
            interval.tick().await;

            match self.evaluator.evaluate().await {
                Ok(allocations) => {
                    for alloc in allocations {
                        info!(
                            alloc_id = %alloc.id,
                            job_id = %alloc.job_id,
                            group = %alloc.group_name,
                            node = %alloc.node_id,
                            "Created allocation"
                        );
                        if let Err(e) = self.executor.start_allocation(alloc).await {
                            warn!(error = %e, "Failed to start allocation");
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Evaluation cycle failed");
                }
            }
        }
    }
}
