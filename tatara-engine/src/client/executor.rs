use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

use tatara_core::domain::allocation::{Allocation, AllocationState, TaskRunState};
use crate::cluster::store::ClusterStore;
use crate::domain::state_store::StateStore;
use crate::drivers::{DriverRegistry, TaskHandle};

struct RunningTask {
    handle: TaskHandle,
    task_name: String,
    alloc_id: Uuid,
}

pub struct Executor {
    store: Arc<StateStore>,
    drivers: Arc<DriverRegistry>,
    alloc_dir: PathBuf,
    running: RwLock<HashMap<Uuid, Vec<RunningTask>>>,
    /// Optional Raft-backed store for reporting observations to the cluster.
    /// When set, state changes are reported through Raft for cluster-wide visibility.
    cluster_store: Option<Arc<ClusterStore>>,
}

impl Executor {
    pub fn new(store: Arc<StateStore>, drivers: Arc<DriverRegistry>, alloc_dir: PathBuf) -> Self {
        Self {
            store,
            drivers,
            alloc_dir,
            running: RwLock::new(HashMap::new()),
            cluster_store: None,
        }
    }

    /// Set the cluster store for Raft-backed observation reporting.
    pub fn with_cluster_store(mut self, cluster_store: Arc<ClusterStore>) -> Self {
        self.cluster_store = Some(cluster_store);
        self
    }

    /// Report an allocation state change through Raft (if cluster store is wired).
    async fn report_to_cluster(&self, alloc_id: Uuid, state: AllocationState) {
        if let Some(ref cs) = self.cluster_store {
            if let Err(e) = cs
                .update_allocation_state(alloc_id, state.clone(), HashMap::new())
                .await
            {
                warn!(
                    alloc_id = %alloc_id,
                    state = ?state,
                    error = %e,
                    "failed to report observation to cluster"
                );
            }
        }
    }

    pub async fn start_allocation(&self, alloc: Allocation) -> Result<()> {
        let alloc_id = alloc.id;
        let job = self
            .store
            .get_job(&alloc.job_id)
            .await
            .context("Job not found for allocation")?;

        let group = job
            .groups
            .iter()
            .find(|g| g.name == alloc.group_name)
            .context("Task group not found in job")?;

        let alloc_path = self.alloc_dir.join(alloc_id.to_string());
        tokio::fs::create_dir_all(&alloc_path).await?;

        let mut tasks = Vec::new();

        for task in &group.tasks {
            let driver = self
                .drivers
                .get(&task.driver)
                .with_context(|| format!("Driver {:?} not available", task.driver))?;

            match driver.start(task, &alloc_path).await {
                Ok(handle) => {
                    info!(
                        alloc_id = %alloc_id,
                        task = %task.name,
                        driver = %driver.name(),
                        pid = ?handle.pid,
                        "Task started"
                    );

                    // Update task state to running
                    self.store
                        .update_allocation(&alloc_id, |a| {
                            if let Some(ts) = a.task_states.get_mut(&task.name) {
                                ts.state = TaskRunState::Running;
                                ts.pid = handle.pid;
                                ts.started_at = Some(handle.started_at);
                            }
                        })
                        .await?;

                    tasks.push(RunningTask {
                        handle,
                        task_name: task.name.clone(),
                        alloc_id,
                    });
                }
                Err(e) => {
                    error!(
                        alloc_id = %alloc_id,
                        task = %task.name,
                        error = %e,
                        "Failed to start task"
                    );

                    self.store
                        .update_allocation(&alloc_id, |a| {
                            if let Some(ts) = a.task_states.get_mut(&task.name) {
                                ts.state = TaskRunState::Dead;
                            }
                            a.state = AllocationState::Failed;
                        })
                        .await?;

                    return Err(e);
                }
            }
        }

        // Mark allocation as running
        self.store
            .update_allocation(&alloc_id, |a| {
                a.state = AllocationState::Running;
            })
            .await?;

        // Report to cluster for distributed visibility
        self.report_to_cluster(alloc_id, AllocationState::Running).await;

        self.running.write().await.insert(alloc_id, tasks);

        Ok(())
    }

    pub async fn stop_allocation(&self, alloc_id: &Uuid, timeout: Duration) -> Result<()> {
        let mut running = self.running.write().await;

        if let Some(tasks) = running.remove(alloc_id) {
            for rt in &tasks {
                let driver = self
                    .drivers
                    .get(&rt.handle.driver)
                    .context("Driver not found")?;

                if let Err(e) = driver.stop(&rt.handle, timeout).await {
                    warn!(
                        alloc_id = %alloc_id,
                        task = %rt.task_name,
                        error = %e,
                        "Failed to stop task"
                    );
                }
            }
        }

        self.store
            .update_allocation(alloc_id, |a| {
                a.state = AllocationState::Complete;
                for ts in a.task_states.values_mut() {
                    ts.state = TaskRunState::Dead;
                    ts.finished_at = Some(chrono::Utc::now());
                }
            })
            .await?;

        // Report completion to cluster
        self.report_to_cluster(*alloc_id, AllocationState::Complete).await;

        Ok(())
    }

    /// Check health of all running allocations. Returns dead allocations.
    pub async fn check_health(&self) -> Vec<Uuid> {
        let running = self.running.read().await;
        let mut dead = Vec::new();

        for (alloc_id, tasks) in running.iter() {
            let mut all_dead = true;

            for rt in tasks {
                if let Some(driver) = self.drivers.get(&rt.handle.driver) {
                    match driver.status(&rt.handle).await {
                        Ok(TaskRunState::Running) => {
                            all_dead = false;
                        }
                        Ok(TaskRunState::Dead) => {
                            let _ = self
                                .store
                                .update_allocation(alloc_id, |a| {
                                    if let Some(ts) = a.task_states.get_mut(&rt.task_name) {
                                        ts.state = TaskRunState::Dead;
                                        ts.finished_at = Some(chrono::Utc::now());
                                    }
                                })
                                .await;
                        }
                        _ => {}
                    }
                }
            }

            if all_dead {
                dead.push(*alloc_id);
            }
        }

        dead
    }

    /// Check health of all running allocations, returning per-task status.
    pub async fn check_task_health_detailed(
        &self,
    ) -> HashMap<Uuid, Vec<(String, TaskRunState)>> {
        let running = self.running.read().await;
        let mut result: HashMap<Uuid, Vec<(String, TaskRunState)>> = HashMap::new();

        for (alloc_id, tasks) in running.iter() {
            let mut task_states = Vec::new();

            for rt in tasks {
                let state = if let Some(driver) = self.drivers.get(&rt.handle.driver) {
                    match driver.status(&rt.handle).await {
                        Ok(s) => s,
                        Err(_) => TaskRunState::Dead,
                    }
                } else {
                    TaskRunState::Dead
                };
                task_states.push((rt.task_name.clone(), state));
            }

            result.insert(*alloc_id, task_states);
        }

        result
    }

    /// Restart a single task within a running allocation.
    pub async fn restart_task(
        &self,
        alloc_id: &Uuid,
        task_name: &str,
    ) -> Result<()> {
        let alloc = self
            .store
            .get_allocation(alloc_id)
            .await
            .context("Allocation not found")?;

        let job = self
            .store
            .get_job(&alloc.job_id)
            .await
            .context("Job not found for allocation")?;

        let group = job
            .groups
            .iter()
            .find(|g| g.name == alloc.group_name)
            .context("Task group not found in job")?;

        let task = group
            .tasks
            .iter()
            .find(|t| t.name == task_name)
            .with_context(|| format!("Task {} not found in group {}", task_name, group.name))?;

        // Stop the old process
        {
            let mut running = self.running.write().await;
            if let Some(tasks) = running.get_mut(alloc_id) {
                if let Some(rt) = tasks.iter().find(|t| t.task_name == task_name) {
                    if let Some(driver) = self.drivers.get(&rt.handle.driver) {
                        let _ = driver.stop(&rt.handle, Duration::from_secs(10)).await;
                    }
                }
                tasks.retain(|t| t.task_name != task_name);
            }
        }

        // Start the task fresh
        let alloc_path = self.alloc_dir.join(alloc_id.to_string());
        tokio::fs::create_dir_all(&alloc_path).await?;

        let driver = self
            .drivers
            .get(&task.driver)
            .with_context(|| format!("Driver {:?} not available", task.driver))?;

        let handle = driver.start(task, &alloc_path).await?;

        info!(
            alloc_id = %alloc_id,
            task = %task_name,
            pid = ?handle.pid,
            "Task restarted"
        );

        // Update task state in store
        self.store
            .update_allocation(alloc_id, |a| {
                if let Some(ts) = a.task_states.get_mut(task_name) {
                    ts.state = TaskRunState::Running;
                    ts.pid = handle.pid;
                    ts.started_at = Some(handle.started_at);
                    ts.restarts += 1;
                }
            })
            .await?;

        // Re-add to running map
        self.running
            .write()
            .await
            .entry(*alloc_id)
            .or_default()
            .push(RunningTask {
                handle,
                task_name: task_name.to_string(),
                alloc_id: *alloc_id,
            });

        Ok(())
    }

    pub async fn get_task_handle(&self, alloc_id: &Uuid, task_name: &str) -> Option<TaskHandle> {
        let running = self.running.read().await;
        running.get(alloc_id).and_then(|tasks| {
            tasks
                .iter()
                .find(|t| t.task_name == task_name)
                .map(|t| t.handle.clone())
        })
    }
}
