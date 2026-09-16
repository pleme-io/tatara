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
    CalmClassification, Classification, ConvergencePointType, DataClassification, SubstrateType,
};
use crate::crd::ProcessSpec;
use crate::export::ExportSpec;
use crate::intent::{AplicacaoIntent, Intent};
use crate::lifetime::{EphemeralLifetime, Lifetime, TeardownPolicy};
use crate::routing::RoutingSpec;

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
    /// [`Self::has_data_classification`], and its future peers on the
    /// four remaining classification axes — `has_horizon_kind`,
    /// `has_optimization_direction`, `has_input_arity`,
    /// `has_output_arity`) routes through THIS
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
    /// [`Self::has_data_classification`] the FOURTH; then
    /// `has_horizon_kind`, `has_optimization_direction`,
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
    /// [`Self::has_data_classification`] the FOURTH; then
    /// `has_horizon_kind`, `has_optimization_direction`,
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
    /// opened the FOURTH; then `has_horizon_kind`,
    /// `has_optimization_direction`, `has_input_arity`,
    /// `has_output_arity`) land as one-line wrappers around the SAME
    /// resolver + the sibling [`Classification`] closed-set
    /// primitive, so a future variant added to
    /// [`CalmClassification`] (or any of the five other closed sets)
    /// reaches BOTH surfaces' `<axis>-<kind>` prefix families through
    /// the SAME closed-set walk with no per-caller edit.
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
    /// `Cow`-resolver carrier (`has_horizon_kind`,
    /// `has_optimization_direction`, `has_input_arity`,
    /// `has_output_arity`) land as one-line wrappers around the SAME
    /// resolver + the sibling [`Classification`] closed-set
    /// primitive, so a future variant added to
    /// [`DataClassification`] (or any of the four other closed sets)
    /// reaches BOTH surfaces' `<axis>-<kind>` prefix families through
    /// the SAME closed-set walk with no per-caller edit.
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
        CalmClassification, ConvergencePointType, DataClassification, SubstrateType,
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
}
