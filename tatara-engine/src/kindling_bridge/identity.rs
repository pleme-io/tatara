use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use tatara_core::cluster::types::{NodeId, NodeMeta, NodeRoles};
use tatara_core::domain::job::{DriverType, Resources};

/// Minimal subset of kindling's NodeIdentity we need.
/// We read kindling's node.yaml directly — no dependency on kindling as a library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindlingIdentity {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub hardware: HardwareConfig,
    #[serde(default)]
    pub fleet: FleetConfig,
    #[serde(default)]
    pub network: NetworkConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HardwareConfig {
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub cpu: CpuConfig,
    #[serde(default)]
    pub memory: Option<MemoryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CpuConfig {
    #[serde(default)]
    pub cores: Option<u32>,
    #[serde(default)]
    pub threads: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub size_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FleetConfig {
    pub controller: Option<String>,
    pub environment: Option<String>,
    pub owner: Option<String>,
    pub team: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub peers: Vec<FleetPeer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetPeer {
    pub name: String,
    pub hostname: String,
    #[serde(default = "default_ssh_user")]
    pub ssh_user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkConfig {
    #[serde(default)]
    pub interfaces: HashMap<String, NetworkInterface>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkInterface {
    pub address: Option<String>,
}

fn default_ssh_user() -> String {
    "root".to_string()
}

/// Load kindling identity from disk.
pub fn load_identity(path: Option<&Path>) -> Result<Option<KindlingIdentity>> {
    let identity_path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| default_identity_path());

    if !identity_path.exists() {
        debug!(path = %identity_path.display(), "Kindling identity file not found");
        return Ok(None);
    }

    let content = std::fs::read_to_string(&identity_path)
        .with_context(|| format!("Failed to read {}", identity_path.display()))?;

    let identity: KindlingIdentity = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", identity_path.display()))?;

    info!(
        hostname = %identity.hostname,
        profile = %identity.profile,
        "Loaded kindling identity"
    );

    Ok(Some(identity))
}

/// Derive a deterministic NodeId from the hostname.
/// Uses a hash to produce a u64 that's stable across restarts.
pub fn derive_node_id(hostname: &str) -> NodeId {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(hostname.as_bytes());
    u64::from_le_bytes(hash[..8].try_into().unwrap())
}

/// Build tatara NodeMeta from kindling identity + runtime detection.
pub fn build_node_meta(
    identity: &KindlingIdentity,
    roles: NodeRoles,
    http_addr: &str,
    gossip_addr: &str,
    raft_addr: &str,
    drivers: Vec<DriverType>,
) -> NodeMeta {
    let cpu_mhz = identity
        .hardware
        .cpu
        .threads
        .or(identity.hardware.cpu.cores)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|p| p.get() as u32)
                .unwrap_or(1)
        }) as u64
        * 1000;

    let memory_mb = identity
        .hardware
        .memory
        .as_ref()
        .map(|m| (m.size_gb * 1024.0) as u64)
        .unwrap_or(0);

    NodeMeta {
        node_id: derive_node_id(&identity.hostname),
        hostname: identity.hostname.clone(),
        http_addr: http_addr.to_string(),
        gossip_addr: gossip_addr.to_string(),
        raft_addr: raft_addr.to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        roles,
        drivers,
        total_resources: Resources {
            cpu_mhz,
            memory_mb,
        },
        available_resources: Resources {
            cpu_mhz,
            memory_mb,
        },
        allocations_running: 0,
        joined_at: chrono::Utc::now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        eligible: true,
            wireguard_pubkey: None,
            tunnel_address: None,
    }
}

/// Extract seed peers from kindling's fleet config.
/// Maps fleet peers to gossip addresses (hostname:gossip_port).
pub fn fleet_seed_peers(fleet: &FleetConfig, gossip_port: u16) -> Vec<String> {
    fleet
        .peers
        .iter()
        .map(|p| format!("{}:{}", p.hostname, gossip_port))
        .collect()
}

fn default_identity_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("kindling")
        .join("node.yaml")
}
