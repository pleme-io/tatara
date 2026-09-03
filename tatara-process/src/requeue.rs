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
}
