//! Activation script — the sh snippet that moves the running system to
//! match the declared `SystemConfig`. NixOS's `switch-to-configuration`
//! equivalent, emitted from typed inputs.
//!
//! We keep it deliberately minimal + POSIX sh. Real NixOS activation is
//! thousands of lines; we reproduce the essential moves:
//!
//!   1. `hostname` — write `/etc/hostname`, `hostname -F`
//!   2. `etc files` — symlink each declared file into `/etc/`
//!   3. `services` — reload systemd, enable/disable per spec
//!   4. `users` — add missing accounts (useradd), never remove (safety)

use crate::config::{InitSystem, SystemConfig};

/// A rendered activation script + provenance metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationScript {
    pub content: String,
    pub shebang: String,
    pub target: String,
}

impl ActivationScript {
    pub fn render(cfg: &SystemConfig) -> Self {
        let mut s = String::new();
        s.push_str("set -eu\n");
        s.push_str("echo \"[tatara-os] activating configuration for ");
        s.push_str(&cfg.hostname);
        s.push_str("\"\n\n");

        // 1. hostname
        s.push_str("# hostname\n");
        s.push_str(&format!(
            "echo '{}' > /etc/hostname\n",
            shell_single_quote(&cfg.hostname)
        ));
        s.push_str(&format!(
            "hostname '{}' || true\n\n",
            shell_single_quote(&cfg.hostname)
        ));

        // 2. /etc files
        if !cfg.environment.etc_files.is_empty() {
            s.push_str("# /etc files\n");
            for f in &cfg.environment.etc_files {
                s.push_str(&format!(
                    "install -D -m 0644 /run/current-system/etc/{0} /etc/{0}\n",
                    shell_path_component(&f.path)
                ));
            }
            s.push('\n');
        }

        // 3. timezone / locale
        if let Some(tz) = &cfg.environment.timezone {
            s.push_str(&format!(
                "ln -sf /usr/share/zoneinfo/{} /etc/localtime\n",
                shell_path_component(tz)
            ));
        }
        if let Some(loc) = &cfg.environment.locale {
            s.push_str(&format!(
                "echo 'LANG={}' > /etc/locale.conf\n",
                shell_single_quote(loc)
            ));
        }
        if cfg.environment.timezone.is_some() || cfg.environment.locale.is_some() {
            s.push('\n');
        }

        // 4. users
        if !cfg.users.is_empty() {
            s.push_str("# users (safety: only add missing, never remove)\n");
            for u in &cfg.users {
                let mut args = format!("--create-home --shell /bin/sh");
                if let Some(uid) = u.uid {
                    args.push_str(&format!(" --uid {uid}"));
                }
                if let Some(home) = &u.home {
                    args = args.replace("--create-home", "");
                    args.push_str(&format!(" --home-dir '{}'", shell_single_quote(home)));
                }
                if let Some(shell) = &u.shell {
                    args = args.replace("--shell /bin/sh", "");
                    args.push_str(&format!(" --shell '{}'", shell_single_quote(shell)));
                }
                s.push_str(&format!(
                    "id -u '{name}' >/dev/null 2>&1 || useradd {args} '{name}'\n",
                    name = shell_single_quote(&u.name),
                ));
            }
            s.push('\n');
        }

        // 5. services
        if !cfg.services.is_empty() {
            match cfg.init {
                InitSystem::Tatara => {
                    // tatara-init reads /etc/tatara/init.lisp at boot; its
                    // reload-on-SIGHUP behavior re-applies the service set
                    // without a full activation. Nothing to do here beyond
                    // signalling it.
                    s.push_str(
                        "# services (tatara-init — PID 1)\n\
                         if [ -r /run/tatara-init.pid ]; then\n\
                         \x20   kill -HUP \"$(cat /run/tatara-init.pid)\" || true\n\
                         fi\n",
                    );
                }
                InitSystem::Systemd => {
                    s.push_str("# services (systemd)\nsystemctl daemon-reload\n");
                    for svc in &cfg.services {
                        let verb = if svc.enable { "enable --now" } else { "disable" };
                        s.push_str(&format!(
                            "systemctl {verb} '{}.service' || true\n",
                            shell_single_quote(&svc.name)
                        ));
                    }
                }
                InitSystem::S6 => {
                    s.push_str("# services (s6)\n");
                    for svc in &cfg.services {
                        let verb = if svc.enable { "enable" } else { "disable" };
                        s.push_str(&format!(
                            "s6-rc {verb} '{}' || true\n",
                            shell_single_quote(&svc.name)
                        ));
                    }
                }
                InitSystem::OpenRC => {
                    s.push_str("# services (openrc)\n");
                    for svc in &cfg.services {
                        let verb = if svc.enable { "add" } else { "del" };
                        s.push_str(&format!(
                            "rc-update {verb} '{}' default || true\n",
                            shell_single_quote(&svc.name)
                        ));
                    }
                }
            }
            s.push('\n');
        }

        s.push_str("echo \"[tatara-os] activation complete\"\n");

        Self {
            shebang: "#!/bin/sh".into(),
            content: s,
            target: "/run/current-system/activate".into(),
        }
    }

    /// Full script text with shebang prepended.
    pub fn as_file_text(&self) -> String {
        format!("{}\n{}", self.shebang, self.content)
    }
}

fn shell_single_quote(s: &str) -> String {
    // POSIX: single quotes can't be escaped inside single-quoted strings,
    // so we close, emit an escaped quote, reopen.
    s.replace('\'', "'\\''")
}

fn shell_path_component(s: &str) -> String {
    shell_single_quote(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EtcFile, EnvSpec, ServiceSpec, UserSpec};

    fn cfg(hostname: &str) -> SystemConfig {
        SystemConfig {
            hostname: hostname.into(),
            system: "x86_64-linux".into(),
            kernel: Default::default(),
            bootloader: Default::default(),
            init: InitSystem::Systemd,
            services: vec![],
            users: vec![],
            filesystems: vec![],
            environment: EnvSpec::default(),
            packages: vec![],
        }
    }

    #[test]
    fn renders_hostname_line() {
        let s = ActivationScript::render(&cfg("plex"));
        assert!(s.content.contains("echo 'plex' > /etc/hostname"));
    }

    #[test]
    fn renders_service_enable_disable() {
        let mut c = cfg("plex");
        c.services = vec![
            ServiceSpec {
                name: "nginx".into(),
                exec: "nginx".into(),
                enable: true,
                extra: vec![],
                package_refs: vec![],
            },
            ServiceSpec {
                name: "fumi".into(),
                exec: "/bin/fumi".into(),
                enable: false,
                extra: vec![],
                package_refs: vec![],
            },
        ];
        let s = ActivationScript::render(&c);
        assert!(s.content.contains("systemctl enable --now 'nginx.service'"));
        assert!(s.content.contains("systemctl disable 'fumi.service'"));
    }

    #[test]
    fn renders_users_idempotently() {
        let mut c = cfg("plex");
        c.users = vec![UserSpec {
            name: "drzzln".into(),
            uid: Some(1000),
            gid: None,
            home: Some("/home/drzzln".into()),
            shell: Some("/bin/zsh".into()),
            groups: vec![],
        }];
        let s = ActivationScript::render(&c);
        // The idempotency guard
        assert!(s.content.contains("id -u 'drzzln' >/dev/null 2>&1"));
        assert!(s.content.contains("useradd"));
        assert!(s.content.contains("--uid 1000"));
    }

    #[test]
    fn renders_timezone_and_locale() {
        let mut c = cfg("plex");
        c.environment = EnvSpec {
            timezone: Some("America/Sao_Paulo".into()),
            locale: Some("en_US.UTF-8".into()),
            etc_files: vec![],
        };
        let s = ActivationScript::render(&c);
        assert!(s.content.contains("/usr/share/zoneinfo/America/Sao_Paulo"));
        assert!(s.content.contains("LANG=en_US.UTF-8"));
    }

    #[test]
    fn renders_etc_file_install_commands() {
        let mut c = cfg("plex");
        c.environment = EnvSpec {
            timezone: None,
            locale: None,
            etc_files: vec![EtcFile {
                path: "hosts".into(),
                content: "127.0.0.1 localhost\n".into(),
            }],
        };
        let s = ActivationScript::render(&c);
        assert!(s.content.contains("install -D -m 0644 /run/current-system/etc/hosts /etc/hosts"));
    }

    #[test]
    fn file_text_prepends_shebang() {
        let s = ActivationScript::render(&cfg("plex"));
        let full = s.as_file_text();
        assert!(full.starts_with("#!/bin/sh\n"));
    }

    #[test]
    fn quotes_hostnames_with_special_chars() {
        let s = ActivationScript::render(&cfg("my'host"));
        // Single quote gets escaped via POSIX convention.
        assert!(s.content.contains("my'\\''host"));
    }
}
