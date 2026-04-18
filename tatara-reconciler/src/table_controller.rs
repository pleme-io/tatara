//! ProcessTable reconciler — `/proc` singleton that aggregates Process status,
//! reaps orphans and zombies, hands out PIDs.

use std::sync::Arc;
use std::time::Duration;

use kube::runtime::controller::Action;
use tracing::info;

use tatara_process::prelude::*;

use crate::context::Context;

pub async fn reconcile(
    _table: Arc<ProcessTable>,
    _ctx: Arc<Context>,
) -> Result<Action, kube::Error> {
    // 1. List all Process resources cluster-wide.
    // 2. Rebuild `status.processes[]` from the list.
    // 3. Compute `processCount` / `readyCount`.
    // 4. Zombie detection: for each Process in Zombie for > zombie_timeout,
    //    patch with deletionGracePeriodSeconds=0.
    // 5. Orphan reaping: if orphan_reaping_enabled and a Process has a
    //    parent_pid that no longer exists in the table, terminate it.
    // 6. PID sequence: ensure `next_sequence` is >= (max existing child + 1).
    info!("ProcessTable heartbeat");
    Ok(Action::requeue(Duration::from_secs(30)))
}

pub fn error_policy(_t: Arc<ProcessTable>, err: &kube::Error, _ctx: Arc<Context>) -> Action {
    tracing::warn!(error = %err, "ProcessTable reconcile error; requeuing");
    Action::requeue(Duration::from_secs(30))
}
