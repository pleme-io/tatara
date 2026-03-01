use openraft::error::{InstallSnapshotError, RPCError, RaftError};
use openraft::network::{RPCOption, RaftNetwork, RaftNetworkFactory};
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use openraft::BasicNode;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::raft_sm::TypeConfig;
use super::types::NodeId;

/// Network layer for Raft — HTTP-based RPCs between nodes.
#[derive(Clone)]
pub struct RaftHttpNetwork {
    client: reqwest::Client,
    /// Map of node_id → HTTP base address (e.g., "http://10.0.0.1:4648")
    peers: Arc<RwLock<HashMap<NodeId, String>>>,
}

impl RaftHttpNetwork {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn update_peer(&self, node_id: NodeId, addr: String) {
        self.peers.write().await.insert(node_id, addr);
    }

    pub async fn remove_peer(&self, node_id: &NodeId) {
        self.peers.write().await.remove(node_id);
    }

    async fn peer_addr(&self, node_id: &NodeId) -> Option<String> {
        self.peers.read().await.get(node_id).cloned()
    }
}

/// A connection to a single Raft peer.
pub struct RaftHttpConnection {
    client: reqwest::Client,
    target_addr: String,
    target_id: NodeId,
}

impl RaftNetworkFactory<TypeConfig> for RaftHttpNetwork {
    type Network = RaftHttpConnection;

    async fn new_client(&mut self, target: NodeId, node: &BasicNode) -> Self::Network {
        // Use node.addr from openraft's BasicNode, or fall back to our peer map
        let addr = if !node.addr.is_empty() {
            node.addr.clone()
        } else {
            self.peer_addr(&target)
                .await
                .unwrap_or_else(|| "http://127.0.0.1:4648".to_string())
        };

        RaftHttpConnection {
            client: self.client.clone(),
            target_addr: addr,
            target_id: target,
        }
    }
}

impl RaftNetwork<TypeConfig> for RaftHttpConnection {
    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<NodeId>, RPCError<NodeId, BasicNode, RaftError<NodeId>>> {
        let url = format!("{}/raft/append", self.target_addr);
        let resp = self
            .client
            .post(&url)
            .json(&rpc)
            .send()
            .await
            .map_err(|e| new_rpc_error(self.target_id, &e))?;

        let result: AppendEntriesResponse<NodeId> = resp
            .json()
            .await
            .map_err(|e| new_rpc_error(self.target_id, &e))?;

        Ok(result)
    }

    async fn install_snapshot(
        &mut self,
        rpc: InstallSnapshotRequest<TypeConfig>,
        _option: RPCOption,
    ) -> Result<
        InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, BasicNode, RaftError<NodeId, InstallSnapshotError>>,
    > {
        let url = format!("{}/raft/snapshot", self.target_addr);
        let resp = self
            .client
            .post(&url)
            .json(&rpc)
            .send()
            .await
            .map_err(|e| new_rpc_error_snap(self.target_id, &e))?;

        let result: InstallSnapshotResponse<NodeId> = resp
            .json()
            .await
            .map_err(|e| new_rpc_error_snap(self.target_id, &e))?;

        Ok(result)
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<NodeId>,
        _option: RPCOption,
    ) -> Result<VoteResponse<NodeId>, RPCError<NodeId, BasicNode, RaftError<NodeId>>> {
        let url = format!("{}/raft/vote", self.target_addr);
        let resp = self
            .client
            .post(&url)
            .json(&rpc)
            .send()
            .await
            .map_err(|e| new_rpc_error(self.target_id, &e))?;

        let result: VoteResponse<NodeId> = resp
            .json()
            .await
            .map_err(|e| new_rpc_error(self.target_id, &e))?;

        Ok(result)
    }
}

fn new_rpc_error(
    target: NodeId,
    e: &reqwest::Error,
) -> RPCError<NodeId, BasicNode, RaftError<NodeId>> {
    let io_err = std::io::Error::new(
        std::io::ErrorKind::ConnectionRefused,
        format!("Node {} unreachable: {}", target, e),
    );
    RPCError::Unreachable(openraft::error::Unreachable::new(&io_err))
}

fn new_rpc_error_snap(
    target: NodeId,
    e: &reqwest::Error,
) -> RPCError<NodeId, BasicNode, RaftError<NodeId, InstallSnapshotError>> {
    let io_err = std::io::Error::new(
        std::io::ErrorKind::ConnectionRefused,
        format!("Node {} unreachable: {}", target, e),
    );
    RPCError::Unreachable(openraft::error::Unreachable::new(&io_err))
}
