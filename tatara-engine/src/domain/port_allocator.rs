//! Dynamic port allocation for tatara workloads.
//!
//! Assigns ports from a configurable range when tasks declare port 0.
//! Tracks allocated ports per allocation for conflict detection.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ops::RangeInclusive;
use tokio::sync::RwLock;
use uuid::Uuid;

/// An allocated port for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocatedPort {
    /// Port label from the job spec (e.g., "http", "grpc").
    pub label: String,

    /// Static port from job spec (if explicitly specified).
    pub static_port: Option<u16>,

    /// The actual port assigned (dynamic or static).
    pub assigned_port: u16,
}

/// Manages port allocation for a node.
pub struct PortAllocator {
    range: RangeInclusive<u16>,
    /// (alloc_id, label) -> assigned port
    allocated: RwLock<HashMap<(Uuid, String), u16>>,
}

impl PortAllocator {
    /// Create a new allocator with the given port range.
    pub fn new(start: u16, end: u16) -> Self {
        Self {
            range: start..=end,
            allocated: RwLock::new(HashMap::new()),
        }
    }

    /// Default range: 20000-32000.
    pub fn default_range() -> Self {
        Self::new(20000, 32000)
    }

    /// Allocate a port for a task. If `requested` is 0, assign dynamically.
    /// If `requested` is non-zero, use it if available.
    pub async fn allocate(
        &self,
        alloc_id: Uuid,
        label: &str,
        requested: u16,
    ) -> Result<AllocatedPort, PortError> {
        let mut allocated = self.allocated.write().await;

        if requested != 0 {
            // Static port — check for conflicts
            let in_use = allocated.values().any(|&p| p == requested);
            if in_use {
                return Err(PortError::Conflict {
                    port: requested,
                    label: label.to_string(),
                });
            }
            allocated.insert((alloc_id, label.to_string()), requested);
            return Ok(AllocatedPort {
                label: label.to_string(),
                static_port: Some(requested),
                assigned_port: requested,
            });
        }

        // Dynamic allocation — find first available in range
        let used: HashSet<u16> = allocated.values().copied().collect();
        for port in self.range.clone() {
            if !used.contains(&port) {
                allocated.insert((alloc_id, label.to_string()), port);
                return Ok(AllocatedPort {
                    label: label.to_string(),
                    static_port: None,
                    assigned_port: port,
                });
            }
        }

        Err(PortError::Exhausted)
    }

    /// Release all ports for an allocation.
    pub async fn release(&self, alloc_id: Uuid) {
        let mut allocated = self.allocated.write().await;
        allocated.retain(|(id, _), _| *id != alloc_id);
    }

    /// Check if a specific port is available on this node.
    pub async fn is_available(&self, port: u16) -> bool {
        let allocated = self.allocated.read().await;
        !allocated.values().any(|&p| p == port)
    }

    /// Get all allocated ports for an allocation.
    pub async fn get_ports(&self, alloc_id: Uuid) -> Vec<AllocatedPort> {
        let allocated = self.allocated.read().await;
        allocated
            .iter()
            .filter(|((id, _), _)| *id == alloc_id)
            .map(|((_, label), &port)| AllocatedPort {
                label: label.clone(),
                static_port: None,
                assigned_port: port,
            })
            .collect()
    }
}

/// Port allocation errors.
#[derive(Debug, thiserror::Error)]
pub enum PortError {
    #[error("port {port} already in use (requested for '{label}')")]
    Conflict { port: u16, label: String },

    #[error("all ports in range exhausted")]
    Exhausted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dynamic_allocation() {
        let alloc = PortAllocator::new(30000, 30005);
        let id = Uuid::new_v4();

        let p1 = alloc.allocate(id, "http", 0).await.unwrap();
        assert_eq!(p1.assigned_port, 30000);
        assert!(p1.static_port.is_none());

        let p2 = alloc.allocate(id, "grpc", 0).await.unwrap();
        assert_eq!(p2.assigned_port, 30001);
    }

    #[tokio::test]
    async fn test_static_allocation() {
        let alloc = PortAllocator::new(30000, 30005);
        let id = Uuid::new_v4();

        let p = alloc.allocate(id, "http", 8080).await.unwrap();
        assert_eq!(p.assigned_port, 8080);
        assert_eq!(p.static_port, Some(8080));
    }

    #[tokio::test]
    async fn test_conflict_detection() {
        let alloc = PortAllocator::new(30000, 30005);
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        alloc.allocate(id1, "http", 8080).await.unwrap();
        let err = alloc.allocate(id2, "http", 8080).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_release() {
        let alloc = PortAllocator::new(30000, 30002);
        let id = Uuid::new_v4();

        alloc.allocate(id, "a", 0).await.unwrap();
        alloc.allocate(id, "b", 0).await.unwrap();
        alloc.allocate(id, "c", 0).await.unwrap();

        // Range exhausted
        let id2 = Uuid::new_v4();
        assert!(alloc.allocate(id2, "d", 0).await.is_err());

        // Release and try again
        alloc.release(id).await;
        assert!(alloc.allocate(id2, "d", 0).await.is_ok());
    }
}
