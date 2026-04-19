//! `tatara-hospedeiro` — guest runtime orchestrator.
//!
//! *Hospedeiro* is Brazilian-Portuguese for "host". This crate is the
//! daemon/library that holds the live set of guests — VMs, WASM
//! components, eventually anything else that fits the `GuestEngine`
//! trait — and dispatches lifecycle operations to the right backend:
//!
//! - `GuestKind::Vm { backend: Hvf }`  → `tatara_hvf::HvfEngine`
//! - `GuestKind::Vm { backend: Vz }`   → `kasou::VmHandle`
//! - `GuestKind::Wasm { runtime: … }`  → `tatara_wasm::WasmEngine`
//!
//! Every guest starts with a `BuildTransportChain` resolution through
//! `tatara_build_remote::LayeredTransport` — Attic → ssh-ng → local —
//! before any backend gets called. If the transport chain fails, the
//! guest never boots. Fail-closed by design.
//!
//! # Status
//!
//! **Phase H.6 landed.** `GuestSupervisor` runs WASM guests end-to-end
//! through `BuildTransportChain` → `WasmEngine` dispatch. VM dispatch
//! stub pending H.2 (HVF). MCP wiring is a cheap follow-on.

#![forbid(unsafe_code)]

pub mod reconcile;
pub mod supervisor;

pub use reconcile::{guest_status_to_process_phase, reconcile_intent, ReconcileError};
pub use supervisor::{GuestRecord, GuestSupervisor, SupervisorError};

use serde::{Deserialize, Serialize};

/// What phase the guest is in. Matches the `Process` CRD phase set
/// from `tatara-process` so Guest lifecycle composes cleanly with the
/// K8s-as-processes model.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum GuestStatus {
    /// BuildTransport is resolving artifacts.
    Building,
    /// Artifacts ready, backend preparing to boot.
    Forking,
    /// Kernel/component entry point invoked.
    Execing,
    /// Up and serving.
    Running,
    /// SIGTERM received; shutdown in progress.
    Exiting,
    /// Clean exit, not yet reaped.
    Zombie,
    /// Reaped — gone from the table.
    Reaped,
    /// Hard failure mid-lifecycle.
    Failed,
}

/// Phase H.1 placeholder. The real trait is defined in H.6 and
/// parameterizes over the GuestSpec type from `tatara-vm`.
pub const CRATE_STATUS: &str = "phase-h1-stub";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_enum_kebab_serialization() {
        assert_eq!(
            serde_json::to_string(&GuestStatus::Running).unwrap(),
            "\"running\""
        );
        assert_eq!(
            serde_json::to_string(&GuestStatus::Execing).unwrap(),
            "\"execing\""
        );
    }

    #[test]
    fn status_round_trip_json() {
        for s in [
            GuestStatus::Building,
            GuestStatus::Forking,
            GuestStatus::Execing,
            GuestStatus::Running,
            GuestStatus::Exiting,
            GuestStatus::Zombie,
            GuestStatus::Reaped,
            GuestStatus::Failed,
        ] {
            let j = serde_json::to_string(&s).unwrap();
            let back: GuestStatus = serde_json::from_str(&j).unwrap();
            assert_eq!(s, back);
        }
    }
}
