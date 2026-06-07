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
}
