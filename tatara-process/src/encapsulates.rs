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
use std::fmt;
use std::str::FromStr;
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

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum EncapsulationKindError {
    #[error(
        "encapsulation kind has no variant set (one of \
         existingHelmRelease/existingKustomization/bareWorkload required)"
    )]
    Empty,
    #[error("encapsulation kind has multiple variants set; exactly one required")]
    Ambiguous,
}

impl EncapsulationKind {
    /// Resolve to exactly one variant.
    pub fn variant(&self) -> Result<EncapsulationKindVariant<'_>, EncapsulationKindError> {
        use crate::tagged_union::{resolve, ResolveError};
        resolve([
            self.existing_helm_release
                .as_ref()
                .map(EncapsulationKindVariant::ExistingHelmRelease),
            self.existing_kustomization
                .as_ref()
                .map(EncapsulationKindVariant::ExistingKustomization),
            self.bare_workload
                .as_ref()
                .map(EncapsulationKindVariant::BareWorkload),
        ])
        .map_err(|e| match e {
            ResolveError::None => EncapsulationKindError::Empty,
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
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "PascalCase")]
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

impl fmt::Display for EncapsulationMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EncapsulationMode {
    type Err = UnknownEncapsulationMode;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for mode in Self::ALL {
            if s == mode.as_str() {
                return Ok(mode);
            }
        }
        Err(UnknownEncapsulationMode(s.to_string()))
    }
}

/// Typed parse failure carrying the offending input verbatim so the
/// operator-facing diagnostic surfaces the bad value, not a normalized
/// form. Symmetric to [`crate::export::UnknownExportTrigger`],
/// [`crate::lifetime::UnknownTeardownPolicy`],
/// [`crate::boundary::UnknownConditionKind`], and
/// [`crate::phase::UnknownPhase`].
#[derive(Debug, thiserror::Error)]
#[error("unknown encapsulation mode: {0}")]
pub struct UnknownEncapsulationMode(pub String);

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
        assert_eq!(k.variant().unwrap_err(), EncapsulationKindError::Empty);
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

    /// `ALL` is the source of truth for the resolver / `FromStr` sweep
    /// — pin its closure so a variant added without an `ALL` entry
    /// fails here (via the uniqueness check) before drifting `as_str`
    /// / `emits_workload` / `preserves_release_name`. The arity is
    /// asserted by the `[Self; 3]` array type itself.
    #[test]
    fn mode_all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for mode in EncapsulationMode::ALL {
            assert!(seen.insert(mode), "duplicate variant in ALL: {mode:?}");
        }
        assert_eq!(seen.len(), EncapsulationMode::ALL.len());
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

    /// Every variant in ALL round-trips through `as_str` ↔ `FromStr`.
    /// Adding a variant without extending `as_str` / `FromStr`'s sweep
    /// of `ALL` fails here.
    #[test]
    fn mode_roundtrip_via_as_str() {
        use std::str::FromStr;
        for mode in EncapsulationMode::ALL {
            assert_eq!(
                EncapsulationMode::from_str(mode.as_str()).unwrap(),
                mode,
                "round-trip failed for {mode:?}"
            );
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — empty / lowercased / typo / unrelated — and the
    /// error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    #[test]
    fn unknown_encapsulation_mode_errors() {
        use std::str::FromStr;
        for bad in ["", "manage", "ADOPT", "Observed", "Wrap"] {
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
}
