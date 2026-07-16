//! Engine selection — map a `(defvm …)` [`Hypervisor`] to an in-process
//! [`maquina_engine::MaquinaEngine`].
//!
//! Only the Rust-native backends are real engines (zero shell-out): `Libkrun`
//! (`tateru`) and `Kasou` (VZ). `Vfkit` / `VfkitDarwin` / `Qemu` are CLI
//! emitters (the `vfkit` module) with no in-process engine — selecting them
//! here is an error by design (see `theory/MAQUINA.md` § zero-shell-out).
//!
//! The real engines live behind tatara-vm's `engines` feature (macOS), which
//! forwards to `maquina-engine`'s `tateru-backend` + `kasou-backend`. Without
//! that feature (the default build, or any non-macOS target), `engine_for`
//! reports the engine as unavailable so the host-agnostic build of tatara-vm
//! stays dependency-light.

use maquina_engine::MaquinaEngine;

use crate::config::Hypervisor;

/// Why an engine could not be selected for a hypervisor.
#[derive(Debug, thiserror::Error)]
pub enum EngineSelectError {
    /// The hypervisor is a CLI emitter, not an in-process engine.
    #[error(
        "hypervisor {0:?} has no in-process MaquinaEngine (it is a CLI emitter); \
         select Libkrun or Kasou"
    )]
    NoEngine(Hypervisor),
    /// No engine compiled in (engines are macOS-only, behind `--features engines`).
    #[error("no máquina engine available: build tatara-vm with --features engines on macOS")]
    Unavailable,
}

/// Select the in-process [`MaquinaEngine`] for a hypervisor.
///
/// # Errors
/// [`EngineSelectError::NoEngine`] for shell-out hypervisors
/// (`Vfkit`/`VfkitDarwin`/`Qemu`).
#[cfg(all(target_os = "macos", feature = "engines"))]
pub fn engine_for(h: &Hypervisor) -> Result<Box<dyn MaquinaEngine>, EngineSelectError> {
    match h {
        Hypervisor::Libkrun => Ok(Box::new(maquina_engine::tateru_backend::TateruEngine::new())),
        Hypervisor::Kasou => Ok(Box::new(maquina_engine::kasou_backend::KasouEngine::new())),
        other => Err(EngineSelectError::NoEngine(*other)),
    }
}

/// Fallback when no engine backend is compiled in (default build / non-macOS).
///
/// # Errors
/// Always [`EngineSelectError::Unavailable`].
#[cfg(not(all(target_os = "macos", feature = "engines")))]
pub fn engine_for(_h: &Hypervisor) -> Result<Box<dyn MaquinaEngine>, EngineSelectError> {
    Err(EngineSelectError::Unavailable)
}
