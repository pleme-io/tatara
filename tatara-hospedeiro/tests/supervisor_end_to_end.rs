//! End-to-end: `GuestSupervisor::boot_wasm_bytes` runs a real WASM
//! module through the engine trait, records a reaped guest, status
//! query reflects reality. Proves the H.6 dispatch layer works.

use tatara_hospedeiro::{GuestStatus, GuestSupervisor};
use tatara_vm::WasmSpec;
use tatara_wasm::{WasiPreview, WasmFeatures, WasmRuntime};
use tatara_build_remote::BuildRef;

/// Pure-compute WAT — no imports, _start returns nop.
const NOP_WAT: &str = r#"(module (func $s (export "_start") nop))"#;

#[test]
fn supervisor_runs_wasm_guest_end_to_end() {
    let bytes = wat::parse_str(NOP_WAT).expect("compile WAT");
    let spec = WasmSpec {
        runtime: WasmRuntime::Wasmtime,
        wasi_preview: WasiPreview::P1,
        component: BuildRef::StorePath("/dev/null".into()), // not used by _bytes variant
        features: WasmFeatures::default(),
    };

    let mut sup = GuestSupervisor::new();
    let status = sup
        .boot_wasm_bytes("smoke-guest", &spec, bytes)
        .expect("boot should succeed");
    assert_eq!(status, GuestStatus::Reaped);

    let record = sup.get("smoke-guest").expect("record must exist");
    assert_eq!(record.name, "smoke-guest");
    assert_eq!(record.status, GuestStatus::Reaped);
    assert_eq!(record.kind_tag, "wasm");
    assert_eq!(record.exit_code, Some(0));
    assert_eq!(sup.len(), 1);

    let removed = sup.remove("smoke-guest").expect("remove returns the record");
    assert_eq!(removed.name, "smoke-guest");
    assert!(sup.is_empty());
}

#[test]
fn supervisor_records_failed_guest_on_non_zero_exit() {
    // A WAT that _starts_ and then traps — engine returns a Run error,
    // NOT a non-zero exit. The supervisor surfaces that as an Err
    // because the engine reported it as `Run` failure. This documents
    // the current contract: supervisor.boot_wasm_bytes returns the
    // engine's Err; the caller decides recording policy.
    const TRAP_WAT: &str = r#"(module (func $s (export "_start") unreachable))"#;
    let bytes = wat::parse_str(TRAP_WAT).expect("compile WAT");
    let spec = WasmSpec {
        runtime: WasmRuntime::Wasmtime,
        wasi_preview: WasiPreview::P1,
        component: BuildRef::StorePath("/dev/null".into()),
        features: WasmFeatures::default(),
    };
    let mut sup = GuestSupervisor::new();
    let err = sup.boot_wasm_bytes("trap-guest", &spec, bytes);
    assert!(err.is_err(), "trap should error");
}

#[test]
fn multiple_guests_coexist_in_the_supervisor() {
    let bytes = wat::parse_str(NOP_WAT).expect("compile WAT");
    let spec = WasmSpec {
        runtime: WasmRuntime::Wasmtime,
        wasi_preview: WasiPreview::P1,
        component: BuildRef::StorePath("/dev/null".into()),
        features: WasmFeatures::default(),
    };
    let mut sup = GuestSupervisor::new();
    for i in 0..5 {
        sup.boot_wasm_bytes(format!("g{i}"), &spec, bytes.clone())
            .unwrap();
    }
    assert_eq!(sup.len(), 5);
    for i in 0..5 {
        let r = sup.get(&format!("g{i}")).unwrap();
        assert_eq!(r.status, GuestStatus::Reaped);
    }
}
