//! In-memory store that mirrors the ClusterStore API without requiring Raft.
//!
//! This is the "virtualized tatara" — a complete, functional implementation of
//! the tatara state machine that runs entirely in-memory. It applies the same
//! state transitions as the Raft state machine but without consensus overhead.

use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::cluster::types::{ClusterState, JobVersionEntry, NodeMeta};
use crate::domain::allocation::{Allocation, AllocationState, TaskState};
use crate::domain::event::{Event, EventKind};
use crate::domain::job::{Job, JobSpec, JobStatus};
use crate::domain::release::{Release, ReleaseStatus};

/// In-memory store that provides the same read/write interface as ClusterStore
/// but backed by a simple RwLock instead of Raft consensus.
///
/// State transitions mirror `src/cluster/raft_sm.rs` exactly.
pub struct InMemoryStore {
    state: RwLock<ClusterState>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(ClusterState::default()),
        }
    }

    /// Create a store pre-populated with initial state.
    pub fn with_state(state: ClusterState) -> Self {
        Self {
            state: RwLock::new(state),
        }
    }

    // ── Reads ──

    pub async fn get_job(&self, id: &str) -> Option<Job> {
        let state = self.state.read().await;
        state.jobs.get(id).cloned()
    }

    pub async fn list_jobs(&self) -> Vec<Job> {
        let state = self.state.read().await;
        state.jobs.values().cloned().collect()
    }

    pub async fn get_allocation(&self, id: &Uuid) -> Option<Allocation> {
        let state = self.state.read().await;
        state.allocations.get(id).cloned()
    }

    pub async fn list_allocations(&self) -> Vec<Allocation> {
        let state = self.state.read().await;
        state.allocations.values().cloned().collect()
    }

    pub async fn list_allocations_for_job(&self, job_id: &str) -> Vec<Allocation> {
        let state = self.state.read().await;
        state
            .allocations
            .values()
            .filter(|a| a.job_id == job_id)
            .cloned()
            .collect()
    }

    pub async fn list_nodes(&self) -> Vec<NodeMeta> {
        let state = self.state.read().await;
        state.nodes.values().cloned().collect()
    }

    pub async fn get_job_history(&self, job_id: &str) -> Vec<JobVersionEntry> {
        let state = self.state.read().await;
        state
            .job_history
            .get(job_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn list_events(
        &self,
        kind: Option<&EventKind>,
        since: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Vec<Event> {
        let state = self.state.read().await;
        state.events.query(kind, since).into_iter().cloned().collect()
    }

    pub async fn list_releases(&self) -> Vec<Release> {
        let state = self.state.read().await;
        state.releases.values().cloned().collect()
    }

    pub async fn get_release(&self, id: &Uuid) -> Option<Release> {
        let state = self.state.read().await;
        state.releases.get(id).cloned()
    }

    // ── Writes (mirrors raft_sm.rs apply logic) ──

    /// Submit a job. Mirrors PutJob command from raft_sm.
    pub async fn put_job(&self, job: Job) -> Result<Job> {
        let mut state = self.state.write().await;

        // Save version history snapshot
        let entry = JobVersionEntry {
            version: job.version,
            spec: JobSpec {
                id: job.id.clone(),
                job_type: job.job_type.clone(),
                groups: job.groups.clone(),
                constraints: job.constraints.clone(),
                meta: job.meta.clone(),
            },
            status: job.status.clone(),
            submitted_at: job.submitted_at,
        };
        state
            .job_history
            .entry(job.id.clone())
            .or_default()
            .push(entry);

        // Emit event
        state.events.push(Event::new(
            EventKind::JobSubmitted,
            serde_json::json!({
                "job_id": job.id,
                "version": job.version,
            }),
        ));

        state.jobs.insert(job.id.clone(), job.clone());
        Ok(job)
    }

    /// Update job status. Mirrors UpdateJobStatus command.
    pub async fn update_job_status(&self, job_id: &str, status: JobStatus) -> Result<Job> {
        let mut state = self.state.write().await;

        let job = state
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        job.status = status.clone();
        job.version += 1;
        let result = job.clone();

        // Emit event
        let kind = match status {
            JobStatus::Dead => EventKind::JobStopped,
            _ => EventKind::JobUpdated,
        };
        state.events.push(Event::new(
            kind,
            serde_json::json!({
                "job_id": job_id,
                "status": format!("{:?}", status),
            }),
        ));

        Ok(result)
    }

    /// Submit an allocation. Mirrors PutAllocation command.
    pub async fn put_allocation(&self, alloc: Allocation) -> Result<Allocation> {
        let mut state = self.state.write().await;

        state.events.push(Event::new(
            EventKind::AllocationPlaced,
            serde_json::json!({
                "alloc_id": alloc.id.to_string(),
                "job_id": alloc.job_id,
                "node_id": alloc.node_id,
            }),
        ));

        state.allocations.insert(alloc.id, alloc.clone());
        Ok(alloc)
    }

    /// Update allocation state. Mirrors UpdateAllocation command.
    pub async fn update_allocation_state(
        &self,
        alloc_id: Uuid,
        new_state: AllocationState,
        task_states: HashMap<String, TaskState>,
    ) -> Result<Allocation> {
        let mut state = self.state.write().await;

        let alloc = state
            .allocations
            .get_mut(&alloc_id)
            .ok_or_else(|| anyhow::anyhow!("Allocation not found: {}", alloc_id))?;

        let kind = match new_state {
            AllocationState::Running => EventKind::AllocationStarted,
            AllocationState::Complete => EventKind::AllocationCompleted,
            AllocationState::Failed => EventKind::AllocationFailed,
            _ => EventKind::AllocationPlaced,
        };

        alloc.state = new_state;
        alloc.task_states = task_states;
        let result = alloc.clone();

        state.events.push(Event::new(
            kind,
            serde_json::json!({
                "alloc_id": alloc_id.to_string(),
            }),
        ));

        Ok(result)
    }

    /// Register a node. Mirrors RegisterNode command.
    pub async fn register_node(&self, meta: NodeMeta) -> Result<()> {
        let mut state = self.state.write().await;

        state.events.push(Event::new(
            EventKind::NodeJoined,
            serde_json::json!({
                "node_id": meta.node_id,
                "hostname": meta.hostname,
            }),
        ));

        state.nodes.insert(meta.node_id, meta);
        Ok(())
    }

    /// Emit a raw event.
    pub async fn emit_event(&self, event: Event) {
        let mut state = self.state.write().await;
        state.events.push(event);
    }

    /// Rollback a job to a previous version. Mirrors RollbackJob command.
    pub async fn rollback_job(&self, job_id: &str, version: u64) -> Result<Job> {
        let mut state = self.state.write().await;

        let history = state
            .job_history
            .get(job_id)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        let entry = history
            .iter()
            .find(|e| e.version == version)
            .ok_or_else(|| {
                anyhow::anyhow!("Version {} not found for job {}", version, job_id)
            })?
            .clone();

        let job = state
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        job.groups = entry.spec.groups;
        job.constraints = entry.spec.constraints;
        job.meta = entry.spec.meta;
        job.version += 1;
        job.status = JobStatus::Pending;
        let result = job.clone();

        state.events.push(Event::new(
            EventKind::JobUpdated,
            serde_json::json!({
                "job_id": job_id,
                "action": "rollback",
                "target_version": version,
            }),
        ));

        Ok(result)
    }

    /// Create a release. Mirrors PutRelease command.
    pub async fn put_release(&self, release: Release) -> Result<Release> {
        let mut state = self.state.write().await;
        state.releases.insert(release.id, release.clone());
        Ok(release)
    }

    /// Update release status. Mirrors UpdateReleaseStatus command.
    pub async fn update_release_status(
        &self,
        release_id: Uuid,
        status: ReleaseStatus,
    ) -> Result<Release> {
        let mut state = self.state.write().await;

        let release = state
            .releases
            .get_mut(&release_id)
            .ok_or_else(|| anyhow::anyhow!("Release not found: {}", release_id))?;

        release.status = status;
        Ok(release.clone())
    }

    /// Drain a node (set ineligible + emit event). Mirrors DrainNode command.
    pub async fn drain_node(&self, node_id: u64) -> Result<()> {
        let mut state = self.state.write().await;

        if let Some(node) = state.nodes.get_mut(&node_id) {
            node.eligible = false;
            state.events.push(Event::new(
                EventKind::NodeDraining,
                serde_json::json!({ "node_id": node_id }),
            ));
            Ok(())
        } else {
            anyhow::bail!("Node not found: {}", node_id)
        }
    }

    /// Set node eligibility. Mirrors SetNodeEligibility command.
    pub async fn set_node_eligibility(&self, node_id: u64, eligible: bool) -> Result<()> {
        let mut state = self.state.write().await;

        if let Some(node) = state.nodes.get_mut(&node_id) {
            node.eligible = eligible;
            Ok(())
        } else {
            anyhow::bail!("Node not found: {}", node_id)
        }
    }

    /// Direct access to the state for assertions in tests.
    pub async fn state(&self) -> tokio::sync::RwLockReadGuard<'_, ClusterState> {
        self.state.read().await
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::job::{JobType, Resources, Task, TaskConfig, TaskGroup};
    use crate::testing::fixtures;

    #[tokio::test]
    async fn test_put_and_get_job() {
        let store = InMemoryStore::new();
        let job = fixtures::job("test-job");

        store.put_job(job.clone()).await.unwrap();

        let retrieved = store.get_job("test-job").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, "test-job");
    }

    #[tokio::test]
    async fn test_job_version_history() {
        let store = InMemoryStore::new();
        let job = fixtures::job("versioned-job");

        store.put_job(job).await.unwrap();
        store
            .update_job_status("versioned-job", JobStatus::Running)
            .await
            .unwrap();

        let history = store.get_job_history("versioned-job").await;
        assert_eq!(history.len(), 1); // Initial submission
        assert_eq!(history[0].version, 1);
    }

    #[tokio::test]
    async fn test_events_emitted_on_job_submit() {
        let store = InMemoryStore::new();
        let job = fixtures::job("event-job");

        store.put_job(job).await.unwrap();

        let events = store.list_events(Some(&EventKind::JobSubmitted), None).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["job_id"], "event-job");
    }

    #[tokio::test]
    async fn test_rollback_job() {
        let store = InMemoryStore::new();
        let job = fixtures::job_with_group("rollback-test", "web", 3, 500, 256);

        store.put_job(job).await.unwrap();
        store
            .update_job_status("rollback-test", JobStatus::Running)
            .await
            .unwrap();

        // Rollback to version 1
        let rolled_back = store.rollback_job("rollback-test", 1).await.unwrap();
        assert_eq!(rolled_back.status, JobStatus::Pending);
        assert_eq!(rolled_back.groups[0].count, 3);
    }

    #[tokio::test]
    async fn test_release_lifecycle() {
        let store = InMemoryStore::new();

        let mut release = Release::new(
            "myapp".to_string(),
            "github:user/myapp".to_string(),
            "job-1".to_string(),
        );
        release.status = ReleaseStatus::Active;

        let created = store.put_release(release).await.unwrap();
        assert_eq!(created.status, ReleaseStatus::Active);

        let updated = store
            .update_release_status(created.id, ReleaseStatus::Superseded)
            .await
            .unwrap();
        assert_eq!(updated.status, ReleaseStatus::Superseded);
    }

    #[tokio::test]
    async fn test_node_drain() {
        let store = InMemoryStore::new();
        let node = fixtures::node_meta(1, "test-node", 4000, 8192);

        store.register_node(node).await.unwrap();
        store.drain_node(1).await.unwrap();

        let nodes = store.list_nodes().await;
        assert!(!nodes[0].eligible);

        let events = store
            .list_events(Some(&EventKind::NodeDraining), None)
            .await;
        assert_eq!(events.len(), 1);
    }
}
