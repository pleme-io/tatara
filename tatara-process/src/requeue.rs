//! Requeue-action primitives — the small typed layer over the
//! `kube::runtime::controller::Action::requeue(std::time::Duration
//! ::from_secs(<secs>))` two-link chain every phase-machine handler,
//! error-policy sink, and Signals ingest arm in the workspace exits
//! through.
//!
//! `kube_runtime` exposes only ONE requeue constructor —
//! `Action::requeue(std::time::Duration)` — but every consumer in
//! this workspace already carries its retry / heartbeat / short-
//! retry budget as a whole-second `u64` (the module-level
//! `TICK_RETRY` / `HEARTBEAT` / `SHORT_RETRY` constants, the
//! per-call literals `1`, `5`, `10`, `15`, `30`, `60`, `120`, and
//! the `ctx.config.heartbeat_seconds` slot). This module owns the
//! one-line chain from that `u64` to the returned `Action`, so a
//! future normalization (an injectable-jitter overlay, a bounded-
//! rate limiter, a per-controller floor / ceiling clamp, a
//! deterministic-clock test hook) lands at ONE substrate primitive
//! and every downstream retry sink inherits the upgrade mechanically.

use kube::runtime::controller::Action;
use std::time::Duration;

/// A `kube::runtime::controller::Action` that re-enqueues the
/// current object `secs` seconds from now — the one-line
/// `Action::requeue(Duration::from_secs(secs))` two-link chain
/// lifted to ONE typed owner past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold.
///
/// Pre-lift the SAME chain was hand-authored at 35 workspace-wide
/// consumer sites across 5 files in the two ACTIVE reconciler
/// crates:
///
/// * `tatara-reconciler::phase_machine` — 22 sites feeding the
///   module-level `TICK_RETRY` / `HEARTBEAT` / `SHORT_RETRY`
///   constants at every FSM handler's tail (the `pending` /
///   `forking` / `execing` / `running` / `attested` / `reconverging`
///   / `releasing` / `exiting` / `failed` / `zombie` / `reaped`
///   arms).
/// * `tatara-reconciler::controller` — 5 sites feeding literal `1`
///   (deletion-preemption + signal-arm re-poll), the
///   `ctx.config.heartbeat_seconds` slot (suspend), and `30`
///   (reconcile-error sink + `error_policy`).
/// * `tatara-reconciler::table_controller` — 2 sites feeding `30`
///   at the ProcessTable heartbeat + error policy.
/// * `tatara-pool-reconciler::controller_pool` — 3 sites feeding
///   the reconcile-interval slot + literal `15`.
/// * `tatara-pool-reconciler::controller_allocation` — 3 sites
///   feeding literal `5` (bind-retry), the reconcile-interval
///   slot, and literal `15`.
///
/// All 35 sites walked the SAME two-link chain — take a whole-
/// second `u64` (a literal, a named constant, or a config slot),
/// wrap it in a `std::time::Duration` via `from_secs`, then hand
/// the duration to `Action::requeue`. Differing only in the
/// second-count operand. Post-lift each callsite reads
/// `tatara_process::requeue::after_secs(N)` and the wrap +
/// requeue chain lives at ONE substrate owner.
///
/// Return-form axis: `kube::runtime::controller::Action` — the
/// exact type every kube-runtime `reconcile` fn returns as
/// `Ok(...)` and every `error_policy` returns bare. The `u64`
/// `secs` parameter matches `Duration::from_secs`'s own signature
/// so the migration is byte-identical: every pre-lift site fed a
/// `u64` (either a literal, a named `pub const N: u64 = ...;`, or
/// a config field typed as `u64`) directly into `Duration::
/// from_secs`, and the same feed continues to work at
/// `after_secs`.
///
/// A future normalization — an injectable jitter overlay that
/// randomizes ±10% of `secs` to avoid a thundering herd of
/// synchronized reconcile ticks, a per-controller
/// floor/ceiling clamp so a mis-configured heartbeat can't drive
/// the API server, an injectable deterministic clock so
/// integration tests can advance requeue budgets without waiting
/// wall-clock time, a per-fleet rate limiter that spreads bursts
/// across a sliding window, a `tracing`-annotated span carrying
/// the requeue reason for post-hoc audit — lands at THIS ONE
/// substrate primitive and every downstream retry sink across
/// the two active reconciler crates inherits the upgrade
/// mechanically. No per-site edit at any of the 35 listed
/// callers or at future consumers (a new phase handler, a new
/// controller crate, a per-Kind retry sink).
///
/// Sibling to the timed-decision primitives in [`crate::time`]
/// on the "second-count → typed timed value" axis: `seconds_ago(N)
/// -> DateTime<Utc>` seeds a wall-clock anchor `N` seconds in the
/// past for `elapsed_since` consumers; `after_secs(N) -> Action`
/// seeds a kube-runtime requeue `N` seconds in the future. Both
/// carry the same "`u64` seconds is the workspace's canonical
/// short-time unit" invariant so a switch to a finer-grained unit
/// (a millisecond-precision retry budget for tight probes) would
/// land at both primitives together rather than as scattered per-
/// site conversions.
#[must_use]
pub fn after_secs(secs: u64) -> Action {
    Action::requeue(Duration::from_secs(secs))
}

/// Second-count budget for the "fast re-poll after a state change"
/// requeue intent (1s). The wire-form value binds the
/// `Ok(after_secs(TICK_RETRY))` / `Ok(after_secs(1))` chains every
/// FSM handler tail + `controller.rs` signal-consumed / deletion-
/// preempt arm exit through — pre-lift restated as the `TICK_RETRY`
/// local const in `tatara-reconciler::phase_machine` and as a bare
/// `1` literal in `tatara-reconciler::controller`, past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold across two files that
/// each named the same intent independently. Post-lift both bind
/// through this ONE workspace-wide constant so a future budget
/// normalization (a bounded-rate limiter's floor, a jitter overlay's
/// pivot) lands at ONE owner.
pub const TICK_SECONDS: u64 = 1;

/// Second-count budget for the "short retry after transient
/// failure" requeue intent (5s). Bound by the `SHORT_RETRY` const in
/// `tatara-reconciler::phase_machine` (execing evaluation / running
/// evaluation / attested probe / reconverging), and by the bare `5`
/// literal in `tatara-pool-reconciler::controller_allocation`'s
/// bind-retry arm, past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// threshold across two files.
pub const SHORT_RETRY_SECONDS: u64 = 5;

/// Second-count budget for the "steady-state heartbeat" requeue
/// intent (30s). Bound by the `HEARTBEAT` const in
/// `tatara-reconciler::phase_machine` (7 handler-tail sites), by the
/// bare `30` literal in `tatara-reconciler::controller`'s
/// reconcile-error sink + `error_policy`, and by the bare `30`
/// literal in `tatara-reconciler::table_controller`'s ProcessTable
/// heartbeat + `error_policy`, past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold across three files that each named the
/// same intent independently.
pub const HEARTBEAT_SECONDS: u64 = 30;

/// The "fast re-poll" requeue action — `after_secs(TICK_SECONDS)`.
///
/// The ONE substrate owner of the "immediate re-poll after a signal
/// consumption or state-change latch" requeue intent every FSM
/// handler tail and top-level dispatcher exits through. Pre-lift the
/// intent was hand-authored at 12 workspace-wide sites past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold across two files, each
/// naming the same intent independently:
///
/// * `tatara-reconciler::phase_machine` — 10 sites fed the local
///   `const TICK_RETRY: u64 = 1;` binding at pending / forking-
///   heartbeat / attested-heartbeat / exiting-tail / zombie-poll /
///   reaped handler tails.
/// * `tatara-reconciler::controller` — 2 sites fed a bare `1`
///   literal at the deletion-preempt requeue and the signal-
///   consumed requeue (both semantically "re-poll immediately").
///
/// Post-lift each callsite reads `tatara_process::requeue::tick()`
/// and the intent → second-count → requeue chain lives at ONE
/// substrate owner. Peer to the [`short_retry`] and [`heartbeat`]
/// helpers on the "named requeue intent" axis; sibling to the raw-
/// second-count [`after_secs`] on the "typed requeue constructor"
/// axis (the semantic helpers here compose through it).
///
/// A future normalization on the tick intent alone (e.g. a
/// millisecond-precision path for tight controller inner loops,
/// bounded by a workspace-wide floor) lands at THIS ONE substrate
/// primitive and every downstream tick site inherits the upgrade
/// mechanically. No per-site edit at any of the 12 listed callers
/// or at future consumers (a new phase handler tail, a new signal
/// arm, a new top-level dispatcher).
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the intent recurred at 12 hand-authored production sites past
/// the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to
/// ONE owner here). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — the intent → second-count mapping lives at
/// ONE typed algebra projection; a regression that drifted the
/// second-count would fail at the byte-shape pin below rather than
/// as silent operator-visible cadence skew across the 12
/// downstream requeue sites).
#[must_use]
pub fn tick() -> Action {
    after_secs(TICK_SECONDS)
}

/// The "short retry after transient failure" requeue action —
/// `after_secs(SHORT_RETRY_SECONDS)`.
///
/// The ONE substrate owner of the "short back-off after a
/// transient error / not-yet-ready state" requeue intent. Pre-lift
/// bound at 5 workspace-wide sites past the ★★ PRIME-DIRECTIVE
/// ≥ 2 duplication threshold across two files:
///
/// * `tatara-reconciler::phase_machine` — 4 sites fed the local
///   `const SHORT_RETRY: u64 = 5;` at handler branches that
///   re-check a slow-converging condition (execing evaluator /
///   running evaluator retry / attested-attestation retry /
///   reconverging spec-hash check).
/// * `tatara-pool-reconciler::controller_allocation` — 1 site fed
///   a bare `5` literal at the Bind-arm patch-failure retry.
///
/// Post-lift each callsite reads
/// `tatara_process::requeue::short_retry()`. Peer to [`tick`] and
/// [`heartbeat`] on the "named requeue intent" axis; each future
/// consumer that observes a transient error and wants a short
/// bounded retry lands as ONE new callsite here instead of another
/// hand-authored `after_secs(5)` chain.
#[must_use]
pub fn short_retry() -> Action {
    after_secs(SHORT_RETRY_SECONDS)
}

/// The "steady-state heartbeat" requeue action —
/// `after_secs(HEARTBEAT_SECONDS)`.
///
/// The ONE substrate owner of the "default periodic reconcile
/// heartbeat" requeue intent. Pre-lift bound at 11 workspace-wide
/// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold
/// across three files:
///
/// * `tatara-reconciler::phase_machine` — 7 sites fed the local
///   `const HEARTBEAT: u64 = 30;` at handler tails that maintain
///   steady-state re-observation (forking / execing / running /
///   attested / reconverging / releasing / failed).
/// * `tatara-reconciler::controller` — 2 sites fed a bare `30`
///   literal at the reconcile-error sink and the `error_policy`
///   return (both semantically "back off to the default
///   heartbeat after an error").
/// * `tatara-reconciler::table_controller` — 2 sites fed a bare
///   `30` literal at the ProcessTable heartbeat return and its
///   `error_policy` return.
///
/// Post-lift each callsite reads
/// `tatara_process::requeue::heartbeat()`. Peer to [`tick`] and
/// [`short_retry`] on the "named requeue intent" axis; each future
/// controller that wants the workspace-canonical heartbeat cadence
/// lands as ONE new callsite here instead of another hand-authored
/// `after_secs(30)` chain, and a future workspace-wide heartbeat
/// re-tuning (a config-slot override, a per-controller adaptive
/// cadence) lands at THIS primitive rather than as a scatter-
/// gather sweep across every reconciler.
#[must_use]
pub fn heartbeat() -> Action {
    after_secs(HEARTBEAT_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── after_secs substrate pins ─────────────────────────────────────
    //
    // Bind [`after_secs`] at fail-before-pass-after granularity so a
    // regression that swapped the unit (`from_millis` instead of
    // `from_secs`), flipped the requeue constructor (`Action::await_change`
    // instead of `Action::requeue`), dropped the return through a stale
    // hardcoded fallback, or reshaped the return form (an owned `Duration`
    // instead of the `Action`) surfaces HERE rather than as silent
    // operator-visible drift at the 35 downstream consumer sites across
    // both active reconciler crates.

    #[test]
    fn after_secs_returns_action_carrying_the_second_count_duration() {
        // Primary shape asserted end-to-end: the returned Action carries
        // the exact `Duration::from_secs(secs)` value on its requeue slot.
        // `Action` doesn't expose its inner Duration as a public getter,
        // so verify the shape via `Debug` — `Action::requeue(Duration::
        // from_secs(30))` formats with `30s` somewhere in the debug output;
        // a regression that unit-swapped to `from_millis` would show `30ms`.
        let debug = format!("{:?}", after_secs(30));
        assert!(
            debug.contains("30s") || debug.contains("30 s") || debug.contains("30.0s"),
            "after_secs(30) debug {debug:?} must mention 30s — a regression that swapped the unit would drift here"
        );
    }

    #[test]
    fn after_secs_zero_returns_action_that_requeues_immediately() {
        // Boundary corner: `secs = 0` yields `Action::requeue(Duration::
        // ZERO)`, the "requeue this reconciliation immediately" shape
        // kube-runtime treats as a tight re-tick. A regression that
        // clamped the zero arm to a minimum (a "no requeue faster than
        // 1s" clamp) would silently delay every tight-loop consumer that
        // legitimately wants an immediate re-tick — the signal-consumed
        // arm in `controller.rs`, a pending-phase check that just
        // observed a wire update.
        let debug = format!("{:?}", after_secs(0));
        assert!(
            debug.contains("0s") || debug.contains("0 s") || debug.contains("0ns"),
            "after_secs(0) debug {debug:?} must reflect a zero requeue duration — a regression that clamped to a minimum would drift here"
        );
    }

    #[test]
    fn after_secs_matches_hand_authored_pre_lift_chain_shape() {
        // Byte-identical parity with the pre-lift `Action::requeue(
        // Duration::from_secs(N))` block that all 35 hand-authored
        // callsites restated verbatim, swept across the representative
        // second-counts every pre-lift consumer used:
        //
        //   1  → deletion-preempt + signal re-poll (`controller.rs`)
        //   5  → bind-retry (`controller_allocation.rs`)
        //   15 → cluster fallback (`controller_pool` / `controller_allocation`)
        //   30 → reconcile-error + table-controller (`controller.rs`,
        //        `table_controller.rs`)
        //   60 → `TICK_RETRY` / `HEARTBEAT` scale (`phase_machine.rs`)
        //   3600 → hour-scale sanity check
        //
        // `Action` doesn't derive `PartialEq`, so parity is asserted via
        // `Debug` output — both blocks build the exact same requeue
        // Action, so their debug reps must string-equal.
        for secs in [1_u64, 5, 15, 30, 60, 3_600] {
            let composed = format!("{:?}", after_secs(secs));
            let hand_authored = format!("{:?}", Action::requeue(Duration::from_secs(secs)));
            assert_eq!(
                composed, hand_authored,
                "after_secs({secs}) debug {composed:?} must match hand-authored Action::requeue(Duration::from_secs({secs})) debug {hand_authored:?} — a regression that reshaped either link would drift here"
            );
        }
    }

    #[test]
    fn after_secs_composes_at_reconcile_return_position() {
        // The canonical consumer shape end-to-end: a `reconcile` fn
        // returns `Ok(after_secs(N))` where the caller expects a
        // `Result<Action, _>` back. A regression that returned a
        // different type (an owned `Duration`, a `Result` wrapper,
        // an `Option<Action>`) would fail to type-check at every
        // consumer. Pin the return shape by explicitly annotating
        // the `Ok` arm so a regression that widened the return
        // surfaces HERE rather than as a workspace-wide type error.
        let out: Result<Action, ()> = Ok(after_secs(5));
        assert!(out.is_ok());
    }

    #[test]
    fn after_secs_composes_at_error_policy_return_position() {
        // Peer to the reconcile-return shape: `error_policy` returns
        // bare `Action` (no `Result` wrapper). Pin that shape too so a
        // regression that changed the return to `Result<Action, _>`
        // would surface HERE rather than as a workspace-wide type
        // error at every `Controller::run(...).error_policy(...)`
        // callsite.
        let _out: Action = after_secs(30);
    }

    #[test]
    fn after_secs_accepts_config_slot_typed_as_u64() {
        // The `ctx.config.heartbeat_seconds` slot and every
        // `TICK_RETRY` / `HEARTBEAT` / `SHORT_RETRY` module-level
        // `pub const N: u64` binding is typed as `u64`; the primitive
        // must accept those without a cast. A regression that
        // narrowed the parameter to `u32` or widened it to `i64`
        // would break either the `phase_machine.rs` const-fed
        // callsites or the `controller.rs` config-fed slot at the
        // migration boundary.
        let heartbeat_seconds: u64 = 60;
        let _out: Action = after_secs(heartbeat_seconds);
    }

    // ─── Named requeue-intent helpers ───────────────────────────────
    //
    // Fail-before-pass-after pins for [`tick`], [`short_retry`], and
    // [`heartbeat`]. Each helper binds ONE named requeue intent to
    // its second-count budget through [`after_secs`]. The pins here
    // catch three regression axes at the substrate boundary:
    //
    //   1. The intent → second-count binding drifts (e.g. `heartbeat`
    //      shifts from 30s to 60s under a partial re-tuning), silently
    //      changing the reconcile cadence across every downstream
    //      consumer.
    //   2. A helper is retargeted onto a different underlying primitive
    //      (`Action::await_change` instead of `Action::requeue`) or
    //      unit (`from_millis` instead of `from_secs`), silently
    //      inverting or scale-shifting the requeue semantics.
    //   3. The public second-count constant and the helper drift out
    //      of lockstep (the helper still returns 30s but the const
    //      reports 60s, misleading any consumer that reads the const
    //      to build its own tuned requeue).

    #[test]
    fn tick_binds_to_one_second_and_matches_pre_lift_hand_authored_chain() {
        // The `TICK_RETRY = 1` local const the pre-lift 10
        // `phase_machine.rs` sites + the bare `1` literal the pre-
        // lift 2 `controller.rs` sites walked, both now bound at ONE
        // owner here. A drift would silently change the tick cadence
        // across all 12 downstream sites.
        assert_eq!(TICK_SECONDS, 1);
        let composed = format!("{:?}", tick());
        let hand_authored = format!("{:?}", after_secs(1));
        assert_eq!(
            composed, hand_authored,
            "tick() must byte-shape-match after_secs(1); the intent → second-count binding drifted",
        );
    }

    #[test]
    fn short_retry_binds_to_five_seconds_and_matches_pre_lift_hand_authored_chain() {
        // The `SHORT_RETRY = 5` local const the pre-lift 4
        // `phase_machine.rs` sites + the bare `5` literal the pre-
        // lift 1 `controller_allocation.rs` bind-retry site walked,
        // both now bound at ONE owner here.
        assert_eq!(SHORT_RETRY_SECONDS, 5);
        let composed = format!("{:?}", short_retry());
        let hand_authored = format!("{:?}", after_secs(5));
        assert_eq!(
            composed, hand_authored,
            "short_retry() must byte-shape-match after_secs(5); the intent → second-count binding drifted",
        );
    }

    #[test]
    fn heartbeat_binds_to_thirty_seconds_and_matches_pre_lift_hand_authored_chain() {
        // The `HEARTBEAT = 30` local const the pre-lift 7
        // `phase_machine.rs` sites + the bare `30` literal the pre-
        // lift 4 `controller.rs` / `table_controller.rs` sites
        // walked, all now bound at ONE owner here. A drift would
        // silently change the heartbeat cadence across all 11
        // downstream sites, potentially exceeding a per-cluster
        // reconcile-rate budget or delaying steady-state
        // re-observation past a control-loop deadline.
        assert_eq!(HEARTBEAT_SECONDS, 30);
        let composed = format!("{:?}", heartbeat());
        let hand_authored = format!("{:?}", after_secs(30));
        assert_eq!(
            composed, hand_authored,
            "heartbeat() must byte-shape-match after_secs(30); the intent → second-count binding drifted",
        );
    }

    #[test]
    fn named_requeue_intents_compose_at_reconcile_return_position() {
        // Every helper returns `Action` and composes at the
        // canonical `Result<Action, _>` return position every
        // `reconcile` fn signature expects. A regression that
        // widened any helper's return (an `Option<Action>` sink,
        // a `Result` wrapper) would fail to type-check here rather
        // than at the 28 downstream migration sites.
        let _tick_ok: Result<Action, ()> = Ok(tick());
        let _short_retry_ok: Result<Action, ()> = Ok(short_retry());
        let _heartbeat_ok: Result<Action, ()> = Ok(heartbeat());
    }

    #[test]
    fn named_requeue_intents_compose_at_error_policy_return_position() {
        // Peer to the reconcile-return shape: `error_policy` returns
        // bare `Action` (no `Result` wrapper). `heartbeat()` is the
        // canonical error-policy return for both `controller.rs` and
        // `table_controller.rs`; pin the shape so a regression that
        // added a wrapper surfaces HERE rather than at the two
        // `error_policy` sites.
        let _tick: Action = tick();
        let _short_retry: Action = short_retry();
        let _heartbeat: Action = heartbeat();
    }

    #[test]
    fn named_requeue_intents_project_distinct_second_counts() {
        // Cross-intent coherence pin: the three named intents must
        // resolve to three distinct second-counts. A regression that
        // collapsed two intents onto the same constant (e.g.
        // `TICK_SECONDS == SHORT_RETRY_SECONDS` after an over-eager
        // "unify budgets" refactor) would fold a semantic distinction
        // into a numeric one, and the 28 downstream sites would
        // silently lose the intent name's meaning even though the
        // helpers still compile.
        assert_ne!(TICK_SECONDS, SHORT_RETRY_SECONDS);
        assert_ne!(TICK_SECONDS, HEARTBEAT_SECONDS);
        assert_ne!(SHORT_RETRY_SECONDS, HEARTBEAT_SECONDS);
        assert!(
            TICK_SECONDS < SHORT_RETRY_SECONDS && SHORT_RETRY_SECONDS < HEARTBEAT_SECONDS,
            "named requeue intents must project onto strictly increasing second-counts (tick < short_retry < heartbeat); a regression that flipped the ordering would silently invert the retry-cadence hierarchy",
        );
    }
}
