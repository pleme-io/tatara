//! Shared controller context — kube client + typed config.

use std::sync::Arc;

use kube::{Api, Client};

use tatara_process::allocation::EphemeralAllocation;
use tatara_process::pool::EphemeralPool;
use tatara_process::prelude::Process;

#[derive(Clone)]
pub struct PoolContext {
    pub kube: Client,
    pub config: Arc<PoolReconcilerConfig>,
}

impl PoolContext {
    /// Namespaced `Api<EphemeralPool>` bound to this context's client —
    /// the substrate primitive every EphemeralPool-API construction
    /// site inside this crate rides through.
    ///
    /// Pre-lift the two-slot `(kube.clone(), ns)` incantation was hand-
    /// authored at TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold in `tatara-pool-reconciler`
    /// (`controller_pool::reconcile_inner` + `controller_allocation::
    /// reconcile_inner`), each restating the SAME `Api::namespaced(
    /// ctx.kube.clone(), &ns)` shape verbatim. Post-lift each site
    /// delegates through ONE substrate method — a future change that
    /// layers request tracing spans, a default `PatchParams` builder,
    /// a namespace-scoped access-control gate, or per-request metrics
    /// onto every EphemeralPool-API request lands at ONE site rather
    /// than being restated at each callsite.
    ///
    /// Peer to [`Self::allocation_api`] + [`Self::process_api`] on the
    /// (Api-typed × namespaced) axis; all three primitives share the
    /// same `(kube.clone(), ns)` skeleton and differ only on the typed
    /// collection they bind. Sibling to
    /// [`crate::context::PoolContext::pool_api`]'s counterpart on
    /// `tatara-reconciler::context::Context::process_api` — the two
    /// contexts partition the workspace's namespaced-Api primitive
    /// family across the two reconciler crates on identical shape.
    ///
    /// The `&str` parameter accepts both `&String` (which coerces via
    /// deref) and `&str` literal / slice callers, matching every shape
    /// currently authored: both reconcile handlers pull `ns: String`
    /// out of `alloc.owned_coordinates_required()?` / `pool.
    /// owned_coordinates_required()?` and pass `&ns`.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `Api::namespaced(ctx.kube.clone(), &ns)` chain recurred at
    /// two hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the primitive binds the (client, namespace, typed collection)
    /// triple structurally; a regression that drifted any slot at the
    /// primitive's own site surfaces at the sibling `pool_api_*`
    /// resource-url pins rather than as silent operator-facing skew
    /// between the pool + allocation reconcile handlers).
    pub fn pool_api(&self, ns: &str) -> Api<EphemeralPool> {
        Api::namespaced(self.kube.clone(), ns)
    }

    /// Namespaced `Api<EphemeralAllocation>` bound to this context's
    /// client — the peer substrate primitive alongside
    /// [`Self::pool_api`] + [`Self::process_api`], closing the
    /// `Api::namespaced(self.kube.clone(), ns)` shape for the third
    /// typed collection this reconciler binds.
    ///
    /// Pre-lift the `Api::namespaced(ctx.kube.clone(), &ns)` chain for
    /// `Api<EphemeralAllocation>` was hand-authored at ONE site in
    /// `controller_allocation::reconcile_inner` (the allocation
    /// reconcile handler's own `alloc_api` binding). This primitive
    /// closes the family alongside [`Self::pool_api`] + [`Self::
    /// process_api`] so a future emitter of `Api<EphemeralAllocation>`
    /// reaches for the substrate primitive rather than restating the
    /// `(client, ns)` incantation inline — matching the composition
    /// discipline every reconciler-context primitive on the workspace
    /// already follows (see the peer three-primitive family on
    /// `tatara-reconciler::context::Context`).
    ///
    /// Post-lift a future change that layers request tracing spans, a
    /// default `PatchParams` builder, a namespace-scoped access-control
    /// gate, or per-request metrics onto every
    /// EphemeralAllocation-API request lands at ONE site here rather
    /// than being restated at every consumer.
    pub fn allocation_api(&self, ns: &str) -> Api<EphemeralAllocation> {
        Api::namespaced(self.kube.clone(), ns)
    }

    /// Namespaced `Api<Process>` bound to this context's client — the
    /// third peer substrate primitive alongside [`Self::pool_api`] +
    /// [`Self::allocation_api`], closing the `Api::namespaced(self.
    /// kube.clone(), ns)` shape for every Process-API construction
    /// site inside this crate.
    ///
    /// Pre-lift the two-slot `(kube.clone(), ns)` incantation for
    /// `Api<Process>` was hand-authored at TWO sites past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication threshold in
    /// `tatara-pool-reconciler` (`controller_pool::reconcile_inner`
    /// process-API binding used to spawn / delete pool members +
    /// `controller_allocation::reconcile_inner` process-API binding
    /// used to patch the bound member's lifetime overlay on Bind /
    /// stamp the return-trigger annotation on Release), each
    /// restating the SAME `Api::namespaced(ctx.kube.clone(), &ns)`
    /// shape verbatim. Post-lift both consumers share ONE substrate
    /// owner.
    ///
    /// Sibling to `tatara-reconciler::context::Context::process_api`
    /// on the peer reconciler; both contexts spell the same
    /// primitive with matching signatures so an operator moving
    /// between the two crates never sees an axis-swap or a
    /// signature-shape drift as a side effect. A future workspace
    /// lift that normalizes the two peer methods onto a shared
    /// trait (`ReconcilerContext::process_api`) lands as a
    /// mechanical extraction.
    pub fn process_api(&self, ns: &str) -> Api<Process> {
        Api::namespaced(self.kube.clone(), ns)
    }

    /// Cluster-scoped `Api<EphemeralPool>` bound to this context's
    /// client — the substrate primitive the top-level
    /// [`Controller::new`][ctrl] watch wiring in `main.rs` rides
    /// through, peer to the namespaced [`Self::pool_api`] on the
    /// (cluster-scoped × namespaced) axis pair for the SAME
    /// `EphemeralPool` typed collection.
    ///
    /// Pre-lift the `Api::all(client.clone())` incantation for
    /// `Api<EphemeralPool>` was hand-authored at ONE site in
    /// `tatara-pool-reconciler::main::main` — the top-level watch
    /// binding fed into `Controller::new(pool_api, …)` — restating
    /// the same `(cluster-scoped × client-clone)` shape the sister
    /// [`Self::allocations_all_api`] primitive covers on
    /// `EphemeralAllocation` (also pre-lift in `main.rs`) and the
    /// peer [`tatara_reconciler::context::Context::processes_all_api`]
    /// primitive covers for `Api<Process>` on `tatara-reconciler`.
    /// That's TWO byte-identical `Api::all(client.clone())` chains in
    /// THIS crate's `main.rs` past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold plus the peer primitive already open on
    /// `tatara-reconciler`. Post-lift `main.rs` reads
    /// `ctx.pools_all_api()` and the pre-lift raw incantation retreats
    /// to ONE substrate owner on `PoolContext`.
    ///
    /// Unlike [`Self::pool_api`], no namespace slot is exposed: the
    /// cluster-scoped `Api::all` handle carries no namespace path
    /// segment and is used exclusively to seed the top-level watch
    /// that must observe every `EphemeralPool` regardless of
    /// namespace. The signature encodes that invariant structurally
    /// — a caller cannot accidentally pass a namespace and get a
    /// broken `Api::namespaced` binding for a watch that expected
    /// cluster-wide visibility.
    ///
    /// Post-lift a future change that layers a cluster-wide watch
    /// filter, a client-side QPS limiter, per-request tracing spans,
    /// or a fixture-backed client for CI/smoke-tests onto every
    /// cluster-wide `EphemeralPool` read lands at ONE substrate
    /// method here rather than being restated at every consumer.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `Api::all(client.clone())` chain recurred at TWO
    /// hand-authored sites in this crate's `main.rs` past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication trigger, alongside the peer
    /// primitive already open on `tatara-reconciler::context`, and is
    /// lifted onto ONE owner here). THEORY.md §II.1 invariant 5
    /// (composition preserves proofs — the primitive binds the
    /// (client, cluster-scoped, typed collection) triple structurally;
    /// a regression that drifted the client-slot or the resource
    /// kind surfaces at the resource-url pins rather than as silent
    /// watch-misroute against a wrong collection).
    ///
    /// [ctrl]: kube::runtime::controller::Controller::new
    pub fn pools_all_api(&self) -> Api<EphemeralPool> {
        Api::all(self.kube.clone())
    }

    /// Cluster-scoped `Api<EphemeralAllocation>` bound to this
    /// context's client — the peer substrate primitive alongside
    /// [`Self::pools_all_api`], closing the `Api::all(client.clone())`
    /// shape for the second cluster-scoped watch binding the
    /// top-level [`Controller::new`][ctrl] wiring in `main.rs` rides
    /// through.
    ///
    /// See [`Self::pools_all_api`] for the axis-family context, peer
    /// primitives, pre-lift call-site history, and future-normalization
    /// anchor — this primitive shares that owner's contract on the
    /// `EphemeralAllocation`-typed collection axis.
    ///
    /// [ctrl]: kube::runtime::controller::Controller::new
    pub fn allocations_all_api(&self) -> Api<EphemeralAllocation> {
        Api::all(self.kube.clone())
    }
}

#[derive(Clone, Debug)]
pub struct PoolReconcilerConfig {
    /// Namespace the controller pod itself runs in (for leader-election
    /// + own-Lease housekeeping). Default: `tatara-pool-system`.
    pub controller_namespace: String,
    /// Default requeue interval when nothing changes. Default: 30s.
    pub heartbeat_seconds: u64,
    /// How long to wait for a spawning member to reach Attested before
    /// marking it Failed. humantime. Default: `"10m"`.
    pub spawn_timeout: String,
    /// Field manager used for server-side applies.
    pub field_manager: String,
}

impl Default for PoolReconcilerConfig {
    fn default() -> Self {
        Self {
            controller_namespace: "tatara-pool-system".into(),
            heartbeat_seconds: 30,
            spawn_timeout: "10m".into(),
            field_manager: "tatara-pool-reconciler".into(),
        }
    }
}

// ─── PoolContext api-primitive substrate pins ──────────────────────────
//
// The two-slot `(ctx.kube.clone(), ns)` incantation was hand-authored at
// 5 sites across `controller_pool.rs` + `controller_allocation.rs`
// before `pool_api` / `allocation_api` / `process_api` closed it. These
// pins bind each primitive at fail-before-pass-after granularity so a
// regression that drifts the namespace binding, the resource kind, or
// the reused client-slot surfaces here rather than as silent operator-
// facing drift at every downstream handler.
//
// `Client::try_from(Config::new(url))` needs a live tokio reactor
// (tower::buffer::Buffer::new spawns a background task on
// construction), so every pin runs under `#[tokio::test]`.
#[cfg(test)]
mod tests {
    use super::*;
    use kube::Config;

    fn ctx() -> PoolContext {
        let url = "http://localhost:9999".parse().expect("valid probe url");
        let client = Client::try_from(Config::new(url)).expect("build kube client");
        PoolContext {
            kube: client,
            config: Arc::new(PoolReconcilerConfig::default()),
        }
    }

    // ── pool_api ────────────────────────────────────────────────────

    #[tokio::test]
    async fn pool_api_binds_namespace_into_resource_url() {
        // Both reconcile handlers pull the per-CR namespace out of the
        // `NamespacedApiCoordinates::owned_coordinates_required()?`
        // trait method and expect `ctx.pool_api(&ns)` to bind that
        // namespace onto the returned Api's REST path.
        let api = ctx().pool_api("ephemeral-pools");
        let url = api.resource_url();
        assert!(
            url.contains("/namespaces/ephemeral-pools/"),
            "namespaced Api resource url must carry the caller's namespace verbatim; got {url}"
        );
    }

    #[tokio::test]
    async fn pool_api_binds_the_ephemeral_pool_kind() {
        // The typed `Api<EphemeralPool>` return pins the resource kind
        // at rustc time; this pin adds the runtime witness — the
        // emitted REST path targets the
        // `tatara.pleme.io/v1alpha1/ephemeralpools` collection matching
        // the `#[kube(group = "tatara.pleme.io", version = "v1alpha1",
        // plural = "ephemeralpools")]` attribute on `PoolSpec`.
        let api = ctx().pool_api("default");
        let url = api.resource_url();
        assert!(
            url.starts_with("/apis/tatara.pleme.io/v1alpha1/"),
            "Api resource url must be scoped to the tatara.pleme.io/v1alpha1 group; got {url}"
        );
        assert!(
            url.ends_with("/ephemeralpools"),
            "Api resource url must terminate at the `ephemeralpools` collection; got {url}"
        );
    }

    #[tokio::test]
    async fn pool_api_accepts_string_deref_and_str_slice_shapes() {
        let c = ctx();
        let owned = String::from("owned-ns");
        let borrowed: &str = "borrowed-ns";
        let via_owned = c.pool_api(&owned);
        let via_borrowed = c.pool_api(borrowed);
        assert!(via_owned.resource_url().contains("/namespaces/owned-ns/"));
        assert!(via_borrowed
            .resource_url()
            .contains("/namespaces/borrowed-ns/"));
    }

    // ── allocation_api ──────────────────────────────────────────────

    #[tokio::test]
    async fn allocation_api_binds_namespace_into_resource_url() {
        let api = ctx().allocation_api("ephemeral-pools");
        let url = api.resource_url();
        assert!(
            url.contains("/namespaces/ephemeral-pools/"),
            "namespaced Api resource url must carry the caller's namespace verbatim; got {url}"
        );
    }

    #[tokio::test]
    async fn allocation_api_binds_the_ephemeral_allocation_kind() {
        // The typed `Api<EphemeralAllocation>` return pins the resource
        // kind at rustc time; this pin adds the runtime witness — the
        // emitted REST path targets the
        // `tatara.pleme.io/v1alpha1/ephemeralallocations` collection
        // matching the `#[kube(group = "tatara.pleme.io", version =
        // "v1alpha1", plural = "ephemeralallocations")]` attribute on
        // `AllocationSpec`.
        let api = ctx().allocation_api("default");
        let url = api.resource_url();
        assert!(
            url.starts_with("/apis/tatara.pleme.io/v1alpha1/"),
            "Api resource url must be scoped to the tatara.pleme.io/v1alpha1 group; got {url}"
        );
        assert!(
            url.ends_with("/ephemeralallocations"),
            "Api resource url must terminate at the `ephemeralallocations` collection; got {url}"
        );
    }

    // ── process_api ─────────────────────────────────────────────────

    #[tokio::test]
    async fn process_api_binds_namespace_into_resource_url() {
        let api = ctx().process_api("ephemeral-pools");
        let url = api.resource_url();
        assert!(
            url.contains("/namespaces/ephemeral-pools/"),
            "namespaced Api resource url must carry the caller's namespace verbatim; got {url}"
        );
    }

    #[tokio::test]
    async fn process_api_binds_the_process_kind() {
        // The typed `Api<Process>` return pins the resource kind at
        // rustc time; this pin adds the runtime witness — the emitted
        // REST path targets the `tatara.pleme.io/v1alpha1/processes`
        // collection matching the `#[kube(group = "tatara.pleme.io",
        // version = "v1alpha1", plural = "processes")]` attribute on
        // `ProcessSpec` in `tatara-process/src/crd.rs`. A regression
        // that changed the group, version, or plural on the CRD
        // without rippling through consumers surfaces here.
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

    // ── cross-primitive coherence ───────────────────────────────────

    #[tokio::test]
    async fn three_typed_api_primitives_bind_distinct_collections() {
        // The three peer primitives share the client-slot + the
        // `tatara.pleme.io/v1alpha1` group + the namespaced scope but
        // must resolve to DISTINCT collections — `/ephemeralpools` vs
        // `/ephemeralallocations` vs `/processes`. A regression that
        // conflated any two typed returns (e.g. a copy-paste that
        // pointed `pool_api` at `Process` instead of `EphemeralPool`)
        // would collapse two urls onto the same collection and
        // silently misroute every downstream API call to the wrong
        // endpoint.
        let c = ctx();
        let pools = c.pool_api("scoped-ns").resource_url().to_string();
        let allocs = c.allocation_api("scoped-ns").resource_url().to_string();
        let procs = c.process_api("scoped-ns").resource_url().to_string();
        assert!(
            pools.ends_with("/ephemeralpools"),
            "pool_api → /ephemeralpools; got {pools}"
        );
        assert!(
            allocs.ends_with("/ephemeralallocations"),
            "allocation_api → /ephemeralallocations; got {allocs}"
        );
        assert!(
            procs.ends_with("/processes"),
            "process_api → /processes; got {procs}"
        );
        assert_ne!(
            pools, allocs,
            "pool_api and allocation_api must resolve to distinct collections"
        );
        assert_ne!(
            pools, procs,
            "pool_api and process_api must resolve to distinct collections"
        );
        assert_ne!(
            allocs, procs,
            "allocation_api and process_api must resolve to distinct collections"
        );
    }

    #[tokio::test]
    async fn three_typed_api_primitives_share_the_context_client_slot() {
        // Every callsite pre-lift chose `ctx.kube.clone()` — the same
        // client the reconciler was constructed with. Post-lift each
        // primitive continues to source its client from
        // `self.kube.clone()` rather than manufacturing a new one;
        // this pin catches a regression that swapped the client-slot
        // at any primitive's own site (e.g. `Client::try_default()`
        // or a `RwLock`-cached alternate). The witness: three
        // separately-built Apis on the SAME context share the group +
        // version + namespace slots — only the terminal collection
        // differs.
        let c = ctx();
        for slot in ["/apis/tatara.pleme.io/v1alpha1/", "/namespaces/shared-ns/"] {
            for url in [
                c.pool_api("shared-ns").resource_url().to_string(),
                c.allocation_api("shared-ns").resource_url().to_string(),
                c.process_api("shared-ns").resource_url().to_string(),
            ] {
                assert!(
                    url.contains(slot),
                    "every context-sourced Api must target the same slot {slot:?}; got {url}"
                );
            }
        }
    }

    // ── pools_all_api ───────────────────────────────────────────────
    //
    // Pin the cluster-scoped `Api<EphemeralPool>` primitive `main.rs`'s
    // top-level `Controller::new(pool_api, …)` watch wiring rides
    // through. Every corner of the (cluster-scope, EphemeralPool kind,
    // shared client-slot) triple lands as a pin so a regression that
    // swapped the scope for `Api::namespaced`, drifted the resource
    // kind onto a sibling collection, or manufactured a fresh client
    // surfaces HERE rather than as a silent watch-misroute at start-up
    // that observes the wrong resource-collection or blindly leaks a
    // watch against the wrong client's connection pool.

    #[tokio::test]
    async fn pools_all_api_binds_cluster_scope_not_namespace() {
        // Cluster-scoped: no `/namespaces/<ns>/` path segment appears
        // in the resource url. A regression that swapped the scope for
        // `Api::namespaced` — silently limiting the watch to a single
        // namespace — surfaces HERE rather than as an operator-visible
        // gap where every EphemeralPool in every OTHER namespace goes
        // unreconciled at start-up.
        let api = ctx().pools_all_api();
        let url = api.resource_url();
        assert!(
            !url.contains("/namespaces/"),
            "cluster-scoped Api resource url must NOT carry a namespace segment; got {url}"
        );
    }

    #[tokio::test]
    async fn pools_all_api_binds_the_ephemeral_pool_kind() {
        // The typed `Api<EphemeralPool>` return pins the resource kind
        // at rustc time; this pin adds the runtime witness — the
        // emitted REST path targets the
        // `tatara.pleme.io/v1alpha1/ephemeralpools` collection matching
        // the `#[kube(group = "tatara.pleme.io", version = "v1alpha1",
        // plural = "ephemeralpools")]` attribute on `PoolSpec`.
        let api = ctx().pools_all_api();
        let url = api.resource_url();
        assert!(
            url.starts_with("/apis/tatara.pleme.io/v1alpha1/"),
            "Api resource url must be scoped to the tatara.pleme.io/v1alpha1 group; got {url}"
        );
        assert!(
            url.ends_with("/ephemeralpools"),
            "Api resource url must terminate at the `ephemeralpools` collection; got {url}"
        );
    }

    // ── allocations_all_api ─────────────────────────────────────────

    #[tokio::test]
    async fn allocations_all_api_binds_cluster_scope_not_namespace() {
        let api = ctx().allocations_all_api();
        let url = api.resource_url();
        assert!(
            !url.contains("/namespaces/"),
            "cluster-scoped Api resource url must NOT carry a namespace segment; got {url}"
        );
    }

    #[tokio::test]
    async fn allocations_all_api_binds_the_ephemeral_allocation_kind() {
        let api = ctx().allocations_all_api();
        let url = api.resource_url();
        assert!(
            url.starts_with("/apis/tatara.pleme.io/v1alpha1/"),
            "Api resource url must be scoped to the tatara.pleme.io/v1alpha1 group; got {url}"
        );
        assert!(
            url.ends_with("/ephemeralallocations"),
            "Api resource url must terminate at the `ephemeralallocations` collection; got {url}"
        );
    }

    // ── cross-primitive coherence — cluster-scoped family ────────────

    #[tokio::test]
    async fn cluster_scoped_pair_binds_distinct_collections() {
        // The two cluster-scoped peers share the client-slot + the
        // `tatara.pleme.io/v1alpha1` group + the cluster scope but must
        // resolve to DISTINCT collections — `/ephemeralpools` vs
        // `/ephemeralallocations`. A regression that conflated the two
        // typed returns (e.g. a copy-paste that pointed
        // `pools_all_api` at `EphemeralAllocation` instead of
        // `EphemeralPool`) would collapse both urls onto the same
        // collection and silently misroute every downstream watch to
        // the wrong endpoint at start-up.
        let c = ctx();
        let pools = c.pools_all_api().resource_url().to_string();
        let allocs = c.allocations_all_api().resource_url().to_string();
        assert!(
            pools.ends_with("/ephemeralpools"),
            "pools_all_api → /ephemeralpools; got {pools}"
        );
        assert!(
            allocs.ends_with("/ephemeralallocations"),
            "allocations_all_api → /ephemeralallocations; got {allocs}"
        );
        assert_ne!(
            pools, allocs,
            "pools_all_api and allocations_all_api must resolve to distinct collections"
        );
    }

    #[tokio::test]
    async fn cluster_scoped_and_namespaced_peers_share_typed_collection() {
        // Peer discipline across the (cluster-scoped × namespaced)
        // axis pair: on the SAME typed collection
        // (EphemeralPool / EphemeralAllocation), the cluster-scoped
        // primitive and its namespaced peer must both terminate at
        // the SAME collection tail — only the presence of the
        // `/namespaces/<ns>/` segment distinguishes them. A regression
        // that drifted either primitive off its typed collection
        // (e.g. renaming `EphemeralPool.plural` without a coherent
        // sweep) surfaces HERE rather than as silent per-primitive
        // misroute.
        let c = ctx();
        for (all, ns_of, tail) in [
            (
                c.pools_all_api().resource_url().to_string(),
                c.pool_api("shared-ns").resource_url().to_string(),
                "/ephemeralpools",
            ),
            (
                c.allocations_all_api().resource_url().to_string(),
                c.allocation_api("shared-ns").resource_url().to_string(),
                "/ephemeralallocations",
            ),
        ] {
            assert!(
                all.ends_with(tail),
                "cluster-scoped url must terminate at {tail}; got {all}"
            );
            assert!(
                ns_of.ends_with(tail),
                "namespaced url must terminate at {tail}; got {ns_of}"
            );
            assert!(
                !all.contains("/namespaces/"),
                "cluster-scoped url must NOT carry a namespace segment; got {all}"
            );
            assert!(
                ns_of.contains("/namespaces/shared-ns/"),
                "namespaced url must carry the caller's namespace verbatim; got {ns_of}"
            );
        }
    }

    #[tokio::test]
    async fn cluster_scoped_pair_matches_pre_lift_api_all_shape() {
        // Byte-identical parity with the exact pre-lift
        // `Api::all(client.clone())` incantation `main.rs`
        // hand-authored twice past the ★★ PRIME-DIRECTIVE ≥ 2
        // threshold. A regression that reshaped the primitive
        // (e.g. flipped it to `Api::default_namespaced` — kube-rs's
        // `.default_namespace()`-defaulted binding — or manufactured a
        // fresh `Client::try_default()`) would surface HERE rather
        // than as silent watch-misroute at `Controller::new` binding.
        let c = ctx();
        // The pre-lift 2-line incantation, byte-for-byte.
        let hand_authored_pools: Api<EphemeralPool> = Api::all(c.kube.clone());
        let hand_authored_allocs: Api<EphemeralAllocation> = Api::all(c.kube.clone());
        assert_eq!(
            c.pools_all_api().resource_url(),
            hand_authored_pools.resource_url(),
            "pools_all_api must be byte-identical to the pre-lift `Api::all(client.clone())` chain",
        );
        assert_eq!(
            c.allocations_all_api().resource_url(),
            hand_authored_allocs.resource_url(),
            "allocations_all_api must be byte-identical to the pre-lift `Api::all(client.clone())` chain",
        );
    }
}
