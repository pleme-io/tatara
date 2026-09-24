//! Lattice algebra over `tatara_process::classification`.
//!
//! Replaces `convergence-controller::qualities_match` and the scattered
//! compliance-baseline comparators with a single `Lattice` trait.
//!
//! Laws (proven by `proptest` in tests):
//!
//! - idempotent:   `a ⊓ a = a`, `a ⊔ a = a`
//! - commutative:  `a ⊓ b = b ⊓ a`, `a ⊔ b = b ⊔ a`
//! - associative:  `a ⊓ (b ⊓ c) = (a ⊓ b) ⊓ c`, similarly for ⊔
//! - absorption:   `a ⊓ (a ⊔ b) = a`, `a ⊔ (a ⊓ b) = a`
//! - leq agrees:   `a ≤ b ⇔ a ⊓ b = a ⇔ a ⊔ b = b`

pub mod baseline;

use tatara_process::classification::{
    CalmClassification, Classification, DataClassification, Horizon, HorizonKind,
    OptimizationDirection, SubstrateType,
};

/// Total-order `min` substrate — `min(a, b)` selected by the caller-
/// supplied `key` projection. At every pair `(a, b)`:
///
/// - `key(a) <= key(b)` selects `a`;
/// - `key(a) >  key(b)` selects `b`.
///
/// The `<=` at the tie boundary makes the primitive deterministic on
/// key ties — same-key inputs always collapse to the LEFT argument.
/// Two consequences the primitive carries by construction:
///
/// - **Meet-idempotence.** `total_min_by_key(a, a, k) = a` for every
///   `a` — `key(a) <= key(a)` is reflexive, so the left branch fires.
/// - **Meet-commutativity when the key is injective on the input
///   domain.** With a strictly injective `key`, `key(a) == key(b)`
///   implies `a == b`, so the left-preference tie-break never sees
///   distinct inputs; total-order `min` on any strict total order
///   satisfies `min(a, b) = min(b, a)`.
///
/// Substrate owner for the total-order-lattice `meet` arm on every
/// axis whose `leq` is a key projection over a strict total order.
/// [`impl Lattice for baseline::Baseline`] routes here through
/// [`baseline::Baseline::total_key`] (a strict total-order projection
/// over `(rank, all_index)` — see the type's docstring for why the
/// tie-break lives at the key rather than at this primitive); [`impl
/// Lattice for tatara_process::classification::DataClassification`]
/// routes here through
/// [`tatara_process::classification::DataClassification::sensitivity_rank`]
/// (strictly monotone over `DataClassification::ALL`, pinned by
/// `data_classification_rank_is_strictly_monotone_over_all`
/// upstream). A future total-order-projected axis lands at ONE call
/// to this primitive AND inherits the commutativity + idempotence
/// invariants it carries.
///
/// Theory anchor: THEORY.md §III (typescape — the total-order
/// lattice-axis primitive lifted to ONE substrate owner) + §II.1
/// invariant 5 (composition preserves proofs — every downstream
/// lattice-law property test that consumes `meet` inherits the
/// primitive's guarantees mechanically once the impl routes through
/// here).
pub fn total_min_by_key<T, K>(a: &T, b: &T, key: impl Fn(&T) -> K) -> T
where
    T: Clone,
    K: Ord,
{
    if key(a) <= key(b) {
        a.clone()
    } else {
        b.clone()
    }
}

/// Total-order `max` substrate — dual of [`total_min_by_key`].
/// `key(a) >= key(b)` selects `a`; `key(a) < key(b)` selects `b`.
/// Same tie-break story: left-preference on `key(a) == key(b)`
/// collapses ties to the LEFT argument, so join-idempotence holds
/// by construction and join-commutativity holds when the key is
/// injective on the input domain.
///
/// Substrate owner for the total-order-lattice `join` arm; peer of
/// [`total_min_by_key`]. Same two consumers (`Baseline`,
/// `DataClassification`) route through this on the dual axis, so a
/// future normalization at either primitive (e.g. shrink-order
/// tweak, per-fleet key weighting) lands at ONE site and every
/// downstream lattice-`join` consumer inherits the upgrade
/// mechanically.
pub fn total_max_by_key<T, K>(a: &T, b: &T, key: impl Fn(&T) -> K) -> T
where
    T: Clone,
    K: Ord,
{
    if key(a) >= key(b) {
        a.clone()
    } else {
        b.clone()
    }
}

/// The lattice trait.
pub trait Lattice: Sized + Clone + PartialEq {
    /// Greatest-lower-bound — strongest common refinement.
    fn meet(&self, other: &Self) -> Self;
    /// Least-upper-bound — weakest common relaxation.
    fn join(&self, other: &Self) -> Self;
    /// `self ≤ other` — `self` is at least as refined as `other`.
    fn leq(&self, other: &Self) -> bool {
        self.meet(other) == *self
    }
    /// Bottom element — `⊥ ≤ x` for all `x`.
    fn bottom() -> Self;
    /// Top element — `x ≤ ⊤` for all `x`.
    fn top() -> Self;
    /// `self ≥ other` — dual of [`Lattice::leq`]. Default routes through
    /// `other.leq(self)` so every impl inherits the algebraic dual for
    /// free, and a future normalization at the primary axis (e.g. an
    /// override that changed [`Lattice::leq`]'s tie-break) lands at ONE
    /// site and this dual inherits mechanically. Consumers that want to
    /// probe "is `self` at least as relaxed as `other`" (the join-side
    /// question) write `self.geq(&other)` instead of `other.leq(&self)`
    /// — the two are byte-identical, but the dual name matches the
    /// join-side reading discipline.
    fn geq(&self, other: &Self) -> bool {
        other.leq(self)
    }
    /// Is `self` the lattice's [`Lattice::bottom`] element? Default
    /// routes through `PartialEq` against [`Lattice::bottom`] — the
    /// trait's own `Sized + Clone + PartialEq` supertraits give this
    /// projection for free. A future impl whose `bottom()` is
    /// value-parameterized (e.g. a per-fleet compliance floor) still
    /// inherits the correct predicate without a hand-authored
    /// per-impl override.
    fn is_bottom(&self) -> bool {
        *self == Self::bottom()
    }
    /// Is `self` the lattice's [`Lattice::top`] element? Dual of
    /// [`Lattice::is_bottom`] on the top-endpoint of the same
    /// closed-set axis. Same routing through `PartialEq` against
    /// [`Lattice::top`].
    fn is_top(&self) -> bool {
        *self == Self::top()
    }
    /// Are `self` and `other` comparable — does the lattice's
    /// partial order relate them in either direction? Default:
    /// `self.leq(other) || other.leq(self)`. A totally-ordered
    /// lattice (e.g. [`baseline::Baseline`], `DataClassification`)
    /// returns `true` for every pair; an antichain-shaped one
    /// (e.g. `SubstrateType`) returns `true` iff the two are equal
    /// OR one endpoint is the distinguished [`Lattice::top`] that
    /// the antichain routes through.
    fn is_comparable(&self, other: &Self) -> bool {
        self.leq(other) || other.leq(self)
    }
    /// Negation of [`Lattice::is_comparable`] — the two elements sit
    /// on distinct branches of the partial order. Pre-lift consumers
    /// hand-authored `!s.leq(&t) && !t.leq(&s)` at each callsite
    /// (surfaces exactly in the SubstrateType `substrate_flat_antichain`
    /// test below), which duplicated the disjunction's algebra at each
    /// consumer AND crossed the ★★ PRIME-DIRECTIVE `≥ 2` duplication
    /// threshold once a second antichain lattice (a future
    /// PointType-axis lift, a per-fleet region-axis lift) landed;
    /// post-lift the whole `!leq && !leq` conjunction binds at ONE
    /// substrate primitive on the [`Lattice`] algebra and every
    /// downstream antichain consumer picks up the predicate through
    /// the default.
    fn is_incomparable(&self, other: &Self) -> bool {
        !self.is_comparable(other)
    }
    /// N-ary [`Lattice::meet`] fold — the strongest common refinement
    /// of every element the iterator yields. The empty iterator
    /// collapses to [`Lattice::top`] because top is the algebraic
    /// identity for meet: `x.meet(&T::top()) == x` for every `x` by
    /// the lattice's absorption law (`⊤` is the greatest element, so
    /// meeting with it never tightens further). A one-element iterator
    /// collapses to that element by identity-absorption; a two-element
    /// iterator matches [`Lattice::meet`] directly; three or more
    /// elements fold left-to-right through the associative combinator.
    ///
    /// Peer of [`Lattice::join_all`] one MEET/JOIN axis over, closing
    /// the (2-ary, N-ary) × (meet, join) grid on the algebra's
    /// combinator surface. Together the two default methods lift the
    /// FOLD arm of the algebra from the caller's hand-rolled
    /// `iter.fold(T::top(), |a, b| a.meet(&b))` composition to ONE
    /// substrate primitive — a consumer with a `Vec<Classification>`
    /// that wants "the common refinement of these ephemeral env
    /// classifications" collapses at ONE call rather than a per-site
    /// fold, and the identity-element choice (top for meet, bottom for
    /// join) binds on the trait rather than on the consumer's guess.
    ///
    /// **Identity-on-empty**: `T::meet_all(std::iter::empty()) ==
    /// T::top()`. The fold's initial value IS the algebraic identity
    /// for meet, so the empty case is a first-class arm — consumers
    /// who probe "what refinement does this (possibly-empty) set
    /// require?" get the well-typed "no constraint" answer without a
    /// per-site `if iter.next().is_none() { T::top() } else { ... }`
    /// branch.
    ///
    /// **Singleton-idempotence**: `T::meet_all([&a]) == a` by the
    /// initial-fold step `T::top().meet(&a) == a`. Every lattice's
    /// `top` obeys `⊤ ⊓ x = x`, so the singleton case bypasses the
    /// consumer's `if len == 1 { return v.clone(); }` short-circuit.
    ///
    /// **Associativity + commutativity**: inherited from [`Lattice::meet`]'s
    /// associativity and commutativity, so a caller can reorder the
    /// iterator without changing the result. Pinned via proptest on
    /// the DataClassification and CalmClassification axes below.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — every downstream N-ary meet consumer inherits
    /// the fold through the default) + THEORY.md §III (typescape — the
    /// N-ary refinement-composition arm on every classification axis
    /// binds at ONE substrate owner on the [`Lattice`] algebra).
    fn meet_all<'a, I>(iter: I) -> Self
    where
        I: IntoIterator<Item = &'a Self>,
        Self: 'a,
    {
        iter.into_iter().fold(Self::top(), |acc, x| acc.meet(x))
    }
    /// N-ary [`Lattice::join`] fold — the weakest common relaxation
    /// covering every element the iterator yields. Dual of
    /// [`Lattice::meet_all`] on the join arm: the empty iterator
    /// collapses to [`Lattice::bottom`] because bottom is the algebraic
    /// identity for join (`x.join(&T::bottom()) == x` for every `x`
    /// via the absorption law — `⊥` is the least element, so joining
    /// with it never relaxes further). A one-element iterator collapses
    /// to that element by identity-absorption; a two-element iterator
    /// matches [`Lattice::join`] directly; three or more elements fold
    /// left-to-right through the associative combinator.
    ///
    /// Together with [`Lattice::meet_all`] closes the (2-ary, N-ary) ×
    /// (meet, join) grid on the algebra's combinator surface. A
    /// consumer with `Vec<Classification>` that wants "the smallest
    /// classification covering every entity in this workload" folds
    /// through this default rather than hand-rolling `iter.fold(
    /// T::bottom(), |a, b| a.join(&b))`.
    ///
    /// **Identity-on-empty**: `T::join_all(std::iter::empty()) ==
    /// T::bottom()`. The fold's initial value IS the algebraic
    /// identity for join, so the empty case yields the well-typed "no
    /// coverage yet" answer without a per-site branch.
    ///
    /// **Singleton-idempotence**: `T::join_all([&a]) == a` by the
    /// initial-fold step `T::bottom().join(&a) == a`.
    ///
    /// **Associativity + commutativity**: inherited from [`Lattice::join`]'s
    /// laws, so a caller can reorder the iterator without changing the
    /// result. Pinned via proptest on the DataClassification and
    /// CalmClassification axes below.
    ///
    /// Theory anchor: same as [`Lattice::meet_all`] on the dual arm —
    /// THEORY.md §II.1 invariant 5 + §III.
    fn join_all<'a, I>(iter: I) -> Self
    where
        I: IntoIterator<Item = &'a Self>,
        Self: 'a,
    {
        iter.into_iter().fold(Self::bottom(), |acc, x| acc.join(x))
    }
}

// ── DataClassification — total order ────────────────────────────────────
//
// Public < Internal < Confidential < Pii < Phi < Pci. The ordering is
// sealed at one site in `tatara_process::classification` —
// `DataClassification::sensitivity_rank` — so a future variant inserted
// in the middle of the enum declaration does not silently shift this
// lattice's `leq` relation. Pre-lift the comparator was `(*self as u8)
// <= (*other as u8)`, which rode silently on declaration order; an
// insertion would have moved every later variant's lattice slot
// without any compile error or test signal. Post-lift the rank is
// declared per-variant on the typed projection, pinned by
// `data_classification_rank_is_strictly_monotone_over_all` and
// `data_classification_rank_agrees_with_partial_ord` in the source
// crate, and `data_classification_leq_uses_typed_rank` below pins
// THIS impl to the typed projection (not the silent cast).

impl Lattice for DataClassification {
    fn meet(&self, other: &Self) -> Self {
        // Route through `total_min_by_key` — the total-order `meet`
        // substrate owner that peers this axis with
        // `Lattice for baseline::Baseline`. Both impls consume ONE
        // primitive on the closed-set-driven-by-a-strict-total-
        // -order-projection shape; a future data-classification axis
        // insertion (the module docstring hypothesizes a fine-grained
        // sensitivity slot between `Confidential` and `Pii`) lands at
        // ONE `sensitivity_rank` arm addition and the delegation here
        // stays untouched. Pinned exhaustively over
        // `DataClassification::ALL^2` by
        // `data_class_meet_and_join_delegate_to_total_min_max_by_key`
        // below.
        crate::total_min_by_key(self, other, |v| v.sensitivity_rank())
    }
    fn join(&self, other: &Self) -> Self {
        // Dual of `meet` on the same `sensitivity_rank` total-order
        // projection — routes through `total_max_by_key` for the same
        // substrate-routing reason.
        crate::total_max_by_key(self, other, |v| v.sensitivity_rank())
    }
    fn leq(&self, other: &Self) -> bool {
        self.sensitivity_rank() <= other.sensitivity_rank()
    }
    fn bottom() -> Self {
        DataClassification::Public
    }
    fn top() -> Self {
        DataClassification::Pci
    }
}

// ── SubstrateType — antichain (flat lattice) ────────────────────────────
// Any two distinct substrates are incomparable; meet is top when distinct.

impl Lattice for SubstrateType {
    fn meet(&self, other: &Self) -> Self {
        if self == other {
            self.clone()
        } else {
            Self::top()
        }
    }
    fn join(&self, other: &Self) -> Self {
        if self == other {
            self.clone()
        } else {
            Self::bottom()
        }
    }
    fn leq(&self, other: &Self) -> bool {
        self == other || *other == Self::top()
    }
    fn bottom() -> Self {
        SubstrateType::Financial
    }
    // Regulatory sits at the top — it absorbs any other substrate's constraints.
    fn top() -> Self {
        SubstrateType::Regulatory
    }
}

// ── CalmClassification — boolean lattice (Monotone ≤ NonMonotone) ──────
impl Lattice for CalmClassification {
    fn meet(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Monotone, _) | (_, Self::Monotone) => Self::Monotone,
            _ => Self::NonMonotone,
        }
    }
    fn join(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::NonMonotone, _) | (_, Self::NonMonotone) => Self::NonMonotone,
            _ => Self::Monotone,
        }
    }
    fn leq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Monotone, _) | (Self::NonMonotone, Self::NonMonotone)
        )
    }
    fn bottom() -> Self {
        Self::Monotone
    }
    fn top() -> Self {
        Self::NonMonotone
    }
}

// ── Horizon — Bounded ≤ Asymptotic (strength of invariant) ──────────────
//
// A Bounded point strictly converges; Asymptotic merely trends. We treat
// Bounded as the refinement (meet), Asymptotic as the relaxation (join).

impl Lattice for Horizon {
    fn meet(&self, other: &Self) -> Self {
        match (self.kind, other.kind) {
            (HorizonKind::Bounded, _) | (_, HorizonKind::Bounded) => Self::bounded(),
            _ => self.clone(),
        }
    }
    fn join(&self, other: &Self) -> Self {
        match (self.kind, other.kind) {
            (HorizonKind::Asymptotic, _) => self.clone(),
            (_, HorizonKind::Asymptotic) => other.clone(),
            _ => Self::bounded(),
        }
    }
    fn leq(&self, other: &Self) -> bool {
        matches!(
            (self.kind, other.kind),
            (HorizonKind::Bounded, _) | (HorizonKind::Asymptotic, HorizonKind::Asymptotic)
        )
    }
    fn bottom() -> Self {
        Self::bounded()
    }
    fn top() -> Self {
        Self::asymptotic("", OptimizationDirection::Minimize, f64::MIN)
    }
}

// ── Classification — pointwise product lattice ──────────────────────────
// `a ⊓ b` meets each axis independently; same for join.
// PointType is left alone (caller is responsible — point types are semantic, not comparable).

impl Lattice for Classification {
    fn meet(&self, other: &Self) -> Self {
        Self {
            // PointType is an antichain — leave the caller's choice alone.
            point_type: self.point_type,
            substrate: self.substrate.meet(&other.substrate),
            horizon: self.horizon.meet(&other.horizon),
            calm: self.calm.meet(&other.calm),
            data_classification: self.data_classification.meet(&other.data_classification),
        }
    }
    fn join(&self, other: &Self) -> Self {
        Self {
            point_type: self.point_type,
            substrate: self.substrate.join(&other.substrate),
            horizon: self.horizon.join(&other.horizon),
            calm: self.calm.join(&other.calm),
            data_classification: self.data_classification.join(&other.data_classification),
        }
    }
    fn leq(&self, other: &Self) -> bool {
        self.substrate.leq(&other.substrate)
            && self.horizon.leq(&other.horizon)
            && self.calm.leq(&other.calm)
            && self.data_classification.leq(&other.data_classification)
    }
    fn bottom() -> Self {
        Self {
            point_type: tatara_process::classification::ConvergencePointType::Transform,
            substrate: SubstrateType::bottom(),
            horizon: Horizon::bottom(),
            calm: CalmClassification::bottom(),
            data_classification: DataClassification::bottom(),
        }
    }
    fn top() -> Self {
        Self {
            point_type: tatara_process::classification::ConvergencePointType::Transform,
            substrate: SubstrateType::top(),
            horizon: Horizon::top(),
            calm: CalmClassification::top(),
            data_classification: DataClassification::top(),
        }
    }
}

/// Convenience — does a cluster classification satisfy a workload's requirements?
///
/// Replaces `convergence_controller::cluster_quality::qualities_match`.
pub fn satisfies(cluster: &Classification, requires: &Classification) -> bool {
    // A cluster must be AT LEAST as strict as the workload's requirements on each axis —
    // i.e., the cluster's class ≤ the requirement's class (more refined).
    cluster.leq(requires)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_process::classification::ConvergencePointType;

    // ── total_min_by_key / total_max_by_key substrate ────────────────
    //
    // Bind [`total_min_by_key`] + [`total_max_by_key`] at fail-before-
    // pass-after granularity. Pre-lift the total-order-lattice `meet`
    // / `join` arm was hand-authored at each impl site (twice — once
    // in `impl Lattice for DataClassification` via `if self.leq(other)
    // { self.clone() } else { other.clone() }`, once in `impl Lattice
    // for baseline::Baseline` via `if self.total_key() <=
    // other.total_key() { *self } else { *other }`) past the ★★
    // PRIME-DIRECTIVE ≥ 2 duplication threshold, so a new total-order-
    // projected axis (a fine-grained data-classification insertion, a
    // new compliance baseline family, any future closed-set enum whose
    // `leq` is a key projection) would recur the same shape a third
    // time. Post-lift both impls delegate to ONE substrate owner AND
    // the routing seals below pin the delegation at every pair in
    // both closed sets so a regression that reverted either impl to
    // the hand-authored shape would silently pass every downstream
    // lattice-law property test (same total order, same answer) but
    // fail the substrate-routing invariant.
    //
    // Tests cover:
    //   1. Strict-inequality behavior at both directions — `min_by_key`
    //      picks the smaller-key argument, `max_by_key` picks the
    //      greater-key argument, and the primitive is symmetric under
    //      the choice.
    //   2. Deterministic left-preference tie-break — same-key inputs
    //      always collapse to the LEFT argument, so the primitive is
    //      idempotent on `total_min_by_key(a, a, k) = a` by
    //      construction and stays commutative when the key is
    //      injective (the two consumer axes both satisfy that
    //      injectivity — `Baseline::total_key` by the `all_index`
    //      tie-break, `DataClassification::sensitivity_rank` by the
    //      strict-monotonicity pin upstream).

    /// [`total_min_by_key`] selects the smaller-key argument at
    /// strict inequality, in either argument order. Fail-before-pass-
    /// after: pre-lift this test cannot compile because
    /// `total_min_by_key` is not exposed as a crate-level function —
    /// the total-order `meet` arm was hand-authored per impl site.
    /// Post-lift the two directions pin symmetry: `min(3, 7) = 3` AND
    /// `min(7, 3) = 3` on `i32`, so the primitive's `key` projection
    /// is consulted rather than any implicit self-preference on the
    /// LEFT argument. The `i32`-identity key exercises the simplest
    /// possible strict total order to isolate the primitive's
    /// selection logic from any consumer-side projection.
    #[test]
    fn total_min_by_key_selects_lesser_key_argument_at_strict_inequality() {
        assert_eq!(total_min_by_key(&3_i32, &7_i32, |v| *v), 3);
        assert_eq!(total_min_by_key(&7_i32, &3_i32, |v| *v), 3);
    }

    /// [`total_min_by_key`] collapses key ties to the LEFT argument
    /// — deterministic left-preference tie-break. Consumers with a
    /// non-injective key MUST understand this tie-break OR route
    /// through an injective key (as
    /// [`baseline::Baseline::total_key`] pairs `rank` with
    /// `all_index` precisely so the primitive stays commutative on
    /// `Baseline::ALL`). Distinct inputs sharing a key surface the
    /// asymmetry: `(left, 4)` and `(right, 4)` under `|v| v.1` both
    /// project to key 4, so `min` returns whichever argument
    /// appeared first.
    #[test]
    fn total_min_by_key_collapses_key_ties_to_the_left_argument() {
        let a = ("left", 4_u8);
        let b = ("right", 4_u8);
        assert_eq!(total_min_by_key(&a, &b, |v| v.1), a);
        assert_eq!(total_min_by_key(&b, &a, |v| v.1), b);
    }

    /// [`total_max_by_key`] selects the greater-key argument at
    /// strict inequality — dual of
    /// [`total_min_by_key_selects_lesser_key_argument_at_strict_inequality`].
    /// Same symmetry check: `max(3, 7) = 7` AND `max(7, 3) = 7` on
    /// `i32`, so the primitive's `key` projection is consulted rather
    /// than any implicit self-preference on the LEFT argument.
    #[test]
    fn total_max_by_key_selects_greater_key_argument_at_strict_inequality() {
        assert_eq!(total_max_by_key(&3_i32, &7_i32, |v| *v), 7);
        assert_eq!(total_max_by_key(&7_i32, &3_i32, |v| *v), 7);
    }

    /// [`total_max_by_key`] collapses key ties to the LEFT argument
    /// — same tie-break story as [`total_min_by_key`] on the dual
    /// axis. `max` on a strict `>=` at ties collapses left; consumers
    /// route through an injective key when downstream commutativity
    /// is required.
    #[test]
    fn total_max_by_key_collapses_key_ties_to_the_left_argument() {
        let a = ("left", 4_u8);
        let b = ("right", 4_u8);
        assert_eq!(total_max_by_key(&a, &b, |v| v.1), a);
        assert_eq!(total_max_by_key(&b, &a, |v| v.1), b);
    }

    /// SEAL TEST: [`impl Lattice for DataClassification`]'s `meet` /
    /// `join` route through [`total_min_by_key`] /
    /// [`total_max_by_key`] with `sensitivity_rank` as the key.
    /// Pinned at every pair in `DataClassification::ALL^2`, so a
    /// regression that reverted the impl to the hand-authored `if
    /// self.leq(other) { self.clone() } else { other.clone() }`
    /// shape (the pre-lift form) would silently pass every downstream
    /// lattice-law property test — the SAME total order gives the
    /// same answer — but would break the substrate-routing invariant
    /// this seal binds.
    #[test]
    fn data_class_meet_and_join_delegate_to_total_min_max_by_key() {
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert_eq!(
                    a.meet(&b),
                    total_min_by_key(&a, &b, |v: &DataClassification| v.sensitivity_rank()),
                    "DataClassification::meet at ({a:?}, {b:?}) has \
                     drifted away from the total_min_by_key substrate \
                     primitive",
                );
                assert_eq!(
                    a.join(&b),
                    total_max_by_key(&a, &b, |v: &DataClassification| v.sensitivity_rank()),
                    "DataClassification::join at ({a:?}, {b:?}) has \
                     drifted away from the total_max_by_key substrate \
                     primitive",
                );
            }
        }
    }

    #[test]
    fn data_classification_total_order() {
        assert!(DataClassification::Public.leq(&DataClassification::Internal));
        assert!(DataClassification::Internal.leq(&DataClassification::Confidential));
        assert!(DataClassification::Confidential.leq(&DataClassification::Pii));
    }

    #[test]
    fn idempotent_meet() {
        let c = Classification {
            point_type: ConvergencePointType::Gate,
            substrate: SubstrateType::Observability,
            horizon: Horizon::bounded(),
            calm: CalmClassification::Monotone,
            data_classification: DataClassification::Internal,
        };
        assert_eq!(c.meet(&c), c);
    }

    #[test]
    fn absorption() {
        let a = Classification {
            point_type: ConvergencePointType::Gate,
            substrate: SubstrateType::Observability,
            horizon: Horizon::bounded(),
            calm: CalmClassification::Monotone,
            data_classification: DataClassification::Internal,
        };
        let b = Classification {
            point_type: ConvergencePointType::Gate,
            substrate: SubstrateType::Observability,
            horizon: Horizon::bounded(),
            calm: CalmClassification::NonMonotone,
            data_classification: DataClassification::Pii,
        };
        assert_eq!(a.meet(&a.join(&b)), a);
    }

    #[test]
    fn calm_monotone_is_refinement() {
        assert!(CalmClassification::Monotone.leq(&CalmClassification::NonMonotone));
        assert!(!CalmClassification::NonMonotone.leq(&CalmClassification::Monotone));
    }

    #[test]
    fn substrate_flat_antichain() {
        let s = SubstrateType::Compute;
        let t = SubstrateType::Storage;
        // Route the pre-lift `!s.leq(&t) && !t.leq(&s)` conjunction
        // through the [`Lattice::is_incomparable`] substrate default
        // — the ONE typed predicate on the [`Lattice`] algebra the
        // trait now carries as its antichain-axis reading, seeding
        // the ★★ PRIME-DIRECTIVE `≥ 2`-consumer lift with a first
        // consumer at this callsite so a hypothetical second
        // antichain lattice (a future PointType-axis lift, a
        // per-fleet region-axis lift) picks up the predicate through
        // the default rather than recurring the disjunction verbatim.
        // Exhaustive over `SubstrateType::ALL^2` at
        // `substrate_type_is_incomparable_matches_the_antichain_shape`
        // below; this sample keeps the (Compute, Storage) documentation
        // pair intact.
        assert!(s.is_incomparable(&t));
        assert!(t.is_incomparable(&s));
        // Meet of distinct substrates climbs to top (Regulatory).
        assert_eq!(s.meet(&t), SubstrateType::Regulatory);
    }

    // ── satisfies() ────────────────────────────────────────────────────

    fn bounded_classification(data: DataClassification) -> Classification {
        Classification {
            point_type: ConvergencePointType::Gate,
            substrate: SubstrateType::Observability,
            horizon: Horizon::bounded(),
            calm: CalmClassification::Monotone,
            data_classification: data,
        }
    }

    #[test]
    fn satisfies_is_true_when_cluster_is_as_refined_as_requirement() {
        // cluster.leq(requirement) ⇔ cluster is at least as refined.
        // Public (bottom) cluster satisfies Public-or-higher requirements.
        let cluster = bounded_classification(DataClassification::Public);
        let requirement_public = bounded_classification(DataClassification::Public);
        let requirement_internal = bounded_classification(DataClassification::Internal);
        assert!(satisfies(&cluster, &requirement_public));
        assert!(satisfies(&cluster, &requirement_internal));
    }

    #[test]
    fn satisfies_is_false_when_cluster_is_less_refined_than_requirement() {
        // A Confidential cluster does NOT satisfy a Public requirement —
        // relaxing a class is a lattice "up" move, not "down".
        // (The naming is counter-intuitive; the inequality direction is
        // what the code actually enforces.)
        let cluster = bounded_classification(DataClassification::Confidential);
        let requirement = bounded_classification(DataClassification::Public);
        assert!(!satisfies(&cluster, &requirement));
    }

    #[test]
    fn satisfies_equal_always_true() {
        // x.leq(x) is reflexive — a cluster always satisfies its own
        // classification requirements.
        let c = bounded_classification(DataClassification::Pii);
        assert!(satisfies(&c, &c));
    }

    // ── DataClassification — total-order lattice laws ──────────────────

    use proptest::prelude::*;

    /// SEAL TEST: this lattice's `leq` agrees with the typed
    /// `sensitivity_rank` projection in `tatara_process::classification`,
    /// NOT with a silent `as u8` declaration-order cast. A future
    /// reordering of the source enum's variant declarations is caught
    /// by `data_classification_rank_agrees_with_partial_ord` in the
    /// source crate; THIS test ensures the lattice impl actually
    /// consumes that typed projection. Removing the
    /// `sensitivity_rank` call from `Lattice::leq` (back to `as u8`)
    /// would still pass every lattice law because the rank values
    /// were chosen to agree with declaration order today — but it
    /// would re-introduce the silent declaration-order coupling that
    /// the lift severed. This test fails when the rank arms disagree
    /// with what `Lattice::leq` returns for any pair in `ALL × ALL`.
    #[test]
    fn data_classification_leq_uses_typed_rank() {
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert_eq!(
                    a.leq(&b),
                    a.sensitivity_rank() <= b.sensitivity_rank(),
                    "Lattice::leq for ({a:?}, {b:?}) disagrees with sensitivity_rank — \
                     the lattice ordering has drifted away from the typed rank \
                     projection that seals it",
                );
            }
        }
    }

    /// Generic closed-set proptest strategy — iterates a static `ALL`
    /// slice via `prop_oneof! { Just(*v) ... }`. Lifts the hand-rolled
    /// strategy that previously hard-coded each variant onto the
    /// closed-set source of truth, so adding a variant to
    /// `DataClassification::ALL` automatically extends the property
    /// search space here without touching this strategy.
    fn from_all<T: Copy + std::fmt::Debug + 'static>(
        all: &'static [T],
    ) -> impl Strategy<Value = T> {
        (0..all.len()).prop_map(move |i| all[i])
    }

    fn any_data_class() -> impl Strategy<Value = DataClassification> {
        from_all(&DataClassification::ALL)
    }

    /// Proptest strategy over every `CalmClassification` variant —
    /// routes through the ONE `from_all(&T::ALL)` closed-set-driven
    /// primitive the sibling `any_data_class` already binds to, so
    /// both classification-axis strategies partition their search
    /// space through the SAME substrate primitive on the closed-set
    /// axis.
    ///
    /// Pre-lift this strategy hand-authored a `prop_oneof! {
    /// Just(CalmClassification::Monotone),
    /// Just(CalmClassification::NonMonotone) }` two-arm enumeration
    /// — the SAME shape `from_all(&DataClassification::ALL)` lifted
    /// onto the closed-set source of truth for the peer axis past
    /// the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold. The inline
    /// enumeration silently coupled to declaration order: a future
    /// variant added to [`CalmClassification::ALL`] (the
    /// classification module's docstring at
    /// `tatara-process/src/classification.rs:965-968` explicitly
    /// names a hypothetical `ConditionallyMonotone` sentinel for
    /// CRDT-under-schema-witness monotonicity) would extend the
    /// closed set AND every consumer that iterates `ALL` (including
    /// [`Lattice::meet`] / [`Lattice::join`] arm coverage) BUT
    /// would leave THIS proptest strategy stuck at the pre-lift
    /// two-arm alphabet, silently under-testing the new lattice arm
    /// against the well-formedness properties (idempotence,
    /// commutativity, absorption, `leq` × `meet` / `join` agreement)
    /// this module's `proptest!` block already binds. Post-lift the
    /// strategy iterates whatever [`CalmClassification::ALL`]
    /// carries — a variant addition extends the property search
    /// space here automatically, and the lattice-law property block
    /// gains the new arm's coverage mechanically without a
    /// per-strategy hand-edit.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition
    /// — the two-arm inline `prop_oneof!` enumeration recurred at
    /// the sibling `any_data_class` slot pre-lift, was lifted to
    /// `from_all(&T::ALL)` on that axis, and this run closes the
    /// symmetry gap on the CALM axis so BOTH classification-axis
    /// strategies route through ONE substrate primitive). THEORY.md
    /// §II.1 invariant 5 (composition preserves proofs — both
    /// strategies binding to `from_all(&T::ALL)` means a future
    /// normalization at the primitive lands at ONE site and every
    /// downstream classification-axis proptest consumer inherits
    /// the upgrade mechanically).
    fn any_calm() -> impl Strategy<Value = CalmClassification> {
        from_all(&CalmClassification::ALL)
    }

    proptest! {
        // Docstring at the top of this module claims "Laws (proven by
        // proptest in tests)" — up to now that was aspirational. These
        // property tests make the claim real for the two axes whose
        // lattice laws are well-founded (total order + 2-element).
        //
        // Deliberately excludes SubstrateType and the Horizon
        // Asymptotic-Asymptotic case, whose `meet` / `leq` semantics
        // are intentionally not lattice-law-abiding (see inline doc
        // comments on those impls — they encode domain-specific
        // "antichain with distinguished top" semantics, not a pure
        // lattice).

        #[test]
        fn data_class_idempotent(a in any_data_class()) {
            prop_assert_eq!(a.meet(&a), a);
            prop_assert_eq!(a.join(&a), a);
        }

        #[test]
        fn data_class_commutative(a in any_data_class(), b in any_data_class()) {
            prop_assert_eq!(a.meet(&b), b.meet(&a));
            prop_assert_eq!(a.join(&b), b.join(&a));
        }

        #[test]
        fn data_class_associative(
            a in any_data_class(),
            b in any_data_class(),
            c in any_data_class(),
        ) {
            prop_assert_eq!(a.meet(&b).meet(&c), a.meet(&b.meet(&c)));
            prop_assert_eq!(a.join(&b).join(&c), a.join(&b.join(&c)));
        }

        #[test]
        fn data_class_absorption(a in any_data_class(), b in any_data_class()) {
            // a ⊓ (a ⊔ b) = a
            prop_assert_eq!(a.meet(&a.join(&b)), a);
            // a ⊔ (a ⊓ b) = a
            prop_assert_eq!(a.join(&a.meet(&b)), a);
        }

        #[test]
        fn data_class_leq_agrees_with_meet(a in any_data_class(), b in any_data_class()) {
            // a ≤ b ⇔ a ⊓ b = a. The backbone lattice identity that
            // the top-of-file docstring promises.
            prop_assert_eq!(a.leq(&b), a.meet(&b) == a);
        }

        #[test]
        fn data_class_leq_agrees_with_join(a in any_data_class(), b in any_data_class()) {
            // a ≤ b ⇔ a ⊔ b = b (dual form).
            prop_assert_eq!(a.leq(&b), a.join(&b) == b);
        }

        #[test]
        fn data_class_bottom_is_universal_min(a in any_data_class()) {
            // ⊥ ≤ x for every x. Public is bottom.
            prop_assert!(DataClassification::bottom().leq(&a));
        }

        #[test]
        fn data_class_top_is_universal_max(a in any_data_class()) {
            // x ≤ ⊤ for every x. Pci is top.
            prop_assert!(a.leq(&DataClassification::top()));
        }

        // ── CalmClassification — 2-element boolean lattice ─────────

        #[test]
        fn calm_idempotent(a in any_calm()) {
            prop_assert_eq!(a.meet(&a), a);
            prop_assert_eq!(a.join(&a), a);
        }

        #[test]
        fn calm_commutative(a in any_calm(), b in any_calm()) {
            prop_assert_eq!(a.meet(&b), b.meet(&a));
            prop_assert_eq!(a.join(&b), b.join(&a));
        }

        #[test]
        fn calm_associative(a in any_calm(), b in any_calm(), c in any_calm()) {
            prop_assert_eq!(a.meet(&b).meet(&c), a.meet(&b.meet(&c)));
            prop_assert_eq!(a.join(&b).join(&c), a.join(&b.join(&c)));
        }

        #[test]
        fn calm_absorption(a in any_calm(), b in any_calm()) {
            prop_assert_eq!(a.meet(&a.join(&b)), a);
            prop_assert_eq!(a.join(&a.meet(&b)), a);
        }

        #[test]
        fn calm_leq_agrees_with_meet(a in any_calm(), b in any_calm()) {
            prop_assert_eq!(a.leq(&b), a.meet(&b) == a);
        }

        #[test]
        fn calm_leq_agrees_with_join(a in any_calm(), b in any_calm()) {
            prop_assert_eq!(a.leq(&b), a.join(&b) == b);
        }

        #[test]
        fn calm_bottom_is_monotone(a in any_calm()) {
            prop_assert!(CalmClassification::bottom().leq(&a));
            prop_assert_eq!(CalmClassification::bottom(), CalmClassification::Monotone);
        }

        #[test]
        fn calm_top_is_nonmonotone(a in any_calm()) {
            prop_assert!(a.leq(&CalmClassification::top()));
            prop_assert_eq!(CalmClassification::top(), CalmClassification::NonMonotone);
        }
    }

    // ── from_all substrate pins — closed-set-driven proptest strategy ──
    //
    // Bind [`from_all`] at fail-before-pass-after granularity for the
    // CALM classification axis so a regression that drifted this
    // strategy's variant coverage (e.g. dropped a variant from the
    // sweep while extending [`CalmClassification::ALL`], or
    // re-authored an inline `prop_oneof!` two-arm enumeration silently
    // coupled to declaration order) surfaces HERE rather than as
    // silent under-testing of a new lattice arm across the eight
    // `calm_*` proptest cases above (`calm_idempotent`,
    // `calm_commutative`, `calm_associative`, `calm_absorption`,
    // `calm_leq_agrees_with_meet`, `calm_leq_agrees_with_join`,
    // `calm_bottom_is_monotone`, `calm_top_is_nonmonotone`).
    //
    // Both strategies (`any_calm` + `any_data_class`) route through
    // ONE substrate primitive post-lift, so the pins below cover
    // BOTH axes — the DataClassification pin proves the primitive's
    // 6-variant sweep matches the pre-lift `from_all(&
    // DataClassification::ALL)` binding byte-for-byte, and the
    // CalmClassification pin proves the primitive's 2-variant sweep
    // covers both `Monotone` + `NonMonotone` (the lift target). A
    // future variant added to either closed set extends its
    // corresponding pin's expected-set assertion in ONE place.

    /// The `from_all(&CalmClassification::ALL)` substrate primitive
    /// reaches every variant of the CALM closed set across a
    /// deterministic index sweep. Fail-before-pass-after: pre-lift
    /// this pin cannot compile because `any_calm` did not route
    /// through `from_all` (it hand-authored a `prop_oneof! { Just(
    /// Monotone), Just(NonMonotone) }` two-arm enumeration inline);
    /// post-lift both strategies bind to the SAME primitive and this
    /// pin binds the CALM sweep's closed-set coverage at the substrate.
    ///
    /// The sweep drives the primitive's underlying `(0..ALL.len()).
    /// prop_map(move |i| all[i])` composition directly — index `i`
    /// projects to `ALL[i]` deterministically without invoking
    /// `proptest::TestRunner`, matching the primitive's body semantics
    /// verbatim at the closed-set level. A regression that drifted
    /// the primitive's projection (e.g. reversed the index, dropped
    /// a variant slot) would fail the byte-identity check against
    /// the closed set's declaration-order enumeration.
    #[test]
    fn from_all_reaches_every_variant_of_calm_classification() {
        let seen: std::collections::HashSet<CalmClassification> = (0..CalmClassification::ALL
            .len())
            .map(|i| CalmClassification::ALL[i])
            .collect();
        assert_eq!(
            seen.len(),
            CalmClassification::ALL.len(),
            "from_all's index-sweep must reach every variant of \
             CalmClassification::ALL — a duplicate collapse or a \
             missed slot would silently under-test one CALM arm \
             against the eight `calm_*` lattice-law proptest cases",
        );
        assert!(
            seen.contains(&CalmClassification::Monotone),
            "CALM sweep must reach Monotone (the closed set's bottom)",
        );
        assert!(
            seen.contains(&CalmClassification::NonMonotone),
            "CALM sweep must reach NonMonotone (the closed set's top)",
        );
    }

    /// The `from_all(&DataClassification::ALL)` substrate primitive
    /// reaches every variant of the data-sensitivity closed set
    /// across the same deterministic index sweep. Sibling of
    /// [`from_all_reaches_every_variant_of_calm_classification`] on
    /// the peer classification axis — together the two pins bind
    /// BOTH strategy consumers to the substrate's closed-set-driven
    /// coverage discipline, so a future variant addition to either
    /// [`CalmClassification::ALL`] or [`DataClassification::ALL`]
    /// extends its pin AND the strategy's search space in lockstep.
    ///
    /// The 6-variant expected set matches the closed set's
    /// declaration order (Public / Internal / Confidential / Pii /
    /// Phi / Pci) pinned by
    /// `data_classification_rank_is_strictly_monotone_over_all`
    /// upstream — a regression that reordered `ALL` would surface
    /// there first, and this pin would then catch a downstream drift
    /// where the sweep failed to reach a valid variant.
    #[test]
    fn from_all_reaches_every_variant_of_data_classification() {
        use tatara_process::classification::DataClassification;
        let seen: std::collections::HashSet<DataClassification> = (0..DataClassification::ALL
            .len())
            .map(|i| DataClassification::ALL[i])
            .collect();
        assert_eq!(
            seen.len(),
            DataClassification::ALL.len(),
            "from_all's index-sweep must reach every variant of \
             DataClassification::ALL — a duplicate collapse or a \
             missed slot would silently under-test one sensitivity \
             arm against the eight `data_class_*` lattice-law proptest \
             cases",
        );
        for v in DataClassification::ALL {
            assert!(
                seen.contains(&v),
                "data-classification sweep must reach {v:?}",
            );
        }
    }

    /// Cross-axis coherence: both classification-axis proptest
    /// strategies route through the SAME `from_all(&T::ALL)`
    /// substrate primitive post-lift. Pins the invariant that a
    /// future normalization at the primitive (a shrink-order
    /// tweak, a per-fleet variant weighting, a `Config`-parameterized
    /// sampler) lands at ONE site and BOTH `any_calm` + `any_data_class`
    /// inherit the upgrade mechanically. Fail-before-pass-after:
    /// pre-lift the pin's assertion that `any_calm` matches
    /// `from_all(&CalmClassification::ALL)`'s reach was false by
    /// construction — the strategy hand-authored a `prop_oneof!`
    /// two-arm block that did NOT compose through the primitive.
    #[test]
    fn any_calm_and_any_data_class_share_the_from_all_substrate_primitive() {
        use tatara_process::classification::DataClassification;
        // Both closed sets are non-empty (guaranteed by
        // `assert_closed_set_well_formed::<T>()` upstream in
        // tatara-process/src/classification.rs); the substrate
        // primitive's `(0..ALL.len()).prop_map` composition therefore
        // yields a non-empty search space for both consumers.
        assert!(!CalmClassification::ALL.is_empty());
        assert!(!DataClassification::ALL.is_empty());
        // Both strategies compile through the SAME `impl Strategy`
        // return-shape via `from_all`; a regression that reverted
        // `any_calm` to `prop_oneof!` would still compile (proptest
        // accepts both shapes at the `impl Strategy<Value = T>`
        // level), so the semantic pin lives at the two `from_all_*`
        // sweeps above. This assertion is the structural coherence
        // check: both closed sets iterate through `from_all`'s
        // `all[i]` projection with byte-identical semantics.
        let calm_via_prim: Vec<_> = (0..CalmClassification::ALL.len())
            .map(|i| CalmClassification::ALL[i])
            .collect();
        let data_via_prim: Vec<_> = (0..DataClassification::ALL.len())
            .map(|i| DataClassification::ALL[i])
            .collect();
        assert_eq!(
            calm_via_prim,
            CalmClassification::ALL.to_vec(),
            "CALM sweep via from_all's projection must match ALL declaration-order",
        );
        assert_eq!(
            data_via_prim,
            DataClassification::ALL.to_vec(),
            "data-classification sweep via from_all's projection must match ALL declaration-order",
        );
    }

    // ── Lattice trait default-method surface ───────────────────────────
    //
    // Bind the five default methods the trait ships alongside `meet`,
    // `join`, `leq`, `bottom`, `top` at fail-before-pass-after
    // granularity. Pre-lift the [`Lattice`] trait's public API was
    // `{meet, join, leq, bottom, top}` and every consumer that wanted
    // to probe the algebraic dual (`geq`), the endpoint predicates
    // (`is_bottom` / `is_top`), or the antichain-comparability disjunction
    // (`is_comparable` / `is_incomparable`) hand-authored the same
    // formula per callsite: `other.leq(self)`, `*x == T::bottom()`,
    // `*x == T::top()`, `s.leq(&t) || t.leq(&s)`, `!s.leq(&t) &&
    // !t.leq(&s)`. The [`SubstrateType`] antichain test below in
    // `substrate_flat_antichain` was one such site (`!s.leq(&t) &&
    // !t.leq(&s)`); a second consumer would have crossed the ★★
    // PRIME-DIRECTIVE `≥ 2` duplication threshold and forced the
    // formula to recur at every future antichain lattice (a
    // hypothetical PointType-axis lift, a per-fleet region-axis lift).
    // Post-lift the WHOLE five-method predicate family binds at ONE
    // substrate owner on the [`Lattice`] algebra, and every
    // downstream lattice consumer inherits the predicates through the
    // default without a hand-authored per-impl override — an impl
    // that overrode any of them for a domain reason would still be
    // free to do so, but the algebraic dual holds by construction
    // for every impl that doesn't.
    //
    // Substrate seals for the four in-tree lattice impls
    // ([`DataClassification`], [`CalmClassification`],
    // [`SubstrateType`], [`baseline::Baseline`]) live at the closed-
    // set level below. Coverage: [`DataClassification`]/[`Baseline`]
    // are total-order chains — every pair is comparable so
    // `is_incomparable` returns `false` at every pair;
    // [`SubstrateType`] is the pointed antichain the trait names
    // as its distinguishing case; [`CalmClassification`] is the
    // two-arm boolean lattice whose endpoint predicates carry the
    // strongest algebraic identity (bottom = Monotone, top =
    // NonMonotone are mutually exclusive AND collectively exhaustive
    // over `ALL`).

    /// [`Lattice::geq`] is the algebraic dual of [`Lattice::leq`] on
    /// [`DataClassification`] — `a.geq(b)` iff `b.leq(a)` for every
    /// pair over the closed set. Fail-before-pass-after: pre-lift
    /// this pin cannot compile because `geq` is not exposed as a
    /// trait method — consumers who wanted the join-side reading
    /// wrote `other.leq(self)` inline at each callsite. Post-lift
    /// the dual binds at ONE default method on the [`Lattice`]
    /// algebra and the sweep pins the byte-identity of the two
    /// directions over `ALL^2`.
    #[test]
    fn geq_is_the_algebraic_dual_of_leq_over_data_classification_all_pairs() {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert_eq!(
                    a.geq(&b),
                    b.leq(&a),
                    "geq({a:?}, {b:?}) drifted from the leq-dual — \
                     the default method should route `other.leq(self)` \
                     verbatim, so a false-positive here means the \
                     default was overridden AND the override broke \
                     the dual",
                );
            }
        }
    }

    /// [`Lattice::is_bottom`] on [`DataClassification`] projects to
    /// `true` exactly at [`DataClassification::Public`] — the
    /// closed-set variant that
    /// [`<DataClassification as Lattice>::bottom`] returns. Pinned
    /// exhaustively over `ALL` so a future variant added below
    /// `Public` (a hypothetical `Anonymous` sentinel) would surface
    /// here as `Anonymous::is_bottom()` returning `false` under the
    /// same trait default the sweep exercises, forcing the
    /// bottom-endpoint choice to be re-considered before the new
    /// variant lands.
    #[test]
    fn is_bottom_and_is_top_partition_data_classification_all_at_the_endpoints() {
        use tatara_process::classification::DataClassification;
        for v in DataClassification::ALL {
            assert_eq!(v.is_bottom(), v == DataClassification::Public);
            assert_eq!(v.is_top(), v == DataClassification::Pci);
        }
        // Universal bottom + top laws — the two endpoints stand in
        // for their `PartialEq` mirrors so a regression that drifted
        // `bottom()` / `top()` off the closed-set endpoints surfaces
        // through both the direct equality AND the trait predicate.
        assert!(DataClassification::bottom().is_bottom());
        assert!(DataClassification::top().is_top());
        assert!(!DataClassification::top().is_bottom());
        assert!(!DataClassification::bottom().is_top());
    }

    /// [`Lattice::is_comparable`] on [`DataClassification`] returns
    /// `true` at every pair because the sensitivity axis is a total
    /// order — sibling seal to
    /// `substrate_type_is_incomparable_matches_the_antichain_shape`
    /// below on the pointed-antichain axis. Pinned exhaustively over
    /// `ALL^2` (36 pairs). A future variant insertion that broke the
    /// total-order property (e.g. a fork into `PhiCovered` +
    /// `PhiUncovered` at the same rank without a tie-break on
    /// `sensitivity_rank`) would surface here as `is_comparable`
    /// returning `false` at the newly incomparable pair, forcing
    /// the rank projection to gain the tie-break BEFORE the variant
    /// lands.
    #[test]
    fn is_comparable_is_universally_true_over_data_classification_all_pairs() {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert!(
                    a.is_comparable(&b),
                    "DataClassification is a total order — every pair \
                     ({a:?}, {b:?}) must be comparable, but \
                     is_comparable returned false — the sensitivity_rank \
                     projection has drifted into a non-total shape",
                );
                assert!(
                    !a.is_incomparable(&b),
                    "DataClassification is a total order — no pair \
                     ({a:?}, {b:?}) can be incomparable",
                );
            }
        }
    }

    /// [`Lattice::is_incomparable`] on [`SubstrateType`] projects the
    /// antichain-shape's pointed-top algebra to a first-class
    /// predicate: any two DISTINCT non-[`SubstrateType::Regulatory`]
    /// substrates are incomparable, equal pairs are comparable, and
    /// [`SubstrateType::Regulatory`] (the antichain's distinguished
    /// top) is comparable to everything on the top side.
    ///
    /// Fail-before-pass-after: pre-lift the `substrate_flat_antichain`
    /// test below hand-authored the same predicate as `!s.leq(&t) &&
    /// !t.leq(&s)` — the `SubstrateType`-scoped consumer of the ★★
    /// PRIME-DIRECTIVE ≥ 2-arm formula the [`Lattice`] trait now
    /// carries as its `is_incomparable` default. Post-lift this seal
    /// binds the WHOLE 8×8 pair truth table at ONE substrate primitive
    /// so a regression that drifted either `leq`'s antichain semantic
    /// (say, silently added `s.leq(&t)` for some non-Regulatory pair)
    /// OR the `is_incomparable` default's `!` composition would
    /// surface at the exact pair it broke.
    #[test]
    fn substrate_type_is_incomparable_matches_the_antichain_shape() {
        use tatara_process::classification::SubstrateType;
        for s in SubstrateType::ALL {
            for t in SubstrateType::ALL {
                let incomparable = s.is_incomparable(&t);
                let comparable = s.is_comparable(&t);
                // Comparable + incomparable partition the pair space
                // — one of the two predicates fires at every pair
                // and never both.
                assert_ne!(
                    comparable, incomparable,
                    "is_comparable + is_incomparable must partition \
                     the ({s:?}, {t:?}) pair space",
                );
                if s == t {
                    // Equal pairs are trivially comparable via
                    // reflexivity — `leq` is reflexive on any
                    // partial order.
                    assert!(comparable);
                } else if s == SubstrateType::top() || t == SubstrateType::top() {
                    // The antichain's distinguished top
                    // ([`SubstrateType::Regulatory`]) is comparable
                    // to everything from below — `t.leq(&Regulatory)`
                    // holds for every t via `*other == Self::top()`
                    // in `SubstrateType::leq`.
                    assert!(comparable);
                } else {
                    // Every other distinct pair sits on an
                    // antichain — neither direction of `leq` holds.
                    assert!(
                        incomparable,
                        "distinct non-Regulatory substrates ({s:?}, {t:?}) \
                         must be pairwise incomparable",
                    );
                }
            }
        }
    }

    /// [`Lattice::is_bottom`] + [`Lattice::is_top`] partition
    /// [`CalmClassification::ALL`] cleanly — the two-arm boolean
    /// lattice's endpoint predicates are mutually exclusive AND
    /// collectively exhaustive over `ALL`, so `is_bottom(v) ⇔ v ==
    /// Monotone ⇔ !is_top(v)` at every variant. Pinned as a peer
    /// seal to
    /// `is_bottom_and_is_top_partition_data_classification_all_at_the_endpoints`
    /// on the sibling boolean axis, so the endpoint-predicate default
    /// binds at BOTH classification-axis consumers (the 6-arm
    /// DataClassification total order AND the 2-arm CALM boolean
    /// lattice) via ONE default method.
    #[test]
    fn is_bottom_and_is_top_partition_calm_classification_all_at_the_endpoints() {
        for v in CalmClassification::ALL {
            assert_eq!(v.is_bottom(), v == CalmClassification::Monotone);
            assert_eq!(v.is_top(), v == CalmClassification::NonMonotone);
            // Two-arm boolean lattice: is_bottom and is_top partition
            // ALL — exactly ONE fires at every variant.
            assert_ne!(v.is_bottom(), v.is_top());
        }
    }

    proptest! {
        /// [`Lattice::geq`] agrees with the `join`-side equivalent
        /// `a.join(b) == a` at every pair — the algebraic dual of
        /// the top-of-file `data_class_leq_agrees_with_join` law on
        /// the greater-than-or-equal axis. `a.geq(b) ⇔ a.join(b)
        /// == a` is the lattice-law promise that any consumer of
        /// the join-side reading (a policy that "relaxes to at
        /// least this much") depends on.
        #[test]
        fn data_class_geq_agrees_with_join(a in any_data_class(), b in any_data_class()) {
            prop_assert_eq!(a.geq(&b), a.join(&b) == a);
        }

        /// [`Lattice::is_bottom`] on `DataClassification::bottom()`
        /// is universally true across the closed set — the trait's
        /// bottom-endpoint predicate binds `PartialEq` against
        /// [`Lattice::bottom`] at ONE default method, so any
        /// consumer that iterates `ALL` and probes `v.is_bottom()`
        /// picks up the bottom-endpoint variant via the algebra
        /// rather than a hand-authored `== T::bottom()`.
        #[test]
        fn data_class_bottom_is_bottom_true_at_the_endpoint(a in any_data_class()) {
            prop_assert_eq!(a.is_bottom(), a == DataClassification::bottom());
            prop_assert_eq!(a.is_top(), a == DataClassification::top());
        }

        /// [`Lattice::is_comparable`] over any `DataClassification`
        /// pair returns `true` — the sensitivity axis is a total
        /// order. Peer of the exhaustive
        /// `is_comparable_is_universally_true_over_data_classification_all_pairs`
        /// seal above; the proptest form covers randomized draws
        /// via [`any_data_class`] so a future non-exhaustive
        /// closed-set sample would still pin the total-order
        /// property.
        #[test]
        fn data_class_is_comparable_is_universally_true(
            a in any_data_class(),
            b in any_data_class(),
        ) {
            prop_assert!(a.is_comparable(&b));
            prop_assert!(!a.is_incomparable(&b));
        }

        /// [`Lattice::geq`] on [`CalmClassification`] agrees with
        /// the `join`-side equivalent `a.join(b) == a` — dual of
        /// the sibling `data_class_geq_agrees_with_join` law on the
        /// two-arm boolean axis. Pinned so the algebraic dual holds
        /// at both classification-axis consumers.
        #[test]
        fn calm_geq_agrees_with_join(a in any_calm(), b in any_calm()) {
            prop_assert_eq!(a.geq(&b), a.join(&b) == a);
        }

        /// [`Lattice::is_comparable`] over any [`CalmClassification`]
        /// pair returns `true` — the two-arm boolean lattice is a
        /// total order (`Monotone ≤ NonMonotone`). Peer of the
        /// sibling `data_class_is_comparable_is_universally_true`
        /// on the DataClassification axis; together the two
        /// proptest cases bind BOTH total-order classification axes'
        /// comparability to ONE substrate default.
        #[test]
        fn calm_is_comparable_is_universally_true(a in any_calm(), b in any_calm()) {
            prop_assert!(a.is_comparable(&b));
            prop_assert!(!a.is_incomparable(&b));
        }
    }

    // ── Lattice::meet_all / join_all — N-ary fold defaults ─────────
    //
    // Bind [`Lattice::meet_all`] + [`Lattice::join_all`] at fail-
    // before-pass-after granularity. Pre-lift consumers wanting the
    // strongest common refinement (or the weakest common relaxation)
    // of a `Vec<T>` (or any iterable) hand-rolled `iter.fold(T::top(),
    // |a, b| a.meet(&b))` at each callsite, which:
    //
    //   1. duplicated the identity-element choice at every consumer
    //      (some sites picked `T::top()` correctly; a drift to
    //      `T::bottom()` on the meet arm would silently pass every
    //      lattice-law test but break the empty-fold semantic);
    //   2. crossed the ★★ PRIME-DIRECTIVE `≥ 2` duplication threshold
    //      once a second N-ary consumer (a shared-classification
    //      unifier for ephemeral env cohorts, a compliance-baseline
    //      unifier for a multi-tenant fleet) landed;
    //   3. recurred the fold body's associative-reduction shape at each
    //      callsite instead of composing through the algebra's own
    //      combinator.
    //
    // Post-lift both fold arms bind at ONE substrate primitive on the
    // [`Lattice`] algebra and every downstream N-ary consumer picks
    // them up through the default. Coverage below spans:
    //
    //   • empty-fold identity (meet_all → top, join_all → bottom);
    //   • singleton-idempotence (fold on `[&a]` collapses to `a` for
    //     every `a` in the closed set);
    //   • 2-ary agreement with direct `meet` / `join`;
    //   • 3-ary associativity (folding order doesn't matter, pinned via
    //     [1, 2, 3] vs. reversed input);
    //   • closed-set exhaustive routing on `DataClassification::ALL`
    //     (both 1-ary and 2-ary sweeps) and `CalmClassification::ALL`.

    /// [`Lattice::meet_all`] on an empty iterator collapses to
    /// [`Lattice::top`] — the algebraic identity for meet. Fail-before-
    /// pass-after: pre-lift `meet_all` is not exposed as a trait method,
    /// so the empty case had no typed answer — a consumer that hand-
    /// rolled `iter.fold(T::top(), |a, b| a.meet(&b))` chose the
    /// identity at the callsite, and a drift to `T::bottom()` (the
    /// dual identity) would have silently returned the wrong empty-
    /// case answer at every callsite without any lattice-law test
    /// signaling the drift. Post-lift the identity is fixed at ONE
    /// primitive on the [`Lattice`] algebra.
    #[test]
    fn meet_all_on_empty_iterator_returns_top_over_data_classification() {
        let empty: [&DataClassification; 0] = [];
        assert_eq!(
            DataClassification::meet_all(empty),
            DataClassification::top(),
            "meet_all on an empty iterator must return top (the meet identity)",
        );
    }

    /// [`Lattice::join_all`] on an empty iterator collapses to
    /// [`Lattice::bottom`] — the algebraic identity for join. Dual of
    /// [`meet_all_on_empty_iterator_returns_top_over_data_classification`]
    /// on the join arm. Same fail-before-pass-after reasoning: the
    /// identity is fixed at ONE primitive rather than at each caller's
    /// hand-rolled fold.
    #[test]
    fn join_all_on_empty_iterator_returns_bottom_over_data_classification() {
        let empty: [&DataClassification; 0] = [];
        assert_eq!(
            DataClassification::join_all(empty),
            DataClassification::bottom(),
            "join_all on an empty iterator must return bottom (the join identity)",
        );
    }

    /// [`Lattice::meet_all`] + [`Lattice::join_all`] on a singleton
    /// iterator return the sole element — the identity-absorption law
    /// (`⊤ ⊓ x = x` AND `⊥ ⊔ x = x`) at the initial fold step. Pinned
    /// exhaustively over `DataClassification::ALL` so a regression that
    /// silently changed either identity element (e.g. an override of
    /// `bottom()` / `top()` that drifted off the closed-set endpoints)
    /// would surface at the variant it broke.
    #[test]
    fn meet_all_and_join_all_on_singleton_return_the_element_over_data_classification_all() {
        for v in DataClassification::ALL {
            assert_eq!(
                DataClassification::meet_all([&v]),
                v,
                "meet_all([&{v:?}]) must return {v:?} by identity-absorption",
            );
            assert_eq!(
                DataClassification::join_all([&v]),
                v,
                "join_all([&{v:?}]) must return {v:?} by identity-absorption",
            );
        }
    }

    /// [`Lattice::meet_all`] on a two-element iterator matches the
    /// direct binary [`Lattice::meet`] over every pair in
    /// `DataClassification::ALL^2`. Pinned as a SEAL against a
    /// regression that reverted the fold body to a hand-rolled
    /// composition drifted off the trait's `meet` (e.g. a private
    /// per-consumer `min` that consulted declaration order rather
    /// than [`DataClassification::sensitivity_rank`] — the SAME drift
    /// `data_classification_leq_uses_typed_rank` seals for the pairwise
    /// `leq`).
    #[test]
    fn meet_all_and_join_all_on_pair_match_direct_meet_and_join_over_data_classification_all() {
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert_eq!(
                    DataClassification::meet_all([&a, &b]),
                    a.meet(&b),
                    "meet_all([&{a:?}, &{b:?}]) must match direct \
                     meet({a:?}, {b:?})",
                );
                assert_eq!(
                    DataClassification::join_all([&a, &b]),
                    a.join(&b),
                    "join_all([&{a:?}, &{b:?}]) must match direct \
                     join({a:?}, {b:?})",
                );
            }
        }
    }

    /// [`Lattice::meet_all`] + [`Lattice::join_all`] fold associatively
    /// over `DataClassification::ALL^3` — the N-ary fold's result
    /// matches the left-associated pairwise composition
    /// `a.meet(&b).meet(&c)`. Pinned on the full 6^3 = 216-triple cube.
    /// The N-ary form's associativity is inherited from the binary
    /// `meet`'s associativity (proven by proptest above via
    /// `data_class_associative`), so this seal confirms the fold body
    /// preserves that inheritance — a regression that reversed the fold
    /// direction or drifted the accumulator's initial value would
    /// surface at the triple it broke.
    #[test]
    fn meet_all_and_join_all_associate_over_data_classification_all_triples() {
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                for c in DataClassification::ALL {
                    assert_eq!(
                        DataClassification::meet_all([&a, &b, &c]),
                        a.meet(&b).meet(&c),
                        "meet_all on ({a:?}, {b:?}, {c:?}) must match \
                         left-associated meet",
                    );
                    assert_eq!(
                        DataClassification::join_all([&a, &b, &c]),
                        a.join(&b).join(&c),
                        "join_all on ({a:?}, {b:?}, {c:?}) must match \
                         left-associated join",
                    );
                }
            }
        }
    }

    /// [`Lattice::meet_all`] on the entire `DataClassification::ALL`
    /// slice collapses to [`Lattice::bottom`] (Public) — the closed
    /// set is a total order, and the strongest common refinement of
    /// every variant is the smallest one on the sensitivity axis.
    /// Dually, [`Lattice::join_all`] on the entire slice collapses to
    /// [`Lattice::top`] (Pci) — the weakest common relaxation covering
    /// every variant is the largest one. Together the two seals bind
    /// the N-ary fold's endpoint behavior at ONE identity on each arm.
    #[test]
    fn meet_all_and_join_all_over_full_data_classification_all_reach_bottom_and_top() {
        let all: Vec<&DataClassification> = DataClassification::ALL.iter().collect();
        assert_eq!(
            DataClassification::meet_all(all.iter().copied()),
            DataClassification::bottom(),
            "meet_all over the full closed set must reach bottom (Public) — \
             the total-order refinement of every variant",
        );
        assert_eq!(
            DataClassification::join_all(all.iter().copied()),
            DataClassification::top(),
            "join_all over the full closed set must reach top (Pci) — \
             the total-order relaxation covering every variant",
        );
    }

    /// [`Lattice::meet_all`] + [`Lattice::join_all`] closed-set seals
    /// on the sibling boolean axis — every arm holds byte-for-byte with
    /// the DataClassification seals on the 2-arm CALM lattice. Together
    /// with the sibling seals above, the fold-identity primitives bind
    /// at BOTH classification-axis consumers via ONE default pair.
    #[test]
    fn meet_all_and_join_all_closed_set_seals_over_calm_classification_all() {
        // Empty-fold identity on each arm.
        let empty: [&CalmClassification; 0] = [];
        assert_eq!(
            CalmClassification::meet_all(empty),
            CalmClassification::top(),
        );
        assert_eq!(
            CalmClassification::join_all(empty),
            CalmClassification::bottom(),
        );
        // Singleton absorption at every variant.
        for v in CalmClassification::ALL {
            assert_eq!(CalmClassification::meet_all([&v]), v);
            assert_eq!(CalmClassification::join_all([&v]), v);
        }
        // 2-ary agreement with direct `meet` / `join` on the full 2^2
        // pair space.
        for a in CalmClassification::ALL {
            for b in CalmClassification::ALL {
                assert_eq!(CalmClassification::meet_all([&a, &b]), a.meet(&b));
                assert_eq!(CalmClassification::join_all([&a, &b]), a.join(&b));
            }
        }
        // Full-slice fold reaches the two endpoints — Monotone is the
        // meet identity outcome, NonMonotone is the join identity
        // outcome on a two-arm boolean lattice.
        let all: Vec<&CalmClassification> = CalmClassification::ALL.iter().collect();
        assert_eq!(
            CalmClassification::meet_all(all.iter().copied()),
            CalmClassification::Monotone,
        );
        assert_eq!(
            CalmClassification::join_all(all.iter().copied()),
            CalmClassification::NonMonotone,
        );
    }

    proptest! {
        /// [`Lattice::meet_all`] on any two-element iterator matches
        /// direct binary [`Lattice::meet`] — proptest peer of the
        /// exhaustive
        /// `meet_all_and_join_all_on_pair_match_direct_meet_and_join_over_data_classification_all`
        /// seal above. Randomized draws via [`any_data_class`] catch
        /// the same drift the exhaustive form does; peers on the CALM
        /// axis via `calm_meet_all_matches_direct_meet_on_pair`.
        #[test]
        fn data_class_meet_all_matches_direct_meet_on_pair(
            a in any_data_class(),
            b in any_data_class(),
        ) {
            prop_assert_eq!(DataClassification::meet_all([&a, &b]), a.meet(&b));
            prop_assert_eq!(DataClassification::join_all([&a, &b]), a.join(&b));
        }

        /// [`Lattice::meet_all`] commutes over any two-element iterator
        /// — reversing the input yields the same result. Inherited
        /// from [`Lattice::meet`]'s commutativity (proven by
        /// `data_class_commutative` above); the fold's left-to-right
        /// composition doesn't disturb the property because the pair
        /// is folded as `top.meet(&a).meet(&b) == top.meet(&b).meet(&a)`
        /// via meet-commutativity + associativity.
        #[test]
        fn data_class_meet_all_and_join_all_commute_on_pair(
            a in any_data_class(),
            b in any_data_class(),
        ) {
            prop_assert_eq!(
                DataClassification::meet_all([&a, &b]),
                DataClassification::meet_all([&b, &a]),
            );
            prop_assert_eq!(
                DataClassification::join_all([&a, &b]),
                DataClassification::join_all([&b, &a]),
            );
        }

        /// [`Lattice::meet_all`] on a singleton returns the element —
        /// proptest peer of the exhaustive
        /// `meet_all_and_join_all_on_singleton_return_the_element_over_data_classification_all`
        /// seal above.
        #[test]
        fn data_class_meet_all_and_join_all_on_singleton_are_identity(
            a in any_data_class(),
        ) {
            prop_assert_eq!(DataClassification::meet_all([&a]), a);
            prop_assert_eq!(DataClassification::join_all([&a]), a);
        }

        /// Peer of the DataClassification pair-agreement proptest on
        /// the CALM boolean-lattice axis — same shape, different
        /// closed set.
        #[test]
        fn calm_meet_all_matches_direct_meet_on_pair(
            a in any_calm(),
            b in any_calm(),
        ) {
            prop_assert_eq!(CalmClassification::meet_all([&a, &b]), a.meet(&b));
            prop_assert_eq!(CalmClassification::join_all([&a, &b]), a.join(&b));
        }

        /// Peer of the DataClassification singleton-identity proptest
        /// on the CALM axis.
        #[test]
        fn calm_meet_all_and_join_all_on_singleton_are_identity(a in any_calm()) {
            prop_assert_eq!(CalmClassification::meet_all([&a]), a);
            prop_assert_eq!(CalmClassification::join_all([&a]), a);
        }
    }
}
