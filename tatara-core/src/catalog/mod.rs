//! Service catalog types for tatara's consul-like service registry.
//!
//! Services are automatically registered when allocations start and
//! deregistered when they stop or become unhealthy. The catalog is
//! replicated via Raft for consistency and propagated via gossip for
//! fast local lookups.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Health status of a service instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceHealth {
    Passing,
    Warning,
    Critical,
    Maintenance,
}

impl Default for ServiceHealth {
    fn default() -> Self {
        Self::Passing
    }
}

/// A registered service instance in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEntry {
    /// Human-readable service name (e.g., "hanabi", "lilitu-backend").
    pub service_name: String,

    /// Unique instance ID (e.g., "hanabi-i-{alloc_id}").
    pub service_id: String,

    /// Node hosting this instance.
    pub node_id: String,

    /// IP address or hostname.
    pub address: String,

    /// Port number.
    pub port: u16,

    /// Optional tags for filtering (e.g., ["production", "v2.1"]).
    #[serde(default)]
    pub tags: Vec<String>,

    /// Arbitrary metadata.
    #[serde(default)]
    pub meta: HashMap<String, String>,

    /// Current health status.
    #[serde(default)]
    pub health: ServiceHealth,

    /// When this instance was registered.
    pub registered_at: DateTime<Utc>,

    /// Allocation ID that owns this service instance.
    pub alloc_id: Option<String>,
}

/// Query parameters for catalog lookups.
#[derive(Debug, Clone, Default)]
pub struct ServiceQuery {
    /// Service name to look up.
    pub service: String,

    /// Optional tag filter.
    pub tag: Option<String>,

    /// Only return healthy instances.
    pub healthy_only: bool,

    /// Prefer instances near this node.
    pub near: Option<String>,
}

impl ServiceEntry {
    /// Check if this entry matches a query.
    pub fn matches(&self, query: &ServiceQuery) -> bool {
        if self.service_name != query.service {
            return false;
        }
        if let Some(tag) = &query.tag {
            if !self.tags.contains(tag) {
                return false;
            }
        }
        if query.healthy_only && self.health != ServiceHealth::Passing {
            return false;
        }
        true
    }
}
