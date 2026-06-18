//! `EncapsulatesSpec` — how a Process relates to pre-existing
//! in-cluster state.
//!
//! The substrate move: every long-running workload on a pleme-io
//! cluster — raw HelmReleases, Flux Kustomizations, bare Deployments
//! — becomes a Process without disruption. Three modes:
//!
//! * **Manage** (default) — Process IS the control loop. New
//!   HR/Kustomization emitted by the reconciler use ownerRefs
//!   pointing at the Process; cascade-delete on Reaped.
//!
//! * **Adopt** — Take over an existing HR/Kustomization in place.
//!   Reconciler emits a new HR with `releaseName` matching the
//!   running release; helm-controller adopts the existing release
//!   under new management. **No pod restart**; no values diff
//!   unless operator changes them. The original raw HR can be
//!   deleted from git after the takeover confirms.
//!
//! * **Observe** — Read-only awareness. Process watches the existing
//!   state for postcondition pillars + emits routing/exports/
//!   attestation, but does NOT modify or own the underlying
//!   HR/Kustomization. Useful for adding DNS + observability to
//!   legacy stacks without taking over.
//!
//! These compose progressively: Observe an HR first to confirm
//! shape, promote to Adopt for zero-downtime takeover, then to
//! Manage once the Process drives values.
//!
//! Lisp authoring:
//! ```lisp
//! :encapsulates (:kind (:existing-helm-release
//!                       :namespace    "akeyless"
//!                       :name         "akeyless-saas"
//!                       :release-name "akeyless-saas-consolidated")
//!                :mode Adopt)
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

/// How a Process wraps pre-existing in-cluster state.
///
/// Optional on `ProcessSpec` — None means the Process is greenfield
/// (Manage mode applied to nothing pre-existing). The render phase
/// branches on `kind` to decide whether to emit fresh resources or
/// reference/adopt running ones.
#[derive(DeriveTataraDomain, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defencapsulates")]
pub struct EncapsulatesSpec {
    /// What kind of pre-existing state.
    pub kind: EncapsulationKind,

    /// Reconciler's relationship to that state. Defaults to `Manage`
    /// (the operational default when `encapsulates` is set without
    /// an explicit mode).
    #[serde(default)]
    pub mode: EncapsulationMode,
}

/// Three concrete kinds the substrate knows how to wrap. Exactly-
/// one-Option pattern matching `Intent` / `Lifetime` — additive on
/// the wire, every variant typed.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EncapsulationKind {
    /// An existing FluxCD HelmRelease. The reconciler emits a new HR
    /// with the SAME `release_name` — helm-controller finds + adopts
    /// the in-cluster release without recreating Pods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_helm_release: Option<ExistingHelmRelease>,

    /// An existing FluxCD Kustomization. The reconciler stops emitting
    /// its own and instead references the existing one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_kustomization: Option<ExistingKustomization>,

    /// Pre-existing in-cluster workload (Deployment/StatefulSet/etc)
    /// not Flux-managed. The reconciler adds ownerRefs + emits
    /// routing only. The workload stays where it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bare_workload: Option<BareWorkload>,
}

/// Resolved enum view used by the render phase.
#[derive(Clone, Debug)]
pub enum EncapsulationKindVariant<'a> {
    ExistingHelmRelease(&'a ExistingHelmRelease),
    ExistingKustomization(&'a ExistingKustomization),
    BareWorkload(&'a BareWorkload),
}

impl EncapsulationKindVariant<'_> {
    /// Reverse projection — every borrowed variant knows its
    /// [`EncapsulationTarget`] discriminator. Pairs with
    /// [`EncapsulationTarget::select`] so
    /// `EncapsulationTarget::select(kind).map(|v| v.target())`
    /// round-trips the closed set on the populated side; pinned by
    /// `encapsulation_target_round_trips_through_variant_target`.
    /// Future target-keyed consumers (metric labels like
    /// `tatara_encapsulations_total{target="existingHelmRelease"}`,
    /// status reason strings, audit-trail classifiers, LSP completion
    /// lists) reach through this projection instead of pattern-matching
    /// the payload-carrying view.
    pub fn target(&self) -> EncapsulationTarget {
        match self {
            Self::ExistingHelmRelease(_) => EncapsulationTarget::ExistingHelmRelease,
            Self::ExistingKustomization(_) => EncapsulationTarget::ExistingKustomization,
            Self::BareWorkload(_) => EncapsulationTarget::BareWorkload,
        }
    }
}

/// Closed-set discriminator over `EncapsulationKind`'s three tagged-union
/// slots. Single source of truth that drives `EncapsulationKind::variant`'s
/// ambiguity + emptiness resolver, the `EncapsulationKindError::Empty`
/// diagnostic message, and the reverse `EncapsulationKindVariant::target`
/// projection. Adding a fourth encapsulation target (e.g., a future
/// `ExistingNamespace`, `ExistingDaemonSet`, or `ExistingService`) lands
/// at one `ALL` entry + one `as_str` arm + one `select` arm + one
/// `EncapsulationKindVariant::target` arm — exhaustively checked by the
/// compiler.
///
/// The (open authoring surface, closed typed discriminator) split mirrors
/// every other multi-Option tagged union on this `ProcessSpec` axis:
/// [`crate::intent::IntentKind`] discriminates [`crate::intent::Intent`];
/// [`crate::lifetime::LifetimeKind`] discriminates
/// [`crate::lifetime::Lifetime`];
/// [`crate::export::ArtifactKind`] discriminates
/// [`crate::export::ArtifactSource`];
/// [`crate::export::ChannelKind`] discriminates
/// [`crate::export::VectorChannel`]. The carrier here is named
/// `EncapsulationKind` (not `Encapsulation`) because it predates the
/// closed-set lift convention; `EncapsulationTarget` is the typed
/// discriminator the rest of the typescape projects through, named for
/// the semantic role each variant plays (a target of encapsulation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_lisp::DeriveClosedSet)]
#[closed_set(via = "as_str", generate_unknown, display)]
pub enum EncapsulationTarget {
    ExistingHelmRelease,
    ExistingKustomization,
    BareWorkload,
}

impl EncapsulationTarget {
    /// The closed set of encapsulation targets — single source of truth
    /// that drives `EncapsulationKind::variant`'s sweep so a variant
    /// added without an `ALL` entry never reaches the resolver. The
    /// `[Self; 3]` array literal forces the arity at compile time.
    pub const ALL: [Self; 3] = [
        Self::ExistingHelmRelease,
        Self::ExistingKustomization,
        Self::BareWorkload,
    ];

    /// Canonical camelCase wire-format key — matches the serde
    /// `rename_all = "camelCase"` field name on the corresponding
    /// `Option<…>` slot of `EncapsulationKind`. The
    /// `EncapsulationKindError::Empty` diagnostic composes the
    /// human-readable list from this projection so a new variant lands
    /// in the operator-facing diagnostic automatically via the `ALL`
    /// sweep, not via hand-maintained error-string drift. Pinned by
    /// `encapsulation_target_as_str_matches_field_name`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExistingHelmRelease => "existingHelmRelease",
            Self::ExistingKustomization => "existingKustomization",
            Self::BareWorkload => "bareWorkload",
        }
    }

    /// Project an `EncapsulationKind` borrow into the optional typed
    /// variant view for this target. Returns `None` iff the matching
    /// slot is `None`. Composes the closed-set sweep
    /// `EncapsulationKind::variant` loops over. Mirrors
    /// [`crate::intent::IntentKind::select`],
    /// [`crate::lifetime::LifetimeKind::select`],
    /// [`crate::export::ArtifactKind::select`], and
    /// [`crate::export::ChannelKind::select`].
    pub fn select<'a>(self, kind: &'a EncapsulationKind) -> Option<EncapsulationKindVariant<'a>> {
        match self {
            Self::ExistingHelmRelease => kind
                .existing_helm_release
                .as_ref()
                .map(EncapsulationKindVariant::ExistingHelmRelease),
            Self::ExistingKustomization => kind
                .existing_kustomization
                .as_ref()
                .map(EncapsulationKindVariant::ExistingKustomization),
            Self::BareWorkload => kind
                .bare_workload
                .as_ref()
                .map(EncapsulationKindVariant::BareWorkload),
        }
    }
}

// `impl FromStr for EncapsulationTarget` + `impl tatara_lisp::ClosedSet for
// EncapsulationTarget` + `impl std::fmt::Display for EncapsulationTarget` are
// generated by `#[derive(tatara_lisp::DeriveClosedSet)]` on the enum
// declaration above. `label` delegates to the inherent
// `EncapsulationTarget::as_str` via `#[closed_set(via = "as_str")]` so the
// camelCase wire-format projection stays load-bearing (matches the serde
// `rename_all = "camelCase"` field names on `EncapsulationKind` AND the
// `ENCAPSULATION_TARGET_LIST` slash-joined operator diagnostic verbatim)
// while generic `T: ClosedSet` consumers reach the STABLE workspace-wide
// name (`label`). The `display` flag emits the `f.write_str(self.as_str())`
// delegation block at the same proc-macro site rather than a hand-rolled
// `fmt::Display` block per implementor.

// `pub struct UnknownEncapsulationTarget(pub String)` is generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"encapsulation target"`
// matches the prior hand-rolled `#[error("unknown encapsulation target: {0}")]`
// verbatim — pinned generically by clause (5) of
// `tatara_lisp::assert_closed_set_well_formed::<EncapsulationTarget>()` (called
// from `encapsulation_target_is_well_formed_closed_set` in the test module).
// Symmetric to [`UnknownEncapsulationMode`], [`crate::export::UnknownArtifactKind`],
// [`crate::export::UnknownChannelKind`], and
// [`crate::lifetime::UnknownTeardownPolicy`].

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum EncapsulationKindError {
    #[error("encapsulation kind has no variant set (one of {0} required)")]
    Empty(&'static str),
    #[error("encapsulation kind has multiple variants set; exactly one required")]
    Ambiguous,
}

/// Slash-joined list of every `EncapsulationTarget::as_str()` — composed
/// once at compile time so `EncapsulationKindError::Empty`'s diagnostic
/// carries the closed-set summary without per-variant string drift.
/// Mirrors [`crate::intent::INTENT_KIND_LIST`] /
/// [`crate::export::ARTIFACT_KIND_LIST`] in shape; pinned by
/// `encapsulation_kind_error_empty_lists_every_target_in_canonical_order`.
const ENCAPSULATION_TARGET_LIST: &str = "existingHelmRelease/existingKustomization/bareWorkload";

impl EncapsulationKind {
    /// Resolve to exactly one variant. Errors on zero or many.
    ///
    /// Sweeps over [`EncapsulationTarget::ALL`] so a fourth variant added
    /// with an `ALL` entry is structurally honored at this site — no
    /// parallel `is_some()` count, no per-variant if-let chain, no
    /// `unreachable!()`. The Empty diagnostic carries the closed-set
    /// list via `ENCAPSULATION_TARGET_LIST`.
    pub fn variant(&self) -> Result<EncapsulationKindVariant<'_>, EncapsulationKindError> {
        use crate::tagged_union::{resolve, ResolveError};
        resolve(EncapsulationTarget::ALL.into_iter().map(|t| t.select(self))).map_err(|e| match e {
            ResolveError::None => EncapsulationKindError::Empty(ENCAPSULATION_TARGET_LIST),
            ResolveError::Many => EncapsulationKindError::Ambiguous,
        })
    }
}

/// Pointer to an existing FluxCD HelmRelease the Process wraps.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExistingHelmRelease {
    /// Namespace of the HelmRelease CR.
    pub namespace: String,
    /// Name of the HelmRelease CR.
    pub name: String,
    /// The `spec.releaseName` Helm used for the actual chart install.
    /// For Adopt mode, the reconciler's emitted HR matches this so
    /// helm-controller adopts in-place. Required because the HR's
    /// `metadata.name` and `spec.releaseName` aren't always equal.
    pub release_name: String,
}

/// Pointer to an existing FluxCD Kustomization the Process wraps.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExistingKustomization {
    pub namespace: String,
    pub name: String,
}

/// Pointer to a bare in-cluster workload (not Flux-managed). The
/// reconciler identifies the underlying Pods by `selector` and adds
/// ownerRefs / routing without emitting a new HR/Kustomization.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BareWorkload {
    /// Namespace the workload lives in.
    pub namespace: String,
    /// Label selector. Must match a single Deployment/StatefulSet/
    /// DaemonSet; multiple matches are a config error.
    pub selector: BTreeMap<String, String>,
}

/// Three modes the reconciler dispatches on at render time.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Serialize,
    Deserialize,
    JsonSchema,
    PartialEq,
    Eq,
    Hash,
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
pub enum EncapsulationMode {
    /// **Default** — Process IS the control loop for whatever is
    /// inside. Emitted HR/Kustomization carry the Process's
    /// ownerRefs; cascade-delete on Reaped.
    #[default]
    Manage,

    /// **Adopt** — Take over the existing release/kustomization in
    /// place. New HR emitted matches the existing `releaseName`;
    /// pods don't restart. Used during migration from raw HR → Process.
    Adopt,

    /// **Observe** — Read-only. Emit routing/exports/attestation but
    /// don't modify the underlying HR/Kustomization. Used to add
    /// DNS + observability to legacy stacks without taking over.
    Observe,
}

impl EncapsulationMode {
    /// The closed set of encapsulation modes — single source of truth
    /// that drives the `as_str` / Display / `FromStr` triad and the
    /// typed `emits_workload` / `preserves_release_name` dispatch.
    /// Adding a fourth variant lands at one `ALL` entry + one `as_str`
    /// arm + one arm in each of the two boolean projections —
    /// exhaustively checked by the compiler (the `[Self; 3]` array
    /// literal forces the arity).
    ///
    /// Sibling closed-set lifts on the same `ProcessSpec` axis:
    /// [`crate::export::ExportTrigger::ALL`],
    /// [`crate::lifetime::TeardownPolicy::ALL`],
    /// [`crate::intent::IntentKind::ALL`],
    /// [`crate::lifetime::LifetimeKind::ALL`],
    /// [`crate::boundary::ConditionKind::ALL`],
    /// [`crate::phase::ProcessPhase::ALL`],
    /// [`crate::signal::ProcessSignal::ALL`].
    pub const ALL: [Self; 3] = [Self::Manage, Self::Adopt, Self::Observe];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim. Used by Display
    /// (single source of truth), by `FromStr` to identify the variant
    /// from its annotation / status-field representation, and by
    /// operator-facing reason strings without reaching for `{:?}` Debug
    /// formatting. Pinned by `mode_as_str_matches_serde`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manage => "Manage",
            Self::Adopt => "Adopt",
            Self::Observe => "Observe",
        }
    }

    /// True iff the reconciler should emit (or re-emit) the
    /// underlying HR/Kustomization at render time.
    /// Observe ⇒ false; Manage/Adopt ⇒ true.
    ///
    /// Closed-set match (not `matches!`) so adding a fourth variant
    /// triggers the compiler's exhaustiveness check at this site
    /// rather than silently defaulting to `false`. ONE typed dispatch
    /// over the closed set that replaces the
    /// `mode == EncapsulationMode::Observe` hand-rolled equality at
    /// the reconciler's render entry — the truth table for "should
    /// this mode emit a workload?" is now owned by the typed surface,
    /// not by a pattern fragment two crates have to keep coherent.
    pub const fn emits_workload(self) -> bool {
        match self {
            Self::Manage | Self::Adopt => true,
            Self::Observe => false,
        }
    }

    /// True iff the reconciler should preserve the existing release
    /// name (so helm-controller adopts in-place). Only Adopt.
    ///
    /// Closed-set match (not `matches!`) so adding a fourth variant
    /// triggers the compiler's exhaustiveness check at this site.
    /// ONE typed dispatch that replaces the
    /// `mode == EncapsulationMode::Adopt` hand-rolled equality at the
    /// reconciler's `render_aplicacao` adoption-annotation branch.
    pub const fn preserves_release_name(self) -> bool {
        match self {
            Self::Adopt => true,
            Self::Manage | Self::Observe => false,
        }
    }
}

// `impl FromStr for EncapsulationMode` + `impl tatara_lisp::ClosedSet for
// EncapsulationMode` + `impl std::fmt::Display for EncapsulationMode` are
// generated by `#[derive(tatara_lisp::DeriveClosedSet)]` on the enum
// declaration above. `label` delegates to the inherent
// `EncapsulationMode::as_str` via `#[closed_set(via = "as_str")]` so the
// PascalCase wire-format projection stays load-bearing (matches the serde
// `rename_all = "PascalCase"` external-tag form on the wire AND the
// reconciler's `mode: {Manage,Adopt,Observe}` status-condition reason
// strings verbatim) while generic `T: ClosedSet` consumers reach the
// STABLE workspace-wide name (`label`). The `display` flag emits the
// `f.write_str(self.as_str())` delegation block at the same proc-macro
// site rather than a hand-rolled `fmt::Display` block per implementor.

// `pub struct UnknownEncapsulationMode(pub String)` is generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"encapsulation mode"`
// matches the prior hand-rolled `#[error("unknown encapsulation mode: {0}")]`
// verbatim — pinned generically by clause (5) of
// `tatara_lisp::assert_closed_set_well_formed::<EncapsulationMode>()` (called
// from `mode_is_well_formed_closed_set` in the test module).
// Symmetric to [`UnknownEncapsulationTarget`], [`crate::export::UnknownExportTrigger`],
// [`crate::lifetime::UnknownTeardownPolicy`],
// [`crate::boundary::UnknownConditionKind`], and
// [`crate::phase::UnknownPhase`].

#[cfg(test)]
mod tests {
    use super::*;

    fn akeyless_adopt() -> EncapsulatesSpec {
        EncapsulatesSpec {
            kind: EncapsulationKind {
                existing_helm_release: Some(ExistingHelmRelease {
                    namespace: "akeyless".into(),
                    name: "akeyless-saas".into(),
                    release_name: "akeyless-saas-consolidated".into(),
                }),
                ..EncapsulationKind::default()
            },
            mode: EncapsulationMode::Adopt,
        }
    }

    #[test]
    fn kind_empty_errors() {
        let k = EncapsulationKind::default();
        assert_eq!(
            k.variant().unwrap_err(),
            EncapsulationKindError::Empty(ENCAPSULATION_TARGET_LIST)
        );
    }

    #[test]
    fn kind_existing_hr_resolves() {
        let s = akeyless_adopt();
        match s.kind.variant().unwrap() {
            EncapsulationKindVariant::ExistingHelmRelease(h) => {
                assert_eq!(h.namespace, "akeyless");
                assert_eq!(h.release_name, "akeyless-saas-consolidated");
            }
            other => panic!("expected ExistingHelmRelease, got {other:?}"),
        }
    }

    #[test]
    fn kind_two_variants_ambiguous() {
        let k = EncapsulationKind {
            existing_helm_release: Some(ExistingHelmRelease {
                namespace: "ns".into(),
                name: "n".into(),
                release_name: "r".into(),
            }),
            existing_kustomization: Some(ExistingKustomization {
                namespace: "ns".into(),
                name: "n".into(),
            }),
            ..EncapsulationKind::default()
        };
        assert_eq!(k.variant().unwrap_err(), EncapsulationKindError::Ambiguous);
    }

    #[test]
    fn mode_dispatch() {
        assert!(EncapsulationMode::Manage.emits_workload());
        assert!(EncapsulationMode::Adopt.emits_workload());
        assert!(!EncapsulationMode::Observe.emits_workload());

        assert!(!EncapsulationMode::Manage.preserves_release_name());
        assert!(EncapsulationMode::Adopt.preserves_release_name());
        assert!(!EncapsulationMode::Observe.preserves_release_name());
    }

    #[test]
    fn mode_default_is_manage() {
        assert_eq!(EncapsulationMode::default(), EncapsulationMode::Manage);
    }

    // ── closed-set algebra for EncapsulationMode (ALL × as_str ×
    //    Display × FromStr × emits_workload × preserves_release_name) ─

    /// Structural well-formedness of [`EncapsulationMode`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `mode_all_is_unique_and_complete` +
    /// `mode_roundtrip_via_as_str` + the empty-input arm of
    /// `unknown_encapsulation_mode_errors`. `FromStr` delegates to
    /// `<Self as tatara_lisp::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the reconciler hits when parsing a
    /// CRD `enum:`-validated `mode` value back to the typed mode.
    #[test]
    fn mode_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<EncapsulationMode>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface and the YAML wire format
    /// the reconciler / operator both read.
    #[test]
    fn mode_as_str_matches_serde() {
        for mode in EncapsulationMode::ALL {
            let serialized = serde_json::to_string(&mode).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                mode.as_str(),
                "as_str drift for {mode:?}: as_str={} serde={unquoted}",
                mode.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. If a reviewer
    /// accidentally re-introduces an inline match in Display, this
    /// test would fail the moment a variant rename touches one site
    /// but not the other.
    #[test]
    fn mode_display_matches_as_str() {
        for mode in EncapsulationMode::ALL {
            assert_eq!(mode.to_string(), mode.as_str());
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / unrelated — and the error
    /// echoes the input verbatim so the operator-facing diagnostic
    /// carries the offending value, not a normalized form. The
    /// empty-input arm is pinned by
    /// [`mode_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownEncapsulationMode`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_encapsulation_mode_errors() {
        use std::str::FromStr;
        for bad in ["manage", "ADOPT", "Observed", "Wrap"] {
            let err = EncapsulationMode::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// TRUTH-TABLE CONTRACT: `emits_workload` / `preserves_release_name`
    /// agree with the documented (mode) -> (bool, bool) table for every
    /// variant. A new variant in `EncapsulationMode` without extending
    /// either projection's match is caught by the compiler (closed-set
    /// match in each method); adding a variant without extending its
    /// truth row is caught here.
    #[test]
    fn mode_projection_truth_table() {
        let table: &[(EncapsulationMode, bool, bool)] = &[
            // (mode, emits_workload, preserves_release_name)
            (EncapsulationMode::Manage, true, false),
            (EncapsulationMode::Adopt, true, true),
            (EncapsulationMode::Observe, false, false),
        ];
        assert_eq!(table.len(), EncapsulationMode::ALL.len());
        for (mode, emits, preserves) in table {
            assert_eq!(
                mode.emits_workload(),
                *emits,
                "emits_workload drift for {mode:?}"
            );
            assert_eq!(
                mode.preserves_release_name(),
                *preserves,
                "preserves_release_name drift for {mode:?}"
            );
        }
    }

    /// DRIFT-PROOF CONTRACT: the hand-rolled
    /// `mode == EncapsulationMode::Observe` and
    /// `mode == EncapsulationMode::Adopt` checks the reconciler's
    /// `render` function used pre-lift agree with the typed
    /// projections for every variant in `ALL`. A regression that
    /// re-introduces a raw `==` against a variant name fails here:
    /// `!emits_workload()` IS "Observe mode" and
    /// `preserves_release_name()` IS "Adopt mode", expressed as a
    /// property of the typed surface rather than a pattern fragment
    /// two crates have to keep coherent.
    #[test]
    fn mode_typed_projections_replace_raw_equality() {
        for mode in EncapsulationMode::ALL {
            assert_eq!(
                !mode.emits_workload(),
                mode == EncapsulationMode::Observe,
                "!emits_workload() drift for {mode:?}"
            );
            assert_eq!(
                mode.preserves_release_name(),
                mode == EncapsulationMode::Adopt,
                "preserves_release_name() drift for {mode:?}"
            );
        }
    }

    #[test]
    fn serde_round_trip_via_yaml() {
        let s = akeyless_adopt();
        let yaml = serde_yaml::to_string(&s).unwrap();
        assert!(yaml.contains("existingHelmRelease:"));
        assert!(yaml.contains("releaseName: akeyless-saas-consolidated"));
        assert!(yaml.contains("mode: Adopt"));
        let back: EncapsulatesSpec = serde_yaml::from_str(&yaml).unwrap();
        assert!(back.kind.existing_helm_release.is_some());
        assert_eq!(back.mode, EncapsulationMode::Adopt);
    }

    #[test]
    fn bare_workload_selector_round_trips() {
        let mut sel = BTreeMap::new();
        sel.insert("app".into(), "akeyless-gator".into());
        sel.insert("tier".into(), "prod".into());
        let s = EncapsulatesSpec {
            kind: EncapsulationKind {
                bare_workload: Some(BareWorkload {
                    namespace: "legacy".into(),
                    selector: sel,
                }),
                ..EncapsulationKind::default()
            },
            mode: EncapsulationMode::Observe,
        };
        let yaml = serde_yaml::to_string(&s).unwrap();
        assert!(yaml.contains("bareWorkload:"));
        assert!(yaml.contains("app: akeyless-gator"));
        assert!(yaml.contains("mode: Observe"));
        let back: EncapsulatesSpec = serde_yaml::from_str(&yaml).unwrap();
        match back.kind.variant().unwrap() {
            EncapsulationKindVariant::BareWorkload(b) => {
                assert_eq!(b.selector.len(), 2);
                assert_eq!(
                    b.selector.get("app").map(String::as_str),
                    Some("akeyless-gator")
                );
            }
            other => panic!("expected BareWorkload, got {other:?}"),
        }
    }

    #[test]
    fn lisp_round_trip_existing_hr() {
        let src = r#"
            (defencapsulates akeyless-adopt
              :kind (:existing-helm-release
                     (:namespace    "akeyless"
                      :name         "akeyless-saas"
                      :release-name "akeyless-saas-consolidated"))
              :mode Adopt)
        "#;
        let defs: Vec<tatara_lisp::NamedDefinition<EncapsulatesSpec>> =
            tatara_lisp::compile_named::<EncapsulatesSpec>(src).expect("compile");
        let d = &defs[0];
        assert_eq!(d.name, "akeyless-adopt");
        assert_eq!(d.spec.mode, EncapsulationMode::Adopt);
        let h = d.spec.kind.existing_helm_release.as_ref().unwrap();
        assert_eq!(h.namespace, "akeyless");
        assert_eq!(h.release_name, "akeyless-saas-consolidated");
    }

    #[test]
    fn lisp_default_mode_is_manage() {
        // `:mode` omitted ⇒ Manage (Default derive).
        let src = r#"
            (defencapsulates greenfield
              :kind (:existing-kustomization
                     (:namespace "flux-system"
                      :name      "openclaw")))
        "#;
        let defs: Vec<tatara_lisp::NamedDefinition<EncapsulatesSpec>> =
            tatara_lisp::compile_named::<EncapsulatesSpec>(src).expect("compile");
        let d = &defs[0];
        assert_eq!(d.spec.mode, EncapsulationMode::Manage);
    }

    // ── closed-set algebra for EncapsulationTarget (ALL × as_str ×
    //    Display × FromStr × select × EncapsulationKindVariant::target) ─

    /// Construct an `EncapsulationKind` with one slot populated — the
    /// composable construction table the closed-set property tests
    /// loop over. Mirrors `single_slot_source` in
    /// [`crate::export`] in shape.
    fn single_slot_kind(target: EncapsulationTarget) -> EncapsulationKind {
        match target {
            EncapsulationTarget::ExistingHelmRelease => EncapsulationKind {
                existing_helm_release: Some(ExistingHelmRelease {
                    namespace: "ns".into(),
                    name: "hr".into(),
                    release_name: "rel".into(),
                }),
                ..EncapsulationKind::default()
            },
            EncapsulationTarget::ExistingKustomization => EncapsulationKind {
                existing_kustomization: Some(ExistingKustomization {
                    namespace: "ns".into(),
                    name: "ks".into(),
                }),
                ..EncapsulationKind::default()
            },
            EncapsulationTarget::BareWorkload => {
                let mut sel = BTreeMap::new();
                sel.insert("app".into(), "x".into());
                EncapsulationKind {
                    bare_workload: Some(BareWorkload {
                        namespace: "ns".into(),
                        selector: sel,
                    }),
                    ..EncapsulationKind::default()
                }
            }
        }
    }

    /// Construct an `EncapsulationKind` with two slots populated — drives
    /// the pairwise `Ambiguous` sweep. Composes the single-slot
    /// constructor on top of itself to keep one source of truth for
    /// per-variant inner payloads.
    fn two_slot_kind(a: EncapsulationTarget, b: EncapsulationTarget) -> EncapsulationKind {
        let ka = single_slot_kind(a);
        let kb = single_slot_kind(b);
        EncapsulationKind {
            existing_helm_release: ka.existing_helm_release.or(kb.existing_helm_release),
            existing_kustomization: ka.existing_kustomization.or(kb.existing_kustomization),
            bare_workload: ka.bare_workload.or(kb.bare_workload),
        }
    }

    /// Structural well-formedness of [`EncapsulationTarget`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `encapsulation_target_all_is_unique_and_complete` +
    /// `encapsulation_target_roundtrip_via_as_str` + the empty-input
    /// arm of `unknown_encapsulation_target_errors`. `FromStr`
    /// delegates to `<Self as tatara_lisp::ClosedSet>::parse_label`, so
    /// this helper exercises the same code path the
    /// `EncapsulationKind::variant` resolver hits when keying on a
    /// camelCase target name back to the typed target.
    #[test]
    fn encapsulation_target_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<EncapsulationTarget>();
    }

    /// CANONICAL-KEY CONTRACT: every `EncapsulationTarget::as_str()`
    /// matches the serde `rename_all = "camelCase"` field name on the
    /// corresponding `Option<…>` slot of `EncapsulationKind`. A future
    /// rename of either the struct field OR the `as_str` arm lands here
    /// at one site, instead of drifting between the typed surface, the
    /// YAML wire format, and the `EncapsulationKindError::Empty`
    /// diagnostic. Drives the closed set via `ALL`.
    #[test]
    fn encapsulation_target_as_str_matches_field_name() {
        for t in EncapsulationTarget::ALL {
            let k = single_slot_kind(t);
            let yaml = serde_yaml::to_string(&k).expect("serialize");
            let key = t.as_str();
            assert!(
                yaml.contains(&format!("{key}:")),
                "as_str(={key:?}) for {t:?} not present in serialized YAML:\n{yaml}"
            );
        }
    }

    /// CANONICAL-NAMES PIN: byte-exact camelCase wire-format pin —
    /// renaming any of these strings IS a wire-format break that fails
    /// this test FIRST so the rename stays a deliberate decision, not a
    /// typo. Locks the (variant → operator-facing key) table.
    #[test]
    fn encapsulation_target_canonical_names_pinned() {
        assert_eq!(
            EncapsulationTarget::ExistingHelmRelease.as_str(),
            "existingHelmRelease"
        );
        assert_eq!(
            EncapsulationTarget::ExistingKustomization.as_str(),
            "existingKustomization"
        );
        assert_eq!(EncapsulationTarget::BareWorkload.as_str(), "bareWorkload");
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. If a reviewer
    /// accidentally re-introduces an inline match in Display, this test
    /// would fail the moment a variant rename touches one site but not
    /// the other.
    #[test]
    fn encapsulation_target_display_matches_as_str() {
        for t in EncapsulationTarget::ALL {
            assert_eq!(t.to_string(), t.as_str());
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical projection
    /// — PascalCased / typo / cross-axis-leaked inputs from sibling
    /// closed-set enums on the same `ProcessSpec` axis (`Manage`,
    /// `Adopt`, `Observe`, `OnAttested`, …) — and the error echoes the
    /// input verbatim so the operator-facing diagnostic carries the
    /// offending value, not a normalized form. `EncapsulationTarget`
    /// is its own axis, NOT a transparent reflection of any sibling.
    /// The empty-input arm is pinned by
    /// [`encapsulation_target_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownEncapsulationTarget`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_encapsulation_target_errors() {
        use std::str::FromStr;
        for bad in [
            "ExistingHelmRelease",
            "existing_helm_release",
            "EXISTINGHELMRELEASE",
            "helmRelease",
            "kustomization",
            "Manage",
            "Adopt",
            "Observe",
            "OnAttested",
        ] {
            let err = EncapsulationTarget::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// ROUND-TRIP CONTRACT: every target reaches its borrowed-variant
    /// view via `select`, and that variant projects back to the same
    /// target via `EncapsulationKindVariant::target`. A regression that
    /// misroutes a `select` arm (e.g.
    /// `Self::ExistingHelmRelease => kind.existing_kustomization
    /// .as_ref()...`) fails loudly here. Also pins that the resolver
    /// lands on the same target.
    #[test]
    fn encapsulation_target_round_trips_through_variant_target() {
        for t in EncapsulationTarget::ALL {
            let k = single_slot_kind(t);
            let v = t.select(&k).expect("populated slot must select");
            assert_eq!(v.target(), t, "round-trip failed for {t:?}");
            assert_eq!(
                k.variant().expect("exactly-one variant").target(),
                t,
                "variant() resolver disagreed on {t:?}"
            );
        }
    }

    /// SELECT-EMPTY CONTRACT: an unpopulated slot returns `None` from
    /// `select`, for every target. Pairs with the resolver's `Empty`
    /// path so a future target's slot defaulting wrong (e.g.
    /// accidentally `Some(Default::default())` instead of `None`) is
    /// caught here.
    #[test]
    fn encapsulation_target_select_returns_none_for_unset_slot() {
        let empty = EncapsulationKind::default();
        for t in EncapsulationTarget::ALL {
            assert!(
                t.select(&empty).is_none(),
                "{t:?} reported populated on a default EncapsulationKind"
            );
        }
    }

    /// EMPTY-DIAGNOSTIC CONTRACT: the closed-set target list embedded
    /// in `EncapsulationKindError::Empty` echoes the canonical join of
    /// every `EncapsulationTarget::as_str()` projection. A variant
    /// added without updating `ENCAPSULATION_TARGET_LIST` (or a renamed
    /// variant) shows up here as a mismatch. Mirrors
    /// `artifact_error_empty_lists_every_kind_in_canonical_order` —
    /// routes through [`tatara_lisp::ClosedSet::labels_joined`].
    #[test]
    fn encapsulation_kind_error_empty_lists_every_target_in_canonical_order() {
        assert_eq!(
            <EncapsulationTarget as tatara_lisp::ClosedSet>::labels_joined("/"),
            ENCAPSULATION_TARGET_LIST,
        );
        // And the diagnostic carries that exact list.
        let err = EncapsulationKind::default().variant().unwrap_err();
        assert_eq!(
            err,
            EncapsulationKindError::Empty(ENCAPSULATION_TARGET_LIST)
        );
    }

    /// AMBIGUOUS-PATH CONTRACT: when two slots are populated the
    /// resolver yields `Ambiguous`, exhaustively across every pair in
    /// `ALL × ALL` (excluding the diagonal). A future asymmetry where
    /// one slot would silently shadow another (e.g. an `if-let` chain
    /// re-introducing first-wins ordering) is caught here.
    #[test]
    fn encapsulation_kind_two_slots_is_ambiguous_across_every_pair() {
        for a in EncapsulationTarget::ALL {
            for b in EncapsulationTarget::ALL {
                if a == b {
                    continue;
                }
                let k = two_slot_kind(a, b);
                assert_eq!(
                    k.variant().unwrap_err(),
                    EncapsulationKindError::Ambiguous,
                    "({a:?}, {b:?}) should resolve Ambiguous"
                );
            }
        }
    }

    // Per-implementor `unknown_X_message_matches_substrate_convention`
    // tests removed — clause (5) of
    // `tatara_lisp::assert_closed_set_well_formed::<T>()` now verifies
    // the substrate-wide `"unknown {SET_LABEL}: {input}"` carrier shape
    // generically (called above on `EncapsulationTarget` /
    // `EncapsulationMode` through their `*_is_well_formed_closed_set`
    // sites). The `SET_LABEL` projection is pinned independently by
    // `tatara_lisp_derive::pascal_to_spaced_lowercase_tests` —
    // together the two contracts guarantee the operator-facing
    // diagnostic without needing per-enum literal pins.
}
