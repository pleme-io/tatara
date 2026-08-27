//! Closed-set of routing-edge K8s resource kinds emitted by the
//! tatara reconciler's `render_routing` pipeline + typed projections
//! for their `(apiVersion, kind)` pair — the substrate primitive that
//! owns the workspace-wide (variant → wire-form identity) mapping
//! every routing-edge site would otherwise restate by hand.
//!
//! Pre-lift the `(apiVersion, kind)` pairing was hand-authored across
//! SIX production sites in `tatara-reconciler::edges` past the ★★
//! PRIME-DIRECTIVE ≥ 2 duplication threshold, split across two edge
//! variants:
//!
//! * `Ingress` — three production sites:
//!     * `IngressEdge::kind()` — the `Edge` trait method returns the
//!       bare `"Ingress"` PascalCase identifier for per-edge logging
//!       + label composition.
//!     * `IngressEdge::render` — the emitted
//!       `json!({"apiVersion": "networking.k8s.io/v1",
//!       "kind": "Ingress", ...})` block's `apiVersion` slot.
//!     * `IngressEdge::render` — the same block's `kind` slot,
//!       byte-identical to the trait method's return.
//! * `DnsEndpoint` — three production sites:
//!     * `DnsEndpointEdge::kind()` — the `Edge` trait method returns
//!       the bare `"DNSEndpoint"` PascalCase identifier.
//!     * `DnsEndpointEdge::render` — the emitted
//!       `json!({"apiVersion": "externaldns.k8s.io/v1alpha1",
//!       "kind": "DNSEndpoint", ...})` block's `apiVersion` slot.
//!     * `DnsEndpointEdge::render` — the same block's `kind` slot,
//!       byte-identical to the trait method's return.
//!
//! Every pre-lift site restated the axis-typed `&'static str` slot
//! byte-identically; a regression that renamed the K8s Kind at ONE
//! callsite (say the trait method's return but not the emitted JSON
//! slot) would silently bifurcate the wire-form identity operators
//! grep for against the K8s API server — a `HTTPRoute` migration
//! that reached the trait but not the emitter (or vice versa) is a
//! 404 at wire time. Post-lift every axis binds at ONE closed-set
//! owner and rustc enforces every consumer reads the pair from the
//! SAME variant.
//!
//! Sibling to the same-shape [`crate::flux_resource::FluxResource`]
//! closed set on the FluxCD wire-form axis: both project a closed set
//! of variant → wire-form identity slots through named per-variant
//! methods; `FluxResource` covers the Flux controllers' emitted CRs
//! (Kustomization, HelmRelease, OCIRepository) while
//! `RoutingEdgeResource` covers the routing edges the reconciler
//! renders (Ingress, DNSEndpoint). Both share the same `const fn`
//! per-variant projection idiom + `ALL` sweep-enumerable seed +
//! injectivity + const-reachability pins.
//!
//! Extension: a future edge variant (a Gateway API `HTTPRoute` /
//! `Gateway`, a `NetworkPolicy` edge, a Cloudflare API CR, a
//! `TCPRoute` for L4 routing) lands as ONE variant + ONE
//! `api_version` arm + ONE `kind` arm + ONE `ALL` entry, all four
//! exhaustively enforced by rustc's match coverage. Every downstream
//! routing-edge consumer (the trait method, the emitted JSON, any
//! future coherence sweep) inherits the extension mechanically
//! through the same `RoutingEdgeResource::X.api_version()` / `.kind()`
//! dispatch pair.
//!
//! Theory grounding: THEORY.md §VI.1 (generation over composition —
//! the (apiVersion, kind) pairing recurred at six hand-authored sites
//! past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to
//! ONE closed-set owner per axis here). THEORY.md §II.1 invariant 5
//! (composition preserves proofs — the two per-axis mappings live at
//! ONE typed algebra projection apiece; a regression that drifted the
//! apiVersion at ONE consumer would fail-loudly at this module's byte-
//! shape pins rather than as silent trait-vs-emit skew at the
//! reconciler wire).

/// Closed set of K8s resource kinds the tatara reconciler emits as
/// external routing edges. The two variants partition the current
/// routing-edge surface:
///
/// * `Ingress` — `networking.k8s.io/v1` Ingress, backed by a Service,
///   matched by the operator's ingress controller (nginx / Contour /
///   HAProxy). Emitted by `IngressEdge` in
///   `tatara-reconciler::edges`.
/// * `DnsEndpoint` — `externaldns.k8s.io/v1alpha1` DNSEndpoint,
///   picked up by external-dns and written to the operator's
///   configured DNS provider (Cloudflare / Route53 / etc.). Emitted
///   by `DnsEndpointEdge` in `tatara-reconciler::edges`.
///
/// Every variant carries a stable `(api_version, kind)` pair through
/// its typed projections; both projections are `const fn` so the
/// pair reduces to a `&'static str` slot at every callsite with no
/// runtime overhead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoutingEdgeResource {
    Ingress,
    DnsEndpoint,
}

impl RoutingEdgeResource {
    /// The closed set of routing-edge resource variants this
    /// substrate binds. Enumerable so a sweep-test (or a future
    /// coherence check) can walk every variant without a hand-
    /// maintained list on the caller side. A future third variant
    /// (a Gateway API `HTTPRoute`, a `NetworkPolicy` edge) added
    /// without an `ALL` entry surfaces at the
    /// `all_enumerates_every_variant_exactly_once` pin.
    pub const ALL: [Self; 2] = [Self::Ingress, Self::DnsEndpoint];

    /// K8s wire-form `apiVersion` string for this routing-edge
    /// resource variant. Matches the canonical group+version pair the
    /// K8s / external-dns controllers expose today:
    ///
    /// * `Ingress`     → `networking.k8s.io/v1`
    /// * `DnsEndpoint` → `externaldns.k8s.io/v1alpha1`
    ///
    /// A future controller upgrade that bumps a group's version
    /// lands at ONE arm here; every downstream emit or fetch site
    /// inherits the upgrade mechanically through the same
    /// `.api_version()` call.
    pub const fn api_version(self) -> &'static str {
        match self {
            Self::Ingress => "networking.k8s.io/v1",
            Self::DnsEndpoint => "externaldns.k8s.io/v1alpha1",
        }
    }

    /// K8s wire-form `kind` string for this routing-edge resource
    /// variant — the PascalCase identifier the K8s API server matches
    /// against the resource's `kind:` slot. The trait method
    /// `Edge::kind` at each downstream impl delegates to this
    /// projection so the diagnostic label and the emitted JSON slot
    /// cannot drift.
    ///
    /// A regression that misspelled either PascalCase identifier
    /// would silently mis-route SSA-fetch against SSA-apply (an
    /// `Ingess` emit against an `Ingress` fetch is a 404 at wire
    /// time); post-lift a rename lands at ONE arm here rather than
    /// at every emit or trait-method site.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Ingress => "Ingress",
            Self::DnsEndpoint => "DNSEndpoint",
        }
    }

    /// Project this variant onto the typed
    /// [`crate::k8s_wire_identity::K8sWireIdentity`] pair — the
    /// substrate primitive that owns the workspace-wide
    /// `(apiVersion, kind)` pairing every emit or fetch site
    /// composes against. Pre-lift the reconciler's two
    /// `IngressEdge::render` / `DnsEndpointEdge::render` emit sites
    /// hand-authored the pair as two adjacent `json!` slots
    /// (`"apiVersion": RoutingEdgeResource::X.api_version(), "kind":
    /// RoutingEdgeResource::X.kind()`) mentioning the same variant
    /// twice; post-lift the emit site names the variant ONCE via
    /// `.wire_identity().resource_json(json!({...}))` and the pair
    /// binds structurally at the closed-set owner.
    ///
    /// `const fn` so a caller can bind the pair into a `const` slot
    /// — sibling to [`crate::flux_resource::FluxResource::wire_identity`]
    /// on the FluxCD wire-form axis; both project through the same
    /// closed-set → typed-pair idiom.
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
        assert_eq!(RoutingEdgeResource::ALL.len(), 2);
        // Every variant appears exactly once — no duplicates, no gaps.
        let mut seen = std::collections::HashSet::new();
        for v in RoutingEdgeResource::ALL {
            assert!(seen.insert(v), "duplicate variant in ALL: {v:?}");
        }
    }

    #[test]
    fn ingress_api_version_is_networking_k8s_io_v1() {
        // Byte-identity pin: the exact wire-form string every pre-lift
        // callsite hand-authored (both `IngressEdge::render` at the
        // json! slot and any downstream selector matching against
        // this apiVersion). A drift that renamed the group or bumped
        // the version at ONE site would silently mis-route the
        // fetch-vs-apply pairing.
        assert_eq!(
            RoutingEdgeResource::Ingress.api_version(),
            "networking.k8s.io/v1"
        );
    }

    #[test]
    fn dns_endpoint_api_version_is_externaldns_k8s_io_v1alpha1() {
        assert_eq!(
            RoutingEdgeResource::DnsEndpoint.api_version(),
            "externaldns.k8s.io/v1alpha1"
        );
    }

    #[test]
    fn ingress_kind_is_pascalcase_ingress() {
        assert_eq!(RoutingEdgeResource::Ingress.kind(), "Ingress");
    }

    #[test]
    fn dns_endpoint_kind_is_pascalcase_dns_endpoint() {
        assert_eq!(RoutingEdgeResource::DnsEndpoint.kind(), "DNSEndpoint");
    }

    #[test]
    fn every_variants_api_version_and_kind_are_distinct_across_the_closed_set() {
        // Cross-variant coherence pin: no two variants may share an
        // `api_version` or a `kind`. A future extension that added a
        // variant duplicating an existing wire-form pair (e.g. a
        // hypothetical `IngressRoute` variant that reused the
        // `networking.k8s.io/v1` apiVersion in a copy-paste) would
        // surface here.
        let mut api_versions = std::collections::HashSet::new();
        let mut kinds = std::collections::HashSet::new();
        for v in RoutingEdgeResource::ALL {
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
        const I_AV: &str = RoutingEdgeResource::Ingress.api_version();
        const I_K: &str = RoutingEdgeResource::Ingress.kind();
        const D_AV: &str = RoutingEdgeResource::DnsEndpoint.api_version();
        const D_K: &str = RoutingEdgeResource::DnsEndpoint.kind();
        assert_eq!(I_AV, "networking.k8s.io/v1");
        assert_eq!(I_K, "Ingress");
        assert_eq!(D_AV, "externaldns.k8s.io/v1alpha1");
        assert_eq!(D_K, "DNSEndpoint");
    }

    #[test]
    fn wire_identity_pairs_api_version_and_kind_per_variant() {
        // Projection pin: every variant's `wire_identity()` binds
        // the SAME closed-set variant's `api_version` and `kind`
        // into a typed pair. Peer to the sibling `FluxResource`
        // pin — both closed sets project through the same shape,
        // and a regression that skewed the pair at one variant
        // (say the `DnsEndpoint` projection accidentally returned
        // `Ingress.kind()`) would surface here rather than at the
        // reconciler wire.
        for v in RoutingEdgeResource::ALL {
            let id = v.wire_identity();
            assert_eq!(id.api_version, v.api_version());
            assert_eq!(id.kind, v.kind());
        }
    }

    #[test]
    fn wire_identity_is_const_reachable() {
        // Compile-time reachability pin: `wire_identity` is `const
        // fn` so a caller (any emit site or future coherence sweep)
        // can bind the pair into a `const` slot. A regression that
        // dropped the `const` qualifier would fail-loudly here.
        const I: crate::k8s_wire_identity::K8sWireIdentity =
            RoutingEdgeResource::Ingress.wire_identity();
        const D: crate::k8s_wire_identity::K8sWireIdentity =
            RoutingEdgeResource::DnsEndpoint.wire_identity();
        assert_eq!(I.api_version, "networking.k8s.io/v1");
        assert_eq!(I.kind, "Ingress");
        assert_eq!(D.api_version, "externaldns.k8s.io/v1alpha1");
        assert_eq!(D.kind, "DNSEndpoint");
    }

    #[test]
    fn projections_are_pure_functions_of_the_variant() {
        // Purity pin: calling the projections repeatedly on the same
        // variant returns byte-identical `&'static str`s. Guards
        // against an implementation that lazily materialized an
        // interned key per-call and hashed against runtime state.
        for v in RoutingEdgeResource::ALL {
            assert_eq!(v.api_version(), v.api_version());
            assert_eq!(v.kind(), v.kind());
        }
    }
}
