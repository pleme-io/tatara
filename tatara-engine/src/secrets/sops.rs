//! SOPS secret provider — decrypts secrets from SOPS-encrypted files.
//! Used for local development with encrypted secret files.

use super::{SecretFetcher, SecretValue};
use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::process::Command;

pub struct SopsSecretFetcher;

#[async_trait]
impl SecretFetcher for SopsSecretFetcher {
    /// Fetch a secret from a SOPS-encrypted file.
    ///
    /// Key format: `path/to/file.yaml#key.subkey`
    /// Runs: `sops --decrypt --extract '["key"]["subkey"]' path/to/file.yaml`
    async fn fetch(&self, key: &str) -> Result<SecretValue> {
        let (file_path, json_path) = key
            .split_once('#')
            .context("SOPS key must be 'file_path#json.path'")?;

        // Convert dot path to SOPS extract format: key.subkey -> ["key"]["subkey"]
        let extract_path = json_path
            .split('.')
            .map(|part| format!("[\"{part}\"]"))
            .collect::<String>();

        let output = Command::new("sops")
            .args(["--decrypt", "--extract", &extract_path, file_path])
            .output()
            .await
            .context("failed to run sops")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("sops decrypt failed: {stderr}");
        }

        let value = String::from_utf8(output.stdout)
            .context("sops output not valid UTF-8")?
            .trim()
            .to_string();

        Ok(SecretValue {
            value,
            version: "sops".to_string(),
        })
    }
}
