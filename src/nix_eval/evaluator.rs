use anyhow::{bail, Context, Result};
use std::path::Path;
use tokio::process::Command;
use tracing::{debug, info};

use crate::domain::job::JobSpec;

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
}
