//! Manages the local nix configuration on behalf of the ro platform.
//!
//! The ro CLI writes a managed include file at `~/.config/nix/ro.conf`
//! which nix.conf includes via `!include`. This avoids touching the
//! user's main nix.conf and lets ro manage substituters, trusted keys,
//! and builder settings independently.
//!
//! The flow:
//!   1. ro fetches PlatformConfig from the API
//!   2. ro writes `~/.config/nix/ro.conf` with substituters + keys
//!   3. ro ensures `~/.config/nix/nix.conf` has `!include ro.conf`
//!   4. nix reads the include at eval time
//!
//! Cached config lives at `~/.config/ro/platform-config.json` so
//! the CLI can operate offline and detect changes on refresh.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::api::PlatformConfig;

/// Cached platform config with metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct CachedConfig {
    pub config: PlatformConfig,
    pub fetched_at: String,
    pub api_endpoint: String,
}

/// Where the cached config lives.
fn cache_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".config/ro/platform-config.json"))
}

/// Where the managed nix config snippet lives.
fn ro_conf_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".config/nix/ro.conf"))
}

/// Where the user's nix.conf lives.
fn nix_conf_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    Ok(home.join(".config/nix/nix.conf"))
}

/// Load the cached platform config, if any.
pub fn load_cached() -> Result<Option<CachedConfig>> {
    let path = cache_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .context("Failed to read cached config")?;
    let cached: CachedConfig = serde_json::from_str(&content)
        .context("Failed to parse cached config")?;
    Ok(Some(cached))
}

/// Save the platform config to the local cache.
pub fn save_cached(config: &PlatformConfig, api_endpoint: &str) -> Result<()> {
    let path = cache_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .context("Failed to create config directory")?;
    }

    let cached = CachedConfig {
        config: config.clone(),
        fetched_at: Utc::now().to_rfc3339(),
        api_endpoint: api_endpoint.to_string(),
    };

    let content = serde_json::to_string_pretty(&cached)
        .context("Failed to serialize config")?;
    fs::write(&path, content)
        .context("Failed to write cached config")?;

    Ok(())
}

/// Generate the content for `~/.config/nix/ro.conf`.
///
/// This is a nix.conf snippet that adds the ro platform's substituters
/// and trusted public keys to the nix configuration.
fn generate_ro_conf(config: &PlatformConfig) -> String {
    let mut lines = vec![
        "# Managed by ro (炉) — do not edit manually.".to_string(),
        format!("# Generated: {}", Utc::now().to_rfc3339()),
        format!("# Platform version: {}", config.version),
        String::new(),
    ];

    if !config.substituters.is_empty() {
        lines.push(format!(
            "extra-substituters = {}",
            config.substituters.join(" ")
        ));
    }

    if !config.trusted_public_keys.is_empty() {
        lines.push(format!(
            "extra-trusted-public-keys = {}",
            config.trusted_public_keys.join(" ")
        ));
    }

    // Builders would go here when tatara bare-metal nodes are registered.
    // e.g., extra-builders = ssh-ng://builder@node1 x86_64-linux ...

    lines.push(String::new());
    lines.join("\n")
}

/// Write the ro.conf snippet and ensure nix.conf includes it.
pub fn apply_nix_config(config: &PlatformConfig) -> Result<ConfigApplyResult> {
    let ro_conf = ro_conf_path()?;
    let nix_conf = nix_conf_path()?;

    // Ensure directories exist
    if let Some(parent) = ro_conf.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = generate_ro_conf(config);

    // Check if anything changed
    let changed = if ro_conf.exists() {
        let existing = fs::read_to_string(&ro_conf).unwrap_or_default();
        // Compare ignoring the "Generated:" timestamp line
        let strip_timestamp = |s: &str| -> String {
            s.lines()
                .filter(|l| !l.starts_with("# Generated:"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        strip_timestamp(&existing) != strip_timestamp(&content)
    } else {
        true
    };

    // Write ro.conf
    fs::write(&ro_conf, &content)
        .context("Failed to write ro.conf")?;

    // Ensure nix.conf includes ro.conf
    let include_added = ensure_nix_conf_include(&nix_conf, &ro_conf)?;

    Ok(ConfigApplyResult {
        ro_conf_path: ro_conf,
        config_changed: changed,
        include_added,
        substituters: config.substituters.clone(),
        trusted_keys: config.trusted_public_keys.clone(),
    })
}

/// Ensure `nix.conf` has `!include ro.conf`.
fn ensure_nix_conf_include(nix_conf: &Path, ro_conf: &Path) -> Result<bool> {
    let include_line = format!("!include {}", ro_conf.display());

    if nix_conf.exists() {
        let content = fs::read_to_string(nix_conf)?;
        if content.contains(&include_line) {
            return Ok(false);
        }
        // Append the include
        let new_content = format!("{}\n{}\n", content.trim_end(), include_line);
        fs::write(nix_conf, new_content)?;
    } else {
        // Create nix.conf with just the include
        if let Some(parent) = nix_conf.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(nix_conf, format!("{include_line}\n"))?;
    }

    Ok(true)
}

/// Result of applying nix configuration.
pub struct ConfigApplyResult {
    pub ro_conf_path: PathBuf,
    pub config_changed: bool,
    pub include_added: bool,
    pub substituters: Vec<String>,
    pub trusted_keys: Vec<String>,
}
