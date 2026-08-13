//! wasmtime backend. Compiled in with `runtime-wasmtime`.
//!
//! Today: WASI Preview 1, sync-run. Component Model + WASI Preview 2 land
//! when the rest of the stack asks for them (cheap — wasmtime already
//! supports both; it's a matter of wiring the linker). What this engine
//! accepts is declared in `engine::facts_of`, not repeated here.
//!
//! **Stdio is granted, never inherited.** This impl used to build its
//! `WasiCtxBuilder` with stdout+stderr unconditionally; it now derives the
//! whole context from `WasmBoot::capabilities`, and adds the WASI linker only
//! when a grant is present.

#![cfg(feature = "runtime-wasmtime")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::preview1::WasiP1Ctx;
use wasmtime_wasi::{pipe::MemoryOutputPipe, DirPerms, FilePerms, WasiCtxBuilder};

use crate::engine::{
    check_boot, check_imports_granted, read_module, WasmBoot, WasmEngine, WasmEngineError,
    WasmHandle,
};
use crate::WasmRuntime;

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
        // Preview + WASI-grant validation, from the one facts table.
        check_boot(&self.facts(), boot)?;

        let bytes = read_module(&boot.module)?;

        // Compile.
        let module = Module::new(&self.engine, &bytes)
            .map_err(|e| WasmEngineError::Compile(e.to_string()))?;

        // Refuse an ungranted import by name, before instantiation. The
        // structural refusal happens anyway (an absent import cannot link);
        // this makes it typed and early.
        check_imports_granted(
            &boot.capabilities,
            module.imports().map(|i| (i.module(), i.name())),
        )?;

        // The WASI context is DERIVED from the declaration. Previously this
        // hardcoded stdout+stderr, so every guest got them whether or not the
        // caller meant to grant them.
        let grant = boot.capabilities.wasi.as_ref();
        let stdout_pipe = MemoryOutputPipe::new(1 << 20);
        let stderr_pipe = MemoryOutputPipe::new(1 << 20);

        let mut builder = WasiCtxBuilder::new();
        if let Some(g) = grant {
            if g.stdout {
                builder.stdout(stdout_pipe.clone());
            }
            if g.stderr {
                builder.stderr(stderr_pipe.clone());
            }
            for (k, v) in &g.env {
                builder.env(k, v);
            }
            if !g.argv.is_empty() {
                builder.args(&g.argv);
            }
            for p in &g.preopens {
                let dir_perms = if p.writable {
                    DirPerms::all()
                } else {
                    DirPerms::READ
                };
                let file_perms = if p.writable {
                    FilePerms::all()
                } else {
                    FilePerms::READ
                };
                builder
                    .preopened_dir(&p.host_path, &p.guest_path, dir_perms, file_perms)
                    .map_err(|e| {
                        WasmEngineError::Io(format!("preopen {}: {e}", p.host_path.display()))
                    })?;
            }
        }
        let wasi = builder.build_p1();

        let mut store: Store<WasiP1Ctx> = Store::new(&self.engine, wasi);
        let mut linker: Linker<WasiP1Ctx> = Linker::new(&self.engine);
        // Only added when WASI was granted: with no grant there is no symbol
        // for a `wasi_snapshot_preview1` import to resolve against.
        if grant.is_some() {
            wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |s| s)
                .map_err(|e| WasmEngineError::Instantiate(e.to_string()))?;
        }

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

// `read_module` lived here, byte-identical to the copies in wasmi_impl and
// wasmer_impl. It is `engine::read_module` now — one implementation.

// Silence unused-import lint for Arc/Mutex that we'll use once
// hospedeiro adds concurrent handle state.
#[allow(dead_code)]
fn _unused_lint_suppressor() -> Option<Arc<Mutex<()>>> {
    None
}
