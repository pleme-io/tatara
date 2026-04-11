//! Networking plane configuration.

use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::PathBuf;

/// Top-level networking configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetConfig {
    /// Whether the networking plane is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// WireGuard mesh configuration.
    #[serde(default)]
    pub mesh: MeshConfig,

    /// Network policy enforcement.
    #[serde(default)]
    pub policy: PolicyConfig,

    /// Flow observability.
    #[serde(default)]
    pub observability: ObservabilityConfig,

    /// WASI runtime configuration.
    #[serde(default)]
    pub wasi: WasiRuntimeConfig,
}

/// WireGuard mesh configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    /// Enable WireGuard mesh between nodes.
    #[serde(default)]
    pub enabled: bool,

    /// Mesh subnet (default: 10.42.0.0/16).
    #[serde(default = "default_mesh_subnet")]
    pub subnet: String,

    /// Path to store WireGuard private key.
    #[serde(default = "default_key_path")]
    pub key_path: PathBuf,

    /// WireGuard listen port.
    #[serde(default = "default_wg_port")]
    pub listen_port: u16,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            subnet: default_mesh_subnet(),
            key_path: default_key_path(),
            listen_port: default_wg_port(),
        }
    }
}

fn default_mesh_subnet() -> String {
    "10.42.0.0/16".to_string()
}

fn default_key_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("tatara")
        .join("wireguard.key")
}

fn default_wg_port() -> u16 {
    51820
}

/// Policy enforcement configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicyConfig {
    /// Enable policy enforcement.
    #[serde(default)]
    pub enabled: bool,

    /// Default policy action when no rule matches.
    #[serde(default = "default_policy_action")]
    pub default_action: String,
}

fn default_policy_action() -> String {
    "allow".to_string()
}

/// Flow observability configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObservabilityConfig {
    /// Enable flow logging.
    #[serde(default)]
    pub enabled: bool,

    /// Maximum flow log entries to retain.
    #[serde(default = "default_flow_capacity")]
    pub flow_capacity: usize,
}

fn default_flow_capacity() -> usize {
    100_000
}

/// WASI runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WasiRuntimeConfig {
    /// Enable WASI workload support.
    #[serde(default)]
    pub enabled: bool,

    /// Maximum memory per WASI instance (bytes).
    #[serde(default = "default_wasi_max_memory")]
    pub max_memory_bytes: u64,

    /// Maximum fuel (instruction count) per WASI instance.
    #[serde(default = "default_wasi_max_fuel")]
    pub max_fuel: u64,
}

fn default_wasi_max_memory() -> u64 {
    256 * 1024 * 1024 // 256 MB
}

fn default_wasi_max_fuel() -> u64 {
    1_000_000_000 // 1 billion instructions
}
