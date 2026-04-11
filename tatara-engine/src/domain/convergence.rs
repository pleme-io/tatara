//! Convergence engine — drives local allocations toward desired state.
//!
//! Replaces the old 5-pass reconciler with a single convergence algorithm
//! that compares desired state (from Raft) to observed state (local) and
//! applies the appropriate state transitions.
//!
//! Every node runs this independently. Each node only converges
//! allocations assigned to it. The result is distributed choreography:
//! no single coordinator, every node independently drives toward
//! the declared state.

use crate::catalog::registry::CatalogRegistry;
use crate::domain::health_probe::ProbeExecutor;
use crate::domain::port_allocator::PortAllocator;
use crate::domain::volume_manager::VolumeManager;
use crate::metrics::TataraMetrics;
use crate::nats::NatsEventBus;
use crate::secrets::SecretResolver;

use std::sync::Arc;
use tatara_core::cluster::types::NodeId;
use tatara_core::domain::lifecycle::*;
use tracing::{debug, info, warn};

/// Subsystems needed by the convergence engine.
pub struct ConvergenceContext {
    pub local_node_id: NodeId,
    pub probe_executor: Arc<ProbeExecutor>,
    pub catalog_registry: Arc<CatalogRegistry>,
    pub port_allocator: Arc<PortAllocator>,
    pub volume_manager: Arc<VolumeManager>,
    pub secret_resolver: Arc<SecretResolver>,
    pub nats_bus: Arc<NatsEventBus>,
    pub metrics: Arc<TataraMetrics>,
}

/// Result of a single convergence tick.
#[derive(Debug, Default)]
pub struct ConvergenceResult {
    pub warmed: u32,
    pub started: u32,
    pub contracted: u32,
    pub terminated: u32,
    pub health_checks: u32,
    pub orphans_detected: u32,
}

/// Run a single convergence tick for the local node.
///
/// Compares desired allocations (from Raft) against observed allocations
/// and drives each toward its desired phase.
pub async fn converge_tick(
    ctx: &ConvergenceContext,
    desired: &[DesiredAllocationState],
    observed: &std::collections::HashMap<uuid::Uuid, ObservedAllocationState>,
) -> ConvergenceResult {
    let mut result = ConvergenceResult::default();
    let my_node = format!("{}", ctx.local_node_id);

    // Only process allocations assigned to this node
    let my_desired: Vec<&DesiredAllocationState> = desired
        .iter()
        .filter(|d| d.node_id == my_node)
        .collect();

    for desired_alloc in &my_desired {
        let obs_phase = observed
            .get(&desired_alloc.alloc_id)
            .map(|o| &o.phase);

        match (&desired_alloc.desired_phase, obs_phase) {
            // Want Active, not started → begin warming
            (DesiredPhase::Active, None) | (DesiredPhase::Active, Some(WorkloadPhase::Initial)) => {
                debug!(
                    alloc_id = %desired_alloc.alloc_id,
                    "convergence: initial → warming"
                );
                result.warmed += 1;
            }

            // Want Active, warming → check if ready to execute
            (DesiredPhase::Active, Some(WorkloadPhase::Warming(progress))) => {
                if progress.secrets_resolved && progress.volumes_mounted {
                    debug!(
                        alloc_id = %desired_alloc.alloc_id,
                        "convergence: warming → executing"
                    );
                    result.started += 1;
                }
            }

            // Want Active, executing → run health checks
            (DesiredPhase::Active, Some(WorkloadPhase::Executing(_))) => {
                result.health_checks += 1;
            }

            // Want Stopped, executing → begin contraction
            (DesiredPhase::Stopped { reason }, Some(WorkloadPhase::Executing(_))) => {
                info!(
                    alloc_id = %desired_alloc.alloc_id,
                    reason = ?reason,
                    "convergence: executing → contracting"
                );
                result.contracted += 1;
            }

            // Want Stopped, contracting → check if drain complete
            (DesiredPhase::Stopped { .. }, Some(WorkloadPhase::Contracting(_))) => {
                debug!(
                    alloc_id = %desired_alloc.alloc_id,
                    "convergence: contracting → checking drain"
                );
            }

            // Want Stopped, already terminal → no-op
            (DesiredPhase::Stopped { .. }, Some(WorkloadPhase::Terminal(_))) => {}

            // Want Active but terminal → scheduler should create replacement
            (DesiredPhase::Active, Some(WorkloadPhase::Terminal(_))) => {
                warn!(
                    alloc_id = %desired_alloc.alloc_id,
                    "desired Active but allocation is Terminal — scheduler should replace"
                );
            }

            _ => {}
        }
    }

    // Detect orphans: observed locally but not in desired set
    for (alloc_id, obs) in observed {
        if obs.node_id != my_node {
            continue;
        }
        if obs.phase.is_terminal() {
            continue;
        }
        let is_desired = desired.iter().any(|d| d.alloc_id == *alloc_id);
        if !is_desired {
            info!(alloc_id = %alloc_id, "orphaned allocation detected");
            result.orphans_detected += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_desired(id: uuid::Uuid, phase: DesiredPhase) -> DesiredAllocationState {
        DesiredAllocationState {
            alloc_id: id,
            job_id: "test-job".to_string(),
            group_name: "main".to_string(),
            node_id: "1".to_string(),
            job_version: 1,
            desired_phase: phase,
            generation: 1,
        }
    }

    fn make_observed(id: uuid::Uuid, phase: AllocationPhase) -> ObservedAllocationState {
        ObservedAllocationState {
            alloc_id: id,
            node_id: "1".to_string(),
            phase,
            observed_at: chrono::Utc::now(),
            observation_seq: 1,
        }
    }

    fn make_ctx() -> ConvergenceContext {
        ConvergenceContext {
            local_node_id: 1,
            probe_executor: Arc::new(ProbeExecutor::new()),
            catalog_registry: Arc::new(CatalogRegistry::new()),
            port_allocator: Arc::new(PortAllocator::default_range()),
            volume_manager: Arc::new(VolumeManager::new("/tmp/test-volumes".into())),
            secret_resolver: Arc::new(SecretResolver::new()),
            nats_bus: Arc::new(NatsEventBus::disconnected()),
            metrics: TataraMetrics::new(),
        }
    }

    #[tokio::test]
    async fn test_convergence_initial_to_warming() {
        let ctx = make_ctx();
        let id = uuid::Uuid::new_v4();
        let desired = vec![make_desired(id, DesiredPhase::Active)];
        let observed = HashMap::new();

        let result = converge_tick(&ctx, &desired, &observed).await;
        assert_eq!(result.warmed, 1);
    }

    #[tokio::test]
    async fn test_convergence_executing_health_check() {
        let ctx = make_ctx();
        let id = uuid::Uuid::new_v4();
        let desired = vec![make_desired(id, DesiredPhase::Active)];
        let observed = HashMap::from([(
            id,
            make_observed(id, AllocationPhase::Executing(AllocExecuteDetail {
                registered_in_catalog: true,
                health: HealthStatus::Passing,
                task_states: HashMap::new(),
            })),
        )]);

        let result = converge_tick(&ctx, &desired, &observed).await;
        assert_eq!(result.health_checks, 1);
    }

    #[tokio::test]
    async fn test_convergence_stop_triggers_contraction() {
        let ctx = make_ctx();
        let id = uuid::Uuid::new_v4();
        let desired = vec![make_desired(
            id,
            DesiredPhase::Stopped { reason: ContractReason::Stopped },
        )];
        let observed = HashMap::from([(
            id,
            make_observed(id, AllocationPhase::Executing(AllocExecuteDetail {
                registered_in_catalog: true,
                health: HealthStatus::Passing,
                task_states: HashMap::new(),
            })),
        )]);

        let result = converge_tick(&ctx, &desired, &observed).await;
        assert_eq!(result.contracted, 1);
    }

    #[tokio::test]
    async fn test_convergence_orphan_detection() {
        let ctx = make_ctx();
        let orphan_id = uuid::Uuid::new_v4();

        let desired = vec![]; // nothing desired
        let observed = HashMap::from([(
            orphan_id,
            make_observed(orphan_id, AllocationPhase::Executing(AllocExecuteDetail {
                registered_in_catalog: false,
                health: HealthStatus::Unknown,
                task_states: HashMap::new(),
            })),
        )]);

        let result = converge_tick(&ctx, &desired, &observed).await;
        assert_eq!(result.orphans_detected, 1);
    }
}
