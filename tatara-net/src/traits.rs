//! Core networking plane trait — platform-agnostic interface.
//!
//! On Linux: backed by eBPF (XDP/TC) + mamorigami WireGuard.
//! On macOS: backed by tun-rs + smoltcp + hanabi proxy.

use async_trait::async_trait;
use uuid::Uuid;

use crate::types::*;

/// Platform-agnostic networking plane.
#[async_trait]
pub trait NetworkPlane: Send + Sync {
    // ── Layer 0: Encrypted Mesh ──

    /// Connect to a peer node in the WireGuard mesh.
    async fn connect_peer(&self, peer: &MeshPeerInfo) -> Result<(), NetError>;

    /// Disconnect a peer from the mesh.
    async fn disconnect_peer(&self, node_id: u64) -> Result<(), NetError>;

    /// Get mesh tunnel states.
    async fn mesh_status(&self) -> Result<Vec<MeshPeerInfo>, NetError>;

    // ── Layer 1: Identity-Based Routing ──

    /// Register a service endpoint (allocation started executing).
    async fn register_endpoint(
        &self,
        identity: &ServiceIdentity,
        endpoint: &ServiceEndpoint,
    ) -> Result<(), NetError>;

    /// Deregister a service endpoint (allocation contracting/terminal).
    async fn deregister_endpoint(
        &self,
        identity: &ServiceIdentity,
        alloc_id: Uuid,
    ) -> Result<(), NetError>;

    /// Update endpoint health (affects routing weight).
    async fn update_endpoint_health(
        &self,
        identity: &ServiceIdentity,
        alloc_id: Uuid,
        health: EndpointHealth,
    ) -> Result<(), NetError>;

    // ── Layer 2: Policy Enforcement ──

    /// Apply a network policy.
    async fn apply_policy(&self, policy: &NetworkPolicy) -> Result<(), NetError>;

    /// Remove a network policy.
    async fn remove_policy(&self, name: &str) -> Result<(), NetError>;

    /// List active policies.
    async fn list_policies(&self) -> Result<Vec<NetworkPolicy>, NetError>;

    // ── Layer 3: Observability ──

    /// Query flow logs.
    async fn get_flows(&self, filter: &FlowFilter) -> Result<Vec<Flow>, NetError>;

    /// Get per-service-pair metrics.
    async fn get_service_metrics(
        &self,
        source: &str,
        dest: &str,
    ) -> Result<ServicePairMetrics, NetError>;
}

/// Networking plane errors.
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("tunnel error: {0}")]
    Tunnel(String),

    #[error("policy error: {0}")]
    Policy(String),

    #[error("routing error: {0}")]
    Routing(String),

    #[error("platform not supported: {0}")]
    PlatformNotSupported(String),

    #[error("eBPF error: {0}")]
    Ebpf(String),

    #[error("WASI error: {0}")]
    Wasi(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
