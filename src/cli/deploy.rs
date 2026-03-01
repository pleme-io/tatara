use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

use super::context::{active_endpoint, endpoint_to_server};
use super::output::{render_value, OutputFormat};
use crate::nix_eval::evaluator::NixEvaluator;

pub async fn run(
    flake_ref: &str,
    set_values: &[(String, String)],
    name: Option<&str>,
    dry_run: bool,
    endpoint: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));

    // Step 1: Resolve flake ref
    println!("Resolving {}...", flake_ref);
    let mut spec = resolve_flake_ref(flake_ref).await?;

    // Step 2: Apply --set overrides
    if !set_values.is_empty() {
        apply_overrides(&mut spec, set_values)?;
    }

    // Override job name if --name provided
    if let Some(n) = name {
        spec["id"] = serde_json::Value::String(n.to_string());
    }

    // Step 3: Dry run — show what would be deployed
    if dry_run {
        println!("--- Dry Run ---");

        // Check if job already exists
        let job_id = spec["id"].as_str().unwrap_or("unknown");
        let client = reqwest::Client::new();
        let url = format!("http://{}/api/v1/jobs/{}", server, job_id);
        let existing = client.get(&url).send().await;

        if let Ok(resp) = existing {
            if resp.status().is_success() {
                let current: serde_json::Value = resp.json().await?;
                println!("Existing job found. Diff:");
                let current_str = serde_json::to_string_pretty(&current["job"])?;
                let new_str = serde_json::to_string_pretty(&spec)?;
                for diff in diff::lines(&current_str, &new_str) {
                    match diff {
                        diff::Result::Left(l) => println!("-{}", l),
                        diff::Result::Right(r) => println!("+{}", r),
                        diff::Result::Both(b, _) => println!(" {}", b),
                    }
                }
            } else {
                println!("New job (no existing job with id '{}'):", job_id);
                println!("{}", serde_json::to_string_pretty(&spec)?);
            }
        }

        println!("\nResources required:");
        if let Some(groups) = spec["groups"].as_array() {
            for group in groups {
                let count = group["count"].as_u64().unwrap_or(1);
                let cpu = group["resources"]["cpu_mhz"].as_u64().unwrap_or(0);
                let mem = group["resources"]["memory_mb"].as_u64().unwrap_or(0);
                println!(
                    "  Group '{}': {} instances x {}MHz CPU, {}MB memory",
                    group["name"].as_str().unwrap_or("?"),
                    count,
                    cpu,
                    mem
                );
            }
        }
        return Ok(());
    }

    // Step 4: Submit job via REST
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/jobs", server);
    let resp = client
        .post(&url)
        .json(&spec)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Server returned {}: {}", status, body);
    }

    let job: serde_json::Value = resp.json().await?;
    let job_id = job["id"].as_str().unwrap_or("unknown");

    // Step 5: Create release record
    let release_body = serde_json::json!({
        "name": name.unwrap_or(job_id),
        "flake_ref": flake_ref,
        "job_id": job_id,
    });

    let release_url = format!("http://{}/api/v1/releases", server);
    let _ = client.post(&release_url).json(&release_body).send().await;

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&job, output)?);
        }
        _ => {
            println!("Deployed: {} (version {})", job_id, job["version"]);
            println!("Flake:   {}", flake_ref);
            println!("Status:  {}", job["status"]);
        }
    }

    // Step 6: Stream events briefly to show deployment progress
    println!("\nWaiting for allocations...");
    let mut settled = false;
    for _ in 0..15 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let alloc_url = format!("http://{}/api/v1/jobs/{}", server, job_id);
        if let Ok(resp) = client.get(&alloc_url).send().await {
            if resp.status().is_success() {
                let detail: serde_json::Value = resp.json().await?;
                if let Some(allocs) = detail["allocations"].as_array() {
                    let running = allocs
                        .iter()
                        .filter(|a| a["state"].as_str() == Some("running"))
                        .count();
                    let total = allocs.len();
                    print!("\r  Allocations: {}/{} running", running, total);
                    if running == total && total > 0 {
                        println!(" - deployment complete!");
                        settled = true;
                        break;
                    }
                }
            }
        }
    }

    if !settled {
        println!("\n  Deployment still in progress. Use 'tatara job get {}' to check.", job_id);
    }

    Ok(())
}

async fn resolve_flake_ref(flake_ref: &str) -> Result<serde_json::Value> {
    // If it looks like a local path, try to eval as a Nix file
    let path = PathBuf::from(flake_ref);
    if path.exists() {
        if flake_ref.ends_with(".nix") {
            let spec = NixEvaluator::eval_file(&path).await?;
            return Ok(serde_json::to_value(spec)?);
        }
        if flake_ref.ends_with(".json") {
            let content = tokio::fs::read_to_string(&path).await?;
            return Ok(serde_json::from_str(&content)?);
        }
        if flake_ref.ends_with(".yaml") || flake_ref.ends_with(".yml") {
            let content = tokio::fs::read_to_string(&path).await?;
            let value: serde_json::Value = serde_yaml::from_str(&content)?;
            return Ok(value);
        }
    }

    // Otherwise, treat as a flake reference and eval via nix
    let output = tokio::process::Command::new("nix")
        .args(["eval", "--json", flake_ref])
        .output()
        .await
        .context("Failed to run nix eval")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("nix eval failed for '{}': {}", flake_ref, stderr);
    }

    let json = String::from_utf8(output.stdout)?;
    let spec: serde_json::Value = serde_json::from_str(&json)?;
    Ok(spec)
}

fn apply_overrides(
    spec: &mut serde_json::Value,
    overrides: &[(String, String)],
) -> Result<()> {
    for (key, value) in overrides {
        // Support dotted paths: "groups.0.count" -> spec["groups"][0]["count"]
        let parts: Vec<&str> = key.split('.').collect();
        let mut target = &mut *spec;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                // Try to parse value as JSON, fall back to string
                let json_value = serde_json::from_str(value)
                    .unwrap_or_else(|_| serde_json::Value::String(value.clone()));
                target[part] = json_value;
            } else if let Ok(idx) = part.parse::<usize>() {
                target = &mut target[idx];
            } else {
                target = &mut target[*part];
            }
        }
    }
    Ok(())
}
