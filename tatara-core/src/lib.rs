pub mod catalog;
pub mod cluster;
pub mod config;
pub mod domain;

/// Prelude — import everything needed for convergence computing.
///
/// ```rust
/// use tatara_core::prelude::*;
/// ```
pub mod prelude {
    // ── Classification (6 dimensions) ──
    pub use crate::domain::classification::{
        AiInterface, AiRole, CalmClassification, ComputationMode, ConvergenceHorizon,
        ConvergenceMechanism, ConvergenceOutcome, ConvergencePointType, OptimizationDirection,
        SubstrateType,
    };

    // ── Core convergence types ──
    pub use crate::domain::convergence_state::{
        BoundaryCheck, BoundaryPhase, ClusterConvergence, ConvergenceBoundary,
        ConvergenceDistance, ConvergencePoint, ConvergenceState,
    };

    // ── Content-addressed identity ──
    pub use crate::domain::point_id::PointId;

    // ── Typed DAGs ──
    pub use crate::domain::convergence_graph::{
        ConvergenceGraph, ConvergencePlan, EdgeType, GraphError, SubstrateDAG, TypedEdge,
    };

    // ── Compliance ──
    pub use crate::domain::compliance_binding::{
        ComplianceBinding, ComplianceClosure, ComplianceControl, DataClassification,
        PointSelector, ResolvedControl, VerificationPhase,
    };

    // ── Emission (asymptotic → bounded) ──
    pub use crate::domain::emission::{
        BoundedPointTemplate, EmissionSchema, EmissionTrigger, InstantiationDecision,
        TriggerCondition,
    };

    // ── Multi-dimensional distance ──
    pub use crate::domain::multi_distance::{ConvergenceBandwidth, MultiDimensionalDistance};
}
