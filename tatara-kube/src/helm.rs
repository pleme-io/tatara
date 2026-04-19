use crate::error::KubeError;
use tokio::process::Command;
use tracing::info;

/// Render a Helm chart to K8s manifests using `helm template`.
///
/// Used for third-party charts that we don't control (e.g., CloudNativePG, Prometheus).
pub async fn helm_template(
    chart: &str,
    repo: &str,
    version: &str,
    namespace: &str,
    release_name: &str,
    values_json: &serde_json::Value,
) -> Result<Vec<serde_json::Value>, KubeError> {
    info!(chart, version, namespace, "rendering helm template");

    let values_str = serde_json::to_string(values_json).unwrap_or_default();

    let output = Command::new("helm")
        .args([
            "template",
            release_name,
            chart,
            "--repo",
            repo,
            "--version",
            version,
            "--namespace",
            namespace,
            "--values",
            "-",
            "--include-crds",
            "--output-format",
            "json",
        ])
        .env("HELM_CACHE_HOME", "/tmp/helm-cache")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| KubeError::HelmTemplateFailed {
            chart: chart.to_string(),
            reason: e.to_string(),
        })?
        .wait_with_output()
        .await
        .map_err(|e| KubeError::HelmTemplateFailed {
            chart: chart.to_string(),
            reason: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Fallback: try with --set-json for values
        let output2 = Command::new("helm")
            .args([
                "template",
                release_name,
                chart,
                "--repo",
                repo,
                "--version",
                version,
                "--namespace",
                namespace,
                "--set-json",
                &format!("global={}", values_str),
                "--include-crds",
            ])
            .output()
            .await
            .map_err(|e| KubeError::HelmTemplateFailed {
                chart: chart.to_string(),
                reason: e.to_string(),
            })?;

        if !output2.status.success() {
            return Err(KubeError::HelmTemplateFailed {
                chart: chart.to_string(),
                reason: stderr.to_string(),
            });
        }

        // Parse YAML output (multiple documents)
        return parse_yaml_documents(&output2.stdout, chart);
    }

    // Try JSON first
    if let Ok(resources) = serde_json::from_slice::<Vec<serde_json::Value>>(&output.stdout) {
        return Ok(resources);
    }

    // Fallback: parse as YAML multi-document
    parse_yaml_documents(&output.stdout, chart)
}

fn parse_yaml_documents(data: &[u8], chart: &str) -> Result<Vec<serde_json::Value>, KubeError> {
    let text = String::from_utf8_lossy(data);
    let mut resources = Vec::new();

    for doc in text.split("---") {
        let trimmed = doc.trim();
        if trimmed.is_empty() || trimmed == "null" {
            continue;
        }
        match serde_yaml_ng::from_str::<serde_json::Value>(trimmed) {
            Ok(val) if val.get("kind").is_some() => resources.push(val),
            Ok(_) => {} // skip non-resource documents
            Err(e) => {
                tracing::warn!(chart, error = %e, "skipping unparseable YAML document");
            }
        }
    }

    Ok(resources)
}
