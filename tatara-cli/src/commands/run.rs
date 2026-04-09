use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use tatara_engine::nix_eval::evaluator::NixEvaluator;

pub async fn run(job_file: &str, eval: bool, server_addr: &str) -> Result<()> {
    let spec = if eval {
        let path = PathBuf::from(job_file);
        if !path.exists() {
            bail!("Nix file not found: {}", job_file);
        }
        NixEvaluator::eval_file(&path).await?
    } else {
        let path = PathBuf::from(job_file);
        if !path.exists() {
            bail!("Job file not found: {}", job_file);
        }
        let content = tokio::fs::read_to_string(&path)
            .await
            .context("Failed to read job file")?;

        if job_file.ends_with(".yaml") || job_file.ends_with(".yml") {
            serde_yaml::from_str(&content).context("Failed to parse YAML job spec")?
        } else {
            serde_json::from_str(&content).context("Failed to parse JSON job spec")?
        }
    };

    let url = format!("http://{}/api/v1/jobs", server_addr);
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&spec)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status().is_success() {
        let job: serde_json::Value = resp.json().await?;
        let job_id = job["id"].as_str().unwrap_or("unknown");
        println!("Job submitted: {}", job_id);
        println!("Status: {}", job["status"]);
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Server returned {}: {}", status, body);
    }

    Ok(())
}
