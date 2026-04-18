//! `ProcessTable` — cluster-scoped `/proc` registry.

use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::phase::ProcessPhase;

/// ProcessTable — the cluster-wide `/proc` equivalent.
///
/// One per cluster (singleton by convention, name `"proc"`).
/// Aggregates every `Process` status and hands out PIDs.
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "tatara.pleme.io",
    version = "v1alpha1",
    kind = "ProcessTable",
    plural = "processtables",
    shortname = "pt",
    status = "ProcessTableStatus",
    printcolumn = r#"{"name":"Procs","type":"integer","jsonPath":".status.processCount"}"#,
    printcolumn = r#"{"name":"Ready","type":"integer","jsonPath":".status.readyCount"}"#,
    printcolumn = r#"{"name":"NextPID","type":"integer","jsonPath":".spec.nextSequence"}"#,
    printcolumn = r#"{"name":"Depth","type":"integer","jsonPath":".spec.maxDepth"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct ProcessTableSpec {
    /// Next hierarchical sequence number to hand out at this level.
    #[serde(default = "default_next_seq")]
    pub next_sequence: u32,

    /// PID path of this cluster's parent (None at the root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_pid: Option<String>,

    /// DNS domain (e.g., `quero.lol`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_domain: Option<String>,

    /// DNS zone id (e.g., Route53 zone id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_zone_id: Option<String>,

    /// Max recursion depth from this node (0 = unlimited).
    #[serde(default)]
    pub max_depth: u32,

    /// Max concurrent direct children (0 = unlimited).
    #[serde(default)]
    pub max_children: u32,

    /// Grace window before escalating SIGTERM → SIGKILL (seconds).
    #[serde(default = "default_sigterm_timeout")]
    pub sigterm_timeout_seconds: u32,

    /// After this long in Zombie, force-reap.
    #[serde(default = "default_zombie_timeout")]
    pub zombie_timeout_seconds: u32,

    /// When true, PID 1 adopts and terminates orphaned Processes.
    #[serde(default = "default_true")]
    pub orphan_reaping_enabled: bool,
}

fn default_next_seq() -> u32 {
    1
}
fn default_sigterm_timeout() -> u32 {
    480
}
fn default_zombie_timeout() -> u32 {
    600
}
fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessTableStatus {
    pub process_count: u32,
    pub ready_count: u32,
    #[serde(default)]
    pub processes: Vec<ProcessEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reconciled: Option<DateTime<Utc>>,
}

/// One row of `/proc`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessEntry {
    pub name: String,
    pub namespace: String,
    pub pid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_pid: Option<String>,
    pub phase: ProcessPhase,
    /// Serialized `ConvergencePointType` (e.g., `"Gate"`).
    pub point_type: String,
    /// Serialized `SubstrateType` (e.g., `"Observability"`).
    pub substrate: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<DateTime<Utc>>,
}
