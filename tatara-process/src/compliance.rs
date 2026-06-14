//! Compliance bindings — CRD-facing with bridges to `tatara_core::compliance_binding`.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use tatara_core::domain::compliance_binding as core;

use crate::phase::ProcessPhase;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
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

impl VerificationPhase {
    /// The closed set of verification phases — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad and the typed
    /// `gates_phase` projection over [`ProcessPhase`]. Adding a fourth
    /// variant lands at one `ALL` entry + one `as_str` arm + one
    /// `gates_phase` arm — exhaustively checked by the compiler (the
    /// `[Self; 3]` array literal forces the arity).
    ///
    /// Sibling closed-set lifts on the same `ProcessSpec` axis:
    /// [`crate::signal::SighupStrategy::ALL`],
    /// [`crate::spec::MustReachPhase::ALL`],
    /// [`crate::intent::WorkloadKind::ALL`],
    /// [`crate::export::ReportFormat::ALL`],
    /// [`crate::encapsulates::EncapsulationMode::ALL`],
    /// [`crate::export::ExportTrigger::ALL`],
    /// [`crate::lifetime::TeardownPolicy::ALL`],
    /// [`crate::boundary::ConditionKind::ALL`],
    /// [`crate::lifetime::LifetimeKind::ALL`],
    /// [`crate::intent::IntentKind::ALL`],
    /// [`crate::phase::ProcessPhase::ALL`],
    /// [`crate::signal::ProcessSignal::ALL`].
    pub const ALL: [Self; 3] = [Self::PlanTime, Self::AtBoundary, Self::PostConvergence];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim AND the CRD `enum:`
    /// enumeration the reconciler stamps on the
    /// `processes.tatara.pleme.io` schema. Pinned by
    /// `verification_phase_as_str_matches_serde` so a variant rename
    /// can't drift between the typed surface, the CRD enum, and the
    /// YAML wire format at one site.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlanTime => "PlanTime",
            Self::AtBoundary => "AtBoundary",
            Self::PostConvergence => "PostConvergence",
        }
    }

    /// Typed `const fn` projection onto the [`ProcessPhase`] gate the
    /// binding's verification blocks when it fails. Each variant maps
    /// to the earliest phase whose entry the binding can prevent:
    ///
    /// - `PlanTime` → `Some(Execing)` — the RENDER phase is what
    ///   PlanTime gates ("Before Execing — fails reconciliation if
    ///   violated"); a violated PlanTime control prevents the
    ///   `Forking → Execing` transition.
    /// - `AtBoundary` → `Some(Attested)` — the VERIFY phase ("gates
    ///   Running → Attested"); a violated AtBoundary control prevents
    ///   the `Running → Attested` transition.
    /// - `PostConvergence` → `None` — the binding is non-blocking
    ///   ("After Attested — continuous audit, emits events on
    ///   violation"); it never gates a transition.
    ///
    /// Single source of truth for the future reconciler control-plane
    /// compliance evaluator's "which transition would a failing
    /// binding block?" decision; pinned by
    /// `verification_phase_gates_phase_truth_table`. Closed-set match
    /// (not `matches!`) so adding a fourth variant triggers the
    /// compiler's exhaustiveness check at this site rather than
    /// silently defaulting to either group.
    pub const fn gates_phase(self) -> Option<ProcessPhase> {
        match self {
            Self::PlanTime => Some(ProcessPhase::Execing),
            Self::AtBoundary => Some(ProcessPhase::Attested),
            Self::PostConvergence => None,
        }
    }
}

impl fmt::Display for VerificationPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for VerificationPhase {
    type Err = UnknownVerificationPhase;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        <Self as tatara_lisp::ClosedSet>::parse_label(s)
    }
}

/// Plug [`VerificationPhase`] into the substrate-wide
/// [`tatara_lisp::ClosedSet`] trait — same four-method contract every
/// closed-set implementor in the workspace plugs into. The trait
/// method `label` delegates to the inherent
/// [`VerificationPhase::as_str`] (which matches the serde
/// `rename_all = "PascalCase"` projection AND the CRD `enum:`
/// enumeration verbatim — pinned by `verification_phase_as_str_matches_serde`).
impl tatara_lisp::ClosedSet for VerificationPhase {
    const ALL: &'static [Self] = &Self::ALL;
    type Unknown = UnknownVerificationPhase;
    fn label(self) -> &'static str {
        Self::as_str(self)
    }
    fn make_unknown(s: &str) -> Self::Unknown {
        UnknownVerificationPhase(s.to_owned())
    }
}

/// Typed parse failure carrying the offending input verbatim so the
/// operator-facing diagnostic surfaces the bad value, not a normalized
/// form. Symmetric to [`crate::signal::UnknownSighupStrategy`],
/// [`crate::spec::UnknownMustReachPhase`],
/// [`crate::intent::UnknownWorkloadKind`],
/// [`crate::export::UnknownReportFormat`],
/// [`crate::encapsulates::UnknownEncapsulationMode`],
/// [`crate::export::UnknownExportTrigger`],
/// [`crate::lifetime::UnknownTeardownPolicy`],
/// [`crate::boundary::UnknownConditionKind`], and
/// [`crate::phase::UnknownPhase`].
#[derive(Debug, thiserror::Error)]
#[error("unknown verification phase: {0}")]
pub struct UnknownVerificationPhase(pub String);

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

    // ── closed-set algebra contracts (ALL × as_str × FromStr × gates_phase) ──

    /// Structural well-formedness of [`VerificationPhase`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants
    /// (`ALL` is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the
    /// hand-derived `verification_phase_all_is_unique_and_complete` +
    /// `verification_phase_roundtrip_via_as_str` + the empty-input
    /// arm of the per-implementor unknown-error test. `FromStr`
    /// delegates to `<Self as tatara_lisp::ClosedSet>::parse_label`,
    /// so this helper exercises the same code path the reconciler
    /// hits when parsing a CRD `enum:`-validated value back to the
    /// typed phase.
    #[test]
    fn verification_phase_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<VerificationPhase>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface and the YAML wire format
    /// the reconciler / operator both read. NOT lifted into the
    /// `ClosedSet` testkit — `serde_json` is NOT a `tatara-lisp`
    /// dependency, and per-implementor serde-shape choices (PascalCase
    /// for CRD enums, snake_case for camelCase carriers, lowercase
    /// for Lisp keyword projections) make a generic helper a
    /// category error.
    #[test]
    fn verification_phase_as_str_matches_serde() {
        for phase in VerificationPhase::ALL {
            let serialized = serde_json::to_string(&phase).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                phase.as_str(),
                "as_str drift for {phase:?}: as_str={} serde={unquoted}",
                phase.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift.
    #[test]
    fn verification_phase_display_matches_as_str() {
        for phase in VerificationPhase::ALL {
            assert_eq!(phase.to_string(), phase.as_str());
        }
    }

    /// `FromStr` rejects domain-specific non-canonical inputs and
    /// the error echoes the input VERBATIM so the operator-facing
    /// diagnostic carries the offending value. Kept per-implementor
    /// because the verbatim-payload contract is a property of the
    /// per-enum `Unknown<X>(pub String)` newtype, not of the trait's
    /// structural surface. (The empty-input arm is now lifted into
    /// `verification_phase_is_well_formed_closed_set`; the
    /// case-drifted / hyphenated / extinct-variant arms stay here as
    /// they're representative non-canonical inputs the operator
    /// might supply.)
    #[test]
    fn unknown_verification_phase_errors() {
        for bad in [
            "plantime",
            "ATBOUNDARY",
            "Plan-Time",
            "post_convergence",
            "Continuous",
        ] {
            let err = VerificationPhase::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// TRUTH-TABLE CONTRACT: `gates_phase` agrees with the documented
    /// per-variant codomain (the phase whose entry a violated binding
    /// blocks, or `None` for non-blocking continuous-audit phases).
    #[test]
    fn verification_phase_gates_phase_truth_table() {
        assert_eq!(
            VerificationPhase::PlanTime.gates_phase(),
            Some(ProcessPhase::Execing)
        );
        assert_eq!(
            VerificationPhase::AtBoundary.gates_phase(),
            Some(ProcessPhase::Attested)
        );
        assert_eq!(VerificationPhase::PostConvergence.gates_phase(), None);
    }

    /// SUBSET CONTRACT: every `Some(target)` `gates_phase` projects to
    /// is a phase reachable as the destination of some legal
    /// `ProcessPhase::can_transition_to` edge. A future variant that
    /// projected to a `ProcessPhase` no transition leads into would
    /// FAIL here, forcing the author to either pick a real gate phase
    /// or extend `can_transition_to` deliberately. The reachability
    /// check is the cross-enum coherence proof — the typed-phase
    /// state machine and the verification-phase gate algebra agree on
    /// which phases are gateable.
    #[test]
    fn verification_phase_gates_phase_projects_to_reachable_phases() {
        for vp in VerificationPhase::ALL {
            if let Some(target) = vp.gates_phase() {
                let reachable = ProcessPhase::ALL
                    .into_iter()
                    .any(|src| src != target && src.can_transition_to(target));
                assert!(
                    reachable,
                    "{vp:?}.gates_phase() = Some({target:?}) but no legal transition lands on {target:?}",
                );
            }
        }
    }

    /// INJECTIVITY CONTRACT: distinct `Some` variants of `gates_phase`
    /// project to distinct `ProcessPhase`s. Pairing this with the
    /// subset contract above forces a future variant to land on a
    /// fresh gateable phase (or project to `None` and be a deliberate
    /// non-blocking auditor).
    #[test]
    fn verification_phase_gates_phase_is_injective() {
        let projections: Vec<ProcessPhase> = VerificationPhase::ALL
            .into_iter()
            .filter_map(VerificationPhase::gates_phase)
            .collect();
        let unique: std::collections::HashSet<_> = projections.iter().copied().collect();
        assert_eq!(
            projections.len(),
            unique.len(),
            "gates_phase projection is not injective: {projections:?}",
        );
    }
}
