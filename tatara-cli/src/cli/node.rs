use anyhow::{bail, Context, Result};

use super::context::{active_endpoint, endpoint_to_server};
use super::output::{build_table, human_duration_since, render_value, status_cell, OutputFormat};

pub async fn list(output: OutputFormat, endpoint: Option<&str>) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/nodes", server);
    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if !resp.status().is_success() {
        bail!("Failed to query nodes");
    }

    let nodes: Vec<serde_json::Value> = resp.json().await?;

    if nodes.is_empty() {
        println!("No nodes.");
        return Ok(());
    }

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&nodes, output)?);
        }
        _ => {
            let wide = matches!(output, OutputFormat::Wide);
            let mut headers = vec![
                "ID",
                "HOSTNAME",
                "STATUS",
                "CPU (MHz)",
                "MEM (MB)",
                "ALLOCS",
            ];
            if wide {
                headers.extend(&["DRIVERS", "OS", "ARCH", "ELIGIBLE", "JOINED"]);
            }
            let mut table = build_table(&headers);

            for node in &nodes {
                let status = node
                    .get("status")
                    .and_then(|s| s.as_str())
                    // NodeMeta doesn't have status directly, infer from availability
                    .unwrap_or("ready");

                let mut row = vec![
                    comfy_table::Cell::new(
                        node["node_id"]
                            .as_u64()
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| node["id"].as_str().unwrap_or("?").to_string()),
                    ),
                    comfy_table::Cell::new(node["hostname"].as_str().unwrap_or("?")),
                    status_cell(status),
                    comfy_table::Cell::new(
                        node["total_resources"]["cpu_mhz"]
                            .as_u64()
                            .unwrap_or(0)
                            .to_string(),
                    ),
                    comfy_table::Cell::new(
                        node["total_resources"]["memory_mb"]
                            .as_u64()
                            .unwrap_or(0)
                            .to_string(),
                    ),
                    comfy_table::Cell::new(
                        node["allocations_running"]
                            .as_u64()
                            .unwrap_or(0)
                            .to_string(),
                    ),
                ];

                if wide {
                    row.push(comfy_table::Cell::new(
                        node["drivers"]
                            .as_array()
                            .map(|d| {
                                d.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(",")
                            })
                            .unwrap_or_default(),
                    ));
                    row.push(comfy_table::Cell::new(node["os"].as_str().unwrap_or("?")));
                    row.push(comfy_table::Cell::new(node["arch"].as_str().unwrap_or("?")));
                    row.push(comfy_table::Cell::new(
                        node["eligible"]
                            .as_bool()
                            .map(|b| if b { "yes" } else { "no" })
                            .unwrap_or("yes"),
                    ));
                    row.push(comfy_table::Cell::new(human_duration_since(
                        node["joined_at"].as_str().unwrap_or(""),
                    )));
                }
                table.add_row(row);
            }
            println!("{table}");
        }
    }
    Ok(())
}

pub async fn status(
    node_id: Option<&str>,
    output: OutputFormat,
    endpoint: Option<&str>,
) -> Result<()> {
    // If a specific node ID is provided, show details for that node
    // Otherwise, same as list
    match node_id {
        Some(_id) => {
            // Individual node status will use a future endpoint
            list(output, endpoint).await
        }
        None => list(output, endpoint).await,
    }
}

pub async fn drain(
    node_id: &str,
    deadline_secs: Option<u64>,
    endpoint: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/nodes/{}/drain", server, node_id);

    let body = serde_json::json!({
        "deadline_secs": deadline_secs,
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status().is_success() {
        let result: serde_json::Value = resp.json().await?;
        match output {
            OutputFormat::Json | OutputFormat::Yaml => {
                println!("{}", render_value(&result, output)?);
            }
            _ => {
                println!("Node {} set to draining", node_id);
                if let Some(deadline) = deadline_secs {
                    println!("Deadline: {}s", deadline);
                }
            }
        }
    } else if resp.status() == 404 {
        bail!("Node not found: {}", node_id);
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }
    Ok(())
}

pub async fn eligibility(
    node_id: &str,
    enable: bool,
    endpoint: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/nodes/{}/eligibility", server, node_id);

    let body = serde_json::json!({
        "eligible": enable,
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status().is_success() {
        let result: serde_json::Value = resp.json().await?;
        match output {
            OutputFormat::Json | OutputFormat::Yaml => {
                println!("{}", render_value(&result, output)?);
            }
            _ => {
                println!(
                    "Node {} eligibility: {}",
                    node_id,
                    if enable { "enabled" } else { "disabled" }
                );
            }
        }
    } else if resp.status() == 404 {
        bail!("Node not found: {}", node_id);
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }
    Ok(())
}
