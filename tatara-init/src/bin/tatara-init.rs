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

use tatara_eval::Interpreter;
use tatara_init::{InitConfig, LinuxSupervisor, Pid};
use tatara_lisp::{domain::TataraDomain, read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --eval <form>  — evaluate a single Lisp form via tatara-eval and
    // exit. Used by services whose :body was declared in Lisp instead
    // of as a shell exec. The supervisor rewrites such services to
    // `/bin/tatara-init --eval '<form>'` in its fork+exec path.
    let args: Vec<String> = std::env::args().collect();
    if let Some(idx) = args.iter().position(|a| a == "--eval") {
        let form = args.get(idx + 1).ok_or("--eval needs a Lisp form")?;
        // Host-side: system builtins (println/sleep/exit/shell/…) on.
        // This path is how `:body` service definitions actually run.
        let interp = Interpreter::new_with_system();
        let value = interp.eval_source(form)?;
        // Services don't usually have anyone to read stdout; still print
        // the final value so `tatara-vmctl logs` shows what happened.
        eprintln!("[tatara-init:eval] {value}");
        return Ok(());
    }
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

    // Essential filesystem mounts (procfs, sysfs, devtmpfs, tmpfs).
    // Failures are logged but don't abort — a running guest may already
    // have something on the mount point, or the kernel may lack a given
    // filesystem type. Either is recoverable; missing /proc is not, but
    // we'd see that via downstream service failures.
    for result in tatara_init::mount_early_filesystems() {
        match result {
            Ok(m) => eprintln!("[tatara-init] mounted {} ({})", m.target, m.fstype),
            Err(e) => eprintln!("[tatara-init] mount warn: {e}"),
        }
    }

    let mut sup = LinuxSupervisor::new();
    let by_name = tatara_init::supervisor::boot(&mut sup, &cfg)?;
    let mut tracking: HashMap<Pid, String> = by_name.into_iter().map(|(n, p)| (p, n)).collect();

    // Main loop: poll reap + honor signals. A production init would use
    // signalfd/kqueue; polling is portable and sufficient for v0.
    loop {
        let events = tatara_init::supervisor::run_once(&mut sup, &cfg, &mut tracking)?;
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
