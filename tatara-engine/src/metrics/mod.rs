//! Prometheus metrics for tatara — jobs, allocations, nodes, reconciler,
//! scheduler, gossip, and driver health.
//!
//! Exposes a `/metrics` endpoint via the REST API router.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Central metrics registry for tatara.
#[derive(Debug, Default)]
pub struct TataraMetrics {
    // Gauges
    pub jobs_total: AtomicU64,
    pub jobs_pending: AtomicU64,
    pub jobs_running: AtomicU64,
    pub allocations_total: AtomicU64,
    pub allocations_running: AtomicU64,
    pub allocations_failed: AtomicU64,
    pub nodes_total: AtomicU64,
    pub nodes_ready: AtomicU64,
    pub services_registered: AtomicU64,

    // Counters
    pub reconcile_total: AtomicU64,
    pub reconcile_errors: AtomicU64,
    pub scheduler_evals: AtomicU64,
    pub health_probes_executed: AtomicU64,
    pub health_probes_failed: AtomicU64,
    pub secrets_fetched: AtomicU64,
    pub ports_allocated: AtomicU64,

    // Timing (last recorded values in milliseconds)
    pub reconcile_duration_ms: AtomicU64,
    pub scheduler_eval_duration_ms: AtomicU64,
    pub nix_eval_duration_ms: AtomicU64,
}

impl TataraMetrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Render metrics in Prometheus text exposition format.
    pub fn render_prometheus(&self) -> String {
        let mut out = String::with_capacity(2048);

        // Gauges
        prom_gauge(&mut out, "tatara_jobs_total", "Total number of jobs", self.jobs_total.load(Ordering::Relaxed));
        prom_gauge(&mut out, "tatara_jobs_pending", "Jobs in pending state", self.jobs_pending.load(Ordering::Relaxed));
        prom_gauge(&mut out, "tatara_jobs_running", "Jobs in running state", self.jobs_running.load(Ordering::Relaxed));
        prom_gauge(&mut out, "tatara_allocations_total", "Total allocations", self.allocations_total.load(Ordering::Relaxed));
        prom_gauge(&mut out, "tatara_allocations_running", "Running allocations", self.allocations_running.load(Ordering::Relaxed));
        prom_gauge(&mut out, "tatara_allocations_failed", "Failed allocations", self.allocations_failed.load(Ordering::Relaxed));
        prom_gauge(&mut out, "tatara_nodes_total", "Total cluster nodes", self.nodes_total.load(Ordering::Relaxed));
        prom_gauge(&mut out, "tatara_nodes_ready", "Ready nodes", self.nodes_ready.load(Ordering::Relaxed));
        prom_gauge(&mut out, "tatara_services_registered", "Registered service instances", self.services_registered.load(Ordering::Relaxed));

        // Counters
        prom_counter(&mut out, "tatara_reconcile_total", "Total reconciliation ticks", self.reconcile_total.load(Ordering::Relaxed));
        prom_counter(&mut out, "tatara_reconcile_errors_total", "Reconciliation errors", self.reconcile_errors.load(Ordering::Relaxed));
        prom_counter(&mut out, "tatara_scheduler_evals_total", "Scheduler evaluation cycles", self.scheduler_evals.load(Ordering::Relaxed));
        prom_counter(&mut out, "tatara_health_probes_total", "Health probes executed", self.health_probes_executed.load(Ordering::Relaxed));
        prom_counter(&mut out, "tatara_health_probes_failed_total", "Health probes failed", self.health_probes_failed.load(Ordering::Relaxed));
        prom_counter(&mut out, "tatara_secrets_fetched_total", "Secrets fetched", self.secrets_fetched.load(Ordering::Relaxed));
        prom_counter(&mut out, "tatara_ports_allocated_total", "Ports allocated", self.ports_allocated.load(Ordering::Relaxed));

        // Timing gauges (last observed value)
        prom_gauge(&mut out, "tatara_reconcile_duration_ms", "Last reconcile duration in ms", self.reconcile_duration_ms.load(Ordering::Relaxed));
        prom_gauge(&mut out, "tatara_scheduler_eval_duration_ms", "Last scheduler eval duration in ms", self.scheduler_eval_duration_ms.load(Ordering::Relaxed));
        prom_gauge(&mut out, "tatara_nix_eval_duration_ms", "Last nix eval duration in ms", self.nix_eval_duration_ms.load(Ordering::Relaxed));

        out
    }

    pub fn inc(&self, field: &AtomicU64) {
        field.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set(&self, field: &AtomicU64, value: u64) {
        field.store(value, Ordering::Relaxed);
    }
}

fn prom_gauge(out: &mut String, name: &str, help: &str, value: u64) {
    out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} gauge\n{name} {value}\n"));
}

fn prom_counter(out: &mut String, name: &str, help: &str, value: u64) {
    out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} counter\n{name} {value}\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_prometheus() {
        let metrics = TataraMetrics::default();
        metrics.jobs_total.store(5, Ordering::Relaxed);
        metrics.jobs_running.store(3, Ordering::Relaxed);
        metrics.reconcile_total.store(100, Ordering::Relaxed);

        let output = metrics.render_prometheus();
        assert!(output.contains("tatara_jobs_total 5"));
        assert!(output.contains("tatara_jobs_running 3"));
        assert!(output.contains("tatara_reconcile_total 100"));
        assert!(output.contains("# TYPE tatara_jobs_total gauge"));
        assert!(output.contains("# TYPE tatara_reconcile_total counter"));
    }

    #[test]
    fn test_prometheus_format_validity() {
        let metrics = TataraMetrics::default();
        let output = metrics.render_prometheus();

        // Every metric must have HELP and TYPE lines
        for line in output.lines() {
            if line.starts_with("# HELP") {
                assert!(line.len() > 7, "HELP line too short: {}", line);
            } else if line.starts_with("# TYPE") {
                assert!(
                    line.contains("gauge") || line.contains("counter"),
                    "TYPE must be gauge or counter: {}",
                    line
                );
            } else if !line.is_empty() {
                // Metric line: name value
                let parts: Vec<&str> = line.split_whitespace().collect();
                assert_eq!(parts.len(), 2, "metric line must be 'name value': {}", line);
                assert!(
                    parts[1].parse::<u64>().is_ok(),
                    "metric value must be numeric: {}",
                    line
                );
            }
        }
    }

    #[test]
    fn test_inc_and_set() {
        let metrics = TataraMetrics::default();
        metrics.inc(&metrics.reconcile_total);
        metrics.inc(&metrics.reconcile_total);
        metrics.set(&metrics.nix_eval_duration_ms, 42);

        assert_eq!(metrics.reconcile_total.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.nix_eval_duration_ms.load(Ordering::Relaxed), 42);
    }
}
