//! `SystemConfig` — the typed surface of a whole operating system.

use serde::{Deserialize, Serialize};
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

// ── subtypes ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum KernelSpec {
    /// Pull the kernel derivation from nixpkgs (or a custom package set).
    /// Default attribute: `linuxPackages.kernel`.
    Bridge { attr_path: String },
    /// A named package from a `tatara-pkgs::PackageSet` resolved at synth time.
    Package { name: String },
    /// The raw tatara `Derivation` — for a fully-custom kernel.
    Custom {
        derivation: tatara_nix::Derivation,
    },
}

impl Default for KernelSpec {
    fn default() -> Self {
        Self::Bridge {
            attr_path: "linuxPackages.kernel".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootloaderKind {
    Grub,
    SystemdBoot,
    Uboot,
    None,
}

impl Default for BootloaderKind {
    fn default() -> Self {
        Self::SystemdBoot
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootloaderSpec {
    #[serde(default)]
    pub kind: BootloaderKind,
    /// E.g., `/boot`, `/dev/sda`. Empty when kind is None.
    #[serde(default)]
    pub device: String,
    /// Extra kernel command-line args. Joined with spaces.
    #[serde(default)]
    pub kernel_cmdline: Vec<String>,
}

impl Default for BootloaderSpec {
    fn default() -> Self {
        Self {
            kind: BootloaderKind::default(),
            device: "/boot".into(),
            kernel_cmdline: vec![],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InitSystem {
    /// tatara-init as PID 1 — the default. No systemd, no dbus, no cgroup
    /// choreography. Services declared in `services` are handed to the
    /// tatara-init supervisor at boot.
    Tatara,
    Systemd,
    S6,
    OpenRC,
}

impl Default for InitSystem {
    fn default() -> Self {
        Self::Tatara
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSpec {
    pub name: String,
    /// Command to exec. Will be wrapped in the init system's unit format.
    pub exec: String,
    /// Auto-start at boot?
    #[serde(default = "default_true")]
    pub enable: bool,
    /// Additional systemd unit directives (raw ini form when using Systemd).
    #[serde(default)]
    pub extra: Vec<String>,
    /// Package references (from PackageSet) this service depends on.
    #[serde(default)]
    pub package_refs: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSpec {
    pub name: String,
    #[serde(default)]
    pub uid: Option<u32>,
    #[serde(default)]
    pub gid: Option<u32>,
    #[serde(default)]
    pub home: Option<String>,
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub groups: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemSpec {
    pub mount: String,
    pub device: String,
    pub fs_type: String,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EnvSpec {
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    /// Extra files to land in `/etc/<path>`: `name` → file contents.
    /// Keyed by path relative to /etc (e.g., `"hosts"`, `"resolv.conf"`).
    #[serde(default)]
    pub etc_files: Vec<EtcFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EtcFile {
    pub path: String,
    pub content: String,
}

// ── root ────────────────────────────────────────────────────────────────

/// The whole operating system as one typed value.
///
/// ```lisp
/// (defsystem plex
///   :hostname "plex"
///   :system   "x86_64-linux"
///   :kernel   (:kind Bridge :attr-path "linuxPackages.kernel")
///   :bootloader (:kind SystemdBoot :device "/boot")
///   :init       Systemd
///   :services   ((:name "nginx" :exec "nginx -g 'daemon off;'"))
///   :users      ((:name "drzzln" :uid 1000)))
/// ```
#[derive(DeriveTataraDomain, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defsystem")]
pub struct SystemConfig {
    pub hostname: String,
    /// E.g., `x86_64-linux`, `aarch64-linux`.
    #[serde(default = "default_system")]
    pub system: String,
    #[serde(default)]
    pub kernel: KernelSpec,
    #[serde(default)]
    pub bootloader: BootloaderSpec,
    #[serde(default)]
    pub init: InitSystem,
    #[serde(default)]
    pub services: Vec<ServiceSpec>,
    #[serde(default)]
    pub users: Vec<UserSpec>,
    #[serde(default)]
    pub filesystems: Vec<FilesystemSpec>,
    #[serde(default)]
    pub environment: EnvSpec,
    /// Named package references that must be installed systemwide.
    #[serde(default)]
    pub packages: Vec<String>,
}

fn default_system() -> String {
    "x86_64-linux".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_lisp::{domain::TataraDomain, read};

    #[test]
    fn minimal_system_compiles_from_lisp() {
        let forms = read(
            r#"(defsystem
                 :hostname "plex"
                 :system   "x86_64-linux")"#,
        )
        .unwrap();
        let s = SystemConfig::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(s.hostname, "plex");
        assert_eq!(s.system, "x86_64-linux");
        // Default init is Tatara — no systemd, tatara is PID 1.
        assert!(matches!(s.init, InitSystem::Tatara));
        assert!(matches!(s.bootloader.kind, BootloaderKind::SystemdBoot));
    }

    #[test]
    fn full_system_compiles_from_lisp() {
        let forms = read(
            r#"(defsystem
                 :hostname "plex"
                 :system   "x86_64-linux"
                 :services ((:name "nginx" :exec "nginx -g 'daemon off;'")
                            (:name "fumi"  :exec "/bin/fumi" :enable #f))
                 :users    ((:name "drzzln" :uid 1000))
                 :packages ("hello" "bash" "zsh"))"#,
        )
        .unwrap();
        let s = SystemConfig::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(s.services.len(), 2);
        assert_eq!(s.services[1].name, "fumi");
        assert!(!s.services[1].enable);
        assert_eq!(s.users[0].name, "drzzln");
        assert_eq!(s.packages, vec!["hello", "bash", "zsh"]);
    }
}
