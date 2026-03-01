use std::time::Duration;
use tracing::debug;

/// Heartbeat stub for Phase 1 (embedded mode — no remote server).
/// Phase 2 will implement gRPC bidirectional heartbeat.
pub struct Heartbeat;

impl Heartbeat {
    pub fn new() -> Self {
        Self
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        debug!("Heartbeat running in embedded mode (no-op)");
        // In embedded mode, server and client share process — no heartbeat needed.
        // Phase 2 will connect to a remote server via gRPC.
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }
}
