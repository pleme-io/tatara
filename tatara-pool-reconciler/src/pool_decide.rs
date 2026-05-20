//! Pure pool-reconcile decision function.
//!
//! Given a Pool spec + observed members + clock, decide what to do
//! this tick. The async controller applies the decision via kube-rs.

use chrono::{DateTime, Utc};

use tatara_process::pool::{EphemeralPool, MemberState, PoolMember};

/// One reconcile decision for a Pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolDecision {
    /// Population matches spec; nothing to do.
    NoOp,
    /// Need to create `count` new members.
    Spawn { count: u32 },
    /// Need to reap `count` members (excess Free members).
    ReapExcess { count: u32 },
    /// Specific members need replacement (e.g. Failed or stale).
    ReplaceMembers { process_names: Vec<String> },
    /// Pool is being deleted; reap every member.
    Drain,
}

impl PoolDecision {
    /// Convenience: nothing to do?
    pub fn is_noop(&self) -> bool {
        matches!(self, PoolDecision::NoOp)
    }
}

/// Pure decision over a Pool's current observation.
///
/// Rules (applied in priority order — first match wins):
///
///   1. Pool deletion-stamped → Drain.
///   2. Failed members present → ReplaceMembers.
///   3. Free member stale (older than `free_ttl`) → ReplaceMembers.
///   4. Active count < min_size → Spawn min_size - active.
///   5. Active count > max_size → ReapExcess.
///   6. Free + Spawning < desired_size → Spawn.
///   7. Free > desired_size → ReapExcess (free overflow only — never
///      reap Allocated members).
///   8. Otherwise NoOp.
///
/// "Active" = sum of MemberStates other than Failed.
pub fn decide_pool_reconcile(
    pool: &EphemeralPool,
    members: &[PoolMember],
    now: DateTime<Utc>,
) -> PoolDecision {
    if pool.metadata.deletion_timestamp.is_some() {
        return PoolDecision::Drain;
    }

    let spec = &pool.spec;

    // Bucket members by state.
    let mut free = 0u32;
    let mut allocated = 0u32;
    let mut spawning = 0u32;
    let mut returning = 0u32;
    let mut failed_names: Vec<String> = Vec::new();
    let mut stale_free_names: Vec<String> = Vec::new();

    let free_ttl = humantime::parse_duration(&spec.free_ttl).unwrap_or_default();

    for m in members {
        match m.state {
            MemberState::Free => {
                free += 1;
                if !free_ttl.is_zero() {
                    if let Some(elapsed) = now
                        .signed_duration_since(m.entered_state_at)
                        .to_std()
                        .ok()
                    {
                        if elapsed > free_ttl {
                            stale_free_names.push(m.process_name.clone());
                        }
                    }
                }
            }
            MemberState::Allocated => allocated += 1,
            MemberState::Spawning => spawning += 1,
            MemberState::Returning => returning += 1,
            MemberState::Failed => failed_names.push(m.process_name.clone()),
        }
    }
    let active = free + allocated + spawning + returning;

    // (2) Replace failed members.
    if !failed_names.is_empty() {
        return PoolDecision::ReplaceMembers {
            process_names: failed_names,
        };
    }

    // (3) Replace stale-free members.
    if !stale_free_names.is_empty() {
        return PoolDecision::ReplaceMembers {
            process_names: stale_free_names,
        };
    }

    // (4) Below min_size — spawn.
    if spec.min_size > 0 && active < spec.min_size {
        return PoolDecision::Spawn {
            count: spec.min_size - active,
        };
    }

    // (5) Above max_size — reap. The kube tail reaps Free members
    // first (never Allocated).
    if spec.max_size > 0 && active > spec.max_size {
        return PoolDecision::ReapExcess {
            count: active - spec.max_size,
        };
    }

    // (6) Below desired — spawn.
    let want = spec.desired_size;
    let supply = free + spawning;
    if supply < want {
        return PoolDecision::Spawn { count: want - supply };
    }

    // (7) Free overflow above desired — reap.
    if free > want.saturating_sub(spawning) {
        let excess = free - want.saturating_sub(spawning);
        if excess > 0 {
            return PoolDecision::ReapExcess { count: excess };
        }
    }

    PoolDecision::NoOp
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::Resource;
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
        }
    }

    fn pool(desired: u32, min: u32, max: u32) -> EphemeralPool {
        let spec = PoolSpec {
            desired_size: desired,
            min_size: min,
            max_size: max,
            return_policy: ReturnPolicy::Replace,
            selector: PoolSelector::default(),
            template: empty_template(),
            free_ttl: "24h".into(),
            max_allocation_ttl: "4h".into(),
        };
        let mut p = EphemeralPool::new("test-pool", spec);
        p.meta_mut().namespace = Some("pools".into());
        p
    }

    fn member(name: &str, state: MemberState, age_secs: i64) -> PoolMember {
        PoolMember {
            process_name: name.into(),
            state,
            entered_state_at: Utc::now() - chrono::Duration::seconds(age_secs),
            allocation_ref: None,
        }
    }

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn empty_pool_below_desired_spawns_to_desired() {
        let p = pool(3, 0, 0);
        let d = decide_pool_reconcile(&p, &[], now());
        assert_eq!(d, PoolDecision::Spawn { count: 3 });
    }

    #[test]
    fn at_desired_with_free_members_noop() {
        let p = pool(2, 0, 0);
        let members = vec![
            member("a", MemberState::Free, 60),
            member("b", MemberState::Free, 60),
        ];
        assert_eq!(decide_pool_reconcile(&p, &members, now()), PoolDecision::NoOp);
    }

    #[test]
    fn excess_free_reaps_back_to_desired() {
        let p = pool(1, 0, 0);
        let members = vec![
            member("a", MemberState::Free, 60),
            member("b", MemberState::Free, 60),
            member("c", MemberState::Free, 60),
        ];
        assert_eq!(
            decide_pool_reconcile(&p, &members, now()),
            PoolDecision::ReapExcess { count: 2 }
        );
    }

    #[test]
    fn allocated_members_are_not_counted_against_supply() {
        // 1 desired, but the only member is Allocated → still spawn 1
        // (the allocated one isn't available for new requestors).
        let p = pool(1, 0, 0);
        let members = vec![member("a", MemberState::Allocated, 60)];
        assert_eq!(
            decide_pool_reconcile(&p, &members, now()),
            PoolDecision::Spawn { count: 1 }
        );
    }

    #[test]
    fn spawning_counts_toward_supply() {
        let p = pool(2, 0, 0);
        let members = vec![
            member("a", MemberState::Spawning, 10),
            member("b", MemberState::Free, 60),
        ];
        assert_eq!(decide_pool_reconcile(&p, &members, now()), PoolDecision::NoOp);
    }

    #[test]
    fn failed_members_replaced_before_other_actions() {
        let p = pool(3, 0, 0);
        let members = vec![
            member("a", MemberState::Free, 60),
            member("bad", MemberState::Failed, 60),
            member("c", MemberState::Spawning, 10),
        ];
        let d = decide_pool_reconcile(&p, &members, now());
        match d {
            PoolDecision::ReplaceMembers { process_names } => {
                assert_eq!(process_names, vec!["bad".to_string()]);
            }
            other => panic!("expected ReplaceMembers, got {other:?}"),
        }
    }

    #[test]
    fn stale_free_member_replaced() {
        let mut p = pool(1, 0, 0);
        p.spec.free_ttl = "10s".into();
        let members = vec![member("old", MemberState::Free, 60)];
        let d = decide_pool_reconcile(&p, &members, now());
        assert!(matches!(d, PoolDecision::ReplaceMembers { .. }));
    }

    #[test]
    fn min_size_enforced_even_when_desired_is_smaller() {
        // desired=0, min=2, allocated=1 → spawn 1 to reach min=2.
        let p = pool(0, 2, 0);
        let members = vec![member("a", MemberState::Allocated, 60)];
        let d = decide_pool_reconcile(&p, &members, now());
        assert_eq!(d, PoolDecision::Spawn { count: 1 });
    }

    #[test]
    fn max_size_cap_reaps_above_ceiling() {
        // max=2, allocated=1 + free=2 → active=3, reap 1.
        let p = pool(5, 0, 2);
        let members = vec![
            member("a", MemberState::Allocated, 60),
            member("b", MemberState::Free, 60),
            member("c", MemberState::Free, 60),
        ];
        let d = decide_pool_reconcile(&p, &members, now());
        assert_eq!(d, PoolDecision::ReapExcess { count: 1 });
    }

    #[test]
    fn deletion_stamp_triggers_drain() {
        let mut p = pool(1, 0, 0);
        p.metadata.deletion_timestamp =
            Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(Utc::now()));
        let members = vec![member("a", MemberState::Free, 60)];
        assert_eq!(
            decide_pool_reconcile(&p, &members, now()),
            PoolDecision::Drain
        );
    }
}
