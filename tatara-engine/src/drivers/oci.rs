use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use tatara_core::domain::allocation::TaskRunState;
use tatara_core::domain::job::{DriverType, Task, TaskConfig};

use super::{Driver, LogEntry, TaskHandle};

#[derive(Debug)]
enum OciBackend {
    AppleContainer,
    Docker,
    Podman,
    None,
}

pub struct OciDriver {
    backend: OciBackend,
}

impl OciDriver {
    pub async fn detect() -> Self {
        // Priority:
        // 1. macOS + `container` CLI (Apple macOS 26+)
        // 2. `docker` CLI
        // 3. `podman` CLI

        if cfg!(target_os = "macos") {
            if which("container").await {
                info!("OCI backend: Apple container CLI");
                return Self {
                    backend: OciBackend::AppleContainer,
                };
            }
        }

        if which("docker").await {
            info!("OCI backend: Docker");
            return Self {
                backend: OciBackend::Docker,
            };
        }

        if which("podman").await {
            info!("OCI backend: Podman");
            return Self {
                backend: OciBackend::Podman,
            };
        }

        warn!("No OCI runtime available");
        Self {
            backend: OciBackend::None,
        }
    }

    fn cli_command(&self) -> &str {
        match &self.backend {
            OciBackend::AppleContainer => "container",
            OciBackend::Docker => "docker",
            OciBackend::Podman => "podman",
            OciBackend::None => unreachable!("OCI driver used without available backend"),
        }
    }
}

#[async_trait]
impl Driver for OciDriver {
    fn name(&self) -> &str {
        "oci"
    }

    async fn available(&self) -> bool {
        !matches!(self.backend, OciBackend::None)
    }

    async fn start(&self, task: &Task, _alloc_dir: &Path) -> Result<TaskHandle> {
        let (image, ports, volumes, entrypoint, command) = match &task.config {
            TaskConfig::Oci {
                image,
                ports,
                volumes,
                entrypoint,
                command,
            } => (
                image.clone(),
                ports.clone(),
                volumes.clone(),
                entrypoint.clone(),
                command.clone(),
            ),
            _ => bail!("OciDriver received non-oci task config"),
        };

        let container_name = format!("tatara-{}", task.name);
        let cli = self.cli_command();

        let mut args = vec!["run".to_string(), "-d".to_string()];
        args.push("--name".to_string());
        args.push(container_name.clone());

        for (host_port, container_port) in &ports {
            args.push("-p".to_string());
            args.push(format!("{}:{}", host_port, container_port));
        }

        for (host_path, container_path) in &volumes {
            args.push("-v".to_string());
            args.push(format!("{}:{}", host_path, container_path));
        }

        for (key, value) in &task.env {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        args.push(image);

        if let Some(ep) = &entrypoint {
            args.push("--entrypoint".to_string());
            args.extend(ep.clone());
        }

        if let Some(cmd) = &command {
            args.extend(cmd.clone());
        }

        let output = Command::new(cli)
            .args(&args)
            .output()
            .await
            .context("Failed to start OCI container")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to start container: {}", stderr);
        }

        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        debug!(
            task = %task.name,
            container_id = %container_id,
            backend = ?self.backend,
            "OCI driver started container"
        );

        Ok(TaskHandle {
            driver: DriverType::Oci,
            pid: None,
            container_id: Some(container_id),
            started_at: Utc::now(),
        })
    }

    async fn stop(&self, handle: &TaskHandle, timeout: Duration) -> Result<()> {
        let container_id = handle
            .container_id
            .as_ref()
            .context("No container ID in OCI task handle")?;

        let cli = self.cli_command();
        let timeout_secs = timeout.as_secs().to_string();

        let output = Command::new(cli)
            .args(["stop", "-t", &timeout_secs, container_id])
            .output()
            .await
            .context("Failed to stop container")?;

        if !output.status.success() {
            warn!(
                container_id = %container_id,
                stderr = %String::from_utf8_lossy(&output.stderr),
                "Container stop returned error"
            );
        }

        // Remove container
        let _ = Command::new(cli)
            .args(["rm", "-f", container_id])
            .output()
            .await;

        Ok(())
    }

    async fn status(&self, handle: &TaskHandle) -> Result<TaskRunState> {
        let container_id = handle
            .container_id
            .as_ref()
            .context("No container ID in OCI task handle")?;

        let cli = self.cli_command();
        let output = Command::new(cli)
            .args(["inspect", "--format", "{{.State.Status}}", container_id])
            .output()
            .await
            .context("Failed to inspect container")?;

        if !output.status.success() {
            return Ok(TaskRunState::Dead);
        }

        let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
        match status.as_str() {
            "running" => Ok(TaskRunState::Running),
            "created" | "restarting" => Ok(TaskRunState::Pending),
            _ => Ok(TaskRunState::Dead),
        }
    }

    async fn logs(&self, handle: &TaskHandle) -> Result<mpsc::Receiver<LogEntry>> {
        let (tx, rx) = mpsc::channel(256);
        let container_id = handle
            .container_id
            .clone()
            .unwrap_or_default();
        let cli = self.cli_command().to_string();

        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};

            let mut child = match Command::new(&cli)
                .args(["logs", "-f", "--timestamps", &container_id])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(e) => {
                    tracing::warn!(error = %e, "failed to spawn container log stream");
                    return;
                }
            };

            if let Some(stdout) = child.stdout.take() {
                let tx_out = tx.clone();
                let cid = container_id.clone();
                tokio::spawn(async move {
                    let mut lines = BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let entry = LogEntry {
                            task_name: cid.clone(),
                            message: line,
                            stream: "stdout".to_string(),
                            timestamp: chrono::Utc::now(),
                        };
                        if tx_out.send(entry).await.is_err() {
                            break;
                        }
                    }
                });
            }

            if let Some(stderr) = child.stderr.take() {
                let tx_err = tx;
                let cid = container_id;
                tokio::spawn(async move {
                    let mut lines = BufReader::new(stderr).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let entry = LogEntry {
                            task_name: cid.clone(),
                            message: line,
                            stream: "stderr".to_string(),
                            timestamp: chrono::Utc::now(),
                        };
                        if tx_err.send(entry).await.is_err() {
                            break;
                        }
                    }
                });
            }

            let _ = child.wait().await;
        });

        Ok(rx)
    }
}

async fn which(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}
