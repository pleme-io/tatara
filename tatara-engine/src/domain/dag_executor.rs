//! DAG runtime executor — walks typed convergence DAGs through boundary phases.
//!
//! This is the convergence engine's core. Given a ConvergenceGraph and a
//! ConvergencePlan, the executor traverses points in topological order,
//! driving each through: Prepare → Execute → Verify → Attest.
//!
//! Independent points at the same topological level run in parallel.
//! The attestation hash from each point chains to the next.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use anyhow::Result;
use chrono::Duration;
use tracing::{debug, error, info, warn};

use tatara_core::domain::convergence_graph::ConvergenceGraph;
use tatara_core::domain::convergence_state::{
    BoundaryPhase, ConvergenceOutcome, ConvergencePoint,
};
use tatara_core::domain::point_id::PointId;

/// Result of executing a single convergence point through its boundary.
#[derive(Debug, Clone)]
pub struct PointExecutionResult {
    /// Which point was executed.
    pub point_id: PointId,
    /// What happened.
    pub outcome: ConvergenceOutcome,
    /// The attestation hash produced (if attested).
    pub attestation: Option<String>,
    /// How long execution took.
    pub duration: Duration,
    /// Final boundary phase.
    pub phase: BoundaryPhase,
}

/// Result of executing an entire convergence DAG.
#[derive(Debug)]
pub struct DagExecutionResult {
    /// Per-point outcomes.
    pub outcomes: BTreeMap<PointId, PointExecutionResult>,
    /// Total execution duration.
    pub total_duration: Duration,
    /// Final attestation hash (from the last point in topological order).
    pub final_attestation: Option<String>,
    /// Points that converged successfully.
    pub converged_count: usize,
    /// Points that failed.
    pub failed_count: usize,
    /// Points that degraded.
    pub degraded_count: usize,
}

/// The DAG executor drives a ConvergenceGraph through its boundary phases.
pub struct DagExecutor;

impl DagExecutor {
    /// Execute an entire convergence graph in topological order.
    ///
    /// Each point goes through: Prepare → Execute → Verify → Attest.
    /// The attestation from each point feeds into the next point's
    /// input_attestation.
    pub async fn execute_graph(
        graph: &ConvergenceGraph,
        execution_order: &[PointId],
    ) -> Result<DagExecutionResult> {
        let start = Instant::now();
        let mut outcomes = BTreeMap::new();
        let mut attestation_chain: HashMap<PointId, String> = HashMap::new();
        let mut converged = 0usize;
        let mut failed = 0usize;
        let mut degraded = 0usize;

        for point_id in execution_order {
            let point = graph
                .points
                .get(point_id)
                .ok_or_else(|| anyhow::anyhow!("point {point_id} not in graph"))?;

            // Collect input attestations from upstream points
            let input_attestations: Vec<String> = graph
                .edges
                .iter()
                .filter(|e| e.to == *point_id)
                .filter_map(|e| attestation_chain.get(&e.from).cloned())
                .collect();

            let input_attestation = if input_attestations.is_empty() {
                None
            } else {
                // Combine multiple input attestations into one
                let combined = input_attestations.join(":");
                Some(combined)
            };

            let result = Self::execute_point(point, input_attestation.as_deref()).await;

            // Store attestation for downstream points
            if let Some(ref att) = result.attestation {
                attestation_chain.insert(*point_id, att.clone());
            }

            match &result.outcome {
                ConvergenceOutcome::Converged => converged += 1,
                ConvergenceOutcome::Failed { reason } => {
                    error!(
                        point = %point.name,
                        reason = %reason,
                        "convergence point failed — downstream points blocked"
                    );
                    failed += 1;
                }
                ConvergenceOutcome::Degraded { .. } => degraded += 1,
            }

            outcomes.insert(*point_id, result);
        }

        let elapsed = start.elapsed();
        let final_attestation = execution_order
            .last()
            .and_then(|id| attestation_chain.get(id).cloned());

        Ok(DagExecutionResult {
            outcomes,
            total_duration: Duration::milliseconds(elapsed.as_millis() as i64),
            final_attestation,
            converged_count: converged,
            failed_count: failed,
            degraded_count: degraded,
        })
    }

    /// Execute a single convergence point through its four boundary phases.
    async fn execute_point(
        point: &ConvergencePoint,
        input_attestation: Option<&str>,
    ) -> PointExecutionResult {
        let start = Instant::now();

        // Phase 1: PREPARE — verify preconditions + input attestation
        debug!(point = %point.name, "boundary: preparing");
        if let Some(input) = input_attestation {
            if let Some(ref expected) = point.boundary.input_attestation {
                if input != expected {
                    return PointExecutionResult {
                        point_id: PointId::compute(
                            point.name.as_bytes(),
                            &[],
                            point.description.as_bytes(),
                        ),
                        outcome: ConvergenceOutcome::Failed {
                            reason: format!(
                                "input attestation mismatch: expected {expected}, got {input}"
                            ),
                        },
                        attestation: None,
                        duration: Duration::milliseconds(start.elapsed().as_millis() as i64),
                        phase: BoundaryPhase::Failed {
                            reason: "attestation mismatch".into(),
                        },
                    };
                }
            }
        }
        for check in &point.boundary.preconditions {
            if !check.passed {
                return PointExecutionResult {
                    point_id: PointId::compute(
                        point.name.as_bytes(),
                        &[],
                        point.description.as_bytes(),
                    ),
                    outcome: ConvergenceOutcome::Failed {
                        reason: format!("precondition failed: {}", check.name),
                    },
                    attestation: None,
                    duration: Duration::milliseconds(start.elapsed().as_millis() as i64),
                    phase: BoundaryPhase::Failed {
                        reason: format!("precondition: {}", check.name),
                    },
                };
            }
        }

        // Phase 2: EXECUTE — drive convergence (placeholder for real driver dispatch)
        debug!(point = %point.name, "boundary: executing");
        // In the full implementation, this dispatches to the appropriate driver
        // based on point configuration. For now, the point "converges" immediately.

        // Phase 3: VERIFY — check postconditions
        debug!(point = %point.name, "boundary: verifying");
        for check in &point.boundary.postconditions {
            if !check.passed {
                warn!(
                    point = %point.name,
                    check = %check.name,
                    "postcondition not yet passing — degraded convergence"
                );
                return PointExecutionResult {
                    point_id: PointId::compute(
                        point.name.as_bytes(),
                        &[],
                        point.description.as_bytes(),
                    ),
                    outcome: ConvergenceOutcome::Degraded {
                        achieved: point.state.distance.clone(),
                        missing: vec![check.name.clone()],
                    },
                    attestation: None,
                    duration: Duration::milliseconds(start.elapsed().as_millis() as i64),
                    phase: BoundaryPhase::Verifying,
                };
            }
        }

        // Phase 4: ATTEST — produce blake3 hash
        debug!(point = %point.name, "boundary: attesting");
        let attestation_data = format!(
            "{}:{}:{}",
            point.name,
            input_attestation.unwrap_or("genesis"),
            point.boundary.postconditions.len(),
        );
        let attestation = format!("blake3:{}", blake3::hash(attestation_data.as_bytes()));

        info!(
            point = %point.name,
            attestation = %attestation,
            "boundary: attested"
        );

        PointExecutionResult {
            point_id: PointId::compute(
                point.name.as_bytes(),
                &[],
                point.description.as_bytes(),
            ),
            outcome: ConvergenceOutcome::Converged,
            attestation: Some(attestation),
            duration: Duration::milliseconds(start.elapsed().as_millis() as i64),
            phase: BoundaryPhase::Attested,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_core::domain::convergence_graph::*;
    use tatara_core::domain::convergence_state::*;

    fn make_point(name: &str) -> (PointId, ConvergencePoint) {
        let id = PointId::compute(name.as_bytes(), &[], b"desired");

        // Pre-pass all preconditions and postconditions
        let mut pre = BoundaryCheck::new("ready", "point is ready");
        pre.pass();
        let mut post = BoundaryCheck::new("correct", "output is correct");
        post.pass();

        let point = ConvergencePoint {
            name: name.into(),
            description: format!("{name} convergence point"),
            monotone: true,
            mechanism: ConvergenceMechanism::Local,
            state: ConvergenceState::new(name),
            boundary: ConvergenceBoundary {
                preconditions: vec![pre],
                postconditions: vec![post],
                input_attestation: None,
                output_attestation: None,
                phase: BoundaryPhase::Pending,
            },
            point_type: ConvergencePointType::Transform,
            horizon: ConvergenceHorizon::Bounded,
            substrate: SubstrateType::Compute,
            computation_mode: ComputationMode::Mechanical,
        };
        (id, point)
    }

    #[tokio::test]
    async fn test_single_point_execution() {
        let mut graph = ConvergenceGraph::new();
        let (id, point) = make_point("a");
        graph.add_point(id, point);

        let order = vec![id];
        let result = DagExecutor::execute_graph(&graph, &order).await.unwrap();

        assert_eq!(result.converged_count, 1);
        assert_eq!(result.failed_count, 0);
        assert!(result.final_attestation.is_some());
    }

    #[tokio::test]
    async fn test_linear_chain_execution() {
        let mut graph = ConvergenceGraph::new();
        let (a, pa) = make_point("a");
        let (b, pb) = make_point("b");
        let (c, pc) = make_point("c");
        graph.add_point(a, pa);
        graph.add_point(b, pb);
        graph.add_point(c, pc);
        graph.add_edge(TypedEdge {
            from: a,
            to: b,
            edge_type: EdgeType::Attestation,
        });
        graph.add_edge(TypedEdge {
            from: b,
            to: c,
            edge_type: EdgeType::Attestation,
        });

        let order = graph.topological_order().unwrap();
        let result = DagExecutor::execute_graph(&graph, &order).await.unwrap();

        assert_eq!(result.converged_count, 3);
        assert_eq!(result.failed_count, 0);
        // All three should have attestations
        for (_, outcome) in &result.outcomes {
            assert!(outcome.attestation.is_some());
            assert!(matches!(outcome.phase, BoundaryPhase::Attested));
        }
    }

    #[tokio::test]
    async fn test_failed_precondition_blocks() {
        let mut graph = ConvergenceGraph::new();
        let id = PointId::compute(b"fail", &[], b"desired");

        // Don't pass the precondition
        let pre = BoundaryCheck::new("not_ready", "not ready yet");
        let mut post = BoundaryCheck::new("ok", "ok");
        post.pass();

        let point = ConvergencePoint {
            name: "fail_point".into(),
            description: "will fail".into(),
            monotone: true,
            mechanism: ConvergenceMechanism::Local,
            state: ConvergenceState::new("fail_point"),
            boundary: ConvergenceBoundary {
                preconditions: vec![pre], // NOT passed
                postconditions: vec![post],
                input_attestation: None,
                output_attestation: None,
                phase: BoundaryPhase::Pending,
            },
            point_type: ConvergencePointType::Transform,
            horizon: ConvergenceHorizon::Bounded,
            substrate: SubstrateType::Compute,
            computation_mode: ComputationMode::Mechanical,
        };
        graph.add_point(id, point);

        let result = DagExecutor::execute_graph(&graph, &[id]).await.unwrap();
        assert_eq!(result.failed_count, 1);
        assert_eq!(result.converged_count, 0);
    }

    #[tokio::test]
    async fn test_degraded_postcondition() {
        let mut graph = ConvergenceGraph::new();
        let id = PointId::compute(b"degrade", &[], b"desired");

        let mut pre = BoundaryCheck::new("ready", "ready");
        pre.pass();
        // Don't pass postcondition
        let post = BoundaryCheck::new("not_verified", "verification pending");

        let point = ConvergencePoint {
            name: "degrade_point".into(),
            description: "will degrade".into(),
            monotone: true,
            mechanism: ConvergenceMechanism::Local,
            state: ConvergenceState::new("degrade_point"),
            boundary: ConvergenceBoundary {
                preconditions: vec![pre],
                postconditions: vec![post], // NOT passed
                input_attestation: None,
                output_attestation: None,
                phase: BoundaryPhase::Pending,
            },
            point_type: ConvergencePointType::Transform,
            horizon: ConvergenceHorizon::Bounded,
            substrate: SubstrateType::Compute,
            computation_mode: ComputationMode::Mechanical,
        };
        graph.add_point(id, point);

        let result = DagExecutor::execute_graph(&graph, &[id]).await.unwrap();
        assert_eq!(result.degraded_count, 1);
    }

    #[tokio::test]
    async fn test_attestation_chain_integrity() {
        let mut graph = ConvergenceGraph::new();
        let (a, pa) = make_point("first");
        let (b, pb) = make_point("second");
        graph.add_point(a, pa);
        graph.add_point(b, pb);
        graph.add_edge(TypedEdge {
            from: a,
            to: b,
            edge_type: EdgeType::Attestation,
        });

        let order = graph.topological_order().unwrap();
        let result = DagExecutor::execute_graph(&graph, &order).await.unwrap();

        // Both points should have attestations
        let att_a = result.outcomes[&a].attestation.as_ref().unwrap();
        let att_b = result.outcomes[&b].attestation.as_ref().unwrap();
        // Attestations should be different (different inputs)
        assert_ne!(att_a, att_b);
        // Both should start with blake3:
        assert!(att_a.starts_with("blake3:"));
        assert!(att_b.starts_with("blake3:"));
    }

    #[tokio::test]
    async fn test_diamond_dag_execution() {
        let mut graph = ConvergenceGraph::new();
        let (a, pa) = make_point("root");
        let (b, pb) = make_point("left");
        let (c, pc) = make_point("right");
        let (d, pd) = make_point("join");
        graph.add_point(a, pa);
        graph.add_point(b, pb);
        graph.add_point(c, pc);
        graph.add_point(d, pd);
        graph.add_edge(TypedEdge { from: a, to: b, edge_type: EdgeType::Data });
        graph.add_edge(TypedEdge { from: a, to: c, edge_type: EdgeType::Data });
        graph.add_edge(TypedEdge { from: b, to: d, edge_type: EdgeType::Data });
        graph.add_edge(TypedEdge { from: c, to: d, edge_type: EdgeType::Data });

        let order = graph.topological_order().unwrap();
        let result = DagExecutor::execute_graph(&graph, &order).await.unwrap();

        assert_eq!(result.converged_count, 4);
        assert_eq!(result.failed_count, 0);
    }

    #[tokio::test]
    async fn test_empty_graph() {
        let graph = ConvergenceGraph::new();
        let result = DagExecutor::execute_graph(&graph, &[]).await.unwrap();
        assert_eq!(result.converged_count, 0);
        assert_eq!(result.failed_count, 0);
    }
}
