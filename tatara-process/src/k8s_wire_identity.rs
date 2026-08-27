//! `K8sWireIdentity` — typed pair carrying a K8s resource's
//! `(apiVersion, kind)` wire-form identity + a composer that seeds a
//! resource JSON with those two slots pre-set — the substrate
//! primitive that owns the workspace-wide "the emit site names its
//! resource kind exactly once" invariant.
//!
//! Pre-lift the 2-slot `{"apiVersion": X.api_version(), "kind":
//! X.kind()}` shape was hand-authored across FIVE production emit
//! sites in `tatara-reconciler` past the ★★ PRIME-DIRECTIVE ≥ 2
//! duplication threshold, split across two typed closed-set
//! substrates and two files:
//!
//! * [`crate::routing_edge_resource::RoutingEdgeResource`] on the
//!   routing-edge axis — TWO emit sites in
//!   `tatara-reconciler::edges`:
//!     * `IngressEdge::render` — the emitted
//!       `json!({"apiVersion": RoutingEdgeResource::Ingress.api_version(),
//!       "kind": RoutingEdgeResource::Ingress.kind(), ...})` block.
//!     * `DnsEndpointEdge::render` — the emitted
//!       `json!({"apiVersion": RoutingEdgeResource::DnsEndpoint.api_version(),
//!       "kind": RoutingEdgeResource::DnsEndpoint.kind(), ...})` block.
//! * [`crate::flux_resource::FluxResource`] on the FluxCD wire-form
//!   axis — THREE emit sites in `tatara-reconciler::render`:
//!     * `render_flux` — the Kustomization emit at
//!       `json!({"apiVersion": FluxResource::Kustomization.api_version(),
//!       "kind": FluxResource::Kustomization.kind(), ...})`.
//!     * `render_aplicacao` — the OCIRepository emit at
//!       `json!({"apiVersion": FluxResource::OCIRepository.api_version(),
//!       "kind": FluxResource::OCIRepository.kind(), ...})`.
//!     * `render_aplicacao` — the HelmRelease emit at
//!       `json!({"apiVersion": FluxResource::HelmRelease.api_version(),
//!       "kind": FluxResource::HelmRelease.kind(), ...})`.
//!
//! At every pre-lift site the emit block referenced the SAME closed-
//! set variant TWICE (once through `.api_version()`, once through
//! `.kind()`), leaving a silent-drift path: a copy-paste that swapped
//! ONE mention of the variant (`FluxResource::HelmRelease.api_version()
//! `+ `FluxResource::OCIRepository.kind()`) would produce a resource
//! whose `(apiVersion, kind)` pair no K8s controller recognizes — a
//! 404 at wire time diagnosed as a broken CRD rather than as a
//! variant skew at the emit site. Post-lift each emit site names the
//! variant ONCE via `.wire_identity()`, and the `(apiVersion, kind)`
//! pair is bound structurally at the [`K8sWireIdentity`] struct.
//!
//! Sibling to the same-axis substrate primitives on the K8s wire-form
//! identity axis: the [`crate::PROCESS_KIND`] + [`crate::api_version`]
//! pair owns the tatara `Process` CRD's own `(apiVersion, kind)` pair
//! that `crate::owner_reference_json` composes; the two closed-set
//! projections [`crate::flux_resource::FluxResource::api_version`] +
//! [`crate::routing_edge_resource::RoutingEdgeResource::api_version`]
//! own the per-variant projections this typed pair composes with.
//! Together they close the axis: every K8s wire-form identity in the
//! reconciler routes through ONE typed owner.
//!
//! Extension: a new closed-set of K8s wire-form identities (a
//! Gateway API `HTTPRoute` / `Gateway` set, a `NetworkPolicy` edge
//! set, a Cloudflare API CR set, a future notification-controller
//! `Alert` / `Provider` variant on the Flux axis) lands as ONE
//! `wire_identity(self) -> K8sWireIdentity` const projection on the
//! new closed set + ONE `impl` block on that new enum; every emit
//! site in the workspace inherits the composer mechanically through
//! the same `.wire_identity().resource_json(json!({...}))` idiom.
//!
//! Theory grounding: THEORY.md §II.1 invariant 5 (composition
//! preserves proofs — the two-slot `(apiVersion, kind)` composition
//! lives at ONE typed algebra projection here; a regression that
//! drifted a variant mention at ONE consumer would fail-loudly at
//! this module's byte-shape pins rather than as silent wire-form
//! skew at the K8s API server). THEORY.md §VI.1 (generation over
//! composition — the 2-slot shape recurred at five hand-authored
//! sites past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is
//! lifted to ONE composer here).

use serde_json::{Map, Value};

/// K8s wire-form identity of a resource — the `(apiVersion, kind)`
/// pair, carried as a typed struct so a caller who receives one
/// cannot skew the two slots at compose time.
///
/// Instances live at
/// [`crate::flux_resource::FluxResource::wire_identity`] and
/// [`crate::routing_edge_resource::RoutingEdgeResource::wire_identity`]
/// (both `const fn` projections off the enclosing closed set); a
/// future closed set names its own
/// `wire_identity(self) -> K8sWireIdentity` const projection and
/// inherits the composer at [`Self::resource_json`] mechanically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct K8sWireIdentity {
    /// K8s wire-form `apiVersion` string — the `<group>/<version>`
    /// pair the K8s API server dispatches on.
    pub api_version: &'static str,
    /// K8s wire-form `kind` string — the PascalCase identifier the
    /// K8s API server matches against the resource's `kind:` slot.
    pub kind: &'static str,
}

impl K8sWireIdentity {
    /// Pair `(api_version, kind)` into a typed identity. `const fn`
    /// so a caller (typically a closed-set variant's projection) can
    /// bind the result into a `const` slot; the two projections
    /// `FluxResource::wire_identity` +
    /// `RoutingEdgeResource::wire_identity` are const-callable so a
    /// future coherence sweep or a compile-time `const` cache reads
    /// the pair with zero runtime overhead.
    pub const fn new(api_version: &'static str, kind: &'static str) -> Self {
        Self { api_version, kind }
    }

    /// Emit as a 2-slot `{ "apiVersion": ..., "kind": ... }` JSON
    /// object — the identity block a full K8s resource JSON prefixes
    /// its `metadata` + `spec` slots with.
    pub fn as_json(self) -> Value {
        let mut m = Map::with_capacity(2);
        m.insert(
            "apiVersion".to_string(),
            Value::String(self.api_version.to_string()),
        );
        m.insert("kind".to_string(), Value::String(self.kind.to_string()));
        Value::Object(m)
    }

    /// Compose a full K8s resource JSON with the identity's
    /// `apiVersion` + `kind` slots pre-seeded on top of a caller-
    /// supplied `extras` object (the caller's `metadata`, `spec`, and
    /// any auxiliary slots). The identity slots always win over
    /// matching keys in `extras`: a caller who accidentally inlined
    /// their own `apiVersion` or `kind` on top of the identity gets
    /// the typed pair, not the inline literal — the whole point of
    /// this composer is that the identity is bound structurally.
    ///
    /// `extras` that isn't a JSON object is treated as an empty
    /// object (returning a pure 2-slot identity block); this matches
    /// the empty-extras fallback every hand-lift pre-refactor sites
    /// would have implied.
    pub fn resource_json(self, extras: Value) -> Value {
        let mut m = match extras {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        // Insert identity slots LAST so they overwrite any collision
        // in `extras` — structural binding of the (apiVersion, kind)
        // pair beats caller-inlined literals.
        m.insert(
            "apiVersion".to_string(),
            Value::String(self.api_version.to_string()),
        );
        m.insert("kind".to_string(), Value::String(self.kind.to_string()));
        Value::Object(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_pairs_api_version_and_kind_by_position() {
        // Positional pin: `new(api_version, kind)` binds the first
        // argument to the `api_version` slot and the second to `kind`.
        // A regression that swapped the two — trivial with two
        // `&'static str` params — would surface here rather than as
        // silently-swapped emits at every callsite.
        let id = K8sWireIdentity::new("group.io/v1", "Widget");
        assert_eq!(id.api_version, "group.io/v1");
        assert_eq!(id.kind, "Widget");
    }

    #[test]
    fn new_is_const_reachable() {
        // Compile-time reachability pin: the constructor is `const
        // fn` so a `const` caller (any closed-set variant projection)
        // can bind the pair into a compile-time slot. A regression
        // that dropped the `const` qualifier would fail-loudly here
        // rather than as a runtime dispatch at every projection call.
        const ID: K8sWireIdentity = K8sWireIdentity::new("g/v", "K");
        assert_eq!(ID.api_version, "g/v");
        assert_eq!(ID.kind, "K");
    }

    #[test]
    fn as_json_emits_two_slot_object() {
        let id = K8sWireIdentity::new("group.io/v1", "Widget");
        let v = id.as_json();
        let obj = v.as_object().expect("as_json emits an Object");
        assert_eq!(obj.len(), 2);
        assert_eq!(obj["apiVersion"], "group.io/v1");
        assert_eq!(obj["kind"], "Widget");
    }

    #[test]
    fn resource_json_merges_identity_with_extras() {
        // The composer prefixes the identity slots onto the caller's
        // extras — the emit-site shape every pre-lift `json!({
        // "apiVersion": X.api_version(), "kind": X.kind(),
        // "metadata": M, "spec": S })` block projected by hand.
        let id = K8sWireIdentity::new("group.io/v1", "Widget");
        let out = id.resource_json(json!({
            "metadata": {"name": "foo"},
            "spec": {"replicas": 3},
        }));
        let obj = out.as_object().expect("resource_json emits an Object");
        assert_eq!(obj["apiVersion"], "group.io/v1");
        assert_eq!(obj["kind"], "Widget");
        assert_eq!(obj["metadata"]["name"], "foo");
        assert_eq!(obj["spec"]["replicas"], 3);
    }

    #[test]
    fn resource_json_identity_slots_win_over_extras_collision() {
        // Coherence pin: a caller who accidentally inlined their own
        // `apiVersion` or `kind` on top of the identity gets the
        // typed pair, not the inline literal. The whole reason to
        // route through this composer is that the identity slots are
        // bound structurally — a regression that let extras win at
        // one collision would defeat the composer's purpose.
        let id = K8sWireIdentity::new("group.io/v1", "Widget");
        let out = id.resource_json(json!({
            "apiVersion": "wrong.io/v0",
            "kind": "WrongKind",
            "metadata": {"name": "foo"},
        }));
        let obj = out.as_object().unwrap();
        assert_eq!(obj["apiVersion"], "group.io/v1");
        assert_eq!(obj["kind"], "Widget");
        assert_eq!(obj["metadata"]["name"], "foo");
    }

    #[test]
    fn resource_json_non_object_extras_returns_pure_identity() {
        // A caller who threaded a non-object extras value (e.g.
        // `Value::Null` fallthrough from an option) gets a pure
        // 2-slot identity block rather than a panic or a silently
        // stringified extras. Matches the empty-extras fallback
        // implied by every pre-lift site.
        let id = K8sWireIdentity::new("group.io/v1", "Widget");
        let out = id.resource_json(Value::Null);
        let obj = out.as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert_eq!(obj["apiVersion"], "group.io/v1");
        assert_eq!(obj["kind"], "Widget");

        let out2 = id.resource_json(Value::String("scalar-extras".into()));
        let obj2 = out2.as_object().unwrap();
        assert_eq!(obj2.len(), 2);
        assert_eq!(obj2["apiVersion"], "group.io/v1");
    }

    #[test]
    fn resource_json_empty_object_extras_returns_pure_identity() {
        let id = K8sWireIdentity::new("group.io/v1", "Widget");
        let out = id.resource_json(json!({}));
        let obj = out.as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert_eq!(obj["apiVersion"], "group.io/v1");
        assert_eq!(obj["kind"], "Widget");
    }

    #[test]
    fn as_json_and_resource_json_agree_on_identity_slots() {
        // Consistency pin: the pure identity emit and the composed
        // emit expose the same `(apiVersion, kind)` slot pair.
        // A regression that special-cased one path (e.g. `as_json`
        // reading a lazily-materialized string but `resource_json`
        // reading the struct field) would surface here.
        let id = K8sWireIdentity::new("group.io/v1", "Widget");
        let pure = id.as_json();
        let composed = id.resource_json(json!({"metadata": {"name": "foo"}}));
        assert_eq!(pure["apiVersion"], composed["apiVersion"]);
        assert_eq!(pure["kind"], composed["kind"]);
    }

    #[test]
    fn struct_equality_pins_the_pair_axis() {
        // Two identities with the same (api_version, kind) compare
        // equal; distinct pairs do not. Guards against a future
        // `Hash` / `Eq` derive drift that skewed variant equivalence.
        let a = K8sWireIdentity::new("g/v1", "K");
        let b = K8sWireIdentity::new("g/v1", "K");
        let c = K8sWireIdentity::new("g/v2", "K");
        let d = K8sWireIdentity::new("g/v1", "L");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }
}
