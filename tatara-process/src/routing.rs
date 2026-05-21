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
//! :routing (:hostnames ((:app "gator" :instance "akeyless-prod")
//!                       (:app "gateway"))
//!           :backend   (:service "akeyless-saas-akeyless-gateway"
//!                       :port    8000)
//!           :stable-name-claim #t
//!           :priority           100)
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

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
    /// Application slot — `api`, `gator`, `gateway`, `web`, etc.
    /// Must be a valid DNS label (RFC 1123): lowercase alpha-num
    /// + hyphen, 1–63 chars, no leading/trailing hyphen. The
    /// reconciler validates this at the boundary.
    pub app: String,

    /// Named instance segment. When `Some("akeyless-prod")` the FQDN
    /// reads `${app}.akeyless-prod.${cluster}.…`. When `None` the
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

#[cfg(test)]
mod tests {
    use super::*;

    fn akeyless_routing() -> RoutingSpec {
        RoutingSpec {
            hostnames: vec![
                RoutingHostname {
                    app: "gator".into(),
                    instance: Some("akeyless-prod".into()),
                    cluster: None,
                },
                RoutingHostname {
                    app: "gateway".into(),
                    instance: Some("akeyless-prod".into()),
                    cluster: None,
                },
            ],
            backend: RoutingBackend {
                service: "akeyless-saas-akeyless-gateway".into(),
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
        let r = akeyless_routing();
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
        let r = akeyless_routing();
        let yaml = serde_yaml::to_string(&r).unwrap();
        // camelCase wire form — what FluxCD / kubectl users see.
        assert!(yaml.contains("hostnames:"));
        assert!(yaml.contains("app: gator"));
        assert!(yaml.contains("instance: akeyless-prod"));
        assert!(yaml.contains("backend:"));
        assert!(yaml.contains("service: akeyless-saas-akeyless-gateway"));
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
            (defrouting akeyless-edges
              :hostnames ((:app "gator"   :instance "akeyless-prod")
                          (:app "gateway" :instance "akeyless-prod"))
              :backend   (:service "akeyless-saas-akeyless-gateway"
                          :port 8000)
              :stable-name-claim #t
              :priority 100)
        "#;
        let defs: Vec<tatara_lisp::NamedDefinition<RoutingSpec>> =
            tatara_lisp::compile_named::<RoutingSpec>(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        assert_eq!(d.name, "akeyless-edges");
        assert_eq!(d.spec.hostnames.len(), 2);
        assert_eq!(d.spec.hostnames[0].app, "gator");
        assert_eq!(
            d.spec.hostnames[0].instance.as_deref(),
            Some("akeyless-prod")
        );
        assert_eq!(d.spec.backend.service, "akeyless-saas-akeyless-gateway");
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
        assert_eq!(
            back.backend.tls_issuer.as_deref(),
            Some("letsencrypt-prod")
        );
        assert_eq!(back.backend.ingress_annotations.len(), 2);
    }
}
