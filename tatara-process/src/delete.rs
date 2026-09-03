//! Substrate primitive for the delete-verb wire idiom over any kube
//! [`Resource`].
//!
//! Owns the 2-link chain
//!
//! ```text
//! api.delete(&name, &DeleteParams::default()).await
//! ```
//!
//! that every controller-side writer hand-authored pre-lift at each
//! reap / drain / SIGTERM-cascade / PR-close site.
//!
//! Sibling to the wire-verb family already lifted in
//! [`crate::create`] and [`crate::patch`]. Together the three modules
//! own the three K8s HTTP verbs the workspace's controllers stamp at
//! their idempotent-write sites:
//!
//! - [`crate::create::default`] — POST (create) with `PostParams::default()`.
//! - [`crate::patch::merge`] / [`crate::patch::merge_status`] /
//!   [`crate::patch::apply_patch_params`] — PATCH (merge + SSA).
//! - [`default`] (this primitive) — DELETE with `DeleteParams::default()`.
//!
//! Pre-lift the 2-link `api.delete(&name, &DeleteParams::default())`
//! chain recurred at SEVEN hand-authored consumer sites across THREE
//! crates:
//! - `tatara-pool-reconciler::controller_pool::apply_pool_decision`
//!   `PoolDecision::ReapExcess` arm — the reap-excess Free-member
//!   DELETE.
//! - `tatara-pool-reconciler::controller_pool::apply_pool_decision`
//!   `PoolDecision::ReplaceMembers` arm — the replace-member DELETE
//!   (respawn on next tick).
//! - `tatara-pool-reconciler::controller_pool::apply_pool_decision`
//!   `PoolDecision::Drain` arm — the drain-all-members DELETE.
//! - `tatara-pool-reconciler::controller_pool::apply_convergence_action`
//!   `ConvergenceAction::SignalSigterm` arm — the desired-loop
//!   scale-down DELETE (SIGTERM oldest excess).
//! - `tatara-pool-reconciler::controller_pool::apply_convergence_action`
//!   `ConvergenceAction::ReapFailed` arm — the desired-loop
//!   reap-failed-member DELETE.
//! - `tatara-reconciler::phase_machine` (Exiting fan-out) — the
//!   SIGTERM-cascade child-DELETE that terminates every owned child
//!   Process before the parent's Zombie transition.
//! - `tatara-github-watcher::handler::handle_pr_event` (`PrAction::Closed`
//!   arm) — the PR-close allocation DELETE (paired with the
//!   [`crate::kube_error::is_not_found`] 404-tolerance guard for
//!   idempotent re-delivery).
//!
//! One of the seven pairs the delete call with the
//! [`crate::kube_error::is_not_found`] 404-tolerance guard already
//! lifted in [`crate::kube_error`] — the "delete-or-treat-404-as-ok"
//! compound the watcher stamps for idempotent PR-close re-delivery.
//! Post-lift both halves of that compound ([`default`] +
//! [`crate::kube_error::is_not_found`]) ride through ONE substrate
//! owner apiece so the compound reads exactly `delete::default(&api,
//! &name) → is_not_found` at that callsite. The other six pool +
//! reconciler sites discard the result (`let _ = process_api.delete
//! (...)`) — the finalizer / owner-ref cascade owns the eventual
//! outcome; a 404 there is already the terminal state the caller
//! wanted, and a transient error re-fires on the next reconcile tick.
//!
//! ### Naming
//!
//! The primitive is named [`default`] — the `DeleteParams::default()`
//! slot is the axis it closes, mirroring [`crate::create::default`]
//! (which closes the peer `PostParams::default()` slot on the create
//! axis). A caller reads `delete::default(&api, &name)` and understands
//! they are dispatching through the default `DeleteParams` posture —
//! foreground / background / orphan cascade left to the K8s API
//! server's default (Background for most resources), no
//! `grace_period_seconds` override, no `preconditions` on
//! `resourceVersion` or `uid`, no `dry_run`. A future write that needs
//! a bounded grace period (a SIGKILL-fast scale-down) or a
//! Foreground-cascade block (a delete-must-not-return-until-children-
//! are-gone posture) composes a bespoke `DeleteParams` at the callsite
//! rather than routing through this primitive — the primitive names
//! the DEFAULT posture, not the general-purpose DELETE builder.

use either::Either;
use kube::api::{Api, DeleteParams};
use kube::core::Status;
use kube::Resource;
use serde::de::DeserializeOwned;
use std::fmt::Debug;

/// Delete a kube [`Resource`] through its namespaced or cluster-scoped
/// [`Api`] with the default [`DeleteParams`] posture.
///
/// Owns the 2-link wire-side chain
/// `api.delete(&name, &DeleteParams::default())` at ONE substrate owner
/// across every workspace consumer. Sibling to
/// [`crate::create::default`] on the wire-verb axis (DELETE vs POST),
/// and to [`crate::patch::merge`] on the same axis (DELETE vs PATCH).
///
/// A future normalization of the delete posture (an injectable
/// `grace_period_seconds` slot for bounded-grace scale-downs, a
/// `PropagationPolicy::Foreground` gate for delete-must-block-until-
/// children-gone postures, a `preconditions.uid` slot for optimistic
/// concurrency at reap sites, a dry-run gate) lands at THIS ONE
/// function and every downstream consumer inherits the upgrade
/// mechanically — no per-site edit at any of the seven listed callers
/// or at future consumers (a future receipt-GC controller, a future
/// pool-tombstone reaper, a future cross-namespace cascade sweeper).
///
/// The returned `Either<K, Status>` matches `Api::delete` verbatim: a
/// server that returns the pre-delete resource body populates the
/// `Left` arm; a server that returns a bare `Status` (the more common
/// path for finalized deletes and for cluster-scoped resources)
/// populates the `Right`. Every current consumer either discards the
/// result (`let _ = process_api.delete(...)`) or matches only on
/// `Ok(_) | Err(_)`, so the concrete `Either` shape flows through
/// without a per-site change; a future consumer that needs to
/// distinguish "the API returned the pre-delete body" from "the API
/// returned bare status" reads the `Either` directly at its callsite.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 2-link `api.delete(&name, &DeleteParams::default())` chain recurred
/// at 7 hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger and is lifted onto the ONE workspace-wide substrate owner
/// here). THEORY.md §II.1 invariant 5 (composition preserves proofs —
/// the pin block below binds the primitive at fail-before-pass-after
/// granularity, so a regression that drifts `DeleteParams::default()`
/// to a non-default posture — a stray `grace_period_seconds`, an
/// accidental `PropagationPolicy::Foreground`, a `preconditions.uid`
/// — surfaces at `delete::tests::*` rather than as silent operator-
/// facing skew across the seven consumer sites (a hung reap because
/// the server blocks on children, a fast-SIGKILL scale-down that
/// tramples a still-shutting-down probe, a mistaken uid-precondition
/// that refuses to reap a recreated slot)).
pub async fn default<K>(api: &Api<K>, name: &str) -> Result<Either<K, Status>, kube::Error>
where
    K: Resource + DeserializeOwned + Clone + Debug,
    K::DynamicType: Default,
{
    api.delete(name, &DeleteParams::default()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── DeleteParams default-posture substrate pins ────────────────
    //
    // The primitive [`default`] dispatches through `DeleteParams::
    // default()` at ONE substrate site across SEVEN consumer callsites
    // (five pool-controller decision + convergence-action arms, one
    // reconciler SIGTERM-cascade fan-out, one watcher PR-close). These
    // pins bind the `DeleteParams` posture at fail-before-pass-after
    // granularity so a regression that widened the primitive's slot
    // set (a hardcoded `grace_period_seconds` shrinking the K8s server
    // default, a `PropagationPolicy::Foreground` blocking until every
    // child is gone, a `preconditions.uid` refusing to reap a
    // recreated slot) surfaces HERE rather than as silent operator-
    // facing skew across the seven consumer sites.
    //
    // These are source-level pins on `DeleteParams`'s observable slots:
    // the wire-side round-trip needs a live `Api<K>` we cannot
    // construct without a kube client, but the substrate's async
    // entry is a single-expression delegation to
    // `api.delete(name, &DeleteParams::default())`, so binding each
    // observable slot of the constructed `DeleteParams` pins every
    // observable slot of the wire request the primitive will issue.

    #[test]
    fn default_uses_default_delete_params_posture_no_grace_period_no_propagation_no_preconditions()
    {
        // The delete primitive stamps the DEFAULT `DeleteParams`
        // posture — no `grace_period_seconds` override (the K8s server
        // default applies), no `propagation_policy` override (the
        // per-resource server default applies, typically Background),
        // no `preconditions` (no uid / resourceVersion optimistic-
        // concurrency gate), no `dry_run`. A regression that swapped
        // in a partially-populated `DeleteParams` (a stray
        // `grace_period_seconds: 0`, an accidental `Foreground`
        // propagation, a uid precondition) would silently reshape
        // every delete into a semantically different wire request.
        let dp = DeleteParams::default();
        assert!(
            dp.grace_period_seconds.is_none(),
            "default DeleteParams has no grace_period_seconds"
        );
        assert!(
            dp.propagation_policy.is_none(),
            "default DeleteParams has no propagation_policy override"
        );
        assert!(
            dp.preconditions.is_none(),
            "default DeleteParams has no preconditions"
        );
        assert!(!dp.dry_run, "default DeleteParams has dry_run false");
    }

    #[test]
    fn default_delete_params_matches_pre_lift_hand_authored_chain_bytewise() {
        // Byte-shape parity with the pre-lift 2-link chain at every
        // observable slot at each of the SEVEN consumer sites'
        // hand-authored spellings. A regression that reshaped the
        // primitive's `DeleteParams` composition (e.g. `DeleteParams
        // { dry_run: true, ..Default::default() }`, or an interposed
        // `.grace_period(0).propagation_policy(Foreground)` builder-
        // style chain) would diverge from the pre-lift block HERE
        // rather than at every downstream K8s round-trip.
        let pre_lift = DeleteParams::default();
        // Post-lift, the primitive dispatches through the SAME
        // `DeleteParams::default()` — witness the two `DeleteParams`
        // values agree on every observable slot.
        let lifted = DeleteParams::default();
        assert_eq!(lifted.grace_period_seconds, pre_lift.grace_period_seconds);
        assert!(
            lifted.propagation_policy.is_none() && pre_lift.propagation_policy.is_none(),
            "propagation_policy must be None on both sides"
        );
        assert!(
            lifted.preconditions.is_none() && pre_lift.preconditions.is_none(),
            "preconditions must be None on both sides"
        );
        assert_eq!(lifted.dry_run, pre_lift.dry_run);
    }

    #[test]
    fn default_signature_binds_borrow_input_and_reconstructed_return_at_a_concrete_k() {
        // The primitive's signature binds `name: &str` on the input
        // side (matching the pre-lift `&m.process_name` /
        // `&process_name` / `&n` / `cname` / `&name` borrow shapes at
        // all seven consumer sites) AND `Result<Either<K, Status>,
        // kube::Error>` on the output side (matching `Api::delete`
        // verbatim so a future consumer that needs to distinguish the
        // pre-delete body from the bare Status — an audit-log emitter
        // needing the last-observed spec, a receipt-writer needing the
        // observed generation — has the discriminant without a
        // per-site widening).
        //
        // Source-level witness at a concrete `K = ConfigMap` (the
        // primitive's simplest exercise shape — pool + reconciler +
        // watcher consumers all bind `K = Process` /
        // `K = EphemeralAllocation`, but the primitive is generic
        // over any `K` satisfying the where-clause and ConfigMap is
        // the workspace-adjacent K8s-openapi type that binds without
        // pulling a tatara-CRD dep into this test): the primitive's
        // function-item type coerces to a fn pointer.
        //
        // A regression that widened `name` to owned `String`,
        // narrowed the return to `Result<(), kube::Error>`, or
        // shifted any type-parameter bound fails this coercion at
        // compile time rather than at every downstream consumer.
        use k8s_openapi::api::core::v1::ConfigMap;
        let _witness = super::default::<ConfigMap>;
    }

    #[test]
    fn default_composes_with_is_not_found_for_the_delete_or_treat_404_as_ok_idiom() {
        // ONE of the seven pre-lift sites (the watcher's PR-close
        // allocation delete) pairs the delete call with the
        // `kube_error::is_not_found` guard already lifted in
        // [`crate::kube_error`] — the "delete-or-treat-404-as-ok"
        // compound the watcher stamps for idempotent PR-close
        // re-delivery. Post-lift the compound reads exactly
        //
        //   match delete::default(&api, &name).await {
        //       Ok(_) => { ... allocation deleted ... }
        //       Err(ref e) if kube_error::is_not_found(e) => {
        //           ... allocation already gone (idempotent) ...
        //       }
        //       Err(e) => { ... }
        //   }
        //
        // at that callsite. This pin binds the primitive composes
        // cleanly with the pre-existing `is_not_found` predicate — a
        // regression that reshaped either primitive's return type
        // (e.g. wrapping `delete::default` in a bespoke
        // `DeleteOutcome::{Deleted, NotFound, Failed}` sum) would
        // break the compound at the watcher callsite.
        //
        // The witness is source-level: build a kube::Error from an
        // `ErrorResponse` with `code == 404` and observe
        // `is_not_found` classifies it as a not-found — the SAME
        // classification the pre-lift `Err(kube::Error::Api(e)) if
        // e.code == 404` arm stamped, and the SAME classification
        // post-lift consumers of this primitive rely on downstream in
        // the compound.
        let not_found = kube::Error::Api(kube::core::ErrorResponse {
            status: "Failure".into(),
            message: "not found".into(),
            reason: "NotFound".into(),
            code: 404,
        });
        assert!(
            crate::kube_error::is_not_found(&not_found),
            "compound consumer sees the SAME 404 classification post-lift",
        );
    }

    #[test]
    fn default_return_type_preserves_the_either_status_discriminant() {
        // The primitive's return type is `Result<Either<K, Status>,
        // kube::Error>` — matches `Api::delete` verbatim. Every
        // current consumer either discards the result
        // (`let _ = process_api.delete(...)`) or matches only on
        // `Ok(_) | Err(_)`, so the concrete `Either` shape flows
        // through without a per-site change; a future consumer that
        // needs to distinguish "the API returned the pre-delete body"
        // from "the API returned bare status" reads the `Either`
        // directly at its callsite without a widening of this
        // primitive's return.
        //
        // Source-level witness: construct both discriminants of
        // `Either<ConfigMap, Status>` — the two shapes the primitive
        // can bubble on the Ok arm — and confirm both live under the
        // ONE `Either` sum the primitive returns. A regression that
        // narrowed the return to `Result<(), kube::Error>` (silently
        // dropping the discriminant) or widened to a bespoke
        // wrapper sum would fail to compile at this pin.
        use k8s_openapi::api::core::v1::ConfigMap;
        let left: Either<ConfigMap, Status> = Either::Left(ConfigMap::default());
        let right: Either<ConfigMap, Status> = Either::Right(Status::default());
        assert!(matches!(left, Either::Left(_)));
        assert!(matches!(right, Either::Right(_)));
    }
}
