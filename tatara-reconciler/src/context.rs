//! Shared reconciler context — K8s client, config, metrics.

use std::sync::Arc;

use kube::{Api, Client};

use tatara_process::prelude::{Process, ProcessTable};

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
        // Delegates through the workspace-wide substrate owner
        // `tatara_process::process_api::namespaced` (peer to
        // `tatara_process::configmap::namespaced` on the K8s-built-in
        // axis), so a future normalization of the ns-scoped
        // Process-handle posture — a wired-in tracing span, a
        // per-namespace retry budget, an SSA field-manager default,
        // a fixture-backed client for CI smoke-tests — lands at ONE
        // substrate primitive and reaches BOTH this per-request
        // reconciler forwarder AND every below-controller consumer
        // (`tatara_reconciler::boundary::{evaluate_process_phase,
        // check_depends_on}`, `tatara_export_worker::main::read_artifact`)
        // through the same owner.
        tatara_process::process_api::namespaced(self.kube.clone(), ns)
    }

    /// Cluster-scoped `Api<ProcessTable>` bound to this context's client —
    /// the peer substrate primitive alongside [`Context::process_api`]
    /// closing the `Api::all(self.kube.clone())` shape every ProcessTable
    /// consumer on this crate rides through.
    ///
    /// `ProcessTable` is a cluster-scoped singleton (see the
    /// `#[kube(kind = "ProcessTable", plural = "processtables",
    /// shortname = "pt")]` attribute on `ProcessTableSpec` in
    /// `tatara-process/src/table.rs`); every construction site pre-lift
    /// authored `let table_api: Api<ProcessTable> =
    /// Api::all(<client>.clone())` verbatim, restating both the typed
    /// collection binding and the `.clone()` incantation. Post-lift each
    /// site delegates through ONE substrate method — a future change
    /// that layers a per-request tracing span, a default
    /// `PatchParams` builder, a namespace-scoped RBAC gate, or per-request
    /// metrics counters onto every ProcessTable request lands at ONE
    /// substrate method here rather than being restated at every
    /// consumer.
    ///
    /// Unlike [`Context::process_api`], no namespace slot is exposed:
    /// the ProcessTable is a cluster-scoped resource (`Api::all`), not
    /// a namespaced one, so the primitive's signature encodes that
    /// invariant structurally — a caller cannot accidentally pass a
    /// namespace and get a broken `Api::namespaced` back.
    pub fn process_table_api(&self) -> Api<ProcessTable> {
        Api::all(self.kube.clone())
    }

    /// Cluster-scoped `Api<Process>` bound to this context's client —
    /// the third peer substrate primitive alongside
    /// [`Context::process_api`] (namespaced `Api<Process>`) and
    /// [`Context::process_table_api`] (cluster-scoped
    /// `Api<ProcessTable>`), closing the `Api::all(self.kube.clone())`
    /// shape every cluster-wide Process consumer on this crate rides
    /// through.
    ///
    /// Where [`Context::process_api`] binds to a specific namespace
    /// for per-Process handler traffic, `processes_all_api` returns a
    /// cluster-wide handle for consumers that need to enumerate
    /// Processes across every namespace — the reap-children walker
    /// (`phase_machine::handle_exiting`) filtering by
    /// `spec.identity.parent`, the claim-arbiter enumerate
    /// (`table_controller::reconcile`) grouping by
    /// `${cluster}/${app}`, and the top-level `Controller::new(...)`
    /// wiring in `main.rs` when `--watch-namespace` is empty.
    ///
    /// The primitive's signature encodes the "no namespace slot"
    /// invariant structurally: a caller cannot accidentally pass a
    /// namespace and get a `Api::namespaced` binding that silently
    /// misroutes every list/patch call. Post-lift a future change
    /// that layers per-request tracing spans, a cluster-wide watch
    /// filter, a client-side QPS limiter, or a fixture-backed client
    /// for CI/smoke-tests onto every cluster-wide Process read lands
    /// at ONE substrate method here.
    pub fn processes_all_api(&self) -> Api<Process> {
        Api::all(self.kube.clone())
    }

    /// Handler-entry composer on the (namespace, name, api) tuple axis —
    /// closes the two-line duet every phase-machine + signal-dispatch
    /// site inside this crate authored verbatim before this composer.
    ///
    /// Pre-lift each site restated the same two-line incantation:
    /// ```ignore
    /// let (ns, name) = p.owned_coordinates_or_err()?;
    /// let api = ctx.process_api(&ns);
    /// ```
    /// … at 6 sites across `phase_machine::handle_{forking,
    /// reconverging, failed, zombie, reaped}` + `signals::consume_effect`.
    ///
    /// Post-lift every site rides ONE substrate primitive — a future
    /// change that wires a per-request tracing span keyed on the
    /// `(ns, name)` pair, a namespace-scoped RBAC gate, a per-Process
    /// metrics label, or a shared retry budget onto every handler-entry
    /// lands at ONE substrate method here rather than being restated at
    /// every callsite.
    ///
    /// The composer is a pure fold of the two peers on this same
    /// `impl` block — [`Context::process_api`] on the client-slot axis
    /// and [`Process::owned_coordinates_or_err`] on the metadata axis
    /// (see `tatara-process/src/crd.rs`) — so a regression that drifts
    /// either peer surfaces at the peer's own substrate pins first, and
    /// the composed shape falls out as a consequence.
    pub fn owned_process_binding(
        &self,
        p: &Process,
    ) -> anyhow::Result<(String, String, Api<Process>)> {
        let (ns, name) = p.owned_coordinates_or_err()?;
        let api = self.process_api(&ns);
        Ok((ns, name, api))
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

    // ─── Context::process_table_api substrate pins ─────────────────────
    //
    // The `Api::all(<client>.clone())` incantation was hand-authored at
    // 4 ProcessTable construction sites (`main.rs`, `phase_machine.rs`
    // × 2, `table_controller.rs`) before `process_table_api` closed it.
    // These pins bind the primitive at fail-before-pass-after
    // granularity so a regression that drifts the resource-kind, the
    // scope (`Api::all` → `Api::namespaced`), or the reused client-slot
    // surfaces here rather than as silent operator-facing drift at
    // every downstream ProcessTable consumer.

    #[tokio::test]
    async fn process_table_api_is_cluster_scoped() {
        // ProcessTable is a cluster-scoped singleton (see the
        // `#[kube(kind = "ProcessTable", plural = "processtables",
        // shortname = "pt")]` attribute on `ProcessTableSpec` in
        // `tatara-process/src/table.rs`). Every downstream `.get(...)`
        // / `.patch(...)` call composes against a cluster-scoped url
        // — the resource path must NOT carry a `/namespaces/<ns>/`
        // slot. A regression that swapped `Api::all` for
        // `Api::namespaced` at the primitive's site surfaces here
        // rather than as a runtime "resource not found" spiral on
        // every ProcessTable consumer.
        let api = ctx().process_table_api();
        let url = api.resource_url();
        assert!(
            !url.contains("/namespaces/"),
            "cluster-scoped ProcessTable Api must NOT carry a namespace slot; got {url}"
        );
    }

    #[tokio::test]
    async fn process_table_api_binds_the_process_table_kind() {
        // The typed `Api<ProcessTable>` return type pins the resource
        // kind at rustc time; this pin adds the runtime witness — the
        // emitted REST path targets the
        // `tatara.pleme.io/v1alpha1/processtables` collection matching
        // the `#[kube(group = "tatara.pleme.io", version = "v1alpha1",
        // plural = "processtables")]` attribute on `ProcessTableSpec`.
        // A regression that changed the group, version, or plural on
        // the CRD without rippling through consumers surfaces here.
        let api = ctx().process_table_api();
        let url = api.resource_url();
        assert!(
            url.starts_with("/apis/tatara.pleme.io/v1alpha1/"),
            "Api resource url must be scoped to the tatara.pleme.io/v1alpha1 group; got {url}"
        );
        assert!(
            url.ends_with("/processtables"),
            "Api resource url must terminate at the `processtables` collection; got {url}"
        );
    }

    #[tokio::test]
    async fn process_table_api_rides_through_the_context_client_slot() {
        // Every pre-lift callsite chose `<client>.clone()` off the same
        // context (either `ctx.kube.clone()` in phase_machine or
        // `kube.clone()` in table_controller/main.rs, where `kube ===
        // ctx.kube`). Post-lift the primitive continues to source its
        // client from `self.kube.clone()` rather than manufacturing a
        // new one; this pin catches a regression that swapped the
        // client-slot at the primitive's own site. The witness: two
        // separately-built Apis on the SAME context resolve to the
        // SAME cluster-scoped url — no per-call url divergence, no
        // hidden namespace binding.
        let c = ctx();
        let a = c.process_table_api();
        let b = c.process_table_api();
        assert_eq!(
            a.resource_url(),
            b.resource_url(),
            "cluster-scoped Api built twice off the same context must resolve to the same url"
        );
    }

    #[tokio::test]
    async fn process_api_and_process_table_api_bind_distinct_collections() {
        // The two peer primitives share the client-slot + the
        // `tatara.pleme.io/v1alpha1` group but must resolve to
        // DISTINCT collections — `/processes` (namespaced) vs
        // `/processtables` (cluster-scoped). A regression that
        // conflated the two typed returns (e.g. a copy-paste that
        // pointed `process_table_api` at `Process` instead of
        // `ProcessTable`) would collapse both urls onto the same
        // collection and silently misroute every ProcessTable
        // consumer to the Process endpoint.
        let c = ctx();
        let processes = c.process_api("default").resource_url().to_string();
        let table = c.process_table_api().resource_url().to_string();
        assert!(
            processes.ends_with("/processes"),
            "process_api → /processes; got {processes}"
        );
        assert!(
            table.ends_with("/processtables"),
            "process_table_api → /processtables; got {table}"
        );
        assert_ne!(
            processes, table,
            "the two peer primitives must resolve to distinct collections"
        );
    }

    // ─── Context::processes_all_api substrate pins ─────────────────────
    //
    // The `Api::all(<client>.clone())` incantation for `Api<Process>`
    // was hand-authored at 3 cluster-wide Process consumer sites
    // (`phase_machine.rs` reap-children walker, `table_controller.rs`
    // claim-arbiter enumerate, `main.rs` top-level watch selector's
    // `--watch-namespace=""` branch) before `processes_all_api` closed
    // it. These pins bind the primitive at fail-before-pass-after
    // granularity so a regression that drifts the resource-kind, the
    // scope (`Api::all` → `Api::namespaced`), or the reused client-slot
    // surfaces here rather than as silent operator-facing drift.

    #[tokio::test]
    async fn processes_all_api_is_cluster_scoped() {
        // Every cluster-wide Process consumer composes against a
        // cluster-scoped url — the resource path must NOT carry a
        // `/namespaces/<ns>/` slot. A regression that swapped
        // `Api::all` for `Api::namespaced` at the primitive's site
        // would silently misroute every cluster-wide enumerate
        // (reap-children, claim-arbiter, top-level watch) at runtime.
        let api = ctx().processes_all_api();
        let url = api.resource_url();
        assert!(
            !url.contains("/namespaces/"),
            "cluster-scoped Api<Process> must NOT carry a namespace slot; got {url}"
        );
    }

    #[tokio::test]
    async fn processes_all_api_binds_the_process_kind() {
        // The typed `Api<Process>` return type pins the resource kind
        // at rustc time; this pin adds the runtime witness — the
        // emitted REST path targets the
        // `tatara.pleme.io/v1alpha1/processes` collection matching
        // the `#[kube(group = "tatara.pleme.io", version =
        // "v1alpha1", plural = "processes")]` attribute on
        // `ProcessSpec`. A regression that changed the group,
        // version, or plural on the CRD without rippling through
        // consumers surfaces here.
        let api = ctx().processes_all_api();
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
    async fn processes_all_api_rides_through_the_context_client_slot() {
        // Every pre-lift callsite chose `<client>.clone()` off the
        // same context (either `ctx.kube.clone()` in phase_machine or
        // `kube.clone()` in table_controller/main.rs, where `kube ===
        // ctx.kube`). Post-lift the primitive continues to source its
        // client from `self.kube.clone()`; this pin catches a
        // regression that swapped the client-slot at the primitive's
        // own site. The witness: two separately-built Apis on the
        // SAME context resolve to the SAME cluster-scoped url — no
        // per-call url divergence, no hidden namespace binding.
        let c = ctx();
        let a = c.processes_all_api();
        let b = c.processes_all_api();
        assert_eq!(
            a.resource_url(),
            b.resource_url(),
            "cluster-scoped Api built twice off the same context must resolve to the same url"
        );
    }

    #[tokio::test]
    async fn processes_all_api_is_scope_peer_of_process_api_over_the_same_collection() {
        // The two Process-bound primitives share the typed collection
        // (`/processes`) but differ ONLY on the scope axis:
        // `processes_all_api` is cluster-scoped (no namespace slot),
        // `process_api(&ns)` is namespaced (carries the namespace).
        // A regression that swapped the scope at either primitive's
        // site (`Api::all` ↔ `Api::namespaced`) would collapse the
        // two axes onto the same shape and silently redirect either
        // the per-handler patch traffic or the cluster-wide
        // enumerate traffic to the wrong scope. The pin: both urls
        // end at `/processes`; only `process_api`'s carries a
        // `/namespaces/<ns>/` slot.
        let c = ctx();
        let all_url = c.processes_all_api().resource_url().to_string();
        let ns_url = c.process_api("scoped-ns").resource_url().to_string();
        assert!(
            all_url.ends_with("/processes"),
            "processes_all_api → /processes; got {all_url}"
        );
        assert!(
            ns_url.ends_with("/processes"),
            "process_api → /processes; got {ns_url}"
        );
        assert!(
            !all_url.contains("/namespaces/"),
            "processes_all_api must NOT carry a namespace slot; got {all_url}"
        );
        assert!(
            ns_url.contains("/namespaces/scoped-ns/"),
            "process_api must carry the caller's namespace slot; got {ns_url}"
        );
    }

    // ─── Context::owned_process_binding substrate pins ─────────────────
    //
    // The two-line duet
    //     let (ns, name) = p.owned_coordinates_or_err()?;
    //     let api = ctx.process_api(&ns);
    // was hand-authored at 6 handler-entry sites across
    // `phase_machine.rs` (handle_forking, handle_reconverging,
    // handle_failed, handle_zombie, handle_reaped) + `signals.rs`
    // (consume_effect) before `owned_process_binding` closed it. These
    // pins bind the composer at fail-before-pass-after granularity so a
    // regression that drifts the tuple ordering, the client-slot, the
    // namespace binding, or the error-shape surfaces here rather than
    // as silent operator-facing drift.
    //
    // The composer forwards onto two peers already covered by their
    // own substrate pins (`Context::process_api` above +
    // `Process::owned_coordinates_or_err` in `tatara-process/src/crd.rs`),
    // so these pins add only the composed-shape witnesses — the tuple
    // ordering, the pass-through of both peers, and the error-shape
    // fidelity on the failing branch.
    fn process_with(ns: Option<&str>, name: Option<&str>) -> Process {
        // Routes the minimal `ProcessSpec` through the substrate
        // composer `ProcessSpec::gate_compute_defaults` (sibling to
        // every `empty_process_spec` fixture family in
        // `tatara-process/src/lib.rs`) so a schema drift lands at ONE
        // owner rather than a hand-authored default-slot litany here.
        let mut p = Process::new(
            name.unwrap_or(""),
            tatara_process::prelude::ProcessSpec::gate_compute_defaults(),
        );
        p.metadata.namespace = ns.map(String::from);
        if name.is_none() {
            p.metadata.name = None;
        }
        p
    }

    #[tokio::test]
    async fn owned_process_binding_returns_ns_name_api_in_that_order() {
        // The tuple ordering is load-bearing — every callsite destructures
        // `let (ns, name, api) = ctx.owned_process_binding(&p)?;` and passes
        // `&api` / `&name` / `&ns` unchanged to `patch::transition`,
        // `patch::remove_finalizer`, log fields, and error wording. A
        // regression that swapped the tuple axes would collide with the
        // downstream consumers' type expectations at rustc time OR misroute
        // a String pair silently.
        let c = ctx();
        let p = process_with(Some("acme-prod"), Some("obs-stack"));
        let (ns, name, api) = c.owned_process_binding(&p).expect("both slots present");
        assert_eq!(ns, "acme-prod");
        assert_eq!(name, "obs-stack");
        assert!(api.resource_url().contains("/namespaces/acme-prod/"));
        assert!(api.resource_url().ends_with("/processes"));
    }

    #[tokio::test]
    async fn owned_process_binding_forwards_namespace_into_process_api_peer() {
        // The composer routes its `ns` slot into `Context::process_api`
        // rather than manufacturing a second Api-builder path. Pin: the
        // composed Api matches the peer's Api on the SAME namespace,
        // proving no drift between the two Api-construction paths.
        let c = ctx();
        let p = process_with(Some("scoped-ns"), Some("some-proc"));
        let (_, _, composed) = c.owned_process_binding(&p).unwrap();
        let peer = c.process_api("scoped-ns");
        assert_eq!(
            composed.resource_url(),
            peer.resource_url(),
            "composed Api must match the process_api peer on the SAME namespace"
        );
    }

    #[tokio::test]
    async fn owned_process_binding_falls_back_to_default_namespace_matching_peer_wording() {
        // A Process with no `.metadata.namespace` falls through
        // `Process::owned_coordinates_or_err`'s substrate fallback (see
        // `owned_coordinates_or_err_namespace_fallback_matches_default_namespace_const`
        // pin in tatara-process). The composer preserves that fallback
        // shape — no divergence between the composed path and the peer
        // path on the "missing namespace" axis.
        let c = ctx();
        let p = process_with(None, Some("some-proc"));
        let (ns, _, api) = c
            .owned_process_binding(&p)
            .expect("name present → composer succeeds");
        // Peer axis: `owned_coordinates_or_err` alone on the same shape.
        let (peer_ns, _) = p.owned_coordinates_or_err().unwrap();
        assert_eq!(
            ns, peer_ns,
            "composed ns must match the substrate peer's fallback"
        );
        assert!(
            api.resource_url().contains(&format!("/namespaces/{ns}/")),
            "composed api must carry the fallback namespace verbatim; got {}",
            api.resource_url()
        );
    }

    #[tokio::test]
    async fn owned_process_binding_propagates_owned_coordinates_error_verbatim() {
        // The composer forwards `owned_coordinates_or_err`'s error via
        // `?` — no re-wording, no map_err. A regression that softened the
        // error shape (`.unwrap_or_default()`, a swallowed match arm,
        // or a re-wrapped anyhow) would surface here as a divergence
        // between the composed error and the peer's error.
        let c = ctx();
        let p = process_with(Some("some-ns"), None);
        let composed_err = c.owned_process_binding(&p).unwrap_err().to_string();
        let peer_err = p.owned_coordinates_or_err().unwrap_err().to_string();
        assert_eq!(
            composed_err, peer_err,
            "composed error must match the substrate peer's error verbatim"
        );
    }

    #[tokio::test]
    async fn processes_all_api_and_process_table_api_bind_distinct_collections() {
        // Both cluster-scoped peers share the client-slot + scope
        // (`Api::all`) + the `tatara.pleme.io/v1alpha1` group but must
        // resolve to DISTINCT collections — `/processes` vs
        // `/processtables`. A regression that conflated the two typed
        // returns (e.g. a copy-paste that pointed
        // `processes_all_api` at `ProcessTable` instead of `Process`)
        // would collapse both urls onto the same collection and
        // silently misroute every cluster-wide Process consumer to
        // the ProcessTable endpoint.
        let c = ctx();
        let processes = c.processes_all_api().resource_url().to_string();
        let table = c.process_table_api().resource_url().to_string();
        assert!(
            processes.ends_with("/processes"),
            "processes_all_api → /processes; got {processes}"
        );
        assert!(
            table.ends_with("/processtables"),
            "process_table_api → /processtables; got {table}"
        );
        assert_ne!(
            processes, table,
            "the two cluster-scoped peer primitives must resolve to distinct collections"
        );
    }
}
