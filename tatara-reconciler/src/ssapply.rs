//! DynamicObject server-side apply — the bridge between the reconciler
//! and arbitrary K8s resources emitted by `render`.
//!
//! Pure helpers (owner ref injection, plural lookup, Ready condition parsing)
//! are unit-tested; the `apply_owned`/`fetch` entry points require a cluster.

use anyhow::{anyhow, Result};
use kube::api::{ApiResource, DynamicObject, Patch, PatchParams};
use kube::{Api, Client};
use serde_json::{json, Value};

use tatara_process::annotations;
use tatara_process::k8s_wire_identity::K8sWireIdentity;
use tatara_process::prelude::{FluxResourceRef, Process, RenderedResourceCoords};

/// Field manager string we use for all SSA writes.
pub const FIELD_MANAGER: &str = "tatara-reconciler";

/// Server-side-apply [`PatchParams`] with [`FIELD_MANAGER`] bound to the
/// `field_manager` slot and `force = true` — the reconciler-crate-local
/// wrapper that binds [`FIELD_MANAGER`] into the substrate primitive
/// [`tatara_process::patch::apply_patch_params`].
///
/// Pre-lift the 2-link `PatchParams::apply(<mgr>).force()` chain was
/// hand-authored at THREE reconciler-crate consumer sites past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold, and TWO of the three
/// silently bypassed the [`FIELD_MANAGER`] const by restating the
/// manager string as a bare `"tatara-reconciler"` literal:
///
/// * [`apply_owned`] (this module) — the DynamicObject SSA writer for
///   every rendered flux/aplicacao resource: read the const correctly
///   (`PatchParams::apply(FIELD_MANAGER).force()`).
/// * `phase_machine::begin_releasing` (`tatara-reconciler/src/phase_machine.rs`)
///   — the `RELEASED_FROM` annotation stamper on the transition into
///   `Releasing`: **hardcoded literal** (`PatchParams::apply("tatara-reconciler").force()`).
/// * `table_controller::reconcile` (`tatara-reconciler/src/table_controller.rs`)
///   — the `ProcessTable.status.claims` map SSA writer on the claim-
///   arbiter tick: **hardcoded literal** (`PatchParams::apply("tatara-reconciler").force()`).
///
/// The two hardcoded-literal sites carried a latent rename-drift
/// defect: any future rename of [`FIELD_MANAGER`] (a per-cluster
/// controller-instance suffix, a per-shard field-manager alias, a
/// migration to the `tatara-reconciler-controller` spelling to
/// disambiguate from `tatara-reconciler` the crate) would land at
/// [`apply_owned`] alone, and every wire request stamped by
/// `begin_releasing` / `table_controller::reconcile` would keep the
/// old manager string until an operator noticed conflicting field-
/// manager ownership at status update time. Post-lift the three
/// consumers share ONE substrate owner reading through the const, so
/// a rename propagates through all three writers mechanically and the
/// primitive's own pin
/// [`tests::apply_patch_params_binds_field_manager_const_verbatim`]
/// catches any regression that reintroduces a hand-authored literal.
///
/// Post workspace-wide lift the underlying 2-link chain rides through
/// the substrate primitive
/// [`tatara_process::patch::apply_patch_params`], which owns the
/// pass-through-parameterized SSA-side wire-posture across the three
/// workspace consumer crates (`tatara-reconciler`,
/// `tatara-pool-reconciler`, `tatara-export-worker`) — this wrapper
/// remains the reconciler-crate binding of [`FIELD_MANAGER`] into that
/// substrate slot, delegating the actual `PatchParams::apply(<mgr>)
/// .force()` composition rather than restating it. A future
/// normalization of the SSA-side posture (an added `dry_run` mode, a
/// `field_validation` default, an injectable retry policy) lands at
/// the substrate primitive and this wrapper inherits it mechanically.
///
/// The `force = true` semantics matches the SSA `force` directive
/// every pre-lift chain applied — the reconciler is the authoritative
/// owner of the field pathways it stamps (rendered-resource
/// annotations, `RELEASED_FROM` marker, `ProcessTable.status.claims`)
/// and reclaims conflicting slots from prior field-manager owners on
/// every apply.
///
/// A `#[must_use]` return keeps a caller from bulding a `PatchParams`
/// via this primitive and then dropping it un-passed to `Api::patch`;
/// the primitive exists to be consumed at a wire-side write, not to
/// probe field-manager state.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// `.apply(<mgr>).force()` chain recurred at 3 hand-authored sites
/// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is
/// lifted to ONE owner here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — the pin block below binds the
/// field-manager const-read + the `force = true` slot + the
/// byte-identical parity with the pre-lift chain, so a regression
/// that widens the corner has to move THESE pins rather than silently
/// trampling the SSA writer axis at every consumer).
#[must_use]
pub fn apply_patch_params() -> PatchParams {
    tatara_process::patch::apply_patch_params(FIELD_MANAGER)
}

/// Shared 2-slot `{MANAGED_BY: FIELD_MANAGER, PROCESS: process_ref}`
/// map — the ONE substrate primitive owning the byte-shape both
/// [`ownership_annotations`] and [`ownership_labels`] return.
///
/// Pre-lift the 2-slot body was open-coded twice: once at
/// `ownership_annotations` (annotations axis), once at
/// `ownership_labels` (labels axis), each restating the same
/// `serde_json::Map::new() + MANAGED_BY insert + PROCESS insert`
/// scaffold verbatim. The two axis-typed public primitives now
/// delegate to this ONE owner so the invariant "labels-axis and
/// annotations-axis ownership tags are byte-identical" holds by
/// construction, not by a cross-axis pin.
///
/// Private to the module: callers keep threading through the
/// axis-typed [`ownership_annotations`] / [`ownership_labels`]
/// primitives because their names encode "am I seeding an
/// annotations block or a labels block" at the callsite — a
/// grep-visible intent axis this internal helper deliberately
/// erases. If the two axes ever diverge (e.g. a labels-only
/// `VERSION` slot, or an annotations-only `LEASE_ID`), the
/// diverging axis extends this base map at its public wrapper;
/// the shared invariants (FIELD_MANAGER on MANAGED_BY, opaque
/// pass-through on PROCESS) stay owned here.
fn ownership_kv_pair(process_ref: &str) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    m.insert(
        annotations::MANAGED_BY.to_string(),
        Value::String(FIELD_MANAGER.to_string()),
    );
    m.insert(
        annotations::PROCESS.to_string(),
        Value::String(process_ref.to_string()),
    );
    m
}

/// Substrate-primitive builder for the standard tatara-reconciler
/// **ownership tag** — the 2-slot
/// `{MANAGED_BY: FIELD_MANAGER, PROCESS: process_ref}`
/// object shape every emitted resource in [`crate::render`] and every
/// routing edge in [`crate::edges`] marks itself with. Every K8s
/// resource this reconciler owns carries this pair so operators (and
/// external tooling — dashboards, GC sweeps, drift detectors) can
/// grep for "resources this reconciler manages" on ONE well-known key
/// pair rather than probing each resource's owner references.
///
/// Pre-lift the 2-slot shape was hand-authored at FIVE sites past the
/// PRIME-DIRECTIVE ≥ 2 duplication threshold:
/// * [`crate::render`] × 3 — Kustomization / OCIRepository /
///   HelmRelease metadata annotations, each literal
///   `json!({ MANAGED_BY: "tatara-reconciler",
///   PROCESS: format!("{ns}/{name}") })` inline.
/// * [`crate::edges::DnsEndpointEdge`] — the DNSEndpoint annotations
///   block, same 2-key literal.
/// * [`crate::edges::IngressEdge`] — the Ingress annotations map,
///   started as two `serde_json::Map::insert(...)` calls seeding the
///   same 2-slot pair before adding routing / TLS annotations.
///
/// PLUS the workspace's own [`inject_annotations`] gate re-authored
/// the same pair at its own local site as
/// `annot.insert(MANAGED_BY, FIELD_MANAGER); annot.insert(PROCESS,
/// format!("{ns}/{name}"));`, so the SSA-time re-injection now
/// delegates through the same primitive as the render-time authoring.
///
/// The literal render + edges sites additionally hand-coded the
/// `"tatara-reconciler"` string on the `MANAGED_BY` slot, so they
/// bypassed [`FIELD_MANAGER`] and would drift silently if the field
/// manager string is ever renamed. Post-lift every one of these sites
/// reads the const, so the invariant "MANAGED_BY == FIELD_MANAGER
/// across every emitted resource" holds by construction.
///
/// Returns a [`serde_json::Map`] rather than a [`Value`] so callers
/// can either drop it under an `"annotations"` / `"labels"` key with
/// `Value::Object(map)` inside a `json!` macro, or `extend` it with
/// additional keys (see [`crate::edges::IngressEdge`] which appends
/// routing-form + backend + cert-manager annotations to the same
/// map, or [`inject_annotations`] which appends PID + CONTENT_HASH
/// + GENERATION + ATTESTATION_ROOT).
///
/// A future addition (e.g. a `VERSION` slot naming the reconciler
/// build, a `LEASE_ID` slot for multi-instance leadership, a
/// `RECONCILE_GENERATION` counter for stall detection) lands at this
/// ONE substrate primitive and every downstream emit site inherits
/// the upgrade mechanically — no per-site hand-edit at render.rs,
/// edges.rs, or inject_annotations.
///
/// Delegates to the shared [`ownership_kv_pair`] owner so the
/// annotations-axis body is byte-identical to the labels-axis body
/// by construction — the pre-existing cross-axis coherence pin
/// [`tests::ownership_labels_pair_matches_annotations_pair`] now
/// holds by construction rather than by two open-coded copies
/// staying in sync.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 2-slot shape recurred at five hand-authored sites well past the
/// PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to ONE
/// owner here). THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — a regression that drifted the annotation key or the
/// field manager string at ONE site surfaces at
/// [`tests::ownership_annotations_produces_field_manager_and_process_ref`]
/// rather than as silent drift at every downstream emit site).
pub fn ownership_annotations(process_ref: &str) -> serde_json::Map<String, Value> {
    ownership_kv_pair(process_ref)
}

/// Substrate-primitive builder for the standard tatara-reconciler
/// **ownership label pair** — the 2-slot
/// `{MANAGED_BY: FIELD_MANAGER, PROCESS: process_ref}` object shape
/// every emitted resource in [`crate::render`] and every routing edge
/// in [`crate::edges`] marks its `metadata.labels` block with. Sibling
/// on the `labels` axis to [`ownership_annotations`] on the
/// `annotations` axis; both carry the SAME 2-key ownership pair so
/// operators grepping resources by either axis (label selectors from
/// `kubectl get -l tatara.pleme.io/process=…`, annotation lookups from
/// the reconciler's own drift detector) land at the identical key
/// pair — the `assert_ownership_pair_matches_annotations` pin binds
/// the invariant at compile-adjacent test granularity.
///
/// Pre-lift the 2-slot label shape was hand-authored at THREE sites
/// past the PRIME-DIRECTIVE ≥ 2 duplication threshold:
/// * [`crate::edges::IngressEdge`] — the Ingress `metadata.labels`
///   block, literal `annotations::MANAGED_BY: "tatara-reconciler",
///   annotations::PROCESS: ctx.process_ref` inside a `json!({...})`
///   before the routing-form + app extension.
/// * [`crate::edges::DnsEndpointEdge`] — the DNSEndpoint
///   `metadata.labels` block, same 2-key literal (already delegating
///   through [`ownership_annotations`] on the annotations axis).
/// * [`crate::render::one_export_job`] — the export Job
///   `metadata.labels` block, same 2-key literal before the ROLE +
///   EXPORT_INDEX extension.
///
/// The three literal sites additionally hand-coded the
/// `"tatara-reconciler"` string on the `MANAGED_BY` slot, so they
/// bypassed [`FIELD_MANAGER`] and would drift silently if the field
/// manager string is ever renamed. Post-lift every one of these
/// sites reads the const, so the invariant "MANAGED_BY ==
/// FIELD_MANAGER across every emitted resource's labels axis" holds
/// by construction, matching the invariant the annotations-axis
/// primitive already enforced.
///
/// **Peer sites intentionally NOT collapsed:** the export Job's pod
/// template `metadata.labels` at [`crate::render::one_export_job`]
/// carries only `{PROCESS, ROLE, EXPORT_INDEX}` — no `MANAGED_BY` —
/// because pod-template labels feed the Job's pod-selector wiring,
/// not reconciler ownership discovery, and adding `MANAGED_BY` there
/// would inflate the selector unnecessarily. That site is a
/// deliberately different shape, not a lift candidate.
///
/// Returns a [`serde_json::Map`] rather than a [`Value`] so callers
/// can either drop it under a `"labels"` key with `Value::Object(map)`
/// inside a `json!` macro, or `extend` it with additional keys
/// (see [`crate::edges::IngressEdge`] / [`crate::edges::DnsEndpointEdge`]
/// which append `app` + `routing-form` labels to the same map, or
/// [`crate::render::one_export_job`] which appends `role` +
/// `export-index` labels).
///
/// A future addition (e.g. a `VERSION` slot naming the reconciler
/// build, a `LEASE_ID` slot for multi-instance leadership, a
/// `RECONCILE_GENERATION` counter for stall detection) lands at this
/// ONE substrate primitive and every downstream emit site inherits
/// the upgrade mechanically — no per-site hand-edit at edges.rs or
/// render.rs. And because the sibling annotations primitive carries
/// the same 2-slot shape, a future `VERSION`/`LEASE_ID` slot added
/// to both primitives keeps the labels-axis and annotations-axis
/// ownership tags in lockstep by construction.
///
/// Delegates to the shared [`ownership_kv_pair`] owner so the
/// labels-axis body is byte-identical to the annotations-axis body
/// by construction — the pre-existing cross-axis coherence pin
/// [`tests::ownership_labels_pair_matches_annotations_pair`] now
/// holds by construction rather than by two open-coded copies
/// staying in sync.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the 2-slot labels shape recurred at three hand-authored sites
/// past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted
/// to ONE owner here). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — a regression that drifted the label key or
/// the field manager string at ONE site surfaces at
/// [`tests::ownership_labels_produces_field_manager_and_process_ref`]
/// / [`tests::ownership_labels_pair_matches_annotations_pair`]
/// rather than as silent drift at every downstream emit site).
pub fn ownership_labels(process_ref: &str) -> serde_json::Map<String, Value> {
    ownership_kv_pair(process_ref)
}

/// Substrate-primitive builder for the standard tatara-reconciler
/// **ownership annotations by (namespace, name) coordinates** — the
/// pre-composed shape [`ownership_annotations`] +
/// [`qualified_process_ref`] compose into at every callsite whose
/// input is a bare `(ns, name)` pair rather than a pre-computed
/// `process_ref`. Peer to [`ownership_annotations`]: the direct
/// primitive fits every callsite that already threads
/// `ctx.process_ref` (all three [`crate::edges`] sites, the
/// `render::one_export_job` label seed via a local
/// `process_ref` binding), and this composed primitive fits every
/// callsite whose input is the raw `(ns, name)` pair the enclosing
/// render / SSA function received from [`Process::coordinates_or_defaults`](tatara_process::prelude::Process::coordinates_or_defaults)
/// or from its own `name: &str, ns: &str` parameters.
///
/// Pre-lift the double composition
/// `ownership_annotations(&qualified_process_ref(ns, name))` was
/// hand-authored at FOUR sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold, each restating the same nested call chain:
/// * [`crate::render::render_flux`] — the Kustomization
///   `metadata.annotations` seed.
/// * [`crate::render::render_aplicacao`] × 2 — the OCIRepository +
///   HelmRelease `metadata.annotations` seeds, same shape.
/// * [`crate::ssapply::inject_annotations`] — the SSA-time
///   re-injection seed inside this module, feeding the standard
///   2-slot ownership tag through
///   `ownership_annotations(&qualified_process_ref(ns, name))` before
///   extending with PID / CONTENT_HASH / GENERATION /
///   ATTESTATION_ROOT.
///
/// Post-lift every one of these callsites reads through this ONE
/// primitive so a future change to the composition order (e.g. a
/// normalization step inserted between `qualified_process_ref` and
/// `ownership_annotations`, a per-coordinate override of one axis
/// only, a cross-cluster prefix rewrite that must happen BEFORE the
/// annotation map is seeded but AFTER the ref is composed) lands at
/// ONE substrate function here and every downstream emit site
/// inherits the upgrade mechanically — no per-site hand-edit at
/// `render_flux` / `render_aplicacao` × 2 / `inject_annotations`.
///
/// A caller with a pre-computed `process_ref` (e.g. the
/// [`crate::edges`] sites threading `ctx.process_ref`) still routes
/// through [`ownership_annotations`] directly — the two primitives
/// partition the input space by whether the caller has already
/// composed the ref. Cross-primitive coherence between the direct
/// and composed forms is pinned by
/// [`tests::ownership_annotations_by_coord_matches_hand_authored_double_call`]
/// so a regression that drifted the composition would surface here
/// rather than as silent operator-facing drift across the four
/// callsites.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the double-nested composition recurred at four hand-authored
/// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and
/// is lifted to ONE owner here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — the pins bind the composed
/// primitive to the direct-primitive-of-composed-ref composition
/// byte-identically, so a regression in either half surfaces at
/// `tests::ownership_annotations_by_coord_*`).
pub fn ownership_annotations_by_coord(ns: &str, name: &str) -> serde_json::Map<String, Value> {
    ownership_annotations(&qualified_process_ref(ns, name))
}

/// Substrate-primitive composer for a **Flux-owned resource's
/// `metadata` block** — owns the 3-slot `{name, namespace,
/// annotations: ownership_annotations_by_coord(ns, name)}` shape every
/// ownership-annotation-owned (label-free, ownerRef-free-at-render-
/// time) Flux resource stamps on its emit at [`crate::render`] time.
/// Peer to [`crate::edges::routing_edge_metadata`] on the routing-
/// edge axis (5-slot shape carrying `labels` + `ownerReferences`);
/// this primitive owns the 3-slot shape the Flux-owned axis emits
/// (Kustomization, OCIRepository, HelmRelease).
///
/// Pre-lift the 3-slot `metadata: { name, namespace, annotations:
/// ownership_annotations_by_coord(ns, name) }` block was hand-
/// authored at THREE sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold:
/// * [`crate::render::render_flux`] — the Kustomization
///   `metadata` seed.
/// * [`crate::render::render_aplicacao`] × 2 — the OCIRepository +
///   HelmRelease `metadata` seeds, same shape.
///
/// Post-lift every one of these callsites reads through this ONE
/// primitive so a future change to the 3-slot composition — a
/// `labels: ownership_labels_by_coord(ns, name)` slot added so
/// operators can `kubectl get -l tatara.pleme.io/process=…` on
/// Flux-owned resources too (today they can only grep by
/// annotation), a `finalizers` slot for cascade-delete gating at
/// the Flux-owned axis (peer to the routing-edge axis's owner-ref-
/// driven cascade), a `generateName` slot for duplicate-safe emits,
/// or a `resourceVersion` precondition slot for optimistic
/// concurrency — lands at ONE substrate function here and every
/// downstream Flux emit site inherits the upgrade mechanically. No
/// per-site hand-edit at `render_flux` / `render_aplicacao` × 2.
///
/// The routing-edge axis carries [`crate::edges::routing_edge_metadata`]'s
/// 5-slot shape ({name, namespace, labels, annotations,
/// ownerReferences}) because routing edges stamp their labels +
/// ownerReferences at render-time (routing edges never route
/// through the SSA-time `inject_annotations` re-injection path
/// this Flux-owned axis does). The two axes partition the emit-
/// site input space by whether the owner-ref + labels are stamped
/// at render-time (routing-edge axis) or at SSA-time
/// (Flux-owned axis via [`crate::ssapply::apply_owned`]'s owner-
/// ref injection + [`crate::ssapply::inject_annotations`]'s
/// annotation re-injection).
///
/// Cross-axis coherence with [`ownership_annotations_by_coord`] +
/// [`inject_annotations`]: at SSA-time [`inject_annotations`]
/// extends the render-time annotations map (this primitive's
/// third slot) with PID / CONTENT_HASH / GENERATION /
/// ATTESTATION_ROOT so the resource's final applied annotations
/// carry the FULL 6-slot tag (MANAGED_BY + PROCESS from this
/// composer + PID + CONTENT_HASH + GENERATION + ATTESTATION_ROOT
/// from `inject_annotations`). The render-time seed's 2-slot tag
/// living at this primitive means a rename of MANAGED_BY /
/// PROCESS / FIELD_MANAGER propagates through
/// [`ownership_annotations_by_coord`] to all three Flux emit sites
/// mechanically.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the 3-slot Flux metadata shape recurred at three hand-authored
/// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and
/// is lifted to ONE owner here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — the pins in
/// [`tests::owned_flux_metadata_*`] bind the composed primitive to
/// the pre-lift hand-authored shape byte-identically, so a
/// regression that drifted the slot naming, dropped a slot, or
/// reshaped one of the three slots surfaces here rather than at
/// every downstream Flux emit site).
pub fn owned_flux_metadata(ns: &str, name: &str) -> Value {
    json!({
        "name": name,
        "namespace": ns,
        "annotations": ownership_annotations_by_coord(ns, name),
    })
}

/// Substrate-primitive builder for the standard tatara-reconciler
/// **namespace-qualified process reference** — the `<ns>/<name>`
/// string every consumer that grepped, keyed, or annotated a
/// Process by "which cluster location owns it" composed by hand.
/// Peer to [`ownership_annotations`], whose `process_ref` parameter
/// is the value this primitive returns.
///
/// **Substrate move** — the composer body now lives in
/// `tatara_process`
/// ([`tatara_process::prelude::qualified_process_ref`]) so
/// callers BELOW the reconciler layer reach the SAME `<ns>/<name>`
/// composer without a `tatara-reconciler` dependency they cannot
/// take (crates like `tatara-export-worker` — whose CLI-driven
/// receipt-owner filter, run-id fallback, and `process_ref`
/// composition all previously hand-authored `format!("{ns}/{name}")`
/// — and `tatara-pool-reconciler` — whose allocation-time
/// requestor-annotation seed did the same). This wrapper stays as
/// the reconciler-side entry point so every pre-existing caller
/// (annotation seeds in [`crate::render`], the SSA-time re-injection
/// in [`inject_annotations`], the claim-arbiter comparator in
/// [`crate::phase_machine::process_holds_any_claim`], the export-Job
/// label-selector composer in [`crate::phase_machine::handle_releasing`])
/// keeps its module path unchanged, but every future addition to
/// the reference shape (a `<ns>/<name>@<gen>` multi-generation
/// variant for attestation grepping, a `<cluster>/<ns>/<name>`
/// cross-cluster form, a normalization pass) lands at the ONE
/// substrate composer.
///
/// Pre-lift the `format!("{ns}/{name}")` incantation was hand-
/// authored at SEVEN reconciler-side sites past the PRIME-DIRECTIVE
/// ≥ 2 duplication threshold:
/// * [`crate::render::render_flux`] — the Kustomization
///   `metadata.annotations` seed (`ownership_annotations(&format!
///   ("{ns}/{name}"))`).
/// * [`crate::render::render_aplicacao`] × 2 — the OCIRepository +
///   HelmRelease `metadata.annotations` seeds, same shape.
/// * [`crate::render::render_export_jobs`] — the per-Process
///   `process_ref` binding threaded through every emitted export
///   Job's owner metadata.
/// * [`crate::ssapply::inject_annotations`] — the SSA-time
///   re-injection's `ownership_annotations(&format!("{ns}/{name}"))`
///   feed, seeding the standard 2-slot ownership tag.
/// * [`crate::phase_machine::process_holds_any_claim`] — the
///   claim-arbiter's `holder` comparator (matches ProcessTable
///   claim rows keyed by `<ns>/<name>`).
/// * [`crate::phase_machine::handle_releasing`] — the export-Job
///   label-selector `PROCESS=<ns>/<name>` filter used to enumerate
///   THIS Process's Jobs (not any sibling Process's).
///
/// PLUS THREE hand-authored sites below the reconciler layer that
/// the substrate lift now covers:
/// * [`tatara-export-worker`]'s CLI-arg driven
///   `process_ref = format!("{}/{}", &cli.process_namespace,
///   &cli.process_name)` binding in the binary entry point.
/// * [`tatara-export-worker`]'s receipt-owner filter
///   (`format!("{ns}/{name}")` gate on the
///   `tatara.pleme.io/process` annotation) inside the
///   `ArtifactVariant::Receipts` handler.
/// * [`tatara-export-worker`]'s
///   [`tatara_export_worker::resolve_run_id`] fallback shape
///   (`format!("{namespace}/{name}")`) when no
///   `experiment_id_override` is set.
///
/// Post-lift every callsite reads through the substrate primitive
/// so a future change to the reference shape lands at ONE
/// composer and every downstream consumer (annotation seed,
/// ProcessTable claim key, label selector, owner metadata, run-id
/// fallback, receipt-owner filter) inherits the upgrade
/// mechanically.
///
/// The `&str` parameters accept both `&String` (which coerces via
/// deref) and `&str` literal / slice callers, matching every shape
/// currently authored: `render_flux` / `render_aplicacao` pass
/// their `ns: &str, name: &str` function params directly;
/// `render_export_jobs` / `handle_releasing` pass `&ns, &name`
/// from `String` locals; `inject_annotations` /
/// `process_holds_any_claim` pass `&str` slices from
/// `.as_deref().unwrap_or(...)`.
///
/// The 2-arg signature encodes the invariant "the qualified
/// reference is EXACTLY `<ns>/<name>`, in that order, joined by
/// a single `/` separator" at the type level — a caller cannot
/// accidentally swap the two axes (which would produce
/// `<name>/<ns>` and silently break every downstream grep) nor
/// omit either half, the way a pre-lift hand-authored
/// `format!("{name}/{ns}")` or `format!("{ns}-{name}")` typo
/// would.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the `<ns>/<name>` shape recurred at ten hand-authored sites
/// well past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is
/// lifted to ONE substrate owner in `tatara-process`).
/// THEORY.md §II.1 invariant 5 (composition preserves proofs — a
/// regression that swapped the two axes or the separator at ONE
/// site surfaces at
/// [`tests::qualified_process_ref_joins_ns_and_name_with_slash`]
/// (+ the substrate-side pin set at
/// `tatara_process::qualified_process_ref_tests::*`) rather than
/// as silent drift at every downstream annotation seed / claim
/// key / label selector / run-id / receipt-owner filter).
pub fn qualified_process_ref(ns: &str, name: &str) -> String {
    tatara_process::prelude::qualified_process_ref(ns, name)
}

/// Substrate-primitive resolver for the standard tatara-reconciler
/// **boundary-target namespace** — the answer to "which cluster
/// namespace do I look this external resource up in?" given an
/// optional operator override on the payload plus the enclosing
/// Process's default namespace. Peer to [`qualified_process_ref`] on
/// the same K8s addressing axis: this primitive answers the ns-slot
/// for a boundary evaluator lookup, [`qualified_process_ref`]
/// composes the two-slot `<ns>/<name>` reference every ownership
/// tag / claim key / label selector reads. Complementary to
/// [`Process::namespace_or_default`](tatara_process::prelude::Process::namespace_or_default),
/// which resolves the SELF-namespace of a Process's own metadata;
/// this primitive resolves an EXTERNAL-target namespace on a
/// boundary evaluator's payload against a `default_ns` — the two
/// primitives are dual halves of the same "K8s ns fallback"
/// convention on the two sides of a boundary evaluator's fetch call.
///
/// Pre-lift the `.as_deref().unwrap_or(default_ns)` incantation was
/// hand-authored at FIVE sites past the PRIME-DIRECTIVE ≥ 2
/// duplication threshold in [`crate::boundary`]:
/// * `evaluate_job_attested` — `JobAttestedParams.namespace` slot
///   resolved against the Process's default_ns before fetching the
///   `batch/v1` Job.
/// * `evaluate_closed_loop_auth` — `ClosedLoopAuthParams.namespace`
///   slot resolved before fetching the closed-loop probe Job + its
///   receipt ConfigMap.
/// * `evaluate_process_phase` — `ProcessPhaseParams.namespace` slot
///   resolved before fetching the referenced Process.
/// * `evaluate_flux_ready` — `NamedResourceParams.namespace` slot
///   resolved before fetching the Flux Kustomization / HelmRelease.
/// * `check_depends_on` — [`tatara_process::prelude::DependsOn`]'s
///   `namespace` slot resolved before fetching each dependency
///   Process, one call per `spec.dependsOn` entry.
///
/// Post-lift every one of these callsites reads through this ONE
/// primitive so a future normalization (case-fold, unicode-safe
/// collation, cluster-prefix stripping for cross-cluster refs, a
/// virtual-cluster prefix rewrite for multi-tenant setups) lands at
/// ONE substrate function here and every downstream boundary
/// evaluator inherits the upgrade mechanically — no per-site edit
/// at `evaluate_job_attested` / `evaluate_closed_loop_auth` /
/// `evaluate_process_phase` / `evaluate_flux_ready` /
/// `check_depends_on`. A regression that re-inlined the
/// `.unwrap_or(default_ns)` incantation at one site — or, worse,
/// swapped the two arguments so that the operator override became
/// the fallback — surfaces at the sibling pins under
/// `tests::resolve_target_namespace_...` rather than as silent drift
/// at every downstream ns-scoped `Api::namespaced` call.
///
/// The 2-arg signature (`Option<&str>`, `&str`) mirrors the
/// callsite-native `parsed.namespace.as_deref()` shape at every
/// evaluator so the lift is a mechanical drop-in — the resolver's
/// contract is exactly "fall back to `default_ns` when the operator
/// omitted the override", nothing more, nothing less. A future
/// caller with an owned `Option<String>` (rather than the
/// per-request `Deserialize`-owned carriers at every current
/// callsite) still rides through via `.as_deref()`, matching the
/// borrow discipline every current callsite already threads.
///
/// The `'a` lifetime binds the returned slice to whichever input
/// outlives the other — the explicit override wins when present,
/// otherwise the default. Both inputs share the SAME lifetime `'a`
/// so a caller cannot accidentally return a reference into a
/// dropped local (e.g. threading a temporary `String::from(...)`
/// into either slot); the type system rejects that at the
/// definition site.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the `.as_deref().unwrap_or(default_ns)` incantation recurred at
/// five hand-authored sites well past the PRIME-DIRECTIVE ≥ 2
/// duplication trigger, and is lifted to ONE owner here).
/// THEORY.md §II.1 invariant 5 (composition preserves proofs — the
/// pins bind both branches of the fallback gate + the
/// argument-order invariant, so a regression surfaces at
/// [`tests::resolve_target_namespace_falls_back_to_default_when_explicit_is_none`]
/// / [`tests::resolve_target_namespace_returns_explicit_when_present`]
/// rather than as silent operator-facing drift across the five
/// boundary evaluators).
pub fn resolve_target_namespace<'a>(explicit: Option<&'a str>, default_ns: &'a str) -> &'a str {
    explicit.unwrap_or(default_ns)
}

/// Resolve an `ApiResource` for `apiVersion/kind`. Hand-maintains plurals
/// for resources we emit or consume — good enough for v0; future move to
/// `kube::discovery` lands when we want to handle arbitrary CRDs.
pub fn api_resource(api_version: &str, kind: &str) -> Result<ApiResource> {
    let (group, version) = match api_version.split_once('/') {
        Some((g, v)) => (g.to_string(), v.to_string()),
        // Core/v1 has no group — api_version is just "v1".
        None => (String::new(), api_version.to_string()),
    };
    let plural = plural_of(kind)?;
    Ok(ApiResource {
        group,
        version,
        api_version: api_version.to_string(),
        kind: kind.to_string(),
        plural: plural.to_string(),
    })
}

fn plural_of(kind: &str) -> Result<&'static str> {
    match kind {
        // Flux source-controller
        "GitRepository" => Ok("gitrepositories"),
        "HelmRepository" => Ok("helmrepositories"),
        "OCIRepository" => Ok("ocirepositories"),
        "Bucket" => Ok("buckets"),
        // Flux kustomize-controller
        "Kustomization" => Ok("kustomizations"),
        // Flux helm-controller
        "HelmRelease" => Ok("helmreleases"),
        // K8s builtin (batch/v1) — Job's plural is required by the
        // `boundary::fetch_job_status` fetch site of the typed
        // [`tatara_process::K8sBuiltinResource::Job`] closed set;
        // pre-lift this arm was absent and every Job fetch through
        // the raw two-`&str` `ssapply::fetch` signature would have
        // failed at `api_resource` with `unknown plural for kind
        // "Job"` at wire time — a latent bug surfaced by the typed
        // cross-path parity pin
        // [`tests::api_resource_of_matches_raw_api_resource_across_every_k8s_builtin_variant`]
        // rather than by any pre-existing runtime probe.
        "Job" => Ok("jobs"),
        // Core kinds we might emit later
        "ConfigMap" => Ok("configmaps"),
        "Secret" => Ok("secrets"),
        "Namespace" => Ok("namespaces"),
        other => Err(anyhow!("unknown plural for kind {other:?}")),
    }
}

/// Server-side apply a JSON resource, injecting owner reference + standard
/// tatara annotations derived from the Process.
///
/// The 3-slot (apiVersion, kind, metadata.name) coordinate extraction
/// delegates to [`RenderedResourceCoords::from_json`] — the substrate
/// primitive that owns the shared `.get(K).and_then(|v| v.as_str())
/// .ok_or_else(|| anyhow!("rendered resource missing X"))?
/// .to_string()` walk. The `namespace` argument stays caller-
/// supplied (the reconciler resolved the target namespace upstream
/// via `resolve_target_namespace`); the primitive's own
/// `namespace_or_default` slot is unused here because the caller
/// already committed to a resolved target.
pub async fn apply_owned(
    client: Client,
    process: &Process,
    namespace: &str,
    mut resource: Value,
) -> Result<()> {
    inject_owner_reference(&mut resource, build_owner_reference(process)?)?;
    inject_annotations(&mut resource, process)?;

    let coords = RenderedResourceCoords::from_json(&resource)?;
    let ar = api_resource(&coords.api_version, &coords.kind)?;
    let obj: DynamicObject = serde_json::from_value(resource)?;
    let api: Api<DynamicObject> = Api::namespaced_with(client, namespace, &ar);

    let pp = apply_patch_params();
    api.patch(&coords.name, &pp, &Patch::Apply(&obj))
        .await
        .map_err(|e| anyhow!("ssapply {}/{}: {e}", coords.kind, coords.name))?;
    Ok(())
}

/// Fetch a DynamicObject by kind + namespace + name. Returns None on 404.
pub async fn fetch(
    client: Client,
    namespace: &str,
    api_version: &str,
    kind: &str,
    name: &str,
) -> Result<Option<DynamicObject>> {
    let ar = api_resource(api_version, kind)?;
    let api: Api<DynamicObject> = Api::namespaced_with(client, namespace, &ar);
    Ok(api.get_opt(name).await?)
}

/// Compose the `kube::api::ApiResource` from a typed
/// [`K8sWireIdentity`] pair — the ONE substrate primitive owning the
/// (identity → ApiResource) mapping every fetch-by-closed-set-variant
/// site composes against. Peer projection to [`api_resource`] on the
/// raw `(&str, &str)` axis; the typed variant gates the two slots so a
/// caller cannot swap them across the call — a copy-paste that
/// inverted `api_version` and `kind` in a raw
/// `api_resource(kind, api_version)` inversion would silently 404 at
/// wire time, while the same inversion at the identity's construction
/// is blocked structurally by
/// [`K8sWireIdentity::new`]'s positional pin.
pub fn api_resource_of(identity: K8sWireIdentity) -> Result<ApiResource> {
    api_resource(identity.api_version, identity.kind)
}

/// Fetch a DynamicObject by its typed K8s wire-form identity + name +
/// namespace. Returns `None` on 404.
///
/// Peer to [`fetch`] on the raw `(api_version: &str, kind: &str)`
/// axis; the typed [`K8sWireIdentity`] argument owns the
/// `(apiVersion, kind)` pair as ONE composite slot so a caller cannot
/// invert the two adjacent `&str` slots. Pre-lift the raw `fetch`
/// signature was called at THREE production sites past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold in `tatara-reconciler`,
/// each threading a `.api_version(), .kind()` pair off a typed closed-
/// set variant into the two adjacent `&str` slots:
///
/// * `boundary::fetch_job_status` — split
///   [`tatara_process::k8s_builtin_resource::K8sBuiltinResource::Job`]
///   into `Job.api_version(), Job.kind()`.
/// * `boundary::verify_receipt_cm` — split
///   [`tatara_process::k8s_builtin_resource::K8sBuiltinResource::ConfigMap`]
///   into `ConfigMap.api_version(), ConfigMap.kind()`.
/// * `boundary::evaluate_flux_ready` — split
///   [`tatara_process::flux_resource::FluxResource`] into
///   `resource.api_version(), resource.kind()`.
///
/// At every pre-lift site the two projections read from the SAME
/// closed-set variant, but the signature admitted arbitrary
/// `(&str, &str)` pairs at the call: a copy-paste that swapped the two
/// slots (`Job.kind(), Job.api_version()`) or paired one variant's
/// apiVersion with another's kind (`Job.api_version(),
/// ConfigMap.kind()`) would silently 404 at wire time and diagnose as
/// a broken CRD rather than as slot skew at the call. Post-lift each
/// caller names the variant ONCE via `.wire_identity()`; the pair
/// binds structurally at [`K8sWireIdentity`] so an inversion at any
/// callsite is unrepresentable.
///
/// Extension: every future consumer that fetches a K8s resource whose
/// wire-form identity lives on one of the three sibling closed sets
/// ([`tatara_process::flux_resource::FluxResource`],
/// [`tatara_process::routing_edge_resource::RoutingEdgeResource`],
/// [`tatara_process::k8s_builtin_resource::K8sBuiltinResource`])
/// inherits the typed slot mechanically through the same
/// `.wire_identity()` projection — no per-callsite `(api_version,
/// kind)` pair. The primary compounding beneficiary is the P3 kenshi-
/// runner lift (documented in CLAUDE.md's Ephemeral story deferred
/// milestones) — kenshi-runner's per-suite Job + receipt ConfigMap
/// fetches ride through the SAME `K8sBuiltinResource::{Job,ConfigMap}`
/// variants that `boundary::fetch_job_status` +
/// `boundary::verify_receipt_cm` already name.
///
/// Theory grounding: THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — the two-slot `(apiVersion, kind)` pair now binds
/// structurally at the typed identity, so a hypothetical slot-inversion
/// regression that got past `.wire_identity()`'s per-variant coherence
/// pin would fail-loudly at the identity's positional pin
/// [`tatara_process::k8s_wire_identity::tests::new_pairs_api_version_and_kind_by_position`]
/// rather than as a silent wire-time 404 at every fetch consumer).
/// THEORY.md §VI.1 (generation over composition — the raw two-slot
/// fetch signature was called at three hand-authored sites past the
/// ≥ 2 duplication trigger, each threading the pair off a closed-set
/// variant; post-lift the pair rides through ONE typed argument).
pub async fn fetch_by_identity(
    client: Client,
    namespace: &str,
    identity: K8sWireIdentity,
    name: &str,
) -> Result<Option<DynamicObject>> {
    // Delegate to the raw two-`&str` [`fetch`] path so the identity-
    // gated composer AND the raw path share ONE implementation body —
    // a regression that specialized one path (a stale-cached
    // `ApiResource`, a normalization step on one side only) is
    // unrepresentable. Error wrapping rides through
    // [`fetch_by_identity_error_context`] so the diagnostic wording
    // sits at ONE substrate helper alongside the peer
    // [`flux_ref_fetch_error_context`] — pre-lift each of the three
    // callsites in `tatara-reconciler::boundary` hand-authored the
    // `.map_err(|e| anyhow!("fetch <Kind-literal> {ns}/{name}: {e}"))?`
    // wrap verbatim, restating `identity.kind` as a bare `&str`
    // literal at each site (`"Job"`, `"ConfigMap"`, `"{kind}"`).
    // Post-lift the label rides through `identity.kind` and the
    // `<ns>/<name>` half rides through [`qualified_process_ref`]; a
    // copy-paste that changed the fetched closed-set variant without
    // also updating the label is unrepresentable because the two
    // mentions of the variant collapse into ONE at the callsite.
    fetch(client, namespace, identity.api_version, identity.kind, name)
        .await
        .map_err(|e| {
            anyhow!(
                "{}: {e}",
                fetch_by_identity_error_context(identity, namespace, name)
            )
        })
}

/// Standardized diagnostic label prefix for every
/// [`K8sWireIdentity`]-gated fetch — the pure helper the
/// [`fetch_by_identity`] composer wraps its raw-fetch error with.
///
/// Pre-lift the `.map_err(|e| anyhow!("fetch <kind-literal> {ns}/{name}:
/// {e}"))?` incantation was hand-authored verbatim at THREE
/// `tatara-reconciler::boundary` sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold, each restating the closed-set variant's
/// [`K8sWireIdentity::kind`] slot as a bare `&str` literal at the
/// `anyhow!` site alongside the same variant's `.wire_identity()` at
/// the `fetch_by_identity` call:
///
/// * [`crate::boundary::fetch_job_status`] — `"fetch Job {ns}/{name}"`
///   alongside [`K8sBuiltinResource::Job.wire_identity()`].
/// * [`crate::boundary::verify_receipt_cm`] — `"fetch ConfigMap
///   {ns}/{name}"` alongside
///   [`K8sBuiltinResource::ConfigMap.wire_identity()`].
/// * [`crate::boundary::evaluate_flux_ready`] — `"fetch {kind}
///   {ns}/{name}"` (where `kind = resource.kind()`) alongside
///   [`FluxResource::X.wire_identity()`].
///
/// Every pre-lift site mentioned the variant TWICE (once at the
/// fetch's `.wire_identity()`, once at the label's `&str` literal),
/// leaving a silent-drift trap: a copy-paste that changed the fetched
/// variant without updating the label (or vice versa) would produce
/// an operator-facing error whose label named a resource the fetch
/// never touched — a wire-form 404 diagnosed by kubectl-side grep as
/// a stale sibling. Post-lift the label rides through `identity.kind`
/// so the label + fetch pair collapse into ONE mention of the
/// variant.
///
/// The `<ns>/<name>` half of the label rides through the already-
/// opened [`qualified_process_ref`] substrate primitive — sibling to
/// the seven other consumers ([`crate::render::render_flux`],
/// [`crate::render::render_aplicacao`] × 2,
/// [`crate::render::render_export_jobs`], [`inject_annotations`],
/// [`crate::phase_machine::process_holds_any_claim`],
/// [`crate::phase_machine::handle_releasing`]) so a future
/// normalization (case-fold, unicode-safe collation, a
/// `<cluster>/<ns>/<name>` cross-cluster form) reaches BOTH the
/// annotation seed shape AND the diagnostic label shape through
/// ONE owner.
///
/// Peer to [`flux_ref_fetch_error_context`] on the diagnostic-wording
/// axis; the two composers partition the fetch-error wording space
/// by whether the caller reaches the raw fetch through a closed-set
/// variant (identity-gated) or a persisted status slice
/// ([`FluxResourceRef`]-gated). A future rename of either wording
/// (a switch to `"fetch <apiVersion> <kind> <ns>/<name>"` for
/// group-tagged observability, a case-fold on the kind slot, a
/// truncation for long-name refs) lands at ONE substrate function
/// per axis.
///
/// Private to the module (module-scope `fn`): no external consumer
/// names the diagnostic wording directly — the wording is consumed
/// EXCLUSIVELY through the [`fetch_by_identity`] composer's error
/// path, and the pins at
/// [`tests::fetch_by_identity_error_context_names_kind_and_ns_slash_name`]
/// bind the "`fetch <kind> <ns>/<name>`" wording invariant so a
/// regression that drifted the wording surfaces at fail-before-pass-
/// after granularity rather than as operator-visible log skew across
/// the three downstream consumers.
///
/// Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — the label's `kind` half + the `<ns>/<name>` half bind
/// structurally through `identity.kind` + `qualified_process_ref`, so
/// a regression that reworded either surfaces at the sibling pins
/// rather than as silent operator-facing log skew at every downstream
/// fetch consumer). THEORY.md §VI.1 (generation over composition —
/// the `.map_err(|e| anyhow!(...))?` incantation recurred at three
/// hand-authored sites past the PRIME-DIRECTIVE ≥ 2 duplication
/// trigger, and is lifted to ONE composer here).
fn fetch_by_identity_error_context(identity: K8sWireIdentity, ns: &str, name: &str) -> String {
    format!(
        "fetch {} {}",
        identity.kind,
        qualified_process_ref(ns, name)
    )
}

/// Standardized diagnostic label prefix for every
/// [`FluxResourceRef`]-gated fetch — the pure `format!("fetch {}/{}", ...)`
/// helper the [`fetch_flux_ref`] composer wraps its raw-fetch error
/// with.
///
/// Pre-lift the `.map_err(|e| anyhow!("fetch {}/{}: {e}", r.kind,
/// r.name))?` incantation was hand-authored verbatim at TWO
/// `tatara-reconciler::phase_machine` sites past the ★★ PRIME-DIRECTIVE
/// ≥ 2 duplication threshold alongside the 5-slot fetch splat. The
/// composer that lifts the splat also lifts the diagnostic wording so
/// a rename of the label (a switch to
/// `"fetch flux <kind>/<namespace>/<name>"` for cluster-tagged
/// observability, a case-fold on the kind slot, a truncation for
/// long-name refs) lands at ONE substrate function here.
///
/// Private to the module: no external consumer names the diagnostic
/// label directly — the wording is consumed EXCLUSIVELY through the
/// [`fetch_flux_ref`] composer's error path, and the pin at
/// [`tests::flux_ref_fetch_error_context_names_kind_and_name`] binds
/// the "`fetch <kind>/<name>`" wording invariant so a regression that
/// drifted the wording surfaces at fail-before-pass-after granularity
/// rather than as operator-visible log skew across the two consumers.
///
/// Uses `r.fetch_coords()`'s `(kind, name)` tuple slots (positions 2
/// and 3) so a swap between the two axes at the [`FluxResourceRef`]
/// projection surfaces symmetrically at the diagnostic label — the
/// pin fails-loudly rather than surfacing as a silent "`fetch
/// <name>/<kind>: ...`" reversal at every operator-facing log line.
fn flux_ref_fetch_error_context(r: &FluxResourceRef) -> String {
    let (_, _, kind, name) = r.fetch_coords();
    format!("fetch {kind}/{name}")
}

/// Fetch a Flux resource by its persisted
/// [`FluxResourceRef`] — the substrate composer that owns the
/// (`FluxResourceRef` → `DynamicObject`) path every Flux drift /
/// readiness probe composes against. Peer to [`fetch_by_identity`] on
/// the closed-set-variant-gated axis; the two composers partition the
/// Flux-fetch input space by whether the caller starts from a static
/// closed-set variant (emit-time, `.wire_identity()`-gated,
/// `K8sWireIdentity`) or a persisted `ProcessStatus.flux_resources`
/// slice (fetch-time, `FluxResourceRef`-gated, owned `String` payload).
///
/// The 4-slot `(namespace, api_version, kind, name)` fetch tuple
/// binds at [`FluxResourceRef::fetch_coords`]'s typed projection so a
/// caller cannot skew any two adjacent `&str` slots at the raw
/// [`fetch`] call — the `String` fields on `FluxResourceRef` are
/// mechanically indistinguishable at the type level, and the tuple's
/// positional pin
/// [`tatara_process::status::tests::flux_resource_ref_fetch_coords_binds_slots_by_position`]
/// is the fail-loud gate against a slot-inversion regression.
///
/// The `.map_err(|e| anyhow!("fetch {}/{}: {e}", r.kind, r.name))?`
/// diagnostic wording is owned by [`flux_ref_fetch_error_context`] —
/// its own pin binds the "`fetch <kind>/<name>`" invariant so a
/// downstream rename of the label surfaces at fail-before-pass-after
/// granularity rather than at every operator-facing log line.
///
/// Pre-lift the 5-slot splat + wrap incantation was hand-authored
/// verbatim at TWO sites in `tatara-reconciler::phase_machine` past
/// the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold:
/// * `handle_running` — the VERIFY-phase per-ref readiness probe
///   that populates the updated `FluxResourceRef` slice with
///   `ready` + `message` + `last_check`.
/// * `handle_attested` — the ATTEST-heartbeat drift detector that
///   short-circuits on the first non-Ready ref.
///
/// Both sites splatted `client, &r.namespace, &r.api_version,
/// &r.kind, &r.name` in THAT order into raw [`fetch`] and wrapped the
/// error with the same `"fetch {kind}/{name}"` prefix. Post-lift each
/// site names the ref ONCE and rides through this ONE composer.
///
/// Extension: every future consumer that reaches for a persisted
/// [`FluxResourceRef`] (a kenshi-runner probe walking the emitted
/// FluxCD test-fixture chart's `ProcessStatus.flux_resources`, a
/// per-membro contract's mirror-audit sweep, a stall-detector that
/// re-probes a slow drift-hold on a heartbeat cadence, a fleet-wide
/// audit tool that grovels through every Process's persisted Flux
/// refs on a nightly cron) inherits the composer mechanically — no
/// per-callsite 5-slot splat, no risk of a copy-paste that swapped
/// the two adjacent `&str` slots the raw `ssapply::fetch` still
/// accepts on its untyped axis for non-`FluxResourceRef` fetches
/// (`RenderedResourceCoords`-driven `phase_machine` fetches, the
/// direct closed-set-variant `boundary` fetches now on
/// [`fetch_by_identity`]).
///
/// Theory grounding: THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — the 4-slot fetch tuple + the diagnostic label
/// bind structurally through [`FluxResourceRef::fetch_coords`] +
/// [`flux_ref_fetch_error_context`], so a regression that drifted the
/// slot order or the wording at ONE site would fail-loudly at the two
/// sibling pins rather than as silent operator-facing wire-form skew
/// at every downstream Flux fetch consumer). THEORY.md §VI.1
/// (generation over composition — the 5-slot splat + wrap
/// incantation recurred at two hand-authored sites past the ≥ 2
/// duplication trigger, and is lifted to ONE composer here).
pub async fn fetch_flux_ref(client: Client, r: &FluxResourceRef) -> Result<Option<DynamicObject>> {
    let (namespace, api_version, kind, name) = r.fetch_coords();
    fetch(client, namespace, api_version, kind, name)
        .await
        .map_err(|e| anyhow!("{}: {e}", flux_ref_fetch_error_context(r)))
}

/// Parsed readiness state of a resource's `status.conditions[type=Ready]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadyState {
    Ready,
    NotReady(Option<String>),
    Unknown,
}

/// Extract `status.conditions[type=Ready]` from a DynamicObject.
pub fn ready_condition(obj: &DynamicObject) -> ReadyState {
    ready_condition_value(&obj.data)
}

/// Same extraction but operating on raw JSON — testable without a cluster.
pub fn ready_condition_value(data: &Value) -> ReadyState {
    let conditions = data
        .get("status")
        .and_then(|s| s.get("conditions"))
        .and_then(|c| c.as_array());
    let Some(conditions) = conditions else {
        return ReadyState::Unknown;
    };
    for c in conditions {
        let Some(typ) = c.get("type").and_then(|v| v.as_str()) else {
            continue;
        };
        if typ != "Ready" {
            continue;
        }
        return match c.get("status").and_then(|v| v.as_str()) {
            Some("True") => ReadyState::Ready,
            Some("False") => {
                ReadyState::NotReady(c.get("message").and_then(|v| v.as_str()).map(String::from))
            }
            _ => ReadyState::Unknown,
        };
    }
    ReadyState::Unknown
}

fn build_owner_reference(p: &Process) -> Result<Value> {
    let name = p
        .metadata
        .name
        .clone()
        .ok_or_else(|| anyhow!("process missing metadata.name"))?;
    let uid = p
        .metadata
        .uid
        .clone()
        .ok_or_else(|| anyhow!("process missing metadata.uid"))?;
    Ok(tatara_process::owner_reference_json(&name, &uid))
}

/// Shared `resource → &mut resource.metadata` walk — the ONE substrate
/// primitive owning the three-line get-or-create-and-type-check chain
/// every SSA-time mutation of a rendered K8s resource's `metadata`
/// block needs.
///
/// Pre-lift the walk was open-coded at TWO adjacent private helpers in
/// this module past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold:
/// * [`inject_owner_reference`] — seeds `metadata.ownerReferences`
///   before pushing the reconciler's `Process` owner reference.
/// * [`inject_annotations`] — seeds `metadata.annotations` before
///   inserting the standard 2-slot ownership tag +
///   PID / CONTENT_HASH / GENERATION / ATTESTATION_ROOT.
///
/// Both restated the same three-step chain verbatim: (a) `.as_object_mut()
/// .ok_or_else(|| anyhow!("resource is not an object"))?` on the root
/// (bare `Value` → `serde_json::Map` guard), (b) `.entry("metadata")
/// .or_insert_with(|| Value::Object(Default::default()))` for the
/// get-or-create step (a rendered resource without `metadata` is legal
/// and gets seeded rather than errored — matches how kubectl treats
/// bare pod manifests), (c) `.as_object_mut().ok_or_else(|| anyhow!(
/// "metadata is not an object"))?` on the intermediate `Value`
/// (metadata slot mistyped as an array / string surfaces as an error
/// rather than a silent `.as_object_mut() → None → skip` no-op).
///
/// Private to the module: no external consumer of `ssapply` writes
/// into `metadata` directly — the two callers threading through this
/// owner do so as part of the `apply_owned` prepare-and-SSA cascade,
/// and every other `metadata`-mutating shape in the reconciler goes
/// through a typed builder ([`crate::render`] emits fresh `json!({
/// "metadata": {...} })` blocks; [`crate::patch::finalizers_metadata_patch`]
/// builds a merge-patch body around a `metadata.finalizers` slot; the
/// `render.rs:379` encapsulation-annotation helper uses a
/// best-effort `.get_mut("metadata")` walk that silently no-ops on a
/// missing block, matching its "attach only if present" contract —
/// a different byte-shape not a lift candidate).
///
/// A future addition (a `metadata.labels` sibling seed, a
/// `metadata.finalizers` sibling seed, a normalization step inserted
/// between the root-guard and the metadata-slot get-or-create, an
/// error-envelope carrying the resource coordinates for observability)
/// lands at this ONE substrate function and both downstream `inject_*`
/// helpers inherit the upgrade mechanically — no per-site hand-edit at
/// `inject_owner_reference` / `inject_annotations`.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// three-step `resource → &mut metadata` walk recurred at two adjacent
/// hand-authored sites past the PRIME-DIRECTIVE ≥ 2 duplication
/// trigger, and is lifted to ONE owner here). THEORY.md §II.1
/// invariant 5 (composition preserves proofs — a regression that
/// drifted the error message wording, silently swallowed a mistyped
/// metadata slot, or reshaped the get-or-create default surfaces at
/// [`tests::metadata_object_mut_seeds_metadata_when_absent`] /
/// [`tests::metadata_object_mut_errors_on_non_object_resource`] /
/// [`tests::metadata_object_mut_errors_on_non_object_metadata`]
/// rather than as silent drift across both downstream `inject_*`
/// callsites simultaneously).
fn metadata_object_mut(resource: &mut Value) -> Result<&mut serde_json::Map<String, Value>> {
    let root = resource
        .as_object_mut()
        .ok_or_else(|| anyhow!("resource is not an object"))?;
    let metadata = root
        .entry("metadata")
        .or_insert_with(|| Value::Object(Default::default()));
    metadata
        .as_object_mut()
        .ok_or_else(|| anyhow!("metadata is not an object"))
}

fn inject_owner_reference(resource: &mut Value, owner_ref: Value) -> Result<()> {
    let md = metadata_object_mut(resource)?;
    let refs = md
        .entry("ownerReferences")
        .or_insert_with(|| Value::Array(vec![]));
    if let Value::Array(arr) = refs {
        arr.push(owner_ref);
    }
    Ok(())
}

fn inject_annotations(resource: &mut Value, process: &Process) -> Result<()> {
    let md = metadata_object_mut(resource)?;
    let annot = md
        .entry("annotations")
        .or_insert_with(|| Value::Object(Default::default()));
    let annot = annot
        .as_object_mut()
        .ok_or_else(|| anyhow!("annotations is not an object"))?;

    // Route the two-slot metadata pull through the substrate primitive
    // on `Process` — the pre-lift hand-authored `.metadata.namespace
    // .as_deref().unwrap_or("default")` +
    // `.metadata.name.as_deref().unwrap_or("unnamed")` incantations
    // now share ONE fallback owner with the render owner-metadata
    // seed (render::render), claim-arbiter row builder
    // (table_controller::reconcile), and boundary-evaluator default-
    // namespace resolver (boundary::evaluate / check_depends_on).
    let (ns, name) = process.coordinates_or_defaults();
    // Seed the standard 2-slot ownership tag through the shared
    // (ns, name) → annotations composer so the SSA-time re-injection
    // uses the exact same key pair + FIELD_MANAGER value the
    // render-time authoring sites do (three render.rs sites now
    // peer through this same composed primitive) — a rename of
    // FIELD_MANAGER, a new mandatory tag added to
    // `ownership_annotations`, or a normalization inserted between
    // `qualified_process_ref` and `ownership_annotations` propagates
    // here mechanically.
    for (k, v) in ownership_annotations_by_coord(ns, name) {
        annot.insert(k, v);
    }

    // Substrate observed-* primitive family (pid + identity +
    // attestation) drops the outer `if let Some(status) =
    // &process.status { … }` destructure that used to nest THREE
    // sibling `.status.<slot>` checks — each per-axis primitive now
    // owns the missing-`status` corner on its own, so the composer
    // reads as one call per axis with no shared destructure. Peer
    // to the ONE-owner routing in `phase_machine::handle_forking`
    // (which reads the identity through the same primitive to seed
    // the FORK-time `derive_identity` fallback).
    if let Some(pid) = process.observed_pid() {
        annot.insert(annotations::PID.to_string(), Value::String(pid.to_string()));
    }
    if let Some(id) = process.observed_identity() {
        annot.insert(
            annotations::CONTENT_HASH.to_string(),
            Value::String(id.content_hash.clone()),
        );
    }
    if let Some(a) = process.observed_attestation() {
        annot.insert(
            annotations::GENERATION.to_string(),
            Value::String(a.generation.to_string()),
        );
        annot.insert(
            annotations::ATTESTATION_ROOT.to_string(),
            Value::String(a.composed_root.clone()),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals_resolve() {
        assert_eq!(plural_of("Kustomization").unwrap(), "kustomizations");
        assert_eq!(plural_of("HelmRelease").unwrap(), "helmreleases");
        assert_eq!(plural_of("GitRepository").unwrap(), "gitrepositories");
        assert!(plural_of("Nonsense").is_err());
    }

    #[test]
    fn api_resource_splits_group_and_version() {
        let ar = api_resource("kustomize.toolkit.fluxcd.io/v1", "Kustomization").unwrap();
        assert_eq!(ar.group, "kustomize.toolkit.fluxcd.io");
        assert_eq!(ar.version, "v1");
        assert_eq!(ar.plural, "kustomizations");
    }

    #[test]
    fn api_resource_handles_core_v1() {
        let ar = api_resource("v1", "ConfigMap").unwrap();
        assert_eq!(ar.group, "");
        assert_eq!(ar.version, "v1");
        assert_eq!(ar.plural, "configmaps");
    }

    // ─── apply_patch_params substrate pins ─────────────────────────
    //
    // The 2-link `PatchParams::apply(<mgr>).force()` chain now rides
    // through the ONE substrate primitive [`apply_patch_params`].
    // Pre-lift the chain appeared at THREE hand-authored sites past
    // the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold, and TWO of
    // the three silently bypassed the [`FIELD_MANAGER`] const with a
    // bare `"tatara-reconciler"` literal — a latent rename-drift
    // defect (`ssapply::apply_owned` read the const; `phase_machine::
    // begin_releasing` and `table_controller::reconcile` did NOT).
    // These pins bind the primitive at fail-before-pass-after
    // granularity so a regression that reintroduces the literal,
    // drops the `force = true` slot, or drifts the SSA byte-shape
    // away from the pre-lift chain surfaces HERE rather than as
    // silent field-manager ownership skew across the three SSA
    // writers.

    #[test]
    fn apply_patch_params_binds_field_manager_const_verbatim() {
        // The primitive reads through [`FIELD_MANAGER`] rather than
        // restating a hand-authored literal. A regression that
        // reverted to a literal (or renamed the const without
        // updating the primitive) surfaces HERE rather than as silent
        // rename drift across the three SSA writers.
        let pp = apply_patch_params();
        assert_eq!(pp.field_manager.as_deref(), Some(FIELD_MANAGER));
        assert_eq!(pp.field_manager.as_deref(), Some("tatara-reconciler"));
    }

    #[test]
    fn apply_patch_params_stamps_force_true() {
        // `force = true` matches the SSA `force` directive every pre-
        // lift chain applied at the reconciler's SSA writer sites —
        // the reconciler is the authoritative owner of the field
        // pathways it stamps and reclaims conflicting slots on every
        // apply. A regression that dropped `.force()` from the
        // primitive would silently 409-conflict at every SSA write.
        let pp = apply_patch_params();
        assert!(pp.force);
    }

    #[test]
    fn apply_patch_params_defaults_dry_run_and_field_validation_off() {
        // The primitive stamps ONLY the field-manager + force slots
        // pre-lift chains stamped — `dry_run` stays `false` and
        // `field_validation` stays `None`. A regression that widened
        // the primitive's slot set (e.g. auto-enabled `dry_run`
        // during a debug pass) would silently no-op every SSA write.
        let pp = apply_patch_params();
        assert!(!pp.dry_run);
        assert!(pp.field_validation.is_none());
    }

    #[test]
    fn apply_patch_params_matches_pre_lift_hand_authored_chain_bytewise() {
        // Byte-shape parity with the pre-lift 2-link chain: the four
        // observable slots (`field_manager`, `force`, `dry_run`,
        // `field_validation`) match `PatchParams::apply(FIELD_MANAGER)
        // .force()` at every slot. A regression that reordered the
        // chain (e.g. `apply(...).dry_run().force()` swap) or drifted
        // any slot's wire representation lands HERE.
        let pre_lift = PatchParams::apply(FIELD_MANAGER).force();
        let lifted = apply_patch_params();
        assert_eq!(lifted.field_manager, pre_lift.field_manager);
        assert_eq!(lifted.force, pre_lift.force);
        assert_eq!(lifted.dry_run, pre_lift.dry_run);
        assert_eq!(
            lifted.field_validation.is_none(),
            pre_lift.field_validation.is_none()
        );
    }

    #[test]
    fn apply_patch_params_matches_pre_lift_hardcoded_literal_bytewise() {
        // The two hardcoded-literal pre-lift sites (`phase_machine::
        // begin_releasing`, `table_controller::reconcile`) spelled
        // the field manager as `"tatara-reconciler"` verbatim. Post-
        // lift both route through `apply_patch_params()`, which reads
        // the const. This pin proves the primitive is byte-identical
        // to the pre-lift literal AT THE CURRENT `FIELD_MANAGER`
        // value — so a rename of the const propagates through both
        // sites mechanically (the primitive's field-manager slot
        // changes with the const) rather than leaving them at the
        // stale literal.
        let hardcoded = PatchParams::apply("tatara-reconciler").force();
        let lifted = apply_patch_params();
        assert_eq!(lifted.field_manager, hardcoded.field_manager);
        assert_eq!(lifted.force, hardcoded.force);
    }

    // ─── fetch_by_identity / api_resource_of substrate pins ────────
    //
    // The typed [`K8sWireIdentity`] pair now gates every fetch of a
    // closed-set-variant K8s resource. Pre-lift the raw two-adjacent-
    // `&str` `fetch(av, kind, ...)` signature was called at THREE
    // hand-authored sites in `tatara-reconciler::boundary` past the
    // ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold; post-lift each
    // site names the variant ONCE via `.wire_identity()` and rides
    // through `fetch_by_identity`. These pins bind the primitive at
    // fail-before-pass-after granularity so a regression that skewed
    // the identity's slots against the composed ApiResource, dropped
    // an axis in the composer, or diverged the identity-gated
    // projection from the raw two-`&str` composer surfaces HERE
    // rather than as an operator-visible wire-form skew at every
    // downstream fetch site.

    #[test]
    fn api_resource_of_composes_group_and_version_from_a_flux_identity() {
        // Typed identity → same `(group, version, plural)` triple as
        // the raw two-`&str` `api_resource` call for the SAME variant.
        // A regression that dropped a slot, swapped the two axes at
        // the composer, or drifted the identity's projection away
        // from the raw path would surface here rather than as an
        // operator-visible SSA-fetch skew at every closed-set-
        // variant-gated fetch site.
        use tatara_process::flux_resource::FluxResource;
        let ar = api_resource_of(FluxResource::Kustomization.wire_identity()).unwrap();
        assert_eq!(ar.group, "kustomize.toolkit.fluxcd.io");
        assert_eq!(ar.version, "v1");
        assert_eq!(ar.plural, "kustomizations");
        assert_eq!(ar.kind, "Kustomization");
        assert_eq!(ar.api_version, "kustomize.toolkit.fluxcd.io/v1");
    }

    #[test]
    fn api_resource_of_handles_core_v1_via_a_k8s_builtin_identity() {
        // Core/v1 has no group prefix — the identity carries the bare
        // `v1` on its `api_version` slot; the composer must decode
        // that as `(group: "", version: "v1")` identically to the raw
        // `api_resource("v1", ...)` path. A regression that special-
        // cased one path but not the other would surface here.
        use tatara_process::k8s_builtin_resource::K8sBuiltinResource;
        let ar = api_resource_of(K8sBuiltinResource::ConfigMap.wire_identity()).unwrap();
        assert_eq!(ar.group, "");
        assert_eq!(ar.version, "v1");
        assert_eq!(ar.plural, "configmaps");
        assert_eq!(ar.kind, "ConfigMap");
        assert_eq!(ar.api_version, "v1");
    }

    #[test]
    fn api_resource_of_matches_raw_api_resource_across_every_flux_resource_variant() {
        // Cross-path coherence pin: for EVERY variant on the
        // [`FluxResource`] closed set, `api_resource_of(v.wire_identity())`
        // returns the SAME `(group, version, api_version, kind,
        // plural)` triple as `api_resource(v.api_version(),
        // v.kind())`. A regression that diverged the identity-gated
        // path from the raw path at ONE arm (a stale-cached
        // ApiResource on the identity side, a normalization step on
        // one side only) would surface here rather than as a silent
        // SSA-fetch skew at every Flux-fetching consumer.
        use tatara_process::flux_resource::FluxResource;
        for v in FluxResource::ALL {
            let id_ar = api_resource_of(v.wire_identity()).unwrap();
            let raw_ar = api_resource(v.api_version(), v.kind()).unwrap();
            assert_eq!(id_ar.group, raw_ar.group, "group skew at {v:?}");
            assert_eq!(id_ar.version, raw_ar.version, "version skew at {v:?}");
            assert_eq!(
                id_ar.api_version, raw_ar.api_version,
                "api_version skew at {v:?}"
            );
            assert_eq!(id_ar.kind, raw_ar.kind, "kind skew at {v:?}");
            assert_eq!(id_ar.plural, raw_ar.plural, "plural skew at {v:?}");
        }
    }

    #[test]
    fn api_resource_of_matches_raw_api_resource_across_every_k8s_builtin_variant() {
        // Cross-path + cross-substrate coherence pin: peer to the
        // FluxResource pin above but on the K8s-builtin closed set.
        // Every consumer that fetches a K8s-builtin resource
        // (`boundary::fetch_job_status`, `boundary::verify_receipt_cm`,
        // any future kenshi-runner / per-membro receipt fetcher)
        // routes through the SAME identity-gated composer, so the
        // triple parity binds the invariant "closed-set-gated fetch
        // is byte-identical to the raw two-`&str` fetch" for both
        // axes.
        use tatara_process::k8s_builtin_resource::K8sBuiltinResource;
        for v in K8sBuiltinResource::ALL {
            let id_ar = api_resource_of(v.wire_identity()).unwrap();
            let raw_ar = api_resource(v.api_version(), v.kind()).unwrap();
            assert_eq!(id_ar.group, raw_ar.group, "group skew at {v:?}");
            assert_eq!(id_ar.version, raw_ar.version, "version skew at {v:?}");
            assert_eq!(
                id_ar.api_version, raw_ar.api_version,
                "api_version skew at {v:?}"
            );
            assert_eq!(id_ar.kind, raw_ar.kind, "kind skew at {v:?}");
            assert_eq!(id_ar.plural, raw_ar.plural, "plural skew at {v:?}");
        }
    }

    #[test]
    fn api_resource_of_propagates_error_when_the_identity_names_an_unknown_kind() {
        // A caller that constructs a raw identity with an unregistered
        // kind (`plural_of` returns Err) sees the same error as the
        // raw two-`&str` path. Pins the "identity-gated composer is a
        // pure delegation, no error-swallowing" invariant.
        let bogus = K8sWireIdentity::new("bogus.io/v1", "Nonsense");
        assert!(api_resource_of(bogus).is_err());
        assert!(api_resource("bogus.io/v1", "Nonsense").is_err());
    }

    #[test]
    fn api_resource_of_binds_positional_slots_symmetrically_with_raw_composer() {
        // Positional-slot coherence pin: an identity whose two slots
        // are set through [`K8sWireIdentity::new`] positional
        // constructor binds identically to the raw
        // `api_resource(av, kind)` positional path — verified by
        // asserting the composed ApiResource's `(kind, api_version)`
        // pair reads back the SAME two slots the identity constructor
        // received. A regression that swapped the identity's slot
        // reads inside `api_resource_of` (reading `kind` in place of
        // `api_version` or vice versa) would fail-loudly here rather
        // than as a silent SSA-fetch skew at every consumer.
        let id = K8sWireIdentity::new("helm.toolkit.fluxcd.io/v2", "HelmRelease");
        let ar = api_resource_of(id).unwrap();
        assert_eq!(ar.api_version, "helm.toolkit.fluxcd.io/v2");
        assert_eq!(ar.kind, "HelmRelease");
        assert_eq!(ar.group, "helm.toolkit.fluxcd.io");
        assert_eq!(ar.version, "v2");
    }

    #[test]
    fn ready_condition_true() {
        let data = json!({
            "status": { "conditions": [
                { "type": "Ready", "status": "True" }
            ]}
        });
        assert_eq!(ready_condition_value(&data), ReadyState::Ready);
    }

    #[test]
    fn ready_condition_false_with_message() {
        let data = json!({
            "status": { "conditions": [
                { "type": "Ready", "status": "False", "message": "pull failed" }
            ]}
        });
        assert_eq!(
            ready_condition_value(&data),
            ReadyState::NotReady(Some("pull failed".to_string()))
        );
    }

    #[test]
    fn ready_condition_missing_is_unknown() {
        let data = json!({ "status": { "conditions": [] } });
        assert_eq!(ready_condition_value(&data), ReadyState::Unknown);
        let data = json!({});
        assert_eq!(ready_condition_value(&data), ReadyState::Unknown);
    }

    #[test]
    fn inject_owner_reference_adds_entry() {
        let mut obj = json!({
            "apiVersion": "v1", "kind": "ConfigMap",
            "metadata": { "name": "x" },
        });
        inject_owner_reference(
            &mut obj,
            json!({ "apiVersion": "tatara.pleme.io/v1alpha1", "kind": "Process", "name": "p", "uid": "u" }),
        )
        .unwrap();
        let refs = obj["metadata"]["ownerReferences"].as_array().unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0]["kind"], "Process");
    }

    // ─── metadata_object_mut substrate pins ─────────────────────────────
    //
    // The three-step `resource → &mut metadata` walk recurred at two
    // adjacent private helpers (`inject_owner_reference`,
    // `inject_annotations`) at the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // threshold before this primitive existed. Both now delegate
    // through the shared owner. These pins bind the primitive at
    // fail-before-pass-after granularity so a regression that drifted
    // the get-or-create default, silently swallowed a mistyped
    // metadata slot, or reworded an error surfaces HERE rather than
    // as silent drift at both downstream `inject_*` callsites.

    #[test]
    fn metadata_object_mut_returns_existing_metadata_object() {
        // Happy path: a resource with an existing `metadata` block
        // yields a mutable reference to that same block — a caller's
        // subsequent `.insert(...)` writes through to the resource's
        // metadata verbatim rather than into a fresh sibling map.
        let mut resource = json!({
            "apiVersion": "v1", "kind": "ConfigMap",
            "metadata": { "name": "x" },
        });
        let md = metadata_object_mut(&mut resource).expect("existing object metadata");
        md.insert(
            "namespace".to_string(),
            Value::String("demo-ns".to_string()),
        );
        assert_eq!(resource["metadata"]["name"], "x");
        assert_eq!(resource["metadata"]["namespace"], "demo-ns");
    }

    #[test]
    fn metadata_object_mut_seeds_metadata_when_absent() {
        // A rendered resource without a `metadata` block is legal
        // (bare pod manifests). The primitive get-or-creates rather
        // than errors — mirroring how `kubectl apply` treats a
        // metadata-less bare pod. A subsequent caller-side `.insert`
        // must persist through to the resource.
        let mut resource = json!({ "apiVersion": "v1", "kind": "ConfigMap" });
        let md = metadata_object_mut(&mut resource).expect("seeded metadata is an object");
        assert!(md.is_empty(), "freshly seeded metadata is an empty map");
        md.insert("name".to_string(), Value::String("x".to_string()));
        assert_eq!(resource["metadata"]["name"], "x");
    }

    #[test]
    fn metadata_object_mut_errors_on_non_object_resource() {
        // The root Value must be a JSON object — the reconciler's
        // render surfaces emit `json!({ ... })` blocks so this
        // holds by construction upstream, but a regression that
        // handed the primitive a bare array / string / number
        // surfaces the error at the primitive rather than as a
        // silent `.as_object_mut() → None → skip` no-op at the
        // downstream callsite. Sweep the three non-object primitive
        // shapes a caller might plausibly hand through.
        for mut bad in [
            json!([{"kind": "not an object"}]),
            json!("just a string"),
            json!(42),
        ] {
            let err = metadata_object_mut(&mut bad).expect_err("non-object root must error");
            assert!(
                err.to_string().contains("resource is not an object"),
                "error message must name the resource axis for grep-ability, got {err:?}"
            );
        }
    }

    #[test]
    fn metadata_object_mut_errors_on_non_object_metadata() {
        // Pre-existing `metadata` slot mistyped as an array / string
        // (common author mistake: `metadata: [ name: x ]` YAML that
        // parses as a list) surfaces as an error rather than as a
        // silent skip that would drop the caller's write. Sweep the
        // three non-object shapes a mistyped metadata slot might
        // plausibly carry.
        for bad in [json!([]), json!("oops"), json!(7)] {
            let mut resource = json!({ "apiVersion": "v1", "kind": "ConfigMap", "metadata": bad });
            let err =
                metadata_object_mut(&mut resource).expect_err("non-object metadata must error");
            assert!(
                err.to_string().contains("metadata is not an object"),
                "error message must name the metadata axis for grep-ability, got {err:?}"
            );
        }
    }

    #[test]
    fn metadata_object_mut_shares_body_across_inject_owner_and_inject_annotations() {
        // Sibling to the delegation pins on `ownership_*`: prove that
        // both `inject_*` helpers now route through the same
        // primitive by exercising a metadata-less resource through
        // BOTH helpers and confirming each produces its expected
        // downstream slot without either silently dropping the
        // metadata seed. Pre-lift each helper open-coded its own
        // walk; a regression that re-open-coded one of the two —
        // silently un-lifting the shared owner — would show up
        // here as one helper failing to produce its slot on a
        // metadata-less resource.
        let mut a = json!({ "apiVersion": "v1", "kind": "ConfigMap" });
        inject_owner_reference(&mut a, json!({ "kind": "Process", "name": "p" })).unwrap();
        assert!(
            a["metadata"]["ownerReferences"].is_array(),
            "inject_owner_reference must seed metadata on a metadata-less resource"
        );

        let process: Process = serde_json::from_value(json!({
            "apiVersion": "tatara.pleme.io/v1alpha1",
            "kind": "Process",
            "metadata": { "name": "demo-app", "namespace": "demo-ns" },
            "spec": {
                "identity": {},
                "classification": { "pointType": "Gate", "substrate": "Compute" },
                "intent": { "flux": { "path": "./", "gitRepository": "flux-system" } },
                "boundary": {},
                "compliance": {},
                "signals": {},
                "lifetime": { "permanent": {} },
                "suspended": false,
            },
        }))
        .expect("Process deserialises from fixture JSON");
        let mut b = json!({ "apiVersion": "v1", "kind": "ConfigMap" });
        inject_annotations(&mut b, &process).unwrap();
        assert!(
            b["metadata"]["annotations"].is_object(),
            "inject_annotations must seed metadata on a metadata-less resource"
        );
    }

    // ─── ownership_kv_pair shared-owner pins ────────────────────────────
    //
    // The two axis-typed public primitives — `ownership_annotations`
    // (annotations axis) and `ownership_labels` (labels axis) — now
    // delegate to the private shared owner `ownership_kv_pair` rather
    // than open-code the 2-slot map body twice. These pins bind the
    // delegation at fail-before-pass-after granularity so a regression
    // that re-open-coded the body at one of the two public wrappers
    // (silently un-lifting the shared owner and re-introducing the
    // possibility of one axis drifting from the other) surfaces here
    // rather than as a subtle divergence between the two axes at every
    // downstream emit site.

    #[test]
    fn ownership_kv_pair_produces_2_slot_map_with_field_manager() {
        // Pin the shared owner's shipped shape directly — a regression
        // that added a third slot, dropped one, or hard-coded the
        // MANAGED_BY value back to a literal surfaces HERE rather
        // than as silent drift at both axis-typed public wrappers
        // simultaneously.
        let m = ownership_kv_pair("demo-ns/demo-app");
        assert_eq!(m.len(), 2);
        assert_eq!(
            m.get(annotations::MANAGED_BY).and_then(Value::as_str),
            Some(FIELD_MANAGER),
            "MANAGED_BY slot must carry FIELD_MANAGER"
        );
        assert_eq!(
            m.get(annotations::PROCESS).and_then(Value::as_str),
            Some("demo-ns/demo-app"),
            "PROCESS slot must ride the caller-supplied process_ref verbatim"
        );
    }

    #[test]
    fn ownership_annotations_delegates_through_shared_kv_pair() {
        // Pin the delegation: the annotations-axis public wrapper
        // returns byte-identically what the shared owner returns.
        // A regression that re-open-coded the body at
        // `ownership_annotations` (e.g. seeded a third slot inline
        // before the return, restoring the pre-lift duplication)
        // surfaces HERE rather than as a labels/annotations divergence
        // downstream.
        for input in [
            "flux-system/observability-stack",
            "just-a-name",
            "",
            "ns/name@42",
            "with spaces and / slashes",
        ] {
            assert_eq!(
                ownership_annotations(input),
                ownership_kv_pair(input),
                "ownership_annotations must delegate to ownership_kv_pair verbatim for {input:?}"
            );
        }
    }

    #[test]
    fn ownership_labels_delegates_through_shared_kv_pair() {
        // Peer to the annotations-axis delegation pin: the labels-axis
        // public wrapper returns byte-identically what the shared
        // owner returns. Ensures the two public wrappers can never
        // drift under the shared-owner design because both wrappers
        // are proven to route through the same body.
        for input in [
            "flux-system/observability-stack",
            "just-a-name",
            "",
            "ns/name@42",
            "with spaces and / slashes",
        ] {
            assert_eq!(
                ownership_labels(input),
                ownership_kv_pair(input),
                "ownership_labels must delegate to ownership_kv_pair verbatim for {input:?}"
            );
        }
    }

    #[test]
    fn ownership_annotations_and_labels_share_body_by_construction() {
        // Sharper than the pre-existing single-input
        // `ownership_labels_pair_matches_annotations_pair` pin: sweep
        // a range of `process_ref` shapes and prove the two public
        // wrappers produce byte-identical maps on every one. Post-lift
        // the invariant "labels-axis body == annotations-axis body"
        // holds by CONSTRUCTION (both delegate to
        // `ownership_kv_pair`), not by two open-coded copies staying
        // in sync — the pre-existing pin still holds, and this
        // extended sweep confirms the construction across the full
        // input-shape sweep that
        // `ownership_annotations_rides_arbitrary_process_ref_shapes`
        // + `ownership_labels_rides_arbitrary_process_ref_shapes`
        // already cover on their respective single axes.
        for input in [
            "flux-system/observability-stack",
            "just-a-name",
            "",
            "ns/name@42",
            "with spaces and / slashes",
        ] {
            assert_eq!(
                ownership_annotations(input),
                ownership_labels(input),
                "labels and annotations axes must produce byte-identical maps for {input:?}"
            );
        }
    }

    // ─── ownership_annotations substrate pins ───────────────────────────
    //
    // The 2-slot `{MANAGED_BY: FIELD_MANAGER, PROCESS: process_ref}`
    // shape recurred at five hand-authored sites (three in
    // `render.rs` + two in `edges.rs`) before this primitive existed,
    // each hand-coding the `"tatara-reconciler"` literal on the
    // MANAGED_BY slot in addition to the shape. These pins bind the
    // primitive at fail-before-pass-after granularity so a regression
    // that drifted a key, swapped the MANAGED_BY value back to a
    // literal, or reordered the slots surfaces here rather than as
    // silent drift at every emitted resource's ownership tag.

    #[test]
    fn ownership_annotations_produces_field_manager_and_process_ref() {
        let m = ownership_annotations("demo-ns/demo-app");
        // Exactly two keys — no accidental extras (a regression that
        // seeded PID / CONTENT_HASH into the primitive would fail
        // here rather than pollute every render/edges callsite).
        assert_eq!(m.len(), 2);
        // MANAGED_BY reads FIELD_MANAGER, NOT the literal
        // `"tatara-reconciler"` string the pre-lift sites hard-coded.
        // A future rename of FIELD_MANAGER now propagates through
        // this primitive to every downstream emit site by
        // construction; a regression that re-hard-coded the literal
        // fails this assertion.
        assert_eq!(
            m.get(annotations::MANAGED_BY).and_then(Value::as_str),
            Some(FIELD_MANAGER),
            "MANAGED_BY slot must carry FIELD_MANAGER, not a hand-authored literal"
        );
        assert_eq!(
            m.get(annotations::PROCESS).and_then(Value::as_str),
            Some("demo-ns/demo-app"),
            "PROCESS slot must ride the caller-supplied process_ref verbatim"
        );
    }

    #[test]
    fn ownership_annotations_rides_arbitrary_process_ref_shapes() {
        // The reconciler shapes `process_ref` as `<ns>/<name>` at the
        // render.rs sites (via `format!("{ns}/{name}")`) and passes
        // pre-composed `ctx.process_ref` at the edges.rs sites. The
        // primitive treats the input as opaque — a caller-composed
        // reference (e.g. a future `<ns>/<name>@<generation>` shape
        // for multi-generation attestation grepping) rides through
        // unchanged, and the empty-string edge case (unnamed process
        // pre-metadata) does not panic.
        for input in [
            "flux-system/observability-stack",
            "just-a-name",
            "",
            "ns/name@42",
            "with spaces and / slashes",
        ] {
            let m = ownership_annotations(input);
            assert_eq!(
                m.get(annotations::PROCESS).and_then(Value::as_str),
                Some(input),
                "PROCESS slot must ride {input:?} verbatim"
            );
        }
    }

    #[test]
    fn ownership_annotations_interpolates_cleanly_through_json_macro() {
        // The three render.rs sites interpolate the primitive under an
        // `"annotations"` key inside a `json!({...})` block. Pin the
        // interop shape so a regression that swapped the return type
        // from `Map` to a `Value` variant that stops interpolating
        // as an object (e.g. `Value::Array`) surfaces here rather
        // than as a broken `metadata.annotations` on every emitted
        // Kustomization / OCIRepository / HelmRelease.
        let m = ownership_annotations("demo/ephemeral-demo");
        let wrapped = json!({
            "metadata": {
                "name": "ephemeral-demo",
                "namespace": "demo",
                "annotations": m.clone(),
            },
        });
        let anns = &wrapped["metadata"]["annotations"];
        assert!(anns.is_object(), "annotations must land as a JSON object");
        assert_eq!(anns[annotations::MANAGED_BY], FIELD_MANAGER);
        assert_eq!(anns[annotations::PROCESS], "demo/ephemeral-demo");
        // And the raw Map serialisation is byte-identical to the
        // interpolated form — no reshaping happens through the
        // macro boundary.
        assert_eq!(serde_json::Value::Object(m), *anns);
    }

    #[test]
    fn inject_annotations_delegates_through_ownership_primitive() {
        // `inject_annotations` seeds its annotation carrier through
        // `ownership_annotations` before extending with PID /
        // CONTENT_HASH / GENERATION / ATTESTATION_ROOT. Pin that the
        // SSA-time re-injection produces the SAME 2-slot ownership
        // pair the render-time authoring does — pre-existing
        // operator-facing keys don't change wording under the lift.
        //
        // Construct a Process via serde_json so the test doesn't need
        // to reproduce the full ProcessSpec builder scaffold from
        // `claim.rs`'s `empty_process` helper. Only metadata.name +
        // metadata.namespace matter for `inject_annotations`'s
        // seed-time behavior; `status` is `None` so no
        // PID / CONTENT_HASH keys land and only the seed keys are
        // asserted.
        let process: Process = serde_json::from_value(json!({
            "apiVersion": "tatara.pleme.io/v1alpha1",
            "kind": "Process",
            "metadata": { "name": "demo-app", "namespace": "demo-ns" },
            "spec": {
                "identity": {},
                "classification": {
                    "pointType": "Gate",
                    "substrate": "Compute",
                },
                "intent": { "flux": {
                    "path": "./",
                    "gitRepository": "flux-system",
                }},
                "boundary": {},
                "compliance": {},
                "signals": {},
                "lifetime": { "permanent": {} },
                "suspended": false,
            },
        }))
        .expect("Process deserialises from fixture JSON");
        let mut resource = json!({
            "apiVersion": "v1", "kind": "ConfigMap",
            "metadata": { "name": "x" },
        });
        inject_annotations(&mut resource, &process).unwrap();
        let anns = &resource["metadata"]["annotations"];
        assert_eq!(anns[annotations::MANAGED_BY], FIELD_MANAGER);
        assert_eq!(anns[annotations::PROCESS], "demo-ns/demo-app");
    }

    // ─── ownership_labels substrate pins ────────────────────────────────
    //
    // The 2-slot `{MANAGED_BY: FIELD_MANAGER, PROCESS: process_ref}`
    // labels shape recurred at three hand-authored sites
    // (edges::IngressEdge, edges::DnsEndpointEdge, render::one_export_job)
    // before this primitive existed, each hand-coding the
    // `"tatara-reconciler"` literal on the MANAGED_BY slot in addition
    // to the shape. These pins mirror the sibling annotations-axis
    // pins immediately above so a regression on either axis surfaces
    // at fail-before-pass-after granularity, and additionally cross-
    // check that the two axes carry the IDENTICAL 2-key pair — the
    // invariant every operator relies on when the same resource is
    // grepped via either axis.

    #[test]
    fn ownership_labels_produces_field_manager_and_process_ref() {
        let m = ownership_labels("demo-ns/demo-app");
        // Exactly two keys — no accidental extras (a regression that
        // seeded the pod-selector-only ROLE/EXPORT_INDEX keys into
        // the primitive would fail here rather than pollute every
        // edges/render callsite's labels block).
        assert_eq!(m.len(), 2);
        // MANAGED_BY reads FIELD_MANAGER, NOT the literal
        // `"tatara-reconciler"` string the pre-lift sites hard-coded.
        // A future rename of FIELD_MANAGER now propagates through
        // this primitive to every downstream emit site by
        // construction; a regression that re-hard-coded the literal
        // fails this assertion.
        assert_eq!(
            m.get(annotations::MANAGED_BY).and_then(Value::as_str),
            Some(FIELD_MANAGER),
            "MANAGED_BY slot must carry FIELD_MANAGER, not a hand-authored literal"
        );
        assert_eq!(
            m.get(annotations::PROCESS).and_then(Value::as_str),
            Some("demo-ns/demo-app"),
            "PROCESS slot must ride the caller-supplied process_ref verbatim"
        );
    }

    #[test]
    fn ownership_labels_rides_arbitrary_process_ref_shapes() {
        // Peer to `ownership_annotations_rides_arbitrary_process_ref_shapes`:
        // the primitive treats the input as opaque, so any
        // caller-composed reference shape (`<ns>/<name>`,
        // `<ns>/<name>@<gen>`, bare `<name>`, empty) rides through
        // unchanged into the labels axis with no interpretation.
        for input in [
            "flux-system/observability-stack",
            "just-a-name",
            "",
            "ns/name@42",
            "with spaces and / slashes",
        ] {
            let m = ownership_labels(input);
            assert_eq!(
                m.get(annotations::PROCESS).and_then(Value::as_str),
                Some(input),
                "PROCESS slot must ride {input:?} verbatim"
            );
        }
    }

    #[test]
    fn ownership_labels_interpolates_cleanly_through_json_macro() {
        // The three shipped sites interpolate the primitive under a
        // `"labels"` key inside a `json!({...})` block via
        // `Value::Object(map)`. Pin the interop shape so a regression
        // that swapped the return type from `Map` to a `Value` variant
        // that stops interpolating as an object surfaces here rather
        // than as a broken `metadata.labels` on every emitted Ingress
        // / DNSEndpoint / export Job.
        let m = ownership_labels("demo/ephemeral-demo");
        let wrapped = json!({
            "metadata": {
                "name": "ephemeral-demo",
                "namespace": "demo",
                "labels": Value::Object(m.clone()),
            },
        });
        let labels = &wrapped["metadata"]["labels"];
        assert!(labels.is_object(), "labels must land as a JSON object");
        assert_eq!(labels[annotations::MANAGED_BY], FIELD_MANAGER);
        assert_eq!(labels[annotations::PROCESS], "demo/ephemeral-demo");
        assert_eq!(serde_json::Value::Object(m), *labels);
    }

    // ─── ownership_annotations_by_coord substrate pins ──────────────────
    //
    // The composed `ownership_annotations(&qualified_process_ref(ns,
    // name))` shape was hand-authored at FOUR sites past the ★★
    // PRIME-DIRECTIVE ≥ 2 duplication threshold — three render.rs
    // sites (Kustomization, OCIRepository, HelmRelease
    // `metadata.annotations` seeds) plus the `inject_annotations`
    // SSA-time re-injection here. These pins bind the composed
    // primitive at fail-before-pass-after granularity so a regression
    // that drifted the composition order, re-inlined the nested call
    // chain at one site, or desynced the composed primitive from the
    // direct-primitive-of-composed-ref shape surfaces here rather
    // than as silent operator-facing drift across the four callsites.

    #[test]
    fn ownership_annotations_by_coord_composes_qualified_ref_into_ownership_map() {
        // Happy path: `(ns, name)` composes into the standard 2-slot
        // ownership tag whose PROCESS slot carries the qualified
        // reference `<ns>/<name>` verbatim. Regression that dropped
        // one axis, swapped the two, or short-circuited to a bare
        // name would surface here rather than at every downstream
        // emit site.
        let m = ownership_annotations_by_coord("demo-ns", "ephemeral-demo");
        assert_eq!(m.len(), 2);
        assert_eq!(
            m.get(annotations::MANAGED_BY).and_then(Value::as_str),
            Some(FIELD_MANAGER),
        );
        assert_eq!(
            m.get(annotations::PROCESS).and_then(Value::as_str),
            Some("demo-ns/ephemeral-demo"),
        );
    }

    #[test]
    fn ownership_annotations_by_coord_matches_hand_authored_double_call() {
        // Byte-identical parity with the pre-lift
        // `ownership_annotations(&qualified_process_ref(ns, name))`
        // incantation across a sweep of shapes every callsite
        // plausibly encounters: canonical `<ns>/<name>` shapes, the
        // empty-string cluster-scoped edge case (unnamed process
        // pre-metadata), and the pathological whitespace-embedded
        // pair `qualified_process_ref_rides_edge_case_axis_shapes`
        // already pins on the sibling primitive. A regression that
        // inserted a normalization step at the composed primitive
        // that the direct-primitive-of-composed-ref chain does NOT
        // apply — or vice versa — surfaces here rather than as
        // silent drift between the two callsite postures the
        // reconciler ships today.
        for (ns, name) in [
            ("flux-system", "observability-stack"),
            ("demo-ns", "ephemeral-demo"),
            ("", ""),
            ("default", ""),
            ("", "orphan"),
            ("weird ns", "with/slash"),
        ] {
            let composed = ownership_annotations_by_coord(ns, name);
            let hand_authored = ownership_annotations(&qualified_process_ref(ns, name));
            assert_eq!(
                composed, hand_authored,
                "composed primitive must be byte-identical to the pre-lift double-call chain on {ns:?}/{name:?}"
            );
        }
    }

    #[test]
    fn ownership_annotations_by_coord_agrees_with_direct_primitive_on_precomposed_ref() {
        // Cross-primitive coherence: the composed
        // `ownership_annotations_by_coord(ns, name)` and the direct
        // `ownership_annotations(process_ref)` split the input space
        // by whether the caller has already composed the ref. Pin
        // that a caller with either posture — the render.rs /
        // `inject_annotations` sites that lift through
        // `ownership_annotations_by_coord`, or the [`crate::edges`]
        // sites that thread `ctx.process_ref` directly through
        // `ownership_annotations` — lands at the SAME 2-slot
        // ownership map when the coordinates alias. A future
        // divergence between the two callsite postures (e.g. a
        // normalization applied only to the composed path) surfaces
        // here rather than as silent operator-facing drift between
        // the render/SSA-emitted metadata and the edges-emitted
        // metadata for what is, structurally, the same Process.
        let composed = ownership_annotations_by_coord("demo-ns", "demo-app");
        let direct = ownership_annotations("demo-ns/demo-app");
        assert_eq!(
            composed, direct,
            "composed and direct primitives must agree on the same underlying process_ref",
        );
    }

    // ─── owned_flux_metadata substrate pins ─────────────────────────────

    #[test]
    fn owned_flux_metadata_carries_all_three_slots() {
        // Shape pin: exactly THREE slots (`name`, `namespace`,
        // `annotations`), no accidental fourth-slot leaks that would
        // silently reshape every downstream Flux-owned emit
        // (Kustomization, OCIRepository, HelmRelease). Regression
        // catcher for a future `labels` / `ownerReferences` /
        // `finalizers` / `generateName` slot added at ONE callsite
        // without going through the primitive.
        let meta = owned_flux_metadata("demo-ns", "ephemeral-demo");
        let obj = meta
            .as_object()
            .expect("owned_flux_metadata must return a JSON object");
        assert_eq!(
            obj.len(),
            3,
            "owned_flux_metadata must carry exactly {{name, namespace, annotations}} — got {} slots",
            obj.len()
        );
        assert!(obj.contains_key("name"), "missing `name` slot");
        assert!(obj.contains_key("namespace"), "missing `namespace` slot");
        assert!(
            obj.contains_key("annotations"),
            "missing `annotations` slot"
        );
    }

    #[test]
    fn owned_flux_metadata_stamps_name_and_namespace_verbatim() {
        // The `name` and `namespace` args ride through verbatim. A
        // regression that swapped the two (yielding a 404 on
        // kubectl lookup) or applied a normalization at ONE arg but
        // not the other surfaces here rather than as apply-time
        // "resource not found" noise at every Flux-owned emit.
        let meta = owned_flux_metadata("prod-ns", "seph-agent");
        assert_eq!(
            meta["name"].as_str(),
            Some("seph-agent"),
            "owned_flux_metadata dropped or reshaped the name arg",
        );
        assert_eq!(
            meta["namespace"].as_str(),
            Some("prod-ns"),
            "owned_flux_metadata dropped or reshaped the namespace arg",
        );
    }

    #[test]
    fn owned_flux_metadata_seeds_annotations_through_ownership_annotations_by_coord() {
        // The annotations slot is byte-identical to what
        // `ownership_annotations_by_coord(ns, name)` returns for
        // every (ns, name) pair. A regression that re-open-coded the
        // annotations axis (e.g. bypassing the composer to hand-
        // author a bare `MANAGED_BY: FIELD_MANAGER` map without the
        // `PROCESS: <ns>/<name>` slot the composer threads through
        // `qualified_process_ref`) silently un-lifts the sibling
        // annotations composer, so a future change to
        // `ownership_annotations_by_coord` (a new mandatory tag, a
        // normalization step) would NOT propagate to the three Flux
        // emit sites.
        for (ns, name) in [
            ("flux-system", "observability-stack"),
            ("demo-ns", "ephemeral-demo"),
            ("prod", "seph-agent"),
        ] {
            let meta = owned_flux_metadata(ns, name);
            let expected = ownership_annotations_by_coord(ns, name);
            assert_eq!(
                meta["annotations"],
                Value::Object(expected),
                "owned_flux_metadata drifted from ownership_annotations_by_coord for {ns:?}/{name:?}",
            );
        }
    }

    #[test]
    fn owned_flux_metadata_matches_hand_authored_pre_lift_bytewise() {
        // The exact 3-slot hand-authored composition every pre-lift
        // callsite restated, byte-for-byte, swept across a corpus of
        // shapes every render.rs callsite plausibly encounters:
        // canonical `<ns>/<name>` shapes, the empty-string cluster-
        // scoped edge case (unnamed process pre-metadata), and the
        // pathological embedded-separator pair
        // `qualified_process_ref_rides_edge_case_axis_shapes` already
        // pins on the sibling primitive. A regression that reordered
        // a slot, dropped one, or added a fourth surfaces at THIS
        // pin rather than as a subtle apply-time drift (namespace
        // drop → K8s defaults to `default` and the apply lands in
        // the wrong namespace; annotations drop → SSA-time
        // `inject_annotations` re-injection has no seed to extend
        // and the resulting resource lacks the standard 2-slot
        // ownership tag operators grep by).
        for (ns, name) in [
            ("flux-system", "observability-stack"),
            ("demo-ns", "ephemeral-demo"),
            ("", ""),
            ("default", ""),
            ("", "orphan"),
            ("weird ns", "with/slash"),
        ] {
            let composed = owned_flux_metadata(ns, name);
            let hand_authored = json!({
                "name": name,
                "namespace": ns,
                "annotations": ownership_annotations_by_coord(ns, name),
            });
            assert_eq!(
                composed, hand_authored,
                "owned_flux_metadata drifted from the pre-lift hand-authored 3-slot shape on {ns:?}/{name:?}",
            );
        }
    }

    #[test]
    fn owned_flux_metadata_annotations_carry_managed_by_and_process_slots() {
        // End-to-end contract pin: `owned_flux_metadata` composes
        // through `ownership_annotations_by_coord` which composes
        // through `qualified_process_ref` + `ownership_annotations`
        // — the emitted annotations block MUST carry the 2-slot
        // ownership tag every SSA-time re-injection extends. A
        // regression that short-circuited the composer or dropped
        // one of the underlying primitives from the chain would
        // fail here at the end-to-end contract layer rather than as
        // an apply-time discovery miss (operators grep resources by
        // `PROCESS=<ns>/<name>` — a missing slot silently drops the
        // resource from every ownership discovery sweep).
        let meta = owned_flux_metadata("demo-ns", "ephemeral-demo");
        let anns = &meta["annotations"];
        assert_eq!(
            anns[annotations::MANAGED_BY].as_str(),
            Some(FIELD_MANAGER),
            "owned_flux_metadata annotations must carry MANAGED_BY=FIELD_MANAGER",
        );
        assert_eq!(
            anns[annotations::PROCESS].as_str(),
            Some("demo-ns/ephemeral-demo"),
            "owned_flux_metadata annotations must carry PROCESS=<ns>/<name>",
        );
    }

    #[test]
    fn owned_flux_metadata_agrees_across_the_three_flux_emit_sites() {
        // Cross-site coherence pin: all three Flux-owned emit sites
        // (Kustomization in `render_flux`, OCIRepository +
        // HelmRelease in `render_aplicacao`) route through this
        // composer, so their emitted `metadata` blocks MUST agree
        // byte-identically for the same `(ns, name)` — proved here
        // by observing the composer's output is a pure function of
        // `(ns, name)` and does NOT depend on any per-emit-site
        // hidden state. A regression that re-open-coded ONE site's
        // metadata scaffold (e.g. re-inlined the Kustomization
        // metadata block against a normalized `ns` while the OCI +
        // HR sites keep the raw `ns`) would silently drift the
        // three emit sites' metadata blocks apart; that drift is
        // impossible while all three route through this ONE
        // composer, and this pin binds the composer's purity
        // contract so a future implementation refactor cannot
        // introduce hidden state without failing here.
        let a = owned_flux_metadata("demo-ns", "ephemeral-demo");
        let b = owned_flux_metadata("demo-ns", "ephemeral-demo");
        assert_eq!(
            a, b,
            "owned_flux_metadata must be a pure function of (ns, name) — same args → same output",
        );
    }

    #[test]
    fn ownership_labels_pair_matches_annotations_pair() {
        // Cross-axis coherence: labels and annotations carry the SAME
        // 2-key ownership pair, byte-identical, for the same
        // `process_ref`. Operators grep resources by either axis
        // (`kubectl get -l tatara.pleme.io/process=…` on the labels
        // axis, annotation lookups on the annotations axis) and land
        // at the identical key pair. A future addition to one
        // primitive that isn't mirrored to the other (e.g. a
        // labels-only `VERSION` slot) would drift the two axes and
        // fail this pin — surfacing the desync at fail-before-pass-
        // after granularity rather than as a silent operator-facing
        // discrepancy between what `-l` selects and what the
        // annotations reader sees.
        let labels_map = ownership_labels("demo-ns/demo-app");
        let annotations_map = ownership_annotations("demo-ns/demo-app");
        assert_eq!(
            labels_map, annotations_map,
            "ownership_labels and ownership_annotations must return byte-identical maps",
        );
    }

    // ─── qualified_process_ref substrate pins ───────────────────────────
    //
    // The `format!("{ns}/{name}")` incantation was hand-authored at
    // SEVEN sites — three inside `render::render_flux`/
    // `render_aplicacao` metadata annotation seeds, one at
    // `render::render_export_jobs` binding `process_ref`, one here at
    // `inject_annotations`'s SSA-time re-injection seed, and two in
    // `phase_machine.rs` (`process_holds_any_claim`'s claim
    // comparator + `handle_releasing`'s label-selector composer).
    // These pins bind the primitive at fail-before-pass-after
    // granularity so a regression that swapped the two axes, changed
    // the separator, or renormalized the input surfaces here rather
    // than as silent operator-facing drift at every downstream
    // annotation seed / claim key / label selector.
    #[test]
    fn qualified_process_ref_joins_ns_and_name_with_slash() {
        // The invariant every downstream consumer composes against:
        // the qualified reference is EXACTLY `<ns>/<name>`, in that
        // order, joined by a single `/`. A regression that inserted
        // a colon, swapped the two axes, or dropped either half would
        // silently break every grep keyed on this shape (annotation
        // reader, claim-arbiter comparator, `PROCESS=<ref>` label
        // selector).
        assert_eq!(
            qualified_process_ref("demo-ns", "ephemeral-demo"),
            "demo-ns/ephemeral-demo",
        );
    }

    #[test]
    fn qualified_process_ref_accepts_string_deref_and_str_slice_shapes() {
        // The seven shipped callsites split across two shapes: the
        // render.rs `render_flux` / `render_aplicacao` sites + the
        // phase_machine.rs `process_holds_any_claim` site pass
        // `&str` slices directly from function params /
        // `.as_deref().unwrap_or(...)`; the render.rs
        // `render_export_jobs` site + the phase_machine.rs
        // `handle_releasing` site pass `&ns, &name` from `String`
        // locals (via deref coercion). Pin both shapes at the type
        // level — the `&str` parameters must accept both without
        // widening.
        let owned_ns = String::from("owned-ns");
        let owned_name = String::from("owned-app");
        let borrowed_ns: &str = "borrowed-ns";
        let borrowed_name: &str = "borrowed-app";
        assert_eq!(
            qualified_process_ref(&owned_ns, &owned_name),
            "owned-ns/owned-app",
        );
        assert_eq!(
            qualified_process_ref(borrowed_ns, borrowed_name),
            "borrowed-ns/borrowed-app",
        );
        // Mixed shapes (one owned, one borrowed) also ride cleanly —
        // matches the phase_machine.rs `process_holds_any_claim`
        // path where both slots come from the same `.as_deref()`
        // chain but future consumers may compose across owned +
        // borrowed provenance.
        assert_eq!(
            qualified_process_ref(&owned_ns, borrowed_name),
            "owned-ns/borrowed-app",
        );
    }

    #[test]
    fn qualified_process_ref_composes_cleanly_into_ownership_annotations() {
        // The primary consumer is `ownership_annotations` — four of
        // the seven pre-lift sites (three in render.rs + this one
        // in ssapply.rs) pass the result straight through as the
        // annotation seed's `process_ref` arg. Pin the composition:
        // building the standard 2-slot ownership tag through
        // `ownership_annotations(&qualified_process_ref(ns, name))`
        // produces the SAME map a pre-lift hand-authored
        // `ownership_annotations(&format!("{ns}/{name}"))` did —
        // no drift between the primitive-composed form and the
        // literal-composed form the tests assert against below.
        let composed = ownership_annotations(&qualified_process_ref("demo", "ephemeral-demo"));
        let hand_authored = ownership_annotations("demo/ephemeral-demo");
        assert_eq!(
            composed, hand_authored,
            "primitive-composed process_ref must produce the same annotation map as a pre-lift hand-authored literal"
        );
        // And the resulting PROCESS slot value is exactly the
        // `<ns>/<name>` shape every downstream `PROCESS=<ref>`
        // label-selector composes against.
        assert_eq!(
            composed.get(annotations::PROCESS).and_then(Value::as_str),
            Some("demo/ephemeral-demo"),
        );
    }

    #[test]
    fn qualified_process_ref_rides_edge_case_axis_shapes() {
        // The reconciler shapes the two axes as arbitrary strings —
        // no length/character validation happens at the composer, so
        // any shape a Process's `metadata.namespace` /
        // `metadata.name` can hold rides through unchanged. Pin the
        // empty-string cases (unnamed process pre-metadata,
        // cluster-scoped `namespace = ""` fallback), the
        // whitespace-and-slash-in-name pathological case (a
        // regression that URL-escaped the input at this primitive
        // would silently break every downstream grep), and the
        // shape `process_holds_any_claim` early-returns on
        // (`name.is_empty()`) so the composer's post-condition
        // matches the caller's pre-condition.
        assert_eq!(qualified_process_ref("", ""), "/");
        assert_eq!(qualified_process_ref("default", ""), "default/");
        assert_eq!(qualified_process_ref("", "orphan"), "/orphan");
        assert_eq!(
            qualified_process_ref("weird ns", "with/slash"),
            "weird ns/with/slash",
        );
    }

    // ─── resolve_target_namespace substrate pins ────────────────────────
    //
    // The `.as_deref().unwrap_or(default_ns)` incantation was
    // hand-authored at FIVE sites in `boundary.rs` — the four
    // kind-typed boundary evaluators (`evaluate_job_attested`,
    // `evaluate_closed_loop_auth`, `evaluate_process_phase`,
    // `evaluate_flux_ready`) plus the `check_depends_on` per-entry
    // loop that resolves each `spec.dependsOn` entry's `namespace`
    // slot. These pins bind the primitive at fail-before-pass-after
    // granularity so a regression that re-inlined the incantation,
    // swapped the argument order (making the override the fallback),
    // or dropped the `unwrap_or` branch surfaces here rather than as
    // silent operator-facing drift at every downstream ns-scoped
    // `Api::namespaced` call.
    #[test]
    fn resolve_target_namespace_falls_back_to_default_when_explicit_is_none() {
        // The most common shape at every boundary evaluator callsite:
        // the operator omitted the payload's optional `namespace`
        // slot, so the resolver picks up the enclosing Process's
        // default namespace and every downstream fetch happens in the
        // Process's own namespace.
        assert_eq!(
            resolve_target_namespace(None, "process-owned-ns"),
            "process-owned-ns",
            "None override must fall back to default_ns"
        );
    }

    #[test]
    fn resolve_target_namespace_returns_explicit_when_present() {
        // The cross-namespace shape: the operator named a specific
        // target namespace on the payload, so the resolver ignores
        // the enclosing Process's default and the downstream fetch
        // happens in the operator's chosen namespace. This is the
        // shape that lets a Process in namespace A gate on a Flux
        // Kustomization / probe Job / dependency Process in
        // namespace B without a per-evaluator hand-authored
        // exception.
        assert_eq!(
            resolve_target_namespace(Some("target-ns"), "process-owned-ns"),
            "target-ns",
            "Some override must win over default_ns"
        );
    }

    #[test]
    fn resolve_target_namespace_honors_explicit_argument_order() {
        // Pin the argument order — a regression that swapped the two
        // arguments (making `default_ns` the override candidate and
        // the operator's payload slot the fallback) would silently
        // route every fetch to the Process's own namespace regardless
        // of the operator's override. This pin surfaces the swap
        // directly on the type-adjacent invariant rather than as
        // downstream misroute at every dependency / Flux / probe /
        // Process lookup.
        let explicit = Some("EXPLICIT");
        let default_ns = "DEFAULT";
        assert_eq!(resolve_target_namespace(explicit, default_ns), "EXPLICIT");
        // And with the override absent, the same call must resolve
        // to `default_ns` — the two branches partition the input
        // space with no overlap.
        assert_eq!(resolve_target_namespace(None, default_ns), "DEFAULT");
    }

    #[test]
    fn resolve_target_namespace_matches_prior_hand_authored_unwrap_shape() {
        // Byte-identical parity with the pre-lift
        // `.as_deref().unwrap_or(default_ns)` incantation across a
        // sweep of shapes every callsite plausibly encounters:
        // empty-string override (an explicit `""` from a payload
        // whose namespace key is present but empty — treated as
        // "explicitly cluster-scoped" rather than "fall back"),
        // whitespace-embedded namespace, K8s canonical shapes, and
        // the None + non-empty default combination that stands in
        // for every default-fed callsite.
        for (explicit_owned, default_ns) in [
            (Some(String::new()), "default"),
            (Some(String::from("kube-system")), "flux-system"),
            (Some(String::from("with spaces")), "default"),
            (None, "flux-system"),
            (None, ""),
        ] {
            let explicit = explicit_owned.as_deref();
            assert_eq!(
                resolve_target_namespace(explicit, default_ns),
                explicit.unwrap_or(default_ns),
                "primitive must be byte-identical to pre-lift `.unwrap_or(default_ns)` on {explicit:?}, default {default_ns:?}"
            );
        }
    }

    // ─── flux_ref_fetch_error_context / fetch_flux_ref substrate pins ──
    //
    // The 5-slot `ssapply::fetch(client, &r.namespace, &r.api_version,
    // &r.kind, &r.name).map_err(|e| anyhow!("fetch {}/{}: {e}",
    // r.kind, r.name))?` splat + wrap recurred at TWO hand-authored
    // sites in `tatara-reconciler::phase_machine` (`handle_running`,
    // `handle_attested`) past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // trigger. Post-lift each site names the ref ONCE and rides
    // through [`fetch_flux_ref`]; the diagnostic wording lives at the
    // pure helper [`flux_ref_fetch_error_context`]. These pins bind
    // the wording invariant + the (kind, name) slot order at
    // fail-before-pass-after granularity so a regression that
    // reversed the two axes in the label (`"fetch <name>/<kind>"`) or
    // reworded the prefix surfaces HERE rather than as silent
    // operator-facing log skew across the two consumers.

    fn sample_flux_ref_for_diag() -> FluxResourceRef {
        // Distinct kind + name so a swap between the two axes at the
        // diagnostic-context helper surfaces as an equality failure
        // rather than as identity coincidence.
        FluxResourceRef {
            api_version: "helm.toolkit.fluxcd.io/v2".to_string(),
            kind: "HelmRelease".to_string(),
            name: "observability-stack".to_string(),
            namespace: "flux-system".to_string(),
            ready: false,
            message: None,
            last_check: None,
        }
    }

    #[test]
    fn flux_ref_fetch_error_context_names_kind_and_name() {
        // Wording pin: the diagnostic prefix reads exactly
        // `"fetch <kind>/<name>"`. A regression that reversed the two
        // axes (`"fetch <name>/<kind>"`), reworded the prefix
        // (`"fetching ..."`), or dropped the slash separator would
        // surface here rather than as silent operator-facing log skew
        // at every downstream Flux fetch consumer.
        let r = sample_flux_ref_for_diag();
        assert_eq!(
            flux_ref_fetch_error_context(&r),
            "fetch HelmRelease/observability-stack"
        );
    }

    #[test]
    fn flux_ref_fetch_error_context_reads_kind_and_name_slots_only() {
        // Slot-coverage pin: the diagnostic label reads ONLY the
        // `kind` and `name` slots of the ref — a caller varying the
        // `namespace` / `api_version` / status slots (ready / message
        // / last_check) leaves the label byte-identical. Pins the
        // composer's contract: the label discriminates between refs
        // by (kind, name), not by any of the other five slots.
        let base = sample_flux_ref_for_diag();
        let expected = flux_ref_fetch_error_context(&base);
        let variants = [
            FluxResourceRef {
                namespace: "some-other-ns".to_string(),
                ..base.clone()
            },
            FluxResourceRef {
                api_version: "helm.toolkit.fluxcd.io/v9".to_string(),
                ..base.clone()
            },
            FluxResourceRef {
                ready: true,
                ..base.clone()
            },
            FluxResourceRef {
                message: Some("some drift".to_string()),
                ..base.clone()
            },
        ];
        for v in variants {
            assert_eq!(
                flux_ref_fetch_error_context(&v),
                expected,
                "the (kind, name) axes gate the label; every other slot must be transparent"
            );
        }
    }

    #[test]
    fn flux_ref_fetch_error_context_swap_between_kind_and_name_diverges() {
        // Slot-inversion pin: a ref with (kind, name) swapped
        // produces a distinct label — the symmetric pair
        // (`sample.kind`, `sample.name`) reads back as (name, kind)
        // through the raw `.fetch_coords()` projection and would
        // fail-loudly at the assertion if a regression reversed the
        // two axes inside the helper. A drop of either axis (leaving
        // a `"fetch <name>"` or `"fetch <kind>"` label) would fail
        // both assertions.
        let normal = sample_flux_ref_for_diag();
        let swapped = FluxResourceRef {
            kind: normal.name.clone(),
            name: normal.kind.clone(),
            ..normal.clone()
        };
        assert_ne!(
            flux_ref_fetch_error_context(&normal),
            flux_ref_fetch_error_context(&swapped),
            "swapping (kind, name) must diverge the label"
        );
        assert_eq!(
            flux_ref_fetch_error_context(&swapped),
            "fetch observability-stack/HelmRelease",
            "the swapped ref must emit the swapped label byte-identically"
        );
    }

    #[test]
    fn flux_ref_fetch_error_context_matches_pre_lift_hand_authored_wording() {
        // Cross-substrate coherence pin: the composed diagnostic
        // wording must be byte-identical to the pre-lift hand-authored
        // `format!("fetch {}/{}", r.kind, r.name)` incantation at
        // BOTH `phase_machine::handle_running` and
        // `phase_machine::handle_attested`. Sweeps a handful of ref
        // shapes so a regression that special-cased one variant
        // (e.g. a `Kustomization`-only path via `if r.kind ==
        // "Kustomization" ...`) would surface here.
        let cases = [
            (
                "kustomize.toolkit.fluxcd.io/v1",
                "Kustomization",
                "observability-stack",
                "flux-system",
            ),
            (
                "helm.toolkit.fluxcd.io/v2",
                "HelmRelease",
                "prometheus-op",
                "monitoring",
            ),
            (
                "source.toolkit.fluxcd.io/v1beta2",
                "OCIRepository",
                "chart-source",
                "flux-system",
            ),
        ];
        for (av, kind, name, ns) in cases {
            let r = FluxResourceRef {
                api_version: av.to_string(),
                kind: kind.to_string(),
                name: name.to_string(),
                namespace: ns.to_string(),
                ready: false,
                message: None,
                last_check: None,
            };
            assert_eq!(
                flux_ref_fetch_error_context(&r),
                format!("fetch {}/{}", r.kind, r.name),
                "post-lift context must equal pre-lift `format!(\"fetch {{kind}}/{{name}}\")` at every callsite shape"
            );
        }
    }

    // ─── fetch_by_identity_error_context substrate pins ──────────────
    //
    // The `.map_err(|e| anyhow!("fetch <kind-literal> {ns}/{name}:
    // {e}"))?` incantation recurred at THREE hand-authored sites in
    // `tatara-reconciler::boundary` past the ★★ PRIME-DIRECTIVE ≥ 2
    // duplication trigger — `fetch_job_status`, `verify_receipt_cm`,
    // `evaluate_flux_ready` — each restating the closed-set variant's
    // `.wire_identity().kind` slot as a bare `&str` literal alongside
    // the same variant's `.wire_identity()` at the fetch call. Post-
    // lift the label rides through `identity.kind` inside the shared
    // helper here, and the `<ns>/<name>` half rides through the
    // sibling substrate primitive [`qualified_process_ref`]. These
    // pins bind the wording invariant + the (kind, ns/name) slot
    // partition at fail-before-pass-after granularity so a regression
    // that dropped the kind slot, swapped the ns/name axes, or
    // reworded the prefix surfaces HERE rather than as silent
    // operator-facing log skew across the three consumers.

    #[test]
    fn fetch_by_identity_error_context_names_kind_and_ns_slash_name() {
        // Wording pin: the diagnostic prefix reads exactly
        // `"fetch <kind> <ns>/<name>"`. A regression that reversed the
        // two ns/name axes (`"fetch <kind> <name>/<ns>"`), reworded
        // the prefix (`"fetching ..."`), dropped the space separator
        // between the kind and the ns/name pair, or dropped the slash
        // separator between ns and name would surface here rather
        // than as silent operator-facing log skew at every downstream
        // identity-gated fetch consumer.
        let identity = K8sWireIdentity::new("batch/v1", "Job");
        assert_eq!(
            fetch_by_identity_error_context(identity, "default", "my-job"),
            "fetch Job default/my-job"
        );
    }

    #[test]
    fn fetch_by_identity_error_context_rides_through_qualified_process_ref() {
        // Substrate-composition pin: the `<ns>/<name>` half of the
        // label is BYTE-IDENTICAL to the sibling substrate primitive
        // [`qualified_process_ref`]. A future normalization at
        // `qualified_process_ref` (case-fold, unicode-safe collation,
        // a `<cluster>/<ns>/<name>` cross-cluster form) reaches this
        // diagnostic label mechanically — a regression that inlined
        // the `format!("{ns}/{name}")` shape here (bypassing the
        // shared owner) would surface at this equality check rather
        // than as a silent diagnostic vs annotation drift the operator
        // sees only when grepping across log lines and annotations.
        let identity = K8sWireIdentity::new("v1", "ConfigMap");
        let ns = "monitoring";
        let name = "prom-op-receipt";
        let composed = fetch_by_identity_error_context(identity, ns, name);
        let ref_ = qualified_process_ref(ns, name);
        assert!(
            composed.ends_with(&ref_),
            "the `<ns>/<name>` tail of the label ({composed:?}) must equal the substrate \
             primitive `qualified_process_ref` output ({ref_:?}) byte-identically"
        );
        assert_eq!(composed, format!("fetch {} {ref_}", identity.kind));
    }

    #[test]
    fn fetch_by_identity_error_context_reads_kind_slot_only_from_identity() {
        // Slot-coverage pin: the label reads ONLY the `kind` slot of
        // the identity — a caller varying the `api_version` slot
        // leaves the label byte-identical. Pins the composer's
        // contract: the label discriminates identities by `kind` (the
        // operator-facing dispatch key), not by `api_version` (the
        // K8s wire-form group/version pair the operator does not
        // grep for). A regression that surfaced the `api_version`
        // slot alongside the `kind` (a switch to `"fetch <apiVersion>
        // <kind> <ns>/<name>"`) would fail this pin and the wording
        // pin above simultaneously.
        let base = K8sWireIdentity::new("batch/v1", "Job");
        let expected = fetch_by_identity_error_context(base, "ns", "n");
        let av_variant = K8sWireIdentity::new("batch/v9alpha42", "Job");
        assert_eq!(
            fetch_by_identity_error_context(av_variant, "ns", "n"),
            expected,
            "the `kind` axis gates the label; the `api_version` slot must be transparent"
        );
    }

    #[test]
    fn fetch_by_identity_error_context_swap_between_kind_variants_diverges() {
        // Slot-inversion pin: a copy-paste that changed the closed-
        // set variant at the identity but forgot to update the label
        // was the exact silent-drift trap the three pre-lift hand-
        // authored callsites were vulnerable to. Post-lift the label
        // derives from `identity.kind`, so changing the variant
        // mechanically changes the label — the two-mention trap is
        // unrepresentable. Pin that swapping between two
        // K8sBuiltinResource variants at the identity produces a
        // distinct label byte-identically.
        use tatara_process::k8s_builtin_resource::K8sBuiltinResource;
        let job_id = K8sBuiltinResource::Job.wire_identity();
        let cm_id = K8sBuiltinResource::ConfigMap.wire_identity();
        let job_label = fetch_by_identity_error_context(job_id, "ns", "n");
        let cm_label = fetch_by_identity_error_context(cm_id, "ns", "n");
        assert_ne!(
            job_label, cm_label,
            "swapping the closed-set variant must diverge the label"
        );
        assert_eq!(job_label, "fetch Job ns/n");
        assert_eq!(cm_label, "fetch ConfigMap ns/n");
    }

    #[test]
    fn fetch_by_identity_error_context_matches_pre_lift_hand_authored_wording() {
        // Cross-substrate coherence pin: the composed diagnostic
        // wording must be byte-identical to the pre-lift hand-
        // authored `format!("fetch {kind} {ns}/{name}")` incantation
        // at ALL THREE `tatara-reconciler::boundary` callsite shapes:
        // `fetch_job_status` (`K8sBuiltinResource::Job`),
        // `verify_receipt_cm` (`K8sBuiltinResource::ConfigMap`),
        // `evaluate_flux_ready` (`FluxResource::{Kustomization,
        // HelmRelease, OCIRepository}`). Sweeps a handful of
        // (identity, ns, name) triples so a regression that special-
        // cased one closed set (e.g. hard-wired `"Job"` at the label
        // slot instead of reading `identity.kind`) would surface
        // here.
        use tatara_process::flux_resource::FluxResource;
        use tatara_process::k8s_builtin_resource::K8sBuiltinResource;
        let cases: &[(K8sWireIdentity, &str, &str)] = &[
            (K8sBuiltinResource::Job.wire_identity(), "default", "my-job"),
            (
                K8sBuiltinResource::ConfigMap.wire_identity(),
                "monitoring",
                "receipt-cm",
            ),
            (
                FluxResource::Kustomization.wire_identity(),
                "flux-system",
                "observability-stack",
            ),
            (
                FluxResource::HelmRelease.wire_identity(),
                "monitoring",
                "prometheus-op",
            ),
            (
                FluxResource::OCIRepository.wire_identity(),
                "flux-system",
                "chart-source",
            ),
        ];
        for (identity, ns, name) in cases {
            assert_eq!(
                fetch_by_identity_error_context(*identity, ns, name),
                format!("fetch {} {ns}/{name}", identity.kind),
                "post-lift wording must equal pre-lift `format!(\"fetch {{kind}} {{ns}}/{{name}}\")` \
                 at every callsite shape"
            );
        }
    }

    #[test]
    fn resolve_target_namespace_composes_cleanly_into_qualified_process_ref() {
        // Peer-primitive coherence: the resolved namespace flows
        // directly into `qualified_process_ref` at the sibling
        // ownership-annotation seed on every render / SSA callsite,
        // and at the diagnostic-composing `format!("{ns}/{name}")`
        // sites `check_depends_on` still uses inline for its
        // `UnmetDependency.message`. Pin that the two peer
        // primitives compose byte-identically to the pre-lift
        // hand-authored `format!("{}/{}", parsed.namespace.as_deref()
        // .unwrap_or(default_ns), parsed.name)` chain at every
        // callsite.
        let composed = qualified_process_ref(
            resolve_target_namespace(Some("target-ns"), "process-owned-ns"),
            "job-name",
        );
        assert_eq!(composed, "target-ns/job-name");
        let composed_default = qualified_process_ref(
            resolve_target_namespace(None, "process-owned-ns"),
            "job-name",
        );
        assert_eq!(composed_default, "process-owned-ns/job-name");
    }
}
