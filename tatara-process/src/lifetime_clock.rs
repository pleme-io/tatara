//! Ephemeral lifetime clock — TTL expiry + teardown-policy decisions.
//!
//! The reconciler consults this module at each phase tick to decide
//! whether a Process should auto-terminate:
//! - TTL is measured from `metadata.creation_timestamp` (the most
//!   deterministic anchor — phaseSince resets per phase).
//! - Teardown policy applies on `Attested` or `Failed` per
//!   `EphemeralLifetime.teardown_policy`.
//!
//! Returning `AutoTerminate::Now { reason }` tells the caller to transition
//! the Process to `Exiting`. The phase machine handles the SIGTERM path
//! from there (children drained, finalizer guards owned resources).

use chrono::{DateTime, Utc};
use std::time::Duration;

use crate::crd::Process;
use crate::lifetime::LifetimeVariant;
use crate::phase::ProcessPhase;

/// Decision the phase machine acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoTerminate {
    /// No auto-terminate signal — continue with the normal phase handler.
    Skip,
    /// Transition the Process to `Exiting` with the given operator-visible reason.
    Now { reason: String },
}

/// Inspect a Process at the given current phase and return whether the
/// ephemeral lifetime clock fires now.
///
/// `now` is injected so unit tests can drive the clock deterministically.
pub fn evaluate(process: &Process, current_phase: ProcessPhase, now: DateTime<Utc>) -> AutoTerminate {
    let lifetime = match process.spec.lifetime.variant() {
        Ok(v) => v,
        Err(_) => return AutoTerminate::Skip, // ambiguous — treat as no-op
    };
    let ephemeral = match lifetime {
        LifetimeVariant::Permanent(_) => return AutoTerminate::Skip,
        LifetimeVariant::Ephemeral(e) => e,
    };

    // 1. Teardown policy on terminal phases.
    if current_phase == ProcessPhase::Attested
        && ephemeral.teardown_policy.should_teardown_on_attested()
    {
        return AutoTerminate::Now {
            reason: format!(
                "ephemeral lifetime: teardown_policy={:?} fired on Attested",
                ephemeral.teardown_policy
            ),
        };
    }
    if current_phase == ProcessPhase::Failed
        && ephemeral.teardown_policy.should_teardown_on_failed()
    {
        return AutoTerminate::Now {
            reason: format!(
                "ephemeral lifetime: teardown_policy={:?} fired on Failed",
                ephemeral.teardown_policy
            ),
        };
    }

    // 2. TTL expiry — applies in any non-terminal phase.
    if !is_terminal_or_exit(current_phase) {
        if let Some(creation) = process.metadata.creation_timestamp.as_ref() {
            if let Ok(ttl) = humantime::parse_duration(&ephemeral.ttl) {
                let elapsed = now.signed_duration_since(creation.0).to_std().ok();
                if let Some(elapsed) = elapsed {
                    if elapsed >= ttl {
                        return AutoTerminate::Now {
                            reason: format!(
                                "ephemeral lifetime: ttl={} expired (elapsed={}s)",
                                ephemeral.ttl,
                                elapsed.as_secs()
                            ),
                        };
                    }
                }
            }
        }
    }

    AutoTerminate::Skip
}

/// Phases past which TTL cannot meaningfully fire — the SIGTERM path
/// is already in progress.
fn is_terminal_or_exit(p: ProcessPhase) -> bool {
    matches!(
        p,
        ProcessPhase::Exiting | ProcessPhase::Zombie | ProcessPhase::Reaped
    )
}

/// Sleep budget the controller should requeue with for a Process whose
/// `evaluate()` returned `Skip` — picks the smaller of HEARTBEAT and
/// TTL-remaining so we don't oversleep past expiry.
pub fn requeue_with_ttl(process: &Process, now: DateTime<Utc>, default: Duration) -> Duration {
    let Ok(LifetimeVariant::Ephemeral(e)) = process.spec.lifetime.variant() else {
        return default;
    };
    let Some(creation) = process.metadata.creation_timestamp.as_ref() else {
        return default;
    };
    let Ok(ttl) = humantime::parse_duration(&e.ttl) else {
        return default;
    };
    let elapsed = match now.signed_duration_since(creation.0).to_std() {
        Ok(d) => d,
        Err(_) => return default,
    };
    let remaining = ttl.checked_sub(elapsed).unwrap_or(Duration::from_secs(0));
    // Never sleep less than 1s; never longer than the default heartbeat.
    let pick = std::cmp::min(default, remaining);
    std::cmp::max(pick, Duration::from_secs(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classification::{Classification, ConvergencePointType, SubstrateType};
    use crate::crd::ProcessSpec;
    use crate::intent::{AplicacaoIntent, Intent};
    use crate::lifetime::{EphemeralLifetime, Lifetime, TeardownPolicy};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;

    fn ephemeral_process(ttl: &str, teardown: TeardownPolicy, age_secs: i64) -> Process {
        let spec = ProcessSpec {
            identity: Default::default(),
            classification: Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
                horizon: Default::default(),
                calm: Default::default(),
                data_classification: Default::default(),
            },
            intent: Intent {
                aplicacao: Some(AplicacaoIntent {
                    chart_ref: "oci://x".into(),
                    version: "1".into(),
                    profile: String::new(),
                    values_overlay: serde_json::Value::Null,
                    release_name: None,
                    target_namespace: None,
                    install_timeout: None,
                }),
                ..Intent::default()
            },
            boundary: Default::default(),
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Lifetime {
                ephemeral: Some(EphemeralLifetime {
                    ttl: ttl.into(),
                    teardown_policy: teardown,
                    max_concurrent: 1,
                    exports: vec![],
                }),
                ..Lifetime::default()
            },
            suspended: false,
        };
        let mut p = Process::new("e", spec);
        p.metadata.namespace = Some("ns".into());
        let creation = Utc::now() - chrono::Duration::seconds(age_secs);
        p.metadata.creation_timestamp = Some(Time(creation));
        p
    }

    fn permanent_process() -> Process {
        let spec = ProcessSpec {
            identity: Default::default(),
            classification: Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
                horizon: Default::default(),
                calm: Default::default(),
                data_classification: Default::default(),
            },
            intent: Intent {
                aplicacao: Some(AplicacaoIntent {
                    chart_ref: "oci://x".into(),
                    version: "1".into(),
                    profile: String::new(),
                    values_overlay: serde_json::Value::Null,
                    release_name: None,
                    target_namespace: None,
                    install_timeout: None,
                }),
                ..Intent::default()
            },
            boundary: Default::default(),
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Lifetime::default(),
            suspended: false,
        };
        Process::new("e", spec)
    }

    #[test]
    fn permanent_never_auto_terminates() {
        let p = permanent_process();
        for phase in [
            ProcessPhase::Pending,
            ProcessPhase::Execing,
            ProcessPhase::Running,
            ProcessPhase::Attested,
            ProcessPhase::Failed,
        ] {
            assert_eq!(evaluate(&p, phase, Utc::now()), AutoTerminate::Skip);
        }
    }

    #[test]
    fn always_teardown_fires_on_attested_and_failed() {
        let p = ephemeral_process("1h", TeardownPolicy::Always, 60);
        let now = Utc::now();
        assert!(matches!(
            evaluate(&p, ProcessPhase::Attested, now),
            AutoTerminate::Now { .. }
        ));
        assert!(matches!(
            evaluate(&p, ProcessPhase::Failed, now),
            AutoTerminate::Now { .. }
        ));
        assert_eq!(
            evaluate(&p, ProcessPhase::Running, now),
            AutoTerminate::Skip
        );
    }

    #[test]
    fn on_attested_only_fires_on_attested() {
        let p = ephemeral_process("1h", TeardownPolicy::OnAttested, 60);
        let now = Utc::now();
        assert!(matches!(
            evaluate(&p, ProcessPhase::Attested, now),
            AutoTerminate::Now { .. }
        ));
        assert_eq!(
            evaluate(&p, ProcessPhase::Failed, now),
            AutoTerminate::Skip
        );
    }

    #[test]
    fn on_failed_only_fires_on_failed() {
        let p = ephemeral_process("1h", TeardownPolicy::OnFailed, 60);
        let now = Utc::now();
        assert_eq!(
            evaluate(&p, ProcessPhase::Attested, now),
            AutoTerminate::Skip
        );
        assert!(matches!(
            evaluate(&p, ProcessPhase::Failed, now),
            AutoTerminate::Now { .. }
        ));
    }

    #[test]
    fn never_skips_phase_terminations_but_still_honors_ttl() {
        let p = ephemeral_process("30s", TeardownPolicy::Never, 60);
        let now = Utc::now();
        // TTL elapsed → TTL fires regardless of policy.
        assert!(matches!(
            evaluate(&p, ProcessPhase::Running, now),
            AutoTerminate::Now { .. }
        ));
        // But not on a terminal phase (already exiting).
        assert_eq!(
            evaluate(&p, ProcessPhase::Exiting, now),
            AutoTerminate::Skip
        );
    }

    #[test]
    fn ttl_not_yet_elapsed_is_skip() {
        let p = ephemeral_process("1h", TeardownPolicy::Never, 60);
        assert_eq!(
            evaluate(&p, ProcessPhase::Running, Utc::now()),
            AutoTerminate::Skip
        );
    }

    #[test]
    fn requeue_picks_min_of_default_and_remaining() {
        let p = ephemeral_process("5m", TeardownPolicy::Always, 60);
        let now = Utc::now();
        let d = requeue_with_ttl(&p, now, Duration::from_secs(30));
        // 5m total - 60s elapsed = 240s remaining; default 30s wins.
        assert_eq!(d, Duration::from_secs(30));

        let p = ephemeral_process("90s", TeardownPolicy::Always, 80);
        let d = requeue_with_ttl(&p, now, Duration::from_secs(30));
        // 90s - 80s = 10s remaining; remaining wins.
        assert!(d <= Duration::from_secs(11) && d >= Duration::from_secs(9));

        let p = ephemeral_process("90s", TeardownPolicy::Always, 91);
        let d = requeue_with_ttl(&p, now, Duration::from_secs(30));
        // Already past TTL — clamp to 1s, not 0.
        assert_eq!(d, Duration::from_secs(1));
    }
}
