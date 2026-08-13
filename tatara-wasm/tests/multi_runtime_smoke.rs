//! Cross-runtime smoke: the same no-imports WAT module instantiates +
//! runs cleanly through every Rust-native runtime we ship. Proves the
//! `WasmEngine` trait is polymorphic.
//!
//! WASI-requiring guests (like the hello-world test in wasmtime_hello.rs)
//! only go through wasmtime today; other runtimes get WASI bridges in
//! a follow-on phase. This test uses a pure-compute module so every
//! engine can handle it.

#![cfg(all(
    feature = "runtime-wasmtime",
    feature = "runtime-wasmer",
    feature = "runtime-wasmi"
))]

use tatara_wasm::{
    engine_for, facts_of, WasiPreview, WasmBoot, WasmCapabilities, WasmFeatures, WasmModuleSource,
    WasmRuntime,
};

/// Pure-compute module: no imports, `_start` is an empty function that
/// returns cleanly. Every engine with a `_start` export convention
/// handles this.
const NO_IMPORTS_WAT: &str = r#"
(module
  (func $start (export "_start")
    nop))
"#;

fn run_on(runtime: WasmRuntime) -> (WasmRuntime, Option<i32>) {
    let engine = engine_for(runtime).unwrap_or_else(|e| panic!("{runtime:?}: {e:?}"));
    let boot = WasmBoot {
        module: WasmModuleSource::Wat(NO_IMPORTS_WAT.to_string()),
        runtime,
        preview: WasiPreview::P1,
        features: WasmFeatures::default(),
        // A pure-compute module imports nothing, so deny-all is exactly
        // right — and it proves the closed default is usable, not merely safe.
        capabilities: WasmCapabilities::closed(),
        name: format!("{runtime:?}-smoke"),
    };
    let handle = engine
        .run(&boot)
        .unwrap_or_else(|e| panic!("{runtime:?}: {e:?}"));
    (handle.runtime, handle.exit_code)
}

/// Every engine must refuse a WASI grant it cannot honour, rather than
/// accepting the declaration and silently ignoring it. wasmi and wasmer wire
/// no WASI context; `facts_of` is what says so, and this proves the facts
/// table is actually consulted on the real `run` path rather than only in
/// unit tests.
///
/// RED RUN (2026-08-13): the `provides_wasi` clause deleted from `check_boot`
/// → `Wasmi accepted a WASI grant it cannot honour`.
#[test]
fn engines_without_wasi_refuse_a_wasi_grant() {
    for runtime in [WasmRuntime::Wasmer, WasmRuntime::Wasmi] {
        assert!(
            !facts_of(runtime).provides_wasi,
            "{runtime:?} is claimed to provide WASI; this test targets the ones that do not"
        );
        let engine = engine_for(runtime).unwrap_or_else(|e| panic!("{runtime:?}: {e:?}"));
        let boot = WasmBoot {
            module: WasmModuleSource::Wat(NO_IMPORTS_WAT.to_string()),
            runtime,
            preview: WasiPreview::P1,
            features: WasmFeatures::default(),
            capabilities: WasmCapabilities::stdio(),
            name: format!("{runtime:?}-wasi-grant"),
        };
        let err = engine
            .run(&boot)
            .expect_err("{runtime:?} accepted a WASI grant it cannot honour");
        let msg = format!("{err:?}");
        assert!(msg.contains("WasiNotProvided"), "{runtime:?}: got {msg}");
    }
}

#[test]
fn all_three_rust_runtimes_agree() {
    for runtime in [
        WasmRuntime::Wasmtime,
        WasmRuntime::Wasmer,
        WasmRuntime::Wasmi,
    ] {
        let (observed_runtime, exit) = run_on(runtime);
        assert_eq!(observed_runtime, runtime);
        assert_eq!(exit, Some(0), "{runtime:?} failed to exit cleanly");
    }
}

#[test]
fn wasmedge_and_wamr_still_report_not_compiled() {
    // Those two ship in H.4 follow-up — they need C++/C SDK linking.
    for runtime in [WasmRuntime::WasmEdge, WasmRuntime::Wamr] {
        match engine_for(runtime) {
            Ok(_) => panic!("{runtime:?} should not be available yet"),
            Err(e) => {
                let msg = format!("{e:?}");
                assert!(msg.contains("RuntimeNotCompiled"), "got {msg} for {runtime:?}");
            }
        }
    }
}
