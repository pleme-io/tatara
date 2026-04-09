use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use crate::p2p::transfer::TransferEngine;

/// Minimal subset of kindling's StoredReport for reading cached reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindlingReport {
    pub checksum: String,
    pub collected_at: String,
    pub collector_version: String,
    pub report: serde_json::Value,
}

/// Read kindling's cached report from disk.
pub fn load_report(path: Option<&Path>) -> Result<Option<KindlingReport>> {
    let report_path = path
        .map(PathBuf::from)
        .unwrap_or_else(default_report_path);

    if !report_path.exists() {
        debug!(path = %report_path.display(), "Kindling report not found");
        return Ok(None);
    }

    let content = std::fs::read_to_string(&report_path)
        .with_context(|| format!("Failed to read {}", report_path.display()))?;

    let report: KindlingReport = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", report_path.display()))?;

    info!(
        checksum = %report.checksum,
        collected_at = %report.collected_at,
        "Loaded kindling report"
    );

    Ok(Some(report))
}

/// Publish kindling's node report to the p2p cache for dissemination.
/// Other nodes can fetch this to understand the cluster's hardware landscape.
pub async fn publish_report(
    transfer: &TransferEngine,
    report: &KindlingReport,
    hostname: &str,
) -> Result<()> {
    let data = serde_json::to_vec(report)?;
    let label = format!("kindling-report:{}", hostname);

    transfer
        .publish(&data, "node_report", &label)
        .await?;

    info!(
        hostname = hostname,
        "Published kindling report to p2p cache"
    );

    Ok(())
}

fn default_report_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("kindling")
        .join("report.json")
}
