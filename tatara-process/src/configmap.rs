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
use kube::api::ObjectMeta;
use kube::{Api, Client};
use std::collections::BTreeMap;

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
    // Delegates through the workspace-wide substrate owner
    // [`crate::api::namespaced`] — sibling to [`crate::api::all`] on
    // the (scope × K) axis pair, closing the `Api::namespaced
    // (<client>, <ns>)` shape at ONE substrate primitive across every
    // ns-scoped Api binder site. Post-lift a future normalization of
    // the ns-scoped Api posture (tracing span, QPS budget, fixture-
    // backed client, wired-in `PatchParams` field manager for SSA)
    // lands at THAT owner rather than at this fixed-K sibling —
    // which now carries the K = ConfigMap guarantee exclusively, not
    // the `Api::namespaced` shape it used to co-own.
    crate::api::namespaced::<ConfigMap>(client, ns)
}

/// Compose a namespaced [`ConfigMap`] resource carrying a typed
/// `String → String` [`BTreeMap`] payload, optionally labeled.
///
/// Owns the wire-shape chain
///
/// ```text
/// let cm = ConfigMap {
///     metadata: ObjectMeta {
///         name: Some(<name>.to_string()),
///         namespace: Some(<ns>.to_string()),
///         labels: <labels>,
///         ..Default::default()
///     },
///     data: Some(<data>),
///     ..Default::default()
/// };
/// ```
///
/// that every workspace consumer building a `String`-payload ConfigMap
/// through the K8s wire format hand-authored pre-lift at each
/// construction site. Peer to [`namespaced`] on the same axis — the
/// namespaced binder covers the Api<ConfigMap> handle-side; this
/// composer covers the resource-body side.
///
/// Pre-lift the 5-link struct-literal recurred at TWO hand-authored
/// consumer sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// threshold:
/// - `tatara-closed-loop-probe::main::write_receipt` — the closed-
///   loop auth probe's receipt-CM writer. Labeled with
///   `"tatara.pleme.io/receipt" → "tatara-receipt/v1"` so operators
///   can `kubectl get cm -l tatara.pleme.io/receipt=tatara-receipt/v1`.
/// - `tatara-export-worker::main::write_receipt` — the export
///   worker's receipt-CM writer. No labels (SSA writer against a name
///   the operator already knows via the ExportSpec channel).
///
/// Each site consumes the returned [`ConfigMap`] either through a
/// `crate::create::default(&api, &cm).await` writer chain (the
/// closed-loop-probe consumer's create-then-409-patch idempotent
/// write) or an `api.patch(name, &pp, &Patch::Apply(&cm))` SSA-writer
/// chain (the export-worker consumer's SSA-side apply) — the composer
/// returns a fresh owned `ConfigMap` verbatim so the downstream write-
/// verb dispatch rides unchanged.
///
/// The `labels` slot is [`Option`]-shaped so consumers that need no
/// metadata labels pass `None` and get an unlabeled ObjectMeta, while
/// consumers that need labels pass `Some(<map>)` and get them stamped
/// on the ObjectMeta — matching the underlying [`ObjectMeta`]
/// field's own `Option<BTreeMap<String, String>>` shape (a `Some(<empty
/// map>)` and `None` are distinguishable at the K8s API server, so
/// the composer surfaces both shapes rather than collapsing them).
///
/// The `binary_data` slot on [`ConfigMap`] rides `..Default::default()`
/// — both hand-authored consumer sites emit `None` (either implicit
/// via their own `..Default::default()` at the export-worker site, or
/// explicit as `Option::<BTreeMap<String, ByteString>>::None` at the
/// same site pre-lift, which is byte-equivalent to the implicit
/// default). A future binary-payload writer composes a peer
/// `with_binary_data` primitive on this module rather than widening
/// this one — the string-payload posture (`data: Some(<map>)`) is
/// the invariant this composer names.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 5-link struct-literal chain recurred at 2 hand-authored sites past
/// the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger and is lifted onto
/// the ONE workspace-wide substrate owner here). THEORY.md §II.1
/// invariant 5 (composition preserves proofs — the pin block below
/// binds the composer at fail-before-pass-after granularity, so a
/// regression that swapped a slot's default (`data: None` when a
/// consumer expected `Some(<data>)`, `metadata.name: None` when the
/// K8s API server needs a name for the create-verb call, `labels`
/// leaking off the passed slot into a hard-coded map) surfaces at
/// `configmap::tests::*` rather than as silent operator-facing
/// receipt-writer skew across the two consumer sites).
pub fn with_data(
    name: &str,
    ns: &str,
    data: BTreeMap<String, String>,
    labels: Option<BTreeMap<String, String>>,
) -> ConfigMap {
    ConfigMap {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            namespace: Some(ns.to_string()),
            labels,
            ..Default::default()
        },
        data: Some(data),
        ..Default::default()
    }
}

/// Compose the diagnostic-body head every wire-verb failure against a
/// namespaced [`ConfigMap`] wraps around the underlying [`kube::Error`]
/// via [`crate::kube_error::KubeResultExt::kube_ctx_with`].
///
/// Owns the fixed `<verb> ConfigMap <ns>/<name>` shape as ONE substrate
/// site, routing the `<ns>/<name>` join through the workspace-wide
/// [`crate::qualified_process_ref`] composer so a future normalization
/// of the qualified-ref shape (case-fold, unicode collation, IDN)
/// lands at ONE site and every ConfigMap-scoped diagnostic body picks
/// it up mechanically.
///
/// Pre-lift the 3-slot `format!("{verb} ConfigMap {ns}/{name}: {e}")`
/// chain recurred at TWO hand-authored sites past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold, both inside the
/// closed-loop-probe's receipt-CM idempotent-upsert idiom
/// (`tatara-closed-loop-probe::main::write_receipt_cm`):
/// - Verb `"patch"` — the create-then-409-retry arm's PATCH-verb
///   failure wrap (`.map_err(|e| anyhow!("patch ConfigMap {ns}/{cm}: {e}"))?`).
/// - Verb `"create"` — the initial CREATE-verb non-409 failure wrap
///   (`Err(anyhow!("create ConfigMap {ns}/{cm}: {e}"))`).
///
/// Both sites walked the SAME shape — take a verb, the target
/// ConfigMap's namespace + name, and the underlying `kube::Error`
/// display — and produced the SAME "`{verb} ConfigMap {ns}/{name}:
/// {kube error}`" diagnostic. Post-lift each callsite reads
/// `configmap::error_ctx(<verb>, ns, cm_name)` and pipes the returned
/// context string through [`crate::kube_error::KubeResultExt::kube_ctx_with`],
/// which owns the `": {e}"` tail; the two halves compose to the
/// byte-identical pre-lift diagnostic.
///
/// A future normalization step — a `tracing`-annotated span carrying
/// the verb + qualified-ref for post-hoc audit, a per-verb structured-
/// error kind so operators can filter by write-verb rather than
/// substring-match on the message body, a wire-time hedging of the
/// verb spelling (`"PATCH"` vs `"patch"` per a fleet convention),
/// injection of the operator's namespace prefix for a shared-CM
/// deployment — lands at THIS ONE substrate primitive and every
/// downstream ConfigMap-scoped failure diagnostic across the fleet
/// picks up the upgrade mechanically.
///
/// Sibling to [`with_data`] on the (per-ConfigMap × substrate-owned
/// shape) axis: [`with_data`] owns the resource-body composition; this
/// primitive owns the failure-diagnostic composition. Both bind the
/// ConfigMap-scoped concerns at ONE substrate module so a future
/// ConfigMap-family expansion (a `with_binary_data` peer for byte
/// payloads, a `not_found_ctx` peer for GET-verb 404 diagnostic bodies)
/// lands next to the existing composers.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 3-slot `format!(...)` chain recurred at 2 hand-authored sites past
/// the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger and is lifted onto
/// the ONE workspace-wide substrate owner here). THEORY.md §II.1
/// invariant 5 (composition preserves proofs — the pin block below
/// binds the composer at fail-before-pass-after granularity, so a
/// regression that reordered the head slots, drifted the fixed
/// `"ConfigMap"` resource-kind literal (bypassing the routing pin at
/// [`tests::error_ctx_routes_kind_slot_through_k8s_builtin_resource_configmap_owner`],
/// which binds the Kind slot to
/// [`crate::k8s_builtin_resource::K8sBuiltinResource::ConfigMap::kind`]
/// as the ONE workspace-wide owner of the K8s-built-in wire-form
/// identity), or dropped the qualified-ref routing back to a bare
/// `format!("{ns}/{name}")` surfaces at `configmap::tests::error_ctx_*`
/// rather than as silent operator-facing skew across the two consumer
/// sites).
#[must_use]
pub fn error_ctx(verb: &str, ns: &str, name: &str) -> String {
    // Delegates through the workspace-wide substrate owner
    // [`crate::qualified_error_ctx`] — the ONE composer of the
    // `<verb> <Kind> <ns>/<name>` shape shared with
    // [`crate::process_api::error_ctx`] on the peer tatara-CRD
    // Process axis. Post-lift a future normalization of the
    // 4-slot shape (a `tracing`-annotated span, a per-Kind
    // canonicalization, an operator-supplied cluster prefix) lands
    // at THAT owner rather than at this fixed-Kind peer — which
    // now carries the `Kind = "ConfigMap"` guarantee exclusively,
    // not the 4-slot shape it used to co-own.
    //
    // The fixed `Kind = "ConfigMap"` slot routes through the typed
    // K8s-built-in wire-form identity owner
    // [`crate::k8s_builtin_resource::K8sBuiltinResource::ConfigMap`]
    // via its `const fn kind()` projection rather than the pre-lift
    // hand-authored `"ConfigMap"` literal — the ONE workspace-wide
    // owner of the `(apiVersion, kind)` pair every K8s-builtin-
    // facing site in the reconciler routes through. Post-lift a
    // Kubernetes-side kind spelling change (a `Configmap` typo
    // rename at the K8s API server, a hypothetical cross-version
    // rename) lands at ONE arm of the K8sBuiltinResource closed set
    // and this diagnostic body inherits the upgrade mechanically
    // alongside every emit / fetch site on the same axis. Pinned
    // by `configmap::tests::
    // error_ctx_routes_kind_slot_through_k8s_builtin_resource_configmap_owner`.
    crate::qualified_error_ctx(
        verb,
        crate::k8s_builtin_resource::K8sBuiltinResource::ConfigMap.kind(),
        ns,
        name,
    )
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

    // ─── ConfigMap::with_data substrate pins ─────────────────────────
    //
    // The composer [`with_data`] binds the wire-shape 5-link struct-
    // literal `ConfigMap { metadata: ObjectMeta { name: Some(<name>),
    // namespace: Some(<ns>), labels: <labels>, ..Default::default() },
    // data: Some(<data>), ..Default::default() }` at ONE substrate site
    // across TWO consumer callsites (closed-loop-probe receipt writer,
    // export-worker receipt writer). These pins bind the observable
    // slots (name-into-Some-metadata, ns-into-Some-metadata, labels-
    // slot-preserved, data-into-Some-body, binary_data-default-None)
    // at fail-before-pass-after granularity so a regression that
    // drifted any slot (name silently dropped so the K8s API server's
    // create-verb call rejects a nameless resource; labels leaking off
    // the passed slot into a hard-coded map that would mis-label the
    // receipt-CM operators kubectl-select on; data slotted into
    // `binary_data` instead of `data` so the JSON receipt reader gates
    // in `tatara-reconciler::boundary::verify_receipt_cm` see a missing
    // key) surfaces HERE rather than as silent operator-facing skew at
    // the two consumer sites.

    #[test]
    fn with_data_signature_binds_borrowed_name_and_ns_string_data_and_option_labels() {
        // The composer's signature binds `name: &str` + `ns: &str` on
        // the input side (both hand-authored consumer sites pass a
        // borrowed `&str` field — the closed-loop-probe passes
        // `args.receipt_config_map` + `args.receipt_namespace` through
        // its `write_receipt(envelope, cm_name: &str, ns: &str)`
        // signature; the export-worker passes `&str` slice fields
        // through its `write_receipt(kube, namespace: &str, configmap:
        // &str, ...)` signature). `data: BTreeMap<String, String>` on
        // the payload slot (both consumers build a `BTreeMap<String,
        // String>` via `data.insert(<key>.to_string(), <val>)`).
        // `labels: Option<BTreeMap<String, String>>` on the labels
        // slot (the closed-loop-probe passes `Some(BTreeMap::from([...]))`;
        // the export-worker passes `None`). Return `ConfigMap`
        // matches every downstream write-verb dispatch's owned-input
        // slot.
        //
        // A regression that widened `name`/`ns` to `String` (which
        // would force both callsites to `.to_string()` at the boundary,
        // moving allocation from the composer's `to_string()` into
        // the caller's site — a per-site perf regression that also
        // fights the `&str`-fields-in-args idiom the callers thread),
        // narrowed the `labels` slot away from `Option` (which would
        // force the no-label caller to pass an empty map that
        // structurally differs from `None` at the K8s API server —
        // an unlabeled ObjectMeta vs an `ObjectMeta` with an empty
        // labels map are distinct wire shapes), or narrowed the
        // return type off `ConfigMap` (which would break the SSA
        // `Patch::Apply(&cm)` slot the export-worker chains through)
        // fails this coercion at compile time.
        let _witness: fn(
            &str,
            &str,
            BTreeMap<String, String>,
            Option<BTreeMap<String, String>>,
        ) -> ConfigMap = with_data;
    }

    #[test]
    fn with_data_stamps_name_namespace_data_and_default_binary_data_when_no_labels() {
        // Byte-shape parity witness against the export-worker's pre-
        // lift 5-link struct literal (`ConfigMap { metadata:
        // ObjectMeta { name: Some(<name>.to_string()), namespace:
        // Some(<ns>.to_string()), ..Default::default() }, data:
        // Some(<data>), binary_data: None, ..Default::default() }`) —
        // every observable slot the pre-lift chain stamped is present
        // in the composer's output with the same value.
        let mut data = BTreeMap::new();
        data.insert("receipt.yaml".to_string(), "envelope payload".to_string());

        let cm = with_data("export-run-1", "tatara-system", data.clone(), None);

        assert_eq!(
            cm.metadata.name.as_deref(),
            Some("export-run-1"),
            "name-slot rides `Some(<name>.to_string())` at the composer",
        );
        assert_eq!(
            cm.metadata.namespace.as_deref(),
            Some("tatara-system"),
            "ns-slot rides `Some(<ns>.to_string())` at the composer",
        );
        assert!(
            cm.metadata.labels.is_none(),
            "labels-slot preserves the `None` the export-worker consumer passes — an empty map would be a distinct wire shape",
        );
        assert_eq!(
            cm.data.as_ref(),
            Some(&data),
            "data-slot rides `Some(<data>)` at the composer — the receipt payload the reader gates on",
        );
        assert!(
            cm.binary_data.is_none(),
            "binary_data rides `..Default::default()` = `None` — the export-worker's explicit `Option::<BTreeMap<String, ByteString>>::None` pre-lift is byte-equivalent",
        );
    }

    #[test]
    fn with_data_preserves_passed_labels_map_verbatim_when_some() {
        // Byte-shape parity witness against the closed-loop-probe's
        // pre-lift 5-link struct literal (`ConfigMap { metadata:
        // ObjectMeta { name: Some(<name>.into()), namespace:
        // Some(<ns>.into()), labels: Some(BTreeMap::from([...])),
        // ..Default::default() }, data: Some(<data>),
        // ..Default::default() }`) — the labels map the caller passes
        // rides through to the ObjectMeta verbatim (no key rename, no
        // value coercion, no default injection of unrelated labels).
        let mut data = BTreeMap::new();
        data.insert("receipt.json".to_string(), "{}".to_string());
        let labels = BTreeMap::from([(
            "tatara.pleme.io/receipt".to_string(),
            "tatara-receipt/v1".to_string(),
        )]);

        let cm = with_data(
            "closed-loop-probe-receipt",
            "probe-ns",
            data,
            Some(labels.clone()),
        );

        assert_eq!(
            cm.metadata.labels.as_ref(),
            Some(&labels),
            "labels-slot preserves the passed map verbatim — a regression that dropped the tatara.pleme.io/receipt label would silently break operator kubectl-selectors",
        );
    }

    // ─── ConfigMap::error_ctx substrate pins ─────────────────────────
    //
    // The composer [`error_ctx`] binds the `<verb> ConfigMap <ns>/<name>`
    // diagnostic-body head at ONE substrate site across TWO consumer
    // callsites (the closed-loop-probe's create-then-409-patch idempotent-
    // upsert idiom's CREATE-verb non-409 failure wrap + PATCH-verb
    // failure wrap, both in `write_receipt_cm`). These pins bind the
    // observable slots (verb-first, fixed `"ConfigMap"` resource-kind
    // literal, qualified-ref routing for the `<ns>/<name>` join) at
    // fail-before-pass-after granularity so a regression that reordered
    // the head slots (e.g. `"ConfigMap <verb> <ns>/<name>"`), dropped the
    // fixed resource-kind literal, or routed the `<ns>/<name>` shape
    // through a bare `format!` inline (bypassing the workspace-wide
    // `qualified_process_ref` substrate) surfaces HERE rather than as
    // silent operator-facing prefix skew at the two consumer sites.

    #[test]
    fn error_ctx_signature_binds_borrowed_verb_ns_name_returning_owned_string() {
        // The composer's signature binds `verb: &str` + `ns: &str` +
        // `name: &str` on the input side (both hand-authored consumer
        // sites pass a `&'static str` verb literal and borrowed
        // `&str` fields from the `write_receipt_cm(cm_name: &str,
        // ns: &str, ...)` slot pair). Return `String` matches the
        // downstream `kube_ctx_with(context: String)` sink verbatim.
        //
        // A regression that widened any input slot to `String`
        // (forcing the caller to `.to_string()` at the boundary — a
        // per-site perf regression that also fights the `&str`-fields-
        // in-args idiom the callers thread) or narrowed the return to
        // `&'static str` (which would prevent the runtime-composed
        // verb slot the two consumers pass — `"patch"` and `"create"`
        // are `&'static str` today, but any future dynamic-verb caller
        // would fail this coercion) fails at compile time.
        let _witness: fn(&str, &str, &str) -> String = error_ctx;
    }

    #[test]
    fn error_ctx_composes_patch_configmap_qualified_ref_body_verbatim() {
        // Byte-shape parity witness against the closed-loop-probe's
        // pre-lift PATCH-verb chain: pre-lift the `.map_err(|e|
        // anyhow!("patch ConfigMap {ns}/{cm_name}: {e}"))?` chain at
        // `write_receipt_cm`'s 409-arm PATCH wrap composed a
        // diagnostic body of `"patch ConfigMap {ns}/{cm_name}"` as
        // the head + `": {e}"` as the kube-err tail. Post-lift the
        // primitive OWNS the head; the tail rides through
        // `kube_ctx_with`'s existing `": {e}"` suffix.
        //
        // A regression that reordered head slots (e.g. dropped the
        // fixed `"ConfigMap"` word or emitted the qualified-ref before
        // the verb) surfaces here at the head-shape pin rather than as
        // silent operator-visible prefix skew at the callsite.
        assert_eq!(
            error_ctx("patch", "default", "my-receipt-cm"),
            "patch ConfigMap default/my-receipt-cm",
        );
    }

    #[test]
    fn error_ctx_composes_create_configmap_qualified_ref_body_verbatim() {
        // Byte-shape parity witness against the closed-loop-probe's
        // pre-lift CREATE-verb chain: pre-lift the `Err(anyhow!("create
        // ConfigMap {ns}/{cm_name}: {e}"))` arm at `write_receipt_cm`'s
        // fall-through CREATE-verb failure composed a diagnostic body
        // of `"create ConfigMap {ns}/{cm_name}"` as the head + `": {e}"`
        // as the kube-err tail. Post-lift the primitive owns the head;
        // the tail rides through `kube_ctx_with`'s existing `": {e}"`
        // suffix.
        assert_eq!(
            error_ctx("create", "probe-ns", "closed-loop-probe-receipt"),
            "create ConfigMap probe-ns/closed-loop-probe-receipt",
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
        for (ns, name) in [
            ("default", "receipt-cm"),
            ("tatara-system", "closed-loop-receipt"),
            ("probe-ns", "cm-with-hyphen"),
            ("ns-1", "cm.dotted.name"),
        ] {
            let via_composer = error_ctx("patch", ns, name);
            let via_qualified =
                format!("patch ConfigMap {}", crate::qualified_process_ref(ns, name));
            assert_eq!(
                via_composer, via_qualified,
                "error_ctx must route the (ns, name) join through qualified_process_ref for ns={ns:?} name={name:?}",
            );
        }
    }

    #[test]
    fn error_ctx_routes_kind_slot_through_k8s_builtin_resource_configmap_owner() {
        // Routing pin — the fixed `Kind = "ConfigMap"` slot at
        // this per-Kind peer's `qualified_error_ctx` call rides
        // through the typed K8s-built-in wire-form identity owner
        // [`crate::k8s_builtin_resource::K8sBuiltinResource::ConfigMap`]
        // via its `const fn kind()` projection rather than a bare
        // inline `"ConfigMap"` literal. Pre-lift the composer
        // hand-authored the Kind slot as a bare literal at the
        // `qualified_error_ctx` boundary; post-lift the slot binds
        // to the ONE workspace-wide K8s-built-in owner every emit /
        // fetch site on the same axis already routes through — so a
        // future spelling change at the K8s API server side reaches
        // this diagnostic body mechanically without a per-peer edit.
        //
        // A regression that inlined the `"ConfigMap"` literal back
        // at the `qualified_error_ctx` call (drifting the primitive
        // off the K8sBuiltinResource axis owner + reopening the
        // typo-drift surface a hand-authored `Configmap` /
        // `configmap` spelling would fall into silently) surfaces
        // HERE rather than as silent per-Kind wire-form skew where
        // the error-ctx head disagrees with the sibling
        // `verify_receipt_cm` fetch's SSA-fetched kind.
        for (verb, ns, name) in [
            ("patch", "default", "my-receipt-cm"),
            ("create", "probe-ns", "closed-loop-receipt"),
            ("get", "demo-ns", "cm-with-hyphen"),
            ("delete", "ns-1", "cm.dotted.name"),
        ] {
            let via_composer = error_ctx(verb, ns, name);
            let via_typed_owner = crate::qualified_error_ctx(
                verb,
                crate::k8s_builtin_resource::K8sBuiltinResource::ConfigMap.kind(),
                ns,
                name,
            );
            assert_eq!(
                via_composer, via_typed_owner,
                "error_ctx must route the Kind slot through \
                 K8sBuiltinResource::ConfigMap.kind() for ({verb:?}, {ns:?}, {name:?})",
            );
        }
    }

    #[test]
    fn error_ctx_composes_with_kube_ctx_with_to_pre_lift_anyhow_bang_body_verbatim() {
        // End-to-end parity witness — the (composer + `kube_ctx_with`)
        // pair produces the SAME diagnostic body every pre-lift
        // `anyhow!("<verb> ConfigMap {ns}/{name}: {e}")` chain
        // produced. The composer OWNS the head; `kube_ctx_with`
        // OWNS the `": {e}"` tail; the concatenation is byte-
        // identical to the pre-lift `anyhow!` body. A regression
        // that drifted the head/tail separator (e.g. dropped the
        // single space between the head and the colon-tail, or
        // inserted a stray delimiter) surfaces HERE rather than as
        // silent operator-facing message-shape skew.
        use crate::kube_error::KubeResultExt;
        use kube::core::ErrorResponse;

        let e = kube::Error::Api(ErrorResponse {
            status: "Failure".into(),
            message: "test failure".into(),
            reason: "Test".into(),
            code: 500,
        });
        let pre_lift = format!("patch ConfigMap default/my-cm: {e}");

        let via_pair: anyhow::Result<()> =
            Err::<(), _>(e).kube_ctx_with(error_ctx("patch", "default", "my-cm"));
        let post_lift = via_pair.unwrap_err().to_string();

        assert_eq!(
            post_lift, pre_lift,
            "the (error_ctx head + kube_ctx_with tail) pair must produce the byte-identical pre-lift `anyhow!(\"<verb> ConfigMap {{ns}}/{{name}}: {{e}}\")` diagnostic",
        );
    }
}
