use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, info};

use tatara_core::domain::job::JobSpec;
use tatara_core::domain::source::{FlakeMetadata, SourceError};

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
    pub async fn flake_metadata(flake_ref: &str, timeout_secs: u64) -> Result<FlakeMetadata, SourceError> {
        info!(flake_ref = %flake_ref, "Fetching flake metadata");

        let output = match tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            Command::new("nix")
                .args(["flake", "metadata", "--json", "--refresh", flake_ref])
                .output(),
        )
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(SourceError::MetadataFetchFailed {
                    flake_ref: flake_ref.to_string(),
                    reason: format!("failed to run nix flake metadata: {}", e),
                });
            }
            Err(_) => {
                return Err(SourceError::Timeout {
                    flake_ref: flake_ref.to_string(),
                    timeout_secs,
                });
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SourceError::MetadataFetchFailed {
                flake_ref: flake_ref.to_string(),
                reason: stderr.to_string(),
            });
        }

        let json = String::from_utf8(output.stdout).map_err(|e| {
            SourceError::MetadataFetchFailed {
                flake_ref: flake_ref.to_string(),
                reason: format!("output is not valid UTF-8: {}", e),
            }
        })?;

        let raw: serde_json::Value = serde_json::from_str(&json).map_err(|e| {
            SourceError::MetadataFetchFailed {
                flake_ref: flake_ref.to_string(),
                reason: format!("failed to parse JSON: {}", e),
            }
        })?;

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

    /// Validate that a flake exports the expected outputs for source reconciliation.
    /// Checks for tataraJobs and tataraMeta. Returns Ok(()) if valid, or
    /// SourceError::ValidationFailed with a list of issues.
    pub async fn validate_source(flake_ref: &str, source_name: &str) -> Result<(), SourceError> {
        // Check `nix flake show` for expected outputs
        let output = Command::new("nix")
            .args(["flake", "show", "--json", flake_ref])
            .output()
            .await
            .map_err(|e| SourceError::ValidationFailed {
                name: source_name.to_string(),
                errors: vec![format!("failed to run nix flake show: {}", e)],
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SourceError::ValidationFailed {
                name: source_name.to_string(),
                errors: vec![format!("nix flake show failed: {}", stderr)],
            });
        }

        let json = String::from_utf8(output.stdout).map_err(|e| {
            SourceError::ValidationFailed {
                name: source_name.to_string(),
                errors: vec![format!("output is not valid UTF-8: {}", e)],
            }
        })?;

        let raw: serde_json::Value = serde_json::from_str(&json).map_err(|e| {
            SourceError::ValidationFailed {
                name: source_name.to_string(),
                errors: vec![format!("failed to parse JSON: {}", e)],
            }
        })?;

        let mut errors = Vec::new();

        if raw.get("tataraJobs").is_none() {
            errors.push("Missing 'tataraJobs' output — source must export tataraJobs.<system>".to_string());
        }

        if raw.get("tataraMeta").is_none() {
            errors.push("Missing 'tataraMeta' output — source should export name and version".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(SourceError::ValidationFailed {
                name: source_name.to_string(),
                errors,
            })
        }
    }

    /// Evaluate tataraJobs from a flake, returning a map of job name to JobSpec.
    pub async fn eval_tatara_jobs(flake_ref: &str) -> Result<HashMap<String, JobSpec>, SourceError> {
        let expr = format!(
            "(builtins.getFlake \"{}\").tataraJobs.${{builtins.currentSystem}} or {{}}",
            flake_ref
        );

        info!(flake_ref = %flake_ref, "Evaluating tataraJobs");

        let output = Command::new("nix")
            .args(["eval", "--json", "--impure", "--expr", &expr])
            .output()
            .await
            .map_err(|e| SourceError::EvalFailed {
                flake_ref: flake_ref.to_string(),
                reason: format!("failed to run nix eval: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SourceError::EvalFailed {
                flake_ref: flake_ref.to_string(),
                reason: stderr.to_string(),
            });
        }

        let json = String::from_utf8(output.stdout).map_err(|e| {
            SourceError::EvalFailed {
                flake_ref: flake_ref.to_string(),
                reason: format!("output is not valid UTF-8: {}", e),
            }
        })?;

        debug!(json = %json, "tataraJobs eval result");

        let jobs: HashMap<String, JobSpec> = serde_json::from_str(&json).map_err(|e| {
            SourceError::EvalFailed {
                flake_ref: flake_ref.to_string(),
                reason: format!("failed to parse tataraJobs: {}", e),
            }
        })?;

        info!(count = jobs.len(), "Evaluated tataraJobs");

        Ok(jobs)
    }
}
