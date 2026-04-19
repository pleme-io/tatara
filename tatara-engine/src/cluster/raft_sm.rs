use openraft::anyerror::AnyError;
use openraft::storage::RaftStateMachine;
use openraft::{
    Entry, EntryPayload, LogId, OptionalSend, RaftSnapshotBuilder, Snapshot, SnapshotMeta,
    StorageError, StorageIOError, StoredMembership,
};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::sync::Arc;
use tokio::sync::RwLock;

use tatara_core::cluster::types::{
    ClusterCommand, ClusterResponse, ClusterState, JobVersionEntry, NodeId,
};
use tatara_core::domain::event::{Event, EventKind};
use tatara_core::domain::job::{JobSpec, JobStatus};
use tatara_core::domain::source::SourceStatus;

openraft::declare_raft_types!(
    pub TypeConfig:
        D = ClusterCommand,
        R = ClusterResponse,
        Node = openraft::BasicNode,
        NodeId = NodeId,
        Entry = Entry<TypeConfig>,
        SnapshotData = Cursor<Vec<u8>>,
);

fn io_read_sm<E: std::error::Error + 'static>(e: &E) -> StorageError<NodeId> {
    StorageIOError::<NodeId>::read_state_machine(AnyError::new(e)).into()
}

fn io_read_snap<E: std::error::Error + 'static>(e: &E) -> StorageError<NodeId> {
    StorageIOError::<NodeId>::read_snapshot(None, AnyError::new(e)).into()
}

/// Raft state machine backed by in-memory ClusterState.
pub struct StateMachine {
    state: Arc<RwLock<StateMachineData>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StateMachineData {
    pub last_applied_log: Option<LogId<NodeId>>,
    pub last_membership: StoredMembership<NodeId, openraft::BasicNode>,
    pub cluster_state: ClusterState,
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(StateMachineData::default())),
        }
    }

    pub fn state(&self) -> Arc<RwLock<StateMachineData>> {
        self.state.clone()
    }
}

impl RaftSnapshotBuilder<TypeConfig> for StateMachine {
    async fn build_snapshot(&mut self) -> Result<Snapshot<TypeConfig>, StorageError<NodeId>> {
        let data = self.state.read().await;
        let bytes = serde_json::to_vec(&*data).map_err(|e| io_read_sm(&e))?;

        let last_applied = data.last_applied_log;
        let membership = data.last_membership.clone();

        let snapshot_id = format!(
            "{}-{}",
            last_applied.map(|l| l.index).unwrap_or(0),
            chrono::Utc::now().timestamp()
        );

        Ok(Snapshot {
            meta: SnapshotMeta {
                last_log_id: last_applied,
                last_membership: membership,
                snapshot_id,
            },
            snapshot: Box::new(Cursor::new(bytes)),
        })
    }
}

impl RaftStateMachine<TypeConfig> for StateMachine {
    type SnapshotBuilder = Self;

    async fn applied_state(
        &mut self,
    ) -> Result<
        (
            Option<LogId<NodeId>>,
            StoredMembership<NodeId, openraft::BasicNode>,
        ),
        StorageError<NodeId>,
    > {
        let data = self.state.read().await;
        Ok((data.last_applied_log, data.last_membership.clone()))
    }

    async fn apply<I>(&mut self, entries: I) -> Result<Vec<ClusterResponse>, StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry<TypeConfig>> + OptionalSend,
    {
        let mut responses = Vec::new();
        let mut data = self.state.write().await;

        for entry in entries {
            data.last_applied_log = Some(entry.log_id);

            if let EntryPayload::Membership(ref membership) = entry.payload {
                data.last_membership =
                    StoredMembership::new(Some(entry.log_id), membership.clone());
                responses.push(ClusterResponse::Ok);
                continue;
            }

            let resp = if let EntryPayload::Normal(cmd) = entry.payload {
                apply_command(&mut data.cluster_state, cmd)
            } else {
                ClusterResponse::Ok
            };

            responses.push(resp);
        }

        Ok(responses)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        StateMachine {
            state: self.state.clone(),
        }
    }

    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<Cursor<Vec<u8>>>, StorageError<NodeId>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<NodeId, openraft::BasicNode>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<NodeId>> {
        let bytes = snapshot.into_inner();
        let new_data: StateMachineData =
            serde_json::from_slice(&bytes).map_err(|e| io_read_snap(&e))?;

        let mut data = self.state.write().await;
        *data = new_data;
        data.last_applied_log = meta.last_log_id;
        data.last_membership = meta.last_membership.clone();

        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<TypeConfig>>, StorageError<NodeId>> {
        let data = self.state.read().await;

        if data.last_applied_log.is_none() {
            return Ok(None);
        }

        let bytes = serde_json::to_vec(&*data).map_err(|e| io_read_sm(&e))?;

        let snapshot_id = format!(
            "{}-snap",
            data.last_applied_log.map(|l| l.index).unwrap_or(0)
        );

        Ok(Some(Snapshot {
            meta: SnapshotMeta {
                last_log_id: data.last_applied_log,
                last_membership: data.last_membership.clone(),
                snapshot_id,
            },
            snapshot: Box::new(Cursor::new(bytes)),
        }))
    }
}

fn apply_command(state: &mut ClusterState, cmd: ClusterCommand) -> ClusterResponse {
    match cmd {
        ClusterCommand::PutJob(job) => {
            let job_clone = job.clone();

            // Save version history snapshot
            let spec = JobSpec {
                id: job.id.clone(),
                job_type: job.job_type.clone(),
                groups: job.groups.clone(),
                constraints: job.constraints.clone(),
                meta: job.meta.clone(),
            };
            let entry = JobVersionEntry {
                version: job.version,
                spec,
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
                    "job_id": &job.id,
                    "version": job.version,
                    "job_type": &job.job_type,
                }),
            ));

            state.jobs.insert(job.id.clone(), job);
            ClusterResponse::Job(job_clone)
        }
        ClusterCommand::UpdateJobStatus { job_id, status } => {
            if let Some(job) = state.jobs.get_mut(&job_id) {
                let old_status = job.status.clone();
                job.status = status.clone();

                // Auto-increment version on status change
                if old_status != status {
                    job.version += 1;

                    // Emit event
                    let kind = match &status {
                        JobStatus::Dead => EventKind::JobStopped,
                        _ => EventKind::JobUpdated,
                    };
                    state.events.push(Event::new(
                        kind,
                        serde_json::json!({
                            "job_id": &job_id,
                            "old_status": &old_status,
                            "new_status": &status,
                            "version": job.version,
                        }),
                    ));
                }

                ClusterResponse::Job(job.clone())
            } else {
                ClusterResponse::Error(format!("Job not found: {}", job_id))
            }
        }
        ClusterCommand::PutAllocation(alloc) => {
            let alloc_clone = alloc.clone();

            // Emit event
            state.events.push(Event::new(
                EventKind::AllocationPlaced,
                serde_json::json!({
                    "alloc_id": alloc.id.to_string(),
                    "job_id": &alloc.job_id,
                    "node_id": &alloc.node_id,
                    "group": &alloc.group_name,
                }),
            ));

            state.allocations.insert(alloc.id, alloc);
            ClusterResponse::Allocation(alloc_clone)
        }
        ClusterCommand::UpdateAllocation {
            alloc_id,
            state: alloc_state,
            task_states,
        } => {
            if let Some(alloc) = state.allocations.get_mut(&alloc_id) {
                let old_state = alloc.state.clone();
                alloc.state = alloc_state.clone();
                alloc.task_states = task_states;

                // Emit event based on state transition
                if old_state != alloc_state {
                    let kind = match &alloc_state {
                        tatara_core::domain::allocation::AllocationState::Running => {
                            EventKind::AllocationStarted
                        }
                        tatara_core::domain::allocation::AllocationState::Failed => {
                            EventKind::AllocationFailed
                        }
                        tatara_core::domain::allocation::AllocationState::Complete => {
                            EventKind::AllocationCompleted
                        }
                        _ => EventKind::AllocationPlaced,
                    };
                    state.events.push(Event::new(
                        kind,
                        serde_json::json!({
                            "alloc_id": alloc_id.to_string(),
                            "job_id": &alloc.job_id,
                            "old_state": &old_state,
                            "new_state": &alloc_state,
                        }),
                    ));
                }

                ClusterResponse::Allocation(alloc.clone())
            } else {
                ClusterResponse::Error(format!("Allocation not found: {}", alloc_id))
            }
        }
        ClusterCommand::RegisterNode(meta) => {
            state.events.push(Event::new(
                EventKind::NodeJoined,
                serde_json::json!({
                    "node_id": meta.node_id,
                    "hostname": &meta.hostname,
                }),
            ));
            state.nodes.insert(meta.node_id, meta);
            ClusterResponse::Ok
        }
        ClusterCommand::RemoveNode(node_id) => {
            state.events.push(Event::new(
                EventKind::NodeLeft,
                serde_json::json!({ "node_id": node_id }),
            ));
            state.nodes.remove(&node_id);
            ClusterResponse::Ok
        }
        ClusterCommand::AdvertiseChunk { hash, node_id } => {
            state.data_index.entry(hash).or_default().push(node_id);
            ClusterResponse::Ok
        }
        ClusterCommand::RemoveChunkAdvertisement { hash, node_id } => {
            if let Some(holders) = state.data_index.get_mut(&hash) {
                holders.retain(|&id| id != node_id);
                if holders.is_empty() {
                    state.data_index.remove(&hash);
                }
            }
            ClusterResponse::Ok
        }

        // ── New commands ──
        ClusterCommand::EmitEvent(event) => {
            state.events.push(event);
            ClusterResponse::Ok
        }

        ClusterCommand::RollbackJob { job_id, version } => {
            let history = state.job_history.get(&job_id);
            let target = history.and_then(|h| h.iter().find(|e| e.version == version));

            match target {
                Some(entry) => {
                    if let Some(job) = state.jobs.get_mut(&job_id) {
                        job.groups = entry.spec.groups.clone();
                        job.constraints = entry.spec.constraints.clone();
                        job.meta = entry.spec.meta.clone();
                        job.version += 1;
                        job.status = JobStatus::Pending; // Re-schedule

                        state.events.push(Event::new(
                            EventKind::JobUpdated,
                            serde_json::json!({
                                "job_id": &job_id,
                                "rolled_back_to": version,
                                "new_version": job.version,
                            }),
                        ));

                        ClusterResponse::Job(job.clone())
                    } else {
                        ClusterResponse::Error(format!("Job not found: {}", job_id))
                    }
                }
                None => ClusterResponse::Error(format!(
                    "Version {} not found for job {}",
                    version, job_id
                )),
            }
        }

        ClusterCommand::PutRelease(release) => {
            let release_clone = release.clone();
            state.releases.insert(release.id, release);
            ClusterResponse::Release(release_clone)
        }

        ClusterCommand::UpdateReleaseStatus { release_id, status } => {
            if let Some(release) = state.releases.get_mut(&release_id) {
                release.status = status;
                ClusterResponse::Release(release.clone())
            } else {
                ClusterResponse::Error(format!("Release not found: {}", release_id))
            }
        }

        ClusterCommand::DrainNode { node_id } => {
            if let Some(node) = state.nodes.get_mut(&node_id) {
                node.eligible = false;
                state.events.push(Event::new(
                    EventKind::NodeDraining,
                    serde_json::json!({
                        "node_id": node_id,
                        "hostname": &node.hostname,
                    }),
                ));
                ClusterResponse::Ok
            } else {
                ClusterResponse::Error(format!("Node not found: {}", node_id))
            }
        }

        ClusterCommand::SetNodeEligibility { node_id, eligible } => {
            if let Some(node) = state.nodes.get_mut(&node_id) {
                node.eligible = eligible;
                ClusterResponse::Ok
            } else {
                ClusterResponse::Error(format!("Node not found: {}", node_id))
            }
        }

        // ── Sources ──
        ClusterCommand::PutSource(source) => {
            let source_clone = source.clone();
            state.events.push(Event::new(
                EventKind::SourceCreated,
                serde_json::json!({
                    "source_id": source.id.to_string(),
                    "name": &source.name,
                    "flake_ref": &source.flake_ref,
                }),
            ));
            state.sources.insert(source.id, source);
            ClusterResponse::Source(source_clone)
        }

        ClusterCommand::UpdateSource {
            source_id,
            status,
            last_rev,
            last_error,
            managed_jobs,
        } => {
            if let Some(source) = state.sources.get_mut(&source_id) {
                let old_status = source.status.clone();
                source.status = status.clone();
                if let Some(rev) = last_rev {
                    source.last_rev = Some(rev);
                }
                source.last_error = last_error;
                if let Some(jobs) = managed_jobs {
                    source.managed_jobs = jobs;
                }
                source.last_reconciled_at = Some(chrono::Utc::now());

                // Emit appropriate event
                let kind = match &status {
                    SourceStatus::Ready => EventKind::SourceReconciled,
                    SourceStatus::Failed => EventKind::SourceFailed,
                    SourceStatus::Suspended if old_status != SourceStatus::Suspended => {
                        EventKind::SourceSuspended
                    }
                    SourceStatus::Pending if old_status == SourceStatus::Suspended => {
                        EventKind::SourceResumed
                    }
                    _ => EventKind::SourceReconciled,
                };
                state.events.push(Event::new(
                    kind,
                    serde_json::json!({
                        "source_id": source_id.to_string(),
                        "name": &source.name,
                        "status": &status,
                    }),
                ));

                ClusterResponse::Source(source.clone())
            } else {
                ClusterResponse::Error(format!("Source not found: {}", source_id))
            }
        }

        ClusterCommand::DeleteSource { source_id } => {
            if let Some(source) = state.sources.remove(&source_id) {
                state.events.push(Event::new(
                    EventKind::SourceReconciled,
                    serde_json::json!({
                        "source_id": source_id.to_string(),
                        "name": &source.name,
                        "action": "deleted",
                    }),
                ));
                ClusterResponse::Ok
            } else {
                ClusterResponse::Error(format!("Source not found: {}", source_id))
            }
        }

        // ── Distributed state machine commands ──
        ClusterCommand::ProposeAllocations {
            expected_generation,
            allocations,
            job_status_updates,
        } => {
            if state.scheduling_generation != expected_generation {
                return ClusterResponse::Error(format!(
                    "scheduling generation conflict: expected {}, current {}",
                    expected_generation, state.scheduling_generation
                ));
            }
            state.scheduling_generation += 1;
            for alloc in allocations {
                state.desired_allocations.insert(alloc.alloc_id, alloc);
            }
            for (job_id, status) in job_status_updates {
                if let Some(job) = state.jobs.get_mut(&job_id) {
                    job.status = status;
                }
            }
            ClusterResponse::Ok
        }

        ClusterCommand::SetDesiredAllocation(desired) => {
            state.desired_allocations.insert(desired.alloc_id, desired);
            ClusterResponse::Ok
        }

        ClusterCommand::RemoveDesiredAllocation { alloc_id } => {
            state.desired_allocations.remove(&alloc_id);
            ClusterResponse::Ok
        }

        ClusterCommand::ReportObservation {
            node_id,
            alloc_id,
            phase,
            observation_seq,
        } => {
            let observed = tatara_core::domain::lifecycle::ObservedAllocationState {
                alloc_id,
                node_id: format!("{node_id}"),
                phase,
                observed_at: chrono::Utc::now(),
                observation_seq,
            };
            state.observed_allocations.insert(alloc_id, observed);
            ClusterResponse::Ok
        }

        ClusterCommand::BatchObservations {
            node_id,
            observations,
            observation_seq,
        } => {
            for (alloc_id, phase) in observations {
                let observed = tatara_core::domain::lifecycle::ObservedAllocationState {
                    alloc_id,
                    node_id: format!("{}", node_id),
                    phase,
                    observed_at: chrono::Utc::now(),
                    observation_seq,
                };
                state.observed_allocations.insert(alloc_id, observed);
            }
            ClusterResponse::Ok
        }

        ClusterCommand::ReportNodePhase { node_id, phase } => {
            state.node_phases.insert(node_id, phase);
            ClusterResponse::Ok
        }

        ClusterCommand::Heartbeat {
            node_id,
            timestamp,
            allocation_summary: _,
        } => {
            if let Some(node) = state.nodes.get_mut(&node_id) {
                // Update the node's last known activity
                node.joined_at = timestamp; // reuse field for last heartbeat
            }
            ClusterResponse::Ok
        }
    }
}
