mod cli;
mod commands;
mod server;

use clap::Parser;
use cli::{
    AllocCmd, Cli, Commands, ContextCmd, EventCmd, ForgeCmd, JobCmd, NodeCmd, ReleaseCmd, SourceCmd,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let output = cli.output;
    let endpoint = cli.endpoint.as_deref();

    match cli.command {
        // ── Job subcommands ──
        Commands::Job { command } => match command {
            JobCmd::List => cli::job::list(output, endpoint).await,
            JobCmd::Get { job_id } => cli::job::get(&job_id, output, endpoint).await,
            JobCmd::Run { job_file, eval } => {
                cli::job::run_job(&job_file, eval, endpoint, output).await
            }
            JobCmd::Stop { job_id } => cli::job::stop(&job_id, endpoint, output).await,
            JobCmd::History { job_id } => cli::job::history(&job_id, output, endpoint).await,
            JobCmd::Diff { job_id, v1, v2 } => cli::job::diff(&job_id, v1, v2, endpoint).await,
            JobCmd::Rollback { job_id, version } => {
                cli::job::rollback(&job_id, version, endpoint, output).await
            }
        },

        // ── Node subcommands ──
        Commands::Node { command } => match command {
            NodeCmd::List => cli::node::list(output, endpoint).await,
            NodeCmd::Status { node_id } => {
                cli::node::status(node_id.as_deref(), output, endpoint).await
            }
            NodeCmd::Drain {
                node_id,
                deadline,
            } => cli::node::drain(&node_id, deadline, endpoint, output).await,
            NodeCmd::Eligibility {
                node_id,
                enable,
                disable,
            } => {
                let eligible = if disable { false } else { enable || true };
                cli::node::eligibility(&node_id, eligible, endpoint, output).await
            }
        },

        // ── Allocation subcommands ──
        Commands::Alloc { command } => match command {
            AllocCmd::List => cli::alloc::list(output, endpoint).await,
            AllocCmd::Get { alloc_id } => cli::alloc::get(&alloc_id, output, endpoint).await,
            AllocCmd::Logs {
                alloc_id,
                task,
                follow,
            } => cli::alloc::logs(&alloc_id, task.as_deref(), follow, endpoint).await,
        },

        // ── Eval ──
        Commands::Eval { nix_file } => {
            let path = std::path::PathBuf::from(&nix_file);
            let spec = tatara_engine::nix_eval::evaluator::NixEvaluator::eval_file(&path).await?;
            println!("{}", serde_json::to_string_pretty(&spec)?);
            Ok(())
        }

        // ── Deploy ──
        Commands::Deploy {
            flake_ref,
            set_values,
            name,
            dry_run,
        } => {
            cli::deploy::run(
                &flake_ref,
                &set_values,
                name.as_deref(),
                dry_run,
                endpoint,
                output,
            )
            .await
        }

        // ── Server ──
        Commands::Server { config } => commands::server_cmd::run(config.as_deref()).await,

        // ── Client ──
        Commands::Client { server, config } => {
            commands::client_cmd::run(server.as_deref(), config.as_deref()).await
        }

        // ── Top ──
        Commands::Top { node, refresh } => {
            cli::top::run(node.as_deref(), refresh, endpoint).await
        }

        // ── Context ──
        Commands::Context { command } => match command {
            ContextCmd::List => cli::context::list(output).await,
            ContextCmd::Use { name } => cli::context::use_context(&name).await,
            ContextCmd::Current => cli::context::current().await,
        },

        // ── Event ──
        Commands::Event { command } => match command {
            EventCmd::List { kind, since } => {
                cli::event::list(kind.as_deref(), since.as_deref(), output, endpoint).await
            }
            EventCmd::Stream { kind } => {
                cli::event::stream(kind.as_deref(), endpoint).await
            }
        },

        // ── Release ──
        Commands::Release { command } => match command {
            ReleaseCmd::List => cli::release::list(output, endpoint).await,
            ReleaseCmd::Get { release_id } => {
                cli::release::get(&release_id, output, endpoint).await
            }
            ReleaseCmd::Promote { release_id } => {
                cli::release::promote(&release_id, endpoint, output).await
            }
            ReleaseCmd::Rollback { release_id } => {
                cli::release::rollback(&release_id, endpoint, output).await
            }
        },

        // ── Forge ──
        Commands::Forge { command } => match command {
            ForgeCmd::Init { name } => cli::forge::init(&name).await,
            ForgeCmd::Validate { path } => cli::forge::validate(&path, output).await,
            ForgeCmd::Inspect { flake_ref } => cli::forge::inspect(&flake_ref, output).await,
        },

        // ── Source ──
        Commands::Source { command } => match command {
            SourceCmd::List => cli::source::list(output, endpoint).await,
            SourceCmd::Get { name_or_id } => {
                cli::source::get(&name_or_id, output, endpoint).await
            }
            SourceCmd::Add {
                name,
                flake_ref,
                kind,
            } => cli::source::add(&name, &flake_ref, &kind, endpoint, output).await,
            SourceCmd::Delete { name_or_id } => {
                cli::source::delete(&name_or_id, endpoint, output).await
            }
            SourceCmd::Sync { name_or_id } => {
                cli::source::sync(&name_or_id, endpoint, output).await
            }
            SourceCmd::Suspend { name_or_id } => {
                cli::source::suspend(&name_or_id, endpoint, output).await
            }
            SourceCmd::Resume { name_or_id } => {
                cli::source::resume(&name_or_id, endpoint, output).await
            }
        },

        // ── Backwards-compatible aliases ──
        Commands::Run {
            job_file,
            eval,
            server,
        } => {
            cli::job::run_job(&job_file, eval, Some(&format!("http://{}", server)), output).await
        }
        Commands::Status { job_id, server } => match job_id {
            Some(id) => {
                cli::job::get(&id, output, Some(&format!("http://{}", server))).await
            }
            None => cli::job::list(output, Some(&format!("http://{}", server))).await,
        },
        Commands::Stop { job_id, server } => {
            cli::job::stop(&job_id, Some(&format!("http://{}", server)), output).await
        }
        Commands::Logs {
            alloc_id,
            task,
            follow,
            server,
        } => {
            cli::alloc::logs(
                &alloc_id,
                task.as_deref(),
                follow,
                Some(&format!("http://{}", server)),
            )
            .await
        }
    }
}
