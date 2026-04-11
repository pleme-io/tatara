//! Sui daemon client — uses sui's REST API for builds and evaluation
//! instead of shelling out to the `nix` CLI.
//!
//! When `sui_daemon_addr` is configured, tatara uses sui-daemon for:
//! - `nix eval` → `POST /api/v1/eval`
//! - `nix build` → `POST /api/v1/build`
//! - Cache push → `POST /api/v1/cache/push`

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Client for the sui daemon REST API.
pub struct SuiClient {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Serialize)]
struct BuildRequest {
    flake_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    extra_args: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BuildResponse {
    store_path: String,
    #[serde(default)]
    build_time_ms: u64,
}

#[derive(Debug, Serialize)]
struct EvalRequest {
    expression: String,
}

#[derive(Debug, Deserialize)]
struct EvalResponse {
    result: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct CachePushRequest {
    store_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_name: Option<String>,
}

impl SuiClient {
    /// Create a new sui client pointing at the given daemon address.
    pub fn new(daemon_addr: &str) -> Self {
        let base_url = if daemon_addr.starts_with("http") {
            daemon_addr.to_string()
        } else {
            format!("http://{daemon_addr}")
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_default();

        Self { client, base_url }
    }

    /// Build a derivation via sui-daemon.
    pub async fn build(
        &self,
        flake_ref: &str,
        system: Option<&str>,
        extra_args: Vec<String>,
    ) -> Result<String> {
        let url = format!("{}/api/v1/build", self.base_url);
        info!(flake_ref, "building via sui-daemon");

        let resp = self
            .client
            .post(&url)
            .json(&BuildRequest {
                flake_ref: flake_ref.to_string(),
                system: system.map(String::from),
                extra_args,
            })
            .send()
            .await
            .context("sui-daemon build request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("sui-daemon build failed ({status}): {body}");
        }

        let result: BuildResponse = resp.json().await?;
        debug!(store_path = %result.store_path, build_time_ms = result.build_time_ms, "build complete");
        Ok(result.store_path)
    }

    /// Evaluate a Nix expression via sui-daemon.
    pub async fn eval_json(&self, expr: &str) -> Result<serde_json::Value> {
        let url = format!("{}/api/v1/eval", self.base_url);
        debug!(expr_len = expr.len(), "evaluating via sui-daemon");

        let resp = self
            .client
            .post(&url)
            .json(&EvalRequest {
                expression: expr.to_string(),
            })
            .send()
            .await
            .context("sui-daemon eval request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("sui-daemon eval failed ({status}): {body}");
        }

        let result: EvalResponse = resp.json().await?;
        Ok(result.result)
    }

    /// Push a store path to the binary cache via sui-daemon.
    pub async fn push_to_cache(
        &self,
        store_path: &str,
        cache_name: Option<&str>,
    ) -> Result<()> {
        let url = format!("{}/api/v1/cache/push", self.base_url);
        info!(store_path, "pushing to sui-cache");

        let resp = self
            .client
            .post(&url)
            .json(&CachePushRequest {
                store_path: store_path.to_string(),
                cache_name: cache_name.map(String::from),
            })
            .send()
            .await
            .context("sui-daemon cache push failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("sui-daemon cache push failed ({status}): {body}");
        }

        Ok(())
    }

    /// Check if the sui-daemon is reachable.
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/health", self.base_url);
        self.client
            .get(&url)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}
