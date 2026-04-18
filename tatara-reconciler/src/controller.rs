//! Top-level reconcile function — dispatches to per-phase handlers.
//!
//! Pre-dispatch checks, in priority order:
//!   1. Deletion: `metadata.deletionTimestamp` set + alive phase → force Exiting.
//!   2. Signal: consume `tatara.pleme.io/signal` annotation before phase work.
//!   3. Suspend: honor `spec.suspended` (or persisted SIGSTOP) — pause heartbeat.
//!   4. Phase: dispatch to `phase_machine::handle_*`.

use std::sync::Arc;
use std::time::Duration;

use kube::api::Api;
use kube::runtime::controller::Action;
use serde_json::json;
use tracing::{info, warn};

use tatara_process::prelude::*;

use crate::context::Context;
use crate::{patch, phase_machine, signals};

/// Reconcile a Process. Top-level dispatcher; phase handlers do the work.
pub async fn reconcile(process: Arc<Process>, ctx: Arc<Context>) -> Result<Action, kube::Error> {
    let name = process.metadata.name.as_deref().unwrap_or("<unnamed>");
    let ns = process.metadata.namespace.as_deref().unwrap_or("default");
    let current_phase = process
        .status
        .as_ref()
        .map(|s| s.phase)
        .unwrap_or(ProcessPhase::Pending);

    info!(namespace = ns, name, phase = %current_phase, "reconcile");

    // 1. Deletion preempts everything — force Exiting if still alive.
    if process.metadata.deletion_timestamp.is_some() && current_phase.is_alive() {
        let api: Api<Process> = Api::namespaced(ctx.kube.clone(), ns);
        let body = json!({
            "phase": ProcessPhase::Exiting,
            "phaseSince": chrono::Utc::now(),
            "message": "deletion requested",
        });
        if let Err(e) = patch::patch_process_status(&api, name, body).await {
            warn!(namespace = ns, name, error = %e, "force-Exiting patch failed; requeuing");
        } else {
            info!(namespace = ns, name, "→ Exiting (deletionTimestamp set)");
        }
        return Ok(Action::requeue(Duration::from_secs(1)));
    }

    // 2. Signal ingestion — only while the Process is still alive.
    //    Dead processes ignore all signals (per Unix).
    if current_phase.is_alive() {
        match signals::ingest(&process, &ctx).await {
            Ok(Some(signal)) => {
                let effect = signals::apply(
                    current_phase,
                    signal,
                    process.spec.signals.sighup_strategy,
                );
                info!(
                    namespace = ns,
                    name,
                    signal = %signal,
                    effect = ?effect,
                    "signal received"
                );
                if let Err(e) = signals::consume_effect(&process, &ctx, effect).await {
                    warn!(error = %e, "signal effect apply failed");
                }
                return Ok(Action::requeue(Duration::from_secs(1)));
            }
            Ok(None) => {}
            Err(e) => warn!(error = %e, "signal ingestion failed; continuing"),
        }
    }

    // 3. Suspend check.
    if process.spec.suspended && current_phase.is_alive() {
        return Ok(Action::requeue(Duration::from_secs(
            ctx.config.heartbeat_seconds,
        )));
    }

    // 4. Phase dispatch.
    let next = match current_phase {
        ProcessPhase::Pending => phase_machine::handle_pending(&process, &ctx).await,
        ProcessPhase::Forking => phase_machine::handle_forking(&process, &ctx).await,
        ProcessPhase::Execing => phase_machine::handle_execing(&process, &ctx).await,
        ProcessPhase::Running => phase_machine::handle_running(&process, &ctx).await,
        ProcessPhase::Attested => phase_machine::handle_attested(&process, &ctx).await,
        ProcessPhase::Reconverging => phase_machine::handle_reconverging(&process, &ctx).await,
        ProcessPhase::Exiting => phase_machine::handle_exiting(&process, &ctx).await,
        ProcessPhase::Failed => phase_machine::handle_failed(&process, &ctx).await,
        ProcessPhase::Zombie => phase_machine::handle_zombie(&process, &ctx).await,
        ProcessPhase::Reaped => phase_machine::handle_reaped(&process, &ctx).await,
    };

    match next {
        Ok(action) => Ok(action),
        Err(e) => {
            warn!(namespace = ns, name, error = %e, "reconcile error — requeuing");
            Ok(Action::requeue(Duration::from_secs(30)))
        }
    }
}

/// kube-runtime error policy — used for `Controller::run`.
pub fn error_policy(_proc: Arc<Process>, err: &kube::Error, _ctx: Arc<Context>) -> Action {
    warn!(error = %err, "controller error; requeuing");
    Action::requeue(Duration::from_secs(30))
}
