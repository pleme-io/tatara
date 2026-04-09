use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::cluster::roles::RoleConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_http_addr")]
    pub http_addr: String,
    #[serde(default = "default_grpc_addr")]
    pub grpc_addr: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub state: StateConfig,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    pub cluster: ClusterConfig,
    #[serde(default)]
    pub p2p: P2pConfig,
    #[serde(default)]
    pub kindling: KindlingConfig,
    #[serde(default)]
    pub reconciler: ReconcilerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateConfig {
    #[serde(default = "default_server_state_dir")]
    pub dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    #[serde(default = "default_eval_interval")]
    pub eval_interval_secs: u64,
    #[serde(default = "default_heartbeat_grace")]
    pub heartbeat_grace_secs: u64,
}

/// Reconciler configuration — controls the reconciliation loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcilerConfig {
    /// Reconciliation loop interval in seconds.
    #[serde(default = "default_reconcile_interval")]
    pub reconcile_interval_secs: u64,

    /// Nix re-evaluation happens every Nth reconcile tick.
    #[serde(default = "default_reeval_frequency")]
    pub reeval_every_n_ticks: u64,

    /// Max concurrent Nix evaluations.
    #[serde(default = "default_max_concurrent_evals")]
    pub max_concurrent_evals: usize,

    /// Enable spec drift detection via Nix re-evaluation.
    #[serde(default = "default_true")]
    pub drift_detection: bool,

    /// Enable source reconciliation (Pass 5).
    #[serde(default = "default_true")]
    pub source_reconciliation: bool,

    /// Source re-check happens every Nth reconcile tick.
    #[serde(default = "default_source_reeval_frequency")]
    pub source_reeval_every_n_ticks: u64,

    /// Timeout for `nix flake metadata` calls (seconds).
    #[serde(default = "default_flake_metadata_timeout")]
    pub flake_metadata_timeout_secs: u64,
}

/// Cluster configuration — gossip, raft, discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Unique cluster identifier (nodes with different IDs ignore each other).
    #[serde(default = "default_cluster_id")]
    pub cluster_id: String,

    /// Gossip listen address (UDP).
    #[serde(default = "default_gossip_addr")]
    pub gossip_addr: String,

    /// Raft listen address (HTTP-based RPCs).
    #[serde(default = "default_raft_addr")]
    pub raft_addr: String,

    /// Static seed peers for gossip bootstrap (host:port).
    /// In addition to mDNS and kindling fleet discovery.
    #[serde(default)]
    pub seed_peers: Vec<String>,

    /// Enable mDNS discovery on local network.
    #[serde(default = "default_true")]
    pub mdns_discovery: bool,

    /// Use kindling fleet peers as gossip seeds.
    #[serde(default = "default_true")]
    pub kindling_fleet_seeds: bool,

    /// Node roles.
    #[serde(default)]
    pub roles: RoleConfig,

    /// Bootstrap as single-node cluster if no peers found.
    #[serde(default = "default_true")]
    pub auto_bootstrap: bool,
}

/// P2P content-addressed cache configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pConfig {
    /// Cache directory.
    #[serde(default = "default_p2p_cache_dir")]
    pub cache_dir: PathBuf,

    /// Maximum cache size in MB. 0 = unlimited.
    #[serde(default)]
    pub max_cache_mb: u64,

    /// Auto-replicate: eagerly fetch data from peers in background.
    #[serde(default = "default_true")]
    pub eager_replication: bool,
}

/// Kindling integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindlingConfig {
    /// Path to kindling's node.yaml. Empty = auto-detect.
    #[serde(default)]
    pub identity_path: Option<String>,

    /// Path to kindling's report.json. Empty = auto-detect.
    #[serde(default)]
    pub report_path: Option<String>,

    /// Kindling daemon HTTP address (for API client).
    /// Empty = auto-detect from default kindling port.
    #[serde(default = "default_kindling_addr")]
    pub daemon_addr: String,

    /// Publish kindling reports to the p2p cache.
    #[serde(default = "default_true")]
    pub publish_reports: bool,

    /// Report refresh interval (seconds). 0 = use kindling's own interval.
    #[serde(default)]
    pub report_refresh_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    #[serde(default = "default_client_server_addr")]
    pub server_addr: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_alloc_dir")]
    pub alloc_dir: PathBuf,
    #[serde(default)]
    pub resources: ResourceConfig,
    #[serde(default)]
    pub drivers: DriverConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    #[serde(default)]
    pub cpu_mhz: u64,
    #[serde(default)]
    pub memory_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverConfig {
    #[serde(default = "default_true")]
    pub exec: bool,
    #[serde(default = "default_true")]
    pub oci: bool,
    #[serde(default = "default_true")]
    pub nix: bool,
}

impl ServerConfig {
    pub fn load(path: Option<&str>) -> anyhow::Result<Self> {
        let config_path = match path {
            Some(p) => PathBuf::from(p),
            None => default_config_dir().join("server.toml"),
        };

        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            Ok(toml::from_str(&contents)?)
        } else {
            Ok(Self::default())
        }
    }
}

impl ClientConfig {
    pub fn load(path: Option<&str>) -> anyhow::Result<Self> {
        let config_path = match path {
            Some(p) => PathBuf::from(p),
            None => default_config_dir().join("client.toml"),
        };

        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            Ok(toml::from_str(&contents)?)
        } else {
            Ok(Self::default())
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            http_addr: default_http_addr(),
            grpc_addr: default_grpc_addr(),
            log_level: default_log_level(),
            state: StateConfig::default(),
            scheduler: SchedulerConfig::default(),
            cluster: ClusterConfig::default(),
            p2p: P2pConfig::default(),
            kindling: KindlingConfig::default(),
            reconciler: ReconcilerConfig::default(),
        }
    }
}

impl Default for ReconcilerConfig {
    fn default() -> Self {
        Self {
            reconcile_interval_secs: default_reconcile_interval(),
            reeval_every_n_ticks: default_reeval_frequency(),
            max_concurrent_evals: default_max_concurrent_evals(),
            drift_detection: true,
            source_reconciliation: true,
            source_reeval_every_n_ticks: default_source_reeval_frequency(),
            flake_metadata_timeout_secs: default_flake_metadata_timeout(),
        }
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_addr: default_client_server_addr(),
            log_level: default_log_level(),
            alloc_dir: default_alloc_dir(),
            resources: ResourceConfig::default(),
            drivers: DriverConfig::default(),
        }
    }
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            dir: default_server_state_dir(),
        }
    }
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            eval_interval_secs: default_eval_interval(),
            heartbeat_grace_secs: default_heartbeat_grace(),
        }
    }
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            cluster_id: default_cluster_id(),
            gossip_addr: default_gossip_addr(),
            raft_addr: default_raft_addr(),
            seed_peers: Vec::new(),
            mdns_discovery: true,
            kindling_fleet_seeds: true,
            roles: RoleConfig::default(),
            auto_bootstrap: true,
        }
    }
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            cache_dir: default_p2p_cache_dir(),
            max_cache_mb: 0,
            eager_replication: true,
        }
    }
}

impl Default for KindlingConfig {
    fn default() -> Self {
        Self {
            identity_path: None,
            report_path: None,
            daemon_addr: default_kindling_addr(),
            publish_reports: true,
            report_refresh_secs: 0,
        }
    }
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            cpu_mhz: 0,
            memory_mb: 0,
        }
    }
}

impl Default for DriverConfig {
    fn default() -> Self {
        Self {
            exec: true,
            oci: true,
            nix: true,
        }
    }
}

fn default_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("tatara")
}

fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("tatara")
}

fn default_http_addr() -> String {
    "0.0.0.0:4646".to_string()
}

fn default_grpc_addr() -> String {
    "0.0.0.0:4647".to_string()
}

fn default_gossip_addr() -> String {
    "0.0.0.0:4648".to_string()
}

fn default_raft_addr() -> String {
    "0.0.0.0:4649".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_cluster_id() -> String {
    "tatara-default".to_string()
}

fn default_server_state_dir() -> PathBuf {
    default_data_dir().join("server")
}

fn default_alloc_dir() -> PathBuf {
    default_data_dir().join("alloc")
}

fn default_p2p_cache_dir() -> PathBuf {
    default_data_dir().join("p2p")
}

fn default_client_server_addr() -> String {
    "127.0.0.1:4647".to_string()
}

fn default_eval_interval() -> u64 {
    1
}

fn default_reconcile_interval() -> u64 {
    10
}

fn default_reeval_frequency() -> u64 {
    6
}

fn default_max_concurrent_evals() -> usize {
    2
}

fn default_heartbeat_grace() -> u64 {
    30
}

fn default_source_reeval_frequency() -> u64 {
    3
}

fn default_flake_metadata_timeout() -> u64 {
    30
}

fn default_kindling_addr() -> String {
    "http://127.0.0.1:3000".to_string()
}

fn default_true() -> bool {
    true
}
