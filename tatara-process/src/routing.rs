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

    /// Cluster override slice with a caller-supplied per-config
    /// fallback applied — the ONE-line collapse of the paired
    /// `self.cluster.as_deref().unwrap_or(fallback)` incantation the
    /// reconciler's FQDN composer + stable-claim group-key composer
    /// both spelled by hand pre-lift.
    ///
    /// Pre-lift the projection was hand-authored at TWO sites past
    /// the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold in
    /// `tatara-reconciler`, each walking the SAME borrow-form
    /// `Option<String>` slot × per-config-fallback shape:
    /// * `render::render_routing` — per-instance FQDN composer seed
    ///   for [`crate::hostname::fmt_fqdn`], keyed on the cluster
    ///   segment.
    /// * `table_controller::stable_name_group_key` — claim-arbiter
    ///   `(cluster, app)` group-key seed, keyed on the cluster
    ///   segment.
    ///
    /// Both sites walked the SAME projection: pull the borrow-form
    /// `.cluster.as_deref()` slot, sink an absent slot to the
    /// per-config fallback the caller threads in from
    /// `Context.config.cluster`. Post-lift both consumers read
    /// `hostname.cluster_or(cfg_cluster)` — the projection sits at
    /// ONE substrate owner, so a future normalization (a case-fold
    /// pass, an empty-string-to-fallback promotion, a cross-cluster
    /// alias resolver, a per-fleet cluster-name canonicalization)
    /// lands here exactly once and every consumer (FQDN composer,
    /// claim-arbiter group key, and any future edge whose downstream
    /// keys on the cluster segment) inherits the upgrade
    /// mechanically.
    ///
    /// Peer to [`Self::is_named`] on the (Option<String> slot ×
    /// fallback shape) axis pair — both live on `RoutingHostname`
    /// and hide the missing-slot corner behind ONE substrate
    /// primitive; both preserve the borrow-form return, so downstream
    /// composers thread the slice without a `.to_string()` step.
    ///
    /// Semantics: an explicit `Some("")` returns the empty string
    /// (matching the pre-lift `.as_deref().unwrap_or(fallback)`
    /// chain's behavior). Callers whose downstream rejects an
    /// empty cluster segment must gate on that separately —
    /// [`crate::hostname::fmt_fqdn`]'s validator does so
    /// automatically via [`crate::hostname::HostnameError::
    /// InvalidLabel`].
    pub fn cluster_or<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.cluster.as_deref().unwrap_or(fallback)
    }

    /// Compose a [`RoutingHostname`] pinned to the "named-slot,
    /// per-config cluster fallback" shape (`instance: Some(<instance>)`,
    /// `cluster: None`) — the ONE substrate primitive owning the
    /// 3-slot `RoutingHostname { app, instance: Some(<instance>),
    /// cluster: None }` fixture literal every consumer restated by
    /// hand pre-lift.
    ///
    /// Pre-lift the same 3-slot chain (`app: <s>.into()`, `instance:
    /// Some(<s>.into())`, `cluster: None`) was hand-authored at TEN
    /// workspace-wide sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold, EVERY one of them the "named instance
    /// segment, cluster-inherits-from-config" shape:
    ///
    /// * `tatara-process::hostname` — two sites: the `resolve_named_slot_wins`
    ///   fixture plus the `end_to_end_named_and_unnamed_for_same_process`
    ///   named-arm fixture.
    /// * `tatara-process::routing` — four sites: the `demo_routing` seed
    ///   (two hostnames), the `hostname_is_named_when_instance_nonempty`
    ///   populated-instance pin, and the `cluster_or_borrow_form_return_shape_matches_fmt_fqdn_arg_shape`
    ///   FQDN-composer parity pin.
    /// * `tatara-reconciler::edges` — two sites: the `api_hostname`
    ///   test fixture plus the `routing_edge_labels_stamps_app_slot_from_hostname`
    ///   APP-slot pin (which stamps `"gateway"` instead of `"api"`).
    /// * `tatara-reconciler::render` — two sites: the `two_hostname_routing`
    ///   seed's `api` + `gateway` hostname pair.
    ///
    /// Post-lift every callsite reads `RoutingHostname::instanced(<app>,
    /// <instance>)` and the three-slot struct's `cluster` slot stays
    /// owned by the ONE substrate site — the per-config-cluster
    /// fallback resolved through [`Self::cluster_or`] at read time
    /// stays the ONLY axis a cluster override travels through, so a
    /// future normalization (a per-cluster canonicalization, a
    /// cross-cluster alias resolver, a claim-arbiter fallback swap)
    /// lands here exactly once and every consumer inherits the
    /// upgrade mechanically. The `impl Into<String>` bound on both
    /// positional args accepts every pre-lift caller shape verbatim
    /// — `&'static str` literals, owned `String` values, and
    /// `.into()`-terminated chains alike — without a per-site
    /// coercion.
    ///
    /// Peer to [`Self::content_hashed`] on the (instance slot ×
    /// cluster slot) axis pair: both live on `RoutingHostname` and
    /// hide the pair's "per-config cluster fallback" corner behind
    /// ONE substrate primitive; [`Self::instanced`] fills the
    /// `Some(<name>)` arm of the `instance` slot, [`Self::content_hashed`]
    /// fills the `None` arm.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `RoutingHostname { app, instance: Some(<i>), cluster: None }`
    /// fixture literal recurred at ten hand-authored sites past the
    /// ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to
    /// ONE owner here). THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — a regression that drifted the default-
    /// cluster sentinel from `None` to a hardcoded string, or
    /// reordered the three struct slots, surfaces at the
    /// `instanced_composes_byte_identical_to_pre_lift_literal_across_every_app_instance_pair`
    /// pin below rather than as silent skew at every downstream
    /// fixture).
    #[must_use]
    pub fn instanced(app: impl Into<String>, instance: impl Into<String>) -> Self {
        Self {
            app: app.into(),
            instance: Some(instance.into()),
            cluster: None,
        }
    }

    /// Compose a [`RoutingHostname`] pinned to the "content-hash
    /// anonymous, per-config cluster fallback" shape (`instance: None`,
    /// `cluster: None`) — the ONE substrate primitive owning the
    /// 3-slot `RoutingHostname { app, instance: None, cluster: None }`
    /// fixture literal every consumer restated by hand pre-lift.
    ///
    /// The `instance: None` slot instructs the reconciler's
    /// [`crate::hostname::resolve_ephemeral_id`] to substitute
    /// `blake3(canonical_spec)[:8]` — the content-hashed FQDN form
    /// documented on [`RoutingHostname`]. Pre-lift the same 3-slot
    /// chain (`app: <s>.into()`, `instance: None`, `cluster: None`)
    /// was hand-authored at NINE workspace-wide sites past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication threshold, EVERY one of them
    /// the "unnamed instance, cluster-inherits-from-config" shape:
    ///
    /// * `tatara-process::hostname` — two sites: the `resolve_unset_named_falls_back`
    ///   fixture plus the `end_to_end_named_and_unnamed_for_same_process`
    ///   anon-arm fixture.
    /// * `tatara-process::routing` — six sites: the `h_anon` pin, the
    ///   `cluster_or_falls_back_to_caller_string_when_cluster_is_none`
    ///   fallback pin, and four more `cluster_or` / round-trip fixtures.
    /// * `tatara-reconciler::render` — one site: the
    ///   `anonymous_hostname_uses_content_hash` FQDN composer pin.
    ///
    /// Peer to [`Self::instanced`] on the (instance slot × cluster
    /// slot) axis pair — [`Self::content_hashed`] fills the `None`
    /// arm of the `instance` slot, [`Self::instanced`] fills the
    /// `Some(<name>)` arm.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `RoutingHostname { app, instance: None, cluster: None }`
    /// fixture literal recurred at nine hand-authored sites past the
    /// ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to
    /// ONE owner here). THEORY.md §II.1 invariant 5.
    #[must_use]
    pub fn content_hashed(app: impl Into<String>) -> Self {
        Self {
            app: app.into(),
            instance: None,
            cluster: None,
        }
    }
}

impl RoutingBackend {
    /// Compose a [`RoutingBackend`] pinned to the "reconciler-default
    /// TLS issuer, no per-Ingress annotations" shape (`tls_issuer:
    /// None`, `ingress_annotations: BTreeMap::new()`) — the ONE
    /// substrate primitive owning the 4-slot `RoutingBackend { service,
    /// port, tls_issuer: None, ingress_annotations: BTreeMap::new() }`
    /// fixture literal every consumer restated by hand pre-lift.
    ///
    /// Pre-lift the same 4-slot chain (`service: <s>.into()`, `port:
    /// <u16>`, `tls_issuer: None`, `ingress_annotations:
    /// BTreeMap::new()`) was hand-authored at SEVEN workspace-wide
    /// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold,
    /// EVERY one of them the "default-issuer, empty-annotations"
    /// shape:
    ///
    /// * `tatara-process::routing` — three sites: the `demo_routing`
    ///   seed plus two round-trip pins (`empty_routing_resolves_no_hostnames`,
    ///   `empty_fields_skip_serialize`).
    /// * `tatara-reconciler::edges` — one site: the `api_backend`
    ///   test fixture consumed by every `IngressEdge` / `DnsEndpointEdge`
    ///   render pin.
    /// * `tatara-reconciler::render` — three sites: the `two_hostname_routing`
    ///   seed's backend, the `empty_hostnames_emits_nothing` pin, and
    ///   the `anonymous_hostname_uses_content_hash` pin.
    ///
    /// Post-lift every callsite reads `RoutingBackend::plain(<service>,
    /// <port>)` and the four-slot struct's `tls_issuer` +
    /// `ingress_annotations` slots stay owned by the ONE substrate
    /// site — a future normalization (a per-fleet default `ClusterIssuer`
    /// selection, a per-fleet baseline Ingress annotation set, a
    /// SPIRE-vs-Let's-Encrypt discriminator) lands here exactly once
    /// and every consumer inherits the upgrade mechanically. The
    /// `impl Into<String>` bound on `service` accepts every pre-lift
    /// caller shape verbatim.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `RoutingBackend { service, port, tls_issuer: None,
    /// ingress_annotations: BTreeMap::new() }` fixture literal recurred
    /// at seven hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// a regression that drifted the default `tls_issuer` sentinel
    /// from `None` to a hardcoded string, or reordered the four
    /// struct slots, surfaces at the
    /// `plain_composes_byte_identical_to_pre_lift_literal_across_every_service_port_pair`
    /// pin below rather than as silent skew at every downstream
    /// fixture).
    #[must_use]
    pub fn plain(service: impl Into<String>, port: u16) -> Self {
        Self {
            service: service.into(),
            port,
            tls_issuer: None,
            ingress_annotations: BTreeMap::new(),
        }
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
                RoutingHostname::instanced("api", "demo-prod"),
                RoutingHostname::instanced("gateway", "demo-prod"),
            ],
            backend: RoutingBackend::plain("demo-app-gateway", 8000),
            stable_name_claim: true,
            priority: 100,
        }
    }

    #[test]
    fn empty_routing_resolves_no_hostnames() {
        let r = RoutingSpec {
            hostnames: vec![],
            backend: RoutingBackend::plain("x", 80),
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
        let h = RoutingHostname::instanced("x", "env-a");
        assert!(h.is_named());

        let h_anon = RoutingHostname::content_hashed("x");
        assert!(!h_anon.is_named());

        let h_empty = RoutingHostname {
            app: "x".into(),
            instance: Some(String::new()),
            cluster: None,
        };
        assert!(!h_empty.is_named()); // empty string ⇒ unnamed
    }

    // ─── RoutingHostname::cluster_or substrate pins ──────────────
    //
    // The pre-lift reconciler restated the same
    // `hostname.cluster.as_deref().unwrap_or(<cfg-cluster>)` chain at
    // TWO callsites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // trigger:
    //   * render.rs::render_routing (line 561) — FQDN composer seed
    //   * table_controller.rs::stable_name_group_key (line 101) —
    //     claim-arbiter group-key seed
    // Every corner of the paired projection is pinned here so a
    // future normalization at the primitive lands with a
    // fail-before-pass-after regression at THIS composer's pins
    // rather than as silent operator-visible drift across the two
    // callsite arms.

    #[test]
    fn cluster_or_returns_slot_when_cluster_is_populated() {
        let h = RoutingHostname {
            app: "api".into(),
            instance: None,
            cluster: Some("pleme-prod".into()),
        };
        assert_eq!(h.cluster_or("pleme-dev"), "pleme-prod");
    }

    #[test]
    fn cluster_or_falls_back_to_caller_string_when_cluster_is_none() {
        let h = RoutingHostname::content_hashed("api");
        assert_eq!(h.cluster_or("pleme-dev"), "pleme-dev");
    }

    #[test]
    fn cluster_or_returns_empty_slice_when_cluster_is_explicitly_empty_string() {
        // Populated-empty short-circuit pin: `Some("")` is a
        // populated slot for `as_deref().unwrap_or(...)`, so the
        // fallback is NOT taken. The downstream FQDN composer's
        // validator (`fmt_fqdn`) rejects the empty label with a
        // typed `HostnameError::InvalidLabel`, not this primitive.
        let h = RoutingHostname {
            app: "api".into(),
            instance: None,
            cluster: Some(String::new()),
        };
        assert_eq!(h.cluster_or("pleme-dev"), "");
    }

    #[test]
    fn cluster_or_is_a_pure_projection() {
        // Two identical inputs → two identical outputs; no interior
        // mutation or per-call hidden state.
        let h = RoutingHostname {
            app: "api".into(),
            instance: Some("demo-prod".into()),
            cluster: Some("pleme-prod".into()),
        };
        let a = h.cluster_or("pleme-dev");
        let b = h.cluster_or("pleme-dev");
        assert_eq!(a, b);
        assert_eq!(a, "pleme-prod");
    }

    #[test]
    fn cluster_or_borrow_form_return_shape_matches_fmt_fqdn_arg_shape() {
        // The primitive returns `&str` so it slots straight into
        // `fmt_fqdn(&hostname.app, eph_id, host_cluster, location,
        // domain)` at `render::render_routing` without a
        // `.to_string()` step. Compose here so a future return-shape
        // change (owned `String`, `Cow<'_, str>`) breaks this pin,
        // not the reconciler.
        use crate::hostname::fmt_fqdn;
        let h = RoutingHostname::instanced("api", "demo-prod");
        let host_cluster: &str = h.cluster_or("pleme-dev");
        let fqdn = fmt_fqdn(
            &h.app,
            h.instance.as_deref().unwrap(),
            host_cluster,
            "use1",
            "quero.lol",
        )
        .expect("fmt_fqdn");
        assert_eq!(fqdn, "api.demo-prod.pleme-dev.use1.quero.lol");
    }

    #[test]
    fn cluster_or_matches_pre_lift_chain_verbatim() {
        // Full 4-corner byte-identical parity table across the
        // `(cluster slot × fallback shape)` axis pair. Any
        // divergence between the primitive and each pre-lift
        // callsite's inline chain surfaces HERE rather than as
        // per-site operator-visible drift.
        let fallbacks = ["pleme-dev", "pleme-prod", "", "some-other-cluster"];
        let cluster_slots = [
            None,
            Some(String::new()),
            Some("pleme-prod".into()),
            Some("edge-1".into()),
        ];
        for fallback in fallbacks {
            for cluster in &cluster_slots {
                let h = RoutingHostname {
                    app: "api".into(),
                    instance: None,
                    cluster: cluster.clone(),
                };
                let pre_lift = h.cluster.as_deref().unwrap_or(fallback);
                let via_primitive = h.cluster_or(fallback);
                assert_eq!(
                    via_primitive, pre_lift,
                    "primitive must match pre-lift `.as_deref().unwrap_or(fallback)` chain \
                     byte-identically at (fallback={fallback:?}, cluster={cluster:?})"
                );
            }
        }
    }

    #[test]
    fn cluster_or_composes_with_stable_group_key_shape() {
        // Peer-composition pin against
        // `table_controller::stable_name_group_key`'s downstream
        // seed shape (`format!("{cluster}/{}", hostname.app)`).
        // A future rename of the separator or the composer's
        // ordering breaks this pin, not the claim-arbiter row seed.
        let h = RoutingHostname::content_hashed("api");
        let cluster = h.cluster_or("pleme-dev");
        let key = format!("{cluster}/{}", h.app);
        assert_eq!(key, "pleme-dev/api");

        let h_over = RoutingHostname {
            app: "api".into(),
            instance: None,
            cluster: Some("pleme-prod".into()),
        };
        let cluster = h_over.cluster_or("pleme-dev");
        let key = format!("{cluster}/{}", h_over.app);
        assert_eq!(key, "pleme-prod/api");
    }

    #[test]
    fn cluster_or_lifetime_ties_output_to_the_shorter_of_self_or_fallback() {
        // Compile-time proof (via the return signature) that the
        // returned slice borrows through EITHER `&self.cluster` or
        // `&fallback` — the caller cannot outlive the shorter of
        // the two. If a future refactor loosens the lifetime to
        // `&'a str` where `'a` is only tied to `self`, this test
        // stops compiling with the fallback-borrow arm.
        let h = RoutingHostname::content_hashed("api");
        {
            let fallback = String::from("pleme-dev");
            let slice = h.cluster_or(&fallback);
            assert_eq!(slice, "pleme-dev");
            // `slice` cannot escape this scope — its lifetime is
            // bounded by `fallback`. That's the compile-time
            // discipline the `<'a>` on the primitive encodes.
        }
    }

    // ─── RoutingHostname::instanced substrate pins ───────────────
    //
    // The pre-lift workspace restated the 3-slot `RoutingHostname {
    // app, instance: Some(<i>), cluster: None }` fixture literal at
    // TEN hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    // duplication trigger. Every corner of the shipped shape is
    // pinned here so a regression that drifted the default-cluster
    // sentinel from `None` to a hardcoded string, or reordered the
    // three struct slots, surfaces at THIS composer's shipped-shape
    // pin rather than as silent skew at every downstream fixture.

    #[test]
    fn instanced_composes_populated_instance_with_default_cluster() {
        let h = RoutingHostname::instanced("api", "demo-prod");
        assert_eq!(h.app, "api");
        assert_eq!(h.instance.as_deref(), Some("demo-prod"));
        assert!(h.cluster.is_none());
    }

    #[test]
    fn instanced_composes_byte_identical_to_pre_lift_literal_across_every_app_instance_pair() {
        // Full 3-corner byte-identical parity table across the
        // `(app × instance)` axis pair. Any divergence between the
        // primitive and each pre-lift callsite's inline literal
        // surfaces HERE rather than as per-site operator-visible
        // drift.
        let pairs = [
            ("api", "demo-prod"),
            ("gateway", "demo-prod"),
            ("x", "env-a"),
        ];
        for (app, instance) in pairs {
            let via_primitive = RoutingHostname::instanced(app, instance);
            let pre_lift = RoutingHostname {
                app: app.into(),
                instance: Some(instance.into()),
                cluster: None,
            };
            assert_eq!(
                via_primitive, pre_lift,
                "primitive must match pre-lift `RoutingHostname {{ app, instance: Some(..), \
                 cluster: None }}` literal byte-identically at (app={app:?}, instance={instance:?})"
            );
        }
    }

    #[test]
    fn instanced_is_named_via_peer_projection() {
        // Peer-composition pin: the primitive's shipped shape must
        // continue to satisfy `is_named` (the sibling `RoutingHostname`
        // projection that reads the same `instance` slot).
        assert!(RoutingHostname::instanced("api", "demo-prod").is_named());
    }

    #[test]
    fn instanced_cluster_or_falls_back_to_caller_string() {
        // Peer-composition pin against `cluster_or`: the primitive
        // stamps `cluster: None`, so `cluster_or` MUST return the
        // caller-supplied fallback verbatim.
        let h = RoutingHostname::instanced("api", "demo-prod");
        assert_eq!(h.cluster_or("pleme-dev"), "pleme-dev");
    }

    // ─── RoutingHostname::content_hashed substrate pins ──────────
    //
    // The pre-lift workspace restated the 3-slot `RoutingHostname {
    // app, instance: None, cluster: None }` fixture literal at NINE
    // hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // trigger. Every corner of the shipped shape is pinned here.

    #[test]
    fn content_hashed_composes_unset_instance_with_default_cluster() {
        let h = RoutingHostname::content_hashed("smoke");
        assert_eq!(h.app, "smoke");
        assert!(h.instance.is_none());
        assert!(h.cluster.is_none());
    }

    #[test]
    fn content_hashed_composes_byte_identical_to_pre_lift_literal_across_every_app_slot() {
        let apps = ["api", "gateway", "smoke", "x"];
        for app in apps {
            let via_primitive = RoutingHostname::content_hashed(app);
            let pre_lift = RoutingHostname {
                app: app.into(),
                instance: None,
                cluster: None,
            };
            assert_eq!(
                via_primitive, pre_lift,
                "primitive must match pre-lift `RoutingHostname {{ app, instance: None, \
                 cluster: None }}` literal byte-identically at (app={app:?})"
            );
        }
    }

    #[test]
    fn content_hashed_is_not_named() {
        // Peer-composition pin against `is_named`: an unset
        // `instance` slot is definitionally content-hashed, i.e. NOT
        // named — the reconciler's FQDN composer downstream
        // substitutes `blake3(canonical_spec)[:8]` for the segment.
        assert!(!RoutingHostname::content_hashed("smoke").is_named());
    }

    // ─── RoutingBackend::plain substrate pins ────────────────────
    //
    // The pre-lift workspace restated the 4-slot `RoutingBackend {
    // service, port, tls_issuer: None, ingress_annotations:
    // BTreeMap::new() }` fixture literal at SEVEN hand-authored sites
    // past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger.

    #[test]
    fn plain_composes_default_issuer_and_empty_annotations() {
        let b = RoutingBackend::plain("svc", 8080);
        assert_eq!(b.service, "svc");
        assert_eq!(b.port, 8080);
        assert!(b.tls_issuer.is_none());
        assert!(b.ingress_annotations.is_empty());
    }

    #[test]
    fn plain_composes_byte_identical_to_pre_lift_literal_across_every_service_port_pair() {
        let pairs = [
            ("demo-app-gateway", 8000_u16),
            ("svc", 80),
            ("svc", 8080),
            ("x", 80),
        ];
        for (service, port) in pairs {
            let via_primitive = RoutingBackend::plain(service, port);
            let pre_lift = RoutingBackend {
                service: service.into(),
                port,
                tls_issuer: None,
                ingress_annotations: BTreeMap::new(),
            };
            assert_eq!(
                via_primitive, pre_lift,
                "primitive must match pre-lift `RoutingBackend {{ service, port, tls_issuer: \
                 None, ingress_annotations: BTreeMap::new() }}` literal byte-identically at \
                 (service={service:?}, port={port})"
            );
        }
    }

    #[test]
    fn plain_wire_form_skips_defaulted_slots() {
        // Peer-composition pin: the primitive stamps `tls_issuer:
        // None` + empty `ingress_annotations`, both of which are
        // `serde(skip_serializing_if)` — so the wire form MUST NOT
        // include either key. A regression that flipped the default
        // sentinels to non-empty values would leak them into every
        // rendered wire form; this pin fails first.
        let b = RoutingBackend::plain("svc", 80);
        let yaml = serde_yaml::to_string(&b).unwrap();
        assert!(!yaml.contains("tlsIssuer:"));
        assert!(!yaml.contains("ingressAnnotations:"));
        assert!(yaml.contains("service: svc"));
        assert!(yaml.contains("port: 80"));
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
            hostnames: vec![RoutingHostname::content_hashed("api")],
            backend: RoutingBackend::plain("svc", 8080),
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
            hostnames: vec![RoutingHostname::content_hashed("api")],
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
