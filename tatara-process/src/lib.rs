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
    pub use crate::{Annotated, DeletionTombstoned, NamespacedApiCoordinates};
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

/// Substrate-primitive trait for the **`Api::namespaced`-shaped
/// coordinate extraction** every tatara-CRD reconciler restated by
/// hand at its top-level `reconcile` dispatcher: pull owned `String`
/// forms of `metadata.namespace` and `metadata.name` and refuse to
/// substitute a workspace-wide default for either slot, because the
/// caller is about to feed the pair positionally into
/// `Api::namespaced(client, &ns)` + `Api::patch(&name, …)` and the
/// K8s API server refuses an empty-string name / namespace path
/// segment.
///
/// Pre-lift the 5-line `.metadata.<slot>.clone().ok_or_else(||
/// anyhow!("<Kind> has no metadata.<slot>"))?` chain (paired at both
/// slots inside every controller's `reconcile_inner`) was hand-
/// authored at TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// threshold in `tatara-pool-reconciler`, each restating the SAME
/// (`namespace` errors, then `name` errors, both owned `String`)
/// contract on a different CRD:
/// * `controller_pool::reconcile_inner` — the pool reconciler's
///   top-level `Pool has no metadata.{namespace,name}` gate,
///   funneling every subsequent `Api::namespaced` + `Api::patch` call
///   through the extracted `(ns, name)` pair.
/// * `controller_allocation::reconcile_inner` — the allocation
///   reconciler's peer gate on `EphemeralAllocation`, funneling the
///   `Api::namespaced` + `Api::patch_status` calls that follow.
///
/// Both sites walked the SAME 5-line paired chain and both wanted the
/// `(String, String)` form the primitive returns — because the
/// produced `ns` outlives the source-object borrow (it feeds
/// `Api::namespaced(client, &ns)` and later log-line interpolations
/// across a stretch of `.await` points) and the `name` similarly
/// threads through `Api::patch(&name, …)` calls downstream. Post-lift
/// each callsite reads `pool.owned_coordinates_required()?` /
/// `alloc.owned_coordinates_required()?` and the produced tuple
/// destructures into the same downstream slots unchanged.
///
/// The blanket impl over `kube::Resource<DynamicType = ()>` (which
/// every `#[derive(CustomResource)]`-generated tatara CRD satisfies)
/// closes the substrate corner ONCE for the entire workspace: adding
/// a third or fourth CRD in a peer crate — a routing-edge object, a
/// receipt registry — inherits the extractor for free at its own
/// `reconcile_inner` dispatcher with zero per-CRD lift work. This is
/// the direction the CSE Compounding Directive names by
/// "solve once, load-bearing fixes only": the primitive lands once
/// and every downstream controller pattern-matches into it without
/// re-authoring the chain.
///
/// Peer to [`crate::prelude::Process::owned_coordinates_or_err`] on
/// the (`Process`-specific × namespace-required) axis pair — the two
/// primitives partition the workspace's owned-form coordinate
/// extraction on the `namespace-required` axis and cover the
/// per-CRD needs they were opened for:
///
/// * ns-defaulted, name-required, `Process`-inherent →
///   [`crate::prelude::Process::owned_coordinates_or_err`]
///   (`tatara-reconciler`'s `phase_machine` / `signals` callers —
///   consumers whose downstream tolerates the workspace's
///   [`crate::prelude::Process::DEFAULT_NAMESPACE`] substitute for a
///   `Process` fixtured pre-namespace-defaulting).
/// * ns-required + name-required, blanket over every CRD → **this
///   method** (`tatara-pool-reconciler`'s pool + allocation reconciler
///   callers — consumers whose downstream refuses BOTH substitutions
///   because the `Api::namespaced` dispatcher expects a real path
///   segment on each axis and the enclosing controller is not
///   authored to run against a namespace-less pool / allocation).
///
/// The error strings are shaped as `"{Kind} has no metadata.{slot}"`
/// with `{Kind}` pulled positionally from `Self::kind(&())` (the
/// kube-rs canonical CRD kind — `"EphemeralPool"` / `"EphemeralAllocation"`
/// — which matches `kubectl get ephemeralpools|ephemeralallocations`
/// output verbatim rather than the pre-lift `"Pool"` / `"Allocation"`
/// short-forms every callsite hard-coded by hand). Routing the type
/// name through `Self::kind` closes the drift path where a future
/// CRD rename or a copy-paste consumer inherited the wrong short-
/// form; the K8s-kind spelling is the ONE canonical name every
/// operator-facing surface (kubectl output, RBAC subject strings,
/// audit-log entries) already uses, so a log-line consumer greppping
/// for either kind hits the primitive's canonical spelling directly.
///
/// A future normalization step (a per-CRD namespace canonicalization
/// pass — case-fold, unicode-safe path-segment validation, a shared
/// [`crate::prelude::Process::DEFAULT_NAMESPACE`]-aware fallback
/// mode gated by an argument) lands at ONE substrate trait method
/// here and every downstream reconciler picks up the upgrade
/// mechanically — no per-callsite hand-edit at `controller_pool` /
/// `controller_allocation` / any future CRD's `reconcile_inner`.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the paired 5-line `.metadata.<slot>.clone().ok_or_else` chain
/// recurred at two hand-authored sites past the ★★ PRIME-DIRECTIVE
/// ≥ 2 duplication trigger, and is lifted onto ONE trait method
/// here). THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — the pins bind the missing-namespace corner, the
/// missing-name corner, the missing-both corner (namespace error
/// wins), the both-slots-present happy path, AND the
/// `Self::kind`-driven error-string spelling per CRD, so a
/// regression that reordered the two `ok_or_else` gates or drifted
/// the error prefix surfaces at `tests::owned_coordinates_required_*`
/// rather than as silent operator-facing skew between the two
/// reconcilers' top-level error-message shapes).
pub trait NamespacedApiCoordinates: kube::Resource<DynamicType = ()> {
    /// Extract the K8s API path coordinates as owned `String`s,
    /// erroring with a `Self::kind`-prefixed message when either
    /// slot is absent. See the trait-level docs for the axis-family
    /// context, peer primitives, and future-normalization anchor.
    fn owned_coordinates_required(&self) -> anyhow::Result<(String, String)> {
        let meta = self.meta();
        let ns = meta
            .namespace
            .clone()
            .ok_or_else(|| anyhow::anyhow!("{} has no metadata.namespace", Self::kind(&())))?;
        let name = meta
            .name
            .clone()
            .ok_or_else(|| anyhow::anyhow!("{} has no metadata.name", Self::kind(&())))?;
        Ok((ns, name))
    }
}

impl<T> NamespacedApiCoordinates for T where T: kube::Resource<DynamicType = ()> {}

/// Substrate-primitive trait for the **deletion-tombstone presence
/// probe** every tatara CRD reconciler restated as
/// `.metadata.deletion_timestamp.is_some()` on the K8s-API-server-
/// stamped `metadata.deletionTimestamp` slot: a `true` reading means
/// the API server has accepted a DELETE and finalizers are draining
/// (the object is still live but the controller must move into its
/// SIGTERM cascade / DELETE-skip branch), while a `false` reading
/// means no delete is in flight.
///
/// Pre-lift the ONE-line `.metadata.deletion_timestamp.is_some()`
/// chain was hand-authored across every tatara-process CRD in
/// consumer crates and independently re-authored as byte-identical
/// inherent methods on [`crate::prelude::Process`] +
/// [`crate::prelude::EphemeralPool`], with the sister CRD
/// [`crate::prelude::EphemeralAllocation`] still on the raw chain in
/// `tatara-pool-reconciler::allocation_decide::AllocationConvergenceCtx::observe`.
/// That's TWO byte-identical inherent implementations past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold on the substrate side
/// PLUS the hand-authored chain on the third CRD — three surfaces
/// spelling the SAME projection, each with the same drift risk (a
/// stale-tombstone grace-period gate, a paused-controller
/// canonicalization, a cross-cluster clock-skew guard would have to
/// land at every surface plus stay coherent).
///
/// Post-lift the substrate owns the probe at ONE trait method with a
/// blanket impl over every `kube::Resource<DynamicType = ()>`, so:
/// * [`crate::prelude::EphemeralAllocation`] inherits the probe for
///   free — its `allocation_decide.rs` hand-authored chain routes
///   through `alloc.is_being_deleted()` post-lift, closing the
///   third-CRD gap noted in the [`crate::prelude::EphemeralPool::is_being_deleted`]
///   commit body (`7f8f104`).
/// * Any future tatara CRD (a routing-edge object, a receipt
///   registry, a fleet-wide claim registry) inherits the probe at
///   its own `reconcile_inner` dispatcher with zero per-CRD lift
///   work — the same solve-once discipline
///   [`NamespacedApiCoordinates`] established for the paired
///   coordinate extractor.
///
/// The two existing inherent methods
/// ([`crate::prelude::Process::is_being_deleted`] +
/// [`crate::prelude::EphemeralPool::is_being_deleted`]) are peers
/// rather than lift casualties: Rust method resolution prefers the
/// inherent over the trait's blanket, so every existing callsite
/// keeps hitting the same code path. The trait's blanket impl
/// closes the substrate corner for CRDs WITHOUT the inherent — the
/// coherence tests pin that the trait and the two inherents produce
/// byte-identical results across every corner of the (missing,
/// present) input matrix, so a future rewrite that consolidates
/// onto the trait doesn't skew any consumer.
///
/// Return-form axis: `bool` matches the copy-form discipline of the
/// two inherent peers and of [`crate::prelude::Process::observed_phase`]
/// — the underlying wire-format slot is an `Option<Time>` carrying
/// only presence information at this axis (the RFC-3339 timestamp
/// payload itself is not what the callers read; all just probe
/// presence to detect the tombstone-stamped state).
///
/// A future normalization step (a per-tombstone staleness gate
/// returning `false` for a tombstone older than the reconciler's
/// grace-period budget, a paused-controller tombstone
/// canonicalization, a cross-cluster tombstone-observation clock
/// skew guard) lands at ONE substrate trait method here — the two
/// inherent forwarders inherit the upgrade mechanically if they are
/// rewired to `<Self as DeletionTombstoned>::is_being_deleted(self)`
/// as a follow-up sweep, and every downstream consumer that already
/// routes through this trait picks it up without a per-callsite
/// hand-edit.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the `.metadata.deletion_timestamp.is_some()` projection recurred
/// as TWO byte-identical inherent implementations on
/// [`crate::prelude::Process`] + [`crate::prelude::EphemeralPool`]
/// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger and is
/// lifted onto ONE trait method here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — the pins bind the missing-
/// tombstone corner + the present-tombstone corner + the copy-form
/// `bool` return + the byte-identical parity with the pre-lift
/// `.is_some()` chain + cross-CRD coherence with both inherent
/// forwarders on the SAME `Process` / `EphemeralPool` value, so a
/// regression that skewed either surface surfaces at
/// `deletion_tombstoned_tests::*` rather than as silent operator-
/// facing skew between the top-level dispatcher's SIGTERM preempt,
/// the SIGTERM cascade's child-fan-out DELETE-skip, the pool
/// reconciler's Drain gate, and the allocation reconciler's release
/// short-circuit on three sibling CRDs.
pub trait DeletionTombstoned: kube::Resource<DynamicType = ()> {
    /// True iff the K8s API server has stamped `metadata.deletionTimestamp`
    /// on this resource — a DELETE is in flight and finalizers are
    /// draining. See the trait-level docs for the axis-family context,
    /// peer inherent methods, and future-normalization anchor.
    fn is_being_deleted(&self) -> bool {
        self.meta().deletion_timestamp.is_some()
    }
}

impl<T> DeletionTombstoned for T where T: kube::Resource<DynamicType = ()> {}

/// Substrate-primitive trait for the ONE **borrow-form annotation
/// lookup** every tatara CRD reconciler restated as the 3-line
/// `.metadata.annotations.as_ref().and_then(|m| m.get(key)).map(String::as_str)`
/// chain (or a `.cloned()` / `.cloned().unwrap_or_default()` variant
/// of the same shape) on the K8s `metadata.annotations` map: returns
/// `Some(&str)` iff the annotations block is present AND the key is
/// present inside it; both missing corners collapse to `None`.
///
/// Peer to [`DeletionTombstoned`] + [`NamespacedApiCoordinates`] on
/// the substrate-primitive-trait axis (kube-Resource blanket impls
/// over `DynamicType = ()`), and peer to the pre-existing
/// [`crate::prelude::Process::annotation`] inherent forwarder on the
/// axis of "one annotation-lookup shape shared across every kube
/// resource, tatara CRD or K8s built-in". The inherent stays as a
/// peer — Rust method resolution prefers an inherent over a trait's
/// blanket impl, so the three consumers already routed through
/// `Process::annotation`
/// (`tatara-reconciler::signals::ingest`,
/// `tatara-reconciler::phase_machine::released_from_annotation`,
/// `tatara-pool-reconciler::controller_pool::process_belongs_to_pool`)
/// keep hitting the byte-identical code path — and the trait's
/// blanket impl closes the substrate corner for kube resources
/// WITHOUT the inherent: post-lift the hand-authored
/// `.metadata.annotations.as_ref().and_then(|m| m.get(KEY))...` chain
/// in `tatara-export-worker::main` (on `k8s_openapi`'s `ConfigMap`,
/// which has no tatara-owned inherent) routes through the trait at
/// `cm.annotation(KEY)`, and any future `EphemeralPool` /
/// `EphemeralAllocation` (or new tatara CRD) consumer that needs an
/// annotation lookup inherits the primitive for free — the same
/// solve-once discipline the two peer traits already established.
///
/// Return-form axis: `Option<&str>` matches the borrow-first
/// discipline of the peer metadata primitives
/// ([`crate::prelude::Process::namespace_or_default`],
/// [`crate::prelude::Process::name_or_placeholder`],
/// [`crate::prelude::Process::coordinates_or_none`], and the inherent
/// [`crate::prelude::Process::annotation`] this trait mirrors). The
/// two corners the pre-lift chain swallowed (missing `annotations`
/// map, missing key inside the map) BOTH collapse to `None` so
/// `.is_some()` / `if let Some(_)` / `Option::map` behave identically
/// on a resource whose annotations block is `None` and on one whose
/// annotations block is populated but omits the key — matching what
/// the pre-lift `.and_then(...)` chain produced.
///
/// A future normalization step (a key-canonicalization pass, a
/// case-fold lookup, a per-key alias table for renamed annotations
/// across API versions, a per-namespace override substrate) lands at
/// ONE trait method here and every downstream consumer — the four
/// current sites plus every future CRD reconciler that inherits the
/// blanket impl — picks up the upgrade mechanically. If the inherent
/// is ever rewired to `<Self as Annotated>::annotation(self, key)`
/// as a follow-up sweep, the three inherent-preferred callsites
/// automatically inherit any trait-level upgrade too.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the annotation-lookup shape recurred as ONE inherent forwarder on
/// `Process` PLUS a hand-authored chain on `ConfigMap` past the
/// ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted onto
/// ONE trait method here). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — the pins bind the missing-annotations corner +
/// the missing-key corner + the borrow-form `&str` lifetime + the
/// byte-identical parity with the pre-lift 3-line chain + the
/// cross-primitive coherence with `Process::annotation` on the SAME
/// `Process` value, so a regression that skewed either surface
/// surfaces at `annotated_tests::*` rather than as silent operator-
/// facing skew between the SIGNAL / RELEASED_FROM / POOL annotation
/// readers on Process and the receipts-owner filter on ConfigMap).
pub trait Annotated: kube::Resource<DynamicType = ()> {
    /// Borrow one key from `metadata.annotations`. See the trait-level
    /// docs for the axis-family context, peer inherent method, and
    /// future-normalization anchor.
    fn annotation(&self, key: &str) -> Option<&str> {
        self.meta()
            .annotations
            .as_ref()
            .and_then(|m| m.get(key))
            .map(String::as_str)
    }
}

impl<T> Annotated for T where T: kube::Resource<DynamicType = ()> {}

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
    /// Stamped by `tatara-pool-reconciler::controller_allocation::
    /// reconcile` on the member `Process` at the moment an
    /// `EphemeralAllocation` transitions Queued → Bound. Value is
    /// the requestor Allocation's `<ns>/<name>` qualified reference
    /// (composed through the same `<ns>/<name>` shape every peer
    /// substrate composer routes through — see
    /// [`crate::qualified_process_ref`]). Downstream consumers
    /// (operator dashboards, admission webhooks, audit-trail
    /// scrapers) grep for this key to answer "which allocator drove
    /// this member Process into its ephemeral overlay".
    pub const REQUESTOR: &str = "tatara.pleme.io/requestor";
    /// Peer to [`REQUESTOR`] on the same allocator-bind axis: the
    /// bare Allocation name (no namespace prefix), stamped alongside
    /// so downstream consumers that key on the Allocation identity
    /// alone (a single-namespace UI, an in-cluster label selector
    /// that already carries the namespace) don't need to re-split
    /// [`REQUESTOR`]'s composed reference.
    pub const ALLOCATION: &str = "tatara.pleme.io/allocation";
    /// Peer to [`REQUESTOR`] + [`ALLOCATION`] on the same
    /// allocator-bind axis: mirrors
    /// [`crate::allocation::RequestorRef.kind`] verbatim onto the
    /// bound member Process so consumers that dispatch on the
    /// requestor-kind axis (a GitHub-PR-scoped webhook, a
    /// scheduler-window scoped fairness gate, a per-kind quota
    /// enforcer) never have to fetch the Allocation object again.
    pub const REQUESTOR_KIND: &str = "tatara.pleme.io/requestor-kind";
    /// Stamped by `tatara-pool-reconciler::controller_pool::
    /// build_member_process` on every Process the pool controller
    /// materializes into a pool slot. Value is the owning
    /// [`crate::pool::EphemeralPool`]'s `metadata.name`; the pool
    /// controller's `process_belongs_to_pool` membership gate reads
    /// this key back through the substrate primitive
    /// [`crate::prelude::Process::annotation`] to filter its owned
    /// members out of the cluster-wide Process listing. Peer to
    /// [`POOL_SLOT`] on the same pool-membership axis; the two keys
    /// travel together at every write site so any future rename (a
    /// `tatara.pleme.io/v2/pool` migration, an alias table for
    /// cross-cluster pool identity, a per-cluster ownership prefix)
    /// lands at ONE `pub const` in the substrate and every
    /// downstream consumer (the pool reconciler's membership gate,
    /// any future observability label emitter, a cross-namespace
    /// pool-topology walker) inherits the upgrade mechanically.
    pub const POOL: &str = "tatara.pleme.io/pool";
    /// Peer to [`POOL`] on the same pool-membership axis: the
    /// zero-based slot index the pool controller assigned to the
    /// member Process, stamped alongside so downstream consumers
    /// that need per-slot identity (a UI grid layout, a per-slot
    /// affinity gate, a slot-scoped audit-trail scraper) can
    /// dispatch on it without re-scanning the pool controller's
    /// naming scheme. Value is the slot's `u32` rendered through
    /// `.to_string()`.
    pub const POOL_SLOT: &str = "tatara.pleme.io/pool-slot";
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

#[cfg(test)]
mod namespaced_api_coordinates_tests {
    //! Pin the [`NamespacedApiCoordinates`] trait's
    //! `owned_coordinates_required` extractor at fail-before-pass-
    //! after granularity across every corner of the (namespace slot,
    //! name slot) × (present, absent) input matrix, on BOTH CRDs the
    //! trait's blanket impl covers today (`EphemeralPool` +
    //! `EphemeralAllocation`). A regression that reordered the two
    //! `ok_or_else` gates, dropped the `Self::kind` prefix, or drifted
    //! the error-string spelling surfaces HERE rather than as silent
    //! operator-facing skew between the two reconcilers' top-level
    //! error messages.
    use super::NamespacedApiCoordinates;
    use crate::allocation::{AllocationSpec, EphemeralAllocation, Requestor};
    use crate::ephemeral::EphemeralSpec;
    use crate::intent::AplicacaoIntent;
    use crate::lifetime::TeardownPolicy;
    use crate::pool::{EphemeralPool, PoolSelector, PoolSpec, ReturnPolicy};

    fn empty_template() -> EphemeralSpec {
        // Mirror `tatara-pool-reconciler::router::tests::empty_template`
        // — the workspace-wide minimal `EphemeralSpec` fixture the sister
        // reconciler tests already use for pool wiring exercised here.
        EphemeralSpec {
            aplicacao: AplicacaoIntent {
                chart_ref: "oci://x".into(),
                version: "1".into(),
                profile: String::new(),
                values_overlay: serde_json::Value::Null,
                release_name: None,
                target_namespace: None,
                install_timeout: None,
            },
            ttl: "1h".into(),
            teardown: TeardownPolicy::Always,
            max_concurrent: 0,
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: None,
            exports: vec![],
            routing: None,
        }
    }

    fn pool_fixture(name: &str, ns: Option<&str>) -> EphemeralPool {
        let spec = PoolSpec {
            desired_size: 1,
            min_size: 0,
            max_size: 0,
            return_policy: ReturnPolicy::Replace,
            selector: PoolSelector::default(),
            template: empty_template(),
            free_ttl: "24h".into(),
            max_allocation_ttl: "4h".into(),
            desired: 0,
            replacement_policy: Default::default(),
            stable_name_claim: false,
        };
        let mut p = EphemeralPool::new(name, spec);
        p.metadata.namespace = ns.map(str::to_string);
        p
    }

    fn alloc_fixture(name: &str, ns: Option<&str>) -> EphemeralAllocation {
        let spec = AllocationSpec {
            pool_ref: None,
            requestor: Requestor {
                kind: "github-pr".into(),
                repo: None,
                branch: None,
                pr_number: None,
                sha: None,
                pr_labels: vec![],
                actor: None,
            },
            ttl: None,
            note: None,
        };
        let mut a = EphemeralAllocation::new(name, spec);
        a.metadata.namespace = ns.map(str::to_string);
        a
    }

    fn nameless_pool(ns: Option<&str>) -> EphemeralPool {
        let mut p = pool_fixture("placeholder", ns);
        p.metadata.name = None;
        p
    }

    fn nameless_alloc(ns: Option<&str>) -> EphemeralAllocation {
        let mut a = alloc_fixture("placeholder", ns);
        a.metadata.name = None;
        a
    }

    // ── Happy path: both slots present ─────────────────────────────

    #[test]
    fn owned_coordinates_required_returns_owned_strings_on_ephemeral_pool_when_both_slots_present()
    {
        let p = pool_fixture("attest-pool", Some("ephemeral-pools"));
        let (ns, name) = p.owned_coordinates_required().unwrap();
        assert_eq!(ns, "ephemeral-pools");
        assert_eq!(name, "attest-pool");
    }

    #[test]
    fn owned_coordinates_required_returns_owned_strings_on_ephemeral_allocation_when_both_slots_present(
    ) {
        let a = alloc_fixture("pr-42-demo", Some("ephemeral-pools"));
        let (ns, name) = a.owned_coordinates_required().unwrap();
        assert_eq!(ns, "ephemeral-pools");
        assert_eq!(name, "pr-42-demo");
    }

    // ── Missing namespace ─────────────────────────────────────────

    #[test]
    fn owned_coordinates_required_errors_on_ephemeral_pool_missing_namespace() {
        let p = pool_fixture("attest-pool", None);
        let err = p.owned_coordinates_required().unwrap_err();
        assert_eq!(err.to_string(), "EphemeralPool has no metadata.namespace");
    }

    #[test]
    fn owned_coordinates_required_errors_on_ephemeral_allocation_missing_namespace() {
        let a = alloc_fixture("pr-42-demo", None);
        let err = a.owned_coordinates_required().unwrap_err();
        assert_eq!(
            err.to_string(),
            "EphemeralAllocation has no metadata.namespace"
        );
    }

    // ── Missing name ──────────────────────────────────────────────

    #[test]
    fn owned_coordinates_required_errors_on_ephemeral_pool_missing_name_when_namespace_present() {
        let p = nameless_pool(Some("ephemeral-pools"));
        let err = p.owned_coordinates_required().unwrap_err();
        assert_eq!(err.to_string(), "EphemeralPool has no metadata.name");
    }

    #[test]
    fn owned_coordinates_required_errors_on_ephemeral_allocation_missing_name_when_namespace_present(
    ) {
        let a = nameless_alloc(Some("ephemeral-pools"));
        let err = a.owned_coordinates_required().unwrap_err();
        assert_eq!(err.to_string(), "EphemeralAllocation has no metadata.name");
    }

    // ── Missing both slots: namespace error wins (pre-lift ordering) ──

    #[test]
    fn owned_coordinates_required_reports_namespace_first_when_both_slots_absent_on_ephemeral_pool()
    {
        // Pre-lift both reconcilers spelled the paired chain as the
        // namespace ok_or_else THEN the name ok_or_else, so the
        // reported error on a fixture missing both slots was always
        // the namespace one. Pin that ordering post-lift so a
        // regression that swapped the two `ok_or_else` blocks
        // surfaces HERE rather than at operator-facing log-line
        // grep drift between the two reconcilers.
        let p = nameless_pool(None);
        let err = p.owned_coordinates_required().unwrap_err();
        assert_eq!(err.to_string(), "EphemeralPool has no metadata.namespace");
    }

    #[test]
    fn owned_coordinates_required_reports_namespace_first_when_both_slots_absent_on_ephemeral_allocation(
    ) {
        let a = nameless_alloc(None);
        let err = a.owned_coordinates_required().unwrap_err();
        assert_eq!(
            err.to_string(),
            "EphemeralAllocation has no metadata.namespace"
        );
    }

    // ── Byte-identical parity with the pre-lift 5-line chain ──────

    #[test]
    fn owned_coordinates_required_matches_pre_lift_pool_reconciler_chain_shape() {
        // Byte-identical parity pin: the primitive produces the SAME
        // `Result<(String, String), anyhow::Error>` shape a pre-lift
        // `.metadata.<slot>.clone().ok_or_else(|| anyhow!("<Kind> has
        // no metadata.<slot>"))?` chain produced at
        // `tatara-pool-reconciler::controller_pool::reconcile_inner`
        // pre-lift, on both the happy and the missing-slot corners.
        // A regression that changed the error prefix, reordered the
        // two gates, or returned a non-`(String, String)` tuple
        // surfaces HERE rather than at every consumer downstream.
        let cases = [
            (Some("prod"), Some("api")),
            (Some("prod"), None),
            (None, Some("orphan")),
            (None, None),
        ];
        for (ns_slot, name_slot) in cases {
            let mut p = pool_fixture("placeholder", ns_slot);
            if let Some(nm) = name_slot {
                p.metadata.name = Some(nm.into());
            } else {
                p.metadata.name = None;
            }

            // Pre-lift 5-line paired chain (with the reconciler's
            // hand-authored short-form `"Pool"` prefix updated to the
            // canonical kube kind `"EphemeralPool"`, matching the
            // primitive's `Self::kind`-driven spelling — the drift
            // is intentional per the trait's docs).
            let pre_lift: anyhow::Result<(String, String)> = (|| {
                let ns =
                    p.metadata.namespace.clone().ok_or_else(|| {
                        anyhow::anyhow!("EphemeralPool has no metadata.namespace")
                    })?;
                let name = p
                    .metadata
                    .name
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("EphemeralPool has no metadata.name"))?;
                Ok((ns, name))
            })();

            let via_primitive = p.owned_coordinates_required();

            // Compare on both the Ok tuple + the error string
            // spelling — anyhow::Error does not derive PartialEq so
            // pattern-match on the Result axis rather than a direct
            // `assert_eq!` on the whole Result.
            match (via_primitive, pre_lift) {
                (Ok(a), Ok(b)) => assert_eq!(a, b),
                (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
                (a, b) => panic!(
                    "primitive vs pre-lift chain disagree on Ok/Err axis for \
                     (ns={ns_slot:?}, name={name_slot:?}): primitive={a:?}, pre_lift={b:?}"
                ),
            }
        }
    }

    #[test]
    fn owned_coordinates_required_matches_pre_lift_allocation_reconciler_chain_shape() {
        // Peer to the pool-side pin above — pin the same byte-
        // identity contract on the allocation reconciler's chain,
        // where the pre-lift error spelling used the short-form
        // `"Allocation"` prefix that the primitive now emits as the
        // canonical kube-kind `"EphemeralAllocation"`.
        let cases = [
            (Some("ephemeral-pools"), Some("pr-42-demo")),
            (Some("ephemeral-pools"), None),
            (None, Some("orphan")),
            (None, None),
        ];
        for (ns_slot, name_slot) in cases {
            let mut a = alloc_fixture("placeholder", ns_slot);
            if let Some(nm) = name_slot {
                a.metadata.name = Some(nm.into());
            } else {
                a.metadata.name = None;
            }

            let pre_lift: anyhow::Result<(String, String)> = (|| {
                let ns = a.metadata.namespace.clone().ok_or_else(|| {
                    anyhow::anyhow!("EphemeralAllocation has no metadata.namespace")
                })?;
                let name =
                    a.metadata.name.clone().ok_or_else(|| {
                        anyhow::anyhow!("EphemeralAllocation has no metadata.name")
                    })?;
                Ok((ns, name))
            })();

            let via_primitive = a.owned_coordinates_required();

            match (via_primitive, pre_lift) {
                (Ok(a), Ok(b)) => assert_eq!(a, b),
                (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
                (a, b) => panic!(
                    "primitive vs pre-lift chain disagree on Ok/Err axis for \
                     (ns={ns_slot:?}, name={name_slot:?}): primitive={a:?}, pre_lift={b:?}"
                ),
            }
        }
    }

    // ── Cross-CRD symmetry: kube kind drives the error prefix ─────

    #[test]
    fn owned_coordinates_required_error_prefix_matches_kube_kind_on_each_crd() {
        // The error prefix is sourced positionally from `Self::kind`
        // so the two CRDs emit distinct kube-canonical spellings
        // without either callsite hard-coding a per-CRD literal.
        // Regressions that hard-coded a shared prefix (e.g. a
        // copy-paste that pasted the pool's error string into the
        // allocation callsite) surface HERE.
        use kube::Resource;
        let p = pool_fixture("p", None);
        let a = alloc_fixture("a", None);
        assert_eq!(
            p.owned_coordinates_required().unwrap_err().to_string(),
            format!("{} has no metadata.namespace", EphemeralPool::kind(&()))
        );
        assert_eq!(
            a.owned_coordinates_required().unwrap_err().to_string(),
            format!(
                "{} has no metadata.namespace",
                EphemeralAllocation::kind(&())
            )
        );
        // Belt-and-suspenders: the two kinds are distinct spellings,
        // so the error strings are distinct too.
        assert_ne!(
            p.owned_coordinates_required().unwrap_err().to_string(),
            a.owned_coordinates_required().unwrap_err().to_string(),
        );
    }
}

#[cfg(test)]
mod deletion_tombstoned_tests {
    //! Pin the [`DeletionTombstoned`] trait's `is_being_deleted` probe
    //! at fail-before-pass-after granularity across every corner of
    //! the (tombstone present, tombstone absent) input matrix, on
    //! ALL THREE tatara-process CRDs the trait's blanket impl covers
    //! today (`Process`, `EphemeralPool`, `EphemeralAllocation`), plus
    //! the cross-CRD coherence with the two pre-existing inherent
    //! forwarders. A regression that skewed the trait's default,
    //! promoted a distinct-payload tombstone to a false negative, or
    //! diverged the trait from either inherent forwarder surfaces
    //! HERE rather than as silent operator-facing skew between the
    //! four consumer sites the primitive owns (the top-level
    //! dispatcher's SIGTERM preempt, the SIGTERM cascade's child-
    //! fan-out DELETE-skip, the pool reconciler's Drain gate, and
    //! the allocation reconciler's release short-circuit) on three
    //! sibling CRDs.
    use super::DeletionTombstoned;
    use crate::allocation::{AllocationSpec, EphemeralAllocation, Requestor};
    use crate::classification::{Classification, ConvergencePointType, SubstrateType};
    use crate::crd::{Process, ProcessSpec};
    use crate::ephemeral::EphemeralSpec;
    use crate::intent::{AplicacaoIntent, Intent};
    use crate::lifetime::TeardownPolicy;
    use crate::pool::{EphemeralPool, PoolSelector, PoolSpec, ReturnPolicy};
    use crate::spec::IdentitySpec;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;

    fn empty_template() -> EphemeralSpec {
        EphemeralSpec {
            aplicacao: AplicacaoIntent {
                chart_ref: "oci://x".into(),
                version: "1".into(),
                profile: String::new(),
                values_overlay: serde_json::Value::Null,
                release_name: None,
                target_namespace: None,
                install_timeout: None,
            },
            ttl: "1h".into(),
            teardown: TeardownPolicy::Always,
            max_concurrent: 0,
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: None,
            exports: vec![],
            routing: None,
        }
    }

    fn empty_pool_spec() -> PoolSpec {
        PoolSpec {
            desired_size: 1,
            min_size: 0,
            max_size: 0,
            return_policy: ReturnPolicy::Replace,
            selector: PoolSelector::default(),
            template: empty_template(),
            free_ttl: "24h".into(),
            max_allocation_ttl: "4h".into(),
            desired: 0,
            replacement_policy: Default::default(),
            stable_name_claim: false,
        }
    }

    fn empty_alloc_spec() -> AllocationSpec {
        AllocationSpec {
            pool_ref: None,
            requestor: Requestor {
                kind: "github-pr".into(),
                repo: None,
                branch: None,
                pr_number: None,
                sha: None,
                pr_labels: vec![],
                actor: None,
            },
            ttl: None,
            note: None,
        }
    }

    fn empty_process_spec() -> ProcessSpec {
        // Mirrors the workspace-standard `empty_spec()` fixture in
        // `crd.rs::tests` — the minimal `ProcessSpec` used across
        // every substrate metadata-projection pin.
        ProcessSpec {
            identity: IdentitySpec::default(),
            classification: Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
                horizon: Default::default(),
                calm: Default::default(),
                data_classification: Default::default(),
            },
            intent: Intent::default(),
            boundary: Default::default(),
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Default::default(),
            routing: None,
            encapsulates: None,
            suspended: false,
        }
    }

    // ── Missing tombstone (default fixture) — trait returns false ─────

    #[test]
    fn is_being_deleted_on_process_missing_tombstone_returns_false_via_trait() {
        let p = Process::new("api", empty_process_spec());
        assert!(!DeletionTombstoned::is_being_deleted(&p));
    }

    #[test]
    fn is_being_deleted_on_ephemeral_pool_missing_tombstone_returns_false_via_trait() {
        let p = EphemeralPool::new("attest-pool", empty_pool_spec());
        assert!(!DeletionTombstoned::is_being_deleted(&p));
    }

    #[test]
    fn is_being_deleted_on_ephemeral_allocation_missing_tombstone_returns_false_via_trait() {
        // The load-bearing corner: EphemeralAllocation had NO inherent
        // is_being_deleted pre-lift — the trait's blanket impl is
        // what closes the substrate gap for the allocation reconciler's
        // hand-authored `.metadata.deletion_timestamp.is_some()` chain.
        let a = EphemeralAllocation::new("pr-42-demo", empty_alloc_spec());
        assert!(!DeletionTombstoned::is_being_deleted(&a));
    }

    // ── Present tombstone — trait returns true ────────────────────────

    #[test]
    fn is_being_deleted_on_process_present_tombstone_returns_true_via_trait() {
        let mut p = Process::new("api", empty_process_spec());
        p.metadata.deletion_timestamp = Some(Time(chrono::Utc::now()));
        assert!(DeletionTombstoned::is_being_deleted(&p));
    }

    #[test]
    fn is_being_deleted_on_ephemeral_pool_present_tombstone_returns_true_via_trait() {
        let mut p = EphemeralPool::new("attest-pool", empty_pool_spec());
        p.metadata.deletion_timestamp = Some(Time(chrono::Utc::now()));
        assert!(DeletionTombstoned::is_being_deleted(&p));
    }

    #[test]
    fn is_being_deleted_on_ephemeral_allocation_present_tombstone_returns_true_via_trait() {
        let mut a = EphemeralAllocation::new("pr-42-demo", empty_alloc_spec());
        a.metadata.deletion_timestamp = Some(Time(chrono::Utc::now()));
        assert!(DeletionTombstoned::is_being_deleted(&a));
    }

    // ── Byte-identical parity with the pre-lift `.is_some()` chain ────

    #[test]
    fn is_being_deleted_matches_pre_lift_deletion_timestamp_is_some_chain_on_ephemeral_allocation()
    {
        // Byte-identical parity pin: the trait's default produces the
        // SAME `bool` a pre-lift `.metadata.deletion_timestamp.is_some()`
        // chain produced at `tatara-pool-reconciler::allocation_decide::
        // AllocationConvergenceCtx::observe` pre-lift, across every
        // corner of the (absent, present-at-now, present-at-past)
        // input matrix. A regression that inserted a normalization
        // step the pre-lift chain does NOT apply — or vice versa —
        // surfaces here rather than as silent drift between the
        // substrate owner and the pre-lift consumer.
        let mut cases: Vec<Option<Time>> = vec![None];
        cases.push(Some(Time(chrono::Utc::now())));
        cases.push(Some(Time(
            chrono::Utc::now() - chrono::Duration::seconds(3600),
        )));

        for ts in cases {
            let mut a = EphemeralAllocation::new("pr-42-demo", empty_alloc_spec());
            a.metadata.deletion_timestamp = ts.clone();

            let pre_lift = a.metadata.deletion_timestamp.is_some();
            let via_trait = DeletionTombstoned::is_being_deleted(&a);

            assert_eq!(
                pre_lift, via_trait,
                "trait probe must be byte-identical to pre-lift .metadata.deletion_timestamp.is_some() on tombstone={ts:?}",
            );
        }
    }

    // ── Cross-CRD coherence with the two inherent forwarders ──────────

    #[test]
    fn trait_probe_coheres_with_process_inherent_is_being_deleted_on_both_corners() {
        // Cross-primitive coherence pin: the trait's default and the
        // pre-existing `Process::is_being_deleted` inherent forwarder
        // return the SAME `bool` on the SAME `Process` value — a
        // future consolidation of the inherent onto the trait's default
        // (or vice versa) cannot land any drift between the two
        // surfaces because this pin binds them at every corner of the
        // (missing, present) input matrix.
        for ts in [None, Some(Time(chrono::Utc::now()))] {
            let mut p = Process::new("api", empty_process_spec());
            p.metadata.deletion_timestamp = ts.clone();
            assert_eq!(
                p.is_being_deleted(),
                DeletionTombstoned::is_being_deleted(&p),
                "Process trait probe must match inherent on tombstone={ts:?}",
            );
        }
    }

    #[test]
    fn trait_probe_coheres_with_ephemeral_pool_inherent_is_being_deleted_on_both_corners() {
        // Peer coherence pin on the sister CRD.
        for ts in [None, Some(Time(chrono::Utc::now()))] {
            let mut p = EphemeralPool::new("attest-pool", empty_pool_spec());
            p.metadata.deletion_timestamp = ts.clone();
            assert_eq!(
                p.is_being_deleted(),
                DeletionTombstoned::is_being_deleted(&p),
                "EphemeralPool trait probe must match inherent on tombstone={ts:?}",
            );
        }
    }

    // ── Inherent-preferred method resolution on Process + EphemeralPool ──

    #[test]
    fn dot_call_on_process_resolves_to_inherent_when_trait_in_scope() {
        // Rust method resolution prefers an inherent over a trait's
        // blanket impl, so `process.is_being_deleted()` with the trait
        // in scope still routes through the inherent — and both
        // return the same `bool` (verified in
        // `trait_probe_coheres_with_process_inherent_is_being_deleted_on_both_corners`).
        // This pin guards against a future refactor that removes the
        // inherent but leaves consumers assuming inherent-preferred
        // resolution — the observable output is identical either way,
        // so the pin locks the invariant that BOTH paths agree.
        let mut p = Process::new("api", empty_process_spec());
        p.metadata.deletion_timestamp = Some(Time(chrono::Utc::now()));
        assert!(p.is_being_deleted());
    }

    #[test]
    fn dot_call_on_ephemeral_allocation_resolves_to_trait_blanket_impl() {
        // The load-bearing corner: `alloc.is_being_deleted()` with
        // the trait in scope routes to the trait's blanket impl
        // (there is no inherent on `EphemeralAllocation`) and
        // produces the expected `bool`. This is what the swept
        // allocation-reconciler callsite depends on post-lift.
        let mut a = EphemeralAllocation::new("pr-42-demo", empty_alloc_spec());
        assert!(!a.is_being_deleted());
        a.metadata.deletion_timestamp = Some(Time(chrono::Utc::now()));
        assert!(a.is_being_deleted());
    }
}

#[cfg(test)]
mod annotated_tests {
    //! Pin the [`Annotated`] trait's `annotation` lookup at fail-
    //! before-pass-after granularity across every corner of the
    //! (annotations map: absent / present-empty / present-with-key /
    //! present-without-key) × (value form: normal / empty-string)
    //! input matrix, on the three tatara-process CRDs the trait's
    //! blanket impl covers today (`Process`, `EphemeralPool`,
    //! `EphemeralAllocation`) PLUS a K8s built-in (`ConfigMap`) — the
    //! load-bearing fourth surface that `tatara-export-worker::main`
    //! consumes post-lift where no tatara-owned inherent forwarder
    //! exists. Also pin cross-primitive coherence with the pre-existing
    //! `Process::annotation` inherent so a future consolidation onto
    //! the trait's default cannot silently skew the three consumers
    //! already routed through the inherent
    //! (`signals::ingest`,
    //! `phase_machine::released_from_annotation`,
    //! `controller_pool::process_belongs_to_pool`).
    use super::Annotated;
    use crate::allocation::{AllocationSpec, EphemeralAllocation, Requestor};
    use crate::classification::{Classification, ConvergencePointType, SubstrateType};
    use crate::crd::{Process, ProcessSpec};
    use crate::ephemeral::EphemeralSpec;
    use crate::intent::{AplicacaoIntent, Intent};
    use crate::lifetime::TeardownPolicy;
    use crate::pool::{EphemeralPool, PoolSelector, PoolSpec, ReturnPolicy};
    use crate::spec::IdentitySpec;
    use k8s_openapi::api::core::v1::ConfigMap;
    use std::collections::BTreeMap;

    fn empty_template() -> EphemeralSpec {
        EphemeralSpec {
            aplicacao: AplicacaoIntent {
                chart_ref: "oci://x".into(),
                version: "1".into(),
                profile: String::new(),
                values_overlay: serde_json::Value::Null,
                release_name: None,
                target_namespace: None,
                install_timeout: None,
            },
            ttl: "1h".into(),
            teardown: TeardownPolicy::Always,
            max_concurrent: 0,
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: None,
            exports: vec![],
            routing: None,
        }
    }

    fn empty_pool_spec() -> PoolSpec {
        PoolSpec {
            desired_size: 1,
            min_size: 0,
            max_size: 0,
            return_policy: ReturnPolicy::Replace,
            selector: PoolSelector::default(),
            template: empty_template(),
            free_ttl: "24h".into(),
            max_allocation_ttl: "4h".into(),
            desired: 0,
            replacement_policy: Default::default(),
            stable_name_claim: false,
        }
    }

    fn empty_alloc_spec() -> AllocationSpec {
        AllocationSpec {
            pool_ref: None,
            requestor: Requestor {
                kind: "github-pr".into(),
                repo: None,
                branch: None,
                pr_number: None,
                sha: None,
                pr_labels: vec![],
                actor: None,
            },
            ttl: None,
            note: None,
        }
    }

    fn empty_process_spec() -> ProcessSpec {
        ProcessSpec {
            identity: IdentitySpec::default(),
            classification: Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
                horizon: Default::default(),
                calm: Default::default(),
                data_classification: Default::default(),
            },
            intent: Intent::default(),
            boundary: Default::default(),
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Default::default(),
            routing: None,
            encapsulates: None,
            suspended: false,
        }
    }

    fn one_annotation(key: &str, value: &str) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        m.insert(key.into(), value.into());
        m
    }

    // ── Missing annotations map — trait returns None on every key ─────

    #[test]
    fn annotation_on_process_missing_annotations_returns_none_via_trait() {
        let mut p = Process::new("api", empty_process_spec());
        p.metadata.annotations = None;
        assert_eq!(Annotated::annotation(&p, "tatara.pleme.io/signal"), None);
        assert_eq!(Annotated::annotation(&p, ""), None);
    }

    #[test]
    fn annotation_on_ephemeral_pool_missing_annotations_returns_none_via_trait() {
        let mut p = EphemeralPool::new("attest-pool", empty_pool_spec());
        p.metadata.annotations = None;
        assert_eq!(Annotated::annotation(&p, "tatara.pleme.io/pool"), None);
    }

    #[test]
    fn annotation_on_ephemeral_allocation_missing_annotations_returns_none_via_trait() {
        // The peer load-bearing corner: EphemeralAllocation has NO
        // inherent `annotation()` pre-lift — the trait's blanket impl
        // is what closes the substrate gap here, exactly as the
        // sibling `DeletionTombstoned` trait already did on the
        // tombstone axis for the SAME third CRD.
        let mut a = EphemeralAllocation::new("pr-42-demo", empty_alloc_spec());
        a.metadata.annotations = None;
        assert_eq!(
            Annotated::annotation(&a, "tatara.pleme.io/requestor-kind"),
            None,
        );
    }

    #[test]
    fn annotation_on_config_map_missing_annotations_returns_none_via_trait() {
        // The load-bearing corner the export-worker's post-lift call
        // depends on: `ConfigMap` is a K8s built-in with no tatara-
        // owned inherent forwarder, and the receipts-owner filter
        // needs to route through the trait's blanket impl at
        // `cm.annotation(KEY)`.
        let cm = ConfigMap::default();
        // `Default::default()` produces an object with an empty
        // ObjectMeta whose `annotations` slot is `None` — the exact
        // missing-annotations corner the trait must collapse to
        // `None` at every key lookup, matching what the pre-lift
        // `cm.metadata.annotations.as_ref().and_then(...)` chain
        // produced.
        assert_eq!(Annotated::annotation(&cm, "tatara.pleme.io/process"), None,);
    }

    // ── Missing key inside populated map — trait returns None ─────────

    #[test]
    fn annotation_on_process_missing_key_returns_none_via_trait() {
        let mut p = Process::new("api", empty_process_spec());
        p.metadata.annotations = Some(one_annotation("other/key", "irrelevant"));
        assert_eq!(Annotated::annotation(&p, "tatara.pleme.io/signal"), None);
        assert_eq!(Annotated::annotation(&p, ""), None);
    }

    #[test]
    fn annotation_on_config_map_missing_key_returns_none_via_trait() {
        let mut cm = ConfigMap::default();
        cm.metadata.annotations = Some(one_annotation("unrelated", "yes"));
        assert_eq!(Annotated::annotation(&cm, "tatara.pleme.io/process"), None,);
    }

    // ── Present key — trait returns borrowed slice ────────────────────

    #[test]
    fn annotation_on_process_present_key_returns_borrowed_slice_via_trait() {
        let mut p = Process::new("api", empty_process_spec());
        p.metadata.annotations = Some(one_annotation("tatara.pleme.io/signal", "SIGHUP"));
        assert_eq!(
            Annotated::annotation(&p, "tatara.pleme.io/signal"),
            Some("SIGHUP"),
        );
    }

    #[test]
    fn annotation_on_ephemeral_pool_present_key_returns_borrowed_slice_via_trait() {
        let mut p = EphemeralPool::new("attest-pool", empty_pool_spec());
        p.metadata.annotations = Some(one_annotation("tatara.pleme.io/pool", "demo-pool"));
        assert_eq!(
            Annotated::annotation(&p, "tatara.pleme.io/pool"),
            Some("demo-pool"),
        );
    }

    #[test]
    fn annotation_on_ephemeral_allocation_present_key_returns_borrowed_slice_via_trait() {
        let mut a = EphemeralAllocation::new("pr-42-demo", empty_alloc_spec());
        a.metadata.annotations = Some(one_annotation(
            "tatara.pleme.io/requestor-kind",
            "github-pr",
        ));
        assert_eq!(
            Annotated::annotation(&a, "tatara.pleme.io/requestor-kind"),
            Some("github-pr"),
        );
    }

    #[test]
    fn annotation_on_config_map_present_key_returns_borrowed_slice_via_trait() {
        // The exact receipts-owner filter shape from
        // `tatara-export-worker::main`: a ConfigMap carrying the
        // `tatara.pleme.io/process` annotation set to the qualified
        // process reference `<ns>/<name>`. Pin that the trait produces
        // the exact borrowed slice the equality comparison against the
        // caller's `want.as_str()` sentinel consumes.
        let mut cm = ConfigMap::default();
        cm.metadata.annotations = Some(one_annotation(
            "tatara.pleme.io/process",
            "demo-ns/demo-app",
        ));
        assert_eq!(
            Annotated::annotation(&cm, "tatara.pleme.io/process"),
            Some("demo-ns/demo-app"),
        );
    }

    // ── Empty-value contract: `Some("")` — the pre-lift chain never
    //    swallowed empty values into `None`, so the trait must not
    //    either. Pinned separately from the missing-slot corners.

    #[test]
    fn annotation_present_key_with_empty_value_returns_some_empty_slice_via_trait() {
        let mut p = Process::new("api", empty_process_spec());
        p.metadata.annotations = Some(one_annotation("tatara.pleme.io/signal", ""));
        assert_eq!(
            Annotated::annotation(&p, "tatara.pleme.io/signal"),
            Some("")
        );
    }

    // ── Byte-identical parity with the pre-lift 3-line chain ──────────

    #[test]
    fn annotation_matches_pre_lift_annotations_lookup_chain_on_config_map() {
        // The four-corner input matrix the pre-lift
        // `cm.metadata.annotations.as_ref().and_then(|m| m.get(KEY))
        // .map(String::as_str)` chain traversed in
        // `tatara-export-worker::main` pre-lift. A regression that
        // inserted a normalization step the pre-lift chain does NOT
        // apply — or vice versa — surfaces here rather than as silent
        // drift between the substrate owner and the pre-lift consumer.
        const KEY: &str = "tatara.pleme.io/process";
        let cases: Vec<(Option<BTreeMap<String, String>>, Option<&str>)> = vec![
            (None, None),
            (Some(BTreeMap::new()), None),
            (Some(one_annotation("unrelated", "yes")), None),
            (
                Some(one_annotation(KEY, "demo-ns/demo-app")),
                Some("demo-ns/demo-app"),
            ),
            (Some(one_annotation(KEY, "")), Some("")),
        ];
        for (anns, expected) in cases {
            let mut cm = ConfigMap::default();
            cm.metadata.annotations = anns.clone();

            let pre_lift: Option<&str> = cm
                .metadata
                .annotations
                .as_ref()
                .and_then(|m| m.get(KEY))
                .map(String::as_str);
            let via_trait = Annotated::annotation(&cm, KEY);

            assert_eq!(
                pre_lift, expected,
                "pre-lift chain must return {expected:?} for annotations={anns:?}",
            );
            assert_eq!(
                via_trait, pre_lift,
                "trait probe must be byte-identical to pre-lift chain for annotations={anns:?}",
            );
        }
    }

    // ── Cross-primitive coherence with Process's inherent forwarder ───

    #[test]
    fn trait_probe_coheres_with_process_inherent_annotation_on_every_corner() {
        // Cross-primitive coherence pin: the trait's default and the
        // pre-existing `Process::annotation` inherent forwarder return
        // the SAME `Option<&str>` on the SAME `Process` value — a
        // future consolidation of the inherent onto the trait's
        // default cannot land any drift because this pin binds them
        // at every corner of the (absent, present-missing-key,
        // present-with-key, present-with-empty-value) input matrix.
        const KEY: &str = "tatara.pleme.io/signal";
        let cases: Vec<Option<BTreeMap<String, String>>> = vec![
            None,
            Some(BTreeMap::new()),
            Some(one_annotation("other/key", "irrelevant")),
            Some(one_annotation(KEY, "SIGHUP")),
            Some(one_annotation(KEY, "")),
        ];
        for anns in cases {
            let mut p = Process::new("api", empty_process_spec());
            p.metadata.annotations = anns.clone();
            let via_inherent = p.annotation(KEY);
            let via_trait = Annotated::annotation(&p, KEY);
            assert_eq!(
                via_inherent, via_trait,
                "Process inherent + Annotated trait must agree on annotations={anns:?}",
            );
        }
    }

    // ── Inherent-preferred method resolution on Process ───────────────

    #[test]
    fn dot_call_on_process_resolves_to_inherent_when_trait_in_scope() {
        // Rust method resolution prefers an inherent over a trait's
        // blanket impl, so `process.annotation(key)` with the trait in
        // scope still routes through the inherent — and both return
        // the same `Option<&str>` (verified in
        // `trait_probe_coheres_with_process_inherent_annotation_on_every_corner`).
        // This pin guards against a future refactor that removes the
        // inherent but leaves consumers assuming inherent-preferred
        // resolution — the observable output is identical either way,
        // so the pin locks the invariant that BOTH paths agree.
        let mut p = Process::new("api", empty_process_spec());
        p.metadata.annotations = Some(one_annotation("tatara.pleme.io/signal", "SIGHUP"));
        assert_eq!(p.annotation("tatara.pleme.io/signal"), Some("SIGHUP"));
    }

    #[test]
    fn dot_call_on_ephemeral_allocation_resolves_to_trait_blanket_impl() {
        // The peer load-bearing corner: `alloc.annotation(key)` with
        // the trait in scope routes to the trait's blanket impl —
        // there is no inherent on `EphemeralAllocation` — and produces
        // the expected `Option<&str>`. The same discipline the sibling
        // `DeletionTombstoned` trait already established on the
        // tombstone axis for the SAME third CRD.
        let mut a = EphemeralAllocation::new("pr-42-demo", empty_alloc_spec());
        assert_eq!(a.annotation("tatara.pleme.io/requestor-kind"), None);
        a.metadata.annotations = Some(one_annotation(
            "tatara.pleme.io/requestor-kind",
            "github-pr",
        ));
        assert_eq!(
            a.annotation("tatara.pleme.io/requestor-kind"),
            Some("github-pr"),
        );
    }

    #[test]
    fn dot_call_on_config_map_resolves_to_trait_blanket_impl() {
        // The load-bearing corner the export-worker's post-lift call
        // exercises: `cm.annotation(KEY)` with the trait in scope
        // routes to the blanket impl (ConfigMap is a K8s built-in
        // with no tatara-owned inherent) and produces the same
        // `Option<&str>` the pre-lift 3-line chain did.
        let mut cm = ConfigMap::default();
        assert_eq!(cm.annotation("tatara.pleme.io/process"), None);
        cm.metadata.annotations = Some(one_annotation(
            "tatara.pleme.io/process",
            "demo-ns/demo-app",
        ));
        assert_eq!(
            cm.annotation("tatara.pleme.io/process"),
            Some("demo-ns/demo-app"),
        );
    }
}

#[cfg(test)]
mod annotations_pins {
    //! Pin the three newly-lifted allocator-bind annotation keys
    //! ([`crate::annotations::REQUESTOR`],
    //! [`crate::annotations::ALLOCATION`],
    //! [`crate::annotations::REQUESTOR_KIND`]) at their canonical
    //! wire-form byte-values, and pin the coherence between each
    //! constant and the pre-lift string literal the sibling writer +
    //! reader test-sites still spell verbatim.
    //!
    //! Pre-lift each of the three keys was a bare `"tatara.pleme.io/…"`
    //! string literal at both the writer (`tatara-pool-reconciler::
    //! controller_allocation::reconcile_inner`'s Bind arm) AND the
    //! reader-side test sites in `annotated_tests` above — six
    //! restatements of `REQUESTOR_KIND` alone past the ★★
    //! PRIME-DIRECTIVE ≥ 2 duplication threshold. Post-lift the writer
    //! keys on the substrate constant; these pins bind the constant's
    //! byte-shape so a future edit that drifted the constant (a
    //! typo'd suffix, an accidental `tatara.pleme.io/v2/…` migration
    //! landing at only the writer, an incoming rename that swapped
    //! two of the three keys) surfaces here rather than as silent
    //! operator-facing skew between the writer and the tatara-process
    //! reader tests that still spell the literal.
    //!
    //! Theory anchor: THEORY.md §II.1 invariant 5 (composition
    //! preserves proofs — the wire-form value each downstream reader
    //! depends on now has a compile-time pin at the substrate).
    use crate::annotations;

    #[test]
    fn requestor_matches_pre_lift_wire_string() {
        assert_eq!(annotations::REQUESTOR, "tatara.pleme.io/requestor");
    }

    #[test]
    fn allocation_matches_pre_lift_wire_string() {
        assert_eq!(annotations::ALLOCATION, "tatara.pleme.io/allocation");
    }

    #[test]
    fn requestor_kind_matches_pre_lift_wire_string() {
        assert_eq!(
            annotations::REQUESTOR_KIND,
            "tatara.pleme.io/requestor-kind",
        );
    }

    #[test]
    fn allocator_bind_axis_keys_are_distinct() {
        // A copy-paste that duplicated one key's value across two
        // slots (an oversight during the initial lift or a future
        // rename that merged two keys by mistake) collapses BOTH
        // downstream readers onto the same wire string and silently
        // loses one of the three axes. Pin the closed set is
        // partition-distinct.
        assert_ne!(annotations::REQUESTOR, annotations::ALLOCATION);
        assert_ne!(annotations::REQUESTOR, annotations::REQUESTOR_KIND);
        assert_ne!(annotations::ALLOCATION, annotations::REQUESTOR_KIND);
    }

    #[test]
    fn allocator_bind_axis_keys_share_tatara_namespace() {
        // Every substrate-owned annotation key inhabits the
        // `tatara.pleme.io/` reverse-DNS namespace; a future rename
        // that dropped the prefix (a bare `"requestor"` key, a
        // typo'd `pleme.io/requestor`) would collide with an
        // arbitrary third-party operator's annotations on the same
        // Process and silently corrupt cross-consumer reads.
        for key in [
            annotations::REQUESTOR,
            annotations::ALLOCATION,
            annotations::REQUESTOR_KIND,
        ] {
            assert!(
                key.starts_with("tatara.pleme.io/"),
                "annotation key {key:?} must inhabit tatara.pleme.io/ namespace",
            );
        }
    }

    // ── Pool-membership axis pins ────────────────────────────────────
    //
    // Pins the two newly-lifted pool-membership annotation keys
    // ([`crate::annotations::POOL`], [`crate::annotations::POOL_SLOT`])
    // at their canonical wire-form byte-values. Pre-lift each key was
    // a file-scope `const ANNOTATION_POOL / ANNOTATION_SLOT` in
    // `tatara-pool-reconciler::controller_pool` PLUS bare
    // `"tatara.pleme.io/pool"` string literals at four reader-side
    // test sites in this crate (in the sibling `annotated_tests` above
    // and in `crd.rs`'s
    // `annotation_composes_borrow_equality_tail_matching_pre_lift_pool`
    // + `annotation_returns_none_when_metadata_annotations_is_none`).
    // Post-lift the writer routes through the substrate constant; a
    // future edit that drifted the constant (a typo'd suffix, an
    // accidental `tatara.pleme.io/v2/pool` migration landing at only
    // the writer, an incoming rename that swapped POOL and POOL_SLOT)
    // surfaces here rather than as silent operator-facing skew
    // between the pool controller's writer and its own membership-
    // gate reader.

    #[test]
    fn pool_matches_pre_lift_wire_string() {
        assert_eq!(annotations::POOL, "tatara.pleme.io/pool");
    }

    #[test]
    fn pool_slot_matches_pre_lift_wire_string() {
        assert_eq!(annotations::POOL_SLOT, "tatara.pleme.io/pool-slot");
    }

    #[test]
    fn pool_membership_axis_keys_are_distinct() {
        // A copy-paste that duplicated one key's value across both
        // slots (an oversight during the initial lift, or a future
        // rename that merged the two keys by mistake) collapses
        // BOTH downstream readers onto the same wire string and
        // silently loses the slot-index axis — the pool controller
        // would still find its own members via POOL but every per-
        // slot dispatch consumer would read the pool name where the
        // slot index used to sit. Pin the closed set is partition-
        // distinct.
        assert_ne!(annotations::POOL, annotations::POOL_SLOT);
    }

    #[test]
    fn pool_membership_axis_keys_share_tatara_namespace() {
        // Same reverse-DNS namespace invariant the allocator-bind
        // axis-family enforces above — a rename that dropped the
        // prefix on either POOL or POOL_SLOT would collide with an
        // arbitrary third-party operator's annotations on the same
        // Process and silently corrupt every pool-membership read.
        for key in [annotations::POOL, annotations::POOL_SLOT] {
            assert!(
                key.starts_with("tatara.pleme.io/"),
                "annotation key {key:?} must inhabit tatara.pleme.io/ namespace",
            );
        }
    }

    #[test]
    fn pool_membership_axis_keys_partition_distinct_from_allocator_bind_axis() {
        // Cross-family distinctness pin — the pool-membership axis
        // (POOL, POOL_SLOT) and the allocator-bind axis (REQUESTOR,
        // ALLOCATION, REQUESTOR_KIND) travel on the SAME member
        // Process at the SAME time (the pool controller writes POOL
        // + POOL_SLOT at creation; the allocator later merges
        // REQUESTOR / ALLOCATION / REQUESTOR_KIND onto the same
        // Process at Bind). A copy-paste that collapsed any axis
        // pair (e.g. POOL and REQUESTOR onto the same wire string)
        // would let one write silently overwrite the other. Pin
        // that every substrate-owned annotation key is unique
        // across the two axis-families.
        let pool_axis = [annotations::POOL, annotations::POOL_SLOT];
        let bind_axis = [
            annotations::REQUESTOR,
            annotations::ALLOCATION,
            annotations::REQUESTOR_KIND,
        ];
        for p in pool_axis {
            for b in bind_axis {
                assert_ne!(
                    p, b,
                    "pool-membership key {p:?} collides with allocator-bind key {b:?}",
                );
            }
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
