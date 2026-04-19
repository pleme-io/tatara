//! Idempotency layer for Raft commands.
//!
//! Prevents duplicate operations during leader transitions or retries.
//! Based on the exactly-once semantics pattern:
//!   idempotency_key → dedup store → TTL expiry → cached response.
//!
//! Reference: Exactly-once semantics in distributed systems
//! (foundational to Raft, Kafka, and production consensus systems).

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default TTL for idempotency keys (5 minutes).
const DEFAULT_TTL_SECS: i64 = 300;

/// A deduplication store that tracks recently processed idempotency keys.
/// Stored in ClusterState and replicated via Raft.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IdempotencyStore {
    /// key → (response, expires_at)
    entries: HashMap<String, IdempotencyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdempotencyEntry {
    /// The cached response for this key.
    pub response: String, // Serialized ClusterResponse
    /// When this entry expires.
    pub expires_at: DateTime<Utc>,
}

impl IdempotencyStore {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Check if an idempotency key has already been processed.
    /// Returns the cached response if it has, None if it hasn't.
    pub fn check(&self, key: &str) -> Option<&str> {
        let entry = self.entries.get(key)?;
        if Utc::now() < entry.expires_at {
            Some(&entry.response)
        } else {
            None // Expired
        }
    }

    /// Record that an idempotency key has been processed with the given response.
    pub fn record(&mut self, key: String, response: String) {
        self.entries.insert(
            key,
            IdempotencyEntry {
                response,
                expires_at: Utc::now() + Duration::seconds(DEFAULT_TTL_SECS),
            },
        );
    }

    /// Record with a custom TTL.
    pub fn record_with_ttl(&mut self, key: String, response: String, ttl_secs: i64) {
        self.entries.insert(
            key,
            IdempotencyEntry {
                response,
                expires_at: Utc::now() + Duration::seconds(ttl_secs),
            },
        );
    }

    /// Garbage collect expired entries. Call periodically.
    pub fn gc(&mut self) {
        let now = Utc::now();
        self.entries.retain(|_, entry| entry.expires_at > now);
    }

    /// Number of active (non-expired) entries.
    pub fn len(&self) -> usize {
        let now = Utc::now();
        self.entries.values().filter(|e| e.expires_at > now).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idempotency_check_and_record() {
        let mut store = IdempotencyStore::new();

        // First time: not found
        assert!(store.check("key-1").is_none());

        // Record
        store.record("key-1".to_string(), "response-1".to_string());

        // Second time: found
        assert_eq!(store.check("key-1"), Some("response-1"));
    }

    #[test]
    fn test_idempotency_expiry() {
        let mut store = IdempotencyStore::new();

        // Record with 0-second TTL (already expired)
        store.record_with_ttl("key-1".to_string(), "response-1".to_string(), -1);

        // Should not find expired key
        assert!(store.check("key-1").is_none());
    }

    #[test]
    fn test_gc() {
        let mut store = IdempotencyStore::new();

        // Record one valid and one expired
        store.record("valid".to_string(), "ok".to_string());
        store.record_with_ttl("expired".to_string(), "old".to_string(), -1);

        store.gc();

        assert_eq!(store.entries.len(), 1);
        assert!(store.entries.contains_key("valid"));
    }

    #[test]
    fn test_len_excludes_expired() {
        let mut store = IdempotencyStore::new();
        store.record("valid".to_string(), "ok".to_string());
        store.record_with_ttl("expired".to_string(), "old".to_string(), -1);

        assert_eq!(store.len(), 1);
    }
}
