//! `ProcessSpec` sub-structures — IdentitySpec, DependsOn, SignalPolicy.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::phase::ProcessPhase;
use crate::signal::SighupStrategy;

/// Identity configuration for a Process.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdentitySpec {
    /// Parent PID path (None for init/PID 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Human name override — if set, used verbatim instead of the content hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_override: Option<String>,
}

/// Dependency edge — constrains this Process to wait for another to reach a phase.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DependsOn {
    /// Target Process `metadata.name`.
    pub name: String,
    /// Target Process namespace. Defaults to this Process's namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Minimum phase the target must reach before we proceed past Forking.
    #[serde(default)]
    pub must_reach: MustReachPhase,
}

/// Allowed "must reach" phases for a dependency — restricted to useful checkpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
pub enum MustReachPhase {
    Running,
    #[default]
    Attested,
}

impl From<MustReachPhase> for ProcessPhase {
    fn from(v: MustReachPhase) -> Self {
        match v {
            MustReachPhase::Running => ProcessPhase::Running,
            MustReachPhase::Attested => ProcessPhase::Attested,
        }
    }
}

/// Signal policy — how the Process responds to signals.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignalPolicy {
    /// Grace before escalating SIGTERM → SIGKILL.
    #[serde(default = "default_sigterm_grace")]
    pub sigterm_grace_seconds: u32,
    /// Permit force-reap via SIGKILL (default: allow).
    #[serde(default = "default_true")]
    pub sigkill_force: bool,
    /// How SIGHUP is handled.
    #[serde(default)]
    pub sighup_strategy: SighupStrategy,
    /// Start suspended — requires SIGCONT to transition past Forking.
    #[serde(default)]
    pub start_suspended: bool,
}

impl Default for SignalPolicy {
    fn default() -> Self {
        Self {
            sigterm_grace_seconds: default_sigterm_grace(),
            sigkill_force: true,
            sighup_strategy: SighupStrategy::default(),
            start_suspended: false,
        }
    }
}

fn default_sigterm_grace() -> u32 {
    480
}
fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn must_reach_default_is_attested() {
        assert_eq!(MustReachPhase::default(), MustReachPhase::Attested);
    }

    #[test]
    fn signal_policy_defaults() {
        let p = SignalPolicy::default();
        assert_eq!(p.sigterm_grace_seconds, 480);
        assert!(p.sigkill_force);
        assert!(!p.start_suspended);
    }
}
