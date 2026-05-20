//! Thin re-export shim — the canonical `lifetime_clock` module now lives
//! in `tatara_process::lifetime_clock` so every pleme-io controller that
//! reconciles `Process` CRDs (kenshi, tatara-reconciler, future
//! out-of-tree consumers) shares one TTL/teardown decision implementation.
//!
//! The re-export preserves `crate::lifetime_clock::{evaluate, AutoTerminate,
//! requeue_with_ttl}` so the existing phase_machine.rs imports compile
//! unchanged. New consumers should depend on `tatara_process` directly
//! and use `tatara_process::lifetime_clock::evaluate`.

pub use tatara_process::lifetime_clock::{evaluate, requeue_with_ttl, AutoTerminate};
