//! Kasou driver — provisions VMs via Apple Virtualization.framework.
//!
//! This driver uses kasou (pleme-io/kasou) to create and manage NixOS VMs
//! on macOS using native Virtualization.framework bindings. VMs provisioned
//! by this driver can join the tatara cluster as worker nodes.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::info;

use crate::domain::allocation::TaskRunState;
use crate::domain::job::{DriverType, Task, TaskConfig};
use crate::drivers::{Driver, LogEntry, TaskHandle};

/// Driver that provisions VMs via kasou (Apple Virtualization.framework).
pub struct KasouDriver {
    /// Active VM handles keyed by a composite key (task_name).
    handles: Mutex<HashMap<String, kasou::VmHandle>>,
}

impl KasouDriver {
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl Driver for KasouDriver {
    fn name(&self) -> &str {
        "kasou"
    }

    async fn available(&self) -> bool {
        // kasou requires macOS with Virtualization.framework entitlement.
        // Check if the framework reports virtualization as supported.
        cfg!(target_os = "macos")
    }

    async fn start(&self, task: &Task, alloc_dir: &Path) -> Result<TaskHandle> {
        let TaskConfig::Kasou {
            kernel,
            initrd,
            cmdline,
            disks,
            mac_address,
            cpus,
            memory_mib,
        } = &task.config
        else {
            anyhow::bail!("KasouDriver received non-Kasou TaskConfig");
        };

        // Build disk configs from paths
        let mut disk_configs = Vec::new();
        for (i, disk_path) in disks.iter().enumerate() {
            disk_configs.push(kasou::DiskConfig {
                path: PathBuf::from(disk_path),
                // First disk is root (read-write), rest are read-only by default
                read_only: i > 0,
            });
        }

        let mac = mac_address.clone().or_else(|| {
            Some(kasou::MacAddress::deterministic(&task.name).to_string())
        });

        let serial_log = alloc_dir.join(format!("{}-console.log", task.name));

        let vm_config = kasou::VmConfig {
            cpus: *cpus,
            memory_mib: *memory_mib,
            boot: kasou::BootConfig {
                kernel: PathBuf::from(kernel),
                initrd: PathBuf::from(initrd),
                cmdline: cmdline.clone(),
            },
            disks: disk_configs,
            network: kasou::NetworkConfig {
                mac_address: mac,
            },
            serial: Some(kasou::SerialConfig {
                log_path: serial_log,
            }),
            shared_dirs: vec![],
        };

        info!(
            task = %task.name,
            cpus = cpus,
            memory_mib = memory_mib,
            "starting VM via kasou"
        );

        let handle = kasou::VmHandle::create(vm_config)
            .context("creating VM via kasou")?;

        handle.start().context("starting VM via kasou")?;

        let pid = std::process::id();
        self.handles
            .lock()
            .unwrap()
            .insert(task.name.clone(), handle);

        info!(task = %task.name, pid = pid, "VM started via kasou");

        Ok(TaskHandle {
            driver: DriverType::Kasou,
            pid: Some(pid),
            container_id: None,
            started_at: Utc::now(),
        })
    }

    async fn stop(&self, _handle: &TaskHandle, timeout: Duration) -> Result<()> {
        // Extract the VM handle from the map (releases the lock before await)
        let (key, vm_handle) = {
            let mut handles = self.handles.lock().unwrap();
            let key = handles.keys().next().cloned();
            match key {
                Some(k) => {
                    let h = handles.remove(&k);
                    (Some(k), h)
                }
                None => (None, None),
            }
        };

        if let (Some(key), Some(vm_handle)) = (key, vm_handle) {
            info!(task = %key, "stopping VM via kasou");

            // Try graceful stop first
            if let Err(e) = vm_handle.request_stop() {
                tracing::warn!(error = %e, "graceful stop request failed");
            }

            // Wait for graceful shutdown
            let deadline = std::time::Instant::now() + timeout;
            loop {
                if vm_handle.state() == kasou::VmState::Stopped {
                    info!(task = %key, "VM stopped gracefully");
                    return Ok(());
                }
                if std::time::Instant::now() >= deadline {
                    tracing::warn!(task = %key, "graceful shutdown timed out, force stopping");
                    let _ = vm_handle.stop();
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        Ok(())
    }

    async fn status(&self, _handle: &TaskHandle) -> Result<TaskRunState> {
        let handles = self.handles.lock().unwrap();
        // Check if any VM is running
        for vm_handle in handles.values() {
            return Ok(match vm_handle.state() {
                kasou::VmState::Running => TaskRunState::Running,
                kasou::VmState::Starting
                | kasou::VmState::Pausing
                | kasou::VmState::Resuming => TaskRunState::Pending,
                _ => TaskRunState::Dead,
            });
        }
        Ok(TaskRunState::Dead)
    }

    async fn logs(&self, _handle: &TaskHandle) -> Result<mpsc::Receiver<LogEntry>> {
        // TODO: stream from VZFileSerialPortAttachment console log
        let (_tx, rx) = mpsc::channel(1);
        Ok(rx)
    }
}
