use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::cache::DataCache;
use super::chunk::{Chunk, ChunkManifest};
use crate::cluster::gossip::GossipCluster;

/// Handles peer-to-peer chunk transfer between nodes.
///
/// Protocol:
/// 1. Node stores data locally → advertises chunk hashes via gossip
/// 2. Node needing data gets manifest → queries gossip for chunk holders
/// 3. Fetches missing chunks from any holder (parallel, random order)
/// 4. Verifies and stores each chunk locally
/// 5. Once complete, advertises its own possession via gossip
///
/// This is BitTorrent-style: content-addressed, swarming, no central tracker.
pub struct TransferEngine {
    cache: Arc<DataCache>,
    gossip: Arc<GossipCluster>,
    http_client: reqwest::Client,
}

impl TransferEngine {
    pub fn new(cache: Arc<DataCache>, gossip: Arc<GossipCluster>) -> Self {
        Self {
            cache,
            gossip,
            http_client: reqwest::Client::new(),
        }
    }

    /// Publish data to the swarm: store locally and advertise via gossip.
    pub async fn publish(
        &self,
        data: &[u8],
        content_type: &str,
        label: &str,
    ) -> Result<ChunkManifest> {
        let manifest = self.cache.store_data(data, content_type, label).await?;

        // Advertise each chunk via gossip
        for hash in &manifest.chunks {
            self.gossip.advertise_chunk(hash).await;
        }

        info!(
            root_hash = %manifest.root_hash,
            chunks = manifest.chunks.len(),
            "Published data to swarm"
        );

        Ok(manifest)
    }

    /// Fetch complete data for a manifest. Downloads missing chunks from peers.
    pub async fn fetch(&self, manifest: &ChunkManifest) -> Result<Vec<u8>> {
        let (have, total) = self.cache.manifest_completeness(manifest).await;

        if have == total {
            // Already have everything
            return self
                .cache
                .retrieve_data(manifest)
                .await?
                .context("Failed to reassemble despite having all chunks");
        }

        info!(
            root_hash = %manifest.root_hash,
            have = have,
            total = total,
            "Fetching missing chunks from peers"
        );

        // Find and fetch missing chunks
        let mut missing: Vec<String> = Vec::new();
        for hash in &manifest.chunks {
            if !self.cache.has_chunk(hash).await {
                missing.push(hash.clone());
            }
        }

        // Fetch missing chunks in parallel (up to 8 concurrent)
        let semaphore = Arc::new(tokio::sync::Semaphore::new(8));
        let mut handles = Vec::new();

        for hash in missing {
            let sem = semaphore.clone();
            let gossip = self.gossip.clone();
            let cache = self.cache.clone();
            let client = self.http_client.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                fetch_chunk_from_peers(&client, &gossip, &cache, &hash).await
            });

            handles.push(handle);
        }

        // Wait for all fetches
        let mut failures = 0;
        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    warn!(error = %e, "Chunk fetch failed");
                    failures += 1;
                }
                Err(e) => {
                    warn!(error = %e, "Chunk fetch task panicked");
                    failures += 1;
                }
            }
        }

        if failures > 0 {
            anyhow::bail!(
                "Failed to fetch {} chunks for manifest {}",
                failures,
                manifest.root_hash
            );
        }

        // Advertise newly acquired chunks
        for hash in &manifest.chunks {
            self.gossip.advertise_chunk(hash).await;
        }

        // Reassemble
        self.cache
            .retrieve_data(manifest)
            .await?
            .context("Failed to reassemble after fetching all chunks")
    }

    /// Serve a chunk to a requesting peer (called by HTTP handler).
    pub async fn serve_chunk(&self, hash: &str) -> Result<Option<Chunk>> {
        self.cache.get_chunk(hash).await
    }

    /// Serve a manifest to a requesting peer.
    pub async fn serve_manifest(&self, root_hash: &str) -> Option<ChunkManifest> {
        self.cache.get_manifest(root_hash).await
    }
}

async fn fetch_chunk_from_peers(
    client: &reqwest::Client,
    gossip: &GossipCluster,
    cache: &DataCache,
    hash: &str,
) -> Result<()> {
    let holders = gossip.find_chunk_holders(hash);

    if holders.is_empty() {
        anyhow::bail!("No holders found for chunk {}", hash);
    }

    // Try each holder (random order would be better, but sequential is fine for now)
    for holder_addr in &holders {
        let url = format!("http://{}/p2p/chunks/{}", holder_addr, hash);

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let data = resp.bytes().await?;
                let chunk = Chunk {
                    hash: hash.to_string(),
                    data: data.to_vec(),
                    size: data.len(),
                };

                if chunk.verify() {
                    cache.put_chunk(&chunk).await?;
                    debug!(hash = hash, from = %holder_addr, "Fetched chunk from peer");
                    return Ok(());
                } else {
                    warn!(
                        hash = hash,
                        from = %holder_addr,
                        "Chunk from peer failed verification"
                    );
                }
            }
            Ok(resp) => {
                debug!(
                    hash = hash,
                    from = %holder_addr,
                    status = %resp.status(),
                    "Peer returned non-success for chunk"
                );
            }
            Err(e) => {
                debug!(
                    hash = hash,
                    from = %holder_addr,
                    error = %e,
                    "Failed to contact peer for chunk"
                );
            }
        }
    }

    anyhow::bail!(
        "Failed to fetch chunk {} from any of {} holders",
        hash,
        holders.len()
    )
}
