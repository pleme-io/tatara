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

/// A wall-clock anchor `secs` seconds before the current instant — the
/// one-line `Utc::now() - chrono::Duration::seconds(secs)` chain lifted
/// to ONE typed owner past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// threshold, and the composition partner of every timed-decision test
/// (or production caller) that needs a "recently-past" anchor to feed
/// [`elapsed_since`] or a `humantime`-parsed budget comparator.
///
/// Pre-lift the SAME chain was hand-authored at 21 workspace-wide
/// consumer sites across 6 files, each restating `Utc::now() -
/// chrono::Duration::seconds(<N>)` verbatim to seed a
/// `DateTime<Utc>` `N` seconds in the past:
///
/// * `tatara-process` — 15 sites across `crd.rs` + `lib.rs` +
///   `lifetime_clock.rs` seeding TTL-expiry, staleness-gate, and
///   observed-anchor tests.
/// * `tatara-reconciler::claim` — 4 sites in the stable-name claim
///   arbiter's pure decision tests, each seeding a `granted_at` or
///   `created_at` anchor for tie-break arithmetic.
/// * `tatara-pool-reconciler` — 2 sites in `pool_decide` +
///   `desired.rs` seeding per-member `entered_state_at` /
///   `created_at` for pool-convergence dwell-time decisions.
///
/// All 21 sites walked the SAME two-link chain — read the wall clock,
/// then subtract a whole-second `chrono::Duration` — differing only in
/// the second-count `N` (`age_secs` parameter, `500`, `720`, `42`,
/// etc.). Post-lift each callsite reads `seconds_ago(N)` and the
/// wall-clock read + subtraction sink lives at ONE substrate owner.
///
/// Return-form axis: `DateTime<Utc>` — the copy-form anchor every
/// consumer's downstream `signed_duration_since` / `[`elapsed_since`]`
/// / `Time(anchor)` composer takes as its second operand. The `i64`
/// `secs` parameter matches `chrono::Duration::seconds`'s own signature
/// so a negative value (rare but permitted) yields a future anchor,
/// mirroring the pre-lift semantics.
///
/// Sibling to [`elapsed_since`] on the same `(now, anchor) → Δ` axis —
/// `elapsed_since` reads the delta between two given anchors,
/// `seconds_ago` produces the anchor `N` seconds before now that the
/// delta consumer needs.
///
/// A future normalization (a monotonic-clock cross-check, an injectable
/// `time_source: impl Fn() -> DateTime<Utc>` for deterministic tests,
/// a per-fleet skew Δ that clamps the wall-clock read past a known-bad
/// range) lands at THIS ONE substrate primitive and every downstream
/// consumer (production callers, test helpers, future timed-decision
/// gates) inherits the upgrade mechanically — no per-site edit at any
/// of the 21 listed callers or at future consumers.
#[must_use]
pub fn seconds_ago(secs: i64) -> DateTime<Utc> {
    Utc::now() - chrono::Duration::seconds(secs)
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

    // ─── seconds_ago substrate pins ────────────────────────────────────
    //
    // Bind [`seconds_ago`] at fail-before-pass-after granularity so a
    // regression that flipped the sign (`+` instead of `-`), swapped
    // the unit (`minutes` instead of `seconds`), dropped the wall-clock
    // read to a stale module-load constant, or reshaped the return
    // form surfaces HERE rather than as silent operator-visible drift
    // at the 21 downstream consumers.

    #[test]
    fn seconds_ago_returns_anchor_in_the_past() {
        // Primary shape asserted end-to-end: the returned anchor lies
        // between `before` and `after`, offset back by exactly `secs`.
        // A regression that flipped the sign to `+` would land the
        // anchor in the future and this window check would fail; a
        // regression that swapped the unit (minutes / hours) would
        // land the anchor far outside the sub-second window.
        let secs = 42_i64;
        let before = Utc::now();
        let anchor = seconds_ago(secs);
        let after = Utc::now();
        assert!(
            anchor <= before - chrono::Duration::seconds(secs) + chrono::Duration::milliseconds(50),
            "anchor {anchor} must be ≤ before − {secs}s (within 50ms scheduler jitter)"
        );
        assert!(
            anchor >= after - chrono::Duration::seconds(secs) - chrono::Duration::milliseconds(50),
            "anchor {anchor} must be ≥ after − {secs}s (within 50ms scheduler jitter)"
        );
    }

    #[test]
    fn seconds_ago_composes_with_elapsed_since_at_ttl_gate_shape() {
        // The canonical downstream composition: a consumer seeds an
        // anchor with `seconds_ago(N)` and immediately feeds it to
        // `elapsed_since(Utc::now(), anchor)`, expecting the returned
        // duration to be ~N seconds. A regression that reshaped either
        // primitive so the two no longer round-trip would surface HERE
        // rather than as silent skew at the TTL-expiry gate, the pool
        // staleness gate, or the requeue-budget picker downstream.
        let secs = 30_i64;
        let anchor = seconds_ago(secs);
        let elapsed = elapsed_since(Utc::now(), anchor).expect("elapsed is Some for past anchor");
        assert!(
            elapsed >= Duration::from_secs(secs as u64),
            "elapsed {elapsed:?} must be ≥ {secs}s — the anchor was stamped {secs}s ago"
        );
        assert!(
            elapsed <= Duration::from_secs(secs as u64) + Duration::from_millis(500),
            "elapsed {elapsed:?} must be within 500ms of {secs}s — a wider drift means the primitive is no longer wall-clock reading"
        );
    }

    #[test]
    fn seconds_ago_matches_hand_authored_pre_lift_chain_shape() {
        // Byte-identical parity with the pre-lift `Utc::now() -
        // chrono::Duration::seconds(N)` block that all 21 hand-
        // authored callsites restated verbatim, swept across the four
        // representative second-counts every pre-lift consumer used
        // (small: 5s, medium: 42s, large: 500s, hour-scale: 3600s).
        // Both blocks read the wall clock at DIFFERENT instants so the
        // two anchors CAN differ by the wall-clock delta between
        // calls — bound the divergence at 100ms scheduler jitter.
        for secs in [5_i64, 42, 500, 3_600] {
            let composed = seconds_ago(secs);
            let hand_authored = Utc::now() - chrono::Duration::seconds(secs);
            let delta = (hand_authored - composed).abs();
            assert!(
                delta <= chrono::Duration::milliseconds(100),
                "composed {composed} and hand-authored {hand_authored} must agree within 100ms scheduler jitter for secs={secs}"
            );
        }
    }

    #[test]
    fn seconds_ago_zero_returns_anchor_at_current_instant() {
        // Boundary corner: `secs = 0` yields the current wall-clock
        // instant — the "just-created" moment. A regression that
        // synthesized a small offset (`Duration::from_secs(1)` for
        // clock skew, a per-fleet Δ) would land the anchor 1 second
        // in the past and every zero-age test seed would be off by
        // that offset. Pin the identity so a future normalization
        // has to explicitly move this pin.
        let before = Utc::now();
        let anchor = seconds_ago(0);
        let after = Utc::now();
        assert!(anchor >= before && anchor <= after);
    }

    #[test]
    fn seconds_ago_negative_returns_anchor_in_the_future() {
        // Corner: a negative `secs` yields a future anchor. Matches
        // `chrono::Duration::seconds`'s own signed semantics — a
        // consumer that wants a future-offset anchor (rare, but the
        // few tests that stamp `Utc::now() + chrono::Duration::
        // seconds(...)` for `fallback` construction can route through
        // this primitive with a negative argument). A regression that
        // clamped the negative arm to `Utc::now()` (or panicked) would
        // silently break future callers.
        let anchor = seconds_ago(-10);
        let now = Utc::now();
        assert!(
            anchor >= now,
            "negative secs must yield a future anchor: anchor {anchor} vs now {now}"
        );
        assert!(
            anchor <= now + chrono::Duration::seconds(11),
            "anchor {anchor} must be within (10s + jitter) after now {now}"
        );
    }
}
