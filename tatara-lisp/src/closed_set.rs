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
        Self::ALL
            .iter()
            .copied()
            .any(|v| <Self as ClosedSet>::label(v) == s)
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
}
