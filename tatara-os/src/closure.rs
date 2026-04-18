//! `SystemClosure` — the set of derivations that together realize one full
//! operating system.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use tatara_nix::derivation::{
    BridgeTarget, BuilderPhase, BuilderPhases, Derivation, Outputs, Source,
};

use crate::activation::ActivationScript;
use crate::config::{KernelSpec, SystemConfig};

#[derive(Debug, Error)]
pub enum SystemClosureError {
    #[error("package `{0}` not found in any configured PackageSet")]
    MissingPackage(String),

    #[error("kernel spec `{0}` references a package not in any PackageSet")]
    MissingKernel(String),
}

/// The set of derivations that together realize one system. The `profile`
/// derivation is the single entry point — `nix build` it (or realize it via
/// `tatara-nix::Realizer`) and the whole closure materializes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemClosure {
    pub hostname: String,
    pub system: String,
    /// Kernel derivation (bridged from nixpkgs by default).
    pub kernel: Derivation,
    /// `/etc` tree (hostname, os-release, passwd, group, user files).
    pub etc: Derivation,
    /// One derivation per service — systemd unit file or s6/openrc script.
    pub services: BTreeMap<String, Derivation>,
    /// Activation script wrapped into a derivation so the whole closure is
    /// reproducible.
    pub activation: Derivation,
    /// Top-level profile: depends on all of the above, installs a single
    /// `/run/current-system` symlink farm.
    pub profile: Derivation,
}

impl SystemClosure {
    /// Produce the full closure from a `SystemConfig`. Packages referenced in
    /// `cfg.packages` are resolved against the provided `PackageSet`; missing
    /// packages become `MissingPackage` errors.
    pub fn from_config<P: tatara_pkgs::PackageSet + ?Sized>(
        cfg: &SystemConfig,
        pkgs: &P,
    ) -> Result<Self, SystemClosureError> {
        let kernel = match &cfg.kernel {
            KernelSpec::Bridge { attr_path } => bridge_derivation(
                format!("linux-kernel-{}", sanitize(&cfg.hostname)),
                BridgeTarget::nixpkgs(attr_path.clone()),
            ),
            KernelSpec::Package { name } => pkgs
                .get(name)
                .ok()
                .flatten()
                .ok_or_else(|| SystemClosureError::MissingKernel(name.clone()))?,
            KernelSpec::Custom { derivation } => derivation.clone(),
        };

        // Emit /etc as a single Inline-sourced derivation: we script the
        // layout directly into a build script. Files live at /etc/<path>.
        let mut etc_script = String::from("set -eu\nmkdir -p $out\n");
        etc_script.push_str(&format!(
            "echo '{}' > $out/hostname\n",
            shell_escape(&cfg.hostname)
        ));
        etc_script.push_str(&format!(
            "cat > $out/os-release <<'EOF'\nNAME=\"tatara-os\"\nID=tatara\nPRETTY_NAME=\"tatara-os {}\"\nEOF\n",
            cfg.hostname
        ));
        for f in &cfg.environment.etc_files {
            etc_script.push_str(&format!(
                "mkdir -p $(dirname $out/{p}) && cat > $out/{p} <<'TATARA_EOF'\n{c}TATARA_EOF\n",
                p = f.path,
                c = f.content,
            ));
        }
        if !cfg.users.is_empty() {
            etc_script.push_str(": > $out/passwd\n");
            for u in &cfg.users {
                let uid = u.uid.unwrap_or(1000);
                let gid = u.gid.unwrap_or(uid);
                let home = u.home.clone().unwrap_or_else(|| format!("/home/{}", u.name));
                let shell = u.shell.clone().unwrap_or_else(|| "/bin/sh".into());
                etc_script.push_str(&format!(
                    "echo '{n}:x:{uid}:{gid}::{home}:{shell}' >> $out/passwd\n",
                    n = u.name,
                ));
            }
        }
        let etc = hermetic_derivation(
            format!("etc-{}", sanitize(&cfg.hostname)),
            None,
            &etc_script,
        );

        // One systemd unit per service.
        let mut services = BTreeMap::new();
        for svc in &cfg.services {
            let unit = format!(
                "[Unit]\nDescription=tatara-os service: {name}\n\n[Service]\nExecStart={exec}\n{extra}\n\n[Install]\nWantedBy=multi-user.target\n",
                name = svc.name,
                exec = svc.exec,
                extra = svc.extra.join("\n"),
            );
            let d = hermetic_derivation(
                format!("unit-{}", sanitize(&svc.name)),
                None,
                &format!(
                    "set -eu\nmkdir -p $out\ncat > $out/{name}.service <<'TATARA_UNIT'\n{unit}TATARA_UNIT\n",
                    name = svc.name,
                    unit = unit,
                ),
            );
            services.insert(svc.name.clone(), d);
        }

        // Activation script wrapped as a derivation.
        let script = ActivationScript::render(cfg).as_file_text();
        let activation = hermetic_derivation(
            format!("activate-{}", sanitize(&cfg.hostname)),
            None,
            &format!(
                "set -eu\nmkdir -p $out\ncat > $out/activate <<'TATARA_ACTIVATE'\n{}\nTATARA_ACTIVATE\nchmod +x $out/activate\n",
                script
            ),
        );

        // Profile: a flat symlink farm in a single output.
        let mut profile_script = String::from(
            "set -eu\nmkdir -p $out\n\
             ln -s ${kernel} $out/kernel\n\
             ln -s ${etc} $out/etc\n\
             ln -s ${activation}/activate $out/activate\n",
        );
        for name in services.keys() {
            profile_script.push_str(&format!(
                "ln -s ${{{name}}} $out/services/{name}\n",
                name = name
            ));
        }
        profile_script.push_str("mkdir -p $out/services\n");

        let profile = hermetic_derivation(
            format!("profile-{}", sanitize(&cfg.hostname)),
            None,
            &profile_script,
        );

        Ok(Self {
            hostname: cfg.hostname.clone(),
            system: cfg.system.clone(),
            kernel,
            etc,
            services,
            activation,
            profile,
        })
    }

    /// Flatten the whole closure to a Vec<&Derivation> for realization.
    pub fn all_derivations(&self) -> Vec<&Derivation> {
        let mut out: Vec<&Derivation> = vec![&self.kernel, &self.etc, &self.activation];
        for d in self.services.values() {
            out.push(d);
        }
        out.push(&self.profile);
        out
    }
}

// ── helpers ─────────────────────────────────────────────────────────────

fn bridge_derivation(name: String, bridge: BridgeTarget) -> Derivation {
    Derivation {
        name,
        version: None,
        inputs: vec![],
        source: Source::default(),
        builder: BuilderPhases::default(),
        outputs: Outputs::default(),
        env: vec![],
        sandbox: Default::default(),
        bridge: Some(bridge),
        nix_expr: None,
    }
}

fn hermetic_derivation(name: String, version: Option<String>, install_cmd: &str) -> Derivation {
    let mut commands = BTreeMap::new();
    commands.insert("Install".to_string(), vec![install_cmd.to_string()]);
    Derivation {
        name,
        version,
        inputs: vec![],
        source: Source::Inline {
            content: String::new(),
        },
        builder: BuilderPhases {
            phases: vec![BuilderPhase::Install],
            commands,
        },
        outputs: Outputs::default(),
        env: vec![],
        sandbox: Default::default(),
        bridge: None,
        nix_expr: None,    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn shell_escape(s: &str) -> String {
    s.replace('\'', "'\\''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{InitSystem, ServiceSpec, UserSpec};
    use tatara_pkgs::NixpkgsBridge;

    fn basic_cfg() -> SystemConfig {
        SystemConfig {
            hostname: "plex".into(),
            system: "x86_64-linux".into(),
            kernel: KernelSpec::Bridge {
                attr_path: "linuxPackages.kernel".into(),
            },
            bootloader: Default::default(),
            init: InitSystem::Systemd,
            services: vec![ServiceSpec {
                name: "nginx".into(),
                exec: "nginx -g 'daemon off;'".into(),
                enable: true,
                extra: vec![],
                package_refs: vec![],
            }],
            users: vec![UserSpec {
                name: "drzzln".into(),
                uid: Some(1000),
                gid: Some(1000),
                home: Some("/home/drzzln".into()),
                shell: Some("/bin/zsh".into()),
                groups: vec![],
            }],
            filesystems: vec![],
            environment: Default::default(),
            packages: vec![],
            sshd: None,
        }
    }

    #[test]
    fn closure_builds_with_all_parts() {
        let pkgs = NixpkgsBridge::new();
        let closure = SystemClosure::from_config(&basic_cfg(), &pkgs).unwrap();
        assert_eq!(closure.hostname, "plex");
        assert!(closure.kernel.bridge.is_some());
        assert_eq!(closure.kernel.bridge.unwrap().attr_path, "linuxPackages.kernel");
        assert_eq!(closure.services.len(), 1);
        assert!(closure.services.contains_key("nginx"));
    }

    #[test]
    fn closure_derivations_have_deterministic_paths() {
        let pkgs = NixpkgsBridge::new();
        let a = SystemClosure::from_config(&basic_cfg(), &pkgs).unwrap();
        let b = SystemClosure::from_config(&basic_cfg(), &pkgs).unwrap();
        assert_eq!(a.profile.store_path(), b.profile.store_path());
        assert_eq!(a.etc.store_path(), b.etc.store_path());
    }

    #[test]
    fn kernel_custom_derivation_passes_through() {
        let pkgs = NixpkgsBridge::new();
        let mut cfg = basic_cfg();
        let custom = hermetic_derivation(
            "my-kernel".into(),
            Some("6.1.0".into()),
            "mkdir -p $out && echo stub > $out/stub",
        );
        cfg.kernel = KernelSpec::Custom {
            derivation: custom.clone(),
        };
        let closure = SystemClosure::from_config(&cfg, &pkgs).unwrap();
        assert_eq!(closure.kernel.name, "my-kernel");
        assert!(closure.kernel.bridge.is_none());
    }

    #[test]
    fn missing_package_kernel_errors() {
        let pkgs = NixpkgsBridge::new().with_names(vec!["hello".into()]);
        let mut cfg = basic_cfg();
        cfg.kernel = KernelSpec::Package {
            name: "nonexistent".into(),
        };
        let err = SystemClosure::from_config(&cfg, &pkgs).unwrap_err();
        assert!(matches!(err, SystemClosureError::MissingKernel(_)));
    }

    #[test]
    fn all_derivations_includes_every_part() {
        let pkgs = NixpkgsBridge::new();
        let closure = SystemClosure::from_config(&basic_cfg(), &pkgs).unwrap();
        let all = closure.all_derivations();
        // kernel + etc + activation + 1 service + profile = 5
        assert_eq!(all.len(), 5);
    }
}
