pub mod exec;
#[cfg(target_os = "macos")]
pub mod kasou;
pub mod nix;
pub mod nix_build;
pub mod oci;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

use tatara_core::domain::allocation::TaskRunState;
use tatara_core::domain::job::{DriverType, Task};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHandle {
    pub driver: DriverType,
    pub pid: Option<u32>,
    pub container_id: Option<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub task_name: String,
    pub message: String,
    pub stream: String,
    pub timestamp: DateTime<Utc>,
}

#[async_trait]
pub trait Driver: Send + Sync {
    fn name(&self) -> &str;
    async fn available(&self) -> bool;
    async fn start(&self, task: &Task, alloc_dir: &Path) -> Result<TaskHandle>;
    async fn stop(&self, handle: &TaskHandle, timeout: Duration) -> Result<()>;
    async fn status(&self, handle: &TaskHandle) -> Result<TaskRunState>;
    async fn logs(&self, handle: &TaskHandle) -> Result<mpsc::Receiver<LogEntry>>;
}

pub struct DriverRegistry {
    drivers: Vec<Box<dyn Driver>>,
}

impl DriverRegistry {
    pub async fn new() -> Self {
        let mut drivers: Vec<Box<dyn Driver>> = Vec::new();

        let exec = exec::ExecDriver;
        if exec.available().await {
            drivers.push(Box::new(exec));
        }

        let oci = oci::OciDriver::detect().await;
        if oci.available().await {
            drivers.push(Box::new(oci));
        }

        let nix = nix::NixDriver;
        if nix.available().await {
            drivers.push(Box::new(nix));
        }

        let nix_build = nix_build::NixBuildDriver;
        if nix_build.available().await {
            drivers.push(Box::new(nix_build));
        }

        #[cfg(target_os = "macos")]
        {
            let kasou_driver = kasou::KasouDriver::new();
            if kasou_driver.available().await {
                drivers.push(Box::new(kasou_driver));
            }
        }

        Self { drivers }
    }

    pub fn get(&self, driver_type: &DriverType) -> Option<&dyn Driver> {
        let name = match driver_type {
            DriverType::Exec => "exec",
            DriverType::Oci => "oci",
            DriverType::Nix => "nix",
            DriverType::NixBuild => "nix-build",
            DriverType::Kasou => "kasou",
            DriverType::Kube => "kube",
            DriverType::Wasi => "wasi",
        };
        self.drivers.iter().find(|d| d.name() == name).map(|d| d.as_ref())
    }

    pub fn available_drivers(&self) -> Vec<DriverType> {
        self.drivers
            .iter()
            .filter_map(|d| match d.name() {
                "exec" => Some(DriverType::Exec),
                "oci" => Some(DriverType::Oci),
                "nix" => Some(DriverType::Nix),
                "nix-build" => Some(DriverType::NixBuild),
                "kasou" => Some(DriverType::Kasou),
                "kube" => Some(DriverType::Kube),
                "wasi" => Some(DriverType::Wasi),
                _ => None,
            })
            .collect()
    }
}
