use serde::{Deserialize, Serialize};

/// Default chunk size: 256 KB.
pub const DEFAULT_CHUNK_SIZE: usize = 256 * 1024;

/// A content-addressed chunk of data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// BLAKE3 hash of the content.
    pub hash: String,
    /// Raw bytes.
    pub data: Vec<u8>,
    /// Size in bytes.
    pub size: usize,
}

/// Metadata describing a complete object split into chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkManifest {
    /// BLAKE3 hash of the complete object.
    pub root_hash: String,
    /// Total size in bytes.
    pub total_size: usize,
    /// Ordered list of chunk hashes.
    pub chunks: Vec<String>,
    /// Content type tag (e.g., "job_spec", "node_report", "log_archive", "nix_closure").
    pub content_type: String,
    /// Human label.
    pub label: String,
}

impl Chunk {
    pub fn new(data: Vec<u8>) -> Self {
        let hash = blake3_hash(&data);
        let size = data.len();
        Self { hash, data, size }
    }

    pub fn verify(&self) -> bool {
        blake3_hash(&self.data) == self.hash
    }
}

impl ChunkManifest {
    /// Split data into content-addressed chunks and produce a manifest.
    pub fn from_data(data: &[u8], content_type: &str, label: &str) -> (Self, Vec<Chunk>) {
        let root_hash = blake3_hash(data);
        let mut chunks = Vec::new();
        let mut chunk_hashes = Vec::new();

        for chunk_data in data.chunks(DEFAULT_CHUNK_SIZE) {
            let chunk = Chunk::new(chunk_data.to_vec());
            chunk_hashes.push(chunk.hash.clone());
            chunks.push(chunk);
        }

        let manifest = Self {
            root_hash,
            total_size: data.len(),
            chunks: chunk_hashes,
            content_type: content_type.to_string(),
            label: label.to_string(),
        };

        (manifest, chunks)
    }

    /// Reassemble data from ordered chunks. Verifies each chunk hash.
    pub fn reassemble(&self, chunks: &[Chunk]) -> Result<Vec<u8>, String> {
        if chunks.len() != self.chunks.len() {
            return Err(format!(
                "Expected {} chunks, got {}",
                self.chunks.len(),
                chunks.len()
            ));
        }

        let mut data = Vec::with_capacity(self.total_size);

        for (i, chunk) in chunks.iter().enumerate() {
            if chunk.hash != self.chunks[i] {
                return Err(format!(
                    "Chunk {} hash mismatch: expected {}, got {}",
                    i, self.chunks[i], chunk.hash
                ));
            }
            if !chunk.verify() {
                return Err(format!("Chunk {} content verification failed", i));
            }
            data.extend_from_slice(&chunk.data);
        }

        // Verify reassembled data
        let actual_hash = blake3_hash(&data);
        if actual_hash != self.root_hash {
            return Err(format!(
                "Root hash mismatch: expected {}, got {}",
                self.root_hash, actual_hash
            ));
        }

        Ok(data)
    }
}

fn blake3_hash(data: &[u8]) -> String {
    let hash = blake3::hash(data);
    hash.to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_roundtrip() {
        let data = b"Hello, tatara! This is some test data for chunking.";
        let (manifest, chunks) = ChunkManifest::from_data(data, "test", "test-data");

        assert_eq!(manifest.total_size, data.len());
        assert_eq!(manifest.chunks.len(), 1); // Small data = 1 chunk

        let reassembled = manifest.reassemble(&chunks).unwrap();
        assert_eq!(reassembled, data);
    }

    #[test]
    fn test_multi_chunk_roundtrip() {
        // Generate data larger than chunk size
        let data: Vec<u8> = (0..DEFAULT_CHUNK_SIZE * 3 + 100)
            .map(|i| (i % 256) as u8)
            .collect();

        let (manifest, chunks) = ChunkManifest::from_data(&data, "test", "big-data");

        assert_eq!(manifest.chunks.len(), 4); // 3 full + 1 partial
        assert_eq!(manifest.total_size, data.len());

        let reassembled = manifest.reassemble(&chunks).unwrap();
        assert_eq!(reassembled, data);
    }

    #[test]
    fn test_chunk_verification() {
        let chunk = Chunk::new(b"test data".to_vec());
        assert!(chunk.verify());

        let mut bad_chunk = chunk.clone();
        bad_chunk.data = b"tampered".to_vec();
        assert!(!bad_chunk.verify());
    }
}
