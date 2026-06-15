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
///   name: pr-123-akeyless
///   namespace: ephemeral-pools
/// spec:
///   poolRef:
///     name: akeyless-attest-pool
///     namespace: ephemeral-pools
///   requestor:
///     kind: github-pr
///     repo: "pleme-io/akeyless-deployment"
///     branch: "fix-something"
///     prNumber: 123
///     prLabels: ["needs-akeyless"]
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

    /// Optional repo identifier (e.g., `"pleme-io/akeyless-deployment"`).
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_lisp::DeriveClosedSet)]
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
// generated by `#[derive(tatara_lisp::DeriveClosedSet)]` on the enum
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
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"requestor kind"`
// matches the prior hand-rolled `#[error("unknown requestor kind: {0}")]`
// verbatim — pinned by `unknown_requestor_kind_message_matches_substrate_convention`.
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
    tatara_lisp::DeriveClosedSet,
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
// generated by `#[derive(tatara_lisp::DeriveClosedSet)]` on the enum
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
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"allocation phase"`
// matches the prior hand-rolled `#[error("unknown allocation phase: {0}")]`
// verbatim — pinned by `unknown_allocation_phase_message_matches_substrate_convention`.
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
            repo: Some("pleme-io/akeyless-deployment".into()),
            branch: Some("fix-something".into()),
            pr_number: Some(123),
            sha: Some("abc123def".into()),
            pr_labels: vec!["needs-akeyless".into()],
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
    /// `<Self as tatara_lisp::ClosedSet>::parse_label`, so this
    /// helper exercises the same code path the allocation reconciler
    /// hits when parsing a CRD `enum:`-validated value back to the
    /// typed phase.
    #[test]
    fn allocation_phase_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<AllocationPhase>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, the YAML
    /// wire format, and the operator-facing reason strings the
    /// reconciler stamps via Display.
    #[test]
    fn allocation_phase_as_str_matches_serde() {
        for phase in AllocationPhase::ALL {
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

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift.
    #[test]
    fn allocation_phase_display_matches_as_str() {
        for phase in AllocationPhase::ALL {
            assert_eq!(phase.to_string(), phase.as_str());
        }
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
    /// `<Self as tatara_lisp::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path
    /// [`Requestor::known_kind`]'s `Option<RequestorKind>` collapse
    /// rides on when classifying inbound `Requestor.kind` strings. The
    /// arity is asserted by the `[Self; 4]` array type itself.
    #[test]
    fn requestor_kind_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<RequestorKind>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` is injective across the closed
    /// set — two variants can't share a wire-format literal (which
    /// would alias under Display + FromStr + the `kind:` filter on
    /// `PoolSelector`).
    #[test]
    fn requestor_kind_as_str_unique_per_variant() {
        use std::collections::HashSet;

        let names: Vec<&'static str> = RequestorKind::ALL.iter().map(|k| k.as_str()).collect();
        let unique: HashSet<&&'static str> = names.iter().collect();
        assert_eq!(
            unique.len(),
            names.len(),
            "non-injective as_str — Display would alias: {names:?}"
        );
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

    /// AUTO-DERIVED LABEL CONTRACT: the `#[closed_set(generate_unknown)]`
    /// attribute emits the carrier with the substrate-wide
    /// `#[error("unknown requestor kind: {0}")]` annotation auto-derived
    /// from the PascalCase enum name (via `pascal_to_spaced_lowercase`).
    /// Pins the projection byte-for-byte against the prior hand-rolled
    /// annotation so a regression in the derive's label helper would
    /// surface here rather than silently drifting the operator-facing
    /// diagnostic — sibling shape to
    /// `unknown_allocation_phase_message_matches_substrate_convention`
    /// below + every `unknown_<thing>_message_matches_substrate_convention`
    /// in `classification.rs` / `pool.rs` / `export.rs`.
    #[test]
    fn unknown_requestor_kind_message_matches_substrate_convention() {
        let err = UnknownRequestorKind("foo".to_string());
        assert_eq!(err.to_string(), "unknown requestor kind: foo");
    }

    /// AUTO-DERIVED LABEL CONTRACT: the `#[closed_set(generate_unknown)]`
    /// attribute emits the carrier with the substrate-wide
    /// `#[error("unknown allocation phase: {0}")]` annotation auto-derived
    /// from the PascalCase enum name. Pins the projection byte-for-byte
    /// against the prior hand-rolled annotation — see
    /// `unknown_requestor_kind_message_matches_substrate_convention`
    /// for the rationale.
    #[test]
    fn unknown_allocation_phase_message_matches_substrate_convention() {
        let err = UnknownAllocationPhase("foo".to_string());
        assert_eq!(err.to_string(), "unknown allocation phase: foo");
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
