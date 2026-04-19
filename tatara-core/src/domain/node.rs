use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use super::job::{DriverType, Resources};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Ready,
    Down,
    Draining,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub address: String,
    pub status: NodeStatus,
    #[serde(default = "default_eligible")]
    pub eligible: bool,
    pub total_resources: Resources,
    pub available_resources: Resources,
    pub attributes: HashMap<String, String>,
    pub drivers: Vec<DriverType>,
    pub last_heartbeat: DateTime<Utc>,
    pub allocations: Vec<Uuid>,
}

fn default_eligible() -> bool {
    true
}

impl Node {
    pub fn local() -> Self {
        let os = std::env::consts::OS.to_string();
        let arch = std::env::consts::ARCH.to_string();
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let mut attributes = HashMap::new();
        attributes.insert("os".to_string(), os.clone());
        attributes.insert("arch".to_string(), arch);
        attributes.insert("hostname".to_string(), hostname.clone());

        let (cpu_mhz, memory_mb) = detect_resources();

        Self {
            id: hostname,
            address: "127.0.0.1:4647".to_string(),
            status: NodeStatus::Ready,
            eligible: true,
            total_resources: Resources { cpu_mhz, memory_mb },
            available_resources: Resources { cpu_mhz, memory_mb },
            attributes,
            drivers: vec![DriverType::Exec],
            last_heartbeat: Utc::now(),
            allocations: Vec::new(),
        }
    }
}

fn detect_resources() -> (u64, u64) {
    let cpu_mhz = (num_cpus() as u64) * 1000;

    #[cfg(target_os = "macos")]
    let memory_mb = {
        use std::process::Command;
        Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8(o.stdout)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
            })
            .unwrap_or(0)
            / (1024 * 1024)
    };

    #[cfg(target_os = "linux")]
    let memory_mb = {
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("MemTotal:"))
                    .and_then(|l| {
                        l.split_whitespace()
                            .nth(1)
                            .and_then(|v| v.parse::<u64>().ok())
                    })
            })
            .unwrap_or(0)
            / 1024
    };

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let memory_mb = 0;

    (cpu_mhz, memory_mb)
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
}
