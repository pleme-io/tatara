//! Boundary conditions — predicates that gate phase transitions.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::flux_resource::FluxResource;

/// Boundary specification — preconditions gate Running,
/// postconditions gate Running → Attested.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Boundary {
    #[serde(default)]
    pub preconditions: Vec<Condition>,
    #[serde(default)]
    pub postconditions: Vec<Condition>,
    /// Max time before VERIFY fails — parsed as a `go`-style duration.
    /// Empty = controller default (15m).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
}

impl Boundary {
    /// True iff at least one [`Condition`] in
    /// `preconditions ∪ postconditions` carries the given
    /// [`ConditionKind`] — the ONE substrate primitive that owns the
    /// (closed-set discriminator, boundary-condition presence) probe on
    /// this typed surface.
    ///
    /// # Semantics
    ///
    /// The two condition vectors are unioned: a caller asking "does this
    /// spec name a `ClosedLoopAuth` predicate anywhere" doesn't care
    /// whether the operator authored it on the pre- or post-condition
    /// side. A boundary with the given kind on ONLY preconditions returns
    /// `true`; a boundary with the given kind on ONLY postconditions
    /// returns `true`; a boundary with neither returns `false`.
    ///
    /// # Sibling to [`crate::intent::Intent::has`] + [`crate::lifetime::Lifetime::has`]
    ///
    /// Same shape, same axis, third instance in the workspace-wide
    /// closed-set-driven presence-probe algebra. `Intent::has` +
    /// `Lifetime::has` publish the same `(&self, K) -> bool` signature
    /// where `K` is the discriminator's `Kind` (auto-derived through
    /// `#[derive(DeriveClosedSet)]`). A future normalization at that
    /// probe shape (a widened return carrying the matching Condition
    /// ref, a debug-build assertion on pre/post drift, a fleet-wide
    /// warn on redundant duplicates) lands at ONE site per surface
    /// and every downstream `<xxx>-<kind>` require-tag family +
    /// closed-set audit dispatcher picks it up mechanically.
    ///
    /// # Peer on the ephemeral surface — [`crate::ephemeral::EphemeralSpec::has_condition_kind`]
    ///
    /// Same signature `(ConditionKind) -> bool`, same union body
    /// (`preconditions.has_kind(k) || postconditions.has_kind(k)`), on
    /// the sugar-surface type [`crate::ephemeral::EphemeralSpec`] whose
    /// pre/post condition vectors live directly on the struct rather
    /// than inside a nested [`Boundary`] slot. Both methods compose
    /// against the ONE slice-level substrate primitive
    /// [`ConditionSliceExt::has_kind`] — a regression at the per-slice
    /// walk fails at that primitive's tests rather than as silent drift
    /// at either struct-level union caller. The ephemeral require-tag
    /// classifier reaches its `condition-<kind>` prefix family through
    /// the peer method byte-for-byte symmetrical with the point
    /// surface's `condition-<kind>` family that composes through this
    /// method.
    ///
    /// # Compounding
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// with the closed-set `FromStr` autoderived on [`ConditionKind`]
    /// through the `strip_and_classify_prefixed_kind` substrate to
    /// publish a `condition-<kind>` prefix family byte-for-byte
    /// symmetrical with `intent-<kind>` + `lifetime-<kind>`. A future
    /// [`ConditionKind`] variant added to `ALL` reaches every downstream
    /// (require-tag classifier, coherence check, editor completion
    /// provider) through the SAME closed-set walk with no per-caller
    /// edit.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — the presence-probe body lives at ONE substrate site so
    /// every downstream `condition-<kind>` requires-tag surface,
    /// closed-set audit dispatcher, and future variant addition binds
    /// through the SAME shape). THEORY.md §VI.1 (generation over
    /// composition — a ninth [`ConditionKind`] variant lands at ONE
    /// `ALL` entry + ONE `as_str` arm and the presence probe picks it
    /// up mechanically without further per-consumer edits).
    #[must_use]
    pub fn has_condition_kind(&self, kind: ConditionKind) -> bool {
        self.has_precondition_kind(kind) || self.has_postcondition_kind(kind)
    }

    /// True iff at least one [`Condition`] in `self.preconditions`
    /// carries the given [`ConditionKind`] — the precondition-side arm
    /// of the (precondition, postcondition, condition-union) triad on
    /// [`Boundary`], sibling to [`Self::has_postcondition_kind`] and
    /// half-composition of [`Self::has_condition_kind`].
    ///
    /// Thin typed delegate to [`ConditionSliceExt::has_kind`] over
    /// [`Self::preconditions`]. Peer of [`Self::has_postcondition_kind`]
    /// on the (precondition, postcondition) partition of the boundary's
    /// two condition-vector slots; both peers compose against the SAME
    /// slice-level substrate primitive and their `||` composition is
    /// [`Self::has_condition_kind`]. A regression that swapped the
    /// slice at either arm (a copy-paste that pointed the precondition
    /// probe at `self.postconditions`, an inline `.iter().any` closure
    /// body that outlasted the lift) surfaces at the composition-law
    /// pin `boundary_has_condition_kind_composes_precondition_and_postcondition_arms`
    /// rather than as silent classifier drift at every downstream
    /// `precondition-<kind>` require-tag callsite.
    ///
    /// # Why lift
    ///
    /// Pre-lift the point-domain `precondition-<kind>` require-tag
    /// classifier in `tatara-reconciler::bin::tatara-check` reached the
    /// precondition-side slice through direct field access
    /// (`spec.boundary.preconditions.has_kind(k)`) while its sibling
    /// `condition-<kind>` classifier routed through the named
    /// [`Self::has_condition_kind`] primitive. The asymmetry meant a
    /// future normalization at the presence-probe shape (a widened
    /// return carrying the matching [`Condition`] ref, a debug-build
    /// assertion on redundant duplicates, a fleet-wide warn on
    /// pre-only ClosedLoopAuth authoring) would land at the union
    /// primitive but bypass the two half-slice classifiers. Post-lift
    /// the (precondition, postcondition, condition-union) triad lives
    /// at ONE typed algebra surface on [`Boundary`], with the
    /// `condition-<K> = precondition-<K> ∨ postcondition-<K>`
    /// composition law pinned as a first-class typed invariant
    /// (see the composition-pin test in this module) rather than a
    /// per-caller discipline.
    ///
    /// # Semantics
    ///
    /// Returns `true` iff `self.preconditions.iter().any(|c| c.kind ==
    /// kind)`. Ignores `self.postconditions` — an operator who authored
    /// the kind on ONLY postconditions gets `false` from this probe and
    /// `true` from [`Self::has_postcondition_kind`]. The two half-slice
    /// arms partition the (kind, side) matrix exhaustively across the
    /// four states (kind absent both, pre-only, post-only, both).
    ///
    /// # Sibling to [`crate::ephemeral::EphemeralSpec::has_precondition_kind`]
    ///
    /// Same shape, same axis, third and fourth methods in the
    /// workspace-wide `has_(pre|post)condition_kind` two-surface
    /// family. [`crate::ephemeral::EphemeralSpec::has_precondition_kind`]
    /// composes byte-identical `preconditions.has_kind(k)` semantics on
    /// the sugar-surface type's direct `preconditions: Vec<Condition>`
    /// field, so both surfaces publish a `precondition-<kind>` require-
    /// tag prefix family byte-for-byte symmetrical (point surface
    /// through this method, ephemeral surface through its peer).
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the per-slice presence-probe body lives at
    /// ONE substrate site so every downstream `precondition-<kind>`
    /// require-tag surface, closed-set audit dispatcher, and future
    /// variant addition binds through the SAME shape). THEORY.md §VI.1
    /// (generation over composition — the union primitive
    /// [`Self::has_condition_kind`] emerges from the composition of
    /// its two half-slice arms rather than as a hand-authored `||`
    /// closure at every downstream consumer).
    #[must_use]
    pub fn has_precondition_kind(&self, kind: ConditionKind) -> bool {
        self.preconditions.has_kind(kind)
    }

    /// True iff at least one [`Condition`] in `self.postconditions`
    /// carries the given [`ConditionKind`] — the postcondition-side arm
    /// of the (precondition, postcondition, condition-union) triad on
    /// [`Boundary`], sibling to [`Self::has_precondition_kind`] and
    /// half-composition of [`Self::has_condition_kind`].
    ///
    /// Thin typed delegate to [`ConditionSliceExt::has_kind`] over
    /// [`Self::postconditions`]. Peer of [`Self::has_precondition_kind`]
    /// on the (precondition, postcondition) partition of the boundary's
    /// two condition-vector slots. See [`Self::has_precondition_kind`]
    /// for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin, and
    /// ONE two-surface parity contract with the ephemeral sugar type
    /// via [`crate::ephemeral::EphemeralSpec::has_postcondition_kind`].
    #[must_use]
    pub fn has_postcondition_kind(&self, kind: ConditionKind) -> bool {
        self.postconditions.has_kind(kind)
    }

    /// Returns the first [`Condition`] in
    /// `preconditions ∪ postconditions` carrying the given
    /// [`ConditionKind`], searching preconditions first — the
    /// widened peer of [`Self::has_condition_kind`] one refinement
    /// higher on the presence-probe algebra.
    ///
    /// # Sibling to [`Self::has_condition_kind`]
    ///
    /// Same axis, one refinement wider: `has_condition_kind` collapses
    /// the return to a `bool` (`find_condition_kind(k).is_some()`);
    /// this method returns the matching `&Condition` so consumers can
    /// read [`Condition::params`] (the `probeImage`, the `expression`,
    /// the `flakeRef`) at the presence probe's own callsite without
    /// re-walking the two condition vectors. Pinned by the composition
    /// law `has_condition_kind(K) == find_condition_kind(K).is_some()`
    /// at [`Boundary`]'s substrate-delegation test.
    ///
    /// # Semantics — precondition takes precedence
    ///
    /// Walks [`Self::preconditions`] first, then [`Self::postconditions`]:
    /// a kind authored on BOTH sides returns the precondition-side
    /// [`Condition`]. Callers that need the postcondition-side match
    /// specifically reach for [`Self::find_postcondition_kind`]; callers
    /// that need every match across both sides walk the two vectors
    /// directly. Composition law: `find_condition_kind(K) ==
    /// find_precondition_kind(K).or_else(|| find_postcondition_kind(K))`,
    /// pinned as a first-class typed invariant.
    ///
    /// # Peer on the ephemeral surface — [`crate::ephemeral::EphemeralSpec::find_condition_kind`]
    ///
    /// Same signature `(ConditionKind) -> Option<&Condition>`, same
    /// precondition-first body, on the sugar-surface type whose
    /// pre/post condition vectors live directly on the struct. Both
    /// methods compose against the SAME slice-level substrate primitive
    /// [`ConditionSliceExt::find_kind`] — a regression at the per-slice
    /// walk fails at that primitive's tests rather than as silent drift
    /// at either struct-level widened caller.
    ///
    /// # Compounding
    ///
    /// A future diagnostic consumer (an operator-facing "condition
    /// {kind} matched on {side} with params.{key}={value}" message
    /// emitted by the require-tag classifier, a coherence check that
    /// verifies "every `ClosedLoopAuth` postcondition carries a
    /// non-empty `probeImage`" by inspecting the returned
    /// `&Condition.params`, an editor completion listing which
    /// params-keys appear on the present kind) reaches for the
    /// matching [`Condition`] through this ONE method rather than
    /// re-walking the two vectors with `iter().find(...)` at the
    /// callsite. The presence-probe axis now carries both refinements
    /// (bool via `has_condition_kind`, `&Condition` via
    /// `find_condition_kind`) at ONE typed algebra surface per struct.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the widened return lives at ONE substrate
    /// site so every downstream diagnostic consumer + coherence check
    /// binds through the SAME shape rather than restating the
    /// `.iter().find(|c| c.kind == K)` closure body).
    #[must_use]
    pub fn find_condition_kind(&self, kind: ConditionKind) -> Option<&Condition> {
        self.find_precondition_kind(kind)
            .or_else(|| self.find_postcondition_kind(kind))
    }

    /// Returns the first [`Condition`] in [`Self::preconditions`]
    /// carrying the given [`ConditionKind`], or `None` — the
    /// precondition-side arm of the (precondition, postcondition,
    /// condition-union) widened triad on [`Boundary`]. Thin typed
    /// delegate to [`ConditionSliceExt::find_kind`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`Self::find_postcondition_kind`] on the (precondition,
    /// postcondition) partition of the boundary's two condition-vector
    /// slots; both peers compose against the SAME slice-level substrate
    /// primitive and their `or_else` composition is
    /// [`Self::find_condition_kind`]. Byte-identical semantics to
    /// [`Self::has_precondition_kind`] with a widened `Option<&Condition>`
    /// return rather than a `bool`.
    #[must_use]
    pub fn find_precondition_kind(&self, kind: ConditionKind) -> Option<&Condition> {
        self.preconditions.find_kind(kind)
    }

    /// Returns the first [`Condition`] in [`Self::postconditions`]
    /// carrying the given [`ConditionKind`], or `None` — the
    /// postcondition-side arm of the (precondition, postcondition,
    /// condition-union) widened triad on [`Boundary`]. Thin typed
    /// delegate to [`ConditionSliceExt::find_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::find_precondition_kind`] on the (precondition,
    /// postcondition) partition of the boundary's two condition-vector
    /// slots. See [`Self::find_precondition_kind`] for the full
    /// rationale — the two methods share ONE lift motivation, ONE
    /// fail-before-pass-after composition-law pin, and ONE two-surface
    /// parity contract with the ephemeral sugar type via
    /// [`crate::ephemeral::EphemeralSpec::find_postcondition_kind`].
    #[must_use]
    pub fn find_postcondition_kind(&self, kind: ConditionKind) -> Option<&Condition> {
        self.postconditions.find_kind(kind)
    }

    /// Returns an iterator over every [`Condition`] in
    /// `preconditions ∪ postconditions` carrying the given
    /// [`ConditionKind`], walking preconditions first — the
    /// widened peer of [`Self::find_condition_kind`] one refinement
    /// higher on the presence-probe algebra. Byte-for-byte
    /// equivalent to
    /// `self.iter_precondition_kind(kind).chain(self.iter_postcondition_kind(kind))`.
    ///
    /// # Sibling to [`Self::find_condition_kind`]
    ///
    /// Same axis, one refinement wider: `find_condition_kind`
    /// collapses the return to the FIRST match (yielding
    /// `Option<&Condition>`); this method yields every match across
    /// both sides. Pinned by the composition law
    /// `find_condition_kind(K) == iter_condition_kind(K).next()` at
    /// [`Boundary`]'s substrate-delegation test — the two refinements
    /// share ONE walk order by construction (preconditions first,
    /// then postconditions), so a regression that reversed the
    /// [`Chain`](std::iter::Chain) order or narrowed the union to an
    /// intersection surfaces HERE at the substrate boundary rather
    /// than as silent skew between the first-match and stream
    /// refinements downstream consumers reach through.
    ///
    /// # Peer on the ephemeral surface — [`crate::ephemeral::EphemeralSpec::iter_condition_kind`]
    ///
    /// Same signature `(ConditionKind) -> Chain<KindMatches<'_>,
    /// KindMatches<'_>>`, same precondition-first chain body, on the
    /// sugar-surface type whose pre/post condition vectors live
    /// directly on the struct. Both methods compose against the SAME
    /// slice-level substrate primitive [`ConditionSliceExt::iter_kind`]
    /// — a regression at the per-slice walk fails at that primitive's
    /// tests rather than as silent drift at either struct-level
    /// widened caller.
    ///
    /// # Compounding
    ///
    /// A future coherence check that enforces "each
    /// [`ConditionKind`] appears at most once across
    /// preconditions ∪ postconditions" reads
    /// `boundary.iter_condition_kind(k).nth(1).is_none()` at ONE
    /// call site rather than restating the count-with-filter closure
    /// body over the two vector slots. A future diagnostic
    /// enumerating every match (an operator-facing "N ClosedLoopAuth
    /// conditions matched, listing sides + params" message emitted
    /// by the require-tag classifier) reaches this ONE method
    /// through `boundary.iter_condition_kind(k).collect()` rather
    /// than chaining two half-slice walks at the callsite.
    /// The presence-probe axis on [`Boundary`] now carries three
    /// refinements (bool via `has_condition_kind`,
    /// `Option<&Condition>` via `find_condition_kind`,
    /// `impl Iterator<Item = &Condition>` via
    /// `iter_condition_kind`) at ONE typed algebra surface, byte-
    /// for-byte peer of the same triad on
    /// [`crate::ephemeral::EphemeralSpec`].
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the widened stream lives at ONE substrate
    /// site so every downstream diagnostic + coherence consumer binds
    /// through the SAME shape rather than restating the two-half
    /// chain body).
    pub fn iter_condition_kind(
        &self,
        kind: ConditionKind,
    ) -> std::iter::Chain<KindMatches<'_>, KindMatches<'_>> {
        self.iter_precondition_kind(kind)
            .chain(self.iter_postcondition_kind(kind))
    }

    /// Returns an iterator over every [`Condition`] in
    /// [`Self::preconditions`] carrying the given [`ConditionKind`]
    /// — the precondition-side arm of the (precondition,
    /// postcondition, condition-union) iterator triad on
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::iter_kind`] over [`Self::preconditions`].
    ///
    /// Peer of [`Self::iter_postcondition_kind`] on the (precondition,
    /// postcondition) partition of the boundary's two condition-vector
    /// slots; both peers compose against the SAME slice-level substrate
    /// primitive and their [`Chain`](std::iter::Chain) composition is
    /// [`Self::iter_condition_kind`]. Byte-identical semantics to
    /// [`Self::find_precondition_kind`] with a widened stream return
    /// rather than only the first match.
    pub fn iter_precondition_kind(&self, kind: ConditionKind) -> KindMatches<'_> {
        self.preconditions.iter_kind(kind)
    }

    /// Returns an iterator over every [`Condition`] in
    /// [`Self::postconditions`] carrying the given [`ConditionKind`]
    /// — the postcondition-side arm of the (precondition,
    /// postcondition, condition-union) iterator triad on
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::iter_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::iter_precondition_kind`] on the (precondition,
    /// postcondition) partition of the boundary's two condition-vector
    /// slots. See [`Self::iter_precondition_kind`] for the full
    /// rationale — the two methods share ONE lift motivation, ONE
    /// fail-before-pass-after composition-law pin, and ONE
    /// two-surface parity contract with the ephemeral sugar type via
    /// [`crate::ephemeral::EphemeralSpec::iter_postcondition_kind`].
    pub fn iter_postcondition_kind(&self, kind: ConditionKind) -> KindMatches<'_> {
        self.postconditions.iter_kind(kind)
    }

    /// Number of [`Condition`]s in `preconditions ∪ postconditions`
    /// carrying the given [`ConditionKind`] — the scalar cardinality
    /// arm of the (precondition, postcondition, condition-union)
    /// count triad on [`Boundary`]. Composed as
    /// `count_precondition_kind(k) + count_postcondition_kind(k)` —
    /// the ONE SUM-composed arm on the presence-probe algebra
    /// (distinct from `has_condition_kind`'s `||` union,
    /// `find_condition_kind`'s `or_else` first-match, and
    /// `iter_condition_kind`'s `Chain` stream).
    ///
    /// # Sibling to [`Self::iter_condition_kind`]
    ///
    /// Same axis, one refinement lower on the cardinality projection:
    /// `iter_condition_kind` yields the whole match stream across both
    /// sides; this method collapses that stream to its cardinality
    /// without materializing any intermediate [`Vec`]. Composition law
    /// `count_condition_kind(K) == iter_condition_kind(K).count()`
    /// pinned as a first-class typed invariant at the substrate-
    /// delegation test.
    ///
    /// # Peer on the ephemeral surface — [`crate::ephemeral::EphemeralSpec::count_condition_kind`]
    ///
    /// Same signature `(ConditionKind) -> usize`, same SUM body, on
    /// the sugar-surface type whose pre/post condition vectors live
    /// directly on the struct. Both methods compose against the SAME
    /// slice-level substrate primitive [`ConditionSliceExt::count_kind`]
    /// — a regression at the per-slice count fails at that primitive's
    /// tests rather than as silent drift at either struct-level union
    /// caller.
    ///
    /// # Compounding
    ///
    /// A future coherence check that enforces "each [`ConditionKind`]
    /// appears at most once across preconditions ∪ postconditions"
    /// reads `boundary.count_condition_kind(k) <= 1` at ONE call site.
    /// A future require-tag classifier arm that surfaces multiplicity
    /// to the operator (a hypothetical `condition-count-<kind>` prefix
    /// family, an audit dump reporting "N ClosedLoopAuth conditions
    /// matched") reaches this ONE method rather than restating the
    /// `.iter_condition_kind(k).count()` chain body at the callsite.
    /// The presence-probe axis on [`Boundary`] now carries FOUR
    /// refinements (bool via `has_condition_kind`, `Option<&Condition>`
    /// via `find_condition_kind`, `impl Iterator<Item = &Condition>`
    /// via `iter_condition_kind`, `usize` via `count_condition_kind`)
    /// at ONE typed algebra surface per struct, byte-for-byte peer of
    /// the same tetrad on [`crate::ephemeral::EphemeralSpec`].
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the scalar cardinality lives at ONE
    /// substrate site so every downstream diagnostic + coherence
    /// consumer binds through the SAME shape rather than restating
    /// the two-half sum body).
    #[must_use]
    pub fn count_condition_kind(&self, kind: ConditionKind) -> usize {
        self.count_precondition_kind(kind) + self.count_postcondition_kind(kind)
    }

    /// Number of [`Condition`]s in [`Self::preconditions`] carrying
    /// the given [`ConditionKind`] — the precondition-side arm of the
    /// (precondition, postcondition, condition-union) count triad on
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::count_kind`] over [`Self::preconditions`].
    ///
    /// Peer of [`Self::count_postcondition_kind`] on the (precondition,
    /// postcondition) partition of the boundary's two condition-vector
    /// slots; both peers compose against the SAME slice-level substrate
    /// primitive and their `+` composition is
    /// [`Self::count_condition_kind`]. Byte-identical semantics to
    /// [`Self::iter_precondition_kind`] with the scalar `usize`
    /// cardinality projection rather than the widened stream.
    #[must_use]
    pub fn count_precondition_kind(&self, kind: ConditionKind) -> usize {
        self.preconditions.count_kind(kind)
    }

    /// Number of [`Condition`]s in [`Self::postconditions`] carrying
    /// the given [`ConditionKind`] — the postcondition-side arm of
    /// the (precondition, postcondition, condition-union) count triad
    /// on [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::count_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::count_precondition_kind`]. See that method for
    /// the full rationale — the two methods share ONE lift motivation,
    /// ONE fail-before-pass-after composition-law pin, and ONE
    /// two-surface parity contract with the ephemeral sugar type via
    /// [`crate::ephemeral::EphemeralSpec::count_postcondition_kind`].
    #[must_use]
    pub fn count_postcondition_kind(&self, kind: ConditionKind) -> usize {
        self.postconditions.count_kind(kind)
    }

    /// The set of [`ConditionKind`] variants that appear at least once in
    /// `preconditions ∪ postconditions`, projected in
    /// [`ConditionKind::ALL`] order — the closed-set-inversion refinement
    /// on the presence-probe algebra (distinct axis from the four point-
    /// probe refinements: bool via [`Self::has_condition_kind`],
    /// `Option<&Condition>` via [`Self::find_condition_kind`],
    /// `impl Iterator<Item = &Condition>` via [`Self::iter_condition_kind`],
    /// `usize` via [`Self::count_condition_kind`]).
    ///
    /// # Composed body
    ///
    /// `ConditionKind::ALL.into_iter().filter(|k|
    /// self.has_condition_kind(*k)).collect()` — a thin projection over
    /// the closed set composed against the two-slice union primitive
    /// [`Self::has_condition_kind`]. Equivalent to the set-union of
    /// [`Self::distinct_precondition_kinds`] and
    /// [`Self::distinct_postcondition_kinds`] projected in canonical
    /// [`ConditionKind::ALL`] order (the union composition law pinned by
    /// the substrate testkit macro [`crate::assert_surface_union_composition_laws`]).
    ///
    /// # Peer on the ephemeral surface — [`crate::ephemeral::EphemeralSpec::distinct_condition_kinds`]
    ///
    /// Same signature `(&Self) -> Vec<ConditionKind>`, same closed-set-
    /// inversion body, on the sugar-surface type whose pre/post condition
    /// vectors live directly on the struct. Both methods compose against
    /// the SAME slice-level substrate primitive
    /// [`ConditionSliceExt::distinct_kinds`] via the two-slice union
    /// composed through [`Self::has_condition_kind`] — a regression at
    /// the per-slice walk fails at that primitive's tests rather than as
    /// silent drift at either struct-level union caller.
    ///
    /// # Sibling to the four point-probe refinements
    ///
    /// FIFTH refinement on the boundary-surface presence-probe algebra,
    /// distinct in axis from the other four: `has_condition_kind` /
    /// `find_condition_kind` / `iter_condition_kind` /
    /// `count_condition_kind` fix a [`ConditionKind`] and vary the return
    /// type; this refinement INVERTS the axis by fixing the boundary and
    /// varying over [`ConditionKind::ALL`]. The composition law
    /// `distinct_condition_kinds().contains(&k) == has_condition_kind(k)`
    /// for every `k ∈ ConditionKind::ALL` binds the closed-set-inversion
    /// probe to the point probe at the (precondition, postcondition,
    /// condition-union) triad.
    ///
    /// # Compounding
    ///
    /// A future coherence check that enforces "every process boundary
    /// carries at least ONE distinct kind" (a warning surfaced when
    /// `spec.boundary.distinct_condition_kinds().is_empty()`) reaches
    /// this ONE method rather than paying for the eight-way sweep with
    /// `has_condition_kind` at every callsite. A future require-tag
    /// classifier that surfaces the distinct-set cardinality as a scalar
    /// (a hypothetical `condition-kinds-distinct-<n>` prefix family, an
    /// audit dump reporting "boundary carries N distinct kinds") reaches
    /// this ONE method through `.distinct_condition_kinds().len()`
    /// rather than restating the closed-set-inverted filter idiom at
    /// every callsite.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — the closed-set-inversion aggregate is a typed projection
    /// of [`Self::has_condition_kind`] over [`ConditionKind::ALL`], and
    /// every downstream aggregate consumer binds through the SAME shape).
    /// THEORY.md §VI.1 (generation over composition — a new
    /// [`ConditionKind`] variant added to `ALL` reaches this method
    /// mechanically through the closed-set walk).
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
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::distinct_kinds`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`Self::distinct_postcondition_kinds`] on the
    /// (precondition, postcondition) partition of the boundary's two
    /// condition-vector slots; both peers compose against the SAME
    /// slice-level substrate primitive and their canonical set-union
    /// (projected in [`ConditionKind::ALL`] order) is
    /// [`Self::distinct_condition_kinds`].
    #[must_use]
    pub fn distinct_precondition_kinds(&self) -> Vec<ConditionKind> {
        self.preconditions.distinct_kinds()
    }

    /// The set of [`ConditionKind`] variants appearing at least once in
    /// [`Self::postconditions`], projected in [`ConditionKind::ALL`]
    /// order — the postcondition-side arm of the (precondition,
    /// postcondition, condition-union) distinct-set triad on
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::distinct_kinds`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::distinct_precondition_kinds`]. See that method
    /// for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin, and
    /// ONE two-surface parity contract with the ephemeral sugar type
    /// via [`crate::ephemeral::EphemeralSpec::distinct_postcondition_kinds`].
    #[must_use]
    pub fn distinct_postcondition_kinds(&self) -> Vec<ConditionKind> {
        self.postconditions.distinct_kinds()
    }

    /// Scalar cardinality of the [`ConditionKind`] set appearing at
    /// least once in `preconditions ∪ postconditions` — the
    /// condition-union arm of the (precondition, postcondition,
    /// condition-union) distinct-kind-count triad on [`Boundary`].
    ///
    /// # Composed body
    ///
    /// `ConditionKind::ALL.iter().filter(|k|
    /// self.has_condition_kind(**k)).count()` — a thin projection over
    /// the closed set composed against the two-slice union primitive
    /// [`Self::has_condition_kind`], byte-identical to the trait-level
    /// [`ConditionSliceExt::distinct_kind_count`] but reaching through
    /// the boundary's two-slice union rather than a single slice.
    /// Equivalent to `self.distinct_condition_kinds().len()` without
    /// materializing the intermediate `Vec<ConditionKind>`.
    ///
    /// # Sibling to [`Self::distinct_condition_kinds`]
    ///
    /// Scalar projection of the closed-set-inversion widened primitive
    /// on the boundary-union surface — where `distinct_condition_kinds`
    /// returns the SET, `distinct_condition_kind_count` collapses it to
    /// its cardinality. Byte-for-byte peer of the point-domain scalar
    /// projection [`ConditionSliceExt::distinct_kind_count`] one
    /// struct-layer down, and of the peer surface sugar
    /// [`crate::ephemeral::EphemeralSpec::distinct_condition_kind_count`]
    /// one struct-layer sideways.
    ///
    /// # Compounding
    ///
    /// A future coherence check that enforces "every process boundary
    /// carries at least ONE distinct kind" now reads
    /// `spec.boundary.distinct_condition_kind_count() > 0` at ONE call
    /// site rather than paying for
    /// `spec.boundary.distinct_condition_kinds().len() > 0` (with its
    /// intermediate heap allocation) or the eight-way `has_*_kind`
    /// sweep at the callsite. A future require-tag classifier arm that
    /// publishes the distinct-set cardinality as a scalar (a
    /// hypothetical `condition-kinds-distinct-<n>` prefix family)
    /// reaches this ONE primitive without allocating.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs (the scalar cardinality composes the SAME
    /// closed-set walk on both this boundary surface and the
    /// slice-level substrate primitive). THEORY.md §VI.1 — generation
    /// over composition (a new [`ConditionKind`] variant added to
    /// `ALL` reaches this primitive mechanically through the closed-set
    /// walk).
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
    /// distinct-kind-count triad on [`Boundary`]. Thin typed delegate
    /// to [`ConditionSliceExt::distinct_kind_count`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`Self::distinct_postcondition_kind_count`] on the
    /// (precondition, postcondition) partition of the boundary's two
    /// condition-vector slots; both peers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-slice
    /// closed-set walk fails at that primitive's tests rather than as
    /// silent drift at either struct-level scalar-cardinality arm.
    #[must_use]
    pub fn distinct_precondition_kind_count(&self) -> usize {
        self.preconditions.distinct_kind_count()
    }

    /// Scalar cardinality of the [`ConditionKind`] set appearing at
    /// least once in [`Self::postconditions`] — the postcondition-side
    /// arm of the (precondition, postcondition, condition-union)
    /// distinct-kind-count triad on [`Boundary`]. Thin typed delegate
    /// to [`ConditionSliceExt::distinct_kind_count`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::distinct_precondition_kind_count`]. See that
    /// method for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin, and
    /// ONE two-surface parity contract with the ephemeral sugar type
    /// via
    /// [`crate::ephemeral::EphemeralSpec::distinct_postcondition_kind_count`].
    #[must_use]
    pub fn distinct_postcondition_kind_count(&self) -> usize {
        self.postconditions.distinct_kind_count()
    }

    /// The set of [`ConditionKind`] variants that do NOT appear in
    /// `preconditions ∪ postconditions`, projected in
    /// [`ConditionKind::ALL`] order — the closed-set-inversion
    /// COMPLEMENT of [`Self::distinct_condition_kinds`] on the
    /// (precondition, postcondition, condition-union) missing-set triad.
    ///
    /// # Composed body
    ///
    /// `ConditionKind::ALL.into_iter().filter(|k|
    /// !self.has_condition_kind(*k)).collect()` — a thin projection
    /// over the closed set composed against the two-slice union
    /// primitive [`Self::has_condition_kind`] under a negated
    /// predicate. Equivalent to the SET-INTERSECTION of
    /// [`Self::missing_precondition_kinds`] and
    /// [`Self::missing_postcondition_kinds`] projected in canonical
    /// [`ConditionKind::ALL`] order — a kind is missing from the
    /// union iff it is missing from BOTH half-slices (the union-
    /// composition law pinned by the substrate testkit macro
    /// [`crate::assert_surface_union_composition_laws`]).
    ///
    /// # Peer on the ephemeral surface — [`crate::ephemeral::EphemeralSpec::missing_condition_kinds`]
    ///
    /// Same signature `(&Self) -> Vec<ConditionKind>`, same closed-set-
    /// complement body, on the sugar-surface type. Both methods compose
    /// against the SAME slice-level substrate primitive
    /// [`ConditionSliceExt::missing_kinds`] via the two-slice union
    /// composed through [`Self::has_condition_kind`] — a regression at
    /// the per-slice walk fails at that primitive's tests rather than
    /// as silent drift at either struct-level complement caller.
    ///
    /// # Sibling to [`Self::distinct_condition_kinds`]
    ///
    /// SIXTH refinement on the boundary-surface presence-probe algebra,
    /// on the SAME closed-set-inversion axis as `distinct_condition_kinds`
    /// but under a NEGATED point-probe. The composition law
    /// `missing_condition_kinds().contains(&k) ==
    /// !has_condition_kind(k)` for every `k ∈ ConditionKind::ALL`
    /// binds the complement to the point probe at the triad — and the
    /// two widened primitives PARTITION `ConditionKind::ALL` (their
    /// union covers `ALL`, their intersection is empty, their
    /// cardinalities sum to `ALL.len()`).
    ///
    /// # Compounding
    ///
    /// A future coherence check that enforces "every process boundary
    /// carries a [`ConditionKind::JobAttested`] postcondition" surfaces
    /// the operator-facing gap diagnostic
    /// `spec.boundary.postconditions.missing_kinds()` verbatim (naming
    /// EVERY kind absent from postconditions in canonical order). A
    /// future operator-facing "boundary is MISSING [JobAttested,
    /// ClosedLoopAuth]" audit dump reads this ONE method rather than
    /// restating the negated closed-set walk at every consumer. A
    /// hypothetical `condition-kinds-missing-<n>` require-tag classifier
    /// prefix family that publishes the missing-set cardinality as a
    /// scalar reaches `.missing_condition_kinds().len()`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the closed-set complement is a typed
    /// projection of [`Self::has_condition_kind`] over
    /// [`ConditionKind::ALL`] under negation, and every downstream
    /// gap-analysis consumer binds through the SAME shape).
    /// THEORY.md §VI.1 (generation over composition — a new
    /// [`ConditionKind`] variant added to `ALL` reaches this method
    /// mechanically through the closed-set walk).
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
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::missing_kinds`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`Self::missing_postcondition_kinds`] on the
    /// (precondition, postcondition) partition of the boundary's two
    /// condition-vector slots; both peers compose against the SAME
    /// slice-level substrate primitive and their SET-INTERSECTION
    /// (projected in [`ConditionKind::ALL`] order) is
    /// [`Self::missing_condition_kinds`].
    #[must_use]
    pub fn missing_precondition_kinds(&self) -> Vec<ConditionKind> {
        self.preconditions.missing_kinds()
    }

    /// The set of [`ConditionKind`] variants that do NOT appear in
    /// [`Self::postconditions`], projected in [`ConditionKind::ALL`]
    /// order — the postcondition-side arm of the (precondition,
    /// postcondition, condition-union) missing-set triad on
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::missing_kinds`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::missing_precondition_kinds`]. See that method
    /// for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin, and
    /// ONE two-surface parity contract with the ephemeral sugar type
    /// via [`crate::ephemeral::EphemeralSpec::missing_postcondition_kinds`].
    #[must_use]
    pub fn missing_postcondition_kinds(&self) -> Vec<ConditionKind> {
        self.postconditions.missing_kinds()
    }

    /// Scalar cardinality of the [`ConditionKind`] set NOT appearing in
    /// `preconditions ∪ postconditions` — the condition-union arm of the
    /// (precondition, postcondition, condition-union) missing-kind-count
    /// triad on [`Boundary`].
    ///
    /// # Composed body
    ///
    /// `ConditionKind::ALL.iter().filter(|k|
    /// !self.has_condition_kind(**k)).count()` — a thin projection over
    /// the closed set composed against the two-slice union primitive
    /// [`Self::has_condition_kind`] under a NEGATED predicate, byte-
    /// identical to the trait-level
    /// [`ConditionSliceExt::missing_kind_count`] but reaching through
    /// the boundary's two-slice union rather than a single slice.
    /// Equivalent to `self.missing_condition_kinds().len()` without
    /// materializing the intermediate `Vec<ConditionKind>`.
    ///
    /// # Sibling to [`Self::missing_condition_kinds`] /
    /// [`Self::distinct_condition_kind_count`]
    ///
    /// Scalar projection of the closed-set-complement widened primitive
    /// on the boundary-union surface — where `missing_condition_kinds`
    /// returns the SET, `missing_condition_kind_count` collapses it to
    /// its cardinality. Byte-for-byte peer of the point-domain scalar
    /// projection [`ConditionSliceExt::missing_kind_count`] one struct-
    /// layer down, and of the peer surface sugar
    /// [`crate::ephemeral::EphemeralSpec::missing_condition_kind_count`]
    /// one struct-layer sideways.
    ///
    /// The scalar-partition composition law
    /// `distinct_condition_kind_count() + missing_condition_kind_count()
    /// == ConditionKind::ALL.len()` binds this method's return to its
    /// distinct-side peer through the closed-set cardinality — the
    /// scalar consequence of the widened-primitive partition law that
    /// [`assert_slice_refinement_composition_laws`] pins on each slice
    /// and that [`crate::assert_surface_union_composition_laws`] lifts
    /// to the two-slice union.
    ///
    /// # Compounding
    ///
    /// A future coherence check that enforces "every process boundary
    /// carries EVERY [`ConditionKind`] under some slot" now reads
    /// `spec.boundary.missing_condition_kind_count() == 0` at ONE call
    /// site rather than paying for
    /// `spec.boundary.missing_condition_kinds().is_empty()` (with its
    /// intermediate heap allocation) or the eight-way negated `has_*_kind`
    /// sweep at the callsite. A future require-tag classifier arm that
    /// publishes the missing-set cardinality as a scalar (a hypothetical
    /// `condition-kinds-missing-<n>` prefix family) reaches this ONE
    /// primitive without allocating.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs (the scalar cardinality composes the SAME
    /// closed-set walk under negation on both this boundary surface and
    /// the slice-level substrate primitive). THEORY.md §VI.1 —
    /// generation over composition (a new [`ConditionKind`] variant
    /// added to `ALL` reaches this primitive mechanically through the
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
    /// triad on [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::missing_kind_count`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`Self::missing_postcondition_kind_count`] on the
    /// (precondition, postcondition) partition of the boundary's two
    /// condition-vector slots; both peers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-slice
    /// negated closed-set walk fails at that primitive's tests rather
    /// than as silent drift at either struct-level scalar-cardinality
    /// arm.
    #[must_use]
    pub fn missing_precondition_kind_count(&self) -> usize {
        self.preconditions.missing_kind_count()
    }

    /// Scalar cardinality of the [`ConditionKind`] set NOT appearing in
    /// [`Self::postconditions`] — the postcondition-side arm of the
    /// (precondition, postcondition, condition-union) missing-kind-count
    /// triad on [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::missing_kind_count`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::missing_precondition_kind_count`]. See that
    /// method for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin, and
    /// ONE two-surface parity contract with the ephemeral sugar type
    /// via
    /// [`crate::ephemeral::EphemeralSpec::missing_postcondition_kind_count`].
    #[must_use]
    pub fn missing_postcondition_kind_count(&self) -> usize {
        self.postconditions.missing_kind_count()
    }

    /// Earliest [`ConditionKind::ALL`] entry present in
    /// `preconditions ∪ postconditions`, or `None` when neither side
    /// populates any variant — the union arm of the (precondition,
    /// postcondition, condition-union) first-distinct-kind triad on
    /// [`Boundary`].
    ///
    /// # Composed body
    ///
    /// `ConditionKind::ALL.iter().copied().find(|k|
    /// self.has_condition_kind(*k))` — a closed-set walk composed
    /// against the two-slice union primitive
    /// [`Self::has_condition_kind`] that SHORT-CIRCUITS at the earliest
    /// match. Byte-identical to the trait-level
    /// [`ConditionSliceExt::first_distinct_kind`] but reaching through
    /// the boundary's two-slice union rather than a single slice.
    /// Equivalent to `self.distinct_condition_kinds().first().copied()`
    /// without materializing the intermediate `Vec<ConditionKind>`.
    ///
    /// # Sibling to [`Self::distinct_condition_kinds`] /
    /// [`Self::distinct_condition_kind_count`]
    ///
    /// Third scalar projection of the closed-set-inversion widened
    /// primitive on the boundary-union surface: `distinct_condition_kinds`
    /// returns the SET, `distinct_condition_kind_count` collapses it to
    /// its cardinality, and `first_distinct_condition_kind` collapses
    /// it to its earliest element. Byte-for-byte peer of the point-domain
    /// scalar projection [`ConditionSliceExt::first_distinct_kind`] one
    /// struct-layer down, and of the peer surface sugar
    /// [`crate::ephemeral::EphemeralSpec::first_distinct_condition_kind`]
    /// one struct-layer sideways.
    ///
    /// # Compounding
    ///
    /// A future coherence check that surfaces "boundary starts with
    /// PromQL" reads `spec.boundary.first_distinct_condition_kind() ==
    /// Some(ConditionKind::PromQL)` at ONE call site rather than
    /// paying for `spec.boundary.distinct_condition_kinds().first() ==
    /// Some(&ConditionKind::PromQL)` (with its intermediate heap
    /// allocation) or the eight-way `has_*_kind` sweep at the callsite.
    /// A future require-tag classifier arm that publishes the earliest
    /// distinct kind as a scalar
    /// (`condition-kinds-first-distinct-<kind>`) reaches this ONE
    /// primitive without allocating.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs (the earliest-element projection composes the
    /// SAME closed-set walk on both this boundary surface and the
    /// slice-level substrate primitive under short-circuit semantics).
    /// THEORY.md §VI.1 — generation over composition (a new
    /// [`ConditionKind`] variant added to `ALL` reaches this primitive
    /// mechanically through the closed-set walk).
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
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::first_distinct_kind`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`Self::first_distinct_postcondition_kind`] on the
    /// (precondition, postcondition) partition of the boundary's two
    /// condition-vector slots; both peers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-slice
    /// short-circuit walk fails at that primitive's tests rather than
    /// as silent drift at either struct-level arm.
    #[must_use]
    pub fn first_distinct_precondition_kind(&self) -> Option<ConditionKind> {
        self.preconditions.first_distinct_kind()
    }

    /// Earliest [`ConditionKind::ALL`] entry present in
    /// [`Self::postconditions`], or `None` when postconditions carry no
    /// matching kind — the postcondition-side arm of the (precondition,
    /// postcondition, condition-union) first-distinct-kind triad on
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::first_distinct_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::first_distinct_precondition_kind`]. See that
    /// method for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin, and
    /// ONE two-surface parity contract with the ephemeral sugar type
    /// via
    /// [`crate::ephemeral::EphemeralSpec::first_distinct_postcondition_kind`].
    #[must_use]
    pub fn first_distinct_postcondition_kind(&self) -> Option<ConditionKind> {
        self.postconditions.first_distinct_kind()
    }

    /// Earliest [`ConditionKind::ALL`] entry ABSENT from
    /// `preconditions ∪ postconditions`, or `None` when the union
    /// carries every variant — the union arm of the (precondition,
    /// postcondition, condition-union) first-missing-kind triad on
    /// [`Boundary`].
    ///
    /// # Composed body
    ///
    /// `ConditionKind::ALL.iter().copied().find(|k|
    /// !self.has_condition_kind(*k))` — a closed-set walk composed
    /// against the two-slice union primitive
    /// [`Self::has_condition_kind`] under a NEGATED predicate that
    /// SHORT-CIRCUITS at the earliest empty slot. Byte-identical to the
    /// trait-level [`ConditionSliceExt::first_missing_kind`] but
    /// reaching through the boundary's two-slice union rather than a
    /// single slice. Equivalent to
    /// `self.missing_condition_kinds().first().copied()` without
    /// materializing the intermediate `Vec<ConditionKind>`.
    ///
    /// # Sibling to [`Self::missing_condition_kinds`] /
    /// [`Self::missing_condition_kind_count`]
    ///
    /// Third scalar projection of the closed-set-complement widened
    /// primitive on the boundary-union surface. Byte-for-byte peer of
    /// [`Self::first_distinct_condition_kind`] one axis over under a
    /// negated predicate: where `first_distinct_condition_kind` scalar-
    /// projects the closed-set-INVERSION widened primitive onto its
    /// earliest element, this method scalar-projects the closed-set-
    /// COMPLEMENT widened primitive onto its earliest element.
    ///
    /// # Compounding
    ///
    /// A future coherence check that surfaces "boundary starts missing
    /// ProcessPhase" reads `spec.boundary.first_missing_condition_kind()
    /// == Some(ConditionKind::ProcessPhase)` at ONE call site rather
    /// than paying for `spec.boundary.missing_condition_kinds().first()
    /// == Some(&ConditionKind::ProcessPhase)` (with its intermediate
    /// heap allocation). An operator-facing "first still-unfilled
    /// closed-loop kind" audit reaches this ONE substrate site rather
    /// than restating the negated closed-set walk at every consumer.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the complement-earliest-element projection
    /// composes the SAME closed-set walk on both this boundary surface
    /// and the slice-level substrate primitive under short-circuit
    /// semantics with a negated predicate). THEORY.md §VI.1
    /// (generation over composition — a new [`ConditionKind`] variant
    /// added to `ALL` reaches this primitive mechanically through the
    /// closed-set walk).
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
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::first_missing_kind`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`Self::first_missing_postcondition_kind`] on the
    /// (precondition, postcondition) partition of the boundary's two
    /// condition-vector slots; both peers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-slice
    /// negated short-circuit walk fails at that primitive's tests
    /// rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn first_missing_precondition_kind(&self) -> Option<ConditionKind> {
        self.preconditions.first_missing_kind()
    }

    /// Earliest [`ConditionKind::ALL`] entry ABSENT from
    /// [`Self::postconditions`], or `None` when postconditions carry
    /// every variant — the postcondition-side arm of the (precondition,
    /// postcondition, condition-union) first-missing-kind triad on
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::first_missing_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::first_missing_precondition_kind`]. See that
    /// method for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin, and
    /// ONE two-surface parity contract with the ephemeral sugar type
    /// via
    /// [`crate::ephemeral::EphemeralSpec::first_missing_postcondition_kind`].
    #[must_use]
    pub fn first_missing_postcondition_kind(&self) -> Option<ConditionKind> {
        self.postconditions.first_missing_kind()
    }

    /// Latest [`ConditionKind::ALL`] entry present in
    /// `preconditions ∪ postconditions`, or `None` when neither side
    /// populates any variant — the union arm of the (precondition,
    /// postcondition, condition-union) last-distinct-kind triad on
    /// [`Boundary`].
    ///
    /// # Composed body
    ///
    /// `ConditionKind::ALL.iter().rev().copied().find(|k|
    /// self.has_condition_kind(*k))` — a REVERSED closed-set walk
    /// composed against the two-slice union primitive
    /// [`Self::has_condition_kind`] that SHORT-CIRCUITS at the latest
    /// match. Byte-identical to the trait-level
    /// [`ConditionSliceExt::last_distinct_kind`] but reaching through
    /// the boundary's two-slice union rather than a single slice.
    /// Equivalent to `self.distinct_condition_kinds().last().copied()`
    /// without materializing the intermediate `Vec<ConditionKind>`.
    ///
    /// # Sibling to [`Self::first_distinct_condition_kind`]
    ///
    /// Time-reversed peer of the earliest-element scalar projection
    /// under the SAME two-slice union predicate. Together with
    /// `first_distinct_condition_kind` and the two `_missing_*` peers
    /// the four scalar-endpoint projections close the "endpoint of
    /// closed-set-inversion/complement widened primitive" refinement
    /// axis on the boundary-union surface.
    ///
    /// # Compounding
    ///
    /// A future coherence check that surfaces "boundary ends with
    /// ClosedLoopAuth" reads `spec.boundary.last_distinct_condition_kind()
    /// == Some(ConditionKind::ClosedLoopAuth)` at ONE call site rather
    /// than paying for `spec.boundary.distinct_condition_kinds().last()
    /// == Some(&…)` with its intermediate heap allocation. A future
    /// require-tag classifier arm that publishes the latest distinct
    /// kind as a scalar (`condition-kinds-last-distinct-<kind>`) reaches
    /// this ONE primitive without allocating.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs (the latest-element projection composes the
    /// SAME reversed closed-set walk on both this boundary surface and
    /// the slice-level substrate primitive under short-circuit
    /// semantics). THEORY.md §VI.1 — generation over composition (a
    /// new [`ConditionKind`] variant added to `ALL` reaches this
    /// primitive mechanically through the reversed closed-set walk).
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
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::last_distinct_kind`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`Self::last_distinct_postcondition_kind`] on the
    /// (precondition, postcondition) partition of the boundary's two
    /// condition-vector slots; both peers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-
    /// slice REVERSED short-circuit walk fails at that primitive's
    /// tests rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn last_distinct_precondition_kind(&self) -> Option<ConditionKind> {
        self.preconditions.last_distinct_kind()
    }

    /// Latest [`ConditionKind::ALL`] entry present in
    /// [`Self::postconditions`], or `None` when postconditions carry
    /// no matching kind — the postcondition-side arm of the
    /// (precondition, postcondition, condition-union) last-distinct-
    /// kind triad on [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::last_distinct_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::last_distinct_precondition_kind`]. See that
    /// method for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin, and
    /// ONE two-surface parity contract with the ephemeral sugar type
    /// via
    /// [`crate::ephemeral::EphemeralSpec::last_distinct_postcondition_kind`].
    #[must_use]
    pub fn last_distinct_postcondition_kind(&self) -> Option<ConditionKind> {
        self.postconditions.last_distinct_kind()
    }

    /// Latest [`ConditionKind::ALL`] entry ABSENT from
    /// `preconditions ∪ postconditions`, or `None` when the union
    /// carries every variant — the union arm of the (precondition,
    /// postcondition, condition-union) last-missing-kind triad on
    /// [`Boundary`].
    ///
    /// # Composed body
    ///
    /// `ConditionKind::ALL.iter().rev().copied().find(|k|
    /// !self.has_condition_kind(*k))` — a REVERSED closed-set walk
    /// composed against the two-slice union primitive
    /// [`Self::has_condition_kind`] under a NEGATED predicate that
    /// SHORT-CIRCUITS at the latest empty slot. Byte-identical to the
    /// trait-level [`ConditionSliceExt::last_missing_kind`] but
    /// reaching through the boundary's two-slice union rather than a
    /// single slice. Equivalent to
    /// `self.missing_condition_kinds().last().copied()` without
    /// materializing the intermediate `Vec<ConditionKind>`.
    ///
    /// # Sibling to [`Self::first_missing_condition_kind`]
    ///
    /// Time-reversed peer of the earliest-element scalar projection
    /// under the SAME negated two-slice union predicate. Fourth
    /// scalar projection on the closed-set-complement axis on the
    /// boundary-union surface (first, count, missing_kinds already
    /// shipped; this method closes the endpoint pair on the
    /// complement side).
    ///
    /// # Compounding
    ///
    /// A future coherence check that surfaces "boundary is latest-
    /// missing PromQL" reads
    /// `spec.boundary.last_missing_condition_kind() ==
    /// Some(ConditionKind::PromQL)` at ONE call site rather than
    /// paying for `spec.boundary.missing_condition_kinds().last()`
    /// with its intermediate heap allocation. An operator-facing
    /// "last still-unfilled closed-loop kind" audit reaches this ONE
    /// substrate site rather than restating the negated reversed
    /// closed-set walk at every consumer.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the complement-latest-element projection
    /// composes the SAME reversed closed-set walk on both this
    /// boundary surface and the slice-level substrate primitive
    /// under short-circuit semantics with a negated predicate).
    /// THEORY.md §VI.1 (generation over composition — a new
    /// [`ConditionKind`] variant added to `ALL` reaches this
    /// primitive mechanically through the reversed closed-set walk).
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
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::last_missing_kind`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`Self::last_missing_postcondition_kind`] on the
    /// (precondition, postcondition) partition of the boundary's two
    /// condition-vector slots; both peers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-
    /// slice negated REVERSED short-circuit walk fails at that
    /// primitive's tests rather than as silent drift at either
    /// struct-level arm.
    #[must_use]
    pub fn last_missing_precondition_kind(&self) -> Option<ConditionKind> {
        self.preconditions.last_missing_kind()
    }

    /// Latest [`ConditionKind::ALL`] entry ABSENT from
    /// [`Self::postconditions`], or `None` when postconditions carry
    /// every variant — the postcondition-side arm of the (precondition,
    /// postcondition, condition-union) last-missing-kind triad on
    /// [`Boundary`]. Thin typed delegate to
    /// [`ConditionSliceExt::last_missing_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::last_missing_precondition_kind`]. See that
    /// method for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin, and
    /// ONE two-surface parity contract with the ephemeral sugar type
    /// via
    /// [`crate::ephemeral::EphemeralSpec::last_missing_postcondition_kind`].
    #[must_use]
    pub fn last_missing_postcondition_kind(&self) -> Option<ConditionKind> {
        self.postconditions.last_missing_kind()
    }

    /// `true` iff `preconditions ∪ postconditions` carries every
    /// [`ConditionKind::ALL`] variant at least once — the union arm
    /// of the (precondition, postcondition, condition-union)
    /// saturation-predicate triad on [`Boundary`].
    ///
    /// # Composed body
    ///
    /// `ConditionKind::ALL.iter().all(|k| self.has_condition_kind(*k))`
    /// — a SHORT-CIRCUITING closed-set walk composed against the
    /// two-slice union primitive [`Self::has_condition_kind`], byte-
    /// identical to the trait-level [`ConditionSliceExt::is_kind_saturated`]
    /// but reaching through the boundary's two-slice union rather than
    /// a single slice. Equivalent to `self.missing_condition_kinds()
    /// .is_empty()` without materializing the `Vec<ConditionKind>`, and
    /// to `self.missing_condition_kind_count() == 0` without paying for
    /// the counter walk on every arm.
    ///
    /// # Peer on the ephemeral surface — [`crate::ephemeral::EphemeralSpec::is_condition_kind_saturated`]
    ///
    /// Byte-identical signature `(&Self) -> bool`, byte-identical
    /// closed-set-walk body, on the sugar-surface type whose pre/post
    /// condition vectors live directly on the struct. Both methods
    /// compose against the SAME slice-level substrate primitive
    /// [`ConditionSliceExt::is_kind_saturated`] via the two-slice
    /// union composed through [`Self::has_condition_kind`] — a
    /// regression at the per-slice `all` short-circuit fails at that
    /// primitive's tests rather than as silent drift at either
    /// struct-level saturation caller.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs (the saturation-endpoint projection composes
    /// the SAME closed-set walk on both this boundary surface and the
    /// slice-level substrate primitive under short-circuit semantics).
    /// THEORY.md §VI.1 — generation over composition (a new
    /// [`ConditionKind`] variant added to `ALL` reaches this primitive
    /// mechanically through the `all` short-circuit).
    #[must_use]
    pub fn is_condition_kind_saturated(&self) -> bool {
        ConditionKind::ALL
            .iter()
            .all(|k| self.has_condition_kind(*k))
    }

    /// `true` iff [`Self::preconditions`] carries every
    /// [`ConditionKind::ALL`] variant at least once — the precondition-
    /// side arm of the (precondition, postcondition, condition-union)
    /// saturation-predicate triad on [`Boundary`]. Thin typed delegate
    /// to [`ConditionSliceExt::is_kind_saturated`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`Self::is_postcondition_kind_saturated`] on the
    /// (precondition, postcondition) partition of the boundary's two
    /// condition-vector slots; both peers compose against the SAME
    /// slice-level substrate primitive so a regression at the per-
    /// slice `all` short-circuit fails at that primitive's tests
    /// rather than as silent drift at either struct-level arm.
    #[must_use]
    pub fn is_precondition_kind_saturated(&self) -> bool {
        self.preconditions.is_kind_saturated()
    }

    /// `true` iff [`Self::postconditions`] carries every
    /// [`ConditionKind::ALL`] variant at least once — the postcondition-
    /// side arm of the (precondition, postcondition, condition-union)
    /// saturation-predicate triad on [`Boundary`]. Thin typed delegate
    /// to [`ConditionSliceExt::is_kind_saturated`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::is_precondition_kind_saturated`]. See that
    /// method for the full rationale — the two methods share ONE lift
    /// motivation, ONE fail-before-pass-after composition-law pin, and
    /// ONE two-surface parity contract with the ephemeral sugar type
    /// via
    /// [`crate::ephemeral::EphemeralSpec::is_postcondition_kind_saturated`].
    #[must_use]
    pub fn is_postcondition_kind_saturated(&self) -> bool {
        self.postconditions.is_kind_saturated()
    }

    /// `true` iff `preconditions ∪ postconditions` carries NO
    /// [`Condition`] with the given [`ConditionKind`] — the union arm
    /// of the (precondition, postcondition, condition-union)
    /// per-kind-complement triad on [`Boundary`], definitional
    /// negation of [`Self::has_condition_kind`].
    ///
    /// # Composed body
    ///
    /// `!self.has_condition_kind(kind)` — the definitional negation
    /// of the two-slice union primitive. Equivalent to the AND of the
    /// two half-slice per-kind-complement arms
    /// (`self.lacks_precondition_kind(k) && self.lacks_postcondition_kind(k)`),
    /// by the boolean identity `!(a || b) == !a && !b`. Both forms
    /// return `true` iff BOTH slices lack the addressed kind; the
    /// composed body chosen here short-circuits through the union
    /// primitive so a regression at the per-slice presence probe fails
    /// at that primitive's tests rather than as silent drift at either
    /// half-slice complement arm. Equivalent to
    /// `self.missing_condition_kinds().contains(&kind)` without
    /// materializing the closed-set-complement Vec at every callsite.
    ///
    /// # Peer on the ephemeral surface — [`crate::ephemeral::EphemeralSpec::lacks_condition_kind`]
    ///
    /// Byte-identical signature `(&Self, ConditionKind) -> bool`,
    /// byte-identical `!self.has_condition_kind(kind)` body, on the
    /// sugar-surface type whose pre/post condition vectors live
    /// directly on the struct. Both methods compose against the SAME
    /// slice-level substrate primitive
    /// [`ConditionSliceExt::lacks_kind`] via the two-slice union
    /// composed through [`Self::has_condition_kind`] — a regression
    /// at the per-slice negation fails at that primitive's tests
    /// rather than as silent drift at either struct-level complement
    /// caller.
    ///
    /// # Compounding
    ///
    /// A `lacks-<kind>` require-tag classifier arm — byte-for-byte
    /// peer of the tagged-union `lacks-<kind>` classifier one struct-
    /// layer up + the future `condition-<kind>` require-tag family's
    /// negated dual — reaches this primitive at ONE call site rather
    /// than negating `boundary.has_condition_kind(k)` at the callsite
    /// or restating `boundary.missing_condition_kinds().contains(&k)`
    /// with its allocation.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the per-kind closed-set-complement
    /// projection composes the SAME two-slice union negation on both
    /// this boundary surface and the slice-level substrate primitive
    /// under definitional negation). THEORY.md §VI.1 (generation over
    /// composition — a new [`ConditionKind`] variant reaches both
    /// surfaces' complement-triads mechanically through the delegated
    /// union primitive).
    #[must_use]
    pub fn lacks_condition_kind(&self, kind: ConditionKind) -> bool {
        !self.has_condition_kind(kind)
    }

    /// `true` iff [`Self::preconditions`] carries NO [`Condition`]
    /// with the given [`ConditionKind`] — the precondition-side arm
    /// of the (precondition, postcondition, condition-union)
    /// per-kind-complement triad on [`Boundary`]. Thin typed delegate
    /// to [`ConditionSliceExt::lacks_kind`] over
    /// [`Self::preconditions`].
    ///
    /// Peer of [`Self::lacks_postcondition_kind`] on the (precondition,
    /// postcondition) partition of the boundary's two condition-vector
    /// slots; both peers compose against the SAME slice-level substrate
    /// primitive so a regression at the per-slice negation fails at
    /// that primitive's tests rather than as silent drift at either
    /// struct-level arm.
    #[must_use]
    pub fn lacks_precondition_kind(&self, kind: ConditionKind) -> bool {
        self.preconditions.lacks_kind(kind)
    }

    /// `true` iff [`Self::postconditions`] carries NO [`Condition`]
    /// with the given [`ConditionKind`] — the postcondition-side arm
    /// of the (precondition, postcondition, condition-union)
    /// per-kind-complement triad on [`Boundary`]. Thin typed delegate
    /// to [`ConditionSliceExt::lacks_kind`] over
    /// [`Self::postconditions`].
    ///
    /// Peer of [`Self::lacks_precondition_kind`]. See that method for
    /// the full rationale — the two methods share ONE lift motivation,
    /// ONE fail-before-pass-after composition-law pin, and ONE
    /// two-surface parity contract with the ephemeral sugar type via
    /// [`crate::ephemeral::EphemeralSpec::lacks_postcondition_kind`].
    #[must_use]
    pub fn lacks_postcondition_kind(&self, kind: ConditionKind) -> bool {
        self.postconditions.lacks_kind(kind)
    }
}

/// Slice-level `(ConditionKind, presence)` probe on any `&[Condition]`
/// — the ONE substrate primitive that owns the
/// `.iter().any(|c| c.kind == K)` walk shape both current production
/// sites hand-authored past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// threshold. Callers compose the two-half union at their site
/// ([`Boundary::has_condition_kind`] on `preconditions ∪
/// postconditions`) or on ONE half only (the ephemeral require-tag
/// classifier's `closed-loop-auth` arm on `spec.postconditions`) —
/// the primitive owns ONLY the per-slice walk, so the composition
/// choice stays typed at the caller.
///
/// # Why lift
///
/// Pre-lift the `.iter().any(|c| c.kind == K)` walk lived
/// hand-authored at THREE production sites: twice inside
/// [`Boundary::has_condition_kind`]'s union (pre + post), once at
/// `evaluate_ephemeral_require_tag`'s `closed-loop-auth` arm in
/// `tatara-reconciler::bin::tatara-check` (with `matches!` sugar
/// instead of `==`, but the same predicate). The (`&[Condition]`,
/// `ConditionKind`) → `bool` shape is the substrate primitive: a
/// future consumer that walks a `Vec<Condition>` (a coherence check
/// that verifies "every `ClosedLoopAuth` postcondition carries an
/// `issuer` param key", an editor completion listing which
/// [`ConditionKind`] arms appear on ONE side only, a hypothetical
/// `postcondition-<kind>` require-tag prefix family that dispatches
/// on `postconditions` alone — the peer of the existing
/// `condition-<kind>` family that dispatches on the pre ∪ post union
/// via [`Boundary::has_condition_kind`]) reaches this ONE primitive
/// through `slice.has_kind(k)` instead of restating the `.iter().any`
/// closure body.
///
/// # Sibling to [`Boundary::has_condition_kind`]
///
/// Same axis, one refinement lower: `Boundary::has_condition_kind` is
/// the two-slice-union probe; `has_kind` here is the one-slice probe
/// the union composes twice. A future normalization at the presence
/// probe shape (widening the return to `Option<&Condition>` for
/// deeper diagnostics, adding a debug-build assertion on redundant
/// duplicates, switching to a linear scan that also counts matches)
/// lands at ONE site here — both [`Boundary::has_condition_kind`] +
/// every downstream `slice.has_kind(K)` callsite pick it up
/// mechanically.
///
/// # Compounding
///
/// [`Self::find_kind`] is the widened primitive returning
/// `Option<&Condition>` that both `has_kind` (`self.find_kind(k).
/// is_some()`, the default body) and future diagnostic consumers
/// compose against. A `has_kind_matching(|&Condition| -> bool)`
/// predicate extension similarly lands as ONE new default method on
/// this trait — the closed-set discriminator case becomes `has_kind(k)
/// == self.has_kind_matching(|c| c.kind == k)` by construction, so a
/// regression that drifted one from the other becomes structurally
/// impossible past the trait boundary.
///
/// Theory anchor: THEORY.md §II.1 invariant 5 — composition preserves
/// proofs; the per-slice walk lives at ONE substrate site so the
/// two-half union in [`Boundary`] and the one-half probe on
/// [`crate::ephemeral::EphemeralSpec::postconditions`] compose
/// through the SAME primitive. THEORY.md §VI.1 — generation over
/// composition; a future `Vec<Condition>` consumer reaches the
/// primitive through `slice.has_kind(k)` with no per-caller
/// restatement of the `.iter().any(|c| c.kind == K)` closure body.
pub trait ConditionSliceExt {
    /// Returns an iterator yielding every [`Condition`] in this slice
    /// whose [`Condition::kind`] equals `kind`, in slice order — the
    /// ONE widened primitive on the slice-level presence-probe axis
    /// that both [`Self::find_kind`] (via the default
    /// `iter_kind(k).next()` body) and [`Self::has_kind`] (via the
    /// transitive `find_kind(k).is_some()` default) compose against.
    ///
    /// # Sibling to [`Self::find_kind`]
    ///
    /// One refinement wider: `find_kind` collapses the return to
    /// `Option<&Condition>` (yielding only the earliest match);
    /// `iter_kind` returns the whole match stream so callers can
    /// [`count`](Iterator::count) it, [`collect`](Iterator::collect)
    /// it into a `Vec<&Condition>`, ask for the
    /// [`nth`](Iterator::nth) element, or compose it with any other
    /// std iterator adaptor without re-walking the slice. The default
    /// body of `find_kind` is `self.iter_kind(kind).next()` — the
    /// two methods share ONE walk semantics by construction, so a
    /// regression that drifted the first-match probe from the
    /// widened stream becomes structurally impossible past the
    /// trait boundary.
    ///
    /// # Semantics
    ///
    /// Yields `&c` for each `c` in this slice with `c.kind == kind`,
    /// in slice order — a slice that carries multiple matches yields
    /// each in turn (the composition law
    /// `find_kind(k) == iter_kind(k).next()` binds the first match
    /// to the earliest position). An empty slice, or a slice with no
    /// matching kind, yields nothing. Byte-for-byte equivalent to
    /// `self.iter().filter(|c| c.kind == kind)`.
    ///
    /// # Compounding
    ///
    /// A future coherence check that verifies "each
    /// [`ConditionKind`] appears at most once per side" reads
    /// `slice.iter_kind(k).nth(1).is_none()` at ONE call site
    /// rather than restating the count-with-filter closure body.
    /// A future diagnostic that enumerates every match of a kind
    /// (an operator-facing "3 PromQL preconditions matched" message,
    /// an audit dump listing every match of a repeated kind) reaches
    /// this ONE primitive through `slice.iter_kind(k).collect()`
    /// rather than re-walking the slice with `.iter().filter(...)`
    /// at the callsite. The presence-probe axis now carries three
    /// refinements (bool via `has_kind`, `Option<&Condition>` via
    /// `find_kind`, `impl Iterator<Item = &Condition>` via
    /// `iter_kind`) at ONE typed algebra surface — every downstream
    /// consumer picks the coarsest one that answers its question and
    /// the coarser ones stay compositionally derived from this
    /// primitive.
    fn iter_kind(&self, kind: ConditionKind) -> KindMatches<'_>;

    /// Returns the first [`Condition`] in this slice that carries the
    /// given [`ConditionKind`], or `None` if none matches. Default
    /// body: `self.iter_kind(kind).next()` — a thin projection of the
    /// widened primitive [`Self::iter_kind`] onto its first element.
    /// The composition law `find_kind(k) == iter_kind(k).next()`
    /// binds the first-match probe to the widened stream at the
    /// trait's default body.
    ///
    /// # Sibling to [`Self::has_kind`]
    ///
    /// One refinement wider: `has_kind` collapses the return to a
    /// `bool`; `find_kind` returns the matching `&Condition` so
    /// callers can read [`Condition::params`] without re-walking the
    /// slice. The default body of `has_kind` is
    /// `self.find_kind(kind).is_some()` — the two methods share ONE
    /// walk semantics by construction. Byte-for-byte equivalent to
    /// `self.iter().find(|c| c.kind == kind)`.
    fn find_kind(&self, kind: ConditionKind) -> Option<&Condition> {
        self.iter_kind(kind).next()
    }

    /// True iff at least one [`Condition`] in this slice carries the
    /// given [`ConditionKind`]. Default body: `self.find_kind(kind).
    /// is_some()`. The single-slice presence probe both
    /// [`Boundary::has_condition_kind`] (twice, in a union) and the
    /// ephemeral `closed-loop-auth` require-tag arm (once, on
    /// postconditions only) compose against.
    fn has_kind(&self, kind: ConditionKind) -> bool {
        self.find_kind(kind).is_some()
    }

    /// Number of [`Condition`]s in this slice carrying the given
    /// [`ConditionKind`] — the scalar cardinality refinement on the
    /// slice-level presence-probe axis. Default body:
    /// `self.iter_kind(kind).count()` — a thin projection of the
    /// widened primitive [`Self::iter_kind`] onto its cardinality.
    ///
    /// # Sibling to [`Self::iter_kind`] / [`Self::find_kind`] / [`Self::has_kind`]
    ///
    /// Fourth refinement on the presence-probe algebra: `iter_kind`
    /// yields the whole match stream, `find_kind` collapses it to the
    /// first match, `has_kind` collapses that to a `bool`, and
    /// `count_kind` collapses the stream to its cardinality without
    /// materializing any intermediate [`Vec`] or `Option`. The
    /// composition laws
    /// `count_kind(k) == iter_kind(k).count()`,
    /// `has_kind(k) == (count_kind(k) > 0)`, and
    /// `find_kind(k).is_some() == (count_kind(k) > 0)`
    /// share ONE walk semantics by construction; a regression that
    /// drifted the cardinality probe from the widened stream becomes
    /// structurally impossible past the trait boundary.
    ///
    /// # Semantics
    ///
    /// Returns `self.iter().filter(|c| c.kind == kind).count()` — a
    /// slice that carries multiple matches returns that count, an
    /// empty slice or a slice with no matching kind returns `0`.
    ///
    /// # Compounding
    ///
    /// A future coherence check that verifies "each [`ConditionKind`]
    /// appears at most once per side" now reads
    /// `slice.count_kind(k) <= 1` at ONE call site rather than
    /// restating either `slice.iter_kind(k).nth(1).is_none()` or the
    /// `iter_kind(k).count() <= 1` idiom. A future require-tag
    /// classifier arm that surfaces multiplicity to the operator
    /// (a hypothetical `condition-count-<kind>` prefix family that
    /// publishes the raw cardinality, an audit dump reporting "3
    /// PromQL preconditions matched") reaches this ONE primitive
    /// through `slice.count_kind(k)` rather than restating the
    /// `.iter_kind(k).count()` chain body at the callsite. The
    /// presence-probe axis now carries FOUR refinements at ONE typed
    /// algebra surface — every downstream consumer picks the coarsest
    /// one that answers its question and the coarser ones stay
    /// compositionally derived from [`Self::iter_kind`].
    fn count_kind(&self, kind: ConditionKind) -> usize {
        self.iter_kind(kind).count()
    }

    /// The set of [`ConditionKind`] variants that appear at least once in
    /// this slice, projected in [`ConditionKind::ALL`] order — the
    /// closed-set-inversion refinement on the slice-level presence-probe
    /// axis. Default body: `ConditionKind::ALL.into_iter().filter(|k|
    /// self.has_kind(*k)).collect()` — a thin projection over the closed
    /// set that composes against [`Self::has_kind`] per variant.
    ///
    /// # Sibling to [`Self::has_kind`] / [`Self::find_kind`] / [`Self::iter_kind`] / [`Self::count_kind`]
    ///
    /// FIFTH refinement on the presence-probe algebra, distinct in axis
    /// from the other four: `has_kind` / `find_kind` / `iter_kind` /
    /// `count_kind` fix a [`ConditionKind`] and vary the return type
    /// (bool / `Option<&Condition>` / `impl Iterator<Item = &Condition>` /
    /// `usize`); this refinement INVERTS the axis by fixing the slice and
    /// varying over [`ConditionKind::ALL`], returning the SET of present
    /// kinds. The composition law
    /// `distinct_kinds().contains(&k) == has_kind(k)` for every
    /// `k ∈ ConditionKind::ALL` binds the closed-set-inversion probe to
    /// the point probe at the trait's default body.
    ///
    /// # Semantics — canonical subsequence of [`ConditionKind::ALL`]
    ///
    /// Returns a `Vec<ConditionKind>` whose elements appear in
    /// [`ConditionKind::ALL`] order with no duplicates. A slice that
    /// carries the same [`ConditionKind`] at multiple positions
    /// contributes ONE entry to the returned set (the closed-set
    /// projection collapses multiplicity — a caller that needs the
    /// per-kind cardinality reaches for [`Self::count_kind`]). An
    /// empty slice, or a slice with no matching kind under any
    /// [`ConditionKind::ALL`] variant, returns an empty vec.
    ///
    /// # Why closed-set-inversion is a distinct axis
    ///
    /// The other four refinements answer "for THIS kind, how does the
    /// slice populate the probe's return type?"; this refinement
    /// answers "for THIS slice, which kinds appear at least once?".
    /// A consumer that needs to enumerate every present kind for an
    /// audit dump (`"boundary carries [PromQL, ClosedLoopAuth]"`), a
    /// coherence check that verifies "every process's boundary carries
    /// at least ONE of {`JobAttested`, `ClosedLoopAuth`}", or a
    /// require-tag family that surfaces the distinct-set as a whole
    /// (`condition-kinds-distinct-count`) reaches this refinement
    /// rather than paying for a per-kind sweep with `has_kind` at the
    /// callsite. The point probe stays composable one axis over
    /// (`slice.has_kind(k)` for a fixed `k`); the aggregate refinement
    /// lives at the same trait, one axis away.
    ///
    /// # Compounding
    ///
    /// A future coherence check that enforces "every boundary carries
    /// at least ONE distinct kind" (a warning surfaced when
    /// `boundary.distinct_condition_kinds().is_empty()`) reaches this
    /// ONE primitive rather than paying for the eight-way
    /// `for k in ConditionKind::ALL { if boundary.has_condition_kind(k)
    /// { return true; } }` sweep at every callsite. A future require-
    /// tag classifier arm that publishes the distinct-set cardinality
    /// as a scalar (a hypothetical `condition-kinds-distinct-<n>`
    /// prefix family, an audit dump reporting "boundary carries N
    /// distinct kinds") reaches this ONE primitive through
    /// `boundary.distinct_condition_kinds().len()` rather than
    /// restating the closed-set-inverted `.iter().filter(...).count()`
    /// idiom at every callsite. The presence-probe axis now carries
    /// FIVE refinements at ONE typed algebra surface — the four point-
    /// probes fixing a kind AND the ONE closed-set-inversion probe
    /// fixing a slice — every downstream consumer picks the one that
    /// answers its question and the others stay compositionally
    /// derived from the single-source-of-truth widened primitive.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The closed-set-inversion projection lives at ONE substrate
    ///   site as a typed projection of [`Self::has_kind`] over the
    ///   closed set [`ConditionKind::ALL`]. Every downstream aggregate
    ///   consumer binds through the SAME shape rather than restating
    ///   the ALL-filter closure body.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`ConditionKind`] variant added to `ALL` reaches this
    ///   primitive mechanically (the closed-set walk picks up the new
    ///   entry) and every downstream consumer sees the wider set
    ///   without further per-caller edit.
    fn distinct_kinds(&self) -> Vec<ConditionKind> {
        ConditionKind::ALL
            .into_iter()
            .filter(|k| self.has_kind(*k))
            .collect()
    }

    /// Scalar cardinality projection of [`Self::distinct_kinds`] onto
    /// its `.len()` — the number of [`ConditionKind`] variants that
    /// appear at least once in this slice. Default body:
    /// `ConditionKind::ALL.iter().filter(|k| self.has_kind(**k)).count()`
    /// — a closed-set walk that composes against [`Self::has_kind`] per
    /// variant WITHOUT materializing an intermediate `Vec<ConditionKind>`.
    /// A slice that carries the same [`ConditionKind`] at multiple
    /// positions contributes `1` to the count (the closed-set projection
    /// collapses multiplicity — a caller that needs the per-kind
    /// cardinality reaches for [`Self::count_kind`]).
    ///
    /// # Sibling to [`Self::distinct_kinds`]
    ///
    /// Scalar projection of the closed-set-inversion widened primitive
    /// — where `distinct_kinds` returns the SET (a `Vec<ConditionKind>`
    /// in canonical [`ConditionKind::ALL`] order), `distinct_kind_count`
    /// collapses that set to its cardinality. The composition law
    /// `distinct_kind_count() == distinct_kinds().len()` binds the
    /// scalar projection to the widened primitive at the trait's
    /// default body and is swept substrate-wide by
    /// [`assert_slice_refinement_composition_laws`] as its sixth arm.
    ///
    /// # Peer to [`crate::tagged_union::TaggedUnion::populated_kind_count`]
    ///
    /// Same shape at the peer axis one struct layer up: where
    /// `populated_kind_count` scalar-projects `populated_kinds` on the
    /// tagged-union parent-level closed-set-inversion axis,
    /// `distinct_kind_count` scalar-projects `distinct_kinds` on the
    /// slice-level closed-set-inversion axis. The two primitives close
    /// the scalar-cardinality refinement at two adjacent typescape
    /// sites — one per closed-set-addressed slice-level refinement,
    /// one per closed-set-addressed tagged-union parent-level
    /// refinement — through the SAME `ClosedSet::ALL`-walk shape.
    ///
    /// # Compounding future consumers
    ///
    /// - A future coherence check that enforces "every boundary carries
    ///   at least ONE distinct kind" now reads
    ///   `slice.distinct_kind_count() > 0` at ONE call site rather than
    ///   paying for `slice.distinct_kinds().len() > 0` (with its
    ///   intermediate heap allocation) or the eight-way sweep with
    ///   `has_kind` at the callsite.
    /// - A future require-tag classifier arm that surfaces the
    ///   distinct-set cardinality as a scalar (a hypothetical
    ///   `condition-kinds-distinct-<n>` prefix family named in
    ///   [`Self::distinct_kinds`]'s doc-comment as a compounding-future
    ///   consumer) reaches this ONE primitive without allocating.
    /// - A future audit dump reporting "boundary carries N distinct
    ///   kinds" reaches `slice.distinct_kind_count()` directly rather
    ///   than restating the `.iter().filter(...).count()` closure body.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The scalar cardinality lives at ONE substrate site as a typed
    ///   projection of [`Self::distinct_kinds`] onto its `.len()`, and
    ///   the default body composes against [`Self::has_kind`] over the
    ///   closed set [`ConditionKind::ALL`] byte-identically to
    ///   `distinct_kinds` without the intermediate `Vec`. Every
    ///   downstream aggregate consumer binds through the SAME shape
    ///   rather than paying for the allocation to reach the
    ///   cardinality.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`ConditionKind`] variant added to `ALL` reaches this
    ///   primitive mechanically (the closed-set walk picks up the new
    ///   entry) and every downstream consumer sees the wider
    ///   cardinality without further per-caller edit.
    fn distinct_kind_count(&self) -> usize {
        ConditionKind::ALL
            .iter()
            .filter(|k| self.has_kind(**k))
            .count()
    }

    /// The set of [`ConditionKind`] variants that do NOT appear in this
    /// slice, projected in [`ConditionKind::ALL`] order — the closed-
    /// set-inversion COMPLEMENT of [`Self::distinct_kinds`]. Default
    /// body: `ConditionKind::ALL.into_iter().filter(|k|
    /// !self.has_kind(*k)).collect()` — a thin projection over the
    /// closed set that composes against [`Self::has_kind`] per variant
    /// under a negated predicate.
    ///
    /// # Sibling to [`Self::distinct_kinds`]
    ///
    /// Complement peer of the closed-set-inversion widened primitive on
    /// the slice-level presence-probe axis. Where `distinct_kinds`
    /// returns the SET of kinds that DO appear at least once,
    /// `missing_kinds` returns the SET of kinds that DO NOT appear.
    /// Both walk [`ConditionKind::ALL`] in canonical order and compose
    /// against the same [`Self::has_kind`] point probe. The two
    /// widened primitives PARTITION [`ConditionKind::ALL`]: their union
    /// equals `ConditionKind::ALL`, their intersection is empty, and
    /// their cardinalities sum to `ConditionKind::ALL.len()` — three
    /// composition laws pinned as the seventh, eighth, and ninth arms
    /// of the substrate testkit
    /// [`assert_slice_refinement_composition_laws`].
    ///
    /// # Peer to [`crate::tagged_union::TaggedUnion::populated_kinds`]'s
    /// hypothetical `unpopulated_kinds` complement
    ///
    /// Same shape at the peer axis one struct layer up: fixing the
    /// parent-side carrier and inverting the presence probe over the
    /// closed set. The two primitives close the "closed-set complement"
    /// refinement at two adjacent typescape sites — one per closed-set-
    /// addressed slice-level refinement (this primitive), one per
    /// closed-set-addressed tagged-union parent-level refinement (a
    /// symmetric future addition).
    ///
    /// # Semantics — canonical subsequence of [`ConditionKind::ALL`]
    ///
    /// Returns a `Vec<ConditionKind>` whose elements appear in
    /// [`ConditionKind::ALL`] order with no duplicates. An empty slice
    /// returns `ConditionKind::ALL.to_vec()` (every kind is missing).
    /// A slice that carries every variant returns an empty vec (no kind
    /// is missing). A slice that carries the same [`ConditionKind`] at
    /// multiple positions still contributes ZERO entries to the missing
    /// set at that kind (the closed-set complement is a SET operation —
    /// multiplicity on the present side is irrelevant to absence on the
    /// missing side).
    ///
    /// # Compounding future consumers
    ///
    /// - A future coherence check that enforces "every process boundary
    ///   carries a [`ConditionKind::JobAttested`] postcondition" now
    ///   surfaces the operator-facing diagnostic
    ///   `spec.boundary.postconditions.missing_kinds()` verbatim
    ///   (naming EVERY kind absent from postconditions in canonical
    ///   order) rather than reaching for `!has_kind(JobAttested)` at a
    ///   per-kind callsite and paying to re-author the diagnostic list.
    /// - An operator-facing "boundary is MISSING [JobAttested,
    ///   ClosedLoopAuth]" audit dump reads
    ///   `boundary.postconditions.missing_kinds()` directly at ONE call
    ///   site rather than restating the negated closed-set walk at
    ///   every consumer.
    /// - A fleet-wide gap analysis ("which processes are missing a
    ///   `ClosedLoopAuth` postcondition") reaches this ONE primitive
    ///   through `spec.boundary.postconditions.missing_kinds()
    ///   .contains(&ConditionKind::ClosedLoopAuth)` rather than paying
    ///   for the negated `.has_kind` sweep at every callsite.
    /// - A hypothetical `condition-kinds-missing-<n>` require-tag
    ///   classifier prefix family that publishes the missing-set
    ///   cardinality as a scalar reads
    ///   [`Self::missing_kind_count`] (the scalar-cardinality peer of
    ///   this widened primitive) without allocating.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The closed-set complement lives at ONE substrate site as a
    ///   typed projection of [`Self::has_kind`] over the closed set
    ///   [`ConditionKind::ALL`] under negation. Every downstream gap-
    ///   analysis consumer binds through the SAME shape rather than
    ///   restating the negated ALL-filter closure body.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`ConditionKind`] variant added to `ALL` reaches this
    ///   primitive mechanically (the closed-set walk picks up the new
    ///   entry on the missing side WITHOUT further per-caller edit —
    ///   any slice that doesn't yet populate the new kind sees it
    ///   listed as missing at every downstream callsite).
    fn missing_kinds(&self) -> Vec<ConditionKind> {
        ConditionKind::ALL
            .into_iter()
            .filter(|k| !self.has_kind(*k))
            .collect()
    }

    /// Scalar cardinality projection of [`Self::missing_kinds`] onto its
    /// `.len()` — the number of [`ConditionKind`] variants that do NOT
    /// appear in this slice. Default body:
    /// `ConditionKind::ALL.iter().filter(|k| !self.has_kind(**k)).count()`
    /// — a closed-set walk composed against [`Self::has_kind`] per variant
    /// under a NEGATED point-probe, WITHOUT materializing the intermediate
    /// `Vec<ConditionKind>` a caller reaching only for the scalar
    /// cardinality otherwise pays for. An empty slice returns
    /// `ConditionKind::ALL.len()` (every kind is missing); a slice
    /// carrying every variant returns `0` (no kind is missing).
    ///
    /// # Sibling to [`Self::missing_kinds`] / [`Self::distinct_kind_count`]
    ///
    /// Scalar projection of the closed-set-complement widened primitive
    /// — where `missing_kinds` returns the SET (a `Vec<ConditionKind>`
    /// in canonical [`ConditionKind::ALL`] order), `missing_kind_count`
    /// collapses that set to its cardinality. The composition law
    /// `missing_kind_count() == missing_kinds().len()` binds the scalar
    /// projection to the widened primitive at the trait's default body
    /// and is swept substrate-wide by
    /// [`assert_slice_refinement_composition_laws`] as its scalar-
    /// cardinality-complement arm.
    ///
    /// Byte-for-byte peer of [`Self::distinct_kind_count`] one axis over
    /// (under a negated `has_kind` predicate): where `distinct_kind_count`
    /// scalar-projects the closed-set-INVERSION widened primitive
    /// `distinct_kinds`, this method scalar-projects the closed-set-
    /// COMPLEMENT widened primitive `missing_kinds`. The two scalar
    /// projections PARTITION the closed-set cardinality:
    /// `distinct_kind_count() + missing_kind_count() ==
    /// ConditionKind::ALL.len()` — the scalar consequence of the
    /// `(distinct_kinds, missing_kinds)` partition law that
    /// [`assert_slice_refinement_composition_laws`] pins at the
    /// widened-primitive layer.
    ///
    /// # Peer to [`crate::tagged_union::TaggedUnion::populated_kind_count`]'s
    /// hypothetical complement peer
    ///
    /// Same shape at the peer axis one struct layer up: fixing the
    /// slice-side carrier and inverting the presence probe over the
    /// closed set under a negated predicate. The two primitives close
    /// the "closed-set-complement scalar cardinality" refinement at
    /// two adjacent typescape sites — one per closed-set-addressed
    /// slice-level refinement (this primitive), one per closed-set-
    /// addressed tagged-union parent-level refinement (a symmetric
    /// future addition).
    ///
    /// # Compounding future consumers
    ///
    /// - A future coherence check that enforces "every process boundary
    ///   carries EVERY [`ConditionKind`] under some slot" now reads
    ///   `spec.boundary.postconditions.missing_kind_count() == 0` at
    ///   ONE call site rather than paying for
    ///   `spec.boundary.postconditions.missing_kinds().is_empty()`
    ///   (with its intermediate heap allocation) or the eight-way
    ///   negated sweep with `has_kind` at the callsite.
    /// - A future require-tag classifier arm that surfaces the missing-
    ///   set cardinality as a scalar (the exact
    ///   `condition-kinds-missing-<n>` require-tag classifier prefix
    ///   family called out in [`Self::missing_kinds`]'s doc-comment as
    ///   a hypothetical compounding-future consumer) reaches this ONE
    ///   primitive without allocating.
    /// - A future gap-analysis dashboard reporting "boundary is missing
    ///   N of {N_TOTAL} distinct kinds" reaches
    ///   `slice.missing_kind_count()` directly rather than restating the
    ///   negated `.iter().filter(...).count()` closure body.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The scalar cardinality lives at ONE substrate site as a typed
    ///   projection of [`Self::missing_kinds`] onto its `.len()`, and
    ///   the default body composes against [`Self::has_kind`] over the
    ///   closed set [`ConditionKind::ALL`] under negation byte-
    ///   identically to `missing_kinds` without the intermediate `Vec`.
    ///   Every downstream aggregate consumer binds through the SAME
    ///   shape rather than paying for the allocation to reach the
    ///   cardinality.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`ConditionKind`] variant added to `ALL` reaches this primitive
    ///   mechanically (the closed-set walk picks up the new entry on
    ///   the missing side WITHOUT further per-caller edit — any slice
    ///   that doesn't yet populate the new kind sees the cardinality
    ///   rise by one at every downstream callsite).
    fn missing_kind_count(&self) -> usize {
        ConditionKind::ALL
            .iter()
            .filter(|k| !self.has_kind(**k))
            .count()
    }

    /// Short-circuiting `Option<ConditionKind>` peer of
    /// [`Self::distinct_kinds`] — the FIRST [`ConditionKind`] variant
    /// present in this slice, in canonical [`ConditionKind::ALL`] order,
    /// or `None` when the slice carries no matching kind. Default body:
    /// `ConditionKind::ALL.iter().copied().find(|k| self.has_kind(*k))`
    /// — a closed-set walk composed against [`Self::has_kind`] per
    /// variant that SHORT-CIRCUITS at the earliest match.
    ///
    /// # Sibling to [`Self::distinct_kinds`] / [`Self::distinct_kind_count`]
    ///
    /// Third refinement on the closed-set-inversion axis, `Option<ConditionKind>`-
    /// valued: `distinct_kinds` returns the SET, `distinct_kind_count`
    /// scalar-projects the cardinality, and `first_distinct_kind`
    /// scalar-projects the SET onto its earliest element. The composition
    /// law `first_distinct_kind() == distinct_kinds().first().copied()`
    /// binds the earliest-element projection to the widened primitive at
    /// the trait's default body — pinned substrate-wide by
    /// [`assert_slice_refinement_composition_laws`] as its
    /// earliest-element-inversion arm. Both coarser projections agree on
    /// emptiness: `first_distinct_kind().is_none() ==
    /// (distinct_kind_count() == 0)`.
    ///
    /// # Peer to [`crate::tagged_union::TaggedUnion::first_populated_kind`]
    ///
    /// Same shape at the peer axis one struct layer up: fixing the
    /// carrier and short-circuiting on the earliest [`ConditionKind::ALL`]
    /// hit under [`Self::has_kind`]. `TaggedUnion::first_populated_kind`
    /// walks the tagged-union parent's closed set; `first_distinct_kind`
    /// here walks [`ConditionKind::ALL`] on the slice-level presence-probe
    /// axis. The two primitives close the "earliest-element scalar-
    /// projection of the closed-set-inversion widened primitive"
    /// refinement at two adjacent typescape sites — one per closed-set-
    /// addressed slice-level refinement (this primitive), one per closed-
    /// set-addressed tagged-union parent-level refinement.
    ///
    /// # Semantics
    ///
    /// Returns `Some(k)` where `k` is the earliest [`ConditionKind::ALL`]
    /// entry with `self.has_kind(k) == true`, or `None` when no kind is
    /// present. An empty slice returns `None`. A slice carrying multiple
    /// variants returns the earliest one in [`ConditionKind::ALL`] order
    /// — a strictly more informative projection than
    /// `distinct_kinds().first().copied()` without materializing the
    /// intermediate `Vec<ConditionKind>` the widened primitive
    /// otherwise pays for.
    ///
    /// # Compounding future consumers
    ///
    /// - An operator-facing "first present kind" diagnostic on an audit
    ///   dump that names ONE kind rather than the full set reaches this
    ///   ONE substrate site rather than paying for
    ///   `slice.distinct_kinds().first().copied()` (with its
    ///   intermediate heap allocation).
    /// - A `first-distinct-<kind>` require-tag classifier arm reads this
    ///   primitive with no allocation, byte-for-byte symmetrical with
    ///   `slice.has_kind(kind)` under a closed-set-inversion projection.
    /// - A fast-path branch that discriminates "empty" from "any
    ///   populated" reads `slice.first_distinct_kind().is_some()` at ONE
    ///   call site rather than allocating a `Vec<ConditionKind>` through
    ///   `!distinct_kinds().is_empty()` or paying for the full
    ///   `distinct_kind_count() > 0` walk.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs. The
    ///   earliest-element projection lives at ONE substrate site as a
    ///   typed projection of [`Self::has_kind`] over the closed set
    ///   [`ConditionKind::ALL`] under short-circuit walk semantics.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`ConditionKind`] variant added to `ALL` reaches this primitive
    ///   mechanically (the closed-set walk picks up the new entry) —
    ///   every downstream consumer sees the wider earliest-hit projection
    ///   without further per-caller edit.
    fn first_distinct_kind(&self) -> Option<ConditionKind> {
        ConditionKind::ALL
            .iter()
            .copied()
            .find(|k| self.has_kind(*k))
    }

    /// Short-circuiting `Option<ConditionKind>` peer of
    /// [`Self::missing_kinds`] — the FIRST [`ConditionKind`] variant
    /// ABSENT from this slice, in canonical [`ConditionKind::ALL`] order,
    /// or `None` when the slice carries every variant. Default body:
    /// `ConditionKind::ALL.iter().copied().find(|k| !self.has_kind(*k))`
    /// — a closed-set walk composed against [`Self::has_kind`] per
    /// variant under NEGATION with SHORT-CIRCUIT at the earliest empty
    /// slot.
    ///
    /// # Sibling to [`Self::missing_kinds`] / [`Self::missing_kind_count`]
    ///
    /// Third refinement on the closed-set-complement axis,
    /// `Option<ConditionKind>`-valued: `missing_kinds` returns the SET,
    /// `missing_kind_count` scalar-projects the cardinality, and
    /// `first_missing_kind` scalar-projects the SET onto its earliest
    /// element. The composition law
    /// `first_missing_kind() == missing_kinds().first().copied()` binds
    /// the earliest-element projection to the widened primitive at the
    /// trait's default body — pinned substrate-wide by
    /// [`assert_slice_refinement_composition_laws`] as its
    /// earliest-element-complement arm. Both coarser projections agree
    /// on saturation: `first_missing_kind().is_none() ==
    /// (missing_kind_count() == 0)`.
    ///
    /// # Peer to [`Self::first_distinct_kind`]
    ///
    /// Closed-set-complement peer of the closed-set-inversion earliest-
    /// element primitive under a negated `has_kind` predicate. The two
    /// primitives PARTITION [`ConditionKind::ALL`]'s earliest-element
    /// projection: at least one of `first_distinct_kind()` and
    /// `first_missing_kind()` is `Some` on any non-degenerate closed set
    /// (both are `Some` iff `1 ≤ distinct_kind_count() <
    /// ConditionKind::ALL.len()`; only the distinct-side is `Some` on a
    /// saturated slice; only the missing-side is `Some` on an empty
    /// slice).
    ///
    /// # Peer to [`crate::tagged_union::TaggedUnion::first_missing_kind`]
    ///
    /// Same shape at the peer axis one struct layer up under a negated
    /// predicate. The two primitives close the "earliest-element scalar-
    /// projection of the closed-set-complement widened primitive"
    /// refinement at two adjacent typescape sites — one per closed-set-
    /// addressed slice-level refinement (this primitive), one per closed-
    /// set-addressed tagged-union parent-level refinement.
    ///
    /// # Semantics
    ///
    /// An empty slice returns `Some(ConditionKind::ALL[0])` (every kind
    /// missing, first hit is index 0). A slice populating exactly `k`
    /// returns `Some(ConditionKind::ALL[0])` if `k != ALL[0]`, else
    /// `Some(ALL[1])` (the earliest non-`k` entry). A saturated slice
    /// carrying every variant returns `None`.
    ///
    /// # Compounding future consumers
    ///
    /// - An operator-facing "first still-unfilled kind" diagnostic on a
    ///   partially-populated boundary reads
    ///   `boundary.postconditions.first_missing_kind()` at ONE substrate
    ///   site — a strictly-more-informative projection than
    ///   `!has_kind(JobAttested)` at a per-kind callsite for a fleet-wide
    ///   "which processes are missing at least one closed-loop kind"
    ///   audit.
    /// - A `first-missing-<kind>` require-tag classifier arm reads this
    ///   primitive with no allocation, byte-for-byte symmetrical with
    ///   `slice.first_distinct_kind()`.
    /// - A fast-path branch that discriminates "saturated" from "at least
    ///   one missing" reads `slice.first_missing_kind().is_some()` at ONE
    ///   call site rather than allocating through
    ///   `!missing_kinds().is_empty()` or paying for the full
    ///   `missing_kind_count() > 0` walk.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs. The
    ///   complement-earliest-element projection lives at ONE substrate
    ///   site as a typed projection of [`Self::has_kind`] over the
    ///   closed set [`ConditionKind::ALL`] under negation with short-
    ///   circuit walk semantics.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`ConditionKind`] variant added to `ALL` reaches this primitive
    ///   mechanically (the closed-set walk picks up the new entry on the
    ///   missing side) — every downstream consumer sees the wider
    ///   complement's earliest hit without further per-caller edit.
    fn first_missing_kind(&self) -> Option<ConditionKind> {
        ConditionKind::ALL
            .iter()
            .copied()
            .find(|k| !self.has_kind(*k))
    }

    /// Short-circuiting `Option<ConditionKind>` peer of
    /// [`Self::distinct_kinds`] — the LAST [`ConditionKind`] variant
    /// present in this slice, in canonical [`ConditionKind::ALL`]
    /// order, or `None` when the slice carries no variant. Default
    /// body: `ConditionKind::ALL.iter().rev().copied().find(|k|
    /// self.has_kind(*k))` — a REVERSED closed-set walk composed
    /// against [`Self::has_kind`] per variant that SHORT-CIRCUITS at
    /// the latest hit.
    ///
    /// # Sibling to [`Self::distinct_kinds`] /
    /// [`Self::distinct_kind_count`] / [`Self::first_distinct_kind`]
    ///
    /// Fourth refinement on the closed-set-inversion axis and second
    /// scalar `Option<ConditionKind>` projection: `distinct_kinds`
    /// returns the SET, `distinct_kind_count` scalar-projects the
    /// cardinality, `first_distinct_kind` scalar-projects the SET
    /// onto its earliest element, and `last_distinct_kind` scalar-
    /// projects the SET onto its latest element. The composition law
    /// `last_distinct_kind() == distinct_kinds().last().copied()`
    /// binds the latest-element projection to the widened primitive
    /// at the trait's default body — pinned substrate-wide by
    /// [`assert_slice_refinement_composition_laws`] as its
    /// latest-element-inversion arm. Both scalar projections agree on
    /// emptiness: `last_distinct_kind().is_none() ==
    /// first_distinct_kind().is_none() == distinct_kinds().is_empty()`.
    ///
    /// # Peer to [`Self::first_distinct_kind`]
    ///
    /// Time-reversed peer under the SAME `has_kind` predicate: where
    /// `first_distinct_kind` walks [`ConditionKind::ALL`] forward and
    /// SHORT-CIRCUITS at the earliest hit, this primitive walks the
    /// SAME closed set in reverse and SHORT-CIRCUITS at the latest
    /// hit. The two primitives close the "endpoint scalar-projection
    /// of the closed-set-inversion widened primitive" refinement pair
    /// at one substrate site — one per endpoint. On a slice with
    /// exactly one distinct kind both projections agree; on a slice
    /// with distinct-kind-count ≥ 2 they yield distinct results
    /// (the earliest and latest elements of the closed-set-inversion
    /// respectively).
    ///
    /// # Semantics
    ///
    /// An empty slice returns `None` (no kind present, no hit on any
    /// walk direction). A slice populating exactly `k` returns
    /// `Some(k)` (single hit; earliest = latest). A saturated slice
    /// carrying every variant returns `Some(ConditionKind::ALL.last()
    /// .unwrap())` (the last ALL entry hits at the earliest walk step
    /// of the reversed walk).
    ///
    /// # Compounding future consumers
    ///
    /// - A `last-distinct-<kind>` require-tag classifier arm reads
    ///   the latest-populated kind through this ONE substrate
    ///   primitive with no allocation, byte-for-byte symmetrical with
    ///   the earliest-hit `slice.first_distinct_kind()` peer.
    /// - A future coherence check that surfaces "boundary ends with
    ///   ClosedLoopAuth" reads
    ///   `spec.boundary.postconditions.last_distinct_kind() ==
    ///   Some(ConditionKind::ClosedLoopAuth)` at ONE call site rather
    ///   than paying for `spec.boundary.postconditions
    ///   .distinct_kinds().last() == Some(&…)` with its intermediate
    ///   heap allocation.
    /// - Combined with [`Self::first_distinct_kind`], operator
    ///   diagnostics that render a "populated-kind range" summary
    ///   (`first..=last` on the closed-set-inversion projection) read
    ///   the two endpoints through TWO substrate primitives at
    ///   symmetric shapes without allocating.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The latest-element projection lives at ONE substrate site as
    ///   a typed projection of [`Self::has_kind`] over the closed set
    ///   [`ConditionKind::ALL`] under REVERSED short-circuit walk
    ///   semantics; byte-for-byte peer of the earliest-element
    ///   projection under FORWARD walk semantics.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`ConditionKind`] variant added to `ALL` reaches this
    ///   primitive mechanically (the reversed closed-set walk picks
    ///   up the new entry at the appropriate position) — every
    ///   downstream consumer sees the wider latest-hit projection
    ///   without further per-caller edit.
    fn last_distinct_kind(&self) -> Option<ConditionKind> {
        ConditionKind::ALL
            .iter()
            .rev()
            .copied()
            .find(|k| self.has_kind(*k))
    }

    /// Short-circuiting `Option<ConditionKind>` peer of
    /// [`Self::missing_kinds`] — the LAST [`ConditionKind`] variant
    /// ABSENT from this slice, in canonical [`ConditionKind::ALL`]
    /// order, or `None` when the slice carries every variant. Default
    /// body: `ConditionKind::ALL.iter().rev().copied().find(|k|
    /// !self.has_kind(*k))` — a REVERSED closed-set walk composed
    /// against [`Self::has_kind`] per variant under NEGATION with
    /// SHORT-CIRCUIT at the latest empty slot.
    ///
    /// # Sibling to [`Self::missing_kinds`] /
    /// [`Self::missing_kind_count`] / [`Self::first_missing_kind`]
    ///
    /// Fourth refinement on the closed-set-complement axis and second
    /// scalar `Option<ConditionKind>` projection: `missing_kinds`
    /// returns the SET, `missing_kind_count` scalar-projects the
    /// cardinality, `first_missing_kind` scalar-projects the SET onto
    /// its earliest element, and `last_missing_kind` scalar-projects
    /// the SET onto its latest element. The composition law
    /// `last_missing_kind() == missing_kinds().last().copied()` binds
    /// the latest-element projection to the widened primitive at the
    /// trait's default body — pinned substrate-wide by
    /// [`assert_slice_refinement_composition_laws`] as its
    /// latest-element-complement arm. Both scalar projections agree
    /// on saturation: `last_missing_kind().is_none() ==
    /// first_missing_kind().is_none() == missing_kinds().is_empty()`.
    ///
    /// # Peer to [`Self::first_missing_kind`]
    ///
    /// Time-reversed peer under the SAME negated `has_kind` predicate:
    /// where `first_missing_kind` walks [`ConditionKind::ALL`] forward
    /// under negation and SHORT-CIRCUITS at the earliest empty slot,
    /// this primitive walks the SAME closed set in reverse and SHORT-
    /// CIRCUITS at the latest empty slot. The two primitives close
    /// the "endpoint scalar-projection of the closed-set-complement
    /// widened primitive" refinement pair at one substrate site.
    ///
    /// # Peer to [`Self::last_distinct_kind`]
    ///
    /// Closed-set-complement peer of the closed-set-inversion latest-
    /// element primitive under a NEGATED `has_kind` predicate. Along
    /// with [`Self::first_distinct_kind`] and [`Self::first_missing_kind`]
    /// the four scalar-endpoint projections partition the endpoint
    /// axis into (present, absent) × (earliest, latest) — every
    /// endpoint-addressable coherence check reads ONE of the four at
    /// ONE call site, never the full `Vec<ConditionKind>` walk.
    ///
    /// # Semantics
    ///
    /// An empty slice returns `Some(ConditionKind::ALL.last().unwrap())`
    /// (every kind missing, latest hit is the last ALL entry). A slice
    /// populating exactly `k` returns `Some(ALL.last().unwrap())` if
    /// `k != ALL.last().unwrap()`, else `Some(ALL[ALL.len() - 2])` (the
    /// latest non-`k` entry). A saturated slice carrying every variant
    /// returns `None`.
    ///
    /// # Compounding future consumers
    ///
    /// - An operator-facing "last still-unfilled kind" diagnostic on a
    ///   partially-populated boundary reads
    ///   `boundary.postconditions.last_missing_kind()` at ONE substrate
    ///   site — a strictly-more-informative projection than
    ///   `!has_kind(ClosedLoopAuth)` at a per-kind callsite for a
    ///   fleet-wide "which processes are latest-missing a specific
    ///   closed-loop kind" audit.
    /// - A `last-missing-<kind>` require-tag classifier arm reads this
    ///   primitive with no allocation, byte-for-byte symmetrical with
    ///   the earliest-hit `slice.first_missing_kind()` peer.
    /// - Combined with [`Self::first_missing_kind`], a coherence check
    ///   that renders a "missing-kind range" summary reads the two
    ///   endpoints through TWO substrate primitives at symmetric
    ///   shapes without allocating through `missing_kinds()`.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The complement-latest-element projection lives at ONE
    ///   substrate site as a typed projection of [`Self::has_kind`]
    ///   over the closed set [`ConditionKind::ALL`] under negation
    ///   with REVERSED short-circuit walk semantics; byte-for-byte
    ///   peer of the complement-earliest-element projection under
    ///   FORWARD walk semantics.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`ConditionKind`] variant added to `ALL` reaches this
    ///   primitive mechanically (the reversed closed-set walk picks
    ///   up the new entry on the missing side at the appropriate
    ///   position) — every downstream consumer sees the wider
    ///   complement's latest hit without further per-caller edit.
    fn last_missing_kind(&self) -> Option<ConditionKind> {
        ConditionKind::ALL
            .iter()
            .rev()
            .copied()
            .find(|k| !self.has_kind(*k))
    }

    /// Boolean saturation predicate on the closed-set-inversion axis —
    /// `true` iff EVERY [`ConditionKind::ALL`] variant appears at least
    /// once in this slice (equivalently, [`Self::missing_kinds`] is
    /// empty).
    ///
    /// Default body:
    /// `ConditionKind::ALL.iter().all(|k| self.has_kind(*k))` — a
    /// SHORT-CIRCUITING closed-set walk composed against [`Self::has_kind`]
    /// per variant that returns `false` at the FIRST missing kind,
    /// WITHOUT materializing [`Self::missing_kinds`]'s `Vec` and WITHOUT
    /// walking every entry to build [`Self::missing_kind_count`]'s
    /// scalar. Strictly cheaper than either widened primitive on every
    /// partially-populated arm (returns at the first empty slot rather
    /// than sweeping the full closed set).
    ///
    /// # Peer to [`crate::tagged_union::TaggedUnion::is_saturated`]
    ///
    /// Slice-level peer of the tagged-union parent-level saturation
    /// predicate one struct-layer up: where `is_saturated` names the
    /// tagged-union arm where every `<Self::Kind as ClosedSet>::ALL`
    /// slot is populated, `is_kind_saturated` names the slice arm where
    /// every [`ConditionKind::ALL`] variant appears at least once. Both
    /// short-circuit at the first missing entry under the SAME
    /// `<CLOSED_SET>::ALL.iter().all(has)` walk shape at two adjacent
    /// typescape sites.
    ///
    /// # Sibling to [`Self::missing_kind_count`] / [`Self::missing_kinds`]
    ///
    /// Boolean cardinality-endpoint peer of the scalar cardinality
    /// primitive on the closed-set-complement axis — where
    /// `missing_kind_count` returns the FULL scalar (any `usize` in
    /// `0..=ConditionKind::ALL.len()`), `is_kind_saturated` collapses
    /// that scalar to its zero-arm Boolean projection. The composition
    /// law `is_kind_saturated() == (missing_kind_count() == 0)` binds
    /// the Boolean projection to the scalar primitive at the trait's
    /// default body — swept substrate-wide by
    /// [`assert_slice_refinement_composition_laws`] as its
    /// saturation-endpoint arm.
    ///
    /// # Semantics
    ///
    /// An empty slice returns `false` (no kind is populated). A slice
    /// carrying a strict subset of [`ConditionKind::ALL`] returns
    /// `false`. A slice that carries every variant at least once
    /// (multiplicity is irrelevant) returns `true` — the SOLE arm
    /// where `is_kind_saturated` returns `true`.
    ///
    /// # Compounding future consumers
    ///
    /// - A future coherence check that enforces "every process boundary
    ///   exhaustively covers every [`ConditionKind`]" reads
    ///   `boundary.postconditions.is_kind_saturated()` at ONE call site
    ///   — one short-circuit walk, no allocation, no scalar equality
    ///   comparison against `ConditionKind::ALL.len()`.
    /// - An `is-kind-saturated` require-tag classifier arm reaches this
    ///   primitive with no allocation, byte-for-byte peer of the
    ///   tagged-union `is-saturated` classifier one struct-layer up.
    /// - A fleet-wide gap-analysis dashboard fast-path that discriminates
    ///   "boundary spans every kind" from "boundary is missing some
    ///   kind" reads `boundary.postconditions.is_kind_saturated()` at
    ///   ONE call site rather than restating either
    ///   `boundary.postconditions.missing_kind_count() == 0` (which
    ///   walks every slot to count) or
    ///   `boundary.postconditions.missing_kinds().is_empty()` (which
    ///   allocates the Vec before the emptiness check).
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The saturation-endpoint projection lives at ONE substrate
    ///   site as a typed short-circuiting closed-set walk
    ///   `ConditionKind::ALL.iter().all(has_kind)`. Every downstream
    ///   consumer binds through the SAME shape rather than restating
    ///   the `== ConditionKind::ALL.len()` scalar composition body.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`ConditionKind`] variant added to `ALL` reaches this
    ///   primitive mechanically through the `all` short-circuit — a
    ///   slice that was previously saturated is no longer saturated
    ///   at every downstream callsite unless it also carries the new
    ///   variant.
    fn is_kind_saturated(&self) -> bool {
        ConditionKind::ALL.iter().all(|k| self.has_kind(*k))
    }

    /// Boolean per-kind complement of [`Self::has_kind`] — `true` iff
    /// NO [`Condition`] in this slice carries the given
    /// [`ConditionKind`] (equivalently, the kind is a member of
    /// [`Self::missing_kinds`]).
    ///
    /// Default body: `!self.has_kind(kind)` — a definitional negation
    /// of the presence-probe primitive. Short-circuits transitively
    /// through [`Self::has_kind`]'s composition down to
    /// [`Self::iter_kind`]: `!self.find_kind(kind).is_some()` returns
    /// as soon as any match is found (yielding `false`) without
    /// walking the rest of the slice, WITHOUT materializing
    /// [`Self::missing_kinds`]'s `Vec` per-kind for a per-kind
    /// question, and WITHOUT allocating the closed-set-complement scan.
    ///
    /// # Peer to [`crate::tagged_union::TaggedUnion::lacks`]
    ///
    /// Slice-level peer of the tagged-union parent-level closed-set-
    /// complement predicate one struct-layer up: where
    /// [`crate::tagged_union::TaggedUnion::lacks`] answers "is THIS
    /// kind's slot on the tagged-union parent empty?", `lacks_kind`
    /// answers "does THIS kind appear in NO condition of the slice?".
    /// Both compose against their per-kind presence primitive under a
    /// definitional negation (`!has(kind)` / `!has_kind(kind)`) at two
    /// adjacent typescape sites — the two primitives close the
    /// closed-set-complement invariant on the per-kind axis at both
    /// struct layers under the SAME shape.
    ///
    /// # Sibling to [`Self::has_kind`]
    ///
    /// Boolean per-kind complement peer of the point-probe primitive
    /// on the closed-set-complement axis — where `has_kind` returns
    /// `true` iff the addressed kind appears at least once,
    /// `lacks_kind` returns its negation: `true` iff the addressed kind
    /// appears zero times. Together the two Booleans partition the
    /// (slice, kind) matrix at the slice-level presence-probe axis:
    /// exactly one of `has_kind(k)` and `lacks_kind(k)` is `true` for
    /// every `k ∈ ConditionKind::ALL`. The definitional complement law
    /// `lacks_kind(k) == !has_kind(k)` is pinned as a first-class typed
    /// invariant by the trait's own default body and swept substrate-
    /// wide by [`assert_slice_refinement_composition_laws`] as its
    /// per-kind-complement arm.
    ///
    /// # Sibling to [`Self::missing_kinds`] / [`Self::missing_kind_count`]
    ///
    /// Per-kind Boolean projection of the closed-set-complement
    /// widened + scalar primitives — where `missing_kinds` returns the
    /// FULL missing-set (a `Vec<ConditionKind>` of every absent kind)
    /// and `missing_kind_count` returns its cardinality (a `usize` in
    /// `0..=ConditionKind::ALL.len()`), `lacks_kind` collapses the
    /// missing-set to its per-kind membership Boolean for ONE
    /// addressed kind. The composition law
    /// `lacks_kind(k) == missing_kinds().contains(&k)` binds this
    /// Boolean projection to the widened closed-set-complement
    /// primitive at the trait's default body — strictly cheaper than
    /// the widened primitive on every per-kind question because the
    /// negation short-circuits at the first match on the has-side
    /// walk rather than allocating the closed-set-complement scan.
    ///
    /// # Semantics
    ///
    /// An empty slice returns `true` for every [`ConditionKind`] (no
    /// kind appears, so every kind is lacked). A slice carrying kind
    /// `k` at any position returns `false` for `lacks_kind(k)` and
    /// `true` for `lacks_kind(k')` for every `k' ≠ k` (single-kind
    /// coverage). A saturated slice (every kind appears at least once)
    /// returns `false` on every arm — the SOLE arrangement where the
    /// primitive returns `false` for every kind.
    ///
    /// # Compounding future consumers
    ///
    /// - A `lacks-<kind>` require-tag classifier arm reaches this
    ///   primitive with no allocation, byte-for-byte peer of the
    ///   tagged-union `lacks-<kind>` classifier one struct-layer up
    ///   under the SAME `!has(kind)` definitional negation shape.
    /// - A dependency-satisfaction coherence check that enforces "no
    ///   process boundary lacks a `ClosedLoopAuth` postcondition" reads
    ///   `boundary.postconditions.lacks_kind(ConditionKind::ClosedLoopAuth)`
    ///   at ONE call site rather than negating
    ///   `boundary.postconditions.has_kind(ConditionKind::ClosedLoopAuth)`
    ///   at the callsite or materializing the closed-set complement
    ///   with `missing_kinds().contains(&k)`.
    /// - A "still missing: <kind>" diagnostic that reports the FIRST
    ///   unmet postcondition kind reads `slice.lacks_kind(k)` inside a
    ///   `ConditionKind::ALL` fold at ONE substrate primitive per test
    ///   rather than restating the negation body at every callsite.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The per-kind closed-set-complement projection lives at ONE
    ///   substrate site as a definitional negation of [`Self::has_kind`].
    ///   Every downstream consumer whose semantic reading is "the
    ///   missing set contains THIS kind" reads through this primitive
    ///   rather than negating `has_kind` at every callsite or paying
    ///   for the closed-set-complement scan.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`ConditionKind`] variant added to `ALL` reaches this
    ///   primitive mechanically through the delegated `has_kind` —
    ///   every downstream `lacks-<kind>` classifier arm sees the wider
    ///   kind set without further per-caller edit.
    fn lacks_kind(&self, kind: ConditionKind) -> bool {
        !self.has_kind(kind)
    }
}

/// Iterator yielded by [`ConditionSliceExt::iter_kind`] — the widened
/// primitive on the slice-level presence-probe axis. Wraps a
/// [`std::slice::Iter`] over `Condition` values with a
/// [`ConditionKind`] discriminator; [`Iterator::next`] short-circuits
/// via [`std::iter::Iterator::find`] on the wrapped iterator so the
/// filter walk is byte-identical to `self.iter().filter(|c| c.kind ==
/// kind).next()` without paying for the anonymous-closure type
/// erasure a chained-adapter return position would carry.
///
/// # Why a named type
///
/// [`ConditionSliceExt::iter_kind`] returns this concrete type rather
/// than `impl Iterator<Item = &Condition>` so downstream consumers
/// (a fleet-wide audit dump that stores match streams in a struct
/// field, a coherence check that composes the iterator against
/// [`std::iter::Chain`] across pre-/post-conditions) name the
/// primitive's return without pulling in RPITIT's unnameable
/// per-callsite type. [`Boundary::iter_condition_kind`] and
/// [`crate::ephemeral::EphemeralSpec::iter_condition_kind`] chain two
/// [`KindMatches`] iterators via [`Iterator::chain`] — the resulting
/// [`std::iter::Chain<KindMatches<'_>, KindMatches<'_>>`] is itself
/// a standard nameable type.
pub struct KindMatches<'a> {
    inner: std::slice::Iter<'a, Condition>,
    kind: ConditionKind,
}

impl<'a> Iterator for KindMatches<'a> {
    type Item = &'a Condition;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.by_ref().find(|c| c.kind == self.kind)
    }
}

impl ConditionSliceExt for [Condition] {
    fn iter_kind(&self, kind: ConditionKind) -> KindMatches<'_> {
        KindMatches {
            inner: self.iter(),
            kind,
        }
    }
}

/// Generic slice-level substrate testkit — pins the FOUR composition
/// laws that bind the [`ConditionSliceExt`] refinement algebra
/// (`iter_kind` → `find_kind` → `has_kind` → `count_kind`) at ONE
/// call site per authored arrangement, sweeping [`ConditionKind::ALL`].
///
/// The [`ConditionSliceExt`] trait publishes four refinements on the
/// slice-level presence-probe axis:
///
/// | refinement | return type | default body                        |
/// |------------|-------------|-------------------------------------|
/// | `iter_kind`| [`KindMatches`]      | (widened primitive, required)      |
/// | `find_kind`| `Option<&Condition>` | `self.iter_kind(k).next()`         |
/// | `has_kind` | `bool`               | `self.find_kind(k).is_some()`      |
/// | `count_kind`| `usize`             | `self.iter_kind(k).count()`        |
///
/// The three coarser refinements are typed projections of the widened
/// primitive by construction. The composition laws that bind them
/// (and therefore surface any implementor that overrode a default
/// with a divergent walk shape — a stored-length cache that drifted,
/// a `.rev().find(...)` returning trailing-first, a `.step_by(2)`
/// artifact from a copy-paste of `iter_kind`) sweep at ONE typed
/// substrate site through this primitive:
///
/// 1. **`find ↔ iter`**: `find_kind(k) == iter_kind(k).next()` — the
///    first-match probe equals the widened stream's first yield.
/// 2. **`count ↔ iter`**: `count_kind(k) == iter_kind(k).count()` —
///    the cardinality probe equals the widened stream's yield count.
/// 3. **`has ↔ find`**: `has_kind(k) == find_kind(k).is_some()` —
///    the presence bit equals the first-match probe's `is_some()`.
/// 4. **`has ↔ count`**: `has_kind(k) == (count_kind(k) > 0)` — the
///    presence bit equals the cardinality's positivity test (the
///    dual composition path from `has` back to the widened primitive
///    that doesn't go through `find`).
///
/// Pre-lift each composition law lived at its own hand-authored
/// nested-`for` loop test in [`tatara_process::boundary`] tests
/// (`condition_slice_find_kind_equals_iter_kind_next`,
/// `condition_slice_count_kind_equals_iter_kind_count`,
/// `condition_slice_has_kind_equals_find_kind_is_some`,
/// `condition_slice_has_and_find_equal_count_greater_than_zero`) —
/// four sibling test bodies whose only per-law knobs were the
/// projection functions being bridged. Post-lift each authored
/// arrangement (empty, single-element, dual-populated, duplicate-
/// populated) pins ALL FOUR laws through ONE
/// `assert_slice_refinement_composition_laws(slice)` call whose body
/// is the substrate primitive's own sweep.
///
/// The primitive binds `<S: ConditionSliceExt + ?Sized>` so both a
/// bare `&[Condition]` and any future implementor of the trait
/// (a wrapper type with additional invariants, an alternative slice
/// projection over a builder's staging Vec) picks up the four-law
/// composition contract through ONE call site. `?Sized` lets the
/// caller pass `slice.as_slice()` or `&owned[..]` without an
/// intermediate reference dance.
///
/// # Compounding
///
/// A FIFTH refinement added to [`ConditionSliceExt`] (a hypothetical
/// `nth_kind(k, n) -> Option<&Condition>` for indexed match access,
/// a `distinct_kinds()` aggregate that returns which kinds appear at
/// least once, a `has_kind_matching(pred)` closure-based predicate
/// probe) lands its composition-law pins as ONE new arm inside this
/// primitive's sweep body. Every downstream test that already reaches
/// this primitive picks up the fifth-refinement pin mechanically —
/// no per-arrangement author-time enumeration of the new law across
/// the four sibling composition-law sites, no re-authored `for kind
/// in ConditionKind::ALL { … }` sweep at every consumer.
///
/// Symmetrical shape to
/// [`crate::tagged_union::assert_find_agrees_with_has`] on the
/// tagged-union parent axis: both project a widened-refinement /
/// coarser-refinement composition law contract onto ONE typed
/// substrate call site, both bind `<T: /* refinement carrier */>`
/// generically, both sweep the addressed closed set
/// ([`ConditionKind::ALL`] here, `<T::Kind as ClosedSet>::ALL`
/// there). The two primitives close the "refinement axis composes"
/// invariant at two adjacent typescape sites — one per closed-set-
/// addressed slice-level refinement, one per closed-set-addressed
/// tagged-union parent-level refinement.
///
/// Theory anchor: THEORY.md §II.1 invariant 5 — composition preserves
/// proofs. The four coarser refinements are typed projections of the
/// widened primitive, and this substrate primitive turns each
/// projection's composition law from doc-prose into a first-class
/// typed theorem provable generically over any
/// `S: ConditionSliceExt + ?Sized`. THEORY.md §VI.1 — generation over
/// composition; a new [`ConditionKind`] variant added to `ALL` reaches
/// every downstream composition-law consumer through the SAME
/// closed-set sweep with no per-caller edit.
#[track_caller]
pub fn assert_slice_refinement_composition_laws<S>(slice: &S)
where
    S: ConditionSliceExt + ?Sized,
{
    let distinct = slice.distinct_kinds();
    for kind in ConditionKind::ALL {
        let find_result = slice.find_kind(kind);
        let has_result = slice.has_kind(kind);
        let count_result = slice.count_kind(kind);
        let iter_next_kind = slice.iter_kind(kind).next().map(|c| c.kind);
        let iter_count = slice.iter_kind(kind).count();

        // find ↔ iter
        assert_eq!(
            find_result.map(|c| c.kind),
            iter_next_kind,
            "find_kind({kind:?}) drifted from iter_kind({kind:?}).next()",
        );
        // count ↔ iter
        assert_eq!(
            count_result, iter_count,
            "count_kind({kind:?}) drifted from iter_kind({kind:?}).count()",
        );
        // has ↔ find
        assert_eq!(
            has_result,
            find_result.is_some(),
            "has_kind({kind:?}) drifted from find_kind({kind:?}).is_some()",
        );
        // has ↔ count
        assert_eq!(
            has_result,
            count_result > 0,
            "has_kind({kind:?}) drifted from (count_kind({kind:?}) > 0)",
        );
        // distinct ↔ has (per-kind membership on the closed-set-inversion axis)
        assert_eq!(
            distinct.contains(&kind),
            has_result,
            "distinct_kinds().contains({kind:?}) drifted from has_kind({kind:?})",
        );
    }

    // distinct ↔ ALL-filter (canonical subsequence — closed-set-inversion
    // walks ConditionKind::ALL in order, filters by has_kind, dedups by
    // construction). A regression that (a) returned duplicates (a naive
    // `.iter().map(|c| c.kind).collect()` override that skipped dedup),
    // (b) drifted the walk order from ConditionKind::ALL to slice-encounter
    // order, or (c) returned a superset containing absent kinds surfaces
    // HERE at the substrate boundary.
    let canonical: Vec<ConditionKind> = ConditionKind::ALL
        .into_iter()
        .filter(|k| slice.has_kind(*k))
        .collect();
    assert_eq!(
        distinct, canonical,
        "distinct_kinds() must yield ConditionKind::ALL-ordered subsequence of kinds where has_kind is true (no duplicates, canonical order)",
    );

    // distinct_kind_count ↔ distinct_kinds.len() — the scalar
    // cardinality projection of the closed-set-inversion widened
    // primitive. A regression that overrode `distinct_kind_count` to
    // skip a kind, double-count a slot, or drift the walk from
    // `ConditionKind::ALL` surfaces HERE at the substrate boundary,
    // not as silent drift at every downstream `distinct-count-<n>`
    // require-tag classifier or audit-dump callsite.
    assert_eq!(
        slice.distinct_kind_count(),
        distinct.len(),
        "distinct_kind_count() drifted from distinct_kinds().len()",
    );

    // missing ↔ has (per-kind complement on the closed-set-inversion
    // axis). Byte-for-byte peer to the `distinct ↔ has` arm above: the
    // present-side widened primitive `distinct_kinds` binds to
    // `has_kind` via `contains(&k) == has_kind(k)`; the missing-side
    // widened primitive `missing_kinds` binds via
    // `contains(&k) == !has_kind(k)` — the SAME point-probe primitive
    // reached under a negated predicate. A regression that overrode
    // `missing_kinds` to omit the negation (returning `distinct_kinds`
    // instead), inverted the wrong side, or dropped a variant surfaces
    // HERE.
    let missing = slice.missing_kinds();
    for kind in ConditionKind::ALL {
        assert_eq!(
            missing.contains(&kind),
            !slice.has_kind(kind),
            "missing_kinds().contains({kind:?}) drifted from !has_kind({kind:?})",
        );
    }

    // missing ↔ ALL-filter (canonical subsequence — closed-set
    // complement walks ConditionKind::ALL in order, filters by
    // !has_kind, dedups by construction). Peer to the `distinct ↔
    // ALL-filter` arm above; catches ordering + dedup drift on the
    // complement side that the per-kind membership arm cannot detect
    // on its own.
    let canonical_missing: Vec<ConditionKind> = ConditionKind::ALL
        .into_iter()
        .filter(|k| !slice.has_kind(*k))
        .collect();
    assert_eq!(
        missing, canonical_missing,
        "missing_kinds() must yield ConditionKind::ALL-ordered subsequence of kinds where has_kind is false (no duplicates, canonical order)",
    );

    // (distinct, missing) partition ConditionKind::ALL — three peer
    // laws that bind the closed-set-inversion widened primitive
    // `distinct_kinds` to its complement peer `missing_kinds`:
    //
    // 1. Disjoint: every kind appears in AT MOST one of the two sets.
    // 2. Covering: every kind appears in AT LEAST one of the two sets
    //    (equivalent to the union covering ConditionKind::ALL).
    // 3. Cardinality partition: `distinct.len() + missing.len() ==
    //    ConditionKind::ALL.len()` — the scalar consequence of (1) +
    //    (2) that a caller reaching for the cardinality peer would
    //    otherwise pay for the two allocations at every callsite.
    for kind in ConditionKind::ALL {
        assert!(
            !(distinct.contains(&kind) && missing.contains(&kind)),
            "(distinct_kinds, missing_kinds) partition invariant violated — both contain {kind:?}",
        );
        assert!(
            distinct.contains(&kind) || missing.contains(&kind),
            "(distinct_kinds, missing_kinds) partition invariant violated — neither contains {kind:?}",
        );
    }
    assert_eq!(
        distinct.len() + missing.len(),
        ConditionKind::ALL.len(),
        "(distinct_kinds, missing_kinds) cardinality partition drift — sum {} ≠ ConditionKind::ALL.len() {}",
        distinct.len() + missing.len(),
        ConditionKind::ALL.len(),
    );

    // missing_kind_count ↔ missing_kinds.len() — the scalar cardinality
    // projection of the closed-set-complement widened primitive. A
    // regression that overrode `missing_kind_count` to drop the
    // negation (returning `distinct_kind_count`), skip a kind, double-
    // count a slot, or drift the walk from `ConditionKind::ALL`
    // surfaces HERE at the substrate boundary, not as silent drift at
    // every downstream `condition-kinds-missing-<n>` require-tag
    // classifier or gap-analysis-dashboard callsite.
    assert_eq!(
        slice.missing_kind_count(),
        missing.len(),
        "missing_kind_count() drifted from missing_kinds().len()",
    );

    // (distinct_kind_count, missing_kind_count) partition
    // ConditionKind::ALL's cardinality — the scalar consequence of the
    // widened-primitive partition law `distinct ∪ missing == ALL,
    // disjoint` above. A regression that (a) drifted the scalar
    // cardinality peer from the widened primitive on either side or
    // (b) drifted the partition invariant surfaces HERE at ONE typed
    // arm rather than as silent drift at every scalar-cardinality
    // callsite that reaches for the sum.
    assert_eq!(
        slice.distinct_kind_count() + slice.missing_kind_count(),
        ConditionKind::ALL.len(),
        "(distinct_kind_count, missing_kind_count) scalar partition drift — sum {} ≠ ConditionKind::ALL.len() {}",
        slice.distinct_kind_count() + slice.missing_kind_count(),
        ConditionKind::ALL.len(),
    );

    // first_distinct_kind ↔ distinct_kinds.first().copied() — the
    // earliest-element scalar projection of the closed-set-inversion
    // widened primitive. Peer of `distinct_kind_count ↔ distinct_kinds
    // .len()` on the scalar-projection axis: where the cardinality peer
    // collapses the SET to its length, the earliest-element peer
    // collapses the SET to its first element. A regression that
    // overrode `first_distinct_kind` to skip a kind, drift the walk
    // from ConditionKind::ALL, forget the short-circuit (returning
    // the LAST hit), or diverge from the widened primitive's canonical
    // ordering surfaces HERE at the substrate boundary, not as silent
    // drift at every downstream `first-distinct-<kind>` require-tag
    // classifier callsite.
    assert_eq!(
        slice.first_distinct_kind(),
        distinct.first().copied(),
        "first_distinct_kind() drifted from distinct_kinds().first().copied()",
    );

    // first_missing_kind ↔ missing_kinds.first().copied() — the
    // earliest-element scalar projection of the closed-set-complement
    // widened primitive. Byte-for-byte peer of `first_distinct_kind`
    // one axis over under a negated predicate: where
    // `first_distinct_kind` scalar-projects the closed-set-INVERSION
    // widened primitive onto its earliest element, this arm scalar-
    // projects the closed-set-COMPLEMENT widened primitive onto its
    // earliest element. A regression that overrode `first_missing_kind`
    // to drop the negation (returning `first_distinct_kind`), skip a
    // kind, drift the walk from ConditionKind::ALL, or forget the
    // short-circuit (returning the LAST missing hit) surfaces HERE at
    // the substrate boundary, not as silent drift at every downstream
    // `first-missing-<kind>` require-tag classifier callsite.
    assert_eq!(
        slice.first_missing_kind(),
        missing.first().copied(),
        "first_missing_kind() drifted from missing_kinds().first().copied()",
    );

    // last_distinct_kind ↔ distinct_kinds.last().copied() — the
    // latest-element scalar projection of the closed-set-inversion
    // widened primitive. Time-reversed peer of `first_distinct_kind
    // ↔ distinct_kinds.first().copied()` under the SAME `has_kind`
    // predicate but with the closed-set walk reversed: where the
    // earliest-element peer picks the smallest ALL index that hits,
    // this arm picks the LARGEST. A regression that overrode
    // `last_distinct_kind` to skip a kind, drift the walk direction
    // (returning `first_distinct_kind`), forget the short-circuit
    // (returning `distinct_kinds().rev().next()` allocation), or
    // diverge from the widened primitive's canonical ordering
    // surfaces HERE at the substrate boundary, not as silent drift
    // at every downstream `last-distinct-<kind>` require-tag
    // classifier callsite.
    assert_eq!(
        slice.last_distinct_kind(),
        distinct.last().copied(),
        "last_distinct_kind() drifted from distinct_kinds().last().copied()",
    );

    // last_missing_kind ↔ missing_kinds.last().copied() — the
    // latest-element scalar projection of the closed-set-complement
    // widened primitive. Byte-for-byte peer of `last_distinct_kind`
    // one axis over under a NEGATED predicate: where
    // `last_distinct_kind` scalar-projects the closed-set-INVERSION
    // widened primitive onto its LATEST element, this arm scalar-
    // projects the closed-set-COMPLEMENT widened primitive onto its
    // LATEST element. A regression that overrode `last_missing_kind`
    // to drop the negation (returning `last_distinct_kind`), reverse
    // the walk direction (returning `first_missing_kind`), skip a
    // kind, or forget the short-circuit surfaces HERE at the
    // substrate boundary, not as silent drift at every downstream
    // `last-missing-<kind>` require-tag classifier callsite.
    assert_eq!(
        slice.last_missing_kind(),
        missing.last().copied(),
        "last_missing_kind() drifted from missing_kinds().last().copied()",
    );

    // is_kind_saturated ↔ (missing_kind_count == 0) — the Boolean
    // saturation-endpoint projection of the closed-set-complement
    // scalar cardinality. Peer of `first_missing_kind ↔ missing_kinds
    // .first().copied()` on the endpoint-projection axis: where the
    // earliest-element peer collapses the missing SET to its first
    // element, this Boolean peer collapses the missing scalar to its
    // zero-arm test. A regression that overrode `is_kind_saturated` to
    // drop the negation (returning `slice.is_empty()`), skip a kind,
    // or drift the walk from `ConditionKind::ALL` surfaces HERE at
    // the substrate boundary, not as silent drift at every downstream
    // `is-kind-saturated` require-tag classifier or fleet-wide gap-
    // analysis dashboard callsite. Byte-for-byte peer of
    // `crate::tagged_union::TaggedUnion::is_saturated` one struct-
    // layer up under the same `<CLOSED_SET>::ALL.iter().all(has)`
    // short-circuit shape.
    assert_eq!(
        slice.is_kind_saturated(),
        slice.missing_kind_count() == 0,
        "is_kind_saturated() drifted from (missing_kind_count() == 0)",
    );
    assert_eq!(
        slice.is_kind_saturated(),
        missing.is_empty(),
        "is_kind_saturated() drifted from missing_kinds().is_empty()",
    );

    // lacks_kind ↔ !has_kind — the Boolean per-kind complement
    // projection on the closed-set-complement axis. Peer of
    // `is_kind_saturated ↔ (missing_kind_count == 0)` on the Boolean-
    // projection axis: where the saturation-endpoint peer collapses
    // the whole missing scalar to its zero-arm test, this per-kind
    // peer collapses the whole missing SET to its per-kind membership
    // Boolean for ONE addressed kind. A regression that overrode
    // `lacks_kind` to drop the negation (returning `has_kind`), swap
    // the wrong side, or drift the walk from `has_kind` surfaces HERE
    // at the substrate boundary, not as silent drift at every
    // downstream `lacks-<kind>` require-tag classifier or
    // dependency-satisfaction coherence check callsite. Byte-for-byte
    // peer of `crate::tagged_union::TaggedUnion::lacks` one struct-
    // layer up under the SAME `!has(kind)` definitional negation
    // shape. Also pins the widened composition law
    // `lacks_kind(k) == missing_kinds().contains(&k)` at every arm —
    // binds the per-kind Boolean projection to the widened closed-set-
    // complement primitive without paying for the Vec allocation.
    for kind in ConditionKind::ALL {
        assert_eq!(
            slice.lacks_kind(kind),
            !slice.has_kind(kind),
            "lacks_kind({kind:?}) drifted from !has_kind({kind:?})",
        );
        assert_eq!(
            slice.lacks_kind(kind),
            missing.contains(&kind),
            "lacks_kind({kind:?}) drifted from missing_kinds().contains(&{kind:?})",
        );
    }
}

/// Substrate testkit macro — pins the FOUR union composition laws that
/// bind the (precondition, postcondition, union) refinement triads on
/// any authored surface exposing the 12-method (has / find / iter /
/// count) × (pre / post / union) `_kind` matrix. Sweeps
/// [`ConditionKind::ALL`] at ONE call site per authored arrangement.
///
/// # The four surface-level union composition laws
///
/// Where the slice-level substrate primitive
/// [`assert_slice_refinement_composition_laws`] pins the algebra that
/// binds the four refinements *on a single slice* (`iter_kind` →
/// `find_kind` → `has_kind` → `count_kind`), this macro pins the peer
/// algebra one struct-layer up: each refinement's union arm on a
/// two-slice surface (a [`Boundary`] with `preconditions` +
/// `postconditions`, an [`crate::ephemeral::EphemeralSpec`] with the
/// same eponymous field pair) composes from its two half-slice arms
/// through a specific monoid operator baked into the refinement's return
/// type:
///
/// | refinement | half-slice arms                             | union composition                     |
/// |------------|---------------------------------------------|---------------------------------------|
/// | `has_*_kind`   | `has_precondition_kind`, `has_postcondition_kind`     | `pre \|\| post` (bool OR)             |
/// | `find_*_kind`  | `find_precondition_kind`, `find_postcondition_kind`   | `pre.or(post)` (first-Some)           |
/// | `iter_*_kind`  | `iter_precondition_kind`, `iter_postcondition_kind`   | `pre.chain(post)` (stream concat)     |
/// | `count_*_kind` | `count_precondition_kind`, `count_postcondition_kind` | `pre + post` (cardinality SUM)        |
///
/// # Why lift
///
/// Pre-lift each surface-level union composition law lived at its own
/// hand-authored nested-`for` loop test on each of the two surfaces —
/// EIGHT sibling test bodies (`boundary_has_condition_kind_composes_precondition_and_postcondition_arms`,
/// `find_condition_kind_triad_delegates_to_slice_find_kind`,
/// `iter_condition_kind_triad_delegates_to_slice_iter_kind`,
/// `boundary_count_condition_kind_triad_delegates_and_sums_slice_count_kind`
/// on the [`Boundary`] surface, byte-for-byte peers on the
/// [`crate::ephemeral::EphemeralSpec`] surface) whose only per-law knobs
/// were the projection functions being bridged and the composition
/// operator (`\|\|` / `Option::or` / `Iterator::chain` / `+`) applied
/// on top. Post-lift each authored `(preconditions, postconditions)`
/// arrangement pins ALL FOUR union composition laws through ONE
/// `assert_surface_union_composition_laws!(surface)` call whose body
/// is the substrate primitive's own sweep, no per-surface author-time
/// enumeration.
///
/// # Why a macro rather than a `pub fn`
///
/// [`Boundary`] and [`crate::ephemeral::EphemeralSpec`] expose the
/// twelve methods as *inherent* methods with matching signatures. A
/// generic `pub fn assert_surface_union_composition_laws<B: T>(&B)`
/// would need a trait `T` publishing those same twelve methods, and
/// implementing that trait on either surface would collide with the
/// eponymous inherent methods at method resolution — the trait
/// impl would either duplicate the inherent-method bodies verbatim
/// (defeating the lift) or require renaming the trait methods with a
/// `_ext` suffix (introducing a parallel API surface). A macro
/// duck-types at expansion time and hits the inherent methods
/// directly, so both surfaces stay bound through the SAME
/// `_kind`-suffixed method names their non-generic callers already
/// reach for, and the pattern generalizes to any future surface that
/// grows the same twelve-method matrix (an `AplicacaoBoundary` typed
/// wrapper, a `PoolBoundary` gate-carrier at
/// [`crate::pool`], the boundary slot on a
/// hypothetical `AttestationBoundary` receipt-envelope surface) with
/// ONE macro invocation per authored arrangement rather than a per-
/// surface re-authored sweep over the four laws.
///
/// # Compounding
///
/// A FIFTH union refinement added to the (has, find, iter, count)
/// tetrad (a hypothetical `first_params_of_kind(k) -> Option<&Value>`
/// projection combining `find_condition_kind(k).map(|c| &c.params)` at
/// real reconciler callsites, a `distinct_kinds() -> impl Iterator<Item
/// = ConditionKind>` aggregate returning which kinds appear at least
/// once on either side, a `has_kind_matching(pred)` closure-based
/// predicate probe) lands its composition-law pin as ONE new arm
/// inside this macro's body. Every downstream test that already reaches
/// this macro picks up the fifth-refinement pin mechanically — no per-
/// arrangement author-time enumeration of the new law across the four
/// sibling composition-law sites on each of the two surfaces, no
/// re-authored `for kind in ConditionKind::ALL { … }` sweep at every
/// consumer.
///
/// Symmetrical shape to [`assert_slice_refinement_composition_laws`]
/// one layer below: both project a widened-refinement / coarser-
/// refinement composition law contract onto ONE typed substrate call
/// site, both sweep the addressed closed set [`ConditionKind::ALL`],
/// both surface any implementor that overrode the union arm with a
/// divergent composition operator (an `&&` inlined where `\|\|` is
/// required, a `pre - post` inlined where `pre + post` is required,
/// a `zip` inlined where `chain` is required, a `and_then` inlined
/// where `or_else` is required) as a first-class typed test failure
/// rather than as silent operator-facing drift at the
/// `condition-<kind>` / `precondition-<kind>` / `postcondition-<kind>`
/// require-tag classifier surfaces downstream.
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs. Each
///   union arm is a typed projection of its two half-slice peers via
///   a specific monoid operator, and this substrate macro turns each
///   projection's composition law from doc-prose into a first-class
///   typed theorem provable against any surface exposing the twelve
///   `_kind`-suffixed inherent methods.
/// - THEORY.md §VI.1 — generation over composition. A new
///   [`ConditionKind`] variant added to `ALL` reaches every downstream
///   union-composition-law consumer through the SAME closed-set sweep
///   with no per-caller edit; a new surface (a typed wrapper carrying
///   the same twelve methods) picks up all four union composition-law
///   pins through ONE macro invocation per authored arrangement.
///
/// # Usage
///
/// ```ignore
/// // Point surface.
/// let mut b = Boundary::default();
/// b.preconditions.push(condition_with(ConditionKind::PromQL));
/// b.postconditions.push(condition_with(ConditionKind::ClosedLoopAuth));
/// assert_surface_union_composition_laws!(b);
///
/// // Ephemeral surface (peer, same primitive).
/// let mut spec = empty_ephemeral();
/// spec.postconditions.push(cond(ConditionKind::JobAttested));
/// assert_surface_union_composition_laws!(spec);
/// ```
#[macro_export]
macro_rules! assert_surface_union_composition_laws {
    ($surface:expr) => {{
        let __surface = &$surface;
        // Hoist distinct_* out of the per-kind loop — closed-set-inversion
        // refinements return the WHOLE distinct-set per call, so a single
        // computation per surface backs the per-kind membership arm inside
        // the loop AND the canonical-order equality after it.
        let __distinct_pre_kinds = __surface.distinct_precondition_kinds();
        let __distinct_post_kinds = __surface.distinct_postcondition_kinds();
        let __distinct_union_kinds = __surface.distinct_condition_kinds();
        let __missing_pre_kinds = __surface.missing_precondition_kinds();
        let __missing_post_kinds = __surface.missing_postcondition_kinds();
        let __missing_union_kinds = __surface.missing_condition_kinds();
        for __kind in $crate::boundary::ConditionKind::ALL {
            // has: union == pre || post (bool OR)
            let __has_via_arms =
                __surface.has_precondition_kind(__kind) || __surface.has_postcondition_kind(__kind);
            ::core::assert_eq!(
                __surface.has_condition_kind(__kind),
                __has_via_arms,
                "surface union has arm drifted from OR of half-slice arms for {:?}",
                __kind,
            );
            // find: union == pre.or(post) (first-Some, kind projection)
            let __find_via_arms = __surface
                .find_precondition_kind(__kind)
                .or(__surface.find_postcondition_kind(__kind))
                .map(|c| c.kind);
            ::core::assert_eq!(
                __surface.find_condition_kind(__kind).map(|c| c.kind),
                __find_via_arms,
                "surface union find arm drifted from precondition.or(postcondition) for {:?}",
                __kind,
            );
            // iter: union == chain(pre, post) (stream concat, kind projection)
            let __iter_via_arms: ::std::vec::Vec<_> = __surface
                .iter_precondition_kind(__kind)
                .chain(__surface.iter_postcondition_kind(__kind))
                .map(|c| c.kind)
                .collect();
            let __iter_via_union: ::std::vec::Vec<_> = __surface
                .iter_condition_kind(__kind)
                .map(|c| c.kind)
                .collect();
            ::core::assert_eq!(
                __iter_via_union,
                __iter_via_arms,
                "surface union iter arm drifted from chain(pre, post) for {:?}",
                __kind,
            );
            // count: union == pre + post (cardinality SUM)
            ::core::assert_eq!(
                __surface.count_condition_kind(__kind),
                __surface.count_precondition_kind(__kind)
                    + __surface.count_postcondition_kind(__kind),
                "surface union count arm drifted from SUM of half-slice arms for {:?}",
                __kind,
            );
            // distinct: union.contains(k) == pre.contains(k) || post.contains(k)
            // (set-union membership per kind on the closed-set-inversion axis)
            ::core::assert_eq!(
                __distinct_union_kinds.contains(&__kind),
                __distinct_pre_kinds.contains(&__kind)
                    || __distinct_post_kinds.contains(&__kind),
                "surface distinct union arm drifted from OR-membership of half-slice distinct arms for {:?}",
                __kind,
            );
            // missing: union.contains(k) == pre.contains(k) && post.contains(k)
            // (set-INTERSECTION membership per kind — a kind is missing
            // from the union iff it is missing from BOTH half-slices,
            // dual of the distinct-set OR composition).
            ::core::assert_eq!(
                __missing_union_kinds.contains(&__kind),
                __missing_pre_kinds.contains(&__kind)
                    && __missing_post_kinds.contains(&__kind),
                "surface missing union arm drifted from AND-membership of half-slice missing arms for {:?}",
                __kind,
            );
            // missing ↔ has: union.contains(k) == !has_condition_kind(k)
            // — binds the missing-set primitive to the point-probe
            // primitive on the surface under a negated predicate.
            ::core::assert_eq!(
                __missing_union_kinds.contains(&__kind),
                !__surface.has_condition_kind(__kind),
                "surface missing union arm drifted from !has_condition_kind for {:?}",
                __kind,
            );
            // lacks: union == pre && post (bool AND — dual of `has`'s
            // `pre || post` OR under `!(a || b) == !a && !b`). A kind is
            // lacked from the union iff BOTH half-slices lack it — the
            // per-kind Boolean-projection peer of the missing-set
            // intersection membership arm above (which composes the SAME
            // AND over the closed-set-complement Vecs); this arm
            // composes it over the per-slice per-kind negation
            // primitives without materializing either side's missing-
            // set Vec. A regression that (a) drifted the union operator
            // to `||` (widening the intersection to a union),
            // (b) dropped the negation on one side, or (c) inverted the
            // wrong slice on the point probe surfaces HERE at the
            // substrate boundary, not as silent drift at every
            // downstream `lacks-<kind>` require-tag classifier callsite.
            let __lacks_via_arms =
                __surface.lacks_precondition_kind(__kind) && __surface.lacks_postcondition_kind(__kind);
            ::core::assert_eq!(
                __surface.lacks_condition_kind(__kind),
                __lacks_via_arms,
                "surface union lacks arm drifted from AND of half-slice lacks arms for {:?}",
                __kind,
            );
            // lacks ↔ has: union == !has_condition_kind(k) — the
            // definitional complement law binds the per-kind Boolean-
            // complement primitive on the surface to the point-probe
            // primitive under negation. Peer of the `missing ↔ has`
            // arm above one refinement lower: the closed-set-complement
            // Vec's per-kind membership equals the per-kind Boolean
            // complement, both equal `!has_condition_kind(k)`. A
            // regression that overrode `lacks_condition_kind` to drop
            // the negation, drift the underlying union primitive, or
            // return `has_condition_kind` surfaces HERE.
            ::core::assert_eq!(
                __surface.lacks_condition_kind(__kind),
                !__surface.has_condition_kind(__kind),
                "surface union lacks arm drifted from !has_condition_kind for {:?}",
                __kind,
            );
        }
        // distinct: union == canonical(pre ∪ post) — closed-set-inversion
        // set-union projected in ConditionKind::ALL order. A regression that
        // (a) reversed the walk order (post-then-pre), (b) preserved
        // slice-encounter order rather than ConditionKind::ALL order, or
        // (c) narrowed the union to an intersection surfaces HERE at the
        // substrate boundary (the per-kind membership arm above catches
        // membership drift; this arm catches ordering + dedup drift the
        // membership arm cannot detect on its own).
        let __expected_distinct_union: ::std::vec::Vec<_> =
            $crate::boundary::ConditionKind::ALL
                .into_iter()
                .filter(|__k| {
                    __distinct_pre_kinds.contains(__k)
                        || __distinct_post_kinds.contains(__k)
                })
                .collect();
        ::core::assert_eq!(
            __distinct_union_kinds, __expected_distinct_union,
            "surface distinct union arm drifted from canonical ConditionKind::ALL-ordered set-union of half-slice distinct arms",
        );
        // missing: union == canonical(pre ∩ post) — closed-set-inversion
        // set-INTERSECTION projected in ConditionKind::ALL order. Dual
        // of the distinct union canonical-order arm above. A regression
        // that (a) reversed the walk order, (b) widened the intersection
        // to a union (returning kinds missing from either side rather
        // than both), or (c) preserved slice-encounter order rather
        // than ConditionKind::ALL order surfaces HERE at the substrate
        // boundary.
        let __expected_missing_union: ::std::vec::Vec<_> =
            $crate::boundary::ConditionKind::ALL
                .into_iter()
                .filter(|__k| {
                    __missing_pre_kinds.contains(__k)
                        && __missing_post_kinds.contains(__k)
                })
                .collect();
        ::core::assert_eq!(
            __missing_union_kinds, __expected_missing_union,
            "surface missing union arm drifted from canonical ConditionKind::ALL-ordered set-INTERSECTION of half-slice missing arms",
        );
    }};
}

/// A single boundary predicate.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub kind: ConditionKind,
    /// Kind-specific payload (free-form JSON).
    #[serde(default)]
    #[schemars(schema_with = "crate::schema_helpers::preserve_unknown_object")]
    pub params: serde_json::Value,
}

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
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", display, generate_unknown)]
pub enum ConditionKind {
    /// Another Process must be in a given phase.
    /// `params`: `{ "processRef": "...", "namespace": "...", "phase": "Attested" }`
    ProcessPhase,
    /// FluxCD `Kustomization.status.conditions[type=Ready]` must be `True`.
    /// `params`: `{ "name": "...", "namespace": "flux-system" }`
    KustomizationHealthy,
    /// FluxCD `HelmRelease.status.conditions[type=Ready]` must be `True`.
    /// `params`: `{ "name": "...", "namespace": "..." }`
    HelmReleaseReleased,
    /// Prometheus query — truthy scalar required.
    /// `params`: `{ "query": "..." }`
    PromQL,
    /// CEL expression over a scoped object set.
    /// `params`: `{ "expression": "..." }`
    Cel,
    /// Nix evaluation equality check.
    /// `params`: `{ "flakeRef": "...", "attribute": "...", "expect": "..." }`
    NixEval,
    /// A Kubernetes Job must complete successfully and its emitted BLAKE3
    /// receipt must verify.
    /// `params`: `{ "name": "...", "namespace": "...", "expectReceipt": true }`
    JobAttested,
    /// Closed-loop authentication probe — the canonical postcondition for
    /// any system that can produce credentials for its own client under
    /// test. The probe Job (rendered by the VERIFY handler) fetches a
    /// fresh secret from `issuer` (a Service inside the same namespace),
    /// presents it to `consumer` (another Service in the same namespace),
    /// and verifies that `consumer` authenticated successfully against
    /// `jwk_source` (the issuer's published JWK endpoint).
    ///
    /// The Job emits a three-pillar BLAKE3 receipt that the reconciler
    /// chains into `status.attestation`. This turns "the gateway↔SaaS
    /// loop holds" from an assertion into a theorem provable for every
    /// ephemeral run.
    ///
    /// `params`:
    /// ```json
    /// {
    ///   "issuer":   { "service": "demo-app-issuer",
    ///                 "port": 8080,
    ///                 "secretPath": "/v2/get-secret-value" },
    ///   "consumer": { "service": "demo-app-gateway",
    ///                 "port": 8000,
    ///                 "authPath": "/api/v3/auth" },
    ///   "jwkSource":{ "service": "demo-app-issuer",
    ///                 "port": 8080,
    ///                 "path": "/.well-known/jwks.json" },
    ///   "probeImage": "ghcr.io/pleme-io/closed-loop-probe:0.1.0",
    ///   "timeoutSeconds": 120
    /// }
    /// ```
    ClosedLoopAuth,
}

impl ConditionKind {
    /// The closed set of boundary-condition kinds the reconciler honors.
    /// Single source of truth that drives the `as_str` / Display /
    /// `FromStr` triad on this enum and the `stub_message` lift of the
    /// "not yet implemented" arms the reconciler used to hand-roll three
    /// times. Adding a 9th variant lands at one `ALL` entry + one `as_str`
    /// arm + one `stub_message` arm — exhaustively checked by the
    /// compiler (the array literal forces arity).
    ///
    /// Sibling closed-set lifts: [`crate::phase::ProcessPhase::ALL`],
    /// [`crate::signal::ProcessSignal::ALL`], [`crate::intent::IntentKind::ALL`],
    /// [`crate::lifetime::LifetimeKind::ALL`].
    pub const ALL: [Self; 8] = [
        Self::ProcessPhase,
        Self::KustomizationHealthy,
        Self::HelmReleaseReleased,
        Self::PromQL,
        Self::Cel,
        Self::NixEval,
        Self::JobAttested,
        Self::ClosedLoopAuth,
    ];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim. Used by Display
    /// (single source of truth), by `FromStr` to identify the variant
    /// from its annotation / status-field representation, and by
    /// operator-facing diagnostics that need the kind name without
    /// re-serializing the enum through serde_json. Pinned by
    /// `condition_kind_as_str_matches_serde`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProcessPhase => "ProcessPhase",
            Self::KustomizationHealthy => "KustomizationHealthy",
            Self::HelmReleaseReleased => "HelmReleaseReleased",
            Self::PromQL => "PromQL",
            Self::Cel => "Cel",
            Self::NixEval => "NixEval",
            Self::JobAttested => "JobAttested",
            Self::ClosedLoopAuth => "ClosedLoopAuth",
        }
    }

    /// The operator-facing "evaluator not yet implemented" message for
    /// stub kinds — `Some` iff this kind has no live evaluator wired in
    /// `tatara-reconciler::boundary`. ONE site owns the per-kind stub
    /// string; the reconciler's dispatch reaches for this projection
    /// instead of hand-rolling three parallel `Unknown(...)` strings.
    ///
    /// A future variant added as a live evaluator returns `None`; a
    /// future variant added as a stub returns `Some("<kind> evaluator
    /// not yet implemented")` — both reachable through one match
    /// instead of three identical-shape arms drifting in parallel.
    pub const fn stub_message(self) -> Option<&'static str> {
        match self {
            Self::PromQL => Some("PromQL evaluator not yet implemented"),
            Self::Cel => Some("CEL evaluator not yet implemented"),
            Self::NixEval => Some("NixEval evaluator not yet implemented"),
            Self::ProcessPhase
            | Self::KustomizationHealthy
            | Self::HelmReleaseReleased
            | Self::JobAttested
            | Self::ClosedLoopAuth => None,
        }
    }

    /// True iff this kind has no live evaluator (its [`Self::stub_message`]
    /// is `Some`). Pairs with the reconciler's `evaluate` dispatch — a
    /// stub kind unconditionally yields `Satisfaction::Unknown`.
    pub const fn is_stub(self) -> bool {
        self.stub_message().is_some()
    }

    /// The [`FluxResource`] variant this condition kind fetches from
    /// the K8s API server, or `None` for non-Flux-fetching kinds — the
    /// typed projection owning the (ConditionKind → FluxResource)
    /// association every reconciler `evaluate` dispatch arm and every
    /// future coherence check binds through.
    ///
    /// Pre-lift the association was open-coded at TWO adjacent
    /// `evaluate` arms in `tatara-reconciler::boundary::evaluate` past
    /// the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold — each arm
    /// hand-authored a `(FluxResource::X.api_version(),
    /// FluxResource::X.kind())` pair as the two `&str` slots the
    /// pre-lift `evaluate_flux_ready(api_version: &str, kind: &str)`
    /// signature required. Post-lift the mapping lives at ONE typed
    /// projection here, the callee accepts a typed
    /// [`FluxResource`] slot (invalid `(apiVersion, kind)` pairings
    /// like Kustomization's apiVersion paired with HelmRelease's kind
    /// become unrepresentable), and the two `evaluate` arms collapse
    /// onto ONE `KustomizationHealthy | HelmReleaseReleased` OR-arm
    /// that reads the FluxResource variant from `.flux_resource()`.
    ///
    /// A future ConditionKind that fetches a fourth Flux resource
    /// variant (a hypothetical `BucketSynced` kind against a Flux
    /// `Bucket` source) lands as ONE new arm here + ONE new variant
    /// on [`FluxResource`] + ONE OR-pattern extension at the
    /// reconciler dispatch — no hand-authored `(apiVersion, kind)`
    /// pair at the callsite, no widening of the callee's signature.
    ///
    /// The three current non-Flux-fetching arms return `None`:
    /// - `ProcessPhase` fetches a tatara `Process` (through its own
    ///   [`crate::api_version`] + [`crate::PROCESS_KIND`] pair, not
    ///   a Flux `(apiVersion, kind)`).
    /// - `JobAttested` / `ClosedLoopAuth` fetch a `batch/v1::Job` +
    ///   an optional receipt `v1::ConfigMap`, both K8s built-ins
    ///   (not Flux resources).
    /// - `PromQL` / `Cel` / `NixEval` are stub evaluators
    ///   ([`Self::is_stub`]) — no cluster fetch at all.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the (ConditionKind → FluxResource)
    /// association lives at ONE typed algebra projection here, not
    /// at every reconciler dispatch arm).
    pub const fn flux_resource(self) -> Option<FluxResource> {
        match self {
            Self::KustomizationHealthy => Some(FluxResource::Kustomization),
            Self::HelmReleaseReleased => Some(FluxResource::HelmRelease),
            Self::ProcessPhase
            | Self::PromQL
            | Self::Cel
            | Self::NixEval
            | Self::JobAttested
            | Self::ClosedLoopAuth => None,
        }
    }
}

// `impl fmt::Display for ConditionKind` + `impl FromStr for
// ConditionKind` + `impl tatara_lisp::ClosedSet for ConditionKind` +
// `pub struct UnknownConditionKind(pub String)` are generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(via =
// "as_str", display, generate_unknown)]` on the enum declaration above.
// The auto-derived label `"condition kind"` matches the prior hand-
// rolled `#[error("unknown condition kind: {0}")]` verbatim. The
// inherent `as_str` projection stays load-bearing — the PascalCase
// wire-format that matches the serde rename + the CRD `enum:` listing
// verbatim (notably preserving `PromQL`'s consecutive caps that heck
// would have lowercased) — while the trait method `label` gives
// generic consumers a STABLE name across the 36+ workspace-wide
// closed-set implementors.

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serde_process_phase_condition() {
        let c = Condition {
            kind: ConditionKind::ProcessPhase,
            params: json!({ "processRef": "secret-injection", "phase": "Attested" }),
        };
        let yaml = serde_yaml::to_string(&c).unwrap();
        assert!(yaml.contains("kind: ProcessPhase"));
        assert!(yaml.contains("processRef: secret-injection"));
    }

    #[test]
    fn serde_closed_loop_auth_condition() {
        let c = Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({
                "issuer":   { "service": "demo-app-issuer", "port": 8080 },
                "consumer": { "service": "demo-app-gateway", "port": 8000 },
                "probeImage": "ghcr.io/pleme-io/closed-loop-probe:0.1.0",
            }),
        };
        let yaml = serde_yaml::to_string(&c).unwrap();
        assert!(yaml.contains("kind: ClosedLoopAuth"));
        assert!(yaml.contains("probeImage: ghcr.io/pleme-io/closed-loop-probe:0.1.0"));
        let back: Condition = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.kind, ConditionKind::ClosedLoopAuth);
    }

    #[test]
    fn serde_job_attested_condition() {
        let c = Condition {
            kind: ConditionKind::JobAttested,
            params: json!({ "name": "seed-job", "namespace": "demo-test" }),
        };
        let yaml = serde_yaml::to_string(&c).unwrap();
        assert!(yaml.contains("kind: JobAttested"));
    }

    // ── closed-set algebra contracts (ALL × as_str × FromStr × stub_message) ─

    /// Structural well-formedness of [`ConditionKind`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through `label ↔
    /// parse_label`, labels are pairwise distinct, `""` is outside the
    /// closed set) at ONE call site. Replaces the hand-derived
    /// `condition_kind_all_is_unique_and_complete` +
    /// `condition_kind_roundtrip_via_as_str` + the empty-input arm of
    /// `unknown_condition_kind_errors`. `FromStr` delegates to
    /// `<Self as tatara_closed_set::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the reconciler hits when parsing a
    /// CRD `enum:`-validated value back to the typed kind.
    #[test]
    fn condition_kind_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<ConditionKind>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site. The probe
    /// confirmed `PromQL` survives `rename_all = "PascalCase"` as
    /// `"PromQL"` (heck preserves consecutive caps in the leading
    /// word), so this contract is the operator-facing pin.
    #[test]
    fn condition_kind_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<ConditionKind>();
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. If a
    /// reviewer accidentally re-introduces an inline match in
    /// Display, this fails the moment a variant rename touches one
    /// site but not the other.
    #[test]
    fn condition_kind_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<ConditionKind>();
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / unrelated — and the error
    /// echoes the input verbatim so the operator-facing diagnostic
    /// carries the offending value, not a normalized form. The
    /// empty-input arm is pinned by
    /// [`condition_kind_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownConditionKind`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_condition_kind_errors() {
        use std::str::FromStr;
        for bad in ["processPhase", "PROMQL", "Promql", "Bogus"] {
            let err = ConditionKind::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// STUB CONTRACT: the three placeholder evaluators
    /// (PromQL / Cel / NixEval) are exactly the set whose
    /// `stub_message` is `Some`. The five live evaluators return
    /// `None`. A future variant promoted from stub → live must drop
    /// its `stub_message` arm; a new stub must add one. Both
    /// transitions land at this test by sweeping ALL.
    #[test]
    fn condition_kind_stub_set_matches_stubs() {
        use ConditionKind::*;
        for kind in ConditionKind::ALL {
            let expected_is_stub = matches!(kind, PromQL | Cel | NixEval);
            assert_eq!(
                kind.is_stub(),
                expected_is_stub,
                "is_stub disagreed for {kind:?}",
            );
            assert_eq!(
                kind.stub_message().is_some(),
                expected_is_stub,
                "stub_message disagreed for {kind:?}",
            );
        }
    }

    /// Pin the exact stub strings so a rename of the operator-facing
    /// "not yet implemented" message lands at one site (here) instead
    /// of three parallel inline strings in the reconciler.
    #[test]
    fn condition_kind_stub_messages_are_pinned() {
        assert_eq!(
            ConditionKind::PromQL.stub_message(),
            Some("PromQL evaluator not yet implemented"),
        );
        assert_eq!(
            ConditionKind::Cel.stub_message(),
            Some("CEL evaluator not yet implemented"),
        );
        assert_eq!(
            ConditionKind::NixEval.stub_message(),
            Some("NixEval evaluator not yet implemented"),
        );
    }

    // ── (ConditionKind → FluxResource) typed projection contracts ────

    /// The two Flux-fetching kinds project to their canonical
    /// [`FluxResource`] variants. A future ConditionKind rename or
    /// FluxResource variant rename that skewed the projection at ONE
    /// arm surfaces here.
    #[test]
    fn kustomization_healthy_projects_to_flux_resource_kustomization() {
        assert_eq!(
            ConditionKind::KustomizationHealthy.flux_resource(),
            Some(FluxResource::Kustomization),
        );
    }

    #[test]
    fn helm_release_released_projects_to_flux_resource_helm_release() {
        assert_eq!(
            ConditionKind::HelmReleaseReleased.flux_resource(),
            Some(FluxResource::HelmRelease),
        );
    }

    /// The six non-Flux-fetching kinds project to `None`. Sweeps
    /// `ConditionKind::ALL` filtering by `flux_resource().is_none()`
    /// so a new variant added without a `flux_resource` arm surfaces
    /// at rustc's non-exhaustive-match gate BEFORE this test even
    /// runs; a new variant added with a hand-coded `Some(...)` arm
    /// that shouldn't fetch Flux surfaces here.
    #[test]
    fn non_flux_fetching_kinds_project_to_none() {
        use ConditionKind::*;
        let non_flux: Vec<_> = ConditionKind::ALL
            .iter()
            .copied()
            .filter(|k| k.flux_resource().is_none())
            .collect();
        assert_eq!(
            non_flux,
            vec![
                ProcessPhase,
                PromQL,
                Cel,
                NixEval,
                JobAttested,
                ClosedLoopAuth
            ],
        );
    }

    /// Every variant of [`ConditionKind`] whose `flux_resource()` is
    /// `Some` uniquely names its FluxResource variant (no two
    /// ConditionKind arms may fetch the SAME FluxResource — that
    /// would signal a redundant closed-set entry). Peers the
    /// `every_variants_api_version_and_kind_are_distinct_across_the_closed_set`
    /// pin on the sibling [`FluxResource`] closed set.
    #[test]
    fn flux_resource_projection_is_injective_on_the_some_arms() {
        let mut seen = std::collections::HashSet::new();
        for k in ConditionKind::ALL {
            if let Some(fr) = k.flux_resource() {
                assert!(
                    seen.insert(fr),
                    "duplicate FluxResource projection at {k:?}: {fr:?}",
                );
            }
        }
    }

    /// `flux_resource` is `const fn` — the projection is reachable
    /// at compile time. A regression that dropped the `const`
    /// qualifier would fail-loudly here rather than as a wrong-slot
    /// runtime dispatch at every consumer callsite.
    #[test]
    fn flux_resource_projection_is_const_fn_reachable() {
        const K: Option<FluxResource> = ConditionKind::KustomizationHealthy.flux_resource();
        const H: Option<FluxResource> = ConditionKind::HelmReleaseReleased.flux_resource();
        const P: Option<FluxResource> = ConditionKind::ProcessPhase.flux_resource();
        assert_eq!(K, Some(FluxResource::Kustomization));
        assert_eq!(H, Some(FluxResource::HelmRelease));
        assert_eq!(P, None);
    }

    // ── Boundary::has_condition_kind substrate pins ──────────────────
    //
    // Fail-before-pass-after granularity: `Boundary::has_condition_kind`
    // did not exist before this commit — the (preconditions +
    // postconditions .iter().any(|c| c.kind == K)) union-probe shape
    // lived hand-authored inline at the ephemeral require-tag surface
    // (`spec.postconditions.iter().any(|c| matches!(c.kind, K))`, sans
    // the pre-condition side). The lift places the closed-set-driven
    // presence probe on ONE substrate site so the point-domain
    // `condition-<kind>` prefix family in `tatara-check` composes it
    // through `strip_and_classify_prefixed_kind` byte-for-byte
    // symmetrical with `intent-<kind>` (via `Intent::has`) +
    // `lifetime-<kind>` (via `Lifetime::has`) — third instance in the
    // workspace closed-set-driven presence-probe algebra.

    fn condition_with(kind: ConditionKind) -> Condition {
        Condition {
            kind,
            params: json!({}),
        }
    }

    /// EMPTY-BOUNDARY pin — a default [`Boundary`] (no preconditions,
    /// no postconditions) returns `false` for EVERY [`ConditionKind`].
    /// Sweep `ConditionKind::ALL` so a new variant added without a
    /// matching arm in the presence probe surfaces at rustc's
    /// exhaustiveness gate on the ALL literal (arity forced by
    /// `[Self; 8]`) rather than as a silent false-positive at every
    /// downstream `condition-<kind>` require-tag callsite.
    #[test]
    fn has_condition_kind_returns_false_on_empty_boundary_for_every_kind() {
        let b = Boundary::default();
        for kind in ConditionKind::ALL {
            assert!(
                !b.has_condition_kind(kind),
                "default boundary must return false for {kind:?}",
            );
        }
    }

    /// POSTCONDITION-only pin — a boundary that carries the kind on
    /// ONLY postconditions returns `true` for that kind, `false` for
    /// every other variant. Sweep the ALL × ALL cross so a regression
    /// that (a) hard-coded the arm to a single kind (silently
    /// returning true for every populated boundary regardless of
    /// which kind was queried), (b) skipped the postcondition side of
    /// the union (silently returning false when the kind lived
    /// post-only), or (c) matched on Condition::params instead of
    /// Condition::kind fails HERE at the substrate primitive.
    #[test]
    fn has_condition_kind_reads_postconditions_per_kind() {
        for populated in ConditionKind::ALL {
            let mut b = Boundary::default();
            b.postconditions.push(condition_with(populated));
            for query in ConditionKind::ALL {
                let expected = query == populated;
                assert_eq!(
                    b.has_condition_kind(query),
                    expected,
                    "postcondition populated={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// PRECONDITION-only pin — mirrors the postcondition sweep on the
    /// other half of the union. Locks the union semantics on both
    /// halves separately so a regression that dropped the
    /// pre-condition side of the OR fails here even though the
    /// postcondition-side pin above passes.
    #[test]
    fn has_condition_kind_reads_preconditions_per_kind() {
        for populated in ConditionKind::ALL {
            let mut b = Boundary::default();
            b.preconditions.push(condition_with(populated));
            for query in ConditionKind::ALL {
                let expected = query == populated;
                assert_eq!(
                    b.has_condition_kind(query),
                    expected,
                    "precondition populated={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// UNION pin — a kind that appears on preconditions returns
    /// `true` even when postconditions carries a DIFFERENT kind, and
    /// vice versa. Pins the OR-composition of the two halves so a
    /// regression that collapsed the union to an intersection (AND)
    /// silently reclassifies pre-only or post-only kinds as absent.
    #[test]
    fn has_condition_kind_unions_pre_and_post_condition_arms() {
        let mut b = Boundary::default();
        b.preconditions
            .push(condition_with(ConditionKind::KustomizationHealthy));
        b.postconditions
            .push(condition_with(ConditionKind::ClosedLoopAuth));
        assert!(
            b.has_condition_kind(ConditionKind::KustomizationHealthy),
            "pre-only kind must resolve through the union",
        );
        assert!(
            b.has_condition_kind(ConditionKind::ClosedLoopAuth),
            "post-only kind must resolve through the union",
        );
        assert!(
            !b.has_condition_kind(ConditionKind::PromQL),
            "an absent kind must return false even with populated halves",
        );
    }

    // ── ConditionSliceExt::has_kind substrate pins ────────────────────
    //
    // Fail-before-pass-after granularity: `ConditionSliceExt::has_kind`
    // did not exist before this commit — the `(&[Condition],
    // ConditionKind) -> bool` walk shape lived hand-authored inline at
    // THREE production sites (twice inside `Boundary::has_condition_kind`
    // on `preconditions` ∪ `postconditions`, once at the ephemeral
    // require-tag classifier's `closed-loop-auth` arm on
    // `spec.postconditions` in `tatara-reconciler::bin::tatara-check`,
    // with `matches!` sugar instead of `==` but the same predicate).
    // The lift places the per-slice presence probe on ONE substrate site
    // so the two-half union at `Boundary` and the one-half probe at the
    // ephemeral surface compose against the SAME primitive rather than
    // restating the `.iter().any(|c| c.kind == K)` closure body.

    /// EMPTY-SLICE pin — an empty `&[Condition]` returns `false` for
    /// EVERY [`ConditionKind`]. Sweep `ConditionKind::ALL` so a new
    /// variant added without a matching arm in the primitive surfaces
    /// at rustc's exhaustiveness gate on the ALL literal (arity forced
    /// by `[Self; 8]`) rather than as a silent false-positive at every
    /// downstream callsite composing this primitive.
    #[test]
    fn condition_slice_has_kind_returns_false_on_empty_slice_for_every_kind() {
        let empty: &[Condition] = &[];
        for kind in ConditionKind::ALL {
            assert!(
                !empty.has_kind(kind),
                "empty slice must return false for {kind:?}",
            );
        }
    }

    /// PER-VARIANT pin — a single-element slice returns `true` for
    /// exactly the kind it carries, `false` for every other variant.
    /// Sweep the ALL × ALL cross so a regression that (a) hard-coded
    /// the arm to a single kind (silently returning true for every
    /// populated slice regardless of query kind), or (b) matched on
    /// [`Condition::params`] instead of [`Condition::kind`] fails HERE
    /// at the substrate primitive.
    #[test]
    fn condition_slice_has_kind_reads_kind_field_per_variant() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            for query in ConditionKind::ALL {
                let expected = query == populated;
                assert_eq!(
                    slice.has_kind(query),
                    expected,
                    "populated={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// MULTI-ENTRY pin — a slice with multiple entries returns `true`
    /// for every kind that appears at any position (existential
    /// quantifier over the slice), `false` for kinds that appear at
    /// no position. Locks the `any` semantics so a regression that
    /// collapsed to a `first`-only probe (`slice.first().map_or(false,
    /// |c| c.kind == kind)`) fails here even though the single-element
    /// per-variant pin above passes.
    #[test]
    fn condition_slice_has_kind_scans_beyond_the_first_position() {
        let slice = [
            condition_with(ConditionKind::KustomizationHealthy),
            condition_with(ConditionKind::ClosedLoopAuth),
            condition_with(ConditionKind::JobAttested),
        ];
        for present in [
            ConditionKind::KustomizationHealthy,
            ConditionKind::ClosedLoopAuth,
            ConditionKind::JobAttested,
        ] {
            assert!(
                slice.has_kind(present),
                "kind at any position must resolve true: {present:?}",
            );
        }
        for absent in [
            ConditionKind::ProcessPhase,
            ConditionKind::HelmReleaseReleased,
            ConditionKind::PromQL,
            ConditionKind::Cel,
            ConditionKind::NixEval,
        ] {
            assert!(
                !slice.has_kind(absent),
                "kind absent from the slice must resolve false: {absent:?}",
            );
        }
    }

    /// COMPOSITION pin — [`Boundary::has_condition_kind`] equals the OR
    /// of the two half-slice probes at EVERY (populated arrangement,
    /// query) pair on `ConditionKind::ALL`. Locks the (union-probe =
    /// pre.has_kind ∨ post.has_kind) composition contract at ONE test
    /// so a regression that (a) dropped the `||` (silently narrowing
    /// the union to an intersection, or to one side only), or
    /// (b) hand-authored the union with a divergent walk shape (e.g.
    /// summing counts, comparing lengths) surfaces HERE at the
    /// composition boundary rather than as silent classifier drift at
    /// every downstream `condition-<kind>` require-tag callsite.
    #[test]
    fn boundary_has_condition_kind_equals_or_of_half_slice_probes() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));
                for query in ConditionKind::ALL {
                    let expected =
                        b.preconditions.has_kind(query) || b.postconditions.has_kind(query);
                    assert_eq!(
                        b.has_condition_kind(query),
                        expected,
                        "union drifted: pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    // ── Boundary::has_(pre|post)condition_kind substrate pins ────────
    //
    // Fail-before-pass-after granularity: the two half-slice arms did
    // not exist before this commit — the point-domain `precondition-
    // <kind>` and `postcondition-<kind>` require-tag classifiers in
    // `tatara-reconciler::bin::tatara-check` reached the two condition
    // slices through direct field access
    // (`spec.boundary.preconditions.has_kind(k)`), bypassing the named
    // [`Boundary`] primitive surface that the union-probe
    // [`Boundary::has_condition_kind`] already routed through. The
    // lift closes the (precondition, postcondition, union) triad on
    // ONE typed algebra surface so a future normalization at the
    // presence-probe shape lands at ONE site for all three arms.

    /// EMPTY-BOUNDARY pin (precondition arm) — a default [`Boundary`]
    /// returns `false` for EVERY [`ConditionKind`] on the precondition
    /// side. Sweep `ConditionKind::ALL` so a new variant added without
    /// a matching arm on the probe surfaces at rustc's exhaustiveness
    /// gate on the ALL literal (arity forced by `[Self; 8]`) rather
    /// than as a silent false-positive at every downstream
    /// `precondition-<kind>` require-tag callsite.
    #[test]
    fn has_precondition_kind_returns_false_on_empty_boundary_for_every_kind() {
        let b = Boundary::default();
        for kind in ConditionKind::ALL {
            assert!(
                !b.has_precondition_kind(kind),
                "default boundary must return false on precondition arm for {kind:?}",
            );
        }
    }

    /// EMPTY-BOUNDARY pin (postcondition arm) — sibling of the
    /// precondition-arm empty pin above on the other half of the
    /// (precondition, postcondition) partition. Locks the empty-slice
    /// arm return on the postcondition side so a regression that
    /// wired the postcondition arm to the precondition slice surfaces
    /// HERE at fail-before-pass-after granularity.
    #[test]
    fn has_postcondition_kind_returns_false_on_empty_boundary_for_every_kind() {
        let b = Boundary::default();
        for kind in ConditionKind::ALL {
            assert!(
                !b.has_postcondition_kind(kind),
                "default boundary must return false on postcondition arm for {kind:?}",
            );
        }
    }

    /// SLICE-SELECTIVITY pin (precondition arm) — a boundary with a
    /// kind on the precondition side ONLY resolves `true` at
    /// `has_precondition_kind` and `false` at `has_postcondition_kind`.
    /// Locks the (side-select, kind-select) partition so a regression
    /// that pointed the precondition arm at `self.postconditions` (a
    /// copy-paste from the sibling arm) surfaces HERE rather than as
    /// silent classifier drift at every downstream
    /// `precondition-<kind>` require-tag callsite.
    #[test]
    fn has_precondition_kind_reads_preconditions_slice_only() {
        for populated in ConditionKind::ALL {
            let mut b = Boundary::default();
            b.preconditions.push(condition_with(populated));
            for query in ConditionKind::ALL {
                let expected_pre = query == populated;
                assert_eq!(
                    b.has_precondition_kind(query),
                    expected_pre,
                    "precondition-only populated={populated:?}: query {query:?} drifted \
                     on precondition arm",
                );
                assert!(
                    !b.has_postcondition_kind(query),
                    "precondition-only populated={populated:?}: query {query:?} must \
                     return false on postcondition arm (postconditions is empty)",
                );
            }
        }
    }

    /// SLICE-SELECTIVITY pin (postcondition arm) — mirror of the
    /// precondition-only sweep on the other half. Locks the sibling
    /// arm's binding to `self.postconditions` so a regression that
    /// pointed the postcondition arm at `self.preconditions` fails
    /// HERE even though the precondition-arm pin above passes.
    #[test]
    fn has_postcondition_kind_reads_postconditions_slice_only() {
        for populated in ConditionKind::ALL {
            let mut b = Boundary::default();
            b.postconditions.push(condition_with(populated));
            for query in ConditionKind::ALL {
                let expected_post = query == populated;
                assert_eq!(
                    b.has_postcondition_kind(query),
                    expected_post,
                    "postcondition-only populated={populated:?}: query {query:?} \
                     drifted on postcondition arm",
                );
                assert!(
                    !b.has_precondition_kind(query),
                    "postcondition-only populated={populated:?}: query {query:?} must \
                     return false on precondition arm (preconditions is empty)",
                );
            }
        }
    }

    /// COMPOSITION-LAW pin — [`Boundary::has_condition_kind`] equals
    /// `has_precondition_kind(k) || has_postcondition_kind(k)` at
    /// EVERY (pre-populated, post-populated, query) triple on
    /// `ConditionKind::ALL`. This is the load-bearing invariant that
    /// makes the (precondition, postcondition, union) triad on
    /// [`Boundary`] a first-class typed algebra rather than a
    /// per-caller discipline: the two half-slice arms + the union arm
    /// compose exactly as `union == pre ∨ post`, and every downstream
    /// `condition-<K> = precondition-<K> ∨ postcondition-<K>` classifier
    /// invariant on `tatara-reconciler::bin::tatara-check` inherits it
    /// mechanically. A regression that (a) dropped the composition (by
    /// re-inlining `.has_kind(kind)` bodies on the union arm), or
    /// (b) drifted ONE of the two half-slice arms without updating the
    /// other, surfaces HERE rather than as silent per-side classifier
    /// drift at the require-tag surfaces.
    #[test]
    fn boundary_has_condition_kind_composes_precondition_and_postcondition_arms() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));
                for query in ConditionKind::ALL {
                    let via_arms =
                        b.has_precondition_kind(query) || b.has_postcondition_kind(query);
                    assert_eq!(
                        b.has_condition_kind(query),
                        via_arms,
                        "union arm drifted from OR of half-slice arms: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin — the two half-slice arms delegate
    /// verbatim to [`ConditionSliceExt::has_kind`] on the underlying
    /// [`Vec<Condition>`] slice, no inline reimplementation. Sweep the
    /// full `ConditionKind::ALL` × `ConditionKind::ALL` cross so a
    /// regression that inlined a divergent walk (`.iter().find(_).
    /// is_some()`, an `.any(|c| matches!(c.kind, K))` that missed a
    /// variant) at either arm surfaces HERE at the substrate
    /// boundary rather than as silent skew between the struct-level
    /// arm and the slice-level primitive downstream consumers reach
    /// through.
    #[test]
    fn has_precondition_and_postcondition_kind_delegate_to_slice_has_kind() {
        for populated in ConditionKind::ALL {
            let mut b = Boundary::default();
            b.preconditions.push(condition_with(populated));
            b.postconditions.push(condition_with(populated));
            for query in ConditionKind::ALL {
                assert_eq!(
                    b.has_precondition_kind(query),
                    b.preconditions.has_kind(query),
                    "precondition arm must delegate to preconditions.has_kind: \
                     populated={populated:?} query={query:?}",
                );
                assert_eq!(
                    b.has_postcondition_kind(query),
                    b.postconditions.has_kind(query),
                    "postcondition arm must delegate to postconditions.has_kind: \
                     populated={populated:?} query={query:?}",
                );
            }
        }
    }

    // ── ConditionSliceExt::find_kind substrate pins + widened triad ──
    //
    // Fail-before-pass-after granularity: `ConditionSliceExt::find_kind`
    // + its three struct-level peers (`Boundary::find_(pre|post)?
    // condition_kind`) did not exist before this commit — the existing
    // `has_*_kind` triad collapses the return to `bool`, losing the
    // matching `&Condition` a future diagnostic consumer (an operator-
    // facing "found on {pre|post}conditions at param.probeImage=X"
    // message, a coherence check verifying "every ClosedLoopAuth
    // postcondition carries a non-empty probeImage", an editor
    // completion listing params-keys per present kind) needs. The lift
    // widens the primitive to `Option<&Condition>` and re-anchors
    // `has_kind` as a default composed from it, so the two refinements
    // share ONE walk semantics by construction.

    /// EMPTY-SLICE pin — an empty `&[Condition]` returns `None` from
    /// `find_kind` for EVERY [`ConditionKind`]. Sweep
    /// `ConditionKind::ALL` so a new variant added without a matching
    /// arm in the primitive surfaces at rustc's exhaustiveness gate on
    /// the ALL literal (arity forced by `[Self; 8]`) rather than as a
    /// silent false-`Some` at every downstream widened callsite.
    #[test]
    fn condition_slice_find_kind_returns_none_on_empty_slice_for_every_kind() {
        let empty: &[Condition] = &[];
        for kind in ConditionKind::ALL {
            assert!(
                empty.find_kind(kind).is_none(),
                "empty slice must return None for {kind:?}",
            );
        }
    }

    /// PER-VARIANT pin — a single-element slice returns `Some` with
    /// the matching kind for exactly the kind it carries, `None` for
    /// every other variant. Sweep the ALL × ALL cross so a regression
    /// that (a) hard-coded the arm to a single kind (silently returning
    /// `Some` for every populated slice regardless of query kind), or
    /// (b) matched on [`Condition::params`] instead of [`Condition::kind`]
    /// fails HERE at the substrate primitive.
    #[test]
    fn condition_slice_find_kind_reads_kind_field_per_variant() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            for query in ConditionKind::ALL {
                let hit = slice.find_kind(query);
                if query == populated {
                    assert_eq!(
                        hit.map(|c| c.kind),
                        Some(populated),
                        "populated={populated:?}: query {query:?} must return Some",
                    );
                } else {
                    assert!(
                        hit.is_none(),
                        "populated={populated:?}: query {query:?} must return None",
                    );
                }
            }
        }
    }

    /// FIRST-MATCH pin — a slice with the same kind at MULTIPLE
    /// positions returns the earliest by position. Locks the `.iter().
    /// find(...)` semantics so a regression that collapsed to a
    /// `.last()` walk (returning the trailing match) or a `.rev().
    /// find(...)` walk (returning the last-inserted match) surfaces
    /// HERE, since diagnostic consumers reading `find_kind(K).unwrap().
    /// params` expect the FIRST occurrence's params-payload not the
    /// last.
    #[test]
    fn condition_slice_find_kind_returns_first_position_on_duplicate_kinds() {
        // Two ClosedLoopAuth entries with distinct params — a first-
        // match walk resolves to the leading entry's params-payload.
        let first = Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({ "probeImage": "first" }),
        };
        let second = Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({ "probeImage": "second" }),
        };
        let slice = [first, second];
        let hit = slice
            .find_kind(ConditionKind::ClosedLoopAuth)
            .expect("populated slice must resolve Some on the matching kind");
        assert_eq!(
            hit.params
                .get("probeImage")
                .and_then(serde_json::Value::as_str),
            Some("first"),
            "find_kind must return the FIRST position's Condition on duplicate kinds",
        );
    }

    /// SLICE-LEVEL DELEGATION pin (has ↔ find) — [`ConditionSliceExt::has_kind`]
    /// equals `find_kind(k).is_some()` at EVERY (populated arrangement,
    /// query) pair on `ConditionKind::ALL`. Turns the trait doc's
    /// "compounding" note ("the closed-set discriminator case becomes
    /// `has_kind(k) == self.find_kind(k).is_some()` by construction")
    /// into a first-class typed test invariant: a future consumer
    /// that overrode the default `has_kind` body with a divergent walk
    /// shape (a `.iter().any(...)` that missed a variant, a `.count() >
    /// 0` predicate on a filtered clone) surfaces HERE at the substrate
    /// boundary rather than as silent skew between the two refinements
    /// downstream consumers reach through.
    #[test]
    fn condition_slice_has_kind_equals_find_kind_is_some() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let slice = [condition_with(pre_kind), condition_with(post_kind)];
                for query in ConditionKind::ALL {
                    assert_eq!(
                        slice.has_kind(query),
                        slice.find_kind(query).is_some(),
                        "slice-level has/find refinement bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (find-triad) — the three widened
    /// `find_*_kind` methods on [`Boundary`] delegate verbatim to
    /// [`ConditionSliceExt::find_kind`] on the underlying
    /// [`Vec<Condition>`] slices, no inline reimplementation. The
    /// `find_condition_kind` union walks preconditions first then
    /// postconditions via `Option::or_else`. Sweep
    /// `ConditionKind::ALL × ConditionKind::ALL × ConditionKind::ALL`
    /// so a regression that (a) inlined a divergent walk at either
    /// half-slice arm, (b) reversed the union walk order (postcondition
    /// first), or (c) collapsed `or_else` to `and_then` (silently
    /// narrowing the union to an intersection) surfaces HERE at the
    /// substrate boundary rather than as silent skew between the
    /// struct-level widened arms and the slice-level primitive.
    #[test]
    fn find_condition_kind_triad_delegates_to_slice_find_kind() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));
                for query in ConditionKind::ALL {
                    let via_pre = b.preconditions.find_kind(query);
                    let via_post = b.postconditions.find_kind(query);
                    assert_eq!(
                        b.find_precondition_kind(query).map(|c| c.kind),
                        via_pre.map(|c| c.kind),
                        "precondition find arm must delegate to preconditions.find_kind: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        b.find_postcondition_kind(query).map(|c| c.kind),
                        via_post.map(|c| c.kind),
                        "postcondition find arm must delegate to postconditions.find_kind: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    let expected_union = via_pre.or(via_post).map(|c| c.kind);
                    assert_eq!(
                        b.find_condition_kind(query).map(|c| c.kind),
                        expected_union,
                        "union find arm must equal precondition.or_else(postcondition): \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// PRECONDITION-PRECEDENCE pin — a kind authored on BOTH sides
    /// returns the precondition-side [`Condition`] from
    /// `find_condition_kind`. Uses two params-distinguishable
    /// [`Condition`]s so a regression that reversed the walk order
    /// (postcondition first) surfaces at the returned params payload
    /// rather than silently at the presence bit (which is `true` on
    /// both walk orders).
    #[test]
    fn find_condition_kind_returns_precondition_side_on_dual_populated() {
        let mut b = Boundary::default();
        b.preconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({ "side": "pre" }),
        });
        b.postconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({ "side": "post" }),
        });
        let hit = b
            .find_condition_kind(ConditionKind::ClosedLoopAuth)
            .expect("dual-populated boundary must resolve Some");
        assert_eq!(
            hit.params.get("side").and_then(serde_json::Value::as_str),
            Some("pre"),
            "find_condition_kind must walk preconditions first: dual-populated kind \
             returned postcondition-side Condition rather than precondition-side",
        );
    }

    /// STRUCT-LEVEL DELEGATION pin (has ↔ find) — the three
    /// [`Boundary`] `has_*_kind` arms equal their widened peers'
    /// `.is_some()` projection at EVERY (pre-populated, post-populated,
    /// query) triple on `ConditionKind::ALL`. The three widened
    /// `find_*_kind` arms are the load-bearing primitives; the three
    /// `has_*_kind` arms are their bool projections. Byte-for-byte
    /// re-anchors the composition-law pin
    /// `boundary_has_condition_kind_composes_precondition_and_postcondition_arms`
    /// through the widened axis so a future consumer that reads
    /// `has_condition_kind` as sugar for `find_condition_kind(k).
    /// is_some()` (rather than as `has_precondition_kind ||
    /// has_postcondition_kind`) stays typed against the SAME truth
    /// table.
    #[test]
    fn boundary_has_triad_equals_find_triad_is_some_projection() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));
                for query in ConditionKind::ALL {
                    assert_eq!(
                        b.has_precondition_kind(query),
                        b.find_precondition_kind(query).is_some(),
                        "precondition has/find bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        b.has_postcondition_kind(query),
                        b.find_postcondition_kind(query).is_some(),
                        "postcondition has/find bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        b.has_condition_kind(query),
                        b.find_condition_kind(query).is_some(),
                        "union has/find bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    // ── ConditionSliceExt::iter_kind substrate pins + widened triad ──
    //
    // Fail-before-pass-after granularity: `ConditionSliceExt::iter_kind`
    // + its three struct-level peers (`Boundary::iter_(pre|post|)?
    // condition_kind`) did not exist before this commit — the existing
    // `find_*_kind` triad collapses the return to `Option<&Condition>`
    // (yielding only the FIRST match), losing the full match stream a
    // future coherence check ("each ConditionKind appears at most
    // once per side" — `iter_kind(k).nth(1).is_none()`) or diagnostic
    // consumer ("N ClosedLoopAuth postconditions matched, listing
    // every param.probeImage" — `iter_kind(k).collect()`) needs. The
    // lift widens the primitive to `KindMatches<'_>` (a named
    // Iterator<Item = &Condition>) and re-anchors `find_kind` as a
    // default composed from it (`self.iter_kind(kind).next()`), so
    // the three refinements share ONE walk semantics by construction.

    /// EMPTY-SLICE pin (iter) — an empty `&[Condition]` yields
    /// nothing from `iter_kind` for EVERY [`ConditionKind`]. Sweep
    /// `ConditionKind::ALL` so a new variant added without a matching
    /// arm in the primitive surfaces at rustc's exhaustiveness gate
    /// on the ALL literal rather than as a silent phantom-yield at
    /// every downstream widened callsite.
    #[test]
    fn condition_slice_iter_kind_yields_nothing_on_empty_slice_for_every_kind() {
        let empty: &[Condition] = &[];
        for kind in ConditionKind::ALL {
            assert_eq!(
                empty.iter_kind(kind).count(),
                0,
                "empty slice must yield nothing on iter_kind for {kind:?}",
            );
        }
    }

    /// PER-VARIANT pin (iter) — a single-element slice yields exactly
    /// that element on the matching kind and nothing on every other
    /// kind. Sweep the ALL × ALL cross so a regression that (a)
    /// hard-coded the filter predicate to a single kind (silently
    /// yielding on every populated slice regardless of query kind),
    /// or (b) matched on [`Condition::params`] instead of
    /// [`Condition::kind`] fails HERE at the substrate primitive.
    #[test]
    fn condition_slice_iter_kind_reads_kind_field_per_variant() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            for query in ConditionKind::ALL {
                let collected: Vec<_> = slice.iter_kind(query).map(|c| c.kind).collect();
                if query == populated {
                    assert_eq!(
                        collected,
                        vec![populated],
                        "populated={populated:?}: query {query:?} must yield [populated]",
                    );
                } else {
                    assert!(
                        collected.is_empty(),
                        "populated={populated:?}: query {query:?} must yield nothing",
                    );
                }
            }
        }
    }

    /// ALL-MATCHES pin — a slice with the same kind at MULTIPLE
    /// positions yields EVERY match in slice order (not just the
    /// first). Uses params-distinguishable [`Condition`]s so a
    /// regression that (a) collapsed to a single-match walk
    /// (`.iter().find(...)` yielding only the earliest and
    /// terminating), (b) reversed the yield order (`.rev().filter`
    /// yielding trailing-first), or (c) de-duplicated by kind (an
    /// erroneous `HashSet::insert`-gated walk) surfaces HERE at the
    /// params payload rather than silently at a downstream
    /// count-based coherence check.
    #[test]
    fn condition_slice_iter_kind_yields_every_match_in_slice_order_on_duplicates() {
        let first = Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({ "probeImage": "first" }),
        };
        let middle = Condition {
            kind: ConditionKind::PromQL,
            params: json!({ "query": "up" }),
        };
        let second_cla = Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({ "probeImage": "second" }),
        };
        let slice = [first, middle, second_cla];
        let hits: Vec<_> = slice
            .iter_kind(ConditionKind::ClosedLoopAuth)
            .map(|c| {
                c.params
                    .get("probeImage")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            hits,
            vec!["first".to_owned(), "second".to_owned()],
            "iter_kind must yield every match in slice order (not just the first)",
        );
        // The interleaved non-matching kind is skipped: two hits, not three.
        assert_eq!(
            slice.iter_kind(ConditionKind::ClosedLoopAuth).count(),
            2,
            "iter_kind must skip non-matching kinds, not include them in the stream",
        );
    }

    /// SLICE-LEVEL DELEGATION pin (find ↔ iter) — the trait's default
    /// `find_kind` body equals `iter_kind(k).next()` at EVERY
    /// (populated arrangement, query) pair on `ConditionKind::ALL`.
    /// Turns the trait doc's composition-law note
    /// ("`find_kind(k) == iter_kind(k).next()` by construction")
    /// into a first-class typed test invariant: a future implementor
    /// that overrode the default `find_kind` body with a divergent
    /// walk shape (a `.iter().rev().find(...)` returning trailing-
    /// first, a hand-rolled loop that walked past the first match)
    /// surfaces HERE at the substrate boundary rather than as silent
    /// skew between the two refinements downstream consumers reach
    /// through.
    #[test]
    fn condition_slice_find_kind_equals_iter_kind_next() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let slice = [condition_with(pre_kind), condition_with(post_kind)];
                for query in ConditionKind::ALL {
                    assert_eq!(
                        slice.find_kind(query).map(|c| c.kind),
                        slice.iter_kind(query).next().map(|c| c.kind),
                        "slice-level find/iter refinement bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (Boundary iter-triad) — the three
    /// widened `iter_*_kind` methods on [`Boundary`] delegate verbatim
    /// to [`ConditionSliceExt::iter_kind`] on the underlying
    /// [`Vec<Condition>`] slices, no inline reimplementation. The
    /// `iter_condition_kind` union chains preconditions first then
    /// postconditions via [`Iterator::chain`]. Sweep
    /// `ConditionKind::ALL × ConditionKind::ALL × ConditionKind::ALL`
    /// so a regression that (a) inlined a divergent walk at either
    /// half-slice arm, (b) reversed the chain order (postcondition
    /// first — walk-order regression on the union), or (c) collapsed
    /// the chain to a `.zip(...)` (silently narrowing the union to
    /// an intersection-by-position) surfaces HERE at the substrate
    /// boundary rather than as silent skew between the struct-level
    /// widened arms and the slice-level primitive.
    #[test]
    fn iter_condition_kind_triad_delegates_to_slice_iter_kind() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));
                for query in ConditionKind::ALL {
                    let via_pre: Vec<_> =
                        b.preconditions.iter_kind(query).map(|c| c.kind).collect();
                    let via_post: Vec<_> =
                        b.postconditions.iter_kind(query).map(|c| c.kind).collect();
                    assert_eq!(
                        b.iter_precondition_kind(query)
                            .map(|c| c.kind)
                            .collect::<Vec<_>>(),
                        via_pre,
                        "precondition iter arm must delegate to preconditions.iter_kind: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        b.iter_postcondition_kind(query)
                            .map(|c| c.kind)
                            .collect::<Vec<_>>(),
                        via_post,
                        "postcondition iter arm must delegate to postconditions.iter_kind: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    let mut expected_union = via_pre.clone();
                    expected_union.extend(via_post.iter().copied());
                    assert_eq!(
                        b.iter_condition_kind(query)
                            .map(|c| c.kind)
                            .collect::<Vec<_>>(),
                        expected_union,
                        "union iter arm must chain precondition ⨟ postcondition: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// STRUCT-LEVEL DELEGATION pin (find ↔ iter on Boundary) — the
    /// three [`Boundary`] `find_*_kind` arms equal their widened
    /// peers' `.next()` projection at EVERY (pre-populated,
    /// post-populated, query) triple on `ConditionKind::ALL`. Byte-
    /// for-byte re-anchors the composition-law pin
    /// `find_condition_kind == iter_condition_kind.next()` through
    /// the widened axis on the parent surface — a future consumer
    /// that reads `find_condition_kind(k)` as sugar for
    /// `iter_condition_kind(k).next()` stays typed against the SAME
    /// truth table on both the slice-level and struct-level layers.
    #[test]
    fn boundary_find_triad_equals_iter_triad_next_projection() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));
                for query in ConditionKind::ALL {
                    assert_eq!(
                        b.find_precondition_kind(query).map(|c| c.kind),
                        b.iter_precondition_kind(query).next().map(|c| c.kind),
                        "precondition find/iter bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        b.find_postcondition_kind(query).map(|c| c.kind),
                        b.iter_postcondition_kind(query).next().map(|c| c.kind),
                        "postcondition find/iter bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        b.find_condition_kind(query).map(|c| c.kind),
                        b.iter_condition_kind(query).next().map(|c| c.kind),
                        "union find/iter bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// PRECONDITION-PRECEDENCE pin (iter) — a kind authored on BOTH
    /// sides yields precondition-side matches FIRST in the union
    /// chain. Uses params-distinguishable [`Condition`]s so a
    /// regression that (a) reversed the chain order on the widened
    /// axis (postcondition first), (b) interleaved the two sides,
    /// or (c) collapsed the chain to a `.zip(...)` fails at the
    /// returned params-payload sequence rather than silently at the
    /// count.
    #[test]
    fn iter_condition_kind_yields_preconditions_before_postconditions_on_dual_populated() {
        let mut b = Boundary::default();
        b.preconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({ "side": "pre-1" }),
        });
        b.preconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({ "side": "pre-2" }),
        });
        b.postconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({ "side": "post-1" }),
        });
        let sides: Vec<_> = b
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
            vec!["pre-1".to_owned(), "pre-2".to_owned(), "post-1".to_owned(),],
            "iter_condition_kind must yield every precondition-side match before any \
             postcondition-side match (chain order pinned by two-surface parity contract)",
        );
    }

    // ----- count_kind — scalar cardinality refinement --------------------
    //
    // The `count_kind` fourth refinement collapses the widened
    // `iter_kind` stream to its cardinality without materializing an
    // intermediate `Vec` or `Option`. Distinct composition law from the
    // three prior refinements: `count_condition_kind` SUMS pre + post
    // (rather than OR-ing them via `has`, or_else-ing them via `find`,
    // or Chain-ing them via `iter`). The tests below pin (a) the default
    // trait body against the primitive `iter_kind(k).count()`, (b) the
    // slice-level composition laws `has_kind(k) == (count_kind(k) > 0)`
    // and `find_kind(k).is_some() == (count_kind(k) > 0)`, (c) the
    // struct-level SUM composition on both `Boundary` half-slice arms,
    // and (d) the two-surface parity contract with
    // `EphemeralSpec::count_(pre|post|)condition_kind` (in ephemeral.rs).

    /// EMPTY-SLICE pin (count) — an empty `&[Condition]` returns `0`
    /// from `count_kind` for EVERY [`ConditionKind`]. Sweep
    /// `ConditionKind::ALL` so a new variant added without a matching
    /// arm surfaces at rustc's exhaustiveness gate on the ALL literal
    /// rather than as silent phantom-cardinality at every downstream
    /// count callsite.
    #[test]
    fn condition_slice_count_kind_returns_zero_on_empty_slice_for_every_kind() {
        let empty: &[Condition] = &[];
        for kind in ConditionKind::ALL {
            assert_eq!(
                empty.count_kind(kind),
                0,
                "empty slice must count 0 for {kind:?}",
            );
        }
    }

    /// PER-VARIANT pin (count) — a single-element slice returns `1`
    /// on the matching kind and `0` on every other kind. Sweep ALL ×
    /// ALL so a regression that (a) hard-coded the filter predicate
    /// to a single kind (silently counting every populated slice
    /// regardless of query), or (b) matched on [`Condition::params`]
    /// instead of [`Condition::kind`] fails HERE at the substrate
    /// primitive.
    #[test]
    fn condition_slice_count_kind_reads_kind_field_per_variant() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            for query in ConditionKind::ALL {
                let expected = if query == populated { 1 } else { 0 };
                assert_eq!(
                    slice.count_kind(query),
                    expected,
                    "populated={populated:?} query={query:?} \
                     must count {expected}",
                );
            }
        }
    }

    /// DUPLICATES pin (count) — a slice with the same kind at
    /// MULTIPLE positions returns the exact match count (not `1`, not
    /// a de-duplicated `1`). A regression that (a) short-circuited on
    /// the first match (an `.iter().find(...)` yielding `0`/`1` sugar
    /// on the count arm), or (b) de-duplicated by kind (an erroneous
    /// `HashSet::insert`-gated walk that swallowed repeats) surfaces
    /// HERE at the cardinality boundary rather than silently at a
    /// downstream count-based coherence check.
    #[test]
    fn condition_slice_count_kind_counts_every_match_on_duplicates() {
        let slice = [
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "first" }),
            },
            Condition {
                kind: ConditionKind::PromQL,
                params: json!({ "query": "up" }),
            },
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "second" }),
            },
        ];
        assert_eq!(slice.count_kind(ConditionKind::ClosedLoopAuth), 2);
        assert_eq!(slice.count_kind(ConditionKind::PromQL), 1);
        for kind in ConditionKind::ALL {
            if matches!(kind, ConditionKind::ClosedLoopAuth | ConditionKind::PromQL) {
                continue;
            }
            assert_eq!(
                slice.count_kind(kind),
                0,
                "non-populated kind {kind:?} must count 0",
            );
        }
    }

    /// SLICE-LEVEL DELEGATION pin (count ↔ iter) — the trait's
    /// default `count_kind` body equals `iter_kind(k).count()` at
    /// EVERY (populated arrangement, query) pair on
    /// `ConditionKind::ALL`. Turns the trait doc's composition-law
    /// note (`count_kind(k) == iter_kind(k).count()` by construction)
    /// into a first-class typed invariant: a future implementor that
    /// overrode the default `count_kind` body with a divergent walk
    /// shape (a stored-length cache that drifted, a `.step_by(2)`
    /// artefact from a copy-paste of `iter_kind`) surfaces HERE.
    #[test]
    fn condition_slice_count_kind_equals_iter_kind_count() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let slice = [condition_with(pre_kind), condition_with(post_kind)];
                for query in ConditionKind::ALL {
                    assert_eq!(
                        slice.count_kind(query),
                        slice.iter_kind(query).count(),
                        "count/iter bridge drifted: pre={pre_kind:?} \
                         post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// SLICE-LEVEL DELEGATION pin (count ↔ has ↔ find) — the two
    /// composition laws
    /// `has_kind(k) == (count_kind(k) > 0)` and
    /// `find_kind(k).is_some() == (count_kind(k) > 0)`
    /// hold at every (populated, populated, query) triple on
    /// `ConditionKind::ALL`. Sweeps both refinement bridges at ONE
    /// site so a regression at the count primitive that drifted from
    /// the presence bit or the first-match probe surfaces HERE.
    #[test]
    fn condition_slice_has_and_find_equal_count_greater_than_zero() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let slice = [condition_with(pre_kind), condition_with(post_kind)];
                for query in ConditionKind::ALL {
                    let count = slice.count_kind(query);
                    assert_eq!(
                        slice.has_kind(query),
                        count > 0,
                        "has/count bridge drifted: pre={pre_kind:?} \
                         post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        slice.find_kind(query).is_some(),
                        count > 0,
                        "find/count bridge drifted: pre={pre_kind:?} \
                         post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (Boundary count-triad) — the three
    /// widened `count_*_kind` methods on [`Boundary`] delegate
    /// verbatim to [`ConditionSliceExt::count_kind`] on the
    /// underlying [`Vec<Condition>`] slices. The
    /// `count_condition_kind` union SUMS preconditions and
    /// postconditions (distinct from the `iter_condition_kind`
    /// [`Chain`](std::iter::Chain), `find_condition_kind`
    /// [`Option::or_else`], and `has_condition_kind` `||`
    /// compositions on the same axis). Sweep `ConditionKind::ALL ×
    /// ConditionKind::ALL × ConditionKind::ALL` so a regression that
    /// (a) inlined a divergent count at either half-slice arm, (b)
    /// subtracted rather than summed, or (c) collapsed the sum to
    /// [`std::cmp::max`] (silently narrowing the union to a max-per-
    /// side probe) surfaces HERE at the substrate boundary.
    #[test]
    fn boundary_count_condition_kind_triad_delegates_and_sums_slice_count_kind() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));
                for query in ConditionKind::ALL {
                    let via_pre = b.preconditions.count_kind(query);
                    let via_post = b.postconditions.count_kind(query);
                    assert_eq!(
                        b.count_precondition_kind(query),
                        via_pre,
                        "boundary precondition count arm must delegate: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        b.count_postcondition_kind(query),
                        via_post,
                        "boundary postcondition count arm must delegate: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        b.count_condition_kind(query),
                        via_pre + via_post,
                        "boundary union count arm must SUM pre + post: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    /// STRUCT-LEVEL DELEGATION pin (count ↔ iter on Boundary) — the
    /// three [`Boundary`] `count_*_kind` arms equal their widened
    /// peers' `.count()` projection at EVERY (pre-populated, post-
    /// populated, query) triple on `ConditionKind::ALL`. Re-anchors
    /// the composition-law pin
    /// `count_condition_kind == iter_condition_kind.count()` through
    /// the cardinality axis on the parent surface — a future consumer
    /// that reads `count_condition_kind(k)` as sugar for
    /// `iter_condition_kind(k).count()` stays typed against the SAME
    /// truth table on both the slice-level and struct-level layers.
    /// Also pins the sum-composition round-trip through the widened
    /// stream: the union arm's SUM equals the chained stream's count.
    #[test]
    fn boundary_count_triad_equals_iter_triad_count_projection() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));
                for query in ConditionKind::ALL {
                    assert_eq!(
                        b.count_precondition_kind(query),
                        b.iter_precondition_kind(query).count(),
                        "precondition count/iter bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        b.count_postcondition_kind(query),
                        b.iter_postcondition_kind(query).count(),
                        "postcondition count/iter bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                    assert_eq!(
                        b.count_condition_kind(query),
                        b.iter_condition_kind(query).count(),
                        "union count/iter bridge drifted: \
                         pre={pre_kind:?} post={post_kind:?} query={query:?}",
                    );
                }
            }
        }
    }

    // ── ConditionSliceExt::distinct_kinds — closed-set-inversion axis ──
    //
    // The fifth refinement on the slice-level presence-probe algebra
    // inverts the axis: the four point-probe refinements (has, find,
    // iter, count) fix a [`ConditionKind`] and vary the return type;
    // `distinct_kinds` fixes the slice and varies over
    // [`ConditionKind::ALL`], returning the SET of present kinds
    // projected in [`ConditionKind::ALL`] order with no duplicates.
    // The composition-law arms in `assert_slice_refinement_composition_laws`
    // pin the fifth refinement against `has_kind` per variant AND
    // against the canonical ALL-order equality; the four dedicated
    // behavior tests below pin the returned VALUE per authored
    // arrangement (empty, single-element populated, dual-populated,
    // duplicate-populated).

    /// EMPTY-SLICE pin — an empty slice returns an empty `Vec` on
    /// `distinct_kinds`, distinct from every populated arrangement.
    /// Locks the zero-element identity so a regression that (a)
    /// returned `ConditionKind::ALL.to_vec()` (the wrong direction of
    /// the closed-set walk), (b) returned a placeholder `[ProcessPhase]`
    /// vec (a copy-paste of the first-variant default in a `impl
    /// Default` for a hypothetical `KindSet` wrapper) surfaces HERE.
    #[test]
    fn condition_slice_distinct_kinds_returns_empty_vec_on_empty_slice() {
        let empty: &[Condition] = &[];
        assert_eq!(
            empty.distinct_kinds(),
            Vec::<ConditionKind>::new(),
            "empty slice must return empty distinct-kinds vec",
        );
    }

    /// PER-VARIANT pin — a slice with EXACTLY ONE `Condition` carrying
    /// the addressed kind returns `[kind]` — a single-element vec
    /// containing exactly that kind. Sweep `ConditionKind::ALL` so a
    /// new variant added without a matching arm in the closed-set walk
    /// surfaces at rustc's exhaustiveness gate on the ALL literal
    /// (arity forced by `[Self; 8]`) rather than as a silent false-
    /// negative at every downstream `distinct_condition_kinds`
    /// callsite. Locks the closed-set-inversion probe body against a
    /// regression that (a) always returned `[ProcessPhase]` regardless
    /// of the actual kind, (b) collapsed `distinct_kinds` to
    /// `iter_kind(<first ALL variant>).map(|c| c.kind).collect()`
    /// (silently filtering to only ProcessPhase matches).
    #[test]
    fn condition_slice_distinct_kinds_returns_single_element_vec_per_variant() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            assert_eq!(
                slice.distinct_kinds(),
                vec![populated],
                "single-populated slice must return exactly [{populated:?}] on distinct_kinds",
            );
        }
    }

    /// DEDUP pin — a slice with the SAME kind at multiple positions
    /// (three interleaved with distinct kinds) returns a distinct-set
    /// containing that kind exactly ONCE. The closed-set-inversion
    /// projection collapses multiplicity — a caller that needs the
    /// per-kind cardinality reaches for `count_kind`; this refinement
    /// returns the PRESENCE set. A regression that (a) omitted the
    /// dedup and returned `[ClosedLoopAuth, PromQL, ClosedLoopAuth,
    /// PromQL, ClosedLoopAuth]` (byte-identical to
    /// `slice.iter().map(|c| c.kind).collect()` — the wrong closed-
    /// set walk direction), (b) counted every duplicate as a distinct
    /// entry via a `.collect::<HashSet<_>>()` without canonicalizing
    /// order surfaces HERE.
    #[test]
    fn condition_slice_distinct_kinds_deduplicates_and_yields_canonical_all_order() {
        let interleaved = [
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "first" }),
            },
            Condition {
                kind: ConditionKind::PromQL,
                params: json!({ "query": "up" }),
            },
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "second" }),
            },
            Condition {
                kind: ConditionKind::PromQL,
                params: json!({ "query": "healthy" }),
            },
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "third" }),
            },
        ];
        // Canonical ConditionKind::ALL order: PromQL is at position 3,
        // ClosedLoopAuth at position 7 in the ALL array. So PromQL comes
        // FIRST in the distinct-set even though ClosedLoopAuth appears
        // FIRST in the slice — the closed-set-inversion walk is
        // ordered by ConditionKind::ALL, not by slice-encounter order.
        assert_eq!(
            interleaved.distinct_kinds(),
            vec![ConditionKind::PromQL, ConditionKind::ClosedLoopAuth],
            "interleaved-duplicate slice must dedup AND order by ConditionKind::ALL, not by slice-encounter order",
        );
    }

    /// FULL-COVERAGE pin — a slice that carries every [`ConditionKind`]
    /// variant returns `ConditionKind::ALL.to_vec()` on `distinct_kinds`.
    /// The closed-set-inversion probe covers the full closed set at ONE
    /// call site — a regression that missed one variant in the walk
    /// (skipping the FIRST or LAST `ALL` entry via a `[1..]` or
    /// `[..ALL.len() - 1]` slice bug in the closed-set walk) surfaces
    /// HERE.
    #[test]
    fn condition_slice_distinct_kinds_covers_full_closed_set_on_saturated_slice() {
        let saturated: Vec<Condition> =
            ConditionKind::ALL.into_iter().map(condition_with).collect();
        assert_eq!(
            saturated.as_slice().distinct_kinds(),
            ConditionKind::ALL.to_vec(),
            "slice containing every ConditionKind must return ConditionKind::ALL as its distinct-set",
        );
    }

    // ── distinct_kind_count — slice-level scalar-cardinality pins ──────
    //
    // The trait-level scalar-cardinality projection of the closed-set-
    // inversion widened primitive: `distinct_kind_count()` collapses
    // `distinct_kinds()` to its cardinality without materializing the
    // intermediate `Vec<ConditionKind>`. Composition law
    // `distinct_kind_count() == distinct_kinds().len()` pinned as the
    // sixth arm of the substrate testkit primitive
    // [`assert_slice_refinement_composition_laws`].

    /// ZERO-ELEMENT pin — an empty slice returns `0` on
    /// `distinct_kind_count`, byte-for-byte with `distinct_kinds().len()`
    /// on the same slice. Locks the zero-element identity so a
    /// regression that (a) returned `ConditionKind::ALL.len()` (the
    /// wrong direction of the closed-set walk — every kind counted
    /// regardless of presence), (b) returned a placeholder `1` (a
    /// copy-paste of a single-slot factory's cardinality), or (c) drifted
    /// off `distinct_kinds().len()` surfaces HERE.
    #[test]
    fn condition_slice_distinct_kind_count_returns_zero_on_empty_slice() {
        let empty: &[Condition] = &[];
        assert_eq!(
            empty.distinct_kind_count(),
            0,
            "empty slice must return 0 on distinct_kind_count",
        );
        assert_eq!(
            empty.distinct_kind_count(),
            empty.distinct_kinds().len(),
            "empty slice distinct_kind_count must equal distinct_kinds().len()",
        );
    }

    /// PER-VARIANT pin — a slice with EXACTLY ONE `Condition` carrying
    /// the addressed kind returns `1` on `distinct_kind_count` — the
    /// single-slot diagonal cardinality. Sweep [`ConditionKind::ALL`]
    /// so a regression that (a) always returned `0` regardless of the
    /// actual kind, (b) always returned `ConditionKind::ALL.len()`
    /// (missed the `filter` step), or (c) collapsed the walk to a
    /// single fixed variant surfaces HERE.
    #[test]
    fn condition_slice_distinct_kind_count_returns_one_per_variant() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            assert_eq!(
                slice.distinct_kind_count(),
                1,
                "single-populated slice must return 1 on distinct_kind_count for {populated:?}",
            );
            assert_eq!(
                slice.distinct_kind_count(),
                slice.distinct_kinds().len(),
                "single-populated distinct_kind_count must equal distinct_kinds().len() for {populated:?}",
            );
        }
    }

    /// DEDUP pin — a slice with the SAME kind at multiple positions
    /// (three interleaved with distinct kinds — two `PromQL`, three
    /// `ClosedLoopAuth`) returns `2` on `distinct_kind_count` (the
    /// scalar cardinality of the DISTINCT presence set, byte-for-byte
    /// with `distinct_kinds().len()` on the same slice). Locks the
    /// closed-set projection against a regression that (a) counted
    /// every occurrence (returning `5` — byte-identical to
    /// `slice.len()`), (b) omitted the dedup and returned `5` via
    /// `.iter().map(|c| c.kind).count()`.
    #[test]
    fn condition_slice_distinct_kind_count_dedups_across_duplicates() {
        let interleaved = [
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "first" }),
            },
            Condition {
                kind: ConditionKind::PromQL,
                params: json!({ "query": "up" }),
            },
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "second" }),
            },
            Condition {
                kind: ConditionKind::PromQL,
                params: json!({ "query": "healthy" }),
            },
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "third" }),
            },
        ];
        assert_eq!(
            interleaved.distinct_kind_count(),
            2,
            "interleaved-duplicate slice must return 2 on distinct_kind_count (PromQL + ClosedLoopAuth)",
        );
        assert_eq!(
            interleaved.distinct_kind_count(),
            interleaved.distinct_kinds().len(),
            "interleaved-duplicate distinct_kind_count must equal distinct_kinds().len()",
        );
    }

    /// FULL-COVERAGE pin — a slice that carries every [`ConditionKind`]
    /// variant returns `ConditionKind::ALL.len()` on `distinct_kind_count`.
    /// The scalar cardinality projection covers the full closed set at
    /// ONE call site — a regression that missed one variant in the walk
    /// (skipping the FIRST or LAST `ALL` entry via a `[1..]` or
    /// `[..ALL.len() - 1]` slice bug in the closed-set walk) surfaces
    /// HERE.
    #[test]
    fn condition_slice_distinct_kind_count_covers_full_closed_set_on_saturated_slice() {
        let saturated: Vec<Condition> =
            ConditionKind::ALL.into_iter().map(condition_with).collect();
        assert_eq!(
            saturated.as_slice().distinct_kind_count(),
            ConditionKind::ALL.len(),
            "slice containing every ConditionKind must return ConditionKind::ALL.len() on distinct_kind_count",
        );
        assert_eq!(
            saturated.as_slice().distinct_kind_count(),
            saturated.as_slice().distinct_kinds().len(),
            "saturated distinct_kind_count must equal distinct_kinds().len()",
        );
    }

    // ── ConditionSliceExt::missing_kinds — closed-set-complement axis ──
    //
    // The complement peer of `distinct_kinds` on the closed-set-
    // inversion axis: `missing_kinds` returns the SET of kinds that
    // do NOT appear in the slice, in canonical [`ConditionKind::ALL`]
    // order. The four tests below pin each authored arrangement's
    // returned VALUE (empty, single-populated, saturated, interleaved-
    // duplicate); the composition-law arms in
    // `assert_slice_refinement_composition_laws` pin the closed-set-
    // partition invariants against `distinct_kinds` and `has_kind`.

    /// EMPTY-SLICE pin — an empty slice returns
    /// `ConditionKind::ALL.to_vec()` on `missing_kinds` (every kind is
    /// missing). Locks the maximum-cardinality identity on the
    /// complement side, byte-for-byte dual to the empty-slice arm of
    /// `distinct_kinds` (which returns an empty vec). A regression that
    /// returned an empty vec (forgot the negation) or a placeholder
    /// `[ProcessPhase]` (a copy-paste of the first-variant default)
    /// surfaces HERE.
    #[test]
    fn condition_slice_missing_kinds_returns_full_closed_set_on_empty_slice() {
        let empty: &[Condition] = &[];
        assert_eq!(
            empty.missing_kinds(),
            ConditionKind::ALL.to_vec(),
            "empty slice must return ConditionKind::ALL on missing_kinds (every kind is missing)",
        );
    }

    /// PER-VARIANT pin — a slice with EXACTLY ONE `Condition` carrying
    /// the addressed kind returns `ConditionKind::ALL` MINUS that kind
    /// on `missing_kinds`. Sweep [`ConditionKind::ALL`] so a regression
    /// that (a) returned an empty vec regardless of the kind, (b)
    /// returned the full ALL vec (forgot to filter), or (c) inverted
    /// the negation and returned only the addressed kind surfaces HERE.
    #[test]
    fn condition_slice_missing_kinds_returns_all_minus_populated_kind() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            let expected: Vec<_> = ConditionKind::ALL
                .into_iter()
                .filter(|k| *k != populated)
                .collect();
            assert_eq!(
                slice.missing_kinds(),
                expected,
                "single-populated slice must return ConditionKind::ALL minus {populated:?} on missing_kinds",
            );
        }
    }

    /// FULL-COVERAGE pin — a slice that carries every [`ConditionKind`]
    /// variant returns an empty vec on `missing_kinds` (no kind is
    /// missing). Dual of the empty-slice arm above; a regression that
    /// returned the full ALL vec regardless of population or inverted
    /// the presence direction surfaces HERE.
    #[test]
    fn condition_slice_missing_kinds_returns_empty_vec_on_saturated_slice() {
        let saturated: Vec<Condition> =
            ConditionKind::ALL.into_iter().map(condition_with).collect();
        assert_eq!(
            saturated.as_slice().missing_kinds(),
            Vec::<ConditionKind>::new(),
            "slice containing every ConditionKind must return empty vec on missing_kinds",
        );
    }

    /// DEDUP pin — a slice with the SAME kind at multiple positions
    /// (three ClosedLoopAuth, two PromQL, none of the other six)
    /// returns those SIX absent kinds on `missing_kinds`, in canonical
    /// [`ConditionKind::ALL`] order — multiplicity on the present side
    /// is irrelevant to the complement. A regression that (a) counted
    /// duplicates as decreasing the missing set (a `saturating_sub`
    /// bug in a cardinality-tracking override), (b) yielded the
    /// missing set in slice-encounter order (which is undefined when
    /// no positions carry the missing kind — a subtle failure mode
    /// that must yield the ALL-ordered subsequence regardless)
    /// surfaces HERE.
    #[test]
    fn condition_slice_missing_kinds_yields_canonical_all_order_on_duplicates() {
        let interleaved = [
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "first" }),
            },
            Condition {
                kind: ConditionKind::PromQL,
                params: json!({ "query": "up" }),
            },
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "second" }),
            },
            Condition {
                kind: ConditionKind::PromQL,
                params: json!({ "query": "healthy" }),
            },
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "third" }),
            },
        ];
        let expected: Vec<_> = ConditionKind::ALL
            .into_iter()
            .filter(|k| *k != ConditionKind::PromQL && *k != ConditionKind::ClosedLoopAuth)
            .collect();
        assert_eq!(
            interleaved.missing_kinds(),
            expected,
            "interleaved-duplicate slice must return canonical ALL-ordered complement of {{PromQL, ClosedLoopAuth}}",
        );
    }

    // ── ConditionSliceExt::missing_kind_count — scalar cardinality pins ─
    //
    // Scalar-cardinality peer of the closed-set-complement widened
    // primitive `missing_kinds`: `missing_kind_count()` collapses the
    // set to its cardinality without allocating. The composition law
    // `missing_kind_count() == missing_kinds().len()` is pinned as the
    // scalar-cardinality-complement arm of
    // `assert_slice_refinement_composition_laws`. The three tests below
    // pin each authored arrangement's returned VALUE (empty, single-
    // populated, saturated) directly against `missing_kinds().len()`.

    /// EMPTY-SLICE pin — an empty slice returns
    /// `ConditionKind::ALL.len()` on `missing_kind_count`, byte-for-byte
    /// with `missing_kinds().len()`. Locks the maximum-cardinality
    /// identity on the complement side; dual of the empty-slice arm on
    /// `distinct_kind_count` which returns `0`. A regression that
    /// forgot the negation, returned `0` (the distinct-kind-count
    /// identity on empty), or returned the wrong constant surfaces
    /// HERE.
    #[test]
    fn condition_slice_missing_kind_count_returns_full_closed_set_on_empty_slice() {
        let empty: &[Condition] = &[];
        assert_eq!(
            empty.missing_kind_count(),
            ConditionKind::ALL.len(),
            "empty slice must return ConditionKind::ALL.len() on missing_kind_count",
        );
        assert_eq!(
            empty.missing_kind_count(),
            empty.missing_kinds().len(),
            "empty slice missing_kind_count must equal missing_kinds().len()",
        );
    }

    /// PER-VARIANT pin — a slice with EXACTLY ONE `Condition` carrying
    /// the addressed kind returns `ConditionKind::ALL.len() - 1` on
    /// `missing_kind_count` (every OTHER kind is missing). Sweep
    /// [`ConditionKind::ALL`] so a regression that returned `0` (forgot
    /// to negate), `ConditionKind::ALL.len()` (forgot the populated
    /// kind), or a per-kind constant surfaces HERE.
    #[test]
    fn condition_slice_missing_kind_count_returns_all_minus_one_per_variant() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            assert_eq!(
                slice.missing_kind_count(),
                ConditionKind::ALL.len() - 1,
                "single-populated slice must return ConditionKind::ALL.len() - 1 on missing_kind_count for {populated:?}",
            );
            assert_eq!(
                slice.missing_kind_count(),
                slice.missing_kinds().len(),
                "single-populated missing_kind_count must equal missing_kinds().len() for {populated:?}",
            );
        }
    }

    /// FULL-COVERAGE pin — a slice that carries every [`ConditionKind`]
    /// variant returns `0` on `missing_kind_count` (no kind is missing).
    /// Dual of the empty-slice arm above; a regression that returned
    /// `ConditionKind::ALL.len()` regardless of population or inverted
    /// the presence direction surfaces HERE.
    #[test]
    fn condition_slice_missing_kind_count_returns_zero_on_saturated_slice() {
        let saturated: Vec<Condition> =
            ConditionKind::ALL.into_iter().map(condition_with).collect();
        assert_eq!(
            saturated.as_slice().missing_kind_count(),
            0,
            "slice containing every ConditionKind must return 0 on missing_kind_count",
        );
        assert_eq!(
            saturated.as_slice().missing_kind_count(),
            saturated.as_slice().missing_kinds().len(),
            "saturated missing_kind_count must equal missing_kinds().len()",
        );
    }

    // ── ConditionSliceExt::is_kind_saturated — Boolean saturation pins ─
    //
    // Short-circuiting Boolean saturation-endpoint peer of the closed-set-
    // complement widened + scalar primitives: `is_kind_saturated()`
    // returns `true` iff every ConditionKind::ALL variant appears at
    // least once in the slice, WITHOUT allocating `missing_kinds` or
    // walking every entry to build `missing_kind_count`. The composition
    // laws `is_kind_saturated() == (missing_kind_count() == 0)` and
    // `is_kind_saturated() == missing_kinds().is_empty()` are pinned as
    // the saturation-endpoint arm of
    // `assert_slice_refinement_composition_laws`. Byte-for-byte peer of
    // `crate::tagged_union::TaggedUnion::is_saturated` one struct-layer
    // up under the SAME `<CLOSED_SET>::ALL.iter().all(has)` short-
    // circuit walk shape.

    /// EMPTY-SLICE pin — an empty slice returns `false` on
    /// `is_kind_saturated` (every kind is missing).
    #[test]
    fn condition_slice_is_kind_saturated_returns_false_on_empty_slice() {
        let empty: &[Condition] = &[];
        assert!(
            !empty.is_kind_saturated(),
            "empty slice must return false on is_kind_saturated",
        );
        assert_eq!(
            empty.is_kind_saturated(),
            empty.missing_kind_count() == 0,
            "empty is_kind_saturated must equal (missing_kind_count() == 0)",
        );
    }

    /// SINGLE-KIND pin — a slice populating exactly one variant returns
    /// `false` on any [`ConditionKind::ALL`] closed set with `N ≥ 2`
    /// (the other `N - 1` variants are missing).
    #[test]
    fn condition_slice_is_kind_saturated_returns_false_on_single_kind_slice() {
        assert!(
            ConditionKind::ALL.len() >= 2,
            "test assumes ConditionKind::ALL has ≥ 2 variants",
        );
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            assert!(
                !slice.is_kind_saturated(),
                "single-populated slice with {populated:?} must return false on is_kind_saturated",
            );
            assert_eq!(
                slice.is_kind_saturated(),
                slice.missing_kind_count() == 0,
                "single-populated is_kind_saturated must equal (missing_kind_count() == 0) for {populated:?}",
            );
        }
    }

    /// FULL-COVERAGE pin — a slice that carries every [`ConditionKind`]
    /// variant returns `true` on `is_kind_saturated` — the SOLE arm
    /// where the primitive returns `true`.
    #[test]
    fn condition_slice_is_kind_saturated_returns_true_on_saturated_slice() {
        let saturated: Vec<Condition> =
            ConditionKind::ALL.into_iter().map(condition_with).collect();
        assert!(
            saturated.as_slice().is_kind_saturated(),
            "slice containing every ConditionKind must return true on is_kind_saturated",
        );
        assert_eq!(
            saturated.as_slice().is_kind_saturated(),
            saturated.as_slice().missing_kind_count() == 0,
            "saturated is_kind_saturated must equal (missing_kind_count() == 0)",
        );
        assert_eq!(
            saturated.as_slice().is_kind_saturated(),
            saturated.as_slice().missing_kinds().is_empty(),
            "saturated is_kind_saturated must equal missing_kinds().is_empty()",
        );
    }

    /// DUPLICATE-COVERAGE pin — a slice that carries every
    /// [`ConditionKind`] variant multiple times still returns `true`
    /// (multiplicity is irrelevant to the saturation predicate on the
    /// closed-set-inversion axis).
    #[test]
    fn condition_slice_is_kind_saturated_ignores_multiplicity() {
        let mut doubled: Vec<Condition> = Vec::new();
        for k in ConditionKind::ALL {
            doubled.push(condition_with(k));
            doubled.push(condition_with(k));
        }
        assert!(
            doubled.as_slice().is_kind_saturated(),
            "slice carrying every ConditionKind twice must return true on is_kind_saturated",
        );
    }

    // ── ConditionSliceExt::lacks_kind — per-kind complement pins ──────
    //
    // Boolean per-kind closed-set-complement peer of `has_kind`:
    // `lacks_kind(k)` returns `true` iff NO Condition in the slice
    // carries the addressed kind, byte-for-byte with `!has_kind(k)`
    // via the definitional negation in the trait's default body.
    // The composition laws `lacks_kind(k) == !has_kind(k)` and
    // `lacks_kind(k) == missing_kinds().contains(&k)` are pinned as
    // the per-kind-complement arm of
    // `assert_slice_refinement_composition_laws`. Byte-for-byte peer
    // of `crate::tagged_union::TaggedUnion::lacks` one struct-layer up
    // under the SAME `!has(kind)` definitional negation shape.

    /// EMPTY-SLICE pin — an empty slice returns `true` for every
    /// [`ConditionKind`] on `lacks_kind` (no kind appears, so every
    /// kind is lacked). Dual of the empty-slice arm on `has_kind`
    /// (which returns `false` for every kind). Sweeps
    /// [`ConditionKind::ALL`] so a regression that dropped the
    /// negation, returned `false` (the has-kind identity on empty),
    /// or drifted to a per-kind constant surfaces HERE.
    #[test]
    fn condition_slice_lacks_kind_returns_true_on_empty_slice_for_every_kind() {
        let empty: &[Condition] = &[];
        for kind in ConditionKind::ALL {
            assert!(
                empty.lacks_kind(kind),
                "empty slice must return true on lacks_kind for {kind:?}",
            );
            assert_eq!(
                empty.lacks_kind(kind),
                !empty.has_kind(kind),
                "empty lacks_kind must equal !has_kind for {kind:?}",
            );
        }
    }

    /// SINGLE-KIND pin — a slice with EXACTLY ONE `Condition` carrying
    /// the addressed kind returns `false` on `lacks_kind` for the
    /// populated kind and `true` for every OTHER kind. Sweeps
    /// [`ConditionKind::ALL`] × [`ConditionKind::ALL`] so a regression
    /// that swapped the wrong side, drifted the negation, or drifted
    /// the walk from `has_kind` surfaces HERE. Also pins the
    /// composition law `lacks_kind(k) == !has_kind(k)` per-kind.
    #[test]
    fn condition_slice_lacks_kind_returns_true_on_every_missing_kind() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            for probe in ConditionKind::ALL {
                let expected_lacks = probe != populated;
                assert_eq!(
                    slice.as_slice().lacks_kind(probe),
                    expected_lacks,
                    "single-populated slice with {populated:?} must return {expected_lacks} on lacks_kind({probe:?})",
                );
                assert_eq!(
                    slice.as_slice().lacks_kind(probe),
                    !slice.as_slice().has_kind(probe),
                    "single-populated lacks_kind({probe:?}) must equal !has_kind({probe:?}) for populated={populated:?}",
                );
            }
        }
    }

    /// SATURATED pin — a slice carrying every [`ConditionKind`] variant
    /// returns `false` on `lacks_kind` for every arm (the SOLE
    /// arrangement where the primitive returns `false` for every kind).
    /// Dual of the SATURATED arm on `is_kind_saturated` which returns
    /// `true`. Pins the composition law `lacks_kind(k) ==
    /// missing_kinds().contains(&k)` per-kind against the empty missing
    /// set.
    #[test]
    fn condition_slice_lacks_kind_returns_false_on_saturated_slice_for_every_kind() {
        let saturated: Vec<Condition> =
            ConditionKind::ALL.into_iter().map(condition_with).collect();
        let missing = saturated.as_slice().missing_kinds();
        for kind in ConditionKind::ALL {
            assert!(
                !saturated.as_slice().lacks_kind(kind),
                "saturated slice must return false on lacks_kind for {kind:?}",
            );
            assert_eq!(
                saturated.as_slice().lacks_kind(kind),
                missing.contains(&kind),
                "saturated lacks_kind({kind:?}) must equal missing_kinds().contains(&{kind:?})",
            );
        }
    }

    /// MULTIPLICITY pin — a slice carrying the addressed kind multiple
    /// times still returns `false` on `lacks_kind` for that kind
    /// (multiplicity is irrelevant to the per-kind Boolean-complement
    /// projection on the closed-set-complement axis, byte-for-byte
    /// with `has_kind`'s multiplicity behavior).
    #[test]
    fn condition_slice_lacks_kind_ignores_multiplicity_on_the_populated_side() {
        for populated in ConditionKind::ALL {
            let slice = [
                condition_with(populated),
                condition_with(populated),
                condition_with(populated),
            ];
            assert!(
                !slice.as_slice().lacks_kind(populated),
                "duplicate-populated slice with {populated:?} must return false on lacks_kind for {populated:?}",
            );
        }
    }

    // ── ConditionSliceExt::first_distinct_kind — earliest-element pins ─
    //
    // Short-circuiting Option<ConditionKind> peer of the closed-set-
    // inversion widened primitive `distinct_kinds`: `first_distinct_kind()`
    // returns the earliest present kind in canonical ConditionKind::ALL
    // order without materializing the intermediate Vec<ConditionKind>.
    // The composition law `first_distinct_kind() == distinct_kinds()
    // .first().copied()` is pinned as the earliest-element-inversion arm
    // of `assert_slice_refinement_composition_laws`.

    /// EMPTY-SLICE pin — an empty slice returns `None` on
    /// `first_distinct_kind`, byte-for-byte with
    /// `distinct_kinds().first().copied()`.
    #[test]
    fn condition_slice_first_distinct_kind_returns_none_on_empty_slice() {
        let empty: &[Condition] = &[];
        assert_eq!(
            empty.first_distinct_kind(),
            None,
            "empty slice must return None on first_distinct_kind",
        );
        assert_eq!(
            empty.first_distinct_kind(),
            empty.distinct_kinds().first().copied(),
            "empty first_distinct_kind must equal distinct_kinds().first().copied()",
        );
    }

    /// PER-VARIANT pin — a slice with EXACTLY ONE `Condition` carrying
    /// the addressed kind returns `Some(that_kind)` on
    /// `first_distinct_kind`.
    #[test]
    fn condition_slice_first_distinct_kind_returns_populated_variant() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            assert_eq!(
                slice.first_distinct_kind(),
                Some(populated),
                "single-populated slice must return Some({populated:?}) on first_distinct_kind",
            );
            assert_eq!(
                slice.first_distinct_kind(),
                slice.distinct_kinds().first().copied(),
                "single-populated first_distinct_kind must equal distinct_kinds().first().copied() for {populated:?}",
            );
        }
    }

    /// FULL-COVERAGE pin — a slice that carries every [`ConditionKind`]
    /// variant returns `Some(ConditionKind::ALL[0])` on
    /// `first_distinct_kind` (the first ALL entry hits at the earliest
    /// walk step).
    #[test]
    fn condition_slice_first_distinct_kind_returns_first_all_on_saturated_slice() {
        let saturated: Vec<Condition> =
            ConditionKind::ALL.into_iter().map(condition_with).collect();
        assert_eq!(
            saturated.as_slice().first_distinct_kind(),
            Some(ConditionKind::ALL[0]),
            "saturated slice must return Some(ConditionKind::ALL[0]) on first_distinct_kind",
        );
        assert_eq!(
            saturated.as_slice().first_distinct_kind(),
            saturated.as_slice().distinct_kinds().first().copied(),
            "saturated first_distinct_kind must equal distinct_kinds().first().copied()",
        );
    }

    // ── ConditionSliceExt::first_missing_kind — earliest-element pins ──

    /// EMPTY-SLICE pin — an empty slice returns
    /// `Some(ConditionKind::ALL[0])` on `first_missing_kind` (every
    /// kind missing, first hit is index 0). Dual of the empty-slice arm
    /// on `first_distinct_kind` which returns `None`.
    #[test]
    fn condition_slice_first_missing_kind_returns_first_all_on_empty_slice() {
        let empty: &[Condition] = &[];
        assert_eq!(
            empty.first_missing_kind(),
            Some(ConditionKind::ALL[0]),
            "empty slice must return Some(ConditionKind::ALL[0]) on first_missing_kind",
        );
        assert_eq!(
            empty.first_missing_kind(),
            empty.missing_kinds().first().copied(),
            "empty first_missing_kind must equal missing_kinds().first().copied()",
        );
    }

    /// PER-VARIANT pin — a slice populating exactly `k` returns
    /// `Some(ALL[0])` if `k != ALL[0]`, else `Some(ALL[1])` (the earliest
    /// non-`k` entry).
    #[test]
    fn condition_slice_first_missing_kind_returns_earliest_absent_variant() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            let expected = ConditionKind::ALL.into_iter().find(|k| *k != populated);
            assert_eq!(
                slice.first_missing_kind(),
                expected,
                "single-populated slice must return earliest ALL entry != {populated:?} on first_missing_kind",
            );
            assert_eq!(
                slice.first_missing_kind(),
                slice.missing_kinds().first().copied(),
                "single-populated first_missing_kind must equal missing_kinds().first().copied() for {populated:?}",
            );
        }
    }

    /// FULL-COVERAGE pin — a slice that carries every [`ConditionKind`]
    /// variant returns `None` on `first_missing_kind` (no kind missing).
    #[test]
    fn condition_slice_first_missing_kind_returns_none_on_saturated_slice() {
        let saturated: Vec<Condition> =
            ConditionKind::ALL.into_iter().map(condition_with).collect();
        assert_eq!(
            saturated.as_slice().first_missing_kind(),
            None,
            "saturated slice must return None on first_missing_kind",
        );
        assert_eq!(
            saturated.as_slice().first_missing_kind(),
            saturated.as_slice().missing_kinds().first().copied(),
            "saturated first_missing_kind must equal missing_kinds().first().copied()",
        );
    }

    // ── ConditionSliceExt::last_distinct_kind — latest-element pins ────
    //
    // Short-circuiting Option<ConditionKind> peer of the closed-set-
    // inversion widened primitive `distinct_kinds` on the LATEST-hit
    // side: `last_distinct_kind()` returns the latest present kind in
    // canonical ConditionKind::ALL order via a REVERSED walk with no
    // intermediate Vec<ConditionKind> allocation. The composition law
    // `last_distinct_kind() == distinct_kinds().last().copied()` is
    // pinned as the latest-element-inversion arm of
    // `assert_slice_refinement_composition_laws`.

    /// EMPTY-SLICE pin — an empty slice returns `None` on
    /// `last_distinct_kind`, byte-for-byte with
    /// `distinct_kinds().last().copied()` (both scalar endpoints agree
    /// on emptiness).
    #[test]
    fn condition_slice_last_distinct_kind_returns_none_on_empty_slice() {
        let empty: &[Condition] = &[];
        assert_eq!(
            empty.last_distinct_kind(),
            None,
            "empty slice must return None on last_distinct_kind",
        );
        assert_eq!(
            empty.last_distinct_kind(),
            empty.distinct_kinds().last().copied(),
            "empty last_distinct_kind must equal distinct_kinds().last().copied()",
        );
    }

    /// PER-VARIANT pin — a slice with EXACTLY ONE `Condition` carrying
    /// the addressed kind returns `Some(that_kind)` on
    /// `last_distinct_kind` (single hit; earliest = latest endpoint).
    #[test]
    fn condition_slice_last_distinct_kind_returns_populated_variant() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            assert_eq!(
                slice.last_distinct_kind(),
                Some(populated),
                "single-populated slice must return Some({populated:?}) on last_distinct_kind",
            );
            assert_eq!(
                slice.last_distinct_kind(),
                slice.distinct_kinds().last().copied(),
                "single-populated last_distinct_kind must equal distinct_kinds().last().copied() for {populated:?}",
            );
            // On single-populated slice both endpoint projections agree.
            assert_eq!(
                slice.last_distinct_kind(),
                slice.first_distinct_kind(),
                "single-populated last_distinct_kind must equal first_distinct_kind for {populated:?} (single hit ⇒ earliest = latest)",
            );
        }
    }

    /// FULL-COVERAGE pin — a slice that carries every [`ConditionKind`]
    /// variant returns `Some(*ConditionKind::ALL.last().unwrap())` on
    /// `last_distinct_kind` (the last ALL entry hits at the earliest
    /// walk step of the REVERSED walk).
    #[test]
    fn condition_slice_last_distinct_kind_returns_last_all_on_saturated_slice() {
        let saturated: Vec<Condition> =
            ConditionKind::ALL.into_iter().map(condition_with).collect();
        let last_all = ConditionKind::ALL.last().copied();
        assert_eq!(
            saturated.as_slice().last_distinct_kind(),
            last_all,
            "saturated slice must return Some(*ConditionKind::ALL.last().unwrap()) on last_distinct_kind",
        );
        assert_eq!(
            saturated.as_slice().last_distinct_kind(),
            saturated.as_slice().distinct_kinds().last().copied(),
            "saturated last_distinct_kind must equal distinct_kinds().last().copied()",
        );
    }

    // ── ConditionSliceExt::last_missing_kind — latest-element pins ─────

    /// EMPTY-SLICE pin — an empty slice returns
    /// `Some(*ConditionKind::ALL.last().unwrap())` on `last_missing_kind`
    /// (every kind missing, latest hit is the last ALL entry). Dual of
    /// the empty-slice arm on `last_distinct_kind` which returns `None`.
    #[test]
    fn condition_slice_last_missing_kind_returns_last_all_on_empty_slice() {
        let empty: &[Condition] = &[];
        let last_all = ConditionKind::ALL.last().copied();
        assert_eq!(
            empty.last_missing_kind(),
            last_all,
            "empty slice must return Some(*ConditionKind::ALL.last().unwrap()) on last_missing_kind",
        );
        assert_eq!(
            empty.last_missing_kind(),
            empty.missing_kinds().last().copied(),
            "empty last_missing_kind must equal missing_kinds().last().copied()",
        );
    }

    /// PER-VARIANT pin — a slice populating exactly `k` returns
    /// `Some(*ALL.last().unwrap())` if `k != ALL.last().unwrap()`, else
    /// `Some(ALL[ALL.len() - 2])` (the latest ALL entry != `k`).
    #[test]
    fn condition_slice_last_missing_kind_returns_latest_absent_variant() {
        for populated in ConditionKind::ALL {
            let slice = [condition_with(populated)];
            let expected = ConditionKind::ALL
                .into_iter()
                .rev()
                .find(|k| *k != populated);
            assert_eq!(
                slice.last_missing_kind(),
                expected,
                "single-populated slice must return latest ALL entry != {populated:?} on last_missing_kind",
            );
            assert_eq!(
                slice.last_missing_kind(),
                slice.missing_kinds().last().copied(),
                "single-populated last_missing_kind must equal missing_kinds().last().copied() for {populated:?}",
            );
        }
    }

    /// FULL-COVERAGE pin — a slice that carries every [`ConditionKind`]
    /// variant returns `None` on `last_missing_kind` (no kind missing).
    #[test]
    fn condition_slice_last_missing_kind_returns_none_on_saturated_slice() {
        let saturated: Vec<Condition> =
            ConditionKind::ALL.into_iter().map(condition_with).collect();
        assert_eq!(
            saturated.as_slice().last_missing_kind(),
            None,
            "saturated slice must return None on last_missing_kind",
        );
        assert_eq!(
            saturated.as_slice().last_missing_kind(),
            saturated.as_slice().missing_kinds().last().copied(),
            "saturated last_missing_kind must equal missing_kinds().last().copied()",
        );
    }

    // ── Boundary distinct-set triad — substrate-delegation pins ────────
    //
    // The (precondition, postcondition, condition-union) distinct-set
    // triad on [`Boundary`] delegates to the slice-level substrate
    // primitive [`ConditionSliceExt::distinct_kinds`] on each half-slice
    // and composes the union via [`Self::has_condition_kind`] over
    // [`ConditionKind::ALL`]. The dedicated tests below pin each arm's
    // delegation shape; the substrate testkit macro
    // `assert_surface_union_composition_laws` (extended in this commit
    // with the closed-set-inversion arm) pins the union composition law
    // against the two half-slice arms in canonical ALL-order.

    /// SUBSTRATE-DELEGATION pin (Boundary distinct-kind-count triad)
    /// — the three `distinct_*_kind_count` methods on [`Boundary`]
    /// delegate to the slice-level substrate primitive
    /// [`ConditionSliceExt::distinct_kind_count`] over the two
    /// `Vec<Condition>` slots (precondition + postcondition) and
    /// compose the union scalar via
    /// `ConditionKind::ALL.filter(|k| has_condition_kind(*k)).count()`.
    /// Sweep `ConditionKind::ALL × ConditionKind::ALL` so a regression
    /// that (a) inlined a divergent closed-set walk at either half-slice
    /// arm, (b) reversed the union walk order, or (c) narrowed the
    /// union to an intersection surfaces HERE. Also pins the
    /// composition law
    /// `distinct_*_kind_count() == distinct_*_kinds().len()` at each
    /// arm — a regression that overrode the scalar projection to skip a
    /// kind or double-count a slot fails HERE.
    #[test]
    fn distinct_condition_kind_count_triad_delegates_and_matches_distinct_kinds_len() {
        // Empty boundary — every arm returns 0.
        let b = Boundary::default();
        for kind in ConditionKind::ALL {
            assert_eq!(
                b.distinct_precondition_kind_count(),
                0,
                "empty boundary must return 0 on distinct_precondition_kind_count, kind={kind:?}",
            );
            assert_eq!(
                b.distinct_postcondition_kind_count(),
                0,
                "empty boundary must return 0 on distinct_postcondition_kind_count, kind={kind:?}",
            );
            assert_eq!(
                b.distinct_condition_kind_count(),
                0,
                "empty boundary must return 0 on distinct_condition_kind_count, kind={kind:?}",
            );
        }

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));

                assert_eq!(
                    b.distinct_precondition_kind_count(),
                    b.preconditions.distinct_kind_count(),
                    "Boundary::distinct_precondition_kind_count must delegate verbatim to \
                     preconditions.distinct_kind_count() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.distinct_precondition_kind_count(),
                    b.distinct_precondition_kinds().len(),
                    "Boundary::distinct_precondition_kind_count must equal \
                     distinct_precondition_kinds().len() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.distinct_postcondition_kind_count(),
                    b.postconditions.distinct_kind_count(),
                    "Boundary::distinct_postcondition_kind_count must delegate verbatim to \
                     postconditions.distinct_kind_count() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.distinct_postcondition_kind_count(),
                    b.distinct_postcondition_kinds().len(),
                    "Boundary::distinct_postcondition_kind_count must equal \
                     distinct_postcondition_kinds().len() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union_count = if pre_kind == post_kind { 1 } else { 2 };
                assert_eq!(
                    b.distinct_condition_kind_count(),
                    expected_union_count,
                    "Boundary::distinct_condition_kind_count must count distinct union kinds \
                     for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.distinct_condition_kind_count(),
                    b.distinct_condition_kinds().len(),
                    "Boundary::distinct_condition_kind_count must equal \
                     distinct_condition_kinds().len() for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (Boundary distinct-set triad) — the
    /// three `distinct_*_kinds` methods on [`Boundary`] delegate to the
    /// slice-level substrate primitive over the two `Vec<Condition>`
    /// slots (precondition + postcondition) and compose the union via
    /// `ConditionKind::ALL.filter(|k| has_condition_kind(*k))`. Sweep
    /// `ConditionKind::ALL × ConditionKind::ALL` so a regression that
    /// (a) inlined a divergent closed-set walk at either half-slice
    /// arm, (b) reversed the union walk order, or (c) narrowed the
    /// union to an intersection surfaces HERE.
    #[test]
    fn distinct_condition_kinds_triad_delegates_to_slice_distinct_kinds() {
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));

                assert_eq!(
                    b.distinct_precondition_kinds(),
                    b.preconditions.distinct_kinds(),
                    "Boundary::distinct_precondition_kinds must delegate verbatim to \
                     preconditions.distinct_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.distinct_postcondition_kinds(),
                    b.postconditions.distinct_kinds(),
                    "Boundary::distinct_postcondition_kinds must delegate verbatim to \
                     postconditions.distinct_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union: Vec<_> = ConditionKind::ALL
                    .into_iter()
                    .filter(|k| pre_kind == *k || post_kind == *k)
                    .collect();
                assert_eq!(
                    b.distinct_condition_kinds(),
                    expected_union,
                    "Boundary::distinct_condition_kinds must equal ConditionKind::ALL-ordered \
                     set-union of the two half-slice distinct-sets for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (Boundary missing-set triad) — the
    /// three `missing_*_kinds` methods on [`Boundary`] delegate to the
    /// slice-level substrate primitive
    /// [`ConditionSliceExt::missing_kinds`] over the two
    /// `Vec<Condition>` slots (precondition + postcondition) and
    /// compose the union via
    /// `ConditionKind::ALL.filter(|k| !has_condition_kind(*k))`. Sweep
    /// `ConditionKind::ALL × ConditionKind::ALL` so a regression that
    /// (a) inlined a divergent closed-set walk at either half-slice
    /// arm, (b) reversed the union walk order, (c) widened the union
    /// intersection to a union (a `||` inlined where `&&` is required
    /// on the missing side), or (d) forgot the negation surfaces HERE.
    /// Also pins the empty-boundary edge case: every arm returns
    /// `ConditionKind::ALL.to_vec()` on an empty boundary.
    #[test]
    fn missing_condition_kinds_triad_delegates_to_slice_missing_kinds() {
        // Empty boundary — every arm returns ConditionKind::ALL (nothing
        // is populated, so every kind is missing on all three slots).
        let b = Boundary::default();
        let all_kinds = ConditionKind::ALL.to_vec();
        assert_eq!(
            b.missing_precondition_kinds(),
            all_kinds,
            "empty boundary must return ConditionKind::ALL on missing_precondition_kinds",
        );
        assert_eq!(
            b.missing_postcondition_kinds(),
            all_kinds,
            "empty boundary must return ConditionKind::ALL on missing_postcondition_kinds",
        );
        assert_eq!(
            b.missing_condition_kinds(),
            all_kinds,
            "empty boundary must return ConditionKind::ALL on missing_condition_kinds",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));

                assert_eq!(
                    b.missing_precondition_kinds(),
                    b.preconditions.missing_kinds(),
                    "Boundary::missing_precondition_kinds must delegate verbatim to \
                     preconditions.missing_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.missing_postcondition_kinds(),
                    b.postconditions.missing_kinds(),
                    "Boundary::missing_postcondition_kinds must delegate verbatim to \
                     postconditions.missing_kinds() for pre={pre_kind:?} post={post_kind:?}",
                );
                // Union: a kind is missing from the union iff it is
                // missing from BOTH half-slices (SET-INTERSECTION).
                let expected_union: Vec<_> = ConditionKind::ALL
                    .into_iter()
                    .filter(|k| pre_kind != *k && post_kind != *k)
                    .collect();
                assert_eq!(
                    b.missing_condition_kinds(),
                    expected_union,
                    "Boundary::missing_condition_kinds must equal ConditionKind::ALL-ordered \
                     set-INTERSECTION of the two half-slice missing-sets for pre={pre_kind:?} post={post_kind:?}",
                );
                // Partition invariant: distinct ∪ missing == ALL, disjoint.
                let distinct = b.distinct_condition_kinds();
                let missing = b.missing_condition_kinds();
                for kind in ConditionKind::ALL {
                    assert!(
                        distinct.contains(&kind) ^ missing.contains(&kind),
                        "(distinct, missing) partition violated on {kind:?} for pre={pre_kind:?} post={post_kind:?}",
                    );
                }
                assert_eq!(
                    distinct.len() + missing.len(),
                    ConditionKind::ALL.len(),
                    "Boundary (distinct, missing) cardinality partition drift for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (Boundary missing-kind-count triad) —
    /// the three `missing_*_kind_count` methods on [`Boundary`] delegate
    /// to the slice-level substrate primitive
    /// [`ConditionSliceExt::missing_kind_count`] over the two
    /// `Vec<Condition>` slots (precondition + postcondition) and
    /// compose the union via
    /// `ConditionKind::ALL.iter().filter(|k|
    /// !self.has_condition_kind(**k)).count()`. Sweep
    /// `ConditionKind::ALL × ConditionKind::ALL` so a regression that
    /// (a) inlined a divergent negated closed-set walk at either half-
    /// slice arm, (b) dropped the negation on the union arm, or (c)
    /// drifted from the widened-primitive length surfaces HERE. Also
    /// pins the scalar-partition invariant
    /// `distinct_kind_count + missing_kind_count == ConditionKind::ALL.len()`
    /// per arrangement.
    #[test]
    fn missing_condition_kind_count_triad_delegates_to_slice_missing_kind_count() {
        // Empty boundary — every arm returns ConditionKind::ALL.len()
        // (nothing is populated, so every kind is missing on all three
        // slots).
        let b = Boundary::default();
        let total = ConditionKind::ALL.len();
        assert_eq!(
            b.missing_precondition_kind_count(),
            total,
            "empty boundary must return ConditionKind::ALL.len() on missing_precondition_kind_count",
        );
        assert_eq!(
            b.missing_postcondition_kind_count(),
            total,
            "empty boundary must return ConditionKind::ALL.len() on missing_postcondition_kind_count",
        );
        assert_eq!(
            b.missing_condition_kind_count(),
            total,
            "empty boundary must return ConditionKind::ALL.len() on missing_condition_kind_count",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));

                // Half-slice arms delegate byte-for-byte to the slice
                // substrate primitive.
                assert_eq!(
                    b.missing_precondition_kind_count(),
                    b.preconditions.missing_kind_count(),
                    "Boundary::missing_precondition_kind_count must delegate verbatim to \
                     preconditions.missing_kind_count() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.missing_postcondition_kind_count(),
                    b.postconditions.missing_kind_count(),
                    "Boundary::missing_postcondition_kind_count must delegate verbatim to \
                     postconditions.missing_kind_count() for pre={pre_kind:?} post={post_kind:?}",
                );
                // Union arm equals missing_condition_kinds().len() — the
                // scalar cardinality of the two-slice intersection.
                assert_eq!(
                    b.missing_condition_kind_count(),
                    b.missing_condition_kinds().len(),
                    "Boundary::missing_condition_kind_count must equal missing_condition_kinds().len() \
                     for pre={pre_kind:?} post={post_kind:?}",
                );
                // Scalar-partition invariant: distinct + missing == ALL.
                assert_eq!(
                    b.distinct_condition_kind_count() + b.missing_condition_kind_count(),
                    ConditionKind::ALL.len(),
                    "Boundary (distinct, missing) scalar partition drift for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (Boundary first-distinct-kind triad) —
    /// the three `first_distinct_*_kind` methods on [`Boundary`]
    /// delegate to the slice-level substrate primitive
    /// [`ConditionSliceExt::first_distinct_kind`] over the two
    /// `Vec<Condition>` slots (precondition + postcondition) and
    /// compose the union via `ConditionKind::ALL.iter().copied()
    /// .find(|k| has_condition_kind(*k))`. Sweep
    /// `ConditionKind::ALL × ConditionKind::ALL` so a regression that
    /// inlined a divergent short-circuit walk at either half-slice arm,
    /// reversed the walk order, or dropped the short-circuit surfaces
    /// HERE. Also pins the composition law `first_distinct_*_kind() ==
    /// distinct_*_kinds().first().copied()` at each arm.
    #[test]
    fn first_distinct_condition_kind_triad_delegates_to_slice_first_distinct_kind() {
        // Empty boundary — every arm returns None.
        let b = Boundary::default();
        assert_eq!(
            b.first_distinct_precondition_kind(),
            None,
            "empty boundary must return None on first_distinct_precondition_kind",
        );
        assert_eq!(
            b.first_distinct_postcondition_kind(),
            None,
            "empty boundary must return None on first_distinct_postcondition_kind",
        );
        assert_eq!(
            b.first_distinct_condition_kind(),
            None,
            "empty boundary must return None on first_distinct_condition_kind",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));

                assert_eq!(
                    b.first_distinct_precondition_kind(),
                    b.preconditions.first_distinct_kind(),
                    "Boundary::first_distinct_precondition_kind must delegate verbatim to \
                     preconditions.first_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.first_distinct_precondition_kind(),
                    b.distinct_precondition_kinds().first().copied(),
                    "Boundary::first_distinct_precondition_kind must equal \
                     distinct_precondition_kinds().first().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.first_distinct_postcondition_kind(),
                    b.postconditions.first_distinct_kind(),
                    "Boundary::first_distinct_postcondition_kind must delegate verbatim to \
                     postconditions.first_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.first_distinct_postcondition_kind(),
                    b.distinct_postcondition_kinds().first().copied(),
                    "Boundary::first_distinct_postcondition_kind must equal \
                     distinct_postcondition_kinds().first().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union = ConditionKind::ALL
                    .into_iter()
                    .find(|k| pre_kind == *k || post_kind == *k);
                assert_eq!(
                    b.first_distinct_condition_kind(),
                    expected_union,
                    "Boundary::first_distinct_condition_kind must equal earliest ALL entry \
                     populated by either half-slice for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.first_distinct_condition_kind(),
                    b.distinct_condition_kinds().first().copied(),
                    "Boundary::first_distinct_condition_kind must equal \
                     distinct_condition_kinds().first().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (Boundary first-missing-kind triad) —
    /// the three `first_missing_*_kind` methods on [`Boundary`]
    /// delegate to the slice-level substrate primitive
    /// [`ConditionSliceExt::first_missing_kind`] over the two
    /// `Vec<Condition>` slots (precondition + postcondition) and
    /// compose the union via `ConditionKind::ALL.iter().copied()
    /// .find(|k| !has_condition_kind(*k))`. Sweep
    /// `ConditionKind::ALL × ConditionKind::ALL` so a regression that
    /// dropped the negation or drifted the short-circuit walk surfaces
    /// HERE. Also pins the composition law `first_missing_*_kind() ==
    /// missing_*_kinds().first().copied()` at each arm.
    #[test]
    fn first_missing_condition_kind_triad_delegates_to_slice_first_missing_kind() {
        // Empty boundary — every arm returns Some(ConditionKind::ALL[0]).
        let b = Boundary::default();
        let first = Some(ConditionKind::ALL[0]);
        assert_eq!(
            b.first_missing_precondition_kind(),
            first,
            "empty boundary must return Some(ConditionKind::ALL[0]) on first_missing_precondition_kind",
        );
        assert_eq!(
            b.first_missing_postcondition_kind(),
            first,
            "empty boundary must return Some(ConditionKind::ALL[0]) on first_missing_postcondition_kind",
        );
        assert_eq!(
            b.first_missing_condition_kind(),
            first,
            "empty boundary must return Some(ConditionKind::ALL[0]) on first_missing_condition_kind",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));

                assert_eq!(
                    b.first_missing_precondition_kind(),
                    b.preconditions.first_missing_kind(),
                    "Boundary::first_missing_precondition_kind must delegate verbatim to \
                     preconditions.first_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.first_missing_precondition_kind(),
                    b.missing_precondition_kinds().first().copied(),
                    "Boundary::first_missing_precondition_kind must equal \
                     missing_precondition_kinds().first().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.first_missing_postcondition_kind(),
                    b.postconditions.first_missing_kind(),
                    "Boundary::first_missing_postcondition_kind must delegate verbatim to \
                     postconditions.first_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.first_missing_postcondition_kind(),
                    b.missing_postcondition_kinds().first().copied(),
                    "Boundary::first_missing_postcondition_kind must equal \
                     missing_postcondition_kinds().first().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union = ConditionKind::ALL
                    .into_iter()
                    .find(|k| pre_kind != *k && post_kind != *k);
                assert_eq!(
                    b.first_missing_condition_kind(),
                    expected_union,
                    "Boundary::first_missing_condition_kind must equal earliest ALL entry \
                     NOT populated by either half-slice for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.first_missing_condition_kind(),
                    b.missing_condition_kinds().first().copied(),
                    "Boundary::first_missing_condition_kind must equal \
                     missing_condition_kinds().first().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (Boundary last-distinct-kind triad)
    /// — the three `last_distinct_*_kind` methods on [`Boundary`]
    /// delegate to the slice-level substrate primitive
    /// [`ConditionSliceExt::last_distinct_kind`] over the two
    /// `Vec<Condition>` slots (precondition + postcondition) and
    /// compose the union via `ConditionKind::ALL.iter().rev().copied()
    /// .find(|k| has_condition_kind(*k))`. Sweep
    /// `ConditionKind::ALL × ConditionKind::ALL` so a regression that
    /// (a) forgot to reverse the walk (returning `first_distinct_*_kind`),
    /// (b) inlined a divergent closed-set walk at either half-slice
    /// arm, or (c) narrowed the union to an intersection surfaces
    /// HERE. Also pins the composition law `last_distinct_*_kind() ==
    /// distinct_*_kinds().last().copied()` at each arm.
    #[test]
    fn last_distinct_condition_kind_triad_delegates_to_slice_last_distinct_kind() {
        // Empty boundary — every arm returns None.
        let b = Boundary::default();
        assert_eq!(
            b.last_distinct_precondition_kind(),
            None,
            "empty boundary must return None on last_distinct_precondition_kind",
        );
        assert_eq!(
            b.last_distinct_postcondition_kind(),
            None,
            "empty boundary must return None on last_distinct_postcondition_kind",
        );
        assert_eq!(
            b.last_distinct_condition_kind(),
            None,
            "empty boundary must return None on last_distinct_condition_kind",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));

                assert_eq!(
                    b.last_distinct_precondition_kind(),
                    b.preconditions.last_distinct_kind(),
                    "Boundary::last_distinct_precondition_kind must delegate verbatim to \
                     preconditions.last_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.last_distinct_precondition_kind(),
                    b.distinct_precondition_kinds().last().copied(),
                    "Boundary::last_distinct_precondition_kind must equal \
                     distinct_precondition_kinds().last().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.last_distinct_postcondition_kind(),
                    b.postconditions.last_distinct_kind(),
                    "Boundary::last_distinct_postcondition_kind must delegate verbatim to \
                     postconditions.last_distinct_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.last_distinct_postcondition_kind(),
                    b.distinct_postcondition_kinds().last().copied(),
                    "Boundary::last_distinct_postcondition_kind must equal \
                     distinct_postcondition_kinds().last().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union = ConditionKind::ALL
                    .into_iter()
                    .rev()
                    .find(|k| pre_kind == *k || post_kind == *k);
                assert_eq!(
                    b.last_distinct_condition_kind(),
                    expected_union,
                    "Boundary::last_distinct_condition_kind must equal latest ALL entry \
                     populated by either half-slice for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.last_distinct_condition_kind(),
                    b.distinct_condition_kinds().last().copied(),
                    "Boundary::last_distinct_condition_kind must equal \
                     distinct_condition_kinds().last().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (Boundary last-missing-kind triad) —
    /// the three `last_missing_*_kind` methods on [`Boundary`]
    /// delegate to the slice-level substrate primitive
    /// [`ConditionSliceExt::last_missing_kind`] over the two
    /// `Vec<Condition>` slots (precondition + postcondition) and
    /// compose the union via `ConditionKind::ALL.iter().rev().copied()
    /// .find(|k| !has_condition_kind(*k))`. Sweep
    /// `ConditionKind::ALL × ConditionKind::ALL` so a regression that
    /// dropped the negation or forgot the reversed short-circuit walk
    /// surfaces HERE. Also pins the composition law `last_missing_*_kind()
    /// == missing_*_kinds().last().copied()` at each arm.
    #[test]
    fn last_missing_condition_kind_triad_delegates_to_slice_last_missing_kind() {
        // Empty boundary — every arm returns Some(*ConditionKind::ALL.last().unwrap()).
        let b = Boundary::default();
        let last = ConditionKind::ALL.last().copied();
        assert_eq!(
            b.last_missing_precondition_kind(),
            last,
            "empty boundary must return Some(*ConditionKind::ALL.last().unwrap()) on last_missing_precondition_kind",
        );
        assert_eq!(
            b.last_missing_postcondition_kind(),
            last,
            "empty boundary must return Some(*ConditionKind::ALL.last().unwrap()) on last_missing_postcondition_kind",
        );
        assert_eq!(
            b.last_missing_condition_kind(),
            last,
            "empty boundary must return Some(*ConditionKind::ALL.last().unwrap()) on last_missing_condition_kind",
        );

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));

                assert_eq!(
                    b.last_missing_precondition_kind(),
                    b.preconditions.last_missing_kind(),
                    "Boundary::last_missing_precondition_kind must delegate verbatim to \
                     preconditions.last_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.last_missing_precondition_kind(),
                    b.missing_precondition_kinds().last().copied(),
                    "Boundary::last_missing_precondition_kind must equal \
                     missing_precondition_kinds().last().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.last_missing_postcondition_kind(),
                    b.postconditions.last_missing_kind(),
                    "Boundary::last_missing_postcondition_kind must delegate verbatim to \
                     postconditions.last_missing_kind() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.last_missing_postcondition_kind(),
                    b.missing_postcondition_kinds().last().copied(),
                    "Boundary::last_missing_postcondition_kind must equal \
                     missing_postcondition_kinds().last().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union = ConditionKind::ALL
                    .into_iter()
                    .rev()
                    .find(|k| pre_kind != *k && post_kind != *k);
                assert_eq!(
                    b.last_missing_condition_kind(),
                    expected_union,
                    "Boundary::last_missing_condition_kind must equal latest ALL entry \
                     NOT populated by either half-slice for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.last_missing_condition_kind(),
                    b.missing_condition_kinds().last().copied(),
                    "Boundary::last_missing_condition_kind must equal \
                     missing_condition_kinds().last().copied() for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }
    }

    /// SUBSTRATE-DELEGATION pin (Boundary saturation-predicate triad)
    /// — the three `is_*_kind_saturated` methods on [`Boundary`]
    /// delegate to the slice-level substrate primitive
    /// [`ConditionSliceExt::is_kind_saturated`] over the two
    /// `Vec<Condition>` slots (precondition + postcondition) and
    /// compose the union via `ConditionKind::ALL.iter().all(|k|
    /// has_condition_kind(*k))`. Sweeps the empty boundary (every arm
    /// returns `false`), a single-populated-per-side arrangement (both
    /// per-slice arms return `false` on any `N ≥ 2` closed set; the
    /// union returns `false` unless the two kinds are distinct AND
    /// `N == 2`), and the saturated boundary (both slices carry every
    /// [`ConditionKind`], every arm returns `true`). Also pins the
    /// composition law `is_*_kind_saturated() ==
    /// missing_*_kinds().is_empty()` at each arm — a regression that
    /// dropped the `all` short-circuit, drifted the walk from
    /// `ConditionKind::ALL`, or negated the wrong side surfaces HERE.
    #[test]
    fn is_condition_kind_saturated_triad_delegates_to_slice_is_kind_saturated() {
        // Empty boundary — every arm returns false; missing_*_kinds
        // covers the full closed set on every arm.
        let b = Boundary::default();
        assert!(
            !b.is_precondition_kind_saturated(),
            "empty boundary must return false on is_precondition_kind_saturated",
        );
        assert!(
            !b.is_postcondition_kind_saturated(),
            "empty boundary must return false on is_postcondition_kind_saturated",
        );
        assert!(
            !b.is_condition_kind_saturated(),
            "empty boundary must return false on is_condition_kind_saturated",
        );
        assert_eq!(
            b.is_precondition_kind_saturated(),
            b.missing_precondition_kinds().is_empty(),
            "empty is_precondition_kind_saturated must equal missing_precondition_kinds().is_empty()",
        );

        // Single-populated per side — every per-slice arm returns
        // false on any N ≥ 2 closed set; the union returns false too
        // (needs every ALL kind covered).
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));
                assert_eq!(
                    b.is_precondition_kind_saturated(),
                    b.preconditions.is_kind_saturated(),
                    "Boundary::is_precondition_kind_saturated must delegate verbatim to \
                     preconditions.is_kind_saturated() for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.is_postcondition_kind_saturated(),
                    b.postconditions.is_kind_saturated(),
                    "Boundary::is_postcondition_kind_saturated must delegate verbatim to \
                     postconditions.is_kind_saturated() for pre={pre_kind:?} post={post_kind:?}",
                );
                let expected_union = ConditionKind::ALL
                    .iter()
                    .all(|k| pre_kind == *k || post_kind == *k);
                assert_eq!(
                    b.is_condition_kind_saturated(),
                    expected_union,
                    "Boundary::is_condition_kind_saturated must equal all-ALL-covered-by-either-slice \
                     for pre={pre_kind:?} post={post_kind:?}",
                );
                assert_eq!(
                    b.is_condition_kind_saturated(),
                    b.missing_condition_kinds().is_empty(),
                    "Boundary::is_condition_kind_saturated must equal missing_condition_kinds().is_empty() \
                     for pre={pre_kind:?} post={post_kind:?}",
                );
            }
        }

        // Saturated boundary — both slices carry every ConditionKind
        // at least once, every arm returns true.
        let mut b = Boundary::default();
        for k in ConditionKind::ALL {
            b.preconditions.push(condition_with(k));
            b.postconditions.push(condition_with(k));
        }
        assert!(
            b.is_precondition_kind_saturated(),
            "saturated boundary must return true on is_precondition_kind_saturated",
        );
        assert!(
            b.is_postcondition_kind_saturated(),
            "saturated boundary must return true on is_postcondition_kind_saturated",
        );
        assert!(
            b.is_condition_kind_saturated(),
            "saturated boundary must return true on is_condition_kind_saturated",
        );
    }

    /// SUBSTRATE-DELEGATION pin (Boundary per-kind-complement triad) —
    /// the three `lacks_*_condition_kind` methods on [`Boundary`]
    /// delegate to the slice-level substrate primitive
    /// [`ConditionSliceExt::lacks_kind`] over the two `Vec<Condition>`
    /// slots (precondition + postcondition) and compose the union via
    /// `!self.has_condition_kind(kind)`. Sweeps the empty boundary
    /// (every arm returns `true` for every kind), a single-populated-
    /// per-side arrangement (per-slice arms return `false` on the
    /// populated kind + `true` on every other kind; the union returns
    /// `false` iff EITHER slice populates the addressed kind), and the
    /// saturated boundary (both slices carry every [`ConditionKind`],
    /// every arm returns `false` for every kind). Also pins the
    /// composition laws `lacks_*_condition_kind(k) ==
    /// !has_*_condition_kind(k)` at each arm AND `lacks_condition_kind(k)
    /// == lacks_precondition_kind(k) && lacks_postcondition_kind(k)`
    /// (the union AND-composition dual of `has`'s OR-composition) — a
    /// regression that dropped the negation, drifted the union operator
    /// to `||`, or negated the wrong side surfaces HERE.
    #[test]
    fn lacks_condition_kind_triad_delegates_to_slice_lacks_kind() {
        // Empty boundary — every arm returns true on every kind.
        let b = Boundary::default();
        for kind in ConditionKind::ALL {
            assert!(
                b.lacks_precondition_kind(kind),
                "empty boundary must return true on lacks_precondition_kind for {kind:?}",
            );
            assert!(
                b.lacks_postcondition_kind(kind),
                "empty boundary must return true on lacks_postcondition_kind for {kind:?}",
            );
            assert!(
                b.lacks_condition_kind(kind),
                "empty boundary must return true on lacks_condition_kind for {kind:?}",
            );
            assert_eq!(
                b.lacks_condition_kind(kind),
                !b.has_condition_kind(kind),
                "empty lacks_condition_kind must equal !has_condition_kind for {kind:?}",
            );
        }

        // Single-populated per side — sweep ALL × ALL, then probe every
        // ConditionKind on the (pre, post, union) triad.
        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut b = Boundary::default();
                b.preconditions.push(condition_with(pre_kind));
                b.postconditions.push(condition_with(post_kind));
                for probe in ConditionKind::ALL {
                    assert_eq!(
                        b.lacks_precondition_kind(probe),
                        b.preconditions.lacks_kind(probe),
                        "Boundary::lacks_precondition_kind must delegate verbatim to preconditions.lacks_kind for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    assert_eq!(
                        b.lacks_postcondition_kind(probe),
                        b.postconditions.lacks_kind(probe),
                        "Boundary::lacks_postcondition_kind must delegate verbatim to postconditions.lacks_kind for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    let expected_union = pre_kind != probe && post_kind != probe;
                    assert_eq!(
                        b.lacks_condition_kind(probe),
                        expected_union,
                        "Boundary::lacks_condition_kind must equal all-ALL-absent-in-both-slices for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    assert_eq!(
                        b.lacks_condition_kind(probe),
                        !b.has_condition_kind(probe),
                        "Boundary::lacks_condition_kind must equal !has_condition_kind for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                    assert_eq!(
                        b.lacks_condition_kind(probe),
                        b.lacks_precondition_kind(probe)
                            && b.lacks_postcondition_kind(probe),
                        "Boundary::lacks_condition_kind must equal AND-of-half-slice-arms for pre={pre_kind:?} post={post_kind:?} probe={probe:?}",
                    );
                }
            }
        }

        // Saturated boundary — both slices carry every ConditionKind,
        // every arm returns false on every kind.
        let mut b = Boundary::default();
        for k in ConditionKind::ALL {
            b.preconditions.push(condition_with(k));
            b.postconditions.push(condition_with(k));
        }
        for kind in ConditionKind::ALL {
            assert!(
                !b.lacks_precondition_kind(kind),
                "saturated boundary must return false on lacks_precondition_kind for {kind:?}",
            );
            assert!(
                !b.lacks_postcondition_kind(kind),
                "saturated boundary must return false on lacks_postcondition_kind for {kind:?}",
            );
            assert!(
                !b.lacks_condition_kind(kind),
                "saturated boundary must return false on lacks_condition_kind for {kind:?}",
            );
        }
    }

    // ── assert_slice_refinement_composition_laws — substrate testkit ──
    //
    // The substrate testkit primitive
    // [`assert_slice_refinement_composition_laws`] pins the FOUR
    // composition laws that bind the [`ConditionSliceExt`] refinement
    // algebra (find ↔ iter, count ↔ iter, has ↔ find, has ↔ count) at
    // ONE call site per authored arrangement, sweeping
    // [`ConditionKind::ALL`]. The four hand-authored slice-level
    // composition-law tests above
    // (`condition_slice_find_kind_equals_iter_kind_next`,
    // `condition_slice_count_kind_equals_iter_kind_count`,
    // `condition_slice_has_kind_equals_find_kind_is_some`,
    // `condition_slice_has_and_find_equal_count_greater_than_zero`)
    // stay as first-class per-law drift-arm pins; this substrate
    // testkit is the compound-lift primitive that binds all four
    // laws through ONE typed sweep so a future FIFTH refinement's
    // composition law picks up its pin as ONE new arm inside the
    // primitive's body rather than as ONE new sibling test at every
    // downstream author-time enumeration.

    /// SUBSTRATE PANEL pin — the substrate testkit primitive
    /// [`assert_slice_refinement_composition_laws`] passes on the
    /// FOUR canonical authored arrangements the trait's downstream
    /// consumers reach for: the empty slice (every refinement returns
    /// its zero-element identity), a single-element populated slice
    /// (every refinement returns the addressed match's projection),
    /// a dual-populated slice with distinct kinds (every refinement
    /// probes the kind field per element), and a duplicate-populated
    /// slice with the same kind at multiple positions (the widened
    /// primitive `iter_kind` yields every match; `find_kind` collapses
    /// to the first; `count_kind` returns the exact cardinality;
    /// `has_kind` returns true). Sweeping the four arrangements at
    /// ONE call site pins that every composition law holds regardless
    /// of the widened primitive's yield structure.
    #[test]
    fn slice_refinement_composition_laws_hold_across_authored_arrangements() {
        let empty: &[Condition] = &[];
        assert_slice_refinement_composition_laws(empty);

        for populated in ConditionKind::ALL {
            let single = [condition_with(populated)];
            assert_slice_refinement_composition_laws(single.as_slice());
        }

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let dual = [condition_with(pre_kind), condition_with(post_kind)];
                assert_slice_refinement_composition_laws(dual.as_slice());
            }
        }

        for populated in ConditionKind::ALL {
            let duplicates = [
                condition_with(populated),
                condition_with(populated),
                condition_with(populated),
            ];
            assert_slice_refinement_composition_laws(duplicates.as_slice());
        }
    }

    /// SUBSTRATE PANEL pin (params-distinguishable duplicates) — the
    /// substrate primitive holds on a slice that carries duplicate
    /// kinds interleaved with a distinct kind, byte-for-byte peer of
    /// the standalone `condition_slice_iter_kind_yields_every_match_in_slice_order_on_duplicates`
    /// / `condition_slice_count_kind_counts_every_match_on_duplicates`
    /// arrangement. Confirms the four composition laws hold when
    /// the widened primitive's yield stream is genuinely multi-element
    /// AND the addressed kind is interleaved with a non-matching kind
    /// (the union structural case that the diagonal-and-corners sweep
    /// above doesn't reach).
    #[test]
    fn slice_refinement_composition_laws_hold_on_interleaved_duplicates() {
        let interleaved = [
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "first" }),
            },
            Condition {
                kind: ConditionKind::PromQL,
                params: json!({ "query": "up" }),
            },
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "second" }),
            },
            Condition {
                kind: ConditionKind::PromQL,
                params: json!({ "query": "healthy" }),
            },
            Condition {
                kind: ConditionKind::ClosedLoopAuth,
                params: json!({ "probeImage": "third" }),
            },
        ];
        assert_slice_refinement_composition_laws(interleaved.as_slice());
    }

    // ── assert_surface_union_composition_laws — substrate testkit ────
    //
    // The substrate testkit macro
    // [`crate::assert_surface_union_composition_laws`] pins the FOUR
    // union composition laws (has: OR, find: or_else, iter: chain,
    // count: SUM) that bind the (pre, post, union) refinement triads
    // on the [`Boundary`] surface at ONE call site per authored
    // arrangement, sweeping [`ConditionKind::ALL`]. The four hand-
    // authored point-surface composition-law tests above
    // (`boundary_has_condition_kind_composes_precondition_and_postcondition_arms`,
    // `find_condition_kind_triad_delegates_to_slice_find_kind`,
    // `iter_condition_kind_triad_delegates_to_slice_iter_kind`,
    // `boundary_count_condition_kind_triad_delegates_and_sums_slice_count_kind`)
    // stay as first-class per-law drift-arm pins; this substrate
    // testkit macro is the compound-lift primitive that binds all
    // four union composition laws through ONE typed sweep so a
    // future FIFTH union refinement picks up its composition-law
    // pin as ONE new arm inside the macro body rather than as ONE
    // new sibling test at every downstream author-time
    // enumeration on each of the two surfaces.

    /// SUBSTRATE PANEL pin — the substrate testkit macro
    /// [`crate::assert_surface_union_composition_laws`] passes on
    /// [`Boundary`] for the four canonical authored arrangements the
    /// surface's downstream consumers reach for: the empty boundary
    /// (every union arm returns its zero-element identity), a
    /// precondition-only populated boundary (every union arm equals
    /// its precondition arm, postcondition arm is empty), a
    /// postcondition-only populated boundary (mirror), and a dual-
    /// populated boundary sweeping `ALL × ALL` (both half-slice arms
    /// contribute; the union monoid operator applies). Sweeping the
    /// four arrangements at ONE call site pins every union
    /// composition law holds regardless of the arrangement's per-
    /// half fill pattern.
    #[test]
    fn boundary_surface_union_composition_laws_hold_across_authored_arrangements() {
        let empty = Boundary::default();
        crate::assert_surface_union_composition_laws!(empty);

        for populated in ConditionKind::ALL {
            let mut pre_only = Boundary::default();
            pre_only.preconditions.push(condition_with(populated));
            crate::assert_surface_union_composition_laws!(pre_only);

            let mut post_only = Boundary::default();
            post_only.postconditions.push(condition_with(populated));
            crate::assert_surface_union_composition_laws!(post_only);
        }

        for pre_kind in ConditionKind::ALL {
            for post_kind in ConditionKind::ALL {
                let mut dual = Boundary::default();
                dual.preconditions.push(condition_with(pre_kind));
                dual.postconditions.push(condition_with(post_kind));
                crate::assert_surface_union_composition_laws!(dual);
            }
        }
    }

    /// SUBSTRATE PANEL pin (params-distinguishable duplicates) — the
    /// substrate macro holds on a [`Boundary`] whose two half-slices
    /// each carry duplicates of the same kind at multiple positions,
    /// interleaved with a distinct kind. The scenario reaches every
    /// union arm at its non-degenerate composition: `has` still
    /// resolves `true` on both halves (OR is not the discriminating
    /// bit), `find` yields the FIRST-precondition-side match
    /// (`or_else` walk order), `iter` yields every match with the
    /// full pre-then-post chain order (five total matches across the
    /// two halves), `count` returns the SUM (five). A regression that
    /// (a) collapsed `find`'s `or_else` to `and_then` (silently
    /// narrowing to intersection), (b) collapsed `iter`'s `chain` to
    /// `zip` (silently truncating to `min(pre, post)`), or (c)
    /// collapsed `count`'s SUM to `max` (silently narrowing the
    /// cardinality) surfaces HERE — the four laws are pinned
    /// simultaneously and any single-arm regression fails one of
    /// the four asserts.
    #[test]
    fn boundary_surface_union_composition_laws_hold_on_interleaved_duplicates() {
        let mut b = Boundary::default();
        b.preconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({ "side": "pre-1" }),
        });
        b.preconditions.push(Condition {
            kind: ConditionKind::PromQL,
            params: json!({ "query": "up" }),
        });
        b.preconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({ "side": "pre-2" }),
        });
        b.postconditions.push(Condition {
            kind: ConditionKind::PromQL,
            params: json!({ "query": "healthy" }),
        });
        b.postconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: json!({ "side": "post-1" }),
        });
        crate::assert_surface_union_composition_laws!(b);
    }
}
