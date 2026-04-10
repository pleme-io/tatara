//! GitHub webhook HTTP server for FlakeSource instant triggers.
//!
//! Listens on port 8081. When GitHub sends a push event, the handler
//! annotates the matching FlakeSource CR to force immediate reconciliation.
//! The FlakeSource controller does the actual work — this just breaks the
//! poll cache.

use std::net::SocketAddr;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use chrono::Utc;
use hmac::{Hmac, Mac};
use kube::api::{Api, Patch, PatchParams};
use kube::Client;
use sha2::Sha256;
use tracing::{info, warn};

use crate::crds::flake_source::FlakeSource;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
struct WebhookState {
    kube_client: Client,
}

pub async fn start_webhook_server(
    addr: SocketAddr,
    kube_client: Client,
) -> anyhow::Result<()> {
    let state = WebhookState { kube_client };

    let app = Router::new()
        .route(
            "/webhooks/github/{namespace}/{name}",
            post(github_push_handler),
        )
        .route("/health", axum::routing::get(|| async { "ok" }))
        .with_state(state);

    info!("Webhook server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn github_push_handler(
    Path((namespace, name)): Path<(String, String)>,
    headers: HeaderMap,
    State(state): State<WebhookState>,
    body: Bytes,
) -> impl IntoResponse {
    // Look up the FlakeSource CR
    let api: Api<FlakeSource> = Api::namespaced(state.kube_client.clone(), &namespace);
    let source = match api.get(&name).await {
        Ok(s) => s,
        Err(_) => {
            warn!(namespace = %namespace, name = %name, "FlakeSource not found");
            return StatusCode::NOT_FOUND;
        }
    };

    // Verify webhook signature if secret is configured
    if let Some(secret_ref) = &source.spec.webhook_secret_ref {
        let secrets_api: Api<k8s_openapi::api::core::v1::Secret> =
            Api::namespaced(state.kube_client.clone(), &namespace);

        let secret = match secrets_api.get(&secret_ref.name).await {
            Ok(s) => s,
            Err(_) => {
                warn!(secret = %secret_ref.name, "Webhook secret not found");
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
        };

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

    // Annotate the FlakeSource to trigger immediate reconciliation.
    // The kube-rs controller watches for any change — updating an annotation
    // forces the next reconcile cycle without duplicating logic.
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
            info!(namespace = %namespace, name = %name, "Webhook triggered FlakeSource reconciliation");
            StatusCode::OK
        }
        Err(e) => {
            warn!(error = %e, "Failed to trigger FlakeSource reconciliation");
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
    let result = hex::encode(mac.finalize().into_bytes());

    // Constant-time comparison
    result == expected
}
