//! wasmi backend. Compiled in with `runtime-wasmi`.
//!
//! wasmi is a pure-Rust WebAssembly interpreter with no `unsafe` and
//! no external deps. Ideal for embedded targets and sandboxed hosts
//! where JIT is disallowed.
//!
//! Today: supports no-imports modules — compile, instantiate, call
//! `_start` (returns nothing, treated as exit 0). WASI bridge lives in
//! a follow-on phase; modules that require WASI imports surface a
//! clean instantiate error.

#![cfg(feature = "runtime-wasmi")]

use std::time::Duration;

use wasmi::{Engine, Linker, Module, Store};

use crate::engine::{WasmBoot, WasmEngine, WasmEngineError, WasmHandle, WasmModuleSource};
use crate::{WasiPreview, WasmRuntime};

pub struct WasmiEngine {
    engine: Engine,
}

impl Default for WasmiEngine {
    fn default() -> Self {
        Self {
            engine: Engine::default(),
        }
    }
}

impl WasmEngine for WasmiEngine {
    fn runtime(&self) -> WasmRuntime {
        WasmRuntime::Wasmi
    }

    fn run(&self, boot: &WasmBoot) -> Result<WasmHandle, WasmEngineError> {
        if boot.preview == WasiPreview::P2 {
            return Err(WasmEngineError::PreviewNotSupported(WasiPreview::P2));
        }
        let bytes = read_module(&boot.module)?;
        let module = Module::new(&self.engine, &bytes[..])
            .map_err(|e| WasmEngineError::Compile(e.to_string()))?;

        let mut store = Store::new(&self.engine, ());
        let linker: Linker<()> = Linker::new(&self.engine);

        let pre = linker
            .instantiate(&mut store, &module)
            .map_err(|e| WasmEngineError::Instantiate(e.to_string()))?;
        let instance = pre
            .start(&mut store)
            .map_err(|e| WasmEngineError::Instantiate(e.to_string()))?;

        let start = instance
            .get_typed_func::<(), ()>(&store, "_start")
            .map_err(|e| WasmEngineError::Instantiate(format!("no _start: {e}")))?;

        let call_result = start.call(&mut store, ());

        let mut handle = WasmHandle::new(&boot.name, WasmRuntime::Wasmi);
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
