//! `tatara-hvf` — Apple Hypervisor.framework backend for tatara guests.
//!
//! This is the **bare-metal** VM engine: direct `hv_vm_*` / `hv_vcpu_*`
//! calls against Hypervisor.framework, own virtio-blk / virtio-net /
//! virtio-9p backends, raw vCPU loop with MMIO callbacks. Target of
//! Phase H.2 per `docs/declarative-guests.md`.
//!
//! # Status
//!
//! **Phase H.1 stub.** This crate reserves the workspace slot and
//! documents the shape. No `hv_*` calls yet — the build-time deps
//! (applevisor-sys / objc2 / virtio-bindings / vm-memory) are commented
//! out in `Cargo.toml` and uncommented in H.2.
//!
//! # Relationship to kasou
//!
//! `kasou` is the Virtualization.framework (VZ) backend. It's the
//! fallback. `tatara-hvf` is the primary — used when the operator
//! wants full vCPU + MMIO control, pluggable virtio device backends,
//! no extra process overhead.
//!
//! # The GuestEngine trait
//!
//! `tatara-hvf` will implement `GuestEngine` from
//! `tatara_vm::engine`. See `docs/declarative-guests.md` §4.
//!
//! ```ignore
//! impl GuestEngine for HvfEngine {
//!     type Handle = HvfVm;
//!     fn boot(&self, spec: &GuestSpec, artifacts: &GuestArtifacts) -> Result<HvfVm> { … }
//!     fn shutdown(&self, h: &HvfVm, grace: Duration) -> Result<()> { … }
//!     fn pause(&self, h: &HvfVm) -> Result<()> { … }
//!     fn resume(&self, h: &HvfVm) -> Result<()> { … }
//!     fn status(&self, h: &HvfVm) -> GuestStatus { … }
//! }
//! ```

#![forbid(unsafe_code)] // Unsafe FFI lands in H.2 behind a narrow, audited boundary.

/// Phase H.1 placeholder. Replaced in H.2.
pub const CRATE_STATUS: &str = "phase-h1-stub";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_marker_present() {
        assert_eq!(CRATE_STATUS, "phase-h1-stub");
    }
}
