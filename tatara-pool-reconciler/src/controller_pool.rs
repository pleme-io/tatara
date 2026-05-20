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
use tatara_process::pool::{
    EphemeralPool, MemberState, PoolMember, PoolPhase, PoolStatus,
};
use tatara_process::prelude::{Process, ProcessSpec};

use crate::context::PoolContext;
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
    let ns = pool
        .metadata
        .namespace
        .clone()
        .ok_or_else(|| anyhow!("Pool has no metadata.namespace"))?;
    let name = pool
        .metadata
        .name
        .clone()
        .ok_or_else(|| anyhow!("Pool has no metadata.name"))?;

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
                process_name: p.metadata.name.clone().unwrap_or_default(),
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

    // 2. Decide.
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
            let pool_uid = pool
                .metadata
                .uid
                .clone()
                .unwrap_or_else(|| name.clone());
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
    if pool.metadata.deletion_timestamp.is_some() {
        return PoolPhase::Draining;
    }
    let free = count_state(members, MemberState::Free);
    let spawning = count_state(members, MemberState::Spawning);
    let supply = free + spawning;
    let want = pool.spec.desired_size;
    if members.is_empty() {
        return PoolPhase::Initializing;
    }
    if pool.spec.min_size > 0 && (free + spawning) < pool.spec.min_size {
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

fn process_belongs_to_pool(p: &Process, pool_name: &str) -> bool {
    p.metadata
        .annotations
        .as_ref()
        .and_then(|a| a.get(ANNOTATION_POOL))
        .map(String::as_str)
        == Some(pool_name)
}

fn process_to_member_state(p: &Process) -> MemberState {
    use tatara_process::phase::ProcessPhase;
    match p.status.as_ref().map(|s| s.phase) {
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
        proc.metadata.owner_references =
            Some(vec![k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference {
                api_version: "tatara.pleme.io/v1alpha1".into(),
                kind: "EphemeralPool".into(),
                name: name.clone(),
                uid: uid.clone(),
                controller: Some(true),
                block_owner_deletion: Some(true),
            }]);
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
            suspended: false,
        };
        spec.lifetime = Lifetime {
            ephemeral: Some(tatara_process::lifetime::EphemeralLifetime {
                ttl: "1h".into(),
                teardown_policy: tatara_process::lifetime::TeardownPolicy::Always,
                max_concurrent: 0,
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
            suspended: false,
        }
    }

    #[test]
    fn belongs_to_pool_via_annotation() {
        let mut p = Process::new("x", empty_spec());
        let mut anns = std::collections::BTreeMap::new();
        anns.insert(ANNOTATION_POOL.into(), "akeyless".into());
        p.metadata.annotations = Some(anns);
        assert!(process_belongs_to_pool(&p, "akeyless"));
        assert!(!process_belongs_to_pool(&p, "other"));
    }
}
