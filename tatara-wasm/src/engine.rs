//! The `WasmEngine` trait — polymorphic over runtime.
//!
//! Each runtime (wasmtime, WasmEdge, Wasmer, wasmi, WAMR) lives behind
//! a Cargo feature flag and implements this trait. Consumers pick a
//! runtime per workload via `WasmSpec::runtime`; `boot()` dispatches
//! to the right implementation.

use std::time::Duration;

use crate::{WasiPreview, WasmFeatures, WasmRuntime};

/// The input to `WasmEngine::boot`. Self-contained: bytes of a WASM
/// module (or a WAT source string), a spec, and an artifact path.
#[derive(Debug, Clone)]
pub struct WasmBoot {
    /// Either raw WASM bytes (preferred) or a WAT text source. Engines
    /// decide how to ingest.
    pub module: WasmModuleSource,

    /// Runtime demanded by the spec.
    pub runtime: WasmRuntime,

    /// WASI preview version.
    pub preview: WasiPreview,

    /// Feature toggles.
    pub features: WasmFeatures,

    /// Name used for logging + handle identification.
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum WasmModuleSource {
    /// Compiled WASM binary bytes — the canonical path.
    Bytes(Vec<u8>),
    /// WebAssembly Text Format source. Engines that don't support WAT
    /// directly must reject this variant or convert via `wat`.
    Wat(String),
    /// A file path on disk. Reader runs inside the engine.
    Path(std::path::PathBuf),
}

/// A live WASM guest handle.
#[derive(Debug)]
pub struct WasmHandle {
    pub name: String,
    pub runtime: WasmRuntime,
    /// Accumulated captured stdout, for tests + replay.
    pub stdout: String,
    /// Accumulated captured stderr.
    pub stderr: String,
    /// The guest's exit code, if it has terminated.
    pub exit_code: Option<i32>,
}

impl WasmHandle {
    #[must_use]
    pub fn new(name: impl Into<String>, runtime: WasmRuntime) -> Self {
        Self {
            name: name.into(),
            runtime,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
        }
    }
}

/// The polymorphic-over-runtime engine trait.
pub trait WasmEngine {
    /// Which runtime this engine represents. Matches `WasmSpec::runtime`.
    fn runtime(&self) -> WasmRuntime;

    /// Boot the module and run it to completion (synchronous today).
    /// Captures stdout/stderr into the returned handle.
    ///
    /// # Errors
    /// Returns `WasmEngineError` on compile / instantiate / runtime failure.
    fn run(&self, boot: &WasmBoot) -> Result<WasmHandle, WasmEngineError>;

    /// Graceful shutdown — runtimes with long-running guests honor this.
    /// The default synchronous `run()` path is already terminated, so the
    /// default impl is a no-op.
    ///
    /// # Errors
    /// Implementations that have background resources may error.
    fn shutdown(
        &self,
        _handle: &mut WasmHandle,
        _grace: Duration,
    ) -> Result<(), WasmEngineError> {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WasmEngineError {
    #[error("runtime not compiled in: {0:?}")]
    RuntimeNotCompiled(WasmRuntime),

    #[error("preview not supported by this runtime: {0:?}")]
    PreviewNotSupported(WasiPreview),

    #[error("module compile failed: {0}")]
    Compile(String),

    #[error("instantiate failed: {0}")]
    Instantiate(String),

    #[error("run failed: {0}")]
    Run(String),

    #[error("io: {0}")]
    Io(String),
}

/// Factory: return a boxed engine for the requested runtime. Errors if
/// the runtime's feature flag is not compiled in.
///
/// # Errors
/// Returns `WasmEngineError::RuntimeNotCompiled` when the requested
/// runtime's feature isn't enabled at compile time.
pub fn engine_for(runtime: WasmRuntime) -> Result<Box<dyn WasmEngine>, WasmEngineError> {
    match runtime {
        #[cfg(feature = "runtime-wasmtime")]
        WasmRuntime::Wasmtime => Ok(Box::new(crate::wasmtime_impl::WasmtimeEngine::default())),

        #[cfg(feature = "runtime-wasmer")]
        WasmRuntime::Wasmer => Ok(Box::new(crate::wasmer_impl::WasmerEngine::default())),

        #[cfg(feature = "runtime-wasmi")]
        WasmRuntime::Wasmi => Ok(Box::new(crate::wasmi_impl::WasmiEngine::default())),

        // WasmEdge + WAMR land in H.4 follow-up (C/C++ SDK linking).
        other => Err(WasmEngineError::RuntimeNotCompiled(other)),
    }
}
