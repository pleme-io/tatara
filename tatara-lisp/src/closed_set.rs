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

    /// The closed set's cardinality — the count of variants in
    /// [`Self::ALL`], surfaced as a compile-time-known [`usize`] on
    /// the trait so downstream generic code can bind to it in const
    /// contexts ([`[T; N]`](array) dimensions,
    /// [`Vec::with_capacity`](::std::vec::Vec::with_capacity) sizing,
    /// bitset widths, per-variant lookup-table shapes) without
    /// re-deriving `Self::ALL.len()` at every call site.
    ///
    /// Default body is `Self::ALL.len()` — the count is a typed
    /// CONSEQUENCE of the [`Self::ALL`] slice, not a second per-
    /// implementor site the operator must keep in sync with the
    /// variant listing. The `<[T]>::len` primitive is const-stable
    /// (Rust 1.39+) so the projection evaluates at compile time on
    /// every implementor whose inherent `pub const ALL: [Self; N]`
    /// is itself a const array literal — the substrate-wide default
    /// shape. Implementors override only when the cardinality surface
    /// needs to diverge from [`Self::ALL`]'s natural length (no
    /// production implementor reaches for this today; the axis exists
    /// for the same reason `via` / `set_label` / `labels` overrides
    /// exist — a typed escape hatch the trait surface exposes rather
    /// than forcing the implementor to hand-roll the impl).
    ///
    /// Sibling posture to [`Self::ALL`] on the (variant listing,
    /// variant count) axis: [`Self::ALL`] is the [`&'static [Self]`](slice)
    /// slice generic consumers iterate over, this const is the
    /// [`usize`] count the same consumers reach for when they need a
    /// compile-time-known dimension. The pair partitions the
    /// (variant enumeration, cardinality) surface exhaustively — one
    /// for iteration, one for const-generic bindings — with the
    /// count derived from the listing at ONE substrate primitive
    /// rather than at TWO independent per-implementor sites.
    ///
    /// Future consumers — a compact-encoding wire format that
    /// packs each variant into `ceil(log2(T::CARDINALITY))` bits, a
    /// per-variant lookup table typed as `[Payload; T::CARDINALITY]`
    /// whose length is verified at compile time against `T::ALL`, a
    /// metrics tagger that pre-sizes a `Vec` for `T::CARDINALITY`
    /// samples, a bitset over the closed-set indexed at
    /// `T::CARDINALITY`-many positions — bind to ONE trait const
    /// instead of hand-rolling the `Self::ALL.len()` composition at
    /// each call site, and the closed-set projection's cardinality
    /// surface evolves at ONE site rather than per-consumer.
    ///
    /// THEORY.md §III — the typescape; the closed-set cardinality
    /// becomes a TYPE-level projection on the trait rather than a
    /// per-consumer inline `Self::ALL.len()` composition at every
    /// downstream generic site. The (variant enumeration, cardinality)
    /// pair partitions the closed-set surface exhaustively into TWO
    /// typed projections, each with a distinct load-bearing consumer
    /// surface — iteration for [`Self::ALL`], const-generic bindings
    /// for this const.
    /// THEORY.md §V.1 — knowable platform; the cardinality was an
    /// unnamed inline projection (`T::ALL.len()`) recurring at 20+
    /// test sites and every prospective const-generic consumer site
    /// pre-lift. Naming it on the trait makes the projection a TYPED
    /// CONSEQUENCE of [`Self::ALL`] — generic consumers see ONE const,
    /// not ONE inline-length-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the cardinality
    /// emerges from the composition of ONE substrate primitive
    /// ([`Self::ALL`]) with the standard-library const `<[T]>::len`
    /// projection rather than as a per-implementor `const N: usize =
    /// _` declaration. A future tightening of [`Self::ALL`] (a future
    /// declaration-time cardinality assertion, a future
    /// `#[closed_set(cardinality = N)]` derive attribute that pins
    /// N at the source) propagates to every closed-set const-generic
    /// consumer through ONE trait const.
    ///
    /// Frontier inspiration: Idris's `Fin n` finite-cardinality type
    /// exposes `n` at the type level so every downstream indexer /
    /// enumerator binds to a compile-time count; Rust's
    /// `std::mem::variant_count::<T>()` intrinsic (unstable, nightly-
    /// only) exposes the same shape from the language side. MLIR's
    /// `mlir::TypeAttrOfBase<TypeParam>::getMaxEnumValInternal` and
    /// LLVM's `EnumAttrParams` similarly surface the enum's cardinality
    /// as a first-class typed integer on the registry. Translation
    /// through pleme-io primitives: a pure default associated const
    /// initializer composing the trait's existing [`Self::ALL`] with
    /// the const-stable `<[T]>::len` slice projection — no new dep,
    /// no unstable intrinsic, no per-implementor override.
    const CARDINALITY: usize = Self::ALL.len();

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
    /// Default body composes [`Self::find_by_label`] with
    /// [`Option::ok_or_else`] into [`Self::make_unknown`] — the sweep
    /// itself lives at ONE substrate primitive
    /// ([`Self::find_by_label`]), and the parse arm threads the
    /// carrier materialization onto its `None` result. Implementors
    /// override only when the parse surface shape diverges (e.g.
    /// [`crate::error::CompilerSpecIoStage`]'s compound
    /// `"{operation}: {label}"` key, which keys on a projection PAIR
    /// rather than a single label, and keeps its bespoke `FromStr`
    /// body). An implementor that overrides [`Self::find_by_label`]
    /// propagates the override through this default body to the
    /// parse-decode arm automatically; the (allocating carrier
    /// decode, non-allocating typed decode, non-allocating
    /// predicate) triad of the closed-set surface funnels every
    /// sweep through ONE typed primitive.
    fn parse_label(s: &str) -> Result<Self, Self::Unknown> {
        Self::find_by_label(s).ok_or_else(|| Self::make_unknown(s))
    }

    /// Zero-allocation typed decode — `Some(v)` when `s` matches some
    /// variant's [`Self::label`] exactly, `None` for every other
    /// string.
    ///
    /// Peer of [`Self::parse_label`] on the (allocating carrier
    /// decode, non-allocating typed decode) axis of the closed-set
    /// surface: [`Self::parse_label`] materializes the typed
    /// [`Self::Unknown`] carrier (owning a [`String`] copy of `s`)
    /// on the reject path even when the caller drops it immediately
    /// with `.ok()`; this method answers the SAME structural
    /// question — "which canonical variant does `s` decode to, if
    /// any?" — with a bare [`Option::None`] on rejection, never
    /// entering [`Self::make_unknown`]. Consumers that need the
    /// typed variant BUT can supply a natural default (a config
    /// field with a fallback, an LSP hover under a candidate label,
    /// a `filter_map` over a candidate stream that projects each
    /// element onto its typed variant) route through this method
    /// rather than paying the carrier allocation for the throwaway
    /// diagnostic.
    ///
    /// Sibling posture to [`Self::contains_label`] one axis over:
    /// both walk [`Self::ALL`] keyed on [`Self::label`], but the
    /// return-type axis partitions the consumer surface —
    /// [`Self::contains_label`] returns a `bool` for the pure
    /// predicate `if`-gates / `filter`-passes / lint checks route
    /// through, this method returns the typed `Option<Self>` for
    /// consumers that need the decoded variant. The two arms of the
    /// axis compose: [`Self::contains_label`] is the default-body
    /// projection of `Self::find_by_label(s).is_some()`, and
    /// [`Self::parse_label`] is the default-body projection of
    /// `Self::find_by_label(s).ok_or_else(|| Self::make_unknown(s))`
    /// — every closed-set sweep threads through ONE primitive.
    ///
    /// The (return-type × side-effect) cross-product of the
    /// closed-set membership surface partitions exhaustively:
    ///
    /// | Return                     | Allocating (materialize `Unknown`) | Non-Allocating              |
    /// |----------------------------|------------------------------------|-----------------------------|
    /// | `Result<Self, Unknown>`    | [`Self::parse_label`]              | —                           |
    /// | `Option<Self>`             | —                                  | [`Self::find_by_label`]     |
    /// | `bool`                     | —                                  | [`Self::contains_label`]    |
    ///
    /// Default body is `Self::ALL.iter().copied().find(|v| v.label()
    /// == s)` — a linear sweep composed from the same TWO substrate
    /// primitives ([`Self::ALL`], [`Self::label`])
    /// [`Self::parse_label`] and [`Self::contains_label`] would walk
    /// over pre-lift, now lifted onto ONE trait body every consumer
    /// routes through. Implementors override only when the typed-
    /// decode surface needs to diverge from the natural
    /// `ALL`-projection (no production implementor reaches for this
    /// today; the axis exists for the same reason
    /// `via` / `set_label` / `labels` / `labels_joined` /
    /// `sorted_labels` / `sorted_labels_joined` / `suggest_closest` /
    /// `parse_label_with_hint` / `contains_label` overrides exist —
    /// a typed escape hatch the trait surface exposes rather than
    /// forcing the implementor to hand-roll the impl).
    ///
    /// Future consumers — a config-field decoder with a natural
    /// fallback (`T::find_by_label(cfg).unwrap_or(T::default_kind())`)
    /// that skips the throwaway Unknown allocation, a
    /// `filter_map`-shaped stream projection over cluster-wide
    /// `tatara.pleme.io/*` annotation keys, an LSP hover pass that
    /// resolves the typed variant under the operator's cursor
    /// WITHOUT allocating a carrier per non-matching hover, an
    /// iac-forge tag decode-loop that partitions each incoming tag
    /// stream into (typed_variant, bare_string) via
    /// `find_by_label(tag).ok_or(tag)` — bind to ONE trait method
    /// instead of hand-rolling either the `parse_label(s).ok()`
    /// carrier-allocating shortcut (which pays the `String`
    /// allocation on every reject) OR the inline
    /// `Self::ALL.iter().copied().find(|v| v.label() == s)`
    /// composition (which re-derives the sweep at every call site).
    ///
    /// THEORY.md §III — the typescape; the zero-allocation typed
    /// decode becomes a TYPE projection on the closed-set algebra
    /// rather than an inline `iter().find(|v| v.label() == s)`
    /// composition at every downstream consumer.
    /// THEORY.md §V.1 — knowable platform; the zero-allocation typed
    /// decode was an unnamed compound of [`Self::ALL`] +
    /// [`Self::label`] + `Iterator::find` pre-lift; naming it on
    /// the trait makes the projection a TYPED CONSEQUENCE of the
    /// two substrate primitives — generic consumers see ONE method,
    /// not ONE typed-decode-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the
    /// zero-allocation typed decode emerges from the composition of
    /// TWO substrate primitives ([`Self::ALL`], [`Self::label`])
    /// rather than as a per-implementor inline `iter+find` pair. A
    /// future tightening of either primitive (a future
    /// `#[closed_set(via = "…")]`-driven projection rename, a future
    /// canonicalization-aware label projection that folds case /
    /// whitespace) propagates to every closed-set consumer through
    /// ONE trait body — including [`Self::parse_label`],
    /// [`Self::contains_label`], and [`Self::suggest_closest`],
    /// which all default-body-delegate to this primitive.
    ///
    /// Frontier inspiration: Rust's `enum_iterator::first_matching`
    /// / Racket's `assf` (`(assf pred lst)` — first association whose
    /// predicate holds) over a closed association list stand as the
    /// same shape one vocabulary over on the finite-type-decode
    /// side. MLIR's `Operation::dyn_cast<T>` on the typed op registry
    /// is the same "look up the typed instance by discriminator,
    /// return `Option`, don't materialize a diagnostic on miss"
    /// axis over its closed-set-of-op-kinds. Translation through
    /// pleme-io primitives: a pure default method composing the
    /// trait's existing [`Self::ALL`] + [`Self::label`] surfaces
    /// with the standard-library `Iterator::find` primitive — no
    /// new dep, no new IR layer, no new per-role primitive.
    fn find_by_label(s: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|v| <Self as ClosedSet>::label(*v) == s)
    }

    /// Pure-membership predicate — `true` iff `s` matches some variant's
    /// [`Self::label`] exactly, `false` for every other string.
    ///
    /// Zero-allocation peer of [`Self::parse_label`]: `parse_label(s)`
    /// materializes the typed [`Self::Unknown`] carrier (owning a
    /// [`String`] copy of `s`) on the reject path even when the caller
    /// drops it immediately with `.is_ok()`; this method answers the
    /// SAME structural question — "is `s` a canonical label of this
    /// closed set" — without ever entering [`Self::make_unknown`]. The
    /// (allocating decode, non-allocating membership) axis of the
    /// closed-set surface partitions cleanly: [`Self::parse_label`]
    /// stays the load-bearing carrier-emitting path structured
    /// diagnostics route through, [`Self::contains_label`] stays the
    /// pure predicate `if`-gates / `filter`-passes / lint checks route
    /// through.
    ///
    /// Default body is `Self::ALL.iter().copied().any(|v| v.label() ==
    /// s)` — a linear sweep composed from the same TWO substrate
    /// primitives ([`Self::ALL`], [`Self::label`]) [`Self::parse_label`]
    /// walks over, without the [`Self::make_unknown`] carrier
    /// materialization the parse path threads on rejection.
    /// Implementors override only when the membership surface needs to
    /// diverge from the natural `ALL`-projection (no production
    /// implementor reaches for this today; the axis exists for the same
    /// reason `via` / `set_label` / `labels` / `labels_joined` /
    /// `sorted_labels` / `sorted_labels_joined` / `suggest_closest` /
    /// `parse_label_with_hint` overrides exist — a typed escape hatch
    /// the trait surface exposes rather than forcing the implementor to
    /// hand-roll the impl).
    ///
    /// Future consumers — a lint that flags unknown kinds without
    /// emitting a full structured diagnostic, an LSP hover pass that
    /// highlights known canonical labels without decoding them, an
    /// annotation-filter gate over cluster-wide `tatara.pleme.io/*`
    /// signal keys, an iac-forge tag pre-check that partitions valid
    /// canonical tags from unknown ones before committing to the typed
    /// decode — bind to ONE trait method instead of hand-rolling the
    /// `parse_label(s).is_ok()` shortcut (which pays the carrier
    /// allocation on every reject) or the inline
    /// `Self::ALL.iter().any(|v| v.label() == s)` composition (which
    /// re-derives the sweep at every call site) at each callsite, and
    /// the closed-set projection's pure-membership surface evolves at
    /// ONE site rather than per-consumer.
    ///
    /// THEORY.md §V.1 — knowable platform; the pure-membership
    /// predicate was an unnamed compound of [`Self::ALL`] +
    /// [`Self::label`] + `Iterator::any` pre-lift; naming it on the
    /// trait makes the projection a TYPED CONSEQUENCE of the two
    /// substrate primitives — generic consumers see ONE method, not
    /// ONE membership-shape-per-crate. Sibling posture to
    /// [`Self::parse_label`] on the (allocating decode, non-allocating
    /// membership) axis: both walk `Self::ALL` keyed on
    /// [`Self::label`], but the return-type axis partitions the
    /// consumer surface — carrier-emitting decoders take
    /// [`Self::parse_label`], predicate-gated filters take
    /// [`Self::contains_label`].
    /// THEORY.md §VI.1 — generation over composition; the
    /// pure-membership predicate emerges from the composition of TWO
    /// substrate primitives ([`Self::ALL`], [`Self::label`]) rather
    /// than as a per-implementor inline `iter+any` pair. A future
    /// tightening of either primitive (a future
    /// `#[closed_set(via = "…")]`-driven projection rename, a future
    /// canonicalization-aware label projection that folds case /
    /// whitespace) propagates to every closed-set consumer through
    /// ONE trait body.
    ///
    /// Frontier inspiration: MLIR's `Type::isa<T>()` and
    /// `Attribute::isa<T>()` typed predicates over the closed-set
    /// registry — the "is this thing of this closed-set-member kind"
    /// question emits at ONE typed method on the registry rather than
    /// at every downstream operation's inline `dyn_cast` sweep.
    /// Racket's `(member sym closed-list)` predicate over a closed
    /// association list stands as the same shape one vocabulary over
    /// on the homoiconic-Lisp side. Translation through pleme-io
    /// primitives: a pure default method composing the trait's
    /// existing [`Self::ALL`] + [`Self::label`] surfaces with the
    /// standard-library `Iterator::any` primitive — no new dep, no
    /// new IR layer, no new per-role primitive.
    fn contains_label(s: &str) -> bool {
        Self::find_by_label(s).is_some()
    }

    /// Collect every variant's canonical [`Self::label`] into a
    /// freshly-allocated `Vec<&'static str>` — `Self::ALL`'s elements
    /// projected through [`Self::label`], in declaration order.
    ///
    /// The substrate-wide
    /// `T::ALL.iter().map(|v| v.label()).collect::<Vec<_>>()` shape
    /// per-implementor test modules re-derived byte-for-byte
    /// (the `*_canonical_names_pinned` / `*_all_is_unique_and_complete`
    /// truth-table tests across `tatara-process` + `tatara-lisp`,
    /// pre-lift) lifted into ONE generic projection. Generic consumers
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

    /// Render the closed set's canonical labels joined by `sep` — the
    /// substrate-wide candidate-list-as-string shape consumers thread
    /// into structured-rejection diagnostics (`expected one of:
    /// nix/flux/lisp/container/aplicacao/guest`, `allowed: :name,
    /// :threshold`, `targets are kustomization|helm-release|deployment`).
    ///
    /// Default body composes [`Self::labels`] with
    /// [`slice::join`](https://doc.rust-lang.org/std/primitive.slice.html#method.join)
    /// — the rendering is a typed CONSEQUENCE of `Self::ALL` +
    /// `Self::label` + the chosen separator. Implementors override only
    /// when the join surface needs to diverge from the natural
    /// `labels().join(sep)` shape (no production implementor reaches for
    /// this today — the axis exists for the same reason `via`,
    /// `set_label`, `labels`, `suggest_closest`, `parse_label_with_hint`
    /// overrides exist: a typed escape hatch the trait surface exposes
    /// rather than forcing the implementor to hand-roll the impl).
    ///
    /// The substrate-wide `let parts: Vec<&'static str> = T::ALL.iter()
    /// .map(label).collect(); parts.join(sep)` shape recurred at FOUR
    /// `tatara-process` test sites pre-lift
    /// (`intent_error_empty_lists_every_kind_in_canonical_order`,
    /// `artifact_error_empty_lists_every_kind_in_canonical_order`,
    /// `channel_error_empty_lists_every_kind_in_canonical_order`,
    /// `encapsulation_kind_error_empty_lists_every_target_in_canonical_order`)
    /// each materializing the labels vec inline and asserting the join
    /// against a hand-rolled `*_KIND_LIST` / `*_TARGET_LIST` constant —
    /// past the ≥3 PRIME-DIRECTIVE trigger. Post-lift each test site
    /// reduces to a single `T::labels_joined(sep)` call and the
    /// candidate-list-as-string rendering binds at ONE trait method
    /// every closed-set consumer can lean on without re-deriving the
    /// `iter().map().collect().join()` triple.
    ///
    /// Production sites that need a `&'static str` (a hand-rolled
    /// `INTENT_KIND_LIST = "nix/flux/lisp/container/aplicacao/guest"`
    /// constant stored in an error variant slot) keep their per-site
    /// cached literal — this method runs at runtime and allocates a
    /// `String`, so it does NOT replace the `const &'static str` shape
    /// inline. Instead it stands as the canonical generative origin the
    /// per-site cached literal is pinned against (via the existing
    /// `*_error_empty_lists_every_kind_in_canonical_order` tests, now
    /// routing through this method), so a regression that drifts the
    /// production `&'static str` from the canonical join fails-loudly
    /// at the test site without per-implementor inline materialization.
    ///
    /// Future consumers — a metrics tagger that wants
    /// `expected_one_of=intent_kinds:nix,flux,lisp,…` in a Prometheus
    /// label, an LSP completion-bar renderer that wants
    /// `nix | flux | lisp | …` separators, a `tatara-check` diagnostic
    /// that wants `expected one of: nix, flux, lisp` for a
    /// natural-language rendering — bind to ONE trait method instead of
    /// hand-rolling the `iter+map+collect+join` triple at each call
    /// site, and the closed-set projection's separator surface evolves
    /// at ONE site rather than per-consumer.
    ///
    /// THEORY.md §V.1 — knowable platform; the joined-candidate-list
    /// shape was a known idiom carried by convention across 4+ test
    /// sites + indirectly across 4+ production `&'static str`
    /// constants. Lifting the join onto the trait makes the shape a
    /// TYPED CONSEQUENCE of [`Self::labels`] + the chosen separator —
    /// generic consumers see ONE method, not ONE join-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the
    /// joined-candidate-list rendering emerges from the composition of
    /// THREE substrate primitives ([`Self::ALL`], [`Self::label`], the
    /// caller-supplied separator) rather than as a per-implementor
    /// inline `collect().join()` triple. A future tightening of the
    /// candidate-list shape (a future Oxford-comma "..., or X" surface,
    /// a future Unicode-aware separator) lands at ONE primitive and
    /// propagates to every closed-set consumer.
    ///
    /// Frontier inspiration: Idris's `show` on closed-set enumerations
    /// — the candidate list emits as a single typed projection on the
    /// finite-type universe rather than per-instance inline rendering.
    /// Translation through pleme-io primitives: a pure default method
    /// composing the trait's existing [`Self::labels`] surface with the
    /// `slice::join` standard-library primitive — no new dep, no new
    /// IR layer.
    fn labels_joined(sep: &str) -> ::std::string::String {
        <Self as ClosedSet>::labels().join(sep)
    }

    /// Project [`Self::labels`] into ASCII-`sort_unstable` lexicographic
    /// order — the substrate-wide canonical candidate-list ordering every
    /// per-implementor `_all_is_unique_and_complete` test inlines a
    /// hand-rolled `let mut sorted: Vec<&str> = T::ALL.iter().map(label)
    /// .collect(); sorted.sort_unstable();` triple to materialize.
    ///
    /// Default body composes [`Self::labels`] with
    /// [`slice::sort_unstable`](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_unstable)
    /// — the sorted-rendering is a typed CONSEQUENCE of `Self::ALL` +
    /// `Self::label` + lexicographic order on `&str`. Implementors
    /// override only when the sort surface needs to diverge from the
    /// natural `labels().sort_unstable()` shape (no production
    /// implementor reaches for this today — the axis exists for the same
    /// reason `via`, `set_label`, `labels`, `labels_joined`,
    /// `suggest_closest`, `parse_label_with_hint` overrides exist: a
    /// typed escape hatch the trait surface exposes rather than forcing
    /// the implementor to hand-roll the impl).
    ///
    /// The substrate-wide `let mut sorted: Vec<&str> = T::ALL.iter()
    /// .map(<via>).collect(); sorted.sort_unstable();` triple recurred
    /// at SEVEN test sites pre-lift (`quote_form_all_is_unique_and_
    /// complete`, `atom_kind_all_is_unique_and_complete`,
    /// `kwarg_path_kind_all_is_unique_and_complete`,
    /// `expected_kwarg_shape_all_is_unique_and_complete`,
    /// `sexp_shape_all_is_unique_and_complete`,
    /// `unquote_form_all_is_unique_and_complete`,
    /// `macro_def_head_all_is_unique_and_complete`) each materializing
    /// the labels vec inline and sorting it in place before asserting
    /// against a hand-rolled sorted truth-table — past the ≥3
    /// PRIME-DIRECTIVE trigger once the per-test inline triple is named.
    /// Post-lift each test site reduces to `assert_eq!(T::sorted_labels(),
    /// vec![<truth-table>])` and the canonical-ordered candidate-list
    /// surface binds at ONE trait method every closed-set consumer can
    /// lean on without re-deriving the `iter+map+collect+sort` quadruple.
    ///
    /// Distinctness of the sorted result is already a substrate-wide
    /// invariant pinned by [`assert_closed_set_well_formed`] (clause 3 —
    /// labels are pairwise distinct), so the per-implementor `sorted ==
    /// deduped` redundant double-check the inline triple carried can
    /// retire alongside the materialization itself; the truth-table
    /// comparison (the per-implementor unique payload — `vec!["'", ",",
    /// ",@", "`"]` for `QuoteForm`, `vec!["bool", "float", "int",
    /// "keyword", "string", "symbol"]` for `AtomKind`, …) stays at the
    /// per-implementor test site as the load-bearing per-enum ground
    /// truth this lift does NOT subsume.
    ///
    /// Future consumers — an LSP completion bar that wants
    /// `nix | flux | lisp | container | aplicacao | guest` in
    /// alphabetical order, a `tatara-check` diagnostic that wants
    /// `expected one of: aplicacao, container, flux, guest, lisp, nix`
    /// for an alphabetized natural-language rendering, a typed
    /// near-miss metric that wants the candidate set in a
    /// deterministic-across-machines canonical order — bind to ONE
    /// trait method instead of hand-rolling the
    /// `iter+map+collect+sort` quadruple at each call site, and the
    /// closed-set projection's canonical-ordering surface evolves at
    /// ONE site rather than per-consumer.
    ///
    /// THEORY.md §V.1 — knowable platform; the sorted-candidate-list
    /// shape was a known idiom carried by convention across 7+ test
    /// sites. Lifting the sort onto the trait makes the shape a TYPED
    /// CONSEQUENCE of [`Self::labels`] + ASCII lexicographic ordering —
    /// generic consumers see ONE method, not ONE sort-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the
    /// sorted-candidate-list rendering emerges from the composition of
    /// THREE substrate primitives ([`Self::ALL`], [`Self::label`],
    /// `slice::sort_unstable`) rather than as a per-implementor inline
    /// `collect+sort` pair. A future tightening of the canonical-
    /// ordering surface (a future Unicode-collation-aware sort, a
    /// future declaration-order sibling) lands at ONE primitive and
    /// propagates to every closed-set consumer.
    ///
    /// Frontier inspiration: Idris's `show` over a finite-type universe
    /// — the canonical-ordered listing emits as a single typed
    /// projection rather than per-instance inline rendering. Translation
    /// through pleme-io primitives: a pure default method composing the
    /// trait's existing [`Self::labels`] surface with the
    /// `slice::sort_unstable` standard-library primitive — no new dep,
    /// no new IR layer.
    fn sorted_labels() -> ::std::vec::Vec<&'static str> {
        let mut labels = <Self as ClosedSet>::labels();
        labels.sort_unstable();
        labels
    }

    /// Render the closed set's canonical labels in ASCII-`sort_unstable`
    /// lexicographic order, joined by `sep` — the substrate-wide
    /// canonical-ordered candidate-list-as-string shape consumers thread
    /// into structured-rejection diagnostics that want alphabetized
    /// rendering (`expected one of: aplicacao/container/flux/guest/lisp/nix`,
    /// LSP completion bars sorted for humans, `tatara-check` "did you
    /// mean X?" hints that walk a byte-wise-sorted candidate table).
    ///
    /// Default body composes [`Self::sorted_labels`] with
    /// [`slice::join`](https://doc.rust-lang.org/std/primitive.slice.html#method.join)
    /// — the sorted-and-joined rendering is a typed CONSEQUENCE of
    /// [`Self::ALL`] + [`Self::label`] + ASCII lexicographic ordering +
    /// the chosen separator. Implementors override only when the sort
    /// / join surface needs to diverge from the natural
    /// `sorted_labels().join(sep)` shape (no production implementor
    /// reaches for this today — the axis exists for the same reason
    /// `via`, `set_label`, `labels`, `labels_joined`, `sorted_labels`,
    /// `suggest_closest`, `parse_label_with_hint` overrides exist: a
    /// typed escape hatch the trait surface exposes rather than forcing
    /// the implementor to hand-roll the impl).
    ///
    /// Sibling posture to the closed set of substrate-wide
    /// candidate-list-as-string projections: [`Self::labels_joined`]
    /// renders declaration-ordered labels (the `INTENT_KIND_LIST`-shaped
    /// production constants that pin canonical serialization order),
    /// [`Self::sorted_labels`] returns lexicographic-ordered labels as
    /// a `Vec<&'static str>` (the truth-table shape per-implementor
    /// `_all_is_unique_and_complete` tests key on), and THIS method
    /// closes the third corner — the lexicographic-ordered
    /// candidate-list-as-string. The three projections partition the
    /// (declaration-vs-lexicographic ordering, Vec-vs-String surface)
    /// cross-product exhaustively: declaration+Vec is [`Self::labels`],
    /// declaration+String is [`Self::labels_joined`], lexicographic+Vec
    /// is [`Self::sorted_labels`], lexicographic+String is this method.
    /// A future consumer that wants a fifth surface (Oxford-comma joins,
    /// Unicode-collation-aware sorting, a bulleted-list renderer) lands
    /// at ONE additional trait method composed from these primitives,
    /// not per-implementor.
    ///
    /// Future consumers — an LSP completion bar rendering
    /// `aplicacao | container | flux | guest | lisp | nix` in
    /// alphabetized grammar-style form, a `tatara-check` diagnostic
    /// rendering `expected one of: aplicacao, container, flux, guest,
    /// lisp, nix` for a natural-language alphabetized surface, a
    /// deterministic-across-machines metric label whose canonical
    /// ordering must not depend on `Self::ALL`'s declaration order —
    /// bind to ONE trait method instead of hand-rolling the
    /// `sorted_labels().join(sep)` compound at each call site, and the
    /// closed-set projection's alphabetized-rendering surface evolves
    /// at ONE site rather than per-consumer.
    ///
    /// THEORY.md §V.1 — knowable platform; the alphabetized
    /// candidate-list-as-string shape sits as an unnamed compound of
    /// [`Self::sorted_labels`] + [`slice::join`] pre-lift; naming it on
    /// the trait makes the projection a TYPED CONSEQUENCE of
    /// [`Self::labels`] + ASCII lexicographic ordering + the chosen
    /// separator — generic consumers see ONE method, not ONE
    /// sort-then-join compound per crate.
    /// THEORY.md §VI.1 — generation over composition; the
    /// alphabetized-candidate-list rendering emerges from the
    /// composition of FOUR substrate primitives ([`Self::ALL`],
    /// [`Self::label`], `slice::sort_unstable`, `slice::join`) rather
    /// than as a per-consumer inline `sort+join` pair. A future
    /// tightening of either primitive (a Unicode-collation-aware sort,
    /// an Oxford-comma-aware join, a locale-sensitive rendering)
    /// propagates to every closed-set consumer through ONE trait body.
    ///
    /// Frontier inspiration: Idris's `show` composed with `sort` over a
    /// finite-type universe — the canonical-ordered rendering emits as
    /// a single typed projection on the finite-type layer rather than
    /// per-instance inline compound. Translation through pleme-io
    /// primitives: a pure default method composing the trait's existing
    /// [`Self::sorted_labels`] surface with the `slice::join`
    /// standard-library primitive — no new dep, no new IR layer.
    fn sorted_labels_joined(sep: &str) -> ::std::string::String {
        <Self as ClosedSet>::sorted_labels().join(sep)
    }

    /// Collect every typed variant into a freshly-allocated `Vec<Self>`
    /// ordered by ASCII lexicographic [`Self::label`] — the typed-variant
    /// sibling of [`Self::sorted_labels`] on the (typed variant,
    /// canonical label) axis of the closed-set candidate-listing surface.
    ///
    /// Peer of [`Self::sorted_labels`] one axis over on the (typed
    /// variant, `&'static str` label) return-type axis: both walk
    /// [`Self::ALL`] and project through [`Self::label`] to key the
    /// ordering, but the return-type axis partitions the consumer
    /// surface — LSP completion / `tatara-check` / metrics consumers
    /// that render the label alone (`expected one of: alpha, beta, gamma`)
    /// take [`Self::sorted_labels`], consumers that need the typed
    /// variant next to the rendered label (an LSP completion API whose
    /// selected item is a typed variant the caller reads back, a
    /// `<variant.label()>: <count>` diagnostic that iterates
    /// per-variant payloads in a machine-independent canonical order,
    /// a metrics tagger that walks typed variants deterministically
    /// across binaries) take this method. The two arms of the axis
    /// compose element-wise: `Self::sorted_variants()[i].label()` equals
    /// `Self::sorted_labels()[i]` for every `i in 0..Self::CARDINALITY`
    /// — the load-bearing invariant the well-formedness sweep's clause
    /// (17) pins.
    ///
    /// The (return-type × ordering) 2×2 matrix on the closed-set
    /// candidate-listing surface partitions post-lift:
    ///
    /// | Ordering       | `Vec<&'static str>`   | `Vec<Self>`                    |
    /// |----------------|-----------------------|--------------------------------|
    /// | Declaration    | [`Self::labels`]      | `Self::ALL.iter().copied()`    |
    /// | Lexicographic  | [`Self::sorted_labels`] | [`Self::sorted_variants`]   |
    ///
    /// The declaration + `Vec<Self>` corner stays at the direct
    /// [`Self::ALL`] slice iterator — no primitive lifts, no default
    /// body, no override axis, since `Self::ALL.iter().copied()` is the
    /// natural zero-primitive projection. The lexicographic +
    /// `Vec<Self>` corner — this method — is the missing lift: sorting
    /// a `Vec<Self>` by label is a non-trivial composition of
    /// [`Self::ALL`] + [`Self::label`] + `slice::sort_unstable_by_key`
    /// that recurs at every prospective consumer site (an LSP
    /// completion pass, a `tatara-check` per-variant diagnostic, a
    /// deterministic-across-machines metric tagger) as the same
    /// `T::ALL.to_vec().sort_unstable_by_key(|v| v.label())` triple.
    ///
    /// Default body composes [`Self::ALL`] with `Vec::from` +
    /// `slice::sort_unstable_by_key` keyed on [`Self::label`] — the
    /// sorted-variant rendering is a typed CONSEQUENCE of `Self::ALL` +
    /// `Self::label` + ASCII lexicographic ordering on the label
    /// projection. Distinctness of the labels is already a substrate-
    /// wide invariant pinned by [`assert_closed_set_well_formed`]
    /// (clause 3 — labels are pairwise distinct), so unstable sorting
    /// is deterministic on every implementor by construction —
    /// `sort_unstable_by_key` never observes two equal keys to
    /// reorder. Implementors override only when the ordering surface
    /// needs to diverge from the natural label-keyed lexicographic
    /// projection (no production implementor reaches for this today;
    /// the axis exists for the same reason `via` / `set_label` /
    /// `labels` / `sorted_labels` overrides exist — a typed escape
    /// hatch the trait surface exposes rather than forcing the
    /// implementor to hand-roll the impl).
    ///
    /// Future consumers — an LSP completion pass that returns typed
    /// variants (not just labels) so the selected completion item
    /// short-circuits back into `T` without a re-decode through
    /// [`Self::find_by_label`], a `tatara-check` diagnostic that
    /// renders per-variant projections (`<variant.label()>:
    /// <variant.short_label()>` diagnostics on the double-label
    /// surface `ProcessSignal` / `ConditionKind` carry) in a
    /// machine-independent canonical order, a metrics tagger that
    /// walks typed variants deterministically across binaries so
    /// per-variant counter payloads emit in the same order on every
    /// build, a per-variant lookup table `[Payload; T::CARDINALITY]`
    /// exhaustively rendered as `(label, payload)` pairs in
    /// alphabetical order — bind to ONE trait method instead of
    /// hand-rolling the `let mut v: Vec<Self> = T::ALL.to_vec(); v
    /// .sort_unstable_by_key(|x| x.label()); v` triple at each call
    /// site, and the closed-set typed-variant canonical-ordering
    /// surface evolves at ONE site rather than per-consumer.
    ///
    /// THEORY.md §III — the typescape; the (typed variant,
    /// lexicographic ordering) projection becomes a TYPE projection on
    /// the trait rather than a per-consumer hand-rolled
    /// `T::ALL.to_vec().sort_unstable_by_key(|v| v.label())` triple at
    /// every downstream stable-ordering site. The (return-type ×
    /// ordering) 2×2 matrix partitions the closed-set
    /// candidate-listing surface exhaustively into FOUR corners
    /// (declaration + `Vec<&str>`, declaration + `Vec<Self>`,
    /// lexicographic + `Vec<&str>`, lexicographic + `Vec<Self>`),
    /// each with a distinct load-bearing consumer surface.
    /// THEORY.md §V.1 — knowable platform; the sorted-typed-variants
    /// shape was an unnamed compound of [`Self::ALL`] +
    /// [`Self::label`] + `slice::sort_unstable_by_key` pre-lift.
    /// Naming it on the trait makes the projection a TYPED CONSEQUENCE
    /// of the two substrate primitives + the label-keyed lexicographic
    /// ordering — generic consumers see ONE method, not ONE
    /// sort-by-label shape per crate.
    /// THEORY.md §VI.1 — generation over composition; the
    /// sorted-typed-variants rendering emerges from the composition of
    /// THREE substrate primitives ([`Self::ALL`], [`Self::label`],
    /// `slice::sort_unstable_by_key`) rather than as a per-implementor
    /// inline `to_vec+sort_unstable_by_key` triple. A future tightening
    /// of any primitive (a Unicode-collation-aware sort, a
    /// `#[closed_set(via = "…")]`-driven projection rename, a
    /// canonicalization-aware label projection that folds case /
    /// whitespace) propagates to every closed-set typed-variant
    /// canonical-ordering consumer through ONE trait body — including
    /// [`Self::sorted_labels`], which stays element-wise aligned
    /// through the well-formedness contract.
    ///
    /// Frontier inspiration: Idris's `sortBy` over a finite-type
    /// universe keyed on a canonical `show` projection — the
    /// canonical-ordered typed listing emits as a single projection on
    /// the finite-type layer rather than per-instance inline compound.
    /// MLIR's `OperationName::getSortedRegisteredOps` on the Op
    /// registry returns typed op-names in a canonical order the
    /// DiagnosticEngine renders per-kind diagnostics against; Racket's
    /// `(sort (enum->list T) #:key T-label)` composes the enum's
    /// canonical listing with a key-projected sort in the same
    /// vocabulary one dispatch axis over. Translation through
    /// pleme-io primitives: a pure default method composing the
    /// trait's existing [`Self::ALL`] surface with `Vec::from` and
    /// `slice::sort_unstable_by_key` keyed on [`Self::label`] — no
    /// new dep, no new IR layer, no supertrait bound.
    fn sorted_variants() -> ::std::vec::Vec<Self> {
        let mut variants: ::std::vec::Vec<Self> = Self::ALL.to_vec();
        variants.sort_unstable_by_key(|v| <Self as ClosedSet>::label(*v));
        variants
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
        Self::find_by_label(target)
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

    /// Zero-allocation typed decode of `s`, threading a typed
    /// [`Self::suggest_closest`] hint into the rejection envelope —
    /// the structured-diagnostic surface that composes
    /// [`Self::find_by_label`] + [`Self::suggest_closest`] into ONE
    /// call a downstream LSP / config-decoder / filter-map consumer
    /// takes as `T: ClosedSet` WITHOUT paying the
    /// [`Self::make_unknown`] carrier allocation
    /// [`Self::parse_label_with_hint`] threads on rejection.
    ///
    /// On exact match returns `Ok(v)` — the hint slot stays absent
    /// because [`Self::suggest_closest`] is "near-miss only" by
    /// contract (a successful lookup short-circuits before
    /// [`Self::suggest_closest`] runs, so the substrate-wide
    /// "did you mean …?" surface never double-emits the same
    /// variant once as a successful decode and once as a hint).
    /// On miss returns `Err(hint)` where `hint` is the typed variant
    /// [`Self::suggest_closest`] keys on — `Some(v)` when a
    /// canonical label sits within the substrate-wide bounded edit
    /// distance, `None` when no candidate qualifies (the
    /// conservative-suggestion contract — silent over guessing).
    ///
    /// Peer of [`Self::parse_label_with_hint`] on the (allocating
    /// carrier decode, non-allocating typed decode) axis of the
    /// closed-set surface: [`Self::parse_label_with_hint`]
    /// materializes the typed [`Self::Unknown`] carrier (owning a
    /// [`String`] copy of `s`) on the reject path even when the
    /// caller drops it immediately with `.map_err(|(_, hint)| hint)`;
    /// this method answers the SAME structural question —
    /// "which canonical variant does `s` decode to (or hint at), if
    /// any?" — without ever entering [`Self::make_unknown`].
    ///
    /// The (side-effect × hint) 2×2 matrix over the closed-set
    /// decoded-arm return type partitions exhaustively post-lift:
    ///
    /// | Side-effect on reject         | No hint                     | With hint                          |
    /// |-------------------------------|-----------------------------|------------------------------------|
    /// | Allocating (materialize carrier) | [`Self::parse_label`]    | [`Self::parse_label_with_hint`]    |
    /// | Non-allocating (typed decode) | [`Self::find_by_label`]     | [`Self::find_by_label_with_hint`]  |
    ///
    /// Default body composes [`Self::find_by_label`] with
    /// [`Self::suggest_closest`] verbatim — the structured shape is
    /// a typed CONSEQUENCE of the two pre-existing primitives, not a
    /// third codepath. Implementors override only when the
    /// composition needs to diverge (no production implementor
    /// reaches for this today; the axis exists for the same reason
    /// `via` / `set_label` / `labels` / `suggest_closest` /
    /// `parse_label_with_hint` overrides exist — a typed escape
    /// hatch the trait surface exposes rather than forcing the
    /// implementor to hand-roll the impl). An implementor that
    /// overrides [`Self::find_by_label`] propagates the override
    /// through this default body to the structured typed-decode arm
    /// automatically; the (allocating carrier decode, non-allocating
    /// typed decode) axis funnels every sweep through ONE typed
    /// primitive on each of the (no-hint, with-hint) columns.
    ///
    /// Future consumers — an LSP hover pass that resolves the typed
    /// variant under the operator's cursor AND (on miss) renders a
    /// `did you mean <v.label()>?` next to a bare rejection WITHOUT
    /// paying carrier allocation per non-matching hover, a
    /// config-field decoder with a natural fallback AND a typed
    /// hint the operator sees when the field's value is a near-miss
    /// (`T::find_by_label_with_hint(cfg).unwrap_or_else(|hint|
    /// { emit_hint(hint); T::default_kind() })`), a `filter_map`-
    /// shaped stream projection over cluster-wide `tatara.pleme.io/*`
    /// annotation keys that partitions each element into
    /// (typed_variant, typed_hint, bare_unrecognized_key) via
    /// `find_by_label_with_hint` — bind to ONE trait method instead
    /// of hand-rolling the
    /// `find_by_label(s).ok_or_else(|| suggest_closest(s))`
    /// composition at each callsite, and the closed-set
    /// zero-allocation structured-decode surface evolves at ONE site
    /// rather than per-consumer.
    ///
    /// THEORY.md §III — the typescape; the structured typed-decode
    /// becomes a TYPE projection on the trait rather than a
    /// per-consumer hand-rolled
    /// (`find_by_label(s).ok_or_else(|| suggest_closest(s))`) call
    /// at every zero-allocation decode boundary. The (allocating
    /// carrier decode, non-allocating typed decode) × (no-hint,
    /// with-hint) 2×2 matrix partitions the structured-decode
    /// surface exhaustively into FOUR typed projections, each with
    /// a distinct load-bearing consumer surface.
    /// THEORY.md §V.1 — knowable platform; the structured
    /// typed-decode was an unnamed compound of
    /// [`Self::find_by_label`] + [`Self::suggest_closest`] pre-lift.
    /// Naming it on the trait makes the projection a TYPED
    /// CONSEQUENCE of the two substrate primitives — generic
    /// consumers see ONE method, not ONE structured-decode-shape-
    /// per-crate.
    /// THEORY.md §VI.1 — generation over composition; the
    /// structured-diagnostic shape emerges from the composition of
    /// TWO substrate primitives ([`Self::find_by_label`],
    /// [`Self::suggest_closest`]) rather than as a per-implementor
    /// structured-decode impl. A future tightening of either
    /// primitive (a future perfect-hash lookup on
    /// [`Self::find_by_label`], a future Damerau-Levenshtein lift
    /// on [`Self::suggest_closest`], a future case-insensitive
    /// projection axis) propagates to every closed-set structured
    /// zero-allocation consumer through ONE trait body.
    ///
    /// Frontier inspiration: rustc's `find_best_match_for_name`
    /// composed with `Symbol::intern` — the typed-symbol lookup with
    /// a bounded near-miss adornment slot, without materializing a
    /// diagnostic on miss when the caller supplies a natural
    /// fallback. MLIR's `Operation::dyn_cast<T>` composed with the
    /// diagnostic engine's registered "did you mean" hook — the
    /// typed op lookup returns `Option<T>` on miss, and the hint is
    /// a separate typed projection over the op-kind registry.
    /// Translation through pleme-io primitives: a pure default
    /// method composing the trait's existing [`Self::find_by_label`]
    /// with [`Self::suggest_closest`] — no new primitive, no new
    /// dep, no new IR layer.
    fn find_by_label_with_hint(s: &str) -> Result<Self, Option<Self>> {
        match Self::find_by_label(s) {
            Some(v) => Ok(v),
            None => Err(Self::suggest_closest(s)),
        }
    }

    /// Project `self` onto its zero-indexed position in [`Self::ALL`] —
    /// the reverse projection of the [`Self::ALL`] slice's array-index
    /// surface. Closes the (variant → position, position → variant)
    /// bijection with `0..Self::CARDINALITY` that every per-variant
    /// lookup-table / bitset / compact-encoding consumer binds to.
    ///
    /// The forward direction — position → variant — is the plain
    /// `Self::ALL[i]` array-indexing surface every consumer already
    /// walks; the reverse direction — variant → position — is this
    /// method. Together they form the bijection
    /// `Self ↔ 0..Self::CARDINALITY` that composes with
    /// [`Self::CARDINALITY`] into a typed const-generic surface
    /// downstream consumers reach for whenever they need to key a
    /// per-variant payload without hand-rolling the sweep.
    ///
    /// Sibling posture to [`Self::find_by_label`] on the (label decode,
    /// index decode) axis of the closed-set surface: [`Self::find_by_label`]
    /// projects a `&str` onto its typed variant through the label
    /// projection, this method projects a typed variant onto its
    /// `usize` position through the [`Self::ALL`] slice. Both walk
    /// [`Self::ALL`] as the single load-bearing per-variant listing,
    /// keyed by a different projection — the (Self → &str, Self →
    /// usize) return-type axis partitions the (typed variant →
    /// canonical projection) surface exhaustively into TWO typed
    /// projections, each with a distinct load-bearing consumer surface.
    ///
    /// The (return-type) axis of the closed-set variant → projection
    /// surface partitions post-lift:
    ///
    /// | Projection direction        | Projection surface        |
    /// |-----------------------------|---------------------------|
    /// | Typed variant → `&'static str` label | [`Self::label`]       |
    /// | Typed variant → `usize` array index  | [`Self::index_of`]    |
    ///
    /// Default body sweeps [`Self::ALL`] keyed on
    /// [`core::mem::discriminant`] — the discriminant-keyed comparator
    /// stays valid for every fieldless / typed enum implementor
    /// (`std::mem::discriminant` on any real enum returns a well-defined
    /// value per variant) WITHOUT forcing a `PartialEq` supertrait onto
    /// the [`ClosedSet`] contract. The trait's supertrait bound stays
    /// `Sized + Copy + 'static` — the minimum surface every implementor
    /// carries — and the discriminant primitive re-uses the enum's
    /// natural per-variant identity. Implementors override with a
    /// per-variant `match` when the O(N) sweep shows up on a hot-path
    /// profile (the substrate-wide typed-emission bind: no production
    /// site today calls `index_of` on a per-message hot path, so the
    /// default sweep costs nothing measurable, and the override axis
    /// exists for the same reason `via` / `set_label` / `labels`
    /// overrides exist — a typed escape hatch the trait surface exposes
    /// rather than forcing the implementor to hand-roll the impl).
    ///
    /// Panics if `Self::ALL` does not contain `self` — a structural
    /// bug at the implementor's [`Self::ALL`] declaration, not a
    /// runtime accident. The panic is guaranteed absent when the
    /// [`Self::ALL`] listing covers every variant (the well-formedness
    /// contract [`assert_closed_set_well_formed`]'s new clause (15)
    /// pins on every implementor), so a passing well-formedness sweep
    /// means every generic consumer can call `index_of` on any typed
    /// variant without threading an `Option` through the return.
    ///
    /// Future consumers — a per-variant lookup table `[Payload;
    /// T::CARDINALITY]` whose index is `variant.index_of()`, a bitset
    /// over the closed set that sets bit `variant.index_of()` per
    /// observed variant, a compact wire encoding that emits
    /// `variant.index_of() as u8` when the cardinality fits in a byte,
    /// a per-variant metrics counter table sized `[u64; T::CARDINALITY]`
    /// that increments `counters[variant.index_of()]` per sample — bind
    /// to ONE trait method instead of hand-rolling either
    /// `T::ALL.iter().position(|v| *v == variant).unwrap()` (which
    /// requires the caller to import `PartialEq` at every call site AND
    /// pay the sweep at every callsite) OR a per-implementor inline
    /// `match self { ... }` (which re-derives the per-variant literal
    /// index at every callsite).
    ///
    /// THEORY.md §III — the typescape; the (typed variant → array
    /// index) projection becomes a TYPE projection on the trait rather
    /// than a per-consumer hand-rolled `T::ALL.iter().position(|v| *v
    /// == self)` composition at every downstream indexing site. The
    /// (variant → `&str`, variant → `usize`) return-type axis
    /// partitions the (typed variant → canonical projection) surface
    /// exhaustively into TWO typed projections, each with a distinct
    /// load-bearing consumer surface — label decoding for
    /// [`Self::label`], array indexing for this method.
    /// THEORY.md §V.1 — knowable platform; the (variant → array
    /// index) projection was an unnamed compound of [`Self::ALL`] +
    /// `Iterator::position` + `PartialEq` pre-lift; naming it on the
    /// trait makes the projection a TYPED CONSEQUENCE of [`Self::ALL`]
    /// alone (the discriminant-keyed sweep re-uses the enum's natural
    /// per-variant identity WITHOUT a `PartialEq` bound) — generic
    /// consumers see ONE method, not ONE position-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the (variant →
    /// array index) projection emerges from the composition of ONE
    /// substrate primitive ([`Self::ALL`]) with the standard-library
    /// [`core::mem::discriminant`] projection and the standard-library
    /// `Iterator::position` primitive rather than as a per-implementor
    /// inline `match` block. A future tightening of [`Self::ALL`] (a
    /// future `#[closed_set(cardinality = N)]` derive attribute that
    /// pins N at the source, a future declaration-time position-
    /// assertion) propagates to every closed-set const-generic consumer
    /// through ONE trait method.
    ///
    /// Frontier inspiration: Idris's `Fin n` finite-cardinality type
    /// with `finToNat : Fin n -> Nat` — the finite-type universe
    /// exposes a canonical (element → natural) projection every
    /// downstream indexer binds to; MLIR's `TypeID` on the Op registry
    /// gives each Op kind a stable index the DiagnosticEngine keys
    /// per-kind counters on; Racket's `enum-index` on a closed enum
    /// projects a symbol onto its declaration-order position; Rust's
    /// `strum::EnumIter::position` composed with `PartialEq` — the
    /// same shape one vocabulary over. Translation through pleme-io
    /// primitives: a pure default method composing the trait's
    /// existing [`Self::ALL`] surface with `Iterator::position` keyed
    /// on [`core::mem::discriminant`] — no new dep, no new IR layer,
    /// no supertrait bound.
    fn index_of(self) -> usize {
        Self::ALL
            .iter()
            .position(|v| core::mem::discriminant(v) == core::mem::discriminant(&self))
            .expect(
                "ClosedSet::index_of: Self::ALL is missing self — implementor's ALL slice doesn't cover every variant",
            )
    }

    /// Recover the typed variant at declaration-order position `i` in
    /// [`Self::ALL`], or [`None`] if `i >= Self::CARDINALITY`.
    ///
    /// The typed inverse of [`Self::index_of`] on the (typed variant ↔
    /// `usize` array index) bijection: [`Self::index_of`] projects a
    /// variant onto its `usize` position through the [`Self::ALL`]
    /// slice; this method projects a `usize` position back onto its
    /// typed variant. Together the two projections close the bijection
    /// with `0..Self::CARDINALITY` at BOTH directions — every generic
    /// consumer that stores a `variant.index_of()` for later decode
    /// (a compact wire encoding that emits `variant.index_of() as u8`
    /// and later recovers the variant, a slotted lookup table
    /// `[Payload; T::CARDINALITY]` scanned back to `(variant, payload)`
    /// pairs for exhaustive iteration, a bitset over the closed set
    /// walked back to the set of observed variants, a metrics
    /// aggregator that stores per-index counters and later renders
    /// `<variant>: <count>` diagnostics) binds to ONE typed inverse
    /// method rather than hand-rolling either `Self::ALL.get(i).copied()`
    /// (which re-derives the same three-primitive composition at every
    /// call site) OR a per-implementor inline `match i { 0 => Some(v0),
    /// 1 => Some(v1), _ => None }` (which re-derives the per-variant
    /// literal → variant table at every callsite AND drifts silently
    /// when [`Self::ALL`] gains a new variant).
    ///
    /// Sibling posture to [`Self::find_by_label`] on the (label decode,
    /// index decode) axis of the closed-set inbound-projection surface:
    /// [`Self::find_by_label`] projects a `&str` label back onto its
    /// typed variant through the [`Self::ALL`] × [`Self::label`] sweep,
    /// this method projects a `usize` position back onto its typed
    /// variant through direct [`Self::ALL`] slice indexing. Both
    /// return an [`Option<Self>`] because the input carrier is wider
    /// than the closed set — every non-canonical `&str` decodes to
    /// [`None`] on the label side, every out-of-range `usize` decodes
    /// to [`None`] on the index side. Both share the SAME
    /// zero-allocation shape and the SAME `Option`-typed rejection
    /// arm; a generic consumer freely swaps between the two decode
    /// surfaces based on its carrier without changing the program's
    /// structured-decode semantics.
    ///
    /// The (return-type × input-carrier) axis of the closed-set
    /// inbound-projection surface partitions post-lift:
    ///
    /// | Input carrier    | Return type          | Projection surface        |
    /// |------------------|----------------------|---------------------------|
    /// | `&str` label     | `Result<Self, U>`    | [`Self::parse_label`]     |
    /// | `&str` label     | `Option<Self>`       | [`Self::find_by_label`]   |
    /// | `&str` label     | `bool`               | [`Self::contains_label`]  |
    /// | `usize` index    | `Option<Self>`       | [`Self::from_index`]      |
    ///
    /// Default body composes ONE substrate primitive ([`Self::ALL`])
    /// with the standard-library `<[T]>::get` bounded-index projection —
    /// no discriminant sweep, no `PartialEq` bound, no per-variant
    /// `match`. Implementors override with a per-index `match` when
    /// the O(1) slice lookup shows up on a hot-path profile (the
    /// substrate-wide typed-emission bind: no production site today
    /// calls `from_index` on a per-message hot path, so the default
    /// slice lookup costs nothing measurable, and the override axis
    /// exists for the same reason `via` / `set_label` / `labels` /
    /// `index_of` overrides exist — a typed escape hatch the trait
    /// surface exposes rather than forcing the implementor to
    /// hand-roll the impl).
    ///
    /// The bounded-index contract — the out-of-range arm returns
    /// [`None`] for every `i >= Self::CARDINALITY` — is guaranteed by
    /// the default `<[T]>::get` composition; the well-formedness
    /// contract [`assert_closed_set_well_formed`]'s new clause (16)
    /// pins the both-directions equality on every implementor, so a
    /// passing well-formedness sweep means every generic consumer can
    /// call `from_index` on any `usize` payload and expect the same
    /// `Option`-typed answer at every crate boundary.
    ///
    /// THEORY.md §III — the typescape; the (array index → typed
    /// variant) projection becomes a TYPE projection on the trait
    /// rather than a per-consumer inline `Self::ALL.get(i).copied()`
    /// composition at every downstream index-decode site. The
    /// (variant → `usize`, `usize` → variant) direction axis of the
    /// (variant ↔ array-index) bijection partitions exhaustively into
    /// TWO typed projections, each with a distinct load-bearing
    /// consumer surface — array indexing for [`Self::index_of`],
    /// variant recovery for this method.
    /// THEORY.md §V.1 — knowable platform; the (array index → typed
    /// variant) projection was an unnamed compound of [`Self::ALL`] +
    /// `<[T]>::get` + `Option::copied` pre-lift; naming it on the
    /// trait makes the projection a TYPED CONSEQUENCE of
    /// [`Self::ALL`] — generic consumers see ONE method, not ONE
    /// slice-lookup-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the (array
    /// index → typed variant) projection emerges from the composition
    /// of ONE substrate primitive ([`Self::ALL`]) with the
    /// standard-library `<[T]>::get` bounded-index projection and the
    /// standard-library `Option::copied` primitive rather than as a
    /// per-implementor inline `match` block. A future tightening of
    /// [`Self::ALL`] (a future `#[closed_set(cardinality = N)]` derive
    /// attribute that pins N at the source, a future declaration-time
    /// position-assertion) propagates to every closed-set const-generic
    /// inverse-decode consumer through ONE trait method.
    ///
    /// Frontier inspiration: Idris's `Fin n` finite-cardinality type
    /// with `natToFin : Nat -> (n : Nat) -> Maybe (Fin n)` — the
    /// finite-type universe exposes a canonical (natural → element)
    /// bounded-decode projection every downstream compact-encoding
    /// binds to, complementing `finToNat` in the opposite direction;
    /// Racket's `enum->object` on a closed enum decodes an index back
    /// to its variant; MLIR's `RegisteredOperationName::get(int)` on
    /// the Op registry decodes a stable index back to its Op kind;
    /// Rust's `strum::EnumIter::nth` composed with `Iterator::nth` —
    /// the same shape one vocabulary over. Translation through
    /// pleme-io primitives: a pure default method composing the
    /// trait's existing [`Self::ALL`] surface with `<[T]>::get` and
    /// `Option::copied` — no new dep, no new IR layer, no supertrait
    /// bound.
    fn from_index(i: usize) -> Option<Self> {
        Self::ALL.get(i).copied()
    }

    /// The declaration-order first variant of the closed set —
    /// `Self::ALL[0]` projected onto the trait surface as a
    /// panic-free typed anchor. Closes the (endpoint = 0) corner of
    /// the closed-set endpoint-anchor axis.
    ///
    /// Sibling posture to [`Self::last`] one axis over on the
    /// (endpoint = 0, endpoint = `CARDINALITY - 1`) partition of the
    /// closed-set endpoint surface: [`Self::first`] returns the
    /// declaration-order head, [`Self::last`] returns the
    /// declaration-order tail. Together the two anchors bracket
    /// [`Self::ALL`] at its two structural endpoints without forcing
    /// generic consumers to either (a) index into the slice directly
    /// (`Self::ALL[0]` / `Self::ALL[Self::ALL.len() - 1]`) — which
    /// makes the endpoint axis a per-consumer duplicated composition
    /// of [`Self::ALL`] + `<[T]>::first` / `<[T]>::last` +
    /// [`Option::copied`] + `Option::unwrap` — OR (b) route through
    /// [`Self::from_index`] with a hand-rolled
    /// `.unwrap_or_else(|| unreachable!())` at each callsite (which
    /// re-derives the `0`-index / `CARDINALITY - 1`-index literal at
    /// every downstream site AND pays an [`Option`]-typed dispatch
    /// the closed-set non-empty contract structurally forbids). Both
    /// endpoints are guaranteed to exist by the well-formedness
    /// contract [`assert_closed_set_well_formed`]'s clause (1) — a
    /// closed set with zero variants is a degenerate codomain the
    /// substrate rejects at the well-formedness boundary — so the
    /// endpoint anchors emit a bare typed variant with no [`Option`]
    /// / [`Result`] indirection.
    ///
    /// The (endpoint × direction) 1×2 matrix over the closed-set
    /// endpoint-anchor surface partitions post-lift:
    ///
    /// | Endpoint direction        | Anchor surface       |
    /// |---------------------------|----------------------|
    /// | Declaration-order head    | [`Self::first`]      |
    /// | Declaration-order tail    | [`Self::last`]       |
    ///
    /// Default body composes ONE substrate primitive ([`Self::ALL`])
    /// with the standard-library slice-index-0 projection — the head
    /// anchor is a typed CONSEQUENCE of [`Self::ALL`] + the non-empty
    /// contract, not a per-implementor `const HEAD: Self = ...`
    /// declaration. Implementors override only when the endpoint
    /// surface needs to diverge from the natural `ALL[0]` shape (no
    /// production implementor reaches for this today; the axis exists
    /// for the same reason `via` / `set_label` / `labels` /
    /// `sorted_labels` / `sorted_variants` / `from_index` overrides
    /// exist — a typed escape hatch the trait surface exposes rather
    /// than forcing the implementor to hand-roll the impl).
    ///
    /// Future consumers — a config-field decoder that binds the
    /// closed-set's canonical default without hand-rolling a per-
    /// implementor `const DEFAULT: T = T::Alpha` declaration (a
    /// `serde` deserializer wrapper that folds a missing field onto
    /// [`Self::first`], a `Default` impl generator that emits
    /// `impl Default for T { fn default() -> T { T::first() } }` at
    /// derive time), a truth-table property test that anchors at
    /// [`Self::first`] / [`Self::last`] as its declaration-order edges
    /// (a quickcheck-shaped variant generator that iterates from the
    /// head to the tail through `from_index` and pins the endpoints
    /// through this pair), a wire-format decoder that emits an
    /// out-of-band "reset to head" sentinel decoded through
    /// [`Self::first`], a state-machine iterator that walks the
    /// declaration-order chain from [`Self::first`] toward
    /// [`Self::last`] via [`Self::from_index`] — bind to ONE trait
    /// method instead of hand-rolling either the `Self::ALL[0]` slice
    /// indexing (which re-derives the same one-primitive projection
    /// at every callsite AND makes every downstream site depend on
    /// `Self::ALL`'s slice-index API) OR the
    /// `Self::from_index(0).unwrap()` composition (which pays an
    /// [`Option`]-typed dispatch the closed-set non-empty contract
    /// structurally forbids), and the closed-set endpoint-anchor
    /// surface evolves at ONE site rather than per-consumer.
    ///
    /// THEORY.md §III — the typescape; the (declaration-order head
    /// endpoint) projection becomes a TYPE projection on the trait
    /// rather than a per-consumer inline `Self::ALL[0]` composition
    /// at every downstream anchor site. The (head, tail) endpoint-
    /// direction axis partitions the closed-set endpoint-anchor
    /// surface exhaustively into TWO typed projections, each with a
    /// distinct load-bearing consumer surface — the head for
    /// canonical defaulters / iterator-start anchors, the tail for
    /// iterator-terminator / bounded-loop guards.
    /// THEORY.md §V.1 — knowable platform; the (declaration-order
    /// head endpoint) projection was an unnamed compound of
    /// [`Self::ALL`] + slice-index-0 pre-lift; naming it on the
    /// trait makes the projection a TYPED CONSEQUENCE of
    /// [`Self::ALL`] alone — generic consumers see ONE method, not
    /// ONE endpoint-shape-per-crate. The well-formedness clause (18)
    /// pins [`Self::first`] against `T::ALL[0]` on every implementor
    /// so a passing well-formedness sweep means every generic
    /// consumer can call [`Self::first`] on any typed variant without
    /// threading an [`Option`] through the return.
    /// THEORY.md §VI.1 — generation over composition; the
    /// (declaration-order head endpoint) projection emerges from the
    /// composition of ONE substrate primitive ([`Self::ALL`]) with
    /// the standard-library slice-index-0 projection rather than as
    /// a per-implementor `const HEAD: Self = ...` declaration. A
    /// future tightening of [`Self::ALL`] (a future
    /// `#[closed_set(cardinality = N)]` derive attribute that pins N
    /// at the source, a future declaration-time endpoint-assertion)
    /// propagates to every closed-set endpoint-anchor consumer
    /// through ONE trait method.
    ///
    /// Frontier inspiration: Racket's `enum-first` / `enum-last` on
    /// closed enumerations, Idris's `Fin n` finite-cardinality type's
    /// `firstFin : Fin (S n)` / `lastFin : Fin (S n)` panic-free
    /// endpoint constructors on the non-empty finite-type universe,
    /// MLIR's `RegisteredOperationName::begin() / end()` on the Op
    /// registry, Haskell's `bounded` type-class `minBound` /
    /// `maxBound` axis over closed enumerations — bounded-type
    /// endpoint anchors exposed as bare typed values rather than
    /// [`Option`]-wrapped decodes. Translation through pleme-io
    /// primitives: a pure default method composing the trait's
    /// existing [`Self::ALL`] surface with the standard-library
    /// slice-index-0 projection — no new dep, no new IR layer, no
    /// supertrait bound, no [`Option`]-typed dispatch.
    fn first() -> Self {
        Self::ALL[0]
    }

    /// The declaration-order last variant of the closed set —
    /// `Self::ALL[Self::ALL.len() - 1]` projected onto the trait
    /// surface as a panic-free typed anchor. Closes the
    /// (endpoint = `CARDINALITY - 1`) corner of the closed-set
    /// endpoint-anchor axis.
    ///
    /// Sibling posture to [`Self::first`] one axis over on the
    /// (endpoint = 0, endpoint = `CARDINALITY - 1`) partition of the
    /// closed-set endpoint surface: [`Self::first`] returns the
    /// declaration-order head, this method returns the
    /// declaration-order tail. See [`Self::first`] for the shared
    /// design rationale, sibling matrix, override axis, future-
    /// consumer inventory, THEORY.md grounding, and frontier
    /// inspiration — this method is the (endpoint = `CARDINALITY - 1`)
    /// arm of the same axis and inherits every property from the
    /// (endpoint = 0) arm's documentation, differing only in the
    /// concrete slice-index projection.
    ///
    /// Default body composes ONE substrate primitive ([`Self::ALL`])
    /// with the standard-library slice-index-`(N - 1)` projection —
    /// the tail anchor is a typed CONSEQUENCE of [`Self::ALL`] + the
    /// non-empty contract, not a per-implementor `const TAIL: Self =
    /// ...` declaration. Both the `Self::ALL.len() - 1` subtraction
    /// AND the subsequent indexing are guaranteed sound by the
    /// well-formedness contract [`assert_closed_set_well_formed`]'s
    /// clause (1) — `Self::ALL` is non-empty, so `Self::ALL.len()`
    /// is `>= 1`, and the subtraction never underflows. The
    /// well-formedness clause (18) pins [`Self::last`] against
    /// `T::ALL[T::ALL.len() - 1]` on every implementor so a passing
    /// well-formedness sweep means every generic consumer can call
    /// [`Self::last`] on any typed variant without threading an
    /// [`Option`] through the return.
    fn last() -> Self {
        Self::ALL[Self::ALL.len() - 1]
    }

    /// The declaration-order head-endpoint membership predicate —
    /// `true` when `self` is [`Self::first`], `false` otherwise.
    /// Closes the (endpoint-anchor `Self`-returning, endpoint-membership
    /// `bool`-returning) return-type axis over the (head, tail) partition
    /// of the declaration-axis endpoint surface.
    ///
    /// The (return-type × endpoint-direction) 2×2 matrix over the
    /// declaration-axis endpoint-anchor surface partitions post-lift:
    ///
    /// | Return type \\ Endpoint    | Head                | Tail               |
    /// |----------------------------|---------------------|--------------------|
    /// | `Self` (anchor)            | [`Self::first`]     | [`Self::last`]     |
    /// | `bool` (membership)        | [`Self::is_first`]  | [`Self::is_last`]  |
    ///
    /// Sibling posture to [`Self::first`] one return-type axis over —
    /// [`Self::first`] projects the head-endpoint variant, this method
    /// answers "am I at the head endpoint?" without threading the caller
    /// through a supertrait [`PartialEq`] bound the [`ClosedSet`] trait
    /// deliberately does not require. Every generic consumer that
    /// wants an O(1) head-boundary query (a bounded-loop guard that
    /// short-circuits before `Self::prev` returns [`None`], a saga-step
    /// engine that emits a "reset" event on the head-endpoint slot, a
    /// truth-table property test that anchors an edge assertion at the
    /// head-endpoint slot, a wraparound-cursor renderer that highlights
    /// the head anchor before wrapping) binds to ONE typed predicate
    /// rather than hand-rolling either the `self.index_of() == 0`
    /// composition (which re-derives the same one-primitive projection
    /// at every callsite) OR the `Self::PartialEq`-bounded
    /// `self == Self::first()` comparison (which the trait's minimal
    /// supertrait pair `Sized + Copy` structurally forbids).
    ///
    /// Default body composes ONE substrate primitive
    /// ([`Self::index_of`]) with an `usize` equality check against `0`
    /// — the head-membership predicate is a typed CONSEQUENCE of the
    /// (variant → declaration-order position) forward projection, not a
    /// per-implementor `match self { Self::Head => true, _ => false }`
    /// block. Implementors override only when the head-membership
    /// surface needs to diverge from the natural
    /// `index_of(self) == 0` shape (no production implementor reaches
    /// for this today; the axis exists for the same reason
    /// `via` / `set_label` / `labels` / `first` overrides exist — a
    /// typed escape hatch the trait surface exposes rather than
    /// forcing the implementor to hand-roll the impl). An implementor
    /// that overrides [`Self::index_of`] propagates the override
    /// through this default body automatically; the (variant → bool
    /// head-membership) projection funnels through ONE typed primitive.
    ///
    /// The head-membership contract — `T::first().is_first() == true`
    /// on every implementor — is guaranteed by the composition through
    /// [`Self::index_of`]'s `0`-slot projection clause (15) pins on
    /// every declaration-order head; the well-formedness clause (30)
    /// pins the composition against the natural
    /// `index_of(self) == 0` shape AND the head-endpoint `true` fixpoint
    /// on every implementor so a passing well-formedness sweep means
    /// every generic consumer can call [`Self::is_first`] on any typed
    /// variant and expect the same `bool` answer at every crate boundary.
    ///
    /// THEORY.md §III — the typescape; the (variant → head-membership
    /// bool) projection becomes a TYPE projection on the trait rather
    /// than a per-consumer inline `self.index_of() == 0` composition at
    /// every downstream head-boundary query site.
    /// THEORY.md §V.1 — knowable platform; the (variant → head-
    /// membership) projection was an unnamed compound of
    /// [`Self::index_of`] + `usize` `==` `0` pre-lift; naming it on the
    /// trait makes the projection a TYPED CONSEQUENCE of ONE substrate
    /// primitive — generic consumers see ONE method, not one
    /// head-boundary-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the (variant →
    /// head-membership) projection emerges from the composition of
    /// [`Self::index_of`] with the standard-library `usize` equality
    /// operator rather than as a per-implementor
    /// `match self { ... }` block. A future tightening of
    /// [`Self::index_of`] (a future perfect-hash forward projection, a
    /// future const-fn axis that makes the predicate callable in const
    /// contexts) propagates to every closed-set head-boundary consumer
    /// through this method's body.
    ///
    /// Frontier inspiration: Racket's `enum-first?` on closed
    /// enumerations (the head-endpoint membership predicate on the
    /// declaration-order chain); Idris's `Fin (S n)` finite-cardinality
    /// type's `isFZ : Fin (S n) -> Bool` predicate on the head slot of
    /// the non-empty finite-type universe; Haskell's `(== minBound)`
    /// on the `Bounded + Enum` type-class pair; MLIR's
    /// `RegisteredOperationName::isBegin()` on the declaration-order
    /// Op registry; Rust's `strum::EnumIter::next().map(|v| v == self)`
    /// composed through the iterator API. Translation through pleme-io
    /// primitives: a pure default method composing the trait's existing
    /// [`Self::index_of`] surface with the standard-library `usize`
    /// equality operator — no new dep, no new IR layer, no supertrait
    /// [`PartialEq`] bound, no [`Option`]-typed dispatch.
    fn is_first(self) -> bool {
        <Self as ClosedSet>::index_of(self) == 0
    }

    /// The declaration-order tail-endpoint membership predicate —
    /// `true` when `self` is [`Self::last`], `false` otherwise.
    /// Closes the (bool, tail) corner of the (return-type ×
    /// endpoint-direction) 2×2 declaration-axis endpoint matrix
    /// alongside [`Self::is_first`].
    ///
    /// Sibling posture to [`Self::is_first`] one axis over on the
    /// (head, tail) partition of the declaration-axis endpoint-
    /// membership surface: [`Self::is_first`] answers "am I at the
    /// head endpoint?", this method answers "am I at the tail
    /// endpoint?". See [`Self::is_first`] for the shared design
    /// rationale, sibling matrix, override axis, future-consumer
    /// inventory, THEORY.md grounding, and frontier inspiration —
    /// this method is the tail-direction arm of the same axis and
    /// inherits every property from the head arm's documentation,
    /// differing only in the `+ 1 == Self::CARDINALITY` boundary
    /// check.
    ///
    /// Default body composes [`Self::index_of`] with an `usize`
    /// equality check against [`Self::CARDINALITY`] under the natural
    /// `+ 1` shift — the tail-membership predicate is a typed
    /// CONSEQUENCE of the composition of the (variant → declaration-
    /// order position) forward projection with the const-visible
    /// variant count, not a per-implementor
    /// `match self { Self::Tail => true, _ => false }` block. The
    /// `+ 1 == Self::CARDINALITY` shape (rather than
    /// `== Self::CARDINALITY - 1`) avoids the `usize` underflow
    /// question on the well-formedness contract clause (1)'s
    /// non-empty guarantee already forbids — clause (1) pins
    /// `Self::CARDINALITY >= 1`, so `Self::CARDINALITY - 1` never
    /// underflows in practice, but the `+ 1 ==` form composes without
    /// ever performing the subtraction, keeping the projection callable
    /// in a future const-fn context without the underflow discharge.
    ///
    /// The tail-membership contract — `T::last().is_last() == true`
    /// on every implementor — is guaranteed by the composition
    /// through [`Self::index_of`]'s `Self::CARDINALITY - 1`-slot
    /// projection clause (15) pins on every declaration-order tail;
    /// the well-formedness clause (30) pins the composition against
    /// the natural `index_of(self) + 1 == Self::CARDINALITY` shape AND
    /// the tail-endpoint `true` fixpoint on every implementor so a
    /// passing well-formedness sweep means every generic consumer can
    /// call [`Self::is_last`] on any typed variant and expect the same
    /// `bool` answer at every crate boundary.
    /// `T::last().is_last() == true` is the natural fixpoint the tail-
    /// endpoint anchor and the tail-membership axis share, mirroring
    /// the `T::first().is_first() == true` fixpoint on the head-
    /// endpoint anchor / head-membership pair.
    fn is_last(self) -> bool {
        <Self as ClosedSet>::index_of(self) + 1 == <Self as ClosedSet>::CARDINALITY
    }

    /// The lexicographically-least variant of the closed set — the
    /// canonical minimum-by-[`Self::label`] under the standard-library
    /// `str: Ord` ordering, projected onto the trait surface as a
    /// panic-free typed anchor. Closes the (lexicographic-order, head)
    /// corner of the (ordering-axis × endpoint-direction) 2×2
    /// endpoint-anchor matrix — sibling posture to [`Self::first`]
    /// (declaration-order, head), [`Self::last`] (declaration-order,
    /// tail), and [`Self::sorted_last`] (lexicographic-order, tail).
    ///
    /// The (ordering-axis × endpoint-direction) 2×2 matrix over the
    /// closed-set endpoint-anchor surface partitions post-lift:
    ///
    /// | Ordering axis \\ Endpoint  | Head                    | Tail                   |
    /// |---------------------------|-------------------------|------------------------|
    /// | Declaration order         | [`Self::first`]         | [`Self::last`]         |
    /// | Lexicographic order       | [`Self::sorted_first`]  | [`Self::sorted_last`]  |
    ///
    /// Default body is a zero-alloc single-pass linear scan over
    /// [`Self::ALL`] keyed on [`Self::label`] — the head anchor never
    /// materializes the `Vec<Self>` [`Self::sorted_variants`] returns.
    /// Implementors override only when the endpoint surface needs to
    /// diverge from the natural label-keyed lex-min shape (no
    /// production implementor reaches for this today; the axis exists
    /// for the same reason `via` / `set_label` / `labels` /
    /// `sorted_labels` / `sorted_variants` / `first` overrides exist —
    /// a typed escape hatch rather than forcing the implementor to
    /// hand-roll the impl).
    ///
    /// The non-empty contract [`assert_closed_set_well_formed`]'s
    /// clause (1) guarantees `Self::ALL[0]` is sound; the label-pairwise-
    /// distinctness contract clause (3) guarantees the lex-min is unique
    /// (a strict `<` in the linear scan cannot reject a canonical
    /// minimum in favor of a later equal-label variant, because no two
    /// canonical labels can be equal). The well-formedness clause (19)
    /// pins [`Self::sorted_first`] against
    /// `T::sorted_variants()[0]` on every implementor so a passing
    /// well-formedness sweep means every generic consumer can call
    /// [`Self::sorted_first`] on any typed variant without threading an
    /// [`Option`] through the return.
    ///
    /// Future consumers — a diagnostic renderer that anchors an
    /// `"expected one of A..Z"` shape at the (lex-min, lex-max)
    /// endpoints without materializing the full sorted-labels list, an
    /// LSP completion default that highlights the alphabetically-first
    /// choice as the pre-selected candidate, a serde deserializer
    /// wrapper that folds a missing field onto the lex-least canonical
    /// variant (rather than the declaration-order head [`Self::first`]
    /// projects, when the closed set's canonical default is defined by
    /// alphabetic order rather than declaration order), a property-test
    /// generator that anchors at the (lex-min, lex-max) edges — bind to
    /// ONE trait method instead of hand-rolling either the
    /// `Self::sorted_variants()[0]` composition (which pays a Vec
    /// allocation the linear scan doesn't need) OR the
    /// `Self::labels().into_iter().min().and_then(Self::find_by_label)`
    /// composition (which pays a Vec-of-labels allocation AND an
    /// [`Option`]-typed dispatch the closed-set non-empty + distinct-
    /// labels contract structurally forbids).
    ///
    /// THEORY.md §III — the typescape; the (lexicographic-order head
    /// endpoint) projection becomes a TYPE projection on the trait
    /// rather than a per-consumer composition of [`Self::sorted_variants`]
    /// combined with `<[Self]>::first` at every downstream anchor site.
    /// The (declaration, lex) × (head, tail) endpoint-anchor 2×2 matrix
    /// partitions the closed-set endpoint-anchor surface exhaustively
    /// into FOUR typed projections, each with a distinct load-bearing
    /// consumer surface.
    ///
    /// THEORY.md §V.1 — knowable platform; the (lexicographic-order
    /// head endpoint) projection was an unnamed compound of
    /// [`Self::ALL`] combined with a label-keyed sort combined with a
    /// slice-index-0 projection pre-lift; naming it on the trait makes
    /// the projection a TYPED CONSEQUENCE of [`Self::ALL`] combined
    /// with [`Self::label`] alone — generic consumers see ONE method,
    /// not ONE lex-endpoint-shape-per-crate. Clause (19) pins it
    /// against [`Self::sorted_variants`]'s head endpoint so the
    /// label-keyed linear scan and the sorted-listing surface stay
    /// aligned at ONE anchor site.
    ///
    /// THEORY.md §VI.1 — generation over composition; the (lex head
    /// endpoint) projection emerges from the composition of TWO
    /// substrate primitives ([`Self::ALL`] combined with
    /// [`Self::label`]) via the standard-library `PartialOrd` on `str`
    /// rather than as a per-implementor `const LEX_HEAD: Self = ...`
    /// literal. A future tightening of the label comparator (a future
    /// `#[closed_set(compare_labels_with = ...)]` derive attribute that
    /// swaps the ordering, a future case-insensitive-label extension)
    /// propagates to every closed-set lex-endpoint consumer through
    /// this method's body.
    ///
    /// Frontier inspiration: Haskell's `Data.List.minimumBy` on a
    /// closed-set candidate list keyed by a projection function; Idris's
    /// `Data.List.min` over the `Fin n` finite-cardinality universe
    /// composed with a labeling projection; MLIR's
    /// `RegisteredOperationName::begin()` on a lexicographically-sorted
    /// Op registry; Racket's `(argmin T-label (enum->list T))` on a
    /// closed-enum candidate list. Translation through pleme-io
    /// primitives: a pure default method composing the trait's existing
    /// [`Self::ALL`] + [`Self::label`] surfaces with a strict-`<` linear
    /// scan — no new dep, no new IR layer, no supertrait bound, no
    /// [`Option`]-typed dispatch, no Vec allocation.
    fn sorted_first() -> Self {
        let mut best = Self::ALL[0];
        let mut best_label = <Self as ClosedSet>::label(best);
        for &v in &Self::ALL[1..] {
            let lbl = <Self as ClosedSet>::label(v);
            if lbl < best_label {
                best = v;
                best_label = lbl;
            }
        }
        best
    }

    /// The lexicographically-greatest variant of the closed set — the
    /// canonical maximum-by-[`Self::label`] under the standard-library
    /// `str: Ord` ordering, projected onto the trait surface as a
    /// panic-free typed anchor. Closes the (lexicographic-order, tail)
    /// corner of the (ordering-axis × endpoint-direction) 2×2
    /// endpoint-anchor matrix.
    ///
    /// Sibling posture to [`Self::sorted_first`] one axis over on the
    /// (head, tail) partition of the lexicographic-order endpoint-anchor
    /// surface: [`Self::sorted_first`] returns the lex-min,
    /// this method returns the lex-max. See [`Self::sorted_first`] for
    /// the shared design rationale, sibling matrix, override axis,
    /// future-consumer inventory, THEORY.md grounding, and frontier
    /// inspiration — this method is the tail arm of the same axis and
    /// inherits every property from the head arm's documentation,
    /// differing only in the strict-`>` comparator direction.
    ///
    /// Default body is a zero-alloc single-pass linear scan over
    /// [`Self::ALL`] keyed on [`Self::label`] with the comparator
    /// inverted — the tail anchor never materializes the `Vec<Self>`
    /// [`Self::sorted_variants`] returns. The well-formedness clause
    /// (19) pins [`Self::sorted_last`] against
    /// `T::sorted_variants()[T::sorted_variants().len() - 1]` on every
    /// implementor so a passing well-formedness sweep means every
    /// generic consumer can call [`Self::sorted_last`] on any typed
    /// variant without threading an [`Option`] through the return.
    fn sorted_last() -> Self {
        let mut best = Self::ALL[0];
        let mut best_label = <Self as ClosedSet>::label(best);
        for &v in &Self::ALL[1..] {
            let lbl = <Self as ClosedSet>::label(v);
            if lbl > best_label {
                best = v;
                best_label = lbl;
            }
        }
        best
    }

    /// The lexicographic-order head-endpoint membership predicate —
    /// `true` when `self` is [`Self::sorted_first`], `false` otherwise.
    /// Closes the (lex, head) corner of the (ordering-axis × endpoint-
    /// direction) 2×2 endpoint-membership matrix alongside
    /// [`Self::is_first`] (declaration, head), [`Self::is_last`]
    /// (declaration, tail), and [`Self::is_sorted_last`] (lex, tail).
    ///
    /// The (ordering-axis × endpoint-direction) 2×2 endpoint-membership
    /// matrix over the closed-set `bool`-typed endpoint surface
    /// partitions post-lift:
    ///
    /// | Ordering axis \\ Endpoint  | Head                     | Tail                    |
    /// |---------------------------|--------------------------|-------------------------|
    /// | Declaration order         | [`Self::is_first`]       | [`Self::is_last`]       |
    /// | Lexicographic order       | [`Self::is_sorted_first`]| [`Self::is_sorted_last`]|
    ///
    /// Combined with the (ordering × direction) 2×2 endpoint-ANCHOR
    /// matrix ([`Self::first`], [`Self::last`], [`Self::sorted_first`],
    /// [`Self::sorted_last`]), the two matrices together close the
    /// (return-type × ordering × direction) 2×2×2 = 8-corner endpoint
    /// cube — every generic consumer that wants a typed answer at an
    /// endpoint slot binds to ONE of the eight methods rather than
    /// hand-rolling either the `Self`-anchor comparison
    /// (`self == T::sorted_first()`, needs a supertrait [`PartialEq`]
    /// bound the trait's minimal `Sized + Copy` supertrait pair
    /// structurally forbids) OR the two-primitive [`Self::sorted_index_of`]
    /// composition (`self.sorted_index_of() == 0`, re-derives the
    /// natural `usize`-equality composition at every callsite).
    ///
    /// Default body composes [`Self::sorted_index_of`] with a `usize`
    /// equality check against `0` — the head-membership predicate is
    /// a typed CONSEQUENCE of the composition of the (variant → lex-
    /// order position) forward projection with the const-visible `0`
    /// slot, not a per-implementor
    /// `match self { Self::LexHead => true, _ => false }` block. The
    /// lex head-membership contract — `T::sorted_first().is_sorted_first()
    /// == true` on every implementor — is guaranteed by the composition
    /// through [`Self::sorted_index_of`]'s `0`-slot projection clause
    /// (22) pins on every canonical variant; the well-formedness
    /// clause (31) pins the composition against the natural
    /// `sorted_index_of(self) == 0` shape AND the lex head-endpoint
    /// `true` fixpoint on every implementor so a passing well-
    /// formedness sweep means every generic consumer can call
    /// [`Self::is_sorted_first`] on any typed variant and expect the
    /// same `bool` answer at every crate boundary.
    ///
    /// Future consumers — an alphabetized LSP completion cursor that
    /// highlights the alphabetically-first choice differently (a
    /// candidate about to be pre-selected on Enter, a keyboard-cursor
    /// wrap-around handler that resets to the lex-min anchor); a
    /// `tatara-check` diagnostic renderer that anchors the
    /// `"expected one of: A..Z"` shape at the lex-min without
    /// materializing the [`Self::sorted_labels`] list; a metrics
    /// tagger that fires a distinguished counter on the lex-min slot;
    /// a serde deserializer wrapper that folds a missing field onto
    /// the lex-least canonical variant (the natural default-alignment
    /// when the closed set's canonical ordering is alphabetic rather
    /// than declaration-based) — bind to ONE typed predicate rather
    /// than hand-rolling either the `self == T::sorted_first()`
    /// comparison (which the trait's minimal supertrait pair
    /// `Sized + Copy` structurally forbids without adding a
    /// [`PartialEq`] bound) OR the two-primitive
    /// `self.sorted_index_of() == 0` composition (which re-derives
    /// the same lex-slot projection at every callsite).
    ///
    /// THEORY.md §III — the typescape; the (variant → lex head-
    /// membership bool) projection becomes a TYPE projection on the
    /// trait rather than a per-consumer inline
    /// `self.sorted_index_of() == 0` composition at every downstream
    /// lex-head-boundary query site.
    /// THEORY.md §V.1 — knowable platform; the (variant → lex head-
    /// membership) projection was an unnamed compound of
    /// [`Self::sorted_index_of`] + `usize` `==` `0` pre-lift; naming
    /// it on the trait makes the projection a TYPED CONSEQUENCE of
    /// ONE substrate primitive — generic consumers see ONE method,
    /// not one lex-head-boundary-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the (variant →
    /// lex head-membership) projection emerges from the composition
    /// of [`Self::sorted_index_of`] with the standard-library `usize`
    /// equality operator rather than as a per-implementor
    /// `match self { ... }` block. A future tightening of
    /// [`Self::sorted_index_of`] (a future perfect-hash lex-order
    /// projection, a future const-fn lex-axis, a future case-
    /// insensitive-label extension that shifts which variant lands
    /// at the lex-min slot) propagates to every closed-set lex-head-
    /// boundary consumer through this method's body.
    ///
    /// Frontier inspiration: Racket's `enum-sorted-first?` on closed
    /// enumerations under lex-ordering; Idris's `min : Fin (S n) ->
    /// Fin (S n) -> Bool` composed with a `sortBy comparingLabel`
    /// prelude on the finite-cardinality universe (folded onto the
    /// head slot of the lex-sorted chain); Haskell's `(== minimumBy
    /// comparing label [minBound..])` on the `Bounded + Enum` type-
    /// class pair; MLIR's `RegisteredOperationName::isLexBegin()` on
    /// the lex-sorted Op registry. Translation through pleme-io
    /// primitives: a pure default method composing the trait's
    /// existing [`Self::sorted_index_of`] surface with the standard-
    /// library `usize` equality operator — no new dep, no new IR
    /// layer, no supertrait [`PartialEq`] bound, no [`Option`]-typed
    /// dispatch.
    fn is_sorted_first(self) -> bool {
        <Self as ClosedSet>::sorted_index_of(self) == 0
    }

    /// The lexicographic-order tail-endpoint membership predicate —
    /// `true` when `self` is [`Self::sorted_last`], `false` otherwise.
    /// Closes the (lex, tail) corner of the (ordering-axis × endpoint-
    /// direction) 2×2 endpoint-membership matrix, completing the
    /// (return-type × ordering × direction) 2×2×2 = 8-corner endpoint
    /// cube alongside [`Self::is_first`], [`Self::is_last`], and
    /// [`Self::is_sorted_first`].
    ///
    /// Sibling posture to [`Self::is_sorted_first`] one axis over on
    /// the (head, tail) partition of the lex-axis endpoint-membership
    /// surface: [`Self::is_sorted_first`] answers "am I at the lex
    /// head endpoint?", this method answers "am I at the lex tail
    /// endpoint?". See [`Self::is_sorted_first`] for the shared design
    /// rationale, sibling matrix, override axis, future-consumer
    /// inventory, THEORY.md grounding, and frontier inspiration —
    /// this method is the tail-direction arm of the same axis and
    /// inherits every property from the head arm's documentation,
    /// differing only in the `+ 1 == Self::CARDINALITY` boundary
    /// check.
    ///
    /// Default body composes [`Self::sorted_index_of`] with an `usize`
    /// equality check against [`Self::CARDINALITY`] under the natural
    /// `+ 1` shift — the lex tail-membership predicate is a typed
    /// CONSEQUENCE of the composition of the (variant → lex-order
    /// position) forward projection with the const-visible variant
    /// count, not a per-implementor
    /// `match self { Self::LexTail => true, _ => false }` block. The
    /// `+ 1 == Self::CARDINALITY` shape (rather than
    /// `== Self::CARDINALITY - 1`) avoids the `usize` underflow
    /// question the well-formedness contract clause (1)'s non-empty
    /// guarantee already forbids — mirroring [`Self::is_last`]'s
    /// declaration-axis shape one ordering axis over so the projection
    /// stays callable in a future const-fn context without the
    /// underflow discharge.
    ///
    /// The lex tail-membership contract —
    /// `T::sorted_last().is_sorted_last() == true` on every
    /// implementor — is guaranteed by the composition through
    /// [`Self::sorted_index_of`]'s `Self::CARDINALITY - 1`-slot
    /// projection clause (22) pins on every lex-order tail; the
    /// well-formedness clause (31) pins the composition against the
    /// natural `sorted_index_of(self) + 1 == Self::CARDINALITY` shape
    /// AND the lex tail-endpoint `true` fixpoint on every implementor
    /// so a passing well-formedness sweep means every generic consumer
    /// can call [`Self::is_sorted_last`] on any typed variant and
    /// expect the same `bool` answer at every crate boundary.
    /// `T::sorted_last().is_sorted_last() == true` is the natural
    /// fixpoint the lex tail-endpoint anchor and the lex tail-
    /// membership axis share, mirroring
    /// `T::last().is_last() == true` (declaration-axis, tail) and
    /// `T::sorted_first().is_sorted_first() == true` (lex-axis, head).
    fn is_sorted_last(self) -> bool {
        <Self as ClosedSet>::sorted_index_of(self) + 1 == <Self as ClosedSet>::CARDINALITY
    }

    /// The declaration-order endpoint-membership predicate — `true`
    /// when `self` is either [`Self::first`] or [`Self::last`],
    /// `false` on every strictly-interior slot. The endpoint-partition
    /// arm of the (endpoint, interior) boolean-partition axis over the
    /// declaration-axis endpoint surface — one predicate-flavor axis
    /// over from the point-membership pair
    /// ([`Self::is_first`], [`Self::is_last`]) and the natural
    /// complement of [`Self::is_interior`].
    ///
    /// Opens the (predicate-flavor × ordering) 2×2 matrix over the
    /// declaration-axis boolean-boundary surface — the endpoint-cube
    /// closure of clauses (30) + (31) named the point-membership arm
    /// per direction; this method + [`Self::is_interior`] name the
    /// **compound partition arm** the point-membership pair induces
    /// under `∨` and its negation:
    ///
    /// | Predicate flavor \\ Ordering    | Declaration            | Lex                         |
    /// |---------------------------------|------------------------|-----------------------------|
    /// | Point (head / tail)             | [`Self::is_first`] / [`Self::is_last`] | [`Self::is_sorted_first`] / [`Self::is_sorted_last`] |
    /// | Boundary (endpoint / interior)  | [`Self::is_endpoint`] / [`Self::is_interior`] | [`Self::is_sorted_endpoint`] / [`Self::is_sorted_interior`] |
    ///
    /// Every generic consumer that partitions the closed set into
    /// (structural-boundary, strict-interior) slots without threading
    /// the caller through a per-endpoint `is_first || is_last`
    /// disjunction (a bounded-iteration guard that emits a
    /// `first-or-last-slot` sentinel event on either terminus, a
    /// wraparound-cursor renderer that renders a shared boundary badge
    /// on both endpoints without duplicating the badge-emit fork,
    /// a truth-table property test that anchors a shared
    /// endpoint-parity assertion across both endpoint anchors, a
    /// saga-step engine that opens a "structural-boundary" audit
    /// event on either the head OR the tail, a per-tick UI carousel
    /// that renders a persistent "at-boundary" glyph on both ends of
    /// the chain, a phase-fold reducer whose interior arm short-
    /// circuits ONLY when the current slot is strictly-interior) binds
    /// to ONE typed compound predicate rather than hand-rolling either
    /// the `self.is_first() || self.is_last()` disjunction (which
    /// re-derives the same two-primitive composition at every callsite
    /// AND makes every downstream site depend on the disjunction
    /// shape) OR the `self.index_of() == 0 || self.index_of() + 1 ==
    /// T::CARDINALITY` composition (which re-derives the same three-
    /// primitive composition at every callsite AND makes every
    /// downstream site depend on the `usize` boundary arithmetic) OR
    /// a per-implementor inline `matches!(self, Self::Head | Self::
    /// Tail)` block (which re-derives the per-variant endpoint table
    /// at every callsite AND drifts silently when [`Self::ALL`] gains
    /// a new variant that reorders the head/tail slots).
    ///
    /// Default body composes [`Self::is_first`] with [`Self::is_last`]
    /// under `||` — the boundary-membership predicate is a typed
    /// CONSEQUENCE of the two pre-existing point-membership primitives
    /// on the declaration axis, not a third codepath through
    /// [`Self::index_of`] arithmetic. Implementors override only when
    /// the boundary-membership surface needs to diverge from the
    /// natural `is_first(self) || is_last(self)` shape (no production
    /// implementor reaches for this today; the axis exists for the
    /// same reason `via` / `set_label` / `labels` / `first` / `last` /
    /// `is_first` / `is_last` overrides exist — a typed escape hatch
    /// the trait surface exposes rather than forcing the implementor
    /// to hand-roll the impl). An implementor that overrides either
    /// [`Self::is_first`] OR [`Self::is_last`] propagates the override
    /// through this default body automatically; the (variant → bool
    /// boundary-membership) projection funnels through the SAME pair of
    /// point-membership primitives the endpoint cube already routes.
    ///
    /// Singleton degeneracy — for a closed set with
    /// `T::CARDINALITY == 1`, [`Self::first`] equals [`Self::last`],
    /// so both point-membership predicates fire on the same variant
    /// and this predicate returns `true` for the sole variant.
    /// [`Self::is_interior`] correspondingly returns `false` — a
    /// singleton has zero interior slots. Mirrors the singleton
    /// collapse the point-membership axis observes at [`Self::first`]
    /// / [`Self::last`] and preserves the (endpoint XOR interior)
    /// partition semantics even at the boundary-cardinality edge.
    ///
    /// The boundary-membership contract —
    /// `T::first().is_endpoint() == true` AND
    /// `T::last().is_endpoint() == true` on every implementor — is
    /// guaranteed by the composition through
    /// [`Self::is_first`] / [`Self::is_last`]'s endpoint-fixpoint
    /// clauses (30); the well-formedness clause (32) pins the
    /// composition against the natural
    /// `is_first(self) || is_last(self)` shape AND the boundary-
    /// endpoint `true` fixpoints on every implementor, so a passing
    /// well-formedness sweep means every generic consumer can call
    /// [`Self::is_endpoint`] on any typed variant and expect the same
    /// `bool` answer at every crate boundary. The (endpoint,
    /// interior) partition is EXHAUSTIVE — every variant in
    /// [`Self::ALL`] answers `true` to EXACTLY ONE of the two
    /// predicates, pinned by clause (32)'s complementarity assertion
    /// `is_endpoint(v) != is_interior(v)` on every representative
    /// input.
    ///
    /// THEORY.md §III — the typescape; the (variant → boundary-
    /// membership bool) projection becomes a TYPE projection on the
    /// trait rather than a per-consumer inline
    /// `self.is_first() || self.is_last()` composition at every
    /// downstream structural-boundary query site. The (predicate-
    /// flavor × ordering) 2×2 matrix over the declaration-axis
    /// boolean-boundary surface opens a NEW axis alongside the
    /// (return-type × direction) 2×2 endpoint-anchor / endpoint-
    /// membership matrix clauses (18) + (30) already close on the
    /// declaration axis.
    /// THEORY.md §V.1 — knowable platform; the (variant → boundary-
    /// membership) projection was an unnamed compound of
    /// [`Self::is_first`] + [`Self::is_last`] + `||` pre-lift;
    /// naming it on the trait makes the projection a TYPED CONSEQUENCE
    /// of the two point-membership primitives — generic consumers see
    /// ONE method, not one boundary-disjunction-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the (variant →
    /// boundary-membership) projection emerges from the composition of
    /// TWO substrate primitives ([`Self::is_first`], [`Self::is_last`])
    /// under the standard-library boolean `||` operator rather than as
    /// a per-implementor `match self { ... }` block. A future
    /// tightening of either primitive (a future const-fn axis that
    /// makes the predicate callable in const contexts, a future
    /// `#[closed_set(head = "…", tail = "…")]` derive attribute that
    /// swaps the endpoint anchors) propagates to every closed-set
    /// boundary-membership consumer through this method's body.
    ///
    /// Frontier inspiration: Racket's `enum-boundary?` on closed
    /// enumerations (the `∨`-composed endpoint membership predicate
    /// over both anchors of the declaration-order chain); Idris's
    /// `Fin (S n)` non-empty finite-cardinality types where the
    /// (head, tail) endpoint partition folds through a shared
    /// `isBoundary : Fin (S n) -> Bool` projection under boolean-or;
    /// Haskell's `(\x -> x == minBound || x == maxBound)` on the
    /// `Bounded + Enum` type-class pair; MLIR's
    /// `RegisteredOperationName::isEndpoint()` on the declaration-
    /// order Op registry; Idris's `Fin n` finite-cardinality type with
    /// a boundary predicate composing the head + tail endpoint
    /// checks; Racket's `(or (enum-first? e v) (enum-last? e v))`
    /// composed at the callsite the substrate would rather bind at
    /// the trait. Translation through pleme-io primitives: a pure
    /// default method composing the trait's existing
    /// [`Self::is_first`] and [`Self::is_last`] point-membership
    /// primitives under the standard-library boolean `||` operator —
    /// no new dep, no new IR layer, no supertrait bound, no `usize`
    /// arithmetic discharge.
    fn is_endpoint(self) -> bool {
        <Self as ClosedSet>::is_first(self) || <Self as ClosedSet>::is_last(self)
    }

    /// The declaration-order interior-membership predicate — `true`
    /// when `self` is neither [`Self::first`] nor [`Self::last`],
    /// `false` on both endpoints. The natural complement of
    /// [`Self::is_endpoint`] on the (endpoint, interior) boolean-
    /// partition axis over the declaration-axis endpoint surface.
    ///
    /// Sibling posture to [`Self::is_endpoint`] one arm over on the
    /// (endpoint, interior) partition — [`Self::is_endpoint`] fires
    /// on both structural anchors, this method fires on every
    /// strictly-interior slot. See [`Self::is_endpoint`] for the
    /// shared design rationale, sibling matrix, override axis,
    /// future-consumer inventory, THEORY.md grounding, and frontier
    /// inspiration — this method is the complement-direction arm of
    /// the same predicate-flavor axis and inherits every property
    /// from the endpoint arm's documentation, differing only in the
    /// leading `!` negation and the strictly-interior consumer
    /// surface (a bounded-loop that walks ONLY interior slots and
    /// short-circuits on either endpoint, a phase-fold reducer whose
    /// interior arm processes NON-boundary payloads and reserves the
    /// endpoint arm for boundary-only side effects, an
    /// alphabetized-completion pass that hides the first + last
    /// entries from a strictly-interior candidate list).
    ///
    /// Default body composes [`Self::is_endpoint`] with the standard-
    /// library `!` operator — the interior-membership predicate is a
    /// typed CONSEQUENCE of the boundary-membership disjunction, not
    /// a third codepath through `is_first ∧ is_last` inversion.
    /// Implementors override only when the interior-membership
    /// surface needs to diverge from the natural
    /// `!is_endpoint(self)` shape (no production implementor reaches
    /// for this today; the axis exists for the same reason
    /// `is_endpoint` / `is_first` / `is_last` overrides exist — a
    /// typed escape hatch rather than forcing the implementor to
    /// hand-roll the impl). An implementor that overrides
    /// [`Self::is_endpoint`] propagates the override through this
    /// default body automatically; the (variant → bool interior-
    /// membership) projection funnels through ONE typed primitive.
    ///
    /// Singleton degeneracy — for a closed set with
    /// `T::CARDINALITY == 1`, [`Self::is_endpoint`] fires on the sole
    /// variant, so this method returns `false` for that variant. A
    /// singleton closed set has ZERO interior slots by construction —
    /// [`Self::ALL`] is `[Self::Only]` and the sole element is BOTH
    /// endpoints simultaneously, leaving no room for a strictly-
    /// interior slot. Mirrors the singleton collapse
    /// [`Self::is_endpoint`] observes and preserves the (endpoint XOR
    /// interior) partition semantics even at the boundary-cardinality
    /// edge.
    ///
    /// The (endpoint, interior) partition contract —
    /// `is_endpoint(v) != is_interior(v)` on every variant `v` in
    /// [`Self::ALL`] — is guaranteed by the default composition
    /// through [`Self::is_endpoint`]'s `!` negation; the well-
    /// formedness clause (32) pins the complementarity assertion on
    /// every implementor so a passing well-formedness sweep means
    /// every generic consumer can call [`Self::is_interior`] on any
    /// typed variant and expect the exact complement of
    /// [`Self::is_endpoint`] on the same variant. The endpoint-fix-
    /// point corollary `T::first().is_interior() == false` AND
    /// `T::last().is_interior() == false` is guaranteed by clause
    /// (32)'s endpoint-anchor pin composed with the complementarity
    /// assertion.
    fn is_interior(self) -> bool {
        !<Self as ClosedSet>::is_endpoint(self)
    }

    /// The lexicographic-order endpoint-membership predicate — `true`
    /// when `self` is either [`Self::sorted_first`] or
    /// [`Self::sorted_last`], `false` on every strictly-interior lex
    /// slot. The endpoint-partition arm of the (endpoint, interior)
    /// boolean-partition axis over the LEX-axis endpoint surface —
    /// one predicate-flavor axis over from the lex point-membership
    /// pair ([`Self::is_sorted_first`], [`Self::is_sorted_last`]) and
    /// the natural complement of [`Self::is_sorted_interior`].
    ///
    /// Closes the (predicate-flavor × ordering) 2×2 matrix over the
    /// boolean-boundary surface — the declaration-axis arm
    /// ([`Self::is_endpoint`] / [`Self::is_interior`]) is one ordering
    /// axis over; this method + [`Self::is_sorted_interior`] name the
    /// **compound partition arm** the lex point-membership pair induces
    /// under `∨` and its negation:
    ///
    /// | Predicate flavor \\ Ordering    | Declaration            | Lex                         |
    /// |---------------------------------|------------------------|-----------------------------|
    /// | Point (head / tail)             | [`Self::is_first`] / [`Self::is_last`] | [`Self::is_sorted_first`] / [`Self::is_sorted_last`] |
    /// | Boundary (endpoint / interior)  | [`Self::is_endpoint`] / [`Self::is_interior`] | [`Self::is_sorted_endpoint`] / [`Self::is_sorted_interior`] |
    ///
    /// Every generic consumer that partitions the closed set into
    /// (lex-structural-boundary, lex-strict-interior) slots without
    /// threading the caller through a per-endpoint
    /// `is_sorted_first || is_sorted_last` disjunction (a bounded
    /// alphabetized-iteration guard that emits a `lex-first-or-last-
    /// slot` sentinel event on either lex terminus, an alphabetized-
    /// carousel renderer that draws a shared boundary badge on both
    /// lex endpoints without duplicating the badge-emit fork, a
    /// truth-table property test that anchors a shared lex-endpoint-
    /// parity assertion across both lex-endpoint anchors, a saga-step
    /// engine that opens a "lex-structural-boundary" audit event on
    /// either the lex head OR the lex tail, an alphabetized-completion
    /// UI that renders a persistent "at-lex-boundary" glyph on both
    /// ends of the alphabetized chain, a lex-phase-fold reducer whose
    /// interior arm short-circuits ONLY when the current slot is
    /// strictly-lex-interior) binds to ONE typed compound predicate
    /// rather than hand-rolling either the
    /// `self.is_sorted_first() || self.is_sorted_last()` disjunction
    /// (which re-derives the same two-primitive composition at every
    /// callsite AND makes every downstream site depend on the
    /// disjunction shape) OR the
    /// `self.sorted_index_of() == 0
    ///  || self.sorted_index_of() + 1 == T::CARDINALITY`
    /// composition (which re-derives the same three-primitive
    /// composition at every callsite AND makes every downstream site
    /// depend on the `usize` boundary arithmetic on the lex axis) OR
    /// a per-implementor inline
    /// `matches!(self, Self::LexHead | Self::LexTail)` block (which
    /// re-derives the per-variant lex-endpoint table at every callsite
    /// AND drifts silently when [`Self::label`] gains a new variant
    /// that reorders the lex-head / lex-tail slots).
    ///
    /// Default body composes [`Self::is_sorted_first`] with
    /// [`Self::is_sorted_last`] under `||` — the lex boundary-
    /// membership predicate is a typed CONSEQUENCE of the two pre-
    /// existing lex point-membership primitives, not a third codepath
    /// through [`Self::sorted_index_of`] arithmetic. Implementors
    /// override only when the lex boundary-membership surface needs to
    /// diverge from the natural
    /// `is_sorted_first(self) || is_sorted_last(self)` shape (no
    /// production implementor reaches for this today; the axis exists
    /// for the same reason `is_sorted_first` / `is_sorted_last` /
    /// `is_endpoint` / `is_interior` overrides exist — a typed escape
    /// hatch the trait surface exposes rather than forcing the
    /// implementor to hand-roll the impl). An implementor that
    /// overrides either [`Self::is_sorted_first`] OR
    /// [`Self::is_sorted_last`] propagates the override through this
    /// default body automatically; the (variant → bool lex-boundary-
    /// membership) projection funnels through the SAME pair of lex
    /// point-membership primitives the endpoint cube already routes.
    ///
    /// Singleton degeneracy — for a closed set with
    /// `T::CARDINALITY == 1`, [`Self::sorted_first`] equals
    /// [`Self::sorted_last`], so both lex point-membership predicates
    /// fire on the same variant and this predicate returns `true` for
    /// the sole variant. [`Self::is_sorted_interior`] correspondingly
    /// returns `false` — a singleton has zero lex-interior slots.
    /// Mirrors the singleton collapse [`Self::is_endpoint`] observes
    /// one ordering axis over and preserves the (lex-endpoint XOR
    /// lex-interior) partition semantics even at the boundary-
    /// cardinality edge.
    ///
    /// The lex boundary-membership contract —
    /// `T::sorted_first().is_sorted_endpoint() == true` AND
    /// `T::sorted_last().is_sorted_endpoint() == true` on every
    /// implementor — is guaranteed by the composition through
    /// [`Self::is_sorted_first`] / [`Self::is_sorted_last`]'s lex-
    /// endpoint-fixpoint clause (31); the well-formedness clause (33)
    /// pins the composition against the natural
    /// `is_sorted_first(self) || is_sorted_last(self)` shape AND the
    /// lex-boundary-endpoint `true` fixpoints on every implementor, so
    /// a passing well-formedness sweep means every generic consumer
    /// can call [`Self::is_sorted_endpoint`] on any typed variant and
    /// expect the same `bool` answer at every crate boundary. The
    /// (lex-endpoint, lex-interior) partition is EXHAUSTIVE — every
    /// variant in [`Self::ALL`] answers `true` to EXACTLY ONE of the
    /// two predicates, pinned by clause (33)'s complementarity
    /// assertion
    /// `is_sorted_endpoint(v) != is_sorted_interior(v)` on every
    /// representative input.
    ///
    /// THEORY.md §III — the typescape; the (variant → lex-boundary-
    /// membership bool) projection becomes a TYPE projection on the
    /// trait rather than a per-consumer inline
    /// `self.is_sorted_first() || self.is_sorted_last()` composition
    /// at every downstream lex-structural-boundary query site. The
    /// (predicate-flavor × ordering) 2×2 matrix over the boolean-
    /// boundary surface CLOSES on the lex axis — the sibling
    /// declaration-axis matrix opened with clause (32) is now paired
    /// under a shared partition-flavor axis.
    /// THEORY.md §V.1 — knowable platform; the (variant → lex-
    /// boundary-membership) projection was an unnamed compound of
    /// [`Self::is_sorted_first`] + [`Self::is_sorted_last`] + `||`
    /// pre-lift; naming it on the trait makes the projection a TYPED
    /// CONSEQUENCE of the two lex point-membership primitives —
    /// generic consumers see ONE method, not one lex-boundary-
    /// disjunction-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the (variant →
    /// lex-boundary-membership) projection emerges from the
    /// composition of TWO substrate primitives
    /// ([`Self::is_sorted_first`], [`Self::is_sorted_last`]) under
    /// the standard-library boolean `||` operator rather than as a
    /// per-implementor `match self { ... }` block. A future tightening
    /// of either primitive (a future const-fn lex axis, a future
    /// case-insensitive-label extension that shifts which variant
    /// lands at the lex-min slot, a future perfect-hash lex-order
    /// projection) propagates to every closed-set lex-boundary-
    /// membership consumer through this method's body.
    ///
    /// Frontier inspiration: Racket's `enum-sorted-boundary?` on
    /// closed enumerations under lex-ordering (the `∨`-composed lex-
    /// endpoint-membership predicate over both anchors of the
    /// alphabetized chain); Idris's `Fin (S n)` non-empty finite-
    /// cardinality types where the (lex head, lex tail) endpoint
    /// partition folds through a shared
    /// `isSortedBoundary : Fin (S n) -> Bool` projection under
    /// boolean-or; Haskell's `(\x -> x == minimumBy comparing label
    /// [minBound..] || x == maximumBy comparing label [minBound..])`
    /// on the `Bounded + Enum` type-class pair with a `sortBy label`
    /// prelude; MLIR's `RegisteredOperationName::isLexEndpoint()` on
    /// the lex-sorted Op registry. Translation through pleme-io
    /// primitives: a pure default method composing the trait's
    /// existing [`Self::is_sorted_first`] and [`Self::is_sorted_last`]
    /// lex point-membership primitives under the standard-library
    /// boolean `||` operator — no new dep, no new IR layer, no
    /// supertrait bound, no `usize` arithmetic discharge.
    fn is_sorted_endpoint(self) -> bool {
        <Self as ClosedSet>::is_sorted_first(self) || <Self as ClosedSet>::is_sorted_last(self)
    }

    /// The lexicographic-order interior-membership predicate — `true`
    /// when `self` is neither [`Self::sorted_first`] nor
    /// [`Self::sorted_last`], `false` on both lex endpoints. The
    /// natural complement of [`Self::is_sorted_endpoint`] on the
    /// (endpoint, interior) boolean-partition axis over the LEX-axis
    /// endpoint surface.
    ///
    /// Sibling posture to [`Self::is_sorted_endpoint`] one arm over
    /// on the (endpoint, interior) partition — [`Self::is_sorted_endpoint`]
    /// fires on both lex-structural anchors, this method fires on
    /// every strictly-lex-interior slot. See [`Self::is_sorted_endpoint`]
    /// for the shared design rationale, sibling matrix, override axis,
    /// future-consumer inventory, THEORY.md grounding, and frontier
    /// inspiration — this method is the complement-direction arm of
    /// the same predicate-flavor axis and inherits every property from
    /// the lex-endpoint arm's documentation, differing only in the
    /// leading `!` negation and the strictly-lex-interior consumer
    /// surface (a bounded lex-loop that walks ONLY interior alphabet
    /// slots and short-circuits on either lex endpoint, a lex-phase-
    /// fold reducer whose interior arm processes NON-boundary
    /// alphabetized payloads and reserves the lex-endpoint arm for
    /// lex-boundary-only side effects, an alphabetized-completion pass
    /// that hides the alphabetically-first + alphabetically-last
    /// entries from a strictly-interior candidate list).
    ///
    /// Default body composes [`Self::is_sorted_endpoint`] with the
    /// standard-library `!` operator — the lex interior-membership
    /// predicate is a typed CONSEQUENCE of the lex boundary-membership
    /// disjunction, not a third codepath through
    /// `is_sorted_first ∧ is_sorted_last` inversion. Implementors
    /// override only when the lex interior-membership surface needs
    /// to diverge from the natural `!is_sorted_endpoint(self)` shape
    /// (no production implementor reaches for this today; the axis
    /// exists for the same reason `is_sorted_endpoint` /
    /// `is_sorted_first` / `is_sorted_last` overrides exist — a typed
    /// escape hatch rather than forcing the implementor to hand-roll
    /// the impl). An implementor that overrides
    /// [`Self::is_sorted_endpoint`] propagates the override through
    /// this default body automatically; the (variant → bool lex-
    /// interior-membership) projection funnels through ONE typed
    /// primitive.
    ///
    /// Singleton degeneracy — for a closed set with
    /// `T::CARDINALITY == 1`, [`Self::is_sorted_endpoint`] fires on
    /// the sole variant, so this method returns `false` for that
    /// variant. A singleton closed set has ZERO lex-interior slots by
    /// construction — [`Self::ALL`] is `[Self::Only]` and the sole
    /// element is BOTH lex endpoints simultaneously, leaving no room
    /// for a strictly-lex-interior slot. Mirrors the singleton
    /// collapse [`Self::is_sorted_endpoint`] observes and preserves
    /// the (lex-endpoint XOR lex-interior) partition semantics even
    /// at the boundary-cardinality edge.
    ///
    /// The (lex-endpoint, lex-interior) partition contract —
    /// `is_sorted_endpoint(v) != is_sorted_interior(v)` on every
    /// variant `v` in [`Self::ALL`] — is guaranteed by the default
    /// composition through [`Self::is_sorted_endpoint`]'s `!`
    /// negation; the well-formedness clause (33) pins the
    /// complementarity assertion on every implementor so a passing
    /// well-formedness sweep means every generic consumer can call
    /// [`Self::is_sorted_interior`] on any typed variant and expect
    /// the exact complement of [`Self::is_sorted_endpoint`] on the
    /// same variant. The lex-endpoint-anti-fix-point corollary
    /// `T::sorted_first().is_sorted_interior() == false` AND
    /// `T::sorted_last().is_sorted_interior() == false` is guaranteed
    /// by clause (33)'s lex-endpoint-anchor pin composed with the
    /// complementarity assertion.
    fn is_sorted_interior(self) -> bool {
        !<Self as ClosedSet>::is_sorted_endpoint(self)
    }

    /// The declaration-order endpoint anchor pair — the tuple
    /// `(T::first(), T::last())` projected onto the trait surface as
    /// ONE call. Closes the pair-aggregation corner of the closed-set
    /// endpoint-anchor return-shape axis on the DECLARATION side —
    /// the missing middle column between the two scalar endpoint
    /// primitives ([`Self::first`], [`Self::last`]) and the collection
    /// aggregation ([`Self::variants`]).
    ///
    /// The (return-shape × declaration-anchor) 3-of-3 return-shape
    /// column over the declaration-order anchor surface partitions
    /// post-lift:
    ///
    /// | Return shape                | Anchor surface           |
    /// |-----------------------------|--------------------------|
    /// | `Self` scalar (head)        | [`Self::first`]          |
    /// | `Self` scalar (tail)        | [`Self::last`]           |
    /// | `(Self, Self)` pair         | [`Self::endpoints`]      |
    /// | `Vec<Self>` collection      | [`Self::variants`]       |
    ///
    /// Sibling posture to [`Self::first`] + [`Self::last`] one return-
    /// shape axis over on the (scalar, pair) partition of the closed-
    /// set declaration-order anchor surface — [`Self::first`] and
    /// [`Self::last`] project the two anchors as separate scalar
    /// values, this method aggregates the two anchors into ONE tuple
    /// call. Every generic consumer that wants BOTH declaration-order
    /// endpoints (a bracketing renderer that emits `<head> ↔ <tail>`
    /// in a diagnostic, a saga-step engine that transitions through
    /// the head-anchor and tail-anchor states, a truth-table property
    /// test that anchors edge assertions at BOTH endpoints, a wire-
    /// format decoder that emits a per-run boundary payload naming
    /// both anchors, a `tatara-check` per-implementor coherence probe
    /// that renders both anchors in ONE diagnostic) binds to ONE
    /// typed call rather than hand-rolling the
    /// `(T::first(), T::last())` two-primitive re-derivation at every
    /// callsite (which pays TWO trait dispatches AND makes every
    /// downstream site depend on the tuple-construction shape).
    ///
    /// Default body composes [`Self::first`] with [`Self::last`]
    /// under the standard-library tuple constructor — the (head, tail)
    /// pair aggregation is a typed CONSEQUENCE of the two pre-existing
    /// endpoint-anchor primitives, not a third codepath through
    /// [`Self::ALL`] with slice-index-0 / slice-index-(N - 1) projection.
    /// Implementors override only when the pair
    /// aggregation needs to diverge from the natural
    /// `(first(), last())` shape (no production implementor reaches for
    /// this today; the axis exists for the same reason
    /// `via` / `set_label` / `labels` / `first` / `last` overrides
    /// exist — a typed escape hatch the trait surface exposes rather
    /// than forcing the implementor to hand-roll the impl). An
    /// implementor that overrides [`Self::first`] or [`Self::last`]
    /// propagates the override through this default body to the pair
    /// aggregation automatically; the (declaration-order head,
    /// declaration-order tail) pair-aggregation surface funnels
    /// through the two scalar endpoint-anchor primitives on each of
    /// its tuple slots.
    ///
    /// Singleton degeneracy — for a closed set with
    /// `T::CARDINALITY == 1`, [`Self::first`] and [`Self::last`] both
    /// return the sole variant, so this method returns
    /// `(Self::Only, Self::Only)` — the pair collapses to a diagonal
    /// tuple over the sole variant. A singleton closed set has ONE
    /// endpoint slot that IS both anchors simultaneously; the pair
    /// aggregation preserves the tuple SHAPE even at the boundary-
    /// cardinality edge where the two SLOTS collapse onto the same
    /// value. Mirrors the singleton collapse [`Self::is_endpoint`]
    /// observes on the (endpoint, interior) partition — both anchor
    /// slots pin the same variant and every generic consumer that
    /// destructures `let (head, tail) = T::endpoints();` reads the
    /// SAME typed variant into `head` and `tail`.
    ///
    /// The endpoint-anchor pair contract —
    /// `T::endpoints() == (T::first(), T::last())` on every
    /// implementor — is guaranteed by the default composition through
    /// the two scalar endpoint-anchor primitives; the well-formedness
    /// contract [`assert_closed_set_well_formed`]'s new clause (34)
    /// pins the composition against the natural
    /// `(first(), last())` shape on every implementor so a passing
    /// well-formedness sweep means every generic consumer can call
    /// [`Self::endpoints`] on any typed variant and expect the same
    /// tuple answer at every crate boundary.
    ///
    /// Future consumers — a boundary badge renderer that emits
    /// `<head> ↔ <tail>` in ONE call instead of a two-primitive
    /// composition at each rendering site, a range walker that
    /// destructures `let (head, tail) = T::endpoints();` and iterates
    /// through the declaration-order chain from `head.index_of()` to
    /// `tail.index_of()`, a saga-step audit event that logs both
    /// anchors atomically without threading the two primitives
    /// through the event constructor, a `tatara-check` per-
    /// implementor coherence probe that renders both anchors as a
    /// pair diagnostic, a truth-table property test that anchors edge
    /// assertions at BOTH endpoints through ONE destructure — bind to
    /// ONE trait method instead of hand-rolling the
    /// `(T::first(), T::last())` two-primitive composition at each
    /// callsite, and the closed-set declaration-order endpoint pair-
    /// aggregation surface evolves at ONE site rather than per-
    /// consumer.
    ///
    /// THEORY.md §III — the typescape; the (declaration-order head,
    /// declaration-order tail) pair-aggregation becomes a TYPE
    /// projection on the trait rather than a per-consumer inline
    /// `(T::first(), T::last())` composition at every downstream
    /// pair-endpoint site. The closed-set endpoint-anchor return-shape
    /// axis gains its pair-aggregation corner — the (`Self` scalar,
    /// `(Self, Self)` pair, `Vec<Self>` collection) return-shape
    /// column on the declaration-order endpoint-anchor row is now
    /// fully closed.
    /// THEORY.md §V.1 — knowable platform; the (declaration-order
    /// endpoint pair) aggregation was an unnamed compound of
    /// [`Self::first`] + [`Self::last`] pre-lift; naming it on the
    /// trait makes the projection a TYPED CONSEQUENCE of the two
    /// substrate primitives — generic consumers see ONE method, not
    /// ONE endpoint-pair-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the
    /// (declaration-order endpoint pair) aggregation emerges from the
    /// composition of TWO substrate primitives ([`Self::first`],
    /// [`Self::last`]) under the standard-library tuple constructor
    /// rather than as a per-implementor
    /// `const ENDPOINTS: (Self, Self) = (Self::Head, Self::Tail)`
    /// declaration. A future tightening of either primitive (a future
    /// const-fn endpoint-anchor axis that makes the pair callable in
    /// const contexts, a future perfect-hash anchor projection)
    /// propagates to every closed-set endpoint-pair consumer through
    /// this method's body.
    ///
    /// Frontier inspiration: Racket's `enum-endpoints` on closed
    /// enumerations (the pair-aggregation of the declaration-order
    /// head + tail anchors on the enumeration chain); Idris's
    /// `Fin (S n)` non-empty finite-cardinality types where
    /// `endpoints : Fin (S n) -> (Fin (S n), Fin (S n))` folds the
    /// (head, tail) endpoint pair through a shared tuple projection;
    /// Haskell's `(minBound, maxBound)` on the `Bounded` type-class
    /// pair — the endpoint-anchor pair exposed as a bare typed tuple
    /// rather than two separate scalar calls; MLIR's
    /// `RegisteredOperationName::begin_end()` on the declaration-
    /// order Op registry (the pair projection over the registered-op
    /// enumeration). Translation through pleme-io primitives: a pure
    /// default method composing the trait's existing [`Self::first`]
    /// and [`Self::last`] scalar endpoint-anchor primitives under the
    /// standard-library tuple constructor — no new dep, no new IR
    /// layer, no supertrait bound, no [`Option`]-typed dispatch.
    fn endpoints() -> (Self, Self) {
        (<Self as ClosedSet>::first(), <Self as ClosedSet>::last())
    }

    /// The lexicographic-order endpoint anchor pair — the tuple
    /// `(T::sorted_first(), T::sorted_last())` projected onto the
    /// trait surface as ONE call. Closes the pair-aggregation corner
    /// of the closed-set endpoint-anchor return-shape axis on the LEX
    /// side.
    ///
    /// Sibling posture to [`Self::endpoints`] one ordering axis over
    /// on the (declaration, lex) partition of the closed-set
    /// endpoint-anchor pair-aggregation surface — [`Self::endpoints`]
    /// fires on the declaration-order (head, tail) pair, this method
    /// fires on the lex-order (head, tail) pair. See
    /// [`Self::endpoints`] for the shared design rationale, sibling
    /// matrix, override axis, future-consumer inventory, THEORY.md
    /// grounding, and frontier inspiration — this method is the lex-
    /// ordering-axis arm of the same return-shape axis and inherits
    /// every property from the declaration-axis arm's documentation,
    /// differing only in the composition through
    /// [`Self::sorted_first`] / [`Self::sorted_last`] instead of
    /// [`Self::first`] / [`Self::last`] and the alphabetized consumer
    /// surface (an alphabetized boundary badge that emits
    /// `<lex-head> ↔ <lex-tail>` in a diagnostic, a lex-ordered saga-
    /// step engine that transitions through the lex-head-anchor and
    /// lex-tail-anchor states, an alphabetized truth-table property
    /// test that anchors edge assertions at BOTH lex endpoints
    /// through ONE destructure, an alphabetized-completion UI that
    /// renders the alphabetized boundary pair atomically).
    ///
    /// Default body composes [`Self::sorted_first`] with
    /// [`Self::sorted_last`] under the standard-library tuple
    /// constructor — the (lex-head, lex-tail) pair aggregation is a
    /// typed CONSEQUENCE of the two pre-existing lex-endpoint-anchor
    /// primitives, not a third codepath through
    /// [`Self::sorted_variants`] + `<[T]>::first` + `<[T]>::last` +
    /// `Option::copied` + `Option::unwrap`. Implementors override
    /// only when the lex-pair aggregation needs to diverge from the
    /// natural `(sorted_first(), sorted_last())` shape (no production
    /// implementor reaches for this today; the axis exists for the
    /// same reason `via` / `set_label` / `labels` / `sorted_labels` /
    /// `sorted_first` / `sorted_last` overrides exist — a typed
    /// escape hatch the trait surface exposes rather than forcing the
    /// implementor to hand-roll the impl). An implementor that
    /// overrides [`Self::sorted_first`] or [`Self::sorted_last`]
    /// propagates the override through this default body to the lex-
    /// pair aggregation automatically; the (lex-head, lex-tail) pair-
    /// aggregation surface funnels through the two scalar lex-
    /// endpoint-anchor primitives on each of its tuple slots.
    ///
    /// Singleton degeneracy — for a closed set with
    /// `T::CARDINALITY == 1`, [`Self::sorted_first`] and
    /// [`Self::sorted_last`] both return the sole variant, so this
    /// method returns `(Self::Only, Self::Only)` — the lex-pair
    /// collapses to a diagonal tuple over the sole variant. Mirrors
    /// [`Self::endpoints`]'s singleton collapse one ordering axis
    /// over and preserves the (lex-head, lex-tail) tuple SHAPE even
    /// at the boundary-cardinality edge where the two SLOTS collapse
    /// onto the same value.
    ///
    /// The lex-endpoint-anchor pair contract —
    /// `T::sorted_endpoints() == (T::sorted_first(), T::sorted_last())`
    /// on every implementor — is guaranteed by the default
    /// composition through the two scalar lex-endpoint-anchor
    /// primitives; the well-formedness contract
    /// [`assert_closed_set_well_formed`]'s new clause (35) pins the
    /// composition against the natural
    /// `(sorted_first(), sorted_last())` shape on every implementor
    /// so a passing well-formedness sweep means every generic
    /// consumer can call [`Self::sorted_endpoints`] on any typed
    /// variant and expect the same tuple answer at every crate
    /// boundary.
    ///
    /// The (ordering × pair-aggregation) 2×1 matrix over the closed-
    /// set endpoint-anchor return-shape axis is now closed at BOTH
    /// ordering corners — [`Self::endpoints`] on the declaration
    /// axis, this method on the lex axis. Together the two methods
    /// cover every ordering corner of the pair-return-shape column
    /// of the closed-set endpoint-anchor matrix. A future range-
    /// walker / boundary-badge / audit-event / per-implementor
    /// coherence probe consumer that binds either method sees the
    /// SAME `(Self, Self)` tuple shape at every crate boundary
    /// regardless of whether it walks the declaration or lex axis.
    fn sorted_endpoints() -> (Self, Self) {
        (
            <Self as ClosedSet>::sorted_first(),
            <Self as ClosedSet>::sorted_last(),
        )
    }

    /// Recover the canonical [`Self::label`] at declaration-order
    /// position `i` in [`Self::ALL`], or [`None`] if
    /// `i >= Self::CARDINALITY`.
    ///
    /// The direct `(usize → &'static str)` projection through the
    /// closed set — the missing corner of the (input carrier ×
    /// return-projection) 3-of-4 matrix formed by the three
    /// pre-existing (variant, `&str` label, `usize` index) inbound-
    /// projection surfaces on the trait:
    ///
    /// | Input carrier    | Output projection      | Projection surface        |
    /// |------------------|------------------------|---------------------------|
    /// | typed variant    | `&'static str` label   | [`Self::label`]           |
    /// | typed variant    | `usize` index          | [`Self::index_of`]        |
    /// | `usize` index    | typed variant          | [`Self::from_index`]      |
    /// | `usize` index    | `&'static str` label   | [`Self::label_at`]        |
    ///
    /// Together with [`Self::label`], [`Self::index_of`], and
    /// [`Self::from_index`], this method closes the projection
    /// triangle over the three closed-set carriers (typed variant,
    /// `&'static str` canonical label, `usize` declaration-order
    /// index) with a direct surface at every (input, output) pair.
    /// Every projection through the closed set — variant → label,
    /// variant → index, index → variant, and index → label — binds
    /// to ONE trait method rather than routing through a two-step
    /// composition at the call site.
    ///
    /// Sibling posture to [`Self::from_index`] one axis over on the
    /// (return-projection) axis of the `usize`-carrier partition:
    /// [`Self::from_index`] projects a `usize` position onto its
    /// typed variant through direct [`Self::ALL`] slice indexing,
    /// this method projects the same `usize` position through to the
    /// typed variant's canonical [`Self::label`] rendering — one
    /// composition step further along the same axis. Both return an
    /// [`Option`] because the input carrier is wider than the closed
    /// set — every out-of-range `usize` decodes to [`None`] on both
    /// projections, and both agree on the (in-range accept,
    /// out-of-range reject) partition slot-for-slot by construction
    /// (this method's default body is [`Self::from_index`] composed
    /// with [`Self::label`], so any consumer that decodes through
    /// this method sees the SAME `Option`-typed rejection arm every
    /// other index-carrier decoder sees).
    ///
    /// Default body composes [`Self::from_index`] with
    /// [`Self::label`] verbatim — the `usize → &'static str` shape
    /// is a typed CONSEQUENCE of the two pre-existing primitives, not
    /// a third codepath. Implementors override only when the
    /// composition needs to diverge from the natural
    /// `from_index(i).map(label)` shape (no production implementor
    /// reaches for this today; the axis exists for the same reason
    /// `via` / `set_label` / `labels` / `sorted_labels` /
    /// `sorted_variants` / `from_index` / `index_of` overrides exist —
    /// a typed escape hatch the trait surface exposes rather than
    /// forcing the implementor to hand-roll the impl). An implementor
    /// that overrides [`Self::from_index`] propagates the override
    /// through this default body to the direct-label projection
    /// automatically; the (typed variant, `&'static str` label,
    /// `usize` index) projection triangle funnels every `usize`-
    /// carrier decode through ONE typed primitive on each of its
    /// (variant, label) return-projection columns.
    ///
    /// The bounded-index contract — the out-of-range arm returns
    /// [`None`] for every `i >= Self::CARDINALITY` — is guaranteed by
    /// the default composition through [`Self::from_index`]'s
    /// `<[T]>::get` slice-bounded projection; the well-formedness
    /// contract [`assert_closed_set_well_formed`]'s new clause (20)
    /// pins the both-directions equality against the natural
    /// composition on every implementor, so a passing well-formedness
    /// sweep means every generic consumer can call `label_at` on any
    /// `usize` payload and expect the same `Option`-typed answer at
    /// every crate boundary.
    ///
    /// Future consumers — a compact wire-format decoder that emits
    /// `variant.index_of() as u8` and later renders
    /// `label_at(byte as usize)` for a diagnostic without materializing
    /// the typed variant AT ALL (the natural `Option`-typed rejection
    /// arm covers out-of-range serialized indices), a metrics tagger
    /// that stores per-slot counter payloads under
    /// `metrics[variant.index_of()]` and later renders per-slot
    /// diagnostics `<label_at(slot)>: <count>` in declaration order
    /// without a re-decode through [`Self::from_index`] +
    /// [`Self::label`] at each rendering site, a `tatara-check`
    /// per-slot diagnostic that walks `0..T::CARDINALITY` and renders
    /// each slot's canonical label without carrying the typed
    /// variant, a bitset-observed-variant renderer that walks the set
    /// bits and renders each slot's label directly without a
    /// [`Self::from_index`]-then-[`Self::label`] two-step at each set
    /// bit — bind to ONE trait method instead of hand-rolling either
    /// `T::from_index(i).map(|v| v.label())` (which re-derives the
    /// same two-primitive composition at every callsite AND makes
    /// every downstream site depend on [`Self::from_index`]'s
    /// `Option`-typed dispatch shape) OR the inline
    /// `T::ALL.get(i).copied().map(|v| v.label())` (which re-derives
    /// the underlying three-primitive composition at every callsite)
    /// at each callsite, and the closed-set `(usize → label)` direct
    /// projection surface evolves at ONE site rather than per-consumer.
    ///
    /// THEORY.md §III — the typescape; the (`usize` array index →
    /// `&'static str` label) projection becomes a TYPE projection on
    /// the trait rather than a per-consumer inline
    /// `Self::from_index(i).map(|v| v.label())` composition at every
    /// downstream index-decode site. The (typed variant, `&'static str`
    /// label, `usize` index) projection triangle over the closed-set
    /// carriers gains its fourth direct edge — every (input, output)
    /// pair over the three carriers binds to ONE typed projection
    /// surface with no two-step composition at the call site.
    /// THEORY.md §V.1 — knowable platform; the (`usize` → `&'static str`)
    /// direct projection was an unnamed compound of [`Self::from_index`]
    /// composed with [`Self::label`] pre-lift; naming it on the trait
    /// makes the projection a TYPED CONSEQUENCE of the two substrate
    /// primitives — generic consumers see ONE method, not ONE
    /// index-to-label-shape-per-crate. The well-formedness clause (20) pins the composition
    /// against the natural `from_index(i).map(label)` shape on every
    /// implementor so a passing well-formedness sweep means every
    /// generic consumer can call `label_at` on any `usize` payload and
    /// expect the same `Option`-typed answer at every crate boundary.
    /// THEORY.md §VI.1 — generation over composition; the direct
    /// (`usize → label`) projection emerges from the composition of
    /// TWO substrate primitives ([`Self::from_index`], [`Self::label`])
    /// rather than as a per-implementor inline
    /// `from_index(i).map(label)` compound. A future tightening of
    /// either primitive (a future perfect-hash `from_index`, a future
    /// canonicalization-aware `label` projection that folds case /
    /// whitespace, a future const-fn `label` axis that makes the
    /// projection compile-time visible) propagates to every closed-set
    /// direct-label-projection consumer through ONE trait body.
    ///
    /// Frontier inspiration: Idris's `Fin n` finite-cardinality type
    /// with a canonical `showFin : Fin n -> String` projection — the
    /// direct (position → rendered label) surface emits as a single
    /// typed method on the finite-type universe rather than per-
    /// instance inline `showFin (fromNat i)` composition. MLIR's
    /// `mlir::OpBuilder::getOperationName(index)` on the Op registry
    /// composes the (index → op) lookup with the (op → name)
    /// projection into ONE direct `(index → name)` surface the
    /// DiagnosticEngine renders per-slot diagnostics against. Racket's
    /// `(enum-label enum i)` on a closed enum projects a declaration-
    /// order position onto its rendered canonical label directly.
    /// Translation through pleme-io primitives: a pure default method
    /// composing the trait's existing [`Self::from_index`] +
    /// [`Self::label`] surfaces — no new dep, no new IR layer, no
    /// supertrait bound, no allocation.
    fn label_at(i: usize) -> Option<&'static str> {
        <Self as ClosedSet>::from_index(i).map(<Self as ClosedSet>::label)
    }

    /// Recover the declaration-order `usize` position in [`Self::ALL`]
    /// of the variant labelled `s`, or [`None`] if `s` matches no
    /// variant's [`Self::label`].
    ///
    /// The direct `(&str → usize)` projection through the closed set —
    /// the missing corner of the (input carrier × return-projection)
    /// 5-of-6 matrix that clause (20)'s `label_at` addition left open
    /// on the projection triangle over the three closed-set carriers
    /// (typed variant, `&'static str` canonical label, `usize`
    /// declaration-order index):
    ///
    /// | Input carrier          | Output projection      | Projection surface           |
    /// |------------------------|------------------------|------------------------------|
    /// | typed variant          | `&'static str` label   | [`Self::label`]              |
    /// | typed variant          | `usize` index          | [`Self::index_of`]           |
    /// | `usize` index          | typed variant          | [`Self::from_index`]         |
    /// | `usize` index          | `&'static str` label   | [`Self::label_at`]           |
    /// | `&'static str` label   | typed variant          | [`Self::find_by_label`]      |
    /// | `&'static str` label   | `usize` index          | [`Self::index_of_label`]     |
    ///
    /// Together with [`Self::label`], [`Self::index_of`],
    /// [`Self::from_index`], [`Self::label_at`], and
    /// [`Self::find_by_label`], this method closes the projection
    /// triangle over the three closed-set carriers with a direct
    /// surface at EVERY (input, output) pair — the six directed edges
    /// spanning the three carriers each bind to ONE trait method
    /// rather than routing through a two-step composition at the call
    /// site. Every projection through the closed set — variant →
    /// label, variant → index, index → variant, index → label, label →
    /// variant, label → index — emits at ONE typed primitive.
    ///
    /// Sibling posture to [`Self::find_by_label`] one axis over on the
    /// (return-projection) axis of the `&'static str`-carrier
    /// partition: [`Self::find_by_label`] projects a `&str` label back
    /// onto its typed variant through the [`Self::ALL`] ×
    /// [`Self::label`] sweep, this method projects the same `&str`
    /// label through to its declaration-order `usize` position — one
    /// composition step further along the same axis. Both return an
    /// [`Option`] because the input carrier is wider than the closed
    /// set — every non-canonical `&str` decodes to [`None`] on both
    /// projections, and both agree on the (in-range accept,
    /// out-of-range reject) partition slot-for-slot by construction
    /// (this method's default body is [`Self::find_by_label`] composed
    /// with [`Self::index_of`], so any consumer that decodes through
    /// this method sees the SAME `Option`-typed rejection arm every
    /// other `&str`-carrier decoder sees).
    ///
    /// Sibling posture to [`Self::label_at`] one axis over on the
    /// (input-carrier) axis of the projection triangle: [`Self::label_at`]
    /// closes the `usize`-carrier direct-label projection, this method
    /// closes the `&str`-carrier direct-index projection — both are
    /// the "one composition step further" corner past the immediate
    /// carrier decode ([`Self::from_index`] / [`Self::find_by_label`]),
    /// projecting through to the OTHER return-projection column that
    /// the same carrier partition exposes. Together the two direct
    /// projections close the (carrier × further-column) 2×2 matrix
    /// past the two immediate decoders on both carrier partitions.
    ///
    /// Default body composes [`Self::find_by_label`] with
    /// [`Self::index_of`] verbatim — the `&str → usize` shape is a
    /// typed CONSEQUENCE of the two pre-existing primitives, not a
    /// third codepath. Implementors override only when the composition
    /// needs to diverge from the natural `find_by_label(s).map(index_of)`
    /// shape (no production implementor reaches for this today; the
    /// axis exists for the same reason `via` / `set_label` / `labels` /
    /// `sorted_labels` / `sorted_variants` / `from_index` / `index_of` /
    /// `label_at` overrides exist — a typed escape hatch the trait
    /// surface exposes rather than forcing the implementor to
    /// hand-roll the impl). An implementor that overrides
    /// [`Self::find_by_label`] (a future perfect-hash label-decoder, a
    /// future canonicalization-aware label projection that folds case
    /// or whitespace) propagates the override through this default
    /// body to the direct-index projection automatically; the
    /// projection triangle funnels every `&str`-carrier decode through
    /// ONE typed primitive on each of its (variant, `usize` index)
    /// return-projection columns.
    ///
    /// The rejection contract — a non-canonical `&str` returns
    /// [`None`], the empty-string boundary that clause (4) reserves as
    /// structurally outside the closed set returns [`None`] — is
    /// guaranteed by the default composition through
    /// [`Self::find_by_label`]'s `Option`-typed sweep; the
    /// well-formedness contract [`assert_closed_set_well_formed`]'s
    /// new clause (21) pins the both-directions equality against the
    /// natural composition on every implementor, so a passing
    /// well-formedness sweep means every generic consumer can call
    /// `index_of_label` on any `&str` payload and expect the same
    /// `Option`-typed answer at every crate boundary.
    ///
    /// Future consumers — a compact wire-format encoder that reads a
    /// `&str` config value (a Kubernetes annotation, a YAML enum
    /// field, a diagnostic input string) and emits its declaration-
    /// order position directly as a `u8` without materializing the
    /// typed variant at the encoder site, a metrics binner that reads
    /// a diagnostic label from an incoming trace event and increments
    /// `counters[T::index_of_label(label)?]` under the per-slot
    /// aggregation shape without a two-step (`find_by_label` →
    /// `index_of`) composition at each rendering site, a `tatara-check`
    /// per-slot per-label diagnostic that partitions a batch of
    /// incoming labels by declaration-order slot for reporting under
    /// the natural `[per_slot; T::CARDINALITY]` aggregation shape, an
    /// LSP-hover / config-decoder rendering that maps parsed labels
    /// back to their canonical slot ordering for stable rendering —
    /// bind to ONE trait method instead of hand-rolling either
    /// `T::find_by_label(s).map(T::index_of)` (which re-derives the
    /// same two-primitive composition at every callsite AND makes
    /// every downstream site depend on [`Self::find_by_label`]'s
    /// `Option`-typed dispatch shape) OR the inline
    /// `T::ALL.iter().position(|v| v.label() == s)` (which re-derives
    /// the underlying three-primitive composition at every callsite,
    /// AND drops the [`Self::find_by_label`] override propagation that
    /// keeps every carrier-decode consumer aligned on a single typed
    /// dispatch) at each callsite, and the closed-set `(&str → index)`
    /// direct projection surface evolves at ONE site rather than
    /// per-consumer.
    ///
    /// THEORY.md §III — the typescape; the (`&'static str` label →
    /// `usize` array index) projection becomes a TYPE projection on
    /// the trait rather than a per-consumer inline
    /// `Self::find_by_label(s).map(Self::index_of)` composition at
    /// every downstream label-to-slot site. The projection triangle
    /// over the closed-set carriers gains its sixth (and final)
    /// direct edge — every (input, output) pair across the (typed
    /// variant, `&'static str` label, `usize` index) carriers binds
    /// to ONE typed projection surface with no two-step composition
    /// at the call site.
    /// THEORY.md §V.1 — knowable platform; the (`&str` → `usize`)
    /// direct projection was an unnamed compound of
    /// [`Self::find_by_label`] composed with [`Self::index_of`]
    /// pre-lift; naming it on the trait makes the projection a TYPED
    /// CONSEQUENCE of the two substrate primitives — generic
    /// consumers see ONE method, not ONE label-to-index-shape-per-crate.
    /// The well-formedness clause (21) pins the composition against
    /// the natural `find_by_label(s).map(index_of)` shape on every
    /// implementor so a passing well-formedness sweep means every
    /// generic consumer can call `index_of_label` on any `&str`
    /// payload and expect the same `Option`-typed answer at every
    /// crate boundary.
    /// THEORY.md §VI.1 — generation over composition; the direct
    /// (`&str → index`) projection emerges from the composition of
    /// TWO substrate primitives ([`Self::find_by_label`],
    /// [`Self::index_of`]) rather than as a per-implementor inline
    /// `find_by_label(s).map(index_of)` compound. A future tightening
    /// of either primitive (a future perfect-hash `find_by_label`, a
    /// future canonicalization-aware label projection that folds
    /// case / whitespace, a future const-fn `index_of` axis that
    /// makes the projection compile-time visible) propagates to
    /// every closed-set direct-index-projection consumer through ONE
    /// trait body.
    ///
    /// Frontier inspiration: Racket's `(enum-index enum sym)` on a
    /// closed enum projects a symbol directly onto its
    /// declaration-order position, without an intermediate typed-
    /// variant materialization the caller must round-trip through.
    /// MLIR's `mlir::TypeID::getIndex(StringRef name)` on the typed
    /// registry composes the (name → op) lookup with the (op →
    /// stable index) projection into ONE direct `(name → index)`
    /// surface the DiagnosticEngine's per-op counters key off.
    /// Clojure's `(.indexOf enum-values kw)` idiom over a keyword-
    /// enum's canonical value set stands as the same shape one
    /// vocabulary over on the JVM-Lisp side. Translation through
    /// pleme-io primitives: a pure default method composing the
    /// trait's existing [`Self::find_by_label`] + [`Self::index_of`]
    /// surfaces — no new dep, no new IR layer, no supertrait bound,
    /// no allocation.
    fn index_of_label(s: &str) -> Option<usize> {
        <Self as ClosedSet>::find_by_label(s).map(<Self as ClosedSet>::index_of)
    }

    /// The declaration-order-index sibling of [`Self::index_of`] one
    /// ordering-axis over — the direct (typed variant → `usize`
    /// lexicographic-order index) projection through the closed set,
    /// keyed on [`Self::label`] under the standard-library `str: Ord`
    /// ordering. Returns the position `v` would occupy in
    /// [`Self::sorted_variants`] — equivalently, the count of canonical
    /// labels strictly less than [`Self::label`]`(self)` under `str::cmp`.
    ///
    /// Closes the (typed variant → `usize` lexicographic index) forward
    /// edge of the (variant, `&'static str` label, `usize` position)
    /// projection triangle on the LEX ordering axis — the sibling
    /// posture to [`Self::index_of`] one ordering-axis over on the
    /// (declaration, lex) partition of the (variant → position)
    /// forward-projection surface. Together with [`Self::index_of`], the
    /// two methods bracket both ordering axes at the (variant → position)
    /// forward edge — every generic consumer that anchors a per-slot
    /// data structure on the closed set (a per-slot metrics counter, a
    /// lex-sorted diagnostic renderer, a compact wire encoding whose
    /// bytes sit in lex order rather than declaration order, a
    /// bitset-observed-slot renderer that walks lex-sorted rendering
    /// order) binds to the lex-ordering-axis surface at ONE trait
    /// method rather than routing through a
    /// `sorted_variants().iter().position(|v| v == self)` composition at
    /// every callsite.
    ///
    /// The (ordering-axis × forward-projection) partition post-lift:
    ///
    /// | Ordering axis        | Forward projection surface   |
    /// |----------------------|------------------------------|
    /// | Declaration order    | [`Self::index_of`]           |
    /// | Lexicographic order  | [`Self::sorted_index_of`]    |
    ///
    /// Default body is a zero-alloc single-pass linear scan over
    /// [`Self::ALL`] keyed on [`Self::label`] — a strict-`<` label
    /// comparison against `self`'s canonical label counted through the
    /// entire slice. The count equals the lex-order position by clause
    /// (3)'s label-pairwise-distinctness contract: no two canonical
    /// labels can be equal, so `str: Ord`'s total order on the labels
    /// projects bijectively onto `0..T::CARDINALITY`, and the count of
    /// strictly-lesser labels is the unique lex-order slot. Implementors
    /// override only when the composition needs to diverge from the
    /// natural label-keyed strict-`<` count shape — a typed escape
    /// hatch the trait surface exposes rather than forcing the
    /// implementor to hand-roll the impl.
    ///
    /// The lex-position contract — the returned `usize` sits in
    /// `0..T::CARDINALITY` for every canonical variant — is guaranteed
    /// by the default composition: the label-pairwise-distinctness
    /// contract makes the strict-`<` count strictly less than
    /// [`Self::CARDINALITY`] (the variant itself never counts, and
    /// [`Self::CARDINALITY - 1`] other variants can at most all sit
    /// below it under `str::cmp`); the well-formedness contract
    /// [`assert_closed_set_well_formed`]'s new clause (22) pins the
    /// both-directions equality against `T::sorted_variants()`'s
    /// position of `self` on every implementor, so a passing
    /// well-formedness sweep means every generic consumer can call
    /// `sorted_index_of` on any typed variant and expect the same
    /// `usize` answer at every crate boundary.
    ///
    /// Future consumers — a lex-sorted per-slot metrics binner that
    /// stores counter payloads at `metrics[variant.sorted_index_of()]`
    /// so a natural per-slot walk in declaration order renders the
    /// metrics in lex-sorted rendering order without a re-sort at the
    /// rendering site, a compact wire-format encoder that emits
    /// `variant.sorted_index_of() as u8` when the wire protocol pins
    /// lex-order stability (a legal / regulatory contract that pins
    /// the byte-order semantics on the CANONICAL alphabetic order
    /// rather than the DECLARATION order — the two are structurally
    /// distinct when the closed set's canonical ordering is defined by
    /// alphabetic order rather than declaration order), a
    /// bitset-observed-slot renderer that renders lex-sorted diagnostics
    /// by walking the bit indices in lex order, a `tatara-check`
    /// per-slot diagnostic that renders `<label>: <count>` in lex order
    /// by keying counters on `sorted_index_of` — bind to ONE trait
    /// method instead of hand-rolling either
    /// `T::sorted_variants().iter().position(|v| v == self).unwrap()`
    /// (which pays a `Vec<Self>` allocation the label-keyed count doesn't
    /// need AND requires a `PartialEq` bound on the closed set) OR the
    /// inline `T::ALL.iter().filter(|v| v.label() < self.label()).count()`
    /// (which re-derives the same one-primitive composition at every
    /// callsite AND makes every downstream site depend on the
    /// [`Self::label`]-keyed strict-`<` shape) at each callsite, and
    /// the closed-set (variant → lex position) direct projection
    /// surface evolves at ONE site rather than per-consumer.
    ///
    /// THEORY.md §III — the typescape; the (typed variant → `usize`
    /// lexicographic-order position) projection becomes a TYPE
    /// projection on the trait rather than a per-consumer inline
    /// `Self::sorted_variants().iter().position(|v| v == self)`
    /// composition at every downstream lex-index site. The (declaration,
    /// lex) × (variant → position) 1×2 forward-projection partition
    /// completes at BOTH ordering axes.
    /// THEORY.md §V.1 — knowable platform; the (variant → lex position)
    /// projection was an unnamed compound of [`Self::sorted_variants`] +
    /// `Iterator::position` + `PartialEq` pre-lift; naming it on the
    /// trait makes the projection a TYPED CONSEQUENCE of [`Self::ALL`]
    /// combined with [`Self::label`] alone — generic consumers see ONE
    /// method, not ONE lex-position-shape-per-crate. Clause (22) pins
    /// [`Self::sorted_index_of`] against `T::sorted_variants()`'s
    /// position of `self` on every implementor so a passing
    /// well-formedness sweep means every generic consumer can call
    /// `sorted_index_of` on any typed variant and expect the same
    /// `usize` answer at every crate boundary.
    /// THEORY.md §VI.1 — generation over composition; the (variant →
    /// lex position) projection emerges from the composition of TWO
    /// substrate primitives ([`Self::ALL`], [`Self::label`]) via the
    /// standard-library `str: Ord` strict-`<` comparator rather than
    /// as a per-implementor inline `sorted_variants().iter().position`
    /// or a `const LEX_POS: [usize; N] = [...]` static table. A future
    /// tightening of either primitive (a future canonicalization-aware
    /// `label` projection, a future
    /// `#[closed_set(compare_labels_with = ...)]` derive attribute that
    /// swaps the ordering, a future case-insensitive-label extension)
    /// propagates to every closed-set lex-position consumer through
    /// ONE trait body.
    ///
    /// Frontier inspiration: Racket's `(sort-index enum sym)` on a
    /// closed enum projects a symbol directly onto its
    /// lexicographic-order position without an intermediate sorted-list
    /// materialization the caller must round-trip through. Idris's
    /// `Data.List.findIndex` composed with a labeling projection on the
    /// `Fin n` finite-cardinality universe delivers the same shape one
    /// vocabulary over on the dependent-type side. MLIR's
    /// `RegisteredOperationName::getStableIndex()` on the
    /// lexicographically-sorted Op registry gives each Op kind a
    /// canonical lex-order slot the DiagnosticEngine's per-slot
    /// counters key off. Haskell's `Data.List.elemIndex` composed with
    /// a `sortBy comparingLabel` prelude on a closed enumeration
    /// projects the same (variant → lex position) shape via the same
    /// two-primitive composition. Translation through pleme-io
    /// primitives: a pure default method composing the trait's
    /// existing [`Self::ALL`] combined with [`Self::label`] surfaces
    /// via a strict-`<` linear scan — no new dep, no new IR layer, no
    /// supertrait bound, no `Vec` allocation, no `PartialEq` bound, no
    /// `Option`-typed dispatch.
    fn sorted_index_of(self) -> usize {
        let my_label = <Self as ClosedSet>::label(self);
        let mut count = 0usize;
        for &v in Self::ALL {
            if <Self as ClosedSet>::label(v) < my_label {
                count += 1;
            }
        }
        count
    }

    /// Recover the typed variant at lexicographic-order position `i` in
    /// [`Self::sorted_variants`], or [`None`] if `i >= Self::CARDINALITY`.
    ///
    /// The typed inverse of [`Self::sorted_index_of`] on the (typed
    /// variant ↔ `usize` lexicographic-order position) bijection:
    /// [`Self::sorted_index_of`] projects a variant onto its `usize`
    /// lex-order position through the strict-`<` label sweep over
    /// [`Self::ALL`]; this method projects a `usize` lex-order position
    /// back onto its typed variant. Together the two projections close
    /// the (variant ↔ lex-order position) bijection with
    /// `0..T::CARDINALITY` at BOTH directions — every generic consumer
    /// that stores a `variant.sorted_index_of()` for later lex-order
    /// decode (a compact wire encoding whose bytes sit in lex order and
    /// later recovers the variant, a lex-sorted slotted lookup table
    /// scanned back to `(variant, payload)` pairs for exhaustive lex-
    /// order iteration, a lex-order bitset walked back to the set of
    /// observed variants in canonical alphabetic order, a Sekiban audit-
    /// trail metric that stores per-lex-slot counters and later renders
    /// `<variant>: <count>` diagnostics in lex order) binds to ONE
    /// typed inverse method rather than hand-rolling either
    /// `Self::sorted_variants().get(i).copied()` (which pays a
    /// `Vec<Self>` allocation the direct-projection surface doesn't
    /// need AND re-derives the same three-primitive composition at
    /// every callsite) OR a per-implementor inline `match i { 0 =>
    /// Some(v0), 1 => Some(v1), _ => None }` keyed on the lex slot
    /// (which re-derives the per-variant lex-slot → variant table at
    /// every callsite AND drifts silently when [`Self::ALL`] gains a
    /// new variant that reorders the lex partition).
    ///
    /// Sibling posture to [`Self::from_index`] one ordering-axis over on
    /// the (declaration, lex) partition of the (`usize` position →
    /// variant) inverse-projection surface: [`Self::from_index`]
    /// projects a `usize` position back onto its typed variant through
    /// direct [`Self::ALL`] slice indexing under declaration order, this
    /// method projects a `usize` position back onto its typed variant
    /// through [`Self::sorted_variants`] slice indexing under
    /// lexicographic order. Both return an [`Option<Self>`] because the
    /// input carrier is wider than the closed set — every out-of-range
    /// `usize` decodes to [`None`] on both axes. Both share the SAME
    /// zero-`PartialEq`-bound shape and the SAME `Option`-typed
    /// rejection arm; a generic consumer freely swaps between the two
    /// inverse-decode surfaces based on its ordering-axis carrier
    /// without changing the program's structured-decode semantics.
    ///
    /// The (ordering-axis × inverse-projection) partition post-lift:
    ///
    /// | Ordering axis        | Inverse projection surface       |
    /// |----------------------|----------------------------------|
    /// | Declaration order    | [`Self::from_index`]             |
    /// | Lexicographic order  | [`Self::from_sorted_index`]      |
    ///
    /// Closes the second direct edge of the (variant, `&'static str`
    /// label, `usize` position) projection triangle on the LEX ordering
    /// axis — [`Self::sorted_index_of`] closed the first direct edge
    /// (variant → lex position), this method closes the second direct
    /// edge (lex position → variant). Downstream lifts
    /// ([`Self::sorted_label_at`], [`Self::sorted_index_of_label`])
    /// close the remaining lex-axis edges on the same natural
    /// composition base.
    ///
    /// Default body composes ONE substrate primitive
    /// ([`Self::sorted_variants`]) with the standard-library
    /// `<[T]>::get` bounded-index projection and [`Option::copied`] —
    /// no discriminant sweep, no `PartialEq` bound, no per-variant
    /// `match`. Sibling posture to [`Self::from_index`]'s
    /// `Self::ALL.get(i).copied()` shape one ordering-axis over: both
    /// bind the natural bounded-decode arm through the same
    /// `<[T]>::get` composition on the corresponding sibling ALL-array
    /// surface. Implementors override with a per-index `match` when
    /// the O(1) slice lookup shows up on a hot-path profile (the
    /// substrate-wide typed-emission bind: no production site today
    /// calls `from_sorted_index` on a per-message hot path, so the
    /// default slice lookup costs nothing measurable, and the override
    /// axis exists for the same reason `via` / `set_label` / `labels` /
    /// `index_of` / `from_index` overrides exist — a typed escape hatch
    /// the trait surface exposes rather than forcing the implementor to
    /// hand-roll the impl).
    ///
    /// The bounded-index contract — the out-of-range arm returns
    /// [`None`] for every `i >= Self::CARDINALITY` — is guaranteed by
    /// the default `<[T]>::get` composition; the well-formedness
    /// contract [`assert_closed_set_well_formed`]'s new clause (23)
    /// pins the both-directions equality on every implementor, so a
    /// passing well-formedness sweep means every generic consumer can
    /// call `from_sorted_index` on any `usize` payload and expect the
    /// same `Option`-typed answer at every crate boundary.
    ///
    /// Future consumers — a lex-order compact wire decoder that maps a
    /// `u8` lex slot back to its variant (a legal / regulatory contract
    /// that pins the byte-order semantics on the CANONICAL alphabetic
    /// order rather than declaration order — the two are structurally
    /// distinct when the closed set's canonical ordering is defined by
    /// alphabetic order), a lex-order slotted lookup table
    /// `[Payload; T::CARDINALITY]` scanned back to `(variant, payload)`
    /// pairs by walking `for i in 0..T::CARDINALITY {
    /// let v = T::from_sorted_index(i).unwrap(); ... }` for exhaustive
    /// lex-order iteration, a lex-order bitset over the closed set
    /// walked back to the set of observed variants in canonical
    /// alphabetic order by mapping each observed bit back through this
    /// method, a `tatara-check` per-slot diagnostic that renders
    /// `<label>: <count>` in lex order and later recovers the typed
    /// variant per slot for cross-checks — bind to ONE trait method
    /// instead of composing three primitives
    /// (`sorted_variants()` + `<[T]>::get` + `Option::copied`) with a
    /// `Vec<Self>` allocation at every call site.
    ///
    /// THEORY.md §III — the typescape; the (`usize` lex position →
    /// typed variant) projection becomes a TYPE projection on the
    /// trait rather than a per-consumer inline
    /// `Self::sorted_variants().get(i).copied()` composition at every
    /// downstream lex-order inverse-decode site. The (declaration, lex)
    /// × (position → variant) 1×2 inverse-projection partition
    /// completes at BOTH ordering axes.
    /// THEORY.md §V.1 — knowable platform; the (lex position → typed
    /// variant) projection was an unnamed compound of
    /// [`Self::sorted_variants`] + `<[T]>::get` + `Option::copied`
    /// pre-lift; naming it on the trait makes the projection a TYPED
    /// CONSEQUENCE of [`Self::sorted_variants`] — generic consumers
    /// see ONE method, not ONE lex-inverse-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the (lex
    /// position → typed variant) projection emerges from the
    /// composition of ONE substrate primitive
    /// ([`Self::sorted_variants`]) with the standard-library
    /// `<[T]>::get` bounded-index projection and the standard-library
    /// `Option::copied` primitive rather than as a per-implementor
    /// inline `match` block. A future tightening of
    /// [`Self::sorted_variants`] (a future
    /// `#[closed_set(compare_labels_with = ...)]` derive attribute that
    /// swaps the ordering, a future canonicalization-aware `label`
    /// projection that reorders the lex partition) propagates to every
    /// closed-set lex-order inverse-decode consumer through ONE trait
    /// method.
    ///
    /// Frontier inspiration: Idris's `Fin n` finite-cardinality type
    /// with `natToFin : Nat -> (n : Nat) -> Maybe (Fin n)` composed
    /// with a lex-sorted labeling projection over the finite-type
    /// universe — the finite-type surface exposes a canonical (natural
    /// → element) bounded-decode projection that a downstream compact-
    /// encoding binds to, in either declaration-order OR lex-order
    /// through the composition. Racket's `(enum-index->object enum n
    /// #:order 'lex)` on a closed enum decodes an index back to its
    /// variant under the chosen ordering; MLIR's
    /// `RegisteredOperationName::get(int)` on the lex-sorted Op registry
    /// decodes a stable lex index back to its Op kind; Haskell's
    /// `Data.List.genericIndex` composed with a `sortBy comparingLabel`
    /// prelude on a closed enumeration projects the same (lex position
    /// → variant) shape via the same two-primitive composition.
    /// Translation through pleme-io primitives: a pure default method
    /// composing the trait's existing [`Self::sorted_variants`]
    /// surface with `<[T]>::get` and `Option::copied` — no new dep, no
    /// new IR layer, no supertrait bound.
    fn from_sorted_index(i: usize) -> Option<Self> {
        Self::sorted_variants().get(i).copied()
    }

    /// Recover the canonical [`Self::label`] at lexicographic-order
    /// position `i` in [`Self::sorted_labels`], or [`None`] if
    /// `i >= Self::CARDINALITY`.
    ///
    /// The direct `(usize` lex-order position → `&'static str` canonical
    /// label) projection through the closed set — the third direct edge
    /// of the (typed variant, `&'static str` label, `usize` position)
    /// projection triangle on the LEX ordering axis. Sibling posture to
    /// [`Self::label_at`] one ordering-axis over on the (declaration,
    /// lex) partition of the (`usize` position → `&'static str` label)
    /// forward-projection surface: [`Self::label_at`] projects a
    /// declaration-order `usize` position onto its canonical label
    /// through [`Self::from_index`] + [`Self::label`], this method
    /// projects the same `usize` position through
    /// [`Self::from_sorted_index`] + [`Self::label`] under the
    /// lexicographic ordering. Together the two direct projections
    /// bracket both ordering axes at the (`usize` position →
    /// `&'static str` label) forward edge — every generic consumer that
    /// renders per-slot diagnostics against a `usize` slot binds to the
    /// ordering-axis surface at ONE trait method rather than routing
    /// through a `from_sorted_index(i).map(label)` composition or a
    /// `sorted_labels().get(i).copied()` route at every callsite.
    ///
    /// The (ordering-axis × direct-projection) partition post-lift:
    ///
    /// | Ordering axis        | (`usize` → label) projection surface |
    /// |----------------------|--------------------------------------|
    /// | Declaration order    | [`Self::label_at`]                   |
    /// | Lexicographic order  | [`Self::sorted_label_at`]            |
    ///
    /// Closes the third direct edge of the lex-axis projection triangle
    /// — [`Self::sorted_index_of`] closed the first direct edge (variant
    /// → lex position), [`Self::from_sorted_index`] closed the second
    /// (lex position → variant), this method closes the third (lex
    /// position → `&'static str` label). [`Self::sorted_index_of_label`]
    /// closes the fourth (and final) direct edge (`&str` label → lex
    /// position) to complete the lex-axis triangle.
    ///
    /// Default body composes [`Self::from_sorted_index`] with
    /// [`Self::label`] verbatim — the `usize → &'static str` shape on
    /// the lex axis is a typed CONSEQUENCE of the two pre-existing
    /// primitives, not a third codepath. Implementors override only
    /// when the composition needs to diverge from the natural
    /// `from_sorted_index(i).map(label)` shape — a typed escape hatch
    /// the trait surface exposes (same axis as `via` / `set_label` /
    /// `labels` / `sorted_labels` / `sorted_variants` / `from_index` /
    /// `index_of` / `label_at` / `from_sorted_index` overrides). An
    /// implementor that overrides [`Self::from_sorted_index`] (a future
    /// perfect-hash lex-slot decoder, a future canonicalization-aware
    /// `label` projection that reorders the lex partition) propagates
    /// the override through this default body to the direct-label
    /// projection automatically; the lex-axis projection triangle
    /// funnels every `usize`-carrier lex-decode through ONE typed
    /// primitive on each of its (typed variant, `&'static str` label)
    /// return-projection columns.
    ///
    /// The bounded-index contract — the out-of-range arm returns
    /// [`None`] for every `i >= Self::CARDINALITY` — is guaranteed by
    /// the default composition through [`Self::from_sorted_index`]'s
    /// `<[T]>::get` slice-bounded projection; the well-formedness
    /// contract [`assert_closed_set_well_formed`]'s new clause (24)
    /// pins the both-directions equality against the natural
    /// composition on every implementor, so a passing well-formedness
    /// sweep means every generic consumer can call `sorted_label_at` on
    /// any `usize` payload and expect the same `Option`-typed answer at
    /// every crate boundary.
    ///
    /// Future consumers — a lex-order compact wire-format decoder that
    /// emits `variant.sorted_index_of() as u8` and later renders
    /// `sorted_label_at(byte as usize)` for a diagnostic WITHOUT
    /// materializing the typed variant at the rendering site (the
    /// natural `Option`-typed rejection arm covers out-of-range
    /// serialized lex indices), a lex-sorted metrics binner that stores
    /// per-slot counter payloads under
    /// `metrics[variant.sorted_index_of()]` and later renders per-slot
    /// diagnostics `<sorted_label_at(slot)>: <count>` in lex order
    /// without a re-decode through [`Self::from_sorted_index`] +
    /// [`Self::label`] at each rendering site, a `tatara-check`
    /// per-slot diagnostic that walks `0..T::CARDINALITY` and renders
    /// each lex slot's canonical label without carrying the typed
    /// variant, a bitset-observed-variant renderer that walks the set
    /// bits under the lex-order rendering shape and renders each lex
    /// slot's label directly without a
    /// [`Self::from_sorted_index`]-then-[`Self::label`] two-step at
    /// each set bit — bind to ONE trait method instead of hand-rolling
    /// either `T::from_sorted_index(i).map(|v| v.label())` (which
    /// re-derives the same two-primitive composition at every callsite
    /// AND makes every downstream site depend on
    /// [`Self::from_sorted_index`]'s `Option`-typed dispatch shape) OR
    /// the inline `T::sorted_labels().get(i).copied()` (which pays a
    /// `Vec<&'static str>` allocation the direct-projection surface
    /// doesn't need AND re-derives the three-primitive composition at
    /// every callsite) at each callsite, and the closed-set `(lex
    /// position → label)` direct projection surface evolves at ONE site
    /// rather than per-consumer.
    ///
    /// THEORY.md §III — the typescape; the (`usize` lex-order position
    /// → `&'static str` canonical label) projection becomes a TYPE
    /// projection on the trait rather than a per-consumer inline
    /// `Self::from_sorted_index(i).map(|v| v.label())` composition at
    /// every downstream lex-slot-decode site. The (declaration, lex) ×
    /// (`usize` position → `&'static str` label) 1×2 forward-projection
    /// partition completes at BOTH ordering axes.
    /// THEORY.md §V.1 — knowable platform; the (`usize` lex position
    /// → `&'static str` label) projection was an unnamed compound of
    /// [`Self::from_sorted_index`] composed with [`Self::label`]
    /// pre-lift; naming it on the trait makes the projection a TYPED
    /// CONSEQUENCE of the two substrate primitives — generic consumers
    /// see ONE method, not ONE lex-slot-to-label-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the direct
    /// (lex position → label) projection emerges from the composition
    /// of TWO substrate primitives ([`Self::from_sorted_index`],
    /// [`Self::label`]) rather than as a per-implementor inline
    /// `from_sorted_index(i).map(label)` compound. A future tightening
    /// of either primitive (a future perfect-hash `from_sorted_index`,
    /// a future canonicalization-aware `label` projection that folds
    /// case / whitespace, a future const-fn `label` axis that makes the
    /// projection compile-time visible) propagates to every closed-set
    /// direct-lex-label-projection consumer through ONE trait body.
    ///
    /// Frontier inspiration: Racket's `(enum-label enum i #:order 'lex)`
    /// on a closed enum projects a lex-order position onto its rendered
    /// canonical label directly under the chosen ordering; MLIR's
    /// `RegisteredOperationName::getStableName(int)` on the lex-sorted
    /// Op registry projects a stable lex slot onto its canonical name
    /// the DiagnosticEngine renders per-slot diagnostics against.
    /// Haskell's `Data.List.genericIndex (sortBy comparingLabel labels)
    /// i` composes the sort with the bounded-index projection into the
    /// same `(lex position → label)` shape one vocabulary over.
    /// Translation through pleme-io primitives: a pure default method
    /// composing the trait's existing [`Self::from_sorted_index`] +
    /// [`Self::label`] surfaces — no new dep, no new IR layer, no
    /// supertrait bound, no allocation.
    fn sorted_label_at(i: usize) -> Option<&'static str> {
        <Self as ClosedSet>::from_sorted_index(i).map(<Self as ClosedSet>::label)
    }

    /// Recover the lexicographic-order `usize` position of the variant
    /// labelled `s`, or [`None`] if `s` matches no variant's
    /// [`Self::label`].
    ///
    /// The direct `(&str → usize` lex-order position) projection through
    /// the closed set — the fourth (and final) direct edge of the (typed
    /// variant, `&'static str` label, `usize` position) projection
    /// triangle on the LEX ordering axis. Sibling posture to
    /// [`Self::index_of_label`] one ordering-axis over on the
    /// (declaration, lex) partition of the (`&str` → `usize` position)
    /// forward-projection surface: [`Self::index_of_label`] projects a
    /// `&str` label onto its declaration-order slot through
    /// [`Self::find_by_label`] + [`Self::index_of`], this method projects
    /// the same `&str` label onto its lexicographic-order slot through
    /// [`Self::find_by_label`] + [`Self::sorted_index_of`]. Together the
    /// two direct projections bracket both ordering axes at the (`&str`
    /// → `usize` position) forward edge — every generic consumer that
    /// slots a per-label incoming payload into a canonical
    /// alphabetical-order slot (a lex-sorted per-label metrics binner
    /// that keys `counters[T::sorted_index_of_label(label)?]` on a diagnostic
    /// input string, a lex-order compact wire-format encoder that reads
    /// a `&str` config value and emits its canonical alphabetic slot
    /// directly as a `u8` without materializing the typed variant at the
    /// encoder site, a `tatara-check` per-lex-slot per-label diagnostic
    /// that partitions a batch of incoming labels by lex-order slot
    /// under the natural `[per_slot; T::CARDINALITY]` aggregation shape,
    /// an LSP-hover / config-decoder rendering that maps parsed labels
    /// back to their canonical alphabetic slot ordering for stable
    /// lex-order rendering) binds to the lex-ordering-axis surface at
    /// ONE trait method rather than routing through a
    /// `find_by_label(s).map(sorted_index_of)` composition or a
    /// `sorted_labels().iter().position(|l| l == s)` scan at every
    /// callsite.
    ///
    /// The (ordering-axis × direct-projection) partition post-lift:
    ///
    /// | Ordering axis        | (`&str` → position) projection surface     |
    /// |----------------------|--------------------------------------------|
    /// | Declaration order    | [`Self::index_of_label`]                   |
    /// | Lexicographic order  | [`Self::sorted_index_of_label`]            |
    ///
    /// Closes the fourth (and final) direct edge of the lex-axis
    /// projection triangle — [`Self::sorted_index_of`] closed the first
    /// direct edge (variant → lex position), [`Self::from_sorted_index`]
    /// closed the second (lex position → variant),
    /// [`Self::sorted_label_at`] closed the third (lex position →
    /// `&'static str` label), this method closes the fourth (`&str`
    /// label → lex position). The (typed variant, `&'static str` label,
    /// `usize` position) projection triangle now stays direct-projection
    /// closed at EVERY (input, output) pair on BOTH the declaration and
    /// lexicographic ordering axes — twelve directed edges total
    /// (6 per ordering axis × 2 ordering axes) each bind to ONE typed
    /// trait method with no two-step composition at the call site.
    ///
    /// Default body composes [`Self::find_by_label`] with
    /// [`Self::sorted_index_of`] verbatim — the `&str → usize` lex-slot
    /// shape is a typed CONSEQUENCE of the two pre-existing primitives,
    /// not a third codepath. Sibling posture to
    /// [`Self::index_of_label`]'s `find_by_label(s).map(index_of)` shape
    /// one ordering-axis over: both bind the natural `&str`-carrier
    /// forward-slot decode through the same [`Self::find_by_label`]
    /// primitive on the label-decode column and diverge only at the
    /// terminal (variant → `usize`) projection they compose with — the
    /// declaration-order axis threads [`Self::index_of`], the
    /// lexicographic-order axis threads [`Self::sorted_index_of`].
    /// Implementors override only when the composition needs to diverge
    /// from the natural `find_by_label(s).map(sorted_index_of)` shape
    /// — a typed escape hatch the trait surface exposes (same axis as
    /// `via` / `set_label` / `labels` / `sorted_labels` /
    /// `sorted_variants` / `from_index` / `index_of` / `label_at` /
    /// `index_of_label` / `from_sorted_index` / `sorted_label_at` /
    /// `sorted_index_of` overrides). An implementor that overrides
    /// [`Self::find_by_label`] (a future perfect-hash label-decoder, a
    /// future canonicalization-aware label projection that folds case
    /// or whitespace) OR [`Self::sorted_index_of`] (a future
    /// `#[closed_set(compare_labels_with = ...)]` derive attribute that
    /// swaps the ordering, a future const-fn lex-position projection)
    /// propagates the override through this default body to the
    /// direct-lex-slot projection automatically; the lex-axis projection
    /// triangle funnels every `&str`-carrier lex-decode through ONE
    /// typed primitive on each of its (variant, `usize` lex position)
    /// return-projection columns.
    ///
    /// The rejection contract — a non-canonical `&str` returns [`None`],
    /// the empty-string boundary that clause (4) reserves as structurally
    /// outside the closed set returns [`None`] — is guaranteed by the
    /// default composition through [`Self::find_by_label`]'s
    /// `Option`-typed sweep; the well-formedness contract
    /// [`assert_closed_set_well_formed`]'s new clause (25) pins the
    /// both-directions equality against the natural composition on
    /// every implementor, so a passing well-formedness sweep means
    /// every generic consumer can call `sorted_index_of_label` on any
    /// `&str` payload and expect the same `Option`-typed answer at
    /// every crate boundary.
    ///
    /// Future consumers — a lex-sorted per-label metrics binner that
    /// reads a diagnostic label from an incoming trace event and
    /// increments `counters[T::sorted_index_of_label(label)?]` under
    /// the lex-sorted per-slot aggregation shape without a two-step
    /// (`find_by_label` → `sorted_index_of`) composition at each
    /// rendering site, a lex-order compact wire-format encoder that
    /// reads a `&str` config value (a Kubernetes annotation, a YAML
    /// enum field, a diagnostic input string) and emits its canonical
    /// alphabetic slot directly as a `u8` without materializing the
    /// typed variant at the encoder site (a legal / regulatory contract
    /// that pins the byte-order semantics on the CANONICAL alphabetic
    /// order rather than declaration order — the two are structurally
    /// distinct when the closed set's canonical ordering is defined by
    /// alphabetic order), a `tatara-check` per-lex-slot per-label
    /// diagnostic that partitions a batch of incoming labels by
    /// lex-order slot for reporting under the natural
    /// `[per_slot; T::CARDINALITY]` aggregation shape, an LSP-hover /
    /// config-decoder rendering that maps parsed labels back to their
    /// canonical alphabetic slot ordering for stable lex-order
    /// rendering — bind to ONE trait method instead of hand-rolling
    /// either `T::find_by_label(s).map(T::sorted_index_of)` (which
    /// re-derives the same two-primitive composition at every callsite
    /// AND makes every downstream site depend on
    /// [`Self::find_by_label`]'s `Option`-typed dispatch shape) OR the
    /// inline `T::sorted_labels().iter().position(|l| l == &s)` (which
    /// pays a `Vec<&'static str>` allocation the label-keyed find
    /// doesn't need AND re-derives the underlying three-primitive
    /// composition at every callsite, AND drops the
    /// [`Self::find_by_label`] override propagation that keeps every
    /// carrier-decode consumer aligned on a single typed dispatch) at
    /// each callsite, and the closed-set `(&str → lex position)` direct
    /// projection surface evolves at ONE site rather than per-consumer.
    ///
    /// THEORY.md §III — the typescape; the (`&'static str` label →
    /// `usize` lex-order position) projection becomes a TYPE projection
    /// on the trait rather than a per-consumer inline
    /// `Self::find_by_label(s).map(Self::sorted_index_of)` composition
    /// at every downstream label-to-lex-slot site. The projection
    /// triangle over the closed-set carriers gains its final direct
    /// edge on the lex ordering axis — every (input, output) pair
    /// across the (typed variant, `&'static str` label, `usize`
    /// position) carriers now binds to ONE typed projection surface at
    /// BOTH ordering axes (declaration and lexicographic) with no
    /// two-step composition at the call site.
    /// THEORY.md §V.1 — knowable platform; the (`&str` → `usize` lex
    /// position) direct projection was an unnamed compound of
    /// [`Self::find_by_label`] composed with [`Self::sorted_index_of`]
    /// pre-lift; naming it on the trait makes the projection a TYPED
    /// CONSEQUENCE of the two substrate primitives — generic consumers
    /// see ONE method, not ONE label-to-lex-slot-shape-per-crate. The
    /// well-formedness clause (25) pins the composition against the
    /// natural `find_by_label(s).map(sorted_index_of)` shape on every
    /// implementor so a passing well-formedness sweep means every
    /// generic consumer can call `sorted_index_of_label` on any `&str`
    /// payload and expect the same `Option`-typed answer at every
    /// crate boundary.
    /// THEORY.md §VI.1 — generation over composition; the direct
    /// (`&str → lex position`) projection emerges from the composition
    /// of TWO substrate primitives ([`Self::find_by_label`],
    /// [`Self::sorted_index_of`]) rather than as a per-implementor
    /// inline `find_by_label(s).map(sorted_index_of)` compound. A future
    /// tightening of either primitive (a future perfect-hash
    /// `find_by_label`, a future canonicalization-aware label
    /// projection that folds case / whitespace, a future const-fn
    /// `sorted_index_of` axis that makes the projection compile-time
    /// visible, a future `#[closed_set(compare_labels_with = ...)]`
    /// derive attribute that swaps the ordering) propagates to every
    /// closed-set direct-lex-slot-projection consumer through ONE trait
    /// body.
    ///
    /// Frontier inspiration: Racket's `(enum-sort-index enum sym)` on a
    /// closed enum projects a symbol directly onto its lexicographic-
    /// order position under the chosen ordering — the label-keyed lex-
    /// slot forward decoder the counterpart to `(enum-index enum sym)`
    /// on the declaration axis. MLIR's `RegisteredOperationName::
    /// getStableIndexByName(StringRef name)` on the lex-sorted Op
    /// registry composes the (name → op) lookup with the (op → stable
    /// lex index) projection into ONE direct `(name → lex index)`
    /// surface the DiagnosticEngine's per-lex-slot counters key off.
    /// Haskell's `Data.List.elemIndex sym (sortBy comparingLabel
    /// labels)` composes the sort with the label-keyed find into the
    /// same `(&str → lex position)` shape one vocabulary over. Idris's
    /// `Data.List.findIndex ((sym ==) . label)` composed with a lex-
    /// sorted `sortBy` prelude on the `Fin n` finite-cardinality
    /// universe delivers the same shape one vocabulary over on the
    /// dependent-type side. Translation through pleme-io primitives:
    /// a pure default method composing the trait's existing
    /// [`Self::find_by_label`] + [`Self::sorted_index_of`] surfaces —
    /// no new dep, no new IR layer, no supertrait bound, no allocation.
    fn sorted_index_of_label(s: &str) -> Option<usize> {
        <Self as ClosedSet>::find_by_label(s).map(<Self as ClosedSet>::sorted_index_of)
    }

    /// The declaration-order neighbor immediately AFTER `self` in
    /// [`Self::ALL`] — `Some(Self::ALL[self.index_of() + 1])` when
    /// `self` is not the tail, [`None`] otherwise.
    ///
    /// The forward-direction arm of the (forward, backward) neighbor
    /// axis over the closed set's declaration-order chain. Together
    /// with [`Self::prev`] the pair closes the (endpoint = 0,
    /// endpoint = `CARDINALITY - 1`) partition of the neighbor
    /// surface — every generic consumer that walks the closed set as
    /// a bounded chain (a state-machine iterator that steps
    /// [`Self::first`] → [`Self::last`] one variant at a time, a
    /// wraparound-cursor renderer that highlights the "next choice"
    /// in an LSP completion list, a truth-table property test that
    /// exercises adjacent-variant transitions, a signal-fold reducer
    /// that walks the chain accumulating state) binds to ONE typed
    /// neighbor method rather than hand-rolling either
    /// `Self::from_index(self.index_of() + 1)` (which re-derives the
    /// same two-primitive composition at every callsite AND makes
    /// every downstream site depend on the `+ 1` arithmetic) OR a
    /// per-implementor inline `match self { A => Some(B), B =>
    /// Some(C), C => None }` (which re-derives the per-variant
    /// neighbor table at every callsite AND drifts silently when
    /// [`Self::ALL`] gains a new variant).
    ///
    /// Sibling posture to [`Self::last`] on the (forward-neighbor,
    /// tail-endpoint) axis of the closed-set traversal surface —
    /// `T::last().next() == None` is the natural fixpoint the
    /// forward-neighbor axis and the tail-endpoint anchor share.
    /// Sibling posture to [`Self::prev`] one axis over on the
    /// (forward, backward) direction partition of the neighbor
    /// surface: this method returns the declaration-order successor,
    /// [`Self::prev`] returns the declaration-order predecessor. The
    /// (endpoint × direction) 2×2 matrix over the closed-set
    /// traversal surface partitions post-lift:
    ///
    /// | Direction \\ Boundary     | Interior             | Boundary         |
    /// |---------------------------|----------------------|------------------|
    /// | Forward (declaration)     | [`Self::next`]       | [`Self::last`]   |
    /// | Backward (declaration)    | [`Self::prev`]       | [`Self::first`]  |
    ///
    /// Default body composes [`Self::index_of`] with
    /// [`Self::from_index`] verbatim — the neighbor projection is a
    /// typed CONSEQUENCE of the pre-existing (variant ↔ `usize`
    /// array index) bijection, not a third codepath. Implementors
    /// override only when the neighbor surface needs to diverge from
    /// the natural `from_index(index_of(self) + 1)` shape (no
    /// production implementor reaches for this today; the axis
    /// exists for the same reason `via` / `set_label` / `labels` /
    /// `from_index` / `first` / `last` overrides exist — a typed
    /// escape hatch the trait surface exposes rather than forcing
    /// the implementor to hand-roll the impl). An implementor that
    /// overrides [`Self::from_index`] propagates the override through
    /// this default body automatically; the (variant → variant)
    /// forward-neighbor projection funnels through ONE typed
    /// primitive.
    ///
    /// The bounded-neighbor contract — the tail arm returns [`None`]
    /// for [`Self::last`] — is guaranteed by the default composition
    /// through [`Self::from_index`]'s `<[T]>::get` slice-bounded
    /// projection; the well-formedness contract
    /// [`assert_closed_set_well_formed`]'s new clause (26) pins the
    /// composition against the natural
    /// `from_index(index_of(self) + 1)` shape AND the tail-endpoint
    /// `None` guard on every implementor, so a passing well-
    /// formedness sweep means every generic consumer can call
    /// [`Self::next`] on any typed variant and expect the same
    /// [`Option`]-typed answer at every crate boundary.
    ///
    /// Future consumers — a state-machine iterator that walks
    /// [`Self::first`] → [`Self::last`] one variant at a time via
    /// `let mut cur = T::first(); while let Some(v) = cur.next() { ...
    /// cur = v; }` without threading either `T::ALL`'s slice-index
    /// API OR a per-variant `match` block through the iterator body,
    /// a wraparound-cursor renderer that highlights the "next choice"
    /// in an LSP completion list by composing `self.next().unwrap_or(
    /// T::first())` — the wraparound is a typed CONSEQUENCE of the
    /// bounded-neighbor axis, an implementor's saga-step engine that
    /// advances phase-by-phase through the workload lifecycle by
    /// binding each phase's forward transition to [`Self::next`], a
    /// truth-table property test that exercises adjacent-variant
    /// transitions without re-deriving the (variant → next-variant)
    /// mapping at each callsite, a signal-fold reducer that walks
    /// the chain accumulating state per-neighbor — bind to ONE trait
    /// method instead of hand-rolling either the
    /// `T::from_index(self.index_of() + 1)` composition (which
    /// re-derives the same two-primitive composition at every
    /// callsite AND makes every downstream site depend on the
    /// arithmetic) OR the inline `T::ALL.get(self.index_of() +
    /// 1).copied()` (which re-derives the underlying three-primitive
    /// composition at every callsite) at each callsite, and the
    /// closed-set forward-neighbor projection surface evolves at ONE
    /// site rather than per-consumer.
    ///
    /// THEORY.md §III — the typescape; the (variant → forward
    /// neighbor) projection becomes a TYPE projection on the trait
    /// rather than a per-consumer inline
    /// `Self::from_index(self.index_of() + 1)` composition at every
    /// downstream traversal site. The (forward, backward) direction
    /// axis of the closed-set traversal surface partitions
    /// exhaustively into TWO typed projections, each with a distinct
    /// load-bearing consumer surface — forward walk for
    /// [`Self::next`], backward walk for [`Self::prev`].
    /// THEORY.md §V.1 — knowable platform; the (variant → forward
    /// neighbor) projection was an unnamed compound of
    /// [`Self::index_of`] + [`Self::from_index`] + `+ 1` arithmetic
    /// pre-lift; naming it on the trait makes the projection a TYPED
    /// CONSEQUENCE of the two substrate primitives — generic
    /// consumers see ONE method, not one forward-neighbor-shape-per-
    /// crate. The well-formedness clause (26) pins the composition
    /// against the natural `from_index(index_of(self) + 1)` shape AND
    /// the tail-endpoint `None` guard on every implementor so a
    /// passing sweep means every generic consumer can call
    /// [`Self::next`] on any typed variant and expect the same
    /// [`Option`]-typed answer at every crate boundary.
    /// THEORY.md §VI.1 — generation over composition; the (variant
    /// → forward neighbor) projection emerges from the composition
    /// of TWO substrate primitives ([`Self::index_of`],
    /// [`Self::from_index`]) via `usize` `+ 1` arithmetic rather
    /// than as a per-implementor `match self { A => Some(B), B =>
    /// Some(C), C => None }` block. A future tightening of either
    /// primitive (a future perfect-hash `from_index`, a future
    /// const-fn `index_of` axis that makes the projection callable
    /// in const contexts) propagates to every closed-set forward-
    /// neighbor consumer through this method's body.
    ///
    /// Frontier inspiration: Racket's `enum-next` on closed
    /// enumerations (the direct-neighbor projection on the
    /// declaration-order chain); Idris's `Fin n` finite-cardinality
    /// type's `weakenN` / `strengthenN` neighbor operators on the
    /// non-empty finite-type universe; Haskell's `succ` on the
    /// `Bounded + Enum` type-class pair (which panics at the tail
    /// endpoint rather than returning an `Option`, one design
    /// decision away — this method takes the `Option`-typed panic-
    /// free arm); MLIR's `RegisteredOperationName::next()` on the
    /// declaration-order Op registry; Rust's `strum::EnumIter` /
    /// `strum::IntoEnumIterator::iter().skip_while(|v| *v != self)
    /// .nth(1)` composed through the iterator API. Translation
    /// through pleme-io primitives: a pure default method composing
    /// the trait's existing [`Self::index_of`] +
    /// [`Self::from_index`] surfaces via `usize` `+ 1` arithmetic —
    /// no new dep, no new IR layer, no supertrait bound, no panic on
    /// the tail-endpoint boundary.
    fn next(self) -> Option<Self> {
        <Self as ClosedSet>::from_index(<Self as ClosedSet>::index_of(self) + 1)
    }

    /// The declaration-order neighbor immediately BEFORE `self` in
    /// [`Self::ALL`] — `Some(Self::ALL[self.index_of() - 1])` when
    /// `self` is not the head, [`None`] otherwise.
    ///
    /// Sibling posture to [`Self::next`] one axis over on the
    /// (forward, backward) direction partition of the closed-set
    /// neighbor surface: [`Self::next`] returns the declaration-order
    /// successor, this method returns the declaration-order
    /// predecessor. See [`Self::next`] for the shared design
    /// rationale, sibling matrix, override axis, future-consumer
    /// inventory, THEORY.md grounding, and frontier inspiration —
    /// this method is the backward-direction arm of the same axis
    /// and inherits every property from the forward arm's
    /// documentation, differing only in the `- 1` arithmetic and the
    /// head-endpoint underflow guard.
    ///
    /// Default body composes [`Self::index_of`] with
    /// [`Self::from_index`] under a `usize` `- 1` subtraction guarded
    /// on `index_of(self) > 0` — the head-endpoint arm returns
    /// [`None`] BEFORE the subtraction is attempted, so the `usize`
    /// arithmetic never underflows. Implementors override only when
    /// the neighbor surface needs to diverge from the natural
    /// `from_index(index_of(self) - 1)` shape (no production
    /// implementor reaches for this today; the axis exists for the
    /// same reason `via` / `set_label` / `labels` / `from_index` /
    /// `first` / `last` / `next` overrides exist — a typed escape
    /// hatch rather than forcing the implementor to hand-roll the
    /// impl). An implementor that overrides [`Self::from_index`]
    /// propagates the override through this default body
    /// automatically; the (variant → variant) backward-neighbor
    /// projection funnels through ONE typed primitive.
    ///
    /// The bounded-neighbor contract — the head arm returns [`None`]
    /// for [`Self::first`] — is guaranteed by the explicit
    /// `index_of(self) == 0` guard; the well-formedness contract
    /// [`assert_closed_set_well_formed`]'s new clause (26) pins the
    /// composition against the natural
    /// `from_index(index_of(self) - 1)` shape on interior variants
    /// AND the head-endpoint `None` guard on every implementor, so a
    /// passing well-formedness sweep means every generic consumer
    /// can call [`Self::prev`] on any typed variant and expect the
    /// same [`Option`]-typed answer at every crate boundary.
    /// `T::first().prev() == None` is the natural fixpoint the
    /// backward-neighbor axis and the head-endpoint anchor share,
    /// mirroring the `T::last().next() == None` fixpoint on the
    /// forward-neighbor / tail-endpoint pair.
    fn prev(self) -> Option<Self> {
        let i = <Self as ClosedSet>::index_of(self);
        if i == 0 {
            None
        } else {
            <Self as ClosedSet>::from_index(i - 1)
        }
    }

    /// The lexicographic-order neighbor immediately AFTER `self` in
    /// [`Self::sorted_variants`] — `Some(Self::sorted_variants()[
    /// self.sorted_index_of() + 1])` when `self` is not the lex-tail,
    /// [`None`] otherwise.
    ///
    /// The forward-direction arm of the (forward, backward) neighbor
    /// axis over the closed set's LEX-order chain — one ordering-axis
    /// over from [`Self::next`], which walks the DECLARATION-order chain.
    /// Together with [`Self::sorted_prev`], the pair closes the
    /// (lex-endpoint = 0, lex-endpoint = `CARDINALITY - 1`) partition of
    /// the lex-order neighbor surface, and together with the pre-existing
    /// [`Self::next`] / [`Self::prev`] pair on the declaration axis
    /// completes the (declaration × lex) × (forward, backward) 2×2
    /// closed-set neighbor matrix:
    ///
    /// | Direction \\ Ordering axis | Declaration order  | Lexicographic order  |
    /// |----------------------------|--------------------|----------------------|
    /// | Forward                    | [`Self::next`]     | [`Self::sorted_next`] |
    /// | Backward                   | [`Self::prev`]     | [`Self::sorted_prev`] |
    ///
    /// Every generic consumer that walks the closed set as a bounded
    /// chain under lexicographic order (an alphabetized-completion LSP
    /// cursor that steps [`Self::sorted_first`] → [`Self::sorted_last`]
    /// one lex slot at a time, a lex-sorted `tatara-check` per-slot
    /// diagnostic renderer that binds `slot.sorted_next()` as its
    /// forward-traversal surface, a lex-order compact-encoded wire
    /// codec that walks slot-by-slot from the head, a Sekiban audit
    /// binner that iterates observed-slot lex neighbors, an
    /// alphabetized property-test sweep that exercises adjacent-
    /// lex-slot transitions) binds to ONE typed lex-neighbor method
    /// rather than hand-rolling either
    /// `Self::from_sorted_index(self.sorted_index_of() + 1)` (which
    /// re-derives the same two-primitive composition at every callsite
    /// AND makes every downstream site depend on the `+ 1` arithmetic)
    /// OR a per-implementor inline `match self { ... }` keyed on the
    /// lex slot (which re-derives the per-variant lex-neighbor table
    /// at every callsite AND drifts silently when [`Self::ALL`] gains
    /// a new variant that reorders the lex partition).
    ///
    /// Sibling posture to [`Self::sorted_last`] on the (forward-neighbor,
    /// lex-tail-endpoint) axis of the lex-order traversal surface —
    /// `T::sorted_last().sorted_next() == None` is the natural fixpoint
    /// the forward-lex-neighbor axis and the lex-tail-endpoint anchor
    /// share, mirroring the `T::last().next() == None` fixpoint on the
    /// declaration axis.
    ///
    /// Default body composes [`Self::sorted_index_of`] with
    /// [`Self::from_sorted_index`] verbatim — the lex-neighbor
    /// projection is a typed CONSEQUENCE of the pre-existing (variant
    /// ↔ `usize` lex-order position) bijection, not a third codepath.
    /// Implementors override only when the lex-neighbor surface needs
    /// to diverge from the natural
    /// `from_sorted_index(sorted_index_of(self) + 1)` shape (no
    /// production implementor reaches for this today; the axis exists
    /// for the same reason `via` / `set_label` / `labels` /
    /// `sorted_index_of` / `from_sorted_index` / `sorted_first` /
    /// `sorted_last` / `next` / `prev` overrides exist — a typed escape
    /// hatch the trait surface exposes rather than forcing the
    /// implementor to hand-roll the impl). An implementor that
    /// overrides [`Self::from_sorted_index`] propagates the override
    /// through this default body automatically; the (variant → variant)
    /// forward-lex-neighbor projection funnels through ONE typed
    /// primitive.
    ///
    /// The bounded-neighbor contract — the lex-tail arm returns
    /// [`None`] for [`Self::sorted_last`] — is guaranteed by the
    /// default composition through [`Self::from_sorted_index`]'s
    /// bounded projection; the well-formedness contract
    /// [`assert_closed_set_well_formed`]'s new clause (27) pins the
    /// composition against the natural
    /// `from_sorted_index(sorted_index_of(self) + 1)` shape AND the
    /// lex-tail-endpoint `None` guard on every implementor, so a
    /// passing well-formedness sweep means every generic consumer can
    /// call [`Self::sorted_next`] on any typed variant and expect the
    /// same [`Option`]-typed answer at every crate boundary.
    ///
    /// THEORY.md §III — the typescape; the (variant → forward
    /// lex-neighbor) projection becomes a TYPE projection on the trait
    /// rather than a per-consumer inline
    /// `Self::from_sorted_index(self.sorted_index_of() + 1)` composition
    /// at every downstream lex-order traversal site. The (declaration,
    /// lex) × (forward, backward) 2×2 neighbor matrix over the
    /// closed-set traversal surface partitions exhaustively into FOUR
    /// typed projections, each with a distinct load-bearing consumer
    /// surface.
    /// THEORY.md §V.1 — knowable platform; the (variant → forward
    /// lex-neighbor) projection was an unnamed compound of
    /// [`Self::sorted_index_of`] + [`Self::from_sorted_index`] + `+ 1`
    /// arithmetic pre-lift; naming it on the trait makes the
    /// projection a TYPED CONSEQUENCE of the two lex-axis substrate
    /// primitives — generic consumers see ONE method, not one
    /// lex-forward-neighbor-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the (variant →
    /// forward lex-neighbor) projection emerges from the composition
    /// of TWO substrate primitives ([`Self::sorted_index_of`],
    /// [`Self::from_sorted_index`]) via `usize` `+ 1` arithmetic rather
    /// than as a per-implementor `match self { ... }` block. A future
    /// tightening of either primitive (a future perfect-hash
    /// `from_sorted_index`, a future
    /// `#[closed_set(compare_labels_with = ...)]` derive attribute
    /// that swaps the ordering) propagates to every closed-set
    /// forward-lex-neighbor consumer through this method's body.
    ///
    /// Frontier inspiration: Racket's `(enum-next enum sym #:order
    /// 'lex)` on a closed enumeration under the lexicographic ordering;
    /// Idris's `Fin n` with a lex-sorted labeling projection composed
    /// through `weakenN` / `strengthenN` on the lex partition; MLIR's
    /// `RegisteredOperationName::nextByLexicalOrder()` on the
    /// lex-sorted Op registry; Haskell's `succ` composed with a
    /// `sortBy comparingLabel` prelude on a `Bounded + Enum` type-class
    /// pair; Rust's `strum::EnumIter` composed through a sorted-by-label
    /// prelude with `.skip_while(|v| *v != self).nth(1)`. Translation
    /// through pleme-io primitives: a pure default method composing the
    /// trait's existing [`Self::sorted_index_of`] +
    /// [`Self::from_sorted_index`] surfaces via `usize` `+ 1`
    /// arithmetic — no new dep, no new IR layer, no supertrait bound,
    /// no panic on the lex-tail-endpoint boundary, no `strum` /
    /// `enum-iterator` crate dependency.
    fn sorted_next(self) -> Option<Self> {
        <Self as ClosedSet>::from_sorted_index(<Self as ClosedSet>::sorted_index_of(self) + 1)
    }

    /// The lexicographic-order neighbor immediately BEFORE `self` in
    /// [`Self::sorted_variants`] — `Some(Self::sorted_variants()[
    /// self.sorted_index_of() - 1])` when `self` is not the lex-head,
    /// [`None`] otherwise.
    ///
    /// Sibling posture to [`Self::sorted_next`] one axis over on the
    /// (forward, backward) direction partition of the lex-order
    /// neighbor surface: [`Self::sorted_next`] returns the lex-order
    /// successor, this method returns the lex-order predecessor. See
    /// [`Self::sorted_next`] for the shared design rationale, sibling
    /// 2×2 matrix, override axis, future-consumer inventory, THEORY.md
    /// grounding, and frontier inspiration — this method is the
    /// backward-direction arm of the same lex-axis and inherits every
    /// property from the forward arm's documentation, differing only
    /// in the `- 1` arithmetic and the lex-head-endpoint underflow
    /// guard.
    ///
    /// Default body composes [`Self::sorted_index_of`] with
    /// [`Self::from_sorted_index`] under a `usize` `- 1` subtraction
    /// guarded on `sorted_index_of(self) > 0` — the lex-head-endpoint
    /// arm returns [`None`] BEFORE the subtraction is attempted, so
    /// the `usize` arithmetic never underflows. Implementors override
    /// only when the lex-neighbor surface needs to diverge from the
    /// natural `from_sorted_index(sorted_index_of(self) - 1)` shape
    /// (no production implementor reaches for this today; the axis
    /// exists for the same reason `via` / `set_label` / `labels` /
    /// `from_sorted_index` / `sorted_first` / `sorted_last` / `next` /
    /// `prev` / `sorted_next` overrides exist — a typed escape hatch
    /// rather than forcing the implementor to hand-roll the impl). An
    /// implementor that overrides [`Self::from_sorted_index`]
    /// propagates the override through this default body automatically;
    /// the (variant → variant) backward-lex-neighbor projection funnels
    /// through ONE typed primitive.
    ///
    /// The bounded-neighbor contract — the lex-head arm returns
    /// [`None`] for [`Self::sorted_first`] — is guaranteed by the
    /// explicit `sorted_index_of(self) == 0` guard; the well-formedness
    /// contract [`assert_closed_set_well_formed`]'s new clause (27)
    /// pins the composition against the natural
    /// `from_sorted_index(sorted_index_of(self) - 1)` shape on interior
    /// lex slots AND the lex-head-endpoint `None` guard on every
    /// implementor, so a passing well-formedness sweep means every
    /// generic consumer can call [`Self::sorted_prev`] on any typed
    /// variant and expect the same [`Option`]-typed answer at every
    /// crate boundary. `T::sorted_first().sorted_prev() == None` is the
    /// natural fixpoint the backward-lex-neighbor axis and the
    /// lex-head-endpoint anchor share, mirroring the
    /// `T::sorted_last().sorted_next() == None` fixpoint on the
    /// forward-lex-neighbor / lex-tail-endpoint pair AND the
    /// `T::first().prev() == None` fixpoint one ordering axis over.
    fn sorted_prev(self) -> Option<Self> {
        let i = <Self as ClosedSet>::sorted_index_of(self);
        if i == 0 {
            None
        } else {
            <Self as ClosedSet>::from_sorted_index(i - 1)
        }
    }

    /// The declaration-order neighbor immediately AFTER `self` in
    /// [`Self::ALL`], WRAPPING to [`Self::first`] at the tail —
    /// `self.next().unwrap_or(Self::first())`. Returns [`Self`],
    /// never [`Option<Self>`]: the wrapping arm folds the tail-
    /// endpoint boundary onto the head-endpoint anchor rather than
    /// leaving the [`None`] the bounded-neighbor axis returns.
    ///
    /// The wrapping-return arm of the (Option-typed, wrapping)
    /// partition over the closed-set forward-neighbor surface — one
    /// return-type axis over from [`Self::next`], which returns the
    /// bounded [`Option<Self>`] variant. Together with
    /// [`Self::cycle_prev`], the pair closes the (forward, backward)
    /// direction axis of the WRAPPING arm on the declaration ordering
    /// axis, and together with the pre-existing [`Self::next`] /
    /// [`Self::prev`] pair opens the (Option-typed, wrapping) × (forward,
    /// backward) 2×2 matrix on the declaration-axis neighbor surface:
    ///
    /// | Return type \\ Direction    | Forward            | Backward           |
    /// |-----------------------------|--------------------|--------------------|
    /// | Option-typed (bounded)      | [`Self::next`]     | [`Self::prev`]     |
    /// | Wrapping (cyclic)           | [`Self::cycle_next`] | [`Self::cycle_prev`] |
    ///
    /// Every generic consumer that walks the closed set as an
    /// INFINITE cyclic chain under declaration order (a wraparound-
    /// cursor LSP completion renderer that steps through variants
    /// unconditionally without threading an `Option`-branch through
    /// the update path, a UI mode selector that "cycles to the next
    /// mode on Tab", a round-robin scheduler that walks a fixed pool
    /// of typed slots forever, a per-tick animation frame picker
    /// that advances one variant per tick and wraps at the tail, a
    /// declaration-order carousel widget) binds to ONE typed
    /// wrapping-neighbor method rather than hand-rolling either
    /// `self.next().unwrap_or(T::first())` (which re-derives the same
    /// two-primitive composition at every callsite AND makes every
    /// downstream site depend on the wrapping-fallback shape) OR
    /// `T::from_index((self.index_of() + 1) % T::CARDINALITY)` (which
    /// re-derives the modular-arithmetic composition at every callsite
    /// AND makes every downstream site depend on the `%` operator on
    /// `usize`) OR a per-implementor inline `match self { A => B, B
    /// => C, C => A }` keyed on the declaration slot (which re-derives
    /// the per-variant wraparound table at every callsite AND drifts
    /// silently when [`Self::ALL`] gains a new variant that reorders
    /// the wraparound edge).
    ///
    /// Sibling posture to [`Self::last`] on the (forward-neighbor,
    /// tail-endpoint) axis of the declaration-order traversal surface —
    /// `T::last().cycle_next() == T::first()` is the natural fixpoint
    /// the forward-wrapping-neighbor axis and the tail-endpoint anchor
    /// share, folding the tail-endpoint boundary onto the head-endpoint
    /// anchor at the shared structural landmark. Mirrors the
    /// `T::last().next() == None` fixpoint on the bounded arm one
    /// return-type axis over.
    ///
    /// Default body composes [`Self::next`] with [`Self::first`]
    /// through `Option::unwrap_or` — the wrapping-neighbor projection
    /// is a typed CONSEQUENCE of the pre-existing (bounded neighbor,
    /// head anchor) pair, not a third codepath. Implementors override
    /// only when the wrapping-neighbor surface needs to diverge from
    /// the natural `next().unwrap_or(first())` shape (no production
    /// implementor reaches for this today; the axis exists for the
    /// same reason `via` / `set_label` / `labels` / `from_index` /
    /// `first` / `last` / `next` / `prev` / `sorted_next` /
    /// `sorted_prev` overrides exist — a typed escape hatch the trait
    /// surface exposes rather than forcing the implementor to hand-
    /// roll the impl). An implementor that overrides [`Self::next`]
    /// or [`Self::first`] propagates the override through this default
    /// body automatically; the (variant → wrapping-forward-neighbor)
    /// projection funnels through TWO typed primitives.
    ///
    /// The wrapping-neighbor contract — the tail arm returns
    /// [`Self::first`] for [`Self::last`] — is guaranteed by the
    /// default composition through [`Self::next`]'s `None` at the tail
    /// AND [`Option::unwrap_or`]'s fallback semantics; the well-
    /// formedness contract [`assert_closed_set_well_formed`]'s new
    /// clause (28) pins the composition against the natural
    /// `next().unwrap_or(first())` shape AND the tail-endpoint
    /// `T::first()` fold on every implementor, so a passing well-
    /// formedness sweep means every generic consumer can call
    /// [`Self::cycle_next`] on any typed variant and expect the same
    /// [`Self`]-typed answer at every crate boundary.
    ///
    /// THEORY.md §III — the typescape; the (variant → wrapping-forward
    /// neighbor) projection becomes a TYPE projection on the trait
    /// rather than a per-consumer inline
    /// `self.next().unwrap_or(T::first())` composition at every
    /// downstream cyclic traversal site. The (Option-typed, wrapping)
    /// × (forward, backward) 2×2 matrix on the declaration-axis
    /// neighbor surface partitions exhaustively into FOUR typed
    /// projections, each with a distinct load-bearing consumer surface.
    /// THEORY.md §V.1 — knowable platform; the wrapping-neighbor
    /// projections were unnamed compounds of [`Self::next`] +
    /// [`Self::first`] + [`Option::unwrap_or`] pre-lift; naming them on
    /// the trait makes the projections TYPED CONSEQUENCES of the two
    /// bounded-arm primitives — generic consumers see ONE wrapping
    /// method per direction, not one wrapping-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the wrapping-
    /// neighbor projection emerges from the composition of TWO
    /// substrate primitives ([`Self::next`], [`Self::first`]) via
    /// [`Option::unwrap_or`] rather than as a per-implementor `match
    /// self { A => B, B => C, C => A }` block or a modular-arithmetic
    /// `T::from_index((self.index_of() + 1) % T::CARDINALITY)`
    /// composition. A future tightening of either primitive (a future
    /// perfect-hash `from_index` that speeds up `next`, a future
    /// `const fn first`) propagates to every closed-set wrapping-
    /// forward-neighbor consumer through this method's body.
    ///
    /// Frontier inspiration: Racket's `(enum-cycle-next enum sym)`
    /// on closed enumerations under a cyclic ordering (which folds
    /// the tail onto the head rather than returning `#f`); Idris's
    /// `Fin n` finite-cardinality type composed with modular
    /// arithmetic through `finToNat` / `natToFin` on the cyclic
    /// projection; Haskell's `succ` on the `Bounded + Enum` type-class
    /// pair wrapped in `catch` to fold `Prelude.succ: bad argument` at
    /// the tail-endpoint onto `minBound` (which reifies the wrapping
    /// arm as an exception-catching shim rather than a total function
    /// — this method takes the total-function arm); Emacs's
    /// `enum-next-cyclic`; UI toolkit "cycle-through-modes" bindings
    /// (Vim's `<Tab>` in command mode, Ctrl+Tab in editor mode).
    /// Translation through pleme-io primitives: a pure default method
    /// composing the trait's existing [`Self::next`] + [`Self::first`]
    /// surfaces via [`Option::unwrap_or`] — no new dep, no new IR
    /// layer, no supertrait bound, no `%` operator on `usize`, no
    /// exception-catching, no `strum` / `enum-iterator` crate
    /// dependency.
    fn cycle_next(self) -> Self {
        <Self as ClosedSet>::next(self).unwrap_or_else(<Self as ClosedSet>::first)
    }

    /// The declaration-order neighbor immediately BEFORE `self` in
    /// [`Self::ALL`], WRAPPING to [`Self::last`] at the head —
    /// `self.prev().unwrap_or(Self::last())`. Returns [`Self`], never
    /// [`Option<Self>`]: the wrapping arm folds the head-endpoint
    /// boundary onto the tail-endpoint anchor rather than leaving the
    /// [`None`] the bounded-neighbor axis returns.
    ///
    /// Sibling posture to [`Self::cycle_next`] one axis over on the
    /// (forward, backward) direction partition of the closed-set
    /// wrapping-neighbor surface: [`Self::cycle_next`] returns the
    /// declaration-order successor with tail-wrap-to-head,
    /// this method returns the declaration-order predecessor with
    /// head-wrap-to-tail. See [`Self::cycle_next`] for the shared
    /// design rationale, sibling 2×2 matrix, override axis, future-
    /// consumer inventory, THEORY.md grounding, and frontier
    /// inspiration — this method is the backward-direction arm of
    /// the same axis and inherits every property from the forward
    /// arm's documentation, differing only in the [`Self::prev`] /
    /// [`Self::last`] substrate primitives it composes.
    ///
    /// Default body composes [`Self::prev`] with [`Self::last`]
    /// through [`Option::unwrap_or`] — the wrapping-neighbor
    /// projection is a typed CONSEQUENCE of the pre-existing (bounded
    /// backward neighbor, tail anchor) pair, not a third codepath.
    /// Implementors override only when the wrapping-neighbor surface
    /// needs to diverge from the natural `prev().unwrap_or(last())`
    /// shape. An implementor that overrides [`Self::prev`] or
    /// [`Self::last`] propagates the override through this default
    /// body automatically; the (variant → wrapping-backward-neighbor)
    /// projection funnels through TWO typed primitives.
    ///
    /// The wrapping-neighbor contract — the head arm returns
    /// [`Self::last`] for [`Self::first`] — is guaranteed by the
    /// default composition through [`Self::prev`]'s `None` at the
    /// head AND [`Option::unwrap_or`]'s fallback semantics; the
    /// well-formedness contract
    /// [`assert_closed_set_well_formed`]'s new clause (28) pins the
    /// composition against the natural `prev().unwrap_or(last())`
    /// shape AND the head-endpoint `T::last()` fold on every
    /// implementor, so a passing well-formedness sweep means every
    /// generic consumer can call [`Self::cycle_prev`] on any typed
    /// variant and expect the same [`Self`]-typed answer at every
    /// crate boundary. `T::first().cycle_prev() == T::last()` is the
    /// natural fixpoint the backward-wrapping-neighbor axis and the
    /// head-endpoint anchor share, mirroring the
    /// `T::last().cycle_next() == T::first()` fixpoint on the
    /// forward-wrapping-neighbor / tail-endpoint pair AND the
    /// `T::first().prev() == None` fixpoint one return-type axis over.
    fn cycle_prev(self) -> Self {
        <Self as ClosedSet>::prev(self).unwrap_or_else(<Self as ClosedSet>::last)
    }

    /// The lexicographic-order neighbor immediately AFTER `self` in
    /// [`Self::sorted_variants`], WRAPPING to [`Self::sorted_first`] at
    /// the lex tail — `self.sorted_next().unwrap_or(Self::sorted_first())`.
    /// Returns [`Self`], never [`Option<Self>`]: the wrapping arm folds
    /// the lex-tail-endpoint boundary onto the lex-head-endpoint anchor
    /// rather than leaving the [`None`] the bounded-lex-neighbor axis
    /// returns.
    ///
    /// The wrapping-return arm of the (Option-typed, wrapping)
    /// partition over the closed-set forward-lex-neighbor surface — one
    /// return-type axis over from [`Self::sorted_next`], which returns
    /// the bounded [`Option<Self>`] variant. Together with
    /// [`Self::cycle_sorted_prev`], the pair closes the (forward,
    /// backward) direction axis of the WRAPPING arm on the lex ordering
    /// axis, and together with the pre-existing [`Self::cycle_next`] /
    /// [`Self::cycle_prev`] pair closes the (declaration, lex) ×
    /// (forward, backward) 2×2 matrix on the WRAPPING partition of the
    /// closed-set neighbor surface:
    ///
    /// | Ordering \\ Direction | Forward wrap                | Backward wrap               |
    /// |-----------------------|-----------------------------|-----------------------------|
    /// | Declaration           | [`Self::cycle_next`]        | [`Self::cycle_prev`]        |
    /// | Lex                   | [`Self::cycle_sorted_next`] | [`Self::cycle_sorted_prev`] |
    ///
    /// Every generic consumer that walks the closed set as an INFINITE
    /// cyclic chain under LEX order (an alphabetized LSP completion
    /// cursor that steps through variants unconditionally without
    /// threading an `Option`-branch through the update path, an
    /// alphabetized round-robin picker that cycles through variants in
    /// canonical name order rather than declaration order, a
    /// lex-sorted per-tick animation frame picker that advances one
    /// alphabetized variant per tick and wraps at the lex tail, an
    /// alphabetized carousel widget) binds to ONE typed
    /// wrapping-lex-neighbor method rather than hand-rolling either
    /// `self.sorted_next().unwrap_or(T::sorted_first())` (which re-derives
    /// the same two-primitive composition at every callsite AND makes
    /// every downstream site depend on the wrapping-fallback shape) OR
    /// `T::from_sorted_index((self.sorted_index_of() + 1) % T::CARDINALITY)`
    /// (which re-derives the modular-arithmetic composition at every
    /// callsite AND makes every downstream site depend on the `%`
    /// operator on `usize`) OR a per-implementor inline `match self { A
    /// => B, B => C, C => A }` keyed on the lex slot (which re-derives
    /// the per-variant lex-wraparound table at every callsite AND drifts
    /// silently when [`Self::ALL`] gains a new variant whose canonical
    /// label reorders the lex-wraparound edge).
    ///
    /// Sibling posture to [`Self::sorted_last`] on the (forward-lex-
    /// neighbor, lex-tail-endpoint) axis of the lex-order traversal
    /// surface — `T::sorted_last().cycle_sorted_next() ==
    /// T::sorted_first()` is the natural fixpoint the
    /// forward-wrapping-lex-neighbor axis and the lex-tail-endpoint
    /// anchor share, folding the lex-tail-endpoint boundary onto the
    /// lex-head-endpoint anchor at the shared structural landmark.
    /// Mirrors the `T::sorted_last().sorted_next() == None` fixpoint on
    /// the bounded lex-arm one return-type axis over AND the
    /// `T::last().cycle_next() == T::first()` fixpoint on the
    /// declaration-wrapping arm one ordering axis over.
    ///
    /// Default body composes [`Self::sorted_next`] with
    /// [`Self::sorted_first`] through `Option::unwrap_or` — the
    /// wrapping-lex-neighbor projection is a typed CONSEQUENCE of the
    /// pre-existing (bounded lex-neighbor, lex-head anchor) pair, not a
    /// third codepath. Implementors override only when the
    /// wrapping-lex-neighbor surface needs to diverge from the natural
    /// `sorted_next().unwrap_or(sorted_first())` shape (no production
    /// implementor reaches for this today; the axis exists for the
    /// same reason `via` / `set_label` / `labels` / `from_sorted_index` /
    /// `sorted_first` / `sorted_last` / `sorted_next` / `sorted_prev` /
    /// `cycle_next` / `cycle_prev` overrides exist — a typed escape
    /// hatch the trait surface exposes rather than forcing the
    /// implementor to hand-roll the impl). An implementor that overrides
    /// [`Self::sorted_next`] or [`Self::sorted_first`] propagates the
    /// override through this default body automatically; the
    /// (variant → wrapping-forward-lex-neighbor) projection funnels
    /// through TWO typed primitives.
    ///
    /// The wrapping-lex-neighbor contract — the lex-tail arm returns
    /// [`Self::sorted_first`] for [`Self::sorted_last`] — is guaranteed
    /// by the default composition through [`Self::sorted_next`]'s `None`
    /// at the lex tail AND [`Option::unwrap_or`]'s fallback semantics;
    /// the well-formedness contract [`assert_closed_set_well_formed`]'s
    /// new clause (29) pins the composition against the natural
    /// `sorted_next().unwrap_or(sorted_first())` shape AND the
    /// lex-tail-endpoint `T::sorted_first()` fold on every implementor,
    /// so a passing well-formedness sweep means every generic consumer
    /// can call [`Self::cycle_sorted_next`] on any typed variant and
    /// expect the same [`Self`]-typed answer at every crate boundary.
    ///
    /// THEORY.md §III — the typescape; the (variant → wrapping-forward
    /// lex-neighbor) projection becomes a TYPE projection on the trait
    /// rather than a per-consumer inline
    /// `self.sorted_next().unwrap_or(T::sorted_first())` composition at
    /// every downstream lex-cyclic-traversal site. The (declaration, lex)
    /// × (forward, backward) 2×2 matrix on the WRAPPING partition of the
    /// closed-set neighbor surface partitions exhaustively into FOUR
    /// typed projections, each with a distinct load-bearing consumer
    /// surface — closing the (Option-typed, wrapping) × (declaration,
    /// lex) × (forward, backward) 2×2×2 = 8-corner neighbor cube at
    /// EVERY corner.
    /// THEORY.md §V.1 — knowable platform; the wrapping-lex-neighbor
    /// projections were unnamed compounds of [`Self::sorted_next`] +
    /// [`Self::sorted_first`] + [`Option::unwrap_or`] pre-lift; naming
    /// them on the trait makes the projections TYPED CONSEQUENCES of the
    /// two lex-bounded-arm primitives — generic consumers see ONE
    /// wrapping method per direction per ordering axis, not one
    /// wrapping-shape-per-crate.
    /// THEORY.md §VI.1 — generation over composition; the wrapping-
    /// lex-neighbor projection emerges from the composition of TWO
    /// substrate primitives ([`Self::sorted_next`],
    /// [`Self::sorted_first`]) via [`Option::unwrap_or`] rather than as
    /// a per-implementor `match self { A => B, B => C, C => A }` block
    /// keyed on the lex slot or a modular-arithmetic
    /// `T::from_sorted_index((self.sorted_index_of() + 1) %
    /// T::CARDINALITY)` composition. A future tightening of either
    /// primitive (a future perfect-hash `from_sorted_index` that speeds
    /// up `sorted_next`, a future `const fn sorted_first`) propagates to
    /// every closed-set wrapping-forward-lex-neighbor consumer through
    /// this method's body.
    ///
    /// Frontier inspiration: Racket's `(sort-cycle-next enum sym)` on
    /// closed enumerations under a lex-cyclic ordering; Common Lisp's
    /// `SXHASH`-keyed lex-sorted enum walkers wrapped in a
    /// `handler-case` that folds the tail-endpoint condition onto the
    /// head-endpoint anchor (which reifies the wrapping arm as a
    /// condition-handler shim rather than a total function — this
    /// method takes the total-function arm); Idris's `Fin n`
    /// finite-cardinality type composed with a lex-ordering permutation
    /// through `finToNat` / `natToFin` on the cyclic projection; UI
    /// toolkit "cycle-through-alphabetized-modes" bindings (Ctrl+n in
    /// alphabetical mode selectors, alphabetized round-robin schedulers
    /// in TUI palette pickers). Translation through pleme-io primitives:
    /// a pure default method composing the trait's existing
    /// [`Self::sorted_next`] + [`Self::sorted_first`] surfaces via
    /// [`Option::unwrap_or`] — no new dep, no new IR layer, no
    /// supertrait bound, no `%` operator on `usize`, no
    /// condition-handling, no `strum` / `enum-iterator` crate dependency.
    fn cycle_sorted_next(self) -> Self {
        <Self as ClosedSet>::sorted_next(self).unwrap_or_else(<Self as ClosedSet>::sorted_first)
    }

    /// The lexicographic-order neighbor immediately BEFORE `self` in
    /// [`Self::sorted_variants`], WRAPPING to [`Self::sorted_last`] at
    /// the lex head — `self.sorted_prev().unwrap_or(Self::sorted_last())`.
    /// Returns [`Self`], never [`Option<Self>`]: the wrapping arm folds
    /// the lex-head-endpoint boundary onto the lex-tail-endpoint anchor
    /// rather than leaving the [`None`] the bounded-lex-neighbor axis
    /// returns.
    ///
    /// Sibling posture to [`Self::cycle_sorted_next`] one axis over on
    /// the (forward, backward) direction partition of the closed-set
    /// wrapping-lex-neighbor surface: [`Self::cycle_sorted_next`]
    /// returns the lex-order successor with lex-tail-wrap-to-lex-head,
    /// this method returns the lex-order predecessor with
    /// lex-head-wrap-to-lex-tail. See [`Self::cycle_sorted_next`] for
    /// the shared design rationale, sibling 2×2 matrix, override axis,
    /// future-consumer inventory, THEORY.md grounding, and frontier
    /// inspiration — this method is the backward-direction arm of the
    /// same axis and inherits every property from the forward arm's
    /// documentation, differing only in the [`Self::sorted_prev`] /
    /// [`Self::sorted_last`] substrate primitives it composes.
    ///
    /// Default body composes [`Self::sorted_prev`] with
    /// [`Self::sorted_last`] through [`Option::unwrap_or`] — the
    /// wrapping-lex-neighbor projection is a typed CONSEQUENCE of the
    /// pre-existing (bounded backward lex-neighbor, lex-tail anchor)
    /// pair, not a third codepath. Implementors override only when the
    /// wrapping-lex-neighbor surface needs to diverge from the natural
    /// `sorted_prev().unwrap_or(sorted_last())` shape. An implementor
    /// that overrides [`Self::sorted_prev`] or [`Self::sorted_last`]
    /// propagates the override through this default body automatically;
    /// the (variant → wrapping-backward-lex-neighbor) projection funnels
    /// through TWO typed primitives.
    ///
    /// The wrapping-lex-neighbor contract — the lex-head arm returns
    /// [`Self::sorted_last`] for [`Self::sorted_first`] — is guaranteed
    /// by the default composition through [`Self::sorted_prev`]'s `None`
    /// at the lex head AND [`Option::unwrap_or`]'s fallback semantics;
    /// the well-formedness contract [`assert_closed_set_well_formed`]'s
    /// new clause (29) pins the composition against the natural
    /// `sorted_prev().unwrap_or(sorted_last())` shape AND the
    /// lex-head-endpoint `T::sorted_last()` fold on every implementor,
    /// so a passing well-formedness sweep means every generic consumer
    /// can call [`Self::cycle_sorted_prev`] on any typed variant and
    /// expect the same [`Self`]-typed answer at every crate boundary.
    /// `T::sorted_first().cycle_sorted_prev() == T::sorted_last()` is
    /// the natural fixpoint the backward-wrapping-lex-neighbor axis and
    /// the lex-head-endpoint anchor share, mirroring the
    /// `T::sorted_last().cycle_sorted_next() == T::sorted_first()`
    /// fixpoint on the forward-wrapping-lex-neighbor / lex-tail-endpoint
    /// pair, the `T::sorted_first().sorted_prev() == None` fixpoint one
    /// return-type axis over, AND the `T::first().cycle_prev() ==
    /// T::last()` fixpoint one ordering axis over. Clauses (28) + (29)
    /// together close the (declaration, lex) × (forward, backward) 2×2
    /// wrapping-neighbor matrix at ALL FOUR wraparound fixpoints,
    /// completing the (Option-typed, wrapping) × (declaration, lex) ×
    /// (forward, backward) 2×2×2 = 8-corner neighbor cube alongside
    /// clauses (26) + (27) on the bounded partition.
    fn cycle_sorted_prev(self) -> Self {
        <Self as ClosedSet>::sorted_prev(self).unwrap_or_else(<Self as ClosedSet>::sorted_last)
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
/// 8. [`ClosedSet::labels_joined`] composes [`ClosedSet::labels`] with
///    [`slice::join`](https://doc.rust-lang.org/std/primitive.slice.html#method.join)
///    verbatim — the joined-candidate-list rendering every
///    diagnostic / metrics consumer routes through emits at ONE
///    trait body. The sweep walks three representative separators
///    (`"/"`, `", "`, `"|"`) so a drift in any one of the three
///    rendering surfaces (slash for the substrate's `INTENT_KIND_LIST`-
///    shaped production constants, comma-space for diagnostic
///    `expected one of: ...` shapes, pipe for grammar-style lists)
///    fails the testkit on every implementor. The default trait body
///    satisfies the clause for free; the assertion catches a future
///    implementor whose override returns a different join shape (a
///    different separator threading, a subset of labels) loudly
///    rather than silently bifurcating the candidate-list-as-string
///    rendering every consumer routes through.
/// 9. [`ClosedSet::sorted_labels`] composes [`ClosedSet::labels`] with
///    [`slice::sort_unstable`](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_unstable)
///    verbatim — the canonical-ordered candidate-list rendering every
///    `_all_is_unique_and_complete` per-implementor test (the 7+
///    `let mut sorted: Vec<&str> = T::ALL.iter().map(<via>).collect();
///    sorted.sort_unstable();` inline triples across `QuoteForm`,
///    `AtomKind`, `KwargPathKind`, `ExpectedKwargShape`, `SexpShape`,
///    `UnquoteForm`, `MacroDefHead`, …) routes through emits at ONE
///    trait body. The default trait body satisfies the clause for free;
///    the assertion catches a future implementor whose override returns
///    a different sort shape (a subset of labels, a different ordering,
///    declaration order instead of lexicographic) loudly rather than
///    silently bifurcating the canonical-ordered candidate-list surface
///    every LSP / `tatara-check` / metrics consumer routes through.
/// 10. [`ClosedSet::sorted_labels_joined`] composes
///     [`ClosedSet::sorted_labels`] with
///     [`slice::join`](https://doc.rust-lang.org/std/primitive.slice.html#method.join)
///     verbatim — the alphabetized joined-candidate-list rendering
///     every diagnostic / metrics consumer that wants a
///     lexicographic-ordered `expected one of: ...` shape routes
///     through emits at ONE trait body. The sweep walks the same three
///     representative separators clause (8) uses (`"/"`, `", "`, `"|"`)
///     so a drift in any one of the three rendering surfaces (slash
///     for ordering-independent production constants, comma-space for
///     natural-language alphabetized `expected one of: ...` shapes,
///     pipe for grammar-style alphabetized alternative lists) fails
///     the testkit on every implementor. The default trait body
///     satisfies the clause for free; the assertion catches a future
///     implementor whose override returns a different sort-then-join
///     shape (a different separator threading, a subset of labels,
///     declaration order instead of lexicographic) loudly rather than
///     silently bifurcating the alphabetized-candidate-list-as-string
///     rendering every LSP / `tatara-check` / metrics consumer routes
///     through.
/// 11. [`ClosedSet::contains_label`] composes [`ClosedSet::ALL`] +
///     [`ClosedSet::label`] with [`Iterator::any`] verbatim — the
///     pure-membership predicate every zero-allocation lint / filter /
///     gate consumer routes through emits at ONE trait body without
///     ever materializing the [`ClosedSet::Unknown`] carrier
///     [`ClosedSet::parse_label`] threads on rejection. The sweep
///     walks every variant's canonical label (expected `true`), the
///     reserved 38-char probe (expected `false` — the same probe
///     clauses (5) + (7) reserve as lexically distinct from every
///     plausible canonical label), and the empty-string boundary
///     (expected `false` matching clause (4)) so a drift in any of
///     the three natural predicate arms (a permissive override that
///     accepts non-canonical strings, a strict override that rejects
///     a canonical label, a subset-projection override that names
///     fewer labels than `Self::ALL.iter().map(label)` covers) fails
///     the testkit on every implementor. The default trait body
///     satisfies the clause for free; the assertion catches a future
///     implementor whose override drifts the composition loudly
///     rather than silently bifurcating the pure-membership surface
///     every lint / filter / gate consumer routes through.
/// 12. [`ClosedSet::find_by_label`] composes [`ClosedSet::ALL`] +
///     [`ClosedSet::label`] with [`Iterator::find`] verbatim — the
///     zero-allocation typed decode every consumer that needs the
///     variant (but can supply a natural fallback) routes through
///     emits at ONE trait body without ever materializing the
///     [`ClosedSet::Unknown`] carrier [`ClosedSet::parse_label`]
///     threads on rejection. The sweep walks every variant's
///     canonical label (expected `Some(v)` — the acceptance arm
///     round-trips through the typed decode), the reserved 38-char
///     probe (expected `None` — the same probe clauses (5) + (7) +
///     (11) reserve as lexically distinct from every plausible
///     canonical label), and the empty-string boundary (expected
///     `None` matching clauses (4) + (11)) so a drift in any of the
///     three natural typed-decode arms (a permissive override that
///     accepts non-canonical strings, a strict override that
///     rejects a canonical label, a subset-projection override that
///     names fewer labels than `Self::ALL.iter().map(label)` covers)
///     fails the testkit on every implementor. The default trait
///     body satisfies the clause for free; the assertion catches a
///     future implementor whose override drifts the composition
///     loudly rather than silently bifurcating the zero-allocation
///     typed-decode surface every consumer routes through. Sibling
///     posture to clause (11) on the (bool, Option<Self>)
///     return-type axis — both walk the SAME (`Self::ALL`,
///     `Self::label`) primitive pair and MUST agree on the
///     underlying (accept, reject) partition; the pin verifies the
///     alignment across both arms of the axis.
/// 13. [`ClosedSet::find_by_label_with_hint`] composes
///     [`ClosedSet::find_by_label`] and [`ClosedSet::suggest_closest`]
///     verbatim — the zero-allocation structured-decode surface
///     every consumer that needs the typed variant AND (on miss) the
///     typed hint, without paying the [`ClosedSet::Unknown`] carrier
///     allocation [`ClosedSet::parse_label_with_hint`] threads on
///     rejection, routes through emits at ONE trait body. Every
///     variant in `T::ALL` decodes to `Ok(v)` through the structured
///     surface (the hint slot is structurally absent on the Ok arm),
///     and the sweep's reserved probe input rejects with `Err(None)`
///     — the probe sits beyond [`ClosedSet::suggest_closest`]'s
///     bounded edit distance by construction (its 38-char body
///     shares no characters with any plausible canonical label), so
///     the conservative-suggestion contract demands the absent hint
///     slot. The default trait body satisfies the clause for free;
///     the assertion catches a future implementor whose
///     `find_by_label_with_hint` override drifts from the natural
///     composition (a degenerate axis the trait surface exposes for
///     the same reason `via` / `set_label` / `labels` overrides
///     exist — a typed escape hatch rather than forcing the
///     implementor to hand-roll the impl). A drifted override that
///     accepts the probe as `Ok`, fabricates a hint for the
///     unrecognizable probe, OR emits the wrong typed decode on a
///     canonical variant fails this clause loudly rather than
///     silently bifurcating the zero-allocation structured-decode
///     surface every LSP / config-decoder / filter-map consumer
///     routes through. Sibling posture to clause (7) on the
///     (allocating carrier decode, non-allocating typed decode)
///     axis — both compose the SAME [`ClosedSet::suggest_closest`]
///     hint primitive next to the underlying typed-decode primitive
///     on their respective (allocating, non-allocating) columns of
///     the (side-effect × hint) 2×2 matrix; the pin verifies the
///     alignment across both arms of the axis.
/// 14. [`ClosedSet::CARDINALITY`] equals [`ClosedSet::ALL`]`.len()` — the
///     const-visible variant count matches the runtime slice length.
///     The default trait const initializer `Self::ALL.len()`
///     satisfies the clause for free; the assertion catches a future
///     implementor whose override drifts the count (a hand-rolled
///     const that reports a different cardinality than `Self::ALL`
///     actually carries) loudly rather than silently bifurcating the
///     const-generic surface every downstream `[Payload;
///     T::CARDINALITY]` array / bitset-width consumer routes through.
///     Sibling posture to clause (1) — clause (1) pins `T::ALL` non-
///     empty, this clause pins the const-visible count against the
///     slice length so a generic const-generic consumer that binds
///     `[Payload; T::CARDINALITY]` and iterates `T::ALL` in lockstep
///     stays sound at both the type-level dimension AND the runtime
///     iteration boundary.
/// 15. For every `i in 0..T::ALL.len()`,
///     `T::ALL[i].index_of()` equals `i` — the (typed variant →
///     `usize` array index) bijection with `0..T::CARDINALITY` holds
///     on every declaration-order position. The default trait body's
///     discriminant-keyed `Iterator::position` sweep satisfies the
///     clause for free; the assertion catches a future implementor
///     whose override drifts from the natural `ALL`-position
///     projection (a hand-rolled `match` that swaps two arms, a
///     constant that reports the same index for every variant, an
///     over-eager caching layer that returns a stale index after a
///     variant-listing edit) loudly rather than silently bifurcating
///     the (variant → array index) bijection every downstream
///     per-variant lookup-table `[Payload; T::CARDINALITY]` /
///     bitset / compact-encoding consumer routes through. Sibling
///     posture to clause (14) — clause (14) pins the const-visible
///     cardinality against `T::ALL`'s slice length, this clause pins
///     the per-variant position against `T::ALL`'s indexed access so
///     the closed set's (typed variant ↔ array-index position)
///     bijection stays sound at the compile-time dimension (clause
///     14) AND the runtime per-variant projection (this clause).
/// 16. For every `i in 0..T::CARDINALITY`, `T::from_index(i)` equals
///     `Some(T::ALL[i])`, AND `T::from_index(T::CARDINALITY)` equals
///     [`None`] — the (`usize` array index → typed variant) inverse
///     projection agrees with direct [`ClosedSet::ALL`] slice indexing
///     on the in-range domain AND rejects the first out-of-range
///     index. Clauses (15) + (16) together pin the (typed variant ↔
///     `usize` array index) bijection at BOTH directions: clause (15)
///     covers the forward `variant.index_of() == i` projection, this
///     clause covers the inverse `T::from_index(i) == Some(v)`
///     projection AND the out-of-range guard. The default trait body's
///     `Self::ALL.get(i).copied()` composition satisfies the clause
///     for free; the assertion catches a future implementor whose
///     override drifts the bounded-decode arm (a permissive override
///     that returns `Some` for an out-of-range index, folding an
///     out-of-range serialized index onto an in-range variant; a
///     strict override that returns `None` for a valid in-range index,
///     silently dropping variants at the compact-decode boundary; a
///     swapped override that recovers the wrong variant for a valid
///     index, silently bifurcating the (variant ↔ index) round-trip)
///     loudly rather than silently bifurcating the inverse-decode
///     surface every downstream compact-encoding / bitset-observed-
///     variant / lookup-table-iteration consumer routes through.
///     Sibling posture to clause (12) on the (label decode, index
///     decode) axis of the closed-set inbound-projection surface —
///     both close the inbound-projection surface with an
///     [`Option`]-typed rejection arm, [`ClosedSet::find_by_label`]
///     for the `&str` carrier, [`ClosedSet::from_index`] for the
///     `usize` carrier; the pin verifies the alignment across both
///     carriers on the (in-range accept, out-of-range reject)
///     partition.
/// 17. [`ClosedSet::sorted_variants`] composes [`ClosedSet::ALL`] with
///     `Vec::from` + `slice::sort_unstable_by_key` keyed on
///     [`ClosedSet::label`] verbatim, AND stays element-wise aligned
///     with [`ClosedSet::sorted_labels`] on the (typed variant,
///     canonical label) axis of the closed-set candidate-listing
///     surface. For every `i in 0..T::CARDINALITY`,
///     `T::sorted_variants()[i].label()` equals
///     `T::sorted_labels()[i]`, AND the sorted-variant slice length
///     equals [`ClosedSet::CARDINALITY`]. The default trait body
///     satisfies the clause for free; the assertion catches a future
///     implementor whose override drifts the composition (a subset of
///     variants, a different ordering, a swapped variant, an
///     off-by-one length) loudly rather than silently bifurcating the
///     sorted-typed-variant candidate-list surface every LSP /
///     `tatara-check` / metrics consumer routes through. Sibling
///     posture to clauses (9) + (16) — clause (9) pins the
///     lexicographic `Vec<&'static str>` corner of the (return-type ×
///     ordering) 2×2 matrix, this clause pins the lexicographic
///     `Vec<Self>` corner AND the element-wise alignment across the
///     two lexicographic corners so a downstream consumer that walks
///     `zip(sorted_variants(), sorted_labels())` per-slot sees the
///     same (typed variant, canonical label) pair on both projections.
///     Clause (16) covered the `usize` inverse-projection alignment
///     with `T::ALL` slice indexing; this clause covers the label-
///     keyed inverse-ordering alignment with `T::sorted_labels`, so
///     the closed set's two structural projections (index-keyed and
///     label-keyed) both stay sound at the runtime element-wise
///     boundary.
/// 18. [`ClosedSet::first`] equals `T::ALL[0]` AND [`ClosedSet::last`]
///     equals `T::ALL[T::ALL.len() - 1]` — the declaration-order
///     endpoint anchors project the head and tail of the [`ClosedSet::ALL`]
///     slice onto the trait surface as bare typed variants (no
///     [`Option`] / [`Result`] indirection, because clause (1) pins
///     [`ClosedSet::ALL`] non-empty so both endpoints are guaranteed
///     to exist). The default trait bodies satisfy the clause for
///     free; the assertion catches a future implementor whose
///     override drifts from the natural slice-endpoint projections
///     (a permissive override that returns some interior variant, a
///     swapped override that returns [`ClosedSet::last`]'s tail for
///     [`ClosedSet::first`], a stale override that returns the wrong
///     endpoint after a variant-listing edit) loudly rather than
///     silently bifurcating the endpoint-anchor surface every
///     downstream defaulter / iterator-start / iterator-terminator
///     consumer routes through. Sibling posture to clauses (15) +
///     (16) — clauses (15) + (16) pin the (typed variant ↔ array
///     index) bijection with `0..T::CARDINALITY` on every interior
///     slot, this clause pins the (head, tail) endpoint anchors
///     against `T::ALL[0]` / `T::ALL[T::ALL.len() - 1]` so the
///     closed set's structural endpoints stay sound at the two
///     canonical anchor sites.
/// 20. For every `i in 0..T::CARDINALITY`, `T::label_at(i)` equals
///     `Some(T::ALL[i].label())`, AND `T::label_at(T::CARDINALITY)`
///     equals [`None`] — the direct (`usize` array index →
///     `&'static str` canonical label) projection agrees with the
///     natural [`ClosedSet::from_index`] + [`ClosedSet::label`]
///     composition on the in-range domain AND rejects the first
///     out-of-range index. Clauses (16) + (20) together pin the
///     `usize`-carrier decode axis of the (typed variant, `&'static
///     str` label, `usize` index) projection triangle at BOTH
///     return-projection columns: clause (16) covers the (index →
///     typed variant) return-projection, this clause covers the
///     (index → `&'static str` label) return-projection. The
///     default trait body's [`ClosedSet::from_index`] +
///     [`ClosedSet::label`] composition satisfies the clause for
///     free; the assertion catches a future implementor whose
///     override drifts the direct-label projection arm (a permissive
///     override that returns `Some(_)` for an out-of-range index,
///     folding an out-of-range serialized index onto an in-range
///     label — silently bifurcating the direct-label projection from
///     [`ClosedSet::from_index`]'s bounded-decode arm; a strict
///     override that returns [`None`] for a valid in-range index,
///     silently dropping labels at the compact-decode boundary; a
///     swapped override that recovers the wrong label for a valid
///     index, silently bifurcating the (variant, `&'static str`
///     label, `usize` index) projection triangle) loudly rather than
///     silently bifurcating the direct-label projection surface every
///     downstream compact-encoding / metrics-per-slot / bitset-
///     observed-slot / `tatara-check` per-slot diagnostic consumer
///     routes through. Sibling posture to clause (16) on the
///     (typed variant, `&'static str` label) return-projection axis
///     of the `usize`-carrier partition — clause (16) closes the
///     (index → typed variant) direct projection, this clause closes
///     the (index → `&'static str` label) direct projection, so the
///     `usize`-carrier partition of the projection triangle stays
///     sound at both return-projection columns AND on both the
///     in-range accept AND the out-of-range reject partitions.
/// 23. For every `i in 0..T::CARDINALITY`, `T::from_sorted_index(i)`
///     equals `Some(T::sorted_variants()[i])`, AND
///     `T::from_sorted_index(T::CARDINALITY)` equals [`None`] — the
///     (`usize` lex-order position → typed variant) inverse projection
///     agrees with direct [`ClosedSet::sorted_variants`] slice indexing
///     on the in-range domain AND rejects the first out-of-range
///     index. Clauses (22) + (23) together pin the (typed variant ↔
///     `usize` lex-order position) bijection at BOTH directions:
///     clause (22) covers the forward `variant.sorted_index_of() == i`
///     projection, this clause covers the inverse
///     `T::from_sorted_index(i) == Some(v)` projection AND the
///     out-of-range guard. The default trait body's
///     `Self::sorted_variants().get(i).copied()` composition satisfies
///     the clause for free; the assertion catches a future implementor
///     whose override drifts the bounded-decode arm (a permissive
///     override that returns `Some(_)` for an out-of-range index, a
///     strict override that returns [`None`] for a valid in-range lex
///     slot, a swapped override that recovers the wrong variant for a
///     valid lex slot) loudly rather than silently bifurcating the
///     lex-order inverse-decode surface every downstream lex-order
///     compact-encoding / lex-order-bitset-observed-variant / lex-
///     order-lookup-table-iteration consumer routes through. Sibling
///     posture to clauses (16) + (22) — clause (16) closes the
///     (declaration-order index → typed variant) inverse projection on
///     the (declaration, lex) partition of the (position → variant)
///     inverse-projection surface; this clause closes the (lex-order
///     index → typed variant) inverse projection so the (declaration,
///     lex) × (position → variant) 1×2 inverse-projection partition
///     completes at BOTH ordering axes.
/// 24. For every `i in 0..T::CARDINALITY`, `T::sorted_label_at(i)`
///     equals `Some(T::sorted_labels()[i])`, AND
///     `T::sorted_label_at(T::CARDINALITY)` equals [`None`] — the
///     direct (`usize` lex-order position → `&'static str` canonical
///     label) projection agrees with the natural
///     [`ClosedSet::from_sorted_index`] + [`ClosedSet::label`]
///     composition on the in-range domain (equivalently, with direct
///     [`ClosedSet::sorted_labels`] slice indexing under lex ordering)
///     AND rejects the first out-of-range lex slot. Clauses (20) + (24)
///     together pin the (`usize` position → `&'static str` label)
///     forward projection at BOTH ordering axes: clause (20) covers the
///     declaration-ordering direct-label projection, this clause covers
///     the lex-ordering direct-label projection. The default trait
///     body's [`ClosedSet::from_sorted_index`] + [`ClosedSet::label`]
///     composition satisfies the clause for free; the assertion catches
///     a future implementor whose override drifts the direct-label
///     projection arm (a permissive override that returns `Some(_)` for
///     an out-of-range lex slot, folding an out-of-range serialized
///     lex index onto an in-range label at the lex-order compact-decode
///     boundary; a strict override that returns [`None`] for a valid
///     in-range lex slot, silently dropping labels at the lex-order
///     rendering boundary; a swapped override that recovers the wrong
///     label for a valid lex slot, silently bifurcating the lex-axis
///     projection triangle) loudly rather than silently bifurcating
///     the direct-label projection surface every downstream lex-order
///     compact-encoding / lex-sorted-metrics-binner / bitset-observed-
///     slot-lex-renderer / `tatara-check` per-lex-slot diagnostic
///     consumer routes through. Sibling posture to clauses (20) + (23)
///     — clause (20) closes the (declaration-order index → `&'static
///     str` label) direct projection, clause (23) closes the (lex-order
///     index → typed variant) inverse projection, this clause closes
///     the (lex-order index → `&'static str` label) direct projection
///     so the lex-axis projection triangle stays sound at BOTH
///     return-projection columns AND on both the in-range accept AND
///     the out-of-range reject partitions.
/// 25. For every variant `v` in `T::ALL`, `T::sorted_index_of_label(
///     v.label())` equals `Some(v.sorted_index_of())`, AND
///     `T::sorted_index_of_label(<reserved probe>)` equals [`None`], AND
///     `T::sorted_index_of_label("")` equals [`None`] — the direct
///     (`&'static str` label → `usize` lex-order position) projection
///     agrees with the natural [`ClosedSet::find_by_label`] +
///     [`ClosedSet::sorted_index_of`] composition on the in-range
///     canonical domain AND rejects both the reserved out-of-set probe
///     AND the empty-string boundary that clause (4) reserves as
///     structurally outside every closed set. Clauses (21) + (25)
///     together pin the (`&str` label → `usize` position) forward
///     projection at BOTH ordering axes: clause (21) covers the
///     declaration-ordering direct-index projection, this clause covers
///     the lex-ordering direct-index projection — so the (`&str` label
///     → `usize` position) forward-projection partition stays sound at
///     BOTH ordering axes. The default trait body's
///     [`ClosedSet::find_by_label`] + [`ClosedSet::sorted_index_of`]
///     composition satisfies the clause for free; the assertion catches
///     a future implementor whose override drifts the direct-lex-slot
///     projection arm (a permissive override that returns `Some(_)` for
///     a non-canonical `&str` — folding an off-set string onto an
///     in-range lex slot at the direct-projection column while
///     `find_by_label` still rejects it, silently bifurcating the
///     `&str`-carrier decode axis on the lex-order projection column;
///     a strict override that returns [`None`] for a canonical variant's
///     label, silently dropping lex slots at the label-decode boundary;
///     a swapped override that recovers the wrong lex slot for a valid
///     canonical label, silently bifurcating the (typed variant,
///     `&'static str` label, `usize` position) lex-axis projection
///     triangle at its fourth direct edge) loudly rather than silently
///     bifurcating the direct-lex-slot projection surface every
///     downstream lex-sorted-metrics-binner / lex-order-compact-encoder
///     / `tatara-check` per-lex-slot per-label diagnostic / LSP-hover
///     consumer routes through. Sibling posture to clauses (12) + (22)
///     — clause (12) closes the (`&str` → typed variant) `Option`-typed
///     direct projection, clause (22) closes the (typed variant →
///     `usize` lex position) forward projection, this clause closes
///     the (`&str` → `usize` lex position) direct projection composed
///     from BOTH primitives AND pins the alignment with the natural
///     two-step composition on every implementor. Together clauses
///     (12) + (22) + (25) close the `&str`-carrier partition of the
///     lex-axis projection triangle at both the immediate (variant)
///     decode column AND the further (lex position) decode column,
///     mirroring the way clauses (12) + (15) + (21) close the same
///     `&str`-carrier partition on the declaration axis. With clauses
///     (20) + (21) + (22) + (23) + (24) + (25) all in place, the
///     (typed variant, `&'static str` label, `usize` position)
///     projection triangle stays direct-projection closed at EVERY
///     (input, output) pair on BOTH ordering axes.
/// 26. For every variant `v` in `T::ALL`, `v.next()` equals
///     `T::from_index(v.index_of() + 1)`, AND `v.prev()` equals
///     `T::from_index(v.index_of() - 1)` when `v.index_of() > 0`,
///     AND `T::first().prev()` equals [`None`], AND `T::last().next()`
///     equals [`None`] — the declaration-order (variant → forward
///     neighbor, variant → backward neighbor) direction pair agrees
///     with the natural [`ClosedSet::index_of`] +
///     [`ClosedSet::from_index`] composition on every interior slot
///     AND rejects both endpoint boundaries. The default trait
///     bodies (`from_index(index_of(self) + 1)` for the forward arm,
///     `from_index(index_of(self) - 1)` guarded on
///     `index_of(self) > 0` for the backward arm) satisfy both arms
///     for free; the assertion catches a future implementor whose
///     override drifts either neighbor arm (a permissive forward
///     override that returns `Some(_)` at the tail — folding a
///     tail-boundary walk onto a wraparound to the head at the
///     forward-projection column while the composed `+ 1` arithmetic
///     would return [`None`] through `from_index`'s `<[T]>::get`;
///     a permissive backward override that returns `Some(_)` at the
///     head — folding a head-boundary walk onto a wraparound to the
///     tail through the `usize` underflow the guard prevents; a
///     swapped override that returns the predecessor for
///     [`Self::next`] AND the successor for [`Self::prev`], silently
///     inverting the traversal direction every downstream state-
///     machine iterator / phase-fold reducer / LSP wraparound-cursor
///     consumer walks over; a stale override that returns the wrong
///     neighbor after a variant-listing edit) loudly rather than
///     silently bifurcating the neighbor-projection surface every
///     downstream state-machine iterator / saga-step engine /
///     truth-table property test / phase-fold reducer consumer
///     routes through. Sibling posture to clauses (15) + (16) + (18)
///     — clauses (15) + (16) pin the (typed variant ↔ `usize` array
///     index) bijection at BOTH directions, clause (18) pins the
///     (head, tail) endpoint anchors against `T::ALL[0]` /
///     `T::ALL[T::ALL.len() - 1]`, this clause pins the (forward,
///     backward) neighbor projections against the composition of
///     both bijection arms AND pins the endpoint-boundary [`None`]
///     guards on both direction arms — so the closed-set traversal
///     surface stays sound at BOTH direction arms AND on the shared
///     endpoint-anchor fixpoints (`T::last().next() == None`,
///     `T::first().prev() == None`) that thread the neighbor axis
///     back through the endpoint-anchor axis.
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
    // (8) — `T::labels_joined(sep)` composes `T::labels()` with
    // `slice::join` verbatim. The default trait body satisfies the
    // clause for free; the assertion catches a future implementor
    // whose override drifts from the natural
    // `labels().join(sep)` shape (a degenerate axis the trait
    // surface exposes for the same reason `via` / `set_label` /
    // `labels` / `suggest_closest` / `parse_label_with_hint`
    // overrides exist — a typed escape hatch rather than forcing
    // the implementor to hand-roll the impl). Sweep three
    // representative separators (the slash, comma-space, and pipe
    // shapes the substrate's existing production sites lean on)
    // so a drift in any one of the three rendering surfaces
    // (slash for `INTENT_KIND_LIST`-shaped lists, comma-space for
    // diagnostic `expected one of: ...` shapes, pipe for ergonomic
    // grammar-style lists) fails the testkit on every implementor.
    for sep in ["/", ", ", "|"] {
        let lifted = T::labels_joined(sep);
        let natural = T::labels().join(sep);
        assert_eq!(
            lifted, natural,
            "{type_name}: T::labels_joined({sep:?}) drifted from T::labels().join({sep:?}) — the joined-candidate-list rendering every diagnostic / metrics consumer routes through no longer matches the natural labels-projection",
        );
    }
    // (9) — `T::sorted_labels()` composes `T::labels()` with
    // `slice::sort_unstable` verbatim. The default trait body satisfies
    // the clause for free; the assertion catches a future implementor
    // whose override drifts from the natural `labels().sort_unstable()`
    // shape (a different ordering, a subset of labels, declaration order
    // instead of lexicographic) loudly rather than silently bifurcating
    // the canonical-ordered candidate-list surface every LSP /
    // `tatara-check` / metrics consumer routes through.
    let lifted_sorted = T::sorted_labels();
    let natural_sorted = {
        let mut v = T::labels();
        v.sort_unstable();
        v
    };
    assert_eq!(
        lifted_sorted, natural_sorted,
        "{type_name}: T::sorted_labels() drifted from `let mut v = T::labels(); v.sort_unstable(); v` — the canonical-ordered candidate-list surface every LSP / `tatara-check` / metrics consumer routes through no longer matches the natural labels-then-sort projection",
    );
    // (10) — `T::sorted_labels_joined(sep)` composes
    // `T::sorted_labels()` with `slice::join` verbatim. The default
    // trait body satisfies the clause for free; the assertion catches
    // a future implementor whose override returns a different shape
    // (a subset of labels, a different ordering, declaration order
    // instead of lexicographic, a wrong separator threading) loudly
    // rather than silently bifurcating the alphabetized-
    // candidate-list-as-string surface every LSP / `tatara-check` /
    // metrics consumer routes through. Sweep the same three
    // representative separators clause (8) uses (`"/"`, `", "`, `"|"`)
    // so an isolated drift on any of the three natural rendering
    // surfaces (slash for ordering-independent production constants,
    // comma-space for natural-language alphabetized `expected one of:
    // ...` shapes, pipe for grammar-style alphabetized alternative
    // lists) fails the testkit on every implementor.
    for sep in ["/", ", ", "|"] {
        let lifted = T::sorted_labels_joined(sep);
        let natural = T::sorted_labels().join(sep);
        assert_eq!(
            lifted, natural,
            "{type_name}: T::sorted_labels_joined({sep:?}) drifted from T::sorted_labels().join({sep:?}) — the alphabetized joined-candidate-list rendering every diagnostic / metrics consumer routes through no longer matches the natural sorted-labels-then-join projection",
        );
    }
    // (11) — `T::contains_label(s)` MUST agree with
    // `T::parse_label(s).is_ok()` on every representative input:
    // every canonical label matches (`true`), the reserved probe
    // rejects (`false`), and the empty-string boundary rejects
    // (`false`) matching clause (4). The default trait body
    // satisfies the clause for free; the assertion catches a future
    // implementor whose override drifts the composition (a
    // permissive override that returns `true` for inputs outside
    // the closed set, a strict override that returns `false` for a
    // canonical label, a subset-projection override that names
    // fewer labels than `Self::ALL.iter().map(label)` covers) loudly
    // rather than silently bifurcating the pure-membership surface
    // every lint / filter / gate consumer routes through.
    for &v in T::ALL {
        let label = v.label();
        assert!(
            T::contains_label(label),
            "{type_name}: T::contains_label({label:?}) returned false for a canonical variant label — the pure-membership predicate drifted from the natural `ALL`-projection",
        );
    }
    assert!(
        !T::contains_label(probe),
        "{type_name}: T::contains_label(<reserved probe>) returned true for an input outside the closed set — the pure-membership predicate accepted a non-canonical string",
    );
    assert!(
        !T::contains_label(""),
        "{type_name}: T::contains_label(\"\") returned true — the pure-membership predicate accepted the empty-string boundary that clause (4) reserves as structurally outside every closed set",
    );
    // (12) — `T::find_by_label(s)` MUST agree with
    // `T::parse_label(s).ok()` on every representative input: every
    // canonical label decodes to `Some(v)` (the acceptance arm
    // round-trips through the typed decode), the reserved probe
    // rejects to `None` (the same probe clauses (5) + (7) + (11)
    // reserve as lexically distinct from every plausible canonical
    // label), and the empty-string boundary rejects to `None`
    // matching clauses (4) + (11). The default trait body satisfies
    // the clause for free; the assertion catches a future
    // implementor whose override drifts the composition (a
    // permissive override that returns `Some(_)` for inputs outside
    // the closed set, a strict override that returns `None` for a
    // canonical label, a subset-projection override that names fewer
    // labels than `Self::ALL.iter().map(label)` covers) loudly
    // rather than silently bifurcating the zero-allocation
    // typed-decode surface every LSP / config-decoder / filter-map
    // consumer routes through. Sibling posture to clause (11) on the
    // (bool, Option<Self>) return-type axis — both walk the SAME
    // (Self::ALL, Self::label) primitive pair and MUST agree on the
    // underlying (accept, reject) partition; the pin verifies the
    // alignment across both arms of the axis.
    for &v in T::ALL {
        let label = v.label();
        match T::find_by_label(label) {
            Some(decoded) => assert_eq!(
                decoded, v,
                "{type_name}: T::find_by_label({label:?}) decoded to a different variant — the zero-allocation typed decode drifted from the natural `ALL`-projection",
            ),
            None => panic!(
                "{type_name}: T::find_by_label({label:?}) returned None for a canonical variant label — the zero-allocation typed decode drifted from the natural `ALL`-projection",
            ),
        }
    }
    assert!(
        T::find_by_label(probe).is_none(),
        "{type_name}: T::find_by_label(<reserved probe>) returned Some(_) for an input outside the closed set — the zero-allocation typed decode accepted a non-canonical string",
    );
    assert!(
        T::find_by_label("").is_none(),
        "{type_name}: T::find_by_label(\"\") returned Some(_) — the zero-allocation typed decode accepted the empty-string boundary that clause (4) reserves as structurally outside every closed set",
    );
    // (13) — `T::find_by_label_with_hint` composes `find_by_label` +
    // `suggest_closest` verbatim. Every variant decodes to `Ok(v)`
    // through the structured surface; the probe rejects with
    // `Err(None)` — the same 38-char probe clause (7) reserves as
    // beyond `suggest_closest`'s bounded edit distance by
    // construction. The default trait body satisfies the clause for
    // free; the assertion catches an override that drifts the
    // composition (accepts the probe as Ok, fabricates a hint for
    // the unrecognizable probe, OR emits the wrong typed decode on
    // a canonical variant). Sibling posture to clause (7) on the
    // (allocating carrier decode, non-allocating typed decode) axis
    // of the closed-set surface — both compose the SAME
    // `suggest_closest` hint primitive next to the underlying
    // typed-decode primitive on their respective columns of the
    // (side-effect × hint) 2×2 matrix.
    for &v in T::ALL {
        let label = v.label();
        match T::find_by_label_with_hint(label) {
            Ok(decoded) => assert_eq!(
                decoded, v,
                "{type_name}: find_by_label_with_hint round-trip {label:?} → variant decoded to a different variant",
            ),
            Err(_) => panic!(
                "{type_name}: find_by_label_with_hint round-trip {label:?} → variant rejected by find_by_label_with_hint",
            ),
        }
    }
    match T::find_by_label_with_hint(probe) {
        Ok(_) => panic!(
            "{type_name}: find_by_label_with_hint accepted the reserved probe input — the structured zero-allocation surface MUST reject every input outside the closed set",
        ),
        Err(hint) => {
            assert!(
                hint.is_none(),
                "{type_name}: find_by_label_with_hint fabricated a `did you mean ...?` hint for the unrecognizable probe — the conservative-suggestion contract demands `None` for inputs beyond the bounded edit distance",
            );
        }
    }
    // (14) — `T::CARDINALITY` MUST equal `T::ALL.len()`. The default
    // trait const initializer `Self::ALL.len()` satisfies the clause
    // for free; the assertion catches a future implementor whose
    // override drifts from the natural `ALL`-length projection (a
    // degenerate axis the trait surface exposes for the same reason
    // `via` / `set_label` / `labels` overrides exist — a typed escape
    // hatch rather than forcing the implementor to hand-roll the
    // impl). A drifted override that reports a different count than
    // `Self::ALL` actually carries silently bifurcates every
    // downstream const-generic consumer's `[T; T::CARDINALITY]`
    // array from `Self::ALL`'s runtime iteration; pinning the
    // equality here catches the drift on every implementor before
    // any consumer sizes a const array against the drifted count.
    // Sibling posture to clause (1) — clause (1) pins `T::ALL` non-
    // empty, this clause pins the const-visible count against the
    // slice length so a generic const-generic consumer that takes
    // `T::CARDINALITY - 1` as a top-rank index stays sound.
    assert_eq!(
        T::CARDINALITY,
        T::ALL.len(),
        "{type_name}: T::CARDINALITY drifted from T::ALL.len() — the const-visible cardinality no longer matches the runtime variant count. A generic const-generic consumer that binds `[Payload; T::CARDINALITY]` against `T::ALL`-length iteration would silently size the wrong dimension",
    );
    // (15) — For every `i in 0..T::ALL.len()`, `T::ALL[i].index_of()`
    // MUST equal `i`. The default trait body's discriminant-keyed
    // `Iterator::position` sweep satisfies the clause for free; the
    // assertion catches a future implementor whose override drifts
    // from the natural `ALL`-position projection (a hand-rolled
    // `match` that swaps two arms, a constant that reports the same
    // index for every variant, an over-eager caching layer that
    // returns a stale index after a variant-listing edit). A drifted
    // override silently bifurcates the (variant → array index)
    // bijection with `0..T::CARDINALITY` — a downstream per-variant
    // lookup-table `[Payload; T::CARDINALITY]` consumer keyed by
    // `variant.index_of()` would land on the wrong slot; pinning the
    // equality here catches the drift on every implementor before any
    // consumer keys a per-variant payload against the drifted
    // position. Sibling posture to clause (14) — clause (14) pins the
    // const-visible cardinality against `T::ALL`'s slice length, this
    // clause pins the per-variant position against `T::ALL`'s indexed
    // access so the closed set's (typed variant ↔ array-index
    // position) bijection stays sound at the compile-time dimension
    // (clause 14) AND the runtime per-variant projection (this
    // clause).
    for (i, &v) in T::ALL.iter().enumerate() {
        assert_eq!(
            v.index_of(),
            i,
            "{type_name}: T::ALL[{i}].index_of() drifted from its declaration-order position — the (variant → array index) bijection with 0..T::CARDINALITY broke on this variant, so a per-variant lookup-table `[Payload; T::CARDINALITY]` consumer keyed by `variant.index_of()` would land on the wrong slot",
        );
    }
    // (16) — For every `i in 0..T::CARDINALITY`, `T::from_index(i)`
    // MUST equal `Some(T::ALL[i])`, AND `T::from_index(T::CARDINALITY)`
    // MUST return `None`. The default trait body's
    // `Self::ALL.get(i).copied()` composition satisfies the clause for
    // free; the assertion catches a future implementor whose override
    // drifts the bounded-decode arm (a permissive override that
    // returns `Some` for an out-of-range index, folding an out-of-
    // range serialized index onto an in-range variant; a strict
    // override that returns `None` for a valid in-range index,
    // silently dropping variants at the compact-decode boundary; a
    // swapped override that recovers the wrong variant for a valid
    // index, silently bifurcating the (variant ↔ index) round-trip).
    // Sibling posture to clause (15) — clause (15) pins the forward
    // `variant.index_of() == i` projection on every declaration-order
    // position, this clause pins the inverse `T::from_index(i) ==
    // Some(v)` projection AND the out-of-range `T::from_index(N) ==
    // None` guard so the (typed variant ↔ `usize` array index)
    // bijection stays sound at BOTH directions on every implementor.
    for (i, &v) in T::ALL.iter().enumerate() {
        let recovered = T::from_index(i);
        assert_eq!(
            recovered,
            Some(v),
            "{type_name}: T::from_index({i}) drifted from Some(T::ALL[{i}]) — the (array index → typed variant) inverse projection broke on this slot, so a downstream compact-encoding consumer that stores `variant.index_of() as u8` and later recovers the variant through `T::from_index(byte as usize)` would land on the wrong variant",
        );
    }
    assert!(
        T::from_index(T::CARDINALITY).is_none(),
        "{type_name}: T::from_index(T::CARDINALITY) returned Some — the out-of-range guard drifted, so a per-variant lookup-table `[Payload; T::CARDINALITY]` consumer that decodes an out-of-range serialized index would silently fold onto an in-range variant rather than surfacing the corruption at the decode boundary",
    );
    // (17) — `T::sorted_variants()` MUST compose `T::ALL` +
    // `Vec::from` + `slice::sort_unstable_by_key` keyed on `label`
    // verbatim, AND stay element-wise aligned with
    // `T::sorted_labels()` on the (typed variant, canonical label)
    // axis. Sweep both the length equality (against `T::CARDINALITY`)
    // AND the per-slot alignment (`sorted_variants()[i].label() ==
    // sorted_labels()[i]` for every `i in 0..T::CARDINALITY`). The
    // default trait body's `to_vec().sort_unstable_by_key(|v| v.label())`
    // composition satisfies the clause for free; the assertion catches
    // a future implementor whose override drifts the composition (a
    // subset of variants, a swapped variant, a different ordering, an
    // off-by-one length) loudly rather than silently bifurcating the
    // sorted-typed-variant candidate-list surface every LSP /
    // `tatara-check` / metrics consumer routes through. Sibling
    // posture to clause (9) — clause (9) pins the lexicographic
    // `Vec<&'static str>` corner of the (return-type × ordering) 2×2
    // matrix, this clause pins the lexicographic `Vec<Self>` corner
    // AND the element-wise alignment across the two lexicographic
    // corners so a downstream consumer that walks `zip(sorted_variants(),
    // sorted_labels())` per-slot sees the same (typed variant,
    // canonical label) pair on both projections.
    let sorted_variants = T::sorted_variants();
    assert_eq!(
        sorted_variants.len(),
        T::CARDINALITY,
        "{type_name}: T::sorted_variants().len() drifted from T::CARDINALITY — the sorted-typed-variant candidate-list surface lost or gained a variant, so a downstream consumer that walks `zip(sorted_variants(), sorted_labels())` per-slot would run off the end or short-cut before covering every variant",
    );
    let sorted_variant_labels: Vec<&'static str> = sorted_variants
        .iter()
        .copied()
        .map(<T as ClosedSet>::label)
        .collect();
    let sorted_labels_reference = T::sorted_labels();
    assert_eq!(
        sorted_variant_labels, sorted_labels_reference,
        "{type_name}: T::sorted_variants() projected element-wise through label() drifted from T::sorted_labels() — the (typed variant, canonical label) alignment on the lexicographic-ordering axis broke, so a downstream consumer that walks `zip(sorted_variants(), sorted_labels())` would see two different renderings on the same slot",
    );
    // (18) — `T::first()` equals `T::ALL[0]` AND `T::last()` equals
    // `T::ALL[T::ALL.len() - 1]`. The default trait bodies satisfy
    // the clause for free; the assertion catches a future implementor
    // whose override drifts from the natural slice-endpoint
    // projections (a permissive override that returns some interior
    // variant, a swapped override that returns the tail for
    // `first()`, a stale override that returns the wrong endpoint
    // after a variant-listing edit) loudly rather than silently
    // bifurcating the endpoint-anchor surface every downstream
    // defaulter / iterator-start / iterator-terminator consumer
    // routes through. Sibling posture to clauses (15) + (16) —
    // clauses (15) + (16) pin the (typed variant ↔ array index)
    // bijection with `0..T::CARDINALITY` on every interior slot,
    // this clause pins the (head, tail) endpoint anchors against
    // `T::ALL[0]` / `T::ALL[T::ALL.len() - 1]` so the closed set's
    // structural endpoints stay sound at the two canonical anchor
    // sites. The non-empty contract clause (1) guarantees both
    // endpoints exist; the subtraction `T::ALL.len() - 1` never
    // underflows.
    assert_eq!(
        T::first(),
        T::ALL[0],
        "{type_name}: T::first() drifted from T::ALL[0] — the declaration-order head endpoint anchor no longer matches the natural slice-index-0 projection, so a downstream defaulter / iterator-start consumer that binds `T::first()` as its canonical anchor would land on the wrong variant",
    );
    assert_eq!(
        T::last(),
        T::ALL[T::ALL.len() - 1],
        "{type_name}: T::last() drifted from T::ALL[T::ALL.len() - 1] — the declaration-order tail endpoint anchor no longer matches the natural slice-index-(N - 1) projection, so a downstream iterator-terminator / bounded-loop consumer that binds `T::last()` as its canonical anchor would land on the wrong variant",
    );
    // (19) — `T::sorted_first()` equals `T::sorted_variants()[0]` AND
    // `T::sorted_last()` equals
    // `T::sorted_variants()[T::sorted_variants().len() - 1]`. The
    // default trait bodies satisfy the clause for free (both compose
    // `T::ALL` + `T::label` via a zero-alloc linear scan whose result
    // agrees with the sorted-listing endpoints by strict-`<` /
    // strict-`>` uniqueness under the label-pairwise-distinctness
    // contract clause (3)); the assertion catches a future implementor
    // whose override drifts from the natural lex-endpoint projections
    // (a permissive override that returns some interior variant, a
    // swapped override that returns the lex-max for `sorted_first()`, a
    // stale override that returns the wrong endpoint after a label
    // edit) loudly rather than silently bifurcating the lex-endpoint
    // anchor surface every downstream defaulter / diagnostic-boundary
    // / property-test consumer routes through. Sibling posture to
    // clauses (17) + (18) — clause (17) pins the sorted-typed-variant
    // listing element-wise against the sorted-labels projection under
    // the lex ordering, clause (18) pins the declaration-order (head,
    // tail) endpoint anchors against `T::ALL[0]` / `T::ALL[T::ALL.len()
    // - 1]`, this clause pins the lex-order (head, tail) endpoint
    // anchors against `T::sorted_variants()[0]` /
    // `T::sorted_variants()[T::sorted_variants().len() - 1]` so the
    // (declaration × lexicographic) × (head, tail) endpoint-anchor 2×2
    // matrix stays fully-pinned. The non-empty contract clause (1) +
    // clause (17)'s length equality guarantee `sorted_variants` is
    // non-empty; the subtraction `sorted_variants.len() - 1` never
    // underflows. Reuses the `sorted_variants` Vec clause (17) already
    // materialized so this clause pays no additional allocation.
    assert_eq!(
        T::sorted_first(),
        sorted_variants[0],
        "{type_name}: T::sorted_first() drifted from T::sorted_variants()[0] — the lexicographic-order head endpoint anchor no longer matches the natural label-keyed lex-min projection, so a downstream diagnostic-boundary / lex-defaulter consumer that binds `T::sorted_first()` as its canonical anchor would land on the wrong variant",
    );
    assert_eq!(
        T::sorted_last(),
        sorted_variants[sorted_variants.len() - 1],
        "{type_name}: T::sorted_last() drifted from T::sorted_variants()[T::sorted_variants().len() - 1] — the lexicographic-order tail endpoint anchor no longer matches the natural label-keyed lex-max projection, so a downstream diagnostic-boundary / bounded-loop-lex consumer that binds `T::sorted_last()` as its canonical anchor would land on the wrong variant",
    );
    // (20) — For every `i in 0..T::CARDINALITY`, `T::label_at(i)`
    // MUST equal `Some(T::ALL[i].label())`, AND
    // `T::label_at(T::CARDINALITY)` MUST equal `None`. The default
    // trait body composes `T::from_index(i).map(T::label)` verbatim
    // and satisfies both arms for free; the assertion catches a
    // future implementor whose override drifts the direct-label
    // projection arm (a permissive override that returns `Some(_)`
    // for an out-of-range index — folding an out-of-range serialized
    // index onto an in-range label at the direct-projection column
    // while `from_index` still rejects it, silently bifurcating the
    // `usize`-carrier decode axis; a strict override that returns
    // `None` for a valid in-range index, silently dropping labels at
    // the compact-decode boundary; a swapped override that recovers
    // the wrong label for a valid index, silently bifurcating the
    // (variant, `&'static str` label, `usize` index) projection
    // triangle at its fourth direct edge) loudly rather than silently
    // bifurcating the direct-label projection surface every downstream
    // compact-encoding / metrics-per-slot / bitset-observed-slot /
    // `tatara-check` per-slot diagnostic consumer routes through.
    // Sibling posture to clause (16) on the (typed variant, `&'static
    // str` label) return-projection axis of the `usize`-carrier
    // partition — clause (16) closes the (index → typed variant)
    // direct projection AND the out-of-range guard, this clause
    // closes the (index → `&'static str` label) direct projection AND
    // the SAME out-of-range guard on the second return-projection
    // column so the `usize`-carrier partition of the projection
    // triangle stays sound at both return-projection columns AND on
    // both the in-range accept AND the out-of-range reject partitions.
    for (i, &v) in T::ALL.iter().enumerate() {
        assert_eq!(
            T::label_at(i),
            Some(v.label()),
            "{type_name}: T::label_at({i}) drifted from Some(T::ALL[{i}].label()) — the direct (usize → &'static str label) projection no longer agrees with the natural from_index+label composition on the in-range accept arm, so a downstream compact-encoding / metrics-per-slot / bitset-observed-slot / tatara-check per-slot diagnostic consumer that binds `T::label_at(i)` as its direct-projection surface would render the wrong canonical label at index {i}",
        );
    }
    assert_eq!(
        T::label_at(T::CARDINALITY),
        None,
        "{type_name}: T::label_at(T::CARDINALITY) drifted from None — the direct (usize → &'static str label) projection accepted the first out-of-range index (T::CARDINALITY), so an out-of-range serialized index would fold onto an in-range canonical label on the direct-projection column while `from_index` still rejects it, silently bifurcating the usize-carrier decode axis of the projection triangle",
    );
    // (21) — For every variant `v` in `T::ALL`, `T::index_of_label(
    // v.label())` MUST equal `Some(v.index_of())`, AND
    // `T::index_of_label(<reserved probe>)` MUST equal `None`, AND
    // `T::index_of_label("")` MUST equal `None`. The default trait body
    // composes `T::find_by_label(s).map(T::index_of)` verbatim and
    // satisfies both arms for free; the assertion catches a future
    // implementor whose override drifts the direct-index projection arm
    // (a permissive override that returns `Some(_)` for a non-canonical
    // `&str` — folding an off-set string onto an in-range slot at the
    // direct-projection column while `find_by_label` still rejects it,
    // silently bifurcating the `&str`-carrier decode axis; a strict
    // override that returns `None` for a canonical variant's label,
    // silently dropping slots at the label-decode boundary; a swapped
    // override that recovers the wrong slot for a valid canonical
    // label, silently bifurcating the (variant, `&'static str` label,
    // `usize` index) projection triangle at its sixth direct edge)
    // loudly rather than silently bifurcating the direct-index
    // projection surface every downstream compact-encoder / metrics-
    // binner / `tatara-check` per-slot per-label diagnostic / LSP-hover
    // consumer routes through. Sibling posture to clauses (12) + (15)
    // — clause (12) closes the (`&str` → typed variant) `Option`-typed
    // direct projection, clause (15) closes the (typed variant →
    // `usize`) forward projection, this clause closes the (`&str` →
    // `usize`) direct projection composed from BOTH primitives AND
    // pins the alignment with the natural two-step composition on
    // every implementor. Together clauses (12) + (15) + (21) close
    // the `&str`-carrier partition of the projection triangle at both
    // the immediate (variant) decode column AND the further (index)
    // decode column, mirroring the way clauses (16) + (20) close the
    // `usize`-carrier partition at both its immediate (variant) decode
    // column AND its further (label) decode column.
    for &v in T::ALL {
        let label = v.label();
        assert_eq!(
            T::index_of_label(label),
            Some(v.index_of()),
            "{type_name}: T::index_of_label({label:?}) drifted from Some(v.index_of()) — the direct (&str label → usize index) projection no longer agrees with the natural find_by_label+index_of composition on the canonical accept arm, so a downstream compact-encoder / metrics-binner / tatara-check per-slot per-label diagnostic / LSP-hover consumer that binds `T::index_of_label(s)` as its direct-projection surface would render the wrong slot for the canonical label {label:?}",
        );
    }
    assert!(
        T::index_of_label(probe).is_none(),
        "{type_name}: T::index_of_label(<reserved probe>) returned Some(_) for an input outside the closed set — the direct (&str label → usize index) projection accepted a non-canonical string, silently folding an off-set string onto an in-range slot at the direct-projection column while `find_by_label` still rejects it, bifurcating the &str-carrier decode axis of the projection triangle",
    );
    assert!(
        T::index_of_label("").is_none(),
        "{type_name}: T::index_of_label(\"\") returned Some(_) — the direct (&str label → usize index) projection accepted the empty-string boundary that clause (4) reserves as structurally outside every closed set, folding the reserved boundary onto an in-range slot at the direct-projection column",
    );
    // (22) — For every variant `v` in `T::ALL`,
    // `T::sorted_index_of(v)` MUST equal the position of `v` in
    // `T::sorted_variants()`. The default trait body is a zero-alloc
    // label-keyed strict-`<` linear scan over `T::ALL`; under clause
    // (3)'s label-pairwise-distinctness contract, the count of labels
    // strictly less than `v.label()` equals the unique lex-order slot
    // `sorted_variants` places `v` in. The assertion catches a future
    // implementor whose override drifts the direct (variant → lex
    // position) projection — a permissive override that returns a
    // slot outside `0..T::CARDINALITY`, a swapped override that
    // returns the declaration-order slot instead of the lex-order
    // slot, a stale override that returns the wrong lex-order slot
    // after a label edit — loudly rather than silently bifurcating
    // the lex-position projection surface every downstream
    // lex-sorted-metrics-binner / lex-order-stable-wire-encoder /
    // bitset-observed-slot-lex-renderer consumer routes through.
    // Sibling posture to clauses (15) + (17) — clause (15) closes the
    // (variant → declaration-order position) forward projection AND
    // pins it against `T::ALL`'s position of `self`, clause (17)
    // closes the sorted-typed-variant listing element-wise against
    // the sorted-labels projection under the lex ordering, this
    // clause closes the (variant → lex-order position) forward
    // projection AND pins it against `T::sorted_variants()`'s
    // position of `self` so the (declaration, lex) × (variant →
    // position) forward-projection partition stays sound at BOTH
    // ordering axes AND on every canonical variant slot. Reuses the
    // `sorted_variants` Vec clauses (17) + (19) already materialized
    // so this clause pays no additional allocation on top of the
    // existing sweep's traversal surface.
    for &v in T::ALL {
        let expected_slot = sorted_variants
            .iter()
            .position(|w| core::mem::discriminant(w) == core::mem::discriminant(&v))
            .expect(
                "assert_closed_set_well_formed: T::sorted_variants() missing a canonical variant — clause (17)'s length equality should already have caught this",
            );
        assert_eq!(
            T::sorted_index_of(v),
            expected_slot,
            "{type_name}: T::sorted_index_of({v:?}) drifted from T::sorted_variants()'s position of the variant — the direct (variant → lex-order position) projection no longer agrees with the natural sorted_variants.position projection, so a downstream lex-sorted-metrics-binner / lex-order-stable-wire-encoder / bitset-observed-slot-lex-renderer consumer that binds `v.sorted_index_of()` as its direct-projection surface would render the wrong lex slot for {v:?}",
        );
    }
    // (23) — For every `i in 0..T::CARDINALITY`,
    // `T::from_sorted_index(i)` MUST equal `Some(T::sorted_variants()
    // [i])`, AND `T::from_sorted_index(T::CARDINALITY)` MUST equal
    // `None`. The (`usize` lex-order position → typed variant) inverse
    // projection agrees with direct `T::sorted_variants` slice indexing
    // on the in-range domain AND rejects the first out-of-range index.
    // Clauses (22) + (23) together pin the (typed variant ↔ `usize`
    // lex-order position) bijection at BOTH directions: clause (22)
    // covers the forward `variant.sorted_index_of() == i` projection,
    // this clause covers the inverse `T::from_sorted_index(i) ==
    // Some(v)` projection AND the out-of-range guard. The default
    // trait body's `Self::sorted_variants().get(i).copied()`
    // composition satisfies the clause for free; the assertion catches
    // a future implementor whose override drifts the bounded-decode
    // arm (a permissive override that returns `Some(_)` for an out-of-
    // range index, folding an out-of-range serialized lex slot onto an
    // in-range variant at the lex-order compact-decode boundary; a
    // strict override that returns `None` for a valid in-range lex
    // slot, silently dropping variants at the lex-order compact-decode
    // boundary; a swapped override that recovers the wrong variant for
    // a valid lex slot, silently bifurcating the (variant ↔ lex-order
    // position) round-trip — a `variant.sorted_index_of()` round-
    // tripped through `T::from_sorted_index(_)` would land at a
    // different variant, breaking every lex-order compact-encoder /
    // lex-order bitset consumer that expects the round-trip to close)
    // loudly rather than silently bifurcating the lex-order inverse-
    // decode surface every downstream lex-order compact-encoding /
    // lex-order-bitset-observed-variant / lex-order-lookup-table-
    // iteration consumer routes through. Sibling posture to clauses
    // (16) + (22) — clause (16) closes the (declaration-order index →
    // typed variant) inverse projection AND pins it against `T::ALL`
    // slice indexing, clause (22) closes the (variant → lex-order
    // position) forward projection AND pins it against
    // `T::sorted_variants()`'s position of `self`, this clause closes
    // the (lex-order index → typed variant) inverse projection AND
    // pins it against `T::sorted_variants()` slice indexing so the
    // (declaration, lex) × (position → variant) 1×2 inverse-projection
    // partition stays sound at BOTH ordering axes AND on every
    // canonical lex slot AND on the first out-of-range boundary.
    // Reuses the `sorted_variants` Vec clauses (17) + (19) + (22)
    // already materialized so this clause pays no additional
    // allocation on top of the existing sweep's traversal surface.
    for (i, &sorted_variant) in sorted_variants.iter().enumerate() {
        let decoded = T::from_sorted_index(i);
        assert_eq!(
            decoded,
            Some(sorted_variant),
            "{type_name}: T::from_sorted_index({i}) drifted from Some(T::sorted_variants()[{i}]) — the direct (lex-order index → typed variant) inverse projection no longer agrees with direct T::sorted_variants slice indexing on the in-range domain, so a downstream lex-order compact-decoder / lex-order-bitset-observed-variant / lex-order-lookup-table consumer that binds `T::from_sorted_index(i)` as its inverse-decode surface would recover the wrong variant for lex slot {i}",
        );
    }
    assert_eq!(
        T::from_sorted_index(T::CARDINALITY),
        None,
        "{type_name}: T::from_sorted_index(T::CARDINALITY) returned Some(_) — the (lex-order index → typed variant) inverse projection accepted the first out-of-range lex slot, folding an out-of-range serialized lex index onto an in-range variant at the lex-order compact-decode boundary. Clauses (14) + (23) together pin `T::CARDINALITY` as the first structurally-out-of-range lex slot — a permissive override that fails this pin bifurcates the lex-order compact-encoding surface every downstream consumer routes through.",
    );
    // (24) — For every `i in 0..T::CARDINALITY`,
    // `T::sorted_label_at(i)` MUST equal `Some(T::sorted_labels()[i])`,
    // AND `T::sorted_label_at(T::CARDINALITY)` MUST equal `None`. The
    // direct (`usize` lex-order position → `&'static str` canonical
    // label) projection agrees with the natural `from_sorted_index` +
    // `label` composition on the in-range domain (equivalently, with
    // direct `T::sorted_labels` slice indexing under lex ordering) AND
    // rejects the first out-of-range lex slot. Clauses (20) + (24)
    // together pin the (`usize` position → `&'static str` label) forward
    // projection at BOTH ordering axes: clause (20) covers the
    // declaration-ordering direct-label projection, this clause covers
    // the lex-ordering direct-label projection. The default trait body's
    // `from_sorted_index(i).map(label)` composition satisfies the clause
    // for free; the assertion catches a future implementor whose
    // override drifts the direct-label projection arm (a permissive
    // override that returns `Some(_)` for an out-of-range lex slot,
    // folding an out-of-range serialized lex index onto an in-range
    // label at the lex-order compact-decode boundary; a strict override
    // that returns `None` for a valid in-range lex slot, silently
    // dropping labels at the lex-order rendering boundary; a swapped
    // override that recovers the wrong label for a valid lex slot,
    // silently bifurcating the lex-axis projection triangle) loudly
    // rather than silently bifurcating the direct-label projection
    // surface every downstream lex-order compact-encoding / lex-sorted-
    // metrics-binner / bitset-observed-slot-lex-renderer / `tatara-check`
    // per-lex-slot diagnostic consumer routes through. Sibling posture
    // to clauses (20) + (23) — clause (20) closes the (declaration-order
    // index → `&'static str` label) direct projection, clause (23)
    // closes the (lex-order index → typed variant) inverse projection,
    // this clause closes the (lex-order index → `&'static str` label)
    // direct projection so the lex-axis projection triangle stays sound
    // at BOTH return-projection columns AND on both the in-range accept
    // AND the out-of-range reject partitions. Reuses the
    // `sorted_labels_reference` Vec clause (17) already materialized so
    // this clause pays no additional allocation on top of the existing
    // sweep's traversal surface.
    for (i, &expected_label) in sorted_labels_reference.iter().enumerate() {
        let decoded = T::sorted_label_at(i);
        assert_eq!(
            decoded,
            Some(expected_label),
            "{type_name}: T::sorted_label_at({i}) drifted from Some(T::sorted_labels()[{i}]) — the direct (lex-order index → `&'static str` label) projection no longer agrees with direct T::sorted_labels slice indexing on the in-range domain, so a downstream lex-order compact-decoder / lex-sorted-metrics-binner / bitset-observed-slot-lex-renderer consumer that binds `T::sorted_label_at(i)` as its direct-label projection surface would render the wrong label for lex slot {i}",
        );
    }
    assert_eq!(
        T::sorted_label_at(T::CARDINALITY),
        None,
        "{type_name}: T::sorted_label_at(T::CARDINALITY) returned Some(_) — the (lex-order index → `&'static str` label) direct projection accepted the first out-of-range lex slot, folding an out-of-range serialized lex index onto an in-range label at the lex-order compact-decode boundary. Clauses (14) + (24) together pin `T::CARDINALITY` as the first structurally-out-of-range lex slot — a permissive override that fails this pin bifurcates the lex-order rendering surface every downstream consumer routes through.",
    );
    // (25) — For every variant `v` in `T::ALL`,
    // `T::sorted_index_of_label(v.label())` MUST equal
    // `Some(v.sorted_index_of())`, AND
    // `T::sorted_index_of_label(<reserved probe>)` MUST equal `None`,
    // AND `T::sorted_index_of_label("")` MUST equal `None`. The default
    // trait body composes `T::find_by_label(s).map(T::sorted_index_of)`
    // verbatim and satisfies both arms for free; the assertion catches
    // a future implementor whose override drifts the direct-lex-slot
    // projection arm (a permissive override that returns `Some(_)` for
    // a non-canonical `&str` — folding an off-set string onto an
    // in-range lex slot at the direct-projection column while
    // `find_by_label` still rejects it, silently bifurcating the
    // `&str`-carrier decode axis on the lex-order projection column;
    // a strict override that returns `None` for a canonical variant's
    // label, silently dropping lex slots at the label-decode boundary;
    // a swapped override that recovers the wrong lex slot for a valid
    // canonical label, silently bifurcating the (typed variant,
    // `&'static str` label, `usize` position) lex-axis projection
    // triangle at its fourth direct edge) loudly rather than silently
    // bifurcating the direct-lex-slot projection surface every
    // downstream lex-sorted-metrics-binner / lex-order-compact-encoder
    // / `tatara-check` per-lex-slot per-label diagnostic / LSP-hover
    // consumer routes through. Sibling posture to clauses (21) + (22)
    // — clause (21) closes the (`&str` → declaration-order index) direct
    // projection AND pins the composition against
    // `find_by_label+index_of`, clause (22) closes the (variant →
    // lex-order position) forward projection AND pins it against
    // `T::sorted_variants()`'s position of `self`, this clause closes
    // the (`&str` → lex-order index) direct projection AND pins the
    // composition against `find_by_label+sorted_index_of` so the
    // (declaration, lex) × (`&str` → position) 1×2 direct-projection
    // partition stays sound at BOTH ordering axes AND on every canonical
    // variant label AND on both the reserved out-of-set probe boundary
    // AND the empty-string boundary. With clauses (20) + (21) + (22) +
    // (23) + (24) + (25) all in place, the (typed variant, `&'static
    // str` label, `usize` position) projection triangle stays direct-
    // projection closed at EVERY (input, output) pair on BOTH ordering
    // axes.
    for &v in T::ALL {
        let label = v.label();
        assert_eq!(
            T::sorted_index_of_label(label),
            Some(v.sorted_index_of()),
            "{type_name}: T::sorted_index_of_label({label:?}) drifted from Some(v.sorted_index_of()) — the direct (&str label → usize lex-order index) projection no longer agrees with the natural find_by_label+sorted_index_of composition on the canonical accept arm, so a downstream lex-sorted-metrics-binner / lex-order-compact-encoder / tatara-check per-lex-slot per-label diagnostic / LSP-hover consumer that binds `T::sorted_index_of_label(s)` as its direct-projection surface would render the wrong lex slot for the canonical label {label:?}",
        );
    }
    assert!(
        T::sorted_index_of_label(probe).is_none(),
        "{type_name}: T::sorted_index_of_label(<reserved probe>) returned Some(_) for an input outside the closed set — the direct (&str label → usize lex-order index) projection accepted a non-canonical string, silently folding an off-set string onto an in-range lex slot at the direct-projection column while `find_by_label` still rejects it, bifurcating the &str-carrier decode axis of the lex-order projection triangle",
    );
    assert!(
        T::sorted_index_of_label("").is_none(),
        "{type_name}: T::sorted_index_of_label(\"\") returned Some(_) — the direct (&str label → usize lex-order index) projection accepted the empty-string boundary that clause (4) reserves as structurally outside every closed set, folding the reserved boundary onto an in-range lex slot at the direct-projection column",
    );
    // (26) — For every variant `v` in `T::ALL`, `v.next()` MUST equal
    // `T::from_index(v.index_of() + 1)`, AND `v.prev()` MUST equal
    // `T::from_index(v.index_of() - 1)` when `v.index_of() > 0`, AND
    // `T::first().prev()` MUST equal `None`, AND `T::last().next()`
    // MUST equal `None`. The default trait bodies compose
    // `from_index(index_of(self) + 1)` (forward arm) and
    // `from_index(index_of(self) - 1)` guarded on `index_of(self) > 0`
    // (backward arm) verbatim and satisfy both arms for free; the
    // assertion catches a future implementor whose override drifts
    // either neighbor projection (a permissive forward override that
    // returns `Some(_)` at the tail — folding a tail-boundary walk
    // onto a wraparound to the head at the forward-projection column
    // while the composed `+ 1` arithmetic would return `None`; a
    // permissive backward override that returns `Some(_)` at the head
    // — folding a head-boundary walk onto a wraparound to the tail
    // through the `usize` underflow the guard prevents; a swapped
    // override that returns the predecessor for `next` AND the
    // successor for `prev`, silently inverting the traversal
    // direction; a stale override that returns the wrong neighbor
    // after a variant-listing edit) loudly rather than silently
    // bifurcating the neighbor-projection surface every downstream
    // state-machine iterator / saga-step engine / truth-table
    // property test / phase-fold reducer consumer routes through.
    // Sibling posture to clauses (15) + (16) + (18) — clauses (15) +
    // (16) pin the (typed variant ↔ `usize` array index) bijection at
    // BOTH directions, clause (18) pins the (head, tail) endpoint
    // anchors against `T::ALL[0]` / `T::ALL[T::ALL.len() - 1]`, this
    // clause pins the (forward, backward) neighbor projections against
    // the composition of both bijection arms AND pins the endpoint-
    // boundary `None` guards on both direction arms — so the closed-
    // set traversal surface stays sound at BOTH direction arms AND on
    // the shared endpoint-anchor fixpoints (`T::last().next() ==
    // None`, `T::first().prev() == None`).
    for &v in T::ALL {
        let i = v.index_of();
        let expected_next = T::from_index(i + 1);
        assert_eq!(
            v.next(),
            expected_next,
            "{type_name}: {v:?}.next() drifted from T::from_index({v:?}.index_of() + 1) — the direct (variant → forward neighbor) projection no longer agrees with the natural index_of+from_index composition, so a downstream state-machine iterator / saga-step engine / phase-fold reducer / LSP wraparound-cursor consumer that binds `v.next()` as its forward-traversal surface would land on the wrong neighbor for {v:?}",
        );
        if i > 0 {
            let expected_prev = T::from_index(i - 1);
            assert_eq!(
                v.prev(),
                expected_prev,
                "{type_name}: {v:?}.prev() drifted from T::from_index({v:?}.index_of() - 1) — the direct (variant → backward neighbor) projection no longer agrees with the natural index_of+from_index composition on an interior slot, so a downstream state-machine iterator / saga-step engine / phase-fold reducer / LSP wraparound-cursor consumer that binds `v.prev()` as its backward-traversal surface would land on the wrong neighbor for {v:?}",
            );
        }
    }
    assert_eq!(
        T::first().prev(),
        None,
        "{type_name}: T::first().prev() returned Some(_) — the (variant → backward neighbor) projection accepted the head-endpoint boundary, silently folding a head-boundary walk onto a wraparound to the tail while the natural `usize` underflow guard should return `None`. Clauses (18) + (26) together pin `T::first().prev() == None` as the structural fixpoint the head-endpoint anchor and the backward-neighbor axis share",
    );
    assert_eq!(
        T::last().next(),
        None,
        "{type_name}: T::last().next() returned Some(_) — the (variant → forward neighbor) projection accepted the tail-endpoint boundary, silently folding a tail-boundary walk onto a wraparound to the head while the natural `<[T]>::get` bounded-index projection should return `None`. Clauses (18) + (26) together pin `T::last().next() == None` as the structural fixpoint the tail-endpoint anchor and the forward-neighbor axis share",
    );
    // (27) — For every variant `v` in `T::ALL`, `v.sorted_next()` MUST
    // equal `T::from_sorted_index(v.sorted_index_of() + 1)`, AND
    // `v.sorted_prev()` MUST equal
    // `T::from_sorted_index(v.sorted_index_of() - 1)` when
    // `v.sorted_index_of() > 0`, AND `T::sorted_first().sorted_prev()`
    // MUST equal `None`, AND `T::sorted_last().sorted_next()` MUST
    // equal `None`. The default trait bodies compose
    // `from_sorted_index(sorted_index_of(self) + 1)` (forward arm) and
    // `from_sorted_index(sorted_index_of(self) - 1)` guarded on
    // `sorted_index_of(self) > 0` (backward arm) verbatim and satisfy
    // both arms for free; the assertion catches a future implementor
    // whose override drifts either lex-neighbor projection (a
    // permissive forward override that returns `Some(_)` at the
    // lex-tail — folding a lex-tail-boundary walk onto a wraparound
    // to the lex-head at the forward-lex-projection column while the
    // composed `+ 1` arithmetic would return `None`; a permissive
    // backward override that returns `Some(_)` at the lex-head —
    // folding a lex-head-boundary walk onto a wraparound to the
    // lex-tail through the `usize` underflow the guard prevents; a
    // swapped override that returns the lex-predecessor for
    // `sorted_next` AND the lex-successor for `sorted_prev`, silently
    // inverting the lex-traversal direction; a stale override that
    // returns the wrong lex-neighbor after a variant-listing edit
    // reorders the lex partition) loudly rather than silently
    // bifurcating the lex-neighbor-projection surface every
    // downstream alphabetized-completion LSP cursor / lex-sorted
    // `tatara-check` per-slot diagnostic renderer / lex-order
    // compact-encoded wire codec / Sekiban audit binner /
    // alphabetized property-test sweep consumer routes through.
    // Sibling posture to clauses (22) + (23) + (26) — clauses (22) +
    // (23) pin the (typed variant ↔ `usize` lex-order position)
    // bijection at BOTH directions, clause (26) pins the (forward,
    // backward) declaration-order neighbor projections against the
    // composition of both declaration-axis bijection arms AND pins
    // the endpoint-boundary `None` guards on both direction arms,
    // this clause pins the (forward, backward) LEX-order neighbor
    // projections against the composition of both lex-axis bijection
    // arms AND pins the lex-endpoint-boundary `None` guards on both
    // direction arms — so the closed-set neighbor surface stays sound
    // at BOTH direction arms AND on BOTH ordering axes AND on the
    // shared lex-endpoint-anchor fixpoints
    // (`T::sorted_last().sorted_next() == None`,
    // `T::sorted_first().sorted_prev() == None`). Clauses (26) + (27)
    // together close the (declaration × lex) × (forward, backward)
    // 2×2 closed-set neighbor matrix at ALL FOUR direct
    // projection surfaces AND at ALL FOUR endpoint-boundary
    // fixpoints.
    for &v in T::ALL {
        let i = v.sorted_index_of();
        let expected_sorted_next = T::from_sorted_index(i + 1);
        assert_eq!(
            v.sorted_next(),
            expected_sorted_next,
            "{type_name}: {v:?}.sorted_next() drifted from T::from_sorted_index({v:?}.sorted_index_of() + 1) — the direct (variant → forward lex-neighbor) projection no longer agrees with the natural sorted_index_of+from_sorted_index composition, so a downstream alphabetized-completion LSP cursor / lex-sorted tatara-check per-slot diagnostic renderer / lex-order compact-encoded wire codec / Sekiban audit binner consumer that binds `v.sorted_next()` as its forward-lex-traversal surface would land on the wrong lex-neighbor for {v:?}",
        );
        if i > 0 {
            let expected_sorted_prev = T::from_sorted_index(i - 1);
            assert_eq!(
                v.sorted_prev(),
                expected_sorted_prev,
                "{type_name}: {v:?}.sorted_prev() drifted from T::from_sorted_index({v:?}.sorted_index_of() - 1) — the direct (variant → backward lex-neighbor) projection no longer agrees with the natural sorted_index_of+from_sorted_index composition on an interior lex slot, so a downstream alphabetized-completion LSP cursor / lex-sorted tatara-check per-slot diagnostic renderer / lex-order compact-encoded wire codec / Sekiban audit binner consumer that binds `v.sorted_prev()` as its backward-lex-traversal surface would land on the wrong lex-neighbor for {v:?}",
            );
        }
    }
    assert_eq!(
        T::sorted_first().sorted_prev(),
        None,
        "{type_name}: T::sorted_first().sorted_prev() returned Some(_) — the (variant → backward lex-neighbor) projection accepted the lex-head-endpoint boundary, silently folding a lex-head-boundary walk onto a wraparound to the lex-tail while the natural `usize` underflow guard should return `None`. Clauses (26) + (27) together pin `T::sorted_first().sorted_prev() == None` as the structural fixpoint the lex-head-endpoint anchor and the backward-lex-neighbor axis share, mirroring `T::first().prev() == None` one ordering axis over",
    );
    assert_eq!(
        T::sorted_last().sorted_next(),
        None,
        "{type_name}: T::sorted_last().sorted_next() returned Some(_) — the (variant → forward lex-neighbor) projection accepted the lex-tail-endpoint boundary, silently folding a lex-tail-boundary walk onto a wraparound to the lex-head while the natural bounded-index projection should return `None`. Clauses (26) + (27) together pin `T::sorted_last().sorted_next() == None` as the structural fixpoint the lex-tail-endpoint anchor and the forward-lex-neighbor axis share, mirroring `T::last().next() == None` one ordering axis over",
    );
    // (28) — For every variant `v` in `T::ALL`, `v.cycle_next()` MUST
    // equal `v.next().unwrap_or(T::first())`, AND `v.cycle_prev()`
    // MUST equal `v.prev().unwrap_or(T::last())`, AND
    // `T::last().cycle_next()` MUST equal `T::first()`, AND
    // `T::first().cycle_prev()` MUST equal `T::last()`. The default
    // trait bodies compose `next().unwrap_or(first())` (forward-
    // wrapping arm) and `prev().unwrap_or(last())` (backward-
    // wrapping arm) verbatim and satisfy both arms for free; the
    // assertion catches a future implementor whose override drifts
    // either wrapping-neighbor projection (a permissive forward-
    // wrapping override that returns some interior variant at the
    // tail rather than the head anchor — folding a cyclic walk onto
    // an unbounded interior loop while the composed `next()
    // .unwrap_or(first())` shape would fold the tail onto
    // `T::first()`; a permissive backward-wrapping override that
    // returns some interior variant at the head rather than the tail
    // anchor — folding a cyclic backward walk onto an unbounded
    // interior loop through the mismatched fallback; a swapped
    // override that returns the wrapping-predecessor for `cycle_next`
    // AND the wrapping-successor for `cycle_prev`, silently inverting
    // the cyclic traversal direction; a stale override that returns
    // the wrong wrapping-neighbor after a variant-listing edit
    // reorders `T::ALL`) loudly rather than silently bifurcating the
    // wrapping-neighbor-projection surface every downstream
    // wraparound-cursor LSP completion renderer / UI mode selector /
    // round-robin scheduler / declaration-order carousel widget /
    // per-tick animation frame picker consumer routes through.
    // Sibling posture to clauses (18) + (26) — clause (18) pins the
    // (head, tail) endpoint anchors against `T::ALL[0]` /
    // `T::ALL[T::ALL.len() - 1]`, clause (26) pins the (forward,
    // backward) bounded-neighbor projections against the composition
    // of both bijection arms AND pins the endpoint-boundary `None`
    // guards on both direction arms, this clause pins the (forward,
    // backward) WRAPPING-neighbor projections against the composition
    // of the bounded-neighbor arm with the SIBLING-direction endpoint
    // anchor AND pins the endpoint-boundary wraparound folds on both
    // direction arms — so the closed-set declaration-axis neighbor
    // surface stays sound on BOTH return-type arms (Option-typed /
    // wrapping) AND on BOTH direction arms AND on the shared
    // endpoint-anchor wraparound fixpoints (`T::last().cycle_next()
    // == T::first()`, `T::first().cycle_prev() == T::last()`).
    // Clauses (26) + (28) together close the (Option-typed, wrapping)
    // × (forward, backward) 2×2 declaration-axis neighbor matrix at
    // ALL FOUR direct projection surfaces AND at ALL FOUR endpoint-
    // boundary fixpoints.
    for &v in T::ALL {
        let expected_cycle_next = v.next().unwrap_or_else(T::first);
        assert_eq!(
            v.cycle_next(),
            expected_cycle_next,
            "{type_name}: {v:?}.cycle_next() drifted from {v:?}.next().unwrap_or(T::first()) — the direct (variant → wrapping-forward neighbor) projection no longer agrees with the natural next+first composition, so a downstream wraparound-cursor LSP completion renderer / UI mode selector / round-robin scheduler / declaration-order carousel widget consumer that binds `v.cycle_next()` as its forward-wrapping-traversal surface would land on the wrong wrapping-neighbor for {v:?}",
        );
        let expected_cycle_prev = v.prev().unwrap_or_else(T::last);
        assert_eq!(
            v.cycle_prev(),
            expected_cycle_prev,
            "{type_name}: {v:?}.cycle_prev() drifted from {v:?}.prev().unwrap_or(T::last()) — the direct (variant → wrapping-backward neighbor) projection no longer agrees with the natural prev+last composition, so a downstream wraparound-cursor LSP completion renderer / UI mode selector / round-robin scheduler / declaration-order carousel widget consumer that binds `v.cycle_prev()` as its backward-wrapping-traversal surface would land on the wrong wrapping-neighbor for {v:?}",
        );
    }
    assert_eq!(
        T::last().cycle_next(),
        T::first(),
        "{type_name}: T::last().cycle_next() returned a variant other than T::first() — the (variant → wrapping-forward neighbor) projection failed to fold the tail-endpoint boundary onto the head-endpoint anchor while the natural next+first composition should return T::first(). Clauses (18) + (28) together pin `T::last().cycle_next() == T::first()` as the structural wraparound fixpoint the tail-endpoint anchor and the forward-wrapping-neighbor axis share, mirroring `T::last().next() == None` one return-type axis over",
    );
    assert_eq!(
        T::first().cycle_prev(),
        T::last(),
        "{type_name}: T::first().cycle_prev() returned a variant other than T::last() — the (variant → wrapping-backward neighbor) projection failed to fold the head-endpoint boundary onto the tail-endpoint anchor while the natural prev+last composition should return T::last(). Clauses (18) + (28) together pin `T::first().cycle_prev() == T::last()` as the structural wraparound fixpoint the head-endpoint anchor and the backward-wrapping-neighbor axis share, mirroring `T::first().prev() == None` one return-type axis over",
    );
    // (29) — For every variant `v` in `T::ALL`, `v.cycle_sorted_next()`
    // MUST equal `v.sorted_next().unwrap_or(T::sorted_first())`, AND
    // `v.cycle_sorted_prev()` MUST equal
    // `v.sorted_prev().unwrap_or(T::sorted_last())`, AND
    // `T::sorted_last().cycle_sorted_next()` MUST equal
    // `T::sorted_first()`, AND `T::sorted_first().cycle_sorted_prev()`
    // MUST equal `T::sorted_last()`. The default trait bodies compose
    // `sorted_next().unwrap_or(sorted_first())` (forward-wrapping-lex
    // arm) and `sorted_prev().unwrap_or(sorted_last())` (backward-
    // wrapping-lex arm) verbatim and satisfy both arms for free; the
    // assertion catches a future implementor whose override drifts
    // either wrapping-lex-neighbor projection (a permissive forward-
    // wrapping-lex override that returns some interior variant at the
    // lex tail rather than the lex-head anchor — folding a lex-cyclic
    // walk onto an unbounded interior loop while the composed
    // `sorted_next().unwrap_or(sorted_first())` shape would fold the
    // lex tail onto `T::sorted_first()`; a permissive backward-
    // wrapping-lex override that returns some interior variant at the
    // lex head rather than the lex-tail anchor — folding a lex-cyclic
    // backward walk onto an unbounded interior loop through the
    // mismatched fallback; a swapped override that returns the
    // wrapping-lex-predecessor for `cycle_sorted_next` AND the
    // wrapping-lex-successor for `cycle_sorted_prev`, silently
    // inverting the cyclic lex-traversal direction; a stale override
    // that returns the wrong wrapping-lex-neighbor after a variant-
    // listing edit reorders the lex partition) loudly rather than
    // silently bifurcating the wrapping-lex-neighbor-projection surface
    // every downstream alphabetized wraparound-cursor LSP completion
    // renderer / alphabetized UI mode selector / alphabetized round-
    // robin scheduler / lex-order carousel widget / alphabetized
    // per-tick animation frame picker consumer routes through. Sibling
    // posture to clauses (18) + (26) + (27) + (28) — clause (18) pins
    // the (head, tail) endpoint anchors against `T::ALL[0]` /
    // `T::ALL[T::ALL.len() - 1]`, clauses (26) + (27) pin the (forward,
    // backward) bounded-neighbor projections on BOTH ordering axes AND
    // pin the endpoint-boundary `None` guards on all four direction
    // arms, clause (28) pins the (forward, backward) WRAPPING-neighbor
    // projections on the DECLARATION axis against the composition of
    // the bounded-neighbor arm with the sibling-direction endpoint
    // anchor AND pins the endpoint-boundary wraparound folds on both
    // declaration-axis direction arms, this clause pins the (forward,
    // backward) WRAPPING-neighbor projections on the LEX axis against
    // the composition of the lex-bounded-neighbor arm with the
    // sibling-direction lex-endpoint anchor AND pins the lex-endpoint-
    // boundary wraparound folds on both lex-axis direction arms — so
    // the closed-set neighbor surface stays sound on BOTH return-type
    // arms (Option-typed / wrapping) AND on BOTH direction arms AND on
    // BOTH ordering axes AND on the shared lex-endpoint-anchor
    // wraparound fixpoints (`T::sorted_last().cycle_sorted_next() ==
    // T::sorted_first()`, `T::sorted_first().cycle_sorted_prev() ==
    // T::sorted_last()`). Clauses (26) + (27) + (28) + (29) together
    // close the (Option-typed, wrapping) × (declaration, lex) ×
    // (forward, backward) 2×2×2 = 8-corner closed-set neighbor cube at
    // ALL EIGHT direct projection surfaces AND at ALL EIGHT endpoint-
    // boundary fixpoints.
    for &v in T::ALL {
        let expected_cycle_sorted_next = v.sorted_next().unwrap_or_else(T::sorted_first);
        assert_eq!(
            v.cycle_sorted_next(),
            expected_cycle_sorted_next,
            "{type_name}: {v:?}.cycle_sorted_next() drifted from {v:?}.sorted_next().unwrap_or(T::sorted_first()) — the direct (variant → wrapping-forward lex-neighbor) projection no longer agrees with the natural sorted_next+sorted_first composition, so a downstream alphabetized wraparound-cursor LSP completion renderer / alphabetized UI mode selector / alphabetized round-robin scheduler / lex-order carousel widget consumer that binds `v.cycle_sorted_next()` as its forward-wrapping-lex-traversal surface would land on the wrong wrapping-lex-neighbor for {v:?}",
        );
        let expected_cycle_sorted_prev = v.sorted_prev().unwrap_or_else(T::sorted_last);
        assert_eq!(
            v.cycle_sorted_prev(),
            expected_cycle_sorted_prev,
            "{type_name}: {v:?}.cycle_sorted_prev() drifted from {v:?}.sorted_prev().unwrap_or(T::sorted_last()) — the direct (variant → wrapping-backward lex-neighbor) projection no longer agrees with the natural sorted_prev+sorted_last composition, so a downstream alphabetized wraparound-cursor LSP completion renderer / alphabetized UI mode selector / alphabetized round-robin scheduler / lex-order carousel widget consumer that binds `v.cycle_sorted_prev()` as its backward-wrapping-lex-traversal surface would land on the wrong wrapping-lex-neighbor for {v:?}",
        );
    }
    assert_eq!(
        T::sorted_last().cycle_sorted_next(),
        T::sorted_first(),
        "{type_name}: T::sorted_last().cycle_sorted_next() returned a variant other than T::sorted_first() — the (variant → wrapping-forward lex-neighbor) projection failed to fold the lex-tail-endpoint boundary onto the lex-head-endpoint anchor while the natural sorted_next+sorted_first composition should return T::sorted_first(). Clauses (18) + (29) together pin `T::sorted_last().cycle_sorted_next() == T::sorted_first()` as the structural lex-wraparound fixpoint the lex-tail-endpoint anchor and the forward-wrapping-lex-neighbor axis share, mirroring `T::sorted_last().sorted_next() == None` one return-type axis over AND `T::last().cycle_next() == T::first()` one ordering axis over",
    );
    assert_eq!(
        T::sorted_first().cycle_sorted_prev(),
        T::sorted_last(),
        "{type_name}: T::sorted_first().cycle_sorted_prev() returned a variant other than T::sorted_last() — the (variant → wrapping-backward lex-neighbor) projection failed to fold the lex-head-endpoint boundary onto the lex-tail-endpoint anchor while the natural sorted_prev+sorted_last composition should return T::sorted_last(). Clauses (18) + (29) together pin `T::sorted_first().cycle_sorted_prev() == T::sorted_last()` as the structural lex-wraparound fixpoint the lex-head-endpoint anchor and the backward-wrapping-lex-neighbor axis share, mirroring `T::sorted_first().sorted_prev() == None` one return-type axis over AND `T::first().cycle_prev() == T::last()` one ordering axis over",
    );
    // (30) — For every variant `v` in `T::ALL`, `v.is_first()` MUST
    // equal `v.index_of() == 0`, AND `v.is_last()` MUST equal
    // `v.index_of() + 1 == T::CARDINALITY`, AND
    // `T::first().is_first()` MUST be `true`, AND
    // `T::last().is_last()` MUST be `true`. The default trait bodies
    // compose `index_of(self) == 0` (head-arm) and
    // `index_of(self) + 1 == Self::CARDINALITY` (tail-arm) verbatim
    // and satisfy both arms for free; the assertion catches a future
    // implementor whose override drifts either endpoint-membership
    // projection (a permissive head override that returns `true` on
    // an interior slot — folding a bounded-loop guard onto the wrong
    // partition of `T::ALL` and silently short-circuiting an iterator
    // before it reaches the head-endpoint; a permissive tail override
    // that returns `true` on an interior slot — folding a
    // termination-detection consumer onto the wrong partition of
    // `T::ALL` and silently short-circuiting the iterator before it
    // reaches the tail-endpoint; a swapped override that returns
    // `is_last`'s answer for `is_first` AND vice-versa, silently
    // inverting the (head, tail) membership partition; a stale
    // override that returns the wrong endpoint membership after a
    // variant-listing edit shifts the slot alignment) loudly rather
    // than silently bifurcating the endpoint-membership projection
    // surface every downstream bounded-loop guard / saga-step
    // engine / truth-table property test / wraparound-cursor
    // renderer / termination-detection consumer routes through.
    // Sibling posture to clauses (15) + (18) — clause (15) pins the
    // (variant → declaration-order position) forward projection
    // against `T::ALL`'s position of `self`, clause (18) pins the
    // (head, tail) endpoint anchors against `T::ALL[0]` /
    // `T::ALL[T::ALL.len() - 1]`, this clause pins the (bool head-
    // membership, bool tail-membership) projections against the
    // composition of the forward-position projection with the const-
    // visible variant count AND pins the endpoint-anchor `true`
    // fixpoints on both direction arms — so the closed-set
    // declaration-axis endpoint surface stays sound on BOTH return-
    // type arms (`Self`-typed anchor / `bool`-typed membership) AND
    // on BOTH direction arms AND on the shared endpoint-anchor
    // membership fixpoints (`T::first().is_first() == true`,
    // `T::last().is_last() == true`). Clauses (18) + (30) together
    // close the (return-type × direction) 2×2 declaration-axis
    // endpoint matrix at ALL FOUR direct projection surfaces AND at
    // BOTH endpoint-anchor membership fixpoints.
    for &v in T::ALL {
        let i = v.index_of();
        assert_eq!(
            v.is_first(),
            i == 0,
            "{type_name}: {v:?}.is_first() drifted from {v:?}.index_of() == 0 — the direct (variant → head-membership bool) projection no longer agrees with the natural `index_of == 0` composition, so a downstream bounded-loop guard / saga-step engine / truth-table property test / wraparound-cursor renderer consumer that binds `v.is_first()` as its head-boundary query surface would answer the wrong `bool` for {v:?}",
        );
        assert_eq!(
            v.is_last(),
            i + 1 == T::CARDINALITY,
            "{type_name}: {v:?}.is_last() drifted from {v:?}.index_of() + 1 == T::CARDINALITY — the direct (variant → tail-membership bool) projection no longer agrees with the natural `index_of + 1 == CARDINALITY` composition, so a downstream termination-detection / bounded-loop-terminator / saga-step engine / wraparound-cursor renderer consumer that binds `v.is_last()` as its tail-boundary query surface would answer the wrong `bool` for {v:?}",
        );
    }
    assert!(
        T::first().is_first(),
        "{type_name}: T::first().is_first() returned false — the (variant → head-membership bool) projection failed to fire on the declaration-order head endpoint anchor while the natural `index_of == 0` composition should return true. Clauses (18) + (30) together pin `T::first().is_first() == true` as the structural fixpoint the head-endpoint anchor and the head-membership predicate axis share",
    );
    assert!(
        T::last().is_last(),
        "{type_name}: T::last().is_last() returned false — the (variant → tail-membership bool) projection failed to fire on the declaration-order tail endpoint anchor while the natural `index_of + 1 == CARDINALITY` composition should return true. Clauses (18) + (30) together pin `T::last().is_last() == true` as the structural fixpoint the tail-endpoint anchor and the tail-membership predicate axis share, mirroring `T::first().is_first() == true` one direction axis over",
    );
    // (31) — For every variant `v` in `T::ALL`, `v.is_sorted_first()`
    // MUST equal `v.sorted_index_of() == 0`, AND `v.is_sorted_last()`
    // MUST equal `v.sorted_index_of() + 1 == T::CARDINALITY`, AND
    // `T::sorted_first().is_sorted_first()` MUST be `true`, AND
    // `T::sorted_last().is_sorted_last()` MUST be `true`. The default
    // trait bodies compose `sorted_index_of(self) == 0` (lex head-arm)
    // and `sorted_index_of(self) + 1 == Self::CARDINALITY` (lex tail-
    // arm) verbatim and satisfy both arms for free; the assertion
    // catches a future implementor whose override drifts either lex-
    // endpoint-membership projection (a permissive lex head override
    // that returns `true` on an interior lex slot — folding a
    // bounded-lex-loop guard onto the wrong partition of the
    // alphabetized listing and silently short-circuiting an
    // alphabetized iterator before it reaches the lex-head endpoint;
    // a permissive lex tail override that returns `true` on an
    // interior lex slot — folding a lex-termination-detection
    // consumer onto the wrong partition; a swapped override that
    // returns `is_sorted_last`'s answer for `is_sorted_first` AND
    // vice-versa, silently inverting the (lex-head, lex-tail)
    // membership partition; a stale override that returns the wrong
    // endpoint membership after a label edit shifts the lex slot
    // alignment) loudly rather than silently bifurcating the lex-
    // endpoint-membership projection surface every downstream
    // alphabetized-LSP-cursor / lex-anchored-diagnostic-renderer /
    // lex-slot-metrics-tagger / alphabetized-default-deserializer
    // consumer routes through. Sibling posture to clause (30) one
    // ordering-axis over on the (declaration, lex) partition of the
    // (return-type × ordering × direction) 2×2×2 endpoint cube —
    // clause (30) pins the (bool head-membership, bool tail-
    // membership) projections on the DECLARATION axis, this clause
    // pins the (bool lex-head-membership, bool lex-tail-membership)
    // projections on the LEX axis. Clauses (18) + (19) + (30) + (31)
    // together close the (return-type × ordering × direction) 2×2×2
    // = 8-corner endpoint cube at ALL EIGHT direct projection
    // surfaces AND at ALL FOUR endpoint-anchor membership fixpoints
    // (`T::first().is_first()`, `T::last().is_last()`,
    // `T::sorted_first().is_sorted_first()`,
    // `T::sorted_last().is_sorted_last()` — one per corner of the
    // (ordering × direction) 2×2 anchor matrix).
    for &v in T::ALL {
        let i = v.sorted_index_of();
        assert_eq!(
            v.is_sorted_first(),
            i == 0,
            "{type_name}: {v:?}.is_sorted_first() drifted from {v:?}.sorted_index_of() == 0 — the direct (variant → lex-head-membership bool) projection no longer agrees with the natural `sorted_index_of == 0` composition, so a downstream alphabetized-LSP-cursor / lex-anchored-diagnostic-renderer / lex-slot-metrics-tagger / alphabetized-default-deserializer consumer that binds `v.is_sorted_first()` as its lex-head-boundary query surface would answer the wrong `bool` for {v:?}",
        );
        assert_eq!(
            v.is_sorted_last(),
            i + 1 == T::CARDINALITY,
            "{type_name}: {v:?}.is_sorted_last() drifted from {v:?}.sorted_index_of() + 1 == T::CARDINALITY — the direct (variant → lex-tail-membership bool) projection no longer agrees with the natural `sorted_index_of + 1 == CARDINALITY` composition, so a downstream alphabetized-termination-detector / lex-bounded-loop-terminator / lex-anchored-diagnostic-renderer consumer that binds `v.is_sorted_last()` as its lex-tail-boundary query surface would answer the wrong `bool` for {v:?}",
        );
    }
    assert!(
        T::sorted_first().is_sorted_first(),
        "{type_name}: T::sorted_first().is_sorted_first() returned false — the (variant → lex-head-membership bool) projection failed to fire on the lex-order head endpoint anchor while the natural `sorted_index_of == 0` composition should return true. Clauses (19) + (31) together pin `T::sorted_first().is_sorted_first() == true` as the structural fixpoint the lex-head-endpoint anchor and the lex-head-membership predicate axis share, mirroring `T::first().is_first() == true` one ordering axis over",
    );
    assert!(
        T::sorted_last().is_sorted_last(),
        "{type_name}: T::sorted_last().is_sorted_last() returned false — the (variant → lex-tail-membership bool) projection failed to fire on the lex-order tail endpoint anchor while the natural `sorted_index_of + 1 == CARDINALITY` composition should return true. Clauses (19) + (31) together pin `T::sorted_last().is_sorted_last() == true` as the structural fixpoint the lex-tail-endpoint anchor and the lex-tail-membership predicate axis share, mirroring `T::last().is_last() == true` one ordering axis over and completing the 8-corner endpoint cube",
    );
    // (32) — For every variant `v` in `T::ALL`, `v.is_endpoint()` MUST
    // equal `v.is_first() || v.is_last()`, AND `v.is_interior()` MUST
    // equal `!(v.is_first() || v.is_last())`, AND
    // `is_endpoint(v) != is_interior(v)` (exhaustive complementarity),
    // AND `T::first().is_endpoint()` MUST be `true`, AND
    // `T::last().is_endpoint()` MUST be `true`, AND
    // `T::first().is_interior()` MUST be `false`, AND
    // `T::last().is_interior()` MUST be `false`. The default trait
    // bodies compose `is_first(self) || is_last(self)` (boundary arm)
    // and `!is_endpoint(self)` (interior arm) verbatim and satisfy
    // both arms for free; the assertion catches a future implementor
    // whose override drifts either boundary-membership projection (a
    // permissive endpoint override that returns `true` on an interior
    // slot — folding a boundary-glyph-emit / audit-event / bounded-
    // iteration-guard consumer onto the wrong partition of `T::ALL`
    // and silently short-circuiting the strict-interior arm; a
    // permissive interior override that returns `true` on an endpoint
    // slot — folding a strictly-interior renderer onto the wrong
    // partition; a swapped override that returns `is_interior`'s
    // answer for `is_endpoint` AND vice-versa, silently inverting the
    // (endpoint, interior) partition — passing the exhaustive
    // complementarity assertion but drifting from the point-
    // membership composition on every variant; a stale override that
    // fails the complementarity assertion — returns the SAME `bool`
    // for both predicates on some variant, breaking the (endpoint XOR
    // interior) partition contract every downstream boundary-vs-
    // interior consumer relies on) loudly rather than silently
    // bifurcating the boundary-membership projection surface every
    // downstream shared-endpoint-badge renderer / boundary-audit-
    // event emitter / strictly-interior phase-fold reducer / carousel-
    // boundary-glyph consumer routes through. Sibling posture to
    // clauses (18) + (30) — clause (18) pins the (head, tail)
    // endpoint anchors against `T::ALL[0]` / `T::ALL[T::ALL.len() -
    // 1]`, clause (30) pins the (bool head-membership, bool tail-
    // membership) projections against the composition of the forward-
    // position projection with the const-visible variant count, this
    // clause pins the (bool boundary-membership, bool interior-
    // membership) partition against the composition of the point-
    // membership pair under `||` AND its negation AND pins the
    // exhaustive complementarity `is_endpoint XOR is_interior` on
    // every variant. Clauses (18) + (30) + (32) together open the
    // (predicate-flavor × direction) 2×2 declaration-axis endpoint
    // matrix at ALL FOUR direct projection surfaces (`Self`-typed
    // anchor / `bool`-typed point membership / `bool`-typed boundary
    // membership / `bool`-typed interior membership) AND at BOTH
    // endpoint-anchor boundary-fixpoints (`T::first().is_endpoint()`,
    // `T::last().is_endpoint()`) AND at BOTH endpoint-anchor
    // interior-fixpoints (`T::first().is_interior() == false`,
    // `T::last().is_interior() == false`).
    for &v in T::ALL {
        let expected_endpoint = v.is_first() || v.is_last();
        assert_eq!(
            v.is_endpoint(),
            expected_endpoint,
            "{type_name}: {v:?}.is_endpoint() drifted from {v:?}.is_first() || {v:?}.is_last() — the direct (variant → boundary-membership bool) projection no longer agrees with the natural `is_first || is_last` composition, so a downstream shared-endpoint-badge renderer / boundary-audit-event emitter / bounded-iteration guard consumer that binds `v.is_endpoint()` as its structural-boundary query surface would answer the wrong `bool` for {v:?}",
        );
        assert_eq!(
            v.is_interior(),
            !expected_endpoint,
            "{type_name}: {v:?}.is_interior() drifted from !({v:?}.is_first() || {v:?}.is_last()) — the direct (variant → interior-membership bool) projection no longer agrees with the natural `!is_endpoint` composition, so a downstream strictly-interior phase-fold reducer / strictly-interior alphabetized-completion pass / boundary-hidden renderer consumer that binds `v.is_interior()` as its strict-interior query surface would answer the wrong `bool` for {v:?}",
        );
        assert_ne!(
            v.is_endpoint(),
            v.is_interior(),
            "{type_name}: {v:?}.is_endpoint() and {v:?}.is_interior() returned the SAME bool — the (endpoint, interior) partition MUST be exhaustive: every variant answers `true` to EXACTLY ONE of the two predicates. A drift here means BOTH predicates fired (a permissive-permissive override pair) OR BOTH predicates rejected (a strict-strict override pair) on {v:?}, breaking the boundary-partition every downstream boundary-vs-interior consumer relies on",
        );
    }
    assert!(
        T::first().is_endpoint(),
        "{type_name}: T::first().is_endpoint() returned false — the (variant → boundary-membership bool) projection failed to fire on the declaration-order head endpoint anchor while the natural `is_first || is_last` composition should return true. Clauses (18) + (30) + (32) together pin `T::first().is_endpoint() == true` as the structural fixpoint the head-endpoint anchor and the boundary-membership predicate axis share",
    );
    assert!(
        T::last().is_endpoint(),
        "{type_name}: T::last().is_endpoint() returned false — the (variant → boundary-membership bool) projection failed to fire on the declaration-order tail endpoint anchor while the natural `is_first || is_last` composition should return true. Clauses (18) + (30) + (32) together pin `T::last().is_endpoint() == true` as the structural fixpoint the tail-endpoint anchor and the boundary-membership predicate axis share, mirroring `T::first().is_endpoint() == true` one direction axis over",
    );
    assert!(
        !T::first().is_interior(),
        "{type_name}: T::first().is_interior() returned true — the (variant → interior-membership bool) projection fired on the declaration-order head endpoint anchor while the natural `!is_endpoint` composition should return false. Clauses (18) + (30) + (32) together pin `T::first().is_interior() == false` as the structural anti-fixpoint the head-endpoint anchor and the interior-membership predicate axis share, mirroring `T::first().is_endpoint() == true` one predicate-flavor axis over",
    );
    assert!(
        !T::last().is_interior(),
        "{type_name}: T::last().is_interior() returned true — the (variant → interior-membership bool) projection fired on the declaration-order tail endpoint anchor while the natural `!is_endpoint` composition should return false. Clauses (18) + (30) + (32) together pin `T::last().is_interior() == false` as the structural anti-fixpoint the tail-endpoint anchor and the interior-membership predicate axis share, mirroring `T::last().is_endpoint() == true` one predicate-flavor axis over",
    );
    // (33) — For every variant `v` in `T::ALL`, `v.is_sorted_endpoint()`
    // MUST equal `v.is_sorted_first() || v.is_sorted_last()`, AND
    // `v.is_sorted_interior()` MUST equal
    // `!(v.is_sorted_first() || v.is_sorted_last())`, AND
    // `is_sorted_endpoint(v) != is_sorted_interior(v)` (exhaustive
    // complementarity), AND `T::sorted_first().is_sorted_endpoint()`
    // MUST be `true`, AND `T::sorted_last().is_sorted_endpoint()` MUST
    // be `true`, AND `T::sorted_first().is_sorted_interior()` MUST be
    // `false`, AND `T::sorted_last().is_sorted_interior()` MUST be
    // `false`. The default trait bodies compose
    // `is_sorted_first(self) || is_sorted_last(self)` (lex-boundary
    // arm) and `!is_sorted_endpoint(self)` (lex-interior arm) verbatim
    // and satisfy both arms for free; the assertion catches a future
    // implementor whose override drifts either lex-boundary-membership
    // projection (a permissive lex-endpoint override that returns
    // `true` on a strict-lex-interior slot — folding an alphabetized-
    // boundary-glyph-emit / lex-audit-event / bounded-alphabetized-
    // iteration-guard consumer onto the wrong partition of `T::ALL`
    // and silently short-circuiting the strict-lex-interior arm; a
    // permissive lex-interior override that returns `true` on a lex-
    // endpoint slot — folding a strictly-lex-interior alphabetized
    // renderer onto the wrong partition; a swapped override that
    // returns `is_sorted_interior`'s answer for `is_sorted_endpoint`
    // AND vice-versa, silently inverting the (lex-endpoint, lex-
    // interior) partition — passing the exhaustive complementarity
    // assertion but drifting from the lex point-membership composition
    // on every variant; a stale override that fails the
    // complementarity assertion — returns the SAME `bool` for both
    // predicates on some variant, breaking the (lex-endpoint XOR lex-
    // interior) partition contract every downstream lex-boundary-vs-
    // interior consumer relies on) loudly rather than silently
    // bifurcating the lex-boundary-membership projection surface every
    // downstream shared-lex-endpoint-badge renderer / lex-boundary-
    // audit-event emitter / strictly-lex-interior phase-fold reducer /
    // alphabetized-carousel-boundary-glyph consumer routes through.
    // Sibling posture to clauses (19) + (31) + (32) — clause (19) pins
    // the (lex head, lex tail) endpoint anchors against
    // `T::sorted_variants()[0]` / `T::sorted_variants()[T::CARDINALITY
    // - 1]`, clause (31) pins the (bool lex-head-membership, bool lex-
    // tail-membership) projections against the composition of the lex-
    // position projection with the const-visible variant count, clause
    // (32) pins the (bool boundary-membership, bool interior-
    // membership) partition on the DECLARATION axis, this clause pins
    // the (bool lex-boundary-membership, bool lex-interior-membership)
    // partition on the LEX axis against the composition of the lex
    // point-membership pair under `||` AND its negation AND pins the
    // exhaustive complementarity `is_sorted_endpoint XOR
    // is_sorted_interior` on every variant. Clauses (19) + (31) + (33)
    // together open the (predicate-flavor × direction) 2×2 lex-axis
    // endpoint matrix at ALL FOUR direct projection surfaces (`Self`-
    // typed lex anchor / `bool`-typed lex point membership / `bool`-
    // typed lex boundary membership / `bool`-typed lex interior
    // membership) AND at BOTH lex-endpoint-anchor lex-boundary-
    // fixpoints (`T::sorted_first().is_sorted_endpoint()`,
    // `T::sorted_last().is_sorted_endpoint()`) AND at BOTH lex-
    // endpoint-anchor lex-interior-anti-fixpoints
    // (`T::sorted_first().is_sorted_interior() == false`,
    // `T::sorted_last().is_sorted_interior() == false`). Clauses (32)
    // + (33) together CLOSE the (predicate-flavor × ordering) 2×2
    // matrix over the boolean-boundary surface — the declaration-axis
    // arm ((32)) and the lex-axis arm ((33)) now cover every ordering
    // × predicate-flavor corner of the boolean-boundary space.
    for &v in T::ALL {
        let expected_sorted_endpoint = v.is_sorted_first() || v.is_sorted_last();
        assert_eq!(
            v.is_sorted_endpoint(),
            expected_sorted_endpoint,
            "{type_name}: {v:?}.is_sorted_endpoint() drifted from {v:?}.is_sorted_first() || {v:?}.is_sorted_last() — the direct (variant → lex-boundary-membership bool) projection no longer agrees with the natural `is_sorted_first || is_sorted_last` composition, so a downstream shared-lex-endpoint-badge renderer / lex-boundary-audit-event emitter / bounded-alphabetized-iteration-guard consumer that binds `v.is_sorted_endpoint()` as its lex-structural-boundary query surface would answer the wrong `bool` for {v:?}",
        );
        assert_eq!(
            v.is_sorted_interior(),
            !expected_sorted_endpoint,
            "{type_name}: {v:?}.is_sorted_interior() drifted from !({v:?}.is_sorted_first() || {v:?}.is_sorted_last()) — the direct (variant → lex-interior-membership bool) projection no longer agrees with the natural `!is_sorted_endpoint` composition, so a downstream strictly-lex-interior phase-fold reducer / strictly-lex-interior alphabetized-completion pass / lex-boundary-hidden renderer consumer that binds `v.is_sorted_interior()` as its strict-lex-interior query surface would answer the wrong `bool` for {v:?}",
        );
        assert_ne!(
            v.is_sorted_endpoint(),
            v.is_sorted_interior(),
            "{type_name}: {v:?}.is_sorted_endpoint() and {v:?}.is_sorted_interior() returned the SAME bool — the (lex-endpoint, lex-interior) partition MUST be exhaustive: every variant answers `true` to EXACTLY ONE of the two predicates. A drift here means BOTH predicates fired (a permissive-permissive override pair) OR BOTH predicates rejected (a strict-strict override pair) on {v:?}, breaking the lex-boundary-partition every downstream lex-boundary-vs-lex-interior consumer relies on",
        );
    }
    assert!(
        T::sorted_first().is_sorted_endpoint(),
        "{type_name}: T::sorted_first().is_sorted_endpoint() returned false — the (variant → lex-boundary-membership bool) projection failed to fire on the lex-order head endpoint anchor while the natural `is_sorted_first || is_sorted_last` composition should return true. Clauses (19) + (31) + (33) together pin `T::sorted_first().is_sorted_endpoint() == true` as the structural fixpoint the lex-head-endpoint anchor and the lex-boundary-membership predicate axis share, mirroring `T::first().is_endpoint() == true` one ordering axis over",
    );
    assert!(
        T::sorted_last().is_sorted_endpoint(),
        "{type_name}: T::sorted_last().is_sorted_endpoint() returned false — the (variant → lex-boundary-membership bool) projection failed to fire on the lex-order tail endpoint anchor while the natural `is_sorted_first || is_sorted_last` composition should return true. Clauses (19) + (31) + (33) together pin `T::sorted_last().is_sorted_endpoint() == true` as the structural fixpoint the lex-tail-endpoint anchor and the lex-boundary-membership predicate axis share, mirroring `T::last().is_endpoint() == true` one ordering axis over and closing the (predicate-flavor × ordering) 2×2 matrix over the boolean-boundary surface",
    );
    assert!(
        !T::sorted_first().is_sorted_interior(),
        "{type_name}: T::sorted_first().is_sorted_interior() returned true — the (variant → lex-interior-membership bool) projection fired on the lex-order head endpoint anchor while the natural `!is_sorted_endpoint` composition should return false. Clauses (19) + (31) + (33) together pin `T::sorted_first().is_sorted_interior() == false` as the structural anti-fixpoint the lex-head-endpoint anchor and the lex-interior-membership predicate axis share, mirroring `T::sorted_first().is_sorted_endpoint() == true` one predicate-flavor axis over",
    );
    assert!(
        !T::sorted_last().is_sorted_interior(),
        "{type_name}: T::sorted_last().is_sorted_interior() returned true — the (variant → lex-interior-membership bool) projection fired on the lex-order tail endpoint anchor while the natural `!is_sorted_endpoint` composition should return false. Clauses (19) + (31) + (33) together pin `T::sorted_last().is_sorted_interior() == false` as the structural anti-fixpoint the lex-tail-endpoint anchor and the lex-interior-membership predicate axis share, mirroring `T::sorted_last().is_sorted_endpoint() == true` one predicate-flavor axis over",
    );
    // (34) — `T::endpoints()` MUST equal `(T::first(), T::last())` —
    // the pair-aggregation on the declaration-axis endpoint-anchor
    // return-shape column projects the two scalar endpoint anchors
    // into a single tuple call. The default trait body composes
    // `(T::first(), T::last())` verbatim and satisfies the clause for
    // free; the assertion catches a future implementor whose override
    // drifts the tuple (a swapped override that returns
    // `(T::last(), T::first())` — silently inverting the (head, tail)
    // tuple-slot semantics every downstream pair-endpoint consumer
    // relies on; a stale override that returns a `(T::Head, T::Head)`
    // diagonal tuple — silently folding both tuple slots onto the
    // head-endpoint anchor and dropping the tail-endpoint from every
    // range-walker / boundary-badge / audit-event / per-implementor
    // coherence probe consumer; a permissive override that fabricates
    // a `(T::Head, T::Interior)` non-endpoint tuple — silently routing
    // a strictly-interior slot into the tail-endpoint tuple slot;
    // a subset-projection override that returns a `(T::Head, T::Head)`
    // singleton-collapse tuple on a `T::CARDINALITY >= 2` closed set
    // — silently collapsing the pair-aggregation onto a diagonal at a
    // cardinality edge where the two slots should diverge) loudly
    // rather than silently bifurcating the pair-aggregation surface
    // every downstream boundary-badge renderer / range-walker
    // destructure / saga-step audit-event emitter / per-implementor
    // coherence probe consumer routes through. Sibling posture to
    // clauses (18) + (30) + (32) — clause (18) pins the individual
    // (head, tail) scalar endpoint-anchor projections against
    // `T::ALL[0]` / `T::ALL[T::CARDINALITY - 1]`, clause (30) pins the
    // per-anchor bool membership projections, clause (32) pins the
    // boundary-partition boolean projections, this clause pins the
    // pair-aggregation tuple projection against the composition of
    // the two scalar endpoint-anchor primitives. Clauses (18) + (34)
    // together open the (return-shape × declaration-anchor) 3-of-3
    // return-shape column (Self scalar head / Self scalar tail /
    // (Self, Self) pair) at ALL THREE direct projection surfaces on
    // the declaration axis.
    assert_eq!(
        T::endpoints(),
        (T::first(), T::last()),
        "{type_name}: T::endpoints() drifted from (T::first(), T::last()) — the direct (declaration-order endpoint pair) tuple projection no longer agrees with the natural `(T::first(), T::last())` two-primitive composition, so a downstream boundary-badge renderer / range-walker destructure / saga-step audit-event emitter / per-implementor coherence probe consumer that binds `T::endpoints()` as its pair-aggregation query surface would answer the wrong tuple",
    );
    // (35) — `T::sorted_endpoints()` MUST equal
    // `(T::sorted_first(), T::sorted_last())` — the pair-aggregation
    // on the lex-axis endpoint-anchor return-shape column projects
    // the two scalar lex-endpoint anchors into a single tuple call.
    // The default trait body composes
    // `(T::sorted_first(), T::sorted_last())` verbatim and satisfies
    // the clause for free; the assertion catches a future implementor
    // whose override drifts the tuple (a swapped override that
    // returns `(T::sorted_last(), T::sorted_first())` — silently
    // inverting the (lex-head, lex-tail) tuple-slot semantics; a
    // stale override that returns a `(T::LexHead, T::LexHead)`
    // diagonal tuple on a non-singleton closed set — silently folding
    // both lex-tuple slots onto the lex-head-endpoint anchor; a
    // permissive override that fabricates a `(T::LexHead, T::Interior)`
    // non-lex-endpoint tuple — silently routing a strictly-lex-
    // interior slot into the lex-tail-endpoint tuple slot; a
    // declaration-axis fold override that returns `(T::first(),
    // T::last())` instead of the lex-endpoint tuple — silently
    // bifurcating the two ordering axes' pair-aggregations onto the
    // SAME tuple, breaking the (declaration, lex) ordering partition
    // every downstream lex-boundary-badge renderer / alphabetized-
    // range-walker destructure / alphabetized-saga-step audit-event
    // emitter / lex-order per-implementor coherence probe consumer
    // relies on) loudly rather than silently bifurcating the lex-
    // pair-aggregation surface every downstream alphabetized-boundary
    // consumer routes through. Sibling posture to clauses (19) + (31)
    // + (33) — clause (19) pins the individual (lex-head, lex-tail)
    // scalar lex-endpoint-anchor projections against
    // `T::sorted_variants()[0]` / `T::sorted_variants()[T::CARDINALITY
    // - 1]`, clause (31) pins the per-anchor lex bool membership
    // projections, clause (33) pins the lex-boundary-partition
    // boolean projections, this clause pins the lex-pair-aggregation
    // tuple projection against the composition of the two scalar
    // lex-endpoint-anchor primitives. Clauses (34) + (35) together
    // CLOSE the (ordering × pair-aggregation) 2×1 matrix over the
    // closed-set endpoint-anchor return-shape axis — the declaration-
    // axis arm ((34)) and the lex-axis arm ((35)) now cover every
    // ordering corner of the pair-return-shape column of the closed-
    // set endpoint-anchor matrix, so every generic consumer that
    // binds either pair-aggregation surface sees the SAME `(Self,
    // Self)` tuple shape at every crate boundary regardless of which
    // ordering axis it walks.
    assert_eq!(
        T::sorted_endpoints(),
        (T::sorted_first(), T::sorted_last()),
        "{type_name}: T::sorted_endpoints() drifted from (T::sorted_first(), T::sorted_last()) — the direct (lex-order endpoint pair) tuple projection no longer agrees with the natural `(T::sorted_first(), T::sorted_last())` two-primitive composition, so a downstream lex-boundary-badge renderer / alphabetized-range-walker destructure / alphabetized-saga-step audit-event emitter / lex-order per-implementor coherence probe consumer that binds `T::sorted_endpoints()` as its lex-pair-aggregation query surface would answer the wrong tuple",
    );
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
    fn contains_label_recognizes_every_canonical_variant_label() {
        // The pure-membership sweep — for every variant `v` in
        // `Self::ALL`, `contains_label(v.label())` returns `true`.
        // Sibling posture to `parse_label_round_trips_every_variant`
        // (decode arm) — this pin covers the predicate arm on the
        // (allocating decode, non-allocating membership) axis. A
        // regression that drops a label from the sweep (an override
        // that names a subset of `Self::ALL` in its membership
        // projection) would silently make `contains_label` return
        // `false` for a canonical variant, bifurcating the
        // pure-membership surface from `parse_label`'s decode surface.
        // Pinning the projection here catches the drift on the
        // stub-level surface before any per-implementor test surfaces
        // it downstream.
        for &v in <StubKind as ClosedSet>::ALL {
            assert!(
                <StubKind as ClosedSet>::contains_label(v.label()),
                "contains_label({:?}) returned false for a canonical variant",
                v.label(),
            );
        }
    }

    #[test]
    fn contains_label_rejects_unknown_input_without_allocating_carrier() {
        // Zero-allocation predicate arm — an input outside the closed
        // set returns `false` WITHOUT threading through
        // `make_unknown` (i.e. without materializing an owned `String`
        // copy of the input). The (allocating decode, non-allocating
        // membership) axis partitions cleanly at the return-type
        // boundary: `parse_label(s)` on the same reject path allocates
        // `UnknownStubKind("delta".to_owned())`; this method does not.
        // Pinning the predicate result here means a generic filter
        // consumer that walks over a candidate stream can lean on
        // `contains_label` for zero-alloc pre-check, then commit to
        // `parse_label` for the typed decode on the surviving
        // subset — the two arms of the axis stay semantically aligned
        // (both agree on membership) while the allocation cost
        // partitions per consumer's needs.
        assert!(!<StubKind as ClosedSet>::contains_label("delta"));
    }

    #[test]
    fn contains_label_rejects_empty_input() {
        // Empty input is structurally outside the closed set — no
        // variant projects to "" through `label`, so the predicate
        // rejects cleanly. Mirrors `parse_label_rejects_empty_input`
        // on the (decode Err, predicate false) axis one arm over.
        // Pinning the empty-input rejection here means the
        // pure-membership surface can't drift its empty-input
        // behavior from the decode surface; the two arms of the
        // (allocating decode, non-allocating membership) axis stay
        // aligned at the boundary case operators hit when a config
        // field is unset but reached anyway.
        assert!(!<StubKind as ClosedSet>::contains_label(""));
    }

    #[test]
    fn contains_label_is_case_sensitive() {
        // The pure-membership predicate inherits the case-sensitive
        // contract from `label`'s byte-for-byte canonical rendering.
        // A future implementor that wants case-insensitive membership
        // must override `contains_label` (and document the divergence)
        // alongside a matching `parse_label` override — the two arms of
        // the (allocating decode, non-allocating membership) axis must
        // stay semantically aligned. Pinning the case-sensitive
        // contract here means the two overrides can't drift
        // independently; a regression that flips only ONE arm to
        // case-insensitive silently bifurcates the surface (an operator
        // could see `parse_label("Alpha") = Err`, `contains_label("Alpha")
        // = true` or vice versa). Sibling posture to
        // `parse_label_is_case_sensitive` one axis over.
        assert!(!<StubKind as ClosedSet>::contains_label("Alpha"));
    }

    #[test]
    fn contains_label_agrees_with_parse_label_is_ok_on_every_probe() {
        // The (allocating decode, non-allocating membership) axis
        // MUST stay semantically aligned on every input the sweep
        // walks. This test pins the alignment against a representative
        // probe set: (a) every canonical variant label — both arms
        // return the acceptance side (`Ok(_)` / `true`); (b) three
        // non-canonical inputs (`"delta"`, `""`, `"Alpha"`) —
        // both arms return the rejection side (`Err(_)` / `false`).
        // The alignment is the load-bearing contract that lets a
        // generic consumer swap `parse_label(s).is_ok()` for the
        // zero-alloc `contains_label(s)` without changing the
        // program's membership semantics. A regression that drifts
        // either arm (a permissive `contains_label` override that
        // accepts unknown strings, a strict `parse_label` override
        // that rejects a canonical label) fails this pin
        // stub-level before any per-implementor sweep depends on the
        // alignment downstream.
        let inputs: [&str; 6] = ["alpha", "beta", "gamma", "delta", "", "Alpha"];
        for input in inputs {
            let decode_ok = <StubKind as ClosedSet>::parse_label(input).is_ok();
            let contains = <StubKind as ClosedSet>::contains_label(input);
            assert_eq!(
                decode_ok, contains,
                "contains_label({input:?}) disagreed with parse_label({input:?}).is_ok() — the (allocating decode, non-allocating membership) axis bifurcated",
            );
        }
    }

    #[test]
    fn find_by_label_recovers_every_canonical_variant() {
        // The zero-allocation typed-decode arm — for every variant
        // `v` in `Self::ALL`, `find_by_label(v.label())` returns
        // `Some(v)`. Sibling posture to
        // `parse_label_round_trips_every_variant` (allocating decode
        // arm) and `contains_label_recognizes_every_canonical_
        // variant_label` (predicate arm) — this pin covers the
        // typed-Option arm of the (return-type × side-effect)
        // matrix. A regression that drops a variant from the sweep
        // (an override that names a subset of `Self::ALL` in its
        // typed-decode projection) would silently make
        // `find_by_label` return `None` for a canonical variant,
        // bifurcating the zero-allocation typed decode from
        // `parse_label`'s decoding surface. Pinning the projection
        // here catches the drift on the stub-level surface before
        // any per-implementor test surfaces it downstream.
        for &v in <StubKind as ClosedSet>::ALL {
            assert_eq!(
                <StubKind as ClosedSet>::find_by_label(v.label()),
                Some(v),
                "find_by_label({:?}) failed to recover the canonical variant",
                v.label(),
            );
        }
    }

    #[test]
    fn find_by_label_rejects_unknown_input_without_allocating_carrier() {
        // Zero-allocation typed-decode reject arm — an input outside
        // the closed set returns `None` WITHOUT threading through
        // `make_unknown` (i.e. without materializing an owned
        // `String` copy of the input). The (allocating carrier
        // decode, non-allocating typed decode, non-allocating
        // predicate) triad of the closed-set surface partitions
        // cleanly at the return-type boundary: `parse_label(s)` on
        // the same reject path allocates
        // `UnknownStubKind("delta".to_owned())`; this method returns
        // a bare `None`. Pinning the reject result here means a
        // generic filter-map consumer that walks over a candidate
        // stream can lean on `find_by_label` for a zero-alloc typed
        // projection — the three arms of the triad stay semantically
        // aligned (all agree on membership) while the allocation
        // cost partitions per consumer's needs.
        assert_eq!(<StubKind as ClosedSet>::find_by_label("delta"), None);
    }

    #[test]
    fn find_by_label_rejects_empty_input() {
        // Empty input is structurally outside the closed set — no
        // variant projects to "" through `label`, so the
        // typed-decode arm rejects cleanly. Mirrors
        // `parse_label_rejects_empty_input` and
        // `contains_label_rejects_empty_input` one axis over on the
        // (Result Err, bool false, Option None) triad. Pinning the
        // empty-input rejection here means the zero-allocation
        // typed-decode surface can't drift its empty-input behavior
        // from the decode / predicate surfaces; the three arms of
        // the triad stay aligned at the boundary case operators hit
        // when a config field is unset but reached anyway.
        assert_eq!(<StubKind as ClosedSet>::find_by_label(""), None);
    }

    #[test]
    fn find_by_label_is_case_sensitive() {
        // The zero-allocation typed decode inherits the
        // case-sensitive contract from `label`'s byte-for-byte
        // canonical rendering. A future implementor that wants
        // case-insensitive typed decoding must override
        // `find_by_label` (and document the divergence) alongside
        // matching `parse_label` + `contains_label` overrides — the
        // three arms of the (Result, bool, Option) triad must stay
        // semantically aligned. Pinning the case-sensitive contract
        // here means the three overrides can't drift independently;
        // a regression that flips only ONE arm to case-insensitive
        // silently bifurcates the surface (an operator could see
        // `parse_label("Alpha") = Err`, `find_by_label("Alpha") =
        // Some(_)` or vice versa). Sibling posture to
        // `parse_label_is_case_sensitive` and
        // `contains_label_is_case_sensitive` one axis over.
        assert_eq!(<StubKind as ClosedSet>::find_by_label("Alpha"), None);
    }

    #[test]
    fn find_by_label_agrees_with_parse_label_and_contains_label_on_every_probe() {
        // The (Result<Self, Unknown>, Option<Self>, bool) triad on
        // the return-type axis MUST stay semantically aligned on
        // every input the sweep walks. This test pins the alignment
        // against a representative probe set: (a) every canonical
        // variant label — all three arms return the acceptance side
        // (`Ok(v)` / `Some(v)` / `true`) AND all three project to
        // the SAME typed variant on the acceptance side; (b) three
        // non-canonical inputs (`"delta"`, `""`, `"Alpha"`) — all
        // three arms return the rejection side (`Err(_)` / `None` /
        // `false`). The alignment is the load-bearing contract that
        // lets a generic consumer freely swap between the three
        // arms of the triad based on its allocation / return-type
        // needs without changing the program's membership semantics.
        // A regression that drifts any arm (a permissive
        // `find_by_label` override that accepts unknown strings, a
        // strict `parse_label` override that rejects a canonical
        // label, a `contains_label` override that names a subset)
        // fails this pin stub-level before any per-implementor
        // sweep depends on the alignment downstream. Sibling
        // posture to
        // `contains_label_agrees_with_parse_label_is_ok_on_every_probe`
        // one axis over — this pin extends the (Result, bool) two-
        // arm alignment to the full (Result, Option<Self>, bool)
        // triad the substrate's closed-set surface exposes.
        let inputs: [&str; 6] = ["alpha", "beta", "gamma", "delta", "", "Alpha"];
        for input in inputs {
            let parsed = <StubKind as ClosedSet>::parse_label(input);
            let found = <StubKind as ClosedSet>::find_by_label(input);
            let contains = <StubKind as ClosedSet>::contains_label(input);
            match (parsed, found) {
                (Ok(a), Some(b)) => {
                    assert_eq!(
                        a, b,
                        "parse_label({input:?}) and find_by_label({input:?}) decoded to different variants — the typed-decode axis bifurcated",
                    );
                    assert!(
                        contains,
                        "contains_label({input:?}) returned false while parse_label / find_by_label accepted — the predicate arm bifurcated from the typed-decode arms",
                    );
                }
                (Err(_), None) => {
                    assert!(
                        !contains,
                        "contains_label({input:?}) returned true while parse_label / find_by_label rejected — the predicate arm bifurcated from the typed-decode arms",
                    );
                }
                (Ok(_), None) | (Err(_), Some(_)) => panic!(
                    "parse_label({input:?}) and find_by_label({input:?}) disagreed on the (accept, reject) partition — the (Result, Option<Self>) return-type axis bifurcated",
                ),
            }
        }
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_find_by_label_and_composition() {
        // The well-formedness sweep's (12) clause —
        // `T::find_by_label(s)` MUST agree with `T::parse_label(s)
        // .ok()` on every representative input (every canonical
        // variant label, the reserved probe, the empty-string
        // boundary). A hand-impl'd implementor whose override drifts
        // the composition — e.g. a permissive override that returns
        // `Some(_)` for the reserved probe input that clauses (5) +
        // (7) + (11) also reserve — fails the sweep loudly rather
        // than silently bifurcating the zero-allocation typed-decode
        // surface every LSP / config-decoder / filter-map consumer
        // routes through. Pinning the failure path here keeps the
        // testkit's (12) clause guaranteed-to-fire — a regression
        // that makes the assertion permissive (e.g. a future "either
        // acceptance" relaxation that only checks canonical labels
        // without checking the probe / empty rejection paths)
        // breaks this stub-level contract before any per-implementor
        // sweep runs. Sibling posture to the eleven sibling
        // `_catches_drift_between_*` pins above (clauses 5-11);
        // together they close the structural-drift-catches sweep
        // on every default composition the trait exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedFindKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedFindKind(pub String);
        impl core::fmt::Display for UnknownDriftedFindKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted find kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedFindKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted find kind";
            type Unknown = UnknownDriftedFindKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedFindKind(s.to_owned())
            }
            fn find_by_label(_s: &str) -> Option<Self> {
                // Drifted override — accepts every input, including
                // the unrecognizable probe the testkit's clause (12)
                // reserves as structurally outside the closed set.
                // Fails the zero-allocation typed-decode alignment
                // with `parse_label(s).ok()` and `contains_label(s)`.
                Some(Self::Only)
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedFindKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a find_by_label override that accepted the reserved probe input",
        );
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
    fn labels_joined_renders_labels_through_chosen_separator() {
        // The joined-candidate-list surface — `T::labels_joined(sep)`
        // composes `T::labels()` with `slice::join` verbatim. Pinning
        // the rendering across THREE representative separators
        // (slash for `INTENT_KIND_LIST`-shaped production constants
        // the substrate's `IntentError::Empty` / `ArtifactError::Empty` /
        // `ChannelError::Empty` / `EncapsulationKindError::Empty`
        // diagnostics carry; comma-space for natural-language
        // `expected one of: ...` shapes; pipe for grammar-style
        // alternative lists) means a regression that drifts the
        // composition at the trait method (a future override that
        // strips a label, threads a different separator, sorts the
        // labels alphabetically rather than preserving declaration
        // order) fails this contract before any per-implementor surface
        // depends on the rendering downstream.
        assert_eq!(
            <StubKind as ClosedSet>::labels_joined("/"),
            "alpha/beta/gamma",
        );
        assert_eq!(
            <StubKind as ClosedSet>::labels_joined(", "),
            "alpha, beta, gamma",
        );
        assert_eq!(
            <StubKind as ClosedSet>::labels_joined("|"),
            "alpha|beta|gamma",
        );
    }

    #[test]
    fn labels_joined_threads_empty_separator_into_a_concatenated_run() {
        // Edge case — an empty separator concatenates the labels
        // without delimitation. The `slice::join` primitive handles
        // this naturally; pinning it here means a future override
        // that special-cases the empty-separator path (an over-eager
        // optimization that returns a constant, a normalization step
        // that swaps `""` for `", "`) fails-loudly rather than
        // silently bifurcating the candidate-list rendering. The
        // joined output stays a valid `String` — `slice::join` does
        // not allocate per-separator-byte when the separator is empty.
        assert_eq!(<StubKind as ClosedSet>::labels_joined(""), "alphabetagamma",);
    }

    #[test]
    fn labels_joined_threads_multi_char_separator_verbatim() {
        // Multi-character separator — `slice::join` interpolates the
        // separator between each pair of labels verbatim. Pinning
        // this here means a future override that re-encodes /
        // normalizes the separator (a strip-whitespace pass, an
        // escape-special-chars pass) fails-loudly rather than
        // silently bifurcating the candidate-list rendering. The
        // " -> " separator is the shape a state-machine diagnostic
        // (`expected one of: alpha -> beta -> gamma`) would lean on,
        // so the multi-char path stays tested even though no
        // production implementor reaches for it today.
        assert_eq!(
            <StubKind as ClosedSet>::labels_joined(" -> "),
            "alpha -> beta -> gamma",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_labels_joined_and_composition() {
        // The well-formedness sweep's (8) clause —
        // `T::labels_joined(sep)` MUST compose `T::labels()` with
        // `slice::join` verbatim across every representative
        // separator. A hand-impl'd implementor whose override drifts
        // the composition (drops a label, threads a different
        // separator, sorts labels alphabetically rather than
        // preserving declaration order) fails the sweep loudly
        // rather than silently bifurcating the joined-candidate-list
        // surface every consumer routes through. Pinning the failure
        // path here keeps the testkit's (8) clause guaranteed-to-fire
        // — a regression that makes the assertion permissive (e.g. a
        // future "any superset" relaxation) breaks this stub-level
        // contract before any per-implementor sweep runs.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedJoinKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedJoinKind(pub String);
        impl core::fmt::Display for UnknownDriftedJoinKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted join kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedJoinKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted join kind";
            type Unknown = UnknownDriftedJoinKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedJoinKind(s.to_owned())
            }
            fn labels_joined(_sep: &str) -> String {
                // Drifted override — returns a hard-coded literal
                // that ignores the caller-supplied separator and the
                // implementor's actual labels surface.
                String::from("WRONG")
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedJoinKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a labels_joined override drifted from the natural labels-join composition",
        );
    }

    #[test]
    fn sorted_labels_renders_labels_in_lexicographic_order() {
        // The sorted-candidate-list surface — `T::sorted_labels()`
        // composes `T::labels()` with `slice::sort_unstable` verbatim.
        // Pinning the rendering against a hand-rolled lexicographic
        // truth-table here means a regression that drifts the
        // composition at the trait method (a future override that
        // strips a label, threads a different ordering, sorts
        // case-insensitively rather than byte-wise, returns
        // declaration order instead of lexicographic) fails this
        // contract before any per-implementor surface depends on the
        // canonical ordering downstream. The `StubKind` labels
        // (`alpha`/`beta`/`gamma`) are already in lexicographic order
        // in declaration, so the round-trip identity holds trivially;
        // the test relies on the sort key being byte-wise ASCII rather
        // than declaration-order to actually exercise the sort step.
        assert_eq!(
            <StubKind as ClosedSet>::sorted_labels(),
            vec!["alpha", "beta", "gamma"],
        );
    }

    #[test]
    fn sorted_labels_normalizes_arbitrary_declaration_order() {
        // The sort-step contract — `T::sorted_labels()` MUST normalize
        // an arbitrary declaration order into ASCII lexicographic
        // order, regardless of the implementor's `ALL`-array layout.
        // A regression that returns `labels()` verbatim (without the
        // sort step) would pass `sorted_labels_renders_labels_in_
        // lexicographic_order` on `StubKind` (because its labels
        // already sit in order) but silently bifurcate the
        // canonical-ordering surface for any implementor whose
        // declaration order differs from byte-wise sort order.
        // Pinning the sort discipline here with a deliberately-
        // out-of-order stub catches that drift directly.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum ReverseStubKind {
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownReverseStubKind(pub String);
        impl core::fmt::Display for UnknownReverseStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown reverse stub kind: {}", self.0)
            }
        }
        impl ClosedSet for ReverseStubKind {
            const ALL: &'static [Self] = &[Self::Gamma, Self::Beta, Self::Alpha];
            const SET_LABEL: &'static str = "reverse stub kind";
            type Unknown = UnknownReverseStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownReverseStubKind(s.to_owned())
            }
        }
        // `labels()` preserves declaration order — `gamma, beta, alpha`.
        assert_eq!(
            <ReverseStubKind as ClosedSet>::labels(),
            vec!["gamma", "beta", "alpha"],
        );
        // `sorted_labels()` normalizes to ASCII lexicographic order —
        // `alpha, beta, gamma`. The composition with `sort_unstable`
        // is the load-bearing step the lift names.
        assert_eq!(
            <ReverseStubKind as ClosedSet>::sorted_labels(),
            vec!["alpha", "beta", "gamma"],
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_sorted_labels_and_composition() {
        // The well-formedness sweep's (9) clause —
        // `T::sorted_labels()` MUST compose `T::labels()` with
        // `slice::sort_unstable` verbatim. A hand-impl'd implementor
        // whose override drifts the composition (returns labels in
        // declaration order, strips a label, threads a different
        // ordering) fails the sweep loudly rather than silently
        // bifurcating the canonical-ordered candidate-list surface
        // every LSP / `tatara-check` / metrics consumer routes
        // through. Pinning the failure path here keeps the testkit's
        // (9) clause guaranteed-to-fire — a regression that makes
        // the assertion permissive (e.g. a future "any permutation"
        // relaxation) breaks this stub-level contract before any
        // per-implementor sweep runs.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedSortKind {
            First,
            Second,
        }
        #[derive(Debug)]
        struct UnknownDriftedSortKind(pub String);
        impl core::fmt::Display for UnknownDriftedSortKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted sort kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedSortKind {
            const ALL: &'static [Self] = &[Self::First, Self::Second];
            const SET_LABEL: &'static str = "drifted sort kind";
            type Unknown = UnknownDriftedSortKind;
            fn label(self) -> &'static str {
                match self {
                    // Labels are intentionally out of declaration
                    // order vs. byte-wise sort: declaration is
                    // `first, second`, but ASCII sort is `second,
                    // first` only if `second < first` lexically (it
                    // is: `'f' < 's'`, so declaration ALREADY equals
                    // sort). Use distinct labels that DO reorder under
                    // sort: `"zeta"` precedes `"alpha"` in declaration
                    // but follows in ASCII sort.
                    Self::First => "zeta",
                    Self::Second => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedSortKind(s.to_owned())
            }
            fn sorted_labels() -> Vec<&'static str> {
                // Drifted override — returns labels in declaration
                // order rather than lexicographic, bifurcating the
                // canonical-ordering surface.
                vec!["zeta", "alpha"]
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedSortKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a sorted_labels override drifted from the natural labels-then-sort composition",
        );
    }

    #[test]
    fn sorted_labels_joined_renders_lexicographic_labels_through_chosen_separator() {
        // The alphabetized joined-candidate-list surface —
        // `T::sorted_labels_joined(sep)` composes `T::sorted_labels()`
        // with `slice::join` verbatim. Pinning the rendering across THREE
        // representative separators (slash for ordering-independent
        // production constants, comma-space for natural-language
        // alphabetized `expected one of: ...` shapes, pipe for grammar-
        // style alphabetized alternative lists) means a regression that
        // drifts the composition at the trait method (a future override
        // that strips a label, threads a different separator, returns
        // declaration order rather than lexicographic) fails this
        // contract before any per-implementor surface depends on the
        // rendering downstream. `StubKind`'s labels (`alpha`/`beta`/
        // `gamma`) are already in lexicographic declaration order, so
        // the sorted rendering coincides with the natural
        // `labels_joined` rendering; the ReverseStubKind pin below
        // exercises the actual sort discipline.
        assert_eq!(
            <StubKind as ClosedSet>::sorted_labels_joined("/"),
            "alpha/beta/gamma",
        );
        assert_eq!(
            <StubKind as ClosedSet>::sorted_labels_joined(", "),
            "alpha, beta, gamma",
        );
        assert_eq!(
            <StubKind as ClosedSet>::sorted_labels_joined("|"),
            "alpha|beta|gamma",
        );
    }

    #[test]
    fn sorted_labels_joined_normalizes_arbitrary_declaration_order() {
        // The sort-step contract on the joined surface —
        // `T::sorted_labels_joined(sep)` MUST normalize an arbitrary
        // declaration order into ASCII lexicographic order BEFORE
        // joining, regardless of the implementor's `ALL`-array layout.
        // A regression that returns `labels_joined(sep)` verbatim
        // (without the sort step) would pass
        // `sorted_labels_joined_renders_lexicographic_labels_through_chosen_separator`
        // on `StubKind` (because its labels already sit in order) but
        // silently bifurcate the alphabetized-rendering surface for any
        // implementor whose declaration order differs from byte-wise
        // sort order. Pinning the sort discipline here with a
        // deliberately-out-of-order stub catches that drift directly.
        // Sibling posture to `sorted_labels_normalizes_arbitrary_declaration_order`
        // one axis over (Vec) — this pin covers the String surface.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum ReverseJoinStubKind {
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownReverseJoinStubKind(pub String);
        impl core::fmt::Display for UnknownReverseJoinStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown reverse join stub kind: {}", self.0)
            }
        }
        impl ClosedSet for ReverseJoinStubKind {
            const ALL: &'static [Self] = &[Self::Gamma, Self::Beta, Self::Alpha];
            const SET_LABEL: &'static str = "reverse join stub kind";
            type Unknown = UnknownReverseJoinStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownReverseJoinStubKind(s.to_owned())
            }
        }
        // `labels_joined` preserves declaration order — `gamma/beta/alpha`.
        assert_eq!(
            <ReverseJoinStubKind as ClosedSet>::labels_joined("/"),
            "gamma/beta/alpha",
        );
        // `sorted_labels_joined` normalizes to ASCII lexicographic order
        // BEFORE joining — `alpha/beta/gamma`. The composition with
        // `sort_unstable` is the load-bearing step the lift names.
        assert_eq!(
            <ReverseJoinStubKind as ClosedSet>::sorted_labels_joined("/"),
            "alpha/beta/gamma",
        );
    }

    #[test]
    fn sorted_labels_joined_threads_empty_separator_into_a_concatenated_sorted_run() {
        // Edge case — an empty separator concatenates the sorted labels
        // without delimitation, distinct from
        // `labels_joined_threads_empty_separator_into_a_concatenated_run`
        // only in that the concatenation happens on the lexicographic-
        // ordered labels rather than the declaration-ordered ones.
        // Pinning it here means a future override that special-cases
        // the empty-separator path (an over-eager optimization that
        // returns a constant, a normalization step that swaps `""` for
        // `", "`) fails-loudly rather than silently bifurcating the
        // alphabetized-rendering surface. The joined output stays a
        // valid `String` — `slice::join` does not allocate per-separator
        // -byte when the separator is empty.
        assert_eq!(
            <StubKind as ClosedSet>::sorted_labels_joined(""),
            "alphabetagamma",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_sorted_labels_joined_and_composition() {
        // The well-formedness sweep's (10) clause —
        // `T::sorted_labels_joined(sep)` MUST compose `T::sorted_labels()`
        // with `slice::join` verbatim across every representative
        // separator. A hand-impl'd implementor whose override drifts
        // the composition (drops a label, threads a different
        // separator, returns declaration order rather than
        // lexicographic) fails the sweep loudly rather than silently
        // bifurcating the alphabetized-joined-candidate-list surface
        // every consumer routes through. Pinning the failure path here
        // keeps the testkit's (10) clause guaranteed-to-fire — a
        // regression that makes the assertion permissive (e.g. a
        // future "any superset" relaxation) breaks this stub-level
        // contract before any per-implementor sweep runs. Sibling
        // posture to the eight sibling `_catches_drift_between_*`
        // pins above (clauses 5-9); together they close the
        // structural-drift-catches sweep on every default composition
        // the trait exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedSortedJoinKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedSortedJoinKind(pub String);
        impl core::fmt::Display for UnknownDriftedSortedJoinKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted sorted join kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedSortedJoinKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted sorted join kind";
            type Unknown = UnknownDriftedSortedJoinKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedSortedJoinKind(s.to_owned())
            }
            fn sorted_labels_joined(_sep: &str) -> String {
                // Drifted override — returns a hard-coded literal that
                // ignores the caller-supplied separator and the
                // implementor's actual sorted-labels surface.
                String::from("WRONG")
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedSortedJoinKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a sorted_labels_joined override drifted from the natural sorted-labels-then-join composition",
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

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_contains_label_and_composition() {
        // The well-formedness sweep's (11) clause —
        // `T::contains_label(s)` MUST agree with
        // `T::parse_label(s).is_ok()` on every representative input
        // (every canonical variant label, the reserved probe, the
        // empty-string boundary). A hand-impl'd implementor whose
        // override drifts the composition — e.g. a permissive
        // override that returns `true` for the reserved probe input
        // that clauses (5) + (7) also reserve — fails the sweep
        // loudly rather than silently bifurcating the pure-membership
        // surface every lint / filter / gate consumer routes
        // through. Pinning the failure path here keeps the testkit's
        // (11) clause guaranteed-to-fire — a regression that makes
        // the assertion permissive (e.g. a future "either
        // acceptance" relaxation that only checks canonical labels
        // without checking the probe / empty rejection paths) breaks
        // this stub-level contract before any per-implementor sweep
        // runs. Sibling posture to the ten sibling
        // `_catches_drift_between_*` pins above (clauses 5-10);
        // together they close the structural-drift-catches sweep on
        // every default composition the trait exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedContainsKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedContainsKind(pub String);
        impl core::fmt::Display for UnknownDriftedContainsKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted contains kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedContainsKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted contains kind";
            type Unknown = UnknownDriftedContainsKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedContainsKind(s.to_owned())
            }
            fn contains_label(_s: &str) -> bool {
                // Drifted override — accepts every input, including
                // the unrecognizable probe the testkit's clause (11)
                // reserves as structurally outside the closed set.
                // Fails the pure-membership alignment with
                // `parse_label(s).is_ok()`.
                true
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedContainsKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a contains_label override that accepted the reserved probe input",
        );
    }

    #[test]
    fn find_by_label_with_hint_returns_ok_variant_for_exact_match() {
        // The exact-match arm — `find_by_label_with_hint("alpha")`
        // returns `Ok(Alpha)` and never enters `suggest_closest`.
        // The hint slot is structurally absent on the Ok arm because
        // the substrate-wide `did you mean …?` surface never double-
        // emits the same variant once as a successful decode and once
        // as a hint. Sibling posture to
        // `parse_label_with_hint_returns_ok_variant_for_exact_match`
        // one axis over on the (allocating carrier decode, non-
        // allocating typed decode) column of the (side-effect × hint)
        // 2×2 matrix. Pinning the exact-match shape here means a
        // generic consumer can lean on the Ok(v) shape to short-
        // circuit before any structured-diagnostic materialization.
        for &v in <StubKind as ClosedSet>::ALL {
            let outcome = <StubKind as ClosedSet>::find_by_label_with_hint(v.label());
            assert_eq!(outcome, Ok(v));
        }
    }

    #[test]
    fn find_by_label_with_hint_returns_hint_for_near_miss() {
        // The near-miss arm — `find_by_label_with_hint("alpa")`
        // returns `Err(Some(Alpha))`: the typed decode misses (the
        // input isn't a canonical label), and the substrate-wide
        // bounded edit distance places "alpha" within reach
        // (Levenshtein distance 1 ≤ bound 2 for 4-char input). The
        // hint adornment is the typed variant a downstream LSP /
        // `tatara-check` consumer renders next to a bare rejection
        // WITHOUT paying the `make_unknown` carrier allocation
        // `parse_label_with_hint` threads on the same reject path.
        // Sibling posture to
        // `parse_label_with_hint_returns_unknown_with_hint_for_near_miss`
        // one axis over — this pin covers the non-allocating column
        // of the (side-effect × hint) 2×2 matrix.
        let outcome = <StubKind as ClosedSet>::find_by_label_with_hint("alpa");
        assert_eq!(outcome, Err(Some(StubKind::Alpha)));
    }

    #[test]
    fn find_by_label_with_hint_returns_none_hint_for_far_miss() {
        // The conservative-suggestion arm — an input whose closest
        // label sits beyond the substrate-wide bounded edit distance
        // returns `Err(None)` rather than `Err(Some(best_of_the_bunch))`.
        // The hint slot stays absent so the "did you mean …?" surface
        // doesn't fabricate an unrelated suggestion. Sibling posture
        // to `parse_label_with_hint_returns_unknown_without_hint_for_far_miss`
        // one axis over. Pinning the contract here means a generic
        // structured-diagnostic consumer that takes the hint slot can
        // rely on its presence as a signal — the operator sees
        // `did you mean …?` only when the substrate has a typed
        // near-miss to point at.
        let outcome = <StubKind as ClosedSet>::find_by_label_with_hint("xxxxxxxx");
        assert_eq!(outcome, Err(None));
    }

    #[test]
    fn find_by_label_with_hint_rejects_unknown_input_without_allocating_carrier() {
        // Zero-allocation structured-decode reject arm — an input
        // outside the closed set returns `Err(hint)` WITHOUT
        // threading through `make_unknown` (i.e. without materializing
        // an owned `String` copy of the input in a typed carrier).
        // The (allocating carrier decode, non-allocating typed
        // decode) axis of the (side-effect × hint) 2×2 matrix
        // partitions cleanly at the return-type boundary:
        // `parse_label_with_hint(s)` on the same reject path
        // allocates `UnknownStubKind("zzzz".to_owned())` next to the
        // hint slot; this method returns a bare `Err(hint)`. Pinning
        // the reject shape here means a generic LSP hover pass /
        // config-decoder consumer can lean on
        // `find_by_label_with_hint` for a zero-alloc structured
        // typed projection — the two columns of the 2×2 matrix stay
        // semantically aligned (both agree on membership) while the
        // allocation cost partitions per consumer's needs. The probe
        // "zzzz" (4 chars → suggestion-bound 2) sits at Levenshtein
        // distance ≥ 4 from every canonical `StubKind` label, so the
        // far-miss `Err(None)` hint shape isolates the reject arm's
        // carrier-non-allocation property from the near-miss hint
        // shape `find_by_label_with_hint_returns_hint_for_near_miss`
        // pins one axis over.
        let outcome = <StubKind as ClosedSet>::find_by_label_with_hint("zzzz");
        assert_eq!(outcome, Err(None));
    }

    #[test]
    fn find_by_label_with_hint_agrees_with_parse_label_with_hint_on_every_probe() {
        // The (side-effect × hint) 2×2 matrix on the return-type axis
        // MUST stay semantically aligned on every input the sweep
        // walks. This test pins the alignment against a representative
        // 6-input probe set covering (a) every canonical variant
        // label — both arms return the acceptance side (`Ok(v)`) AND
        // project to the SAME typed variant; (b) a near-miss —
        // both arms return `Err(_, Some(v))` / `Err(Some(v))` on the
        // hint slot alignment; (c) a far miss — both arms return
        // `Err(_, None)` / `Err(None)` on the hint slot alignment;
        // (d) the empty-string boundary. The alignment is the
        // load-bearing contract that lets a generic consumer freely
        // swap between the two columns of the 2×2 matrix based on
        // its allocation needs without changing the program's
        // structured-decode semantics. A regression that drifts the
        // hint slot on either arm (a permissive
        // `find_by_label_with_hint` override that fabricates a hint
        // for an unknown string, a strict `parse_label_with_hint`
        // override that drops a valid hint on a near-miss) fails
        // this pin stub-level before any per-implementor sweep
        // depends on the alignment downstream.
        let inputs: [&str; 6] = ["alpha", "beta", "gamma", "alpa", "xxxxxxxx", ""];
        for input in inputs {
            let parsed = <StubKind as ClosedSet>::parse_label_with_hint(input);
            let found = <StubKind as ClosedSet>::find_by_label_with_hint(input);
            match (parsed, found) {
                (Ok(a), Ok(b)) => assert_eq!(
                    a, b,
                    "parse_label_with_hint({input:?}) and find_by_label_with_hint({input:?}) decoded to different variants — the structured typed-decode axis bifurcated",
                ),
                (Err((_, hint_p)), Err(hint_f)) => assert_eq!(
                    hint_p, hint_f,
                    "parse_label_with_hint({input:?}) and find_by_label_with_hint({input:?}) disagreed on the typed hint slot — the (side-effect × hint) 2×2 matrix bifurcated on the hint column",
                ),
                (Ok(_), Err(_)) | (Err(_), Ok(_)) => panic!(
                    "parse_label_with_hint({input:?}) and find_by_label_with_hint({input:?}) disagreed on the (accept, reject) partition — the structured decode axis bifurcated",
                ),
            }
        }
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_find_by_label_with_hint_and_composition()
    {
        // The well-formedness sweep's (13) clause —
        // `find_by_label_with_hint` MUST compose `find_by_label` +
        // `suggest_closest` verbatim. A hand-impl'd implementor whose
        // override drifts the composition (accepts the probe as Ok,
        // fabricates a hint for the unrecognizable probe, emits the
        // wrong typed decode on a canonical variant) fails the sweep
        // loudly rather than silently bifurcating the zero-allocation
        // structured-decode surface every LSP / config-decoder /
        // filter-map consumer routes through. Pinning the failure
        // path here keeps the testkit's (13) clause guaranteed-to-
        // fire — a regression that makes the assertion permissive
        // breaks this stub-level contract before any per-implementor
        // sweep runs. Sibling posture to the twelve sibling
        // `_catches_drift_between_*` pins above (clauses 5-12);
        // together they close the structural-drift-catches sweep on
        // every default composition the trait exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedFindHintKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedFindHintKind(pub String);
        impl core::fmt::Display for UnknownDriftedFindHintKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted find hint kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedFindHintKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted find hint kind";
            type Unknown = UnknownDriftedFindHintKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedFindHintKind(s.to_owned())
            }
            fn find_by_label_with_hint(_s: &str) -> Result<Self, Option<Self>> {
                // Drifted override — fabricates a hint for every
                // input, including the unrecognizable probe the
                // testkit's clause (13) reserves. Fails the
                // conservative-suggestion contract on the non-
                // allocating column of the (side-effect × hint) 2×2
                // matrix.
                Err(Some(Self::Only))
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedFindHintKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a find_by_label_with_hint override that fabricated a hint for the unrecognizable probe",
        );
    }

    #[test]
    fn cardinality_matches_all_len() {
        // The const-visible variant count MUST equal `Self::ALL`'s
        // runtime slice length — the default trait const initializer
        // `Self::ALL.len()` satisfies the invariant for free. Pinning
        // the equality here on `StubKind` (3 variants) means a
        // regression that drifts the trait const from the natural
        // `ALL`-length projection (a hand-rolled `const CARDINALITY:
        // usize = 4` on a 3-variant enum) fails this stub-level
        // contract before any per-implementor const-generic surface
        // depends on the count downstream.
        assert_eq!(<StubKind as ClosedSet>::CARDINALITY, 3);
        assert_eq!(
            <StubKind as ClosedSet>::CARDINALITY,
            <StubKind as ClosedSet>::ALL.len(),
        );
    }

    #[test]
    fn cardinality_is_const_evaluable_at_compile_time() {
        // The load-bearing property `CARDINALITY` names — the count
        // is compile-time-known through `<[T]>::len`'s const stability
        // (Rust 1.39+), so downstream const-generic consumers can
        // bind `[Payload; T::CARDINALITY]` at their type signature
        // without threading `T::ALL.len()` through a `const fn`
        // wrapper. Pinning the const-evaluability with a `const N:
        // usize` binding here means a regression that makes the const
        // non-const (a future override that reaches into a `fn` body,
        // a future defaulted-const that references a non-const
        // primitive) fails compilation on this test rather than
        // silently degrading the const-generic surface at every
        // downstream consumer's build. The array binding demonstrates
        // the primary consumer pattern — `[Payload; T::CARDINALITY]`
        // with `Payload = StubKind` here — and asserts the resulting
        // dimension against `T::ALL`'s runtime layout, so the const
        // path AND the runtime path stay aligned at the primitive.
        const N: usize = <StubKind as ClosedSet>::CARDINALITY;
        assert_eq!(N, 3);
        let per_variant_slots: [Option<StubKind>; N] = [None; N];
        assert_eq!(per_variant_slots.len(), <StubKind as ClosedSet>::ALL.len());
    }

    #[test]
    fn cardinality_agrees_with_labels_and_sorted_labels_lengths() {
        // Alignment across the closed set's derived surfaces —
        // `T::CARDINALITY`, `T::ALL.len()`, `T::labels().len()`, and
        // `T::sorted_labels().len()` all name the SAME count through
        // different projections. `T::labels()` projects `T::ALL` +
        // `T::label`; `T::sorted_labels()` projects `T::labels()` +
        // `slice::sort_unstable`; neither shrinks nor grows the
        // cardinality. Pinning the four-way alignment here catches
        // a drift in ANY of the intermediate projections (a
        // duplicate-label bug that silently folds two variants into
        // one at the labels surface, a subset-override on
        // `sorted_labels` that names fewer labels than `ALL` carries)
        // before any generic consumer routes through the drifted
        // projection.
        let expected = <StubKind as ClosedSet>::CARDINALITY;
        assert_eq!(<StubKind as ClosedSet>::ALL.len(), expected);
        assert_eq!(<StubKind as ClosedSet>::labels().len(), expected);
        assert_eq!(<StubKind as ClosedSet>::sorted_labels().len(), expected);
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_cardinality_and_all_len() {
        // The well-formedness sweep's (14) clause — `T::CARDINALITY`
        // MUST equal `T::ALL.len()`. A hand-impl'd implementor whose
        // override drifts the count (a future `const CARDINALITY:
        // usize = N` on an enum whose `ALL` slice carries a different
        // count) fails the sweep loudly rather than silently
        // bifurcating the const-generic surface every downstream
        // `[Payload; T::CARDINALITY]` consumer routes through.
        // Pinning the failure path here keeps the testkit's (14)
        // clause guaranteed-to-fire — a regression that makes the
        // assertion permissive (e.g. a future "either agrees or
        // exceeds" relaxation) breaks this stub-level contract
        // before any per-implementor sweep runs. Sibling posture to
        // the thirteen sibling `_catches_drift_between_*` pins above
        // (clauses 5-13); together they close the structural-drift-
        // catches sweep on every default composition the trait
        // exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedCardinalityKind {
            First,
            Second,
        }
        #[derive(Debug)]
        struct UnknownDriftedCardinalityKind(pub String);
        impl core::fmt::Display for UnknownDriftedCardinalityKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted cardinality kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedCardinalityKind {
            const ALL: &'static [Self] = &[Self::First, Self::Second];
            // Drifted override — reports 5 when `ALL` carries 2.
            // A downstream const-generic consumer that binds
            // `[Payload; T::CARDINALITY]` against `T::ALL`-length
            // iteration would silently size the wrong dimension.
            const CARDINALITY: usize = 5;
            const SET_LABEL: &'static str = "drifted cardinality kind";
            type Unknown = UnknownDriftedCardinalityKind;
            fn label(self) -> &'static str {
                match self {
                    Self::First => "first",
                    Self::Second => "second",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedCardinalityKind(s.to_owned())
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedCardinalityKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a CARDINALITY override drifted from the natural `T::ALL.len()` projection",
        );
    }

    #[test]
    fn index_of_returns_declaration_order_position_for_every_variant() {
        // The (variant → array index) projection — each variant's
        // `index_of()` returns its zero-indexed position in
        // `Self::ALL`. Pinning the per-variant projection here means a
        // regression that drifts the position (a future override that
        // swaps two arms, an over-eager caching layer that reports a
        // stale index) fails this stub-level contract before any
        // per-implementor const-generic surface depends on the
        // bijection downstream. The `StubKind` variants (Alpha, Beta,
        // Gamma) declare in the (0, 1, 2) positions the trait's
        // default discriminant-keyed sweep MUST recover.
        assert_eq!(<StubKind as ClosedSet>::index_of(StubKind::Alpha), 0);
        assert_eq!(<StubKind as ClosedSet>::index_of(StubKind::Beta), 1);
        assert_eq!(<StubKind as ClosedSet>::index_of(StubKind::Gamma), 2);
    }

    #[test]
    fn index_of_round_trips_through_all_indexing_into_the_original_variant() {
        // The bijection contract — `T::ALL[v.index_of()] == v` for
        // every variant. Pinning the round-trip here means the
        // (typed variant → array index) projection AND the
        // (array index → typed variant) `T::ALL[i]` projection stay
        // aligned at the trait surface; a downstream per-variant
        // lookup-table consumer that constructs `[Payload;
        // T::CARDINALITY]` and keys on `variant.index_of()` for
        // read, `T::ALL[i]` for write, MUST see the same variant on
        // both sides of the projection pair. A regression on EITHER
        // side of the bijection (a permissive `index_of` override, a
        // `Self::ALL` that skips a variant) fails this contract
        // stub-level before any per-implementor sweep depends on the
        // bijection downstream. Sibling posture to
        // `parse_label_round_trips_every_variant` one vocabulary over
        // — this pin covers the (variant → array index) round-trip
        // instead of the (variant → label) round-trip.
        for &v in <StubKind as ClosedSet>::ALL {
            let idx = <StubKind as ClosedSet>::index_of(v);
            let round_tripped = <StubKind as ClosedSet>::ALL[idx];
            assert_eq!(
                round_tripped, v,
                "T::ALL[{idx}] failed to recover the original variant {v:?} — the (variant → array index) bijection with 0..T::CARDINALITY broke",
            );
        }
    }

    #[test]
    fn index_of_stays_within_zero_to_cardinality() {
        // The bounded-index contract — `v.index_of() < T::CARDINALITY`
        // for every variant. Pinning the bound here means a downstream
        // const-generic consumer that binds `[Payload;
        // T::CARDINALITY]` and reads `table[variant.index_of()]` stays
        // sound at the compile-time array-dimension boundary — the
        // per-variant index CANNOT overshoot the const-sized array's
        // top-rank index. Sibling posture to
        // `cardinality_matches_all_len` one axis over — that pin ties
        // the const-visible cardinality to the runtime slice length,
        // this pin ties the per-variant index to that const-visible
        // cardinality so both endpoints of the (typed variant ↔
        // array-index position) bijection stay honest.
        let card = <StubKind as ClosedSet>::CARDINALITY;
        for &v in <StubKind as ClosedSet>::ALL {
            let idx = <StubKind as ClosedSet>::index_of(v);
            assert!(
                idx < card,
                "index_of({v:?}) returned {idx}, which is outside the closed set's 0..{card} range — the bijection with 0..T::CARDINALITY broke",
            );
        }
    }

    #[test]
    fn index_of_projects_all_indexing_and_index_of_into_the_identity_permutation() {
        // The exhaustive-permutation contract — collecting
        // `v.index_of()` for each `v in T::ALL.iter()` yields the
        // identity permutation `0..T::CARDINALITY`. Pinning the
        // permutation shape here means a regression that folds two
        // variants onto the same index (a hand-rolled `match` that
        // returns a constant, an over-eager caching layer that stales
        // on a variant-listing edit) fails this stub-level contract
        // even when the individual per-variant assertions would still
        // pass in isolation. The identity-permutation shape is the
        // bijection every downstream per-variant lookup-table /
        // bitset consumer implicitly relies on — no two variants
        // share an index, and every index in `0..T::CARDINALITY` is
        // reached by some variant.
        let indices: Vec<usize> = <StubKind as ClosedSet>::ALL
            .iter()
            .copied()
            .map(<StubKind as ClosedSet>::index_of)
            .collect();
        let identity: Vec<usize> = (0..<StubKind as ClosedSet>::CARDINALITY).collect();
        assert_eq!(
            indices, identity,
            "T::ALL projected through index_of failed to yield the identity permutation 0..T::CARDINALITY — the bijection between variants and array indices broke",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_index_of_and_all_position() {
        // The well-formedness sweep's (15) clause —
        // `T::ALL[i].index_of() == i` for every `i in 0..T::ALL.len()`.
        // A hand-impl'd implementor whose override drifts the
        // position (a hand-rolled `match` that swaps two arms, a
        // constant that reports the same index for every variant, an
        // over-eager caching layer that returns a stale index) fails
        // the sweep loudly rather than silently bifurcating the
        // (variant → array index) bijection with `0..T::CARDINALITY`
        // every downstream per-variant lookup-table / bitset /
        // compact-encoding consumer routes through. Pinning the
        // failure path here keeps the testkit's (15) clause
        // guaranteed-to-fire — a regression that makes the assertion
        // permissive (e.g. a future "any position within bound"
        // relaxation) breaks this stub-level contract before any
        // per-implementor sweep runs. Sibling posture to the fourteen
        // sibling `_catches_drift_between_*` pins above (clauses
        // 5-14); together they close the structural-drift-catches
        // sweep on every default composition the trait exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedIndexKind {
            First,
            Second,
        }
        #[derive(Debug)]
        struct UnknownDriftedIndexKind(pub String);
        impl core::fmt::Display for UnknownDriftedIndexKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted index kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedIndexKind {
            const ALL: &'static [Self] = &[Self::First, Self::Second];
            const SET_LABEL: &'static str = "drifted index kind";
            type Unknown = UnknownDriftedIndexKind;
            fn label(self) -> &'static str {
                match self {
                    Self::First => "first",
                    Self::Second => "second",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedIndexKind(s.to_owned())
            }
            fn index_of(self) -> usize {
                // Drifted override — swaps the two arms, breaking the
                // bijection with the declaration-order positions.
                match self {
                    Self::First => 1,
                    Self::Second => 0,
                }
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedIndexKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted an index_of override drifted from the natural T::ALL-position projection",
        );
    }

    #[test]
    fn from_index_returns_declaration_order_variant_for_every_index() {
        // The (array index → variant) projection — each in-range
        // `from_index(i)` returns `Some(v)` where `v` is `T::ALL[i]`,
        // pinning the direct slice-lookup arm the default trait body
        // exposes. The `StubKind` variants (Alpha, Beta, Gamma)
        // declare in the (0, 1, 2) positions the trait's default
        // `Self::ALL.get(i).copied()` composition MUST recover.
        assert_eq!(
            <StubKind as ClosedSet>::from_index(0),
            Some(StubKind::Alpha),
        );
        assert_eq!(<StubKind as ClosedSet>::from_index(1), Some(StubKind::Beta));
        assert_eq!(
            <StubKind as ClosedSet>::from_index(2),
            Some(StubKind::Gamma),
        );
    }

    #[test]
    fn from_index_returns_none_for_out_of_range_index() {
        // The bounded-decode contract — `from_index(i)` returns `None`
        // for every `i >= T::CARDINALITY`. Pinning the out-of-range
        // guard here means a downstream compact-encoding consumer that
        // decodes a corrupt serialized index (a wire byte that
        // survived a bit flip, a deserializer that fed the wrong u8
        // slot) surfaces the corruption at the decode boundary as
        // `None` rather than silently folding onto an in-range
        // variant. Sweep the first out-of-range index (equals
        // `T::CARDINALITY`) AND a far-out-of-range index
        // (`T::CARDINALITY + 100`) AND the `usize::MAX` sentinel — the
        // three probes catch a permissive override that widens the
        // in-range domain by any amount OR one that special-cases the
        // `T::CARDINALITY` boundary while accepting `T::CARDINALITY +
        // 100` OR one that accepts `usize::MAX` through an unchecked
        // arithmetic path.
        let card = <StubKind as ClosedSet>::CARDINALITY;
        assert!(
            <StubKind as ClosedSet>::from_index(card).is_none(),
            "from_index(CARDINALITY = {card}) returned Some — the out-of-range guard drifted at the boundary",
        );
        assert!(
            <StubKind as ClosedSet>::from_index(card + 100).is_none(),
            "from_index(CARDINALITY + 100) returned Some — the out-of-range guard drifted for far-out-of-range indices",
        );
        assert!(
            <StubKind as ClosedSet>::from_index(usize::MAX).is_none(),
            "from_index(usize::MAX) returned Some — the out-of-range guard drifted for the top-rank sentinel",
        );
    }

    #[test]
    fn from_index_round_trips_through_index_of_into_the_original_variant() {
        // The forward round-trip — `from_index(v.index_of()) ==
        // Some(v)` for every variant `v in T::ALL.iter()`. Pinning the
        // round-trip here means the (typed variant → array index)
        // projection AND the (array index → typed variant) projection
        // stay aligned at the trait surface; a downstream consumer
        // that stores `variant.index_of()` for later decode (a compact
        // wire encoding, a slotted lookup table walked back to
        // `(variant, payload)` pairs, a metrics aggregator rendered
        // back as `<variant>: <count>` diagnostics) MUST see the same
        // variant on both sides of the projection pair. A regression
        // on EITHER side of the bijection (a permissive `index_of`
        // override, a strict `from_index` override that drops a valid
        // in-range index) fails this contract stub-level before any
        // per-implementor sweep depends on the bijection downstream.
        // Sibling posture to
        // `index_of_round_trips_through_all_indexing_into_the_original_variant`
        // one direction over — that pin covers the direct
        // `T::ALL[v.index_of()] == v` round-trip, this pin covers the
        // typed inverse `T::from_index(v.index_of()) == Some(v)`
        // round-trip so both round-trip surfaces stay honest.
        for &v in <StubKind as ClosedSet>::ALL {
            let idx = <StubKind as ClosedSet>::index_of(v);
            let recovered = <StubKind as ClosedSet>::from_index(idx);
            assert_eq!(
                recovered,
                Some(v),
                "from_index(index_of({v:?}) = {idx}) failed to recover the original variant — the (variant ↔ array index) forward round-trip broke",
            );
        }
    }

    #[test]
    fn from_index_reverse_round_trips_through_index_of_into_the_original_index() {
        // The reverse round-trip — `from_index(i).unwrap().index_of()
        // == i` for every `i in 0..T::CARDINALITY`. Pinning the
        // reverse round-trip here means the (array index → typed
        // variant → array index) composition stays the identity
        // permutation on the closed set's `0..T::CARDINALITY` index
        // domain; a downstream consumer that iterates a `[Payload;
        // T::CARDINALITY]` lookup table by index, decodes each slot's
        // index back to a typed variant, and re-encodes for storage
        // MUST see the same index on both sides of the projection
        // pair. Sibling posture to
        // `from_index_round_trips_through_index_of_into_the_original_variant`
        // one direction over — that pin covers the (variant → index →
        // variant) forward round-trip, this pin covers the (index →
        // variant → index) reverse round-trip so both round-trip
        // surfaces close the bijection with `0..T::CARDINALITY` in
        // BOTH directions.
        let card = <StubKind as ClosedSet>::CARDINALITY;
        for i in 0..card {
            let v = <StubKind as ClosedSet>::from_index(i)
                .expect("from_index in 0..CARDINALITY returned None on a valid index");
            let round_tripped = <StubKind as ClosedSet>::index_of(v);
            assert_eq!(
                round_tripped, i,
                "index_of(from_index({i}) = {v:?}) failed to recover the original index — the (array index ↔ variant) reverse round-trip broke",
            );
        }
    }

    #[test]
    fn from_index_projects_zero_to_cardinality_and_from_index_into_all_verbatim() {
        // The exhaustive-permutation contract — collecting
        // `from_index(i).unwrap()` for each `i in 0..T::CARDINALITY`
        // yields the `T::ALL` slice verbatim. Pinning the permutation
        // shape here means a regression that folds two indices onto
        // the same variant (a hand-rolled `match i` that returns a
        // constant, an over-eager caching layer that stales on a
        // variant-listing edit) fails this stub-level contract even
        // when the individual per-index assertions would still pass
        // in isolation. The identity-permutation shape is the
        // bijection every downstream per-variant lookup-table /
        // bitset consumer implicitly relies on — no two indices
        // decode to the same variant, and every variant in `T::ALL`
        // is reached by some index in `0..T::CARDINALITY`.
        let card = <StubKind as ClosedSet>::CARDINALITY;
        let variants: Vec<StubKind> = (0..card)
            .map(|i| {
                <StubKind as ClosedSet>::from_index(i)
                    .expect("from_index in 0..CARDINALITY returned None on a valid index")
            })
            .collect();
        let all: Vec<StubKind> = <StubKind as ClosedSet>::ALL.to_vec();
        assert_eq!(
            variants, all,
            "0..T::CARDINALITY projected through from_index failed to yield T::ALL verbatim — the (array index → variant) inverse projection drifted from the natural T::ALL indexing",
        );
    }

    #[test]
    fn from_index_agrees_with_all_get_copied_on_every_probe() {
        // The natural-projection alignment — `from_index(i)` MUST
        // equal `T::ALL.get(i).copied()` on every representative
        // input: every in-range index (`0..T::CARDINALITY`) matches
        // (`Some(T::ALL[i])`), the first out-of-range index
        // (`T::CARDINALITY`) rejects (`None`), and the top-rank
        // sentinel (`usize::MAX`) rejects (`None`). The default trait
        // body `Self::ALL.get(i).copied()` composition satisfies the
        // alignment for free; the alignment lets a generic consumer
        // freely swap between the trait method and the inline
        // `T::ALL.get(i).copied()` composition without changing the
        // program's structured-decode semantics.
        let card = <StubKind as ClosedSet>::CARDINALITY;
        for i in 0..card {
            assert_eq!(
                <StubKind as ClosedSet>::from_index(i),
                <StubKind as ClosedSet>::ALL.get(i).copied(),
                "from_index({i}) drifted from ALL.get({i}).copied() on the in-range domain",
            );
        }
        assert_eq!(
            <StubKind as ClosedSet>::from_index(card),
            <StubKind as ClosedSet>::ALL.get(card).copied(),
            "from_index(CARDINALITY) drifted from ALL.get(CARDINALITY).copied() at the boundary",
        );
        assert_eq!(
            <StubKind as ClosedSet>::from_index(usize::MAX),
            <StubKind as ClosedSet>::ALL.get(usize::MAX).copied(),
            "from_index(usize::MAX) drifted from ALL.get(usize::MAX).copied() at the top-rank sentinel",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_from_index_and_all_indexing() {
        // The well-formedness sweep's (16) clause — `T::from_index(i)
        // == Some(T::ALL[i])` on the in-range domain AND
        // `T::from_index(T::CARDINALITY) == None` at the out-of-range
        // boundary. A hand-impl'd implementor whose override drifts
        // the bounded-decode arm (a permissive override that returns
        // `Some` for an out-of-range index, a strict override that
        // returns `None` for a valid in-range index, a swapped
        // override that recovers the wrong variant for a valid index)
        // fails the sweep loudly rather than silently bifurcating the
        // (array index → typed variant) inverse projection every
        // downstream compact-encoding / bitset / lookup-table
        // consumer routes through. Pinning the failure path here
        // keeps the testkit's (16) clause guaranteed-to-fire — a
        // regression that makes the assertion permissive (e.g. a
        // future "any variant within bound" relaxation) breaks this
        // stub-level contract before any per-implementor sweep runs.
        // Sibling posture to the fifteen sibling
        // `_catches_drift_between_*` pins above (clauses 5-15);
        // together they close the structural-drift-catches sweep on
        // every default composition the trait exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedFromIndexKind {
            First,
            Second,
        }
        #[derive(Debug)]
        struct UnknownDriftedFromIndexKind(pub String);
        impl core::fmt::Display for UnknownDriftedFromIndexKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted from_index kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedFromIndexKind {
            const ALL: &'static [Self] = &[Self::First, Self::Second];
            const SET_LABEL: &'static str = "drifted from_index kind";
            type Unknown = UnknownDriftedFromIndexKind;
            fn label(self) -> &'static str {
                match self {
                    Self::First => "first",
                    Self::Second => "second",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedFromIndexKind(s.to_owned())
            }
            fn from_index(_i: usize) -> Option<Self> {
                // Drifted override — always returns `Some(First)`,
                // folding index 1 onto index 0's variant AND accepting
                // every out-of-range index. Breaks BOTH the in-range
                // per-slot bijection AND the out-of-range guard.
                Some(Self::First)
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedFromIndexKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a from_index override drifted from the natural Self::ALL.get(i).copied() projection",
        );
    }

    #[test]
    fn sorted_variants_returns_typed_variants_in_lexicographic_label_order() {
        // The sorted-typed-variants surface — `T::sorted_variants()`
        // returns typed variants ordered by ASCII lexicographic
        // `label()`. The `StubKind` labels (`alpha`/`beta`/`gamma`)
        // are already in lexicographic declaration order, so the sort
        // step is the identity permutation on the (Alpha, Beta, Gamma)
        // typed variants; the `ReverseStubKind` pin below exercises
        // the actual sort discipline against an out-of-order
        // declaration. Sibling posture to
        // `sorted_labels_renders_labels_in_lexicographic_order` one
        // axis over on the (typed variant, canonical label)
        // return-type axis — this pin covers the `Vec<Self>` corner
        // of the (return-type × ordering) 2×2 matrix.
        assert_eq!(
            <StubKind as ClosedSet>::sorted_variants(),
            vec![StubKind::Alpha, StubKind::Beta, StubKind::Gamma],
        );
    }

    #[test]
    fn sorted_variants_normalizes_arbitrary_declaration_order() {
        // The sort-step contract on the typed-variant surface —
        // `T::sorted_variants()` MUST normalize an arbitrary
        // declaration order into ASCII lexicographic `label()`
        // order, regardless of the implementor's `ALL`-array
        // layout. A regression that returns `T::ALL.to_vec()`
        // verbatim (without the sort step) would pass
        // `sorted_variants_returns_typed_variants_in_lexicographic_label_order`
        // on `StubKind` (because its labels already sit in order)
        // but silently bifurcate the canonical-ordering surface for
        // any implementor whose declaration order differs from
        // byte-wise sort order. Pinning the sort discipline here
        // with a deliberately-out-of-order stub catches that drift
        // directly. Sibling posture to
        // `sorted_labels_normalizes_arbitrary_declaration_order`
        // one axis over on the `Vec<Self>` return-type — this pin
        // covers the typed-variant surface.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum ReverseVariantStubKind {
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownReverseVariantStubKind(pub String);
        impl core::fmt::Display for UnknownReverseVariantStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown reverse variant stub kind: {}", self.0)
            }
        }
        impl ClosedSet for ReverseVariantStubKind {
            const ALL: &'static [Self] = &[Self::Gamma, Self::Beta, Self::Alpha];
            const SET_LABEL: &'static str = "reverse variant stub kind";
            type Unknown = UnknownReverseVariantStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownReverseVariantStubKind(s.to_owned())
            }
        }
        // `T::ALL` preserves declaration order — Gamma, Beta, Alpha.
        assert_eq!(
            <ReverseVariantStubKind as ClosedSet>::ALL,
            &[
                ReverseVariantStubKind::Gamma,
                ReverseVariantStubKind::Beta,
                ReverseVariantStubKind::Alpha,
            ],
        );
        // `sorted_variants()` normalizes to lexicographic-label order
        // — Alpha, Beta, Gamma. The composition with
        // `sort_unstable_by_key(|v| v.label())` is the load-bearing
        // step the lift names.
        assert_eq!(
            <ReverseVariantStubKind as ClosedSet>::sorted_variants(),
            vec![
                ReverseVariantStubKind::Alpha,
                ReverseVariantStubKind::Beta,
                ReverseVariantStubKind::Gamma,
            ],
        );
    }

    #[test]
    fn sorted_variants_stays_element_wise_aligned_with_sorted_labels() {
        // The load-bearing invariant — `sorted_variants()[i].label()`
        // equals `sorted_labels()[i]` for every `i in
        // 0..T::CARDINALITY`. Pinning the element-wise alignment
        // here means a downstream LSP / `tatara-check` / metrics
        // consumer that walks `zip(sorted_variants(),
        // sorted_labels())` per-slot sees the SAME (typed variant,
        // canonical label) pair on both projections — a regression
        // that permutes ONE arm without the other would silently
        // bifurcate the pairing at the per-slot boundary. Sibling
        // posture to `sorted_labels_renders_labels_in_lexicographic_order`
        // one axis over — this pin covers the alignment across the
        // two lexicographic corners of the (return-type × ordering)
        // 2×2 matrix.
        let variants = <StubKind as ClosedSet>::sorted_variants();
        let labels = <StubKind as ClosedSet>::sorted_labels();
        assert_eq!(variants.len(), labels.len());
        for (i, (v, l)) in variants
            .iter()
            .copied()
            .zip(labels.iter().copied())
            .enumerate()
        {
            assert_eq!(
                v.label(),
                l,
                "sorted_variants()[{i}].label() drifted from sorted_labels()[{i}] — the (typed variant, canonical label) alignment on the lexicographic-ordering axis broke",
            );
        }
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_sorted_variants_and_composition() {
        // The well-formedness sweep's (17) clause —
        // `T::sorted_variants()` MUST compose `T::ALL` +
        // `Vec::from` + `slice::sort_unstable_by_key` keyed on
        // `label` verbatim, AND stay element-wise aligned with
        // `T::sorted_labels()` on the (typed variant, canonical
        // label) axis. A hand-impl'd implementor whose override
        // drifts the composition (a subset of variants, a swapped
        // variant, a different ordering, an off-by-one length)
        // fails the sweep loudly rather than silently bifurcating
        // the sorted-typed-variant candidate-list surface every LSP
        // / `tatara-check` / metrics consumer routes through.
        // Pinning the failure path here keeps the testkit's (17)
        // clause guaranteed-to-fire — a regression that makes the
        // assertion permissive (e.g. a future "any permutation"
        // relaxation) breaks this stub-level contract before any
        // per-implementor sweep runs. Sibling posture to the
        // sixteen sibling `_catches_drift_between_*` pins above
        // (clauses 5-16); together they close the
        // structural-drift-catches sweep on every default
        // composition the trait exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedSortedVariantsKind {
            First,
            Second,
        }
        #[derive(Debug)]
        struct UnknownDriftedSortedVariantsKind(pub String);
        impl core::fmt::Display for UnknownDriftedSortedVariantsKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted sorted variants kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedSortedVariantsKind {
            const ALL: &'static [Self] = &[Self::First, Self::Second];
            const SET_LABEL: &'static str = "drifted sorted variants kind";
            type Unknown = UnknownDriftedSortedVariantsKind;
            fn label(self) -> &'static str {
                match self {
                    // Labels deliberately reorder under sort: "zeta"
                    // precedes "alpha" in declaration but follows in
                    // ASCII sort. The natural `sorted_variants()`
                    // returns [Second, First] (label order alpha,
                    // zeta); the drifted override returns [First,
                    // Second] (declaration order) — the well-formedness
                    // clause (17) catches the misalignment via
                    // `sorted_variants()[i].label() ==
                    // sorted_labels()[i]`.
                    Self::First => "zeta",
                    Self::Second => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedSortedVariantsKind(s.to_owned())
            }
            fn sorted_variants() -> Vec<Self> {
                // Drifted override — returns variants in declaration
                // order rather than lexicographic-label order,
                // bifurcating the (typed variant, canonical label)
                // alignment with `sorted_labels()`.
                vec![Self::First, Self::Second]
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedSortedVariantsKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a sorted_variants override drifted from the natural ALL-then-sort-by-label composition",
        );
    }

    #[test]
    fn first_returns_the_declaration_order_head_variant() {
        // The declaration-order head endpoint anchor — `T::first()`
        // returns `T::ALL[0]` as a bare typed variant with no
        // `Option` / `Result` indirection. The `StubKind` variant
        // listing is `[Alpha, Beta, Gamma]`, so the head anchor is
        // `Alpha`. Sibling posture to
        // `sorted_variants_returns_typed_variants_in_lexicographic_label_order`
        // on the (endpoint-anchor, sorted-listing) axis — both walk
        // `T::ALL` as their load-bearing primitive, one at the
        // slice-index-0 endpoint, the other at every slot under a
        // key-projected sort.
        assert_eq!(<StubKind as ClosedSet>::first(), StubKind::Alpha);
    }

    #[test]
    fn last_returns_the_declaration_order_tail_variant() {
        // The declaration-order tail endpoint anchor — `T::last()`
        // returns `T::ALL[T::ALL.len() - 1]` as a bare typed variant.
        // The `StubKind` variant listing is `[Alpha, Beta, Gamma]`,
        // so the tail anchor is `Gamma`. Sibling posture to
        // `first_returns_the_declaration_order_head_variant` one axis
        // over on the (head, tail) endpoint-direction partition.
        assert_eq!(<StubKind as ClosedSet>::last(), StubKind::Gamma);
    }

    #[test]
    fn first_and_last_bracket_all_slice_on_arbitrary_declaration_order() {
        // The endpoint-anchor contract on an arbitrary declaration
        // order — `T::first()` MUST project the head of `T::ALL`
        // regardless of the implementor's `ALL`-array layout, AND
        // `T::last()` MUST project the tail. A regression that
        // returns a hard-coded variant literal (rather than
        // slice-indexing `T::ALL`) would pass
        // `first_returns_the_declaration_order_head_variant` /
        // `last_returns_the_declaration_order_tail_variant` on
        // `StubKind` (because its declaration order happens to align
        // with any conceivable hard-coded literal) but silently
        // bifurcate the endpoint anchors on any implementor whose
        // `ALL`-array layout differs. Pinning the anchor discipline
        // here with a deliberately-out-of-order stub catches that
        // drift directly. Reuses the `ReverseVariantStubKind`-shaped
        // stub the `sorted_variants_normalizes_arbitrary_declaration_order`
        // sibling exercises so the endpoint contract lands on the
        // SAME declaration-order-inversion probe the sorted-variants
        // contract binds against.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum EndpointStubKind {
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownEndpointStubKind(pub String);
        impl core::fmt::Display for UnknownEndpointStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown endpoint stub kind: {}", self.0)
            }
        }
        impl ClosedSet for EndpointStubKind {
            const ALL: &'static [Self] = &[Self::Gamma, Self::Beta, Self::Alpha];
            const SET_LABEL: &'static str = "endpoint stub kind";
            type Unknown = UnknownEndpointStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownEndpointStubKind(s.to_owned())
            }
        }
        // `T::first()` picks up the head of the declaration order
        // (Gamma), NOT some hard-coded literal `Alpha` — the anchor
        // reads `T::ALL[0]` at runtime.
        assert_eq!(
            <EndpointStubKind as ClosedSet>::first(),
            EndpointStubKind::Gamma,
        );
        // `T::last()` picks up the tail of the declaration order
        // (Alpha), NOT some hard-coded literal `Gamma`.
        assert_eq!(
            <EndpointStubKind as ClosedSet>::last(),
            EndpointStubKind::Alpha,
        );
    }

    #[test]
    fn first_collapses_with_last_on_singleton_closed_set() {
        // The endpoint-anchor degenerate case — a singleton closed
        // set (one variant) has `T::ALL[0] == T::ALL[T::ALL.len() -
        // 1]`, so `T::first() == T::last()`. This is a corner the
        // (head, tail) partition of the endpoint surface REALLY does
        // include, and a regression that treated the two anchors
        // as structurally distinct (a hand-rolled `match` that
        // returned different variants for `first()` vs `last()`)
        // would silently break the natural degenerate-case invariant
        // downstream consumers rely on. Pinning the collapse here
        // catches that drift.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum SingletonStubKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownSingletonStubKind(pub String);
        impl core::fmt::Display for UnknownSingletonStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown singleton stub kind: {}", self.0)
            }
        }
        impl ClosedSet for SingletonStubKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "singleton stub kind";
            type Unknown = UnknownSingletonStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Only => "only",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownSingletonStubKind(s.to_owned())
            }
        }
        assert_eq!(
            <SingletonStubKind as ClosedSet>::first(),
            SingletonStubKind::Only,
        );
        assert_eq!(
            <SingletonStubKind as ClosedSet>::last(),
            SingletonStubKind::Only,
        );
        assert_eq!(
            <SingletonStubKind as ClosedSet>::first(),
            <SingletonStubKind as ClosedSet>::last(),
        );
    }

    #[test]
    fn first_and_last_agree_with_from_index_at_the_endpoint_slots() {
        // The endpoint-anchor / index-decode alignment — `T::first()`
        // MUST equal `T::from_index(0).unwrap()`, AND `T::last()` MUST
        // equal `T::from_index(T::CARDINALITY - 1).unwrap()`. The two
        // primitives project the same `T::ALL` slice at the same
        // endpoint slots through different surfaces (bare typed
        // anchor vs `Option`-typed bounded-index decode), and a
        // downstream consumer freely swaps between the two depending
        // on whether it wants the panic-free anchor or the
        // `Option`-typed decode. Pinning the agreement here catches a
        // future regression that drifts either surface from the
        // shared `Self::ALL[0]` / `Self::ALL[CARDINALITY - 1]`
        // projection.
        assert_eq!(
            <StubKind as ClosedSet>::first(),
            <StubKind as ClosedSet>::from_index(0)
                .expect("first(): CARDINALITY >= 1 by clause (1)"),
        );
        assert_eq!(
            <StubKind as ClosedSet>::last(),
            <StubKind as ClosedSet>::from_index(<StubKind as ClosedSet>::CARDINALITY - 1)
                .expect("last(): CARDINALITY >= 1 by clause (1)"),
        );
    }

    #[test]
    fn is_first_returns_true_only_on_the_declaration_order_head_variant() {
        // The declaration-order head-endpoint membership predicate
        // fires exactly on the head anchor and nowhere else.
        // `StubKind`'s variant listing is `[Alpha, Beta, Gamma]`, so
        // `Alpha.is_first()` is `true` and both `Beta.is_first()` and
        // `Gamma.is_first()` are `false`. Sibling posture to
        // `first_returns_the_declaration_order_head_variant` on the
        // (`Self`-anchor, `bool`-membership) return-type axis.
        assert!(<StubKind as ClosedSet>::is_first(StubKind::Alpha));
        assert!(!<StubKind as ClosedSet>::is_first(StubKind::Beta));
        assert!(!<StubKind as ClosedSet>::is_first(StubKind::Gamma));
    }

    #[test]
    fn is_last_returns_true_only_on_the_declaration_order_tail_variant() {
        // The declaration-order tail-endpoint membership predicate
        // fires exactly on the tail anchor and nowhere else. Sibling
        // posture to `is_first_returns_true_only_on_the_declaration_order_head_variant`
        // one direction over on the (head, tail) partition — the
        // stub's tail is `Gamma`.
        assert!(!<StubKind as ClosedSet>::is_last(StubKind::Alpha));
        assert!(!<StubKind as ClosedSet>::is_last(StubKind::Beta));
        assert!(<StubKind as ClosedSet>::is_last(StubKind::Gamma));
    }

    #[test]
    fn is_first_and_is_last_bracket_all_slice_on_arbitrary_declaration_order() {
        // The endpoint-membership contract on an arbitrary declaration
        // order — `T::first().is_first()` MUST be `true` regardless of
        // which variant literal happens to sit at slice-index-0, AND
        // `T::last().is_last()` MUST be `true` regardless of which
        // variant literal happens to sit at slice-index-(N - 1). A
        // regression that hard-coded the predicate against a variant
        // literal rather than routing through `index_of` would pass
        // `is_first_returns_true_only_on_the_declaration_order_head_variant`
        // / `is_last_returns_true_only_on_the_declaration_order_tail_variant`
        // on `StubKind` and silently bifurcate the predicates on any
        // implementor whose `ALL`-array layout differs.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum EndpointMembershipStubKind {
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownEndpointMembershipStubKind(pub String);
        impl core::fmt::Display for UnknownEndpointMembershipStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown endpoint membership stub kind: {}", self.0)
            }
        }
        impl ClosedSet for EndpointMembershipStubKind {
            const ALL: &'static [Self] = &[Self::Gamma, Self::Beta, Self::Alpha];
            const SET_LABEL: &'static str = "endpoint membership stub kind";
            type Unknown = UnknownEndpointMembershipStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownEndpointMembershipStubKind(s.to_owned())
            }
        }
        assert!(<EndpointMembershipStubKind as ClosedSet>::is_first(
            EndpointMembershipStubKind::Gamma,
        ));
        assert!(!<EndpointMembershipStubKind as ClosedSet>::is_first(
            EndpointMembershipStubKind::Beta,
        ));
        assert!(!<EndpointMembershipStubKind as ClosedSet>::is_first(
            EndpointMembershipStubKind::Alpha,
        ));
        assert!(!<EndpointMembershipStubKind as ClosedSet>::is_last(
            EndpointMembershipStubKind::Gamma,
        ));
        assert!(!<EndpointMembershipStubKind as ClosedSet>::is_last(
            EndpointMembershipStubKind::Beta,
        ));
        assert!(<EndpointMembershipStubKind as ClosedSet>::is_last(
            EndpointMembershipStubKind::Alpha,
        ));
    }

    #[test]
    fn is_first_and_is_last_collapse_true_on_singleton_closed_set() {
        // The endpoint-membership degenerate case — a singleton closed
        // set has ONE variant with `index_of == 0` AND
        // `index_of + 1 == 1 == CARDINALITY`, so both predicates fire
        // on the same variant. Mirrors `first_collapses_with_last_on_singleton_closed_set`
        // one return-type axis over — the two anchor arms collapse on
        // the same variant, the two membership arms both fire on the
        // same variant.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum SingletonMembershipStubKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownSingletonMembershipStubKind(pub String);
        impl core::fmt::Display for UnknownSingletonMembershipStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown singleton membership stub kind: {}", self.0)
            }
        }
        impl ClosedSet for SingletonMembershipStubKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "singleton membership stub kind";
            type Unknown = UnknownSingletonMembershipStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Only => "only",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownSingletonMembershipStubKind(s.to_owned())
            }
        }
        assert!(<SingletonMembershipStubKind as ClosedSet>::is_first(
            SingletonMembershipStubKind::Only,
        ));
        assert!(<SingletonMembershipStubKind as ClosedSet>::is_last(
            SingletonMembershipStubKind::Only,
        ));
    }

    #[test]
    fn is_first_and_is_last_agree_with_first_last_endpoint_anchors() {
        // The endpoint-anchor / endpoint-membership alignment —
        // `T::first().is_first()` MUST be `true` AND
        // `T::last().is_last()` MUST be `true` on every implementor.
        // Complementary agreement — every non-endpoint variant MUST
        // answer `false` to both predicates. Pinning the fixpoints
        // here catches a regression that drifts either surface from
        // the shared `index_of == 0` / `index_of + 1 == CARDINALITY`
        // projection.
        assert!(<StubKind as ClosedSet>::first().is_first());
        assert!(<StubKind as ClosedSet>::last().is_last());
        // Interior slot answers `false` to BOTH predicates on the
        // 3-variant stub.
        assert!(!<StubKind as ClosedSet>::is_first(StubKind::Beta));
        assert!(!<StubKind as ClosedSet>::is_last(StubKind::Beta));
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_is_first_and_index_of_zero() {
        // The well-formedness sweep's (30) clause — `v.is_first()` MUST
        // equal `v.index_of() == 0`. A hand-impl'd implementor whose
        // override drifts the head-membership predicate (returns
        // `true` on an interior slot, returns `false` on the head, a
        // stale override that returns the wrong answer after a
        // variant-listing edit) fails the sweep loudly rather than
        // silently bifurcating the head-membership projection surface
        // every downstream bounded-loop guard / saga-step engine /
        // truth-table property test consumer routes through. Sibling
        // posture to `assert_closed_set_well_formed_catches_drift_between_is_last_and_index_of_plus_one_equals_cardinality`
        // one direction over on the (head, tail) partition of clause
        // (30).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedIsFirstKind {
            Head,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedIsFirstKind(pub String);
        impl core::fmt::Display for UnknownDriftedIsFirstKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted is-first kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedIsFirstKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Tail];
            const SET_LABEL: &'static str = "drifted is-first kind";
            type Unknown = UnknownDriftedIsFirstKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedIsFirstKind(s.to_owned())
            }
            fn is_first(self) -> bool {
                // Drifted override — returns `true` on the tail slot
                // rather than the head, swapping the (head, tail)
                // endpoint-membership partition on the head arm.
                matches!(self, Self::Tail)
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedIsFirstKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted an is_first() override drifted from the natural index_of == 0 composition",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_is_last_and_index_of_plus_one_equals_cardinality(
    ) {
        // The well-formedness sweep's (30) clause — `v.is_last()` MUST
        // equal `v.index_of() + 1 == T::CARDINALITY`. Symmetric to the
        // `_catches_drift_between_is_first_and_index_of_zero` sibling
        // one endpoint over on the (head, tail) partition — this pin
        // covers the tail arm.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedIsLastKind {
            Head,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedIsLastKind(pub String);
        impl core::fmt::Display for UnknownDriftedIsLastKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted is-last kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedIsLastKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Tail];
            const SET_LABEL: &'static str = "drifted is-last kind";
            type Unknown = UnknownDriftedIsLastKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedIsLastKind(s.to_owned())
            }
            fn is_last(self) -> bool {
                // Drifted override — returns `true` on the head slot
                // rather than the tail, swapping the (head, tail)
                // endpoint-membership partition on the tail arm.
                matches!(self, Self::Head)
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedIsLastKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted an is_last() override drifted from the natural index_of + 1 == T::CARDINALITY composition",
        );
    }

    #[test]
    fn is_sorted_first_returns_true_only_on_the_lexicographic_order_head_variant() {
        // The lex-order head-endpoint membership predicate fires
        // exactly on the lex-min anchor and nowhere else. `StubKind`'s
        // labels are ("alpha", "beta", "gamma") — the lex-min is
        // `Alpha`, so `Alpha.is_sorted_first()` is `true` and both
        // `Beta.is_sorted_first()` and `Gamma.is_sorted_first()` are
        // `false`. On this stub the declaration order coincides with
        // the lex order, so `is_first` and `is_sorted_first` agree —
        // the deliberate probe that distinguishes the two ordering
        // axes lives at
        // `is_sorted_first_and_is_sorted_last_bracket_lex_endpoints_on_arbitrary_declaration_order`.
        // Sibling posture to
        // `is_first_returns_true_only_on_the_declaration_order_head_variant`
        // one ordering axis over.
        assert!(<StubKind as ClosedSet>::is_sorted_first(StubKind::Alpha));
        assert!(!<StubKind as ClosedSet>::is_sorted_first(StubKind::Beta));
        assert!(!<StubKind as ClosedSet>::is_sorted_first(StubKind::Gamma));
    }

    #[test]
    fn is_sorted_last_returns_true_only_on_the_lexicographic_order_tail_variant() {
        // The lex-order tail-endpoint membership predicate fires
        // exactly on the lex-max anchor and nowhere else — on
        // `StubKind` that is `Gamma`. Sibling posture to
        // `is_sorted_first_returns_true_only_on_the_lexicographic_order_head_variant`
        // one direction over on the (head, tail) partition.
        assert!(!<StubKind as ClosedSet>::is_sorted_last(StubKind::Alpha));
        assert!(!<StubKind as ClosedSet>::is_sorted_last(StubKind::Beta));
        assert!(<StubKind as ClosedSet>::is_sorted_last(StubKind::Gamma));
    }

    #[test]
    fn is_sorted_first_and_is_sorted_last_bracket_lex_endpoints_on_arbitrary_declaration_order() {
        // The lex-endpoint-membership contract on a declaration order
        // that neither matches nor cleanly reverses the lex order —
        // `T::sorted_first().is_sorted_first()` MUST be `true`
        // regardless of where in `T::ALL` the lex-min variant sits, AND
        // `T::sorted_last().is_sorted_last()` MUST be `true` regardless
        // of where in `T::ALL` the lex-max variant sits. A regression
        // that hard-coded the predicate against a declaration slot (a
        // stray `matches!(self, Self::ALL[0])` shape) rather than
        // routing through `sorted_index_of` would pass
        // `is_sorted_first_returns_true_only_on_the_lexicographic_order_head_variant`
        // on `StubKind` (because its declaration order coincides with
        // its lex order) and silently bifurcate the predicates on any
        // implementor whose declaration and lex orders disagree. The
        // stub's declaration is `[Gamma, Beta, Alpha]` — declaration
        // head `Gamma`, declaration tail `Alpha`, lex head `Alpha`,
        // lex tail `Gamma` — putting the four axis crossings at four
        // distinct variant slots so a lex-vs-declaration confusion in
        // either predicate fails at least one arm.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum LexEndpointMembershipStubKind {
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownLexEndpointMembershipStubKind(pub String);
        impl core::fmt::Display for UnknownLexEndpointMembershipStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown lex endpoint membership stub kind: {}", self.0)
            }
        }
        impl ClosedSet for LexEndpointMembershipStubKind {
            const ALL: &'static [Self] = &[Self::Gamma, Self::Beta, Self::Alpha];
            const SET_LABEL: &'static str = "lex endpoint membership stub kind";
            type Unknown = UnknownLexEndpointMembershipStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownLexEndpointMembershipStubKind(s.to_owned())
            }
        }
        // Lex head `Alpha` fires `is_sorted_first`; declaration head
        // `Gamma` does NOT.
        assert!(
            <LexEndpointMembershipStubKind as ClosedSet>::is_sorted_first(
                LexEndpointMembershipStubKind::Alpha,
            )
        );
        assert!(
            !<LexEndpointMembershipStubKind as ClosedSet>::is_sorted_first(
                LexEndpointMembershipStubKind::Beta,
            )
        );
        assert!(
            !<LexEndpointMembershipStubKind as ClosedSet>::is_sorted_first(
                LexEndpointMembershipStubKind::Gamma,
            )
        );
        // Lex tail `Gamma` fires `is_sorted_last`; declaration tail
        // `Alpha` does NOT.
        assert!(
            <LexEndpointMembershipStubKind as ClosedSet>::is_sorted_last(
                LexEndpointMembershipStubKind::Gamma,
            )
        );
        assert!(
            !<LexEndpointMembershipStubKind as ClosedSet>::is_sorted_last(
                LexEndpointMembershipStubKind::Beta,
            )
        );
        assert!(
            !<LexEndpointMembershipStubKind as ClosedSet>::is_sorted_last(
                LexEndpointMembershipStubKind::Alpha,
            )
        );
    }

    #[test]
    fn is_sorted_first_and_is_sorted_last_collapse_true_on_singleton_closed_set() {
        // The lex-endpoint-membership degenerate case — a singleton
        // closed set has ONE variant with `sorted_index_of == 0` AND
        // `sorted_index_of + 1 == 1 == CARDINALITY`, so both lex-
        // membership predicates fire on the same variant. Mirrors
        // `is_first_and_is_last_collapse_true_on_singleton_closed_set`
        // one ordering axis over.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum SingletonLexMembershipStubKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownSingletonLexMembershipStubKind(pub String);
        impl core::fmt::Display for UnknownSingletonLexMembershipStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown singleton lex membership stub kind: {}", self.0)
            }
        }
        impl ClosedSet for SingletonLexMembershipStubKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "singleton lex membership stub kind";
            type Unknown = UnknownSingletonLexMembershipStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Only => "only",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownSingletonLexMembershipStubKind(s.to_owned())
            }
        }
        assert!(
            <SingletonLexMembershipStubKind as ClosedSet>::is_sorted_first(
                SingletonLexMembershipStubKind::Only,
            )
        );
        assert!(
            <SingletonLexMembershipStubKind as ClosedSet>::is_sorted_last(
                SingletonLexMembershipStubKind::Only,
            )
        );
        // The singleton also satisfies the well-formedness clause (31)
        // sweep — `sorted_index_of == 0` AND `sorted_index_of + 1 ==
        // 1 == CARDINALITY` for the sole variant so BOTH predicates
        // fire, and the endpoint-anchor fixpoints
        // `T::sorted_first().is_sorted_first() == true` +
        // `T::sorted_last().is_sorted_last() == true` hold with the
        // singleton anchor.
        super::assert_closed_set_well_formed::<SingletonLexMembershipStubKind>();
    }

    #[test]
    fn is_sorted_first_and_is_sorted_last_agree_with_sorted_first_last_endpoint_anchors() {
        // The lex-endpoint-anchor / lex-endpoint-membership alignment —
        // `T::sorted_first().is_sorted_first()` MUST be `true` AND
        // `T::sorted_last().is_sorted_last()` MUST be `true` on every
        // implementor. Complementary agreement — every non-endpoint
        // variant MUST answer `false` to both predicates. Pinning the
        // fixpoints here catches a regression that drifts either
        // surface from the shared `sorted_index_of == 0` /
        // `sorted_index_of + 1 == CARDINALITY` projection. Sibling
        // posture to `is_first_and_is_last_agree_with_first_last_endpoint_anchors`
        // one ordering axis over.
        assert!(<StubKind as ClosedSet>::sorted_first().is_sorted_first());
        assert!(<StubKind as ClosedSet>::sorted_last().is_sorted_last());
        // Interior slot answers `false` to BOTH predicates on the
        // 3-variant stub.
        assert!(!<StubKind as ClosedSet>::is_sorted_first(StubKind::Beta));
        assert!(!<StubKind as ClosedSet>::is_sorted_last(StubKind::Beta));
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_is_sorted_first_and_sorted_index_of_zero(
    ) {
        // The well-formedness sweep's (31) clause —
        // `v.is_sorted_first()` MUST equal `v.sorted_index_of() == 0`.
        // A hand-impl'd implementor whose override drifts the lex-
        // head-membership predicate (returns `true` on an interior
        // lex slot, returns `false` on the lex head, a stale override
        // that returns the wrong answer after a label edit shifts the
        // lex slot alignment) fails the sweep loudly rather than
        // silently bifurcating the lex-head-membership projection
        // surface every downstream alphabetized-LSP-cursor / lex-
        // anchored-diagnostic-renderer / alphabetized-default-
        // deserializer consumer routes through. Sibling posture to
        // `assert_closed_set_well_formed_catches_drift_between_is_first_and_index_of_zero`
        // one ordering axis over on the (declaration, lex) partition
        // of the endpoint-membership cube. The stub's variants are
        // ordered `[Head, Tail]` in declaration order with labels
        // `("aaa", "bbb")` — declaration head `Head` coincides with
        // lex head `Head`, so a swapped `matches!(self, Self::Tail)`
        // override is BOTH declaration- and lex-drifted; the pin
        // fires on the lex-drift arm through clause (31).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedIsSortedFirstKind {
            Head,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedIsSortedFirstKind(pub String);
        impl core::fmt::Display for UnknownDriftedIsSortedFirstKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted is-sorted-first kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedIsSortedFirstKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Tail];
            const SET_LABEL: &'static str = "drifted is-sorted-first kind";
            type Unknown = UnknownDriftedIsSortedFirstKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "aaa",
                    Self::Tail => "bbb",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedIsSortedFirstKind(s.to_owned())
            }
            fn is_sorted_first(self) -> bool {
                // Drifted override — returns `true` on the lex tail
                // slot rather than the lex head, swapping the (lex
                // head, lex tail) endpoint-membership partition on
                // the head arm.
                matches!(self, Self::Tail)
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedIsSortedFirstKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted an is_sorted_first() override drifted from the natural sorted_index_of == 0 composition",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_is_sorted_last_and_sorted_index_of_plus_one_equals_cardinality(
    ) {
        // The well-formedness sweep's (31) clause —
        // `v.is_sorted_last()` MUST equal
        // `v.sorted_index_of() + 1 == T::CARDINALITY`. Symmetric to
        // `_catches_drift_between_is_sorted_first_and_sorted_index_of_zero`
        // one endpoint over on the (head, tail) partition of clause
        // (31) — this pin covers the lex tail arm.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedIsSortedLastKind {
            Head,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedIsSortedLastKind(pub String);
        impl core::fmt::Display for UnknownDriftedIsSortedLastKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted is-sorted-last kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedIsSortedLastKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Tail];
            const SET_LABEL: &'static str = "drifted is-sorted-last kind";
            type Unknown = UnknownDriftedIsSortedLastKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "aaa",
                    Self::Tail => "bbb",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedIsSortedLastKind(s.to_owned())
            }
            fn is_sorted_last(self) -> bool {
                // Drifted override — returns `true` on the lex head
                // slot rather than the lex tail, swapping the (lex
                // head, lex tail) endpoint-membership partition on
                // the tail arm.
                matches!(self, Self::Head)
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedIsSortedLastKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted an is_sorted_last() override drifted from the natural sorted_index_of + 1 == T::CARDINALITY composition",
        );
    }

    #[test]
    fn is_sorted_first_and_is_sorted_last_diverge_from_is_first_and_is_last_on_reverse_stub() {
        // The (declaration, lex) × (head, tail) 2×2 endpoint-
        // membership matrix has FOUR corners; a stub whose
        // declaration order is the exact reverse of its lex order
        // puts each corner on a distinct variant slot, distinguishing
        // the lex-axis pair (`is_sorted_first`/`is_sorted_last`) from
        // the declaration-axis pair (`is_first`/`is_last`) at every
        // slot rather than silently collapsing on stubs where the two
        // orderings coincide. Labels `("gamma", "beta", "alpha")` on
        // `[Gamma, Beta, Alpha]` declaration order — declaration head
        // `Gamma` is lex TAIL, declaration tail `Alpha` is lex HEAD.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum ReverseLexMembershipStubKind {
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownReverseLexMembershipStubKind(pub String);
        impl core::fmt::Display for UnknownReverseLexMembershipStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown reverse lex membership stub kind: {}", self.0)
            }
        }
        impl ClosedSet for ReverseLexMembershipStubKind {
            const ALL: &'static [Self] = &[Self::Gamma, Self::Beta, Self::Alpha];
            const SET_LABEL: &'static str = "reverse lex membership stub kind";
            type Unknown = UnknownReverseLexMembershipStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownReverseLexMembershipStubKind(s.to_owned())
            }
        }
        // Declaration head `Gamma` — `is_first` fires, `is_sorted_first`
        // does NOT (lex head is `Alpha`).
        assert!(<ReverseLexMembershipStubKind as ClosedSet>::is_first(
            ReverseLexMembershipStubKind::Gamma,
        ));
        assert!(
            !<ReverseLexMembershipStubKind as ClosedSet>::is_sorted_first(
                ReverseLexMembershipStubKind::Gamma,
            )
        );
        // Declaration head `Gamma` — `is_sorted_last` fires (lex tail
        // is `Gamma`), `is_last` does NOT.
        assert!(<ReverseLexMembershipStubKind as ClosedSet>::is_sorted_last(
            ReverseLexMembershipStubKind::Gamma,
        ));
        assert!(!<ReverseLexMembershipStubKind as ClosedSet>::is_last(
            ReverseLexMembershipStubKind::Gamma,
        ));
        // Declaration tail `Alpha` — `is_last` fires, `is_sorted_last`
        // does NOT (lex tail is `Gamma`).
        assert!(<ReverseLexMembershipStubKind as ClosedSet>::is_last(
            ReverseLexMembershipStubKind::Alpha,
        ));
        assert!(
            !<ReverseLexMembershipStubKind as ClosedSet>::is_sorted_last(
                ReverseLexMembershipStubKind::Alpha,
            )
        );
        // Declaration tail `Alpha` — `is_sorted_first` fires (lex
        // head is `Alpha`), `is_first` does NOT.
        assert!(
            <ReverseLexMembershipStubKind as ClosedSet>::is_sorted_first(
                ReverseLexMembershipStubKind::Alpha,
            )
        );
        assert!(!<ReverseLexMembershipStubKind as ClosedSet>::is_first(
            ReverseLexMembershipStubKind::Alpha,
        ));
        // The reverse stub also satisfies the well-formedness clause
        // (31) sweep — every variant's `is_sorted_first` /
        // `is_sorted_last` agrees with the natural
        // `sorted_index_of == 0` / `sorted_index_of + 1 == CARDINALITY`
        // composition through the default trait bodies, and the
        // endpoint-anchor fixpoints
        // `T::sorted_first().is_sorted_first() == true` +
        // `T::sorted_last().is_sorted_last() == true` hold on the
        // (Alpha, Gamma) lex-endpoint anchors even though the
        // declaration order is reversed.
        super::assert_closed_set_well_formed::<ReverseLexMembershipStubKind>();
    }

    #[test]
    fn is_endpoint_returns_true_only_on_declaration_order_endpoints() {
        // The declaration-order boundary-membership predicate fires on
        // both endpoint anchors and nowhere on the strict interior.
        // `StubKind`'s variant listing is `[Alpha, Beta, Gamma]`, so
        // `Alpha.is_endpoint()` and `Gamma.is_endpoint()` are `true`
        // (declaration head + tail) and `Beta.is_endpoint()` is
        // `false` (strict interior). Sibling posture to
        // `is_first_returns_true_only_on_the_declaration_order_head_variant`
        // + `is_last_returns_true_only_on_the_declaration_order_tail_variant`
        // one predicate-flavor axis over on the (point, boundary)
        // partition — the boundary-membership arm fires on the UNION
        // of the two point-membership arms' fixpoint slots.
        assert!(<StubKind as ClosedSet>::is_endpoint(StubKind::Alpha));
        assert!(!<StubKind as ClosedSet>::is_endpoint(StubKind::Beta));
        assert!(<StubKind as ClosedSet>::is_endpoint(StubKind::Gamma));
    }

    #[test]
    fn is_interior_returns_true_only_on_declaration_order_interior_variants() {
        // The declaration-order interior-membership predicate fires
        // exclusively on the strict interior — the complement of
        // `is_endpoint` under the (endpoint, interior) partition. On
        // `StubKind` the interior is `{Beta}` — the two endpoints
        // `Alpha` (head) and `Gamma` (tail) answer `false`, the sole
        // strictly-interior variant `Beta` answers `true`. Sibling
        // posture to
        // `is_endpoint_returns_true_only_on_declaration_order_endpoints`
        // one arm over on the (endpoint, interior) partition — the two
        // arms partition `T::ALL` exhaustively AND disjointly.
        assert!(!<StubKind as ClosedSet>::is_interior(StubKind::Alpha));
        assert!(<StubKind as ClosedSet>::is_interior(StubKind::Beta));
        assert!(!<StubKind as ClosedSet>::is_interior(StubKind::Gamma));
    }

    #[test]
    fn is_endpoint_and_is_interior_partition_all_slice_on_arbitrary_declaration_order() {
        // The boundary-partition contract on an arbitrary declaration
        // order — regardless of which variant literal happens to sit
        // at slice-index-0 / slice-index-(N - 1), `T::first()` +
        // `T::last()` fire `is_endpoint` and every other slot fires
        // `is_interior`. A regression that hard-coded the predicate
        // against a variant literal rather than routing through the
        // point-membership disjunction would pass
        // `is_endpoint_returns_true_only_on_declaration_order_endpoints`
        // / `is_interior_returns_true_only_on_declaration_order_interior_variants`
        // on `StubKind` and silently bifurcate the predicates on any
        // implementor whose `ALL`-array layout differs. Deliberate
        // 5-variant stub with a strict interior wider than one slot so
        // the (endpoint, interior) partition is directly observable —
        // 2 endpoint slots (slice-index-0 + slice-index-4) + 3
        // strictly-interior slots (slice-index-{1,2,3}). Sibling
        // posture to
        // `is_first_and_is_last_bracket_all_slice_on_arbitrary_declaration_order`
        // one predicate-flavor axis over on the (point, boundary)
        // partition.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum EndpointPartitionStubKind {
            Epsilon,
            Delta,
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownEndpointPartitionStubKind(pub String);
        impl core::fmt::Display for UnknownEndpointPartitionStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown endpoint partition stub kind: {}", self.0)
            }
        }
        impl ClosedSet for EndpointPartitionStubKind {
            const ALL: &'static [Self] = &[
                Self::Epsilon,
                Self::Delta,
                Self::Gamma,
                Self::Beta,
                Self::Alpha,
            ];
            const SET_LABEL: &'static str = "endpoint partition stub kind";
            type Unknown = UnknownEndpointPartitionStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Epsilon => "epsilon",
                    Self::Delta => "delta",
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownEndpointPartitionStubKind(s.to_owned())
            }
        }
        // The declaration head `Epsilon` (slice-index-0) — fires
        // `is_endpoint`, does NOT fire `is_interior`.
        assert!(<EndpointPartitionStubKind as ClosedSet>::is_endpoint(
            EndpointPartitionStubKind::Epsilon,
        ));
        assert!(!<EndpointPartitionStubKind as ClosedSet>::is_interior(
            EndpointPartitionStubKind::Epsilon,
        ));
        // The declaration tail `Alpha` (slice-index-4) — fires
        // `is_endpoint`, does NOT fire `is_interior`.
        assert!(<EndpointPartitionStubKind as ClosedSet>::is_endpoint(
            EndpointPartitionStubKind::Alpha,
        ));
        assert!(!<EndpointPartitionStubKind as ClosedSet>::is_interior(
            EndpointPartitionStubKind::Alpha,
        ));
        // The three strictly-interior slots (`Delta`, `Gamma`,
        // `Beta`) — do NOT fire `is_endpoint`, DO fire `is_interior`.
        for interior in [
            EndpointPartitionStubKind::Delta,
            EndpointPartitionStubKind::Gamma,
            EndpointPartitionStubKind::Beta,
        ] {
            assert!(
                !<EndpointPartitionStubKind as ClosedSet>::is_endpoint(interior),
                "{interior:?}.is_endpoint() returned true on a strictly-interior slot",
            );
            assert!(
                <EndpointPartitionStubKind as ClosedSet>::is_interior(interior),
                "{interior:?}.is_interior() returned false on a strictly-interior slot",
            );
        }
        // Sweep the exhaustive complementarity contract across every
        // variant — `is_endpoint(v) != is_interior(v)` on every slot.
        for &v in <EndpointPartitionStubKind as ClosedSet>::ALL {
            assert_ne!(
                <EndpointPartitionStubKind as ClosedSet>::is_endpoint(v),
                <EndpointPartitionStubKind as ClosedSet>::is_interior(v),
                "{v:?}.is_endpoint() and {v:?}.is_interior() returned the same bool — the (endpoint, interior) partition failed exhaustive complementarity",
            );
        }
    }

    #[test]
    fn is_endpoint_and_is_interior_collapse_on_singleton_closed_set() {
        // The boundary-partition degenerate case — a singleton closed
        // set has ONE variant that is BOTH `T::first()` and
        // `T::last()`, so `is_endpoint` fires (its default body is
        // `is_first || is_last`, `true || true == true`) and
        // `is_interior` does NOT fire (its default body is
        // `!is_endpoint`, `!true == false`). A singleton closed set
        // has ZERO strictly-interior slots by construction — the
        // (endpoint, interior) partition collapses onto the sole
        // variant as an endpoint. Mirrors
        // `is_first_and_is_last_collapse_true_on_singleton_closed_set`
        // one predicate-flavor axis over on the (point, boundary)
        // partition — the two point-membership arms collapse-fire, the
        // boundary arm collapse-fires, the interior arm collapse-does-
        // NOT-fire, and the exhaustive complementarity contract holds
        // trivially at the boundary-cardinality edge.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum SingletonEndpointStubKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownSingletonEndpointStubKind(pub String);
        impl core::fmt::Display for UnknownSingletonEndpointStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown singleton endpoint stub kind: {}", self.0)
            }
        }
        impl ClosedSet for SingletonEndpointStubKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "singleton endpoint stub kind";
            type Unknown = UnknownSingletonEndpointStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Only => "only",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownSingletonEndpointStubKind(s.to_owned())
            }
        }
        assert!(<SingletonEndpointStubKind as ClosedSet>::is_endpoint(
            SingletonEndpointStubKind::Only,
        ));
        assert!(!<SingletonEndpointStubKind as ClosedSet>::is_interior(
            SingletonEndpointStubKind::Only,
        ));
        // Exhaustive complementarity holds at the singleton edge —
        // even on a 1-variant set the (endpoint, interior) partition
        // preserves its XOR contract.
        assert_ne!(
            <SingletonEndpointStubKind as ClosedSet>::is_endpoint(SingletonEndpointStubKind::Only,),
            <SingletonEndpointStubKind as ClosedSet>::is_interior(SingletonEndpointStubKind::Only,),
        );
        // The singleton stub also satisfies the well-formedness
        // clause (32) sweep — the endpoint-anchor fixpoint contract
        // `T::first().is_endpoint() == true` +
        // `T::last().is_endpoint() == true` +
        // `T::first().is_interior() == false` +
        // `T::last().is_interior() == false` degenerates to a single
        // variant answering `true` to `is_endpoint` and `false` to
        // `is_interior`, and the exhaustive complementarity holds on
        // that sole variant.
        super::assert_closed_set_well_formed::<SingletonEndpointStubKind>();
    }

    #[test]
    fn is_endpoint_and_is_interior_agree_with_first_last_endpoint_anchors() {
        // The endpoint-anchor / boundary-membership alignment —
        // `T::first().is_endpoint()` MUST be `true` AND
        // `T::last().is_endpoint()` MUST be `true` AND
        // `T::first().is_interior()` MUST be `false` AND
        // `T::last().is_interior()` MUST be `false` on every
        // implementor. Pinning the fixpoints here catches a
        // regression that drifts either surface from the shared
        // point-membership disjunction on either endpoint. Sibling
        // posture to
        // `is_first_and_is_last_agree_with_first_last_endpoint_anchors`
        // one predicate-flavor axis over on the (point, boundary)
        // partition.
        assert!(<StubKind as ClosedSet>::first().is_endpoint());
        assert!(<StubKind as ClosedSet>::last().is_endpoint());
        assert!(!<StubKind as ClosedSet>::first().is_interior());
        assert!(!<StubKind as ClosedSet>::last().is_interior());
        // Interior slot answers `false` to `is_endpoint` and `true` to
        // `is_interior` on the 3-variant stub.
        assert!(!<StubKind as ClosedSet>::is_endpoint(StubKind::Beta));
        assert!(<StubKind as ClosedSet>::is_interior(StubKind::Beta));
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_is_endpoint_and_is_first_or_is_last() {
        // The well-formedness sweep's (32) clause — `v.is_endpoint()`
        // MUST equal `v.is_first() || v.is_last()`. A hand-impl'd
        // implementor whose override drifts the boundary-membership
        // predicate (returns `true` on an interior slot, returns
        // `false` on the head, a stale override that returns the
        // wrong answer after a variant-listing edit) fails the sweep
        // loudly rather than silently bifurcating the boundary-
        // membership projection surface every downstream shared-
        // endpoint-badge renderer / boundary-audit-event emitter /
        // bounded-iteration-guard consumer routes through. Sibling
        // posture to
        // `assert_closed_set_well_formed_catches_drift_between_is_first_and_index_of_zero`
        // one predicate-flavor axis over on the (point, boundary)
        // partition. The stub's `Middle` slot is a strict-interior
        // variant — a permissive override that returns `true` on
        // `Middle` breaks clause (32)'s per-variant equality pin.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedIsEndpointKind {
            Head,
            Middle,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedIsEndpointKind(pub String);
        impl core::fmt::Display for UnknownDriftedIsEndpointKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted is-endpoint kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedIsEndpointKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Middle, Self::Tail];
            const SET_LABEL: &'static str = "drifted is-endpoint kind";
            type Unknown = UnknownDriftedIsEndpointKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Middle => "middle",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedIsEndpointKind(s.to_owned())
            }
            fn is_endpoint(self) -> bool {
                // Drifted override — returns `true` on every slot,
                // folding the strict-interior partition onto the
                // boundary partition and silently breaking clause
                // (32)'s per-variant equality pin on the `Middle`
                // interior slot.
                let _ = self;
                true
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedIsEndpointKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted an is_endpoint() override drifted from the natural is_first || is_last composition",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_is_interior_and_not_is_endpoint() {
        // The well-formedness sweep's (32) clause — `v.is_interior()`
        // MUST equal `!(v.is_first() || v.is_last())`. Symmetric to
        // `_catches_drift_between_is_endpoint_and_is_first_or_is_last`
        // one arm over on the (endpoint, interior) partition — this
        // pin covers the interior arm. A permissive override that
        // returns `true` on every slot folds the boundary partition
        // onto the interior partition on the endpoint slots and
        // silently breaks clause (32)'s per-variant equality pin AND
        // the exhaustive-complementarity pin AND the interior-anchor
        // anti-fixpoint pins.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedIsInteriorKind {
            Head,
            Middle,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedIsInteriorKind(pub String);
        impl core::fmt::Display for UnknownDriftedIsInteriorKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted is-interior kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedIsInteriorKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Middle, Self::Tail];
            const SET_LABEL: &'static str = "drifted is-interior kind";
            type Unknown = UnknownDriftedIsInteriorKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Middle => "middle",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedIsInteriorKind(s.to_owned())
            }
            fn is_interior(self) -> bool {
                // Drifted override — returns `true` on every slot,
                // folding the boundary partition onto the interior
                // partition on the endpoint slots and silently
                // breaking clause (32)'s per-variant equality pin.
                let _ = self;
                true
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedIsInteriorKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted an is_interior() override drifted from the natural !is_endpoint composition",
        );
    }

    #[test]
    fn is_sorted_endpoint_returns_true_only_on_lex_order_endpoints() {
        // The lex-order boundary-membership predicate fires on both
        // lex-endpoint anchors and nowhere on the strict lex-interior.
        // `StubKind`'s canonical labels are `("alpha", "beta",
        // "gamma")` — the declaration ordering matches the lex
        // ordering here, so the lex-head endpoint is `Alpha` and the
        // lex-tail endpoint is `Gamma`; `Alpha.is_sorted_endpoint()`
        // and `Gamma.is_sorted_endpoint()` are `true` (lex head + lex
        // tail) and `Beta.is_sorted_endpoint()` is `false` (strict lex
        // interior). Sibling posture to
        // `is_sorted_first_returns_true_only_on_the_lex_order_head_variant`
        // + `is_sorted_last_returns_true_only_on_the_lex_order_tail_variant`
        // one predicate-flavor axis over on the (lex-point, lex-
        // boundary) partition — the lex-boundary-membership arm fires
        // on the UNION of the two lex-point-membership arms' fixpoint
        // slots.
        assert!(<StubKind as ClosedSet>::is_sorted_endpoint(StubKind::Alpha));
        assert!(!<StubKind as ClosedSet>::is_sorted_endpoint(StubKind::Beta));
        assert!(<StubKind as ClosedSet>::is_sorted_endpoint(StubKind::Gamma));
    }

    #[test]
    fn is_sorted_interior_returns_true_only_on_lex_order_interior_variants() {
        // The lex-order interior-membership predicate fires exclusively
        // on the strict lex-interior — the complement of
        // `is_sorted_endpoint` under the (lex-endpoint, lex-interior)
        // partition. On `StubKind` the lex-interior is `{Beta}` — the
        // two lex-endpoints `Alpha` (lex head) and `Gamma` (lex tail)
        // answer `false`, the sole strictly-lex-interior variant `Beta`
        // answers `true`. Sibling posture to
        // `is_sorted_endpoint_returns_true_only_on_lex_order_endpoints`
        // one arm over on the (lex-endpoint, lex-interior) partition —
        // the two arms partition `T::ALL` exhaustively AND disjointly
        // under the lex ordering.
        assert!(!<StubKind as ClosedSet>::is_sorted_interior(
            StubKind::Alpha
        ));
        assert!(<StubKind as ClosedSet>::is_sorted_interior(StubKind::Beta));
        assert!(!<StubKind as ClosedSet>::is_sorted_interior(
            StubKind::Gamma
        ));
    }

    #[test]
    fn is_sorted_endpoint_and_is_sorted_interior_partition_all_slice_on_arbitrary_declaration_and_lex_order(
    ) {
        // The lex-boundary-partition contract on a stub whose
        // declaration order deliberately diverges from the lex order —
        // regardless of which variant literal happens to sit at
        // declaration-slice-index-0 / declaration-slice-index-(N - 1),
        // `T::sorted_first()` + `T::sorted_first()` fire
        // `is_sorted_endpoint` (the LEX head + LEX tail) and every
        // other slot fires `is_sorted_interior`. A regression that
        // hard-coded the predicate against a declaration-slice index
        // rather than routing through the LEX-point-membership
        // disjunction would pass
        // `is_sorted_endpoint_returns_true_only_on_lex_order_endpoints`
        // / `is_sorted_interior_returns_true_only_on_lex_order_interior_variants`
        // on `StubKind` (where declaration ordering matches lex
        // ordering) and silently bifurcate the predicates on any
        // implementor whose declaration order diverges from its lex
        // order. Deliberate 5-variant stub with declaration order
        // `[Epsilon, Delta, Gamma, Beta, Alpha]` and labels
        // `("epsilon", "delta", "gamma", "beta", "alpha")`. The lex
        // ordering of the labels is `"alpha" < "beta" < "delta" <
        // "epsilon" < "gamma"`, so the lex-endpoints are `Alpha`
        // (lex head) + `Gamma` (lex tail) while the declaration-
        // endpoints are `Epsilon` (declaration head) + `Alpha`
        // (declaration tail). The (declaration-endpoint, lex-
        // endpoint) crossings put:
        //   * `Alpha` — declaration TAIL AND lex HEAD, so fires BOTH
        //     `is_endpoint` (declaration) and `is_sorted_endpoint`
        //     (lex).
        //   * `Gamma` — declaration INTERIOR AND lex TAIL, so fires
        //     `is_sorted_endpoint` (lex) but NOT `is_endpoint`
        //     (declaration) — the axis-divergence witness that a
        //     declaration-axis regression cannot silently satisfy.
        //   * `Epsilon` — declaration HEAD AND lex INTERIOR, so
        //     fires `is_endpoint` (declaration) but NOT
        //     `is_sorted_endpoint` (lex) — the complementary axis-
        //     divergence witness one direction over.
        //   * `Beta` + `Delta` — strict-interior on BOTH axes.
        // Every (declaration × lex) × (endpoint, interior) axis-
        // crossing has at least one witness slot. A permissive
        // declaration-axis override that folded the LEX partition
        // onto the DECLARATION partition would pass `Alpha` (both
        // axes fire) but fail the `Gamma` / `Epsilon` axis-
        // divergence witnesses loudly. Sibling posture to
        // `is_endpoint_and_is_interior_partition_all_slice_on_arbitrary_declaration_order`
        // one ordering axis over on the (declaration, lex) partition.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum SortedEndpointPartitionStubKind {
            Epsilon,
            Delta,
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownSortedEndpointPartitionStubKind(pub String);
        impl core::fmt::Display for UnknownSortedEndpointPartitionStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown sorted endpoint partition stub kind: {}", self.0)
            }
        }
        impl ClosedSet for SortedEndpointPartitionStubKind {
            const ALL: &'static [Self] = &[
                Self::Epsilon,
                Self::Delta,
                Self::Gamma,
                Self::Beta,
                Self::Alpha,
            ];
            const SET_LABEL: &'static str = "sorted endpoint partition stub kind";
            type Unknown = UnknownSortedEndpointPartitionStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Epsilon => "epsilon",
                    Self::Delta => "delta",
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownSortedEndpointPartitionStubKind(s.to_owned())
            }
        }
        // The lex head `Alpha` (label "alpha") — fires
        // `is_sorted_endpoint`, does NOT fire `is_sorted_interior`.
        assert!(
            <SortedEndpointPartitionStubKind as ClosedSet>::is_sorted_endpoint(
                SortedEndpointPartitionStubKind::Alpha,
            )
        );
        assert!(
            !<SortedEndpointPartitionStubKind as ClosedSet>::is_sorted_interior(
                SortedEndpointPartitionStubKind::Alpha,
            )
        );
        // The lex tail `Gamma` (label "gamma") — fires
        // `is_sorted_endpoint`, does NOT fire `is_sorted_interior`.
        assert!(
            <SortedEndpointPartitionStubKind as ClosedSet>::is_sorted_endpoint(
                SortedEndpointPartitionStubKind::Gamma,
            )
        );
        assert!(
            !<SortedEndpointPartitionStubKind as ClosedSet>::is_sorted_interior(
                SortedEndpointPartitionStubKind::Gamma,
            )
        );
        // The three strictly-lex-interior slots (`Beta`, `Delta`,
        // `Epsilon`) — do NOT fire `is_sorted_endpoint`, DO fire
        // `is_sorted_interior`. Note `Epsilon` is the DECLARATION
        // head but a strictly-LEX-interior slot — the axis-
        // divergence witness a declaration-axis regression would
        // fail to satisfy on this arm.
        for interior in [
            SortedEndpointPartitionStubKind::Beta,
            SortedEndpointPartitionStubKind::Delta,
            SortedEndpointPartitionStubKind::Epsilon,
        ] {
            assert!(
                !<SortedEndpointPartitionStubKind as ClosedSet>::is_sorted_endpoint(interior),
                "{interior:?}.is_sorted_endpoint() returned true on a strictly-lex-interior slot",
            );
            assert!(
                <SortedEndpointPartitionStubKind as ClosedSet>::is_sorted_interior(interior),
                "{interior:?}.is_sorted_interior() returned false on a strictly-lex-interior slot",
            );
        }
        // Sweep the exhaustive complementarity contract across every
        // variant — `is_sorted_endpoint(v) != is_sorted_interior(v)`
        // on every slot.
        for &v in <SortedEndpointPartitionStubKind as ClosedSet>::ALL {
            assert_ne!(
                <SortedEndpointPartitionStubKind as ClosedSet>::is_sorted_endpoint(v),
                <SortedEndpointPartitionStubKind as ClosedSet>::is_sorted_interior(v),
                "{v:?}.is_sorted_endpoint() and {v:?}.is_sorted_interior() returned the same bool — the (lex-endpoint, lex-interior) partition failed exhaustive complementarity",
            );
        }
        // The stub also satisfies the well-formedness sweep — clauses
        // (32) + (33) both fire on a declaration-order that diverges
        // from the lex order, pinning the (declaration-axis) endpoint
        // partition on the declaration endpoints (`Epsilon`, `Alpha`)
        // AND the (lex-axis) endpoint partition on the lex endpoints
        // (`Alpha`, `Epsilon`) — same variant pair, swapped anchor
        // roles. A regression that folded either axis onto the other
        // would fail this sweep loudly.
        super::assert_closed_set_well_formed::<SortedEndpointPartitionStubKind>();
    }

    #[test]
    fn is_sorted_endpoint_and_is_sorted_interior_collapse_on_singleton_closed_set() {
        // The lex-boundary-partition degenerate case — a singleton
        // closed set has ONE variant that is BOTH `T::sorted_first()`
        // and `T::sorted_last()`, so `is_sorted_endpoint` fires (its
        // default body is `is_sorted_first || is_sorted_last`, `true
        // || true == true`) and `is_sorted_interior` does NOT fire
        // (its default body is `!is_sorted_endpoint`, `!true ==
        // false`). A singleton closed set has ZERO strictly-lex-
        // interior slots by construction — the (lex-endpoint, lex-
        // interior) partition collapses onto the sole variant as a
        // lex-endpoint. Mirrors
        // `is_endpoint_and_is_interior_collapse_on_singleton_closed_set`
        // one ordering axis over on the (declaration, lex) partition
        // — the two lex-point-membership arms collapse-fire, the lex-
        // boundary arm collapse-fires, the lex-interior arm collapse-
        // does-NOT-fire, and the exhaustive complementarity contract
        // holds trivially at the boundary-cardinality edge.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum SingletonSortedEndpointStubKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownSingletonSortedEndpointStubKind(pub String);
        impl core::fmt::Display for UnknownSingletonSortedEndpointStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown singleton sorted endpoint stub kind: {}", self.0)
            }
        }
        impl ClosedSet for SingletonSortedEndpointStubKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "singleton sorted endpoint stub kind";
            type Unknown = UnknownSingletonSortedEndpointStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Only => "only",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownSingletonSortedEndpointStubKind(s.to_owned())
            }
        }
        assert!(
            <SingletonSortedEndpointStubKind as ClosedSet>::is_sorted_endpoint(
                SingletonSortedEndpointStubKind::Only,
            )
        );
        assert!(
            !<SingletonSortedEndpointStubKind as ClosedSet>::is_sorted_interior(
                SingletonSortedEndpointStubKind::Only,
            )
        );
        // Exhaustive complementarity holds at the singleton edge —
        // even on a 1-variant set the (lex-endpoint, lex-interior)
        // partition preserves its XOR contract.
        assert_ne!(
            <SingletonSortedEndpointStubKind as ClosedSet>::is_sorted_endpoint(
                SingletonSortedEndpointStubKind::Only,
            ),
            <SingletonSortedEndpointStubKind as ClosedSet>::is_sorted_interior(
                SingletonSortedEndpointStubKind::Only,
            ),
        );
        // The singleton stub also satisfies the well-formedness clause
        // (33) sweep — the lex-endpoint-anchor fixpoint contract
        // `T::sorted_first().is_sorted_endpoint() == true` +
        // `T::sorted_last().is_sorted_endpoint() == true` +
        // `T::sorted_first().is_sorted_interior() == false` +
        // `T::sorted_last().is_sorted_interior() == false` degenerates
        // to a single variant answering `true` to `is_sorted_endpoint`
        // and `false` to `is_sorted_interior`, and the exhaustive
        // complementarity holds on that sole variant.
        super::assert_closed_set_well_formed::<SingletonSortedEndpointStubKind>();
    }

    #[test]
    fn is_sorted_endpoint_and_is_sorted_interior_agree_with_sorted_first_last_endpoint_anchors() {
        // The lex-endpoint-anchor / lex-boundary-membership alignment —
        // `T::sorted_first().is_sorted_endpoint()` MUST be `true` AND
        // `T::sorted_last().is_sorted_endpoint()` MUST be `true` AND
        // `T::sorted_first().is_sorted_interior()` MUST be `false` AND
        // `T::sorted_last().is_sorted_interior()` MUST be `false` on
        // every implementor. Pinning the fixpoints here catches a
        // regression that drifts either surface from the shared lex-
        // point-membership disjunction on either lex endpoint. Sibling
        // posture to
        // `is_endpoint_and_is_interior_agree_with_first_last_endpoint_anchors`
        // one ordering axis over on the (declaration, lex) partition.
        assert!(<StubKind as ClosedSet>::sorted_first().is_sorted_endpoint());
        assert!(<StubKind as ClosedSet>::sorted_last().is_sorted_endpoint());
        assert!(!<StubKind as ClosedSet>::sorted_first().is_sorted_interior());
        assert!(!<StubKind as ClosedSet>::sorted_last().is_sorted_interior());
        // Strict-lex-interior slot answers `false` to
        // `is_sorted_endpoint` and `true` to `is_sorted_interior` on
        // the 3-variant stub (lex head = "alpha", lex tail = "gamma",
        // sole strict-lex-interior = Beta / "beta").
        assert!(!<StubKind as ClosedSet>::is_sorted_endpoint(StubKind::Beta));
        assert!(<StubKind as ClosedSet>::is_sorted_interior(StubKind::Beta));
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_is_sorted_endpoint_and_is_sorted_first_or_is_sorted_last(
    ) {
        // The well-formedness sweep's (33) clause —
        // `v.is_sorted_endpoint()` MUST equal `v.is_sorted_first() ||
        // v.is_sorted_last()`. A hand-impl'd implementor whose
        // override drifts the lex-boundary-membership predicate
        // (returns `true` on a strict-lex-interior slot, returns
        // `false` on the lex head, a stale override that returns the
        // wrong answer after a label edit shifts the lex slot
        // alignment) fails the sweep loudly rather than silently
        // bifurcating the lex-boundary-membership projection surface
        // every downstream shared-lex-endpoint-badge renderer /
        // lex-boundary-audit-event emitter / bounded-alphabetized-
        // iteration-guard consumer routes through. Sibling posture to
        // `assert_closed_set_well_formed_catches_drift_between_is_endpoint_and_is_first_or_is_last`
        // one ordering axis over on the (declaration, lex) partition.
        // The stub's `Middle` slot is a strict-lex-interior variant —
        // a permissive override that returns `true` on `Middle`
        // breaks clause (33)'s per-variant equality pin.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedIsSortedEndpointKind {
            Head,
            Middle,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedIsSortedEndpointKind(pub String);
        impl core::fmt::Display for UnknownDriftedIsSortedEndpointKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted is-sorted-endpoint kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedIsSortedEndpointKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Middle, Self::Tail];
            const SET_LABEL: &'static str = "drifted is-sorted-endpoint kind";
            type Unknown = UnknownDriftedIsSortedEndpointKind;
            fn label(self) -> &'static str {
                match self {
                    // Deliberate label choice: lex order matches
                    // declaration order here ("head" < "middle" <
                    // "tail"), so `Middle` is a strict-lex-interior
                    // slot and a permissive override on it breaks the
                    // per-variant equality pin.
                    Self::Head => "head",
                    Self::Middle => "middle",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedIsSortedEndpointKind(s.to_owned())
            }
            fn is_sorted_endpoint(self) -> bool {
                // Drifted override — returns `true` on every slot,
                // folding the strict-lex-interior partition onto the
                // lex-boundary partition and silently breaking clause
                // (33)'s per-variant equality pin on the `Middle`
                // lex-interior slot.
                let _ = self;
                true
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedIsSortedEndpointKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted an is_sorted_endpoint() override drifted from the natural is_sorted_first || is_sorted_last composition",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_is_sorted_interior_and_not_is_sorted_endpoint(
    ) {
        // The well-formedness sweep's (33) clause —
        // `v.is_sorted_interior()` MUST equal
        // `!(v.is_sorted_first() || v.is_sorted_last())`. Symmetric to
        // `_catches_drift_between_is_sorted_endpoint_and_is_sorted_first_or_is_sorted_last`
        // one arm over on the (lex-endpoint, lex-interior) partition —
        // this pin covers the lex-interior arm. A permissive override
        // that returns `true` on every slot folds the lex-boundary
        // partition onto the lex-interior partition on the lex-
        // endpoint slots and silently breaks clause (33)'s per-variant
        // equality pin AND the exhaustive-complementarity pin AND the
        // lex-interior-anti-fixpoint pins.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedIsSortedInteriorKind {
            Head,
            Middle,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedIsSortedInteriorKind(pub String);
        impl core::fmt::Display for UnknownDriftedIsSortedInteriorKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted is-sorted-interior kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedIsSortedInteriorKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Middle, Self::Tail];
            const SET_LABEL: &'static str = "drifted is-sorted-interior kind";
            type Unknown = UnknownDriftedIsSortedInteriorKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Middle => "middle",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedIsSortedInteriorKind(s.to_owned())
            }
            fn is_sorted_interior(self) -> bool {
                // Drifted override — returns `true` on every slot,
                // folding the lex-boundary partition onto the lex-
                // interior partition on the lex-endpoint slots and
                // silently breaking clause (33)'s per-variant
                // equality pin.
                let _ = self;
                true
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedIsSortedInteriorKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted an is_sorted_interior() override drifted from the natural !is_sorted_endpoint composition",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_first_and_all_head() {
        // The well-formedness sweep's (18) clause — `T::first()` MUST
        // equal `T::ALL[0]`. A hand-impl'd implementor whose override
        // drifts the head anchor (returns an interior variant, returns
        // the tail, returns a stale variant after a variant-listing
        // edit) fails the sweep loudly rather than silently
        // bifurcating the endpoint-anchor surface every downstream
        // defaulter / iterator-start consumer routes through. Pinning
        // the failure path here keeps the testkit's (18) clause
        // guaranteed-to-fire on the head-endpoint arm — a regression
        // that makes the assertion permissive (e.g. a future "any
        // variant that appears in `ALL`" relaxation) breaks this
        // stub-level contract before any per-implementor sweep runs.
        // Sibling posture to `assert_closed_set_well_formed_catches_drift_between_last_and_all_tail`
        // one endpoint over on the (head, tail) partition of clause
        // (18).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedFirstKind {
            Head,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedFirstKind(pub String);
        impl core::fmt::Display for UnknownDriftedFirstKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted first kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedFirstKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Tail];
            const SET_LABEL: &'static str = "drifted first kind";
            type Unknown = UnknownDriftedFirstKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedFirstKind(s.to_owned())
            }
            fn first() -> Self {
                // Drifted override — returns the tail rather than the
                // head, swapping the (head, tail) endpoint anchor
                // partition on the head arm.
                Self::Tail
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedFirstKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a first() override drifted from the natural Self::ALL[0] projection",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_last_and_all_tail() {
        // The well-formedness sweep's (18) clause — `T::last()` MUST
        // equal `T::ALL[T::ALL.len() - 1]`. Symmetric to the
        // `_catches_drift_between_first_and_all_head` sibling one
        // endpoint over on the (head, tail) partition — this pin
        // covers the tail arm.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedLastKind {
            Head,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedLastKind(pub String);
        impl core::fmt::Display for UnknownDriftedLastKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted last kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedLastKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Tail];
            const SET_LABEL: &'static str = "drifted last kind";
            type Unknown = UnknownDriftedLastKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedLastKind(s.to_owned())
            }
            fn last() -> Self {
                // Drifted override — returns the head rather than the
                // tail, swapping the (head, tail) endpoint anchor
                // partition on the tail arm.
                Self::Head
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedLastKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a last() override drifted from the natural Self::ALL[T::ALL.len() - 1] projection",
        );
    }

    #[test]
    fn sorted_first_returns_the_lexicographically_least_variant() {
        // The lexicographic-order head endpoint anchor —
        // `T::sorted_first()` returns the label-keyed minimum as a bare
        // typed variant. `StubKind`'s labels are ("alpha", "beta",
        // "gamma"), so the lex-min anchor is `Alpha`. On this stub the
        // declaration order coincides with the lex order, so `first()`
        // and `sorted_first()` agree — the deliberate probe that
        // distinguishes the two ordering axes lives at
        // `sorted_first_and_sorted_last_bracket_sorted_variants_on_arbitrary_declaration_order`.
        assert_eq!(<StubKind as ClosedSet>::sorted_first(), StubKind::Alpha);
    }

    #[test]
    fn sorted_last_returns_the_lexicographically_greatest_variant() {
        // The lexicographic-order tail endpoint anchor —
        // `T::sorted_last()` returns the label-keyed maximum as a bare
        // typed variant. `StubKind`'s labels are ("alpha", "beta",
        // "gamma"), so the lex-max anchor is `Gamma`. Sibling posture
        // to `sorted_first_returns_the_lexicographically_least_variant`
        // one axis over on the (head, tail) partition of the
        // lexicographic-order endpoint-anchor surface.
        assert_eq!(<StubKind as ClosedSet>::sorted_last(), StubKind::Gamma);
    }

    #[test]
    fn sorted_first_and_sorted_last_bracket_sorted_variants_on_arbitrary_declaration_order() {
        // The lex-endpoint contract on a declaration order that neither
        // matches nor cleanly reverses the lex order — `T::sorted_first()`
        // MUST project the label-keyed minimum regardless of where in
        // `T::ALL` that variant sits, AND `T::sorted_last()` MUST
        // project the label-keyed maximum. A regression that returned
        // `T::ALL[0]` (the declaration head) rather than the lex-min
        // would pass
        // `sorted_first_returns_the_lexicographically_least_variant` on
        // `StubKind` (because its declaration order coincides with its
        // lex order) but silently bifurcate the lex-endpoint anchors on
        // any implementor whose declaration order diverges from the
        // canonical label ordering. Pinning the discipline here with a
        // deliberately-mis-aligned stub whose declaration head
        // (`Gamma`), declaration tail (`Beta`), lex head (`Alpha`), and
        // lex tail (`Gamma`) sit at four distinct axis crossings
        // catches that drift directly.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum LexEndpointStubKind {
            Gamma,
            Alpha,
            Beta,
        }
        #[derive(Debug)]
        struct UnknownLexEndpointStubKind(pub String);
        impl core::fmt::Display for UnknownLexEndpointStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown lex endpoint stub kind: {}", self.0)
            }
        }
        impl ClosedSet for LexEndpointStubKind {
            const ALL: &'static [Self] = &[Self::Gamma, Self::Alpha, Self::Beta];
            const SET_LABEL: &'static str = "lex endpoint stub kind";
            type Unknown = UnknownLexEndpointStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Gamma => "gamma",
                    Self::Alpha => "alpha",
                    Self::Beta => "beta",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownLexEndpointStubKind(s.to_owned())
            }
        }
        // Lex-min is `Alpha` — the linear scan finds "alpha" as the
        // canonical minimum-by-label even though it sits at
        // `ALL[1]`, NOT the declaration head `Gamma` at `ALL[0]`.
        assert_eq!(
            <LexEndpointStubKind as ClosedSet>::sorted_first(),
            LexEndpointStubKind::Alpha,
        );
        // Lex-max is `Gamma` — the linear scan finds "gamma" as the
        // canonical maximum-by-label even though it sits at
        // `ALL[0]`, NOT the declaration tail `Beta` at `ALL[2]`.
        assert_eq!(
            <LexEndpointStubKind as ClosedSet>::sorted_last(),
            LexEndpointStubKind::Gamma,
        );
        // The (declaration, lex) × (head, tail) 2×2 matrix's four
        // corners land on distinct-or-coincident variants precisely as
        // the label projection dictates — `first` != `sorted_first`,
        // `last` != `sorted_last`, `first == sorted_last`,
        // `last != sorted_first` — pinning each corner catches a
        // regression that swaps ANY endpoint arm.
        assert_eq!(
            <LexEndpointStubKind as ClosedSet>::first(),
            LexEndpointStubKind::Gamma,
        );
        assert_eq!(
            <LexEndpointStubKind as ClosedSet>::last(),
            LexEndpointStubKind::Beta,
        );
    }

    #[test]
    fn sorted_first_collapses_with_sorted_last_on_singleton_closed_set() {
        // The lex-endpoint degenerate case — a singleton closed set
        // (one variant) has `T::sorted_variants()[0] ==
        // T::sorted_variants()[T::sorted_variants().len() - 1]`, so
        // `T::sorted_first() == T::sorted_last()`. Sibling posture to
        // `first_collapses_with_last_on_singleton_closed_set` one axis
        // over on the (declaration, lex) ordering partition — the
        // singleton corner collapses on BOTH ordering axes for the same
        // structural reason (a single variant is trivially its own min
        // and max under any total order over labels).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum LexSingletonStubKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownLexSingletonStubKind(pub String);
        impl core::fmt::Display for UnknownLexSingletonStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown lex singleton stub kind: {}", self.0)
            }
        }
        impl ClosedSet for LexSingletonStubKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "lex singleton stub kind";
            type Unknown = UnknownLexSingletonStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Only => "only",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownLexSingletonStubKind(s.to_owned())
            }
        }
        assert_eq!(
            <LexSingletonStubKind as ClosedSet>::sorted_first(),
            LexSingletonStubKind::Only,
        );
        assert_eq!(
            <LexSingletonStubKind as ClosedSet>::sorted_last(),
            LexSingletonStubKind::Only,
        );
        assert_eq!(
            <LexSingletonStubKind as ClosedSet>::sorted_first(),
            <LexSingletonStubKind as ClosedSet>::sorted_last(),
        );
    }

    #[test]
    fn sorted_first_and_sorted_last_agree_with_sorted_variants_at_the_endpoint_slots() {
        // The lex-endpoint anchor / sorted-listing alignment —
        // `T::sorted_first()` MUST equal `T::sorted_variants()[0]`, AND
        // `T::sorted_last()` MUST equal
        // `T::sorted_variants()[T::sorted_variants().len() - 1]`. The
        // two primitives project the same `T::ALL` slice at the same
        // label-keyed endpoint slots through different surfaces (zero-
        // alloc bare typed anchor vs Vec-materializing sorted listing),
        // and a downstream consumer freely swaps between the two
        // depending on whether it wants the panic-free anchor or the
        // full sorted listing. Pinning the agreement here catches a
        // future regression that drifts either surface from the shared
        // label-keyed lex-endpoint projection. Sibling posture to
        // `first_and_last_agree_with_from_index_at_the_endpoint_slots`
        // one axis over on the (declaration, lex) ordering partition.
        let sorted = <StubKind as ClosedSet>::sorted_variants();
        assert_eq!(<StubKind as ClosedSet>::sorted_first(), sorted[0]);
        assert_eq!(
            <StubKind as ClosedSet>::sorted_last(),
            sorted[sorted.len() - 1],
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_sorted_first_and_sorted_variants_head() {
        // The well-formedness sweep's (19) clause —
        // `T::sorted_first()` MUST equal `T::sorted_variants()[0]`. A
        // hand-impl'd implementor whose override drifts the lex-head
        // anchor (returns an interior variant, returns the lex-tail,
        // returns a stale variant after a label edit) fails the sweep
        // loudly rather than silently bifurcating the lex-endpoint-
        // anchor surface every downstream diagnostic-boundary / lex-
        // defaulter consumer routes through. Pinning the failure path
        // here keeps the testkit's (19) clause guaranteed-to-fire on
        // the lex-head arm — a regression that makes the assertion
        // permissive (e.g. a future "any variant that appears in ALL"
        // relaxation) breaks this stub-level contract before any per-
        // implementor sweep runs. Sibling posture to
        // `assert_closed_set_well_formed_catches_drift_between_sorted_last_and_sorted_variants_tail`
        // one endpoint over on the (head, tail) partition of clause
        // (19).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedSortedFirstKind {
            Alef,
            Zayin,
        }
        #[derive(Debug)]
        struct UnknownDriftedSortedFirstKind(pub String);
        impl core::fmt::Display for UnknownDriftedSortedFirstKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted sorted first kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedSortedFirstKind {
            const ALL: &'static [Self] = &[Self::Alef, Self::Zayin];
            const SET_LABEL: &'static str = "drifted sorted first kind";
            type Unknown = UnknownDriftedSortedFirstKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Alef => "alef",
                    Self::Zayin => "zayin",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedSortedFirstKind(s.to_owned())
            }
            fn sorted_first() -> Self {
                // Drifted override — returns the lex-tail rather than
                // the lex-head, swapping the (head, tail) endpoint
                // anchor partition on the lex-head arm.
                Self::Zayin
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedSortedFirstKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a sorted_first() override drifted from the natural T::sorted_variants()[0] projection",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_sorted_last_and_sorted_variants_tail() {
        // The well-formedness sweep's (19) clause — `T::sorted_last()`
        // MUST equal `T::sorted_variants()[T::sorted_variants().len() -
        // 1]`. Symmetric to the
        // `_catches_drift_between_sorted_first_and_sorted_variants_head`
        // sibling one endpoint over on the (head, tail) partition —
        // this pin covers the lex-tail arm.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedSortedLastKind {
            Alef,
            Zayin,
        }
        #[derive(Debug)]
        struct UnknownDriftedSortedLastKind(pub String);
        impl core::fmt::Display for UnknownDriftedSortedLastKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted sorted last kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedSortedLastKind {
            const ALL: &'static [Self] = &[Self::Alef, Self::Zayin];
            const SET_LABEL: &'static str = "drifted sorted last kind";
            type Unknown = UnknownDriftedSortedLastKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Alef => "alef",
                    Self::Zayin => "zayin",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedSortedLastKind(s.to_owned())
            }
            fn sorted_last() -> Self {
                // Drifted override — returns the lex-head rather than
                // the lex-tail, swapping the (head, tail) endpoint
                // anchor partition on the lex-tail arm.
                Self::Alef
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedSortedLastKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a sorted_last() override drifted from the natural T::sorted_variants()[T::sorted_variants().len() - 1] projection",
        );
    }

    #[test]
    fn label_at_recovers_every_canonical_label_at_declaration_order_index() {
        // The direct (`usize` → `&'static str`) projection arm — for
        // every `i in 0..T::CARDINALITY`, `T::label_at(i)` returns
        // `Some(T::ALL[i].label())`. Sibling posture to
        // `from_index_recovers_every_variant_at_declaration_order_index`
        // one axis over on the (return-projection) axis of the
        // `usize`-carrier partition — this pin covers the direct-label
        // return-projection column, that pin covered the typed-variant
        // return-projection column. Both walk `Self::ALL` in
        // declaration order at the same in-range domain, keyed on the
        // typed variant's `label()` projection — the two arms of the
        // return-projection axis MUST agree slot-for-slot on the
        // (variant, `&'static str` label) content the underlying
        // `Self::ALL` entry carries. A regression that drifts either
        // arm (a permissive `label_at` override that recovers a
        // stale label after a variant-listing edit, a swapped override
        // that returns adjacent-slot labels) bifurcates the direct-
        // projection surface from the natural `from_index+label`
        // composition every `usize`-carrier decode consumer routes
        // through. Pinning the projection here catches the drift on
        // the stub-level surface before any per-implementor sweep
        // depends on the alignment downstream.
        for (i, &v) in <StubKind as ClosedSet>::ALL.iter().enumerate() {
            assert_eq!(
                <StubKind as ClosedSet>::label_at(i),
                Some(v.label()),
                "label_at({i}) failed to recover the canonical label at declaration-order index {i}",
            );
        }
    }

    #[test]
    fn label_at_rejects_first_out_of_range_index_at_cardinality() {
        // The out-of-range reject arm — `T::label_at(T::CARDINALITY)`
        // returns `None`. Sibling posture to
        // `from_index_rejects_first_out_of_range_index_at_cardinality`
        // one axis over on the (return-projection) axis of the
        // `usize`-carrier partition — both arms MUST reject the first
        // out-of-range index at the SAME boundary
        // (`T::CARDINALITY`). Pinning the reject result here means a
        // generic compact-encoding consumer that stores `variant
        // .index_of() as u8` and later decodes serialized bytes AT
        // MOST as `T::CARDINALITY - 1` can rely on the direct-label
        // projection to reject any byte at OR beyond
        // `T::CARDINALITY` — the (variant, label) return-projection
        // axis of the `usize`-carrier partition stays semantically
        // aligned on the (in-range accept, out-of-range reject)
        // partition.
        assert_eq!(
            <StubKind as ClosedSet>::label_at(<StubKind as ClosedSet>::CARDINALITY),
            None,
        );
    }

    #[test]
    fn label_at_agrees_with_from_index_composed_with_label_on_every_probe() {
        // The direct (`usize` → `&'static str`) projection MUST agree
        // with the two-step `from_index(i).map(label)` composition on
        // every input the sweep walks. This test pins the alignment
        // against a representative probe set: (a) every in-range
        // declaration-order index (`0..T::CARDINALITY`) — both arms
        // return the acceptance side `Some(label)` AND project to the
        // SAME canonical label; (b) the out-of-range boundary probes
        // (`T::CARDINALITY`, `T::CARDINALITY + 1`, `usize::MAX`) —
        // both arms return the rejection side `None`. The alignment is
        // the load-bearing contract that lets a generic consumer freely
        // swap between the direct-projection surface and the two-step
        // composition based on its rendering / storage needs without
        // changing the program's decoded-label semantics. A regression
        // that drifts either arm (a permissive `label_at` override
        // that accepts out-of-range indices, a strict override that
        // rejects a valid in-range index, a swapped override that
        // recovers the wrong label for a valid index) fails this pin
        // stub-level before any per-implementor sweep depends on the
        // alignment downstream. Sibling posture to
        // `from_index_agrees_with_all_indexing_on_every_probe` one
        // axis over on the (return-projection) axis — this pin
        // extends the (in-range accept, out-of-range reject) alignment
        // to the direct-label return-projection column.
        let cardinality = <StubKind as ClosedSet>::CARDINALITY;
        let probes: [usize; 6] = [0, 1, 2, cardinality, cardinality + 1, usize::MAX];
        for i in probes {
            let direct = <StubKind as ClosedSet>::label_at(i);
            let composed =
                <StubKind as ClosedSet>::from_index(i).map(<StubKind as ClosedSet>::label);
            assert_eq!(
                direct, composed,
                "label_at({i}) disagreed with from_index({i}).map(label) — the direct (usize → &'static str label) projection bifurcated from the natural two-step composition",
            );
        }
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_label_at_and_from_index_composition() {
        // The well-formedness sweep's (20) clause — `T::label_at(i)`
        // MUST equal `Some(T::ALL[i].label())` for every `i in
        // 0..T::CARDINALITY`, AND `T::label_at(T::CARDINALITY)` MUST
        // equal `None`. A hand-impl'd implementor whose override
        // drifts the direct-label projection — e.g. a permissive
        // override that returns `Some(_)` for an out-of-range index —
        // fails the sweep loudly rather than silently bifurcating the
        // direct-label projection surface every downstream compact-
        // encoding / metrics-per-slot / `tatara-check` per-slot
        // diagnostic consumer routes through. Pinning the failure
        // path here keeps the testkit's (20) clause guaranteed-to-fire
        // — a regression that makes the assertion permissive (e.g. a
        // future "either the in-range accept OR the out-of-range
        // reject" relaxation that only checks one arm) breaks this
        // stub-level contract before any per-implementor sweep runs.
        // Sibling posture to the twelve sibling `_catches_drift_between_*`
        // pins above (clauses 5-19); together they close the
        // structural-drift-catches sweep on every default composition
        // the trait exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedLabelAtKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedLabelAtKind(pub String);
        impl core::fmt::Display for UnknownDriftedLabelAtKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted label at kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedLabelAtKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted label at kind";
            type Unknown = UnknownDriftedLabelAtKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedLabelAtKind(s.to_owned())
            }
            fn label_at(_i: usize) -> Option<&'static str> {
                // Drifted override — accepts every `usize` payload,
                // including the reserved out-of-range probe the
                // testkit's clause (20) demands rejects. Fails the
                // direct-projection alignment with `from_index(i)
                // .map(label)` on the out-of-range reject arm.
                Some("only")
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedLabelAtKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a label_at() override drifted from the natural from_index+label composition on the out-of-range reject arm",
        );
    }

    #[test]
    fn index_of_label_recovers_declaration_order_index_for_every_canonical_label() {
        // The direct (`&str` → `usize`) projection accept arm — for
        // every canonical label `v.label()`, `T::index_of_label(label)`
        // returns `Some(v.index_of())`. Sibling posture to
        // `label_at_recovers_every_canonical_label_at_declaration_order_index`
        // one axis over on the (input-carrier) axis of the projection
        // triangle — this pin covers the `&str`-carrier direct-index
        // return-projection column, that pin covered the `usize`-carrier
        // direct-label return-projection column. Both walk `Self::ALL`
        // at their respective canonical inputs (the label side, the
        // index side) and MUST agree slot-for-slot on the underlying
        // (variant, canonical label, declaration-order index) triple.
        // A regression that drifts either arm (a permissive
        // `index_of_label` override that accepts non-canonical strings,
        // a swapped override that returns adjacent-slot indices)
        // bifurcates the direct-projection surface from the natural
        // `find_by_label+index_of` composition every `&str`-carrier
        // decode consumer routes through. Pinning the projection here
        // catches the drift on the stub-level surface before any
        // per-implementor sweep depends on the alignment downstream.
        for (i, &v) in <StubKind as ClosedSet>::ALL.iter().enumerate() {
            let label = v.label();
            assert_eq!(
                <StubKind as ClosedSet>::index_of_label(label),
                Some(i),
                "index_of_label({label:?}) failed to recover the declaration-order index at slot {i}",
            );
        }
    }

    #[test]
    fn index_of_label_rejects_non_canonical_string_without_allocating_carrier() {
        // The direct (`&str` → `usize`) projection reject arm — a
        // non-canonical `&str` returns `None` WITHOUT ever entering
        // `Self::make_unknown` (unlike the allocating `parse_label`
        // path that always materializes the typed carrier on
        // rejection). The reserved 38-char probe sits outside every
        // plausible canonical label by construction, so
        // `T::index_of_label(<probe>)` MUST reject to `None`. Sibling
        // posture to
        // `find_by_label_rejects_unknown_input_without_allocating_carrier`
        // one axis over on the (return-projection) axis of the
        // `&str`-carrier partition — this pin extends the
        // zero-allocation reject contract to the direct-index
        // return-projection column that clause (12)'s pin doesn't
        // reach.
        assert_eq!(
            <StubKind as ClosedSet>::index_of_label("__assert_closed_set_well_formed_probe__",),
            None,
        );
    }

    #[test]
    fn index_of_label_agrees_with_find_by_label_composed_with_index_of_on_every_probe() {
        // The direct (`&str` → `usize`) projection MUST agree with the
        // two-step `find_by_label(s).map(index_of)` composition on
        // every input the sweep walks. This test pins the alignment
        // against a representative probe set: (a) every canonical
        // variant label — both arms return the acceptance side
        // `Some(index)` AND project to the SAME declaration-order
        // slot; (b) the reserved 38-char probe — both arms return the
        // rejection side `None`; (c) the empty-string boundary — both
        // arms return the rejection side `None` matching clause (4)'s
        // structural reservation. The alignment is the load-bearing
        // contract that lets a generic consumer freely swap between
        // the direct-projection surface and the two-step composition
        // based on its rendering / storage needs without changing the
        // program's decoded-slot semantics. A regression that drifts
        // either arm (a permissive `index_of_label` override that
        // accepts non-canonical strings, a strict override that
        // rejects a valid canonical label, a swapped override that
        // recovers the wrong slot for a valid label) fails this pin
        // stub-level before any per-implementor sweep depends on the
        // alignment downstream. Sibling posture to
        // `label_at_agrees_with_from_index_composed_with_label_on_every_probe`
        // one axis over on the (input-carrier) axis — this pin
        // extends the (canonical accept, non-canonical reject)
        // alignment to the `&str`-carrier direct-index projection
        // column.
        let canonical_labels: [&str; 3] = ["alpha", "beta", "gamma"];
        let non_canonical: [&str; 2] = ["__assert_closed_set_well_formed_probe__", ""];
        for s in canonical_labels.iter().chain(non_canonical.iter()).copied() {
            let direct = <StubKind as ClosedSet>::index_of_label(s);
            let composed =
                <StubKind as ClosedSet>::find_by_label(s).map(<StubKind as ClosedSet>::index_of);
            assert_eq!(
                direct, composed,
                "index_of_label({s:?}) disagreed with find_by_label({s:?}).map(index_of) — the direct (&str → usize index) projection bifurcated from the natural two-step composition",
            );
        }
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_index_of_label_and_find_by_label_composition(
    ) {
        // The well-formedness sweep's (21) clause — `T::index_of_label(
        // v.label())` MUST equal `Some(v.index_of())` for every variant
        // `v` in `T::ALL`, AND `T::index_of_label(<reserved probe>)`
        // MUST equal `None`, AND `T::index_of_label("")` MUST equal
        // `None`. A hand-impl'd implementor whose override drifts the
        // direct-index projection — e.g. a permissive override that
        // returns `Some(_)` for a non-canonical `&str` — fails the
        // sweep loudly rather than silently bifurcating the
        // direct-index projection surface every downstream
        // compact-encoder / metrics-binner / `tatara-check` per-slot
        // per-label diagnostic / LSP-hover consumer routes through.
        // Pinning the failure path here keeps the testkit's (21)
        // clause guaranteed-to-fire — a regression that makes the
        // assertion permissive (e.g. a future "either the accept OR
        // the reject arm" relaxation that only checks one arm) breaks
        // this stub-level contract before any per-implementor sweep
        // runs. Sibling posture to the thirteen sibling
        // `_catches_drift_between_*` pins above (clauses 5-20);
        // together they close the structural-drift-catches sweep on
        // every default composition the trait exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedIndexOfLabelKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedIndexOfLabelKind(pub String);
        impl core::fmt::Display for UnknownDriftedIndexOfLabelKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted index of label kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedIndexOfLabelKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted index of label kind";
            type Unknown = UnknownDriftedIndexOfLabelKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedIndexOfLabelKind(s.to_owned())
            }
            fn index_of_label(_s: &str) -> Option<usize> {
                // Drifted override — accepts every `&str` payload,
                // including the reserved probe the testkit's clause
                // (21) demands rejects. Fails the direct-projection
                // alignment with `find_by_label(s).map(index_of)` on
                // the non-canonical reject arm.
                Some(0)
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedIndexOfLabelKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted an index_of_label() override drifted from the natural find_by_label+index_of composition on the non-canonical reject arm",
        );
    }

    #[test]
    fn sorted_index_of_returns_lex_order_position_for_every_variant() {
        // The direct (variant → lex-order position) projection accept
        // arm — for every canonical variant `v`, `T::sorted_index_of(v)`
        // returns the slot `v` sits at in `T::sorted_variants()`. On
        // the `StubKind` fixture, the labels `"alpha"`, `"beta"`,
        // `"gamma"` are already lex-ordered under `str::cmp`, so the
        // (Alpha, Beta, Gamma) declaration-order variants sit at lex
        // slots (0, 1, 2) respectively. Sibling posture to
        // `index_of_returns_declaration_order_position_for_every_variant`
        // one axis over on the (declaration, lex) ordering-axis
        // partition — this pin covers the lex-ordering forward
        // projection, that pin covered the declaration-ordering
        // forward projection. Both walk `Self::ALL` at their
        // respective canonical carriers and MUST agree slot-for-slot
        // on the underlying (variant → position) forward projection
        // in their respective ordering axis.
        assert_eq!(<StubKind as ClosedSet>::sorted_index_of(StubKind::Alpha), 0);
        assert_eq!(<StubKind as ClosedSet>::sorted_index_of(StubKind::Beta), 1);
        assert_eq!(<StubKind as ClosedSet>::sorted_index_of(StubKind::Gamma), 2);
    }

    #[test]
    fn sorted_index_of_stays_within_zero_to_cardinality() {
        // The (variant → lex position) forward projection's
        // range-bound contract — for every canonical variant `v`,
        // `T::sorted_index_of(v)` sits STRICTLY less than
        // `T::CARDINALITY`. The strict-`<` label count over
        // `T::ALL` under the label-pairwise-distinctness contract
        // (clause 3) makes the count at most `T::CARDINALITY - 1`
        // for every variant (the variant itself never counts under
        // strict-`<`, and the (`T::CARDINALITY - 1`) other variants
        // can at most all sit strictly below it under `str::cmp`).
        // Sibling posture to `index_of_stays_within_zero_to_cardinality`
        // one axis over on the (declaration, lex) ordering-axis
        // partition — this pin extends the range-bound contract to
        // the lex-ordering forward projection.
        for &v in <StubKind as ClosedSet>::ALL {
            assert!(
                <StubKind as ClosedSet>::sorted_index_of(v) < <StubKind as ClosedSet>::CARDINALITY,
                "sorted_index_of({v:?}) fell outside 0..T::CARDINALITY",
            );
        }
    }

    #[test]
    fn sorted_index_of_agrees_with_sorted_variants_position_on_every_probe() {
        // The direct (variant → lex position) projection MUST agree
        // with the two-step
        // `sorted_variants().iter().position(|w| *w == v)` composition
        // on every input the sweep walks. This test pins the alignment
        // across every canonical variant. The alignment is the
        // load-bearing contract that lets a generic consumer freely
        // swap between the direct-projection surface (a zero-alloc
        // label-keyed strict-`<` count) and the two-step composition
        // (a `sorted_variants` Vec allocation plus a
        // `Iterator::position` sweep) based on its rendering / storage
        // needs without changing the program's lex-slot semantics.
        // Sibling posture to
        // `index_of_projects_all_indexing_and_index_of_into_the_identity_permutation`
        // one axis over on the (declaration, lex) ordering-axis
        // partition — this pin extends the projection-composition
        // alignment to the lex-ordering forward projection.
        let sorted = <StubKind as ClosedSet>::sorted_variants();
        for &v in <StubKind as ClosedSet>::ALL {
            let direct = <StubKind as ClosedSet>::sorted_index_of(v);
            let composed = sorted.iter().position(|&w| w == v).unwrap();
            assert_eq!(
                direct, composed,
                "sorted_index_of({v:?}) disagreed with sorted_variants().position(|w| *w == {v:?}) — the direct (variant → lex position) projection bifurcated from the natural two-step composition",
            );
        }
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_sorted_index_of_and_sorted_variants_position(
    ) {
        // The well-formedness sweep's (22) clause — for every variant
        // `v` in `T::ALL`, `T::sorted_index_of(v)` MUST equal the
        // position of `v` in `T::sorted_variants()`. A hand-impl'd
        // implementor whose override drifts the direct (variant → lex
        // position) projection — e.g. a stale override that always
        // returns `usize::MAX` — fails the sweep loudly rather than
        // silently bifurcating the lex-position projection surface
        // every downstream lex-sorted-metrics-binner /
        // lex-order-stable-wire-encoder / bitset-observed-slot-lex-
        // renderer consumer routes through. Sibling posture to the
        // fourteen sibling `_catches_drift_between_*` pins above
        // (clauses 5-21); together they close the structural-drift-
        // catches sweep on every default composition the trait
        // exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedSortedIndexOfKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedSortedIndexOfKind(pub String);
        impl core::fmt::Display for UnknownDriftedSortedIndexOfKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted sorted index of kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedSortedIndexOfKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted sorted index of kind";
            type Unknown = UnknownDriftedSortedIndexOfKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedSortedIndexOfKind(s.to_owned())
            }
            fn sorted_index_of(self) -> usize {
                // Drifted override — always returns `usize::MAX`
                // regardless of the variant's lex-order slot. Fails
                // the alignment with `sorted_variants().position(|w|
                // *w == self)` on the singleton set's only slot
                // (which is 0, not usize::MAX).
                usize::MAX
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedSortedIndexOfKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a sorted_index_of() override drifted from the natural sorted_variants.position projection",
        );
    }

    #[test]
    fn from_sorted_index_recovers_every_canonical_variant_at_lex_order_slot() {
        // The direct (lex-order index → variant) inverse projection
        // accept arm — for every canonical lex slot `i`,
        // `T::from_sorted_index(i)` recovers `Some(v)` where `v` is
        // the variant `T::sorted_variants()` places at slot `i`. On
        // the `StubKind` fixture, the labels `"alpha"`, `"beta"`,
        // `"gamma"` are already lex-ordered under `str::cmp`, so lex
        // slots (0, 1, 2) recover (Alpha, Beta, Gamma) respectively.
        // Sibling posture to `from_index_recovers_every_canonical_variant`
        // one axis over on the (declaration, lex) ordering-axis
        // partition of the (position → variant) inverse-projection
        // surface — this pin covers the lex-ordering inverse
        // projection, that pin covered the declaration-ordering
        // inverse projection.
        assert_eq!(
            <StubKind as ClosedSet>::from_sorted_index(0),
            Some(StubKind::Alpha)
        );
        assert_eq!(
            <StubKind as ClosedSet>::from_sorted_index(1),
            Some(StubKind::Beta)
        );
        assert_eq!(
            <StubKind as ClosedSet>::from_sorted_index(2),
            Some(StubKind::Gamma)
        );
    }

    #[test]
    fn from_sorted_index_rejects_first_out_of_range_index_at_cardinality() {
        // The (lex-order index → variant) inverse projection reject
        // arm — `T::from_sorted_index(T::CARDINALITY)` MUST return
        // `None`. The bounded-decode contract closes the first slot
        // strictly outside the closed set at `T::CARDINALITY`, so a
        // downstream lex-order compact-decoder that ingests a `u8`
        // lex slot from a remote-authored payload fails-closed on any
        // out-of-range byte rather than silently folding it onto an
        // in-range variant. Sibling posture to
        // `from_index_rejects_first_out_of_range_index_at_cardinality`
        // one axis over on the (declaration, lex) ordering-axis
        // partition of the (position → variant) inverse-projection
        // surface.
        assert_eq!(
            <StubKind as ClosedSet>::from_sorted_index(<StubKind as ClosedSet>::CARDINALITY),
            None,
            "from_sorted_index(T::CARDINALITY) MUST return None — the bounded-decode arm accepted the first structurally-out-of-range lex slot",
        );
    }

    #[test]
    fn from_sorted_index_agrees_with_sorted_variants_get_copied_on_every_probe() {
        // The direct (lex-order index → variant) inverse projection
        // MUST agree with the two-step
        // `sorted_variants().get(i).copied()` composition on every
        // input the sweep walks. This test pins the alignment across
        // every canonical lex slot AND the first out-of-range slot.
        // The alignment is the load-bearing contract that lets a
        // generic consumer freely swap between the direct-projection
        // surface (a `Self::sorted_variants().get(i).copied()` route)
        // and the two-step composition (`sorted_variants` Vec
        // allocation plus a `<[T]>::get` bounded-index projection)
        // based on its rendering / storage needs without changing the
        // program's lex-order inverse-decode semantics. Sibling
        // posture to `from_index_agrees_with_all_get_copied_on_every_probe`
        // one axis over on the (declaration, lex) ordering-axis
        // partition of the (position → variant) inverse-projection
        // surface.
        let sorted = <StubKind as ClosedSet>::sorted_variants();
        for i in 0..=<StubKind as ClosedSet>::CARDINALITY {
            let direct = <StubKind as ClosedSet>::from_sorted_index(i);
            let composed = sorted.get(i).copied();
            assert_eq!(
                direct, composed,
                "from_sorted_index({i}) disagreed with sorted_variants().get({i}).copied() — the direct (lex-order index → variant) inverse projection bifurcated from the natural two-step composition",
            );
        }
    }

    #[test]
    fn from_sorted_index_round_trips_through_sorted_index_of_on_every_variant() {
        // BIJECTION ROUND-TRIP (lex-order axis): for every canonical
        // variant `v`, `T::from_sorted_index(v.sorted_index_of())` MUST
        // recover `Some(v)` — the (variant → lex position) forward
        // projection composed with the (lex position → variant)
        // inverse projection is the identity on `T::ALL`. This closes
        // the round-trip contract on the lex ordering axis, sibling
        // posture to `from_index_round_trips_through_index_of_on_every_variant`
        // on the declaration ordering axis. Together the two round-
        // trip pins close the (variant ↔ position) bijection at BOTH
        // ordering axes as a runtime-verified TYPED THEOREM rather
        // than a construction-only implication.
        for &v in <StubKind as ClosedSet>::ALL {
            let slot = <StubKind as ClosedSet>::sorted_index_of(v);
            let recovered = <StubKind as ClosedSet>::from_sorted_index(slot);
            assert_eq!(
                recovered,
                Some(v),
                "from_sorted_index(sorted_index_of({v:?})) = from_sorted_index({slot}) failed the round-trip — the (variant → lex position → variant) bijection bifurcated at the round-trip boundary",
            );
        }
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_from_sorted_index_and_sorted_variants_get(
    ) {
        // The well-formedness sweep's (23) clause — for every `i in
        // 0..T::CARDINALITY`, `T::from_sorted_index(i)` MUST equal
        // `Some(T::sorted_variants()[i])`, AND
        // `T::from_sorted_index(T::CARDINALITY)` MUST equal `None`. A
        // hand-impl'd implementor whose override drifts the bounded-
        // decode arm — e.g. a permissive override that returns
        // `Some(_)` regardless of the input lex slot — fails the
        // sweep loudly rather than silently bifurcating the lex-order
        // inverse-decode surface every downstream lex-order compact-
        // encoding / lex-order-bitset-observed-variant / lex-order-
        // lookup-table-iteration consumer routes through. Sibling
        // posture to the fifteen sibling `_catches_drift_between_*`
        // pins above (clauses 5-22); together they close the
        // structural-drift-catches sweep on every default composition
        // the trait exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedFromSortedIndexKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedFromSortedIndexKind(pub String);
        impl core::fmt::Display for UnknownDriftedFromSortedIndexKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted from sorted index kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedFromSortedIndexKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted from sorted index kind";
            type Unknown = UnknownDriftedFromSortedIndexKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedFromSortedIndexKind(s.to_owned())
            }
            fn from_sorted_index(_i: usize) -> Option<Self> {
                // Drifted override — always returns `Some(Only)`
                // regardless of the input lex slot. Fails the
                // out-of-range guard at `T::CARDINALITY` — the
                // permissive body silently folds an out-of-range
                // serialized lex index onto the singleton's only
                // variant.
                Some(Self::Only)
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedFromSortedIndexKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a from_sorted_index() override drifted from the natural sorted_variants.get().copied() bounded-decode arm",
        );
    }

    #[test]
    fn sorted_label_at_recovers_every_canonical_label_at_lex_order_slot() {
        // The direct (lex-order index → `&'static str` label) projection
        // accept arm — for every canonical lex slot `i`,
        // `T::sorted_label_at(i)` recovers `Some(l)` where `l` is the
        // label `T::sorted_labels()` places at slot `i`. On the
        // `StubKind` fixture, the labels `"alpha"`, `"beta"`, `"gamma"`
        // are already lex-ordered under `str::cmp`, so lex slots
        // (0, 1, 2) recover ("alpha", "beta", "gamma") respectively.
        // Sibling posture to `from_sorted_index_recovers_every_canonical_variant_at_lex_order_slot`
        // one return-projection column over — this pin covers the
        // (`usize` lex slot → `&'static str` label) direct projection,
        // that pin covered the (`usize` lex slot → typed variant)
        // inverse projection.
        assert_eq!(<StubKind as ClosedSet>::sorted_label_at(0), Some("alpha"));
        assert_eq!(<StubKind as ClosedSet>::sorted_label_at(1), Some("beta"));
        assert_eq!(<StubKind as ClosedSet>::sorted_label_at(2), Some("gamma"));
    }

    #[test]
    fn sorted_label_at_rejects_first_out_of_range_index_at_cardinality() {
        // The (lex-order index → `&'static str` label) direct projection
        // reject arm — `T::sorted_label_at(T::CARDINALITY)` MUST return
        // `None`. The bounded-decode contract closes the first slot
        // strictly outside the closed set at `T::CARDINALITY`, so a
        // downstream lex-order compact-decoder that ingests a `u8` lex
        // slot from a remote-authored payload fails-closed on any
        // out-of-range byte rather than silently folding it onto an
        // in-range label. Sibling posture to
        // `from_sorted_index_rejects_first_out_of_range_index_at_cardinality`
        // one return-projection column over on the lex-axis triangle.
        assert_eq!(
            <StubKind as ClosedSet>::sorted_label_at(
                <StubKind as ClosedSet>::CARDINALITY
            ),
            None,
            "sorted_label_at(T::CARDINALITY) MUST return None — the bounded-decode arm accepted the first structurally-out-of-range lex slot",
        );
    }

    #[test]
    fn sorted_label_at_agrees_with_from_sorted_index_map_label_on_every_probe() {
        // The direct (lex-order index → `&'static str` label) projection
        // MUST agree with the two-step
        // `from_sorted_index(i).map(label)` composition on every input
        // the sweep walks. This test pins the alignment across every
        // canonical lex slot AND the first out-of-range slot. The
        // alignment is the load-bearing contract that lets a generic
        // consumer freely swap between the direct-projection surface
        // and the two-step composition based on its rendering needs
        // without changing the program's lex-order rendering semantics.
        // Sibling posture to `label_at`'s
        // (declaration-axis) alignment-with-composition test one
        // ordering-axis over.
        for i in 0..=<StubKind as ClosedSet>::CARDINALITY {
            let direct = <StubKind as ClosedSet>::sorted_label_at(i);
            let composed =
                <StubKind as ClosedSet>::from_sorted_index(i).map(<StubKind as ClosedSet>::label);
            assert_eq!(
                direct, composed,
                "sorted_label_at({i}) disagreed with from_sorted_index({i}).map(label) — the direct (lex-order index → label) projection bifurcated from the natural two-step composition",
            );
        }
    }

    #[test]
    fn sorted_label_at_agrees_with_sorted_labels_get_copied_on_every_probe() {
        // The direct (lex-order index → `&'static str` label) projection
        // MUST agree with the `sorted_labels().get(i).copied()` route on
        // every input the sweep walks — the OTHER natural two-step
        // composition through the trait's alternate lex-order candidate-
        // list projection. This test pins the alignment across every
        // canonical lex slot AND the first out-of-range slot, closing
        // the direct-projection surface against BOTH natural two-step
        // compositions (the `from_sorted_index`-then-`label` route AND
        // the `sorted_labels`-then-`get`-`copied` route). A future
        // implementor whose override drifts from EITHER two-step
        // composition would show up on one of the two alignment sweeps
        // but pass the other silently pre-lift; this pin catches the
        // asymmetric drift on the `sorted_labels`-side composition
        // specifically.
        let sorted = <StubKind as ClosedSet>::sorted_labels();
        for i in 0..=<StubKind as ClosedSet>::CARDINALITY {
            let direct = <StubKind as ClosedSet>::sorted_label_at(i);
            let composed = sorted.get(i).copied();
            assert_eq!(
                direct, composed,
                "sorted_label_at({i}) disagreed with sorted_labels().get({i}).copied() — the direct (lex-order index → label) projection bifurcated from the natural Vec-slice composition",
            );
        }
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_sorted_label_at_and_composition() {
        // The well-formedness sweep's (24) clause — for every `i in
        // 0..T::CARDINALITY`, `T::sorted_label_at(i)` MUST equal
        // `Some(T::sorted_labels()[i])`, AND
        // `T::sorted_label_at(T::CARDINALITY)` MUST equal `None`. A
        // hand-impl'd implementor whose override drifts the bounded-
        // decode arm — e.g. a permissive override that returns
        // `Some(_)` regardless of the input lex slot — fails the sweep
        // loudly rather than silently bifurcating the lex-order
        // direct-label projection surface every downstream lex-order
        // compact-encoding / lex-sorted-metrics-binner / bitset-observed-
        // slot-lex-renderer consumer routes through. Sibling posture to
        // the sixteen sibling `_catches_drift_between_*` pins above
        // (clauses 5-23); together they close the structural-drift-
        // catches sweep on every default composition the trait exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedSortedLabelAtKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedSortedLabelAtKind(pub String);
        impl core::fmt::Display for UnknownDriftedSortedLabelAtKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted sorted label at kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedSortedLabelAtKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted sorted label at kind";
            type Unknown = UnknownDriftedSortedLabelAtKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedSortedLabelAtKind(s.to_owned())
            }
            fn sorted_label_at(_i: usize) -> Option<&'static str> {
                // Drifted override — always returns `Some("only")`
                // regardless of the input lex slot. Fails the
                // out-of-range guard at `T::CARDINALITY` — the
                // permissive body silently folds an out-of-range
                // serialized lex index onto the singleton's only
                // label.
                Some("only")
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedSortedLabelAtKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a sorted_label_at() override drifted from the natural from_sorted_index().map(label) direct-label projection arm",
        );
    }

    #[test]
    fn sorted_index_of_label_recovers_lex_order_index_for_every_canonical_label() {
        // The direct (`&str` → `usize` lex position) projection accept
        // arm — for every canonical label `v.label()`,
        // `T::sorted_index_of_label(label)` returns
        // `Some(v.sorted_index_of())`. On the `StubKind` fixture, the
        // labels `"alpha"`, `"beta"`, `"gamma"` are already lex-ordered
        // under `str::cmp`, so the (Alpha, Beta, Gamma) declaration-
        // order variants sit at lex slots (0, 1, 2) — same slots the
        // `sorted_index_of_returns_lex_order_position_for_every_variant`
        // sibling pin already anchors on the variant-carrier side.
        // Sibling posture to
        // `index_of_label_recovers_declaration_order_index_for_every_canonical_label`
        // one ordering-axis over on the (declaration, lex) ordering-
        // axis partition — this pin covers the lex-ordering direct-slot
        // return-projection column, that pin covered the declaration-
        // ordering direct-slot return-projection column. Both walk
        // `Self::ALL` at the label-carrier canonical input and MUST
        // agree slot-for-slot on the underlying (variant, canonical
        // label, position) triple within their respective ordering axis.
        for &v in <StubKind as ClosedSet>::ALL {
            let label = v.label();
            assert_eq!(
                <StubKind as ClosedSet>::sorted_index_of_label(label),
                Some(<StubKind as ClosedSet>::sorted_index_of(v)),
                "sorted_index_of_label({label:?}) failed to recover the lex-order slot for the canonical variant {v:?}",
            );
        }
    }

    #[test]
    fn sorted_index_of_label_rejects_non_canonical_string_without_allocating_carrier() {
        // The direct (`&str` → `usize` lex position) projection reject
        // arm — a non-canonical `&str` returns `None` WITHOUT ever
        // entering `Self::make_unknown` (unlike the allocating
        // `parse_label` path that always materializes the typed carrier
        // on rejection). The reserved 38-char probe sits outside every
        // plausible canonical label by construction, so
        // `T::sorted_index_of_label(<probe>)` MUST reject to `None`.
        // Sibling posture to
        // `index_of_label_rejects_non_canonical_string_without_allocating_carrier`
        // one ordering-axis over on the (declaration, lex) partition —
        // this pin extends the zero-allocation reject contract to the
        // lex-order direct-slot return-projection column.
        assert_eq!(
            <StubKind as ClosedSet>::sorted_index_of_label(
                "__assert_closed_set_well_formed_probe__",
            ),
            None,
        );
    }

    #[test]
    fn sorted_index_of_label_agrees_with_find_by_label_map_sorted_index_of_on_every_probe() {
        // The direct (`&str` → `usize` lex position) projection MUST
        // agree with the two-step `find_by_label(s).map(sorted_index_of)`
        // composition on every input the sweep walks. This test pins
        // the alignment against a representative probe set: (a) every
        // canonical variant label — both arms return the acceptance
        // side `Some(lex_slot)` AND project to the SAME lex-order slot;
        // (b) the reserved 38-char probe — both arms return the
        // rejection side `None`; (c) the empty-string boundary — both
        // arms return the rejection side `None` matching clause (4)'s
        // structural reservation. The alignment is the load-bearing
        // contract that lets a generic consumer freely swap between
        // the direct-projection surface and the two-step composition
        // based on its rendering / storage needs without changing the
        // program's decoded-slot semantics. A regression that drifts
        // either arm (a permissive `sorted_index_of_label` override
        // that accepts non-canonical strings, a strict override that
        // rejects a valid canonical label, a swapped override that
        // recovers the wrong lex slot for a valid label) fails this
        // pin stub-level before any per-implementor sweep depends on
        // the alignment downstream. Sibling posture to
        // `index_of_label_agrees_with_find_by_label_composed_with_index_of_on_every_probe`
        // one ordering-axis over — this pin extends the (canonical
        // accept, non-canonical reject) alignment to the `&str`-carrier
        // lex-order direct-slot projection column.
        let canonical_labels: [&str; 3] = ["alpha", "beta", "gamma"];
        let non_canonical: [&str; 2] = ["__assert_closed_set_well_formed_probe__", ""];
        for s in canonical_labels.iter().chain(non_canonical.iter()).copied() {
            let direct = <StubKind as ClosedSet>::sorted_index_of_label(s);
            let composed = <StubKind as ClosedSet>::find_by_label(s)
                .map(<StubKind as ClosedSet>::sorted_index_of);
            assert_eq!(
                direct, composed,
                "sorted_index_of_label({s:?}) disagreed with find_by_label({s:?}).map(sorted_index_of) — the direct (&str → usize lex-order index) projection bifurcated from the natural two-step composition",
            );
        }
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_sorted_index_of_label_and_composition() {
        // The well-formedness sweep's (25) clause —
        // `T::sorted_index_of_label(v.label())` MUST equal
        // `Some(v.sorted_index_of())` for every variant `v` in `T::ALL`,
        // AND `T::sorted_index_of_label(<reserved probe>)` MUST equal
        // `None`, AND `T::sorted_index_of_label("")` MUST equal `None`.
        // A hand-impl'd implementor whose override drifts the direct-
        // lex-slot projection — e.g. a permissive override that returns
        // `Some(_)` for a non-canonical `&str` — fails the sweep loudly
        // rather than silently bifurcating the lex-order direct-slot
        // projection surface every downstream lex-sorted-metrics-binner
        // / lex-order-compact-encoder / `tatara-check` per-lex-slot
        // per-label diagnostic / LSP-hover consumer routes through.
        // Pinning the failure path here keeps the testkit's (25) clause
        // guaranteed-to-fire — a regression that makes the assertion
        // permissive (e.g. a future "either the accept OR the reject
        // arm" relaxation that only checks one arm) breaks this stub-
        // level contract before any per-implementor sweep runs. Sibling
        // posture to the seventeen sibling `_catches_drift_between_*`
        // pins above (clauses 5-24); together they close the
        // structural-drift-catches sweep on every default composition
        // the trait exposes.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedSortedIndexOfLabelKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownDriftedSortedIndexOfLabelKind(pub String);
        impl core::fmt::Display for UnknownDriftedSortedIndexOfLabelKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted sorted index of label kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedSortedIndexOfLabelKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "drifted sorted index of label kind";
            type Unknown = UnknownDriftedSortedIndexOfLabelKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedSortedIndexOfLabelKind(s.to_owned())
            }
            fn sorted_index_of_label(_s: &str) -> Option<usize> {
                // Drifted override — accepts every `&str` payload,
                // including the reserved probe the testkit's clause
                // (25) demands rejects. Fails the direct-projection
                // alignment with `find_by_label(s).map(sorted_index_of)`
                // on the non-canonical reject arm.
                Some(0)
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedSortedIndexOfLabelKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a sorted_index_of_label() override drifted from the natural find_by_label+sorted_index_of composition on the non-canonical reject arm",
        );
    }

    #[test]
    fn next_walks_declaration_order_forward_chain() {
        // The forward-neighbor projection — `v.next()` walks
        // `T::ALL` one slot forward from each variant, returning
        // `Some` on every interior slot and `None` at the tail. The
        // `StubKind` variant listing is `[Alpha, Beta, Gamma]`, so
        // the forward chain is Alpha → Beta → Gamma → None. Sibling
        // posture to `first_and_last_agree_with_from_index_at_the_endpoint_slots`
        // on the (endpoint-anchor, neighbor) axis — both walk `T::ALL`
        // through the same `index_of` + `from_index` composition,
        // one at the endpoints and one at every step of the
        // declaration-order chain.
        assert_eq!(
            <StubKind as ClosedSet>::next(StubKind::Alpha),
            Some(StubKind::Beta),
        );
        assert_eq!(
            <StubKind as ClosedSet>::next(StubKind::Beta),
            Some(StubKind::Gamma),
        );
        assert_eq!(<StubKind as ClosedSet>::next(StubKind::Gamma), None);
    }

    #[test]
    fn prev_walks_declaration_order_backward_chain() {
        // The backward-neighbor projection — `v.prev()` walks
        // `T::ALL` one slot backward from each variant, returning
        // `Some` on every interior slot and `None` at the head. The
        // `StubKind` variant listing is `[Alpha, Beta, Gamma]`, so
        // the backward chain is Gamma → Beta → Alpha → None. Sibling
        // posture to `next_walks_declaration_order_forward_chain` one
        // axis over on the (forward, backward) direction partition.
        assert_eq!(<StubKind as ClosedSet>::prev(StubKind::Alpha), None);
        assert_eq!(
            <StubKind as ClosedSet>::prev(StubKind::Beta),
            Some(StubKind::Alpha),
        );
        assert_eq!(
            <StubKind as ClosedSet>::prev(StubKind::Gamma),
            Some(StubKind::Beta),
        );
    }

    #[test]
    fn next_and_prev_are_inverses_on_the_interior() {
        // The (forward, backward) neighbor-inverse contract — for
        // every interior variant `v` (not the head, not the tail),
        // `v.next().unwrap().prev() == Some(v)` AND
        // `v.prev().unwrap().next() == Some(v)`. The two projections
        // compose the same `index_of` + `from_index` bijection under
        // opposite `usize` arithmetic (`+ 1` vs `- 1`), so the
        // interior neighbor edges MUST close as a round-trip. A
        // regression that returns the wrong neighbor for either arm
        // would silently break this round-trip, folding a two-step
        // walk `v → v.next().unwrap() → w` onto some
        // `w ≠ v` at the second step's inverse. Sibling posture to
        // `first_and_last_agree_with_from_index_at_the_endpoint_slots`
        // — both anchor the (index_of, from_index) bijection at
        // different structural landmarks (endpoints vs interior
        // neighbors) on the SAME primitive pair.
        for &v in <StubKind as ClosedSet>::ALL {
            if let Some(fwd) = <StubKind as ClosedSet>::next(v) {
                assert_eq!(
                    <StubKind as ClosedSet>::prev(fwd),
                    Some(v),
                    "{v:?}.next().unwrap().prev() did not round-trip",
                );
            }
            if let Some(bwd) = <StubKind as ClosedSet>::prev(v) {
                assert_eq!(
                    <StubKind as ClosedSet>::next(bwd),
                    Some(v),
                    "{v:?}.prev().unwrap().next() did not round-trip",
                );
            }
        }
    }

    #[test]
    fn first_prev_and_last_next_pin_the_endpoint_boundary_fixpoints() {
        // The endpoint-anchor / neighbor-axis fixpoints —
        // `T::first().prev() == None` AND `T::last().next() == None`.
        // The two fixpoints thread the head-endpoint anchor back
        // through the backward-neighbor axis AND the tail-endpoint
        // anchor back through the forward-neighbor axis at ONE
        // structural landmark each. A regression that wraps around
        // (returning `Some(T::last())` for `T::first().prev()`, or
        // `Some(T::first())` for `T::last().next()`) would silently
        // fold a bounded traversal onto an infinite loop every
        // downstream state-machine iterator / phase-fold reducer
        // consumer routes through. Pinning both fixpoints here
        // catches that drift directly on the endpoint boundary.
        assert_eq!(
            <StubKind as ClosedSet>::prev(<StubKind as ClosedSet>::first()),
            None,
        );
        assert_eq!(
            <StubKind as ClosedSet>::next(<StubKind as ClosedSet>::last()),
            None,
        );
    }

    #[test]
    fn next_and_prev_on_singleton_closed_set_both_return_none() {
        // The neighbor-axis degenerate case — a singleton closed set
        // (one variant) has `T::ALL[0] == T::first() == T::last()`,
        // so BOTH `T::first().prev() == None` AND `T::last().next()
        // == None` collapse onto the same variant — every neighbor
        // walk from the sole variant must return `None`. A regression
        // that treats the two direction arms as structurally distinct
        // (a hand-rolled `match` that returned `Some(_)` for one
        // direction) would silently break the natural degenerate-
        // case invariant. Pinning the collapse here catches that
        // drift. Sibling posture to
        // `first_collapses_with_last_on_singleton_closed_set` — same
        // degenerate carve, extended to the neighbor axis.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum NeighborSingletonKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownNeighborSingletonKind(pub String);
        impl core::fmt::Display for UnknownNeighborSingletonKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown neighbor singleton kind: {}", self.0)
            }
        }
        impl ClosedSet for NeighborSingletonKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "neighbor singleton kind";
            type Unknown = UnknownNeighborSingletonKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownNeighborSingletonKind(s.to_owned())
            }
        }
        assert_eq!(
            <NeighborSingletonKind as ClosedSet>::next(NeighborSingletonKind::Only),
            None,
        );
        assert_eq!(
            <NeighborSingletonKind as ClosedSet>::prev(NeighborSingletonKind::Only),
            None,
        );
    }

    #[test]
    fn next_and_prev_walk_arbitrary_declaration_order() {
        // The neighbor-projection contract on an arbitrary
        // declaration order — `v.next()` MUST project the successor
        // in `T::ALL`'s layout regardless of the label ordering, AND
        // `v.prev()` MUST project the predecessor. A regression that
        // keyed on `label()` order (rather than `T::ALL` position)
        // would pass on `StubKind` (where declaration order aligns
        // with alphabetic order) but silently bifurcate on any
        // implementor whose `ALL`-array layout differs from its lex
        // order. Reuses the deliberately-reverse `NeighborReverseKind`
        // shape so the neighbor contract lands on the same
        // declaration-order-inversion probe the sorted-variants /
        // endpoint contracts bind against.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum NeighborReverseKind {
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownNeighborReverseKind(pub String);
        impl core::fmt::Display for UnknownNeighborReverseKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown neighbor reverse kind: {}", self.0)
            }
        }
        impl ClosedSet for NeighborReverseKind {
            const ALL: &'static [Self] = &[Self::Gamma, Self::Beta, Self::Alpha];
            const SET_LABEL: &'static str = "neighbor reverse kind";
            type Unknown = UnknownNeighborReverseKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownNeighborReverseKind(s.to_owned())
            }
        }
        // Declaration order: Gamma → Beta → Alpha. Forward walk
        // follows this layout, NOT the alphabetic order.
        assert_eq!(
            <NeighborReverseKind as ClosedSet>::next(NeighborReverseKind::Gamma),
            Some(NeighborReverseKind::Beta),
        );
        assert_eq!(
            <NeighborReverseKind as ClosedSet>::next(NeighborReverseKind::Beta),
            Some(NeighborReverseKind::Alpha),
        );
        assert_eq!(
            <NeighborReverseKind as ClosedSet>::next(NeighborReverseKind::Alpha),
            None,
        );
        // Backward walk: Alpha → Beta → Gamma → None.
        assert_eq!(
            <NeighborReverseKind as ClosedSet>::prev(NeighborReverseKind::Alpha),
            Some(NeighborReverseKind::Beta),
        );
        assert_eq!(
            <NeighborReverseKind as ClosedSet>::prev(NeighborReverseKind::Beta),
            Some(NeighborReverseKind::Gamma),
        );
        assert_eq!(
            <NeighborReverseKind as ClosedSet>::prev(NeighborReverseKind::Gamma),
            None,
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_next_and_composition() {
        // The well-formedness sweep's (26) clause — `v.next()` MUST
        // equal `T::from_index(v.index_of() + 1)` on every variant.
        // A hand-impl'd implementor whose override drifts the
        // forward-neighbor projection (accepts `Some(_)` at the tail
        // where the natural composition would return `None`, folds a
        // tail-boundary walk onto a wraparound to the head) fails
        // the sweep loudly rather than silently bifurcating the
        // forward-traversal surface every downstream state-machine
        // iterator / saga-step engine / phase-fold reducer consumer
        // routes through. Sibling posture to
        // `assert_closed_set_well_formed_catches_drift_between_prev_and_composition`
        // one direction over on the (forward, backward) partition of
        // clause (26).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedNextKind {
            Head,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedNextKind(pub String);
        impl core::fmt::Display for UnknownDriftedNextKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted next kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedNextKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Tail];
            const SET_LABEL: &'static str = "drifted next kind";
            type Unknown = UnknownDriftedNextKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedNextKind(s.to_owned())
            }
            fn next(self) -> Option<Self> {
                // Drifted override — wraps around at the tail rather
                // than returning `None`, folding a bounded forward
                // walk onto an infinite loop every state-machine
                // iterator consumer routes through.
                Some(match self {
                    Self::Head => Self::Tail,
                    Self::Tail => Self::Head,
                })
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedNextKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a next() override that wraps around at the tail rather than returning None",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_prev_and_composition() {
        // The well-formedness sweep's (26) clause — `v.prev()` MUST
        // equal `T::from_index(v.index_of() - 1)` on every interior
        // variant AND `T::first().prev()` MUST equal `None`. A
        // hand-impl'd implementor whose override drifts the
        // backward-neighbor projection (accepts `Some(_)` at the
        // head where the natural composition would return `None`,
        // folds a head-boundary walk onto a wraparound to the tail)
        // fails the sweep loudly rather than silently bifurcating
        // the backward-traversal surface. Sibling posture to
        // `assert_closed_set_well_formed_catches_drift_between_next_and_composition`
        // one direction over on the (forward, backward) partition of
        // clause (26).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedPrevKind {
            Head,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedPrevKind(pub String);
        impl core::fmt::Display for UnknownDriftedPrevKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted prev kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedPrevKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Tail];
            const SET_LABEL: &'static str = "drifted prev kind";
            type Unknown = UnknownDriftedPrevKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedPrevKind(s.to_owned())
            }
            fn prev(self) -> Option<Self> {
                // Drifted override — wraps around at the head rather
                // than returning `None`, folding a bounded backward
                // walk onto an infinite loop every state-machine
                // iterator consumer routes through.
                Some(match self {
                    Self::Head => Self::Tail,
                    Self::Tail => Self::Head,
                })
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedPrevKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a prev() override that wraps around at the head rather than returning None",
        );
    }

    #[test]
    fn sorted_next_walks_lex_order_forward_chain() {
        // The forward-lex-neighbor projection — `v.sorted_next()` walks
        // `T::sorted_variants()` one slot forward from each variant,
        // returning `Some` on every interior lex slot and `None` at
        // the lex-tail. `StubKind`'s labels are "alpha", "beta",
        // "gamma" so lex order aligns with declaration order and the
        // forward lex chain is Alpha → Beta → Gamma → None. Sibling
        // posture to `next_walks_declaration_order_forward_chain` one
        // ordering axis over on the (declaration, lex) partition.
        assert_eq!(
            <StubKind as ClosedSet>::sorted_next(StubKind::Alpha),
            Some(StubKind::Beta),
        );
        assert_eq!(
            <StubKind as ClosedSet>::sorted_next(StubKind::Beta),
            Some(StubKind::Gamma),
        );
        assert_eq!(<StubKind as ClosedSet>::sorted_next(StubKind::Gamma), None);
    }

    #[test]
    fn sorted_prev_walks_lex_order_backward_chain() {
        // The backward-lex-neighbor projection — `v.sorted_prev()`
        // walks `T::sorted_variants()` one slot backward from each
        // variant, returning `Some` on every interior lex slot and
        // `None` at the lex-head. On `StubKind` lex order aligns with
        // declaration order so the backward lex chain is Gamma → Beta
        // → Alpha → None. Sibling posture to
        // `prev_walks_declaration_order_backward_chain` one ordering
        // axis over on the (declaration, lex) partition.
        assert_eq!(<StubKind as ClosedSet>::sorted_prev(StubKind::Alpha), None);
        assert_eq!(
            <StubKind as ClosedSet>::sorted_prev(StubKind::Beta),
            Some(StubKind::Alpha),
        );
        assert_eq!(
            <StubKind as ClosedSet>::sorted_prev(StubKind::Gamma),
            Some(StubKind::Beta),
        );
    }

    #[test]
    fn sorted_next_and_sorted_prev_are_inverses_on_the_interior() {
        // The (forward, backward) lex-neighbor-inverse contract — for
        // every interior variant `v` (not the lex-head, not the
        // lex-tail), `v.sorted_next().unwrap().sorted_prev() ==
        // Some(v)` AND `v.sorted_prev().unwrap().sorted_next() ==
        // Some(v)`. The two projections compose the same
        // `sorted_index_of` + `from_sorted_index` bijection under
        // opposite `usize` arithmetic (`+ 1` vs `- 1`), so the
        // interior lex-neighbor edges MUST close as a round-trip. A
        // regression that returns the wrong lex-neighbor for either
        // arm would silently break this round-trip, folding a two-step
        // lex-walk `v → v.sorted_next().unwrap() → w` onto some
        // `w ≠ v`. Sibling posture to
        // `next_and_prev_are_inverses_on_the_interior` one ordering
        // axis over on the (declaration, lex) partition.
        for &v in <StubKind as ClosedSet>::ALL {
            if let Some(fwd) = <StubKind as ClosedSet>::sorted_next(v) {
                assert_eq!(
                    <StubKind as ClosedSet>::sorted_prev(fwd),
                    Some(v),
                    "{v:?}.sorted_next().unwrap().sorted_prev() did not round-trip",
                );
            }
            if let Some(bwd) = <StubKind as ClosedSet>::sorted_prev(v) {
                assert_eq!(
                    <StubKind as ClosedSet>::sorted_next(bwd),
                    Some(v),
                    "{v:?}.sorted_prev().unwrap().sorted_next() did not round-trip",
                );
            }
        }
    }

    #[test]
    fn sorted_first_sorted_prev_and_sorted_last_sorted_next_pin_the_lex_endpoint_boundary_fixpoints(
    ) {
        // The lex-endpoint-anchor / lex-neighbor-axis fixpoints —
        // `T::sorted_first().sorted_prev() == None` AND
        // `T::sorted_last().sorted_next() == None`. The two fixpoints
        // thread the lex-head-endpoint anchor back through the
        // backward-lex-neighbor axis AND the lex-tail-endpoint anchor
        // back through the forward-lex-neighbor axis at ONE
        // structural landmark each. A regression that wraps around
        // (returning `Some(T::sorted_last())` for
        // `T::sorted_first().sorted_prev()`, or `Some(T::sorted_first())`
        // for `T::sorted_last().sorted_next()`) would silently fold a
        // bounded lex-traversal onto an infinite loop every downstream
        // alphabetized-completion LSP cursor / lex-order compact-encoded
        // wire codec consumer routes through. Sibling posture to
        // `first_prev_and_last_next_pin_the_endpoint_boundary_fixpoints`
        // one ordering axis over on the (declaration, lex) partition.
        assert_eq!(
            <StubKind as ClosedSet>::sorted_prev(<StubKind as ClosedSet>::sorted_first(),),
            None,
        );
        assert_eq!(
            <StubKind as ClosedSet>::sorted_next(<StubKind as ClosedSet>::sorted_last(),),
            None,
        );
    }

    #[test]
    fn sorted_next_and_sorted_prev_on_singleton_closed_set_both_return_none() {
        // The lex-neighbor-axis degenerate case — a singleton closed
        // set (one variant) has `T::ALL[0] == T::sorted_first() ==
        // T::sorted_last()`, so BOTH `T::sorted_first().sorted_prev()
        // == None` AND `T::sorted_last().sorted_next() == None`
        // collapse onto the same variant — every lex-neighbor walk
        // from the sole variant must return `None`. A regression that
        // treats the two direction arms as structurally distinct
        // would silently break the natural degenerate-case invariant.
        // Sibling posture to
        // `next_and_prev_on_singleton_closed_set_both_return_none`
        // one ordering axis over on the (declaration, lex) partition.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum SortedNeighborSingletonKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownSortedNeighborSingletonKind(pub String);
        impl core::fmt::Display for UnknownSortedNeighborSingletonKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown sorted neighbor singleton kind: {}", self.0)
            }
        }
        impl ClosedSet for SortedNeighborSingletonKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "sorted neighbor singleton kind";
            type Unknown = UnknownSortedNeighborSingletonKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownSortedNeighborSingletonKind(s.to_owned())
            }
        }
        assert_eq!(
            <SortedNeighborSingletonKind as ClosedSet>::sorted_next(
                SortedNeighborSingletonKind::Only,
            ),
            None,
        );
        assert_eq!(
            <SortedNeighborSingletonKind as ClosedSet>::sorted_prev(
                SortedNeighborSingletonKind::Only,
            ),
            None,
        );
    }

    #[test]
    fn sorted_next_and_sorted_prev_walk_lex_order_not_declaration_order() {
        // The lex-neighbor-projection contract on an arbitrary
        // declaration order — `v.sorted_next()` MUST project the
        // LEX successor regardless of the declaration order, AND
        // `v.sorted_prev()` MUST project the LEX predecessor. A
        // regression that keyed on `T::ALL` position (rather than
        // lex-slot) would pass on `StubKind` (where declaration order
        // aligns with alphabetic order) but silently bifurcate on any
        // implementor whose `ALL`-array layout differs from its lex
        // order. Reuses the deliberately-reverse shape so the
        // lex-neighbor contract lands on the same declaration-order-
        // inversion probe the sorted-variants / lex-endpoint contracts
        // bind against. Declaration order is `[Gamma, Beta, Alpha]`
        // but lex order is `[alpha, beta, gamma]` — a `next`-based
        // (declaration-order) walk from Gamma yields Beta, but the
        // sorted_next-based (lex-order) walk from Gamma yields None
        // (Gamma is the lex-tail), and from Alpha yields Beta.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum SortedNeighborReverseKind {
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownSortedNeighborReverseKind(pub String);
        impl core::fmt::Display for UnknownSortedNeighborReverseKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown sorted neighbor reverse kind: {}", self.0)
            }
        }
        impl ClosedSet for SortedNeighborReverseKind {
            const ALL: &'static [Self] = &[Self::Gamma, Self::Beta, Self::Alpha];
            const SET_LABEL: &'static str = "sorted neighbor reverse kind";
            type Unknown = UnknownSortedNeighborReverseKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownSortedNeighborReverseKind(s.to_owned())
            }
        }
        // Lex order: [alpha, beta, gamma] — Alpha is lex-head, Gamma
        // is lex-tail. Declaration order (Gamma → Beta → Alpha) is
        // the REVERSE of the lex order, so the two neighbor pairs
        // walk in opposite directions.
        assert_eq!(
            <SortedNeighborReverseKind as ClosedSet>::sorted_next(SortedNeighborReverseKind::Alpha,),
            Some(SortedNeighborReverseKind::Beta),
        );
        assert_eq!(
            <SortedNeighborReverseKind as ClosedSet>::sorted_next(SortedNeighborReverseKind::Beta,),
            Some(SortedNeighborReverseKind::Gamma),
        );
        assert_eq!(
            <SortedNeighborReverseKind as ClosedSet>::sorted_next(SortedNeighborReverseKind::Gamma,),
            None,
        );
        assert_eq!(
            <SortedNeighborReverseKind as ClosedSet>::sorted_prev(SortedNeighborReverseKind::Alpha,),
            None,
        );
        assert_eq!(
            <SortedNeighborReverseKind as ClosedSet>::sorted_prev(SortedNeighborReverseKind::Beta,),
            Some(SortedNeighborReverseKind::Alpha),
        );
        assert_eq!(
            <SortedNeighborReverseKind as ClosedSet>::sorted_prev(SortedNeighborReverseKind::Gamma,),
            Some(SortedNeighborReverseKind::Beta),
        );
        // Cross-axis divergence pin — the declaration-axis `next`
        // and the lex-axis `sorted_next` disagree on the same
        // starting variant when `T::ALL` is not lex-sorted. Gamma is
        // declaration-order-head (index 0) so
        // `next(Gamma) == Some(Beta)`, but Gamma is lex-tail so
        // `sorted_next(Gamma) == None`. Locks the two neighbor axes
        // as structurally distinct — a regression that folded
        // `sorted_next` back onto `next` would collapse the 2×2
        // matrix onto a 1×2 partition.
        assert_eq!(
            <SortedNeighborReverseKind as ClosedSet>::next(SortedNeighborReverseKind::Gamma,),
            Some(SortedNeighborReverseKind::Beta),
        );
        assert_eq!(
            <SortedNeighborReverseKind as ClosedSet>::sorted_next(SortedNeighborReverseKind::Gamma,),
            None,
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_sorted_next_and_composition() {
        // The well-formedness sweep's (27) clause — `v.sorted_next()`
        // MUST equal `T::from_sorted_index(v.sorted_index_of() + 1)`
        // on every variant. A hand-impl'd implementor whose override
        // drifts the forward-lex-neighbor projection (accepts
        // `Some(_)` at the lex-tail where the natural composition
        // would return `None`, folds a lex-tail-boundary walk onto a
        // wraparound to the lex-head) fails the sweep loudly rather
        // than silently bifurcating the forward-lex-traversal surface
        // every downstream alphabetized-completion LSP cursor /
        // lex-order compact-encoded wire codec consumer routes
        // through. Sibling posture to
        // `assert_closed_set_well_formed_catches_drift_between_next_and_composition`
        // one ordering axis over on the (declaration, lex) partition
        // of clauses (26) + (27).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedSortedNextKind {
            Head,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedSortedNextKind(pub String);
        impl core::fmt::Display for UnknownDriftedSortedNextKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted sorted next kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedSortedNextKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Tail];
            const SET_LABEL: &'static str = "drifted sorted next kind";
            type Unknown = UnknownDriftedSortedNextKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedSortedNextKind(s.to_owned())
            }
            fn sorted_next(self) -> Option<Self> {
                // Drifted override — wraps around at the lex-tail
                // rather than returning `None`, folding a bounded
                // forward lex-walk onto an infinite loop every
                // alphabetized-completion LSP cursor consumer routes
                // through.
                Some(match self {
                    Self::Head => Self::Tail,
                    Self::Tail => Self::Head,
                })
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedSortedNextKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a sorted_next() override that wraps around at the lex-tail rather than returning None",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_sorted_prev_and_composition() {
        // The well-formedness sweep's (27) clause — `v.sorted_prev()`
        // MUST equal `T::from_sorted_index(v.sorted_index_of() - 1)`
        // on every interior variant AND
        // `T::sorted_first().sorted_prev()` MUST equal `None`. A
        // hand-impl'd implementor whose override drifts the
        // backward-lex-neighbor projection (accepts `Some(_)` at the
        // lex-head where the natural composition would return `None`,
        // folds a lex-head-boundary walk onto a wraparound to the
        // lex-tail) fails the sweep loudly rather than silently
        // bifurcating the backward-lex-traversal surface. Sibling
        // posture to
        // `assert_closed_set_well_formed_catches_drift_between_prev_and_composition`
        // one ordering axis over on the (declaration, lex) partition
        // of clauses (26) + (27).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedSortedPrevKind {
            Head,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedSortedPrevKind(pub String);
        impl core::fmt::Display for UnknownDriftedSortedPrevKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted sorted prev kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedSortedPrevKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Tail];
            const SET_LABEL: &'static str = "drifted sorted prev kind";
            type Unknown = UnknownDriftedSortedPrevKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedSortedPrevKind(s.to_owned())
            }
            fn sorted_prev(self) -> Option<Self> {
                // Drifted override — wraps around at the lex-head
                // rather than returning `None`, folding a bounded
                // backward lex-walk onto an infinite loop every
                // alphabetized-completion LSP cursor consumer routes
                // through.
                Some(match self {
                    Self::Head => Self::Tail,
                    Self::Tail => Self::Head,
                })
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedSortedPrevKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a sorted_prev() override that wraps around at the lex-head rather than returning None",
        );
    }

    #[test]
    fn cycle_next_walks_declaration_order_forward_chain_with_tail_wrap() {
        // The forward-wrapping-neighbor projection — `v.cycle_next()`
        // walks `T::ALL` one slot forward from each variant, folding
        // the tail-endpoint boundary onto the head-endpoint anchor
        // (`T::first()`) at the cyclic edge. Declaration order on
        // `StubKind` is [Alpha, Beta, Gamma] so the forward-wrapping
        // chain is Alpha → Beta → Gamma → Alpha → Beta → …, an
        // infinite cyclic walk. Sibling posture to
        // `next_walks_declaration_order_forward_chain` one return-
        // type axis over on the (Option-typed, wrapping) partition.
        assert_eq!(
            <StubKind as ClosedSet>::cycle_next(StubKind::Alpha),
            StubKind::Beta,
        );
        assert_eq!(
            <StubKind as ClosedSet>::cycle_next(StubKind::Beta),
            StubKind::Gamma,
        );
        assert_eq!(
            <StubKind as ClosedSet>::cycle_next(StubKind::Gamma),
            StubKind::Alpha,
        );
    }

    #[test]
    fn cycle_prev_walks_declaration_order_backward_chain_with_head_wrap() {
        // The backward-wrapping-neighbor projection — `v.cycle_prev()`
        // walks `T::ALL` one slot backward from each variant, folding
        // the head-endpoint boundary onto the tail-endpoint anchor
        // (`T::last()`) at the cyclic edge. Declaration order on
        // `StubKind` is [Alpha, Beta, Gamma] so the backward-wrapping
        // chain is Gamma → Beta → Alpha → Gamma → Beta → …, an
        // infinite cyclic backward walk. Sibling posture to
        // `prev_walks_declaration_order_backward_chain` one return-
        // type axis over on the (Option-typed, wrapping) partition.
        assert_eq!(
            <StubKind as ClosedSet>::cycle_prev(StubKind::Alpha),
            StubKind::Gamma,
        );
        assert_eq!(
            <StubKind as ClosedSet>::cycle_prev(StubKind::Beta),
            StubKind::Alpha,
        );
        assert_eq!(
            <StubKind as ClosedSet>::cycle_prev(StubKind::Gamma),
            StubKind::Beta,
        );
    }

    #[test]
    fn cycle_next_and_cycle_prev_are_inverses_on_every_variant() {
        // The (forward-wrap, backward-wrap) inverse contract — for
        // EVERY variant `v` (not just the interior, since the wrapping
        // arm has no bounded-neighbor `None` at the endpoint),
        // `v.cycle_next().cycle_prev() == v` AND
        // `v.cycle_prev().cycle_next() == v`. Every wrapping-neighbor
        // edge closes as a round-trip through the shared bijection —
        // a regression that returns the wrong wrapping-neighbor for
        // either arm would silently break this round-trip, folding a
        // two-step cyclic walk `v → v.cycle_next() → w` onto some
        // `w ≠ v`. Stronger contract than the bounded
        // `next_and_prev_are_inverses_on_the_interior` sibling — the
        // wrapping arm covers the endpoints too, so the inverse
        // contract holds on ALL of `T::ALL` not just the interior.
        for &v in <StubKind as ClosedSet>::ALL {
            let fwd = <StubKind as ClosedSet>::cycle_next(v);
            assert_eq!(
                <StubKind as ClosedSet>::cycle_prev(fwd),
                v,
                "{v:?}.cycle_next().cycle_prev() did not round-trip",
            );
            let bwd = <StubKind as ClosedSet>::cycle_prev(v);
            assert_eq!(
                <StubKind as ClosedSet>::cycle_next(bwd),
                v,
                "{v:?}.cycle_prev().cycle_next() did not round-trip",
            );
        }
    }

    #[test]
    fn last_cycle_next_and_first_cycle_prev_pin_the_endpoint_wraparound_fixpoints() {
        // The endpoint-anchor / wrapping-neighbor-axis fixpoints —
        // `T::last().cycle_next() == T::first()` AND
        // `T::first().cycle_prev() == T::last()`. The two fixpoints
        // thread the tail-endpoint anchor back through the forward-
        // wrapping-neighbor axis AND the head-endpoint anchor back
        // through the backward-wrapping-neighbor axis at ONE structural
        // landmark each — the wraparound edges of the cyclic chain. A
        // regression that returns some interior variant instead of the
        // opposite endpoint anchor (returning `Beta` for
        // `T::last().cycle_next()` on `StubKind`, or returning `Beta`
        // for `T::first().cycle_prev()`) would silently fold a cyclic
        // walk onto an unbounded interior loop every downstream
        // wraparound-cursor / round-robin-scheduler consumer routes
        // through. Sibling posture to
        // `first_prev_and_last_next_pin_the_endpoint_boundary_fixpoints`
        // one return-type axis over on the (Option-typed, wrapping)
        // partition — the bounded arm folds the endpoint boundary onto
        // `None`, the wrapping arm folds it onto the opposite anchor.
        assert_eq!(
            <StubKind as ClosedSet>::cycle_next(<StubKind as ClosedSet>::last()),
            <StubKind as ClosedSet>::first(),
        );
        assert_eq!(
            <StubKind as ClosedSet>::cycle_prev(<StubKind as ClosedSet>::first()),
            <StubKind as ClosedSet>::last(),
        );
    }

    #[test]
    fn cycle_next_and_cycle_prev_on_singleton_closed_set_both_return_self() {
        // The wrapping-neighbor-axis degenerate case — a singleton
        // closed set (one variant) has `T::ALL[0] == T::first() ==
        // T::last()`, so BOTH `T::last().cycle_next() == T::first()`
        // AND `T::first().cycle_prev() == T::last()` collapse onto
        // `Only.cycle_next() == Only` AND `Only.cycle_prev() == Only`
        // — every wrapping-neighbor walk from the sole variant must
        // return the sole variant. A regression that panics on the
        // degenerate case (unwrapping some other fallback that doesn't
        // exist) would break the natural degenerate-case invariant.
        // Sibling posture to
        // `next_and_prev_on_singleton_closed_set_both_return_none`
        // one return-type axis over on the (Option-typed, wrapping)
        // partition — the bounded arm returns `None` on the singleton
        // degenerate case, the wrapping arm returns the sole variant.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum CycleSingletonKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownCycleSingletonKind(pub String);
        impl core::fmt::Display for UnknownCycleSingletonKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown cycle singleton kind: {}", self.0)
            }
        }
        impl ClosedSet for CycleSingletonKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "cycle singleton kind";
            type Unknown = UnknownCycleSingletonKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownCycleSingletonKind(s.to_owned())
            }
        }
        assert_eq!(
            <CycleSingletonKind as ClosedSet>::cycle_next(CycleSingletonKind::Only),
            CycleSingletonKind::Only,
        );
        assert_eq!(
            <CycleSingletonKind as ClosedSet>::cycle_prev(CycleSingletonKind::Only),
            CycleSingletonKind::Only,
        );
        // And clause (28) holds — the well-formedness sweep passes on
        // the singleton stub through the natural composition, since
        // both `Only.next() == None` (bounded arm returns None at the
        // sole slot) AND `T::first() == T::last() == Only` collapse
        // the wraparound onto a self-fixpoint at both direction arms.
        super::assert_closed_set_well_formed::<CycleSingletonKind>();
    }

    #[test]
    fn cycle_next_and_cycle_prev_walk_declaration_order_not_lex_order() {
        // Arbitrary-declaration-order sweep — the wrapping-neighbor
        // pair keys on DECLARATION order (`T::ALL`'s layout), NOT
        // lex order. On a deliberately-reverse stub whose `T::ALL`
        // is `[Gamma, Beta, Alpha]` but whose lex order would be
        // `[alpha, beta, gamma]`, the declaration-order cyclic walk
        // is Gamma → Beta → Alpha → Gamma → …, not the alphabetic
        // Alpha → Beta → Gamma → Alpha → …. A regression that keyed
        // on lex slot (rather than `T::ALL` position) would pass on
        // `StubKind` (where declaration order aligns with alphabetic
        // order) but silently bifurcate on this reverse stub.
        // Reserves the (declaration, lex) 2×1 direction axis on the
        // wrapping partition for a future lex-axis wrapping pair.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum CycleReverseKind {
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownCycleReverseKind(pub String);
        impl core::fmt::Display for UnknownCycleReverseKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown cycle reverse kind: {}", self.0)
            }
        }
        impl ClosedSet for CycleReverseKind {
            const ALL: &'static [Self] = &[Self::Gamma, Self::Beta, Self::Alpha];
            const SET_LABEL: &'static str = "cycle reverse kind";
            type Unknown = UnknownCycleReverseKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownCycleReverseKind(s.to_owned())
            }
        }
        // Declaration-order cyclic forward walk — [Gamma, Beta, Alpha]
        // → tail-wrap back to Gamma at the Alpha edge.
        assert_eq!(
            <CycleReverseKind as ClosedSet>::cycle_next(CycleReverseKind::Gamma),
            CycleReverseKind::Beta,
        );
        assert_eq!(
            <CycleReverseKind as ClosedSet>::cycle_next(CycleReverseKind::Beta),
            CycleReverseKind::Alpha,
        );
        assert_eq!(
            <CycleReverseKind as ClosedSet>::cycle_next(CycleReverseKind::Alpha),
            CycleReverseKind::Gamma,
        );
        // Declaration-order cyclic backward walk — mirror.
        assert_eq!(
            <CycleReverseKind as ClosedSet>::cycle_prev(CycleReverseKind::Gamma),
            CycleReverseKind::Alpha,
        );
        assert_eq!(
            <CycleReverseKind as ClosedSet>::cycle_prev(CycleReverseKind::Beta),
            CycleReverseKind::Gamma,
        );
        assert_eq!(
            <CycleReverseKind as ClosedSet>::cycle_prev(CycleReverseKind::Alpha),
            CycleReverseKind::Beta,
        );
        // Clause (28) holds on the reverse stub — the well-formedness
        // sweep validates the wrapping-neighbor pair composes through
        // the natural `next().unwrap_or(first())` /
        // `prev().unwrap_or(last())` shape on every variant even when
        // declaration order and lex order diverge.
        super::assert_closed_set_well_formed::<CycleReverseKind>();
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_cycle_next_and_composition() {
        // The well-formedness sweep's (28) clause — `v.cycle_next()`
        // MUST equal `v.next().unwrap_or(T::first())` on every
        // variant. A hand-impl'd implementor whose override drifts
        // the forward-wrapping-neighbor projection (returns some
        // interior variant at the tail rather than the head anchor,
        // folding a cyclic walk onto an unbounded interior loop
        // rather than folding the tail-endpoint boundary onto
        // `T::first()`) fails the sweep loudly rather than silently
        // bifurcating the forward-wrapping-traversal surface every
        // downstream wraparound-cursor LSP completion renderer /
        // round-robin scheduler / carousel widget consumer routes
        // through. Sibling posture to
        // `assert_closed_set_well_formed_catches_drift_between_next_and_composition`
        // one return-type axis over on the (Option-typed, wrapping)
        // partition of clauses (26) + (28).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedCycleNextKind {
            Head,
            Middle,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedCycleNextKind(pub String);
        impl core::fmt::Display for UnknownDriftedCycleNextKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted cycle next kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedCycleNextKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Middle, Self::Tail];
            const SET_LABEL: &'static str = "drifted cycle next kind";
            type Unknown = UnknownDriftedCycleNextKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Middle => "middle",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedCycleNextKind(s.to_owned())
            }
            fn cycle_next(self) -> Self {
                // Drifted override — folds the tail onto Middle
                // rather than Head, silently bifurcating the
                // wraparound edge every round-robin scheduler
                // consumer routes through.
                match self {
                    Self::Head => Self::Middle,
                    Self::Middle => Self::Tail,
                    Self::Tail => Self::Middle,
                }
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedCycleNextKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a cycle_next() override that folds the tail onto an interior variant rather than T::first()",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_cycle_prev_and_composition() {
        // The well-formedness sweep's (28) clause — `v.cycle_prev()`
        // MUST equal `v.prev().unwrap_or(T::last())` on every
        // variant. A hand-impl'd implementor whose override drifts
        // the backward-wrapping-neighbor projection (returns some
        // interior variant at the head rather than the tail anchor,
        // folding a cyclic backward walk onto an unbounded interior
        // loop rather than folding the head-endpoint boundary onto
        // `T::last()`) fails the sweep loudly rather than silently
        // bifurcating the backward-wrapping-traversal surface.
        // Sibling posture to
        // `assert_closed_set_well_formed_catches_drift_between_prev_and_composition`
        // one return-type axis over on the (Option-typed, wrapping)
        // partition of clauses (26) + (28).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedCyclePrevKind {
            Head,
            Middle,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedCyclePrevKind(pub String);
        impl core::fmt::Display for UnknownDriftedCyclePrevKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted cycle prev kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedCyclePrevKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Middle, Self::Tail];
            const SET_LABEL: &'static str = "drifted cycle prev kind";
            type Unknown = UnknownDriftedCyclePrevKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Middle => "middle",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedCyclePrevKind(s.to_owned())
            }
            fn cycle_prev(self) -> Self {
                // Drifted override — folds the head onto Middle
                // rather than Tail, silently bifurcating the
                // wraparound edge every round-robin scheduler
                // consumer routes through.
                match self {
                    Self::Head => Self::Middle,
                    Self::Middle => Self::Head,
                    Self::Tail => Self::Middle,
                }
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedCyclePrevKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a cycle_prev() override that folds the head onto an interior variant rather than T::last()",
        );
    }

    #[test]
    fn cycle_sorted_next_walks_lex_order_forward_chain_with_lex_tail_wrap() {
        // The forward-wrapping-lex-neighbor projection —
        // `v.cycle_sorted_next()` walks `T::sorted_variants()` one lex
        // slot forward from each variant, folding the lex-tail-endpoint
        // boundary onto the lex-head-endpoint anchor (`T::sorted_first()`)
        // at the cyclic edge. `StubKind`'s declaration order aligns with
        // lex order ([Alpha, Beta, Gamma] and ["alpha", "beta", "gamma"]),
        // so the forward-wrapping-lex chain reads Alpha → Beta → Gamma →
        // Alpha → …, an infinite lex-cyclic walk. Sibling posture to
        // `cycle_next_walks_declaration_order_forward_chain_with_tail_wrap`
        // one ordering axis over on the (declaration, lex) partition —
        // the aligned-ordering stub coincidentally makes both chains
        // read the same, and the divergent-ordering
        // `cycle_sorted_next_and_cycle_sorted_prev_walk_lex_order_not_declaration_order`
        // sweep below pins the lex-axis binding on a stub whose two
        // orderings intentionally diverge.
        assert_eq!(
            <StubKind as ClosedSet>::cycle_sorted_next(StubKind::Alpha),
            StubKind::Beta,
        );
        assert_eq!(
            <StubKind as ClosedSet>::cycle_sorted_next(StubKind::Beta),
            StubKind::Gamma,
        );
        assert_eq!(
            <StubKind as ClosedSet>::cycle_sorted_next(StubKind::Gamma),
            StubKind::Alpha,
        );
    }

    #[test]
    fn cycle_sorted_prev_walks_lex_order_backward_chain_with_lex_head_wrap() {
        // The backward-wrapping-lex-neighbor projection —
        // `v.cycle_sorted_prev()` walks `T::sorted_variants()` one lex
        // slot backward from each variant, folding the lex-head-endpoint
        // boundary onto the lex-tail-endpoint anchor (`T::sorted_last()`)
        // at the cyclic edge. On `StubKind` (declaration order aligns
        // with lex order) the backward-wrapping-lex chain reads
        // Gamma → Beta → Alpha → Gamma → …, an infinite lex-cyclic
        // backward walk. Sibling posture to
        // `cycle_prev_walks_declaration_order_backward_chain_with_head_wrap`
        // one ordering axis over on the (declaration, lex) partition.
        assert_eq!(
            <StubKind as ClosedSet>::cycle_sorted_prev(StubKind::Alpha),
            StubKind::Gamma,
        );
        assert_eq!(
            <StubKind as ClosedSet>::cycle_sorted_prev(StubKind::Beta),
            StubKind::Alpha,
        );
        assert_eq!(
            <StubKind as ClosedSet>::cycle_sorted_prev(StubKind::Gamma),
            StubKind::Beta,
        );
    }

    #[test]
    fn cycle_sorted_next_and_cycle_sorted_prev_are_inverses_on_every_variant() {
        // The (forward-lex-wrap, backward-lex-wrap) inverse contract —
        // for EVERY variant `v` (not just the interior, since the
        // wrapping arm has no bounded-neighbor `None` at the lex
        // endpoint), `v.cycle_sorted_next().cycle_sorted_prev() == v`
        // AND `v.cycle_sorted_prev().cycle_sorted_next() == v`. Every
        // wrapping-lex-neighbor edge closes as a round-trip through the
        // shared bijection — a regression that returns the wrong
        // wrapping-lex-neighbor for either arm would silently break this
        // round-trip, folding a two-step lex-cyclic walk `v →
        // v.cycle_sorted_next() → w` onto some `w ≠ v`. Stronger
        // contract than the bounded
        // `sorted_next_and_sorted_prev_are_inverses_on_the_interior`
        // sibling — the wrapping arm covers the lex endpoints too, so
        // the inverse contract holds on ALL of `T::ALL` not just the
        // interior. Sibling posture to
        // `cycle_next_and_cycle_prev_are_inverses_on_every_variant` one
        // ordering axis over.
        for &v in <StubKind as ClosedSet>::ALL {
            let fwd = <StubKind as ClosedSet>::cycle_sorted_next(v);
            assert_eq!(
                <StubKind as ClosedSet>::cycle_sorted_prev(fwd),
                v,
                "{v:?}.cycle_sorted_next().cycle_sorted_prev() did not round-trip",
            );
            let bwd = <StubKind as ClosedSet>::cycle_sorted_prev(v);
            assert_eq!(
                <StubKind as ClosedSet>::cycle_sorted_next(bwd),
                v,
                "{v:?}.cycle_sorted_prev().cycle_sorted_next() did not round-trip",
            );
        }
    }

    #[test]
    fn sorted_last_cycle_sorted_next_and_sorted_first_cycle_sorted_prev_pin_the_lex_endpoint_wraparound_fixpoints(
    ) {
        // The lex-endpoint-anchor / wrapping-lex-neighbor-axis fixpoints
        // — `T::sorted_last().cycle_sorted_next() == T::sorted_first()`
        // AND `T::sorted_first().cycle_sorted_prev() == T::sorted_last()`.
        // The two fixpoints thread the lex-tail-endpoint anchor back
        // through the forward-wrapping-lex-neighbor axis AND the
        // lex-head-endpoint anchor back through the backward-wrapping-
        // lex-neighbor axis at ONE structural landmark each — the
        // lex-wraparound edges of the lex-cyclic chain. A regression
        // that returns some interior variant instead of the opposite
        // lex-endpoint anchor (returning `Beta` for
        // `T::sorted_last().cycle_sorted_next()` on `StubKind`, or
        // returning `Beta` for `T::sorted_first().cycle_sorted_prev()`)
        // would silently fold a lex-cyclic walk onto an unbounded
        // interior loop every downstream alphabetized wraparound-cursor
        // / alphabetized round-robin-scheduler consumer routes through.
        // Sibling posture to
        // `last_cycle_next_and_first_cycle_prev_pin_the_endpoint_wraparound_fixpoints`
        // one ordering axis over on the (declaration, lex) partition of
        // the closed-set wrapping-neighbor endpoint-fixpoint surface —
        // the declaration-axis wraparound folds `T::last()` onto
        // `T::first()` at the tail edge AND `T::first()` onto `T::last()`
        // at the head edge; the lex-axis wraparound folds
        // `T::sorted_last()` onto `T::sorted_first()` at the lex-tail
        // edge AND `T::sorted_first()` onto `T::sorted_last()` at the
        // lex-head edge, with the SAME structural fold shape on both
        // ordering axes.
        assert_eq!(
            <StubKind as ClosedSet>::cycle_sorted_next(<StubKind as ClosedSet>::sorted_last(),),
            <StubKind as ClosedSet>::sorted_first(),
        );
        assert_eq!(
            <StubKind as ClosedSet>::cycle_sorted_prev(<StubKind as ClosedSet>::sorted_first(),),
            <StubKind as ClosedSet>::sorted_last(),
        );
    }

    #[test]
    fn cycle_sorted_next_and_cycle_sorted_prev_on_singleton_closed_set_both_return_self() {
        // The wrapping-lex-neighbor-axis degenerate case — a singleton
        // closed set (one variant) has `T::ALL[0] == T::first() ==
        // T::last() == T::sorted_first() == T::sorted_last()`, so BOTH
        // `T::sorted_last().cycle_sorted_next() == T::sorted_first()`
        // AND `T::sorted_first().cycle_sorted_prev() == T::sorted_last()`
        // collapse onto `Only.cycle_sorted_next() == Only` AND
        // `Only.cycle_sorted_prev() == Only` — every wrapping-lex-
        // neighbor walk from the sole variant must return the sole
        // variant. A regression that panics on the degenerate case
        // (unwrapping some other fallback that doesn't exist) would
        // break the natural degenerate-case invariant. Sibling posture
        // to `cycle_next_and_cycle_prev_on_singleton_closed_set_both_return_self`
        // one ordering axis over on the (declaration, lex) partition —
        // both wrapping-neighbor arms collapse identically on the
        // singleton degenerate case.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum CycleSortedSingletonKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownCycleSortedSingletonKind(pub String);
        impl core::fmt::Display for UnknownCycleSortedSingletonKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown cycle sorted singleton kind: {}", self.0)
            }
        }
        impl ClosedSet for CycleSortedSingletonKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "cycle sorted singleton kind";
            type Unknown = UnknownCycleSortedSingletonKind;
            fn label(self) -> &'static str {
                "only"
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownCycleSortedSingletonKind(s.to_owned())
            }
        }
        assert_eq!(
            <CycleSortedSingletonKind as ClosedSet>::cycle_sorted_next(
                CycleSortedSingletonKind::Only,
            ),
            CycleSortedSingletonKind::Only,
        );
        assert_eq!(
            <CycleSortedSingletonKind as ClosedSet>::cycle_sorted_prev(
                CycleSortedSingletonKind::Only,
            ),
            CycleSortedSingletonKind::Only,
        );
        // And clause (29) holds — the well-formedness sweep passes on
        // the singleton stub through the natural composition, since
        // both `Only.sorted_next() == None` (bounded lex arm returns
        // None at the sole slot) AND `T::sorted_first() ==
        // T::sorted_last() == Only` collapse the lex-wraparound onto a
        // self-fixpoint at both direction arms.
        super::assert_closed_set_well_formed::<CycleSortedSingletonKind>();
    }

    #[test]
    fn cycle_sorted_next_and_cycle_sorted_prev_walk_lex_order_not_declaration_order() {
        // Arbitrary-declaration-order sweep — the wrapping-lex-neighbor
        // pair keys on LEX order (`T::sorted_variants()`'s layout), NOT
        // declaration order. On a deliberately-reverse stub whose
        // `T::ALL` is `[Gamma, Beta, Alpha]` but whose lex order is
        // `[Alpha, Beta, Gamma]`, the lex-cyclic forward walk reads
        // Alpha → Beta → Gamma → Alpha → …, NOT the declaration-order
        // Gamma → Beta → Alpha → Gamma → …. A regression that keyed on
        // declaration slot (rather than lex slot) would pass on
        // `StubKind` (where declaration order aligns with lex order)
        // but silently bifurcate on this reverse stub. Sibling posture
        // to `cycle_next_and_cycle_prev_walk_declaration_order_not_lex_order`
        // one ordering axis over — that sweep pinned the declaration-
        // axis binding by diverging from lex order, this sweep pins the
        // lex-axis binding by diverging from declaration order, closing
        // the (declaration, lex) 2×1 direction axis on the wrapping
        // partition at BOTH ordering arms.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum CycleSortedReverseKind {
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownCycleSortedReverseKind(pub String);
        impl core::fmt::Display for UnknownCycleSortedReverseKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown cycle sorted reverse kind: {}", self.0)
            }
        }
        impl ClosedSet for CycleSortedReverseKind {
            const ALL: &'static [Self] = &[Self::Gamma, Self::Beta, Self::Alpha];
            const SET_LABEL: &'static str = "cycle sorted reverse kind";
            type Unknown = UnknownCycleSortedReverseKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownCycleSortedReverseKind(s.to_owned())
            }
        }
        // Lex-order cyclic forward walk — [Alpha, Beta, Gamma] under
        // lex ordering → lex-tail-wrap back to Alpha at the Gamma edge.
        assert_eq!(
            <CycleSortedReverseKind as ClosedSet>::cycle_sorted_next(CycleSortedReverseKind::Alpha,),
            CycleSortedReverseKind::Beta,
        );
        assert_eq!(
            <CycleSortedReverseKind as ClosedSet>::cycle_sorted_next(CycleSortedReverseKind::Beta,),
            CycleSortedReverseKind::Gamma,
        );
        assert_eq!(
            <CycleSortedReverseKind as ClosedSet>::cycle_sorted_next(CycleSortedReverseKind::Gamma,),
            CycleSortedReverseKind::Alpha,
        );
        // Lex-order cyclic backward walk — mirror.
        assert_eq!(
            <CycleSortedReverseKind as ClosedSet>::cycle_sorted_prev(CycleSortedReverseKind::Alpha,),
            CycleSortedReverseKind::Gamma,
        );
        assert_eq!(
            <CycleSortedReverseKind as ClosedSet>::cycle_sorted_prev(CycleSortedReverseKind::Beta,),
            CycleSortedReverseKind::Alpha,
        );
        assert_eq!(
            <CycleSortedReverseKind as ClosedSet>::cycle_sorted_prev(CycleSortedReverseKind::Gamma,),
            CycleSortedReverseKind::Beta,
        );
        // Clause (29) holds on the reverse stub — the well-formedness
        // sweep validates the wrapping-lex-neighbor pair composes
        // through the natural `sorted_next().unwrap_or(sorted_first())`
        // / `sorted_prev().unwrap_or(sorted_last())` shape on every
        // variant even when declaration order and lex order diverge.
        super::assert_closed_set_well_formed::<CycleSortedReverseKind>();
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_cycle_sorted_next_and_composition() {
        // The well-formedness sweep's (29) clause —
        // `v.cycle_sorted_next()` MUST equal
        // `v.sorted_next().unwrap_or(T::sorted_first())` on every
        // variant. A hand-impl'd implementor whose override drifts the
        // forward-wrapping-lex-neighbor projection (returns some
        // interior variant at the lex tail rather than the lex-head
        // anchor, folding a lex-cyclic walk onto an unbounded interior
        // loop rather than folding the lex-tail-endpoint boundary onto
        // `T::sorted_first()`) fails the sweep loudly rather than
        // silently bifurcating the forward-wrapping-lex-traversal
        // surface every downstream alphabetized wraparound-cursor LSP
        // completion renderer / alphabetized round-robin scheduler /
        // lex-order carousel widget consumer routes through. Sibling
        // posture to
        // `assert_closed_set_well_formed_catches_drift_between_cycle_next_and_composition`
        // one ordering axis over on the (declaration, lex) partition of
        // clauses (28) + (29).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedCycleSortedNextKind {
            Head,
            Middle,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedCycleSortedNextKind(pub String);
        impl core::fmt::Display for UnknownDriftedCycleSortedNextKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted cycle sorted next kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedCycleSortedNextKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Middle, Self::Tail];
            const SET_LABEL: &'static str = "drifted cycle sorted next kind";
            type Unknown = UnknownDriftedCycleSortedNextKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Middle => "middle",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedCycleSortedNextKind(s.to_owned())
            }
            fn cycle_sorted_next(self) -> Self {
                // Drifted override — folds the lex tail onto Middle
                // rather than the lex-head anchor, silently
                // bifurcating the lex-wraparound edge every
                // alphabetized round-robin scheduler consumer routes
                // through. Lex order on this stub is `head` < `middle`
                // < `tail` (labels aligned with declaration order), so
                // the intended cycle would fold `tail` onto `head`.
                match self {
                    Self::Head => Self::Middle,
                    Self::Middle => Self::Tail,
                    Self::Tail => Self::Middle,
                }
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedCycleSortedNextKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a cycle_sorted_next() override that folds the lex tail onto an interior variant rather than T::sorted_first()",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_cycle_sorted_prev_and_composition() {
        // The well-formedness sweep's (29) clause —
        // `v.cycle_sorted_prev()` MUST equal
        // `v.sorted_prev().unwrap_or(T::sorted_last())` on every
        // variant. A hand-impl'd implementor whose override drifts the
        // backward-wrapping-lex-neighbor projection (returns some
        // interior variant at the lex head rather than the lex-tail
        // anchor, folding a lex-cyclic backward walk onto an unbounded
        // interior loop rather than folding the lex-head-endpoint
        // boundary onto `T::sorted_last()`) fails the sweep loudly
        // rather than silently bifurcating the backward-wrapping-lex-
        // traversal surface. Sibling posture to
        // `assert_closed_set_well_formed_catches_drift_between_cycle_prev_and_composition`
        // one ordering axis over on the (declaration, lex) partition of
        // clauses (28) + (29).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedCycleSortedPrevKind {
            Head,
            Middle,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedCycleSortedPrevKind(pub String);
        impl core::fmt::Display for UnknownDriftedCycleSortedPrevKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted cycle sorted prev kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedCycleSortedPrevKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Middle, Self::Tail];
            const SET_LABEL: &'static str = "drifted cycle sorted prev kind";
            type Unknown = UnknownDriftedCycleSortedPrevKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Middle => "middle",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedCycleSortedPrevKind(s.to_owned())
            }
            fn cycle_sorted_prev(self) -> Self {
                // Drifted override — folds the lex head onto Middle
                // rather than the lex-tail anchor, silently
                // bifurcating the lex-wraparound edge every
                // alphabetized round-robin scheduler consumer routes
                // through. Lex order on this stub is `head` < `middle`
                // < `tail`, so the intended cycle would fold `head`
                // onto `tail`.
                match self {
                    Self::Head => Self::Middle,
                    Self::Middle => Self::Head,
                    Self::Tail => Self::Middle,
                }
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedCycleSortedPrevKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a cycle_sorted_prev() override that folds the lex head onto an interior variant rather than T::sorted_last()",
        );
    }

    #[test]
    fn endpoints_returns_declaration_order_head_and_tail_as_a_tuple() {
        // The declaration-order pair-aggregation projection returns a
        // tuple of the (head, tail) scalar endpoint anchors — the
        // slot-0 index projects `T::first()` (the declaration head),
        // the slot-1 index projects `T::last()` (the declaration
        // tail). `StubKind`'s declaration order is `[Alpha, Beta,
        // Gamma]`, so `T::endpoints()` returns `(Alpha, Gamma)`.
        // Sibling posture to
        // `first_returns_the_declaration_order_head_variant` +
        // `last_returns_the_declaration_order_tail_variant` one
        // return-shape axis over on the (scalar, pair) partition of
        // the closed-set declaration-order anchor surface — the pair
        // arm aggregates the two scalar arms into ONE tuple call.
        assert_eq!(
            <StubKind as ClosedSet>::endpoints(),
            (StubKind::Alpha, StubKind::Gamma),
        );
    }

    #[test]
    fn sorted_endpoints_returns_lex_order_head_and_tail_as_a_tuple() {
        // The lex-order pair-aggregation projection returns a tuple
        // of the (lex-head, lex-tail) scalar lex-endpoint anchors.
        // `StubKind`'s canonical labels are `("alpha", "beta",
        // "gamma")` — the declaration ordering matches the lex
        // ordering here, so `T::sorted_endpoints()` returns
        // `(Alpha, Gamma)` (lex head = `Alpha`, lex tail = `Gamma`).
        // Sibling posture to
        // `endpoints_returns_declaration_order_head_and_tail_as_a_tuple`
        // one ordering axis over on the (declaration, lex) partition
        // of the closed-set endpoint-anchor pair-aggregation surface.
        assert_eq!(
            <StubKind as ClosedSet>::sorted_endpoints(),
            (StubKind::Alpha, StubKind::Gamma),
        );
    }

    #[test]
    fn endpoints_and_sorted_endpoints_diverge_on_declaration_order_that_diverges_from_lex_order() {
        // The (declaration-axis, lex-axis) pair-aggregation contract
        // on a stub whose declaration order deliberately diverges
        // from the lex order — regardless of which variants happen to
        // sit at declaration-slice-index-0 / declaration-slice-index-
        // (N - 1), `T::endpoints()` folds the DECLARATION endpoints
        // into its tuple and `T::sorted_endpoints()` folds the LEX
        // endpoints into its tuple. A regression that hard-coded
        // `sorted_endpoints` against declaration-slice indices rather
        // than routing through the lex-endpoint-anchor primitives
        // would pass `endpoints_and_sorted_endpoints` on `StubKind`
        // (where declaration ordering matches lex ordering) and
        // silently bifurcate the pair-aggregations on any implementor
        // whose declaration order diverges from its lex order. The
        // deliberate 5-variant stub has declaration order
        // `[Epsilon, Delta, Gamma, Beta, Alpha]` and labels
        // `("epsilon", "delta", "gamma", "beta", "alpha")`. The lex
        // ordering of the labels is `"alpha" < "beta" < "delta" <
        // "epsilon" < "gamma"`, so the lex-endpoints are `Alpha`
        // (lex head) + `Gamma` (lex tail) while the declaration-
        // endpoints are `Epsilon` (declaration head) + `Alpha`
        // (declaration tail). `T::endpoints() == (Epsilon, Alpha)`
        // and `T::sorted_endpoints() == (Alpha, Gamma)` — the two
        // pair-aggregations share ONE variant (`Alpha`) but swap its
        // role (declaration tail vs lex head) AND diverge on the
        // OTHER tuple slot (`Epsilon` vs `Gamma`). Sibling posture to
        // `is_sorted_endpoint_and_is_sorted_interior_partition_all_slice_on_arbitrary_declaration_and_lex_order`
        // one return-shape axis over on the (bool boundary, pair
        // aggregation) partition of the ordering-divergent stub
        // surface.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum EndpointsPairAggregationStubKind {
            Epsilon,
            Delta,
            Gamma,
            Beta,
            Alpha,
        }
        #[derive(Debug)]
        struct UnknownEndpointsPairAggregationStubKind(pub String);
        impl core::fmt::Display for UnknownEndpointsPairAggregationStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(
                    f,
                    "unknown endpoints pair aggregation stub kind: {}",
                    self.0
                )
            }
        }
        impl ClosedSet for EndpointsPairAggregationStubKind {
            const ALL: &'static [Self] = &[
                Self::Epsilon,
                Self::Delta,
                Self::Gamma,
                Self::Beta,
                Self::Alpha,
            ];
            const SET_LABEL: &'static str = "endpoints pair aggregation stub kind";
            type Unknown = UnknownEndpointsPairAggregationStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Epsilon => "epsilon",
                    Self::Delta => "delta",
                    Self::Gamma => "gamma",
                    Self::Beta => "beta",
                    Self::Alpha => "alpha",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownEndpointsPairAggregationStubKind(s.to_owned())
            }
        }
        assert_eq!(
            <EndpointsPairAggregationStubKind as ClosedSet>::endpoints(),
            (
                EndpointsPairAggregationStubKind::Epsilon,
                EndpointsPairAggregationStubKind::Alpha,
            ),
            "endpoints() drifted from (T::first(), T::last()) on a declaration-order-divergent stub — the declaration head is Epsilon and the declaration tail is Alpha",
        );
        assert_eq!(
            <EndpointsPairAggregationStubKind as ClosedSet>::sorted_endpoints(),
            (
                EndpointsPairAggregationStubKind::Alpha,
                EndpointsPairAggregationStubKind::Gamma,
            ),
            "sorted_endpoints() drifted from (T::sorted_first(), T::sorted_last()) on a lex-order-divergent stub — the lex head is Alpha and the lex tail is Gamma",
        );
        // The two pair-aggregations diverge on both tuple slots on
        // this divergent stub — the (declaration, lex) partition is
        // structurally observed.
        assert_ne!(
            <EndpointsPairAggregationStubKind as ClosedSet>::endpoints(),
            <EndpointsPairAggregationStubKind as ClosedSet>::sorted_endpoints(),
            "endpoints() and sorted_endpoints() returned the SAME tuple on a stub whose declaration order deliberately diverges from its lex order — the (declaration, lex) ordering partition MUST be structurally observed by the two pair-aggregation surfaces",
        );
        // The stub also satisfies the well-formedness sweep — clauses
        // (34) + (35) both fire on a declaration-order that diverges
        // from the lex order, pinning the (declaration-axis) pair
        // aggregation on the declaration endpoints (Epsilon, Alpha)
        // AND the (lex-axis) pair aggregation on the lex endpoints
        // (Alpha, Gamma).
        super::assert_closed_set_well_formed::<EndpointsPairAggregationStubKind>();
    }

    #[test]
    fn endpoints_and_sorted_endpoints_collapse_on_singleton_closed_set() {
        // The pair-aggregation degenerate case — a singleton closed
        // set has ONE variant that is BOTH `T::first()` and
        // `T::last()` (and BOTH `T::sorted_first()` and
        // `T::sorted_last()`), so `T::endpoints()` and
        // `T::sorted_endpoints()` both return
        // `(Self::Only, Self::Only)`. The pair aggregation preserves
        // the tuple SHAPE even at the boundary-cardinality edge
        // where the two SLOTS collapse onto the same value. Mirrors
        // `is_endpoint_and_is_interior_collapse_on_singleton_closed_set`
        // + `is_sorted_endpoint_and_is_sorted_interior_collapse_on_singleton_closed_set`
        // one return-shape axis over on the (bool boundary, pair
        // aggregation) partition of the singleton stub surface.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum SingletonEndpointsStubKind {
            Only,
        }
        #[derive(Debug)]
        struct UnknownSingletonEndpointsStubKind(pub String);
        impl core::fmt::Display for UnknownSingletonEndpointsStubKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown singleton endpoints stub kind: {}", self.0)
            }
        }
        impl ClosedSet for SingletonEndpointsStubKind {
            const ALL: &'static [Self] = &[Self::Only];
            const SET_LABEL: &'static str = "singleton endpoints stub kind";
            type Unknown = UnknownSingletonEndpointsStubKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Only => "only",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownSingletonEndpointsStubKind(s.to_owned())
            }
        }
        assert_eq!(
            <SingletonEndpointsStubKind as ClosedSet>::endpoints(),
            (
                SingletonEndpointsStubKind::Only,
                SingletonEndpointsStubKind::Only,
            ),
        );
        assert_eq!(
            <SingletonEndpointsStubKind as ClosedSet>::sorted_endpoints(),
            (
                SingletonEndpointsStubKind::Only,
                SingletonEndpointsStubKind::Only,
            ),
        );
        // Singleton collapse — the declaration-axis and lex-axis
        // pair-aggregations fold onto the SAME diagonal tuple.
        assert_eq!(
            <SingletonEndpointsStubKind as ClosedSet>::endpoints(),
            <SingletonEndpointsStubKind as ClosedSet>::sorted_endpoints(),
        );
        super::assert_closed_set_well_formed::<SingletonEndpointsStubKind>();
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_endpoints_and_first_last() {
        // The well-formedness sweep's (34) clause —
        // `T::endpoints()` MUST equal `(T::first(), T::last())`. A
        // hand-impl'd implementor whose override drifts the pair
        // aggregation (swaps the tuple slots, folds the tail onto the
        // head, fabricates a strictly-interior slot into either
        // tuple slot) fails the sweep loudly rather than silently
        // bifurcating the declaration-axis pair-aggregation surface
        // every downstream boundary-badge renderer / range-walker
        // destructure / saga-step audit-event emitter / per-
        // implementor coherence probe consumer routes through.
        // Sibling posture to
        // `assert_closed_set_well_formed_catches_drift_between_labels_and_all_projection`
        // one return-shape axis over on the (Vec<Self> collection,
        // (Self, Self) pair) partition of the closed-set endpoint-
        // anchor return-shape surface.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedEndpointsKind {
            Head,
            Middle,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedEndpointsKind(pub String);
        impl core::fmt::Display for UnknownDriftedEndpointsKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted endpoints kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedEndpointsKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Middle, Self::Tail];
            const SET_LABEL: &'static str = "drifted endpoints kind";
            type Unknown = UnknownDriftedEndpointsKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Middle => "middle",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedEndpointsKind(s.to_owned())
            }
            fn endpoints() -> (Self, Self) {
                // Drifted override — swaps the (head, tail) tuple
                // slots and silently inverts the pair-aggregation
                // semantics every downstream boundary-badge renderer /
                // range-walker destructure / saga-step audit-event
                // emitter consumer routes through.
                (Self::Tail, Self::Head)
            }
        }
        let outcome =
            std::panic::catch_unwind(super::assert_closed_set_well_formed::<DriftedEndpointsKind>);
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted an endpoints() override that swaps the (head, tail) tuple slots rather than composing (T::first(), T::last())",
        );
    }

    #[test]
    fn assert_closed_set_well_formed_catches_drift_between_sorted_endpoints_and_sorted_first_last()
    {
        // The well-formedness sweep's (35) clause —
        // `T::sorted_endpoints()` MUST equal
        // `(T::sorted_first(), T::sorted_last())`. A hand-impl'd
        // implementor whose override drifts the lex-pair aggregation
        // (folds both lex-tuple slots onto the lex-head-endpoint
        // anchor, fabricates a strictly-lex-interior slot into
        // either tuple slot, folds the declaration-axis pair onto
        // the lex-pair-aggregation surface) fails the sweep loudly
        // rather than silently bifurcating the lex-axis pair-
        // aggregation surface. Sibling posture to
        // `assert_closed_set_well_formed_catches_drift_between_endpoints_and_first_last`
        // one ordering axis over on the (declaration, lex) partition
        // of clauses (34) + (35).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum DriftedSortedEndpointsKind {
            Head,
            Middle,
            Tail,
        }
        #[derive(Debug)]
        struct UnknownDriftedSortedEndpointsKind(pub String);
        impl core::fmt::Display for UnknownDriftedSortedEndpointsKind {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "unknown drifted sorted endpoints kind: {}", self.0)
            }
        }
        impl ClosedSet for DriftedSortedEndpointsKind {
            const ALL: &'static [Self] = &[Self::Head, Self::Middle, Self::Tail];
            const SET_LABEL: &'static str = "drifted sorted endpoints kind";
            type Unknown = UnknownDriftedSortedEndpointsKind;
            fn label(self) -> &'static str {
                match self {
                    Self::Head => "head",
                    Self::Middle => "middle",
                    Self::Tail => "tail",
                }
            }
            fn make_unknown(s: &str) -> Self::Unknown {
                UnknownDriftedSortedEndpointsKind(s.to_owned())
            }
            fn sorted_endpoints() -> (Self, Self) {
                // Drifted override — folds the lex tail onto Middle
                // rather than the lex-tail anchor Tail, silently
                // bifurcating the lex-pair-aggregation surface every
                // downstream alphabetized-boundary-badge renderer /
                // alphabetized-range-walker destructure consumer
                // routes through. Lex order on this stub is
                // `head` < `middle` < `tail`, so the intended pair
                // is `(Head, Tail)`.
                (Self::Head, Self::Middle)
            }
        }
        let outcome = std::panic::catch_unwind(
            super::assert_closed_set_well_formed::<DriftedSortedEndpointsKind>,
        );
        assert!(
            outcome.is_err(),
            "assert_closed_set_well_formed accepted a sorted_endpoints() override that folds the lex tail onto an interior variant rather than composing (T::sorted_first(), T::sorted_last())",
        );
    }
}
