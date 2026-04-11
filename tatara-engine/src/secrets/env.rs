//! Environment variable secret provider — reads secrets from process env.
//! Used for testing and local development.

use super::{SecretFetcher, SecretValue};
use anyhow::Result;
use async_trait::async_trait;

pub struct EnvSecretFetcher;

#[async_trait]
impl SecretFetcher for EnvSecretFetcher {
    async fn fetch(&self, key: &str) -> Result<SecretValue> {
        let value = std::env::var(key)
            .map_err(|_| anyhow::anyhow!("environment variable '{}' not set", key))?;
        Ok(SecretValue {
            value,
            version: "env".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_env_fetch() {
        std::env::set_var("TATARA_TEST_SECRET", "hunter2");
        let fetcher = EnvSecretFetcher;
        let result = fetcher.fetch("TATARA_TEST_SECRET").await.unwrap();
        assert_eq!(result.value, "hunter2");
        std::env::remove_var("TATARA_TEST_SECRET");
    }

    #[tokio::test]
    async fn test_env_fetch_missing() {
        let fetcher = EnvSecretFetcher;
        assert!(fetcher.fetch("DEFINITELY_NOT_SET_XYZ_123").await.is_err());
    }
}
