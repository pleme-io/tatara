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

use std::fmt;

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
///   name: akeyless-attest-pool
///   namespace: ephemeral-pools
/// spec:
///   desiredSize: 3
///   minSize: 1
///   maxSize: 5
///   returnPolicy: Reset
///   selector:
///     repos: ["pleme-io/akeyless-*"]
///     branches: ["main", "release-*"]
///     prLabels: ["needs-akeyless"]
///   template:
///     aplicacao:
///       chartRef: "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment"
///       version: "0.5.5"
///       profile: "gateway-with-internal-saas"
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
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown)]
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

impl fmt::Display for ReplacementPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// `impl FromStr for ReplacementPolicy` + `impl tatara_lisp::ClosedSet for
// ReplacementPolicy` are generated by `#[derive(tatara_lisp::DeriveClosedSet)]`
// on the enum declaration above. `label` delegates to the inherent
// `ReplacementPolicy::as_str` via `#[closed_set(via = "as_str")]` so the
// PascalCase wire-format projection stays load-bearing (matches the
// serde `rename_all = "PascalCase"` output AND the
// `tatara-pool-reconciler::desired::PoolConvergence` Pause reason
// emission verbatim) while generic `T: ClosedSet` consumers reach the
// STABLE workspace-wide name (`label`).

// `pub struct UnknownReplacementPolicy(pub String)` is generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
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

/// Light reference to an `EphemeralAllocation`.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AllocationRef {
    pub name: String,
    pub namespace: String,
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
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown)]
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

impl fmt::Display for MemberState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// `impl FromStr for MemberState` + `impl tatara_lisp::ClosedSet for
// MemberState` are generated by `#[derive(tatara_lisp::DeriveClosedSet)]`
// on the enum declaration above. `label` delegates to the inherent
// `MemberState::as_str` via `#[closed_set(via = "as_str")]` so the
// `pool_phase_from_members` supply calc can keep keying on
// `counts_toward_supply` against the typed variant while a generic
// `T: ClosedSet` consumer reaches the STABLE workspace-wide name
// (`label`) without knowing this enum lives in `tatara-process::pool`.

// `pub struct UnknownMemberState(pub String)` is generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
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
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown)]
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

impl fmt::Display for PoolPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// `impl FromStr for PoolPhase` + `impl tatara_lisp::ClosedSet for PoolPhase`
// are generated by `#[derive(tatara_lisp::DeriveClosedSet)]` on the enum
// declaration above. `label` delegates to the inherent `PoolPhase::as_str`
// via `#[closed_set(via = "as_str")]` so the operator-facing
// `phase={phase}` Display composition keeps reading the same canonical
// PascalCase projection while a generic `T: ClosedSet` consumer (a
// status-aggregator filter, the `feira pool list --healthy` predicate, a
// future SSE event router) can walk every variant without knowing the
// closed set lives in `tatara-process::pool`.

// `pub struct UnknownPoolPhase(pub String)` is generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
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
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown)]
pub enum ReturnPolicy {
    /// Tear down the Process + create a fresh one. Safe but slow
    /// (1-2 min spin-up before the slot is Free again).
    #[default]
    Replace,
    /// Keep the Process running; run a typed `:reset` Job that wipes
    /// state (DB drop, secrets rotate). Fast (~5-10s) but depends on
    /// the reset Job being correct for the workload. Akeyless-style
    /// systems are natural fits because the SaaS API is authoritative.
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

impl fmt::Display for ReturnPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// `impl FromStr for ReturnPolicy` + `impl tatara_lisp::ClosedSet for
// ReturnPolicy` are generated by `#[derive(tatara_lisp::DeriveClosedSet)]`
// on the enum declaration above. `label` delegates to the inherent
// `ReturnPolicy::as_str` via `#[closed_set(via = "as_str")]` so the
// `tatara-pool-reconciler::return_policy::plan_return` dispatch keeps
// reading the canonical PascalCase projection that matches the CRD
// `enum:` literal verbatim, while a generic `T: ClosedSet` consumer
// plugs in without knowing the enum lives in `tatara-process::pool`.

// `pub struct UnknownReturnPolicy(pub String)` is generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
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
        assert!(glob_match("pleme-io/*", "pleme-io/akeyless-deployment"));
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
            repos: vec!["pleme-io/akeyless-*".into()],
            ..Default::default()
        };
        assert!(s.matches(&MatchKey {
            repo: "pleme-io/akeyless-deployment",
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
            pr_labels: vec!["needs-akeyless".into(), "integration".into()],
            ..Default::default()
        };
        // Both labels present → match.
        assert!(s.matches(&MatchKey {
            repo: "x",
            branch: "y",
            pr_labels: &[
                "needs-akeyless".into(),
                "integration".into(),
                "extra".into()
            ],
            kind: "z",
        }));
        // One label missing → no match.
        assert!(!s.matches(&MatchKey {
            repo: "x",
            branch: "y",
            pr_labels: &["needs-akeyless".into()],
            kind: "z",
        }));
    }

    #[test]
    fn specificity_ranks_more_constrained_higher() {
        let general = PoolSelector::default();
        let specific = PoolSelector {
            repos: vec!["pleme-io/*".into()],
            branches: vec!["main".into()],
            pr_labels: vec!["needs-akeyless".into()],
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
    /// `<Self as tatara_lisp::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the pool reconciler hits when
    /// parsing a CRD `enum:`-validated value back to the typed policy.
    #[test]
    fn replacement_policy_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<ReplacementPolicy>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the
    /// YAML wire format.
    #[test]
    fn replacement_policy_as_str_matches_serde() {
        for policy in ReplacementPolicy::ALL {
            let serialized = serde_json::to_string(&policy).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                policy.as_str(),
                "as_str drift for {policy:?}: as_str={} serde={unquoted}",
                policy.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. The operator-facing
    /// "policy={policy}" diagnostic in `tatara-pool-reconciler::desired`
    /// composes through Display rather than through a hard-coded
    /// variant string.
    #[test]
    fn replacement_policy_display_matches_as_str() {
        for policy in ReplacementPolicy::ALL {
            assert_eq!(policy.to_string(), policy.as_str());
        }
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
        tatara_lisp::assert_closed_set_well_formed::<ReturnPolicy>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the
    /// YAML wire format.
    #[test]
    fn return_policy_as_str_matches_serde() {
        for policy in ReturnPolicy::ALL {
            let serialized = serde_json::to_string(&policy).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                policy.as_str(),
                "as_str drift for {policy:?}: as_str={} serde={unquoted}",
                policy.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift, mirroring the
    /// `ReplacementPolicy` discipline.
    #[test]
    fn return_policy_display_matches_as_str() {
        for policy in ReturnPolicy::ALL {
            assert_eq!(policy.to_string(), policy.as_str());
        }
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
        tatara_lisp::assert_closed_set_well_formed::<MemberState>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the YAML
    /// wire format the pool reconciler stamps on
    /// `status.members[].state`.
    #[test]
    fn member_state_as_str_matches_serde() {
        for state in MemberState::ALL {
            let serialized = serde_json::to_string(&state).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                state.as_str(),
                "as_str drift for {state:?}: as_str={} serde={unquoted}",
                state.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. Any operator-facing
    /// "state={state}" diagnostic that composes through Display
    /// inherits the canonical wire-format string automatically.
    #[test]
    fn member_state_display_matches_as_str() {
        for state in MemberState::ALL {
            assert_eq!(state.to_string(), state.as_str());
        }
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
        tatara_lisp::assert_closed_set_well_formed::<PoolPhase>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the YAML
    /// wire format the pool reconciler stamps on `status.phase`.
    #[test]
    fn pool_phase_as_str_matches_serde() {
        for phase in PoolPhase::ALL {
            let serialized = serde_json::to_string(&phase).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                phase.as_str(),
                "as_str drift for {phase:?}: as_str={} serde={unquoted}",
                phase.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. Any operator-facing
    /// "phase={phase}" diagnostic that composes through Display
    /// inherits the canonical wire-format string automatically.
    #[test]
    fn pool_phase_display_matches_as_str() {
        for phase in PoolPhase::ALL {
            assert_eq!(phase.to_string(), phase.as_str());
        }
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
}
