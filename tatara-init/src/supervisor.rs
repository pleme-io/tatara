//! The supervision engine — abstract `Supervisor` trait + two impls.
//!
//! `LinuxSupervisor` does the real PID-1 work via `fork(2)`/`execve(2)`;
//! `MockSupervisor` records actions for unit tests. The orchestration loop
//! in [`run_once`] is written against the trait, so testing the scheduler
//! logic requires no privileges.

use std::collections::HashMap;
use thiserror::Error;

use crate::config::{InitConfig, RestartPolicy, Service};

pub type Pid = i32;

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("failed to spawn {name}: {reason}")]
    Spawn { name: String, reason: String },

    #[error("failed to signal pid {pid}: {reason}")]
    Signal { pid: Pid, reason: String },

    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SupervisorError>;

/// Minimum interface a supervisor backend must expose.
///
/// Designed small enough for a mock to be a few dozen lines and big enough
/// that the scheduler loop doesn't need to reach past it for anything
/// Linux-specific.
pub trait Supervisor {
    /// Spawn a service; return its PID.
    fn spawn(&mut self, svc: &Service) -> Result<Pid>;

    /// Deliver SIGTERM to a PID. For graceful termination.
    fn terminate(&mut self, pid: Pid) -> Result<()>;

    /// Deliver SIGKILL. For the last resort.
    fn kill(&mut self, pid: Pid) -> Result<()>;

    /// Block until any child exits, or return None immediately if no
    /// children have exited. Returns `(pid, exit_status)` when available.
    fn reap_one(&mut self) -> Result<Option<(Pid, i32)>>;

    /// Live children tracked by this supervisor.
    fn children(&self) -> Vec<(Pid, String)>;
}

// ── Mock ────────────────────────────────────────────────────────────────

/// In-memory supervisor for tests. Records spawns + signals; `reap_one`
/// returns queued exits in FIFO order.
#[derive(Default)]
pub struct MockSupervisor {
    next_pid: Pid,
    live: HashMap<Pid, String>,
    /// Pre-queued (pid, exit_status) pairs the test wants `reap_one` to
    /// return next.
    pub queued_exits: Vec<(Pid, i32)>,
    /// Log of every action the scheduler took; lets tests assert on order.
    pub log: Vec<String>,
}

impl MockSupervisor {
    pub fn new() -> Self {
        Self {
            next_pid: 100,
            ..Default::default()
        }
    }

    /// Inject an exit — `reap_one` will surface this next.
    pub fn queue_exit(&mut self, pid: Pid, status: i32) {
        self.queued_exits.push((pid, status));
    }
}

impl Supervisor for MockSupervisor {
    fn spawn(&mut self, svc: &Service) -> Result<Pid> {
        let pid = self.next_pid;
        self.next_pid += 1;
        self.live.insert(pid, svc.name.clone());
        self.log.push(format!("spawn {} ({})", svc.name, pid));
        Ok(pid)
    }

    fn terminate(&mut self, pid: Pid) -> Result<()> {
        self.log.push(format!("SIGTERM {pid}"));
        Ok(())
    }

    fn kill(&mut self, pid: Pid) -> Result<()> {
        self.log.push(format!("SIGKILL {pid}"));
        self.live.remove(&pid);
        Ok(())
    }

    fn reap_one(&mut self) -> Result<Option<(Pid, i32)>> {
        if let Some((pid, status)) = self.queued_exits.pop() {
            let name = self.live.remove(&pid).unwrap_or_else(|| "?".into());
            self.log.push(format!("reap {name} ({pid}) -> {status}"));
            Ok(Some((pid, status)))
        } else {
            Ok(None)
        }
    }

    fn children(&self) -> Vec<(Pid, String)> {
        self.live.iter().map(|(p, n)| (*p, n.clone())).collect()
    }
}

// ── Linux ───────────────────────────────────────────────────────────────

/// Real PID-1 backend. Uses libc `fork`/`execve`/`waitpid`/`kill`. Safe to
/// compile on macOS (same libc surface); running meaningfully requires
/// being PID 1 in a Linux kernel.
pub struct LinuxSupervisor {
    live: HashMap<Pid, String>,
}

impl Default for LinuxSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSupervisor {
    pub fn new() -> Self {
        Self {
            live: HashMap::new(),
        }
    }
}

#[cfg(unix)]
impl Supervisor for LinuxSupervisor {
    fn spawn(&mut self, svc: &Service) -> Result<Pid> {
        use std::ffi::CString;
        // `resolved_exec()` honors the `body` (Lisp form) shortcut when
        // present, rewriting the service to invoke `tatara-init --eval`.
        let argv: Vec<String> = split_exec(&svc.resolved_exec());
        if argv.is_empty() {
            return Err(SupervisorError::Spawn {
                name: svc.name.clone(),
                reason: "empty exec line".into(),
            });
        }
        let argv_c: Vec<CString> = argv
            .iter()
            .map(|s| CString::new(s.as_str()).unwrap_or_default())
            .collect();
        let cwd_c = svc
            .workdir
            .as_deref()
            .and_then(|c| CString::new(c).ok());
        let env_c: Vec<(CString, CString)> = svc
            .env
            .iter()
            .filter_map(|(k, v)| {
                Some((CString::new(k.as_str()).ok()?, CString::new(v.as_str()).ok()?))
            })
            .collect();

        // SAFETY: called before we start threads. fork()==0 is the child.
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return Err(SupervisorError::Spawn {
                name: svc.name.clone(),
                reason: std::io::Error::last_os_error().to_string(),
            });
        }
        if pid == 0 {
            // ── child path: set env, chdir, execvp. No Rust-level panics
            // (would poison shared state in the parent via atexit etc.).
            if let Some(c) = cwd_c.as_ref() {
                unsafe { libc::chdir(c.as_ptr()) };
            }
            for (k, v) in &env_c {
                unsafe { libc::setenv(k.as_ptr(), v.as_ptr(), 1) };
            }
            let mut argv_ptr: Vec<*const libc::c_char> =
                argv_c.iter().map(|s| s.as_ptr()).collect();
            argv_ptr.push(std::ptr::null());
            unsafe { libc::execvp(argv_c[0].as_ptr(), argv_ptr.as_ptr()) };
            // execvp only returns on failure.
            unsafe { libc::_exit(127) };
        }

        self.live.insert(pid, svc.name.clone());
        Ok(pid)
    }

    fn terminate(&mut self, pid: Pid) -> Result<()> {
        send_signal(pid, libc::SIGTERM)
    }

    fn kill(&mut self, pid: Pid) -> Result<()> {
        let out = send_signal(pid, libc::SIGKILL);
        self.live.remove(&pid);
        out
    }

    fn reap_one(&mut self) -> Result<Option<(Pid, i32)>> {
        let mut status: libc::c_int = 0;
        // SAFETY: waitpid is async-signal-safe; -1 reaps any child, WNOHANG
        // makes it non-blocking.
        let r = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if r == 0 {
            return Ok(None);
        }
        if r < 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::ECHILD) {
                return Ok(None);
            }
            return Err(SupervisorError::Io(e));
        }
        self.live.remove(&r);
        let exit_code = unsafe {
            if libc::WIFEXITED(status) {
                libc::WEXITSTATUS(status)
            } else {
                128 + libc::WTERMSIG(status)
            }
        };
        Ok(Some((r, exit_code)))
    }

    fn children(&self) -> Vec<(Pid, String)> {
        self.live.iter().map(|(p, n)| (*p, n.clone())).collect()
    }
}

#[cfg(not(unix))]
impl Supervisor for LinuxSupervisor {
    fn spawn(&mut self, _svc: &Service) -> Result<Pid> {
        Err(SupervisorError::Spawn {
            name: "n/a".into(),
            reason: "LinuxSupervisor requires a Unix host".into(),
        })
    }
    fn terminate(&mut self, _pid: Pid) -> Result<()> {
        Ok(())
    }
    fn kill(&mut self, _pid: Pid) -> Result<()> {
        Ok(())
    }
    fn reap_one(&mut self) -> Result<Option<(Pid, i32)>> {
        Ok(None)
    }
    fn children(&self) -> Vec<(Pid, String)> {
        vec![]
    }
}

#[cfg(unix)]
fn send_signal(pid: Pid, sig: libc::c_int) -> Result<()> {
    // SAFETY: kill(2) takes a PID + signum; no aliasing concerns.
    let r = unsafe { libc::kill(pid, sig) };
    if r != 0 {
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::ESRCH) {
            return Ok(()); // already dead
        }
        return Err(SupervisorError::Signal {
            pid,
            reason: e.to_string(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn send_signal(_pid: Pid, _sig: i32) -> Result<()> {
    Ok(())
}

// ── scheduler core — written once, reused by real + mock backends ────────

/// Walk the config and spawn every enabled service. Returns the per-name
/// PIDs. Deterministic; errors on the first spawn failure with no partial
/// state (caller can retry with a new env).
pub fn boot<S: Supervisor>(sup: &mut S, cfg: &InitConfig) -> Result<HashMap<String, Pid>> {
    let mut by_name = HashMap::new();
    for svc in &cfg.services {
        if !svc.enable {
            continue;
        }
        let pid = sup.spawn(svc)?;
        by_name.insert(svc.name.clone(), pid);
    }
    Ok(by_name)
}

/// Drain any pending child exits. Applies the restart policy for each
/// service keyed by the PID we spawned for it.
pub fn run_once<S: Supervisor>(
    sup: &mut S,
    cfg: &InitConfig,
    tracking: &mut HashMap<Pid, String>,
) -> Result<Vec<ReapedEvent>> {
    let mut events = Vec::new();
    while let Some((pid, status)) = sup.reap_one()? {
        let name = tracking.remove(&pid);
        let svc = name
            .as_deref()
            .and_then(|n| cfg.services.iter().find(|s| s.name == n));
        let restart = svc.map(|s| s.restart).unwrap_or(RestartPolicy::Never);
        let should_restart = match (restart, status) {
            (RestartPolicy::Always, _) => true,
            (RestartPolicy::OnFailure, 0) => false,
            (RestartPolicy::OnFailure, _) => true,
            (RestartPolicy::Never, _) => false,
        };
        let new_pid = if should_restart {
            if let Some(s) = svc {
                let new_pid = sup.spawn(s)?;
                tracking.insert(new_pid, s.name.clone());
                Some(new_pid)
            } else {
                None
            }
        } else {
            None
        };
        events.push(ReapedEvent {
            pid,
            name: name.unwrap_or_else(|| "unknown".into()),
            exit_status: status,
            restarted_as: new_pid,
        });
    }
    Ok(events)
}

#[derive(Debug, Clone)]
pub struct ReapedEvent {
    pub pid: Pid,
    pub name: String,
    pub exit_status: i32,
    pub restarted_as: Option<Pid>,
}

// ── helpers ─────────────────────────────────────────────────────────────

fn split_exec(cmd: &str) -> Vec<String> {
    cmd.split_whitespace().map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RestartPolicy, Service};

    fn svc(name: &str, exec: &str, restart: RestartPolicy, enable: bool) -> Service {
        Service {
            name: name.into(),
            exec: exec.into(),
            body: None,
            restart,
            env: vec![],
            workdir: None,
            enable,
        }
    }

    #[test]
    fn boot_spawns_every_enabled_service() {
        let mut sup = MockSupervisor::new();
        let cfg = InitConfig {
            services: vec![
                svc("a", "/a", RestartPolicy::Never, true),
                svc("b", "/b", RestartPolicy::Never, false), // disabled
                svc("c", "/c", RestartPolicy::Never, true),
            ],
            ..Default::default()
        };
        let by_name = boot(&mut sup, &cfg).unwrap();
        assert_eq!(by_name.len(), 2);
        assert!(by_name.contains_key("a"));
        assert!(by_name.contains_key("c"));
        assert_eq!(sup.children().len(), 2);
    }

    #[test]
    fn restart_policy_always_respawns_on_zero_exit() {
        let mut sup = MockSupervisor::new();
        let cfg = InitConfig {
            services: vec![svc("daemon", "/daemon", RestartPolicy::Always, true)],
            ..Default::default()
        };
        let by_name = boot(&mut sup, &cfg).unwrap();
        let mut tracking: HashMap<Pid, String> = by_name
            .iter()
            .map(|(n, p)| (*p, n.clone()))
            .collect();
        sup.queue_exit(tracking.keys().next().copied().unwrap(), 0);
        let events = run_once(&mut sup, &cfg, &mut tracking).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].restarted_as.is_some());
        assert_eq!(sup.children().len(), 1);
    }

    #[test]
    fn restart_policy_on_failure_respawns_only_on_nonzero() {
        let mut sup = MockSupervisor::new();
        let cfg = InitConfig {
            services: vec![svc("job", "/job", RestartPolicy::OnFailure, true)],
            ..Default::default()
        };
        let by_name = boot(&mut sup, &cfg).unwrap();
        let mut tracking: HashMap<Pid, String> = by_name
            .iter()
            .map(|(n, p)| (*p, n.clone()))
            .collect();
        let pid = *tracking.keys().next().unwrap();
        sup.queue_exit(pid, 0);
        let ev = run_once(&mut sup, &cfg, &mut tracking).unwrap();
        assert_eq!(ev.len(), 1);
        assert!(ev[0].restarted_as.is_none());
        assert_eq!(sup.children().len(), 0);
    }

    #[test]
    fn restart_policy_never_never_respawns() {
        let mut sup = MockSupervisor::new();
        let cfg = InitConfig {
            services: vec![svc("oneshot", "/oneshot", RestartPolicy::Never, true)],
            ..Default::default()
        };
        let by_name = boot(&mut sup, &cfg).unwrap();
        let mut tracking: HashMap<Pid, String> = by_name
            .iter()
            .map(|(n, p)| (*p, n.clone()))
            .collect();
        sup.queue_exit(*tracking.keys().next().unwrap(), 1);
        let ev = run_once(&mut sup, &cfg, &mut tracking).unwrap();
        assert_eq!(ev.len(), 1);
        assert!(ev[0].restarted_as.is_none());
    }

    #[test]
    fn reap_one_returns_none_when_nothing_queued() {
        let mut sup = MockSupervisor::new();
        assert!(sup.reap_one().unwrap().is_none());
    }

    #[test]
    fn mock_logs_are_sequential_and_useful() {
        let mut sup = MockSupervisor::new();
        let s = svc("x", "/x", RestartPolicy::Never, true);
        sup.spawn(&s).unwrap();
        let children = sup.children();
        assert_eq!(children.len(), 1);
        let pid = children[0].0;
        sup.terminate(pid).unwrap();
        sup.queue_exit(pid, 0);
        sup.reap_one().unwrap();
        assert_eq!(sup.log[0], "spawn x (100)");
        assert_eq!(sup.log[1], "SIGTERM 100");
        assert_eq!(sup.log[2], "reap x (100) -> 0");
    }
}
