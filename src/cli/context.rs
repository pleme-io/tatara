use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::output::{build_table, status_cell, OutputFormat, render_value};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextConfig {
    #[serde(default)]
    pub contexts: HashMap<String, ContextEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub endpoint: String,
    #[serde(default)]
    pub default: bool,
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("tatara")
        .join("contexts.toml")
}

pub fn load_config() -> Result<ContextConfig> {
    let path = config_path();
    if !path.exists() {
        return Ok(ContextConfig::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let config: ContextConfig =
        toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(config)
}

fn save_config(config: &ContextConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// Get the active endpoint from context config, or fall back to the default.
pub fn active_endpoint(endpoint_override: Option<&str>) -> String {
    if let Some(ep) = endpoint_override {
        return ep.to_string();
    }
    if let Ok(config) = load_config() {
        // Find the context marked as default
        for (_name, entry) in &config.contexts {
            if entry.default {
                return entry.endpoint.clone();
            }
        }
        // If only one context exists, use it
        if config.contexts.len() == 1 {
            return config
                .contexts
                .values()
                .next()
                .unwrap()
                .endpoint
                .clone();
        }
    }
    "http://127.0.0.1:4646".to_string()
}

/// Extract just host:port from an endpoint URL for backwards compat with old --server flag.
pub fn endpoint_to_server(endpoint: &str) -> String {
    endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint)
        .to_string()
}

pub async fn list(output: OutputFormat) -> Result<()> {
    let config = load_config()?;

    if config.contexts.is_empty() {
        println!("No contexts configured.");
        println!("Add contexts to {}", config_path().display());
        return Ok(());
    }

    match output {
        OutputFormat::Json | OutputFormat::Yaml => {
            println!("{}", render_value(&config.contexts, output)?);
        }
        _ => {
            let mut table = build_table(&["", "NAME", "ENDPOINT"]);
            let mut names: Vec<&String> = config.contexts.keys().collect();
            names.sort();
            for name in names {
                let entry = &config.contexts[name];
                let marker = if entry.default { "*" } else { "" };
                table.add_row(vec![
                    status_cell(if entry.default { "active" } else { "" }),
                    comfy_table::Cell::new(format!("{}{}", marker, name)),
                    comfy_table::Cell::new(&entry.endpoint),
                ]);
            }
            println!("{table}");
        }
    }
    Ok(())
}

pub async fn use_context(name: &str) -> Result<()> {
    let mut config = load_config()?;

    if !config.contexts.contains_key(name) {
        anyhow::bail!(
            "Context '{}' not found. Available: {}",
            name,
            config
                .contexts
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Clear all defaults, set the requested one
    for entry in config.contexts.values_mut() {
        entry.default = false;
    }
    config.contexts.get_mut(name).unwrap().default = true;

    save_config(&config)?;

    let endpoint = &config.contexts[name].endpoint;
    println!("Switched to context '{}' ({})", name, endpoint);
    Ok(())
}

pub async fn current() -> Result<()> {
    let config = load_config()?;
    for (name, entry) in &config.contexts {
        if entry.default {
            println!("{} ({})", name, entry.endpoint);
            return Ok(());
        }
    }
    println!("No active context. Using default: http://127.0.0.1:4646");
    Ok(())
}
