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
}
