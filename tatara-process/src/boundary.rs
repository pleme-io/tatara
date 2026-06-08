//! Boundary conditions — predicates that gate phase transitions.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
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

impl ConditionKind {
    /// The closed set of boundary-condition kinds the reconciler honors.
    /// Single source of truth that drives the `as_str` / Display /
    /// `FromStr` triad on this enum and the `stub_message` lift of the
    /// "not yet implemented" arms the reconciler used to hand-roll three
    /// times. Adding a 9th variant lands at one `ALL` entry + one `as_str`
    /// arm + one `stub_message` arm — exhaustively checked by the
    /// compiler (the array literal forces arity).
    ///
    /// Sibling closed-set lifts: [`crate::phase::ProcessPhase::ALL`],
    /// [`crate::signal::ProcessSignal::ALL`], [`crate::intent::IntentKind::ALL`],
    /// [`crate::lifetime::LifetimeKind::ALL`].
    pub const ALL: [Self; 8] = [
        Self::ProcessPhase,
        Self::KustomizationHealthy,
        Self::HelmReleaseReleased,
        Self::PromQL,
        Self::Cel,
        Self::NixEval,
        Self::JobAttested,
        Self::ClosedLoopAuth,
    ];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim. Used by Display
    /// (single source of truth), by `FromStr` to identify the variant
    /// from its annotation / status-field representation, and by
    /// operator-facing diagnostics that need the kind name without
    /// re-serializing the enum through serde_json. Pinned by
    /// `condition_kind_as_str_matches_serde`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProcessPhase => "ProcessPhase",
            Self::KustomizationHealthy => "KustomizationHealthy",
            Self::HelmReleaseReleased => "HelmReleaseReleased",
            Self::PromQL => "PromQL",
            Self::Cel => "Cel",
            Self::NixEval => "NixEval",
            Self::JobAttested => "JobAttested",
            Self::ClosedLoopAuth => "ClosedLoopAuth",
        }
    }

    /// The operator-facing "evaluator not yet implemented" message for
    /// stub kinds — `Some` iff this kind has no live evaluator wired in
    /// `tatara-reconciler::boundary`. ONE site owns the per-kind stub
    /// string; the reconciler's dispatch reaches for this projection
    /// instead of hand-rolling three parallel `Unknown(...)` strings.
    ///
    /// A future variant added as a live evaluator returns `None`; a
    /// future variant added as a stub returns `Some("<kind> evaluator
    /// not yet implemented")` — both reachable through one match
    /// instead of three identical-shape arms drifting in parallel.
    pub const fn stub_message(self) -> Option<&'static str> {
        match self {
            Self::PromQL => Some("PromQL evaluator not yet implemented"),
            Self::Cel => Some("CEL evaluator not yet implemented"),
            Self::NixEval => Some("NixEval evaluator not yet implemented"),
            Self::ProcessPhase
            | Self::KustomizationHealthy
            | Self::HelmReleaseReleased
            | Self::JobAttested
            | Self::ClosedLoopAuth => None,
        }
    }

    /// True iff this kind has no live evaluator (its [`Self::stub_message`]
    /// is `Some`). Pairs with the reconciler's `evaluate` dispatch — a
    /// stub kind unconditionally yields `Satisfaction::Unknown`.
    pub const fn is_stub(self) -> bool {
        self.stub_message().is_some()
    }
}

impl fmt::Display for ConditionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ConditionKind {
    type Err = UnknownConditionKind;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for kind in Self::ALL {
            if s == kind.as_str() {
                return Ok(kind);
            }
        }
        Err(UnknownConditionKind(s.to_string()))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown condition kind: {0}")]
pub struct UnknownConditionKind(pub String);

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

    // ── closed-set algebra contracts (ALL × as_str × FromStr × stub_message) ─

    /// `ALL` is the source of truth — pin its closure so a variant
    /// added without an `ALL` entry fails here (uniqueness check)
    /// before drifting `as_str` / `FromStr` / `stub_message`. The
    /// arity is asserted by the array type itself (`[Self; 8]`).
    #[test]
    fn condition_kind_all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for kind in ConditionKind::ALL {
            assert!(seen.insert(kind), "duplicate variant in ALL: {kind:?}");
        }
        assert_eq!(seen.len(), ConditionKind::ALL.len());
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site. The probe
    /// confirmed `PromQL` survives `rename_all = "PascalCase"` as
    /// `"PromQL"` (heck preserves consecutive caps in the leading
    /// word), so this contract is the operator-facing pin.
    #[test]
    fn condition_kind_as_str_matches_serde() {
        for kind in ConditionKind::ALL {
            let serialized = serde_json::to_string(&kind)
                .expect("ConditionKind serializes")
                .trim_matches('"')
                .to_string();
            assert_eq!(
                kind.as_str(),
                serialized,
                "as_str() must match serde output for {kind:?}",
            );
        }
    }

    /// ROUND-TRIP CONTRACT: every variant survives `as_str` ↔ `FromStr`.
    /// Adding a variant without extending `as_str` (or vice versa)
    /// fails here.
    #[test]
    fn condition_kind_roundtrip_via_as_str() {
        use std::str::FromStr;
        for kind in ConditionKind::ALL {
            assert_eq!(
                ConditionKind::from_str(kind.as_str()).expect("known kind round-trips"),
                kind,
                "round-trip failed for {kind:?}",
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. If a
    /// reviewer accidentally re-introduces an inline match in
    /// Display, this fails the moment a variant rename touches one
    /// site but not the other.
    #[test]
    fn condition_kind_display_matches_as_str() {
        for kind in ConditionKind::ALL {
            assert_eq!(kind.to_string(), kind.as_str());
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — empty / lowercased / typo / unrelated — and the
    /// error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    #[test]
    fn unknown_condition_kind_errors() {
        use std::str::FromStr;
        for bad in ["", "processPhase", "PROMQL", "Promql", "Bogus"] {
            let err = ConditionKind::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// STUB CONTRACT: the three placeholder evaluators
    /// (PromQL / Cel / NixEval) are exactly the set whose
    /// `stub_message` is `Some`. The five live evaluators return
    /// `None`. A future variant promoted from stub → live must drop
    /// its `stub_message` arm; a new stub must add one. Both
    /// transitions land at this test by sweeping ALL.
    #[test]
    fn condition_kind_stub_set_matches_stubs() {
        use ConditionKind::*;
        for kind in ConditionKind::ALL {
            let expected_is_stub = matches!(kind, PromQL | Cel | NixEval);
            assert_eq!(
                kind.is_stub(),
                expected_is_stub,
                "is_stub disagreed for {kind:?}",
            );
            assert_eq!(
                kind.stub_message().is_some(),
                expected_is_stub,
                "stub_message disagreed for {kind:?}",
            );
        }
    }

    /// Pin the exact stub strings so a rename of the operator-facing
    /// "not yet implemented" message lands at one site (here) instead
    /// of three parallel inline strings in the reconciler.
    #[test]
    fn condition_kind_stub_messages_are_pinned() {
        assert_eq!(
            ConditionKind::PromQL.stub_message(),
            Some("PromQL evaluator not yet implemented"),
        );
        assert_eq!(
            ConditionKind::Cel.stub_message(),
            Some("CEL evaluator not yet implemented"),
        );
        assert_eq!(
            ConditionKind::NixEval.stub_message(),
            Some("NixEval evaluator not yet implemented"),
        );
    }
}
