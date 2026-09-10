//! `ProcessSpec` sub-structures — IdentitySpec, DependsOn, SignalPolicy.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::phase::ProcessPhase;
use crate::signal::SighupStrategy;

/// Identity configuration for a Process.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdentitySpec {
    /// Parent PID path (None for init/PID 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Human name override — if set, used verbatim instead of the content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_override: Option<String>,
}

/// Dependency edge — constrains this Process to wait for another to reach a phase.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DependsOn {
    /// Target Process `metadata.name`.
    pub name: String,
    /// Target Process namespace. Defaults to this Process's namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Minimum phase the target must reach before we proceed past Forking.
    #[serde(default)]
    pub must_reach: MustReachPhase,
}

/// Allowed "must reach" phases for a dependency — restricted to the
/// useful gating checkpoints `Running` (alive + boundary preconditions
/// held) and `Attested` (alive + boundary postconditions held + three-
/// pillar attestation written). Authoring a `DependsOn { must_reach:
/// Forking }` is meaningless; the closed set rules it out at the type
/// level.
///
/// Sibling closed-set lifts on the same `ProcessSpec` axis:
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`].
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
#[closed_set(via = "as_str", display, generate_unknown = "must-reach phase")]
pub enum MustReachPhase {
    Running,
    #[default]
    Attested,
}

impl MustReachPhase {
    /// The closed set of must-reach phases — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad and the typed
    /// `as_process_phase` projection. Adding a third variant (e.g. a
    /// future `Released` checkpoint that waits for the target Process to
    /// have exited cleanly) lands at one `ALL` entry, one `as_str` arm,
    /// and one `as_process_phase` arm — exhaustively checked by the
    /// compiler (the `[Self; 2]` array literal forces the arity).
    pub const ALL: [Self; 2] = [Self::Running, Self::Attested];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim AND the canonical
    /// `ProcessPhase::as_str()` projection on the phase this variant
    /// gates against. Used by Display (single source of truth), by
    /// `FromStr` to identify the variant from its annotation / status-
    /// field representation, and by operator-facing diagnostic strings
    /// (`tatara-reconciler::boundary::check_depends_on` stamps the
    /// required phase via `Display` rather than reaching for `{:?}`
    /// Debug formatting). Pinned by `must_reach_phase_as_str_matches_serde`
    /// AND by `must_reach_phase_as_str_matches_process_phase_as_str` so
    /// a rename on either side surfaces at one site.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Attested => "Attested",
        }
    }

    /// Typed projection into the canonical `ProcessPhase` this variant
    /// gates against. The `From<MustReachPhase> for ProcessPhase` impl
    /// delegates here so callers reach for whichever surface fits (the
    /// `From` for `into()` flows, this `const fn` for const contexts).
    /// Pinned by `must_reach_phase_from_delegates_to_as_process_phase`.
    pub const fn as_process_phase(self) -> ProcessPhase {
        match self {
            Self::Running => ProcessPhase::Running,
            Self::Attested => ProcessPhase::Attested,
        }
    }
}

// `impl FromStr for MustReachPhase` +
// `impl tatara_lisp::ClosedSet for MustReachPhase` +
// `impl fmt::Display for MustReachPhase` +
// `pub struct UnknownMustReachPhase(pub String)` are all generated
// by `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(via =
// "as_str", display, generate_unknown = "must-reach phase")]` on
// the enum declaration above. `label` delegates to the inherent
// `MustReachPhase::as_str` — the PascalCase wire-vocabulary
// projection stays load-bearing (matches the serde rename AND the
// canonical `ProcessPhase::as_str` of the phase this variant gates
// against, pinned by
// `must_reach_phase_as_str_matches_process_phase_as_str`), while
// generic `T: ClosedSet` consumers reach the STABLE workspace-wide
// name (`label`). The explicit `generate_unknown = "must-reach
// phase"` label carries the hyphenated wording that the
// auto-derived `pascal_to_spaced_lowercase("MustReachPhase")` →
// "must reach phase" projection cannot produce — the prior
// hand-rolled `#[error("unknown must-reach phase: {0}")]`
// annotation kept the hyphen, and the explicit attribute preserves
// it through the lift. Symmetric to every other
// `#[derive(DeriveClosedSet)]` implementor across the crate.

impl From<MustReachPhase> for ProcessPhase {
    fn from(v: MustReachPhase) -> Self {
        v.as_process_phase()
    }
}

/// Slice-level `(MustReachPhase, presence)` probe on any `&[DependsOn]`
/// — the ONE substrate primitive that owns the
/// `.iter().any(|d| d.must_reach == K)` walk shape past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold on the `Vec<DependsOn>`
/// axis. Callers compose the answer they want on top:
/// `spec.depends_on.has_must_reach(kind)` for the point-domain
/// `must-reach-<kind>` require-tag family, a coherence check that
/// wants "does any depended-on Process have to reach `Attested`
/// before this one proceeds", an editor completion listing which
/// `MustReachPhase` checkpoints the operator authored — every future
/// consumer reaches this ONE primitive through
/// `slice.has_must_reach(k)` instead of restating the `.iter().any`
/// closure body.
///
/// # Sibling to [`crate::boundary::ConditionSliceExt::has_kind`]
///
/// Same axis, same shape, second instance in the workspace-wide
/// slice-level closed-set-driven presence-probe algebra:
/// `ConditionSliceExt::has_kind` owns the `(&[Condition],
/// ConditionKind) -> bool` walk over the boundary's per-side vectors;
/// `DependsOnSliceExt::has_must_reach` owns the `(&[DependsOn],
/// MustReachPhase) -> bool` walk over the spec's dependency vector.
/// Both live one composition boundary below the tagged-union-parent
/// probes (`Intent::has`, `Lifetime::has`,
/// `Boundary::has_condition_kind`) at the (`&self`, `K`) → `bool`
/// signature, so a future normalization at the slice-level probe shape
/// (widening the return to `Option<&DependsOn>` for deeper
/// diagnostics, adding a debug-build assertion on redundant duplicate
/// entries, switching to a linear scan that also counts matches) lands
/// at ONE site here and every downstream `slice.has_must_reach(K)`
/// callsite picks it up mechanically.
///
/// # Compounding
///
/// The `must-reach-<kind>` require-tag prefix family in
/// `tatara-reconciler::bin::tatara-check` composes this primitive with
/// the closed-set `FromStr` autoderived on [`MustReachPhase`] through
/// the `strip_and_classify_prefixed_kind` substrate to publish a
/// fourth closed-set-driven prefix family byte-for-byte symmetrical
/// with `intent-<kind>` / `lifetime-<kind>` / `condition-<kind>`. A
/// future [`MustReachPhase`] variant added to `ALL` (a hypothetical
/// `Released` checkpoint that waits for the target to have reached a
/// terminal exit; see the SUBSET CONTRACT test on
/// `must_reach_phase_projects_only_to_live_checkpoints` for the
/// semantic gate that guards this) reaches every downstream through
/// the SAME closed-set walk with no per-caller edit.
///
/// Theory anchor: THEORY.md §II.1 invariant 5 — composition preserves
/// proofs; the per-slice `must_reach` walk lives at ONE substrate site
/// so every downstream (require-tag classifier, coherence check,
/// editor completion) binds through the SAME shape rather than
/// restating the `.iter().any(|d| d.must_reach == K)` closure body at
/// each callsite. THEORY.md §VI.1 — generation over composition; a
/// future [`MustReachPhase`] variant lands at ONE `ALL` entry + ONE
/// `as_str` arm on the closed set and the presence probe picks it up
/// mechanically without further per-consumer edits.
pub trait DependsOnSliceExt {
    /// True iff at least one [`DependsOn`] in this slice gates on the
    /// given [`MustReachPhase`] checkpoint. The single-slice
    /// presence probe every consumer of the `(Vec<DependsOn>,
    /// MustReachPhase) -> bool` shape composes against.
    fn has_must_reach(&self, kind: MustReachPhase) -> bool;
}

impl DependsOnSliceExt for [DependsOn] {
    fn has_must_reach(&self, kind: MustReachPhase) -> bool {
        self.iter().any(|d| d.must_reach == kind)
    }
}

/// Signal policy — how the Process responds to signals.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignalPolicy {
    /// Grace before escalating SIGTERM → SIGKILL.
    #[serde(default = "crate::serde_defaults::default_sigterm_grace_seconds")]
    pub sigterm_grace_seconds: u32,
    /// Permit force-reap via SIGKILL (default: allow).
    #[serde(default = "crate::serde_defaults::default_true")]
    pub sigkill_force: bool,
    /// How SIGHUP is handled.
    #[serde(default)]
    pub sighup_strategy: SighupStrategy,
    /// Start suspended — requires SIGCONT to transition past Forking.
    #[serde(default)]
    pub start_suspended: bool,
}

impl Default for SignalPolicy {
    fn default() -> Self {
        Self {
            sigterm_grace_seconds: crate::serde_defaults::default_sigterm_grace_seconds(),
            sigkill_force: true,
            sighup_strategy: SighupStrategy::default(),
            start_suspended: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn must_reach_default_is_attested() {
        assert_eq!(MustReachPhase::default(), MustReachPhase::Attested);
    }

    #[test]
    fn signal_policy_defaults() {
        let p = SignalPolicy::default();
        assert_eq!(p.sigterm_grace_seconds, 480);
        assert!(p.sigkill_force);
        assert!(!p.start_suspended);
    }

    // ── closed-set algebra for MustReachPhase (ALL × as_str × FromStr ×
    //    as_process_phase) ──────────────────────────────────────────────

    /// Structural well-formedness of [`MustReachPhase`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through `label ↔
    /// parse_label`, labels are pairwise distinct, `""` is outside the
    /// closed set) at ONE call site. Replaces the hand-derived
    /// `must_reach_phase_all_is_unique_and_complete` +
    /// `must_reach_phase_roundtrip_via_as_str` + the empty-input arm
    /// of `unknown_must_reach_phase_errors`. `FromStr` delegates to
    /// `<Self as tatara_closed_set::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the reconciler hits when parsing
    /// a CRD `enum:`-validated value back to the typed checkpoint.
    #[test]
    fn must_reach_phase_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<MustReachPhase>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site.
    #[test]
    fn must_reach_phase_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<MustReachPhase>();
    }

    /// CROSS-CRATE CANONICAL-KEY CONTRACT: `MustReachPhase::as_str()`
    /// matches the canonical `ProcessPhase::as_str()` of the phase it
    /// projects to. The two enums share the PascalCase wire format
    /// because `MustReachPhase` is a typed subset of `ProcessPhase`'s
    /// safe gating checkpoints; a rename on either side (a phase
    /// rename in `ProcessPhase::as_str` OR an `as_str` arm typo here)
    /// surfaces here at one site, not buried in a reconciler diagnostic
    /// that quietly drifted away from the typed-phase surface.
    #[test]
    fn must_reach_phase_as_str_matches_process_phase_as_str() {
        for kind in MustReachPhase::ALL {
            assert_eq!(
                kind.as_str(),
                kind.as_process_phase().as_str(),
                "MustReachPhase::as_str() and ProcessPhase::as_str() drift for {kind:?}",
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. If a reviewer
    /// accidentally re-introduces an inline match in Display, this test
    /// would fail the moment a variant rename touches one site but not
    /// the other.
    #[test]
    fn must_reach_phase_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<MustReachPhase>();
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / non-checkpoint phase names —
    /// and the error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    /// Non-checkpoint phases like `Pending` / `Failed` / `Reaped`
    /// (which are legal `ProcessPhase`s but NOT valid
    /// `MustReachPhase` checkpoints) MUST fail to parse — that's the
    /// whole point of the closed subset. The empty-input arm is
    /// pinned by [`must_reach_phase_is_well_formed_closed_set`] via
    /// the `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownMustReachPhase`]
    /// newtype, which the trait's `make_unknown` can't see, AND the
    /// closed-subset contract (non-checkpoint phases reject) the
    /// trait's structural surface can't express.
    #[test]
    fn unknown_must_reach_phase_errors() {
        use std::str::FromStr;
        for bad in [
            "running", "ATTESTED", "Atested", "Pending", "Failed", "Reaped",
        ] {
            let err = MustReachPhase::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// DELEGATION CONTRACT: the `From<MustReachPhase> for ProcessPhase`
    /// impl agrees with the typed `as_process_phase()` projection it
    /// delegates to, for every variant. A regression that re-introduces
    /// an inline match in the `From` impl fails here the moment
    /// `as_process_phase` is the source of truth. Pairs with the
    /// `as_str` cross-crate test above — together they pin that the
    /// projection's value AND wire-format are coherent.
    #[test]
    fn must_reach_phase_from_delegates_to_as_process_phase() {
        for kind in MustReachPhase::ALL {
            let via_from: ProcessPhase = kind.into();
            assert_eq!(
                via_from,
                kind.as_process_phase(),
                "From<MustReachPhase> drift for {kind:?}",
            );
        }
    }

    /// SUBSET CONTRACT: every `MustReachPhase` variant projects to a
    /// `ProcessPhase` that is `is_running()` — i.e. one of the live
    /// gating checkpoints (`Running` or `Attested`). This pins the
    /// closed subset's invariant at the type level: a future
    /// `MustReachPhase::Released` (e.g. wait for the target to reach
    /// `Reaped`) would FAIL this test, forcing the author to either
    /// rename the predicate (`is_running` is wrong for that case) or
    /// reconsider whether `MustReachPhase` is the right surface (it
    /// shouldn't be — `Released` belongs on a separate "wait for
    /// terminal-reached gate" closed set). The compiler enforces
    /// closure-on-arity; this test enforces closure-on-semantics.
    #[test]
    fn must_reach_phase_projects_only_to_live_checkpoints() {
        for kind in MustReachPhase::ALL {
            let p = kind.as_process_phase();
            assert!(
                p.is_running(),
                "{kind:?} → {p:?} must be a live checkpoint (Running or Attested)",
            );
        }
    }

    /// INJECTIVITY CONTRACT: distinct `MustReachPhase` variants project
    /// to distinct `ProcessPhase` values. Pairing this with the subset
    /// contract above forces a future variant addition to land on a
    /// fresh live checkpoint — collapsing two `MustReachPhase` variants
    /// onto the same `ProcessPhase` (e.g. two flavors of `Running`)
    /// silently makes `from` lossy, which `tatara-reconciler::boundary::
    /// check_depends_on`'s diagnostic ("need {required}") would
    /// quietly degrade.
    #[test]
    fn must_reach_phase_projection_is_injective() {
        let mut seen = std::collections::HashSet::new();
        for kind in MustReachPhase::ALL {
            let p = kind.as_process_phase();
            assert!(
                seen.insert(p),
                "MustReachPhase projection collision: {kind:?} → {p:?}",
            );
        }
        assert_eq!(seen.len(), MustReachPhase::ALL.len());
    }

    // ── DependsOnSliceExt::has_must_reach substrate pins ─────────────
    //
    // Fail-before-pass-after granularity: `DependsOnSliceExt` did not
    // exist before this commit — the `(&[DependsOn], MustReachPhase)
    // -> bool` walk did not have a substrate owner yet. The lift places
    // the per-slice presence probe on ONE site so a future consumer
    // (require-tag classifier, coherence check, editor completion)
    // composes against the SAME primitive rather than restating the
    // `.iter().any(|d| d.must_reach == K)` closure body at each site.
    // The tests below sweep the (empty, per-variant, multi-entry) matrix
    // so a variant added to `MustReachPhase::ALL` without a matching
    // primitive arm surfaces at rustc's exhaustiveness gate on the ALL
    // literal (arity forced by `[Self; 2]`) rather than as a silent
    // false-positive at every downstream `slice.has_must_reach(K)`
    // callsite.

    fn dep_with(must_reach: MustReachPhase) -> DependsOn {
        DependsOn {
            name: "target".to_string(),
            namespace: None,
            must_reach,
        }
    }

    /// EMPTY-SLICE pin — an empty `&[DependsOn]` returns `false` for
    /// EVERY [`MustReachPhase`]. Sweep `MustReachPhase::ALL` so a new
    /// variant added without a matching arm in the primitive surfaces
    /// at rustc's exhaustiveness gate on the ALL literal (arity forced
    /// by `[Self; 2]`) rather than as a silent false-positive at every
    /// downstream callsite composing this primitive.
    #[test]
    fn depends_on_slice_has_must_reach_returns_false_on_empty_slice_for_every_kind() {
        let empty: &[DependsOn] = &[];
        for kind in MustReachPhase::ALL {
            assert!(
                !empty.has_must_reach(kind),
                "empty slice must return false for {kind:?}",
            );
        }
    }

    /// PER-VARIANT pin — a single-element slice returns `true` for
    /// exactly the checkpoint it gates on, `false` for every other
    /// variant. Sweep the `ALL × ALL` cross so a regression that
    /// (a) hard-coded the arm to a single kind (silently returning
    /// true for every populated slice regardless of query kind), or
    /// (b) matched on [`DependsOn::name`] instead of
    /// [`DependsOn::must_reach`] fails HERE at the substrate primitive.
    #[test]
    fn depends_on_slice_has_must_reach_reads_must_reach_field_per_variant() {
        for populated in MustReachPhase::ALL {
            let slice = [dep_with(populated)];
            for query in MustReachPhase::ALL {
                let expected = query == populated;
                assert_eq!(
                    slice.has_must_reach(query),
                    expected,
                    "populated={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// MULTI-ENTRY pin — a slice with multiple entries returns `true`
    /// for every kind that appears at any position (existential
    /// quantifier over the slice), `false` for kinds that appear at
    /// no position. Locks the `any` semantics so a regression that
    /// collapsed to a `first`-only probe (`slice.first().map_or(false,
    /// |d| d.must_reach == kind)`) fails here even though the
    /// single-element per-variant pin above passes.
    #[test]
    fn depends_on_slice_has_must_reach_scans_beyond_the_first_position() {
        // Two entries with distinct checkpoints — the `Attested` entry
        // sits at index 1, so a `first`-only regression on the
        // `Running`-at-index-0 arrangement returns the wrong answer for
        // an `Attested` query.
        let slice = [
            dep_with(MustReachPhase::Running),
            dep_with(MustReachPhase::Attested),
        ];
        for present in MustReachPhase::ALL {
            assert!(
                slice.has_must_reach(present),
                "kind at any position must resolve true: {present:?}",
            );
        }
        // Single-checkpoint slice — the OTHER variant must resolve
        // false. Pins the negative arm of the existential quantifier so
        // a regression that widened the probe (e.g. `_ => true` catchall)
        // fails here.
        let running_only = [
            dep_with(MustReachPhase::Running),
            dep_with(MustReachPhase::Running),
        ];
        assert!(
            !running_only.has_must_reach(MustReachPhase::Attested),
            "kind absent from every position must resolve false",
        );
    }
}
