//! axum HTTP handler — verify signature, dispatch on event kind, apply
//! resulting Allocation via kube-rs.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use kube::api::{Api, DeleteParams, PostParams};
use kube::Client;
use tracing::{info, warn};

use tatara_process::allocation::EphemeralAllocation;

use crate::allocation_factory::{allocation_name, build_allocation, FactoryError};
use crate::config::WatcherConfig;
use crate::event::{EventKind, PullRequestEvent};
use crate::verify::verify_signature;

/// Handler state shared across requests.
#[derive(Clone)]
pub struct HandlerState {
    pub config: Arc<WatcherConfig>,
    pub kube: Client,
}

/// POST handler for GitHub webhooks.
pub async fn webhook(
    State(state): State<HandlerState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // 1. Verify HMAC.
    let sig_header = headers
        .get("X-Hub-Signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Err(e) = verify_signature(sig_header, &body, state.config.secret.as_bytes()) {
        warn!(error = %e, "webhook signature verification failed");
        return (StatusCode::UNAUTHORIZED, format!("signature: {e}")).into_response();
    }

    // 2. Dispatch on event kind.
    let event_header = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let kind = EventKind::from_header(event_header);

    match kind {
        EventKind::PullRequest => handle_pr_event(&state, &body).await,
        EventKind::Push => {
            // Push events handled by a separate path (e.g., main-branch
            // attestation runs). v0 just acknowledges.
            (StatusCode::OK, "push event acknowledged (not allocated)").into_response()
        }
        EventKind::Other => (StatusCode::OK, "event ignored").into_response(),
    }
}

async fn handle_pr_event(state: &HandlerState, body: &[u8]) -> axum::response::Response {
    let evt: PullRequestEvent = match serde_json::from_slice(body) {
        Ok(e) => e,
        Err(e) => {
            warn!(error = %e, "failed to parse PR event");
            return (StatusCode::BAD_REQUEST, format!("parse: {e}")).into_response();
        }
    };

    // Repo allowlist.
    if !state.config.allow_repos.is_empty() && !repo_allowed(&evt.repository.full_name, &state.config.allow_repos) {
        info!(repo = %evt.repository.full_name, "repo not in allowlist; skipping");
        return (StatusCode::OK, "repo not in allowlist").into_response();
    }

    use crate::event::PrAction;
    match evt.action {
        PrAction::Closed => {
            // Delete the allocation; pool reconciler returns the member.
            let name = allocation_name(&evt.repository.full_name, evt.number);
            let api: Api<EphemeralAllocation> =
                Api::namespaced(state.kube.clone(), &state.config.namespace);
            match api.delete(&name, &DeleteParams::default()).await {
                Ok(_) => {
                    info!(
                        namespace = %state.config.namespace,
                        allocation = %name,
                        "closed PR → deleted Allocation"
                    );
                    (StatusCode::OK, "allocation deleted").into_response()
                }
                Err(kube::Error::Api(e)) if e.code == 404 => {
                    (StatusCode::OK, "allocation already gone").into_response()
                }
                Err(e) => {
                    warn!(error = %e, "delete failed");
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("delete: {e}"))
                        .into_response()
                }
            }
        }
        PrAction::Opened | PrAction::Reopened | PrAction::Synchronize => {
            // Build + create-or-replace the allocation.
            let alloc = match build_allocation(
                &evt,
                &state.config.namespace,
                state.config.pin_pool.as_deref(),
                state.config.include_drafts,
            ) {
                Ok(a) => a,
                Err(FactoryError::DraftExcluded) => {
                    info!("draft PR — skipping allocation");
                    return (StatusCode::OK, "draft excluded").into_response();
                }
                Err(FactoryError::NotAllocatable(_)) => {
                    return (StatusCode::OK, "action not allocatable").into_response();
                }
            };
            let api: Api<EphemeralAllocation> =
                Api::namespaced(state.kube.clone(), &state.config.namespace);
            match api.create(&PostParams::default(), &alloc).await {
                Ok(_) => {
                    info!(
                        namespace = %state.config.namespace,
                        allocation = alloc.metadata.name.as_deref().unwrap_or("?"),
                        pr_number = evt.number,
                        repo = %evt.repository.full_name,
                        "PR event → created Allocation"
                    );
                    (StatusCode::CREATED, "allocation created").into_response()
                }
                Err(kube::Error::Api(e)) if e.code == 409 => {
                    // Already exists — refresh via PATCH (synchronize event).
                    (StatusCode::OK, "allocation already exists (synchronize)").into_response()
                }
                Err(e) => {
                    warn!(error = %e, "create allocation failed");
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("create: {e}"))
                        .into_response()
                }
            }
        }
        PrAction::Other => (StatusCode::OK, "action ignored").into_response(),
    }
}

fn repo_allowed(repo: &str, allowlist: &[String]) -> bool {
    allowlist.iter().any(|p| repo_matches(p, repo))
}

fn repo_matches(pattern: &str, repo: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/*") {
        repo.starts_with(&format!("{prefix}/"))
    } else {
        pattern == repo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_matches_exact() {
        assert!(repo_matches("pleme-io/akeyless", "pleme-io/akeyless"));
        assert!(!repo_matches("pleme-io/akeyless", "pleme-io/other"));
    }

    #[test]
    fn repo_matches_org_wildcard() {
        assert!(repo_matches("pleme-io/*", "pleme-io/akeyless"));
        assert!(repo_matches("pleme-io/*", "pleme-io/tatara"));
        assert!(!repo_matches("pleme-io/*", "drzln/dotfiles"));
    }

    #[test]
    fn empty_allowlist_skipped_at_caller() {
        // The caller's check `!allowlist.is_empty()` gates this function;
        // sanity test that an empty allowlist would reject everything if
        // called directly.
        assert!(!repo_allowed("anything", &[]));
    }

    #[test]
    fn allowlist_with_one_pattern_filters() {
        let allow = vec!["pleme-io/*".to_string()];
        assert!(repo_allowed("pleme-io/akeyless", &allow));
        assert!(!repo_allowed("drzln/dotfiles", &allow));
    }
}
