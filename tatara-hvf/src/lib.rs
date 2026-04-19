//! `tatara-hvf` — Apple Hypervisor.framework backend for tatara guests.
//!
//! The **bare-metal** VM engine. Direct `hv_vm_*` / `hv_vcpu_*` calls
//! via the `applevisor` safe-wrapper crate, own virtio device backends
//! (landing H.2.2), raw vCPU loop with MMIO callbacks.
//!
//! # Layering
//!
//! ```text
//!   tatara-hvf (this crate)
//!       │  uses
//!       ▼
//!   applevisor (safe wrapper — Quarkslab)
//!       │  links
//!       ▼
//!   Hypervisor.framework  (aarch64-darwin only)
//! ```
//!
//! # Cargo / build
//!
//! The `applevisor` dep is target-gated to `aarch64-darwin`. On any
//! other target, the crate compiles but `HvfEngine::new()` returns
//! `HvfError::UnsupportedPlatform`. This keeps `cargo check
//! --workspace` clean on Linux CI while the real engine is only
//! exercised on M-series Macs.
//!
//! # Entitlements
//!
//! Running any HVF binary requires the `com.apple.security.hypervisor`
//! entitlement. For `cargo test`, either codesign the test binary
//! with an entitlements plist or run tests as root. Currently:
//!
//! ```bash
//! codesign --entitlements entitlements.plist --sign - \
//!   target/debug/deps/tatara_hvf-*
//! ```
//!
//! A follow-up CI helper crate will encapsulate this; for now the
//! `vm_create_is_supported` test is `#[ignore]` by default so `cargo
//! test` doesn't fail when entitlements aren't set.

#![forbid(unsafe_code)]

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub mod hvf_darwin;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub use hvf_darwin::HvfEngine;

#[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]
pub mod stub;

#[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]
pub use stub::HvfEngine;

use thiserror::Error;

/// Errors from the HVF backend.
#[derive(Debug, Error)]
pub enum HvfError {
    #[error("HVF is only available on aarch64-darwin; this target cannot run the real backend")]
    UnsupportedPlatform,

    #[error("hv_vm_create failed: {0}")]
    VmCreate(String),

    #[error("memory map failed: {0}")]
    MemoryMap(String),

    #[error("vcpu create failed: {0}")]
    VCpuCreate(String),

    #[error("vcpu run failed: {0}")]
    VCpuRun(String),

    #[error("register access failed: {0}")]
    Register(String),

    #[error("entitlement missing — requires com.apple.security.hypervisor")]
    MissingEntitlement,

    #[error("applevisor: {0}")]
    Applevisor(String),
}

/// A guest memory region mapped into the VM's physical address space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestRegion {
    pub guest_phys_addr: u64,
    pub size_bytes: usize,
    pub permissions: Permissions,
}

/// Page permissions for a mapped region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Permissions {
    #[must_use]
    pub const fn rwx() -> Self {
        Self {
            read: true,
            write: true,
            execute: true,
        }
    }

    #[must_use]
    pub const fn rx() -> Self {
        Self {
            read: true,
            write: false,
            execute: true,
        }
    }

    #[must_use]
    pub const fn rw() -> Self {
        Self {
            read: true,
            write: true,
            execute: false,
        }
    }
}

/// Phase marker.
pub const CRATE_STATUS: &str = "phase-h2.1";
