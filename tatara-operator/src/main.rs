mod controllers;
mod crds;

use std::sync::Arc;

use anyhow::Result;
use async_nats::jetstream;
use futures::StreamExt;
use kube::runtime::controller::Controller;
use kube::runtime::watcher::Config;
use kube::{Api, Client};
use tracing::info;
use tracing_subscriber::EnvFilter;

use controllers::nix_build::{self, NixBuildContext};
use crds::nix_build::NixBuild;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .json()
        .init();

    info!("Starting tatara-operator");

    // K8s client
    let kube_client = Client::try_default().await?;

    // NATS client
    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://nats.nats.svc:4222".to_string());
    let nats_client = async_nats::connect(&nats_url).await?;
    let jetstream = jetstream::new(nats_client.clone());

    // Ensure BUILD stream exists
    jetstream
        .get_or_create_stream(jetstream::stream::Config {
            name: "BUILD".to_string(),
            subjects: vec!["BUILD.>".to_string()],
            retention: jetstream::stream::RetentionPolicy::WorkQueue,
            ..Default::default()
        })
        .await?;

    info!("Connected to NATS at {}", nats_url);

    let ctx = Arc::new(NixBuildContext {
        kube_client: kube_client.clone(),
        nats_client,
        jetstream,
    });

    // Start completion subscriber (background)
    let ctx_sub = ctx.clone();
    tokio::spawn(async move {
        if let Err(e) = nix_build::start_completion_subscriber(ctx_sub).await {
            tracing::error!(error = %e, "Completion subscriber failed");
        }
    });

    // Start NixBuild controller
    let builds: Api<NixBuild> = Api::all(kube_client);
    Controller::new(builds, Config::default())
        .run(nix_build::reconcile, nix_build::error_policy, ctx)
        .for_each(|res| async move {
            match res {
                Ok(o) => info!(resource = ?o, "Reconciled"),
                Err(e) => tracing::error!(error = %e, "Reconcile failed"),
            }
        })
        .await;

    Ok(())
}
