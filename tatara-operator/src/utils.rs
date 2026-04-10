//! Shared utilities for tatara-operator controllers.

use std::time::Duration;

/// Parse interval strings like "5m", "1h", "30s" → Duration
pub fn parse_interval(interval: &str) -> Duration {
    let interval = interval.trim();
    let (num_str, unit) = interval.split_at(interval.len().saturating_sub(1));
    let num: u64 = num_str.parse().unwrap_or(5);
    match unit {
        "s" => Duration::from_secs(num),
        "m" => Duration::from_secs(num * 60),
        "h" => Duration::from_secs(num * 3600),
        _ => Duration::from_secs(300),
    }
}

/// Truncate to 63 chars (K8s name limit), trim trailing hyphens
pub fn truncate_k8s_name(name: &str) -> String {
    let mut s: String = name.chars().take(63).collect();
    while s.ends_with('-') {
        s.pop();
    }
    s
}

/// Parse "github:owner/repo" → ("owner", "repo")
pub fn parse_github_repo(repo: &str) -> (String, String) {
    let stripped = repo
        .strip_prefix("github:")
        .unwrap_or(repo)
        .trim_start_matches('/');
    let parts: Vec<&str> = stripped.splitn(2, '/').collect();
    (
        parts.first().unwrap_or(&"").to_string(),
        parts.get(1).unwrap_or(&"").to_string(),
    )
}

/// GET /repos/{owner}/{repo}/commits/{branch} → commit SHA
pub async fn get_latest_commit(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    branch: &str,
    token: Option<&str>,
) -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{branch}");

    let mut req = client
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "tatara-operator/0.1.0");

    if let Some(token) = token {
        req = req.header("Authorization", format!("Bearer {token}"));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "GitHub API returned {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        ));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {e}"))?;

    body["sha"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No 'sha' field in response".to_string())
}
