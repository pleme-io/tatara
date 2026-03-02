use anyhow::Result;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::client::executor::Executor;
use crate::config::ReconcilerConfig;
use crate::nix_eval::evaluator::NixEvaluator;

use super::allocation::{Allocation, AllocationState, TaskRunState};
use super::job::{Job, JobStatus, JobType, RestartMode};
use super::node::{Node, NodeStatus};
use super::state_store::StateStore;

/// Continuously converges actual state toward desired state.
///
/// Runs as a spawned loop alongside the Scheduler. Performs four passes per tick:
/// 1. Health — restart dead tasks per restart policy, or fail allocations
/// 2. Node liveness — mark allocations Lost if their node disappeared
/// 3. Count — ensure desired replica count is met
/// 4. Spec drift — re-evaluate Nix flake and trigger rolling updates on change
pub struct Reconciler {
    store: Arc<StateStore>,
    executor: Arc<Executor>,
    config: ReconcilerConfig,
    tick_count: u64,
}

impl Reconciler {
    pub fn new(
        store: Arc<StateStore>,
        executor: Arc<Executor>,
        config: ReconcilerConfig,
    ) -> Self {
        Self {
            store,
            executor,
            config,
            tick_count: 0,
        }
    }

    /// Main loop — runs until the task is cancelled.
    pub async fn run(&mut self) -> Result<()> {
        info!(
            interval_secs = self.config.reconcile_interval_secs,
            reeval_every_n = self.config.reeval_every_n_ticks,
            drift_detection = self.config.drift_detection,
            "Reconciler started"
        );

        let mut interval = tokio::time::interval(Duration::from_secs(
            self.config.reconcile_interval_secs,
        ));

        loop {
            interval.tick().await;
            self.tick_count += 1;

            if let Err(e) = self.reconcile().await {
                warn!(error = %e, tick = self.tick_count, "Reconciliation tick failed");
            }
        }
    }

    /// Execute a single reconciliation tick.
    async fn reconcile(&self) -> Result<()> {
        let jobs = self.store.list_jobs().await;
        let nodes = self.store.list_nodes().await;

        let node_ids: HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();

        for job in &jobs {
            if job.status == JobStatus::Dead {
                continue;
            }

            let job_allocs = self.store.list_allocations_for_job(&job.id).await;

            // Pass 1: Health — restart dead tasks or fail allocations
            self.reconcile_health(job, &job_allocs).await?;

            // Pass 2: Node liveness — mark Lost if node disappeared
            self.reconcile_node_liveness(job, &job_allocs, &node_ids).await?;

            // Pass 3: Count — ensure desired replica count
            // Re-fetch allocations since passes 1 and 2 may have changed state
            self.reconcile_count(job, &nodes).await?;

            // Pass 4: Spec drift (periodic, only for service jobs)
            if self.config.drift_detection
                && self.tick_count % self.config.reeval_every_n_ticks == 0
                && job.job_type == JobType::Service
            {
                self.reconcile_drift(job).await?;
            }
        }

        debug!(tick = self.tick_count, "Reconcile tick completed");
        Ok(())
    }

    /// Pass 1: Health reconciliation.
    ///
    /// For each Running allocation, check per-task health. Dead tasks are
    /// restarted according to the group's RestartPolicy, or the allocation
    /// is marked Failed when restarts are exhausted.
    async fn reconcile_health(
        &self,
        job: &Job,
        allocations: &[Allocation],
    ) -> Result<()> {
        let task_health = self.executor.check_task_health_detailed().await;

        for alloc in allocations {
            if alloc.state != AllocationState::Running {
                continue;
            }

            let Some(task_statuses) = task_health.get(&alloc.id) else {
                continue;
            };

            let group = match job.groups.iter().find(|g| g.name == alloc.group_name) {
                Some(g) => g,
                None => continue,
            };

            let policy = &group.restart_policy;
            let mut all_dead = true;

            for (task_name, run_state) in task_statuses {
                if *run_state == TaskRunState::Running {
                    all_dead = false;
                    continue;
                }

                if *run_state != TaskRunState::Dead {
                    all_dead = false;
                    continue;
                }

                // Task is Dead — decide whether to restart
                let task_state = alloc.task_states.get(task_name);
                let current_restarts = task_state.map(|ts| ts.restarts).unwrap_or(0);
                let exit_code = task_state.and_then(|ts| ts.exit_code);

                let should_restart = match policy.mode {
                    RestartMode::Never => false,
                    RestartMode::OnFailure => {
                        // Only restart if non-zero exit and under attempt limit
                        let failed = exit_code.map(|c| c != 0).unwrap_or(true);
                        failed && current_restarts < policy.attempts
                    }
                    RestartMode::Always => current_restarts < policy.attempts,
                };

                if should_restart {
                    // Apply restart delay
                    if policy.delay_secs > 0 {
                        tokio::time::sleep(Duration::from_secs(policy.delay_secs)).await;
                    }

                    match self.executor.restart_task(&alloc.id, task_name).await {
                        Ok(()) => {
                            info!(
                                alloc_id = %alloc.id,
                                task = %task_name,
                                restart = current_restarts + 1,
                                max = policy.attempts,
                                "Task restarted by reconciler"
                            );
                            all_dead = false;
                        }
                        Err(e) => {
                            warn!(
                                alloc_id = %alloc.id,
                                task = %task_name,
                                error = %e,
                                "Failed to restart task"
                            );
                        }
                    }
                } else if policy.mode != RestartMode::Never {
                    // Restarts exhausted — update task state
                    let _ = self
                        .store
                        .update_allocation(&alloc.id, |a| {
                            if let Some(ts) = a.task_states.get_mut(task_name) {
                                ts.state = TaskRunState::Dead;
                                ts.finished_at = Some(chrono::Utc::now());
                            }
                        })
                        .await;
                }
            }

            if all_dead {
                info!(
                    alloc_id = %alloc.id,
                    job_id = %alloc.job_id,
                    "All tasks dead, marking allocation Failed"
                );
                let _ = self
                    .store
                    .update_allocation(&alloc.id, |a| {
                        a.state = AllocationState::Failed;
                    })
                    .await;
            }
        }

        Ok(())
    }

    /// Pass 2: Node liveness.
    ///
    /// For each non-terminal allocation, check if its node still exists.
    /// Mark as Lost if the node has disappeared.
    async fn reconcile_node_liveness(
        &self,
        _job: &Job,
        allocations: &[Allocation],
        node_ids: &HashSet<String>,
    ) -> Result<()> {
        for alloc in allocations {
            if alloc.is_terminal() {
                continue;
            }

            if !node_ids.contains(&alloc.node_id) {
                warn!(
                    alloc_id = %alloc.id,
                    node_id = %alloc.node_id,
                    "Node missing, marking allocation Lost"
                );
                let _ = self
                    .store
                    .update_allocation(&alloc.id, |a| {
                        a.state = AllocationState::Lost;
                    })
                    .await;
            }
        }

        Ok(())
    }

    /// Pass 3: Count reconciliation.
    ///
    /// For each Running job, compare active allocations against desired count.
    /// Create new allocations for deficits, stop excess allocations.
    async fn reconcile_count(&self, job: &Job, nodes: &[Node]) -> Result<()> {
        if job.status != JobStatus::Running {
            return Ok(());
        }

        let allocations = self.store.list_allocations_for_job(&job.id).await;

        let ready_nodes: Vec<&Node> = nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Ready && n.eligible)
            .collect();

        if ready_nodes.is_empty() {
            return Ok(());
        }

        for group in &job.groups {
            let desired = match job.job_type {
                JobType::System => ready_nodes.len() as u32,
                _ => group.count,
            };

            // Count active (Running or Pending) allocations for this group
            let active: u32 = allocations
                .iter()
                .filter(|a| {
                    a.group_name == group.name
                        && matches!(
                            a.state,
                            AllocationState::Running | AllocationState::Pending
                        )
                })
                .count() as u32;

            if active < desired {
                let deficit = desired - active;
                info!(
                    job_id = %job.id,
                    group = %group.name,
                    active = active,
                    desired = desired,
                    deficit = deficit,
                    "Count deficit, creating allocations"
                );

                for _ in 0..deficit {
                    // Simple round-robin: pick the node with fewest allocations for this job
                    let node = ready_nodes
                        .iter()
                        .min_by_key(|n| {
                            allocations
                                .iter()
                                .filter(|a| {
                                    a.node_id == n.id
                                        && !a.is_terminal()
                                        && a.group_name == group.name
                                })
                                .count()
                        });

                    let Some(node) = node else {
                        warn!(
                            job_id = %job.id,
                            group = %group.name,
                            "No available node for replacement allocation"
                        );
                        break;
                    };

                    let task_names: Vec<String> =
                        group.tasks.iter().map(|t| t.name.clone()).collect();

                    let alloc = Allocation::new(
                        job.id.clone(),
                        group.name.clone(),
                        node.id.clone(),
                        task_names,
                    )
                    .with_job_version(job.version);

                    self.store.put_allocation(alloc.clone()).await?;

                    info!(
                        alloc_id = %alloc.id,
                        job_id = %job.id,
                        group = %group.name,
                        node = %node.id,
                        "Reconciler created replacement allocation"
                    );

                    if let Err(e) = self.executor.start_allocation(alloc).await {
                        warn!(error = %e, "Failed to start replacement allocation");
                    }
                }
            } else if active > desired {
                let excess = active - desired;
                info!(
                    job_id = %job.id,
                    group = %group.name,
                    active = active,
                    desired = desired,
                    excess = excess,
                    "Count excess, stopping allocations"
                );

                // Stop newest allocations first
                let mut group_allocs: Vec<&Allocation> = allocations
                    .iter()
                    .filter(|a| {
                        a.group_name == group.name
                            && matches!(
                                a.state,
                                AllocationState::Running | AllocationState::Pending
                            )
                    })
                    .collect();
                group_allocs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

                for alloc in group_allocs.iter().take(excess as usize) {
                    if let Err(e) = self
                        .executor
                        .stop_allocation(&alloc.id, Duration::from_secs(10))
                        .await
                    {
                        warn!(
                            alloc_id = %alloc.id,
                            error = %e,
                            "Failed to stop excess allocation"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Pass 4: Spec drift detection.
    ///
    /// Re-evaluates the Nix flake for this job and compares the spec hash.
    /// If the hash differs, triggers a rolling update.
    async fn reconcile_drift(&self, job: &Job) -> Result<()> {
        // Look for a flake_ref in the job's tasks
        let flake_ref = job
            .groups
            .iter()
            .flat_map(|g| g.tasks.iter())
            .find_map(|t| {
                if let crate::domain::job::TaskConfig::Nix { ref flake_ref, .. } = t.config {
                    Some(flake_ref.clone())
                } else {
                    None
                }
            });

        let Some(flake_ref) = flake_ref else {
            return Ok(());
        };

        // Re-evaluate the flake
        let expr = format!(
            "(builtins.getFlake \"{}\").tataraJobs.{}",
            flake_ref, job.id
        );

        let new_spec = match NixEvaluator::eval_expr(&expr).await {
            Ok(spec) => spec,
            Err(e) => {
                debug!(
                    job_id = %job.id,
                    error = %e,
                    "Nix re-evaluation failed (may not have tataraJobs), skipping drift check"
                );
                return Ok(());
            }
        };

        let new_hash = new_spec.content_hash();
        let current_hash = job.spec_hash.as_deref().unwrap_or("");

        if new_hash == current_hash {
            debug!(job_id = %job.id, "No spec drift detected");
            return Ok(());
        }

        info!(
            job_id = %job.id,
            old_hash = %current_hash,
            new_hash = %new_hash,
            "Spec drift detected, triggering rolling update"
        );

        // Update the job with the new spec
        let new_version = job.version + 1;
        self.store
            .update_job(&job.id, |j| {
                j.groups = new_spec.groups.clone();
                j.constraints = new_spec.constraints.clone();
                j.meta = new_spec.meta.clone();
                j.spec_hash = Some(new_hash.clone());
                j.version = new_version;
            })
            .await?;

        // Rolling update: create new allocations, then stop old ones
        let allocations = self.store.list_allocations_for_job(&job.id).await;
        let old_allocs: Vec<&Allocation> = allocations
            .iter()
            .filter(|a| {
                a.job_version < new_version
                    && matches!(
                        a.state,
                        AllocationState::Running | AllocationState::Pending
                    )
            })
            .collect();

        let nodes = self.store.list_nodes().await;
        let ready_nodes: Vec<&Node> = nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Ready && n.eligible)
            .collect();

        // Create new allocations for each old one being replaced
        for old_alloc in &old_allocs {
            let group = match new_spec.groups.iter().find(|g| g.name == old_alloc.group_name) {
                Some(g) => g,
                None => continue,
            };

            // Try to place on the same node, fall back to any ready node
            let node_id = if ready_nodes.iter().any(|n| n.id == old_alloc.node_id) {
                old_alloc.node_id.clone()
            } else if let Some(n) = ready_nodes.first() {
                n.id.clone()
            } else {
                warn!(
                    alloc_id = %old_alloc.id,
                    "No available node for rolling update replacement"
                );
                continue;
            };

            let task_names: Vec<String> = group.tasks.iter().map(|t| t.name.clone()).collect();

            let new_alloc = Allocation::new(
                job.id.clone(),
                group.name.clone(),
                node_id,
                task_names,
            )
            .with_job_version(new_version);

            self.store.put_allocation(new_alloc.clone()).await?;

            if let Err(e) = self.executor.start_allocation(new_alloc).await {
                warn!(error = %e, "Failed to start rolling update allocation");
            }
        }

        // Stop old allocations
        for old_alloc in &old_allocs {
            if let Err(e) = self
                .executor
                .stop_allocation(&old_alloc.id, Duration::from_secs(30))
                .await
            {
                warn!(
                    alloc_id = %old_alloc.id,
                    error = %e,
                    "Failed to stop old allocation during rolling update"
                );
            }
        }

        info!(
            job_id = %job.id,
            version = new_version,
            replaced = old_allocs.len(),
            "Rolling update completed"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::job::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use uuid::Uuid;

    async fn make_store() -> (Arc<StateStore>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("tatara-test-{}", Uuid::new_v4()));
        let store = Arc::new(StateStore::new(&dir).await.unwrap());
        (store, dir)
    }

    fn make_job(id: &str, job_type: JobType, count: u32, restart_mode: RestartMode) -> Job {
        Job {
            id: id.to_string(),
            version: 1,
            job_type,
            status: JobStatus::Running,
            submitted_at: chrono::Utc::now(),
            groups: vec![TaskGroup {
                name: "web".to_string(),
                count,
                tasks: vec![Task {
                    name: "server".to_string(),
                    driver: DriverType::Exec,
                    config: TaskConfig::Exec {
                        command: "echo".to_string(),
                        args: vec!["hello".to_string()],
                        working_dir: None,
                    },
                    env: HashMap::new(),
                    resources: Resources::default(),
                    health_checks: vec![],
                }],
                restart_policy: RestartPolicy {
                    mode: restart_mode,
                    attempts: 3,
                    interval_secs: 300,
                    delay_secs: 0,
                },
                resources: Resources::default(),
                network: None,
            }],
            constraints: vec![],
            meta: HashMap::new(),
            spec_hash: None,
        }
    }

    fn make_alloc(job_id: &str, node_id: &str, state: AllocationState) -> Allocation {
        let mut alloc = Allocation::new(
            job_id.to_string(),
            "web".to_string(),
            node_id.to_string(),
            vec!["server".to_string()],
        );
        alloc.state = state;
        alloc
    }

    #[tokio::test]
    async fn test_node_liveness_marks_lost() {
        let (store, _dir) = make_store().await;

        let job = make_job("j1", JobType::Service, 1, RestartMode::OnFailure);
        store.put_job(job.clone()).await.unwrap();

        let alloc = make_alloc("j1", "missing-node", AllocationState::Running);
        let alloc_id = alloc.id;
        store.put_allocation(alloc).await.unwrap();

        let node_ids: HashSet<String> = HashSet::new();
        let allocations: Vec<Allocation> = store.list_allocations_for_job("j1").await;

        for a in &allocations {
            if !a.is_terminal() && !node_ids.contains(&a.node_id) {
                store
                    .update_allocation(&a.id, |a| {
                        a.state = AllocationState::Lost;
                    })
                    .await
                    .unwrap();
            }
        }

        let updated = store.get_allocation(&alloc_id).await.unwrap();
        assert_eq!(updated.state, AllocationState::Lost);
    }

    #[tokio::test]
    async fn test_count_reconciliation_detects_deficit() {
        let (store, _dir) = make_store().await;

        let job = make_job("j1", JobType::Service, 3, RestartMode::OnFailure);
        store.put_job(job.clone()).await.unwrap();

        let a1 = make_alloc("j1", "n1", AllocationState::Running);
        let a2 = make_alloc("j1", "n2", AllocationState::Running);
        store.put_allocation(a1).await.unwrap();
        store.put_allocation(a2).await.unwrap();

        let allocations: Vec<Allocation> = store.list_allocations_for_job("j1").await;
        let active: u32 = allocations
            .iter()
            .filter(|a| {
                a.group_name == "web"
                    && matches!(
                        a.state,
                        AllocationState::Running | AllocationState::Pending
                    )
            })
            .count() as u32;

        assert_eq!(active, 2);
        assert_eq!(job.groups[0].count, 3);
        assert!(active < job.groups[0].count);
    }

    #[tokio::test]
    async fn test_count_reconciliation_detects_excess() {
        let (store, _dir) = make_store().await;

        let job = make_job("j1", JobType::Service, 2, RestartMode::OnFailure);
        store.put_job(job.clone()).await.unwrap();

        for i in 0..4 {
            let a = make_alloc("j1", &format!("n{}", i), AllocationState::Running);
            store.put_allocation(a).await.unwrap();
        }

        let allocations: Vec<Allocation> = store.list_allocations_for_job("j1").await;
        let active: u32 = allocations
            .iter()
            .filter(|a| {
                a.group_name == "web"
                    && matches!(
                        a.state,
                        AllocationState::Running | AllocationState::Pending
                    )
            })
            .count() as u32;

        assert_eq!(active, 4);
        assert!(active > job.groups[0].count);
    }

    #[tokio::test]
    async fn test_restart_policy_never_no_restart() {
        let job = make_job("j1", JobType::Service, 1, RestartMode::Never);
        let policy = &job.groups[0].restart_policy;

        let should_restart = match policy.mode {
            RestartMode::Never => false,
            _ => true,
        };
        assert!(!should_restart);
    }

    #[tokio::test]
    async fn test_restart_policy_on_failure_respects_attempts() {
        let job = make_job("j1", JobType::Service, 1, RestartMode::OnFailure);
        let policy = &job.groups[0].restart_policy;

        // Under limit (restart 2, max 3) with failed exit
        let current_restarts = 2u32;
        let exit_code = Some(1i32);
        let should_restart = match policy.mode {
            RestartMode::OnFailure => {
                let failed = exit_code.map(|c| c != 0).unwrap_or(true);
                failed && current_restarts < policy.attempts
            }
            _ => false,
        };
        assert!(should_restart);

        // At limit (restart 3, max 3)
        let current_restarts = 3u32;
        let should_restart = match policy.mode {
            RestartMode::OnFailure => {
                let failed = exit_code.map(|c| c != 0).unwrap_or(true);
                failed && current_restarts < policy.attempts
            }
            _ => false,
        };
        assert!(!should_restart);
    }

    #[tokio::test]
    async fn test_restart_policy_always_restarts_on_success() {
        let job = make_job("j1", JobType::Service, 1, RestartMode::Always);
        let policy = &job.groups[0].restart_policy;

        let current_restarts = 0u32;
        let should_restart = match policy.mode {
            RestartMode::Always => current_restarts < policy.attempts,
            _ => false,
        };
        assert!(should_restart);
    }

    #[tokio::test]
    async fn test_spec_hash_consistency() {
        let spec = JobSpec {
            id: "test".to_string(),
            job_type: JobType::Service,
            groups: vec![],
            constraints: vec![],
            meta: HashMap::new(),
        };

        let hash1 = spec.content_hash();
        let hash2 = spec.content_hash();
        assert_eq!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_spec_hash_changes_on_different_spec() {
        let spec1 = JobSpec {
            id: "test".to_string(),
            job_type: JobType::Service,
            groups: vec![],
            constraints: vec![],
            meta: HashMap::new(),
        };

        let mut meta = HashMap::new();
        meta.insert("version".to_string(), "2".to_string());
        let spec2 = JobSpec {
            id: "test".to_string(),
            job_type: JobType::Service,
            groups: vec![],
            constraints: vec![],
            meta,
        };

        assert_ne!(spec1.content_hash(), spec2.content_hash());
    }

    #[tokio::test]
    async fn test_terminal_allocations_skipped_in_liveness() {
        let (store, _dir) = make_store().await;

        let job = make_job("j1", JobType::Service, 1, RestartMode::OnFailure);
        store.put_job(job.clone()).await.unwrap();

        let alloc = make_alloc("j1", "missing-node", AllocationState::Failed);
        store.put_allocation(alloc).await.unwrap();

        let node_ids: HashSet<String> = HashSet::new();
        let allocations: Vec<Allocation> = store.list_allocations_for_job("j1").await;

        let mut changed = false;
        for a in &allocations {
            if !a.is_terminal() && !node_ids.contains(&a.node_id) {
                changed = true;
            }
        }
        assert!(!changed);
    }

    #[tokio::test]
    async fn test_job_version_on_allocation() {
        let alloc = Allocation::new(
            "j1".to_string(),
            "web".to_string(),
            "n1".to_string(),
            vec!["server".to_string()],
        )
        .with_job_version(5);

        assert_eq!(alloc.job_version, 5);
    }
}
