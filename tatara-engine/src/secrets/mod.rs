//! Secret provider abstraction — fetches secrets from external vaults
//! at allocation time. Values are never stored in Raft state.

pub mod env;
pub mod sops;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tatara_core::domain::secret::{SecretProvider, SecretRef};
use tracing::{debug, info};

/// A fetched secret value (zeroized on drop if the zeroize feature is enabled).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretValue {
    pub value: String,
    pub version: String,
}

/// Trait for secret provider backends.
#[async_trait]
pub trait SecretFetcher: Send + Sync {
    /// Fetch a secret by its provider-specific key.
    async fn fetch(&self, key: &str) -> Result<SecretValue>;

    /// Check if a secret has been rotated since the given version.
    async fn needs_rotation(&self, key: &str, current_version: &str) -> Result<bool> {
        let current = self.fetch(key).await?;
        Ok(current.version != current_version)
    }
}

/// Resolve a set of secret refs into env vars and file mounts.
pub struct SecretResolver {
    env_fetcher: env::EnvSecretFetcher,
    sops_fetcher: sops::SopsSecretFetcher,
}

impl SecretResolver {
    pub fn new() -> Self {
        Self {
            env_fetcher: env::EnvSecretFetcher,
            sops_fetcher: sops::SopsSecretFetcher,
        }
    }

    /// Resolve all secret refs, returning env vars to inject and files to write.
    pub async fn resolve(&self, refs: &[SecretRef]) -> Result<ResolvedSecrets> {
        let mut env_vars = std::collections::HashMap::new();
        let mut files = Vec::new();

        for secret_ref in refs {
            let fetcher: &dyn SecretFetcher = match secret_ref.provider {
                SecretProvider::Env => &self.env_fetcher,
                SecretProvider::Sops => &self.sops_fetcher,
                SecretProvider::Akeyless => {
                    // Akeyless provider would be wired here when available
                    anyhow::bail!("Akeyless provider not yet configured");
                }
            };

            let value = fetcher.fetch(&secret_ref.key).await?;
            debug!(name = %secret_ref.name, provider = ?secret_ref.provider, "resolved secret");

            if let Some(env_var) = &secret_ref.env_var {
                env_vars.insert(env_var.clone(), value.value.clone());
            }

            if let Some(mount_path) = &secret_ref.mount_path {
                files.push(SecretFile {
                    path: mount_path.clone(),
                    content: value.value.clone(),
                });
            }
        }

        info!(
            env_count = env_vars.len(),
            file_count = files.len(),
            "secrets resolved"
        );
        Ok(ResolvedSecrets { env_vars, files })
    }
}

impl Default for SecretResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolved secrets ready for injection into a task.
#[derive(Debug, Default)]
pub struct ResolvedSecrets {
    /// Environment variables to inject.
    pub env_vars: std::collections::HashMap<String, String>,
    /// Files to write into the allocation directory.
    pub files: Vec<SecretFile>,
}

/// A secret file to write.
#[derive(Debug)]
pub struct SecretFile {
    pub path: String,
    pub content: String,
}
