//! [`ClosedSet`] — the typed witness for the closed-set-enum idiom.
//!
//! The substrate carries 36+ closed-set enums (`AtomKind`,
//! `QuoteForm`, `SexpShape`, `MacroDefHead`, `UnquoteForm`,
//! `KwargPathKind`, `ExpectedKwargShape`, `CompilerSpecIoStage` in
//! this crate; `ProcessPhase`, `ConditionKind`, `IntentKind`,
//! `LifetimeKind`, `TeardownPolicy`, `ProcessSignal`,
//! `ArtifactKind`, `ReportFormat`, `ChannelKind`, `ExportTrigger`,
//! `ReceiptKind`, `RequestorKind`, `SelectStrategyKind`,
//! `EncapsulationMode`, `EncapsulationTarget`, `DataClassification`,
//! `AllocationPhase`, `CalmClassification`, `OptimizationDirection`,
//! `HorizonKind`, `SubstrateType`, `ConvergencePointType`,
//! `MemberState`, `PoolPhase`,
//! `ReplacementPolicy`, `ReturnPolicy`, `AutoTerminateKind`,
//! `TerminateReasonKind`,
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
/// ## `#[derive(ClosedSet)]` proc-macro
///
/// The trait surface is hand-impl-friendly (four methods, no
/// associated types beyond `Unknown`); for implementors that follow
/// the substrate-wide naming convention the
/// [`#[derive(ClosedSet)]`](tatara_lisp_derive::ClosedSet) proc-macro
/// (re-exported as [`crate::DeriveClosedSet`]) collapses the 4-line
/// `impl ClosedSet` + 4-line `impl FromStr` boilerplate onto ONE
/// derive line + a `#[closed_set(via = "<projection>")]` attribute
/// that names the inherent projection method:
///
/// ```ignore
/// #[derive(Clone, Copy, ..., tatara_lisp::DeriveClosedSet)]
/// #[closed_set(via = "as_str")]
/// pub enum ChannelKind { HttpEvent, NatsSubject, Stdout }
///
/// impl ChannelKind {
///     pub const ALL: [Self; 3] = [Self::HttpEvent, Self::NatsSubject, Self::Stdout];
///     pub const fn as_str(self) -> &'static str { ... }
/// }
///
/// #[derive(Debug, thiserror::Error)]
/// #[error("unknown channel kind: {0}")]
/// pub struct UnknownChannelKind(pub String);
/// ```
///
/// The derive expects: `pub const ALL: [Self; N]`, an inherent
/// projection method whose name matches the `via` attribute
/// (defaults to `"label"`), and a struct named
/// `Unknown{EnumName}(pub String)` in the same module (overridable
/// via `#[closed_set(unknown = "...")]`). Bespoke `FromStr` shapes
/// (e.g. [`crate::error::CompilerSpecIoStage`]'s compound
/// `"{operation}: {label}"` key) can suppress the generated
/// `FromStr` via `#[closed_set(no_from_str)]`.
///
/// Implementors that want the carrier ITSELF generated drop the
/// hand-rolled `pub struct UnknownX(pub String)` block and add
/// `#[closed_set(generate_unknown)]` — the derive then emits the
/// carrier with `Debug + Clone + PartialEq + Eq + thiserror::Error`
/// derives and the substrate-wide
/// `#[error("unknown <spaced-lowercase enum name>: {0}")]`
/// annotation (`ChannelKind` → "unknown channel kind: {0}",
/// `ReplacementPolicy` → "unknown replacement policy: {0}"). For
/// irregular labels (`MacroDefHead` → "macro definition head",
/// `MustReachPhase` → "must-reach phase") pin the operator-facing
/// wording with `#[closed_set(generate_unknown = "...")]`.
///
/// Implementors whose Display impl matches the substrate-wide
/// 5-line `f.write_str(self.<via>())` shape (28+ enums on the
/// PascalCase wire-format axis re-derive this byte-for-byte) drop
/// the hand-rolled `impl fmt::Display for X` block and add
/// `#[closed_set(display)]` — the derive then emits
/// `impl fmt::Display for X { f.write_str(Self::<via>(*self)) }`
/// alongside the trait impl, so the `<via> ⇄ Display ⇄ FromStr`
/// triad emits through ONE generative shape per enum. Implementors
/// with a bespoke Display body (e.g. structured-reason formatters
/// that compose more than the canonical wire label) keep their
/// hand-rolled block and leave the flag off.
pub trait ClosedSet: Sized + Copy + 'static {
    /// The closed set — every variant the enum carries, in
    /// declaration order. Implementors typically delegate to an
    /// inherent `pub const ALL: [Self; N]` whose forced-arity array
    /// literal pins the cardinality at the declaration site.
    const ALL: &'static [Self];

    /// The substrate-wide spaced-lowercase NAME of the closed set —
    /// the noun phrase the parse-rejection diagnostic threads into
    /// `"unknown {SET_LABEL}: {input}"` and the typed companion the
    /// trait exposes to generic consumers (metrics taggers, REPL /
    /// LSP completion bars, exhaustive-listing renderers) that want
    /// to name the set without re-deriving the projection at every
    /// call site.
    ///
    /// The substrate-wide convention pins this projection at TWO
    /// sites pre-lift: (1) the auto-derived `pub struct
    /// Unknown{EnumName}(pub String)` carrier's `#[error("unknown
    /// <label>: {0}")]` annotation that
    /// `#[closed_set(generate_unknown[ = "<label>"])]` emits, and
    /// (2) per-implementor `_message_matches_substrate_convention`
    /// test bodies that pin the rendered diagnostic byte-for-byte.
    /// Lifting the label onto the trait means BOTH sites read from
    /// ONE generative origin — the derive computes the label once,
    /// emits it into the carrier's `#[error(...)]` annotation, AND
    /// exposes it through this const so [`assert_closed_set_well_formed`]
    /// can pin the rendered diagnostic shape generically through
    /// the trait rather than through 33+ per-implementor literal
    /// assertions.
    ///
    /// Implementors auto-derive the projection from the PascalCase
    /// enum name via the derive's `pascal_to_spaced_lowercase`
    /// helper (`ChannelKind` → `"channel kind"`, `ReplacementPolicy`
    /// → `"replacement policy"`); for irregular labels
    /// (`MacroDefHead` → `"macro definition head"`, `MustReachPhase`
    /// → `"must-reach phase"`) pin the operator-facing wording via
    /// `#[closed_set(generate_unknown = "...")]` and the derive
    /// threads the SAME label into both the carrier's `#[error(...)]`
    /// annotation AND this const. An explicit
    /// `#[closed_set(set_label = "...")]` override exists for the
    /// degenerate case where an implementor wants to bind the
    /// trait's set name independently of the carrier's diagnostic
    /// label (no production implementor reaches for this today; the
    /// axis exists for the same reason `via` does — a typed escape
    /// hatch the derive surface exposes rather than forcing the
    /// implementor to hand-roll the impl).
    const SET_LABEL: &'static str;

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

    /// Collect every variant's canonical [`Self::label`] into a
    /// freshly-allocated `Vec<&'static str>` — `Self::ALL`'s elements
    /// projected through [`Self::label`], in declaration order.
    ///
    /// The substrate-wide
    /// `T::ALL.iter().map(|v| v.label()).collect::<Vec<_>>()` shape
    /// 10+ implementor sites re-derive byte-for-byte
    /// (`tatara_process::intent::tests::intent_kind_canonical_names_pinned`,
    /// `tatara_process::receipt::tests::receipt_kind_canonical_names_pinned`,
    /// `tatara_process::matrix::tests::matrix_target_all_covers_every_variant`,
    /// `tatara_process::allocation::tests::requestor_kind_as_str_unique_per_variant`,
    /// `tatara_lisp::ast::tests::quote_form_all_covers_every_variant`,
    /// `tatara_lisp::error::tests::macro_def_head_all_covers_every_variant`,
    /// …) lifted into ONE generic projection. Generic consumers
    /// (REPL exhaustive-listing diagnostics, LSP completion bars,
    /// `tatara_lisp::domain::suggest`-keyed near-match suggesters)
    /// take `T: ClosedSet` and call `T::labels()` rather than
    /// hand-rolling the `ALL.iter().map().collect()` triple at each
    /// call site.
    ///
    /// Default body walks [`Self::ALL`] and applies [`Self::label`];
    /// implementors override only when the labels surface diverges
    /// from `Self::ALL`'s natural projection (no production implementor
    /// reaches for this today — the override axis exists for the
    /// degenerate case where an implementor's `labels()` surface
    /// names a subset of `Self::ALL` distinct from what `label()`
    /// projects).
    ///
    /// THEORY.md §V.1 — knowable platform; the labels list was a
    /// known idiom carried by convention across 10+ implementor
    /// sites. Lifting it onto the trait makes the projection a TYPED
    /// CONSEQUENCE of [`Self::ALL`] + [`Self::label`] — generic
    /// consumers see ONE method, not ONE projection-shape-per-crate.
    fn labels() -> ::std::vec::Vec<&'static str> {
        Self::ALL
            .iter()
            .copied()
            .map(<Self as ClosedSet>::label)
            .collect()
    }

    /// Project `needle` onto the closest variant whose
    /// [`Self::label`] sits within the substrate-wide bounded edit
    /// distance — the typed bridge between an unrecognized input and
    /// the "did you mean …?" diagnostic surface.
    ///
    /// Wires [`crate::domain::suggest`] (the workspace-wide
    /// Levenshtein primitive — bound 1 for ≤3 chars, 2 for ≤7 chars,
    /// 3 for ≥8 chars, lexicographic tie-break) into the
    /// [`ClosedSet`] surface so every closed-set parse rejection can
    /// thread a typed hint without re-deriving the metric / the
    /// candidate-list materialization at each consumer site. An
    /// exact match returns [`None`] — that path lives at
    /// [`Self::parse_label`]; this method exists for near-misses.
    /// Inputs beyond the bound return [`None`] so the "did you mean
    /// …?" surface stays conservative rather than guessing.
    ///
    /// Default body walks [`Self::labels`], calls
    /// [`crate::domain::suggest`], and re-keys the suggested label
    /// onto its [`Self::ALL`] variant. Implementors override only
    /// when the suggestion metric needs to diverge from the
    /// substrate-wide Levenshtein bound (no production implementor
    /// reaches for this today; the axis exists for a future
    /// implementor whose canonical labels embed punctuation /
    /// case-sensitive Unicode where the default metric would
    /// systematically miss).
    ///
    /// THEORY.md §V.1 — knowable platform; the "did you mean …?"
    /// suggestion shape ships at ONE primitive ([`crate::domain::suggest`])
    /// the substrate already routes kwarg + domain-keyword
    /// diagnostics through. Lifting the closed-set bridge onto this
    /// trait extends the SAME primitive's reach to every closed-set
    /// enum without forcing each consumer to re-derive the
    /// candidate-list shape.
    ///
    /// THEORY.md §VI.1 — generation over composition; the
    /// suggest-closest behavior emerges from the composition of
    /// THREE substrate primitives ([`Self::ALL`], [`Self::label`],
    /// [`crate::domain::suggest`]) rather than as a per-implementor
    /// edit-distance impl. Future improvements to the suggestion
    /// metric (a future Damerau-Levenshtein lift, a future
    /// case-insensitive override) edit ONE primitive and propagate to
    /// every closed-set consumer.
    ///
    /// Frontier inspiration: rustc's `find_best_match_for_name` on
    /// `Symbol`s, Idris's elaborator-reflection hint pass over its
    /// constructor namespace, Roslyn's `SymbolMatcher` over typed
    /// member tables — bounded edit-distance over a closed symbol
    /// table threaded into the parse-rejection diagnostic. Translation
    /// through pleme-io primitives: a pure default method composing
    /// the trait's [`Self::labels`] iterator with the substrate's
    /// existing [`crate::domain::suggest`] metric.
    fn suggest_closest(needle: &str) -> Option<Self> {
        let candidates = Self::labels();
        let target = crate::domain::suggest(needle, &candidates)?;
        Self::ALL.iter().copied().find(|v| v.label() == target)
    }

    /// Decode `s` into the typed variant, threading a typed
    /// [`Self::suggest_closest`] hint into the rejection envelope —
    /// the structured-diagnostic surface that composes
    /// [`Self::parse_label`] + [`Self::suggest_closest`] into ONE
    /// call a downstream LSP / `tatara-check` consumer takes as
    /// `T: ClosedSet`.
    ///
    /// On exact match returns `Ok(v)` — the hint slot stays absent
    /// because [`Self::suggest_closest`] is "near-miss only" by
    /// contract (a successful parse short-circuits before
    /// [`Self::suggest_closest`] runs, so the substrate-wide
    /// "did you mean …?" surface never double-emits the same
    /// variant once as a successful decode and once as a hint).
    /// On miss returns `Err((unknown, hint))` where `unknown` is
    /// the same typed carrier [`Self::parse_label`] would have
    /// emitted (preserving the substrate-wide
    /// `"unknown {SET_LABEL}: {input}"` rendering through
    /// [`core::fmt::Display`]) and `hint` is the typed variant
    /// [`Self::suggest_closest`] keys on — `Some(v)` when a
    /// canonical label sits within the substrate-wide bounded edit
    /// distance, `None` when no candidate qualifies (the
    /// conservative-suggestion contract — silent over guessing).
    ///
    /// The Err shape `(Self::Unknown, Option<Self>)` is deliberately
    /// asymmetric: the typed carrier is the load-bearing payload
    /// (the substrate-wide rejection surface every existing
    /// implementor's parse boundary emits), while the hint is a
    /// renderable-only adornment a downstream consumer threads next
    /// to the rejection ("did you mean `Failed`?" next to the
    /// `"unknown process phase: Failing"` shape) WITHOUT replacing
    /// the typed rejection itself. Generic consumers that don't
    /// care about the hint take `.0` (the typed carrier); consumers
    /// that DO can render `did you mean <v.label()>?` from the
    /// hint without re-deriving the metric / the candidate-list
    /// materialization at each consumer site.
    ///
    /// Default body composes [`Self::parse_label`] and
    /// [`Self::suggest_closest`] verbatim — the structured shape is
    /// a typed CONSEQUENCE of the two pre-existing primitives, not
    /// a third codepath. Implementors override only when the
    /// composition needs to diverge (no production implementor
    /// reaches for this today; the axis exists for the same reason
    /// `via` / `set_label` / `labels` overrides exist — a typed
    /// escape hatch the trait surface exposes rather than forcing
    /// the implementor to hand-roll the impl).
    ///
    /// THEORY.md §III — the typescape; the structured rejection
    /// becomes a typed projection on the trait rather than a
    /// per-consumer hand-rolled (`parse_label(s).map_err(|e| (e,
    /// Self::suggest_closest(s)))`) call at every parse boundary.
    /// THEORY.md §V.1 — knowable platform; the "did you mean …?"
    /// surface emits at ONE primitive ([`crate::domain::suggest`])
    /// the substrate already routes diagnostics through, and the
    /// composition that threads it next to the typed rejection
    /// emits at ONE trait body that every closed-set enum inherits
    /// through zero additional source.
    /// THEORY.md §VI.1 — generation over composition; the
    /// structured-diagnostic shape emerges from the composition of
    /// FOUR substrate primitives ([`Self::ALL`], [`Self::label`],
    /// [`Self::make_unknown`], [`crate::domain::suggest`]) rather
    /// than as a per-implementor structured-error impl. A future
    /// LSP / `tatara-check` consumer takes `T: ClosedSet` and
    /// renders a typed `"did you mean <variant>?"` next to a
    /// rejection without binding to a per-implementor structured
    /// carrier shape.
    ///
    /// Frontier inspiration: rustc's `MultiSpan` typed-diagnostic
    /// surface — the structured rejection carries both the typed
    /// payload AND the typed adornment slot, with the adornment
    /// rendered next to (not in place of) the typed rejection.
    /// Translation through pleme-io primitives: a pure default
    /// method composing the trait's existing
    /// [`Self::parse_label`] + [`Self::suggest_closest`] surfaces
    /// — no new primitive, no new dep, no new IR layer.
    fn parse_label_with_hint(s: &str) -> Result<Self, (Self::Unknown, Option<Self>)> {
        match Self::parse_label(s) {
            Ok(v) => Ok(v),
            Err(unknown) => Err((unknown, Self::suggest_closest(s))),
        }
    }
}

/// Generic well-formedness contract for a [`ClosedSet`] implementor —
/// the substrate-wide testkit helper that lifts the three structural
/// invariants every per-implementor test module re-derived byte-for-byte
/// pre-lift onto ONE call site:
///
/// 1. `T::ALL` is non-empty — a closed-set with zero variants is a
///    degenerate codomain [`ClosedSet::parse_label`] can never succeed
///    on; an empty `ALL` is a structural bug at the type-system
///    boundary, not a runtime accident.
/// 2. Every variant in `T::ALL` round-trips through
///    [`ClosedSet::label`] → [`ClosedSet::parse_label`] back to itself —
///    the workspace-wide `*_roundtrip_via_as_str` invariant lifted from
///    the per-implementor test surface (`ProcessPhase`,
///    `VerificationPhase`, `MustReachPhase`, `IntentKind`, `LifetimeKind`,
///    …) onto the trait.
/// 3. The labels of `T::ALL` are pairwise distinct — distinctness feeds
///    [`ClosedSet::parse_label`]'s linear sweep: a duplicate would
///    silently fold two variants into one at the round-trip boundary
///    (the sweep returns the FIRST matching variant). The workspace-wide
///    `*_all_is_unique_and_complete` invariant lifted from the
///    per-implementor `HashSet`-sweep test onto the trait.
/// 4. The empty string `""` is OUTSIDE the closed set —
///    `parse_label("")` returns [`Err`]. This is implied by (2) + (3)
///    when no implementor's `label()` projects to `""`, but checking it
///    directly catches a regression where an implementor accidentally
///    introduced an empty label as a variant projection.
/// 5. [`ClosedSet::SET_LABEL`] is non-empty AND the typed parse-rejection
///    carrier's [`core::fmt::Display`] rendering threads it into the
///    substrate-wide `"unknown {SET_LABEL}: {input}"` shape verbatim —
///    the per-implementor `_message_matches_substrate_convention` test
///    that 13+ implementors pin byte-for-byte (`UnknownEncapsulationTarget`
///    → `"unknown encapsulation target: foo"`, `UnknownArtifactKind` →
///    `"unknown artifact kind: foo"`, …) lifted onto the trait so the
///    diagnostic-shape contract emits from ONE generative origin (the
///    derive's `emit_unknown_struct` helper) AND verifies through ONE
///    typed contract (this assertion). A regression that drifts the
///    rendering between two implementors (a future derive emitter that
///    changes the prefix, a hand-rolled carrier whose `#[error(...)]`
///    annotation omits the noun phrase) fails this assertion on the
///    affected implementor without needing 33+ per-implementor literal
///    tests to catch the drift independently.
/// 6. [`ClosedSet::labels`] equals the natural
///    `Self::ALL.iter().copied().map(label).collect()` projection — the
///    labels-list surface generic consumers (REPL exhaustive listers,
///    LSP completion bars, [`ClosedSet::suggest_closest`]'s
///    candidate-list) walk over. The default trait body satisfies the
///    clause for free; the assertion catches a future implementor
///    whose `labels()` override diverges from `ALL`'s natural
///    projection (a degenerate axis the trait surface exposes for the
///    same reason `via` / `set_label` exist — a typed escape hatch
///    rather than forcing the implementor to hand-roll the impl). A
///    drifted override fails this clause loudly rather than silently
///    bifurcating the candidate-list surface every
///    `suggest_closest` consumer routes through.
/// 7. [`ClosedSet::parse_label_with_hint`] composes [`ClosedSet::parse_label`]
///    and [`ClosedSet::suggest_closest`] verbatim — every variant in
///    `T::ALL` decodes to `Ok(v)` through the structured surface
///    (the hint slot is structurally absent on the Ok arm), and the
///    sweep's reserved probe input rejects with the SAME typed
///    carrier [`ClosedSet::parse_label`] would have emitted (same
///    [`core::fmt::Display`] rendering — the substrate-wide
///    `"unknown {SET_LABEL}: {input}"` shape) AND with a `None`
///    hint slot (the probe sits beyond [`ClosedSet::suggest_closest`]'s
///    bounded edit distance by construction — its 38-char body shares
///    no characters with any plausible canonical label). The default
///    trait body satisfies the clause for free; the assertion catches
///    a future implementor whose `parse_label_with_hint` override
///    drifts from the natural composition (a degenerate axis the
///    trait surface exposes for the same reason `via` / `set_label` /
///    `labels` overrides exist — a typed escape hatch rather than
///    forcing the implementor to hand-roll the impl). A drifted
///    override that emits the wrong carrier OR fabricates a hint for
///    the unrecognizable probe fails this clause loudly rather than
///    silently bifurcating the structured-diagnostic surface every
///    `parse_label_with_hint` consumer routes through.
///
/// Per-implementor domain-specific tests STAY in the implementor's
/// test module — the `gates_phase` truth tables, the
/// `can_transition_to` state-machine contracts, the serde wire-format
/// coherence sweeps, the signal-shaped `short_str` dual-projection
/// matches — those project per-variant content the trait's structural
/// contract can't see. This helper lifts ONLY the structural four (+1)
/// every implementor copies.
///
/// Marked `#[track_caller]` so a failure points at the per-implementor
/// test's call site rather than at this helper, giving the operator a
/// stable signal about which closed-set implementor regressed.
///
/// Usage from any per-implementor test module in any crate that
/// depends on `tatara-lisp` (this crate, `tatara-process`,
/// `tatara-domains`, future closed-set implementors):
///
/// ```text
/// #[test]
/// fn process_phase_is_well_formed_closed_set() {
///     tatara_lisp::closed_set::assert_closed_set_well_formed::<ProcessPhase>();
/// }
/// ```
///
/// THEORY.md §V.1 — knowable platform; the three structural test
/// invariants were known patterns carried by convention across 36+
/// per-implementor test modules. This helper makes them a TYPED
/// CONSEQUENCE of the [`ClosedSet`] contract — any future implementor
/// that calls this helper inherits the contract without re-deriving
/// the three assertions.
#[track_caller]
pub fn assert_closed_set_well_formed<T>()
where
    T: ClosedSet + PartialEq + core::fmt::Debug,
    T::Unknown: core::fmt::Display,
{
    let type_name = core::any::type_name::<T>();
    assert!(
        !T::ALL.is_empty(),
        "{type_name}: T::ALL is empty — a closed-set with zero variants is degenerate",
    );
    for &v in T::ALL {
        let label = v.label();
        match T::parse_label(label) {
            Ok(decoded) => assert_eq!(
                decoded, v,
                "{type_name}: round-trip {label:?} → variant decoded to a different variant",
            ),
            Err(_) => {
                panic!("{type_name}: round-trip {label:?} → variant rejected by parse_label",)
            }
        }
    }
    let mut labels: Vec<&'static str> = T::ALL
        .iter()
        .copied()
        .map(<T as ClosedSet>::label)
        .collect();
    let total = labels.len();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(
        labels.len(),
        total,
        "{type_name}: duplicate labels in T::ALL — the parse_label sweep would fold two variants into one",
    );
    assert!(
        T::parse_label("").is_err(),
        "{type_name}: empty string is a valid label — a closed-set whose codomain includes \"\" is degenerate",
    );
    // (5) — SET_LABEL non-empty + carrier renders the substrate-wide
    // `"unknown {SET_LABEL}: {input}"` shape verbatim. The probe
    // input `"__assert_closed_set_well_formed_probe__"` is chosen
    // to be lexically distinct from every conceivable canonical
    // variant label across the substrate (PascalCase wire form,
    // kebab-case keyword form, punctuation marker form) so the
    // sweep `T::parse_label` walks rejects unambiguously and lands
    // in the `make_unknown` carrier — the rendering this assertion
    // pins is the carrier's `Display`, not a `parse_label` Ok-arm.
    assert!(
        !T::SET_LABEL.is_empty(),
        "{type_name}: T::SET_LABEL is empty — the substrate-wide diagnostic shape needs a noun phrase to render `unknown <set>: <input>`",
    );
    let probe = "__assert_closed_set_well_formed_probe__";
    let rendered = T::make_unknown(probe).to_string();
    let expected = {
        let mut out = String::with_capacity("unknown : ".len() + T::SET_LABEL.len() + probe.len());
        out.push_str("unknown ");
        out.push_str(T::SET_LABEL);
        out.push_str(": ");
        out.push_str(probe);
        out
    };
    assert_eq!(
        rendered, expected,
        "{type_name}: parse-rejection carrier's Display drifted from the substrate-wide `unknown {{SET_LABEL}}: {{input}}` shape — the derive's `#[error(...)]` annotation and the trait's SET_LABEL const must thread the SAME noun phrase",
    );
    // (6) — `T::labels()` matches `T::ALL.iter().copied().map(label).collect()`.
    // The default trait body satisfies the clause for free; the
    // assertion catches a future implementor whose override drifts
    // from the natural `ALL`-projection surface every
    // `suggest_closest` consumer walks over. Length AND
    // index-by-index match — neither alone catches "different labels,
    // same length" drift nor "right labels, wrong order" drift.
    let labels = T::labels();
    let natural: Vec<&'static str> = T::ALL
        .iter()
        .copied()
        .map(<T as ClosedSet>::label)
        .collect();
    assert_eq!(
        labels, natural,
        "{type_name}: T::labels() drifted from T::ALL.iter().copied().map(label).collect() — the labels-list surface every `suggest_closest` consumer walks over no longer matches the natural ALL-projection",
    );
    // (7) — `T::parse_label_with_hint` composes `parse_label` +
    // `suggest_closest` verbatim. Every variant decodes to `Ok(v)`
    // through the structured surface; the probe rejects with the
    // SAME carrier shape `parse_label` emits AND with a `None` hint
    // slot (the 38-char probe sits beyond `suggest_closest`'s
    // bounded edit distance by construction — no plausible canonical
    // label shares enough characters with the reserved probe to fall
    // inside the bound-3 window). The default trait body satisfies
    // the clause for free; the assertion catches an override that
    // drifts the composition.
    for &v in T::ALL {
        let label = v.label();
        match T::parse_label_with_hint(label) {
            Ok(decoded) => assert_eq!(
                decoded, v,
                "{type_name}: parse_label_with_hint round-trip {label:?} → variant decoded to a different variant",
            ),
            Err(_) => panic!(
                "{type_name}: parse_label_with_hint round-trip {label:?} → variant rejected by parse_label_with_hint",
            ),
        }
    }
    match T::parse_label_with_hint(probe) {
        Ok(_) => panic!(
            "{type_name}: parse_label_with_hint accepted the reserved probe input — the structured surface MUST reject every input outside the closed set",
        ),
        Err((carrier, hint)) => {
            assert_eq!(
                carrier.to_string(),
                expected,
                "{type_name}: parse_label_with_hint's Err carrier drifted from the substrate-wide `unknown {{SET_LABEL}}: {{input}}` shape — the override emits a different carrier than `parse_label` would",
            );
            assert!(
                hint.is_none(),
                "{type_name}: parse_label_with_hint fabricated a `did you mean ...?` hint for the unrecognizable probe — the conservative-suggestion contract demands `None` for inputs beyond the bounded edit distance",
            );
        }
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

    impl core::fmt::Display for UnknownStubKind {
        // Mirrors the substrate-wide `#[derive(thiserror::Error)] +
        // #[error("unknown <SET_LABEL>: {0}")]` shape the
        // `#[closed_set(generate_unknown)]` proc-macro emits — the
        // stub stays independent of `thiserror` (and the derive) so
        // the trait's contract holds for hand-impl'd carriers too,
        // not just the auto-derived majority.
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "unknown stub kind: {}", self.0)
        }
    }

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
        const SET_LABEL: &'static str = "stub kind";
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
    fn assert_closed_set_well_formed_passes_for_stub() {
        // The testkit helper's happy path — `StubKind`'s three
        // variants are pairwise distinct, round-trip through the
        // trait's `parse_label` ↔ `label`, and reject `""`. This is
        // the call any per-implementor test module makes in lieu of
        // re-deriving the three structural assertions
        // (`*_all_is_unique_and_complete`, `*_roundtrip_via_as_str`,
        // empty-rejection) byte-for-byte. A regression in any of the
        // three invariants fails this assertion in isolation from
        // any real substrate enum so the testkit's contract stays
        // independent of per-implementor truth tables.
        super::assert_closed_set_well_formed::<StubKind>();
    }

    #[test]
    fn set_label_threads_into_substrate_wide_diagnostic_shape() {
        // The closed-set name as a typed surface — `T::SET_LABEL` IS
        // the noun phrase the substrate-wide `"unknown {SET_LABEL}:
        // {input}"` diagnostic threads into. Pinning the
        // `<stub_kind, "unknown stub kind: <input>">` correspondence
        // here means every per-implementor `_message_matches_substrate_
        // convention` test (13+ across the workspace) is a structural
        // CONSEQUENCE of two simpler invariants: (1) the trait's
        // `SET_LABEL` const equals the spaced-lowercase enum name
        // (auto-derived) or its operator-pinned override, and (2)
        // the carrier's Display renders the substrate-wide prefix
        // verbatim around the offending input. Drift between the
        // two surfaces (a future derive change that emits a different
        // prefix, a hand-rolled carrier that omits the noun phrase)
        // fails the well-formedness sweep on every implementor that
        // calls `assert_closed_set_well_formed`, not just the
        // implementors whose tests independently pin the literal.
        assert_eq!(<StubKind as ClosedSet>::SET_LABEL, "stub kind");
        let rendered = <StubKind as ClosedSet>::make_unknown("delta").to_string();
        assert_eq!(rendered, "unknown stub kind: delta");
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_set_label_and_carrier_display() {
        // The well-formedness sweep's (5) clause — the carrier's
        // Display MUST thread `T::SET_LABEL` verbatim through the
        // substrate-wide `"unknown {SET_LABEL}: {input}"` shape. A
        // hand-impl'd implementor whose Display drops the noun phrase
        // (or substitutes a different one) fails the sweep loudly
        // rather than silently bifurcating the substrate-wide
        // diagnostic surface. Pinning the failure path here keeps the
        // testkit's (5) clause guaranteed-to-fire — a regression that
        // makes the assertion permissive (e.g. a future "either of
        // two acceptable shapes" relaxation) breaks this stub-level
        // contract before any per-implementor sweep runs.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedKind(pub String);
        impl core::fmt::Display for UnknownDriftedKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                // Deliberately wrong shape — uses "invalid" instead
                // of the substrate-wide "unknown" prefix.
                write!(f, "invalid drifted kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted kind";
            type Unknown = UnknownDriftedKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedKind(s.to_owned())
            }
        }
        let outcome = std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a carrier whose Display drifted from the substrate-wide shape",
        );
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

    #[test]
    fn labels_collects_each_variants_canonical_label_in_declaration_order() {
        // The labels-list surface — `T::labels()` is the substrate-wide
        // candidate list every generic consumer walks (the REPL
        // exhaustive lister, the LSP completion bar, the
        // `suggest_closest` did-you-mean keyer). Pinning the projection
        // here means a regression that re-orders / subsets the
        // candidate list (a future implementor whose override drops
        // a variant from the labels surface, an unsynchronized
        // refactor that swaps two `ALL` entries without updating the
        // labels projection) fails this contract before any
        // per-implementor test surfaces the drift downstream.
        assert_eq!(
            <StubKind as ClosedSet>::labels(),
            vec!["alpha", "beta", "gamma"],
        );
    }

    #[test]
    fn suggest_closest_recovers_variant_from_single_edit_typo() {
        // The did-you-mean bridge — a single-edit perturbation of a
        // canonical label (`alpha` → `alpa`, one deletion) decodes to
        // the original variant through the typed `suggest_closest`
        // surface. The bound scales with input length (1 for ≤3 chars,
        // 2 for ≤7 chars, 3 for ≥8 chars) so a 5-char "alpa" sits
        // inside the bound-2 window for "alpha". Pinning the recovery
        // path here keeps the substrate-wide Levenshtein primitive's
        // bridge into ClosedSet honest — a regression that changes the
        // bound (a future "stricter near-miss" gate) breaks this
        // stub-level contract before any per-implementor surface
        // depends on the suggestion semantics.
        assert_eq!(
            <StubKind as ClosedSet>::suggest_closest("alpa"),
            Some(StubKind::Alpha),
        );
    }

    #[test]
    fn suggest_closest_returns_none_for_exact_match() {
        // The exact-match contract — `suggest_closest("alpha")`
        // returns `None`, NOT `Some(Alpha)`, because the did-you-mean
        // primitive exists for near-misses only; the exact-match
        // path lives at [`ClosedSet::parse_label`]. Pinning the
        // contract here means a generic diagnostic consumer that
        // chains "parse_label OR suggest_closest" doesn't double-emit
        // the same variant (once as a successful decode, once as a
        // suggested hint). Mirrors `tatara_lisp::domain::suggest`'s
        // `candidate == needle { continue; }` arm — the substrate's
        // suggestion surface is uniformly "near-miss only" rather
        // than "best match including self".
        assert_eq!(<StubKind as ClosedSet>::suggest_closest("alpha"), None,);
    }

    #[test]
    fn suggest_closest_returns_none_for_input_outside_suggestion_bound() {
        // The conservative-suggestion contract — an input whose
        // closest label sits beyond the substrate-wide bounded edit
        // distance returns `None` rather than "best of the bunch".
        // The bound for an 8-char input ("xxxxxxxx") is 3; every
        // candidate label ("alpha"/5, "beta"/4, "gamma"/5) sits at
        // edit distance ≥4 (Levenshtein doesn't reduce below the
        // length difference for disjoint character sets). Pinning the
        // "conservative" semantics here keeps the substrate's
        // "did you mean …?" surface from guessing for the operator —
        // when the input is unrecognizable, the diagnostic stays
        // silent rather than emitting an unrelated suggestion.
        assert_eq!(<StubKind as ClosedSet>::suggest_closest("xxxxxxxx"), None,);
    }

    #[test]
    fn parse_label_with_hint_returns_ok_variant_for_exact_match() {
        // The exact-match arm — `parse_label_with_hint("alpha")`
        // returns `Ok(Alpha)`, NOT `Err((_, Some(Alpha)))`. The Ok
        // arm carries the variant alone (no hint adornment) because
        // the substrate-wide "did you mean …?" surface is
        // near-miss-only by contract: a successful decode is the
        // CANONICAL match, not a near-miss that happens to coincide
        // with a canonical label. Pinning the contract here means a
        // generic structured-diagnostic consumer that takes
        // `parse_label_with_hint(s).err()` and walks the hint slot
        // can rely on the slot being absent for successful decodes
        // — the structured surface doesn't double-emit the variant.
        assert_eq!(
            <StubKind as ClosedSet>::parse_label_with_hint("alpha"),
            Ok(StubKind::Alpha),
        );
    }

    #[test]
    fn parse_label_with_hint_returns_unknown_with_hint_for_near_miss() {
        // The near-miss arm — a single-edit perturbation of a
        // canonical label decodes to `Err((unknown, Some(variant)))`
        // through the structured surface. The carrier preserves the
        // input verbatim (`UnknownStubKind("alpa")`) so the typed
        // rejection rendering stays load-bearing; the hint adornment
        // (`Some(Alpha)`) is the typed variant a downstream LSP /
        // `tatara-check` consumer renders next to the rejection.
        // Pinning the structured shape here means a generic consumer
        // can take `parse_label_with_hint(s).err()` and pattern-
        // match `(carrier, Some(hint))` to render `"unknown stub
        // kind: alpa\n  did you mean alpha?"` without re-deriving
        // the suggestion metric / the candidate-list materialization
        // at every call site.
        let outcome = <StubKind as ClosedSet>::parse_label_with_hint("alpa");
        assert_eq!(
            outcome,
            Err((UnknownStubKind("alpa".to_owned()), Some(StubKind::Alpha))),
        );
    }

    #[test]
    fn parse_label_with_hint_returns_unknown_without_hint_for_far_miss() {
        // The conservative-suggestion arm — an input whose closest
        // label sits beyond the substrate-wide bounded edit distance
        // returns `Err((unknown, None))` rather than `Err((unknown,
        // Some(best_of_the_bunch)))`. The carrier still preserves
        // the input verbatim (the typed rejection surface stays
        // load-bearing); the hint slot stays absent (`None`) so the
        // "did you mean …?" surface doesn't fabricate an
        // unrelated suggestion. Pinning the contract here means a
        // generic structured-diagnostic consumer that takes the hint
        // slot can rely on its presence as a signal — the operator
        // sees `did you mean …?` only when the substrate has a
        // typed near-miss to point at.
        let outcome = <StubKind as ClosedSet>::parse_label_with_hint("xxxxxxxx");
        assert_eq!(outcome, Err((UnknownStubKind("xxxxxxxx".to_owned()), None)),);
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_parse_label_with_hint_and_composition() {
        // The well-formedness sweep's (7) clause —
        // `parse_label_with_hint` MUST compose `parse_label` +
        // `suggest_closest` verbatim. A hand-impl'd implementor
        // whose override drifts the composition (returns the wrong
        // carrier on the Err arm, fabricates a hint for the
        // unrecognizable probe, accepts the probe as `Ok`) fails the
        // sweep loudly rather than silently bifurcating the
        // structured-diagnostic surface every consumer routes
        // through. Pinning the failure path here keeps the
        // testkit's (7) clause guaranteed-to-fire — a regression
        // that makes the assertion permissive (e.g. a future "best
        // of the bunch" relaxation that emits a hint for any input)
        // breaks this stub-level contract before any per-implementor
        // sweep runs.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedHintKind {
            Only,
        }
        #[derive(Debug, PartialEq, Eq)]
        struct UnknownDriftedHintKind(pub String);
        impl core::fmt::Display for UnknownDriftedHintKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted hint kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedHintKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted hint kind";
            type Unknown = UnknownDriftedHintKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedHintKind(s.to_owned())
            }
            fn parse_label_with_hint(_s: &str) -> Result<Self, (Self::Unknown, Option<Self>)> {
                // Drifted override — fabricates a hint for every
                // input, including the unrecognizable probe the
                // testkit's clause (7) reserves. Fails the
                // conservative-suggestion contract.
                Err((
                    UnknownDriftedHintKind(String::from("any")),
                    Some(Self::Only),
                ))
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedHintKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a parse_label_with_hint override that fabricated a hint for the unrecognizable probe",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_labels_and_all_projection() {
        // The well-formedness sweep's (6) clause — `T::labels()`
        // MUST equal the natural
        // `T::ALL.iter().copied().map(label).collect()` projection.
        // A hand-impl'd implementor whose override returns a
        // different shape (a subset, a re-ordering, an externally
        // sourced label list) fails the sweep loudly rather than
        // silently bifurcating the candidate-list surface every
        // `suggest_closest` consumer walks over. Pinning the failure
        // path here keeps the testkit's (6) clause guaranteed-to-fire
        // — a regression that makes the assertion permissive
        // (e.g. a future "any superset" relaxation) breaks this
        // stub-level contract before any per-implementor sweep runs.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedLabelsKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedLabelsKind(pub String);
        impl core::fmt::Display for UnknownDriftedLabelsKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted labels kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedLabelsKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted labels kind";
            type Unknown = UnknownDriftedLabelsKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedLabelsKind(s.to_owned())
            }
            fn labels() -> Vec<&'static str> {
                // Drifted override — returns a label that doesn't
                // appear in `ALL.iter().map(label)`.
                vec!["wrong"]
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedLabelsKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a labels() override drifted from the natural ALL-projection",
        );
    }
}
