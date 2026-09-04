//! Signal ingestion — read `tatara.pleme.io/signal` annotations, parse,
//! strip, apply effect to phase.

use anyhow::{anyhow, Result};
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
    match signal {
        // SIGHUP arms collapsed through the typed `SighupStrategy::sighup_target`
        // projection: target phase is purely a function of the strategy variant,
        // the `is_running()` phase guard is the strategy-independent precondition.
        // `Noop` projects to `None` and falls through to `SignalEffect::Noop`
        // regardless of phase, preserving the pre-lift behavior.
        Sighup => match sighup.sighup_target() {
            Some(target) if phase.is_running() => SignalEffect::TransitionTo(target),
            _ => SignalEffect::Noop,
        },
        Sigterm if phase.is_alive() => SignalEffect::TransitionTo(ProcessPhase::Exiting),
        Sigkill if !phase.is_terminal() => SignalEffect::TransitionTo(ProcessPhase::Reaped),
        Sigusr1 if phase.is_running() => SignalEffect::ForceAttest,
        Sigusr2 if phase.is_running() => SignalEffect::Remediate,
        Sigstop => SignalEffect::Suspend,
        Sigcont => SignalEffect::Resume,
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
    // Annotation lookup via the substrate primitive — pre-lift this
    // was a hand-authored 3-line `.metadata.annotations.as_ref()
    // .and_then(|a| a.get(SIGNAL_ANNOTATION)).cloned()` chain, one of
    // THREE workspace-wide restatements past the ★★ PRIME-DIRECTIVE
    // ≥ 2 duplication threshold (peers at
    // `phase_machine::released_from_annotation` and
    // `tatara-pool-reconciler::controller_pool::process_belongs_to_pool`).
    // Post-lift the three lookups route through ONE substrate owner
    // [`Process::annotation`]; this callsite reapplies its owned
    // `.map(str::to_string)` tail at its own site so the downstream
    // `ProcessSignal::from_str` parse consumes the same owned
    // `String` it did pre-lift.
    let raw = process.annotation(SIGNAL_ANNOTATION).map(str::to_string);
    let Some(raw) = raw else {
        return Ok(None);
    };

    // Owned coordinates via the substrate primitive — pre-lift this
    // was a hand-authored 2-slot unwrap chain, one of two adjacent
    // identical restatements in this file (peer at
    // `consume_effect`), and shared its shape with 10 more
    // restatements past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // threshold in `crate::phase_machine`. Post-lift all 12 sites
    // route through ONE substrate owner
    // [`Process::owned_coordinates_or_err`]; the error wording
    // ("Process has no metadata.name") is preserved verbatim by the
    // substrate so log-line greps that anchored on it keep matching.
    let (ns, name) = process.owned_coordinates_or_err()?;

    // Always strip — JSON merge patch interprets `null` as "remove key".
    let api = ctx.process_api(&ns);
    let strip = json!({
        "metadata": {
            "annotations": { SIGNAL_ANNOTATION: serde_json::Value::Null }
        }
    });
    // Wire-side dispatch rides the substrate primitive
    // `tatara_process::patch::merge` — pre-lift this was a hand-
    // authored `api.patch(&name, &PatchParams::default(),
    // &Patch::Merge(&strip))` chain, one of SIX workspace-wide
    // restatements past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // threshold (adjacent peers at `consume_effect`'s Suspend + Resume
    // arms in this file). Post-lift the primary-resource merge posture
    // lives at ONE substrate owner (peer of `merge_status` on the
    // `/status` subresource axis + `apply_patch_params` on the SSA
    // wire-posture axis).
    tatara_process::patch::merge(&api, &name, &strip)
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
    // Owned coordinates via the substrate primitive — sibling site
    // to the `ingest` restatement above; both routed through
    // [`Process::owned_coordinates_or_err`] post-lift so a future
    // normalization (case-fold, cross-cluster prefix, a rename of
    // the "default" fallback, an alternate error wording) reaches
    // both signal handlers plus the 10 `crate::phase_machine`
    // callers through ONE substrate owner.
    let (ns, name) = process.owned_coordinates_or_err()?;
    let api = ctx.process_api(&ns);

    match effect {
        SignalEffect::Noop => Ok(()),
        SignalEffect::TransitionTo(phase) => {
            patch::patch_process_status(&api, &name, patch::phase_status(phase, None))
                .await
                .map_err(|e| anyhow!("transition via signal: {e}"))?;
            Ok(())
        }
        SignalEffect::ForceAttest => {
            // Flip back to Running — re-verify + re-attest without changing spec.
            patch::patch_process_status(
                &api,
                &name,
                patch::phase_status_msg(ProcessPhase::Running, "forced re-attestation (SIGUSR1)"),
            )
            .await
            .map_err(|e| anyhow!("force attest: {e}"))?;
            Ok(())
        }
        SignalEffect::Suspend => {
            // Wire-side dispatch rides the substrate primitive
            // `tatara_process::patch::merge`; the merge body itself
            // rides the substrate composer
            // `tatara_process::patch::spec_suspended_body` — pre-lift
            // this arm hand-authored BOTH the `api.patch(&name,
            // &PatchParams::default(), &Patch::Merge(&body))` 3-link
            // chain (one of six workspace-wide restatements at the
            // `merge` primitive lift) AND the `json!({ "spec": {
            // "suspended": true } })` body composition (one of two
            // adjacent restatements at the composer lift, peer arm at
            // Resume below). Post-lift both restatements ride through
            // ONE substrate owner each: a future addition to the
            // suspend/resume wire body (a `by:` signal-source slot, a
            // `suspendedAt:` transition timestamp, a symmetry gate
            // that refuses conflicting overlays) lands at
            // `spec_suspended_body` and both arms inherit it
            // mechanically.
            tatara_process::patch::merge(
                &api,
                &name,
                &tatara_process::patch::spec_suspended_body(true),
            )
            .await
            .map_err(|e| anyhow!("suspend: {e}"))?;
            Ok(())
        }
        SignalEffect::Resume => {
            // Peer arm to Suspend above — see that arm's docstring for
            // the composer + wire-dispatch substrate-lift story. Both
            // arms compose the merge body through
            // `tatara_process::patch::spec_suspended_body` (this arm
            // feeds `false`, the peer feeds `true`) and dispatch it
            // through `tatara_process::patch::merge`; a future addition
            // to the shared wire body lands at ONE composer and both
            // arms inherit it.
            tatara_process::patch::merge(
                &api,
                &name,
                &tatara_process::patch::spec_suspended_body(false),
            )
            .await
            .map_err(|e| anyhow!("resume: {e}"))?;
            Ok(())
        }
        SignalEffect::Remediate => {
            // Trigger reconverge with a note; real remediation hooks (kensa) land later.
            patch::patch_process_status(
                &api,
                &name,
                patch::phase_status_msg(
                    ProcessPhase::Reconverging,
                    "remediate requested (SIGUSR2)",
                ),
            )
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
