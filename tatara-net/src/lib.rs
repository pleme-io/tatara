//! tatara-net: Cross-platform networking plane for the tatara cluster fabric.
//!
//! The complete sandwich:
//! ```text
//! ┌─ WASI (wasmtime) ──── Portable, sandboxed workloads ─────────┐
//! ├─ Tatara Runtime ───── Convergence, scheduling, Raft ─────────┤
//! ├─ eBPF (aya) ────────── Kernel hooks, pure Rust ──────────────┤
//! ├─ WireGuard (mamorigami) ─ Encrypted mesh fabric ─────────────┤
//! └─ Hardware ──────────── Abstracted away ──────────────────────┘
//! ```
//!
//! On Linux: eBPF programs (XDP/TC/cgroup) for kernel-level networking.
//! On macOS: userspace networking via tun-rs + smoltcp + hanabi proxy.
//! Everywhere: WireGuard mesh via mamorigami for encrypted inter-node traffic.

pub mod config;
pub mod convergence;
pub mod mesh;
pub mod observability;
pub mod platform;
pub mod policy;
pub mod routing;
pub mod traits;
pub mod types;

#[cfg(feature = "wasi")]
pub mod wasi;

pub use config::NetConfig;
pub use traits::NetworkPlane;
pub use types::*;
