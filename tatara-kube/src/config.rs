use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Top-level configuration for tatara-kube.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubeConfig {
    /// Flake reference containing `kubeResources` output.
    pub flake_ref: String,

    /// Target system for Nix eval (e.g., "x86_64-linux").
    #[serde(default = "default_system")]
    pub system: String,

    /// Reconciliation interval in seconds.
    #[serde(default = "default_reconcile_interval")]
    pub reconcile_interval_secs: u64,

    /// Timeout for `nix flake metadata` calls.
    #[serde(default = "default_metadata_timeout")]
    pub flake_metadata_timeout_secs: u64,

    /// Timeout for `nix eval` calls.
    #[serde(default = "default_eval_timeout")]
    pub nix_eval_timeout_secs: u64,

    /// Field manager name for Server-Side Apply.
    #[serde(default = "default_field_manager")]
    pub field_manager: String,

    /// Whether to force Server-Side Apply (resolve conflicts by taking ownership).
    #[serde(default = "default_true")]
    pub force_apply: bool,

    /// Enable pruning of orphaned resources.
    #[serde(default = "default_true")]
    pub prune: bool,

    /// Enable health checking after apply.
    #[serde(default = "default_true")]
    pub health_check: bool,

    /// Health check timeout per resource in seconds.
    #[serde(default = "default_health_timeout")]
    pub health_check_timeout_secs: u64,

    /// Cluster targets to manage.
    #[serde(default)]
    pub clusters: HashMap<String, ClusterTarget>,

    /// Log level.
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

/// A target Kubernetes cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterTarget {
    /// Kubeconfig path. None = in-cluster or default.
    pub kubeconfig: Option<PathBuf>,

    /// Kubeconfig context name. None = current context.
    pub context: Option<String>,

    /// Nix attribute path within `kubeResources.<system>.clusters`
    pub nix_attr: String,

    /// Whether this cluster is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Namespaces to restrict management to. Empty = all namespaces.
    #[serde(default)]
    pub namespace_allowlist: Vec<String>,
}

fn default_system() -> String {
    #[cfg(target_arch = "x86_64")]
    {
        if cfg!(target_os = "linux") {
            "x86_64-linux".to_string()
        } else {
            "x86_64-darwin".to_string()
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if cfg!(target_os = "linux") {
            "aarch64-linux".to_string()
        } else {
            "aarch64-darwin".to_string()
        }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        "x86_64-linux".to_string()
    }
}

fn default_reconcile_interval() -> u64 {
    30
}
fn default_metadata_timeout() -> u64 {
    30
}
fn default_eval_timeout() -> u64 {
    120
}
fn default_field_manager() -> String {
    "tatara-kube".to_string()
}
fn default_true() -> bool {
    true
}
fn default_health_timeout() -> u64 {
    300
}
fn default_log_level() -> String {
    "info".to_string()
}

impl Default for KubeConfig {
    fn default() -> Self {
        Self {
            flake_ref: String::new(),
            system: default_system(),
            reconcile_interval_secs: default_reconcile_interval(),
            flake_metadata_timeout_secs: default_metadata_timeout(),
            nix_eval_timeout_secs: default_eval_timeout(),
            field_manager: default_field_manager(),
            force_apply: true,
            prune: true,
            health_check: true,
            health_check_timeout_secs: default_health_timeout(),
            clusters: HashMap::new(),
            log_level: default_log_level(),
        }
    }
}
