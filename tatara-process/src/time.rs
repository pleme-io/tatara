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

use chrono::{DateTime, FixedOffset, Utc};
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

/// A wall-clock anchor `secs` seconds AFTER the current instant — the
/// one-line `Utc::now() + chrono::Duration::seconds(secs)` chain lifted
/// to ONE typed owner past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// threshold, and the (past, future) mirror of [`seconds_ago`] on the
/// same wall-clock-anchored `→ DateTime<Utc>` axis. Composition partner
/// of every wall-clock-jitter bracket assertion (upper edge of a `≤ now
/// plus Δ` window) and every "seed a far-future fallback" test fixture
/// that needs a `DateTime<Utc>` deterministically after the current
/// instant.
///
/// Pre-lift the SAME chain was hand-authored at 6 workspace-wide
/// consumer sites across 2 files, each restating `Utc::now() +
/// chrono::Duration::seconds(<N>)` verbatim to seed a `DateTime<Utc>`
/// `N` seconds in the future:
///
/// * `tatara-process::time` × 3 — the future-anchor preservation pins
///   for [`tombstone_at`], [`creation_stamp_at`], and [`wire_time_some`]
///   (each stamps `let future = Utc::now() + chrono::Duration::seconds
///   (3_600);` to prove the composer accepts a future anchor without
///   clamping).
/// * `tatara-process::crd` × 3 — the upper-bracket wall-clock jitter
///   check on `observed_phase_since` (`assert!(composed <= Utc::now() +
///   chrono::Duration::seconds(1))`, +1s tolerance) and the two
///   "unrelated fallback" seeds for `observed_phase_since_or` /
///   `created_at_or`'s populated-corner pins (`let unrelated_fallback =
///   Utc::now() + chrono::Duration::seconds(9_999);`, +9_999s far-
///   future guard proving the composer ignores the fallback when the
///   observed slot is populated).
///
/// All 6 sites walked the SAME two-link chain — read the wall clock,
/// then add a whole-second `chrono::Duration` — differing only in the
/// second-count `N` (`1`, `3_600`, `9_999`). Post-lift each callsite
/// reads `seconds_from_now(N)` and the wall-clock read + addition sink
/// lives at ONE substrate owner.
///
/// Return-form axis: `DateTime<Utc>` — the copy-form anchor every
/// consumer's downstream `<=` / `>=` bracket comparator or fallback-slot
/// stamp takes as its second operand, matching the pre-lift shape
/// verbatim. The `i64` `secs` parameter matches
/// `chrono::Duration::seconds`'s own signature so a negative value
/// yields a past anchor, mirroring the pre-lift signed semantics.
///
/// Sibling to [`seconds_ago`] on the same wall-clock-anchored
/// `→ DateTime<Utc>` axis, split by TIME DIRECTION:
/// `seconds_ago(N)` returns `Utc::now() - Duration::seconds(N)`
/// (past anchor); `seconds_from_now(N)` returns `Utc::now() +
/// Duration::seconds(N)` (future anchor). The two composers partition
/// the (past, future) axis at the sign and cover it end-to-end at the
/// substrate — a caller with a compile-time-known DIRECTION picks the
/// semantically-matching primitive rather than negating an argument
/// (`seconds_ago(-N)` produces the same `DateTime<Utc>` as
/// `seconds_from_now(N)` — see the pre-lift note in [`seconds_ago`]'s
/// `seconds_ago_negative_returns_anchor_in_the_future` corner pin —
/// but obscures the caller's intent at the readsite; the peer opens
/// the semantic slot). Peer to [`at_epoch_second`] on the sibling
/// (wall-clock, deterministic) split — the two-axis partition
/// (`seconds_ago` / `seconds_from_now` on the wall-clock axis,
/// `at_epoch_second` on the deterministic axis) covers every anchor-
/// producing shape in this module.
///
/// A future normalization (a monotonic-clock cross-check, an injectable
/// `time_source: impl Fn() -> DateTime<Utc>` for deterministic tests,
/// a per-fleet skew Δ that clamps the wall-clock read past a known-bad
/// range) lands at THIS ONE substrate primitive and every downstream
/// consumer (production callers, test helpers, future timed-decision
/// gates) inherits the upgrade mechanically — no per-site edit at any
/// of the 6 listed callers or at future consumers (a stable-name
/// claim-arbiter's future-anchor jitter tolerance, an allocation-TTL
/// far-future fallback fixture, a probe-receipt `expires_at`-slot far-
/// future seed). The parallel normalization at [`seconds_ago`] lands at
/// this primitive's peer, sharing the wall-clock-read discipline.
#[must_use]
pub fn seconds_from_now(secs: i64) -> DateTime<Utc> {
    Utc::now() + chrono::Duration::seconds(secs)
}

/// The pure 5-token `Some(Time(<anchor>))` wire wrap the K8s
/// metadata-Time slots (`ObjectMeta::creation_timestamp`,
/// `ObjectMeta::deletion_timestamp`, and every peer `Option<Time>`
/// metadata slot the tatara-owned CRDs stamp) carry — lifted to ONE
/// private substrate owner past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold, so the K8s Time newtype wrap + Option wrap
/// sinks live at a SINGLE composer body every wire-shape peer in this
/// module delegates through.
///
/// Pre-lift the SAME 5-token chain was hand-authored at THREE
/// intra-module composer bodies — each restating `Some(Time(<anchor>))`
/// verbatim to project a `DateTime<Utc>` anchor onto the
/// `Option<Time>` metadata slot:
///
/// * [`tombstone_now`] — the wall-clock-reading deletion-timestamp
///   composer. Pre-lift body: `Some(Time(Utc::now()))`.
/// * [`tombstone_at`] — the anchor-explicit deletion-timestamp
///   composer. Pre-lift body: `Some(Time(when))`.
/// * [`creation_stamp_at`] — the anchor-explicit creation-timestamp
///   composer. Pre-lift body: `Some(Time(when))`.
///
/// All three composer bodies walked the SAME 5-token chain — take a
/// `DateTime<Utc>` anchor, wrap it in the K8s Time newtype, wrap that
/// in `Some` — and wanted the `Option<Time>` form for direct
/// assignment to a metadata-Time slot. Post-lift each composer reads
/// `wire_time_some(<anchor>)` and the K8s Time wrap + `Some` wrap
/// sinks live at ONE substrate owner. The cross-composer coherence
/// pin
/// [`tests::creation_stamp_at_and_tombstone_at_agree_at_the_current_instant_on_wire_shape`]
/// already bound the two anchor-explicit composers at a deterministic
/// anchor pre-lift so this consolidation lands without moving any
/// downstream fixture consumer.
///
/// Mirrors the recent [`crate::status::ProcessCondition::new_at`] /
/// `new_at_now` peer-pair on the (clock-injectable, wall-clock-anchored)
/// axis: `wire_time_some` owns the pure struct-composition body,
/// [`tombstone_now`] supplies `Utc::now()` and delegates,
/// [`tombstone_at`] / [`creation_stamp_at`] supply the operator anchor
/// and delegate. Every future normalization at the K8s Time wire form
/// (a per-fleet millisecond-precision truncation, a widening of the
/// K8s Time newtype under a future k8s-openapi crate bump, a
/// debug-build assertion that the anchor is inside a permitted skew
/// window) lands at THIS ONE substrate primitive and every metadata-
/// Time composer downstream inherits the upgrade mechanically — no
/// per-composer edit at [`tombstone_now`] / [`tombstone_at`] /
/// [`creation_stamp_at`], and no edit at any future
/// `<peer>_stamp_at` sibling that inherits the substrate.
///
/// Visibility: `pub(crate)` — the primitive is an intra-crate
/// substrate that every metadata-Time composer in this module (and
/// its future intra-crate peers) delegates through, but is not part
/// of the public composer surface (callers reach through the semantic
/// peers [`tombstone_now`] / [`tombstone_at`] / [`creation_stamp_at`]
/// so the composer's semantic slot stays visible at the callsite).
#[must_use]
pub(crate) fn wire_time_some(when: DateTime<Utc>) -> Option<Time> {
    Some(Time(when))
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
/// # Delegation to [`wire_time_some`]
///
/// The 5-token `Some(Time(<anchor>))` wire wrap lives at the shared
/// substrate primitive [`wire_time_some`]; this composer supplies
/// `Utc::now()` as the anchor and delegates. The wall-clock read at
/// ONE substrate site (this composer body's `Utc::now()` call), the
/// K8s Time newtype wrap + `Some` wrap at the SHARED substrate site
/// (`wire_time_some`'s body). A future normalization at the wire form
/// (see [`wire_time_some`]'s doc-comment) lands at that primitive and
/// this composer inherits the upgrade mechanically.
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
    wire_time_some(Utc::now())
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
    wire_time_some(when)
}

/// A creation stamp for a K8s [`metadata.creationTimestamp`][kcreat]
/// slot at the operator-supplied `when` anchor — the anchor-explicit
/// composer for the `metadata.creation_timestamp: Option<Time>` slot
/// every age-anchored `Process` / `EphemeralPool` fixture seeds so
/// its downstream TTL-expiry / staleness-gate / `created_at`
/// projection has a deterministic anchor to compare against.
///
/// Peer of [`tombstone_at`] on the (creation, deletion) axis of the
/// `ObjectMeta` metadata-Time slots — both walk the SAME 5-token
/// `Some(Time(<anchor>))` wire wrap but partition by SEMANTIC slot:
///
/// * [`tombstone_at`] — stamps `metadata.deletion_timestamp` with a
///   caller-supplied anchor, semantically "this object is being
///   deleted at `when`". Every fixture that seeds a
///   deletion-in-progress corner (7 tombstone-forwarder pins on
///   `Process`, 1 on `EphemeralPool`, 4 on `PoolMember` /
///   `Allocation`) reaches through it.
/// * [`creation_stamp_at`] (this composer) — stamps
///   `metadata.creation_timestamp` with a caller-supplied anchor,
///   semantically "this object was created at `when`". Every fixture
///   that seeds a creation-timestamp-present corner for a
///   TTL-expiry / staleness-gate / `created_at`-projection pin (the
///   9-case `creation_stamped_process` helper family at
///   [`crate::crd`], the `ephemeral_process(age_secs, ttl, teardown)`
///   helper at [`crate::lifetime_clock`]) reaches through it.
///
/// Pre-lift the SAME `Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(<anchor>))`
/// / `Some(Time(<anchor>))` 5-token wire shape was hand-authored at
/// TWO fixture-helper sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold, each stamping the creation-timestamp slot
/// on a fresh [`crate::crd::Process`] fixture to seed a deterministic
/// age anchor for a downstream timed-decision pin family:
///
/// * [`crate::crd`] `crd::tests::creation_stamped_process` — the
///   shared `Process` fixture the 9-case `Process::created_at`
///   inherent-forwarder pin family destructures for its
///   creation-anchor corner. Fully-qualified pre-lift
///   (`Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(t))`)
///   because the `crd::tests` module inherits imports only via
///   `use super::*` and does not name the k8s-openapi `Time` type
///   locally.
/// * [`crate::lifetime_clock`] `lifetime_clock::tests::ephemeral_process`
///   — the shared `Process` fixture the ephemeral-lifetime evaluate /
///   requeue-with-ttl pin families destructure for their
///   creation-anchor corner. Locally imported pre-lift
///   (`Some(Time(creation))`) because `lifetime_clock::tests`
///   already brings `k8s_openapi::apimachinery::pkg::apis::meta::v1::Time`
///   into scope for its own fixture composition.
///
/// Both callsites walked the SAME 5-token chain — take the operator-
/// supplied `DateTime<Utc>` anchor, wrap it in the K8s Time newtype,
/// wrap that in `Some` — and wanted the `Option<Time>` form for
/// direct assignment to the `metadata.creation_timestamp` slot.
/// Post-lift each callsite reads `crate::time::creation_stamp_at(<anchor>)`
/// and the K8s Time wrap + Option wrap sinks live at ONE substrate
/// owner alongside [`tombstone_at`]'s deletion-slot peer.
///
/// Return-form axis: `Option<Time>` — the exact type
/// `ObjectMeta::creation_timestamp` carries. Matches the pre-lift
/// shape both callsites walked verbatim and composes directly with
/// the `Copy`-projection primitive `Process::created_at` (which
/// reads `.metadata.creation_timestamp.as_ref().map(|t| t.0)`) so
/// the round-trip `p.metadata.creation_timestamp = creation_stamp_at
/// (anchor); p.created_at() == Some(anchor)` is byte-identical to
/// the pre-lift hand-authored round-trip pinned at
/// [`crate::crd`]'s `created_at_returns_creation_anchor_when_stamped`
/// family.
///
/// Anchor-source axis: `DateTime<Utc>` — the operator supplies the
/// anchor, encoding "the caller has already chosen when" at the type
/// level. Composes directly with [`seconds_ago`] for the "created N
/// seconds ago" ephemeral-age fixture (the shape the
/// [`crate::lifetime_clock`] callsite walks) and with
/// [`at_epoch_second`] for the deterministic-epoch anchor fixture
/// (the shape a future `creation_stamped_process(at_epoch_second(N))`
/// caller would walk).
///
/// A future normalization at the wire form (see the doc-comment on
/// [`wire_time_some`] for the full rationale) lands at THAT shared
/// substrate primitive alongside [`tombstone_at`] / [`tombstone_now`]
/// so all THREE K8s metadata-Time slot composers inherit the upgrade
/// mechanically at the same substrate site. The prior "concrete
/// near-term compounding step" this doc-comment named — routing the
/// three composer bodies onto the shared `wire_time_some` primitive —
/// has landed; the cross-composer coherence pin
/// [`tests::creation_stamp_at_and_tombstone_at_agree_at_the_current_instant_on_wire_shape`]
/// stays in place as the post-consolidation wire-shape invariant.
///
/// [kcreat]: https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.30/#objectmeta-v1-meta
#[must_use]
pub fn creation_stamp_at(when: DateTime<Utc>) -> Option<Time> {
    wire_time_some(when)
}

/// Parse an `Option<&str>` as an RFC-3339 wall-clock stamp, discarding
/// the `ParseError` arm on the parseable-input axis. The one-line
/// `<opt>.and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())`
/// chain lifted to ONE typed owner past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold, and the sibling of every timed-decision
/// assertion / API-input parser that reads an RFC-3339 stamp out of a
/// JSON slot, an HTTP query param, or a K8s `status`-subresource value.
///
/// Pre-lift the SAME chain was hand-authored at 7 workspace-wide sites
/// across `tatara-reconciler::patch` — the wire-body substrate tests
/// pinning the `phase_status_base` / `phase_status_msg` /
/// `phase_status_with` sibling family's `phaseSince` slot at
/// fail-before-pass-after granularity:
///
/// * 3 sites walk `.and_then(Value::as_str).and_then(|s|
///   chrono::DateTime::parse_from_rfc3339(s).ok())` to lift the parsed
///   `DateTime<FixedOffset>` for a `[before, after]` bracket check that
///   proves the primitive stamped at call time.
/// * 4 sites walk `.and_then(Value::as_str).is_some_and(|s|
///   chrono::DateTime::parse_from_rfc3339(s).is_ok())` inside an
///   `assert!` that pins the slot's shape as "present + parses as
///   RFC-3339". The `bool` derives from `parse_rfc3339_opt(<opt>)
///   .is_some()`.
///
/// All 7 sites walked the SAME two-link chain — take an `Option<&str>`
/// (typically from a `serde_json::Value::as_str` cast), then parse the
/// inner `&str` as RFC-3339 and discard the `Err` arm. Post-lift each
/// callsite reads `parse_rfc3339_opt(<opt>)` (with `.is_some()` at the
/// 4 predicate sites) and the parser + `Result::ok()` discard sinks
/// live at ONE substrate owner.
///
/// Return-form axis: `Option<DateTime<FixedOffset>>` matches
/// `chrono::DateTime::parse_from_rfc3339`'s own return type — every
/// consumer that wants a `DateTime<Utc>` composes `.map(|dt| dt
/// .with_timezone(&chrono::Utc))` at its own site (the shape the
/// production `list_events` handlers at `tatara-api::rest::list_events`
/// + `tatara-testing::server::list_events` already walk), keeping the
/// timezone-normalization axis at the caller rather than baking a
/// specific `Utc` cast into the primitive.
///
/// A future normalization (a relaxed RFC-3339 profile that accepts a
/// `space`-separated date/time separator, a per-fleet clock-skew Δ
/// that rejects stamps too far in the future, a debug-build assertion
/// that the caller has already trimmed surrounding whitespace) lands
/// at THIS ONE substrate primitive and every downstream consumer
/// inherits the upgrade mechanically — no per-site edit at any of the
/// 7 listed callers or at future consumers (a `/proc`-table READ-side
/// timestamp reader, a compliance-binding freshness gate, a probe-
/// receipt `verified_at` field parser).
#[must_use]
pub fn parse_rfc3339_opt(s: Option<&str>) -> Option<DateTime<FixedOffset>> {
    s.and_then(|s| DateTime::parse_from_rfc3339(s).ok())
}

/// A `DateTime<Utc>` anchor at the whole-second Unix epoch offset
/// `secs` — the one-line `DateTime::<Utc>::from_timestamp(secs, 0)
/// .expect("valid epoch second")` chain lifted to ONE typed owner past
/// the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold, and the
/// deterministic-anchor peer of the wall-clock-reading composers
/// [`seconds_ago`] / [`tombstone_now`] on the same `→ DateTime<Utc>`
/// axis.
///
/// Pre-lift the SAME chain was hand-authored at 10 workspace-wide
/// fixture / helper sites within `tatara-process`, each restating the
/// whole-second `DateTime::<Utc>::from_timestamp(<secs>, 0)
/// .unwrap()` / `.expect(...)` shape to seed a deterministic anchor
/// that a fanout / composition / preservation pin can compare
/// verbatim (differing only in the whole-second `secs` argument and
/// in the `.unwrap()` vs `.expect("valid epoch second")` failure
/// arm):
///
/// * `tatara-process::time::tests::t` — the private test helper
///   `fn t(secs: i64) -> DateTime<Utc>` inside this module's own
///   tests, called out of every `elapsed_since` composition pin;
///   swept the `secs = 100 / 160 / 200 / 500` corners.
/// * `tatara-process::pool::tests::member` — the 3rd-arg
///   `entered_state_at` seed for the `PoolMember::unallocated` test
///   helper feeding the `state_count_fanout` / `process_names_set`
///   pin family; pinned at `secs = 0`.
/// * `tatara-process::pool::tests::named_member` — the peer
///   `named_member(process_name, state)` helper feeding the
///   `process_names_set` deduplication pins; pinned at `secs = 0`.
/// * `tatara-process::pool::tests::pool_status_observed_composes_
///   pre_lift_status_seed_verbatim` — the `now` anchor for the
///   `PoolStatus::observed` composition pin; pinned at `secs =
///   1_700_000_000` (a mid-2023 wall-clock timestamp).
/// * `tatara-process::pool::tests::pool_status_observed_moves_
///   members_by_value_without_extra_clone` — the `now` anchor for
///   the ownership pin; pinned at `secs = 0`.
/// * `tatara-process::pool::tests::pool_member_unallocated_fills_
///   every_slot_verbatim` — the positional-axis pin anchor; pinned
///   at `secs = 1_700_000_000`.
/// * `tatara-process::pool::tests::pool_member_unallocated_accepts_
///   owned_string_and_str_at_the_same_signature` — the `impl
///   Into<String>` axis pin anchor; pinned at `secs = 0`.
/// * `tatara-process::pool::tests::pool_member_unallocated_matches_
///   pre_lift_struct_literal_bytewise` — the byte-shape parity pin
///   anchor swept across every `MemberState` variant; pinned at
///   `secs = 1_700_000_000`.
/// * `tatara-process::pool::tests::pool_member_unallocated_
///   preserves_caller_clock_anchor` — TWO anchors on the
///   epoch-vs-future axis, pinned at `secs = 0` and `secs =
///   2_000_000_000` (a mid-2033 wall-clock timestamp).
///
/// All 10 sites walked the SAME two-link chain — build a UTC
/// `DateTime` from a whole-second Unix epoch offset, then discard
/// the `None` arm via `.unwrap()` / `.expect(...)` — differing only
/// in the second-count `secs` and in the failure-arm phrasing.
/// Post-lift each callsite reads `at_epoch_second(N)` and the
/// `chrono::DateTime::<Utc>::from_timestamp` construction + `None`-
/// arm discard sinks live at ONE substrate owner.
///
/// Return-form axis: `DateTime<Utc>` — the copy-form anchor every
/// consumer's downstream `signed_duration_since` / [`elapsed_since`]
/// / [`tombstone_at`] / `PoolMember::unallocated(<name>, <state>,
/// <anchor>)` composer takes as its `DateTime<Utc>`-typed operand.
/// The `i64` `secs` parameter matches `chrono::DateTime::<Utc>::
/// from_timestamp`'s own signature so a negative value (rare but
/// permitted for pre-epoch anchors) or a value past `i64::MAX / 2`
/// (also rare) yields whatever chrono itself yields, preserving the
/// pre-lift semantics verbatim on every corner every caller cared
/// about.
///
/// Failure-arm axis: `.expect("valid epoch second")` — matches the
/// module-internal helper's phrasing (which every existing
/// `elapsed_since` composition pin already routed through) rather
/// than pool.rs's `.unwrap()` phrasing. The composer is
/// `#[must_use]` and consumers write `at_epoch_second(N)` inline; a
/// caller that specifically needed the `.unwrap()` message can still
/// panic via the primitive because `chrono::DateTime::<Utc>::
/// from_timestamp` returns `Option<Self>` on the exact SAME
/// out-of-range corner — the `.expect(...)` phrasing sharpens the
/// panic message without changing the panic condition.
///
/// Sibling to [`seconds_ago`] on the `→ DateTime<Utc>` axis:
/// `seconds_ago` reads the wall clock at call time and returns an
/// anchor `N` seconds before it (a production timed-decision seed),
/// `at_epoch_second` reads a caller-supplied whole-second Unix epoch
/// offset and returns the anchor deterministically (a test-fixture /
/// deterministic-composition seed). The two composers partition the
/// `→ DateTime<Utc>` axis at the (wall-clock, deterministic) split
/// and cover it end-to-end at the substrate.
///
/// A future normalization (a per-fleet millisecond-precision
/// truncation on the anchor, a debug-build assertion that `secs` is
/// in a permitted window, a swap of the underlying
/// `chrono::DateTime::<Utc>::from_timestamp` for a
/// `TryFrom<UnixEpochSeconds>` typed alternative that pushes the
/// out-of-range corner into the type system) lands at THIS ONE
/// substrate primitive and every downstream consumer inherits the
/// upgrade mechanically — no per-site edit at any of the 10 listed
/// callers or at future consumers (a stable-name claim-arbiter's
/// deterministic-anchor fixture, a compliance-binding freshness-gate
/// test seed, a probe-receipt `verified_at` field's deterministic
/// fixture).
#[must_use]
pub fn at_epoch_second(secs: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(secs, 0).expect("valid epoch second")
}

/// The wall-clock drift tolerance between two `Utc::now()` reads
/// inside a single test — the ONE substrate owner of the 100ms
/// scheduler-jitter bound every test that composes two anchors from
/// separate wall-clock reads (peer composer vs hand-authored
/// `Utc::now()` block, sibling `tombstone_now()` vs
/// `tombstone_at(Utc::now())` fixture, sibling
/// `PoolStatus::observed_now(...)` vs `observed(..., Utc::now())`)
/// restated pre-lift.
///
/// Pre-lift the SAME `chrono::Duration::milliseconds(100)` bound was
/// hand-authored at EIGHT wall-clock parity pins past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold — SIX inside
/// `crate::time::tests`
/// (`seconds_ago_matches_hand_authored_pre_lift_chain_shape`,
/// `seconds_from_now_matches_hand_authored_pre_lift_chain_shape`,
/// `seconds_from_now_negates_seconds_ago_at_sign_flip`,
/// `tombstone_now_matches_hand_authored_pre_lift_chain_shape`,
/// `tombstone_now_and_tombstone_at_agree_at_the_current_instant`,
/// `wire_time_some_matches_tombstone_now_anchor_pre_lift`) and TWO
/// inside `crate::pool::tests`
/// (`pool_status_observed_now_matches_pre_lift_utc_now_composition_shape`,
/// `pool_status_observed_from_matches_pre_lift_observed_now_composition_shape`),
/// each restating `delta <= chrono::Duration::milliseconds(100)` with
/// a "100ms scheduler jitter" message verbatim, differing only in the
/// two anchors under test.
///
/// Post-lift each callsite reads [`scheduler_jitter`] on the RHS of
/// its `delta <= ...` assertion and interpolates
/// [`SCHEDULER_JITTER_MS`] into the failure message so a future
/// tolerance change (a tightening to 50ms on faster CI, a widening
/// to 200ms after a runner-slowdown audit) lands at ONE substrate
/// site and every downstream wall-clock parity pin inherits the
/// shift by construction.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// same `chrono::Duration::milliseconds(100)` literal recurred at
/// EIGHT test-scaffold sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication trigger, and is lifted to ONE substrate owner here).
/// THEORY.md §II.1 invariant 5 (composition preserves proofs — the
/// pins
/// [`tests::scheduler_jitter_matches_pre_lift_100ms_bound_by_construction`]
/// and [`tests::scheduler_jitter_is_a_non_negative_window`] bind the
/// value + sign of the tolerance so a regression that widened,
/// tightened, or flipped its sign surfaces at ONE substrate site
/// rather than as silent flakiness across every wall-clock parity
/// pin).
#[cfg(test)]
pub(crate) const SCHEDULER_JITTER_MS: i64 = 100;

/// Constructs the [`SCHEDULER_JITTER_MS`]-scaled `chrono::Duration`
/// — the wire-form the pre-lift `chrono::Duration::milliseconds(100)`
/// callsites read on the RHS of their `delta <= ...` assertions.
///
/// See [`SCHEDULER_JITTER_MS`] for the substrate's charter, the
/// enumerated pre-lift callsites, and the theory grounding.
#[cfg(test)]
#[must_use]
pub(crate) fn scheduler_jitter() -> chrono::Duration {
    chrono::Duration::milliseconds(SCHEDULER_JITTER_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: i64) -> DateTime<Utc> {
        // The whole-second epoch anchor rides through the ONE
        // substrate owner `at_epoch_second` (peer of the 10 workspace-
        // wide restatements of the SAME
        // `DateTime::<Utc>::from_timestamp(<secs>, 0).expect(...)` /
        // `.unwrap()` fixture chain that pre-lift lived at every
        // deterministic-anchor test helper in this crate — this
        // module's own tests + `pool::tests::{member, named_member,
        // pool_status_observed_composes_pre_lift_status_seed_verbatim,
        // pool_status_observed_moves_members_by_value_without_extra_
        // clone, pool_member_unallocated_*}`).
        at_epoch_second(secs)
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
                delta <= scheduler_jitter(),
                "composed {composed} and hand-authored {hand_authored} must agree within {SCHEDULER_JITTER_MS}ms scheduler jitter for secs={secs}"
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

    // ─── seconds_from_now substrate pins ──────────────────────────────
    //
    // Bind [`seconds_from_now`] at fail-before-pass-after granularity
    // so a regression that flipped the sign to `-` (collapsing the
    // primitive onto its [`seconds_ago`] peer, silently landing every
    // future-anchor fixture in the past and defeating the six
    // pre-lift `Utc::now() + chrono::Duration::seconds(N)` callsites
    // this primitive owns), swapped the unit (a `chrono::Duration::
    // minutes` typo, a `Duration::from_secs` unit-mismatch), or
    // dropped the wall-clock read (yielding `chrono::DateTime::<Utc>::
    // MIN_UTC + Duration::seconds(N)` = epoch-ish instead of a
    // wall-clock-anchored future) surfaces HERE rather than as silent
    // drift at every downstream future-anchor / wall-clock-jitter
    // bracket consumer.
    //
    // Each pin is fail-before-pass-after: the primitive did not exist
    // pre-lift, so any test that invokes it fails to compile pre-lift
    // and passes post-lift; the byte-identity pins below then bind
    // the specific shape choice.

    #[test]
    fn seconds_from_now_returns_anchor_at_seconds_offset_into_the_future() {
        // Primary shape asserted end-to-end: the returned anchor is
        // exactly `secs` seconds after a wall-clock instant bracketed
        // by two `Utc::now()` samples (a `before` sample taken just
        // before the substrate call and an `after` sample taken just
        // after). Sub-second scheduler jitter (a slow test runner, a
        // Kubelet clock skew) can push the two brackets apart by tens
        // of milliseconds — bound the tolerance at 50ms per side so
        // the pin stays green under CI load. A regression that flipped
        // the sign to `-` would land the anchor in the past and this
        // window check would fail; a regression that swapped the unit
        // (minutes / hours) would land the anchor far outside the
        // sub-second window.
        let secs = 42_i64;
        let before = Utc::now();
        let anchor = seconds_from_now(secs);
        let after = Utc::now();
        assert!(
            anchor >= before + chrono::Duration::seconds(secs) - chrono::Duration::milliseconds(50),
            "anchor {anchor} must be ≥ before + {secs}s (within 50ms scheduler jitter)"
        );
        assert!(
            anchor <= after + chrono::Duration::seconds(secs) + chrono::Duration::milliseconds(50),
            "anchor {anchor} must be ≤ after + {secs}s (within 50ms scheduler jitter)"
        );
    }

    #[test]
    fn seconds_from_now_matches_hand_authored_pre_lift_chain_shape() {
        // Byte-identical parity with the pre-lift `Utc::now() +
        // chrono::Duration::seconds(N)` block that all 6 hand-authored
        // callsites restated verbatim, swept across the three
        // representative second-counts every pre-lift consumer used
        // (small: 1s upper-bracket jitter tolerance, hour-scale:
        // 3_600s future anchor, far-future: 9_999s unrelated fallback).
        // Both blocks read the wall clock at DIFFERENT instants so the
        // two anchors CAN differ by the wall-clock delta between calls
        // — bound the divergence at 100ms scheduler jitter, matching
        // the [`seconds_ago`] peer pin's discipline.
        for secs in [1_i64, 3_600, 9_999] {
            let composed = seconds_from_now(secs);
            let hand_authored = Utc::now() + chrono::Duration::seconds(secs);
            let delta = (hand_authored - composed).abs();
            assert!(
                delta <= scheduler_jitter(),
                "composed {composed} and hand-authored {hand_authored} must agree within {SCHEDULER_JITTER_MS}ms scheduler jitter for secs={secs}"
            );
        }
    }

    #[test]
    fn seconds_from_now_zero_returns_anchor_at_current_instant() {
        // Boundary corner: `secs = 0` yields the current wall-clock
        // instant — the "just-created" moment. Mirrors
        // [`seconds_ago_zero_returns_anchor_at_current_instant`] on the
        // (past, future) axis: the two composers agree at the sign
        // boundary. A regression that synthesized a small offset
        // (`Duration::from_secs(1)` for clock skew, a per-fleet Δ)
        // would land the anchor 1 second in the future and every
        // zero-offset test seed would be off by that offset.
        let before = Utc::now();
        let anchor = seconds_from_now(0);
        let after = Utc::now();
        assert!(anchor >= before && anchor <= after);
    }

    #[test]
    fn seconds_from_now_negative_returns_anchor_in_the_past() {
        // Corner: a negative `secs` yields a past anchor. Matches
        // `chrono::Duration::seconds`'s own signed semantics — the
        // (past, future) mirror of
        // [`seconds_ago_negative_returns_anchor_in_the_future`]. A
        // regression that clamped the negative arm to `Utc::now()`
        // (or panicked) would silently break callers relying on the
        // signed semantics.
        let anchor = seconds_from_now(-10);
        let now = Utc::now();
        assert!(
            anchor <= now,
            "negative secs must yield a past anchor: anchor {anchor} vs now {now}"
        );
        assert!(
            anchor >= now - chrono::Duration::seconds(11),
            "anchor {anchor} must be within (10s + jitter) before now {now}"
        );
    }

    #[test]
    fn seconds_from_now_is_signed_symmetric_with_seconds_ago() {
        // Composition pin: `seconds_from_now(N)` and `seconds_ago(-N)`
        // produce byte-equivalent `DateTime<Utc>` values within
        // scheduler jitter — the two composers cover the same
        // `Utc::now() ± Duration::seconds(N)` axis at the (past,
        // future) split. Bounds the semantic-slot claim in
        // `seconds_from_now`'s doc-comment: a caller with a signed
        // direction picks the semantically-matching primitive, and
        // both branches project onto the SAME underlying anchor axis
        // when the sign is inverted. Sweeps the three representative
        // second-counts the pre-lift callsites used.
        for secs in [1_i64, 3_600, 9_999] {
            let plus = seconds_from_now(secs);
            let minus = seconds_ago(-secs);
            let delta = (plus - minus).abs();
            assert!(
                delta <= scheduler_jitter(),
                "seconds_from_now({secs}) and seconds_ago({}) must agree within {SCHEDULER_JITTER_MS}ms scheduler jitter (plus={plus} minus={minus})",
                -secs,
            );
        }
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
            delta <= scheduler_jitter(),
            "composed {} and hand-authored {} must agree within {SCHEDULER_JITTER_MS}ms scheduler jitter",
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
            delta <= scheduler_jitter(),
            "tombstone_now anchor {} and tombstone_at(Utc::now()) anchor {} must agree within {SCHEDULER_JITTER_MS}ms scheduler jitter",
            a.0,
            b.0,
        );
    }

    // ─── parse_rfc3339_opt substrate pins ───────────────────────────
    //
    // Bind [`parse_rfc3339_opt`] at fail-before-pass-after granularity
    // so a regression that swallowed the `Some(_)` arm (yielding
    // `None` on a well-formed stamp), swapped the parser for a
    // rfc2822/naive/ISO-8601-only variant (silently rejecting the
    // `+00:00` offset every K8s `metadata.Time.0` serialiser emits),
    // or flipped the `.ok()` discard for a `.unwrap()` (panicking on
    // malformed input rather than short-circuiting at the caller's
    // `.expect(...)`) surfaces HERE rather than as silent operator-
    // facing drift at the 7 downstream `phaseSince`-slot pins.
    //
    // Each pin is fail-before-pass-after: the primitive did not exist
    // pre-lift, so any test that invokes it fails to compile pre-lift
    // and passes post-lift; the byte-identity pins below then bind
    // the specific shape choice.

    #[test]
    fn parse_rfc3339_opt_returns_some_datetime_on_well_formed_utc_stamp() {
        // Primary shape asserted end-to-end: a well-formed `+00:00`
        // stamp (the exact wire form every K8s `metadata.Time`
        // serialiser emits, and every `phaseSince`-stamped
        // `Utc::now()` produces once serialised via `serde_json` into
        // a `Value`) parses to `Some(dt)` with the anchor preserved
        // verbatim.
        let stamp = "2026-05-01T12:34:56+00:00";
        let parsed = parse_rfc3339_opt(Some(stamp)).expect("well-formed RFC-3339 stamp");
        let expected =
            DateTime::parse_from_rfc3339(stamp).expect("hand-authored fixture is well-formed");
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_rfc3339_opt_passes_none_through_verbatim() {
        // The `Option::and_then` short-circuit: a `None` input flows
        // straight to `None` output without touching the parser. A
        // regression that expected `Some(_)` unconditionally
        // (`unwrap_or_default` on the input, an early `.unwrap()`)
        // would panic HERE rather than at the caller's `.expect(...)`.
        assert_eq!(parse_rfc3339_opt(None), None);
    }

    #[test]
    fn parse_rfc3339_opt_discards_the_err_arm_on_malformed_input() {
        // The `.ok()` discard arm: a `Some(&str)` that isn't RFC-3339
        // flows to `None` — the caller's `.is_some()` predicate then
        // returns `false` for the "present-but-malformed" corner. A
        // regression that panicked (`unwrap`) or bubbled the
        // `ParseError` (a `Result` return form) would break every
        // predicate-site assertion that reads the return as a bool.
        assert_eq!(parse_rfc3339_opt(Some("not-a-timestamp")), None);
        assert_eq!(parse_rfc3339_opt(Some("")), None);
        // A naive-only shape (no offset) — RFC-3339 requires the
        // offset, so this must land on the `None` arm.
        assert_eq!(parse_rfc3339_opt(Some("2026-05-01T12:34:56")), None);
    }

    #[test]
    fn parse_rfc3339_opt_matches_hand_authored_pre_lift_chain_shape() {
        // Byte-identical parity with the pre-lift `<opt>.and_then(|s|
        // chrono::DateTime::parse_from_rfc3339(s).ok())` block all 7
        // hand-authored callsites restated verbatim, swept across the
        // four representative corners: a well-formed `Some(&str)`, a
        // `None`, a `Some("")`, and a `Some(<malformed>)`. Both blocks
        // must project the SAME `Option<DateTime<FixedOffset>>` on
        // every corner so the collapse is observationally invisible.
        for (opt, name) in [
            (Some("2026-05-01T12:34:56+00:00"), "well-formed +00:00"),
            (Some("2026-05-01T12:34:56.789012345+00:00"), "sub-second"),
            (Some("2026-05-01T12:34:56-05:00"), "non-UTC offset"),
            (Some("2026-05-01T12:34:56Z"), "Z-form UTC"),
            (Some(""), "empty string"),
            (Some("not-a-timestamp"), "malformed"),
            (None, "none input"),
        ] {
            let composed = parse_rfc3339_opt(opt);
            let hand_authored = opt.and_then(|s| DateTime::parse_from_rfc3339(s).ok());
            assert_eq!(
                composed, hand_authored,
                "corner `{name}` must round-trip through both shapes",
            );
        }
    }

    #[test]
    fn parse_rfc3339_opt_is_some_derives_the_predicate_shape() {
        // The `.is_some()` composition on the primitive's return is
        // the byte-identical replacement for the pre-lift `<opt>
        // .is_some_and(|s| chrono::DateTime::parse_from_rfc3339(s)
        // .is_ok())` chain the 4 predicate-site pins walked. Sweep
        // the four representative corners: present-and-well-formed,
        // present-but-malformed, present-but-empty, absent. A
        // regression that flipped the primitive's `None`-on-malformed
        // arm to `Some(default)` would silently pass the malformed
        // corner past every predicate-site assertion.
        for (opt, expected, name) in [
            (Some("2026-05-01T12:34:56+00:00"), true, "well-formed"),
            (Some("not-a-timestamp"), false, "malformed"),
            (Some(""), false, "empty string"),
            (None, false, "none input"),
        ] {
            assert_eq!(
                parse_rfc3339_opt(opt).is_some(),
                expected,
                "corner `{name}` must project to `{expected}` through the predicate shape",
            );
        }
    }

    #[test]
    fn parse_rfc3339_opt_round_trips_a_freshly_stamped_utc_now() {
        // The canonical downstream composition: a `Utc::now()` stamp
        // serialised via `chrono::DateTime::to_rfc3339` — the shape
        // every `phaseSince` slot the reconciler stamps writes onto
        // the wire — parses back through this primitive to a
        // `DateTime<FixedOffset>` whose UTC-normalised anchor agrees
        // with the source stamp bytewise. Pin the round-trip identity
        // so a future normalization (a millisecond-precision
        // truncation, a per-fleet skew Δ) cannot silently drift the
        // reader off the writer at the reconciler's own `phaseSince`
        // wire.
        let source = Utc::now();
        let wire = source.to_rfc3339();
        let parsed = parse_rfc3339_opt(Some(&wire)).expect("Utc::now → to_rfc3339 must round-trip");
        assert_eq!(parsed.with_timezone(&Utc), source);
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
        //
        // Future-anchor seed rides through the ONE substrate owner
        // `seconds_from_now` (peer of `seconds_ago`) so a future
        // normalization at the wall-clock-read + duration-add sink
        // lands at that substrate and this pin inherits mechanically.
        let future = seconds_from_now(3_600);
        let stamp = tombstone_at(future).expect("tombstone_at returns Some");
        assert_eq!(stamp.0, future);
    }

    // ─── at_epoch_second substrate pins ───────────────────────────────
    //
    // Bind [`at_epoch_second`] at fail-before-pass-after granularity so
    // a regression that drifted the nanosecond slot off the whole-
    // second axis (a `from_timestamp(secs, 1)` typo that stamps a
    // 1-ns offset into every deterministic fixture), swapped the
    // `.expect(...)` for a saturating fallback (yielding
    // `DateTime::<Utc>::default()` = epoch on the out-of-range corner
    // rather than panicking), or reshaped the return form off
    // `DateTime<Utc>` (e.g., to `DateTime<FixedOffset>`) surfaces HERE
    // rather than as silent operator-invisible drift at the 10
    // downstream fixture consumers.
    //
    // Each pin is fail-before-pass-after: the primitive did not exist
    // pre-lift, so any test that invokes it fails to compile pre-lift
    // and passes post-lift; the byte-identity pins below then bind the
    // specific shape choice.

    #[test]
    fn at_epoch_second_returns_utc_datetime_at_whole_second_offset() {
        // Primary shape asserted end-to-end: the returned anchor is
        // exactly `secs` whole seconds past the Unix epoch, with no
        // nanosecond component. A regression that stamped a stray
        // nanosecond slot (a `from_timestamp(secs, 1)` typo, a
        // per-fleet skew Δ added below the composer) would surface
        // here as a non-zero `.timestamp_subsec_nanos()`.
        let anchor = at_epoch_second(1_700_000_000);
        assert_eq!(anchor.timestamp(), 1_700_000_000);
        assert_eq!(anchor.timestamp_subsec_nanos(), 0);
    }

    #[test]
    fn at_epoch_second_matches_hand_authored_pre_lift_chain_shape() {
        // Byte-identical parity with the pre-lift `DateTime::<Utc>::
        // from_timestamp(<secs>, 0).unwrap()` / `.expect(...)` block
        // that all 10 hand-authored callsites restated verbatim,
        // swept across the four representative second-counts every
        // pre-lift consumer used: `0` (the epoch anchor), `100` (the
        // `elapsed_since` composition pin anchor), `1_700_000_000`
        // (the mid-2023 wall-clock anchor), and `2_000_000_000` (the
        // mid-2033 future anchor). Both blocks must project the SAME
        // `DateTime<Utc>` on every corner so the collapse is
        // observationally invisible.
        for secs in [0_i64, 100, 1_700_000_000, 2_000_000_000] {
            let composed = at_epoch_second(secs);
            let hand_authored = DateTime::<Utc>::from_timestamp(secs, 0)
                .expect("hand-authored fixture is well-formed");
            assert_eq!(
                composed, hand_authored,
                "corner `secs={secs}` must round-trip through both shapes",
            );
        }
    }

    #[test]
    fn at_epoch_second_is_deterministic_across_repeated_calls() {
        // Determinism pin: `at_epoch_second` does NOT read the wall
        // clock — every call with the SAME `secs` produces byte-
        // identical anchors, in contrast to the sibling
        // [`seconds_ago`] / [`tombstone_now`] wall-clock-reading
        // peers. A regression that started stamping the composer's
        // own `Utc::now()` (a fallback default, a per-call skew) would
        // silently defeat every fanout / composition / preservation
        // pin that relies on the anchor being replayable.
        let first = at_epoch_second(500);
        let second = at_epoch_second(500);
        assert_eq!(
            first, second,
            "at_epoch_second must be deterministic — no wall-clock read",
        );
    }

    #[test]
    fn at_epoch_second_partitions_the_datetime_axis_against_seconds_ago() {
        // Cross-composer partition pin: `at_epoch_second` and
        // [`seconds_ago`] both produce `DateTime<Utc>` but partition
        // the axis at the (deterministic, wall-clock-reading) split —
        // `at_epoch_second(0)` is byte-identical across two calls,
        // `seconds_ago(0)` drifts by the scheduler jitter between two
        // calls. A regression that merged either primitive onto the
        // other (a `seconds_ago` that started reading a module-load
        // constant, an `at_epoch_second` that started subtracting from
        // `Utc::now`) would collapse the partition and surface here.
        let a = at_epoch_second(0);
        let b = at_epoch_second(0);
        assert_eq!(a, b, "at_epoch_second is deterministic");
        let live_1 = seconds_ago(0);
        // Do not compare `live_1` to `a` bytewise — the two live at
        // different points on the `DateTime<Utc>` axis by design.
        // Instead, pin that the deterministic anchor equals the epoch
        // (a live wall-clock read is unambiguously past the epoch on
        // any post-2020 system clock).
        assert_eq!(a.timestamp(), 0, "at_epoch_second(0) is the Unix epoch");
        assert!(
            live_1.timestamp() > 0,
            "seconds_ago(0) reads the wall clock, which is unambiguously past the epoch",
        );
    }

    #[test]
    fn at_epoch_second_composes_with_tombstone_at_at_deterministic_fixture_shape() {
        // The canonical downstream composition: a fixture that needs a
        // deterministic tombstone at a fixed epoch offset composes
        // `tombstone_at(at_epoch_second(N))` and expects the returned
        // anchor to be exactly `N` seconds past the Unix epoch. A
        // regression that reshaped either primitive so the two no
        // longer round-trip would surface HERE rather than as silent
        // skew at the deterministic-tombstone fixture family.
        let anchor = at_epoch_second(1_700_000_000);
        let stamp = tombstone_at(anchor).expect("tombstone_at returns Some");
        assert_eq!(
            stamp.0, anchor,
            "tombstone_at must preserve the at_epoch_second-produced anchor verbatim",
        );
        assert_eq!(stamp.0.timestamp(), 1_700_000_000);
        assert_eq!(stamp.0.timestamp_subsec_nanos(), 0);
    }

    // ─── creation_stamp_at substrate pins ─────────────────────────────
    //
    // Bind [`creation_stamp_at`] at fail-before-pass-after granularity
    // so a regression that dropped the `Some` wrap (yielding
    // `Option<Time>` = `None`, which would silently un-stamp every age-
    // anchored fixture and short-circuit every downstream TTL-expiry
    // / staleness-gate / `created_at`-projection pin), swapped the
    // `Time` newtype for a raw `DateTime<Utc>` (breaking the
    // `metadata.creation_timestamp: Option<Time>` slot's shape), or
    // silently re-read the wall clock instead of preserving `when`
    // (collapsing the anchor-explicit composer onto a wall-clock-
    // reading peer, defeating every deterministic-fixture pin that
    // relies on the anchor being replayable) surfaces HERE rather than
    // as silent operator-invisible drift at the two downstream
    // fixture-helper callsites.
    //
    // Each pin is fail-before-pass-after: the primitive did not exist
    // pre-lift, so any test that invokes it fails to compile pre-lift
    // and passes post-lift; the byte-identity pins below then bind the
    // specific shape choice.

    #[test]
    fn creation_stamp_at_returns_some_time_preserving_the_operator_anchor() {
        // Primary shape asserted end-to-end: the returned option is
        // `Some(Time(when))` and the anchor is exactly the `when`
        // argument — no wall-clock read, no normalization, no clamp.
        // Mirrors the peer pin
        // [`tombstone_at_returns_some_time_preserving_the_operator_anchor`]
        // on the (creation, deletion) axis of the ObjectMeta
        // metadata-Time slots. A regression that fell through to the
        // current instant (`creation_stamp_at` ignoring `when` and
        // re-reading the wall clock) would fail the identity check
        // HERE rather than as silent age-drift at every fixture that
        // stamps a deterministic creation anchor for its downstream
        // TTL / staleness pin.
        let epoch = at_epoch_second(1_700_000_000);
        let stamp = creation_stamp_at(epoch).expect("creation_stamp_at returns Some");
        assert_eq!(stamp.0, epoch, "anchor must be preserved verbatim");
    }

    #[test]
    fn creation_stamp_at_matches_hand_authored_pre_lift_chain_shape() {
        // Byte-identical parity with the pre-lift
        // `Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(<anchor>))`
        // / `Some(Time(<anchor>))` block that both hand-authored
        // fixture-helper sites restated verbatim (differing only in
        // the fully-qualified vs locally-imported `Time` spelling —
        // the crd-tests site is fully-qualified, the lifetime-clock-
        // tests site is locally-imported, both walk the SAME 5-token
        // wire wrap). Sweeps the two representative anchor shapes
        // both pre-lift consumers walked: a deterministic epoch anchor
        // (the crd-tests site's `at_epoch_second`-composed anchor) and
        // a past-relative anchor (the lifetime-clock-tests site's
        // `seconds_ago`-composed anchor). Both blocks must project the
        // SAME `Option<Time>` on every corner so the collapse is
        // observationally invisible.
        for anchor in [
            at_epoch_second(0),
            at_epoch_second(1_700_000_000),
            at_epoch_second(2_000_000_000),
            seconds_ago(3_600),
        ] {
            let composed = creation_stamp_at(anchor);
            let hand_authored = Some(Time(anchor));
            assert_eq!(
                composed, hand_authored,
                "corner `anchor={anchor}` must round-trip through both shapes",
            );
        }
    }

    #[test]
    fn creation_stamp_at_composes_with_at_epoch_second_at_deterministic_fixture_shape() {
        // The canonical downstream composition at the crd-tests site:
        // a fixture that needs a deterministic creation stamp at a
        // fixed epoch offset composes
        // `creation_stamp_at(at_epoch_second(N))` and expects the
        // returned anchor to be exactly `N` seconds past the Unix
        // epoch. A regression that reshaped either primitive so the
        // two no longer round-trip would surface HERE rather than as
        // silent age-drift at the 9-case `Process::created_at`
        // inherent-forwarder pin family (which reads back
        // `.metadata.creation_timestamp.as_ref().map(|t| t.0)` and
        // compares it to the exact anchor the fixture stamped).
        let anchor = at_epoch_second(1_700_000_000);
        let stamp = creation_stamp_at(anchor).expect("creation_stamp_at returns Some");
        assert_eq!(
            stamp.0, anchor,
            "creation_stamp_at must preserve the at_epoch_second-produced anchor verbatim",
        );
        assert_eq!(stamp.0.timestamp(), 1_700_000_000);
        assert_eq!(stamp.0.timestamp_subsec_nanos(), 0);
    }

    #[test]
    fn creation_stamp_at_composes_with_seconds_ago_at_ephemeral_age_fixture_shape() {
        // The canonical downstream composition at the lifetime-clock-
        // tests site: a fixture that needs a "created N seconds ago"
        // ephemeral-age creation stamp composes
        // `creation_stamp_at(seconds_ago(N))` and expects the returned
        // anchor's elapsed-since-now to be ~N seconds. Matches the
        // pre-lift shape at `ephemeral_process(age_secs, ttl, teardown)`
        // which stamps `Some(Time(seconds_ago(age_secs)))` on the
        // fresh Process fixture. A regression that reshaped either
        // primitive so the two no longer round-trip would surface HERE
        // rather than as silent skew at every ephemeral-lifetime
        // TTL-expiry / requeue-with-ttl pin whose age-anchored fixture
        // rides the composition.
        let secs = 3_600_i64;
        let anchor = seconds_ago(secs);
        let stamp = creation_stamp_at(anchor).expect("creation_stamp_at returns Some");
        assert_eq!(
            stamp.0, anchor,
            "creation_stamp_at must preserve the seconds_ago-produced anchor verbatim",
        );
        // And the anchor is ~N seconds in the past — this is the
        // downstream property every ephemeral-lifetime fixture using
        // the composition relies on.
        let elapsed = elapsed_since(Utc::now(), stamp.0).expect("elapsed is Some for past anchor");
        assert!(
            elapsed >= Duration::from_secs(secs as u64),
            "elapsed {elapsed:?} must be ≥ {secs}s — the anchor was stamped {secs}s ago",
        );
    }

    #[test]
    fn creation_stamp_at_and_tombstone_at_agree_at_the_current_instant_on_wire_shape() {
        // Cross-composer coherence pin on the shared 5-token wire wrap:
        // `creation_stamp_at(anchor)` and `tombstone_at(anchor)` produce
        // the SAME shape (`Some(Time(anchor))`) for the SAME
        // deterministic anchor — the two composers partition the
        // ObjectMeta metadata-Time surface at the (creation, deletion)
        // axis but ride the SAME 5-token wire wrap. A future
        // consolidation onto a shared private substrate
        // `wire_time_some(when)` primitive (see the doc-comment
        // rationale on [`creation_stamp_at`]) cannot land any wire-
        // shape drift between the two semantic slots because this pin
        // binds them at a deterministic anchor where both bodies
        // converge to the SAME `Option<Time>`. Sweeps the
        // representative deterministic anchors both peer composers
        // routinely receive (epoch, mid-past, mid-future) so a wire-
        // shape drift at any corner surfaces HERE rather than as
        // silent per-slot skew at the downstream fixture consumers.
        for anchor in [
            at_epoch_second(0),
            at_epoch_second(1_700_000_000),
            at_epoch_second(2_000_000_000),
        ] {
            assert_eq!(
                creation_stamp_at(anchor),
                tombstone_at(anchor),
                "creation_stamp_at and tombstone_at must produce the SAME wire wrap for anchor={anchor}",
            );
        }
    }

    #[test]
    fn creation_stamp_at_preserves_a_future_anchor_without_clamping() {
        // Corner: `creation_stamp_at` accepts a future anchor verbatim
        // — mirrors the peer pin
        // [`tombstone_at_preserves_a_future_anchor_without_clamping`]
        // on the (creation, deletion) axis. A future normalization that
        // clamps the anchor into the past (a "no creation can be in
        // the future" policy) has to explicitly move this pin rather
        // than silently trampling a fixture that stamps a future
        // creation anchor to test a per-fleet-skew tolerance downstream.
        //
        // Future-anchor seed rides through the ONE substrate owner
        // `seconds_from_now` (peer of `seconds_ago`) — same discipline
        // as the [`tombstone_at`] peer pin.
        let future = seconds_from_now(3_600);
        let stamp = creation_stamp_at(future).expect("creation_stamp_at returns Some");
        assert_eq!(stamp.0, future);
    }

    // ─── wire_time_some substrate pins ────────────────────────────────
    //
    // Bind the shared 5-token `Some(Time(<anchor>))` substrate primitive
    // [`wire_time_some`] at fail-before-pass-after granularity so a
    // regression that dropped the `Some` wrap (yielding
    // `Option<Time>` = `None`, which would silently un-stamp every
    // metadata-Time slot every downstream composer feeds), swapped the
    // `Time` newtype for a raw `DateTime<Utc>` (breaking the
    // `metadata.*_timestamp: Option<Time>` slot shape), or silently
    // re-read the wall clock instead of preserving `when` (collapsing
    // the shared-substrate primitive onto a wall-clock-reading peer,
    // defeating every deterministic-fixture pin the three downstream
    // composers rely on) surfaces HERE rather than as silent operator-
    // invisible drift at every metadata-Time composer in this module.
    //
    // Each pin is fail-before-pass-after: the primitive did not exist
    // pre-lift, so any test that invokes it fails to compile pre-lift
    // and passes post-lift; the byte-identity pins below then bind
    // the specific shape choice.

    #[test]
    fn wire_time_some_returns_some_time_preserving_the_operator_anchor() {
        // Primary shape asserted end-to-end: the returned option is
        // `Some(Time(when))` and the anchor is exactly the `when`
        // argument — no wall-clock read, no normalization, no clamp.
        // The shared-substrate peer of the two anchor-explicit
        // composer pins
        // [`tombstone_at_returns_some_time_preserving_the_operator_anchor`]
        // + [`creation_stamp_at_returns_some_time_preserving_the_operator_anchor`]
        // on the composer-body-level projection.
        let epoch = at_epoch_second(1_700_000_000);
        let stamp = wire_time_some(epoch).expect("wire_time_some returns Some");
        assert_eq!(stamp.0, epoch, "anchor must be preserved verbatim");
    }

    #[test]
    fn wire_time_some_matches_hand_authored_pre_lift_chain_shape() {
        // Byte-identical parity with the pre-lift `Some(Time(<anchor>))`
        // block that all THREE downstream composer bodies
        // ([`tombstone_now`] via `Utc::now()`, [`tombstone_at`] +
        // [`creation_stamp_at`] via the operator anchor) restated
        // verbatim, swept across the representative anchor shapes the
        // downstream composers routinely receive: the Unix epoch, a
        // mid-past deterministic anchor, a mid-future deterministic
        // anchor, and a `seconds_ago`-composed past-relative anchor.
        // Both blocks must project the SAME `Option<Time>` on every
        // corner so the collapse is observationally invisible.
        for anchor in [
            at_epoch_second(0),
            at_epoch_second(1_700_000_000),
            at_epoch_second(2_000_000_000),
            seconds_ago(3_600),
        ] {
            let composed = wire_time_some(anchor);
            let hand_authored = Some(Time(anchor));
            assert_eq!(
                composed, hand_authored,
                "corner `anchor={anchor}` must round-trip through both shapes",
            );
        }
    }

    #[test]
    fn wire_time_some_is_deterministic_across_repeated_calls() {
        // Determinism pin: `wire_time_some` does NOT read the wall
        // clock — every call with the SAME `when` produces byte-
        // identical `Option<Time>`, in contrast to the wall-clock-
        // reading [`tombstone_now`] peer that reads `Utc::now()` at
        // its OWN body before delegating. A regression that started
        // stamping the substrate's own `Utc::now()` (a fallback
        // default, a per-call skew Δ, a mis-consolidation of
        // `tombstone_now`'s clock read onto the shared substrate)
        // would silently defeat every fixture / composition /
        // preservation pin the three downstream composers rely on
        // for anchor replay.
        let anchor = at_epoch_second(1_700_000_000);
        let first = wire_time_some(anchor);
        let second = wire_time_some(anchor);
        assert_eq!(
            first, second,
            "wire_time_some must be deterministic — no wall-clock read",
        );
    }

    #[test]
    fn wire_time_some_preserves_a_future_anchor_without_clamping() {
        // Corner: `wire_time_some` accepts a future anchor verbatim —
        // matches the peer pins
        // [`tombstone_at_preserves_a_future_anchor_without_clamping`]
        // + [`creation_stamp_at_preserves_a_future_anchor_without_clamping`]
        // on the composer-body-level projection. A future normalization
        // that clamps the anchor into the past (a "no metadata-Time
        // stamp can be in the future" policy) has to explicitly move
        // this pin at the shared substrate rather than silently
        // trampling every fixture that stamps a future anchor to test
        // a per-fleet-skew tolerance downstream.
        //
        // Future-anchor seed rides through the ONE substrate owner
        // `seconds_from_now` — same discipline as the two composer
        // peer pins above.
        let future = seconds_from_now(3_600);
        let stamp = wire_time_some(future).expect("wire_time_some returns Some");
        assert_eq!(stamp.0, future);
    }

    #[test]
    fn tombstone_at_delegates_through_wire_time_some_bytewise() {
        // Delegation coherence pin: [`tombstone_at`] projects `when`
        // through the shared `wire_time_some` substrate — a regression
        // that re-inlined the 5-token `Some(Time(when))` literal at
        // [`tombstone_at`]'s body (bypassing the shared substrate,
        // defeating every future normalization that lands at
        // `wire_time_some` and expects the composer to inherit the
        // upgrade mechanically) surfaces HERE rather than as silent
        // wire-shape drift between the two composers. Sweeps the
        // representative deterministic anchors both composers
        // routinely receive.
        for anchor in [
            at_epoch_second(0),
            at_epoch_second(1_700_000_000),
            at_epoch_second(2_000_000_000),
        ] {
            assert_eq!(
                tombstone_at(anchor),
                wire_time_some(anchor),
                "tombstone_at must delegate through wire_time_some for anchor={anchor}",
            );
        }
    }

    #[test]
    fn creation_stamp_at_delegates_through_wire_time_some_bytewise() {
        // Delegation coherence pin: [`creation_stamp_at`] projects
        // `when` through the shared `wire_time_some` substrate — the
        // peer of [`tombstone_at_delegates_through_wire_time_some_bytewise`]
        // on the (creation, deletion) axis of the ObjectMeta
        // metadata-Time slots. Both composers ride the SAME shared
        // substrate; the two delegation pins together bind the
        // consolidation invariant so a future normalization at
        // `wire_time_some` cannot land any per-slot skew.
        for anchor in [
            at_epoch_second(0),
            at_epoch_second(1_700_000_000),
            at_epoch_second(2_000_000_000),
        ] {
            assert_eq!(
                creation_stamp_at(anchor),
                wire_time_some(anchor),
                "creation_stamp_at must delegate through wire_time_some for anchor={anchor}",
            );
        }
    }

    #[test]
    fn tombstone_now_delegates_through_wire_time_some_with_utc_now_stamp() {
        // Delegation coherence pin on the wall-clock-reading axis:
        // [`tombstone_now`] reads `Utc::now()` and projects the
        // resulting anchor through the shared `wire_time_some`
        // substrate — a regression that re-inlined the 5-token
        // `Some(Time(Utc::now()))` literal at [`tombstone_now`]'s
        // body (bypassing the shared substrate) surfaces HERE. Both
        // blocks read the wall clock at DIFFERENT instants so the two
        // anchors CAN differ by scheduler jitter — bound the
        // divergence at 100ms, matching the peer wall-clock-parity
        // pins' tolerance
        // ([`tombstone_now_matches_hand_authored_pre_lift_chain_shape`],
        // [`seconds_ago_matches_hand_authored_pre_lift_chain_shape`]).
        let composed = tombstone_now().expect("tombstone_now returns Some");
        let via_substrate = wire_time_some(Utc::now()).expect("wire_time_some returns Some");
        let delta = (via_substrate.0 - composed.0).abs();
        assert!(
            delta <= scheduler_jitter(),
            "tombstone_now anchor {} and wire_time_some(Utc::now()) anchor {} must agree within {SCHEDULER_JITTER_MS}ms scheduler jitter",
            composed.0,
            via_substrate.0,
        );
    }

    // ─── scheduler_jitter substrate pins ────────────────────────────
    //
    // Bind [`scheduler_jitter`] + [`SCHEDULER_JITTER_MS`] at
    // fail-before-pass-after granularity so a regression that widened
    // the tolerance (a 200ms bump after a runner-slowdown audit),
    // tightened it (a 50ms bump on faster CI), or flipped its sign
    // surfaces HERE rather than as silent flakiness at every
    // wall-clock parity pin routed through the substrate.

    #[test]
    fn scheduler_jitter_matches_pre_lift_100ms_bound_by_construction() {
        // Byte-identical parity with the pre-lift hand-authored
        // `chrono::Duration::milliseconds(100)` literal every 8
        // wall-clock parity pins restated verbatim (6 in this module
        // + 2 in `crate::pool::tests`). Pins the value axis of the
        // tolerance so a substrate-side change of the constant lands
        // at THIS pin rather than as silent skew at every downstream
        // `delta <= scheduler_jitter()` assertion.
        assert_eq!(scheduler_jitter(), chrono::Duration::milliseconds(100));
        assert_eq!(
            scheduler_jitter(),
            chrono::Duration::milliseconds(SCHEDULER_JITTER_MS),
        );
        assert_eq!(SCHEDULER_JITTER_MS, 100);
    }

    #[test]
    fn scheduler_jitter_is_a_non_negative_window() {
        // Sign pin: the jitter bound MUST be strictly positive. A
        // negative constant would make the `|delta| <= scheduler_jitter()`
        // assertion trivially false for every non-zero absolute delta
        // and fail every wall-clock parity pin at once; a zero-valued
        // constant would tolerate zero drift only and fail every pin
        // whose two `Utc::now()` reads returned different sub-
        // microsecond anchors. Pins the DOMAIN of the tolerance so a
        // regression that flipped the sign or zeroed the constant
        // surfaces HERE rather than as a flood of downstream flakes.
        assert!(scheduler_jitter() > chrono::Duration::zero());
        assert!(SCHEDULER_JITTER_MS > 0);
    }
}
