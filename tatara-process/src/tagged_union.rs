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
}
