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
