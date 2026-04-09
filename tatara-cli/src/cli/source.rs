use anyhow::{bail, Context, Result};

use super::context::{active_endpoint, endpoint_to_server};
use super::output::{build_table, human_duration_since, render_value, status_cell, OutputFormat};

pub async fn list(output: OutputFormat, endpoint: Option<&str>) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/sources", server);

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    let sources: Vec<serde_json::Value> = resp.json().await?;

    if sources.is_empty() {
        println!("No sources.");
        return Ok(());
    }

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&sources, output)?);
        }
        _ => {
            let mut table = build_table(&[
                "ID", "NAME", "FLAKE REF", "STATUS", "REV", "JOBS", "LAST RECONCILED",
            ]);
            for src in &sources {
                let id_str = src["id"].as_str().unwrap_or("?");
                let short_id = id_str.get(..8).unwrap_or(id_str);
                let rev = src["last_rev"]
                    .as_str()
                    .map(|r| r.get(..8).unwrap_or(r))
                    .unwrap_or("-");
                let job_count = src["managed_jobs"]
                    .as_object()
                    .map(|m| m.len())
                    .unwrap_or(0);
                table.add_row(vec![
                    comfy_table::Cell::new(short_id),
                    comfy_table::Cell::new(src["name"].as_str().unwrap_or("?")),
                    comfy_table::Cell::new(src["flake_ref"].as_str().unwrap_or("?")),
                    status_cell(src["status"].as_str().unwrap_or("?")),
                    comfy_table::Cell::new(rev),
                    comfy_table::Cell::new(job_count.to_string()),
                    comfy_table::Cell::new(human_duration_since(
                        src["last_reconciled_at"].as_str().unwrap_or(""),
                    )),
                ]);
            }
            println!("{table}");
        }
    }
    Ok(())
}

pub async fn get(name_or_id: &str, output: OutputFormat, endpoint: Option<&str>) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();

    // Try by ID first, then by name (list + filter)
    let source = resolve_source(&client, &server, name_or_id).await?;

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&source, output)?);
        }
        _ => {
            println!("Source: {}", source["id"].as_str().unwrap_or("?"));
            println!("  Name:       {}", source["name"].as_str().unwrap_or("?"));
            println!(
                "  Flake Ref:  {}",
                source["flake_ref"].as_str().unwrap_or("?")
            );
            println!("  Kind:       {}", source["kind"].as_str().unwrap_or("?"));
            println!("  Status:     {}", source["status"].as_str().unwrap_or("?"));
            if let Some(rev) = source["last_rev"].as_str() {
                println!("  Last Rev:   {}", rev);
            }
            if let Some(err) = source["last_error"].as_str() {
                println!("  Last Error: {}", err);
            }
            if let Some(at) = source["last_reconciled_at"].as_str() {
                println!("  Reconciled: {}", at);
            }
            println!(
                "  Created:    {}",
                source["created_at"].as_str().unwrap_or("?")
            );
            if let Some(jobs) = source["managed_jobs"].as_object() {
                if !jobs.is_empty() {
                    println!("  Managed Jobs:");
                    for (name, hash) in jobs {
                        let short_hash = hash
                            .as_str()
                            .map(|h| h.get(..8).unwrap_or(h))
                            .unwrap_or("?");
                        println!("    - {} (hash: {})", name, short_hash);
                    }
                }
            }
        }
    }
    Ok(())
}

pub async fn add(
    name: &str,
    flake_ref: &str,
    kind: &str,
    endpoint: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/sources", server);

    let kind_value = match kind {
        "git-flake" | "git_flake" => "git_flake",
        "flake-output" | "flake_output" => "flake_output",
        _ => bail!("Invalid source kind: {}. Use 'git-flake' or 'flake-output'", kind),
    };

    let body = serde_json::json!({
        "name": name,
        "flake_ref": flake_ref,
        "kind": kind_value,
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status().is_success() {
        let source: serde_json::Value = resp.json().await?;
        match output {
            OutputFormat::Json | OutputFormat::Yaml => {
                println!("{}", render_value(&source, output)?);
            }
            _ => {
                println!(
                    "Source '{}' added (id: {})",
                    name,
                    source["id"].as_str().unwrap_or("?")
                );
            }
        }
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }
    Ok(())
}

pub async fn delete(
    name_or_id: &str,
    endpoint: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();

    let source = resolve_source(&client, &server, name_or_id).await?;
    let id = source["id"].as_str().unwrap_or(name_or_id);

    let url = format!("http://{}/api/v1/sources/{}", server, id);
    let resp = client
        .delete(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status().is_success() {
        match output {
            OutputFormat::Json | OutputFormat::Yaml => {
                let result: serde_json::Value = resp.json().await?;
                println!("{}", render_value(&result, output)?);
            }
            _ => {
                println!("Source '{}' deleted", name_or_id);
            }
        }
    } else if resp.status() == 404 {
        bail!("Source not found: {}", name_or_id);
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }
    Ok(())
}

pub async fn sync(
    name_or_id: &str,
    endpoint: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();

    let source = resolve_source(&client, &server, name_or_id).await?;
    let id = source["id"].as_str().unwrap_or(name_or_id);

    let url = format!("http://{}/api/v1/sources/{}/sync", server, id);
    let resp = client
        .post(&url)
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
                println!("Source '{}' sync triggered", name_or_id);
            }
        }
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }
    Ok(())
}

pub async fn suspend(
    name_or_id: &str,
    endpoint: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();

    let source = resolve_source(&client, &server, name_or_id).await?;
    let id = source["id"].as_str().unwrap_or(name_or_id);

    let url = format!("http://{}/api/v1/sources/{}/suspend", server, id);
    let resp = client
        .post(&url)
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
                println!("Source '{}' suspended", name_or_id);
            }
        }
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }
    Ok(())
}

pub async fn resume(
    name_or_id: &str,
    endpoint: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();

    let source = resolve_source(&client, &server, name_or_id).await?;
    let id = source["id"].as_str().unwrap_or(name_or_id);

    let url = format!("http://{}/api/v1/sources/{}/resume", server, id);
    let resp = client
        .post(&url)
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
                println!("Source '{}' resumed", name_or_id);
            }
        }
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }
    Ok(())
}

/// Resolve a source by name or UUID. First tries as UUID, then searches by name.
async fn resolve_source(
    client: &reqwest::Client,
    server: &str,
    name_or_id: &str,
) -> Result<serde_json::Value> {
    // Try by ID first
    if let Ok(uuid) = name_or_id.parse::<uuid::Uuid>() {
        let url = format!("http://{}/api/v1/sources/{}", server, uuid);
        let resp = client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to tatara server")?;

        if resp.status().is_success() {
            return Ok(resp.json().await?);
        }
    }

    // Fall back to name lookup
    let url = format!("http://{}/api/v1/sources", server);
    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    let sources: Vec<serde_json::Value> = resp.json().await?;
    sources
        .into_iter()
        .find(|s| s["name"].as_str() == Some(name_or_id))
        .ok_or_else(|| anyhow::anyhow!("Source not found: {}", name_or_id))
}
