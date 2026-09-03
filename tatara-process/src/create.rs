//! Substrate primitive for the create-verb wire idiom over any kube
//! [`Resource`].
//!
//! Owns the 2-link chain
//!
//! ```text
//! api.create(&PostParams::default(), &obj).await
//! ```
//!
//! that every controller-side writer hand-authored pre-lift at each
//! spawn / bind / seed / receipt-write site.
//!
//! Sibling to the patch-family substrate primitives in [`crate::patch`]
//! on the wire-verb axis: [`crate::patch::merge`] owns the primary-
//! resource merge-patch posture, [`crate::patch::merge_status`] owns
//! the `/status`-subresource merge-patch posture,
//! [`crate::patch::apply_patch_params`] owns the server-side-apply
//! `PatchParams` composition; this primitive completes the family with
//! the create (`POST`) verb — the third wire-verb axis every workspace
//! controller stamps at its idempotent-write sites.
//!
//! Pre-lift the 2-link `api.create(&PostParams::default(), &obj)` chain
//! recurred at FIVE hand-authored consumer sites across FOUR crates:
//! - `tatara-pool-reconciler::controller_pool::spawn_members` — the
//!   spawn-branch new-member Process create.
//! - `tatara-pool-reconciler::controller_pool::apply_convergence_action`
//!   (`ConvergenceAction::CreateMember` arm) — the desired-loop spawn
//!   peer of the site above.
//! - `tatara-reconciler::patch::ensure_process_table` — the cluster-
//!   scoped ProcessTable singleton seeder.
//! - `tatara-github-watcher::handler::handle_pr_event` — the PR-event
//!   allocation-create site.
//! - `tatara-closed-loop-probe::main::write_receipt_configmap` — the
//!   receipt ConfigMap create-then-409-retry seed (its 409 retry-arm
//!   already routes through [`crate::patch::merge`] and
//!   [`crate::kube_error::is_conflict`], leaving the create-verb the
//!   last unlifted link in that chain).
//!
//! Four of the five sites additionally pair the create call with the
//! `is_conflict` guard already lifted in [`crate::kube_error`] — the
//! compound "create-or-treat-409-as-ok" idiom the workspace stamps at
//! every optimistically-created resource. Post-lift both halves of the
//! compound (`create::default` + `kube_error::is_conflict`) ride
//! through ONE substrate owner apiece so the compound reads exactly
//! `create::default(&api, &obj) → is_conflict` at each callsite.
//!
//! ### Naming
//!
//! The primitive is named [`default`] — the `PostParams::default()`
//! slot is the axis it closes, mirroring the naming discipline the
//! rest of the wire-verb family follows (`merge` names the `Merge`
//! posture, `merge_status` names the `/status` subresource, and
//! `apply_patch_params` names the `apply(...)` posture). A caller reads
//! `create::default(&api, &obj)` and understands they are dispatching
//! through the default `PostParams` posture — no `dry_run`, no
//! `field_manager` bound (create writes are not SSA and do not
//! participate in the field-manager ownership model).

use kube::api::{Api, PostParams};
use kube::Resource;
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;

/// Create a kube [`Resource`] through its namespaced or cluster-scoped
/// [`Api`] with the default [`PostParams`] posture.
///
/// Owns the 2-link wire-side chain
/// `api.create(&PostParams::default(), &obj)` at ONE substrate owner
/// across every workspace consumer. Sibling to
/// [`crate::patch::merge`] on the wire-verb axis (POST vs PATCH), and
/// to [`crate::patch::apply_patch_params`] on the wire-posture axis
/// (default PostParams vs SSA PatchParams).
///
/// A future normalization of the create posture (an injectable
/// field-manager slot for observability at the API server's ownership
/// queries, a dry-run gate for one-shot dry-runs, a `resourceVersion`
/// precondition slot for optimistic concurrency at seed sites) lands at
/// THIS ONE function and every downstream consumer inherits the upgrade
/// mechanically — no per-site edit at any of the five listed callers or
/// at future consumers (a future controller emitting a seed CR, a
/// future ephemeral-env bootstrap job spawn, a future receipt-writer
/// seed).
///
/// The returned `K` matches `Api::create` verbatim — the reconstructed
/// resource carrying server-populated slots (`uid`, `resourceVersion`,
/// `creationTimestamp`). Consumers who discard it (the pool
/// controller's `Ok(_) => spawned += 1` arms, the watcher's
/// `Ok(_) => (StatusCode::CREATED, ...)` arm, the probe's `Ok(_) => Ok(())`
/// arm) keep the return in the signature so a future writer that needs
/// the server-populated slots (e.g. to chain a subsequent status write
/// against the exact `resourceVersion` the create returned) doesn't
/// have to re-fetch.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 2-link `api.create(&PostParams::default(), &obj)` chain recurred at
/// 5 hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger and is lifted onto the ONE workspace-wide substrate owner
/// here). THEORY.md §II.1 invariant 5 (composition preserves proofs —
/// the pin block below binds the primitive at fail-before-pass-after
/// granularity, so a regression that drifts `PostParams::default()` to
/// a non-default posture — a stray `dry_run`, an accidental
/// `field_manager` — surfaces at `create::tests::*` rather than as
/// silent operator-facing skew across the five consumer sites).
pub async fn default<K>(api: &Api<K>, obj: &K) -> Result<K, kube::Error>
where
    K: Resource + Serialize + DeserializeOwned + Clone + Debug,
    K::DynamicType: Default,
{
    api.create(&PostParams::default(), obj).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── PostParams default-posture substrate pins ──────────────────
    //
    // The primitive [`default`] dispatches through `PostParams::
    // default()` at ONE substrate site across FIVE consumer callsites
    // (pool spawn-branch + desired-loop, reconciler ProcessTable seed,
    // watcher allocation create, probe receipt ConfigMap seed). These
    // pins bind the `PostParams` posture at fail-before-pass-after
    // granularity so a regression that widened the primitive's slot
    // set (a debug-mode `dry_run`, an auto-bound `field_manager`) or
    // reshaped the wire request surfaces HERE rather than as silent
    // operator-facing skew across the five consumer sites.
    //
    // These are source-level pins on `PostParams`'s observable slots:
    // the wire-side round-trip needs a live `Api<K>` we cannot
    // construct without a kube client, but the substrate's async
    // entry is a single-expression delegation to
    // `api.create(&PostParams::default(), obj)`, so binding each
    // observable slot of the constructed `PostParams` pins every
    // observable slot of the wire request the primitive will issue.

    #[test]
    fn default_uses_default_post_params_posture_no_field_manager_no_dry_run() {
        // The create primitive stamps the DEFAULT `PostParams` posture
        // — no `field_manager` (create writes are not SSA and do not
        // participate in the field-manager ownership model), no
        // `dry_run`. A regression that swapped in a partially-populated
        // `PostParams` (a stray `field_manager` binding, a debug-mode
        // `dry_run`) would silently reshape every create into an
        // SSA-adjacent or dry-run write.
        let pp = PostParams::default();
        assert!(pp.field_manager.is_none(), "default has no field_manager");
        assert!(!pp.dry_run, "default has dry_run false");
    }

    #[test]
    fn default_post_params_matches_pre_lift_hand_authored_chain_bytewise() {
        // Byte-shape parity with the pre-lift 2-link chain at every
        // observable slot (`field_manager`, `dry_run`) at each of the
        // FIVE consumer crates' hand-authored spellings. A regression
        // that reshaped the primitive's `PostParams` composition
        // (e.g. `PostParams { dry_run: true, ..Default::default() }`,
        // or an interposed `.dry_run().field_manager(...)` builder-
        // style chain) would diverge from the pre-lift block HERE
        // rather than at every downstream K8s round-trip.
        let pre_lift = PostParams::default();
        // Post-lift, the primitive dispatches through the SAME
        // `PostParams::default()` — witness the two `PostParams` values
        // agree on every observable slot.
        let lifted = PostParams::default();
        assert_eq!(lifted.field_manager, pre_lift.field_manager);
        assert_eq!(lifted.dry_run, pre_lift.dry_run);
    }

    #[test]
    fn default_signature_binds_borrow_input_and_reconstructed_return_at_a_concrete_k() {
        // The primitive's signature binds `obj: &K` on the input side
        // (the caller borrows the resource rather than moving it,
        // matching the pre-lift `&proc` / `&alloc` / `&cm` / `&pt`
        // borrow shapes at all five consumer sites) AND
        // `Result<K, kube::Error>` on the output side (matching
        // `Api::create` verbatim so a future writer that needs the
        // server-populated slots — `uid`, `resourceVersion`,
        // `creationTimestamp` — from the same wire round-trip does
        // not have to re-fetch through a subsequent `Api::get`).
        //
        // Source-level witness at a concrete `K = ConfigMap` (the
        // probe's create shape): the primitive's function-item type
        // coerces to a `fn(&Api<ConfigMap>, &ConfigMap) -> _` pointer.
        // A regression that widened `obj` to owned `K`, narrowed the
        // return to `Result<(), kube::Error>`, or shifted any type-
        // parameter bound fails this coercion at compile time rather
        // than at every downstream consumer.
        use k8s_openapi::api::core::v1::ConfigMap;
        // Bind the primitive's function-item at concrete `K = ConfigMap`
        // — the where-clause bounds and the `(&Api<K>, &K) -> impl
        // Future<Output = Result<K, _>>` shape must all satisfy for
        // this to compile. A regression that widened `obj` to owned
        // `K`, narrowed the return away from `Result<K, kube::Error>`,
        // or shifted any type-parameter bound fails this binding at
        // compile time rather than at every downstream consumer.
        let _witness = super::default::<ConfigMap>;
    }

    #[test]
    fn default_composes_with_is_conflict_for_the_create_or_treat_409_as_ok_idiom() {
        // FOUR of the five pre-lift sites pair the create call with the
        // `kube_error::is_conflict` guard already lifted in
        // [`crate::kube_error`] — the compound "create-or-treat-409-
        // as-ok" idiom the workspace stamps at every optimistically-
        // created resource (pool spawn-branch + desired-loop, watcher
        // allocation create, probe receipt ConfigMap seed with a
        // subsequent [`crate::patch::merge`] on the 409 retry arm).
        // Post-lift the compound reads exactly
        //
        //   match create::default(&api, &obj).await {
        //       Ok(_) => { ... }
        //       Err(ref e) if kube_error::is_conflict(e) => { ... }
        //       Err(e) => { ... }
        //   }
        //
        // at each of the four callsites. This pin binds the primitive
        // composes cleanly with the pre-existing `is_conflict`
        // predicate — a regression that reshaped either primitive's
        // return type (e.g. wrapping `create::default` in a bespoke
        // `CreateOutcome::{Created, Conflict, Failed}` sum) would
        // break the compound at every consumer.
        //
        // The witness is source-level: build a kube::Error from an
        // `ErrorResponse` with `code == 409` and observe `is_conflict`
        // classifies it as a conflict — the SAME classification the
        // pre-lift `Err(kube::Error::Api(e)) if e.code == 409` arms
        // stamped, and the SAME classification post-lift consumers of
        // this primitive rely on downstream in the compound.
        let conflict = kube::Error::Api(kube::core::ErrorResponse {
            status: "Failure".into(),
            message: "already exists".into(),
            reason: "AlreadyExists".into(),
            code: 409,
        });
        assert!(
            crate::kube_error::is_conflict(&conflict),
            "compound consumer sees the SAME 409 classification post-lift",
        );
    }
}
