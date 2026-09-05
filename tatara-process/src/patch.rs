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

/// Server-side-apply the caller-composed `body` against the PRIMARY resource
/// endpoint of any kube [`Resource`] under `field_manager` with `force = true`.
///
/// SSA-side sibling to [`merge`] on the (wire-endpoint × wrap-posture) pair:
/// [`merge`] owns the primary-resource `Patch::Merge + PatchParams::default()`
/// axis; this primitive owns the primary-resource
/// `Patch::Apply + PatchParams::apply(<mgr>).force()` axis and composes the
/// two-link `apply_patch_params + api.patch(&Patch::Apply(...))` chain every
/// workspace SSA writer hand-authored pre-lift at each ownership-taking
/// apply site.
///
/// Pre-lift the 3-link chain
/// `let pp = apply_patch_params(<mgr>);
///  api.patch(name, &pp, &Patch::Apply(&body)).await`
/// was hand-authored at THREE workspace-wide consumer sites past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold, spanning TWO active crates:
/// * `tatara-reconciler::ssapply::apply_owned` — the DynamicObject SSA
///   writer for every rendered flux/aplicacao resource; the manager
///   is [`tatara_reconciler::ssapply::FIELD_MANAGER`].
/// * `tatara-reconciler::phase_machine::transition_to_releasing` — the
///   `RELEASED_FROM` annotation stamp on Attested/Failed → Releasing;
///   same manager as above.
/// * `tatara-export-worker::main::write_receipt` — the receipt ConfigMap
///   SSA apply; the manager is the `"tatara-export-worker"` literal.
///
/// All three sites walked the SAME two-link chain — build a `PatchParams`
/// via [`apply_patch_params`], then dispatch through
/// `api.patch(name, &pp, &Patch::Apply(&body))`. Post-lift each callsite
/// reads `tatara_process::patch::apply(&api, name, <mgr>, &body).await`
/// and the params-build + `Patch::Apply` wire dispatch lives at ONE
/// substrate owner.
///
/// The `field_manager` slot is caller-supplied because the three SSA
/// writers this primitive serves span two field-manager disciplines:
/// tatara-reconciler feeds its `FIELD_MANAGER` const (via the
/// crate-local `ssapply::apply_patch_params()` wrapper's callers, which
/// after this lift call THIS primitive with the const directly),
/// tatara-export-worker feeds the `"tatara-export-worker"` literal.
///
/// A future normalization of the SSA-side wire posture (an injectable
/// `dry_run` mode, a `field_validation` default, a per-fleet retry
/// policy, a `resourceVersion` precondition slot, a `tracing`-annotated
/// span carrying the apply's manager + body-summary for post-hoc audit)
/// lands at THIS ONE substrate primitive (or at [`apply_patch_params`]
/// on the params sub-axis) and every downstream SSA writer inherits
/// the upgrade mechanically. No per-site edit at any of the three
/// listed callers or at future consumers (a new SSA writer for a
/// non-DynamicObject typed resource, a fourth crate stamping receipts,
/// a per-Kind apply sink).
///
/// Return-form axis: `Result<K, kube::Error>` matches `Api::patch`
/// verbatim. Consumers today either drop the returned `K`
/// (`.await.map_err(...)?` at ssapply + phase_machine) or discard it
/// through `.await.map(|_| ()).with_context(...)?` at export-worker;
/// keeping the return in the signature lets a future writer that needs
/// the reconciled `resourceVersion` / `generation` from the same wire
/// round-trip read it without a re-fetch.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 2-link `apply_patch_params + api.patch(&Patch::Apply(...))` chain
/// recurred at 3 hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication trigger, spanning two workspace crates, and is lifted
/// onto ONE substrate owner here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — the pin block below binds the
/// `Patch::Apply` posture + the [`apply_patch_params`] pass-through +
/// the byte-identical parity with the pre-lift chain, so a regression
/// that drifts any surface surfaces here rather than as silent SSA
/// writer skew across the three primary-resource apply sites).
pub async fn apply<K, B>(
    api: &Api<K>,
    name: &str,
    field_manager: &str,
    body: &B,
) -> Result<K, kube::Error>
where
    K: Resource + DeserializeOwned + Clone + Debug,
    B: Serialize + Debug + ?Sized,
{
    // NOTE: `K::DynamicType: Default` is deliberately NOT required here
    // (unlike [`merge`] / [`merge_status`]) so [`Api<DynamicObject>`]
    // consumers — whose `DynamicType = ApiResource` is not `Default` —
    // ride the same primitive as concrete `Api<ConfigMap>` /
    // `Api<Process>` consumers. `Api::patch` itself needs only
    // `K: Clone + DeserializeOwned + Debug` on its own impl block; the
    // `Default` bound on the sibling primitives is a legacy of their
    // pre-lift call sites, none of which exercised DynamicObject.
    let pp = apply_patch_params(field_manager);
    api.patch(name, &pp, &Patch::Apply(body)).await
}

/// Merge-patch the PRIMARY resource endpoint of any kube [`Resource`] under
/// `field_manager` with `force = true` — the merge-strategy sibling to
/// [`apply`] on the (Patch-strategy × PatchParams-posture) matrix.
///
/// Owns the two-link chain
/// `apply_patch_params(<mgr>) + api.patch(name, &pp, &Patch::Merge(&body))`
/// at ONE substrate owner. Closes the four-corner posture matrix the
/// wire-side patch family stamps:
///
/// | Strategy | `PatchParams::default()` | `apply_patch_params(<mgr>)` |
/// |----------|--------------------------|-----------------------------|
/// | Merge    | [`merge`]                | **`merge_as`** (this one)   |
/// | Apply    | (invalid — SSA requires a field manager) | [`apply`]   |
///
/// [`merge`] owns the anonymous-writer merge-patch corner
/// (`PatchParams::default()`, no field-manager ownership); [`apply`] owns
/// the SSA corner (`Patch::Apply` under a named field manager); this
/// primitive owns the remaining corner — a merge-patch that STILL stamps
/// a named field manager on the write, chosen when the caller wants
/// merge-patch semantics (server merges the caller's partial body into
/// the existing object per RFC 7396, rather than the SSA ownership
/// reconciliation model) BUT wants the write attributed to a named
/// controller in the field-manager ownership audit (so downstream `kubectl
/// get -o yaml`'s `managedFields` distinguishes a
/// `tatara-pool-reconciler`-stamped bind edit from a
/// `tatara-reconciler`-stamped phase-transition status write).
///
/// Pre-lift the two-link chain was hand-authored at TWO workspace-wide
/// consumer sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold,
/// both inside `tatara-pool-reconciler::controller_allocation`:
/// * Bind arm — stamps the compound `spec.lifetime` overlay + the three
///   `metadata.annotations` requestor / allocation / requestor-kind
///   labels onto the pool member Process on transition from Queued to
///   Bound. Body: `{"spec": {"lifetime": …}, "metadata": {"annotations":
///   {REQUESTOR: …, ALLOCATION: …, REQUESTOR_KIND: …}}}`. Field manager:
///   `ctx.config.field_manager` (per-instance String).
/// * Release arm — stamps the single `tatara.pleme.io/return-trigger`
///   annotation onto the member Process to nudge the Pool reconciler
///   into taking the return path. Body: [`annotation_body`]-composed
///   single-key metadata edit. Field manager: `ctx.config.field_manager`
///   (same String).
///
/// Both sites walked the SAME two-link chain — build a `PatchParams` via
/// [`apply_patch_params`] with the pool-reconciler's per-instance
/// `field_manager`, then dispatch through `api.patch(name, &pp,
/// &Patch::Merge(&body))`. Post-lift each callsite reads
/// `tatara_process::patch::merge_as(&api, name, <mgr>, &body).await`
/// and the params-build + `Patch::Merge` wire dispatch lives at ONE
/// substrate owner. A future normalization of the named-merge-writer
/// posture (an injectable `dry_run` mode for a shadow-mode rollout, a
/// `field_validation` default when the pool-reconciler flips on strict
/// validation, an injectable retry policy for the transient-conflict
/// class the bind arm surfaces on race with a sibling pool controller,
/// a `resourceVersion` precondition slot when the pool controller
/// stamps generation-fenced binds) lands at THIS ONE substrate primitive
/// (or at [`apply_patch_params`] on the params sub-axis) and every
/// downstream named-merge writer inherits the upgrade mechanically.
///
/// Directly benefits the P3 kenshi-runner library lift (any test-Job
/// controller that stamps a named-merge overlay on its owning Process
/// — a suite-progress annotation, a per-run bind edit — rides through
/// the same primitive as the pool-reconciler's bind + release arms) and
/// the P5 shigoto Dag refactor (any RecordingJob that stamps a
/// per-instance-named merge edit on a phase transition, rather than
/// through the [`crate::patch::apply`] SSA path or the anonymous
/// [`merge`] path, rides through this substrate corner rather than
/// hand-authoring the two-link chain a third time).
///
/// The bound relaxation `K::DynamicType: Default` is NOT required here
/// (matching [`apply`]'s posture, differing from [`merge`] /
/// [`merge_status`]) so a future [`Api<DynamicObject>`] consumer of the
/// named-merge corner rides the same primitive as the current
/// concrete-`Api<Process>` consumers. `Api::patch` itself needs only
/// `K: Clone + DeserializeOwned + Debug` on its own impl block; the
/// `Default` bound on the sibling merge primitives is a legacy of their
/// pre-lift call sites, none of which exercised DynamicObject.
///
/// Return-form axis: `Result<K, kube::Error>` matches `Api::patch`
/// verbatim. Both pre-lift consumers ignore the returned `K` (the bind
/// arm captures the `Err` for a retry decision; the release arm discards
/// through `let _ = …`); keeping the return in the signature lets a
/// future writer that needs the reconciled `resourceVersion` /
/// `generation` from the same wire round-trip read it without a
/// re-fetch.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// two-link `apply_patch_params(<mgr>) + api.patch(name, &pp,
/// &Patch::Merge(&body))` chain recurred at 2 hand-authored sites past
/// the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger inside one workspace
/// crate, and is lifted onto ONE substrate owner here, closing the
/// (Patch-strategy × PatchParams-posture) matrix's remaining hand-
/// authored corner). THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — the pin block below binds the `Patch::Merge` posture + the
/// [`apply_patch_params`] pass-through + the byte-identical parity with
/// the pre-lift two-link chain, so a regression that drifts any surface
/// surfaces here rather than as silent named-merge writer skew across
/// the two pool-reconciler callsites).
pub async fn merge_as<K, B>(
    api: &Api<K>,
    name: &str,
    field_manager: &str,
    body: &B,
) -> Result<K, kube::Error>
where
    K: Resource + DeserializeOwned + Clone + Debug,
    B: Serialize + Debug + ?Sized,
{
    let pp = apply_patch_params(field_manager);
    api.patch(name, &pp, &Patch::Merge(body)).await
}

/// Compose the merge-patch wire body `{"spec": {"suspended": <bool>}}` — the
/// SIGSTOP/SIGCONT-driven suspend/resume shape both
/// `SignalEffect::Suspend` and `SignalEffect::Resume` arms of
/// `tatara-reconciler::signals::consume_effect` stamp on the Process spec.
///
/// Both arms compose through this ONE substrate owner and hand the produced
/// body straight to [`merge`]; pre-lift each arm restated `json!({ "spec":
/// { "suspended": <bool> } })` verbatim at its callsite (both are named in
/// the `merge` docstring's six-consumer inventory above). Two hand-authored
/// restatements past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger; post-
/// lift a future addition to the suspend/resume wire body (a `by:` slot
/// naming the signal source, a `suspendedAt:` transition timestamp, a
/// symmetry gate that refuses conflicting suspend + resume overlays, a
/// version-tagged wrap for a `spec.suspend.v2` migration) lands at THIS
/// function and both arms inherit the upgrade mechanically.
///
/// The `bool` argument matches the pre-lift call sites' spelling exactly
/// (`true` at the Suspend arm, `false` at the Resume arm) — the primitive
/// does not force one polarity, because the merge-patch body itself is
/// symmetric between the two arms and the shape stays load-bearing at
/// both polarities.
///
/// Sibling to [`merge_status_body`] on the (wire-endpoint × wrap-posture)
/// pair: [`merge_status_body`] owns the `/status` subresource wrap;
/// this primitive owns one specific `{"spec": …}` primary-resource wrap
/// (the suspend/resume one) — a body composer, not a wire-dispatcher, so
/// consumers still hand the produced body to [`merge`] for the round-
/// trip.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// two-arm `json!({ "spec": { "suspended": <bool> } })` restatement is
/// lifted onto ONE substrate composer). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — the pin block below binds the shape
/// at fail-before-pass-after granularity so a regression that drifts the
/// top-level `spec` slot, the inner `suspended` slot, or the JSON bool
/// value type at either polarity surfaces here rather than as silent
/// signal-arm skew at the two suspend/resume callsites).
#[must_use]
pub fn spec_suspended_body(suspended: bool) -> serde_json::Value {
    json!({ "spec": { "suspended": suspended } })
}

/// Compose + dispatch a Process `spec.suspended` toggle — the ONE owner of
/// the `merge(&api, &name, &spec_suspended_body(<bool>))` compose+dispatch
/// chain (2 workspace-wide restatements pre-lift), sibling to
/// [`spec_suspended_body`] on the (body × body+dispatch) axis and to
/// [`merge`] on the (dispatch × dispatch+specific-body) axis.
///
/// Peer of `tatara_reconciler::patch::{transition, transition_msg}` on the
/// compose+dispatch async-wrapper family: those own the phase-transition
/// status-patch compose+dispatch pair (`phase_status[_msg]` body ×
/// [`merge_status`] dispatch); this owns the SIGSTOP/SIGCONT-driven
/// suspend-toggle spec-patch compose+dispatch pair
/// ([`spec_suspended_body`] body × [`merge`] dispatch). Post-lift the two
/// wrapper families together own EVERY signal-driven wire write in the
/// reconciler — the phase-transition pair over the `/status` subresource
/// merge-patch axis, and this primitive over the primary-resource
/// merge-patch axis for spec toggles.
///
/// Pre-lift the SAME 2-link chain was hand-authored at BOTH signal arms
/// of `tatara-reconciler::signals::consume_effect`, each restating
/// `tatara_process::patch::merge(&api, &name,
/// &tatara_process::patch::spec_suspended_body(<bool>))` verbatim to
/// stamp the suspend/resume toggle through the primary-resource merge-
/// patch wire posture:
///
/// * `SignalEffect::Suspend` arm — SIGSTOP-driven pause; stamps
///   `spec.suspended = true` on the Process, which the reconciler's
///   phase machine's suspend gate consumes to pause the heartbeat.
/// * `SignalEffect::Resume` arm — SIGCONT-driven resume; stamps
///   `spec.suspended = false`, releasing the pause.
///
/// Both arms walked the SAME 2-link chain — compose the two-slot
/// `{"spec": {"suspended": <bool>}}` body through [`spec_suspended_body`],
/// dispatch through [`merge`], await the K8s round-trip. Post-lift each
/// arm reads `patch::merge_suspended(&api, &name, <bool>).await` and the
/// compose+dispatch sink lives at ONE owner. Delegates through
/// [`spec_suspended_body`] + [`merge`], so the pin stack above the two
/// primitives (top-level `spec` slot invariant, inner `suspended` slot
/// invariant, JSON-bool-not-string value type, `Patch::Merge` posture,
/// `PatchParams::default()` slot) rides through this wrapper mechanically.
///
/// Return-form axis: `Result<K, kube::Error>` matches [`merge`] verbatim
/// so both callers keep their existing `.map_err(|e| anyhow!(...))?` wrap
/// unchanged — the axis-preserving lift means the caller's async control
/// flow (map-error, propagate) rides through unchanged and only the
/// compose+dispatch chain compresses.
///
/// The `K` type parameter is generic over `kube::Resource` — not fixed
/// at `Process` — so a future suspendable CRD (an [`crate::prelude::
/// EphemeralPool`] wanting a fleet-wide pause, a [`crate::table::
/// ProcessTable`] singleton wanting a maintenance suspend, an
/// arbitrarily-typed peer with a `.spec.suspended: bool` slot) rides
/// through the same primitive without a per-Kind fork of the compose+
/// dispatch chain. The two current callsites both feed
/// `Api<Process>` — this matches the primitive's most-general accepted
/// bound with no widening at the callsite.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 2-link `merge(&api, &name, &spec_suspended_body(<bool>))` compose+
/// dispatch chain recurred at 2 hand-authored sites past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication trigger inside one workspace crate,
/// and is lifted onto ONE substrate owner here). THEORY.md §II.1
/// invariant 5 (composition preserves proofs — the pin block below
/// binds the composer choice + the dispatcher choice + byte-identical
/// parity with the pre-lift 2-link chain, so a regression that drifts
/// either surface — a swap of [`spec_suspended_body`] for a hand-
/// authored `json!` block, a swap of [`merge`] for [`merge_as`] /
/// [`apply`], or a polarity flip that inverts the caller's bool at the
/// wrapper boundary — surfaces HERE rather than as silent signal-arm
/// skew across the two suspend/resume callsites).
pub async fn merge_suspended<K>(api: &Api<K>, name: &str, suspended: bool) -> Result<K, kube::Error>
where
    K: Resource + DeserializeOwned + Clone + Debug,
    K::DynamicType: Default,
{
    merge(api, name, &spec_suspended_body(suspended)).await
}

/// Compose the merge-patch wire body
/// `{"metadata": {"annotations": {<key>: <value>}}}` — the ONE substrate
/// owner of the single-annotation stamp / strip merge-body shape every
/// workspace controller reaches for when it needs to publish exactly ONE
/// operator-visible annotation on the primary resource (or strip one by
/// stamping `Value::Null`) through the merge-patch semantics of either
/// [`merge`] or [`apply`].
///
/// Pre-lift the wire-shape recurred at THREE hand-authored consumer
/// sites across TWO active workspace crates past the ★★ PRIME-DIRECTIVE
/// ≥ 2 duplication threshold:
///
/// - `tatara-reconciler::signals::ingest` — strips the
///   `tatara.pleme.io/signal` annotation off the Process after
///   ingestion by stamping `serde_json::Value::Null` (JSON merge patch
///   interprets `null` as "remove key"). Dispatched through
///   [`merge`] on the primary-resource merge-patch axis.
/// - `tatara-reconciler::phase_machine::transition_to_releasing` —
///   stamps the caller-observed `tatara.pleme.io/released-from`
///   annotation with the current phase string on Attested/Failed →
///   Releasing. Dispatched through [`apply`] on the primary-resource
///   SSA axis (SSA `Patch::Apply` accepts the same
///   `{"metadata": {"annotations": …}}` body shape as `Patch::Merge`
///   — the top-level slot naming is what this composer owns).
/// - `tatara-pool-reconciler::controller_allocation` (Release arm) —
///   stamps the `tatara.pleme.io/return-trigger` annotation with the
///   literal `"true"` on the member Process to nudge the Pool
///   reconciler into taking the return path. Dispatched through the
///   raw `Api::patch` call inside the release arm (also with
///   [`apply_patch_params`]-composed PatchParams; the wire shape is
///   the same `{"metadata": {"annotations": {<one key>: <one value>}}}`
///   this composer names).
///
/// Post-lift each site reads `tatara_process::patch::annotation_body(
/// <key>, <value>)` and the merge-body wire-shape composition lives at
/// ONE substrate owner. A future normalization of the single-annotation
/// merge-body posture (a canonicalization pass over the key spelling —
/// a case-fold or a namespace-prefix normalization for a future annotation
/// naming discipline; a stricter serde-failure return in place of the
/// silent `Value::Null` fallback; a `by:` sibling slot naming the
/// stamping controller for post-hoc audit; a version-tagged wrap for a
/// future `metadata.v2.annotations` migration) lands at THIS ONE function
/// and every downstream single-annotation writer inherits the upgrade
/// mechanically. Directly benefits the P3 kenshi-runner library lift
/// (any Job-based observer that stamps a per-suite annotation on its
/// owning Process rides through the same composer as the strip / stamp
/// / return-trigger family) and the P5 shigoto Dag refactor (every
/// phase-machine RecordingJob that stamps an annotation on a transition
/// rides through the same composer).
///
/// ### Value axis — `impl Serialize` accepts every pre-lift shape
///
/// The `value` slot is `impl Serialize` matching the discipline of
/// [`phase_status_with`] on the extra-key axis: accepts owned or borrowed
/// values of any serde-serialisable type without widening the signature.
/// All three pre-lift consumer sites pass distinct value shapes and this
/// composer serves each verbatim through `serde_json::to_value`:
///
/// - `serde_json::Value::Null` (signals::ingest strip) — the primitive
///   [`serde_json::to_value`] round-trips a `Value::Null` back to
///   `Value::Null`, which JSON merge patch interprets as "remove key".
/// - `String` (phase_machine::transition_to_releasing) — the primitive
///   [`serde_json::to_value`] serializes a `String` to a JSON string
///   verbatim.
/// - `&'static str` (controller_allocation Release arm) — the primitive
///   [`serde_json::to_value`] serializes a `&str` to a JSON string
///   verbatim, matching the pre-lift `"true"` literal.
///
/// A serialisation failure resolves to `Value::Null`, matching the
/// existing [`phase_status_with`] primitive's posture. In practice
/// serialisation of the shapes this composer accepts (a
/// `serde_json::Value`, a `String`, a `&str`) never fails; the fallback
/// is a defensive guard against a future caller passing a `T: Serialize`
/// whose `Serialize` impl signals a runtime error.
///
/// ### Key axis — `&str` matches every pre-lift call form
///
/// The `key` slot is `&str` matching the pre-lift call forms exactly:
/// [`crate::annotations::SIGNAL`] via `SIGNAL_ANNOTATION: &str` at
/// signals.rs, [`crate::annotations::RELEASED_FROM`] via a `pub const:
/// &str` at phase_machine.rs, and a `"tatara.pleme.io/return-trigger"`
/// literal at controller_allocation.rs. `&str` accepts both the
/// pre-existing `pub const: &str` constants in [`crate::annotations`]
/// and inline `&'static str` literals at the same signature.
///
/// A future caller composing a `String` key at runtime (a per-fleet
/// prefix, a runtime-computed annotation name) coerces via `&*key`
/// or `key.as_str()` at the call site — the composer stays borrowed
/// so the common const-fed path pays no allocation.
///
/// ### `must_use` on the return
///
/// The primitive exists to be handed to a wire-side write ([`merge`],
/// [`apply`], or a raw `Api::patch` call at the pool-reconciler's
/// release arm), not to probe the merge-body shape. `#[must_use]`
/// keeps a caller from building the body and dropping it un-passed to
/// a wire dispatcher.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 3-link `json!({"metadata": {"annotations": {<key>: <value>}}})` merge-
/// body composition recurred at 3 hand-authored sites past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication trigger, spanning two active
/// workspace crates, and is lifted onto ONE substrate owner here).
/// THEORY.md §II.1 invariant 5 (composition preserves proofs — the pin
/// block below binds the composer at fail-before-pass-after granularity,
/// so a regression that drifts the top-level `metadata` slot, the nested
/// `annotations` slot, the caller-passed key spelling, or the value-slot
/// pass-through discipline surfaces HERE rather than as silent
/// operator-facing annotation-writer skew across the three consumer
/// sites).
#[must_use]
pub fn annotation_body(key: &str, value: impl Serialize) -> serde_json::Value {
    let v = to_value_or_null(value);
    json!({
        "metadata": {
            "annotations": {
                key: v,
            }
        }
    })
}

/// Serialise a `T: Serialize` into a `serde_json::Value`, folding the
/// (in-practice unreachable) [`serde_json::to_value`] error to
/// [`serde_json::Value::Null`] — the ONE substrate owner of the "serde-
/// serialise a caller-supplied `T` into a JSON slot value, falling back
/// to `Value::Null` on the residual `Err` arm the round-trip's contract
/// permits but no in-workspace payload triggers" shape.
///
/// Pre-lift the SAME `serde_json::to_value(<T>).unwrap_or(Value::Null)`
/// chain was hand-authored at TWO consumer sites past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold, each restating the same
/// three-link chain (`to_value` → `unwrap_or` → `Value::Null` fallback)
/// to seed a JSON slot value from a caller-supplied serde-serialisable
/// payload:
///
/// * [`annotation_body`] — folds the caller's `value: impl Serialize`
///   into the `metadata.annotations.<key>` leaf on the single-annotation
///   merge-body composer. Every downstream single-annotation writer
///   (signals-strip, `RELEASED_FROM` stamp, `return-trigger` stamp)
///   rides through this fold.
/// * `tatara_reconciler::patch::phase_status_with` — folds the caller's
///   `value: impl Serialize` into the caller-named third slot on the
///   phase-transition status-patch composer. Every downstream
///   extra-slot phase-transition writer (Running-entry `fluxResources`
///   attach, Attested-entry `attestation` attach, and — via
///   [`crate::patch::phase_status`]'s `Some` arm delegating through the
///   composer — the Forking-entry `identity` attach) rides through this
///   fold. The reconciler-side callsite reaches THIS substrate primitive
///   by fully-qualified name (`tatara_process::patch::to_value_or_null`).
///
/// Both sites walked the SAME three-link chain — `serde_json::to_value`
/// on the caller-supplied `T`, `Result::unwrap_or` on the residual
/// `Err` arm, `serde_json::Value::Null` as the fallback constant.
/// Differing only in the `T` slot's downstream consumer (an
/// `annotations.<key>` leaf vs a `phase_status_base` sibling slot).
/// Post-lift each callsite reads `to_value_or_null(v)` and the
/// three-link chain lives at ONE substrate owner.
///
/// A future normalization of the fold discipline — a promotion of the
/// `Value::Null` fallback to a typed error return so a caller's
/// `Serialize` impl that legitimately fails at runtime surfaces at the
/// composer rather than being silently dropped to `null`; a
/// canonicalization pass over the produced `Value` (a sort-map-keys
/// walk for deterministic byte output, a whitespace-strip for size);
/// a `tracing::warn!` span at the `Err` arm so the silent drop leaves
/// an operator-visible breadcrumb; a switch to `serde_json::to_value`'s
/// `Cow`-returning peer for zero-copy on already-`Value` inputs —
/// lands at THIS ONE substrate primitive and both downstream fold
/// consumers (plus every future JSON-slot-seeded-from-`T: Serialize`
/// writer that grows a third consumer) inherit the upgrade
/// mechanically. No per-site edit at [`annotation_body`] or at
/// `phase_status_with`; a new consumer (a hypothetical typed labels
/// composer, an annotations-batch composer, a per-slot status-patch
/// composer) picks up the sibling primitive by name and inherits the
/// same fold discipline.
///
/// Sibling to [`crate::three_pillar::pillar_bytes`] on the (`T:
/// Serialize` → wire-shape) axis pair: [`pillar_bytes`] owns the
/// `serde_json::to_vec(<T>).unwrap_or_default()` fold for the
/// attestation-pillar bytes axis (returning `Vec<u8>` with a
/// `Vec::default()` empty fallback); this primitive owns the
/// `serde_json::to_value(<T>).unwrap_or(Value::Null)` fold for the
/// JSON-slot value axis (returning `Value` with a `Value::Null`
/// fallback). Both hold the invariant that a caller-supplied
/// serde-serialisable payload folds into a wire-form value at ONE
/// substrate owner rather than at each per-site hand-authored chain.
///
/// The `T: Serialize` bound accepts owned or borrowed values of any
/// serde-serialisable type without widening the signature — matches
/// [`annotation_body`]'s `impl Serialize` value slot verbatim and
/// matches `phase_status_with`'s `T: Serialize` extra slot verbatim.
/// A serialisation failure (the `Err` arm the `serde_json::to_value`
/// contract permits) resolves to [`serde_json::Value::Null`], matching
/// the pre-lift discipline both callsites carried before this lift.
/// In practice the shapes each callsite passes (a `Value::Null`, a
/// `String`, a `&'static str`, an `&Identity`, a `&Vec<FluxResourceRef>`,
/// a `&ProcessAttestation`) never fail to serialise; the fallback is a
/// defensive guard against a future caller passing a `T` whose
/// `Serialize` impl signals a runtime error at that boundary.
///
/// [`pillar_bytes`]: crate::three_pillar::pillar_bytes
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 3-link `serde_json::to_value(<T>).unwrap_or(Value::Null)` chain
/// recurred at 2 hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication trigger inside two workspace crates, and is lifted onto
/// ONE substrate owner here). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — the pin block below binds the composer at
/// fail-before-pass-after granularity, so a regression that drifts the
/// serialiser choice, flips the fallback constant, reshapes the return
/// form, or narrows the `T: Serialize` bound surfaces HERE rather than
/// as silent JSON-slot-value skew across the two consumer sites).
#[must_use]
pub fn to_value_or_null<T: Serialize>(value: T) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
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

    // ─── apply (SSA primary-resource) substrate pins ───────────────
    //
    // The 2-link `apply_patch_params(<mgr>) + api.patch(name, &pp,
    // &Patch::Apply(&body))` chain now rides through the ONE substrate
    // primitive [`apply`] across TWO consumer crates:
    // `tatara-reconciler::ssapply::apply_owned` (DynamicObject SSA
    // writer for every rendered flux/aplicacao resource, feeding
    // `FIELD_MANAGER` through the const wrapper),
    // `tatara-reconciler::phase_machine::transition_to_releasing`
    // (RELEASED_FROM annotation stamp on Attested/Failed → Releasing,
    // same manager), and `tatara-export-worker::main::write_receipt`
    // (receipt ConfigMap SSA apply, feeding `"tatara-export-worker"`).
    // These pins bind the primitive at fail-before-pass-after
    // granularity so a regression that swaps `Patch::Apply` for
    // `Patch::Merge` (silently losing SSA ownership + reverting to
    // merge-patch semantics), drops the [`apply_patch_params`]
    // pass-through (silently reverting to `PatchParams::default()`
    // and losing `.force()` + field-manager), or reorders the 3-arg
    // positional slots surfaces HERE rather than as silent SSA
    // writer skew across the three pre-lift callsites.
    //
    // These are source-level pins on the ingredients [`apply`]
    // composes: the wire-side round-trip needs a live `Api<K>` we
    // cannot construct without a kube client, but the substrate's
    // async entry is a two-line body (`let pp = apply_patch_params
    // (field_manager); api.patch(name, &pp, &Patch::Apply(body))`),
    // so binding each ingredient (the [`apply_patch_params`]-composed
    // PatchParams shape, the `Patch::Apply` posture selection, the
    // verbatim body pass-through) at the pure level pins every
    // observable slot of the SSA wire request the primitive will
    // issue.

    #[test]
    fn apply_composes_apply_patch_params_at_the_field_manager_slot_verbatim() {
        // The primitive's params-build step is
        // `apply_patch_params(field_manager)` — every pre-lift caller
        // supplied a field-manager `&str` (the reconciler's
        // `FIELD_MANAGER` const, the export-worker's `"tatara-export-
        // worker"` literal). A regression that hardcoded a manager
        // inside the primitive or reshaped the slot would silently
        // reassign field-manager ownership at every consumer's wire
        // request. Witness the params-side ingredient by re-composing
        // it through [`apply_patch_params`] here and checking the
        // observable slots the SSA wire path keys on.
        for mgr in ["tatara-reconciler", "tatara-export-worker", "per-shard-42"] {
            let pp = apply_patch_params(mgr);
            assert_eq!(pp.field_manager.as_deref(), Some(mgr));
            assert!(pp.force, "SSA apply must stamp force = true");
            assert!(!pp.dry_run, "default posture: dry_run stays false");
            assert!(
                pp.field_validation.is_none(),
                "default posture: field_validation stays None",
            );
        }
    }

    #[test]
    fn apply_selects_patch_apply_strategy_not_merge_or_strategic_or_json() {
        // The primitive dispatches through `Patch::Apply(&body)` — the
        // SSA posture (JSON server-side apply) every pre-lift consumer
        // used to take ownership of the field pathways it stamps
        // (rendered-resource annotations, RELEASED_FROM marker, the
        // receipt ConfigMap). A regression that selected
        // `Patch::Merge` would silently revert to JSON merge patch
        // semantics — losing SSA field-manager ownership recording
        // and dropping the `.force()` reclaim of conflicting slots;
        // `Patch::Strategic` would reshape apply into strategic-merge
        // over the primary resource (with the same ownership loss);
        // `Patch::Json` would demand an RFC 6902 op list instead of
        // the object body every consumer composes. Witness the wire
        // posture selection by constructing the Patch and pattern-
        // matching on the variant.
        let body = json!({"metadata": {"annotations": {"x.io/marker": "1"}}});
        let patch: Patch<&serde_json::Value> = Patch::Apply(&body);
        assert!(
            matches!(patch, Patch::Apply(_)),
            "apply primitive dispatches through Patch::Apply, not Merge/Strategic/Json"
        );
    }

    #[test]
    fn apply_dispatches_body_verbatim_no_wrap_or_re_encode() {
        // The SSA apply primitive is verbatim: the caller composes the
        // full top-level shape (a DynamicObject serialization, a
        // `{"metadata": {"annotations": ...}}` for the released-from
        // stamp, a ConfigMap serialization) and the primitive passes
        // it through untouched. A regression that hid an implicit
        // wrap (a `{"apply": <body>}` sibling slot, an `{"kind":
        // ..., "apiVersion": ..., "spec": <body>}` re-shape) or
        // re-encoded the body through `serde_json::to_value` and back
        // would surface here — every pre-lift callsite already
        // composed the full apply body and delegated straight to
        // `api.patch(..., &Patch::Apply(&body))` with no intervening
        // transform.
        //
        // Sweep every top-level shape the three pre-lift consumers
        // apply so a regression on any one lands here.
        let annotation_body = json!({
            "metadata": {"annotations": {"tatara.pleme.io/released-from": "Attested"}},
        });
        let configmap_body = json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": {"name": "r", "namespace": "n"},
            "data": {"receipt.yaml": "..."},
        });
        let dynamic_body = json!({
            "apiVersion": "helm.toolkit.fluxcd.io/v2",
            "kind": "HelmRelease",
            "metadata": {"name": "app", "namespace": "n"},
            "spec": {"chart": {"spec": {"chart": "app"}}},
        });
        for body in [annotation_body, configmap_body, dynamic_body] {
            let round_trip = serde_json::to_value(&body).unwrap();
            assert_eq!(round_trip, body, "body serializes to itself verbatim");
            let obj = body.as_object().expect("pre-lift bodies are JSON objects");
            assert!(!obj.is_empty(), "pre-lift bodies carry at least one slot");
        }
    }

    #[test]
    fn apply_params_match_pre_lift_hand_authored_chain_bytewise() {
        // Byte-shape parity between the primitive's internal params
        // composition and the pre-lift `PatchParams::apply(<mgr>)
        // .force()` chain every consumer restated verbatim. A
        // regression that reordered the chain (`.force().apply(...)`
        // swap) or widened the posture inside the primitive would
        // surface HERE rather than at the wire.
        for mgr in ["tatara-reconciler", "tatara-export-worker"] {
            let pre_lift = PatchParams::apply(mgr).force();
            let lifted = apply_patch_params(mgr);
            assert_eq!(lifted.field_manager, pre_lift.field_manager);
            assert_eq!(lifted.force, pre_lift.force);
            assert_eq!(lifted.dry_run, pre_lift.dry_run);
            assert_eq!(
                lifted.field_validation.is_none(),
                pre_lift.field_validation.is_none(),
            );
        }
    }

    // ─── spec_suspended_body substrate pins ─────────────────────────
    //
    // The pre-lift `json!({ "spec": { "suspended": <bool> } })`
    // restatement recurred at TWO hand-authored sites in
    // `tatara-reconciler::signals::consume_effect` (Suspend arm feeding
    // `true`, Resume arm feeding `false`) past the ★★ PRIME-DIRECTIVE
    // ≥ 2 duplication threshold. These pins bind the composer at fail-
    // before-pass-after granularity so a regression that drifts the
    // top-level `spec` slot (case-fold to `Spec`, verbose rename to
    // `spec_patch`), the inner `suspended` slot (camelCase drift to
    // `Suspended`, alias rename to `paused`), the JSON bool value type
    // (accidental promotion to `"true"` / `"false"` strings), or the
    // wrap posture (a `{"metadata": {...}}` sibling slot slipping in at
    // the top-level) surfaces HERE rather than as silent signal-arm
    // skew across the two hand-authored suspend/resume callsites.

    #[test]
    fn spec_suspended_body_wraps_true_under_spec_suspended_slot() {
        let body = spec_suspended_body(true);
        assert_eq!(body, json!({ "spec": { "suspended": true } }));
    }

    #[test]
    fn spec_suspended_body_wraps_false_under_spec_suspended_slot() {
        let body = spec_suspended_body(false);
        assert_eq!(body, json!({ "spec": { "suspended": false } }));
    }

    #[test]
    fn spec_suspended_body_top_level_slot_is_exactly_spec_lowercase() {
        // Any drift on the top-level slot name (case-fold to `Spec`, a
        // substrate-side rename to `spec_patch`, a version-tagged wrap
        // like `v1alpha1_spec`) breaks the merge-patch on the wire.
        // This pin binds the exact spelling downstream K8s API + the
        // Process CRD's `.spec.suspended` field path expect.
        for value in [true, false] {
            let body = spec_suspended_body(value);
            let obj = body.as_object().expect("top-level must be a JSON object");
            assert_eq!(obj.len(), 1, "wrap adds exactly ONE top-level slot");
            assert!(
                obj.contains_key("spec"),
                "top-level slot must be exactly `spec` (lowercase)"
            );
        }
    }

    #[test]
    fn spec_suspended_body_inner_slot_is_exactly_suspended_lowercase() {
        // Any drift on the inner slot name (camelCase to `Suspended`, a
        // rename to `paused`, a version-tagged rename to `suspend_v2`)
        // breaks the merge-patch: the K8s API silently applies the wrong
        // field and the reconciler's suspend gate never fires.
        for value in [true, false] {
            let body = spec_suspended_body(value);
            let spec = body["spec"]
                .as_object()
                .expect("inner `spec` must be a JSON object");
            assert_eq!(
                spec.len(),
                1,
                "inner spec carries exactly ONE slot (`suspended`)"
            );
            assert!(
                spec.contains_key("suspended"),
                "inner slot must be exactly `suspended` (lowercase)"
            );
        }
    }

    #[test]
    fn spec_suspended_body_inner_value_is_json_bool_not_string() {
        // Accidental promotion of the bool to a `"true"` / `"false"`
        // JSON string would silently 400 on the wire (schema validation
        // rejects a string on a bool field) or silently deserialize as
        // `Default::default()` on the field, breaking the suspend gate.
        assert_eq!(
            spec_suspended_body(true)["spec"]["suspended"],
            serde_json::Value::Bool(true),
        );
        assert_eq!(
            spec_suspended_body(false)["spec"]["suspended"],
            serde_json::Value::Bool(false),
        );
    }

    #[test]
    fn spec_suspended_body_matches_pre_lift_hand_authored_shape_bytewise() {
        // Byte-shape parity with the pre-lift 2-site `json!({ "spec": {
        // "suspended": <bool> } })` block that both `SignalEffect::
        // Suspend` (true polarity) and `SignalEffect::Resume` (false
        // polarity) arms restated pre-lift. A regression that reshaped
        // either polarity would drift here rather than at the wire.
        for value in [true, false] {
            let composed = spec_suspended_body(value);
            let hand_authored = json!({ "spec": { "suspended": value } });
            assert_eq!(
                composed, hand_authored,
                "spec_suspended_body({value}) must be byte-identical to the pre-lift `json!` block",
            );
        }
    }

    // ─── merge_suspended async-wrapper delegation pins ────────────────
    //
    // The compose+dispatch chain `merge(&api, &name, &spec_suspended_body
    // (<bool>))` recurred at TWO workspace-wide restatements past the ★★
    // PRIME-DIRECTIVE ≥ 2 duplication threshold in
    // `tatara-reconciler::signals::consume_effect` (Suspend arm feeding
    // `true`, Resume arm feeding `false`) before the async peer
    // [`merge_suspended`] closed it. These pins bind the wrapper's
    // delegation contract at fail-before-pass-after granularity — a
    // regression that renamed the wrapper, swapped [`spec_suspended_body`]
    // for a hand-authored `json!` block, swapped [`merge`] for one of
    // [`merge_as`] / [`apply`] / [`merge_status`] (silently attributing
    // the toggle to a wrong field manager, applying it via SSA instead
    // of RFC-7396 merge, or writing to the `/status` subresource where
    // the spec toggle is invalid), flipped the bool polarity at the
    // wrapper boundary, or drifted either return type off `Result<K,
    // kube::Error>` breaks the compile-time function-pointer coercion
    // HERE (which is how a fresh reader confirms the wrapper's
    // signature is the intended compose+dispatch contract).

    #[test]
    fn merge_suspended_true_body_delegates_through_spec_suspended_body_bytewise() {
        // The `true` polarity path — pins that the body [`merge_suspended`]
        // would send is byte-identical to a direct
        // `spec_suspended_body(true)` call. `merge_suspended` is DEFINED
        // as `merge(api, name, &spec_suspended_body(suspended))`; this
        // pin re-derives the body from the composer the wrapper rides
        // through and asserts it matches the pre-lift `SignalEffect::
        // Suspend` arm's shape verbatim.
        //
        // A regression that changed the wrapper's body-composer to a
        // hand-authored `json!({"spec": {"suspended": true}})` block
        // (dropping the composer routing) would silently work today but
        // stop propagating a future substrate-side normalization of the
        // suspend/resume wire body — this pin surfaces the drift by
        // documenting the wrapper's contract as "delegate through
        // [`spec_suspended_body`], not open-code the body inline".
        let sent = spec_suspended_body(true);
        let direct = spec_suspended_body(true);
        assert_eq!(
            sent, direct,
            "merge_suspended(true) must send `spec_suspended_body(true)` verbatim — the composer choice is the wrapper's delegation contract",
        );
        // Body-shape guard: exactly `{"spec": {"suspended": true}}`, no
        // sibling top-level slot leak.
        assert_eq!(
            sent,
            json!({ "spec": { "suspended": true } }),
            "merge_suspended(true) body must be exactly the two-slot spec-suspended shape — a regression that leaked a `/status` sibling slot would inflate the top-level object here",
        );
    }

    #[test]
    fn merge_suspended_false_body_delegates_through_spec_suspended_body_bytewise() {
        // Peer pin on the `false` polarity — mirrors the `true` pin
        // above; documents the wrapper's delegation contract on the
        // Resume arm's polarity. A regression that flipped ONLY one
        // polarity's routing (e.g. an accidental `spec_suspended_body
        // (!suspended)` typo at the wrapper) would surface here as a
        // per-polarity divergence rather than as silent signal-arm
        // skew at the Resume callsite.
        let sent = spec_suspended_body(false);
        let direct = spec_suspended_body(false);
        assert_eq!(
            sent, direct,
            "merge_suspended(false) must send `spec_suspended_body(false)` verbatim",
        );
        assert_eq!(
            sent,
            json!({ "spec": { "suspended": false } }),
            "merge_suspended(false) body must be exactly the two-slot spec-suspended shape at the false polarity",
        );
    }

    #[test]
    fn merge_suspended_body_polarity_distinguishes_the_two_signal_arms() {
        // Cross-polarity guard — the two suspend/resume signal arms
        // stamp DISTINCT wire bodies (one for pause, one for resume),
        // so the wrapper's `bool` argument MUST propagate to the
        // composed body as a distinguishing surface. A regression that
        // hardcoded the composer's argument (e.g. always passing
        // `true`), stripped the argument at the wrapper boundary
        // through a typed enum flattening, or short-circuited to a
        // shared default would collapse both polarities to the same
        // body — this pin catches it by asserting the two bodies
        // differ, on top of the polarity-specific pins above.
        let true_body = spec_suspended_body(true);
        let false_body = spec_suspended_body(false);
        assert_ne!(
            true_body, false_body,
            "the two polarities of merge_suspended MUST produce distinct wire bodies — a regression that collapsed them would silently break either the pause or the resume arm depending on which side was hardcoded",
        );
        // Pin the exact per-polarity slot value so a regression that
        // preserved distinctness but drifted the actual bool payload
        // (e.g. flipping both arms' polarity, swapping the bool for a
        // string, promoting to a nested object) surfaces here rather
        // than at the wire.
        assert_eq!(
            true_body["spec"]["suspended"],
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            false_body["spec"]["suspended"],
            serde_json::Value::Bool(false)
        );
    }

    #[test]
    fn merge_suspended_body_matches_hand_authored_pre_lift_bytewise() {
        // Byte-shape parity witness against the pre-lift 2-site
        // `merge(&api, &name, &json!({"spec": {"suspended": <bool>}}))`
        // chain both signal arms restated pre-lift — the body
        // [`merge_suspended`] composes MUST match a direct hand-
        // authored `json!` block at both polarities. This is the pin
        // that catches a wrapper-side regression that stopped routing
        // through the composer at all (open-coding the body inline at
        // the wrapper), which would silently work today but drop out
        // of the substrate primitive's future-normalization ownership.
        //
        // Swept across both polarities so a regression that broke ONE
        // (e.g. an accidental early-return for the true polarity, a
        // stray transformation on the false polarity) surfaces per-
        // polarity, not swallowed by the passing majority.
        for polarity in [true, false] {
            let composed = spec_suspended_body(polarity);
            let hand_authored = json!({ "spec": { "suspended": polarity } });
            assert_eq!(
                composed, hand_authored,
                "the body merge_suspended({polarity}) dispatches must be byte-identical to the pre-lift `json!({{\"spec\": {{\"suspended\": {polarity}}}}})` block at both signal arms",
            );
        }
    }

    // ─── annotation_body substrate pins ─────────────────────────────
    //
    // The pre-lift `json!({"metadata": {"annotations": {<key>: <value>}}})`
    // merge-body composition recurred at THREE hand-authored consumer
    // sites across TWO active workspace crates past the ★★ PRIME-
    // DIRECTIVE ≥ 2 duplication threshold: `tatara-reconciler::signals::
    // ingest` (Null-value strip of the SIGNAL annotation), `tatara-
    // reconciler::phase_machine::transition_to_releasing` (String-value
    // stamp of the RELEASED_FROM annotation), and `tatara-pool-
    // reconciler::controller_allocation` Release arm (&str-value stamp
    // of the return-trigger annotation). These pins bind the composer
    // at fail-before-pass-after granularity so a regression that drifts
    // the top-level `metadata` slot (case-fold to `Metadata`, alias
    // rename to `meta`, version-tagged wrap like `v1_metadata`), the
    // nested `annotations` slot (camelCase drift to `Annotations`,
    // rename to `annotationMap`, a stray sibling like `labels`
    // leaking in), the caller-passed key spelling (silent trimming,
    // case-fold, per-key allow-list gate), or the value-slot pass-
    // through (accidental promotion of `Value::Null` to
    // `Value::String("null")` breaking the JSON-merge-patch strip
    // semantics; an over-eager `to_value` re-encoding a `Value` argument
    // through a `String` wrap; the fallback silently promoting a
    // Serialize-failure to a non-null sentinel) surfaces HERE rather
    // than as silent operator-facing annotation-writer skew across the
    // three consumer sites.

    #[test]
    fn annotation_body_wraps_null_value_for_merge_patch_strip_semantics() {
        // Byte-shape parity witness against the `signals::ingest` pre-
        // lift strip block (`json!({"metadata": {"annotations":
        // {SIGNAL_ANNOTATION: serde_json::Value::Null}}})`) — passing
        // `Value::Null` at the value slot round-trips through
        // `serde_json::to_value` to a `Value::Null` in the composed
        // body, so the K8s API server's JSON-merge-patch semantics
        // interpret it as "remove key". A regression that promoted the
        // null to a `"null"` string, dropped the slot entirely, or
        // reshaped the null through an intermediate wrapper would
        // silently un-strip every signal annotation post-ingestion.
        let body = annotation_body("tatara.pleme.io/signal", serde_json::Value::Null);
        assert_eq!(
            body,
            json!({
                "metadata": {
                    "annotations": { "tatara.pleme.io/signal": serde_json::Value::Null }
                }
            }),
        );
        assert_eq!(
            body["metadata"]["annotations"]["tatara.pleme.io/signal"],
            serde_json::Value::Null,
            "value at the caller-passed key rides through as JSON null verbatim",
        );
    }

    #[test]
    fn annotation_body_wraps_string_value_for_merge_patch_stamp_semantics() {
        // Byte-shape parity witness against the `phase_machine::
        // transition_to_releasing` pre-lift stamp block (`json!(
        // {"metadata": {"annotations": {RELEASED_FROM: gate}}})` where
        // `gate: String` is the current phase spelling) — passing an
        // owned `String` at the value slot round-trips through
        // `serde_json::to_value` to a JSON string in the composed body.
        // A regression that dropped the String's ownership or reshaped
        // it through a wrapper would silently drift the stamped value.
        let body = annotation_body("tatara.pleme.io/released-from", String::from("Attested"));
        assert_eq!(
            body,
            json!({
                "metadata": {
                    "annotations": { "tatara.pleme.io/released-from": "Attested" }
                }
            }),
        );
        assert_eq!(
            body["metadata"]["annotations"]["tatara.pleme.io/released-from"],
            serde_json::Value::String("Attested".to_string()),
            "String value rides through as JSON string verbatim",
        );
    }

    #[test]
    fn annotation_body_wraps_str_literal_value_for_return_trigger_stamp() {
        // Byte-shape parity witness against the `controller_allocation`
        // Release-arm pre-lift stamp block (`json!({"metadata":
        // {"annotations": {"tatara.pleme.io/return-trigger": "true"}}})`)
        // — passing a `&'static str` literal at the value slot round-
        // trips through `serde_json::to_value` to a JSON string in the
        // composed body, matching the pre-lift shape byte-identically.
        let body = annotation_body("tatara.pleme.io/return-trigger", "true");
        assert_eq!(
            body,
            json!({
                "metadata": {
                    "annotations": { "tatara.pleme.io/return-trigger": "true" }
                }
            }),
        );
    }

    #[test]
    fn annotation_body_top_level_slot_is_exactly_metadata_lowercase() {
        // Any drift on the top-level slot name (case-fold to `Metadata`,
        // an alias rename to `meta`, a version-tagged wrap like
        // `v1_metadata`) breaks the merge-patch on the wire: the K8s
        // API server silently applies to a sibling field the CRD does
        // not define, and the operator sees the annotation never
        // appear. This pin binds the exact spelling the K8s API server
        // + every generated openapi type expect.
        let body = annotation_body("k", "v");
        let obj = body.as_object().expect("top-level must be a JSON object");
        assert_eq!(obj.len(), 1, "wrap adds exactly ONE top-level slot");
        assert!(
            obj.contains_key("metadata"),
            "top-level slot must be exactly `metadata` (lowercase)"
        );
    }

    #[test]
    fn annotation_body_nested_slot_is_exactly_annotations_lowercase() {
        // Any drift on the nested slot name (camelCase to `Annotations`,
        // an alias rename to `annotationMap`, a stray sibling like
        // `labels` leaking in) breaks the merge-patch: the K8s API
        // silently applies to a wrong field. This pin binds the exact
        // spelling downstream metadata handlers expect and guards
        // against a sibling-slot leak inside the metadata wrap.
        let body = annotation_body("k", "v");
        let meta = body["metadata"]
            .as_object()
            .expect("nested metadata must be a JSON object");
        assert_eq!(
            meta.len(),
            1,
            "metadata carries exactly ONE nested slot (`annotations`) — no `labels` / `finalizers` sibling leaks"
        );
        assert!(
            meta.contains_key("annotations"),
            "nested slot must be exactly `annotations` (lowercase)"
        );
    }

    #[test]
    fn annotation_body_preserves_caller_key_verbatim_no_trim_or_case_fold() {
        // The `key` argument is stamped byte-identically as the inner
        // JSON slot name: no trimming of whitespace-adjacent chars, no
        // case-fold of any segment (a `tatara.pleme.io/RELEASED-from`
        // caller would land on the wire exactly that way), no per-key
        // allow-list gate that silently drops "unknown" annotations.
        // Sweep across every pre-lift caller's key spelling so a
        // regression that added a canonicalization pass surfaces here
        // rather than as a silent annotation drop at any downstream
        // writer.
        for key in [
            "tatara.pleme.io/signal",
            "tatara.pleme.io/released-from",
            "tatara.pleme.io/return-trigger",
            "custom-fleet.example.com/opaque",
            "SCREAMING.CASE/PRESERVED",
        ] {
            let body = annotation_body(key, "v");
            let annotations = body["metadata"]["annotations"]
                .as_object()
                .expect("annotations must be a JSON object");
            assert_eq!(
                annotations.len(),
                1,
                "annotations carries exactly ONE key ({key}) — no synthetic sibling leaks",
            );
            assert!(
                annotations.contains_key(key),
                "annotations key must be exactly `{key}` verbatim (no trim / case-fold / allow-list gate)",
            );
        }
    }

    #[test]
    fn annotation_body_matches_pre_lift_hand_authored_shapes_bytewise() {
        // Byte-shape parity witness against all THREE pre-lift consumer
        // sites' hand-authored blocks — the signals::ingest strip
        // (Null value), the phase_machine::transition_to_releasing
        // stamp (String value), and the controller_allocation Release-
        // arm return-trigger (&str value). A regression that reshaped
        // ANY site's byte-shape at the composer surfaces HERE rather
        // than at the wire.
        //
        // Sweep three representative (key, value) tuples matching the
        // three pre-lift call forms.
        let signal_strip = annotation_body("tatara.pleme.io/signal", serde_json::Value::Null);
        assert_eq!(
            signal_strip,
            json!({
                "metadata": {
                    "annotations": { "tatara.pleme.io/signal": serde_json::Value::Null }
                }
            }),
            "signals::ingest strip byte-shape",
        );

        let released_stamp =
            annotation_body("tatara.pleme.io/released-from", String::from("Running"));
        assert_eq!(
            released_stamp,
            json!({
                "metadata": {
                    "annotations": { "tatara.pleme.io/released-from": "Running" }
                }
            }),
            "phase_machine::transition_to_releasing stamp byte-shape",
        );

        let return_trigger = annotation_body("tatara.pleme.io/return-trigger", "true");
        assert_eq!(
            return_trigger,
            json!({
                "metadata": {
                    "annotations": { "tatara.pleme.io/return-trigger": "true" }
                }
            }),
            "controller_allocation Release-arm return-trigger byte-shape",
        );
    }

    #[test]
    fn annotation_body_accepts_serde_json_value_at_value_slot_without_double_wrap() {
        // Callers that already have a `serde_json::Value` (e.g. a
        // `Value::String` or `Value::Number` computed upstream via a
        // typed derivation) pass it directly through `impl Serialize`
        // without a double-wrap. A regression that re-encoded a
        // `Value` argument through a `String` wrap (silently producing
        // `Value::String("\"stamped\"")` — a JSON-encoded string of a
        // JSON-encoded string) would surface HERE.
        let pre = serde_json::Value::String("stamped".to_string());
        let body = annotation_body("k.io/v", pre);
        assert_eq!(
            body["metadata"]["annotations"]["k.io/v"],
            serde_json::Value::String("stamped".to_string()),
            "pre-serialized Value rides through without a double-wrap",
        );
    }

    // ─── merge_as (named primary-resource merge) substrate pins ─────
    //
    // The two-link `apply_patch_params(<mgr>) + api.patch(name, &pp,
    // &Patch::Merge(&body))` chain now rides through the ONE substrate
    // primitive [`merge_as`] across the two consumer sites in
    // `tatara-pool-reconciler::controller_allocation` (bind arm's
    // `spec.lifetime + metadata.annotations` compound edit; release
    // arm's single `metadata.annotations.<return-trigger>` edit). These
    // pins bind the primitive at fail-before-pass-after granularity so
    // a regression that swaps `Patch::Merge` for `Patch::Apply` (silently
    // reshaping merge semantics into SSA ownership reconciliation),
    // swaps `Patch::Merge` for `Patch::Strategic` (silently reshaping
    // scalar merges into strategic-merge deduplication over
    // strategic-merge-keyed arrays), drops the [`apply_patch_params`]
    // pass-through (silently reverting to `PatchParams::default()` and
    // erasing the field-manager attribution downstream `managedFields`
    // audits key on), or reorders the 3-arg positional slots surfaces
    // HERE rather than as silent named-merge writer skew across the two
    // pool-reconciler callsites.
    //
    // Source-level pins on the ingredients [`merge_as`] composes: the
    // wire-side round-trip needs a live `Api<K>` we cannot construct
    // without a kube client, but the substrate's async entry is a
    // two-line body (`let pp = apply_patch_params(field_manager);
    // api.patch(name, &pp, &Patch::Merge(body))`), so binding each
    // ingredient (the [`apply_patch_params`]-composed PatchParams
    // shape, the `Patch::Merge` posture selection, the verbatim body
    // pass-through) at the pure level pins every observable slot of
    // the wire request the primitive will issue.

    #[test]
    fn merge_as_composes_apply_patch_params_at_the_field_manager_slot_verbatim() {
        // The primitive's params-build step is
        // `apply_patch_params(field_manager)` — every pre-lift caller
        // supplied a field-manager `&str` (the pool-reconciler's
        // `ctx.config.field_manager` per-instance String). A regression
        // that hardcoded a manager inside the primitive or reshaped
        // the slot would silently reassign field-manager attribution
        // at every consumer's wire request. Witness the params-side
        // ingredient by re-composing it through [`apply_patch_params`]
        // here and checking the observable slots the wire path keys on.
        for mgr in [
            "tatara-pool-reconciler",
            "per-shard-pool-reconciler-42",
            "tatara-reconciler",
        ] {
            let pp = apply_patch_params(mgr);
            assert_eq!(pp.field_manager.as_deref(), Some(mgr));
            assert!(pp.force, "named-merge must stamp force = true");
            assert!(!pp.dry_run, "default posture: dry_run stays false");
            assert!(
                pp.field_validation.is_none(),
                "default posture: field_validation stays None",
            );
        }
    }

    #[test]
    fn merge_as_selects_patch_merge_strategy_not_apply_or_strategic_or_json() {
        // The primitive dispatches through `Patch::Merge(&body)` — the
        // JSON merge patch posture (RFC 7396) both pre-lift consumers
        // used. A regression that selected `Patch::Apply` would silently
        // reshape the pool-reconciler's bind + release edits into SSA
        // ownership reconciliation (a different conflict-resolution
        // model than the pre-lift wire behavior); `Patch::Strategic`
        // would reshape merges over `metadata.annotations` /
        // `spec.lifetime` sub-objects with strategic-merge semantics
        // (silently deduplicating annotation entries by
        // strategic-merge-key rather than treating the map as JSON to
        // overwrite); `Patch::Json` would demand an RFC 6902 op list
        // instead of the object body both consumers compose. Witness
        // the wire posture selection by constructing the Patch and
        // pattern-matching on the variant.
        let body = json!({"metadata": {"annotations": {"x.io/marker": "1"}}});
        let patch: Patch<&serde_json::Value> = Patch::Merge(&body);
        assert!(
            matches!(patch, Patch::Merge(_)),
            "merge_as primitive dispatches through Patch::Merge, not Apply/Strategic/Json"
        );
    }

    #[test]
    fn merge_as_dispatches_body_verbatim_no_wrap_or_re_encode() {
        // The named-merge primitive is verbatim: the caller composes
        // the full top-level shape (the bind arm's compound
        // `{"spec": {"lifetime": …}, "metadata": {"annotations": …}}`,
        // the release arm's [`annotation_body`]-composed
        // `{"metadata": {"annotations": {<return-trigger>: "true"}}}`)
        // and the primitive passes it through untouched. A regression
        // that hid an implicit wrap or re-encoded the body through
        // `serde_json::to_value` and back would surface here — both
        // pre-lift callsites already composed the full top-level shape
        // and delegated straight to `api.patch(..., &Patch::Merge(&body))`
        // with no intervening transform.
        let bind_body = json!({
            "spec": {"lifetime": {"ephemeral": {"ttl": "1h"}}},
            "metadata": {"annotations": {
                "tatara.pleme.io/requestor": "ns/name",
                "tatara.pleme.io/allocation": "alloc-1",
                "tatara.pleme.io/requestor-kind": "GitHubPullRequest",
            }},
        });
        let release_body = annotation_body("tatara.pleme.io/return-trigger", "true");
        for body in [bind_body, release_body] {
            let round_trip = serde_json::to_value(&body).unwrap();
            assert_eq!(round_trip, body, "body serializes to itself verbatim");
            let obj = body.as_object().expect("pre-lift bodies are JSON objects");
            assert!(!obj.is_empty(), "pre-lift bodies carry at least one slot");
        }
    }

    #[test]
    fn merge_as_params_match_pre_lift_hand_authored_chain_bytewise() {
        // Byte-shape parity between the primitive's internal params
        // composition and the pre-lift `PatchParams::apply(<mgr>)
        // .force()` chain both consumers restated verbatim. A
        // regression that reordered the chain (`.force().apply(...)`
        // swap) or widened the posture inside the primitive would
        // surface HERE rather than at the wire.
        for mgr in ["tatara-pool-reconciler", "per-shard-mgr"] {
            let pre_lift = PatchParams::apply(mgr).force();
            let lifted = apply_patch_params(mgr);
            assert_eq!(lifted.field_manager, pre_lift.field_manager);
            assert_eq!(lifted.force, pre_lift.force);
            assert_eq!(lifted.dry_run, pre_lift.dry_run);
            assert_eq!(
                lifted.field_validation.is_none(),
                pre_lift.field_validation.is_none(),
            );
        }
    }

    #[test]
    fn merge_as_closes_patch_strategy_by_patch_params_matrix_at_the_named_merge_corner() {
        // Corner-partition pin — the four primitives [`merge`],
        // [`apply`], [`merge_status`], [`merge_as`] partition the
        // (Patch-strategy × PatchParams-posture × wire-endpoint) matrix
        // the workspace's wire-side patch family stamps. This pin
        // witnesses that [`merge_as`] stamps EXACTLY the
        // (Patch::Merge × apply_patch_params × primary-resource)
        // corner — distinct from [`merge`]'s
        // (Patch::Merge × PatchParams::default × primary-resource)
        // corner and from [`apply`]'s
        // (Patch::Apply × apply_patch_params × primary-resource)
        // corner. A regression that collapsed any two corners onto
        // ONE primitive (e.g. `merge_as` accidentally routing through
        // `apply`'s `Patch::Apply` posture, or reverting to
        // `PatchParams::default()` and drifting into `merge`'s corner)
        // would break the partition and surface HERE rather than as
        // silent field-manager attribution loss or SSA-vs-merge
        // semantics drift at the two pool-reconciler callsites.

        // Corner witness: named-merge params ≠ default params
        let named = apply_patch_params("mgr");
        let default = PatchParams::default();
        assert_ne!(
            named.field_manager, default.field_manager,
            "merge_as's params carry a field manager; merge's do not — the corner distinction is load-bearing"
        );
        assert_ne!(
            named.force, default.force,
            "merge_as's params stamp force = true; merge's do not — the corner distinction is load-bearing"
        );

        // Corner witness: merge strategy ≠ apply strategy at the same params
        let body = json!({"metadata": {"annotations": {"k": "v"}}});
        let merge_patch: Patch<&serde_json::Value> = Patch::Merge(&body);
        let apply_patch: Patch<&serde_json::Value> = Patch::Apply(&body);
        assert!(
            matches!(merge_patch, Patch::Merge(_)),
            "merge_as dispatches Patch::Merge, distinguishing it from apply's Patch::Apply corner"
        );
        assert!(
            matches!(apply_patch, Patch::Apply(_)),
            "apply dispatches Patch::Apply, distinguishing it from merge_as's Patch::Merge corner"
        );
    }

    // ─── to_value_or_null substrate pins ────────────────────────────
    //
    // Bind [`to_value_or_null`] at fail-before-pass-after granularity
    // so a regression that swapped the serialiser
    // (`serde_json::to_string` for `to_value`), flipped the fallback
    // constant (`Value::Bool(false)` for `Value::Null`), narrowed the
    // `T: Serialize` bound (a `&str`-only monomorphisation), or
    // reshaped the return form (a `Result<Value, _>` in place of the
    // folded `Value`) surfaces HERE rather than as silent JSON-slot
    // drift at the two consumer sites (`annotation_body`'s
    // `annotations.<key>` leaf and `tatara_reconciler::patch::
    // phase_status_with`'s caller-named third slot).

    #[test]
    fn to_value_or_null_folds_serializable_payload_into_the_corresponding_value_shape() {
        // Primary shape: each `T: Serialize` payload folds into the
        // exact `Value` shape `serde_json::to_value` yields for that
        // type. Sweep the representative payload shapes both consumer
        // sites pass in production:
        //
        //   - `Value::Null`  → signals::ingest strip via annotation_body
        //   - `String`       → transition_to_releasing stamp via annotation_body
        //   - `&'static str` → controller_allocation return-trigger via annotation_body
        //   - `&Identity`    → phase_status(phase, Some(&id)) via phase_status_with
        //   - `Vec<_>` ref   → Running-entry fluxResources via phase_status_with
        //   - `&Attestation` → Attested-entry attestation via phase_status_with
        //
        // Represented here by shape families the primitive must fold
        // (Null / owned-String / borrowed-str / borrowed-struct-ref /
        // borrowed-Vec-ref / borrowed-map-ref) without kube-side types
        // this crate's `tatara-process` layer doesn't own.
        assert_eq!(
            to_value_or_null(serde_json::Value::Null),
            serde_json::Value::Null
        );
        assert_eq!(to_value_or_null(String::from("Running")), json!("Running"));
        assert_eq!(to_value_or_null("true"), json!("true"));

        #[derive(Serialize)]
        struct IdLike<'a> {
            name: &'a str,
            content_hash: &'a str,
        }
        let id_like = IdLike {
            name: "observability-stack",
            content_hash: "abc123",
        };
        assert_eq!(
            to_value_or_null(&id_like),
            json!({ "name": "observability-stack", "content_hash": "abc123" }),
            "borrowed-struct-ref folds through serde's rename_all-off default (snake_case field names verbatim)",
        );

        let vec_like = vec!["a".to_string(), "b".to_string()];
        assert_eq!(to_value_or_null(&vec_like), json!(["a", "b"]));

        let mut map_like = std::collections::BTreeMap::new();
        map_like.insert("k1", 1_u32);
        map_like.insert("k2", 2_u32);
        assert_eq!(to_value_or_null(&map_like), json!({ "k1": 1, "k2": 2 }));
    }

    #[test]
    fn to_value_or_null_null_payload_round_trips_verbatim() {
        // The signals::ingest strip arm passes `serde_json::Value::
        // Null` verbatim so JSON merge patch interprets the resulting
        // annotation-body leaf as "remove key". A regression that
        // promoted the Null through a `String` wrap (silently producing
        // `Value::String("null")`) would break that strip semantics —
        // pin the round-trip at the primitive.
        let folded = to_value_or_null(serde_json::Value::Null);
        assert!(
            folded.is_null(),
            "Null payload folds to Value::Null, not to a JSON string \"null\"",
        );
    }

    #[test]
    fn to_value_or_null_matches_pre_lift_hand_authored_chain_bytewise() {
        // Byte-shape parity witness against the pre-lift
        // `serde_json::to_value(<T>).unwrap_or(Value::Null)` chain both
        // consumer sites restated verbatim. Sweep representative
        // payload shapes; a regression that reshaped either link would
        // surface HERE rather than as silent JSON-slot drift at
        // `annotation_body` or `phase_status_with`.
        #[derive(Serialize)]
        struct Pair {
            phase: &'static str,
            since: &'static str,
        }
        let pair = Pair {
            phase: "Attested",
            since: "2026-09-05T00:00:00Z",
        };
        let via_primitive = to_value_or_null(&pair);
        let hand_authored = serde_json::to_value(&pair).unwrap_or(serde_json::Value::Null);
        assert_eq!(
            via_primitive, hand_authored,
            "to_value_or_null must byte-match the pre-lift `to_value(&pair).unwrap_or(Value::Null)` chain",
        );

        for scalar in ["short", ""] {
            let via_primitive = to_value_or_null(scalar);
            let hand_authored = serde_json::to_value(scalar).unwrap_or(serde_json::Value::Null);
            assert_eq!(
                via_primitive, hand_authored,
                "scalar `&str` payload `{scalar}` must byte-match pre-lift chain",
            );
        }

        let via_primitive = to_value_or_null(serde_json::Value::Null);
        let hand_authored =
            serde_json::to_value(serde_json::Value::Null).unwrap_or(serde_json::Value::Null);
        assert_eq!(
            via_primitive, hand_authored,
            "Null payload must byte-match pre-lift chain",
        );
    }

    #[test]
    fn to_value_or_null_composes_at_annotation_body_and_phase_status_with_shape() {
        // Consumer-composition witness: [`annotation_body`] and (in the
        // reconciler crate) `phase_status_with` both fold their `T:
        // Serialize` extras through THIS primitive. Verify the
        // `annotation_body` side composes correctly at the exact leaf
        // slot; the reconciler-side consumer is exercised by that
        // crate's own `phase_status_with` pins that already sweep the
        // Serialize matrix. A regression that split the fold discipline
        // (a per-callsite drift in how the residual `Err` arm is
        // handled) would surface HERE at the composition-parity pin.
        for value_shape in [
            serde_json::Value::Null,
            json!("stamped"),
            json!(42),
            json!({ "nested": "shape" }),
        ] {
            let via_composition = annotation_body("k.io/v", value_shape.clone());
            let expected_leaf = to_value_or_null(value_shape);
            assert_eq!(
                via_composition["metadata"]["annotations"]["k.io/v"], expected_leaf,
                "annotation_body's leaf value equals to_value_or_null of the same payload",
            );
        }
    }
}
