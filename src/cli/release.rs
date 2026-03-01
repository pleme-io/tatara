use anyhow::{bail, Context, Result};

use super::context::{active_endpoint, endpoint_to_server};
use super::output::{build_table, human_duration_since, render_value, status_cell, OutputFormat};

pub async fn list(output: OutputFormat, endpoint: Option<&str>) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/releases", server);

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    let releases: Vec<serde_json::Value> = resp.json().await?;

    if releases.is_empty() {
        println!("No releases.");
        return Ok(());
    }

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&releases, output)?);
        }
        _ => {
            let mut table = build_table(&[
                "ID", "NAME", "FLAKE REF", "VERSION", "STATUS", "CREATED",
            ]);
            for rel in &releases {
                let id_str = rel["id"].as_str().unwrap_or("?");
                let short_id = id_str.get(..8).unwrap_or(id_str);
                table.add_row(vec![
                    comfy_table::Cell::new(short_id),
                    comfy_table::Cell::new(rel["name"].as_str().unwrap_or("?")),
                    comfy_table::Cell::new(rel["flake_ref"].as_str().unwrap_or("?")),
                    comfy_table::Cell::new(
                        rel["version"].as_u64().unwrap_or(0).to_string(),
                    ),
                    status_cell(rel["status"].as_str().unwrap_or("?")),
                    comfy_table::Cell::new(human_duration_since(
                        rel["created_at"].as_str().unwrap_or(""),
                    )),
                ]);
            }
            println!("{table}");
        }
    }
    Ok(())
}

pub async fn get(release_id: &str, output: OutputFormat, endpoint: Option<&str>) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/releases/{}", server, release_id);

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status() == 404 {
        bail!("Release not found: {}", release_id);
    }
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    let release: serde_json::Value = resp.json().await?;

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&release, output)?);
        }
        _ => {
            println!("Release: {}", release["id"].as_str().unwrap_or("?"));
            println!("  Name:      {}", release["name"].as_str().unwrap_or("?"));
            println!(
                "  Flake Ref: {}",
                release["flake_ref"].as_str().unwrap_or("?")
            );
            if let Some(rev) = release["flake_rev"].as_str() {
                println!("  Flake Rev: {}", rev);
            }
            println!("  Version:   {}", release["version"].as_u64().unwrap_or(0));
            println!("  Status:    {}", release["status"].as_str().unwrap_or("?"));
            println!(
                "  Created:   {}",
                release["created_at"].as_str().unwrap_or("?")
            );
        }
    }
    Ok(())
}

pub async fn promote(
    release_id: &str,
    endpoint: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/releases/{}/promote", server, release_id);

    let resp = client
        .post(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status().is_success() {
        let release: serde_json::Value = resp.json().await?;
        match output {
            OutputFormat::Json | OutputFormat::Yaml => {
                println!("{}", render_value(&release, output)?);
            }
            _ => {
                println!(
                    "Release {} promoted to active",
                    release["id"].as_str().unwrap_or(release_id)
                );
            }
        }
    } else if resp.status() == 404 {
        bail!("Release not found: {}", release_id);
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }
    Ok(())
}

pub async fn rollback(
    release_id: &str,
    endpoint: Option<&str>,
    output: OutputFormat,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/releases/{}/rollback", server, release_id);

    let resp = client
        .post(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status().is_success() {
        let release: serde_json::Value = resp.json().await?;
        match output {
            OutputFormat::Json | OutputFormat::Yaml => {
                println!("{}", render_value(&release, output)?);
            }
            _ => {
                println!(
                    "Release {} rolled back",
                    release["id"].as_str().unwrap_or(release_id)
                );
            }
        }
    } else if resp.status() == 404 {
        bail!("Release not found: {}", release_id);
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }
    Ok(())
}
