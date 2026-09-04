//! Pure allocation-reconcile decision — the allocation reconciler's
//! [`shigoto_types::decision::Decision`] consumer (sibling to the pool
//! desired-count `PoolConvergence` in `desired.rs`). `observe` resolves the
//! world relevant to one allocation (which pool it targets + whether that
//! pool has a free member); `decide` is the pure transition rule.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use tatara_process::allocation::{AllocationPhase, EphemeralAllocation, Requestor};
use tatara_process::pool::{AllocationRef, EphemeralPool, MatchKey, MemberState, PoolMember};
use tatara_process::DeletionTombstoned;

use crate::router::best_match;

/// What the allocation reconciler does this tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocationDecision {
    /// Bind: pool selected + member chosen.
    Bind {
        pool: AllocationRef,
        member_process_name: String,
    },
    /// A pool matched but every member is occupied — try again next tick.
    Wait { pool: AllocationRef },
    /// No pool selector matched. Surface in status; retry on pool spec changes.
    NoMatchingPool,
    /// Already Bound; allocation is stable (heartbeat).
    HeartbeatBound,
    /// `expires_at` reached (or allocation deleted upstream). Trigger return.
    Release {
        member_process_name: String,
        pool: AllocationRef,
    },
    /// Released; nothing to do.
    NoOp,
}

/// The structured allocation-reconcile decision context — one owned,
/// serializable snapshot of everything the transition rule needs: the
/// allocation's lifecycle fields plus the pool it resolved to (and whether
/// that pool has a free member). The async controller builds this from the
/// live `EphemeralAllocation` + candidate pools + a member lookup (the
/// **observe** half); [`AllocationConvergence::decide`] is the pure
/// **decide** half. Because the context is `Serialize`, the decision is
/// table-testable from a literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationConvergenceCtx {
    /// Lifecycle phase (`Pending` if status is unset).
    pub phase: AllocationPhase,
    /// `metadata.deletionTimestamp` is set — a release is in progress.
    pub being_deleted: bool,
    /// TTL deadline for a `Bound` allocation, if any.
    pub expires_at: Option<DateTime<Utc>>,
    /// Currently-assigned member process name (needed to emit `Release`).
    pub assigned_process: Option<String>,
    /// The pool this allocation is bound to (needed to emit `Release`).
    pub bound_pool: Option<AllocationRef>,
    /// Decision clock.
    pub now: DateTime<Utc>,
    /// The pool this allocation should bind to, resolved during `observe` by
    /// `pool_ref` / selector matching. `None` ⇒ no pool matched. Only
    /// consulted on the `Pending`/`Queued` path.
    pub matched_pool: Option<AllocationRef>,
    /// A `Free` member's process name in the matched pool, if any.
    /// `free_member = None` while `matched_pool = Some` ⇒ pool full ⇒ `Wait`.
    pub free_member: Option<String>,
}

impl AllocationConvergenceCtx {
    /// Observe: resolve the world relevant to one allocation. For the
    /// `Pending`/`Queued` path this matches the target pool (explicit
    /// `pool_ref` or selector via [`best_match`]) and looks up a free member;
    /// terminal / deleting / `Bound` allocations need no pool match.
    ///
    /// `pool_members(&pool)` returns the matched pool's members (pure: no
    /// kube calls).
    #[must_use]
    pub fn observe<'a, F>(
        alloc: &EphemeralAllocation,
        candidate_pools: &'a [EphemeralPool],
        pool_members: F,
        now: DateTime<Utc>,
    ) -> Self
    where
        F: Fn(&'a EphemeralPool) -> &'a [PoolMember],
    {
        // Phase snapshot rides through the ONE substrate primitive
        // `EphemeralAllocation::observed_phase_or_pending` — pre-lift
        // this was a hand-authored 5-line `.status.as_ref().map(|s|
        // s.phase).unwrap_or(AllocationPhase::Pending)` chain, the
        // cross-CRD peer of the same-shape lift already owned by
        // `Process::observed_phase_or_pending` (which sank FIVE
        // pre-lift restatements). Both CRDs' phase-slot observers
        // now share ONE substrate accessor per CRD; a future
        // normalization (a generation-filter, a staleness gate, a
        // canonicalization pass) lands at `tatara_process::prelude::
        // EphemeralAllocation::observed_phase_or_pending` and this
        // routing seed inherits the upgrade mechanically alongside
        // the `Process` peer consumers.
        let phase = alloc.observed_phase_or_pending();
        // Tombstone-presence probe rides through the ONE substrate
        // primitive `DeletionTombstoned::is_being_deleted` (blanket
        // impl over every `kube::Resource<DynamicType = ()>`) — pre-
        // lift this was a hand-authored `.metadata.deletion_timestamp
        // .is_some()` chain, the third-CRD peer to the two byte-
        // identical inherent forwarders already living on
        // `Process::is_being_deleted` + `EphemeralPool::is_being_deleted`.
        // Post-lift `EphemeralAllocation` inherits the probe through
        // the substrate trait; a future normalization (grace-period
        // staleness gate, paused-controller canonicalization,
        // cross-cluster clock-skew guard) lands at the ONE trait
        // method and this observer picks it up mechanically.
        let being_deleted = alloc.is_being_deleted();
        // TTL-expiry deadline rides through the ONE substrate primitive
        // `EphemeralAllocation::observed_expires_at` — pre-lift this was
        // a hand-authored `.status.as_ref().and_then(|s| s.expires_at)`
        // chain, the third-slot copy-form peer to the two byte-identical
        // status-projection primitives already living on the same CRD
        // (`observed_phase` on `status.phase`, `observed_bound_pool` on
        // `status.bound_pool`). Post-lift the substrate owns every
        // `Copy`-valued and structured-record status-slot projection on
        // `EphemeralAllocation`; a future normalization (a clock-skew
        // guard, a monotonic-deadline canonicalization, a stale-timestamp
        // gate) lands at the ONE substrate method and this Release-
        // composition TTL gate inherits the upgrade mechanically.
        let expires_at = alloc.observed_expires_at();
        let assigned_process = alloc
            .status
            .as_ref()
            .and_then(|s| s.assigned_process.as_ref())
            .map(|m| m.name.clone());
        // Bound-pool reference rides through the ONE substrate
        // primitive `EphemeralAllocation::observed_bound_pool` — pre-
        // lift this was a hand-authored `.status.as_ref().and_then(|s|
        // s.bound_pool.clone())` chain that allocated an owned
        // `AllocationRef` every reconcile pass, matching the substrate
        // shape the peer `Process::observed_identity` borrow-form
        // primitive already owns on the `status.identity` slot. Post-
        // lift the borrow-form primitive returns the persisted
        // reference in place and this seed clones only at the
        // composition point where the `AllocationConvergenceCtx`
        // snapshot slot (still `Option<AllocationRef>`-typed for
        // serde stability) requires the owned value; the empty-borrow
        // corner clones nothing (`Option::cloned` on `None` is
        // `None`). A future normalization (a generation-filter, a
        // canonicalization pass rejecting a malformed
        // `(name, namespace)` pair, a cross-cluster reference-rewrite
        // gate) lands at the ONE substrate method and this seed
        // inherits the upgrade mechanically alongside the peer
        // Process consumers.
        let bound_pool = alloc.observed_bound_pool().cloned();

        // Resolve the target pool + a free member ONLY on the matching path
        // (Pending/Queued/NoMatchingPool, not deleting/terminal/Bound/Releasing).
        // Lifted onto the typed `AllocationPhase::needs_pool_routing()`
        // closed-set projection — closes the latent gap where `Failed` /
        // `Releasing` (neither `Released` nor `Bound`) used to slip through
        // and trigger a phantom rebind on an allocation that the operator
        // needs to inspect or a release that's mid-flight.
        let (matched_pool, free_member) = if phase.needs_pool_routing() && !being_deleted {
            match resolve_pool(alloc, candidate_pools) {
                Some(pool) => {
                    // Both `AllocationRef` halves ride through the
                    // paired substrate primitives
                    // `EphemeralPool::owned_name_or_empty` +
                    // `EphemeralPool::owned_namespace_or_empty` — pre-
                    // lift both slots were hand-authored
                    // `.metadata.<slot>.clone().unwrap_or_default()`
                    // chains, one of TWO workspace-wide restatements
                    // per slot past the ★★ PRIME-DIRECTIVE ≥ 2
                    // duplication threshold. The overall (name,
                    // namespace) pair rides through the substrate
                    // constructor `AllocationRef::new` — pre-lift the
                    // `AllocationRef { name, namespace }` struct-
                    // literal was hand-authored at FOUR workspace-wide
                    // sites past the same threshold. All three
                    // primitives are now owned by tatara-process; a
                    // future canonicalization pass on either half lands
                    // at the ONE substrate owner and this seed picks
                    // it up mechanically.
                    let pool_ref = AllocationRef::new(
                        pool.owned_name_or_empty(),
                        pool.owned_namespace_or_empty(),
                    );
                    let free = pool_members(pool)
                        .iter()
                        .find(|m| m.state == MemberState::Free)
                        .map(|m| m.process_name.clone());
                    (Some(pool_ref), free)
                }
                None => (None, None),
            }
        } else {
            (None, None)
        };

        Self {
            phase,
            being_deleted,
            expires_at,
            assigned_process,
            bound_pool,
            now,
            matched_pool,
            free_member,
        }
    }
}

/// The allocation-reconcile decision — the allocation reconciler's
/// [`shigoto_types::decision::Decision`] consumer. A zero-sized marker;
/// `decide` is the entire pure transition rule (priority order):
///
/// 1. `Released` ⇒ `NoOp`.
/// 2. Being deleted + assigned ⇒ `Release` (else `NoOp`).
/// 3. `Bound` + expired + assigned ⇒ `Release`; otherwise `HeartbeatBound`.
/// 4. `Pending`/`Queued`: no matched pool ⇒ `NoMatchingPool`; a free member ⇒
///    `Bind`; pool full ⇒ `Wait`.
pub struct AllocationConvergence;

impl shigoto_types::decision::Decision for AllocationConvergence {
    type Ctx = AllocationConvergenceCtx;
    type Action = AllocationDecision;

    fn decide(ctx: &Self::Ctx) -> Self::Action {
        // (1) Terminal — `Released` (clean audit record) or `Failed`
        //     (pool refused; operator intervention needed) — short-circuit
        //     before re-running the routing / heartbeat ladder. Lifted
        //     onto the typed `AllocationPhase::is_terminal()` closed-set
        //     projection so a new terminal variant lands once.
        if ctx.phase.is_terminal() {
            return AllocationDecision::NoOp;
        }

        // (2) Released-but-not-yet-cleaned-up.
        if ctx.being_deleted {
            return match (ctx.assigned_process.as_ref(), ctx.bound_pool.as_ref()) {
                (Some(member), Some(pool)) => AllocationDecision::Release {
                    member_process_name: member.clone(),
                    pool: pool.clone(),
                },
                _ => AllocationDecision::NoOp,
            };
        }

        // (3) Bound — expiry (TTL reached) or heartbeat.
        if ctx.phase == AllocationPhase::Bound {
            if let Some(expires_at) = ctx.expires_at {
                if ctx.now >= expires_at {
                    if let (Some(member), Some(pool)) =
                        (ctx.assigned_process.as_ref(), ctx.bound_pool.as_ref())
                    {
                        return AllocationDecision::Release {
                            member_process_name: member.clone(),
                            pool: pool.clone(),
                        };
                    }
                }
            }
            return AllocationDecision::HeartbeatBound;
        }

        // (4) Pending / Queued — bind to the matched pool's free member.
        match ctx.matched_pool.as_ref() {
            None => AllocationDecision::NoMatchingPool,
            Some(pool) => match ctx.free_member.as_ref() {
                Some(member) => AllocationDecision::Bind {
                    pool: pool.clone(),
                    member_process_name: member.clone(),
                },
                None => AllocationDecision::Wait { pool: pool.clone() },
            },
        }
    }
}

/// Resolve the pool an allocation targets: an explicit `pool_ref` wins;
/// otherwise the requestor's selector picks the best match. Pure.
fn resolve_pool<'a>(
    alloc: &EphemeralAllocation,
    candidate_pools: &'a [EphemeralPool],
) -> Option<&'a EphemeralPool> {
    if let Some(pool_ref) = alloc.spec.pool_ref.as_ref() {
        // The `metadata.name` byte-comparison rides through the
        // substrate primitive `EphemeralPool::has_name` — pre-lift
        // this was a hand-authored `.metadata.name.as_deref() == Some
        // (pool_ref.name.as_str())` chain, one of TWO workspace-wide
        // restatements past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
        // threshold (sibling site at
        // `controller_allocation::reconcile_inner`'s TTL-inheritance
        // fallback). The primitive keeps the `None`-preserving
        // discipline: a namespace-absent pool with an empty
        // `pool_ref.name` returns `false`, not a spurious match. The
        // namespace half is intentionally still spelled here — only
        // ONE site probes `metadata.namespace` this way, so it stays
        // below the ≥ 2 duplication trigger.
        candidate_pools.iter().find(|p| {
            p.has_name(&pool_ref.name)
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
    }
}

/// Decide the next allocation transition. Stable public entry point — an
/// **observe → decide** shim over the [`AllocationConvergence`] `Decision`
/// impl. `pool_members(&pool)` returns the matched pool's members.
pub fn decide_allocation_reconcile<'a, F>(
    alloc: &EphemeralAllocation,
    candidate_pools: &'a [EphemeralPool],
    pool_members: F,
    now: DateTime<Utc>,
) -> AllocationDecision
where
    F: Fn(&'a EphemeralPool) -> &'a [PoolMember],
{
    use shigoto_types::decision::Decision;
    AllocationConvergence::decide(&AllocationConvergenceCtx::observe(
        alloc,
        candidate_pools,
        pool_members,
        now,
    ))
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
    use tatara_process::allocation::{AllocationSpec, AllocationStatus};
    use tatara_process::ephemeral::EphemeralSpec;
    use tatara_process::intent::AplicacaoIntent;
    use tatara_process::lifetime::TeardownPolicy;
    use tatara_process::pool::{PoolSelector, PoolSpec};

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

    fn pool(name: &str, ns: &str, selector: PoolSelector) -> EphemeralPool {
        // Every non-template slot rides the ONE substrate composer
        // [`PoolSpec::with_template`]; see the primitive's doc-comment
        // for the full migration rationale.
        let spec = PoolSpec {
            desired_size: 1,
            selector,
            ..PoolSpec::with_template(empty_template())
        };
        // The 2-line construct-then-set-namespace chain rides the ONE
        // substrate composer [`EphemeralPool::new_in`] — one of FOUR
        // pre-lift sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
        // threshold in this crate; see the primitive's doc-comment
        // for the migration rationale + the sibling
        // `EphemeralAllocation::new_in` peer.
        EphemeralPool::new_in(name, ns, spec)
    }

    fn alloc(kind: &str, repo: &str, branch: &str) -> EphemeralAllocation {
        // AllocationSpec rides through the ONE substrate composer
        // `AllocationSpec::requestor_only` — one of NINE pre-lift
        // exact-match `AllocationSpec { pool_ref: None, requestor:
        // <r>, ttl: None, note: None }` fixture sites past the ★★
        // PRIME-DIRECTIVE ≥ 2 threshold. The inner Requestor keeps
        // its exhaustive struct literal here because this fixture is
        // the ONE callsite that populates repo + branch alongside the
        // kind slot — Requestor::kind_only would leave repo + branch
        // at None, so the caller composes on top of the six-slot
        // default tail via struct-update rather than through the
        // kind-only sibling composer.
        let spec = AllocationSpec::requestor_only(Requestor {
            kind: kind.into(),
            repo: Some(repo.into()),
            branch: Some(branch.into()),
            pr_number: None,
            sha: None,
            pr_labels: vec![],
            actor: None,
        });
        // The 2-line construct-then-set-namespace chain rides the ONE
        // substrate composer [`EphemeralAllocation::new_in`] — one of
        // TWO pre-lift sites past the ★★ PRIME-DIRECTIVE ≥ 2
        // duplication threshold across two workspace crates (peer at
        // `tatara-github-watcher::allocation_factory::build_allocation`
        // — the production PR-webhook emitter); see the primitive's
        // doc-comment for the migration rationale + the sibling
        // `EphemeralPool::new_in` peer.
        EphemeralAllocation::new_in("alloc-1", "pools", spec)
    }

    fn member(name: &str, state: MemberState) -> PoolMember {
        // 4-slot unallocated seed rides through the ONE substrate
        // owner `tatara_process::pool::PoolMember::unallocated` — peer
        // of the four workspace-wide restatements at the production
        // `controller_pool::reconcile_inner` walk + the sibling
        // `pool_decide::tests::member` helper (which anchors via
        // `tatara_process::time::seconds_ago`) + the two `pool::tests::
        // {member, named_member}` helpers in `tatara-process` (which
        // anchor at the epoch). This helper keeps its wall-clock
        // anchor via `Utc::now()` at the callsite — the composer is
        // clock-injectable rather than reading wall time itself.
        PoolMember::unallocated(name, state, Utc::now())
    }

    #[test]
    fn no_matching_pool_when_selector_excludes() {
        let p = pool(
            "demo-pool",
            "pools",
            PoolSelector {
                repos: vec!["pleme-io/demo-*".into()],
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
        let p = pool("demo-pool", "pools", PoolSelector::default());
        let a = alloc("manual", "any/repo", "main");
        let members = vec![member("demo-abcd1234", MemberState::Free)];
        let pools = vec![p];
        let d = decide_allocation_reconcile(&a, &pools, |_| &members, Utc::now());
        match d {
            AllocationDecision::Bind {
                pool,
                member_process_name,
            } => {
                assert_eq!(pool.name, "demo-pool");
                assert_eq!(member_process_name, "demo-abcd1234");
            }
            other => panic!("expected Bind, got {other:?}"),
        }
    }

    #[test]
    fn wait_when_pool_full() {
        let p = pool("demo-pool", "pools", PoolSelector::default());
        let a = alloc("manual", "any/repo", "main");
        let members = vec![member("only-one", MemberState::Allocated)];
        let pools = vec![p];
        let d = decide_allocation_reconcile(&a, &pools, |_| &members, Utc::now());
        match d {
            AllocationDecision::Wait { pool } => {
                assert_eq!(pool.name, "demo-pool");
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
        // Fixture ref rides through the ONE substrate composer
        // `AllocationRef::new` — sibling collapse of the pool-side
        // fixture-family struct-literal onto the substrate primitive
        // whose production callers already route through it.
        a.spec.pool_ref = Some(AllocationRef::new("pinned", "pools"));
        let members = vec![member("pinned-aaaa", MemberState::Free)];
        let pools = vec![p];
        let d = decide_allocation_reconcile(&a, &pools, |_| &members, Utc::now());
        assert!(matches!(d, AllocationDecision::Bind { .. }));
    }

    #[test]
    fn bound_with_expiry_in_past_triggers_release() {
        let p = pool("demo-pool", "pools", PoolSelector::default());
        let mut a = alloc("manual", "any/repo", "main");
        let one_hour_ago = Utc::now() - chrono::Duration::hours(1);
        a.status = Some(AllocationStatus {
            phase: AllocationPhase::Bound,
            phase_since: Some(one_hour_ago),
            bound_pool: Some(AllocationRef::new("demo-pool", "pools")),
            assigned_process: Some(AllocationRef::new("demo-abcd", "pools")),
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
                assert_eq!(member_process_name, "demo-abcd");
                assert_eq!(pool.name, "demo-pool");
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

    /// Regression: a `Failed` allocation without a deletion timestamp
    /// MUST short-circuit to `NoOp` rather than fall through to the
    /// routing branch and silently rebind to a fresh pool member. The
    /// open-coded `phase != Released && phase != Bound` gate used to
    /// let this slip through; the typed `is_terminal()` /
    /// `needs_pool_routing()` projection pair closes both arms. Pool
    /// + free member are deliberately available — the decision must
    /// NOT bind regardless.
    #[test]
    fn failed_allocation_is_noop_without_deletion() {
        let p = pool("demo-pool", "pools", PoolSelector::default());
        let mut a = alloc("manual", "any/repo", "main");
        a.status = Some(AllocationStatus {
            phase: AllocationPhase::Failed,
            message: Some("max_size reached; operator must intervene".into()),
            ..Default::default()
        });
        let members = vec![member("demo-fresh", MemberState::Free)];
        let d = decide_allocation_reconcile(&a, &[p], |_| &members, Utc::now());
        assert_eq!(
            d,
            AllocationDecision::NoOp,
            "Failed allocation MUST NOT rebind to a free member",
        );
    }

    /// Regression: a `Releasing` allocation without a deletion
    /// timestamp MUST NOT trigger pool resolution either — the
    /// release ladder is already in flight; matching a new pool
    /// would race the in-flight return. Same typed-projection fix
    /// covers this arm.
    #[test]
    fn releasing_allocation_does_not_rebind_without_deletion() {
        let p = pool("demo-pool", "pools", PoolSelector::default());
        let mut a = alloc("manual", "any/repo", "main");
        a.status = Some(AllocationStatus {
            phase: AllocationPhase::Releasing,
            bound_pool: Some(AllocationRef::new("demo-pool", "pools")),
            assigned_process: Some(AllocationRef::new("demo-old", "pools")),
            ..Default::default()
        });
        let members = vec![member("demo-fresh", MemberState::Free)];
        let d = decide_allocation_reconcile(&a, &[p], |_| &members, Utc::now());
        // Releasing is neither terminal nor routing-eligible AND not
        // Bound, so it falls through to the priority-4 fallback —
        // which now sees matched_pool=None (routing was skipped) and
        // emits NoMatchingPool. The point is it MUST NOT bind to the
        // fresh free member.
        assert!(
            !matches!(d, AllocationDecision::Bind { .. }),
            "Releasing allocation MUST NOT rebind: got {d:?}",
        );
    }

    #[test]
    fn deletion_timestamp_releases_assigned_process() {
        let p = pool("demo-pool", "pools", PoolSelector::default());
        let mut a = alloc("manual", "any/repo", "main");
        // Routes through the ONE substrate composer
        // `tatara_process::time::tombstone_now` — one of 12 pre-lift
        // exact-match sites past the ★★ PRIME-DIRECTIVE ≥ 2 threshold
        // for the `Some(Time(Utc::now()))` wire shape.
        a.metadata.deletion_timestamp = tatara_process::time::tombstone_now();
        a.status = Some(AllocationStatus {
            phase: AllocationPhase::Bound,
            bound_pool: Some(AllocationRef::new("demo-pool", "pools")),
            assigned_process: Some(AllocationRef::new("demo-xyz", "pools")),
            ..Default::default()
        });
        let d = decide_allocation_reconcile(&a, &[p], |_| &[], Utc::now());
        match d {
            AllocationDecision::Release {
                member_process_name,
                ..
            } => {
                assert_eq!(member_process_name, "demo-xyz");
            }
            other => panic!("expected Release on deletion, got {other:?}"),
        }
    }

    // ── Decision split: decide directly from an AllocationConvergenceCtx
    //    literal (no allocation / pools / member closure needed). ──

    fn pool_ref() -> AllocationRef {
        AllocationRef::new("demo-pool", "pools")
    }

    fn pending_ctx(
        matched: Option<AllocationRef>,
        free: Option<String>,
    ) -> AllocationConvergenceCtx {
        AllocationConvergenceCtx {
            phase: AllocationPhase::Pending,
            being_deleted: false,
            expires_at: None,
            assigned_process: None,
            bound_pool: None,
            now: Utc::now(),
            matched_pool: matched,
            free_member: free,
        }
    }

    #[test]
    fn decide_from_ctx_binds_to_free_member() {
        use shigoto_types::decision::Decision;
        let ctx = pending_ctx(Some(pool_ref()), Some("demo-abcd".into()));
        assert_eq!(
            AllocationConvergence::decide(&ctx),
            AllocationDecision::Bind {
                pool: pool_ref(),
                member_process_name: "demo-abcd".into(),
            }
        );
    }

    #[test]
    fn decide_from_ctx_waits_when_pool_full() {
        use shigoto_types::decision::Decision;
        let ctx = pending_ctx(Some(pool_ref()), None);
        assert_eq!(
            AllocationConvergence::decide(&ctx),
            AllocationDecision::Wait { pool: pool_ref() }
        );
    }

    #[test]
    fn decide_from_ctx_no_matching_pool() {
        use shigoto_types::decision::Decision;
        let ctx = pending_ctx(None, None);
        assert_eq!(
            AllocationConvergence::decide(&ctx),
            AllocationDecision::NoMatchingPool
        );
    }

    #[test]
    fn ctx_serde_roundtrips() {
        let ctx = AllocationConvergenceCtx {
            phase: AllocationPhase::Bound,
            being_deleted: false,
            expires_at: Some(Utc::now()),
            assigned_process: Some("demo-xyz".into()),
            bound_pool: Some(pool_ref()),
            now: Utc::now(),
            matched_pool: None,
            free_member: None,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let back: AllocationConvergenceCtx = serde_json::from_str(&json).unwrap();
        assert_eq!(ctx, back);
    }
}
