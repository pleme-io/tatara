//! Convergence planner — pre-execution analysis of convergence graphs.
//!
//! Before any convergence begins, the planner computes the execution order,
//! resolves compliance bindings, identifies cache hits, and determines the
//! critical path. Like `nix build --dry-run` for convergence.

use tatara_core::domain::compliance_binding::*;
use tatara_core::domain::convergence_graph::*;
use tatara_core::domain::convergence_state::*;
use tatara_core::domain::point_id::PointId;

/// Pre-execution analysis of a convergence graph.
pub struct ConvergencePlanner;

impl ConvergencePlanner {
    /// Analyze a convergence graph and produce an execution plan.
    pub fn plan(
        graph: &ConvergenceGraph,
        bindings: &[ComplianceBinding],
    ) -> Result<ConvergencePlan, GraphError> {
        // 1. Validate the graph
        graph.validate()?;

        // 2. Compute topological order
        let execution_order = graph.topological_order()?;

        // 3. Resolve compliance bindings to specific points
        let compliance = Self::resolve_compliance(graph, bindings);

        // 4. Identify cache hits (points with existing attestations)
        let cache_hits = Self::find_cache_hits(graph);

        // 5. Compute critical path (longest sequential chain)
        let critical_path = Self::compute_critical_path(graph, &execution_order);

        Ok(ConvergencePlan {
            graph: graph.clone(),
            execution_order,
            compliance,
            cache_hits,
            critical_path,
        })
    }

    /// Resolve compliance bindings to specific convergence points.
    fn resolve_compliance(
        graph: &ConvergenceGraph,
        bindings: &[ComplianceBinding],
    ) -> ComplianceClosure {
        let mut resolved = Vec::new();
        let mut plan_time = 0usize;
        let mut at_boundary = 0usize;
        let mut post_convergence = 0usize;

        for binding in bindings {
            let matching_points: Vec<PointId> = graph
                .points
                .iter()
                .filter(|(id, point)| {
                    binding.selector.matches(
                        &point.point_type,
                        &point.substrate,
                        id,
                        None,
                        None,
                    )
                })
                .map(|(id, _)| *id)
                .collect();

            if !matching_points.is_empty() {
                match binding.phase {
                    VerificationPhase::PlanTime => plan_time += 1,
                    VerificationPhase::AtBoundary => at_boundary += 1,
                    VerificationPhase::PostConvergence => post_convergence += 1,
                }

                resolved.push(ResolvedControl {
                    control: binding.control.clone(),
                    point_ids: matching_points,
                    phase: binding.phase,
                });
            }
        }

        ComplianceClosure {
            bindings: bindings.to_vec(),
            resolved,
            plan_time_count: plan_time,
            at_boundary_count: at_boundary,
            post_convergence_count: post_convergence,
        }
    }

    /// Find points that can skip re-execution (already attested).
    fn find_cache_hits(graph: &ConvergenceGraph) -> Vec<PointId> {
        graph
            .points
            .iter()
            .filter(|(_, point)| point.boundary.output_attestation.is_some())
            .map(|(id, _)| *id)
            .collect()
    }

    /// Compute the critical path (longest sequential chain).
    fn compute_critical_path(
        graph: &ConvergenceGraph,
        topo_order: &[PointId],
    ) -> Vec<PointId> {
        if topo_order.is_empty() {
            return Vec::new();
        }

        // Dynamic programming: longest path in DAG
        let mut dist: std::collections::HashMap<PointId, (usize, Option<PointId>)> =
            std::collections::HashMap::new();

        for &id in topo_order {
            dist.insert(id, (1, None));
        }

        for &id in topo_order {
            let current_dist = dist[&id].0;
            for edge in &graph.edges {
                if edge.from == id {
                    let neighbor_dist = dist.get(&edge.to).map(|d| d.0).unwrap_or(0);
                    if current_dist + 1 > neighbor_dist {
                        dist.insert(edge.to, (current_dist + 1, Some(id)));
                    }
                }
            }
        }

        // Find the node with maximum distance
        let (&end, _) = dist
            .iter()
            .max_by_key(|(_, (d, _))| *d)
            .unwrap();

        // Trace back the path
        let mut path = vec![end];
        let mut current = end;
        while let Some((_, Some(prev))) = dist.get(&current) {
            path.push(*prev);
            current = *prev;
        }
        path.reverse();
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_point(name: &str, substrate: SubstrateType) -> (PointId, ConvergencePoint) {
        let id = PointId::compute(name.as_bytes(), &[], b"desired");
        let point = ConvergencePoint {
            name: name.into(),
            description: format!("{name} point"),
            monotone: true,
            mechanism: ConvergenceMechanism::Local,
            state: ConvergenceState::new(name),
            boundary: ConvergenceBoundary::default(),
            point_type: ConvergencePointType::Transform,
            horizon: ConvergenceHorizon::Bounded,
            substrate,
            computation_mode: ComputationMode::Mechanical,
        };
        (id, point)
    }

    #[test]
    fn test_plan_simple_graph() {
        let mut graph = ConvergenceGraph::new();
        let (a, pa) = make_point("a", SubstrateType::Compute);
        let (b, pb) = make_point("b", SubstrateType::Network);
        graph.add_point(a, pa);
        graph.add_point(b, pb);
        graph.add_edge(TypedEdge {
            from: a,
            to: b,
            edge_type: EdgeType::Attestation,
        });

        let plan = ConvergencePlanner::plan(&graph, &[]).unwrap();
        assert_eq!(plan.execution_order.len(), 2);
        assert!(plan.cache_hits.is_empty());
    }

    #[test]
    fn test_plan_with_compliance_bindings() {
        let mut graph = ConvergenceGraph::new();
        let (a, pa) = make_point("a", SubstrateType::Security);
        let (b, pb) = make_point("b", SubstrateType::Compute);
        graph.add_point(a, pa);
        graph.add_point(b, pb);

        let bindings = vec![
            ComplianceBinding {
                selector: PointSelector::BySubstrate(SubstrateType::Security),
                control: ComplianceControl {
                    framework: "nist-800-53".into(),
                    control_id: "AC-6".into(),
                    description: "Least privilege".into(),
                },
                phase: VerificationPhase::PlanTime,
            },
            ComplianceBinding {
                selector: PointSelector::All,
                control: ComplianceControl {
                    framework: "nist-800-53".into(),
                    control_id: "AU-2".into(),
                    description: "Audit events".into(),
                },
                phase: VerificationPhase::AtBoundary,
            },
        ];

        let plan = ConvergencePlanner::plan(&graph, &bindings).unwrap();
        assert_eq!(plan.compliance.plan_time_count, 1);
        assert_eq!(plan.compliance.at_boundary_count, 1);
        assert_eq!(plan.compliance.resolved.len(), 2);

        // AC-6 should only match the Security point
        let ac6 = plan.compliance.resolved.iter().find(|r| r.control.control_id == "AC-6").unwrap();
        assert_eq!(ac6.point_ids.len(), 1);

        // AU-2 should match both points (All selector)
        let au2 = plan.compliance.resolved.iter().find(|r| r.control.control_id == "AU-2").unwrap();
        assert_eq!(au2.point_ids.len(), 2);
    }

    #[test]
    fn test_critical_path_linear() {
        let mut graph = ConvergenceGraph::new();
        let (a, pa) = make_point("a", SubstrateType::Compute);
        let (b, pb) = make_point("b", SubstrateType::Compute);
        let (c, pc) = make_point("c", SubstrateType::Compute);
        graph.add_point(a, pa);
        graph.add_point(b, pb);
        graph.add_point(c, pc);
        graph.add_edge(TypedEdge { from: a, to: b, edge_type: EdgeType::Data });
        graph.add_edge(TypedEdge { from: b, to: c, edge_type: EdgeType::Data });

        let plan = ConvergencePlanner::plan(&graph, &[]).unwrap();
        assert_eq!(plan.critical_path.len(), 3);
    }

    #[test]
    fn test_critical_path_diamond() {
        let mut graph = ConvergenceGraph::new();
        let (a, pa) = make_point("root", SubstrateType::Compute);
        let (b, pb) = make_point("left", SubstrateType::Compute);
        let (c, pc) = make_point("right", SubstrateType::Compute);
        let (d, pd) = make_point("join", SubstrateType::Compute);
        graph.add_point(a, pa);
        graph.add_point(b, pb);
        graph.add_point(c, pc);
        graph.add_point(d, pd);
        graph.add_edge(TypedEdge { from: a, to: b, edge_type: EdgeType::Data });
        graph.add_edge(TypedEdge { from: a, to: c, edge_type: EdgeType::Data });
        graph.add_edge(TypedEdge { from: b, to: d, edge_type: EdgeType::Data });
        graph.add_edge(TypedEdge { from: c, to: d, edge_type: EdgeType::Data });

        let plan = ConvergencePlanner::plan(&graph, &[]).unwrap();
        // Critical path should be length 3 (root → left/right → join)
        assert_eq!(plan.critical_path.len(), 3);
    }

    #[test]
    fn test_cache_hits() {
        let mut graph = ConvergenceGraph::new();
        let (a, mut pa) = make_point("a", SubstrateType::Compute);
        pa.boundary.output_attestation = Some("blake3:cached".into());
        let (b, pb) = make_point("b", SubstrateType::Compute);
        graph.add_point(a, pa);
        graph.add_point(b, pb);

        let plan = ConvergencePlanner::plan(&graph, &[]).unwrap();
        assert_eq!(plan.cache_hits.len(), 1);
        assert!(plan.cache_hits.contains(&a));
    }

    #[test]
    fn test_invalid_graph_fails() {
        let mut graph = ConvergenceGraph::new();
        let (a, pa) = make_point("a", SubstrateType::Compute);
        let (b, pb) = make_point("b", SubstrateType::Compute);
        graph.add_point(a, pa);
        graph.add_point(b, pb);
        graph.add_edge(TypedEdge { from: a, to: b, edge_type: EdgeType::Data });
        graph.add_edge(TypedEdge { from: b, to: a, edge_type: EdgeType::Data });

        assert!(ConvergencePlanner::plan(&graph, &[]).is_err());
    }
}
