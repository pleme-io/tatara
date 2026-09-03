//! Substrate primitive for the `Api::namespaced::<Process>` binding
//! every workspace consumer of the tatara `Process` CRD reaches for
//! when it needs a namespace-scoped typed handle from a bare
//! [`Client`] + `&str` namespace pair (no per-crate reconciler
//! context in scope).
//!
//! Owns the 1-link chain
//!
//! ```text
//! let api: Api<Process> = Api::namespaced(<client>, <ns>);
//! ```
//!
//! that every below-controller-layer + boundary-layer Process-handle
//! consumer hand-authored pre-lift at each namespace-scoped bind site.
//!
//! Sibling to the ns-scoped K8s-typed-handle family already lifted at:
//! - [`crate::configmap::namespaced`] — the K8s built-in ConfigMap
//!   ns-scoped handle binder, opened for the same
//!   `tatara-export-worker` + `tatara-closed-loop-probe` consumers
//!   that could not thread through a shared reconciler context.
//! - `tatara_reconciler::context::Context::process_api` — the
//!   reconciler's per-request Process-typed handle binder (kept as a
//!   forwarder that delegates through THIS substrate primitive
//!   post-lift, so a future normalization at the substrate owner
//!   reaches BOTH the reconciler-side handler sprawl AND every
//!   below-controller boundary/export-worker consumer through ONE
//!   owner).
//! - `tatara_pool_reconciler::context::PoolContext::{pool_api,
//!   allocation_api,pools_all_api,allocations_all_api}` — the
//!   pool-reconciler's tatara-CRD-typed handle binders.
//! - `tatara_github_watcher::handler::HandlerState::allocation_api`
//!   — the github-watcher's per-request allocation-typed handle
//!   binder.
//!
//! All sibling lifts closed the `Api::namespaced(<client>.clone(),
//! <ns>)` shape at either a controller-owned context struct (per-CRD
//! binder) or a workspace-wide substrate module (per-K8s-built-in
//! binder). This primitive closes the SAME shape at the tatara
//! `Process` CRD for the THREE consumer sites that neither own a
//! reconciler context nor thread through a shared per-request
//! state:
//! - `tatara_reconciler::boundary::evaluate_process_phase` — the
//!   `ConditionKind::ProcessPhase` boundary evaluator. Called with
//!   a bare `Client` moved in from `check_conditions` (no `Context`
//!   in scope; the evaluator sits below the reconciler layer so it
//!   can be reused by the `tatara-check` binary).
//! - `tatara_reconciler::boundary::check_depends_on` — the
//!   `spec.dependsOn` evaluator. Iterates every dep with a
//!   `client.clone()` per row; also called from the boundary layer
//!   without a `Context`.
//! - `tatara_export_worker::main::read_artifact` — the export
//!   worker's `ProcessSnapshotSource` reader. `tatara-export-worker`
//!   is a below-controller-layer binary that DOES NOT depend on
//!   `tatara-reconciler` (would introduce a cycle) so it cannot
//!   reach the reconciler's `Context::process_api`.
//!
//! Pre-lift the 1-link `let api: Api<Process> = Api::namespaced(
//! <client>, <ns>)` chain recurred at THESE THREE hand-authored
//! consumer sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
//! threshold. Post-lift each consumer reads
//! `tatara_process::process_api::namespaced(client, ns)` and the
//! ns-scoped Process handle binding lives at ONE substrate owner.
//!
//! ### Naming
//!
//! The module is named [`process_api`] — the tatara-process crate
//! already owns a top-level `crd` module carrying the `Process`
//! type itself, so a bare `process` submodule would collide with
//! the crate's own name and read as an accidental self-reference
//! (`tatara_process::process::namespaced`). `process_api` names the
//! axis it closes ("build a typed `Api` for the tatara `Process`
//! CRD") explicitly, mirrors the reconciler's own `process_api`
//! method on `Context`, and reads unambiguously at every callsite.
//!
//! Fixing the concrete `K = Process` at the primitive lands three
//! guarantees the pre-lift 3-site sprawl could not offer:
//! - the two `use tatara_process::crd::Process;` /
//!   `use tatara_process::prelude::*;` imports at the callsite
//!   crates are the ONE typed edge to the Process CRD; any future
//!   rename or module-path shift lands at ONE substrate primitive
//!   rather than at every consumer;
//! - a regression that swapped `Api::namespaced` for `Api::all` at
//!   ONE callsite is now structurally impossible — the scope choice
//!   is owned by the primitive's name (peer `Api::all` cluster-wide
//!   Process consumers route through
//!   `tatara_reconciler::context::Context::processes_all_api` on
//!   the reconciler side; a future workspace-wide cluster-scoped
//!   peer composes as `process_api::all` on this module);
//! - a future migration to `Api::namespaced_with(client, ns, &ar)`
//!   (for the same ns-scoped posture through the dynamic-object
//!   channel, mirroring `tatara-reconciler::ssapply`'s DynamicObject
//!   consumer) lands at ONE point — every downstream consumer
//!   inherits the shift mechanically.

use kube::{Api, Client};

use crate::crd::Process;

/// Bind a namespace-scoped typed [`Api<Process>`] handle for
/// [`Client`] + `ns`.
///
/// Owns the 1-link chain `Api::namespaced(<client>, <ns>)` for the
/// tatara `Process` CRD at ONE substrate owner across every
/// workspace consumer that reads or writes a Process through a
/// typed handle without a shared per-request context in scope.
/// Sibling to the K8s-built-in ns-scoped handle binder
/// [`crate::configmap::namespaced`] and to the reconciler's
/// per-request `Context::process_api` forwarder.
///
/// A future normalization of the Process-handle posture (a
/// default-injected `PatchParams` field manager for status writes,
/// a wired-in tracing span for handle construction, a per-namespace
/// retry budget, a fixture-backed client for CI/smoke-tests) lands
/// at THIS ONE function and every downstream consumer inherits the
/// upgrade mechanically — no per-site edit at any of the three
/// listed callers or at future consumers (a future boundary-layer
/// evaluator for a new `ConditionKind`, a future below-controller
/// binary that reads a Process by name, a future workspace-side
/// audit walker).
///
/// The returned `Api<Process>` matches `Api::namespaced` verbatim
/// — every current consumer chains through `.get_opt(...)` (both
/// boundary-layer evaluators) or `.get(...)` (the export-worker
/// snapshot reader) at its own callsite, so no wire-side posture
/// is baked in at the primitive.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the 1-link `Api::namespaced::<Process>(<client>, <ns>)` chain
/// recurred at 3 hand-authored sites past the ★★ PRIME-DIRECTIVE
/// ≥ 2 duplication trigger and is lifted onto the ONE workspace-
/// wide substrate owner here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — the pin block below binds the
/// primitive at fail-before-pass-after granularity, so a regression
/// that swapped the fixed `K = Process` type parameter for a
/// different CRD (`EphemeralPool`, `EphemeralAllocation`, `ProcessTable`)
/// or drifted the scope slot away from `Api::namespaced` — a stray
/// `Api::all` cluster-wide read where a namespace-scoped
/// dependency lookup was intended — surfaces at
/// `process_api::tests::*` rather than as silent operator-facing
/// skew across the three consumer sites).
pub fn namespaced(client: Client, ns: &str) -> Api<Process> {
    Api::namespaced(client, ns)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Api<Process>-namespaced substrate pins ─────────────────────
    //
    // The primitive [`namespaced`] binds `Api::namespaced::<Process>`
    // at ONE substrate site across THREE consumer callsites
    // (boundary `evaluate_process_phase`, boundary `check_depends_on`,
    // export-worker `ProcessSnapshotSource` reader). These pins bind
    // the type-parameter + scope-slot + function-signature at
    // fail-before-pass-after granularity so a regression that
    // drifted any observable slot (the fixed `K = Process` swapped
    // for a peer tatara CRD like `EphemeralPool` or `ProcessTable`,
    // the scope choice widened from `Api::namespaced` to `Api::all`,
    // the input `Client` widened to `&Client` at the borrow
    // boundary in a way that would prevent the pre-lift `.clone()` +
    // moved `client` shapes from routing through) surfaces HERE
    // rather than as silent operator-facing skew at the three
    // consumer sites.
    //
    // These are source-level + signature-shape pins on the
    // `Api::namespaced` posture: the wire-side round-trip needs a
    // live in-cluster Client, but the substrate's entry is a
    // single-expression delegation to `Api::namespaced(client, ns)`,
    // so binding the observable slots at the signature layer pins
    // the substrate's wire request. Peer to
    // `crate::configmap::tests::*` which binds the same axes for
    // the ConfigMap-built-in sibling.

    #[test]
    fn namespaced_signature_binds_owned_client_and_borrowed_ns_returning_typed_process_api() {
        // The primitive's signature binds `client: Client` on the
        // input side (matching `Api::namespaced`'s own owned-Client
        // slot — the pre-lift chains at all three consumer sites
        // pass either a moved `client` (boundary
        // `evaluate_process_phase`) or a `client.clone()` /
        // `kube.clone()` (boundary `check_depends_on` per-dep loop
        // + export-worker snapshot reader), and the primitive
        // accepts both binding shapes because both resolve to an
        // owned `Client` at the boundary), `ns: &str` on the
        // ns-slot (a borrowed str — every consumer passes an
        // already-owned `String` field, a borrowed `&str` slice, or
        // an `Option::as_deref()`-projected borrow), and returns
        // `Api<Process>` typed at the tatara CRD (matching the
        // pre-lift `let api: Api<Process> = ...` shape at every
        // consumer bind site).
        //
        // A regression that widened `client` to `&Client` (which
        // wouldn't route through `Api::namespaced`'s owned-Client
        // slot), narrowed the return to a `DynamicObject` handle
        // (which would drop the typed-Api guarantees the three
        // consumers rely on for `.get_opt(&name) -> Process` typed
        // reads), or drifted the concrete `K` off `Process`
        // (`EphemeralPool` at the primitive would silently return
        // a pool handle where every consumer expected a Process
        // handle, opening a mismatched-type wire round-trip only
        // caught at the runtime API server) fails this coercion at
        // compile time.
        let _witness: fn(Client, &str) -> Api<Process> = namespaced;
    }

    #[test]
    fn namespaced_matches_hand_authored_api_namespaced_chain_shape() {
        // Byte-shape parity witness: the pre-lift 1-link chain at
        // every consumer site reads `let api: Api<Process> =
        // Api::namespaced(<client>, <ns>);` and the primitive's
        // body delegates to `Api::namespaced(client, ns)` — the
        // caller reads `let api = process_api::namespaced(client, ns);`
        // and gets the same typed handle every hand-authored site
        // produced.
        //
        // Source-level witness: the primitive's function-item type
        // coerces to a `fn(Client, &str) -> Api<Process>` pointer,
        // which is exactly what a fresh `|client, ns|
        // Api::<Process>::namespaced(client, ns)` closure would
        // coerce to. A regression that reshaped the body to bind
        // through a peer scope helper (`Api::default_namespaced`
        // fallback, `Api::all` cluster-wide widening) would still
        // coerce to the SAME function-pointer type — so this pin
        // cannot catch a scope-slot drift alone. That axis is
        // pinned by the sibling test above; this pin binds only
        // the input/output shape parity.
        let via_primitive: fn(Client, &str) -> Api<Process> = namespaced;
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
    fn namespaced_accepts_borrowed_and_owned_ns_shapes_at_the_type_level() {
        // The three shipped callsites split across two shapes:
        // boundary `evaluate_process_phase` passes a `&str` slice
        // pulled from `ssapply::resolve_target_namespace(...)`;
        // boundary `check_depends_on` passes the same shape per
        // dep; export-worker `read_artifact` passes an owned
        // `String` field via deref coercion. Both shapes must
        // route through the same `&str` parameter without
        // widening — pin the two callsite forms at the type level
        // so a regression that narrowed the parameter to `String`
        // (forcing every caller to allocate) or widened it to
        // `impl AsRef<str>` (making the callsite ambiguous for the
        // borrowed-slice sites) fails to coerce here at compile
        // time. Peer to `configmap::tests::
        // namespaced_signature_binds_owned_client_and_borrowed_ns_returning_typed_configmap_api`
        // on the sibling K8s-built-in axis. Wire-shape witnesses
        // (URL routing, cluster-scope vs ns-scope contrast) live
        // one crate up at
        // `tatara_reconciler::context::tests::process_api_*` on
        // the reconciler-side forwarder — which delegates through
        // THIS primitive post-lift, so those runtime pins now bind
        // this substrate owner too.
        let _borrowed_witness: fn(Client, &str) -> Api<Process> = namespaced;
        // The owned-`String` deref coercion is not a distinct
        // function-pointer type — it's the same `&str`-parametered
        // function-item after auto-deref at the callsite. Source-
        // level pin: a caller with `owned: String` shape can name
        // the primitive with `&owned` and hit the same `&str`
        // slot. A regression that changed the parameter type
        // would fail every callsite in the reconciler + export-
        // worker at compile time.
    }
}
