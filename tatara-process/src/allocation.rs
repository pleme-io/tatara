//! `EphemeralAllocation` CRD — a typed request for a pool member.
//!
//! Pairs with `EphemeralPool`: an Allocation is the request side;
//! the pool reconciler answers it by matching one of its free
//! Process members and stamping the requestor's identity on the
//! Allocation's status.
//!
//! Topology:
//! - The requestor (GitHub PR webhook, CI runner, operator running
//!   `feira allocation request …`) creates an `EphemeralAllocation`.
//! - The pool reconciler watches Allocations; matches `spec.poolRef`
//!   (or routes via PoolSelector if `poolRef` is omitted) to a pool;
//!   picks one Free member; transitions the member to Allocated and
//!   the Allocation to Bound.
//! - When the requestor is done, it deletes the Allocation. The pool
//!   reconciler honors the pool's `returnPolicy` (Reset / Replace /
//!   Keep).

use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::pool::AllocationRef;

/// `EphemeralAllocation` CRD spec — a typed request for a pool member.
///
/// ```yaml
/// apiVersion: tatara.pleme.io/v1alpha1
/// kind: EphemeralAllocation
/// metadata:
///   name: pr-123-demo-app
///   namespace: ephemeral-pools
/// spec:
///   poolRef:
///     name: attest-pool
///     namespace: ephemeral-pools
///   requestor:
///     kind: github-pr
///     repo: "pleme-io/demo-app"
///     branch: "fix-something"
///     prNumber: 123
///     prLabels: ["needs-ephemeral"]
///   ttl: "1h"
/// ```
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "tatara.pleme.io",
    version = "v1alpha1",
    kind = "EphemeralAllocation",
    plural = "ephemeralallocations",
    shortname = "ealloc",
    namespaced,
    status = "AllocationStatus",
    printcolumn = r#"{"name":"Pool","type":"string","jsonPath":".spec.poolRef.name"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#,
    printcolumn = r#"{"name":"Process","type":"string","jsonPath":".status.assignedProcess.name"}"#,
    printcolumn = r#"{"name":"Requestor","type":"string","jsonPath":".spec.requestor.kind"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct AllocationSpec {
    /// Direct pool reference. When set, skip selector-based routing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool_ref: Option<AllocationRef>,

    /// Who is asking for the env.
    pub requestor: Requestor,

    /// How long the requestor needs the env (`humantime`). The pool
    /// reconciler clamps this to `pool.spec.maxAllocationTtl`.
    /// When unset, falls back to the pool's `template.ttl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,

    /// Operator-supplied notes — surfaced in `feira allocation list`
    /// for audit / debugging context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Identity + routing context for a request.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Requestor {
    /// Discriminator: `"github-pr"`, `"manual"`, `"ci-run"`,
    /// `"scheduled"`, … The wire shape is open by design — operators
    /// may register their own kinds and the [`crate::pool::PoolSelector`]
    /// matches on raw string equality. The substrate's own emitters
    /// stamp one of the four canonical kebab-case kinds enumerated by
    /// [`RequestorKind::ALL`]; [`Requestor::known_kind`] projects the
    /// open wire field through that closed-set view at ONE site so
    /// future kind-keyed consumers (pool dashboards, completion lists,
    /// audit-trail classifiers) sweep the typed variants without
    /// re-implementing `match self.kind.as_str()` arm-by-arm. Sibling
    /// shape to [`crate::receipt::ReceiptEnvelope::known_kind`].
    pub kind: String,

    /// Optional repo identifier (e.g., `"pleme-io/demo-app"`).
    /// Matched against `PoolSelector.repos`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,

    /// Optional branch name. Matched against `PoolSelector.branches`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,

    /// Optional PR number (for `kind: github-pr`). Surfaces in
    /// printcolumns + audit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u64>,

    /// Optional commit SHA (for `kind: github-pr` or `ci-run`).
    /// Stamped onto the allocated Process for traceability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,

    /// PR / commit labels — matched as a subset against
    /// `PoolSelector.prLabels`.
    #[serde(default)]
    pub pr_labels: Vec<String>,

    /// Free-form actor — username, CI runner ID, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

impl Requestor {
    /// Decode [`Self::kind`] into the typed [`RequestorKind`] variant
    /// when the wire string matches one of the four substrate-emitted
    /// canonical kebab-case kinds; `None` when the kind is an
    /// operator-registered open string (the schema is open by design —
    /// every allocation remains a valid allocation, but only typed
    /// kinds participate in closed-set dispatch). The (open `String`,
    /// closed-typed view) split lets future kind-keyed consumers
    /// (pool-selector classifiers, dashboard completion, audit-trail
    /// classifiers) sweep the typed variants without touching the
    /// open-by-design wire shape. Lifted as the canonical decode site
    /// so no consumer re-implements the `match self.kind.as_str()` arm-
    /// by-arm — the closed-set sweep happens through
    /// [`RequestorKind::from_str`] at ONE site. Sibling shape to
    /// [`crate::receipt::ReceiptEnvelope::known_kind`].
    #[must_use]
    pub fn known_kind(&self) -> Option<RequestorKind> {
        self.kind.parse().ok()
    }
}

/// Closed-set view over the substrate-emitted canonical
/// [`Requestor::kind`] wire strings — the four kebab-case
/// discriminators every pleme-io requestor stamps onto an
/// [`EphemeralAllocation`]: `github-pr` (the [`tatara_github_watcher`-
/// authored](../../tatara-github-watcher/src/allocation_factory.rs)
/// PR-driven path), `manual` (operator-authored via `feira allocation
/// request …`), `ci-run` (non-PR CI driver), and `scheduled` (a
/// cron-style emitter). The wire field stays `pub kind: String` on
/// [`Requestor`] so operators can register their own kinds without a
/// schema bump; this enum is the typed view future kind-keyed
/// consumers (pool dashboards, LSP completion, audit-trail
/// classifiers) sweep against.
///
/// Pre-lift the four canonical kinds existed only as `&'static str`
/// literals at four scattered sites — the documentation header on
/// [`Requestor::kind`], the [`crate::pool::PoolSelector::kinds`]
/// docstring, the `tatara-github-watcher` allocation factory, and the
/// per-test `kind: "github-pr".into()` fixtures. A rename of one
/// canonical kind (e.g. `"github-pr"` → `"github-pull-request"`) had
/// no compile-time link to the others, so the documentation drifted
/// independently of the emitter, and the [`PoolSelector::matches`]
/// kind-filter silently kept matching the old spelling forever. Post-
/// lift the (canonical-name, typed-variant) pairing binds at ONE site
/// ([`Self::as_str`]); the `From<RequestorKind> for String` bridge
/// lets emitters compose `Requestor { kind: RequestorKind::GithubPr.into(), … }`
/// so the four canonical strings stop appearing as bare `&'static str`
/// literals at author sites.
///
/// Adding a fifth kind (e.g. `Slack` → `"slack"`, `Webhook` →
/// `"webhook"`) lands at one [`Self::ALL`] entry + one [`Self::as_str`]
/// arm — exhaustively checked by the compiler (the `[Self; 4]` array
/// literal forces the arity) AND by the per-variant truth-table tests
/// below.
///
/// Sibling closed-set `ALL`-keyed lifts across the crate:
/// [`crate::receipt::ReceiptKind::ALL`] (the four substrate-emitted
/// receipt kinds — direct shape peer, same open-wire + closed-view
/// split), [`AllocationPhase::ALL`], [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::lifetime_clock::TerminateReasonKind::ALL`].
///
/// Theory anchor: THEORY.md §III — the typescape; the substrate's own
/// requestor kinds become a TYPE rather than four `&'static str`
/// literals at every author + docstring + fixture site. THEORY.md
/// §V.1 — knowable platform; the closed-set view turns "which kinds
/// does the substrate actually emit" from a grep job into a method
/// the compiler enforces exhaustively at every dispatch site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_closed_set::DeriveClosedSet)]
#[closed_set(via = "as_str", generate_unknown, display)]
pub enum RequestorKind {
    /// GitHub pull-request webhook — `tatara-github-watcher` stamps
    /// this on every allocation built from a `PullRequestEvent`.
    GithubPr,
    /// Operator-authored allocation — `feira allocation request …`
    /// and any hand-crafted CR.
    Manual,
    /// Non-PR CI driver — a pipeline run that wants an ephemeral env
    /// without an associated pull request.
    CiRun,
    /// Cron-style scheduled emitter — periodic allocation creation
    /// (e.g. nightly drift detection).
    Scheduled,
}

impl RequestorKind {
    /// The closed set of substrate-emitted requestor kinds — single
    /// source of truth that drives the [`Self::from_str`] decode sweep
    /// AND any future enumeration consumer (pool-selector classifiers,
    /// dashboard completion, `tatara-check` kind enumeration). Adding
    /// a fifth variant (e.g. `Slack` → `"slack"`) lands at one `ALL`
    /// entry + one `as_str` arm — exhaustively checked by the compiler
    /// (the `[Self; 4]` array literal forces the arity) AND by the
    /// per-variant truth-table tests below.
    pub const ALL: [Self; 4] = [Self::GithubPr, Self::Manual, Self::CiRun, Self::Scheduled];

    /// Canonical kebab-case wire-format kind — the literal that lands
    /// in [`Requestor::kind`] when this variant authors the request.
    /// Pinned to four byte-exact strings the substrate has already
    /// published (the `tatara-github-watcher` factory, the operator
    /// fixtures in this file, the `PoolSelector.kinds` filter, the
    /// CRD printcolumns) — renaming any one is a wire-format change,
    /// not a typed-internal refactor, and the
    /// `requestor_kind_canonical_names_pinned` truth-table test fails
    /// first to keep the substrate honest. Used by [`std::fmt::Display`]
    /// (single source of truth) and as the `String` projection that
    /// `From<RequestorKind> for String` ([`Self::into`]) composes so
    /// emitters can spell `Requestor { kind: RequestorKind::GithubPr.into(), … }`
    /// without re-typing the canonical literal at every author site.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GithubPr => "github-pr",
            Self::Manual => "manual",
            Self::CiRun => "ci-run",
            Self::Scheduled => "scheduled",
        }
    }
}

// `impl FromStr for RequestorKind` + `impl tatara_lisp::ClosedSet for
// RequestorKind` + `impl std::fmt::Display for RequestorKind` are
// generated by `#[derive(tatara_closed_set::DeriveClosedSet)]` on the enum
// declaration above. `label` delegates to the inherent
// `RequestorKind::as_str` via `#[closed_set(via = "as_str")]` so the
// kebab-case wire-format projection stays load-bearing (matches the
// `tatara-github-watcher` factory + the CRD printcolumns + the
// `PoolSelector.kinds` filter verbatim) while generic `T: ClosedSet`
// consumers reach the STABLE workspace-wide name (`label`). The
// `display` flag emits the `f.write_str(self.as_str())` delegation
// block — the substrate-wide closed-set-enum idiom's third piece —
// at the same proc-macro site rather than a hand-rolled
// `fmt::Display` block per implementor.

// `pub struct UnknownRequestorKind(pub String)` is generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"requestor kind"`
// matches the prior hand-rolled `#[error("unknown requestor kind: {0}")]`
// verbatim — pinned generically by clause (5) of
// `tatara_closed_set::assert_closed_set_well_formed::<RequestorKind>()` (called
// from `requestor_kind_is_well_formed_closed_set` in the test module).
// Symmetric to every sibling `Unknown*` error in this crate (e.g.
// [`UnknownAllocationPhase`], [`crate::receipt::UnknownReceiptKind`],
// [`crate::phase::UnknownPhase`], [`crate::lifetime::UnknownTeardownPolicy`]).

impl From<RequestorKind> for String {
    /// Composes [`RequestorKind::as_str`] into an owned `String` so
    /// every `impl Into<String>` API surface (the `kind:` field
    /// initializer on [`Requestor`] most notably) accepts the typed
    /// variant transparently — the call site stays
    /// `kind: RequestorKind::GithubPr.into()` and the typed → wire
    /// bridge runs through ONE place. Sibling shape to
    /// [`crate::receipt::ReceiptKind`]'s `From for String`.
    fn from(k: RequestorKind) -> Self {
        k.as_str().to_owned()
    }
}

impl From<RequestorKind> for &'static str {
    fn from(k: RequestorKind) -> Self {
        k.as_str()
    }
}

/// `EphemeralAllocation.status` — observed allocation state.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AllocationStatus {
    /// Current lifecycle phase.
    #[serde(default)]
    pub phase: AllocationPhase,

    /// When the phase last changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_since: Option<DateTime<Utc>>,

    /// Pool that owns the matched member. Set as soon as routing
    /// resolves; not cleared on release (audit trail).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_pool: Option<AllocationRef>,

    /// The Process backing this allocation, if Bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_process: Option<AllocationRef>,

    /// When the allocation was matched to a Process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allocated_at: Option<DateTime<Utc>>,

    /// Wall-clock expiry derived from `spec.ttl` + `allocated_at`.
    /// The pool reconciler force-returns the member at this point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,

    /// Operator-visible message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Standard Conditions.
    #[serde(default)]
    pub conditions: Vec<AllocationCondition>,
}

/// Allocation lifecycle phase.
///
/// Sibling closed-set lifts on the same `EphemeralAllocation` /
/// `EphemeralPool` axis: [`crate::pool::ReplacementPolicy::ALL`],
/// [`crate::pool::ReturnPolicy::ALL`]. Sibling closed-sets on the
/// `tatara-process` algebra: [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`].
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
pub enum AllocationPhase {
    /// Admitted; pool selector matching not yet attempted.
    Pending,
    /// Routed to a pool but no `Free` member is available — queued.
    Queued,
    /// A pool member has been assigned + transitioned to Allocated.
    Bound,
    /// `expires_at` reached or requestor deleted; member is returning.
    Releasing,
    /// Released; the allocation is a permanent audit record.
    Released,
    /// No pool selector matched. The reconciler will retry on each
    /// pool spec update; surfaced in status so operators see why.
    NoMatchingPool,
    /// Pool refused (e.g., `max_size` reached and no member can be
    /// freed) — operator intervention needed.
    Failed,
}

impl Default for AllocationPhase {
    fn default() -> Self {
        Self::Pending
    }
}

impl AllocationPhase {
    /// The closed set of allocation phases — single source of truth
    /// that drives the `as_str` / Display / `FromStr` triad AND the
    /// `is_terminal` / `needs_pool_routing` predicate pair the
    /// allocation reconciler's observe/decide split dispatches on.
    /// Adding an eighth variant lands at one `ALL` entry + one
    /// `as_str` arm + one arm per predicate — exhaustively checked by
    /// the compiler (the `[Self; 7]` array literal forces the arity)
    /// and by the implication test
    /// (`allocation_phase_terminal_excludes_routing`) so a new
    /// variant can't claim to be both terminal AND routing-eligible.
    pub const ALL: [Self; 7] = [
        Self::Pending,
        Self::Queued,
        Self::Bound,
        Self::Releasing,
        Self::Released,
        Self::NoMatchingPool,
        Self::Failed,
    ];

    /// Canonical PascalCase wire-format projection — matches the
    /// serde `rename_all = "PascalCase"` output verbatim AND the CRD
    /// `enum:` enumeration the allocation reconciler stamps on the
    /// `ephemeralallocations.tatara.pleme.io` schema. Pinned by
    /// `allocation_phase_as_str_matches_serde` so a variant rename
    /// can't drift between the typed surface, the CRD enum, the YAML
    /// wire format AND any operator-facing diagnostic composed via
    /// Display rather than a hard-coded literal that would silently
    /// rot.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Queued => "Queued",
            Self::Bound => "Bound",
            Self::Releasing => "Releasing",
            Self::Released => "Released",
            Self::NoMatchingPool => "NoMatchingPool",
            Self::Failed => "Failed",
        }
    }

    /// True iff the allocation has reached an absorbing state —
    /// `Released` (clean audit record) or `Failed` (pool refused;
    /// operator intervention needed). The allocation reconciler
    /// short-circuits both phases to `NoOp` rather than re-running
    /// the routing / heartbeat ladder against a settled record.
    ///
    /// Closed-set match (not `matches!`) so a future variant
    /// triggers the compiler's exhaustiveness check at this site
    /// rather than silently defaulting to `false` and letting a new
    /// terminal phase fall through into pool rebinding. Paired with
    /// `needs_pool_routing` they form the two-axis projection
    /// `allocation_decide::AllocationConvergence::decide` matches
    /// against — the impossible bucket `(true, true)` is pinned
    /// empty by `allocation_phase_terminal_excludes_routing`.
    pub const fn is_terminal(self) -> bool {
        match self {
            Self::Released | Self::Failed => true,
            Self::Pending | Self::Queued | Self::Bound | Self::Releasing | Self::NoMatchingPool => {
                false
            }
        }
    }

    /// True iff the allocation is on the routing path — the
    /// reconciler still needs to resolve a target pool + look up a
    /// free member. `Pending` (just admitted), `Queued` (matched
    /// pool was full last tick), and `NoMatchingPool` (no selector
    /// matched yet; retry on pool spec updates) all live here. The
    /// settled non-terminal phases `Bound` (already matched) and
    /// `Releasing` (being torn down) don't — they short-circuit to
    /// the heartbeat / release ladder without re-resolving the pool.
    ///
    /// Closed-set match (not `matches!`) — same exhaustiveness
    /// discipline as [`Self::is_terminal`]. Lifts the open-coded
    /// `phase != Released && phase != Bound` gate that
    /// `allocation_decide::AllocationConvergenceCtx::observe` used
    /// to predicate pool resolution on, AND closes the latent gap
    /// where `Failed` / `Releasing` (neither `Released` nor `Bound`)
    /// would slip through to the routing branch — a `Failed`
    /// allocation without a deletion timestamp could be silently
    /// rebound to a fresh pool member, which is the opposite of
    /// "operator intervention needed."
    pub const fn needs_pool_routing(self) -> bool {
        match self {
            Self::Pending | Self::Queued | Self::NoMatchingPool => true,
            Self::Bound | Self::Releasing | Self::Released | Self::Failed => false,
        }
    }
}

// `impl FromStr for AllocationPhase` + `impl tatara_lisp::ClosedSet for
// AllocationPhase` + `impl std::fmt::Display for AllocationPhase` are
// generated by `#[derive(tatara_closed_set::DeriveClosedSet)]` on the enum
// declaration above. `label` delegates to the inherent
// `AllocationPhase::as_str` via `#[closed_set(via = "as_str")]` so the
// PascalCase wire-format projection stays load-bearing (matches the serde
// rename + the CRD `enum:` enumeration the allocation reconciler stamps
// on the `ephemeralallocations.tatara.pleme.io` schema verbatim) while
// generic `T: ClosedSet` consumers reach the STABLE workspace-wide name
// (`label`). The `display` flag emits the `f.write_str(self.as_str())`
// delegation block at the same proc-macro site rather than a
// hand-rolled `fmt::Display` block per implementor.

// `pub struct UnknownAllocationPhase(pub String)` is generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"allocation phase"`
// matches the prior hand-rolled `#[error("unknown allocation phase: {0}")]`
// verbatim — pinned generically by clause (5) of
// `tatara_closed_set::assert_closed_set_well_formed::<AllocationPhase>()` (called
// from `allocation_phase_is_well_formed_closed_set` in the test module).
// Symmetric to [`crate::pool::UnknownReplacementPolicy`],
// [`crate::pool::UnknownReturnPolicy`],
// [`crate::lifetime::UnknownTeardownPolicy`],
// [`crate::boundary::UnknownConditionKind`], and
// [`crate::phase::UnknownPhase`].

/// Allocation Condition (same shape as PoolCondition for downstream
/// uniformity).
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AllocationCondition {
    pub type_: String,
    pub status: String,
    pub reason: String,
    pub message: String,
    pub last_transition_time: DateTime<Utc>,
}

impl EphemeralAllocation {
    /// The copy-form status-projection primitive on the phase axis:
    /// returns the [`AllocationPhase`] the pool reconciler currently
    /// persists at `status.phase`, wrapped in an `Option` so the
    /// missing-`status` corner collapses to `None` — the ONE-liner
    /// collapse of the paired `self.status.as_ref().map(|s| s.phase)`
    /// incantation the pool reconciler's `AllocationConvergenceCtx::
    /// observe` restated by hand pre-lift.
    ///
    /// Cross-CRD peer to [`crate::prelude::Process::observed_phase`]
    /// on the (CRD × phase-slot × observed-status) axis pair — both
    /// primitives walk the identical `.status.as_ref().map(|s| s.
    /// phase)` shape, differing only in the `Phase` type projected
    /// ([`AllocationPhase`] vs [`crate::phase::ProcessPhase`]). The
    /// substrate now owns the borrow-form `.status.as_ref().map(|s|
    /// s.phase)` chain axis-uniformly across the two `Phase`-having
    /// CRDs so a future normalization (a generation-filter that
    /// returns `None` for a phase stamped with a stale
    /// `metadata.generation`, a staleness gate that drops a phase
    /// whose observing `phase_since` predates a reconcile deadline,
    /// a canonicalization pass that maps a phase outside the CRD's
    /// closed set to `None`) lands at ONE substrate method per CRD
    /// rather than being restated at every observer.
    #[must_use]
    pub fn observed_phase(&self) -> Option<AllocationPhase> {
        self.status.as_ref().map(|s| s.phase)
    }

    /// The copy-form status-projection primitive on the phase axis
    /// with the [`AllocationPhase::Pending`] sink applied — the
    /// ONE-liner collapse of the paired `self.observed_phase().
    /// unwrap_or(AllocationPhase::Pending)` incantation the pool
    /// reconciler's `AllocationConvergenceCtx::observe` restated by
    /// hand pre-lift as a 5-line `.status.as_ref().map(|s| s.phase).
    /// unwrap_or(AllocationPhase::Pending)` chain.
    ///
    /// Pre-lift the chain sat at [`tatara-pool-reconciler::
    /// allocation_decide::AllocationConvergenceCtx::observe`]'s
    /// `phase` seed. Cross-CRD peer to [`crate::prelude::Process::
    /// observed_phase_or_pending`] on the (CRD × phase-slot × sink)
    /// axis pair — both primitives close the missing-`status`
    /// corner with each CRD's respective [`Default`]-equivalent
    /// `Pending` variant, and both compose on top of their peer
    /// [`Self::observed_phase`] / [`crate::prelude::Process::
    /// observed_phase`] borrow-form projections so a future
    /// normalization at the underlying `observed_phase` primitive
    /// reaches both the raw-`Option` accessor and the `Pending`-
    /// sinked composer through the SAME upstream body.
    ///
    /// The [`AllocationPhase::Pending`] sink is load-bearing as the
    /// "not yet observed" default — the pool reconciler's typed
    /// `AllocationPhase::needs_pool_routing` predicate returns
    /// `true` for `Pending`, so a freshly-admitted Allocation whose
    /// pool reconciler has not yet stamped a `.status` slot reads
    /// as `Pending` and immediately enters the routing ladder,
    /// matching the pre-lift `AllocationPhase::Pending` fallback
    /// semantics verbatim.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition
    /// — the two-link `.status.as_ref().map(|s| s.phase).unwrap_or
    /// (AllocationPhase::Pending)` chain recurred at both the
    /// [`crate::prelude::Process`] site (already lifted onto
    /// [`crate::prelude::Process::observed_phase_or_pending`]) AND
    /// the [`EphemeralAllocation`] site by hand, i.e. the SHAPE
    /// itself recurs past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// trigger, and is lifted to ONE owner per CRD here). THEORY.md
    /// §II.1 invariant 5 (composition preserves proofs — the pins
    /// bind the missing-`status` sink to `Pending` + populated-
    /// status pass-through + every [`AllocationPhase`] variant
    /// round-trip + byte-identical parity with the pre-lift
    /// two-link chain + cross-CRD peer coherence with
    /// [`crate::prelude::Process::observed_phase_or_pending`], so
    /// a regression that drifted any surface at
    /// `tests::observed_phase_*` rather than as silent operator-
    /// facing skew between the allocation observer's routing seed
    /// and the Process observer's dispatch seed).
    #[must_use]
    pub fn observed_phase_or_pending(&self) -> AllocationPhase {
        self.observed_phase().unwrap_or(AllocationPhase::Pending)
    }

    /// The borrow-form status-projection primitive on the bound-pool
    /// axis: returns the [`AllocationRef`] the pool reconciler
    /// currently persists at `status.bound_pool` (name + namespace of
    /// the pool that owns the matched member), with the
    /// missing-`status` corner AND the empty-slot corner BOTH
    /// collapsed to `None` — the ONE-liner collapse of the paired
    /// `self.status.as_ref().and_then(|s| s.bound_pool.<clone|as_ref>())`
    /// incantation the pool reconciler's `AllocationConvergenceCtx::
    /// observe` restated by hand pre-lift.
    ///
    /// Cross-CRD peer to [`crate::prelude::Process::observed_identity`]
    /// on the (CRD × structured-record-slot × borrow-form) axis pair
    /// — both primitives walk the identical `.status.as_ref()
    /// .and_then(|s| s.<slot>.as_ref())` shape, differing only in the
    /// record projected ([`AllocationRef`] here, [`crate::identity::
    /// Identity`] on `Process`). The substrate now owns the
    /// borrow-form `.status.as_ref().and_then(|s| s.<slot>.as_ref())`
    /// chain on the second `structured-record` slot across the two
    /// `status`-having CRDs, so a future normalization step (a
    /// generation-filter that returns `None` for a bound-pool
    /// reference stamped with a stale `metadata.generation`, a
    /// canonicalization pass that rejects a malformed
    /// `(name, namespace)` pair, a cross-cluster reference-rewrite
    /// gate) lands at ONE substrate method per CRD rather than being
    /// restated at every observer.
    ///
    /// Return-form axis: `Option<&AllocationRef>` mirrors the
    /// borrow-first discipline of [`crate::prelude::Process::
    /// observed_identity`]. The lone pre-lift consumer
    /// ([`tatara-pool-reconciler::allocation_decide::
    /// AllocationConvergenceCtx::observe`]'s `bound_pool` seed) spelled
    /// the projection as `.and_then(|s| s.bound_pool.clone())` — an
    /// eager clone allocated inside every reconcile pass even when the
    /// downstream branch (the Release-composition arm) needed only the
    /// borrow for the `.as_ref()` re-projection two lines later.
    /// Post-lift the consumer reaches the primitive borrow-first
    /// (`alloc.observed_bound_pool().cloned()`) and the empty-borrow
    /// corner clones nothing (`Option::cloned` on `None` is `None`);
    /// the composition point where the owned `AllocationRef` fallback
    /// is required (the `AllocationConvergenceCtx` snapshot slot,
    /// still `Option<AllocationRef>`-typed for serde stability) is the
    /// ONLY site that materializes an owned copy.
    ///
    /// The missing-`status` corner AND the populated-status-with-
    /// `bound_pool=None` corner BOTH collapse to `None` so
    /// `.is_some()` / `if let Some(_)` / `.cloned()` behave
    /// identically on an `EphemeralAllocation` whose status field is
    /// `None` and on one whose status carries an unpopulated
    /// `bound_pool` slot — matching what the pre-lift `.and_then(...)`
    /// chain produced. Consumers that need to tell those corners
    /// apart reach for [`Self::status`] directly, exactly as the
    /// existing peer accessors [`Self::observed_phase`] +
    /// [`Self::observed_phase_or_pending`] admit.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition
    /// — the `.status.as_ref().and_then(|s| s.<structured-record>
    /// .<clone|as_ref>())` shape recurred as ONE hand-authored
    /// `.and_then(|s| s.bound_pool.clone())` chain in
    /// [`tatara-pool-reconciler::allocation_decide::
    /// AllocationConvergenceCtx::observe`] AND as the peer
    /// [`crate::prelude::Process::observed_identity`] primitive
    /// already owned on the `Process` CRD's `status.identity` slot,
    /// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger at
    /// substrate-shape level. THEORY.md §II.1 invariant 5
    /// (composition preserves proofs — the pins bind the missing-
    /// `status` corner + the empty-`bound_pool`-slot corner + the
    /// borrow-form `&AllocationRef` lifetime + the zero-copy
    /// projection contract + byte-identical parity with the pre-lift
    /// `.and_then(|s| s.bound_pool.clone())` chain across the full
    /// corner set + cross-CRD peer coherence with
    /// [`crate::prelude::Process::observed_identity`], so a
    /// regression that drifted any surface at
    /// `tests::observed_bound_pool_*` rather than as silent operator-
    /// facing skew between the allocation observer's Release-
    /// composition seed and the Process observer's FORK-time
    /// identity seed on the SAME reconcile tick).
    #[must_use]
    pub fn observed_bound_pool(&self) -> Option<&AllocationRef> {
        self.status.as_ref().and_then(|s| s.bound_pool.as_ref())
    }

    /// The copy-form status-projection primitive on the TTL-expiry axis:
    /// returns the wall-clock deadline the pool reconciler currently
    /// persists at `status.expires_at` (derived from `spec.ttl` +
    /// `allocated_at` at Bind time), wrapped in an `Option` so both the
    /// missing-`status` corner AND the populated-status-with-`expires_at
    /// =None` corner collapse to `None` — the ONE-liner collapse of the
    /// paired `self.status.as_ref().and_then(|s| s.expires_at)`
    /// incantation the pool reconciler's `AllocationConvergenceCtx::
    /// observe` restated by hand pre-lift.
    ///
    /// Same-CRD peer to [`Self::observed_phase`] on the (CRD × copy-form
    /// × status-slot) axis pair — both primitives walk the identical
    /// `.status.as_ref().<map|and_then>(|s| s.<Copy-field>)` shape,
    /// differing only in the record projected ([`DateTime<Utc>`] here,
    /// [`AllocationPhase`] on the phase axis) and in the outer combinator
    /// (`and_then` here because the persisted field is itself an
    /// `Option<DateTime<Utc>>`, `map` there because the persisted phase
    /// is bare). The substrate now owns the copy-form
    /// `.status.as_ref().<map|and_then>(|s| s.<Copy-field>)` chain
    /// axis-uniformly across every `Copy`-valued slot on
    /// `AllocationStatus`, so a future normalization (a clock-skew
    /// guard that drops an `expires_at` stamped before its owning
    /// allocation's observed `allocated_at`, a canonicalization pass
    /// that clamps a deadline to a monotonic upper bound, a stale-
    /// timestamp gate that returns `None` on an `expires_at` older than
    /// a controller-configured horizon) lands at ONE substrate method
    /// rather than being restated at every observer.
    ///
    /// Return-form axis: `Option<DateTime<Utc>>` mirrors the copy-first
    /// discipline of [`Self::observed_phase`]. The lone pre-lift consumer
    /// ([`tatara-pool-reconciler::allocation_decide::
    /// AllocationConvergenceCtx::observe`]'s `expires_at` seed) spelled
    /// the projection as `.status.as_ref().and_then(|s| s.expires_at)` —
    /// a 3-link hand-authored chain the observer walked on every
    /// reconcile pass. Post-lift the consumer reaches the primitive
    /// once and the whole missing-status + empty-slot corner cross
    /// collapses at the substrate rather than at the callsite.
    ///
    /// The missing-`status` corner AND the populated-status-with-
    /// `expires_at=None` corner BOTH collapse to `None` so
    /// `.is_some()` / `if let Some(_)` / any `>=` deadline comparison
    /// behave identically on an `EphemeralAllocation` whose status
    /// field is `None` and on one whose status carries an unpopulated
    /// `expires_at` slot — matching what the pre-lift `.and_then(...)`
    /// chain produced. Consumers that need to tell those corners apart
    /// reach for [`Self::status`] directly, exactly as the existing peer
    /// accessors [`Self::observed_phase`] +
    /// [`Self::observed_phase_or_pending`] admit.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.status.as_ref().and_then(|s| s.<Copy-field>)` shape
    /// recurred as ONE hand-authored chain in
    /// [`tatara-pool-reconciler::allocation_decide::
    /// AllocationConvergenceCtx::observe`] AND as the copy-form peer
    /// [`Self::observed_phase`] primitive already owned on the same
    /// CRD's `status.phase` slot, past the substrate-shape recurrence
    /// trigger; the substrate now owns the third status-projection
    /// primitive on `EphemeralAllocation`, closing the copy-form family
    /// alongside the borrow-form [`Self::observed_bound_pool`]).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs — the
    /// pins bind the missing-`status` corner + the empty-`expires_at`-
    /// slot corner + the copy-form `DateTime<Utc>` return + byte-
    /// identical parity with the pre-lift `.and_then(|s| s.expires_at)`
    /// chain across the full corner set, so a regression that drifted
    /// any surface surfaces at `tests::observed_expires_at_*` rather
    /// than as silent operator-facing skew between the allocation
    /// observer's Release-composition TTL gate and any future consumer
    /// that reaches for the same slot).
    #[must_use]
    pub fn observed_expires_at(&self) -> Option<DateTime<Utc>> {
        self.status.as_ref().and_then(|s| s.expires_at)
    }
}

#[cfg(test)]
mod tests {
    // `FromStr` lives in scope at the test surface only — the derive
    // emits `impl ::core::str::FromStr` via the full path so the lib
    // body no longer reaches `FromStr` directly, but the cross-axis
    // sweeps + the verbatim-echo contract tests call
    // `AllocationPhase::from_str(bad)` / `bad.parse::<RequestorKind>()`.
    use std::str::FromStr;

    use super::*;

    #[test]
    fn requestor_minimum_shape_round_trips() {
        let r = Requestor {
            kind: "github-pr".into(),
            repo: Some("pleme-io/demo-app".into()),
            branch: Some("fix-something".into()),
            pr_number: Some(123),
            sha: Some("abc123def".into()),
            pr_labels: vec!["needs-ephemeral".into()],
            actor: Some("drzln".into()),
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("kind: github-pr"));
        assert!(yaml.contains("prNumber: 123"));
        let back: Requestor = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.kind, "github-pr");
        assert_eq!(back.pr_number, Some(123));
    }

    #[test]
    fn allocation_status_defaults_pending() {
        let s = AllocationStatus::default();
        assert_eq!(s.phase, AllocationPhase::Pending);
        assert!(s.bound_pool.is_none());
        assert!(s.assigned_process.is_none());
    }

    #[test]
    fn allocation_phase_round_trips_via_serde() {
        for p in [
            AllocationPhase::Pending,
            AllocationPhase::Queued,
            AllocationPhase::Bound,
            AllocationPhase::Releasing,
            AllocationPhase::Released,
            AllocationPhase::NoMatchingPool,
            AllocationPhase::Failed,
        ] {
            let s = serde_yaml::to_string(&p).unwrap();
            let back: AllocationPhase = serde_yaml::from_str(&s).unwrap();
            assert_eq!(back, p);
        }
    }

    // ── closed-set algebra contracts for AllocationPhase
    //    (ALL × as_str × FromStr × predicate-pair) ────────────────────

    /// `ALL` is the source of truth — pin its closure so a variant
    /// added without an `ALL` entry fails here via the uniqueness
    /// check before drifting `FromStr` or the sweep tests below. The
    /// arity is asserted by the `[Self; 7]` array type itself.
    ///
    /// Structural well-formedness of [`AllocationPhase`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants
    /// (`ALL` is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `allocation_phase_all_is_unique_and_complete` +
    /// `allocation_phase_roundtrip_via_as_str` + the empty-input arm
    /// of `unknown_allocation_phase_errors`. `FromStr` delegates to
    /// `<Self as tatara_closed_set::ClosedSet>::parse_label`, so this
    /// helper exercises the same code path the allocation reconciler
    /// hits when parsing a CRD `enum:`-validated value back to the
    /// typed phase.
    #[test]
    fn allocation_phase_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<AllocationPhase>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, the YAML
    /// wire format, and the operator-facing reason strings the
    /// reconciler stamps via Display.
    #[test]
    fn allocation_phase_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<AllocationPhase>();
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift.
    #[test]
    fn allocation_phase_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<AllocationPhase>();
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / unrelated — and the error
    /// echoes the input verbatim so the operator-facing diagnostic
    /// carries the offending value, not a normalized form. The
    /// empty-input arm is pinned by
    /// [`allocation_phase_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownAllocationPhase`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_allocation_phase_errors() {
        for bad in [
            "pending",
            "BOUND",
            "no-matching-pool",
            "release",
            "failed_state",
            "Reaped",
        ] {
            let err = AllocationPhase::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// TRUTH-TABLE CONTRACT: the predicate pair agrees with the
    /// documented per-variant disposition. `Released` + `Failed` are
    /// terminal (absorbing); `Pending` / `Queued` / `NoMatchingPool`
    /// need pool routing; `Bound` / `Releasing` are settled-but-not-
    /// terminal (heartbeat / release ladder).
    #[test]
    fn allocation_phase_predicate_truth_tables() {
        assert!(!AllocationPhase::Pending.is_terminal());
        assert!(AllocationPhase::Pending.needs_pool_routing());

        assert!(!AllocationPhase::Queued.is_terminal());
        assert!(AllocationPhase::Queued.needs_pool_routing());

        assert!(!AllocationPhase::Bound.is_terminal());
        assert!(!AllocationPhase::Bound.needs_pool_routing());

        assert!(!AllocationPhase::Releasing.is_terminal());
        assert!(!AllocationPhase::Releasing.needs_pool_routing());

        assert!(AllocationPhase::Released.is_terminal());
        assert!(!AllocationPhase::Released.needs_pool_routing());

        assert!(!AllocationPhase::NoMatchingPool.is_terminal());
        assert!(AllocationPhase::NoMatchingPool.needs_pool_routing());

        assert!(AllocationPhase::Failed.is_terminal());
        assert!(!AllocationPhase::Failed.needs_pool_routing());
    }

    /// IMPLICATION CONTRACT: `is_terminal → !needs_pool_routing`. A
    /// terminal allocation cannot also be routing-eligible — that's
    /// the bug the typed projection closes (a `Failed` allocation
    /// that's neither `Released` nor `Bound` would otherwise slip
    /// through the open-coded gate in `observe` and try to rebind to
    /// a pool member). A future variant that flipped both predicates
    /// true would fail here, forcing the author to flip one or
    /// extend the consumer dispatch site in
    /// `tatara-pool-reconciler::allocation_decide` deliberately
    /// rather than letting an impossible state slip in.
    #[test]
    fn allocation_phase_terminal_excludes_routing() {
        for phase in AllocationPhase::ALL {
            assert!(
                !(phase.is_terminal() && phase.needs_pool_routing()),
                "{phase:?} is both terminal and routing-eligible",
            );
        }
    }

    /// DEFAULT-AGREEMENT CONTRACT: `AllocationPhase::default()` is
    /// `Pending` — the entry state, neither terminal nor settled —
    /// and it lives on the routing path. A future default-variant
    /// rename without flipping the predicates fails here.
    #[test]
    fn allocation_phase_default_is_pending_and_routes() {
        let d = AllocationPhase::default();
        assert_eq!(d, AllocationPhase::Pending);
        assert!(!d.is_terminal());
        assert!(d.needs_pool_routing());
    }

    // ── RequestorKind closed-set truth-table ─────────────────────────

    /// Structural well-formedness of [`RequestorKind`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants
    /// (`ALL` is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `requestor_kind_all_enumerates_each_variant_exactly_once`
    /// + `requestor_kind_from_str_round_trips_canonical_names` + the
    /// empty-input arm of `requestor_kind_from_str_rejects_open_kinds`.
    /// `FromStr` delegates to
    /// `<Self as tatara_closed_set::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path
    /// [`Requestor::known_kind`]'s `Option<RequestorKind>` collapse
    /// rides on when classifying inbound `Requestor.kind` strings. The
    /// arity is asserted by the `[Self; 4]` array type itself.
    #[test]
    fn requestor_kind_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<RequestorKind>();
    }

    /// Byte-exact wire-format pin — renaming any of these is a wire-
    /// format change (the `tatara-github-watcher` emitter, the CRD
    /// printcolumns, the `PoolSelector.kinds` filter strings, the
    /// per-test `kind: "…".into()` fixtures all depend on these
    /// literals), not a typed-internal refactor.
    #[test]
    fn requestor_kind_canonical_names_pinned() {
        assert_eq!(RequestorKind::GithubPr.as_str(), "github-pr");
        assert_eq!(RequestorKind::Manual.as_str(), "manual");
        assert_eq!(RequestorKind::CiRun.as_str(), "ci-run");
        assert_eq!(RequestorKind::Scheduled.as_str(), "scheduled");
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased-mismatch / typo / unrelated — and the
    /// error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    /// The schema is open at the wire layer (operators MAY register
    /// new kinds and `Requestor::known_kind` collapses them to
    /// `None`), but the closed-set view is byte-exact. The empty-input
    /// arm is pinned by [`requestor_kind_is_well_formed_closed_set`]
    /// via the `tatara_lisp::ClosedSet` testkit; the cases here pin
    /// the verbatim-echo contract on the [`UnknownRequestorKind`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn requestor_kind_from_str_rejects_open_kinds() {
        for bad in [
            "github_pr",
            "GithubPr",
            "operator-custom-kind",
            "ci_run",
            "Scheduled",
        ] {
            let err = bad.parse::<RequestorKind>().unwrap_err();
            assert_eq!(err, UnknownRequestorKind(bad.to_string()));
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift (Display is
    /// what operator-facing diagnostics compose against).
    #[test]
    fn requestor_kind_display_delegates_to_as_str() {
        for k in RequestorKind::ALL {
            assert_eq!(format!("{k}"), k.as_str());
        }
    }

    /// The `String` projection that `From<RequestorKind> for String`
    /// ([`RequestorKind::into`]) composes is byte-equal to `as_str`.
    /// This is the typed → wire bridge — emitters spell
    /// `kind: RequestorKind::GithubPr.into()` and the canonical
    /// literal is materialized at ONE place.
    #[test]
    fn requestor_kind_into_string_matches_as_str() {
        for k in RequestorKind::ALL {
            let s: String = k.into();
            assert_eq!(s, k.as_str());
        }
    }

    /// The typed → wire → typed round-trip: composing a `Requestor`
    /// with `kind: RequestorKind::X.into()` produces an object whose
    /// `known_kind()` decodes back to `X`. Pins the bridge invariant
    /// at the `Requestor` boundary, not just at `RequestorKind`.
    #[test]
    fn known_kind_decodes_built_requestors() {
        for k in RequestorKind::ALL {
            let r = Requestor {
                kind: k.into(),
                repo: None,
                branch: None,
                pr_number: None,
                sha: None,
                pr_labels: vec![],
                actor: None,
            };
            assert_eq!(r.known_kind(), Some(k), "round-trip failed for {k:?}");
        }
    }

    /// Open-by-design: a custom operator-registered kind still
    /// stamps a valid `Requestor` (no schema rejection), it just
    /// doesn't project through the closed-set typed view. Mirrors
    /// `ReceiptEnvelope::known_kind`'s open-kind posture.
    #[test]
    fn known_kind_returns_none_for_open_kinds() {
        let r = Requestor {
            kind: "operator-custom-kind".into(),
            repo: None,
            branch: None,
            pr_number: None,
            sha: None,
            pr_labels: vec![],
            actor: None,
        };
        assert_eq!(r.known_kind(), None);
    }

    /// The four canonical literals match every previously-published
    /// fixture / doc anchor in this crate — pinning the bridge to
    /// existing call sites so any drift fails here before the next
    /// release ships.
    #[test]
    fn requestor_kind_matches_existing_fixture_literals() {
        // The `requestor_minimum_shape_round_trips` fixture above
        // composes `kind: "github-pr".into()` verbatim.
        assert_eq!(RequestorKind::GithubPr.as_str(), "github-pr");
        // The `allocation_spec_omits_optional_fields` fixture below
        // composes `kind: "manual".into()` verbatim.
        assert_eq!(RequestorKind::Manual.as_str(), "manual");
    }

    // Per-implementor `unknown_X_message_matches_substrate_convention`
    // tests removed — clause (5) of
    // `tatara_closed_set::assert_closed_set_well_formed::<T>()` now verifies
    // the substrate-wide `"unknown {SET_LABEL}: {input}"` carrier shape
    // generically (called above on `RequestorKind` /
    // `AllocationPhase` through their `*_is_well_formed_closed_set`
    // sites). The `SET_LABEL` projection is pinned independently by
    // `tatara_lisp_derive::pascal_to_spaced_lowercase_tests` —
    // together the two contracts guarantee the operator-facing
    // diagnostic without needing per-enum literal pins.

    // ─── EphemeralAllocation::observed_phase* substrate pins ────────
    //
    // Fail-before-pass-after granularity: neither `observed_phase` nor
    // `observed_phase_or_pending` existed before this commit, so each
    // pin fails to compile until the corresponding inherent method
    // lands. Post-lift the pins bind the missing-`status` corner + the
    // populated-status pass-through + byte-identical parity with the
    // pre-lift 5-line `.status.as_ref().map(|s| s.phase).unwrap_or
    // (AllocationPhase::Pending)` chain the pool reconciler's
    // `AllocationConvergenceCtx::observe` walked. Cross-CRD peer
    // coherence with `Process::observed_phase_or_pending` is pinned
    // by the `_matches_process_peer_shape` sweep at the tail.

    fn alloc_with_phase(phase: AllocationPhase) -> EphemeralAllocation {
        let spec = AllocationSpec {
            pool_ref: None,
            requestor: Requestor {
                kind: "manual".into(),
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
        let mut a = EphemeralAllocation::new("obs-alloc", spec);
        a.status = Some(AllocationStatus {
            phase,
            ..AllocationStatus::default()
        });
        a
    }

    fn alloc_without_status() -> EphemeralAllocation {
        let spec = AllocationSpec {
            pool_ref: None,
            requestor: Requestor {
                kind: "manual".into(),
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
        let mut a = EphemeralAllocation::new("no-status-alloc", spec);
        a.status = None;
        a
    }

    #[test]
    fn observed_phase_returns_none_when_status_is_none() {
        let a = alloc_without_status();
        assert!(a.observed_phase().is_none());
    }

    #[test]
    fn observed_phase_returns_populated_variant_verbatim() {
        for p in AllocationPhase::ALL {
            let a = alloc_with_phase(p);
            assert_eq!(
                a.observed_phase(),
                Some(p),
                "observed_phase must project the persisted variant verbatim for {p:?}"
            );
        }
    }

    #[test]
    fn observed_phase_matches_pre_lift_chain_bytewise() {
        // Sweep every corner: (status: None) plus every populated
        // (status: Some(phase)) variant. The pre-lift chain was
        // `alloc.status.as_ref().map(|s| s.phase)` — a 3-link chain
        // hand-authored inline at the observer. The primitive must
        // return the same `Option<AllocationPhase>` on every corner.
        let none_alloc = alloc_without_status();
        assert_eq!(
            none_alloc.observed_phase(),
            none_alloc.status.as_ref().map(|s| s.phase),
        );
        for p in AllocationPhase::ALL {
            let a = alloc_with_phase(p);
            assert_eq!(
                a.observed_phase(),
                a.status.as_ref().map(|s| s.phase),
                "primitive must be byte-identical to the pre-lift chain for {p:?}",
            );
        }
    }

    #[test]
    fn observed_phase_or_pending_defaults_to_pending_when_status_absent() {
        let a = alloc_without_status();
        assert_eq!(a.observed_phase_or_pending(), AllocationPhase::Pending);
    }

    #[test]
    fn observed_phase_or_pending_returns_populated_phase_verbatim() {
        for p in AllocationPhase::ALL {
            let a = alloc_with_phase(p);
            assert_eq!(
                a.observed_phase_or_pending(),
                p,
                "populated status must pass through verbatim for {p:?}"
            );
        }
    }

    #[test]
    fn observed_phase_or_pending_defaults_agree_with_allocation_phase_default() {
        // The `Pending` sink is load-bearing as the "not yet observed"
        // default. `AllocationPhase::default()` returns `Pending`; the
        // primitive must return the same variant on the missing-status
        // corner. A future default-variant rename that flipped
        // `AllocationPhase::default` without flipping the primitive
        // (or vice versa) surfaces here as a divergent seed for the
        // routing ladder.
        let a = alloc_without_status();
        assert_eq!(a.observed_phase_or_pending(), AllocationPhase::default());
    }

    #[test]
    fn observed_phase_or_pending_matches_pre_lift_chain_bytewise() {
        // The exact pre-lift 5-line chain in
        // `tatara-pool-reconciler::allocation_decide::
        // AllocationConvergenceCtx::observe` was:
        //     let phase = alloc
        //         .status
        //         .as_ref()
        //         .map(|s| s.phase)
        //         .unwrap_or(AllocationPhase::Pending);
        // Sweep every corner: (status: None) plus every populated
        // status variant. The primitive must be byte-identical for
        // every corner so the observer's routing decision matches
        // bytewise post-lift.
        let none_alloc = alloc_without_status();
        assert_eq!(
            none_alloc.observed_phase_or_pending(),
            none_alloc
                .status
                .as_ref()
                .map(|s| s.phase)
                .unwrap_or(AllocationPhase::Pending),
        );
        for p in AllocationPhase::ALL {
            let a = alloc_with_phase(p);
            assert_eq!(
                a.observed_phase_or_pending(),
                a.status
                    .as_ref()
                    .map(|s| s.phase)
                    .unwrap_or(AllocationPhase::Pending),
                "primitive must be byte-identical to the pre-lift 5-line chain for {p:?}",
            );
        }
    }

    #[test]
    fn observed_phase_or_pending_composes_from_observed_phase() {
        // The composer sits on top of the borrow-form projection —
        // `observed_phase_or_pending() == observed_phase().unwrap_or
        // (Pending)`. Pinning the composition means a future
        // normalization step layered onto `observed_phase` (a
        // generation-filter, a staleness gate, a canonicalization
        // pass) reaches BOTH the raw-`Option` accessor and the
        // `Pending`-sinked composer through the SAME upstream body,
        // without needing a per-corner rewrite of the composer.
        let none_alloc = alloc_without_status();
        assert_eq!(
            none_alloc.observed_phase_or_pending(),
            none_alloc
                .observed_phase()
                .unwrap_or(AllocationPhase::Pending),
        );
        for p in AllocationPhase::ALL {
            let a = alloc_with_phase(p);
            assert_eq!(
                a.observed_phase_or_pending(),
                a.observed_phase().unwrap_or(AllocationPhase::Pending),
                "composer must ride on top of the borrow-form projection for {p:?}",
            );
        }
    }

    #[test]
    fn observed_phase_is_a_pure_projection() {
        // Reading the phase twice must not mutate the allocation or
        // its status slot — pure projection semantics. Also witnesses
        // that the accessor doesn't clone / drop the inner `phase`
        // (the `Copy` scalar comes out identical on both reads).
        let a = alloc_with_phase(AllocationPhase::Bound);
        let one = a.observed_phase();
        let two = a.observed_phase();
        assert_eq!(one, two);
        assert!(a.status.is_some(), "projection must not consume the status");
    }

    #[test]
    fn observed_phase_pending_missing_status_and_populated_pending_collapse_to_same_composer_output(
    ) {
        // A subtle correctness pin: the missing-`status` corner and
        // a populated-with-Pending status BOTH read as `Pending`
        // through the composer — the observer cannot distinguish the
        // two through this accessor. This matches the pre-lift 5-line
        // chain's semantics exactly (an operator patching
        // `status.phase: Pending` is indistinguishable from a
        // freshly-admitted allocation with no status stamped yet).
        // The borrow-form `observed_phase` accessor DOES distinguish
        // the two, so a caller that needs to tell them apart reaches
        // for the raw `Option`.
        let none_alloc = alloc_without_status();
        let pending_alloc = alloc_with_phase(AllocationPhase::Pending);

        assert_eq!(
            none_alloc.observed_phase_or_pending(),
            pending_alloc.observed_phase_or_pending(),
        );
        assert_ne!(
            none_alloc.observed_phase(),
            pending_alloc.observed_phase(),
            "borrow-form accessor MUST distinguish missing-status from populated-Pending",
        );
    }

    #[test]
    fn observed_phase_or_pending_missing_status_sink_agrees_with_process_peer_shape() {
        // Cross-CRD peer-axis coherence with
        // `Process::observed_phase_or_pending`. Both primitives walk
        // the identical `.status.as_ref().map(|s| s.phase).unwrap_or
        // (<Phase>::Pending)` chain differing ONLY in the `Phase`
        // type projected. On a missing-status observation, each
        // primitive must return its CRD's `Default`-equivalent
        // `Pending` variant — for `EphemeralAllocation` that's
        // `AllocationPhase::Pending`; for `Process` that's
        // `crate::phase::ProcessPhase::Pending`. This pin binds the
        // sink-parity structurally so a future rename of either
        // default variant surfaces here as a divergent seed for the
        // observer's routing / dispatch decision rather than as
        // silent drift between the two reconcilers.
        let no_status_alloc = alloc_without_status();
        assert_eq!(
            no_status_alloc.observed_phase_or_pending(),
            AllocationPhase::default(),
        );
        // Peer-axis invariant on the `Process` side — the primitive
        // that owns the same shape reads `ProcessPhase::Pending` on
        // the missing-status corner via its own inherent method. The
        // parity is coordinated at the `Default` seat: both CRDs'
        // phase types default to `Pending`, so a rename that broke
        // one without the other would fail one of these two
        // conjoined assertions.
        assert_eq!(AllocationPhase::default(), AllocationPhase::Pending,);
        assert_eq!(
            crate::phase::ProcessPhase::default(),
            crate::phase::ProcessPhase::Pending,
        );
    }

    // ─── EphemeralAllocation::observed_bound_pool substrate pins ────
    //
    // The borrow-form status-projection primitive on the bound-pool
    // axis. Collapses the pre-lift hand-authored `.status.as_ref()
    // .and_then(|s| s.bound_pool.clone())` chain in
    // `tatara-pool-reconciler::allocation_decide::
    // AllocationConvergenceCtx::observe`'s `bound_pool` seed onto the
    // ONE substrate primitive. Cross-CRD peer to
    // `Process::observed_identity` on the (CRD × structured-record-
    // slot × borrow-form) axis pair — both primitives walk the
    // identical `.status.as_ref().and_then(|s| s.<slot>.as_ref())`
    // shape. Each pin is fail-before-pass-after: `observed_bound_pool`
    // did not exist pre-lift, so any test invoking it fails to compile
    // pre-lift and passes post-lift.

    fn sample_pool_ref(name: &str, ns: &str) -> AllocationRef {
        AllocationRef {
            name: name.to_string(),
            namespace: ns.to_string(),
        }
    }

    fn alloc_with_bound_pool(bound: Option<AllocationRef>) -> EphemeralAllocation {
        let spec = AllocationSpec {
            pool_ref: None,
            requestor: Requestor {
                kind: "manual".into(),
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
        let mut a = EphemeralAllocation::new("bp-alloc", spec);
        a.status = Some(AllocationStatus {
            phase: AllocationPhase::Bound,
            bound_pool: bound,
            ..AllocationStatus::default()
        });
        a
    }

    #[test]
    fn observed_bound_pool_returns_none_when_status_is_none() {
        // Missing-`status` corner pin: the primitive collapses the
        // no-status case to `None` so downstream `.is_some()` /
        // `if let Some(_)` / `.cloned().unwrap_or_else(...)` behave
        // identically on an `EphemeralAllocation` whose status field
        // is `None` and on one whose status carries an unpopulated
        // `bound_pool` slot. Matches the pre-lift `.and_then(...)`
        // chain's `None` byte-identically at the pool reconciler's
        // Release-composition seed.
        let a = alloc_without_status();
        assert!(a.observed_bound_pool().is_none());
    }

    #[test]
    fn observed_bound_pool_returns_none_when_slot_is_none() {
        // Empty-slot-under-populated-status corner pin: the primitive
        // returns `None`, matching the missing-`status` corner byte-
        // identically. A regression that treated the two corners
        // differently would silently promote an internal representation
        // detail (whether the pool reconciler has ever written a
        // status subresource) into observable behavior at the
        // Release-composition branch of the allocation reconciler's
        // `decide` transition rule.
        let a = alloc_with_bound_pool(None);
        assert!(a.observed_bound_pool().is_none());
    }

    #[test]
    fn observed_bound_pool_returns_borrow_when_slot_is_populated() {
        // Happy-path pin: with a populated `status.bound_pool` slot,
        // the primitive returns a borrowed `&AllocationRef` whose
        // (name, namespace) fields match the persisted record. A
        // regression that filtered / reshaped / canonicalized the
        // record would surface here rather than as silent skew at the
        // Release-composition seed's `.cloned()` materialization.
        let expected = sample_pool_ref("demo-pool", "pools");
        let a = alloc_with_bound_pool(Some(expected.clone()));
        let observed = a.observed_bound_pool().expect("populated slot");
        assert_eq!(observed, &expected);
        assert_eq!(observed.name, "demo-pool");
        assert_eq!(observed.namespace, "pools");
    }

    #[test]
    fn observed_bound_pool_is_a_zero_copy_borrow_projection() {
        // Borrow-discipline pin: the returned reference points at the
        // persisted `AllocationRef` in place — NOT a fresh allocation
        // or a clone. A regression that switched the projection to an
        // owned `AllocationRef` (via `.clone()`) would defeat the
        // zero-copy contract the lift's primary strict-widening
        // delivers (the observer's Release-composition arm clones
        // once at the composition point where the
        // `AllocationConvergenceCtx` snapshot slot requires the owned
        // value). Peer to the sibling
        // `Process::observed_identity_is_a_zero_copy_borrow_projection`
        // pin on the `Process` CRD's `status.identity` slot.
        let a = alloc_with_bound_pool(Some(sample_pool_ref("demo-pool", "pools")));
        let observed = a.observed_bound_pool().expect("populated slot") as *const _;
        let persisted = a.status.as_ref().unwrap().bound_pool.as_ref().unwrap() as *const _;
        assert!(std::ptr::eq(observed, persisted));
    }

    #[test]
    fn observed_bound_pool_is_a_pure_projection() {
        // Purity pin: calling the projection twice on the same
        // `EphemeralAllocation` returns byte-identical borrows (same
        // pointer). A regression that introduced state — a lazy-
        // cached reference, a normalization step that ran once and
        // cached — would surface here rather than as silent drift
        // between two dispatches within one reconcile pass.
        let a = alloc_with_bound_pool(Some(sample_pool_ref("demo-pool", "pools")));
        let one = a.observed_bound_pool().expect("populated slot") as *const _;
        let two = a.observed_bound_pool().expect("populated slot") as *const _;
        assert!(std::ptr::eq(one, two));
    }

    #[test]
    fn observed_bound_pool_matches_pre_lift_chain_bytewise() {
        // Byte-identical parity pin between the borrow-form primitive
        // here and the pre-lift `tatara-pool-reconciler`
        // `.status.as_ref().and_then(|s| s.bound_pool.clone())` chain.
        // Sweeps every corner every callsite plausibly encounters
        // (missing status, empty `bound_pool` slot, populated
        // `bound_pool` slot). A regression that inserted a
        // normalization step at the primitive the pre-lift chain does
        // NOT apply — or vice versa — surfaces here rather than as
        // silent drift between the pre-lift consumer site and the ONE
        // substrate owner it now routes through.
        fn pre_lift(a: &EphemeralAllocation) -> Option<AllocationRef> {
            a.status.as_ref().and_then(|s| s.bound_pool.clone())
        }
        // Missing status.
        let a = alloc_without_status();
        assert_eq!(a.observed_bound_pool().cloned(), pre_lift(&a));
        // Populated status, empty `bound_pool` slot.
        let a = alloc_with_bound_pool(None);
        assert_eq!(a.observed_bound_pool().cloned(), pre_lift(&a));
        // Populated status, populated `bound_pool` slot.
        let a = alloc_with_bound_pool(Some(sample_pool_ref("demo-pool", "pools")));
        assert_eq!(a.observed_bound_pool().cloned(), pre_lift(&a));
    }

    #[test]
    fn observed_bound_pool_missing_status_and_empty_slot_collapse_to_the_same_option_shape() {
        // Cross-corner coherence pin: the missing-`status` corner and
        // the populated-empty-slot corner return `Option`s whose
        // `.is_none()` / `.is_some()` observations are IDENTICAL. A
        // regression that promoted the missing-`status` corner to a
        // typed error (via a signature change to `Result<_, _>`) — or
        // that widened the empty-slot corner to a synthetic
        // `Some(AllocationRef::default())` — would surface here rather
        // than as silent operator-facing divergence between a never-
        // status-written allocation and a bound-pool-cleared
        // allocation on the Release-composition branch.
        let a_no_status = alloc_without_status();
        let a_empty_slot = alloc_with_bound_pool(None);
        assert_eq!(
            a_no_status.observed_bound_pool().is_none(),
            a_empty_slot.observed_bound_pool().is_none(),
        );
        assert_eq!(
            a_no_status.observed_bound_pool().is_some(),
            a_empty_slot.observed_bound_pool().is_some(),
        );
    }

    #[test]
    fn observed_bound_pool_shape_agrees_with_process_observed_identity_peer_axis() {
        // Cross-CRD peer-axis coherence pin binding the SAME
        // `.status.as_ref().and_then(|s| s.<slot>.as_ref())` shape
        // that both `EphemeralAllocation::observed_bound_pool` (this
        // primitive) and `Process::observed_identity` walk, differing
        // ONLY in the record projected. Structural test — both
        // signatures must resolve as `&Self -> Option<&Record>` fn
        // pointers, so a future rename or a signature drift that
        // (say) widened one side to `Option<Record>` or narrowed one
        // side to `Option<&str>` fails to compile here rather than
        // silently drifting the two reconcilers apart at their
        // respective observer seeds. The runtime side of the pin
        // sweeps the missing-status + empty-slot corners on the
        // `EphemeralAllocation` half; the `Process` half is exercised
        // by its own `crd.rs::tests::observed_identity_*` pin
        // family — this test binds only the peer-axis shape.
        let a_no_status = alloc_without_status();
        let a_empty_slot = alloc_with_bound_pool(None);
        assert!(a_no_status.observed_bound_pool().is_none());
        assert!(a_empty_slot.observed_bound_pool().is_none());
        // Structural peer-axis coherence: bind both signatures as fn
        // pointers at their peer resolution type so the compiler
        // refuses to build if either side's shape drifts. The `_`
        // let-bindings assert the target type inference.
        let _bound_pool_shape: fn(&EphemeralAllocation) -> Option<&AllocationRef> =
            EphemeralAllocation::observed_bound_pool;
        let _identity_shape: fn(&crate::prelude::Process) -> Option<&crate::identity::Identity> =
            crate::prelude::Process::observed_identity;
    }

    // ─── EphemeralAllocation::observed_expires_at substrate pins ────
    //
    // The copy-form status-projection primitive on the TTL-expiry axis.
    // Collapses the pre-lift hand-authored `.status.as_ref().and_then(
    // |s| s.expires_at)` chain in `tatara-pool-reconciler::
    // allocation_decide::AllocationConvergenceCtx::observe`'s
    // `expires_at` seed onto the ONE substrate primitive. Same-CRD peer
    // to `observed_phase` on the (copy-form × status-slot) axis — both
    // primitives walk the identical `.status.as_ref().<map|and_then>(
    // |s| s.<Copy-field>)` shape. Each pin is fail-before-pass-after:
    // `observed_expires_at` did not exist pre-lift, so any test invoking
    // it fails to compile pre-lift and passes post-lift.

    fn alloc_with_expires_at(expires_at: Option<DateTime<Utc>>) -> EphemeralAllocation {
        let spec = AllocationSpec {
            pool_ref: None,
            requestor: Requestor {
                kind: "manual".into(),
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
        let mut a = EphemeralAllocation::new("exp-alloc", spec);
        a.status = Some(AllocationStatus {
            phase: AllocationPhase::Bound,
            expires_at,
            ..AllocationStatus::default()
        });
        a
    }

    #[test]
    fn observed_expires_at_returns_none_when_status_is_none() {
        // Missing-`status` corner pin: the primitive collapses the
        // no-status case to `None` so downstream `.is_some()` / any
        // deadline comparison behaves identically on an
        // `EphemeralAllocation` whose status field is `None` and on
        // one whose status carries an unpopulated `expires_at` slot.
        // Matches the pre-lift `.and_then(...)` chain's `None` byte-
        // identically at the pool reconciler's Release-composition
        // TTL gate.
        let a = alloc_without_status();
        assert!(a.observed_expires_at().is_none());
    }

    #[test]
    fn observed_expires_at_returns_none_when_slot_is_none() {
        // Empty-slot-under-populated-status corner pin: the primitive
        // returns `None`, matching the missing-`status` corner byte-
        // identically. A regression that treated the two corners
        // differently would silently promote an internal representation
        // detail (whether the pool reconciler has ever written a
        // `status.expires_at` field for a not-yet-Bound allocation)
        // into observable behavior at the Release-composition branch
        // of the allocation reconciler's `decide` transition rule.
        let a = alloc_with_expires_at(None);
        assert!(a.observed_expires_at().is_none());
    }

    #[test]
    fn observed_expires_at_returns_populated_timestamp_verbatim() {
        // Happy-path pin: with a populated `status.expires_at` slot,
        // the primitive returns the persisted `DateTime<Utc>` verbatim.
        // A regression that filtered / clamped / canonicalized the
        // timestamp would surface here rather than as silent skew at
        // the Release-composition TTL gate's `>=` deadline comparison.
        let expected = Utc::now();
        let a = alloc_with_expires_at(Some(expected));
        assert_eq!(a.observed_expires_at(), Some(expected));
    }

    #[test]
    fn observed_expires_at_is_a_pure_projection() {
        // Purity pin: calling the projection twice on the same
        // `EphemeralAllocation` returns byte-identical `Option`s. A
        // regression that introduced state — a lazy-cached value, a
        // normalization step that ran once and cached — would surface
        // here rather than as silent drift between two dispatches
        // within one reconcile pass.
        let expected = Utc::now();
        let a = alloc_with_expires_at(Some(expected));
        assert_eq!(a.observed_expires_at(), a.observed_expires_at());
    }

    #[test]
    fn observed_expires_at_matches_pre_lift_chain_bytewise() {
        // Byte-identical parity pin between the copy-form primitive
        // here and the pre-lift `tatara-pool-reconciler`
        // `.status.as_ref().and_then(|s| s.expires_at)` chain. Sweeps
        // every corner every callsite plausibly encounters (missing
        // status, empty `expires_at` slot, populated `expires_at`
        // slot). A regression that inserted a normalization step at
        // the primitive the pre-lift chain does NOT apply — or vice
        // versa — surfaces here rather than as silent drift between
        // the pre-lift consumer site and the ONE substrate owner it
        // now routes through.
        fn pre_lift(a: &EphemeralAllocation) -> Option<DateTime<Utc>> {
            a.status.as_ref().and_then(|s| s.expires_at)
        }
        // Missing status.
        let a = alloc_without_status();
        assert_eq!(a.observed_expires_at(), pre_lift(&a));
        // Populated status, empty `expires_at` slot.
        let a = alloc_with_expires_at(None);
        assert_eq!(a.observed_expires_at(), pre_lift(&a));
        // Populated status, populated `expires_at` slot.
        let a = alloc_with_expires_at(Some(Utc::now()));
        assert_eq!(a.observed_expires_at(), pre_lift(&a));
    }

    #[test]
    fn observed_expires_at_missing_status_and_empty_slot_collapse_to_the_same_option_shape() {
        // Cross-corner coherence pin: the missing-`status` corner and
        // the populated-empty-slot corner return `Option`s whose
        // `.is_none()` / `.is_some()` observations are IDENTICAL. A
        // regression that promoted the missing-`status` corner to a
        // typed error (via a signature change to `Result<_, _>`) — or
        // that widened the empty-slot corner to a synthetic
        // `Some(Utc::now())` — would surface here rather than as
        // silent operator-facing divergence between a never-status-
        // written allocation and a Bind-time-without-TTL allocation on
        // the Release-composition branch.
        let a_no_status = alloc_without_status();
        let a_empty_slot = alloc_with_expires_at(None);
        assert_eq!(
            a_no_status.observed_expires_at().is_none(),
            a_empty_slot.observed_expires_at().is_none(),
        );
        assert_eq!(
            a_no_status.observed_expires_at().is_some(),
            a_empty_slot.observed_expires_at().is_some(),
        );
    }

    #[test]
    fn observed_expires_at_shape_agrees_with_observed_phase_peer_axis() {
        // Same-CRD peer-axis coherence pin binding the SAME
        // `.status.as_ref().<map|and_then>(|s| s.<Copy-field>)` shape
        // that both `EphemeralAllocation::observed_expires_at` (this
        // primitive) and `EphemeralAllocation::observed_phase` walk,
        // differing only in the outer combinator (`and_then` here
        // because the persisted field is itself `Option<T>`, `map`
        // there because the persisted phase is bare) and in the
        // projected `Copy` type. Structural test — both signatures
        // must resolve as `&Self -> Option<T>` fn pointers with `T`
        // `Copy`, so a future rename or a signature drift that (say)
        // widened one side to `Option<&T>` or narrowed one side to
        // `T` fails to compile here rather than silently drifting
        // the family apart. The runtime side of the pin sweeps the
        // missing-status + empty-slot corners on the `expires_at`
        // half; the `phase` half is exercised by its own
        // `tests::observed_phase_*` pin family — this test binds
        // only the peer-axis shape.
        let a_no_status = alloc_without_status();
        let a_empty_slot = alloc_with_expires_at(None);
        assert!(a_no_status.observed_expires_at().is_none());
        assert!(a_empty_slot.observed_expires_at().is_none());
        // Structural peer-axis coherence: bind both signatures as fn
        // pointers at their peer resolution type so the compiler
        // refuses to build if either side's shape drifts.
        let _expires_at_shape: fn(&EphemeralAllocation) -> Option<DateTime<Utc>> =
            EphemeralAllocation::observed_expires_at;
        let _phase_shape: fn(&EphemeralAllocation) -> Option<AllocationPhase> =
            EphemeralAllocation::observed_phase;
    }

    #[test]
    fn allocation_spec_omits_optional_fields() {
        let s = AllocationSpec {
            pool_ref: None,
            requestor: Requestor {
                kind: "manual".into(),
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
        let yaml = serde_yaml::to_string(&s).unwrap();
        assert!(!yaml.contains("poolRef"));
        assert!(!yaml.contains("ttl"));
        assert!(!yaml.contains("note"));
    }
}
