use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    Service,
    Batch,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Dead,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DriverType {
    Exec,
    Oci,
    Nix,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RestartMode {
    OnFailure,
    Always,
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub version: u64,
    pub job_type: JobType,
    pub status: JobStatus,
    pub submitted_at: DateTime<Utc>,
    pub groups: Vec<TaskGroup>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    #[serde(default)]
    pub meta: HashMap<String, String>,
    /// SHA-256 hash of the serialized JobSpec, used for drift detection.
    #[serde(default)]
    pub spec_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGroup {
    pub name: String,
    #[serde(default = "default_count")]
    pub count: u32,
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub restart_policy: RestartPolicy,
    #[serde(default)]
    pub resources: Resources,
    pub network: Option<NetworkConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartPolicy {
    #[serde(default = "default_restart_mode")]
    pub mode: RestartMode,
    #[serde(default = "default_restart_attempts")]
    pub attempts: u32,
    #[serde(default = "default_restart_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_restart_delay")]
    pub delay_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub name: String,
    pub driver: DriverType,
    pub config: TaskConfig,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub resources: Resources,
    #[serde(default)]
    pub health_checks: Vec<HealthCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskConfig {
    Exec {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        working_dir: Option<String>,
    },
    Oci {
        image: String,
        #[serde(default)]
        ports: HashMap<String, String>,
        #[serde(default)]
        volumes: HashMap<String, String>,
        entrypoint: Option<Vec<String>>,
        command: Option<Vec<String>>,
    },
    Nix {
        flake_ref: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Resources {
    #[serde(default)]
    pub cpu_mhz: u64,
    #[serde(default)]
    pub memory_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub attribute: String,
    #[serde(default = "default_operator")]
    pub operator: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    #[serde(default)]
    pub ports: Vec<PortMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub label: String,
    pub value: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HealthCheck {
    Http {
        port: u16,
        path: String,
        #[serde(default = "default_health_interval")]
        interval_secs: u64,
        #[serde(default = "default_health_timeout")]
        timeout_secs: u64,
    },
    Exec {
        command: String,
        #[serde(default = "default_health_interval")]
        interval_secs: u64,
        #[serde(default = "default_health_timeout")]
        timeout_secs: u64,
    },
    Tcp {
        port: u16,
        #[serde(default = "default_health_interval")]
        interval_secs: u64,
        #[serde(default = "default_health_timeout")]
        timeout_secs: u64,
    },
}

/// A submitted job specification (before scheduling).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpec {
    pub id: String,
    #[serde(default = "default_job_type")]
    pub job_type: JobType,
    pub groups: Vec<TaskGroup>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    #[serde(default)]
    pub meta: HashMap<String, String>,
}

impl JobSpec {
    pub fn into_job(self) -> Job {
        let spec_hash = Some(self.content_hash());
        Job {
            id: self.id,
            version: 1,
            job_type: self.job_type,
            status: JobStatus::Pending,
            submitted_at: Utc::now(),
            groups: self.groups,
            constraints: self.constraints,
            meta: self.meta,
            spec_hash,
        }
    }

    /// Compute a SHA-256 hash of the canonical JSON representation of this spec.
    pub fn content_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let canonical = serde_json::to_string(self).unwrap_or_default();
        let hash = Sha256::digest(canonical.as_bytes());
        format!("{:x}", hash)
    }
}

fn default_count() -> u32 {
    1
}

fn default_restart_mode() -> RestartMode {
    RestartMode::OnFailure
}

fn default_restart_attempts() -> u32 {
    3
}

fn default_restart_interval() -> u64 {
    300
}

fn default_restart_delay() -> u64 {
    5
}

fn default_operator() -> String {
    "=".to_string()
}

fn default_health_interval() -> u64 {
    10
}

fn default_health_timeout() -> u64 {
    5
}

fn default_job_type() -> JobType {
    JobType::Service
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            mode: default_restart_mode(),
            attempts: default_restart_attempts(),
            interval_secs: default_restart_interval(),
            delay_secs: default_restart_delay(),
        }
    }
}
