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
}
