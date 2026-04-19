//! `tatara-wasm` — multi-runtime WASI/WASM host for tatara guests.
//!
//! Five runtimes, one trait, Cargo feature flags. Consumers pick the
//! runtime per workload via the Lisp surface:
//!
//! ```lisp
//! (defguest fast-fn   :kind (:wasm :runtime :wasmtime …))
//! (defguest k8s-fn    :kind (:wasm :runtime :wasmedge …))
//! (defguest embed-fn  :kind (:wasm :runtime :wamr   :features (:aot #t :no-std #t)))
//! ```
//!
//! The runtime axis is first-class — all five are shipped with
//! production-grade implementations in Phase H.3 (wasmtime) and
//! H.4 (the rest). See `docs/declarative-guests.md`.
//!
//! # Status
//!
//! **Phase H.1 stub.** The `WasmRuntime` enum + `WasmEngine` trait
//! land here now so `tatara-vm` can reference them. Runtime bodies
//! are empty until H.3/H.4.

#![forbid(unsafe_code)]

pub mod engine;

#[cfg(feature = "runtime-wasmtime")]
pub mod wasmtime_impl;

pub use engine::{
    engine_for, WasmBoot, WasmEngine, WasmEngineError, WasmHandle, WasmModuleSource,
};

use serde::{Deserialize, Serialize};

/// Which WASM runtime hosts this guest.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum WasmRuntime {
    /// Bytecode Alliance, Rust-native, WASI Preview 2 reference impl.
    Wasmtime,
    /// CNCF project, C++, strong K8s story.
    WasmEdge,
    /// Rust-native, multi-target embedding.
    Wasmer,
    /// Pure-Rust interpreter. Small, embedded, no-std-friendly.
    Wasmi,
    /// WebAssembly Micro Runtime — AOT/JIT, tiny footprint.
    Wamr,
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::Wasmtime
    }
}

/// WASI version the guest expects.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WasiPreview {
    /// WASI Preview 1 — long-standing compatibility target.
    P1,
    /// WASI Preview 2 — component model, default for new components.
    #[default]
    P2,
}

/// Feature toggles the runtime must honor.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WasmFeatures {
    #[serde(default)]
    pub aot: bool,
    #[serde(default)]
    pub jit: bool,
    #[serde(default)]
    pub threads: bool,
    #[serde(default)]
    pub simd: bool,
    #[serde(default)]
    pub no_std: bool,
    #[serde(default)]
    pub wasi_nn: bool,
    #[serde(default)]
    pub wasi_http: bool,
}

/// Phase H.1 placeholder. Replaced in H.3 / H.4 with the real engine
/// trait wired to each runtime.
pub const CRATE_STATUS: &str = "phase-h1-stub";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_runtime_is_wasmtime() {
        assert_eq!(WasmRuntime::default(), WasmRuntime::Wasmtime);
    }

    #[test]
    fn default_preview_is_p2() {
        assert_eq!(WasiPreview::default(), WasiPreview::P2);
    }

    #[test]
    fn features_round_trip_json() {
        let f = WasmFeatures {
            aot: true,
            simd: true,
            wasi_http: true,
            ..Default::default()
        };
        let j = serde_json::to_string(&f).unwrap();
        let back: WasmFeatures = serde_json::from_str(&j).unwrap();
        assert_eq!(f, back);
    }

    #[test]
    fn runtime_kebab_serialization() {
        assert_eq!(
            serde_json::to_string(&WasmRuntime::WasmEdge).unwrap(),
            "\"wasm-edge\""
        );
        assert_eq!(
            serde_json::to_string(&WasmRuntime::Wamr).unwrap(),
            "\"wamr\""
        );
    }
}
