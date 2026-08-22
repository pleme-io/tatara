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
/// The [`crate::lifetime::Lifetime`] site is DELIBERATELY excluded —
/// `Lifetime` doesn't impl [`TaggedUnion`] (its `variant()` returns
/// `Ok(Permanent)` on empty rather than an `Empty` typed error), so
/// the `<T: TaggedUnion>` bound doesn't reach it. Same reasoning as
/// [`resolve_or_err`]'s and [`assert_variant_round_trip`]'s and
/// [`assert_two_slots_ambiguous`]'s exclusions.
#[track_caller]
pub fn assert_single_slot_key_matches_label<T, F>(single_slot: F)
where
    T: TaggedUnion + serde::Serialize,
    T::Kind: PartialEq + std::fmt::Debug,
    F: Fn(T::Kind) -> T,
{
    for k in <T::Kind as tatara_closed_set::ClosedSet>::ALL
        .iter()
        .copied()
    {
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
        let expected = <T::Kind as tatara_closed_set::ClosedSet>::label(k);
        assert_eq!(
            keys[0].as_str(),
            expected,
            "wire-key drift for {k:?}: single_slot's populated field '{}' must equal <T::Kind as ClosedSet>::label ({expected:?})",
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
                aplicacao: Some(AplicacaoIntent {
                    chart_ref: "x".into(),
                    version: "1".into(),
                    profile: String::new(),
                    values_overlay: serde_json::Value::Null,
                    release_name: None,
                    target_namespace: None,
                    install_timeout: None,
                }),
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
                http_event: Some(HttpEventChannel {
                    endpoint: None,
                    signal_type: "x".into(),
                }),
                ..VectorChannel::default()
            },
            ChannelKind::NatsSubject => VectorChannel {
                nats_subject: Some(NatsSubjectChannel {
                    subject: "s".into(),
                    stream: "S".into(),
                    url: None,
                }),
                ..VectorChannel::default()
            },
            ChannelKind::Stdout => VectorChannel {
                stdout: Some(StdoutChannel::default()),
                ..VectorChannel::default()
            },
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

        let ch = HttpEventChannel {
            endpoint: None,
            signal_type: "s".into(),
        };
        let cv = ChannelVariant::HttpEvent(&ch);
        assert_eq!(cv.kind(), cv.variant_kind());
    }
}
