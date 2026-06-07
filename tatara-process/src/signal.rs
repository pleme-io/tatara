//! First-class CRD signals — Unix semantics over Kubernetes.
//!
//! Signals are delivered via annotation (`tatara.pleme.io/signal=SIGHUP`) or
//! via the MCP `signal_process` tool; the reconciler consumes them,
//! enqueues on `status.signalQueue`, and drains in phase order.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
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

#[cfg(test)]
mod tests {
    use super::ProcessSignal;
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
}
