//! Secret reference types for tatara workloads.
//!
//! Secrets are fetched at allocation time from external providers.
//! Only references are stored in Raft state — never secret values.

use serde::{Deserialize, Serialize};

/// Secret provider backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SecretProvider {
    /// Akeyless vault.
    Akeyless,
    /// SOPS-encrypted file.
    Sops,
    /// Environment variable (for testing).
    Env,
}

/// A reference to a secret in a job spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretRef {
    /// Logical name in the job spec.
    pub name: String,

    /// Which provider to fetch from.
    pub provider: SecretProvider,

    /// Provider-specific key path (e.g., "/pleme/prod/db-password").
    pub key: String,

    /// If set, inject as this environment variable.
    #[serde(default)]
    pub env_var: Option<String>,

    /// If set, write to this file path inside the allocation directory.
    #[serde(default)]
    pub mount_path: Option<String>,
}
