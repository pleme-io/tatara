use crate::error::KubeError;
use serde::Deserialize;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, info};

/// Nix flake metadata (revision + last modified).
#[derive(Debug, Clone, Deserialize)]
pub struct FlakeMetadata {
    #[serde(alias = "revision", alias = "rev")]
    pub rev: Option<String>,
    #[serde(alias = "lastModified", default)]
    pub last_modified: u64,
    #[serde(default)]
    pub url: String,
}

/// Evaluate kubeResources for a specific cluster.
///
/// Runs: `nix eval --json <flake_ref>#kubeResources.<system>.clusters.<cluster>`
pub async fn eval_cluster_resources(
    flake_ref: &str,
    system: &str,
    cluster: &str,
    timeout_secs: u64,
) -> Result<Vec<serde_json::Value>, KubeError> {
    let attr = format!(
        "{}#kubeResources.{}.clusters.{}",
        flake_ref, system, cluster
    );

    info!(attr = %attr, "evaluating nix expression");

    let output = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        Command::new("nix")
            .args(["eval", "--json", &attr])
            .output(),
    )
    .await
    .map_err(|_| KubeError::NixEvalTimeout {
        flake_ref: flake_ref.to_string(),
        timeout_secs,
    })?
    .map_err(|e| KubeError::NixEvalFailed {
        flake_ref: flake_ref.to_string(),
        reason: e.to_string(),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(KubeError::NixEvalFailed {
            flake_ref: flake_ref.to_string(),
            reason: stderr.to_string(),
        });
    }

    let json = String::from_utf8(output.stdout).map_err(|e| KubeError::NixEvalFailed {
        flake_ref: flake_ref.to_string(),
        reason: format!("output not valid UTF-8: {e}"),
    })?;

    debug!(bytes = json.len(), "nix eval completed");

    let resources: Vec<serde_json::Value> = serde_json::from_str(&json)?;
    Ok(resources)
}

/// Fetch flake metadata (revision, last modified).
///
/// Runs: `nix flake metadata --json <flake_ref>`
pub async fn flake_metadata(
    flake_ref: &str,
    timeout_secs: u64,
) -> Result<FlakeMetadata, KubeError> {
    let output = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        Command::new("nix")
            .args(["flake", "metadata", "--json", "--refresh", flake_ref])
            .output(),
    )
    .await
    .map_err(|_| KubeError::MetadataFetchFailed {
        flake_ref: flake_ref.to_string(),
        reason: "timeout".to_string(),
    })?
    .map_err(|e| KubeError::MetadataFetchFailed {
        flake_ref: flake_ref.to_string(),
        reason: e.to_string(),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(KubeError::MetadataFetchFailed {
            flake_ref: flake_ref.to_string(),
            reason: stderr.to_string(),
        });
    }

    let raw: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| KubeError::MetadataFetchFailed {
            flake_ref: flake_ref.to_string(),
            reason: e.to_string(),
        })?;

    Ok(FlakeMetadata {
        rev: raw
            .get("revision")
            .or_else(|| raw.get("rev"))
            .and_then(|v| v.as_str())
            .map(String::from),
        last_modified: raw
            .get("lastModified")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        url: raw
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}
