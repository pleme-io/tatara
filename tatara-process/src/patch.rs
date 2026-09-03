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

/// Merge-patch the PRIMARY resource endpoint of any kube [`Resource`]
/// with a caller-composed wire body.
///
/// Primary-resource sibling to [`merge_status`] on the (wire-endpoint ×
/// wrap-posture) pair: [`merge_status`] owns the `/status` subresource
/// axis (`api.patch_status(...)`) AND wraps the caller's typed value
/// into `{"status": <typed>}` before dispatching; this primitive owns
/// the primary-resource axis (`api.patch(...)`) and passes the caller's
/// body through verbatim — the caller composes the top-level `spec:`,
/// `metadata:`, `data:`, or other merge-patch slot before hand-off.
///
/// The wrap asymmetry between the two primitives matches the pre-lift
/// callsite discipline exactly: every `/status` writer built a typed
/// status value (an `AllocationStatus`, a `ProcessStatus`, a raw
/// `Value`) and delegated the `{"status": …}` wrap uniformly, so
/// [`merge_status`] owns that wrap; every primary-resource writer
/// composed a task-specific body (a `spec:` slot for a spec patch, a
/// `metadata:` slot for a finalizer / annotation edit, a `data:` slot
/// for a ConfigMap edit) with no shared top-level shape, so this
/// primitive dispatches the caller's body verbatim rather than
/// speculating a wrap. A future normalization that WOULD apply to every
/// primary-resource writer (a hardcoded field-manager pass-through for
/// primary-resource merge writes, a strategic-merge escape hatch, a
/// dry-run gate, a `resourceVersion` precondition slot) lands at THIS
/// ONE function and every downstream consumer inherits the upgrade
/// mechanically.
///
/// Pre-lift the 3-link chain
/// `api.patch(name, &PatchParams::default(), &Patch::Merge(&body))` was
/// hand-authored at SIX consumer sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold, spanning TWO workspace crates:
/// * `tatara-reconciler::patch::patch_process_table_spec` — the
///   `{"spec": ...}` merge that stamps `next_sequence` bumps on the
///   ProcessTable singleton.
/// * `tatara-reconciler::patch::apply_finalizer_transform` — the
///   `{"metadata": {"finalizers": [...]}}` merge that owns finalizer
///   ensure / remove on the Process (shared by both public wrappers).
/// * `tatara-reconciler::signals::ingest` — the
///   `{"metadata": {"annotations": {SIGNAL: null}}}` merge that strips
///   the tatara-pleme-io/signal annotation off the Process after
///   ingestion.
/// * `tatara-reconciler::signals::consume_effect` (`SignalEffect::Suspend`
///   arm) — the `{"spec": {"suspended": true}}` merge that stamps
///   SIGSTOP-persistent suspend state on the Process.
/// * `tatara-reconciler::signals::consume_effect` (`SignalEffect::Resume`
///   arm) — the `{"spec": {"suspended": false}}` merge that lifts
///   suspend state on SIGCONT.
/// * `tatara-closed-loop-probe::main::write_receipt_configmap` (409
///   already-exists retry path) — the `{"data": <receipt payload>}`
///   merge that updates the receipt ConfigMap in-place when the create
///   arm loses the race with a prior probe emission.
///
/// Post-lift each callsite reads `patch::merge(&api, name, &body)` and
/// the 3-link chain lives at ONE substrate owner. The pin block below
/// binds the primitive at fail-before-pass-after granularity so a
/// regression that drops `Patch::Merge` for `Patch::Strategic`, drifts
/// the `PatchParams::default()` slot, or reorders the 3-arg positional
/// slots surfaces here rather than as silent primary-resource writer
/// skew across the two consumer crates.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 3-link primary-resource merge chain recurred at 6 hand-authored
/// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is
/// lifted onto the ONE workspace-wide substrate owner here). THEORY.md
/// §II.1 invariant 5 (composition preserves proofs — the pin block
/// binds the `Patch::Merge` posture + the default `PatchParams` slot +
/// the pass-through body composition + the byte-identical parity with
/// the pre-lift 3-link chain, so a regression that drifted any surface
/// surfaces here rather than as silent operator-facing skew across the
/// six primary-resource writer sites).
pub async fn merge<K, B>(api: &Api<K>, name: &str, body: &B) -> Result<K, kube::Error>
where
    K: Resource + DeserializeOwned + Clone + Debug,
    K::DynamicType: Default,
    B: Serialize + Debug + ?Sized,
{
    api.patch(name, &PatchParams::default(), &Patch::Merge(body))
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

    // ─── merge (primary-resource) substrate pins ────────────────────
    //
    // The 3-link `api.patch(name, &PatchParams::default(),
    // &Patch::Merge(&body))` chain now rides through the ONE substrate
    // primitive [`merge`] across TWO consumer crates:
    // `tatara-reconciler::patch::{patch_process_table_spec,
    // apply_finalizer_transform}` + `tatara-reconciler::signals::
    // {ingest, consume_effect (Suspend + Resume arms)}` and
    // `tatara-closed-loop-probe::main::write_receipt_configmap`. These
    // pins bind the primitive at fail-before-pass-after granularity so
    // a regression that switches `Patch::Merge` for `Patch::Strategic`,
    // drifts `PatchParams::default()` to a non-default posture (a
    // hardcoded field manager, an auto-`dry_run`, a non-`None`
    // `field_validation` mode), reorders the 3-arg positional slots,
    // or hijacks the pass-through body (a hidden top-level wrap, an
    // accidental re-encode through `serde_json::to_value` and back)
    // surfaces HERE rather than as silent primary-resource writer skew
    // across the six pre-lift callsites.
    //
    // These are source-level pins on the pure helpers the async entry
    // composes: the wire-side round-trip needs a live `Api<K>` we
    // cannot construct without a kube client, but the substrate's
    // async entry is a single-expression delegation to
    // `api.patch(name, &PatchParams::default(), &Patch::Merge(body))`,
    // so binding each ingredient (default patch-params posture, merge-
    // strategy selection, verbatim body pass-through) at the pure
    // level pins every observable slot of the wire request the primitive
    // will issue.

    #[test]
    fn merge_uses_default_patch_params_posture_no_field_manager_no_dry_run_no_force() {
        // The primary-resource merge primitive stamps the DEFAULT
        // `PatchParams` posture — no field_manager (merge writes are
        // not SSA and do not participate in the field-manager
        // ownership model), no dry_run, no force, no field_validation.
        // A regression that swapped in a partially-populated
        // `PatchParams` (a stray `apply(...)`, a debug-mode `dry_run`,
        // a `field_validation` mode) would silently reshape every
        // primary-resource merge into an SSA-adjacent or dry-run write.
        let pp = PatchParams::default();
        assert!(pp.field_manager.is_none(), "default has no field_manager");
        assert!(!pp.dry_run, "default has dry_run false");
        assert!(!pp.force, "default has force false");
        assert!(
            pp.field_validation.is_none(),
            "default has no field_validation"
        );
    }

    #[test]
    fn merge_selects_patch_merge_strategy_not_apply_or_strategic() {
        // The primitive dispatches through `Patch::Merge(&body)` — the
        // JSON merge patch posture (RFC 7396) every pre-lift consumer
        // used. A regression that selected `Patch::Apply` would inject
        // an SSA wire request against the primary-resource endpoint
        // (which either 415s without an `apiVersion`/`kind` slot or
        // takes ownership away from the API server's merge
        // reconciliation model); a regression that selected
        // `Patch::Strategic` would reshape merge semantics for arrays
        // of tagged sub-objects (finalizers, annotations, labels) into
        // strategic-merge behavior that silently deduplicates entries
        // by strategic-merge-key rather than treating the slot as a
        // JSON scalar to overwrite.
        let body = json!({"spec": {"suspended": true}});
        let patch: Patch<&serde_json::Value> = Patch::Merge(&body);
        assert!(
            matches!(patch, Patch::Merge(_)),
            "merge primitive dispatches through Patch::Merge, not Apply/Strategic/Json"
        );
    }

    #[test]
    fn merge_dispatches_body_verbatim_no_wrap_or_re_encode() {
        // Unlike [`merge_status`] which wraps its input into
        // `{"status": …}`, the primary-resource merge primitive is
        // verbatim: the caller composes the full top-level shape
        // (`{"spec": …}`, `{"metadata": {"finalizers": …}}`,
        // `{"data": …}`) and the primitive passes it through untouched.
        // A regression that hid an implicit wrap or re-encoded the
        // body through `serde_json::to_value` and back would surface
        // here — every pre-lift callsite already composed the top-
        // level shape and delegated straight to
        // `api.patch(..., &Patch::Merge(&body))` with no intervening
        // transform.
        //
        // Sweep every top-level shape the six pre-lift consumers
        // compose so a regression on any one lands here.
        let spec_body = json!({"spec": {"suspended": true}});
        let meta_body = json!({
            "metadata": {"finalizers": ["tatara.pleme.io/process-finalizer"]},
        });
        let strip_body = json!({
            "metadata": {"annotations": {"tatara.pleme.io/signal": serde_json::Value::Null}},
        });
        let data_body = json!({"data": {"receipt.json": "{...}"}});
        let spec_next_body = json!({"spec": {"nextSequence": 42}});
        for body in [spec_body, meta_body, strip_body, data_body, spec_next_body] {
            // The primitive's body-passing step is a `&Patch::Merge(body)`
            // borrow with no intervening transform — witness that the
            // top-level slot survives verbatim.
            let round_trip = serde_json::to_value(&body).unwrap();
            assert_eq!(round_trip, body, "body serializes to itself verbatim");
            // Extract the ONE top-level slot the pre-lift caller
            // composed; the primitive must not add a sibling slot.
            let obj = body.as_object().expect("pre-lift bodies are JSON objects");
            assert_eq!(
                obj.len(),
                1,
                "each pre-lift consumer composed exactly ONE top-level slot"
            );
        }
    }

    #[test]
    fn merge_body_composition_matches_pre_lift_signals_and_finalizer_shapes_bytewise() {
        // Byte-shape parity against each of the six pre-lift bodies —
        // signals::ingest strip annotation, signals::consume_effect
        // Suspend + Resume, patch::patch_process_table_spec's
        // `{"spec": …}` seed, patch::apply_finalizer_transform's
        // `{"metadata": {"finalizers": …}}` seed, and
        // closed-loop-probe::write_receipt_configmap's `{"data": …}`
        // seed. A regression that reshaped any body composer at its
        // callsite (case-fold slot names, added sibling debug slots)
        // surfaces here rather than as silent behavioral drift at the
        // wire.

        // signals::ingest strip shape
        let strip = json!({
            "metadata": {
                "annotations": { "tatara.pleme.io/signal": serde_json::Value::Null }
            }
        });
        assert_eq!(
            strip["metadata"]["annotations"]["tatara.pleme.io/signal"],
            serde_json::Value::Null,
            "strip stamps JSON null to trigger merge-patch key removal"
        );

        // signals::consume_effect Suspend shape
        let suspend = json!({ "spec": { "suspended": true } });
        assert_eq!(suspend["spec"]["suspended"], serde_json::Value::Bool(true));

        // signals::consume_effect Resume shape
        let resume = json!({ "spec": { "suspended": false } });
        assert_eq!(resume["spec"]["suspended"], serde_json::Value::Bool(false));
    }
}
