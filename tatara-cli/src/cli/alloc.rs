use anyhow::{bail, Context, Result};

use super::context::{active_endpoint, endpoint_to_server};
use super::output::{build_table, human_duration_since, render_value, status_cell, OutputFormat};

pub async fn list(output: OutputFormat, endpoint: Option<&str>) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/allocations", server);
    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    let allocs: Vec<serde_json::Value> = resp.json().await?;

    if allocs.is_empty() {
        println!("No allocations.");
        return Ok(());
    }

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&allocs, output)?);
        }
        _ => {
            let mut table = build_table(&["ID", "JOB", "GROUP", "NODE", "STATE", "CREATED"]);
            for alloc in &allocs {
                let id_str = alloc["id"].as_str().unwrap_or("?");
                let short_id = id_str.get(..8).unwrap_or(id_str);
                table.add_row(vec![
                    comfy_table::Cell::new(short_id),
                    comfy_table::Cell::new(alloc["job_id"].as_str().unwrap_or("?")),
                    comfy_table::Cell::new(alloc["group_name"].as_str().unwrap_or("?")),
                    comfy_table::Cell::new(alloc["node_id"].as_str().unwrap_or("?")),
                    status_cell(alloc["state"].as_str().unwrap_or("?")),
                    comfy_table::Cell::new(human_duration_since(
                        alloc["created_at"].as_str().unwrap_or(""),
                    )),
                ]);
            }
            println!("{table}");
        }
    }
    Ok(())
}

pub async fn get(alloc_id: &str, output: OutputFormat, endpoint: Option<&str>) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/allocations/{}", server, alloc_id);
    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status() == 404 {
        bail!("Allocation not found: {}", alloc_id);
    }
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    let alloc: serde_json::Value = resp.json().await?;

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&alloc, output)?);
        }
        _ => {
            println!("Allocation: {}", alloc["id"].as_str().unwrap_or("?"));
            println!("  Job:     {}", alloc["job_id"].as_str().unwrap_or("?"));
            println!("  Group:   {}", alloc["group_name"].as_str().unwrap_or("?"));
            println!("  Node:    {}", alloc["node_id"].as_str().unwrap_or("?"));
            println!("  State:   {}", alloc["state"].as_str().unwrap_or("?"));
            println!("  Created: {}", alloc["created_at"].as_str().unwrap_or("?"));

            if let Some(tasks) = alloc["task_states"].as_object() {
                if !tasks.is_empty() {
                    println!();
                    let mut table = build_table(&["TASK", "STATE", "PID", "EXIT CODE", "RESTARTS"]);
                    for (name, state) in tasks {
                        table.add_row(vec![
                            comfy_table::Cell::new(name),
                            status_cell(state["state"].as_str().unwrap_or("?")),
                            comfy_table::Cell::new(
                                state["pid"]
                                    .as_u64()
                                    .map(|p| p.to_string())
                                    .unwrap_or_else(|| "-".to_string()),
                            ),
                            comfy_table::Cell::new(
                                state["exit_code"]
                                    .as_i64()
                                    .map(|c| c.to_string())
                                    .unwrap_or_else(|| "-".to_string()),
                            ),
                            comfy_table::Cell::new(
                                state["restarts"].as_u64().unwrap_or(0).to_string(),
                            ),
                        ]);
                    }
                    println!("{table}");
                }
            }
        }
    }
    Ok(())
}

pub async fn logs(
    alloc_id: &str,
    task_name: Option<&str>,
    follow: bool,
    endpoint: Option<&str>,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();

    let mut url = format!("http://{}/api/v1/allocations/{}/logs", server, alloc_id);
    if let Some(task) = task_name {
        url.push_str(&format!("?task={}", task));
    }

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status() == 404 {
        bail!("Allocation not found: {}", alloc_id);
    }
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    let entries: Vec<serde_json::Value> = resp.json().await?;
    for entry in &entries {
        let stream = entry["stream"].as_str().unwrap_or("out");
        let message = entry["message"].as_str().unwrap_or("");
        let task = entry["task_name"].as_str().unwrap_or("?");
        println!("[{}][{}] {}", task, stream, message);
    }

    if follow {
        println!("--- follow mode: polling every 2s (Ctrl+C to stop) ---");
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let resp = client.get(&url).send().await?;
            if resp.status().is_success() {
                let entries: Vec<serde_json::Value> = resp.json().await?;
                for entry in &entries {
                    let stream = entry["stream"].as_str().unwrap_or("out");
                    let message = entry["message"].as_str().unwrap_or("");
                    let task = entry["task_name"].as_str().unwrap_or("?");
                    println!("[{}][{}] {}", task, stream, message);
                }
            }
        }
    }

    Ok(())
}
