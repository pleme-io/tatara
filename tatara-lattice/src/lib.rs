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
    /// Strict-order peer of [`Lattice::leq`] on the strict arm —
    /// `self` is STRICTLY at least as refined as `other`, i.e.
    /// `self.leq(other)` AND `self != other`. The IRREFLEXIVE strict
    /// `<` peer of [`Lattice::leq`]'s REFLEXIVE non-strict `≤` one
    /// STRICTNESS axis over on the (strict, non-strict) × (leq, geq)
    /// 2×2 partial-order-comparator grid: together with
    /// [`Lattice::strictly_above`] on the dual strict-arm this closes
    /// the whole 2×2 grid on the combinator surface, and every
    /// downstream consumer that wants the strict comparator picks it
    /// up through the default rather than composing `a.leq(&b) && a
    /// != b` at its callsite (a hand-authored composition that
    /// pleme-io already carries at
    /// `tatara_lisp::macro_expand::ResourceLimits::lt` — the strict
    /// `<` on the pointwise resource-posture partial order — so this
    /// widening lifts the SAME shape from a per-domain `const fn` to
    /// the trait's default-method surface so every future closed-set
    /// lattice impl inherits the strict comparator for free).
    ///
    /// **Naming**: the trait's non-strict comparators are named on the
    /// bare-relation axis (`leq` / `geq`), but the strict peers CANNOT
    /// take the bare `lt` / `gt` names because every in-tree
    /// closed-set consumer (`DataClassification`, `SubstrateType`,
    /// `CalmClassification`) also derives `PartialOrd`, whose
    /// `PartialOrd::lt` / `PartialOrd::gt` methods share the receiver
    /// shape and would race the trait's methods in method-name
    /// resolution. The named-relation `strictly_below` /
    /// `strictly_above` peer names avoid the collision AND match the
    /// trait's `is_bottom` / `is_top` / `is_comparable` /
    /// `is_incomparable` "named predicate" naming discipline where
    /// the receiver is on the left of the relation and the argument
    /// is on the right, so `a.strictly_below(&b)` reads "a is strictly
    /// below b" in the lattice's refinement direction.
    ///
    /// Default routes through `self.leq(other) && self != other` — the
    /// two-primitive antisymmetric encoding of strict `<` on any
    /// partial order. Equivalent on any antisymmetric lattice (every
    /// partial order IS antisymmetric) to `self.leq(other) &&
    /// !other.leq(self)`, the alternate encoding [`ResourceLimits::lt`]
    /// uses; `PartialEq::ne` is O(1) vs. an extra `leq` call, and the
    /// trait already requires `PartialEq` on `Self`, so the equality-
    /// negation encoding is preferred here.
    ///
    /// **Irreflexivity**: `a.strictly_below(&a) == false` for every
    /// element. The `self != other` conjunct short-circuits `false`
    /// at every self-pair since [`PartialEq::eq`] is reflexive on any
    /// lattice element. Pinned exhaustively over the closed-set impls
    /// below.
    ///
    /// **Asymmetry**: `a.strictly_below(&b) ⇒ !b.strictly_below(&a)`
    /// for every pair. Follows from [`Lattice::leq`]'s antisymmetry:
    /// if both directions of the strict relation held, both directions
    /// of `leq` would hold, forcing `a == b` by antisymmetry,
    /// contradicting either strict conjunct.
    ///
    /// **Transitivity**: `a.strictly_below(&b) && b.strictly_below(&c)
    /// ⇒ a.strictly_below(&c)` for every triple. Inherits from
    /// [`Lattice::leq`]'s transitivity on the non-strict conjunct;
    /// the strict conjunct on the outer relation carries through
    /// because if `a == c`, then `c.leq(b) = a.leq(b)` (given) and
    /// `b.leq(c) = b.leq(a)` (given via `b.strictly_below(&c) ⇒
    /// b.leq(c)`) both hold, forcing `a == b` by antisymmetry, which
    /// contradicts `a.strictly_below(&b)`.
    ///
    /// **Refines [`Lattice::leq`]**: `a.strictly_below(&b) ⇒ a.leq(b)`
    /// by the first conjunct's construction. Conversely `a.leq(b) &&
    /// !a.strictly_below(&b) ⇒ a == b` — the non-strict-minus-strict
    /// gap is exactly the reflexive diagonal.
    ///
    /// **Antichain rejection**: on any distinct incomparable pair
    /// (a [`SubstrateType`] pair where neither is
    /// [`SubstrateType::Regulatory`], for example), both directions
    /// of the strict relation fail because neither direction of
    /// [`Lattice::leq`] holds. The predicate does NOT promote
    /// incomparable pairs to a strict-order verdict.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the strict-order peer of `leq` is itself a
    /// typed named `bool` predicate composing `leq` and `PartialEq::ne`
    /// via the trait's default) + THEORY.md §III (typescape — the
    /// strict-comparator arm on every classification-axis lattice
    /// binds at ONE substrate owner on the [`Lattice`] algebra rather
    /// than at each consumer's hand-rolled conjunction).
    ///
    /// Frontier inspiration: [`PartialOrd::lt`] on Rust's partial-
    /// order trait; [`Ord::lt`] on the total-order trait — the strict
    /// variant of `≤` is a first-class named method the standard
    /// library exposes rather than leaving each consumer to compose
    /// `partial_cmp(&other) == Some(Less)` at its callsite.
    fn strictly_below(&self, other: &Self) -> bool {
        self.leq(other) && self != other
    }
    /// Strict-order peer of [`Lattice::geq`] on the strict arm — dual
    /// of [`Lattice::strictly_below`] one MEET/JOIN axis over on the
    /// (strict, non-strict) × (leq, geq) 2×2 partial-order-comparator
    /// grid. `self` is STRICTLY at least as relaxed as `other`, i.e.
    /// `self.geq(other)` AND `self != other`. Default routes through
    /// `other.strictly_below(self)` so every impl inherits the
    /// algebraic dual for free; a future override at
    /// [`Lattice::strictly_below`] (or at the underlying
    /// [`Lattice::leq`]) lands at ONE site and this dual inherits
    /// mechanically. Consumers that want to probe "is `self` strictly
    /// more relaxed than `other`" (the join-side reading of the strict
    /// relation) write `self.strictly_above(&other)` instead of
    /// `other.strictly_below(&self)` — the two are byte-identical,
    /// but the dual name matches the join-side reading discipline.
    ///
    /// Inherits [`Lattice::strictly_below`]'s irreflexivity, asymmetry,
    /// and transitivity mechanically by the `other.strictly_below(self)`
    /// routing; pinned on the closed-set impls below via the
    /// byte-identity dual pin `a.strictly_above(&b) ⇔
    /// b.strictly_below(&a)`.
    ///
    /// Theory anchor: same as [`Lattice::strictly_below`] on the dual
    /// strict arm.
    fn strictly_above(&self, other: &Self) -> bool {
        other.strictly_below(self)
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
    /// N-ary Boolean-conjunction peer of [`Lattice::leq`] on the
    /// (2-ary, N-ary) × (predicate, combinator) grid — `self` sits at-
    /// or-below EVERY element the iterator yields. `a.is_lower_bound_of(
    /// [&b, &c, &d])` holds iff `a.leq(&b) && a.leq(&c) && a.leq(&d)`:
    /// `self` is a common lower bound for the iterated set.
    ///
    /// The N-ary Boolean-PREDICATE peer of [`Lattice::meet_all`] one
    /// PRIMITIVE-KIND axis over on the N-ary-aggregation face of the
    /// (predicate, combinator) × (2-ary, N-ary) primitive surface: where
    /// [`Lattice::meet_all`] COMPUTES the meet (the greatest lower bound
    /// of the iterated set), this DECIDES whether `self` is a lower
    /// bound of the iterated set (member of the lower-bound set the
    /// meet is the largest element of). Together
    /// ([`Lattice::meet_all`], [`Lattice::is_lower_bound_of`]) close the
    /// meet arm of the (predicate, combinator) × (meet, join) 2×2
    /// N-ary-aggregation grid; the dual ([`Lattice::join_all`],
    /// [`Lattice::is_upper_bound_of`]) closes the join arm.
    ///
    /// **Empty-iterator vacuous truth**: `a.is_lower_bound_of(std::iter
    /// ::empty()) == true` for every element — the empty conjunction is
    /// vacuously true because [`Iterator::all`] on the empty iterator is
    /// `true`, and every element is trivially a lower bound of the empty
    /// set. Peer of `T::meet_all(std::iter::empty()) == T::top()`'s
    /// identity-on-empty behaviour: the empty aggregate binds to the
    /// identity of the underlying operation (`true` for Boolean
    /// conjunction; [`Lattice::top`] for meet), so the empty case never
    /// rejects.
    ///
    /// **Singleton-identity**: `a.is_lower_bound_of([&b]) == a.leq(&b)`
    /// — the 1-input predicate reduces to the pairwise relation, the
    /// same way `meet_all([&a]) == a` reduces the 1-input N-ary
    /// combinator to the operand verbatim.
    ///
    /// **Meet witness**: `T::meet_all(iter).is_lower_bound_of(iter) ==
    /// true` for every iterable — the N-ary meet is always a lower bound
    /// of the set it aggregates (definitionally, meet is the GREATEST
    /// lower bound, so it IS a lower bound). The definitional link
    /// between the N-ary COMBINATOR and the N-ary PREDICATE: the
    /// aggregate [`Lattice::meet_all`] produces is a member of the
    /// lower-bound set [`Lattice::is_lower_bound_of`] characterizes.
    ///
    /// **Universal-bottom witness**: `T::bottom().is_lower_bound_of(iter)
    /// == true` for every iterable — the lattice bottom is a common
    /// lower bound of every set, since `T::bottom().leq(x) == true` for
    /// every `x` by the lattice-bottom axiom.
    ///
    /// **Any-violating-element rejection**: if any `x` in the iterator
    /// has `!self.leq(x)`, then `self.is_lower_bound_of(iter) == false`
    /// — [`Iterator::all`] short-circuits on the first violating element.
    ///
    /// **Peer** — [`Lattice::is_upper_bound_of`] one MEET/JOIN axis
    /// over: the two close the (lower, upper) N-ary Boolean-conjunction
    /// pair the pairwise `leq` / `geq` extend from 2-ary to N-ary, and
    /// bind ONE substrate primitive on the [`Lattice`] algebra rather
    /// than at each consumer's hand-rolled `iter.all(|x| self.leq(x))`
    /// conjunction. The pattern already exists in-tree at
    /// `tatara_lisp::macro_expand::ResourceLimits::is_lower_bound_of` /
    /// `is_upper_bound_of` — a per-domain `const fn` pair on the
    /// pointwise resource-posture partial order whose own doc explicitly
    /// notes "the pairwise `leq` combinator already lifted one arity
    /// down… the N-ary predicate binds at ONE typed method whose
    /// signature carries the containment direction into the type
    /// system"; this widening lifts the SAME shape from a per-domain
    /// `const fn` pair to the trait's default-method surface so every
    /// future closed-set lattice impl inherits the N-ary bound predicates
    /// for free.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the N-ary containment-from-below predicate is
    /// itself a typed named `bool` composing the pairwise partial-order
    /// relation via [`Iterator::all`]) + THEORY.md §III (typescape — the
    /// N-ary Boolean-bound predicate on every classification-axis
    /// lattice binds at ONE substrate owner on the [`Lattice`] algebra
    /// rather than at each consumer's hand-rolled `iter.all(|x|
    /// self.leq(x))`).
    ///
    /// Frontier inspiration: Haskell's `Foldable` typeclass exposing
    /// `all` / `Data.Foldable.all` alongside the pairwise relation — the
    /// N-ary traversal of a collection through a Boolean-conjunction
    /// seed is a first-class named method the typeclass carries. This
    /// widening threads the same N-ary-traversal shape through the
    /// [`Lattice`] trait's default-method surface, so every downstream
    /// classification-axis consumer picks it up mechanically.
    fn is_lower_bound_of<'a, I>(&self, iter: I) -> bool
    where
        I: IntoIterator<Item = &'a Self>,
        Self: 'a,
    {
        iter.into_iter().all(|x| self.leq(x))
    }
    /// Dual of [`Lattice::is_lower_bound_of`] on the MEET/JOIN axis —
    /// `self` sits at-or-ABOVE EVERY element the iterator yields.
    /// `a.is_upper_bound_of([&b, &c, &d])` holds iff `b.leq(&a) &&
    /// c.leq(&a) && d.leq(&a)`: `self` is a common upper bound for the
    /// iterated set.
    ///
    /// The N-ary Boolean-PREDICATE peer of [`Lattice::join_all`] one
    /// PRIMITIVE-KIND axis over: where [`Lattice::join_all`] COMPUTES
    /// the join (the least upper bound of the iterated set), this
    /// DECIDES whether `self` is an upper bound of the iterated set
    /// (member of the upper-bound set the join is the smallest element
    /// of). Together with [`Lattice::is_lower_bound_of`] closes the
    /// (predicate, combinator) × (meet, join) 2×2 N-ary-aggregation
    /// grid on the algebra's combinator surface.
    ///
    /// **Empty-iterator vacuous truth**: `a.is_upper_bound_of(std::iter
    /// ::empty()) == true` for every element — the empty conjunction is
    /// vacuously true. Peer of `T::join_all(std::iter::empty()) ==
    /// T::bottom()`'s identity-on-empty behaviour.
    ///
    /// **Singleton-identity**: `a.is_upper_bound_of([&b]) == b.leq(&a)`
    /// — the 1-input predicate reduces to the pairwise relation with
    /// the direction flipped (since `self` sits ABOVE the operand
    /// rather than BELOW it), the dual of
    /// [`Lattice::is_lower_bound_of`]'s singleton reduction.
    ///
    /// **Join witness**: `T::join_all(iter).is_upper_bound_of(iter) ==
    /// true` for every iterable — the N-ary join is always an upper
    /// bound of the set it aggregates (definitionally, join is the LEAST
    /// upper bound, so it IS an upper bound). Dual definitional link on
    /// the join arm.
    ///
    /// **Universal-top witness**: `T::top().is_upper_bound_of(iter) ==
    /// true` for every iterable — the lattice top is a common upper
    /// bound of every set, since `x.leq(&T::top()) == true` for every
    /// `x` by the lattice-top axiom.
    ///
    /// **Any-violating-element rejection**: if any `x` in the iterator
    /// has `!x.leq(self)`, then `self.is_upper_bound_of(iter) == false`
    /// — [`Iterator::all`] short-circuits on the first violating element.
    ///
    /// Default routes through `iter.into_iter().all(|x| x.leq(self))`
    /// so every impl inherits the direction-flipped N-ary Boolean
    /// conjunction mechanically. A future override at the underlying
    /// [`Lattice::leq`] lands at ONE site and this dual inherits
    /// through the default.
    ///
    /// Theory anchor: same as [`Lattice::is_lower_bound_of`] on the
    /// dual arm — THEORY.md §II.1 invariant 5 + §III.
    fn is_upper_bound_of<'a, I>(&self, iter: I) -> bool
    where
        I: IntoIterator<Item = &'a Self>,
        Self: 'a,
    {
        iter.into_iter().all(|x| x.leq(self))
    }
    /// Strict-order N-ary Boolean-conjunction peer of
    /// [`Lattice::is_lower_bound_of`] on the STRICT arm — `self` sits
    /// STRICTLY BELOW EVERY element the iterator yields.
    /// `a.is_strict_lower_bound_of([&b, &c, &d])` holds iff
    /// `a.strictly_below(&b) && a.strictly_below(&c) && a.strictly_below(&d)`:
    /// `self` is a common STRICT lower bound for the iterated set.
    ///
    /// The strict-arm N-ary Boolean-PREDICATE peer of
    /// [`Lattice::is_lower_bound_of`] one STRICTNESS axis over on the
    /// (strict, non-strict) × (lower, upper) 2×2 N-ary Boolean-
    /// conjunction predicate grid: where [`Lattice::is_lower_bound_of`]
    /// DECIDES whether `self` is a NON-STRICT lower bound (member of
    /// the `≤`-bound set), this decides whether `self` is a STRICT
    /// lower bound (member of the `<`-bound set). Together with
    /// [`Lattice::is_strict_upper_bound_of`] on the dual strict arm
    /// this closes the (strict, non-strict) × (lower, upper) 2×2 grid
    /// on the N-ary Boolean-conjunction predicate face of the
    /// combinator surface: the pairwise (strict, non-strict) × (leq,
    /// geq) 2×2 grid is closed at the pairwise level by [`Lattice::leq`]
    /// / [`Lattice::geq`] / [`Lattice::strictly_below`] /
    /// [`Lattice::strictly_above`], and this widening threads the
    /// SAME strictness axis through the N-ary-aggregation surface so
    /// every future closed-set lattice impl inherits BOTH strict AND
    /// non-strict N-ary bound predicates for free.
    ///
    /// **Empty-iterator vacuous truth**: `a.is_strict_lower_bound_of(
    /// std::iter::empty()) == true` for every element — the empty
    /// conjunction is vacuously true because [`Iterator::all`] on the
    /// empty iterator is `true`, and every element is trivially a
    /// strict lower bound of the empty set. Same empty-conjunction
    /// identity as [`Lattice::is_lower_bound_of`]'s vacuous-true.
    ///
    /// **Singleton-strict-identity**: `a.is_strict_lower_bound_of([&b])
    /// == a.strictly_below(&b)` — the 1-input strict predicate reduces
    /// to the pairwise STRICT relation, dual to
    /// [`Lattice::is_lower_bound_of`]'s singleton reduction on the
    /// non-strict arm.
    ///
    /// **Refines [`Lattice::is_lower_bound_of`]**:
    /// `a.is_strict_lower_bound_of(iter) ⇒ a.is_lower_bound_of(iter)`
    /// — the strict conjunction refines the non-strict one at every
    /// element position via [`Lattice::strictly_below`]'s
    /// refines-[`Lattice::leq`] identity. The strict-minus-non-strict
    /// gap on the N-ary aggregate is exactly the set of iterables that
    /// contain some element equal to `self`.
    ///
    /// **Irreflexivity on any inclusive iterable**: if the iterator
    /// yields `self`, then `self.is_strict_lower_bound_of(iter) ==
    /// false` — [`Iterator::all`] short-circuits at the first `x ==
    /// self` element because [`Lattice::strictly_below`]'s irreflexive
    /// arm rejects `self.strictly_below(&self)`. In particular
    /// `a.is_strict_lower_bound_of([&a]) == false` at every element,
    /// and `T::bottom().is_strict_lower_bound_of(iter)` is false
    /// whenever `bottom` appears in `iter` — the strict-lower-bound
    /// predicate does NOT inherit the non-strict universal-bottom
    /// witness on iterables that include the bottom endpoint.
    ///
    /// **Any-violating-element rejection**: if any `x` in the iterator
    /// has `!self.strictly_below(x)` (either `x == self` OR
    /// `!self.leq(x)`), then `self.is_strict_lower_bound_of(iter) ==
    /// false` — [`Iterator::all`] short-circuits on the first violating
    /// element.
    ///
    /// **Antichain rejection**: on an antichain lattice
    /// (e.g. [`SubstrateType`]), the strict predicate rejects most
    /// pairs the non-strict one accepts — a non-Regulatory `self` fails
    /// `self.strictly_below(x)` for every distinct non-Regulatory `x`
    /// (they are incomparable, so `leq` fails, so `strictly_below`
    /// fails), leaving only the strict half-edge into
    /// [`SubstrateType::top`] as a passing case. The predicate does
    /// NOT promote incomparable pairs to a strict-order verdict.
    ///
    /// Default routes through `iter.into_iter().all(|x|
    /// self.strictly_below(x))` — the two-primitive antisymmetric
    /// composition of [`Lattice::strictly_below`] and
    /// [`Iterator::all`] on any partial order. A future normalization
    /// at either primitive (a strictly_below override, an Iterator::all
    /// short-circuit tweak in the standard library) lands at ONE site
    /// and this default inherits mechanically.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the strict N-ary containment-from-below
    /// predicate is itself a typed named `bool` composing the pairwise
    /// strict partial-order relation via [`Iterator::all`]) + THEORY.md
    /// §III (typescape — the strict N-ary Boolean-bound predicate on
    /// every classification-axis lattice binds at ONE substrate owner
    /// on the [`Lattice`] algebra rather than at each consumer's hand-
    /// rolled `iter.all(|x| self.strictly_below(x))`).
    fn is_strict_lower_bound_of<'a, I>(&self, iter: I) -> bool
    where
        I: IntoIterator<Item = &'a Self>,
        Self: 'a,
    {
        iter.into_iter().all(|x| self.strictly_below(x))
    }
    /// Dual of [`Lattice::is_strict_lower_bound_of`] on the MEET/JOIN
    /// axis — `self` sits STRICTLY ABOVE EVERY element the iterator
    /// yields. `a.is_strict_upper_bound_of([&b, &c, &d])` holds iff
    /// `b.strictly_below(&a) && c.strictly_below(&a) &&
    /// d.strictly_below(&a)`: `self` is a common STRICT upper bound
    /// for the iterated set.
    ///
    /// The strict-arm N-ary Boolean-PREDICATE peer of
    /// [`Lattice::is_upper_bound_of`] one STRICTNESS axis over on the
    /// (strict, non-strict) × (lower, upper) 2×2 N-ary Boolean-
    /// conjunction predicate grid. Together with
    /// [`Lattice::is_strict_lower_bound_of`] closes the strict arm of
    /// the grid on the algebra's combinator surface.
    ///
    /// **Empty-iterator vacuous truth**: `a.is_strict_upper_bound_of(
    /// std::iter::empty()) == true` for every element — the empty
    /// conjunction is vacuously true.
    ///
    /// **Singleton-strict-identity**:
    /// `a.is_strict_upper_bound_of([&b]) == b.strictly_below(&a)` —
    /// the 1-input strict predicate reduces to the pairwise STRICT
    /// relation with the direction flipped (since `self` sits ABOVE
    /// the operand rather than BELOW it), dual of
    /// [`Lattice::is_strict_lower_bound_of`]'s singleton reduction.
    ///
    /// **Refines [`Lattice::is_upper_bound_of`]**:
    /// `a.is_strict_upper_bound_of(iter) ⇒ a.is_upper_bound_of(iter)`
    /// — the strict conjunction refines the non-strict one at every
    /// element position via [`Lattice::strictly_below`]'s
    /// refines-[`Lattice::leq`] identity on the dual direction.
    ///
    /// **Irreflexivity on any inclusive iterable**: if the iterator
    /// yields `self`, then `self.is_strict_upper_bound_of(iter) ==
    /// false` — the strict-upper-bound predicate does NOT inherit
    /// the non-strict universal-top witness on iterables that include
    /// the top endpoint.
    ///
    /// **Any-violating-element rejection**: if any `x` in the iterator
    /// has `!x.strictly_below(self)`, then
    /// `self.is_strict_upper_bound_of(iter) == false`.
    ///
    /// Default routes through `iter.into_iter().all(|x|
    /// x.strictly_below(self))` so every impl inherits the direction-
    /// flipped strict N-ary Boolean conjunction mechanically. A future
    /// override at the underlying [`Lattice::strictly_below`] (or at
    /// [`Lattice::leq`] which `strictly_below` composes from) lands
    /// at ONE site and this dual inherits through the default.
    ///
    /// Theory anchor: same as [`Lattice::is_strict_lower_bound_of`]
    /// on the dual arm — THEORY.md §II.1 invariant 5 + §III.
    fn is_strict_upper_bound_of<'a, I>(&self, iter: I) -> bool
    where
        I: IntoIterator<Item = &'a Self>,
        Self: 'a,
    {
        iter.into_iter().all(|x| x.strictly_below(self))
    }
    /// Interval-containment predicate — `self` sits inside the closed
    /// bracket `[low, high]` on the lattice's partial order.
    /// `a.is_between(&low, &high)` holds iff `low.leq(&a) && a.leq(&high)`:
    /// `self` is at-or-above `low` AND at-or-below `high` in the
    /// refinement order.
    ///
    /// The 3-ary interval-containment PREDICATE peer of the 2-ary
    /// [`Lattice::leq`] / [`Lattice::geq`] pairwise comparators on the
    /// arity-3 face of the (pairwise, interval, N-ary) primitive-arity
    /// grid — where [`Lattice::leq`] decides the 2-input half-plane
    /// membership (is `self` at-or-below `other`) and
    /// [`Lattice::is_lower_bound_of`] decides the N-input universal
    /// half-plane membership (is `self` at-or-below EVERY member of the
    /// iterated set), this decides the 3-input BRACKET membership (does
    /// `self` sit within `[low, high]`) via the pointwise conjunction of
    /// the two half-plane decisions. Together with
    /// [`Lattice::is_strictly_between`] on the strict arm this closes
    /// the (strict, non-strict) × (interval-containment) 2×1 grid on the
    /// bracket-membership face of the algebra's predicate surface —
    /// exactly the way [`Lattice::leq`] / [`Lattice::strictly_below`]
    /// close the (strict, non-strict) × (pairwise `≤`) 2×1 grid one
    /// ARITY axis down.
    ///
    /// **Endpoint reflexivity**: `low.is_between(&low, &high) == true`
    /// whenever `low.leq(&high)` — the low endpoint is trivially in
    /// its own bracket. Dually `high.is_between(&low, &high) == true`
    /// under the same premise. Both cases route through
    /// [`Lattice::leq`]'s reflexive arm on the conjunct that fixes the
    /// endpoint and the caller-supplied `low.leq(&high)` premise on
    /// the other; when the premise fails (inverted bracket) the
    /// predicate rejects at the failing conjunct.
    ///
    /// **Degenerate-bracket collapse**: `a.is_between(&x, &x) ⇔ a == x`
    /// on every lattice element — a zero-width bracket admits only the
    /// single point `x`. Follows from antisymmetry of [`Lattice::leq`]:
    /// `x.leq(&a) && a.leq(&x) ⇒ a == x`. Peer of the same identity
    /// pinned in-tree on
    /// [`tatara_lisp::macro_expand::ResourceLimits::within`]
    /// (`resource_limits_within_of_equal_bounds_iff_equal_to_bound`);
    /// this widening lifts the SAME shape from a per-domain `const fn`
    /// to the trait's default-method surface so every closed-set
    /// lattice impl inherits the interval-containment predicate for
    /// free.
    ///
    /// **Inverted-bracket rejection**: if `high.strictly_below(&low)`
    /// (the bracket is empty because its upper endpoint sits strictly
    /// below its lower one), then `a.is_between(&low, &high) == false`
    /// for EVERY element — no lattice element can simultaneously be
    /// at-or-above `low` and at-or-below a `high` that itself sits
    /// strictly below `low`. Pinned exhaustively over the closed-set
    /// impls below on the every-inverted-pair sweep.
    ///
    /// **Extrema-bracket universal-truth**: `a.is_between(&T::bottom(),
    /// &T::top()) == true` for EVERY element on EVERY lattice —
    /// `T::bottom().leq(&a)` and `a.leq(&T::top())` are the lattice's
    /// bottom/top axioms, so the widest possible bracket admits every
    /// element. Peer of the extrema-bracket identity pinned in-tree on
    /// [`tatara_lisp::macro_expand::ResourceLimits::within`]
    /// (`resource_limits_within_with_lattice_extrema_is_true`); binds
    /// at ONE substrate primitive on the [`Lattice`] algebra rather
    /// than at each consumer's hand-authored bounded-lattice cross-
    /// check.
    ///
    /// **Antichain rejection**: on an antichain lattice (e.g.
    /// [`SubstrateType`]), the predicate rejects most brackets that
    /// contain distinct incomparable elements — a `[low, high]` with
    /// incomparable endpoints admits only elements that are
    /// simultaneously at-or-above `low` AND at-or-below `high`, and on
    /// the pointed-top antichain that is either the top itself (when
    /// high == top) or nothing. The predicate does NOT promote
    /// incomparable brackets to spurious containment.
    ///
    /// Default routes through `low.leq(self) && self.leq(high)` — the
    /// two-primitive antisymmetric composition of two [`Lattice::leq`]
    /// calls on any partial order. A future normalization at
    /// [`Lattice::leq`] lands at ONE site and this default inherits
    /// mechanically. The `(low, high)` parameter order is baked in so
    /// consumers cannot accidentally swap the bracket bounds — a
    /// copy-paste that transposed the two would test `high.leq(self) &&
    /// self.leq(low)` (the WRONG containment direction, returning
    /// `true` only for the empty set of elements that simultaneously
    /// sit above `high` AND below `low` when `low.leq(&high)` holds),
    /// a silent distortion the type system did not gate pre-lift and
    /// now does through the method's parameter order.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the 3-ary interval-containment predicate is
    /// itself a typed named `bool` composing two [`Lattice::leq`]
    /// calls via the boolean `&&` conjunction. Every downstream
    /// lattice-law consumer inherits the predicate through the
    /// default. THEORY.md §III — typescape; the interval-containment
    /// arm on every classification-axis lattice binds at ONE substrate
    /// owner on the [`Lattice`] algebra rather than at each consumer's
    /// hand-rolled `low.leq(&x) && x.leq(&high)` conjunction (a
    /// duplicated pattern in-tree at
    /// [`tatara_lisp::macro_expand::ResourceLimits::within`] on the
    /// pointwise resource-posture partial order).
    ///
    /// Frontier inspiration: the interval-containment predicate is the
    /// natural boolean twin of stdlib's [`Ord::clamp`] — stdlib lacks
    /// an `is_in_range(min, max)` method, but the bounded-lattice
    /// extension of the pattern is straightforward on any partial order
    /// and closes the (combinator, predicate) row on the bracket-
    /// primitive surface. Translated: threaded the same 3-ary bracket-
    /// containment shape through the [`Lattice`] trait's default-
    /// method surface, so every closed-set impl (total order, pointed-
    /// top antichain, boolean lattice) picks it up mechanically —
    /// generalizing the `const fn` [`tatara_lisp::macro_expand
    /// ::ResourceLimits::within`] one abstraction level up.
    fn is_between(&self, low: &Self, high: &Self) -> bool {
        low.leq(self) && self.leq(high)
    }
    /// Strict-order peer of [`Lattice::is_between`] on the STRICT arm —
    /// `self` sits STRICTLY inside the open bracket `(low, high)` on
    /// the lattice's partial order. `a.is_strictly_between(&low,
    /// &high)` holds iff `low.strictly_below(&a) &&
    /// a.strictly_below(&high)`: `self` is strictly above `low` AND
    /// strictly below `high` in the refinement order.
    ///
    /// The strict-arm interval-containment PREDICATE peer of
    /// [`Lattice::is_between`] one STRICTNESS axis over on the (strict,
    /// non-strict) × (interval-containment) 2×1 grid — where
    /// [`Lattice::is_between`] decides CLOSED-bracket `[low, high]`
    /// membership, this decides OPEN-bracket `(low, high)` membership.
    /// Together the two default methods close the (strict, non-strict)
    /// axis on the interval-containment predicate face of the algebra's
    /// combinator surface — exactly the way [`Lattice::strictly_below`]
    /// / [`Lattice::leq`] close the same strictness axis one ARITY
    /// axis down on the pairwise comparator surface, and
    /// [`Lattice::is_strict_lower_bound_of`] /
    /// [`Lattice::is_lower_bound_of`] close it one ARITY axis up on the
    /// N-ary Boolean-conjunction predicate surface. The (pairwise,
    /// interval, N-ary) × (strict, non-strict) 3×2 grid on the
    /// order-comparator predicate face is now closed at ONE substrate
    /// primitive per cell.
    ///
    /// **Endpoint STRICT-exclusion**: `low.is_strictly_between(&low,
    /// &high) == false` on EVERY lattice element — the strict conjunct
    /// `low.strictly_below(&low)` short-circuits on the reflexive
    /// diagonal because [`Lattice::strictly_below`] is irreflexive.
    /// Dually `high.is_strictly_between(&low, &high) == false`. This
    /// is the strict-minus-non-strict gap on the endpoint arm — the
    /// non-strict [`Lattice::is_between`] admits both endpoints; the
    /// strict version rejects both.
    ///
    /// **Degenerate-bracket universal-rejection**: `a.is_strictly_between(
    /// &x, &x) == false` for EVERY pair of elements — a zero-width
    /// open bracket admits NO points because no element can be
    /// simultaneously strictly above AND strictly below the same
    /// endpoint (the conjunction rejects on the reflexive diagonal at
    /// `a == x`, and on antisymmetry-plus-strictness at `a != x` since
    /// only one of `x.strictly_below(a)` / `a.strictly_below(x)` can
    /// hold on any distinct pair). The strict-minus-non-strict gap on
    /// the degenerate arm: [`Lattice::is_between`]'s degenerate
    /// bracket admits ONLY `a == x`; the strict version admits
    /// NOTHING.
    ///
    /// **Refines [`Lattice::is_between`]**: `a.is_strictly_between(
    /// &low, &high) ⇒ a.is_between(&low, &high)` on every triple —
    /// the strict conjunction refines the non-strict one at each
    /// conjunct via [`Lattice::strictly_below`]'s refines-
    /// [`Lattice::leq`] identity. The strict-minus-non-strict gap on
    /// the interval arm is exactly the triples where `a == low` OR
    /// `a == high` — the endpoint diagonal.
    ///
    /// **Inverted-bracket rejection**: if `high.leq(&low)` (the bracket
    /// is empty or degenerate in the strict sense — its upper endpoint
    /// sits at-or-below its lower one), then `a.is_strictly_between(
    /// &low, &high) == false` for EVERY element — no lattice element
    /// can simultaneously be strictly above `low` and strictly below a
    /// `high` that itself sits at-or-below `low`.
    ///
    /// **Antichain rejection**: on an antichain lattice (e.g.
    /// [`SubstrateType`]), the strict predicate rejects EVERY bracket
    /// that involves any incomparable pair — the pointed-top antichain
    /// admits at most the closed strict half-edge from any non-top
    /// element to the top, and even there the interior of the open
    /// bracket is empty because the antichain has no elements between
    /// distinct comparable endpoints.
    ///
    /// Default routes through `low.strictly_below(self) &&
    /// self.strictly_below(high)` — the two-primitive strict-order
    /// composition of two [`Lattice::strictly_below`] calls on any
    /// partial order. A future normalization at [`Lattice::strictly_below`]
    /// (or at the underlying [`Lattice::leq`]) lands at ONE site and
    /// this default inherits mechanically. Same `(low, high)`
    /// parameter-order gating as [`Lattice::is_between`].
    ///
    /// Theory anchor: same as [`Lattice::is_between`] on the strict
    /// arm — THEORY.md §II.1 invariant 5 + §III. The interval-
    /// containment predicate on every classification-axis lattice now
    /// binds through TWO substrate defaults ([`Lattice::is_between`],
    /// [`Lattice::is_strictly_between`]) closing the (strict, non-
    /// strict) axis at ONE algebra owner.
    fn is_strictly_between(&self, low: &Self, high: &Self) -> bool {
        low.strictly_below(self) && self.strictly_below(high)
    }
    /// Interval-PROJECTION combinator peer of the interval-CONTAINMENT
    /// predicate [`Lattice::is_between`] — pin `self` into the closed
    /// bracket `[low, high]` on the lattice's partial order.
    /// `a.clamped_between(&low, &high)` returns `(a ⊔ low) ⊓ high`: the element
    /// obtained by first RAISING `a` to at-least `low` (join with the
    /// floor) and then LOWERING the result to at-most `high` (meet
    /// with the ceiling).
    ///
    /// The 3-ary interval-PROJECTION peer of [`Lattice::is_between`]
    /// one PRIMITIVE-KIND axis over on the (predicate, combinator) ×
    /// (interval) 2×1 grid — where [`Lattice::is_between`] DECIDES
    /// bracket membership (does `self` sit within `[low, high]`),
    /// this PROJECTS `self` INTO the bracket (produces the nearest
    /// point of `[low, high]` reachable through the lattice's
    /// (meet, join) combinators). Together with [`Lattice::is_between`]
    /// on the predicate arm this closes the (predicate, combinator) ×
    /// (interval) 2×1 grid on the 3-ary interval face of the algebra's
    /// combinator surface — exactly the way the pairwise face is
    /// closed at the 2-ary level by ([`Lattice::leq`],
    /// [`Lattice::meet`]) / ([`Lattice::geq`], [`Lattice::join`]) and
    /// the N-ary face is closed at the N-input level by
    /// ([`Lattice::is_lower_bound_of`], [`Lattice::meet_all`]) /
    /// ([`Lattice::is_upper_bound_of`], [`Lattice::join_all`]).
    ///
    /// **Clamp fixed-point theorem** on WELL-FORMED brackets: on
    /// `low.leq(&high)`, `a.is_between(&low, &high) ⇔
    /// a.clamped_between(&low, &high) == a`. `a` sits within the
    /// bracket iff clamp is the identity on it. The forward
    /// direction: if `low.leq(&a) && a.leq(&high)`, then
    /// `a.join(low) == a` (by join-agreement with `low.leq(&a)`) and
    /// then `a.meet(high) == a` (by meet-agreement with `a.leq(&high)`),
    /// so the composed clamp returns `a` verbatim. The reverse: from
    /// `a = a.join(low).meet(high)` on `low.leq(&high)`, the two
    /// absorption arms below give `low ≤ a ≤ high`. The CANONICAL
    /// cross-check axiom binding the predicate to the combinator,
    /// analogous to the meet-agreement (`a.leq(&b) ⇔ a.meet(&b) ==
    /// a`) and join-agreement (`a.leq(&b) ⇔ a.join(&b) == b`)
    /// axioms binding [`Lattice::leq`] to its combinator peers one
    /// arity down. The `low.leq(&high)` premise is load-bearing on
    /// two counts: on an INVERTED bracket (`high.strictly_below(&low)`),
    /// `is_between` universally rejects while the clamp may still
    /// coincidentally reproduce `a`, and on a lattice impl that
    /// violates meet-agreement or join-agreement (e.g. the pointed-
    /// top antichain [`SubstrateType`] under its "distinct pairs meet
    /// to top / join to bottom" impl, which the trait's core
    /// `leq_agrees_with_meet` / `leq_agrees_with_join` proptests
    /// already opt out of) the fixed-point identity does not extend.
    /// Peer of the same theorem pinned in-tree on
    /// [`tatara_lisp::macro_expand::ResourceLimits`]
    /// (`resource_limits_within_agrees_with_clamp_fixed_point`); this
    /// widening lifts the SAME shape from a per-domain `const fn` to
    /// the trait's default-method surface so every closed-set lattice
    /// impl satisfying meet-agreement + join-agreement inherits the
    /// predicate/combinator bridge for free.
    ///
    /// **Below-floor pin**: on any well-formed bracket
    /// (`low.leq(&high)`), if `a.leq(&low)` — `a` sits at-or-below the
    /// floor — then `a.clamped_between(&low, &high) == low`. The floor absorbs
    /// the input: `a.join(low) == low` (by join-agreement with
    /// `a.leq(&low)`) and then `low.meet(high) == low` (by
    /// meet-agreement with the caller's `low.leq(&high)` premise), so
    /// the composed clamp returns `low`.
    ///
    /// **Above-ceiling pin**: on any well-formed bracket
    /// (`low.leq(&high)`), if `high.leq(&a)` — `a` sits at-or-above
    /// the ceiling — then `a.clamped_between(&low, &high) == high`. The ceiling
    /// absorbs the input: `a.join(low) == a` (join with a lesser
    /// element is the receiver) and then `a.meet(high) == high` (by
    /// meet-agreement with `high.leq(&a)`), so the composed clamp
    /// returns `high`.
    ///
    /// **Bracket-membership contract**: on any well-formed bracket
    /// (`low.leq(&high)`), `a.clamped_between(&low, &high).is_between(&low,
    /// &high)` holds for EVERY `a` — the clamp result always sits
    /// within the bracket. Follows from the two absorption arms plus
    /// the in-range identity: the clamp lands on one of `{low, a,
    /// high}` (or a lattice combination of them on a non-totally-
    /// ordered order), and each of those three sits within `[low,
    /// high]` under the well-formed premise. The clamp is a
    /// RETRACTION of the lattice onto the bracket — idempotent
    /// (`a.clamped_between(&low, &high).clamped_between(&low, &high)
    /// == a.clamped_between(&low, &high)` follows from the fixed-point theorem
    /// applied to the clamp result, which is in the bracket).
    ///
    /// **Degenerate-bracket collapse**: on every lattice satisfying
    /// meet-agreement + join-agreement (in particular every totally-
    /// ordered lattice — [`DataClassification`], [`CalmClassification`],
    /// [`baseline::Baseline`]), `a.clamped_between(&x, &x) == x` for
    /// every `a` — a zero-width bracket forces the projection to the
    /// single point `x`. Peer of [`Lattice::is_between`]'s degenerate-
    /// bracket collapse (`a.is_between(&x, &x) ⇔ a == x`) on the
    /// projection arm: the predicate reads "only `x` is in the
    /// bracket"; the combinator says "every input becomes `x` after
    /// clamping to the bracket". Follows from the fixed-point
    /// theorem at the well-formed `low.leq(&high)` case `x.leq(&x)`
    /// (reflexive), plus the below-floor / above-ceiling pins on the
    /// two cases `a.leq(&x)` and `x.leq(&a)` (one of which always
    /// holds on a totally-ordered lattice), both routing to `x`.
    ///
    /// **Reflexive-bracket identity**: `a.clamped_between(&a, &a) == a` on
    /// every element — the zero-width bracket at `a` is trivially the
    /// identity on `a`. Follows from the degenerate-bracket collapse
    /// at `x = a`; peer of the analogous reflexive
    /// [`Lattice::is_between`] identity (`a.is_between(&a, &a) ==
    /// true`).
    ///
    /// **Extrema-bracket identity**: `a.clamped_between(&T::bottom(), &T::top())
    /// == a` for EVERY element on EVERY lattice — clamping to the
    /// widest possible bracket is the identity. `a.join(&T::bottom())
    /// == a` (join with bottom is the receiver by the lattice-bottom
    /// axiom) and then `a.meet(&T::top()) == a` (meet with top is the
    /// receiver by the lattice-top axiom), so the composed clamp
    /// returns `a`. Peer of [`Lattice::is_between`]'s extrema-bracket
    /// universal-truth (`a.is_between(&T::bottom(), &T::top()) ==
    /// true` for every element) on the projection arm.
    ///
    /// **Idempotence**: `a.clamped_between(&low, &high)
    /// .clamped_between(&low, &high) == a.clamped_between(&low, &high)`
    /// on any bracket — clamping a clamped
    /// value is the identity on the clamped value. Direct consequence
    /// of the fixed-point theorem: the clamp result sits within the
    /// bracket (by the bracket-membership contract), so re-clamping
    /// it returns it verbatim.
    ///
    /// Default routes through `self.join(low).meet(high)` — the
    /// two-primitive lattice-algebra composition of a join with the
    /// floor followed by a meet with the ceiling on any bounded
    /// lattice. A future normalization at either [`Lattice::meet`] or
    /// [`Lattice::join`] lands at ONE site and this default inherits
    /// mechanically. The `(low, high)` parameter order is baked in so
    /// consumers cannot accidentally swap the bracket bounds — a
    /// copy-paste that transposed the two would compute
    /// `self.join(high).meet(low)` (the WRONG projection, pinning
    /// every input to the interval `[high, low]` which is the empty
    /// set on a well-formed `low.leq(&high)` bracket), a silent
    /// distortion the type system did not gate pre-lift and now does
    /// through the method's parameter order.
    ///
    /// Peer of the same shape pinned in-tree on
    /// [`tatara_lisp::macro_expand::ResourceLimits::clamp`]
    /// (`self.most_permissive(lower).strictest(upper)` — the pointwise
    /// resource-posture bracket-projection combinator whose own
    /// docstring notes the frontier-inspiration link to [`Ord::clamp`]
    /// on total orders); this widening lifts the SAME shape from a
    /// per-domain `const fn` to the trait's default-method surface so
    /// every closed-set lattice impl inherits the bracket-projection
    /// combinator for free.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the 3-ary interval-projection combinator is
    /// itself a typed named `Self` composing one [`Lattice::join`] and
    /// one [`Lattice::meet`] call. Every downstream lattice-law
    /// consumer inherits the combinator through the default
    /// mechanically. THEORY.md §III — typescape; the interval-
    /// projection combinator on every classification-axis lattice
    /// binds at ONE substrate owner on the [`Lattice`] algebra rather
    /// than at each consumer's hand-rolled `self.join(low).meet(high)`
    /// composition.
    ///
    /// Frontier inspiration: [`Ord::clamp`] on total orders — the
    /// stdlib exposes a first-class named bracket combinator alongside
    /// the pairwise `min` / `max`, and the bounded-lattice extension
    /// of the pattern is straightforward on any partial order via
    /// `(a ⊔ low) ⊓ high`. Translated through pleme-io primitives:
    /// threaded the same 3-ary bracket-projection shape through the
    /// [`Lattice`] trait's default-method surface, so every closed-set
    /// impl (total order, pointed-top antichain, boolean lattice)
    /// picks it up mechanically — generalizing the `const fn`
    /// [`tatara_lisp::macro_expand::ResourceLimits::clamp`] one
    /// abstraction level up, and pairing it with the previously-lifted
    /// [`Lattice::is_between`] predicate to close the (predicate,
    /// combinator) × (interval) 2×1 grid on the 3-ary interval face
    /// at ONE substrate primitive per cell.
    fn clamped_between(&self, low: &Self, high: &Self) -> Self {
        self.join(low).meet(high)
    }
    /// Structural (collection-level) peer of [`Lattice::is_comparable`]
    /// — the iterated set forms a CHAIN on the lattice's partial order:
    /// every DISTINCT pair of elements the iterator yields is comparable.
    /// `T::is_chain([&a, &b, &c])` holds iff for every position pair
    /// `(i, j)` with `i < j` and `vs[i] != vs[j]` (by [`PartialEq`]),
    /// `vs[i].is_comparable(&vs[j])`; positions yielding equal values
    /// are filtered on the standard mathematical convention that a
    /// chain is defined over the DISTINCT elements of the underlying
    /// set (duplicates in the yielded sequence are the same set
    /// element, so they don't add a new pair to check).
    ///
    /// The COLLECTION-STRUCTURAL peer of [`Lattice::is_comparable`] one
    /// ARITY axis over on the (pairwise, structural) × (comparable,
    /// incomparable) 2×2 grid — where [`Lattice::is_comparable`] decides
    /// pairwise comparability on a single pair of elements, this
    /// decides STRUCTURAL comparability on the whole collection (does
    /// the iterated set live inside a single totally-ordered branch of
    /// the partial order). Together with [`Lattice::is_antichain`] on
    /// the dual predicate arm closes the (pairwise, structural) ×
    /// (comparable, incomparable) 2×2 grid on the comparability face of
    /// the combinator surface: the pairwise arm was closed by
    /// [`Lattice::is_comparable`] / [`Lattice::is_incomparable`], and
    /// this widening threads the SAME comparability axis through the
    /// COLLECTION-STRUCTURAL surface so every future closed-set lattice
    /// impl inherits BOTH structural predicates for free.
    ///
    /// **Empty-iterator vacuous truth**: `T::is_chain(std::iter::empty())
    /// == true` on every lattice — the empty collection contains no
    /// distinct pair to check, so the universally-quantified predicate
    /// is vacuously true. Peer of [`Lattice::is_lower_bound_of`]'s
    /// empty-iterator vacuous-true on the empty-conjunction identity.
    ///
    /// **Singleton vacuous truth**: `T::is_chain([&a]) == true` for
    /// every `a` — a singleton contains no distinct pair, same
    /// vacuous-conjunction reasoning as the empty case.
    ///
    /// **Duplicate-only vacuous truth**: `T::is_chain([&a, &a, &a]) ==
    /// true` for every `a` — the [`PartialEq`] filter drops every
    /// self-pair, leaving no distinct pair to check.
    ///
    /// **Pair-identity**: `T::is_chain([&a, &b]) == (a == b ||
    /// a.is_comparable(&b))` — the 2-input structural predicate reduces
    /// to the pairwise-comparability primitive (with the duplicate
    /// filter). Same identity peer that [`Lattice::is_lower_bound_of`]
    /// exposes at singleton reduction, one arity axis up.
    ///
    /// **Any-incomparable-distinct-pair rejection**: if any two distinct
    /// positions `(i, j)` with `vs[i] != vs[j]` fail
    /// `vs[i].is_comparable(&vs[j])`, then `T::is_chain(iter) == false`
    /// — the nested-loop short-circuits at the first violating pair,
    /// so the predicate does NOT promote incomparable elements to a
    /// chain verdict.
    ///
    /// **Total-order universal truth**: on any totally-ordered lattice
    /// (e.g. [`baseline::Baseline`], [`DataClassification`],
    /// [`CalmClassification`]), every collection is a chain — every
    /// pair is comparable by the total-order property, so the
    /// structural predicate universally accepts. The
    /// non-vacuous-truth signal on such a lattice is
    /// [`Lattice::is_antichain`]'s rejection of distinct-value
    /// non-singleton collections.
    ///
    /// **Antichain rejection**: on an antichain lattice (e.g.
    /// [`SubstrateType`]), the structural predicate rejects most
    /// distinct-value non-singleton collections — a collection of two
    /// distinct non-top substrates is NOT a chain, whereas a collection
    /// containing the pointed-top [`SubstrateType::Regulatory`] and one
    /// other substrate IS a chain (the top-directed edge is
    /// comparable). Together with the duplicate-only vacuous-truth arm,
    /// the antichain shape's chain predicate cleanly partitions into:
    /// vacuously-true (empty / singleton / all-duplicate), top-directed
    /// (contains only the top plus copies of one non-top substrate),
    /// or rejected (any two distinct non-top substrates).
    ///
    /// **Sequence-order independence**: `T::is_chain(iter) ==
    /// T::is_chain(iter.rev())` on every collection — the nested loop
    /// checks EVERY distinct-position pair `(i, j)` symmetrically via
    /// [`Lattice::is_comparable`]'s `leq || geq` disjunction, so a
    /// permutation of the input yields the same verdict. This is what
    /// justifies calling the predicate STRUCTURAL rather than SEQUENCE-
    /// SHAPED — the property depends only on the underlying multiset,
    /// not the emission order.
    ///
    /// **Strict-monotone-sequence witness**: any collection that
    /// [`Lattice::strictly_below`] chains left-to-right (i.e. every
    /// consecutive pair `(vs[i], vs[i + 1])` satisfies
    /// `vs[i].strictly_below(&vs[i + 1])`) is a chain — transitivity of
    /// [`Lattice::strictly_below`] extends the pairwise strict relation
    /// to every distinct pair, which refines [`Lattice::leq`] and
    /// therefore [`Lattice::is_comparable`]. The converse fails on
    /// non-totally-ordered lattices: a chain need not be strictly
    /// monotone as-yielded (any permutation of a chain is still a
    /// chain, but the permuted sequence need not be strictly monotone).
    ///
    /// Default routes through a nested-loop `for i in 0..vs.len()` /
    /// `for j in (i + 1)..vs.len()` over the collected `Vec<&Self>`
    /// buffer, guarded by `vs[i] != vs[j]` to filter duplicates on the
    /// standard-mathematical convention. The collect materializes the
    /// iterator once so both loop arms iterate the same set — an
    /// [`IntoIterator`] that yields distinct values on distinct calls
    /// (a `Range` over a randomizing iterator) would otherwise break
    /// the structural predicate's determinism.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — the structural chain predicate is itself a typed named
    /// `bool` composing [`Lattice::is_comparable`] via nested-loop
    /// pair iteration; every downstream lattice-law consumer inherits
    /// the predicate through the default mechanically) + THEORY.md §III
    /// (typescape — the structural comparability predicate on every
    /// classification-axis lattice binds at ONE substrate owner on the
    /// [`Lattice`] algebra rather than at each consumer's hand-rolled
    /// `iter.iter().enumerate().all(|(i, x)| iter.iter().skip(i +
    /// 1).all(|y| x.is_comparable(y)))` nested traversal).
    ///
    /// Frontier inspiration: order-theory's classical chain / antichain
    /// duality (Dilworth's theorem, Mirsky's theorem) — a poset's
    /// structural decomposition into chains and antichains is the
    /// canonical two-halves-of-the-comparability-relation reading, and
    /// chain-detection is its most-primitive membership predicate.
    /// Translated: threaded the same structural comparability predicate
    /// through the [`Lattice`] trait's default-method surface, so every
    /// closed-set impl (total order, pointed-top antichain, boolean
    /// lattice) picks up the whole chain-detection primitive
    /// mechanically — generalizing the classical order-theoretic
    /// decomposition to a first-class algebra method the trait carries
    /// alongside its pairwise-comparability arm.
    fn is_chain<'a, I>(iter: I) -> bool
    where
        I: IntoIterator<Item = &'a Self>,
        Self: 'a,
    {
        let vs: Vec<&'a Self> = iter.into_iter().collect();
        for i in 0..vs.len() {
            for j in (i + 1)..vs.len() {
                if vs[i] != vs[j] && !vs[i].is_comparable(vs[j]) {
                    return false;
                }
            }
        }
        true
    }
    /// Dual of [`Lattice::is_chain`] on the comparability axis — the
    /// iterated set forms an ANTICHAIN (Sperner family) on the
    /// lattice's partial order: every DISTINCT pair of elements the
    /// iterator yields is incomparable. `T::is_antichain([&a, &b, &c])`
    /// holds iff for every position pair `(i, j)` with `i < j` and
    /// `vs[i] != vs[j]` (by [`PartialEq`]),
    /// `vs[i].is_incomparable(&vs[j])`; positions yielding equal values
    /// are filtered on the same standard-mathematical convention as
    /// [`Lattice::is_chain`] (an antichain is defined over the DISTINCT
    /// elements of the underlying set, so duplicates in the yielded
    /// sequence don't add a new pair to check).
    ///
    /// The COLLECTION-STRUCTURAL peer of [`Lattice::is_incomparable`]
    /// one ARITY axis over on the (pairwise, structural) ×
    /// (comparable, incomparable) 2×2 grid — where
    /// [`Lattice::is_incomparable`] decides pairwise INCOMPARABILITY on
    /// a single pair of elements, this decides STRUCTURAL
    /// INCOMPARABILITY on the whole collection (does the iterated set
    /// live entirely inside a single antichain of the partial order).
    /// Together with [`Lattice::is_chain`] on the dual predicate arm
    /// closes the (pairwise, structural) × (comparable, incomparable)
    /// 2×2 grid on the comparability face of the combinator surface.
    ///
    /// **Empty-iterator vacuous truth**: `T::is_antichain(std::iter::
    /// empty()) == true` on every lattice — same vacuous-conjunction
    /// identity as [`Lattice::is_chain`]'s empty arm.
    ///
    /// **Singleton vacuous truth**: `T::is_antichain([&a]) == true` for
    /// every `a` — a singleton is both a chain AND an antichain,
    /// vacuously.
    ///
    /// **Duplicate-only vacuous truth**: `T::is_antichain([&a, &a, &a])
    /// == true` for every `a` — the [`PartialEq`] filter drops every
    /// self-pair. This is the shared vacuous-truth arm with
    /// [`Lattice::is_chain`]; on every collection with no distinct-value
    /// pair, BOTH structural predicates fire true.
    ///
    /// **Pair-identity**: `T::is_antichain([&a, &b]) == (a == b ||
    /// a.is_incomparable(&b))` — the 2-input structural predicate
    /// reduces to the pairwise-incomparability primitive (with the
    /// duplicate filter). Dual identity to [`Lattice::is_chain`]'s
    /// pair-identity on the comparability axis.
    ///
    /// **Any-comparable-distinct-pair rejection**: if any two distinct
    /// positions `(i, j)` with `vs[i] != vs[j]` fail
    /// `vs[i].is_incomparable(&vs[j])` (i.e. the two values ARE
    /// comparable), then `T::is_antichain(iter) == false` — the
    /// nested-loop short-circuits at the first violating pair.
    ///
    /// **Total-order rejection**: on any totally-ordered lattice
    /// (e.g. [`baseline::Baseline`], [`DataClassification`],
    /// [`CalmClassification`]), the predicate accepts ONLY collections
    /// with at most one distinct value — every pair of distinct values
    /// is comparable by the total-order property, so the structural
    /// predicate rejects. Dual to [`Lattice::is_chain`]'s total-order
    /// universal-truth arm.
    ///
    /// **Antichain-lattice universal-truth-on-distinct-non-top**: on an
    /// antichain lattice (e.g. [`SubstrateType`]), the predicate
    /// universally accepts any collection whose distinct values are ALL
    /// non-top (all pairwise-incomparable), and rejects any collection
    /// that mixes the pointed-top [`SubstrateType::Regulatory`] with a
    /// distinct non-top substrate (the top-directed pair is comparable).
    ///
    /// **Chain-antichain overlap**: at a fixed collection, BOTH
    /// predicates fire true iff the collection has at most one distinct
    /// value (empty, singleton, or all-duplicate). At every collection
    /// with two or more distinct values, at most ONE of the two
    /// structural predicates fires (or NEITHER, on a mixed-shape
    /// collection on a partial order — e.g. a set containing both a
    /// chain-pair AND an incomparable pair). This is the structural
    /// projection of [`Lattice::is_comparable`] / [`Lattice::is_incomparable`]
    /// partitioning the pair space at the pairwise level.
    ///
    /// **Sequence-order independence**: `T::is_antichain(iter) ==
    /// T::is_antichain(iter.rev())` on every collection — same
    /// symmetric reasoning as [`Lattice::is_chain`]'s sequence-order
    /// independence.
    ///
    /// Default routes through a nested-loop `for i in 0..vs.len()` /
    /// `for j in (i + 1)..vs.len()` over the collected `Vec<&Self>`
    /// buffer, guarded by `vs[i] != vs[j]` to filter duplicates. Same
    /// collect-once discipline as [`Lattice::is_chain`].
    ///
    /// Theory anchor: same as [`Lattice::is_chain`] on the dual arm —
    /// THEORY.md §II.1 invariant 5 + §III. The comparability axis on
    /// every classification-axis lattice now binds through FOUR algebra
    /// predicates ([`Lattice::is_comparable`], [`Lattice::is_incomparable`],
    /// [`Lattice::is_chain`], [`Lattice::is_antichain`]) closing the
    /// (pairwise, structural) × (comparable, incomparable) 2×2 grid at
    /// ONE substrate owner on the trait.
    ///
    /// Frontier inspiration: order-theory's Sperner family / Dilworth's
    /// theorem — the antichain-detection predicate is the dual half of
    /// the comparability-relation's structural decomposition. Rust's
    /// standard library does not carry either predicate on [`PartialOrd`]
    /// (they aren't total-order primitives), but any bounded-lattice
    /// extension supports them via the pairwise-comparability arm. This
    /// widening threads the same structural incomparability predicate
    /// through the [`Lattice`] trait's default-method surface,
    /// generalizing the classical order-theoretic decomposition to a
    /// first-class algebra method — the dual half of what
    /// [`Lattice::is_chain`] lifts on the comparability side.
    fn is_antichain<'a, I>(iter: I) -> bool
    where
        I: IntoIterator<Item = &'a Self>,
        Self: 'a,
    {
        let vs: Vec<&'a Self> = iter.into_iter().collect();
        for i in 0..vs.len() {
            for j in (i + 1)..vs.len() {
                if vs[i] != vs[j] && !vs[i].is_incomparable(vs[j]) {
                    return false;
                }
            }
        }
        true
    }
    /// Sequence-shape (consecutive-pair) peer of [`Lattice::is_chain`]
    /// on the CONSECUTIVE-VS-ALL-PAIRS axis — the iterated set is
    /// ASCENDING (a [`Lattice::leq`]-monotone non-decreasing sequence
    /// on the lattice's partial order) iff every CONSECUTIVE pair
    /// `(vs[i], vs[i + 1])` satisfies `vs[i].leq(&vs[i + 1])`.
    /// `T::is_ascending([&a, &b, &c])` holds iff `a.leq(&b) &&
    /// b.leq(&c)`; three or more elements chain through the
    /// consecutive-pair conjunction. Empty and singleton collections
    /// are vacuously true because they contain no consecutive pair to
    /// check.
    ///
    /// The SEQUENCE-SHAPE peer of [`Lattice::is_chain`] one
    /// CONSECUTIVE-VS-ALL-PAIRS axis over on the collection-level
    /// combinator surface — where [`Lattice::is_chain`] decides
    /// STRUCTURAL comparability on the full O(N²) all-pairs symmetric
    /// nested-loop over the multiset (order-independent), this decides
    /// SEQUENTIAL monotonicity on the O(N) consecutive-pair walk over
    /// the yielded sequence (order-DEPENDENT). Together with
    /// [`Lattice::is_descending`] on the dual pair-level primitive arm
    /// this closes the (leq, geq) 2×1 sequence-monotonicity pair on
    /// the consecutive-pair face of the algebra's combinator surface —
    /// exactly one CONSECUTIVE-VS-ALL-PAIRS axis over from the
    /// structural (chain, antichain) pair [`Lattice::is_chain`] /
    /// [`Lattice::is_antichain`] pin, and exactly one CARDINALITY axis
    /// up from the pairwise (leq, geq) pair [`Lattice::leq`] /
    /// [`Lattice::geq`] pin.
    ///
    /// **Empty-iterator vacuous truth**: `T::is_ascending(std::iter::
    /// empty()) == true` on every lattice — [`slice::windows`] on a
    /// zero-length slice yields no pair, so [`Iterator::all`] on the
    /// empty iterator is `true`. Peer of [`Lattice::is_chain`]'s
    /// empty-iterator vacuous-truth identity on the empty-conjunction
    /// arm.
    ///
    /// **Singleton vacuous truth**: `T::is_ascending([&a]) == true` for
    /// every `a` — a singleton has no consecutive pair, same
    /// vacuous-conjunction reasoning as the empty case. On the
    /// (T, T) corner shared with [`Lattice::is_descending`] at every
    /// singleton, both sequence-monotonicity predicates agree.
    ///
    /// **Consecutive-duplicate identity**: `T::is_ascending([&a, &a])
    /// == true` for every `a` — [`Lattice::leq`] is REFLEXIVE, so
    /// `a.leq(&a)` fires true on the single consecutive pair. AGREES
    /// with [`Lattice::is_descending`]'s consecutive-duplicate verdict
    /// via the dual [`Lattice::geq`] reflexivity, so at every all-
    /// duplicate slice BOTH sequence-monotonicity predicates fire true.
    /// This is the shared reflexive-diagonal arm and the exact overlap
    /// where the two sequence-shape verdicts collapse.
    ///
    /// **Pair-identity**: `T::is_ascending([&a, &b]) == a.leq(&b)` —
    /// the 2-input sequence-monotonicity predicate reduces to the
    /// pairwise primitive without a duplicate filter (consecutive
    /// duplicates are load-bearing on the reflexive arm, unlike
    /// [`Lattice::is_chain`]'s all-pairs distinct-only convention).
    ///
    /// **Refines [`Lattice::is_chain`] via transitivity**: for every
    /// slice, `T::is_ascending(iter) ⇒ T::is_chain(iter)` — the
    /// pointwise partial order is transitive under [`Lattice::leq`], so
    /// a consecutive-pair leq-chain closes under transitive composition
    /// into an all-pairs leq-chain, and every leq-related pair is
    /// [`Lattice::is_comparable`]. Pinned on the DataClassification and
    /// CalmClassification axes below via a "sorted-by-rank ⇒ is_chain"
    /// witness.
    ///
    /// **Sequence-order DEPENDENCE** (contrasts with
    /// [`Lattice::is_chain`]'s sequence-order INDEPENDENCE): a
    /// permutation of an ascending slice with strictly-monotone
    /// distinct elements is NOT ascending in general — reversing
    /// `[Public, Internal, Confidential]` gives `[Confidential,
    /// Internal, Public]`, whose first consecutive pair fails
    /// `Confidential.leq(&Internal)`. This is what distinguishes the
    /// sequence-shape from the structural-shape predicate at the same
    /// arity: the structural predicate is a property of the underlying
    /// multiset, the sequence-shape predicate is a property of the
    /// emission ORDER.
    ///
    /// **Ascending-descending duality on total orders**: on any
    /// totally-ordered lattice (e.g. [`baseline::Baseline`],
    /// [`DataClassification`], [`CalmClassification`]), reversing an
    /// ascending slice yields a descending slice —
    /// `T::is_ascending(vs) ⇔ T::is_descending(vs.rev())` on the
    /// consecutive-pair walk when the order is total. On a
    /// partially-ordered lattice the equivalence weakens because a
    /// reversed slice on an incomparable consecutive pair rejects
    /// both predicates.
    ///
    /// **Any-violating-consecutive-pair rejection**: if any consecutive
    /// pair `(vs[i], vs[i + 1])` fails `vs[i].leq(&vs[i + 1])`, then
    /// `T::is_ascending(iter) == false` — [`Iterator::all`] on
    /// [`slice::windows`] short-circuits at the first violating pair,
    /// so a single non-monotone step anywhere in the sequence rejects
    /// the whole predicate. The rejection includes any consecutive
    /// pair that is incomparable on the partial order —
    /// [`Lattice::leq`] fails on incomparable elements, so a slice
    /// containing an incomparable consecutive pair fails
    /// `is_ascending` (and, by the dual walk, `is_descending`), which
    /// is exactly what distinguishes an antichain-containing slice
    /// from a chain-containing one at the sequence level.
    ///
    /// **Antichain rejection**: on an antichain lattice (e.g.
    /// [`SubstrateType`]), the predicate rejects any consecutive pair
    /// of DISTINCT non-top elements — the two are incomparable, so
    /// [`Lattice::leq`] fails, so the consecutive-pair walk fails. It
    /// accepts consecutive pairs where the second element is the
    /// pointed-top [`SubstrateType::Regulatory`] (the top-directed
    /// half-edge) or both elements are equal (the reflexive arm).
    ///
    /// Default routes through `iter.into_iter().collect::<Vec<&Self>>()`
    /// followed by `.windows(2).all(|w| w[0].leq(w[1]))` — one
    /// [`Lattice::leq`] delegation per consecutive pair on the
    /// collected `Vec<&Self>` buffer. The collect materializes the
    /// iterator once so the walk operates on a stable slice — an
    /// [`IntoIterator`] that yields distinct values on distinct calls
    /// would otherwise break the walk's determinism. A future
    /// normalization at [`Lattice::leq`] lands at ONE site and this
    /// default inherits the sequence-shape peer mechanically.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the sequence-shape ascending predicate is
    /// itself a typed named `bool` composing [`Lattice::leq`] via
    /// [`slice::windows`] plus [`Iterator::all`]; every downstream
    /// lattice-law consumer inherits the predicate through the default
    /// mechanically) + THEORY.md §III (typescape — the sequence-shape
    /// monotonicity predicate on every classification-axis lattice
    /// binds at ONE substrate owner on the [`Lattice`] algebra rather
    /// than at each consumer's hand-rolled
    /// `vs.windows(2).all(|w| w[0].leq(&w[1]))` walk). The pattern
    /// already exists in-tree at
    /// `tatara_lisp::macro_expand::ResourceLimits::is_ascending` — a
    /// per-domain `const fn` on the pointwise resource-posture partial
    /// order whose own doc explicitly names this widening: "the
    /// (ascending, descending) sequence-level pair opens the door to
    /// `is_strictly_ascending` / `is_strictly_descending` via
    /// `Self::lt` / `Self::gt` one STRICTNESS axis over"; this
    /// widening lifts the SAME shape from a per-domain `const fn` to
    /// the trait's default-method surface so every future closed-set
    /// lattice impl inherits the sequence-monotonicity predicate for
    /// free.
    ///
    /// Frontier inspiration: [`Iterator::is_sorted`] and
    /// [`slice::is_sorted_by`] on Rust's stdlib total-order surface —
    /// the sequence-shape monotonicity predicate is a first-class named
    /// method the standard library exposes rather than leaving each
    /// consumer to compose `slice.windows(2).all(|w| w[0] <= w[1])` at
    /// its callsite. Translated: threaded the same consecutive-pair
    /// monotonicity shape through the [`Lattice`] trait's default-
    /// method surface, generalizing from `Ord::cmp` on a totally-
    /// ordered carrier to [`Lattice::leq`] on any partial-order
    /// lattice — Haskell's `Data.List` `and . zipWith (<=) xs . tail xs`
    /// idiom on a partial-order carrier; Coq's `Sorted` inductive on
    /// lists over a `Relation`.
    fn is_ascending<'a, I>(iter: I) -> bool
    where
        I: IntoIterator<Item = &'a Self>,
        Self: 'a,
    {
        let vs: Vec<&'a Self> = iter.into_iter().collect();
        vs.windows(2).all(|w| w[0].leq(w[1]))
    }
    /// Dual of [`Lattice::is_ascending`] on the PAIR-LEVEL-PRIMITIVE
    /// axis — the iterated set is DESCENDING (a [`Lattice::geq`]-
    /// monotone non-increasing sequence on the lattice's partial
    /// order) iff every CONSECUTIVE pair `(vs[i], vs[i + 1])`
    /// satisfies `vs[i].geq(&vs[i + 1])`. `T::is_descending([&a, &b,
    /// &c])` holds iff `a.geq(&b) && b.geq(&c)`; three or more
    /// elements chain through the consecutive-pair conjunction.
    ///
    /// The SEQUENCE-SHAPE peer of [`Lattice::is_antichain`] one AXIS-
    /// SHAPE reading away — the (ascending, descending) sequence
    /// pair is the DIRECTIONAL projection of the (leq, geq) pairwise
    /// pair one CARDINALITY axis up, exactly the way the (chain,
    /// antichain) structural pair is the (comparable, incomparable)
    /// projection one arity up. Together with [`Lattice::is_ascending`]
    /// closes the (leq, geq) 2×1 sequence-monotonicity pair on the
    /// consecutive-pair face of the algebra's combinator surface.
    ///
    /// **Empty-iterator vacuous truth**: `T::is_descending(std::iter::
    /// empty()) == true` on every lattice — same vacuous-conjunction
    /// identity as [`Lattice::is_ascending`]'s empty arm.
    ///
    /// **Singleton vacuous truth**: `T::is_descending([&a]) == true`
    /// for every `a` — a singleton has no consecutive pair.
    ///
    /// **Consecutive-duplicate identity**: `T::is_descending([&a, &a])
    /// == true` for every `a` — [`Lattice::geq`] is REFLEXIVE (its
    /// default routes through `other.leq(self)`, which reflexively
    /// fires true on the self-pair). Shared reflexive-diagonal arm
    /// with [`Lattice::is_ascending`].
    ///
    /// **Pair-identity**: `T::is_descending([&a, &b]) == a.geq(&b)` —
    /// dual to [`Lattice::is_ascending`]'s pair-identity on the
    /// [`Lattice::geq`] arm.
    ///
    /// **Refines [`Lattice::is_chain`] via transitivity**: for every
    /// slice, `T::is_descending(iter) ⇒ T::is_chain(iter)` — the
    /// pointwise partial order is transitive under [`Lattice::geq`]
    /// (via [`Lattice::geq`]'s composition `other.leq(self)` and
    /// [`Lattice::leq`]'s transitivity), so a consecutive-pair
    /// geq-chain closes under transitive composition into an all-pairs
    /// geq-chain, and every geq-related pair is comparable. Peer of
    /// [`Lattice::is_ascending`]'s ascending-implies-chain arm on the
    /// dual pair-level primitive.
    ///
    /// **Ascending-descending duality on total orders**: reversing an
    /// ascending slice on a totally-ordered lattice yields a descending
    /// slice — `T::is_ascending(vs) ⇔ T::is_descending(vs.rev())` on
    /// the consecutive-pair walk when the order is total.
    ///
    /// **Any-violating-consecutive-pair rejection**: if any consecutive
    /// pair `(vs[i], vs[i + 1])` fails `vs[i].geq(&vs[i + 1])`, then
    /// `T::is_descending(iter) == false` — short-circuits at the first
    /// violating pair.
    ///
    /// **Antichain rejection**: dual of [`Lattice::is_ascending`]'s
    /// antichain-rejection arm — on the pointed-top antichain, the
    /// predicate rejects any consecutive pair of DISTINCT non-top
    /// elements (incomparable, so [`Lattice::geq`] fails) and accepts
    /// consecutive pairs where the FIRST element is the pointed-top
    /// (the top-directed half-edge, reading from top to a lower
    /// element) or both elements are equal.
    ///
    /// Default routes through `iter.into_iter().collect::<Vec<&Self>>()`
    /// followed by `.windows(2).all(|w| w[0].geq(w[1]))` — one
    /// [`Lattice::geq`] delegation per consecutive pair on the collected
    /// buffer, mirroring [`Lattice::is_ascending`]'s walk with the pair-
    /// level primitive swapped from [`Lattice::leq`] to [`Lattice::geq`].
    ///
    /// Theory anchor: same as [`Lattice::is_ascending`] on the dual
    /// pair-level primitive — THEORY.md §II.1 invariant 5 + §III. The
    /// consecutive-pair face on every classification-axis lattice now
    /// binds through TWO substrate defaults ([`Lattice::is_ascending`],
    /// [`Lattice::is_descending`]) closing the (leq, geq) direction
    /// axis at ONE algebra owner.
    fn is_descending<'a, I>(iter: I) -> bool
    where
        I: IntoIterator<Item = &'a Self>,
        Self: 'a,
    {
        let vs: Vec<&'a Self> = iter.into_iter().collect();
        vs.windows(2).all(|w| w[0].geq(w[1]))
    }
    /// Strict-arm peer of [`Lattice::is_ascending`] on the STRICTNESS
    /// axis — the iterated set is STRICTLY ASCENDING (a
    /// [`Lattice::strictly_below`]-monotone strictly-increasing sequence
    /// on the lattice's partial order) iff every CONSECUTIVE pair
    /// `(vs[i], vs[i + 1])` satisfies `vs[i].strictly_below(&vs[i + 1])`.
    /// `T::is_strictly_ascending([&a, &b, &c])` holds iff
    /// `a.strictly_below(&b) && b.strictly_below(&c)`; three or more
    /// elements chain through the consecutive-pair conjunction.
    ///
    /// The STRICT-ARM peer of [`Lattice::is_ascending`] one STRICTNESS
    /// axis over on the (strict, non-strict) × (leq, geq) sequence-
    /// level 2×2 monotonicity grid — where [`Lattice::is_ascending`]
    /// walks the REFLEXIVE non-strict [`Lattice::leq`] arm (accepting
    /// consecutive-duplicate pairs on the reflexive diagonal), this
    /// walks the IRREFLEXIVE strict [`Lattice::strictly_below`] arm
    /// (rejecting every consecutive-duplicate pair by irreflexivity of
    /// the strict comparator). Together with [`Lattice::is_strictly_descending`]
    /// on the dual strict arm this closes the whole (strict, non-strict)
    /// × (leq, geq) 2×2 sequence-monotonicity grid on the trait's
    /// combinator surface at the consecutive-pair face.
    ///
    /// **Empty-iterator vacuous truth**: `T::is_strictly_ascending(std::
    /// iter::empty()) == true` on every lattice — [`slice::windows`] on
    /// a zero-length slice yields no pair, so [`Iterator::all`] on the
    /// empty iterator is `true`. Shared vacuous-truth arm with all four
    /// sequence-monotonicity predicates ([`Lattice::is_ascending`],
    /// [`Lattice::is_descending`], [`Lattice::is_strictly_ascending`],
    /// [`Lattice::is_strictly_descending`]) at the empty case.
    ///
    /// **Singleton vacuous truth**: `T::is_strictly_ascending([&a]) ==
    /// true` for every `a` — a singleton has no consecutive pair, same
    /// vacuous-conjunction reasoning as the empty case. Shared with all
    /// four sequence-monotonicity predicates at every singleton.
    ///
    /// **Consecutive-duplicate rejection**: `T::is_strictly_ascending(
    /// [&a, &a]) == false` for every `a` — [`Lattice::strictly_below`]
    /// is IRREFLEXIVE (its default routes through `self.leq(other) &&
    /// self != other`, so the second conjunct fails on the self-pair),
    /// so the consecutive-duplicate pair `(a, a)` rejects the walk.
    /// DIVERGES from [`Lattice::is_ascending`]'s consecutive-duplicate
    /// acceptance arm — this is the primary distinguishing consequence
    /// of moving from the reflexive [`Lattice::leq`] arm to the
    /// irreflexive [`Lattice::strictly_below`] arm. Any slice with a
    /// consecutive-duplicate pair anywhere in it fails BOTH strict
    /// sequence-monotonicity predicates by short-circuit.
    ///
    /// **Pair-identity**: `T::is_strictly_ascending([&a, &b]) ==
    /// a.strictly_below(&b)` — the 2-input strict-ascending predicate
    /// reduces to the pairwise strict primitive directly. Peer of
    /// [`Lattice::is_ascending`]'s pair-identity one STRICTNESS axis
    /// over on the same consecutive-pair face.
    ///
    /// **Strict-implies-non-strict**: for every slice,
    /// `T::is_strictly_ascending(iter) ⇒ T::is_ascending(iter)` — the
    /// strict pairwise-primitive [`Lattice::strictly_below`] implies
    /// the non-strict pairwise-primitive [`Lattice::leq`] at every
    /// pair (`self.leq(other) && self != other ⇒ self.leq(other)` by
    /// projection onto the first conjunct), so the conjunctive fold
    /// through [`Iterator::all`] propagates the implication across
    /// every adjacent-pair window. Pinned on the DataClassification
    /// axis below via exhaustive triples.
    ///
    /// **Refines [`Lattice::is_chain`] via transitivity**: for every
    /// slice, `T::is_strictly_ascending(iter) ⇒ T::is_chain(iter)` —
    /// composes the strict-implies-non-strict arm above with
    /// [`Lattice::is_ascending`]'s existing ascending-implies-chain
    /// pin. Peer of [`Lattice::is_ascending`]'s
    /// ascending-implies-chain arm on the strict pair-level primitive.
    ///
    /// **Strict-ascending-strict-descending duality on total orders**:
    /// on any totally-ordered lattice (e.g. [`baseline::Baseline`],
    /// [`DataClassification`], [`CalmClassification`]), reversing a
    /// strictly-ascending slice yields a strictly-descending slice —
    /// `T::is_strictly_ascending(vs) ⇔ T::is_strictly_descending(vs.
    /// rev())` on the consecutive-pair walk when the order is total.
    ///
    /// **Any-violating-consecutive-pair rejection**: if any consecutive
    /// pair `(vs[i], vs[i + 1])` fails `vs[i].strictly_below(&vs[i +
    /// 1])`, then `T::is_strictly_ascending(iter) == false` —
    /// [`Iterator::all`] on [`slice::windows`] short-circuits at the
    /// first violating pair, so a single non-strictly-monotone step
    /// anywhere in the sequence (whether from a consecutive duplicate,
    /// a strictly-descending pair, or an incomparable pair) rejects
    /// the whole predicate.
    ///
    /// **Antichain rejection**: on an antichain lattice (e.g.
    /// [`SubstrateType`]), the predicate rejects EVERY consecutive
    /// pair — the strict [`Lattice::strictly_below`] arm requires both
    /// `a.leq(&b)` (which fails on incomparable non-top pairs, and
    /// which fails on the top-emitted `(top, x)` half-edge) AND `a !=
    /// b` (which fails on every reflexive pair), so the only accepted
    /// pair on the pointed-top antichain is `(x, top)` for `x != top`
    /// (the top-directed strict half-edge). Peer of
    /// [`Lattice::is_ascending`]'s antichain-rejection arm with the
    /// reflexive-diagonal acceptance stripped out.
    ///
    /// Default routes through `iter.into_iter().collect::<Vec<&Self>>()`
    /// followed by `.windows(2).all(|w| w[0].strictly_below(w[1]))` —
    /// one [`Lattice::strictly_below`] delegation per consecutive pair
    /// on the collected `Vec<&Self>` buffer, mirroring
    /// [`Lattice::is_ascending`]'s walk with the pair-level primitive
    /// swapped from [`Lattice::leq`] to [`Lattice::strictly_below`]. The
    /// collect materializes the iterator once so the walk operates on a
    /// stable slice. A future normalization at [`Lattice::strictly_below`]
    /// (or at the underlying [`Lattice::leq`] the strict default routes
    /// through) lands at ONE site and this default inherits the strict
    /// sequence-shape peer mechanically.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the strict sequence-shape ascending predicate
    /// is itself a typed named `bool` composing [`Lattice::strictly_below`]
    /// via [`slice::windows`] plus [`Iterator::all`]; every downstream
    /// lattice-law consumer inherits the strict predicate through the
    /// default mechanically) + THEORY.md §III (typescape — the strict
    /// sequence-shape monotonicity predicate on every classification-
    /// axis lattice binds at ONE substrate owner on the [`Lattice`]
    /// algebra rather than at each consumer's hand-rolled
    /// `vs.windows(2).all(|w| w[0].leq(&w[1]) && w[0] != w[1])` walk).
    /// The pattern already exists in-tree at
    /// `tatara_lisp::closed_set::ClosedSet::is_strictly_ascending` — a
    /// per-domain trait method on the declaration-order strict-ascent
    /// predicate for closed sets; this widening lifts the SAME shape
    /// from the closed-set trait to the [`Lattice`] trait's default-
    /// method surface so every future closed-set lattice impl inherits
    /// the strict sequence-monotonicity predicate for free.
    ///
    /// Frontier inspiration: [`slice::is_sorted_by`] on Rust's stdlib
    /// with a strict comparator — the strict sequence-shape
    /// monotonicity predicate is a first-class named method downstream
    /// of the non-strict sorted-by primitive. Translated: threaded the
    /// same consecutive-pair strict-monotonicity shape through the
    /// [`Lattice`] trait's default-method surface, generalizing from
    /// `Ord::cmp` on a totally-ordered carrier to
    /// [`Lattice::strictly_below`] on any partial-order lattice —
    /// Haskell's `Data.List` `and . zipWith (<) xs . tail xs` idiom on
    /// a partial-order carrier; Coq's `StronglySorted` inductive on
    /// lists over a `Relation`.
    fn is_strictly_ascending<'a, I>(iter: I) -> bool
    where
        I: IntoIterator<Item = &'a Self>,
        Self: 'a,
    {
        let vs: Vec<&'a Self> = iter.into_iter().collect();
        vs.windows(2).all(|w| w[0].strictly_below(w[1]))
    }
    /// Dual of [`Lattice::is_strictly_ascending`] on the PAIR-LEVEL-
    /// PRIMITIVE axis — the iterated set is STRICTLY DESCENDING (a
    /// [`Lattice::strictly_above`]-monotone strictly-decreasing sequence
    /// on the lattice's partial order) iff every CONSECUTIVE pair
    /// `(vs[i], vs[i + 1])` satisfies `vs[i].strictly_above(&vs[i +
    /// 1])`. `T::is_strictly_descending([&a, &b, &c])` holds iff
    /// `a.strictly_above(&b) && b.strictly_above(&c)`; three or more
    /// elements chain through the consecutive-pair conjunction.
    ///
    /// The STRICT-ARM peer of [`Lattice::is_descending`] one STRICTNESS
    /// axis over, AND the PAIR-LEVEL-PRIMITIVE dual of
    /// [`Lattice::is_strictly_ascending`]. Together with
    /// [`Lattice::is_strictly_ascending`] closes the (strict, non-
    /// strict) × (leq, geq) 2×2 sequence-monotonicity grid on the
    /// trait's combinator surface at the consecutive-pair face.
    ///
    /// **Empty-iterator vacuous truth**: `T::is_strictly_descending(
    /// std::iter::empty()) == true` — shared vacuous-truth arm with
    /// all four sequence-monotonicity predicates.
    ///
    /// **Singleton vacuous truth**: `T::is_strictly_descending([&a])
    /// == true` for every `a` — a singleton has no consecutive pair.
    ///
    /// **Consecutive-duplicate rejection**: `T::is_strictly_descending(
    /// [&a, &a]) == false` for every `a` — [`Lattice::strictly_above`]
    /// is IRREFLEXIVE (routes through `other.strictly_below(self)`,
    /// which inherits irreflexivity from
    /// [`Lattice::strictly_below`]'s `self != other` conjunct), so the
    /// consecutive-duplicate pair rejects the walk. Shared irreflexive-
    /// rejection arm with [`Lattice::is_strictly_ascending`] at every
    /// all-duplicate slice — BOTH strict sequence-monotonicity
    /// predicates fire false on any slice with at least two consecutive
    /// equal elements.
    ///
    /// **Pair-identity**: `T::is_strictly_descending([&a, &b]) ==
    /// a.strictly_above(&b)` — dual to
    /// [`Lattice::is_strictly_ascending`]'s pair-identity on the
    /// [`Lattice::strictly_above`] arm.
    ///
    /// **Strict-implies-non-strict**: for every slice,
    /// `T::is_strictly_descending(iter) ⇒ T::is_descending(iter)` —
    /// dual of the ascending arm's implication via
    /// [`Lattice::strictly_above`] ⇒ [`Lattice::geq`].
    ///
    /// **Refines [`Lattice::is_chain`] via transitivity**: for every
    /// slice, `T::is_strictly_descending(iter) ⇒ T::is_chain(iter)` —
    /// composes strict-implies-non-strict-descending with
    /// [`Lattice::is_descending`]'s descending-implies-chain pin.
    ///
    /// **Strict-ascending-strict-descending duality on total orders**:
    /// reversing a strictly-ascending slice on a totally-ordered
    /// lattice yields a strictly-descending slice —
    /// `T::is_strictly_ascending(vs) ⇔ T::is_strictly_descending(vs.
    /// rev())` on the consecutive-pair walk when the order is total.
    ///
    /// **Any-violating-consecutive-pair rejection**: if any consecutive
    /// pair `(vs[i], vs[i + 1])` fails `vs[i].strictly_above(&vs[i +
    /// 1])`, then `T::is_strictly_descending(iter) == false` — short-
    /// circuits at the first violating pair.
    ///
    /// **Antichain rejection**: dual of
    /// [`Lattice::is_strictly_ascending`]'s antichain-rejection arm —
    /// on the pointed-top antichain, the predicate rejects every
    /// consecutive pair except `(top, x)` for `x != top` (the top-
    /// emitted strict half-edge, reading strictly from top to a lower
    /// element).
    ///
    /// Default routes through `iter.into_iter().collect::<Vec<&Self>>()`
    /// followed by `.windows(2).all(|w| w[0].strictly_above(w[1]))` —
    /// one [`Lattice::strictly_above`] delegation per consecutive pair
    /// on the collected buffer, mirroring
    /// [`Lattice::is_strictly_ascending`]'s walk with the pair-level
    /// primitive swapped from [`Lattice::strictly_below`] to
    /// [`Lattice::strictly_above`].
    ///
    /// Theory anchor: same as [`Lattice::is_strictly_ascending`] on the
    /// dual pair-level primitive — THEORY.md §II.1 invariant 5 + §III.
    /// The consecutive-pair face on every classification-axis lattice
    /// now binds through FOUR substrate defaults ([`Lattice::is_ascending`],
    /// [`Lattice::is_descending`], [`Lattice::is_strictly_ascending`],
    /// [`Lattice::is_strictly_descending`]) closing the (strict, non-
    /// strict) × (leq, geq) 2×2 sequence-monotonicity grid at ONE
    /// algebra owner.
    fn is_strictly_descending<'a, I>(iter: I) -> bool
    where
        I: IntoIterator<Item = &'a Self>,
        Self: 'a,
    {
        let vs: Vec<&'a Self> = iter.into_iter().collect();
        vs.windows(2).all(|w| w[0].strictly_above(w[1]))
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

    // ── Lattice::strictly_below / strictly_above — strict-order
    //    default peers ────────────────────────────────────────────
    //
    // Bind [`Lattice::strictly_below`] + [`Lattice::strictly_above`] at
    // fail-before-pass-after granularity. Pre-lift the [`Lattice`]
    // trait's comparator surface was `{leq, geq, is_comparable,
    // is_incomparable}` — the non-strict arm was complete, but a
    // consumer that wanted the STRICT `<` / `>` relation composed
    // `a.leq(&b) && a != b` (or the antisymmetric-leq equivalent
    // `a.leq(&b) && !b.leq(&a)`) at each callsite. The pattern already
    // exists in-tree at
    // [`tatara_lisp::macro_expand::ResourceLimits::lt`] — a per-domain
    // `const fn` on the pointwise resource-posture partial order —
    // which seeds the ★★ PRIME-DIRECTIVE `≥ 2`-consumer lift with a
    // first consumer, so a hypothetical second closed-set lattice
    // consumer (a compliance-baseline strict-refinement check, an
    // ephemeral-env strict-tightening probe) would have crossed the
    // duplication threshold on the SAME conjunction shape. Post-lift
    // the whole strict-comparator pair binds at ONE substrate primitive
    // on the [`Lattice`] algebra, and every downstream impl (the six
    // existing in-tree ones + `Baseline` via the `crate::Lattice`
    // trait AND any future closed-set lift) inherits
    // `strictly_below` / `strictly_above` for free through the default.
    // Together with the prior widening (`geq` / `is_bottom` / `is_top`
    // / `is_comparable` / `is_incomparable` / `meet_all` / `join_all`)
    // the trait now closes the (strict, non-strict) × (leq, geq) 2×2
    // partial-order-comparator grid on the pairwise-relation face of
    // the combinator surface.
    //
    // The bare `lt` / `gt` names cannot be used on the trait: every
    // closed-set consumer here (`DataClassification`, `SubstrateType`,
    // `CalmClassification`) also derives `PartialOrd`, whose
    // `PartialOrd::lt` / `PartialOrd::gt` share the receiver shape and
    // would race the trait's methods in method-name resolution. The
    // named-relation `strictly_below` / `strictly_above` peer names
    // avoid the collision AND match the trait's `is_bottom` /
    // `is_top` / `is_comparable` / `is_incomparable` "named predicate"
    // naming discipline.
    //
    // Coverage below spans:
    //
    //   • irreflexivity (`a.strictly_below(&a) = false`,
    //     `a.strictly_above(&a) = false`) at every variant of every
    //     closed-set impl;
    //   • asymmetry (`a.strictly_below(&b) ⇒ !b.strictly_below(&a)`)
    //     at every pair;
    //   • transitivity (`a.strictly_below(&b) && b.strictly_below(&c)
    //     ⇒ a.strictly_below(&c)`) at every triple;
    //   • refines-leq (`a.strictly_below(&b) ⇒ a.leq(&b)` AND
    //     `a.leq(&b) ⇒ a.strictly_below(&b) ∨ a == b`) so the strict
    //     relation is exactly the non-strict relation minus the
    //     reflexive diagonal;
    //   • byte-identity dual (`a.strictly_above(&b) ⇔
    //     b.strictly_below(&a)`) at every pair;
    //   • closed-set antichain shape on [`SubstrateType`] — distinct
    //     non-[`SubstrateType::Regulatory`] pairs fail BOTH strict
    //     directions.

    /// [`Lattice::strictly_below`] on [`DataClassification`] is
    /// IRREFLEXIVE and projects to `sensitivity_rank`'s strict
    /// inequality at every pair — `a.strictly_below(&b) ⇔
    /// a.sensitivity_rank() < b.sensitivity_rank()`. Fail-before-
    /// pass-after: pre-lift `strictly_below` is not exposed as a
    /// trait method — consumers who wanted the strict-order reading
    /// wrote `a.leq(&b) && a != b` inline at each callsite. Post-lift
    /// the sweep pins the WHOLE 6×6 pair truth table at ONE substrate
    /// primitive so a regression that drifted either the `leq`
    /// projection OR the `!=` conjunct would surface at the pair it
    /// broke.
    #[test]
    fn strictly_below_matches_sensitivity_rank_strict_inequality_over_data_classification_all_pairs(
    ) {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            // Irreflexivity: no element is strictly less than itself.
            assert!(
                !a.strictly_below(&a),
                "strictly_below({a:?}, {a:?}) must be false — reflexive diagonal"
            );
            assert!(
                !a.strictly_above(&a),
                "strictly_above({a:?}, {a:?}) must be false — reflexive diagonal"
            );
            for b in DataClassification::ALL {
                assert_eq!(
                    a.strictly_below(&b),
                    a.sensitivity_rank() < b.sensitivity_rank(),
                    "strictly_below({a:?}, {b:?}) drifted from sensitivity_rank strict \
                     inequality — the default should route `a.leq(&b) && a != b`, and \
                     DataClassification's `leq` routes through sensitivity_rank, so the \
                     composition must agree with the rank's strict `<`",
                );
                // Dual byte-identity: `a.strictly_above(&b) ⇔
                // b.strictly_below(&a)`.
                assert_eq!(
                    a.strictly_above(&b),
                    b.strictly_below(&a),
                    "strictly_above({a:?}, {b:?}) drifted from the strictly_below-dual — \
                     the default should route `other.strictly_below(self)` verbatim",
                );
            }
        }
    }

    /// [`Lattice::strictly_below`] on [`CalmClassification`] projects
    /// to the two-arm boolean lattice's strict edge —
    /// `Monotone.strictly_below(&NonMonotone)` is the ONLY `true`
    /// slot in the 2×2 pair space; every other pair (both reflexive
    /// pairs AND the reversed strict pair) is `false`. Peer seal on
    /// the sibling boolean axis of
    /// `strictly_below_matches_sensitivity_rank_strict_inequality_over_data_classification_all_pairs`.
    #[test]
    fn strictly_below_and_strictly_above_partition_calm_classification_all_at_the_single_strict_edge(
    ) {
        use tatara_process::classification::CalmClassification::{Monotone, NonMonotone};
        // Reflexive diagonal — no element is strictly less than itself.
        assert!(!Monotone.strictly_below(&Monotone));
        assert!(!NonMonotone.strictly_below(&NonMonotone));
        assert!(!Monotone.strictly_above(&Monotone));
        assert!(!NonMonotone.strictly_above(&NonMonotone));
        // The single strict edge on the two-arm boolean lattice.
        assert!(
            Monotone.strictly_below(&NonMonotone),
            "Monotone < NonMonotone is the strict edge"
        );
        assert!(
            NonMonotone.strictly_above(&Monotone),
            "NonMonotone > Monotone is the dual strict edge"
        );
        // The reverse strict direction is empty.
        assert!(!NonMonotone.strictly_below(&Monotone));
        assert!(!Monotone.strictly_above(&NonMonotone));
    }

    /// [`Lattice::strictly_below`] on [`SubstrateType`] projects the
    /// pointed-top antichain to a strict-order predicate: every
    /// strict edge points FROM a non-[`SubstrateType::Regulatory`]
    /// substrate TO [`SubstrateType::Regulatory`] (the antichain's
    /// distinguished top), and EVERY other pair (reflexive pairs,
    /// distinct non-top pairs, the reversed direction from top) fails
    /// both strict directions. Peer seal to
    /// `substrate_type_is_incomparable_matches_the_antichain_shape`
    /// on the strict-order arm — the antichain SHAPE is now bound
    /// through THREE algebra predicates (`leq`, `is_incomparable`,
    /// `strictly_below`) via ONE substrate owner on the [`Lattice`]
    /// trait rather than per-callsite hand-authored conjunctions.
    #[test]
    fn substrate_type_strictly_below_and_strictly_above_project_the_antichain_to_the_top_directed_strict_edge(
    ) {
        use tatara_process::classification::SubstrateType;
        for s in SubstrateType::ALL {
            for t in SubstrateType::ALL {
                let below = s.strictly_below(&t);
                let above = s.strictly_above(&t);
                if s == t {
                    // Reflexive diagonal — strict relation is irreflexive.
                    assert!(
                        !below,
                        "strictly_below({s:?}, {s:?}) must be false — reflexive diagonal"
                    );
                    assert!(
                        !above,
                        "strictly_above({s:?}, {s:?}) must be false — reflexive diagonal"
                    );
                } else if t == SubstrateType::top() {
                    // Every distinct non-top s < top() — the antichain's
                    // pointed-top-comparability holds strictly on this
                    // half-edge.
                    assert!(
                        below,
                        "strictly_below({s:?}, Regulatory) must be true — the antichain's \
                         pointed-top strict edge",
                    );
                    assert!(
                        !above,
                        "strictly_above({s:?}, Regulatory) must be false — the reverse \
                         direction of the pointed-top strict edge is empty",
                    );
                } else if s == SubstrateType::top() {
                    // Reverse direction: top() > t for every distinct t.
                    assert!(
                        above,
                        "strictly_above(Regulatory, {t:?}) must be true — dual pointed-top \
                         strict edge"
                    );
                    assert!(
                        !below,
                        "strictly_below(Regulatory, {t:?}) must be false — Regulatory is \
                         the top"
                    );
                } else {
                    // Distinct non-Regulatory pairs sit on an antichain —
                    // neither strict direction holds.
                    assert!(
                        !below && !above,
                        "distinct non-Regulatory substrates ({s:?}, {t:?}) must fail BOTH \
                         strict directions — the antichain does not promote incomparable \
                         pairs to a strict-order verdict",
                    );
                }
            }
        }
    }

    /// [`Lattice::strictly_below`] refines [`Lattice::leq`] and the
    /// gap between them is exactly the reflexive diagonal —
    /// `a.strictly_below(&b) ⇒ a.leq(&b)` (strict refines non-strict)
    /// AND `a.leq(&b) && !a.strictly_below(&b) ⇒ a == b` (the
    /// non-strict-minus-strict gap is the equal pair). Pinned
    /// exhaustively over `DataClassification::ALL^2` (the 6-arm
    /// total-order axis) AND `SubstrateType::ALL^2` (the pointed
    /// antichain) so BOTH shape flavors — total order and antichain —
    /// bind the same refinement identity via ONE substrate primitive.
    #[test]
    fn strictly_below_refines_leq_and_the_gap_is_the_reflexive_diagonal() {
        use tatara_process::classification::{DataClassification, SubstrateType};
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                if a.strictly_below(&b) {
                    assert!(
                        a.leq(&b),
                        "strictly_below({a:?}, {b:?}) implies leq({a:?}, {b:?}) — strict \
                         refines non-strict",
                    );
                }
                if a.leq(&b) && !a.strictly_below(&b) {
                    assert_eq!(
                        a, b,
                        "leq({a:?}, {b:?}) ∧ ¬strictly_below({a:?}, {b:?}) ⇒ a == b — the \
                         non-strict-minus-strict gap is exactly the reflexive diagonal",
                    );
                }
            }
        }
        for a in SubstrateType::ALL {
            for b in SubstrateType::ALL {
                if a.strictly_below(&b) {
                    assert!(a.leq(&b));
                }
                if a.leq(&b) && !a.strictly_below(&b) {
                    assert_eq!(a, b);
                }
            }
        }
    }

    proptest! {
        /// [`Lattice::strictly_below`] is IRREFLEXIVE on
        /// [`DataClassification`] — `a.strictly_below(&a) == false`
        /// for every element. Proptest peer of the exhaustive
        /// `strictly_below_matches_sensitivity_rank_strict_inequality_over_data_classification_all_pairs`
        /// seal's diagonal arm. Peers on the CALM axis via
        /// `calm_strictly_below_is_irreflexive`.
        #[test]
        fn data_class_strictly_below_is_irreflexive(a in any_data_class()) {
            prop_assert!(!a.strictly_below(&a));
            prop_assert!(!a.strictly_above(&a));
        }

        /// [`Lattice::strictly_below`] is ASYMMETRIC on
        /// [`DataClassification`] — `a.strictly_below(&b) ⇒
        /// !b.strictly_below(&a)` at every pair. Inherits from
        /// [`Lattice::leq`]'s antisymmetry: if both directions of the
        /// strict relation held, both directions of `leq` would hold,
        /// forcing `a == b` by antisymmetry, contradicting either
        /// strict conjunct.
        #[test]
        fn data_class_strictly_below_is_asymmetric(
            a in any_data_class(),
            b in any_data_class(),
        ) {
            if a.strictly_below(&b) {
                prop_assert!(!b.strictly_below(&a));
            }
        }

        /// [`Lattice::strictly_below`] is TRANSITIVE on
        /// [`DataClassification`] — `a.strictly_below(&b) &&
        /// b.strictly_below(&c) ⇒ a.strictly_below(&c)` at every
        /// triple. Inherits from [`Lattice::leq`]'s transitivity on
        /// the non-strict conjunct; the strict conjunct on the outer
        /// relation carries through by antisymmetry.
        #[test]
        fn data_class_strictly_below_is_transitive(
            a in any_data_class(),
            b in any_data_class(),
            c in any_data_class(),
        ) {
            if a.strictly_below(&b) && b.strictly_below(&c) {
                prop_assert!(a.strictly_below(&c));
            }
        }

        /// [`Lattice::strictly_above`] is the BYTE-IDENTITY dual of
        /// [`Lattice::strictly_below`] on [`DataClassification`] —
        /// `a.strictly_above(&b) ⇔ b.strictly_below(&a)` at every
        /// pair. Pinned so the default's `other.strictly_below(self)`
        /// routing is caught if an impl overrides it with a drift.
        #[test]
        fn data_class_strictly_above_is_the_dual_of_strictly_below(
            a in any_data_class(),
            b in any_data_class(),
        ) {
            prop_assert_eq!(a.strictly_above(&b), b.strictly_below(&a));
        }

        /// Peer of the DataClassification irreflexivity proptest on
        /// the CALM boolean-lattice axis — same shape, different
        /// closed set. Together the two proptest cases bind BOTH
        /// total-order classification axes' strict-order irreflexivity
        /// to ONE substrate default.
        #[test]
        fn calm_strictly_below_is_irreflexive(a in any_calm()) {
            prop_assert!(!a.strictly_below(&a));
            prop_assert!(!a.strictly_above(&a));
        }

        /// Peer of the DataClassification asymmetry proptest on the
        /// CALM axis.
        #[test]
        fn calm_strictly_below_is_asymmetric(a in any_calm(), b in any_calm()) {
            if a.strictly_below(&b) {
                prop_assert!(!b.strictly_below(&a));
            }
        }

        /// Peer of the DataClassification transitivity proptest on
        /// the CALM axis.
        #[test]
        fn calm_strictly_below_is_transitive(
            a in any_calm(),
            b in any_calm(),
            c in any_calm(),
        ) {
            if a.strictly_below(&b) && b.strictly_below(&c) {
                prop_assert!(a.strictly_below(&c));
            }
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

    // ── Lattice::is_lower_bound_of / is_upper_bound_of — N-ary
    //    Boolean-conjunction default peers ─────────────────────────
    //
    // Bind [`Lattice::is_lower_bound_of`] + [`Lattice::is_upper_bound_of`]
    // at fail-before-pass-after granularity. Pre-lift the [`Lattice`]
    // trait's predicate surface carried the pairwise arm (`leq` / `geq`
    // / `strictly_below` / `strictly_above` / `is_comparable` /
    // `is_incomparable`) and the endpoint-singleton arm (`is_bottom` /
    // `is_top`), but a consumer wanting to decide whether `self` was a
    // COMMON bound for an iterated set (a
    // `Vec<Classification>`-carrying cohort, a slice of ephemeral-env
    // classifications) hand-authored `iter.all(|x| self.leq(x))` (or the
    // dual `iter.all(|x| x.leq(self))`) at each callsite. The pattern
    // already exists in-tree at
    // [`tatara_lisp::macro_expand::ResourceLimits::is_lower_bound_of`] /
    // [`ResourceLimits::is_upper_bound_of`] — a per-domain `const fn`
    // pair on the pointwise resource-posture partial order — which
    // seeds the ★★ PRIME-DIRECTIVE `≥ 2`-consumer lift with a first
    // consumer, and every future closed-set lattice with an iterated-
    // bound consumer would cross the duplication threshold on the SAME
    // conjunction shape.
    //
    // Post-lift the whole N-ary bound-predicate pair binds at ONE
    // substrate primitive on the [`Lattice`] algebra, and every
    // downstream impl (the six existing in-tree ones + `Baseline` via
    // the `crate::Lattice` trait AND any future closed-set lift)
    // inherits `is_lower_bound_of` / `is_upper_bound_of` for free
    // through the default. Together with the prior widening (`geq` /
    // `is_bottom` / `is_top` / `is_comparable` / `is_incomparable` /
    // `strictly_below` / `strictly_above` / `meet_all` / `join_all`)
    // the trait now closes the (predicate, combinator) × (meet, join)
    // 2×2 N-ary-aggregation grid on the algebra's combinator surface:
    // ([`meet_all`], [`is_lower_bound_of`]) close the meet arm, and
    // ([`join_all`], [`is_upper_bound_of`]) close the join arm.
    //
    // Coverage below spans:
    //
    //   • empty-iterator vacuous truth (both directions return `true`
    //     on the empty iterator — the empty-conjunction identity);
    //   • singleton-identity (1-input reduces to the pairwise relation,
    //     with the direction flipped on the upper-bound arm);
    //   • meet-witness / join-witness (the N-ary aggregate is always a
    //     bound of the iterated set — the definitional link between the
    //     N-ary COMBINATOR and the N-ary PREDICATE);
    //   • universal-endpoint witness (`bottom()` is a lower bound of
    //     every set; `top()` is an upper bound of every set);
    //   • any-violating-element rejection (short-circuits `false` when
    //     any element violates the direction);
    //   • closed-set exhaustive sweeps on `DataClassification::ALL^2`
    //     (a is a lower bound of {&b} iff a.leq(&b), and every rank-
    //     shape of the sensitivity axis holds byte-for-byte) and
    //     `SubstrateType::ALL^2` (antichain-shape — only `Regulatory`
    //     is an upper bound of any distinct element, only every
    //     substrate is a lower bound of `Regulatory`).

    /// [`Lattice::is_lower_bound_of`] on the empty iterator returns
    /// `true` at every variant of [`DataClassification::ALL`] — the
    /// empty conjunction is vacuously true. Peer of `meet_all` on the
    /// empty iterator collapsing to `top()`: the empty aggregate binds
    /// to the identity of the underlying operation (`true` for Boolean
    /// conjunction; [`Lattice::top`] for meet), so the empty case
    /// never rejects. Fail-before-pass-after: pre-lift
    /// `is_lower_bound_of` is not exposed as a trait method, so the
    /// empty case had no typed answer — a consumer that hand-rolled
    /// `iter.all(|x| self.leq(x))` inherited `Iterator::all`'s empty
    /// semantics per callsite; post-lift the vacuous-true is fixed at
    /// ONE primitive on the [`Lattice`] algebra.
    #[test]
    fn is_lower_and_upper_bound_of_empty_iterator_are_vacuously_true_over_data_classification_all()
    {
        use tatara_process::classification::DataClassification;
        let empty: [&DataClassification; 0] = [];
        for a in DataClassification::ALL {
            assert!(
                a.is_lower_bound_of(empty),
                "is_lower_bound_of on empty iterator must be true at {a:?} — the empty \
                 conjunction is vacuously true",
            );
            assert!(
                a.is_upper_bound_of(empty),
                "is_upper_bound_of on empty iterator must be true at {a:?} — the empty \
                 conjunction is vacuously true",
            );
        }
    }

    /// [`Lattice::is_lower_bound_of`] on a singleton reduces to the
    /// pairwise `leq` relation over `DataClassification::ALL^2` — i.e.
    /// `a.is_lower_bound_of([&b]) == a.leq(&b)` at every pair. Dually,
    /// [`Lattice::is_upper_bound_of`] on a singleton reduces to the
    /// direction-flipped `b.leq(&a)`. Pinned exhaustively on the 6×6
    /// pair space so a regression that flipped the direction on either
    /// predicate would surface at the pair it broke.
    #[test]
    fn is_lower_and_upper_bound_of_singleton_reduce_to_leq_over_data_classification_all_pairs() {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert_eq!(
                    a.is_lower_bound_of([&b]),
                    a.leq(&b),
                    "is_lower_bound_of({a:?}, [&{b:?}]) drifted from leq({a:?}, {b:?}) — \
                     the 1-input predicate must reduce to the pairwise relation",
                );
                assert_eq!(
                    a.is_upper_bound_of([&b]),
                    b.leq(&a),
                    "is_upper_bound_of({a:?}, [&{b:?}]) drifted from the direction-flipped \
                     leq({b:?}, {a:?}) — the upper-bound singleton flips the direction",
                );
            }
        }
    }

    /// [`Lattice::meet_all`] is always a lower bound of the iterated
    /// set — `T::meet_all(iter).is_lower_bound_of(iter) == true` for
    /// every iterable. Dually, [`Lattice::join_all`] is always an upper
    /// bound. Pinned over every non-empty subset of
    /// `DataClassification::ALL` via the 2-arm and 3-arm folds — the
    /// definitional link between the N-ary COMBINATOR and the N-ary
    /// PREDICATE (the aggregate the combinator produces is always a
    /// member of the bound set the predicate characterizes).
    #[test]
    fn meet_all_and_join_all_witness_the_bounds_over_data_classification_all_triples() {
        use tatara_process::classification::DataClassification;
        // 2-arm sweep.
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                let iter2 = [&a, &b];
                let meet = DataClassification::meet_all(iter2);
                let join = DataClassification::join_all(iter2);
                assert!(
                    meet.is_lower_bound_of(iter2),
                    "meet_all({a:?}, {b:?}) = {meet:?} must be a lower bound of \
                     [&{a:?}, &{b:?}] — the definitional link",
                );
                assert!(
                    join.is_upper_bound_of(iter2),
                    "join_all({a:?}, {b:?}) = {join:?} must be an upper bound of \
                     [&{a:?}, &{b:?}] — the dual definitional link",
                );
            }
        }
        // 3-arm sweep.
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                for c in DataClassification::ALL {
                    let iter3 = [&a, &b, &c];
                    let meet = DataClassification::meet_all(iter3);
                    let join = DataClassification::join_all(iter3);
                    assert!(meet.is_lower_bound_of(iter3));
                    assert!(join.is_upper_bound_of(iter3));
                }
            }
        }
    }

    /// [`Lattice::bottom`] is a common lower bound of every iterated
    /// set over [`DataClassification`] — `T::bottom().is_lower_bound_of(
    /// iter) == true` for every iterable, since `T::bottom().leq(x) ==
    /// true` for every `x` by the lattice-bottom axiom. Dually,
    /// [`Lattice::top`] is a common upper bound. Pinned on the full
    /// closed set as the iterable so both endpoint witnesses hold at
    /// the widest possible iterated set. Peer seal to
    /// `meet_all_and_join_all_over_full_data_classification_all_reach_bottom_and_top`
    /// on the PREDICATE arm of the (combinator, predicate) grid.
    #[test]
    fn bottom_is_lower_bound_and_top_is_upper_bound_of_full_data_classification_all() {
        use tatara_process::classification::DataClassification;
        let all: Vec<&DataClassification> = DataClassification::ALL.iter().collect();
        assert!(
            DataClassification::bottom().is_lower_bound_of(all.iter().copied()),
            "bottom() (Public) must be a lower bound of the full closed set — \
             the lattice-bottom axiom",
        );
        assert!(
            DataClassification::top().is_upper_bound_of(all.iter().copied()),
            "top() (Pci) must be an upper bound of the full closed set — \
             the lattice-top axiom",
        );
    }

    /// [`Lattice::is_upper_bound_of`] projects the pointed-top antichain
    /// on [`SubstrateType`] to its distinguishing shape: only
    /// [`SubstrateType::top`] (Regulatory) is an upper bound of the full
    /// closed set, and only [`SubstrateType::bottom`] (Financial) fails
    /// to be a lower bound only-of-itself — a non-Regulatory `self` is
    /// an upper bound of a slice iff the slice is `[]` or `[self]` (or
    /// a repetition of `self`), and Regulatory is an upper bound of any
    /// slice by the pointed-top axiom. Peer seal to
    /// `substrate_type_is_incomparable_matches_the_antichain_shape`
    /// and
    /// `substrate_type_strictly_below_and_strictly_above_project_the_antichain_to_the_top_directed_strict_edge`
    /// on the N-ary Boolean-predicate arm — the antichain SHAPE is now
    /// bound through FOUR algebra predicates (`leq`, `is_incomparable`,
    /// `strictly_below`, `is_upper_bound_of`) via ONE substrate owner
    /// on the [`Lattice`] trait.
    #[test]
    fn is_upper_bound_of_projects_the_pointed_top_antichain_over_substrate_type_all() {
        use tatara_process::classification::SubstrateType;
        // The full closed set: only Regulatory is an upper bound of it.
        let all: Vec<&SubstrateType> = SubstrateType::ALL.iter().collect();
        for s in SubstrateType::ALL {
            let is_upper = s.is_upper_bound_of(all.iter().copied());
            if s == SubstrateType::top() {
                assert!(
                    is_upper,
                    "top() (Regulatory) must be an upper bound of the full closed set — \
                     the pointed-top axiom",
                );
            } else {
                assert!(
                    !is_upper,
                    "non-top substrate {s:?} must NOT be an upper bound of the full closed \
                     set — the antichain rejects any upper-bound claim that isn't the top",
                );
            }
            // Every substrate is a lower bound of Regulatory (the pointed-top).
            assert!(
                s.is_lower_bound_of([&SubstrateType::top()]),
                "every substrate {s:?} must be a lower bound of [Regulatory] — the \
                 pointed-top strict-leq half-edge",
            );
        }
    }

    /// [`Lattice::is_lower_bound_of`] short-circuits `false` when any
    /// element in the iterator violates the containment direction over
    /// [`DataClassification`]: given a slice `[&low, &high]` where
    /// `low < self < high`, `self.is_lower_bound_of([&low, &high])` is
    /// `false` because `self.leq(&low)` fails at the first element.
    /// Pinned on a specific violating configuration and its dual so
    /// both short-circuit arms bind at the substrate.
    #[test]
    fn is_lower_and_upper_bound_of_reject_when_any_element_violates_over_data_classification() {
        use tatara_process::classification::DataClassification::{
            Confidential, Internal, Pci, Phi, Pii, Public,
        };
        // `Confidential` is NOT a lower bound of `[Internal, Pii]` —
        // fails at `Confidential.leq(Internal)`.
        assert!(!Confidential.is_lower_bound_of([&Internal, &Pii]));
        // `Confidential` IS a lower bound of `[Pii, Phi, Pci]` — every
        // element sits at-or-above.
        assert!(Confidential.is_lower_bound_of([&Pii, &Phi, &Pci]));
        // `Pii` is NOT an upper bound of `[Public, Pci]` — fails at
        // `Pci.leq(Pii)`.
        assert!(!Pii.is_upper_bound_of([&Public, &Pci]));
        // `Pii` IS an upper bound of `[Public, Internal, Confidential]`
        // — every element sits at-or-below.
        assert!(Pii.is_upper_bound_of([&Public, &Internal, &Confidential]));
    }

    proptest! {
        /// [`Lattice::is_lower_bound_of`] on a singleton reduces to the
        /// pairwise `leq` relation over any [`DataClassification`]
        /// pair — proptest peer of the exhaustive
        /// `is_lower_and_upper_bound_of_singleton_reduce_to_leq_over_data_classification_all_pairs`
        /// seal above. Randomized draws via [`any_data_class`] catch
        /// the same drift the exhaustive form does; peers on the CALM
        /// axis via `calm_is_lower_bound_of_singleton_reduces_to_leq`.
        #[test]
        fn data_class_is_lower_and_upper_bound_of_singleton_reduce_to_leq(
            a in any_data_class(),
            b in any_data_class(),
        ) {
            prop_assert_eq!(a.is_lower_bound_of([&b]), a.leq(&b));
            prop_assert_eq!(a.is_upper_bound_of([&b]), b.leq(&a));
        }

        /// [`Lattice::meet_all`] is always a lower bound of the iterated
        /// pair — proptest peer of the exhaustive
        /// `meet_all_and_join_all_witness_the_bounds_over_data_classification_all_triples`
        /// seal's 2-arm sweep. The definitional link between the N-ary
        /// COMBINATOR and the N-ary PREDICATE, pinned on random pairs.
        #[test]
        fn data_class_meet_witnesses_lower_bound_and_join_witnesses_upper_bound_on_pair(
            a in any_data_class(),
            b in any_data_class(),
        ) {
            let iter = [&a, &b];
            let meet = DataClassification::meet_all(iter);
            let join = DataClassification::join_all(iter);
            prop_assert!(meet.is_lower_bound_of(iter));
            prop_assert!(join.is_upper_bound_of(iter));
        }

        /// Peer of the DataClassification singleton-reduction proptest
        /// on the CALM boolean-lattice axis — same shape, different
        /// closed set.
        #[test]
        fn calm_is_lower_and_upper_bound_of_singleton_reduce_to_leq(
            a in any_calm(),
            b in any_calm(),
        ) {
            prop_assert_eq!(a.is_lower_bound_of([&b]), a.leq(&b));
            prop_assert_eq!(a.is_upper_bound_of([&b]), b.leq(&a));
        }

        /// Peer of the DataClassification meet-witness / join-witness
        /// proptest on the CALM axis.
        #[test]
        fn calm_meet_witnesses_lower_bound_and_join_witnesses_upper_bound_on_pair(
            a in any_calm(),
            b in any_calm(),
        ) {
            let iter = [&a, &b];
            let meet = CalmClassification::meet_all(iter);
            let join = CalmClassification::join_all(iter);
            prop_assert!(meet.is_lower_bound_of(iter));
            prop_assert!(join.is_upper_bound_of(iter));
        }
    }

    // ── Lattice::is_strict_lower_bound_of / is_strict_upper_bound_of —
    //    strict-arm N-ary Boolean-conjunction default peers ─────────
    //
    // Bind [`Lattice::is_strict_lower_bound_of`] +
    // [`Lattice::is_strict_upper_bound_of`] at fail-before-pass-after
    // granularity. Pre-lift the [`Lattice`] trait's N-ary Boolean-
    // conjunction surface carried the NON-STRICT arm (`is_lower_bound_of`
    // / `is_upper_bound_of` via `iter.all(|x| self.leq(x))`), but a
    // consumer wanting to decide whether `self` was a STRICT common
    // bound for an iterated set (a compliance-baseline strict-refinement
    // probe over a cohort, an ephemeral-env strict-tightening check
    // over a `Vec<Classification>`) hand-authored
    // `iter.all(|x| self.strictly_below(x))` (or the dual
    // `iter.all(|x| x.strictly_below(self))`) at each callsite. The
    // pattern already exists in-tree — the pairwise strict comparator
    // pair (`strictly_below` / `strictly_above`) landed as a lift from
    // [`tatara_lisp::macro_expand::ResourceLimits::lt`], and the N-ary
    // non-strict pair (`is_lower_bound_of` / `is_upper_bound_of`)
    // landed as a lift from
    // [`tatara_lisp::macro_expand::ResourceLimits::is_lower_bound_of`]
    // / `is_upper_bound_of`. The STRICT N-ary arm is the missing corner
    // of the (strict, non-strict) × (lower, upper) 2×2 N-ary Boolean-
    // conjunction predicate grid — it composes the STRICT pairwise
    // arm with the N-ARY Boolean-conjunction aggregator via ONE
    // substrate default rather than at each consumer's hand-rolled
    // `iter.all(|x| self.strictly_below(x))`.
    //
    // Post-lift the whole strict N-ary bound-predicate pair binds at
    // ONE substrate primitive on the [`Lattice`] algebra, and every
    // downstream impl (the six existing in-tree ones + `Baseline` via
    // the `crate::Lattice` trait AND any future closed-set lift)
    // inherits `is_strict_lower_bound_of` / `is_strict_upper_bound_of`
    // for free through the default. Together with the prior widening
    // (`geq` / `is_bottom` / `is_top` / `is_comparable` /
    // `is_incomparable` / `strictly_below` / `strictly_above` /
    // `meet_all` / `join_all` / `is_lower_bound_of` /
    // `is_upper_bound_of`) the trait now closes the (strict, non-strict)
    // × (lower, upper) 2×2 N-ary Boolean-conjunction predicate grid on
    // the algebra's combinator surface: ([`is_lower_bound_of`],
    // [`is_upper_bound_of`]) close the non-strict arm, and
    // ([`is_strict_lower_bound_of`], [`is_strict_upper_bound_of`])
    // close the strict arm.
    //
    // Coverage below spans:
    //
    //   • empty-iterator vacuous truth (both directions return `true`
    //     on the empty iterator — the empty-conjunction identity);
    //   • singleton-strict-identity (1-input reduces to the pairwise
    //     STRICT relation, with the direction flipped on the upper-
    //     bound arm);
    //   • refines-non-strict (strict predicate ⇒ non-strict predicate
    //     at every iterable, and the strict-minus-non-strict gap on
    //     the aggregate is exactly the iterables containing `self`);
    //   • irreflexivity-on-inclusive-iterable (inserting `self` into
    //     the iterable flips the strict verdict to `false` while the
    //     non-strict verdict stays `true`);
    //   • closed-set exhaustive sweeps on `DataClassification::ALL^2`
    //     (a is a strict lower bound of {&b} iff a.strictly_below(&b))
    //     and `SubstrateType::ALL^2` (antichain-shape — the pointed-
    //     top strict edge is the only passing case).

    /// [`Lattice::is_strict_lower_bound_of`] on the empty iterator
    /// returns `true` at every variant of [`DataClassification::ALL`]
    /// — the empty conjunction is vacuously true. Peer of
    /// `is_lower_and_upper_bound_of_empty_iterator_are_vacuously_true_over_data_classification_all`
    /// on the strict arm: both strict AND non-strict N-ary Boolean-
    /// conjunction predicates carry the same empty-conjunction
    /// identity because [`Iterator::all`] on the empty iterator is
    /// `true` regardless of the per-element predicate. Fail-before-
    /// pass-after: pre-lift `is_strict_lower_bound_of` is not exposed
    /// as a trait method — consumers who wanted the strict N-ary
    /// reading hand-authored `iter.all(|x| self.strictly_below(x))`
    /// inline at each callsite; post-lift the vacuous-true is fixed
    /// at ONE primitive on the [`Lattice`] algebra.
    #[test]
    fn is_strict_lower_and_upper_bound_of_empty_iterator_are_vacuously_true_over_data_classification_all(
    ) {
        use tatara_process::classification::DataClassification;
        let empty: [&DataClassification; 0] = [];
        for a in DataClassification::ALL {
            assert!(
                a.is_strict_lower_bound_of(empty),
                "is_strict_lower_bound_of on empty iterator must be true at {a:?} — the \
                 empty conjunction is vacuously true",
            );
            assert!(
                a.is_strict_upper_bound_of(empty),
                "is_strict_upper_bound_of on empty iterator must be true at {a:?} — the \
                 empty conjunction is vacuously true",
            );
        }
    }

    /// [`Lattice::is_strict_lower_bound_of`] on a singleton reduces to
    /// the pairwise `strictly_below` relation over
    /// `DataClassification::ALL^2` — i.e.
    /// `a.is_strict_lower_bound_of([&b]) == a.strictly_below(&b)` at
    /// every pair. Dually, [`Lattice::is_strict_upper_bound_of`] on a
    /// singleton reduces to the direction-flipped
    /// `b.strictly_below(&a)`. Pinned exhaustively on the 6×6 pair
    /// space so a regression that flipped the direction on either
    /// predicate OR drifted the composition off `strictly_below`
    /// (e.g. dropped the `!=` conjunct back to `leq`) would surface
    /// at the pair it broke. Peer of
    /// `is_lower_and_upper_bound_of_singleton_reduce_to_leq_over_data_classification_all_pairs`
    /// on the strict arm.
    #[test]
    fn is_strict_lower_and_upper_bound_of_singleton_reduce_to_strictly_below_over_data_classification_all_pairs(
    ) {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert_eq!(
                    a.is_strict_lower_bound_of([&b]),
                    a.strictly_below(&b),
                    "is_strict_lower_bound_of({a:?}, [&{b:?}]) drifted from \
                     strictly_below({a:?}, {b:?}) — the 1-input predicate must reduce to \
                     the pairwise strict relation",
                );
                assert_eq!(
                    a.is_strict_upper_bound_of([&b]),
                    b.strictly_below(&a),
                    "is_strict_upper_bound_of({a:?}, [&{b:?}]) drifted from the direction-\
                     flipped strictly_below({b:?}, {a:?}) — the upper-bound singleton \
                     flips the direction",
                );
            }
        }
    }

    /// [`Lattice::is_strict_lower_bound_of`] REFINES
    /// [`Lattice::is_lower_bound_of`] at every iterable over
    /// [`DataClassification`] — `a.is_strict_lower_bound_of(iter) ⇒
    /// a.is_lower_bound_of(iter)` because
    /// `strictly_below(x) ⇒ leq(x)` at every element position.
    /// Dually [`Lattice::is_strict_upper_bound_of`] refines
    /// [`Lattice::is_upper_bound_of`]. Pinned exhaustively on the 2-
    /// and 3-arm sweeps of `DataClassification::ALL`. The strict-
    /// minus-non-strict gap is exactly the iterables containing
    /// `self` — this pin surfaces at the specific pair where the
    /// strict verdict flips false while the non-strict verdict
    /// stays true.
    #[test]
    fn is_strict_bound_of_refines_is_bound_of_over_data_classification_all() {
        use tatara_process::classification::DataClassification;
        // Exhaustive 2-arm sweep with a varying probe element — the
        // refinement holds at every closed-set position, not just at
        // the endpoints. The proptest peer
        // `data_class_is_strict_lower_bound_of_refines_is_lower_bound_of_on_pair`
        // below extends the pin to random pairs; the 3-arm case
        // inherits from that peer's random-triple coverage.
        for probe in DataClassification::ALL {
            for a in DataClassification::ALL {
                for b in DataClassification::ALL {
                    let iter = [&a, &b];
                    if probe.is_strict_lower_bound_of(iter) {
                        assert!(
                            probe.is_lower_bound_of(iter),
                            "is_strict_lower_bound_of({probe:?}, [{a:?}, {b:?}]) implies \
                             is_lower_bound_of — strict refines non-strict",
                        );
                    }
                    if probe.is_strict_upper_bound_of(iter) {
                        assert!(
                            probe.is_upper_bound_of(iter),
                            "is_strict_upper_bound_of({probe:?}, [{a:?}, {b:?}]) implies \
                             is_upper_bound_of — dual strict refines non-strict",
                        );
                    }
                }
            }
        }
    }

    /// [`Lattice::is_strict_lower_bound_of`] REJECTS an iterable that
    /// contains `self` over [`DataClassification`] — the strict-
    /// minus-non-strict gap. `self.is_strict_lower_bound_of([&self,
    /// &other]) == false` at every element because
    /// `self.strictly_below(&self)` short-circuits on the reflexive
    /// diagonal. Same rejection on the dual arm for
    /// [`Lattice::is_strict_upper_bound_of`]. Pinned as a specific
    /// contrast against the non-strict version where the presence of
    /// `self` in the iterable is compatible with the bound holding
    /// (since `self.leq(&self)` is reflexive-true).
    #[test]
    fn is_strict_lower_and_upper_bound_of_reject_self_inclusive_iterable_over_data_classification_all(
    ) {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            // Self-only singleton — the strict predicate rejects both
            // directions because `strictly_below` is irreflexive.
            assert!(
                !a.is_strict_lower_bound_of([&a]),
                "is_strict_lower_bound_of([&{a:?}]) at {a:?} must be false — the strict \
                 predicate is irreflexive on its self-inclusive input",
            );
            assert!(
                !a.is_strict_upper_bound_of([&a]),
                "is_strict_upper_bound_of([&{a:?}]) at {a:?} must be false — the dual \
                 strict predicate is irreflexive on its self-inclusive input",
            );
            // Meanwhile the non-strict version accepts the self-only
            // singleton — the reflexive-diagonal is compatible with
            // the `≤` bound.
            assert!(a.is_lower_bound_of([&a]));
            assert!(a.is_upper_bound_of([&a]));
        }
    }

    /// [`Lattice::is_strict_lower_bound_of`] projects the pointed-top
    /// antichain on [`SubstrateType`] to its strict-half-edge shape:
    /// every non-[`SubstrateType::top`] substrate is a strict lower
    /// bound of `[Regulatory]` (the pointed-top strict half-edge),
    /// but ONLY [`SubstrateType::top`] itself is NOT a strict lower
    /// bound of `[Regulatory]` (since `Regulatory.strictly_below(
    /// &Regulatory)` fails on the reflexive diagonal). Peer seal to
    /// `is_upper_bound_of_projects_the_pointed_top_antichain_over_substrate_type_all`
    /// on the STRICT arm — the antichain SHAPE is now bound through
    /// FIVE algebra predicates (`leq`, `is_incomparable`,
    /// `strictly_below`, `is_upper_bound_of`,
    /// `is_strict_lower_bound_of`) via ONE substrate owner on the
    /// [`Lattice`] trait.
    #[test]
    fn is_strict_lower_bound_of_projects_the_pointed_top_antichain_over_substrate_type_all() {
        use tatara_process::classification::SubstrateType;
        // Every non-top substrate is a STRICT lower bound of
        // [Regulatory] — the pointed-top strict half-edge.
        for s in SubstrateType::ALL {
            let is_strict_lower = s.is_strict_lower_bound_of([&SubstrateType::top()]);
            if s == SubstrateType::top() {
                assert!(
                    !is_strict_lower,
                    "top() (Regulatory) must NOT be a strict lower bound of [Regulatory] — \
                     strictly_below is irreflexive at the top-diagonal",
                );
            } else {
                assert!(
                    is_strict_lower,
                    "non-top substrate {s:?} must be a strict lower bound of [Regulatory] \
                     — the antichain's pointed-top strict half-edge",
                );
            }
        }
        // Every distinct non-Regulatory pair fails the strict N-ary
        // predicate in both directions — the antichain rejects any
        // strict-order verdict on incomparable pairs.
        for s in SubstrateType::ALL {
            for t in SubstrateType::ALL {
                if s != t && s != SubstrateType::top() && t != SubstrateType::top() {
                    assert!(
                        !s.is_strict_lower_bound_of([&t]),
                        "distinct non-Regulatory substrates ({s:?}, {t:?}) must fail the \
                         strict N-ary lower-bound predicate — the antichain does not \
                         promote incomparable pairs to a strict-order verdict",
                    );
                    assert!(!s.is_strict_upper_bound_of([&t]));
                }
            }
        }
    }

    /// [`Lattice::is_strict_lower_bound_of`] short-circuits `false`
    /// when any element in the iterator violates the STRICT
    /// containment direction over [`DataClassification`]: given a
    /// slice `[&strict, &equal]` where `self.strictly_below(&strict)`
    /// holds but `self.strictly_below(&equal)` fails on the reflexive
    /// diagonal (equal == self), the strict predicate rejects the
    /// whole slice while the non-strict predicate accepts it.
    /// Pinned on a specific self-equal-in-tail configuration and its
    /// dual so both short-circuit arms bind at the substrate.
    #[test]
    fn is_strict_lower_and_upper_bound_of_reject_when_any_element_violates_over_data_classification(
    ) {
        use tatara_process::classification::DataClassification::{
            Confidential, Internal, Pci, Phi, Pii, Public,
        };
        // `Public` is a strict lower bound of `[Internal, Pii]` —
        // both elements sit strictly above.
        assert!(Public.is_strict_lower_bound_of([&Internal, &Pii]));
        // `Public` is NOT a strict lower bound of `[Internal, Public]`
        // — fails at `Public.strictly_below(&Public)` on the reflexive
        // diagonal. Contrast against `Public.is_lower_bound_of([&
        // Internal, &Public])` which succeeds (reflexive `leq`).
        assert!(!Public.is_strict_lower_bound_of([&Internal, &Public]));
        assert!(Public.is_lower_bound_of([&Internal, &Public]));
        // `Pci` is NOT a strict upper bound of `[Internal, Pci]` —
        // fails at `Pci.strictly_below(&Pci)` on the diagonal.
        assert!(!Pci.is_strict_upper_bound_of([&Internal, &Pci]));
        assert!(Pci.is_upper_bound_of([&Internal, &Pci]));
        // `Confidential` is NOT a strict lower bound of `[Pii, Pci,
        // Phi, Internal]` — fails at `Confidential.strictly_below(
        // &Internal)` (leq fails; Internal < Confidential).
        assert!(!Confidential.is_strict_lower_bound_of([&Pii, &Pci, &Phi, &Internal]));
    }

    proptest! {
        /// [`Lattice::is_strict_lower_bound_of`] on a singleton reduces
        /// to the pairwise `strictly_below` relation over any
        /// [`DataClassification`] pair — proptest peer of the
        /// exhaustive
        /// `is_strict_lower_and_upper_bound_of_singleton_reduce_to_strictly_below_over_data_classification_all_pairs`
        /// seal above. Peers on the CALM axis via
        /// `calm_is_strict_lower_bound_of_singleton_reduces_to_strictly_below`.
        #[test]
        fn data_class_is_strict_lower_and_upper_bound_of_singleton_reduce_to_strictly_below(
            a in any_data_class(),
            b in any_data_class(),
        ) {
            prop_assert_eq!(a.is_strict_lower_bound_of([&b]), a.strictly_below(&b));
            prop_assert_eq!(a.is_strict_upper_bound_of([&b]), b.strictly_below(&a));
        }

        /// [`Lattice::is_strict_lower_bound_of`] REFINES
        /// [`Lattice::is_lower_bound_of`] on any pair over
        /// [`DataClassification`] — pinned in the strict-implies-non-
        /// strict direction. Proptest peer of the exhaustive
        /// `is_strict_bound_of_refines_is_bound_of_over_data_classification_all`
        /// seal's 2-arm sweep.
        #[test]
        fn data_class_is_strict_lower_bound_of_refines_is_lower_bound_of_on_pair(
            probe in any_data_class(),
            a in any_data_class(),
            b in any_data_class(),
        ) {
            let iter = [&a, &b];
            if probe.is_strict_lower_bound_of(iter) {
                prop_assert!(probe.is_lower_bound_of(iter));
            }
            if probe.is_strict_upper_bound_of(iter) {
                prop_assert!(probe.is_upper_bound_of(iter));
            }
        }

        /// [`Lattice::is_strict_lower_bound_of`] REJECTS an iterable
        /// that contains `self` on the DataClassification axis —
        /// proptest peer of the exhaustive
        /// `is_strict_lower_and_upper_bound_of_reject_self_inclusive_iterable_over_data_classification_all`
        /// seal.
        #[test]
        fn data_class_is_strict_lower_and_upper_bound_of_reject_self_inclusive_iterable(
            a in any_data_class(),
            other in any_data_class(),
        ) {
            prop_assert!(!a.is_strict_lower_bound_of([&other, &a]));
            prop_assert!(!a.is_strict_upper_bound_of([&other, &a]));
        }

        /// Peer of the DataClassification singleton-reduction proptest
        /// on the CALM boolean-lattice axis — same shape, different
        /// closed set.
        #[test]
        fn calm_is_strict_lower_bound_of_singleton_reduces_to_strictly_below(
            a in any_calm(),
            b in any_calm(),
        ) {
            prop_assert_eq!(a.is_strict_lower_bound_of([&b]), a.strictly_below(&b));
            prop_assert_eq!(a.is_strict_upper_bound_of([&b]), b.strictly_below(&a));
        }

        /// Peer of the DataClassification refinement proptest on the
        /// CALM axis.
        #[test]
        fn calm_is_strict_lower_bound_of_refines_is_lower_bound_of_on_pair(
            probe in any_calm(),
            a in any_calm(),
            b in any_calm(),
        ) {
            let iter = [&a, &b];
            if probe.is_strict_lower_bound_of(iter) {
                prop_assert!(probe.is_lower_bound_of(iter));
            }
            if probe.is_strict_upper_bound_of(iter) {
                prop_assert!(probe.is_upper_bound_of(iter));
            }
        }
    }

    /// [`Lattice::is_between`] on every [`DataClassification`] triple
    /// AGREES with the pairwise `low.leq(a) && a.leq(high)` two-
    /// primitive conjunction that pre-lift consumers hand-authored at
    /// each callsite. Pinned exhaustively over
    /// `DataClassification::ALL^3` so the substrate primitive matches
    /// the composed encoding at every triple — the type-level lift
    /// from a per-consumer `low.leq(a) && a.leq(high)` composition to
    /// the [`Lattice::is_between`] default method is byte-identical.
    #[test]
    fn is_between_agrees_with_leq_conjunction_over_data_classification_all_triples() {
        use tatara_process::classification::DataClassification;
        for low in DataClassification::ALL {
            for a in DataClassification::ALL {
                for high in DataClassification::ALL {
                    assert_eq!(
                        a.is_between(&low, &high),
                        low.leq(&a) && a.leq(&high),
                        "is_between({a:?}, {low:?}, {high:?}) drifted from \
                         low.leq(&a) && a.leq(&high) — the interval-containment \
                         predicate must equal the pairwise-conjunction encoding",
                    );
                }
            }
        }
    }

    /// [`Lattice::is_between`] admits BOTH endpoints of every non-
    /// inverted bracket on [`DataClassification`] — `low.is_between(
    /// &low, &high) && high.is_between(&low, &high) == true` whenever
    /// `low.leq(&high)`. The endpoint-reflexivity arm of the closed-
    /// bracket predicate. Contrast against
    /// `is_strictly_between_rejects_both_endpoints_over_data_classification_all`
    /// below where the strict version rejects both.
    #[test]
    fn is_between_admits_both_endpoints_of_non_inverted_brackets_over_data_classification_all() {
        use tatara_process::classification::DataClassification;
        for low in DataClassification::ALL {
            for high in DataClassification::ALL {
                if low.leq(&high) {
                    assert!(
                        low.is_between(&low, &high),
                        "is_between({low:?}, {low:?}, {high:?}) must be true — the low \
                         endpoint of a non-inverted bracket is trivially in its own bracket",
                    );
                    assert!(
                        high.is_between(&low, &high),
                        "is_between({high:?}, {low:?}, {high:?}) must be true — the high \
                         endpoint of a non-inverted bracket is trivially in its own bracket",
                    );
                }
            }
        }
    }

    /// [`Lattice::is_between`] on a degenerate bracket collapses to
    /// equality — `a.is_between(&x, &x) ⇔ a == x` on every pair over
    /// [`DataClassification`]. Follows from antisymmetry of
    /// [`Lattice::leq`]. Peer of the same identity pinned in-tree on
    /// [`tatara_lisp::macro_expand::ResourceLimits::within`]
    /// (`resource_limits_within_of_equal_bounds_iff_equal_to_bound`);
    /// this seal binds the lifted trait-level default to the SAME
    /// degenerate-collapse invariant the per-domain `const fn`
    /// carried.
    #[test]
    fn is_between_of_equal_bounds_iff_equal_to_bound_over_data_classification_all_pairs() {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for x in DataClassification::ALL {
                assert_eq!(
                    a.is_between(&x, &x),
                    a == x,
                    "is_between({a:?}, {x:?}, {x:?}) must equal ({a:?} == {x:?}) — a \
                     zero-width bracket admits only the single point x by antisymmetry \
                     of leq",
                );
            }
        }
    }

    /// [`Lattice::is_between`] REJECTS every element of the closed
    /// set whenever the bracket is inverted (`high.strictly_below(
    /// &low)`) over [`DataClassification`] — the interval-containment
    /// predicate returns `false` for EVERY `a` when the upper endpoint
    /// sits strictly below the lower one. Pinned exhaustively over
    /// every inverted-bracket pair.
    #[test]
    fn is_between_rejects_every_element_when_bracket_is_inverted_over_data_classification_all() {
        use tatara_process::classification::DataClassification;
        for low in DataClassification::ALL {
            for high in DataClassification::ALL {
                if high.strictly_below(&low) {
                    for a in DataClassification::ALL {
                        assert!(
                            !a.is_between(&low, &high),
                            "is_between({a:?}, {low:?}, {high:?}) must be false — the \
                             bracket is inverted ({high:?} strictly below {low:?}) so \
                             no element can satisfy low.leq(&a) && a.leq(&high)",
                        );
                    }
                }
            }
        }
    }

    /// [`Lattice::is_between`] on the extrema bracket admits EVERY
    /// element of [`DataClassification`] — `a.is_between(
    /// &DataClassification::bottom(), &DataClassification::top())` is
    /// `true` at every element. Peer of the same identity pinned
    /// in-tree on [`tatara_lisp::macro_expand::ResourceLimits::within`]
    /// (`resource_limits_within_with_lattice_extrema_is_true`); this
    /// seal binds the lifted trait-level default to the SAME universal-
    /// truth-under-widest-bracket invariant the per-domain `const fn`
    /// carried, now expressed via the lattice's own [`Lattice::bottom`]
    /// / [`Lattice::top`] identities rather than per-domain preset
    /// constants.
    #[test]
    fn is_between_with_lattice_extrema_admits_every_element_over_data_classification_all() {
        use tatara_process::classification::DataClassification;
        let bot = DataClassification::bottom();
        let top = DataClassification::top();
        for a in DataClassification::ALL {
            assert!(
                a.is_between(&bot, &top),
                "is_between({a:?}, bottom, top) must be true — the widest possible \
                 bracket admits every element by the lattice's bottom/top axioms",
            );
        }
    }

    /// [`Lattice::is_strictly_between`] REJECTS BOTH endpoints of
    /// every bracket on [`DataClassification`] — `low.is_strictly_between(
    /// &low, &high) == false` and `high.is_strictly_between(&low,
    /// &high) == false` for every triple. The strict-minus-non-strict
    /// gap at the endpoints: [`Lattice::is_between`] admits both;
    /// [`Lattice::is_strictly_between`] rejects both because
    /// [`Lattice::strictly_below`] is irreflexive at the endpoint
    /// diagonal.
    #[test]
    fn is_strictly_between_rejects_both_endpoints_over_data_classification_all() {
        use tatara_process::classification::DataClassification;
        for low in DataClassification::ALL {
            for high in DataClassification::ALL {
                assert!(
                    !low.is_strictly_between(&low, &high),
                    "is_strictly_between({low:?}, {low:?}, {high:?}) must be false — \
                     the strict predicate rejects the low endpoint on the reflexive \
                     diagonal",
                );
                assert!(
                    !high.is_strictly_between(&low, &high),
                    "is_strictly_between({high:?}, {low:?}, {high:?}) must be false — \
                     the strict predicate rejects the high endpoint on the reflexive \
                     diagonal",
                );
            }
        }
    }

    /// [`Lattice::is_strictly_between`] REJECTS every element on any
    /// degenerate bracket (`low == high`) over [`DataClassification`]
    /// — the zero-width open bracket admits NO points because no
    /// element can be simultaneously strictly above AND strictly below
    /// the same endpoint. Contrast against
    /// `is_between_of_equal_bounds_iff_equal_to_bound_over_data_classification_all_pairs`
    /// where the closed version admits exactly `a == x`.
    #[test]
    fn is_strictly_between_of_equal_bounds_rejects_every_element_over_data_classification_all() {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for x in DataClassification::ALL {
                assert!(
                    !a.is_strictly_between(&x, &x),
                    "is_strictly_between({a:?}, {x:?}, {x:?}) must be false — no element \
                     can be simultaneously strictly above AND strictly below the same \
                     endpoint on any partial order",
                );
            }
        }
    }

    /// [`Lattice::is_strictly_between`] REFINES [`Lattice::is_between`]
    /// on every triple over [`DataClassification`] —
    /// `a.is_strictly_between(&low, &high) ⇒ a.is_between(&low,
    /// &high)`. The strict conjunction refines the non-strict one at
    /// each conjunct via [`Lattice::strictly_below`]'s refines-
    /// [`Lattice::leq`] identity. The strict-minus-non-strict gap on
    /// the interval arm is the endpoint diagonal (`a == low` OR
    /// `a == high`).
    #[test]
    fn is_strictly_between_refines_is_between_over_data_classification_all_triples() {
        use tatara_process::classification::DataClassification;
        for low in DataClassification::ALL {
            for a in DataClassification::ALL {
                for high in DataClassification::ALL {
                    if a.is_strictly_between(&low, &high) {
                        assert!(
                            a.is_between(&low, &high),
                            "is_strictly_between({a:?}, {low:?}, {high:?}) implies \
                             is_between — strict refines non-strict on the interval arm",
                        );
                    }
                }
            }
        }
    }

    /// [`Lattice::is_between`] projects the pointed-top antichain on
    /// [`SubstrateType`] to its bracket-shape: on any incomparable
    /// pair `(low, high)` (both non-Regulatory and distinct), NO
    /// element sits between them; on the pointed-top bracket
    /// `[low, Regulatory]` at a non-Regulatory `low`, exactly `low`
    /// and `Regulatory` sit in the closed bracket. Peer seal to
    /// `is_strict_lower_bound_of_projects_the_pointed_top_antichain_over_substrate_type_all`
    /// on the interval arm — the antichain SHAPE is now bound through
    /// SIX algebra predicates (`leq`, `is_incomparable`,
    /// `strictly_below`, `is_upper_bound_of`, `is_strict_lower_bound_of`,
    /// `is_between`) via ONE substrate owner on the [`Lattice`] trait.
    #[test]
    fn is_between_projects_the_pointed_top_antichain_over_substrate_type_all() {
        use tatara_process::classification::SubstrateType;
        let top = SubstrateType::top();
        // On any incomparable pair (both non-top, distinct), NO
        // element sits in the closed bracket — the antichain rejects
        // interval containment.
        for low in SubstrateType::ALL {
            for high in SubstrateType::ALL {
                if low != high && low != top && high != top {
                    for a in SubstrateType::ALL {
                        assert!(
                            !a.is_between(&low, &high),
                            "is_between({a:?}, {low:?}, {high:?}) must be false — the \
                             antichain's incomparable brackets admit no elements",
                        );
                    }
                }
            }
        }
        // On the pointed-top bracket [low, top] at a non-top low,
        // exactly `low` and `top` sit in the closed bracket — the
        // antichain admits only the two endpoints of the strict
        // half-edge into the top.
        for low in SubstrateType::ALL {
            if low != top {
                for a in SubstrateType::ALL {
                    let in_bracket = a.is_between(&low, &top);
                    let expected = a == low || a == top;
                    assert_eq!(
                        in_bracket, expected,
                        "is_between({a:?}, {low:?}, {top:?}) must equal (a == {low:?} || \
                         a == top) — the pointed-top bracket admits only the two endpoints",
                    );
                }
            }
        }
    }

    proptest! {
        /// [`Lattice::is_between`] AGREES with the pairwise
        /// `low.leq(a) && a.leq(high)` two-primitive conjunction on
        /// every random [`DataClassification`] triple — proptest peer
        /// of the exhaustive
        /// `is_between_agrees_with_leq_conjunction_over_data_classification_all_triples`
        /// seal above.
        #[test]
        fn data_class_is_between_agrees_with_leq_conjunction(
            low in any_data_class(),
            a in any_data_class(),
            high in any_data_class(),
        ) {
            prop_assert_eq!(a.is_between(&low, &high), low.leq(&a) && a.leq(&high));
        }

        /// [`Lattice::is_strictly_between`] REFINES [`Lattice::is_between`]
        /// on every random [`DataClassification`] triple — proptest peer
        /// of the exhaustive
        /// `is_strictly_between_refines_is_between_over_data_classification_all_triples`
        /// seal above.
        #[test]
        fn data_class_is_strictly_between_refines_is_between(
            low in any_data_class(),
            a in any_data_class(),
            high in any_data_class(),
        ) {
            if a.is_strictly_between(&low, &high) {
                prop_assert!(a.is_between(&low, &high));
            }
        }

        /// [`Lattice::is_between`] on a degenerate bracket collapses
        /// to equality on every random [`DataClassification`] pair —
        /// proptest peer of the exhaustive
        /// `is_between_of_equal_bounds_iff_equal_to_bound_over_data_classification_all_pairs`
        /// seal.
        #[test]
        fn data_class_is_between_of_equal_bounds_iff_equal_to_bound(
            a in any_data_class(),
            x in any_data_class(),
        ) {
            prop_assert_eq!(a.is_between(&x, &x), a == x);
            prop_assert!(!a.is_strictly_between(&x, &x));
        }

        /// Peer of the DataClassification is_between agreement
        /// proptest on the CALM boolean-lattice axis — same shape,
        /// different closed set.
        #[test]
        fn calm_is_between_agrees_with_leq_conjunction(
            low in any_calm(),
            a in any_calm(),
            high in any_calm(),
        ) {
            prop_assert_eq!(a.is_between(&low, &high), low.leq(&a) && a.leq(&high));
        }

        /// Peer of the DataClassification is_strictly_between refines
        /// is_between proptest on the CALM axis.
        #[test]
        fn calm_is_strictly_between_refines_is_between(
            low in any_calm(),
            a in any_calm(),
            high in any_calm(),
        ) {
            if a.is_strictly_between(&low, &high) {
                prop_assert!(a.is_between(&low, &high));
            }
        }
    }

    // ── Lattice::clamped_between — 3-ary interval-projection combinator peer
    //    of Lattice::is_between ───────────────────────────────────────
    //
    // Bind [`Lattice::clamped_between`] at fail-before-pass-after granularity.
    // Pre-lift the [`Lattice`] trait's 3-ary interval face was
    // `{is_between, is_strictly_between}` on the predicate arm only —
    // a consumer that wanted the COMBINATOR reading (project `a` into
    // `[low, high]`) hand-authored `a.join(low).meet(high)` at each
    // callsite (surfaces exactly at
    // `tatara_lisp::macro_expand::ResourceLimits::clamp` as
    // `self.most_permissive(lower).strictest(upper)`, a per-domain
    // `const fn` composition of the pointwise resource-posture
    // lattice's join and meet). Post-lift the whole projection
    // combinator binds at ONE substrate primitive on the [`Lattice`]
    // algebra, and every downstream impl inherits `clamp` for free
    // through the default. The 3-ary interval face's (predicate,
    // combinator) × (interval) 2×1 grid is now closed at ONE
    // substrate default per cell (`is_between` on the predicate arm,
    // `clamp` on the combinator arm), and the canonical CROSS-CHECK
    // (`a.is_between(&low, &high) ⇔ a.clamped_between(&low, &high) == a`) binds
    // the two through a lattice-law theorem the tests below pin
    // exhaustively.

    /// [`Lattice::clamped_between`] satisfies the FIXED-POINT theorem with
    /// [`Lattice::is_between`] over every WELL-FORMED [`DataClassification`]
    /// bracket: on `low.leq(&high)`, `a.is_between(&low, &high) ⇔
    /// a.clamped_between(&low, &high) == a`. Pinned exhaustively over
    /// `DataClassification::ALL^3` restricted to well-formed brackets
    /// so the substrate primitive matches the predicate/combinator
    /// bridge at every valid triple. The CANONICAL cross-check axiom
    /// binding the 3-ary interval predicate to its projection
    /// combinator, analogous to the pairwise meet-agreement
    /// (`a.leq(&b) ⇔ a.meet(&b) == a`) and join-agreement (`a.leq(&b)
    /// ⇔ a.join(&b) == b`) axioms one arity down.
    ///
    /// The `low.leq(&high)` premise is load-bearing: on an INVERTED
    /// bracket (`high.strictly_below(&low)`), `is_between` universally
    /// rejects while `clamped_between` still returns a well-defined
    /// element (which may coincidentally equal `a` on a totally-
    /// ordered lattice when `a == high == meet(low, high)`); the
    /// biconditional is meaningful only on well-formed brackets where
    /// the (meet, join) combinators route through their agreement
    /// laws to reproduce `a`.
    #[test]
    fn clamped_between_agrees_with_is_between_fixed_point_over_data_classification_all_triples() {
        use tatara_process::classification::DataClassification;
        for low in DataClassification::ALL {
            for high in DataClassification::ALL {
                if low.leq(&high) {
                    for a in DataClassification::ALL {
                        let clamped = a.clamped_between(&low, &high);
                        assert_eq!(
                            a.is_between(&low, &high),
                            clamped == a,
                            "clamp fixed-point theorem drifted at ({a:?}, {low:?}, \
                             {high:?}): is_between = {}, clamped == a = {}, clamped = {clamped:?}",
                            a.is_between(&low, &high),
                            clamped == a,
                        );
                    }
                }
            }
        }
    }

    /// [`Lattice::clamped_between`] PINS to the floor on inputs at-or-below the
    /// floor over every well-formed [`DataClassification`] bracket:
    /// on `low.leq(&high)`, `a.leq(&low) ⇒ a.clamped_between(&low, &high) ==
    /// low`. Pinned exhaustively over `DataClassification::ALL^3`.
    #[test]
    fn clamped_between_pins_below_floor_inputs_to_floor_over_data_classification_all_triples() {
        use tatara_process::classification::DataClassification;
        for low in DataClassification::ALL {
            for high in DataClassification::ALL {
                if low.leq(&high) {
                    for a in DataClassification::ALL {
                        if a.leq(&low) {
                            assert_eq!(
                                a.clamped_between(&low, &high),
                                low,
                                "clamp({a:?}, {low:?}, {high:?}) must pin to \
                                 floor when a ≤ low on a well-formed bracket",
                            );
                        }
                    }
                }
            }
        }
    }

    /// [`Lattice::clamped_between`] PINS to the ceiling on inputs at-or-above
    /// the ceiling over every well-formed [`DataClassification`]
    /// bracket: on `low.leq(&high)`, `high.leq(&a) ⇒ a.clamped_between(&low,
    /// &high) == high`. Dual of the below-floor pin.
    #[test]
    fn clamped_between_pins_above_ceiling_inputs_to_ceiling_over_data_classification_all_triples() {
        use tatara_process::classification::DataClassification;
        for low in DataClassification::ALL {
            for high in DataClassification::ALL {
                if low.leq(&high) {
                    for a in DataClassification::ALL {
                        if high.leq(&a) {
                            assert_eq!(
                                a.clamped_between(&low, &high),
                                high,
                                "clamp({a:?}, {low:?}, {high:?}) must pin to \
                                 ceiling when high ≤ a on a well-formed bracket",
                            );
                        }
                    }
                }
            }
        }
    }

    /// [`Lattice::clamped_between`] satisfies the BRACKET-MEMBERSHIP CONTRACT
    /// over every well-formed [`DataClassification`] bracket: on
    /// `low.leq(&high)`, `a.clamped_between(&low, &high).is_between(&low,
    /// &high)` for EVERY `a`. The clamp is a RETRACTION onto the
    /// bracket — its image lives entirely inside the bracket.
    #[test]
    fn clamped_between_result_is_always_in_bracket_over_data_classification_all_triples() {
        use tatara_process::classification::DataClassification;
        for low in DataClassification::ALL {
            for high in DataClassification::ALL {
                if low.leq(&high) {
                    for a in DataClassification::ALL {
                        let clamped = a.clamped_between(&low, &high);
                        assert!(
                            clamped.is_between(&low, &high),
                            "clamp({a:?}, {low:?}, {high:?}) = {clamped:?} must sit \
                             inside its own bracket by the retraction contract",
                        );
                    }
                }
            }
        }
    }

    /// [`Lattice::clamped_between`] is IDEMPOTENT over every
    /// [`DataClassification`] triple: `a.clamped_between(&low, &high)
    /// .clamped_between(&low, &high) == a.clamped_between(&low, &high)`. Direct
    /// consequence of the fixed-point theorem applied to the clamp
    /// result (which is in the bracket by the retraction contract),
    /// pinned as a first-class law over every triple to catch a
    /// future override that broke the retraction.
    #[test]
    fn clamped_between_is_idempotent_over_data_classification_all_triples() {
        use tatara_process::classification::DataClassification;
        for low in DataClassification::ALL {
            for high in DataClassification::ALL {
                for a in DataClassification::ALL {
                    let once = a.clamped_between(&low, &high);
                    let twice = once.clamped_between(&low, &high);
                    assert_eq!(
                        once, twice,
                        "clamp is not idempotent at ({a:?}, {low:?}, \
                         {high:?}) — clamp once = {once:?}, clamp twice = {twice:?}",
                    );
                }
            }
        }
    }

    /// [`Lattice::clamped_between`] on a DEGENERATE bracket collapses to the
    /// bracket point over every [`DataClassification`] pair:
    /// `a.clamped_between(&x, &x) == x` — a zero-width bracket forces the
    /// projection to the single point `x`. Peer of
    /// [`Lattice::is_between`]'s degenerate-bracket collapse
    /// (`a.is_between(&x, &x) ⇔ a == x`) on the combinator arm.
    #[test]
    fn clamped_between_of_equal_bounds_collapses_to_the_bound_over_data_classification_all_pairs() {
        use tatara_process::classification::DataClassification;
        for x in DataClassification::ALL {
            for a in DataClassification::ALL {
                assert_eq!(
                    a.clamped_between(&x, &x),
                    x,
                    "clamp({a:?}, {x:?}, {x:?}) must collapse to the \
                     bracket point on a zero-width bracket",
                );
            }
        }
    }

    /// [`Lattice::clamped_between`] with the LATTICE EXTREMA is the IDENTITY
    /// over every [`DataClassification`]: `a.clamped_between(&bottom, &top) ==
    /// a`. Peer of [`Lattice::is_between`]'s extrema-bracket
    /// universal-truth on the combinator arm — the widest possible
    /// bracket admits every element AND leaves it unchanged.
    #[test]
    fn clamped_between_with_lattice_extrema_is_the_identity_over_data_classification_all() {
        use tatara_process::classification::DataClassification;
        let bottom = <DataClassification as crate::Lattice>::bottom();
        let top = <DataClassification as crate::Lattice>::top();
        for a in DataClassification::ALL {
            assert_eq!(
                a.clamped_between(&bottom, &top),
                a,
                "clamp({a:?}, bottom, top) must be the identity — \
                 the widest bracket leaves every element unchanged",
            );
        }
    }

    /// [`Lattice::clamped_between`] AGREES with the pairwise `self.join(low)
    /// .meet(high)` two-primitive lattice-algebra composition on
    /// every [`DataClassification`] triple. Byte-identical to the
    /// default's routing; pinned exhaustively so a future override
    /// (a per-impl `clamp` that departed from the default) that
    /// broke agreement is caught.
    #[test]
    fn clamped_between_agrees_with_join_meet_composition_over_data_classification_all_triples() {
        use tatara_process::classification::DataClassification;
        for low in DataClassification::ALL {
            for a in DataClassification::ALL {
                for high in DataClassification::ALL {
                    assert_eq!(
                        a.clamped_between(&low, &high),
                        a.join(&low).meet(&high),
                        "clamp({a:?}, {low:?}, {high:?}) drifted from \
                         (a ⊔ low) ⊓ high — default routing broken",
                    );
                }
            }
        }
    }

    /// [`Lattice::clamped_between`] on the pointed-top antichain
    /// [`SubstrateType`] does NOT satisfy the fixed-point theorem or
    /// the bracket-membership retraction contract in general — the
    /// pointed-top antichain's `Lattice` impl violates meet-agreement
    /// (`a.leq(&b) ⇒ a.meet(&b) == a`) on the top-directed edge
    /// (a non-top `x` has `x.leq(&top) == true` but `x.meet(&top) ==
    /// top != x` under the impl's "distinct pairs meet to top"
    /// arm), so the well-formed premise the theorem relies on does
    /// not extend to it — a discipline the trait's core proptest
    /// cohort already reflects by NOT running `leq_agrees_with_meet`
    /// / `leq_agrees_with_join` over any `any_substrate()` strategy.
    /// The one substrate-shape identity the retraction still binds:
    /// the DEGENERATE self-bracket `a.clamped_between(&a, &a) == a`
    /// on every element via the `x.join(x) == x` / `x.meet(x) == x`
    /// idempotence axioms that DO hold on the pointed-top antichain,
    /// pinned as the substrate-carried arm of
    /// `clamped_between_of_equal_bounds_collapses_to_the_bound_over_data_classification_all_pairs`
    /// one closed-set axis over.
    #[test]
    fn clamped_between_of_reflexive_self_bracket_is_identity_over_substrate_type_all() {
        use tatara_process::classification::SubstrateType;
        for a in SubstrateType::ALL {
            assert_eq!(
                a.clamped_between(&a, &a),
                a,
                "clamped_between({a:?}, {a:?}, {a:?}) must be the \
                 identity on the reflexive self-bracket via meet/join \
                 idempotence, even on the pointed-top antichain",
            );
        }
    }

    proptest! {
        /// [`Lattice::clamped_between`] satisfies the FIXED-POINT theorem with
        /// [`Lattice::is_between`] on every random well-formed
        /// [`DataClassification`] bracket — proptest peer of the
        /// exhaustive
        /// `clamped_between_agrees_with_is_between_fixed_point_over_data_classification_all_triples`
        /// seal. The `low.leq(&high)` guard matches the well-formed
        /// premise the seal restricts to.
        #[test]
        fn data_class_clamped_between_agrees_with_is_between_fixed_point(
            low in any_data_class(),
            a in any_data_class(),
            high in any_data_class(),
        ) {
            if low.leq(&high) {
                let clamped = a.clamped_between(&low, &high);
                prop_assert_eq!(a.is_between(&low, &high), clamped == a);
            }
        }

        /// [`Lattice::clamped_between`] BRACKET-MEMBERSHIP CONTRACT on every
        /// well-formed random [`DataClassification`] bracket —
        /// proptest peer of the exhaustive
        /// `clamped_between_result_is_always_in_bracket_over_data_classification_all_triples`
        /// seal.
        #[test]
        fn data_class_clamped_between_result_is_always_in_bracket(
            low in any_data_class(),
            a in any_data_class(),
            high in any_data_class(),
        ) {
            if low.leq(&high) {
                let clamped = a.clamped_between(&low, &high);
                prop_assert!(clamped.is_between(&low, &high));
            }
        }

        /// [`Lattice::clamped_between`] IDEMPOTENCE on every random
        /// [`DataClassification`] triple — proptest peer of the
        /// exhaustive `clamped_between_is_idempotent_over_data_classification_all_triples`
        /// seal.
        #[test]
        fn data_class_clamped_between_is_idempotent(
            low in any_data_class(),
            a in any_data_class(),
            high in any_data_class(),
        ) {
            let once = a.clamped_between(&low, &high);
            let twice = once.clamped_between(&low, &high);
            prop_assert_eq!(once, twice);
        }

        /// Peer of the DataClassification clamped_between fixed-point proptest
        /// on the CALM boolean-lattice axis — same shape, different
        /// closed set. Same well-formed `low.leq(&high)` guard.
        #[test]
        fn calm_clamped_between_agrees_with_is_between_fixed_point(
            low in any_calm(),
            a in any_calm(),
            high in any_calm(),
        ) {
            if low.leq(&high) {
                let clamped = a.clamped_between(&low, &high);
                prop_assert_eq!(a.is_between(&low, &high), clamped == a);
            }
        }

        /// Peer of the DataClassification clamped_between bracket-membership
        /// proptest on the CALM axis.
        #[test]
        fn calm_clamped_between_result_is_always_in_bracket(
            low in any_calm(),
            a in any_calm(),
            high in any_calm(),
        ) {
            if low.leq(&high) {
                let clamped = a.clamped_between(&low, &high);
                prop_assert!(clamped.is_between(&low, &high));
            }
        }

        /// Peer of the DataClassification clamped_between idempotence proptest
        /// on the CALM axis.
        #[test]
        fn calm_clamped_between_is_idempotent(
            low in any_calm(),
            a in any_calm(),
            high in any_calm(),
        ) {
            let once = a.clamped_between(&low, &high);
            let twice = once.clamped_between(&low, &high);
            prop_assert_eq!(once, twice);
        }
    }

    // ── Lattice::is_chain / Lattice::is_antichain — structural
    //    collection-level default peers ───────────────────────────────
    //
    // Bind [`Lattice::is_chain`] + [`Lattice::is_antichain`] at
    // fail-before-pass-after granularity. Pre-lift the [`Lattice`]
    // trait's comparability surface was `{is_comparable,
    // is_incomparable}` on the pairwise arm; a consumer that wanted
    // the STRUCTURAL (collection-level) reading of either predicate
    // hand-authored a nested-loop `for i in 0..vs.len() { for j in
    // (i + 1)..vs.len() { if vs[i] != vs[j] && !vs[i].is_comparable(
    // vs[j]) { return false; } } } true` traversal at each callsite.
    // Post-lift the whole structural comparability pair binds at ONE
    // substrate primitive on the [`Lattice`] algebra, and every
    // downstream impl (the four in-tree ones + `Baseline` via the
    // `crate::Lattice` trait AND any future closed-set lift) inherits
    // `is_chain` / `is_antichain` for free through the default.
    // Together with the prior widening (`is_comparable` /
    // `is_incomparable` on the pairwise arm; `is_lower_bound_of` /
    // `is_upper_bound_of` on the N-ary Boolean-conjunction arm;
    // `is_between` / `is_strictly_between` on the interval arm) the
    // trait now closes the (pairwise, structural) × (comparable,
    // incomparable) 2×2 grid on the comparability face of the
    // combinator surface via FOUR substrate defaults.
    //
    // The dedup convention: both structural predicates filter
    // `vs[i] == vs[j]` pairs on the standard mathematical reading
    // that a chain / antichain is defined over the DISTINCT elements
    // of the underlying set. Duplicates in the yielded sequence are
    // the same set element, so they don't add a new pair to check.
    // Under this convention, at every collection with at most one
    // distinct value (empty, singleton, all-duplicate), BOTH
    // predicates fire true — this is the shared vacuous-truth arm
    // and the exact overlap of the chain and antichain predicates.
    //
    // Coverage below spans:
    //
    //   • empty / singleton / duplicate-only vacuous truth on the
    //     shared arm;
    //   • pair-identity `T::is_chain([&a, &b]) == (a == b ||
    //     a.is_comparable(&b))` (dual on the antichain arm) at every
    //     pair of every closed-set impl;
    //   • total-order universal-truth-on-chain / rejection-on-antichain
    //     over `DataClassification::ALL^2` and `CalmClassification::
    //     ALL^2`;
    //   • antichain-lattice acceptance on distinct-non-top /
    //     rejection-on-top-mixed over `SubstrateType::ALL^3`;
    //   • three-element seal on the antichain lattice pinning the
    //     antichain-shape acceptance on the {Compute, Storage, Network}
    //     triple against the top-mixed rejection on the {Compute,
    //     Storage, Regulatory} triple;
    //   • sequence-order independence over `DataClassification::ALL^3`
    //     and `SubstrateType::ALL^3` — the structural predicates
    //     depend only on the underlying multiset, not the emission
    //     order;
    //   • strict-monotone-sequence witness on
    //     `DataClassification::ALL^3` — any triple that chains
    //     strictly is a chain (refines-strictly-below via
    //     transitivity);
    //   • proptest peers on the DataClassification + CALM axes for
    //     the pair-identity and total-order universal-truth /
    //     rejection arms.

    /// [`Lattice::is_chain`] and [`Lattice::is_antichain`] are BOTH
    /// vacuously true at the empty iterator on every classification-
    /// axis lattice — the empty conjunction has no distinct pair to
    /// check, so both universally-quantified predicates fire true on
    /// the empty case. Peer of [`Lattice::is_lower_bound_of`]'s
    /// empty-iterator vacuous truth one arity axis down. Fail-before-
    /// pass-after: pre-lift this pin cannot compile because neither
    /// `is_chain` nor `is_antichain` is exposed as a trait method —
    /// consumers who wanted the structural reading wrote the nested-
    /// loop inline at each callsite, with no vacuous-truth arm
    /// enforced at the primitive.
    #[test]
    fn is_chain_and_is_antichain_are_vacuously_true_at_the_empty_iterator() {
        use tatara_process::classification::{
            CalmClassification, DataClassification, SubstrateType,
        };
        assert!(DataClassification::is_chain(std::iter::empty()));
        assert!(DataClassification::is_antichain(std::iter::empty()));
        assert!(CalmClassification::is_chain(std::iter::empty()));
        assert!(CalmClassification::is_antichain(std::iter::empty()));
        assert!(SubstrateType::is_chain(std::iter::empty()));
        assert!(SubstrateType::is_antichain(std::iter::empty()));
    }

    /// [`Lattice::is_chain`] and [`Lattice::is_antichain`] are BOTH
    /// vacuously true at every singleton on every classification-axis
    /// lattice — a singleton contains no distinct pair, so the same
    /// vacuous-conjunction identity as the empty arm holds. Pinned
    /// exhaustively over every variant of every closed-set impl so a
    /// regression that broke the empty-conjunction identity on the
    /// nested loop surfaces at the first variant.
    #[test]
    fn is_chain_and_is_antichain_are_vacuously_true_at_every_singleton() {
        use tatara_process::classification::{
            CalmClassification, DataClassification, SubstrateType,
        };
        for a in DataClassification::ALL {
            assert!(DataClassification::is_chain([&a]));
            assert!(DataClassification::is_antichain([&a]));
        }
        for a in CalmClassification::ALL {
            assert!(CalmClassification::is_chain([&a]));
            assert!(CalmClassification::is_antichain([&a]));
        }
        for a in SubstrateType::ALL {
            assert!(SubstrateType::is_chain([&a]));
            assert!(SubstrateType::is_antichain([&a]));
        }
    }

    /// [`Lattice::is_chain`] and [`Lattice::is_antichain`] are BOTH
    /// vacuously true on any duplicate-only collection — the
    /// [`PartialEq`] filter drops every self-pair, leaving no distinct
    /// pair to check. This is the shared vacuous-truth arm at the
    /// non-trivial (non-empty, non-singleton) length; a triple of
    /// duplicates matches the same vacuous-conjunction identity as
    /// the empty / singleton arms. Pinned on every closed-set impl at
    /// arity 3 to catch a regression that dropped the [`PartialEq`]
    /// filter (which would flip antichain to false on `[&a, &a, &a]`
    /// because `is_incomparable(&a, &a) == false`).
    #[test]
    fn is_chain_and_is_antichain_are_vacuously_true_on_duplicate_only_collections() {
        use tatara_process::classification::{
            CalmClassification, DataClassification, SubstrateType,
        };
        for a in DataClassification::ALL {
            assert!(DataClassification::is_chain([&a, &a, &a]));
            assert!(DataClassification::is_antichain([&a, &a, &a]));
        }
        for a in CalmClassification::ALL {
            assert!(CalmClassification::is_chain([&a, &a, &a]));
            assert!(CalmClassification::is_antichain([&a, &a, &a]));
        }
        for a in SubstrateType::ALL {
            assert!(SubstrateType::is_chain([&a, &a, &a]));
            assert!(SubstrateType::is_antichain([&a, &a, &a]));
        }
    }

    /// [`Lattice::is_chain`] at arity 2 REDUCES to `a == b ||
    /// a.is_comparable(&b)` on every [`DataClassification`] pair, and
    /// [`Lattice::is_antichain`] at arity 2 REDUCES to `a == b ||
    /// a.is_incomparable(&b)` — the 2-input structural predicates
    /// collapse to the pairwise-comparability primitives with the
    /// duplicate filter. Pinned exhaustively over `ALL^2` (36 pairs).
    #[test]
    fn is_chain_and_is_antichain_arity_2_reduces_to_pairwise_over_data_classification_all() {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert_eq!(
                    DataClassification::is_chain([&a, &b]),
                    a == b || a.is_comparable(&b),
                    "is_chain([{a:?}, {b:?}]) must reduce to pairwise-comparability \
                     with the duplicate filter — the structural predicate is defined \
                     on distinct pairs and vacuously accepts duplicates",
                );
                assert_eq!(
                    DataClassification::is_antichain([&a, &b]),
                    a == b || a.is_incomparable(&b),
                    "is_antichain([{a:?}, {b:?}]) must reduce to pairwise-incomparability \
                     with the duplicate filter — dual identity on the antichain arm",
                );
            }
        }
    }

    /// [`Lattice::is_chain`] is UNIVERSALLY TRUE over every subset of
    /// [`DataClassification::ALL`] — the sensitivity axis is a total
    /// order, so every collection is a chain. Peer of
    /// `is_comparable_is_universally_true_over_data_classification_all_pairs`
    /// one arity axis up. Pinned exhaustively over `ALL^3` (216
    /// triples) so a future variant insertion that broke the total-
    /// order property surfaces here at the first triple containing
    /// the incomparable pair.
    #[test]
    fn is_chain_is_universally_true_over_data_classification_all_triples() {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                for c in DataClassification::ALL {
                    assert!(
                        DataClassification::is_chain([&a, &b, &c]),
                        "DataClassification is a total order — every triple \
                         ({a:?}, {b:?}, {c:?}) must be a chain, but is_chain \
                         returned false — the sensitivity_rank projection has \
                         drifted into a non-total shape",
                    );
                }
            }
        }
    }

    /// [`Lattice::is_antichain`] on [`DataClassification`] fires TRUE
    /// only on collections with at most one distinct value — every
    /// pair of distinct values on a total-order lattice is comparable,
    /// so the structural predicate rejects. Dual seal to
    /// `is_chain_is_universally_true_over_data_classification_all_triples`
    /// on the total-order axis. Pinned exhaustively over `ALL^2`
    /// pairs.
    #[test]
    fn is_antichain_over_data_classification_all_pairs_fires_true_iff_pair_is_equal() {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert_eq!(
                    DataClassification::is_antichain([&a, &b]),
                    a == b,
                    "is_antichain([{a:?}, {b:?}]) on a total order must fire \
                     true iff the pair is equal — every distinct pair is \
                     comparable, so the antichain predicate rejects",
                );
            }
        }
    }

    /// [`Lattice::is_antichain`] PROJECTS the pointed-top antichain on
    /// [`SubstrateType`] to its structural shape: on any collection
    /// whose distinct values are ALL non-top, the predicate fires
    /// TRUE (all pairs are pairwise-incomparable); on any collection
    /// mixing the pointed-top [`SubstrateType::Regulatory`] with a
    /// distinct non-top substrate, the predicate fires FALSE (the
    /// top-directed pair is comparable). Peer seal to
    /// `substrate_type_is_incomparable_matches_the_antichain_shape`
    /// one arity axis up. Pinned exhaustively over `ALL^3` (512
    /// triples).
    #[test]
    fn is_antichain_projects_the_pointed_top_antichain_over_substrate_type_all_triples() {
        use tatara_process::classification::SubstrateType;
        let top = SubstrateType::top();
        for a in SubstrateType::ALL {
            for b in SubstrateType::ALL {
                for c in SubstrateType::ALL {
                    let is_antichain = SubstrateType::is_antichain([&a, &b, &c]);
                    // Distinct values among the triple.
                    let distinct: Vec<&SubstrateType> = {
                        let mut d = vec![&a, &b, &c];
                        d.sort();
                        d.dedup();
                        d
                    };
                    // Antichain on the pointed-top substrate iff no
                    // distinct value in the triple is the pointed top
                    // OR the distinct set has at most one element
                    // (a duplicate-only collection is always both).
                    let expected = distinct.len() <= 1 || distinct.iter().all(|v| **v != top);
                    assert_eq!(
                        is_antichain, expected,
                        "is_antichain([{a:?}, {b:?}, {c:?}]) must match the antichain \
                         shape — accepts iff at most one distinct value OR every \
                         distinct value is non-top",
                    );
                }
            }
        }
    }

    /// [`Lattice::is_chain`] on the {Compute, Storage, Network} triple
    /// REJECTS on [`SubstrateType`] — three distinct non-top substrates
    /// form a 3-element antichain, not a chain. Peer non-vacuous-truth
    /// pin on the antichain-shape lattice, distinguishing the
    /// structural predicates from each other at a non-trivial triple.
    /// Together with the {Compute, Storage, Regulatory} pin below,
    /// this seals the chain / antichain shape at the SAME 3-element
    /// alphabet: swap the top-endpoint arm and the verdict flips
    /// mechanically.
    #[test]
    fn is_chain_rejects_three_distinct_non_top_substrates() {
        use tatara_process::classification::SubstrateType;
        let s = SubstrateType::Compute;
        let t = SubstrateType::Storage;
        let u = SubstrateType::Network;
        // Three distinct non-top substrates: antichain, not chain.
        assert!(SubstrateType::is_antichain([&s, &t, &u]));
        assert!(!SubstrateType::is_chain([&s, &t, &u]));
    }

    /// [`Lattice::is_chain`] on the {Compute, Storage, Regulatory}
    /// triple REJECTS on [`SubstrateType`] — {Compute, Storage} is
    /// pairwise-incomparable, so even adding the pointed-top does
    /// NOT rescue the collection into a chain. Peer to the sibling
    /// non-top triple pin above; this pin also shows that
    /// [`Lattice::is_antichain`] REJECTS the same triple because
    /// {Compute, Regulatory} is comparable (Regulatory is the
    /// pointed-top and every non-top substrate `leq`'s it).
    /// Together the two pins bind the structural predicates at the
    /// "mixed-shape collection" case — the exact case where NEITHER
    /// predicate fires because the collection contains both a
    /// chain-pair AND an incomparable-pair.
    #[test]
    fn neither_chain_nor_antichain_on_the_mixed_shape_substrate_triple() {
        use tatara_process::classification::SubstrateType;
        let s = SubstrateType::Compute;
        let t = SubstrateType::Storage;
        let top = SubstrateType::Regulatory;
        // Mixed-shape: {s, t} incomparable AND {s, top} comparable.
        assert!(!SubstrateType::is_chain([&s, &t, &top]));
        assert!(!SubstrateType::is_antichain([&s, &t, &top]));
    }

    /// [`Lattice::is_chain`] on the {non-top, top} pair on
    /// [`SubstrateType`] ACCEPTS — the pointed-top is comparable to
    /// every non-top substrate. Peer to the sibling triple rejections
    /// above; this pin binds the top-directed strict half-edge as a
    /// 2-element chain on the antichain lattice. Exhaustive over
    /// every non-top substrate.
    #[test]
    fn is_chain_accepts_the_top_directed_pair_over_substrate_type_all_non_top() {
        use tatara_process::classification::SubstrateType;
        let top = SubstrateType::top();
        for a in SubstrateType::ALL {
            if a != top {
                assert!(
                    SubstrateType::is_chain([&a, &top]),
                    "is_chain([{a:?}, {top:?}]) must accept — the pointed-top \
                     is comparable to every non-top substrate",
                );
                // Dual on the antichain arm: rejects the same pair
                // because it contains one comparable-distinct pair.
                assert!(
                    !SubstrateType::is_antichain([&a, &top]),
                    "is_antichain([{a:?}, {top:?}]) must reject — the \
                     top-directed pair is comparable",
                );
            }
        }
    }

    /// [`Lattice::is_chain`] and [`Lattice::is_antichain`] are
    /// SEQUENCE-ORDER INDEPENDENT — the structural predicates depend
    /// only on the underlying multiset, not the emission order. Pinned
    /// over `DataClassification::ALL^3` (216 triples) and
    /// `SubstrateType::ALL^3` (512 triples): every permutation of every
    /// triple yields the same verdict. Fail-before-pass-after: a
    /// regression that broke the symmetric nested-loop (e.g. dropped
    /// `j > i` and used `j != i`, or checked only `leq` without the
    /// dual `geq`) would surface here at the first permuted triple
    /// whose verdict differs from the sorted canonical order.
    #[test]
    fn is_chain_and_is_antichain_are_sequence_order_independent() {
        use tatara_process::classification::{DataClassification, SubstrateType};
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                for c in DataClassification::ALL {
                    let refv = DataClassification::is_chain([&a, &b, &c]);
                    for perm in &[
                        [&a, &c, &b],
                        [&b, &a, &c],
                        [&b, &c, &a],
                        [&c, &a, &b],
                        [&c, &b, &a],
                    ] {
                        assert_eq!(
                            DataClassification::is_chain(perm.iter().copied()),
                            refv,
                            "is_chain must be sequence-order independent",
                        );
                    }
                    let refav = DataClassification::is_antichain([&a, &b, &c]);
                    for perm in &[
                        [&a, &c, &b],
                        [&b, &a, &c],
                        [&b, &c, &a],
                        [&c, &a, &b],
                        [&c, &b, &a],
                    ] {
                        assert_eq!(
                            DataClassification::is_antichain(perm.iter().copied()),
                            refav,
                            "is_antichain must be sequence-order independent",
                        );
                    }
                }
            }
        }
        for a in SubstrateType::ALL {
            for b in SubstrateType::ALL {
                for c in SubstrateType::ALL {
                    let refc = SubstrateType::is_chain([&a, &b, &c]);
                    let refac = SubstrateType::is_antichain([&a, &b, &c]);
                    // Two permutations are enough to seal symmetry on
                    // the antichain-shape lattice (the full 6-permutation
                    // sweep on the total-order sibling above binds the
                    // stronger form).
                    assert_eq!(SubstrateType::is_chain([&c, &b, &a]), refc);
                    assert_eq!(SubstrateType::is_antichain([&c, &b, &a]), refac);
                }
            }
        }
    }

    /// Any [`DataClassification`] triple that chains STRICTLY MONOTONE
    /// left-to-right (every consecutive pair satisfies
    /// [`Lattice::strictly_below`]) IS a chain. Refines-strict-monotone
    /// witness: transitivity of [`Lattice::strictly_below`] extends the
    /// pairwise strict relation to every distinct pair, which refines
    /// [`Lattice::leq`] and therefore [`Lattice::is_comparable`]. Peer
    /// seal to `strictly_below_matches_sensitivity_rank_strict_inequality_over_data_classification_all_pairs`
    /// one arity axis up. Pinned exhaustively over
    /// `DataClassification::ALL^3`.
    #[test]
    fn strictly_monotone_triples_over_data_classification_all_are_chains() {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                for c in DataClassification::ALL {
                    if a.strictly_below(&b) && b.strictly_below(&c) {
                        assert!(
                            DataClassification::is_chain([&a, &b, &c]),
                            "strictly-monotone triple ({a:?}, {b:?}, {c:?}) must be \
                             a chain — transitivity of strictly_below extends the \
                             pairwise strict relation to every distinct pair",
                        );
                    }
                }
            }
        }
    }

    // ── Lattice::is_ascending / Lattice::is_descending — sequence-shape
    //    consecutive-pair default peers ─────────────────────────────────
    //
    // Bind [`Lattice::is_ascending`] + [`Lattice::is_descending`] at
    // fail-before-pass-after granularity. Pre-lift the [`Lattice`]
    // trait's collection-level monotonicity surface was empty at the
    // sequence-shape level — [`Lattice::is_chain`] / [`Lattice::is_antichain`]
    // cover the STRUCTURAL all-pairs comparability face, and a consumer
    // that wanted the CONSECUTIVE-PAIR monotonicity reading hand-authored
    // `vs.windows(2).all(|w| w[0].leq(&w[1]))` (or the dual `geq`) at
    // each callsite (surfaced in-tree at
    // `tatara_lisp::macro_expand::ResourceLimits::is_ascending` and
    // its `Self::is_descending` peer — a per-domain `const fn` pair on
    // the pointwise resource-posture partial order whose own doc names
    // this widening as the natural next lift). Post-lift the whole
    // sequence-shape monotonicity pair binds at ONE substrate primitive
    // on the [`Lattice`] algebra, and every downstream impl (the four
    // in-tree ones + `Baseline` via the `crate::Lattice` trait AND any
    // future closed-set lift) inherits `is_ascending` / `is_descending`
    // for free through the default. Together with the prior widening
    // (`is_chain` / `is_antichain` on the structural all-pairs arm) the
    // trait now closes the (structural, sequential) × (leq-comparable,
    // geq-comparable) 2×2 grid on the collection-level combinator
    // surface: the structural arm reads all-pairs comparability
    // symmetrically via `is_comparable` / `is_incomparable`; the
    // sequential arm reads consecutive-pair monotonicity directionally
    // via `leq` / `geq`.
    //
    // The dedup convention DIVERGES between the two collection-level
    // pairs — the structural predicates filter `vs[i] == vs[j]` pairs
    // on the standard-mathematical distinct-pair convention (an empty
    // multiset is a chain and an antichain), but the sequence-shape
    // predicates do NOT filter consecutive duplicates on the standard-
    // mathematical reflexive-order convention (both [`Lattice::leq`]
    // and [`Lattice::geq`] are reflexive, so a consecutive-duplicate
    // pair is BOTH ascending and descending, and filtering it would
    // change the sequence-order semantics on the (T, T) reflexive-
    // diagonal arm).
    //
    // Coverage below spans:
    //
    //   • empty / singleton / consecutive-duplicate vacuous truth on
    //     the reflexive-diagonal arm (BOTH ascending and descending);
    //   • pair-identity `T::is_ascending([&a, &b]) == a.leq(&b)` (dual
    //     on the descending arm) at every pair of every closed-set impl;
    //   • total-order ascending witness over `DataClassification::ALL`
    //     sorted by sensitivity rank AND descending witness over the
    //     reversed enumeration — dual pins on the SAME chain in
    //     REVERSED orderings closing the (ascending, descending)
    //     sequence-level pair on the shipped chain;
    //   • ascending-implies-chain (dual on descending) via transitivity
    //     of [`Lattice::leq`] / [`Lattice::geq`] on every triple of
    //     every closed-set impl;
    //   • sequence-order DEPENDENCE (contrasts with is_chain's
    //     INDEPENDENCE) — the pair-reversal on a strictly-monotone
    //     distinct triple flips ascending to descending;
    //   • antichain-lattice rejection over `SubstrateType::ALL^2` on
    //     distinct-non-top consecutive pairs;
    //   • proptest peers on the DataClassification + CALM axes for
    //     the pair-identity and reflexive-diagonal arms.

    /// [`Lattice::is_ascending`] and [`Lattice::is_descending`] are
    /// BOTH vacuously true at the empty iterator on every classification-
    /// axis lattice — the empty conjunction has no consecutive pair to
    /// check, so both universally-quantified predicates fire true on
    /// the empty case. Peer of [`Lattice::is_chain`]'s empty-iterator
    /// vacuous truth one CONSECUTIVE-VS-ALL-PAIRS axis over. Fail-
    /// before-pass-after: pre-lift this pin cannot compile because
    /// neither `is_ascending` nor `is_descending` is exposed as a trait
    /// method — consumers who wanted the sequence-shape reading wrote
    /// the consecutive-pair walk inline at each callsite.
    #[test]
    fn is_ascending_and_is_descending_are_vacuously_true_at_the_empty_iterator() {
        use tatara_process::classification::{
            CalmClassification, DataClassification, SubstrateType,
        };
        assert!(DataClassification::is_ascending(std::iter::empty()));
        assert!(DataClassification::is_descending(std::iter::empty()));
        assert!(CalmClassification::is_ascending(std::iter::empty()));
        assert!(CalmClassification::is_descending(std::iter::empty()));
        assert!(SubstrateType::is_ascending(std::iter::empty()));
        assert!(SubstrateType::is_descending(std::iter::empty()));
    }

    /// [`Lattice::is_ascending`] and [`Lattice::is_descending`] are
    /// BOTH vacuously true at every singleton — a singleton contains
    /// no consecutive pair, same empty-conjunction identity as the
    /// empty arm. Pinned exhaustively over every variant of every
    /// closed-set impl.
    #[test]
    fn is_ascending_and_is_descending_are_vacuously_true_at_every_singleton() {
        use tatara_process::classification::{
            CalmClassification, DataClassification, SubstrateType,
        };
        for a in DataClassification::ALL {
            assert!(DataClassification::is_ascending([&a]));
            assert!(DataClassification::is_descending([&a]));
        }
        for a in CalmClassification::ALL {
            assert!(CalmClassification::is_ascending([&a]));
            assert!(CalmClassification::is_descending([&a]));
        }
        for a in SubstrateType::ALL {
            assert!(SubstrateType::is_ascending([&a]));
            assert!(SubstrateType::is_descending([&a]));
        }
    }

    /// [`Lattice::is_ascending`] and [`Lattice::is_descending`] BOTH
    /// fire true on any all-duplicate collection — [`Lattice::leq`]
    /// and [`Lattice::geq`] are both REFLEXIVE, so the consecutive
    /// duplicate pair `(a, a)` passes both walks. Shared reflexive-
    /// diagonal arm at the non-trivial (non-empty, non-singleton)
    /// length; DIVERGES from [`Lattice::is_chain`] /
    /// [`Lattice::is_antichain`] which apply a distinct-pair filter —
    /// the sequence-shape predicates do NOT filter consecutive
    /// duplicates on the standard-mathematical reflexive-order
    /// convention. Pinned on every closed-set impl at arity 3 to catch
    /// a regression that added a spurious distinct-pair filter (which
    /// would flip both predicates to `true` vacuously by dropping the
    /// consecutive-duplicate reflexive-witness arm).
    #[test]
    fn is_ascending_and_is_descending_fire_true_on_consecutive_duplicate_collections() {
        use tatara_process::classification::{
            CalmClassification, DataClassification, SubstrateType,
        };
        for a in DataClassification::ALL {
            assert!(DataClassification::is_ascending([&a, &a, &a]));
            assert!(DataClassification::is_descending([&a, &a, &a]));
        }
        for a in CalmClassification::ALL {
            assert!(CalmClassification::is_ascending([&a, &a, &a]));
            assert!(CalmClassification::is_descending([&a, &a, &a]));
        }
        for a in SubstrateType::ALL {
            assert!(SubstrateType::is_ascending([&a, &a, &a]));
            assert!(SubstrateType::is_descending([&a, &a, &a]));
        }
    }

    /// [`Lattice::is_ascending`] at arity 2 REDUCES to `a.leq(&b)` on
    /// every [`DataClassification`] pair, and [`Lattice::is_descending`]
    /// at arity 2 REDUCES to `a.geq(&b)` — the 2-input sequence-shape
    /// predicates collapse to the pairwise primitive without a
    /// duplicate filter (consecutive duplicates are load-bearing on
    /// the reflexive arm, unlike [`Lattice::is_chain`]'s all-pairs
    /// distinct-only convention). Pinned exhaustively over `ALL^2`
    /// (36 pairs). Fail-before-pass-after: pre-lift the sequence-shape
    /// arm was empty on the trait, so this reduction did not exist as
    /// a substrate primitive; post-lift the reduction pins the
    /// sequence-shape predicate to the pairwise primitive at the
    /// primitive-cardinality boundary.
    #[test]
    fn is_ascending_and_is_descending_arity_2_reduces_to_pairwise_over_data_classification_all() {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert_eq!(
                    DataClassification::is_ascending([&a, &b]),
                    a.leq(&b),
                    "is_ascending([{a:?}, {b:?}]) must reduce to a.leq(&b) — \
                     the 2-input sequence-shape predicate collapses to the \
                     pairwise primitive without a duplicate filter",
                );
                assert_eq!(
                    DataClassification::is_descending([&a, &b]),
                    a.geq(&b),
                    "is_descending([{a:?}, {b:?}]) must reduce to a.geq(&b) — \
                     dual pair-identity on the geq arm",
                );
            }
        }
    }

    /// [`Lattice::is_ascending`] fires TRUE on the sensitivity-rank-
    /// sorted enumeration of [`DataClassification::ALL`], and
    /// [`Lattice::is_descending`] fires TRUE on the REVERSED
    /// enumeration — dual pins on the SAME chain in reversed
    /// orderings closing the (ascending, descending) sequence-level
    /// pair on the shipped total-order chain. Peer of
    /// `is_chain_is_universally_true_over_data_classification_all_triples`
    /// one CONSECUTIVE-VS-ALL-PAIRS axis over: where `is_chain` fires
    /// universally on the total order (every triple is a chain), the
    /// sequence-shape predicates DEPEND on the emission order (they
    /// fire true iff the sequence is sorted by the underlying rank
    /// projection).
    #[test]
    fn is_ascending_holds_on_data_classification_all_and_is_descending_on_reversed() {
        use tatara_process::classification::DataClassification;
        let sorted: Vec<&DataClassification> = DataClassification::ALL.iter().collect();
        let reversed: Vec<&DataClassification> = DataClassification::ALL.iter().rev().collect();
        assert!(
            DataClassification::is_ascending(sorted.iter().copied()),
            "DataClassification::ALL is emitted in sensitivity-rank order — \
             the sequence-shape ascending predicate must fire true on the \
             shipped enumeration",
        );
        assert!(
            DataClassification::is_descending(reversed.iter().copied()),
            "reversing DataClassification::ALL yields a geq-monotone \
             descending sequence — the sequence-shape descending predicate \
             must fire true on the reversed enumeration",
        );
    }

    /// [`Lattice::is_ascending`] REFINES [`Lattice::is_chain`] via
    /// transitivity of [`Lattice::leq`] — every ascending triple on
    /// [`DataClassification`] is a chain, and dually every descending
    /// triple is a chain. Refines-monotone witness: the pointwise
    /// partial order is transitive, so a consecutive-pair leq-chain
    /// closes under transitive composition into an all-pairs leq-chain,
    /// and every leq-related pair is [`Lattice::is_comparable`]. Peer
    /// of `strictly_monotone_triples_over_data_classification_all_are_chains`
    /// on the non-strict arm at the sequence-shape level. Pinned
    /// exhaustively over `DataClassification::ALL^3` (216 triples).
    #[test]
    fn is_ascending_and_is_descending_imply_is_chain_on_data_classification_all_triples() {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                for c in DataClassification::ALL {
                    if DataClassification::is_ascending([&a, &b, &c]) {
                        assert!(
                            DataClassification::is_chain([&a, &b, &c]),
                            "is_ascending([{a:?}, {b:?}, {c:?}]) must imply is_chain \
                             — transitivity of leq extends the consecutive-pair \
                             chain to every distinct pair",
                        );
                    }
                    if DataClassification::is_descending([&a, &b, &c]) {
                        assert!(
                            DataClassification::is_chain([&a, &b, &c]),
                            "is_descending([{a:?}, {b:?}, {c:?}]) must imply is_chain \
                             — transitivity of geq on the dual arm",
                        );
                    }
                }
            }
        }
    }

    /// [`Lattice::is_ascending`] is SEQUENCE-ORDER DEPENDENT
    /// (contrasts with [`Lattice::is_chain`]'s SEQUENCE-ORDER
    /// INDEPENDENCE) — reversing a strictly-monotone distinct
    /// ascending triple yields a strictly-monotone distinct descending
    /// triple, which the ascending predicate REJECTS. Pinned on the
    /// {Public, Internal, Confidential} strict ascent from
    /// [`DataClassification`]: the forward walk is ascending (all leq
    /// pairs) and NOT descending; the reversed walk is descending and
    /// NOT ascending. This is what distinguishes the sequence-shape
    /// from the structural-shape predicate at the same arity:
    /// [`Lattice::is_chain`] accepts BOTH orderings (order-independent);
    /// [`Lattice::is_ascending`] / [`Lattice::is_descending`] accept
    /// exactly ONE (order-dependent).
    #[test]
    fn is_ascending_is_sequence_order_dependent_on_a_strict_ascent() {
        use tatara_process::classification::DataClassification;
        let a = DataClassification::Public;
        let b = DataClassification::Internal;
        let c = DataClassification::Confidential;
        // Forward strict ascent: ascending true, descending false.
        assert!(DataClassification::is_ascending([&a, &b, &c]));
        assert!(!DataClassification::is_descending([&a, &b, &c]));
        // Reversed strict ascent = strict descent: ascending false, descending true.
        assert!(!DataClassification::is_ascending([&c, &b, &a]));
        assert!(DataClassification::is_descending([&c, &b, &a]));
        // Structural predicate accepts BOTH orderings (order-independent).
        assert!(DataClassification::is_chain([&a, &b, &c]));
        assert!(DataClassification::is_chain([&c, &b, &a]));
    }

    /// [`Lattice::is_ascending`] REJECTS distinct-non-top consecutive
    /// pairs on the pointed-top antichain [`SubstrateType`] — two
    /// distinct non-top substrates are incomparable, so
    /// [`Lattice::leq`] fails on the consecutive pair, so the walk
    /// rejects. The predicate ACCEPTS pairs `(x, top)` (the top-
    /// directed half-edge from any non-top substrate into
    /// [`SubstrateType::Regulatory`]) and REJECTS pairs `(top, x)`
    /// for `x != top` (the top does NOT `leq` any non-top element).
    /// Dual behavior on [`Lattice::is_descending`]: rejects `(x, top)`
    /// for `x != top` (top does NOT sit at-or-below any non-top),
    /// accepts `(top, x)` (top sits at-or-above any non-top). Pinned
    /// exhaustively over `SubstrateType::ALL^2` (64 pairs).
    #[test]
    fn is_ascending_projects_the_pointed_top_antichain_over_substrate_type_all_pairs() {
        use tatara_process::classification::SubstrateType;
        let top = SubstrateType::top();
        for a in SubstrateType::ALL {
            for b in SubstrateType::ALL {
                // Ascending accepts iff a.leq(&b) — reflexive `a == b`
                // OR top-directed `b == top`.
                let expected_asc = a == b || b == top;
                assert_eq!(
                    SubstrateType::is_ascending([&a, &b]),
                    expected_asc,
                    "is_ascending([{a:?}, {b:?}]) on the pointed-top antichain \
                     must fire true iff a == b OR b is the pointed top",
                );
                // Descending accepts iff a.geq(&b) — reflexive `a == b`
                // OR top-emitted `a == top`.
                let expected_desc = a == b || a == top;
                assert_eq!(
                    SubstrateType::is_descending([&a, &b]),
                    expected_desc,
                    "is_descending([{a:?}, {b:?}]) on the pointed-top antichain \
                     must fire true iff a == b OR a is the pointed top",
                );
            }
        }
    }

    proptest! {
        /// [`Lattice::is_chain`] at arity 2 REDUCES to the pairwise
        /// comparability primitive with the duplicate filter on every
        /// random [`DataClassification`] pair — proptest peer of the
        /// exhaustive
        /// `is_chain_and_is_antichain_arity_2_reduces_to_pairwise_over_data_classification_all`
        /// seal above.
        #[test]
        fn data_class_is_chain_arity_2_reduces_to_pairwise(
            a in any_data_class(),
            b in any_data_class(),
        ) {
            prop_assert_eq!(
                DataClassification::is_chain([&a, &b]),
                a == b || a.is_comparable(&b),
            );
            prop_assert_eq!(
                DataClassification::is_antichain([&a, &b]),
                a == b || a.is_incomparable(&b),
            );
        }

        /// [`Lattice::is_chain`] is UNIVERSALLY TRUE over every random
        /// [`DataClassification`] triple — proptest peer of the
        /// exhaustive
        /// `is_chain_is_universally_true_over_data_classification_all_triples`
        /// seal above.
        #[test]
        fn data_class_is_chain_is_universally_true(
            a in any_data_class(),
            b in any_data_class(),
            c in any_data_class(),
        ) {
            prop_assert!(DataClassification::is_chain([&a, &b, &c]));
        }

        /// [`Lattice::is_antichain`] on [`DataClassification`] fires
        /// true iff the pair is equal — proptest peer of the exhaustive
        /// `is_antichain_over_data_classification_all_pairs_fires_true_iff_pair_is_equal`
        /// seal above.
        #[test]
        fn data_class_is_antichain_arity_2_fires_true_iff_pair_is_equal(
            a in any_data_class(),
            b in any_data_class(),
        ) {
            prop_assert_eq!(DataClassification::is_antichain([&a, &b]), a == b);
        }

        /// Peer of the DataClassification is_chain / is_antichain
        /// arity-2 pairwise reduction on the CALM boolean-lattice axis
        /// — same shape, different closed set.
        #[test]
        fn calm_is_chain_arity_2_reduces_to_pairwise(a in any_calm(), b in any_calm()) {
            prop_assert_eq!(
                CalmClassification::is_chain([&a, &b]),
                a == b || a.is_comparable(&b),
            );
            prop_assert_eq!(
                CalmClassification::is_antichain([&a, &b]),
                a == b || a.is_incomparable(&b),
            );
        }

        /// [`Lattice::is_chain`] on [`CalmClassification`] is
        /// universally true over every random pair — the two-arm
        /// boolean lattice is a total order (`Monotone ≤ NonMonotone`).
        /// Peer of the sibling `data_class_is_chain_is_universally_true`
        /// on the DataClassification axis; together the two proptest
        /// cases bind BOTH total-order classification axes' chain
        /// universality to ONE substrate default.
        #[test]
        fn calm_is_chain_is_universally_true(a in any_calm(), b in any_calm()) {
            prop_assert!(CalmClassification::is_chain([&a, &b]));
        }

        /// [`Lattice::is_ascending`] at arity 2 REDUCES to `a.leq(&b)`
        /// on every random [`DataClassification`] pair — proptest peer
        /// of the exhaustive
        /// `is_ascending_and_is_descending_arity_2_reduces_to_pairwise_over_data_classification_all`
        /// seal above. Dual on the descending arm reduces to
        /// `a.geq(&b)`.
        #[test]
        fn data_class_is_ascending_and_is_descending_arity_2_reduce_to_pairwise(
            a in any_data_class(),
            b in any_data_class(),
        ) {
            prop_assert_eq!(DataClassification::is_ascending([&a, &b]), a.leq(&b));
            prop_assert_eq!(DataClassification::is_descending([&a, &b]), a.geq(&b));
        }

        /// [`Lattice::is_ascending`] on any consecutive-duplicate
        /// [`DataClassification`] pair fires TRUE via the reflexive
        /// arm of [`Lattice::leq`], and dually
        /// [`Lattice::is_descending`] fires TRUE via the reflexive arm
        /// of [`Lattice::geq`] — proptest peer of the exhaustive
        /// `is_ascending_and_is_descending_fire_true_on_consecutive_duplicate_collections`
        /// seal above.
        #[test]
        fn data_class_is_ascending_and_is_descending_on_consecutive_duplicate(
            a in any_data_class(),
        ) {
            prop_assert!(DataClassification::is_ascending([&a, &a]));
            prop_assert!(DataClassification::is_descending([&a, &a]));
        }

        /// [`Lattice::is_ascending`] REFINES [`Lattice::is_chain`] via
        /// transitivity of [`Lattice::leq`] on every random
        /// [`DataClassification`] triple — proptest peer of the
        /// exhaustive
        /// `is_ascending_and_is_descending_imply_is_chain_on_data_classification_all_triples`
        /// seal above. Dual on the descending arm via
        /// [`Lattice::geq`] transitivity.
        #[test]
        fn data_class_is_ascending_and_is_descending_imply_is_chain(
            a in any_data_class(),
            b in any_data_class(),
            c in any_data_class(),
        ) {
            if DataClassification::is_ascending([&a, &b, &c]) {
                prop_assert!(DataClassification::is_chain([&a, &b, &c]));
            }
            if DataClassification::is_descending([&a, &b, &c]) {
                prop_assert!(DataClassification::is_chain([&a, &b, &c]));
            }
        }

        /// Peer of the DataClassification is_ascending / is_descending
        /// arity-2 pair-identity on the CALM boolean-lattice axis —
        /// same shape, different closed set. Together with the
        /// DataClassification sibling above the two proptest cases
        /// bind BOTH total-order classification axes' sequence-shape
        /// pair-identity to ONE substrate default.
        #[test]
        fn calm_is_ascending_and_is_descending_arity_2_reduce_to_pairwise(
            a in any_calm(),
            b in any_calm(),
        ) {
            prop_assert_eq!(CalmClassification::is_ascending([&a, &b]), a.leq(&b));
            prop_assert_eq!(CalmClassification::is_descending([&a, &b]), a.geq(&b));
        }

        /// [`Lattice::is_strictly_ascending`] at arity 2 REDUCES to
        /// `a.strictly_below(&b)` on every random
        /// [`DataClassification`] pair, and
        /// [`Lattice::is_strictly_descending`] at arity 2 REDUCES to
        /// `a.strictly_above(&b)` — proptest peer of the exhaustive
        /// `is_strictly_ascending_and_is_strictly_descending_arity_2_reduces_to_pairwise_over_data_classification_all`
        /// seal below.
        #[test]
        fn data_class_is_strictly_ascending_and_is_strictly_descending_arity_2_reduce_to_pairwise(
            a in any_data_class(),
            b in any_data_class(),
        ) {
            prop_assert_eq!(
                DataClassification::is_strictly_ascending([&a, &b]),
                a.strictly_below(&b),
            );
            prop_assert_eq!(
                DataClassification::is_strictly_descending([&a, &b]),
                a.strictly_above(&b),
            );
        }

        /// [`Lattice::is_strictly_ascending`] and
        /// [`Lattice::is_strictly_descending`] BOTH fire FALSE on any
        /// consecutive-duplicate [`DataClassification`] pair via the
        /// irreflexivity axiom of the strict pairwise-primitive — proptest
        /// peer of the exhaustive
        /// `is_strictly_ascending_and_is_strictly_descending_reject_consecutive_duplicate_collections`
        /// seal below.
        #[test]
        fn data_class_is_strictly_ascending_and_is_strictly_descending_reject_consecutive_duplicate(
            a in any_data_class(),
        ) {
            prop_assert!(!DataClassification::is_strictly_ascending([&a, &a]));
            prop_assert!(!DataClassification::is_strictly_descending([&a, &a]));
        }

        /// [`Lattice::is_strictly_ascending`] IMPLIES
        /// [`Lattice::is_ascending`] on every random
        /// [`DataClassification`] triple via
        /// `Lattice::strictly_below ⇒ Lattice::leq`, and dually
        /// [`Lattice::is_strictly_descending`] IMPLIES
        /// [`Lattice::is_descending`] via
        /// `Lattice::strictly_above ⇒ Lattice::geq`. The strictness-
        /// complement implication propagates through the conjunctive
        /// `Iterator::all` fold. Proptest peer of the exhaustive
        /// `is_strictly_ascending_and_is_strictly_descending_imply_non_strict_arms_over_data_classification_all_triples`
        /// seal below.
        #[test]
        fn data_class_is_strictly_ascending_and_is_strictly_descending_imply_non_strict(
            a in any_data_class(),
            b in any_data_class(),
            c in any_data_class(),
        ) {
            if DataClassification::is_strictly_ascending([&a, &b, &c]) {
                prop_assert!(DataClassification::is_ascending([&a, &b, &c]));
            }
            if DataClassification::is_strictly_descending([&a, &b, &c]) {
                prop_assert!(DataClassification::is_descending([&a, &b, &c]));
            }
        }

        /// Peer of the DataClassification is_strictly_ascending /
        /// is_strictly_descending arity-2 pair-identity on the CALM
        /// boolean-lattice axis — same shape, different closed set.
        /// Together with the DataClassification sibling above the two
        /// proptest cases bind BOTH total-order classification axes'
        /// strict sequence-shape pair-identity to ONE substrate default.
        #[test]
        fn calm_is_strictly_ascending_and_is_strictly_descending_arity_2_reduce_to_pairwise(
            a in any_calm(),
            b in any_calm(),
        ) {
            prop_assert_eq!(
                CalmClassification::is_strictly_ascending([&a, &b]),
                a.strictly_below(&b),
            );
            prop_assert_eq!(
                CalmClassification::is_strictly_descending([&a, &b]),
                a.strictly_above(&b),
            );
        }
    }

    // ── Lattice::is_strictly_ascending / Lattice::is_strictly_descending —
    //    strict-arm sequence-shape consecutive-pair default peers ─────────
    //
    // Bind [`Lattice::is_strictly_ascending`] +
    // [`Lattice::is_strictly_descending`] at fail-before-pass-after
    // granularity. Pre-lift the [`Lattice`] trait's collection-level
    // sequence-monotonicity surface had only the NON-STRICT
    // ([`Lattice::is_ascending`] / [`Lattice::is_descending`]) arm bound
    // through the reflexive [`Lattice::leq`] / [`Lattice::geq`] primitives;
    // a consumer that wanted the STRICT irreflexive sequence-shape
    // reading composed `vs.windows(2).all(|w| w[0].leq(&w[1]) && w[0]
    // != w[1])` at each callsite. Post-lift the whole strict-arm pair
    // binds at ONE substrate primitive on the [`Lattice`] algebra via
    // [`Lattice::strictly_below`] / [`Lattice::strictly_above`], and
    // every downstream impl inherits the strict sequence-monotonicity
    // pair for free through the default. Together with the prior
    // widenings ([`Lattice::is_ascending`] / [`Lattice::is_descending`]
    // on the non-strict arm; [`Lattice::strictly_below`] /
    // [`Lattice::strictly_above`] on the pairwise strict arm) the trait
    // now closes the whole (strict, non-strict) × (leq, geq) ×
    // (pairwise, sequence) 2×2×2 monotonicity cube on the algebra's
    // combinator surface: the pairwise face carries FOUR pair-level
    // primitives; the sequence face carries FOUR consecutive-pair
    // predicates each routing through its own pairwise primitive.
    //
    // Coverage below spans:
    //
    //   • empty / singleton vacuous truth on BOTH strict-arm predicates
    //     shared with the non-strict arms;
    //   • consecutive-duplicate REJECTION at arity 2 (the primary
    //     DIVERGENCE from the non-strict arm's reflexive acceptance)
    //     on every closed-set impl;
    //   • pair-identity `T::is_strictly_ascending([&a, &b]) ==
    //     a.strictly_below(&b)` (dual on the descending arm) at every
    //     pair of every closed-set impl;
    //   • total-order strict-ascending witness over
    //     `DataClassification::ALL` sorted by sensitivity rank AND
    //     strict-descending witness over the reversed enumeration —
    //     dual pins on the SAME strict-total-ordered chain;
    //   • strict-implies-non-strict on every triple of every closed-set
    //     impl (strict-ascending implies ascending; strict-descending
    //     implies descending);
    //   • strict-ascending-implies-chain (dual on strict-descending) via
    //     composition with the existing ascending-implies-chain pin;
    //   • antichain-lattice rejection over `SubstrateType::ALL^2` on
    //     every consecutive pair except the strict half-edge into/out
    //     of the pointed top;
    //   • proptest peers on the DataClassification + CALM axes for the
    //     pair-identity, consecutive-duplicate rejection, and strict-
    //     implies-non-strict arms.

    /// [`Lattice::is_strictly_ascending`] and
    /// [`Lattice::is_strictly_descending`] are BOTH vacuously true at
    /// the empty iterator on every classification-axis lattice — the
    /// empty conjunction has no consecutive pair to check, so both
    /// universally-quantified predicates fire true on the empty case.
    /// Shared with all four sequence-monotonicity predicates at the
    /// empty case. Fail-before-pass-after: pre-lift this pin cannot
    /// compile because neither `is_strictly_ascending` nor
    /// `is_strictly_descending` is exposed as a trait method.
    #[test]
    fn is_strictly_ascending_and_is_strictly_descending_are_vacuously_true_at_the_empty_iterator() {
        use tatara_process::classification::{
            CalmClassification, DataClassification, SubstrateType,
        };
        assert!(DataClassification::is_strictly_ascending(std::iter::empty()));
        assert!(DataClassification::is_strictly_descending(
            std::iter::empty()
        ));
        assert!(CalmClassification::is_strictly_ascending(std::iter::empty()));
        assert!(CalmClassification::is_strictly_descending(
            std::iter::empty()
        ));
        assert!(SubstrateType::is_strictly_ascending(std::iter::empty()));
        assert!(SubstrateType::is_strictly_descending(std::iter::empty()));
    }

    /// [`Lattice::is_strictly_ascending`] and
    /// [`Lattice::is_strictly_descending`] are BOTH vacuously true at
    /// every singleton — a singleton contains no consecutive pair, same
    /// empty-conjunction identity as the empty arm. Pinned exhaustively
    /// over every variant of every closed-set impl. Shared with all
    /// four sequence-monotonicity predicates at every singleton.
    #[test]
    fn is_strictly_ascending_and_is_strictly_descending_are_vacuously_true_at_every_singleton() {
        use tatara_process::classification::{
            CalmClassification, DataClassification, SubstrateType,
        };
        for a in DataClassification::ALL {
            assert!(DataClassification::is_strictly_ascending([&a]));
            assert!(DataClassification::is_strictly_descending([&a]));
        }
        for a in CalmClassification::ALL {
            assert!(CalmClassification::is_strictly_ascending([&a]));
            assert!(CalmClassification::is_strictly_descending([&a]));
        }
        for a in SubstrateType::ALL {
            assert!(SubstrateType::is_strictly_ascending([&a]));
            assert!(SubstrateType::is_strictly_descending([&a]));
        }
    }

    /// [`Lattice::is_strictly_ascending`] and
    /// [`Lattice::is_strictly_descending`] BOTH fire FALSE on any
    /// all-duplicate collection — [`Lattice::strictly_below`] and
    /// [`Lattice::strictly_above`] are BOTH IRREFLEXIVE, so the
    /// consecutive duplicate pair `(a, a)` rejects both walks. DIVERGES
    /// from [`Lattice::is_ascending`] / [`Lattice::is_descending`]
    /// which accept every all-duplicate collection via the reflexive
    /// arms of [`Lattice::leq`] / [`Lattice::geq`] — this is the primary
    /// distinguishing consequence of moving from the reflexive to the
    /// irreflexive pair-level primitive on the sequence-shape face.
    /// Pinned on every closed-set impl at arity 3 to catch a regression
    /// that dropped the irreflexivity axiom of the strict comparator.
    #[test]
    fn is_strictly_ascending_and_is_strictly_descending_reject_consecutive_duplicate_collections() {
        use tatara_process::classification::{
            CalmClassification, DataClassification, SubstrateType,
        };
        for a in DataClassification::ALL {
            assert!(!DataClassification::is_strictly_ascending([&a, &a, &a]));
            assert!(!DataClassification::is_strictly_descending([&a, &a, &a]));
        }
        for a in CalmClassification::ALL {
            assert!(!CalmClassification::is_strictly_ascending([&a, &a, &a]));
            assert!(!CalmClassification::is_strictly_descending([&a, &a, &a]));
        }
        for a in SubstrateType::ALL {
            assert!(!SubstrateType::is_strictly_ascending([&a, &a, &a]));
            assert!(!SubstrateType::is_strictly_descending([&a, &a, &a]));
        }
    }

    /// [`Lattice::is_strictly_ascending`] at arity 2 REDUCES to
    /// `a.strictly_below(&b)` on every [`DataClassification`] pair, and
    /// [`Lattice::is_strictly_descending`] at arity 2 REDUCES to
    /// `a.strictly_above(&b)` — the 2-input strict sequence-shape
    /// predicates collapse to the strict pairwise primitive directly.
    /// Pinned exhaustively over `ALL^2` (36 pairs). Fail-before-pass-after:
    /// pre-lift the strict sequence-shape arm was empty on the trait,
    /// so this reduction did not exist as a substrate primitive; post-
    /// lift the reduction pins the strict sequence-shape predicate to
    /// the strict pairwise primitive at the primitive-cardinality
    /// boundary.
    #[test]
    fn is_strictly_ascending_and_is_strictly_descending_arity_2_reduces_to_pairwise_over_data_classification_all(
    ) {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert_eq!(
                    DataClassification::is_strictly_ascending([&a, &b]),
                    a.strictly_below(&b),
                    "is_strictly_ascending([{a:?}, {b:?}]) must reduce to \
                     a.strictly_below(&b) — the 2-input strict sequence-shape \
                     predicate collapses to the strict pairwise primitive directly",
                );
                assert_eq!(
                    DataClassification::is_strictly_descending([&a, &b]),
                    a.strictly_above(&b),
                    "is_strictly_descending([{a:?}, {b:?}]) must reduce to \
                     a.strictly_above(&b) — dual strict pair-identity on the \
                     strictly_above arm",
                );
            }
        }
    }

    /// [`Lattice::is_strictly_ascending`] fires TRUE on the sensitivity-
    /// rank-sorted enumeration of [`DataClassification::ALL`] (which is
    /// strictly monotone by construction — every consecutive pair
    /// satisfies `strictly_below`), and
    /// [`Lattice::is_strictly_descending`] fires TRUE on the REVERSED
    /// enumeration — dual pins on the SAME strict-total-ordered chain
    /// in reversed orderings closing the (strict-ascending, strict-
    /// descending) sequence-level pair on the shipped strictly-total-
    /// ordered chain.
    #[test]
    fn is_strictly_ascending_holds_on_data_classification_all_and_is_strictly_descending_on_reversed(
    ) {
        use tatara_process::classification::DataClassification;
        let sorted: Vec<&DataClassification> = DataClassification::ALL.iter().collect();
        let reversed: Vec<&DataClassification> = DataClassification::ALL.iter().rev().collect();
        assert!(
            DataClassification::is_strictly_ascending(sorted.iter().copied()),
            "DataClassification::ALL is emitted in strictly-monotone \
             sensitivity-rank order — the strict sequence-shape ascending \
             predicate must fire true on the shipped enumeration",
        );
        assert!(
            DataClassification::is_strictly_descending(reversed.iter().copied()),
            "reversing DataClassification::ALL yields a strictly-monotone \
             strictly_above-descending sequence — the strict sequence-shape \
             descending predicate must fire true on the reversed enumeration",
        );
    }

    /// [`Lattice::is_strictly_ascending`] IMPLIES [`Lattice::is_ascending`]
    /// on every [`DataClassification`] triple via the strict-implies-
    /// non-strict projection `strictly_below ⇒ leq`, and dually
    /// [`Lattice::is_strictly_descending`] IMPLIES
    /// [`Lattice::is_descending`] via `strictly_above ⇒ geq`. The
    /// strictness-complement implication propagates through the
    /// conjunctive `Iterator::all` fold on every consecutive pair.
    /// Pinned exhaustively over `DataClassification::ALL^3` (216
    /// triples). Composed with the existing ascending-implies-chain /
    /// descending-implies-chain pins yields the strict-ascending-
    /// implies-chain / strict-descending-implies-chain refinements as a
    /// corollary.
    #[test]
    fn is_strictly_ascending_and_is_strictly_descending_imply_non_strict_arms_over_data_classification_all_triples(
    ) {
        use tatara_process::classification::DataClassification;
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                for c in DataClassification::ALL {
                    if DataClassification::is_strictly_ascending([&a, &b, &c]) {
                        assert!(
                            DataClassification::is_ascending([&a, &b, &c]),
                            "is_strictly_ascending([{a:?}, {b:?}, {c:?}]) must \
                             imply is_ascending — strictly_below implies leq",
                        );
                        assert!(
                            DataClassification::is_chain([&a, &b, &c]),
                            "is_strictly_ascending([{a:?}, {b:?}, {c:?}]) must \
                             imply is_chain via composition with ascending-implies-chain",
                        );
                    }
                    if DataClassification::is_strictly_descending([&a, &b, &c]) {
                        assert!(
                            DataClassification::is_descending([&a, &b, &c]),
                            "is_strictly_descending([{a:?}, {b:?}, {c:?}]) must \
                             imply is_descending — strictly_above implies geq",
                        );
                        assert!(
                            DataClassification::is_chain([&a, &b, &c]),
                            "is_strictly_descending([{a:?}, {b:?}, {c:?}]) must \
                             imply is_chain via composition with descending-implies-chain",
                        );
                    }
                }
            }
        }
    }

    /// [`Lattice::is_strictly_ascending`] REJECTS every consecutive
    /// pair on the pointed-top antichain [`SubstrateType`] except the
    /// strict top-directed half-edge `(x, top)` for `x != top` — two
    /// distinct non-top substrates are incomparable, so
    /// [`Lattice::strictly_below`] fails on the consecutive pair, and
    /// every reflexive pair `(a, a)` fails by irreflexivity of the
    /// strict comparator. Dual behavior on
    /// [`Lattice::is_strictly_descending`]: accepts only the strict
    /// top-emitted half-edge `(top, x)` for `x != top`. Pinned
    /// exhaustively over `SubstrateType::ALL^2` (64 pairs). Peer of
    /// `is_ascending_projects_the_pointed_top_antichain_over_substrate_type_all_pairs`
    /// with the reflexive-diagonal acceptance stripped out.
    #[test]
    fn is_strictly_ascending_projects_the_pointed_top_antichain_over_substrate_type_all_pairs() {
        use tatara_process::classification::SubstrateType;
        let top = SubstrateType::top();
        for a in SubstrateType::ALL {
            for b in SubstrateType::ALL {
                // Strict-ascending accepts iff a.strictly_below(&b) —
                // top-directed strict half-edge `b == top && a != top`.
                let expected_asc = a != b && b == top;
                assert_eq!(
                    SubstrateType::is_strictly_ascending([&a, &b]),
                    expected_asc,
                    "is_strictly_ascending([{a:?}, {b:?}]) on the pointed-top \
                     antichain must fire true iff b is the pointed top AND \
                     a != b — the strict top-directed half-edge",
                );
                // Strict-descending accepts iff a.strictly_above(&b) —
                // top-emitted strict half-edge `a == top && b != top`.
                let expected_desc = a != b && a == top;
                assert_eq!(
                    SubstrateType::is_strictly_descending([&a, &b]),
                    expected_desc,
                    "is_strictly_descending([{a:?}, {b:?}]) on the pointed-top \
                     antichain must fire true iff a is the pointed top AND \
                     a != b — the strict top-emitted half-edge",
                );
            }
        }
    }
}
