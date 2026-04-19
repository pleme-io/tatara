//! Typed VM definitions — the Lisp authoring surface.

use serde::{Deserialize, Serialize};
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Hypervisor {
    /// Apple Virtualization.framework via the `vfkit` CLI — Linux guests.
    /// Default on Darwin.
    Vfkit,
    /// Apple Virtualization.framework for **Darwin guests** (Apple Silicon
    /// hosts only). Needs a Darwin IPSW; bootable via `tart` or Apple's own
    /// `macosvm` tool.
    VfkitDarwin,
    /// Portable fallback. KVM on Linux, HVF on Darwin/Intel.
    Qemu,
    /// pleme-io's Rust-native Virtualization.framework wrapper — drives the
    /// VM in-process, no CLI shell-out.
    Kasou,
}

impl Default for Hypervisor {
    fn default() -> Self {
        Self::Vfkit
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum GuestKernel {
    /// Pull the kernel derivation from nixpkgs (or another pkg-set).
    Bridge { attr_path: String },
    /// Raw tatara Derivation for a fully-custom kernel build.
    Custom { derivation: tatara_nix::Derivation },
    /// Darwin guest boot assets (IPSW + RestoreImage). Only meaningful with
    /// `Hypervisor::VfkitDarwin` / `Hypervisor::Kasou` on an Apple-Silicon
    /// host. The IPSW path is passed through to Virtualization.framework's
    /// `MacOSRestoreImage` API.
    DarwinIpsw { ipsw_path: String },
}

impl Default for GuestKernel {
    fn default() -> Self {
        Self::Bridge {
            attr_path: "linuxPackages.kernel".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum GuestRootfs {
    /// Derive the rootfs from a tatara-os `SystemConfig` by name (resolved at
    /// emit time against a provided SystemConfig registry).
    System { name: String },
    /// Pre-built rootfs image as a tatara Derivation (ext4 image in $out/rootfs.img).
    Image { derivation: tatara_nix::Derivation },
    /// Bridge to a nixpkgs attribute producing a disk image (e.g., `nixos-generators.qcow`).
    Bridge { attr_path: String },
}

impl Default for GuestRootfs {
    fn default() -> Self {
        Self::Bridge {
            attr_path: "nixpkgs-images.minimal-rootfs".into(),
        }
    }
}

/// Unit-variant enum serialized as plain strings: `"Nat"`, `"Bridge"`, `"None"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkKind {
    /// NAT via the host.
    Nat,
    /// Bridge to a host interface.
    Bridge,
    /// No network.
    None,
}

impl Default for NetworkKind {
    fn default() -> Self {
        Self::Nat
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSpec {
    #[serde(default)]
    pub kind: NetworkKind,
    #[serde(default)]
    pub subnet: Option<String>,
    #[serde(default)]
    pub host_interface: Option<String>,
}

impl Default for NetworkSpec {
    fn default() -> Self {
        Self {
            kind: NetworkKind::Nat,
            subnet: None,
            host_interface: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSpec {
    /// Host-side path (absolute).
    pub host: String,
    /// Guest mount point (absolute).
    pub guest: String,
    /// Read-only share? Default false.
    #[serde(default)]
    pub read_only: bool,
}

/// The whole guest as one typed value. Parses from `(defvm …)` forms.
///
/// ```lisp
/// (defvm plex-guest
///   :cpus 4
///   :memory_mib 4096
///   :hypervisor (:kind "Vfkit")
///   :kernel     (:kind "Bridge" :attr_path "linuxPackages.kernel")
///   :rootfs     (:kind "Bridge" :attr_path "nixpkgs-images.minimal-rootfs")
///   :network    (:kind "Nat")
///   :cmdline    ("console=hvc0" "init=/bin/tatara-init"))
/// ```
#[derive(DeriveTataraDomain, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defvm")]
pub struct VmSpec {
    pub name: String,
    #[serde(default = "default_cpus")]
    pub cpus: u32,
    #[serde(default = "default_mem_mib")]
    pub memory_mib: u32,
    #[serde(default)]
    pub hypervisor: Hypervisor,
    #[serde(default)]
    pub kernel: GuestKernel,
    #[serde(default)]
    pub initrd: Option<tatara_nix::Derivation>,
    #[serde(default)]
    pub rootfs: GuestRootfs,
    #[serde(default)]
    pub network: NetworkSpec,
    #[serde(default)]
    pub shares: Vec<ShareSpec>,
    /// Kernel command-line. Joined with spaces in emitted config.
    #[serde(default = "default_cmdline")]
    pub cmdline: Vec<String>,
}

fn default_cpus() -> u32 {
    2
}

fn default_mem_mib() -> u32 {
    2048
}

fn default_cmdline() -> Vec<String> {
    vec!["console=hvc0".into(), "init=/bin/tatara-init".into()]
}

impl VmSpec {
    /// The baseline used in examples — proves the Lisp → typed chain without
    /// any external files. Matches what our vfkit emitter wants.
    pub fn plex_default(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cpus: default_cpus(),
            memory_mib: default_mem_mib(),
            hypervisor: Hypervisor::Vfkit,
            kernel: GuestKernel::default(),
            initrd: None,
            rootfs: GuestRootfs::default(),
            network: NetworkSpec::default(),
            shares: vec![],
            cmdline: default_cmdline(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_lisp::{domain::TataraDomain, read};

    #[test]
    fn minimal_defvm_parses() {
        // Top-level kwargs use kebab-case (derive macro convention);
        // nested serde types use snake_case inside their attr-sets.
        let forms = read(
            r#"(defvm
                 :name       "plex-guest"
                 :cpus       2
                 :memory-mib 2048)"#,
        )
        .unwrap();
        let v = VmSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(v.name, "plex-guest");
        assert_eq!(v.cpus, 2);
        assert_eq!(v.memory_mib, 2048);
    }

    #[test]
    fn full_defvm_parses_with_shares_and_network() {
        let forms = read(
            r#"(defvm
                 :name       "plex-guest"
                 :cpus       4
                 :memory-mib 4096
                 :hypervisor (:kind "Vfkit")
                 :kernel     (:kind "Bridge" :attr_path "linuxPackages.kernel")
                 :rootfs     (:kind "Bridge" :attr_path "nixpkgs-images.minimal-rootfs")
                 :network    (:kind "Nat" :subnet "10.200.0.0/24")
                 :shares     ((:host "/Users/drzzln/code" :guest "/mnt/code" :read_only #f))
                 :cmdline    ("console=hvc0" "init=/bin/tatara-init"))"#,
        )
        .unwrap();
        let v = VmSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(v.cpus, 4);
        assert_eq!(v.memory_mib, 4096);
        assert_eq!(v.shares.len(), 1);
        assert_eq!(v.shares[0].host, "/Users/drzzln/code");
        assert_eq!(v.shares[0].guest, "/mnt/code");
        match v.network.kind {
            NetworkKind::Nat => (),
            _ => panic!("expected Nat"),
        }
    }

    #[test]
    fn defaults_are_darwin_friendly() {
        let v = VmSpec::plex_default("plex");
        assert!(matches!(v.hypervisor, Hypervisor::Vfkit));
        assert!(v.cmdline.iter().any(|a| a.contains("tatara-init")));
    }
}
