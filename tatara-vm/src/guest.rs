//! `GuestSpec` — the superset of Linux-VM and WASM guests.
//!
//! Two dispatches:
//!
//!   `(defguest :kind (:vm …) …)`   → `GuestKind::Vm(VmSpec)`
//!   `(defguest :kind (:wasm …) …)` → `GuestKind::Wasm(WasmSpec)`
//!
//! Both share `build`, `build_on`, `network`, `mounts`, `services`,
//! `cmdline`, `resources`. The kind-specific block carries backend +
//! artifact-shape fields (kernel/initrd/rootfs for VMs; component +
//! runtime for WASM).
//!
//! See `tatara/docs/declarative-guests.md` for the full design.

use serde::{Deserialize, Serialize};
use tatara_build_remote::{BuildRef, BuildTransportChain};
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;
use tatara_wasm::{WasiPreview, WasmFeatures, WasmRuntime};

use crate::config::{NetworkSpec, ShareSpec, VmSpec};

/// `(defguest …)` — the authoritative guest spec.
#[derive(DeriveTataraDomain, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defguest")]
pub struct GuestSpec {
    pub name: String,

    /// Linux VM or WASM module. Discriminated on the `:kind` keyword.
    pub kind: GuestKind,

    /// Default command-line — currently only used by VMs.
    #[serde(default)]
    pub cmdline: Vec<String>,

    /// Shared attachments applicable to both kinds.
    #[serde(default)]
    pub network: NetworkSpec,

    #[serde(default)]
    pub shares: Vec<ShareSpec>,

    /// Resource caps honored by whichever backend hosts the guest.
    #[serde(default)]
    pub resources: ResourceLimits,

    /// Where the guest's artifacts are built — Attic / ssh-ng / local.
    /// Defaults to `BuildTransportChain::quero_lol()`.
    #[serde(default = "default_build_on")]
    pub build_on: BuildTransportChain,
}

fn default_build_on() -> BuildTransportChain {
    BuildTransportChain::quero_lol()
}

/// `:kind` — which runtime family hosts this guest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum GuestKind {
    /// Linux VM on HVF (primary) or VZ (fallback). Wraps existing `VmSpec`.
    Vm(VmSpec),

    /// WASI/WASM component on one of five runtimes.
    Wasm(WasmSpec),
}

/// `:wasm` guest — runtime + component + WASI contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmSpec {
    /// Which WASM runtime will host this guest.
    #[serde(default)]
    pub runtime: WasmRuntime,

    /// WASI version.
    #[serde(default)]
    pub wasi_preview: WasiPreview,

    /// The component artifact — a WASM/WAT/Component Model blob produced
    /// by a Nix derivation.
    pub component: BuildRef,

    /// AOT/JIT/SIMD/wasi-http/wasi-nn toggles.
    #[serde(default)]
    pub features: WasmFeatures,
}

/// Shared resource caps — both kinds honor these.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLimits {
    /// Max RSS (MiB). `None` = unlimited (subject to host).
    #[serde(default)]
    pub memory_mib: Option<u32>,

    /// CPU budget hint — informational for VMs, enforced for WASM.
    #[serde(default)]
    pub cpu_ms_budget: Option<u32>,

    /// Max file descriptors.
    #[serde(default)]
    pub fd_limit: Option<u32>,
}

impl GuestSpec {
    /// Wrap an existing `VmSpec` as a Guest — the compatibility shim.
    #[must_use]
    pub fn from_vm(spec: VmSpec) -> Self {
        Self {
            name: spec.name.clone(),
            cmdline: spec.cmdline.clone(),
            network: spec.network.clone(),
            shares: spec.shares.clone(),
            kind: GuestKind::Vm(spec),
            resources: ResourceLimits::default(),
            build_on: default_build_on(),
        }
    }

    /// Is this a VM?
    #[must_use]
    pub const fn is_vm(&self) -> bool {
        matches!(self.kind, GuestKind::Vm(_))
    }

    /// Is this a WASM module?
    #[must_use]
    pub const fn is_wasm(&self) -> bool {
        matches!(self.kind, GuestKind::Wasm(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Hypervisor;

    #[test]
    fn from_vm_wraps_cleanly() {
        let vm = VmSpec::plex_default("plex");
        let g = GuestSpec::from_vm(vm.clone());
        assert_eq!(g.name, "plex");
        assert!(g.is_vm());
        assert!(!g.is_wasm());
        match g.kind {
            GuestKind::Vm(inner) => assert_eq!(inner.hypervisor, Hypervisor::Vfkit),
            _ => panic!(),
        }
    }

    #[test]
    fn wasm_guest_serializes_with_runtime_variant() {
        let g = GuestSpec {
            name: "fast-fn".into(),
            kind: GuestKind::Wasm(WasmSpec {
                runtime: WasmRuntime::Wasmtime,
                wasi_preview: WasiPreview::P2,
                component: BuildRef::Flake {
                    url: "github:pleme-io/cors-proxy".into(),
                    attr: "wasi".into(),
                },
                features: WasmFeatures {
                    simd: true,
                    wasi_http: true,
                    ..Default::default()
                },
            }),
            cmdline: vec![],
            network: NetworkSpec::default(),
            shares: vec![],
            resources: ResourceLimits {
                memory_mib: Some(64),
                cpu_ms_budget: Some(100),
                fd_limit: None,
            },
            build_on: BuildTransportChain::quero_lol(),
        };
        let j = serde_json::to_string(&g).unwrap();
        assert!(j.contains("\"kind\":\"wasm\""));
        assert!(j.contains("\"wasmtime\""));
        assert!(j.contains("\"simd\":true"));

        let back: GuestSpec = serde_json::from_str(&j).unwrap();
        assert_eq!(back, g);
    }

    #[test]
    fn default_build_on_targets_quero_lol() {
        let d = default_build_on();
        assert_eq!(d.attic.as_deref(), Some("quero.lol"));
        assert_eq!(d.remote.as_deref(), Some("ssh://builder.quero.lol"));
        assert!(d.local);
    }

    #[test]
    fn defguest_vm_compiles_from_lisp() {
        use tatara_lisp::domain::TataraDomain;
        use tatara_lisp::reader;

        // Minimal — all-kwargs. VmSpec has rename_all="camelCase" so its
        // own fields are camel; nested GuestKernel/GuestRootfs don't, so
        // their fields stay snake_case in the JSON bridge.
        let src = r#"(defguest :name "sample"
                               :kind (:kind "vm"
                                      :name "sample"
                                      :cpus 4
                                      :memoryMib 4096
                                      :hypervisor (:kind "Vfkit")
                                      :kernel (:kind "Bridge" :attr_path "linuxPackages.kernel")
                                      :rootfs (:kind "Bridge" :attr_path "minimal")
                                      :network (:kind "Nat")
                                      :cmdline ("console=hvc0" "init=/bin/tatara-init"))
                               :cmdline ())"#;
        let forms = reader::read(src).expect("read");
        let args = &forms[0].as_list().unwrap()[1..];
        let guest = GuestSpec::compile_from_args(args).expect("compile defguest");
        assert_eq!(guest.name, "sample");
        assert!(guest.is_vm());
    }

    #[test]
    fn defguest_wasm_compiles_from_lisp() {
        use tatara_lisp::domain::TataraDomain;
        use tatara_lisp::reader;

        let src = r#"(defguest :name "fast-fn"
                               :kind (:kind "wasm"
                                      :runtime "wasmtime"
                                      :wasiPreview "p2"
                                      :component (:kind "flake"
                                                  :value (:url "github:pleme-io/cors-proxy"
                                                          :attr "wasi"))
                                      :features (:simd #t :wasiHttp #t))
                               :cmdline ())"#;
        let forms = reader::read(src).expect("read");
        let args = &forms[0].as_list().unwrap()[1..];
        let guest = GuestSpec::compile_from_args(args).expect("compile defguest wasm");
        assert_eq!(guest.name, "fast-fn");
        assert!(guest.is_wasm());
        match &guest.kind {
            GuestKind::Wasm(w) => {
                assert_eq!(w.runtime, WasmRuntime::Wasmtime);
                assert_eq!(w.wasi_preview, WasiPreview::P2);
                assert!(w.features.simd);
                assert!(w.features.wasi_http);
            }
            _ => panic!(),
        }
    }
}
