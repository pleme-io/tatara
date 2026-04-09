use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::api::{BuildRequest, RoClient};
use crate::nix_config;
use crate::output;

#[derive(Parser)]
#[command(
    name = "ro",
    about = "ro (炉) — Nix build platform CLI",
    version,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Submit a Nix build request
    Build {
        /// Flake reference (e.g., "github:pleme-io/repo#package" or ".#package")
        flake_ref: String,

        /// Target system
        #[arg(short, long, default_value = "x86_64-linux")]
        system: String,

        /// Attic cache to push results to
        #[arg(short, long, default_value = "main")]
        cache: String,

        /// Extra nix build arguments
        #[arg(long)]
        extra_args: Vec<String>,

        /// Build priority (higher = sooner)
        #[arg(short, long, default_value_t = 0)]
        priority: i32,

        /// Wait for build to complete and stream logs
        #[arg(short, long)]
        wait: bool,
    },

    /// Check build status
    Status {
        /// Build ID
        build_id: String,
    },

    /// Stream build logs
    Logs {
        /// Build ID
        build_id: String,

        /// Follow log output
        #[arg(short, long)]
        follow: bool,
    },

    /// List watched flake sources and their cache status
    Sources,

    /// Show cache statistics
    Cache,

    /// Show platform configuration (substituters, public keys)
    Config,

    /// Check platform health
    Health,

    /// First-time setup — fetch config and apply nix configuration
    Init,

    /// Apply/update local nix configuration from the platform
    Configure {
        /// Show what would change without writing files
        #[arg(long)]
        dry_run: bool,
    },

    /// Refresh cached config from the platform API
    Refresh,
}

impl Cli {
    pub async fn run(&self, client: &RoClient) -> Result<()> {
        match &self.command {
            Command::Build {
                flake_ref,
                system,
                cache,
                extra_args,
                priority,
                wait,
            } => {
                let request = BuildRequest {
                    flake_ref: flake_ref.clone(),
                    system: system.clone(),
                    attic_cache: Some(cache.clone()),
                    extra_args: extra_args.clone(),
                    priority: *priority,
                };

                let response = client.submit_build(&request).await?;
                output::print_build_submitted(&response);

                if *wait {
                    output::wait_for_build(client, &response.build_id).await?;
                }

                Ok(())
            }

            Command::Status { build_id } => {
                let status = client.get_build(build_id).await?;
                output::print_build_status(&status);
                Ok(())
            }

            Command::Logs { build_id, follow } => {
                let resp = client.stream_logs(build_id).await?;
                output::stream_sse_logs(resp, *follow).await
            }

            Command::Sources => {
                let sources = client.list_sources().await?;
                output::print_sources(&sources);
                Ok(())
            }

            Command::Cache => {
                let info = client.cache_info().await?;
                output::print_cache_info(&info);
                Ok(())
            }

            Command::Config => {
                let config = client.get_config().await?;
                output::print_platform_config(&config);
                Ok(())
            }

            Command::Health => {
                let healthy = client.health().await?;
                output::print_health(healthy);
                Ok(())
            }

            Command::Init => {
                println!("Connecting to ro platform...");
                let config = client.get_config().await?;
                nix_config::save_cached(&config, &client.base_url())?;
                let result = nix_config::apply_nix_config(&config)?;
                output::print_init_result(&result);
                Ok(())
            }

            Command::Configure { dry_run } => {
                // Use cached config if available, otherwise fetch
                let config = if let Some(cached) = nix_config::load_cached()? {
                    println!("Using cached config (fetched {})", cached.fetched_at);
                    cached.config
                } else {
                    println!("No cached config, fetching from API...");
                    let config = client.get_config().await?;
                    nix_config::save_cached(&config, &client.base_url())?;
                    config
                };

                if *dry_run {
                    output::print_platform_config(&config);
                    println!("\n(dry run — no files written)");
                } else {
                    let result = nix_config::apply_nix_config(&config)?;
                    output::print_configure_result(&result);
                }
                Ok(())
            }

            Command::Refresh => {
                let old = nix_config::load_cached()?;
                println!("Fetching latest platform config...");
                let config = client.get_config().await?;
                nix_config::save_cached(&config, &client.base_url())?;
                let result = nix_config::apply_nix_config(&config)?;
                output::print_refresh_result(&result, old.as_ref());
                Ok(())
            }
        }
    }
}
