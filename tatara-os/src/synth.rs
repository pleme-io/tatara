//! `SystemSynthesizer` — implements the universal `Synthesizer` trait from
//! `tatara_nix::synth`. SystemConfig → SystemClosure → Vec<Artifact>.

use tatara_nix::{Artifact, MultiSynthesizer};

use crate::closure::{SystemClosure, SystemClosureError};
use crate::config::SystemConfig;

/// Multi-file emitter: drops the system closure to disk as a directory of
/// manifests (one per derivation) + an `activate.sh` script + a
/// `system.json` with the full typed config.
pub struct SystemSynthesizer<'a> {
    pub pkgs: &'a dyn tatara_pkgs::PackageSet,
    pub out_prefix: String,
}

impl<'a> SystemSynthesizer<'a> {
    pub fn new(pkgs: &'a dyn tatara_pkgs::PackageSet) -> Self {
        Self {
            pkgs,
            out_prefix: "system".into(),
        }
    }

    pub fn with_prefix(mut self, p: impl Into<String>) -> Self {
        self.out_prefix = p.into();
        self
    }

    pub fn build_closure(&self, cfg: &SystemConfig) -> Result<SystemClosure, SystemClosureError> {
        SystemClosure::from_config(cfg, self.pkgs)
    }
}

impl<'a> MultiSynthesizer for SystemSynthesizer<'a> {
    type Input = SystemConfig;

    fn generate_all(&self, cfg: &SystemConfig) -> Vec<Artifact> {
        let closure = match self.build_closure(cfg) {
            Ok(c) => c,
            Err(e) => {
                return vec![Artifact::new(
                    format!("{}/ERROR.txt", self.out_prefix),
                    e.to_string(),
                )]
            }
        };

        let mut arts = Vec::new();
        let prefix = &self.out_prefix;

        // system.json — the whole typed config, re-serialized.
        if let Ok(json) = serde_json::to_string_pretty(cfg) {
            arts.push(Artifact::new(format!("{prefix}/system.json"), json));
        }

        // closure.json — the derived closure itself (for attestation).
        if let Ok(json) = serde_json::to_string_pretty(&closure) {
            arts.push(Artifact::new(format!("{prefix}/closure.json"), json));
        }

        // activate.sh — the activation script, ready to run after realization.
        let script = crate::activation::ActivationScript::render(cfg);
        arts.push(Artifact::new(
            format!("{prefix}/activate.sh"),
            script.as_file_text(),
        ));

        // manifest.txt — a flat list of derivation names + store paths.
        let mut manifest = format!("# tatara-os closure manifest — {}\n", cfg.hostname);
        for d in closure.all_derivations() {
            manifest.push_str(&format!("{} {}\n", d.store_path(), d.name));
        }
        arts.push(Artifact::new(format!("{prefix}/manifest.txt"), manifest));

        arts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InitSystem;
    use tatara_pkgs::NixpkgsBridge;

    fn cfg() -> SystemConfig {
        SystemConfig {
            hostname: "plex".into(),
            system: "x86_64-linux".into(),
            kernel: Default::default(),
            bootloader: Default::default(),
            init: InitSystem::Systemd,
            services: vec![],
            users: vec![],
            filesystems: vec![],
            environment: Default::default(),
            packages: vec![],
            sshd: None,
        }
    }

    #[test]
    fn synth_emits_core_manifests() {
        let pkgs = NixpkgsBridge::new();
        let s = SystemSynthesizer::new(&pkgs).with_prefix("system");
        let arts = s.generate_all(&cfg());
        let paths: Vec<_> = arts.iter().map(|a| a.path.as_str()).collect();
        assert!(paths.contains(&"system/system.json"));
        assert!(paths.contains(&"system/closure.json"));
        assert!(paths.contains(&"system/activate.sh"));
        assert!(paths.contains(&"system/manifest.txt"));
    }

    #[test]
    fn activate_script_contains_hostname() {
        let pkgs = NixpkgsBridge::new();
        let s = SystemSynthesizer::new(&pkgs);
        let arts = s.generate_all(&cfg());
        let act = arts
            .iter()
            .find(|a| a.path.ends_with("/activate.sh"))
            .unwrap();
        assert!(act.content.contains("echo 'plex' > /etc/hostname"));
    }

    #[test]
    fn manifest_lists_every_derivation_in_closure() {
        let pkgs = NixpkgsBridge::new();
        let mut c = cfg();
        c.services = vec![crate::config::ServiceSpec {
            name: "nginx".into(),
            exec: "nginx".into(),
            enable: true,
            extra: vec![],
            package_refs: vec![],
        }];
        let s = SystemSynthesizer::new(&pkgs);
        let arts = s.generate_all(&c);
        let manifest = arts
            .iter()
            .find(|a| a.path.ends_with("/manifest.txt"))
            .unwrap();
        let nginx_rows = manifest
            .content
            .lines()
            .filter(|l| l.contains("unit-nginx"))
            .count();
        assert_eq!(nginx_rows, 1);
    }

    #[test]
    fn synth_is_deterministic() {
        let pkgs = NixpkgsBridge::new();
        let s = SystemSynthesizer::new(&pkgs);
        let a = s.generate_all(&cfg());
        let b = s.generate_all(&cfg());
        assert_eq!(a, b);
    }
}
