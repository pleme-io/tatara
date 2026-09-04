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
    /// `lifetime_kind_round_trips_through_variant_kind` locally + the
    /// substrate trait [`crate::tagged_union::VariantKind`] shared with
    /// every sibling borrowed-view enum. The impl below delegates to
    /// this body as the ground-truth arm-to-Kind mapping.
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

impl crate::tagged_union::VariantKind<LifetimeKind> for LifetimeVariant<'_> {
    fn variant_kind(&self) -> LifetimeKind {
        self.kind()
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
/// compounding pattern. Adopts `#[derive(DeriveClosedSet)]` +
/// `#[closed_set(via = "as_str", generate_unknown, display)]` so
/// [`tatara_closed_set::ClosedSet`], [`std::fmt::Display`],
/// [`std::str::FromStr`], and the [`UnknownLifetimeKind`] carrier
/// all emerge from ONE derive on the substrate-wide shape every
/// sibling closed-set discriminator across the crate publishes —
/// no hand-rolled `impl` blocks, no drift-risk between the four
/// projections. The parent `Lifetime` doesn't impl
/// [`crate::tagged_union::TaggedUnion`] (empty resolves to
/// `Permanent(&DEFAULT_PERMANENT)`, not to an error), so the
/// `TaggedUnion`-bound substrate primitives don't reach it; the
/// closed-set-bound peer
/// [`crate::tagged_union::assert_wire_key_matches_label`] does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_closed_set::DeriveClosedSet)]
#[closed_set(via = "as_str", generate_unknown, display)]
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

    /// Compound projection: `Some(&e)` iff [`Self::variant`] resolves
    /// unambiguously to `Ephemeral(e)`; `None` for every other outcome
    /// (empty → `Permanent` default, `Permanent` slot only, or
    /// [`LifetimeError::Ambiguous`] when BOTH slots are set).
    ///
    /// The ambiguous case is deliberately collapsed to `None`: an
    /// operator-authored spec with both `permanent:` and `ephemeral:`
    /// populated is a mis-configuration, and every production consumer
    /// of the pair [`crate::lifetime_clock::evaluate`] +
    /// [`crate::lifetime_clock::requeue_with_ttl`] previously
    /// hand-rolled the SAME two-step projection
    /// (`variant().ok()?.as_ephemeral()`) whose Err-arm and
    /// Permanent-arm both fell through to the same "no ephemeral
    /// action" outcome (`AutoTerminate::Skip` / `default` requeue).
    /// Lifting that chained collapse to ONE substrate primitive puts
    /// "the ephemeral spec now, iff the resolver picked it" behind a
    /// single call site and closes the possibility of a per-consumer
    /// drift where one branch honors ambiguity and the other doesn't.
    ///
    /// A future third variant added to `Lifetime` (e.g. `Burst` for
    /// budget-capped non-TTL lifetimes) reaches this projection
    /// through the SAME [`Self::variant`] resolver + the SAME
    /// [`LifetimeVariant::as_ephemeral`] discriminator, so the
    /// ephemeral-only projection stays intact without a new arm here.
    ///
    /// Pinned by
    /// `resolved_ephemeral_projects_only_the_unambiguous_ephemeral_slot`.
    pub fn resolved_ephemeral(&self) -> Option<&EphemeralLifetime> {
        // Pattern-match on the owned `LifetimeVariant` (not
        // `variant.as_ephemeral()`) so the returned borrow carries the
        // resolver's `'_self` lifetime through directly instead of the
        // shorter borrow `as_ephemeral(&self)` synthesizes on the
        // temporary variant. Symmetric peer discriminator arm
        // `LifetimeVariant::as_ephemeral` still owns the closed-set
        // projection for consumers that hold the variant by borrow;
        // this projection is the compound-lift entry point for
        // consumers whose call graph starts from `&Lifetime`.
        match self.variant().ok()? {
            LifetimeVariant::Ephemeral(e) => Some(e),
            LifetimeVariant::Permanent(_) => None,
        }
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
    /// The [`humantime`]-parsed `self.ttl` duration, or `None` if the
    /// operator-authored `ttl` string doesn't parse — the one-line
    /// collapse of the `humantime::parse_duration(&<eph>.ttl).ok()`
    /// chain lifted to ONE typed owner past the ★★ PRIME-DIRECTIVE
    /// ≥ 2 duplication threshold.
    ///
    /// Pre-lift the SAME chain was hand-authored at TWO workspace-wide
    /// consumer sites in [`crate::lifetime_clock`], both walking
    /// `humantime::parse_duration(&<ephemeral>.ttl)` on an
    /// `&EphemeralLifetime` and discarding the parse-error arm to the
    /// downstream "skip the timed decision" branch:
    ///
    /// * [`crate::lifetime_clock::evaluate`] — the TTL-expiry gate.
    ///   Reads `if let Ok(ttl) = humantime::parse_duration(&ephemeral
    ///   .ttl) { … }` inside the non-terminal-phase guard, comparing
    ///   the parsed `Duration` against the wall-clock elapsed distance
    ///   from `metadata.creation_timestamp` to fire
    ///   `AutoTerminate::Now { TtlExpired }`.
    /// * [`crate::lifetime_clock::requeue_with_ttl`] — the sleep-
    ///   budget picker for the reconciler's next requeue. Reads
    ///   `let Ok(ttl) = humantime::parse_duration(&e.ttl) else {
    ///   return default; };` and short-circuits to the caller's
    ///   `default` sleep budget on parse failure.
    ///
    /// Both sites walked the SAME `humantime::parse_duration(&<eph>
    /// .ttl)` chain and both wanted the Option-shape (the `Ok` arm as
    /// the parsed `Duration`, the `Err` arm collapsed to the
    /// downstream skip-branch). Post-lift each caller reaches for
    /// `<eph>.ttl_duration()` and applies its own tail at its own
    /// site (`if let Some(ttl) = …` for the guard, `let Some(ttl) =
    /// … else { return default; }` for the sleep-budget picker).
    ///
    /// Return-form axis: `Option<std::time::Duration>` matches the
    /// downstream comparator's type. The peer projection
    /// [`crate::time::elapsed_since`] returns the SAME
    /// `Option<std::time::Duration>` shape, so the TTL-expiry gate's
    /// `elapsed >= ttl` comparator and the sleep-budget picker's
    /// `ttl.checked_sub(elapsed)` subtraction each land with both
    /// operands on the same axis, no per-consumer conversion.
    ///
    /// The `None` arm is the "operator's ttl string doesn't parse"
    /// corner — a typo (`"1our"`), an unsupported unit, a
    /// non-humantime literal that reached the field. Every consumer
    /// interprets the corner as "no ttl data → don't fire the timed
    /// decision" — [`crate::lifetime_clock::evaluate`] skips the
    /// `AutoTerminate::Now` branch, [`crate::lifetime_clock::
    /// requeue_with_ttl`] returns the caller's `default` sleep
    /// budget. The pins below bind that shape.
    ///
    /// A future normalization (a per-fleet minimum TTL floor before
    /// the humantime cast, a canonical unit-normalization pass, a
    /// warn-log on unparseable strings) lands at THIS ONE substrate
    /// primitive and every downstream ephemeral-TTL consumer inherits
    /// the upgrade mechanically — no per-site edit at either of the
    /// TWO listed callers or at future consumers (an allocation-TTL
    /// remaining-budget picker, a pool free-TTL floor gate, a
    /// stable-name claim-arbiter max-age tie-break).
    ///
    /// Sibling substrate primitive on the same
    /// `(humantime string × Option<Duration>) → Option<Duration>`
    /// axis: [`crate::time::elapsed_since`] — the `(now, anchor) →
    /// Option<Duration>` peer that every timed-decision gate
    /// composes with THIS primitive to produce an `elapsed >= ttl` /
    /// `ttl.checked_sub(elapsed)` comparison.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `humantime::parse_duration(&<eph>.ttl).ok()` chain recurred
    /// at two hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the parse-failure corner AND the empty-ttl corner
    /// AND the humantime edge shapes AND the return-form parity with
    /// [`crate::time::elapsed_since`], so a regression that drifts any
    /// surface fails at `tests::ttl_duration_*` here rather than as
    /// silent operator-facing skew between the TTL-expiry gate and
    /// the sleep-budget picker on the SAME EphemeralLifetime).
    #[must_use]
    pub fn ttl_duration(&self) -> Option<std::time::Duration> {
        humantime::parse_duration(&self.ttl).ok()
    }

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
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Default,
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", display, generate_unknown)]
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

// `impl fmt::Display for TeardownPolicy` + `impl FromStr for
// TeardownPolicy` + `impl tatara_lisp::ClosedSet for TeardownPolicy` +
// `pub struct UnknownTeardownPolicy(pub String)` are generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(via =
// "as_str", display, generate_unknown)]` on the enum declaration above.
// The auto-derived label `"teardown policy"` matches the prior hand-
// rolled `#[error("unknown teardown policy: {0}")]` verbatim. The
// inherent `as_str` projection stays load-bearing — the PascalCase
// wire-format that matches the serde rename + the reconciler's reason-
// string emission verbatim — while the trait method `label` gives
// generic consumers a STABLE name across the 36+ workspace-wide
// closed-set implementors.

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

    /// Structural well-formedness of [`TeardownPolicy`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through `label ↔
    /// parse_label`, labels are pairwise distinct, `""` is outside the
    /// closed set) at ONE call site. Replaces the hand-derived
    /// `teardown_policy_all_is_unique_and_complete` +
    /// `teardown_policy_roundtrip_via_as_str` + the empty-input arm of
    /// `unknown_teardown_policy_errors`. `FromStr` delegates to
    /// `<Self as tatara_closed_set::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the reconciler hits when parsing a
    /// CRD `enum:`-validated value back to the typed policy.
    #[test]
    fn teardown_policy_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<TeardownPolicy>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site. The reason
    /// string `lifetime_clock::evaluate` stamps reaches for the same
    /// projection via `Display`, so a Debug-vs-canonical drift would
    /// surface here, not in operator-facing reason strings.
    #[test]
    fn teardown_policy_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<TeardownPolicy>();
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers (notably `lifetime_clock::evaluate`'s reason string)
    /// reach for either projection without drift.
    #[test]
    fn teardown_policy_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<TeardownPolicy>();
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / unrelated — and the error
    /// echoes the input verbatim so the operator-facing diagnostic
    /// carries the offending value, not a normalized form. The
    /// empty-input arm is pinned by
    /// [`teardown_policy_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownTeardownPolicy`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_teardown_policy_errors() {
        use std::str::FromStr;
        for bad in ["always", "ALWAYS", "OnAtested", "Bogus"] {
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
                http_event: Some(HttpEventChannel::signal("receipt")),
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

    /// Structural well-formedness of [`LifetimeKind`] as a
    /// [`tatara_closed_set::ClosedSet`] implementor — the workspace-
    /// wide testkit that pins ALL structural invariants (`ALL` is
    /// non-empty, every variant round-trips through `label ↔
    /// parse_label`, labels are pairwise distinct, `""` is outside
    /// the closed set, the [`UnknownLifetimeKind`] carrier's Display
    /// renders the substrate-wide `"unknown lifetime kind: <input>"`
    /// shape, `labels()` equals the natural `ALL × label` projection)
    /// at ONE call site. Subsumes the hand-derived
    /// `lifetime_kind_all_is_unique_and_complete` sweep the pre-derive
    /// site published — clauses (1)+(3) of the testkit fold uniqueness
    /// + non-emptiness into the substrate primitive's own body.
    #[test]
    fn lifetime_kind_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<LifetimeKind>();
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. Symmetric to every
    /// sibling `X_display_matches_as_str` invariant across
    /// `tatara-process`; routes through the substrate primitive
    /// [`crate::tagged_union::assert_display_matches_label`] shared
    /// with all 29+ production Display-alignment sites. The auto-
    /// derived `Display` body from `#[closed_set(via = "as_str",
    /// display)]` emits the substrate-wide `f.write_str(Self::as_str
    /// (*self))` shape — a regression that regresses `as_str` (or a
    /// future hand-rolled Display block that drifts from `as_str`)
    /// surfaces here at the substrate-wide alignment probe.
    #[test]
    fn lifetime_kind_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<LifetimeKind>();
    }

    /// CANONICAL-KEY CONTRACT: each variant's `as_str()` matches the
    /// camelCase serde field name on `Lifetime`. A future rename of
    /// any field lands here at one site — and the wire-key alignment
    /// stays coherent with the operator-facing serde shape.
    ///
    /// Routes through the substrate primitive
    /// [`crate::tagged_union::assert_wire_key_matches_label`] — the
    /// bound-relaxed peer of `assert_single_slot_key_matches_label`
    /// that drops the `T: TaggedUnion` requirement so `Lifetime`
    /// (whose empty variant resolves to `Permanent(&DEFAULT_PERMANENT)`
    /// rather than to a [`crate::tagged_union::TaggedUnionError::empty`]
    /// carrier) still binds through ONE substrate wire-key alignment
    /// site. Pre-lift the body restated the same serialize +
    /// exactly-one-key + name-equality sweep at this test surface
    /// verbatim; post-lift the projection lives at ONE substrate
    /// primitive and this site binds through a single call — the
    /// same mechanical shape the four sibling TaggedUnion parents
    /// carry via the trait-projected [`crate::tagged_union::assert_single_slot_key_matches_label`].
    #[test]
    fn lifetime_kind_as_str_matches_lifetime_field_name() {
        crate::tagged_union::assert_wire_key_matches_label::<Lifetime, LifetimeKind, _>(
            single_slot_lifetime,
        );
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

    /// `Lifetime::resolved_ephemeral` — the compound-lift primitive that
    /// composes `variant().ok() + as_ephemeral` — projects to `Some(&e)`
    /// iff the resolver picks the ephemeral slot unambiguously. All
    /// three failure modes (empty → permanent default, permanent-only,
    /// ambiguous) collapse to `None`, matching the pre-lift
    /// `lifetime_clock::evaluate` + `requeue_with_ttl` "no ephemeral
    /// action" outcome (`AutoTerminate::Skip` / `default` requeue).
    ///
    /// The ambiguous → `None` arm is DELIBERATELY the same outcome as
    /// permanent-only: an operator-authored spec with both slots
    /// populated is a mis-configuration, and firing TTL / teardown on
    /// it would be worse than skipping. Pinning that collapse here
    /// closes the possibility of a future per-consumer drift where one
    /// branch honors ambiguity (fires the timed action) and another
    /// doesn't.
    ///
    /// The `Some` arm asserts byte-identity of the projected borrow
    /// against `self.ephemeral.as_ref().unwrap()` — a mis-wire that
    /// silently swapped the projection to `self.permanent.as_ref()`
    /// would surface here as a type mismatch rather than as a runtime
    /// no-op in production.
    #[test]
    fn resolved_ephemeral_projects_only_the_unambiguous_ephemeral_slot() {
        // 1. Empty (both slots None) — resolves to Permanent default.
        let l = Lifetime::default();
        assert!(l.resolved_ephemeral().is_none());

        // 2. Permanent-only.
        let l = Lifetime {
            permanent: Some(PermanentLifetime {}),
            ..Lifetime::default()
        };
        assert!(l.resolved_ephemeral().is_none());

        // 3. Ephemeral-only — the ONE arm that projects.
        let ephemeral = EphemeralLifetime {
            ttl: "13m".into(),
            teardown_policy: TeardownPolicy::OnFailed,
            max_concurrent: 7,
            exports: vec![],
        };
        let l = Lifetime {
            ephemeral: Some(ephemeral.clone()),
            ..Lifetime::default()
        };
        let e = l.resolved_ephemeral().expect("ephemeral-only must project");
        assert_eq!(e.ttl, "13m");
        assert_eq!(e.teardown_policy, TeardownPolicy::OnFailed);
        assert_eq!(e.max_concurrent, 7);
        // The borrow points into `self.ephemeral`, not into a temporary.
        assert!(std::ptr::eq(e, l.ephemeral.as_ref().unwrap()));

        // 4. Ambiguous (both slots set) — collapses to None, NOT to
        //    the ephemeral inner. Guards against a future refactor
        //    that silently unwrapped ambiguity to "prefer ephemeral".
        let l = Lifetime {
            permanent: Some(PermanentLifetime {}),
            ephemeral: Some(EphemeralLifetime::default()),
        };
        assert_eq!(l.variant().unwrap_err(), LifetimeError::Ambiguous);
        assert!(l.resolved_ephemeral().is_none());
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
                        http_event: Some(HttpEventChannel::signal("receipt")),
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

    // ─── EphemeralLifetime::ttl_duration substrate pins ──────────────
    //
    // The `humantime::parse_duration(&<eph>.ttl).ok()` chain was open-
    // lifted from TWO consumer sites in `crate::lifetime_clock`
    // (`evaluate` + `requeue_with_ttl`) onto the ONE substrate
    // primitive [`EphemeralLifetime::ttl_duration`]. These pins bind
    // the primitive at the fail-before-pass-after level so a future
    // regression that swaps the return-form (an
    // `anyhow::Result<Duration>` — a per-consumer normalization gate
    // — a saturating `Duration::ZERO` for the parse-error corner)
    // fails HERE before landing at either consumer.

    fn eph_with_ttl(ttl: &str) -> EphemeralLifetime {
        EphemeralLifetime {
            ttl: ttl.to_string(),
            teardown_policy: TeardownPolicy::default(),
            max_concurrent: 1,
            exports: Vec::new(),
        }
    }

    /// The canonical shape every consumer rides through — a parseable
    /// humantime string projects to `Some(std::time::Duration)` matching
    /// the operator-authored `ttl` verbatim. Pin: the returned duration
    /// is EXACTLY what `humantime::parse_duration` produces for the
    /// same input, in `std::time::Duration` so the downstream
    /// `elapsed >= ttl` / `ttl.checked_sub(elapsed)` comparators land
    /// with both operands on the same axis without a per-consumer
    /// conversion.
    #[test]
    fn ttl_duration_parseable_humantime_projects_to_some() {
        for (ttl, expected) in [
            ("1h", std::time::Duration::from_secs(3600)),
            ("30m", std::time::Duration::from_secs(1800)),
            ("90s", std::time::Duration::from_secs(90)),
            ("5m30s", std::time::Duration::from_secs(330)),
            ("500ms", std::time::Duration::from_millis(500)),
        ] {
            assert_eq!(
                eph_with_ttl(ttl).ttl_duration(),
                Some(expected),
                "ttl_duration drift for {ttl:?}",
            );
        }
    }

    /// The `None` arm is the "operator's ttl string doesn't parse"
    /// corner every consumer collapses to the skip-branch — a typo, an
    /// unsupported unit, an empty string, a free-form label that
    /// reached the field. Pin the boundary at the primitive so a
    /// future normalization can't silently substitute a default in
    /// place of the parse-failure signal.
    #[test]
    fn ttl_duration_unparseable_returns_none() {
        for bad in [
            // Empty string — the operator left the ttl blank.
            "", // Non-humantime literal — the operator wrote a foreign format.
            "forever", "1our",
            // Nonsense that looks numeric but isn't a humantime span.
            "abc",
            // Only-whitespace input — passes serde's non-empty gate but
            // doesn't parse as a duration.
            "   ",
        ] {
            assert_eq!(
                eph_with_ttl(bad).ttl_duration(),
                None,
                "ttl_duration should be None for {bad:?}",
            );
        }
    }

    /// Zero-duration edge — `"0s"` parses to `Duration::ZERO`, not
    /// `None`. Every consumer needs the zero-ttl EphemeralLifetime to
    /// count as "elapsed=0 already ≥ ttl=0" so a zero-TTL ephemeral
    /// expires on its own creation instant; swapping this arm to
    /// `None` would silently keep every zero-TTL Process alive past
    /// the TTL-expiry gate in [`crate::lifetime_clock::evaluate`].
    #[test]
    fn ttl_duration_zero_seconds_returns_some_zero() {
        assert_eq!(
            eph_with_ttl("0s").ttl_duration(),
            Some(std::time::Duration::ZERO),
        );
    }

    /// Subsecond precision survives the parse — a regression that
    /// silently truncated to whole seconds would compare
    /// `elapsed = 500ms` against a `ttl_duration()` of `500ms` as
    /// `500ms >= 0s` (always fire) rather than `500ms >= 500ms`
    /// (fires on the boundary). Peer to the sibling substrate
    /// `elapsed_since` subsecond pin in `crate::time`.
    #[test]
    fn ttl_duration_preserves_subsecond_precision() {
        assert_eq!(
            eph_with_ttl("250ms").ttl_duration(),
            Some(std::time::Duration::from_millis(250)),
        );
    }

    /// Default `EphemeralLifetime` carries the canonical `"1h"` ttl
    /// (matches `default_ttl()` at this file's top), so
    /// `.ttl_duration()` on it agrees with a manually-parsed `"1h"`
    /// pass through `humantime`. Pins the default-ttl contract at
    /// the substrate so a future default rename lands at ONE site
    /// (this ttl_duration pin + the `default_ttl` fn) without silent
    /// wall-clock drift at either consumer.
    #[test]
    fn ttl_duration_of_default_ephemeral_matches_1h() {
        let e = EphemeralLifetime::default();
        assert_eq!(e.ttl, "1h");
        assert_eq!(e.ttl_duration(), Some(std::time::Duration::from_secs(3600)));
    }

    /// Byte-for-byte parity with the pre-lift hand-authored chain —
    /// `<eph>.ttl_duration()` produces the SAME
    /// `Option<std::time::Duration>` as the two-link `humantime::
    /// parse_duration(&<eph>.ttl).ok()` chain both `lifetime_clock`
    /// consumers walked pre-lift. A regression at THIS pin fails
    /// before it lands at either consumer as silent operator-facing
    /// skew between the TTL-expiry gate and the sleep-budget picker.
    #[test]
    fn ttl_duration_matches_pre_lift_hand_authored_chain_bytewise() {
        for ttl in [
            "1h", "30m", "90s", "5m30s", "500ms", "0s", "1us", "forever", "", "1our",
        ] {
            let e = eph_with_ttl(ttl);
            let via_primitive = e.ttl_duration();
            let hand_authored = humantime::parse_duration(&e.ttl).ok();
            assert_eq!(
                via_primitive, hand_authored,
                "ttl_duration must be byte-identical to `humantime::\
                 parse_duration(&self.ttl).ok()` for {ttl:?}",
            );
        }
    }
}
