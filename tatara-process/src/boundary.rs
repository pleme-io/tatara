//! Boundary conditions — predicates that gate phase transitions.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::flux_resource::FluxResource;

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

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", display, generate_unknown)]
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
    ///   "issuer":   { "service": "demo-app-issuer",
    ///                 "port": 8080,
    ///                 "secretPath": "/v2/get-secret-value" },
    ///   "consumer": { "service": "demo-app-gateway",
    ///                 "port": 8000,
    ///                 "authPath": "/api/v3/auth" },
    ///   "jwkSource":{ "service": "demo-app-issuer",
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

    /// The [`FluxResource`] variant this condition kind fetches from
    /// the K8s API server, or `None` for non-Flux-fetching kinds — the
    /// typed projection owning the (ConditionKind → FluxResource)
    /// association every reconciler `evaluate` dispatch arm and every
    /// future coherence check binds through.
    ///
    /// Pre-lift the association was open-coded at TWO adjacent
    /// `evaluate` arms in `tatara-reconciler::boundary::evaluate` past
    /// the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold — each arm
    /// hand-authored a `(FluxResource::X.api_version(),
    /// FluxResource::X.kind())` pair as the two `&str` slots the
    /// pre-lift `evaluate_flux_ready(api_version: &str, kind: &str)`
    /// signature required. Post-lift the mapping lives at ONE typed
    /// projection here, the callee accepts a typed
    /// [`FluxResource`] slot (invalid `(apiVersion, kind)` pairings
    /// like Kustomization's apiVersion paired with HelmRelease's kind
    /// become unrepresentable), and the two `evaluate` arms collapse
    /// onto ONE `KustomizationHealthy | HelmReleaseReleased` OR-arm
    /// that reads the FluxResource variant from `.flux_resource()`.
    ///
    /// A future ConditionKind that fetches a fourth Flux resource
    /// variant (a hypothetical `BucketSynced` kind against a Flux
    /// `Bucket` source) lands as ONE new arm here + ONE new variant
    /// on [`FluxResource`] + ONE OR-pattern extension at the
    /// reconciler dispatch — no hand-authored `(apiVersion, kind)`
    /// pair at the callsite, no widening of the callee's signature.
    ///
    /// The three current non-Flux-fetching arms return `None`:
    /// - `ProcessPhase` fetches a tatara `Process` (through its own
    ///   [`crate::api_version`] + [`crate::PROCESS_KIND`] pair, not
    ///   a Flux `(apiVersion, kind)`).
    /// - `JobAttested` / `ClosedLoopAuth` fetch a `batch/v1::Job` +
    ///   an optional receipt `v1::ConfigMap`, both K8s built-ins
    ///   (not Flux resources).
    /// - `PromQL` / `Cel` / `NixEval` are stub evaluators
    ///   ([`Self::is_stub`]) — no cluster fetch at all.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the (ConditionKind → FluxResource)
    /// association lives at ONE typed algebra projection here, not
    /// at every reconciler dispatch arm).
    pub const fn flux_resource(self) -> Option<FluxResource> {
        match self {
            Self::KustomizationHealthy => Some(FluxResource::Kustomization),
            Self::HelmReleaseReleased => Some(FluxResource::HelmRelease),
            Self::ProcessPhase
            | Self::PromQL
            | Self::Cel
            | Self::NixEval
            | Self::JobAttested
            | Self::ClosedLoopAuth => None,
        }
    }
}

// `impl fmt::Display for ConditionKind` + `impl FromStr for
// ConditionKind` + `impl tatara_lisp::ClosedSet for ConditionKind` +
// `pub struct UnknownConditionKind(pub String)` are generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(via =
// "as_str", display, generate_unknown)]` on the enum declaration above.
// The auto-derived label `"condition kind"` matches the prior hand-
// rolled `#[error("unknown condition kind: {0}")]` verbatim. The
// inherent `as_str` projection stays load-bearing — the PascalCase
// wire-format that matches the serde rename + the CRD `enum:` listing
// verbatim (notably preserving `PromQL`'s consecutive caps that heck
// would have lowercased) — while the trait method `label` gives
// generic consumers a STABLE name across the 36+ workspace-wide
// closed-set implementors.

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serde_process_phase_condition() {
        let c = Condition {
            kind: ConditionKind::ProcessPhase,
            params: json!({ "processRef": "secret-injection", "phase": "Attested" }),
        };
        let yaml = serde_yaml::to_string(&c).unwrap();
        assert!(yaml.contains("kind: ProcessPhase"));
        assert!(yaml.contains("processRef: secret-injection"));
    }

    #[test]
    fn serde_closed_loop_auth_condition() {
        let c = Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({
                "issuer":   { "service": "demo-app-issuer", "port": 8080 },
                "consumer": { "service": "demo-app-gateway", "port": 8000 },
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
            params: json!({ "name": "seed-job", "namespace": "demo-test" }),
        };
        let yaml = serde_yaml::to_string(&c).unwrap();
        assert!(yaml.contains("kind: JobAttested"));
    }

    // ── closed-set algebra contracts (ALL × as_str × FromStr × stub_message) ─

    /// Structural well-formedness of [`ConditionKind`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through `label ↔
    /// parse_label`, labels are pairwise distinct, `""` is outside the
    /// closed set) at ONE call site. Replaces the hand-derived
    /// `condition_kind_all_is_unique_and_complete` +
    /// `condition_kind_roundtrip_via_as_str` + the empty-input arm of
    /// `unknown_condition_kind_errors`. `FromStr` delegates to
    /// `<Self as tatara_closed_set::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the reconciler hits when parsing a
    /// CRD `enum:`-validated value back to the typed kind.
    #[test]
    fn condition_kind_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<ConditionKind>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site. The probe
    /// confirmed `PromQL` survives `rename_all = "PascalCase"` as
    /// `"PromQL"` (heck preserves consecutive caps in the leading
    /// word), so this contract is the operator-facing pin.
    #[test]
    fn condition_kind_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<ConditionKind>();
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. If a
    /// reviewer accidentally re-introduces an inline match in
    /// Display, this fails the moment a variant rename touches one
    /// site but not the other.
    #[test]
    fn condition_kind_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<ConditionKind>();
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / unrelated — and the error
    /// echoes the input verbatim so the operator-facing diagnostic
    /// carries the offending value, not a normalized form. The
    /// empty-input arm is pinned by
    /// [`condition_kind_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownConditionKind`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_condition_kind_errors() {
        use std::str::FromStr;
        for bad in ["processPhase", "PROMQL", "Promql", "Bogus"] {
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

    // ── (ConditionKind → FluxResource) typed projection contracts ────

    /// The two Flux-fetching kinds project to their canonical
    /// [`FluxResource`] variants. A future ConditionKind rename or
    /// FluxResource variant rename that skewed the projection at ONE
    /// arm surfaces here.
    #[test]
    fn kustomization_healthy_projects_to_flux_resource_kustomization() {
        assert_eq!(
            ConditionKind::KustomizationHealthy.flux_resource(),
            Some(FluxResource::Kustomization),
        );
    }

    #[test]
    fn helm_release_released_projects_to_flux_resource_helm_release() {
        assert_eq!(
            ConditionKind::HelmReleaseReleased.flux_resource(),
            Some(FluxResource::HelmRelease),
        );
    }

    /// The six non-Flux-fetching kinds project to `None`. Sweeps
    /// `ConditionKind::ALL` filtering by `flux_resource().is_none()`
    /// so a new variant added without a `flux_resource` arm surfaces
    /// at rustc's non-exhaustive-match gate BEFORE this test even
    /// runs; a new variant added with a hand-coded `Some(...)` arm
    /// that shouldn't fetch Flux surfaces here.
    #[test]
    fn non_flux_fetching_kinds_project_to_none() {
        use ConditionKind::*;
        let non_flux: Vec<_> = ConditionKind::ALL
            .iter()
            .copied()
            .filter(|k| k.flux_resource().is_none())
            .collect();
        assert_eq!(
            non_flux,
            vec![
                ProcessPhase,
                PromQL,
                Cel,
                NixEval,
                JobAttested,
                ClosedLoopAuth
            ],
        );
    }

    /// Every variant of [`ConditionKind`] whose `flux_resource()` is
    /// `Some` uniquely names its FluxResource variant (no two
    /// ConditionKind arms may fetch the SAME FluxResource — that
    /// would signal a redundant closed-set entry). Peers the
    /// `every_variants_api_version_and_kind_are_distinct_across_the_closed_set`
    /// pin on the sibling [`FluxResource`] closed set.
    #[test]
    fn flux_resource_projection_is_injective_on_the_some_arms() {
        let mut seen = std::collections::HashSet::new();
        for k in ConditionKind::ALL {
            if let Some(fr) = k.flux_resource() {
                assert!(
                    seen.insert(fr),
                    "duplicate FluxResource projection at {k:?}: {fr:?}",
                );
            }
        }
    }

    /// `flux_resource` is `const fn` — the projection is reachable
    /// at compile time. A regression that dropped the `const`
    /// qualifier would fail-loudly here rather than as a wrong-slot
    /// runtime dispatch at every consumer callsite.
    #[test]
    fn flux_resource_projection_is_const_fn_reachable() {
        const K: Option<FluxResource> = ConditionKind::KustomizationHealthy.flux_resource();
        const H: Option<FluxResource> = ConditionKind::HelmReleaseReleased.flux_resource();
        const P: Option<FluxResource> = ConditionKind::ProcessPhase.flux_resource();
        assert_eq!(K, Some(FluxResource::Kustomization));
        assert_eq!(H, Some(FluxResource::HelmRelease));
        assert_eq!(P, None);
    }
}
