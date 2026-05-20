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
        f.write_str(match self {
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
        })
    }
}

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
        for p in [
            Pending,
            Forking,
            Execing,
            Running,
            Reconverging,
            Releasing,
            Exiting,
            Zombie,
            Reaped,
        ] {
            assert!(!p.is_terminal_reached(), "{p:?} is not a terminal gate");
        }
    }

    #[test]
    fn releasing_can_only_be_entered_from_terminal_gates() {
        // Releasing has exactly two legal entries — the terminal-
        // reached gates. Anything else is a state-machine bug.
        let entries: Vec<_> = [
            Pending,
            Forking,
            Execing,
            Running,
            Attested,
            Reconverging,
            Releasing,
            Exiting,
            Failed,
            Zombie,
            Reaped,
        ]
        .into_iter()
        .filter(|p| p.can_transition_to(Releasing))
        .collect();
        assert_eq!(entries, vec![Attested, Failed]);
    }

    #[test]
    fn reaped_is_sink() {
        assert!(Reaped.is_terminal());
        for next in [
            Pending,
            Forking,
            Execing,
            Running,
            Attested,
            Reconverging,
            Releasing,
            Exiting,
            Failed,
            Zombie,
        ] {
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
}
