//! Convergence attestation — binds tameshi CertificationArtifact to
//! convergence boundary phases.
//!
//! Every convergence point's Attest phase produces a three-pillar binding:
//!   artifact_hash  = blake3(convergence function output)
//!   control_hash   = blake3(compliance verification result)
//!   intent_hash    = blake3(Nix desired state)
//!
//! This module provides the attestation logic that the DagExecutor calls
//! during the Attest boundary phase.

use serde::{Deserialize, Serialize};

/// A convergence attestation — the three-pillar CertificationArtifact
/// produced by each convergence point's boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergenceAttestation {
    /// Blake3 hash of the convergence function output state.
    pub artifact_hash: String,
    /// Blake3 hash of compliance verification results (if any).
    pub control_hash: Option<String>,
    /// Blake3 hash of the Nix-declared desired state.
    pub intent_hash: String,
    /// Composed root = blake3(artifact || control || intent).
    pub composed_root: String,
    /// Generation counter (monotonic per re-convergence).
    pub generation: u64,
    /// Previous generation's composed_root (append-only chain).
    pub previous_root: Option<String>,
}

impl ConvergenceAttestation {
    /// Produce a new attestation from the three pillars.
    pub fn produce(
        artifact_data: &[u8],
        control_data: Option<&[u8]>,
        intent_data: &[u8],
        generation: u64,
        previous_root: Option<String>,
    ) -> Self {
        let artifact_hash = format!("blake3:{}", blake3::hash(artifact_data));
        let control_hash = control_data.map(|d| format!("blake3:{}", blake3::hash(d)));
        let intent_hash = format!("blake3:{}", blake3::hash(intent_data));

        // Compose the three pillars into a single root
        let mut hasher = blake3::Hasher::new();
        hasher.update(artifact_hash.as_bytes());
        if let Some(ref ch) = control_hash {
            hasher.update(ch.as_bytes());
        }
        hasher.update(intent_hash.as_bytes());
        if let Some(ref prev) = previous_root {
            hasher.update(prev.as_bytes());
        }
        let composed_root = format!("blake3:{}", hasher.finalize());

        Self {
            artifact_hash,
            control_hash,
            intent_hash,
            composed_root,
            generation,
            previous_root,
        }
    }

    /// Verify the composed root is correct given the three pillars.
    pub fn verify(&self) -> bool {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.artifact_hash.as_bytes());
        if let Some(ref ch) = self.control_hash {
            hasher.update(ch.as_bytes());
        }
        hasher.update(self.intent_hash.as_bytes());
        if let Some(ref prev) = self.previous_root {
            hasher.update(prev.as_bytes());
        }
        let expected = format!("blake3:{}", hasher.finalize());
        self.composed_root == expected
    }
}

/// Compliance verification result that feeds into the control_hash pillar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceResult {
    /// Framework that was verified.
    pub framework: String,
    /// Controls that were checked.
    pub controls_checked: Vec<String>,
    /// Controls that passed.
    pub controls_passed: Vec<String>,
    /// Controls that failed.
    pub controls_failed: Vec<String>,
    /// Whether all controls passed.
    pub all_passed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_produce_attestation() {
        let att = ConvergenceAttestation::produce(
            b"workload running",
            Some(b"nist ac-6 passed"),
            b"desired: { replicas: 3 }",
            1,
            None,
        );
        assert!(att.artifact_hash.starts_with("blake3:"));
        assert!(att.control_hash.as_ref().unwrap().starts_with("blake3:"));
        assert!(att.intent_hash.starts_with("blake3:"));
        assert!(att.composed_root.starts_with("blake3:"));
        assert_eq!(att.generation, 1);
    }

    #[test]
    fn test_verify_attestation() {
        let att = ConvergenceAttestation::produce(
            b"artifact",
            Some(b"controls"),
            b"intent",
            0,
            None,
        );
        assert!(att.verify());
    }

    #[test]
    fn test_tampered_attestation_fails_verify() {
        let mut att = ConvergenceAttestation::produce(
            b"artifact",
            Some(b"controls"),
            b"intent",
            0,
            None,
        );
        att.artifact_hash = "blake3:tampered".into();
        assert!(!att.verify());
    }

    #[test]
    fn test_generational_chain() {
        let gen0 = ConvergenceAttestation::produce(
            b"v1",
            None,
            b"intent",
            0,
            None,
        );
        let gen1 = ConvergenceAttestation::produce(
            b"v2",
            None,
            b"intent",
            1,
            Some(gen0.composed_root.clone()),
        );
        assert!(gen1.verify());
        assert_eq!(gen1.previous_root.as_deref(), Some(gen0.composed_root.as_str()));
        assert_ne!(gen0.composed_root, gen1.composed_root);
    }

    #[test]
    fn test_no_compliance() {
        let att = ConvergenceAttestation::produce(
            b"artifact",
            None,
            b"intent",
            0,
            None,
        );
        assert!(att.control_hash.is_none());
        assert!(att.verify());
    }

    #[test]
    fn test_deterministic() {
        let a = ConvergenceAttestation::produce(b"x", Some(b"y"), b"z", 0, None);
        let b = ConvergenceAttestation::produce(b"x", Some(b"y"), b"z", 0, None);
        assert_eq!(a.composed_root, b.composed_root);
    }

    #[test]
    fn test_compliance_result() {
        let result = ComplianceResult {
            framework: "nist-800-53".into(),
            controls_checked: vec!["AC-6".into(), "AU-2".into()],
            controls_passed: vec!["AC-6".into(), "AU-2".into()],
            controls_failed: vec![],
            all_passed: true,
        };
        assert!(result.all_passed);
        assert_eq!(result.controls_checked.len(), 2);
    }
}
