//! `ProcessSpec` sub-structures — IdentitySpec, DependsOn, SignalPolicy.

use std::fmt;
use std::str::FromStr;

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

/// Allowed "must reach" phases for a dependency — restricted to the
/// useful gating checkpoints `Running` (alive + boundary preconditions
/// held) and `Attested` (alive + boundary postconditions held + three-
/// pillar attestation written). Authoring a `DependsOn { must_reach:
/// Forking }` is meaningless; the closed set rules it out at the type
/// level.
///
/// Sibling closed-set lifts on the same `ProcessSpec` axis:
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
pub enum MustReachPhase {
    Running,
    #[default]
    Attested,
}

impl MustReachPhase {
    /// The closed set of must-reach phases — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad and the typed
    /// `as_process_phase` projection. Adding a third variant (e.g. a
    /// future `Released` checkpoint that waits for the target Process to
    /// have exited cleanly) lands at one `ALL` entry, one `as_str` arm,
    /// and one `as_process_phase` arm — exhaustively checked by the
    /// compiler (the `[Self; 2]` array literal forces the arity).
    pub const ALL: [Self; 2] = [Self::Running, Self::Attested];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim AND the canonical
    /// `ProcessPhase::as_str()` projection on the phase this variant
    /// gates against. Used by Display (single source of truth), by
    /// `FromStr` to identify the variant from its annotation / status-
    /// field representation, and by operator-facing diagnostic strings
    /// (`tatara-reconciler::boundary::check_depends_on` stamps the
    /// required phase via `Display` rather than reaching for `{:?}`
    /// Debug formatting). Pinned by `must_reach_phase_as_str_matches_serde`
    /// AND by `must_reach_phase_as_str_matches_process_phase_as_str` so
    /// a rename on either side surfaces at one site.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Attested => "Attested",
        }
    }

    /// Typed projection into the canonical `ProcessPhase` this variant
    /// gates against. The `From<MustReachPhase> for ProcessPhase` impl
    /// delegates here so callers reach for whichever surface fits (the
    /// `From` for `into()` flows, this `const fn` for const contexts).
    /// Pinned by `must_reach_phase_from_delegates_to_as_process_phase`.
    pub const fn as_process_phase(self) -> ProcessPhase {
        match self {
            Self::Running => ProcessPhase::Running,
            Self::Attested => ProcessPhase::Attested,
        }
    }
}

impl fmt::Display for MustReachPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MustReachPhase {
    type Err = UnknownMustReachPhase;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for kind in Self::ALL {
            if s == kind.as_str() {
                return Ok(kind);
            }
        }
        Err(UnknownMustReachPhase(s.to_string()))
    }
}

impl From<MustReachPhase> for ProcessPhase {
    fn from(v: MustReachPhase) -> Self {
        v.as_process_phase()
    }
}

/// Typed parse failure carrying the offending input verbatim so the
/// operator-facing diagnostic surfaces the bad value, not a normalized
/// form. Symmetric to [`crate::phase::UnknownPhase`],
/// [`crate::lifetime::UnknownTeardownPolicy`], and
/// [`crate::boundary::UnknownConditionKind`].
#[derive(Debug, thiserror::Error)]
#[error("unknown must-reach phase: {0}")]
pub struct UnknownMustReachPhase(pub String);

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

    // ── closed-set algebra for MustReachPhase (ALL × as_str × FromStr ×
    //    as_process_phase) ──────────────────────────────────────────────

    /// `ALL` is the source of truth for the `FromStr` sweep and the
    /// projection-truth-table test — pin its closure so a variant added
    /// without an `ALL` entry fails here (via the uniqueness check)
    /// before drifting `as_str` / `as_process_phase`. The arity is
    /// asserted by the `[Self; 2]` array type itself.
    #[test]
    fn must_reach_phase_all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for kind in MustReachPhase::ALL {
            assert!(seen.insert(kind), "duplicate variant in ALL: {kind:?}");
        }
        assert_eq!(seen.len(), MustReachPhase::ALL.len());
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site.
    #[test]
    fn must_reach_phase_as_str_matches_serde() {
        for kind in MustReachPhase::ALL {
            let serialized = serde_json::to_string(&kind)
                .expect("MustReachPhase serializes")
                .trim_matches('"')
                .to_string();
            assert_eq!(
                kind.as_str(),
                serialized,
                "as_str() must match serde output for {kind:?}",
            );
        }
    }

    /// CROSS-CRATE CANONICAL-KEY CONTRACT: `MustReachPhase::as_str()`
    /// matches the canonical `ProcessPhase::as_str()` of the phase it
    /// projects to. The two enums share the PascalCase wire format
    /// because `MustReachPhase` is a typed subset of `ProcessPhase`'s
    /// safe gating checkpoints; a rename on either side (a phase
    /// rename in `ProcessPhase::as_str` OR an `as_str` arm typo here)
    /// surfaces here at one site, not buried in a reconciler diagnostic
    /// that quietly drifted away from the typed-phase surface.
    #[test]
    fn must_reach_phase_as_str_matches_process_phase_as_str() {
        for kind in MustReachPhase::ALL {
            assert_eq!(
                kind.as_str(),
                kind.as_process_phase().as_str(),
                "MustReachPhase::as_str() and ProcessPhase::as_str() drift for {kind:?}",
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. If a reviewer
    /// accidentally re-introduces an inline match in Display, this test
    /// would fail the moment a variant rename touches one site but not
    /// the other.
    #[test]
    fn must_reach_phase_display_matches_as_str() {
        for kind in MustReachPhase::ALL {
            assert_eq!(kind.to_string(), kind.as_str());
        }
    }

    /// ROUND-TRIP CONTRACT: every variant survives `as_str` ↔ `FromStr`.
    /// Adding a variant without extending `as_str` (or vice versa)
    /// fails here.
    #[test]
    fn must_reach_phase_roundtrip_via_as_str() {
        for kind in MustReachPhase::ALL {
            assert_eq!(
                MustReachPhase::from_str(kind.as_str()).expect("known variant round-trips"),
                kind,
                "round-trip failed for {kind:?}",
            );
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — empty / lowercased / typo / non-checkpoint phase
    /// names — and the error echoes the input verbatim so the
    /// operator-facing diagnostic carries the offending value, not a
    /// normalized form. Non-checkpoint phases like `Pending` /
    /// `Failed` / `Reaped` (which are legal `ProcessPhase`s but NOT
    /// valid `MustReachPhase` checkpoints) MUST fail to parse — that's
    /// the whole point of the closed subset.
    #[test]
    fn unknown_must_reach_phase_errors() {
        for bad in [
            "", "running", "ATTESTED", "Atested", "Pending", "Failed", "Reaped",
        ] {
            let err = MustReachPhase::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// DELEGATION CONTRACT: the `From<MustReachPhase> for ProcessPhase`
    /// impl agrees with the typed `as_process_phase()` projection it
    /// delegates to, for every variant. A regression that re-introduces
    /// an inline match in the `From` impl fails here the moment
    /// `as_process_phase` is the source of truth. Pairs with the
    /// `as_str` cross-crate test above — together they pin that the
    /// projection's value AND wire-format are coherent.
    #[test]
    fn must_reach_phase_from_delegates_to_as_process_phase() {
        for kind in MustReachPhase::ALL {
            let via_from: ProcessPhase = kind.into();
            assert_eq!(
                via_from,
                kind.as_process_phase(),
                "From<MustReachPhase> drift for {kind:?}",
            );
        }
    }

    /// SUBSET CONTRACT: every `MustReachPhase` variant projects to a
    /// `ProcessPhase` that is `is_running()` — i.e. one of the live
    /// gating checkpoints (`Running` or `Attested`). This pins the
    /// closed subset's invariant at the type level: a future
    /// `MustReachPhase::Released` (e.g. wait for the target to reach
    /// `Reaped`) would FAIL this test, forcing the author to either
    /// rename the predicate (`is_running` is wrong for that case) or
    /// reconsider whether `MustReachPhase` is the right surface (it
    /// shouldn't be — `Released` belongs on a separate "wait for
    /// terminal-reached gate" closed set). The compiler enforces
    /// closure-on-arity; this test enforces closure-on-semantics.
    #[test]
    fn must_reach_phase_projects_only_to_live_checkpoints() {
        for kind in MustReachPhase::ALL {
            let p = kind.as_process_phase();
            assert!(
                p.is_running(),
                "{kind:?} → {p:?} must be a live checkpoint (Running or Attested)",
            );
        }
    }

    /// INJECTIVITY CONTRACT: distinct `MustReachPhase` variants project
    /// to distinct `ProcessPhase` values. Pairing this with the subset
    /// contract above forces a future variant addition to land on a
    /// fresh live checkpoint — collapsing two `MustReachPhase` variants
    /// onto the same `ProcessPhase` (e.g. two flavors of `Running`)
    /// silently makes `from` lossy, which `tatara-reconciler::boundary::
    /// check_depends_on`'s diagnostic ("need {required}") would
    /// quietly degrade.
    #[test]
    fn must_reach_phase_projection_is_injective() {
        let mut seen = std::collections::HashSet::new();
        for kind in MustReachPhase::ALL {
            let p = kind.as_process_phase();
            assert!(
                seen.insert(p),
                "MustReachPhase projection collision: {kind:?} → {p:?}",
            );
        }
        assert_eq!(seen.len(), MustReachPhase::ALL.len());
    }
}
