//! [`ClosedSet`] — the typed witness for the closed-set-enum idiom.
//!
//! The substrate carries 36+ closed-set enums (`AtomKind`,
//! `QuoteForm`, `SexpShape`, `MacroDefHead`, `UnquoteForm`,
//! `KwargPathKind`, `ExpectedKwargShape`, `CompilerSpecIoStage` in
//! this crate; `ProcessPhase`, `ConditionKind`, `IntentKind`,
//! `LifetimeKind`, `TeardownPolicy`, `ProcessSignal`, `ChannelKind`,
//! `ArtifactKind`, `ReceiptKind`, `RequestorKind`,
//! `AllocationPhase`, `CalmClassification`, `OptimizationDirection`,
//! `HorizonKind`, `SubstrateType`, `ConvergencePointType`,
//! `DataClassification`, `MemberState`, `PoolPhase`,
//! `ReplacementPolicy`, `ReturnPolicy`, `AutoTerminateKind`,
//! `TerminateReasonKind`, `SelectStrategyKind`, `EncapsulationMode`,
//! `EncapsulationTarget`, `ReportFormat`, `ExportTrigger`,
//! `VerificationPhase`, `WorkloadKind`, `MustReachPhase`,
//! `SighupStrategy`, `BreatheDimensionKind`, `MatrixTarget`,
//! `ReportPayloadShape`, … in `tatara-process`). Each one independently
//! re-derives the same four-piece shape:
//!
//! 1. `pub const ALL: [Self; N] = [...]` — the forced-arity array
//!    literal that fails compilation if a new variant lands without
//!    being added to the set.
//! 2. `fn label(self) -> &'static str` (or its domain-canonical
//!    sibling — `prefix`, `marker`, `keyword`, `as_str`) — the typed
//!    projection from variant to the canonical `&'static str` literal
//!    the diagnostic / wire format uses.
//! 3. `impl FromStr` whose body is a linear sweep over `Self::ALL`
//!    keyed on the projection — exactly the same 6-line for-loop /
//!    `Err(Unknown<TypeName>(s.to_owned()))` shape every implementor
//!    re-derives byte-for-byte.
//! 4. `pub struct Unknown<TypeName>(pub String)` with
//!    `#[error("unknown <thing>: {0}")]` — the typed parse-rejection
//!    carrier that hands the offending input back unchanged.
//!
//! Pieces 1, 2, 4 carry per-variant content (the variants themselves,
//! their canonical labels, the rejection-class wording) and stay
//! per-implementor. Piece 3 — the for-loop sweep — is mechanically
//! identical across every implementor and is the duplication this
//! trait lifts.
//!
//! ## Trait surface
//!
//! ```text
//! pub trait ClosedSet: Sized + Copy {
//!     const ALL: &'static [Self];
//!     type Unknown;
//!     fn label(self) -> &'static str;
//!     fn make_unknown(s: &str) -> Self::Unknown;
//!
//!     // Default — the lifted for-loop body.
//!     fn parse_label(s: &str) -> Result<Self, Self::Unknown> { ... }
//! }
//! ```
//!
//! A typical implementor wires in three lines beyond its existing
//! inherent surface, and its hand-rolled `FromStr` body collapses
//! from six lines to one:
//!
//! ```text
//! impl ClosedSet for AtomKind {
//!     const ALL: &'static [Self] = &Self::ALL;
//!     type Unknown = UnknownAtomKind;
//!     fn label(self) -> &'static str { AtomKind::label(self) }
//!     fn make_unknown(s: &str) -> Self::Unknown { UnknownAtomKind(s.to_owned()) }
//! }
//!
//! impl FromStr for AtomKind {
//!     type Err = UnknownAtomKind;
//!     fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
//!         <Self as ClosedSet>::parse_label(s)
//!     }
//! }
//! ```
//!
//! Implementors with a non-`label` inherent projection name
//! (`QuoteForm::prefix`, `UnquoteForm::marker`, `MacroDefHead::keyword`,
//! `tatara_process`'s `*::as_str`) delegate `ClosedSet::label` to
//! their domain-canonical method — the trait method gives every
//! implementor a STABLE name (`label`), with the inherent name kept as
//! the load-bearing domain-vocabulary projection (`prefix` for
//! homoiconic reader-form, `marker` for template-substitution
//! punctuation, `keyword` for macro-head reserved word, `as_str` for
//! `tatara-process`'s PascalCase wire format).
//!
//! ## Theory grounding
//!
//! THEORY.md §V.1 — knowable platform; the for-loop / `Unknown`-
//! emission pattern was a known idiom carried by convention across 36+
//! implementors. This trait makes the idiom a TYPED WITNESS — any new
//! closed-set enum that implements `ClosedSet` plugs into the trait's
//! default `parse_label` and a future generic consumer (a metrics
//! tagger, a Lisp keyword completer, an iac-forge canonical-form
//! renderer over closed-set kinds) can take a `T: ClosedSet`
//! parameter and walk the set without knowing which crate it lives in.
//!
//! THEORY.md §VI.1 — generation over composition; the trait IS the
//! generative shape. New closed-set enums add the trait impl + the
//! one-line FromStr delegation instead of re-deriving the for-loop
//! body, and the parse-rejection diagnostic surface narrows from "36+
//! independent sweeps that must each be kept symmetric" to "one
//! default body, 36+ impls of a four-method contract."

/// The closed-set-enum idiom as a typed witness.
///
/// Implementors carry an inherent `pub const ALL: [Self; N]` with a
/// forced-arity array literal so the compiler enforces variant /
/// cardinality coherence at the declaration; this trait re-exposes
/// the same data as a `&'static [Self]` slice so [`Self::parse_label`]
/// can iterate generically over `Self` without the inherent constant
/// being visible at the call site.
///
/// The default [`Self::parse_label`] is the substrate-wide for-loop
/// pattern lifted into ONE place. Every implementor's
/// [`std::str::FromStr::from_str`] body reduces to a single delegation
/// (`<Self as ClosedSet>::parse_label(s)`), and the per-implementor
/// `Unknown<TypeName>` carrier flows through [`Self::make_unknown`].
///
/// ## Why not a derive macro
///
/// The trait surface is intentionally hand-impl-friendly (four
/// methods, no associated types beyond `Unknown`). A future
/// `#[derive(ClosedSet)]` proc-macro can land additively in
/// `tatara-lisp-derive` without changing this surface — implementors
/// just stop writing the four-line impl by hand.
pub trait ClosedSet: Sized + Copy + 'static {
    /// The closed set — every variant the enum carries, in
    /// declaration order. Implementors typically delegate to an
    /// inherent `pub const ALL: [Self; N]` whose forced-arity array
    /// literal pins the cardinality at the declaration site.
    const ALL: &'static [Self];

    /// The typed parse-rejection carrier this implementor emits when
    /// [`Self::parse_label`] is handed a non-canonical input. The
    /// substrate-wide convention is the
    /// `pub struct UnknownX(pub String)` shape with a
    /// `#[error("unknown <thing>: {0}")]` annotation, but the trait
    /// does not require either — implementors are free to use a
    /// richer carrier (a sum type, a structured diagnostic) as long
    /// as it remains the `FromStr::Err` for the implementing type.
    type Unknown;

    /// Project the typed variant to its canonical `&'static str`
    /// label — the projection [`Self::parse_label`] keys on.
    ///
    /// Implementors with a domain-canonical inherent projection name
    /// (`prefix` for [`crate::ast::QuoteForm`], `marker` for
    /// [`crate::error::UnquoteForm`], `keyword` for
    /// [`crate::error::MacroDefHead`], `as_str` across `tatara-process`'s
    /// PascalCase wire-format enums) delegate this trait method to
    /// their inherent method — the trait method gives generic
    /// consumers a STABLE name (`label`) without renaming the
    /// load-bearing domain vocabulary.
    fn label(self) -> &'static str;

    /// Wrap the offending input verbatim in the typed
    /// parse-rejection carrier — the substrate-wide convention is
    /// `Self::Unknown(s.to_owned())` for the `pub struct UnknownX(pub
    /// String)` shape. The `&str` borrow (rather than `String`) lets
    /// implementors that want to project the input through a
    /// normalization step (a future structured diagnostic carrier
    /// that records both the raw and the normalized form) do so
    /// without forcing the trait surface to materialize an owned
    /// `String` the implementor doesn't need.
    fn make_unknown(s: &str) -> Self::Unknown;

    /// Decode a canonical [`Self::label`] back into the typed variant
    /// — `Ok(v)` when `s` matches some `v.label()` exactly, and
    /// `Err(Self::make_unknown(s))` for every other string.
    ///
    /// Linear sweep over [`Self::ALL`] keyed on [`Self::label`]. The
    /// canonical literals live at ONE site (the implementor's
    /// inherent projection) rather than at TWO (the projection PLUS
    /// the per-variant `FromStr` arm pre-lift); adding a new variant
    /// extends only `Self::ALL` + `Self::label`, NOT a third
    /// per-variant literal site.
    ///
    /// Default body is the substrate-wide pattern lifted into ONE
    /// place; implementors override only when the parse surface
    /// shape diverges (e.g. [`crate::error::CompilerSpecIoStage`]'s
    /// compound `"{operation}: {label}"` key, which keys on a
    /// projection PAIR rather than a single label, and keeps its
    /// bespoke `FromStr` body).
    fn parse_label(s: &str) -> Result<Self, Self::Unknown> {
        for &kind in Self::ALL {
            if s == kind.label() {
                return Ok(kind);
            }
        }
        Err(Self::make_unknown(s))
    }
}

#[cfg(test)]
mod tests {
    use super::ClosedSet;

    /// Stub implementor — exercises the trait's four-method contract +
    /// the default `parse_label` body in isolation from any real
    /// substrate enum. Keeps the trait-level tests independent of the
    /// per-implementor truth-table tests sibling enums carry.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum StubKind {
        Alpha,
        Beta,
        Gamma,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct UnknownStubKind(pub String);

    impl StubKind {
        const ALL: [Self; 3] = [Self::Alpha, Self::Beta, Self::Gamma];

        const fn label(self) -> &'static str {
            match self {
                Self::Alpha => "alpha",
                Self::Beta => "beta",
                Self::Gamma => "gamma",
            }
        }
    }

    impl ClosedSet for StubKind {
        const ALL: &'static [Self] = &Self::ALL;
        type Unknown = UnknownStubKind;
        fn label(self) -> &'static str {
            StubKind::label(self)
        }
        fn make_unknown(s: &str) -> Self::Unknown {
            UnknownStubKind(s.to_owned())
        }
    }

    #[test]
    fn trait_all_re_exposes_inherent_all() {
        // The trait's `ALL` slice points at the same per-implementor
        // inherent `ALL` array — the trait surface mirrors the
        // forced-arity constant rather than duplicating the variant
        // listing. A drift between the two would break every
        // generic consumer that walks `<T as ClosedSet>::ALL`.
        assert_eq!(<StubKind as ClosedSet>::ALL, &StubKind::ALL);
    }

    #[test]
    fn parse_label_round_trips_every_variant() {
        // For every variant `v` in the closed set, `parse_label` of
        // `v.label()` decodes back to `v` exactly — the substrate-wide
        // round-trip invariant the per-implementor sibling tests pin
        // (`atom_kind_label_round_trips_through_from_str`,
        // `sexp_shape_label_round_trips_through_from_str`,
        // `kwarg_path_kind_label_round_trips_through_from_str`, …)
        // lifted onto the trait so any future implementor inherits
        // the contract without re-deriving the assertion.
        for &v in <StubKind as ClosedSet>::ALL {
            let decoded = <StubKind as ClosedSet>::parse_label(v.label());
            assert_eq!(decoded, Ok(v));
        }
    }

    #[test]
    fn parse_label_rejects_unknown_input_via_make_unknown() {
        // A label not in the closed set rejects with the typed
        // `Unknown` carrier wrapping the input verbatim. The verbatim
        // contract is load-bearing: substring-matching on the
        // rendered diagnostic ("unknown stub kind: <input>") in LSP /
        // REPL capture binds to the typed carrier's payload, so any
        // future normalization step at the rejection boundary would
        // silently bifurcate the diagnostic surface.
        let rejection = <StubKind as ClosedSet>::parse_label("delta");
        assert_eq!(rejection, Err(UnknownStubKind("delta".to_owned())));
    }

    #[test]
    fn parse_label_rejects_empty_input() {
        // Empty input is structurally outside the closed set — no
        // variant projects to "" through `label`, so the decode
        // rejects cleanly with the empty string carried through. The
        // empty-input rejection is the boundary case operators hit
        // when a config field is unset but reached anyway; pinning it
        // here means no implementor can drift its empty-input
        // behavior accidentally.
        let rejection = <StubKind as ClosedSet>::parse_label("");
        assert_eq!(rejection, Err(UnknownStubKind(String::new())));
    }

    #[test]
    fn parse_label_is_case_sensitive() {
        // The closed-set labels are the projection the diagnostic
        // surface renders byte-for-byte; case drift between the
        // operator's input and the canonical literal is a rejection,
        // not a normalization. Pinning the case-sensitive contract
        // means a future implementor that wants case-insensitive
        // decoding must override `parse_label` (and document the
        // divergence), not silently subsume the substrate-wide
        // convention.
        let rejection = <StubKind as ClosedSet>::parse_label("Alpha");
        assert_eq!(rejection, Err(UnknownStubKind("Alpha".to_owned())));
    }

    #[test]
    fn closed_set_all_labels_are_distinct() {
        // The closed-set contract demands the per-variant labels
        // partition the projection's codomain injectively; a
        // duplicate label would silently make `parse_label`'s decode
        // ambiguous (the linear sweep returns the FIRST matching
        // variant), which would in turn silently fold two distinct
        // variants into one at the round-trip boundary. Pinning
        // distinctness here means a generic check over any
        // `T: ClosedSet` can catch the regression even before the
        // per-implementor tests run.
        let labels: Vec<&'static str> = <StubKind as ClosedSet>::ALL
            .iter()
            .copied()
            .map(<StubKind as ClosedSet>::label)
            .collect();
        let mut sorted = labels.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len());
    }
}
