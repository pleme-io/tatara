use std::sync::OnceLock;
use std::time::Duration;

use anyhow::Result;
use kazari::style::StyleAtom;
use kazari::{Capability, Print, Role, Stream, Table, Theme};

use crate::api::{BuildResponse, BuildStatus, CacheInfo, PlatformConfig, RoClient, SourceStatus};
use crate::nix_config::{CachedConfig, ConfigApplyResult};

fn caps(stream: Stream) -> Capability {
    static STDOUT: OnceLock<Capability> = OnceLock::new();
    static STDERR: OnceLock<Capability> = OnceLock::new();
    let cell = match stream {
        Stream::Stdout => &STDOUT,
        Stream::Stderr => &STDERR,
    };
    *cell.get_or_init(|| Capability::probe_stream(stream))
}

fn paint_on(stream: Stream, text: &str, role: Role, bold: bool, dim: bool) -> String {
    StyleAtom::resolve(role, Theme::default(), &caps(stream), bold, dim).paint(text)
}

fn paint(text: &str, role: Role, bold: bool, dim: bool) -> String {
    paint_on(Stream::Stdout, text, role, bold, dim)
}

fn ok(text: &str) -> String {
    paint(text, Role::Ok, true, false)
}

fn fail(text: &str) -> String {
    paint(text, Role::Error, true, false)
}

fn pending(text: &str) -> String {
    paint(text, Role::Pending, true, false)
}

fn ident(text: &str) -> String {
    paint(text, Role::Primary, false, false)
}

fn strong(text: &str) -> String {
    paint(text, Role::Text, true, false)
}

fn faint(text: &str) -> String {
    paint(text, Role::TextMuted, false, true)
}

pub fn print_build_submitted(resp: &BuildResponse) {
    println!(
        "{} Build submitted: {} ({})",
        ok("✓"),
        ident(&resp.build_id),
        resp.status
    );
    println!("  Track: {} status {}", strong("ro"), resp.build_id);
    println!("  Logs:  {} logs {} --follow", strong("ro"), resp.build_id);
}

pub fn print_build_status(status: &BuildStatus) {
    let phase_colored = match status.phase.as_str() {
        "Complete" => ok(&status.phase),
        "Failed" => fail(&status.phase),
        "Building" | "Pushing" => pending(&status.phase),
        _ => faint(&status.phase),
    };

    println!("Phase: {phase_colored}");

    if let Some(id) = &status.build_id {
        println!("Build ID: {}", ident(id));
    }
    if let Some(path) = &status.store_path {
        println!("Store path: {path}");
    }
    if let Some(node) = &status.builder_node {
        println!("Builder: {node}");
    }
    if let Some(started) = &status.started_at {
        println!("Started: {started}");
    }
    if let Some(completed) = &status.completed_at {
        println!("Completed: {completed}");
    }
    if let Some(err) = &status.error {
        println!("{}: {}", fail("Error"), err);
    }
}

pub async fn wait_for_build(client: &RoClient, build_id: &str) -> Result<()> {
    println!("{}", faint("Waiting for build to complete..."));

    loop {
        let status = client.get_build(build_id).await?;

        match status.phase.as_str() {
            "Complete" => {
                println!("\n{} Build complete!", ok("✓"));
                if let Some(path) = &status.store_path {
                    println!("Store path: {path}");
                }
                return Ok(());
            }
            "Failed" => {
                println!("\n{} Build failed", fail("✗"));
                if let Some(err) = &status.error {
                    println!("Error: {err}");
                }
                anyhow::bail!("Build failed");
            }
            phase => {
                eprint!(
                    "\r{}: {}  ",
                    paint_on(Stream::Stderr, "Phase", Role::TextMuted, false, true),
                    phase
                );
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
                println!("{data}");
            }
        }
    }

    Ok(())
}

pub fn print_sources(sources: &[SourceStatus]) -> std::io::Result<()> {
    let mut table = Table::new(["Name", "Repo", "Branch", "Commit", "Cached", "Total"]);
    for s in sources {
        table = table.row([
            s.name.clone(),
            s.repo.clone(),
            s.branch.clone(),
            s.last_commit.clone().unwrap_or_else(|| "-".to_string()),
            s.cached_outputs.to_string(),
            s.total_outputs.to_string(),
        ]);
    }
    table.print()
}

pub fn print_cache_info(info: &CacheInfo) {
    let size_mb = info.total_size_bytes / (1024 * 1024);
    println!("Cache: {} ({})", ident(&info.name), info.endpoint);
    println!("NARs:  {}", info.total_nars);
    println!("Size:  {size_mb} MB");
}

pub fn print_platform_config(config: &PlatformConfig) {
    println!("{}", strong("ro platform configuration"));
    println!("Version: {}", config.version);
    println!("\nSubstituters (add to nix.conf):");
    for s in &config.substituters {
        println!("  {s}");
    }
    println!("\nTrusted public keys:");
    for k in &config.trusted_public_keys {
        println!("  {k}");
    }
    println!("\nCache endpoint: {}", config.cache_endpoint);
}

pub fn print_health(healthy: bool) {
    if healthy {
        println!("{} ro platform is healthy", ok("✓"));
    } else {
        println!("{} ro platform is unreachable", fail("✗"));
    }
}

pub fn print_init_result(result: &ConfigApplyResult) {
    println!("{} ro initialized", ok("✓"));
    println!("  Config: {}", result.ro_conf_path.display());
    if result.include_added {
        println!("  Added !include to nix.conf");
    }
    println!("\nSubstituters configured:");
    for s in &result.substituters {
        println!("  {}", ident(s));
    }
    println!("\nTrusted public keys:");
    for k in &result.trusted_keys {
        println!("  {}", faint(k));
    }
    println!("\nNix is now configured to use the ro binary cache.");
    println!(
        "Run {} to refresh config from the platform.",
        strong("ro refresh")
    );
}

pub fn print_configure_result(result: &ConfigApplyResult) {
    if result.config_changed {
        println!("{} nix configuration updated", ok("✓"));
    } else {
        println!("{} nix configuration unchanged", faint("·"));
    }
    println!("  {}", result.ro_conf_path.display());
}

pub fn print_refresh_result(result: &ConfigApplyResult, old: Option<&CachedConfig>) {
    if result.config_changed {
        println!("{} config updated", ok("✓"));

        // Show what changed
        if let Some(old) = old {
            let old_subs: std::collections::HashSet<_> = old.config.substituters.iter().collect();
            let new_subs: std::collections::HashSet<_> = result.substituters.iter().collect();

            for s in &result.substituters {
                if !old_subs.contains(s) {
                    println!("  {} substituter {}", paint("+", Role::Ok, false, false), s);
                }
            }
            for s in &old.config.substituters {
                if !new_subs.contains(s) {
                    println!(
                        "  {} substituter {}",
                        paint("-", Role::Error, false, false),
                        s
                    );
                }
            }
        }
    } else {
        println!("{} config unchanged (already up to date)", faint("·"));
    }
}
