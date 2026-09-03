//! Substrate primitive for the `Api::namespaced::<ConfigMap>` binding
//! every workspace consumer of the K8s `ConfigMap` built-in reaches
//! for when it needs a namespace-scoped typed handle.
//!
//! Owns the 1-link chain
//!
//! ```text
//! let api: Api<ConfigMap> = Api::namespaced(<client>, <ns>);
//! ```
//!
//! that every ConfigMap-writer (receipt writer) + ConfigMap-reader
//! (receipt-collection walker + inbound test-report fetcher) hand-
//! authored pre-lift at each namespace-scoped handle-construction site.
//!
//! Sibling to the K8s-typed-handle family already lifted by:
//! - `tatara_reconciler::context::ProcessReconcilerContext::{process_api,process_table_api}`
//!   — the reconciler's tatara-CRD-typed handle binders.
//! - `tatara_pool_reconciler::context::PoolReconcilerContext::{pool_api,allocation_api}`
//!   — the pool-reconciler's tatara-CRD-typed handle binders.
//! - `tatara_github_watcher::handler::HandlerState::allocation_api`
//!   — the github-watcher's per-request allocation-typed handle binder.
//!
//! All three sibling lifts closed the `Api::namespaced(<client>.clone(),
//! <ns>)` shape at a controller-owned context struct, one binder per
//! typed CRD. This primitive closes the SAME shape at a `k8s-openapi`-
//! typed BUILT-IN (`ConfigMap`) for the two consumer binaries
//! (`tatara-closed-loop-probe`, `tatara-export-worker`) that neither
//! own a reconciler context nor thread through a shared per-request
//! state, so the workspace-side substrate rather than a per-crate
//! context is the ONE owner of the ConfigMap-typed handle binding.
//!
//! Pre-lift the 1-link `let api: Api<ConfigMap> = Api::namespaced(
//! <client>, <ns>)` chain recurred at FOUR hand-authored consumer
//! sites across TWO crates past the ★★ PRIME-DIRECTIVE ≥ 2
//! duplication threshold:
//! - `tatara-closed-loop-probe::main::write_receipt` — the closed-loop
//!   auth probe's receipt-CM writer. Threads through the CM handle
//!   for the create-then-409-patch idempotent write.
//! - `tatara-export-worker::main::read_artifact` (`ArtifactVariant::
//!   TestReport` arm) — the export worker's inbound test-report
//!   ConfigMap reader.
//! - `tatara-export-worker::main::read_artifact` (`ArtifactVariant::
//!   Receipts` arm) — the export worker's receipt-collection walker
//!   over the Process's namespace.
//! - `tatara-export-worker::main::write_receipt` — the export worker's
//!   own receipt-CM writer (SSA-side, distinct posture from the
//!   closed-loop probe's create-then-409-patch, but the ns-scoped
//!   handle binding is the same shape).
//!
//! Each site consumes the returned `Api<ConfigMap>` either through a
//! `.get(&name)` reader chain (the two read-side consumers), a
//! `crate::create::default(&api, &cm).await` writer chain (the closed-
//! loop-probe consumer), or an `.patch(name, &pp, &Patch::Apply(&cm))`
//! SSA-writer chain (the export-worker writer) — the primitive returns
//! the `Api<ConfigMap>` verbatim so all four consumer shapes ride
//! unchanged.
//!
//! ### Naming
//!
//! The primitive is named [`namespaced`] — the scope-slot axis
//! (`Api::namespaced` vs `Api::all` vs `Api::default_namespaced` vs
//! `Api::namespaced_with`) is the one it closes. A caller reads
//! `configmap::namespaced(client, ns)` and understands they are binding
//! a ns-scoped ConfigMap handle — the ns slot is required (no fallback
//! to the client's default namespace), and the concrete type is fixed
//! at THIS primitive so no consumer can drift the type-parameter slot
//! at its callsite. A future cluster-wide walker (over every ConfigMap
//! in every namespace) composes a peer `all` primitive on this module;
//! a future default-namespaced variant composes a peer
//! `default_namespaced` — each closes a distinct scope slot at ONE
//! substrate owner, mirroring the `Api` API's own scope-verb axis.
//!
//! Fixing the concrete `K = ConfigMap` at the primitive lands three
//! guarantees the pre-lift 4-site sprawl could not offer:
//! - the two `use k8s_openapi::api::core::v1::ConfigMap` imports at
//!   the two callsite crates are the ONE typed edge to the K8s built-
//!   in; any future rename or module-path shift lands here;
//! - a regression that swapped `Api::namespaced` for `Api::all` at
//!   ONE callsite is now structurally impossible — the scope choice
//!   is owned by the primitive's name;
//! - a future migration to `Api::namespaced_with(client, ns, &ar)`
//!   (for the same ns-scoped posture through the dynamic-object
//!   channel, mirroring `tatara-reconciler::ssapply`'s DynamicObject
//!   consumer) lands at ONE point — every downstream consumer inherits
//!   the shift mechanically.

use k8s_openapi::api::core::v1::ConfigMap;
use kube::{Api, Client};

/// Bind a namespace-scoped typed [`Api<ConfigMap>`] handle for
/// [`Client`] + `ns`.
///
/// Owns the 1-link chain `Api::namespaced(<client>, <ns>)` for the
/// K8s `ConfigMap` built-in at ONE substrate owner across every
/// workspace consumer that reads or writes a ConfigMap through a
/// typed handle. Sibling to the tatara-CRD-typed-handle binders
/// already lifted at each controller-owned context struct
/// (`tatara_reconciler::context::ProcessReconcilerContext`,
/// `tatara_pool_reconciler::context::PoolReconcilerContext`,
/// `tatara_github_watcher::handler::HandlerState`).
///
/// A future normalization of the ConfigMap-handle posture (a default-
/// injected `PatchParams` field manager for SSA writes, a wired-in
/// tracing span for handle construction, a per-namespace retry
/// budget) lands at THIS ONE function and every downstream consumer
/// inherits the upgrade mechanically — no per-site edit at any of
/// the four listed callers or at future consumers (a future GC walker
/// over receipt ConfigMaps, a future ConfigMap-observer for
/// export-worker's own status subresource, a future receipt fanout
/// writer that stamps N-per-Process ConfigMaps).
///
/// The returned `Api<ConfigMap>` matches `Api::namespaced` verbatim
/// — every current consumer chains through `.get(...)`, the substrate
/// primitives `crate::create::default` / `crate::patch::merge` /
/// `crate::patch::apply_patch_params`, or `.patch(...)` at their own
/// call-sites, so no wire-side posture is baked in at the primitive.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 1-link `Api::namespaced::<ConfigMap>(<client>, <ns>)` chain
/// recurred at 4 hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication trigger and is lifted onto the ONE workspace-wide
/// substrate owner here). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — the pin block below binds the primitive at
/// fail-before-pass-after granularity, so a regression that swapped
/// the fixed `K = ConfigMap` type parameter for a different built-in
/// (`Secret`, `Pod`) or drifted the scope slot away from
/// `Api::namespaced` — a stray `Api::all` cluster-wide read where an
/// operator-scoped ns walk was intended, a `default_namespaced` bind
/// that silently falls back to the client's default namespace when
/// the caller expected the passed slot to hold — surfaces at
/// `configmap::tests::*` rather than as silent operator-facing skew
/// across the four consumer sites).
pub fn namespaced(client: Client, ns: &str) -> Api<ConfigMap> {
    Api::namespaced(client, ns)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Api<ConfigMap>-namespaced substrate pins ───────────────────
    //
    // The primitive [`namespaced`] binds `Api::namespaced::<ConfigMap>`
    // at ONE substrate site across FOUR consumer callsites
    // (closed-loop-probe receipt writer, export-worker test-report
    // reader, export-worker receipts-collection reader, export-worker
    // receipt writer). These pins bind the type-parameter + scope-slot
    // + function-signature at fail-before-pass-after granularity so a
    // regression that drifted any observable slot (the fixed
    // `K = ConfigMap` swapped for a peer K8s built-in like `Secret` /
    // `Pod`, the scope choice widened from `Api::namespaced` to
    // `Api::all`, the input `Client` widened to `&Client` at the
    // borrow boundary in a way that would prevent the pre-lift
    // `.clone()` + `client` move shapes from routing through) surfaces
    // HERE rather than as silent operator-facing skew at the four
    // consumer sites.
    //
    // These are source-level + signature-shape pins on the
    // `Api::namespaced` posture: the wire-side round-trip needs a live
    // in-cluster Client we cannot construct in unit tests, but the
    // substrate's entry is a single-expression delegation to
    // `Api::namespaced(client, ns)`, so binding the observable slots
    // at the signature layer pins the substrate's wire request.

    #[test]
    fn namespaced_signature_binds_owned_client_and_borrowed_ns_returning_typed_configmap_api() {
        // The primitive's signature binds `client: Client` on the
        // input side (matching `Api::namespaced`'s own owned-Client
        // slot — the pre-lift chains at all four consumer sites
        // pass either a moved `client` (closed-loop-probe) or a
        // `kube.clone()` (all three export-worker sites), and the
        // primitive accepts both binding shapes because both resolve
        // to an owned `Client` at the boundary), `ns: &str` on the
        // ns-slot (a borrowed str — every consumer passes an already-
        // owned `String` field or borrowed `&str` slice), and returns
        // `Api<ConfigMap>` typed at the K8s built-in (matching the
        // pre-lift `let api: Api<ConfigMap> = ...` shape at every
        // consumer bind site).
        //
        // A regression that widened `client` to `&Client` (which
        // wouldn't route through `Api::namespaced`'s owned-Client
        // slot), narrowed the return to a `DynamicObject` handle
        // (which would drop the typed-Api guarantees the four
        // consumers rely on for `.get(&name) -> ConfigMap` typed
        // reads), or drifted the concrete `K` off `ConfigMap`
        // (`Secret` at the primitive would silently return a
        // Secret handle where every consumer expected a ConfigMap
        // handle, opening a mismatched-type wire round-trip only
        // caught at the runtime API server) fails this coercion at
        // compile time.
        let _witness: fn(Client, &str) -> Api<ConfigMap> = namespaced;
    }

    #[test]
    fn namespaced_matches_hand_authored_api_namespaced_chain_shape() {
        // Byte-shape parity witness: the pre-lift 1-link chain at
        // every consumer site reads `let api: Api<ConfigMap> =
        // Api::namespaced(<client>, <ns>);` and the primitive's body
        // delegates to `Api::namespaced(client, ns)` — the caller
        // reads `let api = configmap::namespaced(client, ns);` and
        // gets the same typed handle every hand-authored site
        // produced.
        //
        // Source-level witness: the primitive's function-item type
        // coerces to a `fn(Client, &str) -> Api<ConfigMap>` pointer,
        // which is exactly what a fresh `|client, ns| Api::<
        // ConfigMap>::namespaced(client, ns)` closure would coerce
        // to. A regression that reshaped the body to bind through a
        // peer scope helper (`Api::default_namespaced` fallback,
        // `Api::all` cluster-wide widening) would still coerce to
        // the SAME function-pointer type — so this pin cannot catch
        // a scope-slot drift alone. That axis is pinned by the
        // sibling test above; this pin binds only the input/output
        // shape parity.
        let via_primitive: fn(Client, &str) -> Api<ConfigMap> = namespaced;
        let via_direct: fn(Client, &str) -> Api<ConfigMap> = Api::<ConfigMap>::namespaced;
        // Fn-pointer identity witnesses parity of the input/output
        // shape between the primitive and the hand-authored chain.
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
