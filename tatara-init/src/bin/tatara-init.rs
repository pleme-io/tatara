//! tatara-init binary — the PID 1 entry point.
//!
//! Usage:
//!   tatara-init [path/to/init.lisp]
//!
//! Default config path: `$TATARA_INIT_CONFIG` → `/etc/tatara/init.lisp`.
//!
//! This is a reference-implementation loop. The Linux boot path runs this as
//! PID 1 via `init=/bin/tatara-init` on the kernel cmdline. Outside a VM the
//! binary still runs — it just doesn't get the PID-1 privileges.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use tatara_init::{InitConfig, LinuxSupervisor, Pid};
use tatara_lisp::{domain::TataraDomain, read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = resolve_config_path();
    let source = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    let forms = read(&source)?;
    let form = forms
        .into_iter()
        .next()
        .ok_or("config file contains no forms")?;
    let cfg = InitConfig::compile_from_sexp(&form)?;

    eprintln!(
        "[tatara-init] booting '{}' with {} service(s)",
        cfg.name,
        cfg.services.len()
    );

    let mut sup = LinuxSupervisor::new();
    let by_name = tatara_init::supervisor::boot(&mut sup, &cfg)?;
    let mut tracking: HashMap<Pid, String> =
        by_name.into_iter().map(|(n, p)| (p, n)).collect();

    // Main loop: poll reap + honor signals. A production init would use
    // signalfd/kqueue; polling is portable and sufficient for v0.
    loop {
        let events =
            tatara_init::supervisor::run_once(&mut sup, &cfg, &mut tracking)?;
        for ev in events {
            eprintln!(
                "[tatara-init] reaped {} (pid={}) status={}{}",
                ev.name,
                ev.pid,
                ev.exit_status,
                match ev.restarted_as {
                    Some(p) => format!(" → restarted as {p}"),
                    None => "".into(),
                }
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn resolve_config_path() -> PathBuf {
    if let Some(arg) = std::env::args().nth(1) {
        return PathBuf::from(arg);
    }
    if let Ok(env) = std::env::var("TATARA_INIT_CONFIG") {
        return PathBuf::from(env);
    }
    PathBuf::from("/etc/tatara/init.lisp")
}
