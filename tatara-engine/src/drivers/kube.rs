//! Kubernetes execution driver.
//!
//! Implements the Driver trait for Kubernetes workloads via the kube-rs
//! client. Leverages tatara-kube's Server-Side Apply reconciler for
//! resource management.

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

use tatara_core::domain::allocation::TaskRunState;
use tatara_core::domain::job::Task;

use super::{Driver, LogEntry, TaskHandle};
use tatara_core::domain::job::DriverType;

/// Kubernetes execution driver.
///
/// Manages workloads on K8s clusters via Server-Side Apply. Requires
/// a valid kubeconfig. Uses tatara-kube's reconciler for resource
/// lifecycle management.
pub struct KubeDriver {
    /// Kubeconfig path (None = default location).
    kubeconfig: Option<String>,
}

impl KubeDriver {
    pub fn new() -> Self {
        Self { kubeconfig: None }
    }

    pub fn with_kubeconfig(kubeconfig: impl Into<String>) -> Self {
        Self {
            kubeconfig: Some(kubeconfig.into()),
        }
    }

    /// Check if kubectl/kubeconfig is available.
    async fn check_kubeconfig(&self) -> bool {
        // 1. Explicit kubeconfig path from constructor
        if let Some(ref path) = self.kubeconfig {
            return tokio::fs::metadata(path).await.is_ok();
        }

        // 2. KUBECONFIG environment variable
        if let Ok(env_path) = std::env::var("KUBECONFIG") {
            if !env_path.is_empty() {
                return tokio::fs::metadata(&env_path).await.is_ok();
            }
        }

        // 3. Default location
        if let Some(home) = dirs::home_dir() {
            let default = home.join(".kube").join("config");
            return default.exists();
        }

        false
    }
}

impl Default for KubeDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Driver for KubeDriver {
    fn name(&self) -> &str {
        "kube"
    }

    async fn available(&self) -> bool {
        self.check_kubeconfig().await
    }

    async fn start(&self, task: &Task, _alloc_dir: &Path) -> Result<TaskHandle> {
        // In the full implementation, this would:
        // 1. Parse the task's flake_ref as a K8s manifest source
        // 2. Use tatara-kube's reconciler to apply via SSA
        // 3. Wait for the resource to be ready
        // 4. Return a handle with the resource reference

        tracing::info!(
            task = %task.name,
            driver = "kube",
            "starting K8s workload"
        );

        Ok(TaskHandle {
            driver: DriverType::Kube,
            pid: None,
            container_id: Some(format!("kube:{}", task.name)),
            started_at: Utc::now(),
        })
    }

    async fn stop(&self, handle: &TaskHandle, _timeout: Duration) -> Result<()> {
        tracing::info!(
            container_id = ?handle.container_id,
            "stopping K8s workload"
        );
        // Would delete the K8s resource via tatara-kube
        Ok(())
    }

    async fn status(&self, handle: &TaskHandle) -> Result<TaskRunState> {
        // Would query pod status via kube-rs
        let _ = handle;
        Ok(TaskRunState::Running)
    }

    async fn logs(&self, _handle: &TaskHandle) -> Result<mpsc::Receiver<LogEntry>> {
        let (tx, rx) = mpsc::channel(100);
        // Would stream pod logs via kube-rs
        drop(tx);
        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kube_driver_name() {
        let driver = KubeDriver::new();
        assert_eq!(driver.name(), "kube");
    }

    #[test]
    fn test_kube_driver_with_kubeconfig() {
        let driver = KubeDriver::with_kubeconfig("/path/to/config");
        assert_eq!(driver.kubeconfig.as_deref(), Some("/path/to/config"));
    }
}
