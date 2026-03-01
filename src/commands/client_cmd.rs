use anyhow::Result;
use tracing::info;

use crate::config::ClientConfig;

pub async fn run(server_addr: Option<&str>, config_path: Option<&str>) -> Result<()> {
    let mut config = ClientConfig::load(config_path)?;

    if let Some(addr) = server_addr {
        config.server_addr = addr.to_string();
    }

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    info!(
        server = %config.server_addr,
        "tatara client starting (Phase 2: will connect to remote server)"
    );

    // Phase 2: Connect to server via gRPC, start heartbeat loop, receive allocations.
    // For now, just run as a stub.
    info!("Client mode requires Phase 2 (multi-node). Use 'tatara server' for embedded mode.");

    Ok(())
}
