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
    if pool.is_being_deleted() {
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

    // The `humantime::parse_duration(&<field>).ok()` shape rides
    // through the ONE substrate primitive
    // [`tatara_process::pool::PoolSpec::free_ttl_duration`] — pre-lift
    // this was a hand-authored `humantime::parse_duration(&spec
    // .free_ttl).ok()` chain (spelled with a `.unwrap_or_default()`
    // tail collapsing the parse-failure corner to `Duration::ZERO`,
    // which the `!free_ttl.is_zero()` gate below immediately rejected).
    // The shape was already owned at ONE substrate primitive on
    // [`tatara_process::lifetime::EphemeralLifetime::ttl_duration`]
    // (the `spec.lifetime.ephemeral.ttl` axis), so this site restated
    // the SAME shape on a peer humantime field of a peer spec type
    // past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger. Post-lift
    // both peer fields publish the SAME shape at TWO peer inherent
    // methods on peer spec types; a future normalization (per-fleet
    // minimum floor, canonical unit normalization, warn-log on
    // unparseable strings) lands at ONE workspace-wide sweep across
    // the peer axis rather than as a per-callsite hand-edit here + on
    // the ephemeral TTL-expiry gate.
    let free_ttl = spec.free_ttl_duration().unwrap_or_default();

    for m in members {
        match m.state {
            MemberState::Free => {
                free += 1;
                if !free_ttl.is_zero() {
                    // The `(now, m.entered_state_at) → Option
                    // <std::time::Duration>` projection rides through
                    // the ONE substrate primitive
                    // [`tatara_process::time::elapsed_since`], sibling
                    // to the same-chain TTL-expiry gate in
                    // `tatara-process::lifetime_clock::evaluate` and
                    // the sleep-budget picker in `requeue_with_ttl`.
                    // Pre-lift each of the three sites hand-authored
                    // `now.signed_duration_since(<anchor>).to_std()
                    // .ok()` past the ★★ PRIME-DIRECTIVE ≥ 2
                    // duplication threshold; post-lift each routes
                    // through ONE typed owner and a future
                    // normalization (monotonic-clock cross-check,
                    // per-fleet skew tolerance, subsecond truncation)
                    // lands at ONE substrate site.
                    if let Some(elapsed) =
                        tatara_process::time::elapsed_since(now, m.entered_state_at)
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
        return PoolDecision::Spawn {
            count: want - supply,
        };
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

/// Wall-clock-anchored peer of [`decide_pool_reconcile`] — the ONE
/// substrate owner of the `decide_pool_reconcile(<pool>, <members>,
/// chrono::Utc::now())` shape every production status-tick + every
/// clock-defaulted unit test walked pre-lift.
///
/// # Pre-lift shape + migration rationale
///
/// The 3-arg [`decide_pool_reconcile`] takes a `now: DateTime<Utc>`
/// tick-anchor that the (7)-rule ladder threads down to the free-TTL
/// staleness gate; every production reconciler tick reads a fresh
/// wall-clock instant at the callsite and every clock-defaulted test
/// helper (the local `now()` shim that returned `Utc::now()`) fed the
/// SAME instant in. Pre-lift 11 sites (1 production status-tick at
/// [`crate::controller_pool::reconcile_inner`] + 10 unit tests in
/// this module's own suite) walked the SAME 3-arg call with the SAME
/// `chrono::Utc::now()` third argument. Post-lift they share ONE
/// substrate owner; a future clock swap (a monotonic clock cross-
/// check, a per-reconciler injected time source, a test-only override
/// via feature flag) lands at ONE substrate function and every pool-
/// reconcile status-tick + every clock-defaulted test inherits the
/// upgrade mechanically.
///
/// The 3-arg [`decide_pool_reconcile`] peer stays load-bearing for
/// test callers that drive the clock deterministically (any test that
/// pins a specific instant relative to a member's `entered_state_at`
/// still reaches the 3-arg surface with an explicit anchor) — this
/// peer is for the (production-tick + clock-defaulted-test) family
/// where the wall clock is the correct anchor.
///
/// Sibling of [`tatara_process::allocation::AllocationStatus::
/// transition_now`], [`tatara_process::pool::PoolStatus::observed_now`],
/// and [`tatara_process::lifetime_clock::evaluate_now`] on the
/// `(typed pure-fn, wall-clock-anchored peer)` axis for the
/// `tatara-pool-reconciler` decision family — each primitive owns the
/// "read the wall clock at tick-time" projection at ONE substrate
/// composer so the workspace's timed-decision family stays uniform
/// across every `<CRD>Status` composer + every `decide_<crd>_<verb>`
/// entry point.
///
/// # Invariants
///
/// - **Same shape:** returns the SAME [`PoolDecision`] the 3-arg
///   [`decide_pool_reconcile`] returns when passed
///   `chrono::Utc::now()` as the third argument. This is a
///   delegation, not a re-implementation — the `_now_matches_utc_
///   now_stamped_base` parity test in this module's own suite
///   guards the equivalence.
/// - **Wall-clock read once:** `Utc::now()` is called exactly ONCE
///   per invocation, at the primitive's body, so a caller that
///   composes two decisions back-to-back still sees monotonic `now`
///   reads (each call reads a fresh instant, not a cached one) —
///   matches the pre-lift shape where each of the 11 hand-authored
///   sites computed its own `chrono::Utc::now()` at its own line.
///
/// Theory anchor: `theory/THEORY.md` §VI.1 (generation over
/// composition — the 3-arg call with `chrono::Utc::now()` as the
/// third argument recurred at 11 hand-authored sites past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication trigger, lifted onto the ONE
/// crate-wide substrate owner here). `theory/THEORY.md` §II.1
/// invariant 5 (composition preserves proofs — the wall-clock
/// projection lives at ONE site so a future clock swap reaches
/// every pool-reconcile consumer through one edit).
#[must_use]
pub fn decide_pool_reconcile_now(pool: &EphemeralPool, members: &[PoolMember]) -> PoolDecision {
    decide_pool_reconcile(pool, members, Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_process::ephemeral::EphemeralSpec;
    use tatara_process::intent::AplicacaoIntent;
    use tatara_process::lifetime::TeardownPolicy;
    use tatara_process::pool::PoolSpec;

    fn empty_template() -> EphemeralSpec {
        EphemeralSpec {
            aplicacao: AplicacaoIntent::chart_only("oci://x", "1"),
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

    fn pool(desired: u32, min: u32, max: u32) -> EphemeralPool {
        // Every non-template slot rides the ONE substrate composer
        // [`PoolSpec::with_template`]; see the primitive's doc-comment
        // for the full migration rationale.
        let spec = PoolSpec {
            desired_size: desired,
            min_size: min,
            max_size: max,
            ..PoolSpec::with_template(empty_template())
        };
        // The 2-line construct-then-set-namespace chain rides the ONE
        // substrate composer [`EphemeralPool::new_in`] — one of FOUR
        // pre-lift sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
        // threshold in this crate; see the primitive's doc-comment
        // for the migration rationale + the sibling
        // `EphemeralAllocation::new_in` peer.
        EphemeralPool::new_in("test-pool", "pools", spec)
    }

    fn member(name: &str, state: MemberState, age_secs: i64) -> PoolMember {
        // 4-slot unallocated seed rides through the ONE substrate
        // owner `tatara_process::pool::PoolMember::unallocated` — peer
        // of the four workspace-wide restatements of the SAME 4-slot
        // fixture literal at the production `controller_pool::
        // reconcile_inner` walk + the sibling `allocation_decide::
        // tests::member` helper + the two `pool::tests::{member,
        // named_member}` helpers in `tatara-process`. The `seconds_ago`
        // anchor rides in on the peer substrate primitive
        // `tatara_process::time::seconds_ago` (same-axis clock-past
        // composer) so both the anchor + the seed compose through
        // typed substrate owners rather than through hand-authored
        // struct-literals here.
        PoolMember::unallocated(name, state, tatara_process::time::seconds_ago(age_secs))
    }

    // The local `fn now() -> DateTime<Utc> { Utc::now() }` shim that
    // pre-lift wrapped every clock-defaulted test call has been folded
    // into the substrate peer [`decide_pool_reconcile_now`] — every
    // site below reaches the wall-clock projection through the typed
    // owner rather than through an ad-hoc test-side helper.

    #[test]
    fn empty_pool_below_desired_spawns_to_desired() {
        let p = pool(3, 0, 0);
        let d = decide_pool_reconcile_now(&p, &[]);
        assert_eq!(d, PoolDecision::Spawn { count: 3 });
    }

    #[test]
    fn at_desired_with_free_members_noop() {
        let p = pool(2, 0, 0);
        let members = vec![
            member("a", MemberState::Free, 60),
            member("b", MemberState::Free, 60),
        ];
        assert_eq!(decide_pool_reconcile_now(&p, &members), PoolDecision::NoOp);
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
            decide_pool_reconcile_now(&p, &members),
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
            decide_pool_reconcile_now(&p, &members),
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
        assert_eq!(decide_pool_reconcile_now(&p, &members), PoolDecision::NoOp);
    }

    #[test]
    fn failed_members_replaced_before_other_actions() {
        let p = pool(3, 0, 0);
        let members = vec![
            member("a", MemberState::Free, 60),
            member("bad", MemberState::Failed, 60),
            member("c", MemberState::Spawning, 10),
        ];
        let d = decide_pool_reconcile_now(&p, &members);
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
        let d = decide_pool_reconcile_now(&p, &members);
        assert!(matches!(d, PoolDecision::ReplaceMembers { .. }));
    }

    #[test]
    fn min_size_enforced_even_when_desired_is_smaller() {
        // desired=0, min=2, allocated=1 → spawn 1 to reach min=2.
        let p = pool(0, 2, 0);
        let members = vec![member("a", MemberState::Allocated, 60)];
        let d = decide_pool_reconcile_now(&p, &members);
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
        let d = decide_pool_reconcile_now(&p, &members);
        assert_eq!(d, PoolDecision::ReapExcess { count: 1 });
    }

    #[test]
    fn deletion_stamp_triggers_drain() {
        let mut p = pool(1, 0, 0);
        // Routes through the ONE substrate composer
        // `tatara_process::time::tombstone_now` — one of 12 pre-lift
        // exact-match sites past the ★★ PRIME-DIRECTIVE ≥ 2 threshold
        // for the `Some(Time(Utc::now()))` wire shape.
        p.metadata.deletion_timestamp = tatara_process::time::tombstone_now();
        let members = vec![member("a", MemberState::Free, 60)];
        assert_eq!(decide_pool_reconcile_now(&p, &members), PoolDecision::Drain);
    }

    /// Parity guard: the wall-clock-anchored peer
    /// [`decide_pool_reconcile_now`] MUST return the SAME
    /// [`PoolDecision`] the 3-arg [`decide_pool_reconcile`] returns
    /// when passed `chrono::Utc::now()` as the third argument. A
    /// regression that reshaped one surface without the other (a
    /// clock-swap that touched only one, a rule-priority reordering
    /// applied only to the `_now` peer, a rounding step introduced
    /// on the peer's own tick anchor) would silently drift the ONE
    /// substrate owner from its clock-injectable base — the parity
    /// test forces both surfaces to walk the SAME rule ladder.
    ///
    /// Fires the peer + the base at wall clocks that differ only by
    /// the microseconds between the two calls; the decision family
    /// is stable across sub-second drift so both MUST agree even
    /// though `Utc::now()` reads twice.
    #[test]
    fn decide_pool_reconcile_now_matches_utc_now_stamped_base_call() {
        // Rules (2) failed-members-replace + (5) max-cap reap +
        // (6) free-below-desired spawn + (8) NoOp — a spread across
        // the ladder's priority tiers so the parity test exercises
        // every non-time-sensitive arm, not just one.
        let cases: Vec<(EphemeralPool, Vec<PoolMember>, PoolDecision)> = vec![
            // (2) Failed short-circuit.
            (
                pool(3, 0, 0),
                vec![
                    member("a", MemberState::Free, 60),
                    member("bad", MemberState::Failed, 60),
                ],
                PoolDecision::ReplaceMembers {
                    process_names: vec!["bad".to_string()],
                },
            ),
            // (5) Max-size cap reap.
            (
                pool(5, 0, 2),
                vec![
                    member("a", MemberState::Allocated, 60),
                    member("b", MemberState::Free, 60),
                    member("c", MemberState::Free, 60),
                ],
                PoolDecision::ReapExcess { count: 1 },
            ),
            // (6) Below desired → spawn.
            (pool(3, 0, 0), vec![], PoolDecision::Spawn { count: 3 }),
            // (8) NoOp — at desired with free members.
            (
                pool(2, 0, 0),
                vec![
                    member("x", MemberState::Free, 60),
                    member("y", MemberState::Free, 60),
                ],
                PoolDecision::NoOp,
            ),
        ];

        for (p, members, expected) in cases {
            let via_peer = decide_pool_reconcile_now(&p, &members);
            let via_base = decide_pool_reconcile(&p, &members, Utc::now());
            assert_eq!(
                via_peer, expected,
                "peer disagreed with expectation on {expected:?}",
            );
            assert_eq!(
                via_peer, via_base,
                "peer/base drift on {expected:?}: peer={via_peer:?} base={via_base:?}",
            );
        }
    }
}
