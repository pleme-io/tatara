//! Top-level reconcile function — dispatches to per-phase handlers.
//!
//! Pre-dispatch checks, in priority order:
//!   1. Deletion: `metadata.deletionTimestamp` set + alive phase → force Exiting.
//!   2. Signal: consume `tatara.pleme.io/signal` annotation before phase work.
//!   3. Suspend: honor `spec.suspended` (or persisted SIGSTOP) — pause heartbeat.
//!   4. Phase: dispatch to `phase_machine::handle_*`.

use std::sync::Arc;

use kube::runtime::controller::Action;
use tracing::{info, warn};

use tatara_process::prelude::*;

use crate::context::Context;
use crate::{patch, phase_machine, signals};

/// Reconcile a Process. Top-level dispatcher; phase handlers do the work.
pub async fn reconcile(process: Arc<Process>, ctx: Arc<Context>) -> Result<Action, kube::Error> {
    let name = process.metadata.name.as_deref().unwrap_or("<unnamed>");
    let ns = process.metadata.namespace.as_deref().unwrap_or("default");
    // Phase seed rides through the substrate primitive
    // `Process::observed_phase_or_pending` — pre-lift this was a
    // hand-authored `.observed_phase().unwrap_or(ProcessPhase::
    // Pending)` two-link chain, one of FOUR workspace-wide
    // restatements past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // threshold (peers at `boundary::evaluate_process_phase`,
    // `table_controller::stable_name_group_key`, and
    // `tatara-pool-reconciler::controller_pool::reconcile_pool`).
    // Post-lift the four consumers share ONE substrate owner; a
    // future generation-filter or staleness-gate normalization
    // lands at `tatara_process::prelude::Process::
    // observed_phase_or_pending` and every consumer inherits it
    // mechanically.
    let current_phase = process.observed_phase_or_pending();

    info!(namespace = ns, name, phase = %current_phase, "reconcile");

    // 1. Deletion preempts everything — force Exiting if still alive.
    //    Tombstone probe rides through the ONE substrate primitive
    //    `Process::is_being_deleted`, sibling to the same-corner
    //    child-fan-out DELETE-skip in `phase_machine::handle_exiting`.
    if process.is_being_deleted() && current_phase.is_alive() {
        let api = ctx.process_api(ns);
        // Compose+dispatch rides through the substrate peer
        // `patch::transition_msg` — pre-lift this was a hand-authored
        // `patch::patch_process_status(&api, name,
        // patch::phase_status_msg(<phase>, <msg>))` chain, one of NINE
        // workspace-wide restatements past the ★★ PRIME-DIRECTIVE ≥ 2
        // duplication trigger; post-lift the compose+dispatch sink
        // lives at ONE substrate owner (peer of `patch::transition`
        // on the bare-transition axis).
        if let Err(e) =
            patch::transition_msg(&api, name, ProcessPhase::Exiting, "deletion requested").await
        {
            warn!(namespace = ns, name, error = %e, "force-Exiting patch failed; requeuing");
        } else {
            info!(namespace = ns, name, "→ Exiting (deletionTimestamp set)");
        }
        // Fast re-poll rides through the ONE substrate composer
        // `tatara_process::requeue::tick` — pre-lift this + the
        // signal-consumed arm below hand-authored a bare `1` literal
        // at `after_secs(1)`, restating the SAME "immediate re-poll
        // after a state-change latch" intent the sibling
        // `phase_machine::TICK_RETRY` const bound at 10 workspace-
        // wide handler-tail sites, past the ★★ PRIME-DIRECTIVE ≥ 2
        // duplication threshold across two files. Post-lift both
        // sites here + the 10 phase_machine sites read the semantic
        // helper and the intent → second-count binding lives at ONE
        // typed owner.
        return Ok(tatara_process::requeue::tick());
    }

    // 2. Signal ingestion — only while the Process is still alive.
    //    Dead processes ignore all signals (per Unix).
    if current_phase.is_alive() {
        match signals::ingest(&process, &ctx).await {
            Ok(Some(signal)) => {
                let effect =
                    signals::apply(current_phase, signal, process.spec.signals.sighup_strategy);
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
                return Ok(tatara_process::requeue::tick());
            }
            Ok(None) => {}
            Err(e) => warn!(error = %e, "signal ingestion failed; continuing"),
        }
    }

    // 3. Suspend check.
    if process.spec.suspended && current_phase.is_alive() {
        return Ok(tatara_process::requeue::after_secs(
            ctx.config.heartbeat_seconds,
        ));
    }

    // 4. Phase dispatch.
    let next = match current_phase {
        ProcessPhase::Pending => phase_machine::handle_pending(&process, &ctx).await,
        ProcessPhase::Forking => phase_machine::handle_forking(&process, &ctx).await,
        ProcessPhase::Execing => phase_machine::handle_execing(&process, &ctx).await,
        ProcessPhase::Running => phase_machine::handle_running(&process, &ctx).await,
        ProcessPhase::Attested => phase_machine::handle_attested(&process, &ctx).await,
        ProcessPhase::Reconverging => phase_machine::handle_reconverging(&process, &ctx).await,
        ProcessPhase::Releasing => phase_machine::handle_releasing(&process, &ctx).await,
        ProcessPhase::Exiting => phase_machine::handle_exiting(&process, &ctx).await,
        ProcessPhase::Failed => phase_machine::handle_failed(&process, &ctx).await,
        ProcessPhase::Zombie => phase_machine::handle_zombie(&process, &ctx).await,
        ProcessPhase::Reaped => phase_machine::handle_reaped(&process, &ctx).await,
    };

    match next {
        Ok(action) => Ok(action),
        Err(e) => {
            warn!(namespace = ns, name, error = %e, "reconcile error — requeuing");
            // Reconcile-error back-off rides through the ONE
            // substrate composer `tatara_process::requeue::heartbeat`
            // — pre-lift this + the sibling `error_policy` return +
            // the two `table_controller.rs` sites hand-authored a
            // bare `30` literal, restating the SAME "back off to the
            // default steady-state heartbeat cadence after an error"
            // intent the sibling `phase_machine::HEARTBEAT` const
            // bound at 7 handler-tail sites, past the ★★
            // PRIME-DIRECTIVE ≥ 2 duplication threshold across three
            // files.
            Ok(tatara_process::requeue::heartbeat())
        }
    }
}

/// kube-runtime error policy — used for `Controller::run`.
pub fn error_policy(_proc: Arc<Process>, err: &kube::Error, _ctx: Arc<Context>) -> Action {
    warn!(error = %err, "controller error; requeuing");
    // Peer to the reconcile-error sink above — same "back off to
    // heartbeat cadence after an error" intent, same substrate
    // composer.
    tatara_process::requeue::heartbeat()
}
