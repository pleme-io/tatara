use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, info};

use crate::domain::job::JobSpec;
use crate::domain::source::FlakeMetadata;

pub struct NixEvaluator;

impl NixEvaluator {
    /// Evaluate a Nix file to a JSON job spec.
    pub async fn eval_file(path: &Path) -> Result<JobSpec> {
        let path_str = path
            .to_str()
            .context("Invalid path encoding")?;

        info!(path = %path_str, "Evaluating Nix job spec");

        let output = Command::new("nix")
            .args([
                "eval",
                "--json",
                "--impure",
                "-f",
                path_str,
            ])
            .output()
            .await
            .context("Failed to run nix eval")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("nix eval failed: {}", stderr);
        }

        let json = String::from_utf8(output.stdout)
            .context("nix eval output is not valid UTF-8")?;

        debug!(json = %json, "Nix eval result");

        let spec: JobSpec = serde_json::from_str(&json)
            .context("Failed to parse nix eval output as JobSpec")?;

        Ok(spec)
    }

    /// Evaluate a Nix expression string to a JSON job spec.
    pub async fn eval_expr(expr: &str) -> Result<JobSpec> {
        let output = Command::new("nix")
            .args(["eval", "--json", "--impure", "--expr", expr])
            .output()
            .await
            .context("Failed to run nix eval")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("nix eval failed: {}", stderr);
        }

        let json = String::from_utf8(output.stdout)
            .context("nix eval output is not valid UTF-8")?;

        let spec: JobSpec = serde_json::from_str(&json)
            .context("Failed to parse nix eval output as JobSpec")?;

        Ok(spec)
    }

    /// Get flake metadata (revision, last modified, resolved URL).
    pub async fn flake_metadata(flake_ref: &str, timeout_secs: u64) -> Result<FlakeMetadata> {
        info!(flake_ref = %flake_ref, "Fetching flake metadata");

        let output = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            Command::new("nix")
                .args(["flake", "metadata", "--json", "--refresh", flake_ref])
                .output(),
        )
        .await
        .context("nix flake metadata timed out")?
        .context("Failed to run nix flake metadata")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("nix flake metadata failed: {}", stderr);
        }

        let json = String::from_utf8(output.stdout)
            .context("nix flake metadata output is not valid UTF-8")?;

        let raw: serde_json::Value = serde_json::from_str(&json)
            .context("Failed to parse nix flake metadata JSON")?;

        let rev = raw.get("revision")
            .or_else(|| raw.get("rev"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let last_modified = raw.get("lastModified")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let url = raw.get("url")
            .or_else(|| raw.get("resolvedUrl"))
            .and_then(|v| v.as_str())
            .unwrap_or(flake_ref)
            .to_string();

        debug!(rev = ?rev, last_modified = last_modified, url = %url, "Flake metadata");

        Ok(FlakeMetadata {
            rev,
            last_modified,
            url,
        })
    }

    /// Evaluate tataraJobs from a flake, returning a map of job name to JobSpec.
    pub async fn eval_tatara_jobs(flake_ref: &str) -> Result<HashMap<String, JobSpec>> {
        let expr = format!(
            "(builtins.getFlake \"{}\").tataraJobs.${{builtins.currentSystem}} or {{}}",
            flake_ref
        );

        info!(flake_ref = %flake_ref, "Evaluating tataraJobs");

        let output = Command::new("nix")
            .args(["eval", "--json", "--impure", "--expr", &expr])
            .output()
            .await
            .context("Failed to run nix eval for tataraJobs")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("nix eval tataraJobs failed: {}", stderr);
        }

        let json = String::from_utf8(output.stdout)
            .context("nix eval tataraJobs output is not valid UTF-8")?;

        debug!(json = %json, "tataraJobs eval result");

        let jobs: HashMap<String, JobSpec> = serde_json::from_str(&json)
            .context("Failed to parse tataraJobs output")?;

        info!(count = jobs.len(), "Evaluated tataraJobs");

        Ok(jobs)
    }
}
