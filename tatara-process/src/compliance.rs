//! Compliance bindings — CRD-facing with bridges to `tatara_core::compliance_binding`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use tatara_core::domain::compliance_binding as core;

/// Compliance section of `ProcessSpec`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceSpec {
    /// Canonical baseline (e.g., `fedramp-moderate`, `cis-k8s-v1.8`, `soc2`, `pci-dss`).
    /// Semantically the `meet` of all `bindings`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<String>,
    /// Individual control bindings.
    #[serde(default)]
    pub bindings: Vec<ComplianceBinding>,
    /// Allow the reconciler to invoke remediation hooks on violations.
    #[serde(default)]
    pub auto_remediate: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceBinding {
    /// Framework name: `nist-800-53`, `cis-k8s-v1.8`, `fedramp-moderate`, `soc2`, `pci-dss`.
    pub framework: String,
    /// Control id within the framework (e.g., `SC-7`, `5.1.1`).
    pub control_id: String,
    /// When the binding is verified.
    #[serde(default)]
    pub phase: VerificationPhase,
    /// Optional human description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// When a ComplianceBinding is evaluated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
pub enum VerificationPhase {
    /// Before Execing — fails reconciliation if violated.
    PlanTime,
    /// During VERIFY — gates Running → Attested.
    #[default]
    AtBoundary,
    /// After Attested — continuous audit, emits events on violation.
    PostConvergence,
}

impl From<VerificationPhase> for core::VerificationPhase {
    fn from(v: VerificationPhase) -> Self {
        match v {
            VerificationPhase::PlanTime => Self::PlanTime,
            VerificationPhase::AtBoundary => Self::AtBoundary,
            VerificationPhase::PostConvergence => Self::PostConvergence,
        }
    }
}

impl From<core::VerificationPhase> for VerificationPhase {
    fn from(v: core::VerificationPhase) -> Self {
        use core::VerificationPhase as C;
        match v {
            C::PlanTime => Self::PlanTime,
            C::AtBoundary => Self::AtBoundary,
            C::PostConvergence => Self::PostConvergence,
        }
    }
}

impl ComplianceBinding {
    pub fn to_core(&self) -> core::ComplianceControl {
        core::ComplianceControl {
            framework: self.framework.clone(),
            control_id: self.control_id.clone(),
            description: self.description.clone().unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_phase_is_at_boundary() {
        assert_eq!(VerificationPhase::default(), VerificationPhase::AtBoundary);
    }

    #[test]
    fn binding_roundtrip_to_core() {
        let b = ComplianceBinding {
            framework: "nist-800-53".into(),
            control_id: "SC-7".into(),
            phase: VerificationPhase::AtBoundary,
            description: Some("boundary protection".into()),
        };
        let c = b.to_core();
        assert_eq!(c.framework, "nist-800-53");
        assert_eq!(c.control_id, "SC-7");
    }
}
