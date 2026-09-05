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

use tatara_process::annotations;
use tatara_process::routing::{RoutingBackend, RoutingForm, RoutingHostname};
use tatara_process::routing_edge_resource::RoutingEdgeResource;

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

// ─── Shared edge composers ─────────────────────────────────────────

/// Substrate-primitive builder for the standard tatara-reconciler
/// **routing-edge `metadata.labels` map** — the 4-slot
/// `{MANAGED_BY, PROCESS, APP, ROUTING_FORM}` shape every emitted
/// routing edge (Ingress + DNSEndpoint) stamps on itself so operator
/// kubectl-side selectors (`kubectl get -l tatara.pleme.io/app=api,
/// tatara.pleme.io/routing-form=stable`) reach every edge for a
/// given `(app, form)` pair in ONE query regardless of edge kind.
/// Seeds the two-slot ownership pair through the shared substrate
/// primitive [`crate::ssapply::ownership_labels`] then extends with
/// the two routing-axis labels ([`annotations::APP`] +
/// [`annotations::ROUTING_FORM`]) that specialize the ownership
/// tag to a routing edge.
///
/// Pre-lift the 5-line label-map composition was hand-authored at
/// TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold,
/// each restating the same seed + APP-insert + ROUTING_FORM-insert
/// (with the `RoutingForm::from_is_stable(ctx.is_stable).as_str()`
/// call composed inline three levels deep):
/// * [`IngressEdge::render`] — the Ingress `metadata.labels` seed
///   before the `json!({...})` metadata composition.
/// * [`DnsEndpointEdge::render`] — the DNSEndpoint `metadata.labels`
///   seed before the `json!({...})` metadata composition, byte-
///   identical to the Ingress version.
///
/// Post-lift both edges route through this ONE composer so a future
/// edge kind (a Gateway API `HTTPRoute`, a `NetworkPolicy` edge, a
/// Cloudflare API CR) sourcing the same label pair inherits the
/// upgrade mechanically — no per-site hand-edit at the new render
/// impl. The composer takes the whole [`EdgeContext`] rather than
/// unpacked `(process_ref, app, form)` args because the trait's
/// input IS the context, so every current + future [`Edge`] impl has
/// exactly that shape in hand at the render site.
///
/// A future addition (e.g. a `HOSTNAME` slot naming the resolved
/// FQDN, an `EDGE_KIND` slot letting selectors slice by
/// Ingress-vs-DNSEndpoint on the same label axis, a `PRIORITY` slot
/// carrying [`RoutingSpec::priority`][tatara_process::routing::RoutingSpec::priority]
/// through to the labels axis for claim-arbitration greps) lands at
/// this ONE substrate function and every edge kind inherits the
/// upgrade mechanically.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the 4-slot routing-edge labels shape recurred at two hand-
/// authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger, and is lifted to ONE owner here). THEORY.md §II.1
/// invariant 5 (composition preserves proofs — the pins in
/// [`tests::routing_edge_labels_*`] bind the composed primitive to
/// the pre-lift hand-authored shape byte-identically, so a
/// regression that drifted the seed order, the APP slot value, or
/// the ROUTING_FORM `as_str` byte-shape surfaces here rather than
/// at every downstream edge kind).
pub(crate) fn routing_edge_labels(ctx: &EdgeContext<'_>) -> serde_json::Map<String, Value> {
    let mut labels = crate::ssapply::ownership_labels(ctx.process_ref);
    labels.insert(
        annotations::APP.to_string(),
        Value::String(ctx.hostname.app.clone()),
    );
    labels.insert(
        annotations::ROUTING_FORM.to_string(),
        Value::String(
            RoutingForm::from_is_stable(ctx.is_stable)
                .as_str()
                .to_string(),
        ),
    );
    labels
}

/// Substrate-primitive composer for a routing-edge **`metadata` block**
/// — owns the 5-slot `{name, namespace, labels, annotations,
/// ownerReferences}` shape every routing edge (Ingress + DNSEndpoint)
/// stamps on the resource it emits.
///
/// The `name` argument is caller-supplied (composed via
/// [`routing_edge_resource_name`] per-edge with its suffix); the
/// labels axis routes through [`routing_edge_labels`] and the
/// ownerReferences axis through [`build_owner_refs`] so a change to
/// either shape lands at ONE substrate primitive rather than at every
/// [`Edge`] impl. The `annotations` argument is caller-supplied so
/// each edge can extend the ownership seed with edge-specific keys
/// ([`IngressEdge`] adds `ROUTING_FORM` + backend annotations +
/// `cert-manager.io/cluster-issuer`; [`DnsEndpointEdge`] uses the
/// bare [`crate::ssapply::ownership_annotations`] tag).
///
/// Pre-lift the 5-slot metadata block was hand-authored at TWO sites
/// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold, each
/// restating the same `{ name, namespace, labels, annotations,
/// ownerReferences }` json! literal byte-for-byte, differing only in
/// the annotations-slot payload:
/// * [`IngressEdge::render`] — the Ingress `metadata` composition,
///   annotations = the extended map (ownership + ROUTING_FORM +
///   backend + cert-manager).
/// * [`DnsEndpointEdge::render`] — the DNSEndpoint `metadata`
///   composition, annotations = the bare ownership map.
///
/// Post-lift both edges compose their `metadata` through this ONE
/// primitive so a future edge kind (a Gateway API `HTTPRoute`, a
/// `NetworkPolicy` edge, a Cloudflare API CR) sourcing the same
/// 5-slot metadata shape inherits the upgrade mechanically — no
/// per-site restatement of the four shared slots + no risk of the
/// new edge kind silently omitting the labels or ownerReferences
/// axis (a pre-lift trap: a copy-pasted `metadata` block that
/// forgot to include `labels` would drop the sibling routing-edge
/// selectors on the new edge kind without a compile-time signal).
/// A future addition to the metadata shape (a `generateName` slot
/// for duplicate-safe emits, a `finalizers` slot for cascade-delete
/// gating, a `resourceVersion` precondition slot for optimistic
/// concurrency) lands at this ONE substrate function and every
/// routing edge inherits the upgrade mechanically.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the 5-slot metadata shape recurred at two hand-authored sites
/// past the ★★ PRIME-DIRECTIVE trigger, and is lifted to ONE owner
/// here). THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — the pins in [`tests::routing_edge_metadata_*`] bind the
/// composed primitive to the pre-lift hand-authored shape
/// byte-identically, so a regression that drifted the slot naming
/// or reshaped one of the four shared slots surfaces here rather
/// than at every downstream edge kind).
pub(crate) fn routing_edge_metadata(
    ctx: &EdgeContext<'_>,
    name: String,
    annotations: serde_json::Map<String, Value>,
) -> Value {
    let labels_map = routing_edge_labels(ctx);
    let owner_refs = build_owner_refs(ctx);
    json!({
        "name": name,
        "namespace": ctx.process_namespace,
        "labels": Value::Object(labels_map),
        "annotations": Value::Object(annotations),
        "ownerReferences": owner_refs,
    })
}

/// Substrate-primitive composer for a routing-edge **`metadata.name`**
/// — owns the `<process>-<app>-<form>[-<suffix>]` shape every routing
/// edge (Ingress + DNSEndpoint) stamps on the resource it emits.
///
/// The `<form>` segment routes through the same `is_stable` axis every
/// other routing-edge composer keys on: `"stable"` when
/// `ctx.is_stable`, `ctx.ephemeral_id` otherwise. The per-form dispatch
/// rides through ONE composer so a regression that swapped the branch
/// — e.g. emitted the ephemeral-id segment for the stable form —
/// surfaces at this ONE primitive's tests, not as apply-time name
/// collisions where two edge kinds pick different names for the same
/// `(Process, hostname)` pair.
///
/// The `<suffix>` segment disambiguates edges that share a
/// `(process, app, form)` tuple (e.g. DNSEndpoint's `-dns` tail vs
/// Ingress's bare form). Pass `""` to omit the suffix entirely; the
/// composer skips the trailing `-` so an omitted suffix does not
/// leave a dangling separator (which would break DNS-1123 label
/// validity + downstream selector matching).
///
/// Pre-lift the 5-line `if ctx.is_stable { format!("{}-{}-stable",
/// ctx.process_name, ctx.hostname.app) } else { format!("{}-{}-{}",
/// ctx.process_name, ctx.hostname.app, ctx.ephemeral_id) }` block was
/// hand-authored at TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold:
///
/// * [`IngressEdge::name`] — no suffix; `metadata.name` shape is
///   `<process>-<app>-<stable|eph>`.
/// * [`DnsEndpointEdge::name`] — `-dns` suffix; `metadata.name`
///   shape is `<process>-<app>-<stable|eph>-dns`, appending a
///   `-dns` segment to distinguish the DNS record edge from the
///   Ingress edge for the same hostname.
///
/// Post-lift both callsites read `routing_edge_resource_name(ctx,
/// "")` and `routing_edge_resource_name(ctx, "dns")`. A future edge
/// kind (a Gateway API `HTTPRoute` with `-route` suffix, a
/// `NetworkPolicy` edge with `-np` suffix, a Cloudflare API CR with
/// `-cf` suffix) sourcing the same per-form naming shape inherits
/// the upgrade mechanically — no per-site restatement of the
/// `is_stable` branch, and no risk of the new edge kind silently
/// diverging on whether the stable form uses `"stable"` or something
/// else.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the routing-edge name shape recurred at two hand-authored sites
/// past the ★★ PRIME-DIRECTIVE trigger, and is lifted to ONE owner
/// here). THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — the pins in [`tests::routing_edge_resource_name_*`]
/// bind the composed primitive to the pre-lift hand-authored shape
/// byte-identically, and the cross-edge lockstep pin binds both
/// edge kinds' names to the same shared stem).
pub(crate) fn routing_edge_resource_name(ctx: &EdgeContext<'_>, suffix: &str) -> String {
    let form = if ctx.is_stable {
        "stable"
    } else {
        ctx.ephemeral_id
    };
    if suffix.is_empty() {
        format!("{}-{}-{}", ctx.process_name, ctx.hostname.app, form)
    } else {
        format!(
            "{}-{}-{}-{}",
            ctx.process_name, ctx.hostname.app, form, suffix
        )
    }
}

// ─── IngressEdge ───────────────────────────────────────────────────

/// Emit a `networking.k8s.io/v1` Ingress matching the FQDN, backed
/// by `ctx.backend`.
pub struct IngressEdge;

impl IngressEdge {
    /// Compose the Ingress `metadata.name`. Per-instance ⇒
    /// `<process>-<app>-<eph_id>`; stable ⇒ `<process>-<app>-stable`.
    /// Routes through [`routing_edge_resource_name`] with an empty
    /// suffix so the per-form shape stays in lockstep with every
    /// other routing edge kind.
    fn name(ctx: &EdgeContext<'_>) -> String {
        routing_edge_resource_name(ctx, "")
    }
}

impl Edge for IngressEdge {
    fn kind(&self) -> &'static str {
        RoutingEdgeResource::Ingress.kind()
    }

    fn render(&self, ctx: &EdgeContext<'_>) -> Result<Option<Value>> {
        // Seed the annotations map through the shared substrate primitive
        // owning the 2-slot `{MANAGED_BY, PROCESS}` ownership tag; then
        // extend with routing-form + backend + cert-manager annotations.
        // The routing-form axis rides through ONE typed composer
        // (`RoutingForm::from_is_stable` + `as_str`) so the two pre-lift
        // branches collapse to a single insert whose value is decided by
        // the enum, not by an inline ternary restated per site.
        let mut annotations_map = crate::ssapply::ownership_annotations(ctx.process_ref);
        annotations_map.insert(
            annotations::ROUTING_FORM.to_string(),
            Value::String(
                RoutingForm::from_is_stable(ctx.is_stable)
                    .as_str()
                    .to_string(),
            ),
        );
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

        // Compose the 5-slot metadata block through the shared
        // substrate composer that owns the `{name, namespace, labels,
        // annotations, ownerReferences}` shape every routing edge
        // stamps — see [`routing_edge_metadata`] for the pre-lift
        // call-site inventory + the shared-shape invariants a future
        // edge kind (Gateway API HTTPRoute, NetworkPolicy edge,
        // Cloudflare API CR) automatically inherits by routing through
        // the same composer.
        // Routing-edge `(apiVersion, kind)` pairing rides through the
        // shared substrate composer
        // [`tatara_process::K8sWireIdentity::resource_json`] — pre-lift
        // this Ingress block was ONE of TWO hand-authored 2-slot
        // `{"apiVersion": RoutingEdgeResource::X.api_version(), "kind":
        // RoutingEdgeResource::X.kind()}` shapes past the ★★
        // PRIME-DIRECTIVE ≥ 2 duplication threshold that mentioned the
        // same variant twice per emit site (Ingress here +
        // DnsEndpoint below). Post-lift each emit site names the
        // variant ONCE via `.wire_identity()`; the pair binds
        // structurally at the typed
        // [`tatara_process::K8sWireIdentity`] so a copy-paste that
        // swapped ONE mention of the variant across the two slots
        // would no longer compile.
        let ingress = RoutingEdgeResource::Ingress
            .wire_identity()
            .resource_json(json!({
                "metadata": routing_edge_metadata(ctx, Self::name(ctx), annotations_map),
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
            }));
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
    /// Compose the DNSEndpoint `metadata.name`. Per-instance ⇒
    /// `<process>-<app>-<eph_id>-dns`; stable ⇒
    /// `<process>-<app>-stable-dns`. Routes through
    /// [`routing_edge_resource_name`] with a `"dns"` suffix so the
    /// per-form shape stays in lockstep with every other routing
    /// edge kind while the `-dns` tail disambiguates the DNS record
    /// edge from the Ingress edge for the same hostname.
    fn name(ctx: &EdgeContext<'_>) -> String {
        routing_edge_resource_name(ctx, "dns")
    }
}

impl Edge for DnsEndpointEdge {
    fn kind(&self) -> &'static str {
        RoutingEdgeResource::DnsEndpoint.kind()
    }

    fn render(&self, ctx: &EdgeContext<'_>) -> Result<Option<Value>> {
        let target = match &self.ingress_lb_target {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        // Peer to the IngressEdge metadata seed above: both edges
        // route through [`routing_edge_metadata`] so the 5-slot
        // `{name, namespace, labels, annotations, ownerReferences}`
        // shape stays in lockstep by construction. The DNSEndpoint
        // annotations slot carries the bare ownership tag via
        // [`crate::ssapply::ownership_annotations`] (no routing-form
        // / backend / cert-manager extension needed — external-dns
        // reads only the `spec.endpoints` block, not the
        // annotations).
        // Routing-edge `(apiVersion, kind)` pairing rides through the
        // shared substrate composer
        // [`tatara_process::K8sWireIdentity::resource_json`] — sibling
        // to the Ingress emit above. Pre-lift this DnsEndpoint block
        // was the SECOND of TWO hand-authored 2-slot
        // `{"apiVersion": RoutingEdgeResource::X.api_version(), "kind":
        // RoutingEdgeResource::X.kind()}` shapes that mentioned the
        // same variant twice; post-lift each emit site names the
        // variant ONCE via `.wire_identity()`.
        let endpoint = RoutingEdgeResource::DnsEndpoint
            .wire_identity()
            .resource_json(json!({
                "metadata": routing_edge_metadata(
                    ctx,
                    Self::name(ctx),
                    crate::ssapply::ownership_annotations(ctx.process_ref),
                ),
                "spec": {
                    "endpoints": [{
                        "dnsName": ctx.fqdn,
                        "recordType": "CNAME",
                        "recordTTL": self.ttl_seconds as i64,
                        "targets": [target],
                    }]
                }
            }));
        Ok(Some(endpoint))
    }
}

// ─── Shared helpers ────────────────────────────────────────────────

/// Compose the routing-edge `metadata.ownerReferences` slot for the
/// enclosing [`EdgeContext`]. Delegates to the shared substrate
/// composer [`tatara_process::owner_references_json`] which owns the
/// empty-uid gate + the [`tatara_process::owner_reference_json`]
/// scalar entry shape. Sibling on the routing-edge axis to
/// [`routing_edge_labels`] + [`routing_edge_resource_name`] + the
/// annotations-axis seeds [`crate::ssapply::ownership_annotations`] +
/// [`crate::ssapply::ownership_labels`] — every routing edge's four
/// metadata slots (`labels`, `annotations`, `name`, `ownerReferences`)
/// now compose through a single owner primitive apiece, so a change
/// to any one shape lands at ONE substrate function rather than at
/// every `Edge` impl.
fn build_owner_refs(ctx: &EdgeContext<'_>) -> Vec<Value> {
    tatara_process::owner_references_json(ctx.process_name, ctx.process_uid)
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
        RoutingHostname::instanced("api", "demo-prod")
    }

    fn api_backend() -> RoutingBackend {
        RoutingBackend::plain("demo-app-gateway", 8000)
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
        assert_eq!(r["apiVersion"], RoutingEdgeResource::Ingress.api_version());
        assert_eq!(r["kind"], RoutingEdgeResource::Ingress.kind());
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
            r["metadata"]["annotations"][annotations::ROUTING_FORM],
            RoutingForm::Instance.as_str()
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
            r["metadata"]["annotations"][annotations::ROUTING_FORM],
            RoutingForm::Stable.as_str()
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
        assert_eq!(
            r["apiVersion"],
            RoutingEdgeResource::DnsEndpoint.api_version()
        );
        assert_eq!(r["kind"], RoutingEdgeResource::DnsEndpoint.kind());
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
        assert_eq!(edges[0].kind(), RoutingEdgeResource::Ingress.kind());
        assert_eq!(edges[1].kind(), RoutingEdgeResource::DnsEndpoint.kind());
    }

    /// COHERENCE PIN: the `Edge::kind()` trait method return, the
    /// emitted JSON `kind` slot, and the emitted JSON `apiVersion`
    /// slot MUST all agree with the closed-set variant's typed
    /// projections. A regression that renamed the K8s Kind at one
    /// site but not the sibling `apiVersion` group-version bump (or
    /// vice versa) — a `HTTPRoute` migration that reached the trait
    /// method but not the emitter, or that bumped `networking.k8s.io/v1`
    /// to `v1beta2` at the JSON slot but left the trait return byte-
    /// stale — surfaces HERE at the three-way agreement pin rather
    /// than as silent trait-vs-emit skew at the reconciler wire.
    #[test]
    fn edge_kind_matches_closed_set_and_emitted_json() {
        // Ingress path — the emitted JSON's (apiVersion, kind) pair
        // and the trait's `kind()` return all reach through
        // `RoutingEdgeResource::Ingress`.
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(&h, &b, "host.example.com", "demo-prod", false);
        let r = IngressEdge.render(&c).unwrap().unwrap();
        assert_eq!(r["apiVersion"], RoutingEdgeResource::Ingress.api_version());
        assert_eq!(r["kind"], RoutingEdgeResource::Ingress.kind());
        assert_eq!(IngressEdge.kind(), RoutingEdgeResource::Ingress.kind());

        // DNSEndpoint path — same three-way agreement.
        let edge = DnsEndpointEdge {
            ingress_lb_target: Some("lb.example.com".into()),
            ttl_seconds: 60,
        };
        let r = edge.render(&c).unwrap().unwrap();
        assert_eq!(
            r["apiVersion"],
            RoutingEdgeResource::DnsEndpoint.api_version()
        );
        assert_eq!(r["kind"], RoutingEdgeResource::DnsEndpoint.kind());
        assert_eq!(edge.kind(), RoutingEdgeResource::DnsEndpoint.kind());
    }

    // ─── routing_edge_labels substrate pins ───────────────────────
    //
    // The pre-lift 5-line label-map composition was hand-authored at
    // TWO sites (`IngressEdge::render` + `DnsEndpointEdge::render`),
    // each restating the same ownership-seed + APP-insert +
    // ROUTING_FORM-insert. Every byte the pre-lift block produced
    // is pinned here so a regression that inlined any of the four
    // slots at a call site (breaking the primitive's role as the
    // ONE source of truth for the routing-edge labels shape) fails
    // HERE at the composer's shipped-shape pin rather than as
    // silent drift across the two edge kinds.

    #[test]
    fn routing_edge_labels_seeds_ownership_pair_bytewise() {
        // The composer starts from the ownership-labels seed —
        // pin the byte-shape so a regression that dropped the
        // seed call (open-coding the 4-slot literal) surfaces
        // HERE, not as silent detachment from the sibling
        // annotations-axis ownership tag.
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(&h, &b, "host", "demo-prod", false);
        let labels = routing_edge_labels(&c);
        let seed = crate::ssapply::ownership_labels(c.process_ref);
        for (k, v) in &seed {
            assert_eq!(
                labels.get(k),
                Some(v),
                "routing_edge_labels must include the ownership seed for key {k}"
            );
        }
    }

    #[test]
    fn routing_edge_labels_stamps_app_slot_from_hostname() {
        // The APP slot value comes from `ctx.hostname.app` verbatim
        // — pin against a distinctive value so a regression that
        // swapped it for `ctx.process_name` or `ctx.fqdn` surfaces
        // HERE.
        let h = RoutingHostname::instanced("gateway", "demo-prod");
        let b = api_backend();
        let c = ctx(&h, &b, "host", "demo-prod", false);
        let labels = routing_edge_labels(&c);
        assert_eq!(
            labels.get(annotations::APP),
            Some(&Value::String("gateway".to_string())),
        );
    }

    #[test]
    fn routing_edge_labels_stamps_routing_form_via_is_stable() {
        // The ROUTING_FORM slot rides through
        // `RoutingForm::from_is_stable(ctx.is_stable).as_str()` —
        // pin both bool paths so a regression that flipped the
        // is_stable branch surfaces HERE rather than as an
        // operator-visible selector-mismatch after apply.
        let h = api_hostname();
        let b = api_backend();
        for (is_stable, expected) in [(true, RoutingForm::Stable), (false, RoutingForm::Instance)] {
            let c = ctx(&h, &b, "host", "demo-prod", is_stable);
            let labels = routing_edge_labels(&c);
            assert_eq!(
                labels.get(annotations::ROUTING_FORM),
                Some(&Value::String(expected.as_str().to_string())),
                "routing_edge_labels must stamp ROUTING_FORM via RoutingForm::from_is_stable({is_stable})",
            );
        }
    }

    #[test]
    fn routing_edge_labels_carries_all_four_slots() {
        // The 4-slot pin — a regression that added a fifth key or
        // dropped one of the four here would silently reshape
        // every emitted routing edge's labels axis. Post-lift the
        // 4-slot shape is fixed at ONE composer; any future addition
        // (HOSTNAME, EDGE_KIND, PRIORITY, ...) lands here + this
        // pin adjusts once.
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(&h, &b, "host", "demo-prod", true);
        let labels = routing_edge_labels(&c);
        assert_eq!(labels.len(), 4);
        for k in [
            tatara_process::annotations::MANAGED_BY,
            tatara_process::annotations::PROCESS,
            annotations::APP,
            annotations::ROUTING_FORM,
        ] {
            assert!(
                labels.contains_key(k),
                "routing_edge_labels must contain slot: {k}"
            );
        }
    }

    #[test]
    fn routing_edge_labels_matches_hand_authored_pre_lift_bytewise() {
        // The exact 5-line hand-authored composition every pre-lift
        // callsite restated. A regression that reordered a slot,
        // dropped one, or added a fifth here surfaces at THIS pin
        // rather than as a subtle apply-time selector drift when
        // an operator's `kubectl get -l` misses the emitted edge.
        let h = api_hostname();
        let b = api_backend();
        for is_stable in [true, false] {
            let c = ctx(&h, &b, "host", "demo-prod", is_stable);
            let via_composer = routing_edge_labels(&c);

            // The pre-lift 5-line block, byte-for-byte.
            let mut hand_authored = crate::ssapply::ownership_labels(c.process_ref);
            hand_authored.insert(
                annotations::APP.to_string(),
                Value::String(c.hostname.app.clone()),
            );
            hand_authored.insert(
                annotations::ROUTING_FORM.to_string(),
                Value::String(
                    RoutingForm::from_is_stable(c.is_stable)
                        .as_str()
                        .to_string(),
                ),
            );

            assert_eq!(via_composer, hand_authored);
        }
    }

    // ─── routing_edge_resource_name substrate pins ───────────────
    //
    // The pre-lift 5-line `if ctx.is_stable { format! } else {
    // format! }` block was hand-authored at TWO sites
    // (`IngressEdge::name` + `DnsEndpointEdge::name`), each restating
    // the same `<process>-<app>-<stable|eph>` stem shape with a
    // per-edge suffix. Every byte the pre-lift block produced is
    // pinned here so a regression that inlined the branch at a
    // callsite (breaking the primitive's role as the ONE source of
    // truth for the routing-edge naming shape) fails HERE at the
    // composer's shipped-shape pin rather than as an apply-time name
    // collision where two edge kinds pick different names for the
    // same `(Process, hostname)` pair.

    #[test]
    fn routing_edge_resource_name_stable_form_no_suffix() {
        // Stable form + empty suffix produces `<process>-<app>-stable`.
        // Pins the omitted-suffix branch — a regression that emitted a
        // trailing `-` when the suffix is empty surfaces HERE (a
        // dangling `-` breaks DNS-1123 label validity so this pin is
        // load-bearing).
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(&h, &b, "host", "demo-prod", true);
        assert_eq!(routing_edge_resource_name(&c, ""), "demo-prod-api-stable");
    }

    #[test]
    fn routing_edge_resource_name_stable_form_with_suffix() {
        // Stable form + `"dns"` suffix produces
        // `<process>-<app>-stable-<suffix>`. Pins the suffix-appended
        // branch — a regression that dropped the interstitial `-`
        // (yielding `stabledns` rather than `stable-dns`) or the
        // suffix itself surfaces HERE.
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(&h, &b, "host", "demo-prod", true);
        assert_eq!(
            routing_edge_resource_name(&c, "dns"),
            "demo-prod-api-stable-dns"
        );
    }

    #[test]
    fn routing_edge_resource_name_per_instance_no_suffix() {
        // Per-instance form + empty suffix produces
        // `<process>-<app>-<eph_id>`. Pins the ephemeral-id branch —
        // a regression that swapped `ctx.ephemeral_id` for the string
        // `"instance"` (matching the RoutingForm annotation value)
        // surfaces HERE.
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(&h, &b, "host", "eph-xyz-42", false);
        assert_eq!(
            routing_edge_resource_name(&c, ""),
            "demo-prod-api-eph-xyz-42"
        );
    }

    #[test]
    fn routing_edge_resource_name_per_instance_with_suffix() {
        // Per-instance form + `"dns"` suffix produces
        // `<process>-<app>-<eph_id>-<suffix>`. Pins the combined
        // branch (the shape DNSEndpoint emits on every per-instance
        // FQDN).
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(&h, &b, "host", "eph-xyz-42", false);
        assert_eq!(
            routing_edge_resource_name(&c, "dns"),
            "demo-prod-api-eph-xyz-42-dns"
        );
    }

    #[test]
    fn routing_edge_resource_name_matches_hand_authored_pre_lift_bytewise() {
        // The exact 5-line hand-authored composition every pre-lift
        // callsite restated. A regression that reordered the segments,
        // dropped one, or added a fifth surfaces at THIS pin rather
        // than as a subtle apply-time drift when an operator's
        // `kubectl get ingress <name>` returns 404 because the
        // emitted name diverged from the composer's shape.
        let h = api_hostname();
        let b = api_backend();
        for is_stable in [true, false] {
            let c = ctx(&h, &b, "host", "demo-prod", is_stable);

            let ingress_pre_lift = if c.is_stable {
                format!("{}-{}-stable", c.process_name, c.hostname.app)
            } else {
                format!("{}-{}-{}", c.process_name, c.hostname.app, c.ephemeral_id)
            };
            assert_eq!(routing_edge_resource_name(&c, ""), ingress_pre_lift);

            let dns_pre_lift = if c.is_stable {
                format!("{}-{}-stable-dns", c.process_name, c.hostname.app)
            } else {
                format!(
                    "{}-{}-{}-dns",
                    c.process_name, c.hostname.app, c.ephemeral_id
                )
            };
            assert_eq!(routing_edge_resource_name(&c, "dns"), dns_pre_lift);
        }
    }

    #[test]
    fn ingress_and_dns_endpoint_names_share_stem() {
        // Cross-edge coherence pin: both edge kinds route through
        // `routing_edge_resource_name`, so `DnsEndpointEdge`'s name
        // must be exactly `<IngressEdge name>-dns` for the same
        // context. A regression that reshaped one caller in isolation
        // — e.g. dropped `ctx.ephemeral_id` from the per-instance
        // branch on one edge but not the other — surfaces HERE, not
        // as silent divergence where an operator's cross-edge scripts
        // (that pair Ingress with DNSEndpoint by stem) misalign.
        let h = api_hostname();
        let b = api_backend();
        for is_stable in [true, false] {
            let c = ctx(&h, &b, "host", "eph-xyz-42", is_stable);
            let ingress_name = IngressEdge::name(&c);
            let dns_name = DnsEndpointEdge::name(&c);
            assert_eq!(
                dns_name,
                format!("{ingress_name}-dns"),
                "DnsEndpointEdge::name must be `<IngressEdge::name>-dns` (is_stable={is_stable})",
            );
        }
    }

    // ─── routing_edge_metadata substrate pins ──────────────────────
    //
    // The pre-lift 5-slot `{name, namespace, labels, annotations,
    // ownerReferences}` metadata block was hand-authored at TWO sites
    // (`IngressEdge::render` + `DnsEndpointEdge::render`), each
    // restating the same shape with only the annotations-slot payload
    // differing. Every byte the pre-lift block produced is pinned
    // here so a regression that reshaped, reordered, dropped, or
    // added a slot at either callsite (breaking the primitive's role
    // as the ONE source of truth for the routing-edge metadata shape)
    // fails HERE at the composer's shipped-shape pin rather than as
    // silent drift across the two edge kinds.

    #[test]
    fn routing_edge_metadata_carries_all_five_slots() {
        // The 5-slot pin — a regression that added a sixth key or
        // dropped one of the five would silently reshape every
        // emitted routing edge's metadata block. Post-lift the 5-slot
        // shape is fixed at ONE composer; any future addition
        // (generateName, finalizers, resourceVersion, ...) lands here
        // + this pin adjusts once.
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(&h, &b, "host", "demo-prod", false);
        let annotations = crate::ssapply::ownership_annotations(c.process_ref);
        let md = routing_edge_metadata(&c, "my-name".to_string(), annotations);
        let obj = md.as_object().expect("metadata is a JSON object");
        assert_eq!(obj.len(), 5);
        for k in [
            "name",
            "namespace",
            "labels",
            "annotations",
            "ownerReferences",
        ] {
            assert!(
                obj.contains_key(k),
                "routing_edge_metadata must contain slot: {k}"
            );
        }
    }

    #[test]
    fn routing_edge_metadata_stamps_name_and_namespace_verbatim() {
        // The name arg rides through verbatim (caller composed it via
        // `routing_edge_resource_name`), and namespace comes from
        // `ctx.process_namespace`. A regression that swapped the two
        // (yielding `metadata.name == process_namespace` and
        // `metadata.namespace == name`) surfaces HERE rather than as
        // an apply-time 404 when kubectl looks the resource up by
        // name in the wrong namespace.
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(&h, &b, "host", "demo-prod", true);
        let md = routing_edge_metadata(
            &c,
            "explicit-name-42".to_string(),
            crate::ssapply::ownership_annotations(c.process_ref),
        );
        assert_eq!(md["name"], "explicit-name-42");
        assert_eq!(md["namespace"], "demo-ns");
    }

    #[test]
    fn routing_edge_metadata_seeds_labels_through_routing_edge_labels() {
        // The labels axis routes through `routing_edge_labels` — pin
        // that the composed metadata's labels slot is byte-identical
        // to what the shared labels composer returns. A regression
        // that re-open-coded the labels axis (silently un-lifting
        // through the shared owner) surfaces HERE.
        let h = api_hostname();
        let b = api_backend();
        for is_stable in [true, false] {
            let c = ctx(&h, &b, "host", "demo-prod", is_stable);
            let md = routing_edge_metadata(
                &c,
                "n".to_string(),
                crate::ssapply::ownership_annotations(c.process_ref),
            );
            let expected = Value::Object(routing_edge_labels(&c));
            assert_eq!(
                md["labels"], expected,
                "labels slot must delegate to routing_edge_labels (is_stable={is_stable})",
            );
        }
    }

    #[test]
    fn routing_edge_metadata_seeds_owner_refs_through_build_owner_refs() {
        // The ownerReferences axis routes through `build_owner_refs`
        // — pin that the composed metadata's ownerReferences slot is
        // byte-identical to what the shared builder returns. Cover
        // both the populated (uid present) and empty-uid (returns
        // empty vec) branches so a regression that dropped the
        // empty-uid gate surfaces HERE.
        let h = api_hostname();
        let b = api_backend();
        // Populated uid.
        let c = ctx(&h, &b, "host", "demo-prod", false);
        let md = routing_edge_metadata(
            &c,
            "n".to_string(),
            crate::ssapply::ownership_annotations(c.process_ref),
        );
        assert_eq!(
            md["ownerReferences"],
            serde_json::Value::Array(build_owner_refs(&c)),
        );
        // Empty uid — should produce empty array via the shared gate.
        let mut c_empty = ctx(&h, &b, "host", "demo-prod", false);
        c_empty.process_uid = "";
        let md_empty = routing_edge_metadata(
            &c_empty,
            "n".to_string(),
            crate::ssapply::ownership_annotations(c_empty.process_ref),
        );
        assert_eq!(
            md_empty["ownerReferences"],
            serde_json::Value::Array(build_owner_refs(&c_empty)),
        );
        assert!(
            md_empty["ownerReferences"]
                .as_array()
                .expect("array")
                .is_empty(),
            "empty-uid gate must produce an empty ownerReferences array"
        );
    }

    #[test]
    fn routing_edge_metadata_rides_annotations_argument_verbatim() {
        // The annotations arg is caller-composed (Ingress extends
        // with ROUTING_FORM + backend + cert-manager; DNS uses the
        // bare ownership tag). Pin that the composed metadata's
        // annotations slot is byte-identical to what the caller
        // handed in — a regression that reshaped the annotations at
        // the composer (silently rewriting or reordering keys)
        // surfaces HERE rather than as a subtle apply-time drift on
        // per-edge annotation contracts (cert-manager reading a
        // rewritten `cluster-issuer` value, backend annotations
        // dropped, etc.).
        let h = api_hostname();
        let b = api_backend();
        let c = ctx(&h, &b, "host", "demo-prod", true);
        let mut caller_annotations = crate::ssapply::ownership_annotations(c.process_ref);
        caller_annotations.insert(
            "custom.io/marker".to_string(),
            Value::String("keep-me".to_string()),
        );
        let md = routing_edge_metadata(&c, "n".to_string(), caller_annotations.clone());
        assert_eq!(md["annotations"], Value::Object(caller_annotations));
    }

    #[test]
    fn routing_edge_metadata_matches_hand_authored_pre_lift_bytewise() {
        // The exact 5-slot hand-authored composition every pre-lift
        // callsite restated. A regression that reordered a slot,
        // dropped one, or added a sixth surfaces at THIS pin rather
        // than as a subtle apply-time drift when a routing edge's
        // metadata block reaches the K8s wire missing a required
        // slot (ownerReferences drop → orphaned resource on Process
        // delete; labels drop → operator's selector greps miss the
        // edge; namespace drop → K8s defaults to `default` and the
        // apply lands in the wrong namespace).
        let h = api_hostname();
        let b = api_backend();
        for is_stable in [true, false] {
            let c = ctx(&h, &b, "host", "demo-prod", is_stable);
            let annotations = crate::ssapply::ownership_annotations(c.process_ref);

            let via_composer =
                routing_edge_metadata(&c, "test-name".to_string(), annotations.clone());

            // The pre-lift 5-slot block, byte-for-byte.
            let labels_map = routing_edge_labels(&c);
            let owner_refs = build_owner_refs(&c);
            let hand_authored = json!({
                "name": "test-name",
                "namespace": c.process_namespace,
                "labels": Value::Object(labels_map),
                "annotations": Value::Object(annotations),
                "ownerReferences": owner_refs,
            });

            assert_eq!(via_composer, hand_authored);
        }
    }

    #[test]
    fn ingress_and_dns_endpoint_metadata_stay_in_lockstep_shared_slots() {
        // Cross-edge coherence pin: both edge kinds route through
        // `routing_edge_metadata`, so their emitted `metadata` blocks
        // must agree byte-identically on the four shared slots
        // (namespace, labels, ownerReferences — plus the shape of
        // the name slot, though its value differs by the `-dns` DNS
        // suffix). Only the annotations slot legitimately differs.
        // A regression that reshaped one caller in isolation surfaces
        // HERE, not as silent divergence between the two edge kinds'
        // metadata shapes an operator would miss until an apply-time
        // wire-level diff.
        let h = api_hostname();
        let b = api_backend();
        for is_stable in [true, false] {
            let c = ctx(&h, &b, "host", "demo-prod", is_stable);
            let ingress = IngressEdge.render(&c).unwrap().unwrap();
            let dns = DnsEndpointEdge {
                ingress_lb_target: Some("lb.example.com".into()),
                ttl_seconds: 60,
            }
            .render(&c)
            .unwrap()
            .unwrap();
            for slot in ["namespace", "labels", "ownerReferences"] {
                assert_eq!(
                    ingress["metadata"][slot], dns["metadata"][slot],
                    "routing edges must carry byte-identical `metadata.{slot}` \
                     (is_stable={is_stable})",
                );
            }
        }
    }

    #[test]
    fn ingress_and_dns_endpoint_labels_stay_in_lockstep() {
        // Cross-edge coherence pin: both edge kinds route through
        // `routing_edge_labels`, so their emitted `metadata.labels`
        // maps must be byte-identical for the same context. A
        // regression that reshaped one caller in isolation surfaces
        // HERE, not as silent divergence between the Ingress and
        // DNSEndpoint label axes an operator's selector would
        // silently miss on one edge kind but not the other.
        let h = api_hostname();
        let b = api_backend();
        for is_stable in [true, false] {
            let c = ctx(&h, &b, "host", "demo-prod", is_stable);
            let ingress = IngressEdge.render(&c).unwrap().unwrap();
            let dns = DnsEndpointEdge {
                ingress_lb_target: Some("lb.example.com".into()),
                ttl_seconds: 60,
            }
            .render(&c)
            .unwrap()
            .unwrap();
            assert_eq!(
                ingress["metadata"]["labels"], dns["metadata"]["labels"],
                "routing edges must carry byte-identical labels (is_stable={is_stable})",
            );
        }
    }
}
