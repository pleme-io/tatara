//! The `GuestSupervisor` — accepts a `GuestSpec`, resolves artifacts
//! through the build transport, dispatches to the right backend
//! engine, holds handles keyed by guest name, reports status.
//!
//! Today it runs synchronously (boot → run → record → reap) because
//! every backend we ship (wasmtime/wasmer/wasmi) is sync-run. When VM
//! backends land they'll carry their own async lifecycle; the
//! supervisor grows a background executor at that point.

use std::collections::HashMap;
use std::path::PathBuf;

use thiserror::Error;

use tatara_build_remote::{BuildError, BuildTransport};
use tatara_vm::{GuestKind, GuestSpec, WasmSpec};
use tatara_wasm::{
    engine_for, WasmBoot, WasmEngineError, WasmHandle, WasmModuleSource,
};

use crate::GuestStatus;

/// Minimal per-guest record the supervisor holds after dispatch.
#[derive(Debug, Clone)]
pub struct GuestRecord {
    pub name: String,
    pub status: GuestStatus,
    pub kind_tag: &'static str, // "wasm" / "vm"
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl GuestRecord {
    fn from_wasm(handle: &WasmHandle, status: GuestStatus) -> Self {
        Self {
            name: handle.name.clone(),
            status,
            kind_tag: "wasm",
            exit_code: handle.exit_code,
            stdout: handle.stdout.clone(),
            stderr: handle.stderr.clone(),
        }
    }
}

/// The supervisor itself. Keyed by guest name.
#[derive(Default)]
pub struct GuestSupervisor {
    records: HashMap<String, GuestRecord>,
}

impl GuestSupervisor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of records the supervisor currently holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Inspect the record for a guest.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&GuestRecord> {
        self.records.get(name)
    }

    /// Remove a record (equivalent of `reap`).
    pub fn remove(&mut self, name: &str) -> Option<GuestRecord> {
        self.records.remove(name)
    }

    /// Take a `GuestSpec`, route through the declared build transport,
    /// dispatch to the right backend, run, record. Synchronous.
    ///
    /// # Errors
    /// `SupervisorError` on any failure. Each failure class is typed.
    pub fn boot(&mut self, spec: &GuestSpec) -> Result<GuestStatus, SupervisorError> {
        tracing::info!(
            name = spec.name.as_str(),
            kind = kind_tag(&spec.kind),
            "hospedeiro: booting guest"
        );

        match &spec.kind {
            GuestKind::Vm(_) => {
                // Real VM dispatch lands in H.2 (HVF) / kasou (VZ). For
                // now, record the intent so operators can see the guest
                // was requested, error as "backend not ready".
                self.records.insert(
                    spec.name.clone(),
                    GuestRecord {
                        name: spec.name.clone(),
                        status: GuestStatus::Failed,
                        kind_tag: "vm",
                        exit_code: None,
                        stdout: String::new(),
                        stderr: "VM backend not yet wired (waiting on H.2 HVF)".into(),
                    },
                );
                Err(SupervisorError::VmBackendUnavailable)
            }
            GuestKind::Wasm(w) => self.boot_wasm(spec, w),
        }
    }

    fn boot_wasm(
        &mut self,
        spec: &GuestSpec,
        wasm: &WasmSpec,
    ) -> Result<GuestStatus, SupervisorError> {
        // 1. Resolve the component through the transport chain.
        let layered = spec.build_on.to_layered();
        let store_path = if layered.transports.is_empty() {
            return Err(SupervisorError::NoTransportsConfigured);
        } else {
            layered
                .fetch(&wasm.component)
                .map_err(SupervisorError::Build)?
        };

        // 2. Read the bytes at the resolved store path.
        let bytes = std::fs::read(PathBuf::from(&store_path.0))
            .map_err(|e| SupervisorError::Io(format!("{}: {e}", store_path.0)))?;

        // 3. Dispatch to the right WASM runtime.
        let engine = engine_for(wasm.runtime).map_err(SupervisorError::Engine)?;
        let boot = WasmBoot {
            module: WasmModuleSource::Bytes(bytes),
            runtime: wasm.runtime,
            preview: wasm.wasi_preview,
            features: wasm.features.clone(),
            name: spec.name.clone(),
        };

        let handle = engine.run(&boot).map_err(SupervisorError::Engine)?;

        // 4. Map exit_code → status and record.
        let status = match handle.exit_code {
            Some(0) => GuestStatus::Reaped,
            Some(_) => GuestStatus::Failed,
            None => GuestStatus::Zombie,
        };
        let record = GuestRecord::from_wasm(&handle, status);
        self.records.insert(record.name.clone(), record);
        Ok(status)
    }

    /// For dispatching inline-bytes guests (e.g. tests). The `GuestSpec`
    /// contract says artifacts come from a `BuildRef`, but we support
    /// an escape hatch for prompt-driven LLM flows and unit tests that
    /// already have bytes.
    ///
    /// # Errors
    /// Propagates engine errors.
    pub fn boot_wasm_bytes(
        &mut self,
        name: impl Into<String>,
        wasm: &WasmSpec,
        bytes: Vec<u8>,
    ) -> Result<GuestStatus, SupervisorError> {
        let name = name.into();
        let engine = engine_for(wasm.runtime).map_err(SupervisorError::Engine)?;
        let boot = WasmBoot {
            module: WasmModuleSource::Bytes(bytes),
            runtime: wasm.runtime,
            preview: wasm.wasi_preview,
            features: wasm.features.clone(),
            name: name.clone(),
        };
        let handle = engine.run(&boot).map_err(SupervisorError::Engine)?;
        let status = match handle.exit_code {
            Some(0) => GuestStatus::Reaped,
            Some(_) => GuestStatus::Failed,
            None => GuestStatus::Zombie,
        };
        self.records
            .insert(name, GuestRecord::from_wasm(&handle, status));
        Ok(status)
    }
}

fn kind_tag(k: &GuestKind) -> &'static str {
    match k {
        GuestKind::Vm(_) => "vm",
        GuestKind::Wasm(_) => "wasm",
    }
}

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("no transports configured (empty build_on chain)")]
    NoTransportsConfigured,

    #[error("build transport: {0}")]
    Build(BuildError),

    #[error("io: {0}")]
    Io(String),

    #[error("wasm engine: {0}")]
    Engine(WasmEngineError),

    #[error("VM backend not yet wired — waiting on H.2 HVF / kasou integration")]
    VmBackendUnavailable,
}
