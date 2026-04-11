//! Type-level compliance bindings.
//!
//! Controls bind to convergence point TYPES, not instances. "All Security
//! substrate points must satisfy NIST AC-6" is a type-level constraint
//! verified at the phase specified by the binding.

use serde::{Deserialize, Serialize};

use super::convergence_state::{ConvergencePointType, SubstrateType};
use super::point_id::PointId;

/// A compliance control bound to convergence point types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceBinding {
    /// What convergence points this control applies to.
    pub selector: PointSelector,
    /// The compliance control to verify.
    pub control: ComplianceControl,
    /// When this control is verified.
    pub phase: VerificationPhase,
}

/// Selects which convergence points a compliance control applies to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointSelector {
    /// All points of a specific structural type.
    ByType(ConvergencePointType),
    /// All points on a specific substrate.
    BySubstrate(SubstrateType),
    /// Points matching both substrate and type.
    BySubstrateAndType(SubstrateType, ConvergencePointType),
    /// All points in a specific environment.
    ByEnvironment(String),
    /// All points handling data of a specific classification.
    ByDataClassification(DataClassification),
    /// A specific point by ID.
    ById(PointId),
    /// All convergence points (universal control).
    All,
}

/// When compliance is verified relative to convergence execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationPhase {
    /// Verified at plan time by static analysis (zero cost).
    PlanTime,
    /// Verified inline during the convergence boundary.
    AtBoundary,
    /// Verified after convergence via live probes.
    PostConvergence,
}

/// A specific compliance control from a framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceControl {
    /// Framework name (e.g., "nist-800-53", "soc2", "fedramp").
    pub framework: String,
    /// Control identifier (e.g., "AC-6", "CC6.1", "3.4").
    pub control_id: String,
    /// Human-readable description.
    pub description: String,
}

/// Data classification for compliance purposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClassification {
    /// Personally Identifiable Information.
    Pii,
    /// Protected Health Information.
    Phi,
    /// Payment Card Industry data.
    Pci,
    /// Public data.
    Public,
    /// Internal data.
    Internal,
    /// Confidential data.
    Confidential,
}

/// The complete set of compliance controls bound to a convergence DAG.
/// Computed at plan time before any execution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComplianceClosure {
    /// All bindings that apply to this DAG.
    pub bindings: Vec<ComplianceBinding>,
    /// Resolved: which specific points each control applies to.
    pub resolved: Vec<ResolvedControl>,
    /// Count of controls verifiable at plan time (zero cost).
    pub plan_time_count: usize,
    /// Count of controls verifiable at boundary (inline).
    pub at_boundary_count: usize,
    /// Count of controls verifiable post-convergence (live probes).
    pub post_convergence_count: usize,
}

/// A compliance control resolved to specific convergence points.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedControl {
    /// The control being verified.
    pub control: ComplianceControl,
    /// The points this control applies to.
    pub point_ids: Vec<PointId>,
    /// When it's verified.
    pub phase: VerificationPhase,
}

impl PointSelector {
    /// Check if this selector matches a point with the given attributes.
    pub fn matches(
        &self,
        point_type: &ConvergencePointType,
        substrate: &SubstrateType,
        point_id: &PointId,
        environment: Option<&str>,
        data_class: Option<&DataClassification>,
    ) -> bool {
        match self {
            Self::ByType(t) => t == point_type,
            Self::BySubstrate(s) => s == substrate,
            Self::BySubstrateAndType(s, t) => s == substrate && t == point_type,
            Self::ByEnvironment(env) => environment == Some(env.as_str()),
            Self::ByDataClassification(dc) => data_class == Some(dc),
            Self::ById(id) => id == point_id,
            Self::All => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_point_id() -> PointId {
        PointId::compute(b"test", &[], b"state")
    }

    #[test]
    fn test_selector_all_matches_everything() {
        let sel = PointSelector::All;
        assert!(sel.matches(
            &ConvergencePointType::Transform,
            &SubstrateType::Compute,
            &sample_point_id(),
            None,
            None,
        ));
    }

    #[test]
    fn test_selector_by_substrate() {
        let sel = PointSelector::BySubstrate(SubstrateType::Security);
        assert!(sel.matches(
            &ConvergencePointType::Gate,
            &SubstrateType::Security,
            &sample_point_id(),
            None,
            None,
        ));
        assert!(!sel.matches(
            &ConvergencePointType::Gate,
            &SubstrateType::Compute,
            &sample_point_id(),
            None,
            None,
        ));
    }

    #[test]
    fn test_selector_by_type() {
        let sel = PointSelector::ByType(ConvergencePointType::Gate);
        assert!(sel.matches(
            &ConvergencePointType::Gate,
            &SubstrateType::Compute,
            &sample_point_id(),
            None,
            None,
        ));
        assert!(!sel.matches(
            &ConvergencePointType::Transform,
            &SubstrateType::Compute,
            &sample_point_id(),
            None,
            None,
        ));
    }

    #[test]
    fn test_selector_by_data_classification() {
        let sel = PointSelector::ByDataClassification(DataClassification::Pii);
        assert!(sel.matches(
            &ConvergencePointType::Transform,
            &SubstrateType::Storage,
            &sample_point_id(),
            None,
            Some(&DataClassification::Pii),
        ));
        assert!(!sel.matches(
            &ConvergencePointType::Transform,
            &SubstrateType::Storage,
            &sample_point_id(),
            None,
            Some(&DataClassification::Public),
        ));
    }

    #[test]
    fn test_verification_phase_serde() {
        for phase in [
            VerificationPhase::PlanTime,
            VerificationPhase::AtBoundary,
            VerificationPhase::PostConvergence,
        ] {
            let json = serde_json::to_string(&phase).unwrap();
            let parsed: VerificationPhase = serde_json::from_str(&json).unwrap();
            assert_eq!(phase, parsed);
        }
    }

    #[test]
    fn test_compliance_binding_serde() {
        let binding = ComplianceBinding {
            selector: PointSelector::BySubstrate(SubstrateType::Security),
            control: ComplianceControl {
                framework: "nist-800-53".into(),
                control_id: "AC-6".into(),
                description: "Least privilege".into(),
            },
            phase: VerificationPhase::PlanTime,
        };
        let json = serde_json::to_string(&binding).unwrap();
        let _: ComplianceBinding = serde_json::from_str(&json).unwrap();
    }
}
