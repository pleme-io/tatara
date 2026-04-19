//! wasmtime backend. Compiled in with `runtime-wasmtime`.
//!
//! Today: WASI Preview 1, inherit-stdio, sync-run. Component Model +
//! WASI Preview 2 land when the rest of the stack asks for them (cheap
//! — wasmtime already supports both; it's a matter of wiring the
//! linker).

#![cfg(feature = "runtime-wasmtime")]

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::preview1::WasiP1Ctx;
use wasmtime_wasi::{WasiCtxBuilder, pipe::MemoryOutputPipe};

use crate::engine::{WasmBoot, WasmEngine, WasmEngineError, WasmHandle, WasmModuleSource};
use crate::{WasiPreview, WasmRuntime};

pub struct WasmtimeEngine {
    engine: Engine,
}

impl Default for WasmtimeEngine {
    fn default() -> Self {
        let mut config = Config::new();
        config.async_support(false);
        let engine = Engine::new(&config).expect("wasmtime engine init");
        Self { engine }
    }
}

impl WasmEngine for WasmtimeEngine {
    fn runtime(&self) -> WasmRuntime {
        WasmRuntime::Wasmtime
    }

    fn run(&self, boot: &WasmBoot) -> Result<WasmHandle, WasmEngineError> {
        // Preview 2 (component model) lands later — for now, p1 only.
        if boot.preview == WasiPreview::P2 {
            return Err(WasmEngineError::PreviewNotSupported(WasiPreview::P2));
        }

        let bytes = read_module(&boot.module)?;

        // Compile.
        let module = Module::new(&self.engine, &bytes)
            .map_err(|e| WasmEngineError::Compile(e.to_string()))?;

        // WASI context with captured stdout + stderr so the handle
        // reports what the guest printed.
        let stdout_pipe = MemoryOutputPipe::new(1 << 20);
        let stderr_pipe = MemoryOutputPipe::new(1 << 20);
        let wasi = WasiCtxBuilder::new()
            .stdout(stdout_pipe.clone())
            .stderr(stderr_pipe.clone())
            .build_p1();

        let mut store: Store<WasiP1Ctx> = Store::new(&self.engine, wasi);
        let mut linker: Linker<WasiP1Ctx> = Linker::new(&self.engine);
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |s| s)
            .map_err(|e| WasmEngineError::Instantiate(e.to_string()))?;

        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| WasmEngineError::Instantiate(e.to_string()))?;

        // The canonical WASI preview-1 entry is `_start`.
        let start = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .map_err(|e| WasmEngineError::Instantiate(e.to_string()))?;

        // Actually run.
        let call_result = start.call(&mut store, ());

        let mut handle = WasmHandle::new(&boot.name, WasmRuntime::Wasmtime);
        handle.stdout = String::from_utf8_lossy(&stdout_pipe.contents()).into_owned();
        handle.stderr = String::from_utf8_lossy(&stderr_pipe.contents()).into_owned();

        match call_result {
            Ok(()) => {
                handle.exit_code = Some(0);
                Ok(handle)
            }
            Err(e) => {
                // WASI's proc_exit trap carries the exit code.
                if let Some(exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                    handle.exit_code = Some(exit.0);
                    Ok(handle)
                } else {
                    Err(WasmEngineError::Run(format!("{e:?}")))
                }
            }
        }
    }

    fn shutdown(&self, _handle: &mut WasmHandle, _grace: Duration) -> Result<(), WasmEngineError> {
        // Sync-run means guest has already exited by the time we observe
        // a handle. Async/long-running support lands with H.6 hospedeiro.
        Ok(())
    }
}

fn read_module(src: &WasmModuleSource) -> Result<Vec<u8>, WasmEngineError> {
    match src {
        WasmModuleSource::Bytes(b) => Ok(b.clone()),
        WasmModuleSource::Wat(text) => wat::parse_str(text)
            .map_err(|e| WasmEngineError::Compile(format!("WAT parse: {e}"))),
        WasmModuleSource::Path(p) => {
            let mut buf = Vec::new();
            std::fs::File::open(p)
                .and_then(|mut f| f.read_to_end(&mut buf))
                .map_err(|e| WasmEngineError::Io(format!("{}: {e}", p.display())))?;
            Ok(buf)
        }
    }
}

// Silence unused-import lint for Arc/Mutex that we'll use once
// hospedeiro adds concurrent handle state.
#[allow(dead_code)]
fn _unused_lint_suppressor() -> Option<Arc<Mutex<()>>> {
    None
}
