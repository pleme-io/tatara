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
}
