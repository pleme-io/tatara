//! Core networking types for the tatara cluster fabric.

use chrono::{DateTime, Utc};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use uuid::Uuid;

// ── Service Identity (what the networking plane routes on) ──────

/// A service's network identity. The networking plane routes on identity,
/// not IP:port. This decouples networking from infrastructure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ServiceIdentity {
    pub service_name: String,
    pub alloc_id: Option<Uuid>,
    pub namespace: Option<String>,
}

/// A resolved endpoint for a service instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceEndpoint {
    pub node_id: u64,
    pub address: IpAddr,
    pub port: u16,
    pub alloc_id: Uuid,
    pub health: EndpointHealth,
    pub weight: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EndpointHealth {
    #[default]
    Healthy,
    Degraded,
    Unhealthy,
    Draining,
}

// ── Network Policy ─────────────────────────────────────────────

/// Network policy — declared in Nix, enforced by eBPF (Linux) or userspace (macOS).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkPolicy {
    pub name: String,
    #[serde(default)]
    pub ingress: Vec<PolicySelector>,
    #[serde(default)]
    pub egress: Vec<PolicySelector>,
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
    #[serde(default)]
    pub l7_rules: Vec<L7Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PolicySelector {
    Service(String),
    Tag(String),
    Cidr(IpNet),
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicyRule {
    pub protocol: Protocol,
    pub ports: Vec<PortRange>,
    pub action: PolicyAction,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Tcp,
    Udp,
    Icmp,
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortRange {
    pub start: u16,
    pub end: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    Allow,
    Deny,
    Log,
}

/// L7 rule — inspected by hanabi proxy, not eBPF.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum L7Rule {
    HttpPath {
        path_prefix: String,
        methods: Vec<String>,
        action: PolicyAction,
    },
    GrpcMethod {
        service: String,
        method: String,
        action: PolicyAction,
    },
    HeaderMatch {
        header: String,
        value: String,
        action: PolicyAction,
    },
}

// ── Flow Observability ─────────────────────────────────────────

/// A flow log entry — records a connection between two services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub source: FlowEndpoint,
    pub destination: FlowEndpoint,
    pub verdict: PolicyAction,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub latency_us: Option<u64>,
    pub protocol: Protocol,
    pub l7_info: Option<L7FlowInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowEndpoint {
    pub identity: ServiceIdentity,
    pub address: IpAddr,
    pub port: u16,
    pub node_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L7FlowInfo {
    pub http_method: Option<String>,
    pub http_path: Option<String>,
    pub http_status: Option<u16>,
    pub grpc_method: Option<String>,
    pub grpc_status: Option<i32>,
}

/// Filter for querying flows.
#[derive(Debug, Clone, Default)]
pub struct FlowFilter {
    pub source_service: Option<String>,
    pub dest_service: Option<String>,
    pub verdict: Option<PolicyAction>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

// ── Mesh Peer Info ─────────────────────────────────────────────

/// WireGuard mesh peer info — stored in ClusterState via Raft.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshPeerInfo {
    pub node_id: u64,
    pub wireguard_pubkey: String,
    pub tunnel_address: IpAddr,
    pub external_endpoint: Option<SocketAddr>,
    pub tunnel_state: MeshTunnelState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MeshTunnelState {
    #[default]
    Pending,
    Established,
    Degraded,
    Failed,
}

// ── Networking Endpoint State Machine ──────────────────────────

/// Network endpoint state, tracked per (service, alloc_id).
/// Mirrors the WorkloadPhase lifecycle in networking terms.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NetEndpointPhase {
    /// Allocation Initial/early Warming. No networking configured.
    Inactive,
    /// Port allocated, route added, but not accepting traffic yet.
    Preparing,
    /// In routing table, accepting traffic. Health checks active.
    Active { weight: u32 },
    /// New connections routed elsewhere. Existing connections draining.
    Draining { drain_deadline: DateTime<Utc> },
    /// All state cleaned up.
    Removed,
}

impl NetEndpointPhase {
    pub fn is_routable(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    pub fn is_draining(&self) -> bool {
        matches!(self, Self::Draining { .. })
    }
}

// ── Per-Service-Pair Metrics ───────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServicePairMetrics {
    pub bytes_total: u64,
    pub connections_total: u64,
    pub latency_p50_us: u64,
    pub latency_p99_us: u64,
    pub error_rate: f64,
}

// ── WASI Types ─────────────────────────────────────────────────

/// WASI task configuration for the Wasi driver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasiTaskConfig {
    /// Path to .wasm component (can be Nix store path).
    pub wasm_path: String,
    /// WASI capabilities to grant.
    #[serde(default)]
    pub capabilities: WasiCapabilities,
    /// Filesystem mounts (host_path → guest_path).
    #[serde(default)]
    pub mounts: HashMap<String, String>,
    /// Network access configuration.
    #[serde(default)]
    pub network: Option<WasiNetworkConfig>,
}

/// WASI capability grants — capability-based security.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WasiCapabilities {
    pub filesystem: bool,
    pub network: bool,
    pub clocks: bool,
    pub random: bool,
    pub stdout: bool,
    pub stderr: bool,
}

/// Network configuration for WASI workloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasiNetworkConfig {
    /// Services this WASI component is allowed to connect to.
    #[serde(default)]
    pub allowed_services: Vec<String>,
    /// Explicit IP:port pairs allowed.
    #[serde(default)]
    pub allowed_addresses: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_identity_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        let id = ServiceIdentity {
            service_name: "web".to_string(),
            alloc_id: None,
            namespace: None,
        };
        set.insert(id.clone());
        assert!(set.contains(&id));
    }

    #[test]
    fn test_net_endpoint_phase() {
        let active = NetEndpointPhase::Active { weight: 100 };
        assert!(active.is_routable());
        assert!(!active.is_draining());

        let draining = NetEndpointPhase::Draining {
            drain_deadline: Utc::now(),
        };
        assert!(!draining.is_routable());
        assert!(draining.is_draining());
    }

    #[test]
    fn test_mesh_tunnel_state_default() {
        let state = MeshTunnelState::default();
        assert_eq!(state, MeshTunnelState::Pending);
    }

    #[test]
    fn test_network_policy_serde() {
        let policy = NetworkPolicy {
            name: "frontend-policy".to_string(),
            ingress: vec![PolicySelector::Service("gateway".to_string())],
            egress: vec![PolicySelector::Any],
            rules: vec![PolicyRule {
                protocol: Protocol::Tcp,
                ports: vec![PortRange { start: 8080, end: 8080 }],
                action: PolicyAction::Allow,
            }],
            l7_rules: vec![],
        };
        let json = serde_json::to_string(&policy).unwrap();
        let back: NetworkPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy, back);
    }

    #[test]
    fn test_wasi_capabilities_default() {
        let caps = WasiCapabilities::default();
        assert!(!caps.filesystem);
        assert!(!caps.network);
    }

    #[test]
    fn test_flow_filter_default() {
        let filter = FlowFilter::default();
        assert!(filter.source_service.is_none());
        assert!(filter.limit.is_none());
    }

    #[test]
    fn test_mesh_ip_deterministic() {
        // 10.42.{node_id >> 8}.{node_id & 0xFF}
        let node_id: u64 = 42;
        let octet3 = (node_id >> 8) as u8;
        let octet4 = (node_id & 0xFF) as u8;
        let ip = IpAddr::V4(std::net::Ipv4Addr::new(10, 42, octet3, octet4));
        assert_eq!(ip.to_string(), "10.42.0.42");
    }
}
