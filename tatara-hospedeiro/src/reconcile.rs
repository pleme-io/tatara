//! Bridge `tatara_process::GuestIntent` → `GuestSupervisor`.
//!
//! `GuestIntent` in the Process CRD carries the `GuestSpec` as a
//! `serde_json::Value` to keep `tatara-process` decoupled from
//! `tatara-vm`. Hospedeiro owns the dispatch: re-parse the value into
//! a typed `GuestSpec`, hand it to the supervisor, map the resulting
//! `GuestStatus` back onto the Process CRD's `ProcessPhase`.
//!
//! Today the call is synchronous and one-shot — reconcile_intent
//! returns a terminal status. That matches H.6's "run to completion"
//! semantics. Watcher integration (owner refs, finalizers, SIGTERM
//! plumbing from the `tatara.pleme.io/signal` annotation) lands in
//! H.7.2 once hospedeiro grows a background supervisor thread.

use tatara_process::prelude::GuestIntent;
use tatara_vm::GuestSpec;

use crate::supervisor::{GuestSupervisor, SupervisorError};
use crate::GuestStatus;

/// Drive an `Intent::Guest` through one reconciliation pass.
///
/// # Errors
/// * `ReconcileError::ParseSpec` — the intent's embedded JSON isn't a
///   valid `GuestSpec` shape (upstream authoring error).
/// * `ReconcileError::Supervisor` — anything the supervisor rejects
///   (missing transport, failed build, engine error, VM backend not
///   yet wired, etc.). The error carries the precise reason.
pub fn reconcile_intent(
    sup: &mut GuestSupervisor,
    intent: &GuestIntent,
) -> Result<GuestStatus, ReconcileError> {
    let spec: GuestSpec = serde_json::from_value(intent.spec.clone())
        .map_err(|e| ReconcileError::ParseSpec(e.to_string()))?;
    sup.boot(&spec).map_err(ReconcileError::Supervisor)
}

/// Map a terminal `GuestStatus` back onto the canonical Process CRD
/// `ProcessPhase`. Informational — the reconciler's loop does the
/// actual CRD status patch.
///
/// - `Reaped` → `Attested` (clean terminal)
/// - `Failed` / `Zombie` → `Failed`
/// - everything else → `Running` (still in flight)
#[must_use]
pub fn guest_status_to_process_phase(
    status: GuestStatus,
) -> tatara_process::prelude::ProcessPhase {
    use tatara_process::prelude::ProcessPhase;
    match status {
        GuestStatus::Reaped => ProcessPhase::Attested,
        GuestStatus::Failed | GuestStatus::Zombie => ProcessPhase::Failed,
        GuestStatus::Building
        | GuestStatus::Forking
        | GuestStatus::Execing
        | GuestStatus::Running
        | GuestStatus::Exiting => ProcessPhase::Running,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[error("parse spec from intent.guest.spec JSON: {0}")]
    ParseSpec(String),

    #[error("supervisor: {0}")]
    Supervisor(SupervisorError),
}
