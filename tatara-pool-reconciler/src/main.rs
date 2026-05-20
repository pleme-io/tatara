//! tatara-pool-reconciler — binary entry point.
//!
//! Drives two kube-rs Controllers in parallel:
//! - one over `EphemeralPool`
//! - one over `EphemeralAllocation`
//!
//! Both share a `PoolContext` (kube Client + typed config).

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use futures::StreamExt;
use kube::api::Api;
use kube::runtime::controller::Controller;
use kube::Client;
use tracing::info;
use tracing_subscriber::EnvFilter;

use tatara_pool_reconciler::context::{PoolContext, PoolReconcilerConfig};
use tatara_pool_reconciler::{controller_allocation, controller_pool};

use tatara_process::allocation::EphemeralAllocation;
use tatara_process::pool::EphemeralPool;

#[derive(Parser, Debug)]
#[command(name = "tatara-pool-reconciler", version, about = "EphemeralPool + EphemeralAllocation controller")]
struct Args {
    #[arg(long, env = "TATARA_POOL_NAMESPACE", default_value = "tatara-pool-system")]
    controller_namespace: String,
    #[arg(long, env = "TATARA_POOL_HEARTBEAT_SECONDS", default_value_t = 30)]
    heartbeat_seconds: u64,
    #[arg(long, env = "TATARA_POOL_SPAWN_TIMEOUT", default_value = "10m")]
    spawn_timeout: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .json()
        .init();

    let args = Args::parse();
    info!(
        namespace = %args.controller_namespace,
        heartbeat = args.heartbeat_seconds,
        "tatara-pool-reconciler starting"
    );

    let client = Client::try_default().await?;
    let config = Arc::new(PoolReconcilerConfig {
        controller_namespace: args.controller_namespace,
        heartbeat_seconds: args.heartbeat_seconds,
        spawn_timeout: args.spawn_timeout,
        field_manager: "tatara-pool-reconciler".into(),
    });
    let ctx = Arc::new(PoolContext {
        kube: client.clone(),
        config,
    });

    let pool_api: Api<EphemeralPool> = Api::all(client.clone());
    let alloc_api: Api<EphemeralAllocation> = Api::all(client.clone());

    let pool_ctx = ctx.clone();
    let pool_controller = Controller::new(pool_api, Default::default())
        .run(
            controller_pool::reconcile,
            controller_pool::error_policy,
            pool_ctx,
        )
        .for_each(|res| async move {
            if let Err(e) = res {
                tracing::warn!(error = ?e, "pool controller error");
            }
        });

    let alloc_ctx = ctx.clone();
    let alloc_controller = Controller::new(alloc_api, Default::default())
        .run(
            controller_allocation::reconcile,
            controller_allocation::error_policy,
            alloc_ctx,
        )
        .for_each(|res| async move {
            if let Err(e) = res {
                tracing::warn!(error = ?e, "allocation controller error");
            }
        });

    tokio::join!(pool_controller, alloc_controller);
    Ok(())
}
