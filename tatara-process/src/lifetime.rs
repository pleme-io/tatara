//! Process lifetime — Permanent (re-converging) vs Ephemeral (auto-SIGTERM
//! on Attested / TTL / Failed).
//!
//! The wire shape follows the same "exactly-one-optional-field" pattern as
//! `Intent` — one tagged-union idiom across the typescape.
//!
//! Lisp authoring:
//! ```lisp
//! :lifetime (:permanent)
//! :lifetime (:ephemeral :ttl "1h"
//!                       :teardown OnAttested
//!                       :max-concurrent 1)
//! ```
//!
//! Default = `Permanent` — every existing Process keeps its current behavior.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::export::ExportSpec;
use crate::phase::ProcessPhase;

/// Lifetime slot on `ProcessSpec`. Exactly one variant should be populated;
/// when both are unset the resolver returns `Permanent`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Lifetime {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permanent: Option<PermanentLifetime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<EphemeralLifetime>,
}

/// Resolved enum view used by the reconciler.
#[derive(Clone, Debug)]
pub enum LifetimeVariant<'a> {
    Permanent(&'a PermanentLifetime),
    Ephemeral(&'a EphemeralLifetime),
}

impl LifetimeVariant<'_> {
    /// Reverse projection — every borrowed variant knows its
    /// `LifetimeKind` discriminator. Pairs with `LifetimeKind::select`
    /// so `LifetimeKind::select(lifetime).map(|v| v.kind())` round-trips
    /// the closed set on the populated side; pinned by
    /// `lifetime_kind_round_trips_through_variant_kind`.
    pub fn kind(&self) -> LifetimeKind {
        match self {
            Self::Permanent(_) => LifetimeKind::Permanent,
            Self::Ephemeral(_) => LifetimeKind::Ephemeral,
        }
    }

    /// Projection to the inner `EphemeralLifetime` iff this variant is
    /// `Ephemeral`. ONE site owns the "give me only the ephemeral case"
    /// shape every consumer of the lifetime clock previously hand-rolled
    /// via `let Ok(LifetimeVariant::Ephemeral(e)) = ...`; pinned by
    /// `lifetime_variant_as_ephemeral_returns_inner_only_for_ephemeral`.
    pub fn as_ephemeral(&self) -> Option<&EphemeralLifetime> {
        match self {
            Self::Ephemeral(e) => Some(e),
            Self::Permanent(_) => None,
        }
    }

    /// Projection to the inner `PermanentLifetime` iff this variant is
    /// `Permanent`. Symmetric counterpart to [`Self::as_ephemeral`].
    pub fn as_permanent(&self) -> Option<&PermanentLifetime> {
        match self {
            Self::Permanent(p) => Some(p),
            Self::Ephemeral(_) => None,
        }
    }
}

/// Closed-set discriminator over `Lifetime`'s two tagged-union slots.
/// Single source of truth that drives `Lifetime::variant`'s ambiguity
/// resolver, the reverse `LifetimeVariant::kind` projection, and any
/// `select`-style routing. Adding a third lifetime variant (e.g. a
/// future `Burst` slot for budget-capped non-TTL lifetimes) lands at
/// one `ALL` entry + one `as_str` arm + one `select` arm + one
/// `LifetimeVariant::kind` arm — exhaustively checked by the compiler.
///
/// Sibling closed-set lift to [`crate::intent::IntentKind`] on the
/// same `ProcessSpec` axis. Same shape, smaller closed set, same
/// compounding pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LifetimeKind {
    Permanent,
    Ephemeral,
}

impl LifetimeKind {
    /// The closed set of lifetime kinds — single source of truth that
    /// drives `Lifetime::variant`'s sweep so a variant added without
    /// an `ALL` entry never reaches the resolver.
    pub const ALL: [Self; 2] = [Self::Permanent, Self::Ephemeral];

    /// Canonical lower-case wire-format key — matches the serde
    /// `rename_all = "camelCase"` field name on `Lifetime`. Pinned by
    /// `lifetime_kind_as_str_matches_lifetime_field_name`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Permanent => "permanent",
            Self::Ephemeral => "ephemeral",
        }
    }

    /// Project a `Lifetime` borrow into the optional typed variant view
    /// for this kind. Returns `None` iff the matching slot is `None`.
    /// Composes the closed-set sweep `Lifetime::variant` loops over.
    pub fn select<'a>(self, lifetime: &'a Lifetime) -> Option<LifetimeVariant<'a>> {
        match self {
            Self::Permanent => lifetime.permanent.as_ref().map(LifetimeVariant::Permanent),
            Self::Ephemeral => lifetime.ephemeral.as_ref().map(LifetimeVariant::Ephemeral),
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum LifetimeError {
    #[error("lifetime has multiple variants set; at most one required")]
    Ambiguous,
}

impl Lifetime {
    /// True when no variant is set — treated as `Permanent` by the resolver.
    pub fn is_default(&self) -> bool {
        self.permanent.is_none() && self.ephemeral.is_none()
    }

    /// Resolve to a variant view. Empty resolves to `Permanent` (a static
    /// borrow on the embedded `DEFAULT_PERMANENT`); ambiguous (both set) is
    /// an error.
    ///
    /// Sweeps over `LifetimeKind::ALL` so a third variant added with an
    /// `ALL` entry is structurally honored at this site — no parallel
    /// `is_some()` count, no per-variant if-let chain.
    pub fn variant(&self) -> Result<LifetimeVariant<'_>, LifetimeError> {
        use crate::tagged_union::{resolve, ResolveError};
        match resolve(LifetimeKind::ALL.into_iter().map(|k| k.select(self))) {
            Ok(v) => Ok(v),
            Err(ResolveError::None) => Ok(LifetimeVariant::Permanent(&DEFAULT_PERMANENT)),
            Err(ResolveError::Many) => Err(LifetimeError::Ambiguous),
        }
    }

    /// True iff `ephemeral` is set.
    pub fn is_ephemeral(&self) -> bool {
        self.ephemeral.is_some()
    }
}

const DEFAULT_PERMANENT: PermanentLifetime = PermanentLifetime {};

/// Permanent lifetime — the existing Process behavior. SIGHUP re-converges;
/// SIGTERM terminates only on explicit operator action.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PermanentLifetime {}

/// Ephemeral lifetime — Process auto-terminates per `teardown_policy`.
///
/// Phase semantics:
/// - On `Attested` with `teardown_policy ∈ {OnAttested, Always}`:
///   reconciler delivers SIGTERM, Process drives Exiting → Zombie → Reaped.
/// - On `Failed`  with `teardown_policy ∈ {OnFailed,   Always}`:
///   same. Otherwise Process stays at Failed for forensic inspection.
/// - `ttl` is a `humantime` duration (`"1h"`, `"30m"`) checked at every
///   reconcile loop tick. TTL expiry while in any non-terminal phase
///   forces SIGTERM regardless of `teardown_policy`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EphemeralLifetime {
    /// `humantime`-parseable duration from `phaseSince(Forking)` after
    /// which the Process is force-SIGTERM'd.
    #[serde(default = "default_ttl")]
    pub ttl: String,

    /// When the Process auto-terminates.
    #[serde(default)]
    pub teardown_policy: TeardownPolicy,

    /// Cluster-wide concurrency budget across ephemeral Processes that
    /// share the same `spec.identity.name_override` / chart_ref.
    /// `0` = no cap. Enforced by the reconciler before transitioning out
    /// of `Pending`.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,

    /// Declared exports — what artifacts survive teardown and where
    /// they flow. Empty (default) = nothing survives, matching the
    /// "ephemeral leaves no trace" posture. Each `ExportSpec` is
    /// independently triggered during the reconciler's `Releasing`
    /// phase against the terminal `ProcessPhase` reached.
    ///
    /// See [`crate::export`] for the full type. All exports flow
    /// through the pleme-io Vector + NATS layer — there is no
    /// per-spec ad-hoc sink.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<ExportSpec>,
}

impl EphemeralLifetime {
    /// True iff any declared export's [`crate::export::ExportTrigger`]
    /// fires for the given terminal-reached phase. The reconciler
    /// uses this to decide whether to route `Attested`/`Failed`
    /// through `Releasing` (the export window) or skip straight to
    /// `Exiting`/`Zombie`.
    ///
    /// Returns `false` when the export list is empty or no trigger
    /// matches — both cases collapse to the existing teardown path.
    pub fn has_applicable_exports(&self, phase: ProcessPhase) -> bool {
        self.exports.iter().any(|e| e.when.fires_on(phase))
    }

    /// Iterate over the exports whose trigger fires on `phase`.
    /// The reconciler's `handle_releasing` consumes this to emit
    /// one tatara-export-worker Job per surviving spec.
    pub fn applicable_exports(
        &self,
        phase: ProcessPhase,
    ) -> impl Iterator<Item = &ExportSpec> + '_ {
        self.exports.iter().filter(move |e| e.when.fires_on(phase))
    }
}

impl Default for EphemeralLifetime {
    fn default() -> Self {
        Self {
            ttl: default_ttl(),
            teardown_policy: TeardownPolicy::default(),
            max_concurrent: default_max_concurrent(),
            exports: Vec::new(),
        }
    }
}

fn default_ttl() -> String {
    "1h".to_string()
}
fn default_max_concurrent() -> u32 {
    1
}

/// When an ephemeral Process self-terminates.
///
/// Aligns with `ProcessPhase` (`Attested` / `Failed`) rather than borrowing
/// foreign success/failure language — typed phases are the source of truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
pub enum TeardownPolicy {
    /// SIGTERM as soon as the Process reaches `Attested` or `Failed`.
    #[default]
    Always,
    /// SIGTERM only on `Attested`. Leave `Failed` Processes for inspection.
    OnAttested,
    /// SIGTERM only on `Failed`. Leave `Attested` Processes running until
    /// TTL or explicit operator SIGTERM.
    OnFailed,
    /// Never auto-terminate (TTL still applies).
    Never,
}

impl TeardownPolicy {
    /// The closed set of teardown policies — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad and the typed
    /// `should_teardown_on` dispatch over `ProcessPhase`. Adding a fifth
    /// variant lands at one `ALL` entry + one `as_str` arm + one
    /// `should_teardown_on` arm — exhaustively checked by the compiler
    /// (the `[Self; 4]` array literal forces the arity).
    ///
    /// Sibling closed-set lifts on the same `ProcessSpec` axis:
    /// [`super::intent::IntentKind::ALL`], [`super::LifetimeKind::ALL`],
    /// [`crate::boundary::ConditionKind::ALL`],
    /// [`crate::phase::ProcessPhase::ALL`],
    /// [`crate::signal::ProcessSignal::ALL`].
    pub const ALL: [Self; 4] = [Self::Always, Self::OnAttested, Self::OnFailed, Self::Never];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim. Used by Display
    /// (single source of truth), by `FromStr` to identify the variant
    /// from its annotation / status-field representation, and by
    /// operator-facing reason strings the reconciler stamps without
    /// reaching for `{:?}` Debug formatting. Pinned by
    /// `teardown_policy_as_str_matches_serde`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Always => "Always",
            Self::OnAttested => "OnAttested",
            Self::OnFailed => "OnFailed",
            Self::Never => "Never",
        }
    }

    /// True iff, given a `ProcessPhase`, this policy says "tear down."
    /// ONE typed dispatch over the typed phase enum that replaces the
    /// pair of hand-rolled `matches!(self, Self::Always | Self::OnX)`
    /// predicates `lifetime_clock::evaluate` previously branched on.
    /// Non-terminal phases (`Pending` / `Forking` / `Execing` / `Running`
    /// / `Reconverging` / `Releasing` / `Exiting` / `Zombie` / `Reaped`)
    /// always return `false` — teardown is a terminal-phase decision.
    ///
    /// The legacy [`Self::should_teardown_on_attested`] /
    /// [`Self::should_teardown_on_failed`] predicates remain as thin
    /// delegates so existing call sites keep their narrow signatures;
    /// the truth table is pinned by
    /// `teardown_policy_legacy_predicates_delegate_to_phase_dispatch`.
    pub const fn should_teardown_on(self, phase: ProcessPhase) -> bool {
        match phase {
            ProcessPhase::Attested => matches!(self, Self::Always | Self::OnAttested),
            ProcessPhase::Failed => matches!(self, Self::Always | Self::OnFailed),
            ProcessPhase::Pending
            | ProcessPhase::Forking
            | ProcessPhase::Execing
            | ProcessPhase::Running
            | ProcessPhase::Reconverging
            | ProcessPhase::Releasing
            | ProcessPhase::Exiting
            | ProcessPhase::Zombie
            | ProcessPhase::Reaped => false,
        }
    }

    /// Thin delegate to [`Self::should_teardown_on`] for the `Attested`
    /// case — kept so existing call sites (notably the truth-table
    /// test in this module) keep their narrow signature without
    /// reaching for the typed-phase variant.
    pub const fn should_teardown_on_attested(self) -> bool {
        self.should_teardown_on(ProcessPhase::Attested)
    }

    /// Symmetric delegate to [`Self::should_teardown_on`] for the
    /// `Failed` case.
    pub const fn should_teardown_on_failed(self) -> bool {
        self.should_teardown_on(ProcessPhase::Failed)
    }
}

impl fmt::Display for TeardownPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TeardownPolicy {
    type Err = UnknownTeardownPolicy;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for kind in Self::ALL {
            if s == kind.as_str() {
                return Ok(kind);
            }
        }
        Err(UnknownTeardownPolicy(s.to_string()))
    }
}

/// Typed parse failure carrying the offending input verbatim so the
/// operator-facing diagnostic surfaces the bad value, not a normalized
/// form. Symmetric to [`crate::boundary::UnknownConditionKind`] and
/// [`crate::phase::UnknownPhase`].
#[derive(Debug, thiserror::Error)]
#[error("unknown teardown policy: {0}")]
pub struct UnknownTeardownPolicy(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_lifetime_resolves_to_permanent() {
        let l = Lifetime::default();
        assert!(l.is_default());
        assert!(!l.is_ephemeral());
        assert!(matches!(
            l.variant().unwrap(),
            LifetimeVariant::Permanent(_)
        ));
    }

    #[test]
    fn ephemeral_set_resolves() {
        let l = Lifetime {
            ephemeral: Some(EphemeralLifetime::default()),
            ..Lifetime::default()
        };
        assert!(l.is_ephemeral());
        match l.variant().unwrap() {
            LifetimeVariant::Ephemeral(e) => {
                assert_eq!(e.ttl, "1h");
                assert_eq!(e.teardown_policy, TeardownPolicy::Always);
                assert_eq!(e.max_concurrent, 1);
            }
            other => panic!("expected ephemeral, got {other:?}"),
        }
    }

    #[test]
    fn ambiguous_lifetime_errors() {
        let l = Lifetime {
            permanent: Some(PermanentLifetime {}),
            ephemeral: Some(EphemeralLifetime::default()),
        };
        assert_eq!(l.variant().unwrap_err(), LifetimeError::Ambiguous);
    }

    #[test]
    fn teardown_policy_dispatch() {
        assert!(TeardownPolicy::Always.should_teardown_on_attested());
        assert!(TeardownPolicy::Always.should_teardown_on_failed());
        assert!(TeardownPolicy::OnAttested.should_teardown_on_attested());
        assert!(!TeardownPolicy::OnAttested.should_teardown_on_failed());
        assert!(!TeardownPolicy::OnFailed.should_teardown_on_attested());
        assert!(TeardownPolicy::OnFailed.should_teardown_on_failed());
        assert!(!TeardownPolicy::Never.should_teardown_on_attested());
        assert!(!TeardownPolicy::Never.should_teardown_on_failed());
    }

    // ── closed-set algebra for TeardownPolicy (ALL × as_str × FromStr ×
    //    should_teardown_on(phase)) ─

    /// `ALL` is the source of truth for the resolver / `FromStr` sweep
    /// — pin its closure so a variant added without an `ALL` entry
    /// fails here (via the uniqueness check) before drifting `as_str` /
    /// `should_teardown_on`. The arity is asserted by the `[Self; 4]`
    /// array type itself.
    #[test]
    fn teardown_policy_all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for policy in TeardownPolicy::ALL {
            assert!(seen.insert(policy), "duplicate variant in ALL: {policy:?}");
        }
        assert_eq!(seen.len(), TeardownPolicy::ALL.len());
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site. The reason
    /// string `lifetime_clock::evaluate` stamps reaches for the same
    /// projection via `Display`, so a Debug-vs-canonical drift would
    /// surface here, not in operator-facing reason strings.
    #[test]
    fn teardown_policy_as_str_matches_serde() {
        for policy in TeardownPolicy::ALL {
            let serialized = serde_json::to_string(&policy)
                .expect("TeardownPolicy serializes")
                .trim_matches('"')
                .to_string();
            assert_eq!(
                policy.as_str(),
                serialized,
                "as_str() must match serde output for {policy:?}",
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers (notably `lifetime_clock::evaluate`'s reason string)
    /// reach for either projection without drift.
    #[test]
    fn teardown_policy_display_matches_as_str() {
        for policy in TeardownPolicy::ALL {
            assert_eq!(policy.to_string(), policy.as_str());
        }
    }

    /// ROUND-TRIP CONTRACT: every variant survives `as_str` ↔ `FromStr`.
    /// Adding a variant without extending `as_str` (or vice versa)
    /// fails here.
    #[test]
    fn teardown_policy_roundtrip_via_as_str() {
        use std::str::FromStr;
        for policy in TeardownPolicy::ALL {
            assert_eq!(
                TeardownPolicy::from_str(policy.as_str()).expect("known policy round-trips"),
                policy,
                "round-trip failed for {policy:?}",
            );
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — empty / lowercased / typo / unrelated — and the
    /// error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    #[test]
    fn unknown_teardown_policy_errors() {
        use std::str::FromStr;
        for bad in ["", "always", "ALWAYS", "OnAtested", "Bogus"] {
            let err = TeardownPolicy::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// TRUTH-TABLE CONTRACT: `should_teardown_on(phase)` agrees with
    /// the documented (policy, phase) → bool table for every variant
    /// at every typed phase. The two terminal phases (Attested,
    /// Failed) carry the policy-specific result; every non-terminal
    /// phase returns `false`. The closed-set sweep over both
    /// `TeardownPolicy::ALL` and `ProcessPhase::ALL` means a new
    /// variant in either enum reaches this test by iteration — no
    /// per-test array maintenance.
    #[test]
    fn teardown_policy_should_teardown_on_truth_table() {
        for policy in TeardownPolicy::ALL {
            for phase in ProcessPhase::ALL {
                let expected = match phase {
                    ProcessPhase::Attested => {
                        matches!(policy, TeardownPolicy::Always | TeardownPolicy::OnAttested)
                    }
                    ProcessPhase::Failed => {
                        matches!(policy, TeardownPolicy::Always | TeardownPolicy::OnFailed)
                    }
                    _ => false,
                };
                assert_eq!(
                    policy.should_teardown_on(phase),
                    expected,
                    "should_teardown_on({policy:?}, {phase:?}) drift",
                );
            }
        }
    }

    /// DELEGATION CONTRACT: the legacy `should_teardown_on_attested` /
    /// `should_teardown_on_failed` predicates agree with the typed
    /// `should_teardown_on(phase)` dispatch they delegate to, for
    /// every variant. A regression that re-introduces an inline
    /// `matches!` in either legacy predicate fails here the moment
    /// `should_teardown_on` is the source of truth.
    #[test]
    fn teardown_policy_legacy_predicates_delegate_to_phase_dispatch() {
        for policy in TeardownPolicy::ALL {
            assert_eq!(
                policy.should_teardown_on_attested(),
                policy.should_teardown_on(ProcessPhase::Attested),
                "Attested delegate drift for {policy:?}",
            );
            assert_eq!(
                policy.should_teardown_on_failed(),
                policy.should_teardown_on(ProcessPhase::Failed),
                "Failed delegate drift for {policy:?}",
            );
        }
    }

    #[test]
    fn serde_round_trip_ephemeral() {
        let l = Lifetime {
            ephemeral: Some(EphemeralLifetime {
                ttl: "30m".into(),
                teardown_policy: TeardownPolicy::OnAttested,
                max_concurrent: 4,
                exports: vec![],
            }),
            ..Lifetime::default()
        };
        let yaml = serde_yaml::to_string(&l).unwrap();
        assert!(yaml.contains("ttl: 30m"));
        assert!(yaml.contains("teardownPolicy: OnAttested"));
        // Empty exports skip-serialize — explicit zero-trace default.
        assert!(!yaml.contains("exports"));
        let back: Lifetime = serde_yaml::from_str(&yaml).unwrap();
        assert!(back.is_ephemeral());
        assert!(back.ephemeral.unwrap().exports.is_empty());
    }

    #[test]
    fn applicable_exports_filters_by_trigger() {
        use crate::export::{
            ArtifactSource, ExportSpec, ExportTrigger, HttpEventChannel, ReceiptsSource,
            VectorChannel,
        };
        let spec_attested = ExportSpec {
            source: ArtifactSource {
                receipts: Some(ReceiptsSource::default()),
                ..ArtifactSource::default()
            },
            channel: VectorChannel {
                http_event: Some(HttpEventChannel {
                    endpoint: None,
                    signal_type: "receipt".into(),
                }),
                ..VectorChannel::default()
            },
            when: ExportTrigger::OnAttested,
            experiment_id_override: None,
        };
        let spec_failed = ExportSpec {
            when: ExportTrigger::OnFailed,
            ..spec_attested.clone()
        };
        let spec_always = ExportSpec {
            when: ExportTrigger::Always,
            ..spec_attested.clone()
        };

        let lt = EphemeralLifetime {
            ttl: "1h".into(),
            teardown_policy: TeardownPolicy::OnAttested,
            max_concurrent: 1,
            exports: vec![spec_attested, spec_failed, spec_always],
        };

        // Attested gate fires OnAttested + Always — 2 of 3.
        assert!(lt.has_applicable_exports(ProcessPhase::Attested));
        assert_eq!(lt.applicable_exports(ProcessPhase::Attested).count(), 2);

        // Failed gate fires OnFailed + Always — 2 of 3.
        assert!(lt.has_applicable_exports(ProcessPhase::Failed));
        assert_eq!(lt.applicable_exports(ProcessPhase::Failed).count(), 2);

        // Other phases never route through Releasing.
        for p in [
            ProcessPhase::Pending,
            ProcessPhase::Forking,
            ProcessPhase::Execing,
            ProcessPhase::Running,
            ProcessPhase::Reconverging,
            ProcessPhase::Releasing,
            ProcessPhase::Exiting,
            ProcessPhase::Zombie,
            ProcessPhase::Reaped,
        ] {
            assert!(!lt.has_applicable_exports(p));
            assert_eq!(lt.applicable_exports(p).count(), 0);
        }
    }

    #[test]
    fn no_exports_means_no_applicable_exports() {
        let lt = EphemeralLifetime::default();
        assert!(!lt.has_applicable_exports(ProcessPhase::Attested));
        assert!(!lt.has_applicable_exports(ProcessPhase::Failed));
    }

    /// `ALL` is the source of truth for the resolver sweep — pin its
    /// closure so a variant added without an `ALL` entry fails here
    /// (via the uniqueness check) before drifting `variant()`.
    #[test]
    fn lifetime_kind_all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for kind in LifetimeKind::ALL {
            assert!(seen.insert(kind), "duplicate variant in ALL: {kind:?}");
        }
        assert_eq!(seen.len(), LifetimeKind::ALL.len());
    }

    /// CANONICAL-KEY CONTRACT: each variant's `as_str()` matches the
    /// camelCase serde field name on `Lifetime`. A future rename of
    /// any field lands here at one site.
    #[test]
    fn lifetime_kind_as_str_matches_lifetime_field_name() {
        for kind in LifetimeKind::ALL {
            let l = match kind {
                LifetimeKind::Permanent => Lifetime {
                    permanent: Some(PermanentLifetime {}),
                    ..Lifetime::default()
                },
                LifetimeKind::Ephemeral => Lifetime {
                    ephemeral: Some(EphemeralLifetime::default()),
                    ..Lifetime::default()
                },
            };
            let v = serde_json::to_value(&l).expect("Lifetime serializes");
            let obj = v.as_object().expect("Lifetime serializes to object");
            let keys: Vec<&String> = obj.keys().collect();
            assert_eq!(
                keys.len(),
                1,
                "exactly one slot populated for kind {kind:?}, got {keys:?}"
            );
            assert_eq!(
                keys[0],
                kind.as_str(),
                "as_str() must match serde field name for {kind:?}"
            );
        }
    }

    /// ROUND-TRIP CONTRACT: `LifetimeKind::select(lifetime).map(|v|
    /// v.kind()) == Some(kind)`. The reverse `LifetimeVariant::kind`
    /// projection composes the closed set in both directions — a
    /// regression that misroutes a select arm (e.g. `Self::Permanent =>
    /// l.ephemeral.as_ref()...`) fails loudly here.
    #[test]
    fn lifetime_kind_round_trips_through_variant_kind() {
        for kind in LifetimeKind::ALL {
            let l = single_slot_lifetime(kind);
            let v = kind.select(&l).expect("populated slot must select");
            assert_eq!(v.kind(), kind, "round-trip failed for {kind:?}");
            // And the resolver lands on the same variant.
            assert_eq!(
                l.variant().expect("exactly-one variant").kind(),
                kind,
                "variant() resolver disagreed on {kind:?}"
            );
        }
    }

    /// `as_ephemeral` returns `Some` iff the variant is `Ephemeral`.
    /// Pins the lift of the `let Ok(LifetimeVariant::Ephemeral(e)) = ...`
    /// pattern that `lifetime_clock::evaluate` + `requeue_with_ttl`
    /// previously hand-rolled.
    #[test]
    fn lifetime_variant_as_ephemeral_returns_inner_only_for_ephemeral() {
        let permanent = PermanentLifetime {};
        let v = LifetimeVariant::Permanent(&permanent);
        assert!(v.as_ephemeral().is_none());
        assert!(v.as_permanent().is_some());

        let ephemeral = EphemeralLifetime {
            ttl: "42m".into(),
            teardown_policy: TeardownPolicy::OnAttested,
            max_concurrent: 3,
            exports: vec![],
        };
        let v = LifetimeVariant::Ephemeral(&ephemeral);
        let inner = v.as_ephemeral().expect("ephemeral must project");
        assert_eq!(inner.ttl, "42m");
        assert_eq!(inner.teardown_policy, TeardownPolicy::OnAttested);
        assert_eq!(inner.max_concurrent, 3);
        assert!(v.as_permanent().is_none());
    }

    /// EMPTY-RESOLVES-TO-PERMANENT CONTRACT: the resolver's "no slot
    /// set" outcome is `Permanent`, not an error. Pin via the
    /// closed-set kind projection so a future variant added to the
    /// closed set (and to the `Lifetime` struct) without updating
    /// the default resolution would surface here — the default
    /// stays `Permanent` regardless of the closed set's arity.
    #[test]
    fn empty_lifetime_resolves_to_permanent_kind() {
        let l = Lifetime::default();
        let v = l.variant().expect("default lifetime resolves");
        assert_eq!(v.kind(), LifetimeKind::Permanent);
        assert!(v.as_permanent().is_some());
        assert!(v.as_ephemeral().is_none());
    }

    /// Construct a `Lifetime` with exactly the given kind's slot
    /// populated by a minimal valid inner spec. Shared across the
    /// closed-set property tests so they each cover every variant
    /// without restating the construction table.
    fn single_slot_lifetime(kind: LifetimeKind) -> Lifetime {
        match kind {
            LifetimeKind::Permanent => Lifetime {
                permanent: Some(PermanentLifetime {}),
                ..Lifetime::default()
            },
            LifetimeKind::Ephemeral => Lifetime {
                ephemeral: Some(EphemeralLifetime::default()),
                ..Lifetime::default()
            },
        }
    }

    #[test]
    fn exports_round_trip_through_lifetime() {
        use crate::export::{
            ArtifactSource, ExportSpec, ExportTrigger, HttpEventChannel, ReceiptsSource,
            VectorChannel,
        };
        let l = Lifetime {
            ephemeral: Some(EphemeralLifetime {
                ttl: "30m".into(),
                teardown_policy: TeardownPolicy::OnAttested,
                max_concurrent: 1,
                exports: vec![ExportSpec {
                    source: ArtifactSource {
                        receipts: Some(ReceiptsSource::default()),
                        ..ArtifactSource::default()
                    },
                    channel: VectorChannel {
                        http_event: Some(HttpEventChannel {
                            endpoint: None,
                            signal_type: "receipt".into(),
                        }),
                        ..VectorChannel::default()
                    },
                    when: ExportTrigger::OnAttested,
                    experiment_id_override: None,
                }],
            }),
            ..Lifetime::default()
        };
        let yaml = serde_yaml::to_string(&l).unwrap();
        assert!(yaml.contains("exports:"));
        assert!(yaml.contains("receipts: {}"));
        assert!(yaml.contains("signalType: receipt"));
        let back: Lifetime = serde_yaml::from_str(&yaml).unwrap();
        let e = back.ephemeral.unwrap();
        assert_eq!(e.exports.len(), 1);
        assert!(e.exports[0].source.receipts.is_some());
        assert!(e.exports[0].channel.http_event.is_some());
    }
}
