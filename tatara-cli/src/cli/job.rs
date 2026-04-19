use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use super::context::{active_endpoint, endpoint_to_server};
use super::output::{build_table, human_duration_since, render_value, status_cell, OutputFormat};
use tatara_engine::nix_eval::evaluator::NixEvaluator;

pub async fn list(output: OutputFormat, endpoint: Option<&str>) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/jobs", server);
    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    let jobs: Vec<serde_json::Value> = resp.json().await?;

    if jobs.is_empty() {
        println!("No jobs.");
        return Ok(());
    }

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&jobs, output)?);
        }
        _ => {
            let wide = matches!(output, OutputFormat::Wide);
            let mut headers = vec!["ID", "TYPE", "STATUS", "GROUPS", "SUBMITTED"];
            if wide {
                headers.push("VERSION");
                headers.push("META");
            }
            let mut table = build_table(&headers);

            for job in &jobs {
                let mut row = vec![
                    comfy_table::Cell::new(job["id"].as_str().unwrap_or("?")),
                    comfy_table::Cell::new(job["job_type"].as_str().unwrap_or("?")),
                    status_cell(job["status"].as_str().unwrap_or("?")),
                    comfy_table::Cell::new(
                        job["groups"]
                            .as_array()
                            .map(|g| g.len().to_string())
                            .unwrap_or_else(|| "0".to_string()),
                    ),
                    comfy_table::Cell::new(human_duration_since(
                        job["submitted_at"].as_str().unwrap_or(""),
                    )),
                ];
                if wide {
                    row.push(comfy_table::Cell::new(
                        job["version"].as_u64().unwrap_or(0).to_string(),
                    ));
                    row.push(comfy_table::Cell::new(
                        serde_json::to_string(&job["meta"]).unwrap_or_default(),
                    ));
                }
                table.add_row(row);
            }
            println!("{table}");
        }
    }
    Ok(())
}

pub async fn get(job_id: &str, output: OutputFormat, endpoint: Option<&str>) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/jobs/{}", server, job_id);
    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status() == 404 {
        bail!("Job not found: {}", job_id);
    }
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    let detail: serde_json::Value = resp.json().await?;

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&detail, output)?);
        }
        _ => {
            let job = &detail["job"];
            println!("Job: {}", job["id"].as_str().unwrap_or("?"));
            println!("  Type:      {}", job["job_type"].as_str().unwrap_or("?"));
            println!("  Status:    {}", job["status"].as_str().unwrap_or("?"));
            println!("  Version:   {}", job["version"].as_u64().unwrap_or(0));
            println!(
                "  Submitted: {}",
                job["submitted_at"].as_str().unwrap_or("?")
            );
            println!(
                "  Groups:    {}",
                job["groups"].as_array().map(|g| g.len()).unwrap_or(0)
            );

            if let Some(allocs) = detail["allocations"].as_array() {
                if !allocs.is_empty() {
                    println!();
                    let mut table = build_table(&["ALLOC ID", "GROUP", "STATE", "NODE", "CREATED"]);
                    for alloc in allocs {
                        table.add_row(vec![
                            comfy_table::Cell::new(
                                alloc["id"].as_str().unwrap_or("?").get(..8).unwrap_or("?"),
                            ),
                            comfy_table::Cell::new(alloc["group_name"].as_str().unwrap_or("?")),
                            status_cell(alloc["state"].as_str().unwrap_or("?")),
                            comfy_table::Cell::new(alloc["node_id"].as_str().unwrap_or("?")),
                            comfy_table::Cell::new(human_duration_since(
                                alloc["created_at"].as_str().unwrap_or(""),
                            )),
                        ]);
                    }
                    println!("{table}");
                }
            }
        }
    }
    Ok(())
}

pub async fn run_job(
    job_file: &str,
    eval: bool,
    endpoint: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let spec: tatara_core::domain::job::JobSpec = if eval {
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

    let url = format!("http://{}/api/v1/jobs", server);
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&spec)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status().is_success() {
        let job: serde_json::Value = resp.json().await?;
        match output {
            OutputFormat::Json | OutputFormat::Yaml => {
                println!("{}", render_value(&job, output)?);
            }
            _ => {
                println!("Job submitted: {}", job["id"].as_str().unwrap_or("unknown"));
                println!("Status: {}", job["status"]);
            }
        }
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Server returned {}: {}", status, body);
    }

    Ok(())
}

pub async fn stop(job_id: &str, endpoint: Option<&str>, output: OutputFormat) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/jobs/{}/stop", server, job_id);

    let resp = client
        .post(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status().is_success() {
        let job: serde_json::Value = resp.json().await?;
        match output {
            OutputFormat::Json | OutputFormat::Yaml => {
                println!("{}", render_value(&job, output)?);
            }
            _ => {
                println!("Job stopped: {}", job["id"].as_str().unwrap_or(job_id));
                println!("Status: {}", job["status"].as_str().unwrap_or("dead"));
            }
        }
    } else if resp.status() == 404 {
        bail!("Job not found: {}", job_id);
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    Ok(())
}

pub async fn history(job_id: &str, output: OutputFormat, endpoint: Option<&str>) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/jobs/{}/history", server, job_id);

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status() == 404 {
        bail!("Job not found or no history: {}", job_id);
    }
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    let history: Vec<serde_json::Value> = resp.json().await?;

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&history, output)?);
        }
        _ => {
            if history.is_empty() {
                println!("No version history for job: {}", job_id);
                return Ok(());
            }
            let mut table = build_table(&["VERSION", "STATUS", "GROUPS", "SUBMITTED"]);
            for entry in &history {
                table.add_row(vec![
                    comfy_table::Cell::new(entry["version"].as_u64().unwrap_or(0).to_string()),
                    status_cell(entry["status"].as_str().unwrap_or("?")),
                    comfy_table::Cell::new(
                        entry["groups"]
                            .as_array()
                            .map(|g| g.len().to_string())
                            .unwrap_or_else(|| "0".to_string()),
                    ),
                    comfy_table::Cell::new(human_duration_since(
                        entry["submitted_at"].as_str().unwrap_or(""),
                    )),
                ]);
            }
            println!("{table}");
        }
    }
    Ok(())
}

pub async fn diff(job_id: &str, v1: u64, v2: u64, endpoint: Option<&str>) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/jobs/{}/history", server, job_id);

    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        bail!("Failed to fetch job history");
    }

    let history: Vec<serde_json::Value> = resp.json().await?;
    let spec1 = history
        .iter()
        .find(|h| h["version"].as_u64() == Some(v1))
        .ok_or_else(|| anyhow::anyhow!("Version {} not found", v1))?;
    let spec2 = history
        .iter()
        .find(|h| h["version"].as_u64() == Some(v2))
        .ok_or_else(|| anyhow::anyhow!("Version {} not found", v2))?;

    let s1 = serde_json::to_string_pretty(spec1)?;
    let s2 = serde_json::to_string_pretty(spec2)?;

    println!("--- v{}", v1);
    println!("+++ v{}", v2);
    for diff in diff::lines(&s1, &s2) {
        match diff {
            diff::Result::Left(l) => println!("-{}", l),
            diff::Result::Right(r) => println!("+{}", r),
            diff::Result::Both(b, _) => println!(" {}", b),
        }
    }

    Ok(())
}

pub async fn rollback(
    job_id: &str,
    version: u64,
    endpoint: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!(
        "http://{}/api/v1/jobs/{}/rollback/{}",
        server, job_id, version
    );

    let resp = client
        .post(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status().is_success() {
        let job: serde_json::Value = resp.json().await?;
        match output {
            OutputFormat::Json | OutputFormat::Yaml => {
                println!("{}", render_value(&job, output)?);
            }
            _ => {
                println!(
                    "Job {} rolled back to version {}",
                    job["id"].as_str().unwrap_or(job_id),
                    version
                );
            }
        }
    } else if resp.status() == 404 {
        bail!("Job or version not found: {} v{}", job_id, version);
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }
    Ok(())
}
