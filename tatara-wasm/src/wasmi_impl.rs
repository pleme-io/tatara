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

use crate::engine::{
    check_boot, check_imports_granted, read_module, WasmBoot, WasmEngine, WasmEngineError,
    WasmHandle,
};
use crate::WasmRuntime;

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
        check_boot(&self.facts(), boot)?;
        let bytes = read_module(&boot.module)?;
        let module = Module::new(&self.engine, &bytes[..])
            .map_err(|e| WasmEngineError::Compile(e.to_string()))?;

        // wasmi wires no WASI (see `engine::facts_of`), so the only imports
        // that could be granted here are host ones — and `check_boot` has
        // already refused a WASI grant this engine cannot honour.
        check_imports_granted(
            &boot.capabilities,
            module.imports().map(|i| (i.module(), i.name())),
        )?;

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

// `read_module` was a third copy of the same function; it is
// `engine::read_module` now.
