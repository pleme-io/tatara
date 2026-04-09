use anyhow::{bail, Context, Result};

pub async fn run(job_id: &str, server_addr: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/jobs/{}/stop", server_addr, job_id);

    let resp = client
        .post(&url)
        .send()
        .await
        .context("Failed to connect to tatara server")?;

    if resp.status().is_success() {
        let job: serde_json::Value = resp.json().await?;
        println!("Job stopped: {}", job["id"].as_str().unwrap_or(job_id));
        println!("Status: {}", job["status"].as_str().unwrap_or("dead"));
    } else if resp.status() == 404 {
        bail!("Job not found: {}", job_id);
    } else {
        let body = resp.text().await.unwrap_or_default();
        bail!("Server error: {}", body);
    }

    Ok(())
}
