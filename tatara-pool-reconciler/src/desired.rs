//! Pool desired-count convergence — the "always seeking stability"
//! reconciliation loop.
//!
//! The substrate move: separate the **decision** from the **action**.
//! [`decide_pool_convergence`] is a pure function over
//! `(spec, live, failed, now)` returning a `Vec<ConvergenceAction>`
//! that the async controller applies via kube-rs create/delete/
//! patch. No clock, no kube client, no IO — every state-seeking edge
//! case is unit-testable in isolation.
//!
//! Coexists with the legacy `pool_decide::decide_pool_reconcile`
//! (allocation-driven sizing). When `PoolSpec.desired > 0` the new
//! loop takes precedence; when `desired == 0` the legacy path stays
//! authoritative. The controller's reconcile entry point picks one
//! based on this gate.

use chrono::{DateTime, Utc};

use tatara_process::phase::ProcessPhase;
use tatara_process::pool::{EphemeralPool, ReplacementPolicy};

/// One atomic action the controller applies this tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvergenceAction {
    /// Materialize a new pool member from the spec template. The
    /// controller picks a member name + creates the Process.
    CreateMember,
    /// Send SIGTERM to one existing healthy member to scale down.
    /// `process_name` identifies which one (oldest excess by
    /// caller convention).
    SignalSigterm { process_name: String },
    /// Delete one Failed/Reaped member, freeing the slot.
    ReapFailed { process_name: String },
    /// Pause the pool — `desired` is effectively 0 until operator
    /// resumes (only emitted when [`ReplacementPolicy::PausePool`]
    /// triggered).
    Pause { reason: String },
}

/// Snapshot of one Process the convergence loop reasons about.
/// The caller (the async controller) builds this from the cluster's
/// observed state; the decision function doesn't need full
/// Process objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolMemberSnapshot {
    pub process_name: String,
    pub phase: ProcessPhase,
    /// `metadata.creationTimestamp` — used to pick the oldest excess
    /// for scale-down + the oldest live for the claim arbiter.
    pub created_at: DateTime<Utc>,
}

impl PoolMemberSnapshot {
    /// True iff this snapshot is in a phase that counts toward the
    /// pool's healthy member count.
    pub fn is_healthy(&self) -> bool {
        matches!(
            self.phase,
            ProcessPhase::Running | ProcessPhase::Attested
        )
    }

    /// True iff this snapshot is in a phase that should be reaped
    /// + replaced (or held, per policy).
    pub fn is_failed(&self) -> bool {
        matches!(
            self.phase,
            ProcessPhase::Failed | ProcessPhase::Zombie | ProcessPhase::Reaped
        )
    }
}

/// Pure decision over one Pool's current observation.
///
/// Returns a `Vec` because a single reconcile may emit multiple
/// actions (e.g. scale-down + reap-failed). Empty ⇒ pool already
/// matches desired state.
///
/// Rules (priority order):
///
/// 1. `PausePool` policy + any failed members → emit `Pause`.
/// 2. Failed members + `ReplaceImmediate` → reap each.
/// 3. Failed members + `HoldFailed` → do nothing about them
///    (operator inspects + reaps manually).
/// 4. Healthy count < desired → emit `CreateMember` × (desired - healthy).
/// 5. Healthy count > desired → emit `SignalSigterm` × excess, oldest first.
/// 6. Otherwise: no actions.
pub fn decide_pool_convergence(
    pool: &EphemeralPool,
    members: &[PoolMemberSnapshot],
    _now: DateTime<Utc>,
) -> Vec<ConvergenceAction> {
    let desired = pool.spec.desired;
    let policy = pool.spec.replacement_policy;

    let healthy: Vec<&PoolMemberSnapshot> =
        members.iter().filter(|m| m.is_healthy()).collect();
    let failed: Vec<&PoolMemberSnapshot> = members.iter().filter(|m| m.is_failed()).collect();

    let mut actions = Vec::new();

    // (1) PausePool + any failure ⇒ Pause.
    if policy == ReplacementPolicy::PausePool && !failed.is_empty() {
        actions.push(ConvergenceAction::Pause {
            reason: format!(
                "{} member(s) reached Failed/Zombie/Reaped; policy=PausePool",
                failed.len()
            ),
        });
        return actions;
    }

    // (2) ReplaceImmediate ⇒ reap each failed (controller spawns
    //     replacements via rule (4) on the next tick or this same
    //     tick if we also emit CreateMember below).
    if policy == ReplacementPolicy::ReplaceImmediate {
        for m in &failed {
            actions.push(ConvergenceAction::ReapFailed {
                process_name: m.process_name.clone(),
            });
        }
    }
    // (3) HoldFailed ⇒ no reaping action; operator reaps manually.

    // (4)+(5) — count-driven scaling.
    let healthy_count = healthy.len() as u32;
    if healthy_count < desired {
        for _ in 0..(desired - healthy_count) {
            actions.push(ConvergenceAction::CreateMember);
        }
    } else if healthy_count > desired {
        // Pick the oldest excess to SIGTERM (preserves the newer,
        // likely more-current spec).
        let mut sorted: Vec<&&PoolMemberSnapshot> = healthy.iter().collect();
        sorted.sort_by_key(|m| m.created_at);
        let excess = (healthy_count - desired) as usize;
        for m in sorted.iter().take(excess) {
            actions.push(ConvergenceAction::SignalSigterm {
                process_name: m.process_name.clone(),
            });
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::Resource;
    use tatara_process::ephemeral::EphemeralSpec;
    use tatara_process::intent::AplicacaoIntent;
    use tatara_process::lifetime::TeardownPolicy;
    use tatara_process::pool::{PoolSelector, PoolSpec, ReplacementPolicy, ReturnPolicy};

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
            routing: None,
        }
    }

    fn pool_with_desired(desired: u32, policy: ReplacementPolicy) -> EphemeralPool {
        let spec = PoolSpec {
            desired_size: 0,
            min_size: 0,
            max_size: 0,
            return_policy: ReturnPolicy::Replace,
            selector: PoolSelector::default(),
            template: empty_template(),
            free_ttl: "24h".into(),
            max_allocation_ttl: "4h".into(),
            desired,
            replacement_policy: policy,
            stable_name_claim: false,
        };
        let mut p = EphemeralPool::new("test-pool", spec);
        p.meta_mut().namespace = Some("pools".into());
        p
    }

    fn member(name: &str, phase: ProcessPhase, age_secs: i64) -> PoolMemberSnapshot {
        PoolMemberSnapshot {
            process_name: name.into(),
            phase,
            created_at: Utc::now() - chrono::Duration::seconds(age_secs),
        }
    }

    #[test]
    fn empty_pool_below_desired_spawns_to_desired() {
        let p = pool_with_desired(3, ReplacementPolicy::ReplaceImmediate);
        let actions = decide_pool_convergence(&p, &[], Utc::now());
        assert_eq!(actions.len(), 3);
        assert!(actions.iter().all(|a| matches!(a, ConvergenceAction::CreateMember)));
    }

    #[test]
    fn at_desired_no_action() {
        let p = pool_with_desired(2, ReplacementPolicy::ReplaceImmediate);
        let m = vec![
            member("a", ProcessPhase::Running, 60),
            member("b", ProcessPhase::Attested, 60),
        ];
        let actions = decide_pool_convergence(&p, &m, Utc::now());
        assert!(actions.is_empty());
    }

    #[test]
    fn excess_healthy_sigterms_oldest_first() {
        let p = pool_with_desired(1, ReplacementPolicy::ReplaceImmediate);
        let m = vec![
            member("old", ProcessPhase::Running, 1000), // oldest — target
            member("mid", ProcessPhase::Running, 500),
            member("new", ProcessPhase::Running, 60),
        ];
        let actions = decide_pool_convergence(&p, &m, Utc::now());
        assert_eq!(actions.len(), 2);
        match &actions[0] {
            ConvergenceAction::SignalSigterm { process_name } => {
                assert_eq!(process_name, "old"); // oldest goes first
            }
            other => panic!("expected SignalSigterm, got {other:?}"),
        }
    }

    #[test]
    fn failed_reaped_under_replace_immediate() {
        let p = pool_with_desired(2, ReplacementPolicy::ReplaceImmediate);
        let m = vec![
            member("a", ProcessPhase::Running, 60),
            member("b", ProcessPhase::Running, 60),
            member("c", ProcessPhase::Failed, 30),
        ];
        let actions = decide_pool_convergence(&p, &m, Utc::now());
        // ReapFailed for c; no CreateMember because healthy = 2 = desired.
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            ConvergenceAction::ReapFailed { process_name } if process_name == "c"
        ));
    }

    #[test]
    fn failed_held_under_hold_failed() {
        let p = pool_with_desired(2, ReplacementPolicy::HoldFailed);
        let m = vec![
            member("a", ProcessPhase::Running, 60),
            member("b", ProcessPhase::Running, 60),
            member("c", ProcessPhase::Failed, 30),
        ];
        let actions = decide_pool_convergence(&p, &m, Utc::now());
        // No reap (HoldFailed); no spawn (healthy = desired).
        assert!(actions.is_empty());
    }

    #[test]
    fn failed_pauses_pool_under_pause_policy() {
        let p = pool_with_desired(5, ReplacementPolicy::PausePool);
        let m = vec![
            member("a", ProcessPhase::Running, 60),
            member("b", ProcessPhase::Failed, 30),
        ];
        let actions = decide_pool_convergence(&p, &m, Utc::now());
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            ConvergenceAction::Pause { reason } => {
                assert!(reason.contains("PausePool"));
            }
            other => panic!("expected Pause, got {other:?}"),
        }
    }

    #[test]
    fn failed_plus_below_desired_reaps_and_spawns() {
        let p = pool_with_desired(3, ReplacementPolicy::ReplaceImmediate);
        let m = vec![
            member("a", ProcessPhase::Running, 60),
            member("b", ProcessPhase::Failed, 30), // reap
        ];
        let actions = decide_pool_convergence(&p, &m, Utc::now());
        // 1 ReapFailed + 2 CreateMember (healthy=1, desired=3 ⇒ need 2 more)
        assert_eq!(actions.len(), 3);
        let reaps = actions
            .iter()
            .filter(|a| matches!(a, ConvergenceAction::ReapFailed { .. }))
            .count();
        let creates = actions
            .iter()
            .filter(|a| matches!(a, ConvergenceAction::CreateMember))
            .count();
        assert_eq!(reaps, 1);
        assert_eq!(creates, 2);
    }

    #[test]
    fn desired_zero_pauses_implicitly() {
        // desired = 0 ⇒ scale down everything.
        let p = pool_with_desired(0, ReplacementPolicy::ReplaceImmediate);
        let m = vec![
            member("a", ProcessPhase::Running, 60),
            member("b", ProcessPhase::Attested, 60),
        ];
        let actions = decide_pool_convergence(&p, &m, Utc::now());
        // 2 SIGTERMs.
        assert_eq!(actions.len(), 2);
        for a in &actions {
            assert!(matches!(a, ConvergenceAction::SignalSigterm { .. }));
        }
    }

    #[test]
    fn pending_members_dont_count_toward_healthy() {
        // Members in Pending/Forking/Execing are NOT healthy yet —
        // pool keeps spawning until they cross to Running.
        let p = pool_with_desired(3, ReplacementPolicy::ReplaceImmediate);
        let m = vec![
            member("a", ProcessPhase::Pending, 60),
            member("b", ProcessPhase::Forking, 60),
            member("c", ProcessPhase::Execing, 60),
        ];
        let actions = decide_pool_convergence(&p, &m, Utc::now());
        // Healthy = 0; desired = 3. Spawn 3 more even though 3 are
        // already pending. (The controller dedupes by process_name
        // when materializing — pure decision oversupplies; oversupply
        // becomes excess on the next tick once they reach Running.)
        assert_eq!(actions.len(), 3);
        assert!(actions.iter().all(|a| matches!(a, ConvergenceAction::CreateMember)));
    }

    #[test]
    fn releasing_member_not_healthy() {
        // Releasing is the export window — Process is on its way
        // out. NOT eligible to satisfy a desired count.
        let p = pool_with_desired(1, ReplacementPolicy::ReplaceImmediate);
        let m = vec![member("a", ProcessPhase::Releasing, 60)];
        let actions = decide_pool_convergence(&p, &m, Utc::now());
        // Healthy = 0; spawn 1.
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], ConvergenceAction::CreateMember));
    }

    #[test]
    fn snapshot_predicates_align_with_live_phases() {
        // Document: healthy = exactly {Running, Attested}.
        for phase in [
            ProcessPhase::Pending,
            ProcessPhase::Forking,
            ProcessPhase::Execing,
            ProcessPhase::Reconverging,
            ProcessPhase::Releasing,
            ProcessPhase::Exiting,
        ] {
            let s = member("x", phase, 60);
            assert!(!s.is_healthy(), "{phase:?} should not be healthy");
            assert!(!s.is_failed(), "{phase:?} should not be failed");
        }
        for phase in [ProcessPhase::Running, ProcessPhase::Attested] {
            let s = member("x", phase, 60);
            assert!(s.is_healthy());
            assert!(!s.is_failed());
        }
        for phase in [ProcessPhase::Failed, ProcessPhase::Zombie, ProcessPhase::Reaped] {
            let s = member("x", phase, 60);
            assert!(!s.is_healthy());
            assert!(s.is_failed());
        }
    }
}
