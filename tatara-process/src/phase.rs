//! Unix process phases — authoritative state machine.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The Unix-authentic phase a Process is in.
///
/// Canonical transitions:
/// ```text
/// Pending → Forking → Execing → Running → Attested
///                                       ↘ Failed
/// Attested → Reconverging → Execing                       (SIGHUP, no zombie)
/// Attested → Releasing  → Exiting → Zombie → Reaped       (export-then-SIGTERM)
/// Attested → Exiting    → Zombie → Reaped                 (no-exports SIGTERM)
/// Failed   → Releasing  → Zombie → Reaped                 (post-mortem exports)
/// Failed   → Zombie     → Reaped                          (no-exports failed)
/// Running  → Exiting    → Zombie → Reaped                 (early SIGTERM, no exports)
/// Running  → Failed                                       (non-zero exit)
/// ```
///
/// `Releasing` is the export window — the reconciler runs declared
/// `ExportSpec`s (via tatara-export-worker Jobs) between the
/// terminal phase reached (`Attested` or `Failed`) and `Exiting` /
/// `Zombie`. A Process with no `lifetime.ephemeral.exports`, or
/// where no export's trigger matches the phase reached, skips
/// `Releasing` entirely. See [`crate::export`] + [`crate::lifetime`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum ProcessPhase {
    /// Admitted; PID not assigned yet.
    Pending,
    /// PID assigned in ProcessTable; parent linked; content hash computed.
    Forking,
    /// RENDER phase — evaluating Nix / expanding Lisp / rendering Helm;
    /// emitting Kustomization + HelmRelease CRs.
    Execing,
    /// Flux resources applied; boundary preconditions being checked.
    Running,
    /// All postconditions hold; three-pillar attestation written.
    Attested,
    /// SIGHUP received or drift detected; returning to Execing.
    Reconverging,
    /// Export window — running declared `ExportSpec`s before SIGTERM.
    /// Each export becomes a typed Job; the Process advances only
    /// when every Job has reached a terminal state. Failures here
    /// short-circuit straight to `Zombie` (the export attempt itself
    /// is attested; partial-success is fine for best-effort channels).
    Releasing,
    /// SIGTERM received; graceful shutdown; children draining.
    Exiting,
    /// Exited non-zero; awaiting reap.
    Failed,
    /// Exited; children gone; finalizer not yet released.
    Zombie,
    /// Finalizer released; K8s GC will remove.
    Reaped,
}

impl Default for ProcessPhase {
    fn default() -> Self {
        Self::Pending
    }
}

impl ProcessPhase {
    /// The closed set of phases — single source of truth that drives
    /// `as_str` / Display / `FromStr` so adding a variant updates every
    /// projection at once (and the `display_matches_as_str` +
    /// `all_phases_roundtrip_via_as_str` tests pin the bridge). Also
    /// used by the test sites that need to sweep every-other-variant
    /// (`reaped_is_sink`, `releasing_can_only_be_entered_from_terminal_gates`,
    /// `terminal_reached_gates_are_attested_and_failed`), so a new
    /// variant lands in ALL once and reaches every test by iteration
    /// rather than by per-test array maintenance.
    pub const ALL: [Self; 11] = [
        Self::Pending,
        Self::Forking,
        Self::Execing,
        Self::Running,
        Self::Attested,
        Self::Reconverging,
        Self::Releasing,
        Self::Exiting,
        Self::Failed,
        Self::Zombie,
        Self::Reaped,
    ];

    /// Canonical PascalCase wire-format projection. Used by Display
    /// (single source of truth) and by `FromStr` to identify the
    /// variant from its annotation / status-field representation.
    /// The serde rename derives produce the same form on the JSON
    /// boundary; this method exposes it to Rust callers (logs,
    /// annotation values, error messages) without re-serializing.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Forking => "Forking",
            Self::Execing => "Execing",
            Self::Running => "Running",
            Self::Attested => "Attested",
            Self::Reconverging => "Reconverging",
            Self::Releasing => "Releasing",
            Self::Exiting => "Exiting",
            Self::Failed => "Failed",
            Self::Zombie => "Zombie",
            Self::Reaped => "Reaped",
        }
    }

    /// True if the phase is a terminal sink with no further transitions.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Reaped)
    }

    /// True if the process has reached a running state (Running or Attested).
    pub const fn is_running(self) -> bool {
        matches!(self, Self::Running | Self::Attested)
    }

    /// True if the process is still eligible to receive SIGHUP/SIGUSR* signals.
    /// `Releasing` is alive — the Process hasn't been SIGTERM'd yet; its
    /// children (export Jobs) are running.
    pub const fn is_alive(self) -> bool {
        !matches!(self, Self::Zombie | Self::Reaped | Self::Failed)
    }

    /// True if the phase is the export window — declared `ExportSpec`s
    /// run here before SIGTERM. Reserved for the reconciler's
    /// `handle_releasing` step + tatara-export-worker Job emission.
    pub const fn is_releasing(self) -> bool {
        matches!(self, Self::Releasing)
    }

    /// True if the phase is a terminal-reached gate (`Attested` or
    /// `Failed`) — the points where the reconciler decides whether
    /// to enter `Releasing`, jump straight to `Exiting`/`Zombie`, or
    /// stay (for inspection per `TeardownPolicy`).
    pub const fn is_terminal_reached(self) -> bool {
        matches!(self, Self::Attested | Self::Failed)
    }

    /// True if the phase transition `self → next` is legal.
    pub const fn can_transition_to(self, next: Self) -> bool {
        use ProcessPhase::*;
        matches!(
            (self, next),
            (Pending, Forking)
                | (Forking, Execing)
                | (Execing, Running)
                | (Execing, Failed)
                | (Running, Attested)
                | (Running, Exiting)
                | (Running, Failed)
                | (Running, Reconverging)
                | (Attested, Reconverging)
                | (Attested, Releasing)
                | (Attested, Exiting)
                | (Failed, Releasing)
                | (Failed, Zombie)
                | (Releasing, Exiting)
                | (Releasing, Zombie)
                | (Reconverging, Execing)
                | (Exiting, Zombie)
                | (Zombie, Reaped)
        )
    }
}

impl std::fmt::Display for ProcessPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ProcessPhase {
    type Err = UnknownPhase;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for phase in Self::ALL {
            if s == phase.as_str() {
                return Ok(phase);
            }
        }
        Err(UnknownPhase(s.to_string()))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown process phase: {0}")]
pub struct UnknownPhase(pub String);

#[cfg(test)]
mod tests {
    use super::ProcessPhase::*;

    #[test]
    fn canonical_path_is_legal() {
        assert!(Pending.can_transition_to(Forking));
        assert!(Forking.can_transition_to(Execing));
        assert!(Execing.can_transition_to(Running));
        assert!(Running.can_transition_to(Attested));
        assert!(Attested.can_transition_to(Reconverging));
        assert!(Reconverging.can_transition_to(Execing));
        assert!(Attested.can_transition_to(Exiting));
        assert!(Exiting.can_transition_to(Zombie));
        assert!(Zombie.can_transition_to(Reaped));
    }

    /// Releasing path — Attested or Failed may detour through the
    /// export window before terminating. Releasing is itself a
    /// legal source for Exiting (happy path) or Zombie (export-
    /// worker terminal-failure shortcut).
    #[test]
    fn releasing_path_is_legal() {
        assert!(Attested.can_transition_to(Releasing));
        assert!(Failed.can_transition_to(Releasing));
        assert!(Releasing.can_transition_to(Exiting));
        assert!(Releasing.can_transition_to(Zombie));
        // Releasing is alive — children (export Jobs) still running.
        assert!(Releasing.is_alive());
        // Releasing is not a terminal-reached gate.
        assert!(!Releasing.is_terminal_reached());
    }

    #[test]
    fn terminal_reached_gates_are_attested_and_failed() {
        assert!(Attested.is_terminal_reached());
        assert!(Failed.is_terminal_reached());
        // Sweep every other variant via ALL so a future variant is
        // covered automatically (was a hand-maintained 9-entry array).
        for p in super::ProcessPhase::ALL {
            if matches!(p, Attested | Failed) {
                continue;
            }
            assert!(!p.is_terminal_reached(), "{p:?} is not a terminal gate");
        }
    }

    #[test]
    fn releasing_can_only_be_entered_from_terminal_gates() {
        // Releasing has exactly two legal entries — the terminal-
        // reached gates. Anything else is a state-machine bug.
        // ALL is the source of truth for the candidate set.
        let entries: Vec<_> = super::ProcessPhase::ALL
            .into_iter()
            .filter(|p| p.can_transition_to(Releasing))
            .collect();
        assert_eq!(entries, vec![Attested, Failed]);
    }

    #[test]
    fn reaped_is_sink() {
        assert!(Reaped.is_terminal());
        // Sweep every non-Reaped variant via ALL so a new phase
        // pins the sink-ness invariant automatically.
        for next in super::ProcessPhase::ALL {
            if next == Reaped {
                continue;
            }
            assert!(
                !Reaped.can_transition_to(next),
                "Reaped → {next:?} should be illegal"
            );
        }
    }

    #[test]
    fn cannot_skip_forking() {
        assert!(!Pending.can_transition_to(Execing));
        assert!(!Pending.can_transition_to(Running));
    }

    #[test]
    fn running_is_alive() {
        assert!(Running.is_alive());
        assert!(Attested.is_alive());
        assert!(!Zombie.is_alive());
        assert!(!Reaped.is_alive());
    }

    // ── closed-set algebra contracts (ALL × as_str × FromStr) ────────

    /// Every variant in ALL round-trips through `as_str` ↔ `FromStr`.
    /// Adding a variant without extending `as_str` (or vice versa)
    /// fails here.
    #[test]
    fn all_phases_roundtrip_via_as_str() {
        use std::str::FromStr;
        for phase in super::ProcessPhase::ALL {
            assert_eq!(
                super::ProcessPhase::from_str(phase.as_str()).unwrap(),
                phase,
                "round-trip failed for {phase:?}",
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. If a
    /// reviewer accidentally re-introduces an inline match in
    /// Display, this test would fail the moment a variant rename
    /// touches one site but not the other.
    #[test]
    fn display_matches_as_str() {
        for phase in super::ProcessPhase::ALL {
            assert_eq!(phase.to_string(), phase.as_str());
        }
    }

    /// `ALL` is the source of truth — pin its closure so a variant
    /// added without an `ALL` entry fails here (uniqueness check)
    /// before drifting `FromStr` or the sweep tests above. The arity
    /// is asserted by the array type itself (`[Self; 11]`).
    #[test]
    fn all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for phase in super::ProcessPhase::ALL {
            assert!(seen.insert(phase), "duplicate variant in ALL: {phase:?}");
        }
        assert_eq!(seen.len(), super::ProcessPhase::ALL.len());
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — empty / lowercased / typo / unrelated — and the
    /// error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    #[test]
    fn unknown_phase_errors() {
        use std::str::FromStr;
        for bad in ["", "attested", "FAILED", "Cancelled", "Reapped"] {
            let err = super::ProcessPhase::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }
}
