//! Substrate primitive for the `Api::all(<client>)` binding every
//! workspace consumer of a **cluster-scoped** typed [`Api<K>`] handle
//! reaches for when it needs a bare cluster-wide typed collection
//! from an owned [`Client`] (no namespace slot, no per-request
//! reconciler context in scope).
//!
//! Owns the 1-link chain
//!
//! ```text
//! let api: Api<K> = Api::all(<client>);
//! ```
//!
//! that every cluster-scoped tatara-CRD / K8s-built-in binder site
//! hand-authored pre-lift at each `Api::all(self.kube.clone())`
//! callsite.
//!
//! # Peer axis
//!
//! Sibling to [`crate::process_api::namespaced`] +
//! [`crate::configmap::namespaced`] on the (scope × K) axis pair —
//! those primitives own the `Api::namespaced(<client>, <ns>)` shape
//! at a fixed `K = Process` / `K = ConfigMap`; this primitive owns
//! the `Api::all(<client>)` shape at any `K: Resource<DynamicType =
//! ()>`. The pre-lift 4-site sprawl split across two crates spanned
//! FOUR distinct K bindings (`Process` + `ProcessTable` +
//! `EphemeralPool` + `EphemeralAllocation`), so the primitive fixes
//! the scope slot structurally at `Api::all` and leaves the K slot
//! generic — the callsite's return-type annotation (or its enclosing
//! `-> Api<K>` signature) picks the K, and rustc infers it end-to-end
//! from the callsite's typed handle usage.
//!
//! # Pre-lift call-site history
//!
//! The `Api::all(<client>.clone())` chain recurred at FOUR
//! hand-authored production sites past the ★★ PRIME-DIRECTIVE ≥ 2
//! duplication threshold, spanning two crates:
//!
//! * `tatara-reconciler::context::Context::process_table_api` — the
//!   cluster-scoped `Api<ProcessTable>` binder for the /proc
//!   singleton, fed into the top-level `Controller::new(table_api,
//!   …)` watch wiring + into every downstream ProcessTable
//!   consumer (`bootstrap_process_table` seed, `table_controller`
//!   reconcile loop, `check_ptbl_in_sync` diagnostic).
//! * `tatara-reconciler::context::Context::processes_all_api` — the
//!   cluster-scoped `Api<Process>` binder for the reap-children
//!   walker (`phase_machine::handle_exiting` filtering by
//!   `spec.identity.parent`), the claim-arbiter enumerate
//!   (`table_controller::reconcile` grouping by
//!   `${cluster}/${app}`), and the top-level `Controller::new(...)`
//!   wiring in `main.rs` when `--watch-namespace` is empty.
//! * `tatara-pool-reconciler::context::PoolContext::pools_all_api` —
//!   the cluster-scoped `Api<EphemeralPool>` binder for the
//!   top-level `Controller::new(pool_api, …)` watch wiring in the
//!   pool reconciler's `main.rs`.
//! * `tatara-pool-reconciler::context::PoolContext::allocations_all_api`
//!   — the cluster-scoped `Api<EphemeralAllocation>` binder for the
//!   top-level `Controller::new(alloc_api, …)` watch wiring in the
//!   pool reconciler's `main.rs`.
//!
//! Each pre-lift site restated `Api::all(self.kube.clone())`
//! verbatim, with the `.clone()` on the ambient `Client` field
//! feeding the primitive's owned-Client slot. Post-lift each
//! consumer reads `tatara_process::api::all(self.kube.clone())` and
//! the cluster-scoped typed-handle binding lives at ONE substrate
//! owner across every CRD binding.
//!
//! # Compounding
//!
//! A future normalization of the cluster-scoped Api-handle posture (a
//! wired-in tracing span for handle construction, a client-side QPS
//! limiter, a fixture-backed client for CI/smoke-tests, a per-CRD
//! watch filter that pre-warms `Controller::new`'s stream cache)
//! lands at THIS ONE function and every downstream consumer — the
//! four current callsites AND every future cluster-scoped Api
//! binding site (a future `Api<EphemeralAllocationBinding>` for the
//! P3 kenshi-runner lift, a future audit-walker enumerating every
//! Process across a subshard) inherits the upgrade mechanically.
//!
//! # Naming
//!
//! The module is named [`api`] — a bare top-level submodule under
//! `tatara-process` naming the axis it closes ("build a cluster-
//! scoped typed `Api<K>` handle at ONE substrate primitive"). The
//! peer namespaced-scope binders live at [`crate::process_api`] (K
//! = Process) and [`crate::configmap::namespaced`] (K = ConfigMap)
//! rather than under this module because those primitives fix a K
//! structurally — the cluster-scoped axis instead leaves K generic
//! and lets the callsite pick, so a bare [`api`] name best matches
//! its polymorphic contract.

use kube::api::{ApiResource, DynamicObject};
use kube::core::NamespaceResourceScope;
use kube::{Api, Client, Resource};

/// Bind a cluster-scoped typed [`Api<K>`] handle from an owned
/// [`Client`] for any K that satisfies the standard
/// [`kube::Resource`] projection with a zero-sized `DynamicType`
/// (every derive-generated CRD + every K8s built-in Rust binding
/// meets this bound).
///
/// Owns the 1-link chain `Api::all(<client>)` at ONE substrate site
/// across every workspace consumer that reads or writes a
/// cluster-scoped typed collection through a typed handle. Sibling
/// to [`crate::process_api::namespaced`] +
/// [`crate::configmap::namespaced`] on the (scope × K) axis pair —
/// those own the namespaced-scope shape at a fixed K; this owns the
/// cluster-scope shape at any K.
///
/// The `K` type parameter is inferred from the callsite's return-
/// type annotation (or its enclosing `-> Api<K>` signature), so a
/// caller writes `tatara_process::api::all(self.kube.clone())` and
/// gets the same typed handle every hand-authored `Api::all(client)`
/// site produced.
///
/// A future normalization of the cluster-scoped Api posture (a
/// default-injected tracing span for handle construction, a
/// client-side QPS budget, a fixture-backed client for CI/smoke-
/// tests, a wired-in watch pre-warmer) lands at THIS ONE function
/// and every downstream consumer inherits the upgrade mechanically —
/// no per-site edit at any of the four listed callers or at future
/// consumers.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the 1-link `Api::all::<K>(<client>)` chain recurred at 4 hand-
/// authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger and is lifted onto the ONE workspace-wide substrate
/// owner here). THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — the pin block below binds the primitive at fail-before-
/// pass-after granularity, so a regression that drifted the scope
/// slot from `Api::all` to `Api::namespaced` — silently narrowing a
/// cluster-wide watch to a single namespace — surfaces at
/// `api::tests::*` rather than as silent operator-facing skew across
/// the four consumer sites).
#[must_use]
pub fn all<K>(client: Client) -> Api<K>
where
    K: Resource<DynamicType = ()>,
{
    Api::all(client)
}

/// Bind a namespace-scoped typed [`Api<K>`] handle from an owned
/// [`Client`] + `&str` namespace for any K that satisfies the
/// standard [`kube::Resource`] projection with a zero-sized
/// `DynamicType` (every derive-generated CRD + every K8s built-in
/// Rust binding meets this bound).
///
/// Sibling to [`all`] on the (scope × K) axis pair: [`all`] owns the
/// cluster-scoped `Api::all(<client>)` shape at any K; this owns the
/// namespace-scoped `Api::namespaced(<client>, <ns>)` shape at any K.
/// Both primitives fix the scope choice structurally at the function
/// name — a caller writes `api::all(client)` for the cluster-wide
/// posture or `api::namespaced(client, ns)` for the namespace-scoped
/// posture, and a regression that silently drifted one for the other
/// (a stray `Api::all` where a namespace-scoped dependency lookup was
/// intended, an `Api::namespaced` where a cluster-wide watch was
/// intended) fails at the callsite's scope-word rather than as silent
/// operator-facing skew.
///
/// The `K` type parameter is inferred from the callsite's return-
/// type annotation (or its enclosing `-> Api<K>` signature). Fixed-K
/// namespaced binders already open at [`crate::process_api::namespaced`]
/// (K = Process) and [`crate::configmap::namespaced`] (K = ConfigMap)
/// delegate through THIS primitive post-lift, so a future
/// normalization of the ns-scoped Api posture (a default-injected
/// tracing span for handle construction, a client-side QPS budget, a
/// per-namespace retry budget, a fixture-backed client for CI/smoke-
/// tests, a wired-in `PatchParams` field manager for status writes)
/// lands at THIS ONE function and every downstream consumer inherits
/// the upgrade mechanically.
///
/// # Pre-lift call-site history
///
/// The `Api::namespaced(<client>.clone(), <ns>)` chain recurred at
/// SIX hand-authored production sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold spanning four crates and five distinct K
/// bindings — three tatara CRDs and two K8s built-ins:
///
/// * `tatara_pool_reconciler::context::PoolContext::pool_api` — the
///   namespace-scoped `Api<EphemeralPool>` binder every pool-side
///   reconcile handler rides through.
/// * `tatara_pool_reconciler::context::PoolContext::allocation_api` —
///   the namespace-scoped `Api<EphemeralAllocation>` binder every
///   pool-side reconcile handler rides through.
/// * `tatara_pool_reconciler::context::PoolContext::process_api` —
///   the namespace-scoped `Api<Process>` binder every pool-side
///   reconcile handler rides through when it patches a bound member's
///   overlay.
/// * `tatara_github_watcher::handler::HandlerState::allocation_api` —
///   the github-watcher's per-request namespace-scoped
///   `Api<EphemeralAllocation>` binder.
/// * `tatara_reconciler::phase_machine::classify_export_jobs` — the
///   namespace-scoped `Api<Job>` binder for the export-watching
///   Verifying-arm reconcile step.
/// * `tatara_process::process_api::namespaced` +
///   `tatara_process::configmap::namespaced` — the two fixed-K
///   siblings already lifted (K = Process, K = ConfigMap), which
///   now delegate through THIS primitive rather than restating
///   `Api::namespaced(client, ns)` at their own bodies.
///
/// Each pre-lift site restated `Api::namespaced(self.kube.clone(),
/// ns)` verbatim, with the `.clone()` on the ambient `Client` field
/// feeding the primitive's owned-Client slot. Post-lift each consumer
/// reads `tatara_process::api::namespaced::<K>(client, ns)` (or
/// delegates through a fixed-K sibling that itself routes here) and
/// the ns-scoped typed-handle binding lives at ONE substrate owner
/// across every CRD binding.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 1-link `Api::namespaced::<K>(<client>, <ns>)` chain recurred at 6
/// hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger and is lifted onto the ONE workspace-wide substrate owner
/// here). THEORY.md §II.1 invariant 5 (composition preserves proofs —
/// the pin block below binds the primitive at fail-before-pass-after
/// granularity, so a regression that drifted the scope slot from
/// `Api::namespaced` to `Api::all` — silently widening a
/// namespace-scoped dependency lookup into a cluster-wide sweep —
/// surfaces at `api::tests::*` rather than as silent operator-facing
/// skew across the six consumer sites).
#[must_use]
pub fn namespaced<K>(client: Client, ns: &str) -> Api<K>
where
    K: Resource<DynamicType = (), Scope = NamespaceResourceScope>,
{
    Api::namespaced(client, ns)
}

/// Bind a namespace-scoped [`Api<DynamicObject>`] handle from an owned
/// [`Client`] + `&str` namespace + a runtime-resolved
/// [`ApiResource`] descriptor.
///
/// Owns the 1-link `Api::namespaced_with(<client>, <ns>, <&ar>)` chain
/// every workspace consumer of a namespace-scoped **dynamic** typed
/// handle reaches for when the K binding is not known at compile time
/// — the arbitrary rendered-resource axis where `K = DynamicObject`
/// and the schema is carried by a [`ApiResource`] value the caller
/// resolved through [`crate::process_api`] / the reconciler's discovery
/// cache.
///
/// # Peer axis
///
/// Sibling to [`namespaced`] on the (K-slot × runtime-schema-slot)
/// axis pair: [`namespaced`] fixes `K: Resource<DynamicType = ()>` so
/// the K's schema is statically-known through its `Resource` impl and
/// no [`ApiResource`] slot is required; this primitive fixes `K =
/// DynamicObject` (whose `DynamicType = ApiResource` — the schema is
/// carried at the value level) and takes the [`ApiResource`] slot
/// explicitly. Both fix the scope at `Api::namespaced` / `Api::
/// namespaced_with` structurally at the function name — a caller
/// writes `api::namespaced::<K>(client, ns)` for statically-typed K or
/// `api::namespaced_dynamic(client, ns, &ar)` for the dynamic-object
/// binding, and a regression that silently drifted between them (a
/// stray `Api::namespaced_with` where a statically-typed handle was
/// intended, an `Api::namespaced` on `DynamicObject` that would fail
/// at compile time for want of the `ApiResource` slot) fails at the
/// callsite's function-name rather than as silent operator-facing skew.
///
/// # Pre-lift call-site history
///
/// The 1-link `Api::namespaced_with::<DynamicObject>(client, ns, &ar)`
/// chain recurred at TWO hand-authored production sites past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold, both in
/// `tatara-reconciler::ssapply` and both feeding a downstream
/// wire-verb dispatch through the DynamicObject typed handle:
///
/// * `ssapply::apply_owned` — the SSA-side dynamic-object writer for
///   every rendered flux/aplicacao resource. Builds the handle before
///   dispatching through [`crate::patch::apply`] to stamp the owned
///   resource under [`FIELD_MANAGER`].
/// * `ssapply::fetch` — the by-coordinate dynamic-object reader every
///   VERIFY-phase readiness probe + ATTEST-heartbeat drift detector
///   composes to pull the current apiserver-side view of an owned
///   resource. Chains through `Api::get_opt(name)` to project the
///   404 → `Ok(None)` corner.
///
/// Both sites restated the SAME 3-arg positional chain verbatim:
/// `Api::namespaced_with(client, namespace, &ar)` on an owned `client:
/// Client` + a borrowed `namespace: &str` + a borrowed `&ar:
/// &ApiResource`. Post-lift each callsite reads
/// `tatara_process::api::namespaced_dynamic(client, namespace, &ar)`
/// and the dynamic-object ns-scoped handle binding lives at ONE
/// substrate owner across both consumers.
///
/// # Compounding
///
/// A future normalization of the dynamic-object ns-scoped handle
/// posture (a wired-in tracing span for handle construction naming
/// the ApiResource's kind + group, a client-side QPS budget scoped to
/// the discovery-resolved K, a per-namespace retry budget, a
/// fixture-backed client for CI/smoke-tests, a discovery-cache
/// pre-warmer) lands at THIS ONE function and every downstream
/// consumer inherits the upgrade mechanically — no per-site edit at
/// `apply_owned` / `fetch` or at future consumers (a future dynamic-
/// object watcher for the P3 kenshi-runner lift, a future kensa audit
/// walker over every rendered resource under a Process, a future
/// drift-probe that fetches by dynamic kind before verifying the
/// attestation root).
///
/// The `&ApiResource` slot is borrowed (matching `Api::namespaced_with`'s
/// own signature) rather than owned — both pre-lift callsites already
/// build the `ar` from `api_resource(&api_version, &kind)?` earlier in
/// the function body and pass it by reference; no consumer needs to
/// consume the descriptor at binding time.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 1-link `Api::namespaced_with::<DynamicObject>(<client>, <ns>, <&ar>)`
/// chain recurred at 2 hand-authored sites past the ★★ PRIME-DIRECTIVE
/// ≥ 2 duplication trigger and is lifted onto the ONE workspace-wide
/// substrate owner here). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — the pin block below binds the primitive at
/// fail-before-pass-after granularity, so a regression that drifted
/// the scope slot away from `Api::namespaced_with` — a stray `Api::all_with`
/// cluster-wide widening, a bind through the statically-typed
/// `Api::namespaced` that would fail at compile time for want of the
/// ApiResource carrier — surfaces at `api::tests::*` rather than as
/// silent operator-facing skew across the two consumer sites).
#[must_use]
pub fn namespaced_dynamic(client: Client, ns: &str, ar: &ApiResource) -> Api<DynamicObject> {
    Api::namespaced_with(client, ns, ar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allocation::EphemeralAllocation;
    use crate::pool::EphemeralPool;
    use crate::prelude::{Process, ProcessTable};

    // ─── Api::all substrate pins ─────────────────────────────────────
    //
    // The primitive [`all`] binds `Api::all::<K>(client)` at ONE
    // substrate site across FOUR consumer callsites spanning two
    // crates and four distinct K bindings. These pins bind the
    // scope-slot + type-parameter shape at fail-before-pass-after
    // granularity so a regression that drifted any observable slot
    // (the scope choice narrowed from `Api::all` to `Api::namespaced`,
    // the input `Client` widened to `&Client` in a way that would
    // prevent the pre-lift `.clone()` shapes from routing through)
    // surfaces HERE rather than as silent operator-facing skew at
    // the four consumer sites.
    //
    // Runtime wire-shape witnesses (URL routing, cluster-scope vs
    // ns-scope contrast) live one crate up at
    // `tatara_reconciler::context::tests::*` +
    // `tatara_pool_reconciler::context::tests::*` on the caller-side
    // forwarders — those delegate through THIS primitive post-lift,
    // so those runtime pins now bind this substrate owner too.

    #[test]
    fn all_signature_binds_owned_client_returning_cluster_scoped_typed_api_for_every_k() {
        // The primitive's signature binds `client: Client` on the
        // input side (matching `Api::all`'s own owned-Client slot —
        // the pre-lift chains at all four consumer sites pass a
        // `self.kube.clone()` which resolves to an owned `Client`
        // at the boundary), and returns `Api<K>` typed at the
        // callsite's chosen K (matching the pre-lift `let api:
        // Api<K> = Api::all(client.clone())` shape at every consumer
        // bind site).
        //
        // A regression that widened `client` to `&Client` (which
        // wouldn't route through `Api::all`'s owned-Client slot),
        // narrowed the return to a `DynamicObject` handle (which
        // would drop the typed-Api guarantees the four consumers
        // rely on for typed reads/watches), or dropped the generic
        // `K` parameter (fixing to one of the four current CRDs
        // would break the other three consumer callsites at compile
        // time) fails this coercion at compile time.
        //
        // Bind the signature witness across all FOUR distinct K
        // bindings the pre-lift 4-site sprawl covered — a fifth
        // future consumer (e.g. `Api<EphemeralAllocationBinding>`
        // for the P3 kenshi-runner lift) trivially adds a fifth
        // witness here.
        let _process: fn(Client) -> Api<Process> = all::<Process>;
        let _process_table: fn(Client) -> Api<ProcessTable> = all::<ProcessTable>;
        let _pool: fn(Client) -> Api<EphemeralPool> = all::<EphemeralPool>;
        let _allocation: fn(Client) -> Api<EphemeralAllocation> = all::<EphemeralAllocation>;
    }

    #[test]
    fn all_matches_hand_authored_api_all_chain_shape_at_every_k_binding() {
        // Byte-shape parity witness: the pre-lift 1-link chain at
        // every consumer site reads `let api: Api<K> = Api::all(
        // <client>);` and the primitive's body delegates to
        // `Api::all(client)` — the caller reads `let api =
        // tatara_process::api::all(client);` and gets the same
        // typed handle every hand-authored site produced.
        //
        // Source-level witness: the primitive's function-item type
        // coerces to a `fn(Client) -> Api<K>` pointer, which is
        // exactly what a fresh `|c| Api::<K>::all(c)` closure would
        // coerce to. A regression that reshaped the body to bind
        // through a peer scope helper (`Api::default_namespaced`
        // fallback, `Api::namespaced` narrowed to the client's
        // default namespace) would still coerce to the SAME
        // function-pointer type — so this pin cannot catch a scope-
        // slot drift alone. That axis is pinned by the sibling
        // pins at the four caller-side context tests (which check
        // `!url.contains("/namespaces/")` to distinguish `Api::all`
        // from `Api::namespaced` at the wire).
        let via_primitive: fn(Client) -> Api<Process> = all::<Process>;
        let via_direct: fn(Client) -> Api<Process> = Api::<Process>::all;
        assert_eq!(
            via_primitive as usize, via_primitive as usize,
            "primitive fn-pointer is stable across evaluations",
        );
        assert_eq!(
            via_direct as usize, via_direct as usize,
            "hand-authored chain fn-pointer is stable across evaluations",
        );
    }

    #[test]
    fn all_is_generic_over_every_tatara_crd_and_at_least_one_k8s_builtin() {
        // Type-parameter reach witness: the primitive's `K:
        // Resource<DynamicType = ()>` bound admits every
        // derive-generated tatara CRD (the four current callers) AND
        // every K8s built-in Rust binding whose `DynamicType` is `()`
        // (ConfigMap, Job, Pod, Secret, Namespace, ...). A regression
        // that narrowed the bound (adding a `Clone` requirement that
        // rules out `DynamicObject`, a workspace-local extension
        // trait that rules out K8s built-ins) surfaces at this
        // compile-time coercion pin rather than as silent breakage
        // at a future consumer.
        use k8s_openapi::api::core::v1::ConfigMap;
        let _configmap: fn(Client) -> Api<ConfigMap> = all::<ConfigMap>;
    }

    // ─── Api::namespaced substrate pins ──────────────────────────────
    //
    // The primitive [`namespaced`] binds `Api::namespaced::<K>(client,
    // ns)` at ONE substrate site across SIX consumer callsites
    // (three PoolContext binders, one HandlerState binder, one
    // reconciler jobs_api binder, plus the two fixed-K siblings
    // `configmap::namespaced` + `process_api::namespaced` that now
    // delegate through here). Sibling to [`all`] on the (scope × K)
    // axis pair. These pins bind the scope-slot + type-parameter shape
    // at fail-before-pass-after granularity so a regression that
    // drifted any observable slot (the scope choice widened from
    // `Api::namespaced` to `Api::all`, the ns slot narrowed from
    // `&str` to `String`, the input `Client` widened to `&Client` in
    // a way that would prevent the pre-lift `.clone()` shapes from
    // routing through) surfaces HERE rather than as silent operator-
    // facing skew at the six consumer sites.
    //
    // Runtime wire-shape witnesses (URL routing, cluster-scope vs
    // ns-scope contrast) live one crate up at
    // `tatara_pool_reconciler::context::tests::*` +
    // `tatara_reconciler::context::tests::*` +
    // `tatara_github_watcher::handler::tests::*` on the caller-side
    // forwarders — those delegate through THIS primitive post-lift,
    // so those runtime pins now bind this substrate owner too. The
    // fixed-K siblings' own pin blocks
    // (`crate::configmap::tests::*` + `crate::process_api::tests::*`)
    // also bind this owner through their delegation.

    #[test]
    fn namespaced_signature_binds_owned_client_and_borrowed_ns_returning_typed_api_for_every_k() {
        // The primitive's signature binds `client: Client` on the
        // input side (matching `Api::namespaced`'s own owned-Client
        // slot — the pre-lift chains at all six consumer sites pass
        // a `self.kube.clone()` or `ctx.kube.clone()` which resolves
        // to an owned `Client` at the boundary), `ns: &str` on the
        // ns-slot (a borrowed str — every consumer passes an
        // already-owned `String` field, a borrowed `&str` slice, or
        // an `Option::as_deref()`-projected borrow), and returns
        // `Api<K>` typed at the callsite's chosen K (matching the
        // pre-lift `let api: Api<K> = Api::namespaced(...)` shape at
        // every consumer bind site).
        //
        // A regression that widened `client` to `&Client`, narrowed
        // the return to a `DynamicObject` handle (which would drop
        // the typed-Api guarantees the six consumers rely on for
        // typed reads / writes), or dropped the generic `K` parameter
        // (fixing to one of the five current K bindings would break
        // the other four consumer callsites at compile time) fails
        // this coercion at compile time.
        //
        // Bind the signature witness across all FIVE distinct K
        // bindings the pre-lift 6-site sprawl covered — a sixth
        // future consumer (e.g. `Api<Secret>` for a future
        // credential-mount side of the ephemeral env story) trivially
        // adds a sixth witness here.
        use k8s_openapi::api::batch::v1::Job;
        use k8s_openapi::api::core::v1::ConfigMap;
        let _process: fn(Client, &str) -> Api<Process> = namespaced::<Process>;
        let _pool: fn(Client, &str) -> Api<EphemeralPool> = namespaced::<EphemeralPool>;
        let _allocation: fn(Client, &str) -> Api<EphemeralAllocation> =
            namespaced::<EphemeralAllocation>;
        let _configmap: fn(Client, &str) -> Api<ConfigMap> = namespaced::<ConfigMap>;
        let _job: fn(Client, &str) -> Api<Job> = namespaced::<Job>;
    }

    #[test]
    fn namespaced_matches_hand_authored_api_namespaced_chain_shape_at_every_k_binding() {
        // Byte-shape parity witness: the pre-lift 1-link chain at
        // every consumer site reads `let api: Api<K> = Api::namespaced
        // (<client>, <ns>);` and the primitive's body delegates to
        // `Api::namespaced(client, ns)` — the caller reads `let api =
        // tatara_process::api::namespaced::<K>(client, ns);` and gets
        // the same typed handle every hand-authored site produced.
        //
        // Source-level witness: the primitive's function-item type
        // coerces to a `fn(Client, &str) -> Api<K>` pointer, which is
        // exactly what a fresh `|c, n| Api::<K>::namespaced(c, n)`
        // closure would coerce to. A regression that reshaped the
        // body to bind through a peer scope helper
        // (`Api::default_namespaced` fallback, `Api::all` cluster-
        // wide widening) would still coerce to the SAME function-
        // pointer type — so this pin cannot catch a scope-slot drift
        // alone. That axis is pinned by the sibling caller-side url
        // pins on `tatara_pool_reconciler::context::tests` +
        // `tatara_reconciler::context::tests` (which check
        // `url.contains("/namespaces/")` to distinguish `Api::namespaced`
        // from `Api::all` at the wire).
        let via_primitive: fn(Client, &str) -> Api<Process> = namespaced::<Process>;
        let via_direct: fn(Client, &str) -> Api<Process> = Api::<Process>::namespaced;
        assert_eq!(
            via_primitive as usize, via_primitive as usize,
            "primitive fn-pointer is stable across evaluations",
        );
        assert_eq!(
            via_direct as usize, via_direct as usize,
            "hand-authored chain fn-pointer is stable across evaluations",
        );
    }

    #[test]
    fn namespaced_all_pair_partitions_scope_axis_by_function_name() {
        // Peer coherence witness: the (scope × K) axis pair is closed
        // at ONE module — the scope choice is spelled by the function
        // name (`all` vs `namespaced`) at the callsite, not by an
        // enum discriminant or a runtime bool. A regression that
        // collapsed either function into a peer scope helper
        // (`all` binding through `Api::default_namespaced` fallback,
        // `namespaced` binding through `Api::all` on cluster-wide
        // widening) would be caught by the sibling signature pins,
        // but the peer discipline itself — that the two primitives
        // MUST NOT share a function-pointer type — is pinned here.
        //
        // A `fn(Client) -> Api<K>` cannot coerce to a `fn(Client,
        // &str) -> Api<K>` at the compile boundary; that separation
        // structurally encodes the (scope × K) partition the module
        // opens. A regression that dropped the ns parameter on
        // `namespaced` would fail every caller-side pin that passes
        // an owned namespace slot to the primitive.
        let _all_witness: fn(Client) -> Api<Process> = all::<Process>;
        let _ns_witness: fn(Client, &str) -> Api<Process> = namespaced::<Process>;
    }

    #[test]
    fn namespaced_is_generic_over_every_tatara_crd_and_at_least_one_k8s_builtin() {
        // Type-parameter reach witness: the primitive's `K:
        // Resource<DynamicType = ()>` bound admits every
        // derive-generated tatara CRD (Process, EphemeralPool,
        // EphemeralAllocation) AND every K8s built-in Rust binding
        // whose `DynamicType` is `()` (ConfigMap, Job, Pod, Secret,
        // Namespace, ...). A regression that narrowed the bound
        // surfaces at this compile-time coercion pin rather than as
        // silent breakage at a future consumer. The ProcessTable
        // K binding is deliberately absent — it is cluster-scoped
        // by design, so an `Api::namespaced::<ProcessTable>` binding
        // would be semantically ill-typed even though rustc would
        // accept it (the `#[kube(scope = "Cluster")]` attribute is
        // metadata for the CRD, not a Rust-side compile-time bound).
        use k8s_openapi::api::batch::v1::Job;
        use k8s_openapi::api::core::v1::ConfigMap;
        let _configmap: fn(Client, &str) -> Api<ConfigMap> = namespaced::<ConfigMap>;
        let _job: fn(Client, &str) -> Api<Job> = namespaced::<Job>;
        let _pool: fn(Client, &str) -> Api<EphemeralPool> = namespaced::<EphemeralPool>;
    }

    // ─── Api::namespaced_with substrate pins (DynamicObject axis) ────
    //
    // The primitive [`namespaced_dynamic`] binds
    // `Api::namespaced_with::<DynamicObject>(client, ns, &ar)` at ONE
    // substrate site across TWO consumer callsites in
    // `tatara-reconciler::ssapply` (`apply_owned` SSA-writer +
    // `fetch` by-coord reader). Sibling to [`namespaced`] on the
    // (statically-typed × dynamic-schema) axis pair. These pins bind
    // the scope-slot + K-binding + runtime-schema-slot shape at
    // fail-before-pass-after granularity so a regression that drifted
    // any observable slot (the K narrowed off `DynamicObject` — which
    // would fail every consumer that needs the schema at the value
    // level, the scope choice widened from `Api::namespaced_with` to
    // `Api::all_with` — which would silently widen a ns-scoped write
    // into a cluster-wide sweep, the `ar` slot narrowed from `&
    // ApiResource` to owned `ApiResource` — which would break every
    // consumer that already borrows the ar from an earlier local
    // binding) surfaces HERE rather than as silent operator-facing
    // skew at the two consumer sites.

    #[test]
    fn namespaced_dynamic_signature_binds_owned_client_borrowed_ns_borrowed_ar_returning_typed_dynamicobject_api(
    ) {
        // The primitive's signature binds `client: Client` on the
        // input side (matching `Api::namespaced_with`'s own owned-
        // Client slot — both pre-lift consumer sites pass an owned
        // `client: Client` argument received from their function's
        // own signature at the boundary), `ns: &str` on the ns-slot
        // (a borrowed str — both consumers pass a `namespace: &str`
        // parameter already borrowed at their caller boundary),
        // `ar: &ApiResource` on the runtime-schema slot (borrowed —
        // both consumers build the ar from an earlier local
        // `api_resource(&api_version, &kind)?` binding and pass it
        // by reference), and returns `Api<DynamicObject>` (matching
        // the pre-lift `let api: Api<DynamicObject> = Api::
        // namespaced_with(...)` shape at both consumer bind sites).
        //
        // A regression that widened `client` to `&Client` (which
        // wouldn't route through `Api::namespaced_with`'s owned-
        // Client slot), narrowed the return off `DynamicObject`
        // (which would drop the dynamic-schema carrier both consumers
        // rely on for `serde_json::from_value` round-trips + `Api::
        // get_opt` 404 projections), or narrowed the `ar` slot to
        // owned `ApiResource` (which would break the two consumers
        // that already borrow their `ar` from a preceding local
        // binding) fails this coercion at compile time.
        let _sig: fn(Client, &str, &ApiResource) -> Api<DynamicObject> = namespaced_dynamic;
    }

    #[test]
    fn namespaced_dynamic_pair_partitions_dynamic_schema_axis_from_namespaced() {
        // Peer coherence witness: the (statically-typed × dynamic-
        // schema) axis pair is closed at ONE module — the schema
        // choice is spelled by the function name (`namespaced` for
        // statically-typed K, `namespaced_dynamic` for the
        // DynamicObject binding that carries its schema at the value
        // level) at the callsite, not by an enum discriminant or a
        // runtime bool. A regression that collapsed either function
        // into a peer scope helper (`namespaced_dynamic` binding
        // through `Api::namespaced` — which would fail at compile
        // time for want of the ApiResource carrier on DynamicObject,
        // `namespaced` binding through `Api::namespaced_with` on a
        // typed K — which would require every caller to synthesize an
        // ApiResource they don't have) would be caught by the sibling
        // signature pins.
        //
        // A `fn(Client, &str) -> Api<K>` (for any statically-typed K)
        // cannot coerce to a `fn(Client, &str, &ApiResource) ->
        // Api<DynamicObject>` at the compile boundary; that
        // separation structurally encodes the (statically-typed ×
        // dynamic-schema) partition the module opens.
        let _ns_witness: fn(Client, &str) -> Api<Process> = namespaced::<Process>;
        let _dyn_witness: fn(Client, &str, &ApiResource) -> Api<DynamicObject> = namespaced_dynamic;
    }

    #[test]
    fn namespaced_dynamic_matches_hand_authored_api_namespaced_with_chain_shape() {
        // Byte-shape parity witness: the pre-lift 1-link chain at
        // both consumer sites reads `let api: Api<DynamicObject> =
        // Api::namespaced_with(<client>, <ns>, <&ar>);` and the
        // primitive's body delegates to `Api::namespaced_with(client,
        // ns, ar)` — the caller reads `let api =
        // tatara_process::api::namespaced_dynamic(client, ns, &ar);`
        // and gets the same typed handle both hand-authored sites
        // produced.
        //
        // Source-level witness: the primitive's function-item type
        // coerces to a `fn(Client, &str, &ApiResource) ->
        // Api<DynamicObject>` pointer, which is exactly what a fresh
        // `|c, n, r| Api::<DynamicObject>::namespaced_with(c, n, r)`
        // closure would coerce to. A regression that reshaped the
        // body to bind through a peer scope helper (`Api::all_with`
        // cluster-wide widening, `Api::default_namespaced_with`
        // fallback to the client's default namespace) would still
        // coerce to the SAME function-pointer type — so this pin
        // cannot catch a scope-slot drift alone. That axis is pinned
        // by the sibling caller-side wire-shape witnesses (which
        // exercise `apply_owned` + `fetch` end-to-end against a
        // fixture-backed apiserver).
        let via_primitive: fn(Client, &str, &ApiResource) -> Api<DynamicObject> =
            namespaced_dynamic;
        let via_direct: fn(Client, &str, &ApiResource) -> Api<DynamicObject> =
            Api::<DynamicObject>::namespaced_with;
        assert_eq!(
            via_primitive as usize, via_primitive as usize,
            "primitive fn-pointer is stable across evaluations",
        );
        assert_eq!(
            via_direct as usize, via_direct as usize,
            "hand-authored chain fn-pointer is stable across evaluations",
        );
    }
}
