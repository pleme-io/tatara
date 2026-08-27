//! `K8sObjectRef` — typed 3-slot `(kind, name, namespace)` cross-
//! resource reference, and the composer that emits the canonical
//! `{ "kind": …, "name": …, "namespace": … }` JSON pointer one K8s
//! resource carries at a `sourceRef` / `chartRef` / equivalent slot
//! to point at another K8s resource.
//!
//! Pre-lift the 3-slot `{kind, name, namespace}` shape was hand-
//! authored across THREE production emit sites in
//! `tatara-reconciler::render` past the ★★ PRIME-DIRECTIVE ≥ 2
//! duplication threshold — the same JSON shape recurred three times
//! at distinct callers pointing at three distinct K8s resource kinds
//! (`GitRepository`, `OCIRepository`, `HelmRepository`):
//!
//! * `render_flux` — the `Kustomization.spec.sourceRef` block
//!   pointing at a `GitRepository` (with `namespace` falling back to
//!   `"flux-system"` when unspecified).
//! * `render_aplicacao` — the `HelmRelease.spec.chartRef` block
//!   pointing at an `OCIRepository` (its `kind` slot already sourced
//!   through the [`crate::flux_resource::FluxResource::OCIRepository`]
//!   closed-set variant's `.kind()`).
//! * `render_aplicacao` — the `HelmRelease.spec.chart.spec.sourceRef`
//!   block pointing at a `HelmRepository` (in `"flux-system"`).
//!
//! Every pre-lift site restated the same three `Value::String` slot
//! insertions in the same key order — a caller who omitted one slot,
//! swapped `"name"` and `"namespace"`, or added a stray fourth slot
//! (`apiVersion` — a slot the K8s cross-reference form deliberately
//! excludes because it is derived from `kind` by the owning
//! controller) would surface only as a wire-time error at the K8s
//! API server, not at the emit site. Post-lift each emit site
//! composes ONE [`K8sObjectRef`] and calls [`Self::as_json`]; the
//! 3-slot shape lives at ONE composer and rustc enforces every
//! consumer stamps exactly the (kind, name, namespace) triple with
//! no fourth-slot drift.
//!
//! Sibling to the same-axis substrate primitive
//! [`crate::k8s_wire_identity::K8sWireIdentity`] on the K8s wire-
//! form identity axis. [`K8sWireIdentity`] owns the
//! `(apiVersion, kind)` pair a K8s resource carries at its TOP-LEVEL
//! identity slots (its own `apiVersion` + `kind` keys); this
//! primitive owns the `(kind, name, namespace)` triple a K8s
//! resource carries in a NESTED reference slot (a `sourceRef` /
//! `chartRef` pointing at another resource — `apiVersion` deliberately
//! omitted because the owning controller derives it from `kind`).
//! The projection [`K8sWireIdentity::object_ref`] bridges the two:
//! any typed closed-set variant that projects a `K8sWireIdentity`
//! (via [`crate::flux_resource::FluxResource::wire_identity`] or
//! [`crate::routing_edge_resource::RoutingEdgeResource::wire_identity`])
//! composes a `K8sObjectRef` mechanically without restating the
//! `kind` slot at the reference site.
//!
//! Extension: a new cross-reference site the reconciler grows (a
//! Flux `Alert.spec.providerRef` at a `Provider`, a Gateway API
//! `HTTPRoute.spec.parentRefs[i]` at a `Gateway`, a Cloudflare CR
//! that references a `Secret` by `sourceRef`) lands as ONE
//! `.as_json()` call at the emit site; every axis' typed closed
//! set inherits the composer through [`K8sWireIdentity::object_ref`]
//! without a new primitive per axis.
//!
//! Theory grounding: THEORY.md §II.1 invariant 5 (composition
//! preserves proofs — the three-slot cross-reference composition
//! lives at ONE typed algebra composer here; a regression that
//! drifted any one slot at ONE consumer would fail-loudly at this
//! module's byte-shape pins rather than as silent wire-form skew
//! at the K8s API server). THEORY.md §VI.1 (generation over
//! composition — the 3-slot shape recurred at three hand-authored
//! sites past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is
//! lifted to ONE composer here).

use serde_json::{Map, Value};

use crate::k8s_wire_identity::K8sWireIdentity;

/// K8s cross-resource reference — the 3-slot
/// `{ "kind", "name", "namespace" }` pointer one K8s resource
/// carries at a `sourceRef` / `chartRef` / equivalent nested slot
/// to point at another K8s resource in the cluster.
///
/// The slot set is intentionally the 3-slot form (no `apiVersion`)
/// used by FluxCD's `sourceRef` + `chartRef` slots and the K8s
/// `TypedLocalObjectReference` / `TypedObjectReference` shapes: the
/// owning controller derives the apiVersion from `kind` (typically
/// by dispatch through its own resource registry), so an emit site
/// that hand-stamped an `apiVersion` here would either be ignored
/// (adding a fourth slot no controller reads) or silently take
/// precedence over the controller's derivation and mis-route
/// resolution. The composer at [`Self::as_json`] emits exactly the
/// 3-slot shape — a regression that added or removed a slot would
/// surface at this module's byte-shape pins.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct K8sObjectRef {
    /// K8s wire-form `kind` string of the referenced resource — the
    /// PascalCase identifier the owning controller matches against
    /// its resource registry to derive the apiVersion + REST route.
    pub kind: String,
    /// K8s `metadata.name` of the referenced resource — the API-path
    /// leaf segment.
    pub name: String,
    /// K8s `metadata.namespace` of the referenced resource. The K8s
    /// cross-reference form treats this as a required slot even when
    /// the reference lives in the same namespace as the referrer
    /// (every pre-lift callsite in `tatara-reconciler::render`
    /// stamped it explicitly), so this primitive carries it
    /// unconditionally.
    pub namespace: String,
}

impl K8sObjectRef {
    /// Construct a `K8sObjectRef` from the (kind, name, namespace)
    /// triple. Accepts any `Into<String>` at each slot so a caller
    /// with `&str` / `String` / `Cow<'_, str>` composes without a
    /// `.to_string()` per site.
    pub fn new(
        kind: impl Into<String>,
        name: impl Into<String>,
        namespace: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            name: name.into(),
            namespace: namespace.into(),
        }
    }

    /// Emit as a 3-slot `{ "kind": …, "name": …, "namespace": … }`
    /// JSON object — the exact shape every pre-lift `sourceRef` /
    /// `chartRef` block in `tatara-reconciler::render` hand-authored
    /// by inlining three `Value::String` slot insertions. Slot count
    /// + slot keys are pinned by
    /// [`tests::as_json_emits_exactly_three_slots_named_kind_name_namespace`]
    /// so a regression that added or removed a slot (or renamed one
    /// to camelCase drift) surfaces at the primitive rather than as
    /// silent wire-form skew at every emit site.
    pub fn as_json(&self) -> Value {
        let mut m = Map::with_capacity(3);
        m.insert("kind".to_string(), Value::String(self.kind.clone()));
        m.insert("name".to_string(), Value::String(self.name.clone()));
        m.insert(
            "namespace".to_string(),
            Value::String(self.namespace.clone()),
        );
        Value::Object(m)
    }
}

impl K8sWireIdentity {
    /// Bridge from the typed `(apiVersion, kind)` identity pair to
    /// the typed `(kind, name, namespace)` cross-resource reference:
    /// binds the identity's `kind` slot into the reference and
    /// carries the caller-supplied `(name, namespace)` pair through.
    ///
    /// Every typed closed-set variant that projects a
    /// `K8sWireIdentity` (via
    /// [`crate::flux_resource::FluxResource::wire_identity`] or
    /// [`crate::routing_edge_resource::RoutingEdgeResource::wire_identity`])
    /// composes a `K8sObjectRef` mechanically through this projection
    /// without restating the `kind` slot at the reference site. A
    /// future closed-set inherits the same projection chain for free
    /// through its own `wire_identity()` const.
    ///
    /// The projection intentionally DROPS `apiVersion` — the K8s
    /// cross-reference form (Flux's `sourceRef` / `chartRef`, the
    /// K8s `TypedObjectReference` shape) reads only `kind` and
    /// leaves apiVersion resolution to the owning controller's own
    /// resource registry. A caller that needs the full pair reaches
    /// through the identity itself via [`Self::as_json`] or
    /// [`Self::resource_json`], not through this projection.
    pub fn object_ref(self, name: impl Into<String>, namespace: impl Into<String>) -> K8sObjectRef {
        K8sObjectRef::new(self.kind, name, namespace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flux_resource::FluxResource;
    use crate::routing_edge_resource::RoutingEdgeResource;

    #[test]
    fn new_binds_slots_by_position() {
        // Positional pin: `new(kind, name, namespace)` binds the
        // three arguments to the three slots in the given order. A
        // regression that swapped `name` and `namespace` — trivial
        // with three `impl Into<String>` params — would surface here
        // rather than as silently-swapped emits at every callsite.
        let r = K8sObjectRef::new("Widget", "my-widget", "my-ns");
        assert_eq!(r.kind, "Widget");
        assert_eq!(r.name, "my-widget");
        assert_eq!(r.namespace, "my-ns");
    }

    #[test]
    fn new_accepts_str_and_owned_string_at_every_slot() {
        // Composability pin: every slot accepts `&str` and `String`
        // interchangeably. Guards against a future signature drift
        // (e.g. a `&'static str` slot that broke a callsite passing
        // `f.git_repository: String`).
        let a = K8sObjectRef::new("K", "n", "ns");
        let b = K8sObjectRef::new(String::from("K"), String::from("n"), String::from("ns"));
        assert_eq!(a, b);
    }

    #[test]
    fn as_json_emits_exactly_three_slots_named_kind_name_namespace() {
        // Shape pin: the composer emits an object with EXACTLY the
        // three slots `kind`, `name`, `namespace`. A regression that
        // added an `apiVersion` slot (the wrong-form drift a
        // hand-lift might introduce by copy-pasting from a top-level
        // resource identity) would surface here rather than as
        // silent controller mis-resolution at wire time. A regression
        // that renamed any slot (e.g. camelCase drift to `Name`) or
        // dropped one surfaces here as well.
        let r = K8sObjectRef::new("Widget", "my-widget", "my-ns");
        let v = r.as_json();
        let obj = v.as_object().expect("as_json emits an Object");
        assert_eq!(obj.len(), 3, "K8sObjectRef must emit exactly 3 slots");
        assert_eq!(obj["kind"], "Widget");
        assert_eq!(obj["name"], "my-widget");
        assert_eq!(obj["namespace"], "my-ns");
        assert!(
            !obj.contains_key("apiVersion"),
            "K8sObjectRef must not stamp an `apiVersion` slot — the K8s cross-\
             reference form derives apiVersion from `kind` at the owning controller"
        );
    }

    #[test]
    fn as_json_is_a_pure_function_of_the_triple() {
        // Purity pin: two identically-constructed refs emit
        // byte-equal JSON, and one ref emits byte-equal JSON on
        // repeated calls. Guards against a future implementation
        // that lazily materialized a random ordering (would break a
        // downstream `canonical_bytes` equality check at wire time).
        let a = K8sObjectRef::new("K", "n", "ns");
        let b = K8sObjectRef::new("K", "n", "ns");
        assert_eq!(a.as_json(), b.as_json());
        assert_eq!(a.as_json(), a.as_json());
    }

    #[test]
    fn struct_equality_pins_the_triple_axis() {
        // Two refs with the same (kind, name, namespace) compare
        // equal; distinct triples do not. Guards against a future
        // derive drift that skewed slot equivalence.
        let a = K8sObjectRef::new("K", "n", "ns");
        let b = K8sObjectRef::new("K", "n", "ns");
        let diff_kind = K8sObjectRef::new("K2", "n", "ns");
        let diff_name = K8sObjectRef::new("K", "n2", "ns");
        let diff_ns = K8sObjectRef::new("K", "n", "ns2");
        assert_eq!(a, b);
        assert_ne!(a, diff_kind);
        assert_ne!(a, diff_name);
        assert_ne!(a, diff_ns);
    }

    #[test]
    fn wire_identity_object_ref_binds_kind_from_the_identity() {
        // Coherence pin: `K8sWireIdentity::object_ref` sources the
        // reference's `kind` slot from the identity, not from the
        // caller. A regression that stamped the caller's own kind
        // (or the wrong slot from the identity) would break the
        // whole reason to route through this projection — that a
        // future closed-set variant's kind rename lands at ONE arm
        // on the substrate and reaches every reference site
        // mechanically.
        let id = K8sWireIdentity::new("group.io/v1", "Widget");
        let r = id.object_ref("my-widget", "my-ns");
        assert_eq!(r.kind, "Widget");
        assert_eq!(r.name, "my-widget");
        assert_eq!(r.namespace, "my-ns");
    }

    #[test]
    fn wire_identity_object_ref_drops_api_version() {
        // The K8s cross-reference form has no `apiVersion` slot;
        // `object_ref` must NOT plumb it through. A regression that
        // widened the reference to a 4-slot form would surface at
        // this pin AND at `as_json_emits_exactly_three_slots_named_kind_name_namespace`.
        let id = K8sWireIdentity::new("group.io/v1", "Widget");
        let r = id.object_ref("my-widget", "my-ns");
        let obj = r.as_json();
        let obj = obj.as_object().unwrap();
        assert!(!obj.contains_key("apiVersion"));
        assert_eq!(obj.len(), 3);
    }

    #[test]
    fn flux_resource_composes_object_ref_through_wire_identity() {
        // Chain pin: every `FluxResource` variant composes a
        // `K8sObjectRef` through its `wire_identity()` projection.
        // The reference's `kind` slot always agrees with the
        // variant's `.kind()` — a regression at ONE variant's kind
        // rename would surface here rather than at every consumer.
        for v in FluxResource::ALL {
            let r = v.wire_identity().object_ref("some-name", "some-ns");
            assert_eq!(r.kind, v.kind());
            assert_eq!(r.name, "some-name");
            assert_eq!(r.namespace, "some-ns");
        }
    }

    #[test]
    fn routing_edge_resource_composes_object_ref_through_wire_identity() {
        // Chain pin, sibling axis: every `RoutingEdgeResource`
        // variant composes a `K8sObjectRef` through its
        // `wire_identity()` projection — matching what the sibling
        // `FluxResource` axis buys. A future routing-edge kind lands
        // at ONE arm on the closed set and inherits the composer
        // through the same chain.
        for v in RoutingEdgeResource::ALL {
            let r = v.wire_identity().object_ref("edge-name", "edge-ns");
            assert_eq!(r.kind, v.kind());
        }
    }

    #[test]
    fn as_json_matches_pre_lift_flux_source_ref_hand_authored_shape() {
        // Byte-shape pin against the pre-lift hand-authored
        // `sourceRef` block at `tatara-reconciler::render::render_flux`
        // (line 124-133 pre-lift) pointing at a `GitRepository`:
        //   json!({
        //       "kind": "GitRepository",
        //       "name": f.git_repository,
        //       "namespace": f.git_repository_namespace
        //           .clone()
        //           .unwrap_or_else(|| "flux-system".into()),
        //   })
        // Post-lift the same shape composes through this primitive.
        // A regression that reshaped the emitted JSON at either the
        // primitive or the callsite would surface at this pin.
        let hand_authored = serde_json::json!({
            "kind": "GitRepository",
            "name": "flux-system",
            "namespace": "flux-system",
        });
        let composed = K8sObjectRef::new("GitRepository", "flux-system", "flux-system").as_json();
        assert_eq!(composed, hand_authored);
    }

    #[test]
    fn as_json_matches_pre_lift_helm_chart_ref_hand_authored_shape() {
        // Byte-shape pin against the pre-lift hand-authored
        // `chartRef` block at
        // `tatara-reconciler::render::render_aplicacao` (line 248-254
        // pre-lift) pointing at an `OCIRepository`:
        //   json!({
        //       "kind": FluxResource::OCIRepository.kind(),
        //       "name": name,
        //       "namespace": ns,
        //   })
        let hand_authored = serde_json::json!({
            "kind": FluxResource::OCIRepository.kind(),
            "name": "ephemeral-demo",
            "namespace": "demo-test",
        });
        let composed = FluxResource::OCIRepository
            .wire_identity()
            .object_ref("ephemeral-demo", "demo-test")
            .as_json();
        assert_eq!(composed, hand_authored);
    }

    #[test]
    fn as_json_matches_pre_lift_helm_repository_source_ref_hand_authored_shape() {
        // Byte-shape pin against the pre-lift hand-authored
        // `sourceRef` block at
        // `tatara-reconciler::render::render_aplicacao` (line 265-269
        // pre-lift) pointing at a `HelmRepository`:
        //   json!({
        //       "kind": "HelmRepository",
        //       "name": repo,
        //       "namespace": "flux-system",
        //   })
        let hand_authored = serde_json::json!({
            "kind": "HelmRepository",
            "name": "pleme-io",
            "namespace": "flux-system",
        });
        let composed = K8sObjectRef::new("HelmRepository", "pleme-io", "flux-system").as_json();
        assert_eq!(composed, hand_authored);
    }
}
