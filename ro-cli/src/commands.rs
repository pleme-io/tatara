use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::api::{BuildRequest, RoClient};
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
        }
    }
}
