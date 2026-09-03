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

impl HandlerState {
    /// Namespaced `Api<EphemeralAllocation>` bound to this handler's
    /// client + configured watcher namespace — the ONE substrate
    /// primitive that owns the `Api::namespaced(self.kube.clone(),
    /// &self.config.namespace)` shape for the github-watcher.
    ///
    /// Pre-lift the two-slot `(self.kube.clone(), &self.config.
    /// namespace)` incantation was hand-authored at TWO sites in
    /// `handler::handle_pr_event`, past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold — the `PrAction::Closed` delete-branch
    /// slot (`api.delete(&name, …)`) plus the `PrAction::{Opened,
    /// Reopened, Synchronize}` create-branch slot (`api.create(&pp,
    /// &alloc)`) each restated the SAME `Api::namespaced(state.kube.
    /// clone(), &state.config.namespace)` chain verbatim. Post-lift
    /// the two consumers share ONE substrate owner; a future emitter
    /// of `Api<EphemeralAllocation>` on the handler reaches for
    /// `state.allocation_api()` rather than re-authoring the two-slot
    /// chain a third time — matching the composition discipline the
    /// peer [`tatara_pool_reconciler::context::PoolContext::
    /// allocation_api`] substrate primitive already establishes on
    /// the pool-reconciler side of the same CRD.
    ///
    /// The typed `Api<EphemeralAllocation>` return pins the resource
    /// kind at rustc time — a future consumer that reaches for a
    /// different CRD via this primitive's client-slot fails to
    /// compile rather than silently issuing a REST request under the
    /// wrong resource plural.
    pub fn allocation_api(&self) -> Api<EphemeralAllocation> {
        Api::namespaced(self.kube.clone(), &self.config.namespace)
    }
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
    if !state.config.allow_repos.is_empty()
        && !repo_allowed(&evt.repository.full_name, &state.config.allow_repos)
    {
        info!(repo = %evt.repository.full_name, "repo not in allowlist; skipping");
        return (StatusCode::OK, "repo not in allowlist").into_response();
    }

    use crate::event::PrAction;
    match evt.action {
        PrAction::Closed => {
            // Delete the allocation; pool reconciler returns the member.
            let name = allocation_name(&evt.repository.full_name, evt.number);
            // `Api<EphemeralAllocation>` binds via the ONE substrate
            // primitive `HandlerState::allocation_api` — pre-lift this
            // was a hand-authored `Api::namespaced(state.kube.clone(),
            // &state.config.namespace)` chain, one of TWO workspace-
            // wide restatements past the ★★ PRIME-DIRECTIVE ≥ 2
            // duplication threshold (peer at the `PrAction::{Opened,
            // Reopened, Synchronize}` create-branch slot below). Post-
            // lift the two consumers share ONE substrate owner.
            let api = state.allocation_api();
            match api.delete(&name, &DeleteParams::default()).await {
                Ok(_) => {
                    info!(
                        namespace = %state.config.namespace,
                        allocation = %name,
                        "closed PR → deleted Allocation"
                    );
                    (StatusCode::OK, "allocation deleted").into_response()
                }
                // 404 detection rides the substrate primitive
                // `tatara_process::kube_error::is_not_found` — pre-lift
                // this was a hand-authored `Err(kube::Error::Api(e)) if
                // e.code == 404` match-arm guard, one of FIVE workspace-
                // wide restatements past the ★★ PRIME-DIRECTIVE ≥ 2
                // duplication threshold (the OTHER four sites all key
                // off 409, routed through the peer `is_conflict`).
                Err(ref e) if tatara_process::kube_error::is_not_found(e) => {
                    (StatusCode::OK, "allocation already gone").into_response()
                }
                Err(e) => {
                    warn!(error = %e, "delete failed");
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("delete: {e}")).into_response()
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
            // `Api<EphemeralAllocation>` binds via the ONE substrate
            // primitive `HandlerState::allocation_api` — pre-lift this
            // was a hand-authored `Api::namespaced(state.kube.clone(),
            // &state.config.namespace)` chain, peer to the
            // `PrAction::Closed` delete-branch slot already routed
            // through the primitive above. Post-lift both consumers
            // share ONE substrate owner.
            let api = state.allocation_api();
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
                // 409 detection rides the substrate primitive
                // `tatara_process::kube_error::is_conflict` — pre-lift
                // this was a hand-authored `Err(kube::Error::Api(e)) if
                // e.code == 409` match-arm guard, sibling to the 404
                // arm above (both routed through the same substrate
                // module's paired predicates).
                Err(ref e) if tatara_process::kube_error::is_conflict(e) => {
                    // Already exists — refresh via PATCH (synchronize event).
                    (StatusCode::OK, "allocation already exists (synchronize)").into_response()
                }
                Err(e) => {
                    warn!(error = %e, "create allocation failed");
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("create: {e}")).into_response()
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

    // ─── HandlerState api-primitive substrate pins ─────────────────
    //
    // The two-slot `(state.kube.clone(), &state.config.namespace)`
    // incantation was hand-authored at TWO sites in
    // `handle_pr_event` before `HandlerState::allocation_api` closed
    // it. These pins bind the primitive at fail-before-pass-after
    // granularity so a regression that drifts the configured
    // namespace, the resource kind, or the reused client-slot
    // surfaces here rather than as silent operator-facing drift at
    // every downstream webhook path.
    //
    // `Client::try_from(Config::new(url))` needs a live tokio
    // reactor (`tower::buffer::Buffer::new` spawns a background task
    // on construction), so every pin runs under `#[tokio::test]`.
    #[cfg(test)]
    mod api_primitive_pins {
        use super::*;
        use kube::Config;

        fn state_with_namespace(namespace: &str) -> HandlerState {
            let url = "http://localhost:9999".parse().expect("valid probe url");
            let client = Client::try_from(Config::new(url)).expect("build kube client");
            let config = WatcherConfig {
                listen: "0.0.0.0:8080".into(),
                secret: "test-secret".into(),
                namespace: namespace.into(),
                pin_pool: None,
                include_drafts: false,
                allow_repos: Vec::new(),
            };
            HandlerState {
                config: Arc::new(config),
                kube: client,
            }
        }

        #[tokio::test]
        async fn allocation_api_binds_configured_namespace_into_resource_url() {
            // The webhook handler reads the target namespace from
            // `state.config.namespace` (operator-configured via
            // `TATARA_WATCHER_NAMESPACE`) and expects
            // `state.allocation_api()` to bind that namespace onto
            // the returned Api's REST path — the primitive routes
            // the configured slot through to the `Api::namespaced`
            // dispatcher's `ns` argument unchanged.
            let state = state_with_namespace("watcher-test-ns");
            let api = state.allocation_api();
            let url = api.resource_url();
            assert!(
                url.contains("/namespaces/watcher-test-ns/"),
                "allocation_api resource url must carry the configured namespace verbatim; got {url}"
            );
        }

        #[tokio::test]
        async fn allocation_api_binds_the_ephemeral_allocation_kind() {
            // The typed `Api<EphemeralAllocation>` return pins the
            // resource kind at rustc time; this pin adds the runtime
            // witness — the emitted REST path targets the
            // `tatara.pleme.io/v1alpha1/ephemeralallocations`
            // collection matching the `#[kube(group =
            // "tatara.pleme.io", version = "v1alpha1", plural =
            // "ephemeralallocations")]` attribute on
            // `AllocationSpec`.
            let state = state_with_namespace("default");
            let api = state.allocation_api();
            let url = api.resource_url();
            assert!(
                url.starts_with("/apis/tatara.pleme.io/v1alpha1/"),
                "Api resource url must be scoped to the tatara.pleme.io/v1alpha1 group; got {url}"
            );
            assert!(
                url.ends_with("/ephemeralallocations"),
                "Api resource url must terminate at the `ephemeralallocations` collection; got {url}"
            );
        }

        #[tokio::test]
        async fn allocation_api_matches_hand_authored_pre_lift_bytewise() {
            // Bytewise equivalence with the pre-lift `Api::namespaced
            // (state.kube.clone(), &state.config.namespace)` chain —
            // the primitive changes the authoring surface, not the
            // observable REST path, so a regression that drifts the
            // routing on this primitive surfaces here rather than as
            // silent operator-facing drift at every handler branch.
            for ns in ["default", "ephemeral-pools", "watcher-alt"] {
                let state = state_with_namespace(ns);
                let via_primitive = state.allocation_api();
                let via_pre_lift: Api<EphemeralAllocation> =
                    Api::namespaced(state.kube.clone(), &state.config.namespace);
                assert_eq!(
                    via_primitive.resource_url(),
                    via_pre_lift.resource_url(),
                    "allocation_api must be byte-identical to the pre-lift chain for ns={ns:?}"
                );
            }
        }
    }

    #[test]
    fn repo_matches_exact() {
        assert!(repo_matches("pleme-io/demo-app", "pleme-io/demo-app"));
        assert!(!repo_matches("pleme-io/demo-app", "pleme-io/other"));
    }

    #[test]
    fn repo_matches_org_wildcard() {
        assert!(repo_matches("pleme-io/*", "pleme-io/demo-app"));
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
        assert!(repo_allowed("pleme-io/demo-app", &allow));
        assert!(!repo_allowed("drzln/dotfiles", &allow));
    }
}
