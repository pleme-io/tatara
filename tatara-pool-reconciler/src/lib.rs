//! tatara-pool-reconciler — Kubernetes controller for `EphemeralPool` +
//! `EphemeralAllocation` CRDs.
//!
//! Two reconcile loops, one decision algebra each:
//!
//! 1. **Pool reconciler** — watches `EphemeralPool`. For each Pool,
//!    pure `decide_pool_reconcile(pool, observed_members, now)` returns
//!    a typed `PoolDecision` (Spawn N members | Reap N members |
//!    NoOp). The async tail of the controller applies the decision via
//!    kube-rs (create/delete owned Process CRs).
//!
//! 2. **Allocation reconciler** — watches `EphemeralAllocation`. For
//!    each Allocation, pure `decide_allocation_reconcile(alloc,
//!    candidate_pools, members, now)` returns a typed
//!    `AllocationDecision` (Bind { pool, member } | Wait |
//!    NoMatchingPool | ForceRelease). The async tail patches the
//!    Allocation + the assigned Process.
//!
//! All routing / return-policy decisions are pure functions over
//! typed inputs — fully unit-testable without a cluster. The kube
//! controller is the thin async glue that fetches/applies; the
//! decisions are the algebra.

pub mod context;
pub mod naming;
pub mod return_policy;
pub mod router;
pub mod pool_decide;
pub mod allocation_decide;
pub mod controller_pool;
pub mod controller_allocation;

pub use context::{PoolContext, PoolReconcilerConfig};
pub use pool_decide::{decide_pool_reconcile, PoolDecision};
pub use allocation_decide::{decide_allocation_reconcile, AllocationDecision};
pub use router::{best_match, MatchedPool};
pub use return_policy::{plan_return, ReturnPlan};

/// Typed reconciler error — kube `Controller<T>` requires a
/// `std::error::Error`-implementing type, which `anyhow::Error` is
/// not (orphan rules). Both controller paths surface anyhow errors
/// wrapped in this thiserror enum.
#[derive(Debug, thiserror::Error)]
pub enum ReconcilerError {
    /// Anything else (wraps anyhow::Error).
    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for ReconcilerError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e.to_string())
    }
}
