//! Bridge: the tatara-lisp `(defvm …)` envelope (`VmSpec`) → the runtime
//! `maquina_engine::VmShape` that a `MaquinaEngine` programs. The
//! authoring→runtime seam of the máquina primitive (see
//! `theory/MAQUINA.md`).
//!
//! The orphan rule forbids `impl From<VmSpec> for maquina_engine::VmShape`
//! (both `From` and `VmShape` are foreign to this crate), so the projection
//! is an inherent method on the local `VmSpec`.
//!
//! `BootClosure` is intentionally **not** derivable here: its kernel / initrd
//! / rootfs are realized `/nix/store` paths produced by sui + tatara-nix from
//! the `(defsystem …)` value (see `boot.rs`). This module carries only the
//! envelope; the closure is attached at realization time.

use std::path::PathBuf;

use maquina_engine::{NetworkMode, ShareMount, VmShape};

use crate::config::{NetworkKind, VmSpec};

impl VmSpec {
    /// Project this `(defvm …)` envelope onto the backend-neutral
    /// [`maquina_engine::VmShape`].
    ///
    /// `VmSpec` carries no MAC field yet; the engine derives a deterministic
    /// MAC from the máquina id (`kasou::deterministic_mac`), so `mac` is left
    /// `None` here.
    #[must_use]
    pub fn to_maquina_shape(&self) -> VmShape {
        VmShape {
            cpus: self.cpus,
            memory_mib: self.memory_mib,
            mac: None,
            network: match self.network.kind {
                NetworkKind::Nat => NetworkMode::Nat,
                NetworkKind::Bridge => NetworkMode::Bridge {
                    interface: self.network.host_interface.clone().unwrap_or_default(),
                },
                NetworkKind::None => NetworkMode::None,
            },
            shares: self
                .shares
                .iter()
                .map(|s| ShareMount {
                    host: PathBuf::from(&s.host),
                    guest: PathBuf::from(&s.guest),
                    read_only: s.read_only,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::VmSpec;
    use crate::config::{NetworkKind, NetworkSpec, ShareSpec};
    use maquina_engine::NetworkMode;

    #[test]
    fn defvm_projects_onto_vmshape() {
        let vm = VmSpec::plex_default("plex");
        let shape = vm.to_maquina_shape();
        assert_eq!(shape.cpus, vm.cpus);
        assert_eq!(shape.memory_mib, vm.memory_mib);
        assert_eq!(shape.network, NetworkMode::Nat);
        assert!(shape.mac.is_none());
    }

    #[test]
    fn bridge_network_and_shares_carry_over() {
        let mut vm = VmSpec::plex_default("plex");
        vm.network = NetworkSpec {
            kind: NetworkKind::Bridge,
            subnet: None,
            host_interface: Some("en0".into()),
        };
        vm.shares = vec![ShareSpec {
            host: "/Users/drzzln/code".into(),
            guest: "/mnt/code".into(),
            read_only: false,
        }];
        let shape = vm.to_maquina_shape();
        assert_eq!(
            shape.network,
            NetworkMode::Bridge {
                interface: "en0".into()
            }
        );
        assert_eq!(shape.shares.len(), 1);
        assert_eq!(shape.shares[0].guest, std::path::PathBuf::from("/mnt/code"));
    }
}
