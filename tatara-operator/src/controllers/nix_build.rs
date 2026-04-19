use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_nats::jetstream;
use chrono::Utc;
use futures::StreamExt;
use kube::api::{Api, Patch, PatchParams};
use kube::runtime::controller::Action;
use kube::{Client, ResourceExt};
use serde_json::json;
use tracing::{info, warn};
use uuid::Uuid;

use crate::crds::nix_build::{NixBuild, NixBuildPhase, NixBuildStatus};

/// Shared state for the NixBuild controller.
pub struct NixBuildContext {
    pub kube_client: Client,
    pub nats_client: async_nats::Client,
    pub jetstream: jetstream::Context,
}

/// Error type for the reconciler.
#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[error("Kube error: {0}")]
    Kube(#[from] kube::Error),

    #[error("NATS error: {0}")]
    Nats(String),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Reconcile a NixBuild CR.
///
/// State machine:
///   Pending → Queued (publish to NATS) → Building → Pushing → Complete
///   Any → Failed (on error)
pub async fn reconcile(
    build: Arc<NixBuild>,
    ctx: Arc<NixBuildContext>,
) -> Result<Action, ReconcileError> {
    let name = build.name_any();
    let namespace = build.namespace().unwrap_or_default();
    let api: Api<NixBuild> = Api::namespaced(ctx.kube_client.clone(), &namespace);

    let phase = build
        .status
        .as_ref()
        .map(|s| s.phase.clone())
        .unwrap_or(NixBuildPhase::Pending);

    match phase {
        NixBuildPhase::Pending => {
            let build_id = Uuid::new_v4().to_string();

            let request = json!({
                "build_id": build_id,
                "flake_ref": build.spec.flake_ref,
                "system": build.spec.system,
                "attic_cache": build.spec.attic_cache,
                "extra_args": build.spec.extra_args,
                "priority": build.spec.priority,
                "name": name,
                "namespace": namespace,
            });

            let payload = serde_json::to_vec(&request)?;

            ctx.jetstream
                .publish("BUILD.request", payload.into())
                .await
                .map_err(|e| ReconcileError::Nats(e.to_string()))?
                .await
                .map_err(|e| ReconcileError::Nats(e.to_string()))?;

            info!(name = %name, build_id = %build_id, "Published build request to NATS");

            let status = NixBuildStatus {
                phase: NixBuildPhase::Queued,
                build_id: Some(build_id),
                ..Default::default()
            };

            let patch = json!({ "status": status });
            api.patch_status(
                &name,
                &PatchParams::apply("tatara-operator"),
                &Patch::Merge(&patch),
            )
            .await?;

            Ok(Action::requeue(Duration::from_secs(10)))
        }

        NixBuildPhase::Queued | NixBuildPhase::Building | NixBuildPhase::Pushing => {
            Ok(Action::requeue(Duration::from_secs(15)))
        }

        NixBuildPhase::Complete | NixBuildPhase::Failed => Ok(Action::await_change()),
    }
}

/// Error handler for failed reconciliations.
pub fn error_policy(
    build: Arc<NixBuild>,
    err: &ReconcileError,
    _ctx: Arc<NixBuildContext>,
) -> Action {
    let name = build.name_any();
    warn!(name = %name, error = %err, "Reconciliation failed, retrying");
    Action::requeue(Duration::from_secs(30))
}

/// Start the NATS completion subscriber — listens for BUILD.complete.* and updates CRs.
pub async fn start_completion_subscriber(ctx: Arc<NixBuildContext>) -> Result<()> {
    let stream = ctx
        .jetstream
        .get_or_create_stream(jetstream::stream::Config {
            name: "BUILD".to_string(),
            subjects: vec!["BUILD.>".to_string()],
            ..Default::default()
        })
        .await?;

    let consumer = stream
        .create_consumer(jetstream::consumer::pull::Config {
            durable_name: Some("tatara-operator-completions".to_string()),
            filter_subject: "BUILD.complete.*".to_string(),
            ..Default::default()
        })
        .await?;

    let mut messages = consumer.messages().await?;

    while let Some(Ok(msg)) = messages.next().await {
        if let Ok(completion) = serde_json::from_slice::<serde_json::Value>(&msg.payload) {
            let build_id = completion["build_id"].as_str().unwrap_or("");
            let name = completion["name"].as_str().unwrap_or("");
            let namespace = completion["namespace"].as_str().unwrap_or("default");
            let store_path = completion["store_path"].as_str();
            let error_msg = completion["error"].as_str();

            let api: Api<NixBuild> = Api::namespaced(ctx.kube_client.clone(), namespace);

            let status = if let Some(err) = error_msg {
                NixBuildStatus {
                    phase: NixBuildPhase::Failed,
                    build_id: Some(build_id.to_string()),
                    error: Some(err.to_string()),
                    completed_at: Some(Utc::now()),
                    ..Default::default()
                }
            } else {
                NixBuildStatus {
                    phase: NixBuildPhase::Complete,
                    build_id: Some(build_id.to_string()),
                    store_path: store_path.map(String::from),
                    completed_at: Some(Utc::now()),
                    ..Default::default()
                }
            };

            let patch = json!({ "status": status });
            if let Err(e) = api
                .patch_status(
                    name,
                    &PatchParams::apply("tatara-operator"),
                    &Patch::Merge(&patch),
                )
                .await
            {
                warn!(name = %name, error = %e, "Failed to update NixBuild status");
            } else {
                info!(name = %name, build_id = %build_id, phase = ?status.phase, "Updated NixBuild status");
            }
        }

        let _ = msg.ack().await;
    }

    Ok(())
}
