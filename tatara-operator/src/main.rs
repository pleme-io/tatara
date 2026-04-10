use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use async_nats::jetstream;
use futures::StreamExt;
use kube::runtime::controller::Controller;
use kube::runtime::watcher::Config;
use kube::{Api, Client};
use tracing::info;
use tracing_subscriber::EnvFilter;

use tatara_operator::api_server;
use tatara_operator::controllers::flake_source::{self, FlakeSourceContext};
use tatara_operator::controllers::nix_build::{self, NixBuildContext};
use tatara_operator::crds::flake_source::FlakeSource;
use tatara_operator::crds::nix_build::NixBuild;

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

    // ── NixBuild controller ──────────────────────────────────────────────

    let nix_ctx = Arc::new(NixBuildContext {
        kube_client: kube_client.clone(),
        nats_client,
        jetstream,
    });

    // NATS completion subscriber (background)
    let ctx_sub = nix_ctx.clone();
    tokio::spawn(async move {
        if let Err(e) = nix_build::start_completion_subscriber(ctx_sub).await {
            tracing::error!(error = %e, "Completion subscriber failed");
        }
    });

    let builds: Api<NixBuild> = Api::all(kube_client.clone());
    let nix_build_controller = Controller::new(builds, Config::default())
        .run(nix_build::reconcile, nix_build::error_policy, nix_ctx)
        .for_each(|res| async move {
            match res {
                Ok(o) => info!(resource = ?o, "NixBuild reconciled"),
                Err(e) => tracing::error!(error = %e, "NixBuild reconcile failed"),
            }
        });

    // ── FlakeSource controller ───────────────────────────────────────────

    let flake_ctx = Arc::new(FlakeSourceContext {
        kube_client: kube_client.clone(),
        http_client: reqwest::Client::new(),
        github_token: std::env::var("GITHUB_TOKEN").ok(),
    });

    let sources: Api<FlakeSource> = Api::all(kube_client.clone());
    let flake_source_controller = Controller::new(sources, Config::default())
        .run(
            flake_source::reconcile,
            flake_source::error_policy,
            flake_ctx,
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => info!(resource = ?o, "FlakeSource reconciled"),
                Err(e) => tracing::error!(error = %e, "FlakeSource reconcile failed"),
            }
        });

    // ── API server (build submission + webhooks + config) ──────────────

    let api_addr: SocketAddr = "0.0.0.0:8081".parse().unwrap();
    let api_kube = kube_client;
    tokio::spawn(async move {
        if let Err(e) = api_server::start_api_server(api_addr, api_kube).await {
            tracing::error!(error = %e, "API server failed");
        }
    });

    // ── Run both controllers concurrently ────────────────────────────────

    info!("Controllers started");
    tokio::select! {
        () = nix_build_controller => {},
        () = flake_source_controller => {},
    }

    Ok(())
}
