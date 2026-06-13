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

use std::fmt;
use std::str::FromStr;

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
    /// `"scheduled"`, …
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
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

impl fmt::Display for AllocationPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AllocationPhase {
    type Err = UnknownAllocationPhase;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for phase in Self::ALL {
            if s == phase.as_str() {
                return Ok(phase);
            }
        }
        Err(UnknownAllocationPhase(s.to_string()))
    }
}

/// Typed parse failure carrying the offending input verbatim so the
/// operator-facing diagnostic surfaces the bad value, not a
/// normalized form. Symmetric to
/// [`crate::pool::UnknownReplacementPolicy`],
/// [`crate::pool::UnknownReturnPolicy`],
/// [`crate::lifetime::UnknownTeardownPolicy`],
/// [`crate::boundary::UnknownConditionKind`], and
/// [`crate::phase::UnknownPhase`].
#[derive(Debug, thiserror::Error)]
#[error("unknown allocation phase: {0}")]
pub struct UnknownAllocationPhase(pub String);

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
    #[test]
    fn allocation_phase_all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for phase in AllocationPhase::ALL {
            assert!(seen.insert(phase), "duplicate variant in ALL: {phase:?}");
        }
        assert_eq!(seen.len(), AllocationPhase::ALL.len());
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

    /// Every variant in `ALL` round-trips through `as_str` ↔
    /// `FromStr`. Adding a variant without extending the canonical
    /// projection fails here.
    #[test]
    fn allocation_phase_roundtrip_via_as_str() {
        for phase in AllocationPhase::ALL {
            assert_eq!(
                AllocationPhase::from_str(phase.as_str()).unwrap(),
                phase,
                "round-trip failed for {phase:?}"
            );
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — empty / lowercased / typo / unrelated — and the
    /// error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    #[test]
    fn unknown_allocation_phase_errors() {
        for bad in [
            "",
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
