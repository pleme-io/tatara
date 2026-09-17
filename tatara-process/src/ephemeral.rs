//! `EphemeralSpec` — the operator-facing typed surface for ephemeral
//! Aplicacao installations.
//!
//! `EphemeralSpec` is *sugar* on top of `ProcessSpec`. The compounding move
//! is to keep one wire format (`Process`, the Unix-process CRD) and let
//! ephemeral envs be a Process with `:intent (:aplicacao …)` +
//! `:lifetime (:ephemeral …)`. This struct gives that combination a
//! dedicated `(defephemeral …)` keyword and a typed `From` bridge so
//! authoring stays first-class without forking the CRD.
//!
//! Lisp authoring:
//! ```lisp
//! (defephemeral closed-loop-attest
//!   :aplicacao  (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-demo-app"
//!                :version "0.5.5"
//!                :profile "all-in-one"
//!                :values-overlay (:cluster (:name "ephemeral-test-01")
//!                                 :persistence false))
//!   :ttl        "1h"
//!   :teardown   OnAttested
//!   :postconditions
//!     ((:kind HelmReleaseReleased
//!       :params (:name "demo-app-consolidated"
//!                :namespace "demo-test"))
//!      (:kind ClosedLoopAuth
//!       :params (:issuer (:service "demo-app-issuer" :port 8080)
//!                :consumer (:service "demo-app-gateway" :port 8000)
//!                :probeImage "ghcr.io/pleme-io/closed-loop-probe:0.1.0"))))
//! ```

use std::borrow::Cow;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

use crate::boundary::{Boundary, Condition, ConditionKind, ConditionSliceExt};
use crate::classification::{
    Arity, CalmClassification, Classification, ConvergencePointType, DataClassification,
    HorizonKind, OptimizationDirection, SubstrateType,
};
use crate::crd::ProcessSpec;
use crate::export::{ExportSpec, ExportSpecSliceExt};
use crate::intent::{AplicacaoIntent, Intent};
use crate::lifetime::{EphemeralLifetime, Lifetime, TeardownPolicy};
use crate::phase::ProcessPhase;
use crate::routing::{RoutingForm, RoutingSpec};

/// `EphemeralSpec` — typed wrapper that authors `(defephemeral …)`.
///
/// Lowers to a `ProcessSpec` via `From<EphemeralSpec>` — the bridge is
/// pure-typed, no string substitution. Defaults to `point_type = Gate`,
/// `substrate = Compute`, `data_classification = Internal` — every field
/// can be overridden via the full `(defpoint …)` form when the operator
/// needs the lower-level surface.
#[derive(DeriveTataraDomain, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defephemeral")]
pub struct EphemeralSpec {
    /// The Aplicacao chart + profile + overlay to install.
    pub aplicacao: AplicacaoIntent,

    /// TTL — `humantime` duration (`"1h"`, `"30m"`).
    #[serde(default = "crate::lifetime::default_ephemeral_ttl")]
    pub ttl: String,

    /// When the ephemeral Process auto-terminates.
    #[serde(default)]
    pub teardown: TeardownPolicy,

    /// Cluster-wide concurrency budget across ephemeral Processes sharing
    /// the same `:aplicacao :chart-ref`. `0` = no cap.
    #[serde(default = "crate::lifetime::default_ephemeral_max_concurrent")]
    pub max_concurrent: u32,

    /// Boundary postconditions evaluated before reaching `Attested`.
    /// Typically `HelmReleaseReleased` plus one or more `ClosedLoopAuth`
    /// / `JobAttested` checks for test suites + closed-loop probes.
    #[serde(default)]
    pub postconditions: Vec<Condition>,

    /// Optional boundary preconditions (Namespace, Issuer, PullSecret
    /// readiness etc.).
    #[serde(default)]
    pub preconditions: Vec<Condition>,

    /// VERIFY-phase timeout. Empty = controller default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_timeout: Option<String>,

    /// Optional Process classification override. When omitted, defaults
    /// to `Gate / Compute / Internal / Bounded / NonMonotone`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<Classification>,

    /// Optional parent PID path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,

    /// Declared exports — sugar that propagates through to
    /// `lifetime.ephemeral.exports` on the lowered `ProcessSpec`.
    /// Default empty = zero-trace ephemeral (nothing survives
    /// teardown). See [`crate::export`] for the full type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<ExportSpec>,

    /// Routing template — DNS + Ingress declarations inherited by
    /// the materialized `ProcessSpec`. When set on a pool's
    /// `template`, every member receives the same shape; each
    /// member's content-hash form differs by its own canonical
    /// spec (which differs across members by slot index).
    /// See [`crate::routing`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<RoutingSpec>,
}

// `default_ttl` + `default_max_concurrent` bindings for the two serde
// `#[serde(default = "…")]` slots above route through the ONE
// substrate owner [`crate::lifetime::default_ephemeral_ttl`] +
// [`crate::lifetime::default_ephemeral_max_concurrent`] — peer of
// the [`EphemeralLifetime`] serde-default slots on the SAME
// workspace-canonical "ephemeral wire-form defaults" axis.
// Pre-lift both slots carried their own private
// `fn default_*` shims that returned bytewise-identical `"1h"` /
// `1` values as the peer [`EphemeralLifetime`] slots — one of THREE
// (TTL) and TWO (max-concurrent) restatements past the ★★ PRIME-
// DIRECTIVE ≥ 2 duplication threshold. See the substrate owner's
// doc-comment for the full migration rationale.

impl EphemeralSpec {
    /// True iff at least one [`Condition`] in
    /// `preconditions ∪ postconditions` carries the given
    /// [`ConditionKind`] — the peer of
    /// [`crate::boundary::Boundary::has_condition_kind`] on the
    /// [`EphemeralSpec`] surface.
    ///
    /// # Semantics — byte-identical to [`Boundary::has_condition_kind`]
    ///
    /// The two condition vectors are unioned: a caller asking "does this
    /// ephemeral spec name a `ClosedLoopAuth` predicate anywhere" doesn't
    /// care whether the operator authored it on the pre- or post-
    /// condition side. A spec with the given kind on ONLY preconditions
    /// returns `true`; a spec with the given kind on ONLY postconditions
    /// returns `true`; a spec with neither returns `false`.
    ///
    /// Both halves compose through the SAME slice-level substrate
    /// primitive [`ConditionSliceExt::has_kind`] that
    /// [`Boundary::has_condition_kind`] walks — so a regression at the
    /// per-slice presence probe fails at that primitive's tests rather
    /// than as silent drift at either struct-level union caller.
    ///
    /// # Sibling to [`Boundary::has_condition_kind`]
    ///
    /// Same shape, same axis, same body — [`Boundary::has_condition_kind`]
    /// composes `preconditions ∪ postconditions` on the point-domain
    /// [`ProcessSpec`]'s nested [`Boundary`] slot;
    /// [`Self::has_condition_kind`] composes the SAME union on
    /// [`EphemeralSpec`]'s direct pre/post fields. `EphemeralSpec` has no
    /// nested [`Boundary`] struct — the pre/post condition vectors are
    /// stored directly on the sugar-surface type — so a byte-identical
    /// inherent method here lets the ephemeral require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` publish a `condition-<kind>`
    /// closed-set prefix family byte-for-byte symmetrical with the point
    /// surface's family via [`Boundary::has_condition_kind`].
    ///
    /// # Compounding
    ///
    /// The ephemeral require-tag classifier composes this primitive with
    /// the closed-set `FromStr` autoderived on [`ConditionKind`] through
    /// the `strip_and_classify_prefixed_kind` substrate to publish a
    /// fifth closed-set-driven prefix family across the workspace-wide
    /// require-tag algebra (peer of `intent-<kind>` / `lifetime-<kind>` /
    /// `condition-<kind>` / `must-reach-<kind>` on the point surface). A
    /// future [`ConditionKind`] variant added to `ALL` reaches BOTH
    /// surfaces' `condition-<kind>` prefix families through the SAME
    /// closed-set walk with no per-caller edit — the two-surface
    /// symmetry means adding a variant on the closed set publishes it in
    /// lockstep across every downstream consumer.
    ///
    /// A future normalization at the presence-probe shape (a widened
    /// return carrying the matching Condition ref, a debug-build
    /// assertion on pre/post drift, a fleet-wide warn on redundant
    /// duplicates) lands at the ONE slice-level substrate primitive
    /// [`ConditionSliceExt::has_kind`] both this method and
    /// [`Boundary::has_condition_kind`] compose against — so the two
    /// struct-level union methods stay symmetric by construction.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — the union body composes the SAME slice-level substrate
    /// primitive on both this ephemeral surface and the point-domain
    /// [`Boundary`] surface). THEORY.md §VI.1 (generation over
    /// composition — a future [`ConditionKind`] variant added to `ALL`
    /// reaches both `condition-<kind>` require-tag surfaces mechanically
    /// through the SAME closed-set walk).
    #[must_use]
    pub fn has_condition_kind(&self, kind: ConditionKind) -> bool {
        self.preconditions.has_kind(kind) || self.postconditions.has_kind(kind)
    }

    /// True iff this ephemeral spec's stored [`TeardownPolicy`] equals
    /// `kind` — the substrate primitive that owns the
    /// (`&EphemeralSpec`, [`TeardownPolicy`]) → `bool` presence-probe
    /// shape on the sugar-surface type.
    ///
    /// # Peer to [`crate::lifetime::EphemeralLifetime::has_teardown_policy`]
    ///
    /// [`EphemeralLifetime::has_teardown_policy`] carries the same
    /// `(&self, TeardownPolicy) -> bool` signature on the point-surface
    /// carrier ([`ProcessSpec`]'s nested [`crate::lifetime::Lifetime`]
    /// slot reached through
    /// [`crate::lifetime::Lifetime::resolved_ephemeral`]); this peer
    /// composes byte-identical `==` semantics on
    /// [`EphemeralSpec`]'s direct `teardown: TeardownPolicy` scalar
    /// slot, so both surfaces' `teardown-policy-<kind>` require-tag
    /// families ([`crate::lifetime::EphemeralLifetime::has_teardown_policy`]
    /// on the point surface, this peer on the ephemeral surface) route
    /// through the SAME scalar `==` shape. A future normalization at
    /// the probe shape (a widened return carrying a `TerminatePolicy`
    /// disambiguator, a debug-build assertion on operator-set vs
    /// defaulted overrides, a fleet-wide warn on `Never` combined with
    /// short TTLs) lands at ONE site per surface and every downstream
    /// `teardown-policy-<kind>` require-tag family + closed-set audit
    /// dispatcher picks it up mechanically.
    ///
    /// # Semantics — VARIANT match, not POPULATED slot
    ///
    /// [`EphemeralSpec::teardown`] is a required, defaulted scalar
    /// ([`TeardownPolicy::Always`] via `#[default]`); there is no
    /// absent state to detect. `has_teardown_policy(kind)` returns
    /// `true` iff `self.teardown == kind`. On a hand-authored
    /// [`EphemeralSpec`] that omits `:teardown` from the
    /// `(defephemeral …)` form (or a Rust builder that reaches
    /// [`TeardownPolicy::default`]) the probe returns `true` for
    /// [`TeardownPolicy::Always`] and `false` for every other variant
    /// — distinct from the Option-slot axis where a default carrier
    /// returns `false` for EVERY kind. An operator who left
    /// `:teardown` at the substrate default IS configured for
    /// `Always`, and a `:requires (teardown-policy-Always)` check
    /// should pass; only an operator who deliberately overrode the
    /// policy to `OnAttested` / `OnFailed` / `Never` fails the tag on
    /// this axis.
    ///
    /// # Corner — (required-scalar-child)
    ///
    /// Fresh corner on the ephemeral surface's presence-probe algebra:
    /// [`EphemeralSpec`] has no Option-parent hop between the sugar
    /// struct and the `teardown` scalar (the point surface reaches
    /// [`crate::lifetime::EphemeralLifetime::teardown_policy`]
    /// through the Option-parent `resolved_ephemeral()` gate), so the
    /// probe body is a bare scalar `==` on a required field. Distinct
    /// from [`Self::has_condition_kind`] on this same surface, which
    /// walks a `Vec<Condition>` slice-child.
    ///
    /// # Compounding
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// with the closed-set `FromStr` autoderived on [`TeardownPolicy`]
    /// through the `strip_and_classify_prefixed_kind` substrate to
    /// publish a `teardown-policy-<kind>` prefix family byte-for-byte
    /// symmetrical with the point surface's family via
    /// [`crate::lifetime::EphemeralLifetime::has_teardown_policy`]. A
    /// future fifth [`TeardownPolicy`] variant added to `ALL` (a
    /// hypothetical `OnTimeout` for "tear down only on TTL expiry")
    /// reaches BOTH surfaces' `teardown-policy-<kind>` prefix families
    /// through the SAME closed-set walk with no per-caller edit — the
    /// two-surface symmetry means adding a variant on the closed set
    /// publishes it in lockstep across every downstream consumer.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the scalar-carrier presence-probe body lives
    /// at ONE substrate site per surface so every downstream
    /// (`teardown-policy-<kind>` require-tag families on both surfaces
    /// in tatara-check, closed-set audit dispatchers, future variant
    /// additions on [`TeardownPolicy`]) binds through the SAME
    /// `has(kind)` shape rather than restating the `<eph>.teardown ==
    /// kind` closure body at each call site). THEORY.md §VI.1
    /// (generation over composition — a future variant lands at ONE
    /// `ALL` entry + one `as_str` arm on the closed set and the probe
    /// picks it up mechanically without further per-consumer edits).
    #[must_use]
    pub fn has_teardown_policy(&self, kind: TeardownPolicy) -> bool {
        self.teardown == kind
    }

    /// Derived-bool-predicate presence probe on the stored
    /// [`Self::teardown`] slot — `true` iff this ephemeral sugar's
    /// [`TeardownPolicy`] would auto-SIGTERM the Process on the
    /// queried [`ProcessPhase`] transition (as read through
    /// [`TeardownPolicy::should_teardown_on`]).
    ///
    /// # Sibling to [`crate::lifetime::EphemeralLifetime::has_teardown_firing_on`]
    ///
    /// Same shape, same axis, one refinement lower: the point-surface
    /// peer on [`crate::lifetime::EphemeralLifetime`] composes the SAME
    /// [`TeardownPolicy::should_teardown_on`] predicate against the
    /// SAME stored `teardown_policy` slot; this method composes the
    /// same predicate against the sugar surface's flattened
    /// [`Self::teardown`] slot. Both bodies delegate to the ONE
    /// substrate owner [`TeardownPolicy::should_teardown_on`], so a
    /// regression at the (policy, phase) → bool truth table surfaces
    /// at THAT primitive's tests rather than as silent drift at
    /// either struct-level caller.
    ///
    /// # Corner — (required-scalar-parent × derived-bool-predicate-child)
    ///
    /// [`EphemeralSpec::teardown`] is a required, defaulted scalar
    /// ([`TeardownPolicy::Always`] via `#[default]`); there is no
    /// Option-parent hop between the sugar struct and the `teardown`
    /// scalar (the point surface reaches
    /// [`crate::lifetime::EphemeralLifetime::teardown_policy`]
    /// through the Option-parent `resolved_ephemeral()` gate). The
    /// probe body is a bare predicate application on a required
    /// field. Distinct from [`Self::has_teardown_policy`] on this
    /// same surface, which reads the raw stored variant for equality
    /// (`self.teardown == kind`) rather than the derived firing-arm
    /// predicate against a [`ProcessPhase`] argument.
    ///
    /// # Compounding
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// with the closed-set [`crate::phase::ProcessPhase`]'s
    /// autoderived `FromStr` through the
    /// `strip_and_classify_prefixed_kind` substrate to publish a
    /// `teardown-fires-on-<phase>` prefix family byte-for-byte
    /// symmetrical with the point surface's family via
    /// [`crate::lifetime::EphemeralLifetime::has_teardown_firing_on`].
    /// A future fifth [`TeardownPolicy`] variant added to `ALL` (a
    /// hypothetical `OnTimeout` for "tear down only on TTL expiry")
    /// reaches BOTH surfaces' `teardown-fires-on-<phase>` prefix
    /// families through the SAME
    /// [`TeardownPolicy::should_teardown_on`] match with no per-
    /// caller edit — the two-surface symmetry means adding a variant
    /// on the closed set publishes it in lockstep across every
    /// downstream consumer.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the derived-bool-predicate presence-probe
    /// body lives at ONE substrate site per surface, both composing
    /// the SAME [`TeardownPolicy::should_teardown_on`] projection, so
    /// every downstream (`teardown-fires-on-<phase>` require-tag
    /// families on both surfaces in tatara-check, closed-set audit
    /// dispatchers, future variant additions on either
    /// [`TeardownPolicy`] or [`crate::phase::ProcessPhase`]) binds
    /// through the SAME `has_teardown_firing_on(phase)` shape rather
    /// than restating the `<eph>.teardown.should_teardown_on(phase)`
    /// closure body at each call site). THEORY.md §VI.1 (generation
    /// over composition — a future variant lands at ONE `ALL` entry +
    /// one `as_str` arm + one `should_teardown_on` arm on the closed
    /// set and the probe picks it up mechanically without further
    /// per-consumer edits).
    #[must_use]
    pub const fn has_teardown_firing_on(&self, phase: ProcessPhase) -> bool {
        self.teardown.should_teardown_on(phase)
    }

    /// Resolve the operator-authored [`Self::classification`] slot to
    /// the concrete [`Classification`] the point surface sees, filling
    /// `None` through the same [`default_ephemeral_class`] baseline the
    /// `From<EphemeralSpec> for ProcessSpec` lowering uses when the
    /// operator omits `:classification` from the `(defephemeral …)`
    /// form. Returns [`Cow::Borrowed`] on the populated arm (zero
    /// allocation), else [`Cow::Owned`] with the workspace-baseline
    /// `(Gate, Compute, Bounded, Monotone, Internal)` value the sibling
    /// primitive [`Classification::gate_compute`] owns.
    ///
    /// # ONE substrate primitive for `Option<Classification>` resolution
    ///
    /// This is the ONE `EphemeralSpec`-inherent primitive that owns the
    /// `Option<Classification>` → resolved-[`Classification`] walk.
    /// Every downstream classification-axis presence probe on the
    /// [`EphemeralSpec`] surface ([`Self::has_point_type`],
    /// [`Self::has_substrate`], [`Self::has_calm`],
    /// [`Self::has_data_classification`], [`Self::has_horizon_kind`],
    /// [`Self::has_optimization_direction`], [`Self::has_input_arity`],
    /// [`Self::has_output_arity`]) routes through THIS
    /// primitive so the "`None` fills through
    /// [`default_ephemeral_class`]" resolution lives at ONE site rather
    /// than being restated in each per-axis probe body. A future
    /// regression on the fill-through (a shift from the `(Gate,
    /// Compute, …)` baseline to a different `default_ephemeral_class`
    /// body, a shift from the `Option`-carrier shape to a
    /// serde-defaulted required-field carrier, an eventual audit hook
    /// naming the resolved-vs-authored provenance) lands at ONE site
    /// and every downstream axis-probe on the ephemeral surface picks
    /// it up mechanically.
    ///
    /// # Sibling to the `From<EphemeralSpec>` lowering
    ///
    /// The lowering `From<EphemeralSpec> for ProcessSpec` fills
    /// [`ProcessSpec::classification`] through the SAME
    /// `.unwrap_or_else(default_ephemeral_class)` walk that this
    /// primitive owns on the borrow-friendly `Cow` return. Both sites
    /// resolve the same operator-authored slot through the same default
    /// so a future two-surface parity contract on the classification
    /// axes (`point-type-<kind>` on both surfaces, `substrate-<kind>`
    /// on both surfaces, …) reads identically through the sibling
    /// point-surface probe [`Classification::has_<axis>`] on the
    /// lowered `ProcessSpec` and through THIS primitive on the same
    /// authored [`EphemeralSpec`].
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the `Option<Classification>` resolution body
    /// lives at ONE substrate primitive on the ephemeral surface so
    /// every downstream classification-axis probe binds through the
    /// SAME `resolved_classification()` shape rather than restating
    /// the `self.classification.as_ref().unwrap_or(&default_…)`
    /// closure body at each callsite. THEORY.md §VI.1 — generation
    /// over composition; a future classification-axis peer
    /// (`has_substrate`, `has_calm`, …) lands as ONE inherent method
    /// that delegates through the resolver's `has_<axis>(kind)` call
    /// on the sibling [`Classification`] closed-set primitive with no
    /// per-axis restatement of the fill-through logic.
    #[must_use]
    pub fn resolved_classification(&self) -> Cow<'_, Classification> {
        match &self.classification {
            Some(c) => Cow::Borrowed(c),
            None => Cow::Owned(default_ephemeral_class()),
        }
    }

    /// True iff the resolved [`Classification`] carries the given
    /// [`ConvergencePointType`] on its `point_type` slot — byte-for-
    /// byte peer of [`Classification::has_point_type`] wrapped through
    /// the [`Self::resolved_classification`] resolver so an
    /// operator-omitted `:classification` slot reads as the
    /// [`default_ephemeral_class`] baseline the sibling
    /// `From<EphemeralSpec> for ProcessSpec` lowering fills.
    ///
    /// # Two-surface parity contract
    ///
    /// A given [`EphemeralSpec`] classifies identically through this
    /// primitive AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.has_point_type(kind)`
    /// on the lowered `ProcessSpec` — the `Cow<'_, Classification>`
    /// resolver on this side and the `.unwrap_or_else(...)` fill on
    /// the lowering side both dereference the same
    /// `default_ephemeral_class()` value on `None` and the same
    /// authored value on `Some(_)`. This means the ephemeral-surface
    /// `point-type-<kind>` `:requires` family in
    /// `tatara-reconciler::bin::tatara-check` publishes the SAME
    /// truth on the SAME authored spec as the point-surface family
    /// on the mechanically-lowered `ProcessSpec`.
    ///
    /// # Sibling to the seven other classification axes
    ///
    /// FIRST classification-axis peer on the [`EphemeralSpec`]
    /// surface. Six future sibling axes on the SAME `Cow`-resolver
    /// carrier ([`Self::has_substrate`] opened the SECOND,
    /// [`Self::has_calm`] the THIRD,
    /// [`Self::has_data_classification`] the FOURTH,
    /// [`Self::has_horizon_kind`] the FIFTH,
    /// [`Self::has_optimization_direction`] the SIXTH; then
    /// `has_input_arity`, `has_output_arity`) land as one-line
    /// wrappers around the SAME resolver + the sibling
    /// [`Classification`] closed-set primitive, so a future variant
    /// added to [`ConvergencePointType`] (or any of the seven other
    /// closed sets) reaches BOTH surfaces' `<axis>-<kind>` prefix
    /// families through the SAME closed-set walk with no per-caller
    /// edit.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-axis presence-probe body
    /// composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE closed-set
    /// primitive ([`Classification::has_point_type`]) so every
    /// downstream (`point-type-<kind>` require-tag families on both
    /// surfaces in tatara-check, closed-set audit dispatchers, future
    /// variant additions on [`ConvergencePointType`]) binds through
    /// the SAME `has(kind)` shape rather than restating either the
    /// resolver walk or the closed-set equality at the callsite.
    #[must_use]
    pub fn has_point_type(&self, kind: ConvergencePointType) -> bool {
        self.resolved_classification().has_point_type(kind)
    }

    /// True iff the resolved [`Classification`] carries the given
    /// [`SubstrateType`] on its `substrate` slot — byte-for-byte peer
    /// of [`Classification::has_substrate`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot reads as the
    /// [`default_ephemeral_class`] baseline the sibling
    /// `From<EphemeralSpec> for ProcessSpec` lowering fills.
    ///
    /// # Two-surface parity contract
    ///
    /// A given [`EphemeralSpec`] classifies identically through this
    /// primitive AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.has_substrate(kind)`
    /// on the lowered `ProcessSpec` — the `Cow<'_, Classification>`
    /// resolver on this side and the `.unwrap_or_else(...)` fill on
    /// the lowering side both dereference the same
    /// `default_ephemeral_class()` value on `None` and the same
    /// authored value on `Some(_)`. This means the ephemeral-surface
    /// `substrate-<kind>` `:requires` family in
    /// `tatara-reconciler::bin::tatara-check` publishes the SAME
    /// truth on the SAME authored spec as the point-surface family
    /// on the mechanically-lowered `ProcessSpec`.
    ///
    /// # SECOND classification-axis peer on the ephemeral surface
    ///
    /// Peer of [`Self::has_point_type`] — both route through the SAME
    /// [`Self::resolved_classification`] resolver, so the operator-
    /// omitted `:classification` slot's fill-through logic lives at
    /// ONE substrate primitive rather than being restated in each
    /// per-axis probe body. Five future sibling axes on the SAME
    /// `Cow`-resolver carrier ([`Self::has_calm`] opened the THIRD,
    /// [`Self::has_data_classification`] the FOURTH,
    /// [`Self::has_horizon_kind`] the FIFTH,
    /// [`Self::has_optimization_direction`] the SIXTH; then
    /// `has_input_arity`, `has_output_arity`) land as one-line
    /// wrappers around the SAME resolver + the sibling
    /// [`Classification`] closed-set primitive, so a future variant
    /// added to [`SubstrateType`] (or any of the six other closed
    /// sets) reaches BOTH surfaces' `<axis>-<kind>` prefix families
    /// through the SAME closed-set walk with no per-caller edit.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-axis presence-probe body
    /// composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE closed-set
    /// primitive ([`Classification::has_substrate`]) so every
    /// downstream (`substrate-<kind>` require-tag families on both
    /// surfaces in tatara-check, closed-set audit dispatchers, future
    /// variant additions on [`SubstrateType`]) binds through the
    /// SAME `has(kind)` shape rather than restating either the
    /// resolver walk or the closed-set equality at the callsite.
    #[must_use]
    pub fn has_substrate(&self, kind: SubstrateType) -> bool {
        self.resolved_classification().has_substrate(kind)
    }

    /// True iff the resolved [`Classification`] carries the given
    /// [`CalmClassification`] on its `calm` slot — byte-for-byte peer
    /// of [`Classification::has_calm`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot reads as the
    /// [`default_ephemeral_class`] baseline the sibling
    /// `From<EphemeralSpec> for ProcessSpec` lowering fills.
    ///
    /// # Two-surface parity contract
    ///
    /// A given [`EphemeralSpec`] classifies identically through this
    /// primitive AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.has_calm(kind)`
    /// on the lowered `ProcessSpec` — the `Cow<'_, Classification>`
    /// resolver on this side and the `.unwrap_or_else(...)` fill on
    /// the lowering side both dereference the same
    /// `default_ephemeral_class()` value on `None` and the same
    /// authored value on `Some(_)`. This means the ephemeral-surface
    /// `calm-<kind>` `:requires` family in
    /// `tatara-reconciler::bin::tatara-check` publishes the SAME
    /// truth on the SAME authored spec as the point-surface family
    /// on the mechanically-lowered `ProcessSpec`.
    ///
    /// # THIRD classification-axis peer on the ephemeral surface
    ///
    /// Peer of [`Self::has_point_type`] and [`Self::has_substrate`] —
    /// all three route through the SAME
    /// [`Self::resolved_classification`] resolver, so the operator-
    /// omitted `:classification` slot's fill-through logic lives at
    /// ONE substrate primitive rather than being restated in each
    /// per-axis probe body. FIRST occupant on the (Option-parent ×
    /// DEFAULTED-scalar-child × operator-resolvable-baseline) corner
    /// of the ephemeral-surface presence-probe algebra — distinct
    /// from the (Option-parent × NON-DEFAULT-scalar-child) corner
    /// the first two classification-axis peers opened, since
    /// [`CalmClassification`] carries `#[default] = Monotone` on the
    /// closed set. The default-arm short-circuit on the absent-
    /// classification arm reads `true` on the [`CalmClassification`]
    /// child's `#[default]` variant precisely because BOTH the parent
    /// Option's fill-through baseline (`default_ephemeral_class`) AND
    /// the child's own `#[default]` land on the SAME variant
    /// ([`CalmClassification::Monotone`]) — a two-defaults
    /// composition property distinct from the NON-DEFAULT-scalar
    /// peers, whose absent-classification arm defaults through a
    /// specific chosen baseline (`ConvergencePointType::Gate`,
    /// `SubstrateType::Compute`) rather than through the child's own
    /// `#[default]`. Four future sibling axes on the SAME
    /// `Cow`-resolver carrier ([`Self::has_data_classification`]
    /// opened the FOURTH, [`Self::has_horizon_kind`] the FIFTH,
    /// [`Self::has_optimization_direction`] the SIXTH; then
    /// `has_input_arity`, `has_output_arity`) land as one-line
    /// wrappers around the SAME resolver + the sibling
    /// [`Classification`] closed-set primitive, so a future variant
    /// added to [`CalmClassification`] (or any of the five other
    /// closed sets) reaches BOTH surfaces' `<axis>-<kind>` prefix
    /// families through the SAME closed-set walk with no per-caller
    /// edit.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-axis presence-probe body
    /// composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE closed-set
    /// primitive ([`Classification::has_calm`]) so every downstream
    /// (`calm-<kind>` require-tag families on both surfaces in
    /// tatara-check, closed-set audit dispatchers, future variant
    /// additions on [`CalmClassification`]) binds through the SAME
    /// `has(kind)` shape rather than restating either the resolver
    /// walk or the closed-set equality at the callsite.
    #[must_use]
    pub fn has_calm(&self, kind: CalmClassification) -> bool {
        self.resolved_classification().has_calm(kind)
    }

    /// True iff the resolved [`Classification`] carries the given
    /// [`DataClassification`] on its `data_classification` slot —
    /// byte-for-byte peer of [`Classification::has_data_classification`]
    /// wrapped through the [`Self::resolved_classification`] resolver
    /// so an operator-omitted `:classification` slot reads as the
    /// [`default_ephemeral_class`] baseline the sibling
    /// `From<EphemeralSpec> for ProcessSpec` lowering fills.
    ///
    /// # Two-surface parity contract
    ///
    /// A given [`EphemeralSpec`] classifies identically through this
    /// primitive AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.has_data_classification(kind)`
    /// on the lowered `ProcessSpec` — the `Cow<'_, Classification>`
    /// resolver on this side and the `.unwrap_or_else(...)` fill on
    /// the lowering side both dereference the same
    /// `default_ephemeral_class()` value on `None` and the same
    /// authored value on `Some(_)`. This means the ephemeral-surface
    /// `data-classification-<kind>` `:requires` family in
    /// `tatara-reconciler::bin::tatara-check` publishes the SAME
    /// truth on the SAME authored spec as the point-surface family
    /// on the mechanically-lowered `ProcessSpec`.
    ///
    /// # FOURTH classification-axis peer on the ephemeral surface
    ///
    /// Peer of [`Self::has_point_type`], [`Self::has_substrate`], and
    /// [`Self::has_calm`] — all four route through the SAME
    /// [`Self::resolved_classification`] resolver, so the operator-
    /// omitted `:classification` slot's fill-through logic lives at
    /// ONE substrate primitive rather than being restated in each
    /// per-axis probe body. SECOND occupant on the (Option-parent ×
    /// DEFAULTED-scalar-child × operator-resolvable-baseline) corner
    /// of the ephemeral-surface presence-probe algebra alongside
    /// [`Self::has_calm`] — both probe REQUIRED [`Classification`]
    /// sub-slots whose child closed set carries its own `#[default]`
    /// ([`DataClassification::Internal`] here,
    /// [`CalmClassification::Monotone`] on the peer), so the
    /// default-arm short-circuit on the absent-classification arm
    /// reads `true` on the [`DataClassification`] child's
    /// `#[default]` variant precisely because BOTH the parent
    /// Option's fill-through baseline (`default_ephemeral_class`)
    /// AND the child's own `#[default]` land on the SAME variant
    /// ([`DataClassification::Internal`]). The two-defaults
    /// composition property now walks TWO independent defaulted-
    /// scalar-child slots on the SAME ephemeral resolver — a
    /// regression that promoted a different [`DataClassification`]
    /// variant to `#[default]` (or wired the arm to a fixed variant
    /// answer) fails HERE at ONE narrow substrate site before
    /// drifting through every unadorned ephemeral spec's baseline
    /// data-classification answer. Distinct from the FIRST + SECOND
    /// peers on the (Option-parent × NON-DEFAULT-scalar-child)
    /// corner, whose absent-classification arm defaults through a
    /// specific chosen baseline (`ConvergencePointType::Gate`,
    /// `SubstrateType::Compute`) rather than through the child's own
    /// `#[default]`. Four future sibling axes on the SAME
    /// `Cow`-resolver carrier ([`Self::has_horizon_kind`] opened the
    /// FIFTH, [`Self::has_optimization_direction`] the SIXTH; then
    /// `has_input_arity`, `has_output_arity`) land as one-line
    /// wrappers around the SAME resolver + the sibling
    /// [`Classification`] closed-set primitive, so a future variant
    /// added to [`DataClassification`] (or any of the four other
    /// closed sets) reaches BOTH surfaces' `<axis>-<kind>` prefix
    /// families through the SAME closed-set walk with no per-caller
    /// edit.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-axis presence-probe body
    /// composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE closed-set
    /// primitive ([`Classification::has_data_classification`]) so
    /// every downstream (`data-classification-<kind>` require-tag
    /// families on both surfaces in tatara-check, closed-set audit
    /// dispatchers, future variant additions on
    /// [`DataClassification`]) binds through the SAME `has(kind)`
    /// shape rather than restating either the resolver walk or the
    /// closed-set equality at the callsite.
    #[must_use]
    pub fn has_data_classification(&self, kind: DataClassification) -> bool {
        self.resolved_classification().has_data_classification(kind)
    }

    /// True iff the resolved [`Classification`]'s nested [`Horizon`]
    /// carries the given [`HorizonKind`] discriminator on its
    /// `horizon.kind` slot — byte-for-byte peer of
    /// [`Classification::has_horizon_kind`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot reads as the
    /// [`default_ephemeral_class`] baseline the sibling
    /// `From<EphemeralSpec> for ProcessSpec` lowering fills.
    ///
    /// # Two-surface parity contract
    ///
    /// A given [`EphemeralSpec`] classifies identically through this
    /// primitive AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.has_horizon_kind(kind)`
    /// on the lowered `ProcessSpec` — the `Cow<'_, Classification>`
    /// resolver on this side and the `.unwrap_or_else(...)` fill on
    /// the lowering side both dereference the same
    /// `default_ephemeral_class()` value on `None` and the same
    /// authored value on `Some(_)`. This means the ephemeral-surface
    /// `horizon-<kind>` `:requires` family in
    /// `tatara-reconciler::bin::tatara-check` publishes the SAME
    /// truth on the SAME authored spec as the point-surface family
    /// on the mechanically-lowered `ProcessSpec`.
    ///
    /// # FIFTH classification-axis peer on the ephemeral surface
    ///
    /// Peer of [`Self::has_point_type`], [`Self::has_substrate`],
    /// [`Self::has_calm`], and [`Self::has_data_classification`] — all
    /// five route through the SAME [`Self::resolved_classification`]
    /// resolver, so the operator-omitted `:classification` slot's
    /// fill-through logic lives at ONE substrate primitive rather
    /// than being restated in each per-axis probe body. OPENS a fresh
    /// (Option-parent × NESTED-STRUCT-scalar-child ×
    /// operator-resolvable-baseline) corner on the ephemeral-surface
    /// presence-probe algebra — the four prior peers on this surface
    /// all read the closed-set discriminator DIRECTLY off a scalar
    /// [`Classification`] slot (`point_type`, `substrate`, `calm`,
    /// `data_classification`); this probe instead threads through a
    /// NESTED-STRUCT intermediary ([`Horizon`], the defaulted nested
    /// struct owning the `horizon` axis) to reach a scalar
    /// [`HorizonKind`] discriminator on `horizon.kind`. The
    /// default-arm short-circuit on the absent-classification arm
    /// reads `true` on the [`HorizonKind`] child's `#[default]`
    /// variant precisely because BOTH the parent Option's fill-
    /// through baseline ([`default_ephemeral_class`], which fills
    /// `horizon: Horizon::default()`) AND the child's own `#[default]`
    /// land on the SAME variant ([`HorizonKind::Bounded`]). A
    /// regression that dropped `#[default]` on [`HorizonKind`], or
    /// promoted `Asymptotic` to `#[default]`, or wired the arm to a
    /// fixed variant answer, or crossed the wires through the wrong
    /// nested struct fails HERE at ONE narrow substrate site before
    /// drifting through every unadorned ephemeral spec's baseline
    /// horizon answer. Distinct from the FIRST + SECOND peers on the
    /// (Option-parent × NON-DEFAULT-scalar-child) corner
    /// (`has_point_type`, `has_substrate`) whose absent-classification
    /// arm defaults through a specific chosen baseline
    /// (`ConvergencePointType::Gate`, `SubstrateType::Compute`), AND
    /// distinct from the THIRD + FOURTH peers on the (Option-parent ×
    /// DEFAULTED-scalar-child) corner (`has_calm`,
    /// `has_data_classification`) which reach a defaulted scalar
    /// DIRECTLY off the parent without a nested-struct hop. Three
    /// future sibling axes on the SAME `Cow`-resolver carrier
    /// ([`Self::has_optimization_direction`] opened the SIXTH; then
    /// `has_input_arity`, `has_output_arity`) land as one-line
    /// wrappers around the SAME resolver + the sibling
    /// [`Classification`] closed-set primitive, so a future variant
    /// added to [`HorizonKind`] (or any of the three other closed
    /// sets) reaches BOTH surfaces' `<axis>-<kind>` prefix families
    /// through the SAME closed-set walk with no per-caller edit.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-axis presence-probe body
    /// composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE closed-set
    /// primitive ([`Classification::has_horizon_kind`]) so every
    /// downstream (`horizon-<kind>` require-tag families on both
    /// surfaces in tatara-check, closed-set audit dispatchers, future
    /// variant additions on [`HorizonKind`]) binds through the SAME
    /// `has(kind)` shape rather than restating either the resolver
    /// walk or the closed-set equality at the callsite.
    #[must_use]
    pub fn has_horizon_kind(&self, kind: HorizonKind) -> bool {
        self.resolved_classification().has_horizon_kind(kind)
    }

    /// True iff the resolved [`Classification`]'s nested [`Horizon`]
    /// carries the given [`OptimizationDirection`] discriminator on its
    /// `horizon.direction` slot (with the substrate
    /// `Option::unwrap_or_default` treating `None` as the closed set's
    /// `#[default] Minimize`) — byte-for-byte peer of
    /// [`Classification::has_optimization_direction`] wrapped through
    /// the [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot reads as the
    /// [`default_ephemeral_class`] baseline the sibling
    /// `From<EphemeralSpec> for ProcessSpec` lowering fills.
    ///
    /// # Two-surface parity contract
    ///
    /// A given [`EphemeralSpec`] classifies identically through this
    /// primitive AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.has_optimization_direction(kind)`
    /// on the lowered `ProcessSpec` — the `Cow<'_, Classification>`
    /// resolver on this side and the `.unwrap_or_else(...)` fill on
    /// the lowering side both dereference the same
    /// `default_ephemeral_class()` value on `None` and the same
    /// authored value on `Some(_)`, and the sibling
    /// [`Classification::has_optimization_direction`] applies the same
    /// `Option::unwrap_or_default` collapse on the inner
    /// `horizon.direction` slot on both sides. This means the
    /// ephemeral-surface `optimization-direction-<kind>` `:requires`
    /// family in `tatara-reconciler::bin::tatara-check` publishes the
    /// SAME truth on the SAME authored spec as the point-surface
    /// family on the mechanically-lowered `ProcessSpec`.
    ///
    /// # SIXTH classification-axis peer on the ephemeral surface
    ///
    /// Peer of [`Self::has_point_type`], [`Self::has_substrate`],
    /// [`Self::has_calm`], [`Self::has_data_classification`], and
    /// [`Self::has_horizon_kind`] — all six route through the SAME
    /// [`Self::resolved_classification`] resolver, so the operator-
    /// omitted `:classification` slot's fill-through logic lives at
    /// ONE substrate primitive rather than being restated in each per-
    /// axis probe body. SECOND occupant on the (Option-parent ×
    /// NESTED-STRUCT-scalar-child × operator-resolvable-baseline)
    /// corner alongside [`Self::has_horizon_kind`] — both probes thread
    /// through the SAME nested [`Horizon`] intermediary to reach a
    /// scalar discriminator on the six-axis classification lattice, but
    /// this method additionally traverses an `Option`-slot with
    /// `unwrap_or_default` so a Process filled through
    /// [`crate::classification::Horizon::default`] (leaves `direction:
    /// None`) still reads `true` on the closed set's default arm
    /// ([`OptimizationDirection::Minimize`]). The corner therefore
    /// admits BOTH direct nested-scalar shapes ([`Self::has_horizon_kind`]
    /// walks `horizon.kind: HorizonKind` directly) AND Option-nested-
    /// scalar shapes (this method walks `horizon.direction:
    /// Option<OptimizationDirection>` through `unwrap_or_default`),
    /// pinning the corner as a proven-repeatable primitive shape on the
    /// ephemeral surface rather than a single-example curiosity. The
    /// two-defaults composition property (parent Option's fill-through
    /// baseline via `default_ephemeral_class` AND child's closed-set
    /// `#[default]` land on the SAME variant) reaches through TWO
    /// hops here: the parent Option's `.unwrap_or_else(default_…)`
    /// AND the inner Option's `.unwrap_or_default()` both dereference
    /// to the same [`OptimizationDirection::Minimize`] baseline the
    /// closed set publishes. A regression that flipped
    /// [`OptimizationDirection`]'s `#[default]` off `Minimize` (which
    /// would silently invert every unadorned `Asymptotic` Process's
    /// rate-window evaluator polarity), or that dropped the resolver
    /// hop, or that wired the arm to a fixed variant answer, fails
    /// HERE at ONE narrow substrate site before drifting through every
    /// unadorned ephemeral spec's baseline direction answer. Two future
    /// sibling axes on the SAME `Cow`-resolver carrier
    /// (`has_input_arity`, `has_output_arity`) land as one-line
    /// wrappers around the SAME resolver + the sibling
    /// [`Classification`] closed-set primitive, so a future variant
    /// added to [`OptimizationDirection`] (or any of the two other
    /// closed sets) reaches BOTH surfaces' `<axis>-<kind>` prefix
    /// families through the SAME closed-set walk with no per-caller
    /// edit.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-axis presence-probe body
    /// composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE closed-set
    /// primitive ([`Classification::has_optimization_direction`]) so
    /// every downstream (`optimization-direction-<kind>` require-tag
    /// families on both surfaces in tatara-check, closed-set audit
    /// dispatchers, future variant additions on
    /// [`OptimizationDirection`]) binds through the SAME `has(kind)`
    /// shape rather than restating either the resolver walk or the
    /// closed-set equality plus the nested-struct-Option-hop at the
    /// callsite.
    #[must_use]
    pub fn has_optimization_direction(&self, kind: OptimizationDirection) -> bool {
        self.resolved_classification()
            .has_optimization_direction(kind)
    }

    /// True iff the resolved [`Classification`]'s nested
    /// [`ConvergencePointType`] projects (via the many-to-one
    /// [`ConvergencePointType::input_arity`] typed projection) to the
    /// given [`Arity`] discriminator — byte-for-byte peer of
    /// [`Classification::has_input_arity`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-omitted
    /// `:classification` slot reads as the [`default_ephemeral_class`]
    /// baseline the sibling `From<EphemeralSpec> for ProcessSpec`
    /// lowering fills.
    ///
    /// # Two-surface parity contract
    ///
    /// A given [`EphemeralSpec`] classifies identically through this
    /// primitive AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.has_input_arity(kind)`
    /// on the lowered `ProcessSpec` — the `Cow<'_, Classification>`
    /// resolver on this side and the `.unwrap_or_else(...)` fill on the
    /// lowering side both dereference the same
    /// `default_ephemeral_class()` value on `None` and the same
    /// authored value on `Some(_)`, and the sibling
    /// [`Classification::has_input_arity`] applies the same
    /// `point_type.input_arity()` typed projection on both sides. This
    /// means the ephemeral-surface `input-arity-<kind>` `:requires`
    /// family in `tatara-reconciler::bin::tatara-check` publishes the
    /// SAME truth on the SAME authored spec as the point-surface family
    /// on the mechanically-lowered `ProcessSpec`.
    ///
    /// # SEVENTH classification-axis peer on the ephemeral surface — first via a derived-typed-projection
    ///
    /// Peer of [`Self::has_point_type`], [`Self::has_substrate`],
    /// [`Self::has_calm`], [`Self::has_data_classification`],
    /// [`Self::has_horizon_kind`], and
    /// [`Self::has_optimization_direction`] — all seven route through
    /// the SAME [`Self::resolved_classification`] resolver, so the
    /// operator-omitted `:classification` slot's fill-through logic
    /// lives at ONE substrate primitive rather than being restated in
    /// each per-axis probe body. FIRST occupant on the (Option-parent ×
    /// NESTED-STRUCT-scalar-child × derived-typed-projection) corner on
    /// the ephemeral surface — byte-for-byte symmetric with the
    /// derived-typed-projection precedent set by
    /// [`Classification::has_input_arity`] on the point surface: THAT
    /// peer routes through [`ConvergencePointType::input_arity`] on a
    /// required [`Classification`] carrier; THIS peer routes through the
    /// SAME projection on the `Cow`-resolver carrier so the resolver
    /// walk composes with the projection at ONE substrate site rather
    /// than being restated per surface. Distinct from the SIXTH peer
    /// [`Self::has_optimization_direction`] (which walks
    /// `horizon.direction` through an `Option::unwrap_or_default`
    /// collapse to reach a defaulted scalar child) and the FIFTH peer
    /// [`Self::has_horizon_kind`] (which walks `horizon.kind` DIRECTLY
    /// as a scalar without any typed-projection hop) on ONE dimension:
    /// this probe threads through the many-to-one closed-set typed
    /// projection [`ConvergencePointType::input_arity`] (`Transform |
    /// Fork | Broadcast | Observe → One`, `Join | Gate | Select |
    /// Reduce → Many`) so the child's closed set ([`Arity`]) is REACHED
    /// THROUGH a projection layer, not read raw off a scalar. The
    /// corner therefore admits three ephemeral-surface traversal
    /// shapes through the SAME `resolved_classification().<field>`
    /// walk: direct-nested-scalar
    /// ([`Self::has_horizon_kind`] reads `horizon.kind: HorizonKind`
    /// directly), Option-nested-scalar
    /// ([`Self::has_optimization_direction`] reads `horizon.direction:
    /// Option<OptimizationDirection>` through `unwrap_or_default`), and
    /// derived-typed-projection (this method reads
    /// `point_type.input_arity(): Arity` through a many-to-one
    /// projection). The co-tenant derived-typed-projection axis on the
    /// SAME `Cow`-resolver carrier ([`Self::has_output_arity`]) lands as
    /// a one-line wrapper around the SAME resolver + the sibling
    /// [`Classification`] closed-set primitive, so a future variant
    /// added to [`Arity`] or to [`ConvergencePointType`] reaches BOTH
    /// surfaces' `<axis>-<kind>` prefix families through the SAME
    /// closed-set walk with no per-caller edit.
    ///
    /// # Semantics — VARIANT match on the projected image
    ///
    /// [`Arity`] carries no `Default` impl (the 2-arm bare enum with no
    /// `#[default]`), so exactly ONE of the two arms answers `true` per
    /// well-formed [`EphemeralSpec`], with no default-arm short-circuit
    /// shortcut. The absent-`:classification` baseline
    /// [`default_ephemeral_class`] fills `point_type: Gate`, and
    /// [`ConvergencePointType::input_arity`] projects `Gate → Many`, so
    /// the ephemeral sugar surface's `input-arity-Many` require-tag
    /// reads `true` on every operator-authored spec that omits the
    /// `:classification` slot — pinning the workspace's convergent-by-
    /// default point posture on the input side. The many-to-one
    /// projection shape means the answer is invariant under intra-
    /// bucket point-type swaps (`Transform ↔ Fork ↔ Broadcast ↔
    /// Observe` all keep `input-arity-One = true`) and flips at bucket
    /// boundaries (`Transform ↔ Join` flips `input-arity-One` from
    /// `true` to `false`). A regression that dropped the resolver hop,
    /// probed [`ConvergencePointType`] directly (dropping the
    /// `.input_arity()` call), inverted the projection (`One ↔ Many`),
    /// or crossed the wires with the sibling
    /// [`ConvergencePointType::output_arity`] projection fails HERE at
    /// ONE narrow substrate site before drifting through every
    /// unadorned ephemeral spec's baseline input-arity answer.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-axis presence-probe body
    /// composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE closed-set primitive
    /// ([`Classification::has_input_arity`]) so every downstream
    /// (`input-arity-<kind>` require-tag families on both surfaces in
    /// tatara-check, closed-set audit dispatchers, future variant
    /// additions on [`Arity`] or on [`ConvergencePointType`]) binds
    /// through the SAME `has(kind)` shape rather than restating either
    /// the resolver walk or the closed-set equality plus the typed-
    /// projection hop at the callsite.
    #[must_use]
    pub fn has_input_arity(&self, kind: Arity) -> bool {
        self.resolved_classification().has_input_arity(kind)
    }

    /// True iff the resolved [`Classification`]'s nested
    /// [`ConvergencePointType`] projects (via the many-to-one
    /// [`ConvergencePointType::output_arity`] typed projection) to the
    /// given [`Arity`] discriminator — byte-for-byte peer of
    /// [`Classification::has_output_arity`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-omitted
    /// `:classification` slot reads as the [`default_ephemeral_class`]
    /// baseline the sibling `From<EphemeralSpec> for ProcessSpec`
    /// lowering fills.
    ///
    /// # Two-surface parity contract
    ///
    /// A given [`EphemeralSpec`] classifies identically through this
    /// primitive AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.has_output_arity(kind)`
    /// on the lowered `ProcessSpec` — the `Cow<'_, Classification>`
    /// resolver on this side and the `.unwrap_or_else(...)` fill on the
    /// lowering side both dereference the same
    /// `default_ephemeral_class()` value on `None` and the same
    /// authored value on `Some(_)`, and the sibling
    /// [`Classification::has_output_arity`] applies the same
    /// `point_type.output_arity()` typed projection on both sides. This
    /// means the ephemeral-surface `output-arity-<kind>` `:requires`
    /// family in `tatara-reconciler::bin::tatara-check` publishes the
    /// SAME truth on the SAME authored spec as the point-surface family
    /// on the mechanically-lowered `ProcessSpec`.
    ///
    /// # EIGHTH classification-axis peer — closes the ephemeral-side DAG-composition arity pair
    ///
    /// Peer of [`Self::has_point_type`], [`Self::has_substrate`],
    /// [`Self::has_calm`], [`Self::has_data_classification`],
    /// [`Self::has_horizon_kind`], [`Self::has_optimization_direction`],
    /// and [`Self::has_input_arity`] — all eight route through the SAME
    /// [`Self::resolved_classification`] resolver, so the operator-
    /// omitted `:classification` slot's fill-through logic lives at ONE
    /// substrate primitive rather than being restated in each per-axis
    /// probe body. SECOND occupant on the (Option-parent × NESTED-
    /// STRUCT-scalar-child × derived-typed-projection) corner on the
    /// ephemeral surface — co-tenant with [`Self::has_input_arity`] on
    /// the SAME `point_type` scalar carrier through the SAME [`Arity`]
    /// closed set but through the sibling many-to-one typed projection
    /// [`ConvergencePointType::output_arity`] (`Transform | Join | Gate
    /// | Select | Reduce | Observe → One`, `Fork | Broadcast → Many`).
    /// Closes the DAG-composition arity pair on the ephemeral side —
    /// the two projections DISAGREE on the diffusive arms `Fork |
    /// Broadcast` (input `One` vs. output `Many`) and on the convergent
    /// arms `Join | Gate | Select | Reduce` (input `Many` vs. output
    /// `One`), and AGREE on the endomorphic arms `Transform | Observe`
    /// (both `One`). Byte-for-byte symmetric with the DAG-composition
    /// arity pair on the point surface ([`Classification::has_input_arity`] +
    /// [`Classification::has_output_arity`]) — THAT pair walks a required
    /// [`Classification`] carrier; THIS pair walks the SAME projection
    /// pair on the `Cow`-resolver carrier so the resolver walk composes
    /// with the projection at ONE substrate site rather than being
    /// restated per surface.
    ///
    /// # Semantics — VARIANT match on the projected image
    ///
    /// [`Arity`] carries no `Default` impl (the 2-arm bare enum with no
    /// `#[default]`), so exactly ONE of the two arms answers `true` per
    /// well-formed [`EphemeralSpec`], with no default-arm short-circuit
    /// shortcut. The absent-`:classification` baseline
    /// [`default_ephemeral_class`] fills `point_type: Gate`, and
    /// [`ConvergencePointType::output_arity`] projects `Gate → One`, so
    /// the ephemeral sugar surface's `output-arity-One` require-tag
    /// reads `true` on every operator-authored spec that omits the
    /// `:classification` slot — pinning the workspace's convergent-by-
    /// default point posture on the output side. The many-to-one
    /// projection shape means the answer is invariant under intra-
    /// bucket point-type swaps (`Fork ↔ Broadcast` both keep
    /// `output-arity-Many = true`; `Transform ↔ Join ↔ Gate ↔ Select ↔
    /// Reduce ↔ Observe` all keep `output-arity-One = true`) and flips
    /// at bucket boundaries (`Fork ↔ Transform` flips `output-arity-
    /// Many` from `true` to `false`). A regression that dropped the
    /// resolver hop, probed [`ConvergencePointType`] directly (dropping
    /// the `.output_arity()` call), inverted the projection (`One ↔
    /// Many`), or crossed the wires with the sibling
    /// [`ConvergencePointType::input_arity`] projection fails HERE at
    /// ONE narrow substrate site before drifting through every
    /// unadorned ephemeral spec's baseline output-arity answer.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-axis presence-probe body
    /// composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE closed-set primitive
    /// ([`Classification::has_output_arity`]) so every downstream
    /// (`output-arity-<kind>` require-tag families on both surfaces in
    /// tatara-check, closed-set audit dispatchers, future variant
    /// additions on [`Arity`] or on [`ConvergencePointType`]) binds
    /// through the SAME `has(kind)` shape rather than restating either
    /// the resolver walk or the closed-set equality plus the typed-
    /// projection hop at the callsite.
    #[must_use]
    pub fn has_output_arity(&self, kind: Arity) -> bool {
        self.resolved_classification().has_output_arity(kind)
    }

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s [`Horizon`] project to `true`
    /// under [`crate::classification::HorizonKind::terminates`]?
    /// Byte-for-byte peer of
    /// [`Classification::horizon_terminates`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot on `(defephemeral …)` still
    /// answers via the substrate default. The ONE ephemeral-surface
    /// substrate primitive that owns the `(&EphemeralSpec) -> bool`
    /// derived-nullary-boolean walk on the classification-horizon
    /// axis.
    ///
    /// # Two-surface parity — resolver hop + Classification primitive
    ///
    /// Peer of [`Self::has_point_type`], [`Self::has_substrate`],
    /// [`Self::has_calm`], [`Self::has_data_classification`],
    /// [`Self::has_horizon_kind`],
    /// [`Self::has_optimization_direction`],
    /// [`Self::has_input_arity`], and [`Self::has_output_arity`] on
    /// the (resolver-hop × [`Classification`] presence primitive)
    /// axis: all nine methods route through the SAME
    /// [`Self::resolved_classification`] resolver, and each composes
    /// against ONE [`Classification`] primitive. This method
    /// distinguishes itself by targeting the [`Classification`]
    /// primitive [`Classification::horizon_terminates`] which is the
    /// FIRST derived-nullary-boolean (no closed-set argument)
    /// primitive on the [`Classification`] surface — every prior
    /// peer probe on [`Classification`] admits a closed-set `kind`
    /// argument and answers a variant-equality question, while this
    /// probe collapses [`HorizonKind::ALL`] onto a single boolean
    /// via the closed set's own [`HorizonKind::terminates`]
    /// predicate.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `horizon_terminates()` returns `true` iff
    /// `self.resolved_classification().horizon_terminates()`. The
    /// resolver returns the authored [`Classification`] when
    /// present and the substrate default
    /// [`Classification::gate_compute`] on absence. Because
    /// [`Classification::gate_compute`] uses [`Horizon::default`]
    /// (whose `kind` field defaults to [`HorizonKind::Bounded`] via
    /// `#[default]`), a bare ephemeral spec with no `:classification`
    /// slot answers `true` — the default-arm short-circuit
    /// propagates through THREE layers of `Default`
    /// ([`Classification::gate_compute`] → [`Horizon::default`] →
    /// [`HorizonKind::default`]) to this predicate's answer, matching
    /// the default-arm shortcut every prior defaulted-child probe
    /// on this surface publishes. A regression that dropped the
    /// resolver hop, probed [`Classification::has_horizon_kind`]
    /// directly (dropping the `.terminates()` projection), or
    /// crossed the wires with the antisymmetric partner
    /// [`HorizonKind::requires_metric_axes`] fails HERE at ONE
    /// narrow substrate site before drifting through every
    /// unadorned ephemeral spec's baseline horizon-terminates
    /// answer.
    ///
    /// # Compounding
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `terminating-horizon` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `terminating-horizon` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via [`Classification::horizon_terminates`]
    /// directly. The two-surface parity contract holds by
    /// construction: both surfaces route through the SAME
    /// [`Classification::horizon_terminates`] primitive after the
    /// ephemeral surface pays ONE resolver hop — a future
    /// [`HorizonKind`] variant or a future normalization at the
    /// substrate primitive lands at ONE site and both surfaces'
    /// `terminating-horizon` fixed tags inherit the shift
    /// mechanically. A future co-tenant peer on this surface (a
    /// hypothetical `horizon_requires_metric_axes` composing the
    /// antisymmetric partner [`HorizonKind::requires_metric_axes`]
    /// through the SAME resolver hop) lands as ONE peer inherent
    /// method with the same nullary-derived body.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-axis derived-nullary-
    /// boolean probe body composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::horizon_terminates`]) so every downstream
    /// (`terminating-horizon` fixed tags on both surfaces in
    /// tatara-check, future scheduler / termination-shape
    /// validators, future variant additions on [`HorizonKind`])
    /// binds through the SAME `horizon_terminates()` shape rather
    /// than restating either the resolver walk or the closed-set
    /// projection composition at the callsite. THEORY.md §VI.1 —
    /// generation over composition; a future [`HorizonKind`]
    /// variant lands at ONE `ALL` entry + ONE `terminates` arm on
    /// the closed set and both surfaces pick it up mechanically.
    #[must_use]
    pub fn horizon_terminates(&self) -> bool {
        self.resolved_classification().horizon_terminates()
    }

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s [`Horizon`] project to `true`
    /// under [`crate::classification::HorizonKind::requires_metric_axes`]?
    /// Byte-for-byte peer of
    /// [`Classification::horizon_requires_metric_axes`] wrapped
    /// through the [`Self::resolved_classification`] resolver so an
    /// operator-omitted `:classification` slot on `(defephemeral …)`
    /// still answers via the substrate default. The ONE ephemeral-
    /// surface substrate primitive that owns the
    /// `(&EphemeralSpec) -> bool` derived-nullary-boolean walk on
    /// the metric-axes-required question over the classification-
    /// horizon axis.
    ///
    /// # Antisymmetric peer of [`Self::horizon_terminates`]
    ///
    /// Byte-for-byte antisymmetric peer of [`Self::horizon_terminates`]
    /// via the SAME [`Self::resolved_classification`] resolver hop
    /// and the SAME closed set [`crate::classification::HorizonKind`]:
    /// [`Self::horizon_terminates`] composes
    /// [`Classification::horizon_terminates`] (walking
    /// [`crate::classification::HorizonKind::terminates`]); this
    /// method composes the ANTISYMMETRIC partner
    /// [`Classification::horizon_requires_metric_axes`] (walking
    /// [`crate::classification::HorizonKind::requires_metric_axes`]).
    /// The closed set pins the XOR contract
    /// `terminates() ^ requires_metric_axes()` on every variant, so
    /// exactly ONE of these two ephemeral-surface derived-nullary
    /// probes answers `true` per resolved [`Classification`] and the
    /// two probes together partition the resolver's output space into
    /// two disjoint buckets on every ephemeral spec — authored or
    /// defaulted.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `horizon_requires_metric_axes()` returns `true` iff
    /// `self.resolved_classification().horizon_requires_metric_axes()`.
    /// The resolver returns the authored [`Classification`] when
    /// present and the substrate default
    /// [`Classification::gate_compute`] on absence. Because
    /// [`Classification::gate_compute`] uses [`Horizon::default`]
    /// (whose `kind` field defaults to
    /// [`crate::classification::HorizonKind::Bounded`] via
    /// `#[default]`), a bare ephemeral spec with no `:classification`
    /// slot answers `false` — the default-arm short-circuit
    /// propagates through THREE layers of `Default`
    /// ([`Classification::gate_compute`] → [`Horizon::default`] →
    /// [`crate::classification::HorizonKind::default`]) to this
    /// predicate's answer, the mirror image of
    /// [`Self::horizon_terminates`]'s default-arm `true` answer. A
    /// regression that dropped the resolver hop, probed
    /// [`Classification::has_horizon_kind`] directly (dropping the
    /// `.requires_metric_axes()` projection), or crossed the wires
    /// with the antisymmetric partner
    /// [`crate::classification::HorizonKind::terminates`] fails HERE
    /// at ONE narrow substrate site before drifting through every
    /// unadorned ephemeral spec's baseline metric-provisioning
    /// answer.
    ///
    /// # Compounding
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `metric-axes-required` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `metric-axes-required` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::horizon_requires_metric_axes`] directly. The
    /// two-surface parity contract holds by construction: both
    /// surfaces route through the SAME
    /// [`Classification::horizon_requires_metric_axes`] primitive
    /// after the ephemeral surface pays ONE resolver hop — a future
    /// [`crate::classification::HorizonKind`] variant or a future
    /// normalization at the substrate primitive lands at ONE site and
    /// both surfaces' `metric-axes-required` fixed tags inherit the
    /// shift mechanically.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-axis derived-nullary-
    /// boolean probe body composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::horizon_requires_metric_axes`]) so every
    /// downstream (`metric-axes-required` fixed tags on both
    /// surfaces in tatara-check, future scheduler / metric-
    /// provisioning validators, future variant additions on
    /// [`crate::classification::HorizonKind`]) binds through the
    /// SAME `horizon_requires_metric_axes()` shape rather than
    /// restating either the resolver walk or the closed-set
    /// projection composition at the callsite. THEORY.md §VI.1 —
    /// generation over composition; a future
    /// [`crate::classification::HorizonKind`] variant lands at ONE
    /// `ALL` entry + ONE `requires_metric_axes` arm on the closed
    /// set and both surfaces pick it up mechanically.
    #[must_use]
    pub fn horizon_requires_metric_axes(&self) -> bool {
        self.resolved_classification()
            .horizon_requires_metric_axes()
    }

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s [`crate::classification::CalmClassification`]
    /// project to `true` under
    /// [`crate::classification::CalmClassification::requires_coordination`]?
    /// Byte-for-byte peer of
    /// [`Classification::calm_requires_coordination`] wrapped through
    /// the [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot on `(defephemeral …)` still
    /// answers via the substrate default. The ONE ephemeral-surface
    /// substrate primitive that owns the `(&EphemeralSpec) -> bool`
    /// derived-nullary-boolean walk on the coordination-required
    /// question over the classification-calm axis.
    ///
    /// # Third derived-nullary-boolean peer on the ephemeral surface
    ///
    /// Peer of [`Self::horizon_terminates`] and
    /// [`Self::horizon_requires_metric_axes`] on the ephemeral
    /// surface's (resolver-hop × derived-nullary-bool) shape — the
    /// FIRST peer threading the classification-calm axis rather than
    /// the classification-horizon axis. Distinct from both prior
    /// derived-nullary peers by ONE structural degree at the underlying
    /// [`Classification`] primitive: [`Self::horizon_terminates`] +
    /// [`Self::horizon_requires_metric_axes`] both walk the nested
    /// `.horizon.kind` sub-slot's derived projection, while this probe
    /// walks the direct scalar `.calm` field's derived projection.
    /// The resolver-hop shape is byte-identical.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `calm_requires_coordination()` returns `true` iff
    /// `self.resolved_classification().calm_requires_coordination()`.
    /// The resolver returns the authored [`Classification`] when
    /// present and the substrate default
    /// [`Classification::gate_compute`] on absence. Because
    /// [`Classification::gate_compute`] carries
    /// [`crate::classification::CalmClassification::default = Monotone`],
    /// a bare ephemeral spec with no `:classification` slot answers
    /// `false` — the default-arm short-circuit propagates through TWO
    /// layers of `Default` ([`Classification::gate_compute`] →
    /// [`crate::classification::CalmClassification::default`]) to this
    /// predicate's answer. Distinct from the two `horizon_*` peers on
    /// this surface, which short-circuit through THREE layers of
    /// `Default` ([`Classification::gate_compute`] → [`Horizon::default`]
    /// → [`HorizonKind::default`]) because the horizon axis has a
    /// nested-struct wrapper between the classification field and the
    /// closed-set discriminator. A regression that dropped the
    /// resolver hop, probed [`Classification::has_calm`] directly
    /// (dropping the `.requires_coordination()` projection), or
    /// inverted the projection (silently promoting the Monotone
    /// baseline to "requires coordination") fails HERE at ONE narrow
    /// substrate site before drifting through every unadorned
    /// ephemeral spec's baseline coordination-mode answer.
    ///
    /// # Compounding
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `coordination-required` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `coordination-required` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::calm_requires_coordination`] directly. The
    /// two-surface parity contract holds by construction: both
    /// surfaces route through the SAME
    /// [`Classification::calm_requires_coordination`] primitive after
    /// the ephemeral surface pays ONE resolver hop — a future
    /// [`crate::classification::CalmClassification`] variant or a
    /// future normalization at the substrate primitive lands at ONE
    /// site and both surfaces' `coordination-required` fixed tags
    /// inherit the shift mechanically.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-axis derived-nullary-
    /// boolean probe body composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::calm_requires_coordination`]) so every
    /// downstream (`coordination-required` fixed tags on both
    /// surfaces in tatara-check, future scheduler / coordination-mode
    /// validators, future variant additions on
    /// [`crate::classification::CalmClassification`]) binds through
    /// the SAME `calm_requires_coordination()` shape rather than
    /// restating either the resolver walk or the closed-set
    /// projection composition at the callsite. THEORY.md §VI.1 —
    /// generation over composition; a future
    /// [`crate::classification::CalmClassification`] variant lands at
    /// ONE `ALL` entry + ONE `requires_coordination` arm on the
    /// closed set and both surfaces pick it up mechanically.
    #[must_use]
    pub fn calm_requires_coordination(&self) -> bool {
        self.resolved_classification().calm_requires_coordination()
    }

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s [`crate::classification::DataClassification`]
    /// project to `true` under
    /// [`crate::classification::DataClassification::is_regulated`]?
    /// Byte-for-byte peer of
    /// [`Classification::data_is_regulated`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot on `(defephemeral …)` still
    /// answers via the substrate default. The ONE ephemeral-surface
    /// substrate primitive that owns the `(&EphemeralSpec) -> bool`
    /// derived-nullary-boolean walk on the regulated-data question
    /// over the classification-data axis.
    ///
    /// # Fourth derived-nullary-boolean peer on the ephemeral surface
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`], and
    /// [`Self::calm_requires_coordination`] on the ephemeral surface's
    /// (resolver-hop × derived-nullary-bool) shape — the FIRST peer
    /// threading the classification-data axis rather than the horizon
    /// or calm axes. Structural byte-for-byte peer of
    /// [`Self::calm_requires_coordination`]: both walk a DIRECT scalar
    /// closed-set field's derived projection on the resolved
    /// [`Classification`] (`.calm.requires_coordination()` /
    /// `.data_classification.is_regulated()`) — TWO layers of
    /// `Default` short-circuit ([`Classification::gate_compute`] →
    /// the direct scalar child's `#[default]`) — distinct from the
    /// two `horizon_*` peers which walk a NESTED-STRUCT projection
    /// (`.horizon.kind`) with THREE layers of `Default`. The resolver-
    /// hop shape is byte-identical across all four peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `data_is_regulated()` returns `true` iff
    /// `self.resolved_classification().data_is_regulated()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// [`crate::classification::DataClassification::default = Internal`],
    /// a bare ephemeral spec with no `:classification` slot answers
    /// `false` — the default-arm short-circuit propagates through TWO
    /// layers of `Default` ([`Classification::gate_compute`] →
    /// [`crate::classification::DataClassification::default`]) to
    /// this predicate's answer, mirror-image of
    /// [`Self::calm_requires_coordination`]'s Monotone-default
    /// short-circuit through the same structural depth. Distinct
    /// from the two `horizon_*` peers on this surface which short-
    /// circuit through THREE layers of `Default` because the horizon
    /// axis has a nested-struct wrapper. A regression that dropped
    /// the resolver hop, probed [`Classification::has_data_classification`]
    /// directly (dropping the `.is_regulated()` projection), or
    /// inverted the projection (silently promoting the Internal
    /// baseline to "regulated") fails HERE at ONE narrow substrate
    /// site before drifting through every unadorned ephemeral spec's
    /// baseline regulatory-regime answer.
    ///
    /// # Compounding
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `data-regulated` on `EPHEMERAL_FIXED_TAG_ARMS`
    /// — byte-for-byte peer of the point surface's `data-regulated`
    /// fixed tag on `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::data_is_regulated`] directly. The two-
    /// surface parity contract holds by construction: both surfaces
    /// route through the SAME
    /// [`Classification::data_is_regulated`] primitive after the
    /// ephemeral surface pays ONE resolver hop — a future
    /// [`crate::classification::DataClassification`] variant or a
    /// future normalization at the substrate primitive lands at ONE
    /// site and both surfaces' `data-regulated` fixed tags inherit
    /// the shift mechanically.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-data-axis derived-nullary-
    /// boolean probe body composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::data_is_regulated`]) so every downstream
    /// (`data-regulated` fixed tags on both surfaces in tatara-check,
    /// future compliance-baseline / regulatory-regime validators,
    /// future variant additions on
    /// [`crate::classification::DataClassification`]) binds through
    /// the SAME `data_is_regulated()` shape rather than restating
    /// either the resolver walk or the closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation
    /// over composition; a future
    /// [`crate::classification::DataClassification`] variant lands
    /// at ONE `ALL` entry + ONE `is_regulated` arm on the closed set
    /// and both surfaces pick it up mechanically.
    #[must_use]
    pub fn data_is_regulated(&self) -> bool {
        self.resolved_classification().data_is_regulated()
    }

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s [`crate::classification::DataClassification`]
    /// project to `true` under
    /// [`crate::classification::DataClassification::is_restricted`]?
    /// Byte-for-byte peer of
    /// [`Classification::data_is_restricted`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot on `(defephemeral …)` still
    /// answers via the substrate default. The ONE ephemeral-surface
    /// substrate primitive that owns the `(&EphemeralSpec) -> bool`
    /// derived-nullary-boolean walk on the restricted-data question
    /// over the classification-data axis.
    ///
    /// # Fifth derived-nullary-boolean peer on the ephemeral surface
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], and
    /// [`Self::data_is_regulated`] on the ephemeral surface's
    /// (resolver-hop × derived-nullary-bool) shape — the SECOND peer
    /// threading the classification-data axis after
    /// [`Self::data_is_regulated`] opened it, pinning the data axis
    /// as a proven-repeatable structural sub-corner across TWO sibling
    /// closed-set projections (`is_regulated` / `is_restricted`).
    /// Structural byte-for-byte peer of
    /// [`Self::data_is_regulated`]: both walk the SAME DIRECT scalar
    /// closed-set field's derived projection on the resolved
    /// [`Classification`] (`.data_classification.is_regulated()` /
    /// `.is_restricted()`) — TWO layers of `Default` short-circuit
    /// ([`Classification::gate_compute`] → [`crate::classification::DataClassification::default = Internal`])
    /// — distinct from the two `horizon_*` peers which walk a NESTED-
    /// STRUCT projection (`.horizon.kind`) with THREE layers of
    /// `Default`. The resolver-hop shape is byte-identical across all
    /// five peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `data_is_restricted()` returns `true` iff
    /// `self.resolved_classification().data_is_restricted()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// [`crate::classification::DataClassification::default = Internal`],
    /// a bare ephemeral spec with no `:classification` slot answers
    /// `true` — the default-arm short-circuit propagates through TWO
    /// layers of `Default` ([`Classification::gate_compute`] →
    /// [`crate::classification::DataClassification::default`]) to
    /// this predicate's answer. FIRST direct-scalar ephemeral-surface
    /// peer whose absent-classification default answers `true`, not
    /// `false` (`data_is_regulated` and `calm_requires_coordination`
    /// both project `false` on the same absent classification),
    /// mirror-image of [`Self::horizon_terminates`]'s `Bounded`-default
    /// `true` baseline on the nested-struct sub-corner. A regression
    /// that dropped the resolver hop, probed
    /// [`Classification::has_data_classification`] directly (dropping
    /// the `.is_restricted()` projection), or inverted the projection
    /// (silently demoting the Internal baseline to "unrestricted")
    /// fails HERE at ONE narrow substrate site before drifting
    /// through every unadorned ephemeral spec's baseline access-
    /// control-mandatory answer.
    ///
    /// # Compounding
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `data-restricted` on `EPHEMERAL_FIXED_TAG_ARMS`
    /// — byte-for-byte peer of the point surface's `data-restricted`
    /// fixed tag on `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::data_is_restricted`] directly. The two-
    /// surface parity contract holds by construction: both surfaces
    /// route through the SAME
    /// [`Classification::data_is_restricted`] primitive after the
    /// ephemeral surface pays ONE resolver hop — a future
    /// [`crate::classification::DataClassification`] variant or a
    /// future normalization at the substrate primitive lands at ONE
    /// site and both surfaces' `data-restricted` fixed tags inherit
    /// the shift mechanically. The closed-set-internal implication
    /// `is_regulated() ⇒ is_restricted()` composes through the
    /// resolver hop to
    /// `data_is_regulated() ⇒ data_is_restricted()` at this surface
    /// too.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-data-axis derived-nullary-
    /// boolean probe body composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::data_is_restricted`]) so every downstream
    /// (`data-restricted` fixed tags on both surfaces in tatara-check,
    /// future compliance-baseline / access-control-mandatory
    /// validators, future variant additions on
    /// [`crate::classification::DataClassification`]) binds through
    /// the SAME `data_is_restricted()` shape rather than restating
    /// either the resolver walk or the closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation
    /// over composition; a future
    /// [`crate::classification::DataClassification`] variant lands
    /// at ONE `ALL` entry + ONE `is_restricted` arm on the closed set
    /// and both surfaces pick it up mechanically.
    #[must_use]
    pub fn data_is_restricted(&self) -> bool {
        self.resolved_classification().data_is_restricted()
    }

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s
    /// [`crate::classification::ConvergencePointType`] project to
    /// `true` under
    /// [`crate::classification::ConvergencePointType::is_endomorphic`]?
    /// Byte-for-byte peer of
    /// [`Classification::point_is_endomorphic`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot on `(defephemeral …)` still
    /// answers via the substrate default. The ONE ephemeral-surface
    /// substrate primitive that owns the `(&EphemeralSpec) -> bool`
    /// derived-nullary-boolean walk on the 1→1 topology-bucket
    /// question over the classification-`point_type` axis.
    ///
    /// # Sixth derived-nullary-boolean peer on the ephemeral surface
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`],
    /// [`Self::data_is_regulated`], and [`Self::data_is_restricted`]
    /// on the ephemeral surface's (resolver-hop × derived-nullary-bool)
    /// shape — the FIRST peer threading the classification-`point_type`
    /// axis after the two `horizon_*`, one `calm_*`, and two `data_*`
    /// peers populated the horizon, calm, and data axes. Direct-scalar
    /// peer of the sibling `data_*` and `calm_*` arms but distinct by
    /// ONE structural degree at the underlying [`Classification`]
    /// primitive: [`crate::classification::ConvergencePointType`] has
    /// NO [`Default`] impl, so the absent-`:classification` baseline
    /// answers `false` via the resolver's substrate default
    /// [`Classification::gate_compute`] carrying its chosen
    /// `point_type: Gate` field (not via a `#[default]` short-circuit
    /// on the point-type axis itself). The resolver-hop shape is
    /// byte-identical across all six peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `point_is_endomorphic()` returns `true` iff
    /// `self.resolved_classification().point_is_endomorphic()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// [`crate::classification::ConvergencePointType::Gate`] (a
    /// convergent barrier point, not a 1→1 endomorphism), a bare
    /// ephemeral spec with no `:classification` slot answers `false`.
    /// A regression that dropped the resolver hop, probed the wrong
    /// closed-set arm, or inverted the projection fails HERE at ONE
    /// narrow substrate site before drifting through every unadorned
    /// ephemeral spec's DAG-composition answer.
    ///
    /// # Compounding
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `endomorphic-point` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `endomorphic-point` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::point_is_endomorphic`] directly. The two-
    /// surface parity contract holds by construction: both surfaces
    /// route through the SAME
    /// [`Classification::point_is_endomorphic`] primitive after the
    /// ephemeral surface pays ONE resolver hop — a future
    /// [`crate::classification::ConvergencePointType`] variant or a
    /// future normalization at the substrate primitive lands at ONE
    /// site and both surfaces' `endomorphic-point` fixed tags inherit
    /// the shift mechanically. Sibling projections
    /// [`crate::classification::ConvergencePointType::is_diffusive`]
    /// and [`crate::classification::ConvergencePointType::is_convergent`]
    /// compose byte-identically as future seventh + eighth ephemeral-
    /// surface peers; when all three land the three-way partition
    /// contract sealed on the closed set by
    /// `convergence_point_type_buckets_cover_every_variant` composes
    /// through the resolver-hop layer as a substrate-wide theorem.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`point_type`-axis derived-
    /// nullary-boolean probe body composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::point_is_endomorphic`]) so every downstream
    /// (`endomorphic-point` fixed tags on both surfaces in tatara-check,
    /// future DAG composition / edge-cardinality validators, future
    /// variant additions on
    /// [`crate::classification::ConvergencePointType`]) binds through
    /// the SAME `point_is_endomorphic()` shape rather than restating
    /// either the resolver walk or the closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation over
    /// composition; a future
    /// [`crate::classification::ConvergencePointType`] variant lands
    /// at ONE `ALL` entry + ONE `is_endomorphic` arm on the closed
    /// set and both surfaces pick it up mechanically.
    #[must_use]
    pub fn point_is_endomorphic(&self) -> bool {
        self.resolved_classification().point_is_endomorphic()
    }

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s
    /// [`crate::classification::ConvergencePointType`] project to
    /// `true` under
    /// [`crate::classification::ConvergencePointType::is_diffusive`]?
    /// Byte-for-byte peer of
    /// [`Classification::point_is_diffusive`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot on `(defephemeral …)` still
    /// answers via the substrate default. The ONE ephemeral-surface
    /// substrate primitive that owns the `(&EphemeralSpec) -> bool`
    /// derived-nullary-boolean walk on the 1→N fan-out topology-bucket
    /// question over the classification-`point_type` axis.
    ///
    /// # Seventh derived-nullary-boolean peer on the ephemeral surface
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`],
    /// [`Self::data_is_regulated`], [`Self::data_is_restricted`], and
    /// [`Self::point_is_endomorphic`] on the ephemeral surface's
    /// (resolver-hop × derived-nullary-bool) shape — the SEVENTH peer
    /// overall and the SECOND peer threading the classification-
    /// `point_type` axis. Direct-scalar peer of
    /// [`Self::point_is_endomorphic`]: both compose the SAME resolver
    /// hop and the SAME closed-set carrier through the SAME chosen-
    /// field baseline discipline (`Gate.is_diffusive() = false`,
    /// mirror-image of `Gate.is_endomorphic() = false`). The
    /// resolver-hop shape is byte-identical across all seven peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `point_is_diffusive()` returns `true` iff
    /// `self.resolved_classification().point_is_diffusive()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// [`crate::classification::ConvergencePointType::Gate`] (a
    /// convergent barrier, not a fan-out), a bare ephemeral spec with
    /// no `:classification` slot answers `false`. A regression that
    /// dropped the resolver hop, probed the wrong closed-set arm, or
    /// inverted the projection fails HERE at ONE narrow substrate
    /// site before drifting through every unadorned ephemeral spec's
    /// DAG-composition answer.
    ///
    /// # Compounding — first ephemeral-surface corner-peer mutex on the `point_type` axis
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `diffusive-point` on `EPHEMERAL_FIXED_TAG_ARMS`
    /// — byte-for-byte peer of the point surface's `diffusive-point`
    /// fixed tag on `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::point_is_diffusive`] directly. The two-
    /// surface parity contract holds by construction: both surfaces
    /// route through the SAME
    /// [`Classification::point_is_diffusive`] primitive after the
    /// ephemeral surface pays ONE resolver hop. FIRST ephemeral-
    /// surface corner-peer pair on the `point_type` axis (with
    /// [`Self::point_is_endomorphic`]) whose two projections carry a
    /// non-trivial closed-set-internal MUTEX relationship
    /// (`point_is_endomorphic ⇒ ¬point_is_diffusive`), distinct from
    /// the sibling `data`-axis ephemeral corner-peer pair whose two
    /// projections carry a non-trivial IMPLICATION relationship. When
    /// the third sibling [`Self::point_is_convergent`] lands, the
    /// mutex closes into the full three-way XOR partition composed
    /// through the resolver-hop layer as a substrate-wide theorem.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`point_type`-axis derived-
    /// nullary-boolean probe body composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::point_is_diffusive`]) so every downstream
    /// (`diffusive-point` fixed tags on both surfaces in tatara-check,
    /// future DAG composition / edge-cardinality validators, future
    /// variant additions on
    /// [`crate::classification::ConvergencePointType`]) binds through
    /// the SAME `point_is_diffusive()` shape rather than restating
    /// either the resolver walk or the closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation over
    /// composition; a future
    /// [`crate::classification::ConvergencePointType`] variant lands
    /// at ONE `ALL` entry + ONE `is_diffusive` arm on the closed set
    /// and both surfaces pick it up mechanically.
    #[must_use]
    pub fn point_is_diffusive(&self) -> bool {
        self.resolved_classification().point_is_diffusive()
    }

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s
    /// [`crate::classification::ConvergencePointType`] project to
    /// `true` under
    /// [`crate::classification::ConvergencePointType::is_convergent`]?
    /// Byte-for-byte peer of
    /// [`Classification::point_is_convergent`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot on `(defephemeral …)` still
    /// answers via the substrate default. The ONE ephemeral-surface
    /// substrate primitive that owns the `(&EphemeralSpec) -> bool`
    /// derived-nullary-boolean walk on the N→1 fan-in topology-bucket
    /// question over the classification-`point_type` axis.
    ///
    /// # Eighth derived-nullary-boolean peer on the ephemeral surface
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`],
    /// [`Self::data_is_regulated`], [`Self::data_is_restricted`],
    /// [`Self::point_is_endomorphic`], and [`Self::point_is_diffusive`]
    /// on the ephemeral surface's (resolver-hop × derived-nullary-
    /// bool) shape — the EIGHTH peer overall and the THIRD peer
    /// threading the classification-`point_type` axis. Direct-scalar
    /// peer of [`Self::point_is_endomorphic`] and
    /// [`Self::point_is_diffusive`]: the three compose the SAME
    /// resolver hop and the SAME closed-set carrier through the SAME
    /// chosen-field baseline discipline, but the answer flips on the
    /// baseline — `Gate.is_convergent() = true`, so an ephemeral spec
    /// with no `:classification` slot answers `true` HERE (mirror-
    /// inverted from the two sibling probes which answer `false`).
    /// The resolver-hop shape is byte-identical across all eight
    /// peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `point_is_convergent()` returns `true` iff
    /// `self.resolved_classification().point_is_convergent()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// [`crate::classification::ConvergencePointType::Gate`] (the
    /// canonical convergent barrier), a bare ephemeral spec with no
    /// `:classification` slot answers `true` — a regression that
    /// dropped the resolver hop, probed the wrong closed-set arm, or
    /// inverted the projection fails HERE at ONE narrow substrate
    /// site before drifting through every unadorned ephemeral spec's
    /// DAG-composition answer.
    ///
    /// # Compounding — closes the three-way XOR partition on the ephemeral surface
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `convergent-point` on `EPHEMERAL_FIXED_TAG_ARMS`
    /// — byte-for-byte peer of the point surface's `convergent-point`
    /// fixed tag on `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::point_is_convergent`] directly. The two-
    /// surface parity contract holds by construction: both surfaces
    /// route through the SAME
    /// [`Classification::point_is_convergent`] primitive after the
    /// ephemeral surface pays ONE resolver hop. THIRD ephemeral-
    /// surface peer on the `point_type` axis closing the mutex pair
    /// [`Self::point_is_endomorphic`] / [`Self::point_is_diffusive`]
    /// into the FULL three-way XOR partition contract composed
    /// through the resolver-hop layer as a substrate-wide theorem.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`point_type`-axis derived-
    /// nullary-boolean probe body composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::point_is_convergent`]) so every downstream
    /// (`convergent-point` fixed tags on both surfaces in tatara-check,
    /// future DAG composition / edge-cardinality validators, future
    /// variant additions on
    /// [`crate::classification::ConvergencePointType`]) binds through
    /// the SAME `point_is_convergent()` shape rather than restating
    /// either the resolver walk or the closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation over
    /// composition; a future
    /// [`crate::classification::ConvergencePointType`] variant lands
    /// at ONE `ALL` entry + ONE `is_convergent` arm on the closed set
    /// and both surfaces pick it up mechanically.
    #[must_use]
    pub fn point_is_convergent(&self) -> bool {
        self.resolved_classification().point_is_convergent()
    }

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s
    /// [`crate::classification::SubstrateType`] project to `true`
    /// under [`crate::classification::SubstrateType::is_resource`]?
    /// Byte-for-byte peer of
    /// [`Classification::substrate_is_resource`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot on `(defephemeral …)` still
    /// answers via the substrate default. The ONE ephemeral-surface
    /// substrate primitive that owns the `(&EphemeralSpec) -> bool`
    /// derived-nullary-boolean walk on the resource-plane bucket
    /// question over the classification-`substrate` axis.
    ///
    /// # Ninth derived-nullary-boolean peer on the ephemeral surface
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`],
    /// [`Self::data_is_regulated`], [`Self::data_is_restricted`],
    /// [`Self::point_is_endomorphic`], [`Self::point_is_diffusive`],
    /// and [`Self::point_is_convergent`] on the ephemeral surface's
    /// (resolver-hop × derived-nullary-bool) shape — the NINTH peer
    /// overall and the FIRST peer threading the classification-
    /// `substrate` axis (the fourth of six classification axes
    /// participating on this corner, after `horizon`, `calm`,
    /// `data_classification`, and `point_type`). The resolver-hop
    /// shape is byte-identical across all nine peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `substrate_is_resource()` returns `true` iff
    /// `self.resolved_classification().substrate_is_resource()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// [`crate::classification::SubstrateType::Compute`] (the
    /// canonical resource-plane substrate), a bare ephemeral spec
    /// with no `:classification` slot answers `true` — a regression
    /// that dropped the resolver hop, probed the wrong closed-set
    /// arm, or inverted the projection fails HERE at ONE narrow
    /// substrate site before drifting through every unadorned
    /// ephemeral spec's plane-baseline answer.
    ///
    /// # Compounding — opens the substrate axis on the ephemeral surface
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `resource-substrate` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `resource-substrate` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::substrate_is_resource`] directly. The two-
    /// surface parity contract holds by construction: both surfaces
    /// route through the SAME
    /// [`Classification::substrate_is_resource`] primitive after the
    /// ephemeral surface pays ONE resolver hop. FIRST ephemeral-
    /// surface peer on the `substrate` axis — future sibling
    /// projections [`crate::classification::SubstrateType::is_policy`]
    /// and [`crate::classification::SubstrateType::is_telemetry`]
    /// compose byte-identically as future tenth + eleventh peers,
    /// closing the axis into a proven-repeatable three-peer sub-
    /// corner exactly as the `point_type` axis was closed on this
    /// surface by
    /// `ephemeral_point_type_probes_form_three_way_xor_partition_over_all`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`substrate`-axis derived-
    /// nullary-boolean probe body composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::substrate_is_resource`]) so every
    /// downstream (`resource-substrate` fixed tags on both surfaces
    /// in tatara-check, future plane-baseline / compliance-baseline
    /// selectors, future variant additions on
    /// [`crate::classification::SubstrateType`]) binds through the
    /// SAME `substrate_is_resource()` shape rather than restating
    /// either the resolver walk or the closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation
    /// over composition; a future
    /// [`crate::classification::SubstrateType`] variant lands at ONE
    /// `ALL` entry + ONE `is_resource` arm on the closed set and
    /// both surfaces pick it up mechanically.
    #[must_use]
    pub fn substrate_is_resource(&self) -> bool {
        self.resolved_classification().substrate_is_resource()
    }

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s
    /// [`crate::classification::SubstrateType`] project to `true`
    /// under [`crate::classification::SubstrateType::is_policy`]?
    /// Byte-for-byte peer of
    /// [`Classification::substrate_is_policy`] wrapped through the
    /// [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot on `(defephemeral …)` still
    /// answers via the substrate default. The ONE ephemeral-surface
    /// substrate primitive that owns the `(&EphemeralSpec) -> bool`
    /// derived-nullary-boolean walk on the policy-plane bucket
    /// question over the classification-`substrate` axis.
    ///
    /// # Tenth derived-nullary-boolean peer on the ephemeral surface
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`],
    /// [`Self::data_is_regulated`], [`Self::data_is_restricted`],
    /// [`Self::point_is_endomorphic`], [`Self::point_is_diffusive`],
    /// [`Self::point_is_convergent`], and
    /// [`Self::substrate_is_resource`] on the ephemeral surface's
    /// (resolver-hop × derived-nullary-bool) shape — the TENTH peer
    /// overall and the SECOND peer threading the classification-
    /// `substrate` axis, promoting that axis on this surface from a
    /// proven-repeatable one-off to a proven-repeatable pair.
    /// FIRST ephemeral-surface substrate-axis corner-peer pair
    /// carrying a non-trivial closed-set-internal MUTEX relationship
    /// (`substrate_is_resource ⇒ ¬substrate_is_policy`), structural
    /// twin of the sibling `point_type`-axis MUTEX pair sealed on
    /// this surface by
    /// `ephemeral_point_is_endomorphic_and_point_is_diffusive_are_mutex_over_all`.
    /// The resolver-hop shape is byte-identical across all ten peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `substrate_is_policy()` returns `true` iff
    /// `self.resolved_classification().substrate_is_policy()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// [`crate::classification::SubstrateType::Compute`] (the
    /// canonical resource-plane substrate, NOT a policy plane), a
    /// bare ephemeral spec with no `:classification` slot answers
    /// `false` — a regression that dropped the resolver hop, probed
    /// the wrong closed-set arm, or inverted the projection fails
    /// HERE at ONE narrow substrate site before drifting through
    /// every unadorned ephemeral spec's plane-baseline answer.
    ///
    /// # Compounding — second substrate-axis peer on the ephemeral surface
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `policy-substrate` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `policy-substrate` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::substrate_is_policy`] directly. The two-
    /// surface parity contract holds by construction: both surfaces
    /// route through the SAME
    /// [`Classification::substrate_is_policy`] primitive after the
    /// ephemeral surface pays ONE resolver hop. SECOND ephemeral-
    /// surface peer on the `substrate` axis — sibling projection
    /// [`crate::classification::SubstrateType::is_telemetry`]
    /// composes byte-identically as a future eleventh peer, closing
    /// the axis into a proven-repeatable three-peer sub-corner
    /// exactly as the `point_type` axis was closed on this surface
    /// by
    /// `ephemeral_point_type_probes_form_three_way_xor_partition_over_all`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`substrate`-axis derived-
    /// nullary-boolean probe body composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::substrate_is_policy`]) so every
    /// downstream (`policy-substrate` fixed tags on both surfaces
    /// in tatara-check, future plane-baseline / compliance-baseline
    /// selectors, future variant additions on
    /// [`crate::classification::SubstrateType`]) binds through the
    /// SAME `substrate_is_policy()` shape rather than restating
    /// either the resolver walk or the closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation
    /// over composition; a future
    /// [`crate::classification::SubstrateType`] variant lands at ONE
    /// `ALL` entry + ONE `is_policy` arm on the closed set and
    /// both surfaces pick it up mechanically.
    #[must_use]
    pub fn substrate_is_policy(&self) -> bool {
        self.resolved_classification().substrate_is_policy()
    }

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s
    /// [`crate::classification::SubstrateType`] project to `true`
    /// under [`crate::classification::SubstrateType::is_telemetry`]?
    /// Byte-for-byte peer of
    /// [`Classification::substrate_is_telemetry`] wrapped through
    /// the [`Self::resolved_classification`] resolver so an operator-
    /// omitted `:classification` slot on `(defephemeral …)` still
    /// answers via the substrate default. The ONE ephemeral-surface
    /// substrate primitive that owns the `(&EphemeralSpec) -> bool`
    /// derived-nullary-boolean walk on the telemetry-plane bucket
    /// question over the classification-`substrate` axis.
    ///
    /// # Eleventh derived-nullary-boolean peer on the ephemeral surface — CLOSES the substrate axis
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`],
    /// [`Self::data_is_regulated`], [`Self::data_is_restricted`],
    /// [`Self::point_is_endomorphic`], [`Self::point_is_diffusive`],
    /// [`Self::point_is_convergent`], [`Self::substrate_is_resource`],
    /// and [`Self::substrate_is_policy`] on the ephemeral surface's
    /// (resolver-hop × derived-nullary-bool) shape — the ELEVENTH
    /// peer overall and the THIRD peer threading the classification-
    /// `substrate` axis. This peer CLOSES the substrate axis on the
    /// ephemeral surface into the FULL three-way XOR partition
    /// contract `substrate_is_resource ⊕ substrate_is_policy ⊕
    /// substrate_is_telemetry` — sealed on this surface by
    /// `ephemeral_substrate_probes_form_three_way_xor_partition_over_all`,
    /// the resolver-hop peer of the parent-composed
    /// `classification_substrate_probes_form_three_way_xor_partition_over_all`.
    /// Structural twin of the sibling `point_type`-axis ternary lift
    /// sealed on this surface by
    /// `ephemeral_point_type_probes_form_three_way_xor_partition_over_all`.
    /// The resolver-hop shape is byte-identical across all eleven
    /// peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `substrate_is_telemetry()` returns `true` iff
    /// `self.resolved_classification().substrate_is_telemetry()`.
    /// The resolver returns the authored [`Classification`] when
    /// present and the substrate default [`Classification::gate_compute`]
    /// on absence. Because [`Classification::gate_compute`] carries
    /// [`crate::classification::SubstrateType::Compute`] (the
    /// canonical resource-plane substrate, NOT a telemetry plane),
    /// a bare ephemeral spec with no `:classification` slot answers
    /// `false` — a regression that dropped the resolver hop, probed
    /// the wrong closed-set arm, or inverted the projection fails
    /// HERE at ONE narrow substrate site before drifting through
    /// every unadorned ephemeral spec's plane-baseline answer.
    ///
    /// # Compounding — CLOSES the substrate axis on the ephemeral surface
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `telemetry-substrate` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `telemetry-substrate` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::substrate_is_telemetry`] directly. The two-
    /// surface parity contract holds by construction: both surfaces
    /// route through the SAME
    /// [`Classification::substrate_is_telemetry`] primitive after the
    /// ephemeral surface pays ONE resolver hop. THIRD ephemeral-
    /// surface peer on the `substrate` axis — closes the axis into a
    /// proven-repeatable three-peer sub-corner exactly as the
    /// `point_type` axis was closed on this surface by
    /// `ephemeral_point_type_probes_form_three_way_xor_partition_over_all`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`substrate`-axis derived-
    /// nullary-boolean probe body composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::substrate_is_telemetry`]) so every
    /// downstream (`telemetry-substrate` fixed tags on both surfaces
    /// in tatara-check, future plane-baseline / compliance-baseline
    /// selectors, future variant additions on
    /// [`crate::classification::SubstrateType`]) binds through the
    /// SAME `substrate_is_telemetry()` shape rather than restating
    /// either the resolver walk or the closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation
    /// over composition; a future
    /// [`crate::classification::SubstrateType`] variant lands at ONE
    /// `ALL` entry + ONE `is_telemetry` arm on the closed set and
    /// both surfaces pick it up mechanically.
    #[must_use]
    pub fn substrate_is_telemetry(&self) -> bool {
        self.resolved_classification().substrate_is_telemetry()
    }

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s
    /// [`crate::classification::CalmClassification`] project to `true`
    /// under [`crate::classification::CalmClassification::is_monotone`]?
    /// Byte-for-byte peer of [`Classification::calm_is_monotone`]
    /// wrapped through the [`Self::resolved_classification`] resolver
    /// so an operator-omitted `:classification` slot on
    /// `(defephemeral …)` still answers via the substrate default.
    /// The ONE ephemeral-surface substrate primitive that owns the
    /// `(&EphemeralSpec) -> bool` derived-nullary-boolean walk on the
    /// CALM-monotone-plane question — the positive framing peer of
    /// [`Self::calm_requires_coordination`].
    ///
    /// # Twelfth derived-nullary-boolean peer on the ephemeral surface — CLOSES the calm axis
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// and [`Self::substrate_is_telemetry`] on the ephemeral
    /// surface's (resolver-hop × derived-nullary-bool) shape — the
    /// TWELFTH peer overall and the SECOND peer threading the
    /// classification-`calm` axis. This peer CLOSES the calm axis
    /// on the ephemeral surface into the FULL binary XOR partition
    /// contract `calm_is_monotone ⊕ calm_requires_coordination` —
    /// sealed on this surface by
    /// `ephemeral_calm_probes_form_binary_xor_partition_over_all`,
    /// the resolver-hop peer of the parent-composed
    /// `classification_calm_probes_form_binary_xor_partition_over_all`.
    /// Structural twin of the sibling horizon-axis binary XOR
    /// sealed on the closed set by
    /// `horizon_kind_terminate_xor_requires_metric_axes`, lifted
    /// through the resolver hop to the ephemeral surface. The
    /// resolver-hop shape is byte-identical across all twelve peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `calm_is_monotone()` returns `true` iff
    /// `self.resolved_classification().calm_is_monotone()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// [`crate::classification::CalmClassification::default =
    /// Monotone`] via `#[default]`, a bare ephemeral spec with no
    /// `:classification` slot answers `true` — every unadorned
    /// `(defephemeral …)` reads as gossip-eligible under the
    /// positive CALM framing, safe under Hellerstein's theorem
    /// (monotone operations distribute without coordination). A
    /// regression that dropped the resolver hop, probed the wrong
    /// closed-set arm, or inverted the projection fails HERE at ONE
    /// narrow substrate site before drifting through every
    /// unadorned ephemeral spec's positive-CALM-framing answer.
    /// Mirror-inverted from the sibling
    /// `calm_requires_coordination_probes_false_on_absent_classification`
    /// (both walk the SAME defaulted `calm` field, so
    /// `requires_coordination = false` ⇒ `is_monotone = true` on the
    /// closed set's disjoint XOR partition).
    ///
    /// # Compounding — CLOSES the calm axis on the ephemeral surface
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `monotone-calm` on `EPHEMERAL_FIXED_TAG_ARMS`
    /// — byte-for-byte peer of the point surface's `monotone-calm`
    /// fixed tag on `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::calm_is_monotone`] directly. The two-
    /// surface parity contract holds by construction: both surfaces
    /// route through the SAME [`Classification::calm_is_monotone`]
    /// primitive after the ephemeral surface pays ONE resolver hop.
    /// SECOND ephemeral-surface peer on the `calm` axis — CLOSES the
    /// axis into a proven-repeatable two-peer sub-corner exactly as
    /// the `horizon` axis is closed on the closed-set layer by
    /// `horizon_kind_terminate_xor_requires_metric_axes`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`calm`-axis derived-
    /// nullary-boolean probe body composes ONE resolver primitive
    /// ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::calm_is_monotone`]) so every downstream
    /// (`monotone-calm` fixed tags on both surfaces in tatara-check,
    /// future scheduler / gossip-eligibility validators reading the
    /// positive CALM framing, future variant additions on
    /// [`crate::classification::CalmClassification`]) binds through
    /// the SAME `calm_is_monotone()` shape rather than restating
    /// either the resolver walk or the closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation
    /// over composition; a future
    /// [`crate::classification::CalmClassification`] variant lands
    /// at ONE `ALL` entry + ONE `is_monotone` arm on the closed set
    /// and both surfaces pick it up mechanically.
    #[must_use]
    pub fn calm_is_monotone(&self) -> bool {
        self.resolved_classification().calm_is_monotone()
    }

    /// True iff this ephemeral spec's [`Self::routing`] slot is
    /// populated AND the inner [`RoutingSpec`]'s derived
    /// [`RoutingForm`] equals `kind` — the substrate primitive that
    /// owns the (`&EphemeralSpec`, [`RoutingForm`]) → `bool` presence-
    /// probe shape on the sugar-surface type.
    ///
    /// # Peer to [`crate::routing::RoutingSpec::has_form`]
    ///
    /// [`RoutingSpec::has_form`] carries the same `(&self, RoutingForm)
    /// -> bool` signature on the inner routing carrier reached through
    /// the Option gate; this peer composes byte-identical semantics on
    /// [`EphemeralSpec`]'s direct `routing: Option<RoutingSpec>` slot,
    /// so both surfaces' `routing-form-<kind>` require-tag families
    /// route through the SAME `RoutingSpec::has_form` primitive. A
    /// future normalization at the probe shape (a widened return
    /// carrying the derived [`RoutingForm`] variant, a debug-build
    /// assertion on operator-set vs defaulted overrides on the
    /// `stable_name_claim` bool, a fleet-wide warn on `Stable`
    /// combined with content-hashed hostnames) lands at ONE site per
    /// surface and every downstream `routing-form-<kind>` require-tag
    /// family + closed-set audit dispatcher picks it up mechanically.
    ///
    /// # Semantics — Option-gated derived-scalar match
    ///
    /// [`EphemeralSpec::routing`] is an `Option<RoutingSpec>`: `None`
    /// on an in-cluster-only ephemeral env (no per-instance edges
    /// declared), `Some(_)` when the operator authored the
    /// `:routing (…)` slot. `has_routing_form(kind)` returns `true`
    /// iff the slot is `Some(spec)` AND `spec.has_form(kind)` — the
    /// Option-parent gate short-circuits `false` on `None` regardless
    /// of `kind`, and the reachable arm reads the DERIVED
    /// [`RoutingForm`] through the ONE substrate composer
    /// [`RoutingForm::from_is_stable`] over the child
    /// `stable_name_claim` bool (a `false` default projects to
    /// [`RoutingForm::Instance`], a `true` operator override projects
    /// to [`RoutingForm::Stable`]).
    ///
    /// # Corner — (Option-parent × derived-scalar-child)
    ///
    /// SAME corner as the point surface's `routing-form-<kind>`
    /// family (via [`crate::routing::RoutingSpec::has_form`] reached
    /// through `spec.routing.as_ref().is_some_and(|r| r.has_form(k))`)
    /// — both surfaces' Option-parent hop threads through the SAME
    /// `Option<RoutingSpec>` field name on their respective sugar
    /// structs. The [`From<EphemeralSpec>`] lowering copies
    /// `e.routing → ProcessSpec::routing` byte-for-byte at the
    /// [`From`] impl in this module (see the `routing: e.routing`
    /// line), so the SAME `Option<RoutingSpec>` reaches both
    /// surfaces' `routing-form-<kind>` families through the SAME
    /// [`RoutingSpec::has_form`] walk. Distinct from
    /// [`Self::has_teardown_policy`] on this same surface, which
    /// walks a required-scalar-child through no Option-parent hop.
    ///
    /// # Compounding
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// with the closed-set `FromStr` autoderived on [`RoutingForm`]
    /// through the `strip_and_classify_prefixed_kind` substrate to
    /// publish a `routing-form-<kind>` prefix family byte-for-byte
    /// symmetrical with the point surface's family via
    /// [`crate::routing::RoutingSpec::has_form`]. A future third
    /// [`RoutingForm`] variant added to `ALL` (a hypothetical
    /// `Anchored` for "hold the claim only for a specific
    /// generation") reaches BOTH surfaces' `routing-form-<kind>`
    /// prefix families through the SAME closed-set walk with no
    /// per-caller edit — the two-surface symmetry means adding a
    /// variant on the closed set publishes it in lockstep across
    /// every downstream consumer.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the Option-gated derived-scalar-carrier
    /// presence-probe body lives at ONE substrate site per surface
    /// so every downstream (`routing-form-<kind>` require-tag families
    /// on both surfaces in tatara-check, closed-set audit dispatchers,
    /// future variant additions on [`RoutingForm`]) binds through the
    /// SAME `has(kind)` shape rather than restating the
    /// `spec.routing.as_ref().is_some_and(|r| r.has_form(kind))`
    /// closure body at each call site). THEORY.md §VI.1 (generation
    /// over composition — a future variant lands at ONE `ALL` entry +
    /// one `as_str` arm on the closed set and the probe picks it up
    /// mechanically without further per-consumer edits).
    #[must_use]
    pub fn has_routing_form(&self, kind: RoutingForm) -> bool {
        self.routing.as_ref().is_some_and(|r| r.has_form(kind))
    }

    /// True iff at least one declared export in `self.exports` would
    /// fire on the given terminal-reached [`ProcessPhase`] — the peer
    /// of [`crate::lifetime::EphemeralLifetime::has_applicable_exports`]
    /// on the [`EphemeralSpec`] surface.
    ///
    /// # Semantics — byte-identical to [`crate::lifetime::EphemeralLifetime::has_applicable_exports`]
    ///
    /// Both surfaces walk the SAME slice-level substrate primitive
    /// [`ExportSpecSliceExt::has_applicable_at`] on their respective
    /// `Vec<ExportSpec>` slot: [`EphemeralSpec`]'s `exports` field is
    /// copied byte-for-byte into `EphemeralLifetime::exports` at the
    /// `From<EphemeralSpec>` lowering, so a `has_applicable_exports_at`
    /// query on the authored ephemeral spec answers identically to a
    /// `has_applicable_exports` query on the lowered `EphemeralLifetime`.
    /// A regression at the compound `(when, phase) → fires_on(phase)`
    /// walk fails at [`ExportSpecSliceExt::has_applicable_at`]'s tests
    /// rather than as silent drift at either surface's inherent method.
    ///
    /// # Sibling to [`crate::lifetime::EphemeralLifetime::has_applicable_exports`]
    ///
    /// Same shape, same axis, same body — the point-domain surface
    /// composes through `spec.lifetime.resolved_ephemeral().is_some_and(
    /// |e| e.exports.has_applicable_at(phase))`; the ephemeral sugar
    /// surface reads `self.exports.has_applicable_at(phase)` directly
    /// because `EphemeralSpec` stores `exports: Vec<ExportSpec>` as a
    /// top-level field. Both routes bind through THIS ONE slice-level
    /// primitive so a future normalization (widening the trigger from
    /// a stored discriminator to a computed predicate, adding a phase
    /// that composes across multiple trigger arms, threading a
    /// per-export justification back for editor tooltips) lands at ONE
    /// site and every downstream inherits the shift by construction.
    ///
    /// # Compounding
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// with the closed-set [`ProcessPhase`]'s autoderived `FromStr`
    /// through the `strip_and_classify_prefixed_kind` substrate to
    /// publish an `exports-fire-on-<phase>` closed-set prefix family
    /// byte-for-byte symmetrical with the point surface's family via
    /// `spec.lifetime.resolved_ephemeral().is_some_and(|e|
    /// e.exports.has_applicable_at(phase))`. A future twelfth
    /// [`ProcessPhase`] variant reaches BOTH surfaces' prefix families
    /// through the ONE [`crate::export::ExportTrigger::fires_on`]
    /// exhaustive match — either the new phase inherits a per-trigger
    /// fire rule at that single substrate site or it collapses to
    /// `false` for every trigger (the current non-terminal tail),
    /// without a per-caller edit anywhere else.
    ///
    /// A future normalization at the compound `(when, phase) →
    /// fires_on(phase)` walk (a widening that returns the applicable
    /// exports themselves rather than a bool, a debug-build assertion
    /// on redundant `Always`-triggered exports coexisting with an
    /// `OnAttested` peer, a fleet-wide warn on empty-export ephemerals
    /// declaring `OnAttested` postconditions) lands at the ONE
    /// slice-level substrate primitive [`ExportSpecSliceExt::has_applicable_at`]
    /// both this method and [`crate::lifetime::EphemeralLifetime::has_applicable_exports`]
    /// compose against — so the two struct-level union methods stay
    /// symmetric by construction.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — the walk composes the SAME slice-level substrate
    /// primitive on both this ephemeral surface and the
    /// [`crate::lifetime::EphemeralLifetime`] surface, so a regression
    /// at the compound `(when, phase) → fires_on(phase)` chain fails
    /// at ONE site rather than as silent drift between the two peers).
    /// THEORY.md §VI.1 (generation over composition — a future
    /// [`ProcessPhase`] variant or a future [`crate::export::ExportTrigger`]
    /// variant reaches both `exports-fire-on-<phase>` require-tag
    /// surfaces mechanically through the SAME closed-set walk).
    #[must_use]
    pub fn has_applicable_exports_at(&self, phase: ProcessPhase) -> bool {
        self.exports.has_applicable_at(phase)
    }
}

impl From<EphemeralSpec> for ProcessSpec {
    fn from(e: EphemeralSpec) -> Self {
        let classification = e.classification.unwrap_or_else(default_ephemeral_class);
        let mut spec = Self {
            identity: crate::spec::IdentitySpec {
                parent: e.parent,
                name_override: None,
            },
            classification,
            intent: Intent {
                aplicacao: Some(e.aplicacao),
                ..Intent::default()
            },
            boundary: Boundary {
                preconditions: e.preconditions,
                postconditions: e.postconditions,
                timeout: e.verify_timeout,
            },
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            // Routes through the ONE substrate composer
            // [`Lifetime::ephemeral`] — pre-lift this was one of
            // ELEVEN+ hand-authored `Lifetime { ephemeral: Some(<e>),
            // .. }` sites past the ★★ PRIME-DIRECTIVE ≥ 2 threshold.
            // See the composer's doc-comment for the full migration
            // rationale.
            lifetime: Lifetime::ephemeral(EphemeralLifetime {
                ttl: e.ttl,
                teardown_policy: e.teardown,
                max_concurrent: e.max_concurrent,
                exports: e.exports,
            }),
            // R5 — propagate routing template (None = no edges).
            routing: e.routing,
            // EncapsulatesSpec isn't exposed via EphemeralSpec sugar;
            // operators wanting Adopt/Observe author the full
            // (defpoint …) form. Sugar path stays greenfield-Manage.
            encapsulates: None,
            suspended: false,
        };
        // Belt-and-suspenders: make sure exactly-one Intent invariant holds.
        spec.intent.nix = None;
        spec.intent.flux = None;
        spec.intent.lisp = None;
        spec.intent.container = None;
        spec.intent.guest = None;
        spec
    }
}

fn default_ephemeral_class() -> Classification {
    // Delegates through the substrate `(Gate, Compute)` baseline owner
    // so the shape lives at ONE workspace-wide site — see
    // [`Classification::gate_compute`] for the pre-lift ten-callsite
    // duplication history and the sibling-default correspondence
    // pinned there.
    Classification::gate_compute()
}

/// Compile a `(defephemeral …)` Lisp source into named `EphemeralSpec` values.
pub fn compile_ephemeral_source(
    src: &str,
) -> tatara_lisp::Result<Vec<tatara_lisp::NamedDefinition<EphemeralSpec>>> {
    tatara_lisp::compile_named::<EphemeralSpec>(src)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundary::ConditionKind;
    use crate::classification::{
        Arity, CalmClassification, ConvergencePointType, DataClassification, Horizon, HorizonKind,
        OptimizationDirection, SubstrateType,
    };
    use crate::intent::IntentVariant;
    use crate::lifetime::LifetimeVariant;

    fn demo_overlay() -> AplicacaoIntent {
        AplicacaoIntent {
            chart_ref: "oci://ghcr.io/pleme-io/charts/lareira-demo-app".into(),
            version: "0.5.5".into(),
            profile: "all-in-one".into(),
            values_overlay: serde_json::json!({
                "cluster": { "name": "ephemeral-test-01", "namespace": "demo-test" },
                "data": { "mysql": { "persistence": { "enabled": false } } },
                "compliance": { "overlays": [] }
            }),
            release_name: Some("demo-app-consolidated".into()),
            target_namespace: Some("demo-test".into()),
            install_timeout: Some("25m".into()),
        }
    }

    #[test]
    fn defaults_resolve_for_ephemeral_spec() {
        let e = EphemeralSpec {
            aplicacao: demo_overlay(),
            ttl: crate::lifetime::default_ephemeral_ttl(),
            teardown: TeardownPolicy::default(),
            max_concurrent: crate::lifetime::default_ephemeral_max_concurrent(),
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: None,
            exports: vec![],
            routing: None,
        };
        let ps: ProcessSpec = e.into();
        // Intent must resolve to Aplicacao.
        match ps.intent.variant().unwrap() {
            IntentVariant::Aplicacao(a) => {
                assert_eq!(a.profile, "all-in-one");
                assert_eq!(a.install_timeout.as_deref(), Some("25m"));
            }
            other => panic!("expected Aplicacao, got {other:?}"),
        }
        // Lifetime must resolve to Ephemeral with defaults.
        match ps.lifetime.variant().unwrap() {
            LifetimeVariant::Ephemeral(e) => {
                assert_eq!(e.ttl, "1h");
                assert_eq!(e.teardown_policy, TeardownPolicy::Always);
            }
            other => panic!("expected ephemeral, got {other:?}"),
        }
        // Default classification gates the Process at Compute/Internal.
        assert_eq!(ps.classification.point_type, ConvergencePointType::Gate);
        assert_eq!(ps.classification.substrate, SubstrateType::Compute);
    }

    #[test]
    fn ephemeral_lisp_round_trip() {
        let src = r#"
            (defephemeral closed-loop-attest
              :aplicacao (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-demo-app"
                          :version "0.5.5"
                          :profile "all-in-one"
                          :values-overlay (:cluster (:name "ephemeral-test-01")
                                           :data (:mysql (:persistence (:enabled #f)))
                                           :compliance (:overlays []))
                          :release-name "demo-app-consolidated"
                          :target-namespace "demo-test"
                          :install-timeout "25m")
              :ttl "1h"
              :teardown OnAttested
              :max-concurrent 1
              :postconditions
                ((:kind HelmReleaseReleased
                  :params (:name "demo-app-consolidated"
                           :namespace "demo-test"))
                 (:kind ClosedLoopAuth
                  :params (:issuer (:service "demo-app-issuer" :port 8080)
                           :consumer (:service "demo-app-gateway" :port 8000)
                           :probeImage "ghcr.io/pleme-io/closed-loop-probe:0.1.0"))))
        "#;
        let defs = compile_ephemeral_source(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        assert_eq!(d.name, "closed-loop-attest");

        // Aplicacao body landed correctly.
        assert_eq!(
            d.spec.aplicacao.chart_ref,
            "oci://ghcr.io/pleme-io/charts/lareira-demo-app"
        );
        assert_eq!(d.spec.aplicacao.profile, "all-in-one");
        assert_eq!(
            d.spec.aplicacao.target_namespace.as_deref(),
            Some("demo-test")
        );
        // values-overlay JSON is preserved.
        assert_eq!(
            d.spec.aplicacao.values_overlay["cluster"]["name"],
            "ephemeral-test-01"
        );
        // Boolean #f is preserved as a typed JSON bool (not the string "false").
        // tatara-lisp uses Scheme syntax for bools — `#t` / `#f`.
        assert_eq!(
            d.spec.aplicacao.values_overlay["data"]["mysql"]["persistence"]["enabled"],
            false
        );

        // Lifetime knobs.
        assert_eq!(d.spec.ttl, "1h");
        assert_eq!(d.spec.teardown, TeardownPolicy::OnAttested);
        assert_eq!(d.spec.max_concurrent, 1);

        // Two postconditions, both typed.
        assert_eq!(d.spec.postconditions.len(), 2);
        assert_eq!(
            d.spec.postconditions[0].kind,
            ConditionKind::HelmReleaseReleased
        );
        assert_eq!(d.spec.postconditions[1].kind, ConditionKind::ClosedLoopAuth);

        // Lowers to ProcessSpec with the right shape.
        let ps: ProcessSpec = d.spec.clone().into();
        assert!(matches!(
            ps.intent.variant().unwrap(),
            IntentVariant::Aplicacao(_)
        ));
        assert!(matches!(
            ps.lifetime.variant().unwrap(),
            LifetimeVariant::Ephemeral(_)
        ));
        assert_eq!(ps.boundary.postconditions.len(), 2);
    }

    /// End-to-end: the `:exports` slot on `(defephemeral …)` compiles
    /// into typed `ExportSpec` values via the Universal-Deserialize
    /// fallthrough — no per-domain keyword handlers needed.
    ///
    /// Receipts (empty-body source) is exercised via the Rust serde
    /// path only (see `export::tests::export_spec_serde_round_trip`).
    /// tatara-lisp's empty-kw-form `(:)` currently parses as a single-
    /// element array rather than a JSON `{}`; the same limitation
    /// affects `(:permanent)` on Lifetime. Tracked: extend the reader
    /// to accept `(:foo (:))` ⇒ `{"foo": {}}` as a typed-empty form,
    /// then re-enable Receipts here.
    #[test]
    fn exports_lisp_round_trip() {
        use crate::export::{ArtifactVariant, ChannelVariant, ExportTrigger, ReportFormat};
        let src = r#"
            (defephemeral closed-loop-attest
              :aplicacao (:chart-ref "oci://x"
                          :version "1.0.0"
                          :profile "minimal"
                          :values-overlay ())
              :ttl "30m"
              :teardown OnAttested
              :exports
                ((:source  (:test-report (:configmap "junit-results"
                                          :key       "junit.xml"
                                          :format    Junit))
                  :channel (:nats-subject (:subject "pleme.pleme-dev.ephemeral.r1.test-report"
                                           :stream  "EPHEMERAL_TEST_REPORTS"))
                  :when    OnAttested)
                 (:source  (:test-report (:configmap "junit-results"
                                          :key       "junit.xml"
                                          :format    Junit))
                  :channel (:http-event (:signal-type "test-report"))
                  :when    Always)
                 (:source  (:run-marker (:labels (:run-id "r1" :phase "end")))
                  :channel (:http-event (:signal-type "ephemeral-marker"))
                  :when    Always)))
        "#;
        let defs = compile_ephemeral_source(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        assert_eq!(d.spec.exports.len(), 3);

        // First export — TestReport → NATS subject + OnAttested
        let r = &d.spec.exports[0];
        match r.source.variant().unwrap() {
            ArtifactVariant::TestReport(tr) => {
                assert_eq!(tr.configmap, "junit-results");
                assert_eq!(tr.format, ReportFormat::Junit);
            }
            other => panic!("expected TestReport, got {other:?}"),
        }
        match r.channel.variant().unwrap() {
            ChannelVariant::NatsSubject(n) => {
                assert_eq!(n.subject, "pleme.pleme-dev.ephemeral.r1.test-report");
                assert_eq!(n.stream, "EPHEMERAL_TEST_REPORTS");
            }
            other => panic!("expected NatsSubject, got {other:?}"),
        }
        assert_eq!(r.when, ExportTrigger::OnAttested);

        // Second export — TestReport → HTTP + Always
        let t = &d.spec.exports[1];
        match t.channel.variant().unwrap() {
            ChannelVariant::HttpEvent(h) => assert_eq!(h.signal_type, "test-report"),
            other => panic!("expected HttpEvent, got {other:?}"),
        }
        assert_eq!(t.when, ExportTrigger::Always);

        // Third export — RunMarker (BTreeMap<String,String> round-trip).
        // tatara-lisp lowercases + normalizes keyword keys before
        // handing off to serde_json — kebab `:run-id` may land as
        // either `run-id` or `runId` depending on the reader path.
        // Accept either; the round-trip property under test is
        // "label survives compile" not "exact case-form".
        let m = &d.spec.exports[2];
        match m.source.variant().unwrap() {
            ArtifactVariant::RunMarker(rm) => {
                assert_eq!(rm.labels.len(), 2);
                let run_id = rm
                    .labels
                    .get("run-id")
                    .or_else(|| rm.labels.get("runId"))
                    .or_else(|| rm.labels.get("run_id"))
                    .expect("run-id label present under some normalization");
                assert_eq!(run_id, "r1");
                assert_eq!(rm.labels.get("phase").map(String::as_str), Some("end"));
            }
            other => panic!("expected RunMarker, got {other:?}"),
        }

        // Lowered ProcessSpec carries the exports through unchanged.
        let ps: ProcessSpec = d.spec.clone().into();
        assert_eq!(ps.lifetime.ephemeral.as_ref().unwrap().exports.len(), 3);
    }

    // ── EphemeralSpec::has_condition_kind substrate pins ─────────────
    //
    // Fail-before-pass-after granularity:
    // `EphemeralSpec::has_condition_kind` did not exist before this
    // commit — the (preconditions ∪ postconditions .iter().any(|c|
    // c.kind == K)) union-probe shape lived at ONE struct-level site
    // (`Boundary::has_condition_kind` on the point surface's nested
    // [`Boundary`] slot). The lift adds the peer inherent method on the
    // [`EphemeralSpec`] sugar-surface so both struct-level union
    // callers compose against the SAME slice-level substrate primitive
    // [`ConditionSliceExt::has_kind`] in lockstep. A regression that
    // (a) hard-coded the arm to a single kind, (b) dropped the pre-
    // condition side of the OR (a re-inheritance of the pre-lift
    // ephemeral `closed-loop-auth` post-only shape at the union-tag
    // level), or (c) probed the wrong slot fails HERE at the substrate
    // primitive rather than as silent operator-facing drift at the
    // ephemeral `condition-<kind>` require-tag surface.

    fn empty_ephemeral() -> EphemeralSpec {
        EphemeralSpec {
            aplicacao: AplicacaoIntent::chart_only("oci://ghcr.io/x", "1"),
            ttl: "1h".into(),
            teardown: TeardownPolicy::Always,
            max_concurrent: 0,
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: None,
            exports: vec![],
            routing: None,
        }
    }

    fn cond(kind: ConditionKind) -> Condition {
        Condition {
            kind,
            params: serde_json::json!({}),
        }
    }

    /// EMPTY-SPEC pin — a default [`EphemeralSpec`] (empty
    /// preconditions, empty postconditions) returns `false` for EVERY
    /// [`ConditionKind`]. Sweep `ConditionKind::ALL` so a new variant
    /// added without a matching arm in the presence probe surfaces at
    /// rustc's exhaustiveness gate on the ALL literal (arity forced by
    /// `[Self; 8]`) rather than as a silent false-positive at every
    /// downstream `condition-<kind>` ephemeral require-tag callsite.
    /// Byte-for-byte peer of
    /// `has_condition_kind_returns_false_on_empty_boundary_for_every_kind`
    /// on the [`Boundary`] surface.
    #[test]
    fn has_condition_kind_returns_false_on_empty_ephemeral_for_every_kind() {
        let spec = empty_ephemeral();
        for kind in ConditionKind::ALL {
            assert!(
                !spec.has_condition_kind(kind),
                "empty ephemeral spec must return false for {kind:?}",
            );
        }
    }

    /// POSTCONDITION-only pin — an ephemeral spec that carries the
    /// kind on ONLY postconditions returns `true` for that kind,
    /// `false` for every other variant. Sweep the ALL × ALL cross so
    /// a regression that hard-coded the arm to a single kind or
    /// probed the wrong slot fails HERE at the substrate primitive.
    #[test]
    fn has_condition_kind_reads_ephemeral_postconditions_per_kind() {
        for populated in ConditionKind::ALL {
            let mut spec = empty_ephemeral();
            spec.postconditions.push(cond(populated));
            for query in ConditionKind::ALL {
                let expected = query == populated;
                assert_eq!(
                    spec.has_condition_kind(query),
                    expected,
                    "ephemeral postcondition populated={populated:?}: \
                     query {query:?} drifted",
                );
            }
        }
    }

    /// PRECONDITION-only pin — mirrors the postcondition sweep on the
    /// other half of the union. Locks the union semantics on both
    /// halves separately so a regression that dropped the pre-
    /// condition side of the OR fails here even though the
    /// postcondition-side pin above passes.
    #[test]
    fn has_condition_kind_reads_ephemeral_preconditions_per_kind() {
        for populated in ConditionKind::ALL {
            let mut spec = empty_ephemeral();
            spec.preconditions.push(cond(populated));
            for query in ConditionKind::ALL {
                let expected = query == populated;
                assert_eq!(
                    spec.has_condition_kind(query),
                    expected,
                    "ephemeral precondition populated={populated:?}: \
                     query {query:?} drifted",
                );
            }
        }
    }

    /// UNION pin — a kind that appears on preconditions returns
    /// `true` even when postconditions carries a DIFFERENT kind, and
    /// vice versa. Pins the OR-composition of the two halves so a
    /// regression that collapsed the union to an intersection (AND)
    /// silently reclassifies pre-only or post-only kinds as absent.
    /// Byte-for-byte peer of
    /// `has_condition_kind_unions_pre_and_post_condition_arms` on the
    /// [`Boundary`] surface.
    #[test]
    fn has_condition_kind_unions_pre_and_post_ephemeral_condition_arms() {
        let mut spec = empty_ephemeral();
        spec.preconditions
            .push(cond(ConditionKind::KustomizationHealthy));
        spec.postconditions
            .push(cond(ConditionKind::ClosedLoopAuth));
        assert!(
            spec.has_condition_kind(ConditionKind::KustomizationHealthy),
            "pre-only kind must resolve through the union",
        );
        assert!(
            spec.has_condition_kind(ConditionKind::ClosedLoopAuth),
            "post-only kind must resolve through the union",
        );
        assert!(
            !spec.has_condition_kind(ConditionKind::PromQL),
            "an absent kind must return false even with populated halves",
        );
    }

    /// COMPOSITION pin — [`EphemeralSpec::has_condition_kind`] equals
    /// the OR of the two slice-level probes on the pre/post fields.
    /// The struct-level union body composes ONLY [`ConditionSliceExt::has_kind`]
    /// on each half; a regression that inlined a wide-net predicate
    /// (`.iter().any(|c| c.kind != kind).not()`, an `all` instead of
    /// `any`) drifts from the slice-level primitive here. Byte-for-
    /// byte peer of the
    /// `boundary_has_condition_kind_equals_or_of_half_slice_probes`
    /// composition pin on the [`Boundary`] surface.
    #[test]
    fn ephemeral_has_condition_kind_equals_or_of_half_slice_probes() {
        // Sweep every ConditionKind on both halves independently so the
        // cross of half-slice probes reaches the OR-composition body
        // exhaustively.
        for populated in ConditionKind::ALL {
            let mut spec = empty_ephemeral();
            spec.preconditions.push(cond(populated));
            spec.postconditions.push(cond(ConditionKind::PromQL));
            for query in ConditionKind::ALL {
                let via_or_of_halves =
                    spec.preconditions.has_kind(query) || spec.postconditions.has_kind(query);
                assert_eq!(
                    spec.has_condition_kind(query),
                    via_or_of_halves,
                    "populated={populated:?} query={query:?}: struct-level \
                     union drifted from OR of slice-level probes",
                );
            }
        }
    }

    #[test]
    fn from_impl_clears_other_intent_variants() {
        // Even if someone constructs an EphemeralSpec by hand and the
        // resulting ProcessSpec is later mutated, the From bridge sets
        // every non-Aplicacao slot to None explicitly.
        let e = EphemeralSpec {
            aplicacao: demo_overlay(),
            ttl: "10m".into(),
            teardown: TeardownPolicy::Never,
            max_concurrent: 0,
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: Some("seph.1".into()),
            exports: vec![],
            routing: None,
        };
        let ps: ProcessSpec = e.into();
        assert!(ps.intent.nix.is_none());
        assert!(ps.intent.flux.is_none());
        assert!(ps.intent.lisp.is_none());
        assert!(ps.intent.container.is_none());
        assert!(ps.intent.guest.is_none());
        assert!(ps.intent.aplicacao.is_some());
        assert_eq!(ps.identity.parent.as_deref(), Some("seph.1"));
    }

    // ── EphemeralSpec::has_teardown_policy substrate pins ────────────
    //
    // Fail-before-pass-after granularity:
    // `EphemeralSpec::has_teardown_policy` did not exist before this
    // commit — the (`self.teardown == kind`) scalar-carrier probe on
    // the sugar-surface [`EphemeralSpec`] lived only implicitly via
    // hand-authored comparisons at potential future call sites, with
    // no analogue to the peer
    // [`crate::lifetime::EphemeralLifetime::has_teardown_policy`] on
    // the point-surface carrier. The lift adds the peer inherent
    // method on the [`EphemeralSpec`] sugar-surface so both surfaces'
    // `teardown-policy-<kind>` require-tag families in
    // `tatara-reconciler::bin::tatara-check` compose against the SAME
    // scalar `==` shape in lockstep. A regression that (a) hard-coded
    // the arm to a single kind, (b) inverted the closed-set match
    // (silently returning `true` on non-matching variants), or (c)
    // probed the wrong slot (a stray comparison against `ttl` /
    // `max_concurrent`) fails HERE at the substrate primitive rather
    // than as silent operator-facing drift at the ephemeral
    // `teardown-policy-<kind>` require-tag surface.

    /// STORED-slot pin — an ephemeral spec that carries a given
    /// [`TeardownPolicy`] returns `true` for that kind, `false` for
    /// every other variant. Sweep the [`TeardownPolicy::ALL`] × ALL
    /// cross so a regression that hard-coded the arm to a single kind
    /// or wired the closure to a fixed unrelated field fails HERE at
    /// the substrate primitive. Byte-for-byte peer of
    /// [`crate::lifetime::tests::ephemeral_lifetime_has_teardown_policy_returns_true_iff_variant_matches`]
    /// on the point-surface [`crate::lifetime::EphemeralLifetime`]
    /// carrier — the two surfaces publish identical `==` scalar
    /// semantics on their respective `teardown` / `teardown_policy`
    /// slots.
    #[test]
    fn has_teardown_policy_returns_true_iff_ephemeral_teardown_matches_per_kind() {
        for populated in TeardownPolicy::ALL {
            let mut spec = empty_ephemeral();
            spec.teardown = populated;
            for query in TeardownPolicy::ALL {
                let expected = query == populated;
                assert_eq!(
                    spec.has_teardown_policy(query),
                    expected,
                    "ephemeral teardown={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// DEFAULT-SLOT pin — an [`EphemeralSpec`] whose `teardown` slot
    /// is [`TeardownPolicy::default`] (`Always`) returns `true` for
    /// `Always` and `false` for every other variant. The
    /// (required-scalar-child) corner has no absent state — a
    /// hand-authored spec that omits `:teardown` from the
    /// `(defephemeral …)` form IS configured for `Always`, and this
    /// pin locks the corner's default-arm short-circuit as identical
    /// to the (Option-parent × defaulted-scalar-child) corner's
    /// reachable arm on the point surface (both return `true` on
    /// `Always` only). Byte-for-byte peer of
    /// [`crate::lifetime::tests::ephemeral_lifetime_has_teardown_policy_default_probes_always_only`]
    /// on the point-surface carrier.
    #[test]
    fn has_teardown_policy_default_probes_always_only_on_ephemeral() {
        let spec = EphemeralSpec {
            teardown: TeardownPolicy::default(),
            ..empty_ephemeral()
        };
        for kind in TeardownPolicy::ALL {
            let expected = kind == TeardownPolicy::Always;
            assert_eq!(
                spec.has_teardown_policy(kind),
                expected,
                "default ephemeral (teardown=Always) baseline: query {kind:?} must be {expected}",
            );
        }
    }

    // ── derived-bool-predicate presence probe on EphemeralSpec ×
    //    TeardownPolicy × ProcessPhase ──
    //
    // Fail-before-pass-after granularity:
    // [`EphemeralSpec::has_teardown_firing_on`] did not exist before
    // this commit — the ephemeral sugar surface's require-tag algebra
    // discriminated the teardown axis only by the RAW authored variant
    // (via `teardown-policy-<kind>`), never by the derived
    // [`ProcessPhase`] transition the stored policy fires on
    // ([`TeardownPolicy::should_teardown_on`]). Post-lift the shape
    // lives at ONE inherent method that byte-for-byte parallels
    // [`crate::lifetime::EphemeralLifetime::has_teardown_firing_on`]
    // on the point-surface carrier, and both surfaces' require-tag
    // classifiers publish a symmetric `teardown-fires-on-<phase>`
    // family through the SAME predicate.

    /// TRUTH-TABLE DIAGONAL — for every [`TeardownPolicy`] variant,
    /// an [`EphemeralSpec`] whose `teardown` slot is set to that
    /// variant returns `has_teardown_firing_on(phase)` in agreement
    /// with [`TeardownPolicy::should_teardown_on`] for every
    /// [`ProcessPhase`] variant. Sweep [`TeardownPolicy::ALL`] ×
    /// [`ProcessPhase::ALL`] full cross so a regression that hard-
    /// coded the arm to a single policy, wired to the wrong field, or
    /// inverted the predicate direction fails HERE at the substrate
    /// primitive on the sugar surface (byte-for-byte peer of
    /// [`crate::lifetime::tests::ephemeral_lifetime_has_teardown_firing_on_matches_should_teardown_on_per_policy_per_phase`]
    /// on the point carrier).
    #[test]
    fn has_teardown_firing_on_matches_should_teardown_on_per_policy_per_phase_on_ephemeral() {
        for populated in TeardownPolicy::ALL {
            let spec = EphemeralSpec {
                teardown: populated,
                ..empty_ephemeral()
            };
            for phase in ProcessPhase::ALL {
                assert_eq!(
                    spec.has_teardown_firing_on(phase),
                    populated.should_teardown_on(phase),
                    "teardown={populated:?}, phase={phase:?}: predicate drift from \
                     should_teardown_on projection",
                );
            }
        }
    }

    /// TWO-SURFACE PARITY PIN — for every [`TeardownPolicy`] variant
    /// and every [`ProcessPhase`] variant, the sugar-surface probe
    /// and the lowered point-surface probe agree. The `EphemeralSpec
    /// → ProcessSpec` lowering routes the stored `teardown` slot
    /// through the SAME [`TeardownPolicy::should_teardown_on`]
    /// projection on both sides, so the sugar caller and the lowered
    /// caller can never disagree — a regression that (a) drifted
    /// [`Self::teardown`] between sugar and lowered, (b) rewired
    /// either probe body to bypass the shared substrate primitive, or
    /// (c) skewed the (policy, phase) truth table between the two
    /// surfaces fails HERE at the two-surface boundary rather than at
    /// the operator-facing require-tag classifier.
    #[test]
    fn has_teardown_firing_on_matches_point_peer_through_lowered_teardown_policy() {
        for populated in TeardownPolicy::ALL {
            let sugar = EphemeralSpec {
                teardown: populated,
                ..empty_ephemeral()
            };
            let lowered: ProcessSpec = sugar.clone().into();
            let lowered_eph = lowered
                .lifetime
                .resolved_ephemeral()
                .expect("lowered spec must be ephemeral");
            for phase in ProcessPhase::ALL {
                assert_eq!(
                    sugar.has_teardown_firing_on(phase),
                    lowered_eph.has_teardown_firing_on(phase),
                    "sugar-vs-lowered predicate drift for teardown={populated:?}, phase={phase:?}",
                );
            }
        }
    }

    // ── EphemeralSpec::resolved_classification + has_point_type pins ─────
    //
    // Fail-before-pass-after granularity: `resolved_classification` and
    // `has_point_type` did not exist pre-lift on `impl EphemeralSpec` — every
    // caller wanting the resolved [`Classification`] on the ephemeral
    // sugar-surface (currently zero; future ephemeral-surface classification-
    // axis require-tag families in `tatara-reconciler::bin::tatara-check`,
    // typed audit hooks, documentation generators listing the ephemeral
    // surface's known require-tag vocabulary) restated the two-line
    // `self.classification.as_ref().unwrap_or(&default_ephemeral_class())`
    // resolver body at their site. Post-lift both callers of the resolver
    // (`Self::has_point_type` and every future classification-axis peer)
    // route through ONE inherent method that shares the fill-through with
    // the sibling `From<EphemeralSpec> for ProcessSpec` lowering
    // byte-for-byte. A regression that (a) inverted the arm (`Some` filled
    // through the default), (b) drifted the default from the sibling
    // primitive `Classification::gate_compute()`, or (c) shifted the
    // `Cow<'_, Classification>` return shape (a stray `.clone()` on the
    // populated arm) fails HERE at the substrate primitive rather than as
    // silent operator-facing drift at a future
    // `point-type-<kind>` ephemeral require-tag surface.

    /// AUTHORED-slot pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot names a concrete
    /// [`Classification`] returns [`Cow::Borrowed`] pointing at that
    /// authored value from [`Self::resolved_classification`]. Pins the
    /// populated-arm zero-allocation contract: a caller reading past
    /// the resolver sees the SAME byte address the operator authored,
    /// so the resolver does not silently clone the authored slot on
    /// the populated arm.
    #[test]
    fn resolved_classification_borrows_authored_slot() {
        let mut spec = empty_ephemeral();
        let mut authored = Classification::gate_compute();
        authored.point_type = ConvergencePointType::Fork;
        spec.classification = Some(authored.clone());
        let resolved = spec.resolved_classification();
        assert!(matches!(resolved, Cow::Borrowed(_)));
        assert_eq!(&*resolved, &authored);
    }

    /// ABSENT-slot pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot is `None` returns
    /// [`Cow::Owned`] with the SAME value the sibling
    /// [`default_ephemeral_class`] baseline produces. Pins the
    /// two-surface parity contract with `From<EphemeralSpec> for
    /// ProcessSpec`: both sites fill through the SAME baseline on
    /// `None`, so the ephemeral require-tag surface's future
    /// `point-type-<kind>` family reads identically on the authored
    /// spec and on the mechanically lowered `ProcessSpec`.
    #[test]
    fn resolved_classification_fills_default_on_absent_slot() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        let resolved = spec.resolved_classification();
        assert!(matches!(resolved, Cow::Owned(_)));
        assert_eq!(&*resolved, &default_ephemeral_class());
    }

    /// AUTHORED-slot VARIANT-MATCH pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot names a concrete
    /// [`Classification`] returns `true` from
    /// [`Self::has_point_type`] on the authored
    /// [`ConvergencePointType`] slot and `false` for every other
    /// variant. Sweep the [`ConvergencePointType::ALL`] × ALL cross so
    /// a regression that hard-coded the arm to a single kind or wired
    /// the closure to a fixed unrelated slot fails HERE at the
    /// substrate primitive. Byte-for-byte peer of
    /// [`crate::classification::tests`]'s point-surface
    /// [`Classification::has_point_type`] populated-slot sweep on the
    /// SAME closed-set primitive.
    #[test]
    fn has_point_type_returns_true_iff_authored_classification_matches_per_kind() {
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            for query in ConvergencePointType::ALL {
                let expected = query == populated;
                assert_eq!(
                    spec.has_point_type(query),
                    expected,
                    "ephemeral classification.point_type={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// ABSENT-slot DEFAULT-ARM pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot is `None` returns
    /// `true` from [`Self::has_point_type`] on
    /// [`ConvergencePointType::Gate`] (the `default_ephemeral_class`
    /// baseline's `point_type`) and `false` on every other variant.
    /// Pins the (Option-parent × NON-DEFAULT-scalar-child) corner's
    /// default-arm short-circuit: on the ephemeral sugar surface the
    /// parent Option is filled through the workspace baseline rather
    /// than reading `false` on every variant like the encapsulation-
    /// mode / encapsulation-target / routing-form Option-parent
    /// corners.
    #[test]
    fn has_point_type_probes_gate_only_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        for kind in ConvergencePointType::ALL {
            let expected = kind == ConvergencePointType::Gate;
            assert_eq!(
                spec.has_point_type(kind),
                expected,
                "absent classification (defaults to gate_compute): query {kind:?} must be {expected}",
            );
        }
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::has_point_type`] AND through
    /// `<eph.clone().into::<ProcessSpec>>()`
    /// `.classification.has_point_type(kind)` on the mechanically-
    /// lowered `ProcessSpec`. Sweeps (`None` classification, `Some(_)`
    /// classification on every [`ConvergencePointType::ALL`] variant)
    /// × ALL queries so a future regression on either side of the
    /// resolver (a shift in the ephemeral resolver's default, a
    /// shift in the `From<EphemeralSpec>` lowering's fill-through)
    /// fails HERE at the parity boundary.
    #[test]
    fn has_point_type_matches_point_peer_through_lowered_classification() {
        // Absent classification: both surfaces resolve through the SAME
        // default and agree on every variant.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        for query in ConvergencePointType::ALL {
            assert_eq!(
                eph.has_point_type(query),
                lowered.classification.has_point_type(query),
                "None-classification parity drift on query {query:?}",
            );
        }
        // Authored classification: both surfaces read the same authored
        // value verbatim.
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            for query in ConvergencePointType::ALL {
                assert_eq!(
                    eph.has_point_type(query),
                    lowered.classification.has_point_type(query),
                    "authored classification.point_type={populated:?}: parity drift on query {query:?}",
                );
            }
        }
    }

    // ── EphemeralSpec::has_substrate pins ────────────────────────────
    //
    // Fail-before-pass-after granularity: [`Self::has_substrate`] did
    // not exist pre-lift on `impl EphemeralSpec` — every callsite went
    // through `.resolved_classification().substrate == kind` or through
    // the lowered `ProcessSpec`'s `spec.classification.has_substrate`.
    // Post-lift the SECOND classification-axis peer on the ephemeral
    // sugar surface routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling closed-
    // set primitive [`Classification::has_substrate`], so a regression
    // that dropped the resolver hop, inverted the `Some`/`None`
    // fill-through, or wired the closure to a fixed unrelated slot
    // fails HERE.

    /// AUTHORED-slot VARIANT-MATCH pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot names a concrete
    /// [`Classification`] returns `true` from
    /// [`Self::has_substrate`] on the authored [`SubstrateType`] slot
    /// and `false` for every other variant. Sweep the
    /// [`SubstrateType::ALL`] × ALL cross so a regression that
    /// hard-coded the arm to a single kind or wired the closure to a
    /// fixed unrelated slot fails HERE at the substrate primitive.
    /// Byte-for-byte peer of the point-surface
    /// [`Classification::has_substrate`] populated-slot sweep on the
    /// SAME closed-set primitive.
    #[test]
    fn has_substrate_returns_true_iff_authored_classification_matches_per_kind() {
        for populated in SubstrateType::ALL {
            let mut classification = Classification::gate_compute();
            classification.substrate = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            for query in SubstrateType::ALL {
                let expected = query == populated;
                assert_eq!(
                    spec.has_substrate(query),
                    expected,
                    "ephemeral classification.substrate={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// ABSENT-slot DEFAULT-ARM pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot is `None` returns
    /// `true` from [`Self::has_substrate`] on
    /// [`SubstrateType::Compute`] (the `default_ephemeral_class`
    /// baseline's `substrate`) and `false` on every other variant.
    /// Pins the (Option-parent × NON-DEFAULT-scalar-child) corner's
    /// default-arm short-circuit on the SECOND classification-axis
    /// peer: on the ephemeral sugar surface the parent Option is
    /// filled through the workspace baseline rather than reading
    /// `false` on every variant like the Option-parent encapsulates /
    /// routing corners.
    #[test]
    fn has_substrate_probes_compute_only_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        for kind in SubstrateType::ALL {
            let expected = kind == SubstrateType::Compute;
            assert_eq!(
                spec.has_substrate(kind),
                expected,
                "absent classification (defaults to gate_compute): query {kind:?} must be {expected}",
            );
        }
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::has_substrate`] AND through
    /// `<eph.clone().into::<ProcessSpec>>()`
    /// `.classification.has_substrate(kind)` on the mechanically-
    /// lowered `ProcessSpec`. Sweeps (`None` classification, `Some(_)`
    /// classification on every [`SubstrateType::ALL`] variant) × ALL
    /// queries so a future regression on either side of the resolver
    /// (a shift in the ephemeral resolver's default, a shift in the
    /// `From<EphemeralSpec>` lowering's fill-through) fails HERE at
    /// the parity boundary. Byte-for-byte peer of the sibling
    /// [`Self::has_point_type`] two-surface parity pin on the SAME
    /// `Cow`-resolver carrier — the SECOND classification-axis
    /// two-surface parity contract on the ephemeral surface.
    #[test]
    fn has_substrate_matches_point_peer_through_lowered_classification() {
        // Absent classification: both surfaces resolve through the SAME
        // default and agree on every variant.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        for query in SubstrateType::ALL {
            assert_eq!(
                eph.has_substrate(query),
                lowered.classification.has_substrate(query),
                "None-classification parity drift on query {query:?}",
            );
        }
        // Authored classification: both surfaces read the same authored
        // value verbatim.
        for populated in SubstrateType::ALL {
            let mut classification = Classification::gate_compute();
            classification.substrate = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            for query in SubstrateType::ALL {
                assert_eq!(
                    eph.has_substrate(query),
                    lowered.classification.has_substrate(query),
                    "authored classification.substrate={populated:?}: parity drift on query {query:?}",
                );
            }
        }
    }

    // ── EphemeralSpec::has_calm pins ─────────────────────────────────
    //
    // Fail-before-pass-after granularity: [`Self::has_calm`] did not
    // exist pre-lift on `impl EphemeralSpec` — every callsite went
    // through `.resolved_classification().calm == kind` or through the
    // lowered `ProcessSpec`'s `spec.classification.has_calm`. Post-
    // lift the THIRD classification-axis peer on the ephemeral sugar
    // surface routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling closed-
    // set primitive [`Classification::has_calm`], so a regression that
    // dropped the resolver hop, inverted the `Some`/`None` fill-
    // through, or wired the closure to a fixed unrelated slot fails
    // HERE. Distinct from the FIRST + SECOND peers on the (Option-
    // parent × NON-DEFAULT-scalar-child) corner: the (Option-parent ×
    // DEFAULTED-scalar-child) corner this peer opens has BOTH the
    // parent fill-through baseline (`default_ephemeral_class`) AND the
    // child's own `#[default]` land on the SAME variant
    // ([`CalmClassification::Monotone`]), a two-defaults composition
    // property the three pins below all exercise.

    /// AUTHORED-slot VARIANT-MATCH pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot names a concrete
    /// [`Classification`] returns `true` from [`Self::has_calm`] on
    /// the authored [`CalmClassification`] slot and `false` for every
    /// other variant. Sweep the [`CalmClassification::ALL`] × ALL
    /// cross so a regression that hard-coded the arm to a single
    /// kind or wired the closure to a fixed unrelated slot fails HERE
    /// at the substrate primitive. Byte-for-byte peer of the point-
    /// surface [`Classification::has_calm`] populated-slot sweep on
    /// the SAME closed-set primitive.
    #[test]
    fn has_calm_returns_true_iff_authored_classification_matches_per_kind() {
        for populated in CalmClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.calm = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            for query in CalmClassification::ALL {
                let expected = query == populated;
                assert_eq!(
                    spec.has_calm(query),
                    expected,
                    "ephemeral classification.calm={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// ABSENT-slot DEFAULT-ARM pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot is `None` returns
    /// `true` from [`Self::has_calm`] on
    /// [`CalmClassification::Monotone`] (the `default_ephemeral_class`
    /// baseline's `calm` axis AND the [`CalmClassification`] child's
    /// own `#[default]` variant) and `false` on every other variant.
    /// Pins the (Option-parent × DEFAULTED-scalar-child ×
    /// operator-resolvable-baseline) corner's default-arm short-
    /// circuit on the THIRD classification-axis peer — distinct from
    /// the FIRST + SECOND peers on the (Option-parent × NON-DEFAULT-
    /// scalar-child) corner which default through a specific chosen
    /// baseline ([`ConvergencePointType::Gate`],
    /// [`SubstrateType::Compute`]) rather than through the child's
    /// own `#[default]`. Two-defaults composition property: both the
    /// parent fill-through and the child's `#[default]` land on the
    /// SAME variant, so the ephemeral sugar surface's `calm-Monotone`
    /// require-tag reads `true` on every operator-authored spec that
    /// omits both the `:classification` slot AND the `:calm` sub-slot,
    /// pinning the workspace's monotone-by-default posture.
    #[test]
    fn has_calm_probes_monotone_only_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        for kind in CalmClassification::ALL {
            let expected = kind == CalmClassification::Monotone;
            assert_eq!(
                spec.has_calm(kind),
                expected,
                "absent classification (defaults to gate_compute): query {kind:?} must be {expected}",
            );
        }
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::has_calm`] AND through
    /// `<eph.clone().into::<ProcessSpec>>()`
    /// `.classification.has_calm(kind)` on the mechanically-
    /// lowered `ProcessSpec`. Sweeps (`None` classification, `Some(_)`
    /// classification on every [`CalmClassification::ALL`] variant) ×
    /// ALL queries so a future regression on either side of the
    /// resolver (a shift in the ephemeral resolver's default, a shift
    /// in the `From<EphemeralSpec>` lowering's fill-through) fails
    /// HERE at the parity boundary. Byte-for-byte peer of the sibling
    /// [`Self::has_point_type`] + [`Self::has_substrate`] two-surface
    /// parity pins on the SAME `Cow`-resolver carrier — the THIRD
    /// classification-axis two-surface parity contract on the
    /// ephemeral surface, and the FIRST on the (Option-parent ×
    /// DEFAULTED-scalar-child) corner.
    #[test]
    fn has_calm_matches_point_peer_through_lowered_classification() {
        // Absent classification: both surfaces resolve through the SAME
        // default and agree on every variant.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        for query in CalmClassification::ALL {
            assert_eq!(
                eph.has_calm(query),
                lowered.classification.has_calm(query),
                "None-classification parity drift on query {query:?}",
            );
        }
        // Authored classification: both surfaces read the same authored
        // value verbatim.
        for populated in CalmClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.calm = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            for query in CalmClassification::ALL {
                assert_eq!(
                    eph.has_calm(query),
                    lowered.classification.has_calm(query),
                    "authored classification.calm={populated:?}: parity drift on query {query:?}",
                );
            }
        }
    }

    // ── EphemeralSpec::has_data_classification pins ──────────────────
    //
    // Fail-before-pass-after granularity: [`Self::has_data_classification`]
    // did not exist pre-lift on `impl EphemeralSpec` — every callsite
    // went through `.resolved_classification().data_classification ==
    // kind` or through the lowered `ProcessSpec`'s
    // `spec.classification.has_data_classification`. Post-lift the
    // FOURTH classification-axis peer on the ephemeral sugar surface
    // routes through the SAME [`Self::resolved_classification`]
    // resolver + the sibling closed-set primitive
    // [`crate::classification::Classification::has_data_classification`],
    // so a regression that dropped the resolver hop, inverted the
    // `Some`/`None` fill-through, or wired the closure to a fixed
    // unrelated slot fails HERE. SECOND occupant on the (Option-parent
    // × DEFAULTED-scalar-child × operator-resolvable-baseline) corner
    // alongside [`Self::has_calm`]: both the parent fill-through
    // baseline (`default_ephemeral_class`) AND the child's own
    // `#[default]` land on the SAME variant
    // ([`DataClassification::Internal`]), a two-defaults composition
    // property the three pins below all exercise.

    /// AUTHORED-slot VARIANT-MATCH pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot names a concrete
    /// [`Classification`] returns `true` from
    /// [`Self::has_data_classification`] on the authored
    /// [`DataClassification`] slot and `false` for every other
    /// variant. Sweep the [`DataClassification::ALL`] × ALL cross so
    /// a regression that hard-coded the arm to a single kind or
    /// wired the closure to a fixed unrelated slot fails HERE at the
    /// substrate primitive. Byte-for-byte peer of the point-surface
    /// [`Classification::has_data_classification`] populated-slot
    /// sweep on the SAME closed-set primitive.
    #[test]
    fn has_data_classification_returns_true_iff_authored_classification_matches_per_kind() {
        for populated in DataClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.data_classification = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            for query in DataClassification::ALL {
                let expected = query == populated;
                assert_eq!(
                    spec.has_data_classification(query),
                    expected,
                    "ephemeral classification.data_classification={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// ABSENT-slot DEFAULT-ARM pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot is `None` returns
    /// `true` from [`Self::has_data_classification`] on
    /// [`DataClassification::Internal`] (the `default_ephemeral_class`
    /// baseline's `data_classification` axis AND the
    /// [`DataClassification`] child's own `#[default]` variant) and
    /// `false` on every other variant. Pins the (Option-parent ×
    /// DEFAULTED-scalar-child × operator-resolvable-baseline) corner's
    /// default-arm short-circuit on the FOURTH classification-axis
    /// peer — SECOND occupant on that corner after [`Self::has_calm`]
    /// opened it. Two-defaults composition property: both the parent
    /// fill-through and the child's `#[default]` land on the SAME
    /// variant, so the ephemeral sugar surface's
    /// `data-classification-Internal` require-tag reads `true` on
    /// every operator-authored spec that omits both the
    /// `:classification` slot AND the `:data-classification` sub-slot,
    /// pinning the workspace's internal-by-default sensitivity posture.
    #[test]
    fn has_data_classification_probes_internal_only_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        for kind in DataClassification::ALL {
            let expected = kind == DataClassification::Internal;
            assert_eq!(
                spec.has_data_classification(kind),
                expected,
                "absent classification (defaults to gate_compute): query {kind:?} must be {expected}",
            );
        }
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::has_data_classification`] AND
    /// through `<eph.clone().into::<ProcessSpec>>()`
    /// `.classification.has_data_classification(kind)` on the
    /// mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`DataClassification::ALL`] variant) × ALL queries so a
    /// future regression on either side of the resolver (a shift in
    /// the ephemeral resolver's default, a shift in the
    /// `From<EphemeralSpec>` lowering's fill-through) fails HERE at
    /// the parity boundary. Byte-for-byte peer of the sibling
    /// [`Self::has_point_type`] + [`Self::has_substrate`] +
    /// [`Self::has_calm`] two-surface parity pins on the SAME
    /// `Cow`-resolver carrier — the FOURTH classification-axis
    /// two-surface parity contract on the ephemeral surface, and the
    /// SECOND on the (Option-parent × DEFAULTED-scalar-child) corner.
    #[test]
    fn has_data_classification_matches_point_peer_through_lowered_classification() {
        // Absent classification: both surfaces resolve through the SAME
        // default and agree on every variant.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        for query in DataClassification::ALL {
            assert_eq!(
                eph.has_data_classification(query),
                lowered.classification.has_data_classification(query),
                "None-classification parity drift on query {query:?}",
            );
        }
        // Authored classification: both surfaces read the same authored
        // value verbatim.
        for populated in DataClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.data_classification = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            for query in DataClassification::ALL {
                assert_eq!(
                    eph.has_data_classification(query),
                    lowered.classification.has_data_classification(query),
                    "authored classification.data_classification={populated:?}: parity drift on query {query:?}",
                );
            }
        }
    }

    // ── EphemeralSpec::has_horizon_kind pins ─────────────────────────
    //
    // Fail-before-pass-after granularity: [`Self::has_horizon_kind`]
    // did not exist pre-lift on `impl EphemeralSpec` — every callsite
    // went through `.resolved_classification().horizon.kind == kind`
    // or through the lowered `ProcessSpec`'s
    // `spec.classification.has_horizon_kind`. Post-lift the FIFTH
    // classification-axis peer on the ephemeral sugar surface routes
    // through the SAME [`Self::resolved_classification`] resolver +
    // the sibling closed-set primitive
    // [`crate::classification::Classification::has_horizon_kind`], so
    // a regression that dropped the resolver hop, inverted the
    // `Some`/`None` fill-through, or wired the closure to a fixed
    // unrelated slot fails HERE. OPENS a fresh (Option-parent ×
    // NESTED-STRUCT-scalar-child × operator-resolvable-baseline)
    // corner on the ephemeral surface — distinct from the four prior
    // scalar-carrier peers on the (Option-parent × NON-DEFAULT-scalar-
    // child) and (Option-parent × DEFAULTED-scalar-child) corners, all
    // of which reach a discriminator DIRECTLY off a scalar
    // [`Classification`] slot. Both the parent Option's fill-through
    // baseline (`default_ephemeral_class`, which fills
    // `horizon: Horizon::default()`) AND the child's own `#[default]`
    // land on the SAME variant ([`HorizonKind::Bounded`]) — a two-
    // defaults composition property the three pins below all
    // exercise.

    /// AUTHORED-slot VARIANT-MATCH pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot names a concrete
    /// [`Classification`] returns `true` from
    /// [`Self::has_horizon_kind`] on the authored [`HorizonKind`] slot
    /// and `false` for every other variant. Sweep the
    /// [`HorizonKind::ALL`] × ALL cross so a regression that hard-
    /// coded the arm to a single kind or wired the closure to a
    /// fixed unrelated slot (e.g. reading `self.classification` as if
    /// it were a scalar rather than routing through
    /// `resolved_classification().horizon.kind`) fails HERE at the
    /// substrate primitive. Byte-for-byte peer of the point-surface
    /// [`Classification::has_horizon_kind`] populated-slot sweep on
    /// the SAME closed-set primitive.
    #[test]
    fn has_horizon_kind_returns_true_iff_authored_classification_matches_per_kind() {
        for populated in HorizonKind::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon = Horizon {
                kind: populated,
                ..Horizon::default()
            };
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            for query in HorizonKind::ALL {
                let expected = query == populated;
                assert_eq!(
                    spec.has_horizon_kind(query),
                    expected,
                    "ephemeral classification.horizon.kind={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// ABSENT-slot DEFAULT-ARM pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot is `None` returns
    /// `true` from [`Self::has_horizon_kind`] on
    /// [`HorizonKind::Bounded`] (the `default_ephemeral_class`
    /// baseline's `horizon.kind` axis AND the [`HorizonKind`] child's
    /// own `#[default]` variant) and `false` on every other variant.
    /// Pins the fresh (Option-parent × NESTED-STRUCT-scalar-child ×
    /// operator-resolvable-baseline) corner's default-arm short-
    /// circuit on the FIFTH classification-axis peer. Two-defaults
    /// composition property through a NESTED-STRUCT hop: both the
    /// parent Option's fill-through baseline
    /// (`default_ephemeral_class` fills `horizon: Horizon::default()`)
    /// AND the child's own `#[default]` (`HorizonKind::Bounded` via
    /// `#[default]` on the closed set) land on the SAME variant, so
    /// the ephemeral sugar surface's `horizon-Bounded` require-tag
    /// reads `true` on every operator-authored spec that omits both
    /// the `:classification` slot AND the `:horizon` sub-slot,
    /// pinning the workspace's bounded-by-default lifetime posture.
    #[test]
    fn has_horizon_kind_probes_bounded_only_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        for kind in HorizonKind::ALL {
            let expected = kind == HorizonKind::Bounded;
            assert_eq!(
                spec.has_horizon_kind(kind),
                expected,
                "absent classification (defaults to gate_compute): query {kind:?} must be {expected}",
            );
        }
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::has_horizon_kind`] AND through
    /// `<eph.clone().into::<ProcessSpec>>()`
    /// `.classification.has_horizon_kind(kind)` on the mechanically-
    /// lowered `ProcessSpec`. Sweeps (`None` classification, `Some(_)`
    /// classification on every [`HorizonKind::ALL`] variant) × ALL
    /// queries so a future regression on either side of the resolver
    /// (a shift in the ephemeral resolver's default, a shift in the
    /// `From<EphemeralSpec>` lowering's fill-through) fails HERE at
    /// the parity boundary. Byte-for-byte peer of the sibling
    /// [`Self::has_point_type`] + [`Self::has_substrate`] +
    /// [`Self::has_calm`] + [`Self::has_data_classification`] two-
    /// surface parity pins on the SAME `Cow`-resolver carrier — the
    /// FIFTH classification-axis two-surface parity contract on the
    /// ephemeral surface, and the FIRST on the (Option-parent ×
    /// NESTED-STRUCT-scalar-child) corner.
    #[test]
    fn has_horizon_kind_matches_point_peer_through_lowered_classification() {
        // Absent classification: both surfaces resolve through the SAME
        // default and agree on every variant.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        for query in HorizonKind::ALL {
            assert_eq!(
                eph.has_horizon_kind(query),
                lowered.classification.has_horizon_kind(query),
                "None-classification parity drift on query {query:?}",
            );
        }
        // Authored classification: both surfaces read the same authored
        // value verbatim.
        for populated in HorizonKind::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon = Horizon {
                kind: populated,
                ..Horizon::default()
            };
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            for query in HorizonKind::ALL {
                assert_eq!(
                    eph.has_horizon_kind(query),
                    lowered.classification.has_horizon_kind(query),
                    "authored classification.horizon.kind={populated:?}: parity drift on query {query:?}",
                );
            }
        }
    }

    // ── EphemeralSpec::has_optimization_direction pins ───────────────
    //
    // Fail-before-pass-after granularity:
    // [`Self::has_optimization_direction`] did not exist pre-lift on
    // `impl EphemeralSpec` — every callsite went through
    // `.resolved_classification().horizon.direction.unwrap_or_default() == kind`
    // or through the lowered `ProcessSpec`'s
    // `spec.classification.has_optimization_direction`. Post-lift the
    // SIXTH classification-axis peer on the ephemeral sugar surface
    // routes through the SAME [`Self::resolved_classification`]
    // resolver + the sibling closed-set primitive
    // [`crate::classification::Classification::has_optimization_direction`],
    // so a regression that dropped the resolver hop, inverted the
    // `Some`/`None` fill-through, wired the closure to a fixed
    // unrelated slot, or flipped [`OptimizationDirection`]'s
    // `#[default]` off `Minimize` fails HERE. SECOND occupant on the
    // (Option-parent × NESTED-STRUCT-scalar-child × operator-
    // resolvable-baseline) corner alongside
    // [`Self::has_horizon_kind`] — pinning the corner as a proven-
    // repeatable primitive shape on the ephemeral surface with a
    // second nested-struct-child probe, and DEMONSTRATING that the
    // corner admits both direct-scalar and Option-scalar traversals
    // through the SAME nested [`Horizon`] intermediary via the closed
    // set's `Default` on the inner `Option<OptimizationDirection>`
    // slot.

    /// AUTHORED-slot VARIANT-MATCH pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot names a concrete
    /// [`Classification`] whose [`crate::classification::Horizon::direction`]
    /// slot carries `Some(<direction>)` returns `true` from
    /// [`Self::has_optimization_direction`] on the authored
    /// [`OptimizationDirection`] variant and `false` for every other
    /// variant. Sweep the [`OptimizationDirection::ALL`] × ALL cross
    /// so a regression that hard-coded the arm to a single kind, or
    /// dropped the `Option::unwrap_or_default` collapse, or wired the
    /// closure to a fixed unrelated slot (e.g. reading `self.horizon.kind`)
    /// fails HERE at the substrate primitive. Byte-for-byte peer of the
    /// point-surface
    /// [`Classification::has_optimization_direction`] populated-slot
    /// sweep on the SAME closed-set primitive.
    #[test]
    fn has_optimization_direction_returns_true_iff_authored_direction_matches_per_kind() {
        for populated in OptimizationDirection::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon = Horizon {
                direction: Some(populated),
                ..Horizon::default()
            };
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            for query in OptimizationDirection::ALL {
                let expected = query == populated;
                assert_eq!(
                    spec.has_optimization_direction(query),
                    expected,
                    "ephemeral classification.horizon.direction=Some({populated:?}): query {query:?} drifted",
                );
            }
        }
    }

    /// ABSENT-slot DEFAULT-ARM pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot is `None` returns
    /// `true` from [`Self::has_optimization_direction`] on
    /// [`OptimizationDirection::Minimize`] (the `default_ephemeral_class`
    /// baseline fills `horizon: Horizon::default()`, which in turn
    /// leaves `direction: None`, and the substrate's
    /// `Option::unwrap_or_default` collapse then reads
    /// [`OptimizationDirection::Minimize`] via the closed set's
    /// `#[default]`) and `false` on every other variant. Pins the
    /// (Option-parent × NESTED-STRUCT-scalar-child × operator-
    /// resolvable-baseline) corner's default-arm short-circuit on the
    /// SIXTH classification-axis peer through TWO Option-hops: parent
    /// `EphemeralSpec::classification` and inner `Horizon::direction`
    /// both `None`, both collapsing to the closed set's `#[default]`
    /// [`OptimizationDirection::Minimize`]. A regression that promoted
    /// [`OptimizationDirection::Maximize`] to `#[default]` (silently
    /// inverting every unadorned Process's rate-window evaluator
    /// polarity), dropped `Option::unwrap_or_default`, or wired the arm
    /// to a fixed variant answer fails HERE.
    #[test]
    fn has_optimization_direction_probes_minimize_only_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        for kind in OptimizationDirection::ALL {
            let expected = kind == OptimizationDirection::Minimize;
            assert_eq!(
                spec.has_optimization_direction(kind),
                expected,
                "absent classification (defaults to gate_compute): query {kind:?} must be {expected}",
            );
        }
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::has_optimization_direction`] AND
    /// through
    /// `<eph.clone().into::<ProcessSpec>>().classification.has_optimization_direction(kind)`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps three arms —
    /// (`None` classification), (`Some(_)` classification with
    /// `direction: None`), and (`Some(_)` classification on every
    /// [`OptimizationDirection::ALL`] variant) — × ALL queries so a
    /// future regression on either side of the resolver (an ephemeral-
    /// side fill-through drift, a lowering-side `From<EphemeralSpec>`
    /// `unwrap_or_else(default_ephemeral_class)` drift, an inner
    /// `Option::unwrap_or_default` collapse drift on either side)
    /// fails HERE at the parity boundary. Byte-for-byte peer of the
    /// sibling [`Self::has_point_type`] + [`Self::has_substrate`] +
    /// [`Self::has_calm`] + [`Self::has_data_classification`] +
    /// [`Self::has_horizon_kind`] two-surface parity pins on the SAME
    /// `Cow`-resolver carrier — the SIXTH classification-axis two-
    /// surface parity contract on the ephemeral surface, and the
    /// SECOND on the (Option-parent × NESTED-STRUCT-scalar-child)
    /// corner.
    #[test]
    fn has_optimization_direction_matches_point_peer_through_lowered_classification() {
        // Absent classification: both surfaces resolve through the SAME
        // default and agree on every variant.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        for query in OptimizationDirection::ALL {
            assert_eq!(
                eph.has_optimization_direction(query),
                lowered.classification.has_optimization_direction(query),
                "None-classification parity drift on query {query:?}",
            );
        }
        // Authored classification with `direction: None` — the inner
        // Option collapses through `unwrap_or_default` on both sides,
        // reading `Minimize`.
        let mut classification = Classification::gate_compute();
        classification.horizon = Horizon::default();
        let mut eph = empty_ephemeral();
        eph.classification = Some(classification);
        let lowered: ProcessSpec = eph.clone().into();
        for query in OptimizationDirection::ALL {
            assert_eq!(
                eph.has_optimization_direction(query),
                lowered.classification.has_optimization_direction(query),
                "authored classification with horizon.direction=None: parity drift on query {query:?}",
            );
        }
        // Authored classification with `direction: Some(_)` — both
        // surfaces read the same authored value verbatim.
        for populated in OptimizationDirection::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon = Horizon {
                direction: Some(populated),
                ..Horizon::default()
            };
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            for query in OptimizationDirection::ALL {
                assert_eq!(
                    eph.has_optimization_direction(query),
                    lowered.classification.has_optimization_direction(query),
                    "authored classification.horizon.direction=Some({populated:?}): parity drift on query {query:?}",
                );
            }
        }
    }

    // ── EphemeralSpec::has_input_arity pins ──────────────────────────
    //
    // Fail-before-pass-after granularity: [`Self::has_input_arity`] did
    // not exist pre-lift on `impl EphemeralSpec` — every callsite went
    // through `.resolved_classification().point_type.input_arity() ==
    // kind` or through the lowered `ProcessSpec`'s
    // `spec.classification.has_input_arity`. Post-lift the SEVENTH
    // classification-axis peer on the ephemeral sugar surface routes
    // through the SAME [`Self::resolved_classification`] resolver + the
    // sibling closed-set primitive
    // [`crate::classification::Classification::has_input_arity`], so a
    // regression that dropped the resolver hop, dropped the
    // `.input_arity()` projection call, inverted the projection (`One
    // ↔ Many`), or crossed the wires with the sibling
    // [`ConvergencePointType::output_arity`] projection fails HERE.
    // OPENS the (Option-parent × NESTED-STRUCT-scalar-child ×
    // derived-typed-projection) corner on the ephemeral surface —
    // distinct from the two prior nested-scalar peers on the corner
    // (`has_horizon_kind` reads `horizon.kind` directly;
    // `has_optimization_direction` reads `horizon.direction` through an
    // Option collapse), both of which reach a discriminator DIRECTLY off
    // a scalar. This peer instead threads through a many-to-one closed-
    // set typed projection so the child's closed set is REACHED THROUGH
    // a projection layer, pinning the corner as admitting three
    // ephemeral-surface traversal shapes (direct-scalar, Option-scalar-
    // with-default, derived-typed-projection) through the SAME resolver
    // walk.

    /// AUTHORED-slot PROJECTED-VARIANT pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot names a concrete
    /// [`Classification`] with an authored [`ConvergencePointType`]
    /// returns `true` from [`Self::has_input_arity`] on the [`Arity`]
    /// value the projection [`ConvergencePointType::input_arity`] maps
    /// the authored point-type to and `false` for every other variant.
    /// Sweep the [`ConvergencePointType::ALL`] × [`Arity::ALL`] cross so
    /// a regression that (a) dropped the projection call, (b) inverted
    /// the projection, (c) probed [`ConvergencePointType`] directly, or
    /// (d) crossed wires with [`ConvergencePointType::output_arity`]
    /// fails HERE at the substrate primitive. Byte-for-byte peer of the
    /// point-surface [`Classification::has_input_arity`] populated-slot
    /// sweep on the SAME closed-set primitive routed through the SAME
    /// projection.
    #[test]
    fn has_input_arity_returns_true_iff_authored_point_type_projects_per_kind() {
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            let projected = populated.input_arity();
            for query in Arity::ALL {
                let expected = query == projected;
                assert_eq!(
                    spec.has_input_arity(query),
                    expected,
                    "ephemeral classification.point_type={populated:?} (projects to {projected:?}): query {query:?} drifted",
                );
            }
        }
    }

    /// ABSENT-slot PROJECTED-BASELINE pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot is `None` returns `true`
    /// from [`Self::has_input_arity`] on [`Arity::Many`] (the
    /// [`default_ephemeral_class`] baseline fills `point_type: Gate`,
    /// and [`ConvergencePointType::input_arity`] projects
    /// `Gate → Arity::Many`) and `false` on [`Arity::One`]. Pins the
    /// (Option-parent × NESTED-STRUCT-scalar-child × derived-typed-
    /// projection) corner's baseline projection on the SEVENTH
    /// classification-axis peer through a chain of TWO fill-throughs
    /// composed with ONE projection: the parent Option's
    /// `unwrap_or_else(default_ephemeral_class)` picks the substrate
    /// baseline, and the projection then collapses the baseline's
    /// point-type through the closed-set-driven many-to-one bucket
    /// walk. [`Arity`] carries no `#[default]`, so there is NO default-
    /// arm short-circuit shortcut here — the answer flows entirely
    /// through the projection's bucket-membership decision. A
    /// regression that promoted the baseline's `point_type` off `Gate`
    /// (silently flipping every unadorned Process's convergent-by-
    /// default input-side posture to endomorphic or diffusive), dropped
    /// the projection call, inverted the projection, or crossed wires
    /// with [`ConvergencePointType::output_arity`] (which would flip
    /// the baseline answer from `Many` to `One` for `Gate`) fails HERE.
    #[test]
    fn has_input_arity_probes_many_only_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        for kind in Arity::ALL {
            let expected = kind == Arity::Many;
            assert_eq!(
                spec.has_input_arity(kind),
                expected,
                "absent classification (defaults to gate_compute, point_type=Gate → input_arity=Many): query {kind:?} must be {expected}",
            );
        }
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::has_input_arity`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.has_input_arity(kind)`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`ConvergencePointType::ALL`] variant) × [`Arity::ALL`] queries
    /// so a future regression on either side of the resolver (a shift
    /// in the ephemeral resolver's default, a shift in the
    /// `From<EphemeralSpec>` lowering's fill-through, a projection
    /// drift on either side) fails HERE at the parity boundary. Byte-
    /// for-byte peer of the sibling [`Self::has_point_type`] +
    /// [`Self::has_substrate`] + [`Self::has_calm`] +
    /// [`Self::has_data_classification`] + [`Self::has_horizon_kind`] +
    /// [`Self::has_optimization_direction`] two-surface parity pins on
    /// the SAME `Cow`-resolver carrier — the SEVENTH classification-
    /// axis two-surface parity contract on the ephemeral surface, and
    /// the FIRST on the (Option-parent × NESTED-STRUCT-scalar-child ×
    /// derived-typed-projection) corner.
    #[test]
    fn has_input_arity_matches_point_peer_through_lowered_classification() {
        // Absent classification: both surfaces resolve through the SAME
        // default and agree on every variant.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        for query in Arity::ALL {
            assert_eq!(
                eph.has_input_arity(query),
                lowered.classification.has_input_arity(query),
                "None-classification parity drift on query {query:?}",
            );
        }
        // Authored classification: both surfaces read the same authored
        // point_type and route through the same projection.
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            for query in Arity::ALL {
                assert_eq!(
                    eph.has_input_arity(query),
                    lowered.classification.has_input_arity(query),
                    "authored classification.point_type={populated:?}: parity drift on query {query:?}",
                );
            }
        }
    }

    // ── EphemeralSpec::has_output_arity pins ─────────────────────────
    //
    // Fail-before-pass-after granularity: [`Self::has_output_arity`] did
    // not exist pre-lift on `impl EphemeralSpec` — every callsite went
    // through `.resolved_classification().point_type.output_arity() ==
    // kind` or through the lowered `ProcessSpec`'s
    // `spec.classification.has_output_arity`. Post-lift the EIGHTH
    // classification-axis peer on the ephemeral sugar surface routes
    // through the SAME [`Self::resolved_classification`] resolver + the
    // sibling closed-set primitive
    // [`crate::classification::Classification::has_output_arity`], so a
    // regression that dropped the resolver hop, dropped the
    // `.output_arity()` projection call, inverted the projection (`One
    // ↔ Many`), or crossed the wires with the sibling
    // [`ConvergencePointType::input_arity`] projection fails HERE.
    // CLOSES the (Option-parent × NESTED-STRUCT-scalar-child ×
    // derived-typed-projection) corner on the ephemeral surface as the
    // SECOND occupant — co-tenant with [`Self::has_input_arity`] on the
    // SAME `point_type` scalar carrier through the SAME [`Arity`] closed
    // set but through the sibling many-to-one projection, closing the
    // DAG-composition arity pair on the ephemeral side.

    /// AUTHORED-slot PROJECTED-VARIANT pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot names a concrete
    /// [`Classification`] with an authored [`ConvergencePointType`]
    /// returns `true` from [`Self::has_output_arity`] on the [`Arity`]
    /// value the projection [`ConvergencePointType::output_arity`] maps
    /// the authored point-type to and `false` for every other variant.
    /// Sweep the [`ConvergencePointType::ALL`] × [`Arity::ALL`] cross so
    /// a regression that (a) dropped the projection call, (b) inverted
    /// the projection, (c) probed [`ConvergencePointType`] directly, or
    /// (d) crossed wires with [`ConvergencePointType::input_arity`]
    /// fails HERE at the substrate primitive. Byte-for-byte peer of the
    /// point-surface [`Classification::has_output_arity`] populated-slot
    /// sweep on the SAME closed-set primitive routed through the SAME
    /// projection.
    #[test]
    fn has_output_arity_returns_true_iff_authored_point_type_projects_per_kind() {
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            let projected = populated.output_arity();
            for query in Arity::ALL {
                let expected = query == projected;
                assert_eq!(
                    spec.has_output_arity(query),
                    expected,
                    "ephemeral classification.point_type={populated:?} (projects to {projected:?}): query {query:?} drifted",
                );
            }
        }
    }

    /// ABSENT-slot PROJECTED-BASELINE pin — an [`EphemeralSpec`] whose
    /// [`EphemeralSpec::classification`] slot is `None` returns `true`
    /// from [`Self::has_output_arity`] on [`Arity::One`] (the
    /// [`default_ephemeral_class`] baseline fills `point_type: Gate`,
    /// and [`ConvergencePointType::output_arity`] projects
    /// `Gate → Arity::One`) and `false` on [`Arity::Many`]. MIRROR of
    /// the [`Self::has_input_arity`] baseline (`Gate → input_arity =
    /// Many`) — the DAG-composition arity pair projects the same `Gate`
    /// baseline through the two projections to opposite [`Arity`] arms,
    /// so this pin locks the output-side half of that pair against a
    /// regression that (a) promoted the baseline's `point_type` off
    /// `Gate` (silently flipping every unadorned Process's convergent-
    /// by-default output-side posture to diffusive), (b) dropped the
    /// projection call, (c) inverted the projection, or (d) crossed
    /// wires with [`ConvergencePointType::input_arity`] (which would
    /// flip the baseline answer from `One` to `Many` for `Gate`).
    #[test]
    fn has_output_arity_probes_one_only_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        for kind in Arity::ALL {
            let expected = kind == Arity::One;
            assert_eq!(
                spec.has_output_arity(kind),
                expected,
                "absent classification (defaults to gate_compute, point_type=Gate → output_arity=One): query {kind:?} must be {expected}",
            );
        }
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::has_output_arity`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.has_output_arity(kind)`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`ConvergencePointType::ALL`] variant) × [`Arity::ALL`] queries
    /// so a future regression on either side of the resolver fails HERE
    /// at the parity boundary. Byte-for-byte peer of the seven sibling
    /// two-surface parity pins on the SAME `Cow`-resolver carrier — the
    /// EIGHTH classification-axis two-surface parity contract on the
    /// ephemeral surface, closing the SECOND occupant of the (Option-
    /// parent × NESTED-STRUCT-scalar-child × derived-typed-projection)
    /// corner.
    #[test]
    fn has_output_arity_matches_point_peer_through_lowered_classification() {
        // Absent classification: both surfaces resolve through the SAME
        // default and agree on every variant.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        for query in Arity::ALL {
            assert_eq!(
                eph.has_output_arity(query),
                lowered.classification.has_output_arity(query),
                "None-classification parity drift on query {query:?}",
            );
        }
        // Authored classification: both surfaces read the same authored
        // point_type and route through the same projection.
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            for query in Arity::ALL {
                assert_eq!(
                    eph.has_output_arity(query),
                    lowered.classification.has_output_arity(query),
                    "authored classification.point_type={populated:?}: parity drift on query {query:?}",
                );
            }
        }
    }

    /// DAG-COMPOSITION ARITY-PAIR pin — the SEVENTH
    /// ([`Self::has_input_arity`]) and EIGHTH
    /// ([`Self::has_output_arity`]) classification-axis peers on the
    /// ephemeral surface walk the SAME `point_type` scalar carrier
    /// (routed through the SAME [`Self::resolved_classification`]
    /// resolver) through the SAME [`Arity`] closed set but through
    /// DIFFERENT typed projections
    /// ([`ConvergencePointType::input_arity`] vs.
    /// [`ConvergencePointType::output_arity`]). An [`EphemeralSpec`]
    /// with `classification.point_type = Fork` (the diffusive `(One,
    /// Many)` cell) MUST simultaneously answer `has_input_arity(One)`
    /// true AND `has_output_arity(Many)` true AND
    /// `has_input_arity(Many)` false AND `has_output_arity(One)` false.
    /// An [`EphemeralSpec`] with `point_type = Transform` (endomorphic
    /// `(One, One)`) MUST answer BOTH `has_input_arity(One)` and
    /// `has_output_arity(One)` true — the two projections AGREE in the
    /// endomorphic bucket. The absent-classification baseline (Gate,
    /// convergent `(Many, One)`) MUST answer
    /// `has_input_arity(Many)` true AND `has_output_arity(One)` true —
    /// the mirror of the Fork case. A regression that (a) collapsed
    /// `has_output_arity` onto `has_input_arity`, (b) swapped the
    /// projection direction, or (c) drifted the topology-bucket
    /// contract fails HERE at ONE narrow ephemeral-surface site,
    /// symmetric with the point-surface DAG-composition arity-pair pin.
    #[test]
    fn has_input_arity_and_has_output_arity_pin_dag_composition_pair() {
        // Diffusive cell: Fork carries (input, output) = (One, Many)
        let mut classification = Classification::gate_compute();
        classification.point_type = ConvergencePointType::Fork;
        let mut fork = empty_ephemeral();
        fork.classification = Some(classification);
        assert!(fork.has_input_arity(Arity::One));
        assert!(fork.has_output_arity(Arity::Many));
        assert!(!fork.has_input_arity(Arity::Many));
        assert!(!fork.has_output_arity(Arity::One));

        // Endomorphic cell: Transform carries (input, output) = (One, One)
        let mut classification = Classification::gate_compute();
        classification.point_type = ConvergencePointType::Transform;
        let mut transform = empty_ephemeral();
        transform.classification = Some(classification);
        assert!(transform.has_input_arity(Arity::One));
        assert!(transform.has_output_arity(Arity::One));
        assert!(!transform.has_input_arity(Arity::Many));
        assert!(!transform.has_output_arity(Arity::Many));

        // Convergent cell: absent classification defaults to Gate,
        // which carries (input, output) = (Many, One).
        let gate = empty_ephemeral();
        assert!(gate.classification.is_none());
        assert!(gate.has_input_arity(Arity::Many));
        assert!(gate.has_output_arity(Arity::One));
        assert!(!gate.has_input_arity(Arity::One));
        assert!(!gate.has_output_arity(Arity::Many));
    }

    // ── EphemeralSpec::horizon_terminates pins ───────────────────────
    //
    // Fail-before-pass-after granularity: `horizon_terminates` did not
    // exist pre-lift on `impl EphemeralSpec` — every consumer walking
    // the "does this ephemeral spec's horizon terminate?" question
    // went through `.resolved_classification().horizon.kind.terminates()`
    // or through the lowered `ProcessSpec`'s
    // `spec.classification.horizon.kind.terminates()`. Post-lift the
    // NINTH classification-axis peer on the ephemeral surface routes
    // through the SAME [`Self::resolved_classification`] resolver +
    // the sibling substrate primitive
    // [`crate::classification::Classification::horizon_terminates`],
    // so the two-surface parity contract holds by construction — a
    // regression on either side of the resolver fails at these pins
    // before landing at the operator-facing `terminating-horizon`
    // fixed tag in `tatara-check`.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific [`HorizonKind`] variant
    /// answers [`Self::horizon_terminates`] matching the closed
    /// set's own [`HorizonKind::terminates`] truth table. Sweep
    /// [`HorizonKind::ALL`] so a regression that (a) hard-coded the
    /// body to a fixed answer, (b) inverted the projection, or (c)
    /// crossed the wires with the antisymmetric partner
    /// [`HorizonKind::requires_metric_axes`] fails HERE at the
    /// substrate primitive before drifting through the
    /// `terminating-horizon` fixed tag or the peer point surface.
    #[test]
    fn horizon_terminates_returns_horizon_kind_projection_per_kind() {
        for populated in HorizonKind::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon = crate::classification::Horizon {
                kind: populated,
                ..crate::classification::Horizon::default()
            };
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.horizon_terminates(),
                populated.terminates(),
                "authored horizon.kind={populated:?}: horizon_terminates() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which uses
    /// [`crate::classification::Horizon::default`] whose `kind`
    /// defaults to [`HorizonKind::Bounded`] via `#[default]`, and
    /// [`HorizonKind::Bounded::terminates`] projects `true`, so
    /// [`Self::horizon_terminates`] returns `true`. Pins the default-
    /// arm short-circuit through THREE layers of `Default`
    /// ([`Classification::gate_compute`] → [`Horizon::default`] →
    /// [`HorizonKind::default`]) reaching this derived-nullary
    /// predicate — a regression that dropped the resolver hop
    /// (silently answering `false` on an absent classification, as
    /// if the operator's absence meant "no horizon at all") fails
    /// HERE at ONE narrow ephemeral-surface site.
    #[test]
    fn horizon_terminates_probes_true_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            spec.horizon_terminates(),
            "absent classification (defaults to gate_compute, horizon.kind=Bounded → terminates=true)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::horizon_terminates`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.horizon_terminates()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`HorizonKind::ALL`] variant) so a future regression on
    /// either side of the resolver fails HERE at the parity
    /// boundary. Byte-for-byte peer of the eight sibling two-surface
    /// parity pins on the SAME `Cow`-resolver carrier — the NINTH
    /// classification-axis two-surface parity contract on the
    /// ephemeral surface, and the FIRST via a derived-nullary-
    /// boolean predicate rather than a variant-equality probe.
    #[test]
    fn horizon_terminates_matches_point_peer_through_lowered_classification() {
        // Absent classification: both surfaces resolve through the SAME
        // default and agree.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.horizon_terminates(),
            lowered.classification.horizon_terminates(),
            "None-classification parity drift",
        );
        // Authored classification: both surfaces read the same authored
        // horizon.kind and route through the same projection.
        for populated in HorizonKind::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon = crate::classification::Horizon {
                kind: populated,
                ..crate::classification::Horizon::default()
            };
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.horizon_terminates(),
                lowered.classification.horizon_terminates(),
                "authored horizon.kind={populated:?}: parity drift",
            );
        }
    }

    // ── EphemeralSpec::horizon_requires_metric_axes pins ─────────────
    //
    // Fail-before-pass-after granularity: `horizon_requires_metric_axes`
    // did not exist pre-lift on `impl EphemeralSpec` — every consumer
    // walking the "does this ephemeral spec's horizon require metric
    // axes?" question went through
    // `.resolved_classification().horizon.kind.requires_metric_axes()`
    // or through the lowered `ProcessSpec`'s
    // `spec.classification.horizon.kind.requires_metric_axes()`. Post-
    // lift the antisymmetric peer of `horizon_terminates` routes
    // through the SAME [`Self::resolved_classification`] resolver +
    // the sibling substrate primitive
    // [`crate::classification::Classification::horizon_requires_metric_axes`],
    // so the two-surface parity contract holds by construction — a
    // regression on either side of the resolver fails at these pins
    // before landing at the operator-facing `metric-axes-required`
    // fixed tag in `tatara-check`.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific [`HorizonKind`] variant
    /// answers [`Self::horizon_requires_metric_axes`] matching the
    /// closed set's own [`HorizonKind::requires_metric_axes`] truth
    /// table. Sweep [`HorizonKind::ALL`] so a regression that (a)
    /// hard-coded the body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with the antisymmetric
    /// partner [`HorizonKind::terminates`] fails HERE at the
    /// substrate primitive before drifting through the
    /// `metric-axes-required` fixed tag or the peer point surface.
    #[test]
    fn horizon_requires_metric_axes_returns_horizon_kind_projection_per_kind() {
        for populated in HorizonKind::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon = crate::classification::Horizon {
                kind: populated,
                ..crate::classification::Horizon::default()
            };
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.horizon_requires_metric_axes(),
                populated.requires_metric_axes(),
                "authored horizon.kind={populated:?}: horizon_requires_metric_axes() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which uses
    /// [`crate::classification::Horizon::default`] whose `kind`
    /// defaults to [`HorizonKind::Bounded`] via `#[default]`, and
    /// [`HorizonKind::Bounded::requires_metric_axes`] projects
    /// `false`, so [`Self::horizon_requires_metric_axes`] returns
    /// `false`. Pins the default-arm short-circuit through THREE
    /// layers of `Default` ([`Classification::gate_compute`] →
    /// [`Horizon::default`] → [`HorizonKind::default`]) reaching this
    /// derived-nullary predicate — mirror image of
    /// `horizon_terminates_probes_true_on_absent_classification`.
    #[test]
    fn horizon_requires_metric_axes_probes_false_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            !spec.horizon_requires_metric_axes(),
            "absent classification (defaults to gate_compute, horizon.kind=Bounded → requires_metric_axes=false)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::horizon_requires_metric_axes`]
    /// AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.horizon_requires_metric_axes()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`HorizonKind::ALL`] variant) so a future regression on
    /// either side of the resolver fails HERE at the parity
    /// boundary. Byte-for-byte peer of the sibling
    /// `horizon_terminates_matches_point_peer_through_lowered_classification`.
    #[test]
    fn horizon_requires_metric_axes_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.horizon_requires_metric_axes(),
            lowered.classification.horizon_requires_metric_axes(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in HorizonKind::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon = crate::classification::Horizon {
                kind: populated,
                ..crate::classification::Horizon::default()
            };
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.horizon_requires_metric_axes(),
                lowered.classification.horizon_requires_metric_axes(),
                "authored horizon.kind={populated:?}: parity drift",
            );
        }
    }

    // ── EphemeralSpec::calm_requires_coordination pins ───────────────
    //
    // Fail-before-pass-after granularity: `calm_requires_coordination`
    // did not exist pre-lift on `impl EphemeralSpec` — every consumer
    // walking the "does this ephemeral spec require coordination?"
    // question went through
    // `.resolved_classification().calm.requires_coordination()` or
    // through the lowered `ProcessSpec`'s
    // `spec.classification.calm.requires_coordination()`. Post-lift the
    // THIRD derived-nullary-boolean peer on the ephemeral surface
    // (first on the calm axis, after the two horizon-axis peers)
    // routes through the SAME [`Self::resolved_classification`]
    // resolver + the sibling substrate primitive
    // [`crate::classification::Classification::calm_requires_coordination`],
    // so the two-surface parity contract holds by construction — a
    // regression on either side of the resolver fails at these pins
    // before landing at the operator-facing `coordination-required`
    // fixed tag in `tatara-check`.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific [`CalmClassification`]
    /// variant answers [`Self::calm_requires_coordination`] matching
    /// the closed set's own
    /// [`CalmClassification::requires_coordination`] truth table.
    /// Sweep [`CalmClassification::ALL`] so a regression that (a)
    /// hard-coded the body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with a sibling
    /// classification-axis probe fails HERE at the substrate primitive
    /// before drifting through the `coordination-required` fixed tag
    /// or the peer point surface.
    #[test]
    fn calm_requires_coordination_returns_calm_projection_per_kind() {
        for populated in CalmClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.calm = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.calm_requires_coordination(),
                populated.requires_coordination(),
                "authored calm={populated:?}: calm_requires_coordination() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// [`CalmClassification::default = Monotone`], and
    /// [`CalmClassification::Monotone::requires_coordination`] projects
    /// `false`, so [`Self::calm_requires_coordination`] returns
    /// `false`. Pins the default-arm short-circuit through TWO layers
    /// of `Default` ([`Classification::gate_compute`] →
    /// [`CalmClassification::default`]) reaching this derived-nullary
    /// predicate — distinct from the sibling `horizon_*` absent-
    /// classification pins by ONE structural degree (those walk THREE
    /// layers of `Default` because horizon has a nested-struct wrapper;
    /// this walks TWO because `calm` is a direct scalar). A regression
    /// that dropped the resolver hop (silently answering `true` on an
    /// absent classification, as if the operator's absence meant
    /// "requires coordination") fails HERE at ONE narrow ephemeral-
    /// surface site.
    #[test]
    fn calm_requires_coordination_probes_false_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            !spec.calm_requires_coordination(),
            "absent classification (defaults to gate_compute, calm=Monotone → requires_coordination=false)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::calm_requires_coordination`] AND
    /// through
    /// `<eph.clone().into::<ProcessSpec>>().classification.calm_requires_coordination()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`CalmClassification::ALL`] variant) so a future regression on
    /// either side of the resolver fails HERE at the parity boundary.
    /// Byte-for-byte peer of the sibling
    /// `horizon_terminates_matches_point_peer_through_lowered_classification`
    /// on the calm axis.
    #[test]
    fn calm_requires_coordination_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.calm_requires_coordination(),
            lowered.classification.calm_requires_coordination(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in CalmClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.calm = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.calm_requires_coordination(),
                lowered.classification.calm_requires_coordination(),
                "authored calm={populated:?}: parity drift",
            );
        }
    }

    // ── EphemeralSpec::data_is_regulated pins ────────────────────────
    //
    // Fail-before-pass-after granularity: `data_is_regulated` did not
    // exist pre-lift on `impl EphemeralSpec` — every consumer walking
    // the "does this ephemeral spec carry regulated data?" question
    // went through
    // `.resolved_classification().data_classification.is_regulated()`
    // or through the lowered `ProcessSpec`'s
    // `spec.classification.data_classification.is_regulated()`. Post-
    // lift the FOURTH derived-nullary-boolean peer on the ephemeral
    // surface (first on the data axis, after two horizon-axis peers
    // and one calm-axis peer) routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::data_is_regulated`],
    // so the two-surface parity contract holds by construction — a
    // regression on either side of the resolver fails at these pins
    // before landing at the operator-facing `data-regulated` fixed
    // tag in `tatara-check`.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific [`DataClassification`]
    /// variant answers [`Self::data_is_regulated`] matching the
    /// closed set's own [`DataClassification::is_regulated`] truth
    /// table. Sweep [`DataClassification::ALL`] so a regression that
    /// (a) hard-coded the body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with a sibling
    /// classification-axis probe fails HERE at the substrate
    /// primitive before drifting through the `data-regulated` fixed
    /// tag or the peer point surface.
    #[test]
    fn data_is_regulated_returns_data_classification_projection_per_kind() {
        for populated in DataClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.data_classification = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.data_is_regulated(),
                populated.is_regulated(),
                "authored data_classification={populated:?}: data_is_regulated() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// [`DataClassification::default = Internal`], and
    /// [`DataClassification::Internal::is_regulated`] projects
    /// `false`, so [`Self::data_is_regulated`] returns `false`. Pins
    /// the default-arm short-circuit through TWO layers of `Default`
    /// ([`Classification::gate_compute`] →
    /// [`DataClassification::default`]) reaching this derived-nullary
    /// predicate — byte-for-byte structural peer of the sibling
    /// `calm_requires_coordination_probes_false_on_absent_classification`
    /// on the classification-data axis, distinct from the two
    /// `horizon_*` absent-classification pins by ONE structural
    /// degree (those walk THREE layers because horizon has a nested-
    /// struct wrapper; this walks TWO because `data_classification`
    /// is a direct scalar). A regression that dropped the resolver
    /// hop (silently answering `true` on an absent classification,
    /// as if the operator's absence meant "regulated data") fails
    /// HERE at ONE narrow ephemeral-surface site.
    #[test]
    fn data_is_regulated_probes_false_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            !spec.data_is_regulated(),
            "absent classification (defaults to gate_compute, data_classification=Internal → is_regulated=false)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::data_is_regulated`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.data_is_regulated()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`DataClassification::ALL`] variant) so a future regression on
    /// either side of the resolver fails HERE at the parity boundary.
    /// Byte-for-byte peer of the sibling
    /// `calm_requires_coordination_matches_point_peer_through_lowered_classification`
    /// on the data axis.
    #[test]
    fn data_is_regulated_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.data_is_regulated(),
            lowered.classification.data_is_regulated(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in DataClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.data_classification = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.data_is_regulated(),
                lowered.classification.data_is_regulated(),
                "authored data_classification={populated:?}: parity drift",
            );
        }
    }

    // ── EphemeralSpec::data_is_restricted pins ───────────────────────
    //
    // Fail-before-pass-after granularity: `data_is_restricted` did not
    // exist pre-lift on `impl EphemeralSpec` — every consumer walking
    // the "does this ephemeral spec require access controls?" question
    // went through
    // `.resolved_classification().data_classification.is_restricted()`
    // or through the lowered `ProcessSpec`'s
    // `spec.classification.data_classification.is_restricted()`. Post-
    // lift the FIFTH derived-nullary-boolean peer on the ephemeral
    // surface (second on the data axis, after
    // [`Self::data_is_regulated`] opened the axis) routes through the
    // SAME [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::data_is_restricted`],
    // so the two-surface parity contract holds by construction — a
    // regression on either side of the resolver fails at these pins
    // before landing at the operator-facing `data-restricted` fixed
    // tag in `tatara-check`. FIRST direct-scalar ephemeral-surface
    // peer whose absent-classification baseline projects to `true`
    // rather than `false`.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific [`DataClassification`]
    /// variant answers [`Self::data_is_restricted`] matching the
    /// closed set's own [`DataClassification::is_restricted`] truth
    /// table. Sweep [`DataClassification::ALL`] so a regression that
    /// (a) hard-coded the body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with the sibling
    /// [`DataClassification::is_regulated`] projection fails HERE at
    /// the substrate primitive before drifting through the
    /// `data-restricted` fixed tag or the peer point surface.
    #[test]
    fn data_is_restricted_returns_data_classification_projection_per_kind() {
        for populated in DataClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.data_classification = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.data_is_restricted(),
                populated.is_restricted(),
                "authored data_classification={populated:?}: data_is_restricted() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// [`DataClassification::default = Internal`], and
    /// [`DataClassification::Internal::is_restricted`] projects
    /// `true`, so [`Self::data_is_restricted`] returns `true`. Pins
    /// the default-arm short-circuit through TWO layers of `Default`
    /// ([`Classification::gate_compute`] →
    /// [`DataClassification::default`]) reaching this derived-nullary
    /// predicate. FIRST direct-scalar ephemeral-surface peer whose
    /// absent-classification baseline answers `true`, not `false`
    /// (the four earlier direct-scalar peers on this surface —
    /// `data_is_regulated`, `calm_requires_coordination`, plus the
    /// nested-struct `horizon_requires_metric_axes` — all project
    /// `false` on the same absent classification, and only the
    /// sibling nested-struct `horizon_terminates` projects `true`).
    /// A regression that dropped the resolver hop (silently answering
    /// `false` on an absent classification, as if the operator's
    /// absence meant "freely distributable"), or that inverted the
    /// projection while the closed-set primitive stayed intact,
    /// fails HERE at ONE narrow ephemeral-surface site.
    #[test]
    fn data_is_restricted_probes_true_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            spec.data_is_restricted(),
            "absent classification (defaults to gate_compute, data_classification=Internal → is_restricted=true)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::data_is_restricted`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.data_is_restricted()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`DataClassification::ALL`] variant) so a future regression on
    /// either side of the resolver fails HERE at the parity boundary.
    /// Byte-for-byte peer of the sibling
    /// `data_is_regulated_matches_point_peer_through_lowered_classification`
    /// on the same classification-data axis, published a second time
    /// through the antisymmetric closed-set projection.
    #[test]
    fn data_is_restricted_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.data_is_restricted(),
            lowered.classification.data_is_restricted(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in DataClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.data_classification = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.data_is_restricted(),
                lowered.classification.data_is_restricted(),
                "authored data_classification={populated:?}: parity drift",
            );
        }
    }

    /// COMPOSED IMPLICATION pin — the ephemeral-surface counterpart of
    /// the closed-set-internal
    /// `data_classification_regulated_implies_restricted` and its
    /// parent-composed peer
    /// `classification_data_is_regulated_implies_data_is_restricted_over_all`:
    /// for every ([`EphemeralSpec`] with authored classification
    /// carrying every [`DataClassification`] variant, plus the
    /// absent-classification case), the resolver-hop probe pair
    /// satisfies `data_is_regulated() ⇒ data_is_restricted()`. Pins
    /// the implication contract at the ephemeral-surface site so a
    /// regression that (a) inverted the ephemeral
    /// [`Self::data_is_regulated`] resolver hop, (b) inverted the
    /// ephemeral [`Self::data_is_restricted`] resolver hop, or (c)
    /// crossed their wires while the underlying substrate primitives
    /// stayed intact fails HERE. FIRST ephemeral-surface corner-peer
    /// pair whose two projections carry a non-trivial closed-set-
    /// internal implication relationship.
    #[test]
    fn ephemeral_data_is_regulated_implies_data_is_restricted_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        assert!(
            !eph.data_is_regulated() || eph.data_is_restricted(),
            "None-classification: data_is_regulated ⇒ data_is_restricted violated",
        );
        // Authored classification.
        for populated in DataClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.data_classification = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            assert!(
                !eph.data_is_regulated() || eph.data_is_restricted(),
                "authored data_classification={populated:?}: data_is_regulated ⇒ data_is_restricted violated",
            );
        }
    }

    // ── EphemeralSpec::point_is_endomorphic pins ─────────────────────
    //
    // Fail-before-pass-after granularity: `point_is_endomorphic` did
    // not exist pre-lift on `impl EphemeralSpec` — every consumer
    // walking the "does this ephemeral spec's point-type project to
    // the 1→1 endomorphic bucket?" question went through
    // `.resolved_classification().point_type.is_endomorphic()` or the
    // lowered `ProcessSpec`'s
    // `spec.classification.point_type.is_endomorphic()`. Post-lift the
    // SIXTH derived-nullary-boolean peer on the ephemeral surface
    // (first on the `point_type` axis) routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::point_is_endomorphic`],
    // so the two-surface parity contract holds by construction — a
    // regression on either side of the resolver fails at these pins
    // before landing at the operator-facing `endomorphic-point` fixed
    // tag in `tatara-check`.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific [`ConvergencePointType`]
    /// variant answers [`Self::point_is_endomorphic`] matching the
    /// closed set's own [`ConvergencePointType::is_endomorphic`] truth
    /// table. Sweep [`ConvergencePointType::ALL`] so a regression that
    /// (a) hard-coded the body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with the sibling
    /// [`ConvergencePointType::is_diffusive`] /
    /// [`ConvergencePointType::is_convergent`] projections fails
    /// HERE at the substrate primitive before drifting through the
    /// `endomorphic-point` fixed tag or the peer point surface.
    #[test]
    fn point_is_endomorphic_returns_point_type_projection_per_kind() {
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.point_is_endomorphic(),
                populated.is_endomorphic(),
                "authored point_type={populated:?}: point_is_endomorphic() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// [`ConvergencePointType::Gate`] (a convergent barrier, not an
    /// endomorphism), and
    /// [`ConvergencePointType::Gate::is_endomorphic`] projects `false`,
    /// so [`Self::point_is_endomorphic`] returns `false`. Pins the
    /// resolver's chosen-field baseline at ONE narrow site — a
    /// regression that dropped the resolver hop, or that promoted
    /// [`ConvergencePointType::Transform`] to the gate-compute
    /// baseline (silently retargeting every unadorned ephemeral
    /// spec's topology bucket), fails HERE at ONE narrow ephemeral-
    /// surface site. FIRST direct-scalar ephemeral-surface peer whose
    /// absent-classification baseline is a chosen-field answer on the
    /// resolver's [`Classification::gate_compute`] default rather
    /// than a substrate-`#[default]` short-circuit on the closed-set
    /// side ([`ConvergencePointType`] has no `impl Default`).
    #[test]
    fn point_is_endomorphic_probes_false_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            !spec.point_is_endomorphic(),
            "absent classification (defaults to gate_compute, point_type=Gate → is_endomorphic=false)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::point_is_endomorphic`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.point_is_endomorphic()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`ConvergencePointType::ALL`] variant) so a future regression
    /// on either side of the resolver fails HERE at the parity
    /// boundary. Byte-for-byte peer of the sibling
    /// `data_is_restricted_matches_point_peer_through_lowered_classification`
    /// on a DIFFERENT closed-set axis, published a first time through
    /// the `point_type` closed-set projection.
    #[test]
    fn point_is_endomorphic_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.point_is_endomorphic(),
            lowered.classification.point_is_endomorphic(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.point_is_endomorphic(),
                lowered.classification.point_is_endomorphic(),
                "authored point_type={populated:?}: parity drift",
            );
        }
    }

    // ── EphemeralSpec::point_is_diffusive pins ───────────────────────
    //
    // Fail-before-pass-after granularity: `point_is_diffusive` did not
    // exist pre-lift on `impl EphemeralSpec` — every consumer walking
    // the "does this ephemeral spec's point-type project to the 1→N
    // diffusive fan-out bucket?" question went through
    // `.resolved_classification().point_type.is_diffusive()` or the
    // lowered `ProcessSpec`'s
    // `spec.classification.point_type.is_diffusive()`. Post-lift the
    // SEVENTH derived-nullary-boolean peer on the ephemeral surface
    // (SECOND on the `point_type` axis) routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::point_is_diffusive`],
    // so the two-surface parity contract holds by construction.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific [`ConvergencePointType`]
    /// variant answers [`Self::point_is_diffusive`] matching the
    /// closed set's own [`ConvergencePointType::is_diffusive`] truth
    /// table. Sweep [`ConvergencePointType::ALL`] so a regression that
    /// (a) hard-coded the body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with the sibling
    /// [`ConvergencePointType::is_endomorphic`] /
    /// [`ConvergencePointType::is_convergent`] projections fails HERE
    /// at the substrate primitive before drifting through the
    /// `diffusive-point` fixed tag or the peer point surface.
    #[test]
    fn point_is_diffusive_returns_point_type_projection_per_kind() {
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.point_is_diffusive(),
                populated.is_diffusive(),
                "authored point_type={populated:?}: point_is_diffusive() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// [`ConvergencePointType::Gate`] (a convergent barrier, not a
    /// diffusive fan-out), and
    /// [`ConvergencePointType::Gate::is_diffusive`] projects `false`,
    /// so [`Self::point_is_diffusive`] returns `false`. Pins the
    /// resolver's chosen-field baseline at ONE narrow site.
    #[test]
    fn point_is_diffusive_probes_false_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            !spec.point_is_diffusive(),
            "absent classification (defaults to gate_compute, point_type=Gate → is_diffusive=false)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::point_is_diffusive`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.point_is_diffusive()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`ConvergencePointType::ALL`] variant) so a future regression
    /// on either side of the resolver fails HERE at the parity
    /// boundary. Byte-for-byte peer of
    /// `point_is_endomorphic_matches_point_peer_through_lowered_classification`
    /// on the SAME closed-set axis via a sibling projection.
    #[test]
    fn point_is_diffusive_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.point_is_diffusive(),
            lowered.classification.point_is_diffusive(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.point_is_diffusive(),
                lowered.classification.point_is_diffusive(),
                "authored point_type={populated:?}: parity drift",
            );
        }
    }

    /// MUTEX pin — [`Self::point_is_endomorphic`] AND
    /// [`Self::point_is_diffusive`] are NEVER simultaneously true for
    /// ANY [`EphemeralSpec`] (authored or defaulted), since the
    /// underlying [`ConvergencePointType`] closed set carves its
    /// eight variants into THREE disjoint buckets. Sweep the absent-
    /// classification case + every [`ConvergencePointType::ALL`]
    /// variant so a regression that crossed the wires between the
    /// two ephemeral-surface corner peers (one probe silently
    /// composing the wrong closed-set arm at the resolver-hop layer)
    /// fails HERE rather than at every downstream consumer that
    /// trusts the two probes partition the resolver's output into
    /// disjoint buckets. FIRST ephemeral-surface corner-peer pair on
    /// the `point_type` axis whose two projections carry a non-
    /// trivial closed-set-internal MUTEX relationship (distinct from
    /// the sibling `data`-axis pair whose two projections carry a
    /// non-trivial IMPLICATION relationship, sealed by
    /// `ephemeral_data_is_regulated_implies_data_is_restricted_over_all`).
    #[test]
    fn ephemeral_point_is_endomorphic_and_point_is_diffusive_are_mutex_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        assert!(
            !(eph.point_is_endomorphic() && eph.point_is_diffusive()),
            "None-classification: point_is_endomorphic AND point_is_diffusive both true (mutex violated)",
        );
        // Authored classification.
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            assert!(
                !(eph.point_is_endomorphic() && eph.point_is_diffusive()),
                "authored point_type={populated:?}: point_is_endomorphic AND point_is_diffusive both true (mutex violated)",
            );
        }
    }

    // ── EphemeralSpec::point_is_convergent pins ──────────────────────
    //
    // Fail-before-pass-after granularity: `point_is_convergent` did
    // not exist pre-lift on `impl EphemeralSpec` — every consumer
    // walking the "does this ephemeral spec's point-type project to
    // the N→1 convergent fan-in bucket?" question went through
    // `.resolved_classification().point_type.is_convergent()` or the
    // lowered `ProcessSpec`'s
    // `spec.classification.point_type.is_convergent()`. Post-lift the
    // EIGHTH derived-nullary-boolean peer on the ephemeral surface
    // (THIRD on the `point_type` axis) routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::point_is_convergent`],
    // so the two-surface parity contract holds by construction, AND
    // the THREE `point_type`-axis peers on this surface close into
    // the FULL three-way XOR partition contract.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific [`ConvergencePointType`]
    /// variant answers [`Self::point_is_convergent`] matching the
    /// closed set's own [`ConvergencePointType::is_convergent`] truth
    /// table. Sweep [`ConvergencePointType::ALL`] so a regression that
    /// (a) hard-coded the body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with the sibling
    /// [`ConvergencePointType::is_endomorphic`] /
    /// [`ConvergencePointType::is_diffusive`] projections fails HERE
    /// at the substrate primitive before drifting through the
    /// `convergent-point` fixed tag or the peer point surface.
    #[test]
    fn point_is_convergent_returns_point_type_projection_per_kind() {
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.point_is_convergent(),
                populated.is_convergent(),
                "authored point_type={populated:?}: point_is_convergent() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// [`ConvergencePointType::Gate`] (the canonical convergent
    /// barrier), and [`ConvergencePointType::Gate::is_convergent`]
    /// projects `true`, so [`Self::point_is_convergent`] returns
    /// `true`. Pins the resolver's chosen-field baseline at ONE
    /// narrow site — FIRST direct-scalar ephemeral-surface peer whose
    /// absent-classification baseline projects `true` through the
    /// resolver's chosen-field answer, mirror-inverted from the two
    /// sibling `point_is_endomorphic` / `point_is_diffusive`
    /// ephemeral-surface baselines which both project `false`.
    #[test]
    fn point_is_convergent_probes_true_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            spec.point_is_convergent(),
            "absent classification (defaults to gate_compute, point_type=Gate → is_convergent=true)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::point_is_convergent`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.point_is_convergent()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`ConvergencePointType::ALL`] variant) so a future regression
    /// on either side of the resolver fails HERE at the parity
    /// boundary. Byte-for-byte peer of
    /// `point_is_endomorphic_matches_point_peer_through_lowered_classification`
    /// and
    /// `point_is_diffusive_matches_point_peer_through_lowered_classification`
    /// on the SAME closed-set axis via a sibling projection.
    #[test]
    fn point_is_convergent_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.point_is_convergent(),
            lowered.classification.point_is_convergent(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.point_is_convergent(),
                lowered.classification.point_is_convergent(),
                "authored point_type={populated:?}: parity drift",
            );
        }
    }

    /// THREE-WAY XOR PARTITION pin — for the absent-classification
    /// baseline AND every [`ConvergencePointType::ALL`] variant,
    /// EXACTLY ONE of [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], and [`Self::point_is_convergent`]
    /// returns `true`. Closes the mutex pair
    /// `ephemeral_point_is_endomorphic_and_point_is_diffusive_are_mutex_over_all`
    /// into the FULL ternary XOR partition contract on the ephemeral
    /// surface — the resolver-hop peer of the parent-composed
    /// `classification_point_type_probes_form_three_way_xor_partition_over_all`
    /// test. Guarantees the absent-classification case lands in the
    /// convergent bucket (`gate_compute` → Gate → is_convergent =
    /// true), so every unadorned `(defephemeral …)` audits under a
    /// definite non-empty topology bucket.
    #[test]
    fn ephemeral_point_type_probes_form_three_way_xor_partition_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        let buckets = [
            eph.point_is_endomorphic(),
            eph.point_is_diffusive(),
            eph.point_is_convergent(),
        ];
        let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
        assert_eq!(
            hits, 1,
            "None-classification: probes {buckets:?} — exactly one must be true (three-way XOR partition violated)",
        );
        // Authored classification.
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let buckets = [
                eph.point_is_endomorphic(),
                eph.point_is_diffusive(),
                eph.point_is_convergent(),
            ];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "authored point_type={populated:?}: probes {buckets:?} — exactly one must be true (three-way XOR partition violated)",
            );
        }
    }

    // ── EphemeralSpec::substrate_is_resource pins ────────────────────
    //
    // Fail-before-pass-after granularity: `substrate_is_resource` did
    // not exist pre-lift on `impl EphemeralSpec` — every consumer
    // walking the "does this ephemeral spec's substrate project to
    // the resource plane?" question went through
    // `.resolved_classification().substrate.is_resource()` or the
    // lowered `ProcessSpec`'s
    // `spec.classification.substrate.is_resource()`. Post-lift the
    // NINTH derived-nullary-boolean peer on the ephemeral surface
    // (FIRST on the `substrate` axis) routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::substrate_is_resource`],
    // so the two-surface parity contract holds by construction.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific
    /// [`crate::classification::SubstrateType`] variant answers
    /// [`Self::substrate_is_resource`] matching the closed set's own
    /// [`crate::classification::SubstrateType::is_resource`] truth
    /// table. Sweep [`crate::classification::SubstrateType::ALL`]
    /// so a regression that (a) hard-coded the body to a fixed
    /// answer, (b) inverted the projection, or (c) crossed the wires
    /// with the sibling
    /// [`crate::classification::SubstrateType::is_policy`] /
    /// [`crate::classification::SubstrateType::is_telemetry`]
    /// projections fails HERE at the substrate primitive before
    /// drifting through the `resource-substrate` fixed tag or the
    /// peer point surface.
    #[test]
    fn substrate_is_resource_returns_substrate_projection_per_kind() {
        for populated in SubstrateType::ALL {
            let mut classification = Classification::gate_compute();
            classification.substrate = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.substrate_is_resource(),
                populated.is_resource(),
                "authored substrate={populated:?}: substrate_is_resource() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// [`crate::classification::SubstrateType::Compute`] (the
    /// canonical resource-plane substrate), and
    /// [`crate::classification::SubstrateType::Compute::is_resource`]
    /// projects `true`, so [`Self::substrate_is_resource`] returns
    /// `true`. Pins the resolver's chosen-field baseline at ONE
    /// narrow site — mirror-aligned with the sibling
    /// `point_is_convergent_probes_true_on_absent_classification`
    /// baseline (both projections on `gate_compute` chosen fields
    /// answer `true`).
    #[test]
    fn substrate_is_resource_probes_true_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            spec.substrate_is_resource(),
            "absent classification (defaults to gate_compute, substrate=Compute → is_resource=true)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::substrate_is_resource`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.substrate_is_resource()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`crate::classification::SubstrateType::ALL`] variant) so a
    /// future regression on either side of the resolver fails HERE
    /// at the parity boundary. Byte-for-byte peer of
    /// `point_is_convergent_matches_point_peer_through_lowered_classification`
    /// on a sibling classification axis.
    #[test]
    fn substrate_is_resource_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.substrate_is_resource(),
            lowered.classification.substrate_is_resource(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in SubstrateType::ALL {
            let mut classification = Classification::gate_compute();
            classification.substrate = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.substrate_is_resource(),
                lowered.classification.substrate_is_resource(),
                "authored substrate={populated:?}: parity drift",
            );
        }
    }

    // ── EphemeralSpec::substrate_is_policy pins ──────────────────────
    //
    // Fail-before-pass-after granularity: `substrate_is_policy` did
    // not exist pre-lift on `impl EphemeralSpec` — every consumer
    // walking the "does this ephemeral spec's substrate project to
    // the policy plane?" question went through
    // `.resolved_classification().substrate.is_policy()` or the
    // lowered `ProcessSpec`'s
    // `spec.classification.substrate.is_policy()`. Post-lift the
    // TENTH derived-nullary-boolean peer on the ephemeral surface
    // (SECOND on the `substrate` axis) routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::substrate_is_policy`],
    // so the two-surface parity contract holds by construction, AND
    // the two `substrate`-axis peers on this surface open the
    // MUTEX pair on the axis via
    // `ephemeral_substrate_is_resource_and_substrate_is_policy_are_mutex_over_all`.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific
    /// [`crate::classification::SubstrateType`] variant answers
    /// [`Self::substrate_is_policy`] matching the closed set's own
    /// [`crate::classification::SubstrateType::is_policy`] truth
    /// table. Sweep [`crate::classification::SubstrateType::ALL`]
    /// so a regression that (a) hard-coded the body to a fixed
    /// answer, (b) inverted the projection, or (c) crossed the wires
    /// with the sibling
    /// [`crate::classification::SubstrateType::is_resource`] /
    /// [`crate::classification::SubstrateType::is_telemetry`]
    /// projections fails HERE at the substrate primitive before
    /// drifting through the `policy-substrate` fixed tag or the
    /// peer point surface.
    #[test]
    fn substrate_is_policy_returns_substrate_projection_per_kind() {
        for populated in SubstrateType::ALL {
            let mut classification = Classification::gate_compute();
            classification.substrate = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.substrate_is_policy(),
                populated.is_policy(),
                "authored substrate={populated:?}: substrate_is_policy() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// [`crate::classification::SubstrateType::Compute`] (the
    /// canonical resource-plane substrate, NOT a policy plane), and
    /// [`crate::classification::SubstrateType::Compute::is_policy`]
    /// projects `false`, so [`Self::substrate_is_policy`] returns
    /// `false`. Pins the resolver's chosen-field baseline at ONE
    /// narrow site — mirror-inverted from the sibling
    /// `substrate_is_resource_probes_true_on_absent_classification`
    /// (both projections on `gate_compute`'s chosen `substrate`
    /// field, but the sibling answers `true` where this one
    /// answers `false` — the closed set's disjoint plane partition
    /// forbids both being true).
    #[test]
    fn substrate_is_policy_probes_false_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            !spec.substrate_is_policy(),
            "absent classification (defaults to gate_compute, substrate=Compute → is_policy=false)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::substrate_is_policy`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.substrate_is_policy()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`crate::classification::SubstrateType::ALL`] variant) so a
    /// future regression on either side of the resolver fails HERE
    /// at the parity boundary. Byte-for-byte peer of
    /// `substrate_is_resource_matches_point_peer_through_lowered_classification`
    /// on the SAME closed-set axis via a sibling projection.
    #[test]
    fn substrate_is_policy_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.substrate_is_policy(),
            lowered.classification.substrate_is_policy(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in SubstrateType::ALL {
            let mut classification = Classification::gate_compute();
            classification.substrate = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.substrate_is_policy(),
                lowered.classification.substrate_is_policy(),
                "authored substrate={populated:?}: parity drift",
            );
        }
    }

    /// MUTEX pin — [`Self::substrate_is_resource`] AND
    /// [`Self::substrate_is_policy`] are NEVER simultaneously true
    /// for ANY [`EphemeralSpec`] (authored or defaulted), since the
    /// underlying [`crate::classification::SubstrateType`] closed set
    /// carves its eight variants into THREE disjoint buckets. Sweep
    /// the absent-classification case + every
    /// [`crate::classification::SubstrateType::ALL`] variant so a
    /// regression that crossed the wires between the two ephemeral-
    /// surface corner peers (one probe silently composing the wrong
    /// closed-set arm at the resolver-hop layer) fails HERE rather
    /// than at every downstream consumer that trusts the two probes
    /// partition the resolver's output into disjoint buckets.
    /// FIRST ephemeral-surface `substrate`-axis corner-peer pair
    /// carrying a non-trivial MUTEX relationship — structural twin
    /// of the sibling `point_type`-axis MUTEX pair sealed on this
    /// surface by
    /// `ephemeral_point_is_endomorphic_and_point_is_diffusive_are_mutex_over_all`.
    #[test]
    fn ephemeral_substrate_is_resource_and_substrate_is_policy_are_mutex_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        assert!(
            !(eph.substrate_is_resource() && eph.substrate_is_policy()),
            "None-classification: substrate_is_resource AND substrate_is_policy both true (mutex violated)",
        );
        // Authored classification.
        for populated in SubstrateType::ALL {
            let mut classification = Classification::gate_compute();
            classification.substrate = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            assert!(
                !(eph.substrate_is_resource() && eph.substrate_is_policy()),
                "authored substrate={populated:?}: substrate_is_resource AND substrate_is_policy both true (mutex violated)",
            );
        }
    }

    // ── EphemeralSpec::substrate_is_telemetry pins ───────────────────
    //
    // Fail-before-pass-after granularity: `substrate_is_telemetry`
    // did not exist pre-lift on `impl EphemeralSpec` — every consumer
    // walking the "does this ephemeral spec's substrate project to
    // the telemetry plane?" question went through
    // `.resolved_classification().substrate.is_telemetry()` or the
    // lowered `ProcessSpec`'s
    // `spec.classification.substrate.is_telemetry()`. Post-lift the
    // ELEVENTH derived-nullary-boolean peer on the ephemeral surface
    // (THIRD on the `substrate` axis) routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::substrate_is_telemetry`],
    // so the two-surface parity contract holds by construction, AND
    // the three `substrate`-axis peers on this surface CLOSE the
    // axis into the FULL three-way XOR partition contract via
    // `ephemeral_substrate_probes_form_three_way_xor_partition_over_all`.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific
    /// [`crate::classification::SubstrateType`] variant answers
    /// [`Self::substrate_is_telemetry`] matching the closed set's own
    /// [`crate::classification::SubstrateType::is_telemetry`] truth
    /// table. Sweep [`crate::classification::SubstrateType::ALL`]
    /// so a regression that (a) hard-coded the body to a fixed
    /// answer, (b) inverted the projection, or (c) crossed the wires
    /// with the sibling
    /// [`crate::classification::SubstrateType::is_resource`] /
    /// [`crate::classification::SubstrateType::is_policy`]
    /// projections fails HERE at the substrate primitive before
    /// drifting through the `telemetry-substrate` fixed tag or the
    /// peer point surface.
    #[test]
    fn substrate_is_telemetry_returns_substrate_projection_per_kind() {
        for populated in SubstrateType::ALL {
            let mut classification = Classification::gate_compute();
            classification.substrate = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.substrate_is_telemetry(),
                populated.is_telemetry(),
                "authored substrate={populated:?}: substrate_is_telemetry() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// [`crate::classification::SubstrateType::Compute`] (the
    /// canonical resource-plane substrate, NOT a telemetry plane),
    /// and
    /// [`crate::classification::SubstrateType::Compute::is_telemetry`]
    /// projects `false`, so [`Self::substrate_is_telemetry`] returns
    /// `false`. Pins the resolver's chosen-field baseline at ONE
    /// narrow site — aligned with the sibling
    /// `substrate_is_policy_probes_false_on_absent_classification`
    /// (both projections on `gate_compute`'s chosen `substrate`
    /// field project `false` since `Compute` lives in the resource
    /// plane), mirror-inverted from
    /// `substrate_is_resource_probes_true_on_absent_classification`.
    #[test]
    fn substrate_is_telemetry_probes_false_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            !spec.substrate_is_telemetry(),
            "absent classification (defaults to gate_compute, substrate=Compute → is_telemetry=false)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::substrate_is_telemetry`] AND
    /// through
    /// `<eph.clone().into::<ProcessSpec>>().classification.substrate_is_telemetry()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`crate::classification::SubstrateType::ALL`] variant) so a
    /// future regression on either side of the resolver fails HERE
    /// at the parity boundary. Byte-for-byte peer of
    /// `substrate_is_policy_matches_point_peer_through_lowered_classification`
    /// on the SAME closed-set axis via a sibling projection.
    #[test]
    fn substrate_is_telemetry_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.substrate_is_telemetry(),
            lowered.classification.substrate_is_telemetry(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in SubstrateType::ALL {
            let mut classification = Classification::gate_compute();
            classification.substrate = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.substrate_is_telemetry(),
                lowered.classification.substrate_is_telemetry(),
                "authored substrate={populated:?}: parity drift",
            );
        }
    }

    /// MUTEX pin — [`Self::substrate_is_resource`] AND
    /// [`Self::substrate_is_telemetry`] are NEVER simultaneously true
    /// for ANY [`EphemeralSpec`] (authored or defaulted). Second
    /// ephemeral-surface `substrate`-axis corner-peer MUTEX pin —
    /// peer of
    /// `ephemeral_substrate_is_resource_and_substrate_is_policy_are_mutex_over_all`
    /// on a sibling closed-set projection.
    #[test]
    fn ephemeral_substrate_is_resource_and_substrate_is_telemetry_are_mutex_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        assert!(
            !(eph.substrate_is_resource() && eph.substrate_is_telemetry()),
            "None-classification: substrate_is_resource AND substrate_is_telemetry both true (mutex violated)",
        );
        // Authored classification.
        for populated in SubstrateType::ALL {
            let mut classification = Classification::gate_compute();
            classification.substrate = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            assert!(
                !(eph.substrate_is_resource() && eph.substrate_is_telemetry()),
                "authored substrate={populated:?}: substrate_is_resource AND substrate_is_telemetry both true (mutex violated)",
            );
        }
    }

    /// MUTEX pin — [`Self::substrate_is_policy`] AND
    /// [`Self::substrate_is_telemetry`] are NEVER simultaneously true
    /// for ANY [`EphemeralSpec`] (authored or defaulted). Third
    /// ephemeral-surface `substrate`-axis corner-peer MUTEX pin —
    /// completes the three pairwise MUTEX relations alongside
    /// `ephemeral_substrate_is_resource_and_substrate_is_policy_are_mutex_over_all`
    /// and
    /// `ephemeral_substrate_is_resource_and_substrate_is_telemetry_are_mutex_over_all`.
    #[test]
    fn ephemeral_substrate_is_policy_and_substrate_is_telemetry_are_mutex_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        assert!(
            !(eph.substrate_is_policy() && eph.substrate_is_telemetry()),
            "None-classification: substrate_is_policy AND substrate_is_telemetry both true (mutex violated)",
        );
        // Authored classification.
        for populated in SubstrateType::ALL {
            let mut classification = Classification::gate_compute();
            classification.substrate = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            assert!(
                !(eph.substrate_is_policy() && eph.substrate_is_telemetry()),
                "authored substrate={populated:?}: substrate_is_policy AND substrate_is_telemetry both true (mutex violated)",
            );
        }
    }

    /// THREE-WAY XOR PARTITION pin — for the absent-classification
    /// baseline AND every [`crate::classification::SubstrateType::ALL`]
    /// variant, EXACTLY ONE of [`Self::substrate_is_resource`],
    /// [`Self::substrate_is_policy`], and
    /// [`Self::substrate_is_telemetry`] returns `true`. CLOSES the
    /// three pairwise MUTEX pins on the substrate axis
    /// (`substrate_is_resource ⇒ ¬substrate_is_policy`,
    /// `substrate_is_resource ⇒ ¬substrate_is_telemetry`,
    /// `substrate_is_policy ⇒ ¬substrate_is_telemetry`) into the
    /// FULL ternary XOR partition contract on the ephemeral surface
    /// — the resolver-hop peer of the parent-composed
    /// `classification_substrate_probes_form_three_way_xor_partition_over_all`
    /// test. Structural twin of the sibling `point_type`-axis
    /// ternary lift sealed on this surface by
    /// `ephemeral_point_type_probes_form_three_way_xor_partition_over_all`.
    /// Guarantees the absent-classification case lands in the
    /// resource bucket (`gate_compute` → Compute → is_resource =
    /// true), so every unadorned `(defephemeral …)` audits under a
    /// definite non-empty plane bucket.
    #[test]
    fn ephemeral_substrate_probes_form_three_way_xor_partition_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        let buckets = [
            eph.substrate_is_resource(),
            eph.substrate_is_policy(),
            eph.substrate_is_telemetry(),
        ];
        let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
        assert_eq!(
            hits, 1,
            "None-classification: probes {buckets:?} — exactly one must be true (three-way XOR partition violated)",
        );
        // Authored classification.
        for populated in SubstrateType::ALL {
            let mut classification = Classification::gate_compute();
            classification.substrate = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let buckets = [
                eph.substrate_is_resource(),
                eph.substrate_is_policy(),
                eph.substrate_is_telemetry(),
            ];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "authored substrate={populated:?}: probes {buckets:?} — exactly one must be true (three-way XOR partition violated)",
            );
        }
    }

    // ── EphemeralSpec::calm_is_monotone pins ─────────────────────────
    //
    // Fail-before-pass-after granularity: `calm_is_monotone` did not
    // exist pre-lift on `impl EphemeralSpec` — every consumer walking
    // the "can this ephemeral spec participate in gossip-only writes?"
    // question went through the antisymmetric
    // `!self.calm_requires_coordination()` or through
    // `.resolved_classification().calm.is_monotone()`. Post-lift the
    // TWELFTH derived-nullary-boolean peer on the ephemeral surface
    // (SECOND on the calm axis, closing that axis into a binary XOR
    // partition on this surface) routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::calm_is_monotone`], so
    // the two-surface parity contract holds by construction, AND the
    // two calm-axis peers on this surface CLOSE the axis into the
    // FULL binary XOR partition contract via
    // `ephemeral_calm_probes_form_binary_xor_partition_over_all`.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific
    /// [`crate::classification::CalmClassification`] variant answers
    /// [`Self::calm_is_monotone`] matching the closed set's own
    /// [`crate::classification::CalmClassification::is_monotone`]
    /// truth table. Sweep
    /// [`crate::classification::CalmClassification::ALL`] so a
    /// regression that (a) hard-coded the body to a fixed answer,
    /// (b) inverted the projection, or (c) crossed the wires with
    /// the sibling
    /// [`crate::classification::CalmClassification::requires_coordination`]
    /// projection fails HERE at the substrate primitive before
    /// drifting through the `monotone-calm` fixed tag or the peer
    /// point surface.
    #[test]
    fn calm_is_monotone_returns_calm_projection_per_kind() {
        for populated in CalmClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.calm = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.calm_is_monotone(),
                populated.is_monotone(),
                "authored calm={populated:?}: calm_is_monotone() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// [`crate::classification::CalmClassification::default = Monotone`]
    /// via `#[default]`, and
    /// [`crate::classification::CalmClassification::Monotone::is_monotone`]
    /// projects `true`, so [`Self::calm_is_monotone`] returns
    /// `true`. Pins the resolver's default-arm short-circuit through
    /// TWO layers of `Default` ([`Classification::gate_compute`] →
    /// [`crate::classification::CalmClassification::default`])
    /// reaching this derived-nullary predicate. Mirror-inverted from
    /// the sibling
    /// `calm_requires_coordination_probes_false_on_absent_classification`
    /// (both walk the SAME defaulted `calm` field, so
    /// `requires_coordination = false` ⇒ `is_monotone = true` on the
    /// closed set's disjoint XOR partition). Guarantees every
    /// unadorned `(defephemeral …)` reads as gossip-eligible under
    /// the positive CALM framing.
    #[test]
    fn calm_is_monotone_probes_true_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            spec.calm_is_monotone(),
            "absent classification (defaults to gate_compute, calm=Monotone → is_monotone=true)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::calm_is_monotone`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.calm_is_monotone()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`crate::classification::CalmClassification::ALL`] variant) so
    /// a future regression on either side of the resolver fails HERE
    /// at the parity boundary. Byte-for-byte peer of
    /// `calm_requires_coordination_matches_point_peer_through_lowered_classification`
    /// on the SAME closed-set axis via the antisymmetric projection.
    #[test]
    fn calm_is_monotone_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.calm_is_monotone(),
            lowered.classification.calm_is_monotone(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in CalmClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.calm = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.calm_is_monotone(),
                lowered.classification.calm_is_monotone(),
                "authored calm={populated:?}: parity drift",
            );
        }
    }

    /// MUTEX pin — [`Self::calm_requires_coordination`] AND
    /// [`Self::calm_is_monotone`] are NEVER simultaneously true for
    /// ANY [`EphemeralSpec`] (authored or defaulted). FIRST
    /// ephemeral-surface `calm`-axis corner-peer MUTEX pin — the
    /// calm axis's counterpart to the sibling substrate-axis
    /// `ephemeral_substrate_is_resource_and_substrate_is_policy_are_mutex_over_all`
    /// on a binary (rather than ternary) closed set.
    #[test]
    fn ephemeral_calm_requires_coordination_and_calm_is_monotone_are_mutex_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        assert!(
            !(eph.calm_requires_coordination() && eph.calm_is_monotone()),
            "None-classification: calm_requires_coordination AND calm_is_monotone both true (mutex violated)",
        );
        // Authored classification.
        for populated in CalmClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.calm = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            assert!(
                !(eph.calm_requires_coordination() && eph.calm_is_monotone()),
                "authored calm={populated:?}: calm_requires_coordination AND calm_is_monotone both true (mutex violated)",
            );
        }
    }

    /// BINARY XOR PARTITION pin — for the absent-classification
    /// baseline AND every
    /// [`crate::classification::CalmClassification::ALL`] variant,
    /// EXACTLY ONE of [`Self::calm_is_monotone`] and
    /// [`Self::calm_requires_coordination`] returns `true`. CLOSES
    /// the calm-axis MUTEX pin
    /// (`calm_requires_coordination ⇒ ¬calm_is_monotone`) into the
    /// FULL binary XOR partition contract on the ephemeral surface
    /// — the resolver-hop peer of the parent-composed
    /// `classification_calm_probes_form_binary_xor_partition_over_all`
    /// test. Binary counterpart of the ternary XOR partitions sealed
    /// on the sibling `point_type` and `substrate` axes by
    /// `ephemeral_point_type_probes_form_three_way_xor_partition_over_all`
    /// and
    /// `ephemeral_substrate_probes_form_three_way_xor_partition_over_all`.
    /// Guarantees the absent-classification case lands in the
    /// monotone bucket (`gate_compute` → CalmClassification::Monotone
    /// → is_monotone = true), so every unadorned `(defephemeral …)`
    /// audits under a definite non-empty CALM bucket.
    #[test]
    fn ephemeral_calm_probes_form_binary_xor_partition_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        let buckets = [eph.calm_is_monotone(), eph.calm_requires_coordination()];
        let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
        assert_eq!(
            hits, 1,
            "None-classification: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
        );
        // Authored classification.
        for populated in CalmClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.calm = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let buckets = [eph.calm_is_monotone(), eph.calm_requires_coordination()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "authored calm={populated:?}: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
            );
        }
    }

    /// ANTISYMMETRY pin — [`Self::horizon_terminates`] XOR
    /// [`Self::horizon_requires_metric_axes`] holds on every
    /// [`EphemeralSpec`], authored or defaulted. Locks the
    /// composition-level XOR contract at ONE narrow ephemeral-surface
    /// site — a regression that crossed either surface's wires (one
    /// probe silently composing the wrong closed-set arm) surfaces
    /// here rather than at every downstream consumer that trusts the
    /// two probes partition the resolver's output.
    #[test]
    fn ephemeral_horizon_terminates_xor_horizon_requires_metric_axes() {
        // Absent classification.
        let eph = empty_ephemeral();
        assert!(
            eph.horizon_terminates() ^ eph.horizon_requires_metric_axes(),
            "None-classification: XOR must hold",
        );
        // Authored classification.
        for populated in HorizonKind::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon = crate::classification::Horizon {
                kind: populated,
                ..crate::classification::Horizon::default()
            };
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            assert!(
                eph.horizon_terminates() ^ eph.horizon_requires_metric_axes(),
                "authored horizon.kind={populated:?}: XOR must hold",
            );
        }
    }

    // ── EphemeralSpec::has_routing_form pins ─────────────────────────
    //
    // Fail-before-pass-after granularity: `has_routing_form` did not
    // exist pre-lift on `impl EphemeralSpec` — the point-surface
    // `routing-form-<kind>` prefix family in tatara-check routed
    // through `spec.routing.as_ref().is_some_and(|r| r.has_form(k))`
    // inline, so the ephemeral surface had no matching primitive to
    // publish the SAME `routing-form-<kind>` prefix family through
    // the `strip_and_classify_prefixed_kind` substrate. Post-lift the
    // Option-gated derived-scalar-child probe body lives at ONE
    // inherent site on [`EphemeralSpec`] and every consumer (this
    // module's peer-symmetry tests, tatara-check's ephemeral
    // require-tag classifier, any future audit dispatcher walking
    // [`RoutingForm::ALL`] over the ephemeral surface) binds through
    // the SAME `has_routing_form(kind)` shape.

    fn routing_spec(is_stable: bool) -> RoutingSpec {
        use crate::routing::{RoutingBackend, RoutingHostname};
        RoutingSpec {
            hostnames: vec![RoutingHostname::content_hashed("api")],
            backend: RoutingBackend::plain("svc", 80),
            stable_name_claim: is_stable,
            priority: 0,
        }
    }

    /// POPULATED-slot pin — a populated `routing` slot answers `true`
    /// exactly for the [`RoutingForm`] variant its
    /// [`RoutingSpec::has_form`] derived-scalar arm agrees with, and
    /// `false` for every other variant. Sweep the two-boolean × ALL
    /// cross so a regression that (a) hard-coded the arm to a single
    /// variant, (b) dropped the Option-parent gate (silently reading
    /// through `.unwrap_or_default()` on an absent routing slot), or
    /// (c) crossed the wires from
    /// [`RoutingForm::from_is_stable`] to a fixed variant fails
    /// HERE before landing at the operator-facing checks.lisp
    /// surface.
    #[test]
    fn has_routing_form_returns_true_iff_populated_routing_derives_form_per_kind() {
        for is_stable in [true, false] {
            let populated = RoutingForm::from_is_stable(is_stable);
            let mut spec = empty_ephemeral();
            spec.routing = Some(routing_spec(is_stable));
            for query in RoutingForm::ALL {
                let expected = query == populated;
                assert_eq!(
                    spec.has_routing_form(query),
                    expected,
                    "ephemeral routing.stable_name_claim={is_stable} (derives {populated:?}): query {query:?} drifted",
                );
            }
        }
    }

    /// OPTION-PARENT SHORT-CIRCUIT pin — an [`EphemeralSpec`] whose
    /// `routing` slot is `None` returns `false` for every
    /// [`RoutingForm`] variant, INCLUDING the closed set's
    /// derived-default [`RoutingForm::Instance`]. Locks the
    /// Option-parent silencing contract so a regression that dropped
    /// the `spec.routing.as_ref()` gate (silently probing an absent
    /// routing slot as if it carried the defaulted `Instance` form)
    /// fails HERE. Peer to
    /// [`evaluate_point_require_tag_returns_false_on_absent_routing_for_every_routing_form_kind`]
    /// on the point surface — the two-surface symmetry means both
    /// classifiers publish the SAME Option-parent silencing at ONE
    /// substrate site per surface.
    #[test]
    fn has_routing_form_returns_false_on_absent_routing_for_every_kind() {
        let spec = empty_ephemeral();
        assert!(spec.routing.is_none());
        for kind in RoutingForm::ALL {
            assert!(
                !spec.has_routing_form(kind),
                "absent ephemeral routing must return false for {kind:?}",
            );
        }
    }

    /// DEFAULT-ARM SHORT-CIRCUIT pin — an [`EphemeralSpec`] whose
    /// `routing` slot is a [`RoutingSpec`] with `stable_name_claim`
    /// at its `#[serde(default)]` (bool default = `false`) answers
    /// `true` on [`RoutingForm::Instance`] and `false` on every other
    /// variant WITHOUT the operator naming the routing-form axis on
    /// the routing spec. Peer to
    /// [`evaluate_point_require_tag_returns_true_on_default_routing_form_for_instance_only`]
    /// on the point surface — both surfaces read the derived-child
    /// arm through the ONE substrate composer
    /// [`RoutingForm::from_is_stable`], so a future normalization at
    /// the derivation lands at ONE site and every downstream
    /// (routing-form require-tag families on both surfaces,
    /// closed-set audit dispatchers) picks it up mechanically.
    #[test]
    fn has_routing_form_probes_instance_only_on_default_populated_routing() {
        let mut spec = empty_ephemeral();
        spec.routing = Some(routing_spec(bool::default()));
        for kind in RoutingForm::ALL {
            let expected = kind == RoutingForm::Instance;
            assert_eq!(
                spec.has_routing_form(kind),
                expected,
                "default-populated ephemeral routing (stable_name_claim=false → Instance) baseline: query {kind:?} must be {expected}",
            );
        }
    }

    /// TWO-SURFACE SYMMETRY pin — an [`EphemeralSpec`] and the
    /// [`ProcessSpec`] it lowers to through `From<EphemeralSpec>`
    /// answer identically on every [`RoutingForm`] × `is_stable`
    /// combination. Locks the byte-for-byte parity between
    /// [`EphemeralSpec::has_routing_form`] (this new primitive) and
    /// the point surface's `spec.routing.as_ref().is_some_and(|r|
    /// r.has_form(k))` inline projection at the tatara-check dispatch
    /// site. A regression that (a) diverged the ephemeral probe from
    /// the lowered point probe (e.g., dropped the Option-parent gate
    /// on ONE side, crossed the derived-child arm on the OTHER), or
    /// (b) diverged the `From<EphemeralSpec>` lowering's
    /// `routing: e.routing` copy from byte-for-byte forwarding, fails
    /// HERE at the two-surface boundary.
    #[test]
    fn has_routing_form_matches_point_peer_through_lowered_routing() {
        for is_stable in [true, false] {
            let mut authored = empty_ephemeral();
            authored.routing = Some(routing_spec(is_stable));
            let lowered: ProcessSpec = authored.clone().into();
            for kind in RoutingForm::ALL {
                let ephemeral_answer = authored.has_routing_form(kind);
                let point_answer = lowered.routing.as_ref().is_some_and(|r| r.has_form(kind));
                assert_eq!(
                    ephemeral_answer, point_answer,
                    "two-surface routing-form parity drift: stable_name_claim={is_stable}, kind={kind:?}",
                );
            }
        }
    }

    // ── EphemeralSpec::has_applicable_exports_at substrate pins ───────
    //
    // Fail-before-pass-after granularity: `has_applicable_exports_at`
    // did not exist pre-lift on `impl EphemeralSpec` — the peer
    // `EphemeralLifetime::has_applicable_exports` on the lowered
    // `ProcessSpec` surface routed through the compound
    // `.iter().any(|e| e.when.fires_on(phase))` chain inline, so the
    // sugar surface had no matching primitive to publish an
    // `exports-fire-on-<phase>` prefix family through the
    // `strip_and_classify_prefixed_kind` substrate. Post-lift the
    // compound-`(when, phase) → fires_on(phase)` probe body lives at
    // ONE slice-level substrate site (`ExportSpecSliceExt::has_applicable_at`),
    // this ephemeral surface routes through it directly, and the
    // point surface reaches the same primitive through
    // `spec.lifetime.resolved_ephemeral().is_some_and(|e|
    // e.exports.has_applicable_at(phase))`.

    fn export_at(when: crate::export::ExportTrigger) -> ExportSpec {
        use crate::export::{ArtifactSource, ReceiptsSource, StdoutChannel, VectorChannel};
        ExportSpec {
            source: ArtifactSource {
                receipts: Some(ReceiptsSource::default()),
                ..ArtifactSource::default()
            },
            channel: VectorChannel {
                stdout: Some(StdoutChannel::default()),
                ..VectorChannel::default()
            },
            when,
            experiment_id_override: None,
        }
    }

    /// EMPTY-EXPORTS pin — an ephemeral spec with an empty `exports`
    /// vec returns `false` for EVERY [`ProcessPhase`]. Sweep
    /// [`ProcessPhase::ALL`] so a new variant added without a matching
    /// arm in [`crate::export::ExportTrigger::fires_on`] surfaces at
    /// rustc's exhaustiveness gate on the `ALL` literal (arity forced
    /// by `[Self; 11]`) rather than as a silent false-positive at
    /// every downstream `exports-fire-on-<phase>` ephemeral require-tag
    /// callsite.
    #[test]
    fn has_applicable_exports_at_returns_false_on_empty_exports_for_every_phase() {
        let spec = empty_ephemeral();
        assert!(spec.exports.is_empty());
        for phase in ProcessPhase::ALL {
            assert!(
                !spec.has_applicable_exports_at(phase),
                "empty-exports ephemeral must return false for {phase:?}",
            );
        }
    }

    /// PER-TRIGGER × PER-PHASE pin — an ephemeral spec with a single
    /// export answers `has_applicable_exports_at` identically to the
    /// [`crate::export::ExportTrigger::fires_on`] truth table on that
    /// (trigger, phase) pair, for every combination. Sweep the
    /// [`crate::export::ExportTrigger::ALL`] × [`ProcessPhase::ALL`]
    /// cross so a regression that (a) short-circuited to raw `when ==
    /// kind` equality, (b) missed `Always`'s dual-phase coverage, or
    /// (c) inverted a non-terminal phase to return `true` fails HERE
    /// at the substrate primitive rather than at each downstream
    /// `exports-fire-on-<phase>` classifier callsite.
    #[test]
    fn has_applicable_exports_at_matches_fires_on_truth_table_per_pair() {
        for trigger in crate::export::ExportTrigger::ALL {
            let mut spec = empty_ephemeral();
            spec.exports = vec![export_at(trigger)];
            for phase in ProcessPhase::ALL {
                let expected = trigger.fires_on(phase);
                assert_eq!(
                    spec.has_applicable_exports_at(phase),
                    expected,
                    "ephemeral trigger={trigger:?} phase={phase:?} drifted from fires_on",
                );
            }
        }
    }

    /// TWO-SURFACE SYMMETRY pin — an [`EphemeralSpec`] and the
    /// [`ProcessSpec`] it lowers to through `From<EphemeralSpec>`
    /// answer identically on every [`ProcessPhase`] × trigger
    /// combination. Locks the byte-for-byte parity between
    /// [`EphemeralSpec::has_applicable_exports_at`] (this new primitive)
    /// and the point surface's `spec.lifetime.resolved_ephemeral()
    /// .is_some_and(|e| e.exports.has_applicable_at(phase))` projection
    /// at the tatara-check dispatch site. A regression that (a)
    /// diverged the ephemeral probe from the lowered-lifetime probe,
    /// (b) diverged the `From<EphemeralSpec>` lowering's
    /// `exports: e.exports` copy from byte-for-byte forwarding, fails
    /// HERE at the two-surface boundary.
    #[test]
    fn has_applicable_exports_at_matches_point_peer_through_lowered_exports() {
        for trigger in crate::export::ExportTrigger::ALL {
            let mut authored = empty_ephemeral();
            authored.exports = vec![export_at(trigger)];
            let lowered: ProcessSpec = authored.clone().into();
            for phase in ProcessPhase::ALL {
                let ephemeral_answer = authored.has_applicable_exports_at(phase);
                let point_answer = lowered
                    .lifetime
                    .resolved_ephemeral()
                    .is_some_and(|e| e.exports.has_applicable_at(phase));
                assert_eq!(
                    ephemeral_answer, point_answer,
                    "two-surface exports-fire-on parity drift: trigger={trigger:?}, phase={phase:?}",
                );
            }
        }
    }
}
