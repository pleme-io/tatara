//! Typed convergence DAGs.
//!
//! A convergence graph is a DAG of typed convergence points with typed edges.
//! Points are content-addressed via PointId. The graph supports topological
//! ordering, substrate filtering, and validation.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use super::compliance_binding::ComplianceClosure;
use super::convergence_state::{ConvergencePoint, SubstrateType};
use super::multi_distance::ConvergenceBandwidth;
use super::point_id::PointId;

/// The type of relationship between two convergence points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    /// Data flows from one point to another.
    Data,
    /// Control dependency (must complete before next can start).
    Control,
    /// Attestation chain (output attestation feeds input attestation).
    Attestation,
}

/// A typed edge between two convergence points.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedEdge {
    /// Source point.
    pub from: PointId,
    /// Target point.
    pub to: PointId,
    /// Type of relationship.
    pub edge_type: EdgeType,
}

/// The complete convergence graph across all substrates.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConvergenceGraph {
    /// All points, keyed by hex-encoded PointId.
    /// (BTreeMap keys must be strings for JSON serialization.)
    #[serde(with = "point_id_map")]
    pub points: BTreeMap<PointId, ConvergencePoint>,
    /// Typed edges between points.
    pub edges: Vec<TypedEdge>,
}

mod point_id_map {
    use super::*;
    use serde::de::{self, MapAccess, Visitor};
    use serde::ser::SerializeMap;

    pub fn serialize<S>(
        map: &BTreeMap<PointId, ConvergencePoint>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut m = serializer.serialize_map(Some(map.len()))?;
        for (k, v) in map {
            m.serialize_entry(&k.to_hex(), v)?;
        }
        m.end()
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<BTreeMap<PointId, ConvergencePoint>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PointIdMapVisitor;
        impl<'de> Visitor<'de> for PointIdMapVisitor {
            type Value = BTreeMap<PointId, ConvergencePoint>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a map with hex PointId keys")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut access: M) -> Result<Self::Value, M::Error> {
                let mut map = BTreeMap::new();
                while let Some((key, value)) = access.next_entry::<String, ConvergencePoint>()? {
                    let id = PointId::from_hex(&key).map_err(de::Error::custom)?;
                    map.insert(id, value);
                }
                Ok(map)
            }
        }
        deserializer.deserialize_map(PointIdMapVisitor)
    }
}

impl ConvergenceGraph {
    /// Create an empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a convergence point, returning its PointId.
    pub fn add_point(&mut self, id: PointId, point: ConvergencePoint) {
        self.points.insert(id, point);
    }

    /// Add a typed edge between two points.
    pub fn add_edge(&mut self, edge: TypedEdge) {
        self.edges.push(edge);
    }

    /// Compute topological order using Kahn's algorithm.
    /// Returns Err if the graph contains a cycle.
    pub fn topological_order(&self) -> Result<Vec<PointId>, GraphError> {
        let mut in_degree: HashMap<PointId, usize> = HashMap::new();
        let mut adjacency: HashMap<PointId, Vec<PointId>> = HashMap::new();

        for id in self.points.keys() {
            in_degree.entry(*id).or_insert(0);
            adjacency.entry(*id).or_default();
        }

        for edge in &self.edges {
            *in_degree.entry(edge.to).or_insert(0) += 1;
            adjacency.entry(edge.from).or_default().push(edge.to);
        }

        let mut queue: VecDeque<PointId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| *id)
            .collect();

        let mut order = Vec::new();

        while let Some(id) = queue.pop_front() {
            order.push(id);
            if let Some(neighbors) = adjacency.get(&id) {
                for neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(*neighbor);
                        }
                    }
                }
            }
        }

        if order.len() != self.points.len() {
            Err(GraphError::CycleDetected)
        } else {
            Ok(order)
        }
    }

    /// Validate the graph: all edge endpoints exist, no cycles.
    pub fn validate(&self) -> Result<(), GraphError> {
        for edge in &self.edges {
            if !self.points.contains_key(&edge.from) {
                return Err(GraphError::MissingPoint(edge.from));
            }
            if !self.points.contains_key(&edge.to) {
                return Err(GraphError::MissingPoint(edge.to));
            }
        }
        self.topological_order()?;
        Ok(())
    }

    /// Filter points by substrate type.
    pub fn points_by_substrate(&self, substrate: &SubstrateType) -> Vec<(&PointId, &ConvergencePoint)> {
        self.points
            .iter()
            .filter(|(_, p)| &p.substrate == substrate)
            .collect()
    }

    /// Get the number of points.
    pub fn point_count(&self) -> usize {
        self.points.len()
    }

    /// Get the number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Compute forward closure: all points this point transitively depends on.
    /// Walks backward along edges (from → to) to find all upstream dependencies.
    /// Like `nix-store --query --requisites`.
    pub fn forward_closure(&self, point_id: &PointId) -> BTreeSet<PointId> {
        let mut adjacency: HashMap<PointId, Vec<PointId>> = HashMap::new();
        for edge in &self.edges {
            adjacency.entry(edge.to).or_default().push(edge.from);
        }

        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*point_id);

        while let Some(id) = queue.pop_front() {
            if !visited.insert(id) {
                continue;
            }
            if let Some(deps) = adjacency.get(&id) {
                for dep in deps {
                    queue.push_back(*dep);
                }
            }
        }

        visited.remove(point_id);
        visited
    }

    /// Compute reverse closure: all points that transitively depend on this point.
    /// Walks forward along edges (from → to) to find all downstream dependents.
    /// Like `nix-store --query --referrers` (transitive).
    pub fn reverse_closure(&self, point_id: &PointId) -> BTreeSet<PointId> {
        let mut adjacency: HashMap<PointId, Vec<PointId>> = HashMap::new();
        for edge in &self.edges {
            adjacency.entry(edge.from).or_default().push(edge.to);
        }

        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*point_id);

        while let Some(id) = queue.pop_front() {
            if !visited.insert(id) {
                continue;
            }
            if let Some(deps) = adjacency.get(&id) {
                for dep in deps {
                    queue.push_back(*dep);
                }
            }
        }

        visited.remove(point_id);
        visited
    }
}

/// A substrate-scoped subgraph with cross-substrate boundary edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubstrateDAG {
    /// The substrate this DAG belongs to.
    pub substrate: SubstrateType,
    /// Points in this substrate.
    pub points: BTreeMap<PointId, ConvergencePoint>,
    /// Edges within this substrate.
    pub internal_edges: Vec<TypedEdge>,
    /// Edges crossing to other substrates.
    pub cross_edges: Vec<TypedEdge>,
    /// Maximum convergence velocity for this substrate.
    pub bandwidth: ConvergenceBandwidth,
}

/// Pre-execution analysis of a convergence graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergencePlan {
    /// The graph being planned.
    pub graph: ConvergenceGraph,
    /// Topological execution order.
    pub execution_order: Vec<PointId>,
    /// Compliance bindings resolved to specific points.
    pub compliance: ComplianceClosure,
    /// Points that can skip re-execution (attestation unchanged).
    pub cache_hits: Vec<PointId>,
    /// Longest sequential dependency chain.
    pub critical_path: Vec<PointId>,
}

/// Errors in convergence graph operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum GraphError {
    #[error("cycle detected in convergence graph")]
    CycleDetected,
    #[error("edge references missing point: {0}")]
    MissingPoint(PointId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::convergence_state::*;

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
    fn test_empty_graph() {
        let g = ConvergenceGraph::new();
        assert_eq!(g.point_count(), 0);
        assert!(g.validate().is_ok());
        assert!(g.topological_order().unwrap().is_empty());
    }

    #[test]
    fn test_single_point() {
        let mut g = ConvergenceGraph::new();
        let (id, p) = make_point("a", SubstrateType::Compute);
        g.add_point(id, p);
        assert_eq!(g.point_count(), 1);
        assert!(g.validate().is_ok());
        assert_eq!(g.topological_order().unwrap(), vec![id]);
    }

    #[test]
    fn test_linear_chain() {
        let mut g = ConvergenceGraph::new();
        let (a, pa) = make_point("a", SubstrateType::Compute);
        let (b, pb) = make_point("b", SubstrateType::Compute);
        let (c, pc) = make_point("c", SubstrateType::Compute);
        g.add_point(a, pa);
        g.add_point(b, pb);
        g.add_point(c, pc);
        g.add_edge(TypedEdge { from: a, to: b, edge_type: EdgeType::Attestation });
        g.add_edge(TypedEdge { from: b, to: c, edge_type: EdgeType::Attestation });

        let order = g.topological_order().unwrap();
        assert_eq!(order.len(), 3);
        let pos_a = order.iter().position(|x| *x == a).unwrap();
        let pos_b = order.iter().position(|x| *x == b).unwrap();
        let pos_c = order.iter().position(|x| *x == c).unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_cycle_detection() {
        let mut g = ConvergenceGraph::new();
        let (a, pa) = make_point("a", SubstrateType::Compute);
        let (b, pb) = make_point("b", SubstrateType::Compute);
        g.add_point(a, pa);
        g.add_point(b, pb);
        g.add_edge(TypedEdge { from: a, to: b, edge_type: EdgeType::Data });
        g.add_edge(TypedEdge { from: b, to: a, edge_type: EdgeType::Data });

        assert!(matches!(g.topological_order(), Err(GraphError::CycleDetected)));
    }

    #[test]
    fn test_missing_point_validation() {
        let mut g = ConvergenceGraph::new();
        let (a, pa) = make_point("a", SubstrateType::Compute);
        let missing = PointId::compute(b"missing", &[], b"state");
        g.add_point(a, pa);
        g.add_edge(TypedEdge { from: a, to: missing, edge_type: EdgeType::Data });

        assert!(matches!(g.validate(), Err(GraphError::MissingPoint(_))));
    }

    #[test]
    fn test_points_by_substrate() {
        let mut g = ConvergenceGraph::new();
        let (a, pa) = make_point("a", SubstrateType::Compute);
        let (b, pb) = make_point("b", SubstrateType::Security);
        let (c, pc) = make_point("c", SubstrateType::Compute);
        g.add_point(a, pa);
        g.add_point(b, pb);
        g.add_point(c, pc);

        assert_eq!(g.points_by_substrate(&SubstrateType::Compute).len(), 2);
        assert_eq!(g.points_by_substrate(&SubstrateType::Security).len(), 1);
        assert_eq!(g.points_by_substrate(&SubstrateType::Financial).len(), 0);
    }

    #[test]
    fn test_forward_closure() {
        let mut g = ConvergenceGraph::new();
        let (a, pa) = make_point("a", SubstrateType::Compute);
        let (b, pb) = make_point("b", SubstrateType::Compute);
        let (c, pc) = make_point("c", SubstrateType::Compute);
        g.add_point(a, pa);
        g.add_point(b, pb);
        g.add_point(c, pc);
        g.add_edge(TypedEdge { from: a, to: b, edge_type: EdgeType::Data });
        g.add_edge(TypedEdge { from: b, to: c, edge_type: EdgeType::Data });

        let closure = g.forward_closure(&c);
        assert!(closure.contains(&a));
        assert!(closure.contains(&b));
        assert!(!closure.contains(&c));
    }

    #[test]
    fn test_reverse_closure() {
        let mut g = ConvergenceGraph::new();
        let (a, pa) = make_point("a", SubstrateType::Compute);
        let (b, pb) = make_point("b", SubstrateType::Compute);
        let (c, pc) = make_point("c", SubstrateType::Compute);
        g.add_point(a, pa);
        g.add_point(b, pb);
        g.add_point(c, pc);
        g.add_edge(TypedEdge { from: a, to: b, edge_type: EdgeType::Data });
        g.add_edge(TypedEdge { from: b, to: c, edge_type: EdgeType::Data });

        let closure = g.reverse_closure(&a);
        assert!(closure.contains(&b));
        assert!(closure.contains(&c));
        assert!(!closure.contains(&a));
    }

    #[test]
    fn test_diamond_dag() {
        let mut g = ConvergenceGraph::new();
        let (a, pa) = make_point("a", SubstrateType::Compute);
        let (b, pb) = make_point("b", SubstrateType::Compute);
        let (c, pc) = make_point("c", SubstrateType::Compute);
        let (d, pd) = make_point("d", SubstrateType::Compute);
        g.add_point(a, pa);
        g.add_point(b, pb);
        g.add_point(c, pc);
        g.add_point(d, pd);
        g.add_edge(TypedEdge { from: a, to: b, edge_type: EdgeType::Data });
        g.add_edge(TypedEdge { from: a, to: c, edge_type: EdgeType::Data });
        g.add_edge(TypedEdge { from: b, to: d, edge_type: EdgeType::Data });
        g.add_edge(TypedEdge { from: c, to: d, edge_type: EdgeType::Data });

        let order = g.topological_order().unwrap();
        assert_eq!(order.len(), 4);
        let pos_a = order.iter().position(|x| *x == a).unwrap();
        let pos_d = order.iter().position(|x| *x == d).unwrap();
        assert!(pos_a < pos_d);
    }

    #[test]
    fn test_graph_serde() {
        let mut g = ConvergenceGraph::new();
        let (a, pa) = make_point("a", SubstrateType::Compute);
        let (b, pb) = make_point("b", SubstrateType::Network);
        g.add_point(a, pa);
        g.add_point(b, pb);
        g.add_edge(TypedEdge { from: a, to: b, edge_type: EdgeType::Control });

        let json = serde_json::to_string(&g).unwrap();
        let parsed: ConvergenceGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.point_count(), 2);
        assert_eq!(parsed.edge_count(), 1);
    }
}
