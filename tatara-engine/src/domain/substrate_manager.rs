//! Substrate manager — manages per-substrate convergence DAGs.
//!
//! Each operational substrate (financial, compute, network, etc.) has its
//! own convergence DAG. The substrate manager composes them into a single
//! ConvergenceGraph and computes per-substrate convergence distances.

use std::collections::BTreeMap;

use tatara_core::domain::convergence_graph::*;
use tatara_core::domain::convergence_state::*;
use tatara_core::domain::multi_distance::*;
use tatara_core::domain::point_id::PointId;

/// Manages multiple substrate-scoped convergence DAGs and composes them
/// into a unified convergence graph.
pub struct SubstrateManager {
    substrates: BTreeMap<SubstrateType, SubstrateDAG>,
}

impl SubstrateManager {
    pub fn new() -> Self {
        Self {
            substrates: BTreeMap::new(),
        }
    }

    /// Add a substrate DAG.
    pub fn add_substrate(&mut self, dag: SubstrateDAG) {
        self.substrates.insert(dag.substrate, dag);
    }

    /// Get a substrate DAG by type.
    pub fn get_substrate(&self, substrate: &SubstrateType) -> Option<&SubstrateDAG> {
        self.substrates.get(substrate)
    }

    /// Number of managed substrates.
    pub fn substrate_count(&self) -> usize {
        self.substrates.len()
    }

    /// Compose all substrate DAGs into a single convergence graph.
    /// Internal edges stay within substrates. Cross-substrate edges
    /// connect boundary points between substrates.
    pub fn compose_graph(&self) -> ConvergenceGraph {
        let mut graph = ConvergenceGraph::new();

        for dag in self.substrates.values() {
            // Add all points from this substrate
            for (id, point) in &dag.points {
                graph.add_point(*id, point.clone());
            }
            // Add internal edges
            for edge in &dag.internal_edges {
                graph.add_edge(edge.clone());
            }
            // Add cross-substrate edges
            for edge in &dag.cross_edges {
                graph.add_edge(edge.clone());
            }
        }

        graph
    }

    /// Compute per-substrate convergence distance.
    pub fn convergence_per_substrate(&self) -> MultiDimensionalDistance {
        let mut distance = MultiDimensionalDistance::new();

        for (substrate_type, dag) in &self.substrates {
            if dag.points.is_empty() {
                distance.set(*substrate_type, 0.0);
                continue;
            }

            let max_distance: f64 = dag
                .points
                .values()
                .map(|p| p.state.distance.numeric())
                .fold(0.0_f64, f64::max);
            distance.set(*substrate_type, max_distance);
        }

        distance
    }

    /// List cross-substrate dependencies.
    pub fn cross_substrate_edges(&self) -> Vec<(SubstrateType, SubstrateType, TypedEdge)> {
        let mut result = Vec::new();

        for (substrate_type, dag) in &self.substrates {
            for edge in &dag.cross_edges {
                // Determine which substrate the target belongs to
                for (other_type, other_dag) in &self.substrates {
                    if other_type != substrate_type && other_dag.points.contains_key(&edge.to) {
                        result.push((*substrate_type, *other_type, edge.clone()));
                    }
                }
            }
        }

        result
    }
}

impl Default for SubstrateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_substrate_dag(substrate: SubstrateType, point_names: &[&str]) -> SubstrateDAG {
        let mut points = BTreeMap::new();
        let mut ids = Vec::new();

        for name in point_names {
            let id = PointId::compute(name.as_bytes(), &[], b"desired");
            let point = ConvergencePoint {
                name: (*name).into(),
                description: format!("{name} point"),
                monotone: true,
                mechanism: ConvergenceMechanism::Local,
                state: ConvergenceState::new(*name),
                boundary: ConvergenceBoundary::default(),
                point_type: ConvergencePointType::Transform,
                horizon: ConvergenceHorizon::Bounded,
                substrate,
                computation_mode: ComputationMode::Mechanical,
            };
            points.insert(id, point);
            ids.push(id);
        }

        let mut internal_edges = Vec::new();
        for i in 1..ids.len() {
            internal_edges.push(TypedEdge {
                from: ids[i - 1],
                to: ids[i],
                edge_type: EdgeType::Attestation,
            });
        }

        SubstrateDAG {
            substrate,
            points,
            internal_edges,
            cross_edges: Vec::new(),
            bandwidth: ConvergenceBandwidth::Seconds(30),
        }
    }

    #[test]
    fn test_single_substrate() {
        let mut mgr = SubstrateManager::new();
        mgr.add_substrate(make_substrate_dag(
            SubstrateType::Compute,
            &["cpu_alloc", "mem_alloc", "driver_start"],
        ));

        assert_eq!(mgr.substrate_count(), 1);
        let graph = mgr.compose_graph();
        assert_eq!(graph.point_count(), 3);
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn test_multi_substrate_composition() {
        let mut mgr = SubstrateManager::new();
        mgr.add_substrate(make_substrate_dag(SubstrateType::Compute, &["cpu", "mem"]));
        mgr.add_substrate(make_substrate_dag(
            SubstrateType::Network,
            &["dns", "route"],
        ));
        mgr.add_substrate(make_substrate_dag(SubstrateType::Security, &["secret"]));

        assert_eq!(mgr.substrate_count(), 3);
        let graph = mgr.compose_graph();
        assert_eq!(graph.point_count(), 5);
    }

    #[test]
    fn test_convergence_per_substrate() {
        let mut mgr = SubstrateManager::new();
        mgr.add_substrate(make_substrate_dag(SubstrateType::Compute, &["a"]));
        mgr.add_substrate(make_substrate_dag(SubstrateType::Network, &["b"]));

        let dist = mgr.convergence_per_substrate();
        // Default ConvergenceState has Unknown distance (1.0)
        assert_eq!(dist.get(&SubstrateType::Compute), 1.0);
        assert_eq!(dist.get(&SubstrateType::Network), 1.0);
        assert!(!dist.is_converged());
    }

    #[test]
    fn test_empty_manager() {
        let mgr = SubstrateManager::new();
        assert_eq!(mgr.substrate_count(), 0);
        let graph = mgr.compose_graph();
        assert_eq!(graph.point_count(), 0);
    }
}
