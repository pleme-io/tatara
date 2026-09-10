//! Compliance bindings — CRD-facing with bridges to `tatara_core::compliance_binding`.

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
    Default,
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
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

// `impl FromStr for VerificationPhase` +
// `impl tatara_lisp::ClosedSet for VerificationPhase` +
// `impl fmt::Display for VerificationPhase` +
// `pub struct UnknownVerificationPhase(pub String)` are all
// generated by `#[derive(tatara_closed_set::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown, display)]` on the
// enum declaration above. `label` delegates to the inherent
// `VerificationPhase::as_str` (which matches the serde
// `rename_all = "PascalCase"` projection AND the CRD `enum:`
// enumeration verbatim — pinned by
// `verification_phase_as_str_matches_serde`). The auto-derived
// carrier label "verification phase" matches the prior hand-rolled
// `#[error("unknown verification phase: {0}")]` annotation
// byte-for-byte. Symmetric to every other `#[derive(DeriveClosedSet)]`
// implementor across the crate.

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

/// Slice-level `(VerificationPhase, presence)` probe on any
/// `&[ComplianceBinding]` — the ONE substrate primitive that owns the
/// `.iter().any(|b| b.phase == K)` walk shape for the compliance-
/// binding vector. Callers compose the answer they want on top:
/// `spec.compliance.bindings.has_verification_phase(kind)` for the
/// point-domain `verification-phase-<kind>` require-tag family, a
/// coherence check that verifies "every `PlanTime` binding predicates
/// on a framework the fleet publishes", an editor completion listing
/// which [`VerificationPhase`] gates the operator authored — every
/// future consumer reaches this ONE primitive through
/// `slice.has_verification_phase(k)` instead of restating the
/// `.iter().any` closure body.
///
/// # Third instance in the slice-level presence-probe algebra
///
/// Same axis, same shape, third instance in the workspace-wide
/// slice-level closed-set-driven presence-probe algebra alongside
/// [`crate::boundary::ConditionSliceExt::has_kind`] on `&[Condition]`
/// and [`crate::spec::DependsOnSliceExt::has_must_reach`] on
/// `&[DependsOn]`. All three live one composition boundary below the
/// tagged-union-parent probes ([`crate::intent::Intent::has`],
/// [`crate::lifetime::Lifetime::has`],
/// [`crate::boundary::Boundary::has_condition_kind`]) at the
/// (`&self`, `K`) → `bool` signature; a future normalization at the
/// slice-level probe shape (widening the return to
/// `Option<&ComplianceBinding>` for deeper diagnostics, adding a
/// debug-build assertion on redundant duplicate `(framework,
/// control_id)` pairs at the same phase, switching to a linear scan
/// that also counts matches) lands at ONE site here and every
/// downstream `slice.has_verification_phase(K)` callsite picks it up
/// mechanically.
///
/// # Compounding
///
/// The `verification-phase-<kind>` require-tag prefix family in
/// `tatara-reconciler::bin::tatara-check` composes this primitive with
/// the closed-set `FromStr` autoderived on [`VerificationPhase`]
/// through the `strip_and_classify_prefixed_kind` substrate to publish
/// a sixth closed-set-driven prefix family byte-for-byte symmetrical
/// with `intent-<kind>` / `lifetime-<kind>` / `condition-<kind>` /
/// `must-reach-<kind>` / `sighup-<kind>`. Coexists with the coarse
/// `compliance` fixed tag (which answers "does this spec carry ANY
/// compliance binding") — the two tags publish distinct answers.
/// A future fourth [`VerificationPhase`] variant added to `ALL` (a
/// hypothetical `Continuous` checkpoint) reaches every downstream
/// through the SAME closed-set walk with no per-caller edit.
///
/// Theory anchor: THEORY.md §II.1 invariant 5 — composition preserves
/// proofs; the per-slice `phase` walk lives at ONE substrate site so
/// every downstream (require-tag classifier, coherence check, editor
/// completion) binds through the SAME shape rather than restating the
/// `.iter().any(|b| b.phase == K)` closure body at each callsite.
/// THEORY.md §VI.1 — generation over composition; a future
/// [`VerificationPhase`] variant lands at ONE `ALL` entry + ONE
/// `as_str` arm on the closed set and the presence probe picks it up
/// mechanically without further per-consumer edits.
pub trait ComplianceBindingSliceExt {
    /// True iff at least one [`ComplianceBinding`] in this slice
    /// verifies at the given [`VerificationPhase`]. The single-slice
    /// presence probe every consumer of the `(&[ComplianceBinding],
    /// VerificationPhase) -> bool` shape composes against.
    fn has_verification_phase(&self, kind: VerificationPhase) -> bool;
}

impl ComplianceBindingSliceExt for [ComplianceBinding] {
    fn has_verification_phase(&self, kind: VerificationPhase) -> bool {
        self.iter().any(|b| b.phase == kind)
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
    /// delegates to `<Self as tatara_closed_set::ClosedSet>::parse_label`,
    /// so this helper exercises the same code path the reconciler
    /// hits when parsing a CRD `enum:`-validated value back to the
    /// typed phase.
    #[test]
    fn verification_phase_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<VerificationPhase>();
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
        crate::tagged_union::assert_label_matches_serde_serialization::<VerificationPhase>();
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift.
    #[test]
    fn verification_phase_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<VerificationPhase>();
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
        use std::str::FromStr;
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

    // ── ComplianceBindingSliceExt::has_verification_phase substrate pins ──
    //
    // Fail-before-pass-after granularity: `ComplianceBindingSliceExt`
    // did not exist before this commit — the `(&[ComplianceBinding],
    // VerificationPhase) -> bool` walk shape was not spelled anywhere in
    // the workspace. The lift opens the third instance in the slice-
    // level closed-set-driven presence-probe algebra (peer of
    // `ConditionSliceExt::has_kind` on `&[Condition]` and
    // `DependsOnSliceExt::has_must_reach` on `&[DependsOn]`), enabling
    // the sixth `verification-phase-<kind>` require-tag prefix family in
    // `tatara-reconciler::bin::tatara-check` to compose against ONE
    // substrate site rather than restating the `.iter().any(|b| b.phase
    // == K)` closure body inline at the classifier.

    fn binding_at(phase: VerificationPhase) -> ComplianceBinding {
        ComplianceBinding {
            framework: "nist-800-53".into(),
            control_id: "SC-7".into(),
            phase,
            description: None,
        }
    }

    /// EMPTY-SLICE pin — an empty `&[ComplianceBinding]` returns
    /// `false` for EVERY [`VerificationPhase`]. Sweep
    /// [`VerificationPhase::ALL`] so a new variant added without a
    /// matching arm in the primitive surfaces at rustc's exhaustiveness
    /// gate on the ALL literal (arity forced by `[Self; 3]`) rather than
    /// as a silent false-positive at every downstream callsite composing
    /// this primitive.
    #[test]
    fn compliance_binding_slice_has_verification_phase_returns_false_on_empty_slice_for_every_kind()
    {
        let empty: &[ComplianceBinding] = &[];
        for kind in VerificationPhase::ALL {
            assert!(
                !empty.has_verification_phase(kind),
                "empty slice must return false for {kind:?}",
            );
        }
    }

    /// PER-VARIANT pin — a single-element slice returns `true` for
    /// exactly the phase it carries, `false` for every other variant.
    /// Sweep the [`VerificationPhase::ALL`] × ALL cross so a regression
    /// that (a) hard-coded the arm to a single kind (silently returning
    /// true for every populated slice regardless of query kind), or
    /// (b) matched on [`ComplianceBinding::framework`] instead of
    /// [`ComplianceBinding::phase`] fails HERE at the substrate
    /// primitive.
    #[test]
    fn compliance_binding_slice_has_verification_phase_reads_phase_field_per_variant() {
        for populated in VerificationPhase::ALL {
            let slice = [binding_at(populated)];
            for query in VerificationPhase::ALL {
                let expected = query == populated;
                assert_eq!(
                    slice.has_verification_phase(query),
                    expected,
                    "populated={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// MULTI-ENTRY pin — a slice with multiple entries returns `true`
    /// for every phase that appears at any position (existential
    /// quantifier over the slice), `false` for phases that appear at
    /// no position. Locks the `any` semantics so a regression that
    /// collapsed to a `first`-only probe (`slice.first().map_or(false,
    /// |b| b.phase == kind)`) fails here even though the single-element
    /// per-variant pin above passes.
    #[test]
    fn compliance_binding_slice_has_verification_phase_scans_beyond_the_first_position() {
        let slice = [
            binding_at(VerificationPhase::PlanTime),
            binding_at(VerificationPhase::PostConvergence),
        ];
        for present in [
            VerificationPhase::PlanTime,
            VerificationPhase::PostConvergence,
        ] {
            assert!(
                slice.has_verification_phase(present),
                "phase at any position must resolve true: {present:?}",
            );
        }
        assert!(
            !slice.has_verification_phase(VerificationPhase::AtBoundary),
            "phase absent from the slice must resolve false: AtBoundary",
        );
    }
}
