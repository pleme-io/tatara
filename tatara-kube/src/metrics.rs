use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicU64, Ordering};

/// Simple counters for reconciliation metrics.
/// These can be exposed via Prometheus or logged.
#[derive(Debug, Default)]
pub struct Metrics {
    pub reconcile_total: AtomicU64,
    pub reconcile_errors: AtomicU64,
    pub resources_applied: AtomicU64,
    pub resources_pruned: AtomicU64,
    pub resources_healthy: AtomicU64,
    pub resources_unhealthy: AtomicU64,
    pub nix_eval_duration_ms: AtomicU64,
    pub apply_duration_ms: AtomicU64,
}

impl Metrics {
    pub fn inc_reconcile(&self) {
        self.reconcile_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_reconcile_error(&self) {
        self.reconcile_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_applied(&self, count: u64) {
        self.resources_applied.fetch_add(count, Ordering::Relaxed);
    }

    pub fn add_pruned(&self, count: u64) {
        self.resources_pruned.fetch_add(count, Ordering::Relaxed);
    }

    pub fn set_nix_eval_duration(&self, ms: u64) {
        self.nix_eval_duration_ms.store(ms, Ordering::Relaxed);
    }

    pub fn set_apply_duration(&self, ms: u64) {
        self.apply_duration_ms.store(ms, Ordering::Relaxed);
    }

    pub fn summary(&self) -> String {
        format!(
            "reconcile={} errors={} applied={} pruned={} eval_ms={} apply_ms={}",
            self.reconcile_total.load(Ordering::Relaxed),
            self.reconcile_errors.load(Ordering::Relaxed),
            self.resources_applied.load(Ordering::Relaxed),
            self.resources_pruned.load(Ordering::Relaxed),
            self.nix_eval_duration_ms.load(Ordering::Relaxed),
            self.apply_duration_ms.load(Ordering::Relaxed),
        )
    }
}

/// Reconciliation tick result.
#[derive(Debug, Clone)]
pub struct ReconcileStats {
    pub applied: u32,
    pub pruned: u32,
    pub unchanged: u32,
    pub errors: u32,
    pub duration_ms: u64,
    pub timestamp: DateTime<Utc>,
}
