//! Allocation controller — applies `AllocationDecision`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::Utc;
use kube::api::{Api, ListParams, Patch, PatchParams};
use kube::runtime::controller::Action;
use serde_json::json;
use tracing::{info, warn};

use tatara_process::allocation::{AllocationPhase, EphemeralAllocation};
use tatara_process::lifetime::{EphemeralLifetime, Lifetime, TeardownPolicy};
use tatara_process::pool::{AllocationRef, EphemeralPool, PoolMember};
use tatara_process::prelude::Process;

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
    let ns = alloc
        .metadata
        .namespace
        .clone()
        .ok_or_else(|| anyhow!("Allocation has no metadata.namespace"))?;
    let name = alloc
        .metadata
        .name
        .clone()
        .ok_or_else(|| anyhow!("Allocation has no metadata.name"))?;

    let alloc_api: Api<EphemeralAllocation> = Api::namespaced(ctx.kube.clone(), &ns);
    let pool_api: Api<EphemeralPool> = Api::namespaced(ctx.kube.clone(), &ns);
    let process_api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);

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
            let body = json!({
                "status": {
                    "phase": AllocationPhase::NoMatchingPool,
                    "phaseSince": Utc::now(),
                    "message": "no Pool selector matched this Requestor",
                }
            });
            let _ = alloc_api
                .patch_status(&name, &PatchParams::default(), &Patch::Merge(&body))
                .await;
        }
        AllocationDecision::Wait { pool } => {
            let body = json!({
                "status": {
                    "phase": AllocationPhase::Queued,
                    "phaseSince": Utc::now(),
                    "boundPool": pool,
                    "message": "pool matched; no Free member available",
                }
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
                pools
                    .iter()
                    .find(|p| p.metadata.name.as_deref() == Some(pool.name.as_str()))
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
            let proc_patch = json!({
                "spec": { "lifetime": lifetime },
                "metadata": {
                    "annotations": {
                        "tatara.pleme.io/requestor":
                            format!("{}/{}", ns, name),
                        "tatara.pleme.io/allocation":
                            name.clone(),
                        "tatara.pleme.io/requestor-kind":
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
            let body = json!({
                "status": {
                    "phase": AllocationPhase::Bound,
                    "phaseSince": now,
                    "boundPool": pool,
                    "assignedProcess": AllocationRef::new(member_process_name, ns.clone()),
                    "allocatedAt": now,
                    "expiresAt": expires_at,
                    "message": "bound to pool member",
                }
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
            let body = json!({
                "status": {
                    "phase": AllocationPhase::Released,
                    "phaseSince": Utc::now(),
                    "boundPool": pool,
                    "assignedProcess": AllocationRef::new(member_process_name, ns.clone()),
                    "message": "released; pool reconciler will return the member",
                }
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
