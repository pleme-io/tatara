//! Closed-set of K8s **built-in** resource kinds `tatara-reconciler`
//! emits or fetches at the wire + typed projections for their
//! `(apiVersion, kind)` pair — the substrate primitive that owns the
//! workspace-wide (variant → wire-form identity) mapping every
//! K8s-builtin-facing site in the reconciler would otherwise restate
//! by hand.
//!
//! Pre-lift the `(apiVersion, kind)` pairing was hand-authored across
//! THREE production sites in `tatara-reconciler` past the ★★
//! PRIME-DIRECTIVE ≥ 2 duplication threshold, split across two axes:
//!
//! * `Job` (`batch/v1`) — two production sites:
//!     * `tatara-reconciler::boundary::fetch_job_status` — the
//!       `ssapply::fetch(client, ns, "batch/v1", "Job", name)` call
//!       every `evaluate_job_attested` / `evaluate_closed_loop_auth`
//!       postcondition evaluator on the Job-based axes lands at.
//!     * `tatara-reconciler::render::one_export_job` — the emitted
//!       `json!({"apiVersion": "batch/v1", "kind": "Job", ...})`
//!       block seeding every `render_export_jobs` output.
//! * `ConfigMap` (`v1`) — one production site (single site is still
//!   worth binding through the same typed closed set so a future
//!   second consumer inherits the wire-form identity mechanically):
//!     * `tatara-reconciler::boundary::verify_receipt_cm` — the
//!       `ssapply::fetch(client, ns, "v1", "ConfigMap", name)` call
//!       fetching every Job's receipt ConfigMap.
//!
//! At every pre-lift site the wire-form pair was hand-authored as two
//! adjacent `&'static str` literals; a regression that misspelled
//! either slot (a `batch/v1beta1` legacy version at the fetch site
//! against a `batch/v1` emit site, a `Configmap` typo at the fetch
//! side of the receipt loop) would silently 404 at wire time and
//! diagnose as a broken CRD rather than as literal drift at the emit
//! site. Post-lift each site reaches the pair through the ONE typed
//! closed-set owner here, and rustc enforces every consumer reads the
//! pair from the SAME variant.
//!
//! Sibling to the same-axis substrate primitives on the K8s wire-form
//! identity axis:
//!
//! * [`crate::flux_resource::FluxResource`] owns the Flux-controller
//!   axis (`Kustomization` / `HelmRelease` / `OCIRepository`) —
//!   emitted or fetched by the reconciler against the FluxCD
//!   controllers.
//! * [`crate::routing_edge_resource::RoutingEdgeResource`] owns the
//!   routing-edge axis (`Ingress` / `DNSEndpoint`) — emitted by the
//!   reconciler for hostname exposure.
//! * [`crate::PROCESS_KIND`] + [`crate::api_version`] own the tatara
//!   `Process` CRD's own wire-form identity.
//! * [`Self`] (this closed set) owns the K8s **built-in** axis
//!   (`Job` / `ConfigMap`) — the resources the reconciler consumes
//!   from Kubernetes itself (not from Flux or from tatara).
//!
//! Together the four closed sets partition every K8s wire-form
//! identity `tatara-reconciler` reaches at run time, and every
//! consumer routes through ONE typed owner per axis.
//!
//! Extension: a fourth K8s built-in kind the reconciler grows a
//! consumer for (a `batch/v1::CronJob` for scheduled receipts, a
//! `v1::Secret` for a credential-fetching probe, a `v1::Namespace`
//! for a fresh-namespace precondition on ephemeral envs, a
//! `v1::ServiceAccount` for RBAC-precondition checks, an
//! `apps/v1::Deployment` for a live-workload precondition) lands as
//! ONE variant + ONE `api_version` arm + ONE `kind` arm + ONE `ALL`
//! entry, all four exhaustively enforced by rustc's match coverage.
//! Every downstream consumer inherits the extension mechanically
//! through the same `K8sBuiltinResource::X.api_version()` / `.kind()`
//! / `.wire_identity()` dispatch chain.
//!
//! Theory grounding: THEORY.md §VI.1 (generation over composition —
//! the (apiVersion, kind) pairing recurred at three hand-authored
//! sites past the PRIME-DIRECTIVE ≥ 2 duplication trigger — including
//! two Job-axis sites split across the fetch/emit boundary — and is
//! lifted to ONE closed-set owner per axis here). THEORY.md §II.1
//! invariant 5 (composition preserves proofs — the per-axis mappings
//! live at ONE typed algebra projection apiece; a regression that
//! drifted the apiVersion at ONE consumer would fail-loudly at this
//! module's byte-shape pins rather than as silent
//! SSA-fetch-vs-emit skew at the reconciler wire).

/// Closed set of Kubernetes **built-in** resource kinds this repo's
/// reconciler consumes at the wire. Peer to
/// [`crate::flux_resource::FluxResource`] (Flux-controller axis) and
/// [`crate::routing_edge_resource::RoutingEdgeResource`] (routing-edge
/// axis); together the three closed sets partition the reconciler's
/// K8s wire-form surface — this variant owns the resources the
/// reconciler reaches for from Kubernetes itself rather than from Flux
/// or from tatara's own CRDs.
///
/// * `Job` — `batch/v1::Job`, emitted by `render::one_export_job`
///   for every ExportSpec that fires on a terminal-reached gate,
///   fetched by `boundary::fetch_job_status` inside every
///   `evaluate_job_attested` / `evaluate_closed_loop_auth`
///   postcondition evaluator.
/// * `ConfigMap` — `v1::ConfigMap`, fetched by
///   `boundary::verify_receipt_cm` for every Job's receipt after the
///   Job succeeds (the receipt payload is a
///   [`crate::receipt::ReceiptEnvelope`] round-tripped through
///   `data['receipt.json' | 'receipt.yaml']`).
///
/// Every variant carries a stable `(api_version, kind)` pair through
/// its typed projections; both projections are `const fn` so the
/// pair reduces to a `&'static str` slot at every callsite with no
/// runtime overhead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum K8sBuiltinResource {
    Job,
    ConfigMap,
}

impl K8sBuiltinResource {
    /// The closed set of K8s-builtin resource variants this substrate
    /// binds. Enumerable so a sweep-test (or a future coherence check)
    /// can walk every variant without a hand-maintained list on the
    /// caller side.
    pub const ALL: [Self; 2] = [Self::Job, Self::ConfigMap];

    /// K8s wire-form `apiVersion` string for this built-in resource
    /// variant.
    ///
    /// * `Job`       → `batch/v1`
    /// * `ConfigMap` → `v1` (core/v1 has no group prefix)
    ///
    /// A future Kubernetes API-version bump that promoted any
    /// group's version lands at ONE arm here; every downstream emit
    /// or fetch site inherits the upgrade mechanically through the
    /// same `.api_version()` call.
    pub const fn api_version(self) -> &'static str {
        match self {
            Self::Job => "batch/v1",
            Self::ConfigMap => "v1",
        }
    }

    /// K8s wire-form `kind` string for this built-in resource
    /// variant — the PascalCase identifier the K8s API server
    /// matches against the resource's `kind:` slot.
    ///
    /// A regression that misspelled either PascalCase identifier
    /// (a `Configmap` typo, a lowercased `job`) would silently
    /// mis-route SSA-fetch against SSA-apply (a 404 at wire time);
    /// post-lift a rename lands at ONE arm here rather than at
    /// every emit or fetch site.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Job => "Job",
            Self::ConfigMap => "ConfigMap",
        }
    }

    /// Project this variant onto the typed
    /// [`crate::k8s_wire_identity::K8sWireIdentity`] pair — the
    /// substrate primitive that owns the workspace-wide
    /// `(apiVersion, kind)` pairing every emit or fetch site
    /// composes against. Pre-lift the reconciler's
    /// `render::one_export_job` emit site hand-authored the pair as
    /// two adjacent `json!` slots (`"apiVersion": "batch/v1", "kind":
    /// "Job"`) with the axis-typed literals inlined; post-lift the
    /// emit site names the variant ONCE via
    /// `.wire_identity().resource_json(json!({...}))` and the pair
    /// binds structurally at the closed-set owner.
    ///
    /// `const fn` so a caller can bind the pair into a `const` slot
    /// — a future coherence sweep or a compile-time cache reads the
    /// same pair with zero runtime overhead. Peer to the sibling
    /// `wire_identity` projections on
    /// [`crate::flux_resource::FluxResource`] and
    /// [`crate::routing_edge_resource::RoutingEdgeResource`].
    pub const fn wire_identity(self) -> crate::k8s_wire_identity::K8sWireIdentity {
        crate::k8s_wire_identity::K8sWireIdentity::new(self.api_version(), self.kind())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_enumerates_every_variant_exactly_once() {
        // A future 3rd variant added without an `ALL` entry (or a
        // duplicate entry that skewed the sweep) surfaces here.
        assert_eq!(K8sBuiltinResource::ALL.len(), 2);
        let mut seen = std::collections::HashSet::new();
        for v in K8sBuiltinResource::ALL {
            assert!(seen.insert(v), "duplicate variant in ALL: {v:?}");
        }
    }

    #[test]
    fn job_api_version_is_batch_v1() {
        // Byte-identity pin: the exact wire-form string every pre-lift
        // callsite hand-authored (both `boundary::fetch_job_status`
        // and `render::one_export_job`). A drift that renamed the
        // group or bumped the version at ONE site would silently
        // mis-route SSA-fetch against SSA-apply.
        assert_eq!(K8sBuiltinResource::Job.api_version(), "batch/v1");
    }

    #[test]
    fn config_map_api_version_is_bare_v1() {
        // Byte-identity pin: core/v1 has no group prefix, so the
        // apiVersion is the bare `v1` string. A regression that
        // introduced a stray `core/v1` prefix at ONE site would
        // fail-loudly here rather than as a 404 at wire time.
        assert_eq!(K8sBuiltinResource::ConfigMap.api_version(), "v1");
    }

    #[test]
    fn job_kind_is_pascalcase_job() {
        assert_eq!(K8sBuiltinResource::Job.kind(), "Job");
    }

    #[test]
    fn config_map_kind_is_pascalcase_config_map() {
        assert_eq!(K8sBuiltinResource::ConfigMap.kind(), "ConfigMap");
    }

    #[test]
    fn every_variants_api_version_and_kind_are_distinct_across_the_closed_set() {
        // Cross-variant coherence pin: no two variants may share an
        // `api_version` or a `kind`. A future extension that added a
        // variant duplicating an existing wire-form pair (e.g. a
        // `CronJob` variant that copy-pasted the `Job` kind) would
        // surface here rather than as silent operator-visible
        // cross-axis ambiguity at every dispatch.
        let mut api_versions = std::collections::HashSet::new();
        let mut kinds = std::collections::HashSet::new();
        for v in K8sBuiltinResource::ALL {
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
        const J_AV: &str = K8sBuiltinResource::Job.api_version();
        const J_K: &str = K8sBuiltinResource::Job.kind();
        const C_AV: &str = K8sBuiltinResource::ConfigMap.api_version();
        const C_K: &str = K8sBuiltinResource::ConfigMap.kind();
        assert_eq!(J_AV, "batch/v1");
        assert_eq!(J_K, "Job");
        assert_eq!(C_AV, "v1");
        assert_eq!(C_K, "ConfigMap");
    }

    #[test]
    fn wire_identity_pairs_api_version_and_kind_per_variant() {
        // Projection pin: every variant's `wire_identity()` binds the
        // SAME closed-set variant's `api_version` and `kind` into a
        // typed pair. A regression that projected the pair from two
        // different variants (a copy-paste that referenced
        // `Job.api_version()` alongside `ConfigMap.kind()` at the
        // projection body) would surface here rather than as a
        // silent wire-form skew at every reconciler emit or fetch
        // site.
        for v in K8sBuiltinResource::ALL {
            let id = v.wire_identity();
            assert_eq!(id.api_version, v.api_version());
            assert_eq!(id.kind, v.kind());
        }
    }

    #[test]
    fn wire_identity_is_const_reachable() {
        // Compile-time reachability pin: `wire_identity` is `const
        // fn` so a caller can bind the pair into a `const` slot at
        // compile time.
        const J: crate::k8s_wire_identity::K8sWireIdentity =
            K8sBuiltinResource::Job.wire_identity();
        const C: crate::k8s_wire_identity::K8sWireIdentity =
            K8sBuiltinResource::ConfigMap.wire_identity();
        assert_eq!(J.api_version, "batch/v1");
        assert_eq!(J.kind, "Job");
        assert_eq!(C.api_version, "v1");
        assert_eq!(C.kind, "ConfigMap");
    }

    #[test]
    fn projections_are_pure_functions_of_the_variant() {
        // Purity pin: calling the projections repeatedly on the same
        // variant returns byte-identical `&'static str`s. Guards
        // against an implementation that lazily materialized an
        // interned key per-call and hashed against runtime state.
        for v in K8sBuiltinResource::ALL {
            assert_eq!(v.api_version(), v.api_version());
            assert_eq!(v.kind(), v.kind());
        }
    }

    #[test]
    fn peers_do_not_share_wire_form_pairs_with_flux_resource_or_routing_edge_resource() {
        // Cross-substrate coherence pin: the K8s-builtin axis must
        // not overlap with the Flux-controller axis or the
        // routing-edge axis at the wire — a hypothetical variant on
        // this closed set that copy-pasted a FluxResource /
        // RoutingEdgeResource `(apiVersion, kind)` pair would silently
        // let one site's dispatch reach through the wrong closed
        // set. This pin sweeps both peers' `ALL` iterators and
        // asserts every K8s-builtin variant's pair is disjoint from
        // both.
        use crate::flux_resource::FluxResource;
        use crate::routing_edge_resource::RoutingEdgeResource;
        for k in K8sBuiltinResource::ALL {
            for f in FluxResource::ALL {
                assert_ne!(
                    (k.api_version(), k.kind()),
                    (f.api_version(), f.kind()),
                    "K8sBuiltinResource {k:?} must not share a wire-form pair with FluxResource {f:?}"
                );
            }
            for r in RoutingEdgeResource::ALL {
                assert_ne!(
                    (k.api_version(), k.kind()),
                    (r.api_version(), r.kind()),
                    "K8sBuiltinResource {k:?} must not share a wire-form pair with RoutingEdgeResource {r:?}"
                );
            }
        }
    }
}
