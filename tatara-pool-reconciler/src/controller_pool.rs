//! Pool controller — applies `PoolDecision` to the cluster via kube-rs.
//!
//! The decision logic is pure (see `pool_decide`). This module is the
//! thin async glue: it fetches Pools + their owned Processes, calls
//! `decide_pool_reconcile`, and applies the result via kube-rs
//! create/delete + status patch.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::Utc;
use kube::api::{Api, DeleteParams, ListParams, Patch, PatchParams, PostParams};
use kube::runtime::controller::Action;
use serde_json::json;
use tracing::{info, warn};

use crate::ReconcilerError;

use tatara_process::ephemeral::EphemeralSpec;
use tatara_process::lifetime::{Lifetime, PermanentLifetime};
use tatara_process::pool::{EphemeralPool, MemberState, PoolMember, PoolPhase, PoolStatus};
use tatara_process::prelude::{NamespacedApiCoordinates, Process, ProcessSpec};

use crate::context::PoolContext;
use crate::desired::{decide_pool_convergence, ConvergenceAction, PoolMemberSnapshot};
use crate::naming::member_process_name;
use crate::pool_decide::{decide_pool_reconcile, PoolDecision};

const POOL_FINALIZER: &str = "tatara.pleme.io/pool-finalizer";
const ANNOTATION_POOL: &str = "tatara.pleme.io/pool";
const ANNOTATION_SLOT: &str = "tatara.pleme.io/pool-slot";

/// One reconcile pass over a Pool. The kube-rs `Controller` calls this.
pub async fn reconcile(
    pool: Arc<EphemeralPool>,
    ctx: Arc<PoolContext>,
) -> std::result::Result<Action, ReconcilerError> {
    reconcile_inner(pool, ctx).await.map_err(Into::into)
}

async fn reconcile_inner(pool: Arc<EphemeralPool>, ctx: Arc<PoolContext>) -> Result<Action> {
    // The (namespace, name) API-path pair rides through the substrate
    // trait `tatara_process::NamespacedApiCoordinates` (blanket-implemented
    // over every `kube::Resource<DynamicType = ()>` in the workspace) —
    // pre-lift this was a hand-authored paired 5-line `.metadata.<slot>
    // .clone().ok_or_else(|| anyhow!("Pool has no metadata.<slot>"))?`
    // chain, one of TWO workspace-wide restatements past the ★★ PRIME-
    // DIRECTIVE ≥ 2 duplication threshold (peer at
    // `controller_allocation::reconcile_inner`'s Allocation top-level
    // gate; both funneled every downstream `Api::namespaced` +
    // `Api::patch` call through the same shape). Post-lift the two
    // reconcilers share ONE substrate owner + every future CRD in a
    // peer crate inherits the extractor for free at its own
    // `reconcile_inner` dispatcher; the error prefix now spells the
    // canonical kube kind (`EphemeralPool`) rather than the pre-lift
    // short-form (`Pool`), matching `kubectl get ephemeralpools`
    // output verbatim.
    let (ns, name) = pool.owned_coordinates_required()?;

    let pool_api: Api<EphemeralPool> = Api::namespaced(ctx.kube.clone(), &ns);
    let process_api: Api<Process> = Api::namespaced(ctx.kube.clone(), &ns);

    // 1. Fetch the Processes owned by this Pool (annotation-matched).
    let all_processes = process_api
        .list(&ListParams::default())
        .await
        .map_err(|e| anyhow!("list Processes in {ns}: {e}"))?;
    let mut members: Vec<PoolMember> = Vec::new();
    let mut owned: Vec<Process> = Vec::new();
    for p in all_processes.items {
        if process_belongs_to_pool(&p, &name) {
            let state = process_to_member_state(&p);
            members.push(PoolMember {
                // Owned-form name projection rides through the ONE
                // substrate primitive `Process::owned_name_or_empty` —
                // pre-lift this was a hand-authored `.metadata.name
                // .clone().unwrap_or_default()` chain, one of TWO
                // workspace-wide restatements past the ★★ PRIME-
                // DIRECTIVE ≥ 2 duplication threshold (peer at the
                // desired-count `PoolMemberSnapshot` seed below).
                // Post-lift both consumers share ONE substrate owner;
                // a future name-canonicalization pass or per-pool
                // alias table lands at `tatara_process::prelude::
                // Process::owned_name_or_empty` and both row-builder
                // seeds inherit it mechanically.
                process_name: p.owned_name_or_empty(),
                state,
                entered_state_at: p
                    .status
                    .as_ref()
                    .and_then(|s| s.phase_since)
                    .unwrap_or_else(Utc::now),
                allocation_ref: None,
            });
            owned.push(p);
        }
    }

    // 2a. **R11 — desired-count loop gate.** When the operator
    //     declares `spec.desired > 0`, the pool seeks a stable
    //     healthy count and the legacy allocation-driven decision
    //     is bypassed. Snapshots come from observed Process
    //     phases (Running/Attested = healthy; Failed/Zombie/Reaped
    //     = failed; everything else = transient).
    if pool.spec.desired > 0 {
        // Snapshot each owned Process's (phase, created_at) pair through
        // the substrate primitive family on `Process` — pre-lift the
        // 3-line `.status.as_ref().map(|s| s.phase)` chain and the
        // 5-line `.metadata.creation_timestamp.as_ref().map(|t| t.0)
        // .unwrap_or_else(Utc::now)` chain were hand-authored here,
        // duplicating the lift already living at
        // [`tatara_process::prelude::Process::observed_phase`] (owner of
        // the copy-form status-projection primitive on the phase axis)
        // and [`tatara_process::prelude::Process::created_at`] (owner
        // of the copy-form metadata-projection primitive on the
        // creation-timestamp axis). Routing through both substrate
        // primitives means a future normalization (generation filter,
        // clock-skew guard, stale-observation gate) lands at ONE
        // substrate site and this pool-desired-count seed inherits it
        // mechanically — no per-callsite hand-edit here alongside the
        // sibling consumers in `tatara-reconciler` /
        // `tatara-process::lifetime_clock`.
        let snapshots: Vec<PoolMemberSnapshot> = owned
            .iter()
            .map(|p| PoolMemberSnapshot {
                // Owned-form name projection rides through the ONE
                // substrate primitive `Process::owned_name_or_empty` —
                // pre-lift this was a hand-authored `.metadata.name
                // .clone().unwrap_or_default()` chain, sibling to the
                // pool-member seed above; both routed through the
                // same substrate owner so any future name-canonicalization
                // pass lands once.
                process_name: p.owned_name_or_empty(),
                // Phase snapshot rides through the ONE substrate
                // primitive `Process::observed_phase_or_pending` —
                // pre-lift this was a hand-authored `.observed_phase()
                // .unwrap_or(ProcessPhase::Pending)` two-link chain,
                // one of FOUR workspace-wide restatements past the
                // ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold
                // (peers at `tatara-reconciler::controller::reconcile`
                // / `::boundary::evaluate_process_phase` /
                // `::table_controller::stable_name_group_key`).
                // Post-lift the four consumers share ONE substrate
                // owner and this desired-count seed inherits any
                // future generation-filter or staleness-gate
                // normalization mechanically.
                phase: p.observed_phase_or_pending(),
                created_at: p.created_at().unwrap_or_else(Utc::now),
            })
            .collect();
        let actions = decide_pool_convergence(&pool, &snapshots, Utc::now());
        info!(
            namespace = %ns,
            pool = %name,
            desired = pool.spec.desired,
            actions = actions.len(),
            "pool desired-count loop"
        );
        apply_convergence_actions(&pool, &process_api, &ns, &name, &members, actions).await?;

        // Update status from the same observations + skip the
        // legacy decision step.
        let phase = pool_phase_from_members(&pool, &members);
        let status_patch = json!({
            "status": PoolStatus {
                phase,
                phase_since: Some(Utc::now()),
                ready_count: count_state(&members, MemberState::Free),
                allocated_count: count_state(&members, MemberState::Allocated),
                spawning_count: count_state(&members, MemberState::Spawning),
                returning_count: count_state(&members, MemberState::Returning),
                members: members.clone(),
                message: None,
                conditions: vec![],
            },
        });
        let _ = pool_api
            .patch_status(&name, &PatchParams::default(), &Patch::Merge(&status_patch))
            .await;
        return Ok(Action::requeue(Duration::from_secs(
            ctx.config.heartbeat_seconds,
        )));
    }

    // 2b. Legacy allocation-driven decision (desired == 0 path).
    let decision = decide_pool_reconcile(&pool, &members, Utc::now());

    info!(
        namespace = %ns,
        pool = %name,
        members = members.len(),
        decision = ?decision,
        "pool reconcile"
    );

    // 3. Apply the decision.
    match decision {
        PoolDecision::NoOp => {}
        PoolDecision::Spawn { count } => {
            let pool_uid = pool.metadata.uid.clone().unwrap_or_else(|| name.clone());
            let occupied_names: std::collections::HashSet<_> =
                members.iter().map(|m| m.process_name.clone()).collect();
            let mut spawned = 0u32;
            for slot in 0..u32::MAX {
                if spawned >= count {
                    break;
                }
                let proc_name = member_process_name(&name, &pool_uid, slot);
                if occupied_names.contains(&proc_name) {
                    continue;
                }
                let proc = build_member_process(&pool, &proc_name, slot, &name)?;
                match process_api.create(&PostParams::default(), &proc).await {
                    Ok(_) => {
                        info!(namespace = %ns, pool = %name, process = %proc_name, "spawned member");
                        spawned += 1;
                    }
                    Err(kube::Error::Api(e)) if e.code == 409 => {
                        // race — someone else created this Process; treat as ok.
                        spawned += 1;
                    }
                    Err(e) => {
                        warn!(error = %e, "spawn failed; will retry");
                        break;
                    }
                }
            }
        }
        PoolDecision::ReapExcess { count } => {
            // Reap Free members first (never Allocated).
            let to_reap: Vec<_> = members
                .iter()
                .filter(|m| matches!(m.state, MemberState::Free))
                .take(count as usize)
                .collect();
            for m in to_reap {
                let _ = process_api
                    .delete(&m.process_name, &DeleteParams::default())
                    .await;
                info!(namespace = %ns, pool = %name, process = %m.process_name, "reaped excess");
            }
        }
        PoolDecision::ReplaceMembers { process_names } => {
            for n in process_names {
                let _ = process_api.delete(&n, &DeleteParams::default()).await;
                info!(namespace = %ns, pool = %name, process = %n, "replaced (deleted; respawn next tick)");
            }
        }
        PoolDecision::Drain => {
            for m in &members {
                let _ = process_api
                    .delete(&m.process_name, &DeleteParams::default())
                    .await;
            }
        }
    }

    // 4. Update Pool status.
    let phase = pool_phase_from_members(&pool, &members);
    let status_patch = json!({
        "status": PoolStatus {
            phase,
            phase_since: Some(Utc::now()),
            ready_count: count_state(&members, MemberState::Free),
            allocated_count: count_state(&members, MemberState::Allocated),
            spawning_count: count_state(&members, MemberState::Spawning),
            returning_count: count_state(&members, MemberState::Returning),
            members: members.clone(),
            message: None,
            conditions: vec![],
        },
    });
    let _ = pool_api
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(&status_patch))
        .await;

    Ok(Action::requeue(Duration::from_secs(
        ctx.config.heartbeat_seconds,
    )))
}

pub fn error_policy(
    _pool: Arc<EphemeralPool>,
    err: &ReconcilerError,
    _ctx: Arc<PoolContext>,
) -> Action {
    warn!(error = ?err, "pool reconcile failed");
    Action::requeue(Duration::from_secs(15))
}

fn count_state(members: &[PoolMember], target: MemberState) -> u32 {
    members.iter().filter(|m| m.state == target).count() as u32
}

fn pool_phase_from_members(pool: &EphemeralPool, members: &[PoolMember]) -> PoolPhase {
    if pool.is_being_deleted() {
        return PoolPhase::Draining;
    }
    // The (free + spawning) supply calc lives at one site: the
    // `MemberState::counts_toward_supply` closed-set predicate. A
    // future variant that should also count toward supply (e.g. a
    // "Warming" state between Spawning and Free) lands at one
    // predicate arm in `tatara-process::pool::MemberState`; this site
    // inherits the new bucketing automatically. The
    // `member_state_failed_implies_no_supply` disjointness contract
    // in tatara-process pins that a Failed member can never inflate
    // this count.
    let supply = members
        .iter()
        .filter(|m| m.state.counts_toward_supply())
        .count() as u32;
    let want = pool.spec.desired_size;
    if members.is_empty() {
        return PoolPhase::Initializing;
    }
    if pool.spec.min_size > 0 && supply < pool.spec.min_size {
        return PoolPhase::Degraded;
    }
    if supply < want {
        return PoolPhase::ScalingUp;
    }
    if supply > want {
        return PoolPhase::ScalingDown;
    }
    PoolPhase::Steady
}

/// **R11 desired-count action applier.** Translates a
/// `Vec<ConvergenceAction>` from `decide_pool_convergence` into
/// kube-rs create/delete calls. Pure separation: the decision is
/// upstream, this is the I/O glue.
async fn apply_convergence_actions(
    pool: &EphemeralPool,
    process_api: &Api<Process>,
    ns: &str,
    name: &str,
    members: &[PoolMember],
    actions: Vec<ConvergenceAction>,
) -> Result<()> {
    if actions.is_empty() {
        return Ok(());
    }

    let pool_uid = pool
        .metadata
        .uid
        .clone()
        .unwrap_or_else(|| name.to_string());
    let occupied_names: std::collections::HashSet<_> =
        members.iter().map(|m| m.process_name.clone()).collect();

    // Track the next free slot index per call so multiple
    // CreateMember actions in one tick spawn distinct names.
    let mut next_slot: u32 = 0;

    for action in actions {
        match action {
            ConvergenceAction::CreateMember => {
                // Find the next available slot name not currently in
                // use by another member.
                let proc_name = loop {
                    let candidate = member_process_name(name, &pool_uid, next_slot);
                    next_slot += 1;
                    if !occupied_names.contains(&candidate) {
                        break candidate;
                    }
                    if next_slot > u32::MAX / 2 {
                        return Err(anyhow!("pool {name} exhausted slot space"));
                    }
                };
                let proc = build_member_process(pool, &proc_name, next_slot - 1, name)?;
                match process_api.create(&PostParams::default(), &proc).await {
                    Ok(_) => {
                        info!(namespace = %ns, pool = %name, process = %proc_name, "desired-loop spawned");
                    }
                    Err(kube::Error::Api(e)) if e.code == 409 => {
                        // already exists — race with another reconcile; OK.
                    }
                    Err(e) => {
                        warn!(error = %e, pool = %name, "spawn failed; will retry next tick");
                        return Ok(());
                    }
                }
            }
            ConvergenceAction::SignalSigterm { process_name } => {
                // Delete the Process — the reconciler's finalizer +
                // SIGTERM cascade unfolds via the standard exit path.
                let _ = process_api
                    .delete(&process_name, &DeleteParams::default())
                    .await;
                info!(
                    namespace = %ns,
                    pool = %name,
                    process = %process_name,
                    "desired-loop scale-down (SIGTERM oldest excess)"
                );
            }
            ConvergenceAction::ReapFailed { process_name } => {
                let _ = process_api
                    .delete(&process_name, &DeleteParams::default())
                    .await;
                info!(
                    namespace = %ns,
                    pool = %name,
                    process = %process_name,
                    "desired-loop reaped failed member"
                );
            }
            ConvergenceAction::Pause { reason } => {
                // Pool-wide pause is operator-visible via the
                // PoolStatus message; we don't apply further actions
                // this tick. Operator unpauses by acknowledging the
                // failed member (HoldFailed + manual reap) OR by
                // changing replacement_policy.
                warn!(
                    pool = %name,
                    reason = %reason,
                    "desired-loop paused — operator action required"
                );
                return Ok(());
            }
        }
    }
    Ok(())
}

fn process_belongs_to_pool(p: &Process, pool_name: &str) -> bool {
    // Annotation lookup via the substrate primitive — pre-lift this
    // was a hand-authored 3-line `.metadata.annotations.as_ref()
    // .and_then(|a| a.get(ANNOTATION_POOL)).map(String::as_str)`
    // chain, one of THREE workspace-wide restatements past the ★★
    // PRIME-DIRECTIVE ≥ 2 duplication threshold (peers at
    // `tatara-reconciler::signals::ingest` and
    // `tatara-reconciler::phase_machine::released_from_annotation`).
    // Post-lift the three lookups route through ONE substrate owner
    // [`tatara_process::prelude::Process::annotation`]; this callsite
    // composes its `== Some(pool_name)` equality tail at its own site,
    // preserving the pre-lift membership-gate semantics byte-for-byte.
    p.annotation(ANNOTATION_POOL) == Some(pool_name)
}

fn process_to_member_state(p: &Process) -> MemberState {
    use tatara_process::phase::ProcessPhase;
    // Route the phase-observed lookup through the substrate primitive
    // [`tatara_process::prelude::Process::observed_phase`] — pre-lift the
    // 3-line `.status.as_ref().map(|s| s.phase)` chain was hand-authored
    // here, duplicating the lift already living on `Process`. A future
    // normalization (generation filter, stale-observation gate) lands at
    // ONE substrate site and this member-state mapper inherits it
    // mechanically alongside every peer consumer in `tatara-reconciler`.
    match p.observed_phase() {
        Some(ProcessPhase::Attested) => {
            // Bound to an allocation iff lifetime is Ephemeral.
            if p.spec.lifetime.is_ephemeral() {
                MemberState::Allocated
            } else {
                MemberState::Free
            }
        }
        Some(ProcessPhase::Failed) | Some(ProcessPhase::Reaped) => MemberState::Failed,
        Some(ProcessPhase::Exiting | ProcessPhase::Zombie) => MemberState::Returning,
        _ => MemberState::Spawning,
    }
}

fn build_member_process(
    pool: &EphemeralPool,
    process_name: &str,
    slot: u32,
    pool_name: &str,
) -> Result<Process> {
    // Pool members start with Permanent lifetime — allocation flips
    // them to Ephemeral with the requestor's TTL.
    let template: EphemeralSpec = pool.spec.template.clone();
    let mut spec: ProcessSpec = template.into();
    spec.lifetime = Lifetime {
        permanent: Some(PermanentLifetime {}),
        ephemeral: None,
    };

    let mut proc = Process::new(process_name, spec);
    let ns = pool.metadata.namespace.clone();
    proc.metadata.namespace = ns;
    let mut annotations = std::collections::BTreeMap::new();
    annotations.insert(ANNOTATION_POOL.to_string(), pool_name.to_string());
    annotations.insert(ANNOTATION_SLOT.to_string(), slot.to_string());
    proc.metadata.annotations = Some(annotations);

    // Owner reference so K8s cascade-deletes members on Pool deletion.
    if let (Some(uid), Some(name)) = (pool.metadata.uid.as_ref(), pool.metadata.name.as_ref()) {
        proc.metadata.owner_references = Some(vec![
            k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference {
                api_version: "tatara.pleme.io/v1alpha1".into(),
                kind: "EphemeralPool".into(),
                name: name.clone(),
                uid: uid.clone(),
                controller: Some(true),
                block_owner_deletion: Some(true),
            },
        ]);
    }

    let _ = POOL_FINALIZER;
    Ok(proc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_process::phase::ProcessPhase;

    #[test]
    fn process_to_member_state_attested_permanent_is_free() {
        let mut p = Process::new(
            "x",
            ProcessSpec {
                identity: Default::default(),
                classification: tatara_process::classification::Classification {
                    point_type: tatara_process::classification::ConvergencePointType::Gate,
                    substrate: tatara_process::classification::SubstrateType::Compute,
                    horizon: Default::default(),
                    calm: Default::default(),
                    data_classification: Default::default(),
                },
                intent: Default::default(),
                boundary: Default::default(),
                compliance: Default::default(),
                depends_on: vec![],
                signals: Default::default(),
                lifetime: Default::default(),
                routing: None,
                encapsulates: None,
                suspended: false,
            },
        );
        p.status = Some(tatara_process::crd::ProcessStatus {
            phase: ProcessPhase::Attested,
            ..Default::default()
        });
        assert_eq!(process_to_member_state(&p), MemberState::Free);
    }

    #[test]
    fn process_to_member_state_attested_ephemeral_is_allocated() {
        let mut spec = ProcessSpec {
            identity: Default::default(),
            classification: tatara_process::classification::Classification {
                point_type: tatara_process::classification::ConvergencePointType::Gate,
                substrate: tatara_process::classification::SubstrateType::Compute,
                horizon: Default::default(),
                calm: Default::default(),
                data_classification: Default::default(),
            },
            intent: Default::default(),
            boundary: Default::default(),
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Default::default(),
            routing: None,
            encapsulates: None,
            suspended: false,
        };
        spec.lifetime = Lifetime {
            ephemeral: Some(tatara_process::lifetime::EphemeralLifetime {
                ttl: "1h".into(),
                teardown_policy: tatara_process::lifetime::TeardownPolicy::Always,
                max_concurrent: 0,
                exports: vec![],
            }),
            ..Default::default()
        };
        let mut p = Process::new("y", spec);
        p.status = Some(tatara_process::crd::ProcessStatus {
            phase: ProcessPhase::Attested,
            ..Default::default()
        });
        assert_eq!(process_to_member_state(&p), MemberState::Allocated);
    }

    fn empty_spec() -> ProcessSpec {
        ProcessSpec {
            identity: Default::default(),
            classification: tatara_process::classification::Classification {
                point_type: tatara_process::classification::ConvergencePointType::Gate,
                substrate: tatara_process::classification::SubstrateType::Compute,
                horizon: Default::default(),
                calm: Default::default(),
                data_classification: Default::default(),
            },
            intent: Default::default(),
            boundary: Default::default(),
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Default::default(),
            routing: None,
            encapsulates: None,
            suspended: false,
        }
    }

    #[test]
    fn belongs_to_pool_via_annotation() {
        let mut p = Process::new("x", empty_spec());
        let mut anns = std::collections::BTreeMap::new();
        anns.insert(ANNOTATION_POOL.into(), "demo-pool".into());
        p.metadata.annotations = Some(anns);
        assert!(process_belongs_to_pool(&p, "demo-pool"));
        assert!(!process_belongs_to_pool(&p, "other"));
    }
}
