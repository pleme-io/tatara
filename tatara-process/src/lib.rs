//! Process CRD — the K8s-as-Unix-processes wire format.
//!
//! A `Process` is one element of the tatara convergence lattice.
//! Clusters, HelmReleases, migrations, tests — all are Processes.
//! The reconciliation loop *is* Unix: fork → exec → wait → exit → reap.

pub mod allocation;
pub mod attestation;
pub mod boundary;
pub mod classification;
pub mod compliance;
pub mod crd;
pub mod encapsulates;
pub mod env;
pub mod ephemeral;
pub mod export;
pub mod flux_resource;
pub mod hostname;
pub mod identity;
pub mod intent;
pub mod k8s_builtin_resource;
pub mod k8s_object_ref;
pub mod k8s_wire_identity;
pub mod lifetime;
pub mod lifetime_clock;
pub mod matrix;
pub mod phase;
pub mod pool;
pub mod receipt;
pub mod routing;
pub mod routing_edge_resource;
pub mod signal;
pub mod spec;
pub mod status;
pub mod table;
pub mod tagged_union;

pub mod prelude {
    pub use crate::allocation::{
        AllocationCondition, AllocationPhase, AllocationSpec, AllocationStatus,
        EphemeralAllocation, Requestor,
    };
    pub use crate::attestation::ProcessAttestation;
    pub use crate::boundary::{Boundary, Condition, ConditionKind, UnknownConditionKind};
    pub use crate::classification::{
        Arity, CalmClassification, Classification, ConvergencePointType, DataClassification,
        Horizon, HorizonKind, OptimizationDirection, SubstrateType, UnknownCalmClassification,
        UnknownConvergencePointType, UnknownDataClassification, UnknownHorizonKind,
        UnknownOptimizationDirection, UnknownSubstrateType,
    };
    pub use crate::compliance::{
        ComplianceBinding, ComplianceSpec, UnknownVerificationPhase, VerificationPhase,
    };
    pub use crate::crd::{Process, ProcessSpec, ProcessStatus};
    pub use crate::encapsulates::{
        BareWorkload, EncapsulatesSpec, EncapsulationKind, EncapsulationKindError,
        EncapsulationKindVariant, EncapsulationMode, EncapsulationTarget, ExistingHelmRelease,
        ExistingKustomization, UnknownEncapsulationMode, UnknownEncapsulationTarget,
    };
    pub use crate::ephemeral::{compile_ephemeral_source, EphemeralSpec};
    pub use crate::export::{
        ArtifactError, ArtifactKind, ArtifactSource, ArtifactVariant, ChannelError, ChannelKind,
        ChannelVariant, ExportSpec, ExportTrigger, HttpEventChannel, NatsSubjectChannel,
        ProcessSnapshotSource, ReceiptsSource, ReportFormat, ReportPayloadShape, RunMarkerSource,
        StdoutChannel, TestReportSource, UnknownArtifactKind, UnknownChannelKind,
        UnknownExportTrigger, UnknownReportFormat, VectorChannel, DEFAULT_NATS_URL,
        DEFAULT_VECTOR_INGEST,
    };
    pub use crate::flux_resource::FluxResource;
    pub use crate::hostname::{
        ephemeral_id_from_spec, fmt_fqdn, fmt_fqdn_stable, resolve_ephemeral_id, HostnameError,
        EPHEMERAL_ID_HASH_LEN,
    };
    pub use crate::identity::{content_hash, derive_identity, format_process_address, Identity};
    pub use crate::intent::{
        AplicacaoIntent, ContainerIntent, FluxIntent, GuestIntent, HelmLifecyclePolicy,
        HelmRemediationPolicy, Intent, IntentError, IntentKind, IntentVariant, LispIntent,
        NixIntent, UnknownWorkloadKind, WorkloadKind, FLUX_HELM_DEFAULT_INTERVAL,
        HELM_LIFECYCLE_DEFAULT_RETRIES, HELM_LIFECYCLE_DEFAULT_TIMEOUT,
    };
    pub use crate::k8s_builtin_resource::K8sBuiltinResource;
    pub use crate::k8s_object_ref::K8sObjectRef;
    pub use crate::k8s_wire_identity::K8sWireIdentity;
    pub use crate::lifetime::{
        EphemeralLifetime, Lifetime, LifetimeError, LifetimeKind, LifetimeVariant,
        PermanentLifetime, TeardownPolicy, UnknownTeardownPolicy,
    };
    pub use crate::lifetime_clock::{
        evaluate as lifetime_clock_evaluate, AutoTerminate, AutoTerminateKind, TerminateReason,
        TerminateReasonKind, UnknownAutoTerminateKind, UnknownTerminateReasonKind,
    };
    pub use crate::matrix::{
        compile_env_matrix_source, EnvMatrixSpec, MatrixAxis, MatrixBudget, NamedEphemeral,
        SelectStrategy, SelectStrategyKind, UnknownSelectStrategyKind,
    };
    pub use crate::phase::{ProcessPhase, UnknownPhase};
    pub use crate::pool::{
        AllocationRef, EphemeralPool, MatchKey, MemberState, PoolCondition, PoolMember, PoolPhase,
        PoolSelector, PoolSpec, PoolStatus, ReplacementPolicy, ReturnPolicy, UnknownMemberState,
        UnknownPoolPhase, UnknownReplacementPolicy,
    };
    pub use crate::qualified_process_ref;
    pub use crate::receipt::{
        default_receipt_config_map_name, ReceiptEnvelope, ReceiptError, ReceiptKind,
        RECEIPT_CM_SUFFIX, RECEIPT_VERSION,
    };
    pub use crate::routing::{RoutingBackend, RoutingForm, RoutingHostname, RoutingSpec};
    pub use crate::routing_edge_resource::RoutingEdgeResource;
    pub use crate::signal::{ProcessSignal, SighupStrategy, UnknownSighupStrategy};
    pub use crate::spec::{
        DependsOn, IdentitySpec, MustReachPhase, SignalPolicy, UnknownMustReachPhase,
    };
    pub use crate::status::{
        BoundaryStatus, CheckedCondition, ComplianceStatus, FluxResourceRef, ProcessCondition,
        RenderedResourceCoords,
    };
    pub use crate::table::{
        ClaimRecord, ProcessEntry, ProcessTable, ProcessTableSpec, ProcessTableStatus,
    };
}

/// CRD API group for every tatara CRD.
pub const GROUP: &str = "tatara.pleme.io";
/// CRD version for this module.
pub const VERSION: &str = "v1alpha1";
/// Kind spelling of the tatara Process CRD as it appears in a K8s
/// [`OwnerReference.kind`][ownref] field. Peer to [`GROUP`] +
/// [`VERSION`] — centralizes the ONE literal every SSA-time
/// re-injection helper pre-lift restated by hand across
/// `tatara-reconciler` (`render.rs`, `edges.rs`, `ssapply.rs`).
///
/// [ownref]: https://kubernetes.io/docs/concepts/overview/working-with-objects/owners-dependents/
pub const PROCESS_KIND: &str = "Process";

/// Canonical `<GROUP>/<VERSION>` as an owned `String` — the ONE
/// K8s `apiVersion` shape every tatara CRD stamps. Composed from
/// [`GROUP`] + [`VERSION`] so a bump of either constant lands here
/// exactly once; pre-lift, two `tatara-reconciler` sites hand-wrote
/// `format!("{}/{}", tatara_process::GROUP, tatara_process::VERSION)`
/// while a third inlined the literal `"tatara.pleme.io/v1alpha1"`,
/// opening a silent drift path if `VERSION` ever advances past
/// `v1alpha1`.
pub fn api_version() -> String {
    format!("{GROUP}/{VERSION}")
}

/// Substrate-primitive composer for the canonical
/// **namespace-qualified process reference** — the `<ns>/<name>`
/// string every consumer that grepped, keyed, or annotated a
/// Process by "which cluster location owns it" hand-authored as
/// `format!("{ns}/{name}")` at scattered sites across the workspace.
/// Lifted onto `tatara-process` (from its prior home at
/// `tatara_reconciler::ssapply::qualified_process_ref`) so callers
/// BELOW the reconciler layer — `tatara-export-worker` (which does
/// NOT depend on `tatara-reconciler`) and `tatara-pool-reconciler` —
/// reach the SAME composer the reconciler-side sites do, closing
/// the previously-open substrate corner where a downstream consumer
/// re-authored the shape by hand rather than routing through the
/// ONE primitive.
///
/// The `<ns>/<name>` shape is the workspace-wide convention for
/// "how to name a namespaced K8s resource in a single string" — the
/// same shape the K8s API server itself uses in
/// [`OwnerReference`][ownref] pretty-printing, in the `holder` slot of
/// [`crate::table::ClaimRecord`], and in the `tatara.pleme.io/process`
/// annotation every reconciler-emitted resource carries. Callers
/// with a live [`crate::prelude::Process`] compose through
/// [`crate::prelude::Process::coordinates_or_defaults`] +
/// [`Self`] (this function); callers with bare
/// `(ns: &str, name: &str)` params (CLI-arg driven binaries,
/// `metadata`-agnostic composers) call this directly.
///
/// The 2-arg signature encodes the invariant "the qualified
/// reference is EXACTLY `<ns>/<name>`, in that order, joined by a
/// single `/` separator" at the type level — a caller cannot
/// accidentally swap the two axes (which would produce `<name>/<ns>`
/// and silently break every downstream grep) nor omit either half,
/// the way a pre-lift hand-authored `format!("{name}/{ns}")` or
/// `format!("{ns}-{name}")` typo would.
///
/// A future change to the reference shape — a `<ns>/<name>@<gen>`
/// multi-generation variant for attestation grepping, a
/// `<cluster>/<ns>/<name>` cross-cluster form, a normalization
/// (case-fold, unicode-safe collation) that must apply everywhere —
/// lands at ONE substrate function here and every downstream
/// composer (annotation seed, ProcessTable claim key, label
/// selector, owner metadata, export-worker run-id fallback,
/// receipt-owner filter) inherits the upgrade mechanically.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the `<ns>/<name>` shape recurred at hand-authored sites past the
/// ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted onto
/// the ONE workspace-wide owner here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — a regression that swapped the
/// two axes or the separator at ONE site surfaces at
/// [`qualified_process_ref_tests::qualified_process_ref_joins_ns_and_name_with_slash`]
/// rather than as silent drift at every downstream annotation seed
/// / claim key / label selector / run-id / receipt-owner filter).
///
/// [ownref]: https://kubernetes.io/docs/concepts/overview/working-with-objects/owners-dependents/
#[must_use]
pub fn qualified_process_ref(ns: &str, name: &str) -> String {
    format!("{ns}/{name}")
}

/// Build a Kubernetes [`OwnerReference`][ownref] JSON blob pointing
/// at a Process (`kind = `[`PROCESS_KIND`], `apiVersion = `
/// [`api_version`]) with `controller: true` +
/// `blockOwnerDeletion: true` — the exact 6-slot shape every SSA
/// re-injection site pre-lift restated three times across
/// `tatara-reconciler` (`render.rs::owner_refs` for export-Job
/// owners, `edges.rs::build_owner_refs` for Ingress + DNSEndpoint
/// owners, `ssapply.rs::build_owner_reference` for the injected
/// owner-ref stamped on every applied `DynamicObject`). Callers
/// with a live `Process` value read `metadata.{name,uid}` and pass
/// them through as `&str`.
///
/// The 6-slot shape is fixed (`controller` + `blockOwnerDeletion`
/// both `true`); a Process-owned resource that wants a non-
/// controller reference doesn't belong on this owner and can build
/// its own `json!` inline — this primitive is the composer for the
/// canonical "Process controls this resource, cascade-delete on
/// GC" shape, not a general OwnerReference builder.
///
/// [ownref]: https://kubernetes.io/docs/concepts/overview/working-with-objects/owners-dependents/
pub fn owner_reference_json(name: &str, uid: &str) -> serde_json::Value {
    serde_json::json!({
        "apiVersion": api_version(),
        "kind": PROCESS_KIND,
        "name": name,
        "uid": uid,
        "controller": true,
        "blockOwnerDeletion": true,
    })
}

/// Substrate-primitive builder for a Process-owned resource's
/// **`metadata.ownerReferences` array** — the empty-uid-gated,
/// single-entry `Vec<Value>` every emit site that lacks a fully
/// materialized [`crate::prelude::Process`] (i.e. every site that
/// works from a bare `(name, uid)` pair rather than routing through
/// [`ssapply::build_owner_reference`](../tatara_reconciler/ssapply/fn.build_owner_reference.html)'s
/// anyhow-guarded unwrap) hand-composed by wrapping
/// [`owner_reference_json`] in a `Vec::new()` + `is_empty` gate on
/// the `uid` slot.
///
/// The `uid.is_empty()` gate encodes the invariant every caller
/// already enforced: a Process pre-metadata (fixtured in tests, or
/// caught mid-Forking before the API server has stamped a `uid`) has
/// no admissible owner reference to point at, so the emit site
/// stamps `metadata.ownerReferences: []` rather than an
/// owner-referenceless resource pointing at a placeholder uid the K8s
/// GC would silently ignore. Post-lift the gate lives at ONE
/// primitive so a regression that inlined an owner reference for
/// an empty uid — which the API server accepts and quietly detaches
/// from cascade-delete — surfaces at THIS primitive's pin rather
/// than as an operator-visible ownerless resource after apply.
///
/// Pre-lift the 3-line `let mut owner_refs = vec![]; if
/// !uid.is_empty() { owner_refs.push(owner_reference_json(name,
/// uid)); }` incantation was hand-authored at TWO sites past the
/// ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold in
/// `tatara-reconciler`, each restating the same gated composition:
/// * `edges::build_owner_refs` — the shared owner-refs builder both
///   `IngressEdge` + `DnsEndpointEdge` route through, sourcing
///   `(process_name, process_uid)` from the [`crate::edges::EdgeContext`].
/// * `render::one_export_job` — the export Job's owner-refs seed,
///   sourcing `(name, uid)` from the [`crate::prelude::Process`]
///   `render_export_jobs` threaded in.
///
/// Post-lift both callsites read `owner_references_json(name, uid)`.
/// A future addition — e.g. a second owner-reference slot naming a
/// controlling ProcessTable entry, a policy that stamps a stale-uid
/// warning annotation before returning empty, or a normalization
/// that strips a cluster-prefix off the uid — lands at ONE
/// substrate function here and every emit site inherits the upgrade
/// mechanically. The [`ssapply::build_owner_reference`] path (which
/// works from a materialized [`crate::prelude::Process`] and errors
/// on absent `metadata.uid`) is a peer, not a lift candidate: its
/// contract is "the K8s API server assigned a uid, so refuse to
/// SSA-apply resources whose owner cannot be materialized", while
/// this primitive's contract is "the caller has an optional-uid
/// posture; emit `[]` when the uid is absent". The two shapes
/// partition the input space at the "is the enclosing scope
/// obligated to produce a materialized Process reference" axis.
///
/// The 2-arg `(&str, &str)` signature accepts both the
/// `EdgeContext`-sourced `(&str, &str)` slice shape and the
/// `render_export_jobs`-owned `(name: &str, uid: &str)` local shape
/// without widening — matches every current callsite.
pub fn owner_references_json(name: &str, uid: &str) -> Vec<serde_json::Value> {
    if uid.is_empty() {
        vec![]
    } else {
        vec![owner_reference_json(name, uid)]
    }
}

/// Annotation keys the reconciler reads/writes on owned FluxCD resources.
pub mod annotations {
    pub const MANAGED_BY: &str = "tatara.pleme.io/managed-by";
    pub const PROCESS: &str = "tatara.pleme.io/process";
    pub const PID: &str = "tatara.pleme.io/pid";
    pub const CONTENT_HASH: &str = "tatara.pleme.io/content-hash";
    pub const ATTESTATION_ROOT: &str = "tatara.pleme.io/attestation-root";
    pub const GENERATION: &str = "tatara.pleme.io/generation";
    pub const SIGNAL: &str = "tatara.pleme.io/signal";
    /// Stamped by the reconciler when transitioning into `Releasing`
    /// — records which terminal-reached gate the Process came from
    /// (`Attested` or `Failed`) so `handle_releasing` can pick the
    /// matching `ExportTrigger` set + the correct post-Releasing
    /// destination (`Exiting` from Attested, `Zombie` from Failed).
    pub const RELEASED_FROM: &str = "tatara.pleme.io/released-from";
    /// Labels the export-worker Jobs the reconciler emits during
    /// `Releasing`. Selector: `tatara.pleme.io/role=export`.
    pub const ROLE: &str = "tatara.pleme.io/role";
    /// Index of an export inside `lifetime.ephemeral.exports`.
    /// Stamped on the corresponding tatara-export-worker Job + its
    /// receipt ConfigMap so the reconciler can correlate them
    /// without re-parsing the spec JSON.
    pub const EXPORT_INDEX: &str = "tatara.pleme.io/export-index";
    /// Label / annotation key stamping which
    /// `RoutingSpec.hostnames` entry a routing edge (Ingress /
    /// DNSEndpoint) belongs to. Value is the entry's `app` slot;
    /// a `label`-selector on this key slices every emitted edge
    /// for a given `app` regardless of hostname form. Peer to
    /// [`ROUTING_FORM`] on the routing-axis pair.
    pub const APP: &str = "tatara.pleme.io/app";
    /// Label / annotation key stamping the routing form
    /// (`"stable"` | `"instance"`) on every emitted routing edge.
    /// Value is a [`crate::routing::RoutingForm`] wire-form string;
    /// consumers filtering the two forms compare to
    /// [`RoutingForm::as_str`][crate::routing::RoutingForm::as_str],
    /// never to a bare literal.
    pub const ROUTING_FORM: &str = "tatara.pleme.io/routing-form";
}

/// Standard finalizer for the Process reconciler.
pub const PROCESS_FINALIZER: &str = "tatara.pleme.io/process-finalizer";

/// Shared schemars helpers — emit OpenAPI schemas Kubernetes accepts.
/// Free-form `serde_json::Value` fields default to an *empty* schema
/// in schemars, which the K8s API server rejects with "type: Required
/// value: must not be empty for specified object fields". The typed
/// workaround is to emit `{type: object, x-kubernetes-preserve-unknown-
/// fields: true}` — same shape kube-rs's own helpers produce.
pub mod schema_helpers {
    use schemars::{gen::SchemaGenerator, schema::Schema};
    /// Schema for a free-form JSON object field. Apply via
    /// `#[schemars(schema_with = "tatara_process::schema_helpers::preserve_unknown_object")]`
    /// on any `serde_json::Value` / `BTreeMap<String, serde_json::Value>`
    /// field exposed through a CRD.
    pub fn preserve_unknown_object(_g: &mut SchemaGenerator) -> Schema {
        serde_json::from_value(serde_json::json!({
            "type": "object",
            "x-kubernetes-preserve-unknown-fields": true
        }))
        .expect("static JSON literal parses as Schema")
    }
}

#[cfg(test)]
mod owner_reference_tests {
    //! Pin the `owner_reference_json` composer at fail-before-pass-
    //! after granularity. Every shape a pre-lift caller hand-authored
    //! is re-asserted here so a regression that inlined any of the
    //! six slots at a call site (breaking the primitive's role as
    //! the ONE source of truth) fails HERE at the composer's shipped-
    //! shape pin rather than as silent drift between the pre-lift
    //! `render.rs` / `edges.rs` / `ssapply.rs` sites (which pre-lift
    //! already carried TWO different `apiVersion` spellings — a
    //! composed `format!("{}/{}", GROUP, VERSION)` at two sites and
    //! the frozen literal `"tatara.pleme.io/v1alpha1"` at the third).
    use super::{
        api_version, owner_reference_json, owner_references_json, GROUP, PROCESS_KIND, VERSION,
    };
    use serde_json::json;

    #[test]
    fn api_version_composes_group_and_version() {
        // Any bump of GROUP or VERSION lands at ONE composer.
        assert_eq!(api_version(), format!("{GROUP}/{VERSION}"));
    }

    #[test]
    fn api_version_byte_matches_wire_form_pre_lift() {
        // Byte-identity pin: the frozen wire-form literal
        // `"tatara.pleme.io/v1alpha1"` that `ssapply.rs::
        // build_owner_reference` hand-wrote pre-lift must equal the
        // composed shape now sourced through the ONE owner. A
        // future VERSION bump that missed this test would land as
        // an operator-visible reference-mismatch after apply.
        assert_eq!(api_version(), "tatara.pleme.io/v1alpha1");
    }

    #[test]
    fn process_kind_is_process_literal() {
        // Symbol-vs-string pin: any consumer that hand-wrote `"Process"`
        // pre-lift routes through this const post-lift.
        assert_eq!(PROCESS_KIND, "Process");
    }

    #[test]
    fn owner_reference_json_has_all_six_slots_present() {
        let v = owner_reference_json("my-process", "abc-uid");
        let obj = v.as_object().expect("owner reference is a JSON object");
        for k in [
            "apiVersion",
            "kind",
            "name",
            "uid",
            "controller",
            "blockOwnerDeletion",
        ] {
            assert!(obj.contains_key(k), "missing owner-reference slot: {k}");
        }
        assert_eq!(obj.len(), 6, "owner reference must have exactly 6 slots");
    }

    #[test]
    fn owner_reference_json_apiversion_routes_through_api_version_owner() {
        let v = owner_reference_json("x", "y");
        assert_eq!(v["apiVersion"], api_version());
    }

    #[test]
    fn owner_reference_json_kind_routes_through_process_kind_const() {
        let v = owner_reference_json("x", "y");
        assert_eq!(v["kind"], PROCESS_KIND);
    }

    #[test]
    fn owner_reference_json_stamps_supplied_name_and_uid() {
        let v = owner_reference_json("some-name", "some-uid");
        assert_eq!(v["name"], "some-name");
        assert_eq!(v["uid"], "some-uid");
    }

    #[test]
    fn owner_reference_json_controller_and_block_owner_deletion_are_true() {
        // These are structural — a Process-owned resource always
        // has a controlling reference that cascade-deletes with
        // the owner. A regression that flipped either boolean
        // would silently detach every emitted resource.
        let v = owner_reference_json("x", "y");
        assert_eq!(v["controller"], true);
        assert_eq!(v["blockOwnerDeletion"], true);
    }

    #[test]
    fn owner_reference_json_matches_hand_authored_shape_pre_lift() {
        // Byte-shape pin against the exact `json!({…})` incantation
        // every pre-lift call site restated. A regression that
        // reordered a slot, dropped one, or added a seventh here
        // surfaces at THIS pin rather than as a subtle SSA-apply
        // failure downstream when the K8s API server rejects the
        // OwnerReference on schema mismatch.
        let via_owner = owner_reference_json("p", "u");
        let hand_authored = json!({
            "apiVersion": "tatara.pleme.io/v1alpha1",
            "kind": "Process",
            "name": "p",
            "uid": "u",
            "controller": true,
            "blockOwnerDeletion": true,
        });
        assert_eq!(via_owner, hand_authored);
    }

    #[test]
    fn owner_reference_json_preserves_empty_name_and_uid_bytewise() {
        // The primitive does not guard against empty inputs — its
        // callers pre-lift did the empty-check upstream (both the
        // `edges.rs::build_owner_refs` and `render.rs::one_export_job`
        // sites gated on `!uid.is_empty()` before calling this composer,
        // and both now route through `owner_references_json` below;
        // `ssapply.rs::build_owner_reference` unwraps a required
        // `metadata.uid` via anyhow). The scalar composer owns
        // shape composition, not admission control; a downstream
        // rename that wants strict input validation lands as a
        // peer, not a change to the composer's contract.
        let v = owner_reference_json("", "");
        assert_eq!(v["name"], "");
        assert_eq!(v["uid"], "");
    }

    // ─── owner_references_json substrate pins ────────────────────────
    //
    // The 3-line `let mut owner_refs = vec![]; if !uid.is_empty()
    // { owner_refs.push(owner_reference_json(name, uid)); }` gate was
    // hand-authored at TWO sites in `tatara-reconciler`
    // (`edges::build_owner_refs` + `render::one_export_job`) before
    // this primitive existed, each restating the same optional-uid
    // posture that emits `[]` when the caller lacks a K8s-assigned
    // uid to point owners at. These pins bind the primitive at
    // fail-before-pass-after granularity so a regression that
    // inlined an owner reference for an empty uid — silently
    // detaching the resource from cascade-delete — surfaces HERE
    // rather than as an operator-visible ownerless resource after
    // apply, and a regression that added an owner reference of the
    // wrong SHAPE (a peer of `owner_reference_json` that swapped a
    // slot) surfaces via the composed-shape pin below rather than
    // as silent drift at every downstream emit site.

    #[test]
    fn owner_references_json_emits_single_entry_when_uid_present() {
        // The primary shape: a caller with a materialized uid gets
        // exactly one owner reference back — the pre-lift 3-line
        // `vec![]` + `push` gate collapses to this ONE call, and
        // the returned array is a direct-drop `ownerReferences`
        // slot value at every callsite.
        let refs = owner_references_json("demo-app", "abc-uid");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0]["kind"], PROCESS_KIND);
        assert_eq!(refs[0]["name"], "demo-app");
        assert_eq!(refs[0]["uid"], "abc-uid");
        // controller + blockOwnerDeletion routed through the scalar
        // composer — a regression that hand-composed the vec entry
        // rather than delegating would flip one of these booleans.
        assert_eq!(refs[0]["controller"], true);
        assert_eq!(refs[0]["blockOwnerDeletion"], true);
    }

    #[test]
    fn owner_references_json_emits_empty_when_uid_empty() {
        // The load-bearing gate — a pre-metadata Process (fixtured in
        // tests, or caught mid-Forking) has no admissible owner
        // reference to point at. Post-lift the gate lives at ONE
        // primitive so every emit site stamps `[]` uniformly rather
        // than one site accidentally emitting a placeholder-uid
        // owner reference the K8s GC would quietly detach from
        // cascade-delete.
        let refs = owner_references_json("demo-app", "");
        assert!(
            refs.is_empty(),
            "empty uid must produce zero owner references, not a placeholder-uid entry"
        );
    }

    #[test]
    fn owner_references_json_gates_on_uid_not_name() {
        // The gate axis is `uid`, not `name` — a Process with a
        // non-empty name but no uid still emits `[]` (the pre-metadata
        // shape), while a Process with a non-empty uid emits ONE
        // entry even when the name slot is empty (matching the
        // scalar composer's admission-control-free contract). Pin
        // both cross-diagonal combinations so a regression that
        // swapped the gate axis surfaces HERE rather than at every
        // downstream owner-refs consumer.
        assert!(
            owner_references_json("has-name", "").is_empty(),
            "empty uid gates to []; name presence is irrelevant"
        );
        let refs = owner_references_json("", "has-uid");
        assert_eq!(
            refs.len(),
            1,
            "empty name but present uid still emits one entry (name is not the gate)"
        );
        assert_eq!(refs[0]["name"], "");
        assert_eq!(refs[0]["uid"], "has-uid");
    }

    #[test]
    fn owner_references_json_matches_hand_authored_pre_lift_bytewise() {
        // Byte-identical parity with the exact pre-lift 3-line
        // `let mut owner_refs = vec![]; if !uid.is_empty() {
        // owner_refs.push(owner_reference_json(name, uid)); }` gate
        // across the two axis combinations every callsite plausibly
        // encounters. A regression that reordered the two branches,
        // dropped the gate, or reshaped the vec composition surfaces
        // HERE rather than at every downstream `ownerReferences`
        // slot pinned across `edges.rs` + `render.rs` tests.
        for (name, uid) in [
            ("demo-app", "uid-abc"),
            ("demo-app", ""),
            ("", "uid-abc"),
            ("", ""),
        ] {
            let via_primitive = owner_references_json(name, uid);

            // The pre-lift 3-line block, byte-for-byte.
            let mut hand_authored: Vec<serde_json::Value> = vec![];
            if !uid.is_empty() {
                hand_authored.push(owner_reference_json(name, uid));
            }

            assert_eq!(
                via_primitive, hand_authored,
                "owner_references_json must be byte-identical to the pre-lift 3-line gate on ({name:?}, {uid:?})"
            );
        }
    }

    #[test]
    fn owner_references_json_interpolates_cleanly_as_owner_refs_slot() {
        // Both callsites drop the returned vec directly under a
        // `"ownerReferences"` key inside a `json!({...})` block. Pin
        // the interop shape: a JSON-macro-wrapped Value carries the
        // primitive's output as a JSON array with the exact 6-slot
        // entries at each index. A regression that returned a
        // non-array (e.g. a single Value on the one-entry path,
        // requiring per-site vec-wrapping) surfaces HERE rather than
        // as a broken `metadata.ownerReferences` slot on every
        // emitted Ingress / DNSEndpoint / export Job.
        let refs = owner_references_json("demo-app", "abc-uid");
        let wrapped = json!({
            "metadata": {
                "name": "resource",
                "ownerReferences": refs,
            },
        });
        let owner_refs = &wrapped["metadata"]["ownerReferences"];
        assert!(
            owner_refs.is_array(),
            "ownerReferences must land as a JSON array"
        );
        assert_eq!(owner_refs.as_array().unwrap().len(), 1);
        assert_eq!(owner_refs[0]["kind"], PROCESS_KIND);

        // And the empty-uid path lands as an EMPTY array, not a
        // missing key or a null — matches the K8s API server's
        // expectation that the slot is either an array of entries
        // or absent, never a null.
        let empty_refs = owner_references_json("demo-app", "");
        let wrapped_empty = json!({
            "metadata": {
                "name": "resource",
                "ownerReferences": empty_refs,
            },
        });
        let owner_refs_empty = &wrapped_empty["metadata"]["ownerReferences"];
        assert!(owner_refs_empty.is_array());
        assert!(owner_refs_empty.as_array().unwrap().is_empty());
    }
}

#[cfg(test)]
mod qualified_process_ref_tests {
    //! Pin the [`qualified_process_ref`] composer at fail-before-
    //! pass-after granularity. The `<ns>/<name>` shape is the
    //! workspace-wide convention for a namespaced K8s resource
    //! reference — every downstream grep (the reconciler's
    //! `tatara.pleme.io/process` annotation reader, the
    //! [`crate::table::ClaimRecord.holder`] slot, the
    //! export-worker's receipt-owner filter, the reconciler's
    //! `PROCESS=<ref>` label-selector composer) depends on the
    //! two axes landing in `(ns, name)` order joined by a single
    //! `/` separator. A regression that swapped the axes, dropped
    //! either half, or renormalized the input surfaces HERE rather
    //! than as silent operator-facing drift at every downstream
    //! consumer.
    use super::qualified_process_ref;

    #[test]
    fn qualified_process_ref_joins_ns_and_name_with_slash() {
        // The invariant every downstream consumer composes against:
        // the qualified reference is EXACTLY `<ns>/<name>`, in that
        // order, joined by a single `/`.
        assert_eq!(
            qualified_process_ref("demo-ns", "ephemeral-demo"),
            "demo-ns/ephemeral-demo",
        );
    }

    #[test]
    fn qualified_process_ref_binds_positional_slots_by_axis_order() {
        // Positional pin — a copy-paste that swapped the two `&str`
        // arguments (both mechanically interchangeable at the type
        // level) would silently produce `<name>/<ns>` and break every
        // downstream grep keyed on the reference shape. Distinct
        // input slot values so a swap surfaces as an equality
        // failure rather than accidental identity.
        let out = qualified_process_ref("first-slot-ns", "second-slot-name");
        assert!(
            out.starts_with("first-slot-ns/"),
            "position 0 must be the namespace slot: got {out}"
        );
        assert!(
            out.ends_with("/second-slot-name"),
            "position 1 must be the name slot: got {out}"
        );
    }

    #[test]
    fn qualified_process_ref_accepts_string_deref_and_str_slice_shapes() {
        // Consumers split across two callsite shapes: owned
        // `String` locals (via deref coercion), bare `&str` slices,
        // and mixed provenance. Every shape must ride cleanly
        // through the same 2-arg signature — matches every current
        // pre-lift caller in `tatara-export-worker` (CLI-arg driven
        // owned strings + `&str` from a struct field) and in
        // `tatara-reconciler` (owned locals + function-param
        // slices).
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
        assert_eq!(
            qualified_process_ref(&owned_ns, borrowed_name),
            "owned-ns/borrowed-app",
        );
    }

    #[test]
    fn qualified_process_ref_rides_edge_case_axis_shapes() {
        // The composer shapes the two axes as arbitrary strings —
        // no length/character validation happens at the composer,
        // so any shape a Process's `metadata.namespace` /
        // `metadata.name` can hold rides through unchanged. Pin
        // the empty-string cases (unnamed process pre-metadata,
        // cluster-scoped `namespace = ""` fallback), and the
        // whitespace-and-slash-in-name pathological case (a
        // regression that URL-escaped or path-normalized the input
        // at this primitive would silently break every downstream
        // grep).
        assert_eq!(qualified_process_ref("", ""), "/");
        assert_eq!(qualified_process_ref("default", ""), "default/");
        assert_eq!(qualified_process_ref("", "orphan"), "/orphan");
        assert_eq!(
            qualified_process_ref("weird ns", "with/slash"),
            "weird ns/with/slash",
        );
    }

    #[test]
    fn qualified_process_ref_composes_from_process_coordinates_or_defaults() {
        // The primary Process-driven callsite: a live
        // [`crate::prelude::Process`] with populated metadata
        // composes through
        // [`crate::prelude::Process::coordinates_or_defaults`] +
        // [`qualified_process_ref`]. Pin the composition so a
        // regression in either primitive that broke the `(ns,
        // name)` positional contract surfaces HERE rather than as
        // silent drift at every downstream reconciler / export-
        // worker / pool-reconciler consumer.
        use crate::classification::{Classification, ConvergencePointType, SubstrateType};
        use crate::crd::{Process, ProcessSpec};
        let spec = ProcessSpec {
            identity: Default::default(),
            classification: Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
                horizon: Default::default(),
                calm: Default::default(),
                data_classification: Default::default(),
            },
            intent: Default::default(),
            boundary: Default::default(),
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Default::default(),
            routing: None,
            encapsulates: None,
            suspended: false,
        };
        let mut p = Process::new("ephemeral-demo", spec);
        p.metadata.namespace = Some("demo-ns".into());
        let (ns, name) = p.coordinates_or_defaults();
        assert_eq!(
            qualified_process_ref(ns, name),
            "demo-ns/ephemeral-demo",
            "coordinates_or_defaults + qualified_process_ref must \
             compose to the canonical <ns>/<name> shape"
        );
    }

    #[test]
    fn qualified_process_ref_matches_hand_authored_pre_lift_bytewise() {
        // Byte-identical parity with the exact pre-lift
        // `format!("{ns}/{name}")` incantation. A regression that
        // reshaped the separator, reordered the axes, or dropped
        // either half surfaces HERE rather than at every downstream
        // annotation / claim-key / run-id consumer. Sweeps every
        // shape combination the pre-lift callers plausibly
        // encountered.
        for (ns, name) in [
            ("demo-ns", "ephemeral-demo"),
            ("", ""),
            ("default", ""),
            ("", "orphan"),
        ] {
            let via_primitive = qualified_process_ref(ns, name);
            let hand_authored = format!("{ns}/{name}");
            assert_eq!(
                via_primitive, hand_authored,
                "qualified_process_ref must be byte-identical to \
                 the pre-lift `format!(\"{{ns}}/{{name}}\")` \
                 hand-authored shape on ({ns:?}, {name:?})"
            );
        }
    }
}

// ── Lisp → ProcessSpec compile bridge ──────────────────────────────────
//
// `(defpoint NAME :k v …)` compiles to a `NamedDefinition<ProcessSpec>`.
// The derive on ProcessSpec handles every field via the serde Deserialize
// fallthrough — no hand-rolled keyword parsing needed.

/// A named ProcessSpec as produced by `compile_source`.
pub type Definition = tatara_lisp::NamedDefinition<crate::crd::ProcessSpec>;

/// Compile a Lisp source string into a list of named ProcessSpecs.
/// Each top-level `(defpoint NAME …)` form becomes one `Definition`.
pub fn compile_source(src: &str) -> tatara_lisp::Result<Vec<Definition>> {
    tatara_lisp::compile_named::<crate::crd::ProcessSpec>(src)
}

/// Register every domain owned by this crate with the global Lisp
/// dispatcher. Call once per binary, typically near the top of `main`.
/// After this call, `tatara_lisp::domain::lookup("defpoint")` and
/// `lookup("defephemeral")` both resolve to the right typed compiler.
///
/// Idempotent — registering the same type twice is a no-op.
pub fn register_all() {
    tatara_lisp::domain::register::<crate::crd::ProcessSpec>();
    tatara_lisp::domain::register::<crate::ephemeral::EphemeralSpec>();
}

#[cfg(test)]
mod compile_tests {
    use super::compile_source;
    use crate::classification::{ConvergencePointType, SubstrateType};
    use crate::compliance::VerificationPhase;
    use crate::spec::MustReachPhase;

    /// The full derive-powered pipeline — no hand-rolled parsing anywhere.
    /// Every field travels: Lisp → Sexp → serde_json → typed ProcessSpec.
    #[test]
    fn full_processspec_round_trip_via_derive() {
        let src = r#"
            (defpoint observability-stack
              :identity       (:parent "seph.1")
              :classification (:point-type Gate
                               :substrate Observability
                               :horizon (:kind Bounded)
                               :calm Monotone
                               :data-classification Internal)
              :intent         (:nix (:flake-ref "github:pleme-io/k8s"
                                     :attribute "observability"
                                     :attic-cache "main"))
              :boundary       (:postconditions
                                 ((:kind KustomizationHealthy
                                   :params (:name "observability-stack"
                                            :namespace "flux-system"))
                                  (:kind PromQL
                                   :params (:query "up == 1")))
                               :timeout "15m")
              :compliance     (:baseline "fedramp-moderate"
                               :bindings ((:framework "nist-800-53"
                                           :control-id "SC-7"
                                           :phase AtBoundary)))
              :depends-on     ((:name "secret-injection" :must-reach Attested))
              :signals        (:sigterm-grace-seconds 480
                               :sighup-strategy Reconverge))
        "#;
        let defs = compile_source(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        assert_eq!(d.name, "observability-stack");

        // identity
        assert_eq!(d.spec.identity.parent.as_deref(), Some("seph.1"));

        // classification (enums deserialized via symbol → string)
        assert_eq!(d.spec.classification.point_type, ConvergencePointType::Gate);
        assert_eq!(
            d.spec.classification.substrate,
            SubstrateType::Observability
        );

        // intent (tagged-union with one of four options)
        let nix = d.spec.intent.nix.as_ref().expect("nix intent");
        assert_eq!(nix.flake_ref, "github:pleme-io/k8s");
        assert_eq!(nix.attribute, "observability");
        assert_eq!(nix.attic_cache.as_deref(), Some("main"));

        // boundary (Vec<nested struct with params object>)
        assert_eq!(d.spec.boundary.postconditions.len(), 2);
        assert_eq!(d.spec.boundary.timeout.as_deref(), Some("15m"));

        // compliance (Vec<binding with enum phase>)
        assert_eq!(
            d.spec.compliance.baseline.as_deref(),
            Some("fedramp-moderate")
        );
        assert_eq!(d.spec.compliance.bindings.len(), 1);
        assert_eq!(
            d.spec.compliance.bindings[0].phase,
            VerificationPhase::AtBoundary
        );

        // depends_on (Vec<struct with enum>)
        assert_eq!(d.spec.depends_on.len(), 1);
        assert_eq!(d.spec.depends_on[0].must_reach, MustReachPhase::Attested);

        // signals (numeric + enum defaults)
        assert_eq!(d.spec.signals.sigterm_grace_seconds, 480);
    }

    #[test]
    fn missing_required_field_errors() {
        // `:classification` has no #[serde(default)] — omit it and compile must fail.
        let src = r#"(defpoint x :intent (:nix (:flake-ref "f" :attribute "a")))"#;
        assert!(compile_source(src).is_err());
    }

    #[test]
    fn serde_default_fields_are_optional() {
        // Omit every #[serde(default)] field — compile must succeed because
        // the derive honors serde defaults.
        let src = r#"
            (defpoint x
              :classification (:point-type Transform :substrate Compute)
              :intent (:flux (:git-repository "g" :path ".")))
        "#;
        let defs = compile_source(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        assert!(d.spec.depends_on.is_empty());
        assert!(d.spec.boundary.postconditions.is_empty());
        assert!(d.spec.compliance.bindings.is_empty());
        assert!(!d.spec.suspended);
        // Lifetime defaults to Permanent (no variant set, resolver still works).
        assert!(d.spec.lifetime.is_default());
        assert!(!d.spec.lifetime.is_ephemeral());
    }

    /// Registering all process-owned domains is idempotent and resolves
    /// both `defpoint` (ProcessSpec) and `defephemeral` (EphemeralSpec).
    #[test]
    fn register_all_resolves_defpoint_and_defephemeral() {
        use tatara_lisp::domain::lookup;
        super::register_all();
        super::register_all(); // idempotent
        assert!(lookup("defpoint").is_some(), "defpoint must resolve");
        assert!(
            lookup("defephemeral").is_some(),
            "defephemeral must resolve"
        );
    }

    /// End-to-end: a `(defpoint …)` form may carry the full ephemeral
    /// shape directly — `:intent (:aplicacao …)` + `:lifetime (:ephemeral …)`.
    /// This is what the `(defephemeral …)` sugar lowers to via `From`.
    #[test]
    fn defpoint_with_aplicacao_intent_and_ephemeral_lifetime() {
        use crate::intent::IntentVariant;
        use crate::lifetime::{LifetimeVariant, TeardownPolicy};
        let src = r#"
            (defpoint closed-loop-attest
              :classification (:point-type Gate :substrate Compute)
              :intent (:aplicacao
                        (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-demo-app"
                         :version "0.5.5"
                         :profile "all-in-one"
                         :values-overlay (:cluster (:name "ephemeral-test-01"))
                         :target-namespace "demo-test"))
              :boundary (:postconditions
                          ((:kind HelmReleaseReleased
                            :params (:name "demo-app-consolidated"
                                     :namespace "demo-test"))
                           (:kind ClosedLoopAuth
                            :params (:issuer (:service "demo-app-issuer" :port 8080)
                                     :consumer (:service "demo-app-gateway" :port 8000)
                                     :probeImage "ghcr.io/pleme-io/closed-loop-probe:0.1.0"))))
              :lifetime (:ephemeral (:ttl "1h"
                                     :teardown-policy OnAttested
                                     :max-concurrent 1)))
        "#;
        let defs = compile_source(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];

        // Aplicacao intent landed.
        match d.spec.intent.variant().unwrap() {
            IntentVariant::Aplicacao(a) => {
                assert_eq!(a.profile, "all-in-one");
                assert_eq!(a.version, "0.5.5");
                assert_eq!(a.target_namespace.as_deref(), Some("demo-test"));
                assert_eq!(a.values_overlay["cluster"]["name"], "ephemeral-test-01");
            }
            other => panic!("expected Aplicacao, got {other:?}"),
        }

        // Ephemeral lifetime landed with the right teardown policy.
        match d.spec.lifetime.variant().unwrap() {
            LifetimeVariant::Ephemeral(e) => {
                assert_eq!(e.ttl, "1h");
                assert_eq!(e.teardown_policy, TeardownPolicy::OnAttested);
                assert_eq!(e.max_concurrent, 1);
            }
            other => panic!("expected ephemeral, got {other:?}"),
        }

        // Two typed postconditions including ClosedLoopAuth.
        assert_eq!(d.spec.boundary.postconditions.len(), 2);
        assert_eq!(
            d.spec.boundary.postconditions[1].kind,
            crate::boundary::ConditionKind::ClosedLoopAuth
        );
    }
}
