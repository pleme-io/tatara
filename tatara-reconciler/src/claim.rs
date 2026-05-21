//! Stable-name claim arbiter — pure decision functions over the
//! ProcessTable claim registry.
//!
//! Invariant: at most one Process per `(cluster, app)` holds the
//! stable-form DNS name at any moment. Arbitration rule:
//!
//! 1. Only Processes whose phase is in `LIVE_PHASES_FOR_CLAIM` are
//!    eligible candidates (Running, Attested).
//! 2. Only Processes whose `routing.stable_name_claim == true` for
//!    the (cluster, app) tuple are candidates.
//! 3. Higher [`RoutingSpec.priority`] wins.
//! 4. Ties broken by oldest `metadata.creationTimestamp` (stability:
//!    long-lived holders shouldn't lose to brand-new same-priority
//!    Processes).
//! 5. Further ties broken by `${namespace}/${name}` ASCII order
//!    (deterministic across reconciler restarts).
//!
//! The async controller layer (`table_controller`) calls
//! [`decide_claim_for`] every reconcile of the ProcessTable + applies
//! the resulting [`ClaimDecision`] via a status PATCH.

use chrono::{DateTime, Utc};

use tatara_process::phase::ProcessPhase;
use tatara_process::prelude::Process;
use tatara_process::table::ClaimRecord;

/// Phases in which a Process is eligible to hold (or take over) a
/// stable-name claim. Anything else (Pending/Forking/Execing/
/// Reconverging/Releasing/Exiting/Failed/Zombie/Reaped) is
/// disqualified: traffic must not be routed to a Process that isn't
/// answering yet or is on the way out.
pub const LIVE_PHASES_FOR_CLAIM: &[ProcessPhase] =
    &[ProcessPhase::Running, ProcessPhase::Attested];

/// One arbitration outcome for a single `(cluster, app)` key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimDecision {
    /// Hold steady — current holder remains.
    Hold,
    /// Transfer the claim to `next` (a typed `ClaimRecord` ready to
    /// be written to `ProcessTable.status.claims[key]`). Applies
    /// when current holder failed OR a higher-priority candidate
    /// emerged OR there was no holder.
    Transfer { next: ClaimRecord },
    /// Vacate — remove the entry from `ProcessTable.status.claims`.
    /// No eligible candidate exists; the stable DNS form should not
    /// be emitted by anyone until a candidate appears.
    Vacate,
}

/// A candidate Process for a stable-name claim. Built by the caller
/// from a list of Processes filtered by routing.stableNameClaim ==
/// true for the same `(cluster, app)` tuple.
#[derive(Clone, Debug)]
pub struct Candidate<'a> {
    /// `${ns}/${name}` — used as the `holder` field of ClaimRecord.
    pub process_ref: String,
    /// PID for `ClaimRecord.pid`.
    pub pid: String,
    /// Declared priority from `RoutingSpec.priority`.
    pub priority: i32,
    /// Current phase. Used to filter by eligibility before
    /// arbitration.
    pub phase: ProcessPhase,
    /// `metadata.creationTimestamp` for tie-breaking.
    pub created_at: DateTime<Utc>,
    /// Backing Process — only borrowed; the decision doesn't keep
    /// it after the call.
    pub _process: &'a Process,
}

impl<'a> Candidate<'a> {
    /// True iff this candidate is currently in a phase that may
    /// hold a claim.
    pub fn is_live(&self) -> bool {
        LIVE_PHASES_FOR_CLAIM.contains(&self.phase)
    }

    /// True iff this candidate's identity (process_ref + pid)
    /// matches the given holder record.
    pub fn matches_holder(&self, holder: &ClaimRecord) -> bool {
        self.process_ref == holder.holder && self.pid == holder.pid
    }
}

/// Decide the next state of a single stable-name claim entry.
///
/// `key` is the `(cluster, app)` composite (e.g. `"pleme-dev/gator"`)
/// — passed for logging clarity; the decision function only uses it
/// implicitly via the filtered `candidates` slice.
///
/// `current` is the existing entry in `ProcessTable.status.claims[key]`,
/// or `None` if no holder currently exists.
///
/// `candidates` is the set of Processes that have declared
/// `stable_name_claim == true` for this `(cluster, app)`. Eligibility
/// filtering (live phases) happens inside [`decide_claim_for`]; the
/// caller does not need to pre-filter.
///
/// `now` is the timestamp stamped on a new `granted_at` if the
/// decision is `Transfer`.
pub fn decide_claim_for(
    key: &str,
    current: Option<&ClaimRecord>,
    candidates: &[Candidate<'_>],
    now: DateTime<Utc>,
) -> ClaimDecision {
    let _ = key; // reserved for telemetry; doesn't affect the decision

    // 1. Filter to live candidates.
    let live: Vec<&Candidate<'_>> = candidates.iter().filter(|c| c.is_live()).collect();
    if live.is_empty() {
        return match current {
            Some(_) => ClaimDecision::Vacate,
            None => ClaimDecision::Vacate, // already absent; no-op on the wire
        };
    }

    // 2. Pick the highest-priority, oldest, lexicographically-first
    //    live candidate.
    let winner = best_candidate(&live);

    // 3. Compare to current holder.
    match current {
        Some(holder) if winner.matches_holder(holder) => ClaimDecision::Hold,
        _ => ClaimDecision::Transfer {
            next: ClaimRecord {
                holder: winner.process_ref.clone(),
                pid: winner.pid.clone(),
                granted_at: now,
                priority: winner.priority,
            },
        },
    }
}

/// Pure tiebreak — kept exposed for tests + future overrides.
/// Compares two candidates and returns the winner. Symmetric +
/// transitive (totally ordered).
fn best_candidate<'a, 'b>(live: &'b [&'a Candidate<'a>]) -> &'a Candidate<'a> {
    live.iter()
        .copied()
        .reduce(|a, b| {
            // (a) priority desc
            if a.priority != b.priority {
                return if a.priority > b.priority { a } else { b };
            }
            // (b) oldest creation asc
            if a.created_at != b.created_at {
                return if a.created_at < b.created_at { a } else { b };
            }
            // (c) process_ref ASCII asc (deterministic across restarts)
            if a.process_ref < b.process_ref {
                a
            } else {
                b
            }
        })
        .expect("non-empty (caller filtered)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_process::classification::{
        Classification, ConvergencePointType, SubstrateType,
    };
    use tatara_process::crd::{ProcessSpec, ProcessStatus};

    fn candidate<'a>(
        proc: &'a Process,
        priority: i32,
        phase: ProcessPhase,
        age_secs: i64,
    ) -> Candidate<'a> {
        let created_at = Utc::now() - chrono::Duration::seconds(age_secs);
        Candidate {
            process_ref: format!(
                "{}/{}",
                proc.metadata.namespace.as_deref().unwrap_or("ns"),
                proc.metadata.name.as_deref().unwrap_or("n")
            ),
            pid: format!("pid-{}", proc.metadata.name.as_deref().unwrap_or("n")),
            priority,
            phase,
            created_at,
            _process: proc,
        }
    }

    fn empty_process(name: &str, ns: &str) -> Process {
        let spec = ProcessSpec {
            identity: Default::default(),
            classification: Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
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
        let mut p = Process::new(name, spec);
        p.metadata.namespace = Some(ns.into());
        p.status = Some(ProcessStatus::default());
        p
    }

    #[test]
    fn no_candidates_vacates() {
        let d = decide_claim_for("pleme-dev/gator", None, &[], Utc::now());
        assert_eq!(d, ClaimDecision::Vacate);
    }

    #[test]
    fn no_live_candidates_vacates() {
        let p = empty_process("a", "ns");
        let c = vec![candidate(&p, 0, ProcessPhase::Failed, 0)];
        let d = decide_claim_for("k", None, &c, Utc::now());
        assert_eq!(d, ClaimDecision::Vacate);
    }

    #[test]
    fn first_candidate_takes_empty_slot() {
        let p = empty_process("a", "ns");
        let c = vec![candidate(&p, 100, ProcessPhase::Running, 60)];
        let now = Utc::now();
        let d = decide_claim_for("k", None, &c, now);
        match d {
            ClaimDecision::Transfer { next } => {
                assert_eq!(next.holder, "ns/a");
                assert_eq!(next.pid, "pid-a");
                assert_eq!(next.priority, 100);
                assert_eq!(next.granted_at, now);
            }
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn holder_stays_when_still_live() {
        let p = empty_process("a", "ns");
        let c = vec![candidate(&p, 100, ProcessPhase::Attested, 1000)];
        let current = ClaimRecord {
            holder: "ns/a".into(),
            pid: "pid-a".into(),
            granted_at: Utc::now() - chrono::Duration::seconds(500),
            priority: 100,
        };
        let d = decide_claim_for("k", Some(&current), &c, Utc::now());
        assert_eq!(d, ClaimDecision::Hold);
    }

    #[test]
    fn holder_replaced_when_failed() {
        // Current holder is Failed → no longer live → replaced by
        // any live candidate even at lower priority.
        let p_failed = empty_process("a", "ns");
        let p_live = empty_process("b", "ns");
        let c = vec![
            candidate(&p_failed, 100, ProcessPhase::Failed, 1000),
            candidate(&p_live, 0, ProcessPhase::Running, 60),
        ];
        let current = ClaimRecord {
            holder: "ns/a".into(),
            pid: "pid-a".into(),
            granted_at: Utc::now() - chrono::Duration::seconds(500),
            priority: 100,
        };
        let d = decide_claim_for("k", Some(&current), &c, Utc::now());
        match d {
            ClaimDecision::Transfer { next } => assert_eq!(next.holder, "ns/b"),
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn higher_priority_wins_over_holder() {
        let p_low = empty_process("a", "ns");
        let p_high = empty_process("b", "ns");
        let c = vec![
            candidate(&p_low, 50, ProcessPhase::Running, 1000),
            candidate(&p_high, 100, ProcessPhase::Running, 60),
        ];
        let current = ClaimRecord {
            holder: "ns/a".into(),
            pid: "pid-a".into(),
            granted_at: Utc::now() - chrono::Duration::seconds(500),
            priority: 50,
        };
        let d = decide_claim_for("k", Some(&current), &c, Utc::now());
        match d {
            ClaimDecision::Transfer { next } => {
                assert_eq!(next.holder, "ns/b");
                assert_eq!(next.priority, 100);
            }
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn ties_broken_by_oldest_creation() {
        let p_old = empty_process("z", "ns"); // lexicographically last
        let p_new = empty_process("a", "ns"); // lexicographically first
        let c = vec![
            candidate(&p_old, 100, ProcessPhase::Running, 1000), // older
            candidate(&p_new, 100, ProcessPhase::Running, 60),
        ];
        let d = decide_claim_for("k", None, &c, Utc::now());
        match d {
            // Same priority → oldest wins, regardless of name.
            ClaimDecision::Transfer { next } => assert_eq!(next.holder, "ns/z"),
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn ties_broken_lexicographically_after_age() {
        let p_a = empty_process("a", "ns");
        let p_b = empty_process("b", "ns");
        // Same priority + identical creation timestamp.
        let now = Utc::now();
        let c = vec![
            Candidate {
                process_ref: "ns/b".into(),
                pid: "pid-b".into(),
                priority: 100,
                phase: ProcessPhase::Running,
                created_at: now,
                _process: &p_b,
            },
            Candidate {
                process_ref: "ns/a".into(),
                pid: "pid-a".into(),
                priority: 100,
                phase: ProcessPhase::Running,
                created_at: now,
                _process: &p_a,
            },
        ];
        let d = decide_claim_for("k", None, &c, now);
        match d {
            ClaimDecision::Transfer { next } => assert_eq!(next.holder, "ns/a"),
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn negative_priority_yields_to_zero() {
        let p_yield = empty_process("a", "ns");
        let p_active = empty_process("b", "ns");
        let c = vec![
            candidate(&p_yield, -10, ProcessPhase::Running, 1000),
            candidate(&p_active, 0, ProcessPhase::Running, 60),
        ];
        let d = decide_claim_for("k", None, &c, Utc::now());
        match d {
            ClaimDecision::Transfer { next } => {
                assert_eq!(next.holder, "ns/b");
                assert_eq!(next.priority, 0);
            }
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn execing_candidate_disqualified() {
        // Process in Execing phase declares the claim but isn't
        // serving traffic yet — must NOT receive the stable claim.
        let p_execing = empty_process("a", "ns");
        let p_running = empty_process("b", "ns");
        let c = vec![
            candidate(&p_execing, 100, ProcessPhase::Execing, 1000),
            candidate(&p_running, 0, ProcessPhase::Running, 60),
        ];
        let d = decide_claim_for("k", None, &c, Utc::now());
        match d {
            ClaimDecision::Transfer { next } => assert_eq!(next.holder, "ns/b"),
            other => panic!("expected Transfer, got {other:?}"),
        }
    }

    #[test]
    fn live_phases_set_is_exhaustive_for_serving() {
        // Document the invariant: exactly Running + Attested are
        // claim-eligible. If this set ever changes, the
        // disqualification tests above need re-examination.
        assert_eq!(
            LIVE_PHASES_FOR_CLAIM,
            &[ProcessPhase::Running, ProcessPhase::Attested]
        );
    }
}
