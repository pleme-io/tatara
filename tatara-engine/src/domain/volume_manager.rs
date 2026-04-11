//! Volume lifecycle management — create, attach, detach, delete.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tatara_core::domain::volume::{VolumeClaim, VolumeSource, VolumeSpec};
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

/// Handle to an active volume.
#[derive(Debug, Clone)]
pub struct VolumeHandle {
    pub name: String,
    pub path: PathBuf,
    pub read_only: bool,
}

/// Manages volume lifecycle for a node.
pub struct VolumeManager {
    /// Base directory for local volumes.
    volume_dir: PathBuf,
    /// Active volumes: volume_name -> VolumeHandle
    active: RwLock<HashMap<String, VolumeHandle>>,
}

impl VolumeManager {
    pub fn new(volume_dir: PathBuf) -> Self {
        Self {
            volume_dir,
            active: RwLock::new(HashMap::new()),
        }
    }

    /// Create a volume from its spec. Returns a handle for mounting.
    pub async fn create(&self, spec: &VolumeSpec) -> Result<VolumeHandle> {
        let path = match &spec.source {
            VolumeSource::Local { size_mb: _ } => {
                let vol_path = self.volume_dir.join(&spec.name);
                tokio::fs::create_dir_all(&vol_path)
                    .await
                    .with_context(|| format!("failed to create volume dir: {}", vol_path.display()))?;
                info!(volume = %spec.name, path = %vol_path.display(), "created local volume");
                vol_path
            }
            VolumeSource::HostPath { path } => {
                let host_path = PathBuf::from(path);
                if !host_path.exists() {
                    anyhow::bail!("host path does not exist: {}", host_path.display());
                }
                debug!(volume = %spec.name, path = %host_path.display(), "using host path volume");
                host_path
            }
            VolumeSource::Nfs { server, path } => {
                // NFS mount requires platform-specific `mount` syscall.
                // Return error until implemented to prevent silent data loss.
                anyhow::bail!(
                    "NFS mounts not yet implemented (volume '{}', server '{}', path '{}'). \
                     Use Local or HostPath volumes, or mount NFS externally and use HostPath.",
                    spec.name, server, path
                );
            }
        };

        let handle = VolumeHandle {
            name: spec.name.clone(),
            path,
            read_only: spec.read_only,
        };

        self.active
            .write()
            .await
            .insert(spec.name.clone(), handle.clone());

        Ok(handle)
    }

    /// Resolve volume claims against created volumes.
    /// Returns a map of mount_path -> host_path for the driver.
    pub async fn resolve_mounts(
        &self,
        claims: &[VolumeClaim],
    ) -> Result<HashMap<String, String>> {
        let active = self.active.read().await;
        let mut mounts = HashMap::new();

        for claim in claims {
            let handle = active
                .get(&claim.volume_name)
                .with_context(|| format!("volume '{}' not found", claim.volume_name))?;
            mounts.insert(
                handle.path.to_string_lossy().to_string(),
                claim.mount_path.clone(),
            );
        }

        Ok(mounts)
    }

    /// Release a volume by name.
    pub async fn release(&self, name: &str) {
        self.active.write().await.remove(name);
        debug!(volume = name, "released volume");
    }

    /// Delete a local volume (removes the directory).
    pub async fn delete(&self, name: &str) -> Result<()> {
        let path = self.volume_dir.join(name);
        if path.exists() {
            tokio::fs::remove_dir_all(&path)
                .await
                .with_context(|| format!("failed to delete volume: {}", path.display()))?;
            info!(volume = name, "deleted local volume");
        }
        self.active.write().await.remove(name);
        Ok(())
    }

    /// Get a handle to an existing volume.
    pub async fn get(&self, name: &str) -> Option<VolumeHandle> {
        self.active.read().await.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_create_local_volume() {
        let tmp = TempDir::new().unwrap();
        let mgr = VolumeManager::new(tmp.path().to_path_buf());

        let spec = VolumeSpec {
            name: "data".to_string(),
            source: VolumeSource::Local { size_mb: None },
            read_only: false,
        };

        let handle = mgr.create(&spec).await.unwrap();
        assert!(handle.path.exists());
        assert_eq!(handle.name, "data");
    }

    #[tokio::test]
    async fn test_resolve_mounts() {
        let tmp = TempDir::new().unwrap();
        let mgr = VolumeManager::new(tmp.path().to_path_buf());

        let spec = VolumeSpec {
            name: "data".to_string(),
            source: VolumeSource::Local { size_mb: None },
            read_only: false,
        };
        mgr.create(&spec).await.unwrap();

        let claims = vec![VolumeClaim {
            volume_name: "data".to_string(),
            mount_path: "/app/data".to_string(),
            read_only: false,
        }];

        let mounts = mgr.resolve_mounts(&claims).await.unwrap();
        assert_eq!(mounts.len(), 1);
        assert!(mounts.values().next().unwrap() == "/app/data");
    }

    #[tokio::test]
    async fn test_delete_volume() {
        let tmp = TempDir::new().unwrap();
        let mgr = VolumeManager::new(tmp.path().to_path_buf());

        let spec = VolumeSpec {
            name: "ephemeral".to_string(),
            source: VolumeSource::Local { size_mb: None },
            read_only: false,
        };
        let handle = mgr.create(&spec).await.unwrap();
        assert!(handle.path.exists());

        mgr.delete("ephemeral").await.unwrap();
        assert!(!handle.path.exists());
    }
}
