//! Signal ingestion — read `tatara.pleme.io/signal` annotations, parse,
//! strip, apply effect to phase.

use anyhow::Result;
use std::str::FromStr;
use tracing::warn;

use tatara_process::annotations;
use tatara_process::kube_error::KubeResultExt;
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
    // Wire-body composition rides the substrate primitive
    // `tatara_process::patch::annotation_body` — pre-lift this was a
    // hand-authored `json!({"metadata": {"annotations": {SIGNAL_ANNOTATION:
    // serde_json::Value::Null}}})` chain, one of THREE workspace-wide
    // restatements of the single-annotation merge-body shape past the ★★
    // PRIME-DIRECTIVE ≥ 2 duplication threshold (peers at `phase_machine
    // ::transition_to_releasing`'s RELEASED_FROM stamp + `tatara-pool-
    // reconciler::controller_allocation`'s return-trigger stamp). Post-
    // lift the single-annotation merge-body posture lives at ONE
    // substrate owner; passing `Value::Null` at the value slot rides
    // through as JSON null verbatim so the K8s API server's JSON-merge-
    // patch strip semantics fire unchanged.
    let api = ctx.process_api(&ns);
    let strip = tatara_process::patch::annotation_body(SIGNAL_ANNOTATION, serde_json::Value::Null);
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
        .kube_ctx("strip signal annotation")?;

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
    // Owned coordinates + namespaced Api via the composed substrate
    // primitive — sibling site to the `ingest` restatement above; both
    // routed through [`Process::owned_coordinates_or_err`] post-lift so
    // a future normalization (case-fold, cross-cluster prefix, a rename
    // of the "default" fallback, an alternate error wording) reaches
    // both signal handlers plus every `crate::phase_machine` handler-entry
    // through ONE substrate owner. Composed shape from
    // [`Context::owned_process_binding`] on the (ns, name, api) tuple
    // axis — closes the two-line duet every handler-entry authored
    // pre-lift onto ONE primitive.
    let (ns, name, api) = ctx.owned_process_binding(process)?;

    match effect {
        SignalEffect::Noop => Ok(()),
        SignalEffect::TransitionTo(phase) => {
            patch::transition(&api, &name, phase)
                .await
                .kube_ctx("transition via signal")?;
            Ok(())
        }
        SignalEffect::ForceAttest => {
            // Flip back to Running — re-verify + re-attest without changing spec.
            patch::transition_msg(
                &api,
                &name,
                ProcessPhase::Running,
                "forced re-attestation (SIGUSR1)",
            )
            .await
            .kube_ctx("force attest")?;
            Ok(())
        }
        SignalEffect::Suspend => {
            // Compose+dispatch rides through the substrate peer
            // `tatara_process::patch::merge_suspended` — pre-lift this
            // arm hand-authored the 2-link `merge(&api, &name,
            // &spec_suspended_body(true))` chain, one of TWO
            // workspace-wide restatements past the ★★ PRIME-DIRECTIVE
            // ≥ 2 duplication trigger (peer at the Resume arm below).
            // Post-lift the compose+dispatch sink lives at ONE
            // substrate owner; a future addition to the suspend/resume
            // wire body (a `by:` signal-source slot, a `suspendedAt:`
            // transition timestamp, a symmetry gate that refuses
            // conflicting overlays) OR a future normalization of the
            // wire dispatch (a shared retry-budget wrap, an injectable
            // `dry_run` mode) lands at THIS ONE compose+dispatch
            // peer + the underlying `spec_suspended_body` / `merge`
            // primitives, and both arms inherit the upgrade
            // mechanically.
            tatara_process::patch::merge_suspended(&api, &name, true)
                .await
                .kube_ctx("suspend")?;
            Ok(())
        }
        SignalEffect::Resume => {
            // Peer arm to Suspend above — see that arm's docstring for
            // the compose+dispatch substrate-lift story. Both arms
            // route through `tatara_process::patch::merge_suspended`
            // (this arm feeds `false`, the peer feeds `true`); a
            // future addition to the shared wire body or dispatch
            // posture lands at the substrate peer and both arms
            // inherit it mechanically.
            tatara_process::patch::merge_suspended(&api, &name, false)
                .await
                .kube_ctx("resume")?;
            Ok(())
        }
        SignalEffect::Remediate => {
            // Trigger reconverge with a note; real remediation hooks (kensa) land later.
            patch::transition_msg(
                &api,
                &name,
                ProcessPhase::Reconverging,
                "remediate requested (SIGUSR2)",
            )
            .await
            .kube_ctx("remediate")?;
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
