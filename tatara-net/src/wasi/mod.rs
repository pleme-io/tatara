//! WASI driver — runs WASM/WASI components as tatara workloads.
//!
//! Provides a sandboxed, portable execution environment using wasmtime.
//! WASM components are built by Nix (reproducible) and run with
//! capability-based security (only access what's explicitly granted).
//!
//! The complete sandwich:
//! - eBPF hooks the kernel boundary (below)
//! - WASI standardizes the userspace boundary (above)
//! - Together they make every system call observable and controllable

// WASI implementation requires the `wasi` feature flag.
// When enabled, provides WasiDriver implementing the Driver trait.
