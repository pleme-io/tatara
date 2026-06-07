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
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
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
    /// True iff the reconciler should emit (or re-emit) the
    /// underlying HR/Kustomization at render time.
    /// Observe ⇒ false; Manage/Adopt ⇒ true.
    pub fn emits_workload(self) -> bool {
        matches!(self, Self::Manage | Self::Adopt)
    }

    /// True iff the reconciler should preserve the existing release
    /// name (so helm-controller adopts in-place). Only Adopt.
    pub fn preserves_release_name(self) -> bool {
        matches!(self, Self::Adopt)
    }
}

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
