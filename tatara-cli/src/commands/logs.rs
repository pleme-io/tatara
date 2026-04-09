use anyhow::{bail, Context, Result};

pub async fn run(
    alloc_id: &str,
    task_name: Option<&str>,
    follow: bool,
    server_addr: &str,
) -> Result<()> {
    let client = reqwest::Client::new();

    let mut url = format!("http://{}/api/v1/allocations/{}/logs", server_addr, alloc_id);
    if let Some(task) = task_name {
        url.push_str(&format!("?task={}", task));
    }

    let resp = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status().is_success() {
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
                    // In a real implementation, we'd track the last seen offset.
                    // Phase 1: just re-print all logs.
                    for entry in &entries {
                        let stream = entry["stream"].as_str().unwrap_or("out");
                        let message = entry["message"].as_str().unwrap_or("");
                        let task = entry["task_name"].as_str().unwrap_or("?");
                        println!("[{}][{}] {}", task, stream, message);
                    }
                }
            }
        }
    } else if resp.status() == 404 {
        bail!("Allocation not found: {}", alloc_id);
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    Ok(())
}
