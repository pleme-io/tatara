use anyhow::{Context, Result};
use chitchat::{
    spawn_chitchat, ChitchatConfig, ChitchatHandle, ChitchatId, FailureDetectorConfig,
    NodeState, transport::UdpTransport,
};
use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::watch;
use tracing::info;

use super::types::{gossip_keys, NodeId, NodeMeta};

/// Manages gossip-based cluster membership and KV state sharing.
pub struct GossipCluster {
    handle: ChitchatHandle,
    local_id: ChitchatId,
    local_node_id: NodeId,
    /// Watch receiver for live node state — used to read current membership.
    live_watcher: watch::Receiver<BTreeMap<ChitchatId, NodeState>>,
}

impl GossipCluster {
    /// Start a gossip cluster node.
    pub async fn start(
        node_id: NodeId,
        gossip_addr: SocketAddr,
        seed_nodes: Vec<String>,
        cluster_id: &str,
        meta: &NodeMeta,
    ) -> Result<Self> {
        let chitchat_id = ChitchatId::new(
            format!("tatara-{}", node_id),
            chrono::Utc::now().timestamp() as u64,
            gossip_addr,
        );

        let config = ChitchatConfig {
            chitchat_id: chitchat_id.clone(),
            cluster_id: cluster_id.to_string(),
            gossip_interval: Duration::from_millis(500),
            listen_addr: gossip_addr,
            seed_nodes,
            failure_detector_config: FailureDetectorConfig {
                phi_threshold: 8.0,
                initial_interval: Duration::from_millis(500),
                ..Default::default()
            },
            marked_for_deletion_grace_period: Duration::from_secs(60),
            catchup_callback: None,
            extra_liveness_predicate: None,
        };

        // Initial KV pairs to advertise
        let initial_kvs = build_gossip_kvs(meta);

        let transport = UdpTransport;
        let handle = spawn_chitchat(config, initial_kvs, &transport)
            .await
            .context("Failed to start gossip cluster")?;

        // Get a watcher for live node state
        let live_watcher = {
            let chitchat = handle.chitchat();
            let guard = chitchat.lock().await;
            guard.live_nodes_watcher()
        };

        info!(
            node_id = node_id,
            gossip_addr = %gossip_addr,
            "Gossip cluster started"
        );

        Ok(Self {
            handle,
            local_id: chitchat_id,
            local_node_id: node_id,
            live_watcher,
        })
    }

    /// Update local node's gossip state.
    pub async fn update_meta(&self, meta: &NodeMeta) {
        let chitchat = self.handle.chitchat();
        let mut guard = chitchat.lock().await;
        let state = guard.self_node_state();

        let kvs = build_gossip_kvs(meta);
        for (key, value) in kvs {
            state.set(key, value);
        }
    }

    /// Advertise that we have a content-addressed chunk.
    pub async fn advertise_chunk(&self, hash: &str) {
        let chitchat = self.handle.chitchat();
        let mut guard = chitchat.lock().await;
        let state = guard.self_node_state();
        let key = format!("{}:{}", gossip_keys::DATA_HAVE, hash);
        state.set(key, "1".to_string());
    }

    /// Get all live nodes and their metadata.
    pub fn live_nodes(&self) -> Vec<(ChitchatId, HashMap<String, String>)> {
        let current = self.live_watcher.borrow();
        current
            .iter()
            .map(|(id, state)| {
                let kvs: HashMap<String, String> = state
                    .key_values()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                (id.clone(), kvs)
            })
            .collect()
    }

    /// Find nodes that have a specific content-addressed chunk.
    pub fn find_chunk_holders(&self, hash: &str) -> Vec<String> {
        let key = format!("{}:{}", gossip_keys::DATA_HAVE, hash);
        let current = self.live_watcher.borrow();

        let mut holders = Vec::new();
        for (_id, state) in current.iter() {
            if state.get(&key).is_some() {
                if let Some(addr) = state.get(gossip_keys::HTTP_ADDR) {
                    holders.push(addr.to_string());
                }
            }
        }
        holders
    }

    /// Get a clone of the live nodes watcher for membership tracking.
    pub fn live_nodes_watcher(&self) -> watch::Receiver<BTreeMap<ChitchatId, NodeState>> {
        self.live_watcher.clone()
    }

    /// Shutdown gossip.
    pub async fn shutdown(self) -> Result<()> {
        self.handle
            .shutdown()
            .await
            .map_err(|e| anyhow::anyhow!("Gossip shutdown error: {:?}", e))
    }
}

fn build_gossip_kvs(meta: &NodeMeta) -> Vec<(String, String)> {
    vec![
        (
            gossip_keys::META.to_string(),
            serde_json::to_string(meta).unwrap_or_default(),
        ),
        (gossip_keys::HTTP_ADDR.to_string(), meta.http_addr.clone()),
        (gossip_keys::RAFT_ADDR.to_string(), meta.raft_addr.clone()),
        (
            gossip_keys::ROLE.to_string(),
            format!("voter={},worker={}", meta.roles.voter, meta.roles.worker),
        ),
        (
            gossip_keys::LOAD_CPU.to_string(),
            meta.available_resources.cpu_mhz.to_string(),
        ),
        (
            gossip_keys::LOAD_MEM.to_string(),
            meta.available_resources.memory_mb.to_string(),
        ),
        (
            gossip_keys::ALLOC_COUNT.to_string(),
            meta.allocations_running.to_string(),
        ),
        (gossip_keys::VERSION.to_string(), meta.version.clone()),
        (
            gossip_keys::DRIVERS.to_string(),
            serde_json::to_string(&meta.drivers).unwrap_or_default(),
        ),
    ]
}
