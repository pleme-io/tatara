//! `EphemeralPool` CRD — a population of warm, pre-attested ephemeral
//! Processes that get *allocated* to requestors (e.g., a GitHub PR
//! flow) on demand and *returned* (per a typed policy) when the
//! requestor releases them.
//!
//! Compounding move: the pool is a population manager **over the
//! existing Process algebra**, not a parallel runtime. A pool member
//! is just a `Process` with `Lifetime::Permanent` while in the free
//! list; allocation is "the operator (the pool reconciler) flips
//! that Process's lifetime slot to Ephemeral with the requestor's
//! TTL." Zero new compute primitive.
//!
//! Topology:
//!
//! ```text
//! EphemeralPool       (this CRD)
//!   ├── PoolSpec      (desired_size, template (EphemeralSpec), return_policy, selector)
//!   ├── PoolStatus    (phase, free / allocated / spawning / returning counts, members)
//!   └── owns N Processes via ownerReferences (one per pool slot)
//!
//! EphemeralAllocation (see allocation.rs)
//!   ├── AllocationSpec (pool_ref, requestor, requested_at, lifetime override)
//!   └── AllocationStatus (phase, assigned_process_ref, allocated_at, expires_at)
//! ```

use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ephemeral::EphemeralSpec;

/// `EphemeralPool` CRD spec — typed pool of warm Processes.
///
/// ```yaml
/// apiVersion: tatara.pleme.io/v1alpha1
/// kind: EphemeralPool
/// metadata:
///   name: attest-pool
///   namespace: ephemeral-pools
/// spec:
///   desiredSize: 3
///   minSize: 1
///   maxSize: 5
///   returnPolicy: Reset
///   selector:
///     repos: ["pleme-io/demo-*"]
///     branches: ["main", "release-*"]
///     prLabels: ["needs-ephemeral"]
///   template:
///     aplicacao:
///       chartRef: "oci://ghcr.io/pleme-io/charts/lareira-demo-app"
///       version: "0.5.5"
///       profile: "all-in-one"
///       …
///     ttl: "2h"
///     teardown: OnAttested
///     postconditions: [ … ]
/// ```
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "tatara.pleme.io",
    version = "v1alpha1",
    kind = "EphemeralPool",
    plural = "ephemeralpools",
    shortname = "epool",
    namespaced,
    status = "PoolStatus",
    printcolumn = r#"{"name":"Desired","type":"integer","jsonPath":".spec.desiredSize"}"#,
    printcolumn = r#"{"name":"Ready","type":"integer","jsonPath":".status.readyCount"}"#,
    printcolumn = r#"{"name":"Allocated","type":"integer","jsonPath":".status.allocatedCount"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct PoolSpec {
    /// Target number of warm Processes the pool maintains in `Free`
    /// state (sum of Free + Spawning targets `desired_size`).
    pub desired_size: u32,

    /// Hard floor on the free count. The reconciler refuses to scale
    /// below this even on cost-pressure signals. Default = 0.
    #[serde(default)]
    pub min_size: u32,

    /// Hard ceiling on total pool members (free + allocated + spawning).
    /// `0` = no cap. Default = 0.
    #[serde(default)]
    pub max_size: u32,

    /// What to do when an allocation releases.
    #[serde(default)]
    pub return_policy: ReturnPolicy,

    /// Routing selector — which allocation requests this pool serves.
    /// The reconciler matches incoming `EphemeralAllocation` CRs
    /// against this selector (most-specific wins across pools sharing
    /// a namespace).
    #[serde(default)]
    pub selector: PoolSelector,

    /// Template for each pool member — a typed `EphemeralSpec` that
    /// the reconciler lowers to `ProcessSpec` and instantiates.
    /// While in the free list each member's lifetime is overridden
    /// to `Permanent`; allocation flips it back to `Ephemeral` with
    /// the requestor's TTL.
    pub template: EphemeralSpec,

    /// How long a pool member may sit in `Free` before the reconciler
    /// recycles it (humantime). Defends against drift / stale state.
    /// Default `"24h"`.
    #[serde(default = "default_free_ttl")]
    pub free_ttl: String,

    /// Max time the reconciler allows a single allocation to hold a
    /// member before forcibly returning it (humantime). Hard cap
    /// independent of the allocation's own TTL. Default `"4h"`.
    #[serde(default = "default_max_allocation_ttl")]
    pub max_allocation_ttl: String,

    /// **R5 desired-count loop** — when set non-zero, the pool
    /// reconciler maintains exactly this many *healthy* (Running or
    /// Attested) Processes regardless of allocation pressure. Drives
    /// the "always seeking stability" property: failed members are
    /// replaced per `replacement_policy`. `0` keeps the legacy
    /// allocation-driven sizing (desired = floor of free + allocated).
    ///
    /// Operator usage: `desired: 5` means "always have 5 of these
    /// running"; failures auto-replace.
    #[serde(default)]
    pub desired: u32,

    /// **R5** — what the pool reconciler does when a member reaches
    /// `Failed` phase.
    #[serde(default)]
    pub replacement_policy: ReplacementPolicy,

    /// **R5** — when true, exactly one healthy member of the pool
    /// holds the unprefixed-form DNS hostnames declared in
    /// `template.routing` at any moment. The claim arbiter (see
    /// `tatara-reconciler::claim`) transfers atomically when the
    /// holder fails.
    #[serde(default)]
    pub stable_name_claim: bool,
}

impl PoolSpec {
    /// Humantime-parsed [`std::time::Duration`] projection of the
    /// [`Self::free_ttl`] slot — the ONE-line collapse of the paired
    /// `humantime::parse_duration(&<pool>.spec.free_ttl).ok()`
    /// incantation the pool reconciler's stale-free bucket loop
    /// hand-authored pre-lift, sibling to
    /// [`crate::lifetime::EphemeralLifetime::ttl_duration`] on the
    /// SAME `(humantime string field × Option<Duration>) → Option<
    /// Duration>` substrate axis.
    ///
    /// Pre-lift the `humantime::parse_duration(&<field>).ok()` shape
    /// was owned at ONE substrate primitive on
    /// [`crate::lifetime::EphemeralLifetime`] (the `spec.lifetime
    /// .ephemeral.ttl` axis, feeding
    /// [`crate::lifetime_clock::evaluate`]'s TTL-expiry gate + the
    /// `requeue_with_ttl` sleep-budget picker) AND hand-authored at
    /// ONE peer consumer site — `tatara-pool-reconciler::pool_decide
    /// ::decide_pool`, which parses `pool.spec.free_ttl` with the
    /// byte-identical shape (`humantime::parse_duration(&spec
    /// .free_ttl).unwrap_or_default()`) and gates the stale-free
    /// bucket loop on the result. That's ONE substrate owner + ONE
    /// hand-authored chain on a peer humantime field of a peer spec
    /// type past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger —
    /// two surfaces spelling the SAME projection with the SAME drift
    /// risk (a per-fleet minimum TTL floor before the humantime cast,
    /// a canonical unit-normalization pass, a warn-log on
    /// unparseable strings would have had to land at every surface
    /// plus stay coherent between them).
    ///
    /// Post-lift both peer humantime fields
    /// ([`crate::lifetime::EphemeralLifetime::ttl`] +
    /// [`Self::free_ttl`]) publish the SAME shape at TWO peer
    /// inherent methods on peer spec types — the tatara-pool-
    /// reconciler's stale-free bucket loop reads `pool.spec.free_ttl
    /// _duration().unwrap_or_default()` and the produced [`std::time
    /// ::Duration`] feeds the same `!free_ttl.is_zero()` guard +
    /// `elapsed > free_ttl` comparator unchanged. A future
    /// normalization (per-fleet minimum floor, canonical unit
    /// normalization, warn-log on unparseable strings) lands at TWO
    /// substrate methods here + on
    /// [`crate::lifetime::EphemeralLifetime::ttl_duration`], reachable
    /// via ONE workspace-wide sweep across the peer axis rather than
    /// as a per-callsite hand-edit at every downstream humantime-ttl
    /// consumer.
    ///
    /// Return-form axis: `Option<std::time::Duration>` matches the
    /// peer primitive on
    /// [`crate::lifetime::EphemeralLifetime::ttl_duration`] and the
    /// downstream comparator's type. The peer projection
    /// [`crate::time::elapsed_since`] returns the SAME `Option<std
    /// ::time::Duration>` shape, so the stale-free gate's `elapsed >
    /// free_ttl` comparator lands with both operands on the same
    /// axis without a per-consumer conversion step.
    ///
    /// The `None` arm is the "operator's `free_ttl` string doesn't
    /// parse" corner — a typo (`"1our"`), an unsupported unit, a
    /// non-humantime literal that reached the field. The pool
    /// reconciler's stale-free bucket loop collapses the corner via
    /// `.unwrap_or_default()`, yielding the `Duration::ZERO` value
    /// that already gates its follow-on `!free_ttl.is_zero()` check
    /// — post-lift semantics is byte-identical to the pre-lift
    /// hand-authored `humantime::parse_duration(&spec.free_ttl)
    /// .unwrap_or_default()` shape.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `humantime::parse_duration(&<field>).ok()` shape recurred
    /// at ONE substrate owner + ONE hand-authored peer site past the
    /// ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted onto
    /// TWO peer inherent methods on peer spec types here + on
    /// [`crate::lifetime::EphemeralLifetime::ttl_duration`]).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins below bind the parse-failure corner, the empty-ttl
    /// corner, the humantime edge shapes, the return-form parity with
    /// [`crate::lifetime::EphemeralLifetime::ttl_duration`], and the
    /// byte-identical parity with the pre-lift `.ok()` chain on the
    /// SAME `spec.free_ttl` value, so a regression that drifts any
    /// surface fails at `tests::pool_spec_free_ttl_duration_*` here
    /// rather than as silent operator-facing skew between the pool
    /// stale-free bucket loop and the ephemeral TTL-expiry gate on
    /// the two peer humantime-string fields).
    #[must_use]
    pub fn free_ttl_duration(&self) -> Option<std::time::Duration> {
        humantime::parse_duration(&self.free_ttl).ok()
    }

    /// Compose a [`PoolSpec`] for the given member `template`, stamping
    /// every non-template slot at the `#[serde(default …)]` value the
    /// wire-schema publishes above — the ONE substrate composer that
    /// closes the 11-slot `PoolSpec { desired_size: 1, min_size: 0,
    /// max_size: 0, return_policy: ReturnPolicy::Replace, selector:
    /// PoolSelector::default(), template, free_ttl: "24h".into(),
    /// max_allocation_ttl: "4h".into(), desired: 0, replacement_policy:
    /// Default::default(), stable_name_claim: false }` struct-literal
    /// every test-side + reconciler-side seed hand-authored pre-lift.
    ///
    /// Sibling to [`crate::crd::ProcessSpec::gate_compute_defaults`] on
    /// the (spec-type × full-baseline-composer) axis — that primitive
    /// owns the 11-slot [`crate::crd::ProcessSpec`] baseline composer;
    /// this one owns the peer 11-slot [`PoolSpec`] baseline composer.
    /// Both take a caller-supplied slot (there: the classification
    /// baseline via `Classification::gate_compute()`; here: the
    /// `template` [`EphemeralSpec`], which has no natural default) and
    /// fill every other slot at its wire-published default so a caller
    /// composes with struct-update syntax (`PoolSpec { desired_size: 1,
    /// ..PoolSpec::with_template(empty_template()) }`) rather than
    /// re-spelling the 10 defaulted slots at every seed. A future
    /// promotion of a defaulted slot to a non-default (a per-fleet
    /// minimum `min_size` floor, a shifted `default_free_ttl`,
    /// a widened `ReturnPolicy` default) lands at ONE substrate
    /// composer here and every downstream seed inherits the upgrade
    /// mechanically.
    ///
    /// Pre-lift the 11-slot struct-literal was hand-authored at EIGHT
    /// sites across TWO crates past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger:
    /// * `tatara-process::lib::tests::pool_fixture` — the
    ///   `qualified_process_ref` + trait-pin fixture seed;
    /// * `tatara-process::lib::tests::empty_pool_spec` (×2) — the two
    ///   sibling fixtures inside separate pin modules;
    /// * `tatara-process::pool::tests::pool_spec` — the `name_or_empty`
    ///   / `namespace_or_empty` pin fixture;
    /// * `tatara-pool-reconciler::router::tests::pool` — the router-
    ///   candidate-arbiter pin fixture (overrides `selector`);
    /// * `tatara-pool-reconciler::desired::tests::pool_with_desired` —
    ///   the desired-count-loop pin fixture (overrides `desired` +
    ///   `replacement_policy`);
    /// * `tatara-pool-reconciler::pool_decide::tests::pool` — the
    ///   pure-decision pin fixture (overrides sizes);
    /// * `tatara-pool-reconciler::allocation_decide::tests::pool` —
    ///   the allocation-router pin fixture (overrides `selector`).
    ///
    /// The three fields the wire-schema does NOT default (`desired_size`
    /// carries no `#[serde(default)]` above; `template` is the caller-
    /// supplied slot) are stamped at their operator-friendly seed
    /// values here — `desired_size = 0` matches every other reset
    /// slot's `0` / `false` / `Default` stamp, so a caller can compose
    /// `PoolSpec { desired_size: 1, ..PoolSpec::with_template(t) }` for
    /// the single-slot pool the majority of pre-lift seeds spelled, or
    /// `PoolSpec { desired_size: 0, desired: 5, ..with_template(t) }`
    /// for the desired-count-loop shape one seed spelled.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition — the
    /// 11-slot [`PoolSpec`] struct-literal recurred at EIGHT hand-
    /// authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// trigger and is lifted onto ONE workspace-wide owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs — a
    /// regression that drifted a wire-published default at only one
    /// consumer, or that broke the sibling-default correspondence with
    /// [`crate::crd::ProcessSpec::gate_compute_defaults`], surfaces at
    /// this primitive's tests rather than as silent operator-visible
    /// skew across the eight fixtures whose assertions key on the
    /// shape).
    #[must_use]
    pub fn with_template(template: EphemeralSpec) -> Self {
        Self {
            desired_size: 0,
            min_size: 0,
            max_size: 0,
            return_policy: ReturnPolicy::default(),
            selector: PoolSelector::default(),
            template,
            free_ttl: default_free_ttl(),
            max_allocation_ttl: default_max_allocation_ttl(),
            desired: 0,
            replacement_policy: ReplacementPolicy::default(),
            stable_name_claim: false,
        }
    }
}

impl EphemeralPool {
    /// Borrow-form metadata-projection primitive on the `metadata.name`
    /// axis of `EphemeralPool`: returns the K8s object name slice with
    /// the missing-name corner collapsed to the load-bearing empty-string
    /// sentinel — the ONE-liner collapse of the paired
    /// `self.metadata.name.as_deref().unwrap_or("")` incantation every
    /// pool-side consumer restated by hand pre-lift.
    ///
    /// Pre-lift the `.metadata.name.as_deref().unwrap_or("")` chain
    /// was hand-authored at TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold in `tatara-pool-reconciler`, both keyed
    /// by the pool's own name slot:
    /// * `router::pool_name` — the tie-break comparator inside
    ///   `best_match`; a deterministic lexicographic-min-name arbiter
    ///   across two pool candidates whose specificity scores tie.
    /// * `controller_allocation::reconcile_inner` — the `HashMap<
    ///   pool-name, Vec<PoolMember>>` lookup closure fed into
    ///   `decide_allocation_reconcile`; keys the "which pool members
    ///   back this allocation candidate?" projection at every
    ///   allocation-reconcile pass.
    ///
    /// Both sites walked the SAME `.as_deref().unwrap_or("")` chain
    /// and both wanted the `&str` form the primitive returns — as a
    /// borrow suitable for lexicographic `str::cmp` in the tie-break
    /// AND for the `HashMap<String, _>::get(&str)` lookup. Post-lift
    /// each caller reaches for `pool.name_or_empty()` and the produced
    /// slice feeds the same downstream comparator / lookup unchanged.
    ///
    /// The empty-string fallback is the SAME sentinel the sibling
    /// borrow-form primitive [`crate::crd::Process::uid_or_empty`]
    /// returns AND the SAME sentinel the owned-form sibling
    /// [`crate::crd::Process::owned_name_or_empty`] returns on the
    /// `metadata.name` axis of the sister CRD — the three primitives
    /// partition the (borrow-form × owned-form) × (uid × name) corner
    /// of the metadata-slot family on identical fallback semantics
    /// (empty string means "the slot is unset"), so a consumer that
    /// switches between the CRD surfaces based on downstream keying
    /// requirements never sees a different missing-slot spelling as
    /// a side effect.
    ///
    /// Return-form axis: `&str` mirrors the borrow-first discipline
    /// of the peer metadata primitives on `Process`
    /// ([`crate::crd::Process::namespace_or_default`],
    /// [`crate::crd::Process::name_or_placeholder`],
    /// [`crate::crd::Process::uid_or_empty`]). The one missing-slot
    /// corner the chain swallowed pre-lift (missing `metadata.name`)
    /// collapses to the empty-string sentinel so `str::is_empty` /
    /// `HashMap::get` on an unnamed pool behaves identically to what
    /// the pre-lift `.as_deref().unwrap_or("")` chain produced.
    ///
    /// A future normalization step (a name-canonicalization pass, a
    /// case-fold key builder, a per-cluster prefix stripper for
    /// cross-cluster pool-name aliasing) lands at ONE substrate
    /// method here and both downstream consumers pick up the upgrade
    /// mechanically — no per-callsite hand-edit at `pool_name` /
    /// `reconcile_inner`.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.metadata.name.as_deref().unwrap_or("")` chain recurred
    /// at two hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the missing-name corner + the empty-string
    /// sentinel byte-shape + the borrow-form `&str` lifetime + the
    /// byte-identical parity with the pre-lift chain + the fallback-
    /// value coherence with `Process::uid_or_empty` /
    /// `Process::owned_name_or_empty` on the metadata-slot × empty-
    /// sentinel axis, so a regression that drifted any surface at
    /// `tests::name_or_empty_*` here rather than as silent operator-
    /// facing skew between the router tie-break and the allocation
    /// member-lookup on the SAME pool candidate).
    pub fn name_or_empty(&self) -> &str {
        self.metadata.name.as_deref().unwrap_or("")
    }

    /// Owned-form metadata-projection primitive on the `metadata.name`
    /// axis of `EphemeralPool`: returns an owned `String` copy of the K8s
    /// object name with the missing-name corner collapsed to the load-
    /// bearing empty-string sentinel — the ONE-liner collapse of the
    /// paired `self.metadata.name.clone().unwrap_or_default()` incantation
    /// every pool-side consumer restated by hand pre-lift.
    ///
    /// Pre-lift the `.metadata.name.clone().unwrap_or_default()` chain
    /// was hand-authored at TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold in `tatara-pool-reconciler`, both keyed by
    /// the pool's own name slot in an `owned String` context:
    /// * `controller_allocation::reconcile_inner` — the
    ///   `HashMap<String, Vec<PoolMember>>` key seed inside a
    ///   `pools.iter().map(|p| ...).collect()` fanout; the map key is
    ///   the owned `String` form because the produced `HashMap<String, _>`
    ///   outlives the pool-list borrow that generated it and the
    ///   downstream `pool_members.get(pool.name_or_empty())` closure
    ///   consumes it as `&str`.
    /// * `allocation_decide::AllocationConvergenceCtx::observe` — the
    ///   `AllocationRef::name` slot seed stamped on the matched-pool
    ///   handle; the struct literal is `AllocationRef { name: String,
    ///   namespace: String }` and the produced value is threaded through
    ///   the `Decision::decide` transition rule downstream.
    ///
    /// Both sites walked the SAME `.clone().unwrap_or_default()` chain
    /// and both wanted the `String` form the primitive returns — as the
    /// owned key of a `HashMap<String, _>` and as the `String` slot of
    /// an `AllocationRef` struct literal. Post-lift each callsite reads
    /// `pool.owned_name_or_empty()` and the produced value feeds the
    /// same downstream key / struct-literal slot unchanged.
    ///
    /// The empty-string fallback is the SAME sentinel the sibling
    /// borrow-form primitive [`Self::name_or_empty`] returns AND the
    /// SAME sentinel the sibling owned-form primitive
    /// [`crate::crd::Process::owned_name_or_empty`] returns on the
    /// `metadata.name` axis of the sister CRD — the three primitives
    /// partition the (borrow-form × owned-form) corner of the metadata-
    /// name family across BOTH tatara-process CRDs on identical missing-
    /// slot semantics (empty string means "the slot is unset"), so a
    /// consumer that switches between the CRD surfaces based on
    /// downstream ownership requirements never sees a different
    /// missing-slot spelling as a side effect.
    ///
    /// Peer to [`Self::name_or_empty`] on the (return-form × ownership)
    /// axis pair — closes the corner the pool-side family previously
    /// left open:
    ///
    /// * borrow + empty sentinel → [`Self::name_or_empty`] (router tie-
    ///   break comparator, `HashMap<String, _>::get(&str)` lookup —
    ///   consumers whose downstream keys by `&str` and allocates
    ///   nothing);
    /// * owned + empty sentinel → **this method** (HashMap-key seed in
    ///   an outliving-borrow context, `AllocationRef::name` struct-
    ///   literal slot — consumers whose downstream requires the owned
    ///   `String` form because the produced value outlives the source-
    ///   pool borrow).
    ///
    /// A future normalization step (a name-canonicalization pass, a
    /// case-fold key builder, a per-cluster prefix stripper for cross-
    /// cluster pool-name aliasing) lands at ONE substrate method here
    /// and both downstream consumers pick up the upgrade mechanically —
    /// no per-callsite hand-edit at `reconcile_inner` /
    /// `AllocationConvergenceCtx::observe`.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.metadata.name.clone().unwrap_or_default()` chain recurred
    /// at two hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the missing-name corner + the empty-string
    /// sentinel byte-shape + the owned-form `String` return type + the
    /// byte-identical parity with the pre-lift chain + the fallback-
    /// value coherence with [`Self::name_or_empty`] +
    /// [`crate::crd::Process::owned_name_or_empty`] on the metadata-
    /// slot × empty-sentinel axis, so a regression that drifted any
    /// surface at `tests::owned_name_or_empty_*` here rather than as
    /// silent operator-facing skew between the pool-members lookup key
    /// and the AllocationRef seed on the SAME pool candidate).
    pub fn owned_name_or_empty(&self) -> String {
        self.metadata.name.clone().unwrap_or_default()
    }

    /// Copy-form metadata-projection primitive on the deletion-tombstone
    /// axis of `EphemeralPool`: returns `true` iff the K8s API server
    /// has stamped a `metadata.deletionTimestamp` on this pool (the
    /// moment the object entered the "being deleted" corner of its
    /// lifecycle, after which further mutating writes are refused and
    /// finalizers are drained before the object is actually removed) —
    /// the ONE-liner collapse of the paired
    /// `self.metadata.deletion_timestamp.is_some()` incantation every
    /// pool-side consumer restated by hand pre-lift.
    ///
    /// Pre-lift the `.metadata.deletion_timestamp.is_some()` chain was
    /// hand-authored at TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold in `tatara-pool-reconciler`, both
    /// projecting the SAME tombstone-presence predicate on an
    /// `EphemeralPool` value:
    /// * `pool_decide::decide_pool_reconcile` — the pure decision
    ///   function's deletion-preempt gate that forces
    ///   [`PoolDecision::Drain`] as soon as the API server stamps
    ///   the tombstone, before the (desired vs actual) supply-arithmetic
    ///   branches get a chance to run. Wired at the very top of the
    ///   decision so a draining pool never spawns / reaps / expires
    ///   through the normal replenishment arithmetic while the
    ///   deletion is in flight.
    /// * `controller_pool::pool_phase_from_members` — the observed-
    ///   phase composer's tombstone-first arm that returns
    ///   [`PoolPhase::Draining`] regardless of the supply / demand
    ///   arithmetic that would otherwise pick `Ready` / `Scaling` /
    ///   `Degraded`. Keeps the reported phase honest during the
    ///   finalizer drain so operators reading `kubectl get
    ///   ephemeralpools` see the tombstone-present state as
    ///   `Draining`, not as a stale `Ready`.
    ///
    /// Both sites walked the SAME `.metadata.deletion_timestamp
    /// .is_some()` chain and both wanted the `bool` form the primitive
    /// returns — the `decide_pool_reconcile` site to gate the
    /// `→ Drain` short-circuit and the `pool_phase_from_members` site
    /// to gate the `→ Draining` short-circuit. Post-lift each callsite
    /// reads `pool.is_being_deleted()` and the produced `bool` feeds
    /// the same downstream short-circuit unchanged.
    ///
    /// Sibling to [`crate::crd::Process::is_being_deleted`] on the
    /// deletion-tombstone axis of the sister CRD — the two primitives
    /// now partition the tombstone-presence probe across BOTH
    /// tatara-process CRDs on identical missing-slot semantics
    /// (present timestamp means "the API server has begun deletion"),
    /// so an operator or reconciler that switches between the CRD
    /// surfaces never sees a different tombstone-detection spelling
    /// as a side effect.
    ///
    /// Return-form axis: `bool` matches the copy-form discipline of
    /// the sibling [`crate::crd::Process::is_being_deleted`] and of
    /// the pool-side [`crate::phase::ProcessPhase::is_alive`] +
    /// [`Self::name_or_empty`]-family primitives — the underlying
    /// slot is a wire-format `Option<Time>` that carries only
    /// presence information at this axis (the RFC-3339 timestamp
    /// payload itself is not what the two consumers read; both only
    /// probe presence to detect the tombstone-stamped state).
    /// Returning the raw `Option<&Time>` would push the `.is_some()`
    /// probe back to every callsite, restating the pre-lift chain
    /// one link shorter without collapsing the primitive.
    ///
    /// Peer to [`Self::name_or_empty`] and [`Self::owned_name_or_empty`]
    /// on the metadata-projection axis for `EphemeralPool`; this method
    /// opens the presence-probe corner for the tombstone slot. Future
    /// metadata-presence projections on the pool CRD (an
    /// `is_being_finalized` projection on
    /// `metadata.finalizers.is_empty()`'s negation, a `has_owner`
    /// projection on `metadata.owner_references.is_empty()`'s
    /// negation) land as peer methods on this same axis.
    ///
    /// A future normalization step (a per-tombstone staleness gate
    /// that returns `false` for a tombstone older than the reconciler's
    /// grace-period budget, a canonicalization pass that treats a
    /// tombstone from a paused controller as absent, a cross-cluster
    /// tombstone-observation clock skew guard) lands at ONE substrate
    /// method here and both downstream consumers pick up the upgrade
    /// mechanically — no per-callsite hand-edit at
    /// `decide_pool_reconcile` / `pool_phase_from_members`.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.metadata.deletion_timestamp.is_some()` chain recurred at
    /// two hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the missing-tombstone corner + the present-
    /// tombstone corner + the copy-form `bool` return + the byte-
    /// identical parity with the pre-lift `.is_some()` chain + the
    /// cross-CRD coherence with `crate::crd::Process::is_being_deleted`
    /// on the tombstone axis, so a regression that drifted any surface
    /// at `tests::is_being_deleted_*` rather than as silent operator-
    /// facing skew between the pool-reconciler's `→ Drain` decision
    /// and the observed-phase composer's `→ Draining` report on the
    /// SAME `EphemeralPool` within one reconcile pass).
    pub fn is_being_deleted(&self) -> bool {
        self.metadata.deletion_timestamp.is_some()
    }

    /// Owned-form metadata-projection primitive on the `metadata.namespace`
    /// axis of `EphemeralPool`: returns an owned `String` copy of the K8s
    /// namespace with the missing-namespace corner collapsed to the load-
    /// bearing empty-string sentinel — the ONE-liner collapse of the
    /// paired `self.metadata.namespace.clone().unwrap_or_default()`
    /// incantation every pool-side consumer restated by hand pre-lift.
    ///
    /// Pre-lift the `.metadata.namespace.clone().unwrap_or_default()`
    /// chain was hand-authored at TWO sites past the ★★ PRIME-DIRECTIVE
    /// ≥ 2 duplication threshold, both stamping the `AllocationRef
    /// { namespace: String, .. }` slot inside an owned-`String` context:
    /// * `tatara-pool-reconciler::allocation_decide::AllocationConvergenceCtx
    ///   ::observe` — the matched-pool seed's `AllocationRef.namespace`
    ///   slot, right beside the peer [`Self::owned_name_or_empty`] call
    ///   that owns the paired name half. This is the exact site the
    ///   pre-existing peer-primitive doc-comment forecast (`"a future
    ///   run may lift owned_namespace_or_empty as the sibling axis
    ///   peer"`).
    /// * `crate::pool::tests::allocation_ref_new_composes_with_owned_name_or_empty_pool_projection`
    ///   — the composition pin that seeded an `AllocationRef` from the
    ///   same paired-primitive-half construction the production consumer
    ///   in `allocation_decide::observe` performs. Post-lift the pin
    ///   composes two peer primitives (`owned_name_or_empty` +
    ///   `owned_namespace_or_empty`) rather than one primitive plus the
    ///   pre-lift chain, sharpening it from a mixed-form composition
    ///   check into a paired-primitive-family composition check.
    ///
    /// Both sites walked the SAME `.clone().unwrap_or_default()` chain
    /// and both wanted the `String` form the primitive returns — as the
    /// `String` slot of an `AllocationRef` struct literal built through
    /// [`crate::pool::AllocationRef::new`]. Post-lift each callsite reads
    /// `pool.owned_namespace_or_empty()` and the produced value feeds
    /// the same downstream `AllocationRef` slot unchanged.
    ///
    /// The empty-string fallback is the SAME sentinel the sibling owned-
    /// form primitive [`Self::owned_name_or_empty`] returns on the
    /// `metadata.name` axis of the same CRD — the two primitives now
    /// partition the (owned `String` × `metadata.<slot>`) corner of the
    /// pool CRD's metadata family across BOTH object-coordinate slots
    /// on identical missing-slot semantics (empty string means "the
    /// slot is unset"), so the [`crate::pool::AllocationRef::new`]
    /// composer sees a coherent owned-empty pair regardless of which
    /// slot is absent on the source pool. Coherent with the workspace-
    /// wide owned-empty sentinel that the peer primitives
    /// [`crate::crd::Process::uid_or_empty`],
    /// [`crate::crd::Process::owned_name_or_empty`],
    /// [`Self::name_or_empty`], and [`Self::owned_name_or_empty`]
    /// already share on the metadata-slot × empty-sentinel axis.
    ///
    /// Peer to [`Self::owned_name_or_empty`] on the
    /// (`metadata.name` × `metadata.namespace`) axis of the owned-form
    /// projection family — closes the corner the pool-side family
    /// previously left open:
    ///
    /// * owned + name + empty sentinel → [`Self::owned_name_or_empty`]
    ///   (`AllocationRef.name` seed, `HashMap<String, _>` key seed);
    /// * owned + namespace + empty sentinel → **this method**
    ///   (`AllocationRef.namespace` seed — the paired half the same
    ///   `AllocationRef::new(name, namespace)` constructor consumes);
    /// * copy + deletion + tombstone probe → [`Self::is_being_deleted`]
    ///   (the presence-probe corner of the same metadata axis, already
    ///   opened).
    ///
    /// A future normalization step (a namespace-canonicalization pass,
    /// a case-fold key builder, a per-cluster prefix stripper, or the
    /// canonical-namespace default lift that would substitute
    /// [`crate::crd::Process::DEFAULT_NAMESPACE`] on the missing-slot
    /// corner rather than the empty-string sentinel) lands at ONE
    /// substrate method here and both downstream consumers pick up the
    /// upgrade mechanically — no per-callsite hand-edit at
    /// `AllocationConvergenceCtx::observe` / the composition pin.
    ///
    /// The empty-string fallback (rather than
    /// [`crate::crd::Process::DEFAULT_NAMESPACE`]) is DELIBERATELY
    /// pinned: the sole downstream consumer
    /// (`AllocationConvergenceCtx::observe`'s matched-pool seed) feeds
    /// the produced value into `AllocationRef.namespace`, which is then
    /// matched byte-identically against `spec.pool_ref.namespace` at
    /// [`crate::pool::allocation_decide::resolve_pool`]-style comparators.
    /// A silent substitution of `"default"` at this primitive would
    /// alias every namespace-absent pool to the `"default"` bucket at
    /// the matcher, hiding the missing-slot corner from an operator
    /// who explicitly authored an allocation against a namespace-
    /// unset pool. The load-bearing empty-string sentinel keeps the
    /// pre-lift `.clone().unwrap_or_default()` shape verbatim so the
    /// downstream matcher's byte-comparison stays honest.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.metadata.namespace.clone().unwrap_or_default()` chain
    /// recurred at two hand-authored sites past the ★★ PRIME-DIRECTIVE
    /// ≥ 2 duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the missing-namespace corner + the empty-string
    /// sentinel byte-shape + the owned-form `String` return type + the
    /// byte-identical parity with the pre-lift chain + the fallback-
    /// value coherence with [`Self::owned_name_or_empty`] on the
    /// paired-slot axis, so a regression that drifted any surface at
    /// `tests::owned_namespace_or_empty_*` rather than as silent
    /// operator-facing skew between the paired name / namespace halves
    /// of the SAME `AllocationRef` seed).
    pub fn owned_namespace_or_empty(&self) -> String {
        self.metadata.namespace.clone().unwrap_or_default()
    }

    /// Compound owned-form metadata-projection primitive on the paired
    /// `(metadata.uid, metadata.name)` axis of `EphemeralPool`: returns
    /// a stable owned `String` seed for slot-slug derivation, PREFERRING
    /// the K8s-assigned uid, FALLING BACK to the pool's own name, then
    /// SINKING to the load-bearing empty-string sentinel when both slots
    /// are absent — the ONE-liner collapse of the paired
    /// `pool.metadata.uid.clone().unwrap_or_else(|| name.<into>())`
    /// incantation every pool-slot-name-composing consumer restated by
    /// hand pre-lift.
    ///
    /// Pre-lift the `.metadata.uid.clone().unwrap_or_else(|| name.<into>())`
    /// chain was hand-authored at TWO production sites past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication threshold in
    /// `tatara-pool-reconciler::controller_pool`, both feeding the SAME
    /// `member_process_name(&pool_name, &pool_uid_or_name_fallback, slot)`
    /// composer:
    /// * `reconcile_inner` — the desired-count `PoolDecision::Spawn`
    ///   arm's spawn-loop slot-slug seed (fallback bound as
    ///   `|| name.clone()` from the extracted-earlier owned `name`
    ///   half of `owned_coordinates_required()`).
    /// * `apply_convergence_actions` — the legacy allocation-driven
    ///   `ConvergenceAction::CreateMember` arm's slot-slug seed
    ///   (fallback bound as `|| name.to_string()` from the borrowed
    ///   `name: &str` parameter that the same
    ///   `owned_coordinates_required()`-extracted `String` was passed
    ///   through by reference).
    ///
    /// Both sites computed the SAME "prefer the k8s uid; fall back to
    /// the pool's own name" projection on the SAME `EphemeralPool`
    /// value, differing only in the surface syntax of the fallback
    /// (`.clone()` vs `.to_string()`) — a per-callsite typing artefact
    /// of the enclosing scope's `name` binding rather than a semantic
    /// distinction. Post-lift each callsite reads
    /// `pool.owned_uid_or_name_or_empty()` and the produced owned
    /// `String` feeds the same `member_process_name(&name, &_, slot)`
    /// composer verbatim; the caller no longer threads its own local
    /// `name` handle through as the fallback, since the primitive
    /// reaches through the same `self.metadata.name` slot the caller
    /// extracted from earlier — coherent by construction with the
    /// sibling primitive [`Self::owned_name_or_empty`] on the missing-
    /// name corner.
    ///
    /// The compound (uid-preferred, name-fallback, empty-sentinel)
    /// precedence is DELIBERATELY pinned: the K8s API server stamps
    /// `metadata.uid` on every persisted object at admission time, so
    /// the reachable state at both callsites (each already gated by
    /// `owned_coordinates_required()?`) has `uid = Some(_)`. The name
    /// fallback is a load-bearing safety net for the vanishingly rare
    /// pre-admission-uid corner + the unit-test path that constructs
    /// an `EphemeralPool` value in-memory without stamping a uid; the
    /// empty-string sink is the sentinel-coherent complement of the
    /// missing-both corner (both slots `None`) so a regression that
    /// dropped either fallback surfaces as a compiler-visible test
    /// failure rather than as an operator-facing skew between spawn
    /// slots derived from mixed-fallback seeds within one reconcile
    /// pass. Coherent with the workspace-wide owned-empty sentinel
    /// that the peer primitives [`Self::owned_name_or_empty`],
    /// [`Self::owned_namespace_or_empty`],
    /// [`crate::crd::Process::owned_name_or_empty`], and
    /// [`crate::crd::Process::uid_or_empty`] already share on the
    /// metadata-slot × empty-sentinel axis.
    ///
    /// A future normalization step (a per-cluster uid-prefix stripper,
    /// a case-fold key builder, canonicalization of a suspiciously-
    /// empty uid to the name fallback, a namespace-scoped hashing pass
    /// that mixes cluster identity into the seed) lands at ONE
    /// substrate method here and both downstream `spawn` /
    /// `apply_convergence_actions` consumers pick up the upgrade
    /// mechanically — no per-callsite hand-edit at `controller_pool`.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.metadata.uid.clone().unwrap_or_else(|| name.<into>())`
    /// chain recurred at two hand-authored sites past the ★★ PRIME-
    /// DIRECTIVE ≥ 2 duplication trigger, and is lifted to ONE owner
    /// here). THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — the pins bind the uid-present corner + the uid-absent
    /// name-fallback corner + the both-absent empty-sentinel corner +
    /// the owned-form `String` return type + the byte-identical parity
    /// with each pre-lift callsite's fallback surface, so a regression
    /// that drifted any surface at `tests::owned_uid_or_name_or_empty_*`
    /// rather than as silent operator-facing skew between the two
    /// slot-slug seeds within ONE reconcile pass).
    pub fn owned_uid_or_name_or_empty(&self) -> String {
        self.metadata
            .uid
            .clone()
            .unwrap_or_else(|| self.owned_name_or_empty())
    }

    /// Copy-form metadata-projection primitive on the `metadata.name`
    /// axis of `EphemeralPool` in its `presence-and-equal` corner:
    /// returns `true` iff the K8s object name slot is BOTH `Some(_)`
    /// AND byte-identical to the supplied candidate — the ONE-liner
    /// collapse of the paired
    /// `self.metadata.name.as_deref() == Some(candidate)` incantation
    /// every pool-side lookup consumer restated by hand pre-lift.
    ///
    /// Pre-lift the `.metadata.name.as_deref() == Some(<candidate>)`
    /// chain was hand-authored at TWO production sites past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication threshold in
    /// `tatara-pool-reconciler`, both keyed by the `EphemeralPool`'s
    /// own name slot inside a `candidate_pools.iter().find(|p| ...)`
    /// closure that resolves a pool from an `AllocationRef.name` half:
    /// * `allocation_decide::resolve_pool` — the explicit-`pool_ref`
    ///   half of the pool-resolution ladder, one of two conjuncts in
    ///   the `(name == X && namespace == Y)` byte-comparison against
    ///   `AllocationSpec::pool_ref`. Pairs with the sibling namespace
    ///   comparison (a future run may lift `has_namespace` as the
    ///   paired-axis peer once a second namespace-probe site opens).
    /// * `controller_allocation::reconcile_inner` — the TTL-inheritance
    ///   fallback path's pool-lookup by `AllocationDecision::Bind::pool
    ///   .name`, feeding the matched pool's `spec.template.ttl` into
    ///   the just-bound member Process's lifetime overlay.
    ///
    /// Both sites walked the SAME `.as_deref() == Some(<x>.as_str())`
    /// chain against a `&str` candidate held by an [`AllocationRef`]
    /// or a similar owned-name handle, and both wanted the `bool`
    /// form the primitive returns — the transition rule's discriminant
    /// on either the `find(|p| p.has_name(&pool_ref.name))` closure
    /// (which either matches ONE candidate pool or none) or the
    /// TTL-inheritance closure's short-circuit through
    /// `.map(...).unwrap_or_else(...)`. Post-lift each callsite reads
    /// `p.has_name(&candidate)` and the produced `bool` feeds the same
    /// downstream `find` / `map` closure unchanged.
    ///
    /// Distinct in semantics from the sibling primitive
    /// [`Self::name_or_empty`] on the SAME `metadata.name` axis: the
    /// `_or_empty` family folds the missing-slot corner to the load-
    /// bearing empty-string sentinel (so `None` and `Some("")` both
    /// project to `""`), whereas this primitive keeps `None` distinct
    /// from `Some("")` at the `==` operator — a `None` slot returns
    /// `false` even when the candidate is the empty string. That
    /// discipline is load-bearing at both consumer sites: pre-lift
    /// they compared `Option<&str>` against `Some(<candidate>)`, so a
    /// substitution through `Self::name_or_empty` would silently
    /// promote a namespace-absent pool with a `""` candidate into a
    /// spurious match at the `find` closure, aliasing every unnamed
    /// pool to the same lookup bucket at the resolver. Preserving the
    /// `None ⇒ false` corner keeps the resolver's byte-comparison
    /// honest.
    ///
    /// Peer to the sibling substrate primitives already opened on the
    /// pool-side (`metadata.name` × return-form) axis:
    /// * borrow-form + empty sentinel → [`Self::name_or_empty`] (`&str`
    ///   projection with a `""` fallback for missing / explicitly-empty
    ///   name slots; router tie-break comparator);
    /// * owned-form + empty sentinel → [`Self::owned_name_or_empty`]
    ///   (`String` projection with a `""` fallback; `AllocationRef.name`
    ///   seed);
    /// * **presence-and-equal probe → this method** (`bool` projection
    ///   with `None`-preserving semantics; pool-lookup closure
    ///   discriminant).
    ///
    /// A future normalization step (a name-canonicalization pass, a
    /// case-fold key builder, a per-cluster prefix stripper for cross-
    /// cluster pool-name aliasing, or a canonical-namespace default
    /// lift) lands at ONE substrate method here and both downstream
    /// consumers pick up the upgrade mechanically — no per-callsite
    /// hand-edit at `resolve_pool` / `controller_allocation
    /// ::reconcile_inner`.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.metadata.name.as_deref() == Some(<candidate>)` chain
    /// recurred at two hand-authored sites past the ★★ PRIME-
    /// DIRECTIVE ≥ 2 duplication trigger, and is lifted to ONE owner
    /// here). THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — the pins bind the missing-slot corner (`None ⇒
    /// false`, even against a `""` candidate) + the populated-slot
    /// equal corner + the populated-slot unequal corner + the
    /// byte-identical parity with the pre-lift `.as_deref() == Some
    /// (<candidate>)` chain + the disjoint semantics vs. the
    /// `_or_empty` sibling family, so a regression that drifted any
    /// surface at `tests::has_name_*` here rather than as silent
    /// operator-facing skew between the two `find` closures the
    /// primitive owns).
    #[must_use]
    pub fn has_name(&self, candidate: &str) -> bool {
        self.metadata.name.as_deref() == Some(candidate)
    }

    /// The namespaced-CRD constructor composer on the `EphemeralPool`
    /// axis: forwards `(name, spec)` to the kube-derived
    /// [`Self::new`] constructor + stamps `metadata.namespace` with
    /// the caller-supplied slot in ONE step. The ONE-liner collapse
    /// of the paired `let mut p = EphemeralPool::new(<name>, <spec>);
    /// p.meta_mut().namespace = Some(<ns>.into());` incantation every
    /// pool-side test fixture restated by hand pre-lift.
    ///
    /// Pre-lift the 2-line construct-then-set-namespace chain was
    /// hand-authored at FOUR sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold in `tatara-pool-reconciler`, all
    /// composing a namespaced `EphemeralPool` fixture from a `name`
    /// slot and a `PoolSpec`:
    /// * `router::pool` — the selector-routing test fixture pinned
    ///   to `"ephemeral-pools"`.
    /// * `pool_decide::pool` — the desired-count-loop test fixture
    ///   pinned to `"pools"`.
    /// * `desired::pool` — the replacement-policy test fixture
    ///   pinned to `"pools"`.
    /// * `allocation_decide::pool` — the allocation-decision test
    ///   fixture pinned to the caller-supplied `ns` slot.
    ///
    /// All four sites walked the SAME 2-line chain and all four
    /// wanted the `EphemeralPool` back with `metadata.namespace`
    /// stamped as `Some(<ns>.into())`. Post-lift each callsite reads
    /// `EphemeralPool::new_in(<name>, <ns>, <spec>)` and the produced
    /// value feeds the same downstream reconciler-input `Vec<
    /// EphemeralPool>` unchanged.
    ///
    /// The `impl Into<String>` at the `namespace` slot matches the
    /// sibling `impl Into<String>`-widening discipline the workspace's
    /// other namespaced-CRD-adjacent composers walk
    /// ([`crate::pool::PoolMember::unallocated`] on the
    /// `process_name` slot, [`crate::pool::AllocationRef::new`] on the
    /// `(name, namespace)` slot pair, [`crate::allocation::
    /// Requestor::kind_only`] on the `kind` slot) and accepts BOTH
    /// `&'static str` (the majority pre-lift caller shape) AND owned
    /// `String` at the SAME signature.
    ///
    /// Peer to [`crate::allocation::EphemeralAllocation::new_in`] on
    /// the sister `EphemeralAllocation` CRD — the two primitives
    /// partition the namespaced-CRD-constructor family axis for the
    /// two pool-adjacent CRDs the workspace stamps at reconciler
    /// fixture / GitHub-webhook-emitter time. A future normalization
    /// (a per-fleet virtual-cluster prefix rewrite on the `namespace`
    /// slot, a per-cluster canonical case-fold pass, a
    /// `generateName` fallback on the `name` slot, an operator-scoped
    /// default namespace for cluster-local test rigs, an audit-tag
    /// stamped on every fixture-emitted CRD for post-hoc grep
    /// discipline) lands at ONE primitive body per CRD and every
    /// downstream fixture consumer inherits the upgrade mechanically.
    ///
    /// `#[must_use]` on the return keeps a caller from composing the
    /// namespaced value and dropping it un-passed to a reconciler-
    /// input slot or an assertion helper.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the 2-line construct-then-set-namespace chain recurred at
    /// FOUR hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, spanning one crate but four modules, and
    /// is lifted to ONE substrate owner here). THEORY.md §II.1
    /// invariant 5 (composition preserves proofs — the pins below
    /// bind the (name-slot → metadata.name, ns-slot → metadata.
    /// namespace, spec-slot → spec) slot-projection triple + the
    /// byte-identical parity with the pre-lift 2-line chain across
    /// the two representative `impl Into<String>` value shapes
    /// (`&'static str` and owned `String`) + the sibling-composer
    /// coherence with [`Self::new`]).
    #[must_use]
    pub fn new_in(name: &str, namespace: impl Into<String>, spec: PoolSpec) -> Self {
        // Routes through the ONE substrate owner of the
        // `metadata.namespace` stamp — the [`crate::PlacedInNamespace`]
        // blanket-impl trait over `kube::Resource<DynamicType = ()>`.
        // Byte-identical to the pre-lift 3-line body
        // (`Self::new(name, spec); metadata.namespace = Some(namespace
        // .into())`); the trait-forwarding form collapses the mutation
        // duplication with the sibling per-CRD composer
        // [`crate::allocation::EphemeralAllocation::new_in`] and with
        // the render-fixture site on `Process` that has no per-CRD
        // `new_in` sibling.
        use crate::PlacedInNamespace;
        Self::new(name, spec).in_namespace(namespace)
    }
}

/// What the pool reconciler does when a member reaches `Failed`.
///
/// Sibling closed-set lifts on the same `tatara-process` axis:
/// [`crate::compliance::VerificationPhase::ALL`],
/// [`crate::signal::SighupStrategy::ALL`],
/// [`crate::spec::MustReachPhase::ALL`],
/// [`crate::intent::WorkloadKind::ALL`],
/// [`crate::export::ReportFormat::ALL`],
/// [`crate::encapsulates::EncapsulationMode::ALL`],
/// [`crate::export::ExportTrigger::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`].
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Serialize,
    Deserialize,
    JsonSchema,
    PartialEq,
    Eq,
    Hash,
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
pub enum ReplacementPolicy {
    /// **Default** — Failed member is reaped + replaced immediately
    /// (pool stays at `desired` count). Most production-like.
    #[default]
    ReplaceImmediate,
    /// Failed member stays for inspection; pool runs short until the
    /// operator manually reaps it. Useful for debugging.
    HoldFailed,
    /// Failed member triggers pool-wide pause: `desired` is
    /// effectively 0 until the operator manually resumes via a
    /// pool-status patch. Used for "halt on any failure" workflows.
    PausePool,
}

impl ReplacementPolicy {
    /// The closed set of replacement policies — single source of truth
    /// that drives the `as_str` / Display / `FromStr` triad and the
    /// `replaces_failed` / `pauses_on_failure` predicate pair. Adding a
    /// fourth variant lands at one `ALL` entry + one `as_str` arm + one
    /// predicate arm per projection — exhaustively checked by the
    /// compiler (the `[Self; 3]` array literal forces the arity) and by
    /// the predicate-pair injectivity test below (a new variant must
    /// land in its own (replaces_failed, pauses_on_failure) bucket or
    /// the author has to extend the consumer dispatch in
    /// `tatara-pool-reconciler::desired::PoolConvergence::decide`).
    pub const ALL: [Self; 3] = [Self::ReplaceImmediate, Self::HoldFailed, Self::PausePool];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim AND the CRD `enum:`
    /// enumeration the pool reconciler stamps on the
    /// `ephemeralpools.tatara.pleme.io` schema. Pinned by
    /// `replacement_policy_as_str_matches_serde` so a variant rename
    /// can't drift between the typed surface, the CRD enum, the YAML
    /// wire format AND the operator-facing diagnostic (the
    /// `desired.rs` Pause reason composes `policy={policy}` via
    /// Display, not a hard-coded `"PausePool"` literal that would
    /// silently rot).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReplaceImmediate => "ReplaceImmediate",
            Self::HoldFailed => "HoldFailed",
            Self::PausePool => "PausePool",
        }
    }

    /// Should the pool auto-spawn a replacement for a Failed member?
    /// Closed-set match (not `matches!`) so a future variant triggers
    /// the compiler's exhaustiveness check at this site rather than
    /// silently defaulting to `false`. Paired with
    /// `pauses_on_failure` they form the two-axis projection
    /// consumers in `tatara-pool-reconciler::desired::PoolConvergence`
    /// pattern-match against — `replaces_failed` true ⇒ emit
    /// `ReapFailed` per failure; `pauses_on_failure` true with any
    /// failure ⇒ emit `Pause` and short-circuit. The pair is
    /// `(true, false) | (false, false) | (false, true)` — pinned
    /// injective by `replacement_policy_predicate_pair_is_injective`.
    pub const fn replaces_failed(self) -> bool {
        match self {
            Self::ReplaceImmediate => true,
            Self::HoldFailed | Self::PausePool => false,
        }
    }

    /// Should reaching Failed on any member pause the whole pool?
    /// See `replaces_failed` for the closed-match rationale + the
    /// predicate-pair contract.
    pub const fn pauses_on_failure(self) -> bool {
        match self {
            Self::PausePool => true,
            Self::ReplaceImmediate | Self::HoldFailed => false,
        }
    }
}

// `impl FromStr for ReplacementPolicy` + `impl tatara_lisp::ClosedSet for
// ReplacementPolicy` + `impl fmt::Display for ReplacementPolicy` are
// generated by `#[derive(tatara_closed_set::DeriveClosedSet)]` on the enum
// declaration above. `label` delegates to the inherent
// `ReplacementPolicy::as_str` via `#[closed_set(via = "as_str")]` so the
// PascalCase wire-format projection stays load-bearing (matches the
// serde `rename_all = "PascalCase"` output AND the
// `tatara-pool-reconciler::desired::PoolConvergence` Pause reason
// emission verbatim) while generic `T: ClosedSet` consumers reach the
// STABLE workspace-wide name (`label`); Display delegates to the same
// inherent projection via `#[closed_set(display)]` so the
// `Pause` reason emitter's `policy={policy}` composition stays
// pinned on the closed-set algebra rather than on a hand-rolled
// `fmt::Display` block per implementor.

// `pub struct UnknownReplacementPolicy(pub String)` is generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label
// `"replacement policy"` matches the prior hand-rolled
// `#[error("unknown replacement policy: {0}")]` verbatim. Symmetric to
// [`UnknownMemberState`], [`UnknownPoolPhase`], [`UnknownReturnPolicy`],
// [`crate::export::UnknownReportFormat`],
// [`crate::export::UnknownChannelKind`],
// [`crate::export::UnknownExportTrigger`],
// [`crate::lifetime::UnknownTeardownPolicy`],
// [`crate::boundary::UnknownConditionKind`], and
// [`crate::phase::UnknownPhase`].

fn default_free_ttl() -> String {
    "24h".to_string()
}
fn default_max_allocation_ttl() -> String {
    "4h".to_string()
}

/// `EphemeralPool.status` — observed pool population state.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PoolStatus {
    /// Pool lifecycle phase.
    #[serde(default)]
    pub phase: PoolPhase,

    /// When the pool entered the current phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_since: Option<DateTime<Utc>>,

    /// Number of members currently in `Free` state (ready for allocation).
    #[serde(default)]
    pub ready_count: u32,

    /// Number of members currently `Allocated`.
    #[serde(default)]
    pub allocated_count: u32,

    /// Number of members currently `Spawning` (not yet Attested).
    #[serde(default)]
    pub spawning_count: u32,

    /// Number of members currently `Returning` (reset or replace
    /// in progress).
    #[serde(default)]
    pub returning_count: u32,

    /// Member ledger — one entry per pool slot.
    #[serde(default)]
    pub members: Vec<PoolMember>,

    /// Operator-visible message (e.g., "scaled down to floor").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Standard Kubernetes Conditions.
    #[serde(default)]
    pub conditions: Vec<PoolCondition>,
}

/// One pool slot's state.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PoolMember {
    /// `metadata.name` of the backing Process.
    pub process_name: String,
    /// Pool member's current slot state.
    pub state: MemberState,
    /// When the member entered the current state.
    pub entered_state_at: DateTime<Utc>,
    /// If allocated: the AllocationRef holding this slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allocation_ref: Option<AllocationRef>,
}

impl PoolStatus {
    /// Substrate constructor for the observed [`PoolStatus`] seed:
    /// composes the `(phase, phase_since, ready/allocated/spawning
    /// /returning counts, members, message, conditions)` 9-slot record
    /// every pool-reconciler status-patch site restated by hand pre-
    /// lift. The four counters ride a SINGLE closed-set-driven fold
    /// over the members list (one pass rather than four independent
    /// filter-and-count passes); the `message` + `conditions` slots
    /// stay at their invariant `None` / `vec![]` defaults every pre-
    /// lift caller stamped verbatim, and `phase_since` is derived from
    /// the caller-supplied `now` timestamp so the constructor stays
    /// clock-injectable rather than implicitly reading wall time.
    ///
    /// Pre-lift the 11-line
    /// ```rust,ignore
    /// PoolStatus {
    ///     phase,
    ///     phase_since: Some(Utc::now()),
    ///     ready_count: count_state(&members, MemberState::Free),
    ///     allocated_count: count_state(&members, MemberState::Allocated),
    ///     spawning_count: count_state(&members, MemberState::Spawning),
    ///     returning_count: count_state(&members, MemberState::Returning),
    ///     members: members.clone(),
    ///     message: None,
    ///     conditions: vec![],
    /// }
    /// ```
    /// incantation was hand-authored at TWO sites past the ★★ PRIME-
    /// DIRECTIVE ≥ 2 duplication threshold in
    /// `tatara-pool-reconciler::controller_pool::reconcile_inner`,
    /// both restating the same 4-slot count fanout + defaults:
    /// * The `desired > 0` path — status patch after the
    ///   convergence-action loop when the operator drives the pool
    ///   through the R11 desired-count invariant.
    /// * The legacy allocation-driven path (`desired == 0`) — status
    ///   patch after the [`crate::pool::PoolDecision`] apply loop.
    ///
    /// Both sites walked the SAME 4-slot count fanout on the SAME
    /// four `MemberState` variants (Free/Allocated/Spawning/Returning)
    /// and stamped the SAME defaults (`message: None`, `conditions:
    /// vec![]`), even though the four counters walked the members list
    /// four independent times pre-lift when a single pass suffices.
    /// Post-lift both callers write
    /// `PoolStatus::observed(phase, members, Utc::now())` and share
    /// ONE substrate owner; a future counter slot (e.g., a
    /// `warming_count` for a `MemberState::Warming` variant between
    /// Spawning and Free) plugs into the fold at ONE match arm and
    /// both status-patch sites inherit the new slot mechanically.
    ///
    /// The `Failed` variant is deliberately absent from the fold — no
    /// `PoolStatus` slot counts failed members (they surface via
    /// `pool_phase_from_members`'s `PoolPhase::Degraded` transition
    /// instead), and the closed-set match on
    /// [`MemberState`] pins that a future variant which SHOULD count
    /// toward one of the four buckets triggers the compiler's
    /// exhaustiveness check at this fold rather than silently sinking
    /// into `Failed`'s no-op arm.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the 11-line status-seed incantation recurred at two hand-
    /// authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// trigger, and is lifted to ONE owner here). THEORY.md §II.1
    /// invariant 5 (composition preserves proofs — the pins bind the
    /// 4-slot count fanout + the closed-set exhaustiveness on
    /// `MemberState` + the invariant defaults, so a regression that
    /// dropped a counter slot or swapped a variant surfaces at
    /// `tests::pool_status_observed_*` rather than as silent operator-
    /// facing skew between the two status-patch sites on the SAME
    /// pool).
    #[must_use]
    pub fn observed(phase: PoolPhase, members: Vec<PoolMember>, now: DateTime<Utc>) -> Self {
        let (ready_count, allocated_count, spawning_count, returning_count) =
            PoolMember::state_count_fanout(&members);
        Self {
            phase,
            phase_since: Some(now),
            ready_count,
            allocated_count,
            spawning_count,
            returning_count,
            members,
            message: None,
            conditions: vec![],
        }
    }

    /// Wall-clock-anchored peer of [`Self::observed`] — the ONE
    /// substrate owner of the 4-arg `PoolStatus::observed(phase,
    /// members, Utc::now())` composition every pool-reconciler
    /// status-patch site that reads the wall clock at tick-time
    /// hand-authored pre-lift.
    ///
    /// # Why it exists
    ///
    /// Pre-lift the 4-arg `PoolStatus::observed(phase, members.clone(),
    /// chrono::Utc::now())` chain was hand-authored at TWO sites past the
    /// ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold in
    /// `tatara-pool-reconciler::controller_pool::reconcile_inner`, each
    /// pairing the 3-arg [`Self::observed`] composer with a
    /// `chrono::Utc::now()` third argument at the status-patch stamp:
    ///
    /// * The `desired > 0` path — status patch after the
    ///   convergence-action loop when the operator drives the pool
    ///   through the R11 desired-count invariant.
    /// * The legacy allocation-driven path (`desired == 0`) — status
    ///   patch after the [`crate::pool::PoolDecision`] apply loop.
    ///
    /// Both sites walked the SAME 4-arg call with the SAME
    /// `chrono::Utc::now()` third argument — the wall-clock projection
    /// had no per-callsite variation. Post-lift both consumers share ONE
    /// substrate owner for the wall-clock-at-tick projection; a future
    /// clock swap (a monotonic clock cross-check, a per-reconciler
    /// injected time source, a test-only override at the production
    /// callsite via feature flag) lands at ONE substrate function and
    /// every pool-reconciler status-patch site inherits the upgrade
    /// mechanically.
    ///
    /// The 3-arg [`Self::observed`] peer stays load-bearing for test
    /// callers — the injected-`now` shape is what unit tests use to
    /// drive the clock deterministically (every
    /// `PoolStatus::observed(phase, members, seeded_now)` in this
    /// module's own test suite reads that surface). This peer is
    /// production-only: pinning the wall-clock at the substrate site
    /// means no test can accidentally consume it without the
    /// deterministic-clock injection that makes the test meaningful.
    ///
    /// Sibling of
    /// [`crate::lifetime_clock::evaluate_now`] on the (typed
    /// pure-fn, wall-clock-anchored peer) axis — both primitives own
    /// the "read the wall clock at tick-time" projection on a peer
    /// clock-injectable primitive so the workspace's timed-decision
    /// family stays uniform across `EphemeralLifetime` TTL expiry and
    /// `PoolStatus` observed-state stamp.
    ///
    /// # Invariants
    ///
    /// - **Same shape:** returns the SAME [`PoolStatus`] the 3-arg
    ///   [`Self::observed`] returns when passed `chrono::Utc::now()` as
    ///   the third argument. This is a delegation, not a
    ///   re-implementation.
    /// - **Wall-clock read once:** `Utc::now()` is called exactly ONCE
    ///   per invocation, at the primitive's body, so a future consumer
    ///   that chains two `observed_now` calls back-to-back still sees
    ///   monotonic `now` reads (each call reads a fresh instant, not a
    ///   cached one) — matches the pre-lift shape where each of the two
    ///   status-patch sites computed its own `chrono::Utc::now()` at its
    ///   own line.
    ///
    /// # `#[must_use]`
    ///
    /// Every consumer feeds the returned [`PoolStatus`] into
    /// `tatara_process::patch::merge_status(&pool_api, &name, &<status>)`
    /// or a peer status-patch call. Dropping the return means the
    /// observation composed for no observable reason — the attribute
    /// surfaces that as a warning at every call site.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the 4-arg call with `chrono::Utc::now()` as the third argument
    /// recurred at 2 hand-authored sites past the ★★ PRIME-DIRECTIVE
    /// ≥ 2 duplication trigger, lifted onto the ONE workspace-wide
    /// substrate owner here). THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the wall-clock projection lives at ONE site
    /// so a future clock swap reaches both consumers through one edit).
    #[must_use]
    pub fn observed_now(phase: PoolPhase, members: Vec<PoolMember>) -> Self {
        Self::observed(phase, members, Utc::now())
    }
}

impl PoolMember {
    /// Substrate primitive: single-pass closed-set fold over a
    /// `[PoolMember]` slice producing the `(ready, allocated,
    /// spawning, returning)` 4-tuple every `PoolStatus` seed stamps at
    /// its four counter slots. The `Failed` arm is a no-op (no
    /// `PoolStatus` counter tracks failed members — they surface via
    /// [`PoolPhase::Degraded`] instead), pinned by the closed-set
    /// match so a future variant that SHOULD count toward one of the
    /// four buckets triggers the compiler's exhaustiveness check here
    /// rather than silently falling through.
    ///
    /// Consumed by [`PoolStatus::observed`]. A caller that needs a
    /// single per-variant count outside the status-seed fanout should
    /// keep spelling `members.iter().filter(...).count()` rather than
    /// walking this 4-tuple — the fanout is shaped for the
    /// `PoolStatus` fill, not for arbitrary per-variant queries.
    #[must_use]
    pub fn state_count_fanout(members: &[Self]) -> (u32, u32, u32, u32) {
        let mut ready = 0u32;
        let mut allocated = 0u32;
        let mut spawning = 0u32;
        let mut returning = 0u32;
        for m in members {
            match m.state {
                MemberState::Free => ready += 1,
                MemberState::Allocated => allocated += 1,
                MemberState::Spawning => spawning += 1,
                MemberState::Returning => returning += 1,
                MemberState::Failed => {}
            }
        }
        (ready, allocated, spawning, returning)
    }

    /// Substrate primitive: single-pass closed-set collection of the
    /// `process_name` axis over a `[PoolMember]` slice into an owned
    /// `HashSet<String>` — the O(1)-lookup shape every spawn-arm on
    /// the workspace builds pre-collision-check against a candidate
    /// [`crate::pool::PoolMember::process_name`] produced by
    /// [`tatara-pool-reconciler::naming::member_process_name`].
    ///
    /// Pre-lift the 2-line
    /// `members.iter().map(|m| m.process_name.clone()).collect()`
    /// chain was hand-authored at TWO sites past the ★★ PRIME-
    /// DIRECTIVE ≥ 2 duplication threshold in
    /// `tatara-pool-reconciler::controller_pool`, both restating the
    /// SAME `process_name` projection through the SAME
    /// `iter → map → collect` shape and both feeding a `.contains
    /// (&candidate)` probe:
    /// * `reconcile_inner`'s legacy allocation-driven
    ///   `PoolDecision::Spawn` arm (`desired == 0` path) —
    ///   collision-set for
    ///   `member_process_name(&pool_name, &pool_uid, slot)` per spawn
    ///   slot.
    /// * `apply_convergence_actions` — collision-set for the SAME
    ///   composer inside the R11 desired-count
    ///   `ConvergenceAction::CreateMember` loop.
    ///
    /// Post-lift both consumers share ONE substrate owner; the
    /// composed `HashSet<String>` still feeds the same
    /// `HashSet::<String>::contains(&candidate)` probe at each
    /// callsite unchanged. A future normalization step on the
    /// occupied-name axis (case-fold before insertion, a per-cluster
    /// prefix strip, deduplication against a sibling stale-name
    /// registry, exclusion of `Returning`/`Failed` members that no
    /// longer own their slot) lands at ONE substrate method rather
    /// than being restated at each callsite.
    ///
    /// Sibling to [`Self::state_count_fanout`] on the `(collection
    /// shape × slice-owned fold)` axis: both primitives fold a
    /// `[PoolMember]` slice into one caller-shaped aggregate in a
    /// single pass, both are `#[must_use]`, both take the slice by
    /// reference so no caller has to reshape its `Vec<PoolMember>` or
    /// `Vec<PoolMember>` slice upstream.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `HashSet<String>` collision-set shape recurred at TWO
    /// hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the axis (`process_name`), the aggregate shape
    /// (`HashSet<String>`), the empty-slice corner, and the
    /// duplicate-name deduplication semantics `HashSet` provides
    /// implicitly, so a regression at any of those surfaces at
    /// `tests::process_names_set_*` rather than as silent occupied-
    /// slot skew at either spawn arm).
    #[must_use]
    pub fn process_names_set(members: &[Self]) -> std::collections::HashSet<String> {
        members.iter().map(|m| m.process_name.clone()).collect()
    }

    /// Substrate composer for the unallocated `PoolMember` seed: the
    /// 4-slot `{ process_name, state, entered_state_at, allocation_ref:
    /// None }` fixture literal every non-`Allocated`-role callsite
    /// stamped by hand pre-lift.
    ///
    /// Pre-lift the 4-slot struct literal `PoolMember { process_name,
    /// state, entered_state_at, allocation_ref: None }` was hand-authored
    /// at FIVE workspace-wide sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold across TWO crates:
    /// * `tatara-pool-reconciler::controller_pool::reconcile_inner` —
    ///   the production per-owned-Process seed built inside the
    ///   `for p in all_processes.items` walk; `entered_state_at` rides
    ///   in from [`crate::prelude::Process::observed_phase_since`] with
    ///   the `Utc::now` fallback at the callsite.
    /// * `tatara-pool-reconciler::pool_decide::tests::member` — test
    ///   helper for the pool decision suite; `entered_state_at` rides
    ///   in from [`crate::time::seconds_ago`].
    /// * `tatara-pool-reconciler::allocation_decide::tests::member` —
    ///   test helper for the allocation decision suite;
    ///   `entered_state_at` rides in from `Utc::now`.
    /// * `tatara-process::pool::tests::member` — test helper for the
    ///   fanout / status suite; `entered_state_at` rides in from the
    ///   epoch anchor `DateTime::<Utc>::from_timestamp(0, 0)`.
    /// * `tatara-process::pool::tests::named_member` — test helper for
    ///   the `process_names_set` suite; same epoch anchor.
    ///
    /// Every one of those FIVE sites pinned `allocation_ref: None`
    /// verbatim — no `PoolMember` construction site in the workspace
    /// pairs `allocation_ref: Some(<ref>)` with a hand-authored 4-slot
    /// struct literal, so this composer's `None` slot is safe by
    /// construction (the compiler exhaustiveness check on the struct's
    /// four fields catches a future 5th slot addition here rather than
    /// at any of the callsites).
    ///
    /// Post-lift every consumer writes
    /// `PoolMember::unallocated(<name>, <state>, <anchor>)` and shares
    /// ONE substrate owner; a future promotion of the unallocated shape
    /// (a per-cluster clock-skew guard on the `entered_state_at`
    /// anchor, a canonical rename of the None-slot to a typed
    /// `Unallocated` marker, a lint-friendly closed-set restriction to
    /// the four `MemberState` variants that legitimately carry no
    /// `allocation_ref`) lands at ONE substrate site and every downstream
    /// consumer inherits the upgrade mechanically.
    ///
    /// `impl Into<String>` accepts both `&str` literals (every test
    /// helper site) and owned `String` produced by
    /// [`crate::prelude::Process::owned_name_or_empty`] (the production
    /// controller-pool site) without widening the signature.
    ///
    /// Sibling to [`AllocationRef::new`] on the substrate-composer
    /// axis: both take `impl Into<String>`-gated identity slots and
    /// return their owner-type by value; [`AllocationRef::new`] owns
    /// the (name, namespace) pair, this composer owns the four-slot
    /// unallocated-member seed.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the 4-slot unallocated-`PoolMember` seed recurred at FIVE hand-
    /// authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// trigger, and is lifted to ONE owner here). THEORY.md §II.1
    /// invariant 5 (composition preserves proofs — the pins bind the
    /// four-slot fill AND the `allocation_ref: None` invariant AND the
    /// caller-clock-injectability of `entered_state_at`, so a
    /// regression that drifts any surface fails at
    /// `tests::pool_member_unallocated_*` rather than as silent
    /// operator-facing skew between the production controller-pool seed
    /// and the three test-suite helpers on the SAME `PoolMember`
    /// shape).
    #[must_use]
    pub fn unallocated(
        process_name: impl Into<String>,
        state: MemberState,
        entered_state_at: DateTime<Utc>,
    ) -> Self {
        Self {
            process_name: process_name.into(),
            state,
            entered_state_at,
            allocation_ref: None,
        }
    }
}

/// Light reference to an `EphemeralAllocation`.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AllocationRef {
    pub name: String,
    pub namespace: String,
}

impl AllocationRef {
    /// Substrate constructor for [`AllocationRef`]: composes the
    /// `(name, namespace)` pair through ONE `impl Into<String>`-gated
    /// entry point — the ONE-liner collapse of the paired
    /// `AllocationRef { name: n.into(), namespace: ns.into() }`
    /// struct-literal incantation every downstream consumer restated
    /// by hand pre-lift.
    ///
    /// Pre-lift the `AllocationRef { name, namespace }` struct-literal
    /// was hand-authored at FOUR production sites past the ★★ PRIME-
    /// DIRECTIVE ≥ 2 duplication threshold across the workspace, all
    /// composing an owned `(name: String, namespace: String)` pair
    /// under one of two roles:
    /// * `tatara-pool-reconciler::controller_allocation::reconcile_inner`
    ///   Bind path — the `assignedProcess` status slot's ref, pairing
    ///   the just-bound member Process name with the allocation's
    ///   containing namespace.
    /// * `tatara-pool-reconciler::controller_allocation::reconcile_inner`
    ///   Release path — the same `assignedProcess` slot shape, stamped
    ///   at the release-side status patch alongside the (unchanged)
    ///   `boundPool` ref.
    /// * `tatara-pool-reconciler::allocation_decide::AllocationConvergenceCtx::observe`
    ///   pool-matched handle — the `matched_pool` slot's ref, pairing
    ///   [`EphemeralPool::owned_name_or_empty`] with the pool's
    ///   containing namespace.
    /// * `tatara-github-watcher::allocation_factory::allocation_from_pr`
    ///   — the `pool_ref` slot on the `AllocationSpec` emitted from a
    ///   PullRequestEvent, pairing the operator-configured pool name
    ///   with the watcher's target namespace.
    ///
    /// All FOUR sites walked the SAME two-field struct-literal shape
    /// — an owned name half, an owned namespace half — differing only
    /// in provenance. Post-lift each callsite reads
    /// `AllocationRef::new(name, ns)` and the produced value feeds the
    /// same downstream slot (`assignedProcess` / `bound_pool` /
    /// `matched_pool` / `spec.pool_ref`) unchanged. The `impl Into<String>`
    /// signature accepts every provenance the pre-lift sites carried —
    /// owned `String` (the reconciler's owned-form projections), `&str`
    /// (the factory's `n.to_string()` / `namespace.to_string()`
    /// borrow-to-owned promotions), `Cow<str>`, and every other
    /// `Into<String>` implementor — so no callsite has to change its
    /// upstream provenance to route through the primitive.
    ///
    /// Return-form axis: owned [`AllocationRef`] — the wire-format
    /// shape [`crate::pool::AllocationRef`]'s serde `rename_all =
    /// "camelCase"` produces on both spec (`poolRef`) and status
    /// (`boundPool` / `assignedProcess`) slots. The primitive owns
    /// the axis-order `(name, namespace)` — the same order the four
    /// consumers spelled — so a slot swap surfaces at the
    /// `allocation_ref_new_positional_axis_order` pin below rather
    /// than as silent `<namespace>/<name>` inversion downstream.
    ///
    /// Peer to the sibling substrate primitives already opened on the
    /// pool-side (name, namespace) axis pair:
    /// [`EphemeralPool::name_or_empty`] (borrow-form name),
    /// [`EphemeralPool::owned_name_or_empty`] (owned-form name); this
    /// constructor is the composer that folds the owned-form projections
    /// into the wire-format ref shape.
    ///
    /// A future refactor of [`AllocationRef`]'s field set (a
    /// `resource_kind: String` field for cross-CRD refs, an
    /// `api_version: String` field for FQN references, a
    /// canonicalization pass over the namespace half, a non-empty-name
    /// gate) lands at ONE substrate constructor site here and every
    /// downstream consumer inherits the upgrade mechanically — no per-
    /// callsite hand-edit at the FOUR reconciler + factory sites.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `AllocationRef { name, namespace }` struct-literal shape
    /// recurred at four hand-authored sites past the ★★ PRIME-
    /// DIRECTIVE ≥ 2 duplication trigger, and is lifted to ONE owner
    /// here). THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — the pins bind the positional axis-order + the
    /// `Into<String>` provenance closure + byte-identical parity with
    /// the pre-lift struct-literal + `PartialEq` coherence with the
    /// hand-authored form, so a regression that reshaped any surface
    /// at `tests::allocation_ref_new_*` rather than as silent
    /// operator-facing skew between the assignedProcess / bound_pool
    /// / matched_pool / spec.pool_ref slots on the SAME allocation).
    #[must_use]
    pub fn new(name: impl Into<String>, namespace: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            namespace: namespace.into(),
        }
    }
}

/// Per-slot state in the pool's free list.
///
/// Sibling closed-sets on the `EphemeralPool` axis: [`ReplacementPolicy::ALL`]
/// (the on-failure policy that the pool reconciler dispatches against
/// the [`Self::is_failed`] projection), [`ReturnPolicy::ALL`] (the
/// release-time disposition that transitions an [`Self::Allocated`]
/// member into [`Self::Returning`] before it either re-enters
/// [`Self::Free`] or gets [`Self::Spawning`]'d as a fresh slot).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
pub enum MemberState {
    /// Pool reconciler is creating/converging the backing Process.
    Spawning,
    /// Process is `Attested`; ready for allocation.
    Free,
    /// Held by an `EphemeralAllocation`.
    Allocated,
    /// Return policy is being applied (Reset → reset Job; Replace →
    /// Process is being torn down and recreated).
    Returning,
    /// Permanent failure — the member needs operator attention.
    Failed,
}

impl MemberState {
    /// The closed set of member states — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad AND the
    /// `is_failed` / `counts_toward_supply` predicate pair. Adding a
    /// sixth variant lands at one `ALL` entry + one `as_str` arm + one
    /// arm per predicate — exhaustively checked by the compiler (the
    /// `[Self; 5]` array literal forces the arity) and by the
    /// per-variant truth-table contract test (a new variant must
    /// declare its own `(is_failed, counts_toward_supply)` projection
    /// or the consumer dispatch in
    /// `tatara-pool-reconciler::controller_pool::pool_phase_from_members`
    /// and `tatara-pool-reconciler::pool_decide::decide_pool_reconcile`
    /// will silently bucket it into the wrong lifecycle column).
    pub const ALL: [Self; 5] = [
        Self::Spawning,
        Self::Free,
        Self::Allocated,
        Self::Returning,
        Self::Failed,
    ];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim AND the CRD `enum:`
    /// enumeration that `ephemeralpools.tatara.pleme.io` stamps on
    /// `status.members[].state`. Pinned by
    /// `member_state_as_str_matches_serde` so a variant rename can't
    /// drift between the typed surface, the CRD enum, the YAML wire
    /// format AND any future operator-facing diagnostic that composes
    /// `state={state}` via Display rather than a hard-coded literal
    /// that would silently rot.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Spawning => "Spawning",
            Self::Free => "Free",
            Self::Allocated => "Allocated",
            Self::Returning => "Returning",
            Self::Failed => "Failed",
        }
    }

    /// Is this member in a permanent-failure state — needs operator
    /// attention? Closed-set match (not `matches!`) so a future variant
    /// triggers the compiler's exhaustiveness check at this site rather
    /// than silently defaulting to `false`. Consumed by
    /// `tatara-pool-reconciler::pool_decide::decide_pool_reconcile` to
    /// gate the highest-priority `ReplaceMembers` decision branch — a
    /// future variant that should also trigger replacement (e.g.
    /// `MemberState::Quarantined`) flips this predicate at one site
    /// and inherits the priority-1 dispatch without touching the
    /// consumer match arm.
    pub const fn is_failed(self) -> bool {
        match self {
            Self::Failed => true,
            Self::Spawning | Self::Free | Self::Allocated | Self::Returning => false,
        }
    }

    /// Does this member contribute to the pool's *available supply*
    /// (current ready slots + slots coming online)? Closed-set match so
    /// a future variant triggers the compiler's exhaustiveness check.
    /// Consumed by
    /// `tatara-pool-reconciler::controller_pool::pool_phase_from_members`
    /// — the `(free + spawning)` supply calc collapses into one
    /// predicate-driven filter, so a future "warming-up" state
    /// (`MemberState::Warming` between Spawning and Free) plugs into
    /// the supply count at one site rather than three. Disjoint with
    /// `is_failed` — pinned by `member_state_failed_implies_no_supply`
    /// (a Failed member can never count toward supply; the pool
    /// reconciler would otherwise double-count failures as available
    /// capacity).
    pub const fn counts_toward_supply(self) -> bool {
        match self {
            Self::Free | Self::Spawning => true,
            Self::Allocated | Self::Returning | Self::Failed => false,
        }
    }
}

// `impl FromStr for MemberState` + `impl tatara_lisp::ClosedSet for
// MemberState` + `impl fmt::Display for MemberState` are generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` on the enum declaration
// above. `label` delegates to the inherent `MemberState::as_str` via
// `#[closed_set(via = "as_str")]` so the
// `pool_phase_from_members` supply calc can keep keying on
// `counts_toward_supply` against the typed variant while a generic
// `T: ClosedSet` consumer reaches the STABLE workspace-wide name
// (`label`) without knowing this enum lives in `tatara-process::pool`;
// Display delegates to the same inherent projection via
// `#[closed_set(display)]` so the diagnostic emitter's
// `state={state}` composition stays pinned on the closed-set algebra.

// `pub struct UnknownMemberState(pub String)` is generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"member state"`
// matches the prior hand-rolled `#[error("unknown member state: {0}")]`
// verbatim. Symmetric to [`UnknownReplacementPolicy`],
// [`UnknownPoolPhase`], [`UnknownReturnPolicy`],
// [`crate::lifetime::UnknownTeardownPolicy`],
// [`crate::boundary::UnknownConditionKind`], and
// [`crate::phase::UnknownPhase`].

/// Pool lifecycle phase (observed across the whole pool population).
///
/// Sibling closed-set on the same `EphemeralPool` axis as
/// [`MemberState::ALL`] (the per-slot lifecycle this phase aggregates
/// over via [`MemberState::counts_toward_supply`]),
/// [`ReplacementPolicy::ALL`] (on-failure policy) and
/// [`ReturnPolicy::ALL`] (release-time disposition). Together with
/// `MemberState`, this closes the pool reconciler's
/// `(slot-state, pool-phase)` two-tier observation algebra on the
/// same closed-set discipline as the rest of `tatara-process`.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
pub enum PoolPhase {
    /// Just admitted; no members yet.
    Initializing,
    /// `ready_count == desired_size`.
    Steady,
    /// `ready_count + spawning_count < desired_size` and reconciler
    /// is creating new members.
    ScalingUp,
    /// `ready_count > desired_size` and reconciler is reaping excess.
    ScalingDown,
    /// `min_size` constraint violated.
    Degraded,
    /// Pool is being deleted; reconciler is reaping all members.
    Draining,
}

impl Default for PoolPhase {
    fn default() -> Self {
        Self::Initializing
    }
}

impl PoolPhase {
    /// The closed set of pool phases — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad AND the
    /// `is_steady` / `is_terminal` predicate pair. Adding a seventh
    /// variant lands at one `ALL` entry + one `as_str` arm + one arm
    /// per predicate — exhaustively checked by the compiler (the
    /// `[Self; 6]` array literal forces the arity) AND by the
    /// per-variant truth-table contract test (a new variant must
    /// declare its own `(is_steady, is_terminal)` projection or any
    /// future status-aggregator surface — `feira pool list
    /// --healthy`, the operator-facing condition aggregator, the
    /// desired-loop heartbeat short-circuit — will silently bucket
    /// it into the wrong lifecycle column).
    pub const ALL: [Self; 6] = [
        Self::Initializing,
        Self::Steady,
        Self::ScalingUp,
        Self::ScalingDown,
        Self::Degraded,
        Self::Draining,
    ];

    /// Canonical PascalCase wire-format projection — matches the
    /// serde `rename_all = "PascalCase"` output verbatim AND the CRD
    /// `enum:` enumeration that `ephemeralpools.tatara.pleme.io`
    /// stamps on `status.phase`. Pinned by
    /// `pool_phase_as_str_matches_serde` so a variant rename can't
    /// drift between the typed surface, the CRD enum, the YAML wire
    /// format AND any future operator-facing diagnostic that
    /// composes `phase={phase}` via Display rather than a hard-coded
    /// literal that would silently rot. Display + FromStr triad
    /// over `ALL` mirrors `MemberState` / `ReplacementPolicy` /
    /// `ReturnPolicy` / `AllocationPhase` / `TeardownPolicy` /
    /// `ConditionKind` / `ProcessPhase` / `ProcessSignal`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Initializing => "Initializing",
            Self::Steady => "Steady",
            Self::ScalingUp => "ScalingUp",
            Self::ScalingDown => "ScalingDown",
            Self::Degraded => "Degraded",
            Self::Draining => "Draining",
        }
    }

    /// Is the pool fully converged — supply matches desired, no
    /// reconciler-driven population change pending? Closed-set match
    /// (not `matches!`) so a future variant triggers the compiler's
    /// exhaustiveness check at this site rather than silently
    /// defaulting to `false`. Paired with `is_terminal` they form
    /// the two-axis projection that future status aggregators
    /// (operator-facing fleet health, `feira pool list --healthy`,
    /// the SSE filter "show non-steady pools") dispatch against —
    /// `is_steady && !is_terminal` ⇒ converged (goal state);
    /// `!is_steady && is_terminal` ⇒ being deleted (no future
    /// spawn); `!is_steady && !is_terminal` ⇒ transient
    /// (Initializing | ScalingUp | ScalingDown | Degraded — pool
    /// is in motion toward desired). The impossible bucket
    /// `(true, true)` — a draining pool that's somehow also steady
    /// — is pinned empty by `pool_phase_steady_excludes_terminal`.
    pub const fn is_steady(self) -> bool {
        match self {
            Self::Steady => true,
            Self::Initializing
            | Self::ScalingUp
            | Self::ScalingDown
            | Self::Degraded
            | Self::Draining => false,
        }
    }

    /// Is the pool in its absorbing exit state — deletion-stamped,
    /// reconciler is reaping every member, no spawn will ever
    /// happen again? Closed-set match so a future variant triggers
    /// the compiler's exhaustiveness check. See `is_steady` for the
    /// predicate-pair contract + bucket definitions.
    pub const fn is_terminal(self) -> bool {
        match self {
            Self::Draining => true,
            Self::Initializing
            | Self::Steady
            | Self::ScalingUp
            | Self::ScalingDown
            | Self::Degraded => false,
        }
    }
}

// `impl FromStr for PoolPhase` + `impl tatara_lisp::ClosedSet for PoolPhase`
// + `impl fmt::Display for PoolPhase` are generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` on the enum declaration above.
// `label` delegates to the inherent `PoolPhase::as_str` via
// `#[closed_set(via = "as_str")]` so the operator-facing
// `phase={phase}` Display composition keeps reading the same canonical
// PascalCase projection while a generic `T: ClosedSet` consumer (a
// status-aggregator filter, the `feira pool list --healthy` predicate, a
// future SSE event router) can walk every variant without knowing the
// closed set lives in `tatara-process::pool`; Display delegates to the
// same inherent projection via `#[closed_set(display)]` so the
// `phase={phase}` composition stays pinned on the closed-set algebra
// rather than a hand-rolled `fmt::Display` block.

// `pub struct UnknownPoolPhase(pub String)` is generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"pool phase"`
// matches the prior hand-rolled `#[error("unknown pool phase: {0}")]`
// verbatim. Symmetric to [`UnknownMemberState`],
// [`UnknownReplacementPolicy`], [`UnknownReturnPolicy`],
// [`crate::lifetime::UnknownTeardownPolicy`],
// [`crate::boundary::UnknownConditionKind`], and
// [`crate::phase::UnknownPhase`].

/// Standard K8s Condition shape (kept local so tatara-process doesn't
/// depend on k8s_openapi types in its public schema).
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PoolCondition {
    pub type_: String,
    pub status: String,
    pub reason: String,
    pub message: String,
    pub last_transition_time: DateTime<Utc>,
}

/// What the pool does when an allocation releases a member.
///
/// Sibling closed-set on the `EphemeralPool` axis:
/// [`ReplacementPolicy::ALL`]. Sibling closed-sets on the
/// `tatara-process` algebra: [`crate::lifetime::TeardownPolicy::ALL`]
/// (the *release*-time counterpart for non-pooled ephemeral envs),
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`].
#[derive(
    Clone,
    Copy,
    Debug,
    Hash,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    Default,
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
pub enum ReturnPolicy {
    /// Tear down the Process + create a fresh one. Safe but slow
    /// (1-2 min spin-up before the slot is Free again).
    #[default]
    Replace,
    /// Keep the Process running; run a typed `:reset` Job that wipes
    /// state (DB drop, secrets rotate). Fast (~5-10s) but depends on
    /// the reset Job being correct for the workload. API-authoritative
    /// systems are natural fits because the control API owns all state.
    Reset,
    /// Keep the Process indefinitely after release (debugging aid;
    /// operator must `feira pool reap NAME` to clean up). Useful for
    /// post-mortem of a flaky test.
    Keep,
}

impl ReturnPolicy {
    /// The closed set of return policies — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad and the
    /// `keeps_process` / `runs_reset_job` predicate pair. Adding a
    /// fourth variant lands at one `ALL` entry + one `as_str` arm +
    /// one arm per predicate — exhaustively checked by the compiler
    /// (the `[Self; 3]` array literal forces the arity) and by the
    /// predicate-pair injectivity test (a new variant must land in
    /// its own (keeps_process, runs_reset_job) bucket or the author
    /// has to extend the consumer dispatch in
    /// `tatara-pool-reconciler::return_policy::plan_return`).
    pub const ALL: [Self; 3] = [Self::Replace, Self::Reset, Self::Keep];

    /// Canonical PascalCase wire-format projection — matches the
    /// serde `rename_all = "PascalCase"` output verbatim AND the CRD
    /// `enum:` enumeration the pool reconciler stamps on the
    /// `ephemeralpools.tatara.pleme.io` schema. Pinned by
    /// `return_policy_as_str_matches_serde` so a variant rename can't
    /// drift between the typed surface, the CRD enum, the YAML wire
    /// format AND any future operator-facing diagnostic that composes
    /// `policy={policy}` via Display rather than a hard-coded literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Replace => "Replace",
            Self::Reset => "Reset",
            Self::Keep => "Keep",
        }
    }

    /// Does the pool keep the backing Process alive across release?
    /// Closed-set match (not `matches!`) so a future variant triggers
    /// the compiler's exhaustiveness check at this site rather than
    /// silently defaulting to `false`. Paired with `runs_reset_job`
    /// they form the two-axis projection that the consumer in
    /// `tatara-pool-reconciler::return_policy::plan_return` matches
    /// against — `keeps_process` false ⇒ `DeleteAndRespawn`;
    /// `keeps_process && runs_reset_job` ⇒ `ResetThenFree`;
    /// `keeps_process && !runs_reset_job` ⇒ `KeepForInspection`. The
    /// pair is `(false, false) | (true, true) | (true, false)` —
    /// pinned injective by
    /// `return_policy_predicate_pair_is_injective`.
    pub const fn keeps_process(self) -> bool {
        match self {
            Self::Replace => false,
            Self::Reset | Self::Keep => true,
        }
    }

    /// Does the policy run a typed `:reset` Job to wipe state in
    /// place? See `keeps_process` for the closed-match rationale +
    /// the predicate-pair contract.
    pub const fn runs_reset_job(self) -> bool {
        match self {
            Self::Reset => true,
            Self::Replace | Self::Keep => false,
        }
    }
}

// `impl FromStr for ReturnPolicy` + `impl tatara_lisp::ClosedSet for
// ReturnPolicy` + `impl fmt::Display for ReturnPolicy` are generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` on the enum declaration
// above. `label` delegates to the inherent `ReturnPolicy::as_str` via
// `#[closed_set(via = "as_str")]` so the
// `tatara-pool-reconciler::return_policy::plan_return` dispatch keeps
// reading the canonical PascalCase projection that matches the CRD
// `enum:` literal verbatim, while a generic `T: ClosedSet` consumer
// plugs in without knowing the enum lives in `tatara-process::pool`;
// Display delegates to the same inherent projection via
// `#[closed_set(display)]` so the `policy={policy}` diagnostic
// composition stays pinned on the closed-set algebra.

// `pub struct UnknownReturnPolicy(pub String)` is generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"return policy"`
// matches the prior hand-rolled `#[error("unknown return policy: {0}")]`
// verbatim. Symmetric to [`UnknownReplacementPolicy`],
// [`UnknownMemberState`], [`UnknownPoolPhase`],
// [`crate::lifetime::UnknownTeardownPolicy`],
// [`crate::boundary::UnknownConditionKind`], and
// [`crate::phase::UnknownPhase`].

/// Routing selector — matches an `EphemeralAllocation`'s requestor
/// against pool-eligibility predicates.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PoolSelector {
    /// Glob-matched against `EphemeralAllocation.spec.requestor.repo`.
    /// Empty = match every repo.
    #[serde(default)]
    pub repos: Vec<String>,

    /// Glob-matched against `EphemeralAllocation.spec.requestor.branch`.
    /// Empty = match every branch.
    #[serde(default)]
    pub branches: Vec<String>,

    /// PR labels (all-must-match, AND semantics). Empty = no label
    /// requirement.
    #[serde(default)]
    pub pr_labels: Vec<String>,

    /// Allocation `kind` strings this pool can serve (e.g., "github-pr",
    /// "manual", "ci-run"). Empty = any kind.
    #[serde(default)]
    pub kinds: Vec<String>,
}

impl PoolSelector {
    /// Does this selector match the given allocation routing key?
    /// Pure: no side effects.
    pub fn matches(&self, key: &MatchKey<'_>) -> bool {
        glob_any(&self.repos, key.repo)
            && glob_any(&self.branches, key.branch)
            && labels_subset(&self.pr_labels, key.pr_labels)
            && kind_any(&self.kinds, key.kind)
    }

    /// Specificity score — higher = more specific. Used by the
    /// reconciler to break ties between selectors that all match.
    pub fn specificity(&self) -> u32 {
        let mut score = 0;
        if !self.repos.is_empty() {
            score += 8;
        }
        if !self.branches.is_empty() {
            score += 4;
        }
        score += (self.pr_labels.len() as u32) * 2;
        if !self.kinds.is_empty() {
            score += 1;
        }
        score
    }
}

/// Allocation routing key — what the reconciler matches against pool selectors.
#[derive(Clone, Copy, Debug)]
pub struct MatchKey<'a> {
    pub repo: &'a str,
    pub branch: &'a str,
    pub pr_labels: &'a [String],
    pub kind: &'a str,
}

fn glob_any(patterns: &[String], value: &str) -> bool {
    if patterns.is_empty() {
        return true;
    }
    patterns.iter().any(|p| glob_match(p, value))
}

fn kind_any(kinds: &[String], value: &str) -> bool {
    if kinds.is_empty() {
        return true;
    }
    kinds.iter().any(|k| k == value)
}

fn labels_subset(required: &[String], present: &[String]) -> bool {
    required.iter().all(|r| present.iter().any(|p| p == r))
}

/// Minimal glob: supports trailing `*` only (e.g., `"pleme-io/*"`,
/// `"release-*"`). Sufficient for repo/branch routing. Empty pattern
/// matches anything.
fn glob_match(pattern: &str, value: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        value.starts_with(prefix)
    } else {
        pattern == value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The closed-set tests below call `T::from_str(bad)` via the
    // derive-generated `FromStr` impls — bring the trait into scope at
    // the test module so the lib body doesn't carry an otherwise-unused
    // `use std::str::FromStr;` at the file head.
    use std::str::FromStr;

    #[test]
    fn glob_trailing_star_matches_prefix() {
        assert!(glob_match("pleme-io/*", "pleme-io/demo-app"));
        assert!(!glob_match("pleme-io/*", "drzln/dotfiles"));
        assert!(glob_match("release-*", "release-2026-05"));
        assert!(!glob_match("release-*", "main"));
        assert!(glob_match("main", "main"));
        assert!(!glob_match("main", "develop"));
    }

    #[test]
    fn empty_selector_matches_anything() {
        let s = PoolSelector::default();
        assert!(s.matches(&MatchKey {
            repo: "any/repo",
            branch: "any-branch",
            pr_labels: &[],
            kind: "any",
        }));
    }

    #[test]
    fn repo_glob_filters_match_key() {
        let s = PoolSelector {
            repos: vec!["pleme-io/demo-*".into()],
            ..Default::default()
        };
        assert!(s.matches(&MatchKey {
            repo: "pleme-io/demo-app",
            branch: "x",
            pr_labels: &[],
            kind: "y",
        }));
        assert!(!s.matches(&MatchKey {
            repo: "pleme-io/other-repo",
            branch: "x",
            pr_labels: &[],
            kind: "y",
        }));
    }

    #[test]
    fn pr_labels_require_all() {
        let s = PoolSelector {
            pr_labels: vec!["needs-ephemeral".into(), "integration".into()],
            ..Default::default()
        };
        // Both labels present → match.
        assert!(s.matches(&MatchKey {
            repo: "x",
            branch: "y",
            pr_labels: &[
                "needs-ephemeral".into(),
                "integration".into(),
                "extra".into()
            ],
            kind: "z",
        }));
        // One label missing → no match.
        assert!(!s.matches(&MatchKey {
            repo: "x",
            branch: "y",
            pr_labels: &["needs-ephemeral".into()],
            kind: "z",
        }));
    }

    #[test]
    fn specificity_ranks_more_constrained_higher() {
        let general = PoolSelector::default();
        let specific = PoolSelector {
            repos: vec!["pleme-io/*".into()],
            branches: vec!["main".into()],
            pr_labels: vec!["needs-ephemeral".into()],
            kinds: vec!["github-pr".into()],
        };
        assert!(specific.specificity() > general.specificity());
    }

    #[test]
    fn return_policy_defaults_to_replace() {
        assert_eq!(ReturnPolicy::default(), ReturnPolicy::Replace);
    }

    #[test]
    fn pool_phase_defaults_to_initializing() {
        assert_eq!(PoolPhase::default(), PoolPhase::Initializing);
    }

    // ── closed-set algebra contracts for ReplacementPolicy
    //    (ALL × as_str × FromStr × predicate-pair) ────────────────────

    /// Structural well-formedness of [`ReplacementPolicy`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `replacement_policy_all_is_unique_and_complete` +
    /// `replacement_policy_roundtrip_via_as_str` + the empty-input arm
    /// of `unknown_replacement_policy_errors`. `FromStr` delegates to
    /// `<Self as tatara_closed_set::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the pool reconciler hits when
    /// parsing a CRD `enum:`-validated value back to the typed policy.
    #[test]
    fn replacement_policy_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<ReplacementPolicy>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the
    /// YAML wire format.
    #[test]
    fn replacement_policy_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<ReplacementPolicy>();
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. The operator-facing
    /// "policy={policy}" diagnostic in `tatara-pool-reconciler::desired`
    /// composes through Display rather than through a hard-coded
    /// variant string.
    #[test]
    fn replacement_policy_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<ReplacementPolicy>();
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / cross-axis-leaked — and the
    /// error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    /// The empty-input arm is pinned by
    /// [`replacement_policy_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownReplacementPolicy`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_replacement_policy_errors() {
        for bad in [
            "replaceimmediate",
            "PAUSEPOOL",
            "Replace-Immediate",
            "hold_failed",
            "Pause",
            "Reset",
        ] {
            let err = ReplacementPolicy::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// TRUTH-TABLE CONTRACT: the predicate pair agrees with the
    /// documented per-variant on-failure behavior.
    #[test]
    fn replacement_policy_predicate_truth_tables() {
        assert!(ReplacementPolicy::ReplaceImmediate.replaces_failed());
        assert!(!ReplacementPolicy::ReplaceImmediate.pauses_on_failure());

        assert!(!ReplacementPolicy::HoldFailed.replaces_failed());
        assert!(!ReplacementPolicy::HoldFailed.pauses_on_failure());

        assert!(!ReplacementPolicy::PausePool.replaces_failed());
        assert!(ReplacementPolicy::PausePool.pauses_on_failure());
    }

    /// DISJOINTNESS CONTRACT: no variant returns true from BOTH
    /// predicates simultaneously — the two on-failure actions
    /// (reap-each-failed vs pause-whole-pool) are mutually exclusive.
    /// A future `ReplacementPolicy::PauseAndReap` that returned true
    /// from both would FAIL here, forcing the author to either pick
    /// one bucket or extend the consumer dispatch site in
    /// `tatara-pool-reconciler::desired::PoolConvergence::decide`
    /// deliberately rather than silently double-firing both branches.
    #[test]
    fn replacement_policy_predicates_are_disjoint() {
        for policy in ReplacementPolicy::ALL {
            assert!(
                !(policy.replaces_failed() && policy.pauses_on_failure()),
                "{policy:?} returns true from both replaces_failed and pauses_on_failure",
            );
        }
    }

    /// INJECTIVITY CONTRACT: the pair `(replaces_failed,
    /// pauses_on_failure)` is injective across `ALL`. Each variant
    /// projects to its own `(bool, bool)` bucket: `(true, false)` =
    /// reap; `(false, false)` = hold; `(false, true)` = pause. Pairing
    /// this with the disjointness contract above forces a future
    /// variant to land in a fresh `(replaces_failed,
    /// pauses_on_failure)` bucket — or the author extends the consumer
    /// dispatch in `tatara-pool-reconciler::desired::PoolConvergence`
    /// to recognize the new projection bucket.
    #[test]
    fn replacement_policy_predicate_pair_is_injective() {
        let projections: Vec<(bool, bool)> = ReplacementPolicy::ALL
            .into_iter()
            .map(|p| (p.replaces_failed(), p.pauses_on_failure()))
            .collect();
        let unique: std::collections::HashSet<_> = projections.iter().copied().collect();
        assert_eq!(
            projections.len(),
            unique.len(),
            "predicate pair projection is not injective: {projections:?}",
        );
    }

    /// DEFAULT-AGREEMENT CONTRACT: `ReplacementPolicy::default()`
    /// returns the variant tagged `#[default]` in the enum, AND that
    /// variant reaps (the production-safe behavior). A future #[default]
    /// rename without flipping the predicates fails here.
    #[test]
    fn replacement_policy_default_replaces_failed() {
        let d = ReplacementPolicy::default();
        assert_eq!(d, ReplacementPolicy::ReplaceImmediate);
        assert!(d.replaces_failed());
        assert!(!d.pauses_on_failure());
    }

    #[test]
    fn kinds_filter_to_known_set() {
        let s = PoolSelector {
            kinds: vec!["github-pr".into(), "manual".into()],
            ..Default::default()
        };
        assert!(s.matches(&MatchKey {
            repo: "x",
            branch: "y",
            pr_labels: &[],
            kind: "github-pr",
        }));
        assert!(!s.matches(&MatchKey {
            repo: "x",
            branch: "y",
            pr_labels: &[],
            kind: "scheduled",
        }));
    }

    // ── closed-set algebra contracts for ReturnPolicy
    //    (ALL × as_str × FromStr × predicate-pair) ────────────────────

    /// Structural well-formedness of [`ReturnPolicy`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — testkit lift
    /// symmetric to [`replacement_policy_is_well_formed_closed_set`]
    /// above.
    #[test]
    fn return_policy_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<ReturnPolicy>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the
    /// YAML wire format.
    #[test]
    fn return_policy_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<ReturnPolicy>();
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift, mirroring the
    /// `ReplacementPolicy` discipline.
    #[test]
    fn return_policy_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<ReturnPolicy>();
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / cross-axis-leaked — and the
    /// error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    /// The empty-input arm is pinned by
    /// [`return_policy_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit.
    #[test]
    fn unknown_return_policy_errors() {
        for bad in [
            "replace",
            "RESET",
            "Re-place",
            "keep_for_inspection",
            "DeleteAndRespawn",
            "ReplaceImmediate",
        ] {
            let err = ReturnPolicy::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// TRUTH-TABLE CONTRACT: the predicate pair agrees with the
    /// documented per-variant on-release behavior.
    #[test]
    fn return_policy_predicate_truth_tables() {
        assert!(!ReturnPolicy::Replace.keeps_process());
        assert!(!ReturnPolicy::Replace.runs_reset_job());

        assert!(ReturnPolicy::Reset.keeps_process());
        assert!(ReturnPolicy::Reset.runs_reset_job());

        assert!(ReturnPolicy::Keep.keeps_process());
        assert!(!ReturnPolicy::Keep.runs_reset_job());
    }

    /// IMPLICATION CONTRACT: `runs_reset_job` implies `keeps_process`.
    /// You cannot run a typed `:reset` Job against a Process you've
    /// just deleted; the impossible bucket `(false, true)` must stay
    /// empty. A future variant returning true from `runs_reset_job`
    /// while returning false from `keeps_process` fails here, which
    /// forces the author to either flip `keeps_process` to true or
    /// extend the consumer dispatch site in
    /// `tatara-pool-reconciler::return_policy::plan_return`
    /// deliberately rather than letting an impossible state slip in.
    #[test]
    fn return_policy_reset_implies_keeps_process() {
        for policy in ReturnPolicy::ALL {
            if policy.runs_reset_job() {
                assert!(
                    policy.keeps_process(),
                    "{policy:?} runs a reset job but does not keep the process",
                );
            }
        }
    }

    /// INJECTIVITY CONTRACT: the pair `(keeps_process, runs_reset_job)`
    /// is injective across `ALL`. Each variant projects to its own
    /// `(bool, bool)` bucket: `(false, false)` = delete + respawn;
    /// `(true, true)` = reset-in-place; `(true, false)` = keep for
    /// inspection. Pairing this with the implication contract above
    /// forces a future variant to land in a fresh
    /// `(keeps_process, runs_reset_job)` bucket — or the author
    /// extends the consumer dispatch in
    /// `tatara-pool-reconciler::return_policy::plan_return` to
    /// recognize the new projection bucket.
    #[test]
    fn return_policy_predicate_pair_is_injective() {
        let projections: Vec<(bool, bool)> = ReturnPolicy::ALL
            .into_iter()
            .map(|p| (p.keeps_process(), p.runs_reset_job()))
            .collect();
        let unique: std::collections::HashSet<_> = projections.iter().copied().collect();
        assert_eq!(
            projections.len(),
            unique.len(),
            "predicate pair projection is not injective: {projections:?}",
        );
    }

    /// DEFAULT-AGREEMENT CONTRACT: `ReturnPolicy::default()` returns
    /// the variant tagged `#[default]` in the enum, AND that variant
    /// is the safe "tear down + respawn" behavior — neither keeps the
    /// process nor runs a reset Job. A future `#[default]` rename
    /// without flipping the predicates fails here.
    #[test]
    fn return_policy_default_is_replace_and_neither_predicate_fires() {
        let d = ReturnPolicy::default();
        assert_eq!(d, ReturnPolicy::Replace);
        assert!(!d.keeps_process());
        assert!(!d.runs_reset_job());
    }

    // ── closed-set algebra contracts for MemberState
    //    (ALL × as_str × FromStr × predicate pair) ────────────────────

    /// Structural well-formedness of [`MemberState`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — testkit lift
    /// symmetric to [`replacement_policy_is_well_formed_closed_set`]
    /// and [`return_policy_is_well_formed_closed_set`] above.
    #[test]
    fn member_state_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<MemberState>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the YAML
    /// wire format the pool reconciler stamps on
    /// `status.members[].state`.
    #[test]
    fn member_state_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<MemberState>();
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. Any operator-facing
    /// "state={state}" diagnostic that composes through Display
    /// inherits the canonical wire-format string automatically.
    #[test]
    fn member_state_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<MemberState>();
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / cross-axis-leaked — and
    /// the error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    /// The empty-input arm is pinned by
    /// [`member_state_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit. The cross-axis leak cases
    /// pin the closed-set REJECTION contract that the trait can't see:
    /// `"ReplaceImmediate"`, `"Reset"`, and `"Attested"` are valid
    /// labels for sibling enums (`ReplacementPolicy`, `ReturnPolicy`,
    /// `ProcessPhase`) but MUST reject here, because the codomains
    /// are disjoint.
    #[test]
    fn unknown_member_state_errors() {
        for bad in [
            "free",
            "SPAWNING",
            "Free-State",
            "allocated_now",
            "ReplaceImmediate", // ReplacementPolicy-axis leak
            "Reset",            // ReturnPolicy-axis leak
            "Attested",         // ProcessPhase-axis leak
        ] {
            let err = MemberState::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// TRUTH-TABLE CONTRACT: the predicate pair agrees with the
    /// documented per-variant lifecycle role. The pool reconciler's
    /// `pool_phase_from_members` supply calc collapses
    /// `count_state(Free) + count_state(Spawning)` into one
    /// `counts_toward_supply` filter; this table pins the per-variant
    /// projection that consumer depends on.
    #[test]
    fn member_state_predicate_truth_tables() {
        assert!(!MemberState::Spawning.is_failed());
        assert!(MemberState::Spawning.counts_toward_supply());

        assert!(!MemberState::Free.is_failed());
        assert!(MemberState::Free.counts_toward_supply());

        assert!(!MemberState::Allocated.is_failed());
        assert!(!MemberState::Allocated.counts_toward_supply());

        assert!(!MemberState::Returning.is_failed());
        assert!(!MemberState::Returning.counts_toward_supply());

        assert!(MemberState::Failed.is_failed());
        assert!(!MemberState::Failed.counts_toward_supply());
    }

    /// DISJOINTNESS CONTRACT: no variant returns true from BOTH
    /// `is_failed` and `counts_toward_supply` simultaneously — a
    /// failed member can never be counted as available capacity. A
    /// future variant that returned true from both would FAIL here,
    /// forcing the author to either drop it from supply, or extend
    /// the consumer's bucketing in
    /// `tatara-pool-reconciler::controller_pool::pool_phase_from_members`
    /// deliberately rather than silently inflating the pool's supply
    /// count with failed slots.
    #[test]
    fn member_state_failed_implies_no_supply() {
        for state in MemberState::ALL {
            assert!(
                !(state.is_failed() && state.counts_toward_supply()),
                "{state:?} returns true from both is_failed and counts_toward_supply — \
                 a failed member can never be counted as available pool capacity",
            );
        }
    }

    /// COVERAGE CONTRACT: every variant lands somewhere — either
    /// in supply, or as a failed slot, or as an in-use bucket
    /// (`Allocated | Returning`). A future variant that returns
    /// `false` from `counts_toward_supply` AND `false` from
    /// `is_failed` is fine *iff* it represents an in-use slot; this
    /// test pins the existing variants in their declared buckets so
    /// the consumer-side dispatch in
    /// `tatara-pool-reconciler::pool_decide::decide_pool_reconcile`
    /// stays grounded.
    #[test]
    fn member_state_buckets_cover_every_variant() {
        let mut supply = 0u32;
        let mut failed = 0u32;
        let mut in_use = 0u32;
        for state in MemberState::ALL {
            match (state.is_failed(), state.counts_toward_supply()) {
                (true, false) => failed += 1,
                (false, true) => supply += 1,
                (false, false) => in_use += 1,
                (true, true) => panic!("disjointness already pins this empty for {state:?}"),
            }
        }
        assert_eq!(supply, 2, "supply bucket: Free + Spawning");
        assert_eq!(failed, 1, "failed bucket: Failed");
        assert_eq!(in_use, 2, "in-use bucket: Allocated + Returning");
        assert_eq!(supply + failed + in_use, MemberState::ALL.len() as u32);
    }

    // ── closed-set algebra contracts for PoolPhase
    //    (ALL × as_str × FromStr × predicate pair) ────────────────────

    /// Structural well-formedness of [`PoolPhase`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — testkit lift
    /// symmetric to [`member_state_is_well_formed_closed_set`] above.
    #[test]
    fn pool_phase_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<PoolPhase>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the YAML
    /// wire format the pool reconciler stamps on `status.phase`.
    #[test]
    fn pool_phase_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<PoolPhase>();
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. Any operator-facing
    /// "phase={phase}" diagnostic that composes through Display
    /// inherits the canonical wire-format string automatically.
    #[test]
    fn pool_phase_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<PoolPhase>();
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / cross-axis-leaked — and
    /// the error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    /// The empty-input arm is pinned by
    /// [`pool_phase_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit. The cross-axis leak cases
    /// (`"Free"`, `"Replace"`, `"Attested"`, `"HoldFailed"`) pin the
    /// closed-set REJECTION contract that the trait can't see — those
    /// are valid sibling-axis labels but MUST reject here.
    #[test]
    fn unknown_pool_phase_errors() {
        for bad in [
            "steady",
            "SCALINGUP",
            "Scaling-Up",
            "scaling_down",
            "Free",       // MemberState-axis leak
            "Replace",    // ReturnPolicy-axis leak
            "Attested",   // ProcessPhase-axis leak
            "HoldFailed", // ReplacementPolicy-axis leak
        ] {
            let err = PoolPhase::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// TRUTH-TABLE CONTRACT: the predicate pair agrees with the
    /// documented per-variant lifecycle role. Pinning this table at
    /// one site means any future status-aggregator surface
    /// (`feira pool list --healthy`, the SSE filter, the desired-loop
    /// heartbeat short-circuit) reads the same projection that the
    /// reconciler writes.
    #[test]
    fn pool_phase_predicate_truth_tables() {
        assert!(!PoolPhase::Initializing.is_steady());
        assert!(!PoolPhase::Initializing.is_terminal());

        assert!(PoolPhase::Steady.is_steady());
        assert!(!PoolPhase::Steady.is_terminal());

        assert!(!PoolPhase::ScalingUp.is_steady());
        assert!(!PoolPhase::ScalingUp.is_terminal());

        assert!(!PoolPhase::ScalingDown.is_steady());
        assert!(!PoolPhase::ScalingDown.is_terminal());

        assert!(!PoolPhase::Degraded.is_steady());
        assert!(!PoolPhase::Degraded.is_terminal());

        assert!(!PoolPhase::Draining.is_steady());
        assert!(PoolPhase::Draining.is_terminal());
    }

    /// DISJOINTNESS CONTRACT: no variant returns true from BOTH
    /// `is_steady` and `is_terminal` simultaneously — a draining pool
    /// is by definition transitioning OUT, not the goal converged
    /// state. A future variant that returned true from both would
    /// FAIL here, forcing the author to either pick one bucket or
    /// extend the consumer dispatch sites (status aggregators,
    /// heartbeat short-circuit) deliberately rather than silently
    /// double-firing both branches.
    #[test]
    fn pool_phase_steady_excludes_terminal() {
        for phase in PoolPhase::ALL {
            assert!(
                !(phase.is_steady() && phase.is_terminal()),
                "{phase:?} returns true from both is_steady and is_terminal — \
                 a draining pool is by definition not the converged goal state",
            );
        }
    }

    /// COVERAGE CONTRACT: every variant lands somewhere — either the
    /// converged goal (`Steady`), the absorbing exit (`Draining`),
    /// or the transient bucket (`Initializing | ScalingUp |
    /// ScalingDown | Degraded` — pool is in motion toward desired).
    /// A future variant that returns `false` from BOTH predicates is
    /// fine *iff* it represents an in-motion state; this test pins
    /// the existing variants in their declared buckets so the
    /// projection consumers stay grounded.
    #[test]
    fn pool_phase_buckets_cover_every_variant() {
        let mut converged = 0u32;
        let mut terminal = 0u32;
        let mut transient = 0u32;
        for phase in PoolPhase::ALL {
            match (phase.is_steady(), phase.is_terminal()) {
                (true, false) => converged += 1,
                (false, true) => terminal += 1,
                (false, false) => transient += 1,
                (true, true) => panic!("disjointness already pins this empty for {phase:?}"),
            }
        }
        assert_eq!(converged, 1, "converged bucket: Steady");
        assert_eq!(terminal, 1, "terminal bucket: Draining");
        assert_eq!(
            transient, 4,
            "transient bucket: Initializing + ScalingUp + ScalingDown + Degraded"
        );
        assert_eq!(
            converged + terminal + transient,
            PoolPhase::ALL.len() as u32
        );
    }

    /// DEFAULT-AGREEMENT CONTRACT: `PoolPhase::default()` returns the
    /// variant a freshly-admitted pool should land in — `Initializing`
    /// — AND that variant is neither steady (no members yet) nor
    /// terminal (not deletion-stamped). A future `Default` rename
    /// without flipping the predicates fails here.
    #[test]
    fn pool_phase_default_is_initializing_in_transient_bucket() {
        let d = PoolPhase::default();
        assert_eq!(d, PoolPhase::Initializing);
        assert!(!d.is_steady());
        assert!(!d.is_terminal());
    }

    // ─────────────────────────────────────────────────────────────────
    // `EphemeralPool::name_or_empty` — borrow-form metadata-projection
    // primitive on the `metadata.name` axis. Pins the missing-slot
    // corner, the populated-slot corner, the pre-lift chain-shape
    // parity, and the pure-projection discipline that the two
    // `tatara-pool-reconciler` consumers routed onto the primitive
    // depend on. See the primitive's doc-comment for the full
    // migration rationale.
    // ─────────────────────────────────────────────────────────────────

    fn empty_template() -> EphemeralSpec {
        EphemeralSpec {
            aplicacao: crate::intent::AplicacaoIntent::chart_only("oci://x", "1"),
            ttl: "1h".into(),
            teardown: crate::lifetime::TeardownPolicy::Always,
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

    fn pool_spec() -> PoolSpec {
        // Every non-template slot rides the ONE substrate composer
        // [`PoolSpec::with_template`] at its wire-published default;
        // pre-lift this fixture spelled the full 11-slot struct-literal
        // verbatim as one of eight cross-crate hand-authored copies past
        // the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold. See the
        // primitive's doc-comment for the full migration rationale.
        PoolSpec {
            desired_size: 1,
            ..PoolSpec::with_template(empty_template())
        }
    }

    fn pool_named(name: &str) -> EphemeralPool {
        EphemeralPool::new(name, pool_spec())
    }

    fn pool_unnamed() -> EphemeralPool {
        let mut p = EphemeralPool::new("scratch", pool_spec());
        p.metadata.name = None;
        p
    }

    #[test]
    fn name_or_empty_returns_empty_string_when_metadata_name_is_none() {
        let p = pool_unnamed();
        assert!(p.metadata.name.is_none(), "fixture invariant");
        assert_eq!(p.name_or_empty(), "");
    }

    #[test]
    fn name_or_empty_returns_populated_slot_verbatim() {
        let p = pool_named("attest-pool");
        assert_eq!(p.name_or_empty(), "attest-pool");
    }

    #[test]
    fn name_or_empty_returns_empty_string_when_slot_is_explicitly_empty_string() {
        // Corner between `None` (missing slot) and `Some(String::new())`
        // (populated slot containing the empty string): the primitive
        // MUST fold both to the same `""` byte-shape so a downstream
        // `HashMap<String,_>::get(name)` / `str::cmp` sees ONE
        // "unnamed pool" bucket regardless of which shape the K8s API
        // server materialized. This is byte-identical to what the
        // pre-lift `.as_deref().unwrap_or("")` chain produced.
        let mut p = pool_named("scratch");
        p.metadata.name = Some(String::new());
        assert_eq!(p.name_or_empty(), "");
    }

    #[test]
    fn name_or_empty_is_a_pure_projection() {
        // Consecutive calls return byte-identical slices — no cached
        // state, no mutation on the `EphemeralPool` between calls.
        // Guards against a future refactor that plants a cache field
        // and drifts one caller from another silently.
        let p = pool_named("router-pool");
        assert_eq!(p.name_or_empty(), p.name_or_empty());
        assert_eq!(p.name_or_empty(), "router-pool");
        assert_eq!(p.name_or_empty(), "router-pool");
    }

    #[test]
    fn name_or_empty_matches_pre_lift_chain_verbatim() {
        // Byte-identical parity with the two hand-authored
        // `.metadata.name.as_deref().unwrap_or("")` chains the
        // primitive replaces in `tatara-pool-reconciler::router` and
        // `tatara-pool-reconciler::controller_allocation`. Runs across
        // the FULL corner set of the metadata.name slot: absent,
        // present-with-value, present-with-empty-string.
        let cases: [(Option<String>, &str); 3] = [
            (None, ""),
            (Some("attest-pool".into()), "attest-pool"),
            (Some(String::new()), ""),
        ];
        for (slot, expected) in cases {
            let mut p = pool_named("scratch");
            p.metadata.name = slot.clone();
            let pre_lift = p.metadata.name.as_deref().unwrap_or("");
            assert_eq!(pre_lift, expected, "pre-lift chain sanity");
            assert_eq!(p.name_or_empty(), pre_lift);
            assert_eq!(p.name_or_empty(), expected);
        }
    }

    #[test]
    fn name_or_empty_borrows_from_metadata_name_slot() {
        // The returned `&str` is tied to the `EphemeralPool`'s
        // lifetime — the caller can compare / hash / index without
        // allocating. This is the load-bearing property that lets
        // the `HashMap<String, _>::get(pool.name_or_empty())` closure
        // in `controller_allocation::reconcile_inner` skip cloning.
        let p = pool_named("attest-pool");
        let s: &str = p.name_or_empty();
        assert_eq!(s.as_ptr(), p.metadata.name.as_deref().unwrap().as_ptr());
    }

    // ─── EphemeralPool::owned_name_or_empty substrate pins ────────────
    //
    // The owned-form peer of the borrow-form `name_or_empty` primitive
    // above. Sibling to the sister-CRD primitive
    // `crate::crd::Process::owned_name_or_empty` (owned + empty sentinel
    // on `Process::metadata.name`) — the four primitives now partition
    // the (borrow × owned) × (name × uid) corner of the metadata-slot
    // family on identical missing-slot semantics across BOTH tatara-
    // process CRDs (`Process::uid_or_empty` + `Process::owned_name_or_empty`
    // + `EphemeralPool::name_or_empty` + this method). Fail-before-pass-
    // after granularity: `owned_name_or_empty` did not exist on the pool
    // CRD pre-lift; the compiler cannot resolve the name until the impl
    // block above is in place, so a rollback of the primitive breaks
    // this whole module.
    #[test]
    fn owned_name_or_empty_returns_empty_string_when_metadata_name_is_none() {
        let p = pool_unnamed();
        assert!(p.metadata.name.is_none(), "fixture invariant");
        assert_eq!(p.owned_name_or_empty(), String::new());
    }

    #[test]
    fn owned_name_or_empty_returns_owned_string_when_slot_is_populated() {
        let p = pool_named("attest-pool");
        assert_eq!(p.owned_name_or_empty(), "attest-pool");
    }

    #[test]
    fn owned_name_or_empty_returns_empty_string_when_slot_is_explicitly_empty_string() {
        // Corner between `None` (missing slot) and `Some(String::new())`
        // (populated slot containing the empty string): the primitive
        // MUST fold both to the same `""` byte-shape so a downstream
        // `HashMap<String,_>::get(name)` sees ONE "unnamed pool" bucket
        // regardless of which shape the K8s API server materialized.
        // Byte-identical to what the pre-lift `.clone().unwrap_or_default()`
        // chain produced.
        let mut p = pool_named("scratch");
        p.metadata.name = Some(String::new());
        assert_eq!(p.owned_name_or_empty(), String::new());
        assert!(p.owned_name_or_empty().is_empty());
    }

    #[test]
    fn owned_name_or_empty_is_a_pure_projection() {
        // Consecutive calls return byte-identical Strings — no cached
        // state, no mutation on the `EphemeralPool` between calls.
        // Guards against a future refactor that plants a cache field
        // and drifts one caller from another silently.
        let p = pool_named("router-pool");
        assert_eq!(p.owned_name_or_empty(), p.owned_name_or_empty());
        assert_eq!(p.owned_name_or_empty(), "router-pool");
        assert_eq!(p.owned_name_or_empty(), "router-pool");
    }

    #[test]
    fn owned_name_or_empty_matches_pre_lift_chain_verbatim() {
        // Byte-identical parity with the two hand-authored
        // `.metadata.name.clone().unwrap_or_default()` chains the
        // primitive replaces in `tatara-pool-reconciler::
        // controller_allocation::reconcile_inner` (HashMap key seed)
        // and `tatara-pool-reconciler::allocation_decide::
        // AllocationConvergenceCtx::observe` (AllocationRef.name slot
        // seed). Runs across the FULL corner set of the metadata.name
        // slot: absent, present-with-value, present-with-empty-string.
        // A regression that inserted a normalization step at the
        // primitive the pre-lift chain does NOT apply — or vice versa —
        // surfaces here rather than as silent drift between the two
        // owned-form callsites and the ONE substrate owner they now
        // route through.
        let cases: [(Option<String>, &str); 3] = [
            (None, ""),
            (Some("attest-pool".into()), "attest-pool"),
            (Some(String::new()), ""),
        ];
        for (slot, expected) in cases {
            let mut p = pool_named("scratch");
            p.metadata.name = slot.clone();
            let pre_lift = p.metadata.name.clone().unwrap_or_default();
            assert_eq!(pre_lift.as_str(), expected, "pre-lift chain sanity");
            assert_eq!(p.owned_name_or_empty(), pre_lift);
            assert_eq!(p.owned_name_or_empty().as_str(), expected);
        }
    }

    #[test]
    fn owned_name_or_empty_matches_borrow_form_peer_on_populated_slot() {
        // Cross-primitive coherence pin at the sibling corner: when the
        // slot is present, the borrow-form (`name_or_empty`) and owned-
        // form (`owned_name_or_empty`) primitives return the SAME byte
        // sequence and differ only in ownership. A regression that
        // skewed one form's fallback would surface here rather than as
        // silent drift between the router tie-break comparator and the
        // AllocationRef seed on the SAME pool.
        let p = pool_named("attest-pool");
        assert_eq!(p.name_or_empty(), p.owned_name_or_empty().as_str());
    }

    #[test]
    fn owned_name_or_empty_matches_borrow_form_peer_on_missing_slot() {
        // Sibling corner of the coherence pin above: when the slot is
        // absent (or explicitly empty), BOTH primitives fold to the
        // same empty-string byte-shape. The load-bearing property is
        // that a caller who switches between the two return-forms
        // based on downstream ownership requirements never sees a
        // different missing-slot spelling as a side effect.
        let p = pool_unnamed();
        assert_eq!(p.name_or_empty(), p.owned_name_or_empty().as_str());
        assert_eq!(p.name_or_empty(), "");
        assert_eq!(p.owned_name_or_empty(), String::new());
    }

    // ─── EphemeralPool::is_being_deleted substrate pins ───────────────
    //
    // Pins the copy-form metadata-projection primitive on the deletion-
    // tombstone axis of the pool CRD. Peer to the borrow-form + owned-
    // form metadata-fallback family (`name_or_empty`,
    // `owned_name_or_empty`); this one opens the presence-probe corner
    // for the tombstone slot. Sibling to the sister-CRD primitive
    // `crate::crd::Process::is_being_deleted` — the two primitives
    // now partition the tombstone-presence probe across BOTH tatara-
    // process CRDs on identical missing-slot semantics. Fail-before-
    // pass-after granularity: `is_being_deleted` did not exist on the
    // pool CRD pre-lift; the compiler cannot resolve the name until
    // the impl block above is in place, so a rollback of the primitive
    // breaks this whole module.

    fn tombstoned_pool() -> EphemeralPool {
        let mut p = pool_named("attest-pool");
        p.metadata.namespace = Some("ephemeral-pools".into());
        // Routes through the ONE substrate composer
        // `tatara_process::time::tombstone_now` — see the peer
        // `tombstoned_process` doc-comment in `crd.rs` for the full
        // migration rationale.
        p.metadata.deletion_timestamp = crate::time::tombstone_now();
        p
    }

    #[test]
    fn is_being_deleted_returns_false_when_deletion_timestamp_is_absent() {
        // Missing-tombstone corner pin: the primitive collapses the
        // no-tombstone case to `false` so the `→ Drain` short-circuit
        // at `decide_pool_reconcile` is NOT taken and the observed-
        // phase composer at `pool_phase_from_members` proceeds to its
        // normal (free / spawning / allocated) arithmetic branches
        // instead of short-circuiting to `PoolPhase::Draining`.
        // Matches the pre-lift `.is_some()` chain's `false` byte-
        // identically at every consumer's downstream gate.
        let mut p = pool_named("attest-pool");
        p.metadata.deletion_timestamp = None;
        assert!(!p.is_being_deleted());
    }

    #[test]
    fn is_being_deleted_returns_true_when_deletion_timestamp_is_present() {
        // Present-tombstone corner pin: the primitive returns `true`
        // on any populated `metadata.deletionTimestamp` slot regardless
        // of the timestamp payload — the two consumers only read the
        // tombstone's PRESENCE, never its RFC-3339 timestamp value.
        // A regression that gated the `true` return on the timestamp
        // being non-epoch, or parsed the timestamp before returning,
        // would surface here rather than as silent skew at the
        // `→ Drain` decision or the `→ Draining` phase report on the
        // SAME `EphemeralPool`.
        let p = tombstoned_pool();
        assert!(p.is_being_deleted());
    }

    #[test]
    fn is_being_deleted_is_a_pure_projection() {
        // Purity pin: two consecutive calls return byte-identical
        // `bool` values (no lazy materialization, no interior
        // mutation of `self`). Peer to the sibling
        // `name_or_empty_is_a_pure_projection` +
        // `owned_name_or_empty_is_a_pure_projection` pins in this
        // module and to `is_being_deleted_is_a_pure_projection` on
        // the sister-CRD `Process`; all four bind the pure-projection
        // discipline on the ONE substrate accessor per metadata slot.
        let p = tombstoned_pool();
        let a = p.is_being_deleted();
        let b = p.is_being_deleted();
        assert_eq!(a, b);
        assert!(a);
    }

    #[test]
    fn is_being_deleted_matches_pre_lift_pool_reconciler_chain_shape() {
        // Parity pin: sweeps the two corners every pre-lift consumer
        // plausibly encountered (missing tombstone, present tombstone)
        // and compares the substrate call against a hand-authored pre-
        // lift chain byte-identically. A regression that reshaped
        // either corner would surface here rather than as silent
        // operator-facing skew between the pool-reconciler's `→ Drain`
        // decision and the observed-phase composer's `→ Draining`
        // report on the SAME `EphemeralPool` within one reconcile
        // pass.
        fn pre_lift(p: &EphemeralPool) -> bool {
            p.metadata.deletion_timestamp.is_some()
        }
        // Missing slot.
        let mut p = pool_named("attest-pool");
        p.metadata.deletion_timestamp = None;
        assert_eq!(p.is_being_deleted(), pre_lift(&p));
        // Populated slot.
        let p = tombstoned_pool();
        assert_eq!(p.is_being_deleted(), pre_lift(&p));
    }

    #[test]
    fn is_being_deleted_composes_with_pool_phase_draining_at_reconcile_preempt() {
        // Call-site-shape pin: the `pool_phase_from_members`
        // deletion-preempt returns `PoolPhase::Draining` as soon as
        // `pool.is_being_deleted()` holds, regardless of the (free +
        // spawning) supply arithmetic that would otherwise pick
        // `Ready` / `Scaling` / `Degraded`. The `→ Drain` decision at
        // `decide_pool_reconcile` composes with the same probe on the
        // same tombstone-presence slot. A regression that broadened
        // the tombstone probe implicitly (returning `false` on a
        // present but zero-timestamp) or narrowed it (requiring an
        // additional `.finalizers.is_empty()` conjunct that the two
        // consumers never spelled) would surface here rather than as
        // silent operator-facing skew between the pool reconciler's
        // decision and the observed-phase composer on the SAME
        // `EphemeralPool` within one reconcile pass.
        let alive = pool_named("attest-pool");
        assert!(!alive.is_being_deleted());
        let dying = tombstoned_pool();
        assert!(dying.is_being_deleted());
    }

    // ─── EphemeralPool::owned_namespace_or_empty substrate pins ───────
    //
    // The owned-form peer of the `owned_name_or_empty` primitive on the
    // sibling `metadata.namespace` axis — the paired half of the
    // `AllocationRef { name, namespace }` struct literal both
    // `AllocationConvergenceCtx::observe` and the composition pin
    // consume through the SAME `AllocationRef::new(name, namespace)`
    // constructor. Fail-before-pass-after granularity:
    // `owned_namespace_or_empty` did not exist on the pool CRD pre-
    // lift; the compiler cannot resolve the name until the impl block
    // above is in place, so a rollback of the primitive breaks this
    // whole module.
    #[test]
    fn owned_namespace_or_empty_returns_empty_string_when_metadata_namespace_is_none() {
        // Missing-slot corner pin: the primitive collapses the no-
        // namespace case to the load-bearing empty-string sentinel so
        // the downstream `AllocationRef.namespace` slot carries `""`
        // rather than a defaulted `"default"` string. See the doc-
        // comment's DELIBERATE-EMPTY-SENTINEL rationale for why the
        // fallback matches `.clone().unwrap_or_default()` byte-for-
        // byte rather than substituting `Process::DEFAULT_NAMESPACE`
        // at the primitive.
        let mut p = pool_named("attest-pool");
        p.metadata.namespace = None;
        assert!(p.metadata.namespace.is_none(), "fixture invariant");
        assert_eq!(p.owned_namespace_or_empty(), String::new());
    }

    #[test]
    fn owned_namespace_or_empty_returns_owned_string_when_slot_is_populated() {
        let mut p = pool_named("attest-pool");
        p.metadata.namespace = Some("ephemeral-pools".into());
        assert_eq!(p.owned_namespace_or_empty(), "ephemeral-pools");
    }

    #[test]
    fn owned_namespace_or_empty_returns_empty_string_when_slot_is_explicitly_empty_string() {
        // Corner between `None` (missing slot) and `Some(String::new())`
        // (populated slot containing the empty string): the primitive
        // MUST fold both to the same `""` byte-shape so a downstream
        // `AllocationRef.namespace ==` comparator at
        // `resolve_pool` sees ONE "unset namespace" bucket regardless
        // of which shape the K8s API server materialized. Byte-
        // identical to what the pre-lift `.clone().unwrap_or_default()`
        // chain produced.
        let mut p = pool_named("attest-pool");
        p.metadata.namespace = Some(String::new());
        assert_eq!(p.owned_namespace_or_empty(), String::new());
        assert!(p.owned_namespace_or_empty().is_empty());
    }

    #[test]
    fn owned_namespace_or_empty_is_a_pure_projection() {
        // Consecutive calls return byte-identical Strings — no cached
        // state, no mutation on the `EphemeralPool` between calls.
        // Peer to the sibling `owned_name_or_empty_is_a_pure_projection`
        // pin in this module and to `is_being_deleted_is_a_pure_projection`
        // on the same CRD; all three bind the pure-projection
        // discipline on the ONE substrate accessor per metadata slot.
        let mut p = pool_named("attest-pool");
        p.metadata.namespace = Some("ephemeral-pools".into());
        assert_eq!(p.owned_namespace_or_empty(), p.owned_namespace_or_empty());
        assert_eq!(p.owned_namespace_or_empty(), "ephemeral-pools");
        assert_eq!(p.owned_namespace_or_empty(), "ephemeral-pools");
    }

    #[test]
    fn owned_namespace_or_empty_matches_pre_lift_chain_verbatim() {
        // Byte-identical parity with the two hand-authored
        // `.metadata.namespace.clone().unwrap_or_default()` chains
        // the primitive replaces in `tatara-pool-reconciler::
        // allocation_decide::AllocationConvergenceCtx::observe`
        // (matched-pool `AllocationRef.namespace` seed) and in the
        // sibling composition pin
        // `allocation_ref_new_composes_with_owned_name_or_empty_pool_projection`.
        // Runs across the FULL corner set of the metadata.namespace
        // slot: absent, present-with-value, present-with-empty-string.
        // A regression that inserted a normalization step at the
        // primitive the pre-lift chain does NOT apply — or vice versa —
        // surfaces here rather than as silent drift between the two
        // owned-form callsites and the ONE substrate owner they now
        // route through.
        let cases: [(Option<String>, &str); 3] = [
            (None, ""),
            (Some("ephemeral-pools".into()), "ephemeral-pools"),
            (Some(String::new()), ""),
        ];
        for (slot, expected) in cases {
            let mut p = pool_named("attest-pool");
            p.metadata.namespace = slot.clone();
            let pre_lift = p.metadata.namespace.clone().unwrap_or_default();
            assert_eq!(pre_lift.as_str(), expected, "pre-lift chain sanity");
            assert_eq!(p.owned_namespace_or_empty(), pre_lift);
            assert_eq!(p.owned_namespace_or_empty().as_str(), expected);
        }
    }

    #[test]
    fn owned_namespace_or_empty_composes_with_owned_name_or_empty_on_paired_slot_axis() {
        // Paired-axis coherence pin: the two owned-form primitives on
        // the pool CRD's `metadata.name` + `metadata.namespace` slots
        // share the SAME empty-string sentinel on the missing corner,
        // so a caller that composes both halves into an
        // `AllocationRef` (as `AllocationConvergenceCtx::observe`
        // does) never sees a mixed-fallback pair (one `""`, the
        // other `"default"`) as a side effect of one slot being
        // absent. A regression that skewed either primitive's
        // fallback would surface here rather than as silent operator-
        // facing skew between the paired halves of the SAME
        // `AllocationRef` seed.
        let mut p = pool_named("attest-pool");
        p.metadata.namespace = None;
        p.metadata.name = None;
        assert_eq!(p.owned_name_or_empty(), p.owned_namespace_or_empty());
        assert_eq!(p.owned_name_or_empty(), String::new());
        assert_eq!(p.owned_namespace_or_empty(), String::new());
    }

    #[test]
    fn owned_namespace_or_empty_does_not_default_to_process_default_namespace() {
        // Deliberate-empty-sentinel pin: the primitive's fallback is
        // `""`, NOT `crate::crd::Process::DEFAULT_NAMESPACE`. The
        // sole downstream consumer (`AllocationConvergenceCtx::observe`)
        // feeds the produced value into `AllocationRef.namespace`,
        // which is then matched byte-identically against
        // `spec.pool_ref.namespace` at `resolve_pool`. A silent
        // substitution of `"default"` at this primitive would alias
        // every namespace-absent pool to the `"default"` bucket at
        // the matcher, hiding the missing-slot corner from an
        // operator who explicitly authored an allocation against a
        // namespace-unset pool. Pinned so a future "helpful"
        // canonicalization step lands as a compiler-visible failure
        // here rather than as silent operator-facing skew at the
        // matched-pool seed.
        let mut p = pool_named("attest-pool");
        p.metadata.namespace = None;
        assert_ne!(
            p.owned_namespace_or_empty(),
            crate::crd::Process::DEFAULT_NAMESPACE
        );
        assert_eq!(p.owned_namespace_or_empty(), "");
    }

    // ─── EphemeralPool::owned_uid_or_name_or_empty substrate pins ─────
    //
    // Pins the compound owned-form projection on the paired
    // `(metadata.uid, metadata.name)` axis of the pool CRD — the
    // ONE-liner collapse of the paired `.metadata.uid.clone()
    // .unwrap_or_else(|| name.<into>())` chain every pool-slot-name
    // consumer restated by hand pre-lift at TWO production sites in
    // `tatara-pool-reconciler::controller_pool` (spawn arm +
    // apply_convergence_actions arm), both feeding the SAME
    // `member_process_name(&pool_name, &pool_uid_or_name_fallback,
    // slot)` composer. Fail-before-pass-after granularity:
    // `owned_uid_or_name_or_empty` did not exist on the pool CRD pre-
    // lift; the compiler cannot resolve the name until the impl block
    // above is in place, so a rollback of the primitive breaks this
    // whole module.
    #[test]
    fn owned_uid_or_name_or_empty_returns_uid_when_uid_is_present() {
        // Preferred-slot pin: uid populated → uid wins, regardless of
        // whether the name-fallback slot is populated. Byte-identical
        // to what each pre-lift `.metadata.uid.clone().unwrap_or_else
        // (|| name.<into>())` chain returned in the reachable-state
        // corner where the K8s API server has stamped a uid (the
        // common case at both callsites, which are already gated by
        // `owned_coordinates_required()?`).
        let mut p = pool_named("attest-pool");
        p.metadata.uid = Some("uid-42".into());
        assert_eq!(p.owned_uid_or_name_or_empty(), "uid-42");
    }

    #[test]
    fn owned_uid_or_name_or_empty_falls_back_to_name_when_uid_is_missing() {
        // Fallback-slot pin: uid absent → name wins. Byte-identical
        // to what each pre-lift chain returned in the corner where
        // the K8s API server has NOT yet stamped a uid (pre-admission
        // / unit-test in-memory pool). The pre-lift chain reached
        // the fallback via a locally-bound `name` string derived from
        // the same `.metadata.name` slot the primitive reaches via
        // `owned_name_or_empty()`.
        let mut p = pool_named("attest-pool");
        p.metadata.uid = None;
        assert_eq!(p.owned_uid_or_name_or_empty(), "attest-pool");
    }

    #[test]
    fn owned_uid_or_name_or_empty_sinks_to_empty_when_both_slots_are_missing() {
        // Missing-both corner pin: uid absent AND name absent → the
        // load-bearing empty-string sentinel. Coherent with the
        // sibling primitives `owned_name_or_empty` +
        // `owned_namespace_or_empty` on the SAME empty-sentinel axis.
        // A regression that dropped either fallback surfaces here
        // rather than as a runtime panic on `.unwrap()` at a spawn
        // callsite that assumed both slots were populated.
        let mut p = pool_named("attest-pool");
        p.metadata.uid = None;
        p.metadata.name = None;
        assert_eq!(p.owned_uid_or_name_or_empty(), String::new());
        assert!(p.owned_uid_or_name_or_empty().is_empty());
    }

    #[test]
    fn owned_uid_or_name_or_empty_prefers_uid_when_both_slots_are_present() {
        // Precedence pin: both slots populated → uid wins. The pre-
        // lift `.unwrap_or_else(|| name.<into>())` chain's short-
        // circuit on the `Some(u)` arm skipped the fallback entirely;
        // the primitive matches that byte-for-byte via `.clone()
        // .unwrap_or_else(|| self.owned_name_or_empty())`, so the
        // name-fallback slot is not read when uid is populated.
        let mut p = pool_named("attest-pool");
        p.metadata.uid = Some("uid-preferred".into());
        p.metadata.name = Some("attest-pool".into());
        assert_eq!(p.owned_uid_or_name_or_empty(), "uid-preferred");
        assert_ne!(p.owned_uid_or_name_or_empty(), "attest-pool");
    }

    #[test]
    fn owned_uid_or_name_or_empty_returns_uid_even_when_uid_is_explicitly_empty_string() {
        // Corner between `None` (missing slot) and `Some(String::new())`
        // (populated slot containing the empty string): the primitive
        // MUST return the populated-empty-string uid rather than
        // falling back to the name half — byte-identical to what the
        // pre-lift `.metadata.uid.clone().unwrap_or_else(|| name...)`
        // chain produced, whose `unwrap_or_else` short-circuits on
        // `Some(_)` regardless of the wrapped value. Pinned so a
        // future "helpful" canonicalization that treats
        // `Some(String::new())` as `None` at the primitive lands as
        // a compiler-visible failure here rather than as silent
        // operator-facing skew between the two spawn-slot-slug seeds.
        let mut p = pool_named("attest-pool");
        p.metadata.uid = Some(String::new());
        p.metadata.name = Some("attest-pool".into());
        assert_eq!(p.owned_uid_or_name_or_empty(), String::new());
        assert_ne!(p.owned_uid_or_name_or_empty(), "attest-pool");
    }

    #[test]
    fn owned_uid_or_name_or_empty_is_a_pure_projection() {
        // Consecutive calls return byte-identical Strings across the
        // FULL corner set (uid-present, uid-absent name-fallback,
        // both-absent empty-sentinel) — no cached state, no mutation
        // on the `EphemeralPool` between calls. Peer to the sibling
        // `owned_name_or_empty_is_a_pure_projection` +
        // `owned_namespace_or_empty_is_a_pure_projection` pins in
        // this module; all three bind the pure-projection discipline
        // on the ONE substrate accessor per metadata-derived slot.
        let mut p = pool_named("attest-pool");
        p.metadata.uid = Some("uid-42".into());
        assert_eq!(
            p.owned_uid_or_name_or_empty(),
            p.owned_uid_or_name_or_empty()
        );
        p.metadata.uid = None;
        assert_eq!(
            p.owned_uid_or_name_or_empty(),
            p.owned_uid_or_name_or_empty()
        );
        p.metadata.name = None;
        assert_eq!(
            p.owned_uid_or_name_or_empty(),
            p.owned_uid_or_name_or_empty()
        );
    }

    #[test]
    fn owned_uid_or_name_or_empty_matches_pre_lift_chain_verbatim() {
        // Byte-identical parity with the two hand-authored
        // `.metadata.uid.clone().unwrap_or_else(|| name.<into>())`
        // chains the primitive replaces in
        // `tatara-pool-reconciler::controller_pool` (spawn arm +
        // apply_convergence_actions arm). Runs across the FULL
        // corner set of the paired (metadata.uid, metadata.name)
        // slots. A regression that inserted a normalization step at
        // the primitive the pre-lift chain does NOT apply — or vice
        // versa — surfaces here rather than as silent drift between
        // the two owned-form callsites and the ONE substrate owner
        // they now route through.
        let cases: [(Option<String>, Option<String>, &str); 6] = [
            (Some("uid-42".into()), Some("attest-pool".into()), "uid-42"),
            (Some("uid-42".into()), None, "uid-42"),
            (Some(String::new()), Some("attest-pool".into()), ""),
            (None, Some("attest-pool".into()), "attest-pool"),
            (None, Some(String::new()), ""),
            (None, None, ""),
        ];
        for (uid_slot, name_slot, expected) in cases {
            let mut p = pool_named("attest-pool");
            p.metadata.uid = uid_slot.clone();
            p.metadata.name = name_slot.clone();
            // Reproduce the pre-lift chain shape at the spawn arm
            // (fallback `|| name.clone()` on an extracted-earlier
            // `String` name) — semantically equivalent to
            // `.metadata.name.clone().unwrap_or_default()` at the
            // point of call because `owned_coordinates_required()?`
            // gate guarantees the caller's `name` binding matches
            // the pool's own `metadata.name` slot.
            let pre_lift = p
                .metadata
                .uid
                .clone()
                .unwrap_or_else(|| p.metadata.name.clone().unwrap_or_default());
            assert_eq!(pre_lift.as_str(), expected, "pre-lift chain sanity");
            assert_eq!(p.owned_uid_or_name_or_empty(), pre_lift);
            assert_eq!(p.owned_uid_or_name_or_empty().as_str(), expected);
        }
    }

    #[test]
    fn owned_uid_or_name_or_empty_composes_with_member_process_name_seed_shape() {
        // Composition pin: the produced owned `String` feeds the
        // downstream `member_process_name(&pool_name, &pool_uid_or_
        // name_fallback, slot)` composer at both callsites, so the
        // seed's `String` shape must survive being borrowed as
        // `&str` for the composer without any owned/borrow-form
        // adaptation at the callsite. Binds the primitive's return
        // type + the borrow-form availability that the pre-lift
        // chain also produced (a locally-owned `String` from
        // `.clone().unwrap_or_else(|| name.<into>())`).
        let mut p = pool_named("attest-pool");
        p.metadata.uid = Some("uid-42".into());
        let seed: String = p.owned_uid_or_name_or_empty();
        let _borrowed: &str = &seed;
        assert_eq!(seed, "uid-42");
        p.metadata.uid = None;
        let seed_fallback: String = p.owned_uid_or_name_or_empty();
        let _borrowed_fallback: &str = &seed_fallback;
        assert_eq!(seed_fallback, "attest-pool");
    }

    // ─── AllocationRef::new substrate pins ────────────────────────────
    //
    // Pins the substrate constructor for [`AllocationRef`] — the
    // ONE-liner composer that lifts the paired
    // `AllocationRef { name, namespace }` struct-literal every
    // downstream consumer restated by hand pre-lift at FOUR production
    // sites (2 × controller_allocation.rs assignedProcess seeds, 1 ×
    // allocation_decide.rs pool_ref seed, 1 × allocation_factory.rs
    // pool_ref seed) onto ONE substrate owner on `AllocationRef`.
    // Fail-before-pass-after granularity: `AllocationRef::new` did not
    // exist pre-lift; the compiler cannot resolve the name until the
    // impl block above is in place, so a rollback of the primitive
    // breaks this whole module.

    #[test]
    fn allocation_ref_new_composes_owned_string_pair_verbatim() {
        // Happy-path pin: the constructor materializes an
        // `AllocationRef { name: <name>, namespace: <namespace> }`
        // byte-identical to the pre-lift struct literal every consumer
        // spelled. A regression that dropped either slot (e.g. an
        // erroneous `..Default::default()` on a shape that never had
        // a Default derive) surfaces here rather than as silent slot
        // loss downstream at the assignedProcess / bound_pool /
        // matched_pool / spec.pool_ref sinks.
        let r = AllocationRef::new(String::from("pr-42-demo"), String::from("ephemeral-pools"));
        assert_eq!(r.name, "pr-42-demo");
        assert_eq!(r.namespace, "ephemeral-pools");
    }

    #[test]
    fn allocation_ref_new_matches_pre_lift_struct_literal_verbatim() {
        // Byte-identical parity pin: the substrate constructor and the
        // hand-authored struct literal produce equal `AllocationRef`
        // values on every provenance the FOUR pre-lift sites carried
        // (owned `String` from an owned-form projection; `&str`
        // promoted through `.to_string()`). A regression that inserted
        // a normalization step at the primitive the pre-lift literal
        // does NOT apply — or vice versa — surfaces here rather than
        // as silent drift between the four consumers and the ONE
        // substrate owner they now route through.
        let owned_name = String::from("pr-42-demo");
        let owned_ns = String::from("ephemeral-pools");
        let lifted = AllocationRef::new(owned_name.clone(), owned_ns.clone());
        let pre_lift = AllocationRef {
            name: owned_name,
            namespace: owned_ns,
        };
        assert_eq!(lifted, pre_lift);
    }

    #[test]
    fn allocation_ref_new_accepts_str_provenance_via_into_string() {
        // `Into<String>` provenance-closure pin: the primitive accepts
        // every provenance the pre-lift sites carried. The
        // controller_allocation.rs assignedProcess seeds passed owned
        // `String` values (a moved `member_process_name` +
        // `ns.clone()`); the allocation_factory.rs pool_ref seed
        // passed `&str` (`n.to_string()` / `namespace.to_string()`).
        // Both provenances produce byte-identical output. A future
        // refactor of the constructor signature that demanded owned
        // `String` at author sites (dropping `impl Into<String>`)
        // would force `.to_string()` back at the FOUR call sites — the
        // pin fences that regression at ONE place.
        let from_str = AllocationRef::new("pr-42-demo", "ephemeral-pools");
        let from_string =
            AllocationRef::new(String::from("pr-42-demo"), String::from("ephemeral-pools"));
        assert_eq!(from_str, from_string);
        // Mixed provenance is also load-bearing: the allocation_decide.rs
        // matched_pool seed pairs an owned `String` (from
        // `EphemeralPool::owned_name_or_empty()`) with a hand-authored
        // `.clone().unwrap_or_default()` — also `String`. The
        // controller_allocation.rs paths pair a moved `String` name
        // with a `.clone()`-ed `ns: String`. Verify (owned, borrow)
        // and (borrow, owned) both compose to the same shape as
        // (owned, owned) / (borrow, borrow).
        let mixed_a = AllocationRef::new(String::from("pr-42-demo"), "ephemeral-pools");
        let mixed_b = AllocationRef::new("pr-42-demo", String::from("ephemeral-pools"));
        assert_eq!(from_str, mixed_a);
        assert_eq!(from_str, mixed_b);
    }

    #[test]
    fn allocation_ref_new_positional_axis_order_pinned_name_first_namespace_second() {
        // Axis-order pin: name is the FIRST positional argument;
        // namespace is the SECOND. Reversing the pair at the
        // constructor is the exact regression this pin fences — the
        // FOUR pre-lift sites all spelled `name` before `namespace`
        // (matching the struct definition's field order in
        // `pub struct AllocationRef { pub name, pub namespace }`)
        // and the wire-format serde output `{ "name": "...",
        // "namespace": "..." }` reflects that order. A slot swap at
        // the primitive would surface here rather than as silent
        // `<namespace>/<name>` inversion at every downstream
        // qualified-ref composer that reads `{ref.name}/{ref.namespace}`
        // as an audit-log key.
        let r = AllocationRef::new("alpha-name", "beta-namespace");
        assert_eq!(r.name, "alpha-name");
        assert_eq!(r.namespace, "beta-namespace");
        assert_ne!(r.name, "beta-namespace");
        assert_ne!(r.namespace, "alpha-name");
    }

    #[test]
    fn allocation_ref_new_preserves_empty_string_verbatim() {
        // Empty-string sentinel pin: the constructor is pure — it does
        // NOT canonicalize empty inputs (does NOT default an empty
        // namespace to `"default"`; does NOT reject an empty name).
        // Preserves the pre-lift shape the allocation_decide.rs
        // matched_pool seed relied on: when the pool's metadata.namespace
        // is absent, `.clone().unwrap_or_default()` yields the empty
        // string, and the AllocationRef's namespace slot carries that
        // empty string verbatim to the downstream `bound_pool` sink.
        // A future canonicalization pass (e.g. defaulting to
        // `Process::DEFAULT_NAMESPACE`) MUST land here, not at the
        // primitive body silently, so the pre-lift consumers' empty-
        // sentinel semantics are the visible contract of the new
        // constructor.
        let r = AllocationRef::new("", "");
        assert_eq!(r.name, "");
        assert_eq!(r.namespace, "");
        let mixed = AllocationRef::new("pr-42-demo", "");
        assert_eq!(mixed.name, "pr-42-demo");
        assert_eq!(mixed.namespace, "");
    }

    #[test]
    fn allocation_ref_new_composes_with_owned_name_or_empty_pool_projection() {
        // Composition pin: the constructor composes with the paired
        // substrate primitives [`EphemeralPool::owned_name_or_empty`]
        // + [`EphemeralPool::owned_namespace_or_empty`] at the
        // allocation_decide.rs pool_ref seed — the same primitive
        // family the pool CRD opened for both halves of the
        // `AllocationRef { name, namespace }` struct literal. The
        // composed pair carries an owned `String` name half (from
        // `pool.owned_name_or_empty()`) and an owned `String`
        // namespace half (from `pool.owned_namespace_or_empty()`) —
        // no pre-lift chain remains. A regression that broke the
        // primitive family's `impl Into<String>` acceptance of an
        // owned `String` return type would surface here rather than
        // as silent build failure at the pool-reconciler matched_pool
        // seed.
        let pool = pool_named("attest-pool");
        let r = AllocationRef::new(pool.owned_name_or_empty(), pool.owned_namespace_or_empty());
        assert_eq!(r.name, "attest-pool");
        assert_eq!(r.namespace, pool.owned_namespace_or_empty());
    }

    #[test]
    fn allocation_ref_new_returns_wire_format_serialization_verbatim() {
        // Wire-format pin: the constructor produces an
        // [`AllocationRef`] whose serde `rename_all = "camelCase"`
        // serialization is byte-identical to the pre-lift struct
        // literal's serialization. The `bound_pool` and
        // `assignedProcess` slots on `AllocationStatus` (and the
        // `poolRef` slot on `AllocationSpec`) all round-trip through
        // this shape — the pin fences a regression that added a
        // private field or a `#[serde(skip)]` accidentally.
        let r = AllocationRef::new("pr-42-demo", "ephemeral-pools");
        let yaml = serde_yaml::to_string(&r).expect("AllocationRef serializes to yaml");
        assert!(yaml.contains("name: pr-42-demo"), "{yaml}");
        assert!(yaml.contains("namespace: ephemeral-pools"), "{yaml}");
        let back: AllocationRef =
            serde_yaml::from_str(&yaml).expect("AllocationRef round-trips through yaml");
        assert_eq!(back, r);
    }

    fn member(state: MemberState) -> PoolMember {
        // 4-slot unallocated seed rides through the ONE substrate
        // owner `PoolMember::unallocated` (peer of the four workspace-
        // wide restatements of the SAME `PoolMember { process_name,
        // state, entered_state_at, allocation_ref: None }` fixture
        // literal that pre-lift lived at the production `controller_
        // pool::reconcile_inner` walk + the two `pool_decide::tests::
        // member` / `allocation_decide::tests::member` helpers + the
        // sibling `named_member` helper in this file).
        PoolMember::unallocated("m", state, crate::time::at_epoch_second(0))
    }

    #[test]
    fn state_count_fanout_returns_all_zeros_on_empty_slice() {
        // Zero-length pin: the empty-members corner produces a
        // 4-tuple of zero counters, matching the pre-lift
        // `count_state` fanout's four `.iter().filter(...).count()`
        // calls each returning 0 on an empty iterator.
        assert_eq!(PoolMember::state_count_fanout(&[]), (0, 0, 0, 0));
    }

    #[test]
    fn state_count_fanout_partitions_variants_into_correct_slots() {
        // Positional-axis pin: the returned 4-tuple's slot order
        // matches the four `PoolStatus` counter slots in declaration
        // order — `(ready, allocated, spawning, returning)`. A
        // regression that swapped two slots (e.g., `ready` ↔
        // `spawning`) surfaces here rather than as an operator-facing
        // scale-out oscillation at the pool reconciler.
        let members = vec![
            member(MemberState::Free),
            member(MemberState::Free),
            member(MemberState::Allocated),
            member(MemberState::Spawning),
            member(MemberState::Spawning),
            member(MemberState::Spawning),
            member(MemberState::Returning),
        ];
        assert_eq!(PoolMember::state_count_fanout(&members), (2, 1, 3, 1));
    }

    #[test]
    fn state_count_fanout_excludes_failed_from_every_counter() {
        // Closed-set pin: no `PoolStatus` slot counts `Failed` members
        // (they surface via `PoolPhase::Degraded` instead of a status
        // counter). This test fences a regression that let a `Failed`
        // member drift into one of the four counters and inflate the
        // operator-visible ready/allocated/spawning/returning fanout.
        let members = vec![
            member(MemberState::Failed),
            member(MemberState::Failed),
            member(MemberState::Failed),
        ];
        assert_eq!(PoolMember::state_count_fanout(&members), (0, 0, 0, 0));

        // Mixed with a Free member: the Free member is counted, the
        // Failed members are not.
        let mixed = vec![
            member(MemberState::Free),
            member(MemberState::Failed),
            member(MemberState::Failed),
        ];
        assert_eq!(PoolMember::state_count_fanout(&mixed), (1, 0, 0, 0));
    }

    #[test]
    fn state_count_fanout_matches_pre_lift_count_state_helper_verbatim() {
        // Parity pin: for every possible members list, the 4-tuple
        // returned by the substrate primitive matches the pre-lift
        // `count_state(&members, MemberState::<slot>)` fanout that
        // pool-reconciler restated at both status-patch sites. The
        // pre-lift helper was
        // ```rust,ignore
        // fn count_state(members: &[PoolMember], target: MemberState) -> u32 {
        //     members.iter().filter(|m| m.state == target).count() as u32
        // }
        // ```
        // — re-implemented inline here as an oracle.
        fn count_state(members: &[PoolMember], target: MemberState) -> u32 {
            members.iter().filter(|m| m.state == target).count() as u32
        }
        let members = vec![
            member(MemberState::Free),
            member(MemberState::Allocated),
            member(MemberState::Allocated),
            member(MemberState::Spawning),
            member(MemberState::Returning),
            member(MemberState::Returning),
            member(MemberState::Failed),
        ];
        let (ready, allocated, spawning, returning) = PoolMember::state_count_fanout(&members);
        assert_eq!(ready, count_state(&members, MemberState::Free));
        assert_eq!(allocated, count_state(&members, MemberState::Allocated));
        assert_eq!(spawning, count_state(&members, MemberState::Spawning));
        assert_eq!(returning, count_state(&members, MemberState::Returning));
    }

    // ─── PoolMember::process_names_set substrate pins ─────────────────
    //
    // Pins the closed-set slice-owned collection primitive on the
    // `process_name` axis into a `HashSet<String>` — the O(1)-lookup
    // shape both spawn arms in
    // `tatara-pool-reconciler::controller_pool` build pre-collision-
    // check against a candidate `member_process_name(&pool_name,
    // &pool_uid, slot)`. Sibling to `state_count_fanout` on the
    // `(collection shape × slice-owned fold)` axis; the fanout owns
    // the state-counter tuple corner, this primitive owns the
    // process-name-lookup corner. Fail-before-pass-after granularity:
    // `process_names_set` did not exist pre-lift; the compiler cannot
    // resolve the name until the impl block above is in place, so a
    // rollback of the primitive breaks this whole test group.

    fn named_member(process_name: &str, state: MemberState) -> PoolMember {
        // 4-slot unallocated seed rides through the ONE substrate
        // owner `PoolMember::unallocated` — sibling to the `member`
        // helper in this file on the same epoch-anchored axis.
        PoolMember::unallocated(process_name, state, crate::time::at_epoch_second(0))
    }

    #[test]
    fn process_names_set_returns_empty_hashset_on_empty_slice() {
        // Zero-length pin: the empty-members corner produces an
        // empty `HashSet<String>`, matching the pre-lift
        // `.iter().map(...).collect()` chain's empty-iterator
        // behavior. A regression that started producing a sentinel
        // entry (a `""` placeholder, a static seed) on the empty-
        // slice corner would silently reject the first spawn slot
        // downstream — the pin closes that failure mode.
        let empty: Vec<PoolMember> = vec![];
        assert!(PoolMember::process_names_set(&empty).is_empty());
    }

    #[test]
    fn process_names_set_collects_every_process_name_from_populated_slice() {
        // Positive pin: every `PoolMember`'s `process_name` slot
        // lands in the returned `HashSet<String>` verbatim. Cross-
        // state (Free / Allocated / Spawning / Returning / Failed)
        // to prove the primitive is state-agnostic — the spawn arms
        // check occupancy on the name axis, NOT the state axis, so a
        // future refactor that filtered by state would silently
        // leave a returned/failed slot open to a duplicate spawn.
        let members = vec![
            named_member("pool-a-0", MemberState::Free),
            named_member("pool-a-1", MemberState::Allocated),
            named_member("pool-a-2", MemberState::Spawning),
            named_member("pool-a-3", MemberState::Returning),
            named_member("pool-a-4", MemberState::Failed),
        ];
        let set = PoolMember::process_names_set(&members);
        assert_eq!(set.len(), 5);
        for slot in 0..5 {
            let want = format!("pool-a-{slot}");
            assert!(set.contains(&want), "missing {want}; set = {set:?}");
        }
    }

    #[test]
    fn process_names_set_deduplicates_duplicate_process_names() {
        // Deduplication pin: two `PoolMember` entries with the same
        // `process_name` (a race between the two spawn arms, an
        // adopted foreign Process the reconciler picked up twice)
        // collapse to ONE entry in the `HashSet<String>`. Pins the
        // `HashSet` deduplication semantics the pre-lift `.iter()
        // .map(...).collect()` chain already inherited from the
        // `FromIterator` impl — a regression that swapped the
        // aggregate to a `Vec<String>` or `BTreeSet<String>` still
        // matches the shape but changes the operator-visible count
        // at the `.len()` probe here.
        let members = vec![
            named_member("pool-b-0", MemberState::Free),
            named_member("pool-b-0", MemberState::Spawning),
            named_member("pool-b-1", MemberState::Free),
        ];
        let set = PoolMember::process_names_set(&members);
        assert_eq!(set.len(), 2);
        assert!(set.contains("pool-b-0"));
        assert!(set.contains("pool-b-1"));
    }

    #[test]
    fn process_names_set_membership_probe_matches_pre_lift_chain_verbatim() {
        // Byte-identical parity pin: the `.contains(&candidate)`
        // probe on the substrate's `HashSet<String>` return returns
        // the same `bool` as the pre-lift `members.iter().map(|m|
        // m.process_name.clone()).collect::<HashSet<_>>().contains
        // (&candidate)` chain across the FULL cross product of
        // (candidate ∈ {an existing name, a novel name, the empty
        // string}). A regression that inserted a normalization step
        // at the primitive the pre-lift chain does NOT apply — or
        // vice versa — surfaces here rather than as silent drift
        // between the two spawn arms the primitive owns.
        let members = vec![
            named_member("pool-c-0", MemberState::Free),
            named_member("pool-c-1", MemberState::Allocated),
        ];
        let candidates: [&str; 4] = ["pool-c-0", "pool-c-1", "pool-c-2", ""];
        let via_primitive = PoolMember::process_names_set(&members);
        for candidate in candidates {
            let pre_lift: std::collections::HashSet<String> =
                members.iter().map(|m| m.process_name.clone()).collect();
            assert_eq!(
                via_primitive.contains(candidate),
                pre_lift.contains(candidate),
                "candidate = {candidate:?}"
            );
        }
    }

    #[test]
    fn process_names_set_is_a_pure_projection() {
        // Consecutive calls on the same slice return equal sets —
        // no cached state, no mutation on the input. Guards against
        // a future refactor that plants a cache field somewhere and
        // drifts one caller from another silently.
        let members = vec![
            named_member("pool-d-0", MemberState::Free),
            named_member("pool-d-1", MemberState::Spawning),
        ];
        let first = PoolMember::process_names_set(&members);
        let second = PoolMember::process_names_set(&members);
        assert_eq!(first, second);
    }

    #[test]
    fn pool_status_observed_composes_pre_lift_status_seed_verbatim() {
        // Composition pin: the substrate constructor produces a
        // `PoolStatus` structurally equal to the pre-lift 11-line
        // struct literal both pool-reconciler status-patch sites
        // stamped by hand. Any drift in the defaults (`message`,
        // `conditions`) or in the counter fanout surfaces here.
        let now = crate::time::at_epoch_second(1_700_000_000);
        let members = vec![
            member(MemberState::Free),
            member(MemberState::Allocated),
            member(MemberState::Spawning),
            member(MemberState::Returning),
            member(MemberState::Failed),
        ];
        let member_count = members.len();
        let observed = PoolStatus::observed(PoolPhase::Steady, members, now);
        assert_eq!(observed.phase, PoolPhase::Steady);
        assert_eq!(observed.phase_since, Some(now));
        assert_eq!(observed.ready_count, 1);
        assert_eq!(observed.allocated_count, 1);
        assert_eq!(observed.spawning_count, 1);
        assert_eq!(observed.returning_count, 1);
        assert_eq!(observed.members.len(), member_count);
        assert!(observed.message.is_none());
        assert!(observed.conditions.is_empty());
    }

    #[test]
    fn pool_status_observed_moves_members_by_value_without_extra_clone() {
        // Ownership pin: the constructor consumes the members Vec by
        // value rather than borrowing + cloning internally. Both pre-
        // lift sites called `.clone()` on their `members` binding for
        // the struct-literal `members:` slot; the substrate lift keeps
        // the same one-clone bound at the caller (or a straight move
        // if the caller no longer needs the local `members` binding
        // after the seed) rather than accidentally cloning twice.
        let members = vec![member(MemberState::Free), member(MemberState::Spawning)];
        let now = crate::time::at_epoch_second(0);
        let observed = PoolStatus::observed(PoolPhase::Steady, members, now);
        assert_eq!(observed.members.len(), 2);
    }

    // ─── PoolStatus::observed_now substrate pins ─────────────────────
    //
    // Bind [`PoolStatus::observed_now`] at fail-before-pass-after
    // granularity so a regression that dropped the wall-clock read
    // (yielding a `phase_since` of `Some(DateTime::default())`),
    // reshaped the delegation target (a peer 4-arg composer that
    // stamped different defaults), or diverged the peer from the 3-arg
    // [`PoolStatus::observed`] on any observable slot surfaces HERE
    // rather than as silent operator-facing drift at the two
    // controller_pool status-patch sites.
    //
    // Each pin is fail-before-pass-after: the primitive did not exist
    // pre-lift, so any test that invokes it fails to compile pre-lift
    // and passes post-lift; the byte-identity pins below then bind the
    // specific shape choice.

    #[test]
    fn pool_status_observed_now_composes_through_observed_with_wall_clock() {
        // Composition pin: `observed_now` MUST agree with the 3-arg
        // `observed(phase, members, Utc::now())` peer at every slot
        // other than `phase_since` (which reads the wall clock at
        // different instants and diverges by scheduler jitter). A
        // regression that specialized either composer (a stray
        // canonicalization at `observed_now`, a swapped default at the
        // 3-arg peer) would surface HERE rather than as silent skew at
        // the two controller_pool sites the primitive owns.
        let members = vec![
            member(MemberState::Free),
            member(MemberState::Allocated),
            member(MemberState::Spawning),
            member(MemberState::Returning),
        ];
        let via_now = PoolStatus::observed_now(PoolPhase::Steady, members.clone());
        let via_injected =
            PoolStatus::observed(PoolPhase::Steady, members.clone(), chrono::Utc::now());
        assert_eq!(via_now.phase, via_injected.phase);
        assert_eq!(via_now.ready_count, via_injected.ready_count);
        assert_eq!(via_now.allocated_count, via_injected.allocated_count);
        assert_eq!(via_now.spawning_count, via_injected.spawning_count);
        assert_eq!(via_now.returning_count, via_injected.returning_count);
        assert_eq!(via_now.members.len(), via_injected.members.len());
        assert_eq!(via_now.message, via_injected.message);
        assert_eq!(via_now.conditions.len(), via_injected.conditions.len());
    }

    #[test]
    fn pool_status_observed_now_reads_wall_clock_into_phase_since() {
        // Wall-clock pin: `phase_since` MUST fall between `Utc::now()`
        // reads bracketed around the call. A regression that dropped
        // the wall-clock read to a module-load constant (`Utc::now()`
        // captured at `static` init), a `DateTime::default()` (epoch),
        // or a stale `None` would fail this bracket check.
        let before = chrono::Utc::now();
        let observed = PoolStatus::observed_now(PoolPhase::Steady, vec![]);
        let after = chrono::Utc::now();
        let phase_since = observed
            .phase_since
            .expect("observed_now must stamp phase_since with the wall clock");
        assert!(
            phase_since >= before && phase_since <= after,
            "phase_since {phase_since} must fall in [{before}, {after}]"
        );
    }

    #[test]
    fn pool_status_observed_now_stamps_the_same_defaults_as_the_injected_peer() {
        // Defaults pin: `message: None` + `conditions: vec![]` MUST
        // agree with the 3-arg [`PoolStatus::observed`] peer verbatim.
        // A regression that stamped a per-caller message default at
        // `observed_now` (a "wall-clock-stamped observation" prefix,
        // say) or seeded a "just-observed" Condition row would surface
        // HERE rather than as silent operator-facing drift at either
        // status-patch site.
        let observed = PoolStatus::observed_now(PoolPhase::Steady, vec![]);
        assert!(observed.message.is_none());
        assert!(observed.conditions.is_empty());
    }

    #[test]
    fn pool_status_observed_now_wall_clock_is_read_per_invocation_not_cached() {
        // Monotonic-read pin: two back-to-back `observed_now` calls
        // MUST read `Utc::now()` twice — the second `phase_since` MUST
        // be `>=` the first. A regression that cached a wall-clock read
        // into a `OnceLock` / lazy `static` would fire the SAME
        // `phase_since` for every caller on the reconciler's process
        // and every status-patch would carry the module-load instant
        // rather than the tick instant. Both instants may coincide on
        // a fast machine; use `>=` (not `>`) to keep the pin robust
        // against subsecond scheduler granularity while still catching
        // a cached-constant regression (where the second read would
        // be < the wall clock).
        let first = PoolStatus::observed_now(PoolPhase::Steady, vec![])
            .phase_since
            .expect("first observed_now stamps phase_since");
        let second = PoolStatus::observed_now(PoolPhase::Steady, vec![])
            .phase_since
            .expect("second observed_now stamps phase_since");
        assert!(
            second >= first,
            "second phase_since {second} must be >= first phase_since {first}"
        );
        // AND the second read MUST NOT precede the wall clock reads
        // bracketing the call — a cached-past constant would fail
        // this bound.
        let after = chrono::Utc::now();
        assert!(
            second <= after,
            "second phase_since {second} must be <= {after}"
        );
    }

    #[test]
    fn pool_status_observed_now_matches_pre_lift_utc_now_composition_shape() {
        // Byte-identical parity with the pre-lift
        // `PoolStatus::observed(phase, members.clone(), Utc::now())`
        // block both hand-authored callsites restated at their status-
        // patch sites, swept across representative pool-phase variants.
        // Both blocks read the wall clock at DIFFERENT instants so the
        // two anchors CAN differ by the wall-clock delta between calls
        // — bound the divergence at 100ms scheduler jitter, matching
        // the peer `seconds_ago_matches_hand_authored_pre_lift_chain_shape`
        // pin's tolerance on the sibling `crate::time` module.
        let members = vec![member(MemberState::Free), member(MemberState::Spawning)];
        for phase in [PoolPhase::Steady, PoolPhase::ScalingUp, PoolPhase::Degraded] {
            let composed = PoolStatus::observed_now(phase, members.clone())
                .phase_since
                .expect("observed_now stamps phase_since");
            let hand_authored = PoolStatus::observed(phase, members.clone(), chrono::Utc::now())
                .phase_since
                .expect("observed stamps phase_since");
            let delta = (hand_authored - composed).abs();
            assert!(
                delta <= chrono::Duration::milliseconds(100),
                "composed {composed} and hand-authored {hand_authored} must agree within 100ms scheduler jitter for phase={phase:?}"
            );
        }
    }

    // ─── PoolMember::unallocated substrate pins ───────────────────────
    //
    // Pins the 4-slot `{ process_name, state, entered_state_at,
    // allocation_ref: None }` composer's fill at fail-before-pass-after
    // granularity: `unallocated` did not exist pre-lift; the compiler
    // cannot resolve the name until the impl block above is in place,
    // so a rollback of the primitive breaks this whole test group. The
    // primitive owns FIVE workspace-wide seed sites (one production
    // walk in `tatara-pool-reconciler::controller_pool::reconcile_inner`
    // and four test helpers across `pool.rs`, `pool_decide.rs`, and
    // `allocation_decide.rs`) so a regression that drifts any of the
    // four slots (a mistyped `allocation_ref: Some(<sentinel>)`, a
    // reversed positional order at the composer entry, an accidental
    // canonicalization of the `entered_state_at` anchor) surfaces here
    // rather than as silent operator-facing skew between the production
    // seed and the three test-suite helpers on the SAME `PoolMember`
    // shape.

    #[test]
    fn pool_member_unallocated_fills_every_slot_verbatim() {
        // Positional-axis pin: the composer's four inputs land at the
        // four struct slots in declaration order. A regression that
        // swapped `process_name` and `entered_state_at` at the composer
        // entry (or that renamed the `allocation_ref` invariant slot to
        // a different `None`-preserving field) surfaces here.
        let anchor = crate::time::at_epoch_second(1_700_000_000);
        let m = PoolMember::unallocated("pool-x-0", MemberState::Free, anchor);
        assert_eq!(m.process_name, "pool-x-0");
        assert_eq!(m.state, MemberState::Free);
        assert_eq!(m.entered_state_at, anchor);
        assert!(m.allocation_ref.is_none());
    }

    #[test]
    fn pool_member_unallocated_accepts_owned_string_and_str_at_the_same_signature() {
        // `impl Into<String>` axis pin: both the `&'static str` shape
        // (every test-helper site) and the `String` shape (produced by
        // `Process::owned_name_or_empty` at the production
        // `controller_pool::reconcile_inner` site) reach the same
        // composer entry without a per-caller conversion. A regression
        // that narrowed the signature to `&str` alone would break the
        // production site's `owned_name_or_empty` handoff; a regression
        // that narrowed to `String` alone would force every test helper
        // to `.into()` at the callsite. This pin fences both corners.
        let anchor = crate::time::at_epoch_second(0);
        let via_str = PoolMember::unallocated("pool-y-0", MemberState::Spawning, anchor);
        let owned: String = "pool-y-0".to_string();
        let via_string = PoolMember::unallocated(owned, MemberState::Spawning, anchor);
        assert_eq!(via_str.process_name, via_string.process_name);
        assert_eq!(via_str.state, via_string.state);
        assert_eq!(via_str.entered_state_at, via_string.entered_state_at);
        assert_eq!(via_str.allocation_ref, via_string.allocation_ref);
    }

    #[test]
    fn pool_member_unallocated_matches_pre_lift_struct_literal_bytewise() {
        // Byte-shape parity pin: the composer output is structurally
        // equal to the pre-lift 4-slot struct literal every hand-
        // authored site stamped. Sweeps every `MemberState` variant so
        // a regression that special-cased one variant (e.g., pinned
        // `Allocated` to a bogus `Some(<placeholder>)` at the composer)
        // surfaces here rather than at the four downstream helpers'
        // callsites.
        let anchor = crate::time::at_epoch_second(1_700_000_000);
        for state in [
            MemberState::Free,
            MemberState::Allocated,
            MemberState::Spawning,
            MemberState::Returning,
            MemberState::Failed,
        ] {
            let via_primitive = PoolMember::unallocated("m", state, anchor);
            let hand_authored = PoolMember {
                process_name: "m".into(),
                state,
                entered_state_at: anchor,
                allocation_ref: None,
            };
            assert_eq!(via_primitive.process_name, hand_authored.process_name);
            assert_eq!(via_primitive.state, hand_authored.state);
            assert_eq!(
                via_primitive.entered_state_at,
                hand_authored.entered_state_at
            );
            assert_eq!(via_primitive.allocation_ref, hand_authored.allocation_ref);
        }
    }

    #[test]
    fn pool_member_unallocated_preserves_caller_clock_anchor() {
        // Clock-injectability pin: the composer does NOT read wall
        // time on its own — every consumer supplies its own
        // `entered_state_at` anchor (the production site from
        // `Process::observed_phase_since`, the `pool_decide` helper
        // from `crate::time::seconds_ago`, the `allocation_decide` and
        // `pool.rs` helpers from `Utc::now` / the epoch anchor). A
        // regression that started stamping the composer's own
        // `Utc::now()` would silently reset every downstream anchor
        // and break the fanout tests' epoch-based expectations.
        let epoch = crate::time::at_epoch_second(0);
        let future = crate::time::at_epoch_second(2_000_000_000);
        let anchored_at_epoch = PoolMember::unallocated("a", MemberState::Free, epoch);
        let anchored_at_future = PoolMember::unallocated("b", MemberState::Free, future);
        assert_eq!(anchored_at_epoch.entered_state_at, epoch);
        assert_eq!(anchored_at_future.entered_state_at, future);
        assert_ne!(
            anchored_at_epoch.entered_state_at, anchored_at_future.entered_state_at,
            "composer must preserve the caller-supplied anchor verbatim",
        );
    }

    // ─── EphemeralPool::has_name substrate pins ───────────────────────
    //
    // Pins the copy-form metadata-projection primitive on the
    // `metadata.name` axis's presence-and-equal corner — the
    // discriminant every `candidate_pools.iter().find(|p| ...)`
    // closure that resolves a pool from an owned-name handle
    // (`AllocationRef.name` / `AllocationDecision::Bind.pool.name`)
    // routes through. Sibling to the `_or_empty` family on the SAME
    // slot ([`EphemeralPool::name_or_empty`] +
    // [`EphemeralPool::owned_name_or_empty`]) — this primitive owns
    // the `None`-preserving corner the `_or_empty` family folds away.
    // Fail-before-pass-after granularity: `has_name` did not exist
    // pre-lift; the compiler cannot resolve the name until the impl
    // block above is in place, so a rollback of the primitive breaks
    // this whole module.
    #[test]
    fn has_name_returns_true_when_slot_is_populated_and_equal() {
        // Happy-path pin: the slot is set AND byte-identical to the
        // candidate. Both pre-lift `find` closures — `resolve_pool`'s
        // explicit-`pool_ref` half and `controller_allocation`'s TTL-
        // inheritance fallback — resolve their target pool exactly in
        // this corner, and the primitive returns `true` here to
        // authorize the resolution.
        let p = pool_named("attest-pool");
        assert!(p.has_name("attest-pool"));
    }

    #[test]
    fn has_name_returns_false_when_slot_is_populated_and_different() {
        // Populated-slot inequality pin: the primitive returns `false`
        // for every candidate that is NOT byte-identical to the slot,
        // including strict subsequences (`"attest"` vs. `"attest-pool"`),
        // strict superstrings (`"attest-pool-2"` vs. `"attest-pool"`),
        // and case-differ variants. This is the load-bearing property
        // that lets `find(|p| p.has_name(&candidate))` reject
        // non-matching pools rather than aliasing them together.
        let p = pool_named("attest-pool");
        assert!(!p.has_name("other-pool"));
        assert!(!p.has_name("attest"));
        assert!(!p.has_name("attest-pool-2"));
        assert!(!p.has_name("ATTEST-POOL"));
    }

    #[test]
    fn has_name_returns_false_when_slot_is_none_even_against_empty_candidate() {
        // The `None`-preserving discipline pin: an unset `metadata.name`
        // slot returns `false` even when the candidate is the empty
        // string. Distinguishes `has_name` from a naïve substitution
        // through the sibling `name_or_empty` primitive, which would
        // fold both `None` and `Some("")` to `""` and silently promote
        // an unnamed pool with an empty candidate into a spurious
        // match at the resolver's `find` closure. Byte-identical to
        // what the pre-lift `.as_deref() == Some(<candidate>)` chain
        // produced (`None == Some("")` is `false`), which is what
        // both consumer sites relied on.
        let p = pool_unnamed();
        assert!(p.metadata.name.is_none(), "fixture invariant");
        assert!(!p.has_name(""));
        assert!(!p.has_name("attest-pool"));
    }

    #[test]
    fn has_name_returns_true_only_when_populated_slot_and_candidate_are_both_empty() {
        // Populated-empty-slot corner pin: `Some(String::new())` is a
        // populated slot with an empty payload. `has_name("")` returns
        // `true` here (byte-identical `""` on both sides), while
        // `has_name("<anything else>")` returns `false`. This is the
        // corner where `has_name` DIVERGES from `name_or_empty`
        // observably: the `_or_empty` family folds this corner into
        // the same bucket as `None`, but `has_name` keeps the
        // presence bit visible — `Some("") == Some("")` is `true`
        // while `None == Some("")` is `false`.
        let mut p = pool_named("scratch");
        p.metadata.name = Some(String::new());
        assert!(p.has_name(""));
        assert!(!p.has_name("attest-pool"));
    }

    #[test]
    fn has_name_matches_pre_lift_chain_verbatim_across_full_corner_set() {
        // Byte-identical parity pin: the primitive returns the same
        // `bool` as the pre-lift `.metadata.name.as_deref() == Some
        // (candidate)` chain across the FULL cross product of
        // (slot ∈ {None, Some("attest-pool"), Some("")}) × (candidate
        // ∈ {"attest-pool", "", "other"}). A regression that inserted
        // a normalization step at the primitive the pre-lift chain
        // does NOT apply — or vice versa — surfaces here rather than
        // as silent drift between the two `find` closures the primitive
        // owns.
        let slots: [Option<String>; 3] =
            [None, Some(String::from("attest-pool")), Some(String::new())];
        let candidates: [&str; 3] = ["attest-pool", "", "other"];
        for slot in slots {
            let mut p = pool_named("scratch");
            p.metadata.name = slot.clone();
            for candidate in candidates {
                let pre_lift = p.metadata.name.as_deref() == Some(candidate);
                assert_eq!(
                    p.has_name(candidate),
                    pre_lift,
                    "slot = {slot:?}, candidate = {candidate:?}"
                );
            }
        }
    }

    #[test]
    fn has_name_diverges_from_name_or_empty_on_the_missing_slot_corner() {
        // Cross-primitive discipline pin: `has_name("")` and
        // `name_or_empty() == ""` MUST disagree on the `None`-slot
        // corner. `name_or_empty` returns `""` (its load-bearing
        // sentinel), so a naïve `name_or_empty() == ""` probe would
        // return `true` here — aliasing every unnamed pool to the
        // empty-candidate bucket at the resolver. `has_name`
        // preserves `Option::as_deref() == Some(_)`'s `None ⇒ false`
        // semantics, so it returns `false` and rejects the spurious
        // match. This test fences the WHOLE reason `has_name` exists
        // as a distinct primitive from the `_or_empty` family: a
        // future refactor that collapsed `has_name` into
        // `name_or_empty() == candidate` would break this pin and
        // silently regress the resolver's byte-comparison honesty.
        let p = pool_unnamed();
        assert_eq!(p.name_or_empty(), "");
        assert!(!p.has_name(""));
    }

    #[test]
    fn has_name_is_a_pure_projection() {
        // Consecutive calls with the same candidate return the same
        // `bool` — no cached state, no mutation on the `EphemeralPool`
        // between calls. Guards against a future refactor that plants
        // a cache field on `EphemeralPool` and drifts one caller from
        // another silently.
        let p = pool_named("router-pool");
        assert_eq!(p.has_name("router-pool"), p.has_name("router-pool"));
        assert_eq!(p.has_name("other"), p.has_name("other"));
        assert!(p.has_name("router-pool"));
        assert!(!p.has_name("other"));
    }

    // ─── PoolSpec::free_ttl_duration substrate pins ─────────────────
    //
    // The `humantime::parse_duration(&<field>).ok()` shape rides
    // through TWO peer inherent methods on peer spec types post-lift:
    // [`crate::lifetime::EphemeralLifetime::ttl_duration`] on the
    // `spec.lifetime.ephemeral.ttl` axis + [`PoolSpec::free_ttl_
    // duration`] on the `pool.spec.free_ttl` axis. These pins bind the
    // pool-spec-side primitive at fail-before-pass-after granularity
    // so a regression that drifts either surface (a per-fleet minimum
    // floor added at only one primitive, a canonical unit-normalization
    // pass, a warn-log on unparseable strings) fails here rather than
    // as silent operator-facing skew between the pool stale-free
    // bucket loop in `tatara-pool-reconciler::pool_decide::decide_pool`
    // and the ephemeral TTL-expiry gate in
    // `tatara-process::lifetime_clock::evaluate`.

    fn pool_spec_with_free_ttl(free_ttl: &str) -> PoolSpec {
        PoolSpec {
            free_ttl: free_ttl.into(),
            ..pool_spec()
        }
    }

    #[test]
    fn pool_spec_free_ttl_duration_parseable_humantime_projects_to_some() {
        for (ttl, expected_secs) in [
            ("30s", 30u64),
            ("5m", 300),
            ("1h", 3600),
            ("24h", 86_400),
            ("1d", 86_400),
        ] {
            let spec = pool_spec_with_free_ttl(ttl);
            assert_eq!(
                spec.free_ttl_duration(),
                Some(std::time::Duration::from_secs(expected_secs)),
                "free_ttl_duration drift for {ttl:?}",
            );
        }
    }

    #[test]
    fn pool_spec_free_ttl_duration_unparseable_returns_none() {
        // A typo (`"1our"`), an unsupported unit (`"1w"` — humantime
        // supports `w`, but `"forever"` doesn't), a non-humantime
        // literal that reached the field via API-server acceptance
        // ALL collapse to `None`. The `pool_decide::decide_pool`
        // caller collapses the corner via `.unwrap_or_default()`,
        // yielding `Duration::ZERO` — byte-identical to the pre-lift
        // hand-authored `humantime::parse_duration(&spec.free_ttl)
        // .unwrap_or_default()` semantics.
        for bad in ["", "1our", "forever", "not-a-duration", "1", "-1s"] {
            let spec = pool_spec_with_free_ttl(bad);
            assert_eq!(
                spec.free_ttl_duration(),
                None,
                "free_ttl_duration should be None for {bad:?}",
            );
        }
    }

    #[test]
    fn pool_spec_free_ttl_duration_zero_seconds_returns_some_zero() {
        // `"0s"` is a parseable-but-zero humantime literal — the
        // primitive returns `Some(Duration::ZERO)`, distinguishable
        // from the parse-failure `None` corner. Downstream consumers
        // that gate on `!free_ttl.is_zero()` collapse this back
        // together with the `None`-via-`unwrap_or_default()` corner,
        // but the primitive itself keeps the two shapes distinct so
        // a future consumer needing that distinction can reach for
        // it without a re-parse.
        let spec = pool_spec_with_free_ttl("0s");
        assert_eq!(
            spec.free_ttl_duration(),
            Some(std::time::Duration::ZERO),
            "0s should project to Some(Duration::ZERO), not None",
        );
    }

    #[test]
    fn pool_spec_free_ttl_duration_default_free_ttl_matches_24h() {
        // The default `free_ttl` is `"24h"` (via [`default_free_ttl`]).
        // The primitive on a `PoolSpec` carrying the default must
        // agree with a manually-parsed `"24h"` — a future
        // `default_free_ttl` change (a shorter recycling window, a
        // per-fleet override) reaches BOTH surfaces at once (this
        // pin + the `default_free_ttl` fn) without silent skew.
        let spec = pool_spec_with_free_ttl(&default_free_ttl());
        assert_eq!(
            spec.free_ttl_duration(),
            Some(std::time::Duration::from_secs(24 * 3600)),
        );
    }

    #[test]
    fn pool_spec_free_ttl_duration_matches_pre_lift_hand_authored_chain_bytewise() {
        // Byte-shape parity with the pre-lift hand-authored chain the
        // `pool_decide::decide_pool` stale-free bucket loop restated
        // (`humantime::parse_duration(&spec.free_ttl).ok()` — the
        // `.ok()` tail and the caller's `.unwrap_or_default()` compose
        // to the same `Duration::ZERO`-on-failure semantics). Sweeps
        // every callsite corner the pool reconciler plausibly
        // encounters: the default `"24h"` free-recycling window, a
        // short-window test override (`"10s"`), a parse-failure typo,
        // an empty string.
        for ttl in ["24h", "10s", "1our", ""] {
            let spec = pool_spec_with_free_ttl(ttl);
            let via_primitive = spec.free_ttl_duration();
            let hand_authored = humantime::parse_duration(&spec.free_ttl).ok();
            assert_eq!(
                via_primitive, hand_authored,
                "free_ttl_duration must be byte-identical to `humantime::\
                 parse_duration(&spec.free_ttl).ok()` for {ttl:?}",
            );
        }
    }

    #[test]
    fn pool_spec_free_ttl_duration_matches_peer_ephemeral_lifetime_ttl_duration_shape() {
        // Return-shape parity with the peer primitive
        // [`crate::lifetime::EphemeralLifetime::ttl_duration`]: given
        // the SAME humantime string on both peer fields (the pool
        // `free_ttl` slot AND the ephemeral `ttl` slot), the two
        // primitives return byte-identical `Option<Duration>` values.
        // A regression that inserted a per-primitive normalization
        // step at only one surface — a per-fleet minimum floor, a
        // canonical unit-normalization pass — surfaces here rather
        // than as silent operator-facing skew between the pool
        // stale-free bucket loop and the ephemeral TTL-expiry gate
        // on the SAME humantime literal.
        for ttl in ["30s", "1h", "24h", "1our", ""] {
            let pool_spec = pool_spec_with_free_ttl(ttl);
            let eph = crate::lifetime::EphemeralLifetime {
                ttl: ttl.into(),
                ..Default::default()
            };
            assert_eq!(
                pool_spec.free_ttl_duration(),
                eph.ttl_duration(),
                "peer-primitive shape drift for {ttl:?}",
            );
        }
    }

    // ── PoolSpec::with_template substrate pins ──────────────────────
    //
    // The 11-slot `PoolSpec { desired_size: <N>, min_size: 0, max_size:
    // 0, return_policy: ReturnPolicy::Replace, selector: <PoolSelector
    // ::default() or override>, template: <EphemeralSpec>, free_ttl:
    // "24h".into(), max_allocation_ttl: "4h".into(), desired: 0,
    // replacement_policy: Default::default(), stable_name_claim: false
    // }` struct-literal was open-coded verbatim at EIGHT hand-authored
    // callsites across two crates before this primitive closed it.
    // These pins bind the composed shape at fail-before-pass-after
    // granularity so a regression that drifted the wire-published
    // default at only one slot — a shorter `default_free_ttl`, a
    // widened `ReturnPolicy` default, a promoted `stable_name_claim`
    // seed — surfaces HERE rather than as silent operator-visible drift
    // across every fixture that keys assertions on the shape.
    fn hand_authored_pre_lift_with_template() -> PoolSpec {
        PoolSpec {
            desired_size: 0,
            min_size: 0,
            max_size: 0,
            return_policy: ReturnPolicy::Replace,
            selector: PoolSelector::default(),
            template: empty_template(),
            free_ttl: "24h".into(),
            max_allocation_ttl: "4h".into(),
            desired: 0,
            replacement_policy: ReplacementPolicy::default(),
            stable_name_claim: false,
        }
    }

    #[test]
    fn with_template_stamps_caller_supplied_template_verbatim() {
        // The caller-supplied slot is the ONE the substrate does not
        // default. A regression that reshaped the primitive's
        // pass-through — a hidden re-encode through
        // `serde_json::to_value` and back, a per-primitive
        // normalization that flipped a defaulted-inner slot — would
        // surface HERE rather than at every downstream seed whose
        // assertions key on the template shape.
        let t = empty_template();
        let s = PoolSpec::with_template(t.clone());
        assert_eq!(
            serde_json::to_value(&s.template).unwrap(),
            serde_json::to_value(&t).unwrap(),
        );
    }

    #[test]
    fn with_template_defaulted_slots_ride_wire_schema_defaults() {
        // Pins the sibling-default correspondence the doc-comment
        // names — every non-template slot rides its own
        // `#[serde(default = "…")]` value from the `pub struct
        // PoolSpec` schema above. A regression that promoted any
        // defaulted slot to a non-default (a shorter
        // `default_free_ttl`, a widened `ReturnPolicy` default, a
        // `stable_name_claim: true` seed) would move the baseline
        // HERE rather than at every downstream fixture.
        let s = PoolSpec::with_template(empty_template());
        assert_eq!(s.desired_size, 0);
        assert_eq!(s.min_size, 0);
        assert_eq!(s.max_size, 0);
        assert_eq!(s.return_policy, ReturnPolicy::default());
        assert_eq!(
            serde_json::to_value(&s.selector).unwrap(),
            serde_json::to_value(PoolSelector::default()).unwrap(),
        );
        assert_eq!(s.free_ttl, default_free_ttl());
        assert_eq!(s.max_allocation_ttl, default_max_allocation_ttl());
        assert_eq!(s.desired, 0);
        assert_eq!(s.replacement_policy, ReplacementPolicy::default());
        assert!(!s.stable_name_claim);
    }

    #[test]
    fn with_template_matches_hand_authored_pre_lift_bytewise() {
        // Byte-identical parity pin between the substrate primitive
        // and the pre-lift 11-slot struct-literal that recurred at
        // eight hand-authored sites (compared with `desired_size:
        // 0` to match the primitive's baseline — the five hand-
        // authored `desired_size: 1` sites compose the baseline via
        // struct-update and the pin below binds THAT axis
        // separately). Compares via `serde_json` value equality —
        // `PoolSpec` does not derive `PartialEq` (the typed fields
        // it composes over do not uniformly derive it), so a
        // serialize round-trip is the shape-equality currency the
        // pin family already uses.
        let composed = PoolSpec::with_template(empty_template());
        let hand = hand_authored_pre_lift_with_template();
        assert_eq!(
            serde_json::to_value(&composed).unwrap(),
            serde_json::to_value(&hand).unwrap(),
        );
    }

    #[test]
    fn with_template_supports_struct_update_override_at_each_pre_lift_axis() {
        // Sweeps every override axis the eight pre-lift seeds
        // exercised via struct-update syntax:
        // * `desired_size: 1` — six sites (the majority of pre-lift
        //   fixtures use a single-slot pool).
        // * `selector: <custom>` — two sites (router.rs +
        //   allocation_decide.rs).
        // * `desired: N` + `replacement_policy: <policy>` — one
        //   site (desired.rs's desired-count-loop fixture).
        // * `desired_size: N, min_size: N, max_size: N` — one site
        //   (pool_decide.rs's pure-decision fixture).
        // A regression that broke the struct-update path (e.g. a
        // `#[non_exhaustive]` attribute added to `PoolSpec` that
        // would refuse struct-update syntax across crate boundaries)
        // surfaces at compile time HERE rather than as an eight-site
        // downstream break.
        let base = PoolSpec::with_template(empty_template());
        let size_1 = PoolSpec {
            desired_size: 1,
            ..PoolSpec::with_template(empty_template())
        };
        assert_eq!(base.desired_size, 0);
        assert_eq!(size_1.desired_size, 1);
        // Every other slot rides the base composition.
        assert_eq!(size_1.free_ttl, base.free_ttl);
        assert_eq!(size_1.max_allocation_ttl, base.max_allocation_ttl);

        let custom_selector = PoolSelector::default();
        let with_selector = PoolSpec {
            desired_size: 1,
            selector: custom_selector,
            ..PoolSpec::with_template(empty_template())
        };
        assert_eq!(with_selector.desired_size, 1);
        assert_eq!(with_selector.free_ttl, base.free_ttl);

        let with_desired = PoolSpec {
            desired: 5,
            replacement_policy: ReplacementPolicy::HoldFailed,
            ..PoolSpec::with_template(empty_template())
        };
        assert_eq!(with_desired.desired, 5);
        assert_eq!(
            with_desired.replacement_policy,
            ReplacementPolicy::HoldFailed
        );
        assert_eq!(with_desired.desired_size, 0);

        let with_sizes = PoolSpec {
            desired_size: 3,
            min_size: 1,
            max_size: 5,
            ..PoolSpec::with_template(empty_template())
        };
        assert_eq!(with_sizes.desired_size, 3);
        assert_eq!(with_sizes.min_size, 1);
        assert_eq!(with_sizes.max_size, 5);
        assert_eq!(with_sizes.replacement_policy, base.replacement_policy);
    }

    #[test]
    fn with_template_is_call_time_construction_not_a_shared_singleton() {
        // Two independent calls produce structurally-equal but
        // distinct values — pins that the primitive is a plain
        // constructor rather than a `lazy_static` clone whose in-
        // place mutation at one consumer would silently mutate the
        // shape at every other consumer. Mirrors the sibling
        // `gate_compute_defaults_is_call_time_construction_not_a_
        // shared_singleton` pin on `ProcessSpec::gate_compute_defaults`.
        let a = PoolSpec::with_template(empty_template());
        let b = PoolSpec::with_template(empty_template());
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::to_value(&b).unwrap(),
        );
        assert!(!std::ptr::eq(&a, &b));
    }

    #[test]
    fn with_template_free_ttl_composes_with_free_ttl_duration_at_default_window() {
        // The primitive's `free_ttl` slot rides `default_free_ttl()`;
        // the sibling `free_ttl_duration` primitive parses that
        // literal into the same 24h `Duration` every pre-lift
        // reconciler-side seed produced. Pins the round-trip so a
        // regression that shifted `default_free_ttl` without
        // updating this baseline (or vice versa) surfaces HERE
        // rather than as silent skew between the composer and the
        // ttl-parse gate that consumes it.
        let s = PoolSpec::with_template(empty_template());
        assert_eq!(
            s.free_ttl_duration(),
            Some(std::time::Duration::from_secs(24 * 3600)),
        );
    }

    // ─── EphemeralPool::new_in substrate pins ─────────────────────────
    //
    // The pre-lift 2-line `let mut p = EphemeralPool::new(<name>,
    // <spec>); p.meta_mut().namespace = Some(<ns>.into());` chain
    // recurred at FOUR workspace-wide fixture sites in
    // `tatara-pool-reconciler` past the ★★ PRIME-DIRECTIVE ≥ 2
    // threshold. Post-lift the ONE substrate composer stamps a
    // namespaced `EphemeralPool` from `(name, ns, spec)` in one call.
    // Fail-before-pass-after granularity: `new_in` did not exist pre-
    // lift; the compiler cannot resolve the name until the impl block
    // above is in place, so a rollback of the primitive breaks this
    // whole pin block.

    #[test]
    fn new_in_stamps_metadata_name_from_the_name_slot() {
        // `name` slot → `metadata.name` projection pin. Guards against
        // a regression that dropped the `name` slot into a `generate_
        // name` slot, an `annotations` seed, or any downstream slot the
        // kube-derived [`Self::new`] does not populate at
        // `metadata.name` verbatim.
        let s = pool_spec();
        let p = EphemeralPool::new_in("attest-pool", "pools", s);
        assert_eq!(p.metadata.name.as_deref(), Some("attest-pool"));
    }

    #[test]
    fn new_in_stamps_metadata_namespace_from_the_ns_slot() {
        // `ns` slot → `metadata.namespace` projection pin. Guards
        // against a regression that dropped the `ns` slot into a
        // `labels` seed, an unrelated annotation, or that stamped
        // `namespace = None` even after a caller-supplied value.
        let s = pool_spec();
        let p = EphemeralPool::new_in("attest-pool", "pools", s);
        assert_eq!(p.metadata.namespace.as_deref(), Some("pools"));
    }

    #[test]
    fn new_in_stamps_spec_from_the_spec_slot_verbatim() {
        // `spec` slot → `spec` projection pin. A regression that
        // silently normalized the caller-supplied spec inside the
        // composer (a defaulted-slot reset, a per-fleet override) would
        // diverge from the byte-identical pass-through the pre-lift
        // 2-line chain produced.
        let mut s = pool_spec();
        s.desired_size = 7;
        let p = EphemeralPool::new_in("attest-pool", "pools", s.clone());
        assert_eq!(p.spec.desired_size, s.desired_size);
        assert_eq!(p.spec.min_size, s.min_size);
        assert_eq!(p.spec.max_size, s.max_size);
    }

    #[test]
    fn new_in_accepts_both_owned_and_borrowed_namespace_slot() {
        // The `impl Into<String>` ergonomic contract round-trips
        // through both `&'static str` (majority pre-lift caller shape)
        // AND owned `String` at the SAME signature. Guards against a
        // regression that narrowed the slot to `&str` only or that
        // silently double-`.into()`d an already-owned String.
        let s = pool_spec();
        let via_str = EphemeralPool::new_in("attest-pool", "pools", s.clone());
        let via_string = EphemeralPool::new_in("attest-pool", String::from("pools"), s.clone());
        assert_eq!(via_str.metadata.namespace, via_string.metadata.namespace);
    }

    #[test]
    fn new_in_matches_pre_lift_construct_then_set_namespace_bytewise() {
        // Byte-shape parity witness against the pre-lift 2-line chain
        // across the two representative namespace shapes the collapsed
        // sites used (`"ephemeral-pools"` at `router::pool`, `"pools"`
        // at `pool_decide::pool` + `desired::pool` +
        // `allocation_decide::pool`). A regression that shifted the
        // composer's output would diverge from the pre-lift literal
        // HERE rather than at every downstream fixture's downstream
        // assertion.
        for ns in ["ephemeral-pools", "pools"] {
            let via_primitive = EphemeralPool::new_in("attest-pool", ns, pool_spec());
            let mut hand_authored = EphemeralPool::new("attest-pool", pool_spec());
            hand_authored.metadata.namespace = Some(ns.into());
            assert_eq!(via_primitive.metadata.name, hand_authored.metadata.name);
            assert_eq!(
                via_primitive.metadata.namespace,
                hand_authored.metadata.namespace,
            );
        }
    }

    #[test]
    fn new_in_defaults_other_metadata_slots_at_kube_derived_new() {
        // The composer forwards to the kube-derived [`Self::new`] for
        // every non-namespace metadata slot. A regression that stamped
        // finalizers, owner_references, labels, or annotations inside
        // the composer's body — inheriting the pre-lift chain's
        // undocumented emptiness at those slots — would surface here.
        let p = EphemeralPool::new_in("attest-pool", "pools", pool_spec());
        assert!(p.metadata.finalizers.is_none());
        assert!(p.metadata.owner_references.is_none());
        assert!(p.metadata.labels.is_none());
        assert!(p.metadata.annotations.is_none());
    }
}
