//! tatara-reconciler entrypoint — FluxCD-adjacent K8s controller that treats
//! every `Process` CRD as a Unix process in the tatara convergence lattice.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use axum::routing::get;
use axum::Router;
use clap::Parser;
use futures::StreamExt;
use kube::runtime::controller::Controller;
use kube::runtime::watcher;
use kube::{Api, Client};
use tracing::info;
use tracing_subscriber::EnvFilter;

use tatara_process::prelude::*;
use tatara_reconciler::context::{Context, ReconcilerConfig};
use tatara_reconciler::{controller, table_controller};

#[derive(Parser, Debug)]
#[command(name = "tatara-reconciler", about)]
struct Cli {
    /// Namespace to watch (empty = all).
    #[arg(long, env = "TATARA_WATCH_NAMESPACE", default_value = "")]
    watch_namespace: String,
    /// Namespace the controller runs in.
    #[arg(
        long,
        env = "TATARA_CONTROLLER_NAMESPACE",
        default_value = "tatara-system"
    )]
    controller_namespace: String,
    /// Health + metrics bind address.
    #[arg(long, env = "TATARA_HEALTH_ADDR", default_value = "0.0.0.0:8080")]
    health_addr: SocketAddr,
    /// Heartbeat interval in seconds.
    #[arg(long, env = "TATARA_HEARTBEAT_SECONDS", default_value_t = 30u64)]
    heartbeat_seconds: u64,
    /// Container image stamped into every tatara-export-worker Job
    /// the reconciler emits during the `Releasing` phase.
    #[arg(
        long,
        env = "TATARA_EXPORT_WORKER_IMAGE",
        default_value = "ghcr.io/pleme-io/tatara-export-worker:0.2.0"
    )]
    export_worker_image: String,
    /// ServiceAccount the export-worker Jobs run as. Operators
    /// provision it (Role + RoleBinding granting list/get/patch on
    /// ConfigMaps + get on Processes) via the reconciler's Helm
    /// chart.
    #[arg(
        long,
        env = "TATARA_EXPORT_WORKER_SERVICE_ACCOUNT",
        default_value = "tatara-export-worker"
    )]
    export_worker_service_account: String,

    /// **R9 fleet routing** — cluster name stamped into every emitted
    /// FQDN (`${app}.${eph_id}.${cluster}.${location}.${domain}`).
    /// Overrides per-cluster via the reconciler Helm chart.
    #[arg(long, env = "TATARA_FLEET_CLUSTER", default_value = "pleme-dev")]
    fleet_cluster: String,

    #[arg(long, env = "TATARA_FLEET_LOCATION", default_value = "use1")]
    fleet_location: String,

    #[arg(long, env = "TATARA_FLEET_DOMAIN", default_value = "quero.lol")]
    fleet_domain: String,

    /// External-dns CNAME target — when set, the reconciler emits
    /// DNSEndpoint resources for every Process routing entry.
    /// None/empty ⇒ Ingress still emits but DNS skipped.
    #[arg(long, env = "TATARA_DNS_LB_TARGET", default_value = "")]
    dns_lb_target: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    let cli = Cli::parse();
    info!(
        watch_namespace = %cli.watch_namespace,
        controller_namespace = %cli.controller_namespace,
        "starting tatara-reconciler"
    );

    let kube = Client::try_default().await?;
    let config = Arc::new(ReconcilerConfig {
        controller_namespace: cli.controller_namespace,
        default_boundary_timeout_seconds: 900,
        heartbeat_seconds: cli.heartbeat_seconds,
        process_table_name: "proc".into(),
        export_worker_image: cli.export_worker_image,
        export_worker_service_account: cli.export_worker_service_account,
        cluster: cli.fleet_cluster,
        location: cli.fleet_location,
        domain: cli.fleet_domain,
        dns_lb_target: if cli.dns_lb_target.is_empty() {
            None
        } else {
            Some(cli.dns_lb_target)
        },
    });
    let ctx = Arc::new(Context {
        kube: kube.clone(),
        config,
    });

    // ── Process controller ────────────────────────────────────────────
    let processes: Api<Process> = if cli.watch_namespace.is_empty() {
        Api::all(kube.clone())
    } else {
        Api::namespaced(kube.clone(), &cli.watch_namespace)
    };
    let proc_ctl = Controller::new(processes, watcher::Config::default())
        .run(controller::reconcile, controller::error_policy, ctx.clone())
        .for_each(|res| async move {
            match res {
                Ok(o) => info!(resource = ?o, "Process reconciled"),
                Err(e) => tracing::error!(error = %e, "Process reconcile failed"),
            }
        });

    // ── ProcessTable controller ───────────────────────────────────────
    let tables: Api<ProcessTable> = Api::all(kube.clone());
    let table_ctl = Controller::new(tables, watcher::Config::default())
        .run(
            table_controller::reconcile,
            table_controller::error_policy,
            ctx.clone(),
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => info!(resource = ?o, "ProcessTable reconciled"),
                Err(e) => tracing::error!(error = %e, "ProcessTable reconcile failed"),
            }
        });

    // ── Health + metrics endpoint ─────────────────────────────────────
    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind(cli.health_addr).await?;
    let http = axum::serve(listener, app);

    tokio::select! {
        _ = proc_ctl => info!("Process controller exited"),
        _ = table_ctl => info!("ProcessTable controller exited"),
        r = http => {
            if let Err(e) = r {
                tracing::error!(error = %e, "HTTP server error");
            }
        }
    }

    Ok(())
}
