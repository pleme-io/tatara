use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use tatara_core::domain::allocation::TaskRunState;
use tatara_core::domain::job::{DriverType, Task, TaskConfig};

use super::exec::{is_process_alive, send_signal_and_wait};
use super::{Driver, LogEntry, TaskHandle};

/// Driver for `nix build` — produces store paths and optionally pushes to Attic cache.
pub struct NixBuildDriver;

#[async_trait]
impl Driver for NixBuildDriver {
    fn name(&self) -> &str {
        "nix-build"
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
        let (flake_ref, system, extra_args, attic_cache) = match &task.config {
            TaskConfig::NixBuild {
                flake_ref,
                system,
                extra_args,
                attic_cache,
            } => (
                flake_ref.clone(),
                system.clone(),
                extra_args.clone(),
                attic_cache.clone(),
            ),
            _ => bail!("NixBuildDriver received non-nix-build task config"),
        };

        let log_dir = alloc_dir.join(&task.name);
        tokio::fs::create_dir_all(&log_dir).await?;

        let stdout_file = std::fs::File::create(log_dir.join("stdout.log"))
            .context("Failed to create stdout log")?;
        let stderr_file = std::fs::File::create(log_dir.join("stderr.log"))
            .context("Failed to create stderr log")?;

        // Build the nix command
        let mut cmd = Command::new("nix");
        cmd.arg("build")
            .arg(&flake_ref)
            .arg("--print-out-paths")
            .arg("--no-link");

        if let Some(sys) = &system {
            cmd.arg("--system").arg(sys);
        }

        for arg in &extra_args {
            cmd.arg(arg);
        }

        cmd.envs(&task.env)
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file))
            .kill_on_drop(false);

        info!(
            flake_ref = %flake_ref,
            system = ?system,
            attic_cache = ?attic_cache,
            "Starting nix build"
        );

        let child = cmd.spawn().context("Failed to spawn nix build")?;
        let pid = child.id();

        // Spawn background task to wait for completion and optionally push to Attic
        let log_dir_clone = log_dir.clone();
        tokio::spawn(async move {
            let output = child.wait_with_output().await;
            match output {
                Ok(out) if out.status.success() => {
                    // Read store path from stdout log
                    if let Ok(stdout) = tokio::fs::read_to_string(log_dir_clone.join("stdout.log")).await {
                        let store_path = stdout.trim();
                        info!(store_path = %store_path, "nix build complete");

                        // Push to Attic if configured
                        if let Some(cache_name) = attic_cache {
                            info!(cache = %cache_name, "Pushing to Attic cache");
                            let push_result = Command::new("attic")
                                .arg("push")
                                .arg(&cache_name)
                                .arg(store_path)
                                .output()
                                .await;

                            match push_result {
                                Ok(r) if r.status.success() => {
                                    info!(cache = %cache_name, "Attic push complete");
                                }
                                Ok(r) => {
                                    warn!(
                                        cache = %cache_name,
                                        stderr = %String::from_utf8_lossy(&r.stderr),
                                        "Attic push failed (non-fatal)"
                                    );
                                }
                                Err(e) => {
                                    warn!(cache = %cache_name, error = %e, "Attic push command failed");
                                }
                            }
                        }
                    }
                }
                Ok(out) => {
                    warn!(exit_code = ?out.status.code(), "nix build failed");
                }
                Err(e) => {
                    warn!(error = %e, "Failed to wait for nix build");
                }
            }
        });

        Ok(TaskHandle {
            driver: DriverType::NixBuild,
            pid,
            container_id: None,
            started_at: Utc::now(),
        })
    }

    async fn stop(&self, handle: &TaskHandle, timeout: Duration) -> Result<()> {
        if let Some(pid) = handle.pid {
            send_signal_and_wait(pid, timeout).await?;
        }
        Ok(())
    }

    async fn status(&self, handle: &TaskHandle) -> Result<TaskRunState> {
        match handle.pid {
            Some(pid) if is_process_alive(pid) => Ok(TaskRunState::Running),
            Some(_) => Ok(TaskRunState::Dead),
            None => Ok(TaskRunState::Dead),
        }
    }

    async fn logs(&self, handle: &TaskHandle) -> Result<mpsc::Receiver<LogEntry>> {
        // TODO: Stream from build log file
        let (_tx, rx) = mpsc::channel(1);
        Ok(rx)
    }
}
