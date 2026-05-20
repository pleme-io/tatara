//! tatara-github-watcher binary.

use std::sync::Arc;

use anyhow::Result;
use axum::{routing::post, Router};
use clap::Parser;
use kube::Client;
use tracing::info;
use tracing_subscriber::EnvFilter;

use tatara_github_watcher::config::WatcherConfig;
use tatara_github_watcher::handler::{webhook, HandlerState};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .json()
        .init();

    let config = WatcherConfig::parse();
    let kube = Client::try_default().await?;

    info!(
        listen = %config.listen,
        namespace = %config.namespace,
        pin_pool = ?config.pin_pool,
        allow_repos = ?config.allow_repos,
        "tatara-github-watcher starting"
    );

    let state = HandlerState {
        config: Arc::new(config.clone()),
        kube,
    };
    let app = Router::new()
        .route("/webhook", post(webhook))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
