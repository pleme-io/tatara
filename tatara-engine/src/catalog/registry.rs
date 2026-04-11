//! In-memory service catalog with query support.

use chrono::Utc;
use std::collections::HashMap;
use tatara_core::catalog::{ServiceEntry, ServiceHealth, ServiceQuery};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Manages the local service catalog.
pub struct CatalogRegistry {
    /// service_name -> Vec<ServiceEntry>
    services: RwLock<HashMap<String, Vec<ServiceEntry>>>,
}

impl CatalogRegistry {
    pub fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
        }
    }

    /// Register a service instance.
    pub async fn register(&self, entry: ServiceEntry) {
        let mut services = self.services.write().await;
        info!(
            service = %entry.service_name,
            id = %entry.service_id,
            address = %entry.address,
            port = entry.port,
            "registering service"
        );
        services
            .entry(entry.service_name.clone())
            .or_default()
            .push(entry);
    }

    /// Deregister a service instance by its unique ID.
    pub async fn deregister(&self, service_id: &str) {
        let mut services = self.services.write().await;
        for instances in services.values_mut() {
            instances.retain(|e| e.service_id != service_id);
        }
        // Remove empty service names
        services.retain(|_, v| !v.is_empty());
        info!(service_id, "deregistered service");
    }

    /// Deregister all services for an allocation.
    pub async fn deregister_allocation(&self, alloc_id: &str) {
        let mut services = self.services.write().await;
        for instances in services.values_mut() {
            instances.retain(|e| e.alloc_id.as_deref() != Some(alloc_id));
        }
        services.retain(|_, v| !v.is_empty());
        debug!(alloc_id, "deregistered allocation services");
    }

    /// Update the health status of a service instance.
    pub async fn update_health(&self, service_id: &str, health: ServiceHealth) {
        let mut services = self.services.write().await;
        for instances in services.values_mut() {
            for entry in instances.iter_mut() {
                if entry.service_id == service_id {
                    entry.health = health.clone();
                    debug!(
                        service_id,
                        health = ?health,
                        "updated service health"
                    );
                    return;
                }
            }
        }
    }

    /// Query the catalog for service instances.
    pub async fn query(&self, q: &ServiceQuery) -> Vec<ServiceEntry> {
        let services = self.services.read().await;
        services
            .get(&q.service)
            .map(|instances| {
                let mut results: Vec<ServiceEntry> = instances
                    .iter()
                    .filter(|e| e.matches(q))
                    .cloned()
                    .collect();
                // Sort by health (passing first), then by node proximity if `near` is set
                if let Some(near) = &q.near {
                    results.sort_by(|a, b| {
                        let a_local = a.node_id == *near;
                        let b_local = b.node_id == *near;
                        b_local.cmp(&a_local)
                    });
                }
                results
            })
            .unwrap_or_default()
    }

    /// List all registered service names.
    pub async fn list_services(&self) -> Vec<String> {
        let services = self.services.read().await;
        services.keys().cloned().collect()
    }

    /// Get all instances of a service.
    pub async fn get_service(&self, name: &str) -> Vec<ServiceEntry> {
        let services = self.services.read().await;
        services.get(name).cloned().unwrap_or_default()
    }

    /// Get a snapshot of the entire catalog (for Raft state serialization).
    pub async fn snapshot(&self) -> HashMap<String, Vec<ServiceEntry>> {
        self.services.read().await.clone()
    }

    /// Restore catalog from a snapshot (after Raft leader election).
    pub async fn restore(&self, snapshot: HashMap<String, Vec<ServiceEntry>>) {
        let mut services = self.services.write().await;
        *services = snapshot;
    }

    /// Count total registered instances.
    pub async fn instance_count(&self) -> usize {
        let services = self.services.read().await;
        services.values().map(|v| v.len()).sum()
    }
}

impl Default for CatalogRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str, port: u16) -> ServiceEntry {
        ServiceEntry {
            service_name: name.to_string(),
            service_id: format!("{name}-i-{port}"),
            node_id: "node-1".to_string(),
            address: "127.0.0.1".to_string(),
            port,
            tags: vec!["production".to_string()],
            meta: HashMap::new(),
            health: ServiceHealth::Passing,
            registered_at: Utc::now(),
            alloc_id: Some(format!("alloc-{port}")),
        }
    }

    #[tokio::test]
    async fn test_register_and_query() {
        let reg = CatalogRegistry::new();
        reg.register(make_entry("web", 8080)).await;
        reg.register(make_entry("web", 8081)).await;
        reg.register(make_entry("api", 9090)).await;

        let q = ServiceQuery {
            service: "web".to_string(),
            ..Default::default()
        };
        let results = reg.query(&q).await;
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_healthy_only_query() {
        let reg = CatalogRegistry::new();
        reg.register(make_entry("web", 8080)).await;
        let mut unhealthy = make_entry("web", 8081);
        unhealthy.health = ServiceHealth::Critical;
        reg.register(unhealthy).await;

        let q = ServiceQuery {
            service: "web".to_string(),
            healthy_only: true,
            ..Default::default()
        };
        let results = reg.query(&q).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].port, 8080);
    }

    #[tokio::test]
    async fn test_deregister() {
        let reg = CatalogRegistry::new();
        reg.register(make_entry("web", 8080)).await;
        reg.register(make_entry("web", 8081)).await;

        reg.deregister("web-i-8080").await;

        let results = reg.get_service("web").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].port, 8081);
    }

    #[tokio::test]
    async fn test_deregister_allocation() {
        let reg = CatalogRegistry::new();
        reg.register(make_entry("web", 8080)).await;
        reg.register(make_entry("api", 9090)).await;

        reg.deregister_allocation("alloc-8080").await;

        assert_eq!(reg.get_service("web").await.len(), 0);
        assert_eq!(reg.get_service("api").await.len(), 1);
    }

    #[tokio::test]
    async fn test_update_health() {
        let reg = CatalogRegistry::new();
        reg.register(make_entry("web", 8080)).await;

        reg.update_health("web-i-8080", ServiceHealth::Critical)
            .await;

        let instances = reg.get_service("web").await;
        assert_eq!(instances[0].health, ServiceHealth::Critical);
    }

    #[tokio::test]
    async fn test_list_services() {
        let reg = CatalogRegistry::new();
        reg.register(make_entry("web", 8080)).await;
        reg.register(make_entry("api", 9090)).await;

        let mut names = reg.list_services().await;
        names.sort();
        assert_eq!(names, vec!["api", "web"]);
    }

    #[tokio::test]
    async fn test_near_sorting() {
        let reg = CatalogRegistry::new();
        let mut remote = make_entry("web", 8080);
        remote.node_id = "remote-node".to_string();
        let local = make_entry("web", 8081);
        reg.register(remote).await;
        reg.register(local).await;

        let q = ServiceQuery {
            service: "web".to_string(),
            near: Some("node-1".to_string()),
            ..Default::default()
        };
        let results = reg.query(&q).await;
        assert_eq!(results[0].node_id, "node-1"); // local first
    }
}
