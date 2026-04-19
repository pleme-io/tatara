//! Universal workload lifecycle — the distributed state machine.
//!
//! Every workload (task, allocation, job, node) follows:
//!   Initial → Warming → Executing → Contracting → Terminal
//!
//! This applies at every level with type-specific detail structs.
//! The generic `WorkloadPhase<W, E, C, T>` is parameterized by
//! detail types for each phase.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── Generic Phase Enum ─────────────────────────────────────────

/// The universal workload lifecycle phase.
///
/// State transitions form a strict DAG:
/// ```text
///   Initial → Warming → Executing → Contracting → Terminal
///                     ↘ Contracting (warm failed)
///                     ↘ Terminal (fast-fail, no cleanup needed)
///                                  → Initial (successful contraction, restart)
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum WorkloadPhase<W, E, C, T> {
    /// Defined but not active. Zero resources allocated.
    Initial,
    /// Preparing to execute. Resources being acquired.
    Warming(W),
    /// Active and serving.
    Executing(E),
    /// Gracefully winding down.
    Contracting(C),
    /// Final state. Will not transition again (unless recycled to Initial).
    Terminal(T),
}

impl<W, E, C, T> WorkloadPhase<W, E, C, T> {
    pub fn phase_name(&self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Warming(_) => "warming",
            Self::Executing(_) => "executing",
            Self::Contracting(_) => "contracting",
            Self::Terminal(_) => "terminal",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminal(_))
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Warming(_) | Self::Executing(_))
    }

    pub fn is_initial(&self) -> bool {
        matches!(self, Self::Initial)
    }
}

/// Validate that a phase transition is legal.
pub fn is_valid_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("initial", "warming")
            | ("warming", "executing")
            | ("warming", "contracting")
            | ("warming", "terminal")
            | ("executing", "contracting")
            | ("contracting", "terminal")
            | ("contracting", "initial")
    )
}

// ── Shared Types ───────────────────────────────────────────────

/// Why a workload is contracting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContractReason {
    Stopped,
    Superseded { new_version: u64 },
    NodeDrain,
    ScaleDown,
    HealthFailure,
    ResourcePressure,
}

/// Final outcome of a workload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Failed,
    Lost,
    Cancelled,
}

/// Health status (used across levels).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    #[default]
    Unknown,
    Passing,
    Warning {
        message: String,
    },
    Critical {
        message: String,
    },
}

/// Desired phase for an allocation (what the scheduler wants).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DesiredPhase {
    Active,
    Stopped { reason: ContractReason },
}

// ── Task-Level Detail Types ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskWarmProgress {
    pub fetch_progress: f64,
    pub deps_resolved: bool,
    pub port_allocated: bool,
    pub warmup_checks_passed: u32,
    pub warmup_checks_required: u32,
}

impl Default for TaskWarmProgress {
    fn default() -> Self {
        Self {
            fetch_progress: 0.0,
            deps_resolved: false,
            port_allocated: false,
            warmup_checks_passed: 0,
            warmup_checks_required: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskExecuteDetail {
    pub pid: Option<u32>,
    pub container_id: Option<String>,
    pub health: HealthStatus,
    pub started_at: DateTime<Utc>,
    pub health_check_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskContractDetail {
    pub reason: ContractReason,
    pub signal_sent_at: Option<DateTime<Utc>>,
    pub grace_period_secs: u64,
    pub drain_connections: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskTerminalDetail {
    pub outcome: Outcome,
    pub exit_code: Option<i32>,
    pub finished_at: DateTime<Utc>,
    pub restarts: u32,
}

/// Concrete task lifecycle phase.
pub type TaskPhase =
    WorkloadPhase<TaskWarmProgress, TaskExecuteDetail, TaskContractDetail, TaskTerminalDetail>;

// ── Allocation-Level Detail Types ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllocWarmProgress {
    pub secrets_resolved: bool,
    pub volumes_mounted: bool,
    pub task_progress: HashMap<String, TaskWarmProgress>,
    /// Network identity assigned in the routing table.
    #[serde(default)]
    pub network_identity_assigned: bool,
    /// Endpoint registered in the networking plane.
    #[serde(default)]
    pub endpoint_registered: bool,
}

impl Default for AllocWarmProgress {
    fn default() -> Self {
        Self {
            secrets_resolved: false,
            volumes_mounted: false,
            task_progress: HashMap::new(),
            network_identity_assigned: false,
            endpoint_registered: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllocExecuteDetail {
    pub registered_in_catalog: bool,
    pub health: HealthStatus,
    pub task_states: HashMap<String, TaskPhase>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllocContractDetail {
    pub reason: ContractReason,
    pub deregistered_from_catalog: bool,
    pub task_states: HashMap<String, TaskPhase>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllocTerminalDetail {
    pub outcome: Outcome,
    pub finished_at: DateTime<Utc>,
}

/// Concrete allocation lifecycle phase.
pub type AllocationPhase =
    WorkloadPhase<AllocWarmProgress, AllocExecuteDetail, AllocContractDetail, AllocTerminalDetail>;

// ── Node-Level Detail Types ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeWarmProgress {
    pub raft_joined: bool,
    pub gossip_converged: bool,
    pub drivers_ready: Vec<String>,
    /// WireGuard mesh tunnel established.
    #[serde(default)]
    pub wireguard_tunnel_up: bool,
    /// Number of mesh peers connected.
    #[serde(default)]
    pub mesh_peers_connected: u32,
}

impl Default for NodeWarmProgress {
    fn default() -> Self {
        Self {
            raft_joined: false,
            gossip_converged: false,
            drivers_ready: Vec::new(),
            wireguard_tunnel_up: false,
            mesh_peers_connected: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeExecuteDetail {
    pub eligible: bool,
    pub allocation_count: u32,
    pub last_heartbeat: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeContractDetail {
    pub reason: ContractReason,
    pub draining_allocations: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeTerminalDetail {
    pub departed_at: DateTime<Utc>,
    pub reason: ContractReason,
}

/// Concrete node lifecycle phase.
pub type NodePhase =
    WorkloadPhase<NodeWarmProgress, NodeExecuteDetail, NodeContractDetail, NodeTerminalDetail>;

// ── Desired/Observed State (for Raft replication) ──────────────

/// What the scheduler declares an allocation should be.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesiredAllocationState {
    pub alloc_id: Uuid,
    pub job_id: String,
    pub group_name: String,
    pub node_id: String,
    pub job_version: u64,
    pub desired_phase: DesiredPhase,
    pub generation: u64,
}

/// What a node observes an allocation to actually be.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedAllocationState {
    pub alloc_id: Uuid,
    pub node_id: String,
    pub phase: AllocationPhase,
    pub observed_at: DateTime<Utc>,
    pub observation_seq: u64,
}

// ── Migration from legacy enums ────────────────────────────────

use super::allocation::{AllocationState, TaskRunState};

impl From<AllocationState> for AllocationPhase {
    fn from(state: AllocationState) -> Self {
        match state {
            AllocationState::Pending => AllocationPhase::Initial,
            AllocationState::Running => AllocationPhase::Executing(AllocExecuteDetail {
                registered_in_catalog: false,
                health: HealthStatus::Unknown,
                task_states: HashMap::new(),
            }),
            AllocationState::Complete => AllocationPhase::Terminal(AllocTerminalDetail {
                outcome: Outcome::Success,
                finished_at: Utc::now(),
            }),
            AllocationState::Failed => AllocationPhase::Terminal(AllocTerminalDetail {
                outcome: Outcome::Failed,
                finished_at: Utc::now(),
            }),
            AllocationState::Lost => AllocationPhase::Terminal(AllocTerminalDetail {
                outcome: Outcome::Lost,
                finished_at: Utc::now(),
            }),
        }
    }
}

impl From<TaskRunState> for TaskPhase {
    fn from(state: TaskRunState) -> Self {
        match state {
            TaskRunState::Pending => TaskPhase::Initial,
            TaskRunState::Running => TaskPhase::Executing(TaskExecuteDetail {
                pid: None,
                container_id: None,
                health: HealthStatus::Unknown,
                started_at: Utc::now(),
                health_check_epoch: 0,
            }),
            TaskRunState::Dead => TaskPhase::Terminal(TaskTerminalDetail {
                outcome: Outcome::Failed,
                exit_code: None,
                finished_at: Utc::now(),
                restarts: 0,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_names() {
        let phase: TaskPhase = TaskPhase::Initial;
        assert_eq!(phase.phase_name(), "initial");

        let phase: TaskPhase = TaskPhase::Warming(TaskWarmProgress::default());
        assert_eq!(phase.phase_name(), "warming");
        assert!(phase.is_active());

        let phase: TaskPhase = TaskPhase::Terminal(TaskTerminalDetail {
            outcome: Outcome::Success,
            exit_code: Some(0),
            finished_at: Utc::now(),
            restarts: 0,
        });
        assert!(phase.is_terminal());
        assert!(!phase.is_active());
    }

    #[test]
    fn test_valid_transitions() {
        assert!(is_valid_transition("initial", "warming"));
        assert!(is_valid_transition("warming", "executing"));
        assert!(is_valid_transition("executing", "contracting"));
        assert!(is_valid_transition("contracting", "terminal"));
        assert!(is_valid_transition("contracting", "initial")); // restart
        assert!(is_valid_transition("warming", "terminal")); // fast-fail

        assert!(!is_valid_transition("initial", "executing")); // skip warm
        assert!(!is_valid_transition("executing", "warming")); // backward
        assert!(!is_valid_transition("terminal", "initial")); // dead is dead
        assert!(!is_valid_transition("initial", "contracting")); // nothing to contract
    }

    #[test]
    fn test_allocation_phase_from_legacy() {
        let phase: AllocationPhase = AllocationState::Pending.into();
        assert!(phase.is_initial());

        let phase: AllocationPhase = AllocationState::Running.into();
        assert_eq!(phase.phase_name(), "executing");

        let phase: AllocationPhase = AllocationState::Failed.into();
        assert!(phase.is_terminal());
    }

    #[test]
    fn test_task_phase_from_legacy() {
        let phase: TaskPhase = TaskRunState::Pending.into();
        assert!(phase.is_initial());

        let phase: TaskPhase = TaskRunState::Running.into();
        assert!(phase.is_active());

        let phase: TaskPhase = TaskRunState::Dead.into();
        assert!(phase.is_terminal());
    }

    #[test]
    fn test_serde_roundtrip() {
        let phase = AllocationPhase::Warming(AllocWarmProgress {
            secrets_resolved: true,
            volumes_mounted: false,
            network_identity_assigned: true,
            endpoint_registered: false,
            task_progress: HashMap::from([(
                "web".to_string(),
                TaskWarmProgress {
                    fetch_progress: 0.75,
                    deps_resolved: true,
                    port_allocated: true,
                    warmup_checks_passed: 2,
                    warmup_checks_required: 3,
                },
            )]),
        });

        let json = serde_json::to_string(&phase).unwrap();
        let back: AllocationPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(phase, back);
    }

    #[test]
    fn test_desired_phase_serde() {
        let desired = DesiredPhase::Stopped {
            reason: ContractReason::ScaleDown,
        };
        let json = serde_json::to_string(&desired).unwrap();
        let back: DesiredPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(desired, back);
    }
}
