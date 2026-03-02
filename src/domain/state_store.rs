use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;
use uuid::Uuid;

use super::allocation::Allocation;
use super::job::Job;
use super::node::Node;
use super::source::Source;

#[derive(Debug, Serialize, Deserialize, Default)]
struct StateSnapshot {
    jobs: HashMap<String, Job>,
    allocations: HashMap<Uuid, Allocation>,
    nodes: HashMap<String, Node>,
    #[serde(default)]
    sources: HashMap<Uuid, Source>,
}

pub struct StateStore {
    dir: PathBuf,
    state: RwLock<StateSnapshot>,
}

impl StateStore {
    pub async fn new(dir: &Path) -> Result<Self> {
        tokio::fs::create_dir_all(dir)
            .await
            .context("Failed to create state directory")?;

        let state_file = dir.join("state.json");
        let state = if state_file.exists() {
            let data = tokio::fs::read_to_string(&state_file)
                .await
                .context("Failed to read state file")?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            StateSnapshot::default()
        };

        Ok(Self {
            dir: dir.to_path_buf(),
            state: RwLock::new(state),
        })
    }

    pub async fn put_job(&self, job: Job) -> Result<()> {
        let mut state = self.state.write().await;
        state.jobs.insert(job.id.clone(), job);
        self.persist(&state).await
    }

    pub async fn get_job(&self, id: &str) -> Option<Job> {
        let state = self.state.read().await;
        state.jobs.get(id).cloned()
    }

    pub async fn list_jobs(&self) -> Vec<Job> {
        let state = self.state.read().await;
        state.jobs.values().cloned().collect()
    }

    pub async fn update_job<F>(&self, id: &str, f: F) -> Result<Option<Job>>
    where
        F: FnOnce(&mut Job),
    {
        let mut state = self.state.write().await;
        if let Some(job) = state.jobs.get_mut(id) {
            f(job);
            let job = job.clone();
            self.persist(&state).await?;
            Ok(Some(job))
        } else {
            Ok(None)
        }
    }

    pub async fn put_allocation(&self, alloc: Allocation) -> Result<()> {
        let mut state = self.state.write().await;
        state.allocations.insert(alloc.id, alloc);
        self.persist(&state).await
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

    pub async fn update_allocation<F>(&self, id: &Uuid, f: F) -> Result<Option<Allocation>>
    where
        F: FnOnce(&mut Allocation),
    {
        let mut state = self.state.write().await;
        if let Some(alloc) = state.allocations.get_mut(id) {
            f(alloc);
            let alloc = alloc.clone();
            self.persist(&state).await?;
            Ok(Some(alloc))
        } else {
            Ok(None)
        }
    }

    pub async fn put_node(&self, node: Node) -> Result<()> {
        let mut state = self.state.write().await;
        state.nodes.insert(node.id.clone(), node);
        self.persist(&state).await
    }

    pub async fn get_node(&self, id: &str) -> Option<Node> {
        let state = self.state.read().await;
        state.nodes.get(id).cloned()
    }

    pub async fn list_nodes(&self) -> Vec<Node> {
        let state = self.state.read().await;
        state.nodes.values().cloned().collect()
    }

    pub async fn put_source(&self, source: Source) -> Result<()> {
        let mut state = self.state.write().await;
        state.sources.insert(source.id, source);
        self.persist(&state).await
    }

    pub async fn get_source(&self, id: &Uuid) -> Option<Source> {
        let state = self.state.read().await;
        state.sources.get(id).cloned()
    }

    pub async fn get_source_by_name(&self, name: &str) -> Option<Source> {
        let state = self.state.read().await;
        state.sources.values().find(|s| s.name == name).cloned()
    }

    pub async fn list_sources(&self) -> Vec<Source> {
        let state = self.state.read().await;
        state.sources.values().cloned().collect()
    }

    pub async fn update_source<F>(&self, id: &Uuid, f: F) -> Result<Option<Source>>
    where
        F: FnOnce(&mut Source),
    {
        let mut state = self.state.write().await;
        if let Some(source) = state.sources.get_mut(id) {
            f(source);
            let source = source.clone();
            self.persist(&state).await?;
            Ok(Some(source))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_source(&self, id: &Uuid) -> Result<()> {
        let mut state = self.state.write().await;
        state.sources.remove(id);
        self.persist(&state).await
    }

    async fn persist(&self, state: &StateSnapshot) -> Result<()> {
        let data = serde_json::to_string_pretty(state)
            .context("Failed to serialize state")?;

        let tmp = self.dir.join("state.json.tmp");
        let target = self.dir.join("state.json");

        tokio::fs::write(&tmp, &data)
            .await
            .context("Failed to write temporary state file")?;
        tokio::fs::rename(&tmp, &target)
            .await
            .context("Failed to rename state file")?;

        Ok(())
    }
}
