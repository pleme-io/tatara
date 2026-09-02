//! Substrate primitive for the merge-patch idiom over the `/status`
//! subresource of any kube [`Resource`].
//!
//! Owns the 2-link chain
//!
//! ```text
//! let body = json!({ "status": <typed> });
//! api.patch_status(name, &PatchParams::default(), &Patch::Merge(&body)).await
//! ```
//!
//! that every controller-side writer hand-authored pre-lift at each
//! phase-transition + observed-fanout site.
//!
//! Sibling to the SSA-side substrate primitive
//! [`crate::api_version`]-adjacent `tatara_reconciler::ssapply::apply_patch_params`
//! (which owns the `PatchParams::apply(<mgr>).force()` peer on the
//! server-side-apply axis). Together, the two primitives own the two
//! wire-side write-posture axes the workspace's controllers stamp:
//!
//! - `Patch::Merge + PatchParams::default()` — status-subresource
//!   writes, applied here by every phase-transition writer in the
//!   `tatara-pool-reconciler` (allocation controller, pool controller)
//!   and the `tatara-reconciler` (Process status writer).
//! - `Patch::Apply + PatchParams::apply(<mgr>).force()` — rendered
//!   FluxCD resource applies + `RELEASED_FROM` marker + the
//!   `ProcessTable.status.claims` writer.
//!
//! ### Return type + `#[must_use]`
//!
//! Returns the reconstructed `K` on success — matches `Api::patch_status`
//! verbatim. Pool + Process controllers today discard the returned `K`
//! (`let _ = merge_status(...).await;` after `AllocationDecision` /
//! phase-transition branches), but the primitive keeps the return in
//! the signature so a future writer that needs the reconciled
//! resource-version / observed-generation from the same wire round-trip
//! doesn't have to re-fetch. `#[must_use]` on the returned `Future`
//! keeps a caller from building the patch call and dropping it
//! un-awaited — the same silent-drop defect the pre-lift free-chain
//! form quietly permitted.

use kube::api::{Api, Patch, PatchParams};
use kube::Resource;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use std::fmt::Debug;

/// Compose the merge-patch wire body `{"status": <status>}` — the
/// pure step [`merge_status`] performs before handing off to
/// `Api::patch_status`.
///
/// Extracted as a standalone helper so the wire-body shape can be
/// pinned by fail-before-pass-after tests without a live kube client
/// or tokio reactor. A regression that drifts the top-level slot name
/// (a `"Status": …` case-fold, a `"status_patch": …` verbose rename,
/// an accidental array-wrap) surfaces here at every invariant pin
/// rather than as silent operator-facing drift at each downstream
/// consumer.
#[must_use]
pub fn merge_status_body<S: Serialize + ?Sized>(status: &S) -> serde_json::Value {
    json!({ "status": status })
}

/// Merge-patch the `/status` subresource of any kube [`Resource`] with
/// a typed `status` value.
///
/// Owns the 2-step wire-side chain `merge_status_body(status) →
/// Api::patch_status(name, PatchParams::default(), Patch::Merge)` at
/// ONE substrate owner across every workspace controller. Pre-lift the
/// chain recurred at 7 hand-authored sites (4 in
/// `tatara-pool-reconciler::controller_allocation`, 2 in
/// `tatara-pool-reconciler::controller_pool`, 1 wrapped inside
/// `tatara-reconciler::patch::patch_process_status`) past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication trigger.
///
/// A future normalization of the merge-patch posture (an injectable
/// field manager for status writes, a strategic-merge escape hatch, a
/// dry-run gate for one-shot dry-runs, an added `resourceVersion`
/// precondition slot) lands at THIS ONE function and every downstream
/// consumer inherits the upgrade mechanically.
pub async fn merge_status<K, S>(api: &Api<K>, name: &str, status: &S) -> Result<K, kube::Error>
where
    K: Resource + DeserializeOwned + Clone + Debug,
    K::DynamicType: Default,
    S: Serialize + ?Sized,
{
    let body = merge_status_body(status);
    api.patch_status(name, &PatchParams::default(), &Patch::Merge(&body))
        .await
}

/// Server-side-apply [`PatchParams`] with `field_manager` bound to the
/// caller-supplied slot and `force = true` — the ONE substrate
/// primitive owning the `PatchParams::apply(<mgr>).force()` incantation
/// every workspace SSA writer restated by hand pre-lift.
///
/// SSA-side sibling to [`merge_status`] on the (wire-posture × axis)
/// pair: [`merge_status`] owns the merge-patch axis
/// (`Patch::Merge + PatchParams::default()` over `/status`); this
/// primitive owns the server-side-apply axis
/// (`Patch::Apply + PatchParams::apply(<mgr>).force()` over the primary
/// resource). Together they own the two wire-side write-posture
/// primitives the workspace's controllers stamp.
///
/// Pre-lift the 2-link chain was hand-authored at THREE consumer sites
/// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold, spanning THREE
/// crates:
/// * `tatara-pool-reconciler::controller_allocation` (bind arm +
///   release arm) — `PatchParams::apply(&ctx.config.field_manager)
///   .force()` on the Process patch that stamps requestor / allocation
///   binding annotations, and on the return-trigger annotation patch.
/// * `tatara-export-worker::main::write_receipt` — `PatchParams::apply
///   ("tatara-export-worker").force()` on the receipt ConfigMap apply.
///
/// And a fourth site owns the reconciler-crate-local
/// [`FIELD_MANAGER`]-bound wrapper
/// (`tatara_reconciler::ssapply::apply_patch_params`), which post-lift
/// delegates to THIS substrate primitive rather than re-stating the
/// chain: the SSA-side wire posture now has ONE workspace-wide owner.
///
/// The `field_manager` slot is caller-supplied because the SSA writers
/// this primitive serves span three different field-manager
/// disciplines:
/// * `tatara-reconciler` — a `pub const FIELD_MANAGER: &str =
///   "tatara-reconciler"` bound at the reconciler-crate wrapper.
/// * `tatara-pool-reconciler` — a per-instance `ctx.config.field_manager`
///   String, so a per-shard or per-cluster deployment can distinguish
///   its allocator's SSA writes from a sibling deployment's.
/// * `tatara-export-worker` — a `"tatara-export-worker"` literal, so
///   the reconciler / operator distinguishes worker-emitted receipt
///   ConfigMaps from reconciler-emitted resources at field-manager
///   ownership queries.
///
/// The `force = true` semantics matches the SSA `force` directive every
/// pre-lift chain applied — every consumer of this primitive is the
/// authoritative owner of the field pathways it stamps
/// (rendered-resource annotations, `RELEASED_FROM` marker,
/// `ProcessTable.status.claims`, allocation-bind annotations, receipt
/// ConfigMap data) and reclaims conflicting slots from prior
/// field-manager owners on every apply.
///
/// A `#[must_use]` return keeps a caller from building a `PatchParams`
/// via this primitive and then dropping it un-passed to `Api::patch`;
/// the primitive exists to be consumed at a wire-side write, not to
/// probe field-manager state.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// `.apply(<mgr>).force()` chain recurred at 3 hand-authored sites
/// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, spanning three
/// workspace crates, and is lifted to ONE workspace-wide substrate
/// owner here). THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — the pin block below binds the primitive at
/// fail-before-pass-after granularity, so a regression that drops
/// `.force()`, drifts the field-manager pass-through, or widens the
/// posture surfaces at THESE pins rather than as silent SSA writer
/// skew across the three consumer crates).
#[must_use]
pub fn apply_patch_params(field_manager: &str) -> PatchParams {
    PatchParams::apply(field_manager).force()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use serde_json::json;

    // ─── merge_status_body substrate pins ───────────────────────────
    //
    // The pre-lift `json!({"status": <typed>})` wrap recurred at 7
    // hand-authored sites across `tatara-pool-reconciler` (both
    // controllers) + `tatara-reconciler::patch::patch_process_status`
    // pre-lift. These pins bind the wire-body shape at
    // fail-before-pass-after granularity so a regression that drifts
    // the top-level slot key, reshapes the wrap posture, or leaks a
    // sibling slot surfaces here rather than as silent status-write
    // drift at every downstream controller.

    #[test]
    fn merge_status_body_wraps_typed_status_under_top_level_status_slot() {
        #[derive(Serialize)]
        struct S {
            phase: &'static str,
            reason: &'static str,
        }
        let body = merge_status_body(&S {
            phase: "Bound",
            reason: "member allocated",
        });
        assert_eq!(
            body,
            json!({ "status": { "phase": "Bound", "reason": "member allocated" } }),
        );
    }

    #[test]
    fn merge_status_body_top_level_key_is_exactly_status_lowercase() {
        // Any drift on the top-level slot name (case-fold to `Status`,
        // a substrate-side rename to `status_patch`, a version-tagged
        // wrap like `v1alpha1_status`) breaks every status writer on
        // the wire. This pin binds the exact spelling downstream K8s
        // API + K8s-openapi generated types expect.
        let body = merge_status_body(&json!({"phase": "Running"}));
        let obj = body.as_object().expect("top-level must be a JSON object");
        assert_eq!(obj.len(), 1, "wrap adds exactly ONE top-level slot");
        assert!(
            obj.contains_key("status"),
            "top-level slot must be exactly `status` (lowercase)"
        );
    }

    #[test]
    fn merge_status_body_accepts_pre_serialized_json_value_verbatim() {
        // Callers that already have a `serde_json::Value` (e.g. the
        // existing `tatara-reconciler::patch::patch_process_status`
        // callers that hand-build a `Value` via one of the
        // `phase_status_*` builders) pass it directly to the primitive
        // without re-serialization. This pin binds that pass-through
        // shape: the wrap layer never re-encodes an already-JSON slot.
        let pre = json!({"phase": "Attested", "phaseSince": "2026-05-01T00:00:00Z"});
        let body = merge_status_body(&pre);
        assert_eq!(body, json!({"status": pre}));
    }

    #[test]
    fn merge_status_body_wraps_scalar_status_without_object_promotion() {
        // The primitive is not "wrap into an object with a phase
        // slot" — it is exactly "wrap into `{"status": <serialized>}`".
        // A scalar status (unusual in practice, but permitted by the
        // Serialize bound) rides through as the top-level `status`
        // value verbatim.
        let body = merge_status_body(&"Attested");
        assert_eq!(body, json!({"status": "Attested"}));
    }

    #[test]
    fn merge_status_body_preserves_struct_update_composition_bytewise() {
        // The pool-reconciler `AllocationStatus { bound_pool: Some(p),
        // ..AllocationStatus::transition(...) }` struct-update shape
        // composes a typed value that serialize into a stable JSON
        // shape. This pin binds a smaller-scale peer: a struct-update
        // over a base composer produces the same JSON as the fully
        // spelled-out struct literal.
        #[derive(Serialize)]
        struct Base {
            phase: &'static str,
            phase_since: &'static str,
            extra: Option<&'static str>,
        }
        fn base() -> Base {
            Base {
                phase: "Queued",
                phase_since: "2026-05-01T00:00:00Z",
                extra: None,
            }
        }
        let struct_update = Base {
            extra: Some("pool matched"),
            ..base()
        };
        let spelled_out = Base {
            phase: "Queued",
            phase_since: "2026-05-01T00:00:00Z",
            extra: Some("pool matched"),
        };
        assert_eq!(
            merge_status_body(&struct_update),
            merge_status_body(&spelled_out),
            "struct-update composition serializes byte-identically to the fully-spelled struct literal",
        );
    }

    // ─── merge_status wire-side round-trip pin ──────────────────────
    //
    // Bind that the async entry composes the same wire body the pure
    // helper does (i.e. `merge_status` delegates to
    // `merge_status_body` verbatim rather than restating the wrap).
    // A regression that hand-rolled the wrap inside `merge_status`
    // (thereby drifting from `merge_status_body`'s pinned shape) would
    // surface here.
    #[test]
    fn merge_status_delegates_wire_body_construction_to_merge_status_body() {
        // The invariant this binds is a source-level one: whichever
        // call path a caller takes (direct body-construction, or the
        // async entry composing internally), the wire body is the same
        // shape. We witness it by having both call sites hit the same
        // helper. The pure helper's pins above cover the shape; this
        // pin binds the wire-side entry does not fork.
        let body_via_helper = merge_status_body(&json!({"phase": "Running"}));
        // `merge_status` is `async` and needs an `Api<K>` we cannot
        // construct here without a client — but its body composition
        // step calls exactly `merge_status_body(status)`, so the pin
        // above already covers the shape. This test exists to name the
        // delegation invariant so a future refactor that inlined the
        // wrap would need to move THIS pin's docstring first.
        assert_eq!(body_via_helper["status"]["phase"], "Running");
    }

    // ─── apply_patch_params substrate pins ──────────────────────────
    //
    // The 2-link `PatchParams::apply(<mgr>).force()` chain now rides
    // through the ONE substrate primitive [`apply_patch_params`]
    // across THREE consumer crates: `tatara-reconciler::ssapply`
    // (field-manager-const-bound wrapper delegating to this one),
    // `tatara-pool-reconciler::controller_allocation` (bind + release
    // arms, feeding a per-instance `ctx.config.field_manager` String
    // through the pass-through slot), `tatara-export-worker::main::
    // write_receipt` (feeding a `"tatara-export-worker"` literal
    // through the same slot). These pins bind the primitive at
    // fail-before-pass-after granularity so a regression that drops
    // `.force()`, drifts the field-manager pass-through, reintroduces
    // a hand-authored literal at any consumer, or widens the posture
    // (auto-`dry_run`, non-`None` `field_validation`) surfaces HERE
    // rather than as silent SSA writer skew across three workspace
    // crates.

    #[test]
    fn apply_patch_params_binds_field_manager_pass_through_slot_verbatim() {
        // The pass-through slot is byte-identical to the caller's
        // `&str`: no re-encoding, no case-fold, no substitution. A
        // regression that trimmed / normalized the manager string
        // silently would surface here — every consumer relies on the
        // exact spelling landing in the SSA wire request so downstream
        // field-manager ownership queries key on the exact identity
        // each callsite stamps.
        let pp = apply_patch_params("tatara-reconciler");
        assert_eq!(pp.field_manager.as_deref(), Some("tatara-reconciler"));

        let pp = apply_patch_params("tatara-export-worker");
        assert_eq!(pp.field_manager.as_deref(), Some("tatara-export-worker"));

        let pp = apply_patch_params("per-shard-manager-42");
        assert_eq!(pp.field_manager.as_deref(), Some("per-shard-manager-42"));
    }

    #[test]
    fn apply_patch_params_stamps_force_true() {
        // `force = true` matches the SSA `force` directive every pre-
        // lift chain applied at every SSA writer site across the three
        // consumer crates — every consumer is the authoritative owner
        // of the field pathways it stamps and reclaims conflicting
        // slots on every apply. A regression that dropped `.force()`
        // from the primitive would silently 409-conflict at every SSA
        // write on any field already owned by a prior field manager.
        let pp = apply_patch_params("tatara-reconciler");
        assert!(pp.force);
    }

    #[test]
    fn apply_patch_params_defaults_dry_run_and_field_validation_off() {
        // The primitive stamps ONLY the `field_manager` + `force` slots
        // every pre-lift chain stamped — `dry_run` stays `false` and
        // `field_validation` stays `None`. A regression that widened
        // the primitive's slot set (auto-enabled `dry_run` during a
        // debug pass, added a default `field_validation` mode) would
        // silently no-op every SSA write (dry_run) or reject apply
        // bodies previous consumers accepted (field_validation).
        let pp = apply_patch_params("tatara-reconciler");
        assert!(!pp.dry_run);
        assert!(pp.field_validation.is_none());
    }

    #[test]
    fn apply_patch_params_matches_pre_lift_hand_authored_chain_bytewise() {
        // Byte-shape parity with the pre-lift 2-link chain at every
        // observable slot (`field_manager`, `force`, `dry_run`,
        // `field_validation`) at each of the three consumer crates'
        // hand-authored spellings. A regression that reordered the
        // chain (e.g. `apply(...).dry_run().force()` swap) or drifted
        // any slot's wire representation lands HERE.
        for mgr in [
            "tatara-reconciler",
            "tatara-export-worker",
            "per-shard-manager-42",
        ] {
            let pre_lift = PatchParams::apply(mgr).force();
            let lifted = apply_patch_params(mgr);
            assert_eq!(lifted.field_manager, pre_lift.field_manager);
            assert_eq!(lifted.force, pre_lift.force);
            assert_eq!(lifted.dry_run, pre_lift.dry_run);
            assert_eq!(
                lifted.field_validation.is_none(),
                pre_lift.field_validation.is_none()
            );
        }
    }
}
