use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use super::raft_node::RaftCluster;
use super::raft_sm::StateMachineData;
use super::types::{
    ClusterCommand, ClusterResponse, JobVersionEntry, NodeId, NodeMeta,
};
use crate::domain::allocation::{Allocation, AllocationState, TaskState};
use crate::domain::event::{Event, EventKind};
use crate::domain::job::{Job, JobStatus};
use crate::domain::release::{Release, ReleaseStatus};
use crate::domain::source::{Source, SourceStatus};

/// Cluster-backed store that reads from the in-memory Raft state machine
/// and writes through Raft consensus with full propagation tracking.
///
/// All state feeds into the in-memory data structure (Raft SM).
/// API reads come from memory (eventually consistent by default).
/// Writes are not complete until confirmed propagated across the entire cluster.
pub struct ClusterStore {
    raft: Arc<RaftCluster>,
    /// Direct reference to the in-memory state for fast reads.
    state: Arc<RwLock<StateMachineData>>,
    /// How long to wait for full propagation before giving up.
    propagation_timeout: Duration,
}

/// Result of a write operation including propagation status.
#[derive(Debug)]
pub struct WriteResult<T> {
    pub value: T,
    /// The Raft log index this write was committed at.
    pub log_index: u64,
    /// Whether the write has been confirmed propagated to ALL nodes.
    pub fully_propagated: bool,
    /// Number of nodes that have applied this write.
    pub propagated_count: usize,
    /// Total number of nodes in the cluster.
    pub total_nodes: usize,
}

impl ClusterStore {
    pub fn new(raft: Arc<RaftCluster>) -> Self {
        let state = raft.read_local_sync();

        Self {
            raft,
            state,
            propagation_timeout: Duration::from_secs(10),
        }
    }

    /// Set the maximum time to wait for full propagation on writes.
    pub fn with_propagation_timeout(mut self, timeout: Duration) -> Self {
        self.propagation_timeout = timeout;
        self
    }

    // ── Reads (from in-memory state machine — eventually consistent) ──

    pub async fn get_job(&self, id: &str) -> Option<Job> {
        let data = self.state.read().await;
        data.cluster_state.jobs.get(id).cloned()
    }

    pub async fn list_jobs(&self) -> Vec<Job> {
        let data = self.state.read().await;
        data.cluster_state.jobs.values().cloned().collect()
    }

    pub async fn get_allocation(&self, id: &uuid::Uuid) -> Option<Allocation> {
        let data = self.state.read().await;
        data.cluster_state.allocations.get(id).cloned()
    }

    pub async fn list_allocations(&self) -> Vec<Allocation> {
        let data = self.state.read().await;
        data.cluster_state.allocations.values().cloned().collect()
    }

    pub async fn list_allocations_for_job(&self, job_id: &str) -> Vec<Allocation> {
        let data = self.state.read().await;
        data.cluster_state
            .allocations
            .values()
            .filter(|a| a.job_id == job_id)
            .cloned()
            .collect()
    }

    pub async fn get_node_meta(&self, id: &NodeId) -> Option<NodeMeta> {
        let data = self.state.read().await;
        data.cluster_state.nodes.get(id).cloned()
    }

    pub async fn list_nodes(&self) -> Vec<NodeMeta> {
        let data = self.state.read().await;
        data.cluster_state.nodes.values().cloned().collect()
    }

    /// Get job version history.
    pub async fn get_job_history(&self, job_id: &str) -> Vec<JobVersionEntry> {
        let data = self.state.read().await;
        data.cluster_state
            .job_history
            .get(job_id)
            .cloned()
            .unwrap_or_default()
    }

    /// List events with optional filtering.
    pub async fn list_events(
        &self,
        kind: Option<&EventKind>,
        since: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Vec<Event> {
        let data = self.state.read().await;
        data.cluster_state
            .events
            .query(kind, since)
            .into_iter()
            .cloned()
            .collect()
    }

    /// List all releases.
    pub async fn list_releases(&self) -> Vec<Release> {
        let data = self.state.read().await;
        data.cluster_state.releases.values().cloned().collect()
    }

    /// Get a specific release.
    pub async fn get_release(&self, id: &uuid::Uuid) -> Option<Release> {
        let data = self.state.read().await;
        data.cluster_state.releases.get(id).cloned()
    }

    /// List all sources.
    pub async fn list_sources(&self) -> Vec<Source> {
        let data = self.state.read().await;
        data.cluster_state.sources.values().cloned().collect()
    }

    /// Get a specific source.
    pub async fn get_source(&self, id: &uuid::Uuid) -> Option<Source> {
        let data = self.state.read().await;
        data.cluster_state.sources.get(id).cloned()
    }

    /// Get a source by name.
    pub async fn get_source_by_name(&self, name: &str) -> Option<Source> {
        let data = self.state.read().await;
        data.cluster_state
            .sources
            .values()
            .find(|s| s.name == name)
            .cloned()
    }

    /// Linearizable read — confirms leadership first. Use for operations
    /// that absolutely need the latest state (rare).
    pub async fn get_job_linearizable(&self, id: &str) -> Result<Option<Job>> {
        let state = self.raft.read_state().await?;
        let data = state.read().await;
        Ok(data.cluster_state.jobs.get(id).cloned())
    }

    // ── Writes (through Raft with propagation tracking) ──

    /// Submit a job. Waits for full cluster propagation.
    pub async fn put_job(&self, job: Job) -> Result<WriteResult<Job>> {
        let resp = self.raft.write(ClusterCommand::PutJob(job)).await?;
        let job = match resp {
            ClusterResponse::Job(j) => j,
            ClusterResponse::Error(e) => anyhow::bail!("Failed to put job: {}", e),
            _ => anyhow::bail!("Unexpected response from Raft"),
        };

        let log_index = self.current_commit_index().await;
        let prop = self.await_propagation(log_index).await;

        Ok(WriteResult {
            value: job,
            log_index,
            fully_propagated: prop.fully_propagated,
            propagated_count: prop.propagated_count,
            total_nodes: prop.total_nodes,
        })
    }

    /// Update job status. Waits for full cluster propagation.
    pub async fn update_job_status(
        &self,
        job_id: &str,
        status: JobStatus,
    ) -> Result<WriteResult<Job>> {
        let resp = self
            .raft
            .write(ClusterCommand::UpdateJobStatus {
                job_id: job_id.to_string(),
                status,
            })
            .await?;

        let job = match resp {
            ClusterResponse::Job(j) => j,
            ClusterResponse::Error(e) => anyhow::bail!("Failed to update job: {}", e),
            _ => anyhow::bail!("Unexpected response from Raft"),
        };

        let log_index = self.current_commit_index().await;
        let prop = self.await_propagation(log_index).await;

        Ok(WriteResult {
            value: job,
            log_index,
            fully_propagated: prop.fully_propagated,
            propagated_count: prop.propagated_count,
            total_nodes: prop.total_nodes,
        })
    }

    /// Submit an allocation. Waits for full cluster propagation.
    pub async fn put_allocation(&self, alloc: Allocation) -> Result<WriteResult<Allocation>> {
        let resp = self
            .raft
            .write(ClusterCommand::PutAllocation(alloc))
            .await?;

        let alloc = match resp {
            ClusterResponse::Allocation(a) => a,
            ClusterResponse::Error(e) => anyhow::bail!("Failed to put allocation: {}", e),
            _ => anyhow::bail!("Unexpected response from Raft"),
        };

        let log_index = self.current_commit_index().await;
        let prop = self.await_propagation(log_index).await;

        Ok(WriteResult {
            value: alloc,
            log_index,
            fully_propagated: prop.fully_propagated,
            propagated_count: prop.propagated_count,
            total_nodes: prop.total_nodes,
        })
    }

    /// Update allocation state. Waits for full cluster propagation.
    pub async fn update_allocation_state(
        &self,
        alloc_id: uuid::Uuid,
        state: AllocationState,
        task_states: std::collections::HashMap<String, TaskState>,
    ) -> Result<WriteResult<Allocation>> {
        let resp = self
            .raft
            .write(ClusterCommand::UpdateAllocation {
                alloc_id,
                state,
                task_states,
            })
            .await?;

        let alloc = match resp {
            ClusterResponse::Allocation(a) => a,
            ClusterResponse::Error(e) => anyhow::bail!("Failed to update allocation: {}", e),
            _ => anyhow::bail!("Unexpected response from Raft"),
        };

        let log_index = self.current_commit_index().await;
        let prop = self.await_propagation(log_index).await;

        Ok(WriteResult {
            value: alloc,
            log_index,
            fully_propagated: prop.fully_propagated,
            propagated_count: prop.propagated_count,
            total_nodes: prop.total_nodes,
        })
    }

    /// Register a node in the cluster.
    pub async fn register_node(&self, meta: NodeMeta) -> Result<WriteResult<()>> {
        self.raft.write(ClusterCommand::RegisterNode(meta)).await?;

        let log_index = self.current_commit_index().await;
        let prop = self.await_propagation(log_index).await;

        Ok(WriteResult {
            value: (),
            log_index,
            fully_propagated: prop.fully_propagated,
            propagated_count: prop.propagated_count,
            total_nodes: prop.total_nodes,
        })
    }

    /// Advertise a chunk in the content-addressed data index.
    pub async fn advertise_chunk(&self, hash: String, node_id: NodeId) -> Result<()> {
        self.raft
            .write(ClusterCommand::AdvertiseChunk { hash, node_id })
            .await?;
        Ok(())
    }

    /// Emit an event into the cluster event ring.
    pub async fn emit_event(&self, event: Event) -> Result<()> {
        self.raft.write(ClusterCommand::EmitEvent(event)).await?;
        Ok(())
    }

    /// Rollback a job to a previous version.
    pub async fn rollback_job(
        &self,
        job_id: &str,
        version: u64,
    ) -> Result<WriteResult<Job>> {
        let resp = self
            .raft
            .write(ClusterCommand::RollbackJob {
                job_id: job_id.to_string(),
                version,
            })
            .await?;

        let job = match resp {
            ClusterResponse::Job(j) => j,
            ClusterResponse::Error(e) => anyhow::bail!("{}", e),
            _ => anyhow::bail!("Unexpected response from Raft"),
        };

        let log_index = self.current_commit_index().await;
        let prop = self.await_propagation(log_index).await;

        Ok(WriteResult {
            value: job,
            log_index,
            fully_propagated: prop.fully_propagated,
            propagated_count: prop.propagated_count,
            total_nodes: prop.total_nodes,
        })
    }

    /// Create a release.
    pub async fn put_release(&self, release: Release) -> Result<WriteResult<Release>> {
        let resp = self
            .raft
            .write(ClusterCommand::PutRelease(release))
            .await?;

        let release = match resp {
            ClusterResponse::Release(r) => r,
            ClusterResponse::Error(e) => anyhow::bail!("Failed to put release: {}", e),
            _ => anyhow::bail!("Unexpected response from Raft"),
        };

        let log_index = self.current_commit_index().await;
        let prop = self.await_propagation(log_index).await;

        Ok(WriteResult {
            value: release,
            log_index,
            fully_propagated: prop.fully_propagated,
            propagated_count: prop.propagated_count,
            total_nodes: prop.total_nodes,
        })
    }

    /// Update release status.
    pub async fn update_release_status(
        &self,
        release_id: uuid::Uuid,
        status: ReleaseStatus,
    ) -> Result<WriteResult<Release>> {
        let resp = self
            .raft
            .write(ClusterCommand::UpdateReleaseStatus {
                release_id,
                status,
            })
            .await?;

        let release = match resp {
            ClusterResponse::Release(r) => r,
            ClusterResponse::Error(e) => anyhow::bail!("{}", e),
            _ => anyhow::bail!("Unexpected response from Raft"),
        };

        let log_index = self.current_commit_index().await;
        let prop = self.await_propagation(log_index).await;

        Ok(WriteResult {
            value: release,
            log_index,
            fully_propagated: prop.fully_propagated,
            propagated_count: prop.propagated_count,
            total_nodes: prop.total_nodes,
        })
    }

    /// Drain a node (set ineligible + emit event).
    pub async fn drain_node(&self, node_id: NodeId) -> Result<()> {
        let resp = self
            .raft
            .write(ClusterCommand::DrainNode { node_id })
            .await?;
        match resp {
            ClusterResponse::Ok => Ok(()),
            ClusterResponse::Error(e) => anyhow::bail!("{}", e),
            _ => Ok(()),
        }
    }

    /// Set node scheduling eligibility.
    pub async fn set_node_eligibility(
        &self,
        node_id: NodeId,
        eligible: bool,
    ) -> Result<()> {
        let resp = self
            .raft
            .write(ClusterCommand::SetNodeEligibility { node_id, eligible })
            .await?;
        match resp {
            ClusterResponse::Ok => Ok(()),
            ClusterResponse::Error(e) => anyhow::bail!("{}", e),
            _ => Ok(()),
        }
    }

    /// Create a source.
    pub async fn put_source(&self, source: Source) -> Result<WriteResult<Source>> {
        let resp = self
            .raft
            .write(ClusterCommand::PutSource(source))
            .await?;

        let source = match resp {
            ClusterResponse::Source(s) => s,
            ClusterResponse::Error(e) => anyhow::bail!("Failed to put source: {}", e),
            _ => anyhow::bail!("Unexpected response from Raft"),
        };

        let log_index = self.current_commit_index().await;
        let prop = self.await_propagation(log_index).await;

        Ok(WriteResult {
            value: source,
            log_index,
            fully_propagated: prop.fully_propagated,
            propagated_count: prop.propagated_count,
            total_nodes: prop.total_nodes,
        })
    }

    /// Update source status, revision, error, and managed jobs.
    pub async fn update_source(
        &self,
        source_id: uuid::Uuid,
        status: SourceStatus,
        last_rev: Option<String>,
        last_error: Option<String>,
        managed_jobs: Option<std::collections::HashMap<String, String>>,
    ) -> Result<WriteResult<Source>> {
        let resp = self
            .raft
            .write(ClusterCommand::UpdateSource {
                source_id,
                status,
                last_rev,
                last_error,
                managed_jobs,
            })
            .await?;

        let source = match resp {
            ClusterResponse::Source(s) => s,
            ClusterResponse::Error(e) => anyhow::bail!("{}", e),
            _ => anyhow::bail!("Unexpected response from Raft"),
        };

        let log_index = self.current_commit_index().await;
        let prop = self.await_propagation(log_index).await;

        Ok(WriteResult {
            value: source,
            log_index,
            fully_propagated: prop.fully_propagated,
            propagated_count: prop.propagated_count,
            total_nodes: prop.total_nodes,
        })
    }

    /// Delete a source.
    pub async fn delete_source(&self, source_id: uuid::Uuid) -> Result<()> {
        let resp = self
            .raft
            .write(ClusterCommand::DeleteSource { source_id })
            .await?;
        match resp {
            ClusterResponse::Ok => Ok(()),
            ClusterResponse::Error(e) => anyhow::bail!("{}", e),
            _ => Ok(()),
        }
    }

    // ── Propagation tracking ──

    /// Get the current commit index from Raft metrics.
    async fn current_commit_index(&self) -> u64 {
        let metrics = self.raft.raft.metrics().borrow().clone();
        metrics
            .last_applied
            .map(|l| l.index)
            .unwrap_or(0)
    }

    /// Wait until all nodes in the cluster have applied up to `target_index`.
    ///
    /// Returns propagation status. A write is only considered fully complete
    /// when ALL nodes have confirmed application of the write.
    async fn await_propagation(&self, target_index: u64) -> PropagationStatus {
        let deadline = tokio::time::Instant::now() + self.propagation_timeout;
        let mut interval = tokio::time::interval(Duration::from_millis(100));

        loop {
            interval.tick().await;

            let status = self.check_propagation(target_index).await;

            if status.fully_propagated {
                debug!(
                    log_index = target_index,
                    nodes = status.total_nodes,
                    "Write fully propagated to all nodes"
                );
                return status;
            }

            if tokio::time::Instant::now() >= deadline {
                warn!(
                    log_index = target_index,
                    propagated = status.propagated_count,
                    total = status.total_nodes,
                    "Write propagation timed out — not all nodes confirmed"
                );
                return status;
            }
        }
    }

    /// Check current propagation status for a given log index.
    async fn check_propagation(&self, target_index: u64) -> PropagationStatus {
        let metrics = self.raft.raft.metrics().borrow().clone();

        // Count voters and learners from replication state
        let replication = metrics.replication;

        match replication {
            Some(ref rep) => {
                let total = rep.len() + 1; // +1 for the leader itself
                let leader_applied = metrics
                    .last_applied
                    .map(|l| l.index)
                    .unwrap_or(0);

                let mut propagated = if leader_applied >= target_index {
                    1 // Leader has applied
                } else {
                    0
                };

                for (_node_id, log_id_opt) in rep.iter() {
                    if let Some(log_id) = log_id_opt {
                        if log_id.index >= target_index {
                            propagated += 1;
                        }
                    }
                }

                PropagationStatus {
                    fully_propagated: propagated >= total,
                    propagated_count: propagated,
                    total_nodes: total,
                }
            }
            None => {
                // Single node — propagation is immediate
                PropagationStatus {
                    fully_propagated: true,
                    propagated_count: 1,
                    total_nodes: 1,
                }
            }
        }
    }
}

struct PropagationStatus {
    fully_propagated: bool,
    propagated_count: usize,
    total_nodes: usize,
}
