//! Three-pillar BLAKE3 attestation — wire-compatible with
//! `tatara_engine::domain::attestation::ConvergenceAttestation`.
//!
//! Every BLAKE3-side operation (compose, verify) rides through the
//! substrate primitive [`crate::three_pillar`] — pre-lift the same
//! domain-tagged chain + constant-time comparator lived at TWO
//! sites (here + `crate::receipt`), each with its own private
//! `DOMAIN_TAG`, `composed_hex`/`compose_root` fn, and
//! `constant_time_eq` body. Post-lift the theorem-critical
//! composition + the comparator live at ONE substrate owner so a
//! future CRD-version bump or a comparator normalization lands at
//! one edit rather than two silently divergent ones. See the
//! module-doc of [`crate::three_pillar`] for the full lift
//! narrative.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::three_pillar;

/// Attestation written to `Process.status.attestation` after each convergence cycle.
///
/// Composition:
/// ```text
/// composed_root = BLAKE3(
///     "tatara-process/v1alpha1\n"
///     ++ artifact_hash ++ "\n"
///     ++ control_hash.unwrap_or("") ++ "\n"
///     ++ intent_hash ++ "\n"
///     ++ previous_root.unwrap_or("")
/// )
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessAttestation {
    /// `BLAKE3(rendered resources ++ their applied-status digests)`.
    pub artifact_hash: String,
    /// `BLAKE3(compliance-verification proof)` — absent iff no compliance bindings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_hash: Option<String>,
    /// `BLAKE3(canonical-spec ++ nix-store-path? ++ lisp-AST?)`.
    pub intent_hash: String,
    /// `BLAKE3` of the three pillars + previous root.
    pub composed_root: String,
    /// Monotonic generation counter — starts at 0, increments each cycle.
    pub generation: u64,
    /// The prior `composed_root` in the chain. `None` for generation 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_root: Option<String>,
    /// When the attestation was computed.
    pub attested_at: DateTime<Utc>,
}

impl ProcessAttestation {
    /// Compose an attestation from the three pillars + chain context.
    pub fn compose(
        artifact_hash: String,
        control_hash: Option<String>,
        intent_hash: String,
        previous_root: Option<String>,
        generation: u64,
    ) -> Self {
        let composed_root = three_pillar::compose_root(
            &artifact_hash,
            control_hash.as_deref(),
            &intent_hash,
            previous_root.as_deref(),
        );
        Self {
            artifact_hash,
            control_hash,
            intent_hash,
            composed_root,
            generation,
            previous_root,
            attested_at: Utc::now(),
        }
    }

    /// Convenience for the initial attestation (generation 0, no previous root).
    pub fn initial(
        artifact_hash: String,
        control_hash: Option<String>,
        intent_hash: String,
    ) -> Self {
        Self::compose(artifact_hash, control_hash, intent_hash, None, 0)
    }

    /// Convenience for chaining: `self.next(new_pillars)` yields the next attestation.
    pub fn next(
        &self,
        artifact_hash: String,
        control_hash: Option<String>,
        intent_hash: String,
    ) -> Self {
        Self::compose(
            artifact_hash,
            control_hash,
            intent_hash,
            Some(self.composed_root.clone()),
            self.generation + 1,
        )
    }

    /// Verify that `composed_root` is consistent with the pillars + `previous_root`.
    pub fn verify(&self) -> bool {
        let recomputed = three_pillar::compose_root(
            &self.artifact_hash,
            self.control_hash.as_deref(),
            &self.intent_hash,
            self.previous_root.as_deref(),
        );
        three_pillar::constant_time_eq(recomputed.as_bytes(), self.composed_root.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_has_generation_zero() {
        let a = ProcessAttestation::initial("a".into(), None, "i".into());
        assert_eq!(a.generation, 0);
        assert!(a.previous_root.is_none());
        assert!(a.verify());
    }

    #[test]
    fn chain_extends_previous_root() {
        let a0 = ProcessAttestation::initial("a0".into(), Some("c0".into()), "i0".into());
        let a1 = a0.next("a1".into(), Some("c1".into()), "i1".into());
        assert_eq!(a1.generation, 1);
        assert_eq!(a1.previous_root.as_deref(), Some(a0.composed_root.as_str()));
        assert_ne!(a0.composed_root, a1.composed_root);
        assert!(a1.verify());
    }

    #[test]
    fn verify_detects_tamper() {
        let mut a = ProcessAttestation::initial("a".into(), None, "i".into());
        assert!(a.verify());
        a.artifact_hash = "tampered".into();
        assert!(!a.verify());
    }

    #[test]
    fn control_hash_affects_root() {
        let a = ProcessAttestation::initial("x".into(), None, "y".into());
        let b = ProcessAttestation::initial("x".into(), Some("c".into()), "y".into());
        assert_ne!(a.composed_root, b.composed_root);
    }
}
