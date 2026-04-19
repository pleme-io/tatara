use anyhow::{bail, Context, Result};

pub async fn run(job_id: Option<&str>, server_addr: &str) -> Result<()> {
    let client = reqwest::Client::new();

    match job_id {
        Some(id) => {
            let url = format!("http://{}/api/v1/jobs/{}", server_addr, id);
            let resp = client
                .get(&url)
                .send()
                .await
                .context("Failed to connect to tatara server")?;

            if resp.status().is_success() {
                let detail: serde_json::Value = resp.json().await?;
                let job = &detail["job"];
                println!("Job: {}", job["id"].as_str().unwrap_or("?"));
                println!("  Type:      {}", job["job_type"].as_str().unwrap_or("?"));
                println!("  Status:    {}", job["status"].as_str().unwrap_or("?"));
                println!(
                    "  Submitted: {}",
                    job["submitted_at"].as_str().unwrap_or("?")
                );
                println!(
                    "  Groups:    {}",
                    job["groups"].as_array().map(|g| g.len()).unwrap_or(0)
                );

                if let Some(allocs) = detail["allocations"].as_array() {
                    println!("\n  Allocations:");
                    for alloc in allocs {
                        println!(
                            "    {} ({}) — {} on {}",
                            alloc["id"].as_str().unwrap_or("?"),
                            alloc["group_name"].as_str().unwrap_or("?"),
                            alloc["state"].as_str().unwrap_or("?"),
                            alloc["node_id"].as_str().unwrap_or("?"),
                        );
                    }
                }
            } else if resp.status() == 404 {
                bail!("Job not found: {}", id);
            } else {
                let body = resp.text().await.unwrap_or_default();
                bail!("Server error: {}", body);
            }
        }
        None => {
            let url = format!("http://{}/api/v1/jobs", server_addr);
            let resp = client
                .get(&url)
                .send()
                .await
                .context("Failed to connect to tatara server")?;

            if resp.status().is_success() {
                let jobs: Vec<serde_json::Value> = resp.json().await?;
                if jobs.is_empty() {
                    println!("No jobs.");
                } else {
                    println!(
                        "{:<20} {:<10} {:<10} {:<8}",
                        "ID", "TYPE", "STATUS", "GROUPS"
                    );
                    for job in &jobs {
                        println!(
                            "{:<20} {:<10} {:<10} {:<8}",
                            job["id"].as_str().unwrap_or("?"),
                            job["job_type"].as_str().unwrap_or("?"),
                            job["status"].as_str().unwrap_or("?"),
                            job["groups"].as_array().map(|g| g.len()).unwrap_or(0),
                        );
                    }
                }
            } else {
                let body = resp.text().await.unwrap_or_default();
                bail!("Server error: {}", body);
            }
        }
    }

    Ok(())
}
