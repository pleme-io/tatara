//! Closed-set of FluxCD resource kinds emitted or consumed by the
//! tatara reconciler + typed projections for their `(apiVersion, kind)`
//! pair — the substrate primitive that owns the workspace-wide (variant
//! → wire-form identity) mapping every Flux-facing site would otherwise
//! restate by hand.
//!
//! Pre-lift the `(apiVersion, kind)` pairing was hand-authored across
//! FIVE production sites in `tatara-reconciler` past the ★★
//! PRIME-DIRECTIVE ≥ 2 duplication threshold, split across two axes:
//!
//! * `Kustomization` — two production sites:
//!     * `tatara-reconciler::boundary::evaluate` — the
//!       `ConditionKind::KustomizationHealthy` arm's
//!       `evaluate_flux_ready(..., "kustomize.toolkit.fluxcd.io/v1",
//!       "Kustomization")` call.
//!     * `tatara-reconciler::render::render_flux` — the emitted
//!       `json!({"apiVersion": "kustomize.toolkit.fluxcd.io/v1",
//!       "kind": "Kustomization", ...})` block.
//! * `HelmRelease` — two production sites:
//!     * `tatara-reconciler::boundary::evaluate` — the
//!       `ConditionKind::HelmReleaseReleased` arm's
//!       `evaluate_flux_ready(..., "helm.toolkit.fluxcd.io/v2",
//!       "HelmRelease")` call.
//!     * `tatara-reconciler::render::render_aplicacao` — the emitted
//!       `json!({"apiVersion": "helm.toolkit.fluxcd.io/v2",
//!       "kind": "HelmRelease", ...})` block.
//! * `OCIRepository` — two production sites (both in
//!   `tatara-reconciler::render::render_aplicacao`):
//!     * The emitted `json!({"apiVersion":
//!       "source.toolkit.fluxcd.io/v1beta2", "kind": "OCIRepository",
//!       ...})` block for the `oci://` chart-ref branch.
//!     * The inline `chartRef.kind = "OCIRepository"` slot on the
//!       sibling HelmRelease spec.
//!
//! Every pre-lift site restated the axis-typed `&'static str` slot
//! byte-identically; a regression that swapped ONE arm's `apiVersion`
//! from `v1` to `v1beta1` (a Flux API-version bump that reaches only
//! one of the two sites) or misspelled `Kustomization` at ONE site
//! would silently mis-route the reconciler's SSA-fetch against the
//! K8s API server (fetch under one apiVersion + apply under another
//! is a 404 at wire time). Post-lift every axis binds at ONE closed-
//! set owner and rustc enforces every consumer reads the pair from
//! the SAME variant.
//!
//! Sibling to the same-shape `IntentKind` closed set on the
//! `Intent` tagged-union axis (in `crate::intent`): both project a
//! closed set of variant → wire-form identity slots through named
//! per-variant methods; `IntentKind::as_str` returns the camelCase
//! wire key of a serde variant, while `FluxResource::api_version` +
//! `FluxResource::kind` return the K8s `apiVersion` + `kind` wire
//! strings of a Flux resource variant. Peer to the sibling
//! substrate primitives `owner_reference_json` / `api_version()` /
//! `PROCESS_KIND` in `crate::lib` on the same "K8s wire-form
//! identity" axis but for the tatara `Process` CRD itself, rather
//! than for the Flux resources tatara's reconciler emits or
//! consumes.
//!
//! Extension: a fourth Flux resource kind (a `Bucket` for
//! S3-backed sources, a `Receiver` for webhook-triggered
//! reconciliation, an `Alert` / `Provider` for notification-
//! controller integration, an `ImagePolicy` / `ImageRepository`
//! for image-driven reconciliation, a hypothetical Flux
//! `GitRepository` variant we don't yet reach for at the wire) lands
//! as ONE variant + ONE `api_version` arm + ONE `kind` arm + ONE
//! `ALL` entry, all four exhaustively enforced by rustc's match
//! coverage. Every downstream Flux-facing consumer inherits the
//! extension mechanically through the same `FluxResource::X.api_version()`
//! / `.kind()` dispatch pair.
//!
//! Theory grounding: THEORY.md §VI.1 (generation over composition —
//! the (apiVersion, kind) pairing recurred at five hand-authored sites
//! past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to
//! ONE closed-set owner per axis here). THEORY.md §II.1 invariant 5
//! (composition preserves proofs — the two per-axis mappings live at
//! ONE typed algebra projection apiece; a regression that drifted the
//! apiVersion at ONE consumer would fail-loudly at this module's byte-
//! shape pins rather than as silent SSA-fetch-vs-emit skew at the
//! reconciler wire).

/// Closed set of FluxCD resource kinds this repo's reconciler emits or
/// consumes at the wire. The three variants partition the current
/// Flux-facing surface:
///
/// * `Kustomization` — `kustomize-controller` reconciles a set of
///   K8s manifests from a source (`GitRepository` / `OCIRepository`).
///   Emitted by [`crate::intent::FluxIntent`]; consumed by
///   `ConditionKind::KustomizationHealthy`.
/// * `HelmRelease` — `helm-controller` installs / upgrades a Helm
///   chart from a source. Emitted by [`crate::intent::AplicacaoIntent`];
///   consumed by `ConditionKind::HelmReleaseReleased`.
/// * `OCIRepository` — `source-controller` pulls a chart from an OCI
///   registry. Emitted by [`crate::intent::AplicacaoIntent`] for
///   `oci://` chart refs, referenced by the sibling HelmRelease's
///   `chartRef` slot.
///
/// Every variant carries a stable `(api_version, kind)` pair through
/// its typed projections; both projections are `const fn` so the
/// pair reduces to a `&'static str` slot at every callsite with no
/// runtime overhead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FluxResource {
    Kustomization,
    HelmRelease,
    OCIRepository,
}

impl FluxResource {
    /// The closed set of Flux resource variants this substrate binds.
    /// Enumerable so a sweep-test (or a future coherence check) can
    /// walk every variant without a hand-maintained list on the caller
    /// side.
    pub const ALL: [Self; 3] = [Self::Kustomization, Self::HelmRelease, Self::OCIRepository];

    /// K8s wire-form `apiVersion` string for this Flux resource
    /// variant. Matches the canonical group+version pair the Flux
    /// controllers expose today:
    ///
    /// * `Kustomization` → `kustomize.toolkit.fluxcd.io/v1`
    /// * `HelmRelease`  → `helm.toolkit.fluxcd.io/v2`
    /// * `OCIRepository` → `source.toolkit.fluxcd.io/v1beta2`
    ///
    /// A future Flux upgrade that bumps any group's version lands at
    /// ONE arm here; every downstream emit or fetch site inherits the
    /// upgrade mechanically through the same `.api_version()` call.
    pub const fn api_version(self) -> &'static str {
        match self {
            Self::Kustomization => "kustomize.toolkit.fluxcd.io/v1",
            Self::HelmRelease => "helm.toolkit.fluxcd.io/v2",
            Self::OCIRepository => "source.toolkit.fluxcd.io/v1beta2",
        }
    }

    /// K8s wire-form `kind` string for this Flux resource variant —
    /// the PascalCase identifier the K8s API server matches against
    /// the resource's `kind:` slot.
    ///
    /// A regression that misspelled any of the three PascalCase
    /// identifiers would silently mis-route SSA-fetch against SSA-
    /// apply (a `HelmRelese` emit against a `HelmRelease` fetch is
    /// a 404 at wire time); post-lift a rename lands at ONE arm
    /// here rather than at every emit or fetch site.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Kustomization => "Kustomization",
            Self::HelmRelease => "HelmRelease",
            Self::OCIRepository => "OCIRepository",
        }
    }

    /// Project this variant onto the typed
    /// [`crate::k8s_wire_identity::K8sWireIdentity`] pair — the
    /// substrate primitive that owns the workspace-wide
    /// `(apiVersion, kind)` pairing every emit or fetch site
    /// composes against. Pre-lift the reconciler's three
    /// `render_flux` / `render_aplicacao` emit sites hand-authored
    /// the pair as two adjacent `json!` slots (`"apiVersion":
    /// FluxResource::X.api_version(), "kind": FluxResource::X.kind()`)
    /// mentioning the same variant twice; post-lift the emit site
    /// names the variant ONCE via
    /// `.wire_identity().resource_json(json!({...}))` and the pair
    /// binds structurally at the closed-set owner.
    ///
    /// `const fn` so a caller can bind the pair into a `const` slot
    /// — a future coherence sweep or a compile-time cache reads the
    /// same pair with zero runtime overhead.
    pub const fn wire_identity(self) -> crate::k8s_wire_identity::K8sWireIdentity {
        crate::k8s_wire_identity::K8sWireIdentity::new(self.api_version(), self.kind())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_enumerates_every_variant_exactly_once() {
        // A future 4th variant added without an `ALL` entry (or a
        // duplicate entry that skewed the sweep) surfaces here.
        assert_eq!(FluxResource::ALL.len(), 3);
        // Every variant appears exactly once — no duplicates, no gaps.
        let mut seen = std::collections::HashSet::new();
        for v in FluxResource::ALL {
            assert!(seen.insert(v), "duplicate variant in ALL: {v:?}");
        }
    }

    #[test]
    fn kustomization_api_version_is_kustomize_toolkit_v1() {
        // Byte-identity pin: the exact wire-form string every pre-lift
        // callsite hand-authored (both `boundary::evaluate` and
        // `render::render_flux`). A drift that renamed the group or
        // bumped the version at ONE site would silently mis-route
        // the fetch-vs-apply pairing.
        assert_eq!(
            FluxResource::Kustomization.api_version(),
            "kustomize.toolkit.fluxcd.io/v1"
        );
    }

    #[test]
    fn helm_release_api_version_is_helm_toolkit_v2() {
        assert_eq!(
            FluxResource::HelmRelease.api_version(),
            "helm.toolkit.fluxcd.io/v2"
        );
    }

    #[test]
    fn oci_repository_api_version_is_source_toolkit_v1beta2() {
        assert_eq!(
            FluxResource::OCIRepository.api_version(),
            "source.toolkit.fluxcd.io/v1beta2"
        );
    }

    #[test]
    fn kustomization_kind_is_pascalcase_kustomization() {
        assert_eq!(FluxResource::Kustomization.kind(), "Kustomization");
    }

    #[test]
    fn helm_release_kind_is_pascalcase_helm_release() {
        assert_eq!(FluxResource::HelmRelease.kind(), "HelmRelease");
    }

    #[test]
    fn oci_repository_kind_is_pascalcase_oci_repository() {
        assert_eq!(FluxResource::OCIRepository.kind(), "OCIRepository");
    }

    #[test]
    fn every_variants_api_version_and_kind_are_distinct_across_the_closed_set() {
        // Cross-variant coherence pin: no two variants may share an
        // `api_version` or a `kind`. A future extension that added a
        // variant duplicating an existing wire-form pair (e.g. a
        // `HelmChart` variant that reused the HelmRelease apiVersion
        // in a copy-paste) would surface here.
        let mut api_versions = std::collections::HashSet::new();
        let mut kinds = std::collections::HashSet::new();
        for v in FluxResource::ALL {
            assert!(
                api_versions.insert(v.api_version()),
                "duplicate api_version at {v:?}: {}",
                v.api_version()
            );
            assert!(
                kinds.insert(v.kind()),
                "duplicate kind at {v:?}: {}",
                v.kind()
            );
        }
    }

    #[test]
    fn api_version_and_kind_are_const_fn_reachable() {
        // Compile-time reachability pin: every variant's projections
        // are `const fn`, so a caller can bind them into a `const`
        // slot. A regression that dropped the `const` qualifier
        // would fail-loudly here rather than as a wrong-slot runtime
        // dispatch at every callsite.
        const K_AV: &str = FluxResource::Kustomization.api_version();
        const K_K: &str = FluxResource::Kustomization.kind();
        const H_AV: &str = FluxResource::HelmRelease.api_version();
        const H_K: &str = FluxResource::HelmRelease.kind();
        const O_AV: &str = FluxResource::OCIRepository.api_version();
        const O_K: &str = FluxResource::OCIRepository.kind();
        assert_eq!(K_AV, "kustomize.toolkit.fluxcd.io/v1");
        assert_eq!(K_K, "Kustomization");
        assert_eq!(H_AV, "helm.toolkit.fluxcd.io/v2");
        assert_eq!(H_K, "HelmRelease");
        assert_eq!(O_AV, "source.toolkit.fluxcd.io/v1beta2");
        assert_eq!(O_K, "OCIRepository");
    }

    #[test]
    fn wire_identity_pairs_api_version_and_kind_per_variant() {
        // Projection pin: every variant's `wire_identity()` binds the
        // SAME closed-set variant's `api_version` and `kind` into a
        // typed pair. A regression that projected the pair from two
        // different variants (a copy-paste that referenced
        // `Kustomization.api_version()` alongside `HelmRelease.kind()`
        // at the projection body) would surface here rather than as
        // a silent wire-form skew at every reconciler emit site.
        for v in FluxResource::ALL {
            let id = v.wire_identity();
            assert_eq!(id.api_version, v.api_version());
            assert_eq!(id.kind, v.kind());
        }
    }

    #[test]
    fn wire_identity_is_const_reachable() {
        // Compile-time reachability pin: `wire_identity` is `const
        // fn` so a caller can bind the pair into a `const` slot at
        // compile time. A regression that dropped the `const`
        // qualifier would fail-loudly here rather than as a runtime
        // dispatch surfacing at every projection site.
        const K: crate::k8s_wire_identity::K8sWireIdentity =
            FluxResource::Kustomization.wire_identity();
        const H: crate::k8s_wire_identity::K8sWireIdentity =
            FluxResource::HelmRelease.wire_identity();
        const O: crate::k8s_wire_identity::K8sWireIdentity =
            FluxResource::OCIRepository.wire_identity();
        assert_eq!(K.api_version, "kustomize.toolkit.fluxcd.io/v1");
        assert_eq!(K.kind, "Kustomization");
        assert_eq!(H.api_version, "helm.toolkit.fluxcd.io/v2");
        assert_eq!(H.kind, "HelmRelease");
        assert_eq!(O.api_version, "source.toolkit.fluxcd.io/v1beta2");
        assert_eq!(O.kind, "OCIRepository");
    }

    #[test]
    fn projections_are_pure_functions_of_the_variant() {
        // Purity pin: calling the projections repeatedly on the same
        // variant returns byte-identical `&'static str`s. Guards against
        // an implementation that lazily materialized an interned key
        // per-call and hashed against runtime state.
        for v in FluxResource::ALL {
            assert_eq!(v.api_version(), v.api_version());
            assert_eq!(v.kind(), v.kind());
        }
    }
}
