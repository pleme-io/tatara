//! End-to-end: a trivial WAT module runs through the wasmtime backend
//! via `WasmEngine::run`, captures stdout via WASI, asserts exit = 0.
//!
//! This is the H.3 correctness proof. Nothing here calls unsafe FFI
//! or touches the host filesystem — pure Rust + wasmtime.

#![cfg(feature = "runtime-wasmtime")]

use tatara_wasm::{
    engine_for, WasiPreview, WasmBoot, WasmFeatures, WasmModuleSource, WasmRuntime,
};

/// Minimal WASI p1 "hello world" in WebAssembly Text Format. Imports
/// `fd_write` from `wasi_snapshot_preview1`, writes 13 bytes of
/// "Hello, tatara\n" to fd 1 (stdout), then returns cleanly from
/// `_start`.
const HELLO_WAT: &str = r#"
(module
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))

  (memory 1)
  (export "memory" (memory 0))

  ;; The string body is at offset 16. iovec is at offset 0:
  ;;   iovec[0].buf_ptr = 16
  ;;   iovec[0].buf_len = 14
  (data (i32.const 0)  "\10\00\00\00") ;; buf_ptr  (16)
  (data (i32.const 4)  "\0e\00\00\00") ;; buf_len  (14)
  (data (i32.const 16) "Hello, tatara\n")

  (func $start (export "_start")
    (drop
      (call $fd_write
        (i32.const 1)   ;; fd = stdout
        (i32.const 0)   ;; iov base
        (i32.const 1)   ;; iov count
        (i32.const 32)  ;; nwritten out
      ))
  )
)
"#;

#[test]
fn hello_world_through_wasmtime() {
    let engine = engine_for(WasmRuntime::Wasmtime).expect("wasmtime feature compiled in");

    let boot = WasmBoot {
        module: WasmModuleSource::Wat(HELLO_WAT.to_string()),
        runtime: WasmRuntime::Wasmtime,
        preview: WasiPreview::P1,
        features: WasmFeatures::default(),
        name: "hello".into(),
    };

    let handle = engine.run(&boot).expect("run must succeed");
    assert_eq!(handle.runtime, WasmRuntime::Wasmtime);
    assert_eq!(handle.exit_code, Some(0), "exit 0 expected");
    assert_eq!(handle.stdout, "Hello, tatara\n");
    assert_eq!(handle.stderr, "");
}

#[test]
fn p2_is_rejected_on_wasmtime_p1_path() {
    let engine = engine_for(WasmRuntime::Wasmtime).expect("wasmtime feature compiled in");
    let boot = WasmBoot {
        module: WasmModuleSource::Wat(HELLO_WAT.to_string()),
        runtime: WasmRuntime::Wasmtime,
        preview: WasiPreview::P2,
        features: WasmFeatures::default(),
        name: "hello".into(),
    };
    let err = engine.run(&boot).unwrap_err();
    // Explicit error — not a silent degradation. Component Model lands later.
    let msg = format!("{err:?}");
    assert!(msg.contains("PreviewNotSupported"), "got {msg}");
}

#[test]
fn unknown_runtime_is_explicit_error() {
    match engine_for(WasmRuntime::WasmEdge) {
        Ok(_) => panic!("WasmEdge engine should not be available"),
        Err(e) => {
            let msg = format!("{e:?}");
            assert!(msg.contains("RuntimeNotCompiled"), "got {msg}");
        }
    }
}
