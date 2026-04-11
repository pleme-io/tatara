//! Adapter between ClusterStore (Raft-backed) and the Evaluator/Scheduler.
//!
//! The Evaluator was originally written against StateStore (local JSON file).
//! This adapter wraps ClusterStore to provide the same read interface,
//! ensuring the scheduler reads from Raft-replicated state instead of
//! divergent local state.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::cluster::store::ClusterStore;
use tatara_core::cluster::types::NodeMeta;
use tatara_core::domain::allocation::Allocation;
use tatara_core::domain::job::{Job, JobStatus};
use tatara_core::domain::node::{Node, NodeStatus};

/// Wraps ClusterStore to provide StateStore-compatible reads for the Evaluator.
pub struct ClusterStoreAdapter {
    store: Arc<ClusterStore>,
}

impl ClusterStoreAdapter {
    pub fn new(store: Arc<ClusterStore>) -> Self {
        Self { store }
    }

    /// List all jobs (from Raft state, eventually consistent).
    pub async fn list_jobs(&self) -> Vec<Job> {
        self.store.list_jobs().await
    }

    /// List all nodes as `Node` type (converted from `NodeMeta`).
    pub async fn list_nodes(&self) -> Vec<Node> {
        self.store
            .list_nodes()
            .await
            .into_iter()
            .map(node_meta_to_node)
            .collect()
    }

    /// Get the current scheduling generation.
    pub async fn scheduling_generation(&self) -> u64 {
        let state = self.store.state().await;
        state.scheduling_generation
    }

    /// Submit a job through Raft.
    pub async fn put_job(&self, job: Job) -> Result<()> {
        self.store.put_job(job).await?;
        Ok(())
    }

    /// Update job status through Raft.
    pub async fn update_job_status(&self, job_id: &str, status: JobStatus) -> Result<()> {
        self.store.update_job_status(job_id, status).await?;
        Ok(())
    }

    /// Submit an allocation through Raft.
    pub async fn put_allocation(&self, alloc: Allocation) -> Result<()> {
        self.store.put_allocation(alloc).await?;
        Ok(())
    }

    /// Check if this node is the Raft leader.
    pub async fn is_leader(&self) -> bool {
        self.store.is_leader().await
    }
}

/// Convert NodeMeta (cluster type) to Node (domain type) for the Evaluator.
fn node_meta_to_node(meta: NodeMeta) -> Node {
    let mut attributes = HashMap::new();
    attributes.insert("os".to_string(), meta.os.clone());
    attributes.insert("arch".to_string(), meta.arch.clone());
    attributes.insert("hostname".to_string(), meta.hostname.clone());

    Node {
        id: format!("{}", meta.node_id),
        address: meta.http_addr.clone(),
        status: if meta.eligible {
            NodeStatus::Ready
        } else {
            NodeStatus::Draining
        },
        eligible: meta.eligible,
        total_resources: meta.total_resources,
        available_resources: meta.available_resources,
        attributes,
        drivers: meta.drivers,
        last_heartbeat: meta.joined_at,
        allocations: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tatara_core::cluster::types::NodeRoles;
    use tatara_core::domain::job::Resources;

    #[test]
    fn test_node_meta_conversion() {
        let meta = NodeMeta {
            node_id: 42,
            hostname: "test-host".to_string(),
            http_addr: "127.0.0.1:4646".to_string(),
            gossip_addr: "127.0.0.1:5679".to_string(),
            raft_addr: "127.0.0.1:4649".to_string(),
            os: "darwin".to_string(),
            arch: "aarch64".to_string(),
            roles: NodeRoles::default(),
            drivers: vec![],
            total_resources: Resources {
                cpu_mhz: 4000,
                memory_mb: 8192,
            },
            available_resources: Resources {
                cpu_mhz: 3000,
                memory_mb: 6144,
            },
            allocations_running: 2,
            joined_at: Utc::now(),
            version: "0.2.0".to_string(),
            eligible: true,
            wireguard_pubkey: None,
            tunnel_address: None,
        };

        let node = node_meta_to_node(meta);
        assert_eq!(node.id, "42");
        assert_eq!(node.status, NodeStatus::Ready);
        assert!(node.eligible);
        assert_eq!(node.attributes["arch"], "aarch64");
        assert_eq!(node.total_resources.cpu_mhz, 4000);
    }
}
