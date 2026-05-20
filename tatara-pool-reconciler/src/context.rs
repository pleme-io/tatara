//! Shared controller context — kube client + typed config.

use std::sync::Arc;

use kube::Client;

#[derive(Clone)]
pub struct PoolContext {
    pub kube: Client,
    pub config: Arc<PoolReconcilerConfig>,
}

#[derive(Clone, Debug)]
pub struct PoolReconcilerConfig {
    /// Namespace the controller pod itself runs in (for leader-election
    /// + own-Lease housekeeping). Default: `tatara-pool-system`.
    pub controller_namespace: String,
    /// Default requeue interval when nothing changes. Default: 30s.
    pub heartbeat_seconds: u64,
    /// How long to wait for a spawning member to reach Attested before
    /// marking it Failed. humantime. Default: `"10m"`.
    pub spawn_timeout: String,
    /// Field manager used for server-side applies.
    pub field_manager: String,
}

impl Default for PoolReconcilerConfig {
    fn default() -> Self {
        Self {
            controller_namespace: "tatara-pool-system".into(),
            heartbeat_seconds: 30,
            spawn_timeout: "10m".into(),
            field_manager: "tatara-pool-reconciler".into(),
        }
    }
}
