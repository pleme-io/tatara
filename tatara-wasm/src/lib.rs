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
//! The runtime axis is first-class. See `docs/declarative-guests.md`.
//!
//! # Status — three of five, and read it from [`engine::facts_of`]
//!
//! This block used to say two contradictory things ten lines apart: the
//! header claimed *"all five are shipped with production-grade
//! implementations"* while the Status block called the crate a *"Phase H.1
//! stub"* whose *"runtime bodies are empty"*. **Both were wrong, in opposite
//! directions**, and a reader got whichever one they scrolled to. Corrected
//! 2026-08-13 against the tree.
//!
//! What is true: **three of the five runtimes have bodies** — wasmtime
//! (WASI preview 1, real `WasiCtxBuilder`), wasmer and wasmi (compile +
//! instantiate + `_start`, no WASI bridge). `WasmEdge` and `Wamr` have no
//! implementation in this crate and [`engine_for`] returns
//! `RuntimeNotCompiled` for both.
//!
//! Prose is the wrong place to keep that, which is why it is also **data**:
//! [`engine::facts_of`] is one exhaustive `match` recording what each runtime
//! implements, which previews it accepts and whether it wires WASI, and
//! [`CRATE_STATUS`] is gated against it so this paragraph cannot drift from
//! the code again.

#![forbid(unsafe_code)]

pub mod engine;

#[cfg(feature = "runtime-wasmtime")]
pub mod wasmtime_impl;

#[cfg(feature = "runtime-wasmer")]
pub mod wasmer_impl;

#[cfg(feature = "runtime-wasmi")]
pub mod wasmi_impl;

pub use engine::{
    check_boot, check_imports_granted, engine_for, facts_of, implemented_runtimes, read_module,
    EngineFacts, HostImport, Preopen, WasiGrant, WasmBoot, WasmCapabilities, WasmEngine,
    WasmEngineError, WasmHandle, WasmModuleSource, ALL_RUNTIMES, WASI_P1_MODULE,
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
///
/// **The default is `P1`, and that is a fix rather than a preference.** It was
/// `P2` — the one arm *no* engine in this crate implements — so a `WasmBoot`
/// built from defaults failed on every backend at run time. The trap was live,
/// not theoretical: `tatara_vm::WasmSpec::wasi_preview` carries
/// `#[serde(default)]`, so a `(defguest …)` that simply omitted the preview
/// deserialized straight into the unbootable arm.
///
/// The invariant, not the constant, is what
/// `engine::tests::default_preview_is_accepted_by_every_implemented_engine`
/// gates: whatever this default is, every implemented engine must accept it.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WasiPreview {
    /// WASI Preview 1 — long-standing compatibility target, and the only
    /// preview any engine here implements today.
    #[default]
    P1,
    /// WASI Preview 2 — component model. **No engine in this crate accepts
    /// this yet**; `engine::facts_of` is the authority on which do.
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

/// The crate's maturity, as one string a consumer can log.
///
/// It read `"phase-h1-stub"` while three engines had working bodies — a public
/// const that told every consumer something false. Kept rather than removed
/// (deleting a `pub const` is a breaking change for a published crate, and the
/// name is not the defect), corrected, and now **gated**: the count in it is
/// checked against [`implemented_runtimes`], so it cannot go stale the way the
/// module header did.
pub const CRATE_STATUS: &str = "3-of-5-runtimes-implemented";

/// The count [`CRATE_STATUS`] claims, parsed back out of it.
///
/// Deliberately re-derived from the string rather than formatted into it: the
/// gate has to read what a consumer reads, or it is checking its own input.
#[cfg(test)]
#[must_use]
fn claimed_runtime_count() -> Option<usize> {
    CRATE_STATUS.split('-').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_runtime_is_wasmtime() {
        assert_eq!(WasmRuntime::default(), WasmRuntime::Wasmtime);
    }

    /// Was `default_preview_is_p2`, which pinned the defect: it asserted the
    /// default was the one arm every engine rejected, and passed for exactly
    /// as long as the bug existed. The replacement is strictly stronger — it
    /// pins the *bootability* of the default rather than its identity, and
    /// `engine::tests::default_preview_is_accepted_by_every_implemented_engine`
    /// carries the invariant across every engine.
    ///
    /// RED RUN (2026-08-13): `#[default]` moved back to `P2` → this fails with
    /// `the default preview must be one an engine implements`.
    #[test]
    fn default_preview_is_one_an_engine_implements() {
        let default = WasiPreview::default();
        assert!(
            facts_of(WasmRuntime::Wasmtime).accepts(default),
            "the default preview must be one an engine implements, \
             got {default:?}"
        );
    }

    /// `CRATE_STATUS` is prose-shaped, so tie its number to the table.
    ///
    /// RED RUN (2026-08-13): `CRATE_STATUS` set to `"5-of-5-runtimes-implemented"`
    /// → `CRATE_STATUS claims 5 implemented runtimes, facts_of has 3`.
    #[test]
    fn crate_status_matches_the_facts_table() {
        let claimed = claimed_runtime_count().expect("CRATE_STATUS must start with a count");
        let actual = implemented_runtimes().len();
        assert_eq!(
            claimed, actual,
            "CRATE_STATUS claims {claimed} implemented runtimes, facts_of has {actual}"
        );
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
