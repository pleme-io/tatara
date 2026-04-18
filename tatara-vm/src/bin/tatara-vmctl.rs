//! tatara-vmctl — control plane for tatara-os guests.
//!
//! Works against VMs declared in `$TATARA_OS_ROOT` (defaults to
//! `$HOME/.local/share/tatara-os`). Each subdirectory is a VM; the
//! `system.json` + `vm.json` files (emitted by `tatara-boot-gen`) make it
//! discoverable. Per-VM state (pid, started_at, last known guest IP) lives
//! at `<vm>/state.json`; vfkit stdout/stderr lands in `<vm>/console.log`.
//!
//! Commands:
//!
//!   list                      — every declared VM + whether it's running
//!   status <name>             — full state for one VM (pid/IP/uptime/artifacts)
//!   build  <name>             — realize kernel.nix + initrd.nix into /nix/store
//!   up     <name>             — build + splice vm.json + launch vfkit detached
//!   down   <name>             — SIGTERM the vfkit PID, wait, SIGKILL fallback
//!   destroy <name>            — down + remove the VM directory
//!   logs   [--follow] <name>  — cat/tail `<vm>/console.log`
//!   ssh    <name> [-- cmd]    — ssh into guest using the last-known IP
//!   ip     <name>             — print the guest IP, discovering via arp if needed
//!   help                      — this message
//!
//! Nord-themed stderr via tatara-ui (disabled by NO_COLOR).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tatara_ui::{
    ArtifactState, Cell, EventStream, LogLevel, Renderer, Role, ShortHash, ThemeSpec, UiEvent,
};

// ── discovery + state ───────────────────────────────────────────────────

fn tatara_os_root() -> PathBuf {
    if let Ok(r) = std::env::var("TATARA_OS_ROOT") {
        return PathBuf::from(r);
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".local/share/tatara-os");
    }
    PathBuf::from("/tmp/tatara-os")
}

/// Per-VM persistent state. Read/updated by `up`/`down`, queried by everything.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct VmState {
    pid: Option<i32>,
    started_at: Option<String>, // ISO-8601
    guest_ip: Option<String>,
    guest_mac: Option<String>,
    boot_hash: Option<String>,
}

impl VmState {
    fn path(dir: &Path) -> PathBuf {
        dir.join("state.json")
    }
    fn load(dir: &Path) -> Self {
        std::fs::read_to_string(Self::path(dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    fn save(&self, dir: &Path) -> std::io::Result<()> {
        let p = Self::path(dir);
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(&p, serde_json::to_string_pretty(self).unwrap_or_default())
    }
}

/// Shape of `vm.json` — we only peek at device list to recover the guest MAC.
#[derive(Debug, Deserialize)]
struct VmJsonPeek {
    #[serde(default)]
    devices: Vec<serde_json::Value>,
}

fn discover_vms() -> Vec<(String, PathBuf)> {
    let root = tatara_os_root();
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return out;
    };
    for e in entries.flatten() {
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        if !dir.join("system.json").exists() {
            continue;
        }
        if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
            out.push((name.to_string(), dir));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn vm_dir(name: &str) -> Result<PathBuf, String> {
    let d = tatara_os_root().join(name);
    if !d.join("system.json").exists() {
        return Err(format!("no such VM: {name} (expected {}/system.json)", d.display()));
    }
    Ok(d)
}

fn pid_alive(pid: i32) -> bool {
    #[cfg(unix)]
    {
        // kill(pid, 0) returns 0 iff the process exists and we can signal it.
        // ESRCH means no such process; EPERM means it exists but different user.
        let r = unsafe { libc::kill(pid, 0) };
        if r == 0 {
            return true;
        }
        let err = std::io::Error::last_os_error();
        err.raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn send_signal(pid: i32, sig: i32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        if unsafe { libc::kill(pid, sig) } == 0 {
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::ESRCH) {
            return Ok(());
        }
        Err(e)
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, sig);
        Ok(())
    }
}

// ── IP discovery ────────────────────────────────────────────────────────

fn guest_mac_from_vm_json(dir: &Path) -> Option<String> {
    let path = dir.join("vm.json");
    let raw = std::fs::read_to_string(&path).ok()?;
    let v: VmJsonPeek = serde_json::from_str(&raw).ok()?;
    for d in &v.devices {
        if d.get("device").and_then(|x| x.as_str()) == Some("virtio-net") {
            if let Some(m) = d.get("mac-address").and_then(|x| x.as_str()) {
                return Some(m.to_string());
            }
        }
    }
    None
}

/// Query `arp -a`, return the IP bound to `mac` (case-insensitive). Skips
/// entries flagged `(incomplete)`. Returns `None` if absent.
fn arp_lookup_ip(mac: &str) -> Option<String> {
    let out = Command::new("arp").arg("-a").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let needle = mac.to_lowercase();
    for line in text.lines() {
        if !line.to_lowercase().contains(&needle) {
            continue;
        }
        if line.contains("incomplete") {
            continue;
        }
        // Lines look like: `? (192.168.64.5) at aa:bb:cc:dd:ee:ff on bridge100 ifscope [bridge]`
        if let Some(open) = line.find('(') {
            if let Some(close) = line[open..].find(')') {
                return Some(line[open + 1..open + close].to_string());
            }
        }
    }
    None
}

fn resolve_guest_ip(dir: &Path, st: &mut VmState) -> Option<String> {
    if let Some(ip) = &st.guest_ip {
        return Some(ip.clone());
    }
    let mac = st
        .guest_mac
        .clone()
        .or_else(|| guest_mac_from_vm_json(dir))?;
    st.guest_mac = Some(mac.clone());
    let ip = arp_lookup_ip(&mac)?;
    st.guest_ip = Some(ip.clone());
    let _ = st.save(dir);
    Some(ip)
}

// ── UX helpers ──────────────────────────────────────────────────────────

struct Ctx {
    renderer: Renderer,
    stream: EventStream,
}

impl Ctx {
    fn new() -> Self {
        let theme = ThemeSpec::nord_arctic();
        Self {
            renderer: Renderer::new(theme.to_role_map()),
            stream: EventStream::new(),
        }
    }
    fn banner(&mut self, title: &str) {
        let e = UiEvent::Banner {
            title: title.into(),
            subtitle: None,
        };
        let _ = self.renderer.render_one(&e, &mut std::io::stderr());
        self.stream.push(e);
    }
    fn section(&mut self, title: &str) {
        let e = UiEvent::Section { title: title.into() };
        let _ = self.renderer.render_one(&e, &mut std::io::stderr());
        self.stream.push(e);
    }
    fn info(&mut self, msg: impl Into<String>) {
        let e = UiEvent::Log {
            level: LogLevel::Info,
            message: msg.into(),
        };
        let _ = self.renderer.render_one(&e, &mut std::io::stderr());
        self.stream.push(e);
    }
    fn success(&mut self, msg: impl Into<String>) {
        let e = UiEvent::Log {
            level: LogLevel::Success,
            message: msg.into(),
        };
        let _ = self.renderer.render_one(&e, &mut std::io::stderr());
        self.stream.push(e);
    }
    fn warn(&mut self, msg: impl Into<String>) {
        let e = UiEvent::Log {
            level: LogLevel::Warn,
            message: msg.into(),
        };
        let _ = self.renderer.render_one(&e, &mut std::io::stderr());
        self.stream.push(e);
    }
    fn error(&mut self, msg: impl Into<String>) {
        let e = UiEvent::Log {
            level: LogLevel::Error,
            message: msg.into(),
        };
        let _ = self.renderer.render_one(&e, &mut std::io::stderr());
        self.stream.push(e);
    }
    fn row(&mut self, cells: Vec<Cell>) {
        let e = UiEvent::Row { cells };
        let _ = self.renderer.render_one(&e, &mut std::io::stderr());
        self.stream.push(e);
    }
}

// ── commands ────────────────────────────────────────────────────────────

fn cmd_list(ctx: &mut Ctx) -> ExitCode {
    ctx.banner("tatara-vmctl · list");
    let vms = discover_vms();
    if vms.is_empty() {
        ctx.info(format!(
            "no VMs declared under {}",
            tatara_os_root().display()
        ));
        return ExitCode::SUCCESS;
    }
    for (name, dir) in vms {
        let st = VmState::load(&dir);
        let alive = st.pid.map(pid_alive).unwrap_or(false);
        let state = if alive { "running" } else { "stopped" };
        let role = if alive { Role::Success } else { Role::Dim };
        ctx.row(vec![
            Cell::with_role(name, Role::Primary),
            Cell::with_role(state, role),
            Cell::with_role(
                st.pid
                    .map(|p| format!("pid:{p}"))
                    .unwrap_or_else(|| "-".into()),
                Role::Dim,
            ),
            Cell::with_role(st.guest_ip.unwrap_or_else(|| "-".into()), Role::Info),
        ]);
    }
    ExitCode::SUCCESS
}

fn cmd_status(ctx: &mut Ctx, name: &str) -> ExitCode {
    ctx.banner("tatara-vmctl · status");
    let Ok(dir) = vm_dir(name) else {
        ctx.error(format!("no such VM: {name}"));
        return ExitCode::from(2);
    };
    let mut st = VmState::load(&dir);
    let alive = st.pid.map(pid_alive).unwrap_or(false);
    ctx.info(format!("root: {}", dir.display()));
    ctx.info(format!(
        "state: {}  pid: {}  started: {}",
        if alive { "running" } else { "stopped" },
        st.pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "-".into()),
        st.started_at.clone().unwrap_or_else(|| "-".into()),
    ));
    // Artifact presence.
    for f in ["system.json", "vm.json", "initrd.nix", "kernel.nix", "launch.sh"] {
        let present = dir.join(f).exists();
        let sigil = if present { "✓" } else { "✗" };
        let role = if present { Role::Success } else { Role::Warn };
        ctx.row(vec![
            Cell::with_role(sigil, role),
            Cell::with_role(f, Role::Primary),
        ]);
    }
    // Best-effort IP discovery.
    if alive {
        if let Some(ip) = resolve_guest_ip(&dir, &mut st) {
            ctx.success(format!("guest IP: {ip}"));
        } else {
            ctx.warn("guest IP: not yet visible via arp");
        }
    }
    ExitCode::SUCCESS
}

fn cmd_build(ctx: &mut Ctx, name: &str) -> ExitCode {
    ctx.banner("tatara-vmctl · build");
    let Ok(dir) = vm_dir(name) else {
        ctx.error(format!("no such VM: {name}"));
        return ExitCode::from(2);
    };
    for f in ["kernel.nix", "initrd.nix"] {
        let path = dir.join(f);
        if !path.exists() {
            ctx.warn(format!("{f}: missing, skipping"));
            continue;
        }
        ctx.section(f);
        let out = Command::new("nix")
            .args([
                "build",
                "-f",
                path.to_str().unwrap(),
                "--no-link",
                "--print-out-paths",
            ])
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let store = String::from_utf8_lossy(&o.stdout).trim().to_string();
                ctx.renderer
                    .render_one(
                        &UiEvent::Artifact {
                            name: store.clone(),
                            hash: ShortHash::from_blake3_hex(
                                store.trim_start_matches("/nix/store/"),
                            ),
                            state: ArtifactState::Built { elapsed_ms: 0 },
                        },
                        &mut std::io::stderr(),
                    )
                    .ok();
            }
            Ok(o) => {
                ctx.error(format!(
                    "nix build failed for {f}: {}",
                    String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or("")
                ));
                return ExitCode::FAILURE;
            }
            Err(e) => {
                ctx.error(format!("nix build errored for {f}: {e}"));
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

fn cmd_up(ctx: &mut Ctx, name: &str) -> ExitCode {
    ctx.banner("tatara-vmctl · up");
    let Ok(dir) = vm_dir(name) else {
        ctx.error(format!("no such VM: {name}"));
        return ExitCode::from(2);
    };
    let mut st = VmState::load(&dir);
    if let Some(pid) = st.pid {
        if pid_alive(pid) {
            ctx.warn(format!("already running (pid {pid})"));
            return ExitCode::SUCCESS;
        }
    }
    // Delegate the actual nix-build + jq splice + vfkit exec to launch.sh.
    let launcher = dir.join("launch.sh");
    if !launcher.exists() {
        ctx.error("launch.sh missing (re-run home-manager activation?)");
        return ExitCode::FAILURE;
    }
    let log = dir.join("console.log");
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log);
    let log_file = match log_file {
        Ok(f) => f,
        Err(e) => {
            ctx.error(format!("opening console.log: {e}"));
            return ExitCode::FAILURE;
        }
    };
    // Spawn detached: separate session, redirect stdio to the log.
    #[cfg(unix)]
    use std::os::unix::process::CommandExt;
    let mut cmd = Command::new(&launcher);
    cmd.stdout(Stdio::from(log_file.try_clone().unwrap_or_else(|_| log_file.try_clone().unwrap())))
        .stderr(Stdio::from(log_file))
        .stdin(Stdio::null())
        .current_dir(&dir);
    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            // Detach from parent terminal so the process survives us.
            libc::setsid();
            Ok(())
        });
    }
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            ctx.error(format!("spawn failed: {e}"));
            return ExitCode::FAILURE;
        }
    };
    let pid = child.id() as i32;
    st.pid = Some(pid);
    st.started_at = Some(iso_now());
    let _ = st.save(&dir);
    ctx.success(format!("launched vfkit pid {pid}; console → {}", log.display()));
    // Don't wait on child — we want it backgrounded.
    std::mem::forget(child);
    ExitCode::SUCCESS
}

fn cmd_down(ctx: &mut Ctx, name: &str) -> ExitCode {
    ctx.banner("tatara-vmctl · down");
    let Ok(dir) = vm_dir(name) else {
        ctx.error(format!("no such VM: {name}"));
        return ExitCode::from(2);
    };
    let mut st = VmState::load(&dir);
    let Some(pid) = st.pid else {
        ctx.warn("no recorded pid — nothing to stop");
        return ExitCode::SUCCESS;
    };
    if !pid_alive(pid) {
        ctx.warn(format!("pid {pid} already dead; clearing state"));
        st.pid = None;
        let _ = st.save(&dir);
        return ExitCode::SUCCESS;
    }
    ctx.info(format!("SIGTERM → {pid}"));
    let _ = send_signal(pid, 15);
    // Wait up to 15 s for graceful exit.
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(500));
        if !pid_alive(pid) {
            break;
        }
    }
    if pid_alive(pid) {
        ctx.warn(format!("SIGKILL → {pid} (still alive after 15s)"));
        let _ = send_signal(pid, 9);
    }
    st.pid = None;
    let _ = st.save(&dir);
    ctx.success("stopped");
    ExitCode::SUCCESS
}

fn cmd_destroy(ctx: &mut Ctx, name: &str) -> ExitCode {
    ctx.banner("tatara-vmctl · destroy");
    if cmd_down(ctx, name) != ExitCode::SUCCESS {
        return ExitCode::FAILURE;
    }
    let Ok(dir) = vm_dir(name) else {
        return ExitCode::from(2);
    };
    match std::fs::remove_dir_all(&dir) {
        Ok(_) => {
            ctx.success(format!("removed {}", dir.display()));
            ExitCode::SUCCESS
        }
        Err(e) => {
            ctx.error(format!("remove failed: {e}"));
            ExitCode::FAILURE
        }
    }
}

fn cmd_logs(ctx: &mut Ctx, name: &str, follow: bool) -> ExitCode {
    let Ok(dir) = vm_dir(name) else {
        ctx.error(format!("no such VM: {name}"));
        return ExitCode::from(2);
    };
    let log = dir.join("console.log");
    if !log.exists() {
        ctx.warn(format!("no console log yet ({})", log.display()));
        return ExitCode::SUCCESS;
    }
    let mut cmd = Command::new("tail");
    cmd.arg("-n").arg("200");
    if follow {
        cmd.arg("-f");
    }
    cmd.arg(&log);
    match cmd.status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

fn cmd_ip(ctx: &mut Ctx, name: &str) -> ExitCode {
    let Ok(dir) = vm_dir(name) else {
        ctx.error(format!("no such VM: {name}"));
        return ExitCode::from(2);
    };
    let mut st = VmState::load(&dir);
    match resolve_guest_ip(&dir, &mut st) {
        Some(ip) => {
            println!("{ip}");
            ExitCode::SUCCESS
        }
        None => {
            ctx.error("no guest IP visible yet (arp quiet; is the VM up + reachable?)");
            ExitCode::FAILURE
        }
    }
}

fn cmd_ssh(ctx: &mut Ctx, name: &str, user: &str, extra: &[String]) -> ExitCode {
    let Ok(dir) = vm_dir(name) else {
        ctx.error(format!("no such VM: {name}"));
        return ExitCode::from(2);
    };
    let mut st = VmState::load(&dir);
    let Some(ip) = resolve_guest_ip(&dir, &mut st) else {
        ctx.error("no guest IP visible; try `tatara-vmctl up` first or wait for boot");
        return ExitCode::FAILURE;
    };
    let target = format!("{user}@{ip}");
    let mut cmd = Command::new("ssh");
    cmd.arg("-o").arg("StrictHostKeyChecking=no")
        .arg("-o").arg("UserKnownHostsFile=/dev/null")
        .arg(target);
    for e in extra {
        cmd.arg(e);
    }
    match cmd.status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => ExitCode::from(s.code().unwrap_or(1) as u8),
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "ssh error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn iso_now() -> String {
    // Minimal ISO-8601 without pulling in chrono.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{now}")
}

fn usage() {
    eprintln!(
        "\
tatara-vmctl — control plane for tatara-os guests

Usage:
  tatara-vmctl list
  tatara-vmctl status   <name>
  tatara-vmctl build    <name>
  tatara-vmctl up       <name>
  tatara-vmctl down     <name>
  tatara-vmctl destroy  <name>
  tatara-vmctl logs     [--follow] <name>
  tatara-vmctl ip       <name>
  tatara-vmctl ssh      <name> [--user NAME] [-- cmd...]
  tatara-vmctl help

Env:
  TATARA_OS_ROOT     root dir for declared VMs (default: ~/.local/share/tatara-os)
  NO_COLOR           disables the Nord-themed ANSI output
"
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        return ExitCode::from(2);
    }
    let cmd = args[1].as_str();
    let rest = &args[2..];
    let mut ctx = Ctx::new();
    match cmd {
        "list" | "ls" => cmd_list(&mut ctx),
        "status" if !rest.is_empty() => cmd_status(&mut ctx, &rest[0]),
        "build" if !rest.is_empty() => cmd_build(&mut ctx, &rest[0]),
        "up" if !rest.is_empty() => cmd_up(&mut ctx, &rest[0]),
        "down" if !rest.is_empty() => cmd_down(&mut ctx, &rest[0]),
        "destroy" if !rest.is_empty() => cmd_destroy(&mut ctx, &rest[0]),
        "logs" => {
            let (follow, name) = if rest.len() >= 2 && rest[0] == "--follow" {
                (true, rest[1].clone())
            } else if !rest.is_empty() {
                (false, rest[0].clone())
            } else {
                usage();
                return ExitCode::from(2);
            };
            cmd_logs(&mut ctx, &name, follow)
        }
        "ip" if !rest.is_empty() => cmd_ip(&mut ctx, &rest[0]),
        "ssh" if !rest.is_empty() => {
            let name = rest[0].clone();
            let mut user = "root".to_string();
            let mut extras = Vec::new();
            let mut i = 1;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--user" if i + 1 < rest.len() => {
                        user = rest[i + 1].clone();
                        i += 2;
                    }
                    "--" => {
                        extras.extend(rest[i + 1..].iter().cloned());
                        break;
                    }
                    other => {
                        extras.push(other.to_string());
                        i += 1;
                    }
                }
            }
            cmd_ssh(&mut ctx, &name, &user, &extras)
        }
        "help" | "--help" | "-h" => {
            usage();
            ExitCode::SUCCESS
        }
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}
