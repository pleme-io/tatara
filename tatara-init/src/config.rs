//! Typed configuration — the tatara-init equivalent of a systemd unit list.

use serde::{Deserialize, Serialize};
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestartPolicy {
    /// Do not restart on exit.
    Never,
    /// Restart on any non-zero exit.
    OnFailure,
    /// Restart unconditionally (the classic daemon loop).
    Always,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::OnFailure
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub name: String,
    /// Command line. Shell-unescaped; split on whitespace for argv.
    /// Ignored when `body` is present (tatara-init synthesizes the exec
    /// line to invoke its own `--eval` subcommand with the body form).
    #[serde(default)]
    pub exec: String,
    /// Tatara-lisp form evaluated by tatara-init's embedded interpreter.
    /// When set, tatara-init's supervisor spawns `/bin/tatara-init --eval
    /// '<form>'` as the service — no shell, no external binary, just our
    /// own tatara-eval loop running in a forked child.
    #[serde(default)]
    pub body: Option<String>,
    /// How to react to exits.
    #[serde(default)]
    pub restart: RestartPolicy,
    /// Environment pairs.
    #[serde(default)]
    pub env: Vec<(String, String)>,
    /// Optional working directory. Defaults to `/`.
    #[serde(default)]
    pub workdir: Option<String>,
    /// Auto-start at boot? Default true.
    #[serde(default = "default_true")]
    pub enable: bool,
}

impl Service {
    /// Resolve the command line that `LinuxSupervisor::spawn` should exec.
    /// When `body` is set, this substitutes the real exec with a
    /// `tatara-init --eval …` invocation of our own binary.
    pub fn resolved_exec(&self) -> String {
        match &self.body {
            Some(form) => {
                // Escape single quotes by closing + escaping + reopening,
                // POSIX-shell-style.
                let escaped = form.replace('\'', "'\\''");
                format!("/bin/tatara-init --eval '{escaped}'")
            }
            None => self.exec.clone(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// The single root document. Parses from `(definit …)` forms.
#[derive(DeriveTataraDomain, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[tatara(keyword = "definit")]
pub struct InitConfig {
    /// Human label for the init config (logs + attestation).
    #[serde(default = "default_name")]
    pub name: String,
    /// Services to supervise. Starts in declaration order; stops in reverse.
    #[serde(default)]
    pub services: Vec<Service>,
    /// Reap orphaned children (the canonical PID-1 duty). Default on.
    #[serde(default = "default_true")]
    pub reap_zombies: bool,
    /// On receipt of `SIGHUP`, re-read `/etc/tatara/init.lisp` and diff-apply.
    #[serde(default = "default_true")]
    pub reload_on_sighup: bool,
}

fn default_name() -> String {
    "tatara-init".into()
}

impl Default for InitConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            services: vec![],
            reap_zombies: true,
            reload_on_sighup: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_lisp::{domain::TataraDomain, read};

    #[test]
    fn empty_definit_parses() {
        // Note: the tatara-lisp-derive macro currently doesn't honor serde's
        // per-field `default = "fn"`, so omitted bools come back as `false`.
        // Callers that care should use `InitConfig::default()` and modify
        // only what they need, or pass values via the other (typed) API.
        let forms = read(r#"(definit :name "plex-boot")"#).unwrap();
        let c = InitConfig::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(c.name, "plex-boot");
        assert!(c.services.is_empty());
    }

    #[test]
    fn services_round_trip_through_lisp() {
        let forms = read(
            r#"(definit
                 :name "plex-boot"
                 :services ((:name "sshd"     :exec "/bin/sshd -D")
                            (:name "fumi"     :exec "/bin/fumi"  :enable #f)))"#,
        )
        .unwrap();
        let c = InitConfig::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(c.services.len(), 2);
        assert_eq!(c.services[0].name, "sshd");
        assert!(c.services[0].enable);
        assert_eq!(c.services[1].name, "fumi");
        assert!(!c.services[1].enable);
    }

    #[test]
    fn restart_policy_defaults_to_on_failure() {
        let svc = Service {
            name: "x".into(),
            exec: "/x".into(),
            body: None,
            restart: Default::default(),
            env: vec![],
            workdir: None,
            enable: true,
        };
        assert!(matches!(svc.restart, RestartPolicy::OnFailure));
    }

    #[test]
    fn resolved_exec_uses_body_when_present() {
        let svc = Service {
            name: "greet".into(),
            exec: String::new(),
            body: Some("(println 42)".into()),
            restart: Default::default(),
            env: vec![],
            workdir: None,
            enable: true,
        };
        assert_eq!(
            svc.resolved_exec(),
            "/bin/tatara-init --eval '(println 42)'"
        );
    }

    #[test]
    fn resolved_exec_falls_back_to_exec_when_body_absent() {
        let svc = Service {
            name: "x".into(),
            exec: "/bin/x arg1 arg2".into(),
            body: None,
            restart: Default::default(),
            env: vec![],
            workdir: None,
            enable: true,
        };
        assert_eq!(svc.resolved_exec(), "/bin/x arg1 arg2");
    }

    #[test]
    fn resolved_exec_escapes_embedded_single_quotes() {
        let svc = Service {
            name: "quoted".into(),
            exec: String::new(),
            body: Some("(a 'b c)".into()),
            restart: Default::default(),
            env: vec![],
            workdir: None,
            enable: true,
        };
        // Single quotes in the body must get escaped so the outer shell
        // single-quoted string terminates correctly.
        assert!(svc.resolved_exec().contains("'\\''"));
    }
}
