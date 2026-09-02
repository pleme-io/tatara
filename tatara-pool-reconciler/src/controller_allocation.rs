//! Allocation controller — applies `AllocationDecision`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::Utc;
use kube::api::{ListParams, Patch, PatchParams};
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
            let body = json!({
                "status": AllocationStatus::transition(
                    AllocationPhase::NoMatchingPool,
                    "no Pool selector matched this Requestor",
                    Utc::now(),
                ),
            });
            let _ = alloc_api
                .patch_status(&name, &PatchParams::default(), &Patch::Merge(&body))
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
            let body = json!({
                "status": AllocationStatus {
                    bound_pool: Some(pool),
                    ..AllocationStatus::transition(
                        AllocationPhase::Queued,
                        "pool matched; no Free member available",
                        Utc::now(),
                    )
                },
            });
            let _ = alloc_api
                .patch_status(&name, &PatchParams::default(), &Patch::Merge(&body))
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
            if let Err(e) = process_api
                .patch(
                    &member_process_name,
                    &PatchParams::apply(&ctx.config.field_manager).force(),
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
            // The `assignedProcess` status slot rides through the
            // substrate constructor `AllocationRef::new` — pre-lift
            // this was a hand-authored `AllocationRef { name, namespace }`
            // struct-literal, one of FOUR workspace-wide restatements
            // past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold
            // (peers at the Release path below, at `allocation_decide::
            // AllocationConvergenceCtx::observe`'s pool_ref seed, and
            // at `tatara-github-watcher::allocation_factory`'s
            // pool_ref seed). Post-lift the four consumers share ONE
            // substrate owner.
            // Peer to the NoMatchingPool / Wait arms above: the same
            // substrate composer owns the invariant `phase +
            // phase_since + message` triplet; this arm's four extra
            // slots (`bound_pool` + `assigned_process` +
            // `allocated_at` + `expires_at`) ride in via struct-update
            // onto the seed. The composer accepts the shared local
            // `now` binding so the same wall-clock read reaches BOTH
            // `phase_since` and `allocated_at` — pre-lift the two
            // slots stamped from the SAME `now`, so the composer's
            // clock-injectability preserves that shape exactly.
            let body = json!({
                "status": AllocationStatus {
                    bound_pool: Some(pool),
                    assigned_process: Some(AllocationRef::new(member_process_name, ns.clone())),
                    allocated_at: Some(now),
                    expires_at: Some(expires_at),
                    ..AllocationStatus::transition(
                        AllocationPhase::Bound,
                        "bound to pool member",
                        now,
                    )
                },
            });
            let _ = alloc_api
                .patch_status(&name, &PatchParams::default(), &Patch::Merge(&body))
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
            let _ = process_api
                .patch(
                    &member_process_name,
                    &PatchParams::apply(&ctx.config.field_manager).force(),
                    &Patch::Merge(&json!({
                        "metadata": { "annotations": {
                            "tatara.pleme.io/return-trigger": "true",
                        }}
                    })),
                )
                .await;
            // The `assignedProcess` status slot rides through the
            // substrate constructor `AllocationRef::new` — sibling
            // shape to the Bind path's stamp above, both routed
            // through the ONE substrate owner so a future refactor of
            // the ref shape (added field, canonicalization, non-empty
            // gate) lands at ONE place rather than at both status-
            // patch sites here.
            // Peer to the NoMatchingPool / Wait / Bind arms above:
            // the same substrate composer owns the invariant `phase +
            // phase_since + message` triplet; the branch-specific
            // `bound_pool` + `assigned_process` slots ride in via
            // struct-update onto the seed so the four `reconcile_inner`
            // patch sites now share ONE substrate owner for the
            // three always-present slots.
            let body = json!({
                "status": AllocationStatus {
                    bound_pool: Some(pool),
                    assigned_process: Some(AllocationRef::new(member_process_name, ns.clone())),
                    ..AllocationStatus::transition(
                        AllocationPhase::Released,
                        "released; pool reconciler will return the member",
                        Utc::now(),
                    )
                },
            });
            let _ = alloc_api
                .patch_status(&name, &PatchParams::default(), &Patch::Merge(&body))
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
