//! Wall-clock time primitives — the small typed layer over the
//! `chrono::DateTime<Utc>` → `std::time::Duration` bridge every timed
//! decision in the workspace passes through.
//!
//! Kubernetes exposes wall-clock anchors on the wire as
//! `k8s_openapi::apimachinery::pkg::apis::meta::v1::Time`
//! (`DateTime<Utc>` after `.0`) — `metadata.creationTimestamp`,
//! `status.phaseSince`, `PoolMember.enteredStateAt`, etc. Every timed
//! decision (TTL expiry, sleep-budget picker, staleness gate) then
//! projects `(now, anchor)` onto an `Option<std::time::Duration>` so
//! it can be compared to a `humantime`-parsed budget (also
//! `std::time::Duration`). This module owns the one-line chain that
//! projection reduces to.

use chrono::{DateTime, Utc};
use std::time::Duration;

/// Elapsed wall-clock time between `anchor` and `now`, or `None` if
/// `anchor` is in `now`'s future (a clock rewind or a mis-sequenced
/// anchor). The one-line `now.signed_duration_since(anchor).to_std()
/// .ok()` chain lifted to ONE typed owner past the ★★ PRIME-DIRECTIVE
/// ≥ 2 duplication threshold, and the peer of every timed-decision
/// gate that compares an anchor to a `humantime`-parsed budget.
///
/// Pre-lift the SAME chain was hand-authored at THREE workspace-wide
/// consumer sites, each projecting a `(now, anchor)` pair onto an
/// `Option<std::time::Duration>` for comparison against a
/// `humantime`-parsed TTL/free-TTL:
///
/// * [`crate::lifetime_clock::evaluate`] — the ephemeral-lifetime
///   TTL-expiry gate. Reads
///   `now.signed_duration_since(creation).to_std().ok()` inside the
///   non-terminal-phase guard, fires `AutoTerminate::Now { TtlExpired }`
///   iff the elapsed duration is `>= ttl`.
/// * [`crate::lifetime_clock::requeue_with_ttl`] — the sleep-budget
///   picker for the reconciler's next requeue, choosing the smaller
///   of HEARTBEAT and TTL-remaining so the controller doesn't oversleep
///   past a TTL boundary. Reads the SAME two-link chain via a `match`
///   that maps the `Err` arm onto the caller's `default` fallback.
/// * `tatara-pool-reconciler::pool_decide::decide_pool_reconcile` —
///   the Free-member staleness gate. Reads
///   `now.signed_duration_since(m.entered_state_at).to_std().ok()` per
///   `MemberState::Free` row and pushes the member's process-name onto
///   the stale-Free list iff the elapsed duration exceeds the pool's
///   `free_ttl`.
///
/// All THREE sites walked the SAME two-link chain — take the signed
/// chrono delta, then discard the negative-anchor arm — differing
/// only in the tail (`if let Some(elapsed)` guard, `match` with a
/// per-fn `default` fallback, `if let Some` composed with a per-member
/// push). Post-lift each callsite reads `elapsed_since(now, anchor)`
/// and applies its own tail at its own site.
///
/// Return-form axis: `Option<std::time::Duration>` matches the
/// downstream comparator's type. `humantime::parse_duration` returns
/// `Result<std::time::Duration, _>`, so the elapsed-side projection
/// yielding the SAME `std::time::Duration` puts both operands of the
/// comparator on the same axis without a per-consumer conversion.
///
/// The `None` arm is the "clock ran backwards or the anchor is in the
/// future" corner — a Kubelet clock skew, a `Time` slot stamped with
/// `.0 == Utc::now() + Δ`, or a test that fixes `now` before the
/// anchor to prove the timed decision short-circuits. Every consumer
/// interprets the corner as "no elapsed data → don't fire the timed
/// action"; the pins below bind that shape.
///
/// A future normalization (a monotonic-clock cross-check, a
/// millisecond-precision truncation for cross-node determinism, a
/// per-fleet skew tolerance that bumps a small `Δ` past a negative
/// signed delta before the `to_std().ok()` cast) lands at THIS ONE
/// substrate primitive and every downstream timed-decision consumer
/// inherits the upgrade mechanically — no per-site edit at any of
/// the THREE listed callers or at future consumers (an
/// allocation-TTL expiry gate, a stable-name claim-arbiter age
/// tie-break, a pool member's Allocated-state max-age reap probe).
#[must_use]
pub fn elapsed_since(now: DateTime<Utc>, anchor: DateTime<Utc>) -> Option<Duration> {
    now.signed_duration_since(anchor).to_std().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(secs, 0).expect("valid epoch second")
    }

    #[test]
    fn anchor_before_now_returns_positive_delta() {
        // The canonical shape every consumer rides through — `anchor`
        // stamped in the past, `now` fixed later, the elapsed duration
        // available for comparison against a `humantime`-parsed budget.
        // Pin: the returned duration is exactly the second-count delta
        // between the two anchors, in `std::time::Duration` so the
        // downstream `elapsed >= ttl` / `elapsed > free_ttl` comparator
        // works without a per-consumer conversion.
        let anchor = t(100);
        let now = t(160);
        assert_eq!(elapsed_since(now, anchor), Some(Duration::from_secs(60)));
    }

    #[test]
    fn anchor_equals_now_returns_zero_duration() {
        // Boundary corner: `now == anchor` yields `Some(Duration::ZERO)`
        // rather than `None`. Every consumer needs the "just-stamped"
        // moment to count as elapsed=0 (not as "no elapsed data"): the
        // TTL-expiry gate at `evaluate` fires iff `elapsed >= ttl`, so
        // a zero-ttl ephemeral must expire on its own creation instant
        // — swapping this arm to `None` would silently keep every
        // zero-TTL Process alive.
        let same = t(500);
        assert_eq!(elapsed_since(same, same), Some(Duration::ZERO));
    }

    #[test]
    fn anchor_after_now_returns_none() {
        // The clock-skew / mis-sequenced-anchor corner: `anchor > now`
        // yields `None`. Every consumer interprets `None` as "don't
        // fire the timed action this tick" — the TTL-expiry gate skips
        // the `AutoTerminate::Now` branch, the sleep-budget picker
        // returns the caller's `default`, the pool staleness gate
        // leaves the member off the stale-Free list. A regression that
        // returned a saturating `Duration::ZERO` for this corner would
        // silently fire a zero-TTL ephemeral's expiry the moment its
        // creation timestamp landed one Kubelet clock-skew millisecond
        // ahead of the reconciler's `now`.
        let anchor = t(200);
        let now = t(100);
        assert_eq!(elapsed_since(now, anchor), None);
    }

    #[test]
    fn subsecond_precision_survives_the_to_std_cast() {
        // The `chrono::Duration → std::time::Duration` cast preserves
        // subsecond precision — a regression that silently truncated
        // to whole seconds would compare an "elapsed = 500ms" against
        // a `humantime::parse_duration("1s")` budget as "0s < 1s"
        // rather than "500ms < 1s" and misfire on any decision whose
        // budget straddles a second boundary. Pin the cast at the
        // primitive so a future normalization can't silently drop the
        // subsecond bits.
        let anchor = DateTime::<Utc>::from_timestamp(100, 0).expect("valid epoch second");
        let now = DateTime::<Utc>::from_timestamp(100, 500_000_000).expect("valid epoch nanos");
        assert_eq!(elapsed_since(now, anchor), Some(Duration::from_millis(500)));
    }

    #[test]
    fn one_nanosecond_backwards_returns_none() {
        // The `.to_std().ok()` cast rejects negative chrono deltas by
        // returning `Err` — one nanosecond of backwards skew is enough
        // to reach the `None` arm. Pin the boundary at exactly the
        // point the wire-shape flips so a future normalization that
        // widens the tolerance (a per-fleet skew Δ, a monotonic-clock
        // cross-check) has to move THIS pin rather than silently
        // trampling every consumer's negative-anchor short-circuit.
        let anchor = DateTime::<Utc>::from_timestamp(100, 1).expect("valid epoch nano");
        let now = DateTime::<Utc>::from_timestamp(100, 0).expect("valid epoch second");
        assert_eq!(elapsed_since(now, anchor), None);
    }
}
