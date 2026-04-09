use anyhow::Result;

use tatara_core::config::ServerConfig;

pub async fn run(config_path: Option<&str>) -> Result<()> {
    let config = ServerConfig::load(config_path)?;
    crate::server::run(config).await
}
