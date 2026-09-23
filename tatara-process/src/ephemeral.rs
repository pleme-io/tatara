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
    Arity, CalmClassification, Classification, ClassificationAxis, ConvergencePointType,
    DataClassification, HorizonKind, OptimizationDirection, SubstrateType,
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
        self.has_precondition_kind(kind) || self.has_postcondition_kind(kind)
    }

    /// True iff at least one [`Condition`] in `self.preconditions`
    /// carries the given [`ConditionKind`] — the precondition-side arm
    /// of the (precondition, postcondition, condition-union) triad on
    /// [`EphemeralSpec`], sibling to [`Self::has_postcondition_kind`]
    /// and half-composition of [`Self::has_condition_kind`].
    ///
    /// Thin typed delegate to [`ConditionSliceExt::has_kind`] over
    /// [`Self::preconditions`]. Peer of
    /// [`crate::boundary::Boundary::has_precondition_kind`] on the
    /// point-domain surface — both peers compose against the SAME
    /// slice-level substrate primitive
    /// ([`crate::boundary::ConditionSliceExt::has_kind`]) so a
    /// regression at the per-slice presence probe fails at that
    /// primitive's tests rather than as silent drift at either
    /// struct-level half-slice arm.
    ///
    /// # Why lift
    ///
    /// See [`crate::boundary::Boundary::has_precondition_kind`] for
    /// the full rationale — the two surfaces (point + ephemeral)
    /// publish their `precondition-<kind>` / `postcondition-<kind>`
    /// require-tag prefix families byte-for-byte symmetrical, each
    /// through its own struct-level half-slice arm. Post-lift the
    /// (precondition, postcondition, condition-union) triad lives at
    /// ONE typed algebra surface per struct rather than at a mixed
    /// (union-arm-via-method, half-slice-arms-via-direct-field-access)
    /// asymmetry on the ephemeral side.
    ///
    /// # Semantics — byte-identical to the point-domain peer
    ///
    /// Returns `true` iff `self.preconditions.iter().any(|c| c.kind ==
    /// kind)`. Ignores `self.postconditions` — an operator who
    /// authored the kind on ONLY postconditions gets `false` from this
    /// probe and `true` from [`Self::has_postcondition_kind`]. The two
    /// half-slice arms partition the (kind, side) matrix exhaustively
    /// across the four states (kind absent both, pre-only, post-only,
    /// both).
    #[must_use]
    pub fn has_precondition_kind(&self, kind: ConditionKind) -> bool {
        self.preconditions.has_kind(kind)
    }

    /// True iff at least one [`Condition`] in `self.postconditions`
    /// carries the given [`ConditionKind`] — the postcondition-side arm
    /// of the (precondition, postcondition, condition-union) triad on
    /// [`EphemeralSpec`], sibling to [`Self::has_precondition_kind`]
    /// and half-composition of [`Self::has_condition_kind`].
    ///
    /// Thin typed delegate to [`ConditionSliceExt::has_kind`] over
    /// [`Self::postconditions`]. Peer of
    /// [`crate::boundary::Boundary::has_postcondition_kind`] on the
    /// point-domain surface. See [`Self::has_precondition_kind`] for
    /// the full rationale — both half-slice arms share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin, and
    /// ONE two-surface parity contract with the point-domain
    /// [`crate::boundary::Boundary`] peer methods.
    #[must_use]
    pub fn has_postcondition_kind(&self, kind: ConditionKind) -> bool {
        self.postconditions.has_kind(kind)
    }

    /// Returns the first [`Condition`] in
    /// `preconditions ∪ postconditions` carrying the given
    /// [`ConditionKind`], searching preconditions first — the peer of
    /// [`crate::boundary::Boundary::find_condition_kind`] on the
    /// [`EphemeralSpec`] sugar surface.
    ///
    /// # Semantics — byte-identical to [`Boundary::find_condition_kind`]
    ///
    /// Walks `self.preconditions` first, then `self.postconditions`:
    /// a kind authored on BOTH sides returns the precondition-side
    /// [`Condition`]. Composition law:
    /// `find_condition_kind(K) == find_precondition_kind(K).or_else(||
    /// find_postcondition_kind(K))`, pinned as a first-class typed
    /// invariant. Both halves compose through the SAME slice-level
    /// substrate primitive [`crate::boundary::ConditionSliceExt::find_kind`]
    /// that [`Boundary::find_condition_kind`] walks — so a regression
    /// at the per-slice walk fails at that primitive's tests rather
    /// than as silent drift at either struct-level widened caller.
    ///
    /// # Sibling to [`Self::has_condition_kind`]
    ///
    /// Same axis, one refinement wider: `has_condition_kind` collapses
    /// the return to a `bool` (`find_condition_kind(k).is_some()`);
    /// this method returns the matching `&Condition` so consumers can
    /// read [`Condition::params`] at the presence-probe callsite
    /// without re-walking the two condition vectors. Pinned by the
    /// composition law
    /// `has_condition_kind(K) == find_condition_kind(K).is_some()`.
    ///
    /// # Compounding
    ///
    /// A future diagnostic consumer on the ephemeral surface (an
    /// operator-facing "closed-loop-auth matched with
    /// params.probeImage=X" message emitted by the ephemeral require-
    /// tag classifier, a coherence check on the ephemeral surface that
    /// verifies "every `ClosedLoopAuth` postcondition carries a non-
    /// empty `probeImage`", an editor completion listing params-keys
    /// per present ephemeral kind) reaches for the matching
    /// [`Condition`] through this ONE method rather than re-walking
    /// the two vectors at the callsite. Byte-for-byte peer of the
    /// point-domain widened triad on [`Boundary`], so the two-surface
    /// parity contract now covers both refinements (bool via has,
    /// `&Condition` via find) on the condition axis.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the widened union body composes the SAME
    /// slice-level substrate primitive on both this ephemeral surface
    /// and the point-domain [`Boundary`] surface). THEORY.md §VI.1
    /// (generation over composition — a future [`ConditionKind`]
    /// variant added to `ALL` reaches both surfaces' widened triads
    /// mechanically through the SAME closed-set walk).
    #[must_use]
    pub fn find_condition_kind(&self, kind: ConditionKind) -> Option<&Condition> {
        self.find_precondition_kind(kind)
            .or_else(|| self.find_postcondition_kind(kind))
    }

    /// Returns the first [`Condition`] in [`Self::preconditions`]
    /// carrying the given [`ConditionKind`], or `None` — the
    /// precondition-side arm of the (precondition, postcondition,
    /// condition-union) widened triad on [`EphemeralSpec`]. Thin typed
    /// delegate to [`crate::boundary::ConditionSliceExt::find_kind`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of [`crate::boundary::Boundary::find_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice walk fails at that primitive's tests rather than as
    /// silent drift at either struct-level widened half-slice arm.
    /// Byte-identical semantics to [`Self::has_precondition_kind`]
    /// with a widened `Option<&Condition>` return rather than a
    /// `bool`.
    #[must_use]
    pub fn find_precondition_kind(&self, kind: ConditionKind) -> Option<&Condition> {
        self.preconditions.find_kind(kind)
    }

    /// Returns the first [`Condition`] in [`Self::postconditions`]
    /// carrying the given [`ConditionKind`], or `None` — the
    /// postcondition-side arm of the (precondition, postcondition,
    /// condition-union) widened triad on [`EphemeralSpec`]. Thin typed
    /// delegate to [`crate::boundary::ConditionSliceExt::find_kind`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of [`crate::boundary::Boundary::find_postcondition_kind`]
    /// on the point-domain surface. See [`Self::find_precondition_kind`]
    /// for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin, and
    /// ONE two-surface parity contract with the point-domain
    /// [`crate::boundary::Boundary`] widened peer methods.
    #[must_use]
    pub fn find_postcondition_kind(&self, kind: ConditionKind) -> Option<&Condition> {
        self.postconditions.find_kind(kind)
    }

    /// Returns an iterator over every [`Condition`] in
    /// `preconditions ∪ postconditions` carrying the given
    /// [`ConditionKind`], walking preconditions first — the peer of
    /// [`crate::boundary::Boundary::iter_condition_kind`] on the
    /// [`EphemeralSpec`] sugar surface.
    ///
    /// # Semantics — byte-identical to [`Boundary::iter_condition_kind`]
    ///
    /// Chains [`Self::iter_precondition_kind`] with
    /// [`Self::iter_postcondition_kind`] via [`Iterator::chain`]:
    /// yields every precondition-side match in slice order, then
    /// every postcondition-side match in slice order. Composition
    /// law:
    /// `find_condition_kind(K) == iter_condition_kind(K).next()`,
    /// pinned as a first-class typed invariant. Both halves compose
    /// through the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::iter_kind`] that
    /// [`Boundary::iter_condition_kind`] chains — so a regression at
    /// the per-slice walk fails at that primitive's tests rather than
    /// as silent drift at either struct-level widened caller.
    ///
    /// # Sibling to [`Self::find_condition_kind`]
    ///
    /// Same axis, one refinement wider: `find_condition_kind`
    /// collapses the return to the FIRST match; this method yields
    /// every match across both sides. Byte-for-byte peer of the
    /// point-domain widened triad on [`Boundary`], so the two-surface
    /// parity contract now covers three refinements (bool via has,
    /// `&Condition` via find, `impl Iterator<Item = &Condition>` via
    /// iter) on the condition axis.
    ///
    /// # Compounding
    ///
    /// A future ephemeral-surface coherence check that enforces
    /// "each [`ConditionKind`] appears at most once across
    /// preconditions ∪ postconditions" reads
    /// `spec.iter_condition_kind(k).nth(1).is_none()` at ONE call
    /// site. A future ephemeral require-tag classifier arm that
    /// counts matches (a hypothetical `condition-count-<kind>` prefix
    /// family that surfaces multiplicity to the operator) reaches
    /// this ONE method through `spec.iter_condition_kind(k).count()`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the widened stream body composes the SAME
    /// slice-level substrate primitive on both this ephemeral surface
    /// and the point-domain [`Boundary`] surface). THEORY.md §VI.1
    /// (generation over composition — a future [`ConditionKind`]
    /// variant added to `ALL` reaches both surfaces' iterator triads
    /// mechanically through the SAME closed-set walk).
    pub fn iter_condition_kind(
        &self,
        kind: ConditionKind,
    ) -> std::iter::Chain<crate::boundary::KindMatches<'_>, crate::boundary::KindMatches<'_>> {
        self.iter_precondition_kind(kind)
            .chain(self.iter_postcondition_kind(kind))
    }

    /// Returns an iterator over every [`Condition`] in
    /// [`Self::preconditions`] carrying the given [`ConditionKind`]
    /// — the precondition-side arm of the (precondition,
    /// postcondition, condition-union) iterator triad on
    /// [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::iter_kind`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`crate::boundary::Boundary::iter_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice walk fails at that primitive's tests rather than as
    /// silent drift at either struct-level widened half-slice arm.
    /// Byte-identical semantics to [`Self::find_precondition_kind`]
    /// with a widened stream return rather than only the first match.
    pub fn iter_precondition_kind(&self, kind: ConditionKind) -> crate::boundary::KindMatches<'_> {
        self.preconditions.iter_kind(kind)
    }

    /// Returns an iterator over every [`Condition`] in
    /// [`Self::postconditions`] carrying the given [`ConditionKind`]
    /// — the postcondition-side arm of the (precondition,
    /// postcondition, condition-union) iterator triad on
    /// [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::iter_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`crate::boundary::Boundary::iter_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::iter_precondition_kind`] for the full rationale — the
    /// two methods share ONE lift motivation, ONE fail-before-
    /// pass-after composition-law pin, and ONE two-surface parity
    /// contract with the point-domain [`crate::boundary::Boundary`]
    /// widened peer methods.
    pub fn iter_postcondition_kind(&self, kind: ConditionKind) -> crate::boundary::KindMatches<'_> {
        self.postconditions.iter_kind(kind)
    }

    /// Number of [`Condition`]s in `preconditions ∪ postconditions`
    /// carrying the given [`ConditionKind`] — the peer of
    /// [`crate::boundary::Boundary::count_condition_kind`] on the
    /// [`EphemeralSpec`] sugar surface.
    ///
    /// # Semantics — byte-identical to [`Boundary::count_condition_kind`]
    ///
    /// Composed as
    /// `count_precondition_kind(k) + count_postcondition_kind(k)` —
    /// the SUM-composed arm on the presence-probe algebra (distinct
    /// from `has_condition_kind`'s `||`, `find_condition_kind`'s
    /// `or_else`, and `iter_condition_kind`'s `Chain`). Composition
    /// law `count_condition_kind(K) == iter_condition_kind(K).count()`
    /// pinned as a first-class typed invariant. Both halves compose
    /// through the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::count_kind`] that
    /// [`Boundary::count_condition_kind`] sums — so a regression at
    /// the per-slice count fails at that primitive's tests rather
    /// than as silent drift at either struct-level widened caller.
    ///
    /// # Sibling to [`Self::iter_condition_kind`]
    ///
    /// Same axis, one refinement lower on the cardinality projection:
    /// `iter_condition_kind` yields the whole match stream; this
    /// method collapses that stream to its cardinality. Byte-for-byte
    /// peer of the point-domain count triad on [`Boundary`], so the
    /// two-surface parity contract now covers four refinements (bool
    /// via has, `&Condition` via find, `impl Iterator<Item =
    /// &Condition>` via iter, `usize` via count) on the condition
    /// axis.
    ///
    /// # Compounding
    ///
    /// A future ephemeral-surface coherence check that enforces
    /// "each [`ConditionKind`] appears at most once across
    /// preconditions ∪ postconditions" reads
    /// `spec.count_condition_kind(k) <= 1` at ONE call site. A future
    /// ephemeral require-tag classifier arm that surfaces multiplicity
    /// to the operator (a hypothetical `condition-count-<kind>` prefix
    /// family that publishes the raw cardinality on the ephemeral
    /// surface, an operator-facing "3 ClosedLoopAuth postconditions
    /// matched" message) reaches this ONE method rather than restating
    /// the `.iter_condition_kind(k).count()` chain body at the
    /// callsite.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the scalar cardinality body composes the
    /// SAME slice-level substrate primitive on both this ephemeral
    /// surface and the point-domain [`Boundary`] surface). THEORY.md
    /// §VI.1 (generation over composition — a future
    /// [`ConditionKind`] variant added to `ALL` reaches both surfaces'
    /// count triads mechanically through the SAME closed-set walk).
    #[must_use]
    pub fn count_condition_kind(&self, kind: ConditionKind) -> usize {
        self.count_precondition_kind(kind) + self.count_postcondition_kind(kind)
    }

    /// Number of [`Condition`]s in [`Self::preconditions`] carrying
    /// the given [`ConditionKind`] — the precondition-side arm of the
    /// (precondition, postcondition, condition-union) count triad on
    /// [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::count_kind`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`crate::boundary::Boundary::count_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice count fails at that primitive's tests rather than as
    /// silent drift at either struct-level count arm.
    #[must_use]
    pub fn count_precondition_kind(&self, kind: ConditionKind) -> usize {
        self.preconditions.count_kind(kind)
    }

    /// Number of [`Condition`]s in [`Self::postconditions`] carrying
    /// the given [`ConditionKind`] — the postcondition-side arm of
    /// the (precondition, postcondition, condition-union) count triad
    /// on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::count_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`crate::boundary::Boundary::count_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::count_precondition_kind`] for the full rationale — the
    /// two methods share ONE lift motivation, ONE fail-before-
    /// pass-after composition-law pin, and ONE two-surface parity
    /// contract with the point-domain [`crate::boundary::Boundary`]
    /// count peer methods.
    #[must_use]
    pub fn count_postcondition_kind(&self, kind: ConditionKind) -> usize {
        self.postconditions.count_kind(kind)
    }

    /// The set of [`ConditionKind`] variants appearing at least once in
    /// `preconditions ∪ postconditions`, projected in
    /// [`ConditionKind::ALL`] order — the peer of
    /// [`crate::boundary::Boundary::distinct_condition_kinds`] on the
    /// [`EphemeralSpec`] sugar surface.
    ///
    /// # Semantics — byte-identical to [`crate::boundary::Boundary::distinct_condition_kinds`]
    ///
    /// Composed as `ConditionKind::ALL.into_iter().filter(|k|
    /// self.has_condition_kind(*k)).collect()` — the ONE closed-set-
    /// inversion arm on the presence-probe algebra (distinct in axis
    /// from the four point-probe arms `has_condition_kind` /
    /// `find_condition_kind` / `iter_condition_kind` /
    /// `count_condition_kind` which fix a [`ConditionKind`] and vary
    /// the return type). Equivalent to the set-union of
    /// [`Self::distinct_precondition_kinds`] and
    /// [`Self::distinct_postcondition_kinds`] projected in canonical
    /// [`ConditionKind::ALL`] order.
    ///
    /// # Peer on the point surface — [`crate::boundary::Boundary::distinct_condition_kinds`]
    ///
    /// Same signature `(&Self) -> Vec<ConditionKind>`, same closed-set-
    /// inversion body, on the point-domain [`crate::boundary::Boundary`]
    /// nested-slot carrier. Both methods compose against the SAME
    /// slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::distinct_kinds`] via the
    /// two-slice union composed through [`Self::has_condition_kind`] —
    /// a regression at the per-slice walk fails at that primitive's
    /// tests rather than as silent drift at either struct-level union
    /// caller.
    ///
    /// # Sibling to the four point-probe refinements
    ///
    /// FIFTH refinement on the ephemeral-surface presence-probe algebra,
    /// distinct in axis from the other four. The composition law
    /// `distinct_condition_kinds().contains(&k) == has_condition_kind(k)`
    /// for every `k ∈ ConditionKind::ALL` binds the closed-set-inversion
    /// probe to the point probe at the (precondition, postcondition,
    /// condition-union) triad. The two-surface parity contract now
    /// covers FIVE refinements (bool / `&Condition` / `impl Iterator` /
    /// `usize` / `Vec<ConditionKind>` closed-set-inversion) on the
    /// condition axis, byte-for-byte peer of the point-domain triad on
    /// [`crate::boundary::Boundary`].
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — the closed-set-inversion aggregate composes the SAME
    /// slice-level substrate primitive on both this ephemeral surface
    /// and the point-domain [`crate::boundary::Boundary`] surface).
    /// THEORY.md §VI.1 (generation over composition — a future
    /// [`ConditionKind`] variant added to `ALL` reaches both surfaces'
    /// distinct-set triads mechanically through the SAME closed-set
    /// walk).
    #[must_use]
    pub fn distinct_condition_kinds(&self) -> Vec<ConditionKind> {
        ConditionKind::ALL
            .into_iter()
            .filter(|k| self.has_condition_kind(*k))
            .collect()
    }

    /// The set of [`ConditionKind`] variants appearing at least once in
    /// [`Self::preconditions`], projected in [`ConditionKind::ALL`]
    /// order — the precondition-side arm of the (precondition,
    /// postcondition, condition-union) distinct-set triad on
    /// [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::distinct_kinds`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`crate::boundary::Boundary::distinct_precondition_kinds`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice closed-set walk fails at that primitive's tests
    /// rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn distinct_precondition_kinds(&self) -> Vec<ConditionKind> {
        self.preconditions.distinct_kinds()
    }

    /// The set of [`ConditionKind`] variants appearing at least once in
    /// [`Self::postconditions`], projected in [`ConditionKind::ALL`]
    /// order — the postcondition-side arm of the (precondition,
    /// postcondition, condition-union) distinct-set triad on
    /// [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::distinct_kinds`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`crate::boundary::Boundary::distinct_postcondition_kinds`]
    /// on the point-domain surface. See
    /// [`Self::distinct_precondition_kinds`] for the full rationale —
    /// the two methods share ONE lift motivation, ONE fail-before-
    /// pass-after composition-law pin, and ONE two-surface parity
    /// contract with the point-domain
    /// [`crate::boundary::Boundary`] distinct-set peer methods.
    #[must_use]
    pub fn distinct_postcondition_kinds(&self) -> Vec<ConditionKind> {
        self.postconditions.distinct_kinds()
    }

    /// Zero-allocation iterator peer of [`Self::distinct_condition_kinds`]
    /// — the condition-union arm of the (precondition, postcondition,
    /// condition-union) closed-set-inversion iterator triad on
    /// [`EphemeralSpec`]. Byte-identical to
    /// [`crate::boundary::Boundary::iter_distinct_condition_kinds`] on the
    /// point-domain surface. Walks [`ConditionKind::ALL`] in canonical
    /// order and yields every [`ConditionKind`] appearing at least once in
    /// `preconditions ∪ postconditions`, WITHOUT materializing an
    /// intermediate `Vec<ConditionKind>`.
    pub fn iter_distinct_condition_kinds(&self) -> impl Iterator<Item = ConditionKind> + '_ {
        ConditionKind::ALL
            .iter()
            .copied()
            .filter(|&k| self.has_condition_kind(k))
    }

    /// Zero-allocation iterator peer of
    /// [`Self::distinct_precondition_kinds`] — the precondition-side arm
    /// of the (precondition, postcondition, condition-union) closed-set-
    /// inversion iterator triad on [`EphemeralSpec`]. Thin typed delegate
    /// to [`crate::boundary::ConditionSliceExt::iter_distinct_kinds`] over
    /// [`Self::preconditions`].
    pub fn iter_distinct_precondition_kinds(&self) -> impl Iterator<Item = ConditionKind> + '_ {
        self.preconditions.iter_distinct_kinds()
    }

    /// Zero-allocation iterator peer of
    /// [`Self::distinct_postcondition_kinds`] — the postcondition-side
    /// arm of the (precondition, postcondition, condition-union) closed-
    /// set-inversion iterator triad on [`EphemeralSpec`]. Thin typed
    /// delegate to
    /// [`crate::boundary::ConditionSliceExt::iter_distinct_kinds`] over
    /// [`Self::postconditions`].
    pub fn iter_distinct_postcondition_kinds(&self) -> impl Iterator<Item = ConditionKind> + '_ {
        self.postconditions.iter_distinct_kinds()
    }

    /// Scalar cardinality of the [`ConditionKind`] set appearing at
    /// least once in `preconditions ∪ postconditions` — the peer of
    /// [`crate::boundary::Boundary::distinct_condition_kind_count`] on
    /// the [`EphemeralSpec`] sugar surface.
    ///
    /// # Composed body — byte-identical to
    /// [`crate::boundary::Boundary::distinct_condition_kind_count`]
    ///
    /// `ConditionKind::ALL.iter().filter(|k|
    /// self.has_condition_kind(**k)).count()` — the scalar cardinality
    /// projection of [`Self::distinct_condition_kinds`] onto its
    /// `.len()`, without materializing the intermediate
    /// `Vec<ConditionKind>`. Byte-identical to the peer method on the
    /// point-domain [`crate::boundary::Boundary`] surface — both
    /// compose against the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::distinct_kind_count`] via
    /// the two-slice union composed through [`Self::has_condition_kind`]
    /// so a regression at the per-slice closed-set walk fails at that
    /// primitive's tests rather than as silent drift at either
    /// struct-level scalar-cardinality caller.
    ///
    /// # Sibling to [`Self::distinct_condition_kinds`]
    ///
    /// Scalar projection of the closed-set-inversion widened primitive
    /// on the ephemeral-union surface — where `distinct_condition_kinds`
    /// returns the SET, `distinct_condition_kind_count` collapses it to
    /// its cardinality. The two-surface parity contract now covers SIX
    /// refinements (bool / `&Condition` / `impl Iterator` / `usize` /
    /// `Vec<ConditionKind>` closed-set-inversion / `usize` scalar
    /// cardinality of the closed-set-inversion) on the condition axis,
    /// byte-for-byte peer of the point-domain triad on
    /// [`crate::boundary::Boundary`].
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — the scalar cardinality composes the SAME closed-set
    /// walk on both this ephemeral surface and the point-domain
    /// [`crate::boundary::Boundary`] surface). THEORY.md §VI.1
    /// (generation over composition — a future [`ConditionKind`] variant
    /// added to `ALL` reaches both surfaces' distinct-kind-count triads
    /// mechanically through the SAME closed-set walk).
    #[must_use]
    pub fn distinct_condition_kind_count(&self) -> usize {
        ConditionKind::ALL
            .iter()
            .filter(|k| self.has_condition_kind(**k))
            .count()
    }

    /// Scalar cardinality of the [`ConditionKind`] set appearing at
    /// least once in [`Self::preconditions`] — the precondition-side
    /// arm of the (precondition, postcondition, condition-union)
    /// distinct-kind-count triad on [`EphemeralSpec`]. Thin typed
    /// delegate to
    /// [`crate::boundary::ConditionSliceExt::distinct_kind_count`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::distinct_precondition_kind_count`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice closed-set walk fails at that primitive's tests rather
    /// than as silent drift at either struct-level arm.
    #[must_use]
    pub fn distinct_precondition_kind_count(&self) -> usize {
        self.preconditions.distinct_kind_count()
    }

    /// Scalar cardinality of the [`ConditionKind`] set appearing at
    /// least once in [`Self::postconditions`] — the postcondition-side
    /// arm of the (precondition, postcondition, condition-union)
    /// distinct-kind-count triad on [`EphemeralSpec`]. Thin typed
    /// delegate to
    /// [`crate::boundary::ConditionSliceExt::distinct_kind_count`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::distinct_postcondition_kind_count`]
    /// on the point-domain surface. See
    /// [`Self::distinct_precondition_kind_count`] for the full rationale
    /// — the two methods share ONE lift motivation, ONE fail-before-
    /// pass-after composition-law pin, and ONE two-surface parity
    /// contract with the point-domain
    /// [`crate::boundary::Boundary`] distinct-kind-count peer methods.
    #[must_use]
    pub fn distinct_postcondition_kind_count(&self) -> usize {
        self.postconditions.distinct_kind_count()
    }

    /// The set of [`ConditionKind`] variants that do NOT appear in
    /// `preconditions ∪ postconditions`, projected in
    /// [`ConditionKind::ALL`] order — the closed-set-inversion
    /// COMPLEMENT of [`Self::distinct_condition_kinds`] on the
    /// (precondition, postcondition, condition-union) missing-set triad.
    /// Byte-identical peer of
    /// [`crate::boundary::Boundary::missing_condition_kinds`] on the
    /// ephemeral sugar surface.
    ///
    /// # Composed body — byte-identical to
    /// [`crate::boundary::Boundary::missing_condition_kinds`]
    ///
    /// `ConditionKind::ALL.into_iter().filter(|k|
    /// !self.has_condition_kind(*k)).collect()` — a thin projection
    /// over the closed set composed against the two-slice union
    /// primitive [`Self::has_condition_kind`] under a negated
    /// predicate. Equivalent to the SET-INTERSECTION of
    /// [`Self::missing_precondition_kinds`] and
    /// [`Self::missing_postcondition_kinds`] projected in canonical
    /// [`ConditionKind::ALL`] order (the union-composition law pinned
    /// by [`crate::assert_surface_union_composition_laws`]).
    ///
    /// # Peer on the point surface — [`crate::boundary::Boundary::missing_condition_kinds`]
    ///
    /// Same signature `(&Self) -> Vec<ConditionKind>`, same closed-set-
    /// complement body, on the point-domain [`crate::boundary::Boundary`]
    /// nested-slot carrier. Both methods compose against the SAME
    /// slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::missing_kinds`] via the
    /// two-slice union composed through [`Self::has_condition_kind`] —
    /// a regression at the per-slice walk fails at that primitive's
    /// tests rather than as silent drift at either struct-level
    /// complement caller.
    ///
    /// # Sibling to [`Self::distinct_condition_kinds`]
    ///
    /// SIXTH refinement on the ephemeral-surface presence-probe algebra,
    /// on the SAME closed-set-inversion axis as `distinct_condition_kinds`
    /// but under a NEGATED point-probe. The two-surface parity contract
    /// now covers SEVEN refinements (bool / `&Condition` /
    /// `impl Iterator` / `usize` / `Vec<ConditionKind>` closed-set-
    /// inversion / `usize` scalar cardinality of the closed-set-
    /// inversion / `Vec<ConditionKind>` closed-set-complement) on the
    /// condition axis, byte-for-byte peer of the point-domain triad on
    /// [`crate::boundary::Boundary`].
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the closed-set complement composes the SAME
    /// closed-set walk on both this ephemeral surface and the point-
    /// domain [`crate::boundary::Boundary`] surface).
    /// THEORY.md §VI.1 (generation over composition — a future
    /// [`ConditionKind`] variant added to `ALL` reaches both surfaces'
    /// missing-set triads mechanically through the SAME closed-set walk).
    #[must_use]
    pub fn missing_condition_kinds(&self) -> Vec<ConditionKind> {
        ConditionKind::ALL
            .into_iter()
            .filter(|k| !self.has_condition_kind(*k))
            .collect()
    }

    /// The set of [`ConditionKind`] variants that do NOT appear in
    /// [`Self::preconditions`], projected in [`ConditionKind::ALL`]
    /// order — the precondition-side arm of the (precondition,
    /// postcondition, condition-union) missing-set triad on
    /// [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::missing_kinds`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`crate::boundary::Boundary::missing_precondition_kinds`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice closed-set walk fails at that primitive's tests
    /// rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn missing_precondition_kinds(&self) -> Vec<ConditionKind> {
        self.preconditions.missing_kinds()
    }

    /// The set of [`ConditionKind`] variants that do NOT appear in
    /// [`Self::postconditions`], projected in [`ConditionKind::ALL`]
    /// order — the postcondition-side arm of the (precondition,
    /// postcondition, condition-union) missing-set triad on
    /// [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::missing_kinds`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`crate::boundary::Boundary::missing_postcondition_kinds`]
    /// on the point-domain surface. See
    /// [`Self::missing_precondition_kinds`] for the full rationale —
    /// the two methods share ONE lift motivation, ONE fail-before-
    /// pass-after composition-law pin, and ONE two-surface parity
    /// contract with the point-domain
    /// [`crate::boundary::Boundary`] missing-set peer methods.
    #[must_use]
    pub fn missing_postcondition_kinds(&self) -> Vec<ConditionKind> {
        self.postconditions.missing_kinds()
    }

    /// Zero-allocation iterator peer of [`Self::missing_condition_kinds`]
    /// — the condition-union arm of the (precondition, postcondition,
    /// condition-union) closed-set-complement iterator triad on
    /// [`EphemeralSpec`]. Byte-identical to
    /// [`crate::boundary::Boundary::iter_missing_condition_kinds`] on the
    /// point-domain surface. Walks [`ConditionKind::ALL`] in canonical
    /// order and yields every [`ConditionKind`] that does NOT appear in
    /// `preconditions ∪ postconditions`, WITHOUT materializing an
    /// intermediate `Vec<ConditionKind>`.
    pub fn iter_missing_condition_kinds(&self) -> impl Iterator<Item = ConditionKind> + '_ {
        ConditionKind::ALL
            .iter()
            .copied()
            .filter(|&k| !self.has_condition_kind(k))
    }

    /// Zero-allocation iterator peer of
    /// [`Self::missing_precondition_kinds`] — the precondition-side arm
    /// of the (precondition, postcondition, condition-union) closed-set-
    /// complement iterator triad on [`EphemeralSpec`]. Thin typed delegate
    /// to [`crate::boundary::ConditionSliceExt::iter_missing_kinds`] over
    /// [`Self::preconditions`].
    pub fn iter_missing_precondition_kinds(&self) -> impl Iterator<Item = ConditionKind> + '_ {
        self.preconditions.iter_missing_kinds()
    }

    /// Zero-allocation iterator peer of
    /// [`Self::missing_postcondition_kinds`] — the postcondition-side arm
    /// of the (precondition, postcondition, condition-union) closed-set-
    /// complement iterator triad on [`EphemeralSpec`]. Thin typed delegate
    /// to [`crate::boundary::ConditionSliceExt::iter_missing_kinds`] over
    /// [`Self::postconditions`].
    pub fn iter_missing_postcondition_kinds(&self) -> impl Iterator<Item = ConditionKind> + '_ {
        self.postconditions.iter_missing_kinds()
    }

    /// Scalar cardinality of the [`ConditionKind`] set NOT appearing in
    /// `preconditions ∪ postconditions` — the peer of
    /// [`crate::boundary::Boundary::missing_condition_kind_count`] on
    /// the [`EphemeralSpec`] sugar surface.
    ///
    /// # Composed body — byte-identical to
    /// [`crate::boundary::Boundary::missing_condition_kind_count`]
    ///
    /// `ConditionKind::ALL.iter().filter(|k|
    /// !self.has_condition_kind(**k)).count()` — the scalar cardinality
    /// projection of [`Self::missing_condition_kinds`] onto its
    /// `.len()`, without materializing the intermediate
    /// `Vec<ConditionKind>`. Byte-identical to the peer method on the
    /// point-domain [`crate::boundary::Boundary`] surface — both
    /// compose against the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::missing_kind_count`] via
    /// the two-slice union composed through [`Self::has_condition_kind`]
    /// so a regression at the per-slice negated closed-set walk fails
    /// at that primitive's tests rather than as silent drift at either
    /// struct-level scalar-cardinality caller.
    ///
    /// # Sibling to [`Self::missing_condition_kinds`]
    ///
    /// Scalar projection of the closed-set-complement widened primitive
    /// on the ephemeral-union surface — where `missing_condition_kinds`
    /// returns the SET, `missing_condition_kind_count` collapses it to
    /// its cardinality. The two-surface parity contract now covers
    /// EIGHT refinements (bool / `&Condition` / `impl Iterator` /
    /// `usize` / `Vec<ConditionKind>` closed-set-inversion / `usize`
    /// scalar cardinality of the closed-set-inversion /
    /// `Vec<ConditionKind>` closed-set-complement / `usize` scalar
    /// cardinality of the closed-set-complement) on the condition axis,
    /// byte-for-byte peer of the point-domain triad on
    /// [`crate::boundary::Boundary`].
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — the scalar cardinality composes the SAME closed-set
    /// walk under negation on both this ephemeral surface and the
    /// point-domain [`crate::boundary::Boundary`] surface).
    /// THEORY.md §VI.1 (generation over composition — a future
    /// [`ConditionKind`] variant added to `ALL` reaches both surfaces'
    /// missing-kind-count triads mechanically through the SAME
    /// closed-set walk).
    #[must_use]
    pub fn missing_condition_kind_count(&self) -> usize {
        ConditionKind::ALL
            .iter()
            .filter(|k| !self.has_condition_kind(**k))
            .count()
    }

    /// Scalar cardinality of the [`ConditionKind`] set NOT appearing in
    /// [`Self::preconditions`] — the precondition-side arm of the
    /// (precondition, postcondition, condition-union) missing-kind-count
    /// triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::missing_kind_count`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::missing_precondition_kind_count`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice negated closed-set walk fails at that primitive's tests
    /// rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn missing_precondition_kind_count(&self) -> usize {
        self.preconditions.missing_kind_count()
    }

    /// Scalar cardinality of the [`ConditionKind`] set NOT appearing in
    /// [`Self::postconditions`] — the postcondition-side arm of the
    /// (precondition, postcondition, condition-union) missing-kind-count
    /// triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::missing_kind_count`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::missing_postcondition_kind_count`]
    /// on the point-domain surface. See
    /// [`Self::missing_precondition_kind_count`] for the full rationale
    /// — the two methods share ONE lift motivation, ONE fail-before-
    /// pass-after composition-law pin, and ONE two-surface parity
    /// contract with the point-domain
    /// [`crate::boundary::Boundary`] missing-kind-count peer methods.
    #[must_use]
    pub fn missing_postcondition_kind_count(&self) -> usize {
        self.postconditions.missing_kind_count()
    }

    /// Earliest [`ConditionKind::ALL`] entry present in
    /// `preconditions ∪ postconditions`, or `None` when neither side
    /// populates any variant — the peer of
    /// [`crate::boundary::Boundary::first_distinct_condition_kind`]
    /// on the [`EphemeralSpec`] sugar surface.
    ///
    /// # Composed body — byte-identical to
    /// [`crate::boundary::Boundary::first_distinct_condition_kind`]
    ///
    /// `ConditionKind::ALL.iter().copied().find(|k|
    /// self.has_condition_kind(*k))` — the earliest-element scalar
    /// projection of [`Self::distinct_condition_kinds`] onto its first
    /// entry, without materializing the intermediate
    /// `Vec<ConditionKind>`. Byte-identical to the peer method on the
    /// point-domain [`crate::boundary::Boundary`] surface — both
    /// compose against the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::first_distinct_kind`] via
    /// the two-slice union composed through
    /// [`Self::has_condition_kind`] so a regression at the per-slice
    /// short-circuit walk fails at that primitive's tests rather than
    /// as silent drift at either struct-level earliest-element caller.
    ///
    /// # Sibling to [`Self::distinct_condition_kinds`]
    ///
    /// Third scalar projection of the closed-set-inversion widened
    /// primitive on the ephemeral-union surface. The two-surface
    /// parity contract now covers NINE refinements on the condition
    /// axis (bool / `&Condition` / `impl Iterator` / `usize` /
    /// `Vec<ConditionKind>` closed-set-inversion / `usize` scalar
    /// cardinality of the closed-set-inversion / `Vec<ConditionKind>`
    /// closed-set-complement / `usize` scalar cardinality of the
    /// closed-set-complement / `Option<ConditionKind>` earliest-element
    /// scalar of the closed-set-inversion), byte-for-byte peer of the
    /// point-domain triad on [`crate::boundary::Boundary`].
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the earliest-element projection composes the
    /// SAME closed-set walk on both this ephemeral surface and the
    /// point-domain [`crate::boundary::Boundary`] surface under short-
    /// circuit semantics). THEORY.md §VI.1 (generation over composition
    /// — a future [`ConditionKind`] variant added to `ALL` reaches both
    /// surfaces' first-distinct-kind triads mechanically through the
    /// SAME closed-set walk).
    #[must_use]
    pub fn first_distinct_condition_kind(&self) -> Option<ConditionKind> {
        ConditionKind::ALL
            .iter()
            .copied()
            .find(|k| self.has_condition_kind(*k))
    }

    /// Earliest [`ConditionKind::ALL`] entry present in
    /// [`Self::preconditions`], or `None` when preconditions carry no
    /// matching kind — the precondition-side arm of the (precondition,
    /// postcondition, condition-union) first-distinct-kind triad on
    /// [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::first_distinct_kind`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::first_distinct_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice short-circuit walk fails at that primitive's tests
    /// rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn first_distinct_precondition_kind(&self) -> Option<ConditionKind> {
        self.preconditions.first_distinct_kind()
    }

    /// Earliest [`ConditionKind::ALL`] entry present in
    /// [`Self::postconditions`], or `None` when postconditions carry
    /// no matching kind — the postcondition-side arm of the
    /// (precondition, postcondition, condition-union) first-distinct-
    /// kind triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::first_distinct_kind`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::first_distinct_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::first_distinct_precondition_kind`] for the full
    /// rationale — the two methods share ONE lift motivation, ONE
    /// fail-before-pass-after composition-law pin, and ONE two-surface
    /// parity contract with the point-domain
    /// [`crate::boundary::Boundary`] first-distinct-kind peer methods.
    #[must_use]
    pub fn first_distinct_postcondition_kind(&self) -> Option<ConditionKind> {
        self.postconditions.first_distinct_kind()
    }

    /// Earliest [`ConditionKind::ALL`] entry ABSENT from
    /// `preconditions ∪ postconditions`, or `None` when the union
    /// carries every variant — the peer of
    /// [`crate::boundary::Boundary::first_missing_condition_kind`]
    /// on the [`EphemeralSpec`] sugar surface.
    ///
    /// # Composed body — byte-identical to
    /// [`crate::boundary::Boundary::first_missing_condition_kind`]
    ///
    /// `ConditionKind::ALL.iter().copied().find(|k|
    /// !self.has_condition_kind(*k))` — the earliest-element scalar
    /// projection of [`Self::missing_condition_kinds`] onto its first
    /// entry under a NEGATED predicate. Byte-identical to the peer
    /// method on the point-domain [`crate::boundary::Boundary`]
    /// surface — both compose against the SAME slice-level substrate
    /// primitive [`crate::boundary::ConditionSliceExt::first_missing_kind`]
    /// via the two-slice union composed through
    /// [`Self::has_condition_kind`] so a regression at the per-slice
    /// negated short-circuit walk fails at that primitive's tests
    /// rather than as silent drift at either struct-level earliest-
    /// element caller.
    ///
    /// # Sibling to [`Self::missing_condition_kinds`]
    ///
    /// Third scalar projection of the closed-set-complement widened
    /// primitive on the ephemeral-union surface. The two-surface
    /// parity contract now covers TEN refinements on the condition
    /// axis (the nine listed at [`Self::first_distinct_condition_kind`]
    /// plus `Option<ConditionKind>` earliest-element scalar of the
    /// closed-set-complement), byte-for-byte peer of the point-domain
    /// triad on [`crate::boundary::Boundary`].
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the complement-earliest-element projection
    /// composes the SAME closed-set walk on both this ephemeral
    /// surface and the point-domain [`crate::boundary::Boundary`]
    /// surface under short-circuit semantics with a negated predicate).
    /// THEORY.md §VI.1 (generation over composition — a future
    /// [`ConditionKind`] variant added to `ALL` reaches both surfaces'
    /// first-missing-kind triads mechanically through the SAME closed-
    /// set walk).
    #[must_use]
    pub fn first_missing_condition_kind(&self) -> Option<ConditionKind> {
        ConditionKind::ALL
            .iter()
            .copied()
            .find(|k| !self.has_condition_kind(*k))
    }

    /// Earliest [`ConditionKind::ALL`] entry ABSENT from
    /// [`Self::preconditions`], or `None` when preconditions carry
    /// every variant — the precondition-side arm of the (precondition,
    /// postcondition, condition-union) first-missing-kind triad on
    /// [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::first_missing_kind`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::first_missing_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice negated short-circuit walk fails at that primitive's
    /// tests rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn first_missing_precondition_kind(&self) -> Option<ConditionKind> {
        self.preconditions.first_missing_kind()
    }

    /// Earliest [`ConditionKind::ALL`] entry ABSENT from
    /// [`Self::postconditions`], or `None` when postconditions carry
    /// every variant — the postcondition-side arm of the (precondition,
    /// postcondition, condition-union) first-missing-kind triad on
    /// [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::first_missing_kind`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::first_missing_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::first_missing_precondition_kind`] for the full
    /// rationale — the two methods share ONE lift motivation, ONE
    /// fail-before-pass-after composition-law pin, and ONE two-surface
    /// parity contract with the point-domain
    /// [`crate::boundary::Boundary`] first-missing-kind peer methods.
    #[must_use]
    pub fn first_missing_postcondition_kind(&self) -> Option<ConditionKind> {
        self.postconditions.first_missing_kind()
    }

    /// Latest [`ConditionKind::ALL`] entry present in
    /// `preconditions ∪ postconditions`, or `None` when neither side
    /// populates any variant — the peer of
    /// [`crate::boundary::Boundary::last_distinct_condition_kind`]
    /// on the [`EphemeralSpec`] sugar surface.
    ///
    /// # Composed body — byte-identical to
    /// [`crate::boundary::Boundary::last_distinct_condition_kind`]
    ///
    /// `ConditionKind::ALL.iter().rev().copied().find(|k|
    /// self.has_condition_kind(*k))` — the latest-element scalar
    /// projection of [`Self::distinct_condition_kinds`] onto its last
    /// entry via a REVERSED closed-set walk, without materializing
    /// the intermediate `Vec<ConditionKind>`. Byte-identical to the
    /// peer method on the point-domain [`crate::boundary::Boundary`]
    /// surface — both compose against the SAME slice-level substrate
    /// primitive [`crate::boundary::ConditionSliceExt::last_distinct_kind`]
    /// via the two-slice union composed through
    /// [`Self::has_condition_kind`] so a regression at the per-slice
    /// REVERSED short-circuit walk fails at that primitive's tests
    /// rather than as silent drift at either struct-level latest-
    /// element caller.
    ///
    /// # Sibling to [`Self::first_distinct_condition_kind`] /
    /// [`Self::distinct_condition_kinds`]
    ///
    /// Time-reversed scalar peer of the earliest-element projection
    /// under the SAME two-slice union predicate. The two-surface
    /// parity contract now covers ELEVEN refinements on the condition
    /// axis (the nine listed at `first_distinct_condition_kind` plus
    /// `Option<ConditionKind>` earliest-element scalar of the closed-
    /// set-complement (`first_missing_*_kind`), plus this
    /// `Option<ConditionKind>` latest-element scalar of the closed-
    /// set-inversion (`last_distinct_*_kind`)). Byte-for-byte peer of
    /// the point-domain triad on [`crate::boundary::Boundary`].
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the latest-element projection composes the
    /// SAME reversed closed-set walk on both this ephemeral surface
    /// and the point-domain [`crate::boundary::Boundary`] surface
    /// under short-circuit semantics). THEORY.md §VI.1 (generation
    /// over composition — a future [`ConditionKind`] variant added to
    /// `ALL` reaches both surfaces' last-distinct-kind triads
    /// mechanically through the SAME reversed closed-set walk).
    #[must_use]
    pub fn last_distinct_condition_kind(&self) -> Option<ConditionKind> {
        ConditionKind::ALL
            .iter()
            .rev()
            .copied()
            .find(|k| self.has_condition_kind(*k))
    }

    /// Latest [`ConditionKind::ALL`] entry present in
    /// [`Self::preconditions`], or `None` when preconditions carry no
    /// matching kind — the precondition-side arm of the (precondition,
    /// postcondition, condition-union) last-distinct-kind triad on
    /// [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::last_distinct_kind`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::last_distinct_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice REVERSED short-circuit walk fails at that primitive's
    /// tests rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn last_distinct_precondition_kind(&self) -> Option<ConditionKind> {
        self.preconditions.last_distinct_kind()
    }

    /// Latest [`ConditionKind::ALL`] entry present in
    /// [`Self::postconditions`], or `None` when postconditions carry
    /// no matching kind — the postcondition-side arm of the
    /// (precondition, postcondition, condition-union) last-distinct-
    /// kind triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::last_distinct_kind`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::last_distinct_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::last_distinct_precondition_kind`] for the full
    /// rationale — the two methods share ONE lift motivation, ONE
    /// fail-before-pass-after composition-law pin, and ONE two-surface
    /// parity contract with the point-domain
    /// [`crate::boundary::Boundary`] last-distinct-kind peer methods.
    #[must_use]
    pub fn last_distinct_postcondition_kind(&self) -> Option<ConditionKind> {
        self.postconditions.last_distinct_kind()
    }

    /// Latest [`ConditionKind::ALL`] entry ABSENT from
    /// `preconditions ∪ postconditions`, or `None` when the union
    /// carries every variant — the peer of
    /// [`crate::boundary::Boundary::last_missing_condition_kind`]
    /// on the [`EphemeralSpec`] sugar surface.
    ///
    /// # Composed body — byte-identical to
    /// [`crate::boundary::Boundary::last_missing_condition_kind`]
    ///
    /// `ConditionKind::ALL.iter().rev().copied().find(|k|
    /// !self.has_condition_kind(*k))` — the latest-element scalar
    /// projection of [`Self::missing_condition_kinds`] onto its last
    /// entry via a REVERSED closed-set walk under a NEGATED
    /// predicate. Byte-identical to the peer method on the point-
    /// domain [`crate::boundary::Boundary`] surface — both compose
    /// against the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::last_missing_kind`] via
    /// the two-slice union composed through
    /// [`Self::has_condition_kind`] so a regression at the per-slice
    /// negated REVERSED short-circuit walk fails at that primitive's
    /// tests rather than as silent drift at either struct-level
    /// latest-element caller.
    ///
    /// # Sibling to [`Self::first_missing_condition_kind`] /
    /// [`Self::missing_condition_kinds`]
    ///
    /// Time-reversed scalar peer of the earliest-element projection
    /// under the SAME negated two-slice union predicate. The two-
    /// surface parity contract now covers TWELVE refinements on the
    /// condition axis (the ten listed at `first_missing_condition_kind`
    /// plus `Option<ConditionKind>` latest-element scalar of the
    /// closed-set-inversion (`last_distinct_*_kind`), plus this
    /// `Option<ConditionKind>` latest-element scalar of the closed-
    /// set-complement). Byte-for-byte peer of the point-domain triad
    /// on [`crate::boundary::Boundary`].
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the complement-latest-element projection
    /// composes the SAME reversed closed-set walk on both this
    /// ephemeral surface and the point-domain
    /// [`crate::boundary::Boundary`] surface under short-circuit
    /// semantics with a negated predicate). THEORY.md §VI.1
    /// (generation over composition — a future [`ConditionKind`]
    /// variant added to `ALL` reaches both surfaces' last-missing-kind
    /// triads mechanically through the SAME reversed closed-set walk).
    #[must_use]
    pub fn last_missing_condition_kind(&self) -> Option<ConditionKind> {
        ConditionKind::ALL
            .iter()
            .rev()
            .copied()
            .find(|k| !self.has_condition_kind(*k))
    }

    /// Latest [`ConditionKind::ALL`] entry ABSENT from
    /// [`Self::preconditions`], or `None` when preconditions carry
    /// every variant — the precondition-side arm of the (precondition,
    /// postcondition, condition-union) last-missing-kind triad on
    /// [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::last_missing_kind`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::last_missing_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice negated REVERSED short-circuit walk fails at that
    /// primitive's tests rather than as silent drift at either
    /// struct-level arm.
    #[must_use]
    pub fn last_missing_precondition_kind(&self) -> Option<ConditionKind> {
        self.preconditions.last_missing_kind()
    }

    /// Latest [`ConditionKind::ALL`] entry ABSENT from
    /// [`Self::postconditions`], or `None` when postconditions carry
    /// every variant — the postcondition-side arm of the
    /// (precondition, postcondition, condition-union) last-missing-
    /// kind triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::last_missing_kind`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::last_missing_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::last_missing_precondition_kind`] for the full
    /// rationale — the two methods share ONE lift motivation, ONE
    /// fail-before-pass-after composition-law pin, and ONE two-surface
    /// parity contract with the point-domain
    /// [`crate::boundary::Boundary`] last-missing-kind peer methods.
    #[must_use]
    pub fn last_missing_postcondition_kind(&self) -> Option<ConditionKind> {
        self.postconditions.last_missing_kind()
    }

    /// `true` iff `preconditions ∪ postconditions` carries every
    /// [`ConditionKind::ALL`] variant at least once — the peer of
    /// [`crate::boundary::Boundary::is_condition_kind_saturated`] on
    /// the [`EphemeralSpec`] sugar surface.
    ///
    /// # Composed body — byte-identical to
    /// [`crate::boundary::Boundary::is_condition_kind_saturated`]
    ///
    /// `ConditionKind::ALL.iter().all(|k| self.has_condition_kind(*k))`
    /// — the saturation-endpoint projection of
    /// [`Self::missing_condition_kinds`] onto its emptiness test via
    /// a SHORT-CIRCUITING closed-set walk under the two-slice union
    /// primitive [`Self::has_condition_kind`]. Byte-identical to the
    /// peer method on the point-domain [`crate::boundary::Boundary`]
    /// surface — both compose against the SAME slice-level substrate
    /// primitive [`crate::boundary::ConditionSliceExt::is_kind_saturated`]
    /// via the two-slice union so a regression at the per-slice `all`
    /// short-circuit fails at that primitive's tests rather than as
    /// silent drift at either struct-level saturation caller.
    ///
    /// # Sibling to [`Self::missing_condition_kinds`] /
    /// [`Self::missing_condition_kind_count`]
    ///
    /// Boolean saturation-endpoint peer of the widened and scalar
    /// closed-set-complement primitives on the ephemeral-union
    /// surface — where those primitives return the SET and its
    /// cardinality, `is_condition_kind_saturated` collapses the
    /// scalar to its zero-arm Boolean projection.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the saturation-endpoint projection composes
    /// the SAME closed-set walk on both this ephemeral surface and the
    /// point-domain [`crate::boundary::Boundary`] surface under
    /// short-circuit semantics). THEORY.md §VI.1 (generation over
    /// composition — a future [`ConditionKind`] variant added to `ALL`
    /// reaches both surfaces' saturation-predicate triads mechanically
    /// through the SAME closed-set walk).
    #[must_use]
    pub fn is_condition_kind_saturated(&self) -> bool {
        ConditionKind::ALL
            .iter()
            .all(|k| self.has_condition_kind(*k))
    }

    /// `true` iff [`Self::preconditions`] carries every
    /// [`ConditionKind::ALL`] variant at least once — the precondition-
    /// side arm of the (precondition, postcondition, condition-union)
    /// saturation-predicate triad on [`EphemeralSpec`]. Thin typed
    /// delegate to
    /// [`crate::boundary::ConditionSliceExt::is_kind_saturated`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::is_precondition_kind_saturated`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice `all` short-circuit fails at that primitive's tests
    /// rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn is_precondition_kind_saturated(&self) -> bool {
        self.preconditions.is_kind_saturated()
    }

    /// `true` iff [`Self::postconditions`] carries every
    /// [`ConditionKind::ALL`] variant at least once — the postcondition-
    /// side arm of the (precondition, postcondition, condition-union)
    /// saturation-predicate triad on [`EphemeralSpec`]. Thin typed
    /// delegate to
    /// [`crate::boundary::ConditionSliceExt::is_kind_saturated`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::is_postcondition_kind_saturated`]
    /// on the point-domain surface. See
    /// [`Self::is_precondition_kind_saturated`] for the full rationale
    /// — the two methods share ONE lift motivation, ONE fail-before-
    /// pass-after composition-law pin, and ONE two-surface parity
    /// contract with the point-domain
    /// [`crate::boundary::Boundary`] saturation-predicate peer methods.
    #[must_use]
    pub fn is_postcondition_kind_saturated(&self) -> bool {
        self.postconditions.is_kind_saturated()
    }

    /// `true` iff `preconditions ∪ postconditions` is MISSING at least
    /// one [`ConditionKind::ALL`] variant — the peer of
    /// [`crate::boundary::Boundary::has_any_missing_condition_kind`] on
    /// the [`EphemeralSpec`] sugar surface.
    ///
    /// # Composed body — byte-identical to
    /// [`crate::boundary::Boundary::has_any_missing_condition_kind`]
    ///
    /// `!self.is_condition_kind_saturated()` — the at-least-one
    /// halfspace projection of [`Self::missing_condition_kinds`] onto
    /// its non-emptiness test via a SHORT-CIRCUITING closed-set walk
    /// under the two-slice union primitive [`Self::has_condition_kind`]
    /// negated. Byte-identical to the peer method on the point-domain
    /// [`crate::boundary::Boundary`] surface — both compose against the
    /// SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_any_missing_kind`] via
    /// the two-slice union so a regression at the per-slice `all`
    /// short-circuit under negation fails at that primitive's tests
    /// rather than as silent drift at either struct-level at-least-
    /// one halfspace caller.
    ///
    /// # Sibling to [`Self::missing_condition_kinds`] /
    /// [`Self::missing_condition_kind_count`]
    ///
    /// Boolean at-least-one halfspace peer of the widened and scalar
    /// closed-set-complement primitives on the ephemeral-union
    /// surface — where those primitives return the SET and its
    /// cardinality, `has_any_missing_condition_kind` collapses either
    /// to its `>= 1` halfspace Boolean.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the at-least-one halfspace projection
    /// composes the SAME closed-set walk under negation on both this
    /// ephemeral surface and the point-domain
    /// [`crate::boundary::Boundary`] surface under short-circuit
    /// semantics). THEORY.md §VI.1 (generation over composition — a
    /// future [`ConditionKind`] variant added to `ALL` reaches both
    /// surfaces' at-least-one halfspace triads mechanically through
    /// the SAME closed-set walk).
    #[must_use]
    pub fn has_any_missing_condition_kind(&self) -> bool {
        !self.is_condition_kind_saturated()
    }

    /// `true` iff [`Self::preconditions`] is MISSING at least one
    /// [`ConditionKind::ALL`] variant — the precondition-side arm of
    /// the (precondition, postcondition, condition-union) at-least-
    /// one halfspace triad on [`EphemeralSpec`]. Thin typed delegate
    /// to [`crate::boundary::ConditionSliceExt::has_any_missing_kind`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_any_missing_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice `all` short-circuit under negation fails at that
    /// primitive's tests rather than as silent drift at either
    /// struct-level arm.
    #[must_use]
    pub fn has_any_missing_precondition_kind(&self) -> bool {
        self.preconditions.has_any_missing_kind()
    }

    /// `true` iff [`Self::postconditions`] is MISSING at least one
    /// [`ConditionKind::ALL`] variant — the postcondition-side arm of
    /// the (precondition, postcondition, condition-union) at-least-
    /// one halfspace triad on [`EphemeralSpec`]. Thin typed delegate
    /// to [`crate::boundary::ConditionSliceExt::has_any_missing_kind`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_any_missing_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::has_any_missing_precondition_kind`] for the full
    /// rationale — the two methods share ONE lift motivation, ONE
    /// fail-before-pass-after composition-law pin, and ONE two-surface
    /// parity contract with the point-domain
    /// [`crate::boundary::Boundary`] at-least-one halfspace peer
    /// methods.
    #[must_use]
    pub fn has_any_missing_postcondition_kind(&self) -> bool {
        self.postconditions.has_any_missing_kind()
    }

    /// `true` iff `preconditions ∪ postconditions` carries at least one
    /// [`ConditionKind::ALL`] variant — the peer of
    /// [`crate::boundary::Boundary::has_any_distinct_condition_kind`]
    /// on the [`EphemeralSpec`] sugar surface.
    ///
    /// # Composed body — byte-identical to
    /// [`crate::boundary::Boundary::has_any_distinct_condition_kind`]
    ///
    /// `ConditionKind::ALL.iter().copied().any(|k|
    /// self.has_condition_kind(k))` — the at-least-one halfspace
    /// projection of [`Self::distinct_condition_kinds`] onto its non-
    /// emptiness test via a SHORT-CIRCUITING closed-set walk under the
    /// two-slice union primitive [`Self::has_condition_kind`]. Byte-
    /// identical to the peer method on the point-domain
    /// [`crate::boundary::Boundary`] surface — both compose against
    /// the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_any_distinct_kind`]
    /// via the two-slice union so a regression at the per-slice `any`
    /// short-circuit fails at that primitive's tests rather than as
    /// silent drift at either struct-level at-least-one halfspace
    /// caller.
    ///
    /// # Sibling to [`Self::distinct_condition_kinds`] /
    /// [`Self::distinct_condition_kind_count`]
    ///
    /// Boolean at-least-one halfspace peer of the widened and scalar
    /// closed-set-inversion primitives on the ephemeral-union
    /// surface — where those primitives return the SET and its
    /// cardinality, `has_any_distinct_condition_kind` collapses either
    /// to its `>= 1` halfspace Boolean.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the at-least-one halfspace projection
    /// composes the SAME closed-set walk on both this ephemeral
    /// surface and the point-domain [`crate::boundary::Boundary`]
    /// surface under short-circuit semantics). THEORY.md §VI.1
    /// (generation over composition — a future [`ConditionKind`]
    /// variant added to `ALL` reaches both surfaces' at-least-one
    /// halfspace triads mechanically through the SAME closed-set
    /// walk).
    #[must_use]
    pub fn has_any_distinct_condition_kind(&self) -> bool {
        ConditionKind::ALL
            .iter()
            .copied()
            .any(|k| self.has_condition_kind(k))
    }

    /// `true` iff [`Self::preconditions`] carries at least one
    /// [`ConditionKind::ALL`] variant — the precondition-side arm of
    /// the (precondition, postcondition, condition-union) at-least-
    /// one halfspace triad on [`EphemeralSpec`] on the closed-set-
    /// inversion axis. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_any_distinct_kind`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_any_distinct_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice `any` short-circuit fails at that primitive's tests
    /// rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn has_any_distinct_precondition_kind(&self) -> bool {
        self.preconditions.has_any_distinct_kind()
    }

    /// `true` iff [`Self::postconditions`] carries at least one
    /// [`ConditionKind::ALL`] variant — the postcondition-side arm of
    /// the (precondition, postcondition, condition-union) at-least-
    /// one halfspace triad on [`EphemeralSpec`] on the closed-set-
    /// inversion axis. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_any_distinct_kind`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_any_distinct_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::has_any_distinct_precondition_kind`] for the full
    /// rationale — the two methods share ONE lift motivation, ONE
    /// fail-before-pass-after composition-law pin, and ONE two-surface
    /// parity contract with the point-domain
    /// [`crate::boundary::Boundary`] at-least-one halfspace peer
    /// methods.
    #[must_use]
    pub fn has_any_distinct_postcondition_kind(&self) -> bool {
        self.postconditions.has_any_distinct_kind()
    }

    /// `true` iff `preconditions ∪ postconditions` carries EXACTLY
    /// ONE [`ConditionKind::ALL`] variant — the union arm of the
    /// (precondition, postcondition, condition-union) cardinality-
    /// mid-endpoint triad on [`EphemeralSpec`] closing the singleton-
    /// coverage arm on the closed-set-inversion axis on the union of
    /// the two condition slots. The Boolean cardinality-mid-endpoint
    /// fast-path peer of [`Self::has_any_distinct_condition_kind`]
    /// (≥1 halfspace) on the union axis: where the at-least-one
    /// halfspace predicate answers "is ANY kind covered by the
    /// union?", `has_unique_distinct_condition_kind` answers "is
    /// EXACTLY ONE kind covered by the union?".
    ///
    /// Composed body: constructs a two-step-short-circuit walk over
    /// [`ConditionKind::ALL`] under the [`Self::has_condition_kind`]
    /// union primitive — the first covered union arm surfaces, then
    /// the walk short-circuits at the second. Byte-for-byte peer of
    /// [`crate::boundary::ConditionSliceExt::has_unique_distinct_kind`]
    /// one slice-layer down, lifted to compose against
    /// [`Self::has_condition_kind`]'s pre-OR-post union rather than
    /// against a single slice's `has_kind`.
    ///
    /// # Peer on the point-domain surface — [`crate::boundary::Boundary::has_unique_distinct_condition_kind`]
    ///
    /// Byte-identical signature `(&Self) -> bool`, byte-identical
    /// two-step short-circuit body composed against the point-domain
    /// surface's own union primitive. Both methods compose against
    /// the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_unique_distinct_kind`]
    /// via the two-slice union — a regression at the per-slice
    /// singleton-coverage walk fails at that primitive's tests rather
    /// than as silent drift at either struct-level singleton-coverage
    /// caller.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the cardinality-mid-endpoint projection on
    /// the closed-set-inversion axis composes the SAME two-step
    /// short-circuit walk on both this ephemeral surface and the
    /// point-domain [`crate::boundary::Boundary`] surface). THEORY.md
    /// §VI.1 (generation over composition — a new [`ConditionKind`]
    /// variant reaches both surfaces' cardinality-mid-endpoint triads
    /// mechanically through the delegated union primitive).
    #[must_use]
    pub fn has_unique_distinct_condition_kind(&self) -> bool {
        let mut it = ConditionKind::ALL
            .iter()
            .copied()
            .filter(|k| self.has_condition_kind(*k));
        it.next().is_some() && it.next().is_none()
    }

    /// `true` iff [`Self::preconditions`] carries EXACTLY ONE
    /// [`ConditionKind::ALL`] variant — the precondition-side arm of
    /// the (precondition, postcondition, condition-union)
    /// cardinality-mid-endpoint triad on [`EphemeralSpec`] on the
    /// closed-set-inversion axis. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_unique_distinct_kind`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_unique_distinct_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice two-step short-circuit walk fails at that primitive's
    /// tests rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn has_unique_distinct_precondition_kind(&self) -> bool {
        self.preconditions.has_unique_distinct_kind()
    }

    /// `true` iff [`Self::postconditions`] carries EXACTLY ONE
    /// [`ConditionKind::ALL`] variant — the postcondition-side arm of
    /// the (precondition, postcondition, condition-union)
    /// cardinality-mid-endpoint triad on [`EphemeralSpec`] on the
    /// closed-set-inversion axis. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_unique_distinct_kind`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_unique_distinct_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::has_unique_distinct_precondition_kind`] for the full
    /// rationale — the two methods share ONE lift motivation, ONE
    /// fail-before-pass-after composition-law pin, and ONE two-surface
    /// parity contract with the point-domain
    /// [`crate::boundary::Boundary`] cardinality-mid-endpoint peer
    /// methods.
    #[must_use]
    pub fn has_unique_distinct_postcondition_kind(&self) -> bool {
        self.postconditions.has_unique_distinct_kind()
    }

    /// `true` iff `preconditions ∪ postconditions` COVERS AT LEAST
    /// TWO [`ConditionKind::ALL`] variants — the union arm of the
    /// (precondition, postcondition, condition-union) cardinality-
    /// many-arm triad on [`EphemeralSpec`] closing the "≥ 2 kinds
    /// covered" arm on the union of the two condition slots. The
    /// Boolean cardinality many-arm fast-path peer of
    /// [`Self::has_unique_distinct_condition_kind`] (=1 arm) and
    /// [`Self::has_any_distinct_condition_kind`] (≥1 halfspace):
    /// closes the {0, 1, ≥2} trichotomy on the distinct axis at the
    /// ephemeral union struct layer.
    ///
    /// Composed body: constructs a two-step-short-circuit walk over
    /// [`ConditionKind::ALL`] under the [`Self::has_condition_kind`]
    /// union primitive — pulls up to two hits off the filtered
    /// iterator; the primitive returns `true` iff BOTH the first and
    /// the second are [`Some`]. Byte-for-byte peer of
    /// [`crate::boundary::ConditionSliceExt::has_multiple_distinct_kinds`]
    /// one slice-layer down, lifted to compose against
    /// [`Self::has_condition_kind`]'s pre-OR-post union rather than
    /// against a single slice's `has_kind`.
    ///
    /// # Peer on the point-domain surface — [`crate::boundary::Boundary::has_multiple_distinct_condition_kind`]
    ///
    /// Byte-identical signature `(&Self) -> bool`, byte-identical
    /// two-step short-circuit body composed against the point-domain
    /// surface's own union primitive. Both methods compose against
    /// the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_multiple_distinct_kinds`]
    /// via the two-slice union — a regression at the per-slice many-
    /// arm walk fails at that primitive's tests rather than as silent
    /// drift at either struct-level many-distinct caller.
    ///
    /// # Sibling to [`Self::distinct_condition_kinds`] /
    /// [`Self::distinct_condition_kind_count`]
    ///
    /// Cardinality-many-arm Boolean projection of the widened +
    /// scalar closed-set-inversion primitives on the ephemeral-union
    /// surface — where those primitives return the FULL distinct SET
    /// (a `Vec` of every present kind) and its cardinality (a `usize`
    /// in `0..=ConditionKind::ALL.len()`),
    /// `has_multiple_distinct_condition_kind` collapses either the
    /// widened primitive to its ≥ 2-length Boolean or the scalar to
    /// its `>= 2` cardinality-many-arm Boolean. Strictly cheaper than
    /// either widened primitive on every arm with `≥ 2` distinct kinds
    /// because the walk short-circuits at the second distinct kind
    /// rather than allocating the closed-set-inversion scan or walking
    /// every slot to build the scalar cardinality.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the cardinality-many-arm projection on the
    /// distinct axis composes the SAME two-step short-circuit walk
    /// under a two-slice union on both this ephemeral surface and the
    /// point-domain [`crate::boundary::Boundary`] surface). THEORY.md
    /// §VI.1 (generation over composition — a new [`ConditionKind`]
    /// variant reaches both surfaces' cardinality-many-arm triads
    /// mechanically through the delegated union primitive).
    #[must_use]
    pub fn has_multiple_distinct_condition_kind(&self) -> bool {
        let mut it = ConditionKind::ALL
            .iter()
            .copied()
            .filter(|k| self.has_condition_kind(*k));
        it.next().is_some() && it.next().is_some()
    }

    /// `true` iff [`Self::preconditions`] carries AT LEAST TWO
    /// [`ConditionKind::ALL`] variants — the precondition-side arm of
    /// the (precondition, postcondition, condition-union) cardinality-
    /// many-arm triad on [`EphemeralSpec`] on the closed-set-inversion
    /// axis. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_multiple_distinct_kinds`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_multiple_distinct_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice two-step short-circuit walk fails at that primitive's
    /// tests rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn has_multiple_distinct_precondition_kind(&self) -> bool {
        self.preconditions.has_multiple_distinct_kinds()
    }

    /// `true` iff [`Self::postconditions`] carries AT LEAST TWO
    /// [`ConditionKind::ALL`] variants — the postcondition-side arm of
    /// the (precondition, postcondition, condition-union) cardinality-
    /// many-arm triad on [`EphemeralSpec`] on the closed-set-inversion
    /// axis. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_multiple_distinct_kinds`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_multiple_distinct_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::has_multiple_distinct_precondition_kind`] for the full
    /// rationale — the two methods share ONE lift motivation, ONE
    /// fail-before-pass-after composition-law pin, and ONE two-surface
    /// parity contract with the point-domain
    /// [`crate::boundary::Boundary`] cardinality-many-arm peer
    /// methods.
    #[must_use]
    pub fn has_multiple_distinct_postcondition_kind(&self) -> bool {
        self.postconditions.has_multiple_distinct_kinds()
    }

    /// `true` iff `preconditions ∪ postconditions` carries AT MOST ONE
    /// [`ConditionKind::ALL`] variant — the union arm of the
    /// (precondition, postcondition, condition-union) cardinality
    /// "≤ 1" triad on [`EphemeralSpec`] closing the "at most one kind
    /// covered" arm on the union of the two condition slots on the
    /// closed-set-inversion axis. The Boolean cardinality "≤ 1"
    /// negation peer of [`Self::has_multiple_distinct_condition_kind`]
    /// (≥ 2 many-arm) under the definitional negation
    /// `!has_multiple_distinct_condition_kind`, and the trichotomy-
    /// union peer of `!has_any_distinct_condition_kind` (=0 empty-
    /// endpoint) OR [`Self::has_unique_distinct_condition_kind`] (=1
    /// mid-endpoint) — names the arrangement space where the ephemeral
    /// spec is EMPTY-OR-SINGLETON on the union (zero or exactly one
    /// kind present across the union of the two slices).
    ///
    /// Composed body: `!self.has_multiple_distinct_condition_kind()`
    /// — a definitional negation of the many-arm union primitive.
    /// Short-circuits transitively through
    /// [`Self::has_multiple_distinct_condition_kind`]'s two-step
    /// short-circuit walk over [`ConditionKind::ALL`] under
    /// [`Self::has_condition_kind`]. Byte-for-byte peer of
    /// [`crate::boundary::ConditionSliceExt::has_at_most_one_distinct_kind`]
    /// one slice-layer down, lifted to compose against
    /// [`Self::has_condition_kind`]'s pre-OR-post union rather than
    /// against a single slice's `has_kind`.
    ///
    /// # Peer on the point-domain surface — [`crate::boundary::Boundary::has_at_most_one_distinct_condition_kind`]
    ///
    /// Byte-identical signature `(&Self) -> bool`, byte-identical
    /// definitional-negation body composed against the point-domain
    /// surface's own many-arm union primitive. Both methods compose
    /// against the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_at_most_one_distinct_kind`]
    /// via the two-slice union — a regression at the per-slice "≤ 1"
    /// negation fails at that primitive's tests rather than as silent
    /// drift at either struct-level empty-or-singleton caller.
    ///
    /// # Sibling to [`Self::distinct_condition_kinds`] /
    /// [`Self::distinct_condition_kind_count`]
    ///
    /// Cardinality "≤ 1" Boolean projection of the widened + scalar
    /// closed-set-inversion primitives on the ephemeral-union
    /// surface — where those primitives return the FULL distinct SET
    /// (a `Vec` of every present kind) and its cardinality (a `usize`
    /// in `0..=ConditionKind::ALL.len()`),
    /// `has_at_most_one_distinct_condition_kind` collapses either the
    /// widened primitive to its `≤ 1`-length Boolean or the scalar
    /// to its `<= 1` cardinality-negation Boolean. Strictly cheaper
    /// than either widened primitive on every arm because the
    /// underlying many-arm walk short-circuits at the second distinct
    /// kind — a subsequent bit-flip surfaces at ONE substrate call
    /// with no allocation.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the cardinality "≤ 1" projection on the
    /// distinct axis composes the SAME definitional negation of the
    /// many-arm two-step short-circuit walk on both this ephemeral
    /// surface and the point-domain [`crate::boundary::Boundary`]
    /// surface). THEORY.md §VI.1 (generation over composition — a
    /// new [`ConditionKind`] variant reaches both surfaces'
    /// cardinality "≤ 1" triads mechanically through the delegated
    /// union primitive).
    #[must_use]
    pub fn has_at_most_one_distinct_condition_kind(&self) -> bool {
        !self.has_multiple_distinct_condition_kind()
    }

    /// `true` iff [`Self::preconditions`] carries AT MOST ONE
    /// [`ConditionKind::ALL`] variant — the precondition-side arm of
    /// the (precondition, postcondition, condition-union) cardinality
    /// "≤ 1" triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_at_most_one_distinct_kind`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_at_most_one_distinct_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice "≤ 1" negation fails at that primitive's tests
    /// rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn has_at_most_one_distinct_precondition_kind(&self) -> bool {
        self.preconditions.has_at_most_one_distinct_kind()
    }

    /// `true` iff [`Self::postconditions`] carries AT MOST ONE
    /// [`ConditionKind::ALL`] variant — the postcondition-side arm of
    /// the (precondition, postcondition, condition-union) cardinality
    /// "≤ 1" triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_at_most_one_distinct_kind`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_at_most_one_distinct_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::has_at_most_one_distinct_precondition_kind`] for the
    /// full rationale — the two methods share ONE lift motivation,
    /// ONE fail-before-pass-after composition-law pin, and ONE two-
    /// surface parity contract with the point-domain
    /// [`crate::boundary::Boundary`] cardinality "≤ 1" peer methods.
    #[must_use]
    pub fn has_at_most_one_distinct_postcondition_kind(&self) -> bool {
        self.postconditions.has_at_most_one_distinct_kind()
    }

    /// `true` iff `preconditions ∪ postconditions` is MISSING EXACTLY
    /// ONE [`ConditionKind::ALL`] variant — the union arm of the
    /// (precondition, postcondition, condition-union) cardinality-
    /// mid-endpoint triad on [`EphemeralSpec`] closing the near-
    /// saturation-endpoint on the union of the two condition slots.
    /// The Boolean cardinality-mid-endpoint fast-path peer of
    /// [`Self::is_condition_kind_saturated`]: where the saturation-
    /// endpoint predicate answers "is the union covered by every ALL
    /// variant?", `has_unique_missing_condition_kind` answers "is the
    /// union one kind away from covered?".
    ///
    /// Composed body: constructs a two-step-short-circuit walk over
    /// [`ConditionKind::ALL`] under the [`Self::has_condition_kind`]
    /// union primitive negated — the first missing union arm surfaces,
    /// then the walk short-circuits at the second. Byte-for-byte peer
    /// of
    /// [`crate::boundary::ConditionSliceExt::has_unique_missing_kind`]
    /// one slice-layer down, lifted to compose against
    /// [`Self::has_condition_kind`]'s pre-OR-post union rather than
    /// against a single slice's `has_kind`.
    ///
    /// # Peer on the point-domain surface — [`crate::boundary::Boundary::has_unique_missing_condition_kind`]
    ///
    /// Byte-identical signature `(&Self) -> bool`, byte-identical
    /// two-step short-circuit body composed against the point-domain
    /// surface's own union primitive. Both methods compose against
    /// the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_unique_missing_kind`]
    /// via the two-slice union — a regression at the per-slice
    /// near-saturation-endpoint walk fails at that primitive's tests
    /// rather than as silent drift at either struct-level near-
    /// saturation caller.
    ///
    /// # Sibling to [`Self::missing_condition_kinds`] /
    /// [`Self::missing_condition_kind_count`]
    ///
    /// Cardinality-mid-endpoint Boolean projection of the widened +
    /// scalar closed-set-complement primitives on the ephemeral-union
    /// surface — where those primitives return the FULL missing SET
    /// (a `Vec` of every absent kind) and its cardinality (a `usize`
    /// in `0..=ConditionKind::ALL.len()`),
    /// `has_unique_missing_condition_kind` collapses either the
    /// widened primitive to its unit-length Boolean or the scalar to
    /// its `== 1` cardinality-mid-endpoint Boolean. Strictly cheaper
    /// than either widened primitive on every arm with `≥ 2` missing
    /// kinds because the negation short-circuits at the second
    /// missing kind rather than allocating the closed-set-complement
    /// scan or walking every slot to build the scalar cardinality.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the cardinality-mid-endpoint projection on
    /// the missing axis composes the SAME two-step short-circuit walk
    /// under a two-slice union negation on both this ephemeral
    /// surface and the point-domain
    /// [`crate::boundary::Boundary`] surface). THEORY.md §VI.1
    /// (generation over composition — a new [`ConditionKind`]
    /// variant reaches both surfaces' cardinality-mid-endpoint triads
    /// mechanically through the delegated union primitive).
    #[must_use]
    pub fn has_unique_missing_condition_kind(&self) -> bool {
        let mut it = ConditionKind::ALL
            .iter()
            .copied()
            .filter(|k| !self.has_condition_kind(*k));
        it.next().is_some() && it.next().is_none()
    }

    /// `true` iff [`Self::preconditions`] is MISSING EXACTLY ONE
    /// [`ConditionKind::ALL`] variant — the precondition-side arm of
    /// the (precondition, postcondition, condition-union)
    /// cardinality-mid-endpoint triad on [`EphemeralSpec`]. Thin
    /// typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_unique_missing_kind`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_unique_missing_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice two-step short-circuit walk under negation fails at
    /// that primitive's tests rather than as silent drift at either
    /// struct-level arm.
    #[must_use]
    pub fn has_unique_missing_precondition_kind(&self) -> bool {
        self.preconditions.has_unique_missing_kind()
    }

    /// `true` iff [`Self::postconditions`] is MISSING EXACTLY ONE
    /// [`ConditionKind::ALL`] variant — the postcondition-side arm of
    /// the (precondition, postcondition, condition-union)
    /// cardinality-mid-endpoint triad on [`EphemeralSpec`]. Thin
    /// typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_unique_missing_kind`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_unique_missing_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::has_unique_missing_precondition_kind`] for the full
    /// rationale — the two methods share ONE lift motivation, ONE
    /// fail-before-pass-after composition-law pin, and ONE two-surface
    /// parity contract with the point-domain
    /// [`crate::boundary::Boundary`] cardinality-mid-endpoint peer
    /// methods.
    #[must_use]
    pub fn has_unique_missing_postcondition_kind(&self) -> bool {
        self.postconditions.has_unique_missing_kind()
    }

    /// `true` iff `preconditions ∪ postconditions` is MISSING AT
    /// LEAST TWO [`ConditionKind::ALL`] variants — the union arm of
    /// the (precondition, postcondition, condition-union) cardinality-
    /// many-arm triad on [`EphemeralSpec`] closing the "≥ 2 holes
    /// remaining" arm on the union of the two condition slots. The
    /// Boolean cardinality many-arm fast-path peer of
    /// [`Self::has_unique_missing_condition_kind`] (=1 arm) and
    /// [`Self::is_condition_kind_saturated`] (=0 arm): closes the
    /// {0, 1, ≥2} trichotomy on the missing axis at the ephemeral
    /// union struct layer.
    ///
    /// Composed body: constructs a two-step-short-circuit walk over
    /// [`ConditionKind::ALL`] under the [`Self::has_condition_kind`]
    /// union primitive negated — pulls up to two hits off the
    /// filtered iterator; the primitive returns `true` iff BOTH the
    /// first and the second are [`Some`]. Byte-for-byte peer of
    /// [`crate::boundary::ConditionSliceExt::has_multiple_missing_kinds`]
    /// one slice-layer down, lifted to compose against
    /// [`Self::has_condition_kind`]'s pre-OR-post union rather than
    /// against a single slice's `has_kind`.
    ///
    /// # Peer on the point-domain surface — [`crate::boundary::Boundary::has_multiple_missing_condition_kind`]
    ///
    /// Byte-identical signature `(&Self) -> bool`, byte-identical
    /// two-step short-circuit body composed against the point-domain
    /// surface's own union primitive. Both methods compose against
    /// the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_multiple_missing_kinds`]
    /// via the two-slice union — a regression at the per-slice many-
    /// arm walk fails at that primitive's tests rather than as silent
    /// drift at either struct-level many-missing caller.
    ///
    /// # Sibling to [`Self::missing_condition_kinds`] /
    /// [`Self::missing_condition_kind_count`]
    ///
    /// Cardinality-many-arm Boolean projection of the widened +
    /// scalar closed-set-complement primitives on the ephemeral-union
    /// surface — where those primitives return the FULL missing SET
    /// (a `Vec` of every absent kind) and its cardinality (a `usize`
    /// in `0..=ConditionKind::ALL.len()`),
    /// `has_multiple_missing_condition_kind` collapses either the
    /// widened primitive to its ≥ 2-length Boolean or the scalar to
    /// its `>= 2` cardinality-many-arm Boolean. Strictly cheaper than
    /// either widened primitive on every arm with `≥ 2` missing kinds
    /// because the negation short-circuits at the second missing kind
    /// rather than allocating the closed-set-complement scan or
    /// walking every slot to build the scalar cardinality.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the cardinality-many-arm projection on the
    /// missing axis composes the SAME two-step short-circuit walk
    /// under a two-slice union negation on both this ephemeral
    /// surface and the point-domain
    /// [`crate::boundary::Boundary`] surface). THEORY.md §VI.1
    /// (generation over composition — a new [`ConditionKind`]
    /// variant reaches both surfaces' cardinality-many-arm triads
    /// mechanically through the delegated union primitive).
    #[must_use]
    pub fn has_multiple_missing_condition_kind(&self) -> bool {
        let mut it = ConditionKind::ALL
            .iter()
            .copied()
            .filter(|k| !self.has_condition_kind(*k));
        it.next().is_some() && it.next().is_some()
    }

    /// `true` iff [`Self::preconditions`] is MISSING AT LEAST TWO
    /// [`ConditionKind::ALL`] variants — the precondition-side arm of
    /// the (precondition, postcondition, condition-union) cardinality-
    /// many-arm triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_multiple_missing_kinds`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_multiple_missing_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice two-step short-circuit walk under negation fails at
    /// that primitive's tests rather than as silent drift at either
    /// struct-level arm.
    #[must_use]
    pub fn has_multiple_missing_precondition_kind(&self) -> bool {
        self.preconditions.has_multiple_missing_kinds()
    }

    /// `true` iff [`Self::postconditions`] is MISSING AT LEAST TWO
    /// [`ConditionKind::ALL`] variants — the postcondition-side arm of
    /// the (precondition, postcondition, condition-union) cardinality-
    /// many-arm triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_multiple_missing_kinds`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_multiple_missing_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::has_multiple_missing_precondition_kind`] for the full
    /// rationale — the two methods share ONE lift motivation, ONE
    /// fail-before-pass-after composition-law pin, and ONE two-surface
    /// parity contract with the point-domain
    /// [`crate::boundary::Boundary`] cardinality-many-arm peer
    /// methods.
    #[must_use]
    pub fn has_multiple_missing_postcondition_kind(&self) -> bool {
        self.postconditions.has_multiple_missing_kinds()
    }

    /// `true` iff `preconditions ∪ postconditions` is MISSING AT MOST
    /// ONE [`ConditionKind::ALL`] variant — the union arm of the
    /// (precondition, postcondition, condition-union) cardinality
    /// "≤ 1" triad on [`EphemeralSpec`] closing the "at most one hole
    /// remaining" arm on the union of the two condition slots. The
    /// Boolean cardinality "≤ 1" negation peer of
    /// [`Self::has_multiple_missing_condition_kind`] (≥ 2 many-arm)
    /// under the definitional negation
    /// `!has_multiple_missing_condition_kind`, and the trichotomy-
    /// union peer of [`Self::is_condition_kind_saturated`] (=0
    /// zero-arm) OR [`Self::has_unique_missing_condition_kind`] (=1
    /// mid-endpoint) — names the arrangement space where the
    /// ephemeral spec is SATURATED-OR-NEAR-SATURATED on the union
    /// (zero or exactly one kind missing across the union of the two
    /// slices).
    ///
    /// Composed body: `!self.has_multiple_missing_condition_kind()`
    /// — a definitional negation of the many-arm union primitive.
    /// Short-circuits transitively through
    /// [`Self::has_multiple_missing_condition_kind`]'s two-step short-
    /// circuit walk over [`ConditionKind::ALL`] under negated
    /// [`Self::has_condition_kind`]. Byte-for-byte peer of
    /// [`crate::boundary::ConditionSliceExt::has_at_most_one_missing_kind`]
    /// one slice-layer down, lifted to compose against
    /// [`Self::has_condition_kind`]'s pre-OR-post union rather than
    /// against a single slice's `has_kind`.
    ///
    /// # Peer on the point-domain surface — [`crate::boundary::Boundary::has_at_most_one_missing_condition_kind`]
    ///
    /// Byte-identical signature `(&Self) -> bool`, byte-identical
    /// definitional-negation body composed against the point-domain
    /// surface's own many-arm union primitive. Both methods compose
    /// against the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_at_most_one_missing_kind`]
    /// via the two-slice union — a regression at the per-slice "≤ 1"
    /// negation fails at that primitive's tests rather than as silent
    /// drift at either struct-level near-saturation-or-saturated
    /// caller.
    ///
    /// # Sibling to [`Self::missing_condition_kinds`] /
    /// [`Self::missing_condition_kind_count`]
    ///
    /// Cardinality "≤ 1" Boolean projection of the widened + scalar
    /// closed-set-complement primitives on the ephemeral-union
    /// surface — where those primitives return the FULL missing SET
    /// (a `Vec` of every absent kind) and its cardinality (a `usize`
    /// in `0..=ConditionKind::ALL.len()`),
    /// `has_at_most_one_missing_condition_kind` collapses either the
    /// widened primitive to its `≤ 1`-length Boolean or the scalar
    /// to its `<= 1` cardinality-negation Boolean. Strictly cheaper
    /// than either widened primitive on every arm because the
    /// underlying many-arm walk short-circuits at the second missing
    /// kind — a subsequent bit-flip surfaces at ONE substrate call
    /// with no allocation.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the cardinality "≤ 1" projection on the
    /// missing axis composes the SAME definitional negation of the
    /// many-arm two-step short-circuit walk on both this ephemeral
    /// surface and the point-domain [`crate::boundary::Boundary`]
    /// surface). THEORY.md §VI.1 (generation over composition — a
    /// new [`ConditionKind`] variant reaches both surfaces'
    /// cardinality "≤ 1" triads mechanically through the delegated
    /// union primitive).
    #[must_use]
    pub fn has_at_most_one_missing_condition_kind(&self) -> bool {
        !self.has_multiple_missing_condition_kind()
    }

    /// `true` iff [`Self::preconditions`] is MISSING AT MOST ONE
    /// [`ConditionKind::ALL`] variant — the precondition-side arm of
    /// the (precondition, postcondition, condition-union) cardinality
    /// "≤ 1" triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_at_most_one_missing_kind`]
    /// over [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_at_most_one_missing_precondition_kind`]
    /// on the point-domain surface — both peers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice "≤ 1" negation fails at that primitive's tests
    /// rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn has_at_most_one_missing_precondition_kind(&self) -> bool {
        self.preconditions.has_at_most_one_missing_kind()
    }

    /// `true` iff [`Self::postconditions`] is MISSING AT MOST ONE
    /// [`ConditionKind::ALL`] variant — the postcondition-side arm of
    /// the (precondition, postcondition, condition-union) cardinality
    /// "≤ 1" triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_at_most_one_missing_kind`]
    /// over [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_at_most_one_missing_postcondition_kind`]
    /// on the point-domain surface. See
    /// [`Self::has_at_most_one_missing_precondition_kind`] for the
    /// full rationale — the two methods share ONE lift motivation,
    /// ONE fail-before-pass-after composition-law pin, and ONE two-
    /// surface parity contract with the point-domain
    /// [`crate::boundary::Boundary`] cardinality "≤ 1" peer methods.
    #[must_use]
    pub fn has_at_most_one_missing_postcondition_kind(&self) -> bool {
        self.postconditions.has_at_most_one_missing_kind()
    }

    /// `true` iff `preconditions ∪ postconditions` carries NO
    /// [`crate::boundary::Condition`] with the given [`ConditionKind`]
    /// — the peer of
    /// [`crate::boundary::Boundary::lacks_condition_kind`] on the
    /// [`EphemeralSpec`] sugar surface.
    ///
    /// # Composed body — byte-identical to
    /// [`crate::boundary::Boundary::lacks_condition_kind`]
    ///
    /// `!self.has_condition_kind(kind)` — the definitional negation of
    /// the two-slice union primitive [`Self::has_condition_kind`].
    /// Byte-identical to the peer method on the point-domain
    /// [`crate::boundary::Boundary`] surface — both compose against
    /// the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::lacks_kind`] via the
    /// two-slice union so a regression at the per-slice negation
    /// fails at that primitive's tests rather than as silent drift at
    /// either struct-level complement caller.
    ///
    /// # Sibling to [`Self::missing_condition_kinds`] /
    /// [`Self::missing_condition_kind_count`]
    ///
    /// Per-kind Boolean projection of the widened + scalar closed-set-
    /// complement primitives on the ephemeral-union surface — where
    /// those primitives return the FULL missing SET (a `Vec` of every
    /// absent kind) and its cardinality (a `usize`),
    /// `lacks_condition_kind` collapses the missing SET to its
    /// per-kind membership Boolean for ONE addressed kind. Strictly
    /// cheaper than reaching for the widened primitive on every
    /// per-kind question because the negation short-circuits through
    /// [`Self::has_condition_kind`] rather than allocating the
    /// closed-set-complement scan.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the per-kind closed-set-complement
    /// projection composes the SAME two-slice union negation on both
    /// this ephemeral surface and the point-domain
    /// [`crate::boundary::Boundary`] surface under definitional
    /// negation). THEORY.md §VI.1 (generation over composition — a
    /// future [`ConditionKind`] variant reaches both surfaces'
    /// per-kind-complement triads mechanically through the delegated
    /// union primitive).
    #[must_use]
    pub fn lacks_condition_kind(&self, kind: ConditionKind) -> bool {
        !self.has_condition_kind(kind)
    }

    /// `true` iff [`Self::preconditions`] carries NO
    /// [`crate::boundary::Condition`] with the given [`ConditionKind`]
    /// — the precondition-side arm of the (precondition, postcondition,
    /// condition-union) per-kind-complement triad on [`EphemeralSpec`].
    /// Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::lacks_kind`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::lacks_precondition_kind`] on the
    /// point-domain surface — both peers compose against the SAME
    /// slice-level substrate primitive so a regression at the
    /// per-slice negation fails at that primitive's tests rather than
    /// as silent drift at either struct-level arm.
    #[must_use]
    pub fn lacks_precondition_kind(&self, kind: ConditionKind) -> bool {
        self.preconditions.lacks_kind(kind)
    }

    /// `true` iff [`Self::postconditions`] carries NO
    /// [`crate::boundary::Condition`] with the given [`ConditionKind`]
    /// — the postcondition-side arm of the (precondition, postcondition,
    /// condition-union) per-kind-complement triad on [`EphemeralSpec`].
    /// Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::lacks_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::lacks_postcondition_kind`] on the
    /// point-domain surface. See [`Self::lacks_precondition_kind`] for
    /// the full rationale — the two methods share ONE lift motivation,
    /// ONE fail-before-pass-after composition-law pin, and ONE
    /// two-surface parity contract with the point-domain
    /// [`crate::boundary::Boundary`] per-kind-complement peer methods.
    #[must_use]
    pub fn lacks_postcondition_kind(&self, kind: ConditionKind) -> bool {
        self.postconditions.lacks_kind(kind)
    }

    /// `true` iff `preconditions ∪ postconditions` carries at least
    /// one [`crate::boundary::Condition`] with the given
    /// [`ConditionKind`] AND carries no [`crate::boundary::Condition`]
    /// whose kind is anything OTHER than `kind` — the union arm of
    /// the (precondition, postcondition, condition-union) kind-scoped
    /// strict-refinement triad on [`EphemeralSpec`], byte-for-byte
    /// peer of the point-domain
    /// [`crate::boundary::Boundary::has_only_condition_kind`] under
    /// the same fused-closed-set-walk body.
    ///
    /// # Composed body
    ///
    /// A FUSED short-circuit closed-set walk over
    /// [`ConditionKind::ALL`] under [`Self::has_condition_kind`] that
    /// returns `false` at the EARLIEST kind whose presence spans
    /// either slice's populated set and is NOT `kind`, and returns
    /// `true` iff the sweep completes with `kind` seen as the sole
    /// distinct populated kind. Strictly cheaper than the widened
    /// composition
    /// `self.distinct_condition_kinds() == vec![kind]` (which
    /// allocates the distinct-kind Vec before the equality test) or
    /// the (pre, post) AND-of-strict-refinement
    /// `self.preconditions.has_only_kind(kind)
    ///     && self.postconditions.has_only_kind(kind)` (which is TOO
    /// STRICT — a single-slice-populated arrangement whose empty side
    /// returns `false` fails this AND but IS well-formed on the
    /// union).
    ///
    /// # Peer on the point-domain surface — [`crate::boundary::Boundary::has_only_condition_kind`]
    ///
    /// Byte-identical signature `(&Self, ConditionKind) -> bool`,
    /// byte-identical fused-closed-set-walk body, on the point-domain
    /// surface whose pre/post condition vectors live inside a
    /// [`crate::boundary::Boundary`] slot. Both methods compose
    /// against the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_only_kind`] via the
    /// two-slice union composed through [`Self::has_condition_kind`]
    /// — a regression at the per-slice fused walk fails at that
    /// primitive's tests rather than as silent drift at either
    /// struct-level kind-scoped-strict-refinement caller.
    ///
    /// # Compounding
    ///
    /// A future coherence check verifying "every ephemeral spec whose
    /// postconditions carry ONLY `ClosedLoopAuth` (no `JobAttested`,
    /// no `Cel`, ...) is a well-formed closed-loop probe" reads
    /// `spec.has_only_condition_kind(ConditionKind::ClosedLoopAuth)`
    /// at ONE call site rather than restating either widened
    /// composition. A `has-only-<kind>` require-tag classifier arm on
    /// the ephemeral surface reaches this primitive at ONE substrate
    /// call — byte-for-byte peer of the tagged-union
    /// `has-only-<kind>` classifier one struct-layer up under the
    /// SAME fused short-circuit walk shape.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the kind-scoped strict-refinement projection
    /// composes the SAME fused short-circuit closed-set walk under
    /// [`Self::has_condition_kind`] on both this ephemeral surface
    /// and the point-domain [`crate::boundary::Boundary`] surface).
    /// THEORY.md §VI.1 (generation over composition — a future
    /// [`ConditionKind`] variant reaches both surfaces' kind-scoped
    /// strict-refinement triads mechanically through the delegated
    /// union primitive).
    #[must_use]
    pub fn has_only_condition_kind(&self, kind: ConditionKind) -> bool {
        let mut saw_kind = false;
        for k in ConditionKind::ALL {
            if !self.has_condition_kind(k) {
                continue;
            }
            if k == kind {
                saw_kind = true;
            } else {
                return false;
            }
        }
        saw_kind
    }

    /// `true` iff [`Self::preconditions`] carries at least one
    /// [`crate::boundary::Condition`] with the given
    /// [`ConditionKind`] AND carries no
    /// [`crate::boundary::Condition`] whose kind is anything OTHER
    /// than `kind` — the precondition-side arm of the (precondition,
    /// postcondition, condition-union) kind-scoped strict-refinement
    /// triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_only_kind`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_only_precondition_kind`] on
    /// the point-domain surface — both peers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-
    /// slice fused walk fails at that primitive's tests rather than
    /// as silent drift at either struct-level arm.
    #[must_use]
    pub fn has_only_precondition_kind(&self, kind: ConditionKind) -> bool {
        self.preconditions.has_only_kind(kind)
    }

    /// `true` iff [`Self::postconditions`] carries at least one
    /// [`crate::boundary::Condition`] with the given
    /// [`ConditionKind`] AND carries no
    /// [`crate::boundary::Condition`] whose kind is anything OTHER
    /// than `kind` — the postcondition-side arm of the (precondition,
    /// postcondition, condition-union) kind-scoped strict-refinement
    /// triad on [`EphemeralSpec`]. Thin typed delegate to
    /// [`crate::boundary::ConditionSliceExt::has_only_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::has_only_postcondition_kind`] on
    /// the point-domain surface. See [`Self::has_only_precondition_kind`]
    /// for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin,
    /// and ONE two-surface parity contract with the point-domain
    /// [`crate::boundary::Boundary`] kind-scoped-strict-refinement
    /// peer methods.
    #[must_use]
    pub fn has_only_postcondition_kind(&self, kind: ConditionKind) -> bool {
        self.postconditions.has_only_kind(kind)
    }

    /// `true` iff `preconditions ∪ postconditions` carries NO
    /// [`crate::boundary::Condition`] with the given [`ConditionKind`]
    /// AND carries at least one [`crate::boundary::Condition`] for
    /// every OTHER [`ConditionKind`] — the union arm of the
    /// (precondition, postcondition, condition-union) kind-scoped
    /// strict-refinement-on-missing triad on [`EphemeralSpec`], byte-
    /// for-byte peer of the point-domain
    /// [`crate::boundary::Boundary::lacks_only_condition_kind`] under
    /// the same fused-closed-set-walk body on the missing axis.
    ///
    /// # Composed body
    ///
    /// A FUSED short-circuit closed-set walk over
    /// [`ConditionKind::ALL`] under [`Self::has_condition_kind`] that
    /// skips every populated kind, returns `false` at the EARLIEST
    /// kind whose absence spans both slices' missing sets and is NOT
    /// `kind`, and returns `true` iff the sweep completes with `kind`
    /// seen as the sole missing kind. Strictly cheaper than the
    /// widened composition
    /// `self.missing_condition_kinds() == vec![kind]` (which allocates
    /// the missing-kind Vec before the equality test) or the
    /// (pre AND post) AND-of-strict-refinement
    /// `self.preconditions.lacks_only_kind(kind)
    ///     && self.postconditions.lacks_only_kind(kind)` (which is TOO
    /// STRICT — a single-slice-populated arrangement whose empty side
    /// returns `false` fails this AND but IS well-formed on the
    /// union).
    ///
    /// # Peer on the point-domain surface — [`crate::boundary::Boundary::lacks_only_condition_kind`]
    ///
    /// Byte-identical signature `(&Self, ConditionKind) -> bool`,
    /// byte-identical fused-closed-set-walk body under complement, on
    /// the point-domain surface whose pre/post condition vectors live
    /// inside a [`crate::boundary::Boundary`] slot. Both methods
    /// compose against the SAME slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::lacks_only_kind`] via
    /// the two-slice union composed through
    /// [`Self::has_condition_kind`] — a regression at the per-slice
    /// fused walk under complement fails at that primitive's tests
    /// rather than as silent drift at either struct-level kind-scoped-
    /// strict-refinement-on-missing caller.
    ///
    /// # Compounding
    ///
    /// A future coherence check verifying "every partially-attested
    /// ephemeral closed-loop probe is missing ONLY the
    /// `ClosedLoopAuth` postcondition" reads
    /// `spec.lacks_only_condition_kind(ConditionKind::ClosedLoopAuth)`
    /// at ONE call site rather than restating either widened
    /// composition. A `lacks-only-<kind>` require-tag classifier arm
    /// on the ephemeral surface reaches this primitive at ONE
    /// substrate call — byte-for-byte peer of the tagged-union
    /// `lacks-only-<kind>` classifier one struct-layer up under the
    /// SAME fused short-circuit walk shape.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the kind-scoped strict-refinement projection
    /// on the missing axis composes the SAME fused short-circuit
    /// closed-set walk under [`Self::has_condition_kind`] on both this
    /// ephemeral surface and the point-domain
    /// [`crate::boundary::Boundary`] surface). THEORY.md §VI.1
    /// (generation over composition — a future [`ConditionKind`]
    /// variant reaches both surfaces' kind-scoped-strict-refinement-
    /// on-missing triads mechanically through the delegated union
    /// primitive).
    #[must_use]
    pub fn lacks_only_condition_kind(&self, kind: ConditionKind) -> bool {
        let mut saw_kind = false;
        for k in ConditionKind::ALL {
            if self.has_condition_kind(k) {
                continue;
            }
            if k == kind {
                saw_kind = true;
            } else {
                return false;
            }
        }
        saw_kind
    }

    /// `true` iff [`Self::preconditions`] carries NO
    /// [`crate::boundary::Condition`] with the given [`ConditionKind`]
    /// AND carries at least one [`crate::boundary::Condition`] for
    /// every OTHER [`ConditionKind`] — the precondition-side arm of
    /// the (precondition, postcondition, condition-union) kind-scoped-
    /// strict-refinement-on-missing triad on [`EphemeralSpec`]. Thin
    /// typed delegate to
    /// [`crate::boundary::ConditionSliceExt::lacks_only_kind`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::lacks_only_precondition_kind`] on
    /// the point-domain surface — both peers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-
    /// slice fused walk under complement fails at that primitive's
    /// tests rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn lacks_only_precondition_kind(&self, kind: ConditionKind) -> bool {
        self.preconditions.lacks_only_kind(kind)
    }

    /// `true` iff [`Self::postconditions`] carries NO
    /// [`crate::boundary::Condition`] with the given [`ConditionKind`]
    /// AND carries at least one [`crate::boundary::Condition`] for
    /// every OTHER [`ConditionKind`] — the postcondition-side arm of
    /// the (precondition, postcondition, condition-union) kind-scoped-
    /// strict-refinement-on-missing triad on [`EphemeralSpec`]. Thin
    /// typed delegate to
    /// [`crate::boundary::ConditionSliceExt::lacks_only_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of
    /// [`crate::boundary::Boundary::lacks_only_postcondition_kind`] on
    /// the point-domain surface. See [`Self::lacks_only_precondition_kind`]
    /// for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin,
    /// and ONE two-surface parity contract with the point-domain
    /// [`crate::boundary::Boundary`] kind-scoped-strict-refinement-
    /// on-missing peer methods.
    #[must_use]
    pub fn lacks_only_postcondition_kind(&self, kind: ConditionKind) -> bool {
        self.postconditions.lacks_only_kind(kind)
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

    /// Overlay a single [`ClassificationAxis`] variant onto this
    /// ephemeral spec's authored [`Self::classification`] slot, filling
    /// `None` through [`Classification::gate_compute`] before the
    /// overlay so the resulting slot carries `Some(_)` regardless of
    /// the pre-call state. Fluent chaining primitive: the peer of
    /// [`ProcessSpec::gate_compute_with_axis`] (fresh-spec × axis
    /// overlay) and [`Classification::with_axis`] (arbitrary-base ×
    /// axis overlay) on the ephemeral sugar surface.
    ///
    /// # Substrate ergonomics
    ///
    /// Pre-lift the four-line shape `let mut classification =
    /// Classification::gate_compute(); classification.<axis> =
    /// populated; let spec = EphemeralSpec { classification:
    /// Some(classification), ..ephemeral_fixture() };` (and its newer
    /// three-line peer `let classification =
    /// Classification::gate_compute_with_axis(populated); let spec =
    /// EphemeralSpec { classification: Some(classification),
    /// ..ephemeral_fixture() };`) recurred at THIRTY-SIX hand-authored
    /// callsites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger
    /// inside `tatara-reconciler::bin::tatara-check`'s
    /// `evaluate_ephemeral_require_tag_*` classifier-facing test
    /// module. Post-lift each callsite reads
    /// `let spec = ephemeral_fixture().with_classification_axis(populated);`
    /// — one line, one immutable binding, and every per-axis loop
    /// dispatches its per-iteration axis mutation through the SAME
    /// [`ClassificationAxis::overlay`] trait rather than by directly
    /// poking a `classification.<axis>` field or restating the
    /// `Some(_)` wrap.
    ///
    /// # Fluent chaining semantics
    ///
    /// * `EphemeralSpec { classification: None, .. }
    ///   .with_classification_axis(axis)` produces
    ///   `EphemeralSpec { classification:
    ///   Some(Classification::gate_compute_with_axis(axis)), .. }` —
    ///   the `None`-arm short-circuit fills through
    ///   [`Classification::gate_compute`] identically to the sibling
    ///   [`Self::resolved_classification`] resolver on the read side.
    /// * `EphemeralSpec { classification: Some(prior), .. }
    ///   .with_classification_axis(axis)` produces
    ///   `EphemeralSpec { classification: Some(prior.with_axis(axis)),
    ///   .. }` — the axis overlay composes onto the existing carrier
    ///   via [`ClassificationAxis::overlay`], preserving every other
    ///   axis slot on `prior`. Chained calls
    ///   `.with_classification_axis(a).with_classification_axis(b)`
    ///   compose arbitrary N-axis conjunctions on the ephemeral
    ///   sugar surface with the same order-independence guarantee
    ///   [`Classification::with_axis`] carries on distinct-slot axes.
    ///
    /// # Sibling to [`ProcessSpec::gate_compute_with_axis`]
    ///
    /// Same (spec-carrier × axis) shape, one refinement lower on
    /// the composition-depth axis: `ProcessSpec::gate_compute_with_axis`
    /// owns the (fresh-`gate_compute_defaults`-spec × axis-overlay)
    /// construction on the point-surface carrier;
    /// [`Self::with_classification_axis`] owns the
    /// (arbitrary-`EphemeralSpec` × axis-overlay-onto-authored-classification)
    /// construction on the ephemeral sugar-surface carrier. Both
    /// primitives compose through the SAME
    /// [`ClassificationAxis::overlay`] trait so a regression on any
    /// axis's overlay surfaces at both composer owners' pin sets
    /// simultaneously.
    ///
    /// # Compounding
    ///
    /// A future SIXTH classification axis lands as ONE peer
    /// `impl ClassificationAxis` — every ephemeral-surface fixture
    /// that binds through this primitive picks up the sixth axis
    /// mechanically without a `classification.<new-axis> = value;`
    /// restatement per site. A future audit dispatcher walking the
    /// (ephemeral-surface × axis-loop) shape (per-axis matrix
    /// generator, closed-set-sweep sagas, per-axis-XOR-partition-
    /// witness synthesis on the ephemeral side) binds through the
    /// SAME composer regardless of which axis it targets. Directly
    /// benefits the P1 caixa-tatara renderer target
    /// (`(defaplicacao …)` → `Process` mechanical lowering test
    /// fixtures that construct authored classifications through the
    /// ephemeral sugar surface) and future ephemeral-surface XOR-
    /// partition landmark tests peer to the point-surface pins in
    /// `tatara-check.rs`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the [`ClassificationAxis::overlay`] trait
    /// owns the axis-dispatch proof at ONE site and this primitive
    /// extends the ONE-site guarantee to the (ephemeral-spec ×
    /// authored-classification × axis-overlay) construction shape.
    /// THEORY.md §VI.1 — generation over composition; the 3-to-4-line
    /// hand-authored classification-then-wrap shape recurred at ≥ 36
    /// hand-authored callsites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold and is lifted onto ONE substrate owner
    /// here.
    #[must_use]
    pub fn with_classification_axis<A: ClassificationAxis>(mut self, axis: A) -> Self {
        let mut c = self
            .classification
            .take()
            .unwrap_or_else(Classification::gate_compute);
        axis.overlay(&mut c);
        self.classification = Some(c);
        self
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

    /// Derived-boolean predicate — does this ephemeral spec's
    /// resolved [`Classification`]'s
    /// [`crate::classification::DataClassification`] project to `true`
    /// under [`crate::classification::DataClassification::is_public`]?
    /// Byte-for-byte peer of [`Classification::data_is_public`]
    /// wrapped through the [`Self::resolved_classification`] resolver
    /// so an operator-omitted `:classification` slot on
    /// `(defephemeral …)` still answers via the substrate default.
    /// The ONE ephemeral-surface substrate primitive that owns the
    /// `(&EphemeralSpec) -> bool` derived-nullary-boolean walk on the
    /// freely-distributable-data question — the positive framing peer
    /// of [`Self::data_is_restricted`].
    ///
    /// # Thirteenth derived-nullary-boolean peer on the ephemeral surface — CLOSES the data axis
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// [`Self::substrate_is_telemetry`], and [`Self::calm_is_monotone`]
    /// on the ephemeral surface's (resolver-hop × derived-nullary-bool)
    /// shape — the THIRTEENTH peer overall and the THIRD peer
    /// threading the classification-`data_classification` axis. This
    /// peer CLOSES the data axis on the ephemeral surface into the
    /// FULL binary XOR partition contract
    /// `data_is_public ⊕ data_is_restricted` — sealed on this surface
    /// by `ephemeral_data_probes_form_binary_xor_partition_over_all`,
    /// the resolver-hop peer of the parent-composed
    /// `classification_data_probes_form_binary_xor_partition_over_all`.
    /// Structural twin of the sibling calm-axis binary XOR sealed on
    /// this surface by
    /// `ephemeral_calm_probes_form_binary_xor_partition_over_all`,
    /// lifted through the resolver hop from the six-variant data-axis
    /// closed set to the ephemeral surface. The resolver-hop shape is
    /// byte-identical across all thirteen peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `data_is_public()` returns `true` iff
    /// `self.resolved_classification().data_is_public()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// [`crate::classification::DataClassification::default =
    /// Internal`] via `#[default]`, a bare ephemeral spec with no
    /// `:classification` slot answers `false` — every unadorned
    /// `(defephemeral …)` reads as access-controlled by default (safe
    /// under compliance baseline: an operator must deliberately opt
    /// the dataset into public distribution rather than the substrate
    /// silently promoting an unadorned Process onto the freely-
    /// distributable path). A regression that dropped the resolver
    /// hop, probed the wrong closed-set arm, or inverted the
    /// projection fails HERE at ONE narrow substrate site before
    /// drifting through every unadorned ephemeral spec's positive-
    /// distribution-framing answer. Mirror-inverted from the sibling
    /// `data_is_restricted_probes_true_on_absent_classification`
    /// (both walk the SAME defaulted `data_classification` field, so
    /// `is_restricted = true` ⇒ `is_public = false` on the closed
    /// set's disjoint XOR partition).
    ///
    /// # Compounding — CLOSES the data axis on the ephemeral surface
    ///
    /// The ephemeral require-tag classifier composes this primitive
    /// as a fixed tag `public-data` on `EPHEMERAL_FIXED_TAG_ARMS`
    /// — byte-for-byte peer of the point surface's `public-data`
    /// fixed tag on `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::data_is_public`] directly. The two-
    /// surface parity contract holds by construction: both surfaces
    /// route through the SAME [`Classification::data_is_public`]
    /// primitive after the ephemeral surface pays ONE resolver hop.
    /// THIRD ephemeral-surface peer on the `data_classification` axis
    /// — CLOSES the axis into a proven-repeatable three-peer sub-
    /// corner (data_is_regulated, data_is_restricted, data_is_public)
    /// whose complementary XOR partition seals on the closed set by
    /// `data_classification_public_xor_restricted` and composes
    /// through the resolver hop as a substrate-wide theorem.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`data_classification`-axis
    /// derived-nullary-boolean probe body composes ONE resolver
    /// primitive ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::data_is_public`]) so every downstream
    /// (`public-data` fixed tags on both surfaces in tatara-check,
    /// future compliance-baseline / audit-log-optional validators
    /// reading the positive distribution framing, future variant
    /// additions on
    /// [`crate::classification::DataClassification`]) binds through
    /// the SAME `data_is_public()` shape rather than restating either
    /// the resolver walk or the closed-set projection composition at
    /// the callsite. THEORY.md §VI.1 — generation over composition; a
    /// future [`crate::classification::DataClassification`] variant
    /// lands at ONE `ALL` entry + ONE `is_public` arm on the closed
    /// set and both surfaces pick it up mechanically.
    #[must_use]
    pub fn data_is_public(&self) -> bool {
        self.resolved_classification().data_is_public()
    }

    /// Derived-boolean predicate — does this ephemeral spec's resolved
    /// [`Classification`]'s
    /// [`crate::classification::Horizon::direction`] slot (defaulted
    /// through [`crate::classification::OptimizationDirection::default =
    /// Minimize`] on absence) project to `true` under
    /// [`crate::classification::OptimizationDirection::prefers_lower`]?
    /// Byte-for-byte peer of
    /// [`crate::classification::Classification::direction_prefers_lower`]
    /// wrapped through the [`Self::resolved_classification`] resolver so
    /// an operator-omitted `:classification` slot on
    /// `(defephemeral …)` still answers via the substrate default. The
    /// ONE ephemeral-surface substrate primitive that owns the
    /// `(&EphemeralSpec) -> bool` derived-nullary-boolean walk on the
    /// lower-is-better optimization-polarity question.
    ///
    /// # Fourteenth derived-nullary-boolean peer on the ephemeral surface — opens the optimization-direction axis
    ///
    /// Peer of the thirteen prior nullary-boolean substrate primitives
    /// on [`EphemeralSpec`] ([`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// [`Self::substrate_is_telemetry`], [`Self::calm_is_monotone`],
    /// [`Self::data_is_public`]) on the ephemeral surface's
    /// (resolver-hop × derived-nullary-bool) shape — the FOURTEENTH
    /// peer overall and the FIRST peer threading the classification-
    /// `horizon.direction` axis on this surface. Opens the SIXTH
    /// classification axis into the ephemeral fixed-tag algebra after
    /// the horizon, calm, data, point, and substrate axes. The
    /// resolver-hop shape is byte-identical across all fourteen peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `direction_prefers_lower()` returns `true` iff
    /// `self.resolved_classification().direction_prefers_lower()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// `horizon: Horizon::default()` whose `direction` field is `None`,
    /// and [`crate::classification::OptimizationDirection::default =
    /// Minimize`] projects `prefers_lower = true`, a bare ephemeral
    /// spec with no `:classification` slot answers `true` — every
    /// unadorned `(defephemeral …)` reads as lower-is-better under the
    /// substrate polarity default (safe under the asymptotic-health
    /// rate-window evaluator's convention: an operator must
    /// deliberately opt into Maximize polarity rather than the
    /// substrate silently flipping every unadorned Process onto the
    /// higher-is-better path). A regression that dropped the resolver
    /// hop, probed the wrong closed-set arm, or inverted the projection
    /// fails HERE at ONE narrow substrate site before drifting through
    /// every unadorned ephemeral spec's rate-window evaluator polarity.
    ///
    /// # Compounding — opens the optimization-direction axis on the ephemeral surface
    ///
    /// The ephemeral require-tag classifier composes this primitive as
    /// a fixed tag `prefers-lower-direction` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `prefers-lower-direction` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::direction_prefers_lower`] directly. The
    /// two-surface parity contract holds by construction: both surfaces
    /// route through the SAME [`Classification::direction_prefers_lower`]
    /// primitive after the ephemeral surface pays ONE resolver hop.
    /// A future antisymmetric peer (`direction_prefers_higher`) closes
    /// the binary XOR partition on this axis — mirror of the calm-axis
    /// (`monotone-calm ⊕ coordination-required`) and data-axis
    /// (`public-data ⊕ data-restricted`) closures on this surface.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`horizon.direction`-axis
    /// derived-nullary-boolean probe body composes ONE resolver
    /// primitive ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::direction_prefers_lower`]) so every
    /// downstream (the `prefers-lower-direction` fixed tags on both
    /// surfaces in tatara-check, future asymptotic-health rate-window
    /// / regression-detector evaluators, future variant additions on
    /// [`crate::classification::OptimizationDirection`]) binds through
    /// the SAME `direction_prefers_lower()` shape rather than restating
    /// either the resolver walk or the closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation over
    /// composition; a future
    /// [`crate::classification::OptimizationDirection`] variant lands
    /// at ONE `ALL` entry + ONE `prefers_lower` arm on the closed set
    /// and both surfaces pick it up mechanically.
    #[must_use]
    pub fn direction_prefers_lower(&self) -> bool {
        self.resolved_classification().direction_prefers_lower()
    }

    /// POSITIVE-FRAMING PEER of [`Self::direction_prefers_lower`] —
    /// does this ephemeral spec's resolved [`Classification`]'s
    /// [`crate::classification::Horizon::direction`] slot (defaulted
    /// through [`crate::classification::OptimizationDirection::default =
    /// Minimize`] on absence) project to `true` under
    /// [`crate::classification::OptimizationDirection::prefers_higher`]?
    /// Byte-for-byte peer of
    /// [`crate::classification::Classification::direction_prefers_higher`]
    /// wrapped through the [`Self::resolved_classification`] resolver
    /// so an operator-omitted `:classification` slot on
    /// `(defephemeral …)` still answers via the substrate default. The
    /// ONE ephemeral-surface substrate primitive that owns the
    /// `(&EphemeralSpec) -> bool` derived-nullary-boolean walk on the
    /// higher-is-better optimization-polarity question.
    ///
    /// # Fifteenth derived-nullary-boolean peer on the ephemeral surface — CLOSES the optimization-direction axis
    ///
    /// Peer of the fourteen prior nullary-boolean substrate primitives
    /// on [`EphemeralSpec`] ([`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// [`Self::substrate_is_telemetry`], [`Self::calm_is_monotone`],
    /// [`Self::data_is_public`], [`Self::direction_prefers_lower`]) on
    /// the ephemeral surface's (resolver-hop × derived-nullary-bool)
    /// shape — the FIFTEENTH peer overall and the SECOND peer
    /// threading the classification-`horizon.direction` axis on this
    /// surface. CLOSES the SIXTH classification axis into a binary XOR
    /// partition on the ephemeral surface after the horizon, calm,
    /// data, point, and substrate axes — completing the axis-coverage
    /// milestone on this surface: ALL SIX classification axes now
    /// have their partitions closed at the ephemeral-surface derived-
    /// nullary corner. The resolver-hop shape is byte-identical across
    /// all fifteen peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `direction_prefers_higher()` returns `true` iff
    /// `self.resolved_classification().direction_prefers_higher()`.
    /// The resolver returns the authored [`Classification`] when
    /// present and the substrate default
    /// [`Classification::gate_compute`] on absence. Because
    /// [`Classification::gate_compute`] carries `horizon:
    /// Horizon::default()` whose `direction` field is `None`, and
    /// [`crate::classification::OptimizationDirection::default =
    /// Minimize`] projects `prefers_higher = false`, a bare ephemeral
    /// spec with no `:classification` slot answers `false` — every
    /// unadorned `(defephemeral …)` reads as lower-is-better under the
    /// substrate polarity default (safe under the asymptotic-health
    /// rate-window evaluator's convention: an operator must
    /// deliberately opt into Maximize polarity rather than the
    /// substrate silently flipping every unadorned Process onto the
    /// higher-is-better path). A regression that dropped the resolver
    /// hop, probed the wrong closed-set arm, or inverted the
    /// projection fails HERE at ONE narrow substrate site before
    /// drifting through every unadorned ephemeral spec's rate-window
    /// evaluator polarity.
    ///
    /// # Compounding — CLOSES the optimization-direction axis on the ephemeral surface
    ///
    /// The ephemeral require-tag classifier composes this primitive as
    /// a fixed tag `prefers-higher-direction` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `prefers-higher-direction` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::direction_prefers_higher`] directly. The
    /// two-surface parity contract holds by construction: both
    /// surfaces route through the SAME
    /// [`Classification::direction_prefers_higher`] primitive after
    /// the ephemeral surface pays ONE resolver hop. SECOND
    /// optimization-direction-axis peer CLOSES the axis into the FULL
    /// binary XOR partition contract on this surface — the resolver-
    /// hop peer of the parent-composed
    /// `classification_direction_probes_form_binary_xor_partition_over_all`,
    /// mirror of the calm-axis (`monotone-calm ⊕ coordination-required`)
    /// and data-axis (`public-data ⊕ data-restricted`) closures on
    /// this surface.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`horizon.direction`-axis
    /// derived-nullary-boolean probe body composes ONE resolver
    /// primitive ([`Self::resolved_classification`]) with ONE
    /// [`Classification`] primitive
    /// ([`Classification::direction_prefers_higher`]) so every
    /// downstream (the `prefers-higher-direction` fixed tags on both
    /// surfaces in tatara-check, future asymptotic-health rate-window
    /// / regression-detector evaluators, future variant additions on
    /// [`crate::classification::OptimizationDirection`]) binds through
    /// the SAME `direction_prefers_higher()` shape rather than
    /// restating either the resolver walk or the closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation over
    /// composition; a future
    /// [`crate::classification::OptimizationDirection`] variant lands
    /// at ONE `ALL` entry + ONE `prefers_higher` arm on the closed set
    /// and both surfaces pick it up mechanically.
    #[must_use]
    pub fn direction_prefers_higher(&self) -> bool {
        self.resolved_classification().direction_prefers_higher()
    }

    /// Derived-boolean predicate — does this ephemeral spec's resolved
    /// [`Classification`]'s `point_type` slot project to `Arity::One`
    /// under
    /// [`crate::classification::ConvergencePointType::input_arity`]?
    /// Byte-for-byte peer of
    /// [`crate::classification::Classification::input_arity_is_one`]
    /// wrapped through the [`Self::resolved_classification`] resolver
    /// so an operator-omitted `:classification` slot on
    /// `(defephemeral …)` still answers via the substrate default. The
    /// ONE ephemeral-surface substrate primitive that owns the
    /// `(&EphemeralSpec) -> bool` derived-nullary-boolean walk on the
    /// single-input side of the DAG-composition input-arity projection.
    ///
    /// # Sixteenth derived-nullary-boolean peer on the ephemeral surface — opens the input-arity axis
    ///
    /// Peer of the fifteen prior nullary-boolean substrate primitives
    /// on [`EphemeralSpec`] ([`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// [`Self::substrate_is_telemetry`], [`Self::calm_is_monotone`],
    /// [`Self::data_is_public`], [`Self::direction_prefers_lower`],
    /// [`Self::direction_prefers_higher`]) on the ephemeral surface's
    /// (resolver-hop × derived-nullary-bool) shape — the SIXTEENTH
    /// peer overall and the FIRST peer threading the classification-
    /// `point_type`-derived input-arity axis on this surface. Opens
    /// the SEVENTH classification axis into the ephemeral fixed-tag
    /// algebra after the horizon, calm, data, point-type, substrate,
    /// and optimization-direction axes. First peer on the derived-
    /// typed-projection stratum of the ephemeral surface — composes
    /// an extra closed-set-level projection hop
    /// ([`crate::classification::ConvergencePointType::input_arity`])
    /// compared to the sibling `point_is_*` triple that walks the raw
    /// `point_type` slot through the resolver. The resolver-hop shape
    /// is byte-identical across all sixteen peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `input_arity_is_one()` returns `true` iff
    /// `self.resolved_classification().input_arity_is_one()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// `point_type: Gate` and `Gate.input_arity() = Many`, a bare
    /// ephemeral spec with no `:classification` slot answers `false` —
    /// every unadorned `(defephemeral …)` lands in the multi-input
    /// bucket under the substrate default (`Gate` gates a
    /// many-to-one bucket dispatch, so the single-input bucket only
    /// applies to operator-authored specs on the `Transform | Fork |
    /// Broadcast | Observe` arms). A regression that dropped the
    /// resolver hop, probed the wrong closed-set arm, or crossed the
    /// wires with the sibling
    /// [`crate::classification::ConvergencePointType::output_arity`]
    /// projection (which disagrees on six of the eight variants) fails
    /// HERE at ONE narrow substrate site before drifting through
    /// every unadorned ephemeral spec's DAG-composition input-arity
    /// audit.
    ///
    /// # Compounding — opens the input-arity axis on the ephemeral surface
    ///
    /// The ephemeral require-tag classifier will compose this
    /// primitive as a fixed tag `single-input-arity` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `single-input-arity` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::input_arity_is_one`] directly. The
    /// two-surface parity contract holds by construction: both
    /// surfaces route through the SAME
    /// [`Classification::input_arity_is_one`] primitive after the
    /// ephemeral surface pays ONE resolver hop. A future antisymmetric
    /// peer ([`Self::input_arity_is_many`]) closes the binary XOR
    /// partition on this axis — mirror of the calm-axis
    /// (`monotone-calm ⊕ coordination-required`), data-axis
    /// (`public-data ⊕ data-restricted`), and optimization-direction-
    /// axis (`prefers-lower-direction ⊕ prefers-higher-direction`)
    /// closures on this surface.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`point_type`-derived
    /// input-arity-axis derived-nullary-boolean probe body composes
    /// ONE resolver primitive ([`Self::resolved_classification`])
    /// with ONE [`Classification`] primitive
    /// ([`Classification::input_arity_is_one`]) so every downstream
    /// (the future `single-input-arity` fixed tag on the ephemeral
    /// surface in tatara-check, future DAG-composition input-arity
    /// validators keying on the single-input framing, future variant
    /// additions on
    /// [`crate::classification::ConvergencePointType`]) binds through
    /// the SAME `input_arity_is_one()` shape rather than restating
    /// either the resolver walk or the two-hop closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation over
    /// composition; a future
    /// [`crate::classification::ConvergencePointType`] variant lands
    /// at ONE `ALL` entry + ONE `input_arity` arm on the closed set
    /// and both surfaces pick it up mechanically.
    #[must_use]
    pub fn input_arity_is_one(&self) -> bool {
        self.resolved_classification().input_arity_is_one()
    }

    /// ANTISYMMETRIC PEER of [`Self::input_arity_is_one`] — does
    /// this ephemeral spec's resolved [`Classification`]'s `point_type`
    /// slot project to `Arity::Many` under
    /// [`crate::classification::ConvergencePointType::input_arity`]?
    /// Byte-for-byte peer of
    /// [`crate::classification::Classification::input_arity_is_many`]
    /// wrapped through the [`Self::resolved_classification`] resolver
    /// so an operator-omitted `:classification` slot on
    /// `(defephemeral …)` still answers via the substrate default. The
    /// ONE ephemeral-surface substrate primitive that owns the
    /// `(&EphemeralSpec) -> bool` derived-nullary-boolean walk on the
    /// multi-input side of the DAG-composition input-arity projection.
    ///
    /// # Seventeenth derived-nullary-boolean peer on the ephemeral surface — CLOSES the input-arity axis
    ///
    /// Peer of the sixteen prior nullary-boolean substrate primitives
    /// on [`EphemeralSpec`] ([`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// [`Self::substrate_is_telemetry`], [`Self::calm_is_monotone`],
    /// [`Self::data_is_public`], [`Self::direction_prefers_lower`],
    /// [`Self::direction_prefers_higher`], [`Self::input_arity_is_one`])
    /// on the ephemeral surface's (resolver-hop × derived-nullary-
    /// bool) shape — the SEVENTEENTH peer overall and the SECOND peer
    /// threading the classification-`point_type`-derived input-arity
    /// axis on this surface. CLOSES the SEVENTH classification axis
    /// into the FULL binary XOR partition contract
    /// `input_arity_is_one ⊕ input_arity_is_many` on the ephemeral
    /// surface — the resolver-hop peer of the parent-composed
    /// `classification_input_arity_probes_form_binary_xor_partition_over_all`.
    /// The resolver-hop shape is byte-identical across all seventeen
    /// peers.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `input_arity_is_many()` returns `true` iff
    /// `self.resolved_classification().input_arity_is_many()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// `point_type: Gate` and `Gate.input_arity() = Many`, a bare
    /// ephemeral spec with no `:classification` slot answers `true` —
    /// every unadorned `(defephemeral …)` lands in the multi-input
    /// bucket under the substrate default. Direct antisymmetric
    /// mirror of [`Self::input_arity_is_one`] on the SAME resolver
    /// walk + SAME projection through the SAME closed set.
    ///
    /// # Compounding — CLOSES the input-arity axis on the ephemeral surface
    ///
    /// The ephemeral require-tag classifier will compose this
    /// primitive as a fixed tag `multi-input-arity` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `multi-input-arity` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::input_arity_is_many`] directly. The
    /// two-surface parity contract holds by construction: both
    /// surfaces route through the SAME
    /// [`Classification::input_arity_is_many`] primitive after the
    /// ephemeral surface pays ONE resolver hop. SECOND input-arity-
    /// axis peer CLOSES the axis into the FULL binary XOR partition
    /// contract on this surface — the resolver-hop peer of the
    /// parent-composed
    /// `classification_input_arity_probes_form_binary_xor_partition_over_all`,
    /// mirror of the calm-axis (`monotone-calm ⊕
    /// coordination-required`), data-axis (`public-data ⊕
    /// data-restricted`), and optimization-direction-axis
    /// (`prefers-lower-direction ⊕ prefers-higher-direction`)
    /// closures on this surface — the SEVENTH classification axis to
    /// reach the closed XOR partition landmark on the ephemeral
    /// resolver-hop surface, opening the derived-typed-projection
    /// stratum on this surface for the first time.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`point_type`-derived
    /// input-arity-axis derived-nullary-boolean probe body composes
    /// ONE resolver primitive ([`Self::resolved_classification`])
    /// with ONE [`Classification`] primitive
    /// ([`Classification::input_arity_is_many`]) so every downstream
    /// (the future `multi-input-arity` fixed tag on the ephemeral
    /// surface in tatara-check, future DAG-composition input-arity
    /// validators keying on the multi-input framing, future variant
    /// additions on
    /// [`crate::classification::ConvergencePointType`]) binds through
    /// the SAME `input_arity_is_many()` shape rather than restating
    /// either `!self.input_arity_is_one()` or the two-hop
    /// `self.resolved_classification().point_type.input_arity().is_many()`
    /// chain at each callsite. THEORY.md §VI.1 — generation over
    /// composition; a future
    /// [`crate::classification::ConvergencePointType`] variant lands
    /// at ONE `ALL` entry + ONE `input_arity` arm on the closed set
    /// and both surfaces pick it up mechanically.
    #[must_use]
    pub fn input_arity_is_many(&self) -> bool {
        self.resolved_classification().input_arity_is_many()
    }

    /// Derived-boolean predicate — does this ephemeral spec's resolved
    /// [`Classification`]'s `point_type` slot project to `Arity::One`
    /// under
    /// [`crate::classification::ConvergencePointType::output_arity`]?
    /// Byte-for-byte peer of
    /// [`crate::classification::Classification::output_arity_is_one`]
    /// wrapped through the [`Self::resolved_classification`] resolver
    /// so an operator-omitted `:classification` slot on
    /// `(defephemeral …)` still answers via the substrate default. The
    /// ONE ephemeral-surface substrate primitive that owns the
    /// `(&EphemeralSpec) -> bool` derived-nullary-boolean walk on the
    /// single-output side of the DAG-composition output-arity projection.
    ///
    /// # Eighteenth derived-nullary-boolean peer on the ephemeral surface — opens the output-arity axis
    ///
    /// Peer of the seventeen prior nullary-boolean substrate primitives
    /// on [`EphemeralSpec`] ([`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// [`Self::substrate_is_telemetry`], [`Self::calm_is_monotone`],
    /// [`Self::data_is_public`], [`Self::direction_prefers_lower`],
    /// [`Self::direction_prefers_higher`], [`Self::input_arity_is_one`],
    /// [`Self::input_arity_is_many`]) on the ephemeral surface's
    /// (resolver-hop × derived-nullary-bool) shape — the EIGHTEENTH
    /// peer overall and the FIRST peer threading the classification-
    /// `point_type`-derived OUTPUT-arity axis on this surface. Opens
    /// the EIGHTH classification axis into the ephemeral fixed-tag
    /// algebra after the horizon, calm, data, point-type, substrate,
    /// optimization-direction, and input-arity axes. SECOND peer on
    /// the derived-typed-projection stratum of the ephemeral surface
    /// (after [`Self::input_arity_is_one`]) — composes an extra
    /// closed-set-level projection hop
    /// ([`crate::classification::ConvergencePointType::output_arity`])
    /// compared to the sibling `point_is_*` triple that walks the raw
    /// `point_type` slot through the resolver. The resolver-hop shape
    /// is byte-identical across all eighteen peers.
    ///
    /// # Distinctness from the input-arity axis
    ///
    /// The input-arity and output-arity axes carve the eight-variant
    /// [`crate::classification::ConvergencePointType`] closed set into
    /// DISTINCT partitions — six of the eight variants (`Fork |
    /// Broadcast | Join | Gate | Select | Reduce`) DISAGREE between the
    /// two projections, and only the two endomorphic variants
    /// (`Transform | Observe` — both `(One, One)`) agree. The ephemeral
    /// resolver-hop surface inherits this distinctness verbatim: the
    /// absent-classification baseline (`gate_compute` → `point_type:
    /// Gate`) FLIPS between the two axes — `input_arity_is_one` is
    /// `false` on the baseline but `output_arity_is_one` is `true`.
    /// So `output_arity_is_one` is NOT a redundant restatement of
    /// `input_arity_is_one` even after both wrap through the SAME
    /// resolver.
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `output_arity_is_one()` returns `true` iff
    /// `self.resolved_classification().output_arity_is_one()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// `point_type: Gate` and `Gate.output_arity() = One`, a bare
    /// ephemeral spec with no `:classification` slot answers `true` —
    /// every unadorned `(defephemeral …)` lands in the single-output
    /// bucket under the substrate default (`Gate` gates a many-to-one
    /// bucket dispatch, so the multi-output bucket only applies to
    /// operator-authored specs on the `Fork | Broadcast` arms). A
    /// regression that dropped the resolver hop, probed the wrong
    /// closed-set arm, or crossed the wires with the sibling
    /// [`crate::classification::ConvergencePointType::input_arity`]
    /// projection (which disagrees on six of the eight variants) fails
    /// HERE at ONE narrow substrate site before drifting through every
    /// unadorned ephemeral spec's DAG-composition output-arity audit.
    ///
    /// # Compounding — opens the output-arity axis on the ephemeral surface
    ///
    /// The ephemeral require-tag classifier will compose this
    /// primitive as a fixed tag `single-output-arity` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `single-output-arity` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::output_arity_is_one`] directly. The
    /// two-surface parity contract holds by construction: both
    /// surfaces route through the SAME
    /// [`Classification::output_arity_is_one`] primitive after the
    /// ephemeral surface pays ONE resolver hop. A future antisymmetric
    /// peer ([`Self::output_arity_is_many`]) closes the binary XOR
    /// partition on this axis — mirror of the input-arity-axis
    /// (`input_arity_is_one ⊕ input_arity_is_many`), the calm-axis
    /// (`monotone-calm ⊕ coordination-required`), the data-axis
    /// (`public-data ⊕ data-restricted`), and the optimization-
    /// direction-axis (`prefers-lower-direction ⊕
    /// prefers-higher-direction`) closures on this surface,
    /// completing the DAG-composition arity PAIR on the ephemeral
    /// derived-typed-projection stratum.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`point_type`-derived
    /// output-arity-axis derived-nullary-boolean probe body composes
    /// ONE resolver primitive ([`Self::resolved_classification`])
    /// with ONE [`Classification`] primitive
    /// ([`Classification::output_arity_is_one`]) so every downstream
    /// (the future `single-output-arity` fixed tag on the ephemeral
    /// surface in tatara-check, future DAG-composition output-arity
    /// validators keying on the single-output framing, future variant
    /// additions on
    /// [`crate::classification::ConvergencePointType`]) binds through
    /// the SAME `output_arity_is_one()` shape rather than restating
    /// either the resolver walk or the two-hop closed-set projection
    /// composition at the callsite. THEORY.md §VI.1 — generation over
    /// composition; a future
    /// [`crate::classification::ConvergencePointType`] variant lands
    /// at ONE `ALL` entry + ONE `output_arity` arm on the closed set
    /// and both surfaces pick it up mechanically.
    #[must_use]
    pub fn output_arity_is_one(&self) -> bool {
        self.resolved_classification().output_arity_is_one()
    }

    /// ANTISYMMETRIC PEER of [`Self::output_arity_is_one`] — does
    /// this ephemeral spec's resolved [`Classification`]'s `point_type`
    /// slot project to `Arity::Many` under
    /// [`crate::classification::ConvergencePointType::output_arity`]?
    /// Byte-for-byte peer of
    /// [`crate::classification::Classification::output_arity_is_many`]
    /// wrapped through the [`Self::resolved_classification`] resolver
    /// so an operator-omitted `:classification` slot on
    /// `(defephemeral …)` still answers via the substrate default. The
    /// ONE ephemeral-surface substrate primitive that owns the
    /// `(&EphemeralSpec) -> bool` derived-nullary-boolean walk on the
    /// multi-output side of the DAG-composition output-arity projection.
    ///
    /// # Nineteenth derived-nullary-boolean peer on the ephemeral surface — CLOSES the output-arity axis
    ///
    /// Peer of the eighteen prior nullary-boolean substrate primitives
    /// on [`EphemeralSpec`] ([`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// [`Self::substrate_is_telemetry`], [`Self::calm_is_monotone`],
    /// [`Self::data_is_public`], [`Self::direction_prefers_lower`],
    /// [`Self::direction_prefers_higher`], [`Self::input_arity_is_one`],
    /// [`Self::input_arity_is_many`], [`Self::output_arity_is_one`])
    /// on the ephemeral surface's (resolver-hop × derived-nullary-
    /// bool) shape — the NINETEENTH peer overall and the SECOND peer
    /// threading the classification-`point_type`-derived OUTPUT-arity
    /// axis on this surface. CLOSES the EIGHTH classification axis
    /// into the FULL binary XOR partition contract
    /// `output_arity_is_one ⊕ output_arity_is_many` on the ephemeral
    /// surface — the resolver-hop peer of the parent-composed
    /// `classification_output_arity_probes_form_binary_xor_partition_over_all`.
    /// The resolver-hop shape is byte-identical across all nineteen
    /// peers. Completes the DAG-composition arity PAIR on the
    /// ephemeral derived-typed-projection stratum
    /// (`input_arity_is_{one,many}` + `output_arity_is_{one,many}` on
    /// the SAME resolver walk through the SAME closed set).
    ///
    /// # Semantics — resolver hop + derived-nullary-boolean
    ///
    /// `output_arity_is_many()` returns `true` iff
    /// `self.resolved_classification().output_arity_is_many()`. The
    /// resolver returns the authored [`Classification`] when present
    /// and the substrate default [`Classification::gate_compute`] on
    /// absence. Because [`Classification::gate_compute`] carries
    /// `point_type: Gate` and `Gate.output_arity() = One`, a bare
    /// ephemeral spec with no `:classification` slot answers `false` —
    /// every unadorned `(defephemeral …)` lands in the single-output
    /// bucket under the substrate default. Direct antisymmetric
    /// mirror of [`Self::output_arity_is_one`] on the SAME resolver
    /// walk + SAME projection through the SAME closed set.
    ///
    /// # Compounding — CLOSES the output-arity axis on the ephemeral surface
    ///
    /// The ephemeral require-tag classifier will compose this
    /// primitive as a fixed tag `multi-output-arity` on
    /// `EPHEMERAL_FIXED_TAG_ARMS` — byte-for-byte peer of the point
    /// surface's `multi-output-arity` fixed tag on
    /// `POINT_FIXED_TAG_ARMS` via
    /// [`Classification::output_arity_is_many`] directly. The
    /// two-surface parity contract holds by construction: both
    /// surfaces route through the SAME
    /// [`Classification::output_arity_is_many`] primitive after the
    /// ephemeral surface pays ONE resolver hop. SECOND output-arity-
    /// axis peer CLOSES the axis into the FULL binary XOR partition
    /// contract on this surface — the resolver-hop peer of the
    /// parent-composed
    /// `classification_output_arity_probes_form_binary_xor_partition_over_all`,
    /// mirror of the input-arity-axis (`input_arity_is_one ⊕
    /// input_arity_is_many`), the calm-axis (`monotone-calm ⊕
    /// coordination-required`), the data-axis (`public-data ⊕
    /// data-restricted`), and the optimization-direction-axis
    /// (`prefers-lower-direction ⊕ prefers-higher-direction`)
    /// closures on this surface — the EIGHTH classification axis to
    /// reach the closed XOR partition landmark on the ephemeral
    /// resolver-hop surface, completing the DAG-composition arity
    /// PAIR on the derived-typed-projection stratum of this surface.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the classification-`point_type`-derived
    /// output-arity-axis derived-nullary-boolean probe body composes
    /// ONE resolver primitive ([`Self::resolved_classification`])
    /// with ONE [`Classification`] primitive
    /// ([`Classification::output_arity_is_many`]) so every downstream
    /// (the future `multi-output-arity` fixed tag on the ephemeral
    /// surface in tatara-check, future DAG-composition output-arity
    /// validators keying on the multi-output framing, future variant
    /// additions on
    /// [`crate::classification::ConvergencePointType`]) binds through
    /// the SAME `output_arity_is_many()` shape rather than restating
    /// either `!self.output_arity_is_one()` or the two-hop
    /// `self.resolved_classification().point_type.output_arity().is_many()`
    /// chain at each callsite. THEORY.md §VI.1 — generation over
    /// composition; a future
    /// [`crate::classification::ConvergencePointType`] variant lands
    /// at ONE `ALL` entry + ONE `output_arity` arm on the closed set
    /// and both surfaces pick it up mechanically.
    #[must_use]
    pub fn output_arity_is_many(&self) -> bool {
        self.resolved_classification().output_arity_is_many()
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
    use crate::boundary::{assert_slice_refinement_composition_laws, ConditionKind};
    use crate::classification::{
        Arity, CalmClassification, ConvergencePointType, DataClassification, Horizon, HorizonKind,
        OptimizationDirection, SubstrateType,
    };
    use crate::intent::IntentVariant;
    use crate::lifetime::LifetimeVariant;

    /// LANDMARK PIN — the (ephemeral-surface test-fixture ×
    /// [`Classification::gate_compute_with_axis`] on horizon-nested
    /// axes) sweep equivalence. Nine ephemeral-surface probe-sweep
    /// tests in this module (`has_horizon_kind_*`,
    /// `has_optimization_direction_*`, `horizon_terminates_*`,
    /// `horizon_requires_metric_axes_*`, `horizon_terminates_xor_*`)
    /// pre-sweep restated the SAME `let mut c =
    /// Classification::gate_compute(); c.horizon = Horizon { <slot>:
    /// populated, ..Horizon::default() }` five-line fixture at each
    /// callsite, mutating exactly ONE horizon-nested slot to
    /// `populated`; post-sweep each callsite reads
    /// [`Classification::gate_compute_with_axis(populated)`] — one
    /// line — and the four-baseline-slot restatement lives at ONE
    /// substrate primitive. This pin asserts byte-parity between the
    /// pre-sweep hand-authored `Horizon` struct-literal shape (both
    /// the [`HorizonKind::kind`] mutation shape AND the
    /// [`OptimizationDirection`]-into-`Some(_)` mutation shape) and
    /// the post-sweep composer output on every variant of each closed
    /// set, so a regression that either (a) changed
    /// [`ClassificationAxis for HorizonKind`] to stomp a non-`kind`
    /// sub-slot, (b) changed [`ClassificationAxis for OptimizationDirection`]
    /// to drop the `Some(...)` wrap, or (c) reintroduced a whole-
    /// `Horizon`-reset shape that dropped a sibling sub-slot would
    /// fail HERE at ONE landmark site before landing at the peer
    /// probe-sweep pins that use the composer.
    ///
    /// Byte-for-byte peer of the sibling landmark
    /// `with_axis_optimization_direction_overlay_wraps_variant_in_some`
    /// on the point-surface classification-module tests — this pin
    /// carries the same substrate contract through to the ephemeral-
    /// surface tests that consume the composer.
    #[test]
    fn gate_compute_with_axis_on_horizon_nested_axes_matches_hand_authored_shape() {
        for kind in HorizonKind::ALL {
            let via_composer = Classification::gate_compute_with_axis(kind);
            let mut via_hand_authored = Classification::gate_compute();
            via_hand_authored.horizon = Horizon {
                kind,
                ..Horizon::default()
            };
            assert_eq!(
                via_composer, via_hand_authored,
                "HorizonKind::{kind:?}: composer vs pre-sweep hand-authored struct-literal drift",
            );
        }
        for direction in OptimizationDirection::ALL {
            let via_composer = Classification::gate_compute_with_axis(direction);
            let mut via_hand_authored = Classification::gate_compute();
            via_hand_authored.horizon = Horizon {
                direction: Some(direction),
                ..Horizon::default()
            };
            assert_eq!(
                via_composer, via_hand_authored,
                "OptimizationDirection::{direction:?}: composer vs pre-sweep hand-authored struct-literal drift",
            );
        }
    }

    /// Primitive-owner pin — `EphemeralSpec::with_classification_axis`
    /// on a `classification: None` carrier produces an ephemeral spec
    /// whose `classification` slot is
    /// `Some(Classification::gate_compute_with_axis(axis))` byte-for-
    /// byte on every axis-variant, and preserves every non-
    /// classification slot at its pre-call value. A regression that
    /// (a) failed to wrap the composed [`Classification`] in `Some(_)`
    /// on the `None`-arm, (b) mutated a sibling slot on `EphemeralSpec`
    /// through the axis overlay, or (c) picked a different `None`-arm
    /// fill-through than the sibling
    /// [`Self::resolved_classification`] resolver would fail HERE.
    #[test]
    fn with_classification_axis_on_none_arm_fills_through_gate_compute() {
        fn baseline() -> EphemeralSpec {
            EphemeralSpec {
                aplicacao: demo_overlay(),
                ttl: "2h".into(),
                teardown: TeardownPolicy::OnAttested,
                max_concurrent: 3,
                postconditions: vec![],
                preconditions: vec![],
                verify_timeout: Some("30m".into()),
                classification: None,
                parent: Some("seph.1".into()),
                exports: vec![],
                routing: None,
            }
        }
        // Direct-scalar axes: composer output matches
        // `Classification::gate_compute_with_axis(axis)` byte-for-byte,
        // wrapped in `Some(_)`.
        for kind in ConvergencePointType::ALL {
            let via_composer = baseline().with_classification_axis(kind);
            assert_eq!(
                via_composer.classification,
                Some(Classification::gate_compute_with_axis(kind)),
                "ConvergencePointType::{kind:?}: composer vs gate_compute_with_axis Some(_) drift on None-arm",
            );
        }
        for kind in SubstrateType::ALL {
            let via_composer = baseline().with_classification_axis(kind);
            assert_eq!(
                via_composer.classification,
                Some(Classification::gate_compute_with_axis(kind)),
                "SubstrateType::{kind:?}: composer vs gate_compute_with_axis Some(_) drift on None-arm",
            );
        }
        for kind in CalmClassification::ALL {
            let via_composer = baseline().with_classification_axis(kind);
            assert_eq!(
                via_composer.classification,
                Some(Classification::gate_compute_with_axis(kind)),
                "CalmClassification::{kind:?}: composer vs gate_compute_with_axis Some(_) drift on None-arm",
            );
        }
        for kind in DataClassification::ALL {
            let via_composer = baseline().with_classification_axis(kind);
            assert_eq!(
                via_composer.classification,
                Some(Classification::gate_compute_with_axis(kind)),
                "DataClassification::{kind:?}: composer vs gate_compute_with_axis Some(_) drift on None-arm",
            );
        }
        // Horizon-nested axes: same shape through the trait's
        // sub-slot overlay.
        for kind in HorizonKind::ALL {
            let via_composer = baseline().with_classification_axis(kind);
            assert_eq!(
                via_composer.classification,
                Some(Classification::gate_compute_with_axis(kind)),
                "HorizonKind::{kind:?}: composer vs gate_compute_with_axis Some(_) drift on None-arm",
            );
        }
        for direction in OptimizationDirection::ALL {
            let via_composer = baseline().with_classification_axis(direction);
            assert_eq!(
                via_composer.classification,
                Some(Classification::gate_compute_with_axis(direction)),
                "OptimizationDirection::{direction:?}: composer vs gate_compute_with_axis Some(_) drift on None-arm",
            );
        }
        // Non-classification slots: every one preserved byte-for-byte
        // across the overlay on every axis. Compare through JSON
        // round-trip since `AplicacaoIntent` / `ExportSpec` /
        // `RoutingSpec` do not carry `PartialEq`.
        for kind in ConvergencePointType::ALL {
            let via_composer = baseline().with_classification_axis(kind);
            let baseline_ref = baseline();
            assert_eq!(
                serde_json::to_string(&via_composer.aplicacao).unwrap(),
                serde_json::to_string(&baseline_ref.aplicacao).unwrap(),
                "aplicacao slot drifted under axis overlay for kind={kind:?}",
            );
            assert_eq!(via_composer.ttl, baseline_ref.ttl);
            assert_eq!(via_composer.teardown, baseline_ref.teardown);
            assert_eq!(via_composer.max_concurrent, baseline_ref.max_concurrent);
            assert_eq!(
                via_composer.postconditions.len(),
                baseline_ref.postconditions.len()
            );
            assert_eq!(
                via_composer.preconditions.len(),
                baseline_ref.preconditions.len()
            );
            assert_eq!(via_composer.verify_timeout, baseline_ref.verify_timeout);
            assert_eq!(via_composer.parent, baseline_ref.parent);
            assert_eq!(via_composer.exports.len(), baseline_ref.exports.len());
            assert!(via_composer.routing.is_none());
        }
    }

    /// Primitive-owner pin —
    /// `EphemeralSpec::with_classification_axis` on a
    /// `classification: Some(prior)` carrier composes the axis
    /// overlay onto `prior` via [`ClassificationAxis::overlay`],
    /// preserving every OTHER axis slot on `prior`. Distinct from the
    /// `None`-arm pin above: the `Some(prior)` arm does NOT reset
    /// through [`Classification::gate_compute`], and consecutive
    /// `.with_classification_axis(...)` calls compose arbitrary
    /// N-axis conjunctions on the ephemeral surface with the same
    /// order-independence guarantee [`Classification::with_axis`]
    /// carries on distinct-slot axes.
    #[test]
    fn with_classification_axis_on_some_arm_chains_onto_prior() {
        fn baseline() -> EphemeralSpec {
            EphemeralSpec {
                aplicacao: demo_overlay(),
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
        // Prior authored point_type = Fork; overlay substrate = Storage
        // preserves the Fork point_type on the composed classification.
        let seeded = baseline().with_classification_axis(ConvergencePointType::Fork);
        let composed = seeded.with_classification_axis(SubstrateType::Storage);
        let classification = composed
            .classification
            .as_ref()
            .expect("with_classification_axis populates Some(_)");
        assert_eq!(classification.point_type, ConvergencePointType::Fork);
        assert_eq!(classification.substrate, SubstrateType::Storage);
        // Order independence on distinct-slot axes: swapping the axis
        // chain reads the SAME final classification.
        let forward = baseline()
            .with_classification_axis(ConvergencePointType::Fork)
            .with_classification_axis(SubstrateType::Storage)
            .with_classification_axis(CalmClassification::NonMonotone)
            .with_classification_axis(DataClassification::Pii)
            .classification
            .unwrap();
        let reverse = baseline()
            .with_classification_axis(DataClassification::Pii)
            .with_classification_axis(CalmClassification::NonMonotone)
            .with_classification_axis(SubstrateType::Storage)
            .with_classification_axis(ConvergencePointType::Fork)
            .classification
            .unwrap();
        assert_eq!(
            forward, reverse,
            "with_classification_axis chain must be order-independent on distinct-slot axes",
        );
        // Nested horizon-sub-slot overlays compose onto the same
        // carrier without stomping each other: the (kind, direction)
        // pair rides both chains.
        let paired = baseline()
            .with_classification_axis(HorizonKind::Asymptotic)
            .with_classification_axis(OptimizationDirection::Maximize)
            .classification
            .unwrap();
        assert_eq!(paired.horizon.kind, HorizonKind::Asymptotic);
        assert_eq!(
            paired.horizon.direction,
            Some(OptimizationDirection::Maximize)
        );
    }

    /// Primitive-owner pin —
    /// `EphemeralSpec::with_classification_axis` composes byte-for-
    /// byte with the pre-sweep hand-authored two-shape callsite
    /// pattern that recurred at ~36 sites in
    /// `tatara-reconciler::bin::tatara-check`: either
    /// `let mut c = Classification::gate_compute(); c.<axis> =
    /// populated; EphemeralSpec { classification: Some(c), ..
    /// baseline }`, or the newer `let c =
    /// Classification::gate_compute_with_axis(populated); EphemeralSpec
    /// { classification: Some(c), ..baseline }`. Both restated
    /// pre-sweep shapes classify identically to
    /// `baseline.with_classification_axis(populated)` on every
    /// [`ClassificationAxis`] impl. A regression that drifted the
    /// composer body away from the pre-sweep shape (a stray reset of a
    /// non-classification slot, a stomping of a nested horizon sub-
    /// slot on the direct-scalar axes) fails HERE at ONE landmark site
    /// before drifting through the ~36 swept callsites in tatara-
    /// check.rs.
    #[test]
    fn with_classification_axis_matches_pre_sweep_hand_authored_shape() {
        fn baseline() -> EphemeralSpec {
            EphemeralSpec {
                aplicacao: demo_overlay(),
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
        // Direct-scalar axes: `<eph>.with_classification_axis(kind)`
        // matches the pre-sweep two-shape callsite pattern on every
        // ConvergencePointType variant.
        for kind in ConvergencePointType::ALL {
            let via_composer = baseline().with_classification_axis(kind);
            let mut hand_classification = Classification::gate_compute();
            hand_classification.point_type = kind;
            let via_hand = EphemeralSpec {
                classification: Some(hand_classification),
                ..baseline()
            };
            assert_eq!(
                via_composer.classification, via_hand.classification,
                "ConvergencePointType::{kind:?}: composer vs pre-sweep hand-authored classification drift",
            );
        }
        for kind in SubstrateType::ALL {
            let via_composer = baseline().with_classification_axis(kind);
            let mut hand_classification = Classification::gate_compute();
            hand_classification.substrate = kind;
            let via_hand = EphemeralSpec {
                classification: Some(hand_classification),
                ..baseline()
            };
            assert_eq!(
                via_composer.classification, via_hand.classification,
                "SubstrateType::{kind:?}: composer vs pre-sweep hand-authored classification drift",
            );
        }
        for kind in CalmClassification::ALL {
            let via_composer = baseline().with_classification_axis(kind);
            let mut hand_classification = Classification::gate_compute();
            hand_classification.calm = kind;
            let via_hand = EphemeralSpec {
                classification: Some(hand_classification),
                ..baseline()
            };
            assert_eq!(
                via_composer.classification, via_hand.classification,
                "CalmClassification::{kind:?}: composer vs pre-sweep hand-authored classification drift",
            );
        }
        for kind in DataClassification::ALL {
            let via_composer = baseline().with_classification_axis(kind);
            let mut hand_classification = Classification::gate_compute();
            hand_classification.data_classification = kind;
            let via_hand = EphemeralSpec {
                classification: Some(hand_classification),
                ..baseline()
            };
            assert_eq!(
                via_composer.classification, via_hand.classification,
                "DataClassification::{kind:?}: composer vs pre-sweep hand-authored classification drift",
            );
        }
        // Horizon-nested axes: composer matches the newer
        // `gate_compute_with_axis` shape used on the horizon-nested
        // sweep sites in tatara-check.rs.
        for kind in HorizonKind::ALL {
            let via_composer = baseline().with_classification_axis(kind);
            let via_hand = EphemeralSpec {
                classification: Some(Classification::gate_compute_with_axis(kind)),
                ..baseline()
            };
            assert_eq!(
                via_composer.classification, via_hand.classification,
                "HorizonKind::{kind:?}: composer vs pre-sweep gate_compute_with_axis Some(_) drift",
            );
        }
        for direction in OptimizationDirection::ALL {
            let via_composer = baseline().with_classification_axis(direction);
            let via_hand = EphemeralSpec {
                classification: Some(Classification::gate_compute_with_axis(direction)),
                ..baseline()
            };
            assert_eq!(
                via_composer.classification, via_hand.classification,
                "OptimizationDirection::{direction:?}: composer vs pre-sweep gate_compute_with_axis Some(_) drift",
            );
        }
    }

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

    // ── EphemeralSpec::has_(pre|post)condition_kind substrate pins ──
    //
    // Fail-before-pass-after granularity: the two half-slice arms did
    // not exist on the ephemeral surface before this commit — the
    // ephemeral require-tag classifier in `tatara-check` and the
    // `closed-loop-auth` fixed-tag arm reached
    // `spec.postconditions.has_kind(K)` through direct field access,
    // asymmetric with the union-arm [`EphemeralSpec::has_condition_kind`]
    // that already routed through the named struct method. The lift
    // closes the (precondition, postcondition, union) triad on the
    // ephemeral sugar surface so a future normalization at the
    // presence-probe shape lands at ONE site per surface for all
    // three arms.

    /// EMPTY-SPEC pin — an ephemeral spec with no preconditions and
    /// no postconditions returns `false` for EVERY [`ConditionKind`]
    /// on both half-slice arms. Sweep `ConditionKind::ALL` so a new
    /// variant added without a matching arm surfaces at rustc's
    /// exhaustiveness gate on the ALL literal (arity forced by the
    /// closed-set array) rather than as a silent false-positive at
    /// every downstream require-tag callsite on the ephemeral
    /// surface.
    #[test]
    fn ephemeral_has_precondition_and_postcondition_kind_return_false_on_empty_spec() {
        let spec = empty_ephemeral();
        for kind in ConditionKind::ALL {
            assert!(
                !spec.has_precondition_kind(kind),
                "empty ephemeral must return false on precondition arm for {kind:?}",
            );
            assert!(
                !spec.has_postcondition_kind(kind),
                "empty ephemeral must return false on postcondition arm for {kind:?}",
            );
        }
    }

    /// SLICE-SELECTIVITY pin (precondition arm) — an ephemeral spec
    /// with a kind on the precondition side ONLY resolves `true` at
    /// [`EphemeralSpec::has_precondition_kind`] and `false` at
    /// [`EphemeralSpec::has_postcondition_kind`]. Locks the (side-
    /// select, kind-select) partition so a regression that pointed
    /// the precondition arm at `self.postconditions` (a copy-paste
    /// from the sibling arm during the lift) surfaces HERE.
    #[test]
    fn ephemeral_has_precondition_kind_reads_preconditions_slice_only() {
        for populated in ConditionKind::ALL {
            let mut spec = empty_ephemeral();
            spec.preconditions.push(cond(populated));
            for query in ConditionKind::ALL {
                let expected_pre = query == populated;
                assert_eq!(
                    spec.has_precondition_kind(query),
                    expected_pre,
                    "precondition-only populated={populated:?}: query {query:?} \
                     drifted on ephemeral precondition arm",
                );
                assert!(
                    !spec.has_postcondition_kind(query),
                    "precondition-only populated={populated:?}: query {query:?} must \
                     return false on ephemeral postcondition arm (postconditions is empty)",
                );
            }
        }
    }

    /// SLICE-SELECTIVITY pin (postcondition arm) — mirror of the
    /// precondition-only sweep on the other half. Locks the
    /// postcondition arm's binding to `self.postconditions` so a
    /// regression that pointed it at `self.preconditions` fails HERE
    /// even though the precondition-arm pin above passes.
    #[test]
    fn ephemeral_has_postcondition_kind_reads_postconditions_slice_only() {
        for populated in ConditionKind::ALL {
            let mut spec = empty_ephemeral();
            spec.postconditions.push(cond(populated));
            for query in ConditionKind::ALL {
                let expected_post = query == populated;
                assert_eq!(
                    spec.has_postcondition_kind(query),
                    expected_post,
                    "postcondition-only populated={populated:?}: query {query:?} \
                     drifted on ephemeral postcondition arm",
                );
                assert!(
                    !spec.has_precondition_kind(query),
                    "postcondition-only populated={populated:?}: query {query:?} must \
                     return false on ephemeral precondition arm (preconditions is empty)",
                );
            }
        }
    }

    /// COMPOSITION-LAW pin — [`EphemeralSpec::has_condition_kind`]
    /// equals `has_precondition_kind(k) || has_postcondition_kind(k)`
    /// at EVERY (pre-populated, post-populated, query) triple on
    /// `ConditionKind::ALL`. Byte-for-byte peer of the
    /// `boundary_has_condition_kind_composes_precondition_and_postcondition_arms`
    /// composition-law pin on the [`Boundary`] surface — the
    /// two-surface parity contract binds the ephemeral sugar type
    /// and the point-domain boundary type through the SAME
    /// (`condition_kind = precondition_kind ∨ postcondition_kind`)
    /// composition, so every downstream `condition-<K>` require-tag
    /// classifier on either surface inherits the composition
    /// mechanically.
    #[test]
    fn ephemeral_has_condition_kind_composes_precondition_and_postcondition_arms() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                for query in ConditionKind::ALL {
                    let via_arms =
                        spec.has_precondition_kind(query) || spec.has_postcondition_kind(query);
                    assert_eq!(
                        spec.has_condition_kind(query),
                        via_arms,
                        "ephemeral union arm drifted from OR of half-slice arms: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin — the two half-slice arms on the
    /// ephemeral surface delegate verbatim to
    /// [`crate::boundary::ConditionSliceExt::has_kind`] on the
    /// underlying [`Vec<Condition>`] slice, no inline reimplementation.
    /// Sweep the full `ConditionKind::ALL` × `ConditionKind::ALL`
    /// cross so a regression that inlined a divergent walk at either
    /// arm surfaces HERE at the substrate boundary rather than as
    /// silent skew between the struct-level arm and the slice-level
    /// primitive.
    #[test]
    fn ephemeral_has_precondition_and_postcondition_kind_delegate_to_slice_has_kind() {
        for populated in ConditionKind::ALL {
            let mut spec = empty_ephemeral();
            spec.preconditions.push(cond(populated));
            spec.postconditions.push(cond(populated));
            for query in ConditionKind::ALL {
                assert_eq!(
                    spec.has_precondition_kind(query),
                    spec.preconditions.has_kind(query),
                    "ephemeral precondition arm must delegate to preconditions.has_kind: \
                     populated={populated:?} query={query:?}",
                );
                assert_eq!(
                    spec.has_postcondition_kind(query),
                    spec.postconditions.has_kind(query),
                    "ephemeral postcondition arm must delegate to postconditions.has_kind: \
                     populated={populated:?} query={query:?}",
                );
            }
        }
    }

    // ── EphemeralSpec::find_(pre|post|)condition_kind widened triad ──
    //
    // Fail-before-pass-after granularity: the three widened
    // `find_*_kind` arms did not exist on the ephemeral surface before
    // this commit — the (widened `Option<&Condition>` return) axis
    // lived at ONE struct-level site (`Boundary::find_condition_kind`
    // on the point surface's nested [`Boundary`] slot). The lift adds
    // the peer inherent methods on the [`EphemeralSpec`] sugar-surface
    // so both struct-level widened callers compose against the SAME
    // slice-level substrate primitive
    // [`crate::boundary::ConditionSliceExt::find_kind`] in lockstep.
    // A regression that (a) hard-coded the arm to a single kind, (b)
    // reversed the walk order on the union (postcondition first), or
    // (c) collapsed `or_else` to `and_then` (silently narrowing the
    // union to an intersection) fails HERE at the substrate primitive
    // rather than as silent operator-facing drift at the ephemeral
    // require-tag surface.

    /// EMPTY-SPEC pin (find-triad) — a default [`EphemeralSpec`]
    /// (empty preconditions, empty postconditions) returns `None`
    /// from every widened arm for EVERY [`ConditionKind`]. Sweep
    /// `ConditionKind::ALL` × three-arm cross so a new variant added
    /// without a matching arm surfaces at rustc's exhaustiveness gate
    /// on the ALL literal (arity forced by the closed-set array)
    /// rather than as a silent false-`Some` at every downstream
    /// widened callsite on the ephemeral surface.
    #[test]
    fn ephemeral_find_condition_kind_triad_returns_none_on_empty_spec() {
        let spec = empty_ephemeral();
        for kind in ConditionKind::ALL {
            assert!(
                spec.find_precondition_kind(kind).is_none(),
                "empty ephemeral must return None on precondition find arm for {kind:?}",
            );
            assert!(
                spec.find_postcondition_kind(kind).is_none(),
                "empty ephemeral must return None on postcondition find arm for {kind:?}",
            );
            assert!(
                spec.find_condition_kind(kind).is_none(),
                "empty ephemeral must return None on union find arm for {kind:?}",
            );
        }
    }

    /// SUBSTRATE-DELEGATION pin (ephemeral find-triad) — the three
    /// widened `find_*_kind` methods on [`EphemeralSpec`] delegate
    /// verbatim to [`crate::boundary::ConditionSliceExt::find_kind`]
    /// on the underlying [`Vec<Condition>`] slices, no inline
    /// reimplementation. The `find_condition_kind` union walks
    /// preconditions first then postconditions via `Option::or_else`.
    /// Sweep `ConditionKind::ALL × ConditionKind::ALL × ConditionKind::ALL`
    /// so a regression that (a) inlined a divergent walk at either
    /// half-slice arm, (b) reversed the union walk order on the
    /// ephemeral surface only (breaking two-surface parity with
    /// [`crate::boundary::Boundary::find_condition_kind`]), or (c)
    /// collapsed `or_else` to `and_then` surfaces HERE at the substrate
    /// boundary. Byte-for-byte peer of the point-domain
    /// `find_condition_kind_triad_delegates_to_slice_find_kind` pin.
    #[test]
    fn ephemeral_find_condition_kind_triad_delegates_to_slice_find_kind() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                for query in ConditionKind::ALL {
                    let via_pre = spec.preconditions.find_kind(query);
                    let via_post = spec.postconditions.find_kind(query);
                    assert_eq!(
                        spec.find_precondition_kind(query).map(|c| c.kind),
                        via_pre.map(|c| c.kind),
                        "ephemeral precondition find arm must delegate: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        spec.find_postcondition_kind(query).map(|c| c.kind),
                        via_post.map(|c| c.kind),
                        "ephemeral postcondition find arm must delegate: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    let expected_union = via_pre.or(via_post).map(|c| c.kind);
                    assert_eq!(
                        spec.find_condition_kind(query).map(|c| c.kind),
                        expected_union,
                        "ephemeral union find arm must equal precondition.or_else(postcondition): \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// PRECONDITION-PRECEDENCE pin (ephemeral) — a kind authored on
    /// BOTH sides returns the precondition-side [`Condition`] from
    /// `find_condition_kind`. Byte-for-byte peer of the point-domain
    /// `find_condition_kind_returns_precondition_side_on_dual_populated`
    /// pin, so the two-surface parity contract binds the walk order
    /// on both surfaces through ONE composition law. Uses two params-
    /// distinguishable [`Condition`]s so a regression on the ephemeral
    /// surface only that reversed the walk order surfaces at the
    /// returned params payload rather than silently at the presence
    /// bit.
    #[test]
    fn ephemeral_find_condition_kind_returns_precondition_side_on_dual_populated() {
        let mut spec = empty_ephemeral();
        spec.preconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: serde_json::json!({ "side": "pre" }),
        });
        spec.postconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: serde_json::json!({ "side": "post" }),
        });
        let hit = spec
            .find_condition_kind(ConditionKind::ClosedLoopAuth)
            .expect("dual-populated ephemeral spec must resolve Some");
        assert_eq!(
            hit.params.get("side").and_then(serde_json::Value::as_str),
            Some("pre"),
            "ephemeral find_condition_kind must walk preconditions first",
        );
    }

    /// STRUCT-LEVEL DELEGATION pin (ephemeral has ↔ find) — the three
    /// [`EphemeralSpec`] `has_*_kind` arms equal their widened peers'
    /// `.is_some()` projection at EVERY (pre-populated, post-populated,
    /// query) triple on `ConditionKind::ALL`. Byte-for-byte peer of
    /// the point-domain
    /// `boundary_has_triad_equals_find_triad_is_some_projection` pin,
    /// so both surfaces' has/find refinement bridge stays symmetric by
    /// construction — a future consumer that reads
    /// `spec.has_condition_kind(k)` as sugar for
    /// `spec.find_condition_kind(k).is_some()` on either surface stays
    /// typed against the SAME truth table across the two-surface
    /// parity contract.
    #[test]
    fn ephemeral_has_triad_equals_find_triad_is_some_projection() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                for query in ConditionKind::ALL {
                    assert_eq!(
                        spec.has_precondition_kind(query),
                        spec.find_precondition_kind(query).is_some(),
                        "ephemeral precondition has/find bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        spec.has_postcondition_kind(query),
                        spec.find_postcondition_kind(query).is_some(),
                        "ephemeral postcondition has/find bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        spec.has_condition_kind(query),
                        spec.find_condition_kind(query).is_some(),
                        "ephemeral union has/find bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    // ── EphemeralSpec::iter_(pre|post|)condition_kind widened triad ──
    //
    // Fail-before-pass-after granularity: the three widened
    // `iter_*_kind` arms did not exist on the ephemeral surface before
    // this commit — the (widened `impl Iterator<Item = &Condition>`
    // stream) axis lived at ONE struct-level site
    // (`Boundary::iter_condition_kind` on the point surface's nested
    // [`Boundary`] slot). The lift adds the peer inherent methods on
    // the [`EphemeralSpec`] sugar-surface so both struct-level widened
    // callers compose against the SAME slice-level substrate primitive
    // [`crate::boundary::ConditionSliceExt::iter_kind`] in lockstep.
    // A regression that (a) hard-coded the arm to a single kind, (b)
    // reversed the chain order on the union (postcondition first), or
    // (c) collapsed the chain to a `.zip(...)` (silently narrowing the
    // union to an intersection-by-position) fails HERE at the
    // substrate primitive rather than as silent operator-facing drift
    // at the ephemeral require-tag surface.

    /// EMPTY-SPEC pin (iter-triad) — a default [`EphemeralSpec`]
    /// (empty preconditions, empty postconditions) yields nothing
    /// from every widened arm for EVERY [`ConditionKind`]. Sweep
    /// `ConditionKind::ALL` × three-arm cross so a new variant added
    /// without a matching arm surfaces at rustc's exhaustiveness gate
    /// on the ALL literal rather than as a silent phantom-yield at
    /// every downstream widened callsite on the ephemeral surface.
    #[test]
    fn ephemeral_iter_condition_kind_triad_yields_nothing_on_empty_spec() {
        let spec = empty_ephemeral();
        for kind in ConditionKind::ALL {
            assert_eq!(
                spec.iter_precondition_kind(kind).count(),
                0,
                "empty ephemeral must yield nothing on precondition iter arm for {kind:?}",
            );
            assert_eq!(
                spec.iter_postcondition_kind(kind).count(),
                0,
                "empty ephemeral must yield nothing on postcondition iter arm for {kind:?}",
            );
            assert_eq!(
                spec.iter_condition_kind(kind).count(),
                0,
                "empty ephemeral must yield nothing on union iter arm for {kind:?}",
            );
        }
    }

    /// SUBSTRATE-DELEGATION pin (ephemeral iter-triad) — the three
    /// widened `iter_*_kind` methods on [`EphemeralSpec`] delegate
    /// verbatim to [`crate::boundary::ConditionSliceExt::iter_kind`]
    /// on the underlying [`Vec<Condition>`] slices, no inline
    /// reimplementation. The `iter_condition_kind` union chains
    /// preconditions first then postconditions via
    /// [`Iterator::chain`]. Sweep
    /// `ConditionKind::ALL × ConditionKind::ALL × ConditionKind::ALL`
    /// so a regression that (a) inlined a divergent walk at either
    /// half-slice arm, (b) reversed the chain order on the ephemeral
    /// surface only (breaking two-surface parity with
    /// [`crate::boundary::Boundary::iter_condition_kind`]), or (c)
    /// collapsed the chain to a `.zip(...)` surfaces HERE at the
    /// substrate boundary. Byte-for-byte peer of the point-domain
    /// `iter_condition_kind_triad_delegates_to_slice_iter_kind` pin.
    #[test]
    fn ephemeral_iter_condition_kind_triad_delegates_to_slice_iter_kind() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                for query in ConditionKind::ALL {
                    let via_pre: Vec<_> = spec
                        .preconditions
                        .iter_kind(query)
                        .map(|c| c.kind)
                        .collect();
                    let via_post: Vec<_> = spec
                        .postconditions
                        .iter_kind(query)
                        .map(|c| c.kind)
                        .collect();
                    assert_eq!(
                        spec.iter_precondition_kind(query)
                            .map(|c| c.kind)
                            .collect::<Vec<_>>(),
                        via_pre,
                        "ephemeral precondition iter arm must delegate: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        spec.iter_postcondition_kind(query)
                            .map(|c| c.kind)
                            .collect::<Vec<_>>(),
                        via_post,
                        "ephemeral postcondition iter arm must delegate: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    let mut expected_union = via_pre.clone();
                    expected_union.extend(via_post.iter().copied());
                    assert_eq!(
                        spec.iter_condition_kind(query)
                            .map(|c| c.kind)
                            .collect::<Vec<_>>(),
                        expected_union,
                        "ephemeral union iter arm must chain precondition ⨟ postcondition: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// PRECONDITION-PRECEDENCE pin (ephemeral iter) — a kind
    /// authored on BOTH sides yields precondition-side matches
    /// FIRST in the union chain. Byte-for-byte peer of the
    /// point-domain
    /// `iter_condition_kind_yields_preconditions_before_postconditions_on_dual_populated`
    /// pin — the two-surface parity contract binds the chain order
    /// on both surfaces through ONE composition law. Uses two
    /// params-distinguishable [`Condition`]s so a regression on the
    /// ephemeral surface only that reversed the chain order surfaces
    /// at the returned params payload rather than silently at the
    /// count.
    #[test]
    fn ephemeral_iter_condition_kind_yields_preconditions_before_postconditions_on_dual_populated()
    {
        let mut spec = empty_ephemeral();
        spec.preconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: serde_json::json!({ "side": "pre-1" }),
        });
        spec.postconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: serde_json::json!({ "side": "post-1" }),
        });
        spec.postconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: serde_json::json!({ "side": "post-2" }),
        });
        let sides: Vec<_> = spec
            .iter_condition_kind(ConditionKind::ClosedLoopAuth)
            .map(|c| {
                c.params
                    .get("side")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            sides,
            vec!["pre-1".to_owned(), "post-1".to_owned(), "post-2".to_owned(),],
            "ephemeral iter_condition_kind must yield every precondition-side match before \
             any postcondition-side match (chain order pinned by two-surface parity)",
        );
    }

    /// STRUCT-LEVEL DELEGATION pin (find ↔ iter on EphemeralSpec) —
    /// the three [`EphemeralSpec`] `find_*_kind` arms equal their
    /// widened peers' `.next()` projection at EVERY (pre-populated,
    /// post-populated, query) triple on `ConditionKind::ALL`.
    /// Byte-for-byte peer of the point-domain
    /// `boundary_find_triad_equals_iter_triad_next_projection` pin,
    /// so both surfaces' find/iter refinement bridge stays symmetric
    /// by construction across the two-surface parity contract.
    #[test]
    fn ephemeral_find_triad_equals_iter_triad_next_projection() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                for query in ConditionKind::ALL {
                    assert_eq!(
                        spec.find_precondition_kind(query).map(|c| c.kind),
                        spec.iter_precondition_kind(query).next().map(|c| c.kind),
                        "ephemeral precondition find/iter bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        spec.find_postcondition_kind(query).map(|c| c.kind),
                        spec.iter_postcondition_kind(query).next().map(|c| c.kind),
                        "ephemeral postcondition find/iter bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        spec.find_condition_kind(query).map(|c| c.kind),
                        spec.iter_condition_kind(query).next().map(|c| c.kind),
                        "ephemeral union find/iter bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    // ── EphemeralSpec count triad — scalar cardinality peers ─────────
    //
    // Byte-for-byte peers of the point-domain `Boundary`
    // `count_(pre|post|)condition_kind` triad, tested at the ephemeral
    // sugar surface. Same SUM composition on the union arm, same
    // slice-level substrate delegation, same composition-law bridge
    // against the widened iter refinement.

    /// EMPTY-SPEC pin (count-triad) — a default [`EphemeralSpec`]
    /// counts `0` from every arm of the count triad for EVERY
    /// [`ConditionKind`].
    #[test]
    fn ephemeral_count_condition_kind_triad_returns_zero_on_empty_spec() {
        let spec = empty_ephemeral();
        for kind in ConditionKind::ALL {
            assert_eq!(
                spec.count_precondition_kind(kind),
                0,
                "empty ephemeral must count 0 on precondition arm for {kind:?}",
            );
            assert_eq!(
                spec.count_postcondition_kind(kind),
                0,
                "empty ephemeral must count 0 on postcondition arm for {kind:?}",
            );
            assert_eq!(
                spec.count_condition_kind(kind),
                0,
                "empty ephemeral must count 0 on union arm for {kind:?}",
            );
        }
    }

    /// SUBSTRATE-DELEGATION pin (ephemeral count-triad) — the three
    /// widened `count_*_kind` methods on [`EphemeralSpec`] delegate
    /// verbatim to [`crate::boundary::ConditionSliceExt::count_kind`]
    /// on the underlying [`Vec<Condition>`] slices. The
    /// `count_condition_kind` union SUMS preconditions and
    /// postconditions. Byte-for-byte peer of the point-domain
    /// `boundary_count_condition_kind_triad_delegates_and_sums_slice_count_kind`
    /// pin; a regression that (a) subtracted rather than summed, (b)
    /// collapsed the sum to [`std::cmp::max`], or (c) inlined a
    /// divergent count at either half-slice arm on the ephemeral
    /// surface only (breaking two-surface parity with [`Boundary`])
    /// surfaces HERE.
    #[test]
    fn ephemeral_count_condition_kind_triad_delegates_and_sums_slice_count_kind() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                for query in ConditionKind::ALL {
                    let via_pre = spec.preconditions.count_kind(query);
                    let via_post = spec.postconditions.count_kind(query);
                    assert_eq!(
                        spec.count_precondition_kind(query),
                        via_pre,
                        "ephemeral precondition count arm must delegate: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        spec.count_postcondition_kind(query),
                        via_post,
                        "ephemeral postcondition count arm must delegate: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        spec.count_condition_kind(query),
                        via_pre + via_post,
                        "ephemeral union count arm must SUM pre + post: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// STRUCT-LEVEL DELEGATION pin (count ↔ iter on EphemeralSpec) —
    /// the three [`EphemeralSpec`] `count_*_kind` arms equal their
    /// widened peers' `.count()` projection at EVERY (pre-populated
    /// twice, post-populated, query) triple. Byte-for-byte peer of
    /// the point-domain
    /// `boundary_count_triad_equals_iter_triad_count_projection`
    /// pin. Uses two-preconditions authoring so the union arm's SUM
    /// composition witnesses a nontrivial cardinality (rather than
    /// coinciding with the presence bit).
    #[test]
    fn ephemeral_count_triad_equals_iter_triad_count_projection() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                for query in ConditionKind::ALL {
                    assert_eq!(
                        spec.count_precondition_kind(query),
                        spec.iter_precondition_kind(query).count(),
                        "ephemeral precondition count/iter bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        spec.count_postcondition_kind(query),
                        spec.iter_postcondition_kind(query).count(),
                        "ephemeral postcondition count/iter bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        spec.count_condition_kind(query),
                        spec.iter_condition_kind(query).count(),
                        "ephemeral union count/iter bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    // ── EphemeralSpec distinct-set triad — substrate-delegation pin ──
    //
    // The (precondition, postcondition, condition-union) distinct-set
    // triad on [`EphemeralSpec`] delegates to the slice-level substrate
    // primitive [`crate::boundary::ConditionSliceExt::distinct_kinds`]
    // on each half-slice and composes the union via
    // [`Self::has_condition_kind`] over [`ConditionKind::ALL`] — byte-
    // for-byte peer of the point-surface distinct-set triad on
    // [`crate::boundary::Boundary`]. The two-surface parity contract
    // now covers FIVE refinements on the condition axis: the four
    // point-probe refinements (has / find / iter / count) AND the ONE
    // closed-set-inversion refinement (distinct-set) on both surfaces.

    /// SUBSTRATE-DELEGATION pin (ephemeral surface, distinct-kind-count
    /// triad) — the three `distinct_*_kind_count` methods on
    /// [`EphemeralSpec`] delegate to the slice-level substrate
    /// primitive [`crate::boundary::ConditionSliceExt::distinct_kind_count`]
    /// over the two `Vec<Condition>` slots and compose the union
    /// scalar via `ConditionKind::ALL.filter(|k|
    /// has_condition_kind(*k)).count()`. Byte-for-byte peer of the
    /// point-surface pin
    /// `distinct_condition_kind_count_triad_delegates_to_slice_distinct_kind_count`
    /// on [`crate::boundary::Boundary`] — the two-surface parity
    /// contract now binds every downstream scalar-cardinality consumer
    /// on either surface to the SAME closed-set walk through ONE
    /// substrate rather than through per-surface `.distinct_*_kinds().len()`
    /// re-materializations that pay for a heap allocation.
    #[test]
    fn ephemeral_distinct_condition_kind_count_triad_delegates_and_matches_distinct_kinds_len() {
        // Empty spec — every arm returns 0.
        let spec = empty_ephemeral();
        for kind in ConditionKind::ALL {
            assert_eq!(
                spec.distinct_precondition_kind_count(),
                0,
                "empty ephemeral spec must return 0 on distinct_precondition_kind_count, kind={kind:?}",
            );
            assert_eq!(
                spec.distinct_postcondition_kind_count(),
                0,
                "empty ephemeral spec must return 0 on distinct_postcondition_kind_count, kind={kind:?}",
            );
            assert_eq!(
                spec.distinct_condition_kind_count(),
                0,
                "empty ephemeral spec must return 0 on distinct_condition_kind_count, kind={kind:?}",
            );
        }

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));

                assert_eq!(
                    spec.distinct_precondition_kind_count(),
                    spec.preconditions.distinct_kind_count(),
                    "EphemeralSpec::distinct_precondition_kind_count must delegate verbatim to \
                     preconditions.distinct_kind_count() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.distinct_precondition_kind_count(),
                    spec.distinct_precondition_kinds().len(),
                    "EphemeralSpec::distinct_precondition_kind_count must equal \
                     distinct_precondition_kinds().len() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.distinct_postcondition_kind_count(),
                    spec.postconditions.distinct_kind_count(),
                    "EphemeralSpec::distinct_postcondition_kind_count must delegate verbatim to \
                     postconditions.distinct_kind_count() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.distinct_postcondition_kind_count(),
                    spec.distinct_postcondition_kinds().len(),
                    "EphemeralSpec::distinct_postcondition_kind_count must equal \
                     distinct_postcondition_kinds().len() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union_count = if pre_kind == post_kind { 1 } else { 2 };
                assert_eq!(
                    spec.distinct_condition_kind_count(),
                    expected_union_count,
                    "EphemeralSpec::distinct_condition_kind_count must count distinct union kinds \
                     for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.distinct_condition_kind_count(),
                    spec.distinct_condition_kinds().len(),
                    "EphemeralSpec::distinct_condition_kind_count must equal \
                     distinct_condition_kinds().len() for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (ephemeral surface, distinct-set triad)
    /// — the three `distinct_*_kinds` methods on [`EphemeralSpec`]
    /// delegate to the slice-level substrate primitive over the two
    /// `Vec<Condition>` slots and compose the union via
    /// `ConditionKind::ALL.filter(|k| has_condition_kind(*k))`. Byte-
    /// for-byte peer of the point-surface pin
    /// `distinct_condition_kinds_triad_delegates_to_slice_distinct_kinds`
    /// on [`crate::boundary::Boundary`] — the two-surface parity
    /// contract binds every downstream distinct-set consumer on either
    /// surface to the SAME closed-set-inversion primitive through ONE
    /// substrate rather than through per-surface re-authored sweeps.
    #[test]
    fn ephemeral_distinct_condition_kinds_triad_delegates_to_slice_distinct_kinds() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));

                assert_eq!(
                    spec.distinct_precondition_kinds(),
                    spec.preconditions.distinct_kinds(),
                    "EphemeralSpec::distinct_precondition_kinds must delegate verbatim to \
                     preconditions.distinct_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.distinct_postcondition_kinds(),
                    spec.postconditions.distinct_kinds(),
                    "EphemeralSpec::distinct_postcondition_kinds must delegate verbatim to \
                     postconditions.distinct_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union: Vec<_> = ConditionKind::ALL
                    .into_iter()
                    .filter(|k| pre_kind == *k || post_kind == *k)
                    .collect();
                assert_eq!(
                    spec.distinct_condition_kinds(),
                    expected_union,
                    "EphemeralSpec::distinct_condition_kinds must equal ConditionKind::ALL-ordered \
                     set-union of the two half-slice distinct-sets for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec distinct-set ITERATOR
    /// triad) — the three `iter_distinct_*_condition_kinds` methods on
    /// [`EphemeralSpec`] delegate to the slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::iter_distinct_kinds`] over
    /// the two `Vec<Condition>` slots and compose the union via
    /// `ConditionKind::ALL.iter().copied().filter(|&k|
    /// has_condition_kind(k))`. Byte-for-byte peer of
    /// `iter_distinct_condition_kinds_triad_delegates_to_slice_iter_distinct_kinds`
    /// on the point-domain [`crate::boundary::Boundary`] surface — both
    /// peers compose against the SAME slice-level iterator substrate.
    #[test]
    fn ephemeral_iter_distinct_condition_kinds_triad_delegates_to_slice_iter_distinct_kinds() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));

                let pre_via_iter: Vec<_> = spec.iter_distinct_precondition_kinds().collect();
                assert_eq!(
                    pre_via_iter,
                    spec.distinct_precondition_kinds(),
                    "EphemeralSpec::iter_distinct_precondition_kinds().collect() drifted from \
                     distinct_precondition_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                let post_via_iter: Vec<_> = spec.iter_distinct_postcondition_kinds().collect();
                assert_eq!(
                    post_via_iter,
                    spec.distinct_postcondition_kinds(),
                    "EphemeralSpec::iter_distinct_postcondition_kinds().collect() drifted from \
                     distinct_postcondition_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                let union_via_iter: Vec<_> = spec.iter_distinct_condition_kinds().collect();
                assert_eq!(
                    union_via_iter,
                    spec.distinct_condition_kinds(),
                    "EphemeralSpec::iter_distinct_condition_kinds().collect() drifted from \
                     distinct_condition_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec missing-set ITERATOR
    /// triad) — the three `iter_missing_*_condition_kinds` methods on
    /// [`EphemeralSpec`] delegate to the slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::iter_missing_kinds`] over
    /// the two `Vec<Condition>` slots and compose the union via
    /// `ConditionKind::ALL.iter().copied().filter(|&k|
    /// !has_condition_kind(k))`. Peer of
    /// `ephemeral_iter_distinct_condition_kinds_triad_delegates_to_slice_iter_distinct_kinds`
    /// on the missing side under a NEGATED point-probe.
    #[test]
    fn ephemeral_iter_missing_condition_kinds_triad_delegates_to_slice_iter_missing_kinds() {
        let empty = empty_ephemeral();
        let all: Vec<_> = ConditionKind::ALL.to_vec();
        assert_eq!(
            empty.iter_missing_precondition_kinds().collect::<Vec<_>>(),
            all,
            "empty ephemeral spec must yield ConditionKind::ALL on iter_missing_precondition_kinds",
        );
        assert_eq!(
            empty.iter_missing_postcondition_kinds().collect::<Vec<_>>(),
            all,
            "empty ephemeral spec must yield ConditionKind::ALL on iter_missing_postcondition_kinds",
        );
        assert_eq!(
            empty.iter_missing_condition_kinds().collect::<Vec<_>>(),
            all,
            "empty ephemeral spec must yield ConditionKind::ALL on iter_missing_condition_kinds",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));

                let pre_via_iter: Vec<_> = spec.iter_missing_precondition_kinds().collect();
                assert_eq!(
                    pre_via_iter,
                    spec.missing_precondition_kinds(),
                    "EphemeralSpec::iter_missing_precondition_kinds().collect() drifted from \
                     missing_precondition_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                let post_via_iter: Vec<_> = spec.iter_missing_postcondition_kinds().collect();
                assert_eq!(
                    post_via_iter,
                    spec.missing_postcondition_kinds(),
                    "EphemeralSpec::iter_missing_postcondition_kinds().collect() drifted from \
                     missing_postcondition_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                let union_via_iter: Vec<_> = spec.iter_missing_condition_kinds().collect();
                assert_eq!(
                    union_via_iter,
                    spec.missing_condition_kinds(),
                    "EphemeralSpec::iter_missing_condition_kinds().collect() drifted from \
                     missing_condition_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec missing-set triad) —
    /// the three `missing_*_kinds` methods on [`EphemeralSpec`]
    /// delegate to the slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::missing_kinds`] over the
    /// two `Vec<Condition>` slots and compose the union via
    /// `ConditionKind::ALL.filter(|k| !has_condition_kind(*k))`. Sweep
    /// `ConditionKind::ALL × ConditionKind::ALL`. Byte-for-byte peer of
    /// `missing_condition_kinds_triad_delegates_to_slice_missing_kinds`
    /// on the point-domain [`crate::boundary::Boundary`] surface —
    /// both peers compose against the SAME slice-level substrate
    /// primitive so a regression at the per-slice complement walk
    /// fails at that primitive's tests rather than as silent drift at
    /// either struct-level arm.
    #[test]
    fn ephemeral_missing_condition_kinds_triad_delegates_to_slice_missing_kinds() {
        // Empty ephemeral spec — every arm returns ConditionKind::ALL.
        let empty = empty_ephemeral();
        let all_kinds = ConditionKind::ALL.to_vec();
        assert_eq!(
            empty.missing_precondition_kinds(),
            all_kinds,
            "empty ephemeral spec must return ConditionKind::ALL on missing_precondition_kinds",
        );
        assert_eq!(
            empty.missing_postcondition_kinds(),
            all_kinds,
            "empty ephemeral spec must return ConditionKind::ALL on missing_postcondition_kinds",
        );
        assert_eq!(
            empty.missing_condition_kinds(),
            all_kinds,
            "empty ephemeral spec must return ConditionKind::ALL on missing_condition_kinds",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));

                assert_eq!(
                    spec.missing_precondition_kinds(),
                    spec.preconditions.missing_kinds(),
                    "EphemeralSpec::missing_precondition_kinds must delegate verbatim to \
                     preconditions.missing_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.missing_postcondition_kinds(),
                    spec.postconditions.missing_kinds(),
                    "EphemeralSpec::missing_postcondition_kinds must delegate verbatim to \
                     postconditions.missing_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                // Union: a kind is missing from the union iff it is
                // missing from BOTH half-slices (SET-INTERSECTION).
                let expected_union: Vec<_> = ConditionKind::ALL
                    .into_iter()
                    .filter(|k| pre_kind != *k && post_kind != *k)
                    .collect();
                assert_eq!(
                    spec.missing_condition_kinds(),
                    expected_union,
                    "EphemeralSpec::missing_condition_kinds must equal ConditionKind::ALL-ordered \
                     set-INTERSECTION of the two half-slice missing-sets for pre={pre_kind:?} post={post_kind:?}",
                );
                // Partition invariant (distinct ∪ missing == ALL, disjoint).
                let distinct = spec.distinct_condition_kinds();
                let missing = spec.missing_condition_kinds();
                for kind in ConditionKind::ALL {
                    assert!(
                        distinct.contains(&kind) ^ missing.contains(&kind),
                        "EphemeralSpec (distinct, missing) partition violated on {kind:?} for pre={pre_kind:?} post={post_kind:?}",
                    );
                }
                assert_eq!(
                    distinct.len() + missing.len(),
                    ConditionKind::ALL.len(),
                    "EphemeralSpec (distinct, missing) cardinality partition drift for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec missing-kind-count triad)
    /// — the three `missing_*_kind_count` methods on [`EphemeralSpec`]
    /// delegate to the slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::missing_kind_count`] over
    /// the two `Vec<Condition>` slots and compose the union scalar via
    /// `ConditionKind::ALL.iter().filter(|k|
    /// !has_condition_kind(**k)).count()`. Sweep
    /// `ConditionKind::ALL × ConditionKind::ALL`. Byte-for-byte peer of
    /// `missing_condition_kind_count_triad_delegates_to_slice_missing_kind_count`
    /// on the point-domain [`crate::boundary::Boundary`] surface —
    /// both peers compose against the SAME slice-level substrate
    /// primitive so a regression at the per-slice negated closed-set
    /// walk fails at that primitive's tests rather than as silent drift
    /// at either struct-level scalar-cardinality arm. Also pins the
    /// scalar-partition invariant `distinct_kind_count +
    /// missing_kind_count == ConditionKind::ALL.len()` per arrangement.
    #[test]
    fn ephemeral_missing_condition_kind_count_triad_delegates_to_slice_missing_kind_count() {
        // Empty ephemeral spec — every arm returns ConditionKind::ALL.len().
        let empty = empty_ephemeral();
        let total = ConditionKind::ALL.len();
        assert_eq!(
            empty.missing_precondition_kind_count(),
            total,
            "empty ephemeral spec must return ConditionKind::ALL.len() on missing_precondition_kind_count",
        );
        assert_eq!(
            empty.missing_postcondition_kind_count(),
            total,
            "empty ephemeral spec must return ConditionKind::ALL.len() on missing_postcondition_kind_count",
        );
        assert_eq!(
            empty.missing_condition_kind_count(),
            total,
            "empty ephemeral spec must return ConditionKind::ALL.len() on missing_condition_kind_count",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));

                // Half-slice arms delegate byte-for-byte to the slice
                // substrate primitive.
                assert_eq!(
                    spec.missing_precondition_kind_count(),
                    spec.preconditions.missing_kind_count(),
                    "EphemeralSpec::missing_precondition_kind_count must delegate verbatim to \
                     preconditions.missing_kind_count() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.missing_precondition_kind_count(),
                    spec.missing_precondition_kinds().len(),
                    "EphemeralSpec::missing_precondition_kind_count must equal \
                     missing_precondition_kinds().len() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.missing_postcondition_kind_count(),
                    spec.postconditions.missing_kind_count(),
                    "EphemeralSpec::missing_postcondition_kind_count must delegate verbatim to \
                     postconditions.missing_kind_count() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.missing_postcondition_kind_count(),
                    spec.missing_postcondition_kinds().len(),
                    "EphemeralSpec::missing_postcondition_kind_count must equal \
                     missing_postcondition_kinds().len() for pre={pre_kind:?} post={post_kind:?}",
                );
                // Union arm equals missing_condition_kinds().len().
                assert_eq!(
                    spec.missing_condition_kind_count(),
                    spec.missing_condition_kinds().len(),
                    "EphemeralSpec::missing_condition_kind_count must equal \
                     missing_condition_kinds().len() for pre={pre_kind:?} post={post_kind:?}",
                );
                // Scalar-partition invariant: distinct + missing == ALL.
                assert_eq!(
                    spec.distinct_condition_kind_count() + spec.missing_condition_kind_count(),
                    ConditionKind::ALL.len(),
                    "EphemeralSpec (distinct, missing) scalar partition drift for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec first-distinct-kind
    /// triad) — the three `first_distinct_*_kind` methods on
    /// [`EphemeralSpec`] delegate to the slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::first_distinct_kind`] over
    /// the two `Vec<Condition>` slots and compose the union via
    /// `ConditionKind::ALL.iter().copied().find(|k|
    /// has_condition_kind(*k))`. Byte-for-byte peer of
    /// `first_distinct_condition_kind_triad_delegates_to_slice_first_distinct_kind`
    /// on the point-domain [`crate::boundary::Boundary`] surface — both
    /// peers compose against the SAME slice-level substrate primitive
    /// so a regression at the per-slice short-circuit walk fails at
    /// that primitive's tests rather than as silent drift at either
    /// struct-level earliest-element arm.
    #[test]
    fn ephemeral_first_distinct_condition_kind_triad_delegates_to_slice_first_distinct_kind() {
        // Empty ephemeral spec — every arm returns None.
        let empty = empty_ephemeral();
        assert_eq!(
            empty.first_distinct_precondition_kind(),
            None,
            "empty ephemeral spec must return None on first_distinct_precondition_kind",
        );
        assert_eq!(
            empty.first_distinct_postcondition_kind(),
            None,
            "empty ephemeral spec must return None on first_distinct_postcondition_kind",
        );
        assert_eq!(
            empty.first_distinct_condition_kind(),
            None,
            "empty ephemeral spec must return None on first_distinct_condition_kind",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));

                assert_eq!(
                    spec.first_distinct_precondition_kind(),
                    spec.preconditions.first_distinct_kind(),
                    "EphemeralSpec::first_distinct_precondition_kind must delegate verbatim to \
                     preconditions.first_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.first_distinct_precondition_kind(),
                    spec.distinct_precondition_kinds().first().copied(),
                    "EphemeralSpec::first_distinct_precondition_kind must equal \
                     distinct_precondition_kinds().first().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.first_distinct_postcondition_kind(),
                    spec.postconditions.first_distinct_kind(),
                    "EphemeralSpec::first_distinct_postcondition_kind must delegate verbatim to \
                     postconditions.first_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.first_distinct_postcondition_kind(),
                    spec.distinct_postcondition_kinds().first().copied(),
                    "EphemeralSpec::first_distinct_postcondition_kind must equal \
                     distinct_postcondition_kinds().first().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union = ConditionKind::ALL
                    .into_iter()
                    .find(|k| pre_kind == *k || post_kind == *k);
                assert_eq!(
                    spec.first_distinct_condition_kind(),
                    expected_union,
                    "EphemeralSpec::first_distinct_condition_kind must equal earliest ALL entry \
                     populated by either half-slice for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.first_distinct_condition_kind(),
                    spec.distinct_condition_kinds().first().copied(),
                    "EphemeralSpec::first_distinct_condition_kind must equal \
                     distinct_condition_kinds().first().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec first-missing-kind triad)
    /// — the three `first_missing_*_kind` methods on [`EphemeralSpec`]
    /// delegate to the slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::first_missing_kind`] over
    /// the two `Vec<Condition>` slots and compose the union via
    /// `ConditionKind::ALL.iter().copied().find(|k|
    /// !has_condition_kind(*k))`. Byte-for-byte peer of
    /// `first_missing_condition_kind_triad_delegates_to_slice_first_missing_kind`
    /// on the point-domain [`crate::boundary::Boundary`] surface.
    #[test]
    fn ephemeral_first_missing_condition_kind_triad_delegates_to_slice_first_missing_kind() {
        // Empty ephemeral spec — every arm returns Some(ConditionKind::ALL[0]).
        let empty = empty_ephemeral();
        let first = Some(ConditionKind::ALL[0]);
        assert_eq!(
            empty.first_missing_precondition_kind(),
            first,
            "empty ephemeral spec must return Some(ConditionKind::ALL[0]) on first_missing_precondition_kind",
        );
        assert_eq!(
            empty.first_missing_postcondition_kind(),
            first,
            "empty ephemeral spec must return Some(ConditionKind::ALL[0]) on first_missing_postcondition_kind",
        );
        assert_eq!(
            empty.first_missing_condition_kind(),
            first,
            "empty ephemeral spec must return Some(ConditionKind::ALL[0]) on first_missing_condition_kind",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));

                assert_eq!(
                    spec.first_missing_precondition_kind(),
                    spec.preconditions.first_missing_kind(),
                    "EphemeralSpec::first_missing_precondition_kind must delegate verbatim to \
                     preconditions.first_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.first_missing_precondition_kind(),
                    spec.missing_precondition_kinds().first().copied(),
                    "EphemeralSpec::first_missing_precondition_kind must equal \
                     missing_precondition_kinds().first().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.first_missing_postcondition_kind(),
                    spec.postconditions.first_missing_kind(),
                    "EphemeralSpec::first_missing_postcondition_kind must delegate verbatim to \
                     postconditions.first_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.first_missing_postcondition_kind(),
                    spec.missing_postcondition_kinds().first().copied(),
                    "EphemeralSpec::first_missing_postcondition_kind must equal \
                     missing_postcondition_kinds().first().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union = ConditionKind::ALL
                    .into_iter()
                    .find(|k| pre_kind != *k && post_kind != *k);
                assert_eq!(
                    spec.first_missing_condition_kind(),
                    expected_union,
                    "EphemeralSpec::first_missing_condition_kind must equal earliest ALL entry \
                     NOT populated by either half-slice for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.first_missing_condition_kind(),
                    spec.missing_condition_kinds().first().copied(),
                    "EphemeralSpec::first_missing_condition_kind must equal \
                     missing_condition_kinds().first().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec last-distinct-kind
    /// triad) — the three `last_distinct_*_kind` methods on
    /// [`EphemeralSpec`] delegate to the slice-level substrate
    /// primitive [`crate::boundary::ConditionSliceExt::last_distinct_kind`]
    /// over the two `Vec<Condition>` slots and compose the union via
    /// `ConditionKind::ALL.iter().rev().copied().find(|k|
    /// has_condition_kind(*k))`. Byte-for-byte peer of
    /// `last_distinct_condition_kind_triad_delegates_to_slice_last_distinct_kind`
    /// on the point-domain [`crate::boundary::Boundary`] surface —
    /// both peers compose against the SAME slice-level substrate
    /// primitive so a regression at the per-slice REVERSED short-
    /// circuit walk fails at that primitive's tests rather than as
    /// silent drift at either struct-level latest-element arm.
    #[test]
    fn ephemeral_last_distinct_condition_kind_triad_delegates_to_slice_last_distinct_kind() {
        // Empty ephemeral spec — every arm returns None.
        let empty = empty_ephemeral();
        assert_eq!(
            empty.last_distinct_precondition_kind(),
            None,
            "empty ephemeral spec must return None on last_distinct_precondition_kind",
        );
        assert_eq!(
            empty.last_distinct_postcondition_kind(),
            None,
            "empty ephemeral spec must return None on last_distinct_postcondition_kind",
        );
        assert_eq!(
            empty.last_distinct_condition_kind(),
            None,
            "empty ephemeral spec must return None on last_distinct_condition_kind",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));

                assert_eq!(
                    spec.last_distinct_precondition_kind(),
                    spec.preconditions.last_distinct_kind(),
                    "EphemeralSpec::last_distinct_precondition_kind must delegate verbatim to \
                     preconditions.last_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.last_distinct_precondition_kind(),
                    spec.distinct_precondition_kinds().last().copied(),
                    "EphemeralSpec::last_distinct_precondition_kind must equal \
                     distinct_precondition_kinds().last().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.last_distinct_postcondition_kind(),
                    spec.postconditions.last_distinct_kind(),
                    "EphemeralSpec::last_distinct_postcondition_kind must delegate verbatim to \
                     postconditions.last_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.last_distinct_postcondition_kind(),
                    spec.distinct_postcondition_kinds().last().copied(),
                    "EphemeralSpec::last_distinct_postcondition_kind must equal \
                     distinct_postcondition_kinds().last().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union = ConditionKind::ALL
                    .into_iter()
                    .rev()
                    .find(|k| pre_kind == *k || post_kind == *k);
                assert_eq!(
                    spec.last_distinct_condition_kind(),
                    expected_union,
                    "EphemeralSpec::last_distinct_condition_kind must equal latest ALL entry \
                     populated by either half-slice for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.last_distinct_condition_kind(),
                    spec.distinct_condition_kinds().last().copied(),
                    "EphemeralSpec::last_distinct_condition_kind must equal \
                     distinct_condition_kinds().last().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec last-missing-kind triad)
    /// — the three `last_missing_*_kind` methods on [`EphemeralSpec`]
    /// delegate to the slice-level substrate primitive
    /// [`crate::boundary::ConditionSliceExt::last_missing_kind`] over
    /// the two `Vec<Condition>` slots and compose the union via
    /// `ConditionKind::ALL.iter().rev().copied().find(|k|
    /// !has_condition_kind(*k))`. Byte-for-byte peer of
    /// `last_missing_condition_kind_triad_delegates_to_slice_last_missing_kind`
    /// on the point-domain [`crate::boundary::Boundary`] surface.
    #[test]
    fn ephemeral_last_missing_condition_kind_triad_delegates_to_slice_last_missing_kind() {
        // Empty ephemeral spec — every arm returns Some(*ConditionKind::ALL.last().unwrap()).
        let empty = empty_ephemeral();
        let last = ConditionKind::ALL.last().copied();
        assert_eq!(
            empty.last_missing_precondition_kind(),
            last,
            "empty ephemeral spec must return Some(*ConditionKind::ALL.last().unwrap()) on last_missing_precondition_kind",
        );
        assert_eq!(
            empty.last_missing_postcondition_kind(),
            last,
            "empty ephemeral spec must return Some(*ConditionKind::ALL.last().unwrap()) on last_missing_postcondition_kind",
        );
        assert_eq!(
            empty.last_missing_condition_kind(),
            last,
            "empty ephemeral spec must return Some(*ConditionKind::ALL.last().unwrap()) on last_missing_condition_kind",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));

                assert_eq!(
                    spec.last_missing_precondition_kind(),
                    spec.preconditions.last_missing_kind(),
                    "EphemeralSpec::last_missing_precondition_kind must delegate verbatim to \
                     preconditions.last_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.last_missing_precondition_kind(),
                    spec.missing_precondition_kinds().last().copied(),
                    "EphemeralSpec::last_missing_precondition_kind must equal \
                     missing_precondition_kinds().last().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.last_missing_postcondition_kind(),
                    spec.postconditions.last_missing_kind(),
                    "EphemeralSpec::last_missing_postcondition_kind must delegate verbatim to \
                     postconditions.last_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.last_missing_postcondition_kind(),
                    spec.missing_postcondition_kinds().last().copied(),
                    "EphemeralSpec::last_missing_postcondition_kind must equal \
                     missing_postcondition_kinds().last().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union = ConditionKind::ALL
                    .into_iter()
                    .rev()
                    .find(|k| pre_kind != *k && post_kind != *k);
                assert_eq!(
                    spec.last_missing_condition_kind(),
                    expected_union,
                    "EphemeralSpec::last_missing_condition_kind must equal latest ALL entry \
                     NOT populated by either half-slice for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.last_missing_condition_kind(),
                    spec.missing_condition_kinds().last().copied(),
                    "EphemeralSpec::last_missing_condition_kind must equal \
                     missing_condition_kinds().last().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    // ── assert_slice_refinement_composition_laws — mirror invocations ──
    //
    // The substrate testkit primitive
    // [`crate::boundary::assert_slice_refinement_composition_laws`]
    // pins the FOUR composition laws that bind the
    // [`crate::boundary::ConditionSliceExt`] refinement algebra
    // (find ↔ iter, count ↔ iter, has ↔ find, has ↔ count) at ONE
    // call site per authored arrangement, sweeping
    // [`ConditionKind::ALL`]. The two ephemeral-surface tests below
    // dispatch the primitive against the two `Vec<Condition>` slots
    // ([`EphemeralSpec::preconditions`] +
    // [`EphemeralSpec::postconditions`]) authored through the
    // ephemeral-surface test-fixture — byte-for-byte peer of the
    // point-surface `slice_refinement_composition_laws_hold_across_authored_arrangements`
    // + `slice_refinement_composition_laws_hold_on_interleaved_duplicates`
    // pins on the [`crate::boundary::Boundary`] surface. Two-surface
    // parity contract: the substrate primitive holds on every slice
    // reachable through either the point-surface `.preconditions` /
    // `.postconditions` fields OR the ephemeral-surface's
    // eponymous field pair.

    /// SUBSTRATE PANEL pin (ephemeral surface) — the substrate
    /// primitive [`assert_slice_refinement_composition_laws`] holds
    /// on both [`EphemeralSpec::preconditions`] and
    /// [`EphemeralSpec::postconditions`] slices for every populated-
    /// pair authored through the ephemeral-surface test-fixture.
    /// Byte-for-byte peer of
    /// `slice_refinement_composition_laws_hold_across_authored_arrangements`
    /// on the point surface.
    #[test]
    fn ephemeral_slice_refinement_composition_laws_hold_across_authored_arrangements() {
        let empty = empty_ephemeral();
        assert_slice_refinement_composition_laws(empty.preconditions.as_slice());
        assert_slice_refinement_composition_laws(empty.postconditions.as_slice());

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                assert_slice_refinement_composition_laws(spec.preconditions.as_slice());
                assert_slice_refinement_composition_laws(spec.postconditions.as_slice());
            }
        }

        for populated in ConditionKind::ALL {
            let mut spec = empty_ephemeral();
            spec.preconditions.push(cond(populated));
            spec.preconditions.push(cond(populated));
            spec.preconditions.push(cond(populated));
            spec.postconditions.push(cond(populated));
            spec.postconditions.push(cond(populated));
            assert_slice_refinement_composition_laws(spec.preconditions.as_slice());
            assert_slice_refinement_composition_laws(spec.postconditions.as_slice());
        }
    }

    // ── assert_surface_union_composition_laws — ephemeral surface ────
    //
    // The substrate testkit macro
    // [`crate::assert_surface_union_composition_laws`] pins the FOUR
    // union composition laws (has: OR, find: or_else, iter: chain,
    // count: SUM) that bind the (pre, post, union) refinement triads
    // on the [`EphemeralSpec`] sugar-surface at ONE call site per
    // authored arrangement, sweeping [`ConditionKind::ALL`]. Byte-for-
    // byte peer of the point-surface
    // `boundary_surface_union_composition_laws_hold_across_authored_arrangements`
    // / `boundary_surface_union_composition_laws_hold_on_interleaved_duplicates`
    // pins on the [`crate::boundary::Boundary`] surface — the two-
    // surface parity contract binds every downstream `condition-<K>`
    // / `precondition-<K>` / `postcondition-<K>` require-tag classifier
    // on either surface to the SAME four union-composition operators
    // through ONE substrate primitive rather than through per-surface
    // author-time re-authored sweeps.

    /// SUBSTRATE PANEL pin (ephemeral surface) — the substrate macro
    /// [`crate::assert_surface_union_composition_laws`] passes on
    /// [`EphemeralSpec`] for the four canonical authored arrangements
    /// (empty spec, precondition-only populated, postcondition-only
    /// populated, dual-populated sweep over `ALL × ALL`). Byte-for-byte
    /// peer of the point-surface
    /// `boundary_surface_union_composition_laws_hold_across_authored_arrangements`
    /// pin — the two-surface parity contract binds every union
    /// composition law on both surfaces to the SAME substrate
    /// primitive.
    #[test]
    fn ephemeral_surface_union_composition_laws_hold_across_authored_arrangements() {
        let empty = empty_ephemeral();
        crate::assert_surface_union_composition_laws!(empty);

        for populated in ConditionKind::ALL {
            let mut pre_only = empty_ephemeral();
            pre_only.preconditions.push(cond(populated));
            crate::assert_surface_union_composition_laws!(pre_only);

            let mut post_only = empty_ephemeral();
            post_only.postconditions.push(cond(populated));
            crate::assert_surface_union_composition_laws!(post_only);
        }

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut dual = empty_ephemeral();
                dual.preconditions.push(cond(pre_kind));
                dual.postconditions.push(cond(post_kind));
                crate::assert_surface_union_composition_laws!(dual);
            }
        }
    }

    /// SUBSTRATE PANEL pin (ephemeral surface, params-distinguishable
    /// duplicates) — the substrate macro holds on an [`EphemeralSpec`]
    /// whose two half-slices each carry duplicates of the same kind at
    /// multiple positions interleaved with a distinct kind. Byte-for-
    /// byte peer of the point-surface
    /// `boundary_surface_union_composition_laws_hold_on_interleaved_duplicates`
    /// pin — the non-degenerate composition of every union arm on the
    /// sugar-surface binds against the SAME four monoid operators as
    /// the point-surface peer. A regression on the ephemeral surface
    /// only that (a) collapsed `find`'s `or_else` to `and_then`, (b)
    /// collapsed `iter`'s `chain` to `zip`, or (c) collapsed `count`'s
    /// SUM to `max` surfaces HERE, breaking two-surface parity.
    #[test]
    fn ephemeral_surface_union_composition_laws_hold_on_interleaved_duplicates() {
        let mut spec = empty_ephemeral();
        spec.preconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: serde_json::json!({ "side": "pre-1" }),
        });
        spec.preconditions.push(Condition {
            kind: ConditionKind::PromQL,
            params: serde_json::json!({ "query": "up" }),
        });
        spec.preconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: serde_json::json!({ "side": "pre-2" }),
        });
        spec.postconditions.push(Condition {
            kind: ConditionKind::PromQL,
            params: serde_json::json!({ "query": "healthy" }),
        });
        spec.postconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: serde_json::json!({ "side": "post-1" }),
        });
        crate::assert_surface_union_composition_laws!(spec);
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
            let classification = Classification::gate_compute_with_axis(populated);
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
            let classification = Classification::gate_compute_with_axis(populated);
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
            let classification = Classification::gate_compute_with_axis(populated);
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
            let classification = Classification::gate_compute_with_axis(populated);
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
            let classification = Classification::gate_compute_with_axis(populated);
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
            let classification = Classification::gate_compute_with_axis(populated);
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
            let classification = Classification::gate_compute_with_axis(populated);
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
            let classification = Classification::gate_compute_with_axis(populated);
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

    // ── EphemeralSpec::data_is_public pins ───────────────────────────
    //
    // Fail-before-pass-after granularity: `data_is_public` did not
    // exist pre-lift on `impl EphemeralSpec` — every consumer walking
    // the "is this ephemeral spec's dataset publicly distributable?"
    // question went through the antisymmetric
    // `!self.data_is_restricted()` or through
    // `.resolved_classification().data_classification.is_public()`.
    // Post-lift the THIRTEENTH derived-nullary-boolean peer on the
    // ephemeral surface (THIRD on the data axis, closing that axis
    // into a binary XOR partition on this surface) routes through the
    // SAME [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::data_is_public`], so
    // the two-surface parity contract holds by construction, AND the
    // two-way public/restricted split on this surface CLOSES the
    // data axis into the FULL binary XOR partition contract via
    // `ephemeral_data_probes_form_binary_xor_partition_over_all`.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries a specific
    /// [`crate::classification::DataClassification`] variant answers
    /// [`Self::data_is_public`] matching the closed set's own
    /// [`crate::classification::DataClassification::is_public`] truth
    /// table. Sweep
    /// [`crate::classification::DataClassification::ALL`] so a
    /// regression that (a) hard-coded the body to a fixed answer,
    /// (b) inverted the projection, or (c) crossed the wires with
    /// the sibling
    /// [`crate::classification::DataClassification::is_restricted`]
    /// projection fails HERE at the substrate primitive before
    /// drifting through the `public-data` fixed tag or the peer
    /// point surface.
    #[test]
    fn data_is_public_returns_data_projection_per_kind() {
        for populated in DataClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.data_classification = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.data_is_public(),
                populated.is_public(),
                "authored data_classification={populated:?}: data_is_public() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// [`crate::classification::DataClassification::default = Internal`]
    /// via `#[default]`, and
    /// [`crate::classification::DataClassification::Internal::is_public`]
    /// projects `false`, so [`Self::data_is_public`] returns `false`.
    /// Pins the resolver's default-arm short-circuit through TWO
    /// layers of `Default` ([`Classification::gate_compute`] →
    /// [`crate::classification::DataClassification::default`])
    /// reaching this derived-nullary predicate. Mirror-inverted from
    /// the sibling
    /// `data_is_restricted_probes_true_on_absent_classification`
    /// (both walk the SAME defaulted `data_classification` field, so
    /// `is_restricted = true` ⇒ `is_public = false` on the closed
    /// set's disjoint XOR partition). Guarantees every unadorned
    /// `(defephemeral …)` audits under the access-controlled default
    /// rather than silently promoting an unadorned dataset onto the
    /// freely-distributable path.
    #[test]
    fn data_is_public_probes_false_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            !spec.data_is_public(),
            "absent classification (defaults to gate_compute, data_classification=Internal → is_public=false)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::data_is_public`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.data_is_public()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`crate::classification::DataClassification::ALL`] variant) so
    /// a future regression on either side of the resolver fails HERE
    /// at the parity boundary. Byte-for-byte peer of
    /// `data_is_restricted_matches_point_peer_through_lowered_classification`
    /// on the SAME closed-set axis via the antisymmetric projection.
    #[test]
    fn data_is_public_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.data_is_public(),
            lowered.classification.data_is_public(),
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
                eph.data_is_public(),
                lowered.classification.data_is_public(),
                "authored data_classification={populated:?}: parity drift",
            );
        }
    }

    /// MUTEX pin — [`Self::data_is_regulated`] AND
    /// [`Self::data_is_public`] are NEVER simultaneously true for ANY
    /// [`EphemeralSpec`] (authored or defaulted). FIRST ephemeral-
    /// surface data-axis antisymmetric MUTEX pin against the
    /// positive-distribution framing: sealed on the closed set by
    /// `data_classification_regulated_implies_not_public` and lifted
    /// through the resolver hop as a substrate-wide contract on this
    /// surface.
    #[test]
    fn ephemeral_data_is_regulated_and_data_is_public_are_mutex_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        assert!(
            !(eph.data_is_regulated() && eph.data_is_public()),
            "None-classification: data_is_regulated AND data_is_public both true (mutex violated)",
        );
        // Authored classification.
        for populated in DataClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.data_classification = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            assert!(
                !(eph.data_is_regulated() && eph.data_is_public()),
                "authored data_classification={populated:?}: data_is_regulated AND data_is_public both true (mutex violated)",
            );
        }
    }

    /// BINARY XOR PARTITION pin — for the absent-classification
    /// baseline AND every
    /// [`crate::classification::DataClassification::ALL`] variant,
    /// EXACTLY ONE of [`Self::data_is_public`] and
    /// [`Self::data_is_restricted`] returns `true`. CLOSES the data-
    /// axis MUTEX pin (`data_is_regulated ⇒ ¬data_is_public`) into
    /// the FULL binary XOR partition contract on the ephemeral
    /// surface — the resolver-hop peer of the parent-composed
    /// `classification_data_probes_form_binary_xor_partition_over_all`
    /// test. Binary counterpart of the ternary XOR partitions sealed
    /// on the sibling `point_type` and `substrate` axes by
    /// `ephemeral_point_type_probes_form_three_way_xor_partition_over_all`
    /// and
    /// `ephemeral_substrate_probes_form_three_way_xor_partition_over_all`.
    /// Guarantees the absent-classification case lands in the
    /// access-controlled bucket (`gate_compute` →
    /// DataClassification::Internal → is_public = false,
    /// is_restricted = true), so every unadorned `(defephemeral …)`
    /// audits under a definite non-empty distribution bucket.
    #[test]
    fn ephemeral_data_probes_form_binary_xor_partition_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        let buckets = [eph.data_is_public(), eph.data_is_restricted()];
        let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
        assert_eq!(
            hits, 1,
            "None-classification: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
        );
        // Authored classification.
        for populated in DataClassification::ALL {
            let mut classification = Classification::gate_compute();
            classification.data_classification = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let buckets = [eph.data_is_public(), eph.data_is_restricted()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "authored data_classification={populated:?}: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
            );
        }
    }

    // ── EphemeralSpec::direction_prefers_lower pins ─────────────────
    //
    // Fail-before-pass-after granularity: `direction_prefers_lower`
    // did not exist pre-lift on `impl EphemeralSpec` — every consumer
    // walking the "does this ephemeral spec's rate-window evaluator
    // treat decreasing values as improvement?" question went through
    // `.resolved_classification().horizon.direction.unwrap_or_default().prefers_lower()`.
    // Post-lift the FOURTEENTH derived-nullary-boolean peer on the
    // ephemeral surface (FIRST on the optimization-direction axis,
    // opening the SIXTH classification axis into the fixed-tag algebra)
    // routes through the SAME [`Self::resolved_classification`] resolver
    // + the sibling substrate primitive
    // [`crate::classification::Classification::direction_prefers_lower`],
    // so the two-surface parity contract holds by construction.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries `Some(variant)` on `horizon.direction`
    /// answers [`Self::direction_prefers_lower`] matching the closed
    /// set's own
    /// [`crate::classification::OptimizationDirection::prefers_lower`]
    /// truth table. Sweep
    /// [`crate::classification::OptimizationDirection::ALL`] so a
    /// regression that (a) hard-coded the body to a fixed answer,
    /// (b) inverted the projection, (c) dropped the `.unwrap_or_default()`
    /// hop, or (d) crossed the wires with a sibling classification-axis
    /// probe fails HERE at the substrate primitive before drifting
    /// through the `prefers-lower-direction` fixed tag or the peer
    /// point surface.
    #[test]
    fn direction_prefers_lower_returns_direction_projection_per_kind() {
        for populated in OptimizationDirection::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon.direction = Some(populated);
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.direction_prefers_lower(),
                populated.prefers_lower(),
                "authored horizon.direction={populated:?}: direction_prefers_lower() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// `horizon: Horizon::default()` whose `direction` field is `None`,
    /// so `unwrap_or_default()` defaults to
    /// [`crate::classification::OptimizationDirection::Minimize`] via
    /// `#[default]`, and `Minimize.prefers_lower()` projects `true`,
    /// so [`Self::direction_prefers_lower`] returns `true`. Pins the
    /// resolver's default-arm short-circuit through THREE layers of
    /// `Default` ([`Classification::gate_compute`] →
    /// [`crate::classification::Horizon::default`] with `direction: None`
    /// → [`crate::classification::OptimizationDirection::default =
    /// Minimize`]) reaching this derived-nullary predicate. Guarantees
    /// every unadorned `(defephemeral …)` reads under the lower-is-
    /// better polarity default (safe under the asymptotic-health
    /// rate-window evaluator convention: an operator must deliberately
    /// opt into Maximize polarity).
    #[test]
    fn direction_prefers_lower_probes_true_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            spec.direction_prefers_lower(),
            "absent classification (defaults to gate_compute, horizon.direction=None → unwrap_or_default=Minimize → prefers_lower=true)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::direction_prefers_lower`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.direction_prefers_lower()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`crate::classification::OptimizationDirection::ALL`] variant) so
    /// a future regression on either side of the resolver fails HERE
    /// at the parity boundary. Byte-for-byte peer of
    /// `calm_is_monotone_matches_point_peer_through_lowered_classification`
    /// on the analog closed-set axis via the same resolver-hop shape.
    #[test]
    fn direction_prefers_lower_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.direction_prefers_lower(),
            lowered.classification.direction_prefers_lower(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in OptimizationDirection::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon.direction = Some(populated);
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.direction_prefers_lower(),
                lowered.classification.direction_prefers_lower(),
                "authored horizon.direction={populated:?}: parity drift",
            );
        }
    }

    // ── EphemeralSpec::direction_prefers_higher pins ────────────────
    //
    // Fail-before-pass-after granularity: `direction_prefers_higher`
    // did not exist pre-lift on `impl EphemeralSpec` — the positive
    // higher-is-better framing peer of
    // [`Self::direction_prefers_lower`] had no ephemeral-surface
    // substrate owner. Post-lift the FIFTEENTH derived-nullary-boolean
    // peer on the ephemeral surface (SECOND on the optimization-
    // direction axis, CLOSING the SIXTH classification axis into a
    // binary XOR partition on this surface) routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::direction_prefers_higher`],
    // so the two-surface parity contract holds by construction, AND
    // the two-way lower/higher split on this surface CLOSES the
    // optimization-direction axis into the FULL binary XOR partition
    // contract via
    // `ephemeral_direction_probes_form_binary_xor_partition_over_all`.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries `Some(variant)` on `horizon.direction`
    /// answers [`Self::direction_prefers_higher`] matching the closed
    /// set's own
    /// [`crate::classification::OptimizationDirection::prefers_higher`]
    /// truth table. Sweep
    /// [`crate::classification::OptimizationDirection::ALL`] so a
    /// regression that (a) hard-coded the body to a fixed answer,
    /// (b) inverted the projection, (c) dropped the `.unwrap_or_default()`
    /// hop, or (d) crossed the wires with a sibling classification-
    /// axis probe fails HERE at the substrate primitive before
    /// drifting through the `prefers-higher-direction` fixed tag or
    /// the peer point surface.
    #[test]
    fn direction_prefers_higher_returns_direction_projection_per_kind() {
        for populated in OptimizationDirection::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon.direction = Some(populated);
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.direction_prefers_higher(),
                populated.prefers_higher(),
                "authored horizon.direction={populated:?}: direction_prefers_higher() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries
    /// `horizon: Horizon::default()` whose `direction` field is `None`,
    /// so `unwrap_or_default()` defaults to
    /// [`crate::classification::OptimizationDirection::Minimize`] via
    /// `#[default]`, and `Minimize.prefers_higher()` projects `false`,
    /// so [`Self::direction_prefers_higher`] returns `false`. Pins
    /// the resolver's default-arm short-circuit through THREE layers
    /// of `Default` ([`Classification::gate_compute`] →
    /// [`crate::classification::Horizon::default`] with `direction:
    /// None` → [`crate::classification::OptimizationDirection::default =
    /// Minimize`]) reaching this derived-nullary predicate. Guarantees
    /// every unadorned `(defephemeral …)` reads UNDER the lower-is-
    /// better polarity default (safe under the asymptotic-health
    /// rate-window evaluator convention: an operator must
    /// deliberately opt into Maximize polarity). Mirror-inverted from
    /// the sibling `direction_prefers_lower_probes_true_on_absent_classification`
    /// baseline on the same resolver walk.
    #[test]
    fn direction_prefers_higher_probes_false_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            !spec.direction_prefers_higher(),
            "absent classification (defaults to gate_compute, horizon.direction=None → unwrap_or_default=Minimize → prefers_higher=false)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::direction_prefers_higher`] AND
    /// through
    /// `<eph.clone().into::<ProcessSpec>>().classification.direction_prefers_higher()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`crate::classification::OptimizationDirection::ALL`] variant)
    /// so a future regression on either side of the resolver fails
    /// HERE at the parity boundary. Byte-for-byte peer of
    /// `direction_prefers_lower_matches_point_peer_through_lowered_classification`
    /// on the antisymmetric closed-set arm via the same resolver-hop
    /// shape.
    #[test]
    fn direction_prefers_higher_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.direction_prefers_higher(),
            lowered.classification.direction_prefers_higher(),
            "None-classification parity drift",
        );
        // Authored classification.
        for populated in OptimizationDirection::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon.direction = Some(populated);
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let lowered: ProcessSpec = eph.clone().into();
            assert_eq!(
                eph.direction_prefers_higher(),
                lowered.classification.direction_prefers_higher(),
                "authored horizon.direction={populated:?}: parity drift",
            );
        }
    }

    /// BINARY XOR PARTITION pin — for the absent-classification
    /// baseline AND every
    /// [`crate::classification::OptimizationDirection::ALL`] variant,
    /// EXACTLY ONE of [`Self::direction_prefers_lower`] and
    /// [`Self::direction_prefers_higher`] returns `true`. CLOSES the
    /// optimization-direction axis into the FULL binary XOR partition
    /// contract on the ephemeral surface — the resolver-hop peer of
    /// the parent-composed
    /// `classification_direction_probes_form_binary_xor_partition_over_all`
    /// test. Binary counterpart of the ternary XOR partitions sealed
    /// on the sibling `point_type` and `substrate` axes by
    /// `ephemeral_point_type_probes_form_three_way_xor_partition_over_all`
    /// and
    /// `ephemeral_substrate_probes_form_three_way_xor_partition_over_all`,
    /// structural twin of the calm/data binary partitions
    /// `ephemeral_calm_probes_form_binary_xor_partition_over_all` and
    /// `ephemeral_data_probes_form_binary_xor_partition_over_all`.
    /// This pin is the SIXTH (and final) classification axis to reach
    /// the closed XOR partition landmark on the ephemeral resolver-
    /// hop surface — ALL SIX classification axes (horizon, calm,
    /// data, point, substrate, optimization-direction) now have
    /// their partitions closed on the ephemeral surface at this
    /// corner. Guarantees the absent-classification case lands in
    /// the definite lower-is-better bucket (`gate_compute` →
    /// Horizon::default → direction: None →
    /// OptimizationDirection::default = Minimize → prefers_lower =
    /// true, prefers_higher = false), so every unadorned
    /// `(defephemeral …)` audits under a definite non-empty polarity
    /// bucket.
    #[test]
    fn ephemeral_direction_probes_form_binary_xor_partition_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        let buckets = [
            eph.direction_prefers_lower(),
            eph.direction_prefers_higher(),
        ];
        let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
        assert_eq!(
            hits, 1,
            "None-classification: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
        );
        // Authored classification.
        for populated in OptimizationDirection::ALL {
            let mut classification = Classification::gate_compute();
            classification.horizon.direction = Some(populated);
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let buckets = [
                eph.direction_prefers_lower(),
                eph.direction_prefers_higher(),
            ];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "authored horizon.direction={populated:?}: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
            );
        }
    }

    // ── EphemeralSpec::input_arity_is_one pins ──────────────────────
    //
    // Fail-before-pass-after granularity: `input_arity_is_one` did not
    // exist pre-lift on `impl EphemeralSpec` — every consumer walking
    // the "does this ephemeral spec's DAG-composition input port
    // accept a single upstream edge?" question went through
    // `.resolved_classification().point_type.input_arity().is_one()`.
    // Post-lift the SIXTEENTH derived-nullary-boolean peer on the
    // ephemeral surface (FIRST on the input-arity axis, opening the
    // SEVENTH classification axis into the fixed-tag algebra + the
    // derived-typed-projection stratum on this surface for the first
    // time) routes through the SAME [`Self::resolved_classification`]
    // resolver + the sibling substrate primitive
    // [`crate::classification::Classification::input_arity_is_one`],
    // so the two-surface parity contract holds by construction.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries `point_type: kind` answers
    /// [`Self::input_arity_is_one`] matching the closed set's own
    /// [`crate::classification::ConvergencePointType::input_arity`]
    /// truth table projected through [`Arity::is_one`]. Sweep
    /// [`crate::classification::ConvergencePointType::ALL`] so a
    /// regression that (a) hard-coded the body to a fixed answer,
    /// (b) inverted the projection, (c) dropped the resolver hop, or
    /// (d) crossed the wires with the sibling `output_arity`
    /// projection (which disagrees on six of eight variants) fails
    /// HERE at the substrate primitive before drifting through the
    /// future `single-input-arity` fixed tag or the peer point
    /// surface.
    #[test]
    fn input_arity_is_one_returns_input_arity_projection_per_kind() {
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.input_arity_is_one(),
                populated.input_arity().is_one(),
                "authored point_type={populated:?}: input_arity_is_one() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries `point_type:
    /// Gate` and `Gate.input_arity() = Many`, so
    /// [`Self::input_arity_is_one`] returns `false`. Pins the
    /// resolver's default-arm short-circuit reaching this derived-
    /// nullary predicate — every unadorned `(defephemeral …)` lands
    /// in the multi-input bucket under the substrate default. Mirror-
    /// inverted from the sibling `input_arity_is_many` baseline on
    /// the same resolver walk (the XOR partition forces exactly one
    /// bucket per baseline).
    #[test]
    fn input_arity_is_one_probes_false_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            !spec.input_arity_is_one(),
            "absent classification (defaults to gate_compute, point_type=Gate → input_arity=Many → is_one=false)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::input_arity_is_one`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.input_arity_is_one()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`crate::classification::ConvergencePointType::ALL`] variant)
    /// so a future regression on either side of the resolver fails
    /// HERE at the parity boundary. Byte-for-byte peer of
    /// `direction_prefers_lower_matches_point_peer_through_lowered_classification`
    /// on the same resolver-hop shape.
    #[test]
    fn input_arity_is_one_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.input_arity_is_one(),
            lowered.classification.input_arity_is_one(),
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
                eph.input_arity_is_one(),
                lowered.classification.input_arity_is_one(),
                "authored point_type={populated:?}: parity drift",
            );
        }
    }

    // ── EphemeralSpec::input_arity_is_many pins ─────────────────────
    //
    // Fail-before-pass-after granularity: `input_arity_is_many` did
    // not exist pre-lift on `impl EphemeralSpec` — the multi-input
    // framing peer of [`Self::input_arity_is_one`] had no ephemeral-
    // surface substrate owner. Post-lift the SEVENTEENTH derived-
    // nullary-boolean peer on the ephemeral surface (SECOND on the
    // input-arity axis, CLOSING the SEVENTH classification axis into
    // a binary XOR partition on this surface) routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::input_arity_is_many`],
    // so the two-surface parity contract holds by construction, AND
    // the two-way single/many split on this surface CLOSES the
    // input-arity axis into the FULL binary XOR partition contract
    // via `ephemeral_input_arity_probes_form_binary_xor_partition_over_all`.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries `point_type: kind` answers
    /// [`Self::input_arity_is_many`] matching the closed set's own
    /// [`crate::classification::ConvergencePointType::input_arity`]
    /// truth table projected through [`Arity::is_many`]. Sweep
    /// [`crate::classification::ConvergencePointType::ALL`] so a
    /// regression that (a) hard-coded the body to a fixed answer,
    /// (b) inverted the projection, (c) dropped the resolver hop, or
    /// (d) crossed the wires with the sibling `output_arity`
    /// projection fails HERE at the substrate primitive before
    /// drifting through the future `multi-input-arity` fixed tag or
    /// the peer point surface.
    #[test]
    fn input_arity_is_many_returns_input_arity_projection_per_kind() {
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.input_arity_is_many(),
                populated.input_arity().is_many(),
                "authored point_type={populated:?}: input_arity_is_many() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries `point_type:
    /// Gate` and `Gate.input_arity() = Many`, so
    /// [`Self::input_arity_is_many`] returns `true`. Pins the
    /// resolver's default-arm short-circuit reaching this derived-
    /// nullary predicate — every unadorned `(defephemeral …)` lands
    /// in the multi-input bucket under the substrate default. Mirror-
    /// inverted from the sibling `input_arity_is_one` baseline on
    /// the same resolver walk (the XOR partition forces exactly one
    /// bucket per baseline).
    #[test]
    fn input_arity_is_many_probes_true_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            spec.input_arity_is_many(),
            "absent classification (defaults to gate_compute, point_type=Gate → input_arity=Many → is_many=true)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::input_arity_is_many`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.input_arity_is_many()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`crate::classification::ConvergencePointType::ALL`] variant)
    /// so a future regression on either side of the resolver fails
    /// HERE at the parity boundary. Byte-for-byte peer of
    /// `input_arity_is_one_matches_point_peer_through_lowered_classification`
    /// on the antisymmetric closed-set arm via the same resolver-hop
    /// shape.
    #[test]
    fn input_arity_is_many_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.input_arity_is_many(),
            lowered.classification.input_arity_is_many(),
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
                eph.input_arity_is_many(),
                lowered.classification.input_arity_is_many(),
                "authored point_type={populated:?}: parity drift",
            );
        }
    }

    /// BINARY XOR PARTITION pin — for the absent-classification
    /// baseline AND every
    /// [`crate::classification::ConvergencePointType::ALL`] variant,
    /// EXACTLY ONE of [`Self::input_arity_is_one`] and
    /// [`Self::input_arity_is_many`] returns `true`. CLOSES the
    /// input-arity axis into the FULL binary XOR partition contract
    /// on the ephemeral surface — the resolver-hop peer of the
    /// parent-composed
    /// `classification_input_arity_probes_form_binary_xor_partition_over_all`
    /// test. Binary counterpart of the ternary XOR partitions sealed
    /// on the sibling `point_type` and `substrate` axes by
    /// `ephemeral_point_type_probes_form_three_way_xor_partition_over_all`
    /// and
    /// `ephemeral_substrate_probes_form_three_way_xor_partition_over_all`,
    /// structural twin of the calm/data/direction binary partitions
    /// `ephemeral_calm_probes_form_binary_xor_partition_over_all`,
    /// `ephemeral_data_probes_form_binary_xor_partition_over_all`,
    /// and
    /// `ephemeral_direction_probes_form_binary_xor_partition_over_all`.
    /// This pin is the SEVENTH classification axis to reach the
    /// closed XOR partition landmark on the ephemeral resolver-hop
    /// surface — the FIRST closed axis on the derived-typed-
    /// projection stratum of this surface, opening the stratum beyond
    /// the six stored classification slots. Guarantees the absent-
    /// classification case lands in the definite multi-input bucket
    /// (`gate_compute` → point_type=Gate → input_arity=Many →
    /// is_one=false, is_many=true), so every unadorned
    /// `(defephemeral …)` audits under a definite non-empty input-
    /// arity bucket.
    #[test]
    fn ephemeral_input_arity_probes_form_binary_xor_partition_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        let buckets = [eph.input_arity_is_one(), eph.input_arity_is_many()];
        let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
        assert_eq!(
            hits, 1,
            "None-classification: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
        );
        // Authored classification.
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let buckets = [eph.input_arity_is_one(), eph.input_arity_is_many()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "authored point_type={populated:?}: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
            );
        }
    }

    // ── EphemeralSpec::output_arity_is_one pins ─────────────────────
    //
    // Fail-before-pass-after granularity: `output_arity_is_one` did not
    // exist pre-lift on `impl EphemeralSpec` — every consumer walking
    // the "does this ephemeral spec's DAG-composition output port emit
    // to a single downstream edge?" question went through
    // `.resolved_classification().point_type.output_arity().is_one()`.
    // Post-lift the EIGHTEENTH derived-nullary-boolean peer on the
    // ephemeral surface (FIRST on the output-arity axis, opening the
    // EIGHTH classification axis into the fixed-tag algebra + the
    // SECOND peer on the derived-typed-projection stratum after
    // [`Self::input_arity_is_one`]) routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::output_arity_is_one`],
    // so the two-surface parity contract holds by construction.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries `point_type: kind` answers
    /// [`Self::output_arity_is_one`] matching the closed set's own
    /// [`crate::classification::ConvergencePointType::output_arity`]
    /// truth table projected through [`Arity::is_one`]. Sweep
    /// [`crate::classification::ConvergencePointType::ALL`] so a
    /// regression that (a) hard-coded the body to a fixed answer,
    /// (b) inverted the projection, (c) dropped the resolver hop, or
    /// (d) crossed the wires with the sibling `input_arity`
    /// projection (which disagrees on six of eight variants) fails
    /// HERE at the substrate primitive before drifting through the
    /// future `single-output-arity` fixed tag or the peer point
    /// surface.
    #[test]
    fn output_arity_is_one_returns_output_arity_projection_per_kind() {
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.output_arity_is_one(),
                populated.output_arity().is_one(),
                "authored point_type={populated:?}: output_arity_is_one() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries `point_type:
    /// Gate` and `Gate.output_arity() = One`, so
    /// [`Self::output_arity_is_one`] returns `true`. Pins the
    /// resolver's default-arm short-circuit reaching this derived-
    /// nullary predicate — every unadorned `(defephemeral …)` lands
    /// in the single-output bucket under the substrate default.
    /// Mirror-inverted from the sibling `output_arity_is_many`
    /// baseline on the same resolver walk (the XOR partition forces
    /// exactly one bucket per baseline). Note the workspace-baseline
    /// answer FLIPS between the input-arity and output-arity axes on
    /// the exact same absent-classification baseline: the input-arity
    /// sibling `input_arity_is_one` answers `false`, but this
    /// output-arity peer answers `true` — direct evidence at the
    /// resolver-hop layer that the two axes carve the closed set
    /// into structurally different partitions.
    #[test]
    fn output_arity_is_one_probes_true_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            spec.output_arity_is_one(),
            "absent classification (defaults to gate_compute, point_type=Gate → output_arity=One → is_one=true)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::output_arity_is_one`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.output_arity_is_one()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`crate::classification::ConvergencePointType::ALL`] variant)
    /// so a future regression on either side of the resolver fails
    /// HERE at the parity boundary. Byte-for-byte peer of
    /// `input_arity_is_one_matches_point_peer_through_lowered_classification`
    /// on the sibling output-arity projection via the same
    /// resolver-hop shape.
    #[test]
    fn output_arity_is_one_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.output_arity_is_one(),
            lowered.classification.output_arity_is_one(),
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
                eph.output_arity_is_one(),
                lowered.classification.output_arity_is_one(),
                "authored point_type={populated:?}: parity drift",
            );
        }
    }

    // ── EphemeralSpec::output_arity_is_many pins ────────────────────
    //
    // Fail-before-pass-after granularity: `output_arity_is_many` did
    // not exist pre-lift on `impl EphemeralSpec` — the multi-output
    // framing peer of [`Self::output_arity_is_one`] had no ephemeral-
    // surface substrate owner. Post-lift the NINETEENTH derived-
    // nullary-boolean peer on the ephemeral surface (SECOND on the
    // output-arity axis, CLOSING the EIGHTH classification axis into
    // a binary XOR partition on this surface) routes through the SAME
    // [`Self::resolved_classification`] resolver + the sibling
    // substrate primitive
    // [`crate::classification::Classification::output_arity_is_many`],
    // so the two-surface parity contract holds by construction, AND
    // the two-way single/many split on this surface CLOSES the
    // output-arity axis into the FULL binary XOR partition contract
    // via `ephemeral_output_arity_probes_form_binary_xor_partition_over_all`,
    // completing the DAG-composition arity PAIR on the ephemeral
    // derived-typed-projection stratum.

    /// PER-VARIANT pin — an [`EphemeralSpec`] whose authored
    /// [`Classification`] carries `point_type: kind` answers
    /// [`Self::output_arity_is_many`] matching the closed set's own
    /// [`crate::classification::ConvergencePointType::output_arity`]
    /// truth table projected through [`Arity::is_many`]. Sweep
    /// [`crate::classification::ConvergencePointType::ALL`] so a
    /// regression that (a) hard-coded the body to a fixed answer,
    /// (b) inverted the projection, (c) dropped the resolver hop, or
    /// (d) crossed the wires with the sibling `input_arity`
    /// projection fails HERE at the substrate primitive before
    /// drifting through the future `multi-output-arity` fixed tag or
    /// the peer point surface.
    #[test]
    fn output_arity_is_many_returns_output_arity_projection_per_kind() {
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut spec = empty_ephemeral();
            spec.classification = Some(classification);
            assert_eq!(
                spec.output_arity_is_many(),
                populated.output_arity().is_many(),
                "authored point_type={populated:?}: output_arity_is_many() drift",
            );
        }
    }

    /// ABSENT-CLASSIFICATION SHORT-CIRCUIT pin — an [`EphemeralSpec`]
    /// with `classification: None` routes through the
    /// [`Self::resolved_classification`] resolver's substrate default
    /// [`Classification::gate_compute`], which carries `point_type:
    /// Gate` and `Gate.output_arity() = One`, so
    /// [`Self::output_arity_is_many`] returns `false`. Pins the
    /// resolver's default-arm short-circuit reaching this derived-
    /// nullary predicate — every unadorned `(defephemeral …)` lands
    /// in the single-output bucket under the substrate default.
    /// Mirror-inverted from the sibling `output_arity_is_one`
    /// baseline on the same resolver walk (the XOR partition forces
    /// exactly one bucket per baseline).
    #[test]
    fn output_arity_is_many_probes_false_on_absent_classification() {
        let spec = empty_ephemeral();
        assert!(spec.classification.is_none());
        assert!(
            !spec.output_arity_is_many(),
            "absent classification (defaults to gate_compute, point_type=Gate → output_arity=One → is_many=false)",
        );
    }

    /// TWO-SURFACE PARITY pin — the SAME [`EphemeralSpec`] classifies
    /// identically through [`Self::output_arity_is_many`] AND through
    /// `<eph.clone().into::<ProcessSpec>>().classification.output_arity_is_many()`
    /// on the mechanically-lowered `ProcessSpec`. Sweeps (`None`
    /// classification, `Some(_)` classification on every
    /// [`crate::classification::ConvergencePointType::ALL`] variant)
    /// so a future regression on either side of the resolver fails
    /// HERE at the parity boundary. Byte-for-byte peer of
    /// `output_arity_is_one_matches_point_peer_through_lowered_classification`
    /// on the antisymmetric closed-set arm via the same resolver-hop
    /// shape.
    #[test]
    fn output_arity_is_many_matches_point_peer_through_lowered_classification() {
        // Absent classification.
        let eph = empty_ephemeral();
        let lowered: ProcessSpec = eph.clone().into();
        assert_eq!(
            eph.output_arity_is_many(),
            lowered.classification.output_arity_is_many(),
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
                eph.output_arity_is_many(),
                lowered.classification.output_arity_is_many(),
                "authored point_type={populated:?}: parity drift",
            );
        }
    }

    /// BINARY XOR PARTITION pin — for the absent-classification
    /// baseline AND every
    /// [`crate::classification::ConvergencePointType::ALL`] variant,
    /// EXACTLY ONE of [`Self::output_arity_is_one`] and
    /// [`Self::output_arity_is_many`] returns `true`. CLOSES the
    /// output-arity axis into the FULL binary XOR partition contract
    /// on the ephemeral surface — the resolver-hop peer of the
    /// parent-composed
    /// `classification_output_arity_probes_form_binary_xor_partition_over_all`
    /// test. Binary counterpart of the ternary XOR partitions sealed
    /// on the sibling `point_type` and `substrate` axes by
    /// `ephemeral_point_type_probes_form_three_way_xor_partition_over_all`
    /// and
    /// `ephemeral_substrate_probes_form_three_way_xor_partition_over_all`,
    /// structural twin of the calm/data/direction/input-arity binary
    /// partitions on this surface. This pin is the EIGHTH
    /// classification axis to reach the closed XOR partition landmark
    /// on the ephemeral resolver-hop surface — the SECOND closed axis
    /// on the derived-typed-projection stratum of this surface,
    /// completing the DAG-composition arity PAIR on the ephemeral
    /// stratum after the input-arity closure. Guarantees the absent-
    /// classification case lands in the definite single-output bucket
    /// (`gate_compute` → point_type=Gate → output_arity=One →
    /// is_one=true, is_many=false), so every unadorned
    /// `(defephemeral …)` audits under a definite non-empty
    /// output-arity bucket.
    #[test]
    fn ephemeral_output_arity_probes_form_binary_xor_partition_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        let buckets = [eph.output_arity_is_one(), eph.output_arity_is_many()];
        let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
        assert_eq!(
            hits, 1,
            "None-classification: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
        );
        // Authored classification.
        for populated in ConvergencePointType::ALL {
            let mut classification = Classification::gate_compute();
            classification.point_type = populated;
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let buckets = [eph.output_arity_is_one(), eph.output_arity_is_many()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "authored point_type={populated:?}: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
            );
        }
    }

    /// BINARY XOR PARTITION pin — for the absent-classification
    /// baseline AND every
    /// [`crate::classification::HorizonKind::ALL`] variant, EXACTLY
    /// ONE of [`Self::horizon_terminates`] and
    /// [`Self::horizon_requires_metric_axes`] returns `true`. CLOSES
    /// the horizon axis into the FULL binary XOR partition contract
    /// on the ephemeral surface — the resolver-hop peer of the
    /// parent-composed
    /// `classification_horizon_probes_form_binary_xor_partition_over_all`
    /// test. Binary counterpart of the ternary XOR partitions sealed
    /// on the sibling `point_type` and `substrate` axes by
    /// `ephemeral_point_type_probes_form_three_way_xor_partition_over_all`
    /// and
    /// `ephemeral_substrate_probes_form_three_way_xor_partition_over_all`,
    /// structural twin of the calm/data binary partitions
    /// `ephemeral_calm_probes_form_binary_xor_partition_over_all`
    /// and
    /// `ephemeral_data_probes_form_binary_xor_partition_over_all`.
    /// This pin is the FIFTH (and final) classification axis to reach
    /// the closed XOR partition landmark on the ephemeral resolver-
    /// hop surface, sealing every classification axis under the
    /// SAME `hits == 1` bucket-array contract. Guarantees the absent-
    /// classification case lands in the definite terminating bucket
    /// (`gate_compute` → HorizonKind::Bounded → terminates = true,
    /// requires_metric_axes = false), so every unadorned
    /// `(defephemeral …)` audits under a definite non-empty horizon
    /// bucket. Rewritten from the earlier binary-XOR-only form
    /// (walked as `a ^ b`) into the canonical bucket-array shape
    /// shared with the calm/data partitions.
    #[test]
    fn ephemeral_horizon_probes_form_binary_xor_partition_over_all() {
        // Absent classification.
        let eph = empty_ephemeral();
        let buckets = [eph.horizon_terminates(), eph.horizon_requires_metric_axes()];
        let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
        assert_eq!(
            hits, 1,
            "None-classification: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
        );
        // Authored classification.
        for populated in HorizonKind::ALL {
            let classification = Classification::gate_compute_with_axis(populated);
            let mut eph = empty_ephemeral();
            eph.classification = Some(classification);
            let buckets = [eph.horizon_terminates(), eph.horizon_requires_metric_axes()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "authored horizon.kind={populated:?}: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
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

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec saturation-predicate
    /// triad) — the three `is_*_kind_saturated` methods on
    /// [`EphemeralSpec`] delegate to the slice-level substrate primitive
    /// [`ConditionSliceExt::is_kind_saturated`] over the two
    /// `Vec<Condition>` slots (precondition + postcondition) and
    /// compose the union via `ConditionKind::ALL.iter().all(|k|
    /// has_condition_kind(*k))`. Two-surface parity pin against
    /// [`crate::boundary::Boundary::is_condition_kind_saturated`] on the
    /// point-domain [`ProcessSpec`] surface — the two struct-level
    /// saturation callers compose against the SAME slice-level
    /// substrate primitive so a regression at the per-slice `all`
    /// short-circuit fails at that primitive's tests rather than as
    /// silent drift at either sugar-surface arm.
    #[test]
    fn is_condition_kind_saturated_triad_delegates_to_slice_is_kind_saturated() {
        // Empty ephemeral spec — every arm returns false.
        let spec = empty_ephemeral();
        assert!(
            !spec.is_precondition_kind_saturated(),
            "empty ephemeral must return false on is_precondition_kind_saturated",
        );
        assert!(
            !spec.is_postcondition_kind_saturated(),
            "empty ephemeral must return false on is_postcondition_kind_saturated",
        );
        assert!(
            !spec.is_condition_kind_saturated(),
            "empty ephemeral must return false on is_condition_kind_saturated",
        );
        assert_eq!(
            spec.is_condition_kind_saturated(),
            spec.missing_condition_kinds().is_empty(),
            "empty is_condition_kind_saturated must equal missing_condition_kinds().is_empty()",
        );

        // Single-populated per side — sweep ALL × ALL.
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                assert_eq!(
                    spec.is_precondition_kind_saturated(),
                    spec.preconditions.is_kind_saturated(),
                    "EphemeralSpec::is_precondition_kind_saturated must delegate verbatim to \
                     preconditions.is_kind_saturated() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.is_postcondition_kind_saturated(),
                    spec.postconditions.is_kind_saturated(),
                    "EphemeralSpec::is_postcondition_kind_saturated must delegate verbatim to \
                     postconditions.is_kind_saturated() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union = ConditionKind::ALL
                    .iter()
                    .all(|k| pre_kind == *k || post_kind == *k);
                assert_eq!(
                    spec.is_condition_kind_saturated(),
                    expected_union,
                    "EphemeralSpec::is_condition_kind_saturated must equal all-ALL-covered-by-either-slice \
                     for pre={pre_kind:?} post={post_kind:?}",
                );

                // Two-surface parity: lowered ProcessSpec's Boundary
                // must agree bit-for-bit with the ephemeral sugar
                // triad on every arm.
                let lowered: ProcessSpec = spec.clone().into();
                assert_eq!(
                    spec.is_precondition_kind_saturated(),
                    lowered.boundary.is_precondition_kind_saturated(),
                    "two-surface is_precondition_kind_saturated parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.is_postcondition_kind_saturated(),
                    lowered.boundary.is_postcondition_kind_saturated(),
                    "two-surface is_postcondition_kind_saturated parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.is_condition_kind_saturated(),
                    lowered.boundary.is_condition_kind_saturated(),
                    "two-surface is_condition_kind_saturated parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }

        // Saturated ephemeral — both slices carry every ConditionKind,
        // every arm returns true.
        let mut spec = empty_ephemeral();
        for k in ConditionKind::ALL {
            spec.preconditions.push(cond(k));
            spec.postconditions.push(cond(k));
        }
        assert!(
            spec.is_precondition_kind_saturated(),
            "saturated ephemeral must return true on is_precondition_kind_saturated",
        );
        assert!(
            spec.is_postcondition_kind_saturated(),
            "saturated ephemeral must return true on is_postcondition_kind_saturated",
        );
        assert!(
            spec.is_condition_kind_saturated(),
            "saturated ephemeral must return true on is_condition_kind_saturated",
        );
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec at-least-one halfspace
    /// triad) — the three `has_any_missing_*_condition_kind` methods
    /// on [`EphemeralSpec`] delegate to the slice-level substrate
    /// primitive
    /// [`crate::boundary::ConditionSliceExt::has_any_missing_kind`]
    /// over the two `Vec<Condition>` slots (precondition +
    /// postcondition) and compose the union via
    /// `!self.is_condition_kind_saturated()`. Two-surface parity pin
    /// against
    /// [`crate::boundary::Boundary::has_any_missing_condition_kind`] on
    /// the point-domain [`ProcessSpec`] surface — the two struct-level
    /// at-least-one halfspace callers compose against the SAME slice-
    /// level substrate primitive so a regression at the per-slice
    /// `all` short-circuit under negation fails at that primitive's
    /// tests rather than as silent drift at either sugar-surface arm.
    #[test]
    fn has_any_missing_condition_kind_triad_delegates_to_slice_has_any_missing_kind() {
        // Empty ephemeral spec — every arm returns true (every kind is
        // missing from every slice + from the union).
        let spec = empty_ephemeral();
        assert!(
            spec.has_any_missing_precondition_kind(),
            "empty ephemeral must return true on has_any_missing_precondition_kind",
        );
        assert!(
            spec.has_any_missing_postcondition_kind(),
            "empty ephemeral must return true on has_any_missing_postcondition_kind",
        );
        assert!(
            spec.has_any_missing_condition_kind(),
            "empty ephemeral must return true on has_any_missing_condition_kind",
        );
        assert_eq!(
            spec.has_any_missing_condition_kind(),
            !spec.is_condition_kind_saturated(),
            "empty has_any_missing_condition_kind must equal !is_condition_kind_saturated()",
        );

        // Single-populated per side — sweep ALL × ALL, then pin the
        // (pre, post, union) triad + two-surface parity against the
        // lowered ProcessSpec's Boundary.
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                assert_eq!(
                    spec.has_any_missing_precondition_kind(),
                    spec.preconditions.has_any_missing_kind(),
                    "EphemeralSpec::has_any_missing_precondition_kind must delegate verbatim to \
                     preconditions.has_any_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_any_missing_postcondition_kind(),
                    spec.postconditions.has_any_missing_kind(),
                    "EphemeralSpec::has_any_missing_postcondition_kind must delegate verbatim to \
                     postconditions.has_any_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union = !ConditionKind::ALL
                    .iter()
                    .all(|k| pre_kind == *k || post_kind == *k);
                assert_eq!(
                    spec.has_any_missing_condition_kind(),
                    expected_union,
                    "EphemeralSpec::has_any_missing_condition_kind must equal \
                     !all-ALL-covered-by-either-slice \
                     for pre={pre_kind:?} post={post_kind:?}",
                );

                // Two-surface parity: lowered ProcessSpec's Boundary
                // must agree bit-for-bit with the ephemeral sugar
                // triad on every arm.
                let lowered: ProcessSpec = spec.clone().into();
                assert_eq!(
                    spec.has_any_missing_precondition_kind(),
                    lowered.boundary.has_any_missing_precondition_kind(),
                    "two-surface has_any_missing_precondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_any_missing_postcondition_kind(),
                    lowered.boundary.has_any_missing_postcondition_kind(),
                    "two-surface has_any_missing_postcondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_any_missing_condition_kind(),
                    lowered.boundary.has_any_missing_condition_kind(),
                    "two-surface has_any_missing_condition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }

        // Saturated ephemeral — both slices carry every ConditionKind,
        // every arm returns false.
        let mut spec = empty_ephemeral();
        for k in ConditionKind::ALL {
            spec.preconditions.push(cond(k));
            spec.postconditions.push(cond(k));
        }
        assert!(
            !spec.has_any_missing_precondition_kind(),
            "saturated ephemeral must return false on has_any_missing_precondition_kind",
        );
        assert!(
            !spec.has_any_missing_postcondition_kind(),
            "saturated ephemeral must return false on has_any_missing_postcondition_kind",
        );
        assert!(
            !spec.has_any_missing_condition_kind(),
            "saturated ephemeral must return false on has_any_missing_condition_kind",
        );
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec at-least-one halfspace
    /// triad on the closed-set-inversion axis) — the three
    /// `has_any_distinct_*_condition_kind` methods on
    /// [`EphemeralSpec`] delegate to the slice-level substrate
    /// primitive
    /// [`crate::boundary::ConditionSliceExt::has_any_distinct_kind`]
    /// over the two `Vec<Condition>` slots (precondition +
    /// postcondition) and compose the union via a SHORT-CIRCUITING
    /// closed-set walk over [`ConditionKind::ALL`] under
    /// [`EphemeralSpec::has_condition_kind`]. Two-surface parity pin
    /// against
    /// [`crate::boundary::Boundary::has_any_distinct_condition_kind`]
    /// on the point-domain [`ProcessSpec`] surface — the two struct-
    /// level at-least-one halfspace callers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-
    /// slice `any` short-circuit fails at that primitive's tests
    /// rather than as silent drift at either sugar-surface arm.
    #[test]
    fn has_any_distinct_condition_kind_triad_delegates_to_slice_has_any_distinct_kind() {
        // Empty ephemeral spec — every arm returns false (no kind
        // present in either slice).
        let spec = empty_ephemeral();
        assert!(
            !spec.has_any_distinct_precondition_kind(),
            "empty ephemeral must return false on has_any_distinct_precondition_kind",
        );
        assert!(
            !spec.has_any_distinct_postcondition_kind(),
            "empty ephemeral must return false on has_any_distinct_postcondition_kind",
        );
        assert!(
            !spec.has_any_distinct_condition_kind(),
            "empty ephemeral must return false on has_any_distinct_condition_kind",
        );

        // Single-populated per side — sweep ALL × ALL, then pin the
        // (pre, post, union) triad + two-surface parity against the
        // lowered ProcessSpec's Boundary.
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                assert_eq!(
                    spec.has_any_distinct_precondition_kind(),
                    spec.preconditions.has_any_distinct_kind(),
                    "EphemeralSpec::has_any_distinct_precondition_kind must delegate verbatim to \
                     preconditions.has_any_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_any_distinct_postcondition_kind(),
                    spec.postconditions.has_any_distinct_kind(),
                    "EphemeralSpec::has_any_distinct_postcondition_kind must delegate verbatim to \
                     postconditions.has_any_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert!(
                    spec.has_any_distinct_precondition_kind(),
                    "single-populated preconditions must return true on has_any_distinct_precondition_kind for pre={pre_kind:?}",
                );
                assert!(
                    spec.has_any_distinct_postcondition_kind(),
                    "single-populated postconditions must return true on has_any_distinct_postcondition_kind for post={post_kind:?}",
                );
                assert!(
                    spec.has_any_distinct_condition_kind(),
                    "single-populated-per-side must return true on has_any_distinct_condition_kind for pre={pre_kind:?} post={post_kind:?}",
                );

                // Two-surface parity: lowered ProcessSpec's Boundary
                // must agree bit-for-bit with the ephemeral sugar
                // triad on every arm.
                let lowered: ProcessSpec = spec.clone().into();
                assert_eq!(
                    spec.has_any_distinct_precondition_kind(),
                    lowered.boundary.has_any_distinct_precondition_kind(),
                    "two-surface has_any_distinct_precondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_any_distinct_postcondition_kind(),
                    lowered.boundary.has_any_distinct_postcondition_kind(),
                    "two-surface has_any_distinct_postcondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_any_distinct_condition_kind(),
                    lowered.boundary.has_any_distinct_condition_kind(),
                    "two-surface has_any_distinct_condition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }

        // Single-populated precondition only — precondition arm true,
        // postcondition arm false, union true.
        for pre_kind in ConditionKind::ALL {
            let mut spec = empty_ephemeral();
            spec.preconditions.push(cond(pre_kind));
            assert!(
                spec.has_any_distinct_precondition_kind(),
                "pre-only ephemeral must return true on has_any_distinct_precondition_kind for pre={pre_kind:?}",
            );
            assert!(
                !spec.has_any_distinct_postcondition_kind(),
                "pre-only ephemeral must return false on has_any_distinct_postcondition_kind for pre={pre_kind:?}",
            );
            assert!(
                spec.has_any_distinct_condition_kind(),
                "pre-only ephemeral must return true on has_any_distinct_condition_kind for pre={pre_kind:?}",
            );
        }

        // Saturated ephemeral — both slices carry every ConditionKind,
        // every arm returns true.
        let mut spec = empty_ephemeral();
        for k in ConditionKind::ALL {
            spec.preconditions.push(cond(k));
            spec.postconditions.push(cond(k));
        }
        assert!(
            spec.has_any_distinct_precondition_kind(),
            "saturated ephemeral must return true on has_any_distinct_precondition_kind",
        );
        assert!(
            spec.has_any_distinct_postcondition_kind(),
            "saturated ephemeral must return true on has_any_distinct_postcondition_kind",
        );
        assert!(
            spec.has_any_distinct_condition_kind(),
            "saturated ephemeral must return true on has_any_distinct_condition_kind",
        );
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec singleton-coverage
    /// triad on the closed-set-inversion axis) — the three
    /// `has_unique_distinct_*_condition_kind` methods on
    /// [`EphemeralSpec`] delegate to the slice-level substrate
    /// primitive
    /// [`crate::boundary::ConditionSliceExt::has_unique_distinct_kind`]
    /// over the two `Vec<Condition>` slots (precondition +
    /// postcondition) and compose the union via a two-step-short-
    /// circuit walk over [`ConditionKind::ALL`] under
    /// [`EphemeralSpec::has_condition_kind`]. Two-surface parity pin
    /// against
    /// [`crate::boundary::Boundary::has_unique_distinct_condition_kind`]
    /// on the point-domain [`ProcessSpec`] surface — the two struct-
    /// level singleton-coverage callers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-
    /// slice two-step short-circuit walk fails at that primitive's
    /// tests rather than as silent drift at either sugar-surface arm.
    #[test]
    fn has_unique_distinct_condition_kind_triad_delegates_to_slice_has_unique_distinct_kind() {
        // Empty ephemeral spec — every arm returns false (0 distinct,
        // not exactly 1).
        let spec = empty_ephemeral();
        assert!(
            !spec.has_unique_distinct_precondition_kind(),
            "empty ephemeral must return false on has_unique_distinct_precondition_kind",
        );
        assert!(
            !spec.has_unique_distinct_postcondition_kind(),
            "empty ephemeral must return false on has_unique_distinct_postcondition_kind",
        );
        assert!(
            !spec.has_unique_distinct_condition_kind(),
            "empty ephemeral must return false on has_unique_distinct_condition_kind",
        );
        assert_eq!(
            spec.has_unique_distinct_condition_kind(),
            spec.distinct_condition_kind_count() == 1,
            "empty has_unique_distinct_condition_kind must equal (distinct_condition_kind_count() == 1)",
        );

        // Single-populated per side — sweep ALL × ALL. Every per-
        // slice arm returns true; the union returns true iff the two
        // populated kinds coincide (union covers exactly one kind).
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                assert_eq!(
                    spec.has_unique_distinct_precondition_kind(),
                    spec.preconditions.has_unique_distinct_kind(),
                    "EphemeralSpec::has_unique_distinct_precondition_kind must delegate verbatim to \
                     preconditions.has_unique_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_unique_distinct_postcondition_kind(),
                    spec.postconditions.has_unique_distinct_kind(),
                    "EphemeralSpec::has_unique_distinct_postcondition_kind must delegate verbatim to \
                     postconditions.has_unique_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                let covered_count = ConditionKind::ALL
                    .into_iter()
                    .filter(|k| *k == pre_kind || *k == post_kind)
                    .count();
                let expected_union = covered_count == 1;
                assert_eq!(
                    spec.has_unique_distinct_condition_kind(),
                    expected_union,
                    "EphemeralSpec::has_unique_distinct_condition_kind must equal \
                     (covered-ALL-count == 1) for pre={pre_kind:?} post={post_kind:?}",
                );

                // Two-surface parity: lowered ProcessSpec's Boundary
                // must agree bit-for-bit with the ephemeral sugar
                // triad on every arm.
                let lowered: ProcessSpec = spec.clone().into();
                assert_eq!(
                    spec.has_unique_distinct_precondition_kind(),
                    lowered.boundary.has_unique_distinct_precondition_kind(),
                    "two-surface has_unique_distinct_precondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_unique_distinct_postcondition_kind(),
                    lowered.boundary.has_unique_distinct_postcondition_kind(),
                    "two-surface has_unique_distinct_postcondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_unique_distinct_condition_kind(),
                    lowered.boundary.has_unique_distinct_condition_kind(),
                    "two-surface has_unique_distinct_condition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }

        // Saturated ephemeral — every arm returns false on N ≥ 2 (N
        // distinct, not exactly 1).
        if ConditionKind::ALL.len() >= 2 {
            let mut spec = empty_ephemeral();
            for k in ConditionKind::ALL {
                spec.preconditions.push(cond(k));
                spec.postconditions.push(cond(k));
            }
            assert!(
                !spec.has_unique_distinct_precondition_kind(),
                "saturated ephemeral must return false on has_unique_distinct_precondition_kind",
            );
            assert!(
                !spec.has_unique_distinct_postcondition_kind(),
                "saturated ephemeral must return false on has_unique_distinct_postcondition_kind",
            );
            assert!(
                !spec.has_unique_distinct_condition_kind(),
                "saturated ephemeral must return false on has_unique_distinct_condition_kind",
            );
        }
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec cardinality-many-arm
    /// triad on the closed-set-inversion axis) — the three
    /// `has_multiple_distinct_*_condition_kind` methods on
    /// [`EphemeralSpec`] delegate to the slice-level substrate
    /// primitive
    /// [`crate::boundary::ConditionSliceExt::has_multiple_distinct_kinds`]
    /// over the two `Vec<Condition>` slots (precondition +
    /// postcondition) and compose the union via a two-step-short-
    /// circuit walk over [`ConditionKind::ALL`] under
    /// [`EphemeralSpec::has_condition_kind`]. Two-surface parity pin
    /// against
    /// [`crate::boundary::Boundary::has_multiple_distinct_condition_kind`]
    /// on the point-domain [`ProcessSpec`] surface — the two struct-
    /// level many-distinct callers compose against the SAME slice-
    /// level substrate primitive so a regression at the per-slice
    /// two-step short-circuit walk fails at that primitive's tests
    /// rather than as silent drift at either sugar-surface arm.
    #[test]
    fn has_multiple_distinct_condition_kind_triad_delegates_to_slice_has_multiple_distinct_kinds() {
        // Empty ephemeral spec — every arm returns false (0 distinct,
        // not ≥ 2).
        let spec = empty_ephemeral();
        assert!(
            !spec.has_multiple_distinct_precondition_kind(),
            "empty ephemeral must return false on has_multiple_distinct_precondition_kind",
        );
        assert!(
            !spec.has_multiple_distinct_postcondition_kind(),
            "empty ephemeral must return false on has_multiple_distinct_postcondition_kind",
        );
        assert!(
            !spec.has_multiple_distinct_condition_kind(),
            "empty ephemeral must return false on has_multiple_distinct_condition_kind",
        );
        assert_eq!(
            spec.has_multiple_distinct_condition_kind(),
            spec.distinct_condition_kind_count() >= 2,
            "empty has_multiple_distinct_condition_kind must equal (distinct_condition_kind_count() >= 2)",
        );

        // Single-populated per side — sweep ALL × ALL. Every per-slice
        // arm returns false (1 distinct per slice, not ≥ 2); the
        // union returns true iff the two kinds DIFFER (union covers 2
        // distinct kinds).
        assert!(
            ConditionKind::ALL.len() >= 2,
            "test assumes ConditionKind::ALL has ≥ 2 variants",
        );
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                assert_eq!(
                    spec.has_multiple_distinct_precondition_kind(),
                    spec.preconditions.has_multiple_distinct_kinds(),
                    "EphemeralSpec::has_multiple_distinct_precondition_kind must delegate verbatim to \
                     preconditions.has_multiple_distinct_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_multiple_distinct_postcondition_kind(),
                    spec.postconditions.has_multiple_distinct_kinds(),
                    "EphemeralSpec::has_multiple_distinct_postcondition_kind must delegate verbatim to \
                     postconditions.has_multiple_distinct_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                let covered_count = ConditionKind::ALL
                    .into_iter()
                    .filter(|k| *k == pre_kind || *k == post_kind)
                    .count();
                let expected_union = covered_count >= 2;
                assert_eq!(
                    spec.has_multiple_distinct_condition_kind(),
                    expected_union,
                    "EphemeralSpec::has_multiple_distinct_condition_kind must equal \
                     (covered-ALL-count >= 2) for pre={pre_kind:?} post={post_kind:?}",
                );

                // Two-surface parity: lowered ProcessSpec's Boundary
                // must agree bit-for-bit with the ephemeral sugar
                // triad on every arm.
                let lowered: ProcessSpec = spec.clone().into();
                assert_eq!(
                    spec.has_multiple_distinct_precondition_kind(),
                    lowered.boundary.has_multiple_distinct_precondition_kind(),
                    "two-surface has_multiple_distinct_precondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_multiple_distinct_postcondition_kind(),
                    lowered.boundary.has_multiple_distinct_postcondition_kind(),
                    "two-surface has_multiple_distinct_postcondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_multiple_distinct_condition_kind(),
                    lowered.boundary.has_multiple_distinct_condition_kind(),
                    "two-surface has_multiple_distinct_condition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }

        // Saturated ephemeral — every arm returns true on N ≥ 2 (N
        // distinct, ≥ 2).
        let mut spec = empty_ephemeral();
        for k in ConditionKind::ALL {
            spec.preconditions.push(cond(k));
            spec.postconditions.push(cond(k));
        }
        assert!(
            spec.has_multiple_distinct_precondition_kind(),
            "saturated ephemeral must return true on has_multiple_distinct_precondition_kind",
        );
        assert!(
            spec.has_multiple_distinct_postcondition_kind(),
            "saturated ephemeral must return true on has_multiple_distinct_postcondition_kind",
        );
        assert!(
            spec.has_multiple_distinct_condition_kind(),
            "saturated ephemeral must return true on has_multiple_distinct_condition_kind",
        );
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec cardinality "≤ 1" triad
    /// on the closed-set-inversion axis) — the three
    /// `has_at_most_one_distinct_*_condition_kind` methods on
    /// [`EphemeralSpec`] delegate to the slice-level substrate
    /// primitive
    /// [`crate::boundary::ConditionSliceExt::has_at_most_one_distinct_kind`]
    /// over the two `Vec<Condition>` slots (precondition +
    /// postcondition) and compose the union via a definitional
    /// negation of the many-arm two-step-short-circuit walk over
    /// [`ConditionKind::ALL`] under
    /// [`EphemeralSpec::has_condition_kind`]. Two-surface parity pin
    /// against
    /// [`crate::boundary::Boundary::has_at_most_one_distinct_condition_kind`]
    /// on the point-domain [`ProcessSpec`] surface — the two struct-
    /// level empty-or-singleton callers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-
    /// slice "≤ 1" negation fails at that primitive's tests rather
    /// than as silent drift at either sugar-surface arm.
    #[test]
    fn has_at_most_one_distinct_condition_kind_triad_delegates_to_slice_has_at_most_one_distinct_kind(
    ) {
        // Empty ephemeral spec — every arm returns true (0 distinct,
        // ≤ 1).
        let spec = empty_ephemeral();
        assert!(
            spec.has_at_most_one_distinct_precondition_kind(),
            "empty ephemeral must return true on has_at_most_one_distinct_precondition_kind",
        );
        assert!(
            spec.has_at_most_one_distinct_postcondition_kind(),
            "empty ephemeral must return true on has_at_most_one_distinct_postcondition_kind",
        );
        assert!(
            spec.has_at_most_one_distinct_condition_kind(),
            "empty ephemeral must return true on has_at_most_one_distinct_condition_kind",
        );
        assert_eq!(
            spec.has_at_most_one_distinct_condition_kind(),
            spec.distinct_condition_kind_count() <= 1,
            "empty has_at_most_one_distinct_condition_kind must equal (distinct_condition_kind_count() <= 1)",
        );

        // Single-populated per side — sweep ALL × ALL. Every per-slice
        // arm returns true (1 distinct per slice, ≤ 1); the union
        // returns true iff the two kinds COINCIDE (union has 1
        // distinct), otherwise the union has 2 distinct and drops to
        // false.
        assert!(
            ConditionKind::ALL.len() >= 2,
            "test assumes ConditionKind::ALL has ≥ 2 variants",
        );
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                assert_eq!(
                    spec.has_at_most_one_distinct_precondition_kind(),
                    spec.preconditions.has_at_most_one_distinct_kind(),
                    "EphemeralSpec::has_at_most_one_distinct_precondition_kind must delegate verbatim to \
                     preconditions.has_at_most_one_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_at_most_one_distinct_postcondition_kind(),
                    spec.postconditions.has_at_most_one_distinct_kind(),
                    "EphemeralSpec::has_at_most_one_distinct_postcondition_kind must delegate verbatim to \
                     postconditions.has_at_most_one_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                let covered_count = ConditionKind::ALL
                    .into_iter()
                    .filter(|k| *k == pre_kind || *k == post_kind)
                    .count();
                let expected_union = covered_count <= 1;
                assert_eq!(
                    spec.has_at_most_one_distinct_condition_kind(),
                    expected_union,
                    "EphemeralSpec::has_at_most_one_distinct_condition_kind must equal \
                     (covered-ALL-count <= 1) for pre={pre_kind:?} post={post_kind:?}",
                );

                // Two-surface parity: lowered ProcessSpec's Boundary
                // must agree bit-for-bit with the ephemeral sugar
                // triad on every arm.
                let lowered: ProcessSpec = spec.clone().into();
                assert_eq!(
                    spec.has_at_most_one_distinct_precondition_kind(),
                    lowered.boundary.has_at_most_one_distinct_precondition_kind(),
                    "two-surface has_at_most_one_distinct_precondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_at_most_one_distinct_postcondition_kind(),
                    lowered.boundary.has_at_most_one_distinct_postcondition_kind(),
                    "two-surface has_at_most_one_distinct_postcondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    spec.has_at_most_one_distinct_condition_kind(),
                    lowered.boundary.has_at_most_one_distinct_condition_kind(),
                    "two-surface has_at_most_one_distinct_condition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }

        // Saturated ephemeral — every arm returns false on N ≥ 2 (N
        // distinct, not ≤ 1).
        let mut spec = empty_ephemeral();
        for k in ConditionKind::ALL {
            spec.preconditions.push(cond(k));
            spec.postconditions.push(cond(k));
        }
        assert!(
            !spec.has_at_most_one_distinct_precondition_kind(),
            "saturated ephemeral must return false on has_at_most_one_distinct_precondition_kind",
        );
        assert!(
            !spec.has_at_most_one_distinct_postcondition_kind(),
            "saturated ephemeral must return false on has_at_most_one_distinct_postcondition_kind",
        );
        assert!(
            !spec.has_at_most_one_distinct_condition_kind(),
            "saturated ephemeral must return false on has_at_most_one_distinct_condition_kind",
        );
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec cardinality-mid-endpoint
    /// triad) — the three `has_unique_missing_*_condition_kind`
    /// methods on [`EphemeralSpec`] delegate to the slice-level
    /// substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_unique_missing_kind`]
    /// over the two `Vec<Condition>` slots (precondition +
    /// postcondition) and compose the union via a two-step-short-
    /// circuit walk over [`ConditionKind::ALL`] under negated
    /// [`EphemeralSpec::has_condition_kind`]. Two-surface parity pin
    /// against
    /// [`crate::boundary::Boundary::has_unique_missing_condition_kind`]
    /// on the point-domain [`ProcessSpec`] surface — the two struct-
    /// level near-saturation-endpoint callers compose against the
    /// SAME slice-level substrate primitive so a regression at the
    /// per-slice two-step short-circuit walk under negation fails at
    /// that primitive's tests rather than as silent drift at either
    /// sugar-surface arm.
    #[test]
    fn has_unique_missing_condition_kind_triad_delegates_to_slice_has_unique_missing_kind() {
        // Empty ephemeral spec — every arm returns false (all N
        // missing, not exactly 1) on any N ≥ 2 closed set.
        assert!(
            ConditionKind::ALL.len() >= 2,
            "test assumes ConditionKind::ALL has ≥ 2 variants",
        );
        let spec = empty_ephemeral();
        assert!(
            !spec.has_unique_missing_precondition_kind(),
            "empty ephemeral must return false on has_unique_missing_precondition_kind",
        );
        assert!(
            !spec.has_unique_missing_postcondition_kind(),
            "empty ephemeral must return false on has_unique_missing_postcondition_kind",
        );
        assert!(
            !spec.has_unique_missing_condition_kind(),
            "empty ephemeral must return false on has_unique_missing_condition_kind",
        );
        assert_eq!(
            spec.has_unique_missing_condition_kind(),
            spec.missing_condition_kind_count() == 1,
            "empty has_unique_missing_condition_kind must equal (missing_condition_kind_count() == 1)",
        );

        // Single-populated per side — sweep ALL × ALL on N ≥ 3 closed
        // sets. Every per-slice arm returns false; the union returns
        // true iff exactly one ALL variant is uncovered.
        if ConditionKind::ALL.len() >= 3 {
            for pre_kind in ConditionKind::ALL {
                for post_kind in ConditionKind::ALL {
                    let mut spec = empty_ephemeral();
                    spec.preconditions.push(cond(pre_kind));
                    spec.postconditions.push(cond(post_kind));
                    assert_eq!(
                        spec.has_unique_missing_precondition_kind(),
                        spec.preconditions.has_unique_missing_kind(),
                        "EphemeralSpec::has_unique_missing_precondition_kind must delegate verbatim to \
                         preconditions.has_unique_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                    );
                    assert_eq!(
                        spec.has_unique_missing_postcondition_kind(),
                        spec.postconditions.has_unique_missing_kind(),
                        "EphemeralSpec::has_unique_missing_postcondition_kind must delegate verbatim to \
                         postconditions.has_unique_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                    );
                    let uncovered = ConditionKind::ALL
                        .into_iter()
                        .filter(|k| *k != pre_kind && *k != post_kind)
                        .count();
                    let expected_union = uncovered == 1;
                    assert_eq!(
                        spec.has_unique_missing_condition_kind(),
                        expected_union,
                        "EphemeralSpec::has_unique_missing_condition_kind must equal \
                         (uncovered-ALL-count == 1) for pre={pre_kind:?} post={post_kind:?}",
                    );

                    // Two-surface parity: lowered ProcessSpec's
                    // Boundary must agree bit-for-bit with the
                    // ephemeral sugar triad on every arm.
                    let lowered: ProcessSpec = spec.clone().into();
                    assert_eq!(
                        spec.has_unique_missing_precondition_kind(),
                        lowered.boundary.has_unique_missing_precondition_kind(),
                        "two-surface has_unique_missing_precondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                    );
                    assert_eq!(
                        spec.has_unique_missing_postcondition_kind(),
                        lowered.boundary.has_unique_missing_postcondition_kind(),
                        "two-surface has_unique_missing_postcondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                    );
                    assert_eq!(
                        spec.has_unique_missing_condition_kind(),
                        lowered.boundary.has_unique_missing_condition_kind(),
                        "two-surface has_unique_missing_condition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                    );
                }
            }
        }

        // Near-saturation-endpoint per side — each slice carries
        // every ConditionKind except one. Every per-slice arm returns
        // true; the union returns true iff BOTH slices omit the SAME
        // kind.
        for pre_omit in ConditionKind::ALL {
            for post_omit in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                for k in ConditionKind::ALL {
                    if k != pre_omit {
                        spec.preconditions.push(cond(k));
                    }
                    if k != post_omit {
                        spec.postconditions.push(cond(k));
                    }
                }
                assert!(
                    spec.has_unique_missing_precondition_kind(),
                    "near-saturation-endpoint precondition slice (omitting {pre_omit:?}) must return true on has_unique_missing_precondition_kind",
                );
                assert!(
                    spec.has_unique_missing_postcondition_kind(),
                    "near-saturation-endpoint postcondition slice (omitting {post_omit:?}) must return true on has_unique_missing_postcondition_kind",
                );
                let expected_union = pre_omit == post_omit;
                assert_eq!(
                    spec.has_unique_missing_condition_kind(),
                    expected_union,
                    "EphemeralSpec::has_unique_missing_condition_kind on both-slices-near-saturated must equal (pre_omit == post_omit) for pre_omit={pre_omit:?} post_omit={post_omit:?}",
                );

                // Two-surface parity for near-saturation arm.
                let lowered: ProcessSpec = spec.clone().into();
                assert_eq!(
                    spec.has_unique_missing_precondition_kind(),
                    lowered.boundary.has_unique_missing_precondition_kind(),
                    "two-surface has_unique_missing_precondition_kind near-saturation parity drift for pre_omit={pre_omit:?} post_omit={post_omit:?}",
                );
                assert_eq!(
                    spec.has_unique_missing_postcondition_kind(),
                    lowered.boundary.has_unique_missing_postcondition_kind(),
                    "two-surface has_unique_missing_postcondition_kind near-saturation parity drift for pre_omit={pre_omit:?} post_omit={post_omit:?}",
                );
                assert_eq!(
                    spec.has_unique_missing_condition_kind(),
                    lowered.boundary.has_unique_missing_condition_kind(),
                    "two-surface has_unique_missing_condition_kind near-saturation parity drift for pre_omit={pre_omit:?} post_omit={post_omit:?}",
                );
            }
        }

        // Saturated ephemeral — every arm returns false (0 missing,
        // not exactly 1).
        let mut spec = empty_ephemeral();
        for k in ConditionKind::ALL {
            spec.preconditions.push(cond(k));
            spec.postconditions.push(cond(k));
        }
        assert!(
            !spec.has_unique_missing_precondition_kind(),
            "saturated ephemeral must return false on has_unique_missing_precondition_kind",
        );
        assert!(
            !spec.has_unique_missing_postcondition_kind(),
            "saturated ephemeral must return false on has_unique_missing_postcondition_kind",
        );
        assert!(
            !spec.has_unique_missing_condition_kind(),
            "saturated ephemeral must return false on has_unique_missing_condition_kind",
        );
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec cardinality-many-arm
    /// triad) — the three `has_multiple_missing_*_condition_kind`
    /// methods on [`EphemeralSpec`] delegate to the slice-level
    /// substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_multiple_missing_kinds`]
    /// over the two `Vec<Condition>` slots (precondition +
    /// postcondition) and compose the union via a two-step-short-
    /// circuit walk over [`ConditionKind::ALL`] under negated
    /// [`EphemeralSpec::has_condition_kind`]. Two-surface parity pin
    /// against
    /// [`crate::boundary::Boundary::has_multiple_missing_condition_kind`]
    /// on the point-domain [`ProcessSpec`] surface — the two struct-
    /// level cardinality-many-arm callers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-
    /// slice two-step short-circuit walk under negation fails at that
    /// primitive's tests rather than as silent drift at either sugar-
    /// surface arm.
    #[test]
    fn has_multiple_missing_condition_kind_triad_delegates_to_slice_has_multiple_missing_kinds() {
        // Empty ephemeral spec — every arm returns true (all N
        // missing, ≥ 2) on any N ≥ 2 closed set.
        assert!(
            ConditionKind::ALL.len() >= 2,
            "test assumes ConditionKind::ALL has ≥ 2 variants",
        );
        let spec = empty_ephemeral();
        assert!(
            spec.has_multiple_missing_precondition_kind(),
            "empty ephemeral must return true on has_multiple_missing_precondition_kind",
        );
        assert!(
            spec.has_multiple_missing_postcondition_kind(),
            "empty ephemeral must return true on has_multiple_missing_postcondition_kind",
        );
        assert!(
            spec.has_multiple_missing_condition_kind(),
            "empty ephemeral must return true on has_multiple_missing_condition_kind",
        );
        assert_eq!(
            spec.has_multiple_missing_condition_kind(),
            spec.missing_condition_kind_count() >= 2,
            "empty has_multiple_missing_condition_kind must equal (missing_condition_kind_count() >= 2)",
        );

        // Single-populated per side — sweep ALL × ALL on N ≥ 3 closed
        // sets. Every per-slice arm returns true; the union returns
        // true iff ≥ 2 ALL variants are uncovered.
        if ConditionKind::ALL.len() >= 3 {
            for pre_kind in ConditionKind::ALL {
                for post_kind in ConditionKind::ALL {
                    let mut spec = empty_ephemeral();
                    spec.preconditions.push(cond(pre_kind));
                    spec.postconditions.push(cond(post_kind));
                    assert_eq!(
                        spec.has_multiple_missing_precondition_kind(),
                        spec.preconditions.has_multiple_missing_kinds(),
                        "EphemeralSpec::has_multiple_missing_precondition_kind must delegate verbatim to \
                         preconditions.has_multiple_missing_kinds() for pre={pre_kind:?} post={post_kind:?}",
                    );
                    assert_eq!(
                        spec.has_multiple_missing_postcondition_kind(),
                        spec.postconditions.has_multiple_missing_kinds(),
                        "EphemeralSpec::has_multiple_missing_postcondition_kind must delegate verbatim to \
                         postconditions.has_multiple_missing_kinds() for pre={pre_kind:?} post={post_kind:?}",
                    );
                    let uncovered = ConditionKind::ALL
                        .into_iter()
                        .filter(|k| *k != pre_kind && *k != post_kind)
                        .count();
                    let expected_union = uncovered >= 2;
                    assert_eq!(
                        spec.has_multiple_missing_condition_kind(),
                        expected_union,
                        "EphemeralSpec::has_multiple_missing_condition_kind must equal \
                         (uncovered-ALL-count >= 2) for pre={pre_kind:?} post={post_kind:?}",
                    );

                    // Two-surface parity: lowered ProcessSpec's
                    // Boundary must agree bit-for-bit with the
                    // ephemeral sugar triad on every arm.
                    let lowered: ProcessSpec = spec.clone().into();
                    assert_eq!(
                        spec.has_multiple_missing_precondition_kind(),
                        lowered.boundary.has_multiple_missing_precondition_kind(),
                        "two-surface has_multiple_missing_precondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                    );
                    assert_eq!(
                        spec.has_multiple_missing_postcondition_kind(),
                        lowered.boundary.has_multiple_missing_postcondition_kind(),
                        "two-surface has_multiple_missing_postcondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                    );
                    assert_eq!(
                        spec.has_multiple_missing_condition_kind(),
                        lowered.boundary.has_multiple_missing_condition_kind(),
                        "two-surface has_multiple_missing_condition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                    );
                }
            }
        }

        // Near-saturation-endpoint per side — each slice carries
        // every ConditionKind except one. Every per-slice arm returns
        // false (exactly 1 missing per slice, not ≥ 2). The union
        // has at most 1 missing (pre and post's omissions either
        // coincide → 1 missing, or differ → 0 missing), so the union
        // is always false on this arm.
        for pre_omit in ConditionKind::ALL {
            for post_omit in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                for k in ConditionKind::ALL {
                    if k != pre_omit {
                        spec.preconditions.push(cond(k));
                    }
                    if k != post_omit {
                        spec.postconditions.push(cond(k));
                    }
                }
                assert!(
                    !spec.has_multiple_missing_precondition_kind(),
                    "near-saturation-endpoint precondition slice (omitting {pre_omit:?}) must return false on has_multiple_missing_precondition_kind",
                );
                assert!(
                    !spec.has_multiple_missing_postcondition_kind(),
                    "near-saturation-endpoint postcondition slice (omitting {post_omit:?}) must return false on has_multiple_missing_postcondition_kind",
                );
                assert!(
                    !spec.has_multiple_missing_condition_kind(),
                    "EphemeralSpec::has_multiple_missing_condition_kind on both-slices-near-saturated must always be false (union missing ≤ 1) for pre_omit={pre_omit:?} post_omit={post_omit:?}",
                );

                // Two-surface parity for near-saturation arm.
                let lowered: ProcessSpec = spec.clone().into();
                assert_eq!(
                    spec.has_multiple_missing_precondition_kind(),
                    lowered.boundary.has_multiple_missing_precondition_kind(),
                    "two-surface has_multiple_missing_precondition_kind near-saturation parity drift for pre_omit={pre_omit:?} post_omit={post_omit:?}",
                );
                assert_eq!(
                    spec.has_multiple_missing_postcondition_kind(),
                    lowered.boundary.has_multiple_missing_postcondition_kind(),
                    "two-surface has_multiple_missing_postcondition_kind near-saturation parity drift for pre_omit={pre_omit:?} post_omit={post_omit:?}",
                );
                assert_eq!(
                    spec.has_multiple_missing_condition_kind(),
                    lowered.boundary.has_multiple_missing_condition_kind(),
                    "two-surface has_multiple_missing_condition_kind near-saturation parity drift for pre_omit={pre_omit:?} post_omit={post_omit:?}",
                );
            }
        }

        // Saturated ephemeral — every arm returns false (0 missing,
        // not ≥ 2).
        let mut spec = empty_ephemeral();
        for k in ConditionKind::ALL {
            spec.preconditions.push(cond(k));
            spec.postconditions.push(cond(k));
        }
        assert!(
            !spec.has_multiple_missing_precondition_kind(),
            "saturated ephemeral must return false on has_multiple_missing_precondition_kind",
        );
        assert!(
            !spec.has_multiple_missing_postcondition_kind(),
            "saturated ephemeral must return false on has_multiple_missing_postcondition_kind",
        );
        assert!(
            !spec.has_multiple_missing_condition_kind(),
            "saturated ephemeral must return false on has_multiple_missing_condition_kind",
        );
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec cardinality "≤ 1"
    /// triad) — the three `has_at_most_one_missing_*_condition_kind`
    /// methods on [`EphemeralSpec`] delegate to the slice-level
    /// substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_at_most_one_missing_kind`]
    /// over the two `Vec<Condition>` slots (precondition +
    /// postcondition) and compose the union via
    /// `!self.has_multiple_missing_condition_kind()` — a definitional
    /// negation of the many-arm union primitive. Two-surface parity
    /// pin against
    /// [`crate::boundary::Boundary::has_at_most_one_missing_condition_kind`]
    /// on the point-domain [`ProcessSpec`] surface — the two struct-
    /// level cardinality "≤ 1" callers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-
    /// slice "≤ 1" negation fails at that primitive's tests rather
    /// than as silent drift at either sugar-surface arm.
    #[test]
    fn has_at_most_one_missing_condition_kind_triad_delegates_to_slice_has_at_most_one_missing_kind(
    ) {
        // Empty ephemeral spec — every arm returns false (all N
        // missing, not ≤ 1) on any N ≥ 2 closed set.
        assert!(
            ConditionKind::ALL.len() >= 2,
            "test assumes ConditionKind::ALL has ≥ 2 variants",
        );
        let spec = empty_ephemeral();
        assert!(
            !spec.has_at_most_one_missing_precondition_kind(),
            "empty ephemeral must return false on has_at_most_one_missing_precondition_kind",
        );
        assert!(
            !spec.has_at_most_one_missing_postcondition_kind(),
            "empty ephemeral must return false on has_at_most_one_missing_postcondition_kind",
        );
        assert!(
            !spec.has_at_most_one_missing_condition_kind(),
            "empty ephemeral must return false on has_at_most_one_missing_condition_kind",
        );
        assert_eq!(
            spec.has_at_most_one_missing_condition_kind(),
            spec.missing_condition_kind_count() <= 1,
            "empty has_at_most_one_missing_condition_kind must equal (missing_condition_kind_count() <= 1)",
        );

        // Single-populated per side — sweep ALL × ALL on N ≥ 3
        // closed sets. Every per-slice arm returns false; the union
        // returns true iff ≤ 1 ALL variant is uncovered.
        if ConditionKind::ALL.len() >= 3 {
            for pre_kind in ConditionKind::ALL {
                for post_kind in ConditionKind::ALL {
                    let mut spec = empty_ephemeral();
                    spec.preconditions.push(cond(pre_kind));
                    spec.postconditions.push(cond(post_kind));
                    assert_eq!(
                        spec.has_at_most_one_missing_precondition_kind(),
                        spec.preconditions.has_at_most_one_missing_kind(),
                        "EphemeralSpec::has_at_most_one_missing_precondition_kind must delegate verbatim to \
                         preconditions.has_at_most_one_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                    );
                    assert_eq!(
                        spec.has_at_most_one_missing_postcondition_kind(),
                        spec.postconditions.has_at_most_one_missing_kind(),
                        "EphemeralSpec::has_at_most_one_missing_postcondition_kind must delegate verbatim to \
                         postconditions.has_at_most_one_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                    );
                    let uncovered = ConditionKind::ALL
                        .into_iter()
                        .filter(|k| *k != pre_kind && *k != post_kind)
                        .count();
                    let expected_union = uncovered <= 1;
                    assert_eq!(
                        spec.has_at_most_one_missing_condition_kind(),
                        expected_union,
                        "EphemeralSpec::has_at_most_one_missing_condition_kind must equal \
                         (uncovered-ALL-count <= 1) for pre={pre_kind:?} post={post_kind:?}",
                    );

                    // Two-surface parity: lowered ProcessSpec's
                    // Boundary must agree bit-for-bit with the
                    // ephemeral sugar triad on every arm.
                    let lowered: ProcessSpec = spec.clone().into();
                    assert_eq!(
                        spec.has_at_most_one_missing_precondition_kind(),
                        lowered.boundary.has_at_most_one_missing_precondition_kind(),
                        "two-surface has_at_most_one_missing_precondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                    );
                    assert_eq!(
                        spec.has_at_most_one_missing_postcondition_kind(),
                        lowered.boundary.has_at_most_one_missing_postcondition_kind(),
                        "two-surface has_at_most_one_missing_postcondition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                    );
                    assert_eq!(
                        spec.has_at_most_one_missing_condition_kind(),
                        lowered.boundary.has_at_most_one_missing_condition_kind(),
                        "two-surface has_at_most_one_missing_condition_kind parity drift for pre={pre_kind:?} post={post_kind:?}",
                    );
                }
            }
        }

        // Near-saturation-endpoint per side — each slice carries
        // every ConditionKind except one. Every per-slice arm returns
        // true (exactly 1 missing per slice, ≤ 1). The union has ≤ 1
        // missing (pre and post's omissions either coincide → 1
        // missing, or differ → 0 missing), so the union is always
        // true on this arm.
        for pre_omit in ConditionKind::ALL {
            for post_omit in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                for k in ConditionKind::ALL {
                    if k != pre_omit {
                        spec.preconditions.push(cond(k));
                    }
                    if k != post_omit {
                        spec.postconditions.push(cond(k));
                    }
                }
                assert!(
                    spec.has_at_most_one_missing_precondition_kind(),
                    "near-saturation-endpoint precondition slice (omitting {pre_omit:?}) must return true on has_at_most_one_missing_precondition_kind",
                );
                assert!(
                    spec.has_at_most_one_missing_postcondition_kind(),
                    "near-saturation-endpoint postcondition slice (omitting {post_omit:?}) must return true on has_at_most_one_missing_postcondition_kind",
                );
                assert!(
                    spec.has_at_most_one_missing_condition_kind(),
                    "EphemeralSpec::has_at_most_one_missing_condition_kind on both-slices-near-saturated must always be true (union missing ≤ 1) for pre_omit={pre_omit:?} post_omit={post_omit:?}",
                );

                // Two-surface parity for near-saturation arm.
                let lowered: ProcessSpec = spec.clone().into();
                assert_eq!(
                    spec.has_at_most_one_missing_precondition_kind(),
                    lowered.boundary.has_at_most_one_missing_precondition_kind(),
                    "two-surface has_at_most_one_missing_precondition_kind near-saturation parity drift for pre_omit={pre_omit:?} post_omit={post_omit:?}",
                );
                assert_eq!(
                    spec.has_at_most_one_missing_postcondition_kind(),
                    lowered.boundary.has_at_most_one_missing_postcondition_kind(),
                    "two-surface has_at_most_one_missing_postcondition_kind near-saturation parity drift for pre_omit={pre_omit:?} post_omit={post_omit:?}",
                );
                assert_eq!(
                    spec.has_at_most_one_missing_condition_kind(),
                    lowered.boundary.has_at_most_one_missing_condition_kind(),
                    "two-surface has_at_most_one_missing_condition_kind near-saturation parity drift for pre_omit={pre_omit:?} post_omit={post_omit:?}",
                );
            }
        }

        // Saturated ephemeral — every arm returns true (0 missing,
        // ≤ 1).
        let mut spec = empty_ephemeral();
        for k in ConditionKind::ALL {
            spec.preconditions.push(cond(k));
            spec.postconditions.push(cond(k));
        }
        assert!(
            spec.has_at_most_one_missing_precondition_kind(),
            "saturated ephemeral must return true on has_at_most_one_missing_precondition_kind",
        );
        assert!(
            spec.has_at_most_one_missing_postcondition_kind(),
            "saturated ephemeral must return true on has_at_most_one_missing_postcondition_kind",
        );
        assert!(
            spec.has_at_most_one_missing_condition_kind(),
            "saturated ephemeral must return true on has_at_most_one_missing_condition_kind",
        );
    }

    /// SUBSTRATE-DELEGATION pin (EphemeralSpec per-kind-complement
    /// triad) — the three `lacks_*_condition_kind` methods on
    /// [`EphemeralSpec`] delegate to the slice-level substrate primitive
    /// [`ConditionSliceExt::lacks_kind`] over the two `Vec<Condition>`
    /// slots (precondition + postcondition) and compose the union via
    /// `!self.has_condition_kind(kind)`. Two-surface parity pin against
    /// [`crate::boundary::Boundary::lacks_condition_kind`] on the
    /// point-domain [`ProcessSpec`] surface — the two struct-level
    /// per-kind-complement callers compose against the SAME slice-level
    /// substrate primitive so a regression at the per-slice negation
    /// fails at that primitive's tests rather than as silent drift at
    /// either sugar-surface arm. Also pins the composition laws
    /// `lacks_*_condition_kind(k) == !has_*_condition_kind(k)` at each
    /// arm AND `lacks_condition_kind(k) == lacks_precondition_kind(k) &&
    /// lacks_postcondition_kind(k)` (the union AND-composition dual of
    /// `has`'s OR-composition).
    #[test]
    fn lacks_condition_kind_triad_delegates_to_slice_lacks_kind() {
        // Empty ephemeral spec — every arm returns true on every kind.
        let spec = empty_ephemeral();
        for kind in ConditionKind::ALL {
            assert!(
                spec.lacks_precondition_kind(kind),
                "empty ephemeral must return true on lacks_precondition_kind for {kind:?}",
            );
            assert!(
                spec.lacks_postcondition_kind(kind),
                "empty ephemeral must return true on lacks_postcondition_kind for {kind:?}",
            );
            assert!(
                spec.lacks_condition_kind(kind),
                "empty ephemeral must return true on lacks_condition_kind for {kind:?}",
            );
            assert_eq!(
                spec.lacks_condition_kind(kind),
                !spec.has_condition_kind(kind),
                "empty lacks_condition_kind must equal !has_condition_kind for {kind:?}",
            );
        }

        // Single-populated per side — sweep ALL × ALL, then probe every
        // ConditionKind on the (pre, post, union) triad + two-surface
        // parity against the lowered ProcessSpec's Boundary.
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                let lowered: ProcessSpec = spec.clone().into();
                for probe in ConditionKind::ALL {
                    assert_eq!(
                        spec.lacks_precondition_kind(probe),
                        spec.preconditions.lacks_kind(probe),
                        "EphemeralSpec::lacks_precondition_kind must delegate verbatim to preconditions.lacks_kind for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    assert_eq!(
                        spec.lacks_postcondition_kind(probe),
                        spec.postconditions.lacks_kind(probe),
                        "EphemeralSpec::lacks_postcondition_kind must delegate verbatim to postconditions.lacks_kind for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    let expected_union = pre_kind != probe && post_kind != probe;
                    assert_eq!(
                        spec.lacks_condition_kind(probe),
                        expected_union,
                        "EphemeralSpec::lacks_condition_kind must equal all-ALL-absent-in-both-slices for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    assert_eq!(
                        spec.lacks_condition_kind(probe),
                        !spec.has_condition_kind(probe),
                        "EphemeralSpec::lacks_condition_kind must equal !has_condition_kind for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    assert_eq!(
                        spec.lacks_condition_kind(probe),
                        spec.lacks_precondition_kind(probe)
                            && spec.lacks_postcondition_kind(probe),
                        "EphemeralSpec::lacks_condition_kind must equal AND-of-half-slice-arms for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );

                    // Two-surface parity: lowered ProcessSpec's Boundary
                    // must agree bit-for-bit with the ephemeral sugar
                    // triad on every arm.
                    assert_eq!(
                        spec.lacks_precondition_kind(probe),
                        lowered.boundary.lacks_precondition_kind(probe),
                        "two-surface lacks_precondition_kind parity drift for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    assert_eq!(
                        spec.lacks_postcondition_kind(probe),
                        lowered.boundary.lacks_postcondition_kind(probe),
                        "two-surface lacks_postcondition_kind parity drift for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    assert_eq!(
                        spec.lacks_condition_kind(probe),
                        lowered.boundary.lacks_condition_kind(probe),
                        "two-surface lacks_condition_kind parity drift for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                }
            }
        }

        // Saturated ephemeral — both slices carry every ConditionKind,
        // every arm returns false on every kind.
        let mut spec = empty_ephemeral();
        for k in ConditionKind::ALL {
            spec.preconditions.push(cond(k));
            spec.postconditions.push(cond(k));
        }
        for kind in ConditionKind::ALL {
            assert!(
                !spec.lacks_precondition_kind(kind),
                "saturated ephemeral must return false on lacks_precondition_kind for {kind:?}",
            );
            assert!(
                !spec.lacks_postcondition_kind(kind),
                "saturated ephemeral must return false on lacks_postcondition_kind for {kind:?}",
            );
            assert!(
                !spec.lacks_condition_kind(kind),
                "saturated ephemeral must return false on lacks_condition_kind for {kind:?}",
            );
        }
    }

    /// TRIAD delegation pin — the (precondition, postcondition,
    /// condition-union) kind-scoped strict-refinement triad on
    /// [`EphemeralSpec`] agrees byte-for-byte with the slice-level
    /// substrate primitive
    /// [`crate::boundary::ConditionSliceExt::has_only_kind`] on every
    /// authored arrangement AND with the lowered
    /// [`ProcessSpec::boundary`]'s kind-scoped strict-refinement
    /// triad through the `From<EphemeralSpec>` bridge — the two-
    /// surface parity contract at the well-formed-diagonal arm.
    ///
    /// Sweeps [`ConditionKind::ALL`] × [`ConditionKind::ALL`] over
    /// single-populated-per-side arrangements (the well-formed
    /// diagonal), probing every [`ConditionKind`] at the union arm
    /// against the DERIVED oracle `pre_kind == probe && post_kind ==
    /// probe`. Also sweeps the single-side-only-populated arms (the
    /// union carries a singleton distinct set — pins the union arm
    /// reaches the union primitive, not the (pre AND post) AND-
    /// composition). A regression at the union arm's fused walk or
    /// at the `From<EphemeralSpec>` bridge surfaces HERE.
    #[test]
    fn has_only_condition_kind_triad_delegates_to_slice_has_only_kind() {
        // Empty ephemeral spec — every arm returns false on every
        // kind (no kind is populated, so no kind is "only").
        let spec = empty_ephemeral();
        for kind in ConditionKind::ALL {
            assert!(
                !spec.has_only_precondition_kind(kind),
                "empty ephemeral must return false on has_only_precondition_kind for {kind:?}",
            );
            assert!(
                !spec.has_only_postcondition_kind(kind),
                "empty ephemeral must return false on has_only_postcondition_kind for {kind:?}",
            );
            assert!(
                !spec.has_only_condition_kind(kind),
                "empty ephemeral must return false on has_only_condition_kind for {kind:?}",
            );
        }

        // Single-populated per side — sweep ALL × ALL, then probe
        // every ConditionKind on the (pre, post, union) triad + two-
        // surface parity against the lowered ProcessSpec's Boundary.
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut spec = empty_ephemeral();
                spec.preconditions.push(cond(pre_kind));
                spec.postconditions.push(cond(post_kind));
                let lowered: ProcessSpec = spec.clone().into();
                for probe in ConditionKind::ALL {
                    assert_eq!(
                        spec.has_only_precondition_kind(probe),
                        spec.preconditions.has_only_kind(probe),
                        "EphemeralSpec::has_only_precondition_kind must delegate verbatim to preconditions.has_only_kind for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    assert_eq!(
                        spec.has_only_postcondition_kind(probe),
                        spec.postconditions.has_only_kind(probe),
                        "EphemeralSpec::has_only_postcondition_kind must delegate verbatim to postconditions.has_only_kind for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    let expected_union = pre_kind == probe && post_kind == probe;
                    assert_eq!(
                        spec.has_only_condition_kind(probe),
                        expected_union,
                        "EphemeralSpec::has_only_condition_kind must equal (pre_kind == probe && post_kind == probe) for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );

                    // Two-surface parity: lowered ProcessSpec's
                    // Boundary must agree bit-for-bit with the
                    // ephemeral sugar triad on every arm.
                    assert_eq!(
                        spec.has_only_precondition_kind(probe),
                        lowered.boundary.has_only_precondition_kind(probe),
                        "two-surface has_only_precondition_kind parity drift for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    assert_eq!(
                        spec.has_only_postcondition_kind(probe),
                        lowered.boundary.has_only_postcondition_kind(probe),
                        "two-surface has_only_postcondition_kind parity drift for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    assert_eq!(
                        spec.has_only_condition_kind(probe),
                        lowered.boundary.has_only_condition_kind(probe),
                        "two-surface has_only_condition_kind parity drift for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                }
            }
        }

        // Single-side-only populated — the union carries a singleton
        // distinct set; the union arm returns `true` for the populated
        // kind and `false` for every other kind, DESPITE the empty
        // side's `has_only_kind` returning `false`. Pins that the
        // union arm reaches the union primitive
        // [`Self::has_condition_kind`], not the (pre AND post) AND-
        // composition of the per-slice arms. Also pins two-surface
        // parity on the single-side arrangement.
        for populated in ConditionKind::ALL {
            let mut spec = empty_ephemeral();
            spec.preconditions.push(cond(populated));
            let lowered: ProcessSpec = spec.clone().into();
            for probe in ConditionKind::ALL {
                let expected = probe == populated;
                assert_eq!(
                    spec.has_only_condition_kind(probe),
                    expected,
                    "pre-only ephemeral populated={populated:?} must return {expected} on has_only_condition_kind({probe:?})",
                );
                assert_eq!(
                    spec.has_only_condition_kind(probe),
                    lowered.boundary.has_only_condition_kind(probe),
                    "two-surface pre-only has_only_condition_kind parity drift for populated={populated:?} probe={probe:?}",
                );
            }

            let mut spec = empty_ephemeral();
            spec.postconditions.push(cond(populated));
            let lowered: ProcessSpec = spec.clone().into();
            for probe in ConditionKind::ALL {
                let expected = probe == populated;
                assert_eq!(
                    spec.has_only_condition_kind(probe),
                    expected,
                    "post-only ephemeral populated={populated:?} must return {expected} on has_only_condition_kind({probe:?})",
                );
                assert_eq!(
                    spec.has_only_condition_kind(probe),
                    lowered.boundary.has_only_condition_kind(probe),
                    "two-surface post-only has_only_condition_kind parity drift for populated={populated:?} probe={probe:?}",
                );
            }
        }

        // Saturated ephemeral — both slices carry every ConditionKind,
        // every arm returns false on every kind (N distinct kinds, no
        // kind is "only").
        let mut spec = empty_ephemeral();
        for k in ConditionKind::ALL {
            spec.preconditions.push(cond(k));
            spec.postconditions.push(cond(k));
        }
        for kind in ConditionKind::ALL {
            assert!(
                !spec.has_only_precondition_kind(kind),
                "saturated ephemeral must return false on has_only_precondition_kind for {kind:?}",
            );
            assert!(
                !spec.has_only_postcondition_kind(kind),
                "saturated ephemeral must return false on has_only_postcondition_kind for {kind:?}",
            );
            assert!(
                !spec.has_only_condition_kind(kind),
                "saturated ephemeral must return false on has_only_condition_kind for {kind:?}",
            );
        }
    }

    /// TRIAD delegation pin — the (precondition, postcondition,
    /// condition-union) kind-scoped strict-refinement-on-missing triad
    /// on [`EphemeralSpec`] agrees byte-for-byte with the slice-level
    /// substrate primitive
    /// [`crate::boundary::ConditionSliceExt::lacks_only_kind`] on every
    /// authored arrangement, AND agrees bit-for-bit with the lowered
    /// [`ProcessSpec::boundary`]'s triad via the [`From`] bridge.
    ///
    /// Sweeps [`ConditionKind::ALL`] × [`ConditionKind::ALL`] over
    /// single-populated-per-side arrangements + near-saturation-per-
    /// side arrangements + single-side-only near-saturation
    /// arrangements. The union arm is probed against the DERIVED
    /// oracle `spec.missing_condition_kinds() == vec![probe]`, and
    /// the per-slice arms delegate to the slice substrate primitive
    /// verbatim. Two-surface parity ensures a regression at the
    /// `From<EphemeralSpec>` bridge (a re-ordered condition Vec, a
    /// dropped ClosedLoopAuth default) surfaces HERE at the union arm.
    #[test]
    fn lacks_only_condition_kind_triad_delegates_to_slice_lacks_only_kind() {
        // Empty ephemeral spec — every kind is missing on N ≥ 2, so
        // no kind is "only" missing on any arm.
        let spec = empty_ephemeral();
        let lowered: ProcessSpec = spec.clone().into();
        for kind in ConditionKind::ALL {
            assert_eq!(
                spec.lacks_only_precondition_kind(kind),
                spec.preconditions.lacks_only_kind(kind),
                "empty ephemeral lacks_only_precondition_kind must delegate to preconditions.lacks_only_kind for {kind:?}",
            );
            assert_eq!(
                spec.lacks_only_postcondition_kind(kind),
                spec.postconditions.lacks_only_kind(kind),
                "empty ephemeral lacks_only_postcondition_kind must delegate to postconditions.lacks_only_kind for {kind:?}",
            );
            assert_eq!(
                spec.lacks_only_condition_kind(kind),
                lowered.boundary.lacks_only_condition_kind(kind),
                "two-surface empty lacks_only_condition_kind parity drift for {kind:?}",
            );
        }

        // Near-saturation per side — build an ephemeral spec whose both
        // sides carry every kind except one; sweep every omitted kind
        // and probe every ConditionKind on the (pre, post, union) triad.
        for omitted in ConditionKind::ALL {
            let mut spec = empty_ephemeral();
            for k in ConditionKind::ALL {
                if k != omitted {
                    spec.preconditions.push(cond(k));
                    spec.postconditions.push(cond(k));
                }
            }
            let lowered: ProcessSpec = spec.clone().into();
            for probe in ConditionKind::ALL {
                let expected = probe == omitted;
                assert_eq!(
                    spec.lacks_only_precondition_kind(probe),
                    expected,
                    "near-saturation ephemeral omitted={omitted:?} must return {expected} on lacks_only_precondition_kind({probe:?})",
                );
                assert_eq!(
                    spec.lacks_only_postcondition_kind(probe),
                    expected,
                    "near-saturation ephemeral omitted={omitted:?} must return {expected} on lacks_only_postcondition_kind({probe:?})",
                );
                assert_eq!(
                    spec.lacks_only_condition_kind(probe),
                    expected,
                    "near-saturation ephemeral omitted={omitted:?} must return {expected} on lacks_only_condition_kind({probe:?})",
                );
                assert_eq!(
                    spec.lacks_only_condition_kind(probe),
                    spec.missing_condition_kinds() == vec![probe],
                    "near-saturation ephemeral omitted={omitted:?} must agree with missing_condition_kinds() == vec![{probe:?}]",
                );

                // Two-surface parity via lowered ProcessSpec.
                assert_eq!(
                    spec.lacks_only_precondition_kind(probe),
                    lowered.boundary.lacks_only_precondition_kind(probe),
                    "two-surface lacks_only_precondition_kind parity drift for omitted={omitted:?} probe={probe:?}",
                );
                assert_eq!(
                    spec.lacks_only_postcondition_kind(probe),
                    lowered.boundary.lacks_only_postcondition_kind(probe),
                    "two-surface lacks_only_postcondition_kind parity drift for omitted={omitted:?} probe={probe:?}",
                );
                assert_eq!(
                    spec.lacks_only_condition_kind(probe),
                    lowered.boundary.lacks_only_condition_kind(probe),
                    "two-surface lacks_only_condition_kind parity drift for omitted={omitted:?} probe={probe:?}",
                );
            }
        }

        // Single-side-only near-saturation — the populated side covers
        // every kind except one; the OTHER side is empty. The union
        // still has missing set `{omitted}` (the populated side's hole
        // wins), so the union arm returns `true` for `omitted` and
        // `false` for every other kind, DESPITE the empty side's
        // `lacks_only_kind` returning `false` on every kind for N ≥ 2.
        // Pins that the union arm reaches the union primitive, not the
        // (pre AND post) AND-composition.
        for omitted in ConditionKind::ALL {
            let mut spec = empty_ephemeral();
            for k in ConditionKind::ALL {
                if k != omitted {
                    spec.preconditions.push(cond(k));
                }
            }
            let lowered: ProcessSpec = spec.clone().into();
            for probe in ConditionKind::ALL {
                let expected = probe == omitted;
                assert_eq!(
                    spec.lacks_only_condition_kind(probe),
                    expected,
                    "pre-only near-saturation ephemeral omitted={omitted:?} must return {expected} on lacks_only_condition_kind({probe:?})",
                );
                assert_eq!(
                    spec.lacks_only_condition_kind(probe),
                    lowered.boundary.lacks_only_condition_kind(probe),
                    "two-surface pre-only near-saturation lacks_only_condition_kind parity drift for omitted={omitted:?} probe={probe:?}",
                );
            }
        }

        // Saturated ephemeral — every kind populated, no kind missing,
        // every arm returns false on every kind.
        let mut spec = empty_ephemeral();
        for k in ConditionKind::ALL {
            spec.preconditions.push(cond(k));
            spec.postconditions.push(cond(k));
        }
        for kind in ConditionKind::ALL {
            assert!(
                !spec.lacks_only_precondition_kind(kind),
                "saturated ephemeral must return false on lacks_only_precondition_kind for {kind:?}",
            );
            assert!(
                !spec.lacks_only_postcondition_kind(kind),
                "saturated ephemeral must return false on lacks_only_postcondition_kind for {kind:?}",
            );
            assert!(
                !spec.lacks_only_condition_kind(kind),
                "saturated ephemeral must return false on lacks_only_condition_kind for {kind:?}",
            );
        }
    }
}
