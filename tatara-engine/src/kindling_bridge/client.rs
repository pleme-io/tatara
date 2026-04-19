use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// HTTP client for the kindling daemon API.
///
/// Speaks to kindling's REST API (`/api/v1/*`) to fetch
/// identity, reports, platform info, nix status, store info, etc.
pub struct KindlingClient {
    base_url: String,
    http: reqwest::Client,
}

// ── Response types (mirrors kindling's API types) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NixStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub nix_path: Option<String>,
    pub install_method: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub target_triple: String,
    pub is_wsl: bool,
    pub has_systemd: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreInfo {
    pub store_dir: String,
    pub store_size_bytes: Option<u64>,
    pub path_count: Option<u64>,
    pub roots_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NixConfig {
    pub substituters: Vec<String>,
    pub trusted_public_keys: Vec<String>,
    pub max_jobs: Option<String>,
    pub cores: Option<String>,
    pub experimental_features: Vec<String>,
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcStatus {
    pub auto_gc_enabled: bool,
    pub schedule_secs: u64,
    pub last_gc_at: Option<String>,
    pub last_gc_freed_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcResult {
    pub freed_bytes: u64,
    pub freed_paths: u64,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimiseResult {
    pub deduplicated_bytes: u64,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheInfo {
    pub substituter: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonHealth {
    pub version: String,
    pub uptime_secs: u64,
    pub platform: PlatformInfo,
    pub nix: NixStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIdentity {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub hardware: serde_json::Value,
    #[serde(default)]
    pub network: serde_json::Value,
    #[serde(default)]
    pub fleet: serde_json::Value,
    #[serde(default)]
    pub kubernetes: serde_json::Value,
    #[serde(default)]
    pub nix: serde_json::Value,
    #[serde(default)]
    pub services: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredReport {
    pub checksum: String,
    pub collected_at: String,
    pub collector_version: String,
    pub report: serde_json::Value,
}

impl KindlingClient {
    pub fn new(base_url: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        }
    }

    /// Check if kindling daemon is reachable.
    pub async fn is_healthy(&self) -> bool {
        match self.http.get(self.url("/health")).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    // ── Status & Health ──

    pub async fn health(&self) -> Result<DaemonHealth> {
        self.get("/health").await
    }

    pub async fn nix_status(&self) -> Result<NixStatus> {
        self.get("/api/v1/status").await
    }

    pub async fn platform(&self) -> Result<PlatformInfo> {
        self.get("/api/v1/platform").await
    }

    // ── Store & Config ──

    pub async fn store_info(&self) -> Result<StoreInfo> {
        self.get("/api/v1/store").await
    }

    pub async fn nix_config(&self) -> Result<NixConfig> {
        self.get("/api/v1/config").await
    }

    pub async fn caches(&self) -> Result<Vec<CacheInfo>> {
        self.get("/api/v1/caches").await
    }

    // ── Garbage Collection ──

    pub async fn gc_status(&self) -> Result<GcStatus> {
        self.get("/api/v1/gc").await
    }

    pub async fn run_gc(&self) -> Result<GcResult> {
        self.post("/api/v1/gc/run").await
    }

    // ── Store Optimization ──

    pub async fn optimise_store(&self) -> Result<OptimiseResult> {
        self.post("/api/v1/store/optimise").await
    }

    // ── Identity & Reports ──

    pub async fn identity(&self) -> Result<Option<NodeIdentity>> {
        match self.http.get(self.url("/api/v1/identity")).send().await {
            Ok(resp) if resp.status().as_u16() == 404 => Ok(None),
            Ok(resp) if resp.status().is_success() => {
                let identity = resp.json().await.context("Failed to parse identity")?;
                Ok(Some(identity))
            }
            Ok(resp) => {
                anyhow::bail!("Kindling identity request failed: {}", resp.status())
            }
            Err(e) => Err(e).context("Failed to reach kindling daemon"),
        }
    }

    pub async fn report(&self) -> Result<Option<StoredReport>> {
        match self.http.get(self.url("/api/v1/report")).send().await {
            Ok(resp) if resp.status().as_u16() == 503 => Ok(None),
            Ok(resp) if resp.status().is_success() => {
                let report = resp.json().await.context("Failed to parse report")?;
                Ok(Some(report))
            }
            Ok(resp) => {
                anyhow::bail!("Kindling report request failed: {}", resp.status())
            }
            Err(e) => Err(e).context("Failed to reach kindling daemon"),
        }
    }

    pub async fn refresh_report(&self) -> Result<StoredReport> {
        self.post("/api/v1/report/refresh").await
    }

    // ── Internal helpers ──

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path);
        debug!(url = %url, "Kindling API GET");

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Failed to reach kindling daemon at {}", url))?;

        if !resp.status().is_success() {
            anyhow::bail!("Kindling API {} returned {}", path, resp.status());
        }

        resp.json()
            .await
            .with_context(|| format!("Failed to parse response from {}", path))
    }

    async fn post<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.url(path);
        debug!(url = %url, "Kindling API POST");

        let resp = self
            .http
            .post(&url)
            .send()
            .await
            .with_context(|| format!("Failed to reach kindling daemon at {}", url))?;

        if !resp.status().is_success() {
            anyhow::bail!("Kindling API {} returned {}", path, resp.status());
        }

        resp.json()
            .await
            .with_context(|| format!("Failed to parse response from {}", path))
    }
}

/// Attempt to connect to kindling and log what we find.
/// Non-fatal — tatara works fine without kindling.
pub async fn probe_kindling(addr: &str) -> Option<KindlingClient> {
    let client = KindlingClient::new(addr);

    if !client.is_healthy().await {
        warn!(
            addr = addr,
            "Kindling daemon not reachable — running without it"
        );
        return None;
    }

    match client.health().await {
        Ok(health) => {
            tracing::info!(
                version = %health.version,
                uptime = health.uptime_secs,
                nix_installed = health.nix.installed,
                nix_version = ?health.nix.version,
                platform = %health.platform.os,
                arch = %health.platform.arch,
                "Connected to kindling daemon"
            );
            Some(client)
        }
        Err(e) => {
            warn!(error = %e, "Kindling daemon responded but health check failed");
            None
        }
    }
}
