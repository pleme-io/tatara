use chrono::{DateTime, Utc};
use openraft::BasicNode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::domain::event::{Event, EventRing};
use crate::domain::job::{DriverType, JobSpec, Resources};
use crate::domain::release::Release;
use crate::domain::source::{Source, SourceStatus};

/// Unique node identifier within the cluster.
pub type NodeId = u64;

/// Openraft node info — carries the gRPC/HTTP address.
pub type RaftNode = BasicNode;

/// Per-node metadata advertised via gossip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMeta {
    pub node_id: NodeId,
    pub hostname: String,
    pub http_addr: String,
    pub gossip_addr: String,
    pub raft_addr: String,
    pub os: String,
    pub arch: String,
    pub roles: NodeRoles,
    pub drivers: Vec<DriverType>,
    pub total_resources: Resources,
    pub available_resources: Resources,
    pub allocations_running: u32,
    pub joined_at: DateTime<Utc>,
    pub version: String,
    /// Whether this node is eligible for scheduling.
    #[serde(default = "default_eligible")]
    pub eligible: bool,
}

fn default_eligible() -> bool {
    true
}

/// Role configuration — what this node is willing to do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRoles {
    /// Participates in Raft consensus.
    pub voter: bool,
    /// Runs workload allocations.
    pub worker: bool,
    /// Priority for leader election (higher = preferred). 0 = never lead.
    pub voter_priority: u32,
}

impl Default for NodeRoles {
    fn default() -> Self {
        Self {
            voter: true,
            worker: true,
            voter_priority: 10,
        }
    }
}

/// Cluster-wide state snapshot (replicated via Raft).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClusterState {
    pub jobs: HashMap<String, crate::domain::job::Job>,
    pub allocations: HashMap<uuid::Uuid, crate::domain::allocation::Allocation>,
    /// Nodes known to the cluster (from gossip + raft membership).
    pub nodes: HashMap<NodeId, NodeMeta>,
    /// Content-addressed data store index (hash → list of nodes that have it).
    pub data_index: HashMap<String, Vec<NodeId>>,
    /// Monotonic counter for state changes.
    pub last_applied_log: u64,
    /// Event ring buffer.
    #[serde(default)]
    pub events: EventRing,
    /// Job version history: job_id → list of (version, job_spec_snapshot).
    #[serde(default)]
    pub job_history: HashMap<String, Vec<JobVersionEntry>>,
    /// Release tracking.
    #[serde(default)]
    pub releases: HashMap<uuid::Uuid, Release>,
    /// Source tracking (GitOps flake watchers).
    #[serde(default)]
    pub sources: HashMap<uuid::Uuid, Source>,
}

/// A snapshot of a job at a particular version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobVersionEntry {
    pub version: u64,
    pub spec: JobSpec,
    pub status: crate::domain::job::JobStatus,
    pub submitted_at: DateTime<Utc>,
}

/// A command applied to the Raft state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClusterCommand {
    // ── Job lifecycle ──
    PutJob(crate::domain::job::Job),
    UpdateJobStatus {
        job_id: String,
        status: crate::domain::job::JobStatus,
    },

    // ── Allocation lifecycle ──
    PutAllocation(crate::domain::allocation::Allocation),
    UpdateAllocation {
        alloc_id: uuid::Uuid,
        state: crate::domain::allocation::AllocationState,
        task_states: HashMap<String, crate::domain::allocation::TaskState>,
    },

    // ── Node membership ──
    RegisterNode(NodeMeta),
    RemoveNode(NodeId),

    // ── Content-addressed data index ──
    AdvertiseChunk {
        hash: String,
        node_id: NodeId,
    },
    RemoveChunkAdvertisement {
        hash: String,
        node_id: NodeId,
    },

    // ── Events ──
    EmitEvent(Event),

    // ── Job versioning ──
    RollbackJob {
        job_id: String,
        version: u64,
    },

    // ── Releases ──
    PutRelease(Release),
    UpdateReleaseStatus {
        release_id: uuid::Uuid,
        status: crate::domain::release::ReleaseStatus,
    },

    // ── Node lifecycle ──
    DrainNode {
        node_id: NodeId,
    },
    SetNodeEligibility {
        node_id: NodeId,
        eligible: bool,
    },

    // ── Sources ──
    PutSource(Source),
    UpdateSource {
        source_id: uuid::Uuid,
        status: SourceStatus,
        last_rev: Option<String>,
        last_error: Option<String>,
        managed_jobs: Option<HashMap<String, String>>,
    },
    DeleteSource {
        source_id: uuid::Uuid,
    },
}

/// Response from applying a command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClusterResponse {
    Ok,
    Job(crate::domain::job::Job),
    Allocation(crate::domain::allocation::Allocation),
    Release(Release),
    Source(Source),
    JobHistory(Vec<JobVersionEntry>),
    Events(Vec<Event>),
    Error(String),
}

/// Gossip key prefixes for chitchat KV namespace.
pub mod gossip_keys {
    pub const META: &str = "meta";
    pub const ROLE: &str = "role";
    pub const LOAD_CPU: &str = "load:cpu";
    pub const LOAD_MEM: &str = "load:mem";
    pub const ALLOC_COUNT: &str = "alloc:count";
    pub const DRIVERS: &str = "drivers";
    pub const DATA_HAVE: &str = "data:have";
    pub const VERSION: &str = "version";
    pub const HTTP_ADDR: &str = "http_addr";
    pub const RAFT_ADDR: &str = "raft_addr";
    pub const RAFT_LEADER: &str = "raft:leader";
}
