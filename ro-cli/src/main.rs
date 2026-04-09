mod api;
mod commands;
mod config;
mod output;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use commands::Cli;
use config::RoConfig;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")))
        .init();

    let cli = Cli::parse();
    let config = RoConfig::load()?;
    let client = api::RoClient::new(&config)?;

    cli.run(&client).await
}
