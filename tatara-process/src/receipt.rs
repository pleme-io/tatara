//! `tatara-receipt/v1` — the typed receipt envelope every pleme-io Job
//! emits to prove its work was done.
//!
//! Today's consumers (and the only ones supported on `tatara-receipt/v1`):
//! - **closed-loop auth probes** — `kind = "closed-loop-auth"`. Stamps
//!   that a system's bundled identity issuer authenticated its bundled
//!   client. The substrate primitive every closed-loop-testable product
//!   composes (Akeyless gator↔gateway, future: identity providers,
//!   message brokers, databases that can issue creds to themselves).
//! - **schema/migration runs** — `kind = "db-migration"`. shinka emits
//!   one per applied migration; pillars carry the diff hash.
//! - **test suites** — `kind = "test-suite"`. kenshi-runner et al.
//! - **nix builds** — `kind = "nix-build"`. Carries the store-path
//!   pillar as `artifact_hash`.
//! - Anything else — operators register new `kind` strings; the
//!   schema is open by design (the *shape* is fixed; the kind is data).
//!
//! Lives in `tatara-process` so `ReceiptEnvelope → ProcessAttestation`
//! is a local typed bridge — the reconciler's verifier and any future
//! Process consumer share one parse.
//!
//! Wire format (snake_case to match the existing ConfigMap payload
//! shape the akeyless-closed-loop-probe chart writes):
//!
//! ```yaml
//! version: tatara-receipt/v1
//! kind: closed-loop-auth
//! composed_root: <26-char hex>
//! intent_hash:   <hex>
//! artifact_hash: <hex>
//! control_hash:  <hex>
//! generated_at:  2026-05-19T22:00:00Z
//! process_ref:   "akeyless-test/ephemeral-akeyless"   # optional
//! evidence:      { ... }                              # optional, free-form
//! ```

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::attestation::ProcessAttestation;

/// Canonical version string. Bump → `tatara-receipt/v2` if the wire
/// shape changes; parsers refuse anything else for the v1 reader.
pub const RECEIPT_VERSION: &str = "tatara-receipt/v1";

/// Typed receipt envelope. Any Job in pleme-io that wants its result to
/// chain into a Process's `status.attestation` writes one of these.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ReceiptEnvelope {
    /// Must equal `RECEIPT_VERSION`. Mismatches reject the receipt.
    pub version: String,
    /// What this receipt proves. Known: `closed-loop-auth`, `db-migration`,
    /// `test-suite`, `nix-build`. Operators may register new kinds —
    /// the envelope is open.
    pub kind: String,
    /// Three-pillar root: `BLAKE3(domain ++ artifact ++ control ++ intent ++ previous)`.
    pub composed_root: String,
    /// Pillar 1: what the Job was *trying* to do (canonical intent).
    pub intent_hash: String,
    /// Pillar 2: what the Job *produced* (artifact / proof material).
    pub artifact_hash: String,
    /// Pillar 3: how the Job *verified* its work (controls / signatures /
    /// auth steps). Empty string when there was no control step.
    pub control_hash: String,
    /// Timestamp the Job set when it wrote the receipt.
    pub generated_at: DateTime<Utc>,
    /// Optional owning-Process reference (`namespace/name`). When the
    /// reconciler creates the Job it stamps this in via the downward
    /// API; receipts without it still parse for ad-hoc / out-of-cluster
    /// runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_ref: Option<String>,
    /// Optional structured evidence. Free-form JSON. The reconciler does
    /// not parse this — it's for human / downstream-tool inspection.
    #[serde(default, skip_serializing_if = "is_null")]
    pub evidence: serde_json::Value,
}

fn is_null(v: &serde_json::Value) -> bool {
    v.is_null()
}

/// Why a receipt is rejected. Kept as a typed enum so callers can
/// pattern-match on the failure mode and surface targeted operator
/// messages.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReceiptError {
    #[error("invalid JSON: {0}")]
    InvalidJson(String),
    #[error("invalid YAML: {0}")]
    InvalidYaml(String),
    #[error("version != {RECEIPT_VERSION} (got {0:?})")]
    WrongVersion(String),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("kind is empty")]
    EmptyKind,
    #[error("composed_root mismatch (got {got}, want {want})")]
    RootMismatch { got: String, want: String },
}

impl ReceiptEnvelope {
    /// Build a receipt envelope from typed pillars + kind. `generated_at`
    /// defaults to `Utc::now()`.
    pub fn build(
        kind: impl Into<String>,
        intent_hash: impl Into<String>,
        artifact_hash: impl Into<String>,
        control_hash: impl Into<String>,
        previous_root: Option<&str>,
    ) -> Self {
        let intent_hash = intent_hash.into();
        let artifact_hash = artifact_hash.into();
        let control_hash = control_hash.into();
        let composed_root = compose_root(
            &artifact_hash,
            if control_hash.is_empty() {
                None
            } else {
                Some(control_hash.as_str())
            },
            &intent_hash,
            previous_root,
        );
        Self {
            version: RECEIPT_VERSION.into(),
            kind: kind.into(),
            composed_root,
            intent_hash,
            artifact_hash,
            control_hash,
            generated_at: Utc::now(),
            process_ref: None,
            evidence: serde_json::Value::Null,
        }
    }

    /// Parse a receipt from a JSON string.
    pub fn parse_json(payload: &str) -> Result<Self, ReceiptError> {
        let env: Self = serde_json::from_str(payload)
            .map_err(|e| ReceiptError::InvalidJson(e.to_string()))?;
        env.verify_shape()?;
        Ok(env)
    }

    /// Parse a receipt from a YAML string. Useful for ConfigMaps that
    /// store the payload in YAML form.
    pub fn parse_yaml(payload: &str) -> Result<Self, ReceiptError> {
        let env: Self = serde_yaml::from_str(payload)
            .map_err(|e| ReceiptError::InvalidYaml(e.to_string()))?;
        env.verify_shape()?;
        Ok(env)
    }

    /// Parse via JSON first, then YAML if JSON fails. Lets a single
    /// reader accept either wire form without the operator having to
    /// declare it. Useful when the Job writes JSON and the reconciler
    /// reads back through a kube DynamicObject whose `data` is YAML.
    pub fn parse_either(payload: &str) -> Result<Self, ReceiptError> {
        match Self::parse_json(payload) {
            Ok(env) => Ok(env),
            Err(_) => Self::parse_yaml(payload),
        }
    }

    /// Verify the schema-level invariants: correct version + non-empty
    /// kind + non-empty pillar hashes (length-only, not BLAKE3-recompute).
    pub fn verify_shape(&self) -> Result<(), ReceiptError> {
        if self.version != RECEIPT_VERSION {
            return Err(ReceiptError::WrongVersion(self.version.clone()));
        }
        if self.kind.is_empty() {
            return Err(ReceiptError::EmptyKind);
        }
        if self.composed_root.is_empty() {
            return Err(ReceiptError::MissingField("composed_root"));
        }
        if self.intent_hash.is_empty() {
            return Err(ReceiptError::MissingField("intent_hash"));
        }
        if self.artifact_hash.is_empty() {
            return Err(ReceiptError::MissingField("artifact_hash"));
        }
        // control_hash MAY be empty when there is no control step;
        // the BLAKE3 compose treats empty as "absent" via Option.
        Ok(())
    }

    /// Verify that `composed_root` is consistent with the pillars.
    /// `expected_previous_root` is the previous root in the Process's
    /// attestation chain (or `None` for first attestation).
    pub fn verify_root(&self, expected_previous_root: Option<&str>) -> bool {
        let want = compose_root(
            &self.artifact_hash,
            if self.control_hash.is_empty() {
                None
            } else {
                Some(self.control_hash.as_str())
            },
            &self.intent_hash,
            expected_previous_root,
        );
        constant_time_eq(want.as_bytes(), self.composed_root.as_bytes())
    }

    /// Strict-equality check against an operator-provided expected root.
    /// Returns the receipt's root unchanged on success.
    pub fn expect_root(&self, expected: Option<&str>) -> Result<&str, ReceiptError> {
        if let Some(want) = expected {
            if want != self.composed_root {
                return Err(ReceiptError::RootMismatch {
                    got: self.composed_root.clone(),
                    want: want.to_string(),
                });
            }
        }
        Ok(&self.composed_root)
    }

    /// Lower into a `ProcessAttestation` — the canonical handoff so a
    /// Job's typed receipt becomes evidence on a Process. `generation`
    /// + `previous_root` come from the owning Process's prior
    /// attestation (or 0 + None for the first cycle).
    pub fn to_attestation(&self, generation: u64, previous_root: Option<&str>) -> ProcessAttestation {
        ProcessAttestation::compose(
            self.artifact_hash.clone(),
            if self.control_hash.is_empty() {
                None
            } else {
                Some(self.control_hash.clone())
            },
            self.intent_hash.clone(),
            previous_root.map(String::from),
            generation,
        )
    }
}

const DOMAIN_TAG: &[u8] = b"tatara-process/v1alpha1\n";

/// Same composition as `ProcessAttestation::composed_hex` — kept local so
/// `tatara_process::receipt::compose_root(...)` is a single line in
/// downstream code without re-importing the attestation module.
fn compose_root(
    artifact: &str,
    control: Option<&str>,
    intent: &str,
    previous: Option<&str>,
) -> String {
    let mut h = blake3::Hasher::new();
    h.update(DOMAIN_TAG);
    h.update(artifact.as_bytes());
    h.update(b"\n");
    h.update(control.unwrap_or("").as_bytes());
    h.update(b"\n");
    h.update(intent.as_bytes());
    h.update(b"\n");
    h.update(previous.unwrap_or("").as_bytes());
    hex::encode(h.finalize().as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> &'static str {
        // Composed_root precomputed from compose_root("bbbb", Some("cccc"), "aaaa", None)
        // (recomputed at test time to be canonical; this string is regenerated
        // if the domain tag ever changes).
        r#"{
            "version": "tatara-receipt/v1",
            "kind": "closed-loop-auth",
            "composed_root": "RECOMPUTE",
            "intent_hash":   "aaaa",
            "artifact_hash": "bbbb",
            "control_hash":  "cccc",
            "generated_at":  "2026-05-19T12:00:00Z"
        }"#
    }

    fn canonical_payload_json() -> String {
        let root = compose_root("bbbb", Some("cccc"), "aaaa", None);
        sample_payload().replace("RECOMPUTE", &root)
    }

    #[test]
    fn build_produces_valid_envelope() {
        let r = ReceiptEnvelope::build("test-suite", "i", "a", "c", None);
        assert_eq!(r.version, RECEIPT_VERSION);
        assert_eq!(r.kind, "test-suite");
        assert!(r.verify_shape().is_ok());
        assert!(r.verify_root(None));
    }

    #[test]
    fn build_empty_control_omits_from_root() {
        let with_empty = ReceiptEnvelope::build("nix-build", "i", "a", "", None);
        let with_explicit_none = ReceiptEnvelope::build("nix-build", "i", "a", "", None);
        assert_eq!(with_empty.composed_root, with_explicit_none.composed_root);

        // And differs from a receipt with a real control hash.
        let with_control = ReceiptEnvelope::build("nix-build", "i", "a", "c", None);
        assert_ne!(with_empty.composed_root, with_control.composed_root);
    }

    #[test]
    fn parse_json_round_trip() {
        let r = ReceiptEnvelope::parse_json(&canonical_payload_json()).expect("parse");
        assert_eq!(r.kind, "closed-loop-auth");
        assert!(r.verify_root(None));
    }

    #[test]
    fn parse_yaml_round_trip() {
        let yaml = r#"
version: tatara-receipt/v1
kind: db-migration
composed_root: ROOT
intent_hash:   aaaa
artifact_hash: bbbb
control_hash:  cccc
generated_at:  2026-05-19T12:00:00Z
"#
        .replace("ROOT", &compose_root("bbbb", Some("cccc"), "aaaa", None));
        let r = ReceiptEnvelope::parse_yaml(&yaml).expect("yaml parse");
        assert_eq!(r.kind, "db-migration");
        assert!(r.verify_root(None));
    }

    #[test]
    fn parse_either_falls_back_to_yaml() {
        let yaml = r#"
version: tatara-receipt/v1
kind: test-suite
composed_root: ROOT
intent_hash:   aaaa
artifact_hash: bbbb
control_hash:  cccc
generated_at:  2026-05-19T12:00:00Z
"#
        .replace("ROOT", &compose_root("bbbb", Some("cccc"), "aaaa", None));
        assert!(ReceiptEnvelope::parse_either(&yaml).is_ok());
    }

    #[test]
    fn wrong_version_rejected() {
        let mut env: serde_json::Value = serde_json::from_str(&canonical_payload_json()).unwrap();
        env["version"] = "tatara-receipt/v2".into();
        let err = ReceiptEnvelope::parse_json(&env.to_string()).unwrap_err();
        assert!(matches!(err, ReceiptError::WrongVersion(ref s) if s == "tatara-receipt/v2"));
    }

    #[test]
    fn missing_field_rejected() {
        let mut env: serde_json::Value = serde_json::from_str(&canonical_payload_json()).unwrap();
        env.as_object_mut().unwrap().remove("intent_hash");
        let err = ReceiptEnvelope::parse_json(&env.to_string()).unwrap_err();
        assert!(matches!(err, ReceiptError::InvalidJson(_)));
    }

    #[test]
    fn unknown_field_rejected() {
        let mut env: serde_json::Value = serde_json::from_str(&canonical_payload_json()).unwrap();
        env["forged_extra"] = "should-fail".into();
        let err = ReceiptEnvelope::parse_json(&env.to_string()).unwrap_err();
        assert!(matches!(err, ReceiptError::InvalidJson(_)));
    }

    #[test]
    fn empty_kind_rejected_in_verify_shape() {
        let mut r = ReceiptEnvelope::build("k", "i", "a", "c", None);
        r.kind = String::new();
        assert!(matches!(r.verify_shape(), Err(ReceiptError::EmptyKind)));
    }

    #[test]
    fn expect_root_matches_or_mismatches() {
        let r = ReceiptEnvelope::build("test-suite", "i", "a", "c", None);
        let root = r.composed_root.clone();
        assert!(r.expect_root(Some(&root)).is_ok());
        let err = r.expect_root(Some("nope")).unwrap_err();
        assert!(matches!(err, ReceiptError::RootMismatch { .. }));
        assert!(r.expect_root(None).is_ok());
    }

    #[test]
    fn lower_to_attestation_chains_pillars() {
        let r = ReceiptEnvelope::build("closed-loop-auth", "i", "a", "c", None);
        let a = r.to_attestation(0, None);
        assert_eq!(a.intent_hash, "i");
        assert_eq!(a.artifact_hash, "a");
        assert_eq!(a.control_hash.as_deref(), Some("c"));
        // Both compose the same root.
        assert_eq!(a.composed_root, r.composed_root);
        assert!(a.verify());

        let next = r.to_attestation(1, Some(&a.composed_root));
        assert_eq!(next.generation, 1);
        assert_eq!(next.previous_root.as_deref(), Some(a.composed_root.as_str()));
        // The composed_root differs because previous_root is included.
        assert_ne!(next.composed_root, a.composed_root);
    }

    #[test]
    fn verify_root_detects_tamper() {
        let mut r = ReceiptEnvelope::build("closed-loop-auth", "i", "a", "c", None);
        assert!(r.verify_root(None));
        r.intent_hash = "tampered".into();
        assert!(!r.verify_root(None));
    }

    #[test]
    fn process_ref_optional_and_round_trips() {
        let mut r = ReceiptEnvelope::build("test-suite", "i", "a", "c", None);
        r.process_ref = Some("akeyless-test/ephemeral".into());
        let s = serde_json::to_string(&r).unwrap();
        let back = ReceiptEnvelope::parse_json(&s).expect("round-trip");
        assert_eq!(back.process_ref.as_deref(), Some("akeyless-test/ephemeral"));
    }

    #[test]
    fn evidence_round_trips() {
        let mut r = ReceiptEnvelope::build("test-suite", "i", "a", "c", None);
        r.evidence = serde_json::json!({ "passed": 12, "failed": 0, "duration_ms": 4200 });
        let s = serde_json::to_string(&r).unwrap();
        let back = ReceiptEnvelope::parse_json(&s).expect("round-trip");
        assert_eq!(back.evidence["passed"], 12);
    }
}
