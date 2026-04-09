use serde::{Deserialize, Serialize};
use tracing::info;

use super::types::NodeRoles;

/// Role configuration loaded from config or pinned explicitly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    #[serde(default = "default_true")]
    pub voter: bool,
    #[serde(default = "default_true")]
    pub worker: bool,
    #[serde(default = "default_priority")]
    pub voter_priority: u32,
}

impl Default for RoleConfig {
    fn default() -> Self {
        Self {
            voter: true,
            worker: true,
            voter_priority: 10,
        }
    }
}

impl From<RoleConfig> for NodeRoles {
    fn from(cfg: RoleConfig) -> Self {
        NodeRoles {
            voter: cfg.voter,
            worker: cfg.worker,
            voter_priority: cfg.voter_priority,
        }
    }
}

/// Determines effective roles for this node.
///
/// If roles are explicitly pinned in config, use those.
/// Otherwise, default to both voter + worker (every node can do everything).
pub fn resolve_roles(config: &RoleConfig) -> NodeRoles {
    let roles = NodeRoles::from(config.clone());

    info!(
        voter = roles.voter,
        worker = roles.worker,
        priority = roles.voter_priority,
        "Node roles resolved"
    );

    roles
}

fn default_true() -> bool {
    true
}

fn default_priority() -> u32 {
    10
}
