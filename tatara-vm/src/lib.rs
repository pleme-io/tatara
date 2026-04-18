//! # tatara-vm — Darwin-hosted Linux guests for tatara-os
//!
//! Typed `VmSpec` authored in tatara-lisp, emitted as config for whichever
//! hypervisor the host has available:
//!
//!   - **`vfkit`** — Apple Virtualization.framework, minimal CLI, JSON config.
//!     Default on Darwin. Installed via nixpkgs' `vfkit` attribute.
//!   - **`qemu`** — portable fallback; KVM on Linux, HVF on Darwin (intel).
//!   - **`kasou`** — pleme-io's own Virtualization.framework wrapper. Used when
//!     we want Rust-native control, no CLI shell-out.
//!
//! The Nix story: the VM's **kernel** + **initrd** + **rootfs** are tatara
//! `Derivation` values. That means every guest is content-addressed, and two
//! identical `VmSpec` values produce the same bytes in `/nix/store`. Darwin
//! hosts the hypervisor; Nix (on Darwin) builds every guest artifact.
//!
//! Lisp authoring:
//!
//! ```lisp
//! (defvm plex-guest
//!   :cpus     4
//!   :memory-mib 4096
//!   :hypervisor (:kind Vfkit)
//!   :kernel   (:bridge "linuxPackages.kernel")
//!   :initrd   (:bridge "tatara.initrd")
//!   :rootfs   (:system plex)
//!   :network  ((:kind Nat :subnet "10.200.0.0/24"))
//!   :shares   ((:host "/Users/drzzln/code" :guest "/mnt/code"))
//!   :cmdline  ("console=hvc0" "init=/bin/tatara-init"))
//! ```

pub mod config;
pub mod vfkit;

pub use config::{
    GuestKernel, GuestRootfs, Hypervisor, NetworkSpec, NetworkKind, ShareSpec, VmSpec,
};
pub use vfkit::{VfkitEmitter, VfkitJson};
