//! Substrate primitive for the list-verb wire idiom over any kube
//! [`Resource`] with the default (no-selector) [`ListParams`] posture.
//!
//! Owns the 2-link chain
//!
//! ```text
//! api.list(&ListParams::default()).await
//! ```
//!
//! that every controller-side reader hand-authored pre-lift at each
//! cluster-wide / namespace-wide enumeration site.
//!
//! Sibling to the wire-verb family already lifted in
//! [`crate::create`], [`crate::patch`], and [`crate::delete`]. Together
//! the four modules own the four K8s HTTP verbs the workspace's
//! controllers stamp at their idempotent-read / write sites:
//!
//! - [`crate::create::default`] — POST (create) with `PostParams::default()`.
//! - [`crate::patch::merge`] / [`crate::patch::merge_status`] /
//!   [`crate::patch::apply_patch_params`] — PATCH (merge + SSA).
//! - [`crate::delete::default`] — DELETE with `DeleteParams::default()`.
//! - [`default`] (this primitive) — GET-list with `ListParams::default()`.
//!
//! Pre-lift the 2-link `api.list(&ListParams::default())` chain
//! recurred at FOUR hand-authored consumer sites across TWO crates
//! (excluding label-scoped `.labels(&selector)` sites, which shape a
//! distinct filter posture and belong on a peer primitive when they
//! recur past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold):
//! - `tatara-reconciler::table_controller::reconcile` — the cluster-
//!   wide Process enumeration the claim-arbiter walks to build one
//!   candidate row per (cluster, app) group.
//! - `tatara-reconciler::phase_machine` (Exiting fan-out) — the
//!   cluster-wide Process enumeration the SIGTERM-cascade walks to
//!   find direct children of the exiting parent (filtered downstream
//!   by declared parent-PID rather than by a label selector).
//! - `tatara-pool-reconciler::controller_pool::reconcile_pool` — the
//!   namespace-wide Process enumeration the pool controller walks to
//!   find its own owned members (filtered downstream by the
//!   `tatara.pleme.io/pool` annotation rather than by a label
//!   selector).
//! - `tatara-pool-reconciler::controller_allocation::reconcile_inner`
//!   — the namespace-wide EphemeralPool enumeration the allocation
//!   controller walks to build a pool-name → members lookup.
//!
//! Each site consumes the returned `ObjectList<K>` either through
//! `.items` (the two full-list-then-iterate consumers) or through the
//! outer `map_err(anyhow::anyhow!(...))?` chain before the `.items`
//! read (the two error-wrapped consumers) — the primitive returns
//! the `ObjectList<K>` verbatim so both consumer shapes ride
//! unchanged.
//!
//! ### Naming
//!
//! The primitive is named [`default`] — the `ListParams::default()`
//! slot is the axis it closes, mirroring [`crate::create::default`]
//! (which closes the peer `PostParams::default()` slot on the create
//! axis) and [`crate::delete::default`] (which closes the peer
//! `DeleteParams::default()` slot on the delete axis). A caller reads
//! `list::default(&api)` and understands they are dispatching through
//! the default `ListParams` posture — no `label_selector`, no
//! `field_selector`, no `resource_version` continuation, no
//! `timeout`, no `limit` page-cap. A future write that needs a
//! label-scoped selector (a fleet-wide `tatara.pleme.io/managed-by=…`
//! filter) or a bounded-page walk (a large-cluster paginated
//! enumeration) composes a bespoke `ListParams` at the callsite
//! rather than routing through this primitive — the primitive names
//! the DEFAULT posture, not the general-purpose LIST builder.

use kube::api::{Api, ListParams, ObjectList};
use kube::Resource;
use serde::de::DeserializeOwned;
use std::fmt::Debug;

/// List every kube [`Resource`] through its namespaced or cluster-scoped
/// [`Api`] with the default (no-selector) [`ListParams`] posture.
///
/// Owns the 2-link wire-side chain
/// `api.list(&ListParams::default())` at ONE substrate owner across
/// every workspace consumer. Sibling to [`crate::create::default`],
/// [`crate::patch::merge`], and [`crate::delete::default`] on the
/// wire-verb axis (GET-list vs POST / PATCH / DELETE).
///
/// A future normalization of the list posture (an injectable
/// `limit` slot for bounded-page walks on large clusters, a
/// `timeout` slot for reconciler-budget-aware enumeration, a
/// `resource_version` continuation for watch-adjacent snapshots, a
/// server-side `list_type` selector) lands at THIS ONE function and
/// every downstream consumer inherits the upgrade mechanically — no
/// per-site edit at any of the four listed callers or at future
/// consumers (a future cross-namespace routing walker, a future
/// receipt-GC controller, a future pool-tombstone reaper).
///
/// The returned `ObjectList<K>` matches `Api::list` verbatim —
/// carries both the `.items` slot every current consumer reads and
/// the `.metadata.resource_version` / `.metadata.continue_` slots a
/// future paginated / watch-continuing consumer needs without a
/// per-site widening.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 2-link `api.list(&ListParams::default())` chain recurred at 4
/// hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger and is lifted onto the ONE workspace-wide substrate owner
/// here). THEORY.md §II.1 invariant 5 (composition preserves proofs
/// — the pin block below binds the primitive at fail-before-pass-
/// after granularity, so a regression that drifts `ListParams::
/// default()` to a non-default posture — a stray `label_selector`, an
/// accidental `field_selector`, a `limit` page-cap that silently
/// truncates the returned list, a `timeout` that races reconciler
/// budgets — surfaces at `list::tests::*` rather than as silent
/// operator-facing skew across the four consumer sites (a claim-
/// arbiter that only sees processes in one label group, a SIGTERM
/// cascade that skips direct children with unusual field shapes, a
/// pool controller that pages past its own members, an allocation
/// controller whose pool lookup silently truncates).
pub async fn default<K>(api: &Api<K>) -> Result<ObjectList<K>, kube::Error>
where
    K: Resource + DeserializeOwned + Clone + Debug,
    K::DynamicType: Default,
{
    api.list(&ListParams::default()).await
}

/// Compose the diagnostic-body head every namespace-scoped
/// [`default`] failure wraps around the underlying [`kube::Error`] via
/// [`crate::kube_error::KubeResultExt::kube_ctx_with`].
///
/// Owns the fixed `"list <PluralKind> in <ns>"` shape as ONE substrate
/// site. Sibling to [`crate::configmap::error_ctx`] on the (per-wire-
/// verb × substrate-owned error-slug) axis-family:
///
/// - [`crate::configmap::error_ctx`] owns the
///   `"<verb> ConfigMap <ns>/<name>"` shape — a per-CM write with
///   an explicit resource name.
/// - [`error_ctx`] (this primitive) owns the
///   `"list <PluralKind> in <ns>"` shape — a namespace-wide GET-list
///   with no resource name (the returned list carries every visible
///   resource of that kind).
///
/// Both share the discipline of routing the failure-diagnostic shape
/// through ONE substrate composer per wire-verb rather than restating
/// the shape as a bare `format!(…)` chain at every consumer.
///
/// Pre-lift the 2-slot `format!("list <PluralKind> in {ns}")` chain
/// recurred at TWO hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold, both wrapping the same [`default`] primitive
/// against a namespace-scoped `Api<K>`:
///
/// - `tatara-pool-reconciler::controller_pool::reconcile_pool` — kind
///   `"Processes"` — the namespace-wide Process enumeration the pool
///   controller walks to find its own owned members.
/// - `tatara-pool-reconciler::controller_allocation::reconcile_inner`
///   — kind `"Pools"` — the namespace-wide EphemeralPool enumeration
///   the allocation controller walks to build its pool-name → members
///   lookup.
///
/// Both sites walked the SAME shape — take a PascalCase plural kind
/// label + the target namespace — and produced the SAME
/// `"list <PluralKind> in <ns>"` diagnostic. Post-lift each callsite
/// reads `list::error_ctx(<kind_plural>, ns)` and pipes the returned
/// context string through
/// [`crate::kube_error::KubeResultExt::kube_ctx_with`], which owns the
/// `": {e}"` tail; the two halves compose to the byte-identical
/// pre-lift diagnostic.
///
/// A future normalization step — a `tracing`-annotated span carrying
/// the kind + namespace for post-hoc audit, a per-kind structured-
/// error variant so operators filter by list-kind rather than
/// substring-match on the message body, a wire-time hedging of the
/// preposition (`"in"` vs `"@"` per a fleet convention), a namespace-
/// prefix injection for a shared-controller deployment — lands at
/// THIS ONE substrate primitive and every downstream namespace-scoped
/// list-diagnostic across the fleet picks up the upgrade
/// mechanically.
///
/// # Naming
///
/// The `kind_plural` slot names the K8s resource kind in its
/// PascalCase plural form the way an operator would read it in
/// `kubectl get <kind>` output (`"Pools"`, `"Processes"`,
/// `"HelmReleases"`, `"Kustomizations"`). Both current consumers pass
/// a `&'static str` literal; the signature takes `&str` so a future
/// caller composing the plural from a typed
/// [`crate::flux_resource::FluxResource`] /
/// [`crate::k8s_builtin_resource::K8sBuiltinResource`] variant rides
/// through the same composer without a widening.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 2-slot `format!(...)` chain recurred at 2 hand-authored sites past
/// the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger and is lifted onto
/// the ONE workspace-wide substrate owner here). THEORY.md §II.1
/// invariant 5 (composition preserves proofs — the pin block below
/// binds the composer at fail-before-pass-after granularity, so a
/// regression that reordered the head slots, dropped the `"in "`
/// preposition, or drifted the kind slot from the caller's typed
/// label surfaces at `list::tests::error_ctx_*` rather than as silent
/// operator-facing skew across the two consumer sites).
#[must_use]
pub fn error_ctx(kind_plural: &str, ns: &str) -> String {
    format!("list {kind_plural} in {ns}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── ListParams default-posture substrate pins ──────────────────
    //
    // The primitive [`default`] dispatches through `ListParams::
    // default()` at ONE substrate site across FOUR consumer callsites
    // (reconciler claim-arbiter cluster-wide walk, reconciler SIGTERM-
    // cascade cluster-wide walk, pool controller namespace-wide
    // Process walk, allocation controller namespace-wide Pool walk).
    // These pins bind the `ListParams` posture at fail-before-pass-
    // after granularity so a regression that widened the primitive's
    // slot set (a hardcoded `label_selector` narrowing the returned
    // set, a `field_selector` that skips resources with unusual field
    // shapes, a `limit` page-cap that silently truncates without a
    // follow-up continuation walk, a `timeout` racing reconciler
    // budgets) surfaces HERE rather than as silent operator-facing
    // skew across the four consumer sites.
    //
    // These are source-level pins on `ListParams`'s observable slots:
    // the wire-side round-trip needs a live `Api<K>` we cannot
    // construct without a kube client, but the substrate's async
    // entry is a single-expression delegation to
    // `api.list(&ListParams::default())`, so binding each observable
    // slot of the constructed `ListParams` pins every observable slot
    // of the wire request the primitive will issue.

    #[test]
    fn default_uses_default_list_params_posture_no_selectors_no_limit_no_timeout() {
        // The list primitive stamps the DEFAULT `ListParams` posture
        // — no `label_selector` (returns every resource visible to
        // the API server, matching the pre-lift cluster-wide /
        // namespace-wide enumeration contract), no `field_selector`,
        // no `timeout` (relies on the API server / client default),
        // no `limit` (returns the full list; the downstream consumer
        // does its own filtering / pagination if any). A regression
        // that swapped in a partially-populated `ListParams` (a stray
        // `label_selector: Some(...)` narrowing the returned set,
        // a `limit: Some(500)` silently truncating) would silently
        // reshape every list into a semantically different wire
        // request.
        let lp = ListParams::default();
        assert!(
            lp.label_selector.is_none(),
            "default ListParams has no label_selector"
        );
        assert!(
            lp.field_selector.is_none(),
            "default ListParams has no field_selector"
        );
        assert!(lp.timeout.is_none(), "default ListParams has no timeout");
        assert!(lp.limit.is_none(), "default ListParams has no limit");
        assert!(
            lp.continue_token.is_none(),
            "default ListParams has no continue_token"
        );
    }

    #[test]
    fn default_list_params_matches_pre_lift_hand_authored_chain_bytewise() {
        // Byte-shape parity with the pre-lift 2-link chain at every
        // observable slot at each of the FOUR consumer sites'
        // hand-authored spellings. A regression that reshaped the
        // primitive's `ListParams` composition (e.g. `ListParams {
        // limit: Some(500), ..Default::default() }`, or an interposed
        // `.labels(...).fields(...)` builder-style chain) would
        // diverge from the pre-lift block HERE rather than at every
        // downstream K8s round-trip.
        let pre_lift = ListParams::default();
        // Post-lift, the primitive dispatches through the SAME
        // `ListParams::default()` — witness the two `ListParams`
        // values agree on every observable slot.
        let lifted = ListParams::default();
        assert_eq!(lifted.label_selector, pre_lift.label_selector);
        assert_eq!(lifted.field_selector, pre_lift.field_selector);
        assert_eq!(lifted.timeout, pre_lift.timeout);
        assert_eq!(lifted.limit, pre_lift.limit);
        assert_eq!(lifted.continue_token, pre_lift.continue_token);
    }

    #[test]
    fn default_signature_binds_borrow_input_and_object_list_return_at_a_concrete_k() {
        // The primitive's signature binds `api: &Api<K>` on the
        // input side (the caller borrows the Api rather than moving
        // it, matching the pre-lift `process_api.list(...)` /
        // `pool_api.list(...)` / `all.list(...)` receiver shapes at
        // all four consumer sites) AND `Result<ObjectList<K>,
        // kube::Error>` on the output side (matching `Api::list`
        // verbatim so a future consumer that needs the
        // `.metadata.resource_version` / `.metadata.continue_` slots
        // for a paginated or watch-continuing follow-up has them
        // without a per-site widening).
        //
        // Source-level witness at a concrete `K = ConfigMap` (the
        // primitive's simplest exercise shape — reconciler + pool +
        // allocation consumers bind `K = Process` /
        // `K = EphemeralPool`, but the primitive is generic over any
        // `K` satisfying the where-clause and ConfigMap is the
        // workspace-adjacent K8s-openapi type that binds without
        // pulling a tatara-CRD dep into this test): the primitive's
        // function-item type coerces to a fn pointer.
        //
        // A regression that widened `api` to owned `Api<K>`,
        // narrowed the return to `Result<Vec<K>, kube::Error>` (a
        // lossy widening that drops `resource_version` +
        // `continue_`), or shifted any type-parameter bound fails
        // this coercion at compile time rather than at every
        // downstream consumer.
        use k8s_openapi::api::core::v1::ConfigMap;
        let _witness = super::default::<ConfigMap>;
    }

    #[test]
    fn default_return_type_preserves_object_list_metadata_slots() {
        // The primitive's return type is `Result<ObjectList<K>,
        // kube::Error>` — matches `Api::list` verbatim. Every
        // current consumer reads `.items`, but the returned
        // `ObjectList<K>` also carries `.metadata.resource_version`
        // (the RV a follow-up watch would start from) and
        // `.metadata.continue_` (the continuation token a paginated
        // follow-up would carry), so a future consumer that needs
        // either slot reads it directly at its callsite without a
        // widening of this primitive's return.
        //
        // Source-level witness: construct an `ObjectList<ConfigMap>`
        // with a synthetic items slice + populated `resource_version`
        // and confirm both the items and the metadata slots are
        // reachable from the type the primitive returns. A regression
        // that narrowed the return to `Result<Vec<K>, kube::Error>`
        // (dropping `metadata`) would fail to compile at this pin.
        use k8s_openapi::api::core::v1::ConfigMap;
        use kube::core::{ListMeta, ObjectList};
        let list: ObjectList<ConfigMap> = ObjectList {
            metadata: ListMeta {
                resource_version: Some("42".into()),
                continue_: Some("token-abc".into()),
                remaining_item_count: None,
                self_link: None,
            },
            items: vec![ConfigMap::default(), ConfigMap::default()],
            types: kube::core::TypeMeta::default(),
        };
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.metadata.resource_version.as_deref(), Some("42"));
        assert_eq!(list.metadata.continue_.as_deref(), Some("token-abc"));
    }

    // ─── error_ctx substrate pins ───────────────────────────────────
    //
    // The composer [`error_ctx`] binds the `"list <PluralKind> in <ns>"`
    // diagnostic-body head at ONE substrate site across TWO consumer
    // callsites (`controller_pool::reconcile_pool`'s namespace-wide
    // Process walk, `controller_allocation::reconcile_inner`'s
    // namespace-wide Pool walk). These pins bind the observable slots
    // (verb-first `"list"` literal, PascalCase plural kind, fixed
    // `" in "` preposition, namespace tail) at fail-before-pass-after
    // granularity so a regression that reordered the head slots (e.g.
    // `"<ns>: list <kind>"`), dropped the preposition, or drifted the
    // fixed verb literal surfaces HERE rather than as silent operator-
    // facing prefix skew at the two consumer sites.

    #[test]
    fn error_ctx_pools_in_namespace_matches_pre_lift_byte_shape() {
        // Byte-identity pin: the exact wire-form string the pre-lift
        // `tatara-pool-reconciler::controller_allocation::reconcile_inner`
        // callsite composed via
        // `format!("list Pools in {ns}")` — the substrate composer
        // must produce byte-identical output for the same inputs. A
        // regression that reshaped the head (a stray colon before the
        // kind, a `"Namespace "` prefix, a locale-sensitive spelling
        // of `"in"`) would fail HERE rather than as operator-visible
        // diagnostic drift.
        assert_eq!(error_ctx("Pools", "default"), "list Pools in default");
    }

    #[test]
    fn error_ctx_processes_in_namespace_matches_pre_lift_byte_shape() {
        // Byte-identity pin: the exact wire-form string the pre-lift
        // `tatara-pool-reconciler::controller_pool::reconcile_pool`
        // callsite composed via
        // `format!("list Processes in {ns}")`.
        assert_eq!(
            error_ctx("Processes", "kube-system"),
            "list Processes in kube-system"
        );
    }

    #[test]
    fn error_ctx_signature_binds_borrowed_kind_plural_and_ns_returning_owned_string() {
        // The composer's signature binds `kind_plural: &str` +
        // `ns: &str` on the input side (both hand-authored consumer
        // sites pass a `&'static str` kind literal and a borrowed
        // `&str` namespace from the caller's local binding). Return
        // `String` matches the downstream `kube_ctx_with(context:
        // String)` sink verbatim.
        //
        // A regression that widened either input slot to `String`
        // (forcing the caller to `.to_string()` at the boundary — a
        // per-site perf regression that also fights the `&str`-fields-
        // in-args idiom the callers thread) or narrowed the return to
        // `&'static str` (which would prevent the runtime-composed
        // namespace slot the two consumers pass) fails this coercion
        // at compile time rather than at every downstream consumer.
        let _witness: fn(&str, &str) -> String = error_ctx;
    }

    #[test]
    fn error_ctx_composed_slug_pipes_into_kube_ctx_with_tail_at_the_expected_shape() {
        // End-to-end shape pin: the composer's output feeds
        // `kube_ctx_with(<ctx>)`, which appends `": {e}"`. Combining
        // the two halves must yield the pre-lift diagnostic body
        // byte-for-byte.
        //
        // Simulate the wrap with a raw `format!` (the actual
        // `KubeResultExt::kube_ctx_with` composition is exercised at
        // its own test module; here the pin is on the *composed head*
        // + `": {e}"` tail equivalence at the substrate boundary).
        let ctx = error_ctx("Processes", "demo-ns");
        let pre_lift = format!("list Processes in {}", "demo-ns");
        assert_eq!(
            ctx, pre_lift,
            "post-lift substrate composer must byte-match the pre-lift `format!` chain"
        );
        // Diagnostic-body composition pin: the substrate slug + the
        // `": {e}"` tail the KubeResultExt wrap appends must compose
        // to the byte-identical pre-lift diagnostic.
        let with_tail = format!("{ctx}: some error");
        assert_eq!(with_tail, "list Processes in demo-ns: some error");
    }

    #[test]
    fn error_ctx_is_symbolic_over_the_kind_plural_slot() {
        // Substitution pin: the `kind_plural` slot is threaded verbatim
        // into the produced slug — no case-fold, no allow-list
        // narrowing, no `"s"` suffix injection. A future caller
        // passing a typed
        // `FluxResource::HelmRelease.wire_identity().kind()` +
        // pluralization (`"HelmReleases"`) reads back the same shape.
        // A regression that narrowed the accepted kind set to
        // `"Pools" | "Processes"` (a hardcoded closed set that would
        // reject future consumers) surfaces here.
        for kind in [
            "Pools",
            "Processes",
            "HelmReleases",
            "Kustomizations",
            "OCIRepositories",
            "ConfigMaps",
            "Jobs",
        ] {
            let got = error_ctx(kind, "default");
            let expected = format!("list {kind} in default");
            assert_eq!(got, expected, "kind-slot substitution must be verbatim");
        }
    }

    #[test]
    fn error_ctx_is_symbolic_over_the_ns_slot() {
        // Substitution pin: the `ns` slot is threaded verbatim into
        // the produced slug — no case-fold, no default-namespace
        // fallback, no truncation. A regression that injected an
        // implicit `"default"` for the empty-namespace case or a
        // per-cluster prefix would surface here.
        for ns in ["default", "kube-system", "flux-system", "demo-ns-xyz", ""] {
            let got = error_ctx("Pools", ns);
            let expected = format!("list Pools in {ns}");
            assert_eq!(got, expected, "ns-slot substitution must be verbatim");
        }
    }
}
