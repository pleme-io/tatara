//! Allocation controller — applies `AllocationDecision`.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::Utc;
use kube::runtime::controller::Action;
use serde_json::json;
use tracing::{info, warn};

use tatara_process::allocation::{AllocationPhase, AllocationStatus, EphemeralAllocation};
use tatara_process::annotations;
use tatara_process::lifetime::{EphemeralLifetime, Lifetime, TeardownPolicy};
use tatara_process::pool::{AllocationRef, PoolMember};
use tatara_process::prelude::NamespacedApiCoordinates;

use crate::allocation_decide::{decide_allocation_reconcile, AllocationDecision};
use crate::context::PoolContext;
use crate::ReconcilerError;

const ALLOC_FINALIZER: &str = "tatara.pleme.io/allocation-finalizer";

pub async fn reconcile(
    alloc: Arc<EphemeralAllocation>,
    ctx: Arc<PoolContext>,
) -> std::result::Result<Action, ReconcilerError> {
    reconcile_inner(alloc, ctx).await.map_err(Into::into)
}

async fn reconcile_inner(alloc: Arc<EphemeralAllocation>, ctx: Arc<PoolContext>) -> Result<Action> {
    // The (namespace, name) API-path pair rides through the substrate
    // trait `tatara_process::NamespacedApiCoordinates` (blanket-implemented
    // over every `kube::Resource<DynamicType = ()>` in the workspace) —
    // pre-lift this was a hand-authored paired 5-line `.metadata.<slot>
    // .clone().ok_or_else(|| anyhow!("Allocation has no metadata.<slot>"))?`
    // chain, sibling to the pool reconciler's own top-level gate. Both
    // sites walked the SAME shape and funneled every downstream
    // `Api::namespaced` + `Api::patch_status` call through the same
    // extracted `(ns, name)` tuple. Post-lift the trait's blanket impl
    // owns the extraction; the error prefix now spells the canonical
    // kube kind (`EphemeralAllocation`) rather than the pre-lift short-
    // form (`Allocation`), matching `kubectl get ephemeralallocations`
    // output verbatim.
    let (ns, name) = alloc.owned_coordinates_required()?;

    // All three `Api<T>` handles ride through the substrate
    // primitives `PoolContext::{allocation_api, pool_api, process_api}`
    // — pre-lift each slot was a hand-authored `Api::namespaced(
    // ctx.kube.clone(), &ns)` chain, and the two collections shared
    // with `controller_pool::reconcile_inner` (`Api<EphemeralPool>` +
    // `Api<Process>`) each lived at TWO workspace-wide restatements
    // past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold. The
    // `Api<EphemeralAllocation>` slot has a single production
    // callsite today and rides the peer substrate primitive so a
    // future emitter of it reaches for the primitive rather than
    // restating the incantation inline — matching the composition
    // discipline every reconciler-context primitive on the workspace
    // already follows.
    let alloc_api = ctx.allocation_api(&ns);
    let pool_api = ctx.pool_api(&ns);
    let process_api = ctx.process_api(&ns);

    // 1. Gather candidate pools in this namespace.
    // Wire-verb dispatch routes through the ONE substrate primitive
    // `tatara_process::list::default` — pre-lift this was a hand-
    // authored `.list(&ListParams::default())` 2-link chain, one of
    // FOUR workspace-wide restatements past the ★★ PRIME-DIRECTIVE
    // ≥ 2 duplication threshold (peers at the claim-arbiter walk in
    // `tatara-reconciler::table_controller`, the SIGTERM-cascade
    // fan-out in `tatara-reconciler::phase_machine`, and the pool
    // controller's owned-members walk in `controller_pool`).
    // Post-lift the four consumers share ONE substrate owner; the
    // namespace-wide EphemeralPool walk here inherits any future
    // paginated `limit` / reconciler-budget `timeout` / resource-
    // version-continuation normalization mechanically.
    let pools = tatara_process::list::default(&pool_api)
        .await
        .map_err(|e| anyhow!("list Pools in {ns}: {e}"))?
        .items;

    // 2. Build a lookup of pool name → members (sourced from each Pool's status).
    //
    // The `HashMap<String, _>` key seed rides through the substrate
    // primitive `EphemeralPool::owned_name_or_empty` — pre-lift this
    // was a hand-authored `.metadata.name.clone().unwrap_or_default()`
    // chain, one of TWO workspace-wide restatements past the ★★ PRIME-
    // DIRECTIVE ≥ 2 duplication threshold (peer at
    // `allocation_decide::AllocationConvergenceCtx::observe`'s
    // `AllocationRef::name` seed). Post-lift the two consumers share
    // ONE substrate owner; the produced key still flows into the same
    // `HashMap<String, _>` map slot unchanged.
    let pool_members: std::collections::HashMap<String, Vec<PoolMember>> = pools
        .iter()
        .map(|p| {
            let key = p.owned_name_or_empty();
            let members = p
                .status
                .as_ref()
                .map(|s| s.members.clone())
                .unwrap_or_default();
            (key, members)
        })
        .collect();

    // 3. Decide.
    //
    // The pool → members lookup key rides through the substrate
    // primitive `EphemeralPool::name_or_empty` — pre-lift this was a
    // hand-authored `.metadata.name.as_deref().unwrap_or("")` chain,
    // one of TWO workspace-wide restatements past the ★★ PRIME-
    // DIRECTIVE ≥ 2 duplication threshold (peer at
    // `router::best_match` tie-break comparator). Post-lift the two
    // consumers share ONE substrate owner; the `HashMap<String, _>::
    // get(&str)` lookup still consumes the borrow directly, so this
    // reroute allocates nothing new.
    let decision = decide_allocation_reconcile(
        &alloc,
        &pools,
        |p| {
            pool_members
                .get(p.name_or_empty())
                .map(Vec::as_slice)
                .unwrap_or(&[])
        },
        Utc::now(),
    );

    info!(
        namespace = %ns,
        allocation = %name,
        decision = ?decision,
        "allocation reconcile"
    );

    // 4. Apply.
    match decision {
        AllocationDecision::NoOp | AllocationDecision::HeartbeatBound => {}
        AllocationDecision::NoMatchingPool => {
            // The `AllocationDecision::NoMatchingPool` phase-transition
            // status seed rides through the substrate composer
            // [`tatara_process::allocation::AllocationStatus::transition`]
            // — pre-lift this was a hand-authored 4-line `json!({
            // "status": { "phase": ..., "phaseSince": Utc::now(),
            // "message": ..., } })` incantation, one of FOUR
            // workspace-wide restatements past the ★★ PRIME-DIRECTIVE
            // ≥ 2 duplication threshold in this module (peers at the
            // Wait / Bind / Release arms below). Post-lift the four
            // consumers share ONE substrate owner so a rename of the
            // typed [`AllocationStatus`] field naming (a `phaseSince`
            // → `phase_since` serde surface change, a structured-
            // envelope promotion of `message`) lands at ONE derive
            // and every emit site inherits the upgrade mechanically.
            let _ = tatara_process::patch::merge_status(
                &alloc_api,
                &name,
                &AllocationStatus::transition(
                    AllocationPhase::NoMatchingPool,
                    "no Pool selector matched this Requestor",
                    Utc::now(),
                ),
            )
            .await;
        }
        AllocationDecision::Wait { pool } => {
            // Peer to the NoMatchingPool arm above: the same substrate
            // composer stamps the `phase + phase_since + message`
            // invariant triplet; the branch-specific `bound_pool` slot
            // rides in via struct-update syntax onto the seed so a
            // future addition to the composer's stamped triplet (a
            // `by:` transition-source annotation, a
            // `transitionCount:` diagnostic counter) reaches this
            // Wait/Queued site through the substrate rather than by
            // hand-edit.
            let _ = tatara_process::patch::merge_status(
                &alloc_api,
                &name,
                &AllocationStatus {
                    bound_pool: Some(pool),
                    ..AllocationStatus::transition(
                        AllocationPhase::Queued,
                        "pool matched; no Free member available",
                        Utc::now(),
                    )
                },
            )
            .await;
        }
        AllocationDecision::Bind {
            pool,
            member_process_name,
        } => {
            // Flip the Process's lifetime to Ephemeral with the
            // allocation's TTL.
            let ttl = alloc.spec.ttl.clone().unwrap_or_else(|| {
                // Pool-lookup by `AllocationRef.name` rides through the
                // substrate primitive `EphemeralPool::has_name` — pre-
                // lift this was a hand-authored `.metadata.name.as_deref
                // () == Some(pool.name.as_str())` chain, one of TWO
                // workspace-wide restatements past the ★★ PRIME-
                // DIRECTIVE ≥ 2 duplication threshold (sibling site at
                // `allocation_decide::resolve_pool`'s explicit-`pool_ref`
                // half). The primitive keeps the `None`-preserving
                // discipline so a namespace-absent pool with an empty
                // `pool.name` returns `false`, not a spurious match at
                // the TTL-inheritance fallback that would silently
                // clone the wrong pool's `spec.template.ttl` into the
                // newly-bound member Process's lifetime overlay.
                pools
                    .iter()
                    .find(|p| p.has_name(&pool.name))
                    .map(|p| p.spec.template.ttl.clone())
                    .unwrap_or_else(|| "1h".into())
            });
            // Routes through the ONE substrate composer
            // [`tatara_process::lifetime::Lifetime::ephemeral`] —
            // pre-lift this was one of ELEVEN+ hand-authored
            // `Lifetime { ephemeral: Some(<e>), permanent: None }`
            // sites past the ★★ PRIME-DIRECTIVE ≥ 2 threshold. See
            // the composer's doc-comment for the full migration
            // rationale.
            let lifetime = Lifetime::ephemeral(EphemeralLifetime {
                ttl: ttl.clone(),
                teardown_policy: TeardownPolicy::Always,
                max_concurrent: 0,
                // Allocation-patch path doesn't add new exports;
                // any :exports on the underlying pool template
                // are already on spec.lifetime.ephemeral when
                // the pool reconciler materialized the Process.
                exports: vec![],
            });
            // Annotation keys ride through the substrate constants
            // `tatara_process::annotations::{REQUESTOR, ALLOCATION,
            // REQUESTOR_KIND}` — pre-lift each of the three keys was a
            // bare `"tatara.pleme.io/…"` string literal at this write
            // site AND at multiple test-side reader sites in
            // `tatara-process/src/lib.rs`
            // (`Annotated::annotation(&a, "tatara.pleme.io/requestor-
            // kind")` etc.), one of the six workspace-wide restatements
            // of the `REQUESTOR_KIND` wire string alone past the ★★
            // PRIME-DIRECTIVE ≥ 2 duplication threshold. Post-lift the
            // writer and every future reader route through the ONE
            // substrate owner; a rename of any key (a `tatara.pleme.io/
            // v2/requestor` migration, an alias table for cross-cluster
            // requestor identity, a per-namespace override) lands at
            // ONE `pub const` in the substrate and every downstream
            // consumer inherits the upgrade mechanically.
            //
            // REQUESTOR value composition rides through the substrate
            // primitive `tatara_process::qualified_process_ref` — pre-
            // lift this was a hand-authored `format!("{}/{}", ns, name)`
            // chain, the LAST live-crate restatement of the workspace-
            // wide `<ns>/<name>` byte-shape past the ★★ PRIME-DIRECTIVE
            // ≥ 2 duplication threshold. Every OTHER consumer of the
            // shape (the SSA re-injection's `ownership_annotations`
            // seed in `tatara-reconciler::ssapply`, the boundary-
            // evaluator input in the same module, the export-worker's
            // `run-id` fallback, the ClaimRecord `holder` slot in
            // `tatara-process::table`) already routed through the
            // primitive; this Bind arm was the sole hand-authored
            // corner left. Post-lift every `<ns>/<name>` join in the
            // live workspace lands at ONE substrate owner — a future
            // normalization of the reference shape (a case-fold, a
            // cross-cluster `<cluster>/<ns>/<name>` variant, a
            // `<ns>/<name>@<gen>` multi-generation form for
            // attestation grepping, a unicode-safe collation) reaches
            // this call site through ONE substrate function rather
            // than by hand-edit. The `&ns` / `&name` borrow avoids the
            // pre-lift `.clone()`s implicit in the `format!(...)`
            // positional-argument expansion.
            let proc_patch = json!({
                "spec": { "lifetime": lifetime },
                "metadata": {
                    "annotations": {
                        annotations::REQUESTOR:
                            tatara_process::qualified_process_ref(&ns, &name),
                        annotations::ALLOCATION:
                            name.clone(),
                        annotations::REQUESTOR_KIND:
                            alloc.spec.requestor.kind.clone(),
                    }
                }
            });
            // Named-merge compose+dispatch rides through the ONE
            // substrate primitive `tatara_process::patch::merge_as` —
            // pre-lift this was a hand-authored 3-link
            // `process_api.patch(&member_process_name, &apply_patch_params
            // (&ctx.config.field_manager), &Patch::Merge(&proc_patch))`
            // chain, one of TWO workspace-wide restatements past the ★★
            // PRIME-DIRECTIVE ≥ 2 duplication threshold (peer at the
            // Release arm below feeding the same
            // `ctx.config.field_manager` through the same
            // `apply_patch_params + Patch::Merge` two-link chain). Post-
            // lift both consumers share ONE substrate owner for the
            // (Patch::Merge × apply_patch_params × primary-resource)
            // corner of the wire-side patch-family matrix, closing its
            // last hand-authored corner (siblings: `merge` on the
            // default-params corner, `apply` on the SSA-Apply corner,
            // `merge_status` on the /status subresource corner). A
            // future normalization of the named-merge posture (an added
            // `dry_run` mode, a `field_validation` default, an
            // injectable retry policy for the transient-conflict class
            // this arm surfaces on race with a sibling pool controller,
            // a `resourceVersion` precondition slot for
            // generation-fenced binds) lands at ONE substrate site.
            if let Err(e) = tatara_process::patch::merge_as(
                &process_api,
                &member_process_name,
                &ctx.config.field_manager,
                &proc_patch,
            )
            .await
            {
                warn!(error = %e, "bind failed; will retry");
                return Ok(tatara_process::requeue::after_secs(5));
            }

            // Status patch on Allocation.
            let now = Utc::now();
            let ttl_duration =
                humantime::parse_duration(&ttl).unwrap_or(std::time::Duration::from_secs(3600));
            let expires_at = now
                + chrono::Duration::from_std(ttl_duration)
                    .unwrap_or_else(|_| chrono::Duration::hours(1));
            // The compound `bound_pool + assigned_process` pair rides
            // through the substrate composer
            // [`tatara_process::allocation::AllocationStatus::bound_transition`]
            // — pre-lift this pair rode struct-update syntax onto
            // [`AllocationStatus::transition`] at TWO workspace-wide
            // sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
            // threshold in this module (peer at the Release arm
            // below). Post-lift both consumers share ONE substrate
            // owner; the branch-specific `allocated_at + expires_at`
            // stamp rides in via struct-update onto the compound
            // composer's seed so a future symmetry gate on the
            // bound-set axis (assigned_process's namespace matching
            // the bound_pool's namespace, a canonicalization pass, a
            // backwards-compatibility rename) lands at ONE substrate
            // site rather than at each callsite here. The compound
            // composer accepts the shared local `now` binding so the
            // same wall-clock read reaches BOTH `phase_since` and
            // `allocated_at` — pre-lift the two slots stamped from
            // the SAME `now`, so the composer's clock-injectability
            // preserves that shape exactly.
            let _ = tatara_process::patch::merge_status(
                &alloc_api,
                &name,
                &AllocationStatus {
                    allocated_at: Some(now),
                    expires_at: Some(expires_at),
                    ..AllocationStatus::bound_transition(
                        AllocationPhase::Bound,
                        "bound to pool member",
                        now,
                        pool,
                        AllocationRef::new(member_process_name, ns.clone()),
                    )
                },
            )
            .await;
        }
        AllocationDecision::Release {
            member_process_name,
            pool,
        } => {
            // Trigger return path on the Process — flip back to
            // Permanent OR delete entirely depending on pool's
            // ReturnPolicy (the Pool reconciler will pick this up next
            // tick).
            // Peer to the Bind arm above: SSA-side wire-posture rides
            // through the ONE substrate primitive
            // `tatara_process::patch::apply_patch_params`, the return-
            // trigger annotation apply feeding the same
            // `ctx.config.field_manager` as its sibling. Wire-body
            // composition rides the substrate primitive
            // `tatara_process::patch::annotation_body` — pre-lift this
            // was a hand-authored `json!({"metadata": {"annotations":
            // {<return-trigger-key>: "true"}}})` chain inside the
            // `Patch::Merge` slot, one of THREE workspace-wide
            // restatements of the single-annotation merge-body shape
            // past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold
            // (peers at `tatara-reconciler::signals::ingest`'s Null-
            // value SIGNAL strip + `tatara-reconciler::phase_machine::
            // transition_to_releasing`'s RELEASED_FROM stamp). Post-
            // lift the single-annotation merge-body posture lives at
            // ONE substrate owner.
            let trigger_body =
                tatara_process::patch::annotation_body("tatara.pleme.io/return-trigger", "true");
            // Peer to the Bind arm above: named-merge compose+dispatch
            // rides through the ONE substrate primitive
            // `tatara_process::patch::merge_as`, closing the last hand-
            // authored corner of the (Patch::Merge × apply_patch_params
            // × primary-resource) wire-posture matrix. Post-lift both
            // pool-reconciler callsites share ONE substrate owner; see
            // the Bind arm's re-route above for the full peer inventory.
            let _ = tatara_process::patch::merge_as(
                &process_api,
                &member_process_name,
                &ctx.config.field_manager,
                &trigger_body,
            )
            .await;
            // Peer to the Bind arm above: the compound `bound_pool +
            // assigned_process` pair rides through the substrate
            // composer
            // [`tatara_process::allocation::AllocationStatus::bound_transition`]
            // — post-lift both consumers share ONE substrate owner
            // for the pair AND for the composed base `phase +
            // phase_since + message` triplet, so this Release arm
            // has no addenda past the compound composer's seed and
            // patches the composed [`AllocationStatus`] verbatim.
            let _ = tatara_process::patch::merge_status(
                &alloc_api,
                &name,
                &AllocationStatus::bound_transition(
                    AllocationPhase::Released,
                    "released; pool reconciler will return the member",
                    Utc::now(),
                    pool,
                    AllocationRef::new(member_process_name, ns.clone()),
                ),
            )
            .await;
        }
    }

    let _ = ALLOC_FINALIZER;
    Ok(tatara_process::requeue::after_secs(
        ctx.config.heartbeat_seconds,
    ))
}

pub fn error_policy(
    _alloc: Arc<EphemeralAllocation>,
    err: &ReconcilerError,
    _ctx: Arc<PoolContext>,
) -> Action {
    warn!(error = ?err, "allocation reconcile failed");
    tatara_process::requeue::after_secs(15)
}

#[cfg(test)]
mod tests {
    //! Substrate-discipline pins for the Bind arm's REQUESTOR
    //! annotation seed — post-lift the value rides through the ONE
    //! substrate composer `tatara_process::qualified_process_ref`
    //! (which owns the `<ns>/<name>` byte-shape workspace-wide) rather
    //! than a hand-authored `format!("{}/{}", ns, name)` chain. These
    //! pins bind the composer's output byte-identically to the pre-
    //! lift spelling AND to the actual annotation value the Bind arm
    //! embeds in the merge-patch body, so a regression that reverted
    //! the call site to a hand-authored `format!` (with a subtly
    //! wrong separator, reversed axis order, or the two-arg positional
    //! `format!("{name}/{ns}")` typo) surfaces here rather than as
    //! silent drift in the requestor grep discipline that the
    //! reconciler + audit trail + pool-decide observer all key on.
    //!
    //! The pins re-materialize the ONE annotation slot the reconciler
    //! composes at the Bind arm; they do NOT re-drive the reconcile
    //! loop (which would require a running kube API + Pool fixtures).
    //! That is deliberate — the substrate primitive already carries
    //! its own bytewise pins in `tatara-process::qualified_process_ref_tests`;
    //! this module's job is to lock in the CALL-SITE discipline that
    //! the Bind arm reaches through the substrate rather than
    //! restating the shape by hand.
    use super::*;

    #[test]
    fn requestor_annotation_composes_through_qualified_process_ref() {
        // Bytewise pin: the substrate composer's output must be
        // byte-identical to the pre-lift `format!("{}/{}", ns, name)`
        // spelling for every representative (ns, name) axis-shape the
        // Bind arm sees in production (typed namespaces, empty-slot
        // corners the K8s API server itself accepts, edge-case names
        // that a hand-authored `format!` typo would silently corrupt).
        // A regression that re-authored the composer to canonicalize
        // one of these corners without updating the reference
        // `format!` spelling would surface here rather than as
        // divergent requestor greps downstream.
        for (ns, name) in [
            ("demo-ns", "req-1"),
            ("", ""),
            ("default", ""),
            ("", "orphan"),
            ("weird ns", "with/slash"),
            ("pleme-dev", "ephemeral-demo"),
        ] {
            let hand_authored = format!("{}/{}", ns, name);
            assert_eq!(
                tatara_process::qualified_process_ref(ns, name),
                hand_authored,
                "substrate `qualified_process_ref` must be byte-identical to the \
                 pre-lift `format!(\"{{}}/{{}}\", ns, name)` for (ns={ns:?}, name={name:?})",
            );
        }
    }

    #[test]
    fn requestor_annotation_slot_matches_substrate_composer_bytewise() {
        // Re-materialize the exact `metadata.annotations` slot the
        // Bind arm composes and pin the REQUESTOR value against the
        // substrate composer's output. The composed slot below MUST
        // stay in lockstep with the `proc_patch` composition inside
        // `reconcile_inner`'s Bind arm; a regression that dropped the
        // substrate delegation from that composition (or renamed the
        // REQUESTOR const key) surfaces here rather than as silent
        // annotation drift on every allocation-bound Process.
        let ns = "demo-ns";
        let name = "req-1";
        let kind = "manual";

        let composed = json!({
            "metadata": {
                "annotations": {
                    annotations::REQUESTOR:
                        tatara_process::qualified_process_ref(ns, name),
                    annotations::ALLOCATION: name,
                    annotations::REQUESTOR_KIND: kind,
                }
            }
        });

        assert_eq!(
            composed["metadata"]["annotations"][annotations::REQUESTOR],
            serde_json::Value::String("demo-ns/req-1".into()),
            "REQUESTOR annotation slot must serialise to the canonical `<ns>/<name>` shape",
        );
        assert_eq!(
            composed["metadata"]["annotations"][annotations::ALLOCATION],
            serde_json::Value::String(name.into()),
        );
        assert_eq!(
            composed["metadata"]["annotations"][annotations::REQUESTOR_KIND],
            serde_json::Value::String(kind.into()),
        );
    }

    #[test]
    fn requestor_annotation_axis_order_survives_composer_swap() {
        // Regression pin against the pre-lift positional-`format!`
        // typo class: `format!("{name}/{ns}")` (axis swap) or
        // `format!("{ns}-{name}")` (wrong separator) would produce a
        // value the reconciler's grep discipline silently mis-keys.
        // The substrate composer's 2-arg (ns, name) signature encodes
        // the axis order at the type level — a swap here surfaces as
        // a divergent value, not a divergent shape.
        let via_substrate = tatara_process::qualified_process_ref("ns-first", "name-second");
        assert_eq!(via_substrate, "ns-first/name-second");
        assert_ne!(via_substrate, "name-second/ns-first");
        assert_ne!(via_substrate, "ns-first-name-second");
    }
}
