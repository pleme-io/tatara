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
}

impl std::fmt::Display for ProcessSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ProcessSignal {
    type Err = UnknownSignal;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "SIGHUP" | "HUP" => Ok(Self::Sighup),
            "SIGTERM" | "TERM" => Ok(Self::Sigterm),
            "SIGKILL" | "KILL" => Ok(Self::Sigkill),
            "SIGUSR1" | "USR1" => Ok(Self::Sigusr1),
            "SIGUSR2" | "USR2" => Ok(Self::Sigusr2),
            "SIGSTOP" | "STOP" => Ok(Self::Sigstop),
            "SIGCONT" | "CONT" => Ok(Self::Sigcont),
            other => Err(UnknownSignal(other.to_string())),
        }
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
    fn roundtrip_via_string() {
        for s in [
            ProcessSignal::Sighup,
            ProcessSignal::Sigterm,
            ProcessSignal::Sigkill,
            ProcessSignal::Sigusr1,
            ProcessSignal::Sigusr2,
            ProcessSignal::Sigstop,
            ProcessSignal::Sigcont,
        ] {
            assert_eq!(ProcessSignal::from_str(s.as_str()).unwrap(), s);
        }
    }

    #[test]
    fn short_form_accepted() {
        assert_eq!(ProcessSignal::from_str("HUP").unwrap(), ProcessSignal::Sighup);
        assert_eq!(ProcessSignal::from_str("term").unwrap(), ProcessSignal::Sigterm);
    }

    #[test]
    fn unknown_errors() {
        assert!(ProcessSignal::from_str("SIGFOO").is_err());
    }
}
