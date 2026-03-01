use anyhow::{Context, Result};
use openraft::{BasicNode, Config, Raft};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use super::network::RaftHttpNetwork;
use super::raft_log::LogStore;
use super::raft_sm::{StateMachine, StateMachineData, TypeConfig};
use super::types::{ClusterCommand, ClusterResponse, NodeId};

pub type TataraRaft = Raft<TypeConfig>;

/// A running Raft node.
pub struct RaftCluster {
    pub raft: TataraRaft,
    pub state_machine: Arc<RwLock<StateMachineData>>,
    pub network: RaftHttpNetwork,
    node_id: NodeId,
}

impl RaftCluster {
    /// Initialize a new Raft node.
    pub async fn start(
        node_id: NodeId,
        _raft_addr: &str,
        data_dir: &PathBuf,
    ) -> Result<Self> {
        let config = Config {
            heartbeat_interval: 500,
            election_timeout_min: 1500,
            election_timeout_max: 3000,
            max_in_snapshot_log_to_keep: 500,
            ..Default::default()
        };
        let config = Arc::new(config.validate().context("Invalid Raft config")?);

        let log_path = data_dir.join("raft.redb");
        let log_store = LogStore::new(&log_path)
            .context("Failed to open Raft log store")?;

        let state_machine = StateMachine::new();
        let sm_data = state_machine.state();

        let network = RaftHttpNetwork::new();

        let raft = Raft::new(node_id, config, network.clone(), log_store, state_machine)
            .await
            .context("Failed to initialize Raft")?;

        info!(node_id = node_id, "Raft node initialized");

        Ok(Self {
            raft,
            state_machine: sm_data,
            network,
            node_id,
        })
    }

    /// Bootstrap a single-node cluster (first node).
    pub async fn bootstrap_single(&self, raft_addr: &str) -> Result<()> {
        let mut members = BTreeMap::new();
        members.insert(
            self.node_id,
            BasicNode {
                addr: raft_addr.to_string(),
            },
        );

        self.raft
            .initialize(members)
            .await
            .map_err(|e| anyhow::anyhow!("Raft bootstrap failed: {}", e))?;

        info!(node_id = self.node_id, "Bootstrapped single-node Raft cluster");
        Ok(())
    }

    /// Add a new voter to the cluster.
    pub async fn add_voter(&self, node_id: NodeId, addr: &str) -> Result<()> {
        let node = BasicNode {
            addr: addr.to_string(),
        };

        // First add as learner
        self.raft
            .add_learner(node_id, node, true)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to add learner: {}", e))?;

        // Then promote to voter
        let members = self.current_members().await?;
        let mut new_members = members;
        new_members.insert(node_id);

        self.raft
            .change_membership(new_members, false)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to change membership: {}", e))?;

        info!(node_id = node_id, addr = addr, "Added voter to Raft cluster");
        Ok(())
    }

    /// Add a learner (non-voting replica).
    pub async fn add_learner(&self, node_id: NodeId, addr: &str) -> Result<()> {
        let node = BasicNode {
            addr: addr.to_string(),
        };

        self.raft
            .add_learner(node_id, node, true)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to add learner: {}", e))?;

        info!(node_id = node_id, "Added learner to Raft cluster");
        Ok(())
    }

    /// Write a command through Raft (goes to leader).
    pub async fn write(&self, cmd: ClusterCommand) -> Result<ClusterResponse> {
        let resp = self
            .raft
            .client_write(cmd)
            .await
            .map_err(|e| anyhow::anyhow!("Raft write failed: {}", e))?;

        Ok(resp.data)
    }

    /// Read cluster state (linearizable — confirms leadership first).
    pub async fn read_state(&self) -> Result<Arc<RwLock<StateMachineData>>> {
        // Ensure we're reading from a confirmed leader
        self.raft
            .ensure_linearizable()
            .await
            .map_err(|e| anyhow::anyhow!("Linearizable read failed: {}", e))?;

        Ok(self.state_machine.clone())
    }

    /// Read cluster state (eventually consistent — local read, no leader check).
    pub async fn read_local(&self) -> Arc<RwLock<StateMachineData>> {
        self.state_machine.clone()
    }

    /// Synchronous local read — returns the shared state reference directly.
    /// Used by components that need the reference at construction time.
    pub fn read_local_sync(&self) -> Arc<RwLock<StateMachineData>> {
        self.state_machine.clone()
    }

    /// Check if this node is the current leader.
    pub async fn is_leader(&self) -> bool {
        self.raft.ensure_linearizable().await.is_ok()
    }

    /// Get the current leader's node ID.
    pub async fn current_leader(&self) -> Option<NodeId> {
        self.raft.current_leader().await
    }

    /// Get current Raft membership.
    async fn current_members(&self) -> Result<std::collections::BTreeSet<NodeId>> {
        let metrics = self.raft.metrics().borrow().clone();
        Ok(metrics
            .membership_config
            .membership()
            .voter_ids()
            .collect())
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }
}
