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
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
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
    /// Should the pool auto-spawn a replacement for a Failed member?
    pub fn replaces_failed(self) -> bool {
        matches!(self, Self::ReplaceImmediate)
    }

    /// Should reaching Failed on any member pause the whole pool?
    pub fn pauses_on_failure(self) -> bool {
        matches!(self, Self::PausePool)
    }
}

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
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

/// Pool lifecycle phase (observed across the whole pool population).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
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
            pr_labels: &["needs-akeyless".into(), "integration".into(), "extra".into()],
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
}
