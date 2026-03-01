use anyhow::{bail, Context, Result};

use super::context::{active_endpoint, endpoint_to_server};
use super::output::{build_table, human_duration_since, render_value, status_cell, OutputFormat};

pub async fn list(
    kind: Option<&str>,
    since: Option<&str>,
    output: OutputFormat,
    endpoint: Option<&str>,
) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));
    let client = reqwest::Client::new();

    let mut url = format!("http://{}/api/v1/events", server);
    let mut params = Vec::new();
    if let Some(k) = kind {
        params.push(format!("kind={}", k));
    }
    if let Some(s) = since {
        params.push(format!("since={}", s));
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    let events: Vec<serde_json::Value> = resp.json().await?;

    if events.is_empty() {
        println!("No events.");
        return Ok(());
    }

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&events, output)?);
        }
        _ => {
            let mut table = build_table(&["TIME", "KIND", "DETAILS"]);
            for event in &events {
                let kind_str = event["kind"].as_str().unwrap_or("?");
                table.add_row(vec![
                    comfy_table::Cell::new(human_duration_since(
                        event["timestamp"].as_str().unwrap_or(""),
                    )),
                    status_cell(kind_str),
                    comfy_table::Cell::new(
                        serde_json::to_string(&event["payload"])
                            .unwrap_or_default()
                            .chars()
                            .take(80)
                            .collect::<String>(),
                    ),
                ]);
            }
            println!("{table}");
        }
    }
    Ok(())
}

pub async fn stream(kind: Option<&str>, endpoint: Option<&str>) -> Result<()> {
    let server = endpoint_to_server(&active_endpoint(endpoint));

    let mut url = format!("http://{}/api/v1/events/stream", server);
    if let Some(k) = kind {
        url.push_str(&format!("?kind={}", k));
    }

    println!("Streaming events from {} (Ctrl+C to stop)...", server);

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    // Read SSE stream line by line
    use futures_core::Stream;
    use tokio_stream::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(result) = stream.next().await {
        let chunk = result.map_err(|e| anyhow::anyhow!("Stream error: {}", e))?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        // Process complete SSE messages (delimited by double newlines)
        while let Some(pos) = buffer.find("\n\n") {
            let message = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            for line in message.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                        let kind_str = event["kind"].as_str().unwrap_or("?");
                        let ts = event["timestamp"].as_str().unwrap_or("?");
                        let payload = serde_json::to_string(&event["payload"])
                            .unwrap_or_default();
                        println!("[{}] {} {}", ts, kind_str, payload);
                    }
                }
            }
        }
    }

    Ok(())
}
