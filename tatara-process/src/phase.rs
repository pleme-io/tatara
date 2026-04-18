//! Unix process phases — authoritative state machine.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The Unix-authentic phase a Process is in.
///
/// Canonical transitions:
/// ```text
/// Pending → Forking → Execing → Running → Attested
///                                       ↘ Failed
/// Attested → Reconverging → Execing          (SIGHUP, no zombie)
/// Running  → Exiting      → Zombie → Reaped  (SIGTERM path)
/// Running  → Failed       → Zombie → Reaped  (non-zero exit)
/// ```
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
    pub const fn is_alive(self) -> bool {
        !matches!(self, Self::Zombie | Self::Reaped | Self::Failed)
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
                | (Attested, Exiting)
                | (Reconverging, Execing)
                | (Exiting, Zombie)
                | (Failed, Zombie)
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

    #[test]
    fn reaped_is_sink() {
        assert!(Reaped.is_terminal());
        for next in [Pending, Forking, Execing, Running, Attested, Reconverging, Exiting, Failed, Zombie] {
            assert!(!Reaped.can_transition_to(next), "Reaped → {next:?} should be illegal");
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
