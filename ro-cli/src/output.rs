use std::time::Duration;

use anyhow::Result;
use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Table};

use crate::api::{BuildResponse, BuildStatus, CacheInfo, PlatformConfig, RoClient, SourceStatus};
use crate::nix_config::{CachedConfig, ConfigApplyResult};

pub fn print_build_submitted(resp: &BuildResponse) {
    println!(
        "{} Build submitted: {} ({})",
        "✓".green().bold(),
        resp.build_id.cyan(),
        resp.status
    );
    println!("  Track: {} status {}", "ro".bold(), resp.build_id);
    println!("  Logs:  {} logs {} --follow", "ro".bold(), resp.build_id);
}

pub fn print_build_status(status: &BuildStatus) {
    let phase_colored = match status.phase.as_str() {
        "Complete" => status.phase.green().bold(),
        "Failed" => status.phase.red().bold(),
        "Building" | "Pushing" => status.phase.yellow().bold(),
        _ => status.phase.dimmed(),
    };

    println!("Phase: {}", phase_colored);

    if let Some(id) = &status.build_id {
        println!("Build ID: {}", id.cyan());
    }
    if let Some(path) = &status.store_path {
        println!("Store path: {}", path);
    }
    if let Some(node) = &status.builder_node {
        println!("Builder: {}", node);
    }
    if let Some(started) = &status.started_at {
        println!("Started: {}", started);
    }
    if let Some(completed) = &status.completed_at {
        println!("Completed: {}", completed);
    }
    if let Some(err) = &status.error {
        println!("{}: {}", "Error".red().bold(), err);
    }
}

pub async fn wait_for_build(client: &RoClient, build_id: &str) -> Result<()> {
    println!("{}", "Waiting for build to complete...".dimmed());

    loop {
        let status = client.get_build(build_id).await?;

        match status.phase.as_str() {
            "Complete" => {
                println!(
                    "\n{} Build complete!",
                    "✓".green().bold()
                );
                if let Some(path) = &status.store_path {
                    println!("Store path: {}", path);
                }
                return Ok(());
            }
            "Failed" => {
                println!(
                    "\n{} Build failed",
                    "✗".red().bold()
                );
                if let Some(err) = &status.error {
                    println!("Error: {}", err);
                }
                anyhow::bail!("Build failed");
            }
            phase => {
                eprint!("\r{}: {}  ", "Phase".dimmed(), phase);
            }
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

pub async fn stream_sse_logs(resp: reqwest::Response, _follow: bool) -> Result<()> {
    use futures::StreamExt;
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        let text = String::from_utf8_lossy(&bytes);
        // Parse SSE format: "data: <message>\n\n"
        for line in text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                println!("{}", data);
            }
        }
    }

    Ok(())
}

pub fn print_sources(sources: &[SourceStatus]) {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(["Name", "Repo", "Branch", "Commit", "Cached", "Total"]);

    for s in sources {
        table.add_row([
            &s.name,
            &s.repo,
            &s.branch,
            s.last_commit.as_deref().unwrap_or("-"),
            &s.cached_outputs.to_string(),
            &s.total_outputs.to_string(),
        ]);
    }

    println!("{table}");
}

pub fn print_cache_info(info: &CacheInfo) {
    let size_mb = info.total_size_bytes / (1024 * 1024);
    println!("Cache: {} ({})", info.name.cyan(), info.endpoint);
    println!("NARs:  {}", info.total_nars);
    println!("Size:  {} MB", size_mb);
}

pub fn print_platform_config(config: &PlatformConfig) {
    println!("{}", "ro platform configuration".bold());
    println!("Version: {}", config.version);
    println!("\nSubstituters (add to nix.conf):");
    for s in &config.substituters {
        println!("  {}", s);
    }
    println!("\nTrusted public keys:");
    for k in &config.trusted_public_keys {
        println!("  {}", k);
    }
    println!("\nCache endpoint: {}", config.cache_endpoint);
}

pub fn print_health(healthy: bool) {
    if healthy {
        println!("{} ro platform is healthy", "✓".green().bold());
    } else {
        println!("{} ro platform is unreachable", "✗".red().bold());
    }
}

pub fn print_init_result(result: &ConfigApplyResult) {
    println!("{} ro initialized", "✓".green().bold());
    println!("  Config: {}", result.ro_conf_path.display());
    if result.include_added {
        println!("  Added !include to nix.conf");
    }
    println!("\nSubstituters configured:");
    for s in &result.substituters {
        println!("  {}", s.cyan());
    }
    println!("\nTrusted public keys:");
    for k in &result.trusted_keys {
        println!("  {}", k.dimmed());
    }
    println!("\nNix is now configured to use the ro binary cache.");
    println!("Run {} to refresh config from the platform.", "ro refresh".bold());
}

pub fn print_configure_result(result: &ConfigApplyResult) {
    if result.config_changed {
        println!("{} nix configuration updated", "✓".green().bold());
    } else {
        println!("{} nix configuration unchanged", "·".dimmed());
    }
    println!("  {}", result.ro_conf_path.display());
}

pub fn print_refresh_result(result: &ConfigApplyResult, old: Option<&CachedConfig>) {
    if result.config_changed {
        println!("{} config updated", "✓".green().bold());

        // Show what changed
        if let Some(old) = old {
            let old_subs: std::collections::HashSet<_> = old.config.substituters.iter().collect();
            let new_subs: std::collections::HashSet<_> = result.substituters.iter().collect();

            for s in &result.substituters {
                if !old_subs.contains(s) {
                    println!("  {} substituter {}", "+".green(), s);
                }
            }
            for s in &old.config.substituters {
                if !new_subs.contains(s) {
                    println!("  {} substituter {}", "-".red(), s);
                }
            }
        }
    } else {
        println!("{} config unchanged (already up to date)", "·".dimmed());
    }
}
