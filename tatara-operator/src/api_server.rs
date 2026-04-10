//! ro platform REST API server.
//!
//! Serves the external API that ro CLI, tend, and other clients use.
//! All mutating endpoints require bearer token auth. Read-only endpoints
//! (health, config) are unauthenticated.
//!
//! Endpoints:
//!   GET  /health              — health check (no auth)
//!   GET  /config              — platform config for clients (no auth)
//!   POST /api/v1/builds       — submit a build (auth required)
//!   GET  /api/v1/builds/{id}  — get build status (auth required)
//!   GET  /api/v1/sources      — list FlakeSource status (auth required)
//!   GET  /api/v1/cache        — cache statistics (auth required)
//!
//! Auth: Bearer token from RO_API_TOKEN env var or K8s Secret.

use std::net::SocketAddr;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use hmac::{Hmac, Mac};
use kube::api::{Api, ListParams, ObjectMeta, Patch, PatchParams, PostParams};
use kube::{Client, ResourceExt};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::{info, warn};
use uuid::Uuid;

use crate::crds::flake_source::FlakeSource;
use crate::crds::nix_build::{NixBuild, NixBuildSpec};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct ApiState {
    pub kube_client: Client,
    pub api_token: Option<String>,
    pub cache_endpoint: String,
    pub platform_version: String,
}

// ── API types (match ro-cli/src/api.rs) ──────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BuildRequest {
    pub flake_ref: String,
    pub system: String,
    pub attic_cache: Option<String>,
    pub extra_args: Vec<String>,
    pub priority: i32,
}

#[derive(Debug, Serialize)]
pub struct BuildResponse {
    pub build_id: String,
    pub status: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct BuildStatus {
    pub build_id: Option<String>,
    pub phase: String,
    pub store_path: Option<String>,
    pub builder_node: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PlatformConfig {
    pub substituters: Vec<String>,
    pub trusted_public_keys: Vec<String>,
    pub cache_endpoint: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct SourceStatus {
    pub name: String,
    pub repo: String,
    pub branch: String,
    pub last_commit: Option<String>,
    pub cached_outputs: u32,
    pub total_outputs: u32,
}

#[derive(Debug, Serialize)]
pub struct CacheInfo {
    pub name: String,
    pub endpoint: String,
    pub total_nars: u64,
    pub total_size_bytes: u64,
}

// ── Server ───────────────────────────────────────────────────────────────

pub async fn start_api_server(
    addr: SocketAddr,
    kube_client: Client,
) -> anyhow::Result<()> {
    let api_token = std::env::var("RO_API_TOKEN").ok();
    let cache_endpoint = std::env::var("RO_CACHE_ENDPOINT")
        .unwrap_or_else(|_| "http://attic.nix-cache.svc:80".to_string());
    let platform_version = env!("CARGO_PKG_VERSION").to_string();

    if api_token.is_none() {
        warn!("RO_API_TOKEN not set — API endpoints are unauthenticated");
    }

    let state = ApiState {
        kube_client,
        api_token,
        cache_endpoint,
        platform_version,
    };

    let app = Router::new()
        // Unauthenticated
        .route("/health", get(health))
        .route("/config", get(get_config))
        // Authenticated
        .route("/api/v1/builds", post(submit_build))
        .route("/api/v1/builds/{id}", get(get_build))
        .route("/api/v1/sources", get(list_sources))
        .route("/api/v1/cache", get(get_cache_info))
        // GitHub webhooks (separate auth via HMAC)
        .route(
            "/webhooks/github/{namespace}/{name}",
            post(github_push_handler),
        )
        .with_state(state);

    info!("API server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ── Auth middleware ──────────────────────────────────────────────────────

fn check_auth(headers: &HeaderMap, expected: &Option<String>) -> Result<(), StatusCode> {
    let Some(expected_token) = expected else {
        return Ok(()); // No token configured = open access
    };

    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match auth {
        Some(token) if token == expected_token => Ok(()),
        Some(_) => Err(StatusCode::UNAUTHORIZED),
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

// ── Handlers ─────────────────────────────────────────────────────────────

async fn health() -> &'static str {
    "ok"
}

async fn get_config(State(state): State<ApiState>) -> Json<PlatformConfig> {
    Json(PlatformConfig {
        substituters: vec![format!("{}/main", state.cache_endpoint)],
        trusted_public_keys: vec![],
        cache_endpoint: state.cache_endpoint,
        version: state.platform_version,
    })
}

async fn submit_build(
    headers: HeaderMap,
    State(state): State<ApiState>,
    Json(req): Json<BuildRequest>,
) -> Result<Json<BuildResponse>, StatusCode> {
    check_auth(&headers, &state.api_token)?;

    let build_id = Uuid::new_v4().to_string();
    let build_name = format!("ro-{}", &build_id[..8]);

    let build = NixBuild::new(
        &build_name,
        NixBuildSpec {
            flake_ref: req.flake_ref.clone(),
            system: req.system,
            attic_cache: req.attic_cache,
            extra_args: req.extra_args,
            priority: req.priority,
        },
    );

    let mut build = build;
    build.metadata = ObjectMeta {
        name: Some(build_name.clone()),
        namespace: Some("tatara-system".to_string()),
        labels: Some(
            [
                ("tatara.pleme.io/source".to_string(), "api".to_string()),
                ("tatara.pleme.io/build-id".to_string(), build_id.clone()),
            ]
            .into(),
        ),
        ..Default::default()
    };

    let builds_api: Api<NixBuild> =
        Api::namespaced(state.kube_client.clone(), "tatara-system");

    builds_api
        .create(&PostParams::default(), &build)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to create NixBuild CR");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!(build_id = %build_id, flake_ref = %req.flake_ref, "Build submitted via API");

    Ok(Json(BuildResponse {
        build_id,
        status: "Pending".to_string(),
        name: build_name,
    }))
}

async fn get_build(
    headers: HeaderMap,
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<BuildStatus>, StatusCode> {
    check_auth(&headers, &state.api_token)?;

    // Search for NixBuild CR by build-id label
    let builds_api: Api<NixBuild> =
        Api::namespaced(state.kube_client.clone(), "tatara-system");

    let lp = ListParams::default().labels(&format!("tatara.pleme.io/build-id={id}"));
    let builds = builds_api.list(&lp).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let build = builds.items.first().ok_or(StatusCode::NOT_FOUND)?;
    let status = build.status.as_ref();

    Ok(Json(BuildStatus {
        build_id: status.and_then(|s| s.build_id.clone()),
        phase: status
            .map(|s| format!("{:?}", s.phase))
            .unwrap_or_else(|| "Pending".to_string()),
        store_path: status.and_then(|s| s.store_path.clone()),
        builder_node: status.and_then(|s| s.builder_node.clone()),
        started_at: status.and_then(|s| s.started_at.map(|t| t.to_rfc3339())),
        completed_at: status.and_then(|s| s.completed_at.map(|t| t.to_rfc3339())),
        error: status.and_then(|s| s.error.clone()),
    }))
}

async fn list_sources(
    headers: HeaderMap,
    State(state): State<ApiState>,
) -> Result<Json<Vec<SourceStatus>>, StatusCode> {
    check_auth(&headers, &state.api_token)?;

    let api: Api<FlakeSource> = Api::all(state.kube_client.clone());
    let sources = api
        .list(&ListParams::default())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result: Vec<SourceStatus> = sources
        .items
        .iter()
        .map(|s| {
            let status = s.status.as_ref();
            SourceStatus {
                name: s.name_any(),
                repo: s.spec.repo.clone(),
                branch: s.spec.branch.clone(),
                last_commit: status.and_then(|st| st.last_commit.clone()),
                cached_outputs: status.map(|st| st.cached_outputs).unwrap_or(0),
                total_outputs: status.map(|st| st.total_outputs).unwrap_or(0),
            }
        })
        .collect();

    Ok(Json(result))
}

async fn get_cache_info(
    headers: HeaderMap,
    State(state): State<ApiState>,
) -> Result<Json<CacheInfo>, StatusCode> {
    check_auth(&headers, &state.api_token)?;

    Ok(Json(CacheInfo {
        name: "main".to_string(),
        endpoint: state.cache_endpoint,
        total_nars: 0,
        total_size_bytes: 0,
    }))
}

// ── GitHub webhook handler (HMAC auth, same as before) ───────────────────

async fn github_push_handler(
    Path((namespace, name)): Path<(String, String)>,
    headers: HeaderMap,
    State(state): State<ApiState>,
    body: Bytes,
) -> impl IntoResponse {
    let api: Api<FlakeSource> = Api::namespaced(state.kube_client.clone(), &namespace);
    let source = match api.get(&name).await {
        Ok(s) => s,
        Err(_) => {
            warn!(namespace = %namespace, name = %name, "FlakeSource not found");
            return StatusCode::NOT_FOUND;
        }
    };

    if let Some(secret_ref) = &source.spec.webhook_secret_ref {
        let secrets_api: Api<k8s_openapi::api::core::v1::Secret> =
            Api::namespaced(state.kube_client.clone(), &namespace);

        if let Ok(secret) = secrets_api.get(&secret_ref.name).await {
            let key = secret
                .data
                .as_ref()
                .and_then(|d| d.get(&secret_ref.key))
                .map(|v| v.0.clone());

            if let Some(key) = key {
                let sig_header = headers
                    .get("x-hub-signature-256")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");

                if !verify_signature(&key, &body, sig_header) {
                    warn!(namespace = %namespace, name = %name, "Invalid webhook signature");
                    return StatusCode::UNAUTHORIZED;
                }
            }
        }
    }

    let patch = serde_json::json!({
        "metadata": {
            "annotations": {
                "tatara.pleme.io/webhook-trigger": Utc::now().to_rfc3339(),
            }
        }
    });

    match api
        .patch(&name, &PatchParams::default(), &Patch::Merge(&patch))
        .await
    {
        Ok(_) => {
            info!(namespace = %namespace, name = %name, "Webhook triggered reconciliation");
            StatusCode::OK
        }
        Err(e) => {
            warn!(error = %e, "Failed to trigger reconciliation");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

fn verify_signature(key: &[u8], body: &[u8], signature_header: &str) -> bool {
    let expected = match signature_header.strip_prefix("sha256=") {
        Some(hex_sig) => hex_sig,
        None => return false,
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(key) else {
        return false;
    };
    mac.update(body);
    hex::encode(mac.finalize().into_bytes()) == expected
}
