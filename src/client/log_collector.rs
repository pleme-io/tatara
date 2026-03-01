use anyhow::{Context, Result};
use chrono::Utc;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::debug;

use crate::drivers::LogEntry;

pub struct LogCollector {
    alloc_dir: PathBuf,
}

impl LogCollector {
    pub fn new(alloc_dir: PathBuf) -> Self {
        Self { alloc_dir }
    }

    /// Read existing logs for an allocation's task.
    pub async fn read_logs(
        &self,
        alloc_id: &str,
        task_name: &str,
    ) -> Result<Vec<LogEntry>> {
        let task_dir = self.alloc_dir.join(alloc_id).join(task_name);
        let mut entries = Vec::new();

        for (stream, filename) in [("stdout", "stdout.log"), ("stderr", "stderr.log")] {
            let path = task_dir.join(filename);
            if path.exists() {
                let content = tokio::fs::read_to_string(&path)
                    .await
                    .with_context(|| format!("Failed to read {}", path.display()))?;

                for line in content.lines() {
                    entries.push(LogEntry {
                        task_name: task_name.to_string(),
                        message: line.to_string(),
                        stream: stream.to_string(),
                        timestamp: Utc::now(),
                    });
                }
            }
        }

        Ok(entries)
    }

    /// Stream logs by tailing log files. Returns a channel of log entries.
    pub async fn tail_logs(
        &self,
        alloc_id: &str,
        task_name: &str,
    ) -> Result<mpsc::Receiver<LogEntry>> {
        let (tx, rx) = mpsc::channel(256);
        let stdout_path = self
            .alloc_dir
            .join(alloc_id)
            .join(task_name)
            .join("stdout.log");
        let stderr_path = self
            .alloc_dir
            .join(alloc_id)
            .join(task_name)
            .join("stderr.log");
        let task_name_stdout = task_name.to_string();
        let task_name_stderr = task_name.to_string();
        let tx_stderr = tx.clone();

        tokio::spawn(async move {
            tail_file(stdout_path, "stdout", &task_name_stdout, tx).await;
        });

        tokio::spawn(async move {
            tail_file(stderr_path, "stderr", &task_name_stderr, tx_stderr).await;
        });

        Ok(rx)
    }
}

async fn tail_file(path: PathBuf, stream: &str, task_name: &str, tx: mpsc::Sender<LogEntry>) {
    use tokio::io::{AsyncBufReadExt, BufReader};

    // Wait for file to exist
    loop {
        if path.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(e) => {
            debug!(path = %path.display(), error = %e, "Failed to open log file");
            return;
        }
    };

    let mut reader = BufReader::new(file).lines();
    let stream = stream.to_string();
    let task_name = task_name.to_string();

    loop {
        match reader.next_line().await {
            Ok(Some(line)) => {
                let entry = LogEntry {
                    task_name: task_name.clone(),
                    message: line,
                    stream: stream.clone(),
                    timestamp: Utc::now(),
                };
                if tx.send(entry).await.is_err() {
                    return;
                }
            }
            Ok(None) => {
                // EOF — wait and retry (tail -f behavior)
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            Err(_) => return,
        }
    }
}
