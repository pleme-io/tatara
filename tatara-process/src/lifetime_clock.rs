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
use std::fmt;
use std::time::Duration;

use crate::crd::Process;
use crate::lifetime::TeardownPolicy;
use crate::phase::ProcessPhase;

/// Decision the phase machine acts on.
///
/// Two-variant payload-carrying enum: `Skip` carries no payload (no-op
/// signal to the controller), `Now` carries the typed [`TerminateReason`]
/// that the controller stamps onto `status.message`. The
/// (payload-carrying-enum, payload-stripped-typed-discriminator) split
/// — `Now(reason)` on the wire-shape, [`AutoTerminateKind::Now`] for
/// closed dispatch — is the same shape every sibling closed-set lift
/// in this crate carries (see [`crate::lifetime_clock::TerminateReason`]
/// → [`TerminateReasonKind`], [`crate::matrix::SelectStrategy`] →
/// [`crate::matrix::SelectStrategyKind`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoTerminate {
    /// No auto-terminate signal — continue with the normal phase handler.
    Skip,
    /// Transition the Process to `Exiting` with the given operator-visible reason.
    Now { reason: TerminateReason },
}

impl AutoTerminate {
    /// Discriminator projection — strips the [`Now`]-variant payload and
    /// returns the closed-set kind. Used by the kind-sweep tests and by
    /// any future consumer that groups decisions by category (metrics
    /// labels, dashboard enumeration, `status.conditions[].reason`
    /// reason-keys) without pattern-matching the full payload.
    ///
    /// [`Now`]: AutoTerminate::Now
    pub const fn kind(&self) -> AutoTerminateKind {
        match self {
            Self::Skip => AutoTerminateKind::Skip,
            Self::Now { .. } => AutoTerminateKind::Now,
        }
    }

    /// Reason projection — `Some(&reason)` when the decision is
    /// [`Now`], `None` when [`Skip`]. The closed-set predicate dual:
    /// callers that need only the payload (e.g. to stamp
    /// `status.message`) reach through this projection instead of the
    /// inline `if let AutoTerminate::Now { reason } = …` destructure,
    /// so the variant-name → payload-field binding lives at ONE site.
    /// Adding a third payload-carrying variant in the future updates
    /// every consumer through this method's exhaustiveness check
    /// rather than scattering destructures across the call graph.
    ///
    /// [`Now`]: AutoTerminate::Now
    /// [`Skip`]: AutoTerminate::Skip
    pub const fn reason(&self) -> Option<&TerminateReason> {
        match self {
            Self::Skip => None,
            Self::Now { reason } => Some(reason),
        }
    }

    /// `true` iff the decision is [`Now`]. Symmetric to [`Self::is_skip`].
    ///
    /// [`Now`]: AutoTerminate::Now
    pub const fn is_now(&self) -> bool {
        matches!(self, Self::Now { .. })
    }

    /// `true` iff the decision is [`Skip`]. Symmetric to [`Self::is_now`].
    ///
    /// [`Skip`]: AutoTerminate::Skip
    pub const fn is_skip(&self) -> bool {
        matches!(self, Self::Skip)
    }
}

/// The closed set of [`AutoTerminate`] kinds — the discriminator view,
/// payload-stripped, that sibling closed-set enums in this crate carry
/// (see [`TerminateReasonKind`], [`crate::matrix::SelectStrategyKind`],
/// [`crate::lifetime::LifetimeKind`]).
///
/// Drives the `as_str` / Display / `FromStr` triad over [`Self::ALL`] so
/// a new variant added with an `ALL` entry automatically extends the
/// parser, the canonical wire-format projection, and any future
/// metrics-label / dashboard / `status.conditions[].reason` enumeration
/// that needs to enumerate the decision categories. The `[Self; 2]`
/// array literal forces the arity so a third variant cannot land
/// without bumping the constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_lisp::DeriveClosedSet)]
#[closed_set(via = "as_str", display, generate_unknown = "auto-terminate kind")]
pub enum AutoTerminateKind {
    /// The kind-view of [`AutoTerminate::Skip`].
    Skip,
    /// The kind-view of [`AutoTerminate::Now`] — the payload is
    /// stripped at this projection.
    Now,
}

impl AutoTerminateKind {
    /// The closed set — single source of truth for `as_str` / Display /
    /// `FromStr`.
    pub const ALL: [Self; 2] = [Self::Skip, Self::Now];

    /// Canonical PascalCase wire-format projection. Mirrors the
    /// `tatara-process` PascalCase idiom used by every other closed-set
    /// enum's `as_str` projection (e.g. [`ProcessPhase::as_str`],
    /// [`TerminateReasonKind::as_str`]). A future metrics-label /
    /// `status.conditions[].reason` field reads this projection
    /// directly.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skip => "Skip",
            Self::Now => "Now",
        }
    }
}

// `impl fmt::Display for AutoTerminateKind` + `impl FromStr for
// AutoTerminateKind` + `impl tatara_lisp::ClosedSet for
// AutoTerminateKind` + `pub struct UnknownAutoTerminateKind(pub
// String)` are generated by `#[derive(tatara_lisp::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", display, generate_unknown =
// "auto-terminate kind")]` on the enum declaration above. The explicit
// label pins the pre-lift wording (with hyphen) against the auto-
// projection `pascal_to_spaced_lowercase("AutoTerminateKind")` →
// "auto terminate kind" (no hyphen) — the operator-facing
// `#[error("unknown auto-terminate kind: {0}")]` annotation stays byte-
// for-byte identical to the pre-lift hand-roll. The inherent `as_str`
// projection stays load-bearing — the PascalCase wire-format the
// `evaluate` decision-projection's emitted reason reads — while the
// trait method `label` gives generic consumers a STABLE name across
// the workspace-wide closed-set implementors.

/// Why the ephemeral lifetime clock fired.
///
/// Typed image of the two reason strings the pre-lift evaluator composed
/// inline with `format!(…)`. Each variant carries the typed payload its
/// `Display` formats against the canonical PascalCase projection of
/// [`TeardownPolicy`] / [`ProcessPhase`], so the operator-visible reason
/// is read off the typed surface rather than a free-form template that
/// could drift on a variant rename. The reason string is the deliverable
/// the reconciler stamps onto `status.message`; this enum is the source
/// of truth.
///
/// Adding a third cause (e.g. parent-cascade from a SIGKILL'd parent in
/// the hierarchical PID model, OOM-style memory-pressure pre-emption, or
/// a future ResourceQuota gate) lands at one variant + one [`Display`]
/// arm + one [`TerminateReasonKind`] entry — exhaustively checked by the
/// compiler AND by the per-variant truth-table tests.
///
/// Sibling closed-set lifts on the same `tatara-process` axis:
/// [`crate::intent::IntentKind::ALL`], [`crate::LifetimeKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminateReason {
    /// The Process reached a terminal-gate phase ([`ProcessPhase::Attested`]
    /// or [`ProcessPhase::Failed`]) and the ephemeral lifetime's
    /// [`TeardownPolicy`] elected to fire on that phase.
    TeardownPolicy {
        policy: TeardownPolicy,
        phase: ProcessPhase,
    },
    /// The ephemeral lifetime's TTL elapsed in a non-terminal phase.
    /// `ttl` carries the operator-authored `humantime` string verbatim
    /// (e.g. `"1h"`, `"30m"`) so the reason surfaces the spec field as
    /// it was written, not as it parsed. `elapsed` is the wall-clock
    /// distance from `metadata.creation_timestamp` at evaluation time.
    TtlExpired { ttl: String, elapsed: Duration },
}

impl TerminateReason {
    /// Discriminator projection — strips the payload, yielding the
    /// closed-set kind. Used by the reason-kind sweep tests and by any
    /// future consumer that wants to group reasons by cause without
    /// pattern-matching the full payload (e.g. metrics labels, future
    /// `status.conditions` reason-keys).
    pub const fn kind(&self) -> TerminateReasonKind {
        match self {
            Self::TeardownPolicy { .. } => TerminateReasonKind::TeardownPolicy,
            Self::TtlExpired { .. } => TerminateReasonKind::TtlExpired,
        }
    }
}

impl fmt::Display for TerminateReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // LOAD-BEARING CONTRACT: the strings produced here are the
        // operator-visible reasons the reconciler stamps onto
        // `status.message` and `status.conditions[…].message`. They
        // must match the pre-lift `format!(…)` output byte-for-byte
        // so existing alerts, dashboards, and operator runbooks keep
        // matching. Pinned by `terminate_reason_display_matches_pre_lift`.
        match self {
            Self::TeardownPolicy { policy, phase } => {
                write!(
                    f,
                    "ephemeral lifetime: teardown_policy={} fired on {}",
                    policy.as_str(),
                    phase.as_str(),
                )
            }
            Self::TtlExpired { ttl, elapsed } => {
                write!(
                    f,
                    "ephemeral lifetime: ttl={} expired (elapsed={}s)",
                    ttl,
                    elapsed.as_secs(),
                )
            }
        }
    }
}

/// The closed set of [`TerminateReason`] kinds — the discriminator
/// view, payload-stripped, that sibling closed-set enums in this
/// crate carry (see [`ProcessPhase`], [`TeardownPolicy`]).
///
/// Drives the `as_str` / Display / `FromStr` triad over [`Self::ALL`] so
/// a new variant added with an `ALL` entry automatically extends the
/// parser, the canonical wire-format projection, and any future
/// metrics-label / `status.conditions[].reason` enumeration that needs
/// to enumerate the reason categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_lisp::DeriveClosedSet)]
#[closed_set(via = "as_str", display, generate_unknown)]
pub enum TerminateReasonKind {
    TeardownPolicy,
    TtlExpired,
}

impl TerminateReasonKind {
    /// The closed set — single source of truth for `as_str` / Display /
    /// `FromStr`. The `[Self; 2]` array literal forces the arity so a
    /// third variant added without an `ALL` entry fails at the type
    /// level before the test sweep below runs.
    pub const ALL: [Self; 2] = [Self::TeardownPolicy, Self::TtlExpired];

    /// Canonical PascalCase wire-format projection. Mirrors the
    /// `tatara-process` PascalCase idiom used by every other closed-set
    /// enum's `as_str` projection (e.g. [`ProcessPhase::as_str`],
    /// [`TeardownPolicy::as_str`]). A future `status.conditions[].reason`
    /// field reads this projection directly.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TeardownPolicy => "TeardownPolicy",
            Self::TtlExpired => "TtlExpired",
        }
    }
}

// `impl fmt::Display for TerminateReasonKind` + `impl FromStr for
// TerminateReasonKind` + `impl tatara_lisp::ClosedSet for
// TerminateReasonKind` + `pub struct UnknownTerminateReasonKind(pub
// String)` are generated by `#[derive(tatara_lisp::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", display, generate_unknown)]` on the
// enum declaration above. The auto-derived label `"terminate reason
// kind"` matches the prior hand-rolled `#[error("unknown terminate
// reason kind: {0}")]` verbatim. The inherent `as_str` projection
// stays load-bearing — the PascalCase wire-format the
// `crate::lifetime_clock::evaluate` decision-projection's emitted
// reason reads — while the trait method `label` gives generic
// consumers a STABLE name across the workspace-wide closed-set
// implementors.

/// Inspect a Process at the given current phase and return whether the
/// ephemeral lifetime clock fires now.
///
/// `now` is injected so unit tests can drive the clock deterministically.
pub fn evaluate(
    process: &Process,
    current_phase: ProcessPhase,
    now: DateTime<Utc>,
) -> AutoTerminate {
    // Closed-set projection: ambiguous → no-op; permanent → no-op;
    // ephemeral → fall through to teardown / TTL checks. ONE
    // `as_ephemeral` gate replaces the previous two-step
    // match-then-match dance and shares the projection with
    // `requeue_with_ttl` below.
    let Ok(variant) = process.spec.lifetime.variant() else {
        return AutoTerminate::Skip;
    };
    let Some(ephemeral) = variant.as_ephemeral() else {
        return AutoTerminate::Skip;
    };

    // 1. Teardown policy on terminal phases — ONE typed dispatch over
    //    `(TeardownPolicy, ProcessPhase)` replaces the previous pair of
    //    near-identical Attested/Failed branches. Non-terminal phases
    //    short-circuit inside `should_teardown_on`. The reason is the
    //    typed `TerminateReason::TeardownPolicy` variant whose `Display`
    //    composes the operator-visible string against the canonical
    //    PascalCase projection (`TeardownPolicy::as_str` +
    //    `ProcessPhase::as_str`), not a free-form template.
    if ephemeral.teardown_policy.should_teardown_on(current_phase) {
        return AutoTerminate::Now {
            reason: TerminateReason::TeardownPolicy {
                policy: ephemeral.teardown_policy,
                phase: current_phase,
            },
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
                            reason: TerminateReason::TtlExpired {
                                ttl: ephemeral.ttl.clone(),
                                elapsed,
                            },
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
    // Shared `as_ephemeral` projection with [`evaluate`] — the
    // "give me only the ephemeral case" shape lives at one site.
    let Ok(variant) = process.spec.lifetime.variant() else {
        return default;
    };
    let Some(e) = variant.as_ephemeral() else {
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
            routing: None,
            encapsulates: None,
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
            routing: None,
            encapsulates: None,
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
        assert_eq!(evaluate(&p, ProcessPhase::Failed, now), AutoTerminate::Skip);
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

    /// REASON-STRING CONTRACT: the operator-visible reason composes
    /// the canonical PascalCase projection of `TeardownPolicy` and
    /// `ProcessPhase` (via Display) rather than the Debug formatting
    /// used pre-lift. A future variant rename of either enum updates
    /// the reason string at ONE site (the `as_str` arm) instead of
    /// drifting between the typed surface and the operator log.
    #[test]
    fn teardown_reason_string_uses_canonical_projection() {
        let p = ephemeral_process("1h", TeardownPolicy::OnAttested, 60);
        match evaluate(&p, ProcessPhase::Attested, Utc::now()) {
            AutoTerminate::Now { reason } => {
                let rendered = reason.to_string();
                assert!(
                    rendered.contains("teardown_policy=OnAttested"),
                    "expected canonical PascalCase policy, got: {rendered}",
                );
                assert!(
                    rendered.contains("fired on Attested"),
                    "expected canonical PascalCase phase, got: {rendered}",
                );
            }
            other => panic!("expected AutoTerminate::Now, got {other:?}"),
        }

        let p = ephemeral_process("1h", TeardownPolicy::Always, 60);
        match evaluate(&p, ProcessPhase::Failed, Utc::now()) {
            AutoTerminate::Now { reason } => {
                let rendered = reason.to_string();
                assert!(rendered.contains("teardown_policy=Always"));
                assert!(rendered.contains("fired on Failed"));
            }
            other => panic!("expected AutoTerminate::Now, got {other:?}"),
        }
    }

    // ── TerminateReason / TerminateReasonKind closed-set contracts ────

    /// BYTE-FOR-BYTE PRE-LIFT CONTRACT: the Display impl on
    /// `TerminateReason` must produce the exact string the pre-lift
    /// inline `format!(…)` calls produced. Existing alerts, dashboards,
    /// and operator runbooks that grep `status.message` for these
    /// substrings keep matching. A future variant rename of
    /// `TeardownPolicy` / `ProcessPhase` updates the rendered string
    /// here automatically (Display reads `as_str` projection), but the
    /// template — `"ephemeral lifetime: teardown_policy={} fired on {}"`
    /// vs `"ephemeral lifetime: ttl={} expired (elapsed={}s)"` — is
    /// pinned at the Display site.
    #[test]
    fn terminate_reason_display_matches_pre_lift() {
        // TeardownPolicy variant — every combination of policy × phase
        // sweeps both PascalCase projections.
        for policy in TeardownPolicy::ALL {
            for phase in ProcessPhase::ALL {
                let reason = TerminateReason::TeardownPolicy { policy, phase };
                let expected = format!(
                    "ephemeral lifetime: teardown_policy={} fired on {}",
                    policy.as_str(),
                    phase.as_str(),
                );
                assert_eq!(
                    reason.to_string(),
                    expected,
                    "Display drifted for ({policy:?}, {phase:?})",
                );
            }
        }
        // TtlExpired variant — pins the ttl-verbatim + elapsed-secs
        // template against representative humantime strings the
        // EphemeralLifetime.ttl field accepts.
        for (ttl, elapsed_secs) in [("1h", 0u64), ("30m", 60), ("90s", 100), ("5m30s", 3600)] {
            let reason = TerminateReason::TtlExpired {
                ttl: ttl.to_string(),
                elapsed: Duration::from_secs(elapsed_secs),
            };
            assert_eq!(
                reason.to_string(),
                format!("ephemeral lifetime: ttl={ttl} expired (elapsed={elapsed_secs}s)"),
            );
        }
    }

    /// Reason `kind()` projection — closed-set match so a future
    /// variant triggers exhaustiveness checking at the projection
    /// site rather than silently bucketing through a wildcard. Every
    /// variant's `kind()` matches its `TerminateReasonKind` peer.
    #[test]
    fn terminate_reason_kind_truth_table() {
        assert_eq!(
            TerminateReason::TeardownPolicy {
                policy: TeardownPolicy::Always,
                phase: ProcessPhase::Attested,
            }
            .kind(),
            TerminateReasonKind::TeardownPolicy,
        );
        assert_eq!(
            TerminateReason::TtlExpired {
                ttl: "1h".to_string(),
                elapsed: Duration::from_secs(0),
            }
            .kind(),
            TerminateReasonKind::TtlExpired,
        );
    }

    /// `ALL` is the source of truth; a variant added without an `ALL`
    /// entry fails here (uniqueness check) before any sweep test below
    /// runs. Arity is asserted by the array type itself (`[Self; 2]`).
    /// Exercise the substrate-wide [`tatara_lisp::ClosedSet`] contract on
    /// [`TerminateReasonKind`] — pins the structural three-plus-one
    /// (`ALL` is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the
    /// hand-derived `terminate_reason_kind_all_is_unique_and_complete`
    /// + `terminate_reason_kind_roundtrip_via_as_str` + the empty-input
    /// arm of `unknown_terminate_reason_kind_errors`. `FromStr`
    /// delegates to `<Self as tatara_lisp::ClosedSet>::parse_label`,
    /// so this helper exercises the same code path the lifetime-clock
    /// evaluator hits when parsing a typed reason back out of a
    /// `status.conditions[].reason` slot.
    #[test]
    fn terminate_reason_kind_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<TerminateReasonKind>();
    }

    /// `Display` IS `as_str` — pinning this lets future callers reach
    /// for either projection without drift.
    #[test]
    fn terminate_reason_kind_display_matches_as_str() {
        for kind in TerminateReasonKind::ALL {
            assert_eq!(kind.to_string(), kind.as_str());
        }
    }

    /// Every kind's `as_str` is in canonical PascalCase. The first
    /// character is uppercase; no whitespace; no separators. The
    /// `tatara-process` PascalCase idiom holds at one test site.
    #[test]
    fn terminate_reason_kind_as_str_is_pascal_case() {
        for kind in TerminateReasonKind::ALL {
            let s = kind.as_str();
            assert!(!s.is_empty(), "as_str empty for {kind:?}");
            assert!(
                s.chars().next().unwrap().is_ascii_uppercase(),
                "as_str not PascalCase for {kind:?}: {s}",
            );
            assert!(
                !s.contains(|c: char| c.is_whitespace() || c == '_' || c == '-'),
                "as_str carries separator for {kind:?}: {s}",
            );
        }
    }

    /// `FromStr` rejects strings outside the canonical projection
    /// (lowercased / typo / cross-axis-leaked) and echoes the input
    /// verbatim. The empty-string arm is covered by
    /// `terminate_reason_kind_is_well_formed_closed_set` via the
    /// [`tatara_lisp::ClosedSet`] contract; the verbatim-echo arms
    /// stay here because they pin the `UnknownTerminateReasonKind`
    /// newtype payload contract the trait's `make_unknown` cannot
    /// see. Cross-axis inputs (ProcessPhase / TeardownPolicy variant
    /// names) MUST fail — `TerminateReasonKind` is its own axis, not
    /// a transparent reflection of either.
    #[test]
    fn unknown_terminate_reason_kind_errors() {
        use std::str::FromStr;
        for bad in [
            "teardownPolicy",
            "TEARDOWN_POLICY",
            "Teardown",
            "TtlExpire",
            "ttl_expired",
            "ttlExpired",
            // Cross-axis-leaked — must NOT cross axes.
            "Attested",
            "Failed",
            "Always",
            "OnAttested",
            "OnFailed",
            "Never",
            "Permanent",
            "Ephemeral",
        ] {
            let err = TerminateReasonKind::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// The reason `evaluate` returns under teardown maps to
    /// `TerminateReasonKind::TeardownPolicy` AND its payload reflects
    /// the spec's `(teardown_policy, current_phase)` verbatim — the
    /// typed surface IS the source of truth, not an inline format
    /// template. A future consumer that wants to group reasons by
    /// kind in metrics labels reads `reason.kind()`, not a substring
    /// match.
    #[test]
    fn evaluate_typed_reason_carries_teardown_payload() {
        for (policy, phase) in [
            (TeardownPolicy::Always, ProcessPhase::Attested),
            (TeardownPolicy::Always, ProcessPhase::Failed),
            (TeardownPolicy::OnAttested, ProcessPhase::Attested),
            (TeardownPolicy::OnFailed, ProcessPhase::Failed),
        ] {
            let p = ephemeral_process("1h", policy, 60);
            match evaluate(&p, phase, Utc::now()) {
                AutoTerminate::Now { reason } => {
                    assert_eq!(reason.kind(), TerminateReasonKind::TeardownPolicy);
                    assert_eq!(
                        reason,
                        TerminateReason::TeardownPolicy { policy, phase },
                        "typed payload drift for ({policy:?}, {phase:?})",
                    );
                }
                other => {
                    panic!("expected AutoTerminate::Now for ({policy:?}, {phase:?}), got {other:?}",)
                }
            }
        }
    }

    /// TTL expiry returns a `TtlExpired` reason whose `ttl` field is
    /// the operator-authored humantime string verbatim (NOT the
    /// parsed `Duration`'s pretty-print) and whose `elapsed` is the
    /// wall-clock distance. Pinned here so a future evaluator change
    /// that re-formats the ttl through `humantime::format_duration`
    /// would fail.
    #[test]
    fn evaluate_typed_reason_carries_ttl_payload() {
        let p = ephemeral_process("30s", TeardownPolicy::Never, 60);
        let now = Utc::now();
        match evaluate(&p, ProcessPhase::Running, now) {
            AutoTerminate::Now { reason } => {
                assert_eq!(reason.kind(), TerminateReasonKind::TtlExpired);
                match reason {
                    TerminateReason::TtlExpired { ttl, elapsed } => {
                        assert_eq!(ttl, "30s", "ttl should be verbatim spec string");
                        assert!(
                            elapsed >= Duration::from_secs(30),
                            "elapsed should be at least the ttl",
                        );
                    }
                    other => panic!("expected TtlExpired, got {other:?}"),
                }
            }
            other => panic!("expected AutoTerminate::Now, got {other:?}"),
        }
    }

    // ── AutoTerminate / AutoTerminateKind closed-set contracts ────────

    /// Exercise the substrate-wide [`tatara_lisp::ClosedSet`] contract on
    /// [`AutoTerminateKind`] — pins the structural three-plus-one
    /// (`ALL` is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the
    /// hand-derived uniqueness sweep in
    /// `auto_terminate_kind_kind_projection_is_exhaustive_over_all`'s
    /// pre-lift form + the `auto_terminate_kind_roundtrip_via_as_str`
    /// hand-rolled sweep + the empty-input arm of
    /// `unknown_auto_terminate_kind_errors`. `FromStr` delegates to
    /// `<Self as tatara_lisp::ClosedSet>::parse_label`, so this
    /// helper exercises the same code path the lifetime-clock
    /// evaluator hits when parsing a typed kind back out of a
    /// `status.conditions[].reason` slot.
    #[test]
    fn auto_terminate_kind_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<AutoTerminateKind>();
    }

    /// Every entry in `ALL` is reachable through a concrete
    /// [`AutoTerminate`] value via [`AutoTerminate::kind`] — the
    /// projection is exhaustive across the variant set. Pre-lift this
    /// pin was bundled with a uniqueness HashSet sweep that
    /// [`auto_terminate_kind_is_well_formed_closed_set`] now covers
    /// generically through the [`tatara_lisp::ClosedSet`] contract;
    /// post-lift this test keeps only the domain-specific
    /// `kind()`-exhaustiveness contract (the (variant-name →
    /// payload-stripped kind) binding the [`AutoTerminate`] surface
    /// projects through). A future third payload-carrying
    /// `AutoTerminate` variant updates this pin AND
    /// [`AutoTerminate::kind`]'s exhaustiveness match together,
    /// exhaustively checked by the compiler.
    #[test]
    fn auto_terminate_kind_kind_projection_is_exhaustive_over_all() {
        let by_all: std::collections::HashSet<_> = AutoTerminateKind::ALL.iter().copied().collect();
        let sample_reason = TerminateReason::TtlExpired {
            ttl: "1h".into(),
            elapsed: Duration::from_secs(0),
        };
        let by_concrete: std::collections::HashSet<_> = [
            AutoTerminate::Skip.kind(),
            AutoTerminate::Now {
                reason: sample_reason,
            }
            .kind(),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            by_concrete, by_all,
            "kind() projection not exhaustive over ALL"
        );
    }

    /// BYTE-EXACT canonical wire-format pin — renaming either of the two
    /// canonical strings is a wire-format change that fails this test
    /// FIRST so it stays a deliberate change, not a silent rename that
    /// drifts existing alerts / dashboards / operator runbooks.
    #[test]
    fn auto_terminate_kind_canonical_names_pinned() {
        assert_eq!(AutoTerminateKind::Skip.as_str(), "Skip");
        assert_eq!(AutoTerminateKind::Now.as_str(), "Now");
    }

    /// Every kind's `as_str` is in canonical PascalCase. The first
    /// character is uppercase; no whitespace; no separators. The
    /// `tatara-process` PascalCase idiom holds at one test site.
    #[test]
    fn auto_terminate_kind_as_str_is_pascal_case() {
        for kind in AutoTerminateKind::ALL {
            let s = kind.as_str();
            assert!(!s.is_empty(), "as_str empty for {kind:?}");
            assert!(
                s.chars().next().unwrap().is_ascii_uppercase(),
                "as_str not PascalCase for {kind:?}: {s}",
            );
            assert!(
                !s.contains(|c: char| c.is_whitespace() || c == '_' || c == '-'),
                "as_str carries separator for {kind:?}: {s}",
            );
        }
    }

    /// `Display` IS `as_str` — pinning this lets future callers reach
    /// for either projection without drift.
    #[test]
    fn auto_terminate_kind_display_matches_as_str() {
        for kind in AutoTerminateKind::ALL {
            assert_eq!(kind.to_string(), kind.as_str());
        }
    }

    /// `FromStr` rejects strings outside the canonical projection
    /// (lowercased / typo / cross-axis-leaked) and echoes the input
    /// verbatim. The empty-string arm AND the round-trip sweep are
    /// covered by `auto_terminate_kind_is_well_formed_closed_set` via
    /// the [`tatara_lisp::ClosedSet`] contract; the cases here pin the
    /// `UnknownAutoTerminateKind` newtype payload contract the
    /// trait's `make_unknown` cannot see. Cross-axis inputs
    /// (ProcessPhase / TeardownPolicy / TerminateReasonKind variant
    /// names) MUST fail — `AutoTerminateKind` is its own axis, not
    /// a transparent reflection of any sibling enum.
    #[test]
    fn unknown_auto_terminate_kind_errors() {
        use std::str::FromStr;
        for bad in [
            "skip",
            "now",
            "SKIP",
            "NOW",
            "S",
            "N",
            "no-op",
            "terminate",
            // Cross-axis-leaked — must NOT cross axes.
            "Attested",
            "Failed",
            "TeardownPolicy",
            "TtlExpired",
            "Always",
            "Permanent",
            "Ephemeral",
        ] {
            let err = AutoTerminateKind::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// `reason()` projection: `Now { reason }` returns `Some(&reason)`,
    /// `Skip` returns `None`. The (variant-name → payload-field)
    /// binding lives at ONE site so a future third payload-carrying
    /// variant updates every consumer through this method's
    /// exhaustiveness check rather than scattering destructures across
    /// the call graph.
    #[test]
    fn auto_terminate_reason_projection() {
        assert!(AutoTerminate::Skip.reason().is_none());

        let reason = TerminateReason::TtlExpired {
            ttl: "1h".into(),
            elapsed: Duration::from_secs(0),
        };
        let now = AutoTerminate::Now {
            reason: reason.clone(),
        };
        assert_eq!(now.reason(), Some(&reason));

        let teardown = TerminateReason::TeardownPolicy {
            policy: TeardownPolicy::OnAttested,
            phase: ProcessPhase::Attested,
        };
        let now = AutoTerminate::Now {
            reason: teardown.clone(),
        };
        assert_eq!(now.reason(), Some(&teardown));
    }

    /// `is_now` / `is_skip` are exact complements over the closed set —
    /// `is_now ⊕ is_skip = true` for every variant. Locks the predicate
    /// pair so a future third variant that's neither Skip nor Now must
    /// extend BOTH predicates in lockstep (or this contract fails).
    #[test]
    fn auto_terminate_predicate_pair_is_exhaustive_complement() {
        let reason = TerminateReason::TtlExpired {
            ttl: "1h".into(),
            elapsed: Duration::from_secs(0),
        };
        for decision in [
            AutoTerminate::Skip,
            AutoTerminate::Now {
                reason: reason.clone(),
            },
        ] {
            assert_ne!(
                decision.is_now(),
                decision.is_skip(),
                "predicate pair drift for {decision:?}",
            );
            // The kind projection agrees with each predicate.
            assert_eq!(decision.is_now(), decision.kind() == AutoTerminateKind::Now);
            assert_eq!(
                decision.is_skip(),
                decision.kind() == AutoTerminateKind::Skip
            );
            // `reason()` agrees with `is_now`.
            assert_eq!(decision.reason().is_some(), decision.is_now());
        }
    }

    /// The `kind()` projection on the typed result of `evaluate` agrees
    /// with the behavioural expectation: ephemeral-on-Attested with an
    /// OnAttested policy returns `Now`, permanent never does. Closes
    /// the loop between the closed-set view and the live decision so
    /// any future kind-keyed metrics label (e.g.
    /// `tatara_lifetime_clock_decisions_total{kind="Now"}`) reads the
    /// typed projection rather than the inline destructure.
    #[test]
    fn evaluate_decision_kind_agrees_with_runtime_behaviour() {
        let p = permanent_process();
        for phase in [
            ProcessPhase::Pending,
            ProcessPhase::Running,
            ProcessPhase::Attested,
            ProcessPhase::Failed,
        ] {
            let decision = evaluate(&p, phase, Utc::now());
            assert_eq!(
                decision.kind(),
                AutoTerminateKind::Skip,
                "permanent Process must always Skip; got Now for phase={phase:?}",
            );
            assert!(decision.reason().is_none());
        }

        let p = ephemeral_process("1h", TeardownPolicy::OnAttested, 60);
        let now = Utc::now();
        assert_eq!(
            evaluate(&p, ProcessPhase::Attested, now).kind(),
            AutoTerminateKind::Now,
        );
        assert_eq!(
            evaluate(&p, ProcessPhase::Running, now).kind(),
            AutoTerminateKind::Skip,
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
