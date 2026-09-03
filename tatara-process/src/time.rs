//! Wall-clock time primitives — the small typed layer over the
//! `chrono::DateTime<Utc>` → `std::time::Duration` bridge every timed
//! decision in the workspace passes through, plus the K8s wire-form
//! composers ([`tombstone_now`], [`tombstone_at`]) that lift a
//! `DateTime<Utc>` anchor into the `Option<Time>` shape
//! `ObjectMeta::deletion_timestamp` (and its metadata-Time peers)
//! carry.
//!
//! Kubernetes exposes wall-clock anchors on the wire as
//! `k8s_openapi::apimachinery::pkg::apis::meta::v1::Time`
//! (`DateTime<Utc>` after `.0`) — `metadata.creationTimestamp`,
//! `metadata.deletionTimestamp`, `status.phaseSince`,
//! `PoolMember.enteredStateAt`, etc. Every timed decision (TTL
//! expiry, sleep-budget picker, staleness gate) then projects
//! `(now, anchor)` onto an `Option<std::time::Duration>` so it can
//! be compared to a `humantime`-parsed budget (also
//! `std::time::Duration`). This module owns the one-line chain that
//! projection reduces to on the READ side, and — via [`tombstone_now`]
//! and [`tombstone_at`] — the 5-token `Some(Time(<anchor>))` wire
//! wrap every WRITE-side fixture that seeds a tombstone-present
//! corner stamps on the metadata slot.

use chrono::{DateTime, Utc};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
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

/// A tombstone stamp for a K8s [`metadata.deletionTimestamp`][kdel]
/// slot at the current wall-clock instant — the wire shape K8s
/// stamps once the API server has received a DELETE request but the
/// finalizer chain has not yet released the object for GC. The
/// `Option<Time>` return form matches the slot's own type
/// (`ObjectMeta::deletion_timestamp: Option<Time>`) so the tombstone
/// composes directly into the metadata without a per-caller `Some(...)`
/// wrap or a per-caller `Time(...)` wrap of the `Utc::now()` read.
///
/// Pre-lift the SAME
/// `Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(Utc::now()))`
/// / `Some(Time(chrono::Utc::now()))` 5-token wire shape was hand-
/// authored at 11 workspace-wide fixture sites across four files,
/// each stamping the tombstone slot on one of the three tatara-owned
/// CRDs to seed a deletion-in-progress fixture:
///
/// * [`crate::crd`] `crd::deletion_tombstoned_tests::tombstoned_process`
///   — the shared `Process` fixture the [`crate::crd::Process::is_being_deleted`]
///   inherent-forwarder pin family (7 test cases at `crd.rs` line 4956)
///   destructures for its tombstone-present corner.
/// * [`crate::pool`] `pool::deletion_tombstoned_tests::tombstoned_pool`
///   — the peer `EphemeralPool` fixture the sibling
///   [`crate::pool::EphemeralPool::is_being_deleted`] inherent-forwarder
///   pin family destructures (at `pool.rs` line 2768).
/// * `tatara-pool-reconciler::allocation_decide::tests::
///   deletion_timestamp_releases_assigned_process` — the allocation
///   reconciler's tombstone-releases-bind pin (at `allocation_decide.rs`
///   line 609).
/// * `tatara-pool-reconciler::pool_decide::tests::
///   deletion_stamp_triggers_drain` — the pool reconciler's
///   tombstone-triggers-Drain pin (at `pool_decide.rs` line 343).
/// * [`crate::deletion_tombstoned_tests`] — 7 pins in `lib.rs` (lines
///   1588, 1595, 1602, 1621, 1649, 1663, 1688, 1701) covering the
///   trait's blanket-impl behavior across all three CRDs plus the
///   two inherent-forwarder coherence pins.
///
/// Every callsite walked the SAME 5-token chain — take the wall-clock
/// instant, wrap it in the K8s Time newtype, wrap that in `Some` — and
/// wanted the `Option<Time>` form for direct assignment to the
/// `metadata.deletion_timestamp` slot. Post-lift each callsite reads
/// `tombstone_now()` and the wall-clock read + K8s Time wrap + Option
/// wrap sinks live at ONE substrate owner.
///
/// Return-form axis: `Option<Time>` — the exact type
/// `ObjectMeta::deletion_timestamp` carries. A caller wanting the bare
/// [`Time`] (e.g. seeding a `LastTransitionTime` on a `Condition`,
/// where the field is `Time` and not `Option<Time>`) unwraps via
/// `tombstone_now().unwrap()` at the callsite — but this primitive's
/// contract is the `Option<Time>` slot, matching the pre-lift shape
/// every one of the 11 hand-authored callsites walked. The peer
/// [`tombstone_at`] takes an explicit anchor for callers that need a
/// past-anchored tombstone (e.g. a "stamped an hour ago" fixture for
/// a stale-tombstone garbage-collection probe).
///
/// Peer to [`crate::DeletionTombstoned`] on the (WRITE, READ) axis:
/// [`crate::DeletionTombstoned::is_being_deleted`] is the READ probe
/// (the trait's blanket impl reads `.metadata.deletion_timestamp.
/// is_some()` on any tatara CRD); [`tombstone_now`] is the WRITE
/// composer (the substrate owner for the 5-token wire shape every
/// fixture that seeds a tombstone-present corner stamps). The two
/// primitives partition the deletion-timestamp surface at the (read,
/// write) axis and cover it end-to-end at the substrate.
///
/// A future normalization (a monotonic-clock cross-check on the wall
/// read, a per-fleet skew Δ that biases the tombstone anchor past a
/// known-bad range, a widening of the K8s Time wire form under a
/// future k8s-openapi crate bump, a debug-build assertion that the
/// caller has admission privileges to stamp a tombstone at all) lands
/// at THIS ONE substrate primitive and every downstream fixture / seed
/// / stamp callsite inherits the upgrade mechanically — no per-site
/// edit at any of the 11 listed callers or at future consumers (a
/// stable-name claim-arbiter's tombstoned-generation seed, a
/// tatara-testing helper that stamps a tombstone on a mock-server
/// object, an admission-webhook fixture that fires the tombstone
/// stamp itself).
///
/// [kdel]: https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.30/#objectmeta-v1-meta
#[must_use]
pub fn tombstone_now() -> Option<Time> {
    Some(Time(Utc::now()))
}

/// A tombstone stamp for a K8s [`metadata.deletionTimestamp`][kdel]
/// slot at the operator-supplied `when` anchor — the peer of
/// [`tombstone_now`] on the anchor-explicit axis. Composes directly
/// with [`seconds_ago`] so a callsite needing a "stamped `N` seconds
/// ago" tombstone (e.g. a stale-tombstone garbage-collection probe, a
/// fixture that seeds a tombstone predating the reconciler's `now` by
/// enough to trip a `deletion_grace_period_seconds` cutoff) reads
/// `tombstone_at(seconds_ago(N))` and routes through ONE substrate
/// owner for both the anchor construction and the wire-form wrap.
///
/// Pre-lift the SAME `Some(Time(<anchor>))` wire shape was hand-
/// authored at 1 workspace-wide site — the tombstone-present corner
/// of [`crate::deletion_tombstoned_tests::is_being_deleted_matches_pre_lift_deletion_timestamp_is_some_chain_on_ephemeral_allocation`],
/// which sweeps three corners of the (absent, present-at-now,
/// present-at-past) input matrix and stamps `Some(Time(seconds_ago(3600)))`
/// on the present-at-past corner. Together with [`tombstone_now`]'s
/// 11 callsites, the pair covers the 12-site `Some(Time(<anchor>))`
/// family the substrate opens ownership over.
///
/// The `DateTime<Utc>` parameter form encodes the invariant "the caller
/// has already chosen the anchor" at the type level — a caller wanting
/// the current-instant tombstone routes through [`tombstone_now`]
/// instead of `tombstone_at(Utc::now())`, keeping the wall-clock read
/// at ONE substrate owner and avoiding the "did the caller mean the
/// clock at the seed-instant or the clock at the assertion-instant"
/// ambiguity a `DateTime<Utc>::default()` form would open.
///
/// A future normalization at the wire form (see the doc-comment on
/// [`tombstone_now`] for the full rationale) lands at THIS primitive
/// alongside [`tombstone_now`] so both anchor shapes inherit the
/// upgrade mechanically at the same substrate site.
///
/// [kdel]: https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.30/#objectmeta-v1-meta
#[must_use]
pub fn tombstone_at(when: DateTime<Utc>) -> Option<Time> {
    Some(Time(when))
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

    // ─── tombstone_now + tombstone_at substrate pins ──────────────────
    //
    // Bind the two K8s-wire tombstone composers at fail-before-pass-
    // after granularity so a regression that dropped the `Some` wrap
    // (yielding `Option<Time>` = `None`, which would silently un-
    // tombstone every fixture), swapped the `Time` newtype for a raw
    // `DateTime<Utc>` (breaking the `metadata.deletion_timestamp: Option<Time>`
    // slot's shape), or diverged the two composers on the anchor axis
    // (a `tombstone_at(when)` that ignored `when` and read the wall
    // clock, an anchor-invariant that clamped a future anchor into
    // the past) surfaces HERE rather than as silent operator-facing
    // skew at the 12 downstream fixture consumers.
    //
    // Each pin is fail-before-pass-after: the primitives did not exist
    // pre-lift, so any test that invokes them fails to compile pre-
    // lift and passes post-lift; the byte-identity pins below then
    // bind the specific shape choice.

    #[test]
    fn tombstone_now_returns_some_time_at_current_instant() {
        // Primary shape asserted end-to-end: the returned option is
        // `Some(Time(anchor))` with the anchor bracketed by two
        // wall-clock reads taken immediately before + after the call.
        // A regression that dropped the `Some` wrap would fail the
        // outer `is_some()` probe; a regression that stamped a
        // constant (module-load `Utc::now()`, a `DateTime::<Utc>::
        // default()` = epoch) would fail the bracket check.
        let before = Utc::now();
        let stamp = tombstone_now();
        let after = Utc::now();
        let stamped = stamp.expect("tombstone_now must return Some(Time(...))");
        assert!(
            stamped.0 >= before && stamped.0 <= after,
            "tombstone anchor {} must fall in [{before}, {after}]",
            stamped.0,
        );
    }

    #[test]
    fn tombstone_now_matches_hand_authored_pre_lift_chain_shape() {
        // Byte-identical parity with the pre-lift
        // `Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(
        // Utc::now()))` block that all 11 hand-authored fixture sites
        // restated verbatim (differing only in `chrono::Utc` vs `Utc`
        // module-path prefix). Both blocks read the wall clock at
        // DIFFERENT instants so the two anchors CAN differ by the
        // wall-clock delta between calls — bound the divergence at
        // 100ms scheduler jitter, matching the peer
        // `seconds_ago_matches_hand_authored_pre_lift_chain_shape`
        // pin's tolerance.
        let composed = tombstone_now().expect("tombstone_now returns Some");
        let hand_authored = Some(Time(Utc::now())).expect("hand-authored fixture");
        let delta = (hand_authored.0 - composed.0).abs();
        assert!(
            delta <= chrono::Duration::milliseconds(100),
            "composed {} and hand-authored {} must agree within 100ms scheduler jitter",
            composed.0,
            hand_authored.0,
        );
    }

    #[test]
    fn tombstone_at_returns_some_time_preserving_the_operator_anchor() {
        // Primary shape for the anchor-explicit peer: the returned
        // option is `Some(Time(when))` and the anchor is exactly the
        // `when` argument — no wall-clock read, no normalization, no
        // clamp. A regression that fell through to the current instant
        // (`tombstone_at` ignoring `when` and re-reading the wall
        // clock) would fail the identity check.
        let epoch = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).expect("valid epoch second");
        let stamp = tombstone_at(epoch).expect("tombstone_at returns Some");
        assert_eq!(stamp.0, epoch, "anchor must be preserved verbatim");
    }

    #[test]
    fn tombstone_at_composes_with_seconds_ago_at_stale_fixture_shape() {
        // The canonical downstream composition: a fixture that needs a
        // "stamped N seconds ago" tombstone composes `tombstone_at(
        // seconds_ago(N))` and expects the returned anchor to be ~N
        // seconds in the past. Matches the single pre-lift site at
        // `lib.rs::deletion_tombstoned_tests::is_being_deleted_matches_pre_lift_deletion_timestamp_is_some_chain_on_ephemeral_allocation`
        // which stamps `Some(Time(crate::time::seconds_ago(3600)))`.
        // A regression that reshaped either primitive so the two no
        // longer round-trip would surface HERE rather than as silent
        // skew at the stale-tombstone fixture family.
        let secs = 3_600_i64;
        let anchor = seconds_ago(secs);
        let stamp = tombstone_at(anchor).expect("tombstone_at returns Some");
        assert_eq!(
            stamp.0, anchor,
            "tombstone_at must preserve the seconds_ago-produced anchor verbatim",
        );
        // And the anchor is ~N seconds in the past — this is the
        // downstream property every fixture using the composition
        // relies on.
        let elapsed = elapsed_since(Utc::now(), stamp.0).expect("elapsed is Some for past anchor");
        assert!(
            elapsed >= Duration::from_secs(secs as u64),
            "elapsed {elapsed:?} must be ≥ {secs}s — the anchor was stamped {secs}s ago",
        );
    }

    #[test]
    fn tombstone_now_and_tombstone_at_agree_at_the_current_instant() {
        // Cross-composer coherence pin: `tombstone_now()` and
        // `tombstone_at(Utc::now())` produce the SAME shape (`Some(
        // Time(...))`) with anchors that agree within scheduler jitter.
        // A future refactor that consolidated one composer onto the
        // other (or split them further) cannot land any anchor-axis
        // drift because this pin binds them at the current-instant
        // corner where both callsites converge.
        let a = tombstone_now().expect("tombstone_now returns Some");
        let b = tombstone_at(Utc::now()).expect("tombstone_at returns Some");
        let delta = (b.0 - a.0).abs();
        assert!(
            delta <= chrono::Duration::milliseconds(100),
            "tombstone_now anchor {} and tombstone_at(Utc::now()) anchor {} must agree within 100ms scheduler jitter",
            a.0,
            b.0,
        );
    }

    #[test]
    fn tombstone_at_preserves_a_future_anchor_without_clamping() {
        // Corner: `tombstone_at` accepts a future anchor verbatim —
        // matches the pre-lift `Some(Time(<future>))` shape a caller
        // wanting a future-offset tombstone would hand-author. Pin the
        // identity so a future normalization that clamps the anchor
        // into the past (a "no tombstone can be in the future" policy)
        // has to explicitly move this pin rather than silently
        // trampling future callers.
        let future = Utc::now() + chrono::Duration::seconds(3_600);
        let stamp = tombstone_at(future).expect("tombstone_at returns Some");
        assert_eq!(stamp.0, future);
    }
}
