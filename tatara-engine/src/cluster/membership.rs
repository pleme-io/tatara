use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::gossip::GossipCluster;
use super::raft_node::RaftCluster;
use tatara_core::cluster::types::{gossip_keys, NodeMeta};

/// Bridges gossip membership with Raft consensus.
///
/// Gossip handles: failure detection, node metadata sharing, peer discovery.
/// Raft handles: consistent state mutations (job scheduling, allocation).
///
/// This reconciler watches gossip membership changes and:
/// - Adds new voter-capable nodes to Raft
/// - Removes dead nodes from Raft
/// - Updates the peer address map in the Raft network layer
pub struct MembershipReconciler {
    gossip: Arc<GossipCluster>,
    raft: Arc<RaftCluster>,
    known_peers: RwLock<HashMap<String, NodeMeta>>,
}

impl MembershipReconciler {
    pub fn new(gossip: Arc<GossipCluster>, raft: Arc<RaftCluster>) -> Self {
        Self {
            gossip,
            raft,
            known_peers: RwLock::new(HashMap::new()),
        }
    }

    /// Run the reconciliation loop.
    pub async fn run(&self) -> Result<()> {
        info!("Membership reconciler started");
        let mut interval = tokio::time::interval(Duration::from_secs(5));

        loop {
            interval.tick().await;

            if let Err(e) = self.reconcile().await {
                warn!(error = %e, "Membership reconciliation failed");
            }
        }
    }

    async fn reconcile(&self) -> Result<()> {
        let live_nodes = self.gossip.live_nodes();
        let mut current_peers = HashMap::new();

        for (chitchat_id, kvs) in &live_nodes {
            // Parse NodeMeta from gossip KV
            let meta_json = match kvs.get(gossip_keys::META) {
                Some(json) => json,
                None => continue,
            };

            let meta: NodeMeta = match serde_json::from_str(meta_json) {
                Ok(m) => m,
                Err(e) => {
                    debug!(
                        peer = %chitchat_id.node_id,
                        error = %e,
                        "Failed to parse peer metadata"
                    );
                    continue;
                }
            };

            // Update Raft network's peer address map
            self.raft
                .network
                .update_peer(meta.node_id, meta.raft_addr.clone())
                .await;

            current_peers.insert(chitchat_id.node_id.clone(), meta);
        }

        // Detect new peers that are voters
        let known = self.known_peers.read().await;
        for (id, meta) in &current_peers {
            if !known.contains_key(id) && meta.roles.voter {
                // New voter-capable node — try to add to Raft
                if self.raft.is_leader().await {
                    info!(
                        node_id = meta.node_id,
                        addr = %meta.raft_addr,
                        "Adding new voter to Raft cluster"
                    );
                    if let Err(e) = self.raft.add_voter(meta.node_id, &meta.raft_addr).await {
                        warn!(
                            node_id = meta.node_id,
                            error = %e,
                            "Failed to add voter to Raft"
                        );
                    }
                }
            }
        }

        // Detect departed peers
        let departed: Vec<String> = known
            .keys()
            .filter(|id| !current_peers.contains_key(*id))
            .cloned()
            .collect();

        for id in &departed {
            if let Some(meta) = known.get(id) {
                info!(
                    node_id = meta.node_id,
                    "Peer departed — detected via gossip"
                );
                // Don't remove from Raft immediately — let it handle via heartbeat timeout.
                // Just clean up the network peer map.
                self.raft.network.remove_peer(&meta.node_id).await;
            }
        }

        // Update known peers
        drop(known);
        *self.known_peers.write().await = current_peers;

        Ok(())
    }
}
