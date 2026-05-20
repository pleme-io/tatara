//! Pure allocation-reconcile decision function.

use chrono::{DateTime, Utc};

use tatara_process::allocation::{AllocationPhase, EphemeralAllocation, Requestor};
use tatara_process::pool::{
    AllocationRef, EphemeralPool, MatchKey, MemberState, PoolMember,
};

use crate::router::best_match;

/// What the allocation reconciler does this tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocationDecision {
    /// Bind: pool selected + member chosen.
    Bind { pool: AllocationRef, member_process_name: String },
    /// A pool matched but every member is occupied — try again next tick.
    Wait { pool: AllocationRef },
    /// No pool selector matched. Surface in status; retry on pool spec changes.
    NoMatchingPool,
    /// Already Bound; allocation is stable (heartbeat).
    HeartbeatBound,
    /// `expires_at` reached (or allocation deleted upstream). Trigger return.
    Release { member_process_name: String, pool: AllocationRef },
    /// Released; nothing to do.
    NoOp,
}

/// Decide the next allocation transition.
///
/// `pool_members(&pool)` returns the slice of PoolMembers for the pool
/// whose Allocations the caller is matching. Pure: no kube calls.
pub fn decide_allocation_reconcile<'a, F>(
    alloc: &EphemeralAllocation,
    candidate_pools: &'a [EphemeralPool],
    pool_members: F,
    now: DateTime<Utc>,
) -> AllocationDecision
where
    F: Fn(&'a EphemeralPool) -> &'a [PoolMember],
{
    let phase = alloc
        .status
        .as_ref()
        .map(|s| s.phase)
        .unwrap_or(AllocationPhase::Pending);

    // Terminal phases — short-circuit.
    if phase == AllocationPhase::Released {
        return AllocationDecision::NoOp;
    }

    // Released-but-not-yet-cleaned-up.
    if alloc.metadata.deletion_timestamp.is_some() {
        if let Some(status) = alloc.status.as_ref() {
            if let (Some(member), Some(pool)) =
                (status.assigned_process.as_ref(), status.bound_pool.as_ref())
            {
                return AllocationDecision::Release {
                    member_process_name: member.name.clone(),
                    pool: pool.clone(),
                };
            }
        }
        return AllocationDecision::NoOp;
    }

    // Expiry — TTL reached.
    if phase == AllocationPhase::Bound {
        if let Some(expires_at) = alloc.status.as_ref().and_then(|s| s.expires_at) {
            if now >= expires_at {
                let status = alloc.status.as_ref().unwrap();
                if let (Some(member), Some(pool)) =
                    (status.assigned_process.as_ref(), status.bound_pool.as_ref())
                {
                    return AllocationDecision::Release {
                        member_process_name: member.name.clone(),
                        pool: pool.clone(),
                    };
                }
            }
        }
        return AllocationDecision::HeartbeatBound;
    }

    // Pending / Queued — try to match.
    let pool = if let Some(pool_ref) = alloc.spec.pool_ref.as_ref() {
        candidate_pools.iter().find(|p| {
            p.metadata.name.as_deref() == Some(pool_ref.name.as_str())
                && p.metadata.namespace.as_deref() == Some(pool_ref.namespace.as_str())
        })
    } else {
        let key = match_key_from_requestor(&alloc.spec.requestor);
        let labels_owned: Vec<String> = alloc.spec.requestor.pr_labels.clone();
        let key = MatchKey {
            repo: key.repo,
            branch: key.branch,
            pr_labels: &labels_owned,
            kind: key.kind,
        };
        best_match(candidate_pools, &key).map(|m| m.pool)
    };

    let pool = match pool {
        Some(p) => p,
        None => return AllocationDecision::NoMatchingPool,
    };

    let pool_ref = AllocationRef {
        name: pool.metadata.name.clone().unwrap_or_default(),
        namespace: pool.metadata.namespace.clone().unwrap_or_default(),
    };
    let members = pool_members(pool);
    let free_member = members.iter().find(|m| m.state == MemberState::Free);
    match free_member {
        Some(m) => AllocationDecision::Bind {
            pool: pool_ref,
            member_process_name: m.process_name.clone(),
        },
        None => AllocationDecision::Wait { pool: pool_ref },
    }
}

/// Build a MatchKey from a Requestor for selector routing.
fn match_key_from_requestor(r: &Requestor) -> MatchKeyStrs<'_> {
    MatchKeyStrs {
        repo: r.repo.as_deref().unwrap_or(""),
        branch: r.branch.as_deref().unwrap_or(""),
        kind: r.kind.as_str(),
    }
}

struct MatchKeyStrs<'a> {
    repo: &'a str,
    branch: &'a str,
    kind: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::Resource;
    use tatara_process::allocation::{AllocationSpec, AllocationStatus};
    use tatara_process::ephemeral::EphemeralSpec;
    use tatara_process::intent::AplicacaoIntent;
    use tatara_process::lifetime::TeardownPolicy;
    use tatara_process::pool::{PoolSelector, PoolSpec, ReturnPolicy};

    fn empty_template() -> EphemeralSpec {
        EphemeralSpec {
            aplicacao: AplicacaoIntent {
                chart_ref: "oci://x".into(),
                version: "1".into(),
                profile: String::new(),
                values_overlay: serde_json::Value::Null,
                release_name: None,
                target_namespace: None,
                install_timeout: None,
            },
            ttl: "1h".into(),
            teardown: TeardownPolicy::Always,
            max_concurrent: 0,
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: None,
            exports: vec![],
        }
    }

    fn pool(name: &str, ns: &str, selector: PoolSelector) -> EphemeralPool {
        let spec = PoolSpec {
            desired_size: 1,
            min_size: 0,
            max_size: 0,
            return_policy: ReturnPolicy::Replace,
            selector,
            template: empty_template(),
            free_ttl: "24h".into(),
            max_allocation_ttl: "4h".into(),
        };
        let mut p = EphemeralPool::new(name, spec);
        p.meta_mut().namespace = Some(ns.into());
        p
    }

    fn alloc(kind: &str, repo: &str, branch: &str) -> EphemeralAllocation {
        let spec = AllocationSpec {
            pool_ref: None,
            requestor: Requestor {
                kind: kind.into(),
                repo: Some(repo.into()),
                branch: Some(branch.into()),
                pr_number: None,
                sha: None,
                pr_labels: vec![],
                actor: None,
            },
            ttl: None,
            note: None,
        };
        let mut a = EphemeralAllocation::new("alloc-1", spec);
        a.meta_mut().namespace = Some("pools".into());
        a
    }

    fn member(name: &str, state: MemberState) -> PoolMember {
        PoolMember {
            process_name: name.into(),
            state,
            entered_state_at: Utc::now(),
            allocation_ref: None,
        }
    }

    #[test]
    fn no_matching_pool_when_selector_excludes() {
        let p = pool(
            "akeyless",
            "pools",
            PoolSelector {
                repos: vec!["pleme-io/akeyless-*".into()],
                ..Default::default()
            },
        );
        let a = alloc("manual", "drzln/dotfiles", "main");
        let pools = vec![p];
        let d = decide_allocation_reconcile(&a, &pools, |_| &[], Utc::now());
        assert_eq!(d, AllocationDecision::NoMatchingPool);
    }

    #[test]
    fn bind_when_free_member_available() {
        let p = pool("akeyless", "pools", PoolSelector::default());
        let a = alloc("manual", "any/repo", "main");
        let members = vec![member("akeyless-abcd1234", MemberState::Free)];
        let pools = vec![p];
        let d = decide_allocation_reconcile(&a, &pools, |_| &members, Utc::now());
        match d {
            AllocationDecision::Bind {
                pool,
                member_process_name,
            } => {
                assert_eq!(pool.name, "akeyless");
                assert_eq!(member_process_name, "akeyless-abcd1234");
            }
            other => panic!("expected Bind, got {other:?}"),
        }
    }

    #[test]
    fn wait_when_pool_full() {
        let p = pool("akeyless", "pools", PoolSelector::default());
        let a = alloc("manual", "any/repo", "main");
        let members = vec![member("only-one", MemberState::Allocated)];
        let pools = vec![p];
        let d = decide_allocation_reconcile(&a, &pools, |_| &members, Utc::now());
        match d {
            AllocationDecision::Wait { pool } => {
                assert_eq!(pool.name, "akeyless");
            }
            other => panic!("expected Wait, got {other:?}"),
        }
    }

    #[test]
    fn explicit_pool_ref_bypasses_selector_routing() {
        // Pool selector says repos: ["other/*"], but pool_ref pins it.
        let p = pool(
            "pinned",
            "pools",
            PoolSelector {
                repos: vec!["other/*".into()],
                ..Default::default()
            },
        );
        let mut a = alloc("manual", "completely-unrelated/repo", "main");
        a.spec.pool_ref = Some(AllocationRef {
            name: "pinned".into(),
            namespace: "pools".into(),
        });
        let members = vec![member("pinned-aaaa", MemberState::Free)];
        let pools = vec![p];
        let d = decide_allocation_reconcile(&a, &pools, |_| &members, Utc::now());
        assert!(matches!(d, AllocationDecision::Bind { .. }));
    }

    #[test]
    fn bound_with_expiry_in_past_triggers_release() {
        let p = pool("akeyless", "pools", PoolSelector::default());
        let mut a = alloc("manual", "any/repo", "main");
        let one_hour_ago = Utc::now() - chrono::Duration::hours(1);
        a.status = Some(AllocationStatus {
            phase: AllocationPhase::Bound,
            phase_since: Some(one_hour_ago),
            bound_pool: Some(AllocationRef {
                name: "akeyless".into(),
                namespace: "pools".into(),
            }),
            assigned_process: Some(AllocationRef {
                name: "akeyless-abcd".into(),
                namespace: "pools".into(),
            }),
            allocated_at: Some(one_hour_ago),
            expires_at: Some(one_hour_ago + chrono::Duration::minutes(30)),
            message: None,
            conditions: vec![],
        });
        let d = decide_allocation_reconcile(&a, &[p], |_| &[], Utc::now());
        match d {
            AllocationDecision::Release {
                member_process_name,
                pool,
            } => {
                assert_eq!(member_process_name, "akeyless-abcd");
                assert_eq!(pool.name, "akeyless");
            }
            other => panic!("expected Release, got {other:?}"),
        }
    }

    #[test]
    fn released_allocation_is_noop() {
        let mut a = alloc("manual", "any/repo", "main");
        a.status = Some(AllocationStatus {
            phase: AllocationPhase::Released,
            ..Default::default()
        });
        let d = decide_allocation_reconcile(&a, &[], |_| &[], Utc::now());
        assert_eq!(d, AllocationDecision::NoOp);
    }

    #[test]
    fn deletion_timestamp_releases_assigned_process() {
        let p = pool("akeyless", "pools", PoolSelector::default());
        let mut a = alloc("manual", "any/repo", "main");
        a.metadata.deletion_timestamp =
            Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(Utc::now()));
        a.status = Some(AllocationStatus {
            phase: AllocationPhase::Bound,
            bound_pool: Some(AllocationRef {
                name: "akeyless".into(),
                namespace: "pools".into(),
            }),
            assigned_process: Some(AllocationRef {
                name: "akeyless-xyz".into(),
                namespace: "pools".into(),
            }),
            ..Default::default()
        });
        let d = decide_allocation_reconcile(&a, &[p], |_| &[], Utc::now());
        match d {
            AllocationDecision::Release { member_process_name, .. } => {
                assert_eq!(member_process_name, "akeyless-xyz");
            }
            other => panic!("expected Release on deletion, got {other:?}"),
        }
    }
}
