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
    engine_for, WasiPreview, WasmBoot, WasmFeatures, WasmModuleSource, WasmRuntime,
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
        name: format!("{runtime:?}-smoke"),
    };
    let handle = engine
        .run(&boot)
        .unwrap_or_else(|e| panic!("{runtime:?}: {e:?}"));
    (handle.runtime, handle.exit_code)
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
