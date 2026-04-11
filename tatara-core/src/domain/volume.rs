//! Persistent volume types for tatara workloads.

use serde::{Deserialize, Serialize};

/// Type of volume backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VolumeType {
    /// Locally-managed directory on the node.
    Local,
    /// Bind-mount to an existing host path.
    HostPath,
    /// NFS mount.
    Nfs,
}

/// Volume source configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VolumeSource {
    Local {
        #[serde(default)]
        size_mb: Option<u64>,
    },
    HostPath {
        path: String,
    },
    Nfs {
        server: String,
        path: String,
    },
}

/// A volume declaration in a task group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSpec {
    pub name: String,
    pub source: VolumeSource,
    #[serde(default)]
    pub read_only: bool,
}

/// A volume mount in a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeClaim {
    pub volume_name: String,
    pub mount_path: String,
    #[serde(default)]
    pub read_only: bool,
}
