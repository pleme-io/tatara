use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tatara_kube::{cluster::ClusterManager, config::KubeConfig, KubeReconciler};
use tracing::info;

#[derive(Parser)]
#[command(name = "tatara-kube", about = "Nix-native Kubernetes reconciler")]
struct Cli {
    /// Config file path
    #[arg(long, short)]
    config: Option<PathBuf>,

    /// Flake reference (overrides config file)
    #[arg(long)]
    flake_ref: Option<String>,

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the reconciler loop
    Reconcile {
        /// Run once and exit
        #[arg(long)]
        once: bool,

        /// Dry run — show what would be applied without applying
        #[arg(long)]
        dry_run: bool,

        /// Target cluster name
        #[arg(long)]
        cluster: Option<String>,
    },

    /// Show the reconcile plan without applying
    Plan {
        /// Target cluster name
        #[arg(long)]
        cluster: Option<String>,
    },

    /// Show diff between desired state (Nix) and actual state (cluster)
    Diff {
        /// Target cluster name
        #[arg(long)]
        cluster: Option<String>,
    },

    /// List managed resources for a cluster
    Status {
        /// Target cluster name
        #[arg(long)]
        cluster: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cli.log_level)),
        )
        .init();

    let mut config = if let Some(path) = &cli.config {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content)?
    } else {
        KubeConfig::default()
    };

    if let Some(flake_ref) = &cli.flake_ref {
        config.flake_ref.clone_from(flake_ref);
    }

    match cli.command {
        Commands::Reconcile {
            once,
            dry_run: _,
            cluster,
        } => {
            let cluster_mgr = ClusterManager::default_client().await?;
            let mut reconciler = KubeReconciler::new(config.clone());
            let cluster_name = cluster.as_deref().unwrap_or("default");

            let client = cluster_mgr
                .get(cluster_name)
                .or_else(|| cluster_mgr.get("default"))
                .ok_or_else(|| anyhow::anyhow!("no client for cluster '{}'", cluster_name))?;

            let nix_attr = config
                .clusters
                .get(cluster_name)
                .map(|c| c.nix_attr.as_str())
                .unwrap_or(cluster_name);

            if once {
                let stats = reconciler
                    .reconcile_cluster(client, cluster_name, nix_attr)
                    .await?;
                info!(
                    "reconcile complete: applied={} pruned={} errors={} duration={}ms",
                    stats.applied, stats.pruned, stats.errors, stats.duration_ms
                );
            } else {
                loop {
                    match reconciler
                        .reconcile_cluster(client, cluster_name, nix_attr)
                        .await
                    {
                        Ok(stats) => {
                            info!(
                                "tick: applied={} pruned={} errors={} duration={}ms",
                                stats.applied, stats.pruned, stats.errors, stats.duration_ms
                            );
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "reconciliation failed");
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(
                        config.reconcile_interval_secs,
                    ))
                    .await;
                }
            }
        }
        Commands::Plan { cluster } => {
            let cluster_name = cluster.as_deref().unwrap_or("default");
            let nix_attr = config
                .clusters
                .get(cluster_name)
                .map(|c| c.nix_attr.as_str())
                .unwrap_or(cluster_name);

            info!("evaluating nix for cluster '{}'", cluster_name);
            let resources = tatara_kube::nix_eval::eval_cluster_resources(
                &config.flake_ref,
                &config.system,
                nix_attr,
                config.nix_eval_timeout_secs,
            )
            .await?;
            println!("{} resources would be applied", resources.len());
            for r in &resources {
                let kind = r.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                let name = r
                    .pointer("/metadata/name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let ns = r
                    .pointer("/metadata/namespace")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-");
                println!("  {}/{} ({})", ns, name, kind);
            }
        }
        Commands::Diff { cluster: _ } => {
            println!("diff command not yet implemented");
        }
        Commands::Status { cluster: _ } => {
            println!("status command not yet implemented");
        }
    }

    Ok(())
}
