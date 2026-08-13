//! Wasmer backend. Compiled in with `runtime-wasmer`.
//!
//! Today: no-imports module path — compile via Cranelift, instantiate,
//! call `_start`. WASI via wasmer-wasix lands alongside a per-runtime
//! WASI bridge layer in a follow-on. Matches the discipline applied
//! to the wasmi backend.

#![cfg(feature = "runtime-wasmer")]

use std::time::Duration;

use wasmer::{imports, Instance, Module, Store};

use crate::engine::{
    check_boot, check_imports_granted, read_module, WasmBoot, WasmEngine, WasmEngineError,
    WasmHandle,
};
use crate::WasmRuntime;

pub struct WasmerEngine;

impl Default for WasmerEngine {
    fn default() -> Self {
        Self
    }
}

impl WasmEngine for WasmerEngine {
    fn runtime(&self) -> WasmRuntime {
        WasmRuntime::Wasmer
    }

    fn run(&self, boot: &WasmBoot) -> Result<WasmHandle, WasmEngineError> {
        check_boot(&self.facts(), boot)?;
        let bytes = read_module(&boot.module)?;

        let mut store = Store::default();
        let module = Module::new(&store, &bytes)
            .map_err(|e| WasmEngineError::Compile(e.to_string()))?;

        // As wasmi: no WASI bridge here, so a granted import can only be a
        // host one. The empty `imports!` below is now a *consequence* of the
        // declaration rather than a hardcoded posture.
        // wasmer's `ImportType` owns its strings, so these are cloned rather
        // than borrowed — the shared validator takes any `AsRef<str>` pair.
        check_imports_granted(
            &boot.capabilities,
            module
                .imports()
                .map(|i| (i.module().to_string(), i.name().to_string())),
        )?;

        let import_object = imports! {};
        let instance = Instance::new(&mut store, &module, &import_object)
            .map_err(|e| WasmEngineError::Instantiate(e.to_string()))?;

        let start = instance
            .exports
            .get_typed_function::<(), ()>(&store, "_start")
            .map_err(|e| WasmEngineError::Instantiate(format!("no _start: {e}")))?;

        let call_result = start.call(&mut store);

        let mut handle = WasmHandle::new(&boot.name, WasmRuntime::Wasmer);
        match call_result {
            Ok(()) => {
                handle.exit_code = Some(0);
                Ok(handle)
            }
            Err(e) => Err(WasmEngineError::Run(format!("{e:?}"))),
        }
    }

    fn shutdown(&self, _handle: &mut WasmHandle, _grace: Duration) -> Result<(), WasmEngineError> {
        Ok(())
    }
}

// `read_module` was the second copy of the same function; it is
// `engine::read_module` now.
