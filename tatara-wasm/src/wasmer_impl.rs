//! Wasmer backend. Compiled in with `runtime-wasmer`.
//!
//! Today: no-imports module path — compile via Cranelift, instantiate,
//! call `_start`. WASI via wasmer-wasix lands alongside a per-runtime
//! WASI bridge layer in a follow-on. Matches the discipline applied
//! to the wasmi backend.

#![cfg(feature = "runtime-wasmer")]

use std::time::Duration;

use wasmer::{imports, Instance, Module, Store};

use crate::engine::{WasmBoot, WasmEngine, WasmEngineError, WasmHandle, WasmModuleSource};
use crate::{WasiPreview, WasmRuntime};

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
        if boot.preview == WasiPreview::P2 {
            return Err(WasmEngineError::PreviewNotSupported(WasiPreview::P2));
        }
        let bytes = read_module(&boot.module)?;

        let mut store = Store::default();
        let module = Module::new(&store, &bytes)
            .map_err(|e| WasmEngineError::Compile(e.to_string()))?;
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

fn read_module(src: &WasmModuleSource) -> Result<Vec<u8>, WasmEngineError> {
    match src {
        WasmModuleSource::Bytes(b) => Ok(b.clone()),
        WasmModuleSource::Wat(text) => wat::parse_str(text)
            .map_err(|e| WasmEngineError::Compile(format!("WAT parse: {e}"))),
        WasmModuleSource::Path(p) => std::fs::read(p)
            .map_err(|e| WasmEngineError::Io(format!("{}: {e}", p.display()))),
    }
}
