//! Signal ingestion — read `tatara.pleme.io/signal` annotations, parse,
//! strip, apply effect to phase.

use anyhow::{anyhow, Result};
use kube::api::{Api, Patch, PatchParams};
use serde_json::json;
use std::str::FromStr;
use tracing::warn;

use tatara_process::annotations;
use tatara_process::phase::ProcessPhase;
use tatara_process::prelude::Process;
use tatara_process::signal::{ProcessSignal, SighupStrategy};

use crate::context::Context;
use crate::patch;

/// What the phase machine should do in response to one drained signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalEffect {
    /// Leave phase alone.
    Noop,
    /// Transition to a specific phase next tick.
    TransitionTo(ProcessPhase),
    /// Re-attest without phase change.
    ForceAttest,
    /// Pause reconciliation.
    Suspend,
    /// Resume reconciliation.
    Resume,
    /// Remediate (invoke kensa hooks).
    Remediate,
}

pub fn apply(phase: ProcessPhase, signal: ProcessSignal, sighup: SighupStrategy) -> SignalEffect {
    use ProcessSignal::*;
    match (signal, sighup) {
        (Sighup, SighupStrategy::Noop) => SignalEffect::Noop,
        (Sighup, SighupStrategy::Reconverge) if phase.is_running() => {
            SignalEffect::TransitionTo(ProcessPhase::Reconverging)
        }
        (Sighup, SighupStrategy::Restart) if phase.is_running() => {
            SignalEffect::TransitionTo(ProcessPhase::Exiting)
        }
        (Sigterm, _) if phase.is_alive() => SignalEffect::TransitionTo(ProcessPhase::Exiting),
        (Sigkill, _) if !phase.is_terminal() => SignalEffect::TransitionTo(ProcessPhase::Reaped),
        (Sigusr1, _) if phase.is_running() => SignalEffect::ForceAttest,
        (Sigusr2, _) if phase.is_running() => SignalEffect::Remediate,
        (Sigstop, _) => SignalEffect::Suspend,
        (Sigcont, _) => SignalEffect::Resume,
        _ => SignalEffect::Noop,
    }
}

pub const SIGNAL_ANNOTATION: &str = annotations::SIGNAL;

/// Read + strip the signal annotation on a Process.
///
/// Signals are one-shot: we remove the annotation even when parsing fails,
/// so a typo in `kubectl annotate` doesn't wedge the reconcile loop forever.
/// Returns `Ok(Some(signal))` on valid parse, `Ok(None)` otherwise.
pub async fn ingest(process: &Process, ctx: &Context) -> Result<Option<ProcessSignal>> {
    let raw = process
        .metadata
        .annotations
        .as_ref()
        .and_then(|a| a.get(SIGNAL_ANNOTATION))
        .cloned();
    let Some(raw) = raw else {
        return Ok(None);
    };

    let ns = process
        .metadata
        .namespace
        .clone()
        .unwrap_or_else(|| "default".into());
    let name = process
        .metadata
        .name
        .clone()
        .ok_or_else(|| anyhow!("Process has no metadata.name"))?;

    // Always strip — JSON merge patch interprets `null` as "remove key".
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);
    let strip = json!({
        "metadata": {
            "annotations": { SIGNAL_ANNOTATION: serde_json::Value::Null }
        }
    });
    api.patch(&name, &PatchParams::default(), &Patch::Merge(&strip))
        .await
        .map_err(|e| anyhow!("strip signal annotation: {e}"))?;

    match ProcessSignal::from_str(&raw) {
        Ok(s) => Ok(Some(s)),
        Err(_) => {
            warn!(
                namespace = %ns,
                name = %name,
                annotation = %raw,
                "unknown signal; stripped without effect"
            );
            Ok(None)
        }
    }
}

/// Apply a `SignalEffect` by patching the Process.
pub async fn consume_effect(process: &Process, ctx: &Context, effect: SignalEffect) -> Result<()> {
    let ns = process
        .metadata
        .namespace
        .clone()
        .unwrap_or_else(|| "default".into());
    let name = process
        .metadata
        .name
        .clone()
        .ok_or_else(|| anyhow!("Process has no metadata.name"))?;
    let api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);

    match effect {
        SignalEffect::Noop => Ok(()),
        SignalEffect::TransitionTo(phase) => {
            let body = json!({
                "phase": phase,
                "phaseSince": chrono::Utc::now(),
            });
            patch::patch_process_status(&api, &name, body)
                .await
                .map_err(|e| anyhow!("transition via signal: {e}"))?;
            Ok(())
        }
        SignalEffect::ForceAttest => {
            // Flip back to Running — re-verify + re-attest without changing spec.
            let body = json!({
                "phase": ProcessPhase::Running,
                "phaseSince": chrono::Utc::now(),
                "message": "forced re-attestation (SIGUSR1)",
            });
            patch::patch_process_status(&api, &name, body)
                .await
                .map_err(|e| anyhow!("force attest: {e}"))?;
            Ok(())
        }
        SignalEffect::Suspend => {
            api.patch(
                &name,
                &PatchParams::default(),
                &Patch::Merge(&json!({ "spec": { "suspended": true } })),
            )
            .await
            .map_err(|e| anyhow!("suspend: {e}"))?;
            Ok(())
        }
        SignalEffect::Resume => {
            api.patch(
                &name,
                &PatchParams::default(),
                &Patch::Merge(&json!({ "spec": { "suspended": false } })),
            )
            .await
            .map_err(|e| anyhow!("resume: {e}"))?;
            Ok(())
        }
        SignalEffect::Remediate => {
            // Trigger reconverge with a note; real remediation hooks (kensa) land later.
            let body = json!({
                "phase": ProcessPhase::Reconverging,
                "phaseSince": chrono::Utc::now(),
                "message": "remediate requested (SIGUSR2)",
            });
            patch::patch_process_status(&api, &name, body)
                .await
                .map_err(|e| anyhow!("remediate: {e}"))?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sighup_reconverge_on_running() {
        assert_eq!(
            apply(
                ProcessPhase::Running,
                ProcessSignal::Sighup,
                SighupStrategy::Reconverge
            ),
            SignalEffect::TransitionTo(ProcessPhase::Reconverging)
        );
    }

    #[test]
    fn sigterm_on_attested_begins_exit() {
        assert_eq!(
            apply(
                ProcessPhase::Attested,
                ProcessSignal::Sigterm,
                SighupStrategy::Noop
            ),
            SignalEffect::TransitionTo(ProcessPhase::Exiting)
        );
    }

    #[test]
    fn sigkill_on_zombie_reaps() {
        assert_eq!(
            apply(
                ProcessPhase::Zombie,
                ProcessSignal::Sigkill,
                SighupStrategy::Noop
            ),
            SignalEffect::TransitionTo(ProcessPhase::Reaped)
        );
    }

    #[test]
    fn sigusr1_only_when_running() {
        assert_eq!(
            apply(
                ProcessPhase::Pending,
                ProcessSignal::Sigusr1,
                SighupStrategy::Noop
            ),
            SignalEffect::Noop
        );
        assert_eq!(
            apply(
                ProcessPhase::Attested,
                ProcessSignal::Sigusr1,
                SighupStrategy::Noop
            ),
            SignalEffect::ForceAttest
        );
    }
}
