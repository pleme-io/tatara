//! Allocation controller — applies `AllocationDecision`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::Utc;
use kube::api::{ListParams, Patch};
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
    let pools = pool_api
        .list(&ListParams::default())
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
            let lifetime = Lifetime {
                permanent: None,
                ephemeral: Some(EphemeralLifetime {
                    ttl: ttl.clone(),
                    teardown_policy: TeardownPolicy::Always,
                    max_concurrent: 0,
                    // Allocation-patch path doesn't add new exports;
                    // any :exports on the underlying pool template
                    // are already on spec.lifetime.ephemeral when
                    // the pool reconciler materialized the Process.
                    exports: vec![],
                }),
            };
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
            let proc_patch = json!({
                "spec": { "lifetime": lifetime },
                "metadata": {
                    "annotations": {
                        annotations::REQUESTOR:
                            format!("{}/{}", ns, name),
                        annotations::ALLOCATION:
                            name.clone(),
                        annotations::REQUESTOR_KIND:
                            alloc.spec.requestor.kind.clone(),
                    }
                }
            });
            // SSA-side wire-posture rides through the substrate
            // primitive `tatara_process::patch::apply_patch_params` —
            // pre-lift this was a hand-authored 2-link
            // `PatchParams::apply(&ctx.config.field_manager).force()`
            // chain, one of THREE workspace-wide sites past the ★★
            // PRIME-DIRECTIVE ≥ 2 duplication threshold (peer at the
            // Release arm below feeding the same
            // `ctx.config.field_manager`, sibling at
            // `tatara-export-worker::main::write_receipt` feeding a
            // `"tatara-export-worker"` literal, and the
            // reconciler-crate-local wrapper
            // `tatara_reconciler::ssapply::apply_patch_params` feeding
            // the `FIELD_MANAGER` const). Post-lift every SSA writer
            // across three consumer crates shares ONE substrate owner
            // for the `PatchParams::apply(<mgr>).force()` shape; a
            // future normalization of the SSA-side posture (an added
            // `dry_run` mode, a `field_validation` default, an
            // injectable retry policy) lands at ONE substrate site.
            if let Err(e) = process_api
                .patch(
                    &member_process_name,
                    &tatara_process::patch::apply_patch_params(&ctx.config.field_manager),
                    &Patch::Merge(&proc_patch),
                )
                .await
            {
                warn!(error = %e, "bind failed; will retry");
                return Ok(Action::requeue(Duration::from_secs(5)));
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
            // `ctx.config.field_manager` as its sibling.
            let _ = process_api
                .patch(
                    &member_process_name,
                    &tatara_process::patch::apply_patch_params(&ctx.config.field_manager),
                    &Patch::Merge(&json!({
                        "metadata": { "annotations": {
                            "tatara.pleme.io/return-trigger": "true",
                        }}
                    })),
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
    Ok(Action::requeue(Duration::from_secs(
        ctx.config.heartbeat_seconds,
    )))
}

pub fn error_policy(
    _alloc: Arc<EphemeralAllocation>,
    err: &ReconcilerError,
    _ctx: Arc<PoolContext>,
) -> Action {
    warn!(error = ?err, "allocation reconcile failed");
    Action::requeue(Duration::from_secs(15))
}
