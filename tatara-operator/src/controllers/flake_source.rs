use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use kube::api::{Api, ObjectMeta, Patch, PatchParams, PostParams};
use kube::runtime::controller::Action;
use kube::{Client, Resource, ResourceExt};
use serde_json::json;
use tracing::{info, warn};

use crate::crds::flake_source::{FlakeSource, FlakeSourceStatus, OutputState, OutputStatus};
use crate::crds::nix_build::{NixBuild, NixBuildPhase, NixBuildSpec};

pub struct FlakeSourceContext {
    pub kube_client: Client,
    pub http_client: reqwest::Client,
    pub github_token: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[error("Kube error: {0}")]
    Kube(#[from] kube::Error),

    #[error("GitHub API error: {0}")]
    GitHub(String),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Reconcile a FlakeSource CR.
///
/// 1. Poll GitHub for the latest commit on the watched branch.
/// 2. If the commit differs from status.last_commit, create NixBuild CRs for each output.
/// 3. Update FlakeSource status.
/// 4. Requeue after poll_interval.
pub async fn reconcile(
    source: Arc<FlakeSource>,
    ctx: Arc<FlakeSourceContext>,
) -> Result<Action, ReconcileError> {
    let name = source.name_any();
    let namespace = source.namespace().unwrap_or_default();
    let api: Api<FlakeSource> = Api::namespaced(ctx.kube_client.clone(), &namespace);
    let builds_api: Api<NixBuild> = Api::namespaced(ctx.kube_client.clone(), &namespace);

    let (owner, repo) = parse_github_repo(&source.spec.repo);
    let branch = &source.spec.branch;

    // Poll GitHub for the latest commit
    let latest_commit = match get_latest_commit(
        &ctx.http_client,
        &owner,
        &repo,
        branch,
        ctx.github_token.as_deref(),
    )
    .await
    {
        Ok(sha) => sha,
        Err(e) => {
            warn!(source = %name, error = %e, "Failed to poll GitHub");
            let status = json!({
                "status": {
                    "lastPolled": Utc::now().to_rfc3339(),
                    "lastError": format!("{e}"),
                }
            });
            api.patch_status(&name, &PatchParams::apply("tatara-operator"), &Patch::Merge(&status))
                .await?;
            return Ok(Action::requeue(parse_interval(&source.spec.poll_interval)));
        }
    };

    let last_commit = source
        .status
        .as_ref()
        .and_then(|s| s.last_commit.clone());

    let is_new = last_commit.as_deref() != Some(&latest_commit);
    let is_first = last_commit.is_none() && source.spec.build_on_create;

    if is_new || is_first {
        info!(
            source = %name,
            commit = %latest_commit,
            previous = ?last_commit,
            "New commit detected, creating NixBuild CRs"
        );

        let mut output_statuses = Vec::new();

        for output in &source.spec.outputs {
            let build_name = format!("{}-{}", name, slugify_attr(&output.attr));
            let build_name = truncate_k8s_name(&build_name);

            // Merge extra args: source-level + output-level
            let mut extra_args = source.spec.extra_args.clone();
            extra_args.extend(output.extra_args.clone());

            let flake_ref = format!("{}#{}", source.spec.repo, output.attr);

            let build = NixBuild::new(
                &build_name,
                NixBuildSpec {
                    flake_ref,
                    system: output.system.clone(),
                    attic_cache: Some(source.spec.attic_cache.clone()),
                    extra_args,
                    priority: 0,
                },
            );

            // Set owner reference so NixBuild CRs are GC'd with the FlakeSource
            let mut build = build;
            build.metadata = ObjectMeta {
                name: Some(build_name.clone()),
                namespace: Some(namespace.clone()),
                owner_references: Some(vec![source.controller_owner_ref(&()).unwrap()]),
                labels: Some(
                    [
                        ("tatara.pleme.io/flake-source".to_string(), name.clone()),
                        (
                            "tatara.pleme.io/commit".to_string(),
                            latest_commit[..8.min(latest_commit.len())].to_string(),
                        ),
                    ]
                    .into(),
                ),
                ..Default::default()
            };

            // Create or replace the NixBuild CR
            match builds_api.create(&PostParams::default(), &build).await {
                Ok(_) => {
                    info!(build = %build_name, "Created NixBuild CR");
                }
                Err(kube::Error::Api(err)) if err.code == 409 => {
                    // Already exists — delete and recreate for the new commit
                    let _ = builds_api
                        .delete(&build_name, &Default::default())
                        .await;
                    builds_api.create(&PostParams::default(), &build).await?;
                    info!(build = %build_name, "Replaced existing NixBuild CR");
                }
                Err(e) => return Err(e.into()),
            }

            output_statuses.push(OutputStatus {
                attr: output.attr.clone(),
                state: OutputState::Pending,
                store_path: None,
                build_ref: Some(build_name),
                last_built: None,
            });
        }

        // Update FlakeSource status
        let status = json!({
            "status": FlakeSourceStatus {
                last_polled: Some(Utc::now()),
                last_commit: Some(latest_commit),
                previous_commit: last_commit,
                cached_outputs: 0,
                total_outputs: source.spec.outputs.len() as u32,
                output_statuses,
                last_error: None,
            }
        });
        api.patch_status(
            &name,
            &PatchParams::apply("tatara-operator"),
            &Patch::Merge(&status),
        )
        .await?;
    } else {
        // No new commit — check build progress for existing outputs
        let mut updated = false;
        if let Some(status) = &source.status {
            let mut output_statuses = status.output_statuses.clone();
            let mut cached = 0u32;

            for os in &mut output_statuses {
                if let Some(build_ref) = &os.build_ref {
                    if let Ok(build) = builds_api.get(build_ref).await {
                        if let Some(bs) = &build.status {
                            let new_state = match &bs.phase {
                                NixBuildPhase::Complete => {
                                    os.store_path = bs.store_path.clone();
                                    os.last_built = Some(Utc::now());
                                    OutputState::Cached
                                }
                                NixBuildPhase::Failed => OutputState::Failed,
                                NixBuildPhase::Building
                                | NixBuildPhase::Pushing
                                | NixBuildPhase::Queued => OutputState::Building,
                                NixBuildPhase::Pending => OutputState::Pending,
                            };
                            if new_state != os.state {
                                os.state = new_state;
                                updated = true;
                            }
                        }
                    }
                }
                if os.state == OutputState::Cached {
                    cached += 1;
                }
            }

            if updated {
                let patch = json!({
                    "status": {
                        "lastPolled": Utc::now().to_rfc3339(),
                        "cachedOutputs": cached,
                        "outputStatuses": output_statuses,
                        "lastError": null,
                    }
                });
                api.patch_status(
                    &name,
                    &PatchParams::apply("tatara-operator"),
                    &Patch::Merge(&patch),
                )
                .await?;
            }
        }
    }

    Ok(Action::requeue(parse_interval(&source.spec.poll_interval)))
}

pub fn error_policy(
    _source: Arc<FlakeSource>,
    error: &ReconcileError,
    _ctx: Arc<FlakeSourceContext>,
) -> Action {
    warn!(error = %error, "FlakeSource reconcile error, retrying in 60s");
    Action::requeue(Duration::from_secs(60))
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Parse "github:owner/repo" → ("owner", "repo")
fn parse_github_repo(repo: &str) -> (String, String) {
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

/// Turn "packages.x86_64-linux.akeyless-backend-auth" → "akeyless-backend-auth"
fn slugify_attr(attr: &str) -> String {
    attr.rsplit('.').next().unwrap_or(attr).to_string()
}

/// Truncate to 63 chars (K8s name limit), trim trailing hyphens
fn truncate_k8s_name(name: &str) -> String {
    let mut s: String = name.chars().take(63).collect();
    while s.ends_with('-') {
        s.pop();
    }
    s
}

/// Parse interval strings like "5m", "1h", "30s" → Duration
fn parse_interval(interval: &str) -> Duration {
    let interval = interval.trim();
    let (num_str, unit) = interval.split_at(interval.len().saturating_sub(1));
    let num: u64 = num_str.parse().unwrap_or(5);
    match unit {
        "s" => Duration::from_secs(num),
        "m" => Duration::from_secs(num * 60),
        "h" => Duration::from_secs(num * 3600),
        _ => Duration::from_secs(300), // default 5m
    }
}

/// GET /repos/{owner}/{repo}/commits/{branch} → commit SHA
async fn get_latest_commit(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    branch: &str,
    token: Option<&str>,
) -> Result<String, String> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/commits/{branch}"
    );

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_repo() {
        let (owner, repo) = parse_github_repo("github:pleme-io/blackmatter-akeyless");
        assert_eq!(owner, "pleme-io");
        assert_eq!(repo, "blackmatter-akeyless");
    }

    #[test]
    fn test_slugify_attr() {
        assert_eq!(
            slugify_attr("packages.x86_64-linux.akeyless-backend-auth"),
            "akeyless-backend-auth"
        );
        assert_eq!(slugify_attr("default"), "default");
    }

    #[test]
    fn test_parse_interval() {
        assert_eq!(parse_interval("5m"), Duration::from_secs(300));
        assert_eq!(parse_interval("1h"), Duration::from_secs(3600));
        assert_eq!(parse_interval("30s"), Duration::from_secs(30));
    }

    #[test]
    fn test_truncate_k8s_name() {
        let long = "a".repeat(100);
        assert_eq!(truncate_k8s_name(&long).len(), 63);
    }
}
