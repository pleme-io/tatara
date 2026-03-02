use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// A git-hosted Nix flake (e.g., "github:pleme-io/tatara-infra")
    GitFlake,
    /// A direct flake output reference (e.g., "path:/nix/store/...")
    FlakeOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Pending,
    Ready,
    Failed,
    Suspended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: Uuid,
    pub name: String,
    pub kind: SourceKind,
    /// Flake reference, e.g., "github:pleme-io/tatara-infra"
    pub flake_ref: String,
    pub status: SourceStatus,
    /// Last observed flake revision (git commit hash).
    pub last_rev: Option<String>,
    pub last_reconciled_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    /// Map of job_name -> spec content hash for managed jobs.
    #[serde(default)]
    pub managed_jobs: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
}

impl Source {
    pub fn new(name: String, kind: SourceKind, flake_ref: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            kind,
            flake_ref,
            status: SourceStatus::Pending,
            last_rev: None,
            last_reconciled_at: None,
            last_error: None,
            managed_jobs: HashMap::new(),
            created_at: Utc::now(),
        }
    }
}

/// Request to create a source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSourceRequest {
    pub name: String,
    pub flake_ref: String,
    #[serde(default = "default_kind")]
    pub kind: SourceKind,
}

fn default_kind() -> SourceKind {
    SourceKind::GitFlake
}

/// Metadata returned by `nix flake metadata`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeMetadata {
    /// Git commit hash (None for path flakes).
    pub rev: Option<String>,
    /// Last modified timestamp (unix epoch).
    pub last_modified: u64,
    /// Resolved URL.
    pub url: String,
}

/// Structured errors for source reconciliation.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// Failed to fetch flake metadata (network, auth, invalid ref).
    #[error("metadata fetch failed for '{flake_ref}': {reason}")]
    MetadataFetchFailed {
        flake_ref: String,
        reason: String,
    },

    /// Failed to evaluate tataraJobs from the flake.
    #[error("eval failed for '{flake_ref}': {reason}")]
    EvalFailed {
        flake_ref: String,
        reason: String,
    },

    /// Source validation failed (missing outputs, bad structure).
    #[error("validation failed for source '{name}': {errors:?}")]
    ValidationFailed {
        name: String,
        errors: Vec<String>,
    },

    /// Failed to apply a job change (create/update/delete).
    #[error("job operation failed for '{job_name}' in source '{source_name}': {reason}")]
    JobOperationFailed {
        source_name: String,
        job_name: String,
        reason: String,
    },

    /// Timeout during a nix operation.
    #[error("operation timed out for '{flake_ref}' after {timeout_secs}s")]
    Timeout {
        flake_ref: String,
        timeout_secs: u64,
    },
}
