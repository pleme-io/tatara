//! `RoutingSpec` — declared DNS + Ingress edges this Process exposes.
//!
//! The substrate move: every Process can declare hostnames at which
//! it answers. The reconciler emits one `networking.k8s.io/v1`
//! Ingress + one `externaldns.k8s.io/v1alpha1` DNSEndpoint per
//! entry, owned by the Process via ownerRefs (cascade-delete on
//! Reaped). DNS records are declarative — the Process IS the source
//! of truth for `${app}.${eph_id}.${cluster}.${location}.${domain}`.
//!
//! Two hostname forms:
//!
//! 1. **Per-instance** — `${app}.${eph_id}.${cluster}.${loc}.${domain}`.
//!    The `eph_id` segment is the `hostnames[i].instance` value when
//!    set, or the BLAKE3:8 short-hash of the Process's canonical
//!    spec when unset. Stable for the lifetime of the spec; new
//!    spec content ⇒ new hash ⇒ new slot.
//!
//! 2. **Stable claim** — `${app}.${cluster}.${loc}.${domain}` (no
//!    `eph_id` segment). Emitted iff `stable_name_claim: true` AND
//!    this Process currently holds the ProcessTable.claims entry
//!    for `(cluster, app)`. The claim arbiter handles atomic
//!    transfer when the holder fails.
//!
//! Lisp authoring:
//! ```lisp
//! :routing (:hostnames ((:app "api" :instance "demo-prod")
//!                       (:app "gateway"))
//!           :backend   (:service "demo-app-gateway"
//!                       :port    8000)
//!           :stable-name-claim #t
//!           :priority           100)
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tatara_lisp::DeriveTataraDomain;

/// Declared external edges (DNS + Ingress) this Process exposes.
///
/// Optional on `ProcessSpec` — None means the Process is in-cluster-
/// only, matching today's default behavior. The reconciler only
/// emits routing artifacts when this slot is populated.
#[derive(DeriveTataraDomain, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defrouting")]
pub struct RoutingSpec {
    /// Hostnames this Process answers on. Empty list is legal but
    /// nonsensical (no Ingress, no DNS) — operators should drop the
    /// `routing` slot entirely instead. The reconciler warns on
    /// empty hostnames.
    #[serde(default)]
    pub hostnames: Vec<RoutingHostname>,

    /// Single backend Service every hostname routes to. Per-hostname
    /// backends are a future extension; v1 keeps the simple shape.
    pub backend: RoutingBackend,

    /// When true, additionally emit the *unprefixed* form of every
    /// hostname (`${app}.${cluster}.${loc}.${domain}` — no
    /// `eph_id` segment) iff this Process currently holds the
    /// ProcessTable claim for `(cluster, app)`. At most one Process
    /// per (cluster, app) holds the claim.
    #[serde(default)]
    pub stable_name_claim: bool,

    /// Claim arbitration priority. Higher wins. Ties broken by
    /// oldest `creationTimestamp`. Negative values legal (signals
    /// "prefer not to hold the claim"). Default 0.
    #[serde(default)]
    pub priority: i32,
}

/// One entry in `RoutingSpec.hostnames`.
///
/// Emitted FQDN: `${app}.${ephemeral_id}.${cluster}.${location}.${domain}`
/// where:
/// * `app` and (optional) `instance` come from this struct;
/// * `cluster` falls back to reconciler-config when unset;
/// * `location` and `domain` are reconciler-config (from
///   `nix/lib/fleet-domains.nix`).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoutingHostname {
    /// Application slot — `api`, `gateway`, `web`, etc.
    /// Must be a valid DNS label (RFC 1123): lowercase alpha-num
    /// + hyphen, 1–63 chars, no leading/trailing hyphen. The
    /// reconciler validates this at the boundary.
    pub app: String,

    /// Named instance segment. When `Some("demo-prod")` the FQDN
    /// reads `${app}.demo-prod.${cluster}.…`. When `None` the
    /// reconciler substitutes `blake3(canonical_spec)[:8]` —
    /// deterministic per-spec, changes when the spec changes.
    ///
    /// Must be a valid DNS label when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,

    /// Cluster override. Empty/None ⇒ reconciler-config default
    /// (e.g., `pleme-dev`). Used for cross-cluster routing rules,
    /// rare in practice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<String>,
}

/// Backend Service the FQDN's Ingress routes traffic to.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoutingBackend {
    /// In-cluster Service name (same namespace as the Process).
    pub service: String,

    /// Port number on the Service to route to.
    pub port: u16,

    /// `ClusterIssuer` name for TLS. None ⇒ reconciler-config
    /// default (typically `letsencrypt-prod` or the cluster's
    /// SPIRE-issuing issuer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_issuer: Option<String>,

    /// Annotations stamped on every emitted Ingress. Common keys:
    /// `nginx.ingress.kubernetes.io/rate-limit`, `nginx.ingress.
    /// kubernetes.io/proxy-body-size`. The reconciler MERGES these
    /// with its own annotations; conflict ⇒ this map wins.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ingress_annotations: BTreeMap<String, String>,
}

impl RoutingSpec {
    /// True iff at least one hostname is declared. The reconciler
    /// uses this to short-circuit: empty routing ⇒ no emission.
    pub fn has_hostnames(&self) -> bool {
        !self.hostnames.is_empty()
    }

    /// Total count of FQDNs this Process will emit:
    /// `hostnames.len()` per-instance + `hostnames.len()` stable
    /// when the claim is held.
    pub fn emitted_fqdn_count(&self, claim_held: bool) -> usize {
        self.hostnames.len() * if claim_held { 2 } else { 1 }
    }
}

impl RoutingHostname {
    /// True iff this entry resolves to a named slot (vs content-hash).
    pub fn is_named(&self) -> bool {
        self.instance.as_deref().is_some_and(|s| !s.is_empty())
    }
}

/// Wire-form value stamped at
/// [`tatara_process::annotations::ROUTING_FORM`][
/// crate::annotations::ROUTING_FORM] on every routing edge
/// (Ingress + DNSEndpoint) — both the `annotations` axis and the
/// `labels` axis carry it. Distinguishes the two FQDN shapes
/// [`RoutingSpec`] emits: the per-instance form
/// (`${app}.${eph_id}.${cluster}.${loc}.${domain}`) and the
/// stable-claim form (`${app}.${cluster}.${loc}.${domain}`,
/// emitted iff `stable_name_claim` is set and this Process
/// currently holds the ProcessTable claim for `(cluster, app)`).
///
/// The pre-lift reconciler restated the same
/// `if ctx.is_stable { "stable" } else { "instance" }` ternary at
/// three call sites (an Ingress annotation, an Ingress label, a
/// DNSEndpoint label) plus two byte-literal comparison sites in
/// render tests. This typed enum turns that stringly-typed
/// disjunction into a two-variant type with a single wire
/// encoding, so a future edge kind (a Gateway API `HTTPRoute`, a
/// `NetworkPolicy` edge) sourcing the axis through
/// [`RoutingForm::from_is_stable`] + [`RoutingForm::as_str`]
/// cannot drift from the two existing edges' spellings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutingForm {
    /// Emitted iff `RoutingSpec.stable_name_claim = true` AND
    /// this Process currently holds the ProcessTable claim for
    /// `(cluster, app)`. FQDN drops the `${eph_id}` segment.
    Stable,
    /// Emitted for every declared hostname entry (default). FQDN
    /// carries the `${eph_id}` segment resolved by
    /// [`crate::hostname::resolve_ephemeral_id`].
    Instance,
}

impl RoutingForm {
    /// Wire-form byte-shape stamped into the
    /// [`ROUTING_FORM`][crate::annotations::ROUTING_FORM]
    /// annotation / label. The reconciler's stable-form filter
    /// checks byte-identity against these two strings — a rename
    /// here is a wire-form break every operator's kubectl-side
    /// selector notices.
    pub const fn as_str(self) -> &'static str {
        match self {
            RoutingForm::Stable => "stable",
            RoutingForm::Instance => "instance",
        }
    }

    /// Route the reconciler's `EdgeContext::is_stable` bool
    /// through ONE composer so every downstream axis (the
    /// stable-form suffix in edge resource names + the
    /// [`ROUTING_FORM`][crate::annotations::ROUTING_FORM] value
    /// on labels + annotations) shares the same source of truth.
    pub const fn from_is_stable(is_stable: bool) -> Self {
        if is_stable {
            RoutingForm::Stable
        } else {
            RoutingForm::Instance
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_routing() -> RoutingSpec {
        RoutingSpec {
            hostnames: vec![
                RoutingHostname {
                    app: "api".into(),
                    instance: Some("demo-prod".into()),
                    cluster: None,
                },
                RoutingHostname {
                    app: "gateway".into(),
                    instance: Some("demo-prod".into()),
                    cluster: None,
                },
            ],
            backend: RoutingBackend {
                service: "demo-app-gateway".into(),
                port: 8000,
                tls_issuer: None,
                ingress_annotations: BTreeMap::new(),
            },
            stable_name_claim: true,
            priority: 100,
        }
    }

    #[test]
    fn empty_routing_resolves_no_hostnames() {
        let r = RoutingSpec {
            hostnames: vec![],
            backend: RoutingBackend {
                service: "x".into(),
                port: 80,
                tls_issuer: None,
                ingress_annotations: BTreeMap::new(),
            },
            stable_name_claim: false,
            priority: 0,
        };
        assert!(!r.has_hostnames());
        assert_eq!(r.emitted_fqdn_count(false), 0);
        assert_eq!(r.emitted_fqdn_count(true), 0);
    }

    #[test]
    fn fqdn_count_doubles_when_claim_held() {
        let r = demo_routing();
        assert_eq!(r.emitted_fqdn_count(false), 2);
        assert_eq!(r.emitted_fqdn_count(true), 4);
    }

    #[test]
    fn hostname_is_named_when_instance_nonempty() {
        let h = RoutingHostname {
            app: "x".into(),
            instance: Some("env-a".into()),
            cluster: None,
        };
        assert!(h.is_named());

        let h_anon = RoutingHostname {
            app: "x".into(),
            instance: None,
            cluster: None,
        };
        assert!(!h_anon.is_named());

        let h_empty = RoutingHostname {
            app: "x".into(),
            instance: Some(String::new()),
            cluster: None,
        };
        assert!(!h_empty.is_named()); // empty string ⇒ unnamed
    }

    #[test]
    fn serde_round_trip_via_yaml() {
        let r = demo_routing();
        let yaml = serde_yaml::to_string(&r).unwrap();
        // camelCase wire form — what FluxCD / kubectl users see.
        assert!(yaml.contains("hostnames:"));
        assert!(yaml.contains("app: api"));
        assert!(yaml.contains("instance: demo-prod"));
        assert!(yaml.contains("backend:"));
        assert!(yaml.contains("service: demo-app-gateway"));
        assert!(yaml.contains("port: 8000"));
        assert!(yaml.contains("stableNameClaim: true"));
        assert!(yaml.contains("priority: 100"));

        let back: RoutingSpec = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.hostnames.len(), 2);
        assert!(back.stable_name_claim);
        assert_eq!(back.priority, 100);
    }

    #[test]
    fn empty_fields_skip_serialize() {
        // Minimal RoutingSpec — verify that absent optional fields
        // don't pollute the wire format.
        let r = RoutingSpec {
            hostnames: vec![RoutingHostname {
                app: "api".into(),
                instance: None,
                cluster: None,
            }],
            backend: RoutingBackend {
                service: "svc".into(),
                port: 8080,
                tls_issuer: None,
                ingress_annotations: BTreeMap::new(),
            },
            stable_name_claim: false,
            priority: 0,
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        // Optional + empty fields must NOT appear in the wire form.
        assert!(!yaml.contains("instance:"));
        assert!(!yaml.contains("cluster:"));
        assert!(!yaml.contains("tlsIssuer:"));
        assert!(!yaml.contains("ingressAnnotations:"));
    }

    #[test]
    fn lisp_round_trip_via_defrouting() {
        // The `(defrouting …)` keyword is registered by
        // tatara_process::register_all (R3 adds this to the
        // registry); for now compile via tatara_lisp directly.
        let src = r#"
            (defrouting demo-edges
              :hostnames ((:app "api"   :instance "demo-prod")
                          (:app "gateway" :instance "demo-prod"))
              :backend   (:service "demo-app-gateway"
                          :port 8000)
              :stable-name-claim #t
              :priority 100)
        "#;
        let defs: Vec<tatara_lisp::NamedDefinition<RoutingSpec>> =
            tatara_lisp::compile_named::<RoutingSpec>(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        assert_eq!(d.name, "demo-edges");
        assert_eq!(d.spec.hostnames.len(), 2);
        assert_eq!(d.spec.hostnames[0].app, "api");
        assert_eq!(d.spec.hostnames[0].instance.as_deref(), Some("demo-prod"));
        assert_eq!(d.spec.backend.service, "demo-app-gateway");
        assert_eq!(d.spec.backend.port, 8000);
        assert!(d.spec.stable_name_claim);
        assert_eq!(d.spec.priority, 100);
    }

    #[test]
    fn lisp_round_trip_anonymous_instance() {
        // `:instance` omitted ⇒ content-hash form (filled in by the
        // hostname helper, not stored). Round-trip via Lisp +
        // serde proves the Option<String> default flows cleanly.
        let src = r#"
            (defrouting smoke-edges
              :hostnames ((:app "smoke"))
              :backend   (:service "smoke" :port 80))
        "#;
        let defs: Vec<tatara_lisp::NamedDefinition<RoutingSpec>> =
            tatara_lisp::compile_named::<RoutingSpec>(src).expect("compile");
        let d = &defs[0];
        assert_eq!(d.spec.hostnames.len(), 1);
        assert_eq!(d.spec.hostnames[0].instance, None);
        assert!(!d.spec.stable_name_claim); // default false
        assert_eq!(d.spec.priority, 0); // default 0
    }

    // ─── RoutingForm substrate pins ───────────────────────────────
    //
    // The pre-lift `tatara-reconciler::edges` sites hand-wrote
    // three `if ctx.is_stable { "stable" } else { "instance" }`
    // ternaries at every axis (Ingress annotation, Ingress label,
    // DNSEndpoint label) plus two byte-literal reads in render
    // tests. Every byte the ternary + literals produced is pinned
    // here so a rename of a `RoutingForm::as_str` arm surfaces at
    // THIS composer's shipped-shape pin rather than as silent
    // drift between the pre-lift edge sites (which pre-lift had
    // already grown five copies of the same two-literal set).

    #[test]
    fn routing_form_as_str_matches_wire_form_pre_lift() {
        // Byte-identity pin: the pre-lift ternary at
        // `edges.rs::IngressEdge::render`,
        // `edges.rs::DnsEndpointEdge::render` restated these two
        // literals verbatim. A rename here is an
        // operator-visible selector-mismatch after apply.
        assert_eq!(RoutingForm::Stable.as_str(), "stable");
        assert_eq!(RoutingForm::Instance.as_str(), "instance");
    }

    #[test]
    fn routing_form_from_is_stable_routes_true_and_false() {
        // Boolean → enum decision pinned here rather than restated
        // as an inline ternary at every callsite.
        assert_eq!(RoutingForm::from_is_stable(true), RoutingForm::Stable);
        assert_eq!(RoutingForm::from_is_stable(false), RoutingForm::Instance);
    }

    #[test]
    fn routing_form_round_trip_via_bool() {
        // The decision the reconciler's `EdgeContext::is_stable`
        // bool encodes is a two-variant disjunction; round-trip
        // both bool values through the enum to prove the composer
        // preserves the axis in both directions.
        for is_stable in [true, false] {
            let form = RoutingForm::from_is_stable(is_stable);
            let expected = if is_stable { "stable" } else { "instance" };
            assert_eq!(form.as_str(), expected);
        }
    }

    #[test]
    fn routing_form_annotation_key_is_prefixed_process_ns() {
        // Byte-shape pin against the pre-lift string literal
        // `edges.rs` restated four times (two annotation branches
        // + two label sites). A rename that missed one of the
        // pre-lift sites would silently split the axis across two
        // K8s label keys — the const now closes that drift path.
        assert_eq!(
            crate::annotations::ROUTING_FORM,
            "tatara.pleme.io/routing-form"
        );
    }

    #[test]
    fn routing_app_annotation_key_is_prefixed_process_ns() {
        // Peer to `ROUTING_FORM`: pre-lift restated at the two
        // `edges.rs` label sites (Ingress + DNSEndpoint).
        assert_eq!(crate::annotations::APP, "tatara.pleme.io/app");
    }

    #[test]
    fn ingress_annotations_round_trip() {
        let mut annotations = BTreeMap::new();
        annotations.insert(
            "nginx.ingress.kubernetes.io/rate-limit".into(),
            "100".into(),
        );
        annotations.insert(
            "nginx.ingress.kubernetes.io/proxy-body-size".into(),
            "10m".into(),
        );
        let r = RoutingSpec {
            hostnames: vec![RoutingHostname {
                app: "api".into(),
                instance: None,
                cluster: None,
            }],
            backend: RoutingBackend {
                service: "svc".into(),
                port: 8080,
                tls_issuer: Some("letsencrypt-prod".into()),
                ingress_annotations: annotations,
            },
            stable_name_claim: false,
            priority: 0,
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("tlsIssuer: letsencrypt-prod"));
        assert!(yaml.contains("rate-limit"));
        let back: RoutingSpec = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.backend.tls_issuer.as_deref(), Some("letsencrypt-prod"));
        assert_eq!(back.backend.ingress_annotations.len(), 2);
    }
}
