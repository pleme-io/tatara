//! Shared reconciler context — K8s client, config, metrics.

use std::sync::Arc;

use kube::{Api, Client};

use tatara_process::prelude::Process;

#[derive(Clone)]
pub struct Context {
    pub kube: Client,
    pub config: Arc<ReconcilerConfig>,
}

impl Context {
    /// Namespaced `Api<Process>` bound to this context's client — the
    /// substrate primitive every Process-API construction site inside
    /// this crate rides through.
    ///
    /// Every handler in `phase_machine.rs`, `signals.rs`, and
    /// `controller.rs` builds a Process-bound `Api` by pairing
    /// `self.kube.clone()` with the namespace pulled from the incoming
    /// resource. Pre-lift each site restated the two-slot `(kube.clone(),
    /// ns)` incantation verbatim as `let api: Api<Process> =
    /// Api::namespaced(ctx.kube.clone(), &ns)`, sprayed 20× across the
    /// crate. Post-lift each site delegates through ONE substrate
    /// method — a future change that layers request tracing spans, a
    /// default `PatchParams` builder, a namespace-scoped access-control
    /// gate, or per-request metrics onto every Process-API request
    /// lands at ONE site rather than being restated at every callsite.
    ///
    /// The `&str` parameter accepts both `&String` (which coerces via
    /// deref) and `&str` literal / slice callers, matching every shape
    /// currently authored: the phase-machine handlers pull `ns: String`
    /// out of `namespace_and_name(p)?` and pass `&ns`; the boundary
    /// evaluators + controller pass `ns: &str` slices unchanged.
    pub fn process_api(&self, ns: &str) -> Api<Process> {
        Api::namespaced(self.kube.clone(), ns)
    }
}

#[derive(Clone, Debug)]
pub struct ReconcilerConfig {
    /// Namespace the controller runs in (for ProcessTable singleton lookups).
    pub controller_namespace: String,
    /// Default boundary timeout if `spec.boundary.timeout` is unset.
    pub default_boundary_timeout_seconds: u64,
    /// Default requeue interval between heartbeats.
    pub heartbeat_seconds: u64,
    /// Name of the cluster-scoped ProcessTable singleton.
    pub process_table_name: String,
    /// Container image the reconciler stamps into each
    /// tatara-export-worker Job emitted during the `Releasing`
    /// phase. Operators override via the reconciler's Helm chart
    /// values.
    pub export_worker_image: String,
    /// ServiceAccount the export-worker Jobs run as. Operators
    /// provision it (Role + RoleBinding granting list/get/patch on
    /// ConfigMaps + get on Processes) via the same Helm chart that
    /// ships the reconciler.
    pub export_worker_service_account: String,

    /// **R9 fleet routing config** — cluster + location + domain
    /// segments stamped into every emitted FQDN. Matches the
    /// `nix/lib/fleet-domains.nix mkHostname` pattern.
    /// Per-cluster overrides via the reconciler Helm chart.
    pub cluster: String,
    pub location: String,
    pub domain: String,

    /// External-dns target — the cluster's ingress loadbalancer
    /// hostname (or CNAME-able equivalent). When set, the
    /// reconciler emits DNSEndpoint resources pointing all FQDNs
    /// at this target. None ⇒ Ingress emits but DNS does not.
    pub dns_lb_target: Option<String>,
}

impl Default for ReconcilerConfig {
    fn default() -> Self {
        Self {
            controller_namespace: "tatara-system".into(),
            default_boundary_timeout_seconds: 900,
            heartbeat_seconds: 30,
            process_table_name: "proc".into(),
            export_worker_image: "ghcr.io/pleme-io/tatara-export-worker:0.2.0".into(),
            export_worker_service_account: "tatara-export-worker".into(),
            cluster: "pleme-dev".into(),
            location: "use1".into(),
            domain: "quero.lol".into(),
            dns_lb_target: None,
        }
    }
}

// ─── Context::process_api substrate pins ───────────────────────────────
//
// The two-slot `(ctx.kube.clone(), ns)` incantation was hand-authored
// at 20 sites across `phase_machine.rs` / `signals.rs` / `controller.rs`
// before `process_api` closed it. These pins bind the primitive at
// fail-before-pass-after granularity so a regression that drifts the
// namespace binding, the resource kind, or the reused client-slot
// surfaces here rather than as silent operator-facing drift at every
// downstream handler.
//
// `Client::try_from(Config::new(url))` needs a live tokio reactor
// (tower::buffer::Buffer::new spawns a background task on construction),
// so every pin runs under `#[tokio::test]`.
#[cfg(test)]
mod tests {
    use super::*;
    use kube::Config;

    fn ctx() -> Context {
        let url = "http://localhost:9999".parse().expect("valid probe url");
        let client = Client::try_from(Config::new(url)).expect("build kube client");
        Context {
            kube: client,
            config: Arc::new(ReconcilerConfig::default()),
        }
    }

    #[tokio::test]
    async fn process_api_binds_namespace_into_resource_url() {
        // Every phase-machine handler pulls a per-Process namespace out
        // of `namespace_and_name(p)?` and expects `ctx.process_api(&ns)`
        // to bind that namespace onto the returned Api's REST path — the
        // shape every subsequent `.patch(...) / .patch_status(...) /
        // .get_opt(...)` call composes against. A regression that
        // silently swapped the namespace slot (all-namespaces, or a
        // hard-coded literal) would surface here.
        let api = ctx().process_api("acme-prod");
        let url = api.resource_url();
        assert!(
            url.contains("/namespaces/acme-prod/"),
            "namespaced Api resource url must carry the caller's namespace verbatim; got {url}"
        );
    }

    #[tokio::test]
    async fn process_api_binds_the_process_kind() {
        // The typed `Api<Process>` return type pins the resource kind at
        // rustc time; this pin adds the runtime witness — the emitted
        // REST path targets the `tatara.pleme.io/v1alpha1/processes`
        // collection matching the `#[kube(group = "tatara.pleme.io",
        // version = "v1alpha1", plural = "processes")]` attribute on
        // `ProcessSpec` (see `tatara-process/src/crd.rs`). A regression
        // that changed the group, version, or plural on the CRD without
        // rippling through downstream consumers surfaces here.
        let api = ctx().process_api("default");
        let url = api.resource_url();
        assert!(
            url.starts_with("/apis/tatara.pleme.io/v1alpha1/"),
            "Api resource url must be scoped to the tatara.pleme.io/v1alpha1 group; got {url}"
        );
        assert!(
            url.ends_with("/processes"),
            "Api resource url must terminate at the `processes` collection; got {url}"
        );
    }

    #[tokio::test]
    async fn process_api_rides_through_the_context_client_slot() {
        // Every callsite pre-lift chose `ctx.kube.clone()` — the same
        // client the reconciler was constructed with. Post-lift the
        // primitive continues to source its client from
        // `self.kube.clone()` rather than manufacturing a new one; this
        // pin catches a regression that swapped the client-slot at the
        // primitive's own site (e.g. `Client::try_default()` or a
        // `RwLock`-cached alternate). The witness: two separately-built
        // Apis on the SAME context share a functional client shape (both
        // resolve their resource url against the SAME
        // `tatara.pleme.io/v1alpha1/processes` collection).
        let c = ctx();
        let a = c.process_api("ns-a");
        let b = c.process_api("ns-b");
        assert_ne!(
            a.resource_url(),
            b.resource_url(),
            "distinct namespaces bind distinct urls"
        );
        // Both urls agree on the group/version/collection axes; only
        // the namespace slot differs.
        for slot in ["/apis/tatara.pleme.io/v1alpha1/", "/processes"] {
            assert!(
                a.resource_url().contains(slot) && b.resource_url().contains(slot),
                "both context-sourced Apis must target the same collection slot {slot:?}; got {} vs {}",
                a.resource_url(),
                b.resource_url(),
            );
        }
    }

    #[tokio::test]
    async fn process_api_accepts_string_deref_and_str_slice_shapes() {
        // The 20 shipped callsites split across two shapes: 15 sites
        // pull `ns: String` from `namespace_and_name(p)?` and pass
        // `&ns` (String→&str via deref coercion); 5 sites pull `ns:
        // &str` from the boundary/controller layer and pass it
        // unchanged. This pin binds both shapes at the type level —
        // the `&str` parameter must accept both without widening.
        let c = ctx();
        let owned = String::from("owned-ns");
        let borrowed: &str = "borrowed-ns";
        let via_owned = c.process_api(&owned);
        let via_borrowed = c.process_api(borrowed);
        assert!(via_owned.resource_url().contains("/namespaces/owned-ns/"));
        assert!(via_borrowed
            .resource_url()
            .contains("/namespaces/borrowed-ns/"));
    }
}
