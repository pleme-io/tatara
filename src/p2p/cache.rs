use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::chunk::{Chunk, ChunkManifest};

/// Local content-addressed cache. Stores chunks on disk, indexed by BLAKE3 hash.
///
/// Layout:
///   <cache_dir>/
///     manifests/<root_hash>.json     — chunk manifests
///     chunks/<first2>/<hash>         — raw chunk data (sharded by first 2 hex chars)
pub struct DataCache {
    dir: PathBuf,
    /// In-memory index: hash → true if we have the chunk locally.
    index: RwLock<HashSet<String>>,
    /// Manifest index: root_hash → manifest.
    manifests: RwLock<HashMap<String, ChunkManifest>>,
}

impl DataCache {
    pub async fn new(dir: &Path) -> Result<Self> {
        tokio::fs::create_dir_all(dir.join("manifests")).await?;
        tokio::fs::create_dir_all(dir.join("chunks")).await?;

        let cache = Self {
            dir: dir.to_path_buf(),
            index: RwLock::new(HashSet::new()),
            manifests: RwLock::new(HashMap::new()),
        };

        cache.rebuild_index().await?;
        Ok(cache)
    }

    /// Store a chunk locally.
    pub async fn put_chunk(&self, chunk: &Chunk) -> Result<()> {
        if !chunk.verify() {
            anyhow::bail!("Chunk verification failed: {}", chunk.hash);
        }

        let path = self.chunk_path(&chunk.hash);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&path, &chunk.data).await?;
        self.index.write().await.insert(chunk.hash.clone());

        debug!(hash = %chunk.hash, size = chunk.size, "Stored chunk");
        Ok(())
    }

    /// Retrieve a chunk from local cache.
    pub async fn get_chunk(&self, hash: &str) -> Result<Option<Chunk>> {
        if !self.has_chunk(hash).await {
            return Ok(None);
        }

        let path = self.chunk_path(hash);
        let data = tokio::fs::read(&path).await?;

        let chunk = Chunk {
            hash: hash.to_string(),
            data,
            size: 0, // Will be filled
        };

        // Verify
        if !chunk.verify() {
            warn!(hash = hash, "Cached chunk failed verification — removing");
            let _ = tokio::fs::remove_file(&path).await;
            self.index.write().await.remove(hash);
            return Ok(None);
        }

        Ok(Some(Chunk {
            size: chunk.data.len(),
            ..chunk
        }))
    }

    /// Check if we have a chunk.
    pub async fn has_chunk(&self, hash: &str) -> bool {
        self.index.read().await.contains(hash)
    }

    /// Store a manifest.
    pub async fn put_manifest(&self, manifest: &ChunkManifest) -> Result<()> {
        let path = self
            .dir
            .join("manifests")
            .join(format!("{}.json", manifest.root_hash));
        let data = serde_json::to_string_pretty(manifest)?;
        tokio::fs::write(&path, data).await?;
        self.manifests
            .write()
            .await
            .insert(manifest.root_hash.clone(), manifest.clone());

        debug!(
            root_hash = %manifest.root_hash,
            chunks = manifest.chunks.len(),
            content_type = %manifest.content_type,
            "Stored manifest"
        );
        Ok(())
    }

    /// Get a manifest.
    pub async fn get_manifest(&self, root_hash: &str) -> Option<ChunkManifest> {
        self.manifests.read().await.get(root_hash).cloned()
    }

    /// List all chunk hashes we have.
    pub async fn local_chunks(&self) -> Vec<String> {
        self.index.read().await.iter().cloned().collect()
    }

    /// List all manifests we have.
    pub async fn local_manifests(&self) -> Vec<ChunkManifest> {
        self.manifests.read().await.values().cloned().collect()
    }

    /// Check how many chunks of a manifest we have locally.
    pub async fn manifest_completeness(&self, manifest: &ChunkManifest) -> (usize, usize) {
        let index = self.index.read().await;
        let have = manifest
            .chunks
            .iter()
            .filter(|h| index.contains(*h))
            .count();
        (have, manifest.chunks.len())
    }

    /// Store complete data: split into chunks, store all, store manifest.
    pub async fn store_data(
        &self,
        data: &[u8],
        content_type: &str,
        label: &str,
    ) -> Result<ChunkManifest> {
        let (manifest, chunks) = ChunkManifest::from_data(data, content_type, label);

        for chunk in &chunks {
            self.put_chunk(chunk).await?;
        }
        self.put_manifest(&manifest).await?;

        info!(
            root_hash = %manifest.root_hash,
            chunks = chunks.len(),
            total_size = manifest.total_size,
            "Data stored in p2p cache"
        );

        Ok(manifest)
    }

    /// Retrieve complete data from cache, given a manifest.
    pub async fn retrieve_data(&self, manifest: &ChunkManifest) -> Result<Option<Vec<u8>>> {
        let mut chunks = Vec::with_capacity(manifest.chunks.len());

        for hash in &manifest.chunks {
            match self.get_chunk(hash).await? {
                Some(chunk) => chunks.push(chunk),
                None => return Ok(None), // Missing chunk
            }
        }

        manifest
            .reassemble(&chunks)
            .map(Some)
            .map_err(|e| anyhow::anyhow!("Reassembly failed: {}", e))
    }

    /// Cache size in bytes (approximate).
    pub async fn size_bytes(&self) -> u64 {
        let mut total = 0u64;
        let chunks_dir = self.dir.join("chunks");

        if let Ok(mut entries) = tokio::fs::read_dir(&chunks_dir).await {
            while let Ok(Some(shard)) = entries.next_entry().await {
                if let Ok(mut shard_entries) = tokio::fs::read_dir(shard.path()).await {
                    while let Ok(Some(entry)) = shard_entries.next_entry().await {
                        if let Ok(meta) = entry.metadata().await {
                            total += meta.len();
                        }
                    }
                }
            }
        }

        total
    }

    fn chunk_path(&self, hash: &str) -> PathBuf {
        let shard = &hash[..2.min(hash.len())];
        self.dir.join("chunks").join(shard).join(hash)
    }

    async fn rebuild_index(&self) -> Result<()> {
        let mut index = self.index.write().await;
        let mut manifests = self.manifests.write().await;

        // Scan chunk directory
        let chunks_dir = self.dir.join("chunks");
        if let Ok(mut entries) = tokio::fs::read_dir(&chunks_dir).await {
            while let Ok(Some(shard)) = entries.next_entry().await {
                if let Ok(mut shard_entries) = tokio::fs::read_dir(shard.path()).await {
                    while let Ok(Some(entry)) = shard_entries.next_entry().await {
                        if let Some(hash) = entry.file_name().to_str() {
                            index.insert(hash.to_string());
                        }
                    }
                }
            }
        }

        // Scan manifest directory
        let manifests_dir = self.dir.join("manifests");
        if let Ok(mut entries) = tokio::fs::read_dir(&manifests_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        if let Ok(data) = tokio::fs::read_to_string(entry.path()).await {
                            if let Ok(manifest) = serde_json::from_str::<ChunkManifest>(&data) {
                                manifests.insert(manifest.root_hash.clone(), manifest);
                            }
                        }
                    }
                }
            }
        }

        info!(
            chunks = index.len(),
            manifests = manifests.len(),
            "P2P cache index rebuilt"
        );

        Ok(())
    }
}
