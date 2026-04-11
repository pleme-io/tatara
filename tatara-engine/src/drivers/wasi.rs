//! WASI driver — runs WASM/WASI components as tatara workloads via wasmtime.
//!
//! Phase 1: subprocess approach (wasmtime CLI).
//! Phase 2: embedded wasmtime (library) with host functions.
//!
//! The complete sandwich:
//! - eBPF hooks the kernel boundary (below)
//! - WASI standardizes the userspace boundary (above)
//! - Together every system call is observable and controllable

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::{Driver, LogEntry, TaskHandle};
use tatara_core::domain::allocation::TaskRunState;
use tatara_core::domain::job::{DriverType, Task, TaskConfig};

/// WASI driver — runs WASM/WASI components via wasmtime subprocess.
///
/// Detects wasmtime on PATH. Maps WasiCapabilities to --wasi flags.
/// Log capture via stdout/stderr files (same pattern as ExecDriver).
/// Process lifecycle via PID (same pattern as NixDriver).
pub struct WasiDriver;

#[async_trait]
impl Driver for WasiDriver {
    fn name(&self) -> &str {
        "wasi"
    }

    async fn available(&self) -> bool {
        Command::new("wasmtime")
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn start(&self, task: &Task, alloc_dir: &Path) -> Result<TaskHandle> {
        let (wasm_path, capabilities, mounts, allowed_services) = match &task.config {
            TaskConfig::Wasi {
                wasm_path,
                capabilities,
                mounts,
                allowed_services,
            } => (wasm_path, capabilities, mounts, allowed_services),
            _ => bail!("WasiDriver received non-WASI task config"),
        };

        let mut cmd = Command::new("wasmtime");
        cmd.arg("run");

        // Map capabilities to --wasi flags (WASI Preview 2 inherit model)
        if capabilities.network {
            cmd.args(["--wasi", "inherit-network"]);
        }
        if capabilities.filesystem {
            cmd.args(["--wasi", "inherit-filesystem"]);
        }
        if capabilities.clocks {
            cmd.args(["--wasi", "inherit-clocks"]);
        }
        if capabilities.random {
            cmd.args(["--wasi", "inherit-random"]);
        }
        if capabilities.stdout || capabilities.stderr {
            cmd.args(["--wasi", "inherit-stdio"]);
        }

        // Filesystem mounts as --dir flags
        for (host_path, guest_path) in mounts {
            cmd.args(["--dir", &format!("{host_path}::{guest_path}")]);
        }

        // Environment variables
        for (k, v) in &task.env {
            cmd.args(["--env", &format!("{k}={v}")]);
        }

        // Fuel metering for resource limits (heuristic: cpu_mhz * 1M instructions)
        if task.resources.cpu_mhz > 0 {
            let fuel = task.resources.cpu_mhz * 1_000_000;
            cmd.args(["--fuel", &fuel.to_string()]);
        }

        // The WASM component to run
        cmd.arg(wasm_path);

        // Log capture to allocation directory
        let log_dir = alloc_dir.join(&task.name);
        tokio::fs::create_dir_all(&log_dir)
            .await
            .context("Failed to create WASI log directory")?;

        let stdout_file = std::fs::File::create(log_dir.join("stdout.log"))
            .context("Failed to create stdout log")?;
        let stderr_file = std::fs::File::create(log_dir.join("stderr.log"))
            .context("Failed to create stderr log")?;

        cmd.stdout(stdout_file);
        cmd.stderr(stderr_file);
        cmd.kill_on_drop(false);

        info!(
            task = %task.name,
            wasm_path = %wasm_path,
            network = capabilities.network,
            filesystem = capabilities.filesystem,
            "starting WASI component"
        );

        let child = cmd.spawn().context("Failed to spawn wasmtime")?;
        let pid = child.id();

        // Detach the child process (tatara manages lifecycle via PID)
        std::mem::forget(child);

        Ok(TaskHandle {
            driver: DriverType::Wasi,
            pid,
            container_id: None,
            started_at: Utc::now(),
        })
    }

    async fn stop(&self, handle: &TaskHandle, timeout: Duration) -> Result<()> {
        let Some(pid) = handle.pid else {
            return Ok(());
        };

        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;

            let pid = Pid::from_raw(pid as i32);

            // SIGTERM first (graceful)
            if kill(pid, Signal::SIGTERM).is_err() {
                return Ok(()); // Process already gone
            }

            // Wait for process to exit
            let deadline = tokio::time::Instant::now() + timeout;
            loop {
                if kill(pid, None).is_err() {
                    return Ok(()); // Process exited
                }
                if tokio::time::Instant::now() >= deadline {
                    // Force kill
                    let _ = kill(pid, Signal::SIGKILL);
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        #[cfg(not(unix))]
        {
            warn!("WASI process signal management not supported on this platform");
            Ok(())
        }
    }

    async fn status(&self, handle: &TaskHandle) -> Result<TaskRunState> {
        let Some(pid) = handle.pid else {
            return Ok(TaskRunState::Dead);
        };

        #[cfg(unix)]
        {
            use nix::sys::signal::kill;
            use nix::unistd::Pid;

            let pid = Pid::from_raw(pid as i32);
            if kill(pid, None).is_ok() {
                Ok(TaskRunState::Running)
            } else {
                Ok(TaskRunState::Dead)
            }
        }

        #[cfg(not(unix))]
        {
            Ok(TaskRunState::Dead)
        }
    }

    async fn logs(&self, handle: &TaskHandle) -> Result<mpsc::Receiver<LogEntry>> {
        // Same pattern as ExecDriver — tail log files
        let (tx, rx) = mpsc::channel(256);

        // Log streaming is handled by the LogCollector which reads
        // stdout.log/stderr.log from the allocation directory.
        // This method returns an empty channel; real streaming happens
        // via the /api/v1/allocations/{id}/logs endpoint.
        tokio::spawn(async move {
            let _ = tx;
            // Log tailing handled by LogCollector
        });

        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_wasi_driver_name() {
        let driver = WasiDriver;
        assert_eq!(driver.name(), "wasi");
    }

    #[tokio::test]
    async fn test_wasi_driver_available() {
        let driver = WasiDriver;
        // May or may not be available depending on environment
        let _ = driver.available().await;
    }
}
