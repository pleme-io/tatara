//! Routing edge renderers — the `Edge` trait + per-target impls.
//!
//! The substrate move: every external edge a Process exposes (DNS
//! record, Ingress entry, future Cloudflare Route record, future
//! mTLS-only Service binding) is rendered through one `Edge` trait
//! with a typed JSON output. The reconciler's `render_routing`
//! iterates declared hostnames + dispatches each through all
//! registered `Edge` impls. Adding a new edge target (e.g. a
//! Cloudflare API CR) means one new impl + one registration; no
//! changes to the dispatch loop.
//!
//! Currently two impls ship:
//!
//! * [`IngressEdge`] — emits `networking.k8s.io/v1` Ingress
//!   matching the FQDN, backed by the Process's declared Service.
//! * [`DnsEndpointEdge`] — emits `externaldns.k8s.io/v1alpha1`
//!   DNSEndpoint, which external-dns picks up to write the actual
//!   record into the operator's chosen DNS provider.
//!
//! Both are pure functions of `(EdgeContext, fqdn)` — no kube calls,
//! no clock. The caller (`render_routing`) SSA-applies the resulting
//! `Value`s.

use anyhow::Result;
use serde_json::{json, Value};

use tatara_process::routing::{RoutingBackend, RoutingHostname};

/// Per-render context the edges share. The reconciler builds this
/// once at the start of `render_routing` so each edge sees the same
/// owner refs, process metadata, and namespace.
#[derive(Clone, Debug)]
pub struct EdgeContext<'a> {
    /// Owning Process's `metadata.name`.
    pub process_name: &'a str,
    /// Owning Process's `metadata.namespace`.
    pub process_namespace: &'a str,
    /// Owning Process's `metadata.uid` (empty when fixturing tests).
    pub process_uid: &'a str,
    /// `${ns}/${name}` label value used for the
    /// `tatara.pleme.io/process` annotation.
    pub process_ref: &'a str,
    /// Hostname entry being rendered.
    pub hostname: &'a RoutingHostname,
    /// Resolved `${ephemeral_id}` segment (either named or
    /// content-hash; the resolver lives in
    /// `tatara_process::hostname::resolve_ephemeral_id`).
    pub ephemeral_id: &'a str,
    /// Backend Service + port + TLS hints.
    pub backend: &'a RoutingBackend,
    /// Resolved FQDN — `${app}.${ephemeral_id}.${cluster}.${loc}.${domain}`
    /// for per-instance, `${app}.${cluster}.${loc}.${domain}` for
    /// stable-claim.
    pub fqdn: &'a str,
    /// Whether this entry is the stable-claim form (drives Ingress
    /// name uniqueness + DNSEndpoint record name).
    pub is_stable: bool,
}

/// One typed edge renderer. Pure function of `EdgeContext`.
///
/// Implementations return `Ok(Some(value))` to emit a single K8s
/// resource (`Vec<Value>` in the caller); `Ok(None)` to opt out for
/// this specific FQDN; `Err` to fail the whole render.
pub trait Edge {
    /// Short identifier for logging + per-edge labels. Stable across
    /// reconciler versions.
    fn kind(&self) -> &'static str;

    /// Render the typed resource for this edge.
    fn render(&self, ctx: &EdgeContext<'_>) -> Result<Option<Value>>;
}

// ─── IngressEdge ───────────────────────────────────────────────────

/// Emit a `networking.k8s.io/v1` Ingress matching the FQDN, backed
/// by `ctx.backend`.
pub struct IngressEdge;

impl IngressEdge {
    /// Compose the resource-name suffix. Per-instance ⇒
    /// `<process>-<app>-<eph_id>`; stable ⇒ `<process>-<app>-stable`.
    fn name(ctx: &EdgeContext<'_>) -> String {
        if ctx.is_stable {
            format!("{}-{}-stable", ctx.process_name, ctx.hostname.app)
        } else {
            format!(
                "{}-{}-{}",
                ctx.process_name, ctx.hostname.app, ctx.ephemeral_id
            )
        }
    }
}

impl Edge for IngressEdge {
    fn kind(&self) -> &'static str {
        "Ingress"
    }

    fn render(&self, ctx: &EdgeContext<'_>) -> Result<Option<Value>> {
        // Seed the annotations map through the shared substrate primitive
        // owning the 2-slot `{MANAGED_BY, PROCESS}` ownership tag; then
        // extend with routing-form + backend + cert-manager annotations.
        let mut annotations_map = crate::ssapply::ownership_annotations(ctx.process_ref);
        if ctx.is_stable {
            annotations_map.insert(
                "tatara.pleme.io/routing-form".to_string(),
                Value::String("stable".into()),
            );
        } else {
            annotations_map.insert(
                "tatara.pleme.io/routing-form".to_string(),
                Value::String("instance".into()),
            );
        }
        for (k, v) in &ctx.backend.ingress_annotations {
            annotations_map.insert(k.clone(), Value::String(v.clone()));
        }
        let issuer = ctx
            .backend
            .tls_issuer
            .as_deref()
            .unwrap_or("letsencrypt-prod");
        annotations_map.insert(
            "cert-manager.io/cluster-issuer".to_string(),
            Value::String(issuer.to_string()),
        );

        let owner_refs = build_owner_refs(ctx);

        // Seed the labels map through the shared substrate primitive
        // owning the 2-slot `{MANAGED_BY, PROCESS}` ownership tag on
        // the labels axis (peer to `ownership_annotations` used above
        // for the annotations axis); then extend with app +
        // routing-form labels. Post-lift the labels ownership pair
        // reads `FIELD_MANAGER` rather than the hand-coded
        // `"tatara-reconciler"` literal, closing the same drift-risk
        // sweep the annotations-axis primitive already closed.
        let mut labels_map = crate::ssapply::ownership_labels(ctx.process_ref);
        labels_map.insert(
            "tatara.pleme.io/app".to_string(),
            Value::String(ctx.hostname.app.clone()),
        );
        labels_map.insert(
            "tatara.pleme.io/routing-form".to_string(),
            Value::String(if ctx.is_stable { "stable" } else { "instance" }.to_string()),
        );

        let ingress = json!({
            "apiVersion": "networking.k8s.io/v1",
            "kind": "Ingress",
            "metadata": {
                "name": Self::name(ctx),
                "namespace": ctx.process_namespace,
                "labels": Value::Object(labels_map),
                "annotations": Value::Object(annotations_map),
                "ownerReferences": owner_refs,
            },
            "spec": {
                "ingressClassName": "nginx",
                "tls": [{
                    "hosts": [ctx.fqdn],
                    "secretName": format!("{}-tls", Self::name(ctx)),
                }],
                "rules": [{
                    "host": ctx.fqdn,
                    "http": {
                        "paths": [{
                            "path": "/",
                            "pathType": "Prefix",
                            "backend": {
                                "service": {
                                    "name": ctx.backend.service,
                                    "port": { "number": ctx.backend.port as i64 },
                                }
                            }
                        }]
                    }
                }]
            }
        });
        Ok(Some(ingress))
    }
}

// ─── DnsEndpointEdge ───────────────────────────────────────────────

/// Emit `externaldns.k8s.io/v1alpha1` DNSEndpoint for the FQDN.
/// external-dns picks it up + writes the actual record to the
/// configured provider (Cloudflare / Route53 / etc.).
///
/// Resolves to a CNAME pointing at the cluster's ingress
/// loadbalancer hostname (operator-provisioned, supplied to the
/// reconciler via `EdgeContext::ingress_lb_target`). When the
/// loadbalancer is unknown the DNSEndpoint is omitted (external-dns
/// would have nothing to point at anyway).
pub struct DnsEndpointEdge {
    /// Hostname/CNAME target every emitted record points at. E.g.
    /// `pleme-dev.use1.quero.lol` or `<lb>.elb.amazonaws.com`.
    /// `None` ⇒ skip DNS emission for now (Ingress still emits).
    pub ingress_lb_target: Option<String>,
    /// Record TTL in seconds. Default 60.
    pub ttl_seconds: u32,
}

impl Default for DnsEndpointEdge {
    fn default() -> Self {
        Self {
            ingress_lb_target: None,
            ttl_seconds: 60,
        }
    }
}

impl DnsEndpointEdge {
    fn name(ctx: &EdgeContext<'_>) -> String {
        if ctx.is_stable {
            format!("{}-{}-stable-dns", ctx.process_name, ctx.hostname.app)
        } else {
            format!(
                "{}-{}-{}-dns",
                ctx.process_name, ctx.hostname.app, ctx.ephemeral_id
            )
        }
    }
}

impl Edge for DnsEndpointEdge {
    fn kind(&self) -> &'static str {
        "DNSEndpoint"
    }

    fn render(&self, ctx: &EdgeContext<'_>) -> Result<Option<Value>> {
        let target = match &self.ingress_lb_target {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        // Peer to the IngressEdge labels seed above and to the
        // annotations-axis feed on the next line: the labels ownership
        // pair rides through the shared substrate primitive so both
        // axes on this resource are held in lockstep by construction.
        let mut labels_map = crate::ssapply::ownership_labels(ctx.process_ref);
        labels_map.insert(
            "tatara.pleme.io/app".to_string(),
            Value::String(ctx.hostname.app.clone()),
        );
        labels_map.insert(
            "tatara.pleme.io/routing-form".to_string(),
            Value::String(if ctx.is_stable { "stable" } else { "instance" }.to_string()),
        );

        let endpoint = json!({
            "apiVersion": "externaldns.k8s.io/v1alpha1",
            "kind": "DNSEndpoint",
            "metadata": {
                "name": Self::name(ctx),
                "namespace": ctx.process_namespace,
                "labels": Value::Object(labels_map),
                "annotations": crate::ssapply::ownership_annotations(ctx.process_ref),
                "ownerReferences": build_owner_refs(ctx),
            },
            "spec": {
                "endpoints": [{
                    "dnsName": ctx.fqdn,
                    "recordType": "CNAME",
                    "recordTTL": self.ttl_seconds as i64,
                    "targets": [target],
                }]
            }
        });
        Ok(Some(endpoint))
    }
}

// ─── Shared helpers ────────────────────────────────────────────────

fn build_owner_refs(ctx: &EdgeContext<'_>) -> Vec<Value> {
    if ctx.process_uid.is_empty() {
        return vec![];
    }
    vec![tatara_process::owner_reference_json(
        ctx.process_name,
        ctx.process_uid,
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn ctx<'a>(
        hostname: &'a RoutingHostname,
        backend: &'a RoutingBackend,
        fqdn: &'a str,
        ephemeral_id: &'a str,
        is_stable: bool,
    ) -> EdgeContext<'a> {
        EdgeContext {
            process_name: "demo-prod",
            process_namespace: "demo-ns",
            process_uid: "uid-abc",
            process_ref: "demo-ns/demo-prod",
            hostname,
            ephemeral_id,
            backend,
            fqdn,
            is_stable,
        }
    }

    fn api_hostname() -> RoutingHostname {
        RoutingHostname {
            app: "api".into(),
            instance: Some("demo-prod".into()),
            cluster: None,
        }
    }

    fn api_backend() -> RoutingBackend {
        RoutingBackend {
            service: "demo-app-gateway".into(),
            port: 8000,
            tls_issuer: None,
            ingress_annotations: BTreeMap::new(),
        }
    }

    #[test]
    fn ingress_per_instance() {
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(
            &h,
            &b,
            "api.demo-prod.pleme-dev.use1.quero.lol",
            "demo-prod",
            false,
        );
        let r = IngressEdge.render(&c).unwrap().unwrap();
        assert_eq!(r["apiVersion"], "networking.k8s.io/v1");
        assert_eq!(r["kind"], "Ingress");
        assert_eq!(r["metadata"]["name"], "demo-prod-api-demo-prod");
        assert_eq!(r["metadata"]["namespace"], "demo-ns");
        assert_eq!(
            r["spec"]["rules"][0]["host"],
            "api.demo-prod.pleme-dev.use1.quero.lol"
        );
        assert_eq!(
            r["spec"]["rules"][0]["http"]["paths"][0]["backend"]["service"]["name"],
            "demo-app-gateway"
        );
        assert_eq!(
            r["spec"]["rules"][0]["http"]["paths"][0]["backend"]["service"]["port"]["number"],
            8000
        );
        // OwnerRef points at the Process.
        assert_eq!(r["metadata"]["ownerReferences"][0]["kind"], "Process");
        assert_eq!(r["metadata"]["ownerReferences"][0]["name"], "demo-prod");
        // TLS issuer defaulted.
        assert_eq!(
            r["metadata"]["annotations"]["cert-manager.io/cluster-issuer"],
            "letsencrypt-prod"
        );
        // routing-form annotation present.
        assert_eq!(
            r["metadata"]["annotations"]["tatara.pleme.io/routing-form"],
            "instance"
        );
    }

    #[test]
    fn ingress_stable_form_uses_stable_name() {
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(&h, &b, "api.pleme-dev.use1.quero.lol", "demo-prod", true);
        let r = IngressEdge.render(&c).unwrap().unwrap();
        assert_eq!(r["metadata"]["name"], "demo-prod-api-stable");
        assert_eq!(
            r["spec"]["rules"][0]["host"],
            "api.pleme-dev.use1.quero.lol"
        );
        assert_eq!(
            r["metadata"]["annotations"]["tatara.pleme.io/routing-form"],
            "stable"
        );
    }

    #[test]
    fn ingress_carries_custom_annotations() {
        let h = api_hostname();
        let mut anns = BTreeMap::new();
        anns.insert(
            "nginx.ingress.kubernetes.io/rate-limit".into(),
            "100".into(),
        );
        let b = RoutingBackend {
            service: "svc".into(),
            port: 80,
            tls_issuer: Some("custom-issuer".into()),
            ingress_annotations: anns,
        };
        let c = ctx(&h, &b, "host.example.com", "demo-prod", false);
        let r = IngressEdge.render(&c).unwrap().unwrap();
        assert_eq!(
            r["metadata"]["annotations"]["nginx.ingress.kubernetes.io/rate-limit"],
            "100"
        );
        assert_eq!(
            r["metadata"]["annotations"]["cert-manager.io/cluster-issuer"],
            "custom-issuer"
        );
    }

    #[test]
    fn dns_endpoint_emits_when_lb_target_set() {
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(
            &h,
            &b,
            "api.demo-prod.pleme-dev.use1.quero.lol",
            "demo-prod",
            false,
        );
        let edge = DnsEndpointEdge {
            ingress_lb_target: Some("pleme-dev.use1.quero.lol".into()),
            ttl_seconds: 30,
        };
        let r = edge.render(&c).unwrap().unwrap();
        assert_eq!(r["apiVersion"], "externaldns.k8s.io/v1alpha1");
        assert_eq!(r["kind"], "DNSEndpoint");
        assert_eq!(r["metadata"]["name"], "demo-prod-api-demo-prod-dns");
        assert_eq!(
            r["spec"]["endpoints"][0]["dnsName"],
            "api.demo-prod.pleme-dev.use1.quero.lol"
        );
        assert_eq!(r["spec"]["endpoints"][0]["recordType"], "CNAME");
        assert_eq!(
            r["spec"]["endpoints"][0]["targets"][0],
            "pleme-dev.use1.quero.lol"
        );
        assert_eq!(r["spec"]["endpoints"][0]["recordTTL"], 30);
    }

    #[test]
    fn dns_endpoint_skips_when_no_lb_target() {
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(&h, &b, "host", "demo-prod", false);
        let edge = DnsEndpointEdge {
            ingress_lb_target: None,
            ..DnsEndpointEdge::default()
        };
        assert!(edge.render(&c).unwrap().is_none());
    }

    #[test]
    fn owner_refs_skipped_when_uid_empty() {
        let h = api_hostname();
        let b = api_backend();
        let mut c = ctx(&h, &b, "host", "demo-prod", false);
        c.process_uid = "";
        let r = IngressEdge.render(&c).unwrap().unwrap();
        let owners = r["metadata"]["ownerReferences"].as_array().unwrap();
        assert!(owners.is_empty());
    }

    /// The Edge trait is dyn-compatible — confirm by storing impls
    /// behind a trait object. The reconciler's render loop iterates
    /// `&[Box<dyn Edge>]` so this property is load-bearing.
    #[test]
    fn edge_trait_object_is_dyn_compatible() {
        let edges: Vec<Box<dyn Edge>> = vec![
            Box::new(IngressEdge),
            Box::new(DnsEndpointEdge {
                ingress_lb_target: Some("lb".into()),
                ttl_seconds: 60,
            }),
        ];
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].kind(), "Ingress");
        assert_eq!(edges[1].kind(), "DNSEndpoint");
    }
}
