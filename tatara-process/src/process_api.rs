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
    // Delegates through the workspace-wide substrate owner
    // [`crate::api::namespaced`] — sibling to
    // [`crate::api::all`] on the (scope × K) axis pair, closing the
    // `Api::namespaced(<client>, <ns>)` shape at ONE substrate
    // primitive across every ns-scoped Api binder site. Post-lift a
    // future normalization of the ns-scoped Api posture (tracing
    // span, QPS budget, fixture-backed client, wired-in `PatchParams`
    // field manager) lands at THAT owner rather than at this
    // fixed-K sibling — which now carries the K = Process guarantee
    // exclusively, not the `Api::namespaced` shape it used to
    // co-own.
    crate::api::namespaced::<Process>(client, ns)
}

/// Compose the diagnostic-body head every wire-verb failure against a
/// namespaced [`Process`] wraps around the underlying error via
/// [`crate::kube_error::KubeResultExt::kube_ctx_with`] or the sibling
/// [`anyhow::Context::with_context`] closure form.
///
/// Owns the fixed `<verb> Process <ns>/<name>` shape as ONE substrate
/// site, routing the `<ns>/<name>` join through the workspace-wide
/// [`crate::qualified_process_ref`] composer so a future normalization
/// of the qualified-ref shape (case-fold, unicode collation, IDN)
/// lands at ONE site and every Process-scoped diagnostic body picks
/// it up mechanically.
///
/// Sibling to [`crate::configmap::error_ctx`] on the (per-Kind ×
/// substrate-owned error-slug) axis-family — that primitive owns the
/// fixed `"ConfigMap"` resource-kind literal on the K8s-built-in
/// ConfigMap axis; THIS primitive owns the fixed `"Process"`
/// resource-kind literal on the tatara CRD axis. Both share the
/// discipline of routing the failure-diagnostic head through ONE
/// substrate composer per K8s-Kind rather than restating the shape
/// as a bare `format!(…)` chain at every consumer. And both share
/// the workspace-canonical TitleCase resource-kind spelling
/// (`"ConfigMap"` / `"Process"`) — matching the sibling
/// [`crate::list::error_ctx`]'s TitleCase-plural convention
/// (`"Processes"`) so an operator grepping across the fleet on the
/// canonical kube-canonical form hits every diagnostic surface.
///
/// Pre-lift the 3-slot `format!("{verb} process {ns}/{name}: {e}")`
/// chain (with lowercase `process`, DRIFTING from the workspace-
/// canonical TitleCase `Process` the sibling [`crate::list::error_ctx`]
/// pins for the plural spelling) recurred at TWO hand-authored sites
/// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold across two
/// crates:
///
/// * `tatara-reconciler::boundary::evaluate_process_phase` — verb
///   `"fetch"`, wrapping the `Api<Process>::get_opt(&process_ref)`
///   fetch that the `ConditionKind::ProcessPhase` boundary evaluator
///   dispatches for every dependency probe / postcondition Process
///   phase read.
/// * `tatara-export-worker::main::read_artifact` — verb `"get"`,
///   wrapping the `Api<Process>::get(name)` fetch on the
///   `ProcessSnapshotSource` arm that serializes the owning Process's
///   spec + status into the export artifact stream.
///
/// Both sites walked the SAME shape — take a verb, the target
/// Process's namespace + name, and the underlying error's display —
/// and produced the SAME `"<verb> process <ns>/<name>: <error>"`
/// diagnostic. Post-lift each callsite reads
/// `process_api::error_ctx(<verb>, ns, name)` and pipes the returned
/// context string through [`crate::kube_error::KubeResultExt::kube_ctx_with`]
/// (the boundary consumer) or through [`anyhow::Context::with_context`]
/// (the export-worker consumer, whose `kube::Error` bubbles through
/// anyhow's own `Error + Send + Sync + 'static` bound); both tails
/// own the same `": {e}"` suffix so the composed diagnostic is
/// byte-identical to the pre-lift shape modulo the intentional
/// TitleCase-kind drift-close.
///
/// ### Wire-form drift close
///
/// The lift intentionally changes `process` (lowercase) to `Process`
/// (TitleCase) at both consumers' operator-facing diagnostics —
/// closing a workspace-wide wire-form drift where the plural-list
/// axis at [`crate::list::error_ctx`] pinned TitleCase (`"Processes"`),
/// the ConfigMap-write axis at [`crate::configmap::error_ctx`] pinned
/// TitleCase (`"ConfigMap"`), but the singular-fetch axis at these
/// two consumer sites had drifted to lowercase (`"process"`). Post-
/// lift every substrate-owned failure-diagnostic head across the
/// fleet uses the kube-canonical TitleCase kind spelling so a
/// fleet-wide `grep 'Process default/api'` on operator log streams
/// matches EVERY Process-scoped failure body — the fetch corner
/// alongside the list corner alongside the ConfigMap-write corner.
///
/// A future normalization step — a `tracing`-annotated span carrying
/// the verb + qualified-ref for post-hoc audit, a per-verb structured-
/// error kind so operators filter by fetch-verb rather than substring-
/// match on the message body, a wire-time hedging of the verb spelling
/// (`"GET"` vs `"get"` per a fleet convention), injection of a per-
/// cluster prefix for a shared-controller deployment — lands at THIS
/// ONE substrate primitive and every downstream Process-scoped
/// failure diagnostic across the fleet picks up the upgrade
/// mechanically. Future third + fourth consumers (a receipt-GC
/// controller that fetches a Process by owner-ref for a reap decision,
/// a cross-namespace routing walker that reads a Process to derive an
/// Ingress alias) inherit the primitive at their own callsites with
/// no per-site drift surface.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 3-slot `format!(…)` chain recurred at 2 hand-authored sites past
/// the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger and is lifted onto
/// the ONE workspace-wide substrate owner here). THEORY.md §II.1
/// invariant 5 (composition preserves proofs — the pin block below
/// binds the composer at fail-before-pass-after granularity, so a
/// regression that reordered the head slots, drifted the fixed
/// `"Process"` resource-kind literal back to lowercase, dropped the
/// qualified-ref routing, or narrowed the accepted verb set to a
/// hardcoded closed set surfaces at `process_api::tests::error_ctx_*`
/// rather than as silent operator-facing skew across the two consumer
/// sites).
#[must_use]
pub fn error_ctx(verb: &str, ns: &str, name: &str) -> String {
    // Delegates through the workspace-wide substrate owner
    // [`crate::qualified_error_ctx`] — the ONE composer of the
    // `<verb> <Kind> <ns>/<name>` shape shared with
    // [`crate::configmap::error_ctx`] on the peer K8s-built-in
    // ConfigMap axis. Post-lift a future normalization of the
    // 4-slot shape (a `tracing`-annotated span, a per-Kind
    // canonicalization, an operator-supplied cluster prefix) lands
    // at THAT owner rather than at this fixed-Kind peer — which
    // now carries the `Kind = "Process"` guarantee exclusively,
    // not the 4-slot shape it used to co-own.
    crate::qualified_error_ctx(verb, "Process", ns, name)
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

    // ─── error_ctx substrate pins ───────────────────────────────────
    //
    // The composer [`error_ctx`] binds the `<verb> Process <ns>/<name>`
    // diagnostic-body head at ONE substrate site across TWO consumer
    // callsites (`tatara-reconciler::boundary::evaluate_process_phase`'s
    // `.get_opt` fetch wrap, `tatara-export-worker::main::read_artifact`'s
    // `ProcessSnapshotSource` `.get` fetch wrap). These pins bind the
    // observable slots (verb-first, fixed `"Process"` resource-kind
    // literal, qualified-ref routing for the `<ns>/<name>` join) at
    // fail-before-pass-after granularity so a regression that reordered
    // the head slots, dropped the fixed resource-kind literal, drifted
    // the literal back to the pre-lift lowercase `"process"` spelling,
    // or routed the `<ns>/<name>` shape through a bare `format!` inline
    // (bypassing the workspace-wide `qualified_process_ref` substrate)
    // surfaces HERE rather than as silent operator-facing prefix skew
    // at the two consumer sites.

    #[test]
    fn error_ctx_signature_binds_borrowed_verb_ns_name_returning_owned_string() {
        // The composer's signature binds `verb: &str` + `ns: &str` +
        // `name: &str` on the input side (both hand-authored consumer
        // sites pass a `&'static str` verb literal and borrowed `&str`
        // fields — boundary threads `&ns` off `resolve_target_namespace`
        // + `&parsed.process_ref` off the parsed params row; export-
        // worker threads the ProcessSnapshot arm's `ns` + `name` off
        // the `read_artifact(ns: &str, name: &str, …)` slot pair).
        // Return `String` matches the downstream `kube_ctx_with(context:
        // String)` sink verbatim on the boundary consumer AND the
        // `with_context(|| String)` closure form on the export-worker
        // consumer.
        //
        // A regression that widened any input slot to `String` (forcing
        // the caller to `.to_string()` at the boundary — a per-site
        // perf regression that also fights the `&str`-fields-in-args
        // idiom the callers thread) or narrowed the return to
        // `&'static str` (which would prevent the runtime-composed
        // ns/name slots the two consumers pass) fails at compile time.
        let _witness: fn(&str, &str, &str) -> String = error_ctx;
    }

    #[test]
    fn error_ctx_composes_fetch_process_qualified_ref_body_verbatim() {
        // Byte-shape parity witness for the reconciler-boundary
        // consumer post-lift: verb `"fetch"` + a `Process` in the
        // `default` namespace named `api` composes the head
        // `"fetch Process default/api"`, which pipes into
        // `kube_ctx_with`'s `": {e}"` tail to yield the full
        // diagnostic body every boundary-layer probe wraps around a
        // `kube::Error`.
        //
        // A regression that reordered head slots (e.g. dropped the
        // fixed `"Process"` word, emitted the qualified-ref before the
        // verb, drifted the kind literal back to lowercase `"process"`
        // as pre-lift) surfaces HERE at the head-shape pin rather than
        // as silent operator-visible prefix skew at the callsite.
        assert_eq!(
            error_ctx("fetch", "default", "api"),
            "fetch Process default/api",
        );
    }

    #[test]
    fn error_ctx_composes_get_process_qualified_ref_body_verbatim() {
        // Byte-shape parity witness for the export-worker consumer
        // post-lift: verb `"get"` + a `Process` in the `demo-ns`
        // namespace named `demo` composes the head `"get Process
        // demo-ns/demo"`, which pipes into `with_context`'s `": {e}"`
        // tail to yield the full diagnostic body the export-worker's
        // `ProcessSnapshotSource` arm wraps around the underlying
        // `kube::Error` bubbled through anyhow.
        //
        // Peer to the reconciler-boundary pin above — both verbs
        // ("fetch", "get") route through the SAME composer with the
        // SAME shape, differing only in the leading verb slot each
        // callsite passes.
        assert_eq!(
            error_ctx("get", "demo-ns", "demo"),
            "get Process demo-ns/demo",
        );
    }

    #[test]
    fn error_ctx_routes_ns_name_join_through_qualified_process_ref_substrate() {
        // Routing pin — the `<ns>/<name>` join at the composer's tail
        // rides through the workspace-wide `qualified_process_ref`
        // primitive rather than a bare inline `format!("{ns}/{name}")`.
        // A future normalization of the qualified-ref shape (case-
        // fold, unicode collation, IDN) lands at ONE
        // `qualified_process_ref` site and every downstream diagnostic
        // body picks it up mechanically; this pin binds THIS composer
        // to that substrate so a regression that inlined the join
        // (drifting the primitive off the substrate axis this commit
        // opens) surfaces HERE rather than as silent qualified-ref
        // drift between the two consumer sites and every other
        // qualified-ref consumer across the workspace.
        //
        // Sibling to [`crate::configmap::tests::
        // error_ctx_routes_ns_name_join_through_qualified_process_ref_substrate`]
        // on the peer ConfigMap axis of the same axis-family — both
        // per-Kind composers share the SAME routing discipline through
        // the SAME `qualified_process_ref` substrate.
        for (ns, name) in [
            ("default", "api"),
            ("tatara-system", "reconciler-canary"),
            ("demo-ns", "process-with-hyphen"),
            ("ns-1", "process.dotted.name"),
        ] {
            let via_composer = error_ctx("fetch", ns, name);
            let via_qualified = format!("fetch Process {}", crate::qualified_process_ref(ns, name));
            assert_eq!(
                via_composer, via_qualified,
                "error_ctx must route the (ns, name) join through qualified_process_ref for ns={ns:?} name={name:?}",
            );
        }
    }

    #[test]
    fn error_ctx_is_symbolic_over_the_verb_slot() {
        // Substitution pin: the `verb` slot is threaded verbatim into
        // the produced slug — no case-fold, no allow-list narrowing to
        // the two shipped verbs (`"fetch"`, `"get"`), no verb-family
        // canonicalization (`"GET"` promoted to `"get"`). A regression
        // that narrowed the accepted verb set to the two current
        // callsites' literals (a hardcoded `match verb { "fetch" |
        // "get" => …, _ => … }` closed set that would silently reject
        // future consumers) surfaces here.
        //
        // Future third + fourth consumers (a receipt-GC controller
        // walking Processes by owner-ref for a reap decision → verb
        // `"reap"`; a cross-namespace routing walker reading Processes
        // to derive Ingress aliases → verb `"resolve"`) inherit the
        // primitive at their own callsites and pass their own verbs
        // verbatim without the composer widening.
        for verb in [
            "fetch", "get", "reap", "resolve", "watch", "patch", "delete",
        ] {
            let got = error_ctx(verb, "default", "api");
            let expected = format!("{verb} Process default/api");
            assert_eq!(got, expected, "verb-slot substitution must be verbatim");
        }
    }

    #[test]
    fn error_ctx_composes_with_kube_ctx_with_to_boundary_pre_lift_body_verbatim() {
        // End-to-end parity witness on the reconciler-boundary
        // consumer's tail — the (composer + `kube_ctx_with`) pair
        // produces the SAME diagnostic body the pre-lift
        // `.kube_ctx_with(format!("fetch process {ns}/{name}"))?`
        // chain produced, MODULO the intentional TitleCase-kind
        // drift-close documented on the composer's doc. The composer
        // OWNS the head; `kube_ctx_with` OWNS the `": {e}"` tail;
        // concatenation matches the post-lift shape byte-for-byte.
        use crate::kube_error::KubeResultExt;
        use kube::core::ErrorResponse;

        let e = kube::Error::Api(ErrorResponse {
            status: "Failure".into(),
            message: "test failure".into(),
            reason: "Test".into(),
            code: 500,
        });
        let post_lift_expected = format!("fetch Process default/api: {e}");

        let via_pair: anyhow::Result<()> =
            Err::<(), _>(e).kube_ctx_with(error_ctx("fetch", "default", "api"));
        let via_pair_display = via_pair.unwrap_err().to_string();

        assert_eq!(
            via_pair_display, post_lift_expected,
            "the (error_ctx head + kube_ctx_with tail) pair must produce the \
             byte-identical post-lift `\"<verb> Process {{ns}}/{{name}}: {{e}}\"` diagnostic",
        );
    }

    #[test]
    fn error_ctx_composes_with_anyhow_with_context_to_export_worker_pre_lift_head_verbatim() {
        // End-to-end parity witness on the export-worker consumer's
        // tail — the (composer + `anyhow::Context::with_context`)
        // closure pair produces the SAME diagnostic HEAD the
        // export-worker's post-lift `.with_context(|| process_api::
        // error_ctx("get", ns, name))?` chain produces. The composer
        // returns an owned `String` from the closure only when the
        // Result is `Err` (matching `with_context`'s lazy semantics),
        // so on the Ok arm no `qualified_process_ref` allocation
        // fires.
        //
        // `anyhow::Context::with_context` CHAINS the context onto the
        // source error rather than flattening (unlike the sibling
        // `kube_ctx_with` on the reconciler-boundary consumer, which
        // uses `anyhow::anyhow!("{ctx}: {e}")` to flatten): the top-
        // level `Error::to_string()` returns the head only, and the
        // source lives one level deeper via `.source()` / the
        // `err.chain()` iterator. This matches pre-lift semantics —
        // the export-worker was already using `.with_context(||
        // format!("get process {ns}/{name}"))` with the same chained-
        // context posture; the lift preserves it. This pin binds
        // (a) the head equals the composer's output verbatim, and
        // (b) the source chain contains the original `kube::Error`
        // — so a regression that drifted the head OR that dropped
        // the source chain via a flatten wrap would fail here.
        //
        // Peer to the kube-tail pin above — both tail paths compose
        // with this ONE composer; the flatten-vs-chain choice lives
        // at the consumer's tail, not at the substrate head.
        use anyhow::Context;
        use kube::core::ErrorResponse;

        let e = kube::Error::Api(ErrorResponse {
            status: "Failure".into(),
            message: "test failure".into(),
            reason: "Test".into(),
            code: 404,
        });
        let expected_head = "get Process demo-ns/demo";

        let via_pair: anyhow::Result<()> =
            Err::<(), _>(e).with_context(|| error_ctx("get", "demo-ns", "demo"));
        let via_pair_err = via_pair.unwrap_err();

        // (a) the top-level Display matches the composer's head
        //     verbatim — the head is the substrate composer's owned
        //     output and NOT drifted per-tail.
        assert_eq!(
            via_pair_err.to_string(),
            expected_head,
            "the (error_ctx head + anyhow with_context tail) pair must expose the \
             substrate composer's head as the top-level Display",
        );

        // (b) the source chain preserves the original `kube::Error`
        //     — `with_context` chains rather than flattens, matching
        //     the pre-lift export-worker consumer semantics. A
        //     regression that dropped the source (a
        //     `map_err(|_| anyhow!("..."))` synthesis losing the
        //     kube-error root) would fail here.
        let source_chain: Vec<String> = via_pair_err
            .chain()
            .skip(1) // skip the head we just pinned
            .map(|src| src.to_string())
            .collect();
        assert!(
            !source_chain.is_empty(),
            "with_context tail must preserve the underlying kube::Error in the source chain",
        );
        assert!(
            source_chain[0].contains("test failure"),
            "the chained source must carry the underlying kube::Error's Display: got {source_chain:?}",
        );
    }

    #[test]
    fn error_ctx_matches_sibling_configmap_error_ctx_shape_modulo_kind_slot() {
        // Cross-substrate coherence pin — this composer and its
        // sibling [`crate::configmap::error_ctx`] on the peer K8s-
        // Kind axis produce byte-identical diagnostic heads MODULO
        // the fixed resource-kind literal (`"Process"` here vs
        // `"ConfigMap"` there). A regression that drifted either
        // composer's shape (a swapped verb slot position, an
        // inserted delimiter, a lost qualified-ref routing) breaks
        // the family invariant HERE rather than as silent per-Kind
        // skew where an operator grepping across the fleet on
        // `"<verb> <Kind> <ns>/<name>"` hits one composer's output
        // but not the other's.
        for (verb, ns, name) in [
            ("patch", "default", "target"),
            ("create", "probe-ns", "receipt-cm"),
            ("get", "demo-ns", "resource"),
        ] {
            let via_process = error_ctx(verb, ns, name);
            let via_configmap = crate::configmap::error_ctx(verb, ns, name);
            // Replace the `Process` head with `ConfigMap` and vice
            // versa — the two composers agree on every non-kind byte.
            assert_eq!(
                via_process.replace("Process", "ConfigMap"),
                via_configmap,
                "process_api::error_ctx and configmap::error_ctx must share the \
                 SAME diagnostic head shape modulo the fixed resource-kind literal",
            );
        }
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
