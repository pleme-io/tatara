use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use kube::api::{Api, ListParams, ObjectMeta, Patch, PatchParams, PostParams};
use kube::runtime::controller::Action;
use kube::{Client, Resource, ResourceExt};
use serde_json::json;
use tracing::{debug, info, warn};

use crate::crds::flake_org::{FlakeOrg, FlakeOrgStatus, OrgRepoStatus};
use crate::crds::flake_source::{FlakeOutput, FlakeSource, FlakeSourceSpec};
use crate::utils;

pub struct FlakeOrgContext {
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

/// Reconcile a FlakeOrg CR.
///
/// 1. List repos in the GitHub org (paginated).
/// 2. For each repo, check for flake.nix.
/// 3. Create/update FlakeSource CRs for repos with flake.nix.
/// 4. Delete orphaned FlakeSource CRs for removed repos.
/// 5. Update FlakeOrg status.
pub async fn reconcile(
    org: Arc<FlakeOrg>,
    ctx: Arc<FlakeOrgContext>,
) -> Result<Action, ReconcileError> {
    let name = org.name_any();
    let namespace = org.namespace().unwrap_or_default();
    let org_api: Api<FlakeOrg> = Api::namespaced(ctx.kube_client.clone(), &namespace);
    let source_api: Api<FlakeSource> = Api::namespaced(ctx.kube_client.clone(), &namespace);

    info!(org = %org.spec.org, "Scanning GitHub org");

    // List repos in the org
    let repos = match list_org_repos(
        &ctx.http_client,
        &org.spec.org,
        ctx.github_token.as_deref(),
    )
    .await
    {
        Ok(repos) => repos,
        Err(e) => {
            warn!(org = %org.spec.org, error = %e, "Failed to list org repos");
            let status = json!({
                "status": {
                    "lastScanned": Utc::now().to_rfc3339(),
                    "lastError": format!("{e}"),
                }
            });
            org_api
                .patch_status(&name, &PatchParams::apply("tatara-operator"), &Patch::Merge(&status))
                .await?;
            return Ok(Action::requeue(Duration::from_secs(120)));
        }
    };

    // Filter repos
    let filtered: Vec<_> = repos
        .iter()
        .filter(|r| {
            if org.spec.exclude.contains(&r.name) {
                return false;
            }
            if let Some(include) = &org.spec.include {
                return include.contains(&r.name);
            }
            true
        })
        .collect();

    let mut repo_statuses = Vec::new();
    let mut flake_count = 0u32;
    let mut skipped_count = 0u32;
    let mut created_sources = Vec::new();

    for repo in &filtered {
        let has_flake = if org.spec.auto_detect_flakes {
            match has_flake_nix(
                &ctx.http_client,
                &org.spec.org,
                &repo.name,
                ctx.github_token.as_deref(),
            )
            .await
            {
                Ok(v) => v,
                Err(e) => {
                    debug!(repo = %repo.name, error = %e, "Failed to check flake.nix, skipping");
                    false
                }
            }
        } else {
            true // Assume all repos have flakes if auto_detect is off
        };

        if !has_flake && org.spec.skip_non_flake {
            debug!(repo = %repo.name, "No flake.nix, skipping");
            skipped_count += 1;
            repo_statuses.push(OrgRepoStatus {
                repo: repo.name.clone(),
                has_flake: false,
                flake_source_ref: None,
                last_checked: Some(Utc::now()),
            });
            continue;
        }

        flake_count += 1;

        // Create FlakeSource CR name
        let source_name = utils::truncate_k8s_name(&format!("{}-{}", org.spec.org, repo.name));
        created_sources.push(source_name.clone());

        // Build the FlakeSource spec
        let outputs = vec![FlakeOutput {
            attr: format!("packages.{}.default", org.spec.default_system),
            system: org.spec.default_system.clone(),
            extra_args: org.spec.extra_args.clone(),
        }];

        let fs_spec = FlakeSourceSpec {
            repo: format!("github:{}/{}", org.spec.org, repo.name),
            branch: repo.default_branch.clone().unwrap_or_else(|| org.spec.default_branch.clone()),
            poll_interval: org.spec.default_source_poll_interval.clone(),
            webhook_secret_ref: None,
            outputs,
            attic_cache: org.spec.default_attic_cache.clone(),
            build_on_create: true,
            extra_args: org.spec.extra_args.clone(),
        };

        // Check if FlakeSource already exists
        let existing = source_api.get_opt(&source_name).await?;

        if existing.is_none() {
            let mut fs = FlakeSource::new(&source_name, fs_spec);
            fs.metadata = ObjectMeta {
                name: Some(source_name.clone()),
                namespace: Some(namespace.clone()),
                owner_references: Some(vec![org.controller_owner_ref(&()).unwrap()]),
                labels: Some(
                    [
                        ("tatara.pleme.io/flake-org".to_string(), name.clone()),
                        ("tatara.pleme.io/managed-by".to_string(), "flake-org".to_string()),
                    ]
                    .into(),
                ),
                ..Default::default()
            };

            match source_api.create(&PostParams::default(), &fs).await {
                Ok(_) => info!(source = %source_name, repo = %repo.name, "Created FlakeSource"),
                Err(kube::Error::Api(err)) if err.code == 409 => {
                    debug!(source = %source_name, "FlakeSource already exists");
                }
                Err(e) => {
                    warn!(source = %source_name, error = %e, "Failed to create FlakeSource");
                }
            }
        }

        repo_statuses.push(OrgRepoStatus {
            repo: repo.name.clone(),
            has_flake: true,
            flake_source_ref: Some(source_name),
            last_checked: Some(Utc::now()),
        });
    }

    // Delete orphaned FlakeSource CRs (owned by this FlakeOrg but repo no longer in org)
    let owned_sources = source_api
        .list(&ListParams::default().labels(&format!("tatara.pleme.io/flake-org={name}")))
        .await?;

    for fs in owned_sources.items {
        let fs_name = fs.name_any();
        if !created_sources.contains(&fs_name) {
            info!(source = %fs_name, "Deleting orphaned FlakeSource");
            let _ = source_api.delete(&fs_name, &Default::default()).await;
        }
    }

    // Update status
    let status = json!({
        "status": FlakeOrgStatus {
            last_scanned: Some(Utc::now()),
            discovered_repos: filtered.len() as u32,
            flake_repos: flake_count,
            skipped_repos: skipped_count,
            repo_statuses,
            last_error: None,
        }
    });
    org_api
        .patch_status(&name, &PatchParams::apply("tatara-operator"), &Patch::Merge(&status))
        .await?;

    info!(
        org = %org.spec.org,
        discovered = filtered.len(),
        flakes = flake_count,
        skipped = skipped_count,
        "Org scan complete"
    );

    Ok(Action::requeue(utils::parse_interval(&org.spec.poll_interval)))
}

pub fn error_policy(
    _org: Arc<FlakeOrg>,
    error: &ReconcileError,
    _ctx: Arc<FlakeOrgContext>,
) -> Action {
    warn!(error = %error, "FlakeOrg reconcile error, retrying in 120s");
    Action::requeue(Duration::from_secs(120))
}

// ── GitHub API helpers ───────────────────────────────────────────────────

#[derive(Debug)]
struct OrgRepo {
    name: String,
    default_branch: Option<String>,
}

/// List all repos in a GitHub org (paginated).
async fn list_org_repos(
    client: &reqwest::Client,
    org: &str,
    token: Option<&str>,
) -> Result<Vec<OrgRepo>, String> {
    let mut repos = Vec::new();
    let mut page = 1u32;

    loop {
        let url = format!(
            "https://api.github.com/orgs/{org}/repos?per_page=100&page={page}&type=all"
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

        let body: Vec<serde_json::Value> = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {e}"))?;

        if body.is_empty() {
            break;
        }

        for repo in &body {
            if let Some(name) = repo["name"].as_str() {
                repos.push(OrgRepo {
                    name: name.to_string(),
                    default_branch: repo["default_branch"].as_str().map(|s| s.to_string()),
                });
            }
        }

        if body.len() < 100 {
            break; // Last page
        }
        page += 1;
    }

    Ok(repos)
}

/// Check if a repo has flake.nix at the root.
async fn has_flake_nix(
    client: &reqwest::Client,
    org: &str,
    repo: &str,
    token: Option<&str>,
) -> Result<bool, String> {
    let url = format!("https://api.github.com/repos/{org}/{repo}/contents/flake.nix");

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

    Ok(resp.status().is_success())
}
