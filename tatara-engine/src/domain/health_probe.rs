//! Health probe executor for HTTP, TCP, and Exec checks.
//!
//! Executes health probes declared in task specs and returns results
//! that feed into the reconciler's restart logic and service catalog.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tatara_core::domain::job::HealthCheck;
use tokio::net::TcpStream;
use tokio::process::Command;
use tracing::{debug, warn};

/// Result of a single probe execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeResult {
    Passing { latency_ms: u64 },
    Warning { message: String, latency_ms: u64 },
    Critical { message: String },
    Timeout,
}

impl ProbeResult {
    pub fn is_passing(&self) -> bool {
        matches!(self, Self::Passing { .. })
    }

    pub fn is_failing(&self) -> bool {
        matches!(self, Self::Critical { .. } | Self::Timeout)
    }
}

/// Tracked state for a health probe across reconciler ticks.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProbeState {
    pub last_result: Option<ProbeResult>,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub last_checked: Option<DateTime<Utc>>,
}

/// Executes health probes against running tasks.
pub struct ProbeExecutor {
    client: reqwest::Client,
}

impl ProbeExecutor {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .no_proxy()
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// Execute a health check and return the result.
    pub async fn execute(&self, check: &HealthCheck) -> ProbeResult {
        match check {
            HealthCheck::Http {
                port,
                path,
                timeout_secs,
                ..
            } => self.probe_http(*port, path, *timeout_secs).await,
            HealthCheck::Tcp {
                port, timeout_secs, ..
            } => self.probe_tcp(*port, *timeout_secs).await,
            HealthCheck::Exec {
                command,
                timeout_secs,
                ..
            } => self.probe_exec(command, *timeout_secs).await,
        }
    }

    async fn probe_http(&self, port: u16, path: &str, timeout_secs: u64) -> ProbeResult {
        let url = format!("http://127.0.0.1:{port}{path}");
        let start = std::time::Instant::now();

        match tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            self.client.get(&url).send(),
        )
        .await
        {
            Ok(Ok(resp)) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let status = resp.status();
                if status.is_success() {
                    debug!(url = %url, status = %status, latency_ms, "health check passed");
                    ProbeResult::Passing { latency_ms }
                } else if status.as_u16() == 429 {
                    ProbeResult::Warning {
                        message: format!("rate limited (HTTP 429)"),
                        latency_ms,
                    }
                } else if status.is_server_error() {
                    ProbeResult::Critical {
                        message: format!("HTTP {status}"),
                    }
                } else {
                    ProbeResult::Warning {
                        message: format!("HTTP {status}"),
                        latency_ms,
                    }
                }
            }
            Ok(Err(e)) => {
                warn!(url = %url, error = %e, "health check failed");
                ProbeResult::Critical {
                    message: e.to_string(),
                }
            }
            Err(_) => ProbeResult::Timeout,
        }
    }

    async fn probe_tcp(&self, port: u16, timeout_secs: u64) -> ProbeResult {
        let addr = format!("127.0.0.1:{port}");
        let start = std::time::Instant::now();

        match tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            TcpStream::connect(&addr),
        )
        .await
        {
            Ok(Ok(_)) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                debug!(addr = %addr, latency_ms, "TCP health check passed");
                ProbeResult::Passing { latency_ms }
            }
            Ok(Err(e)) => ProbeResult::Critical {
                message: e.to_string(),
            },
            Err(_) => ProbeResult::Timeout,
        }
    }

    async fn probe_exec(&self, command: &str, timeout_secs: u64) -> ProbeResult {
        let start = std::time::Instant::now();

        let parts: Vec<&str> = command.split_whitespace().collect();
        let (cmd, args) = match parts.split_first() {
            Some((cmd, args)) => (*cmd, args),
            None => {
                return ProbeResult::Critical {
                    message: "empty command".to_string(),
                }
            }
        };

        match tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            Command::new(cmd).args(args).output(),
        )
        .await
        {
            Ok(Ok(output)) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                if output.status.success() {
                    ProbeResult::Passing { latency_ms }
                } else {
                    ProbeResult::Critical {
                        message: format!("exit code: {:?}", output.status.code()),
                    }
                }
            }
            Ok(Err(e)) => ProbeResult::Critical {
                message: e.to_string(),
            },
            Err(_) => ProbeResult::Timeout,
        }
    }
}

impl Default for ProbeExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_probe_refuses_unbound_port() {
        let executor = ProbeExecutor::new();
        let result = executor.probe_tcp(59999, 2).await;
        assert!(result.is_failing());
    }

    #[tokio::test]
    async fn test_exec_probe_true() {
        let executor = ProbeExecutor::new();
        let result = executor.probe_exec("true", 5).await;
        assert!(result.is_passing());
    }

    #[tokio::test]
    async fn test_exec_probe_false() {
        let executor = ProbeExecutor::new();
        let result = executor.probe_exec("false", 5).await;
        assert!(result.is_failing());
    }
}
