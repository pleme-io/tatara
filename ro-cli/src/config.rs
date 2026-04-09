use anyhow::Result;
use serde::{Deserialize, Serialize};
use shikumi::{ConfigDiscovery, ConfigStore};

/// Client configuration — follows the shikumi pattern:
///   Nix module generates ~/.config/ro/ro.yaml → Rust reads it at runtime.
///   Env vars with RO_ prefix override any file values.
///
/// The only required setting is api_endpoint. Everything else is discovered
/// from the ro API at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoConfig {
    /// The ro platform API endpoint (e.g., https://api.ro.ben-kar.com)
    pub api_endpoint: String,

    /// Output format for CLI commands
    #[serde(default = "default_format")]
    pub output_format: OutputFormat,

    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Follow logs by default when streaming
    #[serde(default)]
    pub follow_logs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    Yaml,
}

fn default_format() -> OutputFormat {
    OutputFormat::Table
}

fn default_timeout() -> u64 {
    120
}

impl Default for RoConfig {
    fn default() -> Self {
        Self {
            api_endpoint: "https://api.ro.ben-kar.com".to_string(),
            output_format: OutputFormat::Table,
            timeout_secs: 120,
            follow_logs: false,
        }
    }
}

impl RoConfig {
    /// Load config via shikumi: ~/.config/ro/ro.yaml + RO_ env prefix.
    ///
    /// Priority: env vars > config file > defaults
    /// This follows the Nix → YAML → Rust pattern:
    ///   1. Nix module writes ~/.config/ro/ro.yaml at activation
    ///   2. This function reads it via shikumi
    ///   3. RO_ env vars override for CI/scripts
    pub fn load() -> Result<Self> {
        // Discover config file path via shikumi XDG convention:
        //   ~/.config/ro/ro.yaml (written by Nix module at activation)
        let discovery = ConfigDiscovery::new("ro");

        match discovery.discover() {
            Ok(path) => {
                let store = ConfigStore::<Self>::load(&path, "RO_")
                    .map_err(|e| anyhow::anyhow!("Failed to load config from {}: {}", path.display(), e))?;
                // Guard<Arc<T>> → clone the inner T
                let guard = store.get();
                Ok((**guard).clone())
            }
            Err(_) => {
                // No config file found — try env var, then default
                if let Ok(endpoint) = std::env::var("RO_API_ENDPOINT") {
                    Ok(Self {
                        api_endpoint: endpoint,
                        ..Self::default()
                    })
                } else {
                    Ok(Self::default())
                }
            }
        }
    }
}
