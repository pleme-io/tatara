//! Boundary conditions — predicates that gate phase transitions.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Boundary specification — preconditions gate Running,
/// postconditions gate Running → Attested.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Boundary {
    #[serde(default)]
    pub preconditions: Vec<Condition>,
    #[serde(default)]
    pub postconditions: Vec<Condition>,
    /// Max time before VERIFY fails — parsed as a `go`-style duration.
    /// Empty = controller default (15m).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
}

/// A single boundary predicate.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub kind: ConditionKind,
    /// Kind-specific payload (free-form JSON).
    #[serde(default)]
    #[schemars(schema_with = "crate::schema_helpers::preserve_unknown_object")]
    pub params: serde_json::Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum ConditionKind {
    /// Another Process must be in a given phase.
    /// `params`: `{ "processRef": "...", "namespace": "...", "phase": "Attested" }`
    ProcessPhase,
    /// FluxCD `Kustomization.status.conditions[type=Ready]` must be `True`.
    /// `params`: `{ "name": "...", "namespace": "flux-system" }`
    KustomizationHealthy,
    /// FluxCD `HelmRelease.status.conditions[type=Ready]` must be `True`.
    /// `params`: `{ "name": "...", "namespace": "..." }`
    HelmReleaseReleased,
    /// Prometheus query — truthy scalar required.
    /// `params`: `{ "query": "..." }`
    PromQL,
    /// CEL expression over a scoped object set.
    /// `params`: `{ "expression": "..." }`
    Cel,
    /// Nix evaluation equality check.
    /// `params`: `{ "flakeRef": "...", "attribute": "...", "expect": "..." }`
    NixEval,
    /// A Kubernetes Job must complete successfully and its emitted BLAKE3
    /// receipt must verify.
    /// `params`: `{ "name": "...", "namespace": "...", "expectReceipt": true }`
    JobAttested,
    /// Closed-loop authentication probe — the canonical postcondition for
    /// any system that can produce credentials for its own client under
    /// test. The probe Job (rendered by the VERIFY handler) fetches a
    /// fresh secret from `issuer` (a Service inside the same namespace),
    /// presents it to `consumer` (another Service in the same namespace),
    /// and verifies that `consumer` authenticated successfully against
    /// `jwk_source` (the issuer's published JWK endpoint).
    ///
    /// The Job emits a three-pillar BLAKE3 receipt that the reconciler
    /// chains into `status.attestation`. This turns "the gateway↔SaaS
    /// loop holds" from an assertion into a theorem provable for every
    /// ephemeral run.
    ///
    /// `params`:
    /// ```json
    /// {
    ///   "issuer":   { "service": "akeyless-saas-akeyless-gator",
    ///                 "port": 8080,
    ///                 "secretPath": "/v2/get-secret-value" },
    ///   "consumer": { "service": "akeyless-saas-akeyless-gateway",
    ///                 "port": 8000,
    ///                 "authPath": "/api/v3/auth" },
    ///   "jwkSource":{ "service": "akeyless-saas-akeyless-gator",
    ///                 "port": 8080,
    ///                 "path": "/.well-known/jwks.json" },
    ///   "probeImage": "ghcr.io/pleme-io/closed-loop-probe:0.1.0",
    ///   "timeoutSeconds": 120
    /// }
    /// ```
    ClosedLoopAuth,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serde_process_phase_condition() {
        let c = Condition {
            kind: ConditionKind::ProcessPhase,
            params: json!({ "processRef": "akeyless-injection", "phase": "Attested" }),
        };
        let yaml = serde_yaml::to_string(&c).unwrap();
        assert!(yaml.contains("kind: ProcessPhase"));
        assert!(yaml.contains("processRef: akeyless-injection"));
    }

    #[test]
    fn serde_closed_loop_auth_condition() {
        let c = Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({
                "issuer":   { "service": "akeyless-saas-akeyless-gator", "port": 8080 },
                "consumer": { "service": "akeyless-saas-akeyless-gateway", "port": 8000 },
                "probeImage": "ghcr.io/pleme-io/closed-loop-probe:0.1.0",
            }),
        };
        let yaml = serde_yaml::to_string(&c).unwrap();
        assert!(yaml.contains("kind: ClosedLoopAuth"));
        assert!(yaml.contains("probeImage: ghcr.io/pleme-io/closed-loop-probe:0.1.0"));
        let back: Condition = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.kind, ConditionKind::ClosedLoopAuth);
    }

    #[test]
    fn serde_job_attested_condition() {
        let c = Condition {
            kind: ConditionKind::JobAttested,
            params: json!({ "name": "seed-job", "namespace": "akeyless-test" }),
        };
        let yaml = serde_yaml::to_string(&c).unwrap();
        assert!(yaml.contains("kind: JobAttested"));
    }
}
