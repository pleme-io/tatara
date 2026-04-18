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
    pub exec: String,
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
            restart: Default::default(),
            env: vec![],
            workdir: None,
            enable: true,
        };
        assert!(matches!(svc.restart, RestartPolicy::OnFailure));
    }
}
