use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::debug;

use tatara_core::domain::allocation::TaskRunState;
use tatara_core::domain::job::{DriverType, Task, TaskConfig};

use super::exec::{is_process_alive, send_signal_and_wait};
use super::{Driver, LogEntry, TaskHandle};

pub struct NixDriver;

#[async_trait]
impl Driver for NixDriver {
    fn name(&self) -> &str {
        "nix"
    }

    async fn available(&self) -> bool {
        Command::new("nix")
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn start(&self, task: &Task, alloc_dir: &Path) -> Result<TaskHandle> {
        let (flake_ref, args) = match &task.config {
            TaskConfig::Nix { flake_ref, args } => (flake_ref.clone(), args.clone()),
            _ => bail!("NixDriver received non-nix task config"),
        };

        let log_dir = alloc_dir.join(&task.name);
        tokio::fs::create_dir_all(&log_dir).await?;

        let stdout_file = std::fs::File::create(log_dir.join("stdout.log"))
            .context("Failed to create stdout log")?;
        let stderr_file = std::fs::File::create(log_dir.join("stderr.log"))
            .context("Failed to create stderr log")?;

        // `nix run <flake_ref> -- <args>`
        let mut cmd = Command::new("nix");
        cmd.arg("run")
            .arg(&flake_ref)
            .arg("--")
            .args(&args)
            .envs(&task.env)
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file))
            .kill_on_drop(false);

        let child = cmd.spawn().context("Failed to spawn nix run")?;
        let pid = child.id();

        debug!(
            task = %task.name,
            flake_ref = %flake_ref,
            pid = ?pid,
            "Nix driver started process"
        );

        std::mem::forget(child);

        Ok(TaskHandle {
            driver: DriverType::Nix,
            pid,
            container_id: None,
            started_at: Utc::now(),
        })
    }

    async fn stop(&self, handle: &TaskHandle, timeout: Duration) -> Result<()> {
        let pid = handle.pid.context("No PID in nix task handle")?;
        send_signal_and_wait(pid, timeout).await
    }

    async fn status(&self, handle: &TaskHandle) -> Result<TaskRunState> {
        let pid = handle.pid.context("No PID in nix task handle")?;
        if is_process_alive(pid) {
            Ok(TaskRunState::Running)
        } else {
            Ok(TaskRunState::Dead)
        }
    }

    async fn logs(&self, _handle: &TaskHandle) -> Result<mpsc::Receiver<LogEntry>> {
        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(async move {
            let _ = tx;
        });
        Ok(rx)
    }
}
