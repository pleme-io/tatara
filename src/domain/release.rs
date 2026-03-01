use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::job::JobSpec;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseStatus {
    Pending,
    Active,
    Superseded,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub id: Uuid,
    pub name: String,
    pub flake_ref: String,
    pub flake_rev: Option<String>,
    pub job_id: String,
    pub job_spec_snapshot: Option<JobSpec>,
    pub version: u64,
    pub status: ReleaseStatus,
    pub created_at: DateTime<Utc>,
}

impl Release {
    pub fn new(name: String, flake_ref: String, job_id: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            flake_ref,
            flake_rev: None,
            job_id,
            job_spec_snapshot: None,
            version: 1,
            status: ReleaseStatus::Pending,
            created_at: Utc::now(),
        }
    }
}

/// Request to create a release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReleaseRequest {
    pub name: String,
    pub flake_ref: String,
    pub job_id: String,
    #[serde(default)]
    pub flake_rev: Option<String>,
}
