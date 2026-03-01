use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::domain::allocation::TaskRunState;
use crate::domain::job::{DriverType, Task, TaskConfig};

use super::{Driver, LogEntry, TaskHandle};

pub struct ExecDriver;

#[async_trait]
impl Driver for ExecDriver {
    fn name(&self) -> &str {
        "exec"
    }

    async fn available(&self) -> bool {
        true
    }

    async fn start(&self, task: &Task, alloc_dir: &Path) -> Result<TaskHandle> {
        let (command, args, working_dir) = match &task.config {
            TaskConfig::Exec {
                command,
                args,
                working_dir,
            } => (command.clone(), args.clone(), working_dir.clone()),
            _ => bail!("ExecDriver received non-exec task config"),
        };

        let log_dir = alloc_dir.join(&task.name);
        tokio::fs::create_dir_all(&log_dir).await?;

        let stdout_path = log_dir.join("stdout.log");
        let stderr_path = log_dir.join("stderr.log");

        let stdout_file = std::fs::File::create(&stdout_path)
            .context("Failed to create stdout log")?;
        let stderr_file = std::fs::File::create(&stderr_path)
            .context("Failed to create stderr log")?;

        let mut cmd = Command::new(&command);
        cmd.args(&args)
            .envs(&task.env)
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file))
            .kill_on_drop(false);

        if let Some(dir) = &working_dir {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn().context("Failed to spawn process")?;
        let pid = child.id();

        debug!(
            task = %task.name,
            command = %command,
            pid = ?pid,
            "Exec driver started process"
        );

        // Detach — the process runs independently. We track by PID.
        std::mem::forget(child);

        Ok(TaskHandle {
            driver: DriverType::Exec,
            pid,
            container_id: None,
            started_at: Utc::now(),
        })
    }

    async fn stop(&self, handle: &TaskHandle, timeout: Duration) -> Result<()> {
        let pid = handle.pid.context("No PID in exec task handle")?;
        send_signal_and_wait(pid, timeout).await
    }

    async fn status(&self, handle: &TaskHandle) -> Result<TaskRunState> {
        let pid = handle.pid.context("No PID in exec task handle")?;

        if is_process_alive(pid) {
            Ok(TaskRunState::Running)
        } else {
            Ok(TaskRunState::Dead)
        }
    }

    async fn logs(&self, _handle: &TaskHandle) -> Result<mpsc::Receiver<LogEntry>> {
        let (tx, rx) = mpsc::channel(256);
        // Logs are read via LogCollector, not streamed from the driver.
        tokio::spawn(async move {
            let _ = tx;
        });
        Ok(rx)
    }
}

#[cfg(unix)]
pub(crate) async fn send_signal_and_wait(pid: u32, timeout: Duration) -> Result<()> {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    let nix_pid = Pid::from_raw(pid as i32);

    // Send SIGTERM
    let _ = kill(nix_pid, Signal::SIGTERM);

    // Wait for graceful shutdown
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if !is_process_alive(pid) {
            debug!(pid, "Process terminated gracefully");
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Force kill
    warn!(pid, "Process did not terminate gracefully, sending SIGKILL");
    let _ = kill(nix_pid, Signal::SIGKILL);
    Ok(())
}

#[cfg(not(unix))]
pub(crate) async fn send_signal_and_wait(_pid: u32, _timeout: Duration) -> Result<()> {
    anyhow::bail!("Process signal management not supported on this platform")
}

pub(crate) fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        kill(Pid::from_raw(pid as i32), None).is_ok()
    }
    #[cfg(not(unix))]
    {
        false
    }
}
