//! `tagged_union::resolve` — the typescape's "exactly-one-Option" pattern,
//! lifted to one source of truth.
//!
//! Several CRD-facing types in this crate ([`crate::intent::Intent`],
//! [`crate::lifetime::Lifetime`], [`crate::export::ArtifactSource`],
//! [`crate::export::VectorChannel`], [`crate::encapsulates::EncapsulationKind`])
//! carry `N` `Option<T>` fields where exactly one is expected to be
//! populated on the wire. Each previously hand-rolled the same
//! `count() + if-let-chain + unreachable!()` body — four parallel tables
//! (the struct fields, an `is_some()` count array, an `if-let-else`
//! resolution chain, and any sibling projection like `IntentVariant::kind`)
//! kept coherent only by code review. The `unreachable!()` arm at the
//! bottom of every chain was a sentinel that fires at runtime if the
//! parallel tables ever drift.
//!
//! This module collapses the resolver to ONE typed sweep over an
//! `IntoIterator<Item = Option<V>>` of candidate variant projections.
//! Adding a new tagged-union variant is now ONE additional line at the
//! callsite — no `unreachable!()` arm to update, no parallel `is_some()`
//! count array to extend.

/// Outcome of [`resolve`] when the candidate list isn't exactly-one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveError {
    /// No candidate was populated.
    None,
    /// More than one candidate was populated.
    Many,
}

/// Resolve at most one populated variant from a candidate list.
///
/// Each item in `candidates` is the projected borrowed-variant view for
/// the corresponding `Option<T>` field — `None` when the field is unset,
/// `Some(V::Variant(...))` when set.
///
/// Returns the single populated variant, [`ResolveError::None`] when
/// none are populated, or [`ResolveError::Many`] when more than one are.
///
/// The body is one short-circuiting sweep — `Many` is returned as soon
/// as the second populated entry is seen, without scanning the rest.
pub fn resolve<V>(candidates: impl IntoIterator<Item = Option<V>>) -> Result<V, ResolveError> {
    let mut found: Option<V> = None;
    for candidate in candidates {
        if candidate.is_some() {
            if found.is_some() {
                return Err(ResolveError::Many);
            }
            found = candidate;
        }
    }
    found.ok_or(ResolveError::None)
}

/// Sibling error carriers on tagged-union `.variant()` sites all
/// project the two [`ResolveError`] arms onto the SAME closed-set
/// diagnostic shape — `Empty(&'static str)` for "no variant set"
/// (carrying the closed-set kind list so the operator diagnostic
/// names every candidate) and a payload-free `Ambiguous` for
/// "multiple variants set". This trait names that shared shape as
/// ONE typed contract; [`resolve_or_err`] then composes [`resolve`]
/// with the trait so each per-carrier `.map_err(|e| match e { ... })`
/// site collapses to a one-line typed dispatch.
///
/// Impls live at each error carrier's own module so the (diagnostic
/// message, closed-set list) pair stays owned by the carrier that
/// publishes it — the trait is the projection, not the message.
pub trait TaggedUnionError: Sized {
    /// Construct the "no variant set" arm with the closed-set kind
    /// list slash-joined into the diagnostic payload.
    fn empty(kinds: &'static str) -> Self;
    /// Construct the "multiple variants set" arm.
    fn ambiguous() -> Self;
}

/// Resolve at most one populated variant, mapping the two
/// [`ResolveError`] arms onto the caller's typed carrier via
/// [`TaggedUnionError`]. The compound-lift primitive: sweep +
/// short-circuit + typed-error dispatch as ONE call.
///
/// Substrate primitive for the four sibling `Xxx::variant()` sites
/// on `ProcessSpec` (`Intent::variant`,
/// `EncapsulationKind::variant`, `ArtifactSource::variant`,
/// `VectorChannel::variant`) that previously restated the SAME
/// `.map_err(|e| match e { None => Empty(LIST), Many => Ambiguous })`
/// two-arm dispatch at each call site — every one of them a
/// byte-identical restatement of the (empty→list, many→ambiguous)
/// projection whose payload identity is strictly the carrier's own
/// diagnostic. A fifth sibling error carrier picks up the projection
/// through ONE `impl TaggedUnionError` block + ONE `resolve_or_err`
/// call site.
///
/// The [`Lifetime::variant`](crate::lifetime::Lifetime::variant)
/// site is DELIBERATELY not routed through this primitive — its
/// `ResolveError::None` arm resolves to a `Permanent` default
/// variant, not to an `Empty` typed error, so the projection shape
/// diverges at the None arm.
pub fn resolve_or_err<V, E: TaggedUnionError>(
    candidates: impl IntoIterator<Item = Option<V>>,
    kinds: &'static str,
) -> Result<V, E> {
    resolve(candidates).map_err(|e| match e {
        ResolveError::None => E::empty(kinds),
        ResolveError::Many => E::ambiguous(),
    })
}

/// Declare a sibling error carrier for a tagged-union `.variant()`
/// site — the enum + [`TaggedUnionError`] impl in ONE authoring
/// surface.
///
/// Every one of the four production `.variant()` sites on
/// `ProcessSpec` ([`crate::intent::Intent`],
/// [`crate::encapsulates::EncapsulationKind`],
/// [`crate::export::ArtifactSource`],
/// [`crate::export::VectorChannel`]) pre-lift restated the same
/// four-piece authoring shape by hand:
///
/// 1. `#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq,
///    Eq)]` on the carrier — byte-identical across all four.
/// 2. A two-variant enum body (`Empty(&'static str)`, `Ambiguous`)
///    — structurally identical.
/// 3. Two `#[error(...)]` messages whose only per-carrier knob is a
///    noun-prefix (`"intent"`, `"encapsulation kind"`, ...) — every
///    other byte of the (`"has no variant set (one of {0}
///    required)"`, `"has multiple variants set; exactly one
///    required"`) tails was verbatim.
/// 4. A six-line `impl TaggedUnionError` whose two constructor
///    bodies re-projected `Self::Empty(kinds)` / `Self::Ambiguous`
///    onto each carrier's own typed variants.
///
/// The macro collapses (1) + (2) + (4) onto ONE call and takes the
/// two per-carrier operator-facing diagnostic literals as named
/// arguments so (3) stays visible at the callsite without re-authoring
/// the shared derive set or trait impl. A fifth sibling carrier
/// lands as ONE `declare_tagged_union_error!` invocation — no
/// re-authored `#[derive(...)]`, no re-authored two-variant enum
/// body, no re-authored `impl TaggedUnionError` block.
///
/// Emitted derives include `Copy` — the `Empty` arm carries only
/// a `&'static str` and the `Ambiguous` arm is payload-free, so
/// the carrier is always `Copy` regardless of caller.
///
/// # Example
///
/// ```ignore
/// declare_tagged_union_error! {
///     pub IntentError,
///     empty = "intent has no variant set (one of {0} required)",
///     ambiguous = "intent has multiple variants set; exactly one required",
/// }
/// ```
///
/// Expands to the enum + [`TaggedUnionError`] impl for
/// `IntentError`; the `Empty` arm carries the caller's closed-set
/// kind-list literal.
#[macro_export]
macro_rules! declare_tagged_union_error {
    (
        $(#[$attr:meta])*
        $vis:vis $name:ident,
        empty = $empty:literal,
        ambiguous = $ambiguous:literal $(,)?
    ) => {
        $(#[$attr])*
        #[derive(
            ::std::clone::Clone,
            ::std::marker::Copy,
            ::std::fmt::Debug,
            ::thiserror::Error,
            ::std::cmp::PartialEq,
            ::std::cmp::Eq,
        )]
        $vis enum $name {
            #[error($empty)]
            Empty(&'static str),
            #[error($ambiguous)]
            Ambiguous,
        }

        impl $crate::tagged_union::TaggedUnionError for $name {
            fn empty(kinds: &'static str) -> Self {
                Self::Empty(kinds)
            }
            fn ambiguous() -> Self {
                Self::Ambiguous
            }
        }
    };
}

/// Declare the three-block impl stanza a tagged-union parent type
/// publishes to the substrate — inherent `.variant()` forwarder +
/// [`VariantSelector<Parent>`] impl on the sibling `Kind` +
/// [`TaggedUnion`] impl on the parent — in ONE authoring surface.
///
/// Every one of the four production `.variant()` sites on
/// `ProcessSpec` ([`crate::intent::Intent`],
/// [`crate::encapsulates::EncapsulationKind`],
/// [`crate::export::ArtifactSource`],
/// [`crate::export::VectorChannel`]) pre-lift restated the same three
/// impl blocks by hand:
///
/// 1. `impl $parent { pub fn variant(&self) -> Result<$variant<'_>, $err> { ... } }`
///    — a one-line delegation to the [`TaggedUnion::variant`] default
///    body, plus 5 lines of rustdoc cross-referencing the other three
///    sibling `.variant()` sites verbatim.
/// 2. `impl VariantSelector<$parent> for $kind { type Variant<'a> = $variant<'a>; fn select(...) { <$kind>::select(self, parent) } }`
///    — 6 lines whose only per-site knobs are (`$parent`, `$kind`,
///    `$variant`); the trait method body a straight delegation to the
///    inherent `<$kind>::select`.
/// 3. `impl TaggedUnion for $parent { type Kind = $kind; type Error = $err; const KIND_LIST = $kind_list; }`
///    — 3 associated-item assignments whose only per-site knobs are
///    the (`$kind`, `$err`, `$kind_list`) tuple.
///
/// The macro takes the (`$parent`, `$kind`, `$variant`, `$err`,
/// `$kind_list`) five-tuple as named arguments and emits all three
/// blocks. A fifth sibling tagged-union parent picks up all three
/// impls through ONE macro call — no re-authored inherent
/// `.variant()` forwarder, no re-authored `impl VariantSelector`
/// block, no re-authored `impl TaggedUnion` block.
///
/// The emitted inherent `.variant()`'s rustdoc is canonical (names
/// the substrate primitive, not the exact set of sibling sites) so
/// a fifth sibling doesn't drift the cross-ref count against reality
/// merely by existing.
///
/// # Example
///
/// ```ignore
/// declare_tagged_union_impls! {
///     parent = Intent,
///     kind = IntentKind,
///     variant = IntentVariant,
///     error = IntentError,
///     kind_list = INTENT_KIND_LIST,
/// }
/// ```
///
/// Expands to the inherent `Intent::variant`, the
/// `VariantSelector<Intent>` impl on `IntentKind`, and the
/// `TaggedUnion` impl on `Intent`.
#[macro_export]
macro_rules! declare_tagged_union_impls {
    (
        parent = $parent:ty,
        kind = $kind:ty,
        variant = $variant:ident,
        error = $err:ty,
        kind_list = $kind_list:expr $(,)?
    ) => {
        impl $parent {
            /// Resolve to exactly one variant. Errors on zero or many.
            ///
            /// One-line inherent forwarder that delegates the sweep
            /// body to the substrate primitive
            /// [`crate::tagged_union::TaggedUnion::variant`] — every
            /// production `.variant()` site on `ProcessSpec` dispatches
            /// through this ONE default body so the resolve-sweep
            /// pattern lives at ONE substrate site. The inherent surface
            /// stays load-bearing so consumer callsites don't need
            /// `use TaggedUnion`.
            pub fn variant(&self) -> ::std::result::Result<$variant<'_>, $err> {
                <Self as $crate::tagged_union::TaggedUnion>::variant(self)
            }

            /// Presence probe — does this tagged union carry a
            /// populated slot addressed by the given closed-set
            /// discriminator?
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::has`] — every
            /// closed-set-driven presence check on `ProcessSpec`
            /// dispatches through this ONE default body so the
            /// per-slot `spec.<field>.is_some()` pattern lives at
            /// ONE substrate site. The inherent surface stays
            /// load-bearing so consumer callsites don't need
            /// `use TaggedUnion`.
            pub fn has(&self, kind: $kind) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::has(self, kind)
            }

            /// Closed-set-complement peer of [`Self::has`] — `true` iff
            /// the given `kind` is MISSING (its slot on this tagged
            /// union is empty).
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::lacks`], whose
            /// default body is `!self.has(kind)`. Every consumer whose
            /// semantic reading is "the missing set contains this
            /// kind" — a "still missing: <kind>" diagnostic, a
            /// `lacks-<kind>` require-tag classifier arm, a
            /// dependency-satisfaction check — reads
            /// `parent.lacks(kind)` through the inherent surface
            /// rather than negating `parent.has(kind)` at the call
            /// site. The definitional complement law
            /// `parent.lacks(kind) == !parent.has(kind)` and the
            /// kind-scoped implication
            /// `parent.lacks_only(kind) → parent.lacks(kind)` are
            /// pinned as first-class typed invariants by the trait's
            /// own default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_lacks_matches_has_complement`].
            pub fn lacks(&self, kind: $kind) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::lacks(self, kind)
            }

            /// Widened peer of [`Self::has`] — returns the borrowed
            /// variant view addressed by `kind`, or `None` when the
            /// matching slot is empty.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::find`], whose
            /// default body is `kind.select(self)`. Every
            /// closed-set-driven `kind.select(&parent)` callsite that
            /// pre-lift required `use VariantSelector` at the caller
            /// now reads `parent.find(kind)` through the inherent
            /// surface, byte-for-byte symmetrical with
            /// `parent.has(kind)`. The composition law
            /// `parent.has(kind) == parent.find(kind).is_some()` is
            /// pinned as a first-class typed invariant by the trait's
            /// own `has` default body
            /// (`self.find(kind).is_some()`), swept substrate-wide by
            /// [`crate::tagged_union::assert_find_agrees_with_has`].
            pub fn find(&self, kind: $kind) -> ::std::option::Option<$variant<'_>> {
                <Self as $crate::tagged_union::TaggedUnion>::find(self, kind)
            }

            /// Closed-set-inversion peer of [`Self::has`] / [`Self::find`]
            /// — returns the canonical-ordered `Vec` of populated
            /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL)
            /// discriminators.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::populated_kinds`],
            /// whose default body is
            /// `<Kind as ClosedSet>::ALL.iter().copied().filter(|k|
            /// self.has(*k)).collect()`. Every consumer that needs
            /// to enumerate which slots on a tagged-union parent are
            /// populated (an operator-facing "Ambiguous named
            /// [Nix, Container]" diagnostic composed on the malformed
            /// arm; a closed-set audit dispatcher; a
            /// `populated-kind-count-<n>` require-tag classifier
            /// prefix) reads `parent.populated_kinds()` through the
            /// inherent surface, byte-for-byte symmetrical with
            /// `parent.has(kind)` / `parent.find(kind)`. The
            /// composition law
            /// `parent.populated_kinds().contains(&k) == parent.has(k)`
            /// is pinned as a first-class typed invariant by the
            /// trait's own default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_populated_kinds_matches_has`].
            pub fn populated_kinds(&self) -> ::std::vec::Vec<$kind> {
                <Self as $crate::tagged_union::TaggedUnion>::populated_kinds(self)
            }

            /// Scalar cardinality peer of [`Self::populated_kinds`] —
            /// the number of populated slots on this tagged union.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::populated_kind_count`],
            /// whose default body is
            /// `<Kind as ClosedSet>::ALL.iter().copied().filter(|k|
            /// self.has(*k)).count()`. Every consumer that needs the
            /// cardinality of the populated-slot set as a scalar
            /// (a `populated-kind-count-<n>` require-tag classifier
            /// prefix; a fast-path branch on the Ambiguous-arm side
            /// that discriminates "well-formed" from "malformed with
            /// N slots"; a coherence check that verifies "every
            /// well-formed parent has exactly one populated slot")
            /// reads `parent.populated_kind_count()` through the
            /// inherent surface, byte-for-byte symmetrical with
            /// `parent.has(kind)` / `parent.find(kind)` /
            /// `parent.populated_kinds()`. The composition law
            /// `parent.populated_kind_count() == parent.populated_kinds().len()`
            /// is pinned as a first-class typed invariant by the
            /// trait's own default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_populated_kind_count_matches_populated_kinds`].
            pub fn populated_kind_count(&self) -> usize {
                <Self as $crate::tagged_union::TaggedUnion>::populated_kind_count(self)
            }

            /// Closed-set-COMPLEMENT peer of [`Self::populated_kinds`]
            /// — returns the canonical-ordered `Vec` of EMPTY
            /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL)
            /// discriminators.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::missing_kinds`],
            /// whose default body is
            /// `<Kind as ClosedSet>::ALL.iter().copied().filter(|k|
            /// !self.has(*k)).collect()`. Every consumer that needs to
            /// enumerate which slots on a tagged-union parent are
            /// ABSENT (an operator-facing "still missing [Nix, Container]"
            /// diagnostic on the partially-populated arm; a coherence
            /// check verifying "every process boundary carries every
            /// intent slot"; a `missing-<kind>` require-tag classifier
            /// arm) reads `parent.missing_kinds()` through the
            /// inherent surface, byte-for-byte symmetrical with
            /// `parent.populated_kinds()`. The partition law
            /// `parent.populated_kinds() ∪ parent.missing_kinds() ==
            /// ClosedSet::ALL` (with the two sets disjoint) is pinned
            /// as a first-class typed invariant by the trait's own
            /// default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_missing_kinds_matches_has`].
            pub fn missing_kinds(&self) -> ::std::vec::Vec<$kind> {
                <Self as $crate::tagged_union::TaggedUnion>::missing_kinds(self)
            }

            /// Scalar cardinality peer of [`Self::missing_kinds`] —
            /// the number of EMPTY slots on this tagged union.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::missing_kind_count`],
            /// whose default body is
            /// `<Kind as ClosedSet>::ALL.iter().copied().filter(|k|
            /// !self.has(*k)).count()`. Every consumer that needs the
            /// cardinality of the missing-slot set as a scalar (a
            /// `missing-kind-count-<n>` require-tag classifier prefix;
            /// a fast-path branch that discriminates "well-formed"
            /// from "N missing slots"; a coherence check that verifies
            /// "every well-formed parent has exactly ALL.len() - 1
            /// missing slots") reads `parent.missing_kind_count()`
            /// through the inherent surface, byte-for-byte symmetrical
            /// with `parent.populated_kind_count()`. The scalar
            /// partition law `parent.populated_kind_count() +
            /// parent.missing_kind_count() == <Kind as ClosedSet>::ALL.len()`
            /// is pinned by the trait's own default body and swept
            /// substrate-wide by
            /// [`crate::tagged_union::assert_missing_kind_count_matches_missing_kinds`].
            pub fn missing_kind_count(&self) -> usize {
                <Self as $crate::tagged_union::TaggedUnion>::missing_kind_count(self)
            }

            /// Short-circuiting `Option<$kind>` peer of
            /// [`Self::populated_kinds`] — the FIRST populated kind on
            /// this tagged union in canonical
            /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL)
            /// order, or `None` when no slot is populated.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::first_populated_kind`],
            /// whose default body is
            /// `<Kind as ClosedSet>::ALL.iter().copied().find(|k|
            /// self.has(*k))`. Every consumer that needs the earliest
            /// populated slot on a tagged-union parent as an
            /// `Option<Kind>` (an operator-facing "Ambiguous, starting
            /// at Nix" diagnostic on the malformed arm; a
            /// `first-populated-<kind>` require-tag classifier arm; a
            /// fast-path branch that discriminates "empty" from "any
            /// populated") reads `parent.first_populated_kind()` through
            /// the inherent surface, byte-for-byte symmetrical with
            /// `parent.populated_kinds()` / `parent.has(kind)`. The
            /// composition law `parent.first_populated_kind() ==
            /// parent.populated_kinds().first().copied()` is pinned as
            /// a first-class typed invariant by the trait's own default
            /// body and swept substrate-wide by
            /// [`crate::tagged_union::assert_first_populated_kind_matches_populated_kinds`].
            pub fn first_populated_kind(&self) -> ::std::option::Option<$kind> {
                <Self as $crate::tagged_union::TaggedUnion>::first_populated_kind(self)
            }

            /// Short-circuiting `Option<$kind>` peer of
            /// [`Self::missing_kinds`] — the FIRST missing kind on this
            /// tagged union in canonical
            /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL)
            /// order, or `None` when EVERY slot is populated.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::first_missing_kind`],
            /// whose default body is
            /// `<Kind as ClosedSet>::ALL.iter().copied().find(|k|
            /// !self.has(*k))`. Byte-for-byte symmetrical with
            /// `parent.first_populated_kind()` under a negated
            /// predicate; the two primitives PARTITION
            /// `ClosedSet::ALL`'s earliest-element projection on the
            /// (populated, missing) split. The composition law
            /// `parent.first_missing_kind() ==
            /// parent.missing_kinds().first().copied()` is pinned as a
            /// first-class typed invariant by the trait's own default
            /// body and swept substrate-wide by
            /// [`crate::tagged_union::assert_first_missing_kind_matches_missing_kinds`].
            pub fn first_missing_kind(&self) -> ::std::option::Option<$kind> {
                <Self as $crate::tagged_union::TaggedUnion>::first_missing_kind(self)
            }

            /// Short-circuiting `Option<$kind>` peer of
            /// [`Self::populated_kinds`] — the LAST populated kind on
            /// this tagged union in canonical
            /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL)
            /// order, or `None` when no slot is populated.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::last_populated_kind`],
            /// whose default body is
            /// `<Kind as ClosedSet>::ALL.iter().rev().copied().find(|k|
            /// self.has(*k))` — a REVERSED closed-set walk composed
            /// against `self.has` per variant that SHORT-CIRCUITS at
            /// the latest match. Byte-for-byte time-reversed peer of
            /// [`Self::first_populated_kind`]. Empty parent returns
            /// `None`; well-formed parent returns `Some(k)` (the sole
            /// populated slot); malformed (Ambiguous) parent returns
            /// `Some(k)` where `k` is the LATEST populated slot in
            /// canonical `ALL` order — the operator-diagnostic "and
            /// last at Z" peer of the "Ambiguous, starting at Nix"
            /// upgrade the first-projection enables. The composition
            /// law `parent.last_populated_kind() ==
            /// parent.populated_kinds().last().copied()` is pinned as
            /// a first-class typed invariant by the trait's own default
            /// body and swept substrate-wide by
            /// [`crate::tagged_union::assert_last_populated_kind_matches_populated_kinds`].
            pub fn last_populated_kind(&self) -> ::std::option::Option<$kind> {
                <Self as $crate::tagged_union::TaggedUnion>::last_populated_kind(self)
            }

            /// Short-circuiting `Option<$kind>` peer of
            /// [`Self::missing_kinds`] — the LAST missing kind on this
            /// tagged union in canonical
            /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL)
            /// order, or `None` when EVERY slot is populated.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::last_missing_kind`],
            /// whose default body is
            /// `<Kind as ClosedSet>::ALL.iter().rev().copied().find(|k|
            /// !self.has(*k))`. Byte-for-byte time-reversed peer of
            /// [`Self::first_missing_kind`] under an identical negated
            /// predicate. The two primitives PARTITION
            /// `ClosedSet::ALL`'s endpoint projection on the (populated,
            /// missing) × (earliest, latest) product together with the
            /// `first_*` peers — every endpoint-addressable coherence
            /// check reads ONE of the four at ONE call site without
            /// allocating a `Vec<$kind>`. The composition law
            /// `parent.last_missing_kind() ==
            /// parent.missing_kinds().last().copied()` is pinned as a
            /// first-class typed invariant by the trait's own default
            /// body and swept substrate-wide by
            /// [`crate::tagged_union::assert_last_missing_kind_matches_missing_kinds`].
            pub fn last_missing_kind(&self) -> ::std::option::Option<$kind> {
                <Self as $crate::tagged_union::TaggedUnion>::last_missing_kind(self)
            }

            /// Exactly-one-populated `Option<$kind>` peer of
            /// [`Self::populated_kinds`] — `Some(k)` iff `k` is the
            /// SOLE populated kind on this tagged union, else `None`.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::unique_populated_kind`],
            /// whose default body is a two-step-short-circuit walk
            /// over `<Kind as ClosedSet>::ALL` returning `Some(k)`
            /// only when EXACTLY ONE `has(k)` is `true`. Every
            /// consumer that needs the resolved kind identity on the
            /// well-formed arm (without paying for the borrowed
            /// variant view [`Self::variant`] returns, and without
            /// materializing the [`Self::Error`] carrier on the
            /// empty / malformed arms) reads
            /// `parent.unique_populated_kind()` through the inherent
            /// surface — `Some(k)` names well-formed exactly-one,
            /// `None` collapses BOTH the empty AND the malformed
            /// (ambiguous) arms.
            ///
            /// The composition laws
            /// `parent.unique_populated_kind().is_some() ==
            /// (parent.populated_kind_count() == 1)` and (on the
            /// `Some` arm) `parent.unique_populated_kind() ==
            /// parent.first_populated_kind() ==
            /// parent.last_populated_kind()` are pinned as first-
            /// class typed invariants by the trait's own default body
            /// and swept substrate-wide by
            /// [`crate::tagged_union::assert_unique_populated_kind_matches_populated_kinds`].
            pub fn unique_populated_kind(&self) -> ::std::option::Option<$kind> {
                <Self as $crate::tagged_union::TaggedUnion>::unique_populated_kind(self)
            }

            /// Exactly-one-missing `Option<$kind>` peer of
            /// [`Self::missing_kinds`] — `Some(k)` iff `k` is the
            /// SOLE missing kind on this tagged union, else `None`.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::unique_missing_kind`],
            /// whose default body is a two-step-short-circuit walk
            /// over `<Kind as ClosedSet>::ALL` under a NEGATED `has`
            /// predicate returning `Some(k)` only when EXACTLY ONE
            /// `!has(k)` is `true`. Byte-for-byte symmetrical with
            /// `parent.unique_populated_kind()` under complement; on
            /// tagged unions with `<Kind as ClosedSet>::ALL.len() >
            /// 2` the primitive returns `Some` only on the near-
            /// saturation arm (`ALL.len() - 1` populated).
            ///
            /// The composition laws
            /// `parent.unique_missing_kind().is_some() ==
            /// (parent.missing_kind_count() == 1)` and (on the
            /// `Some` arm) `parent.unique_missing_kind() ==
            /// parent.first_missing_kind() ==
            /// parent.last_missing_kind()` are pinned as first-class
            /// typed invariants by the trait's own default body and
            /// swept substrate-wide by
            /// [`crate::tagged_union::assert_unique_missing_kind_matches_missing_kinds`].
            pub fn unique_missing_kind(&self) -> ::std::option::Option<$kind> {
                <Self as $crate::tagged_union::TaggedUnion>::unique_missing_kind(self)
            }

            /// Boolean cardinality-endpoint peer of [`Self::populated_kinds`]
            /// — `true` iff NO slot on this tagged union is populated.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::is_empty`], whose
            /// default body is `!<Kind as ClosedSet>::ALL.iter().any(|k|
            /// self.has(k))` — a short-circuiting closed-set walk that
            /// returns `true` iff every point-probe returns `false`,
            /// WITHOUT materializing the `Vec` `populated_kinds` would
            /// build. Every consumer that needs the zero-arm Boolean
            /// projection of the populated cardinality (a fast-path
            /// guard on "any content at all"; an operator-facing
            /// "carrier missing content" diagnostic on the `Empty` arm;
            /// an `is-empty` require-tag classifier arm) reads
            /// `parent.is_empty()` through the inherent surface, byte-
            /// for-byte symmetrical with `parent.is_saturated()` under
            /// the (populated, missing) complement axis. The
            /// composition law
            /// `parent.is_empty() == (parent.populated_kind_count() == 0)`
            /// is pinned as a first-class typed invariant by the
            /// trait's own default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_is_empty_matches_populated_kind_count`].
            pub fn is_empty(&self) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::is_empty(self)
            }

            /// Boolean cardinality-endpoint peer of [`Self::missing_kinds`]
            /// — `true` iff EVERY slot on this tagged union is populated
            /// (i.e. the missing set is empty).
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::is_saturated`], whose
            /// default body is `<Kind as ClosedSet>::ALL.iter().all(|k|
            /// self.has(k))` — a short-circuiting closed-set walk that
            /// returns `true` iff every point-probe returns `true`,
            /// WITHOUT materializing the `Vec` `missing_kinds` would
            /// build. Every consumer that needs the zero-arm Boolean
            /// projection of the missing cardinality (a fast-path guard
            /// discriminating "over-populated" from "well-formed or
            /// partial"; an operator-facing "over-populated carrier"
            /// diagnostic; an `is-saturated` require-tag classifier
            /// arm) reads `parent.is_saturated()` through the inherent
            /// surface, byte-for-byte symmetrical with
            /// `parent.is_empty()` under the (populated, missing)
            /// complement axis. The composition law
            /// `parent.is_saturated() == (parent.missing_kind_count() == 0)`
            /// is pinned as a first-class typed invariant by the
            /// trait's own default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_is_saturated_matches_missing_kind_count`].
            pub fn is_saturated(&self) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::is_saturated(self)
            }

            /// Boolean cardinality "at-least-one" peer of
            /// [`Self::populated_kinds`] — `true` iff AT LEAST ONE slot
            /// on this tagged union is populated (the populated set is
            /// NON-empty).
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::has_any_populated_kind`],
            /// whose default body is `<Kind as ClosedSet>::ALL.iter().any(|k|
            /// self.has(k))` — a short-circuiting closed-set walk that
            /// returns `true` at the FIRST populated slot, WITHOUT
            /// materializing the `Vec` `populated_kinds` would build.
            /// Every consumer that needs the ≥ 1 halfspace on the
            /// populated cardinality (a boundary-progress "any content
            /// at all" diagnostic; an `is-non-empty` require-tag
            /// classifier arm; a fast-path branch discriminating "some
            /// populated" from "all missing") reads
            /// `parent.has_any_populated_kind()` through the inherent
            /// surface — byte-for-byte definitional complement of
            /// `parent.is_empty()`, no readerly inversion at the
            /// callsite, and byte-for-byte symmetrical with
            /// `parent.has_any_missing_kind()` under the (populated,
            /// missing) complement axis. The composition law
            /// `parent.has_any_populated_kind() == !parent.is_empty()`
            /// is pinned as a first-class typed invariant by the
            /// trait's own default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_has_any_populated_kind_matches_populated_kind_count`].
            pub fn has_any_populated_kind(&self) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::has_any_populated_kind(self)
            }

            /// Boolean cardinality "at-least-one" peer of
            /// [`Self::missing_kinds`] — `true` iff AT LEAST ONE slot on
            /// this tagged union is missing (the missing set is
            /// NON-empty).
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::has_any_missing_kind`],
            /// whose default body is `<Kind as ClosedSet>::ALL.iter().any(|k|
            /// !self.has(k))` — a short-circuiting closed-set walk under
            /// a negated `has` predicate that returns `true` at the
            /// FIRST missing slot, WITHOUT materializing the `Vec`
            /// `missing_kinds` would build. Every consumer that needs
            /// the ≥ 1 halfspace on the missing cardinality (an
            /// operator-facing "not fully populated" diagnostic; a
            /// `has-any-missing-kind` require-tag classifier arm; a
            /// fast-path branch discriminating "any slot still absent"
            /// from "over-populated / saturated") reads
            /// `parent.has_any_missing_kind()` through the inherent
            /// surface — byte-for-byte definitional complement of
            /// `parent.is_saturated()`, no readerly inversion at the
            /// callsite, and byte-for-byte symmetrical with
            /// `parent.has_any_populated_kind()` under the (populated,
            /// missing) complement axis. The composition law
            /// `parent.has_any_missing_kind() == !parent.is_saturated()`
            /// is pinned as a first-class typed invariant by the
            /// trait's own default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_has_any_missing_kind_matches_missing_kind_count`].
            pub fn has_any_missing_kind(&self) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::has_any_missing_kind(self)
            }

            /// Boolean cardinality-mid-endpoint peer of
            /// [`Self::unique_populated_kind`] — `true` iff EXACTLY ONE
            /// slot on this tagged union is populated.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::has_unique_populated_kind`],
            /// whose default body is `self.unique_populated_kind().is_some()`
            /// — the Boolean projection of the two-step-short-circuit
            /// closed-set walk `unique_populated_kind` already performs,
            /// without paying for a `Vec<$kind>` allocation on any arm.
            /// Every consumer that needs the exactly-one-populated arm
            /// as a `bool` (a fast-path branch on the well-formed arm
            /// that skips the borrowed-view / error-carrier
            /// materialization [`Self::variant`] would pay for; an
            /// operator-facing "well-formed" diagnostic on the resolver's
            /// Ok arm; a `has-unique-populated-kind` require-tag
            /// classifier arm; a coherence check verifying "every
            /// production parent from a `single_slot_X` factory is
            /// well-formed") reads `parent.has_unique_populated_kind()`
            /// through the inherent surface, byte-for-byte symmetrical
            /// with `parent.is_empty()` / `parent.is_saturated()` under
            /// the (zero-, one-arm) × (populated, missing) cardinality
            /// grid. The composition law
            /// `parent.has_unique_populated_kind() == (parent.populated_kind_count() == 1)`
            /// is pinned as a first-class typed invariant by the trait's
            /// own default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_has_unique_populated_kind_matches_populated_kind_count`].
            pub fn has_unique_populated_kind(&self) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::has_unique_populated_kind(self)
            }

            /// Boolean cardinality-mid-endpoint peer of
            /// [`Self::unique_missing_kind`] — `true` iff EXACTLY ONE
            /// slot on this tagged union is missing.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::has_unique_missing_kind`],
            /// whose default body is `self.unique_missing_kind().is_some()`
            /// — the Boolean projection of the two-step-short-circuit
            /// closed-set walk `unique_missing_kind` already performs.
            /// Every consumer that needs the exactly-one-missing arm as
            /// a `bool` (a fast-path branch on the near-saturation arm;
            /// an operator-facing "one slot away from saturated"
            /// diagnostic; a `has-unique-missing-kind` require-tag
            /// classifier arm) reads `parent.has_unique_missing_kind()`
            /// through the inherent surface, byte-for-byte symmetrical
            /// with `parent.has_unique_populated_kind()` under the
            /// (populated, missing) complement axis. The composition law
            /// `parent.has_unique_missing_kind() == (parent.missing_kind_count() == 1)`
            /// is pinned as a first-class typed invariant by the trait's
            /// own default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_has_unique_missing_kind_matches_missing_kind_count`].
            pub fn has_unique_missing_kind(&self) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::has_unique_missing_kind(self)
            }

            /// Boolean cardinality many-arm peer of
            /// [`Self::has_unique_populated_kind`] — `true` iff TWO
            /// OR MORE slots on this tagged union are populated (i.e.
            /// the "ambiguous" arm of the resolver contract).
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::has_multiple_populated_kinds`],
            /// whose default body is a two-step-short-circuit closed-
            /// set walk under [`Self::has`] that returns `true` iff
            /// the filtered iterator yields at least two hits, WITHOUT
            /// materializing the `Vec` `populated_kinds` would build.
            /// The short-circuit fires on the SECOND populated slot
            /// — strictly cheaper than the widened primitive on every
            /// arm past the second populated slot.
            ///
            /// # Sibling to the Boolean cardinality trichotomy
            ///
            /// Third arm of the {0, 1, ≥2} cardinality trichotomy on
            /// the populated axis. Together with [`Self::is_empty`]
            /// (zero-arm) and [`Self::has_unique_populated_kind`]
            /// (one-arm), these three Boolean primitives partition
            /// every tagged-union state coherently — EXACTLY ONE of
            /// the three returns `true` on any given parent. Maps
            /// directly onto the three arms of the resolver contract
            /// [`Self::variant`] returns:
            /// `is_empty()` ↔ `Err(Error::empty)`,
            /// `has_unique_populated_kind()` ↔ `Ok(Variant)`,
            /// `has_multiple_populated_kinds()` ↔ `Err(Error::ambiguous)`.
            ///
            /// The composition law
            /// `parent.has_multiple_populated_kinds() == (parent.populated_kind_count() >= 2)`
            /// is pinned as a first-class typed invariant by the
            /// trait's own default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_has_multiple_populated_kinds_matches_populated_kind_count`].
            pub fn has_multiple_populated_kinds(&self) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::has_multiple_populated_kinds(self)
            }

            /// Boolean cardinality many-arm peer of
            /// [`Self::has_unique_missing_kind`] — `true` iff TWO OR
            /// MORE slots on this tagged union are missing.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::has_multiple_missing_kinds`],
            /// whose default body is a two-step-short-circuit closed-
            /// set walk under a NEGATED [`Self::has`] predicate that
            /// returns `true` iff the filtered iterator yields at
            /// least two hits, WITHOUT materializing the `Vec`
            /// `missing_kinds` would build. Byte-for-byte symmetrical
            /// with `parent.has_multiple_populated_kinds()` under the
            /// (populated, missing) complement axis.
            ///
            /// Third arm of the {0, 1, ≥2} cardinality trichotomy on
            /// the missing axis. Together with [`Self::is_saturated`]
            /// (zero-arm) and [`Self::has_unique_missing_kind`]
            /// (one-arm), these three Boolean primitives partition
            /// every tagged-union state coherently on the complement
            /// axis. The composition law
            /// `parent.has_multiple_missing_kinds() == (parent.missing_kind_count() >= 2)`
            /// is pinned as a first-class typed invariant by the
            /// trait's own default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_has_multiple_missing_kinds_matches_missing_kind_count`].
            pub fn has_multiple_missing_kinds(&self) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::has_multiple_missing_kinds(self)
            }

            /// Boolean parent-state middle-arm projection — `true` iff
            /// this tagged union has AT LEAST ONE populated slot AND AT
            /// LEAST ONE missing slot, i.e. it is neither
            /// [`Self::is_empty`] nor [`Self::is_saturated`].
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::is_partially_populated`],
            /// whose default body is a FUSED short-circuit closed-set
            /// walk that returns `true` at the EARLIEST slot where both
            /// a populated AND a missing kind have been observed —
            /// byte-for-byte cheaper than the widened composition
            /// `!self.is_empty() && !self.is_saturated()` (two closed-
            /// set walks) on every partially-populated arm.
            ///
            /// # Sibling to the parent-state trichotomy
            ///
            /// Middle arm of the natural `{Empty | Partial | Saturated}`
            /// parent-state trichotomy — orthogonal to the {0, 1, ≥2}
            /// cardinality trichotomies on the populated / missing
            /// axes. Together with [`Self::is_empty`] (all-missing arm)
            /// and [`Self::is_saturated`] (all-populated arm), these
            /// three Boolean primitives partition every tagged-union
            /// state coherently on the parent-state axis — EXACTLY ONE
            /// of the three returns `true` on any given parent. The
            /// trichotomy partition law
            /// `usize::from(is_empty()) + usize::from(is_partially_populated())
            /// + usize::from(is_saturated()) == 1` is pinned as a first-
            /// class typed invariant by the trait's own default body
            /// and swept substrate-wide by
            /// [`crate::tagged_union::assert_is_partially_populated_matches_cardinality`].
            pub fn is_partially_populated(&self) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::is_partially_populated(self)
            }

            /// Kind-scoped strict refinement of [`Self::has`] — `true`
            /// iff the given `kind` is populated AND no OTHER slot on
            /// this tagged union is populated. The "exactly this one
            /// variant" predicate.
            ///
            /// One-line inherent forwarder that delegates to the
            /// substrate primitive
            /// [`crate::tagged_union::TaggedUnion::has_only`], whose
            /// default body is a FUSED short-circuit closed-set walk
            /// that returns `false` at the EARLIEST populated slot
            /// whose kind is NOT `kind`, and returns `true` iff the
            /// sweep completes with `kind` seen as the sole populated
            /// slot. Byte-for-byte cheaper than either widened
            /// composition `self.unique_populated_kind() ==
            /// Some(kind)` (which walks until the SECOND populated
            /// slot) or `self.has(kind) &&
            /// self.has_unique_populated_kind()` (which walks the
            /// closed set twice) on every arm where the parent
            /// carries a populated slot that isn't `kind`.
            ///
            /// # Sibling to [`Self::has`]
            ///
            /// Kind-scoped strict-refinement peer: `has(kind)` is the
            /// SUBSET predicate; `has_only(kind)` is the EQUAL
            /// predicate. The implication
            /// `has_only(kind) → has(kind)` binds the pair on the
            /// strict-refinement axis. The composition law
            /// `parent.has_only(kind) ==
            /// (parent.unique_populated_kind() == Some(kind))` is
            /// pinned as a first-class typed invariant by the trait's
            /// own default body and swept substrate-wide by
            /// [`crate::tagged_union::assert_has_only_matches_unique_populated_kind`].
            pub fn has_only(&self, kind: $kind) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::has_only(self, kind)
            }

            /// Closed-set-complement peer of [`Self::has_only`] —
            /// `true` iff the given `kind` is MISSING AND no OTHER slot
            /// on this tagged union is missing.
            ///
            /// One-line inherent forwarder that delegates the fused
            /// short-circuit walk to the substrate primitive
            /// [`crate::tagged_union::TaggedUnion::lacks_only`], whose
            /// default body walks
            /// `<Self::Kind as ClosedSet>::ALL` under a negated
            /// [`crate::tagged_union::TaggedUnion::has`] and returns
            /// `false` at the EARLIEST missing slot whose kind is not
            /// `kind`. Byte-for-byte cheaper than either widened
            /// composition
            /// `self.unique_missing_kind() == Some(kind)` (which walks
            /// until the SECOND missing slot before comparing) or
            /// `!self.has(kind) && self.has_unique_missing_kind()`
            /// (two closed-set walks) on every arm where the parent
            /// carries a missing slot that isn't `kind`.
            ///
            /// # Sibling to [`Self::has_only`]
            ///
            /// Closed-set-complement peer: `has_only(kind)` names
            /// parents whose SOLE populated slot is `kind`;
            /// `lacks_only(kind)` names parents whose SOLE missing slot
            /// is `kind`. The composition law
            /// `parent.lacks_only(kind) ==
            /// (parent.unique_missing_kind() == Some(kind))` and the
            /// cardinality-refinement law
            /// `parent.lacks_only(kind) == (!parent.has(kind) &&
            /// parent.has_unique_missing_kind())` are pinned as first-
            /// class typed invariants by the trait's own default body
            /// and swept substrate-wide by
            /// [`crate::tagged_union::assert_lacks_only_matches_unique_missing_kind`].
            pub fn lacks_only(&self, kind: $kind) -> bool {
                <Self as $crate::tagged_union::TaggedUnion>::lacks_only(self, kind)
            }
        }

        impl $crate::tagged_union::VariantSelector<$parent> for $kind {
            type Variant<'a> = $variant<'a>;
            fn select<'a>(self, parent: &'a $parent) -> ::std::option::Option<$variant<'a>>
            where
                Self: 'a,
            {
                <$kind>::select(self, parent)
            }
        }

        impl $crate::tagged_union::TaggedUnion for $parent {
            type Kind = $kind;
            type Error = $err;
            const KIND_LIST: &'static str = $kind_list;
        }
    };
}

/// Project the borrowed-view of a tagged-union variant addressed by
/// this closed-set discriminator.
///
/// Companion trait to [`TaggedUnion`] — binds a `Kind` closed-set to
/// the parent `P` it discriminates AND to the borrowed-view
/// [`Self::Variant<'a>`] the resolver hands out. Every one of the
/// four production `.variant()` sites on `ProcessSpec`
/// ([`crate::intent::Intent`], [`crate::encapsulates::EncapsulationKind`],
/// [`crate::export::ArtifactSource`], [`crate::export::VectorChannel`])
/// pre-lift restated the same
/// `Self::Kind::ALL.into_iter().map(|k| k.select(self))` sweep body
/// verbatim at its inherent `.variant()`. Post-lift the trait binds
/// `(k.select(self), Variant<'a>)` onto ONE typed contract per Kind
/// so [`TaggedUnion::variant`]'s default body can dispatch the sweep
/// generically — the four sibling inherent bodies collapse to
/// one-line delegations and a fifth sibling picks up the sweep for
/// free through ONE `impl VariantSelector` block.
///
/// The GAT `Variant<'a>` carries the parent's lifetime so a borrowed
/// view projected from `&'a P` composes typed with the resolver's
/// short-circuit — every projection stays a compile-time refinement,
/// no `Box<dyn ...>` erasure. The GAT is additionally bound to
/// [`VariantKind<Self>`] so every implementor's borrowed view knows
/// its addressing Kind — the reverse projection of [`Self::select`]
/// closed at compile-time so a fifth sibling that adds `impl
/// VariantSelector` without opening the peer `impl VariantKind` fails
/// at the trait bound, not later at a per-consumer round-trip test.
pub trait VariantSelector<P: ?Sized>: Copy + 'static {
    /// The borrowed-view enum returned by the parent's inherent
    /// `.variant()` method — one arm per closed-set variant, each
    /// arm carrying a `&'a` reference into the parent's populated
    /// slot. Bound generically here so [`TaggedUnion::variant`]'s
    /// default body can name the return type without restating it
    /// per parent. Additionally bound to [`VariantKind<Self>`] so
    /// the reverse projection `Variant<'a> → Self` is closed at the
    /// trait boundary — every implementor's borrowed view knows its
    /// addressing Kind through ONE typed contract, and the substrate
    /// testkit [`assert_variant_round_trip`] composes `select`
    /// (forward) with `variant_kind` (reverse) generically.
    type Variant<'a>: VariantKind<Self>
    where
        P: 'a,
        Self: 'a;

    /// Project a `&'a P` borrow into the optional typed variant view
    /// for `self` (the addressed discriminator). Returns `None` iff
    /// the matching slot on `P` is `None`. Composes the closed-set
    /// sweep [`TaggedUnion::variant`] loops over.
    fn select<'a>(self, parent: &'a P) -> Option<Self::Variant<'a>>
    where
        Self: 'a;
}

/// Reverse projection — every borrowed-variant view enum knows its
/// closed-set `K` discriminator.
///
/// Dual of [`VariantSelector<P>::select`] on the addressed Kind:
/// where the selector projects a parent borrow forward into an
/// optional Variant, this trait projects a populated Variant back
/// into the Kind that addresses it. Together they compose the
/// round-trip contract every tagged-union `.variant()` site pins
/// via the substrate testkit [`assert_variant_round_trip`]:
/// `k.select(&parent).map(|v| v.variant_kind()) == Some(k)` on the
/// populated side, and `parent.variant().unwrap().variant_kind() == k`
/// through the [`TaggedUnion::variant`] resolver's default body.
///
/// Every borrowed-view enum on `ProcessSpec`'s tagged-union axis
/// ([`crate::intent::IntentVariant<'_>`],
/// [`crate::lifetime::LifetimeVariant<'_>`],
/// [`crate::encapsulates::EncapsulationKindVariant<'_>`],
/// [`crate::export::ArtifactVariant<'_>`],
/// [`crate::export::ChannelVariant<'_>`]) pre-lift restated the same
/// `match self { Self::A(_) => K::A, Self::B(_) => K::B, ... }`
/// per-arm mapping at its own inherent method (named `.kind()` on
/// four of five sites; `.target()` on
/// [`crate::encapsulates::EncapsulationKindVariant`] where the
/// discriminator's semantic role is a target of encapsulation, not
/// a kind of parent). The reverse-projection body must stay
/// per-implementor — it names the ground-truth arm-to-Kind mapping
/// only the site knows — but the CONTRACT lives at ONE typed
/// surface so:
///
/// * Every downstream generic consumer binds through
///   `<T::Variant<'_> as VariantKind<T::Kind>>::variant_kind(&v)`
///   instead of a per-parent inherent-method restatement.
/// * [`VariantSelector<P>::Variant<'a>`] bounds this trait — a
///   fifth sibling that adds `impl VariantSelector<P> for XKind`
///   without the peer `impl VariantKind<XKind> for XVariant<'_>`
///   fails at the associated-type bound, so the reverse projection
///   is closed at compile-time across every implementor.
/// * The generic testkit [`assert_variant_round_trip`] composes
///   `select` (forward) with `variant_kind` (reverse) at ONE
///   substrate site — the four sibling
///   `_kind_round_trips_through_variant_kind` /
///   `_target_round_trips_through_variant_target` test bodies
///   collapse to one-line invocations.
///
/// The trait method is named [`Self::variant_kind`] rather than
/// `kind` to avoid shadowing the inherent `.kind()` (or
/// `.target()`) methods each borrowed-view enum already publishes.
/// Every impl body is a one-line delegation to the site's inherent
/// method — the substrate stays the projection, not the mapping.
pub trait VariantKind<K: Copy + 'static> {
    /// Project a borrowed-variant view back into its addressing
    /// closed-set `K` discriminator. Round-trips the closed set on
    /// the populated side against [`VariantSelector::select`] — a
    /// value returned by `k.select(&parent).unwrap()` must satisfy
    /// `variant_kind() == k`, and a value returned by
    /// `parent.variant().unwrap()` must satisfy `variant_kind() ==
    /// k` for the populated slot's `k`.
    fn variant_kind(&self) -> K;
}

/// Generic round-trip testkit — pins that
/// [`VariantSelector::select`] (forward projection) and
/// [`VariantKind::variant_kind`] (reverse projection) compose the
/// closed set in both directions on the populated side.
///
/// Substrate primitive for the four sibling
/// `_kind_round_trips_through_variant_kind` /
/// `_target_round_trips_through_variant_target` tests on
/// `ProcessSpec` ([`crate::intent::Intent`],
/// [`crate::encapsulates::EncapsulationKind`],
/// [`crate::export::ArtifactSource`],
/// [`crate::export::VectorChannel`]) that pre-lift each restated the
/// same two-arm round-trip probe at their own test bodies:
///
/// 1. For each `k in K::ALL`, construct a parent with only slot `k`
///    populated (via a site-local `single_slot_X(k) -> Parent`
///    helper).
/// 2. Assert that `k.select(&parent).unwrap().variant_kind() == k`
///    (the forward-then-reverse round-trip).
/// 3. Assert that `parent.variant().unwrap().variant_kind() == k`
///    (the resolver-then-reverse round-trip).
///
/// Post-lift each site's round-trip test collapses to ONE
/// `assert_variant_round_trip::<T, _>(single_slot_X)` invocation
/// whose body is the substrate primitive's own dispatch. A fifth
/// sibling picks up the round-trip check through ONE call site.
///
/// The `make_parent` closure stays per-site — every one of the four
/// production sites already owns a
/// `single_slot_intent(k) / single_slot_source(k) /
/// single_slot_channel(k) / single_slot_kind(t)` helper that
/// constructs a minimally-valid parent with the addressed slot's
/// inner spec populated; the closure IS the round-trip's ground
/// truth for "populate slot k", and lifting it into the primitive
/// would collapse the per-site construction knowledge that stays
/// deliberately local.
///
/// The [`crate::lifetime::Lifetime`] site is DELIBERATELY excluded
/// — `Lifetime` doesn't impl [`TaggedUnion`] (its `variant()` returns
/// `Ok(Permanent)` on empty, not an `Empty` typed error), so the
/// `<T: TaggedUnion>` bound doesn't reach it. Its per-site
/// round-trip test binds through [`VariantKind`] directly on
/// [`crate::lifetime::LifetimeVariant`] instead.
#[track_caller]
pub fn assert_variant_round_trip<T, F>(make_parent: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for k in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = make_parent(k);
        let selected = k.select(&parent).unwrap_or_else(|| {
            panic!("VariantSelector::select must return Some for populated slot {k:?}")
        });
        assert_eq!(
            <<T::Kind as VariantSelector<T>>::Variant<'_> as VariantKind<T::Kind>>::variant_kind(
                &selected,
            ),
            k,
            "select→variant_kind round-trip failed for {k:?}",
        );
        let resolved = parent.variant().ok().unwrap_or_else(|| {
            panic!("TaggedUnion::variant must resolve exactly-one populated for {k:?}")
        });
        assert_eq!(
            <<T::Kind as VariantSelector<T>>::Variant<'_> as VariantKind<T::Kind>>::variant_kind(
                &resolved,
            ),
            k,
            "variant()→variant_kind resolver disagreed on {k:?}",
        );
    }
}

/// Declarative surface that names the (Kind, Error, KIND_LIST) triple
/// a tagged-union `.variant()` site publishes to the substrate — and
/// provides the sweep body as ONE default method every implementor
/// picks up for free.
///
/// Every one of the four production `.variant()` sites on `ProcessSpec`
/// ([`crate::intent::Intent`], [`crate::encapsulates::EncapsulationKind`],
/// [`crate::export::ArtifactSource`], [`crate::export::VectorChannel`])
/// exposes the SAME three-piece surface: a closed-set discriminator
/// [`Self::Kind`], a typed [`Self::Error`] carrier that projects onto
/// the shared [`TaggedUnionError`] contract, and a slash-joined
/// operator diagnostic literal [`Self::KIND_LIST`]. Pre-lift the
/// triple lived on each parent type as independent inherent items —
/// the (Kind, Error) types cross-referenced only by module-doc prose,
/// the `KIND_LIST` `&'static str` maintained separately at each site
/// alongside the inherent `.variant()` body. Post-lift the trait
/// binds the three onto ONE typed contract per parent so downstream
/// generic code binds to `<T: TaggedUnion>` instead of restating the
/// per-parent quadruple of associated names.
///
/// The [`Self::variant`] default method is the substrate primitive
/// every inherent `.variant()` on the four production sites delegates
/// to — one-line inherent forwarders preserve the load-bearing
/// calling convention (so no downstream callsite needs
/// `use crate::tagged_union::TaggedUnion` to reach `.variant()`) while
/// the resolve-sweep body lives at ONE substrate site. Adding a fifth
/// sibling means ONE `impl TaggedUnion` block + ONE
/// `impl VariantSelector<Self>` block on the sibling `Kind` + ONE
/// one-line inherent forwarder — no re-authored 5-line
/// `resolve_or_err(K::ALL.into_iter().map(|k| k.select(self)),
/// KIND_LIST)` sweep body.
///
/// The `Kind` type is bound to [`tatara_closed_set::ClosedSet`] so
/// generic testkit primitives (starting with
/// [`assert_kind_list_matches_closed_set`]) can compose
/// `<Self::Kind as ClosedSet>::labels_joined("/")` against
/// [`Self::KIND_LIST`] byte-identically across every implementor —
/// the diagnostic-stability invariant every sibling pre-lift pinned
/// through a hand-rolled per-site test body. It is additionally
/// bound to [`VariantSelector<Self>`] so [`Self::variant`]'s default
/// body reaches `k.select(self)` generically.
///
/// The [`crate::lifetime::Lifetime`] site is DELIBERATELY not routed
/// through this trait — its `variant()` returns `Ok(Permanent)` on
/// empty rather than an `Empty` typed error, so its projection shape
/// diverges from the four Empty-projecting siblings. Same reasoning
/// as [`resolve_or_err`]'s explicit exclusion of `Lifetime`.
pub trait TaggedUnion: Sized {
    /// The closed-set discriminator over this tagged-union's variants.
    /// Bound to [`tatara_closed_set::ClosedSet`] so the generic
    /// diagnostic-stability testkit ([`assert_kind_list_matches_closed_set`])
    /// can project `<Self::Kind as ClosedSet>::labels_joined("/")`
    /// against [`Self::KIND_LIST`] byte-identically. Additionally
    /// bound to [`VariantSelector<Self>`] so [`Self::variant`]'s
    /// default body can dispatch `k.select(self)` at each
    /// [`ClosedSet::ALL`] entry generically.
    type Kind: tatara_closed_set::ClosedSet + VariantSelector<Self>;

    /// The typed error carrier returned by the parent's inherent
    /// `.variant()` method — projects onto the shared
    /// [`TaggedUnionError`] contract so [`resolve_or_err`]'s two-arm
    /// dispatch reaches every implementor uniformly.
    type Error: TaggedUnionError;

    /// Slash-joined operator diagnostic literal — the payload of
    /// [`TaggedUnionError::empty`] when no slot is populated on this
    /// tagged union. Pinned against
    /// `<Self::Kind as tatara_closed_set::ClosedSet>::labels_joined("/")`
    /// by [`assert_kind_list_matches_closed_set`] so a variant added
    /// to `Self::Kind` without updating this constant (or a renamed
    /// variant) fails-loudly at the testkit boundary.
    const KIND_LIST: &'static str;

    /// Sweep over every [`Self::Kind`] discriminator in
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) order,
    /// projecting each into the parent's borrowed variant view via
    /// [`VariantSelector::select`], and resolve to exactly one populated
    /// variant through [`resolve_or_err`]. Errors on zero (with
    /// [`Self::KIND_LIST`] carried on the [`TaggedUnionError::empty`]
    /// arm) or many.
    ///
    /// The substrate primitive every one of the four production
    /// `.variant()` sites on `ProcessSpec` dispatches through — the
    /// per-parent inherent `.variant()` is a one-line delegation to
    /// this default so the calling convention (`intent.variant()`,
    /// `channel.variant()`, ...) stays load-bearing at the callsite
    /// without every consumer picking up `use TaggedUnion`.
    ///
    /// Adding a fifth sibling picks up this body for free — no
    /// re-authored `resolve_or_err(...)` sweep at the impl block.
    fn variant(&self) -> Result<<Self::Kind as VariantSelector<Self>>::Variant<'_>, Self::Error> {
        resolve_or_err(
            <Self::Kind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .copied()
                .map(|k| k.select(self)),
            Self::KIND_LIST,
        )
    }

    /// Widened peer of [`Self::has`] — projects a `&'a Self` borrow
    /// into the optional borrowed-variant view addressed by `kind`,
    /// or `None` when the matching slot on `Self` is empty.
    ///
    /// One-liner that delegates to [`VariantSelector::select`] on the
    /// closed-set discriminator; the substrate primitive both
    /// [`Self::has`] (via the default `self.find(kind).is_some()`
    /// body) and future diagnostic consumers (an operator-facing
    /// require-tag classifier that reads the populated slot's inner
    /// payload for a `param.key=value` message, a coherence check
    /// that projects the borrowed variant into its
    /// [`VariantKind::variant_kind`] Kind for round-trip validation
    /// without going through the resolver's Empty/Ambiguous carriers)
    /// compose against.
    ///
    /// # Sibling to [`Self::has`]
    ///
    /// One refinement wider: `has` collapses the return to a `bool`;
    /// `find` returns the matching borrowed [`VariantSelector::Variant`]
    /// so callers can read the populated slot's inner spec without
    /// re-projecting through `kind.select(self)` at the callsite (and
    /// without pulling `use VariantSelector` into scope). The default
    /// body of `has` is `self.find(kind).is_some()` — the two methods
    /// share ONE walk semantics by construction, so a regression that
    /// drifted the presence probe from the widened probe becomes
    /// structurally impossible past the trait boundary.
    ///
    /// # Peer to [`crate::boundary::ConditionSliceExt::find_kind`]
    ///
    /// Same shape, same axis, second instance in the workspace-wide
    /// `(K) -> Option<&V>` widened presence-probe algebra:
    /// [`ConditionSliceExt::find_kind`] returns `Option<&Condition>`
    /// on the slice-level ONE-shape probe; `find` here returns
    /// `Option<Variant<'_>>` on the tagged-union parent-level
    /// N-slot probe. Both refine their `has_kind` / `has` bool peer
    /// through the same `find(...).is_some()` composition law.
    ///
    /// # Semantics
    ///
    /// Returns `Some(v)` where `v` is the borrowed-view projection of
    /// the populated slot addressed by `kind`, or `None` iff that
    /// slot is `None`. Byte-for-byte equivalent to
    /// `kind.select(self)`; existing `k.select(&parent)` callsites
    /// route through this inherent surface after the macro-emitted
    /// forwarder lands.
    fn find(&self, kind: Self::Kind) -> Option<<Self::Kind as VariantSelector<Self>>::Variant<'_>> {
        kind.select(self)
    }

    /// Presence probe — does this tagged union carry a populated
    /// slot addressed by the given closed-set discriminator?
    ///
    /// Default body: `self.find(kind).is_some()`. The presence half
    /// of the resolve contract, without allocating an [`Self::Error`]
    /// carrier when the caller only needs the yes/no answer.
    /// Substrate primitive for closed-set-driven dispatch tables
    /// (e.g. tatara-check's `intent-<kind>` requires-tag sweep) where
    /// a hand-authored per-slot `spec.<field>.is_some()` chain
    /// otherwise drifts from the
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL)
    /// enumeration as new variants land.
    ///
    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` picks this up for free through the trait default
    /// — the [`declare_tagged_union_impls!`] macro emits a one-line
    /// inherent forwarder so `intent.has(kind)` reads at consumer
    /// callsites without `use TaggedUnion`. Adding a fifth sibling
    /// picks up the presence probe with zero re-authored body.
    fn has(&self, kind: Self::Kind) -> bool {
        self.find(kind).is_some()
    }

    /// Closed-set-complement peer of [`Self::has`] — `true` iff the
    /// given `kind` is MISSING (its slot on this tagged union is
    /// empty). The definitional dual of the presence probe on the
    /// MISSING axis.
    ///
    /// Default body: `!self.has(kind)`. One bit-flip; the primitive
    /// value here is naming — every consumer that reads "the missing
    /// set contains `kind`" or "the parent lacks this dependency"
    /// gets a first-class typed predicate whose call-site text reads
    /// correctly on the missing axis, without inverting the reader's
    /// parse of `!parent.has(...)` at every site.
    ///
    /// # Sibling to [`Self::has`]
    ///
    /// Closed-set-complement peer on the (populated, missing)
    /// duality: `has(kind)` names parents whose POPULATED set
    /// contains `kind`; `lacks(kind)` names parents whose MISSING
    /// set contains `kind`. The definitional complement law
    /// `lacks(kind) == !has(kind)` holds on every arm and every kind
    /// — pinned as a first-class typed invariant by the trait's own
    /// default body and swept substrate-wide by
    /// [`assert_lacks_matches_has_complement`].
    ///
    /// # Sibling to [`Self::lacks_only`]
    ///
    /// Kind-scoped strict-refinement peer on the MISSING axis:
    /// `lacks(kind)` is the SUBSET predicate (`kind` missing, maybe
    /// others too); `lacks_only(kind)` is the EQUAL predicate
    /// (`kind` missing AND ONLY `kind`). The implication
    /// `lacks_only(kind) → lacks(kind)` binds the pair on the
    /// strict-refinement axis — byte-for-byte missing-axis peer of
    /// the populated-axis `has_only(kind) → has(kind)` implication.
    /// Together with `has(kind)`, `has_only(kind)`, and
    /// `lacks_only(kind)` the four predicates close the 2×2
    /// (populated, missing) × (subset, equal) grid on the
    /// kind-scoped tagged-union axis.
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N ≥ 2` and a fixed argument `kind`:
    ///
    /// - Empty parent (0 populated, N missing): `true` — every kind
    ///   is missing, so any `kind` satisfies the predicate.
    /// - Well-formed parent with `kind` populated (1 populated ==
    ///   kind): `false` — the populated slot addresses `kind`, so
    ///   `kind` is not missing.
    /// - Well-formed parent with OTHER kind populated (1 populated
    ///   != kind): `true` — the sole populated slot is not `kind`,
    ///   so `kind` is missing.
    /// - Saturated parent (N populated, 0 missing): `false` — every
    ///   kind is populated, so `kind` is not missing.
    ///
    /// # Kind-domain cardinality
    ///
    /// `<Self::Kind as ClosedSet>::ALL.iter().filter(|k| parent.lacks(*k)).count()
    /// == parent.missing_kind_count()` — the count of kinds
    /// satisfying `lacks` on any arm is exactly the parent's
    /// missing-slot count. Closed-set-complement peer of the
    /// populated-axis law `count k where has(k) ==
    /// populated_kind_count()`. Binds the kind-scoped SUBSET
    /// primitive on the missing axis to the arg-less cardinality
    /// scalar at ONE substrate site.
    ///
    /// # Compounding future consumers
    ///
    /// - Any consumer whose semantic reading is "the missing set
    ///   contains this kind" — a "still missing: <kind>" diagnostic,
    ///   a `lacks-<kind>` require-tag classifier arm, a
    ///   dependency-satisfaction check — reads `parent.lacks(kind)`
    ///   through the inherent surface rather than negating
    ///   `parent.has(kind)` at the call site. The primitive costs
    ///   one bit-flip past [`Self::has`]; the reader-facing win is
    ///   that `!parent.has(k)` no longer needs to be re-parsed as
    ///   "the missing set contains k" at every missing-axis call
    ///   site.
    /// - The kind-scoped implication
    ///   `lacks_only(kind) → lacks(kind)` becomes a first-class
    ///   typed law binding [`Self::lacks_only`] to `lacks` on the
    ///   strict-refinement axis — byte-for-byte missing-axis peer
    ///   of `has_only(kind) → has(kind)`.
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically through the delegated
    /// [`Self::has`] — the closed-set walk extended by
    /// [`Self::has`]'s default body composition picks up the new
    /// slot at every downstream callsite without further per-caller
    /// edit.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The closed-set-complement projection lives at ONE substrate
    ///   site as a definitional negation of [`Self::has`]. The
    ///   complement law `lacks(kind) == !has(kind)` and the
    ///   kind-scoped implication `lacks_only(kind) → lacks(kind)`
    ///   are pinned across every production tagged union at compile
    ///   time via the trait's default body composition, not
    ///   per-parent.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this
    ///   primitive mechanically through the delegated [`Self::has`]
    ///   — every downstream consumer sees the widened kind set
    ///   without further per-caller edit.
    fn lacks(&self, kind: Self::Kind) -> bool {
        !self.has(kind)
    }

    /// Closed-set-inversion refinement — enumerate the set of
    /// [`Self::Kind`] discriminators whose corresponding slot on
    /// `self` is populated, in canonical
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) order.
    ///
    /// Default body:
    /// `<Kind as ClosedSet>::ALL.iter().copied().filter(|k| self.has(*k)).collect()`.
    /// A tagged-union parent that satisfies the exactly-one-slot
    /// contract returns a `Vec` of length 0 (empty parent — matches
    /// [`Self::variant`]'s `Empty` arm) or 1 (well-formed — matches
    /// the `Ok` arm); a malformed parent with multiple populated
    /// slots returns a `Vec` of length ≥ 2 in canonical `ALL` order
    /// (matches the `Ambiguous` arm and NAMES which slots are
    /// populated, unlike the payload-free `Ambiguous` carrier).
    ///
    /// # Sibling to [`Self::has`] / [`Self::find`]
    ///
    /// One refinement wider on the ORTHOGONAL axis: `has(k) / find(k)`
    /// fix a `Self::Kind` and vary the return type (`bool` /
    /// `Option<Variant>`); this refinement INVERTS the axis by fixing
    /// the parent and varying over `Kind::ALL`, returning the SET of
    /// populated kinds. The composition law
    /// `populated_kinds().contains(&k) == has(k)` for every
    /// `k ∈ Kind::ALL` binds the two axes structurally through the
    /// default body — a regression that overrode `populated_kinds`
    /// to skip a kind, return duplicates, or drift the walk order
    /// surfaces at the substrate testkit
    /// [`assert_populated_kinds_matches_has`].
    ///
    /// # Peer to [`crate::boundary::ConditionSliceExt::distinct_kinds`]
    ///
    /// Same shape, same axis, second instance in the workspace-wide
    /// closed-set-inversion refinement algebra:
    /// [`ConditionSliceExt::distinct_kinds`] returns
    /// `Vec<ConditionKind>` on the slice-level presence-probe axis
    /// (fixes the slice, varies over `ConditionKind::ALL`);
    /// `populated_kinds` here returns `Vec<Self::Kind>` on the
    /// tagged-union parent-level presence-probe axis (fixes the
    /// parent, varies over `<Self::Kind as ClosedSet>::ALL`). Both
    /// refine their `has(k) / has_kind(k)` bool peer through the
    /// same `ALL.filter(has).collect()` composition law.
    ///
    /// # Compounding future consumers
    ///
    /// - An operator-facing `Ambiguous(Vec<Kind>)` diagnostic that
    ///   NAMES which slots collide (upgrading the payload-free
    ///   [`TaggedUnionError::ambiguous`] carrier without touching the
    ///   resolver's short-circuit) reads `parent.populated_kinds()`
    ///   directly on the malformed arm.
    /// - A closed-set audit dispatcher that enumerates every
    ///   populated slot for a fleet-wide "which parents carry
    ///   {Container, Nix, Aplicacao}" query reaches ONE substrate
    ///   primitive rather than paying for a per-kind `has(k)` sweep
    ///   at every callsite.
    /// - A hypothetical `populated-kind-count-<n>` require-tag
    ///   classifier prefix family that publishes the populated-set
    ///   cardinality as a scalar reads `parent.populated_kinds().len()`.
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically (the closed-set walk picks up the
    /// new entry) and every downstream consumer sees the wider set
    /// without further per-caller edit.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The closed-set-inversion refinement lives at ONE substrate
    ///   site as a typed projection of [`Self::has`] over the closed
    ///   set `<Self::Kind as ClosedSet>::ALL`. Every downstream
    ///   aggregate consumer binds through the SAME shape rather
    ///   than restating the `ALL`-filter closure body.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically and every downstream consumer sees the wider
    ///   set with no per-caller edit.
    fn populated_kinds(&self) -> ::std::vec::Vec<Self::Kind> {
        <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| self.has(*k))
            .collect()
    }

    /// Scalar cardinality refinement on the closed-set-inversion axis —
    /// the number of [`Self::Kind`] discriminators whose corresponding
    /// slot on `self` is populated.
    ///
    /// Default body:
    /// `<Self::Kind as ClosedSet>::ALL.iter().copied().filter(|k| self.has(*k)).count()`
    /// — a closed-set walk that composes against [`Self::has`] per
    /// variant WITHOUT materializing an intermediate `Vec`. A tagged-
    /// union parent that satisfies the exactly-one-slot contract
    /// returns `0` (empty — matches [`Self::variant`]'s `Empty` arm),
    /// `1` (well-formed — matches the `Ok` arm), or `≥ 2` (malformed
    /// — matches the `Ambiguous` arm) exactly aligned with
    /// [`Self::populated_kinds`]`().len()` but without paying for the
    /// heap allocation and dealloc a caller only needing the scalar
    /// cardinality otherwise pays.
    ///
    /// # Sibling to [`Self::populated_kinds`]
    ///
    /// Scalar projection of the closed-set-inversion widened primitive
    /// — where `populated_kinds` returns the SET (a `Vec<Self::Kind>`
    /// in canonical `ClosedSet::ALL` order), `populated_kind_count`
    /// collapses that set to its cardinality. The composition law
    /// `populated_kind_count() == populated_kinds().len()` binds the
    /// scalar projection to the widened primitive at the trait's
    /// default body — a regression that overrode
    /// `populated_kind_count` to skip a kind, double-count a slot, or
    /// drift the walk from `ClosedSet::ALL` surfaces at the substrate
    /// testkit
    /// [`assert_populated_kind_count_matches_populated_kinds`].
    ///
    /// # Peer to [`crate::boundary::ConditionSliceExt::count_kind`]
    ///
    /// Not a direct peer — `count_kind(k)` on the slice-level axis
    /// fixes a `ConditionKind` and returns the per-kind cardinality
    /// (how many `Condition`s in the slice carry `k`);
    /// `populated_kind_count` on the tagged-union parent-level axis
    /// INVERTS by fixing the parent and returning the cardinality of
    /// the populated-kind SET (how many distinct slots on the parent
    /// are populated). The distinct peer to `count_kind` on the
    /// tagged-union axis would be a hypothetical `populated_slots(k)
    /// -> usize` — but since every tagged-union slot is `Option<T>`
    /// (populated or not, cardinality ∈ {0, 1}), that peer reduces
    /// to `has(k) as usize` and doesn't earn its own name. The
    /// canonical scalar peer on the tagged-union axis is this
    /// closed-set-inversion cardinality.
    ///
    /// # Sibling of [`Self::has`] / [`Self::find`] / [`Self::populated_kinds`]
    ///
    /// Fourth refinement on the tagged-union presence-probe algebra,
    /// scalar-valued on the closed-set-inversion axis: `has` collapses
    /// per-kind presence to a `bool`, `find` widens per-kind to
    /// `Option<Variant>`, `populated_kinds` inverts to the SET of
    /// populated kinds, and `populated_kind_count` scalar-projects
    /// that set to its cardinality. Every downstream consumer picks
    /// the coarsest refinement that answers its question — a
    /// `populated-kind-count-<n>` require-tag classifier prefix
    /// (called out in [`Self::populated_kinds`]'s doc-comment as a
    /// hypothetical compounding-future consumer) now reaches
    /// `parent.populated_kind_count()` at ONE substrate site rather
    /// than paying for `parent.populated_kinds().len()` (with its
    /// intermediate heap allocation) or the per-kind
    /// `<Kind::ALL>.iter().filter(|k| parent.has(*k)).count()` closure
    /// body at the callsite.
    ///
    /// # Compounding future consumers
    ///
    /// - A `populated-kind-count-<n>` require-tag classifier prefix
    ///   family that publishes the populated-set cardinality as a
    ///   scalar (the exact use case named in
    ///   [`Self::populated_kinds`]'s doc-comment) reaches this ONE
    ///   primitive without allocating.
    /// - A fast-path branch on `Ambiguous`-arm callers that need to
    ///   distinguish "well-formed" from "malformed with N slots" reads
    ///   `parent.populated_kind_count() > 1` at ONE call site rather
    ///   than reaching for the Vec-materializing widened primitive.
    /// - Any coherence check that verifies "every well-formed process
    ///   parent has exactly one populated slot" now reads
    ///   `parent.populated_kind_count() == 1` at ONE site rather than
    ///   restating `parent.populated_kinds().len() == 1` with its
    ///   allocation cost, or the semantically-equivalent (but
    ///   parent-arm-projected) `parent.variant().is_ok()`.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The scalar cardinality lives at ONE substrate site as a
    ///   typed projection of [`Self::populated_kinds`] onto its
    ///   `.len()`, and the default body composes against
    ///   [`Self::has`] over the closed set `<Self::Kind as
    ///   ClosedSet>::ALL` byte-identically to `populated_kinds`
    ///   without the intermediate `Vec`. Every downstream aggregate
    ///   consumer binds through the SAME shape rather than paying
    ///   for the allocation to reach the cardinality.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically (the closed-set walk picks up the new entry)
    ///   and every downstream consumer sees the wider cardinality
    ///   without further per-caller edit.
    fn populated_kind_count(&self) -> usize {
        <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| self.has(*k))
            .count()
    }

    /// Closed-set-COMPLEMENT refinement — enumerate the set of
    /// [`Self::Kind`] discriminators whose corresponding slot on
    /// `self` is EMPTY, in canonical
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) order.
    ///
    /// Default body:
    /// `<Kind as ClosedSet>::ALL.iter().copied().filter(|k| !self.has(*k)).collect()`.
    /// A tagged-union parent that satisfies the exactly-one-slot
    /// contract returns a `Vec` of length `ALL.len()` (empty parent —
    /// every slot is missing, aligns with [`Self::variant`]'s `Empty`
    /// arm) or `ALL.len() - 1` (well-formed — every slot BUT the
    /// populated one is missing, aligns with the `Ok` arm); a
    /// malformed parent with N populated slots returns a `Vec` of
    /// length `ALL.len() - N` in canonical `ALL` order (aligns with
    /// the `Ambiguous` arm and NAMES which slots are absent,
    /// complementing [`Self::populated_kinds`] which NAMES which are
    /// populated).
    ///
    /// # Sibling to [`Self::populated_kinds`]
    ///
    /// Closed-set-complement peer of the closed-set-inversion widened
    /// primitive — where `populated_kinds` returns the SET of
    /// populated kinds, `missing_kinds` returns its COMPLEMENT within
    /// `ClosedSet::ALL`. The two primitives PARTITION the closed set:
    /// `populated_kinds() ∪ missing_kinds() == ClosedSet::ALL` and the
    /// two sets are disjoint. The composition law
    /// `missing_kinds().contains(&k) == !has(k)` for every
    /// `k ∈ Kind::ALL` binds the two axes structurally through the
    /// default body — a regression that overrode `missing_kinds` to
    /// skip a kind, return duplicates, or drift the walk order
    /// surfaces at the substrate testkit
    /// [`assert_missing_kinds_matches_has`].
    ///
    /// # Peer to [`crate::boundary::ConditionSliceExt::missing_kinds`]
    ///
    /// Same shape, same axis, second instance in the workspace-wide
    /// closed-set-complement refinement algebra:
    /// [`ConditionSliceExt::missing_kinds`] returns
    /// `Vec<ConditionKind>` on the slice-level presence-probe axis
    /// (fixes the slice, varies over `ConditionKind::ALL` under a
    /// negated predicate); `missing_kinds` here returns
    /// `Vec<Self::Kind>` on the tagged-union parent-level presence-
    /// probe axis (fixes the parent, varies over `<Self::Kind as
    /// ClosedSet>::ALL` under a negated predicate). Both refine their
    /// `has(k) / has_kind(k)` bool peer through the same
    /// `ALL.filter(!has).collect()` composition law — the parent-axis
    /// complement of the widened `populated_kinds` primitive.
    ///
    /// # Compounding future consumers
    ///
    /// - An operator-facing "which slots are still absent" diagnostic
    ///   on the malformed / partially-populated arm reads
    ///   `parent.missing_kinds()` at ONE substrate site rather than
    ///   paying for a negated `<Kind::ALL>.iter().filter(|k|
    ///   !parent.has(*k)).collect()` closure body at the callsite —
    ///   or the strictly-worse
    ///   `<Kind::ALL>.iter().filter(|k| !parent.populated_kinds().contains(k)).collect()`
    ///   double-loop.
    /// - A future require-tag classifier arm that publishes the
    ///   missing-set membership at fleet audit time (`missing-<kind>`
    ///   as the negated peer of a hypothetical `populated-<kind>`) reads
    ///   `parent.missing_kinds().contains(&k)` at ONE call site.
    /// - A hypothetical `missing-kind-count-<n>` require-tag
    ///   classifier prefix family that publishes the missing-set
    ///   cardinality as a scalar reads [`Self::missing_kind_count`]
    ///   (the scalar-cardinality peer of this widened primitive)
    ///   without allocating.
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically (the closed-set walk picks up the
    /// new entry on the missing side WITHOUT further per-caller edit
    /// — any parent that doesn't yet populate the new slot sees it
    /// listed as missing at every downstream callsite).
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The closed-set complement lives at ONE substrate site as a
    ///   typed projection of [`Self::has`] over the closed set
    ///   `<Self::Kind as ClosedSet>::ALL` under negation. Every
    ///   downstream gap-analysis consumer binds through the SAME shape
    ///   rather than restating the negated `ALL`-filter closure body.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically and every downstream consumer sees the wider
    ///   complement without further per-caller edit.
    fn missing_kinds(&self) -> ::std::vec::Vec<Self::Kind> {
        <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| !self.has(*k))
            .collect()
    }

    /// Scalar cardinality refinement on the closed-set-complement axis —
    /// the number of [`Self::Kind`] discriminators whose corresponding
    /// slot on `self` is EMPTY.
    ///
    /// Default body:
    /// `<Self::Kind as ClosedSet>::ALL.iter().copied().filter(|k| !self.has(*k)).count()`
    /// — a closed-set walk that composes against [`Self::has`] per
    /// variant under a NEGATED point-probe, WITHOUT materializing an
    /// intermediate `Vec`. A tagged-union parent that satisfies the
    /// exactly-one-slot contract returns `ALL.len()` (empty — every
    /// slot missing, matches [`Self::variant`]'s `Empty` arm),
    /// `ALL.len() - 1` (well-formed — matches the `Ok` arm), or
    /// `ALL.len() - N` for N-populated (malformed — matches the
    /// `Ambiguous` arm), exactly aligned with [`Self::missing_kinds`]
    /// `().len()` but without paying for the heap allocation a caller
    /// only needing the scalar cardinality otherwise pays.
    ///
    /// # Sibling to [`Self::missing_kinds`] / [`Self::populated_kind_count`]
    ///
    /// Scalar projection of the closed-set-complement widened primitive
    /// — where `missing_kinds` returns the SET (a `Vec<Self::Kind>` in
    /// canonical `ClosedSet::ALL` order), `missing_kind_count`
    /// collapses that set to its cardinality. The composition law
    /// `missing_kind_count() == missing_kinds().len()` binds the
    /// scalar projection to the widened primitive at the trait's
    /// default body — a regression that overrode `missing_kind_count`
    /// to skip a kind, double-count a slot, or drift the walk from
    /// `ClosedSet::ALL` surfaces at the substrate testkit
    /// [`assert_missing_kind_count_matches_missing_kinds`].
    ///
    /// Byte-for-byte peer of [`Self::populated_kind_count`] one axis
    /// over (under a negated `has` predicate): where
    /// `populated_kind_count` scalar-projects the closed-set-INVERSION
    /// widened primitive `populated_kinds`, this method scalar-projects
    /// the closed-set-COMPLEMENT widened primitive `missing_kinds`.
    /// The two scalar projections PARTITION the closed-set cardinality:
    /// `populated_kind_count() + missing_kind_count() ==
    /// <Self::Kind as ClosedSet>::ALL.len()` — the scalar consequence
    /// of the `(populated_kinds, missing_kinds)` partition law that
    /// [`assert_missing_kinds_matches_has`] pins at the widened-
    /// primitive layer.
    ///
    /// # Peer to [`crate::boundary::ConditionSliceExt::missing_kind_count`]
    ///
    /// Same shape at the peer axis one struct layer down: fixing the
    /// slice-side carrier and inverting the presence probe over the
    /// closed set under a negated predicate. The two primitives close
    /// the "closed-set-complement scalar cardinality" refinement at
    /// two adjacent typescape sites — one per closed-set-addressed
    /// slice-level refinement, one per closed-set-addressed
    /// tagged-union parent-level refinement (this primitive).
    ///
    /// # Compounding future consumers
    ///
    /// - A `missing-kind-count-<n>` require-tag classifier prefix
    ///   family that publishes the missing-set cardinality as a scalar
    ///   (the exact use case named in [`Self::missing_kinds`]'s
    ///   doc-comment as a hypothetical compounding-future consumer)
    ///   reaches this ONE primitive without allocating.
    /// - A fast-path branch on `Ambiguous`-arm callers that need to
    ///   distinguish "one missing slot" (well-formed exactly-one) from
    ///   "N missing slots" (malformed with populated_kind_count > 1)
    ///   reads `parent.missing_kind_count() == ALL.len() - 1` at ONE
    ///   call site rather than reaching for the Vec-materializing
    ///   widened primitive.
    /// - Any coherence check that verifies "every well-formed process
    ///   parent has exactly `ALL.len() - 1` missing slots" now reads
    ///   `parent.missing_kind_count() == <Kind as ClosedSet>::ALL.len() - 1`
    ///   at ONE site rather than restating
    ///   `parent.missing_kinds().len() == ALL.len() - 1` with its
    ///   allocation cost, or the semantically-equivalent (but
    ///   parent-arm-projected) `parent.variant().is_ok()`.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The scalar cardinality lives at ONE substrate site as a typed
    ///   projection of [`Self::missing_kinds`] onto its `.len()`, and
    ///   the default body composes against [`Self::has`] over the
    ///   closed set `<Self::Kind as ClosedSet>::ALL` under negation
    ///   byte-identically to `missing_kinds` without the intermediate
    ///   `Vec`. Every downstream aggregate consumer binds through the
    ///   SAME shape rather than paying for the allocation to reach the
    ///   cardinality.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically (the closed-set walk picks up the new entry on
    ///   the missing side WITHOUT further per-caller edit — any parent
    ///   that doesn't yet populate the new slot sees the cardinality
    ///   rise by one at every downstream callsite).
    fn missing_kind_count(&self) -> usize {
        <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| !self.has(*k))
            .count()
    }

    /// Short-circuiting `Option<Self::Kind>` peer of
    /// [`Self::populated_kinds`] — the FIRST populated kind on this
    /// tagged union in canonical
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) order, or
    /// `None` when no slot is populated.
    ///
    /// Default body:
    /// `<Kind as ClosedSet>::ALL.iter().copied().find(|k| self.has(*k))`
    /// — a closed-set walk that composes against [`Self::has`] per
    /// variant and SHORT-CIRCUITS at the earliest match. An empty parent
    /// returns `None` (matches [`Self::variant`]'s `Empty` arm); a
    /// well-formed parent returns `Some(k)` where `k` is the sole
    /// populated slot (matches the `Ok` arm's variant kind through
    /// [`VariantKind`]); a malformed parent with multiple populated
    /// slots returns `Some(k)` where `k` is the EARLIEST populated
    /// slot in canonical `ALL` order — a strictly more informative
    /// projection than the payload-free [`TaggedUnionError::ambiguous`]
    /// carrier, without materializing the intermediate
    /// `Vec<Self::Kind>` [`Self::populated_kinds`] otherwise pays for.
    ///
    /// # Sibling to [`Self::populated_kinds`] / [`Self::populated_kind_count`]
    ///
    /// Third refinement on the closed-set-inversion axis, `Option<Kind>`-
    /// valued: `populated_kinds` returns the SET, `populated_kind_count`
    /// scalar-projects that set's cardinality, and `first_populated_kind`
    /// scalar-projects the SET onto its earliest element. The
    /// composition law `first_populated_kind() ==
    /// populated_kinds().first().copied()` binds the earliest-element
    /// projection to the widened primitive at the trait's default body —
    /// pinned substrate-wide by
    /// [`assert_first_populated_kind_matches_populated_kinds`]. Both
    /// coarser projections agree on emptiness:
    /// `first_populated_kind().is_none() == (populated_kind_count() == 0)`.
    ///
    /// # Peer to [`Self::variant`] on the malformed arm
    ///
    /// On well-formed parents the two projections agree
    /// (`self.variant().ok().map(|v| v.variant_kind()) ==
    /// first_populated_kind()`). On malformed (Ambiguous) parents they
    /// diverge: `variant()` returns `Err(Ambiguous)` payload-free,
    /// while `first_populated_kind()` names the earliest populated
    /// slot. Operator diagnostics that want "started at X first" text
    /// on the Ambiguous arm reach this ONE primitive with O(1) storage
    /// and short-circuit walk cost, without paying for the widened
    /// `populated_kinds().first().copied()` allocation the
    /// composition law equates it to.
    ///
    /// # Compounding future consumers
    ///
    /// - An operator-facing "Ambiguous, starting at Nix" upgrade of the
    ///   payload-free [`TaggedUnionError::ambiguous`] carrier reads
    ///   `parent.first_populated_kind()` at ONE substrate site.
    /// - A `first-populated-<kind>` require-tag classifier arm reads
    ///   this primitive with no allocation, byte-for-byte symmetrical
    ///   with `parent.has(kind)`.
    /// - A fast-path branch that discriminates "empty" from "any
    ///   populated" reads `parent.first_populated_kind().is_some()` at
    ///   ONE call site rather than allocating a `Vec` through
    ///   `!populated_kinds().is_empty()`.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The earliest-element projection lives at ONE substrate site as
    ///   a typed projection of [`Self::has`] over the closed set
    ///   `<Self::Kind as ClosedSet>::ALL` under short-circuit walk
    ///   semantics. Every downstream consumer binds through the SAME
    ///   shape rather than reaching for
    ///   `populated_kinds().first().copied()` with its allocation cost.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically (the closed-set walk picks up the new entry) —
    ///   any parent that populates only the new variant returns
    ///   `Some(new_variant)` at every downstream callsite without
    ///   further per-caller edit.
    fn first_populated_kind(&self) -> Option<Self::Kind> {
        <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .find(|k| self.has(*k))
    }

    /// Short-circuiting `Option<Self::Kind>` peer of
    /// [`Self::missing_kinds`] — the FIRST missing kind on this tagged
    /// union in canonical
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) order, or
    /// `None` when EVERY slot is populated.
    ///
    /// Default body:
    /// `<Kind as ClosedSet>::ALL.iter().copied().find(|k| !self.has(*k))`
    /// — a closed-set walk composed against [`Self::has`] per variant
    /// under NEGATION with SHORT-CIRCUIT at the earliest empty slot. An
    /// empty parent returns `Some(ALL[0])` (every slot missing, first
    /// hit is index 0); a well-formed parent populating slot `k`
    /// returns `Some(ALL[0])` if `k != ALL[0]`, else `Some(ALL[1])`
    /// (the earliest non-`k` entry); a saturated parent with every
    /// slot populated (structurally impossible on the exactly-one
    /// contract but semantically well-defined) returns `None`.
    ///
    /// # Sibling to [`Self::missing_kinds`] / [`Self::missing_kind_count`]
    ///
    /// Third refinement on the closed-set-complement axis,
    /// `Option<Kind>`-valued: `missing_kinds` returns the COMPLEMENT SET,
    /// `missing_kind_count` scalar-projects its cardinality, and
    /// `first_missing_kind` scalar-projects the SET onto its earliest
    /// element. The composition law `first_missing_kind() ==
    /// missing_kinds().first().copied()` binds the earliest-element
    /// projection to the widened primitive at the trait's default
    /// body — pinned substrate-wide by
    /// [`assert_first_missing_kind_matches_missing_kinds`]. Both
    /// coarser projections agree on saturation:
    /// `first_missing_kind().is_none() == (missing_kind_count() == 0)`.
    ///
    /// # Peer to [`Self::first_populated_kind`]
    ///
    /// Closed-set-complement peer of the closed-set-inversion earliest-
    /// element primitive under a negated `has` predicate. The two
    /// primitives PARTITION `ClosedSet::ALL`'s earliest-element
    /// projection: at least one of `first_populated_kind()` and
    /// `first_missing_kind()` is `Some` on any non-degenerate closed
    /// set (they are both `Some` iff `1 ≤ populated_kind_count() <
    /// ALL.len()`).
    ///
    /// # Compounding future consumers
    ///
    /// - An operator-facing "first still-unfilled dependency" diagnostic
    ///   on the partially-populated arm of an aggregate boundary check
    ///   reads `parent.first_missing_kind()` at ONE substrate site.
    /// - A `first-missing-<kind>` require-tag classifier arm reads this
    ///   primitive with no allocation, byte-for-byte symmetrical with
    ///   `parent.first_populated_kind()`.
    /// - A fast-path branch that discriminates "saturated" from "at
    ///   least one missing" reads `parent.first_missing_kind().is_some()`
    ///   at ONE call site rather than allocating through
    ///   `!missing_kinds().is_empty()` or paying for the full
    ///   `missing_kind_count() > 0` walk.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The complement-earliest-element projection lives at ONE
    ///   substrate site as a typed projection of [`Self::has`] over the
    ///   closed set `<Self::Kind as ClosedSet>::ALL` under negation
    ///   with short-circuit walk semantics.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically (the closed-set walk picks up the new entry on
    ///   the missing side) — every downstream consumer sees the wider
    ///   complement's earliest hit without further per-caller edit.
    fn first_missing_kind(&self) -> Option<Self::Kind> {
        <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .find(|k| !self.has(*k))
    }

    /// Short-circuiting `Option<Self::Kind>` peer of
    /// [`Self::populated_kinds`] — the LAST populated kind on this
    /// tagged union in canonical
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) order, or
    /// `None` when no slot is populated.
    ///
    /// Default body:
    /// `<Kind as ClosedSet>::ALL.iter().rev().copied().find(|k|
    /// self.has(*k))` — a REVERSED closed-set walk that composes
    /// against [`Self::has`] per variant and SHORT-CIRCUITS at the
    /// latest match. Byte-for-byte time-reversed peer of
    /// [`Self::first_populated_kind`] under identical predicate
    /// composition. An empty parent returns `None`; a well-formed
    /// parent returns `Some(k)` where `k` is the sole populated slot
    /// (matches the `Ok` arm's variant kind through [`VariantKind`]);
    /// a malformed parent with multiple populated slots returns
    /// `Some(k)` where `k` is the LATEST populated slot in canonical
    /// `ALL` order — the operator-diagnostic peer of
    /// [`Self::first_populated_kind`] on the malformed arm.
    ///
    /// # Sibling to [`Self::first_populated_kind`]
    ///
    /// FOURTH refinement on the closed-set-inversion axis under a
    /// REVERSED walk, `Option<Kind>`-valued: together with
    /// [`Self::first_populated_kind`] the two primitives project
    /// [`Self::populated_kinds`] onto its endpoint pair (earliest,
    /// latest). On the well-formed (exactly-one) arm they agree
    /// (`first_populated_kind() == last_populated_kind()` = `Some(k)`);
    /// on the empty arm they agree (`None`); on the malformed
    /// (Ambiguous) arm they disagree exactly when the populated set
    /// has cardinality `> 1` (the operator-diagnostic contract
    /// `"Ambiguous, from X to Y"` reads both projections at ONE call
    /// site through this trait's default bodies).
    ///
    /// The composition law `last_populated_kind() ==
    /// populated_kinds().last().copied()` binds the latest-element
    /// projection to the widened primitive at the trait's default
    /// body — pinned substrate-wide by
    /// [`assert_last_populated_kind_matches_populated_kinds`]. Both
    /// coarser projections agree on emptiness:
    /// `last_populated_kind().is_none() == (populated_kind_count() == 0)`.
    ///
    /// # Compounding future consumers
    ///
    /// - The `"Ambiguous, from X to Y"` upgrade of the payload-free
    ///   [`TaggedUnionError::ambiguous`] carrier reads
    ///   `parent.first_populated_kind()` AND
    ///   `parent.last_populated_kind()` at TWO substrate primitives
    ///   with O(1) storage on each side.
    /// - A `last-populated-<kind>` require-tag classifier arm reads
    ///   this primitive with no allocation, byte-for-byte symmetrical
    ///   with `parent.first_populated_kind()`.
    /// - A fast-path branch that discriminates "empty" from "any
    ///   populated" gains a REVERSED short-circuit option
    ///   (`parent.last_populated_kind().is_some()`) that commits to
    ///   the latest-populated slot's identity rather than the
    ///   earliest.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The latest-element projection lives at ONE substrate site as
    ///   a typed projection of [`Self::has`] over the closed set
    ///   `<Self::Kind as ClosedSet>::ALL` under REVERSED short-circuit
    ///   walk semantics — byte-for-byte time-reversed peer of the
    ///   earliest-element projection.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically (the reversed closed-set walk picks up the new
    ///   entry at its canonical `ALL` position) — every downstream
    ///   consumer sees the wider latest-hit projection with no
    ///   per-caller edit.
    fn last_populated_kind(&self) -> Option<Self::Kind> {
        <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .rev()
            .copied()
            .find(|k| self.has(*k))
    }

    /// Short-circuiting `Option<Self::Kind>` peer of
    /// [`Self::missing_kinds`] — the LAST missing kind on this tagged
    /// union in canonical
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) order, or
    /// `None` when EVERY slot is populated.
    ///
    /// Default body:
    /// `<Kind as ClosedSet>::ALL.iter().rev().copied().find(|k|
    /// !self.has(*k))` — a REVERSED closed-set walk composed against
    /// [`Self::has`] per variant under NEGATION with SHORT-CIRCUIT at
    /// the latest empty slot. Byte-for-byte time-reversed peer of
    /// [`Self::first_missing_kind`] under identical predicate
    /// composition. An empty parent returns `Some(ALL[ALL.len()-1])`
    /// (every slot missing, latest hit is the last index); a well-
    /// formed parent populating slot `k` returns
    /// `Some(ALL[ALL.len()-1])` when `k != ALL[ALL.len()-1]`, else
    /// `Some(ALL[ALL.len()-2])` (the latest non-`k` entry); a
    /// saturated parent returns `None`.
    ///
    /// # Sibling to [`Self::first_missing_kind`]
    ///
    /// FOURTH refinement on the closed-set-complement axis under a
    /// REVERSED walk, `Option<Kind>`-valued: together with
    /// [`Self::first_missing_kind`] the two primitives project
    /// [`Self::missing_kinds`] onto its endpoint pair (earliest,
    /// latest). The composition law `last_missing_kind() ==
    /// missing_kinds().last().copied()` binds the latest-element
    /// projection to the widened primitive at the trait's default
    /// body — pinned substrate-wide by
    /// [`assert_last_missing_kind_matches_missing_kinds`]. Both
    /// coarser projections agree on saturation:
    /// `last_missing_kind().is_none() == (missing_kind_count() == 0)`.
    ///
    /// # Endpoint partition
    ///
    /// Together with [`Self::first_populated_kind`],
    /// [`Self::first_missing_kind`], and [`Self::last_populated_kind`],
    /// this primitive closes the FOUR-corner endpoint projection of
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) on the
    /// (populated, missing) × (earliest, latest) product — every
    /// endpoint-addressable coherence check reads ONE of the four at
    /// ONE call site without allocating a `Vec<Self::Kind>` through
    /// `populated_kinds()` / `missing_kinds()`.
    ///
    /// # Compounding future consumers
    ///
    /// - An operator-facing "last still-unfilled dependency"
    ///   diagnostic on the partially-populated arm of an aggregate
    ///   boundary check reads `parent.last_missing_kind()` at ONE
    ///   substrate site.
    /// - A `last-missing-<kind>` require-tag classifier arm reads this
    ///   primitive with no allocation, byte-for-byte symmetrical with
    ///   `parent.last_populated_kind()`.
    /// - A fast-path branch that discriminates "saturated" from "at
    ///   least one missing" now has two symmetric short-circuit walk
    ///   options (`parent.first_missing_kind().is_some()` from the
    ///   FORWARD walk, `parent.last_missing_kind().is_some()` from
    ///   the REVERSED walk) both returning the same Boolean
    ///   projection but committing to different endpoint disclosures.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The complement-latest-element projection lives at ONE
    ///   substrate site as a typed projection of [`Self::has`] over
    ///   the closed set `<Self::Kind as ClosedSet>::ALL` under
    ///   REVERSED negation-and-short-circuit walk semantics.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically (the reversed closed-set walk picks up the new
    ///   entry at its canonical `ALL` position on the missing side).
    fn last_missing_kind(&self) -> Option<Self::Kind> {
        <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .rev()
            .copied()
            .find(|k| !self.has(*k))
    }

    /// Exactly-one-populated `Option<Self::Kind>` peer of
    /// [`Self::populated_kinds`] — `Some(k)` iff `k` is the SOLE
    /// populated kind on this tagged union, else `None`.
    ///
    /// Default body walks `<Self::Kind as ClosedSet>::ALL` under
    /// [`Self::has`] and returns `Some(k)` iff EXACTLY ONE hit is seen,
    /// short-circuiting at the SECOND hit — a two-step iterator peer
    /// of the earliest / latest short-circuit walks whose truth-table
    /// projection is disjoint from `first_populated_kind` /
    /// `last_populated_kind` on the malformed arm (both endpoints name
    /// SOME populated slot on a two-populated parent, `unique` names
    /// `None`).
    ///
    /// # Sibling to [`Self::first_populated_kind`] / [`Self::last_populated_kind`]
    ///
    /// FIFTH refinement on the closed-set-inversion axis under
    /// exactly-one-hit semantics, `Option<Kind>`-valued: together with
    /// [`Self::first_populated_kind`] and [`Self::last_populated_kind`]
    /// the three primitives project [`Self::populated_kinds`] onto its
    /// cardinality-conditioned scalar identity. On the well-formed
    /// (exactly-one) arm all three agree (`unique == first == last =
    /// Some(k)`); on the empty arm all three agree (`None`); on the
    /// malformed (Ambiguous, cardinality ≥ 2) arm the three DIVERGE:
    /// `first`/`last` name the endpoint populated slots (Some), while
    /// `unique` returns `None` — the ONLY endpoint-projection primitive
    /// in the algebra that distinguishes well-formed from malformed at
    /// its return type without paying for a [`Self::variant`] error-
    /// carrier allocation.
    ///
    /// The composition laws
    /// `unique_populated_kind().is_some() == (populated_kind_count() == 1)`
    /// and (on the `Some` arm) `unique_populated_kind() ==
    /// first_populated_kind() == last_populated_kind()` bind the
    /// exactly-one scalar identity to the widened primitives at the
    /// trait's default body — pinned substrate-wide by
    /// [`assert_unique_populated_kind_matches_populated_kinds`].
    ///
    /// # Peer to [`Self::variant`] as a kind-only projection
    ///
    /// Byte-for-byte equivalent to
    /// `self.variant().ok().map(|v| v.variant_kind())` on the trait's
    /// exactly-one contract, but WITHOUT paying for the [`Self::Error`]
    /// carrier's allocation on the failing arms, and WITHOUT reaching
    /// [`VariantSelector::Variant`] / [`VariantKind::variant_kind`]. A
    /// `use TaggedUnion` scope at the consumer is enough; the borrowed
    /// variant view is not needed. On well-formed parents the two
    /// projections agree; on empty AND malformed parents they agree by
    /// returning `None` (unlike `first_populated_kind`, which returns
    /// `Some` on malformed).
    ///
    /// # Compounding future consumers
    ///
    /// - A closed-set-driven "resolved kind identity" dispatch that
    ///   only needs the Kind (not the borrowed variant) reads
    ///   `parent.unique_populated_kind()` at ONE substrate site — one
    ///   short-circuit walk, no error-carrier allocation, no
    ///   VariantKind projection.
    /// - A coherence check that verifies "every well-formed process
    ///   parent has a unique populated kind" now reads
    ///   `parent.unique_populated_kind().is_some()` at ONE site rather
    ///   than restating `parent.populated_kind_count() == 1` (which
    ///   discards the resolved kind identity) or
    ///   `parent.variant().is_ok()` (which pays for the error carrier).
    /// - A future require-tag classifier arm that publishes the
    ///   exactly-one resolved kind (`unique-populated-<kind>`) at fleet
    ///   audit time reads this primitive with no allocation, byte-for-
    ///   byte symmetrical with the `first-populated-<kind>` and
    ///   `last-populated-<kind>` sibling classifier families.
    /// - A fast-path branch on the (empty, well-formed, ambiguous)
    ///   trichotomy that needs to distinguish "well-formed with kind X"
    ///   from BOTH "empty" AND "ambiguous" reaches this primitive at
    ///   ONE call site: `Some(k)` names the well-formed arm's kind,
    ///   `None` collapses the two failing arms together.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The exactly-one-hit projection lives at ONE substrate site as
    ///   a typed two-step-short-circuit walk over
    ///   `<Self::Kind as ClosedSet>::ALL` under [`Self::has`]. The
    ///   composition laws above compose the SAME shape as the endpoint
    ///   projections, differing only in the truth-table arm on the
    ///   malformed side.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically — any parent populating only the new variant
    ///   returns `Some(new_variant)` at every downstream callsite
    ///   without further per-caller edit.
    fn unique_populated_kind(&self) -> Option<Self::Kind> {
        let mut iter = <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| self.has(*k));
        let first = iter.next()?;
        match iter.next() {
            None => Some(first),
            Some(_) => None,
        }
    }

    /// Exactly-one-missing `Option<Self::Kind>` peer of
    /// [`Self::missing_kinds`] — `Some(k)` iff `k` is the SOLE missing
    /// kind on this tagged union, else `None`.
    ///
    /// Default body walks `<Self::Kind as ClosedSet>::ALL` under a
    /// NEGATED [`Self::has`] predicate and returns `Some(k)` iff
    /// EXACTLY ONE empty slot is seen, short-circuiting at the SECOND
    /// empty slot. Byte-for-byte peer of
    /// [`Self::unique_populated_kind`] under the complement axis.
    ///
    /// # Sibling to [`Self::first_missing_kind`] / [`Self::last_missing_kind`]
    ///
    /// FIFTH refinement on the closed-set-complement axis under
    /// exactly-one-hit semantics, `Option<Kind>`-valued: together with
    /// [`Self::first_missing_kind`] and [`Self::last_missing_kind`] the
    /// three primitives project [`Self::missing_kinds`] onto its
    /// cardinality-conditioned scalar identity on the empty side. The
    /// composition laws
    /// `unique_missing_kind().is_some() == (missing_kind_count() == 1)`
    /// and (on the `Some` arm) `unique_missing_kind() ==
    /// first_missing_kind() == last_missing_kind()` bind the exactly-
    /// one scalar identity to the widened primitives at the trait's
    /// default body — pinned substrate-wide by
    /// [`assert_unique_missing_kind_matches_missing_kinds`].
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N`:
    ///
    /// - Empty parent (0 populated, N missing): `None` (N ≥ 2 missing
    ///   on any non-degenerate closed set, so not unique).
    /// - Well-formed parent (1 populated, N-1 missing): `None` when
    ///   `N > 2` (N-1 ≥ 2 missing, not unique), `Some(the-one-missing)`
    ///   when `N == 2` (exactly one missing — the peer of the
    ///   populated slot).
    /// - N-1-populated parent (structurally the missing-side peer of
    ///   the well-formed arm): `Some(the-lone-empty)` — the ONLY arm
    ///   where `unique_missing_kind` returns `Some` on a `N > 2`
    ///   closed set.
    /// - Saturated parent (N populated, 0 missing): `None`.
    ///
    /// # Peer to [`Self::unique_populated_kind`]
    ///
    /// Closed-set-complement peer of the closed-set-inversion exactly-
    /// one-hit primitive under a negated `has` predicate. The two
    /// primitives are useful in DIFFERENT structural regimes: the
    /// populated peer names well-formed parents (1 populated of N),
    /// the missing peer names the missing-side complement (1 missing
    /// of N). On tagged unions with `N == 2` (rare — most `ALL`s are
    /// ≥ 3) the two coincide (a well-formed 1-of-2 parent has 1
    /// missing too).
    ///
    /// # Compounding future consumers
    ///
    /// - An operator-facing "one dependency still unfulfilled: X"
    ///   diagnostic on an aggregate boundary check reads
    ///   `parent.unique_missing_kind()` at ONE substrate site — one
    ///   short-circuit walk, no allocation.
    /// - A `unique-missing-<kind>` require-tag classifier arm reads
    ///   this primitive with no allocation, byte-for-byte symmetrical
    ///   with `parent.unique_populated_kind()`.
    /// - A fast-path branch on the near-saturation arm that
    ///   discriminates "exactly one slot still empty" from "0 or ≥ 2
    ///   still empty" reads `parent.unique_missing_kind().is_some()`
    ///   at ONE call site.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The complement-exactly-one-hit projection lives at ONE
    ///   substrate site as a typed two-step-short-circuit walk over
    ///   `<Self::Kind as ClosedSet>::ALL` under a negated
    ///   [`Self::has`] predicate — byte-for-byte peer of the
    ///   populated-side primitive under complement.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically on the missing side.
    fn unique_missing_kind(&self) -> Option<Self::Kind> {
        let mut iter = <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| !self.has(*k));
        let first = iter.next()?;
        match iter.next() {
            None => Some(first),
            Some(_) => None,
        }
    }

    /// Boolean cardinality-endpoint peer of [`Self::populated_kinds`] —
    /// `true` iff NO slot on this tagged union is populated.
    ///
    /// Default body:
    /// `!<Self::Kind as ClosedSet>::ALL.iter().copied().any(|k| self.has(k))`
    /// — a short-circuiting closed-set walk under [`Self::has`] that
    /// returns `true` iff every point-probe returns `false`, WITHOUT
    /// materializing the [`Vec`] `populated_kinds` would build and
    /// WITHOUT paying for the `usize` `populated_kind_count` would
    /// count. The `!any` composition short-circuits at the FIRST
    /// populated slot on the non-empty arms — strictly cheaper than
    /// either widened primitive on every arm where the parent has ≥ 1
    /// populated slot.
    ///
    /// # Sibling to [`Self::populated_kind_count`]
    ///
    /// Boolean cardinality-endpoint peer of the scalar cardinality
    /// primitive — where `populated_kind_count` returns the FULL scalar
    /// (any `usize` in `0..=ALL.len()`), `is_empty` collapses that
    /// scalar to its zero-arm Boolean projection. The composition law
    /// `is_empty() == (populated_kind_count() == 0)` binds the Boolean
    /// projection to the scalar primitive at the trait's default body —
    /// swept substrate-wide by
    /// [`assert_is_empty_matches_populated_kind_count`]. Byte-for-byte
    /// symmetrical with [`Self::is_saturated`] under the (populated,
    /// missing) complement axis: where `is_empty` names the zero-arm
    /// of the populated cardinality, `is_saturated` names the zero-arm
    /// of the missing cardinality (equivalently, the top-arm of the
    /// populated cardinality — `populated_kind_count() == ALL.len()`).
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N ≥ 1`:
    ///
    /// - Empty parent (0 populated, N missing): `true` — the SOLE
    ///   arm where `is_empty` returns `true`. Aligns with
    ///   [`Self::variant`]'s `Empty` arm (which returns
    ///   [`TaggedUnionError::empty`] carrying `KIND_LIST` verbatim).
    ///   The [`Self::empty`](TaggedUnionError::empty) factory produces
    ///   parents on this arm — pins one direction of the "empty ↔
    ///   is_empty()" symmetry.
    /// - Well-formed parent (1 populated, N-1 missing): `false`.
    /// - K-populated parent for `1 ≤ K ≤ N`: `false`.
    /// - Saturated parent (N populated, 0 missing): `false`.
    ///
    /// # Compounding future consumers
    ///
    /// - A fast-path branch that discriminates "any content at all"
    ///   from "empty carrier" — the most common tagged-union top-level
    ///   guard — reads `parent.is_empty()` at ONE substrate site with
    ///   ONE short-circuit walk (returns at the first populated slot),
    ///   rather than reaching for either `populated_kind_count() == 0`
    ///   (which walks every slot) or `!variant().is_ok()` (which pays
    ///   for the borrowed-view projection and the error-carrier
    ///   materialization on failing arms).
    /// - An operator-facing "carrier missing content" diagnostic on
    ///   the `Empty` arm of [`Self::variant`] reads `parent.is_empty()`
    ///   at ONE substrate site — one short-circuit walk, no allocation,
    ///   no error-carrier materialization.
    /// - An `is-empty` require-tag classifier arm reaches this
    ///   primitive at ONE call site, byte-for-byte symmetrical with
    ///   the sibling `is-saturated` arm.
    /// - A coherence check verifying "every well-formed parent has at
    ///   least one populated slot" reads `!parent.is_empty()` at ONE
    ///   site rather than the widened-primitive composition
    ///   `!parent.populated_kinds().is_empty()` (which pays for the
    ///   Vec) or `parent.populated_kind_count() > 0` (which walks every
    ///   slot).
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically (the closed-set walk picks up the
    /// new entry as an additional short-circuit slot — a parent that
    /// populates ONLY the new variant returns `false` at every
    /// downstream callsite without further per-caller edit; an all-
    /// empty parent continues to return `true` past every entry
    /// including the new one).
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The Boolean cardinality-endpoint projection lives at ONE
    ///   substrate site as a typed short-circuiting closed-set walk
    ///   `!<Self::Kind as ClosedSet>::ALL.iter().any(has)` — byte-
    ///   for-byte peer of `populated_kind_count()` composed against
    ///   `== 0`, but without the counter allocation on every arm and
    ///   with a first-populated-slot short-circuit that neither
    ///   widened primitive offers.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically through the `any` short-circuit.
    fn is_empty(&self) -> bool {
        !<Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .any(|k| self.has(k))
    }

    /// Boolean cardinality-endpoint peer of [`Self::missing_kinds`] —
    /// `true` iff EVERY slot on this tagged union is populated (i.e.
    /// the missing set is empty).
    ///
    /// Default body:
    /// `<Self::Kind as ClosedSet>::ALL.iter().copied().all(|k| self.has(k))`
    /// — a short-circuiting closed-set walk under [`Self::has`] that
    /// returns `true` iff every point-probe returns `true`, WITHOUT
    /// materializing the [`Vec`] `missing_kinds` would build and
    /// WITHOUT paying for the `usize` `missing_kind_count` would
    /// count. The `all` composition short-circuits at the FIRST
    /// missing slot on the non-saturated arms — strictly cheaper than
    /// either widened primitive on every arm where the parent has ≥ 1
    /// missing slot.
    ///
    /// # Sibling to [`Self::missing_kind_count`]
    ///
    /// Boolean cardinality-endpoint peer of the scalar cardinality
    /// primitive — where `missing_kind_count` returns the FULL scalar
    /// (any `usize` in `0..=ALL.len()`), `is_saturated` collapses that
    /// scalar to its zero-arm Boolean projection. The composition law
    /// `is_saturated() == (missing_kind_count() == 0)` binds the
    /// Boolean projection to the scalar primitive at the trait's
    /// default body — swept substrate-wide by
    /// [`assert_is_saturated_matches_missing_kind_count`]. Byte-for-
    /// byte symmetrical with [`Self::is_empty`] under the (populated,
    /// missing) complement axis: where `is_empty` names the zero-arm
    /// of the populated cardinality, `is_saturated` names the zero-arm
    /// of the missing cardinality.
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N ≥ 1`:
    ///
    /// - Empty parent (0 populated, N missing): `false`.
    /// - Well-formed parent (1 populated, N-1 missing): `false` (on
    ///   any `N ≥ 2` closed set). On the degenerate `N == 1` closed
    ///   set the well-formed and saturated arms coincide — both
    ///   primitives return `false` on the empty arm and `true` on the
    ///   single-populated arm — but real-world tagged unions in this
    ///   workspace all have `N ≥ 2`.
    /// - K-populated parent for `0 ≤ K < N`: `false`.
    /// - Saturated parent (N populated, 0 missing): `true` — the SOLE
    ///   arm where `is_saturated` returns `true`.
    ///
    /// # Compounding future consumers
    ///
    /// - A fast-path branch that discriminates "over-populated"
    ///   (saturated, structurally malformed on any `N ≥ 2` tagged
    ///   union) from "well-formed or partial" reads
    ///   `parent.is_saturated()` at ONE substrate site with ONE short-
    ///   circuit walk (returns at the first missing slot), rather than
    ///   reaching for either `missing_kind_count() == 0` (which walks
    ///   every slot) or `populated_kind_count() == ALL.len()` (same
    ///   cost, different axis).
    /// - An operator-facing "over-populated carrier" diagnostic that
    ///   surfaces the pathological case where every slot on a `N ≥ 2`
    ///   tagged union is populated reads `parent.is_saturated()` at
    ///   ONE substrate site — one short-circuit walk, no allocation.
    /// - An `is-saturated` require-tag classifier arm reaches this
    ///   primitive at ONE call site, byte-for-byte symmetrical with
    ///   the sibling `is-empty` arm.
    /// - A coherence check verifying "no production tagged union has
    ///   ever been observed saturated" reads `!parent.is_saturated()`
    ///   at ONE site — the substrate's structural pin on the top-arm
    ///   of the cardinality lattice.
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically (the closed-set walk picks up the
    /// new entry as an additional short-circuit slot — a parent that
    /// was previously saturated is no longer saturated at every
    /// downstream callsite unless it also populates the new slot; a
    /// parent that populates every slot including the new one
    /// continues to return `true`).
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The Boolean cardinality-top-endpoint projection lives at ONE
    ///   substrate site as a typed short-circuiting closed-set walk
    ///   `<Self::Kind as ClosedSet>::ALL.iter().all(has)` — byte-for-
    ///   byte peer of `missing_kind_count()` composed against `== 0`,
    ///   but without the counter allocation on every arm and with a
    ///   first-missing-slot short-circuit that neither widened
    ///   primitive offers.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically through the `all` short-circuit.
    fn is_saturated(&self) -> bool {
        <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .all(|k| self.has(k))
    }

    /// Boolean cardinality "at-least-one" peer of
    /// [`Self::populated_kinds`] — `true` iff AT LEAST ONE slot on this
    /// tagged union is populated (i.e. the populated set is NON-empty).
    ///
    /// Default body:
    /// `<Self::Kind as ClosedSet>::ALL.iter().copied().any(|k| self.has(k))`
    /// — a short-circuiting closed-set walk under [`Self::has`] that
    /// returns `true` at the FIRST populated slot, WITHOUT materializing
    /// the [`Vec`] `populated_kinds` would build and WITHOUT paying for
    /// the `usize` `populated_kind_count` would count. The `any`
    /// composition short-circuits at the FIRST populated slot on every
    /// non-empty arm — strictly cheaper than either widened primitive
    /// `populated_kind_count() > 0` (which walks every slot) or
    /// `!populated_kinds().is_empty()` (which pays for the `Vec`
    /// allocation before the emptiness check).
    ///
    /// # Sibling to [`Self::is_empty`]
    ///
    /// Definitional-complement Boolean peer on the SAME populated
    /// cardinality axis: where [`Self::is_empty`] names the zero-arm
    /// (0 populated), `has_any_populated_kind` names the ≥ 1 halfspace
    /// (any positive cardinality). The composition law
    /// `has_any_populated_kind() == !is_empty()` binds the two primitives
    /// at the trait's default body — one bit-flip past [`Self::is_empty`]'s
    /// `!any` short-circuit. Byte-for-byte peer of
    /// [`Self::has_any_missing_kind`] under the (populated, missing)
    /// complement axis: where `has_any_missing_kind` names the ≥ 1
    /// missing halfspace via the negated `has`, this primitive names
    /// the ≥ 1 populated halfspace via the plain `has`.
    ///
    /// # Cardinality-grid closure
    ///
    /// Third row of the Boolean cardinality grid on the tagged-union
    /// parent axis — the SUBSET side of the complement dichotomy between
    /// the zero-arm and the at-least-one halfspace. The four rows now
    /// close the {0, ≥1, =1, ≥2} cardinality lattice on both the
    /// populated and missing axes:
    ///
    /// |                | populated axis                          | missing axis                           |
    /// |----------------|-----------------------------------------|----------------------------------------|
    /// | ZERO (== 0)    | [`Self::is_empty`]                      | [`Self::is_saturated`]                 |
    /// | AT LEAST ONE   | `has_any_populated_kind` (this)         | [`Self::has_any_missing_kind`]         |
    /// | UNIQUE (== 1)  | [`Self::has_unique_populated_kind`]     | [`Self::has_unique_missing_kind`]      |
    /// | AT LEAST TWO   | [`Self::has_multiple_populated_kinds`]  | [`Self::has_multiple_missing_kinds`]   |
    ///
    /// The AT LEAST ONE row partitions the ZERO row's exhaustive
    /// complement — for any given parent, `is_empty()` and
    /// `has_any_populated_kind()` XOR to `true` (exactly one returns
    /// `true`). The row is ALSO the disjunction of the UNIQUE and AT
    /// LEAST TWO rows: `has_any_populated_kind() ==
    /// has_unique_populated_kind() || has_multiple_populated_kinds()`
    /// — the {=1, ≥2} refinement of the ≥ 1 halfspace at ONE substrate
    /// site.
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N ≥ 1`:
    ///
    /// - Empty parent (0 populated, N missing): `false` — the SOLE arm
    ///   where `has_any_populated_kind` returns `false`.
    /// - Well-formed parent (1 populated, N-1 missing): `true`.
    /// - K-populated parent for `1 ≤ K ≤ N`: `true`.
    /// - Saturated parent (N populated, 0 missing): `true`.
    ///
    /// # Compounding future consumers
    ///
    /// - A boundary-progress "any content at all" diagnostic on an
    ///   aggregate condition-carrier reads
    ///   `parent.has_any_populated_kind()` at ONE substrate site — one
    ///   short-circuit walk, no allocation, and no readerly parse of
    ///   `!parent.is_empty()` inversion at the callsite.
    /// - An `is-non-empty` require-tag classifier arm reaches this
    ///   primitive at ONE call site — the SUBSET peer of the sibling
    ///   `is-empty` classifier arm, byte-for-byte symmetrical with the
    ///   sibling `has-any-missing-kind` arm under the (populated,
    ///   missing) complement axis.
    /// - A fast-path branch that discriminates "some populated" from
    ///   "all missing" (the resolver's non-`Empty`-arm halfspace) reads
    ///   `parent.has_any_populated_kind()` at ONE call site — same
    ///   FIRST-populated-slot short-circuit as [`Self::is_empty`], no
    ///   inversion.
    /// - A coherence check verifying "every production parent from a
    ///   `single_slot_X` factory is NON-empty" reads
    ///   `parent.has_any_populated_kind()` at ONE site rather than
    ///   `!parent.is_empty()` (which asks the reader to invert the
    ///   parse) or `parent.populated_kind_count() > 0` (which walks
    ///   every slot).
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically — the `any` short-circuit picks up
    /// the new slot as an additional first-hit candidate at every
    /// downstream callsite without further per-caller edit.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The Boolean at-least-one projection on the populated axis
    ///   lives at ONE substrate site as a typed short-circuiting
    ///   closed-set walk `<Self::Kind as ClosedSet>::ALL.iter().any(has)`
    ///   — byte-for-byte definitional complement of [`Self::is_empty`]'s
    ///   `!<ALL>.iter().any(has)`, semantically identical to
    ///   `populated_kind_count() > 0` on every arm with the same
    ///   first-hit short-circuit that [`Self::is_empty`] enjoys.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically through the `any` short-circuit.
    fn has_any_populated_kind(&self) -> bool {
        <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .any(|k| self.has(k))
    }

    /// Boolean cardinality "at-least-one" peer of [`Self::missing_kinds`]
    /// — `true` iff AT LEAST ONE slot on this tagged union is missing
    /// (i.e. the missing set is NON-empty).
    ///
    /// Default body:
    /// `<Self::Kind as ClosedSet>::ALL.iter().copied().any(|k| !self.has(k))`
    /// — a short-circuiting closed-set walk under a NEGATED [`Self::has`]
    /// that returns `true` at the FIRST missing slot, WITHOUT
    /// materializing the [`Vec`] `missing_kinds` would build and WITHOUT
    /// paying for the `usize` `missing_kind_count` would count. The
    /// `any` composition short-circuits at the FIRST missing slot on
    /// every non-saturated arm — strictly cheaper than either widened
    /// primitive `missing_kind_count() > 0` (which walks every slot) or
    /// `!missing_kinds().is_empty()` (which pays for the `Vec`
    /// allocation before the emptiness check).
    ///
    /// # Sibling to [`Self::is_saturated`]
    ///
    /// Definitional-complement Boolean peer on the SAME missing
    /// cardinality axis: where [`Self::is_saturated`] names the zero-arm
    /// (0 missing), `has_any_missing_kind` names the ≥ 1 missing
    /// halfspace (any positive missing cardinality). The composition
    /// law `has_any_missing_kind() == !is_saturated()` binds the two
    /// primitives at the trait's default body — one bit-flip past
    /// [`Self::is_saturated`]'s `all` short-circuit. Byte-for-byte peer
    /// of [`Self::has_any_populated_kind`] under the (populated,
    /// missing) complement axis: where `has_any_populated_kind` names
    /// the ≥ 1 populated halfspace via the plain `has`, this primitive
    /// names the ≥ 1 missing halfspace via the negated `has`.
    ///
    /// # Cardinality-grid closure
    ///
    /// Third row of the Boolean cardinality grid on the tagged-union
    /// parent axis — see [`Self::has_any_populated_kind`] for the full
    /// grid. The disjunctive decomposition
    /// `has_any_missing_kind() == has_unique_missing_kind() ||
    /// has_multiple_missing_kinds()` binds the ≥ 1 halfspace to the
    /// {=1, ≥2} refinement at ONE substrate site — byte-for-byte peer
    /// of the populated-axis disjunctive decomposition.
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N ≥ 1`:
    ///
    /// - Empty parent (0 populated, N missing): `true` — the empty
    ///   parent has EVERY slot missing.
    /// - Well-formed parent (1 populated, N-1 missing): `true` on any
    ///   `N ≥ 2`. On the degenerate `N == 1` closed set the well-formed
    ///   parent has 0 missing, so `has_any_missing_kind()` returns
    ///   `false` — but real-world tagged unions in this workspace all
    ///   have `N ≥ 2`.
    /// - K-populated parent for `0 ≤ K < N`: `true`.
    /// - Saturated parent (N populated, 0 missing): `false` — the SOLE
    ///   arm where `has_any_missing_kind` returns `false`.
    ///
    /// # Compounding future consumers
    ///
    /// - An operator-facing "not fully populated" diagnostic on an
    ///   aggregate condition-carrier reads
    ///   `parent.has_any_missing_kind()` at ONE substrate site — one
    ///   short-circuit walk, no allocation, no readerly parse of
    ///   `!parent.is_saturated()` inversion at the callsite.
    /// - A `has-any-missing-kind` require-tag classifier arm reaches
    ///   this primitive at ONE call site — the SUBSET peer of the
    ///   sibling `is-saturated` classifier arm, closed-set-complement
    ///   mirror of `has-any-populated-kind` on the populated axis.
    /// - A fast-path branch that discriminates "any slot still absent"
    ///   from "over-populated / saturated" reads
    ///   `parent.has_any_missing_kind()` at ONE call site — same
    ///   FIRST-missing-slot short-circuit as [`Self::is_saturated`],
    ///   no inversion.
    /// - A coherence check verifying "no production parent from a
    ///   `single_slot_X` factory is saturated" reads
    ///   `parent.has_any_missing_kind()` at ONE site — every
    ///   `ALL.len() ≥ 2` well-formed parent leaves `ALL.len() - 1 ≥ 1`
    ///   slot missing, so this predicate is a substrate structural pin
    ///   on the well-formed diagonal.
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically — the `any` short-circuit picks up
    /// the new slot as an additional first-hit candidate at every
    /// downstream callsite without further per-caller edit.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The Boolean at-least-one projection on the missing axis lives
    ///   at ONE substrate site as a typed short-circuiting closed-set
    ///   walk `<Self::Kind as ClosedSet>::ALL.iter().any(|k| !has(k))`
    ///   — byte-for-byte definitional complement of
    ///   [`Self::is_saturated`]'s `<ALL>.iter().all(has)` (via the De
    ///   Morgan dual), semantically identical to
    ///   `missing_kind_count() > 0` on every arm with the same first-
    ///   hit short-circuit that [`Self::is_saturated`] enjoys.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically through the `any` short-circuit.
    fn has_any_missing_kind(&self) -> bool {
        <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .any(|k| !self.has(k))
    }

    /// Boolean cardinality-mid-endpoint peer of
    /// [`Self::unique_populated_kind`] — `true` iff EXACTLY ONE slot on
    /// this tagged union is populated.
    ///
    /// Default body: `self.unique_populated_kind().is_some()` — the
    /// Boolean projection of the two-step-short-circuit closed-set walk
    /// [`Self::unique_populated_kind`] already performs, without paying
    /// for the [`Vec`] `populated_kinds` would build or the counter
    /// walk `populated_kind_count` would perform. The `unique_*`
    /// primitive short-circuits at the SECOND populated slot on the
    /// malformed arms, so the `is_some` projection here short-circuits
    /// on the same schedule — strictly cheaper than the widened
    /// primitives on every arm where the parent has ≥ 2 populated
    /// slots.
    ///
    /// # Sibling to [`Self::populated_kind_count`]
    ///
    /// Boolean cardinality-mid-endpoint peer of the scalar cardinality
    /// primitive — where `populated_kind_count` returns the FULL scalar
    /// (any `usize` in `0..=ALL.len()`), `has_unique_populated_kind`
    /// collapses that scalar to its one-arm Boolean projection. The
    /// composition law
    /// `has_unique_populated_kind() == (populated_kind_count() == 1)`
    /// binds the Boolean projection to the scalar primitive at the
    /// trait's default body — swept substrate-wide by
    /// [`assert_has_unique_populated_kind_matches_populated_kind_count`].
    /// Together with [`Self::is_empty`] (zero-arm of the populated
    /// axis) and [`Self::is_saturated`] (zero-arm of the missing
    /// axis), these three Boolean cardinality primitives close the
    /// substrate's 2×2 endpoint grid on the tagged-union parent axis:
    ///
    /// |          | populated                     | missing                        |
    /// |----------|-------------------------------|--------------------------------|
    /// | zero-arm | [`Self::is_empty`]            | [`Self::is_saturated`]         |
    /// | one-arm  | [`Self::has_unique_populated_kind`] | [`Self::has_unique_missing_kind`] |
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N ≥ 2`:
    ///
    /// - Empty parent (0 populated, N missing): `false`.
    /// - Well-formed parent (1 populated, N-1 missing): `true` — the
    ///   SOLE arm where `has_unique_populated_kind` returns `true`.
    ///   Aligns with [`Self::variant`]'s `Ok` arm (the single-populated
    ///   arm where the resolver returns exactly one variant) — this
    ///   primitive is the `bool`-valued projection of that Ok arm.
    /// - K-populated parent for `K ≥ 2`: `false`.
    /// - Saturated parent (N populated, 0 missing on any `N ≥ 2`
    ///   closed set): `false`.
    ///
    /// # Compounding future consumers
    ///
    /// - A fast-path branch that discriminates "well-formed" from
    ///   "empty or ambiguous" reads `parent.has_unique_populated_kind()`
    ///   at ONE substrate site with the same two-step short-circuit
    ///   walk `unique_populated_kind` already performs, rather than
    ///   reaching for `parent.variant().is_ok()` (which pays for the
    ///   borrowed-view projection AND the error-carrier
    ///   materialization on failing arms) or
    ///   `parent.populated_kind_count() == 1` (which walks every slot).
    /// - An operator-facing "well-formed" diagnostic on the resolver's
    ///   Ok arm reads `parent.has_unique_populated_kind()` at ONE
    ///   substrate site — one two-step short-circuit walk, no
    ///   allocation, no borrowed-view materialization.
    /// - A `has-unique-populated-kind` require-tag classifier arm
    ///   reaches this primitive at ONE call site, byte-for-byte
    ///   symmetrical with the sibling `is-empty` / `is-saturated` /
    ///   `has-unique-missing-kind` arms across the closed 2×2 grid.
    /// - A coherence check verifying "every production parent from a
    ///   `single_slot_X` factory is well-formed" reads
    ///   `parent.has_unique_populated_kind()` at ONE site rather than
    ///   the widened-primitive composition
    ///   `parent.populated_kind_count() == 1`.
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically through the `unique_populated_kind`
    /// short-circuit (the closed-set walk picks up the new entry as an
    /// additional short-circuit slot — a parent that populates ONLY
    /// the new variant returns `true` at every downstream callsite
    /// without further per-caller edit).
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The Boolean cardinality-mid-endpoint projection lives at ONE
    ///   substrate site as the `is_some` projection of the
    ///   `unique_populated_kind` two-step short-circuit walk — byte-
    ///   for-byte peer of `populated_kind_count()` composed against
    ///   `== 1`, but with a second-populated-slot short-circuit that
    ///   the scalar counter primitive does not offer.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically through the `unique_populated_kind` short-circuit.
    fn has_unique_populated_kind(&self) -> bool {
        self.unique_populated_kind().is_some()
    }

    /// Boolean cardinality-mid-endpoint peer of
    /// [`Self::unique_missing_kind`] — `true` iff EXACTLY ONE slot on
    /// this tagged union is missing.
    ///
    /// Default body: `self.unique_missing_kind().is_some()` — the
    /// Boolean projection of the two-step-short-circuit closed-set
    /// walk [`Self::unique_missing_kind`] already performs under a
    /// negated `has` predicate, without paying for the [`Vec`]
    /// `missing_kinds` would build or the counter walk
    /// `missing_kind_count` would perform. The `unique_*` primitive
    /// short-circuits at the SECOND missing slot on the partial arms,
    /// so the `is_some` projection here short-circuits on the same
    /// schedule — strictly cheaper than the widened primitives on
    /// every arm where the parent has ≥ 2 missing slots.
    ///
    /// # Sibling to [`Self::missing_kind_count`]
    ///
    /// Boolean cardinality-mid-endpoint peer of the scalar complement
    /// cardinality primitive — where `missing_kind_count` returns the
    /// FULL scalar (any `usize` in `0..=ALL.len()`),
    /// `has_unique_missing_kind` collapses that scalar to its one-arm
    /// Boolean projection. The composition law
    /// `has_unique_missing_kind() == (missing_kind_count() == 1)`
    /// binds the Boolean projection to the scalar primitive at the
    /// trait's default body — swept substrate-wide by
    /// [`assert_has_unique_missing_kind_matches_missing_kind_count`].
    /// Byte-for-byte symmetrical with [`Self::has_unique_populated_kind`]
    /// under the (populated, missing) complement axis.
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N ≥ 2`:
    ///
    /// - Empty parent (0 populated, N missing): `false` on any
    ///   `N ≥ 2` closed set (on the degenerate `N == 1` closed set
    ///   empty and one-missing coincide; no production tagged union
    ///   in this workspace has `N == 1`).
    /// - Well-formed parent (1 populated, N-1 missing): `false` on
    ///   any `N ≥ 3` closed set. On `N == 2` well-formed and one-
    ///   missing coincide — the primitive returns `true` because
    ///   `N - 1 == 1`.
    /// - K-populated parent for `2 ≤ K ≤ N-1` on `N ≥ 3` closed sets:
    ///   `false` in general; `true` only on the `(N-1)`-populated arm
    ///   (near-saturation, one slot missing).
    /// - Saturated parent (N populated, 0 missing): `false`.
    ///
    /// # Compounding future consumers
    ///
    /// - A fast-path branch on the near-saturation arm (exactly one
    ///   slot missing, structurally malformed on any `N ≥ 3` tagged
    ///   union in that it composes multiple populated slots) reads
    ///   `parent.has_unique_missing_kind()` at ONE substrate site
    ///   with the same two-step short-circuit walk
    ///   `unique_missing_kind` already performs, rather than reaching
    ///   for `parent.missing_kind_count() == 1` (which walks every
    ///   slot).
    /// - An operator-facing "one slot away from saturated" diagnostic
    ///   on the near-saturation arm reads
    ///   `parent.has_unique_missing_kind()` at ONE substrate site.
    /// - A `has-unique-missing-kind` require-tag classifier arm
    ///   reaches this primitive at ONE call site, byte-for-byte
    ///   symmetrical with the sibling `has-unique-populated-kind` arm
    ///   under the (populated, missing) complement axis.
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically through the `unique_missing_kind`
    /// short-circuit.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The Boolean cardinality-mid-endpoint projection on the
    ///   missing axis lives at ONE substrate site as the `is_some`
    ///   projection of the `unique_missing_kind` two-step short-circuit
    ///   walk — byte-for-byte peer of `missing_kind_count()` composed
    ///   against `== 1`, but with a second-missing-slot short-circuit
    ///   that the scalar counter primitive does not offer.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically through the `unique_missing_kind` short-circuit.
    fn has_unique_missing_kind(&self) -> bool {
        self.unique_missing_kind().is_some()
    }

    /// Boolean cardinality many-arm peer of
    /// [`Self::has_unique_populated_kind`] — `true` iff TWO OR MORE
    /// slots on this tagged union are populated.
    ///
    /// Default body: a two-step-short-circuit closed-set walk under
    /// [`Self::has`] that pulls two hits off the filtered iterator
    /// and returns `true` iff both are `Some`, WITHOUT paying for the
    /// [`Vec`] `populated_kinds` would build or the counter walk
    /// `populated_kind_count` would perform. Short-circuits at the
    /// SECOND populated slot — strictly cheaper than either widened
    /// primitive on every arm past the second populated slot.
    ///
    /// # Sibling to the Boolean cardinality trichotomy
    ///
    /// Third arm of the {0, 1, ≥2} cardinality trichotomy on the
    /// populated axis, closing the natural partition alongside
    /// [`Self::is_empty`] (zero-arm) and
    /// [`Self::has_unique_populated_kind`] (one-arm). Every tagged-
    /// union state satisfies EXACTLY ONE of the three predicates —
    /// the three Boolean projections partition
    /// `0..=<Self::Kind as ClosedSet>::ALL.len()` at 0, 1, and ≥2
    /// respectively. Maps directly onto the three arms of the
    /// resolver contract [`Self::variant`] returns:
    ///
    /// | populated count | Boolean primitive                       | `variant()`             |
    /// |-----------------|-----------------------------------------|-------------------------|
    /// | 0               | [`Self::is_empty`]                      | `Err(Error::empty)`     |
    /// | 1               | [`Self::has_unique_populated_kind`]     | `Ok(Variant)`           |
    /// | ≥ 2             | `has_multiple_populated_kinds` (this)   | `Err(Error::ambiguous)` |
    ///
    /// The composition law `has_multiple_populated_kinds() ==
    /// (populated_kind_count() >= 2)` binds the Boolean projection
    /// to the scalar primitive at the trait's default body — swept
    /// substrate-wide by
    /// [`assert_has_multiple_populated_kinds_matches_populated_kind_count`].
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N ≥ 2`:
    ///
    /// - Empty parent (0 populated): `false`.
    /// - Well-formed parent (1 populated): `false`.
    /// - K-populated parent for `K ≥ 2`: `true`.
    /// - Saturated parent (N populated, `N ≥ 2`): `true`.
    ///
    /// # Compounding future consumers
    ///
    /// - A fast-path branch that discriminates "ambiguous" from
    ///   "empty or well-formed" reads
    ///   `parent.has_multiple_populated_kinds()` at ONE substrate
    ///   site with a two-step short-circuit walk, rather than
    ///   `parent.variant().is_err_and(|e| matches!(e,
    ///   TaggedUnionError::Ambiguous))` (which materializes the
    ///   borrowed-view AND the error-carrier) or
    ///   `parent.populated_kind_count() >= 2` (which walks every
    ///   slot).
    /// - An operator-facing "over-populated / ambiguous carrier"
    ///   diagnostic reads `parent.has_multiple_populated_kinds()`
    ///   at ONE substrate site — one two-step short-circuit walk,
    ///   no allocation.
    /// - A `has-multiple-populated-kinds` require-tag classifier
    ///   arm reaches this primitive at ONE call site, byte-for-byte
    ///   symmetrical with the sibling zero-arm / one-arm classifier
    ///   arms across the closed 2×3 grid.
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically — the closed-set walk picks up
    /// the new slot as an additional two-step-short-circuit
    /// candidate.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The Boolean cardinality many-arm projection lives at ONE
    ///   substrate site as a typed two-step-short-circuit walk over
    ///   `<Self::Kind as ClosedSet>::ALL` under [`Self::has`] — byte-
    ///   for-byte peer of `populated_kind_count()` composed against
    ///   `>= 2`, but with a second-populated-slot short-circuit that
    ///   the scalar counter primitive does not offer.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this
    ///   primitive mechanically.
    fn has_multiple_populated_kinds(&self) -> bool {
        let mut iter = <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| self.has(*k));
        iter.next().is_some() && iter.next().is_some()
    }

    /// Boolean cardinality many-arm peer of
    /// [`Self::has_unique_missing_kind`] — `true` iff TWO OR MORE
    /// slots on this tagged union are missing.
    ///
    /// Default body: a two-step-short-circuit closed-set walk under
    /// a NEGATED [`Self::has`] predicate that pulls two hits off the
    /// filtered iterator and returns `true` iff both are `Some`.
    /// Byte-for-byte peer of [`Self::has_multiple_populated_kinds`]
    /// under the (populated, missing) complement axis.
    ///
    /// # Sibling to the Boolean cardinality trichotomy
    ///
    /// Third arm of the {0, 1, ≥2} cardinality trichotomy on the
    /// missing axis, closing the natural partition alongside
    /// [`Self::is_saturated`] (zero-arm) and
    /// [`Self::has_unique_missing_kind`] (one-arm). The composition
    /// law `has_multiple_missing_kinds() == (missing_kind_count() >=
    /// 2)` binds the Boolean projection to the scalar complement
    /// cardinality primitive at the trait's default body — swept
    /// substrate-wide by
    /// [`assert_has_multiple_missing_kinds_matches_missing_kind_count`].
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N`:
    ///
    /// - Empty parent (0 populated, N missing): `true` iff `N ≥ 2`
    ///   (every production union in the workspace).
    /// - Well-formed parent (1 populated, N-1 missing): `true` iff
    ///   `N ≥ 3`. On `N == 2` the well-formed arm has exactly one
    ///   missing slot, so this primitive returns `false`.
    /// - K-populated parent for `K ≤ N-2`: `true`.
    /// - Near-saturated parent (N-1 populated, 1 missing): `false`
    ///   (exactly one missing, not many).
    /// - Saturated parent (N populated, 0 missing): `false`.
    ///
    /// # Compounding future consumers
    ///
    /// - A fast-path branch that discriminates "≥ 2 slots still
    ///   unfulfilled" from "0 or 1 slot still unfulfilled" (an
    ///   aggregate boundary progress-guard: at least two conditions
    ///   still open) reads `parent.has_multiple_missing_kinds()` at
    ///   ONE substrate site.
    /// - An operator-facing "≥ 2 dependencies still unfulfilled"
    ///   diagnostic reads `parent.has_multiple_missing_kinds()` at
    ///   ONE substrate site — one two-step short-circuit walk under
    ///   the negated predicate.
    /// - A `has-multiple-missing-kinds` require-tag classifier arm
    ///   reaches this primitive at ONE call site, byte-for-byte
    ///   symmetrical with `has_multiple_populated_kinds` under the
    ///   complement axis.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    /// - THEORY.md §VI.1 — generation over composition.
    fn has_multiple_missing_kinds(&self) -> bool {
        let mut iter = <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| !self.has(*k));
        iter.next().is_some() && iter.next().is_some()
    }

    /// Boolean parent-state middle-arm projection — `true` iff this
    /// tagged union has AT LEAST ONE populated slot AND AT LEAST ONE
    /// missing slot, i.e. it is neither [`Self::is_empty`] nor
    /// [`Self::is_saturated`].
    ///
    /// Default body: a FUSED short-circuit closed-set walk that tracks
    /// two Boolean flags (`has_populated`, `has_missing`) across
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) under
    /// [`Self::has`] and returns `true` at the EARLIEST slot where
    /// both flags have flipped. Best-case O(2) walk (index 0 populated
    /// combined with index 1 missing, or vice versa); worst case walks
    /// the full closed set only when EVERY slot is populated or EVERY
    /// slot is missing (the two arms where the return value is `false`).
    /// Byte-for-byte cheaper than the widened composition
    /// `!self.is_empty() && !self.is_saturated()` (which walks the
    /// closed set TWICE — once under `any`, once under `all`) on every
    /// partially-populated arm.
    ///
    /// # Sibling to the parent-state trichotomy
    ///
    /// Middle arm of the natural `{Empty | Partial | Saturated}`
    /// parent-state trichotomy — orthogonal to the {0, 1, ≥2}
    /// cardinality trichotomies already closed on the populated /
    /// missing axes. Together with [`Self::is_empty`] (all-missing
    /// arm) and [`Self::is_saturated`] (all-populated arm), these three
    /// Boolean primitives partition every tagged-union state on the
    /// parent-state axis — EXACTLY ONE of the three returns `true` on
    /// any given parent whose `<Self::Kind as ClosedSet>::ALL.len() ≥
    /// 1`:
    ///
    /// | parent state | primitive                          | populated cardinality       |
    /// |--------------|------------------------------------|-----------------------------|
    /// | Empty        | [`Self::is_empty`]                 | `0`                         |
    /// | Partial      | `is_partially_populated` (this)    | `0 < populated < ALL.len()` |
    /// | Saturated    | [`Self::is_saturated`]             | `ALL.len()`                 |
    ///
    /// The trichotomy partition law
    /// `usize::from(is_empty()) + usize::from(is_partially_populated())
    /// + usize::from(is_saturated()) == 1` on every arm is a genuinely
    /// new proof binding the three parent-state endpoints together as
    /// a typed algebraic invariant — swept substrate-wide by
    /// [`assert_is_partially_populated_matches_cardinality`].
    ///
    /// # Composition laws
    ///
    /// - `is_partially_populated() == !is_empty() && !is_saturated()`
    ///   — the negation-of-both-endpoints composition, at the trait
    ///   default body's SAME fused short-circuit walk.
    /// - `is_partially_populated() == (populated_kind_count() > 0
    ///   && missing_kind_count() > 0)` — the paired scalar-projection
    ///   composition.
    /// - `is_partially_populated() == (0 < populated_kind_count()
    ///   && populated_kind_count() < ALL.len())` — the single-axis
    ///   strict-inequality composition (populated cardinality lies in
    ///   the open interval `(0, ALL.len())`).
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N ≥ 2`:
    ///
    /// - Empty parent (0 populated, N missing): `false` (empty arm).
    /// - Well-formed parent (1 populated, N-1 missing on any `N ≥ 2`):
    ///   `true` — the SOLE `Ok` arm of [`Self::variant`] lies inside
    ///   the partial region.
    /// - K-populated parent for `0 < K < N`: `true`.
    /// - Saturated parent (N populated, 0 missing on any `N ≥ 2`):
    ///   `false` (saturated arm).
    ///
    /// # Compounding future consumers
    ///
    /// - A boundary-progress "some done, some pending" diagnostic on
    ///   an aggregate condition-carrier reads
    ///   `parent.is_partially_populated()` at ONE substrate site —
    ///   the exact "in flight" arm — rather than composing
    ///   `!parent.is_empty() && !parent.is_saturated()` (two closed-
    ///   set walks) or `parent.populated_kind_count() > 0 &&
    ///   parent.missing_kind_count() > 0` (two counter walks).
    /// - A fast-path branch that discriminates "mixed" from "empty or
    ///   saturated" reads this primitive with ONE fused short-circuit
    ///   walk, strictly cheaper than either widened composition.
    /// - An `is-partially-populated` require-tag classifier arm
    ///   reaches this primitive at ONE call site, byte-for-byte
    ///   symmetrical with the sibling `is-empty` / `is-saturated`
    ///   arms on the closed parent-state trichotomy.
    /// - An operator-facing "in-flight ambiguous carrier" diagnostic
    ///   (the resolver's `Err(Ambiguous)` arm's non-saturated sub-arm)
    ///   reads `parent.is_partially_populated() && parent.has_multiple_populated_kinds()`
    ///   composing two short-circuit walks — strictly cheaper than
    ///   materializing the `variant()` error carrier.
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically — the fused walk picks up the new
    /// slot as an additional short-circuit candidate (a parent that
    /// previously satisfied `is_partially_populated` because it had
    /// both populated and missing slots continues to satisfy it; a
    /// previously-saturated parent that leaves the new slot missing
    /// becomes partially populated at every downstream callsite
    /// without further per-caller edit).
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The parent-state middle-arm projection lives at ONE
    ///   substrate site as a fused short-circuit walk over
    ///   `<Self::Kind as ClosedSet>::ALL` under [`Self::has`] with
    ///   early exit on the first observed populated/missing pair —
    ///   byte-for-byte cheaper than the widened negation-of-both-
    ///   endpoints composition, and semantically identical on every
    ///   arm. The trichotomy partition law
    ///   `is_empty + is_partially_populated + is_saturated == 1`
    ///   lives at ONE substrate site inside the testkit's per-arm
    ///   sweep — pinned across every production tagged union at
    ///   compile time via the trait's default body composition, not
    ///   per-parent.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically through the fused walk — the trichotomy holds
    ///   on the widened kind set without further per-caller edit.
    fn is_partially_populated(&self) -> bool {
        let mut has_populated = false;
        let mut has_missing = false;
        for k in <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            if self.has(k) {
                has_populated = true;
            } else {
                has_missing = true;
            }
            if has_populated && has_missing {
                return true;
            }
        }
        false
    }

    /// Kind-scoped strict refinement of [`Self::has`] — `true` iff the
    /// given `kind` is populated AND no OTHER slot on this tagged union
    /// is populated. The "exactly this one variant" predicate.
    ///
    /// Default body: a FUSED short-circuit closed-set walk under
    /// [`Self::has`] that returns `false` at the EARLIEST populated
    /// slot whose kind is NOT `kind`, and returns `true` iff the sweep
    /// completes with `kind` seen as the sole populated slot. Byte-for-
    /// byte cheaper than either widened composition
    /// `self.unique_populated_kind() == Some(kind)` (which walks until
    /// the SECOND populated slot before comparing) or
    /// `self.has(kind) && self.has_unique_populated_kind()` (two
    /// closed-set walks) on every arm where the parent carries a
    /// populated slot that isn't `kind`.
    ///
    /// # Sibling to [`Self::has`]
    ///
    /// Kind-scoped strict-refinement peer: `has(kind)` is the SUBSET
    /// predicate (`kind` populated, maybe others too); `has_only(kind)`
    /// is the EQUAL predicate (`kind` populated AND ONLY `kind`). The
    /// implication `has_only(kind) → has(kind)` binds the pair on the
    /// strict-refinement axis; the reverse implication holds only on
    /// well-formed parents (`has_unique_populated_kind() == true`).
    ///
    /// # Peer to [`Self::unique_populated_kind`]
    ///
    /// Same axis, argument-scoped projection: where
    /// `unique_populated_kind()` returns `Some(k)` iff exactly one slot
    /// is populated AND names which one, `has_only(kind)` returns
    /// `true` iff exactly one slot is populated AND that slot is the
    /// passed `kind`. The composition law
    /// `has_only(kind) == (unique_populated_kind() == Some(kind))`
    /// binds the two primitives at the trait's default body — swept
    /// substrate-wide by
    /// [`assert_has_only_matches_unique_populated_kind`].
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N ≥ 2` and a fixed argument `kind`:
    ///
    /// - Empty parent (0 populated, N missing): `false` — no populated
    ///   slot, so `kind` isn't the sole populated kind.
    /// - Well-formed parent with `kind` populated (1 populated ==
    ///   kind): `true` — the SOLE arm where `has_only(kind)` returns
    ///   `true`. Aligns with [`Self::variant`]'s `Ok(Variant)` arm
    ///   where the resolver names the same kind.
    /// - Well-formed parent with other kind populated (1 populated !=
    ///   kind): `false` — the populated slot addresses a different
    ///   kind.
    /// - K-populated parent for `K ≥ 2`: `false` — multiple populated
    ///   slots, so no single kind is the "only" one.
    /// - Saturated parent (N populated, 0 missing on any `N ≥ 2`):
    ///   `false`.
    ///
    /// # Kind-domain exhaustivity
    ///
    /// A parent satisfies `has_only(k)` for AT MOST one `k`, since two
    /// distinct kinds cannot both be the sole populated slot. On the
    /// well-formed arm the count is exactly 1 (the addressed kind); on
    /// every non-well-formed arm the count is 0. This kind-domain
    /// exhaustivity law binds the argument-scoped projection to the
    /// arg-less uniqueness predicate at ONE substrate site.
    ///
    /// # Compounding future consumers
    ///
    /// - A dispatch table that runs a per-kind branch only when the
    ///   parent is unambiguously that kind reads `parent.has_only(k)`
    ///   at ONE substrate site with ONE fused short-circuit walk —
    ///   strictly cheaper than either widened composition.
    /// - An `is-only-<kind>` require-tag classifier arm reaches this
    ///   primitive at ONE call site — the kind-scoped peer of the
    ///   arg-less `has_unique_populated_kind` classifier.
    /// - A coherence check verifying "every parent from a
    ///   `single_slot_X(k)` factory is unambiguously kind `k`" reads
    ///   `parent.has_only(k)` at ONE site — the strongest structural
    ///   pin on the well-formed diagonal.
    /// - An operator-facing "unambiguously kind=<k>" diagnostic on the
    ///   resolver's Ok arm reads `parent.has_only(k)` after
    ///   `first_populated_kind` names the resolved kind — one walk, no
    ///   allocation, no `Option<Kind>` construction.
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically — the fused walk picks up the new
    /// slot as an additional short-circuit candidate at every
    /// downstream callsite without further per-caller edit.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The kind-scoped strict-refinement projection lives at ONE
    ///   substrate site as a fused short-circuit walk over
    ///   `<Self::Kind as ClosedSet>::ALL` under [`Self::has`] with
    ///   early exit on the first populated slot whose kind is not
    ///   `kind` — byte-for-byte cheaper than the widened composition
    ///   `unique_populated_kind() == Some(kind)`, semantically
    ///   identical on every arm.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically through the fused walk.
    fn has_only(&self, kind: Self::Kind) -> bool
    where
        Self::Kind: PartialEq,
    {
        let mut saw_kind = false;
        for k in <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            if !self.has(k) {
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

    /// Kind-scoped strict refinement of `!Self::has(kind)` — `true` iff
    /// the given `kind` is MISSING AND no OTHER slot on this tagged
    /// union is missing. The "exactly this one variant is absent"
    /// predicate — closed-set-complement mirror of [`Self::has_only`].
    ///
    /// Default body: a FUSED short-circuit closed-set walk under a
    /// negated [`Self::has`] that returns `false` at the EARLIEST
    /// missing slot whose kind is NOT `kind`, and returns `true` iff
    /// the sweep completes with `kind` seen as the sole missing slot.
    /// Byte-for-byte cheaper than either widened composition
    /// `self.unique_missing_kind() == Some(kind)` (which walks until
    /// the SECOND missing slot before comparing) or
    /// `!self.has(kind) && self.has_unique_missing_kind()` (two
    /// closed-set walks) on every arm where the parent carries a
    /// missing slot that isn't `kind`.
    ///
    /// # Sibling to [`Self::has_only`]
    ///
    /// Closed-set-complement peer of [`Self::has_only`] under a negated
    /// [`Self::has`] predicate — where `has_only(kind)` names parents
    /// whose SOLE populated slot is `kind`, `lacks_only(kind)` names
    /// parents whose SOLE missing slot is `kind`. Byte-for-byte
    /// symmetrical fused-walk shape; the two primitives are useful in
    /// DIFFERENT structural regimes: `has_only` names well-formed
    /// parents (1 of N populated); `lacks_only` names the missing-side
    /// complement (N-1 of N populated — the near-saturation arm). On
    /// tagged unions with `N == 2` the two coincide (a well-formed
    /// 1-of-2 parent has 1 missing too, so `has_only(a)` and
    /// `lacks_only(b)` name the same arm iff `a != b`).
    ///
    /// # Peer to [`Self::unique_missing_kind`]
    ///
    /// Same axis, argument-scoped projection: where
    /// `unique_missing_kind()` returns `Some(k)` iff exactly one slot
    /// is missing AND names which one, `lacks_only(kind)` returns
    /// `true` iff exactly one slot is missing AND that slot is the
    /// passed `kind`. The composition law
    /// `lacks_only(kind) == (unique_missing_kind() == Some(kind))`
    /// binds the two primitives at the trait's default body — swept
    /// substrate-wide by
    /// [`assert_lacks_only_matches_unique_missing_kind`].
    ///
    /// # Truth table on the exactly-one-slot tagged-union contract
    ///
    /// For a tagged union with `<Self::Kind as ClosedSet>::ALL` of
    /// cardinality `N ≥ 2` and a fixed argument `kind`:
    ///
    /// - Empty parent (0 populated, N missing): `false` on any `N ≥ 2`
    ///   — N missing slots, so `kind` isn't the sole missing kind.
    /// - Well-formed parent (1 populated, N-1 missing): `false` when
    ///   `N > 2` (N-1 ≥ 2 missing, no unique missing); on `N == 2`
    ///   with populated `p`, `lacks_only(kind) == (kind != p)` (the
    ///   one missing slot is the non-populated one).
    /// - N-1-populated parent (missing-side peer of the well-formed
    ///   arm, 1 missing): `true` iff `kind` names the sole missing
    ///   slot — the SOLE arm where `lacks_only(kind)` returns `true`
    ///   on any `N > 2` closed set.
    /// - Saturated parent (N populated, 0 missing): `false`.
    ///
    /// # Kind-domain exhaustivity
    ///
    /// A parent satisfies `lacks_only(k)` for AT MOST one `k`, since
    /// two distinct kinds cannot both be the sole missing slot. On
    /// the near-saturation arm the count is exactly 1 (the addressed
    /// missing kind); on every other arm the count is 0. This kind-
    /// domain exhaustivity law binds the argument-scoped projection
    /// to the arg-less uniqueness predicate at ONE substrate site,
    /// byte-for-byte peer of the `has_only` exhaustivity law under
    /// complement.
    ///
    /// # Kind-scoped implication
    ///
    /// `lacks_only(kind) → !has(kind)` — if `kind` is the sole missing
    /// slot then `kind` cannot be populated. Complement mirror of the
    /// `has_only(kind) → has(kind)` implication that binds
    /// [`Self::has_only`] to [`Self::has`] on the strict-refinement
    /// axis; here the implication binds `lacks_only` to `!has` on the
    /// closed-set-complement axis.
    ///
    /// # Compounding future consumers
    ///
    /// - An operator-facing "exactly one dependency still unfulfilled:
    ///   X" diagnostic on an aggregate boundary check whose `X` is
    ///   known statically reads `parent.lacks_only(X)` at ONE
    ///   substrate site — one fused short-circuit walk, no allocation,
    ///   strictly cheaper than the widened composition.
    /// - A `lacks-only-<kind>` require-tag classifier arm reaches this
    ///   primitive at ONE call site — the argument-scoped peer of the
    ///   arg-less `has_unique_missing_kind` classifier, closed-set-
    ///   complement mirror of the `is-only-<kind>` classifier arm on
    ///   the populated axis.
    /// - A coherence check verifying "the near-saturation parent from
    ///   an `all_but_one_slot_X(k)` factory is unambiguously missing
    ///   kind `k`" reads `parent.lacks_only(k)` at ONE site — the
    ///   strongest structural pin on the missing-side well-formed
    ///   diagonal.
    /// - A fast-path branch on the near-saturation arm that
    ///   discriminates "exactly one specific slot still empty" from
    ///   "0 or ≥ 2 still empty or some OTHER slot empty" reads
    ///   `parent.lacks_only(kind)` at ONE call site — the fused-walk
    ///   short-circuit is strictly cheaper than
    ///   `parent.unique_missing_kind() == Some(kind)` on every arm
    ///   where a first-missing-slot mismatch would prune the walk
    ///   before the second missing slot.
    ///
    /// A new [`Self::Kind`] variant added to
    /// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) reaches
    /// this primitive mechanically — the fused walk picks up the new
    /// slot as an additional short-circuit candidate at every
    /// downstream callsite without further per-caller edit.
    ///
    /// # Theory grounding
    ///
    /// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
    ///   The kind-scoped strict-refinement projection on the missing
    ///   axis lives at ONE substrate site as a fused short-circuit
    ///   walk over `<Self::Kind as ClosedSet>::ALL` under a negated
    ///   [`Self::has`] with early exit on the first missing slot
    ///   whose kind is not `kind` — byte-for-byte peer of
    ///   [`Self::has_only`]'s fused walk under complement,
    ///   semantically identical to
    ///   `unique_missing_kind() == Some(kind)` on every arm.
    /// - THEORY.md §VI.1 — generation over composition. A new
    ///   [`Self::Kind`] variant added to `ALL` reaches this primitive
    ///   mechanically through the fused walk.
    fn lacks_only(&self, kind: Self::Kind) -> bool
    where
        Self::Kind: PartialEq,
    {
        let mut saw_kind = false;
        for k in <Self::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            if self.has(k) {
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
}

/// Generic diagnostic-stability testkit — pins that [`TaggedUnion::KIND_LIST`]
/// matches `<T::Kind as tatara_closed_set::ClosedSet>::labels_joined("/")`
/// byte-identically for every implementor.
///
/// Substrate primitive for the four sibling
/// `_error_empty_lists_every_kind_in_canonical_order` tests on
/// `ProcessSpec` ([`crate::intent::Intent`],
/// [`crate::encapsulates::EncapsulationKind`],
/// [`crate::export::ArtifactSource`], [`crate::export::VectorChannel`])
/// that pre-lift each restated the same
/// `assert_eq!(<XxxKind as ClosedSet>::labels_joined("/"),
/// XXX_KIND_LIST)` two-argument comparison at their own test bodies —
/// byte-identical projections whose only per-carrier knobs (the Kind
/// type + the KIND_LIST constant) are the two associated items the
/// [`TaggedUnion`] trait names. Post-lift each site collapses to ONE
/// `assert_kind_list_matches_closed_set::<Xxx>()` invocation whose
/// body is the substrate primitive's own dispatch.
///
/// A fifth sibling tagged-union parent picks up the diagnostic-
/// stability check through ONE `impl TaggedUnion for X` block + ONE
/// `assert_kind_list_matches_closed_set::<X>()` call site — no
/// re-authored `<XKind as ClosedSet>::labels_joined("/")` composition
/// at the test site, no re-authored per-site `assert_eq!` pair.
#[track_caller]
pub fn assert_kind_list_matches_closed_set<T: TaggedUnion>() {
    let derived = <T::Kind as tatara_closed_set::ClosedSet>::labels_joined("/");
    assert_eq!(
        derived,
        T::KIND_LIST,
        "TaggedUnion KIND_LIST drift — must equal <T::Kind as ClosedSet>::labels_joined(\"/\")",
    );
}

/// Generic presence-probe testkit — pins that [`TaggedUnion::has`]
/// agrees with [`VariantSelector::select`]`.is_some()` across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) entry, both
/// on the diagonal (populated slot AND matching kind → `true`) and
/// off the diagonal (populated slot BUT other kind → `false`).
///
/// Substrate primitive for the presence-probe half of the tagged-
/// union contract — dispatch tables that key off `intent-<kind>` /
/// `channel-<kind>` / `source-<kind>` require-tags gain a `.has(k)`
/// call that structurally CANNOT drift from the closed-set sweep,
/// but the pin here surfaces a `has` override that would break the
/// contract (e.g. a future specialization that always returned
/// `false`) at ONE call site rather than at every downstream
/// dispatcher.
///
/// A fifth sibling tagged-union parent picks up the presence-probe
/// check through ONE `assert_has_matches_select::<X, _>(single_slot)`
/// invocation — no re-authored `for k in K::ALL { … }` sweep at the
/// test site.
#[track_caller]
pub fn assert_has_matches_select<T, F>(single_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        for probed in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let expected = probed == populated;
            assert_eq!(
                parent.has(probed),
                expected,
                "TaggedUnion::has drift — populated={populated:?} probed={probed:?} expected={expected}",
            );
            assert_eq!(
                probed.select(&parent).is_some(),
                expected,
                "VariantSelector::select drift — populated={populated:?} probed={probed:?} expected={expected}",
            );
        }
    }
}

/// Generic widened-probe testkit — pins that [`TaggedUnion::find`]
/// agrees with [`TaggedUnion::has`] AND with
/// [`VariantSelector::select`] across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) entry, and
/// that the returned borrowed view round-trips through
/// [`VariantKind::variant_kind`] back to the addressing Kind on the
/// populated diagonal.
///
/// Substrate primitive for the widened half of the presence-probe
/// contract — dispatch tables that key off `intent-<kind>` /
/// `channel-<kind>` / `source-<kind>` require-tags gain a `.find(k)`
/// call whose return type carries the borrowed variant payload for
/// diagnostic composition (an operator-facing "channel-<kind>
/// matched with e.channel.<field>.<key>=<value>" message, a
/// coherence check that projects the borrowed variant into its
/// Kind for round-trip validation), and the pin here surfaces a
/// `find` override that would drift from the composition law
/// `has(k) == find(k).is_some()` at ONE call site rather than at
/// every downstream dispatcher.
///
/// The three sub-assertions swept per (populated, probed) pair:
///
/// 1. `parent.find(probed).is_some() == parent.has(probed)` — the
///    composition law binding [`TaggedUnion::has`] to
///    [`TaggedUnion::find`] via `find(k).is_some()`.
/// 2. `parent.find(probed).is_some() == probed.select(&parent).is_some()`
///    — the widened primitive delegates to
///    [`VariantSelector::select`] on the Kind, so a regression that
///    inlined a divergent walk body at the trait's `find` default
///    fails here rather than as silent drift at every downstream
///    diagnostic consumer.
/// 3. On the populated diagonal (`probed == populated`), the
///    returned borrowed view satisfies
///    `find(k).unwrap().variant_kind() == k` — the round-trip
///    contract that closes `find` (forward-widened) against
///    [`VariantKind::variant_kind`] (reverse projection).
///
/// A fifth sibling tagged-union parent picks up the widened-probe
/// check through ONE `assert_find_agrees_with_has::<X, _>(single_slot)`
/// invocation — no re-authored `for k in K::ALL { … }` sweep at the
/// test site.
#[track_caller]
pub fn assert_find_agrees_with_has<T, F>(single_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        for probed in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let expected = probed == populated;
            let via_has = parent.has(probed);
            let via_find = parent.find(probed).is_some();
            let via_select = probed.select(&parent).is_some();
            assert_eq!(
                via_find, via_has,
                "TaggedUnion::find drifted from has — populated={populated:?} probed={probed:?}",
            );
            assert_eq!(
                via_find, via_select,
                "TaggedUnion::find drifted from VariantSelector::select — populated={populated:?} probed={probed:?}",
            );
            assert_eq!(
                via_find, expected,
                "TaggedUnion::find truth-table drift — populated={populated:?} probed={probed:?} expected={expected}",
            );
            if expected {
                let variant = parent.find(probed).unwrap_or_else(|| {
                    panic!("TaggedUnion::find must return Some for populated slot {probed:?}",)
                });
                assert_eq!(
                    <<T::Kind as VariantSelector<T>>::Variant<'_> as VariantKind<T::Kind>>::variant_kind(
                        &variant,
                    ),
                    probed,
                    "find→variant_kind round-trip failed for {probed:?}",
                );
            }
        }
    }
}

/// Generic closed-set-inversion testkit — pins that
/// [`TaggedUnion::populated_kinds`] composes over
/// [`TaggedUnion::has`] across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) entry on
/// the single-slot side, that the returned `Vec` is the canonical
/// [`ClosedSet::ALL`]-ordered filter of `has(k)`, and that on the
/// populated diagonal `single_slot(k).populated_kinds()` equals
/// `vec![k]` exactly (length 1, canonical ordered, no drift).
///
/// Parent-axis substrate primitive for the tagged-union closed-set-
/// inversion refinement — the peer of
/// [`crate::boundary::assert_slice_refinement_composition_laws`]'s
/// `distinct_kinds` sub-arm on the slice-level presence-probe axis,
/// lifted here to the tagged-union parent-level presence-probe axis
/// (same shape, same composition operator, second instance in the
/// workspace-wide closed-set-inversion refinement algebra).
///
/// The three sub-assertions swept per (populated, probed) pair:
///
/// 1. Per-kind membership: `parent.populated_kinds().contains(&k) ==
///    parent.has(k)` for every `k ∈ ClosedSet::ALL` — a regression
///    that overrode `populated_kinds` to skip a kind, drift the walk
///    order from canonical `ALL` to slot-encounter order, or return
///    a superset containing absent kinds surfaces at the specific
///    kind's per-pair assertion.
/// 2. Canonical `ALL`-filter equality:
///    `parent.populated_kinds() == ALL.iter().copied().filter(|k|
///    parent.has(*k)).collect()` — a regression that returned
///    duplicates (a naive override that skipped dedup by
///    construction) or drifted the walk order surfaces at the
///    post-loop equality assert.
/// 3. Single-slot diagonal: `single_slot(k).populated_kinds() ==
///    vec![k]` exactly — pins the single-populated arm's cardinality
///    (length 1) and ordering (the addressed kind's own position in
///    `ALL`) together at ONE assert.
///
/// Substrate primitive for future per-parent
/// `X_populated_kinds_matches_has` tests that would otherwise each
/// restate the same nested-`for populated in K::ALL { for probed in
/// K::ALL { … } }` sweep + canonical-order equality + single-slot
/// diagonal pin — every one of the four production `.variant()`
/// parents on `ProcessSpec` binds through this ONE primitive with a
/// per-site `single_slot` factory. A fifth sibling picks up the
/// closed-set-inversion check through ONE call site — no re-authored
/// `for k in K::ALL { … }` sweep at the test surface, no re-authored
/// `assert_eq!` triad.
///
/// The `single_slot` closure stays per-site — reused verbatim from
/// the sibling primitives ([`assert_variant_round_trip`],
/// [`assert_find_agrees_with_has`],
/// [`assert_single_slot_key_matches_label`]) — the closure IS the
/// "populate slot k" ground truth for the parent's field structure.
///
/// The [`crate::lifetime::Lifetime`] site is DELIBERATELY excluded
/// through the `T: TaggedUnion` bound — `Lifetime`'s `variant()`
/// returns `Ok(Permanent)` on empty rather than an `Empty` typed
/// error, so its projection shape diverges from the four
/// Empty-projecting parents. Same reasoning as
/// [`assert_variant_round_trip`]'s /
/// [`assert_find_agrees_with_has`]'s exclusions.
#[track_caller]
pub fn assert_populated_kinds_matches_has<T, F>(single_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let kinds = parent.populated_kinds();
        // Per-kind membership composition law.
        for probed in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            assert_eq!(
                kinds.contains(&probed),
                parent.has(probed),
                "TaggedUnion::populated_kinds().contains({probed:?}) drifted from has({probed:?}) — populated={populated:?}",
            );
        }
        // Canonical ALL-filter equality — pins dedup, walk order, and
        // membership consistency at ONE assert.
        let canonical: Vec<T::Kind> = <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| parent.has(*k))
            .collect();
        assert_eq!(
            kinds, canonical,
            "TaggedUnion::populated_kinds() must yield ClosedSet::ALL-ordered subsequence where has is true (no duplicates, canonical order) — populated={populated:?}",
        );
        // Single-slot diagonal — the addressed slot IS the ONLY
        // populated slot on the parent single_slot produces, so the
        // canonical filter yields exactly [populated].
        assert_eq!(
            kinds,
            vec![populated],
            "TaggedUnion::populated_kinds() on single_slot({populated:?}) must return vec![{populated:?}] exactly",
        );
    }
}

/// Generic two-slot closed-set-inversion testkit — peer of
/// [`assert_populated_kinds_matches_has`] on the ambiguous-parent
/// side. Pins that a `two_slot(a, b)` parent's `populated_kinds()`
/// yields the canonical `ClosedSet::ALL`-ordered pair
/// `[min_all(a,b), max_all(a,b)]` (length exactly 2, dedup + walk
/// order enforced), and that per-kind membership composes
/// byte-identically against `has(k)` on the malformed-parent arm.
///
/// The two-slot fixture is the SAME factory production sites already
/// hand [`assert_two_slots_ambiguous`] — every one of the four
/// production `.variant()` parents on `ProcessSpec` composes
/// `two_slot(a, b)` through per-field `Option::or` on
/// `single_slot(a)` and `single_slot(b)`, so BOTH slots on the
/// resulting parent are populated. The primitive's off-diagonal
/// sweep (`a != b`) pins that `populated_kinds()` NAMES both
/// populated slots on the malformed arm — the diagnostic-surface
/// promise the payload-free
/// [`TaggedUnionError::ambiguous`] carrier stops short of.
///
/// The three sub-assertions swept per `(a, b)` off-diagonal pair:
///
/// 1. Cardinality: `populated_kinds().len() == 2` — a regression
///    that returned a length-1 vec (silently short-circuiting on
///    the first populated slot; drifting the walk from `ALL` to
///    single-match `find`) fails HERE at the length assert.
/// 2. Per-kind membership: `populated_kinds().contains(&k) ==
///    has(k)` for every `k ∈ ClosedSet::ALL` — the composition law
///    of the closed-set-inversion refinement, pinned on the
///    multi-populated arm.
/// 3. Canonical `ALL`-filter equality:
///    `populated_kinds() == ALL.iter().copied().filter(|k|
///    parent.has(*k)).collect()` — pins the walk order (a
///    regression that yielded `[b, a]` because it walked the two
///    populated slots in construction order instead of
///    `ClosedSet::ALL` order fails at the equality assert).
///
/// A fifth sibling tagged-union parent picks up the two-slot
/// closed-set-inversion check through ONE call site — no
/// re-authored nested-for sweep at the test surface, no re-authored
/// `assert_eq!` triad.
///
/// Same `Lifetime` exclusion as [`assert_populated_kinds_matches_has`]:
/// the `T: TaggedUnion` bound doesn't reach it.
#[track_caller]
pub fn assert_populated_kinds_across_pairs<T, F>(two_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind, T::Kind) -> T,
{
    for a in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        for b in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            if a == b {
                continue;
            }
            let parent = two_slot(a, b);
            let kinds = parent.populated_kinds();
            assert_eq!(
                kinds.len(),
                2,
                "TaggedUnion::populated_kinds() on two_slot({a:?}, {b:?}) must return exactly two populated kinds, got {kinds:?}",
            );
            for probed in <T::Kind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .copied()
            {
                assert_eq!(
                    kinds.contains(&probed),
                    parent.has(probed),
                    "TaggedUnion::populated_kinds().contains({probed:?}) drifted from has({probed:?}) — (a, b)=({a:?}, {b:?})",
                );
            }
            let canonical: Vec<T::Kind> = <T::Kind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .copied()
                .filter(|k| parent.has(*k))
                .collect();
            assert_eq!(
                kinds, canonical,
                "TaggedUnion::populated_kinds() must yield ClosedSet::ALL-ordered pair on two_slot({a:?}, {b:?}) — got {kinds:?}, expected {canonical:?}",
            );
        }
    }
}

/// Generic scalar-cardinality testkit — pins that
/// [`TaggedUnion::populated_kind_count`] agrees with
/// [`TaggedUnion::populated_kinds`]`.len()` across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement AND that on the populated diagonal
/// `single_slot(k).populated_kind_count()` equals `1` exactly (aligned
/// with the single-slot arm's `populated_kinds()` returning
/// `vec![k]`).
///
/// Parent-axis substrate primitive for the scalar-cardinality
/// refinement of the tagged-union closed-set-inversion axis — the
/// scalar projection of [`assert_populated_kinds_matches_has`]'s
/// widened primitive. Together they close the two-refinement
/// composition contract that binds
/// [`TaggedUnion::populated_kind_count`] against
/// [`TaggedUnion::populated_kinds`]:
///
/// 1. **`count ↔ kinds.len()`**: `populated_kind_count() ==
///    populated_kinds().len()` — a regression that overrode
///    `populated_kind_count` to skip a kind (returning `0` on a
///    populated parent), double-count a slot (returning `2` on a
///    single-slot parent), or drift the walk from `ClosedSet::ALL`
///    surfaces at the substrate boundary here.
/// 2. **Single-slot diagonal**: `single_slot(k).populated_kind_count()
///    == 1` — pins the well-formed arm's expected cardinality
///    against the empty (`0`) and Ambiguous (`≥ 2`) arms, at ONE
///    `assert_eq!` per addressed kind.
///
/// Substrate primitive for future per-parent
/// `X_populated_kind_count_matches_populated_kinds_len` tests that
/// would otherwise each restate the same nested-`for k in K::ALL {
/// … }` sweep + composition-law equality + single-slot cardinality
/// pin — every one of the four production `.variant()` parents on
/// `ProcessSpec` binds through this ONE primitive with a per-site
/// `single_slot` factory. A fifth sibling picks up the scalar-
/// cardinality check through ONE call site — no re-authored
/// `for k in K::ALL { … }` sweep at the test surface, no re-authored
/// `assert_eq!` pair.
///
/// The `single_slot` closure stays per-site — reused verbatim from
/// the sibling primitives ([`assert_variant_round_trip`],
/// [`assert_find_agrees_with_has`],
/// [`assert_populated_kinds_matches_has`],
/// [`assert_single_slot_key_matches_label`]) — the closure IS the
/// "populate slot k" ground truth for the parent's field structure.
///
/// The [`crate::lifetime::Lifetime`] site is DELIBERATELY excluded
/// through the `T: TaggedUnion` bound — `Lifetime`'s `variant()`
/// returns `Ok(Permanent)` on empty rather than an `Empty` typed
/// error, so its projection shape diverges from the four
/// Empty-projecting parents. Same reasoning as
/// [`assert_variant_round_trip`]'s /
/// [`assert_find_agrees_with_has`]'s /
/// [`assert_populated_kinds_matches_has`]'s exclusions.
#[track_caller]
pub fn assert_populated_kind_count_matches_populated_kinds<T, F>(single_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let count = parent.populated_kind_count();
        let kinds_len = parent.populated_kinds().len();
        // Composition law: scalar cardinality projection agrees with
        // the widened primitive's `Vec::len()`.
        assert_eq!(
            count, kinds_len,
            "TaggedUnion::populated_kind_count() drifted from populated_kinds().len() — populated={populated:?}",
        );
        // Single-slot diagonal — a well-formed parent from single_slot
        // populates exactly the addressed slot, so the scalar cardinality
        // is 1.
        assert_eq!(
            count, 1,
            "TaggedUnion::populated_kind_count() on single_slot({populated:?}) must equal 1 exactly (well-formed arm cardinality)",
        );
    }
}

/// Generic closed-set-COMPLEMENT testkit — pins that
/// [`TaggedUnion::missing_kinds`] composes over
/// [`TaggedUnion::has`] under NEGATION across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) entry on
/// the single-slot side, that the returned `Vec` is the canonical
/// [`ClosedSet::ALL`]-ordered filter of `!has(k)`, that on the
/// populated diagonal `single_slot(k).missing_kinds()` equals
/// `ALL \ {k}` exactly (length `ALL.len() - 1`, canonical ordered,
/// `k` absent), AND that the partition law
/// `populated_kinds() ∪ missing_kinds() == ClosedSet::ALL` (with
/// the two sets disjoint) holds byte-identically.
///
/// Parent-axis substrate primitive for the tagged-union closed-set-
/// complement refinement — the peer of
/// [`crate::boundary::assert_slice_refinement_composition_laws`]'s
/// `missing_kinds` sub-arm on the slice-level presence-probe axis,
/// lifted here to the tagged-union parent-level presence-probe axis
/// (same shape, same composition operator under negation, second
/// instance in the workspace-wide closed-set-complement refinement
/// algebra).
///
/// The FOUR sub-assertions swept per populated slot:
///
/// 1. Per-kind membership under negation:
///    `parent.missing_kinds().contains(&k) == !parent.has(k)` for
///    every `k ∈ ClosedSet::ALL` — a regression that overrode
///    `missing_kinds` to skip a kind, drift the walk order from
///    canonical `ALL`, or return a superset containing populated
///    kinds surfaces at the specific kind's per-pair assertion.
/// 2. Canonical `ALL`-filter equality under negation:
///    `parent.missing_kinds() == ALL.iter().copied().filter(|k|
///    !parent.has(*k)).collect()` — a regression that returned
///    duplicates or drifted the walk order surfaces at the
///    post-loop equality assert.
/// 3. Single-slot diagonal: `single_slot(k).missing_kinds()`
///    equals `ALL` with `k` removed — length exactly `ALL.len() - 1`,
///    canonical order preserved. Pins the well-formed arm's
///    complement cardinality.
/// 4. Partition law: `populated_kinds() ∪ missing_kinds() ==
///    ClosedSet::ALL` byte-identically (concatenated then re-sorted
///    into canonical `ALL` order) AND the two sets are disjoint
///    (no kind appears in both). A regression on either side of the
///    partition (a kind that appears in NEITHER, or in BOTH) fails
///    HERE at the partition assert — the compound-lift's most-
///    load-bearing invariant.
///
/// Substrate primitive for future per-parent
/// `X_missing_kinds_matches_has` tests that would otherwise each
/// restate the same nested-`for populated in K::ALL { for probed
/// in K::ALL { … } }` sweep + canonical-order equality + single-
/// slot diagonal pin + partition-law composition — every one of
/// the four production `.variant()` parents on `ProcessSpec` binds
/// through this ONE primitive with a per-site `single_slot`
/// factory. A fifth sibling picks up the closed-set-complement
/// check through ONE call site — no re-authored `for k in K::ALL
/// { … }` sweep at the test surface, no re-authored `assert_eq!`
/// quad.
///
/// The `single_slot` closure stays per-site — reused verbatim from
/// the sibling primitives ([`assert_variant_round_trip`],
/// [`assert_find_agrees_with_has`],
/// [`assert_populated_kinds_matches_has`],
/// [`assert_populated_kind_count_matches_populated_kinds`],
/// [`assert_single_slot_key_matches_label`]).
///
/// The [`crate::lifetime::Lifetime`] site is DELIBERATELY excluded
/// through the `T: TaggedUnion` bound — same reasoning as the
/// sibling primitives.
#[track_caller]
pub fn assert_missing_kinds_matches_has<T, F>(single_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let missing = parent.missing_kinds();
        let populated_kinds = parent.populated_kinds();
        // Per-kind membership composition law under negation, AND the
        // XOR partition arm: every k ∈ ALL appears in exactly one of
        // (populated_kinds, missing_kinds).
        for probed in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            assert_eq!(
                missing.contains(&probed),
                !parent.has(probed),
                "TaggedUnion::missing_kinds().contains({probed:?}) drifted from !has({probed:?}) — populated={populated:?}",
            );
            // XOR partition law: k ∈ populated_kinds ⊕ k ∈ missing_kinds
            // — every closed-set entry lives on EXACTLY ONE side of the
            // partition (populated OR missing, never both, never neither).
            let in_populated = populated_kinds.contains(&probed);
            let in_missing = missing.contains(&probed);
            assert!(
                in_populated ^ in_missing,
                "partition law violated — {probed:?} appears in {} of (populated_kinds, missing_kinds), not exactly one (populated={populated:?})",
                (in_populated as u8) + (in_missing as u8),
            );
        }
        // Canonical ALL-filter equality under negation — pins dedup,
        // walk order, and membership consistency at ONE assert.
        let canonical: Vec<T::Kind> = <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| !parent.has(*k))
            .collect();
        assert_eq!(
            missing, canonical,
            "TaggedUnion::missing_kinds() must yield ClosedSet::ALL-ordered subsequence where !has is true (no duplicates, canonical order) — populated={populated:?}",
        );
        // Single-slot diagonal — a well-formed parent from single_slot
        // populates exactly the addressed slot, so the missing set is
        // `ALL \ {populated}` in canonical order (length ALL.len() - 1,
        // `populated` absent).
        let expected_missing: Vec<T::Kind> = <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| *k != populated)
            .collect();
        assert_eq!(
            missing, expected_missing,
            "TaggedUnion::missing_kinds() on single_slot({populated:?}) must return ClosedSet::ALL with {populated:?} removed",
        );
    }
}

/// Generic scalar-cardinality testkit for the closed-set-complement
/// axis — pins that [`TaggedUnion::missing_kind_count`] agrees with
/// [`TaggedUnion::missing_kinds`]`.len()` across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement AND that on the populated diagonal
/// `single_slot(k).missing_kind_count()` equals `ALL.len() - 1`
/// exactly (aligned with the single-slot arm's `missing_kinds()`
/// returning `ALL \ {k}`) AND that the scalar partition law
/// `populated_kind_count() + missing_kind_count() == ALL.len()`
/// holds byte-identically.
///
/// Parent-axis substrate primitive for the scalar-cardinality
/// refinement of the tagged-union closed-set-complement axis — the
/// scalar projection of [`assert_missing_kinds_matches_has`]'s
/// widened primitive. Together they close the three-refinement
/// composition contract that binds
/// [`TaggedUnion::missing_kind_count`] against
/// [`TaggedUnion::missing_kinds`] and against
/// [`TaggedUnion::populated_kind_count`]:
///
/// 1. **`count ↔ kinds.len()`**: `missing_kind_count() ==
///    missing_kinds().len()` — a regression that overrode
///    `missing_kind_count` to skip a kind (returning the populated
///    count instead), double-count a slot, or drift the walk from
///    `ClosedSet::ALL` surfaces at the substrate boundary here.
/// 2. **Single-slot diagonal**: `single_slot(k).missing_kind_count()
///    == ALL.len() - 1` — pins the well-formed arm's complement
///    cardinality against the empty (`ALL.len()`) and Ambiguous
///    (`< ALL.len() - 1`) arms.
/// 3. **Scalar partition law**: `populated_kind_count() +
///    missing_kind_count() == ALL.len()` — the scalar consequence
///    of the `(populated_kinds, missing_kinds)` partition law that
///    [`assert_missing_kinds_matches_has`] pins at the widened-
///    primitive layer. A regression on either scalar side (an
///    off-by-one on missing, a drift on populated) fails HERE at
///    the sum assertion.
///
/// Substrate primitive for future per-parent
/// `X_missing_kind_count_matches_missing_kinds_len` tests that
/// would otherwise each restate the same nested-`for k in K::ALL {
/// … }` sweep + composition-law equality + single-slot cardinality
/// pin + scalar partition — every one of the four production
/// `.variant()` parents on `ProcessSpec` binds through this ONE
/// primitive with a per-site `single_slot` factory. A fifth sibling
/// picks up the scalar-cardinality check through ONE call site.
///
/// The `single_slot` closure stays per-site — reused verbatim from
/// the sibling primitives ([`assert_variant_round_trip`],
/// [`assert_find_agrees_with_has`],
/// [`assert_populated_kinds_matches_has`],
/// [`assert_populated_kind_count_matches_populated_kinds`],
/// [`assert_missing_kinds_matches_has`],
/// [`assert_single_slot_key_matches_label`]).
///
/// The [`crate::lifetime::Lifetime`] site is DELIBERATELY excluded
/// through the `T: TaggedUnion` bound — same reasoning as the
/// sibling primitives.
#[track_caller]
pub fn assert_missing_kind_count_matches_missing_kinds<T, F>(single_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    let all_len = <T::Kind as tatara_closed_set::ClosedSet>::ALL.len();
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let count = parent.missing_kind_count();
        let missing_len = parent.missing_kinds().len();
        // Composition law: scalar cardinality projection agrees with
        // the widened primitive's `Vec::len()`.
        assert_eq!(
            count, missing_len,
            "TaggedUnion::missing_kind_count() drifted from missing_kinds().len() — populated={populated:?}",
        );
        // Single-slot diagonal — a well-formed parent from single_slot
        // populates exactly the addressed slot, so the missing count is
        // ALL.len() - 1.
        assert_eq!(
            count,
            all_len - 1,
            "TaggedUnion::missing_kind_count() on single_slot({populated:?}) must equal ALL.len() - 1 exactly (well-formed arm complement cardinality)",
        );
        // Scalar partition law: populated_kind_count + missing_kind_count == ALL.len().
        let populated_count = parent.populated_kind_count();
        assert_eq!(
            populated_count + count,
            all_len,
            "scalar partition law violated — populated_kind_count + missing_kind_count must equal ClosedSet::ALL.len() (populated={populated:?})",
        );
    }
}

/// Generic earliest-populated-kind testkit — pins that
/// [`TaggedUnion::first_populated_kind`] agrees with
/// [`TaggedUnion::populated_kinds`]`.first().copied()` across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement AND that on the populated diagonal
/// `single_slot(k).first_populated_kind()` equals `Some(k)` exactly.
///
/// Parent-axis substrate primitive for the earliest-element scalar
/// projection of the tagged-union closed-set-inversion axis — the
/// `Option<Kind>`-valued projection of
/// [`assert_populated_kinds_matches_has`]'s widened primitive. The
/// three sub-assertions swept per populated slot:
///
/// 1. **`first ↔ kinds.first().copied()`**: `first_populated_kind() ==
///    populated_kinds().first().copied()` — a regression that
///    overrode `first_populated_kind` to skip the earliest match (a
///    `.rev().find(...)` inlined by mistake), drop the short-circuit
///    (allocating a full `Vec` at the callsite), or drift the walk
///    from `ClosedSet::ALL` surfaces here.
/// 2. **Single-slot diagonal**: `single_slot(k).first_populated_kind()
///    == Some(k)` — the earliest populated slot on a well-formed
///    parent IS the sole populated slot.
/// 3. **Emptiness composition law**: `first_populated_kind().is_none()
///    == (populated_kind_count() == 0)` — the earliest-element
///    projection agrees with the scalar cardinality on the empty
///    boundary. (Trivially `false == false` on every single-slot
///    arrangement; the load-bearing case is the sibling
///    empty-parent probe outside this primitive.)
///
/// A fifth sibling picks up the earliest-populated check through ONE
/// call site — no re-authored `for k in K::ALL` sweep, no re-authored
/// `assert_eq!(single_slot(k).first_populated_kind(), Some(k))`.
///
/// Same `Lifetime` exclusion as the sibling primitives.
#[track_caller]
pub fn assert_first_populated_kind_matches_populated_kinds<T, F>(single_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let first = parent.first_populated_kind();
        let via_kinds = parent.populated_kinds().first().copied();
        // Composition law: earliest-element projection agrees with the
        // widened primitive's `Vec::first().copied()`.
        assert_eq!(
            first, via_kinds,
            "TaggedUnion::first_populated_kind() drifted from populated_kinds().first().copied() — populated={populated:?}",
        );
        // Single-slot diagonal — a well-formed parent from single_slot
        // populates exactly the addressed slot, so the earliest
        // populated slot IS that slot.
        assert_eq!(
            first,
            Some(populated),
            "TaggedUnion::first_populated_kind() on single_slot({populated:?}) must equal Some({populated:?}) exactly",
        );
        // Emptiness composition law on the well-formed diagonal —
        // exactly-one is a strictly non-empty populated set, so the
        // scalar cardinality and the earliest-element `is_some()`
        // agree.
        assert_eq!(
            first.is_some(),
            parent.populated_kind_count() > 0,
            "first_populated_kind().is_some() drifted from (populated_kind_count() > 0) — populated={populated:?}",
        );
    }
}

/// Generic earliest-missing-kind testkit — pins that
/// [`TaggedUnion::first_missing_kind`] agrees with
/// [`TaggedUnion::missing_kinds`]`.first().copied()` across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement AND that on the populated diagonal
/// `single_slot(k).first_missing_kind()` equals the earliest `ALL`
/// entry NOT equal to `k`.
///
/// Parent-axis substrate primitive for the earliest-element scalar
/// projection of the tagged-union closed-set-complement axis — the
/// `Option<Kind>`-valued projection of
/// [`assert_missing_kinds_matches_has`]'s widened primitive under a
/// negated `has` predicate. The three sub-assertions swept per
/// populated slot:
///
/// 1. **`first ↔ missing.first().copied()`**: `first_missing_kind()
///    == missing_kinds().first().copied()` — a regression that
///    overrode `first_missing_kind` to drop the negation (returning
///    the populated side instead) or drift the walk from
///    `ClosedSet::ALL` surfaces here.
/// 2. **Single-slot diagonal**: `single_slot(k).first_missing_kind()`
///    equals the earliest `ALL` entry not equal to `k` — a well-
///    formed parent's missing set is `ALL \ {k}` in canonical order,
///    so its earliest element is `ALL[0]` when `k != ALL[0]`, else
///    `ALL[1]`.
/// 3. **Emptiness composition law**: `first_missing_kind().is_some()
///    == (missing_kind_count() > 0)` — the earliest-missing
///    projection agrees with the scalar complement cardinality.
///    Non-trivial on the single-slot arm when `ALL.len() > 1`.
///
/// A fifth sibling picks up the earliest-missing check through ONE
/// call site.
///
/// Same `Lifetime` exclusion as the sibling primitives.
#[track_caller]
pub fn assert_first_missing_kind_matches_missing_kinds<T, F>(single_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let first = parent.first_missing_kind();
        let via_missing = parent.missing_kinds().first().copied();
        // Composition law: earliest-element projection agrees with the
        // widened primitive's `Vec::first().copied()`.
        assert_eq!(
            first, via_missing,
            "TaggedUnion::first_missing_kind() drifted from missing_kinds().first().copied() — populated={populated:?}",
        );
        // Single-slot diagonal — the missing set is ALL \ {populated}
        // in canonical order, so its earliest element is the earliest
        // ALL entry not equal to populated.
        let expected_first_missing = <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .find(|k| *k != populated);
        assert_eq!(
            first, expected_first_missing,
            "TaggedUnion::first_missing_kind() on single_slot({populated:?}) must equal earliest ClosedSet::ALL entry != {populated:?}",
        );
        // Emptiness composition law — the earliest-missing projection
        // agrees with the scalar complement cardinality's positivity.
        assert_eq!(
            first.is_some(),
            parent.missing_kind_count() > 0,
            "first_missing_kind().is_some() drifted from (missing_kind_count() > 0) — populated={populated:?}",
        );
    }
}

/// Generic latest-populated-kind testkit — pins that
/// [`TaggedUnion::last_populated_kind`] agrees with
/// [`TaggedUnion::populated_kinds`]`.last().copied()` across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement AND that on the populated diagonal
/// `single_slot(k).last_populated_kind()` equals `Some(k)` exactly.
///
/// Parent-axis substrate primitive for the latest-element scalar
/// projection of the tagged-union closed-set-inversion axis — the
/// `Option<Kind>`-valued REVERSED-walk peer of
/// [`assert_first_populated_kind_matches_populated_kinds`]'s
/// earliest-element projection. The three sub-assertions swept per
/// populated slot:
///
/// 1. **`last ↔ kinds.last().copied()`**: `last_populated_kind() ==
///    populated_kinds().last().copied()` — a regression that overrode
///    `last_populated_kind` to walk `ALL` forward (defeating the
///    time-reversal), drop the short-circuit, or drift the walk from
///    `ClosedSet::ALL` surfaces here.
/// 2. **Single-slot diagonal**: `single_slot(k).last_populated_kind()
///    == Some(k)` — the sole populated slot on a well-formed parent
///    IS both the earliest AND the latest populated slot (the
///    endpoint projections agree on the exactly-one arm).
/// 3. **Emptiness composition law**: `last_populated_kind().is_none()
///    == (populated_kind_count() == 0)` — the latest-element
///    projection agrees with the scalar cardinality on the empty
///    boundary. (Trivially `false == false` on every single-slot
///    arrangement; the load-bearing case is the sibling empty-parent
///    probe outside this primitive.)
///
/// A fifth sibling picks up the latest-populated check through ONE
/// call site — no re-authored reversed `for k in K::ALL.iter().rev()`
/// sweep at the test surface, no re-authored
/// `assert_eq!(single_slot(k).last_populated_kind(), Some(k))`.
///
/// Same `Lifetime` exclusion as the sibling primitives.
#[track_caller]
pub fn assert_last_populated_kind_matches_populated_kinds<T, F>(single_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let last = parent.last_populated_kind();
        let via_kinds = parent.populated_kinds().last().copied();
        // Composition law: latest-element projection agrees with the
        // widened primitive's `Vec::last().copied()`.
        assert_eq!(
            last, via_kinds,
            "TaggedUnion::last_populated_kind() drifted from populated_kinds().last().copied() — populated={populated:?}",
        );
        // Single-slot diagonal — the sole populated slot IS both the
        // earliest and the latest, so the endpoint projections
        // coincide.
        assert_eq!(
            last,
            Some(populated),
            "TaggedUnion::last_populated_kind() on single_slot({populated:?}) must equal Some({populated:?}) exactly",
        );
        // Emptiness composition law on the well-formed diagonal.
        assert_eq!(
            last.is_some(),
            parent.populated_kind_count() > 0,
            "last_populated_kind().is_some() drifted from (populated_kind_count() > 0) — populated={populated:?}",
        );
    }
}

/// Generic latest-missing-kind testkit — pins that
/// [`TaggedUnion::last_missing_kind`] agrees with
/// [`TaggedUnion::missing_kinds`]`.last().copied()` across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement AND that on the populated diagonal
/// `single_slot(k).last_missing_kind()` equals the latest `ALL` entry
/// NOT equal to `k`.
///
/// Parent-axis substrate primitive for the latest-element scalar
/// projection of the tagged-union closed-set-complement axis — the
/// `Option<Kind>`-valued REVERSED-walk peer of
/// [`assert_first_missing_kind_matches_missing_kinds`]'s
/// earliest-element projection under a negated `has` predicate. The
/// three sub-assertions swept per populated slot:
///
/// 1. **`last ↔ missing.last().copied()`**: `last_missing_kind() ==
///    missing_kinds().last().copied()` — a regression that overrode
///    `last_missing_kind` to walk `ALL` forward (defeating the
///    time-reversal), drop the negation (returning the populated
///    side's latest instead), or drift the walk from `ClosedSet::ALL`
///    surfaces here.
/// 2. **Single-slot diagonal**: `single_slot(k).last_missing_kind()`
///    equals the LATEST `ALL` entry not equal to `k` — a well-formed
///    parent's missing set is `ALL \ {k}` in canonical order, so its
///    latest element is `ALL[ALL.len()-1]` when `k != ALL[ALL.len()-1]`,
///    else `ALL[ALL.len()-2]`.
/// 3. **Emptiness composition law**: `last_missing_kind().is_some()
///    == (missing_kind_count() > 0)` — the latest-missing projection
///    agrees with the scalar complement cardinality. Non-trivial on
///    the single-slot arm when `ALL.len() > 1`.
///
/// A fifth sibling picks up the latest-missing check through ONE call
/// site.
///
/// Same `Lifetime` exclusion as the sibling primitives.
#[track_caller]
pub fn assert_last_missing_kind_matches_missing_kinds<T, F>(single_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let last = parent.last_missing_kind();
        let via_missing = parent.missing_kinds().last().copied();
        // Composition law: latest-element projection agrees with the
        // widened primitive's `Vec::last().copied()`.
        assert_eq!(
            last, via_missing,
            "TaggedUnion::last_missing_kind() drifted from missing_kinds().last().copied() — populated={populated:?}",
        );
        // Single-slot diagonal — the missing set is ALL \ {populated}
        // in canonical order, so its latest element is the latest ALL
        // entry not equal to populated.
        let expected_last_missing = <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .rev()
            .copied()
            .find(|k| *k != populated);
        assert_eq!(
            last, expected_last_missing,
            "TaggedUnion::last_missing_kind() on single_slot({populated:?}) must equal latest ClosedSet::ALL entry != {populated:?}",
        );
        // Emptiness composition law — the latest-missing projection
        // agrees with the scalar complement cardinality's positivity.
        assert_eq!(
            last.is_some(),
            parent.missing_kind_count() > 0,
            "last_missing_kind().is_some() drifted from (missing_kind_count() > 0) — populated={populated:?}",
        );
    }
}

/// Generic exactly-one-populated-kind testkit — pins that
/// [`TaggedUnion::unique_populated_kind`] returns `Some(k)` iff exactly
/// one slot is populated (and names that slot's kind), and `None` on
/// every empty / ambiguous parent, across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement.
///
/// Parent-axis substrate primitive for the exactly-one-hit scalar
/// projection of the tagged-union closed-set-inversion axis — the
/// `Option<Kind>`-valued exactly-one peer of
/// [`assert_first_populated_kind_matches_populated_kinds`] and
/// [`assert_last_populated_kind_matches_populated_kinds`]'s endpoint
/// projections. The four sub-assertions swept per populated slot:
///
/// 1. **`unique ↔ exactly-one on kinds`**: `unique_populated_kind() ==
///    Some(k)` iff `populated_kinds() == vec![k]` — a regression that
///    dropped the second-hit short-circuit (returning `Some(first)`
///    on a two-populated parent) fails on the sibling
///    two-populated pin above.
/// 2. **Single-slot diagonal**: `single_slot(k).unique_populated_kind()
///    == Some(k)` — the sole populated slot IS the unique populated
///    kind.
/// 3. **Cardinality composition law**:
///    `unique_populated_kind().is_some() == (populated_kind_count()
///    == 1)` — the exactly-one predicate agrees with the scalar
///    cardinality on every well-formed / empty / ambiguous arm.
/// 4. **Endpoint agreement on Some**: on the `Some` arm,
///    `unique_populated_kind() == first_populated_kind() ==
///    last_populated_kind()` — the three endpoint-projection
///    primitives coincide on the exactly-one arm and DIVERGE only on
///    the ambiguous arm.
///
/// A fifth sibling picks up the exactly-one-populated check through
/// ONE call site — no re-authored `count == 1` composition at the
/// test surface, no re-authored `single_slot(k).unique_populated_kind()
/// == Some(k)` diagonal pin, no re-authored endpoint-agreement
/// projection.
///
/// Same `Lifetime` exclusion as the sibling primitives.
#[track_caller]
pub fn assert_unique_populated_kind_matches_populated_kinds<T, F>(single_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let unique = parent.unique_populated_kind();
        // Composition law: exactly-one predicate on the widened primitive.
        let kinds = parent.populated_kinds();
        let expected = if kinds.len() == 1 {
            Some(kinds[0])
        } else {
            None
        };
        assert_eq!(
            unique, expected,
            "TaggedUnion::unique_populated_kind() drifted from (populated_kinds().len() == 1 ? Some(kinds[0]) : None) — populated={populated:?}",
        );
        // Single-slot diagonal — a well-formed parent from single_slot
        // populates exactly the addressed slot, so the unique populated
        // kind IS that slot.
        assert_eq!(
            unique,
            Some(populated),
            "TaggedUnion::unique_populated_kind() on single_slot({populated:?}) must equal Some({populated:?}) exactly",
        );
        // Cardinality composition law — exactly-one predicate agrees
        // with the scalar cardinality's equality-to-one.
        assert_eq!(
            unique.is_some(),
            parent.populated_kind_count() == 1,
            "unique_populated_kind().is_some() drifted from (populated_kind_count() == 1) — populated={populated:?}",
        );
        // Endpoint-agreement — on the Some arm the three endpoint
        // projections coincide.
        if unique.is_some() {
            assert_eq!(
                unique,
                parent.first_populated_kind(),
                "unique_populated_kind() must equal first_populated_kind() on the Some arm — populated={populated:?}",
            );
            assert_eq!(
                unique,
                parent.last_populated_kind(),
                "unique_populated_kind() must equal last_populated_kind() on the Some arm — populated={populated:?}",
            );
        }
    }
}

/// Generic exactly-one-missing-kind testkit — pins that
/// [`TaggedUnion::unique_missing_kind`] returns `Some(k)` iff exactly
/// one slot is missing (and names that slot's kind), and `None` on
/// every parent whose missing-set cardinality is not one, across
/// every [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-
/// slot arrangement.
///
/// Parent-axis substrate primitive for the exactly-one-hit scalar
/// projection of the tagged-union closed-set-COMPLEMENT axis under a
/// negated `has` predicate. The three sub-assertions swept per
/// populated slot (single-slot diagonal only — on any tagged union
/// with `ALL.len() > 2` the single-slot arrangement has ≥ 2 missing
/// slots, so the primitive returns `None`; the load-bearing `Some`
/// pins are the sibling near-saturation probes outside this
/// primitive):
///
/// 1. **`unique ↔ exactly-one on missing`**: `unique_missing_kind()
///    == Some(k)` iff `missing_kinds() == vec![k]` — a regression
///    that dropped the second-hit short-circuit (returning
///    `Some(first)` on a two-missing parent) fails here.
/// 2. **Cardinality composition law**:
///    `unique_missing_kind().is_some() == (missing_kind_count() ==
///    1)` — the exactly-one predicate agrees with the scalar
///    complement cardinality on every well-formed / empty / ambiguous
///    arm.
/// 3. **Endpoint agreement on Some**: on the `Some` arm,
///    `unique_missing_kind() == first_missing_kind() ==
///    last_missing_kind()` — the three endpoint-projection
///    primitives on the missing axis coincide when exactly one slot
///    is empty.
///
/// A fifth sibling picks up the exactly-one-missing check through
/// ONE call site.
///
/// Same `Lifetime` exclusion as the sibling primitives.
#[track_caller]
pub fn assert_unique_missing_kind_matches_missing_kinds<T, F>(single_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let unique = parent.unique_missing_kind();
        // Composition law: exactly-one predicate on the widened
        // primitive.
        let missing = parent.missing_kinds();
        let expected = if missing.len() == 1 {
            Some(missing[0])
        } else {
            None
        };
        assert_eq!(
            unique, expected,
            "TaggedUnion::unique_missing_kind() drifted from (missing_kinds().len() == 1 ? Some(missing[0]) : None) — populated={populated:?}",
        );
        // Cardinality composition law — exactly-one predicate agrees
        // with the scalar complement cardinality's equality-to-one.
        assert_eq!(
            unique.is_some(),
            parent.missing_kind_count() == 1,
            "unique_missing_kind().is_some() drifted from (missing_kind_count() == 1) — populated={populated:?}",
        );
        // Endpoint-agreement — on the Some arm the three endpoint
        // projections on the missing axis coincide.
        if unique.is_some() {
            assert_eq!(
                unique,
                parent.first_missing_kind(),
                "unique_missing_kind() must equal first_missing_kind() on the Some arm — populated={populated:?}",
            );
            assert_eq!(
                unique,
                parent.last_missing_kind(),
                "unique_missing_kind() must equal last_missing_kind() on the Some arm — populated={populated:?}",
            );
        }
    }
}

/// Generic zero-populated-cardinality Boolean testkit — pins that
/// [`TaggedUnion::is_empty`] agrees with the scalar cardinality
/// primitive's equality-to-zero across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement AND the empty-parent baseline.
///
/// Parent-axis substrate primitive for the Boolean cardinality-
/// endpoint scalar projection of the tagged-union closed-set-inversion
/// axis under a zero-arm equality — the `bool`-valued zero-endpoint
/// peer of [`assert_populated_kind_count_matches_populated_kinds`]'s
/// scalar cardinality projection. The three sub-assertions swept per
/// populated slot + the ONE baseline sub-assertion on the empty
/// parent:
///
/// 1. **Cardinality composition law**: `is_empty() ==
///    (populated_kind_count() == 0)` — the Boolean projection agrees
///    with the scalar cardinality's zero-arm equality on every empty /
///    well-formed / partial / saturated arm. Byte-identical to the
///    trait's default body, pinning it substrate-wide so a regression
///    that overrides `is_empty` to skip the sweep or return the wrong
///    Boolean fails here.
/// 2. **Widened-primitive agreement**: `is_empty() ==
///    populated_kinds().is_empty()` — the two zero-arm projections of
///    the populated cardinality (via `is_empty()` short-circuit walk
///    vs. via `populated_kinds()` Vec materialization then `.is_empty()`)
///    coincide byte-identically.
/// 3. **Single-slot diagonal**: `single_slot(k).is_empty() == false` —
///    a well-formed parent from `single_slot` populates exactly the
///    addressed slot, so it CANNOT be empty. Pins that the primitive
///    doesn't drift onto the populated side of the endpoint.
/// 4. **Empty-parent baseline** (swept once outside the per-`k` loop):
///    `T::empty(T::KIND_LIST).is_empty() == true` (via a constructed
///    all-`None` parent since [`TaggedUnionError::empty`] is on the
///    error carrier, not the parent factory — the parent-side empty
///    fixture is composed by the caller through `Default` on the
///    sibling scaffold). Pins the primitive's zero-arm — a regression
///    that inverted the negation surfaces here.
///
/// A fifth sibling tagged-union parent picks up the zero-cardinality-
/// Boolean check through ONE `impl TaggedUnion for X` block + ONE
/// per-site `single_slot_X` factory + ONE per-site `empty_X` factory,
/// plus ONE call site — no re-authored `is_empty` sweep at the test
/// surface.
///
/// Same `Lifetime` exclusion as the sibling primitives — see
/// [`assert_two_slots_ambiguous`].
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs. The
///   Boolean zero-endpoint projection binds through the SAME shape
///   the scalar cardinality binds through (a closed-set walk under
///   `Self::has`), differing only in the return-type collapse
///   (`bool` vs. `usize`) and the short-circuit gate (`!any` vs.
///   `count`).
/// - THEORY.md §VI.1 — generation over composition. A new
///   [`Self::Kind`] variant added to `ALL` reaches this primitive
///   mechanically through the `any` short-circuit at the trait's
///   default body.
#[track_caller]
pub fn assert_is_empty_matches_populated_kind_count<T, F, G>(single_slot: F, empty_parent: G)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
    G: Fn() -> T,
{
    // Empty-parent baseline — the SOLE arm where `is_empty()` returns
    // `true`. The caller supplies the empty-parent fixture (an all-
    // `None` construction on the sibling scaffold's field structure).
    let empty = empty_parent();
    assert!(
        empty.is_empty(),
        "TaggedUnion::is_empty() on empty_parent() must equal true",
    );
    assert_eq!(
        empty.is_empty(),
        empty.populated_kind_count() == 0,
        "empty_parent().is_empty() drifted from (populated_kind_count() == 0)",
    );
    assert_eq!(
        empty.is_empty(),
        empty.populated_kinds().is_empty(),
        "empty_parent().is_empty() drifted from populated_kinds().is_empty()",
    );

    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let is_empty = parent.is_empty();
        // Cardinality composition law — Boolean projection agrees with
        // the scalar cardinality's zero-arm equality.
        assert_eq!(
            is_empty,
            parent.populated_kind_count() == 0,
            "TaggedUnion::is_empty() drifted from (populated_kind_count() == 0) — populated={populated:?}",
        );
        // Widened-primitive agreement — the two zero-arm projections
        // of the populated cardinality coincide.
        assert_eq!(
            is_empty,
            parent.populated_kinds().is_empty(),
            "TaggedUnion::is_empty() drifted from populated_kinds().is_empty() — populated={populated:?}",
        );
        // Single-slot diagonal — a well-formed parent from single_slot
        // is NEVER empty.
        assert!(
            !is_empty,
            "TaggedUnion::is_empty() on single_slot({populated:?}) must equal false",
        );
    }
}

/// Generic zero-missing-cardinality Boolean testkit — pins that
/// [`TaggedUnion::is_saturated`] agrees with the scalar complement
/// cardinality primitive's equality-to-zero across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement AND the empty-parent baseline.
///
/// Parent-axis substrate primitive for the Boolean cardinality-
/// endpoint scalar projection of the tagged-union closed-set-COMPLEMENT
/// axis under a zero-arm equality — the `bool`-valued top-endpoint
/// peer of [`assert_missing_kind_count_matches_missing_kinds`]'s
/// scalar complement cardinality projection. The three sub-assertions
/// swept per populated slot + the ONE baseline sub-assertion on the
/// empty parent:
///
/// 1. **Cardinality composition law**: `is_saturated() ==
///    (missing_kind_count() == 0)` — the Boolean projection agrees
///    with the scalar complement cardinality's zero-arm equality on
///    every empty / well-formed / partial / saturated arm. Byte-
///    identical to the trait's default body, pinning it substrate-
///    wide so a regression that overrides `is_saturated` to skip the
///    sweep or return the wrong Boolean fails here.
/// 2. **Widened-primitive agreement**: `is_saturated() ==
///    missing_kinds().is_empty()` — the two zero-arm projections of
///    the missing cardinality (via `is_saturated()` short-circuit walk
///    vs. via `missing_kinds()` Vec materialization then
///    `.is_empty()`) coincide byte-identically.
/// 3. **Single-slot diagonal** (on `ALL.len() ≥ 2` closed sets):
///    `single_slot(k).is_saturated() == false` — a well-formed parent
///    from `single_slot` populates exactly one slot, leaving at least
///    one slot missing (`ALL.len() - 1 ≥ 1`), so it CANNOT be
///    saturated on any real-world tagged union in this workspace.
///    Pins that the primitive doesn't drift onto the missing-side
///    zero endpoint.
/// 4. **Empty-parent baseline** (swept once outside the per-`k` loop):
///    `empty_parent().is_saturated() == false` (empty has EVERY slot
///    missing, so `ALL.len() ≥ 1` missing, NEVER zero). Pins the
///    primitive's opposite-arm on the same fixture the empty-Boolean
///    peer pins its zero-arm.
///
/// A fifth sibling tagged-union parent picks up the zero-complement-
/// cardinality-Boolean check through ONE `impl TaggedUnion for X`
/// block + ONE per-site `single_slot_X` factory + ONE per-site
/// `empty_X` factory + ONE call site — no re-authored `is_saturated`
/// sweep at the test surface.
///
/// Same `Lifetime` exclusion as the sibling primitives.
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs. The
///   Boolean top-endpoint projection binds through the SAME shape
///   the scalar complement cardinality binds through (a closed-set
///   walk under `Self::has`), differing only in the return-type
///   collapse (`bool` vs. `usize`) and the short-circuit gate (`all`
///   vs. `count`).
/// - THEORY.md §VI.1 — generation over composition. A new
///   [`Self::Kind`] variant added to `ALL` reaches this primitive
///   mechanically through the `all` short-circuit at the trait's
///   default body.
#[track_caller]
pub fn assert_is_saturated_matches_missing_kind_count<T, F, G>(single_slot: F, empty_parent: G)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
    G: Fn() -> T,
{
    // Empty-parent baseline — the empty parent has EVERY slot missing,
    // so `is_saturated()` returns `false` (the opposite endpoint of
    // where `is_empty()` returns `true`).
    let empty = empty_parent();
    assert!(
        !empty.is_saturated(),
        "TaggedUnion::is_saturated() on empty_parent() must equal false — every slot is missing",
    );
    assert_eq!(
        empty.is_saturated(),
        empty.missing_kind_count() == 0,
        "empty_parent().is_saturated() drifted from (missing_kind_count() == 0)",
    );
    assert_eq!(
        empty.is_saturated(),
        empty.missing_kinds().is_empty(),
        "empty_parent().is_saturated() drifted from missing_kinds().is_empty()",
    );

    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let is_saturated = parent.is_saturated();
        // Cardinality composition law — Boolean projection agrees with
        // the scalar complement cardinality's zero-arm equality.
        assert_eq!(
            is_saturated,
            parent.missing_kind_count() == 0,
            "TaggedUnion::is_saturated() drifted from (missing_kind_count() == 0) — populated={populated:?}",
        );
        // Widened-primitive agreement — the two zero-arm projections
        // of the missing cardinality coincide.
        assert_eq!(
            is_saturated,
            parent.missing_kinds().is_empty(),
            "TaggedUnion::is_saturated() drifted from missing_kinds().is_empty() — populated={populated:?}",
        );
        // Single-slot diagonal (on any `ALL.len() ≥ 2` closed set) — a
        // well-formed parent leaves `ALL.len() - 1 ≥ 1` missing, so it
        // CANNOT be saturated. This holds for every production tagged
        // union in the workspace (all have `ALL.len() ≥ 2`).
        assert!(
            !is_saturated,
            "TaggedUnion::is_saturated() on single_slot({populated:?}) must equal false — ALL.len() >= 2",
        );
    }
}

/// Generic at-least-one-populated-cardinality Boolean testkit — pins
/// that [`TaggedUnion::has_any_populated_kind`] agrees with its
/// definitional complement [`TaggedUnion::is_empty`] AND with the
/// scalar cardinality primitive's strict-inequality-to-zero across
/// every [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-
/// slot arrangement AND the empty-parent baseline.
///
/// Parent-axis substrate primitive for the Boolean at-least-one
/// halfspace projection on the tagged-union closed-set-inversion axis
/// — the `bool`-valued definitional complement of
/// [`assert_is_empty_matches_populated_kind_count`]'s zero-endpoint
/// Boolean projection, and the SUBSET peer of the zero-arm Boolean on
/// the populated cardinality lattice. The four sub-assertions swept
/// per populated slot + the ONE baseline sub-assertion on the empty
/// parent:
///
/// 1. **Definitional complement law**: `has_any_populated_kind() ==
///    !is_empty()` — the SUBSET Boolean is the bit-flip of the
///    zero-endpoint Boolean on every empty / well-formed / partial /
///    saturated arm. Byte-identical to the trait's default body
///    (both walk `<Self::Kind as ClosedSet>::ALL.iter().any(has)`,
///    the endpoint arm negates the whole expression), pinning the
///    pair substrate-wide so a regression that overrides
///    `has_any_populated_kind` to skip the sweep or drift off the
///    complement law surfaces here.
/// 2. **Cardinality composition law**: `has_any_populated_kind() ==
///    (populated_kind_count() > 0)` — the ≥ 1 halfspace agrees with
///    the scalar cardinality's strict-inequality-to-zero on every arm.
/// 3. **Widened-primitive agreement**: `has_any_populated_kind() ==
///    !populated_kinds().is_empty()` — the two at-least-one
///    projections of the populated cardinality (via
///    `has_any_populated_kind()` short-circuit walk vs. via
///    `populated_kinds()` `Vec` materialization then `!is_empty()`)
///    coincide byte-identically.
/// 4. **Single-slot diagonal**: `single_slot(k).has_any_populated_kind()
///    == true` — a well-formed parent from `single_slot` populates
///    exactly one slot, so the ≥ 1 halfspace returns `true`. Pins
///    that the primitive doesn't drift off the well-formed arm.
/// 5. **Empty-parent baseline** (swept once outside the per-`k` loop):
///    `empty_parent().has_any_populated_kind() == false` (empty has
///    zero populated). Pins the primitive's opposite arm on the same
///    fixture the zero-endpoint peer pins its zero-arm — the only arm
///    where the ≥ 1 halfspace returns `false`.
///
/// A fifth sibling tagged-union parent picks up the at-least-one-
/// populated-cardinality-Boolean check through ONE `impl TaggedUnion
/// for X` block + ONE per-site `single_slot_X` factory + ONE per-site
/// `empty_X` factory + ONE call site — no re-authored
/// `has_any_populated_kind` sweep at the test surface.
///
/// Same `Lifetime` exclusion as the sibling primitives — see
/// [`assert_two_slots_ambiguous`].
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs. The
///   Boolean at-least-one halfspace projection binds through the SAME
///   shape [`assert_is_empty_matches_populated_kind_count`] binds
///   through (a closed-set `any` walk under `Self::has`), differing
///   only in the final negation the zero-endpoint applies — pinned as
///   the definitional-complement law at ONE substrate site inside the
///   testkit's per-arm sweep.
/// - THEORY.md §VI.1 — generation over composition. A new
///   [`Self::Kind`] variant added to `ALL` reaches this primitive
///   mechanically through the `any` short-circuit at the trait's
///   default body.
#[track_caller]
pub fn assert_has_any_populated_kind_matches_populated_kind_count<T, F, G>(
    single_slot: F,
    empty_parent: G,
) where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
    G: Fn() -> T,
{
    // Empty-parent baseline — the SOLE arm where
    // `has_any_populated_kind()` returns `false`. The definitional
    // complement law binds this to `is_empty() == true`.
    let empty = empty_parent();
    assert!(
        !empty.has_any_populated_kind(),
        "TaggedUnion::has_any_populated_kind() on empty_parent() must equal false",
    );
    assert_eq!(
        empty.has_any_populated_kind(),
        !empty.is_empty(),
        "empty_parent().has_any_populated_kind() drifted from !is_empty()",
    );
    assert_eq!(
        empty.has_any_populated_kind(),
        empty.populated_kind_count() > 0,
        "empty_parent().has_any_populated_kind() drifted from (populated_kind_count() > 0)",
    );
    assert_eq!(
        empty.has_any_populated_kind(),
        !empty.populated_kinds().is_empty(),
        "empty_parent().has_any_populated_kind() drifted from !populated_kinds().is_empty()",
    );

    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let has_any = parent.has_any_populated_kind();
        // Definitional complement law — SUBSET Boolean is the bit-flip
        // of the zero-endpoint Boolean.
        assert_eq!(
            has_any,
            !parent.is_empty(),
            "TaggedUnion::has_any_populated_kind() drifted from !is_empty() — populated={populated:?}",
        );
        // Cardinality composition law — ≥ 1 halfspace agrees with
        // scalar cardinality's strict-inequality-to-zero.
        assert_eq!(
            has_any,
            parent.populated_kind_count() > 0,
            "TaggedUnion::has_any_populated_kind() drifted from (populated_kind_count() > 0) — populated={populated:?}",
        );
        // Widened-primitive agreement — the two at-least-one projections
        // of the populated cardinality coincide.
        assert_eq!(
            has_any,
            !parent.populated_kinds().is_empty(),
            "TaggedUnion::has_any_populated_kind() drifted from !populated_kinds().is_empty() — populated={populated:?}",
        );
        // Single-slot diagonal — a well-formed parent from single_slot
        // is ALWAYS at least one populated.
        assert!(
            has_any,
            "TaggedUnion::has_any_populated_kind() on single_slot({populated:?}) must equal true",
        );
    }
}

/// Generic at-least-one-missing-cardinality Boolean testkit — pins
/// that [`TaggedUnion::has_any_missing_kind`] agrees with its
/// definitional complement [`TaggedUnion::is_saturated`] AND with the
/// scalar complement cardinality primitive's strict-inequality-to-zero
/// across every [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL)
/// single-slot arrangement AND the empty-parent baseline.
///
/// Parent-axis substrate primitive for the Boolean at-least-one
/// halfspace projection on the tagged-union closed-set-COMPLEMENT axis
/// — the `bool`-valued definitional complement of
/// [`assert_is_saturated_matches_missing_kind_count`]'s zero-endpoint
/// Boolean projection, and the SUBSET peer of the zero-arm Boolean on
/// the missing cardinality lattice. Byte-for-byte symmetrical with
/// [`assert_has_any_populated_kind_matches_populated_kind_count`]
/// under the (populated, missing) complement axis.
///
/// The four sub-assertions swept per populated slot + the ONE baseline
/// sub-assertion on the empty parent:
///
/// 1. **Definitional complement law**: `has_any_missing_kind() ==
///    !is_saturated()` — the SUBSET Boolean is the bit-flip of the
///    zero-endpoint Boolean on every arm. Byte-identical to the trait's
///    default body (via De Morgan: `any(|k| !has(k)) == !all(|k|
///    has(k))`), pinning the pair substrate-wide.
/// 2. **Cardinality composition law**: `has_any_missing_kind() ==
///    (missing_kind_count() > 0)` — the ≥ 1 halfspace agrees with the
///    scalar complement cardinality's strict-inequality-to-zero on
///    every arm.
/// 3. **Widened-primitive agreement**: `has_any_missing_kind() ==
///    !missing_kinds().is_empty()` — the two at-least-one projections
///    of the missing cardinality coincide byte-identically.
/// 4. **Single-slot diagonal** (on `ALL.len() ≥ 2` closed sets):
///    `single_slot(k).has_any_missing_kind() == true` — a well-formed
///    parent from `single_slot` populates exactly one slot, leaving at
///    least one slot missing (`ALL.len() - 1 ≥ 1`), so the ≥ 1 missing
///    halfspace returns `true` on every real-world tagged union in
///    this workspace.
/// 5. **Empty-parent baseline** (swept once outside the per-`k` loop):
///    `empty_parent().has_any_missing_kind() == true` (empty has EVERY
///    slot missing on any `N ≥ 1`, so ≥ 1 missing). Pins the
///    primitive's non-saturated arm on the same fixture the zero-
///    endpoint peer pins its opposite arm.
///
/// Same `Lifetime` exclusion as the sibling primitives.
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs. The
///   Boolean at-least-one halfspace projection on the missing axis
///   binds through the SAME shape
///   [`assert_is_saturated_matches_missing_kind_count`] binds through
///   (a closed-set walk under `Self::has`), pinned as the
///   definitional-complement law at ONE substrate site inside the
///   testkit's per-arm sweep — the trait's default body composes
///   `any(|k| !has(k))` which is De-Morgan-equivalent to
///   `!all(|k| has(k))`, the exact expression `is_saturated()`
///   negates.
/// - THEORY.md §VI.1 — generation over composition. A new
///   [`Self::Kind`] variant added to `ALL` reaches this primitive
///   mechanically through the `any` short-circuit at the trait's
///   default body.
#[track_caller]
pub fn assert_has_any_missing_kind_matches_missing_kind_count<T, F, G>(
    single_slot: F,
    empty_parent: G,
) where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
    G: Fn() -> T,
{
    // Empty-parent baseline — the empty parent has EVERY slot missing,
    // so `has_any_missing_kind()` returns `true` (the opposite endpoint
    // of where `is_saturated()` returns `true`).
    let empty = empty_parent();
    assert!(
        empty.has_any_missing_kind(),
        "TaggedUnion::has_any_missing_kind() on empty_parent() must equal true — every slot is missing",
    );
    assert_eq!(
        empty.has_any_missing_kind(),
        !empty.is_saturated(),
        "empty_parent().has_any_missing_kind() drifted from !is_saturated()",
    );
    assert_eq!(
        empty.has_any_missing_kind(),
        empty.missing_kind_count() > 0,
        "empty_parent().has_any_missing_kind() drifted from (missing_kind_count() > 0)",
    );
    assert_eq!(
        empty.has_any_missing_kind(),
        !empty.missing_kinds().is_empty(),
        "empty_parent().has_any_missing_kind() drifted from !missing_kinds().is_empty()",
    );

    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let has_any = parent.has_any_missing_kind();
        // Definitional complement law — SUBSET Boolean is the bit-flip
        // of the zero-endpoint Boolean.
        assert_eq!(
            has_any,
            !parent.is_saturated(),
            "TaggedUnion::has_any_missing_kind() drifted from !is_saturated() — populated={populated:?}",
        );
        // Cardinality composition law — ≥ 1 halfspace agrees with
        // scalar complement cardinality's strict-inequality-to-zero.
        assert_eq!(
            has_any,
            parent.missing_kind_count() > 0,
            "TaggedUnion::has_any_missing_kind() drifted from (missing_kind_count() > 0) — populated={populated:?}",
        );
        // Widened-primitive agreement — the two at-least-one projections
        // of the missing cardinality coincide.
        assert_eq!(
            has_any,
            !parent.missing_kinds().is_empty(),
            "TaggedUnion::has_any_missing_kind() drifted from !missing_kinds().is_empty() — populated={populated:?}",
        );
        // Single-slot diagonal (on any `ALL.len() ≥ 2` closed set) — a
        // well-formed parent leaves `ALL.len() - 1 ≥ 1` missing, so
        // ≥ 1 missing halfspace holds. Every production tagged union
        // in the workspace has `ALL.len() ≥ 2`.
        assert!(
            has_any,
            "TaggedUnion::has_any_missing_kind() on single_slot({populated:?}) must equal true — ALL.len() >= 2",
        );
    }
}

/// Generic one-populated-cardinality Boolean testkit — pins that
/// [`TaggedUnion::has_unique_populated_kind`] agrees with the scalar
/// cardinality primitive's equality-to-one across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement AND the empty-parent baseline.
///
/// Parent-axis substrate primitive for the Boolean cardinality-mid-
/// endpoint scalar projection of the tagged-union closed-set-inversion
/// axis under a one-arm equality — the `bool`-valued one-endpoint peer
/// of [`assert_populated_kind_count_matches_populated_kinds`]'s scalar
/// cardinality projection. Together with [`assert_is_empty_matches_populated_kind_count`]
/// and [`assert_is_saturated_matches_missing_kind_count`] this closes
/// the substrate's 2×2 Boolean-endpoint sweep on the tagged-union
/// parent axis. The three sub-assertions swept per populated slot +
/// the ONE baseline sub-assertion on the empty parent:
///
/// 1. **Cardinality composition law**: `has_unique_populated_kind() ==
///    (populated_kind_count() == 1)` — the Boolean projection agrees
///    with the scalar cardinality's one-arm equality on every empty /
///    well-formed / partial / saturated arm. Byte-identical to the
///    trait's default body composed with `unique_populated_kind`,
///    pinning it substrate-wide so a regression that overrides
///    `has_unique_populated_kind` to skip the sweep or return the
///    wrong Boolean fails here.
/// 2. **Unique-primitive agreement**: `has_unique_populated_kind() ==
///    unique_populated_kind().is_some()` — the trait's default body,
///    pinned explicitly so a regression on the `unique_*` primitive
///    or on the Boolean projection's `is_some` collapse surfaces at
///    ONE assertion.
/// 3. **Single-slot diagonal**: `single_slot(k).has_unique_populated_kind()
///    == true` — a well-formed parent from `single_slot` populates
///    exactly one slot, so the one-arm Boolean returns `true`. Pins
///    that the primitive doesn't drift off the well-formed arm.
/// 4. **Empty-parent baseline** (swept once outside the per-`k` loop):
///    `empty_parent().has_unique_populated_kind() == false` (zero
///    populated, not one). Pins the primitive's opposite-arm on the
///    same fixture the zero-endpoint peer pins its zero-arm.
///
/// A fifth sibling tagged-union parent picks up the one-cardinality-
/// Boolean check through ONE `impl TaggedUnion for X` block plus ONE
/// per-site `single_slot_X` factory plus ONE per-site `empty_X` factory
/// plus ONE call site — no re-authored `has_unique_populated_kind`
/// sweep at the test surface.
///
/// Same `Lifetime` exclusion as the sibling primitives — see
/// [`assert_two_slots_ambiguous`].
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs. The
///   Boolean one-endpoint projection binds through the SAME shape
///   the scalar cardinality binds through (a closed-set walk under
///   `Self::has` composed with a two-step short-circuit), differing
///   only in the return-type collapse (`bool` vs. `usize`) and the
///   equality gate (`is_some` vs. `== 1`).
/// - THEORY.md §VI.1 — generation over composition. A new
///   [`Self::Kind`] variant added to `ALL` reaches this primitive
///   mechanically through the `unique_populated_kind` two-step short-
///   circuit at the trait's default body.
#[track_caller]
pub fn assert_has_unique_populated_kind_matches_populated_kind_count<T, F, G>(
    single_slot: F,
    empty_parent: G,
) where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
    G: Fn() -> T,
{
    // Empty-parent baseline — the empty parent has ZERO populated
    // slots, so `has_unique_populated_kind()` returns `false` (the
    // opposite endpoint of where a single-slot parent returns `true`).
    let empty = empty_parent();
    assert!(
        !empty.has_unique_populated_kind(),
        "TaggedUnion::has_unique_populated_kind() on empty_parent() must equal false",
    );
    assert_eq!(
        empty.has_unique_populated_kind(),
        empty.populated_kind_count() == 1,
        "empty_parent().has_unique_populated_kind() drifted from (populated_kind_count() == 1)",
    );
    assert_eq!(
        empty.has_unique_populated_kind(),
        empty.unique_populated_kind().is_some(),
        "empty_parent().has_unique_populated_kind() drifted from unique_populated_kind().is_some()",
    );

    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let has_unique = parent.has_unique_populated_kind();
        // Cardinality composition law — Boolean projection agrees with
        // the scalar cardinality's one-arm equality.
        assert_eq!(
            has_unique,
            parent.populated_kind_count() == 1,
            "TaggedUnion::has_unique_populated_kind() drifted from (populated_kind_count() == 1) — populated={populated:?}",
        );
        // Unique-primitive agreement — the Boolean is the `is_some`
        // projection of the Option-valued unique primitive.
        assert_eq!(
            has_unique,
            parent.unique_populated_kind().is_some(),
            "TaggedUnion::has_unique_populated_kind() drifted from unique_populated_kind().is_some() — populated={populated:?}",
        );
        // Single-slot diagonal — a well-formed parent from single_slot
        // has exactly one populated slot, so the one-arm Boolean is
        // `true`.
        assert!(
            has_unique,
            "TaggedUnion::has_unique_populated_kind() on single_slot({populated:?}) must equal true",
        );
    }
}

/// Generic one-missing-cardinality Boolean testkit — pins that
/// [`TaggedUnion::has_unique_missing_kind`] agrees with the scalar
/// complement cardinality primitive's equality-to-one across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement AND the empty-parent baseline.
///
/// Parent-axis substrate primitive for the Boolean cardinality-mid-
/// endpoint scalar projection of the tagged-union closed-set-COMPLEMENT
/// axis under a one-arm equality — the `bool`-valued one-endpoint peer
/// of [`assert_missing_kind_count_matches_missing_kinds`]'s scalar
/// complement cardinality projection. Byte-for-byte symmetrical with
/// [`assert_has_unique_populated_kind_matches_populated_kind_count`]
/// under the (populated, missing) complement axis. The three
/// sub-assertions swept per populated slot + the baseline sub-assertion
/// on the empty parent:
///
/// 1. **Cardinality composition law**: `has_unique_missing_kind() ==
///    (missing_kind_count() == 1)` — the Boolean projection agrees
///    with the scalar complement cardinality's one-arm equality on
///    every empty / well-formed / partial / saturated arm.
/// 2. **Unique-primitive agreement**: `has_unique_missing_kind() ==
///    unique_missing_kind().is_some()` — the trait's default body,
///    pinned explicitly.
/// 3. **Single-slot diagonal on `ALL.len() ≥ 3` closed sets**:
///    `single_slot(k).has_unique_missing_kind() == false` — a well-
///    formed parent leaves `ALL.len() - 1 ≥ 2` missing on any
///    `ALL.len() ≥ 3` closed set, so the one-arm Boolean returns
///    `false`. On the degenerate `ALL.len() == 2` closed set (e.g.
///    `Lifetime`, which this testkit excludes through the `TaggedUnion`
///    bound) well-formed and one-missing coincide; on every
///    production tagged union in the workspace (`ALL.len() ≥ 3`) the
///    diagonal returns `false`.
/// 4. **Empty-parent baseline**: `empty_parent().has_unique_missing_kind()
///    == false` (empty has EVERY slot missing, `ALL.len() ≥ 2` on
///    every production union, so never exactly one).
///
/// A fifth sibling tagged-union parent picks up the one-complement-
/// cardinality-Boolean check through ONE `impl TaggedUnion for X`
/// block plus ONE per-site `single_slot_X` factory plus ONE per-site
/// `empty_X` factory plus ONE call site — no re-authored
/// `has_unique_missing_kind` sweep at the test surface.
///
/// Same `Lifetime` exclusion as the sibling primitives.
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
/// - THEORY.md §VI.1 — generation over composition.
#[track_caller]
pub fn assert_has_unique_missing_kind_matches_missing_kind_count<T, F, G>(
    single_slot: F,
    empty_parent: G,
) where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
    G: Fn() -> T,
{
    // Empty-parent baseline — the empty parent has ALL.len() missing
    // slots, so `has_unique_missing_kind()` returns `false` on any
    // ALL.len() >= 2 closed set (every production union).
    let empty = empty_parent();
    assert!(
        !empty.has_unique_missing_kind(),
        "TaggedUnion::has_unique_missing_kind() on empty_parent() must equal false — ALL.len() >= 2 missing",
    );
    assert_eq!(
        empty.has_unique_missing_kind(),
        empty.missing_kind_count() == 1,
        "empty_parent().has_unique_missing_kind() drifted from (missing_kind_count() == 1)",
    );
    assert_eq!(
        empty.has_unique_missing_kind(),
        empty.unique_missing_kind().is_some(),
        "empty_parent().has_unique_missing_kind() drifted from unique_missing_kind().is_some()",
    );

    let all_len = <T::Kind as tatara_closed_set::ClosedSet>::ALL.len();
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let has_unique = parent.has_unique_missing_kind();
        // Cardinality composition law — Boolean projection agrees with
        // the scalar complement cardinality's one-arm equality.
        assert_eq!(
            has_unique,
            parent.missing_kind_count() == 1,
            "TaggedUnion::has_unique_missing_kind() drifted from (missing_kind_count() == 1) — populated={populated:?}",
        );
        // Unique-primitive agreement — the Boolean is the `is_some`
        // projection of the Option-valued unique primitive.
        assert_eq!(
            has_unique,
            parent.unique_missing_kind().is_some(),
            "TaggedUnion::has_unique_missing_kind() drifted from unique_missing_kind().is_some() — populated={populated:?}",
        );
        // Single-slot diagonal — a well-formed parent has ALL.len() - 1
        // missing slots. On ALL.len() == 2 the diagonal returns `true`
        // (2 - 1 == 1); on ALL.len() >= 3 it returns `false`.
        let expected_diagonal = all_len == 2;
        assert_eq!(
            has_unique,
            expected_diagonal,
            "TaggedUnion::has_unique_missing_kind() on single_slot({populated:?}) must equal {expected_diagonal} (ALL.len() == {all_len} → missing == {})",
            all_len - 1,
        );
    }
}

/// Generic ≥2-populated-cardinality Boolean testkit — pins that
/// [`TaggedUnion::has_multiple_populated_kinds`] agrees with the
/// scalar cardinality primitive's `>= 2` inequality across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement, every off-diagonal two-slot pair, AND the empty-
/// parent baseline.
///
/// Parent-axis substrate primitive for the Boolean cardinality many-
/// arm scalar projection of the tagged-union closed-set-inversion
/// axis under a `>= 2` inequality — third arm of the {0, 1, ≥2}
/// cardinality trichotomy on the populated axis, byte-for-byte peer
/// of [`assert_is_empty_matches_populated_kind_count`] (zero-arm) and
/// [`assert_has_unique_populated_kind_matches_populated_kind_count`]
/// (one-arm). The primitives partition every tagged-union state — on
/// any parent EXACTLY ONE of `is_empty()`,
/// `has_unique_populated_kind()`, `has_multiple_populated_kinds()`
/// returns `true`, closing the trichotomy at the trait's default
/// bodies. The four sub-assertions swept per populated slot + the
/// baseline sub-assertions + the two-slot sweep:
///
/// 1. **Cardinality composition law**: `has_multiple_populated_kinds()
///    == (populated_kind_count() >= 2)` on every empty / well-formed
///    / two-slot / saturated arm.
/// 2. **Trichotomy partition law**: EXACTLY ONE of `is_empty()`,
///    `has_unique_populated_kind()`, `has_multiple_populated_kinds()`
///    returns `true` on every arm swept — pinned as
///    `usize::from(is_empty()) + usize::from(has_unique_populated_kind())
///    + usize::from(has_multiple_populated_kinds()) == 1`.
/// 3. **Empty-parent baseline**: `empty_parent().has_multiple_populated_kinds()
///    == false` (zero populated, not many).
/// 4. **Single-slot diagonal**:
///    `single_slot(k).has_multiple_populated_kinds() == false` on
///    every `k` in `ClosedSet::ALL` (one populated, not many).
/// 5. **Two-slot diagonal**: for every off-diagonal `(a, b)` pair,
///    `two_slot(a, b).has_multiple_populated_kinds() == true` (two
///    populated, definitively many).
///
/// A fifth sibling tagged-union parent picks up the many-cardinality
/// Boolean check through ONE `impl TaggedUnion for X` block plus ONE
/// per-site `single_slot_X` factory plus ONE per-site `two_slot_X`
/// factory plus ONE per-site `empty_X` factory plus ONE call site.
///
/// Same [`crate::lifetime::Lifetime`] exclusion as the sibling
/// primitives — `Lifetime` doesn't impl [`TaggedUnion`].
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
/// - THEORY.md §VI.1 — generation over composition.
#[track_caller]
pub fn assert_has_multiple_populated_kinds_matches_populated_kind_count<T, F, G, H>(
    single_slot: F,
    two_slot: G,
    empty_parent: H,
) where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
    G: Fn(T::Kind, T::Kind) -> T,
    H: Fn() -> T,
{
    // Empty-parent baseline — zero populated slots, so
    // `has_multiple_populated_kinds()` returns `false`.
    let empty = empty_parent();
    assert!(
        !empty.has_multiple_populated_kinds(),
        "TaggedUnion::has_multiple_populated_kinds() on empty_parent() must equal false",
    );
    assert_eq!(
        empty.has_multiple_populated_kinds(),
        empty.populated_kind_count() >= 2,
        "empty_parent().has_multiple_populated_kinds() drifted from (populated_kind_count() >= 2)",
    );
    // Trichotomy partition on the empty arm — is_empty is true, the
    // other two are false.
    assert_eq!(
        usize::from(empty.is_empty())
            + usize::from(empty.has_unique_populated_kind())
            + usize::from(empty.has_multiple_populated_kinds()),
        1,
        "empty_parent() must satisfy EXACTLY ONE of is_empty / has_unique_populated_kind / has_multiple_populated_kinds",
    );

    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let has_multiple = parent.has_multiple_populated_kinds();
        // Cardinality composition law.
        assert_eq!(
            has_multiple,
            parent.populated_kind_count() >= 2,
            "TaggedUnion::has_multiple_populated_kinds() drifted from (populated_kind_count() >= 2) — populated={populated:?}",
        );
        // Single-slot diagonal — one populated, not many.
        assert!(
            !has_multiple,
            "TaggedUnion::has_multiple_populated_kinds() on single_slot({populated:?}) must equal false",
        );
        // Trichotomy partition on the well-formed arm —
        // has_unique_populated_kind is true, the other two are false.
        assert_eq!(
            usize::from(parent.is_empty())
                + usize::from(parent.has_unique_populated_kind())
                + usize::from(has_multiple),
            1,
            "single_slot({populated:?}) must satisfy EXACTLY ONE of is_empty / has_unique_populated_kind / has_multiple_populated_kinds",
        );
    }

    // Two-slot sweep — every off-diagonal pair has ≥ 2 populated.
    for a in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        for b in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            if a == b {
                continue;
            }
            let parent = two_slot(a, b);
            let has_multiple = parent.has_multiple_populated_kinds();
            assert!(
                has_multiple,
                "TaggedUnion::has_multiple_populated_kinds() on two_slot({a:?}, {b:?}) must equal true",
            );
            assert_eq!(
                has_multiple,
                parent.populated_kind_count() >= 2,
                "TaggedUnion::has_multiple_populated_kinds() drifted from (populated_kind_count() >= 2) — pair=({a:?}, {b:?})",
            );
            // Trichotomy partition on the two-slot arm —
            // has_multiple_populated_kinds is true, the other two
            // are false.
            assert_eq!(
                usize::from(parent.is_empty())
                    + usize::from(parent.has_unique_populated_kind())
                    + usize::from(has_multiple),
                1,
                "two_slot({a:?}, {b:?}) must satisfy EXACTLY ONE of is_empty / has_unique_populated_kind / has_multiple_populated_kinds",
            );
        }
    }
}

/// Generic ≥2-missing-cardinality Boolean testkit — pins that
/// [`TaggedUnion::has_multiple_missing_kinds`] agrees with the scalar
/// complement cardinality primitive's `>= 2` inequality across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement, every off-diagonal two-slot pair, AND the empty-
/// parent baseline.
///
/// Byte-for-byte peer of
/// [`assert_has_multiple_populated_kinds_matches_populated_kind_count`]
/// under the (populated, missing) complement axis. Third arm of the
/// {0, 1, ≥2} cardinality trichotomy on the missing axis, closing the
/// natural partition alongside
/// [`assert_is_saturated_matches_missing_kind_count`] (zero-arm) and
/// [`assert_has_unique_missing_kind_matches_missing_kind_count`]
/// (one-arm). Same trichotomy partition law:
/// `is_saturated() + has_unique_missing_kind() +
/// has_multiple_missing_kinds() == 1` on every arm.
///
/// The single-slot diagonal expectation depends on `ALL.len()`:
///
/// - `ALL.len() == 2`: well-formed has 1 missing, so
///   `has_multiple_missing_kinds() == false` (production `Lifetime`
///   is excluded via the `TaggedUnion` bound anyway).
/// - `ALL.len() >= 3`: well-formed has `ALL.len() - 1 >= 2` missing,
///   so `has_multiple_missing_kinds() == true`.
///
/// The two-slot diagonal expectation similarly depends:
///
/// - `ALL.len() == 3`: two_slot has `3 - 2 == 1` missing → `false`.
/// - `ALL.len() >= 4`: two_slot has `ALL.len() - 2 >= 2` missing →
///   `true`.
///
/// A fifth sibling tagged-union parent picks up the many-complement-
/// cardinality Boolean check through ONE `impl TaggedUnion for X`
/// block plus ONE per-site `single_slot_X` factory plus ONE per-site
/// `two_slot_X` factory plus ONE per-site `empty_X` factory plus ONE
/// call site.
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
/// - THEORY.md §VI.1 — generation over composition.
#[track_caller]
pub fn assert_has_multiple_missing_kinds_matches_missing_kind_count<T, F, G, H>(
    single_slot: F,
    two_slot: G,
    empty_parent: H,
) where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
    G: Fn(T::Kind, T::Kind) -> T,
    H: Fn() -> T,
{
    let all_len = <T::Kind as tatara_closed_set::ClosedSet>::ALL.len();
    // Empty-parent baseline — ALL.len() missing slots, so
    // `has_multiple_missing_kinds()` returns `true` on any
    // ALL.len() >= 2 closed set.
    let empty = empty_parent();
    let empty_expected = all_len >= 2;
    assert_eq!(
        empty.has_multiple_missing_kinds(),
        empty_expected,
        "TaggedUnion::has_multiple_missing_kinds() on empty_parent() must equal {empty_expected} (ALL.len() == {all_len})",
    );
    assert_eq!(
        empty.has_multiple_missing_kinds(),
        empty.missing_kind_count() >= 2,
        "empty_parent().has_multiple_missing_kinds() drifted from (missing_kind_count() >= 2)",
    );
    // Trichotomy partition on the empty arm — has_multiple_missing_kinds
    // is true (ALL.len() >= 2), is_saturated + has_unique_missing_kind
    // are false.
    assert_eq!(
        usize::from(empty.is_saturated())
            + usize::from(empty.has_unique_missing_kind())
            + usize::from(empty.has_multiple_missing_kinds()),
        1,
        "empty_parent() must satisfy EXACTLY ONE of is_saturated / has_unique_missing_kind / has_multiple_missing_kinds",
    );

    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let has_multiple = parent.has_multiple_missing_kinds();
        // Cardinality composition law.
        assert_eq!(
            has_multiple,
            parent.missing_kind_count() >= 2,
            "TaggedUnion::has_multiple_missing_kinds() drifted from (missing_kind_count() >= 2) — populated={populated:?}",
        );
        // Single-slot diagonal — well-formed has ALL.len() - 1
        // missing. `>= 2` iff `ALL.len() >= 3`.
        let expected_diagonal = all_len >= 3;
        assert_eq!(
            has_multiple,
            expected_diagonal,
            "TaggedUnion::has_multiple_missing_kinds() on single_slot({populated:?}) must equal {expected_diagonal} (ALL.len() == {all_len} → missing == {})",
            all_len - 1,
        );
        // Trichotomy partition on the well-formed arm.
        assert_eq!(
            usize::from(parent.is_saturated())
                + usize::from(parent.has_unique_missing_kind())
                + usize::from(has_multiple),
            1,
            "single_slot({populated:?}) must satisfy EXACTLY ONE of is_saturated / has_unique_missing_kind / has_multiple_missing_kinds",
        );
    }

    // Two-slot sweep — every off-diagonal pair has ALL.len() - 2
    // missing.
    for a in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        for b in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            if a == b {
                continue;
            }
            let parent = two_slot(a, b);
            let has_multiple = parent.has_multiple_missing_kinds();
            let expected_two_slot = all_len >= 4;
            assert_eq!(
                has_multiple,
                expected_two_slot,
                "TaggedUnion::has_multiple_missing_kinds() on two_slot({a:?}, {b:?}) must equal {expected_two_slot} (ALL.len() == {all_len} → missing == {})",
                all_len - 2,
            );
            assert_eq!(
                has_multiple,
                parent.missing_kind_count() >= 2,
                "TaggedUnion::has_multiple_missing_kinds() drifted from (missing_kind_count() >= 2) — pair=({a:?}, {b:?})",
            );
            // Trichotomy partition on the two-slot arm.
            assert_eq!(
                usize::from(parent.is_saturated())
                    + usize::from(parent.has_unique_missing_kind())
                    + usize::from(has_multiple),
                1,
                "two_slot({a:?}, {b:?}) must satisfy EXACTLY ONE of is_saturated / has_unique_missing_kind / has_multiple_missing_kinds",
            );
        }
    }
}

/// Generic parent-state-middle-arm Boolean testkit — pins that
/// [`TaggedUnion::is_partially_populated`] agrees with the paired
/// scalar-cardinality strict-inequality composition
/// `(populated_kind_count() > 0 && missing_kind_count() > 0)` across
/// every [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-
/// slot arrangement, every off-diagonal two-slot pair, AND the empty-
/// parent baseline.
///
/// Parent-state-axis substrate primitive for the Boolean middle-arm
/// projection of the `{Empty | Partial | Saturated}` trichotomy —
/// orthogonal to the {0, 1, ≥2} cardinality trichotomies already
/// closed on the populated / missing axes. The four sub-assertions
/// swept per populated slot + the four baseline sub-assertions on the
/// empty parent + the four sub-assertions swept per off-diagonal pair
/// bind FOUR composition laws per arm:
///
/// 1. **Widened negation-of-both-endpoints composition law**:
///    `is_partially_populated() == !is_empty() && !is_saturated()` —
///    the natural composition that the trait's default body's fused
///    walk collapses into ONE closed-set traversal. Pinned so a
///    regression that overrides `is_partially_populated` to skip the
///    sweep or return the wrong Boolean fails here at the widened
///    negation.
/// 2. **Paired scalar-cardinality composition law**:
///    `is_partially_populated() == (populated_kind_count() > 0 &&
///    missing_kind_count() > 0)` — the Boolean projection agrees with
///    the paired scalar-cardinality strict-inequality composition on
///    every empty / well-formed / partial / saturated arm.
/// 3. **Single-axis open-interval composition law**:
///    `is_partially_populated() == (0 < populated_kind_count() &&
///    populated_kind_count() < ALL.len())` — the Boolean projection
///    agrees with the single-axis strict-inequality composition
///    (populated cardinality lies in the open interval `(0, ALL.len())`).
/// 4. **Parent-state trichotomy partition law**:
///    `usize::from(is_empty()) + usize::from(is_partially_populated()) + usize::from(is_saturated()) == 1`
///    — EXACTLY ONE of the three parent-state Boolean primitives
///    returns `true` on every arm. This is the genuinely new proof
///    this testkit adds: the natural parent-state trichotomy
///    partitions every tagged-union state coherently, and this law
///    lives at ONE substrate site inside the testkit's per-arm sweep,
///    pinned across every production tagged union.
///
/// The three arm expectations:
///
/// - **Empty-parent baseline** (swept once outside the per-`k` loop):
///   `empty_parent().is_partially_populated() == false` (zero
///   populated, so the negation `!is_empty()` fails). Pins the
///   primitive's opposite-arm on the same fixture the empty-Boolean
///   peer pins its zero-arm.
/// - **Single-slot diagonal** (on `ALL.len() ≥ 2` closed sets):
///   `single_slot(k).is_partially_populated() == true` — a well-formed
///   parent from `single_slot` populates exactly one slot (0 <
///   populated < N), so the middle arm returns `true`. Pins the
///   primitive doesn't drift onto either endpoint.
/// - **Two-slot sweep** (on `ALL.len() ≥ 3` closed sets, which every
///   production tagged union in the workspace satisfies):
///   `two_slot(a, b).is_partially_populated() == true` — an
///   off-diagonal pair populates exactly two slots (0 < 2 <= N-1 < N
///   for N ≥ 3), so the middle arm returns `true`. On `ALL.len() ==
///   2` (production `Lifetime` excluded via the `TaggedUnion` bound)
///   two_slot would be saturated (`false`), but no production tagged
///   union has `ALL.len() == 2`.
///
/// A fifth sibling tagged-union parent picks up the middle-arm-
/// Boolean check through ONE `impl TaggedUnion for X` block plus ONE
/// per-site `single_slot_X` factory plus ONE per-site `two_slot_X`
/// factory plus ONE per-site `empty_X` factory plus ONE call site —
/// no re-authored `is_partially_populated` sweep at the test surface.
///
/// Same `Lifetime` exclusion as the sibling primitives — see
/// [`assert_two_slots_ambiguous`].
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs. The
///   Boolean parent-state middle-arm projection binds through the
///   SAME shape the two endpoint primitives bind through (a closed-
///   set walk under `Self::has`), differing only in the fused
///   short-circuit gate (both flags flipped) versus the endpoint
///   primitives' single-flag `any` / `all` short-circuits. The
///   trichotomy partition law lives at ONE substrate site inside the
///   testkit's per-arm sweep — pinned across every production tagged
///   union at compile time via the trait's default body composition,
///   not per-parent.
/// - THEORY.md §VI.1 — generation over composition. A new
///   [`Self::Kind`] variant added to `ALL` reaches this primitive
///   mechanically through the fused walk at the trait's default body.
#[track_caller]
pub fn assert_is_partially_populated_matches_cardinality<T, F, G, H>(
    single_slot: F,
    two_slot: G,
    empty_parent: H,
) where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
    G: Fn(T::Kind, T::Kind) -> T,
    H: Fn() -> T,
{
    let all_len = <T::Kind as tatara_closed_set::ClosedSet>::ALL.len();

    // Empty-parent baseline — zero populated slots, so
    // `is_partially_populated()` returns `false` (the empty arm of the
    // parent-state trichotomy, not the partial arm).
    let empty = empty_parent();
    // Anchor the baseline factory on the genuine empty arm — a saturated
    // factory would also return `false` from `is_partially_populated()`
    // (both endpoints of the trichotomy sit on the `false` side of the
    // middle-arm), so this explicit `is_empty()` pin distinguishes the
    // empty arm from the saturated arm on the baseline.
    assert!(
        empty.is_empty(),
        "TaggedUnion::is_partially_populated() testkit: empty_parent() must satisfy is_empty() == true",
    );
    assert!(
        !empty.is_partially_populated(),
        "TaggedUnion::is_partially_populated() on empty_parent() must equal false",
    );
    // Widened negation-of-both-endpoints composition law on the empty
    // arm.
    assert_eq!(
        empty.is_partially_populated(),
        !empty.is_empty() && !empty.is_saturated(),
        "empty_parent().is_partially_populated() drifted from (!is_empty() && !is_saturated())",
    );
    // Paired scalar-cardinality composition law on the empty arm.
    assert_eq!(
        empty.is_partially_populated(),
        empty.populated_kind_count() > 0 && empty.missing_kind_count() > 0,
        "empty_parent().is_partially_populated() drifted from (populated_kind_count() > 0 && missing_kind_count() > 0)",
    );
    // Parent-state trichotomy partition law on the empty arm —
    // is_empty is true, the other two are false.
    assert_eq!(
        usize::from(empty.is_empty())
            + usize::from(empty.is_partially_populated())
            + usize::from(empty.is_saturated()),
        1,
        "empty_parent() must satisfy EXACTLY ONE of is_empty / is_partially_populated / is_saturated",
    );

    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let is_partial = parent.is_partially_populated();
        // Widened negation-of-both-endpoints composition law.
        assert_eq!(
            is_partial,
            !parent.is_empty() && !parent.is_saturated(),
            "TaggedUnion::is_partially_populated() drifted from (!is_empty() && !is_saturated()) — populated={populated:?}",
        );
        // Paired scalar-cardinality composition law.
        assert_eq!(
            is_partial,
            parent.populated_kind_count() > 0 && parent.missing_kind_count() > 0,
            "TaggedUnion::is_partially_populated() drifted from (populated_kind_count() > 0 && missing_kind_count() > 0) — populated={populated:?}",
        );
        // Single-axis open-interval composition law.
        assert_eq!(
            is_partial,
            0 < parent.populated_kind_count() && parent.populated_kind_count() < all_len,
            "TaggedUnion::is_partially_populated() drifted from (0 < populated_kind_count() < ALL.len()) — populated={populated:?}",
        );
        // Single-slot diagonal (on any `ALL.len() ≥ 2` closed set) —
        // well-formed has 1 populated + `ALL.len() - 1 ≥ 1` missing,
        // so the middle arm returns `true`. This holds for every
        // production tagged union in the workspace (all have
        // `ALL.len() ≥ 2`).
        assert!(
            is_partial,
            "TaggedUnion::is_partially_populated() on single_slot({populated:?}) must equal true — ALL.len() >= 2",
        );
        // Parent-state trichotomy partition on the well-formed arm.
        assert_eq!(
            usize::from(parent.is_empty())
                + usize::from(is_partial)
                + usize::from(parent.is_saturated()),
            1,
            "single_slot({populated:?}) must satisfy EXACTLY ONE of is_empty / is_partially_populated / is_saturated",
        );
    }

    // Two-slot sweep — every off-diagonal pair has 2 populated
    // + `ALL.len() - 2` missing.
    for a in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        for b in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            if a == b {
                continue;
            }
            let parent = two_slot(a, b);
            let is_partial = parent.is_partially_populated();
            // Widened negation-of-both-endpoints composition law.
            assert_eq!(
                is_partial,
                !parent.is_empty() && !parent.is_saturated(),
                "TaggedUnion::is_partially_populated() drifted from (!is_empty() && !is_saturated()) — pair=({a:?}, {b:?})",
            );
            // Paired scalar-cardinality composition law.
            assert_eq!(
                is_partial,
                parent.populated_kind_count() > 0 && parent.missing_kind_count() > 0,
                "TaggedUnion::is_partially_populated() drifted from (populated_kind_count() > 0 && missing_kind_count() > 0) — pair=({a:?}, {b:?})",
            );
            // Two-slot diagonal — on `ALL.len() >= 3` the two-slot
            // parent has 2 populated + `ALL.len() - 2 >= 1` missing,
            // so the middle arm returns `true`. On `ALL.len() == 2`
            // (excluded via the `TaggedUnion` bound anyway) two_slot
            // would be saturated (`false`).
            let expected_two_slot = all_len >= 3;
            assert_eq!(
                is_partial,
                expected_two_slot,
                "TaggedUnion::is_partially_populated() on two_slot({a:?}, {b:?}) must equal {expected_two_slot} (ALL.len() == {all_len})",
            );
            // Parent-state trichotomy partition on the two-slot arm.
            assert_eq!(
                usize::from(parent.is_empty())
                    + usize::from(is_partial)
                    + usize::from(parent.is_saturated()),
                1,
                "two_slot({a:?}, {b:?}) must satisfy EXACTLY ONE of is_empty / is_partially_populated / is_saturated",
            );
        }
    }
}

/// Generic kind-scoped strict-refinement testkit — pins that
/// [`TaggedUnion::has_only`] agrees with the widened composition
/// `unique_populated_kind() == Some(kind)` across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) `× ALL`
/// single-slot (populated, probed) pair, every off-diagonal two-slot
/// pair `× ALL`, AND the empty-parent baseline `× ALL`.
///
/// Kind-scoped-strict-refinement-axis substrate primitive for the
/// argument-taking uniqueness peer of [`TaggedUnion::has`] — the
/// EQUAL predicate to `has`'s SUBSET predicate. FIVE composition laws
/// per arm are pinned per (populated / pair / empty × probed) sub-
/// assertion:
///
/// 1. **Widened uniqueness composition law**:
///    `has_only(kind) == (unique_populated_kind() == Some(kind))` —
///    the canonical composition that the trait's default body's fused
///    walk collapses into ONE short-circuit closed-set traversal.
///    Pinned so a regression that overrides `has_only` to skip the
///    sweep, drop the "no other populated" check, or return the wrong
///    Boolean fails here at the widened uniqueness composition.
/// 2. **Cardinality-refinement composition law**:
///    `has_only(kind) == (has(kind) && has_unique_populated_kind())`
///    — the paired-endpoint composition binding the strict refinement
///    to the arg-less uniqueness predicate. Pinned so a regression
///    that drops the "exactly one populated" check (returning `true`
///    on a multi-populated parent whose SET of populated kinds
///    contains `kind`) is caught here.
/// 3. **Kind-scoped implication law**:
///    `has_only(kind) → has(kind)` — every arm where `has_only`
///    returns `true` must satisfy `has(kind) == true` (the SUBSET
///    predicate must accept every parent the EQUAL predicate
///    accepts). Pinned so a regression that returns `true` on an
///    empty parent or a parent that populates a DIFFERENT kind is
///    caught here.
/// 4. **Kind-domain exhaustivity law**:
///    `<Kind as ClosedSet>::ALL.iter().filter(|k|
///    parent.has_only(*k)).count() ≤ 1` on every arm — a parent
///    satisfies `has_only(k)` for AT MOST one `k`, since two distinct
///    kinds cannot both be the sole populated slot. On the well-
///    formed arm the count is exactly 1 (the addressed kind); on the
///    empty AND multi-populated arms the count is 0. This kind-domain
///    exhaustivity law binds the argument-scoped projection to the
///    arg-less uniqueness predicate at ONE substrate site.
/// 5. **Well-formed diagonal law**:
///    `single_slot(k).has_only(k) == true` on every `k ∈
///    ClosedSet::ALL` — the single-slot factory constructs a well-
///    formed parent, so every `has_only(k)` on the diagonal is
///    `true`. Pinned so a regression that returns `false` on the
///    well-formed arm (e.g. a typo `!self.has(k)` in the trait
///    default) is caught here.
///
/// The three arm expectations:
///
/// - **Empty-parent baseline** (swept `× ALL` outside the per-slot
///   loop): `empty_parent().has_only(k) == false` for every `k` — no
///   populated slot, so no kind is the sole populated kind.
/// - **Single-slot sweep** (swept on `ClosedSet::ALL × ALL`):
///   `single_slot(populated).has_only(kind) == (populated == kind)`
///   — the well-formed truth table.
/// - **Two-slot sweep** (swept on the off-diagonal `× ALL`):
///   `two_slot(a, b).has_only(k) == false` for every `k` — multi-
///   populated parents satisfy `has_only(k)` for NO kind.
///
/// A fifth sibling tagged-union parent picks up the kind-scoped-
/// strict-refinement check through ONE `impl TaggedUnion for X`
/// block plus ONE per-site `single_slot_X` factory plus ONE per-site
/// `two_slot_X` factory plus ONE per-site `empty_X` factory plus ONE
/// call site — no re-authored `has_only` sweep at the test surface.
///
/// Same [`crate::lifetime::Lifetime`] exclusion as the sibling
/// primitives — the `T: TaggedUnion` bound doesn't reach it.
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs. The
///   kind-scoped strict-refinement projection binds through the SAME
///   shape the arg-less uniqueness peer binds through (a closed-set
///   walk under `Self::has`), differing only in the argument-scoped
///   short-circuit gate (first populated slot mismatched → `false`).
///   The kind-domain exhaustivity law
///   `count k where has_only(k) ≤ 1` lives at ONE substrate site
///   inside the testkit's per-arm sweep — pinned across every
///   production tagged union at compile time via the trait's default
///   body composition, not per-parent.
/// - THEORY.md §VI.1 — generation over composition. A new
///   [`Self::Kind`] variant added to `ALL` reaches this primitive
///   mechanically through the fused walk at the trait's default body.
#[track_caller]
pub fn assert_has_only_matches_unique_populated_kind<T, F, G, H>(
    single_slot: F,
    two_slot: G,
    empty_parent: H,
) where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
    G: Fn(T::Kind, T::Kind) -> T,
    H: Fn() -> T,
{
    // Empty-parent baseline — every `has_only(k)` returns `false`
    // because no slot is populated.
    let empty = empty_parent();
    assert!(
        empty.is_empty(),
        "TaggedUnion::has_only() testkit: empty_parent() must satisfy is_empty() == true",
    );
    for k in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let via_has_only = empty.has_only(k);
        assert!(
            !via_has_only,
            "empty_parent().has_only({k:?}) must equal false",
        );
        // Widened uniqueness composition law on the empty arm.
        assert_eq!(
            via_has_only,
            empty.unique_populated_kind() == Some(k),
            "empty_parent().has_only({k:?}) drifted from (unique_populated_kind() == Some({k:?}))",
        );
        // Cardinality-refinement composition law on the empty arm.
        assert_eq!(
            via_has_only,
            empty.has(k) && empty.has_unique_populated_kind(),
            "empty_parent().has_only({k:?}) drifted from (has({k:?}) && has_unique_populated_kind())",
        );
    }
    // Kind-domain exhaustivity on the empty arm — no kind is the sole
    // populated kind, so the count is 0.
    let empty_count = <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
        .filter(|k| empty.has_only(*k))
        .count();
    assert_eq!(
        empty_count, 0,
        "empty_parent(): exactly 0 kinds must satisfy has_only, got {empty_count}",
    );

    // Single-slot sweep — the well-formed truth table across
    // `ClosedSet::ALL × ALL`.
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        for probed in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let expected = probed == populated;
            let via_has_only = parent.has_only(probed);
            // Truth table on the well-formed diagonal — `true` iff the
            // probed kind equals the populated kind.
            assert_eq!(
                via_has_only, expected,
                "single_slot({populated:?}).has_only({probed:?}) must equal {expected}",
            );
            // Widened uniqueness composition law.
            assert_eq!(
                via_has_only,
                parent.unique_populated_kind() == Some(probed),
                "single_slot({populated:?}).has_only({probed:?}) drifted from (unique_populated_kind() == Some({probed:?}))",
            );
            // Cardinality-refinement composition law.
            assert_eq!(
                via_has_only,
                parent.has(probed) && parent.has_unique_populated_kind(),
                "single_slot({populated:?}).has_only({probed:?}) drifted from (has({probed:?}) && has_unique_populated_kind())",
            );
            // Kind-scoped implication law — has_only implies has.
            if via_has_only {
                assert!(
                    parent.has(probed),
                    "single_slot({populated:?}).has_only({probed:?}) == true but has({probed:?}) == false",
                );
            }
        }
        // Kind-domain exhaustivity on the well-formed arm — exactly 1
        // kind (the populated one) satisfies has_only.
        let well_formed_count = <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| parent.has_only(*k))
            .count();
        assert_eq!(
            well_formed_count, 1,
            "single_slot({populated:?}): exactly 1 kind must satisfy has_only, got {well_formed_count}",
        );
    }

    // Two-slot sweep — every off-diagonal pair populates two slots, so
    // has_only(k) == false for every k, and no kind satisfies has_only
    // on the multi-populated arm.
    for a in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        for b in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            if a == b {
                continue;
            }
            let parent = two_slot(a, b);
            for k in <T::Kind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .copied()
            {
                let via_has_only = parent.has_only(k);
                assert!(
                    !via_has_only,
                    "two_slot({a:?}, {b:?}).has_only({k:?}) must equal false",
                );
                // Widened uniqueness composition law on the multi-
                // populated arm.
                assert_eq!(
                    via_has_only,
                    parent.unique_populated_kind() == Some(k),
                    "two_slot({a:?}, {b:?}).has_only({k:?}) drifted from (unique_populated_kind() == Some({k:?}))",
                );
                // Cardinality-refinement composition law.
                assert_eq!(
                    via_has_only,
                    parent.has(k) && parent.has_unique_populated_kind(),
                    "two_slot({a:?}, {b:?}).has_only({k:?}) drifted from (has({k:?}) && has_unique_populated_kind())",
                );
            }
            // Kind-domain exhaustivity on the multi-populated arm — no
            // kind is the sole populated kind.
            let multi_count = <T::Kind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .copied()
                .filter(|k| parent.has_only(*k))
                .count();
            assert_eq!(
                multi_count, 0,
                "two_slot({a:?}, {b:?}): exactly 0 kinds must satisfy has_only, got {multi_count}",
            );
        }
    }
}

/// Generic kind-scoped strict-refinement testkit on the MISSING axis —
/// pins that [`TaggedUnion::lacks_only`] agrees with
/// [`TaggedUnion::unique_missing_kind`]'s
/// argument-scoped projection, [`TaggedUnion::has`]'s negated
/// cardinality-refinement, AND the kind-scoped implication
/// `lacks_only(kind) → !has(kind)` across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement, every off-diagonal two-slot pair, AND the empty-parent
/// baseline.
///
/// Closed-set-complement mirror of
/// [`assert_has_only_matches_unique_populated_kind`] under the
/// (populated, missing) duality — where the populated-axis primitive
/// binds `has_only(kind)` to `unique_populated_kind()`, this primitive
/// binds `lacks_only(kind)` to `unique_missing_kind()` through the
/// same shape (a closed-set walk under `Self::has`, differing only in
/// the negation of the presence probe). The four sub-assertions swept
/// per single-slot arrangement + the two-slot sweep + the empty-parent
/// baseline:
///
/// 1. **Widened uniqueness composition law**:
///    `lacks_only(kind) == (unique_missing_kind() == Some(kind))` on
///    every arm — the fused walk's argument-scoped projection agrees
///    with the arg-less unique-missing primitive's `Option::eq` on
///    `Some(kind)`. Byte-for-byte peer of
///    [`assert_has_only_matches_unique_populated_kind`]'s widened
///    uniqueness law under complement.
/// 2. **Cardinality-refinement composition law**:
///    `lacks_only(kind) == (!has(kind) && has_unique_missing_kind())`
///    on every arm — the fused walk agrees with the two-step
///    composition of the negated presence probe and the arg-less
///    missing-cardinality Boolean. Closed-set-complement mirror of
///    the populated-axis cardinality-refinement law.
/// 3. **Kind-scoped implication law**:
///    `lacks_only(kind) → !has(kind)` on every arm — if `kind` is the
///    sole missing slot then `kind` cannot be populated. Complement
///    mirror of the `has_only(kind) → has(kind)` implication that
///    binds [`TaggedUnion::has_only`] to [`TaggedUnion::has`] on the
///    strict-refinement axis; here the implication binds `lacks_only`
///    to `!has` on the closed-set-complement axis.
/// 4. **Kind-domain exhaustivity law**: `<T::Kind as ClosedSet>::ALL
///    .iter().filter(|k| parent.lacks_only(*k)).count() ≤ 1` on every
///    arm — a parent satisfies `lacks_only(k)` for AT MOST one `k`,
///    since two distinct kinds cannot both be the sole missing slot.
///    On the near-saturation arm (exactly 1 missing) the count is 1;
///    on every other arm the count is 0. Closed-set-complement mirror
///    of the populated-axis exhaustivity law under complement.
/// 5. **Missing-diagonal well-formed law**:
///    `unique_missing_kind()` is the source of truth for which kind
///    (if any) is uniquely missing on each arm — the testkit reads it
///    directly and asserts `lacks_only(k) == (unique_missing_kind()
///    == Some(k))` for every `k`, so the testkit doesn't hard-code
///    `ALL.len()`-dependent arm expectations (an empty parent on
///    `ALL.len() == 1` is uniquely missing that one kind, whereas on
///    `ALL.len() >= 2` no kind is uniquely missing; a single-slot
///    parent on `ALL.len() == 2` has one missing kind, whereas on
///    `ALL.len() >= 3` it has ≥ 2 missing; a two-slot parent on
///    `ALL.len() == 3` has one missing kind, whereas on `ALL.len()
///    >= 4` it has ≥ 2 missing). The composition-law shape binds
///    every `ALL.len()` regime through the same substrate site.
///
/// The three arm expectations:
///
/// - **Empty-parent baseline** (swept `× ALL` outside the per-slot
///   loop): on any `ALL.len() >= 2` closed set every kind is missing,
///   so `lacks_only(k) == false` for every `k` — no kind is the sole
///   missing kind. Every production parent is `ALL.len() >= 3`.
/// - **Single-slot sweep** (swept on `ClosedSet::ALL × ALL`): the
///   composition-law shape reads `unique_missing_kind()` directly, so
///   the testkit binds every `ALL.len()` regime without a hard-coded
///   arm expectation. Assertion messages carry the (`populated`,
///   `probed`) pair verbatim.
/// - **Two-slot sweep** (swept on the off-diagonal `× ALL`): on
///   `ALL.len() == 3` every off-diagonal pair leaves exactly 1 slot
///   missing (the third kind) — the SOLE `ALL.len()` regime where
///   `lacks_only(third) == true` on the two-slot arm. On `ALL.len()
///   >= 4` the two-slot arm has ≥ 2 missing, so `lacks_only(k) ==
///   false` for every `k`. The composition-law shape binds every
///   regime.
///
/// A fifth sibling tagged-union parent picks up the kind-scoped-
/// strict-refinement check on the missing axis through ONE `impl
/// TaggedUnion for X` block plus ONE per-site `single_slot_X` factory
/// plus ONE per-site `two_slot_X` factory plus ONE per-site `empty_X`
/// factory plus ONE call site — no re-authored `lacks_only` sweep at
/// the test surface.
///
/// Same [`crate::lifetime::Lifetime`] exclusion as the sibling
/// primitives — the `T: TaggedUnion` bound doesn't reach it.
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs. The
///   kind-scoped strict-refinement projection on the MISSING axis
///   binds through the SAME shape the populated-axis peer binds
///   through (a closed-set walk under `Self::has`), differing only in
///   the negation of the presence probe. The composition laws
///   (widened uniqueness on the missing side, cardinality-refinement
///   under complement, kind-scoped implication under complement,
///   kind-domain exhaustivity on the missing side) live at ONE
///   substrate site inside the testkit's per-arm sweep — pinned
///   across every production tagged union at compile time via the
///   trait's default body composition, not per-parent.
/// - THEORY.md §VI.1 — generation over composition. A new
///   [`Self::Kind`] variant added to `ALL` reaches this primitive
///   mechanically through the fused walk at the trait's default body.
#[track_caller]
pub fn assert_lacks_only_matches_unique_missing_kind<T, F, G, H>(
    single_slot: F,
    two_slot: G,
    empty_parent: H,
) where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
    G: Fn(T::Kind, T::Kind) -> T,
    H: Fn() -> T,
{
    // Empty-parent baseline — every `lacks_only(k)` returns `false` on
    // any `ALL.len() >= 2` closed set (every production union) because
    // every kind is missing so no kind is uniquely missing. The
    // composition-law shape below reads `unique_missing_kind()`
    // directly, so the testkit binds every `ALL.len()` regime without
    // a hard-coded arm expectation.
    let empty = empty_parent();
    assert!(
        empty.is_empty(),
        "TaggedUnion::lacks_only() testkit: empty_parent() must satisfy is_empty() == true",
    );
    for k in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let via_lacks_only = empty.lacks_only(k);
        // Widened uniqueness composition law on the empty arm.
        assert_eq!(
            via_lacks_only,
            empty.unique_missing_kind() == Some(k),
            "empty_parent().lacks_only({k:?}) drifted from (unique_missing_kind() == Some({k:?}))",
        );
        // Cardinality-refinement composition law on the empty arm
        // under complement.
        assert_eq!(
            via_lacks_only,
            !empty.has(k) && empty.has_unique_missing_kind(),
            "empty_parent().lacks_only({k:?}) drifted from (!has({k:?}) && has_unique_missing_kind())",
        );
        // Kind-scoped implication law on the empty arm — lacks_only
        // implies !has.
        if via_lacks_only {
            assert!(
                !empty.has(k),
                "empty_parent().lacks_only({k:?}) == true but has({k:?}) == true",
            );
        }
    }
    // Kind-domain exhaustivity on the empty arm — at most 1 kind is
    // the sole missing kind. On `ALL.len() >= 2` the count is 0; on
    // the degenerate `ALL.len() == 1` regime (no production parent)
    // the count is 1.
    let empty_count = <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
        .filter(|k| empty.lacks_only(*k))
        .count();
    assert!(
        empty_count <= 1,
        "empty_parent(): at most 1 kind may satisfy lacks_only, got {empty_count}",
    );

    let all_len = <T::Kind as tatara_closed_set::ClosedSet>::ALL.len();

    // Single-slot sweep — the composition-law shape across
    // `ClosedSet::ALL × ALL`, plus a factory-precondition truth-table
    // pin whose expected shape is derived from the abstract factory
    // contract (`single_slot(populated)` populates exactly `populated`
    // → the missing set is `ALL - {populated}`, size `all_len - 1`).
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        for probed in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let via_lacks_only = parent.lacks_only(probed);
            // Factory-precondition truth table on the well-formed
            // single-slot arm: the missing set is `ALL - {populated}`,
            // so lacks_only(probed) is `true` iff exactly one slot is
            // missing (`all_len == 2`) AND probed names that missing
            // slot (`probed != populated`). This hard-codes the well-
            // formed diagonal expectation so a factory drift that
            // populates the wrong kind — or an empty parent, or the
            // saturated parent — surfaces here BEFORE any composition
            // law reconciles two internally-drifted trait bodies.
            let expected_single = all_len == 2 && probed != populated;
            assert_eq!(
                via_lacks_only, expected_single,
                "single_slot({populated:?}).lacks_only({probed:?}) must equal {expected_single} on ALL.len() == {all_len}",
            );
            // Widened uniqueness composition law — the primary
            // pin-point on the missing axis.
            assert_eq!(
                via_lacks_only,
                parent.unique_missing_kind() == Some(probed),
                "single_slot({populated:?}).lacks_only({probed:?}) drifted from (unique_missing_kind() == Some({probed:?}))",
            );
            // Cardinality-refinement composition law under complement.
            assert_eq!(
                via_lacks_only,
                !parent.has(probed) && parent.has_unique_missing_kind(),
                "single_slot({populated:?}).lacks_only({probed:?}) drifted from (!has({probed:?}) && has_unique_missing_kind())",
            );
            // Kind-scoped implication law under complement —
            // lacks_only implies !has.
            if via_lacks_only {
                assert!(
                    !parent.has(probed),
                    "single_slot({populated:?}).lacks_only({probed:?}) == true but has({probed:?}) == true",
                );
            }
        }
        // Kind-domain exhaustivity on the well-formed arm — at most 1
        // kind satisfies lacks_only. On `ALL.len() == 2` the count is
        // exactly 1 (the non-populated kind); on `ALL.len() >= 3` the
        // count is 0.
        let well_formed_count = <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| parent.lacks_only(*k))
            .count();
        let expected_well_formed_count = usize::from(all_len == 2);
        assert_eq!(
            well_formed_count, expected_well_formed_count,
            "single_slot({populated:?}): exactly {expected_well_formed_count} kinds must satisfy lacks_only on ALL.len() == {all_len}, got {well_formed_count}",
        );
    }

    // Two-slot sweep — every off-diagonal pair populates two slots, so
    // the missing set is `ALL - {a, b}`, size `all_len - 2`. On
    // `ALL.len() == 3` exactly 1 slot is missing (the third kind), so
    // exactly 1 kind satisfies lacks_only. On `ALL.len() >= 4` ≥ 2
    // slots are missing, so no kind satisfies lacks_only. The
    // composition-law shape binds every regime; the factory-
    // precondition truth-table pin catches drift like a saturated /
    // empty / single-slot two_slot factory that would otherwise slip
    // past the internally-consistent composition laws.
    for a in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        for b in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            if a == b {
                continue;
            }
            let parent = two_slot(a, b);
            for k in <T::Kind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .copied()
            {
                let via_lacks_only = parent.lacks_only(k);
                // Factory-precondition truth table on the two-slot
                // arm: the missing set is `ALL - {a, b}`, so
                // lacks_only(k) is `true` iff exactly one slot is
                // missing (`all_len == 3`) AND k names that missing
                // slot (`k != a && k != b`).
                let expected_two = all_len == 3 && k != a && k != b;
                assert_eq!(
                    via_lacks_only, expected_two,
                    "two_slot({a:?}, {b:?}).lacks_only({k:?}) must equal {expected_two} on ALL.len() == {all_len}",
                );
                // Widened uniqueness composition law on the multi-
                // populated arm.
                assert_eq!(
                    via_lacks_only,
                    parent.unique_missing_kind() == Some(k),
                    "two_slot({a:?}, {b:?}).lacks_only({k:?}) drifted from (unique_missing_kind() == Some({k:?}))",
                );
                // Cardinality-refinement composition law under
                // complement.
                assert_eq!(
                    via_lacks_only,
                    !parent.has(k) && parent.has_unique_missing_kind(),
                    "two_slot({a:?}, {b:?}).lacks_only({k:?}) drifted from (!has({k:?}) && has_unique_missing_kind())",
                );
                // Kind-scoped implication law under complement.
                if via_lacks_only {
                    assert!(
                        !parent.has(k),
                        "two_slot({a:?}, {b:?}).lacks_only({k:?}) == true but has({k:?}) == true",
                    );
                }
            }
            // Kind-domain exhaustivity on the multi-populated arm — at
            // most 1 kind is the sole missing kind. On `ALL.len() ==
            // 3` the count is exactly 1 (the third kind); on
            // `ALL.len() >= 4` the count is 0.
            let multi_count = <T::Kind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .copied()
                .filter(|k| parent.lacks_only(*k))
                .count();
            let expected_multi_count = usize::from(all_len == 3);
            assert_eq!(
                multi_count, expected_multi_count,
                "two_slot({a:?}, {b:?}): exactly {expected_multi_count} kinds must satisfy lacks_only on ALL.len() == {all_len}, got {multi_count}",
            );
        }
    }
}

/// Generic closed-set-complement testkit on the kind-scoped SUBSET
/// axis — pins that [`TaggedUnion::lacks`] agrees with the negated
/// [`TaggedUnion::has`], the missing-set membership projection
/// [`TaggedUnion::missing_kinds`], the kind-scoped strict-refinement
/// peer [`TaggedUnion::lacks_only`], AND the missing-axis cardinality
/// scalar [`TaggedUnion::missing_kind_count`] across every
/// [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL) single-slot
/// arrangement, every off-diagonal two-slot pair, AND the empty-
/// parent baseline.
///
/// Closed-set-complement mirror of [`TaggedUnion::has`] under the
/// (populated, missing) duality — where `has(kind)` is the populated-
/// axis SUBSET primitive, `lacks(kind)` is the missing-axis SUBSET
/// primitive. Together with [`TaggedUnion::has_only`] (populated-axis
/// EQUAL) and [`TaggedUnion::lacks_only`] (missing-axis EQUAL) they
/// close the 2×2 kind-scoped (populated, missing) × (subset, equal)
/// grid. The five sub-assertions swept per arrangement + the empty-
/// parent baseline:
///
/// 1. **Definitional complement law**: `lacks(kind) == !has(kind)`
///    on every arm — the trait's default body composition is a
///    single bit-flip past [`TaggedUnion::has`], and no override
///    may drift the two primitives apart.
/// 2. **Missing-set membership composition law**:
///    `lacks(kind) == missing_kinds().contains(&kind)` on every
///    arm — closed-set-complement peer of the populated-axis law
///    `has(kind) == populated_kinds().contains(&kind)` swept by
///    [`assert_populated_kinds_matches_has`].
/// 3. **Kind-scoped implication law**: `lacks_only(kind) →
///    lacks(kind)` on every arm — if `kind` is the SOLE missing
///    slot then `kind` is missing. Byte-for-byte missing-axis peer
///    of the `has_only(kind) → has(kind)` implication that binds
///    [`TaggedUnion::has_only`] to [`TaggedUnion::has`] on the
///    strict-refinement axis.
/// 4. **Cardinality-partition law**: `<T::Kind as ClosedSet>::ALL
///    .iter().filter(|k| parent.lacks(*k)).count() ==
///    parent.missing_kind_count()` on every arm — the count of
///    kinds satisfying `lacks` equals the parent's missing-slot
///    count. Closed-set-complement peer of the populated-axis law
///    `count k where has(k) == populated_kind_count()`.
/// 5. **Factory-precondition truth table** whose expected shape is
///    derived from the abstract factory contract (`empty_parent()`
///    missing set is all of `ALL`, size `all_len`;
///    `single_slot(populated)` missing set is `ALL - {populated}`,
///    size `all_len - 1`; `two_slot(a, b)` missing set is `ALL -
///    {a, b}`, size `all_len - 2`) — hard-codes the arm expectation
///    across every `ALL.len()` regime so a factory drift that
///    yields a saturated / drifted parent surfaces BEFORE any
///    composition law reconciles two internally-drifted trait
///    bodies.
///
/// A fifth sibling tagged-union parent picks up the closed-set-
/// complement check on the kind-scoped SUBSET axis through ONE
/// `impl TaggedUnion for X` block plus ONE per-site `single_slot_X`
/// factory plus ONE per-site `two_slot_X` factory plus ONE per-site
/// `empty_X` factory plus ONE call site — no re-authored `lacks`
/// sweep at the test surface.
///
/// Same [`crate::lifetime::Lifetime`] exclusion as the sibling
/// primitives — the `T: TaggedUnion` bound doesn't reach it.
///
/// # Theory grounding
///
/// - THEORY.md §II.1 invariant 5 — composition preserves proofs.
///   The kind-scoped closed-set-complement projection lives at ONE
///   substrate site as a definitional negation of
///   [`TaggedUnion::has`]. The five composition laws (definitional
///   complement, missing-set membership, kind-scoped implication
///   from `lacks_only`, cardinality partition against
///   `missing_kind_count`, factory-precondition truth table) live at
///   ONE substrate site inside the testkit's per-arm sweep — pinned
///   across every production tagged union at compile time via the
///   trait's default body composition, not per-parent.
/// - THEORY.md §VI.1 — generation over composition. A new
///   [`Self::Kind`] variant added to `ALL` reaches this primitive
///   mechanically through the delegated [`Self::has`] — the five
///   laws hold on the widened kind set without further per-caller
///   edit.
#[track_caller]
pub fn assert_lacks_matches_has_complement<T, F, G, H>(single_slot: F, two_slot: G, empty_parent: H)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
    G: Fn(T::Kind, T::Kind) -> T,
    H: Fn() -> T,
{
    let all_len = <T::Kind as tatara_closed_set::ClosedSet>::ALL.len();

    // Empty-parent baseline — every `lacks(k)` returns `true`
    // (empty parent has every slot missing). The factory-
    // precondition truth-table pin catches an `empty_parent` that
    // drifts from empty (a single-slot or saturated factory
    // masquerading as empty) BEFORE any composition law reconciles
    // two internally-drifted trait bodies.
    let empty = empty_parent();
    assert!(
        empty.is_empty(),
        "TaggedUnion::lacks() testkit: empty_parent() must satisfy is_empty() == true",
    );
    let empty_missing_count = empty.missing_kind_count();
    for k in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let via_lacks = empty.lacks(k);
        // Factory-precondition truth table on the empty arm: the
        // missing set is all of `ALL`, so `lacks(k) == true` for
        // every `k`.
        assert!(
            via_lacks,
            "empty_parent().lacks({k:?}) must equal true (empty parent has every slot missing)",
        );
        // Definitional complement law on the empty arm.
        assert_eq!(
            via_lacks,
            !empty.has(k),
            "empty_parent().lacks({k:?}) drifted from !has({k:?})",
        );
        // Missing-set membership composition law on the empty arm.
        assert_eq!(
            via_lacks,
            empty.missing_kinds().contains(&k),
            "empty_parent().lacks({k:?}) drifted from missing_kinds().contains(&{k:?})",
        );
        // Kind-scoped implication law on the empty arm — lacks_only
        // implies lacks. On any `ALL.len() >= 2` closed set the
        // empty parent has ≥ 2 missing so lacks_only(k) == false on
        // every k, and the implication is vacuously true; on the
        // degenerate `ALL.len() == 1` regime lacks_only(k) == true
        // on the sole k, and the implication holds because lacks(k)
        // == true too.
        if empty.lacks_only(k) {
            assert!(
                via_lacks,
                "empty_parent().lacks_only({k:?}) == true but lacks({k:?}) == false",
            );
        }
    }
    // Cardinality-partition law on the empty arm — every kind
    // satisfies lacks, so the count equals missing_kind_count()
    // which equals ALL.len().
    let empty_count = <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
        .filter(|k| empty.lacks(*k))
        .count();
    assert_eq!(
        empty_count, empty_missing_count,
        "empty_parent(): count of kinds satisfying lacks ({empty_count}) drifted from missing_kind_count() ({empty_missing_count})",
    );
    assert_eq!(
        empty_count, all_len,
        "empty_parent(): count of kinds satisfying lacks must equal ALL.len() ({all_len}), got {empty_count}",
    );

    // Single-slot sweep — the composition-law shape across
    // `ClosedSet::ALL × ALL`, plus a factory-precondition truth-
    // table pin whose expected shape is derived from the abstract
    // factory contract (`single_slot(populated)` populates exactly
    // `populated` → the missing set is `ALL - {populated}`, so
    // `lacks(probed) == true` iff `probed != populated`).
    for populated in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        let parent = single_slot(populated);
        let parent_missing_count = parent.missing_kind_count();
        for probed in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let via_lacks = parent.lacks(probed);
            // Factory-precondition truth table on the well-formed
            // single-slot arm.
            let expected_single = probed != populated;
            assert_eq!(
                via_lacks, expected_single,
                "single_slot({populated:?}).lacks({probed:?}) must equal {expected_single}",
            );
            // Definitional complement law.
            assert_eq!(
                via_lacks,
                !parent.has(probed),
                "single_slot({populated:?}).lacks({probed:?}) drifted from !has({probed:?})",
            );
            // Missing-set membership composition law.
            assert_eq!(
                via_lacks,
                parent.missing_kinds().contains(&probed),
                "single_slot({populated:?}).lacks({probed:?}) drifted from missing_kinds().contains(&{probed:?})",
            );
            // Kind-scoped implication law — lacks_only implies
            // lacks.
            if parent.lacks_only(probed) {
                assert!(
                    via_lacks,
                    "single_slot({populated:?}).lacks_only({probed:?}) == true but lacks({probed:?}) == false",
                );
            }
        }
        // Cardinality-partition law on the well-formed arm — the
        // count of kinds satisfying lacks equals
        // missing_kind_count() which equals ALL.len() - 1.
        let well_formed_count = <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
            .filter(|k| parent.lacks(*k))
            .count();
        assert_eq!(
            well_formed_count, parent_missing_count,
            "single_slot({populated:?}): count of kinds satisfying lacks ({well_formed_count}) drifted from missing_kind_count() ({parent_missing_count})",
        );
        let expected_single_missing = all_len - 1;
        assert_eq!(
            well_formed_count, expected_single_missing,
            "single_slot({populated:?}): count of kinds satisfying lacks must equal ALL.len() - 1 ({expected_single_missing}), got {well_formed_count}",
        );
    }

    // Two-slot sweep — every off-diagonal pair populates two slots,
    // so the missing set is `ALL - {a, b}`, size `all_len - 2`, and
    // `lacks(k) == true` iff `k != a && k != b`. On `ALL.len() == 2`
    // `all_len - 2 == 0` (the two-slot arm saturates), so `lacks(k)
    // == false` on every k; the composition-law shape binds every
    // regime.
    for a in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        for b in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            if a == b {
                continue;
            }
            let parent = two_slot(a, b);
            let parent_missing_count = parent.missing_kind_count();
            for k in <T::Kind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .copied()
            {
                let via_lacks = parent.lacks(k);
                // Factory-precondition truth table on the two-slot
                // arm.
                let expected_two = k != a && k != b;
                assert_eq!(
                    via_lacks, expected_two,
                    "two_slot({a:?}, {b:?}).lacks({k:?}) must equal {expected_two}",
                );
                // Definitional complement law.
                assert_eq!(
                    via_lacks,
                    !parent.has(k),
                    "two_slot({a:?}, {b:?}).lacks({k:?}) drifted from !has({k:?})",
                );
                // Missing-set membership composition law.
                assert_eq!(
                    via_lacks,
                    parent.missing_kinds().contains(&k),
                    "two_slot({a:?}, {b:?}).lacks({k:?}) drifted from missing_kinds().contains(&{k:?})",
                );
                // Kind-scoped implication law.
                if parent.lacks_only(k) {
                    assert!(
                        via_lacks,
                        "two_slot({a:?}, {b:?}).lacks_only({k:?}) == true but lacks({k:?}) == false",
                    );
                }
            }
            // Cardinality-partition law on the two-slot arm.
            let multi_count = <T::Kind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .copied()
                .filter(|k| parent.lacks(*k))
                .count();
            assert_eq!(
                multi_count, parent_missing_count,
                "two_slot({a:?}, {b:?}): count of kinds satisfying lacks ({multi_count}) drifted from missing_kind_count() ({parent_missing_count})",
            );
            let expected_two_missing = all_len - 2;
            assert_eq!(
                multi_count, expected_two_missing,
                "two_slot({a:?}, {b:?}): count of kinds satisfying lacks must equal ALL.len() - 2 ({expected_two_missing}), got {multi_count}",
            );
        }
    }
}

/// Generic ambiguity testkit — pins that [`TaggedUnion::variant`]
/// resolves to [`TaggedUnionError::ambiguous`] on EVERY off-diagonal
/// `(a, b)` pair in [`ClosedSet::ALL`](tatara_closed_set::ClosedSet::ALL)
/// `× ALL`.
///
/// Substrate primitive for the sibling
/// `_two_slots_is_ambiguous_across_every_pair` tests on `ProcessSpec`
/// ([`crate::encapsulates::EncapsulationKind`],
/// [`crate::export::ArtifactSource`], [`crate::export::VectorChannel`])
/// that pre-lift each restated the same nested-`for a in K::ALL { for
/// b in K::ALL { if a == b { continue; } … } }` sweep at their own
/// test bodies — byte-identical projections whose only per-carrier
/// knobs are the (Kind type + the `two_slot_X(a, b) -> Parent`
/// two-slot factory) pair. Post-lift each site collapses to ONE
/// `assert_two_slots_ambiguous::<Xxx, _>(two_slot_X)` invocation.
///
/// The `two_slot` closure stays per-site — every one of the three
/// production sites already owns a `two_slot_kind /
/// two_slot_source / two_slot_channel` helper that composes two
/// `single_slot_X`s per-field. The closure IS the "populate both
/// slots a and b" ground truth for the carrier's field structure;
/// lifting it into the primitive would collapse per-site field-
/// composition knowledge that stays deliberately local.
///
/// The pair sweep excludes the diagonal (`a == b`) — a single slot
/// populated is exactly-one, not many, and the round-trip primitive
/// [`assert_variant_round_trip`] already pins that populated slot's
/// resolution. This primitive is the peer contract for the Many arm.
///
/// A fifth sibling tagged-union parent picks up the ambiguity check
/// through ONE `impl TaggedUnion for X` block + ONE per-site
/// `two_slot_X` helper + ONE `assert_two_slots_ambiguous::<X, _>`
/// call site — no re-authored nested-for sweep at the test surface,
/// no re-authored `assert_eq!(..., X::Error::Ambiguous, ...)` arm.
///
/// The [`crate::lifetime::Lifetime`] site is DELIBERATELY excluded
/// — `Lifetime` doesn't impl [`TaggedUnion`] (its error carrier has
/// no `Empty` arm; its `variant()` returns `Ok(Permanent)` on empty
/// rather than an `Empty` typed error), so the `<T: TaggedUnion>`
/// bound doesn't reach it. Its per-site ambiguity assertion binds
/// through the inherent `.variant()` + hand-authored two-slot
/// probe. Same reasoning as [`resolve_or_err`]'s and
/// [`assert_variant_round_trip`]'s exclusions.
#[track_caller]
pub fn assert_two_slots_ambiguous<T, F>(two_slot: F)
where
    T: TaggedUnion,
    T::Kind: PartialEq + std::fmt::Debug,
    T::Error: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind, T::Kind) -> T,
{
    let expected = T::Error::ambiguous();
    for a in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
        for b in <T::Kind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            if a == b {
                continue;
            }
            let parent = two_slot(a, b);
            let err = parent.variant().err().unwrap_or_else(|| {
                panic!("({a:?}, {b:?}) two-slot parent must not resolve to a variant")
            });
            assert_eq!(err, expected, "({a:?}, {b:?}) should resolve Ambiguous");
        }
    }
}

/// Generic wire-key / kind-label alignment testkit — pins that every
/// single-slot parent serializes to a JSON object with EXACTLY ONE key
/// whose name equals `<T::Kind as tatara_closed_set::ClosedSet>::label`
/// on the populated slot's kind.
///
/// Substrate primitive for the four sibling
/// `X_kind_as_str_matches_field_name` / `intent_kind_as_str_matches_intent_field_name`
/// tests on `ProcessSpec` ([`crate::intent::Intent`],
/// [`crate::encapsulates::EncapsulationKind`],
/// [`crate::export::ArtifactSource`], [`crate::export::VectorChannel`])
/// that pre-lift each restated the same wire-format sweep at their own
/// test bodies:
///
/// 1. For each `k in K::ALL`, construct a single-slot parent via
///    the site-local `single_slot_X(k) -> Parent` factory.
/// 2. Serialize it to the wire format and assert that the emitted
///    key matches `k.as_str()`.
///
/// Post-lift each site's alignment test collapses to ONE
/// `assert_single_slot_key_matches_label::<T, _>(single_slot_X)`
/// invocation whose body IS the substrate primitive's own dispatch.
/// A fifth sibling picks up the alignment check through ONE call site.
///
/// The primitive projects through `serde_json::to_value` rather than
/// `serde_yaml::to_string` for two reasons: (1) the check is
/// structural (exactly-one-key + name equality), not textual (substring
/// against a `"{key}:"` YAML fragment), so a future site that gains
/// non-tagged-union metadata fields is caught HERE at the exactly-one
/// arm — the YAML-substring check the three encapsulates / export sites
/// carried pre-lift would silently pass on such drift. (2) serde's
/// field-rename projection (`rename_all = "camelCase"`) is format-
/// agnostic, so a JSON check pins the SAME invariant a YAML check
/// would pin, byte-identically. Every one of the four production
/// parents already emits exactly one key on a single-slot populate —
/// their `#[serde(default, skip_serializing_if = "Option::is_none")]`
/// annotations on every tagged-union slot guarantee it — so upgrading
/// the three YAML sites to the JSON exactly-one check is a strict
/// strengthening.
///
/// The `single_slot` closure stays per-site — every one of the four
/// production sites already owns a `single_slot_intent /
/// single_slot_kind / single_slot_source / single_slot_channel` helper
/// that constructs a minimally-valid parent with the addressed slot's
/// inner spec populated; the closure IS the "populate slot k" ground
/// truth for the carrier's field structure. Reused verbatim from the
/// [`assert_variant_round_trip`] primitive.
///
/// The [`crate::lifetime::Lifetime`] site is DELIBERATELY excluded
/// from THIS trait-projected surface — `Lifetime` doesn't impl
/// [`TaggedUnion`] (its `variant()` returns `Ok(Permanent)` on empty
/// rather than an `Empty` typed error), so the `<T: TaggedUnion>`
/// bound doesn't reach it. The bound-relaxed peer
/// [`assert_wire_key_matches_label`] carries the SAME sweep body
/// under `<T: Serialize>` + `<K: ClosedSet>` alone — Lifetime binds
/// through it directly and this trait-projected surface becomes a
/// one-line delegation whose only load-bearing purpose is to name
/// the TaggedUnion parent's `T::Kind` associated type at the call
/// site (existing `assert_single_slot_key_matches_label::<T, _>(f)`
/// callers stay unchanged; the peer inflects the same body onto
/// non-TaggedUnion parents).
#[track_caller]
pub fn assert_single_slot_key_matches_label<T, F>(single_slot: F)
where
    T: TaggedUnion + serde::Serialize,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    assert_wire_key_matches_label::<T, T::Kind, F>(single_slot);
}

/// Bound-relaxed peer of [`assert_single_slot_key_matches_label`] —
/// the SAME wire-key alignment sweep, but on any `(K, T)` pair where
/// `K: ClosedSet` addresses `T: Serialize` through a caller-supplied
/// `single_slot: Fn(K) -> T` factory. Drops the `T: TaggedUnion`
/// bound the sibling primitive carries so parents whose empty
/// resolution shape diverges from the tagged-union convention (the
/// canonical example: [`crate::lifetime::Lifetime`], whose empty
/// resolves to `Permanent(&DEFAULT_PERMANENT)` rather than to an
/// [`TaggedUnionError::empty`] carrier) still bind through ONE
/// substrate wire-key alignment site.
///
/// The two primitives share ONE sweep body; the trait-projected
/// [`assert_single_slot_key_matches_label`] is now a one-line
/// delegation to this bound-relaxed peer, so every drift-arm the
/// sibling `#[should_panic]` probe pins on the delegating surface
/// mechanically pins here too. The compounding gain: a fifth parent
/// whose closed-set kind K doesn't ride the TaggedUnion trait (a
/// future variant surface with a default-arm on empty; a wire-only
/// enum whose parent is a wrapper struct that never publishes a
/// resolver; a K-addressed `HashMap<K, Payload>` where the payload
/// isn't a tagged-union variant carrier at all) picks up wire-key
/// alignment through ONE call site — no re-authored serialize +
/// exactly-one-key + name-equality body at the test surface, no
/// per-parent drift risk where the trait-projected surface catches
/// it and the bespoke surface forgets.
///
/// The primitive binds `<K: ClosedSet + PartialEq + Debug>` (the
/// strict union of the sweep body's projection + the panic-message
/// substrate-wide shape) — every production `ClosedSet` implementor
/// across the crate carries `Debug + PartialEq` through the
/// substrate-wide `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash,
/// DeriveClosedSet)]` shape, so no site pays a bound-widening cost
/// to bind through this peer.
#[track_caller]
pub fn assert_wire_key_matches_label<T, K, F>(single_slot: F)
where
    T: serde::Serialize,
    K: tatara_closed_set::ClosedSet + PartialEq + std::fmt::Debug,
    F: Fn(K) -> T,
{
    for k in <K as tatara_closed_set::ClosedSet>::ALL.iter().copied() {
        let parent = single_slot(k);
        let value = serde_json::to_value(&parent)
            .unwrap_or_else(|e| panic!("single_slot({k:?}) must serialize as JSON: {e}"));
        let obj = value.as_object().unwrap_or_else(|| {
            panic!("single_slot({k:?}) must serialize to a JSON object, got {value}")
        });
        let keys: Vec<&String> = obj.keys().collect();
        assert_eq!(
            keys.len(),
            1,
            "single_slot({k:?}) must serialize to exactly one populated field, got keys: {keys:?}",
        );
        let expected = <K as tatara_closed_set::ClosedSet>::label(k);
        assert_eq!(
            keys[0].as_str(),
            expected,
            "wire-key drift for {k:?}: single_slot's populated field '{}' must equal <K as ClosedSet>::label ({expected:?})",
            keys[0],
        );
    }
}

/// Generic Display / [`ClosedSet::label`](tatara_closed_set::ClosedSet::label)
/// alignment testkit — pins that [`core::fmt::Display`] renders each variant
/// BYTE-IDENTICALLY to the trait-visible `ClosedSet::label` projection for
/// every implementor.
///
/// Substrate primitive for the 29 sibling
/// `X_display_matches_as_str` tests across `tatara-process`
/// (`AllocationPhase`, `IntentKind`, `WorkloadKind`, `EncapsulationMode`,
/// `EncapsulationTarget`, `ConditionKind`, `TerminateReasonKind`,
/// `AutoTerminateKind`, `SighupStrategy`, `ReplacementPolicy`,
/// `ReturnPolicy`, `MemberState`, `PoolPhase`, `VerificationPhase`,
/// `SelectStrategyKind`, `MustReachPhase`, `ExportTrigger`,
/// `ReportFormat`, `ReportPayloadShape`, `ArtifactKind`, `ChannelKind`,
/// `DataClassification`, `ConvergencePointType`, `Arity`,
/// `SubstrateType`, `CalmClassification`, `OptimizationDirection`,
/// `HorizonKind`, `TeardownPolicy`) that pre-lift each restated the
/// same
/// ```text
/// for v in K::ALL {
///     assert_eq!(v.to_string(), v.as_str());
/// }
/// ```
/// two-line probe verbatim at their own test bodies — byte-identical
/// projections whose only per-carrier knob is the closed-set type name.
/// Post-lift each site collapses to ONE
/// `assert_display_matches_label::<X>()` invocation whose body IS the
/// substrate primitive's own dispatch.
///
/// The primitive projects through the STABLE trait-visible name
/// [`ClosedSet::label`](tatara_closed_set::ClosedSet::label) rather
/// than the inherent `.as_str()` each site publishes locally. Every
/// production implementor here derives its `label` body from `as_str`
/// via `#[closed_set(via = "as_str", display)]` (the substrate-wide
/// derive shape), so the two are byte-identical by construction; the
/// primitive's projection through `label` therefore pins the SAME
/// invariant the pre-lift bodies pinned while binding to the
/// stable trait-visible surface. A future implementor whose inherent
/// canonical projection is named something other than `as_str` (e.g.
/// `.keyword()`, `.spelling()`) but still routes through
/// `#[closed_set(via = "...", display)]` picks up the alignment check
/// through ONE `assert_display_matches_label::<X>()` invocation with
/// no inherent-name coupling at the test site.
///
/// A fifth (or thirtieth, or hundredth) implementor picks up the
/// Display-alignment check through ONE
/// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `display`
/// attribute + ONE `assert_display_matches_label::<X>()` call site —
/// no re-authored two-line
/// `for v in K::ALL { assert_eq!(v.to_string(), v.as_str()) }` body
/// at the test surface, no per-site drift risk where 28 sibling
/// tests carry the assertion and the 29th forgets.
///
/// Sibling shape to [`assert_kind_list_matches_closed_set`] on the
/// (`T::KIND_LIST` slash-join, `Display` byte-identity) axis: both
/// project the closed-set's label surface onto ONE typed contract
/// and pin it against a per-implementor rendering; the former for
/// the tagged-union parent's [`TaggedUnion::KIND_LIST`] `&'static str`,
/// this one for the enum's `Display` byte stream. Together they close
/// the "label surface must round-trip verbatim" invariant every
/// closed-set-carrying implementor across the crate publishes.
#[track_caller]
pub fn assert_display_matches_label<T>()
where
    T: tatara_closed_set::ClosedSet + core::fmt::Display + PartialEq + core::fmt::Debug,
{
    let type_name = core::any::type_name::<T>();
    for &v in <T as tatara_closed_set::ClosedSet>::ALL {
        let rendered = v.to_string();
        let expected = <T as tatara_closed_set::ClosedSet>::label(v);
        assert_eq!(
            rendered.as_str(),
            expected,
            "{type_name}: Display drifted from ClosedSet::label for {v:?} — expected {expected:?}, got {rendered:?}",
        );
    }
}

/// CANONICAL-KEY CONTRACT testkit — pins that each variant's serde
/// serialization (as a JSON string value, unquoted) matches its
/// canonical [`ClosedSet::label`](tatara_closed_set::ClosedSet::label)
/// projection BYTE-IDENTICALLY for every implementor.
///
/// Substrate primitive for the 20 sibling
/// `X_as_str_matches_serde` tests across `tatara-process`
/// (`TeardownPolicy`, `EncapsulationMode`, `ConditionKind`,
/// `SighupStrategy`, `ReplacementPolicy`, `ReturnPolicy`, `MemberState`,
/// `PoolPhase`, `VerificationPhase`, `MustReachPhase`, `WorkloadKind`,
/// `ExportTrigger`, `ReportFormat`, `DataClassification`,
/// `ConvergencePointType`, `SubstrateType`, `CalmClassification`,
/// `OptimizationDirection`, `HorizonKind`, `AllocationPhase`) that
/// pre-lift each restated the same
/// ```text
/// for v in K::ALL {
///     let serialized = serde_json::to_string(&v).expect("serialize");
///     let unquoted = serialized
///         .trim_start_matches('"')
///         .trim_end_matches('"')
///         .to_string();
///     assert_eq!(unquoted, v.as_str(), "as_str drift for {v:?}: ...");
/// }
/// ```
/// four-line probe verbatim at their own test bodies — byte-identical
/// projections whose only per-carrier knob is the closed-set type name.
/// Post-lift each site collapses to ONE
/// `assert_label_matches_serde_serialization::<X>()` invocation whose
/// body IS the substrate primitive's own dispatch.
///
/// The primitive projects through the STABLE trait-visible name
/// [`ClosedSet::label`](tatara_closed_set::ClosedSet::label) rather
/// than the inherent `.as_str()` each site publishes locally. Every
/// production implementor here derives its `label` body from `as_str`
/// via `#[closed_set(via = "as_str", display)]` + `#[serde(rename_all
/// = "PascalCase")]` (the substrate-wide derive shape), so the two are
/// byte-identical by construction; the primitive's projection through
/// `label` therefore pins the SAME invariant the pre-lift bodies
/// pinned while binding to the stable trait-visible surface. A future
/// implementor whose canonical inherent projection is named something
/// other than `as_str` (e.g. `.keyword()`, `.spelling()`) but still
/// routes through `#[closed_set(via = "...")]` picks up the wire-format
/// alignment check through ONE call with no inherent-name coupling at
/// the test site.
///
/// A twenty-first (or hundredth) implementor picks up the alignment
/// check through ONE `#[derive(tatara_closed_set::DeriveClosedSet)]` +
/// `#[derive(serde::Serialize)]` + `#[serde(rename_all = "...")]`
/// attribute + ONE `assert_label_matches_serde_serialization::<X>()`
/// call site — no re-authored four-line probe body at the test surface,
/// no per-site drift risk where 19 sibling tests carry the assertion
/// and the 20th forgets, no `serde_json::to_string`+`trim_matches`+
/// `assert_eq!` composition re-derived per implementor.
///
/// Sibling shape to [`assert_display_matches_label`] on the
/// (Display byte-identity, serde-wire-format byte-identity) axis: both
/// project the closed-set's label surface onto ONE typed contract and
/// pin it against a per-implementor rendering; the former for the
/// enum's [`Display`](core::fmt::Display) byte stream, this one for
/// the serde JSON-string wire format. Together they close the "label
/// surface renders verbatim across every projection consumers reach
/// for" invariant every closed-set-carrying implementor across the
/// crate publishes.
#[track_caller]
pub fn assert_label_matches_serde_serialization<T>()
where
    T: tatara_closed_set::ClosedSet + serde::Serialize + core::fmt::Debug,
{
    let type_name = core::any::type_name::<T>();
    for &v in <T as tatara_closed_set::ClosedSet>::ALL {
        let serialized = serde_json::to_string(&v).unwrap_or_else(|e| {
            panic!("{type_name}: closed-set variant {v:?} must serialize: {e}")
        });
        let unquoted = serialized.trim_start_matches('"').trim_end_matches('"');
        let expected = <T as tatara_closed_set::ClosedSet>::label(v);
        assert_eq!(
            unquoted,
            expected,
            "{type_name}: serde output drifted from ClosedSet::label for {v:?} — expected {expected:?}, got {unquoted:?} (full serialization {serialized:?})",
        );
    }
}

/// CLOSED-SET CONVENTION PANEL testkit — pins the FULL three-axis
/// label-surface convention (parse round-trip, Display byte-identity,
/// serde-JSON-string byte-identity) at ONE substrate call site per
/// implementor.
///
/// Compound-lift of [`tatara_closed_set::assert_closed_set_well_formed`]
/// + [`assert_display_matches_label`] + [`assert_label_matches_serde_
/// serialization`] — every closed-set enum on `ProcessSpec` that
/// carries the substrate-wide `#[derive(DeriveClosedSet)] +
/// #[derive(Serialize)] + #[closed_set(via = "as_str", display)] +
/// #[serde(rename_all = "PascalCase")]` shape publishes ALL THREE
/// axes of the label surface, and pre-lift each production test
/// module hand-authored three sibling one-line tests
/// (`X_is_well_formed_closed_set`, `X_display_matches_as_str`,
/// `X_as_str_matches_serde`) that each restated the SAME
/// `crate::tagged_union::assert_<axis>::<X>()` invocation with only
/// the axis name varying between siblings. Post-lift each site
/// collapses to ONE `assert_closed_set_convention_panel::<X>()`
/// invocation whose body IS the three-axis composition dispatched
/// through the substrate primitive here.
///
/// The three sub-assertions stay independently callable — a future
/// implementor that publishes only two of the three axes (a
/// `Display`-less internal enum, e.g., or a `Serialize`-less
/// runtime-only enum) still binds through the two sibling primitives
/// individually. The compound is a strict superset: any implementor
/// that satisfies the compound's bounds already satisfies each
/// sub-assertion's bounds by construction, and the failure mode of
/// each sub-assertion still surfaces with the exact-message
/// granularity `#[track_caller]` gives the individual primitives
/// (the compound is `#[track_caller]` too, so a sub-assertion panic
/// surfaces at the compound's call site — a future promotion could
/// wrap each sub-assertion in a `std::panic::catch_unwind` to
/// aggregate all three axis failures into ONE panic message, but the
/// pre-lift discipline is that each axis's failure surfaces with its
/// own diagnostic).
///
/// The compound's bounds are the strict union of the three sub-
/// assertions' bounds:
///   - [`assert_closed_set_well_formed`] requires
///     `T: ClosedSet + PartialEq + Debug` + `T::Unknown: Display`;
///   - [`assert_display_matches_label`] requires
///     `T: ClosedSet + Display + PartialEq + Debug`;
///   - [`assert_label_matches_serde_serialization`] requires
///     `T: ClosedSet + Serialize + Debug`.
/// The union `T: ClosedSet + Serialize + Display + PartialEq + Debug`
/// + `T::Unknown: Display` is what every 3-axis production consumer
/// already satisfies through the substrate-wide derive shape — any
/// implementor that fails the compound's bounds would ALSO fail the
/// individual sub-assertions' bounds, so the compound doesn't shrink
/// the reachable set of implementors relative to hand-authoring the
/// three sibling calls.
///
/// A future FOURTH label-surface projection (e.g. a `serde_yaml`
/// byte-identity axis if the crate gains a YAML wire form on closed-
/// set enums, or a `kubectl_annotation` axis if the reconciler grows
/// an annotation-carried label surface) lands as ONE new
/// `assert_<axis>_matches_label::<T>()` substrate primitive + ONE
/// new line inside this compound's body. Every one of the ~20
/// production implementors of the panel picks up the fourth-axis
/// alignment check mechanically at their sole `assert_closed_set_
/// convention_panel::<X>()` call site — no per-implementor test-site
/// authoring, no per-crate test-site drop pathway where 19 sibling
/// call sites carry the check and the 20th forgets. The exact
/// promise `e4a4eba`'s future gain #2 named after
/// `assert_label_matches_serde_serialization` opened the wire-format
/// axis: a workspace-wide panel with byte-identical calling shapes
/// (`assert_X::<T>()`) that composes as freely as its sub-primitives.
///
/// Sibling shape to [`assert_variant_round_trip`] +
/// [`assert_kind_list_matches_closed_set`] +
/// [`assert_two_slots_ambiguous`] +
/// [`assert_single_slot_key_matches_label`] on the tagged-union
/// PARENT axis: the parent-side compound would compose the four
/// parent-side per-axis primitives, this one composes the three
/// child-side per-axis primitives on the child's [`ClosedSet`]
/// surface. Together the two compounds close the "closed-set
/// convention holds across every projection consumers reach for" at
/// two adjacent panels — one per closed-set-carrying enum, one per
/// tagged-union parent.
///
/// Theory anchor: THEORY.md §V.1 (knowable platform) — the
/// three-axis label-surface convention becomes ONE typed theorem
/// provable generically over any
/// `T: ClosedSet + Serialize + Display + PartialEq + Debug` bound
/// rather than THREE hand-authored per-implementor one-line probes
/// held coherent by test-module convention. THEORY.md §II.1
/// invariant 5 (composition preserves proofs) — the three sub-
/// assertions compose structurally through ONE primitive here, so a
/// regression at ONE axis surfaces at the sub-assertion's own
/// panic message rather than as silent drift at every consumer that
/// might otherwise forget to include the axis in its per-site
/// author-time enumeration.
#[track_caller]
pub fn assert_closed_set_convention_panel<T>()
where
    T: tatara_closed_set::ClosedSet
        + serde::Serialize
        + core::fmt::Display
        + PartialEq
        + core::fmt::Debug,
    T::Unknown: core::fmt::Display,
{
    tatara_closed_set::assert_closed_set_well_formed::<T>();
    assert_display_matches_label::<T>();
    assert_label_matches_serde_serialization::<T>();
}

/// TAGGED-UNION CONVENTION PANEL testkit — pins the FULL four-axis
/// tagged-union parent convention (KIND_LIST diagnostic-stability,
/// variant round-trip on the single-slot side, ALL×ALL two-slot
/// ambiguity, wire-key alignment on the single-slot side) at ONE
/// substrate call site per parent.
///
/// Parent-side compound-lift, sibling to
/// [`assert_closed_set_convention_panel`] on the child's
/// [`tatara_closed_set::ClosedSet`] axis. Composes
/// [`assert_kind_list_matches_closed_set`] (no fixture) +
/// [`assert_variant_round_trip`] (`single_slot`) +
/// [`assert_two_slots_ambiguous`] (`two_slot`) +
/// [`assert_single_slot_key_matches_label`] (`single_slot`).
///
/// Every one of the four production `.variant()` parents on
/// `ProcessSpec` ([`crate::intent::Intent`],
/// [`crate::encapsulates::EncapsulationKind`],
/// [`crate::export::ArtifactSource`],
/// [`crate::export::VectorChannel`]) publishes the four-axis
/// convention through the shared substrate-wide attribute-set:
/// `#[derive(DeriveClosedSet)]` on the addressing `Kind`,
/// `declare_tagged_union_impls!` for the resolver+selector+trait
/// triple, `#[serde(rename_all = "camelCase")]` +
/// `#[serde(default, skip_serializing_if = "Option::is_none")]` on
/// every tagged-union slot. Pre-lift each production site
/// hand-authored FOUR sibling per-axis tests (`X_kind_round_trips_through_variant_kind`
/// / `X_kind_list_matches_ClosedSet_labels` /
/// `X_two_slots_are_ambiguous` /
/// `X_kind_as_str_matches_field_name`) that each restated the
/// SAME `crate::tagged_union::assert_<axis>::<T, _>(fixture)`
/// invocation with only the axis name + fixture arity varying
/// between siblings. Post-lift each site's four per-axis sibling
/// tests can collapse to ONE
/// `assert_tagged_union_convention_panel::<T, _, _>(
/// single_slot_X, two_slot_X)` invocation whose body IS the
/// four-axis composition dispatched through the substrate
/// primitive here.
///
/// The two closures stay per-site — every one of the four
/// production parents already owns a `single_slot_X(k) -> Parent`
/// / `two_slot_X(a, b) -> Parent` pair, and the substrate-local
/// `{single,two}_slot_*_probe` peers (siblings to the wire-key
/// sweep's substrate-local probes) let the substrate-wide sweep
/// below bind through the compound without reaching across the
/// per-crate test-module boundaries. Lifting the two closures
/// into the primitive would collapse the per-site construction
/// knowledge that stays deliberately local — the closure IS the
/// "populate slot k" / "populate the (a, b) pair" ground truth
/// for the parent's field structure.
///
/// Bounds are the strict union of the four sub-assertions' bounds:
/// [`assert_kind_list_matches_closed_set`] requires
/// `T: TaggedUnion`; [`assert_variant_round_trip`] requires
/// `T: TaggedUnion` + `T::Kind: PartialEq + Debug`
/// + `F: Fn(T::Kind) -> T`; [`assert_two_slots_ambiguous`] requires
/// `T: TaggedUnion` + `T::Kind: PartialEq + Debug`
/// + `T::Error: PartialEq + Debug` + `F: Fn(T::Kind, T::Kind) -> T`;
/// [`assert_single_slot_key_matches_label`] requires
/// `T: TaggedUnion + Serialize` + `T::Kind: PartialEq + Debug`
/// + `F: Fn(T::Kind) -> T`. The union
/// `T: TaggedUnion + Serialize` + `T::Kind: PartialEq + Debug`
/// + `T::Error: PartialEq + Debug` + `F1: Fn(T::Kind) -> T`
/// + `F2: Fn(T::Kind, T::Kind) -> T` is what every one of the four
/// production parents already satisfies through the shared
/// substrate-wide impls — any implementor that fails the compound's
/// bounds would ALSO fail the individual sub-assertions' bounds,
/// so the compound doesn't shrink the reachable set of
/// implementors relative to hand-authoring the four sibling calls.
/// The `single_slot` closure is dispatched to
/// [`assert_variant_round_trip`] by reference so the compound can
/// re-dispatch it to [`assert_single_slot_key_matches_label`] by
/// value on the final call — a caller passes ONE `Fn(T::Kind) -> T`
/// factory (not `FnOnce`) at the two axes that need it.
///
/// `#[track_caller]` on both the compound and each sub-primitive,
/// so a sub-assertion panic surfaces at the compound's caller site
/// with the failing axis's exact panic-message substring
/// (e.g. "TaggedUnion KIND_LIST drift", "select→variant_kind
/// round-trip failed", "should resolve Ambiguous", "wire-key
/// drift"). The four sub-assertions stay independently callable —
/// a future parent that publishes only three of the four axes (a
/// wire-format-less runtime parent, e.g., or an
/// ambiguity-less parent whose `.variant()` short-circuits on
/// the first populated slot) still binds through the sibling
/// primitives individually.
///
/// A future FIFTH parent-side projection (e.g. a
/// `two_slots_have_stable_diagnostic` axis if the ambiguity error
/// gains a per-parent operator-facing message, or a
/// `variant_kind_stays_stable_across_generation` axis if the
/// resolver's iteration order becomes load-bearing) lands as ONE
/// new `assert_<axis>::<T, _>(...)` substrate primitive + ONE new
/// line inside this compound's body. Every one of the four
/// production parents picks up the fifth-axis alignment check
/// mechanically at their sole
/// `assert_tagged_union_convention_panel::<T, _, _>(single_slot,
/// two_slot)` call site — no per-parent test-site authoring, no
/// per-crate test-site drop pathway where 3 sibling call sites
/// carry the check and the 4th forgets. The exact promise the
/// child-side [`assert_closed_set_convention_panel`] compound's
/// docstring named on the child axis, extended here to the parent
/// axis: a workspace-wide panel with byte-identical calling shapes
/// (`assert_<compound>::<T, _, _>(single_slot, two_slot)`) that
/// composes as freely as its sub-primitives.
///
/// The [`crate::lifetime::Lifetime`] site is DELIBERATELY excluded
/// through the `T: TaggedUnion` bound — `Lifetime`'s `variant()`
/// returns `Ok(Permanent)` on empty rather than an `Empty` typed
/// error, so its projection shape diverges from the four
/// Empty-projecting parents. Same reasoning as [`resolve_or_err`]'s
/// / [`assert_variant_round_trip`]'s / [`assert_two_slots_ambiguous`]'s
/// / [`assert_single_slot_key_matches_label`]'s exclusions.
///
/// Theory anchor: THEORY.md §V.1 (knowable platform) — the
/// four-axis parent-side tagged-union convention becomes ONE typed
/// theorem provable generically over any
/// `T: TaggedUnion + Serialize` bound rather than FOUR
/// hand-authored per-parent tests held coherent by test-module
/// convention. THEORY.md §II.1 invariant 5 (composition preserves
/// proofs) — the four sub-assertions compose structurally through
/// ONE primitive here, so a regression at ONE axis surfaces at the
/// sub-assertion's own panic message rather than as silent drift
/// at every parent that might otherwise forget to include the
/// axis in its per-site author-time enumeration.
#[track_caller]
pub fn assert_tagged_union_convention_panel<T, F1, F2>(single_slot: F1, two_slot: F2)
where
    T: TaggedUnion + serde::Serialize,
    T::Kind: PartialEq + std::fmt::Debug,
    T::Error: PartialEq + std::fmt::Debug,
    F1: Fn(T::Kind) -> T,
    F2: Fn(T::Kind, T::Kind) -> T,
{
    assert_kind_list_matches_closed_set::<T>();
    assert_variant_round_trip::<T, _>(&single_slot);
    assert_two_slots_ambiguous::<T, _>(two_slot);
    assert_has_matches_select::<T, _>(&single_slot);
    assert_find_agrees_with_has::<T, _>(&single_slot);
    assert_single_slot_key_matches_label::<T, _>(single_slot);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum V {
        A,
        B,
        C,
    }

    #[test]
    fn empty_candidate_list_is_none() {
        let r: Result<V, _> = resolve(std::iter::empty());
        assert_eq!(r.unwrap_err(), ResolveError::None);
    }

    #[test]
    fn all_none_is_none() {
        let r: Result<V, _> = resolve([None, None, None]);
        assert_eq!(r.unwrap_err(), ResolveError::None);
    }

    #[test]
    fn single_some_is_resolved_regardless_of_position() {
        assert_eq!(resolve([Some(V::A), None, None]).unwrap(), V::A);
        assert_eq!(resolve([None, Some(V::B), None]).unwrap(), V::B);
        assert_eq!(resolve([None, None, Some(V::C)]).unwrap(), V::C);
    }

    #[test]
    fn two_or_more_some_is_many() {
        assert_eq!(
            resolve([Some(V::A), Some(V::B), None]).unwrap_err(),
            ResolveError::Many
        );
        assert_eq!(
            resolve([Some(V::A), None, Some(V::C)]).unwrap_err(),
            ResolveError::Many
        );
        assert_eq!(
            resolve([None, Some(V::B), Some(V::C)]).unwrap_err(),
            ResolveError::Many
        );
        assert_eq!(
            resolve([Some(V::A), Some(V::B), Some(V::C)]).unwrap_err(),
            ResolveError::Many
        );
    }

    /// Short-circuit invariant: once `Many` is decided, the sweep does
    /// NOT inspect further candidates. Encode it as a side-effect probe.
    #[test]
    fn many_short_circuits_after_second_some() {
        let mut visited = 0usize;
        let candidates = (0..4).map(|i| {
            visited += 1;
            // first two are Some, the rest would be Some too if we got there.
            Some(i)
        });
        // We can't actually consume `visited` here because it's borrowed in
        // the closure — fold the count via the resolver's short-circuit.
        let _ = resolve(candidates);
        // The resolver evaluates the iterator lazily up to the second
        // Some — index 0 (found = Some(0)), index 1 (Many → return).
        assert_eq!(visited, 2);
    }

    /// The helper is value-agnostic — works with borrowed enum-view
    /// types matching the actual on-the-typescape callsites.
    #[test]
    fn works_with_borrowed_enum_view() {
        #[derive(Debug, PartialEq)]
        enum View<'a> {
            X(&'a u32),
            Y(&'a String),
        }
        let x = 7u32;
        let r = resolve([Some(View::X(&x)), None]).unwrap();
        assert_eq!(r, View::X(&7));
    }

    /// Local sibling-shaped carrier used to pin the trait +
    /// [`resolve_or_err`] dispatch without depending on the
    /// crate's real error types.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum E {
        Empty(&'static str),
        Ambiguous,
    }

    impl TaggedUnionError for E {
        fn empty(kinds: &'static str) -> Self {
            E::Empty(kinds)
        }
        fn ambiguous() -> Self {
            E::Ambiguous
        }
    }

    /// Four-outcome truth table at the compound-lift boundary.
    /// Pins that the two failure arms of [`resolve`] project onto
    /// the trait's two typed constructors byte-identically, and
    /// that the Ok arm falls through untouched.
    #[test]
    fn resolve_or_err_dispatches_each_arm_through_the_trait() {
        const KINDS: &str = "a/b/c";

        assert_eq!(
            resolve_or_err::<V, E>([Some(V::A), None, None], KINDS).unwrap(),
            V::A
        );
        assert_eq!(
            resolve_or_err::<V, E>([None, Some(V::B), None], KINDS).unwrap(),
            V::B
        );

        assert_eq!(
            resolve_or_err::<V, E>([None, None, None], KINDS).unwrap_err(),
            E::Empty(KINDS)
        );

        assert_eq!(
            resolve_or_err::<V, E>([Some(V::A), Some(V::B), None], KINDS).unwrap_err(),
            E::Ambiguous
        );
    }

    /// The trait's Empty arm carries the &'static str the caller
    /// hands `resolve_or_err`, verbatim — a rename at the caller's
    /// `KINDS` constant reaches the diagnostic surface intact.
    #[test]
    fn resolve_or_err_empty_carries_the_caller_kinds_verbatim() {
        const KINDS_ALPHA: &str = "alpha/beta";
        const KINDS_GAMMA: &str = "gamma/delta/epsilon";

        assert_eq!(
            resolve_or_err::<V, E>([None, None], KINDS_ALPHA).unwrap_err(),
            E::Empty(KINDS_ALPHA)
        );
        assert_eq!(
            resolve_or_err::<V, E>([None, None, None], KINDS_GAMMA).unwrap_err(),
            E::Empty(KINDS_GAMMA)
        );
    }

    /// The compound-lift preserves [`resolve`]'s short-circuit at
    /// the Many arm — a third-and-later candidate is not
    /// inspected once the second populated entry is seen.
    #[test]
    fn resolve_or_err_short_circuits_on_many() {
        let mut visited = 0usize;
        let candidates = (0..4).map(|i| {
            visited += 1;
            Some(i)
        });
        let _ = resolve_or_err::<i32, E>(candidates, "irrelevant");
        assert_eq!(visited, 2);
    }

    // -------------------------------------------------------------------
    // `declare_tagged_union_error!` macro-emitted carrier — pins the
    // shape a fifth sibling would land through the macro instead of
    // hand-rolling the enum + `impl TaggedUnionError` block.
    // -------------------------------------------------------------------

    crate::declare_tagged_union_error! {
        pub(super) MacroEmittedError,
        empty = "test carrier has no variant set (one of {0} required)",
        ambiguous = "test carrier has multiple variants set; exactly one required",
    }

    /// The macro-emitted carrier's [`TaggedUnionError`] impl dispatches
    /// the same four-outcome truth table [`resolve_or_err`] pins for a
    /// hand-rolled carrier — pins that swapping a hand-rolled carrier
    /// for a macro-emitted one preserves the compound-lift's projection
    /// byte-identically.
    #[test]
    fn macro_emitted_carrier_projects_through_resolve_or_err() {
        const KINDS: &str = "one/two/three";

        assert_eq!(
            resolve_or_err::<V, MacroEmittedError>([Some(V::A), None, None], KINDS).unwrap(),
            V::A
        );
        assert_eq!(
            resolve_or_err::<V, MacroEmittedError>([None, None, None], KINDS).unwrap_err(),
            MacroEmittedError::Empty(KINDS)
        );
        assert_eq!(
            resolve_or_err::<V, MacroEmittedError>([Some(V::A), Some(V::B), None], KINDS)
                .unwrap_err(),
            MacroEmittedError::Ambiguous
        );
    }

    /// The macro-emitted carrier's `#[error(...)]` messages render the
    /// two operator-facing diagnostic strings the caller handed the
    /// macro, verbatim — a rename at the caller's literal reaches the
    /// operator diagnostic surface intact.
    #[test]
    fn macro_emitted_carrier_display_renders_caller_literals_verbatim() {
        assert_eq!(
            MacroEmittedError::Empty("alpha/beta").to_string(),
            "test carrier has no variant set (one of alpha/beta required)",
        );
        assert_eq!(
            MacroEmittedError::Ambiguous.to_string(),
            "test carrier has multiple variants set; exactly one required",
        );
    }

    /// The macro-emitted carrier is `Copy` — a substrate-wide promise
    /// pinned by the macro's `#[derive(..., Copy, ...)]` header so a
    /// consumer treating the carrier as a value type (memcpy-cheap
    /// return, `.copied()` on an `Option<&E>`) stays valid across every
    /// carrier the macro emits.
    #[test]
    fn macro_emitted_carrier_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<MacroEmittedError>();
    }

    // -------------------------------------------------------------------
    // `TaggedUnion` trait — declarative surface pinning the
    // (Kind, Error, KIND_LIST) triple. `assert_kind_list_matches_closed_set`
    // is the generic diagnostic-stability testkit primitive shared by
    // every implementor's `_error_empty_lists_every_kind_in_canonical_order`
    // site.
    // -------------------------------------------------------------------

    /// Local sibling-shaped Kind enum used to pin the trait's
    /// diagnostic-stability primitive without depending on the crate's
    /// four production tagged unions. Uses [`tatara_closed_set::DeriveClosedSet`]
    /// so `<Self as ClosedSet>::labels_joined("/")` reaches the same
    /// substrate composition the four production sites bind through.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_closed_set::DeriveClosedSet)]
    #[closed_set(via = "as_str", generate_unknown, display)]
    enum LocalKind {
        Alpha,
        Beta,
        Gamma,
    }

    impl LocalKind {
        const ALL: [Self; 3] = [Self::Alpha, Self::Beta, Self::Gamma];
        const fn as_str(self) -> &'static str {
            match self {
                Self::Alpha => "alpha",
                Self::Beta => "beta",
                Self::Gamma => "gamma",
            }
        }
    }

    /// Local parent type — impls [`TaggedUnion`] with a `KIND_LIST`
    /// literal that matches the canonical `<LocalKind as
    /// ClosedSet>::labels_joined("/")` projection. Carries three
    /// `Option<u32>` slots so the substrate-primitive
    /// [`TaggedUnion::variant`] default method can be exercised
    /// directly on a sibling-shaped-but-crate-local parent, isolated
    /// from the four production tagged unions.
    ///
    /// Derives [`serde::Serialize`] with `skip_serializing_if =
    /// "Option::is_none"` on every slot so the wire-format primitive
    /// [`assert_single_slot_key_matches_label`] can be exercised
    /// directly against the sibling-shaped scaffold — mirrors the
    /// `#[serde(default, skip_serializing_if = "Option::is_none")]`
    /// annotation every one of the four production tagged unions
    /// carries on its own slots.
    #[derive(Default, serde::Serialize)]
    struct LocalParent {
        #[serde(skip_serializing_if = "Option::is_none")]
        alpha: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        beta: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        gamma: Option<u32>,
    }

    /// Borrowed-view of a populated slot on [`LocalParent`] — the
    /// return type of [`LocalKind::select`] and the substrate-primitive
    /// [`TaggedUnion::variant`] default on `LocalParent`.
    #[derive(Debug, PartialEq)]
    enum LocalVariant<'a> {
        Alpha(&'a u32),
        Beta(&'a u32),
        Gamma(&'a u32),
    }

    impl VariantSelector<LocalParent> for LocalKind {
        type Variant<'a> = LocalVariant<'a>;
        fn select<'a>(self, parent: &'a LocalParent) -> Option<LocalVariant<'a>>
        where
            Self: 'a,
        {
            match self {
                Self::Alpha => parent.alpha.as_ref().map(LocalVariant::Alpha),
                Self::Beta => parent.beta.as_ref().map(LocalVariant::Beta),
                Self::Gamma => parent.gamma.as_ref().map(LocalVariant::Gamma),
            }
        }
    }

    impl VariantKind<LocalKind> for LocalVariant<'_> {
        fn variant_kind(&self) -> LocalKind {
            match self {
                Self::Alpha(_) => LocalKind::Alpha,
                Self::Beta(_) => LocalKind::Beta,
                Self::Gamma(_) => LocalKind::Gamma,
            }
        }
    }

    crate::declare_tagged_union_error! {
        pub(super) LocalParentError,
        empty = "local carrier has no variant set (one of {0} required)",
        ambiguous = "local carrier has multiple variants set; exactly one required",
    }

    impl TaggedUnion for LocalParent {
        type Kind = LocalKind;
        type Error = LocalParentError;
        const KIND_LIST: &'static str = "alpha/beta/gamma";
    }

    /// The testkit primitive resolves the canonical join of every
    /// `LocalKind` variant's label against the trait's `KIND_LIST`
    /// constant byte-identically — the four production sites bind
    /// through this exact dispatch. The Ok arm is the "no drift"
    /// outcome; a divergence surfaces as a labeled assertion failure.
    #[test]
    fn assert_kind_list_matches_closed_set_accepts_coherent_impl() {
        assert_kind_list_matches_closed_set::<LocalParent>();
    }

    /// The testkit primitive is a `#[track_caller]` compound-lift:
    /// a drift between `<T::Kind as ClosedSet>::labels_joined("/")`
    /// and `T::KIND_LIST` fails the assertion at the caller's site,
    /// not inside the primitive body. Pin the failing case with a
    /// local parent whose `KIND_LIST` is deliberately mis-authored
    /// (a variant reorder), so a regression that drops the drift
    /// detection fails-loudly here.
    #[test]
    #[should_panic(expected = "TaggedUnion KIND_LIST drift")]
    fn assert_kind_list_matches_closed_set_rejects_drifted_impl() {
        struct Drifted;
        // The `TaggedUnion` trait bounds `Kind: VariantSelector<Self>`
        // with `Variant<'a>: VariantKind<Self>`; the drift test only
        // exercises `assert_kind_list_matches_closed_set` (which reaches
        // the (Kind, KIND_LIST) pair, not the sweep body), so reusing
        // the sibling `LocalVariant<'a>` (with its already-load-bearing
        // `impl VariantKind<LocalKind>`) + always-`None` `select`
        // satisfies both bounds without wiring a real projection.
        impl VariantSelector<Drifted> for LocalKind {
            type Variant<'a> = LocalVariant<'a>;
            fn select<'a>(self, _: &'a Drifted) -> Option<LocalVariant<'a>>
            where
                Self: 'a,
            {
                None
            }
        }
        impl TaggedUnion for Drifted {
            type Kind = LocalKind;
            type Error = LocalParentError;
            // Deliberate drift — canonical join is "alpha/beta/gamma".
            const KIND_LIST: &'static str = "beta/alpha/gamma";
        }
        assert_kind_list_matches_closed_set::<Drifted>();
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` impls [`TaggedUnion`] with `KIND_LIST` reaching
    /// the substrate primitive `assert_kind_list_matches_closed_set`
    /// coherently. Sweep every production implementor at ONE
    /// substrate boundary so a regression that drifts a production
    /// site's `KIND_LIST` (or renames a `Kind` variant without
    /// updating the constant) fails BOTH at the per-crate test site
    /// AND at this substrate-wide sweep — no per-implementor test
    /// site can drop the check silently.
    #[test]
    fn every_production_tagged_union_binds_through_the_testkit_primitive() {
        assert_kind_list_matches_closed_set::<crate::intent::Intent>();
        assert_kind_list_matches_closed_set::<crate::encapsulates::EncapsulationKind>();
        assert_kind_list_matches_closed_set::<crate::export::ArtifactSource>();
        assert_kind_list_matches_closed_set::<crate::export::VectorChannel>();
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the wire-key primitive
    /// [`assert_single_slot_key_matches_label`] coherently — every
    /// per-site `single_slot_X(k)` factory serializes to a JSON object
    /// with EXACTLY ONE key whose name equals `k.label()` (delegating
    /// to each Kind's inherent `as_str`, matching the parent's serde
    /// `rename_all = "camelCase"` projection). Sweep every production
    /// implementor at ONE substrate boundary so a regression that
    /// drifts a production site's `single_slot_X` factory (populates
    /// the wrong slot; leaks residual slots between calls) OR the
    /// parent's field-to-kind alignment (`as_str` returns "receipts"
    /// but the field is named `receipt`) fails BOTH at the per-crate
    /// test site AND at this substrate-wide sweep — no per-implementor
    /// test site can drop the check silently.
    #[test]
    fn every_production_tagged_union_binds_through_the_wire_key_testkit_primitive() {
        assert_single_slot_key_matches_label::<crate::intent::Intent, _>(single_slot_intent_probe);
        assert_single_slot_key_matches_label::<crate::encapsulates::EncapsulationKind, _>(
            single_slot_encapsulation_kind_probe,
        );
        assert_single_slot_key_matches_label::<crate::export::ArtifactSource, _>(
            single_slot_artifact_source_probe,
        );
        assert_single_slot_key_matches_label::<crate::export::VectorChannel, _>(
            single_slot_vector_channel_probe,
        );
    }

    /// The parent-side four-axis compound-lift dispatches Ok on a
    /// coherent implementor — the [`LocalParent`] scaffold publishes
    /// every axis (`TaggedUnion` via
    /// [`crate::declare_tagged_union_error`]-emitted `LocalParentError`
    /// + Serialize via `#[derive(serde::Serialize)]` +
    /// `LocalKind: PartialEq + Debug` +
    /// `LocalParentError: PartialEq + Debug`), matching the
    /// substrate-wide four-axis convention every one of the four
    /// production parents carries. The Ok arm is the "no drift"
    /// outcome; a divergence at ANY sub-assertion's composition
    /// inside the compound (accidentally dropped, silently reordered,
    /// or short-circuited) surfaces at the sub-primitive's own
    /// panic message (each sub-primitive is `#[track_caller]`), and
    /// the per-axis failing arms are pinned by the sibling
    /// `#[should_panic]` probes already at the per-axis primitive
    /// layer (`assert_kind_list_matches_closed_set_rejects_drifted_impl`,
    /// `assert_variant_round_trip_rejects_factory_that_leaves_slot_empty`,
    /// `assert_two_slots_ambiguous_rejects_factory_that_populates_only_one_slot`,
    /// `assert_single_slot_key_matches_label_rejects_factory_that_populates_wrong_slot`).
    /// Re-authoring per-axis drift probes at the compound layer
    /// would restate the SAME four axis-typed contracts through a
    /// compound wrapper without adding a new gate.
    #[test]
    fn assert_tagged_union_convention_panel_accepts_coherent_local_impl() {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(11),
                    LocalKind::Beta => p.beta = Some(22),
                    LocalKind::Gamma => p.gamma = Some(33),
                }
            }
            p
        }
        assert_tagged_union_convention_panel::<LocalParent, _, _>(single_slot, two_slot);
    }

    /// Every one of the four production `.variant()` parents on
    /// `ProcessSpec` binds through the four-axis convention-panel
    /// primitive [`assert_tagged_union_convention_panel`] coherently.
    /// Sweep every production parent at ONE substrate boundary so a
    /// regression that (a) drops ANY of the four sub-assertions from
    /// the compound's body, (b) reorders them in a way that skips
    /// one on Ok, (c) silently binds the compound against a
    /// hollowed-out sub-assertion body, or (d) drifts a substrate-
    /// local `{single,two}_slot_*_probe` fixture (populates the
    /// wrong slot; leaks residual slots between calls; the `.or()`
    /// composition drops a slot on the two-slot side) fails BOTH at
    /// the per-crate test site AND at this substrate-wide sweep.
    ///
    /// Pinned in lock-step with the sibling
    /// `every_production_tagged_union_binds_through_the_testkit_primitive`
    /// (KIND_LIST axis) and
    /// `every_production_tagged_union_binds_through_the_wire_key_testkit_primitive`
    /// (wire-key axis) sweeps — every parent enumerated below is a
    /// member of BOTH sibling sweeps (their bounds are strict
    /// subsets of the compound's `T: TaggedUnion + Serialize` +
    /// `T::Kind: PartialEq + Debug` + `T::Error: PartialEq + Debug`
    /// bound), and every parent additionally publishes both a
    /// substrate-local `single_slot_*_probe` and a
    /// substrate-local `two_slot_*_probe` peer above. Post-sweep the
    /// substrate-wide four-axis parent-side convention-panel
    /// discipline is a property of the workspace, not a per-file
    /// convention — even before any per-site test-body sweep
    /// collapses the four per-parent sibling tests into ONE compound
    /// call each.
    #[test]
    fn every_production_tagged_union_binds_through_the_convention_panel_testkit_primitive() {
        assert_tagged_union_convention_panel::<crate::intent::Intent, _, _>(
            single_slot_intent_probe,
            two_slot_intent_probe,
        );
        assert_tagged_union_convention_panel::<crate::encapsulates::EncapsulationKind, _, _>(
            single_slot_encapsulation_kind_probe,
            two_slot_encapsulation_kind_probe,
        );
        assert_tagged_union_convention_panel::<crate::export::ArtifactSource, _, _>(
            single_slot_artifact_source_probe,
            two_slot_artifact_source_probe,
        );
        assert_tagged_union_convention_panel::<crate::export::VectorChannel, _, _>(
            single_slot_vector_channel_probe,
            two_slot_vector_channel_probe,
        );
    }

    /// The Display / label alignment primitive dispatches Ok on a
    /// coherent implementor — the [`LocalKind`] scaffold derives
    /// `Display` from `label` via `#[closed_set(via = "as_str",
    /// display)]`, matching the substrate-wide derive shape every
    /// production implementor across the crate carries. The Ok arm
    /// is the "no drift" outcome; a divergence surfaces as a labeled
    /// assertion failure at the caller site (this test's own line).
    #[test]
    fn assert_display_matches_label_accepts_coherent_impl() {
        assert_display_matches_label::<LocalKind>();
    }

    /// A local closed-set scaffold whose `Display` deliberately
    /// diverges from `label` — pins the failing arm of the primitive.
    /// The `#[closed_set(via = "as_str")]` attribute WITHOUT `display`
    /// leaves the `Display` impl uncovered by the derive, and the
    /// hand-authored `impl Display` below emits a suffixed rendering
    /// that no `label` projection returns. A regression that drops
    /// the alignment assertion inside
    /// [`assert_display_matches_label`] fails-loudly at this
    /// `#[should_panic]` probe before it can silently thread through
    /// the 29 production `X_display_matches_as_str` sites.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_closed_set::DeriveClosedSet)]
    #[closed_set(via = "as_str", generate_unknown)]
    enum DisplayDriftKind {
        Alpha,
        Beta,
    }

    impl DisplayDriftKind {
        const ALL: [Self; 2] = [Self::Alpha, Self::Beta];
        const fn as_str(self) -> &'static str {
            match self {
                Self::Alpha => "alpha",
                Self::Beta => "beta",
            }
        }
    }

    impl std::fmt::Display for DisplayDriftKind {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            // Deliberate drift — Display suffixes the label with a
            // marker no `label` projection returns.
            write!(f, "{}!", self.as_str())
        }
    }

    #[test]
    #[should_panic(expected = "Display drifted from ClosedSet::label")]
    fn assert_display_matches_label_rejects_drifted_impl() {
        assert_display_matches_label::<DisplayDriftKind>();
    }

    /// Every closed-set enum across `tatara-process` that carried a
    /// hand-rolled `X_display_matches_as_str` test pre-lift now binds
    /// through the substrate primitive at ONE call site each.  This
    /// substrate-wide sweep pins every production Display-alignment
    /// consumer at ONE boundary so a per-crate test-site drop cannot
    /// silently disable the check — the sweep here catches the drift
    /// even when the per-site test body is removed. Mirrors the
    /// `every_production_tagged_union_binds_through_the_testkit_primitive`
    /// and `every_production_tagged_union_binds_through_the_wire_key_testkit_primitive`
    /// sibling sweeps on the (`KIND_LIST` slash-join, wire-key)
    /// axes; this one closes the (`Display` byte-identity) axis.
    #[test]
    fn every_production_display_impl_binds_through_the_testkit_primitive() {
        assert_display_matches_label::<crate::allocation::AllocationPhase>();
        assert_display_matches_label::<crate::boundary::ConditionKind>();
        assert_display_matches_label::<crate::classification::Arity>();
        assert_display_matches_label::<crate::classification::CalmClassification>();
        assert_display_matches_label::<crate::classification::ConvergencePointType>();
        assert_display_matches_label::<crate::classification::DataClassification>();
        assert_display_matches_label::<crate::classification::HorizonKind>();
        assert_display_matches_label::<crate::classification::OptimizationDirection>();
        assert_display_matches_label::<crate::classification::SubstrateType>();
        assert_display_matches_label::<crate::compliance::VerificationPhase>();
        assert_display_matches_label::<crate::encapsulates::EncapsulationMode>();
        assert_display_matches_label::<crate::encapsulates::EncapsulationTarget>();
        assert_display_matches_label::<crate::export::ArtifactKind>();
        assert_display_matches_label::<crate::export::ChannelKind>();
        assert_display_matches_label::<crate::export::ExportTrigger>();
        assert_display_matches_label::<crate::export::ReportFormat>();
        assert_display_matches_label::<crate::export::ReportPayloadShape>();
        assert_display_matches_label::<crate::intent::IntentKind>();
        assert_display_matches_label::<crate::intent::WorkloadKind>();
        assert_display_matches_label::<crate::lifetime::LifetimeKind>();
        assert_display_matches_label::<crate::lifetime::TeardownPolicy>();
        assert_display_matches_label::<crate::lifetime_clock::AutoTerminateKind>();
        assert_display_matches_label::<crate::lifetime_clock::TerminateReasonKind>();
        assert_display_matches_label::<crate::matrix::SelectStrategyKind>();
        assert_display_matches_label::<crate::pool::MemberState>();
        assert_display_matches_label::<crate::pool::PoolPhase>();
        assert_display_matches_label::<crate::pool::ReplacementPolicy>();
        assert_display_matches_label::<crate::pool::ReturnPolicy>();
        assert_display_matches_label::<crate::signal::SighupStrategy>();
        assert_display_matches_label::<crate::spec::MustReachPhase>();
    }

    /// Local closed-set scaffold whose serde `rename_all = "lowercase"`
    /// projection matches its `via = "as_str"` label byte-identically —
    /// pins the Ok arm of the wire-format primitive. Every production
    /// implementor across the crate carries the substrate-wide
    /// `#[closed_set(via = "as_str")]` + `#[serde(rename_all = ...)]`
    /// pair whose alignment this scaffold pins on the sibling-shaped
    /// local surface.
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Hash,
        serde::Serialize,
        tatara_closed_set::DeriveClosedSet,
    )]
    #[serde(rename_all = "lowercase")]
    #[closed_set(via = "as_str", generate_unknown)]
    enum SerdeAlignedKind {
        Alpha,
        Beta,
    }

    impl SerdeAlignedKind {
        const ALL: [Self; 2] = [Self::Alpha, Self::Beta];
        const fn as_str(self) -> &'static str {
            match self {
                Self::Alpha => "alpha",
                Self::Beta => "beta",
            }
        }
    }

    #[test]
    fn assert_label_matches_serde_serialization_accepts_coherent_impl() {
        assert_label_matches_serde_serialization::<SerdeAlignedKind>();
    }

    /// A local closed-set scaffold whose serde output deliberately
    /// diverges from `label` — pins the failing arm of the wire-format
    /// primitive. The `#[serde(rename_all = "UPPERCASE")]` projection
    /// emits uppercase JSON strings while the `via = "as_str"` label
    /// stays lowercase. A regression that drops the alignment assertion
    /// inside [`assert_label_matches_serde_serialization`] fails-loudly
    /// at this `#[should_panic]` probe before it can silently thread
    /// through the 20 production `X_as_str_matches_serde` sites.
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Hash,
        serde::Serialize,
        tatara_closed_set::DeriveClosedSet,
    )]
    #[serde(rename_all = "UPPERCASE")]
    #[closed_set(via = "as_str", generate_unknown)]
    enum SerdeDriftKind {
        Alpha,
        Beta,
    }

    impl SerdeDriftKind {
        const ALL: [Self; 2] = [Self::Alpha, Self::Beta];
        const fn as_str(self) -> &'static str {
            match self {
                Self::Alpha => "alpha",
                Self::Beta => "beta",
            }
        }
    }

    #[test]
    #[should_panic(expected = "serde output drifted from ClosedSet::label")]
    fn assert_label_matches_serde_serialization_rejects_drifted_impl() {
        assert_label_matches_serde_serialization::<SerdeDriftKind>();
    }

    /// Local closed-set scaffold whose ALL THREE axes of the label-
    /// surface convention align by construction — pins the Ok arm of
    /// the compound-panel primitive.
    ///
    /// `#[serde(rename_all = "lowercase")]` matches the `via = "as_str"`
    /// labels byte-identically (the serde-alignment axis). The
    /// `display` sub-attribute on `#[closed_set(via = "as_str",
    /// display)]` derives `impl Display` from the same `as_str`
    /// projection (the Display-alignment axis). The `generate_unknown`
    /// sub-attribute emits the `T::Unknown` carrier the round-trip
    /// axis's `parse_label` returns on unknown input. Together these
    /// three attributes stamp the substrate-wide derive shape every
    /// production 3-axis-panel consumer carries; a caller that lands
    /// through this scaffold satisfies EVERY bound the compound's
    /// where-clause names.
    ///
    /// Peer to the sibling per-axis fixtures [`LocalKind`] (Display
    /// axis, no serde) and [`SerdeAlignedKind`] (serde axis, no
    /// Display) on the label-surface primitive family; this fixture
    /// closes the diagonal by carrying both attribute-sets at once,
    /// so a regression at ANY sub-assertion's composition inside the
    /// compound (the compound accidentally dropping the well-formed
    /// call, silently reordering the three calls, wrapping them in a
    /// short-circuit that skips the middle one on Ok, …) fails the
    /// compound's happy-path pin below rather than as silent drift at
    /// every 3-axis consumer.
    #[derive(
        Clone,
        Copy,
        Debug,
        PartialEq,
        Eq,
        Hash,
        serde::Serialize,
        tatara_closed_set::DeriveClosedSet,
    )]
    #[serde(rename_all = "lowercase")]
    #[closed_set(via = "as_str", generate_unknown, display)]
    enum PanelAlignedKind {
        Alpha,
        Beta,
    }

    impl PanelAlignedKind {
        const ALL: [Self; 2] = [Self::Alpha, Self::Beta];
        const fn as_str(self) -> &'static str {
            match self {
                Self::Alpha => "alpha",
                Self::Beta => "beta",
            }
        }
    }

    /// The compound-panel primitive dispatches Ok on a coherent
    /// implementor — [`PanelAlignedKind`] carries every attribute the
    /// substrate-wide 3-axis derive shape publishes, so all three
    /// sub-assertions the compound composes (well-formed, Display /
    /// label, serde / label) pass by construction. The Ok arm is the
    /// "no drift on any axis" outcome; a divergence at any single
    /// sub-assertion surfaces as that sub-assertion's own labeled
    /// panic message (with the caller-attributed line via
    /// `#[track_caller]` on both the compound and its sub-
    /// primitives), NOT as a silent pass.
    ///
    /// The per-axis failing arms are pinned by the sibling per-axis
    /// #[should_panic] probes above:
    ///   - the round-trip axis's failing arm is pinned by
    ///     [`tatara_closed_set::assert_closed_set_well_formed`]'s own
    ///     `#[should_panic]` probe in the `tatara-closed-set` crate;
    ///   - the Display axis's failing arm is pinned by
    ///     [`assert_display_matches_label_rejects_drifted_impl`] on
    ///     [`DisplayDriftKind`];
    ///   - the serde axis's failing arm is pinned by
    ///     [`assert_label_matches_serde_serialization_rejects_drifted_impl`]
    ///     on [`SerdeDriftKind`].
    /// Each per-axis drift fixture already surfaces its axis's exact
    /// panic-message substring, so re-authoring per-axis
    /// `#[should_panic]` probes at the compound layer would restate
    /// the SAME three axis-typed contracts through a compound
    /// wrapper — one more copy of the same three pins, not a new
    /// gate. The compound's happy-path pin here suffices to verify
    /// the composition doesn't lose ANY sub-assertion (a regression
    /// that swallows one axis silently would still fail the sibling
    /// sub-assertion's own drift probe on the drift fixture).
    #[test]
    fn assert_closed_set_convention_panel_accepts_coherent_impl() {
        assert_closed_set_convention_panel::<PanelAlignedKind>();
    }

    /// Every closed-set enum across `tatara-process` that publishes
    /// ALL THREE axes of the label-surface convention (well-formed +
    /// Display-alignment + serde-alignment) now binds through the
    /// substrate compound-panel primitive at ONE call site each in
    /// this sweep. Pinned in lock-step with the sibling
    /// `every_production_serde_serialization_binds_through_the_testkit_primitive`
    /// sweep — every enum enumerated below is a member of BOTH sweeps
    /// (the compound's `T: Serialize + Display + ClosedSet + ...`
    /// bound is a strict superset of `assert_label_matches_serde_
    /// serialization`'s `T: ClosedSet + Serialize + Debug` bound, and
    /// the 20 wire-format consumers all additionally impl Display via
    /// `#[closed_set(via = "as_str", display)]`).
    ///
    /// A regression that (a) drops the compound's `assert_closed_set_
    /// well_formed` dispatch, (b) reorders the three sub-assertions
    /// in a way that skips one on Ok, or (c) silently binds the
    /// compound against a hollowed-out sub-assertion body catches
    /// here at the substrate-wide boundary — the sweep pins every
    /// production 3-axis consumer's compound-panel discipline through
    /// ONE test even before any per-site test-body sweep collapses
    /// the three per-enum sibling tests into ONE compound call each.
    /// Post-sweep the substrate-wide compound-panel discipline is a
    /// property of the workspace, not a per-file convention.
    #[test]
    fn every_production_convention_panel_binds_through_the_testkit_primitive() {
        assert_closed_set_convention_panel::<crate::allocation::AllocationPhase>();
        assert_closed_set_convention_panel::<crate::boundary::ConditionKind>();
        assert_closed_set_convention_panel::<crate::classification::CalmClassification>();
        assert_closed_set_convention_panel::<crate::classification::ConvergencePointType>();
        assert_closed_set_convention_panel::<crate::classification::DataClassification>();
        assert_closed_set_convention_panel::<crate::classification::HorizonKind>();
        assert_closed_set_convention_panel::<crate::classification::OptimizationDirection>();
        assert_closed_set_convention_panel::<crate::classification::SubstrateType>();
        assert_closed_set_convention_panel::<crate::compliance::VerificationPhase>();
        assert_closed_set_convention_panel::<crate::encapsulates::EncapsulationMode>();
        assert_closed_set_convention_panel::<crate::export::ExportTrigger>();
        assert_closed_set_convention_panel::<crate::export::ReportFormat>();
        assert_closed_set_convention_panel::<crate::intent::WorkloadKind>();
        assert_closed_set_convention_panel::<crate::lifetime::TeardownPolicy>();
        assert_closed_set_convention_panel::<crate::pool::MemberState>();
        assert_closed_set_convention_panel::<crate::pool::PoolPhase>();
        assert_closed_set_convention_panel::<crate::pool::ReplacementPolicy>();
        assert_closed_set_convention_panel::<crate::pool::ReturnPolicy>();
        assert_closed_set_convention_panel::<crate::signal::SighupStrategy>();
        assert_closed_set_convention_panel::<crate::spec::MustReachPhase>();
    }

    /// Every closed-set enum across `tatara-process` that carried a
    /// hand-rolled `X_as_str_matches_serde` test pre-lift now binds
    /// through the substrate primitive at ONE call site each. This
    /// substrate-wide sweep pins every production wire-format alignment
    /// consumer at ONE boundary so a per-crate test-site drop cannot
    /// silently disable the check — the sweep here catches the drift
    /// even when the per-site test body is removed. Mirrors the sibling
    /// `every_production_display_impl_binds_through_the_testkit_primitive`
    /// sweep on the (Display byte-identity) axis; this one closes the
    /// (serde JSON-string byte-identity) axis.
    #[test]
    fn every_production_serde_serialization_binds_through_the_testkit_primitive() {
        assert_label_matches_serde_serialization::<crate::allocation::AllocationPhase>();
        assert_label_matches_serde_serialization::<crate::boundary::ConditionKind>();
        assert_label_matches_serde_serialization::<crate::classification::CalmClassification>();
        assert_label_matches_serde_serialization::<crate::classification::ConvergencePointType>();
        assert_label_matches_serde_serialization::<crate::classification::DataClassification>();
        assert_label_matches_serde_serialization::<crate::classification::HorizonKind>();
        assert_label_matches_serde_serialization::<crate::classification::OptimizationDirection>();
        assert_label_matches_serde_serialization::<crate::classification::SubstrateType>();
        assert_label_matches_serde_serialization::<crate::compliance::VerificationPhase>();
        assert_label_matches_serde_serialization::<crate::encapsulates::EncapsulationMode>();
        assert_label_matches_serde_serialization::<crate::export::ExportTrigger>();
        assert_label_matches_serde_serialization::<crate::export::ReportFormat>();
        assert_label_matches_serde_serialization::<crate::intent::WorkloadKind>();
        assert_label_matches_serde_serialization::<crate::lifetime::TeardownPolicy>();
        assert_label_matches_serde_serialization::<crate::pool::MemberState>();
        assert_label_matches_serde_serialization::<crate::pool::PoolPhase>();
        assert_label_matches_serde_serialization::<crate::pool::ReplacementPolicy>();
        assert_label_matches_serde_serialization::<crate::pool::ReturnPolicy>();
        assert_label_matches_serde_serialization::<crate::signal::SighupStrategy>();
        assert_label_matches_serde_serialization::<crate::spec::MustReachPhase>();
    }

    // Substrate-local single-slot factories — mirror the per-site
    // `single_slot_X` test helpers each production site owns, so the
    // substrate-wide sweep above binds through the wire-key primitive
    // without reaching across the per-crate test-module boundaries the
    // per-site helpers are scoped to. The primitive only requires that
    // the addressed slot on the parent is populated; the inner spec's
    // exact field values are irrelevant to the wire-key check.

    fn single_slot_intent_probe(kind: crate::intent::IntentKind) -> crate::intent::Intent {
        use crate::intent::{
            AplicacaoIntent, ContainerIntent, FluxIntent, GuestIntent, Intent, IntentKind,
            LispIntent, NixIntent, WorkloadKind,
        };
        match kind {
            IntentKind::Nix => Intent {
                nix: Some(NixIntent {
                    flake_ref: "f".into(),
                    attribute: "a".into(),
                    system: None,
                    attic_cache: None,
                    extra_args: vec![],
                    delegate_to_nix_build: false,
                }),
                ..Intent::default()
            },
            IntentKind::Flux => Intent {
                flux: Some(FluxIntent {
                    git_repository: "g".into(),
                    path: "p".into(),
                    git_repository_namespace: None,
                    target_namespace: None,
                    decrypt_sops: true,
                    helm_chart: None,
                    helm_values: None,
                }),
                ..Intent::default()
            },
            IntentKind::Lisp => Intent {
                lisp: Some(LispIntent {
                    source: "()".into(),
                    reader: "tatara-lisp".into(),
                    version: "v1".into(),
                    bindings: std::collections::BTreeMap::new(),
                }),
                ..Intent::default()
            },
            IntentKind::Container => Intent {
                container: Some(ContainerIntent {
                    image: "x".into(),
                    replicas: None,
                    command: vec![],
                    args: vec![],
                    env: std::collections::BTreeMap::new(),
                    workload_kind: WorkloadKind::default(),
                }),
                ..Intent::default()
            },
            IntentKind::Aplicacao => Intent {
                aplicacao: Some(AplicacaoIntent::chart_only("x", "1")),
                ..Intent::default()
            },
            IntentKind::Guest => Intent {
                guest: Some(GuestIntent {
                    spec: serde_json::json!({"name": "x"}),
                    state_dir: None,
                    allow_remote_build: None,
                }),
                ..Intent::default()
            },
        }
    }

    fn single_slot_encapsulation_kind_probe(
        target: crate::encapsulates::EncapsulationTarget,
    ) -> crate::encapsulates::EncapsulationKind {
        use crate::encapsulates::{
            BareWorkload, EncapsulationKind, EncapsulationTarget, ExistingHelmRelease,
            ExistingKustomization,
        };
        match target {
            EncapsulationTarget::ExistingHelmRelease => EncapsulationKind {
                existing_helm_release: Some(ExistingHelmRelease {
                    namespace: "ns".into(),
                    name: "hr".into(),
                    release_name: "rel".into(),
                }),
                ..EncapsulationKind::default()
            },
            EncapsulationTarget::ExistingKustomization => EncapsulationKind {
                existing_kustomization: Some(ExistingKustomization {
                    namespace: "ns".into(),
                    name: "ks".into(),
                }),
                ..EncapsulationKind::default()
            },
            EncapsulationTarget::BareWorkload => {
                let mut sel = std::collections::BTreeMap::new();
                sel.insert("app".into(), "x".into());
                EncapsulationKind {
                    bare_workload: Some(BareWorkload {
                        namespace: "ns".into(),
                        selector: sel,
                    }),
                    ..EncapsulationKind::default()
                }
            }
        }
    }

    fn single_slot_artifact_source_probe(
        kind: crate::export::ArtifactKind,
    ) -> crate::export::ArtifactSource {
        use crate::export::{
            ArtifactKind, ArtifactSource, ProcessSnapshotSource, ReceiptsSource, ReportFormat,
            RunMarkerSource, TestReportSource,
        };
        match kind {
            ArtifactKind::Receipts => ArtifactSource {
                receipts: Some(ReceiptsSource::default()),
                ..ArtifactSource::default()
            },
            ArtifactKind::TestReport => ArtifactSource {
                test_report: Some(TestReportSource {
                    configmap: "cm".into(),
                    key: "k".into(),
                    format: ReportFormat::Junit,
                    namespace: None,
                }),
                ..ArtifactSource::default()
            },
            ArtifactKind::ProcessSnapshot => ArtifactSource {
                process_snapshot: Some(ProcessSnapshotSource::default()),
                ..ArtifactSource::default()
            },
            ArtifactKind::RunMarker => ArtifactSource {
                run_marker: Some(RunMarkerSource::default()),
                ..ArtifactSource::default()
            },
        }
    }

    fn single_slot_vector_channel_probe(
        kind: crate::export::ChannelKind,
    ) -> crate::export::VectorChannel {
        use crate::export::{
            ChannelKind, HttpEventChannel, NatsSubjectChannel, StdoutChannel, VectorChannel,
        };
        match kind {
            ChannelKind::HttpEvent => VectorChannel {
                http_event: Some(HttpEventChannel::signal("x")),
                ..VectorChannel::default()
            },
            ChannelKind::NatsSubject => VectorChannel {
                nats_subject: Some(NatsSubjectChannel::publish("s", "S")),
                ..VectorChannel::default()
            },
            ChannelKind::Stdout => VectorChannel {
                stdout: Some(StdoutChannel::default()),
                ..VectorChannel::default()
            },
        }
    }

    // Substrate-local two-slot factories — peers to the sibling
    // `single_slot_*_probe` block above. Each composes
    // `single_slot_*_probe(a)` with `single_slot_*_probe(b)`
    // through per-field `Option::or` on the parent's tagged-union
    // slots, matching the shape every per-site `two_slot_X(a, b)`
    // helper across the four production parents already carries.
    // The ambiguity-primitive only requires that BOTH addressed
    // slots on the parent are populated; the inner spec's exact
    // field values are irrelevant to the two-slot ambiguity check.

    fn two_slot_intent_probe(
        a: crate::intent::IntentKind,
        b: crate::intent::IntentKind,
    ) -> crate::intent::Intent {
        let ia = single_slot_intent_probe(a);
        let ib = single_slot_intent_probe(b);
        crate::intent::Intent {
            nix: ia.nix.or(ib.nix),
            flux: ia.flux.or(ib.flux),
            lisp: ia.lisp.or(ib.lisp),
            container: ia.container.or(ib.container),
            aplicacao: ia.aplicacao.or(ib.aplicacao),
            guest: ia.guest.or(ib.guest),
        }
    }

    fn two_slot_encapsulation_kind_probe(
        a: crate::encapsulates::EncapsulationTarget,
        b: crate::encapsulates::EncapsulationTarget,
    ) -> crate::encapsulates::EncapsulationKind {
        let ka = single_slot_encapsulation_kind_probe(a);
        let kb = single_slot_encapsulation_kind_probe(b);
        crate::encapsulates::EncapsulationKind {
            existing_helm_release: ka.existing_helm_release.or(kb.existing_helm_release),
            existing_kustomization: ka.existing_kustomization.or(kb.existing_kustomization),
            bare_workload: ka.bare_workload.or(kb.bare_workload),
        }
    }

    fn two_slot_artifact_source_probe(
        a: crate::export::ArtifactKind,
        b: crate::export::ArtifactKind,
    ) -> crate::export::ArtifactSource {
        let sa = single_slot_artifact_source_probe(a);
        let sb = single_slot_artifact_source_probe(b);
        crate::export::ArtifactSource {
            receipts: sa.receipts.or(sb.receipts),
            test_report: sa.test_report.or(sb.test_report),
            process_snapshot: sa.process_snapshot.or(sb.process_snapshot),
            run_marker: sa.run_marker.or(sb.run_marker),
        }
    }

    fn two_slot_vector_channel_probe(
        a: crate::export::ChannelKind,
        b: crate::export::ChannelKind,
    ) -> crate::export::VectorChannel {
        let ca = single_slot_vector_channel_probe(a);
        let cb = single_slot_vector_channel_probe(b);
        crate::export::VectorChannel {
            http_event: ca.http_event.or(cb.http_event),
            nats_subject: ca.nats_subject.or(cb.nats_subject),
            stdout: ca.stdout.or(cb.stdout),
        }
    }

    /// The trait's `KIND_LIST` associated const IS the same
    /// `&'static str` the inherent `_LIST` constant publishes at
    /// each production site — pin identity via `std::ptr::eq` so a
    /// future silent copy (e.g. `const KIND_LIST: &'static str =
    /// "...literal...";` at the impl block) is caught here.
    #[test]
    fn production_tagged_union_kind_list_borrows_the_inherent_constant() {
        assert!(std::ptr::eq(
            <crate::intent::Intent as TaggedUnion>::KIND_LIST,
            crate::intent::INTENT_KIND_LIST,
        ));
        assert!(std::ptr::eq(
            <crate::encapsulates::EncapsulationKind as TaggedUnion>::KIND_LIST,
            crate::encapsulates::ENCAPSULATION_TARGET_LIST,
        ));
        assert!(std::ptr::eq(
            <crate::export::ArtifactSource as TaggedUnion>::KIND_LIST,
            crate::export::ARTIFACT_KIND_LIST,
        ));
        assert!(std::ptr::eq(
            <crate::export::VectorChannel as TaggedUnion>::KIND_LIST,
            crate::export::CHANNEL_KIND_LIST,
        ));
    }

    // -------------------------------------------------------------------
    // `TaggedUnion::variant` default method — substrate primitive every
    // production `.variant()` inherent method delegates to. Pin the
    // four-outcome truth table (Empty on all-none, Ambiguous on many,
    // Ok on exactly-one at every position) directly on the sibling-
    // shaped local parent + local kind + local variant scaffold, so a
    // regression on the default body's short-circuit or
    // ClosedSet::ALL iteration shape fails here — before any per-parent
    // inherent test surfaces the drift.
    // -------------------------------------------------------------------

    /// Every populated position across [`LocalKind::ALL`] resolves to
    /// its own [`LocalVariant`] arm through the default body's
    /// `resolve_or_err(<Kind as ClosedSet>::ALL.iter().copied()
    /// .map(|k| k.select(self)), KIND_LIST)` sweep. Pin every position
    /// so a regression that drifts the iteration order (or drops the
    /// `.iter().copied()` bridge to owned-`Copy` Kinds) fails at ONE
    /// substrate boundary rather than at four per-parent inherent test
    /// sites.
    #[test]
    fn tagged_union_default_variant_resolves_each_populated_slot() {
        let mut p = LocalParent {
            alpha: Some(11),
            ..Default::default()
        };
        assert_eq!(
            <LocalParent as TaggedUnion>::variant(&p).unwrap(),
            LocalVariant::Alpha(&11)
        );
        p = LocalParent {
            beta: Some(22),
            ..Default::default()
        };
        assert_eq!(
            <LocalParent as TaggedUnion>::variant(&p).unwrap(),
            LocalVariant::Beta(&22)
        );
        p = LocalParent {
            gamma: Some(33),
            ..Default::default()
        };
        assert_eq!(
            <LocalParent as TaggedUnion>::variant(&p).unwrap(),
            LocalVariant::Gamma(&33)
        );
    }

    /// A [`LocalParent`] with no populated slot resolves through the
    /// default body to a [`TaggedUnionError::empty`] carrier whose
    /// payload IS the trait's [`TaggedUnion::KIND_LIST`] constant —
    /// pin identity via [`std::ptr::eq`] so a regression that
    /// composes a fresh `&'static str` at the empty arm (instead of
    /// carrying the trait's constant verbatim) is caught here. This
    /// is the substrate-wide guarantee the four production sites'
    /// operator diagnostics depend on: a rename at
    /// `<Parent as TaggedUnion>::KIND_LIST` reaches the error surface
    /// intact through ONE `&'static str` handoff.
    #[test]
    fn tagged_union_default_variant_empty_carries_kind_list_by_pointer() {
        let empty = LocalParent::default();
        let err = <LocalParent as TaggedUnion>::variant(&empty).unwrap_err();
        match err {
            LocalParentError::Empty(list) => {
                assert!(
                    std::ptr::eq(list, <LocalParent as TaggedUnion>::KIND_LIST),
                    "TaggedUnion::variant default must carry KIND_LIST by pointer, not by re-composition",
                );
            }
            LocalParentError::Ambiguous => {
                panic!("expected Empty carrier, got Ambiguous");
            }
        }
    }

    /// A [`LocalParent`] with two populated slots resolves through
    /// the default body to a [`TaggedUnionError::ambiguous`] carrier —
    /// pin the Many arm at the substrate boundary so a regression
    /// that drops the short-circuit (or misroutes the Many arm to
    /// Empty) is caught here.
    #[test]
    fn tagged_union_default_variant_ambiguous_on_multiple_populated_slots() {
        let p = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: None,
        };
        assert_eq!(
            <LocalParent as TaggedUnion>::variant(&p).unwrap_err(),
            LocalParentError::Ambiguous
        );
    }

    /// Every one of the four production `.variant()` inherent methods
    /// dispatches through the trait's default body byte-identically —
    /// pin the delegation shape (inherent forwarder → trait default)
    /// on a probe per parent so a regression that copies the pre-lift
    /// hand-rolled `resolve_or_err(...)` body back into the inherent
    /// method (instead of the `<Self as TaggedUnion>::variant(self)`
    /// one-line delegation) reaches this substrate boundary before it
    /// reaches any operator diagnostic.
    #[test]
    fn every_production_inherent_variant_dispatches_through_trait_default() {
        use crate::encapsulates::{EncapsulationKind, EncapsulationKindError};
        use crate::export::{ArtifactError, ArtifactSource, ChannelError, VectorChannel};
        use crate::intent::{Intent, IntentError};

        // Intent: default of all-None resolves to Empty via the delegation.
        let i = Intent::default();
        match (i.variant(), <Intent as TaggedUnion>::variant(&i)) {
            (Err(IntentError::Empty(a)), Err(IntentError::Empty(b))) => assert!(
                std::ptr::eq(a, b),
                "Intent inherent and trait dispatch must return the same &'static str",
            ),
            (a, b) => panic!("Intent inherent/trait mismatch: inherent={a:?}, trait={b:?}"),
        }

        // EncapsulationKind: same Empty projection through both dispatch paths.
        let k = EncapsulationKind::default();
        match (k.variant(), <EncapsulationKind as TaggedUnion>::variant(&k)) {
            (Err(EncapsulationKindError::Empty(a)), Err(EncapsulationKindError::Empty(b))) => {
                assert!(
                std::ptr::eq(a, b),
                "EncapsulationKind inherent and trait dispatch must return the same &'static str",
            )
            }
            (a, b) => {
                panic!("EncapsulationKind inherent/trait mismatch: inherent={a:?}, trait={b:?}")
            }
        }

        // ArtifactSource: same Empty projection through both dispatch paths.
        let s = ArtifactSource::default();
        match (s.variant(), <ArtifactSource as TaggedUnion>::variant(&s)) {
            (Err(ArtifactError::Empty(a)), Err(ArtifactError::Empty(b))) => assert!(
                std::ptr::eq(a, b),
                "ArtifactSource inherent and trait dispatch must return the same &'static str",
            ),
            (a, b) => panic!("ArtifactSource inherent/trait mismatch: inherent={a:?}, trait={b:?}"),
        }

        // VectorChannel: same Empty projection through both dispatch paths.
        let c = VectorChannel::default();
        match (c.variant(), <VectorChannel as TaggedUnion>::variant(&c)) {
            (Err(ChannelError::Empty(a)), Err(ChannelError::Empty(b))) => assert!(
                std::ptr::eq(a, b),
                "VectorChannel inherent and trait dispatch must return the same &'static str",
            ),
            (a, b) => panic!("VectorChannel inherent/trait mismatch: inherent={a:?}, trait={b:?}"),
        }
    }

    // -------------------------------------------------------------------
    // `declare_tagged_union_impls!` macro — the three-block impl stanza
    // (inherent `.variant()` forwarder + `VariantSelector<Parent>` on
    // the Kind + `TaggedUnion` on the parent) as ONE authoring surface.
    // Pin the macro's shape against a sibling-shaped local family so a
    // regression on any of the three emitted blocks fails here before
    // it reaches the four production sites.
    // -------------------------------------------------------------------

    /// Local sibling-shaped Kind for the macro-emitted-impls test — a
    /// dedicated closed set so this test can't share substrate with the
    /// hand-rolled [`LocalKind`] block above. Uses
    /// [`tatara_closed_set::DeriveClosedSet`] so the macro's
    /// `TaggedUnion` bound (`Kind: ClosedSet + VariantSelector<Self>`)
    /// is satisfied through the derive.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_closed_set::DeriveClosedSet)]
    #[closed_set(via = "as_str", generate_unknown)]
    enum MacroLocalKind {
        Foo,
        Bar,
    }

    impl MacroLocalKind {
        const ALL: [Self; 2] = [Self::Foo, Self::Bar];
        const fn as_str(self) -> &'static str {
            match self {
                Self::Foo => "foo",
                Self::Bar => "bar",
            }
        }
        fn select<'a>(self, parent: &'a MacroLocalParent) -> Option<MacroLocalVariant<'a>> {
            match self {
                Self::Foo => parent.foo.as_ref().map(MacroLocalVariant::Foo),
                Self::Bar => parent.bar.as_ref().map(MacroLocalVariant::Bar),
            }
        }
    }

    /// Local sibling-shaped parent for the macro-emitted-impls test —
    /// distinct from [`LocalParent`] so the macro's emitted impls
    /// don't collide with the hand-rolled trait impls above.
    ///
    /// Derives [`serde::Serialize`] with `skip_serializing_if =
    /// "Option::is_none"` on every slot so the wire-format primitive
    /// [`assert_single_slot_key_matches_label`] can be exercised
    /// through the macro-emitted `TaggedUnion` impl path — pins the
    /// substrate-wide guarantee that a fifth sibling landing through
    /// [`declare_tagged_union_impls!`] picks up the wire-alignment
    /// check for free.
    #[derive(Default, serde::Serialize)]
    struct MacroLocalParent {
        #[serde(skip_serializing_if = "Option::is_none")]
        foo: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        bar: Option<u32>,
    }

    /// Borrowed-view of a populated slot on [`MacroLocalParent`] — the
    /// return type of the macro-emitted inherent `.variant()`.
    #[derive(Debug, PartialEq)]
    enum MacroLocalVariant<'a> {
        Foo(&'a u32),
        Bar(&'a u32),
    }

    impl VariantKind<MacroLocalKind> for MacroLocalVariant<'_> {
        fn variant_kind(&self) -> MacroLocalKind {
            match self {
                Self::Foo(_) => MacroLocalKind::Foo,
                Self::Bar(_) => MacroLocalKind::Bar,
            }
        }
    }

    crate::declare_tagged_union_error! {
        pub(super) MacroLocalError,
        empty = "macro-local parent has no variant set (one of {0} required)",
        ambiguous = "macro-local parent has multiple variants set; exactly one required",
    }

    /// Slash-joined kind list — literal peer of
    /// [`crate::intent::INTENT_KIND_LIST`] etc. that the macro's
    /// `KIND_LIST` associated const borrows verbatim.
    const MACRO_LOCAL_KIND_LIST: &str = "foo/bar";

    // ONE macro call emits: inherent `MacroLocalParent::variant`,
    // `impl VariantSelector<MacroLocalParent> for MacroLocalKind`, and
    // `impl TaggedUnion for MacroLocalParent`. The four production
    // sites bind through this exact same call shape.
    crate::declare_tagged_union_impls! {
        parent = MacroLocalParent,
        kind = MacroLocalKind,
        variant = MacroLocalVariant,
        error = MacroLocalError,
        kind_list = MACRO_LOCAL_KIND_LIST,
    }

    /// The macro-emitted `impl TaggedUnion` binds the (Kind, Error,
    /// KIND_LIST) triple exactly as a hand-rolled block would — pin
    /// the diagnostic-stability testkit primitive through the macro's
    /// output so a regression on any of the three associated items
    /// (say the macro pulling `KIND_LIST` from the wrong argument
    /// slot) fails here.
    #[test]
    fn macro_emitted_tagged_union_impl_binds_kind_list_coherently() {
        assert_kind_list_matches_closed_set::<MacroLocalParent>();
        assert!(std::ptr::eq(
            <MacroLocalParent as TaggedUnion>::KIND_LIST,
            MACRO_LOCAL_KIND_LIST,
        ));
    }

    /// The macro-emitted inherent `.variant()` forwarder dispatches
    /// through the trait default body — every populated slot resolves
    /// to its own [`MacroLocalVariant`] arm, all-none resolves to
    /// [`TaggedUnionError::empty`] carrying the trait's `KIND_LIST`
    /// by pointer, two-populated resolves to
    /// [`TaggedUnionError::ambiguous`]. The four production sites
    /// exercise the same four-outcome truth table through the same
    /// macro-emitted delegation shape.
    #[test]
    fn macro_emitted_inherent_variant_dispatches_the_four_outcome_truth_table() {
        // Foo populated.
        let p = MacroLocalParent {
            foo: Some(11),
            bar: None,
        };
        assert_eq!(p.variant().unwrap(), MacroLocalVariant::Foo(&11));

        // Bar populated.
        let p = MacroLocalParent {
            foo: None,
            bar: Some(22),
        };
        assert_eq!(p.variant().unwrap(), MacroLocalVariant::Bar(&22));

        // All none — Empty arm carries the trait's KIND_LIST value.
        // The by-pointer preservation across the trait default body is
        // pinned substrate-wide by
        // `tagged_union_default_variant_empty_carries_kind_list_by_pointer`
        // on the sibling hand-rolled `LocalParent`; this test only pins
        // that the macro-emitted `KIND_LIST = MACRO_LOCAL_KIND_LIST`
        // assignment reaches the operator diagnostic value-identically.
        let p = MacroLocalParent::default();
        match p.variant().unwrap_err() {
            MacroLocalError::Empty(list) => assert_eq!(list, MACRO_LOCAL_KIND_LIST),
            MacroLocalError::Ambiguous => panic!("expected Empty, got Ambiguous"),
        }

        // Two populated — Ambiguous.
        let p = MacroLocalParent {
            foo: Some(1),
            bar: Some(2),
        };
        assert_eq!(p.variant().unwrap_err(), MacroLocalError::Ambiguous);
    }

    /// The macro-emitted inherent `.has()` forwarder dispatches
    /// through the trait default body — the presence probe agrees
    /// with `Kind::select(&parent).is_some()` on the diagonal
    /// (populated slot AND matching Kind → `true`) and off the
    /// diagonal (populated slot BUT other Kind → `false`) for the
    /// same four-outcome truth table the macro-emitted `.variant()`
    /// covers. The four production sites bind through this exact
    /// same macro-emitted delegation shape; the substrate testkit
    /// primitive [`assert_has_matches_select`] sweeps this contract
    /// generically once each production Kind picks up the macro's
    /// output.
    #[test]
    fn macro_emitted_inherent_has_dispatches_the_presence_probe_diagonal() {
        // Foo populated → has(Foo) is true, has(Bar) is false.
        let p = MacroLocalParent {
            foo: Some(11),
            bar: None,
        };
        assert!(p.has(MacroLocalKind::Foo));
        assert!(!p.has(MacroLocalKind::Bar));

        // Bar populated → has(Bar) is true, has(Foo) is false.
        let p = MacroLocalParent {
            foo: None,
            bar: Some(22),
        };
        assert!(!p.has(MacroLocalKind::Foo));
        assert!(p.has(MacroLocalKind::Bar));

        // All none — every probe is false; no Empty carrier
        // allocation on this path (the presence-probe half of the
        // resolve contract deliberately elides diagnostic composition
        // when the caller only needs yes/no).
        let p = MacroLocalParent::default();
        assert!(!p.has(MacroLocalKind::Foo));
        assert!(!p.has(MacroLocalKind::Bar));

        // Two populated — has(k) is true for BOTH populated slots
        // (the probe is a per-slot projection, not the parent-wide
        // resolver — Ambiguous is a resolve outcome, not a presence
        // outcome).
        let p = MacroLocalParent {
            foo: Some(1),
            bar: Some(2),
        };
        assert!(p.has(MacroLocalKind::Foo));
        assert!(p.has(MacroLocalKind::Bar));
    }

    /// The macro-emitted inherent `.find()` forwarder dispatches
    /// through the trait default body — every populated slot resolves
    /// to `Some(matching-borrow)`, empty slots to `None`, and the
    /// composition law `parent.has(k) == parent.find(k).is_some()`
    /// holds at every arm of the four-outcome truth table. Additional
    /// pointer-identity pin: the borrowed reference returned by
    /// `p.find(k)` on a populated slot IS the same reference that
    /// `<Kind>::select(k, &p)` returns — a regression that inlines a
    /// divergent projection body at the macro's emitted forwarder
    /// (rather than reaching the trait's `<Self as
    /// TaggedUnion>::find(self, kind)` one-line delegation) is caught
    /// here.
    #[test]
    fn macro_emitted_inherent_find_dispatches_the_presence_probe_diagonal() {
        // Foo populated → find(Foo) borrows the inner ref, find(Bar)
        // is None, and `has` agrees with `find(...).is_some()` on
        // both arms.
        let p = MacroLocalParent {
            foo: Some(77),
            bar: None,
        };
        match p.find(MacroLocalKind::Foo) {
            Some(MacroLocalVariant::Foo(v)) => {
                assert_eq!(*v, 77, "find must borrow the populated inner");
                assert_eq!(
                    p.has(MacroLocalKind::Foo),
                    true,
                    "composition law: has must agree with find(...).is_some() on populated slot",
                );
                // Pointer-identity check: `find` delegates to
                // `kind.select(self)` byte-identically. The returned
                // borrow IS the borrow `select` returns.
                let via_select = MacroLocalKind::Foo.select(&p).unwrap();
                match via_select {
                    MacroLocalVariant::Foo(w) => assert!(
                        std::ptr::eq(v, w),
                        "macro-emitted find must return the SAME borrow as VariantSelector::select",
                    ),
                    MacroLocalVariant::Bar(_) => {
                        panic!(
                            "VariantSelector::select disagreed with find on the populated Foo slot"
                        )
                    }
                }
            }
            other => panic!("expected Foo populated, got {other:?}"),
        }
        assert!(p.find(MacroLocalKind::Bar).is_none());
        assert_eq!(
            p.has(MacroLocalKind::Bar),
            false,
            "composition law: has must agree with find(...).is_some() on empty slot",
        );

        // All none — find returns None for every kind; has agrees.
        let p = MacroLocalParent::default();
        for kind in MacroLocalKind::ALL {
            assert!(p.find(kind).is_none());
            assert_eq!(
                p.has(kind),
                false,
                "composition law on empty parent: has must equal find(...).is_some()",
            );
        }

        // Two populated — find(k) is Some for BOTH populated slots
        // (the widened primitive is a per-slot projection, not the
        // parent-wide resolver — Ambiguous is a resolve outcome, not
        // a find outcome).
        let p = MacroLocalParent {
            foo: Some(1),
            bar: Some(2),
        };
        assert!(p.find(MacroLocalKind::Foo).is_some());
        assert!(p.find(MacroLocalKind::Bar).is_some());
    }

    /// The macro-emitted `VariantSelector` impl's `select` body
    /// delegates to the Kind's inherent `<Kind>::select(self, parent)`
    /// — pin the delegation via `std::ptr::eq` on the returned
    /// borrowed view so a regression that inlines a divergent select
    /// body (rather than reaching the inherent method) is caught here.
    #[test]
    fn macro_emitted_variant_selector_delegates_to_inherent_select() {
        let p = MacroLocalParent {
            foo: Some(7),
            bar: None,
        };
        // Trait-dispatched select projects through the macro-emitted body.
        let via_trait =
            <MacroLocalKind as VariantSelector<MacroLocalParent>>::select(MacroLocalKind::Foo, &p)
                .unwrap();
        // Inherent select projects through the direct impl.
        let via_inherent = MacroLocalKind::Foo.select(&p).unwrap();
        match (via_trait, via_inherent) {
            (MacroLocalVariant::Foo(a), MacroLocalVariant::Foo(b)) => {
                assert!(
                    std::ptr::eq(a, b),
                    "macro-emitted VariantSelector::select must delegate to <Kind>::select — same borrow, not a copy",
                );
            }
            _ => panic!("expected Foo arm on both dispatch paths"),
        }
    }

    /// The trait's default body sweeps `<Kind as ClosedSet>::ALL` in
    /// declaration order — pin the iteration order against the
    /// production `Kind::ALL` inherent const on every implementor so
    /// a regression on `DeriveClosedSet`'s ALL-projection (or a
    /// silent reorder of the enum's variant declarations that drifts
    /// only ONE of the two arrays) fails at ONE substrate boundary.
    #[test]
    fn every_production_kind_closedset_all_matches_inherent_all() {
        use crate::encapsulates::EncapsulationTarget;
        use crate::export::{ArtifactKind, ChannelKind};
        use crate::intent::IntentKind;

        assert_eq!(
            <IntentKind as tatara_closed_set::ClosedSet>::ALL,
            IntentKind::ALL.as_slice(),
        );
        assert_eq!(
            <EncapsulationTarget as tatara_closed_set::ClosedSet>::ALL,
            EncapsulationTarget::ALL.as_slice(),
        );
        assert_eq!(
            <ArtifactKind as tatara_closed_set::ClosedSet>::ALL,
            ArtifactKind::ALL.as_slice(),
        );
        assert_eq!(
            <ChannelKind as tatara_closed_set::ClosedSet>::ALL,
            ChannelKind::ALL.as_slice(),
        );
    }

    // -------------------------------------------------------------------
    // `VariantKind<K>` trait — reverse projection from a borrowed-variant
    // view back into its addressing Kind, and `assert_variant_round_trip`
    // as the substrate testkit primitive that composes it with
    // `VariantSelector::select` on the populated side. Pin the four-arm
    // truth table (every position round-trips through select→variant_kind
    // AND through variant()→variant_kind) directly on the sibling-shaped
    // local scaffold, so a regression on either projection or on the
    // resolver default body fails here — before any per-parent inherent
    // test surfaces the drift.
    // -------------------------------------------------------------------

    /// Every populated position across [`LocalKind::ALL`] round-trips
    /// through both `select→variant_kind` AND `variant()→variant_kind`
    /// on the sibling-shaped local scaffold. Pins the substrate
    /// primitive's four-arm truth table at ONE boundary — a regression
    /// on either projection direction (or on the resolver default
    /// short-circuit / iteration order) fails here before any per-parent
    /// inherent test surfaces the drift.
    #[test]
    fn assert_variant_round_trip_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_variant_round_trip::<LocalParent, _>(make_local);
    }

    /// The testkit primitive is a `#[track_caller]` compound-lift: a
    /// factory that fails to populate the addressed slot fails at the
    /// caller's site with a labeled panic message, not silently. Pin
    /// the failing case with a deliberately empty parent factory so a
    /// regression that drops the "select must return Some" check
    /// fails-loudly here — the missing-slot arm is the substrate
    /// primitive's first failure mode.
    #[test]
    #[should_panic(expected = "VariantSelector::select must return Some for populated slot")]
    fn assert_variant_round_trip_rejects_factory_that_leaves_slot_empty() {
        // Factory that returns an all-empty parent regardless of k —
        // every `k.select(&parent)` returns None, so the primitive
        // panics at the "must return Some" arm.
        fn empty_factory(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_variant_round_trip::<LocalParent, _>(empty_factory);
    }

    // -------------------------------------------------------------------
    // `TaggedUnion::find` — the widened peer of `TaggedUnion::has` on the
    // presence-probe algebra. Pin every arm of the four-outcome truth
    // table (empty parent → None, populated-diagonal → Some(matching
    // borrow), populated-off-diagonal → None, two-populated → Some for
    // BOTH populated slots) directly on the sibling-shaped local scaffold
    // AND on the macro-emitted inherent surface. A regression on the
    // default body's `kind.select(self)` delegation (or on the emitted
    // inherent forwarder's `<Self as TaggedUnion>::find(self, kind)`
    // one-line body) fails here before it reaches any of the four
    // production sites.
    // -------------------------------------------------------------------

    /// EMPTY-PARENT pin — a default [`LocalParent`] returns `None` at
    /// `find` for EVERY [`LocalKind`], sweeping `ClosedSet::ALL` so a
    /// new variant added without a matching arm in the primitive
    /// surfaces at rustc's exhaustiveness gate on the ALL literal
    /// rather than as a silent false-positive at every downstream
    /// consumer composing this primitive.
    #[test]
    fn tagged_union_default_find_returns_none_on_empty_parent_for_every_kind() {
        let empty = LocalParent::default();
        for kind in <LocalKind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            assert!(
                <LocalParent as TaggedUnion>::find(&empty, kind).is_none(),
                "empty parent must return None at find for {kind:?}",
            );
        }
    }

    /// DELEGATION pin — every populated position across
    /// [`LocalKind::ALL`] returns `Some(matching-borrow)` at `find`,
    /// AND the returned borrowed view carries the SAME reference as
    /// `probed.select(&parent).unwrap()` (byte-identical delegation:
    /// `find` IS `kind.select(self)`, not a re-projection).
    /// Composition-law pin: `has(k) == find(k).is_some()` on both
    /// diagonal (populated slot AND matching Kind → true) and
    /// off-diagonal (populated slot BUT other Kind → false).
    #[test]
    fn tagged_union_default_find_delegates_to_select_across_every_kind() {
        for populated in <LocalKind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let parent = match populated {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(101),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(202),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(303),
                    ..Default::default()
                },
            };
            for probed in <LocalKind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .copied()
            {
                let via_find = <LocalParent as TaggedUnion>::find(&parent, probed);
                let via_select = probed.select(&parent);
                assert_eq!(
                    via_find.is_some(),
                    via_select.is_some(),
                    "find drifted from select — populated={populated:?} probed={probed:?}",
                );
                assert_eq!(
                    parent.has(probed),
                    via_find.is_some(),
                    "has drifted from find(k).is_some() — populated={populated:?} probed={probed:?}",
                );
                if let Some(v) = via_find {
                    assert_eq!(
                        <LocalVariant<'_> as VariantKind<LocalKind>>::variant_kind(&v),
                        probed,
                        "find→variant_kind round-trip failed — populated={populated:?} probed={probed:?}",
                    );
                    // Populated iff probed == populated (single-slot
                    // parent) — off-diagonal arms return None above
                    // and never reach this Some-branch.
                    assert_eq!(
                        probed, populated,
                        "off-diagonal probe should have returned None at find",
                    );
                }
            }
        }
    }

    /// TWO-POPULATED pin — a parent with two populated slots returns
    /// `Some(matching-borrow)` at `find` for BOTH populated Kinds
    /// (unlike `variant()` which resolves to `Ambiguous`), and `None`
    /// for the empty third Kind. Locks the presence-probe axis of the
    /// widened primitive against a regression that inlined the
    /// resolver's short-circuit body into `find` (silently narrowing
    /// two populated to Ambiguous instead of a per-slot borrow).
    #[test]
    fn tagged_union_default_find_projects_per_slot_on_multi_populated_parent() {
        let parent = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: None,
        };
        assert!(
            <LocalParent as TaggedUnion>::find(&parent, LocalKind::Alpha).is_some(),
            "find must project Alpha slot in a two-populated parent",
        );
        assert!(
            <LocalParent as TaggedUnion>::find(&parent, LocalKind::Beta).is_some(),
            "find must project Beta slot in a two-populated parent",
        );
        assert!(
            <LocalParent as TaggedUnion>::find(&parent, LocalKind::Gamma).is_none(),
            "find must return None for the empty Gamma slot",
        );
    }

    /// `assert_find_agrees_with_has` testkit accepts the coherent
    /// local scaffold — sweeping every `(populated, probed)` pair
    /// through the three sub-assertions (find↔has, find↔select,
    /// diagonal round-trip). A regression on any of the three
    /// composition laws fails at the substrate primitive's
    /// `#[track_caller]` boundary here rather than at four per-parent
    /// production sites downstream.
    #[test]
    fn assert_find_agrees_with_has_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_find_agrees_with_has::<LocalParent, _>(make_local);
    }

    // -------------------------------------------------------------------
    // `TaggedUnion::populated_kinds` default method + the
    // closed-set-inversion refinement's per-parent semantics — pin the
    // three arms (empty parent → empty vec, single-slot → vec![k],
    // multi-populated → vec[a..b] in ClosedSet::ALL order) directly on
    // the sibling-shaped `LocalParent` scaffold. Peer of the boundary-
    // side `ConditionSliceExt::distinct_kinds` primitive's three-arm
    // pin on the slice-level presence-probe axis.
    // -------------------------------------------------------------------

    /// EMPTY parent — the default body's `ALL.filter(has).collect()`
    /// sweep yields an empty vec when no slot is populated. Pins the
    /// zero-cardinality arm: a regression that mis-composed the
    /// `ALL.iter()` bridge (short-circuiting past the empty case),
    /// returned a non-empty sentinel on empty input, or leaked stale
    /// closed-set entries as false-positive members fails HERE at the
    /// substrate boundary.
    #[test]
    fn tagged_union_default_populated_kinds_returns_empty_vec_on_empty_parent() {
        let empty = LocalParent::default();
        assert!(
            <LocalParent as TaggedUnion>::populated_kinds(&empty).is_empty(),
            "populated_kinds() must return empty Vec when no slot is populated",
        );
    }

    /// SINGLE-SLOT parent — the default body sweeps `ClosedSet::ALL`
    /// with `has(k)` and collects the singleton `[k]` for each
    /// single-populated arrangement. Pins the length-1 arm's
    /// cardinality (must be exactly 1) AND ordering (the addressed
    /// kind's own position in `ClosedSet::ALL`) at ONE `assert_eq!`
    /// per kind — a regression that projected the wrong Kind, drifted
    /// the walk from `has` to a divergent projection, or paired two
    /// kinds together on a single-slot input fails HERE per addressed
    /// kind. Sweeps every `LocalKind::ALL` entry so no per-variant
    /// specialization can silently drop the check.
    #[test]
    fn tagged_union_default_populated_kinds_returns_single_element_vec_per_variant() {
        for populated in <LocalKind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let parent = match populated {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            };
            assert_eq!(
                <LocalParent as TaggedUnion>::populated_kinds(&parent),
                vec![populated],
                "single-slot parent must return exactly [{populated:?}] on populated_kinds",
            );
        }
    }

    /// MULTI-POPULATED parent — the default body yields the canonical
    /// `ClosedSet::ALL`-ordered pair `[Alpha, Beta]` for a two-slot
    /// arrangement populated in the CONSTRUCTION order `(Beta, Alpha)`.
    /// Pins the walk order arm: a regression that yielded slot-
    /// construction-order (`[Beta, Alpha]`) instead of canonical
    /// `ALL`-order fails HERE at the equality assert. Also pins the
    /// non-short-circuiting arm — a regression that inlined the
    /// resolver's short-circuit body into `populated_kinds` (silently
    /// narrowing two populated to a length-1 vec containing the first
    /// slot) fails at the length side of the equality.
    #[test]
    fn tagged_union_default_populated_kinds_walks_canonical_all_order_on_multi_populated_parent() {
        let parent = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: None,
        };
        assert_eq!(
            <LocalParent as TaggedUnion>::populated_kinds(&parent),
            vec![LocalKind::Alpha, LocalKind::Beta],
            "multi-populated parent must return canonical ClosedSet::ALL-ordered kinds",
        );
    }

    /// SATURATED parent — every slot populated returns
    /// `LocalKind::ALL.to_vec()` exactly. Pins the full-closed-set-
    /// coverage arm: a `[1..]` or `[..ALL.len() - 1]` walk bug that
    /// silently truncated the swept range at either end surfaces at
    /// the equality assert here.
    #[test]
    fn tagged_union_default_populated_kinds_covers_full_closed_set_on_saturated_parent() {
        let saturated = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: Some(3),
        };
        assert_eq!(
            <LocalParent as TaggedUnion>::populated_kinds(&saturated),
            <LocalKind as tatara_closed_set::ClosedSet>::ALL.to_vec(),
            "saturated parent must return ClosedSet::ALL.to_vec() on populated_kinds",
        );
    }

    /// `assert_populated_kinds_matches_has` testkit accepts the
    /// coherent local scaffold — sweeping every `(populated, probed)`
    /// pair through the three sub-assertions (per-kind membership,
    /// canonical `ALL`-filter equality, single-slot diagonal). A
    /// regression on any of the three composition laws fails at the
    /// substrate primitive's `#[track_caller]` boundary here rather
    /// than at four per-parent production sites downstream.
    #[test]
    fn assert_populated_kinds_matches_has_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_populated_kinds_matches_has::<LocalParent, _>(make_local);
    }

    /// A factory that yields an all-empty parent (so
    /// `populated_kinds()` returns `[]`) MUST fail-loudly at the
    /// caller's site through the primitive's single-slot diagonal
    /// arm — the empty vec does not equal `vec![populated]` for the
    /// swept `populated` kind. Pin the diagonal-arm failure mode so
    /// a regression that silently succeeded on an all-empty factory
    /// (e.g. the primitive was refactored to skip the diagonal
    /// assert on `kinds.is_empty()`) is caught here.
    #[test]
    #[should_panic(expected = "must return vec![Alpha] exactly")]
    fn assert_populated_kinds_matches_has_rejects_factory_that_populates_no_slots() {
        fn empty_factory(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_populated_kinds_matches_has::<LocalParent, _>(empty_factory);
    }

    /// A factory that yields a two-slot parent (so
    /// `populated_kinds()` returns `[k1, k2]` for TWO populated
    /// slots on a supposedly single-slot factory) MUST fail-loudly at
    /// the caller's site through the primitive's single-slot diagonal
    /// arm — the length-2 vec does not equal `vec![populated]`. Pin
    /// the diagonal-arm cardinality failure mode so a regression that
    /// silently succeeded on a broken factory (populating both the
    /// addressed slot AND an extra one) is caught here.
    #[test]
    #[should_panic(expected = "must return vec![Alpha] exactly")]
    fn assert_populated_kinds_matches_has_rejects_factory_that_populates_extra_slot() {
        fn always_pair(k: LocalKind) -> LocalParent {
            let mut p = LocalParent {
                gamma: Some(99),
                ..Default::default()
            };
            match k {
                LocalKind::Alpha => p.alpha = Some(11),
                LocalKind::Beta => p.beta = Some(22),
                LocalKind::Gamma => p.gamma = Some(33),
            }
            p
        }
        assert_populated_kinds_matches_has::<LocalParent, _>(always_pair);
    }

    /// `assert_populated_kinds_across_pairs` testkit accepts the
    /// coherent local scaffold — sweeping every off-diagonal `(a, b)`
    /// pair through the three sub-assertions (cardinality-2,
    /// per-kind membership, canonical `ALL`-filter equality). A
    /// regression on any of the three composition laws (or on the
    /// diagonal-skip) fails at the substrate primitive's
    /// `#[track_caller]` boundary here rather than at four per-parent
    /// production sites downstream.
    #[test]
    fn assert_populated_kinds_across_pairs_accepts_coherent_local_impl() {
        fn two_local(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(11),
                    LocalKind::Beta => p.beta = Some(22),
                    LocalKind::Gamma => p.gamma = Some(33),
                }
            }
            p
        }
        assert_populated_kinds_across_pairs::<LocalParent, _>(two_local);
    }

    /// A two-slot factory that yields a single-populated parent (so
    /// `populated_kinds()` returns `[k1]` for a two-slot input) MUST
    /// fail-loudly at the caller's site through the primitive's
    /// cardinality-2 arm — the length-1 vec does not satisfy
    /// `kinds.len() == 2`. Pin the cardinality-arm failure mode so a
    /// regression that silently succeeded on a broken factory
    /// (populating only the first of the two addressed slots) is
    /// caught here.
    #[test]
    #[should_panic(expected = "must return exactly two populated kinds")]
    fn assert_populated_kinds_across_pairs_rejects_factory_that_populates_only_one_slot() {
        fn single_only(a: LocalKind, _: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            match a {
                LocalKind::Alpha => p.alpha = Some(11),
                LocalKind::Beta => p.beta = Some(22),
                LocalKind::Gamma => p.gamma = Some(33),
            }
            p
        }
        assert_populated_kinds_across_pairs::<LocalParent, _>(single_only);
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the single-slot closed-set-inversion
    /// primitive `assert_populated_kinds_matches_has` coherently — every
    /// per-site `single_slot_X(k)` factory produces a parent whose
    /// `populated_kinds()` equals `vec![k]` and whose per-kind
    /// composition law `populated_kinds().contains(&k) == has(k)` holds
    /// for every `k ∈ ClosedSet::ALL`. Sweep every production
    /// implementor at ONE substrate boundary so a regression that
    /// drifts a production site's `single_slot_X` factory OR the
    /// default `populated_kinds` body (a specialization that
    /// short-circuited, drifted the walk order, or returned duplicates)
    /// fails BOTH at any future per-crate test site AND at this
    /// substrate-wide sweep.
    #[test]
    fn every_production_tagged_union_binds_through_the_populated_kinds_testkit_primitive() {
        assert_populated_kinds_matches_has::<crate::intent::Intent, _>(single_slot_intent_probe);
        assert_populated_kinds_matches_has::<crate::encapsulates::EncapsulationKind, _>(
            single_slot_encapsulation_kind_probe,
        );
        assert_populated_kinds_matches_has::<crate::export::ArtifactSource, _>(
            single_slot_artifact_source_probe,
        );
        assert_populated_kinds_matches_has::<crate::export::VectorChannel, _>(
            single_slot_vector_channel_probe,
        );
    }

    /// Peer of
    /// `every_production_tagged_union_binds_through_the_populated_kinds_testkit_primitive`
    /// on the two-slot ambiguous-parent side — every production
    /// `.variant()` parent binds through the pair primitive
    /// `assert_populated_kinds_across_pairs` coherently, so a
    /// regression that inlined the resolver's short-circuit body into
    /// `populated_kinds` on any production site (silently narrowing
    /// two populated slots to a length-1 vec) fails at ONE substrate
    /// boundary across all four parents.
    #[test]
    fn every_production_tagged_union_binds_through_the_populated_kinds_pair_testkit_primitive() {
        assert_populated_kinds_across_pairs::<crate::intent::Intent, _>(two_slot_intent_probe);
        assert_populated_kinds_across_pairs::<crate::encapsulates::EncapsulationKind, _>(
            two_slot_encapsulation_kind_probe,
        );
        assert_populated_kinds_across_pairs::<crate::export::ArtifactSource, _>(
            two_slot_artifact_source_probe,
        );
        assert_populated_kinds_across_pairs::<crate::export::VectorChannel, _>(
            two_slot_vector_channel_probe,
        );
    }

    // -------------------------------------------------------------------
    // `TaggedUnion::populated_kind_count` — scalar cardinality refinement
    // on the closed-set-inversion axis. Pin the three arms (empty parent
    // → 0, single-slot → 1, multi-populated → N) directly on the sibling-
    // shaped `LocalParent` scaffold and the composition law
    // `populated_kind_count() == populated_kinds().len()` at the substrate
    // testkit `assert_populated_kind_count_matches_populated_kinds`. Peer
    // of the widened primitive `populated_kinds` (see the block above);
    // the scalar projection collapses the widened Vec to its length
    // without allocating.
    // -------------------------------------------------------------------

    /// EMPTY parent — the default body's `ALL.filter(has).count()`
    /// sweep yields `0` when no slot is populated. Pins the zero-
    /// cardinality arm: a regression that mis-composed the
    /// `ALL.iter()` bridge (short-circuiting past the empty case),
    /// returned a non-zero sentinel on empty input, or leaked stale
    /// closed-set entries as false-positive members fails HERE at the
    /// substrate boundary.
    #[test]
    fn tagged_union_default_populated_kind_count_returns_zero_on_empty_parent() {
        let empty = LocalParent::default();
        assert_eq!(
            <LocalParent as TaggedUnion>::populated_kind_count(&empty),
            0,
            "populated_kind_count() must return 0 when no slot is populated",
        );
    }

    /// SINGLE-SLOT parent — the default body sweeps `ClosedSet::ALL`
    /// with `has(k)` and counts the singleton `1` for each single-
    /// populated arrangement. Pins the length-1 arm's cardinality at
    /// ONE `assert_eq!` per kind — a regression that projected the
    /// wrong Kind, drifted the walk from `has` to a divergent
    /// projection, or paired two kinds together on a single-slot input
    /// fails HERE per addressed kind. Sweeps every `LocalKind::ALL`
    /// entry so no per-variant specialization can silently drop the
    /// check.
    #[test]
    fn tagged_union_default_populated_kind_count_returns_one_per_single_slot_variant() {
        for populated in <LocalKind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let parent = match populated {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            };
            assert_eq!(
                <LocalParent as TaggedUnion>::populated_kind_count(&parent),
                1,
                "single-slot parent must return 1 on populated_kind_count for {populated:?}",
            );
        }
    }

    /// MULTI-POPULATED parent — the default body yields `2` for a two-
    /// slot arrangement, `3` for a fully-saturated three-slot parent.
    /// Pins the non-short-circuiting arm — a regression that inlined
    /// the resolver's short-circuit body into `populated_kind_count`
    /// (silently narrowing two populated to `1`) fails HERE at the
    /// equality assert.
    #[test]
    fn tagged_union_default_populated_kind_count_walks_full_closed_set_on_multi_populated_parent() {
        let two = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: None,
        };
        assert_eq!(
            <LocalParent as TaggedUnion>::populated_kind_count(&two),
            2,
            "two-populated parent must return 2 on populated_kind_count",
        );
        let saturated = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: Some(3),
        };
        assert_eq!(
            <LocalParent as TaggedUnion>::populated_kind_count(&saturated),
            3,
            "saturated parent must return LocalKind::ALL.len() on populated_kind_count",
        );
    }

    /// Composition law `populated_kind_count() == populated_kinds().len()`
    /// binds the scalar cardinality projection to the widened primitive
    /// across every `ClosedSet::ALL × {empty, single_slot, two_slot,
    /// saturated}` combination. Pins the byte-identity of the two
    /// projections on the empty / single / multi / saturated arms — a
    /// regression that overrode `populated_kind_count` with an
    /// off-by-one walk, a `find(k).is_none()`-inverted body (returning
    /// the ABSENT count), or a divergent short-circuit fails HERE at
    /// the equality assert.
    #[test]
    fn tagged_union_default_populated_kind_count_matches_populated_kinds_len() {
        let arrangements: [LocalParent; 4] = [
            LocalParent::default(),
            LocalParent {
                alpha: Some(1),
                ..Default::default()
            },
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: None,
            },
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            },
        ];
        for (idx, parent) in arrangements.iter().enumerate() {
            assert_eq!(
                <LocalParent as TaggedUnion>::populated_kind_count(parent),
                <LocalParent as TaggedUnion>::populated_kinds(parent).len(),
                "populated_kind_count() must equal populated_kinds().len() for arrangement idx {idx}",
            );
        }
    }

    /// `assert_populated_kind_count_matches_populated_kinds` testkit
    /// accepts the coherent local scaffold — sweeping every populated
    /// kind through the two sub-assertions (composition law
    /// `count == kinds.len()` + single-slot diagonal `count == 1`). A
    /// regression on either composition law fails at the substrate
    /// primitive's `#[track_caller]` boundary here rather than at four
    /// per-parent production sites downstream.
    #[test]
    fn assert_populated_kind_count_matches_populated_kinds_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_populated_kind_count_matches_populated_kinds::<LocalParent, _>(make_local);
    }

    /// A factory that yields an all-empty parent (so
    /// `populated_kind_count()` returns `0`) MUST fail-loudly at the
    /// caller's site through the primitive's single-slot diagonal arm
    /// — the `0` cardinality does not satisfy `count == 1` on the
    /// swept `populated` kind. Pin the diagonal-arm failure mode so a
    /// regression that silently succeeded on an all-empty factory
    /// (e.g. the primitive was refactored to skip the diagonal assert
    /// on `count == 0`) is caught here.
    #[test]
    #[should_panic(expected = "must equal 1 exactly (well-formed arm cardinality)")]
    fn assert_populated_kind_count_matches_populated_kinds_rejects_empty_factory() {
        fn empty_factory(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_populated_kind_count_matches_populated_kinds::<LocalParent, _>(empty_factory);
    }

    /// A factory that yields a two-slot parent (so
    /// `populated_kind_count()` returns `2` on a supposedly single-slot
    /// factory) MUST fail-loudly at the caller's site through the
    /// primitive's single-slot diagonal arm — the `2` cardinality does
    /// not satisfy `count == 1`. Pin the diagonal-arm cardinality
    /// failure mode so a regression that silently succeeded on a
    /// broken factory (populating both the addressed slot AND an extra
    /// one) is caught here.
    #[test]
    #[should_panic(expected = "must equal 1 exactly (well-formed arm cardinality)")]
    fn assert_populated_kind_count_matches_populated_kinds_rejects_two_slot_factory() {
        fn always_pair(k: LocalKind) -> LocalParent {
            let mut p = LocalParent {
                gamma: Some(99),
                ..Default::default()
            };
            match k {
                LocalKind::Alpha => p.alpha = Some(11),
                LocalKind::Beta => p.beta = Some(22),
                LocalKind::Gamma => p.gamma = Some(33),
            }
            p
        }
        assert_populated_kind_count_matches_populated_kinds::<LocalParent, _>(always_pair);
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the scalar-cardinality primitive
    /// `assert_populated_kind_count_matches_populated_kinds` coherently
    /// — every per-site `single_slot_X(k)` factory produces a parent
    /// whose `populated_kind_count()` equals `1` AND whose composition
    /// law `count == populated_kinds().len()` holds. Sweep every
    /// production implementor at ONE substrate boundary so a regression
    /// that drifts a production site's `single_slot_X` factory OR the
    /// default `populated_kind_count` body (a specialization that
    /// short-circuited, drifted the walk order, or double-counted a
    /// slot) fails BOTH at any future per-crate test site AND at this
    /// substrate-wide sweep.
    #[test]
    fn every_production_tagged_union_binds_through_the_populated_kind_count_testkit_primitive() {
        assert_populated_kind_count_matches_populated_kinds::<crate::intent::Intent, _>(
            single_slot_intent_probe,
        );
        assert_populated_kind_count_matches_populated_kinds::<
            crate::encapsulates::EncapsulationKind,
            _,
        >(single_slot_encapsulation_kind_probe);
        assert_populated_kind_count_matches_populated_kinds::<crate::export::ArtifactSource, _>(
            single_slot_artifact_source_probe,
        );
        assert_populated_kind_count_matches_populated_kinds::<crate::export::VectorChannel, _>(
            single_slot_vector_channel_probe,
        );
    }

    // -------------------------------------------------------------------
    // `TaggedUnion::missing_kinds` default method + the
    // closed-set-COMPLEMENT refinement's per-parent semantics — pin the
    // three arms (empty parent → full closed set, single-slot → ALL \
    // {k} in canonical order, saturated → empty vec) directly on the
    // sibling-shaped `LocalParent` scaffold. Peer of the boundary-side
    // `ConditionSliceExt::missing_kinds` primitive's three-arm pin on
    // the slice-level presence-probe axis; closed-set-COMPLEMENT peer
    // of the parent-level `populated_kinds` primitive above.
    // -------------------------------------------------------------------

    /// EMPTY parent — the default body's `ALL.filter(!has).collect()`
    /// sweep yields the FULL `ClosedSet::ALL` vec when no slot is
    /// populated (every kind is missing). Pins the full-cardinality
    /// arm: a regression that mis-composed the `ALL.iter()` bridge
    /// (short-circuiting past the empty case), inverted the negation
    /// (returning `populated_kinds`), or dropped closed-set entries as
    /// false-negative absences fails HERE at the substrate boundary.
    #[test]
    fn tagged_union_default_missing_kinds_returns_full_closed_set_on_empty_parent() {
        let empty = LocalParent::default();
        assert_eq!(
            <LocalParent as TaggedUnion>::missing_kinds(&empty),
            <LocalKind as tatara_closed_set::ClosedSet>::ALL.to_vec(),
            "missing_kinds() must return ClosedSet::ALL when no slot is populated",
        );
    }

    /// SINGLE-SLOT parent — the default body sweeps `ClosedSet::ALL`
    /// with `!has(k)` and collects `ALL \ {populated}` for each
    /// single-populated arrangement. Pins the length-(ALL.len()-1)
    /// arm's cardinality AND ordering (canonical `ClosedSet::ALL`
    /// order, `populated` absent) at ONE `assert_eq!` per kind — a
    /// regression that inverted the negation (returning `vec![populated]`
    /// instead of `ALL \ {populated}`) fails HERE per addressed kind.
    #[test]
    fn tagged_union_default_missing_kinds_returns_complement_per_variant() {
        for populated in <LocalKind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let parent = match populated {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            };
            let expected: Vec<LocalKind> = <LocalKind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .copied()
                .filter(|k| *k != populated)
                .collect();
            assert_eq!(
                <LocalParent as TaggedUnion>::missing_kinds(&parent),
                expected,
                "single-slot parent must return ClosedSet::ALL \\ {{{populated:?}}} on missing_kinds",
            );
        }
    }

    /// SATURATED parent — every slot populated returns an empty vec on
    /// `missing_kinds`. Pins the zero-cardinality arm on the complement
    /// side (mirror of `populated_kinds` returning `ALL.to_vec()` on
    /// the saturated arm).
    #[test]
    fn tagged_union_default_missing_kinds_returns_empty_vec_on_saturated_parent() {
        let saturated = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: Some(3),
        };
        assert!(
            <LocalParent as TaggedUnion>::missing_kinds(&saturated).is_empty(),
            "saturated parent must return empty Vec on missing_kinds",
        );
    }

    /// Partition law binding `populated_kinds` and `missing_kinds` on
    /// every `LocalParent` arrangement: every `k ∈ ClosedSet::ALL`
    /// lives on EXACTLY ONE side of the partition (populated OR
    /// missing, never both, never neither). Pins the compound-lift's
    /// most-load-bearing invariant at ONE `assert!` per (arrangement,
    /// kind) pair — a regression that returned overlapping or
    /// disjoint-but-incomplete sets fails HERE at the XOR arm.
    #[test]
    fn tagged_union_default_populated_kinds_and_missing_kinds_partition_the_closed_set() {
        let arrangements: [LocalParent; 4] = [
            LocalParent::default(),
            LocalParent {
                alpha: Some(1),
                ..Default::default()
            },
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: None,
            },
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            },
        ];
        for (idx, parent) in arrangements.iter().enumerate() {
            let populated = <LocalParent as TaggedUnion>::populated_kinds(parent);
            let missing = <LocalParent as TaggedUnion>::missing_kinds(parent);
            for &k in <LocalKind as tatara_closed_set::ClosedSet>::ALL.iter() {
                let in_populated = populated.contains(&k);
                let in_missing = missing.contains(&k);
                assert!(
                    in_populated ^ in_missing,
                    "arrangement idx {idx} — {k:?} must live on exactly one side of (populated, missing), got in_populated={in_populated} in_missing={in_missing}",
                );
            }
        }
    }

    /// `assert_missing_kinds_matches_has` testkit accepts the coherent
    /// local scaffold — sweeping every populated slot through the
    /// per-kind negation + canonical `ALL`-filter + single-slot
    /// diagonal + XOR partition arms. A regression on any of the four
    /// composition laws fails at the substrate primitive's
    /// `#[track_caller]` boundary here rather than at four per-parent
    /// production sites downstream.
    #[test]
    fn assert_missing_kinds_matches_has_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_missing_kinds_matches_has::<LocalParent, _>(make_local);
    }

    /// A factory that yields an all-empty parent MUST fail-loudly at
    /// the caller's site through the primitive's single-slot diagonal
    /// arm — the full `ALL` vec (every kind missing) does not equal
    /// `ALL \ {populated}` (which excludes `populated`). Pin the
    /// diagonal-arm failure mode so a regression that silently
    /// succeeded on an all-empty factory is caught here.
    #[test]
    #[should_panic(expected = "must return ClosedSet::ALL with Alpha removed")]
    fn assert_missing_kinds_matches_has_rejects_factory_that_populates_no_slots() {
        fn empty_factory(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_missing_kinds_matches_has::<LocalParent, _>(empty_factory);
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the single-slot closed-set-complement
    /// primitive `assert_missing_kinds_matches_has` coherently — every
    /// per-site `single_slot_X(k)` factory produces a parent whose
    /// `missing_kinds()` equals `ALL \ {k}` and whose per-kind
    /// negation composition law `missing_kinds().contains(&k) == !has(k)`
    /// holds for every `k ∈ ClosedSet::ALL`, AND the XOR partition law
    /// with `populated_kinds` binds byte-identically at every closed-
    /// set entry. Sweep every production implementor at ONE substrate
    /// boundary so a regression that drifts a production site's
    /// `single_slot_X` factory OR the default `missing_kinds` body (a
    /// specialization that inverted the negation, short-circuited, or
    /// drifted the walk order) fails BOTH at any future per-crate test
    /// site AND at this substrate-wide sweep.
    #[test]
    fn every_production_tagged_union_binds_through_the_missing_kinds_testkit_primitive() {
        assert_missing_kinds_matches_has::<crate::intent::Intent, _>(single_slot_intent_probe);
        assert_missing_kinds_matches_has::<crate::encapsulates::EncapsulationKind, _>(
            single_slot_encapsulation_kind_probe,
        );
        assert_missing_kinds_matches_has::<crate::export::ArtifactSource, _>(
            single_slot_artifact_source_probe,
        );
        assert_missing_kinds_matches_has::<crate::export::VectorChannel, _>(
            single_slot_vector_channel_probe,
        );
    }

    // -------------------------------------------------------------------
    // `TaggedUnion::missing_kind_count` — scalar cardinality refinement
    // on the closed-set-COMPLEMENT axis. Pin the three arms (empty parent
    // → ALL.len(), single-slot → ALL.len() - 1, saturated → 0) directly
    // on the sibling-shaped `LocalParent` scaffold and the composition
    // law `missing_kind_count() == missing_kinds().len()` + the scalar
    // partition law `populated_kind_count + missing_kind_count ==
    // ALL.len()` at the substrate testkit
    // `assert_missing_kind_count_matches_missing_kinds`.
    // -------------------------------------------------------------------

    /// EMPTY parent — the default body's `ALL.filter(!has).count()`
    /// sweep yields `ALL.len()` when no slot is populated. Pins the
    /// full-cardinality complement arm.
    #[test]
    fn tagged_union_default_missing_kind_count_returns_all_len_on_empty_parent() {
        let empty = LocalParent::default();
        assert_eq!(
            <LocalParent as TaggedUnion>::missing_kind_count(&empty),
            <LocalKind as tatara_closed_set::ClosedSet>::ALL.len(),
            "missing_kind_count() must return ALL.len() when no slot is populated",
        );
    }

    /// SINGLE-SLOT parent — the default body sweeps `ClosedSet::ALL`
    /// with `!has(k)` and counts `ALL.len() - 1` for each single-
    /// populated arrangement. Pins the well-formed arm's complement
    /// cardinality per addressed kind.
    #[test]
    fn tagged_union_default_missing_kind_count_returns_all_len_minus_one_per_single_slot_variant() {
        let expected = <LocalKind as tatara_closed_set::ClosedSet>::ALL.len() - 1;
        for populated in <LocalKind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let parent = match populated {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            };
            assert_eq!(
                <LocalParent as TaggedUnion>::missing_kind_count(&parent),
                expected,
                "single-slot parent must return ALL.len() - 1 on missing_kind_count for {populated:?}",
            );
        }
    }

    /// SATURATED parent — every slot populated returns `0` on
    /// `missing_kind_count`. Pins the zero-cardinality complement arm
    /// (mirror of `populated_kind_count` returning `ALL.len()` on the
    /// saturated arm).
    #[test]
    fn tagged_union_default_missing_kind_count_returns_zero_on_saturated_parent() {
        let saturated = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: Some(3),
        };
        assert_eq!(
            <LocalParent as TaggedUnion>::missing_kind_count(&saturated),
            0,
            "saturated parent must return 0 on missing_kind_count",
        );
    }

    /// Composition law `missing_kind_count() == missing_kinds().len()`
    /// binds the scalar cardinality projection to the widened primitive
    /// across every `ClosedSet::ALL × {empty, single_slot, two_slot,
    /// saturated}` combination. AND the scalar partition law
    /// `populated_kind_count() + missing_kind_count() == ALL.len()`
    /// binds the two axes byte-identically. Pins BOTH invariants at
    /// ONE test.
    #[test]
    fn tagged_union_default_missing_kind_count_matches_missing_kinds_len_and_partitions() {
        let all_len = <LocalKind as tatara_closed_set::ClosedSet>::ALL.len();
        let arrangements: [LocalParent; 4] = [
            LocalParent::default(),
            LocalParent {
                alpha: Some(1),
                ..Default::default()
            },
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: None,
            },
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            },
        ];
        for (idx, parent) in arrangements.iter().enumerate() {
            let count = <LocalParent as TaggedUnion>::missing_kind_count(parent);
            let missing_len = <LocalParent as TaggedUnion>::missing_kinds(parent).len();
            assert_eq!(
                count, missing_len,
                "missing_kind_count() must equal missing_kinds().len() for arrangement idx {idx}",
            );
            let populated_count = <LocalParent as TaggedUnion>::populated_kind_count(parent);
            assert_eq!(
                populated_count + count,
                all_len,
                "scalar partition law violated at arrangement idx {idx} — populated_kind_count + missing_kind_count must equal ALL.len()",
            );
        }
    }

    /// `assert_missing_kind_count_matches_missing_kinds` testkit
    /// accepts the coherent local scaffold — sweeping every populated
    /// kind through the three sub-assertions (composition law
    /// `count == missing_kinds.len()` + single-slot diagonal `count ==
    /// ALL.len() - 1` + scalar partition law
    /// `populated_kind_count + missing_kind_count == ALL.len()`). A
    /// regression on any of the three fails at the substrate
    /// primitive's `#[track_caller]` boundary.
    #[test]
    fn assert_missing_kind_count_matches_missing_kinds_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_missing_kind_count_matches_missing_kinds::<LocalParent, _>(make_local);
    }

    /// A factory that yields an all-empty parent (so
    /// `missing_kind_count()` returns `ALL.len()`) MUST fail-loudly at
    /// the caller's site through the primitive's single-slot diagonal
    /// arm — the `ALL.len()` cardinality does not equal `ALL.len() - 1`.
    #[test]
    #[should_panic(expected = "must equal ALL.len() - 1 exactly")]
    fn assert_missing_kind_count_matches_missing_kinds_rejects_empty_factory() {
        fn empty_factory(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_missing_kind_count_matches_missing_kinds::<LocalParent, _>(empty_factory);
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the scalar-cardinality complement
    /// primitive `assert_missing_kind_count_matches_missing_kinds`
    /// coherently — every per-site `single_slot_X(k)` factory produces
    /// a parent whose `missing_kind_count()` equals `ALL.len() - 1`
    /// AND whose composition law `count == missing_kinds().len()` AND
    /// scalar partition law `populated_kind_count + missing_kind_count
    /// == ALL.len()` all hold. Sweep every production implementor at
    /// ONE substrate boundary.
    #[test]
    fn every_production_tagged_union_binds_through_the_missing_kind_count_testkit_primitive() {
        assert_missing_kind_count_matches_missing_kinds::<crate::intent::Intent, _>(
            single_slot_intent_probe,
        );
        assert_missing_kind_count_matches_missing_kinds::<crate::encapsulates::EncapsulationKind, _>(
            single_slot_encapsulation_kind_probe,
        );
        assert_missing_kind_count_matches_missing_kinds::<crate::export::ArtifactSource, _>(
            single_slot_artifact_source_probe,
        );
        assert_missing_kind_count_matches_missing_kinds::<crate::export::VectorChannel, _>(
            single_slot_vector_channel_probe,
        );
    }

    // -------------------------------------------------------------------
    // `TaggedUnion::first_populated_kind` / `first_missing_kind` — the
    // short-circuiting `Option<Kind>` peers of `populated_kinds` /
    // `missing_kinds`. Pin the four-outcome truth table (empty parent
    // → `first_populated_kind` is `None`, `first_missing_kind` is
    // `Some(ALL[0])`; populated diagonal → `first_populated_kind` is
    // `Some(k)`, `first_missing_kind` is the earliest `ALL` entry
    // != `k`; multi-populated → `first_populated_kind` names the
    // EARLIEST populated slot in canonical `ALL` order) directly on
    // the `LocalParent` scaffold AND via the substrate testkit
    // primitives, so a regression on the default body's short-circuit
    // or negation composition fails here before any per-parent
    // inherent test surfaces the drift.
    // -------------------------------------------------------------------

    /// EMPTY-PARENT pin — a default [`LocalParent`] returns `None` at
    /// `first_populated_kind` (no slot populated) and `Some(ALL[0])` at
    /// `first_missing_kind` (every slot missing, earliest hit is
    /// index 0 of the canonical closed-set walk). Composition-law pin:
    /// `first_populated_kind().is_none() == (populated_kind_count() ==
    /// 0)` and `first_missing_kind() == Some(ALL[0])` on the empty
    /// boundary.
    #[test]
    fn tagged_union_default_first_kinds_on_empty_parent() {
        let empty = LocalParent::default();
        assert_eq!(
            <LocalParent as TaggedUnion>::first_populated_kind(&empty),
            None,
        );
        assert_eq!(
            <LocalParent as TaggedUnion>::first_missing_kind(&empty),
            Some(<LocalKind as tatara_closed_set::ClosedSet>::ALL[0]),
        );
    }

    /// SINGLE-SLOT DIAGONAL pin — every populated position across
    /// [`LocalKind::ALL`] returns `Some(k)` at `first_populated_kind`
    /// (the sole populated slot IS the earliest one) AND the earliest
    /// `ALL` entry != `k` at `first_missing_kind`. Both projections
    /// agree with the widened primitives via
    /// `first_populated_kind() == populated_kinds().first().copied()`
    /// and `first_missing_kind() == missing_kinds().first().copied()`.
    #[test]
    fn tagged_union_default_first_kinds_on_single_slot_diagonal() {
        for populated in <LocalKind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let parent = match populated {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            };
            assert_eq!(
                <LocalParent as TaggedUnion>::first_populated_kind(&parent),
                Some(populated),
            );
            let expected_first_missing = <LocalKind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .copied()
                .find(|k| *k != populated);
            assert_eq!(
                <LocalParent as TaggedUnion>::first_missing_kind(&parent),
                expected_first_missing,
            );
            // Composition laws vs. widened primitives.
            assert_eq!(
                parent.first_populated_kind(),
                parent.populated_kinds().first().copied(),
            );
            assert_eq!(
                parent.first_missing_kind(),
                parent.missing_kinds().first().copied(),
            );
        }
    }

    /// TWO-POPULATED pin — a `LocalParent` with two populated slots
    /// returns `first_populated_kind() == Some(min_all(a, b))` (the
    /// EARLIEST populated slot in canonical `ClosedSet::ALL` order —
    /// strictly more informative than the payload-free
    /// [`LocalParentError::Ambiguous`] carrier `variant()` returns on
    /// the same input). Pins the walk order on the Ambiguous arm at
    /// ONE substrate boundary — a regression that iterates `ALL` in
    /// reverse or in construction order fails here.
    #[test]
    fn tagged_union_default_first_populated_kind_names_earliest_of_two_populated_slots() {
        // Alpha + Beta populated → earliest is Alpha (ALL[0]).
        let p = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: None,
        };
        assert_eq!(p.first_populated_kind(), Some(LocalKind::Alpha));
        // Missing set is [Gamma]; earliest missing is Gamma.
        assert_eq!(p.first_missing_kind(), Some(LocalKind::Gamma));

        // Beta + Gamma populated → earliest is Beta.
        let p = LocalParent {
            alpha: None,
            beta: Some(1),
            gamma: Some(2),
        };
        assert_eq!(p.first_populated_kind(), Some(LocalKind::Beta));
        assert_eq!(p.first_missing_kind(), Some(LocalKind::Alpha));

        // Alpha + Gamma populated → earliest is Alpha.
        let p = LocalParent {
            alpha: Some(1),
            beta: None,
            gamma: Some(2),
        };
        assert_eq!(p.first_populated_kind(), Some(LocalKind::Alpha));
        assert_eq!(p.first_missing_kind(), Some(LocalKind::Beta));
    }

    /// SATURATED-PARENT pin — a `LocalParent` with EVERY slot
    /// populated returns `Some(ALL[0])` at `first_populated_kind`
    /// (earliest hit on the all-`true` predicate is index 0) and
    /// `None` at `first_missing_kind` (no missing slot exists). Pins
    /// the earliest-missing projection's `None` arm at ONE substrate
    /// boundary — a regression that returned `Some(ALL[0])` (dropping
    /// the negation) or `Some(ALL[ALL.len()-1])` (walking in reverse)
    /// fails here.
    #[test]
    fn tagged_union_default_first_missing_kind_returns_none_on_saturated_parent() {
        let p = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: Some(3),
        };
        assert_eq!(p.first_populated_kind(), Some(LocalKind::Alpha));
        assert_eq!(p.first_missing_kind(), None);
    }

    /// The `assert_first_populated_kind_matches_populated_kinds`
    /// primitive accepts the [`LocalParent`] scaffold coherently — the
    /// Ok arm is the "no drift" outcome.
    #[test]
    fn assert_first_populated_kind_matches_populated_kinds_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_first_populated_kind_matches_populated_kinds::<LocalParent, _>(make_local);
    }

    /// A factory that yields an all-empty parent (so
    /// `first_populated_kind()` returns `None`) MUST fail-loudly at
    /// the caller's site through the primitive's single-slot diagonal
    /// arm — `None` does not equal `Some(populated)`.
    #[test]
    #[should_panic(expected = "must equal Some(")]
    fn assert_first_populated_kind_matches_populated_kinds_rejects_empty_factory() {
        fn empty_factory(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_first_populated_kind_matches_populated_kinds::<LocalParent, _>(empty_factory);
    }

    /// The `assert_first_missing_kind_matches_missing_kinds` primitive
    /// accepts the [`LocalParent`] scaffold coherently.
    #[test]
    fn assert_first_missing_kind_matches_missing_kinds_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_first_missing_kind_matches_missing_kinds::<LocalParent, _>(make_local);
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the earliest-populated primitive
    /// coherently — every per-site `single_slot_X(k)` factory produces
    /// a parent whose `first_populated_kind()` equals `Some(k)`.
    #[test]
    fn every_production_tagged_union_binds_through_the_first_populated_kind_testkit_primitive() {
        assert_first_populated_kind_matches_populated_kinds::<crate::intent::Intent, _>(
            single_slot_intent_probe,
        );
        assert_first_populated_kind_matches_populated_kinds::<
            crate::encapsulates::EncapsulationKind,
            _,
        >(single_slot_encapsulation_kind_probe);
        assert_first_populated_kind_matches_populated_kinds::<crate::export::ArtifactSource, _>(
            single_slot_artifact_source_probe,
        );
        assert_first_populated_kind_matches_populated_kinds::<crate::export::VectorChannel, _>(
            single_slot_vector_channel_probe,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the earliest-missing primitive
    /// coherently.
    #[test]
    fn every_production_tagged_union_binds_through_the_first_missing_kind_testkit_primitive() {
        assert_first_missing_kind_matches_missing_kinds::<crate::intent::Intent, _>(
            single_slot_intent_probe,
        );
        assert_first_missing_kind_matches_missing_kinds::<crate::encapsulates::EncapsulationKind, _>(
            single_slot_encapsulation_kind_probe,
        );
        assert_first_missing_kind_matches_missing_kinds::<crate::export::ArtifactSource, _>(
            single_slot_artifact_source_probe,
        );
        assert_first_missing_kind_matches_missing_kinds::<crate::export::VectorChannel, _>(
            single_slot_vector_channel_probe,
        );
    }

    // -------------------------------------------------------------------
    // `TaggedUnion::last_populated_kind` / `last_missing_kind` — the
    // short-circuiting REVERSED-walk `Option<Kind>` peers of
    // `first_populated_kind` / `first_missing_kind`. Pin the four-outcome
    // truth table (empty parent → `last_populated_kind` is `None`,
    // `last_missing_kind` is `Some(ALL[ALL.len()-1])`; populated diagonal
    // → `last_populated_kind` is `Some(k)`, `last_missing_kind` is the
    // latest `ALL` entry != `k`; multi-populated → `last_populated_kind`
    // names the LATEST populated slot in canonical `ALL` order;
    // saturated → `last_missing_kind` is `None`) directly on the
    // `LocalParent` scaffold AND via the substrate testkit primitives,
    // so a regression on the reversed default body's short-circuit or
    // negation composition fails here before any per-parent inherent
    // test surfaces the drift.
    // -------------------------------------------------------------------

    /// EMPTY-PARENT pin — a default [`LocalParent`] returns `None` at
    /// `last_populated_kind` (no slot populated) and
    /// `Some(ALL[ALL.len()-1])` at `last_missing_kind` (every slot
    /// missing, latest hit is the last index of the canonical closed-
    /// set walk under REVERSED iteration). Composition-law pin:
    /// `last_populated_kind().is_none() == (populated_kind_count() ==
    /// 0)` and `last_missing_kind() == Some(ALL[ALL.len()-1])` on the
    /// empty boundary.
    #[test]
    fn tagged_union_default_last_kinds_on_empty_parent() {
        let empty = LocalParent::default();
        assert_eq!(
            <LocalParent as TaggedUnion>::last_populated_kind(&empty),
            None,
        );
        let all_len = <LocalKind as tatara_closed_set::ClosedSet>::ALL.len();
        assert_eq!(
            <LocalParent as TaggedUnion>::last_missing_kind(&empty),
            Some(<LocalKind as tatara_closed_set::ClosedSet>::ALL[all_len - 1]),
        );
    }

    /// SINGLE-SLOT DIAGONAL pin — every populated position across
    /// [`LocalKind::ALL`] returns `Some(k)` at `last_populated_kind`
    /// (the sole populated slot IS both the earliest AND the latest)
    /// AND the LATEST `ALL` entry != `k` at `last_missing_kind`. Both
    /// projections agree with the widened primitives via
    /// `last_populated_kind() == populated_kinds().last().copied()`
    /// and `last_missing_kind() == missing_kinds().last().copied()`.
    #[test]
    fn tagged_union_default_last_kinds_on_single_slot_diagonal() {
        for populated in <LocalKind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let parent = match populated {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            };
            assert_eq!(
                <LocalParent as TaggedUnion>::last_populated_kind(&parent),
                Some(populated),
            );
            let expected_last_missing = <LocalKind as tatara_closed_set::ClosedSet>::ALL
                .iter()
                .rev()
                .copied()
                .find(|k| *k != populated);
            assert_eq!(
                <LocalParent as TaggedUnion>::last_missing_kind(&parent),
                expected_last_missing,
            );
            // Composition laws vs. widened primitives.
            assert_eq!(
                parent.last_populated_kind(),
                parent.populated_kinds().last().copied(),
            );
            assert_eq!(
                parent.last_missing_kind(),
                parent.missing_kinds().last().copied(),
            );
        }
    }

    /// TWO-POPULATED pin — a `LocalParent` with two populated slots
    /// returns `last_populated_kind() == Some(max_all(a, b))` (the
    /// LATEST populated slot in canonical `ClosedSet::ALL` order —
    /// byte-for-byte time-reversed peer of the earliest-populated
    /// projection). Pins the walk order on the Ambiguous arm at ONE
    /// substrate boundary — a regression that iterates `ALL` forward
    /// (defeating the time-reversal) fails here.
    #[test]
    fn tagged_union_default_last_populated_kind_names_latest_of_two_populated_slots() {
        // Alpha + Beta populated → latest is Beta (ALL[1]).
        let p = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: None,
        };
        assert_eq!(p.last_populated_kind(), Some(LocalKind::Beta));
        // Missing set is [Gamma]; latest missing is Gamma.
        assert_eq!(p.last_missing_kind(), Some(LocalKind::Gamma));

        // Beta + Gamma populated → latest is Gamma.
        let p = LocalParent {
            alpha: None,
            beta: Some(1),
            gamma: Some(2),
        };
        assert_eq!(p.last_populated_kind(), Some(LocalKind::Gamma));
        assert_eq!(p.last_missing_kind(), Some(LocalKind::Alpha));

        // Alpha + Gamma populated → latest is Gamma.
        let p = LocalParent {
            alpha: Some(1),
            beta: None,
            gamma: Some(2),
        };
        assert_eq!(p.last_populated_kind(), Some(LocalKind::Gamma));
        assert_eq!(p.last_missing_kind(), Some(LocalKind::Beta));
    }

    /// SATURATED-PARENT pin — a `LocalParent` with EVERY slot
    /// populated returns `Some(ALL[ALL.len()-1])` at
    /// `last_populated_kind` (latest hit on the all-`true` predicate
    /// under REVERSED iteration is the last index) and `None` at
    /// `last_missing_kind` (no missing slot exists). Pins the latest-
    /// missing projection's `None` arm at ONE substrate boundary — a
    /// regression that returned `Some(ALL[ALL.len()-1])` (dropping the
    /// negation) or `Some(ALL[0])` (defeating the time-reversal)
    /// fails here.
    #[test]
    fn tagged_union_default_last_missing_kind_returns_none_on_saturated_parent() {
        let p = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: Some(3),
        };
        let all_len = <LocalKind as tatara_closed_set::ClosedSet>::ALL.len();
        assert_eq!(
            p.last_populated_kind(),
            Some(<LocalKind as tatara_closed_set::ClosedSet>::ALL[all_len - 1]),
        );
        assert_eq!(p.last_missing_kind(), None);
    }

    /// The `assert_last_populated_kind_matches_populated_kinds`
    /// primitive accepts the [`LocalParent`] scaffold coherently — the
    /// Ok arm is the "no drift" outcome.
    #[test]
    fn assert_last_populated_kind_matches_populated_kinds_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_last_populated_kind_matches_populated_kinds::<LocalParent, _>(make_local);
    }

    /// A factory that yields an all-empty parent (so
    /// `last_populated_kind()` returns `None`) MUST fail-loudly at the
    /// caller's site through the primitive's single-slot diagonal arm
    /// — `None` does not equal `Some(populated)`.
    #[test]
    #[should_panic(expected = "must equal Some(")]
    fn assert_last_populated_kind_matches_populated_kinds_rejects_empty_factory() {
        fn empty_factory(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_last_populated_kind_matches_populated_kinds::<LocalParent, _>(empty_factory);
    }

    /// The `assert_last_missing_kind_matches_missing_kinds` primitive
    /// accepts the [`LocalParent`] scaffold coherently.
    #[test]
    fn assert_last_missing_kind_matches_missing_kinds_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_last_missing_kind_matches_missing_kinds::<LocalParent, _>(make_local);
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the latest-populated primitive
    /// coherently — every per-site `single_slot_X(k)` factory produces
    /// a parent whose `last_populated_kind()` equals `Some(k)`.
    #[test]
    fn every_production_tagged_union_binds_through_the_last_populated_kind_testkit_primitive() {
        assert_last_populated_kind_matches_populated_kinds::<crate::intent::Intent, _>(
            single_slot_intent_probe,
        );
        assert_last_populated_kind_matches_populated_kinds::<
            crate::encapsulates::EncapsulationKind,
            _,
        >(single_slot_encapsulation_kind_probe);
        assert_last_populated_kind_matches_populated_kinds::<crate::export::ArtifactSource, _>(
            single_slot_artifact_source_probe,
        );
        assert_last_populated_kind_matches_populated_kinds::<crate::export::VectorChannel, _>(
            single_slot_vector_channel_probe,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the latest-missing primitive
    /// coherently.
    #[test]
    fn every_production_tagged_union_binds_through_the_last_missing_kind_testkit_primitive() {
        assert_last_missing_kind_matches_missing_kinds::<crate::intent::Intent, _>(
            single_slot_intent_probe,
        );
        assert_last_missing_kind_matches_missing_kinds::<crate::encapsulates::EncapsulationKind, _>(
            single_slot_encapsulation_kind_probe,
        );
        assert_last_missing_kind_matches_missing_kinds::<crate::export::ArtifactSource, _>(
            single_slot_artifact_source_probe,
        );
        assert_last_missing_kind_matches_missing_kinds::<crate::export::VectorChannel, _>(
            single_slot_vector_channel_probe,
        );
    }

    // -------------------------------------------------------------------
    // `TaggedUnion::unique_populated_kind` / `unique_missing_kind` — the
    // short-circuiting `Option<Kind>` peers on the exactly-one-hit axis.
    // Pin the four-outcome truth table (empty parent → both `None`;
    // single-slot diagonal → `unique_populated_kind` is `Some(k)`,
    // `unique_missing_kind` is `None` on `ALL.len() > 2`; two-populated
    // parent → `unique_populated_kind` is `None`, `unique_missing_kind`
    // is `Some(the-one-missing)`; saturated → both `None`) directly on
    // the `LocalParent` scaffold AND via the substrate testkit
    // primitives, so a regression on the two-step short-circuit's
    // second-hit truncation or the negation composition fails here
    // before any per-parent inherent test surfaces the drift.
    // -------------------------------------------------------------------

    /// EMPTY-PARENT pin — a default [`LocalParent`] returns `None` at
    /// BOTH `unique_populated_kind` (zero populated, not exactly-one)
    /// and `unique_missing_kind` (three missing on a `ALL.len() == 3`
    /// closed set, not exactly-one). Pins the empty-arm collapse — the
    /// two primitives agree on `None` when the closed-set cardinality
    /// is ≥ 3, distinguishing the exactly-one primitive from the
    /// endpoint primitives (`first_missing_kind` on an empty parent
    /// returns `Some(ALL[0])`, not `None`).
    #[test]
    fn tagged_union_default_unique_kinds_on_empty_parent() {
        let empty = LocalParent::default();
        assert_eq!(
            <LocalParent as TaggedUnion>::unique_populated_kind(&empty),
            None,
        );
        assert_eq!(
            <LocalParent as TaggedUnion>::unique_missing_kind(&empty),
            None,
        );
    }

    /// SINGLE-SLOT DIAGONAL pin — every populated position across
    /// [`LocalKind::ALL`] returns `Some(k)` at `unique_populated_kind`
    /// (the sole populated slot IS the exactly-one hit) AND `None` at
    /// `unique_missing_kind` (two missing slots on the `ALL.len() == 3`
    /// closed set, not exactly-one). The `Some` arm's endpoint
    /// agreement composes with `first_populated_kind` /
    /// `last_populated_kind` at the trait defaults (`unique == first
    /// == last` on exactly-one).
    #[test]
    fn tagged_union_default_unique_kinds_on_single_slot_diagonal() {
        for populated in <LocalKind as tatara_closed_set::ClosedSet>::ALL
            .iter()
            .copied()
        {
            let parent = match populated {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            };
            assert_eq!(
                <LocalParent as TaggedUnion>::unique_populated_kind(&parent),
                Some(populated),
            );
            assert_eq!(
                <LocalParent as TaggedUnion>::unique_missing_kind(&parent),
                None,
            );
            // Endpoint-agreement composition — on Some, the three
            // endpoint-projection primitives agree.
            assert_eq!(
                parent.unique_populated_kind(),
                parent.first_populated_kind()
            );
            assert_eq!(parent.unique_populated_kind(), parent.last_populated_kind());
        }
    }

    /// TWO-POPULATED pin — a `LocalParent` with two populated slots
    /// returns `unique_populated_kind() == None` (two populated, not
    /// exactly-one) and `unique_missing_kind() == Some(the-one-missing)`
    /// (one missing, exactly-one — the ONLY arm where the missing-side
    /// primitive returns `Some` on a `ALL.len() == 3` closed set). Pins
    /// the two-step short-circuit's second-hit collapse at ONE
    /// substrate boundary — a regression that returned `Some(first)`
    /// after seeing two populated slots (defeating the exactly-one
    /// contract) fails here.
    #[test]
    fn tagged_union_default_unique_kinds_on_two_populated_parent() {
        // Alpha + Beta populated → 2 populated (unique_populated=None),
        // 1 missing = Gamma (unique_missing=Some(Gamma)).
        let p = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: None,
        };
        assert_eq!(p.unique_populated_kind(), None);
        assert_eq!(p.unique_missing_kind(), Some(LocalKind::Gamma));
        // Endpoint-agreement composition on the missing-side Some arm.
        assert_eq!(p.unique_missing_kind(), p.first_missing_kind());
        assert_eq!(p.unique_missing_kind(), p.last_missing_kind());

        // Beta + Gamma populated → unique_missing=Some(Alpha).
        let p = LocalParent {
            alpha: None,
            beta: Some(1),
            gamma: Some(2),
        };
        assert_eq!(p.unique_populated_kind(), None);
        assert_eq!(p.unique_missing_kind(), Some(LocalKind::Alpha));

        // Alpha + Gamma populated → unique_missing=Some(Beta).
        let p = LocalParent {
            alpha: Some(1),
            beta: None,
            gamma: Some(2),
        };
        assert_eq!(p.unique_populated_kind(), None);
        assert_eq!(p.unique_missing_kind(), Some(LocalKind::Beta));
    }

    /// SATURATED-PARENT pin — a `LocalParent` with EVERY slot
    /// populated returns `None` at BOTH `unique_populated_kind` (three
    /// populated, not exactly-one) AND `unique_missing_kind` (zero
    /// missing, not exactly-one). Pins the saturated-arm collapse — the
    /// two primitives agree on `None` when the closed-set cardinality
    /// is ≥ 3, distinguishing the exactly-one primitive from the
    /// endpoint primitives (`last_populated_kind` on a saturated
    /// parent returns `Some(ALL[ALL.len()-1])`, not `None`).
    #[test]
    fn tagged_union_default_unique_kinds_on_saturated_parent() {
        let p = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: Some(3),
        };
        assert_eq!(p.unique_populated_kind(), None);
        assert_eq!(p.unique_missing_kind(), None);
    }

    /// The `assert_unique_populated_kind_matches_populated_kinds`
    /// primitive accepts the [`LocalParent`] scaffold coherently — the
    /// Ok arm is the "no drift" outcome.
    #[test]
    fn assert_unique_populated_kind_matches_populated_kinds_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_unique_populated_kind_matches_populated_kinds::<LocalParent, _>(make_local);
    }

    /// A factory that yields an all-empty parent (so
    /// `unique_populated_kind()` returns `None`) MUST fail-loudly at
    /// the caller's site through the primitive's single-slot diagonal
    /// arm — `None` does not equal `Some(populated)`.
    #[test]
    #[should_panic(expected = "must equal Some(")]
    fn assert_unique_populated_kind_matches_populated_kinds_rejects_empty_factory() {
        fn empty_factory(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_unique_populated_kind_matches_populated_kinds::<LocalParent, _>(empty_factory);
    }

    /// The `assert_unique_missing_kind_matches_missing_kinds` primitive
    /// accepts the [`LocalParent`] scaffold coherently.
    #[test]
    fn assert_unique_missing_kind_matches_missing_kinds_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_unique_missing_kind_matches_missing_kinds::<LocalParent, _>(make_local);
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the exactly-one-populated primitive
    /// coherently — every per-site `single_slot_X(k)` factory produces
    /// a parent whose `unique_populated_kind()` equals `Some(k)`.
    #[test]
    fn every_production_tagged_union_binds_through_the_unique_populated_kind_testkit_primitive() {
        assert_unique_populated_kind_matches_populated_kinds::<crate::intent::Intent, _>(
            single_slot_intent_probe,
        );
        assert_unique_populated_kind_matches_populated_kinds::<
            crate::encapsulates::EncapsulationKind,
            _,
        >(single_slot_encapsulation_kind_probe);
        assert_unique_populated_kind_matches_populated_kinds::<crate::export::ArtifactSource, _>(
            single_slot_artifact_source_probe,
        );
        assert_unique_populated_kind_matches_populated_kinds::<crate::export::VectorChannel, _>(
            single_slot_vector_channel_probe,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the exactly-one-missing primitive
    /// coherently.
    #[test]
    fn every_production_tagged_union_binds_through_the_unique_missing_kind_testkit_primitive() {
        assert_unique_missing_kind_matches_missing_kinds::<crate::intent::Intent, _>(
            single_slot_intent_probe,
        );
        assert_unique_missing_kind_matches_missing_kinds::<crate::encapsulates::EncapsulationKind, _>(
            single_slot_encapsulation_kind_probe,
        );
        assert_unique_missing_kind_matches_missing_kinds::<crate::export::ArtifactSource, _>(
            single_slot_artifact_source_probe,
        );
        assert_unique_missing_kind_matches_missing_kinds::<crate::export::VectorChannel, _>(
            single_slot_vector_channel_probe,
        );
    }

    // -------------------------------------------------------------------
    // `TaggedUnion::is_empty` / `TaggedUnion::is_saturated` trait-level
    // truth-table pins on the sibling-shaped `LocalParent` scaffold.
    // The two primitives are the Boolean cardinality-endpoint peers of
    // `populated_kind_count() == 0` and `missing_kind_count() == 0`
    // respectively — every arm below pins one truth-table entry directly
    // on the trait's default body without reaching for either scalar
    // primitive.
    // -------------------------------------------------------------------

    /// EMPTY-PARENT pin — a default-constructed `LocalParent` (every
    /// slot None) returns `true` at `is_empty` (zero populated slots)
    /// AND `false` at `is_saturated` (three missing slots, not zero).
    /// Pins the zero-arm of the `is_empty` primitive and the negation
    /// of the `is_saturated` primitive on the same fixture — a
    /// regression that inverted either default body's composition
    /// direction fails here.
    #[test]
    fn tagged_union_default_is_empty_and_is_saturated_on_empty_parent() {
        let p = LocalParent::default();
        assert!(p.is_empty(), "empty parent must be is_empty");
        assert!(!p.is_saturated(), "empty parent must NOT be is_saturated");
        // Composition law with the scalar cardinality primitives.
        assert_eq!(p.is_empty(), p.populated_kind_count() == 0);
        assert_eq!(p.is_saturated(), p.missing_kind_count() == 0);
        // Widened-primitive agreement.
        assert_eq!(p.is_empty(), p.populated_kinds().is_empty());
        assert_eq!(p.is_saturated(), p.missing_kinds().is_empty());
    }

    /// SINGLE-SLOT-DIAGONAL pin — a `LocalParent` populating exactly
    /// one slot returns `false` at BOTH `is_empty` (one populated, not
    /// zero) AND `is_saturated` (two missing, not zero). Pins that a
    /// well-formed parent lands OUTSIDE both cardinality endpoints —
    /// the Boolean primitives coincide on `false` on this arm, and
    /// only on the empty parent (`is_empty` true) or a saturated
    /// parent (`is_saturated` true) do they diverge.
    #[test]
    fn tagged_union_default_is_empty_and_is_saturated_on_single_slot_diagonal() {
        for (populated, parent) in [
            (
                LocalKind::Alpha,
                LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
            ),
            (
                LocalKind::Beta,
                LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
            ),
            (
                LocalKind::Gamma,
                LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            ),
        ] {
            assert!(
                !parent.is_empty(),
                "single_slot({populated:?}) must NOT be is_empty",
            );
            assert!(
                !parent.is_saturated(),
                "single_slot({populated:?}) must NOT be is_saturated",
            );
            assert_eq!(parent.is_empty(), parent.populated_kind_count() == 0);
            assert_eq!(parent.is_saturated(), parent.missing_kind_count() == 0);
        }
    }

    /// SATURATED-PARENT pin — a `LocalParent` with EVERY slot populated
    /// returns `false` at `is_empty` (three populated, not zero) AND
    /// `true` at `is_saturated` (zero missing). Pins the top-arm of
    /// the `is_saturated` primitive and the negation of the `is_empty`
    /// primitive on the same fixture — the mirror of the empty-parent
    /// pin above, distinguishing the two cardinality endpoints on
    /// opposite arms of the same closed-set walk.
    #[test]
    fn tagged_union_default_is_empty_and_is_saturated_on_saturated_parent() {
        let p = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: Some(3),
        };
        assert!(!p.is_empty(), "saturated parent must NOT be is_empty");
        assert!(p.is_saturated(), "saturated parent must be is_saturated");
        assert_eq!(p.is_empty(), p.populated_kind_count() == 0);
        assert_eq!(p.is_saturated(), p.missing_kind_count() == 0);
        assert_eq!(p.is_empty(), p.populated_kinds().is_empty());
        assert_eq!(p.is_saturated(), p.missing_kinds().is_empty());
    }

    // -------------------------------------------------------------------
    // Truth-table pins for `TaggedUnion::has_unique_populated_kind` and
    // `TaggedUnion::has_unique_missing_kind` — three arms (empty,
    // single-slot diagonal, saturated) on the sibling-shaped
    // `LocalParent` scaffold. The two primitives are the Boolean
    // cardinality-mid-endpoint peers of `populated_kind_count() == 1`
    // and `missing_kind_count() == 1` respectively — every arm below
    // pins one truth-table entry directly on the trait's default body
    // without reaching for either scalar primitive.
    // -------------------------------------------------------------------

    /// EMPTY-PARENT pin — a default-constructed `LocalParent` (every
    /// slot None) returns `false` at BOTH `has_unique_populated_kind`
    /// (zero populated, not one) AND `has_unique_missing_kind` (three
    /// missing on `ALL.len() == 3`, not one). Pins the zero-populated
    /// arm of the first primitive and the ALL.len()-missing arm of the
    /// second on the same fixture.
    #[test]
    fn tagged_union_default_has_unique_kinds_on_empty_parent() {
        let p = LocalParent::default();
        assert!(
            !p.has_unique_populated_kind(),
            "empty parent must NOT be has_unique_populated_kind (zero populated)",
        );
        assert!(
            !p.has_unique_missing_kind(),
            "empty parent must NOT be has_unique_missing_kind (three missing)",
        );
        // Composition law with the scalar cardinality primitives.
        assert_eq!(p.has_unique_populated_kind(), p.populated_kind_count() == 1);
        assert_eq!(p.has_unique_missing_kind(), p.missing_kind_count() == 1);
        // Unique-primitive agreement.
        assert_eq!(
            p.has_unique_populated_kind(),
            p.unique_populated_kind().is_some()
        );
        assert_eq!(
            p.has_unique_missing_kind(),
            p.unique_missing_kind().is_some()
        );
    }

    /// SINGLE-SLOT-DIAGONAL pin — a `LocalParent` populating exactly
    /// one slot returns `true` at `has_unique_populated_kind` (one
    /// populated) AND `false` at `has_unique_missing_kind` (two
    /// missing on `ALL.len() == 3`, not one). Pins that the well-
    /// formed arm coincides with the one-arm of the populated
    /// cardinality and lies OUTSIDE the one-arm of the missing
    /// cardinality on any `ALL.len() ≥ 3` closed set.
    #[test]
    fn tagged_union_default_has_unique_kinds_on_single_slot_diagonal() {
        for (populated, parent) in [
            (
                LocalKind::Alpha,
                LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
            ),
            (
                LocalKind::Beta,
                LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
            ),
            (
                LocalKind::Gamma,
                LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            ),
        ] {
            assert!(
                parent.has_unique_populated_kind(),
                "single_slot({populated:?}) must be has_unique_populated_kind",
            );
            assert!(
                !parent.has_unique_missing_kind(),
                "single_slot({populated:?}) must NOT be has_unique_missing_kind (2 missing on ALL.len()==3)",
            );
            assert_eq!(
                parent.has_unique_populated_kind(),
                parent.populated_kind_count() == 1
            );
            assert_eq!(
                parent.has_unique_missing_kind(),
                parent.missing_kind_count() == 1
            );
        }
    }

    /// SATURATED-PARENT pin — a `LocalParent` with EVERY slot populated
    /// returns `false` at BOTH `has_unique_populated_kind` (three
    /// populated, not one) AND `has_unique_missing_kind` (zero missing,
    /// not one). Pins the top-arm of the populated cardinality (which
    /// is NOT the one-arm) and the zero-arm of the missing cardinality
    /// (also NOT the one-arm) on the same fixture — the two primitives
    /// coincide on `false` here, distinguishing them from the
    /// (near-)saturation and near-empty arms outside the LocalParent
    /// scaffold's reach.
    #[test]
    fn tagged_union_default_has_unique_kinds_on_saturated_parent() {
        let p = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: Some(3),
        };
        assert!(
            !p.has_unique_populated_kind(),
            "saturated parent must NOT be has_unique_populated_kind (three populated)",
        );
        assert!(
            !p.has_unique_missing_kind(),
            "saturated parent must NOT be has_unique_missing_kind (zero missing)",
        );
        assert_eq!(p.has_unique_populated_kind(), p.populated_kind_count() == 1);
        assert_eq!(p.has_unique_missing_kind(), p.missing_kind_count() == 1);
    }

    /// NEAR-SATURATED (two-slot) pin — a `LocalParent` with exactly
    /// two slots populated returns `false` at `has_unique_populated_kind`
    /// (two populated, not one) AND `true` at `has_unique_missing_kind`
    /// (one missing on `ALL.len() == 3`). This is the SOLE arm on the
    /// LocalParent scaffold where the two Boolean cardinality-mid-
    /// endpoint peers DIVERGE — the pin distinguishes them from every
    /// other truth-table arm where they coincide.
    #[test]
    fn tagged_union_default_has_unique_kinds_on_near_saturated_parent() {
        for parent in [
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                ..Default::default()
            },
            LocalParent {
                alpha: Some(1),
                gamma: Some(3),
                ..Default::default()
            },
            LocalParent {
                beta: Some(2),
                gamma: Some(3),
                ..Default::default()
            },
        ] {
            assert!(
                !parent.has_unique_populated_kind(),
                "near-saturated parent must NOT be has_unique_populated_kind (2 populated)",
            );
            assert!(
                parent.has_unique_missing_kind(),
                "near-saturated parent must be has_unique_missing_kind (1 missing)",
            );
            assert_eq!(
                parent.has_unique_populated_kind(),
                parent.populated_kind_count() == 1,
            );
            assert_eq!(
                parent.has_unique_missing_kind(),
                parent.missing_kind_count() == 1,
            );
        }
    }

    // -------------------------------------------------------------------
    // `TaggedUnion::has_multiple_(populated|missing)_kinds` default-body
    // truth table — pin the four cardinality arms (empty, single-slot
    // diagonal, near-saturated, saturated) on the sibling-shaped
    // `LocalParent` scaffold. These two primitives are the Boolean
    // cardinality many-arm peers of `populated_kind_count() >= 2` and
    // `missing_kind_count() >= 2` — the third arm of the {0, 1, ≥2}
    // cardinality trichotomy that closes alongside `is_empty` /
    // `has_unique_populated_kind` (populated axis) and `is_saturated` /
    // `has_unique_missing_kind` (missing axis).
    // -------------------------------------------------------------------

    /// EMPTY-PARENT pin — an empty `LocalParent` returns `false` at
    /// `has_multiple_populated_kinds` (zero populated) AND `true` at
    /// `has_multiple_missing_kinds` (three missing on `ALL.len() == 3`,
    /// which is `>= 2`). Also pins the trichotomy partition law: on
    /// the empty arm exactly `is_empty()` is true on the populated
    /// axis, and exactly `has_multiple_missing_kinds()` is true on
    /// the missing axis.
    #[test]
    fn tagged_union_default_has_multiple_kinds_on_empty_parent() {
        let p = LocalParent::default();
        assert!(
            !p.has_multiple_populated_kinds(),
            "empty parent must NOT be has_multiple_populated_kinds (zero populated)",
        );
        assert!(
            p.has_multiple_missing_kinds(),
            "empty parent must be has_multiple_missing_kinds (three missing on ALL.len() == 3)",
        );
        // Composition laws.
        assert_eq!(
            p.has_multiple_populated_kinds(),
            p.populated_kind_count() >= 2
        );
        assert_eq!(p.has_multiple_missing_kinds(), p.missing_kind_count() >= 2);
        // Trichotomy partition — EXACTLY ONE of the three Boolean
        // primitives on each axis is true.
        assert_eq!(
            usize::from(p.is_empty())
                + usize::from(p.has_unique_populated_kind())
                + usize::from(p.has_multiple_populated_kinds()),
            1,
            "populated-axis trichotomy must be exactly-one on the empty arm",
        );
        assert_eq!(
            usize::from(p.is_saturated())
                + usize::from(p.has_unique_missing_kind())
                + usize::from(p.has_multiple_missing_kinds()),
            1,
            "missing-axis trichotomy must be exactly-one on the empty arm",
        );
    }

    /// SINGLE-SLOT-DIAGONAL pin — a `LocalParent` populating exactly
    /// one slot returns `false` at `has_multiple_populated_kinds` AND
    /// `true` at `has_multiple_missing_kinds` (two missing on
    /// `ALL.len() == 3`, which is `>= 2`).
    #[test]
    fn tagged_union_default_has_multiple_kinds_on_single_slot_diagonal() {
        for (populated, parent) in [
            (
                LocalKind::Alpha,
                LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
            ),
            (
                LocalKind::Beta,
                LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
            ),
            (
                LocalKind::Gamma,
                LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            ),
        ] {
            assert!(
                !parent.has_multiple_populated_kinds(),
                "single_slot({populated:?}) must NOT be has_multiple_populated_kinds",
            );
            assert!(
                parent.has_multiple_missing_kinds(),
                "single_slot({populated:?}) must be has_multiple_missing_kinds (2 missing on ALL.len() == 3)",
            );
            assert_eq!(
                parent.has_multiple_populated_kinds(),
                parent.populated_kind_count() >= 2,
            );
            assert_eq!(
                parent.has_multiple_missing_kinds(),
                parent.missing_kind_count() >= 2,
            );
            // Trichotomy partition — well-formed arm satisfies
            // `has_unique_populated_kind` on the populated axis and
            // `has_multiple_missing_kinds` on the missing axis.
            assert_eq!(
                usize::from(parent.is_empty())
                    + usize::from(parent.has_unique_populated_kind())
                    + usize::from(parent.has_multiple_populated_kinds()),
                1,
                "populated-axis trichotomy must be exactly-one on single_slot({populated:?})",
            );
            assert_eq!(
                usize::from(parent.is_saturated())
                    + usize::from(parent.has_unique_missing_kind())
                    + usize::from(parent.has_multiple_missing_kinds()),
                1,
                "missing-axis trichotomy must be exactly-one on single_slot({populated:?})",
            );
        }
    }

    /// NEAR-SATURATED (two-slot) pin — a `LocalParent` with exactly
    /// two slots populated returns `true` at `has_multiple_populated_kinds`
    /// (two populated) AND `false` at `has_multiple_missing_kinds`
    /// (one missing on `ALL.len() == 3`).
    #[test]
    fn tagged_union_default_has_multiple_kinds_on_near_saturated_parent() {
        for parent in [
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                ..Default::default()
            },
            LocalParent {
                alpha: Some(1),
                gamma: Some(3),
                ..Default::default()
            },
            LocalParent {
                beta: Some(2),
                gamma: Some(3),
                ..Default::default()
            },
        ] {
            assert!(
                parent.has_multiple_populated_kinds(),
                "near-saturated parent must be has_multiple_populated_kinds (2 populated)",
            );
            assert!(
                !parent.has_multiple_missing_kinds(),
                "near-saturated parent must NOT be has_multiple_missing_kinds (1 missing)",
            );
            assert_eq!(
                parent.has_multiple_populated_kinds(),
                parent.populated_kind_count() >= 2,
            );
            assert_eq!(
                parent.has_multiple_missing_kinds(),
                parent.missing_kind_count() >= 2,
            );
            // Trichotomy partition — near-saturated arm satisfies
            // `has_multiple_populated_kinds` on the populated axis and
            // `has_unique_missing_kind` on the missing axis.
            assert_eq!(
                usize::from(parent.is_empty())
                    + usize::from(parent.has_unique_populated_kind())
                    + usize::from(parent.has_multiple_populated_kinds()),
                1,
                "populated-axis trichotomy must be exactly-one on near-saturated arm",
            );
            assert_eq!(
                usize::from(parent.is_saturated())
                    + usize::from(parent.has_unique_missing_kind())
                    + usize::from(parent.has_multiple_missing_kinds()),
                1,
                "missing-axis trichotomy must be exactly-one on near-saturated arm",
            );
        }
    }

    /// SATURATED-PARENT pin — a `LocalParent` with EVERY slot
    /// populated returns `true` at `has_multiple_populated_kinds`
    /// (three populated) AND `false` at `has_multiple_missing_kinds`
    /// (zero missing).
    #[test]
    fn tagged_union_default_has_multiple_kinds_on_saturated_parent() {
        let p = LocalParent {
            alpha: Some(1),
            beta: Some(2),
            gamma: Some(3),
        };
        assert!(
            p.has_multiple_populated_kinds(),
            "saturated parent must be has_multiple_populated_kinds (three populated)",
        );
        assert!(
            !p.has_multiple_missing_kinds(),
            "saturated parent must NOT be has_multiple_missing_kinds (zero missing)",
        );
        assert_eq!(
            p.has_multiple_populated_kinds(),
            p.populated_kind_count() >= 2
        );
        assert_eq!(p.has_multiple_missing_kinds(), p.missing_kind_count() >= 2);
        // Trichotomy partition — saturated arm satisfies
        // `has_multiple_populated_kinds` on the populated axis and
        // `is_saturated` on the missing axis.
        assert_eq!(
            usize::from(p.is_empty())
                + usize::from(p.has_unique_populated_kind())
                + usize::from(p.has_multiple_populated_kinds()),
            1,
            "populated-axis trichotomy must be exactly-one on saturated arm",
        );
        assert_eq!(
            usize::from(p.is_saturated())
                + usize::from(p.has_unique_missing_kind())
                + usize::from(p.has_multiple_missing_kinds()),
            1,
            "missing-axis trichotomy must be exactly-one on saturated arm",
        );
    }

    /// The `assert_is_empty_matches_populated_kind_count` primitive
    /// accepts the [`LocalParent`] scaffold coherently.
    #[test]
    fn assert_is_empty_matches_populated_kind_count_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_is_empty_matches_populated_kind_count::<LocalParent, _, _>(
            make_local,
            LocalParent::default,
        );
    }

    /// A factory that yields an all-empty parent on the single-slot
    /// diagonal (so `is_empty()` returns `true` when the diagonal
    /// contract requires `false`) MUST fail-loudly at the caller's
    /// site through the primitive's single-slot-diagonal arm — a
    /// regression that dropped the `!is_empty` assertion on the
    /// well-formed arm surfaces here.
    #[test]
    #[should_panic(expected = "must equal false")]
    fn assert_is_empty_matches_populated_kind_count_rejects_empty_factory() {
        fn empty_factory(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_is_empty_matches_populated_kind_count::<LocalParent, _, _>(
            empty_factory,
            LocalParent::default,
        );
    }

    /// A factory that yields a NON-empty parent from `empty_parent()`
    /// (so `is_empty()` returns `false` when the baseline contract
    /// requires `true`) MUST fail-loudly at the caller's site through
    /// the primitive's baseline arm — a regression that dropped the
    /// empty-parent baseline assertion surfaces here.
    #[test]
    #[should_panic(expected = "on empty_parent() must equal true")]
    fn assert_is_empty_matches_populated_kind_count_rejects_non_empty_baseline() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        fn non_empty_baseline() -> LocalParent {
            LocalParent {
                alpha: Some(999),
                ..Default::default()
            }
        }
        assert_is_empty_matches_populated_kind_count::<LocalParent, _, _>(
            make_local,
            non_empty_baseline,
        );
    }

    /// The `assert_is_saturated_matches_missing_kind_count` primitive
    /// accepts the [`LocalParent`] scaffold coherently.
    #[test]
    fn assert_is_saturated_matches_missing_kind_count_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_is_saturated_matches_missing_kind_count::<LocalParent, _, _>(
            make_local,
            LocalParent::default,
        );
    }

    /// A factory that yields a saturated parent on the single-slot
    /// diagonal (so `is_saturated()` returns `true` when the diagonal
    /// contract requires `false`) MUST fail-loudly at the caller's
    /// site through the primitive's single-slot-diagonal arm.
    #[test]
    #[should_panic(expected = "must equal false")]
    fn assert_is_saturated_matches_missing_kind_count_rejects_saturated_factory() {
        fn saturated_factory(_: LocalKind) -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            }
        }
        assert_is_saturated_matches_missing_kind_count::<LocalParent, _, _>(
            saturated_factory,
            LocalParent::default,
        );
    }

    /// A factory that yields a saturated parent from `empty_parent()`
    /// (so `is_saturated()` returns `true` when the baseline contract
    /// requires `false`) MUST fail-loudly at the caller's site through
    /// the primitive's baseline arm.
    #[test]
    #[should_panic(expected = "on empty_parent() must equal false")]
    fn assert_is_saturated_matches_missing_kind_count_rejects_saturated_baseline() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        fn saturated_baseline() -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            }
        }
        assert_is_saturated_matches_missing_kind_count::<LocalParent, _, _>(
            make_local,
            saturated_baseline,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the zero-populated-cardinality
    /// Boolean primitive coherently — every per-site `single_slot_X(k)`
    /// factory produces a `!is_empty()` parent, and
    /// `X::default().is_empty() == true` on the empty-parent baseline.
    #[test]
    fn every_production_tagged_union_binds_through_the_is_empty_testkit_primitive() {
        assert_is_empty_matches_populated_kind_count::<crate::intent::Intent, _, _>(
            single_slot_intent_probe,
            crate::intent::Intent::default,
        );
        assert_is_empty_matches_populated_kind_count::<crate::encapsulates::EncapsulationKind, _, _>(
            single_slot_encapsulation_kind_probe,
            crate::encapsulates::EncapsulationKind::default,
        );
        assert_is_empty_matches_populated_kind_count::<crate::export::ArtifactSource, _, _>(
            single_slot_artifact_source_probe,
            crate::export::ArtifactSource::default,
        );
        assert_is_empty_matches_populated_kind_count::<crate::export::VectorChannel, _, _>(
            single_slot_vector_channel_probe,
            crate::export::VectorChannel::default,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the zero-missing-cardinality
    /// Boolean primitive coherently — every per-site `single_slot_X(k)`
    /// factory produces a `!is_saturated()` parent (there are ≥ 2
    /// missing slots on every real-world tagged union in the
    /// workspace), and `X::default().is_saturated() == false` on the
    /// empty-parent baseline.
    #[test]
    fn every_production_tagged_union_binds_through_the_is_saturated_testkit_primitive() {
        assert_is_saturated_matches_missing_kind_count::<crate::intent::Intent, _, _>(
            single_slot_intent_probe,
            crate::intent::Intent::default,
        );
        assert_is_saturated_matches_missing_kind_count::<
            crate::encapsulates::EncapsulationKind,
            _,
            _,
        >(
            single_slot_encapsulation_kind_probe,
            crate::encapsulates::EncapsulationKind::default,
        );
        assert_is_saturated_matches_missing_kind_count::<crate::export::ArtifactSource, _, _>(
            single_slot_artifact_source_probe,
            crate::export::ArtifactSource::default,
        );
        assert_is_saturated_matches_missing_kind_count::<crate::export::VectorChannel, _, _>(
            single_slot_vector_channel_probe,
            crate::export::VectorChannel::default,
        );
    }

    /// The `assert_has_any_populated_kind_matches_populated_kind_count`
    /// primitive accepts the [`LocalParent`] scaffold coherently.
    #[test]
    fn assert_has_any_populated_kind_matches_populated_kind_count_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_has_any_populated_kind_matches_populated_kind_count::<LocalParent, _, _>(
            make_local,
            LocalParent::default,
        );
    }

    /// A factory that yields an all-empty parent on the single-slot
    /// diagonal (so `has_any_populated_kind()` returns `false` when the
    /// diagonal contract requires `true`) MUST fail-loudly at the
    /// caller's site through the primitive's single-slot-diagonal arm.
    #[test]
    #[should_panic(expected = "must equal true")]
    fn assert_has_any_populated_kind_matches_populated_kind_count_rejects_empty_factory() {
        fn empty_factory(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_has_any_populated_kind_matches_populated_kind_count::<LocalParent, _, _>(
            empty_factory,
            LocalParent::default,
        );
    }

    /// A factory that yields a NON-empty parent from `empty_parent()`
    /// (so `has_any_populated_kind()` returns `true` when the baseline
    /// contract requires `false`) MUST fail-loudly at the caller's site
    /// through the primitive's baseline arm.
    #[test]
    #[should_panic(expected = "on empty_parent() must equal false")]
    fn assert_has_any_populated_kind_matches_populated_kind_count_rejects_non_empty_baseline() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        fn non_empty_baseline() -> LocalParent {
            LocalParent {
                alpha: Some(999),
                ..Default::default()
            }
        }
        assert_has_any_populated_kind_matches_populated_kind_count::<LocalParent, _, _>(
            make_local,
            non_empty_baseline,
        );
    }

    /// The `assert_has_any_missing_kind_matches_missing_kind_count`
    /// primitive accepts the [`LocalParent`] scaffold coherently.
    /// `LocalKind::ALL.len() == 3` so a well-formed single-slot parent
    /// has `3 - 1 == 2` missing slots, meaning `has_any_missing_kind()
    /// == true` on the diagonal.
    #[test]
    fn assert_has_any_missing_kind_matches_missing_kind_count_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_has_any_missing_kind_matches_missing_kind_count::<LocalParent, _, _>(
            make_local,
            LocalParent::default,
        );
    }

    /// A factory that yields a saturated parent on the single-slot
    /// diagonal (so `has_any_missing_kind()` returns `false` when the
    /// diagonal contract requires `true`) MUST fail-loudly at the
    /// caller's site through the primitive's single-slot-diagonal arm.
    #[test]
    #[should_panic(expected = "must equal true")]
    fn assert_has_any_missing_kind_matches_missing_kind_count_rejects_saturated_factory() {
        fn saturated_factory(_: LocalKind) -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            }
        }
        assert_has_any_missing_kind_matches_missing_kind_count::<LocalParent, _, _>(
            saturated_factory,
            LocalParent::default,
        );
    }

    /// A factory that yields a saturated parent from `empty_parent()`
    /// (so `has_any_missing_kind()` returns `false` when the baseline
    /// contract requires `true`) MUST fail-loudly at the caller's site
    /// through the primitive's baseline arm.
    #[test]
    #[should_panic(expected = "on empty_parent() must equal true")]
    fn assert_has_any_missing_kind_matches_missing_kind_count_rejects_saturated_baseline() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        fn saturated_baseline() -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            }
        }
        assert_has_any_missing_kind_matches_missing_kind_count::<LocalParent, _, _>(
            make_local,
            saturated_baseline,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the at-least-one-populated-
    /// cardinality Boolean primitive coherently — every per-site
    /// `single_slot_X(k)` factory produces a `has_any_populated_kind()
    /// == true` parent, and `X::default().has_any_populated_kind() ==
    /// false` on the empty-parent baseline.
    #[test]
    fn every_production_tagged_union_binds_through_the_has_any_populated_kind_testkit_primitive() {
        assert_has_any_populated_kind_matches_populated_kind_count::<crate::intent::Intent, _, _>(
            single_slot_intent_probe,
            crate::intent::Intent::default,
        );
        assert_has_any_populated_kind_matches_populated_kind_count::<
            crate::encapsulates::EncapsulationKind,
            _,
            _,
        >(
            single_slot_encapsulation_kind_probe,
            crate::encapsulates::EncapsulationKind::default,
        );
        assert_has_any_populated_kind_matches_populated_kind_count::<
            crate::export::ArtifactSource,
            _,
            _,
        >(
            single_slot_artifact_source_probe,
            crate::export::ArtifactSource::default,
        );
        assert_has_any_populated_kind_matches_populated_kind_count::<
            crate::export::VectorChannel,
            _,
            _,
        >(
            single_slot_vector_channel_probe,
            crate::export::VectorChannel::default,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the at-least-one-missing-
    /// cardinality Boolean primitive coherently — every per-site
    /// `single_slot_X(k)` factory produces a `has_any_missing_kind() ==
    /// true` parent (there are ≥ 2 missing slots on every real-world
    /// tagged union in the workspace, since `ALL.len() ≥ 2`), and
    /// `X::default().has_any_missing_kind() == true` on the empty-
    /// parent baseline (every slot is missing).
    #[test]
    fn every_production_tagged_union_binds_through_the_has_any_missing_kind_testkit_primitive() {
        assert_has_any_missing_kind_matches_missing_kind_count::<crate::intent::Intent, _, _>(
            single_slot_intent_probe,
            crate::intent::Intent::default,
        );
        assert_has_any_missing_kind_matches_missing_kind_count::<
            crate::encapsulates::EncapsulationKind,
            _,
            _,
        >(
            single_slot_encapsulation_kind_probe,
            crate::encapsulates::EncapsulationKind::default,
        );
        assert_has_any_missing_kind_matches_missing_kind_count::<crate::export::ArtifactSource, _, _>(
            single_slot_artifact_source_probe,
            crate::export::ArtifactSource::default,
        );
        assert_has_any_missing_kind_matches_missing_kind_count::<crate::export::VectorChannel, _, _>(
            single_slot_vector_channel_probe,
            crate::export::VectorChannel::default,
        );
    }

    /// The `assert_has_unique_populated_kind_matches_populated_kind_count`
    /// primitive accepts the [`LocalParent`] scaffold coherently.
    #[test]
    fn assert_has_unique_populated_kind_matches_populated_kind_count_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_has_unique_populated_kind_matches_populated_kind_count::<LocalParent, _, _>(
            make_local,
            LocalParent::default,
        );
    }

    /// A factory that yields an all-empty parent on the single-slot
    /// diagonal (so `has_unique_populated_kind()` returns `false` when
    /// the diagonal contract requires `true`) MUST fail-loudly at the
    /// caller's site through the primitive's single-slot-diagonal arm.
    #[test]
    #[should_panic(expected = "must equal true")]
    fn assert_has_unique_populated_kind_matches_populated_kind_count_rejects_empty_factory() {
        fn empty_factory(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_has_unique_populated_kind_matches_populated_kind_count::<LocalParent, _, _>(
            empty_factory,
            LocalParent::default,
        );
    }

    /// A factory that yields a WELL-FORMED parent from `empty_parent()`
    /// (so `has_unique_populated_kind()` returns `true` when the
    /// baseline contract requires `false`) MUST fail-loudly at the
    /// caller's site through the primitive's baseline arm.
    #[test]
    #[should_panic(expected = "on empty_parent() must equal false")]
    fn assert_has_unique_populated_kind_matches_populated_kind_count_rejects_wellformed_baseline() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        fn wellformed_baseline() -> LocalParent {
            LocalParent {
                alpha: Some(999),
                ..Default::default()
            }
        }
        assert_has_unique_populated_kind_matches_populated_kind_count::<LocalParent, _, _>(
            make_local,
            wellformed_baseline,
        );
    }

    /// The `assert_has_unique_missing_kind_matches_missing_kind_count`
    /// primitive accepts the [`LocalParent`] scaffold coherently.
    /// `LocalKind::ALL.len() == 3` so a well-formed single-slot parent
    /// has `3 - 1 == 2` missing slots, meaning
    /// `has_unique_missing_kind() == false` on the diagonal.
    #[test]
    fn assert_has_unique_missing_kind_matches_missing_kind_count_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_has_unique_missing_kind_matches_missing_kind_count::<LocalParent, _, _>(
            make_local,
            LocalParent::default,
        );
    }

    /// A factory that yields a NEAR-SATURATED (two-slot) parent on the
    /// single-slot diagonal — so `has_unique_missing_kind()` returns
    /// `true` (exactly one missing on an `ALL.len() == 3` closed set)
    /// when the diagonal contract on this scaffold requires `false`
    /// (a well-formed one-slot parent has two missing, not one) — MUST
    /// fail-loudly at the caller's site through the primitive's
    /// single-slot-diagonal arm.
    #[test]
    #[should_panic(expected = "must equal false")]
    fn assert_has_unique_missing_kind_matches_missing_kind_count_rejects_near_saturated_factory() {
        fn near_saturated(k: LocalKind) -> LocalParent {
            // Populate two slots regardless of `k`, leaving exactly one
            // missing — mimics a factory that "helpfully" pre-populates
            // extras and drifts off the well-formed diagonal.
            let mut p = LocalParent {
                alpha: Some(1),
                beta: Some(2),
                ..Default::default()
            };
            if let LocalKind::Gamma = k {
                p.gamma = Some(3);
                // Now saturated — drop back to two-slot by clearing
                // alpha, so exactly one missing again.
                p.alpha = None;
            }
            p
        }
        assert_has_unique_missing_kind_matches_missing_kind_count::<LocalParent, _, _>(
            near_saturated,
            LocalParent::default,
        );
    }

    /// A factory that yields a NEAR-SATURATED parent from
    /// `empty_parent()` (so `has_unique_missing_kind()` returns `true`
    /// when the baseline contract requires `false`) MUST fail-loudly
    /// at the caller's site through the primitive's baseline arm.
    #[test]
    #[should_panic(expected = "on empty_parent() must equal false")]
    fn assert_has_unique_missing_kind_matches_missing_kind_count_rejects_near_saturated_baseline() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        fn near_saturated_baseline() -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                ..Default::default()
            }
        }
        assert_has_unique_missing_kind_matches_missing_kind_count::<LocalParent, _, _>(
            make_local,
            near_saturated_baseline,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the one-populated-cardinality
    /// Boolean primitive coherently — every per-site `single_slot_X(k)`
    /// factory produces a `has_unique_populated_kind() == true` parent,
    /// and `X::default().has_unique_populated_kind() == false` on the
    /// empty-parent baseline.
    #[test]
    fn every_production_tagged_union_binds_through_the_has_unique_populated_kind_testkit_primitive()
    {
        assert_has_unique_populated_kind_matches_populated_kind_count::<crate::intent::Intent, _, _>(
            single_slot_intent_probe,
            crate::intent::Intent::default,
        );
        assert_has_unique_populated_kind_matches_populated_kind_count::<
            crate::encapsulates::EncapsulationKind,
            _,
            _,
        >(
            single_slot_encapsulation_kind_probe,
            crate::encapsulates::EncapsulationKind::default,
        );
        assert_has_unique_populated_kind_matches_populated_kind_count::<
            crate::export::ArtifactSource,
            _,
            _,
        >(
            single_slot_artifact_source_probe,
            crate::export::ArtifactSource::default,
        );
        assert_has_unique_populated_kind_matches_populated_kind_count::<
            crate::export::VectorChannel,
            _,
            _,
        >(
            single_slot_vector_channel_probe,
            crate::export::VectorChannel::default,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the one-missing-cardinality Boolean
    /// primitive coherently — every per-site `single_slot_X(k)` factory
    /// produces a `has_unique_missing_kind() == false` parent (there
    /// are ≥ 2 missing slots on every real-world tagged union in the
    /// workspace: `Intent` `ALL.len() == 6`, `EncapsulationKind` `>= 3`,
    /// `ArtifactSource` `>= 3`, `VectorChannel` `>= 3`), and
    /// `X::default().has_unique_missing_kind() == false` on the empty-
    /// parent baseline (every slot missing, not exactly one).
    #[test]
    fn every_production_tagged_union_binds_through_the_has_unique_missing_kind_testkit_primitive() {
        assert_has_unique_missing_kind_matches_missing_kind_count::<crate::intent::Intent, _, _>(
            single_slot_intent_probe,
            crate::intent::Intent::default,
        );
        assert_has_unique_missing_kind_matches_missing_kind_count::<
            crate::encapsulates::EncapsulationKind,
            _,
            _,
        >(
            single_slot_encapsulation_kind_probe,
            crate::encapsulates::EncapsulationKind::default,
        );
        assert_has_unique_missing_kind_matches_missing_kind_count::<
            crate::export::ArtifactSource,
            _,
            _,
        >(
            single_slot_artifact_source_probe,
            crate::export::ArtifactSource::default,
        );
        assert_has_unique_missing_kind_matches_missing_kind_count::<
            crate::export::VectorChannel,
            _,
            _,
        >(
            single_slot_vector_channel_probe,
            crate::export::VectorChannel::default,
        );
    }

    // -------------------------------------------------------------------
    // `assert_has_multiple_(populated|missing)_kinds_matches_(populated|
    // missing)_kind_count` — the ≥2-cardinality Boolean testkit
    // primitives. Pin acceptance on the coherent LocalParent scaffold +
    // rejection on the two obvious factory drifts + a production sweep
    // binding all four `.variant()` sites through the trichotomy law
    // (is_empty + has_unique_populated_kind + has_multiple_populated_kinds
    // == 1 on every arm, and the missing-axis peer).
    // -------------------------------------------------------------------

    /// The `assert_has_multiple_populated_kinds_matches_populated_kind_count`
    /// primitive accepts the [`LocalParent`] scaffold coherently — the
    /// coherent-impl side has no false-positive drift on the empty
    /// baseline, the single-slot diagonal, or the two-slot sweep.
    #[test]
    fn assert_has_multiple_populated_kinds_matches_populated_kind_count_accepts_coherent_local_impl(
    ) {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        assert_has_multiple_populated_kinds_matches_populated_kind_count::<LocalParent, _, _, _>(
            single_slot,
            two_slot,
            LocalParent::default,
        );
    }

    /// The primitive rejects an `empty_parent` factory that yields a
    /// two-slot parent (baseline expects zero-populated on empty).
    #[test]
    #[should_panic(
        expected = "TaggedUnion::has_multiple_populated_kinds() on empty_parent() must equal false"
    )]
    fn assert_has_multiple_populated_kinds_matches_populated_kind_count_rejects_two_slot_baseline()
    {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        fn two_slot_baseline() -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                ..Default::default()
            }
        }
        assert_has_multiple_populated_kinds_matches_populated_kind_count::<LocalParent, _, _, _>(
            single_slot,
            two_slot,
            two_slot_baseline,
        );
    }

    /// The primitive rejects a `two_slot` factory that yields a
    /// single-slot parent (two-slot sweep expects has_multiple ==
    /// true).
    #[test]
    #[should_panic(expected = "TaggedUnion::has_multiple_populated_kinds() on two_slot(")]
    fn assert_has_multiple_populated_kinds_matches_populated_kind_count_rejects_single_slot_two_slot_factory(
    ) {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn drifted_two_slot(a: LocalKind, _: LocalKind) -> LocalParent {
            // Only populates the first slot — the two-slot invariant
            // is violated.
            single_slot(a)
        }
        assert_has_multiple_populated_kinds_matches_populated_kind_count::<LocalParent, _, _, _>(
            single_slot,
            drifted_two_slot,
            LocalParent::default,
        );
    }

    /// The `assert_has_multiple_missing_kinds_matches_missing_kind_count`
    /// primitive accepts the [`LocalParent`] scaffold coherently.
    #[test]
    fn assert_has_multiple_missing_kinds_matches_missing_kind_count_accepts_coherent_local_impl() {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        assert_has_multiple_missing_kinds_matches_missing_kind_count::<LocalParent, _, _, _>(
            single_slot,
            two_slot,
            LocalParent::default,
        );
    }

    /// The primitive rejects a `two_slot` factory that yields an
    /// empty parent (two-slot expects `ALL.len() - 2 == 1` missing
    /// on `LocalParent`, whose composition law asserts
    /// `has_multiple_missing_kinds() == false`; an empty factory
    /// yields `ALL.len() == 3` missing where the primitive returns
    /// `true` — the composition law and the trichotomy both drift).
    #[test]
    #[should_panic]
    fn assert_has_multiple_missing_kinds_matches_missing_kind_count_rejects_empty_two_slot_factory()
    {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn empty_two_slot(_: LocalKind, _: LocalKind) -> LocalParent {
            // Always yields an empty parent — zero populated, three
            // missing. The two-slot invariant is violated.
            LocalParent::default()
        }
        assert_has_multiple_missing_kinds_matches_missing_kind_count::<LocalParent, _, _, _>(
            single_slot,
            empty_two_slot,
            LocalParent::default,
        );
    }

    /// The primitive rejects a `empty_parent` factory that yields a
    /// saturated parent (empty baseline expects has_multiple_missing
    /// == true on ALL.len() == 3 since 3 missing >= 2).
    #[test]
    #[should_panic]
    fn assert_has_multiple_missing_kinds_matches_missing_kind_count_rejects_saturated_empty_baseline(
    ) {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        fn saturated_baseline() -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            }
        }
        assert_has_multiple_missing_kinds_matches_missing_kind_count::<LocalParent, _, _, _>(
            single_slot,
            two_slot,
            saturated_baseline,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the many-cardinality Boolean
    /// primitive on the populated axis coherently — every per-site
    /// `single_slot_X(k)` factory produces `has_multiple_populated_kinds()
    /// == false`, every `two_slot_X(a, b)` produces `== true`, and
    /// `X::default().has_multiple_populated_kinds() == false`. The
    /// trichotomy partition law (`is_empty` plus `has_unique_populated_kind`
    /// plus `has_multiple_populated_kinds` sums to `1`) is pinned inside
    /// the testkit on every arm.
    #[test]
    fn every_production_tagged_union_binds_through_the_has_multiple_populated_kinds_testkit_primitive(
    ) {
        assert_has_multiple_populated_kinds_matches_populated_kind_count::<
            crate::intent::Intent,
            _,
            _,
            _,
        >(
            single_slot_intent_probe,
            two_slot_intent_probe,
            crate::intent::Intent::default,
        );
        assert_has_multiple_populated_kinds_matches_populated_kind_count::<
            crate::encapsulates::EncapsulationKind,
            _,
            _,
            _,
        >(
            single_slot_encapsulation_kind_probe,
            two_slot_encapsulation_kind_probe,
            crate::encapsulates::EncapsulationKind::default,
        );
        assert_has_multiple_populated_kinds_matches_populated_kind_count::<
            crate::export::ArtifactSource,
            _,
            _,
            _,
        >(
            single_slot_artifact_source_probe,
            two_slot_artifact_source_probe,
            crate::export::ArtifactSource::default,
        );
        assert_has_multiple_populated_kinds_matches_populated_kind_count::<
            crate::export::VectorChannel,
            _,
            _,
            _,
        >(
            single_slot_vector_channel_probe,
            two_slot_vector_channel_probe,
            crate::export::VectorChannel::default,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the many-cardinality Boolean
    /// primitive on the missing axis coherently. On `Intent`
    /// (`ALL.len() == 6`), `EncapsulationKind` (`>= 3`),
    /// `ArtifactSource` (`>= 3`), `VectorChannel` (`>= 3`), the
    /// single-slot diagonal returns `true` (`ALL.len() - 1 >= 2`);
    /// on `Intent` (`ALL.len() == 6 >= 4`) the two-slot sweep also
    /// returns `true`. The trichotomy partition law on the missing
    /// axis (`is_saturated + has_unique_missing_kind +
    /// has_multiple_missing_kinds == 1`) is pinned inside the testkit
    /// on every arm.
    #[test]
    fn every_production_tagged_union_binds_through_the_has_multiple_missing_kinds_testkit_primitive(
    ) {
        assert_has_multiple_missing_kinds_matches_missing_kind_count::<
            crate::intent::Intent,
            _,
            _,
            _,
        >(
            single_slot_intent_probe,
            two_slot_intent_probe,
            crate::intent::Intent::default,
        );
        assert_has_multiple_missing_kinds_matches_missing_kind_count::<
            crate::encapsulates::EncapsulationKind,
            _,
            _,
            _,
        >(
            single_slot_encapsulation_kind_probe,
            two_slot_encapsulation_kind_probe,
            crate::encapsulates::EncapsulationKind::default,
        );
        assert_has_multiple_missing_kinds_matches_missing_kind_count::<
            crate::export::ArtifactSource,
            _,
            _,
            _,
        >(
            single_slot_artifact_source_probe,
            two_slot_artifact_source_probe,
            crate::export::ArtifactSource::default,
        );
        assert_has_multiple_missing_kinds_matches_missing_kind_count::<
            crate::export::VectorChannel,
            _,
            _,
            _,
        >(
            single_slot_vector_channel_probe,
            two_slot_vector_channel_probe,
            crate::export::VectorChannel::default,
        );
    }

    /// The `assert_is_partially_populated_matches_cardinality` primitive
    /// accepts the [`LocalParent`] scaffold coherently — the middle-arm
    /// Boolean projection reads `true` on every single-slot and two-slot
    /// arrangement (0 < populated < 3) and `false` on the empty
    /// baseline (0 populated), and the parent-state trichotomy partition
    /// (`is_empty + is_partially_populated + is_saturated == 1`) holds
    /// on every arm.
    #[test]
    fn assert_is_partially_populated_matches_cardinality_accepts_coherent_local_impl() {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        assert_is_partially_populated_matches_cardinality::<LocalParent, _, _, _>(
            single_slot,
            two_slot,
            LocalParent::default,
        );
    }

    /// The primitive rejects a `single_slot` factory that yields an
    /// empty parent (single-slot expects `is_partially_populated() ==
    /// true` because on `ALL.len() == 3` a well-formed parent has
    /// `1 populated + 2 missing` — but an empty factory yields 0
    /// populated, so the middle-arm assertion drifts).
    #[test]
    #[should_panic(expected = "must equal true")]
    fn assert_is_partially_populated_matches_cardinality_rejects_empty_single_slot_factory() {
        fn empty_single_slot(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        assert_is_partially_populated_matches_cardinality::<LocalParent, _, _, _>(
            empty_single_slot,
            two_slot,
            LocalParent::default,
        );
    }

    /// The primitive rejects an `empty_parent` factory that yields a
    /// saturated parent (empty baseline expects
    /// `is_partially_populated() == false` because 0 populated is the
    /// empty arm — a saturated factory has `ALL.len()` populated + 0
    /// missing, which is ALSO the `false` arm of the middle Boolean
    /// but drifts on the trichotomy partition since
    /// `is_saturated == true` while the primitive expected
    /// `is_empty == true` on the baseline).
    #[test]
    #[should_panic]
    fn assert_is_partially_populated_matches_cardinality_rejects_saturated_empty_baseline() {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        fn saturated_baseline() -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            }
        }
        assert_is_partially_populated_matches_cardinality::<LocalParent, _, _, _>(
            single_slot,
            two_slot,
            saturated_baseline,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the parent-state-middle-arm Boolean
    /// primitive coherently — every per-site `single_slot_X(k)` factory
    /// produces `is_partially_populated() == true` (well-formed has
    /// `1 populated + ALL.len() - 1 ≥ 1 missing`), every
    /// `two_slot_X(a, b)` produces `== true` (`ALL.len() ≥ 3` on every
    /// production union so two_slot has `2 populated + ALL.len() - 2
    /// ≥ 1 missing`), and `X::default().is_partially_populated() ==
    /// false` on the empty-parent baseline. The parent-state
    /// trichotomy partition law (`is_empty + is_partially_populated
    /// + is_saturated == 1`) is pinned inside the testkit on every arm.
    #[test]
    fn every_production_tagged_union_binds_through_the_is_partially_populated_testkit_primitive() {
        assert_is_partially_populated_matches_cardinality::<crate::intent::Intent, _, _, _>(
            single_slot_intent_probe,
            two_slot_intent_probe,
            crate::intent::Intent::default,
        );
        assert_is_partially_populated_matches_cardinality::<
            crate::encapsulates::EncapsulationKind,
            _,
            _,
            _,
        >(
            single_slot_encapsulation_kind_probe,
            two_slot_encapsulation_kind_probe,
            crate::encapsulates::EncapsulationKind::default,
        );
        assert_is_partially_populated_matches_cardinality::<crate::export::ArtifactSource, _, _, _>(
            single_slot_artifact_source_probe,
            two_slot_artifact_source_probe,
            crate::export::ArtifactSource::default,
        );
        assert_is_partially_populated_matches_cardinality::<crate::export::VectorChannel, _, _, _>(
            single_slot_vector_channel_probe,
            two_slot_vector_channel_probe,
            crate::export::VectorChannel::default,
        );
    }

    /// The `assert_has_only_matches_unique_populated_kind` primitive
    /// accepts the [`LocalParent`] scaffold coherently — the kind-scoped
    /// strict-refinement predicate reads `true` iff the probed kind
    /// equals the populated kind on every single-slot arrangement (the
    /// diagonal), `false` on every off-diagonal pair regardless of
    /// probed kind, and `false` on the empty baseline for every kind.
    /// The five composition laws (widened uniqueness, cardinality-
    /// refinement, kind-scoped implication, kind-domain exhaustivity,
    /// well-formed diagonal) hold on every arm.
    #[test]
    fn assert_has_only_matches_unique_populated_kind_accepts_coherent_local_impl() {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        assert_has_only_matches_unique_populated_kind::<LocalParent, _, _, _>(
            single_slot,
            two_slot,
            LocalParent::default,
        );
    }

    /// The primitive rejects a `single_slot` factory that populates
    /// the WRONG kind (always `Beta` regardless of what kind is asked
    /// for) — the well-formed diagonal law
    /// `single_slot(k).has_only(k) == true` fails on
    /// `k ∈ {Alpha, Gamma}` where the factory populated `Beta` instead.
    #[test]
    #[should_panic(expected = "must equal true")]
    fn assert_has_only_matches_unique_populated_kind_rejects_wrong_slot_factory() {
        fn always_beta(_: LocalKind) -> LocalParent {
            LocalParent {
                beta: Some(2),
                ..Default::default()
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        assert_has_only_matches_unique_populated_kind::<LocalParent, _, _, _>(
            always_beta,
            two_slot,
            LocalParent::default,
        );
    }

    /// The primitive rejects an `empty_parent` factory that yields a
    /// saturated parent — the empty-baseline exhaustivity assertion
    /// `empty_parent().is_empty() == true` fails on the saturated
    /// baseline, catching a factory that mis-represents the empty arm.
    #[test]
    #[should_panic(expected = "must satisfy is_empty() == true")]
    fn assert_has_only_matches_unique_populated_kind_rejects_saturated_empty_baseline() {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        fn saturated_baseline() -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            }
        }
        assert_has_only_matches_unique_populated_kind::<LocalParent, _, _, _>(
            single_slot,
            two_slot,
            saturated_baseline,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the kind-scoped strict-refinement
    /// Boolean primitive coherently — every per-site `single_slot_X(k)`
    /// factory produces `has_only(k) == true` (well-formed truth table
    /// on the diagonal), every off-diagonal probe returns `false`
    /// (well-formed truth table off the diagonal), every
    /// `two_slot_X(a, b)` produces `has_only(k) == false` for every
    /// `k` (multi-populated arm), and `X::default().has_only(k) ==
    /// false` on the empty baseline for every `k`. The kind-domain
    /// exhaustivity law (`count k where has_only(k) ≤ 1` per parent,
    /// with equality iff well-formed) is pinned inside the testkit on
    /// every arm.
    #[test]
    fn every_production_tagged_union_binds_through_the_has_only_testkit_primitive() {
        assert_has_only_matches_unique_populated_kind::<crate::intent::Intent, _, _, _>(
            single_slot_intent_probe,
            two_slot_intent_probe,
            crate::intent::Intent::default,
        );
        assert_has_only_matches_unique_populated_kind::<
            crate::encapsulates::EncapsulationKind,
            _,
            _,
            _,
        >(
            single_slot_encapsulation_kind_probe,
            two_slot_encapsulation_kind_probe,
            crate::encapsulates::EncapsulationKind::default,
        );
        assert_has_only_matches_unique_populated_kind::<crate::export::ArtifactSource, _, _, _>(
            single_slot_artifact_source_probe,
            two_slot_artifact_source_probe,
            crate::export::ArtifactSource::default,
        );
        assert_has_only_matches_unique_populated_kind::<crate::export::VectorChannel, _, _, _>(
            single_slot_vector_channel_probe,
            two_slot_vector_channel_probe,
            crate::export::VectorChannel::default,
        );
    }

    // -------------------------------------------------------------------
    // `assert_lacks_only_matches_unique_missing_kind` — the closed-set-
    // complement mirror of `assert_has_only_matches_unique_populated_kind`
    // on the MISSING axis. Pin the composition-law truth table
    // (`lacks_only(kind) == (unique_missing_kind() == Some(kind))`,
    // cardinality-refinement under complement, kind-scoped implication
    // under complement, kind-domain exhaustivity ≤ 1) directly on the
    // sibling-shaped `LocalParent` scaffold + on every one of the four
    // production `.variant()` parents — a regression on either the fused
    // walk's negated presence probe, the argument-scoped short-circuit,
    // or the exhaustivity partition fails here before any per-parent
    // consumer surfaces the drift.
    // -------------------------------------------------------------------

    /// The `assert_lacks_only_matches_unique_missing_kind` primitive
    /// accepts the [`LocalParent`] scaffold coherently — the closed-set-
    /// complement mirror of the populated-axis kind-scoped strict-
    /// refinement predicate reads `true` iff the probed kind names the
    /// SOLE missing slot. On `LocalParent`'s `ALL.len() == 3` closed
    /// set: the empty baseline has 3 missing (so `lacks_only(k) ==
    /// false` for every `k`), every single-slot arm has 2 missing (so
    /// `lacks_only(k) == false` for every `k`), and every off-diagonal
    /// two-slot arm has 1 missing — the third kind, where `lacks_only`
    /// returns `true` for that one probe and `false` for the two
    /// populated probes. The four composition laws (widened
    /// uniqueness, cardinality-refinement under complement, kind-
    /// scoped implication under complement, kind-domain exhaustivity)
    /// hold on every arm.
    #[test]
    fn assert_lacks_only_matches_unique_missing_kind_accepts_coherent_local_impl() {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        assert_lacks_only_matches_unique_missing_kind::<LocalParent, _, _, _>(
            single_slot,
            two_slot,
            LocalParent::default,
        );
    }

    /// The primitive rejects a `two_slot` factory that yields a
    /// saturated parent (all three slots populated, zero missing) —
    /// the factory-precondition truth table on the two-slot arm reads
    /// `expected == (k != a && k != b)` for the third kind on
    /// `ALL.len() == 3`, but the saturated factory has zero missing so
    /// `lacks_only(third) == false` where `expected == true`. Caught
    /// by the hard-coded arm expectation BEFORE any composition law
    /// reconciles two internally-drifted trait bodies.
    #[test]
    #[should_panic(expected = "must equal true on ALL.len() == 3")]
    fn assert_lacks_only_matches_unique_missing_kind_rejects_saturated_two_slot_factory() {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn saturated_two_slot(_: LocalKind, _: LocalKind) -> LocalParent {
            // Always yields a saturated parent — zero missing.
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            }
        }
        assert_lacks_only_matches_unique_missing_kind::<LocalParent, _, _, _>(
            single_slot,
            saturated_two_slot,
            LocalParent::default,
        );
    }

    /// The primitive rejects a `empty_parent` factory that yields a
    /// saturated parent — the empty-baseline exhaustivity assertion
    /// `empty_parent().is_empty() == true` fails on the saturated
    /// baseline, catching a factory that mis-represents the empty arm.
    #[test]
    #[should_panic(expected = "must satisfy is_empty() == true")]
    fn assert_lacks_only_matches_unique_missing_kind_rejects_saturated_empty_baseline() {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        fn saturated_baseline() -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            }
        }
        assert_lacks_only_matches_unique_missing_kind::<LocalParent, _, _, _>(
            single_slot,
            two_slot,
            saturated_baseline,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the kind-scoped strict-refinement
    /// Boolean primitive on the MISSING axis coherently — on the
    /// three `ALL.len() == 3` sites (`EncapsulationKind`,
    /// `ArtifactSource`, `VectorChannel`) every off-diagonal
    /// `two_slot_X(a, b)` produces `lacks_only(third) == true` for
    /// exactly the third kind and `lacks_only(k) == false` for the
    /// two populated kinds; on the `ALL.len() == 6` site (`Intent`)
    /// every off-diagonal two-slot arm has 4 missing so `lacks_only(k)
    /// == false` for every `k`. Every single-slot arm on every site
    /// has `ALL.len() - 1 >= 2` missing, so `lacks_only(k) == false`
    /// for every `k`. The `X::default()` empty baseline on every site
    /// has `ALL.len() >= 3` missing, so `lacks_only(k) == false` for
    /// every `k`. The composition-law shape binds every regime
    /// through the same substrate site. The kind-domain exhaustivity
    /// law (`count k where lacks_only(k) ≤ 1` per parent, with
    /// equality iff exactly one slot is missing) is pinned inside the
    /// testkit on every arm.
    #[test]
    fn every_production_tagged_union_binds_through_the_lacks_only_testkit_primitive() {
        assert_lacks_only_matches_unique_missing_kind::<crate::intent::Intent, _, _, _>(
            single_slot_intent_probe,
            two_slot_intent_probe,
            crate::intent::Intent::default,
        );
        assert_lacks_only_matches_unique_missing_kind::<
            crate::encapsulates::EncapsulationKind,
            _,
            _,
            _,
        >(
            single_slot_encapsulation_kind_probe,
            two_slot_encapsulation_kind_probe,
            crate::encapsulates::EncapsulationKind::default,
        );
        assert_lacks_only_matches_unique_missing_kind::<crate::export::ArtifactSource, _, _, _>(
            single_slot_artifact_source_probe,
            two_slot_artifact_source_probe,
            crate::export::ArtifactSource::default,
        );
        assert_lacks_only_matches_unique_missing_kind::<crate::export::VectorChannel, _, _, _>(
            single_slot_vector_channel_probe,
            two_slot_vector_channel_probe,
            crate::export::VectorChannel::default,
        );
    }

    // -------------------------------------------------------------------
    // `assert_lacks_matches_has_complement` — the missing-axis SUBSET
    // primitive testkit. Pin the composition-law truth table
    // (definitional complement, missing-set membership, kind-scoped
    // implication from lacks_only, cardinality partition against
    // missing_kind_count, factory-precondition arm expectation) directly
    // on the sibling-shaped `LocalParent` scaffold + on every one of the
    // four production `.variant()` parents — a regression on either the
    // definitional negation, the missing-set membership projection, or
    // the cardinality partition fails here before any per-parent
    // consumer surfaces the drift.
    // -------------------------------------------------------------------

    /// The `assert_lacks_matches_has_complement` primitive accepts the
    /// [`LocalParent`] scaffold coherently — the closed-set-complement
    /// peer of the kind-scoped SUBSET populated-axis predicate reads
    /// `true` iff the probed kind is missing. On `LocalParent`'s
    /// `ALL.len() == 3` closed set: the empty baseline has 3 missing
    /// (so `lacks(k) == true` for every `k`), every single-slot arm
    /// has 2 missing (so `lacks(k) == true` for every `k != populated`
    /// and `false` for `k == populated`), and every off-diagonal
    /// two-slot arm has 1 missing (so `lacks(k) == true` for the third
    /// kind and `false` for the two populated kinds). The five
    /// composition laws (definitional complement, missing-set
    /// membership, kind-scoped implication from lacks_only,
    /// cardinality partition against missing_kind_count, factory-
    /// precondition arm expectation) hold on every arm.
    #[test]
    fn assert_lacks_matches_has_complement_accepts_coherent_local_impl() {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        assert_lacks_matches_has_complement::<LocalParent, _, _, _>(
            single_slot,
            two_slot,
            LocalParent::default,
        );
    }

    /// The primitive rejects a `single_slot` factory that yields a
    /// saturated parent (all three slots populated, zero missing) —
    /// the factory-precondition truth table on the well-formed
    /// single-slot arm reads `expected == (probed != populated)`, but
    /// the saturated factory has zero missing so `lacks(probed) ==
    /// false` for EVERY probe, mismatching the `true` expectation
    /// on every off-diagonal probe. Caught by the hard-coded arm
    /// expectation BEFORE the definitional complement law reconciles
    /// two internally-drifted trait bodies.
    #[test]
    #[should_panic(expected = "must equal true")]
    fn assert_lacks_matches_has_complement_rejects_saturated_single_slot_factory() {
        fn saturated_single_slot(_: LocalKind) -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        assert_lacks_matches_has_complement::<LocalParent, _, _, _>(
            saturated_single_slot,
            two_slot,
            LocalParent::default,
        );
    }

    /// The primitive rejects an `empty_parent` factory that yields a
    /// saturated parent — the empty-baseline exhaustivity assertion
    /// `empty_parent().is_empty() == true` fails on the saturated
    /// baseline, catching a factory that mis-represents the empty arm
    /// BEFORE any composition law reconciles two internally-drifted
    /// trait bodies.
    #[test]
    #[should_panic(expected = "must satisfy is_empty() == true")]
    fn assert_lacks_matches_has_complement_rejects_saturated_empty_baseline() {
        fn single_slot(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(1),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(2),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(3),
                    ..Default::default()
                },
            }
        }
        fn two_slot(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(1),
                    LocalKind::Beta => p.beta = Some(2),
                    LocalKind::Gamma => p.gamma = Some(3),
                }
            }
            p
        }
        fn saturated_baseline() -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: Some(3),
            }
        }
        assert_lacks_matches_has_complement::<LocalParent, _, _, _>(
            single_slot,
            two_slot,
            saturated_baseline,
        );
    }

    /// Every one of the four production `.variant()` sites on
    /// `ProcessSpec` binds through the closed-set-complement peer of
    /// `has` on the kind-scoped SUBSET axis coherently — every
    /// `single_slot_X(k)` factory produces `lacks(k) == false` on the
    /// diagonal and `lacks(other) == true` off-diagonal, every
    /// `two_slot_X(a, b)` produces `lacks(k) == true` iff `k != a && k
    /// != b`, and `X::default().lacks(k) == true` on the empty
    /// baseline for every `k`. The cardinality-partition law (`count k
    /// where lacks(k) == missing_kind_count()` per parent) is pinned
    /// inside the testkit on every arm.
    #[test]
    fn every_production_tagged_union_binds_through_the_lacks_testkit_primitive() {
        assert_lacks_matches_has_complement::<crate::intent::Intent, _, _, _>(
            single_slot_intent_probe,
            two_slot_intent_probe,
            crate::intent::Intent::default,
        );
        assert_lacks_matches_has_complement::<crate::encapsulates::EncapsulationKind, _, _, _>(
            single_slot_encapsulation_kind_probe,
            two_slot_encapsulation_kind_probe,
            crate::encapsulates::EncapsulationKind::default,
        );
        assert_lacks_matches_has_complement::<crate::export::ArtifactSource, _, _, _>(
            single_slot_artifact_source_probe,
            two_slot_artifact_source_probe,
            crate::export::ArtifactSource::default,
        );
        assert_lacks_matches_has_complement::<crate::export::VectorChannel, _, _, _>(
            single_slot_vector_channel_probe,
            two_slot_vector_channel_probe,
            crate::export::VectorChannel::default,
        );
    }

    // -------------------------------------------------------------------
    // `assert_two_slots_ambiguous` — the ALL×ALL ambiguity sweep as ONE
    // substrate primitive. Pin the truth table (every off-diagonal pair
    // resolves to `TaggedUnionError::ambiguous`, diagonal pairs are
    // skipped, a factory that yields a non-Ambiguous parent fails-loudly
    // at the caller's site) directly on the sibling-shaped `LocalParent`
    // scaffold — a regression on either the pair-iteration order or the
    // expected-carrier composition fails here before any per-parent test
    // surfaces the drift.
    // -------------------------------------------------------------------

    /// Every off-diagonal pair across [`LocalKind::ALL`] × `ALL`
    /// resolves through the substrate primitive to
    /// [`LocalParentError::Ambiguous`] on the sibling-shaped local
    /// scaffold. Pins the primitive's Ok arm (no false positives on the
    /// coherent-impl side) at ONE boundary — a regression that drops
    /// the diagonal skip, mis-iterates `ClosedSet::ALL`, or composes a
    /// divergent expected carrier fails here before any per-parent
    /// inherent test surfaces the drift.
    #[test]
    fn assert_two_slots_ambiguous_accepts_coherent_local_impl() {
        fn two_local(a: LocalKind, b: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            for k in [a, b] {
                match k {
                    LocalKind::Alpha => p.alpha = Some(11),
                    LocalKind::Beta => p.beta = Some(22),
                    LocalKind::Gamma => p.gamma = Some(33),
                }
            }
            p
        }
        assert_two_slots_ambiguous::<LocalParent, _>(two_local);
    }

    /// A factory that yields a single-slot parent for the FIRST kind
    /// (ignoring the second) — every off-diagonal pair resolves to
    /// exactly-one Ok(Variant), NOT Ambiguous — MUST fail-loudly at
    /// the caller's site through the primitive's "two-slot parent
    /// must not resolve to a variant" arm. Pin the Ok-side failure
    /// mode so a regression that mis-routes the substrate primitive's
    /// resolved-Ok arm past the assertion (silently succeeding on a
    /// single-slot factory) is caught here.
    #[test]
    #[should_panic(expected = "two-slot parent must not resolve to a variant")]
    fn assert_two_slots_ambiguous_rejects_factory_that_populates_only_one_slot() {
        fn single_only(a: LocalKind, _: LocalKind) -> LocalParent {
            let mut p = LocalParent::default();
            match a {
                LocalKind::Alpha => p.alpha = Some(11),
                LocalKind::Beta => p.beta = Some(22),
                LocalKind::Gamma => p.gamma = Some(33),
            }
            p
        }
        assert_two_slots_ambiguous::<LocalParent, _>(single_only);
    }

    /// A factory that yields an all-empty parent (so `.variant()`
    /// resolves to the `Empty` carrier, NOT `Ambiguous`) MUST
    /// fail-loudly at the caller's site through the primitive's
    /// `assert_eq!` arm — the composed expected carrier
    /// [`TaggedUnionError::ambiguous`] mismatches the resolved
    /// [`TaggedUnionError::empty`] carrier. Pin the Empty-arm failure
    /// mode so a regression that mis-projects the None arm of
    /// [`ResolveError`] onto Ambiguous (silently succeeding on an
    /// empty factory) is caught here.
    #[test]
    #[should_panic(expected = "should resolve Ambiguous")]
    fn assert_two_slots_ambiguous_rejects_factory_that_populates_no_slots() {
        fn empty_factory(_: LocalKind, _: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_two_slots_ambiguous::<LocalParent, _>(empty_factory);
    }

    // -------------------------------------------------------------------
    // `assert_single_slot_key_matches_label` — the wire-key / kind-label
    // alignment sweep as ONE substrate primitive. Pin the truth table
    // (every populated slot serializes to exactly one JSON key whose
    // name equals the addressing kind's ClosedSet label; a factory that
    // populates the wrong slot / no slot / multiple slots fails-loudly
    // at the caller's site) directly on the sibling-shaped `LocalParent`
    // scaffold — a regression on either the exactly-one arm or the
    // name-equality arm fails here before any per-parent inherent test
    // surfaces the drift.
    // -------------------------------------------------------------------

    /// Every kind across [`LocalKind::ALL`] serializes through the
    /// substrate primitive to a JSON object with EXACTLY ONE key whose
    /// name equals `<LocalKind as ClosedSet>::label` on the addressed
    /// kind. Pins the primitive's Ok arm (no false positives on the
    /// coherent-impl side) at ONE boundary — a regression that inspects
    /// the wrong serde value (e.g. `to_string` instead of `to_value`),
    /// counts fields off-by-one, or projects the wrong `ClosedSet`
    /// method (`labels_joined` instead of `label`) fails here before any
    /// per-parent inherent test surfaces the drift.
    #[test]
    fn assert_single_slot_key_matches_label_accepts_coherent_local_impl() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_single_slot_key_matches_label::<LocalParent, _>(make_local);
    }

    /// A factory that returns a single-slot parent for the WRONG kind
    /// (populates `beta` regardless of what kind is asked for) MUST
    /// fail-loudly at the caller's site through the primitive's
    /// name-equality arm — the emitted key does not match the addressed
    /// kind's label. Pins the drift-detection failure mode so a
    /// regression that drops the `assert_eq!(keys[0], label)` arm
    /// (silently succeeding on any-key-at-all) is caught here. The
    /// caller's site is the `#[should_panic]` boundary through the
    /// primitive's `#[track_caller]` compound-lift.
    #[test]
    #[should_panic(expected = "wire-key drift")]
    fn assert_single_slot_key_matches_label_rejects_factory_that_populates_wrong_slot() {
        fn always_beta(_: LocalKind) -> LocalParent {
            LocalParent {
                beta: Some(22),
                ..Default::default()
            }
        }
        assert_single_slot_key_matches_label::<LocalParent, _>(always_beta);
    }

    /// A factory that returns an all-empty parent (so serializing
    /// yields ZERO keys, not exactly-one) MUST fail-loudly at the
    /// caller's site through the primitive's exactly-one arm. Pins the
    /// zero-key failure mode so a regression that projects
    /// `obj.keys().count() >= 1` (rather than `== 1`) is caught here.
    #[test]
    #[should_panic(expected = "exactly one populated field")]
    fn assert_single_slot_key_matches_label_rejects_factory_that_populates_no_slots() {
        fn empty_factory(_: LocalKind) -> LocalParent {
            LocalParent::default()
        }
        assert_single_slot_key_matches_label::<LocalParent, _>(empty_factory);
    }

    /// A factory that returns a parent with TWO populated slots (so
    /// serializing yields two keys, not exactly-one) MUST fail-loudly
    /// at the caller's site through the primitive's exactly-one arm.
    /// Pins the many-keys failure mode so a regression that projects
    /// `obj.keys().count() <= 1` (rather than `== 1`) is caught here.
    /// Cross-pins the substrate promise that a single-slot factory
    /// truly populates ONE slot — a future factory bug that leaks
    /// residual populated slots between calls (e.g. via shared mutable
    /// state) is caught HERE at the primitive boundary.
    #[test]
    #[should_panic(expected = "exactly one populated field")]
    fn assert_single_slot_key_matches_label_rejects_factory_that_populates_two_slots() {
        fn two_slot_factory(_: LocalKind) -> LocalParent {
            LocalParent {
                alpha: Some(1),
                beta: Some(2),
                gamma: None,
            }
        }
        assert_single_slot_key_matches_label::<LocalParent, _>(two_slot_factory);
    }

    /// The macro-emitted [`MacroLocalParent`] scaffold impls
    /// [`TaggedUnion`] through the [`declare_tagged_union_impls!`]
    /// three-block macro AND additionally derives `serde::Serialize` +
    /// `#[serde(skip_serializing_if = "Option::is_none")]` on every
    /// slot — so the wire-key primitive dispatches on the MACRO-emitted
    /// impl path byte-identically with the hand-rolled [`LocalParent`]
    /// path above. Pins the substrate-wide guarantee that a fifth
    /// sibling landing through the macro picks up the wire-alignment
    /// check for free, without a hand-rolled `TaggedUnion` block, so
    /// long as its serde derives match the substrate-wide
    /// `skip_serializing_if = "Option::is_none"` shape every production
    /// site already carries. A regression that mis-routes the
    /// primitive's serialize call through the WRONG entry point (e.g.
    /// calling a bespoke `to_json` that bypasses serde) is caught here.
    #[test]
    fn assert_single_slot_key_matches_label_accepts_macro_emitted_impl() {
        fn make_macro_local(k: MacroLocalKind) -> MacroLocalParent {
            match k {
                MacroLocalKind::Foo => MacroLocalParent {
                    foo: Some(7),
                    bar: None,
                },
                MacroLocalKind::Bar => MacroLocalParent {
                    foo: None,
                    bar: Some(8),
                },
            }
        }
        assert_single_slot_key_matches_label::<MacroLocalParent, _>(make_macro_local);
    }

    // -------------------------------------------------------------------
    // `assert_wire_key_matches_label` — bound-relaxed peer of the
    // `assert_single_slot_key_matches_label` primitive. Pin the truth
    // table (every populated slot serializes to exactly one JSON key
    // whose name equals the addressing kind's ClosedSet label; a
    // factory that populates the wrong slot / no slot / multiple slots
    // fails-loudly at the caller's site) on a NON-TaggedUnion parent
    // scaffold — the delegation-only path from the trait-projected
    // primitive would silently pass this test if the bound-relaxed
    // primitive's body regressed, so the direct-dispatch probes here
    // pin the bound-relaxed pathway independently.
    // -------------------------------------------------------------------

    /// Local parent that carries the wire-format shape (`Option<T>`
    /// slots + `#[serde(skip_serializing_if = "Option::is_none")]`
    /// annotations) but DELIBERATELY does NOT impl [`TaggedUnion`] —
    /// pins the bound-relaxed sweep on the exact shape [`crate::lifetime::Lifetime`]
    /// carries in production (empty resolves to a default variant,
    /// not to a typed error, so the trait's `T::Error` bound doesn't
    /// hold and the trait-projected surface excludes it).
    #[derive(Default, serde::Serialize)]
    struct BareParent {
        #[serde(skip_serializing_if = "Option::is_none")]
        alpha: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        beta: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        gamma: Option<u32>,
    }

    /// The bound-relaxed primitive dispatches Ok on a coherent
    /// non-TaggedUnion impl — pin the happy path directly on the
    /// [`BareParent`] scaffold so a regression that gates the sweep
    /// body on the `T: TaggedUnion` bound (accidentally re-adding it,
    /// or projecting through `T::Kind` instead of the caller-supplied
    /// `K` generic) fails HERE at the primitive-independent boundary
    /// rather than at the [`crate::lifetime::Lifetime`] production
    /// site alone. The Ok arm is the "no drift" outcome; a divergence
    /// surfaces as a labeled assertion failure at the caller site
    /// (this test's own line) via the primitive's `#[track_caller]`.
    #[test]
    fn assert_wire_key_matches_label_accepts_coherent_bare_parent_impl() {
        fn make_bare(k: LocalKind) -> BareParent {
            match k {
                LocalKind::Alpha => BareParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => BareParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => BareParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        assert_wire_key_matches_label::<BareParent, LocalKind, _>(make_bare);
    }

    /// A factory that returns a bare-parent for the WRONG kind
    /// (populates `beta` regardless of what kind is asked for) MUST
    /// fail-loudly at the caller's site through the bound-relaxed
    /// primitive's name-equality arm — the emitted key does not match
    /// the addressed kind's label. Pins the drift-detection failure
    /// mode on the non-TaggedUnion pathway so a regression that drops
    /// the `assert_eq!(keys[0], label)` arm (silently succeeding on
    /// any-key-at-all) is caught here — mechanical peer of the
    /// sibling `assert_single_slot_key_matches_label_rejects_factory_that_populates_wrong_slot`
    /// on the TaggedUnion pathway.
    #[test]
    #[should_panic(expected = "wire-key drift")]
    fn assert_wire_key_matches_label_rejects_factory_that_populates_wrong_slot() {
        fn always_beta(_: LocalKind) -> BareParent {
            BareParent {
                beta: Some(22),
                ..Default::default()
            }
        }
        assert_wire_key_matches_label::<BareParent, LocalKind, _>(always_beta);
    }

    /// A factory that returns an all-empty bare-parent (so serializing
    /// yields ZERO keys, not exactly-one) MUST fail-loudly at the
    /// caller's site through the bound-relaxed primitive's
    /// exactly-one arm. Pins the zero-key failure mode on the
    /// non-TaggedUnion pathway.
    #[test]
    #[should_panic(expected = "exactly one populated field")]
    fn assert_wire_key_matches_label_rejects_factory_that_populates_no_slots() {
        fn empty_factory(_: LocalKind) -> BareParent {
            BareParent::default()
        }
        assert_wire_key_matches_label::<BareParent, LocalKind, _>(empty_factory);
    }

    /// The trait-projected [`assert_single_slot_key_matches_label`]
    /// is a one-line delegation to the bound-relaxed
    /// [`assert_wire_key_matches_label`] peer — pin the delegation
    /// shape at ONE boundary so a regression that inlines a
    /// divergent sweep body into the trait-projected surface (rather
    /// than the one-line dispatch) is caught here. Ok on a coherent
    /// impl means BOTH primitives dispatch through the SAME body on
    /// the same fixture — [`LocalParent`] impls [`TaggedUnion`], so
    /// both the trait-projected surface and the bound-relaxed peer
    /// reach it, and a divergence between the two dispatches would
    /// surface here as one succeeding + the other failing.
    #[test]
    fn assert_single_slot_key_matches_label_delegates_to_wire_key_matches_label() {
        fn make_local(k: LocalKind) -> LocalParent {
            match k {
                LocalKind::Alpha => LocalParent {
                    alpha: Some(11),
                    ..Default::default()
                },
                LocalKind::Beta => LocalParent {
                    beta: Some(22),
                    ..Default::default()
                },
                LocalKind::Gamma => LocalParent {
                    gamma: Some(33),
                    ..Default::default()
                },
            }
        }
        // Both surfaces reach the same body — dispatched here through
        // BOTH entry points so a divergence between them fails one
        // arm while the other passes.
        assert_single_slot_key_matches_label::<LocalParent, _>(make_local);
        assert_wire_key_matches_label::<LocalParent, LocalKind, _>(make_local);
    }

    /// Every one of the five production borrowed-view enums impls
    /// [`VariantKind`] byte-identically with its inherent `.kind()`
    /// (or `.target()` on `EncapsulationKindVariant`) — pin the
    /// delegation shape at ONE substrate boundary so a regression that
    /// inlines a divergent match body into the trait impl (rather than
    /// the one-line delegation) is caught here. `Lifetime`'s
    /// borrowed-view is included even though `Lifetime` isn't a
    /// [`TaggedUnion`] impl — the reverse projection applies uniformly.
    #[test]
    fn every_production_variant_kind_impl_matches_inherent_projection() {
        use crate::encapsulates::{EncapsulationKindVariant, ExistingHelmRelease};
        use crate::export::{ArtifactVariant, ChannelVariant, HttpEventChannel, ReceiptsSource};
        use crate::intent::{IntentVariant, NixIntent};
        use crate::lifetime::{LifetimeVariant, PermanentLifetime};

        let nix = NixIntent {
            flake_ref: "github:a/b".into(),
            attribute: "x".into(),
            system: None,
            attic_cache: None,
            extra_args: vec![],
            delegate_to_nix_build: false,
        };
        let iv = IntentVariant::Nix(&nix);
        assert_eq!(iv.kind(), iv.variant_kind());

        let perm = PermanentLifetime::default();
        let lv = LifetimeVariant::Permanent(&perm);
        assert_eq!(lv.kind(), lv.variant_kind());

        let hr = ExistingHelmRelease {
            namespace: "ns".into(),
            name: "n".into(),
            release_name: "r".into(),
        };
        let ev = EncapsulationKindVariant::ExistingHelmRelease(&hr);
        assert_eq!(ev.target(), ev.variant_kind());

        let rs = ReceiptsSource {};
        let av = ArtifactVariant::Receipts(&rs);
        assert_eq!(av.kind(), av.variant_kind());

        let ch = HttpEventChannel::signal("s");
        let cv = ChannelVariant::HttpEvent(&ch);
        assert_eq!(cv.kind(), cv.variant_kind());
    }
}
