//! First-class CRD signals — Unix semantics over Kubernetes.
//!
//! Signals are delivered via annotation (`tatara.pleme.io/signal=SIGHUP`) or
//! via the MCP `signal_process` tool; the reconciler consumes them,
//! enqueues on `status.signalQueue`, and drains in phase order.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::phase::ProcessPhase;

/// The first-class signals the controller honors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcessSignal {
    /// Reconfigure — re-enter Execing without termination.
    /// Fires on spec change, drift detection, or manual invocation.
    Sighup,
    /// Graceful terminate — finalizer path with children draining first.
    Sigterm,
    /// Force terminate — `grace_period_seconds: 0` on all owned resources.
    Sigkill,
    /// Force re-attestation without spec change — recomputes three-pillar hash.
    Sigusr1,
    /// Force remediation — invokes kensa remediation hooks.
    Sigusr2,
    /// Pause reconciliation — fixed-point driver is suspended.
    Sigstop,
    /// Resume reconciliation after SIGSTOP.
    Sigcont,
}

impl ProcessSignal {
    /// The closed set of signals — single source of truth that
    /// drives `as_str` / `short_str` / `FromStr` so adding a variant
    /// updates every projection at once (and the
    /// `short_str_strips_sig_prefix` + `all_signals_roundtrip_*`
    /// tests pin the bridge).
    pub const ALL: [Self; 7] = [
        Self::Sighup,
        Self::Sigterm,
        Self::Sigkill,
        Self::Sigusr1,
        Self::Sigusr2,
        Self::Sigstop,
        Self::Sigcont,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sighup => "SIGHUP",
            Self::Sigterm => "SIGTERM",
            Self::Sigkill => "SIGKILL",
            Self::Sigusr1 => "SIGUSR1",
            Self::Sigusr2 => "SIGUSR2",
            Self::Sigstop => "SIGSTOP",
            Self::Sigcont => "SIGCONT",
        }
    }

    /// The short alias accepted by `FromStr` — the canonical
    /// `as_str()` form with the leading `"SIG"` stripped (`"HUP"`,
    /// `"TERM"`, …). The `short_str_strips_sig_prefix` test asserts
    /// this contract structurally so the two projections cannot
    /// drift.
    pub const fn short_str(self) -> &'static str {
        match self {
            Self::Sighup => "HUP",
            Self::Sigterm => "TERM",
            Self::Sigkill => "KILL",
            Self::Sigusr1 => "USR1",
            Self::Sigusr2 => "USR2",
            Self::Sigstop => "STOP",
            Self::Sigcont => "CONT",
        }
    }
}

impl std::fmt::Display for ProcessSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ProcessSignal {
    type Err = UnknownSignal;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let upper = s.to_ascii_uppercase();
        for sig in Self::ALL {
            if upper == sig.as_str() || upper == sig.short_str() {
                return Ok(sig);
            }
        }
        Err(UnknownSignal(upper))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown signal: {0}")]
pub struct UnknownSignal(pub String);

/// How a Process handles SIGHUP.
///
/// Sibling closed-set lifts on the same `ProcessSpec` axis:
/// [`ProcessSignal::ALL`] (parser triad), [`crate::phase::ProcessPhase::ALL`]
/// (target codomain), [`crate::spec::MustReachPhase::ALL`] (typed subset of
/// ProcessPhase).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
pub enum SighupStrategy {
    /// Running → Reconverging → Execing without tearing down resources.
    #[default]
    Reconverge,
    /// Running → Exiting → Reaped → Pending (full respawn).
    Restart,
    /// Ignore the signal.
    Noop,
}

impl SighupStrategy {
    /// The closed set of SIGHUP strategies — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad and the typed
    /// `sighup_target` projection. Adding a fourth strategy (e.g. a
    /// future `Suspend` that maps SIGHUP to `SignalEffect::Suspend`)
    /// lands at one `ALL` entry, one `as_str` arm, and one
    /// `sighup_target` arm — exhaustively checked by the compiler
    /// (the `[Self; 3]` array literal forces the arity).
    pub const ALL: [Self; 3] = [Self::Reconverge, Self::Restart, Self::Noop];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim. Used by Display
    /// (single source of truth), by `FromStr` to identify the variant
    /// from its annotation / status-field representation, and by
    /// operator-facing diagnostic strings without re-serializing
    /// through `serde_json`. Pinned by
    /// `sighup_strategy_as_str_matches_serde`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reconverge => "Reconverge",
            Self::Restart => "Restart",
            Self::Noop => "Noop",
        }
    }

    /// Typed projection: when this Process receives SIGHUP while in a
    /// `is_running()` phase, which `ProcessPhase` does the reconciler
    /// transition into? `None` means SIGHUP is a no-op (the `Noop`
    /// strategy). The phase guard lives at the call site (the strategy
    /// itself doesn't know about phase), so the codomain is purely a
    /// function of the strategy variant.
    ///
    /// Used by `tatara_reconciler::signals::apply` to lift the three
    /// SIGHUP arms (Reconverge / Restart / Noop) into one
    /// projection-driven arm. A future variant lands at one
    /// `sighup_target` arm; `apply` doesn't change.
    ///
    /// Pinned by `sighup_target_truth_table` (per-variant codomain)
    /// AND by `sighup_target_projects_only_to_legal_sighup_transitions`
    /// (every `Some(target)` must be reachable from `Running` /
    /// `Attested` via `ProcessPhase::can_transition_to`).
    pub const fn sighup_target(self) -> Option<ProcessPhase> {
        match self {
            Self::Reconverge => Some(ProcessPhase::Reconverging),
            Self::Restart => Some(ProcessPhase::Exiting),
            Self::Noop => None,
        }
    }
}

impl std::fmt::Display for SighupStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SighupStrategy {
    type Err = UnknownSighupStrategy;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        <Self as tatara_lisp::ClosedSet>::parse_label(s)
    }
}

/// Plug [`SighupStrategy`] into the substrate-wide
/// [`tatara_lisp::ClosedSet`] trait — the four-method contract that
/// collapses the linear-sweep for-loop from this enum's
/// [`std::str::FromStr::from_str`] body into ONE place
/// ([`tatara_lisp::ClosedSet::parse_label`]'s default body) shared
/// with every other `tatara-process` closed-set implementor
/// ([`crate::phase::ProcessPhase`],
/// [`crate::compliance::VerificationPhase`],
/// [`crate::lifetime::TeardownPolicy`], …).
///
/// [`ProcessSignal`] is NOT a `ClosedSet` implementor — its `FromStr`
/// keys on a compound projection (`as_str` || `short_str`) with
/// case-insensitive uppercase normalization rather than a single
/// canonical label, the same exemption pattern as
/// [`tatara_lisp::CompilerSpecIoStage`]'s compound `(operation,
/// label)` key. Only [`SighupStrategy`] (single PascalCase label, no
/// normalization) plugs in here.
impl tatara_lisp::ClosedSet for SighupStrategy {
    const ALL: &'static [Self] = &Self::ALL;
    type Unknown = UnknownSighupStrategy;
    fn label(self) -> &'static str {
        Self::as_str(self)
    }
    fn make_unknown(s: &str) -> Self::Unknown {
        UnknownSighupStrategy(s.to_owned())
    }
}

/// Typed parse failure carrying the offending input verbatim so the
/// operator-facing diagnostic surfaces the bad value, not a normalized
/// form. Symmetric to [`UnknownSignal`], [`crate::phase::UnknownPhase`],
/// [`crate::spec::UnknownMustReachPhase`], and
/// [`crate::boundary::UnknownConditionKind`].
#[derive(Debug, thiserror::Error)]
#[error("unknown sighup strategy: {0}")]
pub struct UnknownSighupStrategy(pub String);

#[cfg(test)]
mod tests {
    use super::{ProcessSignal, SighupStrategy, UnknownSighupStrategy};
    use crate::phase::ProcessPhase;
    use std::str::FromStr;

    #[test]
    fn all_signals_roundtrip_canonical() {
        for sig in ProcessSignal::ALL {
            assert_eq!(ProcessSignal::from_str(sig.as_str()).unwrap(), sig);
        }
    }

    #[test]
    fn all_signals_roundtrip_short() {
        for sig in ProcessSignal::ALL {
            assert_eq!(ProcessSignal::from_str(sig.short_str()).unwrap(), sig);
        }
    }

    /// The structural contract that lets `FromStr` parse off two
    /// projections without drift: `short_str()` is exactly
    /// `as_str()` minus the leading `"SIG"`. Any new variant whose
    /// canonical form doesn't follow the `SIG*` convention has to
    /// either rename or override this test deliberately.
    #[test]
    fn short_str_strips_sig_prefix() {
        for sig in ProcessSignal::ALL {
            assert_eq!(sig.as_str().strip_prefix("SIG"), Some(sig.short_str()));
        }
    }

    /// `ALL` is the source of truth for the parser table — pin its
    /// closure so a variant added without an `ALL` entry fails here
    /// (via the uniqueness check) before drifting `FromStr`.
    #[test]
    fn all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for sig in ProcessSignal::ALL {
            assert!(seen.insert(sig), "duplicate variant in ALL: {sig:?}");
        }
        // The arity is asserted by the array type itself (`[Self; 7]`),
        // but the uniqueness check above + the const-array length is
        // what makes ALL a closed set rather than just a list.
        assert_eq!(seen.len(), ProcessSignal::ALL.len());
    }

    #[test]
    fn lowercase_short_form_accepted() {
        assert_eq!(
            ProcessSignal::from_str("hup").unwrap(),
            ProcessSignal::Sighup
        );
        assert_eq!(
            ProcessSignal::from_str("term").unwrap(),
            ProcessSignal::Sigterm
        );
    }

    #[test]
    fn unknown_errors() {
        let err = ProcessSignal::from_str("sigfoo").unwrap_err();
        // The error carries the uppercased input — preserves the
        // pre-refactor contract that `UnknownSignal` echoes the
        // normalized form, not the operator's casing.
        assert_eq!(err.0, "SIGFOO");
    }

    // ── closed-set algebra for SighupStrategy (ALL × as_str × FromStr ×
    //    sighup_target) ────────────────────────────────────────────────

    /// Structural well-formedness of [`SighupStrategy`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through `label ↔
    /// parse_label`, labels are pairwise distinct, `""` is outside the
    /// closed set) at ONE call site. Replaces the hand-derived
    /// `sighup_strategy_all_is_unique_and_complete` +
    /// `sighup_strategy_roundtrip_via_as_str` + the empty-input arm of
    /// `unknown_sighup_strategy_errors`. `FromStr` delegates to
    /// `<Self as tatara_lisp::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the reconciler hits when parsing a
    /// CRD `enum:`-validated value back to the typed strategy.
    #[test]
    fn sighup_strategy_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<SighupStrategy>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, not in a CRD
    /// `enum:` enumeration that quietly drifted away from the typed
    /// surface.
    #[test]
    fn sighup_strategy_as_str_matches_serde() {
        for strat in SighupStrategy::ALL {
            let serialized = serde_json::to_string(&strat)
                .expect("SighupStrategy serializes")
                .trim_matches('"')
                .to_string();
            assert_eq!(
                strat.as_str(),
                serialized,
                "as_str() must match serde output for {strat:?}",
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift.
    #[test]
    fn sighup_strategy_display_matches_as_str() {
        for strat in SighupStrategy::ALL {
            assert_eq!(strat.to_string(), strat.as_str());
        }
    }

    /// `FromStr` rejects strings outside the canonical projection
    /// (lowercased / typo / unrelated) — and the error echoes the
    /// input verbatim so the operator-facing diagnostic carries the
    /// offending value, not a normalized form. The empty-input arm is
    /// pinned by [`sighup_strategy_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownSighupStrategy`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_sighup_strategy_errors() {
        for bad in ["reconverge", "RESTART", "Suspend", "noop "] {
            let err = SighupStrategy::from_str(bad).unwrap_err();
            let UnknownSighupStrategy(payload) = &err;
            assert_eq!(payload, bad, "error payload should echo input verbatim");
        }
    }

    /// PROJECTION TRUTH TABLE: pin the per-variant codomain of
    /// `sighup_target`. A future variant addition lands here at one
    /// arm — and the compiler's closed-set match in `sighup_target`
    /// catches the missing arm before this test runs.
    #[test]
    fn sighup_target_truth_table() {
        assert_eq!(
            SighupStrategy::Reconverge.sighup_target(),
            Some(ProcessPhase::Reconverging)
        );
        assert_eq!(
            SighupStrategy::Restart.sighup_target(),
            Some(ProcessPhase::Exiting)
        );
        assert_eq!(SighupStrategy::Noop.sighup_target(), None);
    }

    /// SEMANTIC SUBSET CONTRACT: every `Some(target)` produced by
    /// `sighup_target` must be reachable from a `is_running()` phase
    /// (`Running` or `Attested`) via `ProcessPhase::can_transition_to`.
    /// This pins the contract `tatara_reconciler::signals::apply`
    /// relies on: the typed projection produces only phases the
    /// reconciler is allowed to transition into from the SIGHUP
    /// reception sites. A future `SighupStrategy::Refresh` that
    /// projected to `ProcessPhase::Pending` would FAIL here (Pending
    /// is not a legal successor of Running/Attested), forcing the
    /// author to either pick a legal target phase or extend
    /// `ProcessPhase::can_transition_to` deliberately.
    #[test]
    fn sighup_target_projects_only_to_legal_sighup_transitions() {
        for strat in SighupStrategy::ALL {
            if let Some(target) = strat.sighup_target() {
                let reachable_from_running = ProcessPhase::Running.can_transition_to(target);
                let reachable_from_attested = ProcessPhase::Attested.can_transition_to(target);
                assert!(
                    reachable_from_running || reachable_from_attested,
                    "{strat:?}.sighup_target() = {target:?} must be reachable from \
                     Running or Attested via can_transition_to",
                );
            }
        }
    }

    /// PROJECTION INJECTIVITY: distinct variants that produce `Some`
    /// project to distinct `ProcessPhase`s. Pairing this with the
    /// reachability contract above forces a future SIGHUP strategy
    /// variant to land on a fresh legal SIGHUP-target phase (or to
    /// project to `None` and be a deliberate no-op).
    #[test]
    fn sighup_target_projection_is_injective() {
        let mut seen = std::collections::HashSet::new();
        for strat in SighupStrategy::ALL {
            if let Some(target) = strat.sighup_target() {
                assert!(
                    seen.insert(target),
                    "two variants project to the same ProcessPhase: {target:?}",
                );
            }
        }
    }
}
