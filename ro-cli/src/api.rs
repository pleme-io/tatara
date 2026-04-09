use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::RoConfig;

/// HTTP client for the ro platform API.
pub struct RoClient {
    http: Client,
    base_url: String,
}

// ── API types ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct BuildRequest {
    pub flake_ref: String,
    pub system: String,
    pub attic_cache: Option<String>,
    pub extra_args: Vec<String>,
    pub priority: i32,
}

#[derive(Debug, Deserialize)]
pub struct BuildResponse {
    pub build_id: String,
    pub status: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct BuildStatus {
    pub build_id: Option<String>,
    pub phase: String,
    pub store_path: Option<String>,
    pub builder_node: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    pub substituters: Vec<String>,
    pub trusted_public_keys: Vec<String>,
    pub cache_endpoint: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct SourceStatus {
    pub name: String,
    pub repo: String,
    pub branch: String,
    pub last_commit: Option<String>,
    pub cached_outputs: u32,
    pub total_outputs: u32,
}

#[derive(Debug, Deserialize)]
pub struct CacheInfo {
    pub name: String,
    pub endpoint: String,
    pub total_nars: u64,
    pub total_size_bytes: u64,
}

// ── Client implementation ────────────────────────────────────────────────

impl RoClient {
    pub fn new(config: &RoConfig) -> Result<Self> {
        let http = Client::builder()
            .user_agent("ro-cli/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            http,
            base_url: config.api_endpoint.trim_end_matches('/').to_string(),
        })
    }

    /// GET /config — discover platform configuration
    pub async fn get_config(&self) -> Result<PlatformConfig> {
        let resp = self.http
            .get(format!("{}/config", self.base_url))
            .send()
            .await
            .context("Failed to connect to ro API")?;

        resp.error_for_status_ref()
            .context("ro API returned an error")?;

        resp.json().await.context("Failed to parse config response")
    }

    /// POST /api/v1/builds — submit a build request
    pub async fn submit_build(&self, request: &BuildRequest) -> Result<BuildResponse> {
        let resp = self.http
            .post(format!("{}/api/v1/builds", self.base_url))
            .json(request)
            .send()
            .await
            .context("Failed to submit build")?;

        resp.error_for_status_ref()
            .context("Build submission failed")?;

        resp.json().await.context("Failed to parse build response")
    }

    /// GET /api/v1/builds/{id} — get build status
    pub async fn get_build(&self, build_id: &str) -> Result<BuildStatus> {
        let resp = self.http
            .get(format!("{}/api/v1/builds/{}", self.base_url, build_id))
            .send()
            .await
            .context("Failed to get build status")?;

        resp.error_for_status_ref()
            .context("Build status request failed")?;

        resp.json().await.context("Failed to parse build status")
    }

    /// GET /api/v1/builds/{id}/logs — stream build logs (SSE)
    pub async fn stream_logs(&self, build_id: &str) -> Result<reqwest::Response> {
        let resp = self.http
            .get(format!("{}/api/v1/builds/{}/logs", self.base_url, build_id))
            .send()
            .await
            .context("Failed to stream build logs")?;

        resp.error_for_status_ref()
            .context("Log stream request failed")?;

        Ok(resp)
    }

    /// GET /api/v1/sources — list FlakeSource status
    pub async fn list_sources(&self) -> Result<Vec<SourceStatus>> {
        let resp = self.http
            .get(format!("{}/api/v1/sources", self.base_url))
            .send()
            .await
            .context("Failed to list sources")?;

        resp.error_for_status_ref()?;
        resp.json().await.context("Failed to parse sources")
    }

    /// GET /api/v1/cache — get cache info
    pub async fn cache_info(&self) -> Result<CacheInfo> {
        let resp = self.http
            .get(format!("{}/api/v1/cache", self.base_url))
            .send()
            .await
            .context("Failed to get cache info")?;

        resp.error_for_status_ref()?;
        resp.json().await.context("Failed to parse cache info")
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// GET /health — check API health
    pub async fn health(&self) -> Result<bool> {
        let resp = self.http
            .get(format!("{}/health", self.base_url))
            .send()
            .await;

        Ok(resp.map(|r| r.status().is_success()).unwrap_or(false))
    }
}
