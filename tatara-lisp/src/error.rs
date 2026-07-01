use thiserror::Error;

pub type Result<T> = std::result::Result<T, LispError>;

#[derive(Debug, Error)]
pub enum LispError {
    #[error("unexpected character {0:?} at position {1}")]
    UnexpectedChar(char, usize),
    #[error("unterminated string literal at position {0}")]
    UnterminatedString(usize),
    #[error("unmatched closing paren at position {pos}")]
    UnmatchedParen { pos: usize },
    #[error("unmatched opening paren at position {pos}")]
    UnmatchedOpenParen { pos: usize },
    #[error("unexpected end of input at position {pos}")]
    Eof { pos: usize },
    #[error("invalid number literal {0:?}")]
    InvalidNumber(String),
    #[error("unknown symbol: {0}")]
    UnknownSymbol(String),
    #[error("type error: expected {expected}, got {got}")]
    Type { expected: &'static str, got: String },
    #[error("compile error in {form}: {message}")]
    Compile { form: String, message: String },
    /// Structured type mismatch — both sides are first-class fields, not
    /// embedded substrings of `message`. Rendered as `"compile error in
    /// {form}: expected {expected}, got {got}"` so the user-facing string
    /// matches the legacy `Compile`-shaped diagnostic byte-for-byte; the
    /// gain is structural — authoring tools (REPL, LSP, `tatara-check`)
    /// pattern-match on the variant and bind directly to `expected` /
    /// `got` instead of substring-parsing the rendered message.
    ///
    /// `form` is the typed closed-set `KwargPath` enum so consumers
    /// pattern-match on path-shape identity (`KwargPath::Item { .. }`,
    /// `KwargPath::Slot(_)`, `KwargPath::Named(_)`) directly rather than
    /// substring-matching the rendered prefix. The Display projection
    /// flows through `KwargPath::Display`, so the user-facing
    /// `"compile error in {form}: …"` rendering matches the legacy
    /// `String`-shaped diagnostic byte-for-byte.
    ///
    /// `expected` is the typed closed-set `ExpectedKwargShape` enum —
    /// the seven reachable expected-shape labels the typed-entry kwarg
    /// gate emits (`Keyword` ⊎ `String` ⊎ `Int` ⊎ `Number` ⊎ `Bool` ⊎
    /// `List` ⊎ `ListOfStrings`) encoded as a TYPE so a typo in any
    /// label literal can never drift into the diagnostic at runtime;
    /// consumers (REPL, LSP, `tatara-check`) pattern-match on
    /// `ExpectedKwargShape::Number` etc. directly instead of
    /// substring-matching `expected == "number"`. Same posture as
    /// `LispError::Defmacro*.head: MacroDefHead`,
    /// `LispError::UnboundTemplateVar.prefix: UnquoteForm`,
    /// `LispError::CompilerSpecIo.stage: CompilerSpecIoStage`,
    /// `LispError::TemplateInvariant.kind: TemplateInvariantKind`, and
    /// `LispError::TypeMismatch.form: KwargPath`: the closed set
    /// becomes a TYPE rather than a `&'static str` projection at the
    /// helper boundary. The Display projection flows through
    /// `ExpectedKwargShape::Display`, so the user-facing
    /// `"... expected {expected}, ..."` rendering matches the legacy
    /// `&'static str`-shaped diagnostic byte-for-byte.
    ///
    /// `got` is the typed closed-set `SexpShape` enum — the twelve
    /// reachable Sexp outermost shapes (`Nil` ⊎ `Symbol` ⊎ `Keyword` ⊎
    /// `String` ⊎ `Int` ⊎ `Float` ⊎ `Bool` ⊎ `List` ⊎ `Quote` ⊎
    /// `Quasiquote` ⊎ `Unquote` ⊎ `UnquoteSplice`) encoded as variant
    /// identities so the SexpShape that the typed-entry gate observed
    /// is load-bearing data in the type system. Consumers (REPL, LSP,
    /// `tatara-check`) pattern-match on `SexpShape::Int` etc. directly
    /// instead of substring-matching `got == "int"`. Same posture as
    /// `form: KwargPath` and `expected: ExpectedKwargShape`: after this
    /// lift the TypeMismatch variant's identity is fully closed-set
    /// typed in ALL THREE of its slots — no `&'static str` projection
    /// at any helper boundary, every reachable identity encoded as a
    /// variant of a typed enum. The Display projection flows through
    /// `SexpShape::Display` (which delegates to `SexpShape::label()`),
    /// so the user-facing `"... got {got}"` rendering matches the
    /// legacy `&'static str`-shaped diagnostic byte-for-byte. When a
    /// future run gives `Sexp` source spans, `pos: Option<usize>`
    /// lands here in ONE place and every type-mismatch site picks up
    /// positional rendering via `crate::diagnostic::format_diagnostic`
    /// mechanically.
    #[error("compile error in {form}: expected {expected}, got {got}")]
    TypeMismatch {
        form: KwargPath,
        expected: ExpectedKwargShape,
        got: SexpShape,
    },
    /// Structural head-mismatch — the `(head ...)` of a top-level form
    /// didn't match `T::KEYWORD`. Both sides are first-class fields, not
    /// embedded substrings of `message`. Rendered as `"compile error in
    /// {keyword}: expected ({keyword} ...), got ({got} ...)"` so the
    /// user-facing string matches the legacy `Compile`-shaped diagnostic
    /// byte-for-byte; the gain is structural — authoring tools (REPL,
    /// LSP, `tatara-check`) pattern-match on the variant and bind
    /// directly to `keyword` / `got` instead of substring-parsing the
    /// rendered message.
    ///
    /// `keyword` is `&'static str` because it always comes from
    /// `T::KEYWORD`, a compile-time literal; `got` is `String` because
    /// it is an arbitrary symbol from the source. When a future run
    /// gives `Sexp` source spans, `pos: Option<usize>` lands here in
    /// ONE place and every head-mismatch site picks up positional
    /// rendering via `crate::diagnostic::format_diagnostic`
    /// mechanically.
    #[error("compile error in {keyword}: expected ({keyword} ...), got ({got} ...)")]
    HeadMismatch { keyword: &'static str, got: String },
    #[error("unknown {category}: {value}")]
    Unknown {
        category: &'static str,
        value: String,
    },
    #[error("missing required field: {0}")]
    Missing(&'static str),
    /// A kwargs list of odd length: the last element has no partner. The
    /// `dangling` field holds the offending element's `Sexp::Display`
    /// projection — `:query` for a keyword whose value got lost, or the
    /// literal form of a stray non-keyword. Naming both halves of the
    /// failure (the failure mode AND the offending element) is the
    /// typed-entry gate's structural-completeness floor (THEORY.md §V.1):
    /// without it the operator must re-read the source to find what
    /// actually misfired.
    #[error("odd keyword arguments: dangling element `{dangling}`")]
    OddKwargs { dangling: String },
    /// An unquote (`,name`) or unquote-splice (`,@name`) in a macro template
    /// body referenced a name that wasn't bound by the macro's params (during
    /// `compile_template`) or wasn't available in the substitution scope
    /// (during `substitute`). `prefix` is the syntactic marker — `","` for
    /// unquote, `",@"` for splice — so the rendered diagnostic preserves the
    /// form exactly as the author wrote it. `hint` is `Some(name)` when the
    /// substrate found a near-miss against the available bound names within
    /// the bounded edit distance (see `crate::domain::suggest`); `None`
    /// otherwise — a wrong hint is worse than no hint, so the slot stays
    /// empty unless the substrate is confident.
    ///
    /// `prefix` is `UnquoteForm` — the closed-set typed enum whose two
    /// variants are EXACTLY the two reachable syntactic markers
    /// (`Unquote` ⊎ `Splice`). Encoding the closed set as a TYPE makes the
    /// constraint that ONLY 2 marker identities are reachable load-bearing
    /// in the type system — a third pseudo-marker can never drift into the
    /// diagnostic at runtime; consumers (REPL, LSP, `tatara-check`)
    /// pattern-match on `UnquoteForm::Splice` etc. directly instead of
    /// substring-matching `prefix == ",@"`. Same posture as
    /// `LispError::Defmacro*.head: MacroDefHead`,
    /// `LispError::CompilerSpecIo.stage: CompilerSpecIoStage`, and
    /// `LispError::TemplateInvariant.kind: TemplateInvariantKind`: the
    /// closed set becomes a TYPE rather than a `&'static str` projection
    /// at the helper boundary. `name` and `hint` are `String` because
    /// they come from arbitrary source / the live bindings set. When a
    /// future run gives `Sexp` source spans, `pos: Option<usize>` lands
    /// here in ONE place and every unbound-template-var site picks up
    /// positional rendering via `crate::diagnostic::format_diagnostic`
    /// mechanically.
    ///
    /// Display matches the legacy `Compile`-shaped diagnostic byte-for-byte
    /// when `hint` is `None` — `"compile error in {prefix}{name}: unbound"`
    /// — so existing consumer assertions that pattern-match on the message
    /// substring keep passing. With a hint, the suffix `"; did you mean
    /// {prefix}{hint}?"` is appended; the prefix is preserved in the hint so
    /// the operator can copy-paste the suggestion verbatim.
    #[error(
        "compile error in {prefix}{name}: unbound{}",
        unbound_hint_suffix(*prefix, hint.as_deref())
    )]
    UnboundTemplateVar {
        prefix: UnquoteForm,
        name: String,
        hint: Option<String>,
    },
    /// A kwargs slice contained the same `:key` twice. The offending key is
    /// carried as a structural field — not embedded in a free-form message —
    /// so authoring surfaces (REPL, LSP, `tatara-check`) pattern-match on
    /// the variant and bind to `key` directly instead of substring-parsing
    /// the rendered diagnostic. Same posture as the `OddKwargs { dangling }`
    /// sibling: every distinct `parse_kwargs` failure mode (odd length,
    /// not-a-keyword-at-position, duplicate key) is now a structural variant
    /// of `LispError`, not a `Compile`-shaped substring.
    ///
    /// `key` is `String` because it comes from arbitrary source. Display
    /// renders `"compile error in :{key}: duplicate keyword"` byte-for-byte
    /// equivalent to the legacy `Compile { form: kwarg_form(key), message:
    /// "duplicate keyword" }` shape, so existing consumer assertions
    /// (`msg.contains(":name")`, `msg.contains("duplicate keyword")`) pass
    /// unchanged. When a future run gives `Sexp` source spans, `pos:
    /// Option<usize>` lands here in ONE place and every duplicate-kwarg
    /// site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in :{key}: duplicate keyword")]
    DuplicateKwarg { key: String },
    /// A required kwarg was absent from the kwargs slice. The offending key
    /// is carried as a structural field — not embedded in a free-form
    /// message — so authoring surfaces (REPL, LSP, `tatara-check`)
    /// pattern-match on the variant and bind to `key` directly instead of
    /// substring-parsing the rendered diagnostic. Same posture as
    /// `DuplicateKwarg { key }` and `OddKwargs { dangling }`: every
    /// distinct typed-entry kwarg failure mode is now a structural variant
    /// of `LispError`, not a `Compile`-shaped substring. Sibling of the
    /// pre-existing `Missing(&'static str)` variant — `MissingKwarg`
    /// covers the runtime-key path the kwargs extractors share, while
    /// `Missing` stays for compile-time-known names.
    ///
    /// `key` is `String` because it comes from the runtime kwargs lookup
    /// (each derive-generated extractor and every hand-written
    /// `TataraDomain` impl can pass an arbitrary key). Display renders
    /// `"compile error in :{key}: required but not provided"`
    /// byte-for-byte equivalent to the legacy `Compile { form:
    /// kwarg_form(key), message: "required but not provided" }` shape, so
    /// existing consumer assertions (`msg.contains(":threshold")`,
    /// `msg.contains("required")`) pass unchanged. When a future run gives
    /// `Sexp` source spans, `pos: Option<usize>` lands here in ONE place
    /// and every missing-kwarg site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in :{key}: required but not provided")]
    MissingKwarg { key: String },
    /// A kwargs slice contained a `:key` that isn't in the allowed-kwarg
    /// set for the surrounding `TataraDomain`. The offending key, the
    /// near-miss hint (if any), and the full allowed set are all carried
    /// as first-class fields — not embedded in a free-form message — so
    /// authoring surfaces (REPL, LSP, `tatara-check`) pattern-match on
    /// the variant and bind to `key` / `hint` / `allowed` directly
    /// instead of substring-parsing the rendered message. Same posture
    /// as the `OddKwargs { dangling }`, `DuplicateKwarg { key }`, and
    /// `MissingKwarg { key }` siblings: every distinct typed-entry
    /// kwarg-gate failure mode is now a structural variant of
    /// `LispError`, not a `Compile`-shaped substring.
    ///
    /// `key` is `String` because it comes from arbitrary source. `hint`
    /// is `Some(allowed_keyword)` when `crate::domain::suggest` ranks an
    /// allowed kwarg within the bounded edit distance; `None`
    /// otherwise — a wrong hint is worse than no hint, so the slot
    /// stays empty unless the substrate is confident. `allowed` is
    /// `Vec<String>` (sorted lexicographically by `unknown_kwarg`)
    /// because the variant owns its data — the derive-emitted
    /// `&'static [&'static str]` allowed-set crosses the structural
    /// boundary as owned `String`s so the diagnostic crosses thread
    /// boundaries cleanly and lives independent of the call frame.
    /// Display matches the legacy `Compile { form: kwarg_form(key),
    /// message: "unknown keyword (...)" }` rendering byte-for-byte
    /// (`"compile error in :{key}: unknown keyword (did you mean
    /// :{hint}?; allowed: :a, :b, :c)"` with a hint, `"compile error
    /// in :{key}: unknown keyword (allowed: :a, :b, :c)"` without),
    /// so existing consumer assertions
    /// (`msg.contains("unknown keyword")`,
    /// `msg.contains("did you mean :threshold?")`,
    /// `msg.contains("allowed: ")`) pass unchanged. When a future
    /// run gives `Sexp` source spans, `pos: Option<usize>` lands
    /// here in ONE place and every unknown-kwarg site picks up
    /// positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error(
        "compile error in :{key}: unknown keyword{}",
        unknown_kwarg_suffix(hint.as_deref(), allowed)
    )]
    UnknownKwarg {
        key: String,
        hint: Option<String>,
        allowed: Vec<String>,
    },
    /// A registry-dispatched form `(<keyword> ...)` whose head symbol isn't in
    /// the global `TataraDomain` registry. The offending keyword, the
    /// near-miss hint (if any), and the full registered keyword set are all
    /// carried as first-class fields — not embedded in a free-form message —
    /// so authoring surfaces (REPL, LSP, `tatara-check`) pattern-match on the
    /// variant and bind to `keyword` / `hint` / `registered` directly instead
    /// of substring-parsing the rendered message. Same posture as the
    /// `UnknownKwarg { key, hint, allowed }` sibling: the kwarg-gate's
    /// unknown-allowed-set rejection and the registry-gate's
    /// unknown-registered-set rejection share ONE structural shape.
    ///
    /// `keyword` is `String` because it comes from arbitrary source (a
    /// top-level form's head symbol). `hint` is
    /// `Some(registered_keyword)` when `crate::domain::suggest_keyword`
    /// ranks a registered keyword within the bounded edit distance;
    /// `None` otherwise — a wrong hint is worse than no hint, so the slot
    /// stays empty unless the substrate is confident. `registered` is
    /// `Vec<String>` (sorted lexicographically by
    /// `unknown_domain_keyword`) because the variant owns its data — the
    /// registry's `&'static [&'static str]` keyword-set crosses the
    /// structural boundary as owned `String`s so the diagnostic crosses
    /// thread boundaries cleanly and lives independent of the call frame.
    /// Empty `registered` (no domains seeded) renders `(no domains
    /// registered)` so the operator sees the structural reason — the
    /// registry has no candidates at all — instead of a misleading empty
    /// "registered: " suffix. When a future run gives `Sexp` source
    /// spans, `pos: Option<usize>` lands here in ONE place and every
    /// unknown-domain-keyword site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error(
        "unknown domain keyword: ({keyword} ...){}",
        unknown_domain_keyword_suffix(hint.as_deref(), registered)
    )]
    UnknownDomainKeyword {
        keyword: String,
        hint: Option<String>,
        registered: Vec<String>,
    },
    /// The slot inside a `Sexp::Unquote(_)` (`,X`) or
    /// `Sexp::UnquoteSplice(_)` (`,@X`) was not a symbol. The `prefix`
    /// field is the syntactic marker — `","` for unquote, `",@"` for
    /// splice — so the rendered diagnostic preserves the form exactly
    /// as the author wrote it. The `got` field is the offending inner's
    /// `Sexp::Display` projection so the operator sees both what was
    /// expected (a symbol — the only form a no-evaluator template can
    /// substitute) and what was actually written (the literal value —
    /// `(list 1 2)`, `5`, `:foo`, etc.). Naming both halves of the
    /// failure is the typed-entry gate's structural-completeness floor
    /// (THEORY.md §V.1).
    ///
    /// Sibling of `UnboundTemplateVar { prefix, name, hint }` for the
    /// same template-side typed-entry surface — that variant fires when
    /// the slot IS a symbol but the symbol isn't bound; this variant
    /// fires when the slot isn't a symbol at all. After this lift every
    /// distinct typed-entry template-gate failure mode binds to ONE
    /// structural variant of `LispError`, not a `Compile`-shaped
    /// substring.
    ///
    /// `prefix` is `UnquoteForm` — the closed-set typed enum whose two
    /// variants are EXACTLY the two reachable syntactic markers
    /// (`Unquote` ⊎ `Splice`). Encoding the closed set as a TYPE makes
    /// the constraint load-bearing in the type system — a third
    /// pseudo-marker can never drift into the diagnostic at runtime;
    /// consumers (REPL, LSP, `tatara-check`) pattern-match on
    /// `UnquoteForm::Splice` etc. directly instead of substring-matching
    /// `prefix == ",@"`. Same typed-slot posture as `UnboundTemplateVar`'s
    /// `prefix` slot, parallel to `LispError::Defmacro*.head:
    /// MacroDefHead`. `got` is `SexpWitness` — the closed-set typed
    /// joint identity pairing the offending inner's `SexpShape` (the
    /// twelve reachable outermost shapes the reader can produce) with
    /// its `Sexp::Display` projection (the literal value the author
    /// wrote — `(list 1 2)`, `5`, `:foo`, etc.). Same typed-witness
    /// posture as `SpliceOutsideList.got: SexpWitness`: authoring
    /// tools (REPL, LSP, `tatara-check`) bind to BOTH `got.shape`
    /// (structurally pattern-matchable on `SexpShape::List` etc.) AND
    /// `got.display` (the literal value, renderable verbatim) without
    /// losing either side. The two template-gate `,X/,@X` rejection
    /// variants (`NonSymbolUnquoteTarget` AND `SpliceOutsideList`)
    /// now share ONE typed witness identity at their `got` slot —
    /// every Sexp-display-source `got` slot on the template-gate's
    /// distinct rejection variants carries the SAME typed primitive.
    /// When a future run gives `Sexp` source spans, `pos:
    /// Option<usize>` lands here in ONE place and every
    /// non-symbol-unquote-target site picks up positional rendering
    /// via `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {prefix}: expected symbol, got {got}")]
    NonSymbolUnquoteTarget {
        prefix: UnquoteForm,
        got: SexpWitness,
    },
    /// A `,@X` (unquote-splice) appeared at a syntactic position where there
    /// is no containing list to splice into — i.e. the splice is the entire
    /// macro-template body, not nested inside a `(... ,@xs ...)` list. Splice
    /// is always list-flattening: `,@(a b c)` inside `(outer ,@xs)` becomes
    /// `(outer a b c)`. At a non-list position there is no list to flatten
    /// into; the form is ill-positioned regardless of whether the inner slot
    /// is a symbol, a literal, or a bound list.
    ///
    /// Sibling of `NonSymbolUnquoteTarget { prefix, got }` and
    /// `UnboundTemplateVar { prefix, name, hint }` for the template-gate's
    /// other distinct failure modes — together the three close every
    /// distinct typed-entry template-gate failure mode for the no-evaluator
    /// template language: each is a structural variant of `LispError`, not
    /// a `Compile`-shaped substring. `prefix` is implicit (always `,@`) and
    /// elided from the variant: this failure mode names ONE syntactic
    /// marker, parallel to how `OddKwargs` names ONE failure mode (odd-length
    /// kwargs slice) without a syntactic-marker slot.
    ///
    /// `got` is `SexpWitness` — the closed-set typed joint identity
    /// pairing the offending inner's `SexpShape` (the twelve reachable
    /// outermost shapes the reader can produce) with its
    /// `Sexp::Display` projection (the literal value the author wrote
    /// — `xs`, `(list 1 2)`, `5`, `:foo`, etc.). Promotes the legacy
    /// `got: String` shape to a typed witness so authoring tools (REPL,
    /// LSP, `tatara-check`) bind to BOTH `got.shape` (structurally
    /// pattern-matchable on `SexpShape::List` etc.) AND `got.display`
    /// (the literal value, renderable verbatim) without losing either
    /// side. Naming both the failure mode AND the offending element
    /// is the typed-entry gate's structural-completeness floor
    /// (THEORY.md §V.1) — without it the operator must re-read the
    /// source to find what actually misfired. After this lift the
    /// structural identity is part of the variant's typed data shape;
    /// a regression that re-collapses `got` to a free-form `String`
    /// loses the rustc-enforced closed-set guarantee on shape
    /// identity.
    ///
    /// First consumer of the `SexpWitness` primitive. Sibling lifts
    /// landed for `NonSymbolUnquoteTarget.got`, `NonSymbolParam.got`,
    /// `DefmacroNonSymbolName.got`, and `DefmacroNonListParams.got`;
    /// the remaining trajectory —
    /// `RestParamMissingName.got: Option<String>` and
    /// `MissingHeadSymbol.got: Option<String>` — is the next set of
    /// moves: every `got: String` (or `Option<String>`) slot whose
    /// source is `Sexp::Display` picks up the typed witness
    /// mechanically once the variant's data shape is bumped. The
    /// remaining two are both `Option<String>` — the typed witness
    /// lands on the `Some` arm directly, the `None` arm encodes the
    /// "missing entirely" sub-mode that's structurally distinct from
    /// "present but malformed".
    ///
    /// When a future run gives `Sexp` source spans, `pos: Option<usize>`
    /// lands on `SexpWitness` ONCE and every splice-outside-list site
    /// picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    ///
    /// Display renders `"compile error in ,@: \`,@\` may only appear inside
    /// a list (got ,@{got})"` — the legacy substring `"\`,@\` may only
    /// appear inside a list"` is preserved verbatim so authoring tools that
    /// substring-match on the rendered diagnostic see no drift; the
    /// parenthetical `(got ,@{got})` names the offending form so an LSP
    /// quick-fix that surfaces "the splice has no containing list; you
    /// wrote `,@xs`" gains the literal value as data, no message re-parsing
    /// required. The `{got}` slot flows through `SexpWitness::Display`,
    /// which writes only the `display` field, so the rendering is
    /// byte-for-byte identical to the legacy `got: String` shape.
    #[error("compile error in ,@: `,@` may only appear inside a list (got ,@{got})")]
    SpliceOutsideList { got: SexpWitness },
    /// A macro was called with fewer arguments than its required-param arity:
    /// `(defmacro f (a b) `(,a ,b)) (f 1)` — `b` has no arg. Both the failing
    /// macro's name AND the un-bound param are first-class structural fields,
    /// not embedded substrings of `message`, so authoring surfaces (REPL,
    /// LSP, `tatara-check`) pattern-match on the variant and bind to
    /// `macro_name` / `param` directly instead of substring-parsing the
    /// rendered message. Sibling of `MissingKwarg { key }` for the
    /// macro-call-gate's positional-arity surface — that variant fires when
    /// a `(<head> :key value …)` kwargs form omits a required keyword;
    /// this variant fires when a `(<macroname> a b …)` call omits a required
    /// positional param. The two close every distinct typed-entry
    /// missing-required surface in the substrate.
    ///
    /// Same single emission shape across both expansion strategies — the
    /// substitute path's `bind_args` and the bytecode path's
    /// `apply_compiled` share ONE structural variant, parallel to how the
    /// template-gate's `SpliceOutsideList` is shared across both paths
    /// (THEORY.md §II.1 invariant 2 — free middle: which strategy you
    /// picked must not change which inputs you reject). Before this lift
    /// the same failure mode emitted ONE `LispError::Compile { form:
    /// format!("call to {macro_name}"), message: format!("missing
    /// required arg: {param}") }` triple at TWO call sites — the
    /// three-times rule had two sites with byte-identical shape and one
    /// failure mode.
    ///
    /// `macro_name` and `param` are `String` because they come from
    /// arbitrary source (the call-site head symbol AND the
    /// macro-definition's param symbol). Display matches the legacy
    /// `Compile`-shaped diagnostic byte-for-byte — `"compile error in call
    /// to {macro_name}: missing required arg: {param}"` — so existing
    /// consumer assertions (`msg.contains("missing required arg")`) pass
    /// unchanged. When a future run gives `Sexp` source spans, `pos:
    /// Option<usize>` lands here in ONE place and every missing-macro-arg
    /// site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in call to {macro_name}: missing required arg: {param}")]
    MissingMacroArg { macro_name: String, param: String },
    /// A macro was called with MORE arguments than its declared
    /// required+optional arity, on a param list with NO `&rest` slot to
    /// collect the surplus: `(defmacro f (a b) `(,a ,b)) (f 1 2 3)` — `3`
    /// has nowhere to bind. The mirror at the call-site of
    /// `RestParamTrailingTokens` (the definition-site rejection that
    /// surfaces tokens trailing a `&rest <name>` clause, lifted in the
    /// prior-run typed-promotion lineage): that variant rejects malformed
    /// DEFINITIONS that the typed `MacroParams` shape cannot hold (a
    /// `&rest` clause is structurally LAST); this variant rejects
    /// malformed CALLS that the typed `bind` cannot honor (a rest-less
    /// param list has a FIXED maximum arity equal to
    /// `required.len() + optional.len()`). Together with
    /// `MissingMacroArg`, the macro-call-gate's positional-arity surface
    /// is now structurally complete in both directions — too-few AND
    /// too-many — closing the asymmetry where the typed-entry gate
    /// rejected too-few-args loudly but silently truncated too-many to
    /// the slice `bind` could consume.
    ///
    /// A rest-PRESENT param list has no maximum arity (the `&rest` slot
    /// collects every trailing arg into a `Sexp::List`), so this
    /// rejection fires ONLY when `MacroParams.rest` is `None`. The
    /// `expected` slot is `required.len() + optional.len()` — the maximum
    /// number of args the rest-less binder can consume; `got` is
    /// `args.len()` — the actual number supplied. Surfacing both lets
    /// authoring tools (REPL, LSP, `tatara-check`) name the
    /// "you supplied {got} args but the macro takes at most {expected}"
    /// quick-fix without re-deriving either count from the source.
    ///
    /// The leftmost-priority discipline is preserved: `MissingMacroArg`
    /// for a missing REQUIRED arg fires BEFORE this too-many gate
    /// (`bind` iterates the required walk first and bails on the first
    /// missing slot), so `(defmacro f (a b c) …) (f 1)` is
    /// `MissingMacroArg { param: "b" }`, NOT `TooManyMacroArgs`. The two
    /// failure modes are structurally disjoint: too-few-required vs.
    /// too-many-with-no-rest.
    ///
    /// `macro_name` is `String` because it comes from arbitrary source
    /// (the call-site head symbol); `expected` and `got` are `usize`
    /// arities. Display matches the legacy `Compile`-shaped diagnostic
    /// style — `"compile error in call to {macro_name}: too many args:
    /// expected at most {expected}, got {got}"` — so the same
    /// `"compile error in call to {macro_name}:"` substring authoring
    /// tools' assertions key on stays unchanged across the new
    /// rejection mode. When a future run gives `Sexp` source spans,
    /// `pos: Option<usize>` lands here in ONE place and every
    /// too-many-macro-args site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically — same
    /// posture as `MissingMacroArg`.
    #[error(
        "compile error in call to {macro_name}: too many args: \
         expected at most {expected}, got {got}"
    )]
    TooManyMacroArgs {
        macro_name: String,
        expected: usize,
        got: usize,
    },
    /// A non-symbol element appeared in a `defmacro` / `defpoint-template`
    /// / `defcheck` param list at the named position. The legacy
    /// `LispError::Compile { form: "defmacro params", message: "expected
    /// symbol" }` shape named only the failure mode — it didn't say WHICH
    /// element of the param list misfired NOR what was found in its slot.
    /// The structural variant names both: `position` is the 0-based index
    /// of the offending element within the param list, `got` is its
    /// `Sexp::Display` projection so the operator sees the literal value
    /// they wrote (`5`, `"x"`, `:foo`, `(nested)`) instead of the bare
    /// "expected symbol" verdict. Naming both the position AND the
    /// offending element is the typed-entry gate's
    /// structural-completeness floor (THEORY.md §V.1) — without both an
    /// LSP that wants to surface "the third element of your param list
    /// isn't a symbol; you wrote `5`" must re-parse the source.
    ///
    /// Sibling of `MissingMacroArg { macro_name, param }` for the
    /// macro-call-gate's positional-arity surface — that variant fires
    /// when a CALL `(<macroname> a b …)` omits a required positional
    /// param; this variant fires when the DEFMACRO `(defmacro <name> (a
    /// b …) …)` declaration's param list contains a non-symbol where a
    /// param name was expected. The two are the macro-call-gate and the
    /// defmacro-syntax-gate's first-named structural failure modes
    /// respectively — call-site malformed vs. definition-site malformed.
    ///
    /// `position` is `usize` because it is always the loop index inside
    /// `parse_params`; `got` is `SexpWitness` — the closed-set typed
    /// joint identity (structural `SexpShape` + renderable
    /// `Sexp::Display` projection) the offending-value side of the
    /// typed-entry rejection owes the operator. Third consumer of the
    /// `SexpWitness` primitive (after `SpliceOutsideList.got` and
    /// `NonSymbolUnquoteTarget.got`); same posture — authoring tools
    /// (REPL, LSP, `tatara-check`) bind to BOTH `got.shape`
    /// (`SexpShape::Int`, `SexpShape::Keyword`, `SexpShape::List`, etc.)
    /// AND `got.display` (the literal value, renderable verbatim)
    /// jointly across the variant slot rather than substring-grepping
    /// a free-form `String`. Display projects the witness's `display`
    /// field verbatim into the `#[error(... got {got})]` annotation's
    /// `{got}` slot, so the rendered `"compile error in defmacro params:
    /// expected symbol at position {position}, got <display>"` shape is
    /// byte-for-byte identical to the pre-lift `got: String` rendering;
    /// authoring tools that substring-grep on the rendered diagnostic
    /// see no drift. When a future run gives `Sexp` source spans, `pos:
    /// Option<usize>` lands inside `SexpWitness` in ONE place and every
    /// non-symbol-param site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error(
        "compile error in defmacro params: expected symbol at position \
         {position}, got {got}"
    )]
    NonSymbolParam { position: usize, got: SexpWitness },
    /// A `&rest` marker in a `defmacro` / `defpoint-template` / `defcheck`
    /// param list was followed by no element at all (`(&rest)`,
    /// `(a &rest)`) OR by a non-symbol element (`(&rest 5)`,
    /// `(&rest :foo)`). The legacy `LispError::Compile { form: "defmacro
    /// params", message: "&rest needs a name" }` shape named only the
    /// failure mode — it didn't say WHICH `&rest` (i.e. its position
    /// within the param list) misfired NOR what was found in the slot
    /// where the rest-name should have been. The structural variant
    /// names both: `rest_position` is the 0-based index of the `&rest`
    /// marker within the param list, `got` is the offending follower's
    /// typed witness (`Some(SexpWitness::new(SexpShape::Int, "5"))`,
    /// `Some(SexpWitness::new(SexpShape::Keyword, ":foo"))`,
    /// `Some(SexpWitness::new(SexpShape::List, "(nested)"))`) or
    /// `None` when the marker was the last element in the list and
    /// nothing followed at all. Naming both the marker position AND
    /// the offending follower (or its absence) is the typed-entry
    /// gate's structural-completeness floor (THEORY.md §V.1) —
    /// without both, an LSP that wants to surface "your `&rest` at
    /// param-list position 1 has no name; you wrote `5` instead of
    /// a symbol" must re-parse the source.
    ///
    /// Sibling of `NonSymbolParam { position, got }` for the
    /// defmacro-syntax-gate's other definition-site failure mode —
    /// that variant fires when a NON-`&rest` element at a param
    /// position isn't a symbol; this variant fires specifically on the
    /// post-`&rest` follower slot, where the failure mode bifurcates
    /// into "missing entirely" vs. "present but not a symbol". Both
    /// modes share ONE structural variant via `got: Option<SexpWitness>`
    /// (parallel to how `UnboundTemplateVar` and `UnknownKwarg` carry
    /// `hint: Option<String>` for a present-or-absent secondary slot)
    /// rather than splitting into two near-identical variants — the
    /// failure mode IS one ("rest name missing"); the bifurcation is
    /// in the renderable detail, not in what the gate rejects.
    ///
    /// Together, `NonSymbolParam` and `RestParamMissingName` close the
    /// `parse_params` pair — every distinct failure mode the
    /// `parse_params` walker can emit is now a structural variant of
    /// `LispError`, not a `Compile`-shaped substring.
    ///
    /// `rest_position` is `usize` because it is always the loop index
    /// inside `parse_params` at which the `&rest` marker was matched;
    /// `got` is `Option<SexpWitness>` — the SIXTH consumer of the typed
    /// `SexpWitness` primitive (after `SpliceOutsideList.got`,
    /// `NonSymbolUnquoteTarget.got`, `NonSymbolParam.got`,
    /// `DefmacroNonSymbolName.got`, and `DefmacroNonListParams.got`).
    /// The `Option`-shape bifurcates structurally into "missing
    /// entirely" (`None`, when the marker was the param list's last
    /// element) and "present but malformed" (`Some(SexpWitness)`, when
    /// a non-symbol follower came from arbitrary source via
    /// `Sexp::Display`); the typed witness lands on the `Some` arm
    /// only. Display preserves the legacy `"compile error in defmacro
    /// params: &rest needs a name"` prefix byte-for-byte so authoring
    /// tools that substring-grep on the rendered diagnostic see no
    /// drift; the structural detail (`(rest marker at position
    /// {rest_position}, got {got})` when present, `(rest marker at
    /// position {rest_position}, none provided)` when absent) is
    /// appended. When a future run gives `Sexp` source spans, `pos:
    /// Option<usize>` lands inside `SexpWitness` in ONE place and
    /// every rest-param-missing-name site picks up positional
    /// rendering via `crate::diagnostic::format_diagnostic`
    /// mechanically.
    #[error(
        "compile error in defmacro params: &rest needs a name{}",
        rest_param_missing_name_suffix(*rest_position, got.as_ref().map(|w| w.display.as_str()))
    )]
    RestParamMissingName {
        rest_position: usize,
        got: Option<SexpWitness>,
    },
    /// A `&rest <name>` param was followed by one or more further tokens —
    /// `(defmacro f (a &rest xs extra) …)`. The `&rest` name binds every
    /// remaining call arg into a list, so it is structurally the LAST thing
    /// a param list can name: nothing can follow it. Before this variant
    /// `parse_params` returned the moment it bound the rest name, SILENTLY
    /// DROPPING any trailing tokens — `extra` above vanished with no error,
    /// so an author who fat-fingered a stray param (or wrote `&rest xs
    /// &optional y` expecting a feature that doesn't exist yet) got no
    /// signal that their text was ignored. This variant turns that silent
    /// drop into a loud rejection at the typed-entry gate.
    ///
    /// Sibling of `NonSymbolParam` and `RestParamMissingName` — the third
    /// and final `parse_params` definition-site failure mode. The earlier
    /// two fire on a param SLOT (a non-symbol where a name was expected) and
    /// on the post-`&rest` follower (missing-or-malformed name); this one
    /// fires once the rest name is bound and the walker discovers the param
    /// list does not end there. Together the three now genuinely close the
    /// `parse_params` walker — every shape it accepts is a well-formed
    /// `MacroParams`, and every shape it rejects is a structural variant of
    /// `LispError`, not a silently-truncated `Vec` nor a `Compile`-shaped
    /// substring.
    ///
    /// `rest_position` is the loop index at which the `&rest` marker was
    /// matched (parallel to `RestParamMissingName.rest_position`), so an LSP
    /// quick-fix can point at the `&rest` form whose name must be last.
    /// `extra` is the count of trailing tokens (always ≥ 1) and `first` is
    /// the typed witness of the first of them — the SEVENTH consumer of the
    /// `SexpWitness` primitive. `first` is a non-`Option` witness (unlike
    /// `RestParamMissingName.got`) because the trailing run is non-empty by
    /// construction: this variant is only built when `list[rest_position +
    /// 2..]` has a first element. Display appends `(rest marker at position
    /// {rest_position}, {extra} trailing after name, first: {first})` via
    /// `rest_param_trailing_tokens_suffix`, which delegates the bare
    /// parenthetical to the shared `paren_suffix`. When a future run gives
    /// `Sexp` source spans, `pos: Option<usize>` lands inside `SexpWitness`
    /// in ONE place and this site picks up positional rendering
    /// mechanically — exactly as its two siblings do.
    #[error(
        "compile error in defmacro params: &rest name must be last{}",
        rest_param_trailing_tokens_suffix(*rest_position, *extra, &first.display)
    )]
    RestParamTrailingTokens {
        rest_position: usize,
        extra: usize,
        first: SexpWitness,
    },
    /// A `defmacro` / `defpoint-template` / `defcheck` param list carried a
    /// SECOND `&optional` marker — `(defmacro f (a &optional b &optional c)
    /// …)`. The canonical Lisp lambda-list (`(req* &optional opt* &rest r)`,
    /// the shape [`MacroParams`](crate::macro_expand::MacroParams) makes a
    /// type) has exactly ONE optional section, between the required run and
    /// the rest. A second `&optional` is unrepresentable: `MacroParams.optional`
    /// is one flat `Vec`, not a sequence of sections. Without this gate the
    /// walker would treat the second `&optional` as an optional param literally
    /// NAMED `&optional`, binding call args to a marker symbol — the precise
    /// silent misalignment the typed param-list shape exists to forbid (a
    /// sibling of the index-misalignment `MacroParams` ruled out when it
    /// replaced `Vec<Param>`).
    ///
    /// Sibling of `RestParamTrailingTokens` — both fire INSIDE `parse_params`
    /// once a marker is matched and the walker finds the surrounding param
    /// list's marker structure is one the canonical lambda-list ordering
    /// cannot represent (tokens after `&rest <name>`; a repeated `&optional`).
    /// `first_position` is the loop index of the first `&optional` marker,
    /// `second_position` the index of the offending second one — naming both
    /// lets an LSP quick-fix point at the redundant marker to delete. Neither
    /// is a `SexpWitness`: both elements ARE the `&optional` symbol by
    /// construction (the variant is only built when `s == "&optional"` twice),
    /// so there is nothing to witness — only the two positions carry
    /// information. When a future run gives `Sexp` source spans, the marker
    /// positions gain editor-ready rendering by threading spans here.
    #[error(
        "compile error in defmacro params: &optional may appear at most once{}",
        optional_marker_repeated_suffix(*first_position, *second_position)
    )]
    OptionalMarkerRepeated {
        first_position: usize,
        second_position: usize,
    },
    /// A `defmacro` / `defpoint-template` / `defcheck` `&optional` section
    /// carried a list-form entry that did not match the only admissible
    /// shape `(NAME DEFAULT)` — exactly two elements with a symbol head.
    /// Per-param default forms are the typed promotion of `optional:
    /// Vec<String>` to `optional: Vec<OptionalParam>` that the prior
    /// `&optional` run signposted, and this variant is the gate that
    /// admits only the canonical `(NAME DEFAULT)` shape into the typed
    /// `OptionalParam.default` slot. Four distinct list shapes are
    /// rejected, named via the closed-set typed `reason`
    /// ([`OptionalParamMalformedReason`]):
    ///
    ///   * `()`              — empty list spec.
    ///   * `(name)`          — one element, no default form supplied.
    ///   * `(name d e …)`    — three or more elements (CL's
    ///     `(name default supplied-p)` shape is not yet supported — no
    ///     `supplied-p` variable binding without an evaluator).
    ///   * `(5 default)`     — first element isn't a symbol.
    ///
    /// Sibling of `OptionalMarkerRepeated` (the `&optional`-section
    /// marker gate) and `NonSymbolParam` (the bare-symbol gate): together
    /// the three close every distinct typed-entry rejection the optional
    /// section can emit. The bare-symbol form `&optional x` is still
    /// admitted through the bare-symbol path; the list form `&optional
    /// (x default)` is admitted iff this gate accepts the spec.
    ///
    /// `position` is the loop index of the offending list inside
    /// `parse_params`, parallel to `OptionalMarkerRepeated.first_position`
    /// / `RestParamTrailingTokens.rest_position` — naming the position
    /// lets an LSP quick-fix point at the spec to repair. `got` is
    /// `SexpWitness` — the closed-set typed joint identity pairing the
    /// offending list's `SexpShape::List` with its `Sexp::Display`
    /// projection, so consumers (REPL, LSP, `tatara-check`) bind to
    /// BOTH the structural shape AND the renderable literal jointly,
    /// same posture as `NonSymbolParam.got` / `OptionalMarkerRepeated`'s
    /// SexpWitness siblings. `reason` is `OptionalParamMalformedReason`
    /// — the closed-set typed enum whose four variants are EXACTLY the
    /// four reachable list-spec rejection modes, encoded as a TYPE so a
    /// future fifth reason (e.g. supplied-p once an evaluator lands)
    /// becomes a type-level extension rather than a substring drift.
    /// Mirror at the `parse_params` optional-section boundary of the
    /// prior-run `MacroDefHead` / `UnquoteForm` / `TemplateInvariantKind`
    /// / `CompilerSpecIoStage` closed-set lifts.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform / "make invalid
    /// states unrepresentable"; the four malformed list-spec shapes are
    /// nonsense `MacroParams` cannot hold, so the gate must REJECT
    /// rather than bind args to a marker symbol or drop the extras
    /// silently. THEORY.md §II.1 invariant 1 — typed entry; a malformed
    /// default-form spec is exactly the failure mode the typed-entry
    /// gate exists to reject — and the gate must reject DEFINITIONS as
    /// readily as it rejects CALLS. THEORY.md §II.1 invariant 2 — free
    /// middle; the gate fires inside `parse_params` BEFORE either
    /// expansion strategy runs, so both `Expander::new()` (bytecode) and
    /// `Expander::new_substitute_only()` (substitute) reject the SAME
    /// malformed spec at the SAME gate.
    #[error(
        "compile error in defmacro params: malformed &optional spec, got {got}{}",
        optional_param_malformed_suffix(*position, *reason)
    )]
    OptionalParamMalformed {
        position: usize,
        got: SexpWitness,
        reason: OptionalParamMalformedReason,
    },
    /// A `defmacro` / `defpoint-template` / `defcheck` form had fewer
    /// than 4 list elements: the head keyword must be followed by a
    /// name symbol, a param list, and a body — three required slots
    /// after the head, total length 4. The legacy `LispError::Compile
    /// { form: head.to_string(), message: "(defmacro name (params)
    /// body) required" }` shape named only the failure mode — it
    /// didn't say HOW MANY elements the operator actually wrote, so
    /// an authoring surface that wants to surface "you wrote 2
    /// elements; need 4" had to re-parse the source. The structural
    /// variant carries both: `head` is the head keyword (one of
    /// `"defmacro"` / `"defpoint-template"` / `"defcheck"`); `arity`
    /// is the actual length of the form, including the head element.
    /// Naming the actual arity is the typed-entry gate's structural-
    /// completeness floor (THEORY.md §V.1).
    ///
    /// Sibling of `NonSymbolParam` and `RestParamMissingName` for
    /// the defmacro-syntax-gate's other definition-site failure
    /// modes — those variants fire INSIDE `parse_params`, AFTER the
    /// arity gate has passed; this variant fires AT the arity gate
    /// itself, BEFORE name / params / body validation can run.
    /// Together, the three close `macro_def_from`'s outermost
    /// rejection chain — every distinct failure mode the gate can
    /// emit at the top level becomes a structural variant of
    /// `LispError`, not a `Compile`-shaped substring.
    ///
    /// `head` is `MacroDefHead` — the closed-set typed enum whose
    /// three variants are EXACTLY the three reachable head keywords
    /// (`Defmacro` ⊎ `DefpointTemplate` ⊎ `Defcheck`). Encoding the
    /// closed set as a TYPE makes the constraint that ONLY 3 head
    /// identities are reachable load-bearing in the type system — a
    /// fourth pseudo-head can never drift into the diagnostic at
    /// runtime; consumers (REPL, LSP, `tatara-check`) pattern-match
    /// on `MacroDefHead::Defcheck` etc. directly instead of
    /// substring-matching `head == "defcheck"`. Same posture as
    /// `LispError::CompilerSpecIo.stage: CompilerSpecIoStage` and
    /// `LispError::TemplateInvariant.kind: TemplateInvariantKind`:
    /// the closed set becomes a TYPE rather than a `&'static str`
    /// projection at the helper boundary. `arity` is `usize` because
    /// it is always `list.len()` at the call site (the length of
    /// the form including the head element). Display renders the
    /// head via `MacroDefHead`'s Display impl (which projects
    /// through `MacroDefHead::keyword()` to the canonical `&'static
    /// str` literal), so the legacy `head: &'static str`-shaped
    /// diagnostic rides through byte-for-byte.
    ///
    /// Display preserves the legacy `"(defmacro name (params) body)
    /// required"` substring byte-for-byte: the head is parameterized
    /// in the prefix `compile error in {head}:`, but the example
    /// template literal stays `(defmacro name (params) body)` —
    /// matching the legacy form's small infidelity for non-defmacro
    /// heads (the legacy shape rendered `compile error in
    /// defpoint-template: (defmacro name (params) body) required`)
    /// so authoring tools that substring-grep on the legacy
    /// rendering see no drift; the structural detail (`got {arity}
    /// elements, need 4`) is appended. When a future run gives
    /// `Sexp` source spans, `pos: Option<usize>` lands here in ONE
    /// place and every defmacro-arity site picks up positional
    /// rendering via `crate::diagnostic::format_diagnostic`
    /// mechanically.
    #[error(
        "compile error in {head}: (defmacro name (params) body) required \
         (got {arity} elements, need 4)"
    )]
    DefmacroArity { head: MacroDefHead, arity: usize },
    /// A `defmacro` / `defpoint-template` / `defcheck` form passed the
    /// arity gate (≥4 elements) but its name slot — `list[1]`, the
    /// element directly after the head — wasn't a symbol. The legacy
    /// `LispError::Compile { form: head.to_string(), message: "expected
    /// name symbol" }` shape named only the failure mode — it didn't
    /// say WHAT was found in the name slot, so an authoring surface
    /// that wants to surface "you wrote `5` where a name symbol was
    /// expected" had to re-parse the source. The structural variant
    /// carries both: `head` is the head keyword (one of `"defmacro"` /
    /// `"defpoint-template"` / `"defcheck"`); `got` is the offending
    /// `Sexp::Display` projection of the non-symbol element. Naming
    /// both the head AND the offending element is the typed-entry
    /// gate's structural-completeness floor (THEORY.md §V.1).
    ///
    /// Sibling of `DefmacroArity` and the `parse_params` pair
    /// (`NonSymbolParam`, `RestParamMissingName`) for the
    /// defmacro-syntax-gate's other definition-site failure modes.
    /// Walking a malformed `(defmacro …)` from the outside in, the
    /// gate fires:
    ///   1. `DefmacroArity { head, arity }` if the form has fewer
    ///      than 4 elements (`(defmacro)`, `(defmacro f)`).
    ///   2. `DefmacroNonSymbolName { head, got }` if list[1] isn't a
    ///      symbol (`(defmacro 5 () body)`, `(defmacro :foo () body)`).
    ///   3. Inside `parse_params`: `NonSymbolParam { position, got }`
    ///      and `RestParamMissingName { rest_position, got }`.
    ///
    /// This run lifts step 2; the only remaining `Compile`-shaped
    /// site in `macro_def_from` is the `expected param list` gate
    /// (list[2] is not a list), which is the next move in the same
    /// rejection chain.
    ///
    /// `head` is `MacroDefHead` — same typed closed-set posture as
    /// `DefmacroArity.head`: the three reachable head identities
    /// (`Defmacro` ⊎ `DefpointTemplate` ⊎ `Defcheck`) are encoded as
    /// a TYPE so consumers pattern-match on variant identity rather
    /// than substring-comparing the rendered `head` literal.
    /// `got` is `SexpWitness` — the closed-set typed joint identity
    /// pairing the offending name-slot element's `SexpShape` (the
    /// twelve reachable outermost shapes the reader can produce) with
    /// its `Sexp::Display` projection (`5`, `:foo`, `"name"`,
    /// `(nested)`, etc.). Fourth consumer of the `SexpWitness`
    /// primitive (after `SpliceOutsideList.got`,
    /// `NonSymbolUnquoteTarget.got`, and `NonSymbolParam.got`):
    /// authoring tools (REPL, LSP, `tatara-check`) bind to BOTH
    /// `got.shape` (structurally pattern-matchable on `SexpShape::Int`,
    /// `SexpShape::Keyword`, `SexpShape::List`, etc.) AND `got.display`
    /// (the literal value, renderable verbatim) jointly across the
    /// variant slot.
    ///
    /// Display preserves the legacy `"expected name symbol"` substring
    /// byte-for-byte: the prefix `compile error in {head}:` matches
    /// the legacy `Compile { form: head.to_string(), message:
    /// "expected name symbol" }` shape; the structural detail (`,
    /// got {got}`) is appended. `{got}` flows through
    /// `SexpWitness::Display`, which writes only the `display` field,
    /// so the rendering is byte-for-byte identical to the legacy
    /// `got: String` shape. When a future run gives `Sexp` source
    /// spans, `pos: Option<usize>` lands inside `SexpWitness` in ONE
    /// place and every non-symbol-name site picks up positional
    /// rendering via `crate::diagnostic::format_diagnostic`
    /// mechanically.
    #[error("compile error in {head}: expected name symbol, got {got}")]
    DefmacroNonSymbolName {
        head: MacroDefHead,
        got: SexpWitness,
    },
    /// A `defmacro` / `defpoint-template` / `defcheck` form passed both
    /// the arity gate (≥4 elements) AND the name-symbol gate (list[1]
    /// is a symbol) but its param-list slot — `list[2]`, the third
    /// element after the head — wasn't a list. The legacy
    /// `LispError::Compile { form: head.to_string(), message: "expected
    /// param list" }` shape named only the failure mode — it didn't
    /// say WHAT was found in the param-list slot, so an authoring
    /// surface that wants to surface "you wrote `x` where a param list
    /// was expected" had to re-parse the source. The structural variant
    /// carries both: `head` is the head keyword (one of `"defmacro"` /
    /// `"defpoint-template"` / `"defcheck"`); `got` is the offending
    /// `Sexp::Display` projection of the non-list element. Naming both
    /// the head AND the offending element is the typed-entry gate's
    /// structural-completeness floor (THEORY.md §V.1).
    ///
    /// Sibling of `DefmacroArity`, `DefmacroNonSymbolName`, and the
    /// `parse_params` pair (`NonSymbolParam`, `RestParamMissingName`)
    /// for the defmacro-syntax-gate's other definition-site failure
    /// modes. Walking a malformed `(defmacro …)` from the outside in,
    /// the gate fires:
    ///   1. `DefmacroArity { head, arity }` if the form has fewer
    ///      than 4 elements (`(defmacro)`, `(defmacro f)`).
    ///   2. `DefmacroNonSymbolName { head, got }` if list[1] isn't a
    ///      symbol (`(defmacro 5 () body)`).
    ///   3. `DefmacroNonListParams { head, got }` if list[2] isn't a
    ///      list (`(defmacro f x body)`).
    ///   4. Inside `parse_params`: `NonSymbolParam { position, got }`
    ///      and `RestParamMissingName { rest_position, got }`.
    ///
    /// This run lifts step 3; after it, every inline `LispError::Compile
    /// { … }` triple in `macro_def_from` has been lifted to a structural
    /// variant — the entire `macro_def_from` rejection chain (arity →
    /// name-symbol → param-list → parse_params) is structurally typed
    /// for failure modes, with each variant naming WHICH failure mode
    /// AND WHAT was offending.
    ///
    /// `head` is `MacroDefHead` — same typed closed-set posture as
    /// `DefmacroArity.head` and `DefmacroNonSymbolName.head`. After
    /// this lift all three `Defmacro*` variants share ONE typed
    /// head identity, parallel to how `LispError::CompilerSpecIo`
    /// carries `stage: CompilerSpecIoStage` for the four
    /// disk-persistence (operation, stage) pairs.
    /// `got` is `SexpWitness` — the closed-set typed joint identity
    /// pairing the offending param-list-slot element's `SexpShape`
    /// (the twelve reachable outermost shapes the reader can produce)
    /// with its `Sexp::Display` projection (`x`, `5`, `:foo`,
    /// `"params"`, etc.). Fifth consumer of the `SexpWitness`
    /// primitive (after `SpliceOutsideList.got`,
    /// `NonSymbolUnquoteTarget.got`, `NonSymbolParam.got`, and
    /// `DefmacroNonSymbolName.got`): authoring tools (REPL, LSP,
    /// `tatara-check`) bind to BOTH `got.shape` (structurally
    /// pattern-matchable on `SexpShape::Symbol`, `SexpShape::Int`,
    /// `SexpShape::Keyword`, `SexpShape::String`, etc.) AND
    /// `got.display` (the literal value, renderable verbatim) jointly
    /// across the variant slot. After this lift the entire
    /// `macro_def_from` rejection chain — arity → name-symbol →
    /// param-list — shares ONE typed witness identity at every
    /// `Sexp::Display`-source slot; the only remaining unlifted
    /// rejection points in `macro_def_from`'s typed-entry chain are
    /// `RestParamMissingName.got: Option<String>` (inside
    /// `parse_params`) and `MissingHeadSymbol.got: Option<String>`
    /// (at the outer typed-entry gate).
    ///
    /// Display preserves the legacy `"expected param list"` substring
    /// byte-for-byte: the prefix `compile error in {head}:` matches
    /// the legacy `Compile { form: head.to_string(), message:
    /// "expected param list" }` shape; the structural detail (`, got
    /// {got}`) is appended. `{got}` flows through
    /// `SexpWitness::Display`, which writes only the `display` field,
    /// so the rendering is byte-for-byte identical to the legacy
    /// `got: String` shape. When a future run gives `Sexp` source
    /// spans, `pos: Option<usize>` lands inside `SexpWitness` in ONE
    /// place and every non-list-params site picks up positional
    /// rendering via `crate::diagnostic::format_diagnostic`
    /// mechanically.
    #[error("compile error in {head}: expected param list, got {got}")]
    DefmacroNonListParams {
        head: MacroDefHead,
        got: SexpWitness,
    },
    /// `T::compile_from_sexp` (the `TataraDomain` trait default) was
    /// passed something that isn't a list — a bare atom (`5`, `:foo`,
    /// `"x"`, `name`) where a top-level `(KEYWORD …)` form was
    /// expected. The legacy `LispError::Compile { form:
    /// keyword.to_string(), message: "expected list form" }` shape
    /// named only the failure mode and the keyword, and required
    /// authoring tools (REPL, LSP, `tatara-check`) to substring-grep
    /// the rendered message to recognize this specific gate. The
    /// structural variant carries `keyword` as a first-class field so
    /// consumers pattern-match on the variant and bind directly to
    /// the keyword instead of substring-parsing.
    ///
    /// Sibling of `HeadMismatch` — both are typed-entry rejection
    /// gates inside the trait default `compile_from_sexp` walking a
    /// malformed form from the outside in:
    ///   1. `NotAListForm { keyword }` if the form isn't a list at
    ///      all (`5`, `:foo`, `"x"`, `name` — bare atoms).
    ///   2. `LispError::Compile { form, message: "missing head
    ///      symbol" }` (NOT YET LIFTED) if the list is empty or
    ///      list[0] isn't a symbol (`()`, `(5 …)`, `(:foo …)`).
    ///   3. `HeadMismatch { keyword, got }` if list[0] is a symbol
    ///      but doesn't match `T::KEYWORD` (`(other-name …)`).
    ///
    /// After this lift step 1 is structural; the `missing head
    /// symbol` gate is the next move in the same rejection chain
    /// (its own structural-variant lift, parallel to how the
    /// `defmacro_*` family was lifted gate-by-gate).
    ///
    /// `keyword` is `&'static str` because every call site passes
    /// `Self::KEYWORD` from the trait default — a compile-time
    /// literal sourced from the `#[tatara(keyword = "...")]` derive
    /// attribute (or hand-written const). Using a static slot makes
    /// that compile-time guarantee load-bearing in the type system
    /// (a typo in the keyword can never drift into the diagnostic at
    /// runtime — the type system is the floor, same posture as
    /// `HeadMismatch.keyword`, `TypeMismatch.expected`, and the
    /// `Defmacro*.head` family).
    ///
    /// Display preserves the legacy `"expected list form"` substring
    /// AND the `"compile error in {keyword}:"` prefix byte-for-byte
    /// — `"compile error in {keyword}: expected list form"` — so
    /// existing consumer assertions (e.g., the
    /// `compile_from_sexp_emits_compile_for_non_list_form` test
    /// against `MonitorSpec`, `tatara-check`'s diagnostic capture)
    /// pass unchanged. The variant carries no `got` slot because the
    /// offending value's type is itself the diagnostic — `5` /
    /// `:foo` / `"x"` / `name` all reduce to the same failure mode
    /// (not a list); naming the type would be redundant with what
    /// the source already shows. When a future run gives `Sexp`
    /// source spans, `pos: Option<usize>` lands here in ONE place
    /// and every not-a-list-form site picks up positional rendering
    /// via `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {keyword}: expected list form")]
    NotAListForm { keyword: &'static str },
    /// `T::compile_from_sexp` was passed a list whose head can't be
    /// projected to a symbol — either the list is empty (`()` — there
    /// is no first element) or its first element exists but isn't a
    /// symbol (`(5 …)`, `(:foo …)`, `("x" …)`, `((nested) …)`). The
    /// legacy `LispError::Compile { form: keyword.to_string(),
    /// message: "missing head symbol" }` shape collapsed both
    /// sub-modes into one diagnostic — a `()` form and a `(5 …)` form
    /// produced byte-identical messages, so an authoring surface that
    /// wants to surface "your form is empty" vs. "your form's head is
    /// `5`, not a symbol" had to re-parse the source. The structural
    /// variant carries `got: Option<String>` so the two sub-modes are
    /// distinguishable structurally — `None` for the empty-list case,
    /// `Some(g)` for the present-but-not-symbol case where `g` is
    /// the offending head's `Sexp::Display` projection. Naming both
    /// the failure mode AND the structural detail (empty vs. offending
    /// head) is the typed-entry gate's structural-completeness floor
    /// (THEORY.md §V.1).
    ///
    /// Sibling of `NotAListForm { keyword }` and `HeadMismatch
    /// { keyword, got }` — together the three close every distinct
    /// failure mode the trait-default `compile_from_sexp` rejection
    /// chain can emit. Walking a malformed `(KEYWORD …)` form from
    /// the outside in:
    ///   1. `NotAListForm { keyword }` — the form isn't a list at all
    ///      (`5`, `:foo`, `"x"`, `name` — bare atoms).
    ///   2. `MissingHeadSymbol { keyword, got }` — the form is a
    ///      list but list[0] doesn't exist (`()`) or isn't a symbol
    ///      (`(5 …)`, `(:foo …)`).
    ///   3. `HeadMismatch { keyword, got }` — list[0] is a symbol
    ///      but doesn't match `T::KEYWORD` (`(other-name …)`).
    ///
    /// After this lift the entire `compile_from_sexp` rejection chain
    /// is structurally typed for failure modes — every distinct
    /// rejection binds to ONE structural variant of `LispError`, not
    /// a `Compile`-shaped substring. The `got: Option<String>`
    /// posture parallels `RestParamMissingName.got: Option<String>`:
    /// the failure mode IS one ("head can't be projected to a
    /// symbol"); the bifurcation is in the renderable detail (empty
    /// vs. present-but-wrong-type), not in what the gate rejects, so
    /// the two sub-modes share ONE variant rather than splitting into
    /// near-identical siblings.
    ///
    /// `keyword` is `&'static str` because every call site passes
    /// `Self::KEYWORD` from the trait default — a compile-time literal
    /// sourced from the `#[tatara(keyword = "...")]` derive attribute
    /// (or hand-written const). Using a static slot makes that
    /// compile-time guarantee load-bearing in the type system (a typo
    /// in the keyword can never drift into the diagnostic at runtime —
    /// the type system is the floor, same posture as
    /// `NotAListForm.keyword`, `HeadMismatch.keyword`, and the
    /// `Defmacro*.head` family). `got` is `Option<SexpWitness>` — the
    /// SEVENTH consumer of the typed `SexpWitness` primitive (after
    /// `SpliceOutsideList.got`, `NonSymbolUnquoteTarget.got`,
    /// `NonSymbolParam.got`, `DefmacroNonSymbolName.got`,
    /// `DefmacroNonListParams.got`, and `RestParamMissingName.got`).
    /// The `Option`-wrap bifurcates structurally between "missing
    /// entirely" (`None`, when the list is empty) and "present but
    /// malformed" (`Some(SexpWitness)`, when the head exists but
    /// isn't a symbol); the typed witness lands on the `Some` arm
    /// only — same posture as `RestParamMissingName.got:
    /// Option<SexpWitness>`. With this lift EVERY Sexp-display-source
    /// `got` slot in the substrate carries ONE typed identity:
    /// the typed-entry / template-gate / defmacro-syntax-gate
    /// rejection surface is structurally unified end-to-end across
    /// ALL `got: <T>` slots where `<T>` projects from `Sexp::Display`.
    ///
    /// Display preserves the legacy `"missing head symbol"` substring
    /// AND the `"compile error in {keyword}:"` prefix byte-for-byte —
    /// `"compile error in {keyword}: missing head symbol"` is the
    /// stable prefix; the structural detail (`(empty list)` for
    /// `None`, `(got {g})` for `Some(g)`) is appended in a
    /// parenthetical, parallel to how `RestParamMissingName` appends
    /// `(rest marker at position {n}, got {g})` /
    /// `(rest marker at position {n}, none provided)` and how
    /// `SpliceOutsideList` appends `(got ,@{got})`. The `{g}` slot
    /// flows through `SexpWitness::Display`, which writes only the
    /// `display` field, so the rendering is byte-for-byte identical
    /// to the pre-lift `Option<String>` shape. When a future run
    /// gives `Sexp` source spans, `pos: Option<usize>` lands inside
    /// `SexpWitness` in ONE place and every missing-head-symbol site
    /// picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error(
        "compile error in {keyword}: missing head symbol{}",
        missing_head_symbol_suffix(got.as_ref().map(|w| w.display.as_str()))
    )]
    MissingHeadSymbol {
        keyword: &'static str,
        got: Option<SexpWitness>,
    },
    /// `compile_named_from_forms::<T>` — driving every `(KEYWORD NAME …)`
    /// positional-name surface (`(defpoint NAME …)`, `(defalertpolicy
    /// NAME …)`) — was passed a list whose head matched `T::KEYWORD` but
    /// whose tail had no NAME slot at all. `(defpoint)` — list.len() == 1
    /// (just the keyword); the gate fires before NAME extraction. The
    /// legacy `LispError::Compile { form: T::KEYWORD.to_string(),
    /// message: format!("expected ({} NAME …)", T::KEYWORD) }` shape
    /// named the failure mode AND the keyword by embedding both into a
    /// formatted message — required authoring tools (REPL, LSP,
    /// `tatara-check`) to substring-grep the rendered diagnostic to
    /// recognize this specific gate. The structural variant carries
    /// `keyword` as a first-class field so consumers pattern-match on
    /// the variant and bind to the keyword directly instead of
    /// substring-parsing.
    ///
    /// Sibling of `NotAListForm { keyword }`, `MissingHeadSymbol
    /// { keyword, got }`, and `HeadMismatch { keyword, got }` — those
    /// close the trait-default `compile_from_sexp` rejection chain
    /// (the keyword-only entry point, `(KEYWORD :k v …)`); this
    /// variant opens the parallel `compile_named_from_forms`
    /// rejection chain (the positional-name entry point, `(KEYWORD
    /// NAME :k v …)`). Walking a malformed `(KEYWORD NAME …)` form
    /// from the outside in:
    ///   1. `NamedFormMissingName { keyword }` — the form passes the
    ///      keyword-head match but has no NAME slot (`(defpoint)`).
    ///   2. `LispError::Compile { form, message: "positional NAME
    ///      must be a symbol or string" }` (NOT YET LIFTED) — the
    ///      form has a NAME slot but it's not a symbol or string
    ///      (`(defpoint 5 …)`, `(defpoint :foo …)`, `(defpoint
    ///      (nested) …)`).
    ///   3. Inside `T::compile_from_args(&list[2..])` — derive-
    ///      generated kwargs handling with its own structural
    ///      variants (`UnknownKwarg`, `MissingKwarg`, etc.).
    ///
    /// This run lifts step 1; step 2 is the next move in the same
    /// rejection chain (its own structural-variant lift, parallel to
    /// how the `compile_from_sexp` chain was lifted gate-by-gate
    /// across `092a2b2` (`NotAListForm`) and `b3e941e`
    /// (`MissingHeadSymbol`)).
    ///
    /// `keyword` is `&'static str` because every call site passes
    /// `T::KEYWORD` from `compile_named_from_forms` — a compile-time
    /// literal sourced from the `#[tatara(keyword = "...")]` derive
    /// attribute (or hand-written const). Using a static slot makes
    /// that compile-time guarantee load-bearing in the type system —
    /// a typo in the keyword can never drift into the diagnostic at
    /// runtime, the type system is the floor, same posture as
    /// `NotAListForm.keyword`, `MissingHeadSymbol.keyword`,
    /// `HeadMismatch.keyword`, `TypeMismatch.expected`, and the
    /// `Defmacro*.head` family. The variant carries no `arity` slot
    /// because the offending form's structure is invariant — every
    /// trigger has list.len() == 1 exactly (list[0] is the keyword,
    /// no list[1] for NAME); naming a fixed value would be
    /// misleading data, parallel to how `NotAListForm` carries no
    /// `got` slot (the form's not-a-list type is itself the
    /// diagnostic).
    ///
    /// Display matches the legacy `Compile`-shaped diagnostic
    /// byte-for-byte — `"compile error in {keyword}: expected
    /// ({keyword} NAME …)"` (note: `…` is the Unicode horizontal
    /// ellipsis U+2026, preserved verbatim from the legacy
    /// `format!("expected ({} NAME …)", T::KEYWORD)` shape) — so
    /// existing consumer assertions (`tatara-check`'s diagnostic
    /// capture, REPL substring-greps) pass unchanged. When a future
    /// run gives `Sexp` source spans, `pos: Option<usize>` lands
    /// here in ONE place and every named-form-missing-name site
    /// picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {keyword}: expected ({keyword} NAME …)")]
    NamedFormMissingName { keyword: &'static str },
    /// `compile_named_from_forms::<T>` was passed a `(KEYWORD NAME …)` form
    /// whose NAME slot exists but isn't projectable to a symbol or string —
    /// `(defpoint 5 …)`, `(defpoint :foo …)`, `(defpoint (nested) …)`. Gate
    /// 2 of the same rejection chain `NamedFormMissingName` opens: that
    /// variant fires when there is no NAME slot at all (`(defpoint)` —
    /// list.len() == 1); this variant fires when the NAME slot exists but
    /// is wrong-typed. Together the two close `compile_named_from_forms`'s
    /// outer rejection chain — every typed-entry rejection mode in the
    /// positional-name authoring surface is now a structural variant of
    /// `LispError`, not a `Compile`-shaped substring.
    ///
    /// `keyword` is `&'static str` because every call site passes
    /// `T::KEYWORD` — a compile-time literal sourced from the
    /// `#[tatara(keyword = "...")]` derive attribute (or hand-written
    /// const); a typo in the keyword can never drift into the diagnostic
    /// at runtime. `got` is the typed closed-set `SexpShape` enum —
    /// the twelve reachable Sexp outermost shapes encoded as variant
    /// identities so the SexpShape that the typed-entry gate observed
    /// is load-bearing data in the type system. Same posture as
    /// `TypeMismatch.got: SexpShape`: consumers pattern-match on
    /// `SexpShape::Int` etc. directly instead of substring-matching
    /// `got == "int"`. Encoding the closed set as a TYPE makes the
    /// compile-time guarantee load-bearing, parallel to
    /// `NotAListForm.keyword`, `MissingHeadSymbol.keyword`,
    /// `HeadMismatch.keyword`, and the `Defmacro*.head` family.
    ///
    /// Display preserves the legacy `"positional NAME must be a symbol
    /// or string"` substring AND the `"compile error in {keyword}:"`
    /// prefix byte-for-byte; the structural detail (`(got {got})`) is
    /// appended in a parenthetical, parallel to how `MissingHeadSymbol`
    /// appends `(got {g})` / `(empty list)` and how `RestParamMissingName`
    /// appends `(rest marker at position {n}, got {g})`. When a future
    /// run gives `Sexp` source spans, `pos: Option<usize>` lands here in
    /// ONE place and every named-form-non-symbol-name site picks up
    /// positional rendering via `crate::diagnostic::format_diagnostic`
    /// mechanically.
    #[error("compile error in {keyword}: positional NAME must be a symbol or string (got {got})")]
    NamedFormNonSymbolName {
        keyword: &'static str,
        got: SexpShape,
    },
    /// `rewrite_typed::<T>` — the typed-exit gate of the self-optimization
    /// primitive (THEORY.md §II.1 invariant 3) — was handed a rewriter
    /// closure whose output, after typed round-trip through canonical JSON,
    /// did not project to `Sexp::List`. The round-trip contract is:
    /// serialize `T` → `Sexp::List` (alternating kwargs), hand the list
    /// to the rewriter `F`, re-enter `T::compile_from_args` over the
    /// returned list's items. A non-list result violates that contract —
    /// the gate fires before `compile_from_args` runs, so a wrong-shaped
    /// rewriter output is rejected at the typed-exit boundary rather than
    /// confusingly later inside the kwargs decoder.
    ///
    /// Mirror at the typed-exit boundary of the typed-entry-side
    /// `NamedFormNonSymbolName` lift: the latter rejects a wrong-typed
    /// NAME slot at `compile_named_from_forms::<T>`'s entry; this variant
    /// rejects a wrong-typed rewriter output at `rewrite_typed::<T>`'s
    /// exit. Both round-trip the same compile-time `T::KEYWORD` projection
    /// into the variant's `keyword` slot, so authoring tools (REPL, LSP,
    /// `tatara-check`) bind on variant identity at both boundaries of the
    /// self-optimization primitive rather than substring-grepping the
    /// rendered diagnostic.
    ///
    /// `keyword` is `&'static str` because every call site passes
    /// `T::KEYWORD` from `rewrite_typed::<T>` — a compile-time literal
    /// sourced from the `#[tatara(keyword = "...")]` derive attribute (or
    /// hand-written const). Using a static slot makes that compile-time
    /// guarantee load-bearing in the type system — a typo can never drift
    /// into the diagnostic at runtime, the type system is the floor, same
    /// posture as `NotAListForm.keyword`, `MissingHeadSymbol.keyword`,
    /// `HeadMismatch.keyword`, `NamedFormMissingName.keyword`, and
    /// `NamedFormNonSymbolName.keyword`.
    ///
    /// `got` is `SexpWitness` — the closed-set typed joint identity
    /// pairing the offending rewriter output's `SexpShape` (the twelve
    /// reachable Sexp outermost shapes the rewriter closure can produce)
    /// with its `Sexp::Display` projection (the literal value the rewriter
    /// actually returned — `42`, `:foo`, `"bad"`, `notify-ref`, `()`,
    /// etc.). EIGHTH consumer of the typed `SexpWitness` primitive
    /// introduced in `error.rs`'s `SpliceOutsideList.got` lift, and the
    /// FIRST consumer on the typed-EXIT boundary — sibling lifts of
    /// `SpliceOutsideList.got: SexpWitness`, `NonSymbolUnquoteTarget.got:
    /// SexpWitness`, `NonSymbolParam.got: SexpWitness`,
    /// `DefmacroNonSymbolName.got: SexpWitness`,
    /// `DefmacroNonListParams.got: SexpWitness`,
    /// `RestParamMissingName.got: Option<SexpWitness>`, and
    /// `MissingHeadSymbol.got: Option<SexpWitness>` close the typed-ENTRY
    /// rejection surface across the substrate's seven entry-side gates.
    /// This eighth lift extends the typed-identity unification contract
    /// across BOTH boundaries of the typed-IR algebra
    /// (THEORY.md §II.1 invariant 1 + invariant 3) — every
    /// `Sexp::Display`-source `got` slot in the substrate, regardless of
    /// whether the rejection fires at typed-ENTRY (compile_from_sexp
    /// chain, template-gate, defmacro-syntax-gate, parse_params walker)
    /// or typed-EXIT (rewrite_typed's `Sexp::List`-contract gate), now
    /// shares ONE typed witness identity at the variant slot. Authoring
    /// tools (REPL, LSP, `tatara-check`) bind to BOTH `got.shape`
    /// (structurally pattern-matchable on `SexpShape::Int` etc.) AND
    /// `got.display` (the literal value, renderable verbatim) jointly
    /// for a typed-exit-side rejection too — no projection-to-`String`
    /// at the helper boundary loses the structural identity. Promotes
    /// the legacy `got: String` shape parallel to how the seven entry-
    /// side lifts promoted theirs.
    ///
    /// Display matches the legacy `Compile`-shaped diagnostic byte-for-
    /// byte — `"compile error in {keyword}: rewriter must return a list;
    /// got {got}"` — so existing consumer assertions (`tatara-check`'s
    /// diagnostic capture, REPL substring-greps that match on `"rewriter
    /// must return a list; got "`) pass unchanged across the lift. The
    /// `{got}` slot flows through `SexpWitness::Display`, which writes
    /// only the `display` field, so the rendering is byte-for-byte
    /// identical to the pre-lift `got: String` shape. When a future run
    /// gives `Sexp` source spans, `pos: Option<usize>` lands inside
    /// `SexpWitness` in ONE place and every rewriter-non-list site
    /// picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {keyword}: rewriter must return a list; got {got}")]
    RewriterNonList {
        keyword: &'static str,
        got: SexpWitness,
    },
    /// `serde_json::to_value` of a typed `T` value (any registered
    /// `TataraDomain`) errored. Two sites share this failure mode:
    /// `register::<T>`'s registry-dispatch closure (the registered
    /// handler serializes the just-typed value to JSON for the
    /// dispatcher) and `rewrite_typed::<T>`'s round-trip prelude (the
    /// self-optimization primitive serializes its input to JSON before
    /// projecting it to a `Sexp::List` for the rewriter closure). Both
    /// funnel through `serialize_to_json_err::<T>` so the type-level
    /// `T::KEYWORD` projection is mechanically threaded into the
    /// `keyword` slot, parallel to how `rewriter_non_list_err::<T>`
    /// threads `T::KEYWORD` into `RewriterNonList.keyword`.
    ///
    /// Mirror at the typed-exit boundary of the typed-entry-side
    /// `from_value` failure path: `extract_via_serde` /
    /// `extract_optional_via_serde` / `extract_vec_via_serde` route
    /// through `deserialize_err` / `deserialize_item_err`, which now
    /// produce the structural `LispError::KwargDeserialize { key, idx,
    /// message }` variant — the typed-entry-side sibling of this lift.
    /// After both lifts BOTH directions of the JSON-projection round-
    /// trip — `to_value` (typed-exit, keyword-keyed) AND `from_value`
    /// (typed-entry, key-keyed) — are structurally typed; there are
    /// zero `LispError::Compile { ... }` construction sites left in
    /// `tatara-lisp/src/domain.rs`.
    ///
    /// Sibling of `RewriterNonList { keyword, got }` for the
    /// `rewrite_typed::<T>` rejection chain — that variant fires when
    /// the rewriter's OUTPUT is not a list; this variant fires when
    /// the round-trip's INPUT (the typed value) fails to project to
    /// JSON at all. Together with `RewriterNonList`, every distinct
    /// `to_value`-side rejection mode in the self-optimization
    /// primitive and the registry-dispatch closure binds to ONE
    /// structural variant of `LispError`, not a `Compile`-shaped
    /// substring.
    ///
    /// `keyword` is `&'static str` because every call site projects
    /// `T::KEYWORD` via `serialize_to_json_err::<T>` — a compile-time
    /// literal sourced from the `#[tatara(keyword = "...")]` derive
    /// attribute (or hand-written const). Using a static slot makes
    /// that compile-time guarantee load-bearing in the type system —
    /// a typo can never drift into the diagnostic at runtime, the
    /// type system is the floor, same posture as
    /// `RewriterNonList.keyword`, `NamedFormMissingName.keyword`,
    /// `NamedFormNonSymbolName.keyword`, `NotAListForm.keyword`,
    /// `MissingHeadSymbol.keyword`, `HeadMismatch.keyword`, and the
    /// `Defmacro*.head` family. `message` is `String` because it
    /// carries the `serde_json::Error::Display` projection (errors
    /// render `expected … at line L column C` shapes — arbitrary text
    /// from the underlying `serde_json::Error`). Carrying the rendered
    /// message rather than a `#[source] serde_json::Error` keeps the
    /// variant's structural shape parallel to every other String-
    /// carrying variant in this enum — every consumer renders via
    /// Display, none consumes the underlying error chain.
    ///
    /// Display matches the legacy `Compile`-shaped diagnostic byte-
    /// for-byte — `"compile error in {keyword}: serialize: {message}"`
    /// — so existing consumer assertions (`tatara-check`'s diagnostic
    /// capture, REPL substring-greps that match on `"serialize: "`)
    /// pass unchanged across the lift. When a future run gives `Sexp`
    /// source spans, `pos: Option<usize>` lands here in ONE place and
    /// every domain-serialize site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {keyword}: serialize: {message}")]
    DomainSerialize {
        keyword: &'static str,
        message: String,
    },
    /// `serde_json::from_value::<T>` of a kwarg's canonical-JSON projection
    /// errored. Two distinct sites share this failure mode through ONE
    /// structural variant whose data carries the typed closed-set
    /// `KwargPath` enum directly — `KwargPath::Named(key)` for the scalar
    /// / `Option<T>` path, `KwargPath::Item { key, idx }` for the per-item
    /// path inside a `Vec<T>` kwarg. The bifurcation lives inside the
    /// typed enum's variant identity, not in a sibling `idx: Option<usize>`
    /// slot:
    ///
    ///   1. `extract_via_serde` (required) and `extract_optional_via_serde`
    ///      (optional) — kwarg-keyed `from_value` failures at the scalar /
    ///      `Option<T>` path. `path: KwargPath::Named(key)`; the failure
    ///      binds to the kwarg slot identity ONLY (`:{key}`).
    ///   2. `extract_vec_via_serde` (per-item) — kwarg-AND-index-keyed
    ///      `from_value` failures inside a `Vec<T>` kwarg's items.
    ///      `path: KwargPath::Item { key, idx }`; the failure binds to the
    ///      kwarg slot AND the failing item index (`:{key}[{i}]`).
    ///
    /// Mirror at the typed-entry JSON boundary of the typed-exit-side
    /// `DomainSerialize { keyword, message }` lift: the latter rejects a
    /// `to_value::<T>` failure (typed-exit, keyword-keyed, sourced from
    /// `T::KEYWORD` and so `&'static str`); this variant rejects a
    /// `from_value::<T>` failure (typed-entry, kwargs-path-keyed, sourced
    /// from the runtime kwarg lookup and carried as a typed `KwargPath`).
    /// Together the two close the JSON-projection boundary of the
    /// typed-domain surface — every distinct `serde_json` failure mode at
    /// the typed-domain boundary binds to ONE structural variant of
    /// `LispError`, not a `Compile`-shaped substring.
    ///
    /// Sibling of `TypeMismatch.form: KwargPath`: both kwargs-path-keyed
    /// typed-entry rejection modes now carry the SAME typed kwargs-path
    /// identity inside their variant's data shape. The `(key, idx:
    /// Option<usize>)` bifurcation collapses into `KwargPath`'s variant
    /// identity — `Named` vs. `Item` — so the invalid combination
    /// `(key: "", idx: Some(0))` for a scalar path (or any combination that
    /// invented a fourth sub-mode) becomes structurally unrepresentable
    /// rather than re-asserted at the helper boundary via runtime
    /// `Option::is_some` comparison. Same closed-set posture as
    /// `LispError::TypeMismatch.form: KwargPath`,
    /// `LispError::Defmacro*.head: MacroDefHead`,
    /// `LispError::UnboundTemplateVar.prefix: UnquoteForm`,
    /// `LispError::CompilerSpecIo.stage: CompilerSpecIoStage`, and
    /// `LispError::TemplateInvariant.kind: TemplateInvariantKind`.
    ///
    /// `path` is `KwargPath` — the closed-set typed enum whose variants
    /// are EXACTLY the reachable kwargs-path shapes (`Named(String)` /
    /// `Item { key: String, idx: usize }` / `Slot(usize)`). The runtime
    /// `kwarg lookup` source-of-key is carried inside the typed enum's
    /// `String` payload; the per-item-index bifurcation is the enum's
    /// `Named` vs. `Item` variant identity, not a sibling Option slot.
    /// `message` is `String` because it carries the
    /// `serde_json::Error::Display` projection (errors render `expected …
    /// at line L column C` shapes — arbitrary text from the underlying
    /// `serde_json::Error`); carrying the rendered message rather than a
    /// `#[source] serde_json::Error` keeps the variant's structural shape
    /// parallel to every other String-carrying variant in this enum
    /// (`DomainSerialize.message`, `Compile.message`).
    ///
    /// `message` carries the raw `serde_json::Error::Display` projection
    /// — NO `"deserialize: "` prefix in the field, the prefix is in the
    /// `Display` rendering — so consumers that pattern-match on
    /// `message` get the underlying diagnostic unchanged, parallel to how
    /// `DomainSerialize.message` carries the raw `serde_json` projection
    /// (the `"serialize: "` prefix lives in Display, not in the slot).
    ///
    /// Display matches the legacy `Compile`-shaped diagnostic byte-for-
    /// byte across both sub-modes via `KwargPath`'s Display projection:
    /// `"compile error in :{key}: deserialize: {message}"` for
    /// `KwargPath::Named`, `"compile error in :{key}[{idx}]: deserialize:
    /// {message}"` for `KwargPath::Item` — so existing substring-grep
    /// consumers (`tatara-check`'s diagnostic capture, REPL substring-greps
    /// that match on `"deserialize: "`, `":steps[1]"`, `":level"`) pass
    /// unchanged across the lift. When a future run gives `Sexp` source
    /// spans, `pos: Option<usize>` lands here in ONE place and every
    /// kwarg-deserialize site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {path}: deserialize: {message}")]
    KwargDeserialize { path: KwargPath, message: String },
    /// `compiler_spec.rs`'s disk-persistence surface emitted an
    /// I/O or serde failure. Four call sites in `compiler_spec.rs`
    /// share this failure mode through ONE structural variant keyed
    /// on the closed-set `CompilerSpecIoStage` enum (`realize_to_disk`
    /// × {serialize, write} ⊎ `load_from_disk` × {read, deserialize}).
    ///
    /// Encoding the (operation, stage) pair as ONE typed enum (rather
    /// than two `&'static str` slots `operation` × `stage`) makes the
    /// constraint that ONLY 4 of the 2×4 = 8 hypothetical pairs are
    /// reachable load-bearing in the type system — a typo like
    /// `(operation: "load_from_disk", stage: "write")` becomes
    /// structurally unrepresentable rather than re-asserted at the
    /// helper boundary via runtime string comparison. Same posture as
    /// `MacroDefHead` in `macro_expand.rs`: the closed set becomes a
    /// TYPE, and rustc's exhaustiveness check is the future invariant-
    /// keeper. Adding a new disk-persistence operation (e.g.,
    /// `load_from_str`) requires extending `CompilerSpecIoStage`,
    /// which rustc-enforces matching at every projection site
    /// (`operation()` / `label()`).
    ///
    /// Mirror at the disk boundary of the typed-domain JSON-projection
    /// round-trip's `DomainSerialize` / `KwargDeserialize` sibling pair
    /// at the in-memory kwarg boundary: those variants reject
    /// `to_value::<T>` / `from_value::<T>` failures at the typed-domain
    /// boundary; this variant rejects file I/O + top-level JSON
    /// failures at the disk boundary. After this lift, every distinct
    /// failure mode in `tatara-lisp/src/compiler_spec.rs`'s persistence
    /// surface is structurally typed; there are zero
    /// `LispError::Compile { ... }` construction sites left in
    /// `tatara-lisp/src/compiler_spec.rs`.
    ///
    /// `stage` is `CompilerSpecIoStage` — a closed-set typed enum
    /// whose `operation()` and `label()` projections feed the Display
    /// rendering — so the compile-time guarantee on BOTH slots is
    /// load-bearing in the type system. `message` is `String` because
    /// it carries the underlying error's `Display` projection
    /// (`serde_json::Error` for serialize / deserialize, `std::io::Error`
    /// for read / write — arbitrary text); carrying the rendered
    /// message rather than a `#[source]` chain keeps the variant's
    /// structural shape parallel to every other String-carrying variant
    /// in this enum (`DomainSerialize.message`, `KwargDeserialize.message`,
    /// `Compile.message`).
    ///
    /// `message` carries the raw underlying-error `Display` projection
    /// — NO `"{stage}: "` prefix in the field, the prefix is in the
    /// `Display` rendering — so consumers that pattern-match on
    /// `message` get the underlying diagnostic unchanged, parallel to
    /// how `DomainSerialize.message` carries the raw `serde_json`
    /// projection (the `"serialize: "` prefix lives in Display, not in
    /// the slot) and `KwargDeserialize.message` carries the raw
    /// `serde_json` projection (the `"deserialize: "` prefix lives in
    /// Display, not in the slot).
    ///
    /// Display matches the legacy `Compile`-shaped diagnostic byte-for-
    /// byte across all four stages — `"compile error in {operation}:
    /// {stage}: {message}"` where `{operation}` is `stage.operation()`
    /// and `{stage}` is `stage.label()` — so existing consumer
    /// assertions (`tatara-check`'s diagnostic capture, REPL substring-
    /// greps that match on `"realize_to_disk"`, `"load_from_disk"`,
    /// `"serialize: "`, `"write: "`, `"read: "`, `"deserialize: "`)
    /// pass unchanged across the lift. When a future run gives `Sexp`
    /// source spans, `pos: Option<usize>` lands here in ONE place
    /// (though the disk surface is non-positional — failures originate
    /// from file I/O / serde, not from a Sexp slot — so the field
    /// would stay `None` at every call site, the variant joining the
    /// `position_is_none_for_non_positional_variants` cohort).
    #[error("compile error in {}: {}: {message}", stage.operation(), stage.label())]
    CompilerSpecIo {
        stage: CompilerSpecIoStage,
        message: String,
    },
    /// `apply_compiled`'s bytecode-runtime invariant violation. Four call
    /// sites in `macro_expand.rs::apply_compiled` share this failure mode
    /// through ONE structural variant keyed on the closed-set
    /// `TemplateInvariantKind` enum. Every violation here is a
    /// COMPILER-INTERNAL bug — the bytecode that drives `apply_compiled`
    /// is produced by `compile_template` / `compile_node` in this same
    /// module, and a well-formed bytecode never references an
    /// out-of-bounds param index (Subst / Splice gates) nor leaves the
    /// runtime stack unbalanced at the final pop (EndList / no-value
    /// gates).
    ///
    /// Encoding the four failure modes as ONE typed enum (rather than a
    /// free-form `message: String` slot) makes the constraint that ONLY
    /// 4 distinct violations are reachable load-bearing in the type
    /// system — a regression that drifts the failure mode (e.g. a fifth
    /// "wrong opcode" gate added without a `TemplateInvariantKind`
    /// extension) becomes a `match` compile error at the projection site,
    /// not a substring-grep regression that ships. Same posture as
    /// `CompilerSpecIoStage` for `CompilerSpecIo`: the closed set becomes
    /// a TYPE, not a `matches!` literal in the helper. The index slot of
    /// the Subst / Splice gates lives INSIDE the variant
    /// (`SubstBadIndex(usize)` / `SpliceBadIndex(usize)`) rather than on
    /// the outer variant as `op_index: Option<usize>`, so the invalid
    /// combination `EndListEmptyStack { op_index: Some(_) }` is
    /// structurally unrepresentable — the type system encodes "this gate
    /// has an index, that gate does not."
    ///
    /// Display matches the legacy `Compile`-shaped diagnostic
    /// byte-for-byte across all four kinds — `"compile error in
    /// {macro_name}: {kind.message()}"` — so existing consumer assertions
    /// (`tatara-check`'s diagnostic capture, REPL substring-greps that
    /// match on `"compiled template referenced bad param index"`,
    /// `"compiled template referenced bad splice index"`, `"compiled
    /// template: EndList with empty stack"`, `"compiled template produced
    /// no value"`) pass unchanged across the lift.
    ///
    /// `macro_name` is `String` because it comes from arbitrary source
    /// (the call-site head symbol). `kind` is `TemplateInvariantKind` —
    /// a closed-set typed enum whose `message()` projection feeds the
    /// Display rendering.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the closed set
    /// of bytecode-invariant failure modes becomes a TYPE rather than a
    /// runtime string-comparison-and-format dance. THEORY.md §VI.1 —
    /// generation over composition; the typed enum lands the structural-
    /// completeness floor for the bytecode-runtime surface, parallel to
    /// how `CompilerSpecIoStage` lands the structural-completeness floor
    /// for the disk-persistence surface and `MacroDefHead` lands it for
    /// the macro-definition-head closed set. THEORY.md §II.1 invariant 5
    /// (composition preserves proofs): a well-formed bytecode invariant
    /// is the proof that drives the interpreter; the structural variant
    /// makes the proof's REJECTION shape first-class.
    #[error("compile error in {macro_name}: {}", kind.message())]
    TemplateInvariant {
        macro_name: String,
        kind: TemplateInvariantKind,
    },
}

/// Closed-set identifier for the (operation, stage) pair of a
/// `LispError::CompilerSpecIo` failure. Encodes the four reachable
/// pairs in `tatara-lisp/src/compiler_spec.rs`'s disk-persistence
/// surface — `realize_to_disk` × {serialize, write} ⊎ `load_from_disk`
/// × {read, deserialize} — as a typed enum, so invalid combinations
/// like `(load_from_disk, write)` or `(realize_to_disk, deserialize)`
/// are structurally unrepresentable rather than re-asserted at the
/// helper boundary via runtime string comparison.
///
/// Same posture as `MacroDefHead` in `macro_expand.rs`: the closed set
/// becomes a TYPE, not a `matches!` literal AND a triplicate
/// `match operation { ... }` projection inside each error helper. The
/// `operation()` / `label()` projections feed the
/// `LispError::CompilerSpecIo` Display rendering directly via the
/// `#[error(...)]` annotation; adding a new disk-persistence operation
/// (e.g., `load_from_str` for in-memory loads) requires extending this
/// enum, which rustc-enforces matching at every projection site.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the closed set
/// of (operation, stage) pairs becomes a TYPE rather than a runtime
/// string-comparison-and-format dance. THEORY.md §VI.1 — generation
/// over composition; the typed enum lands the structural-completeness
/// floor for the disk-persistence surface, parallel to how
/// `MacroDefHead` lands the structural-completeness floor for the
/// macro-definition-head closed set.
///
/// `#[derive(tatara_lisp_derive::ClosedSet)]` emits the
/// substrate-wide `impl crate::ClosedSet for CompilerSpecIoStage` +
/// the `pub struct UnknownCompilerSpecIoStage(pub String)`
/// parse-rejection carrier alongside the enum declaration. The
/// `#[closed_set(no_from_str)]` axis suppresses the auto-emitted
/// `FromStr` delegation — this enum's parse surface is the
/// compound `"{operation}: {label}"` key (a projection PAIR
/// rather than a single label), so the FromStr body below stays
/// hand-rolled. The `#[closed_set(generate_unknown)]` axis emits
/// the `UnknownCompilerSpecIoStage` carrier with the
/// auto-projected `"unknown compiler spec io stage: {0}"`
/// `#[error(...)]` annotation (matching the pre-lift wording
/// byte-for-byte via `pascal_to_spaced_lowercase`). `via` defaults
/// to `"label"` so the trait's `ClosedSet::label` projection
/// delegates to the inherent `label()` method — generic
/// consumers walking `ALL` and stringifying through the trait see
/// the singular labels (`"serialize"` / `"write"` / `"read"` /
/// `"deserialize"`) while the operator-facing `Display` rendering
/// stays at the compound `"{operation}: {label}"` shape via the
/// hand-rolled block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, tatara_lisp_derive::ClosedSet)]
#[closed_set(no_from_str, generate_unknown)]
pub enum CompilerSpecIoStage {
    /// `serde_json::to_string_pretty` of a `CompilerSpec` errored
    /// inside `realize_to_disk`.
    RealizeToDiskSerialize,
    /// `std::fs::write` of the serialized `CompilerSpec` JSON errored
    /// inside `realize_to_disk`.
    RealizeToDiskWrite,
    /// `std::fs::read_to_string` of the on-disk `CompilerSpec` JSON
    /// errored inside `load_from_disk`.
    LoadFromDiskRead,
    /// `serde_json::from_str` of the on-disk `CompilerSpec` JSON
    /// errored inside `load_from_disk`.
    LoadFromDiskDeserialize,
}

impl CompilerSpecIoStage {
    /// The public entry point's name — the `{form}` slot of the legacy
    /// `Compile`-shaped diagnostic. `realize_to_disk` for the
    /// serialize / write variants; `load_from_disk` for the read /
    /// deserialize variants.
    #[must_use]
    pub fn operation(self) -> &'static str {
        match self {
            Self::RealizeToDiskSerialize | Self::RealizeToDiskWrite => "realize_to_disk",
            Self::LoadFromDiskRead | Self::LoadFromDiskDeserialize => "load_from_disk",
        }
    }

    /// The step within the operation that failed — the `{stage}` slot
    /// of the legacy `"{stage}: {error}"` message shape. One of
    /// `"serialize"`, `"write"`, `"read"`, `"deserialize"`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::RealizeToDiskSerialize => "serialize",
            Self::RealizeToDiskWrite => "write",
            Self::LoadFromDiskRead => "read",
            Self::LoadFromDiskDeserialize => "deserialize",
        }
    }

    /// Closed-set enumeration of every reachable [`CompilerSpecIoStage`]
    /// variant — the four (operation, label) pairs the disk-persistence
    /// surface emits (`realize_to_disk` × {`serialize`, `write`} ⊎
    /// `load_from_disk` × {`read`, `deserialize`}). The `[Self; 4]`
    /// array literal forces the arity so a fifth pair — a hypothetical
    /// `LoadFromStrDeserialize` once an in-memory `load_from_str` lands,
    /// or a `RealizeToDiskAtomicReplace` if the realize path grows a
    /// crash-safe-rename stage — cannot be added at the type without
    /// extending this constant.
    ///
    /// Sibling closed-set lift to every other typed-shape enum the
    /// substrate carries: this crate's own [`ExpectedKwargShape::ALL`]
    /// (the seven reachable expected-kwarg shapes — single-projection
    /// closed set), [`SexpShape::ALL`] (the twelve reachable observed-
    /// Sexp shapes — single-projection closed set), [`MacroDefHead::ALL`]
    /// (the three reachable macro-definition heads), [`UnquoteForm::ALL`]
    /// (the two reachable template-marker forms),
    /// [`crate::ast::AtomKind::ALL`], [`crate::ast::QuoteForm::ALL`], and
    /// across the workspace `ConditionKind::ALL`, `ProcessPhase::ALL`,
    /// `RequestorKind::ALL`, `ReceiptKind::ALL`, … . What's distinct here:
    /// this is the substrate's first *compound-key* closed set — the
    /// reachable identity is the PAIR `(operation, label)`, not either
    /// projection alone (`operation` partitions ALL into 2-of-2 halves,
    /// `label` is bijective with ALL by accident of the current four
    /// variants). [`FromStr`] keys on the compound rendering so the
    /// cross-product reachability constraint — only 4 of the 8
    /// conceivable `(operation, label)` pairs are reachable, e.g.
    /// `(load_from_disk, write)` is structurally absurd — becomes a
    /// load-bearing property of the parse boundary, not a runtime
    /// re-assertion at the helper.
    ///
    /// Future consumers that compose against [`Self::ALL`]: LSP / REPL
    /// completion for the operator-facing rendered (operation, label)
    /// pairs (every `compile error in X: Y: ...` substring in
    /// `LispError::CompilerSpecIo` keys on this set's projection through
    /// [`Self::operation`] and [`Self::label`]); `tatara-check` coverage
    /// assertions over which disk-persistence stages reach a
    /// [`LispError::CompilerSpecIo`] site at all — the typed sweep
    /// replaces a hand-rolled `match`-over-string vocabulary at consumer
    /// boundaries; any future audit-trail metric jointly labeled by
    /// [`Self::operation`] × [`Self::label`] (e.g.
    /// `tatara_lisp_compiler_spec_io_total{operation="realize_to_disk",
    /// stage="serialize"}`) — the metric label set IS [`Self::ALL`]
    /// mapped through the projection pair.
    pub const ALL: [Self; 4] = [
        Self::RealizeToDiskSerialize,
        Self::RealizeToDiskWrite,
        Self::LoadFromDiskRead,
        Self::LoadFromDiskDeserialize,
    ];
}

/// Standalone rendering of a [`CompilerSpecIoStage`] in the canonical
/// compound `"{operation}: {label}"` form — byte-for-byte the same
/// substring that lands inside the
/// [`LispError::CompilerSpecIo`] diagnostic between `"compile error in
/// "` and `": {message}"`. Pinning Display to the compound form means a
/// consumer that extracts the (operation, label) prefix from a rendered
/// diagnostic — for example by `s.strip_prefix("compile error in ")
/// .and_then(|t| t.rsplit_once(": "))` — round-trips the captured
/// substring through [`FromStr`] back into the typed variant exactly.
///
/// The compound form is load-bearing: `label` alone would be bijective
/// with the current four variants (`"serialize"` / `"write"` / `"read"`
/// / `"deserialize"`) but a future fifth variant like
/// `LoadFromStrDeserialize` would collide with `LoadFromDiskDeserialize`
/// on `label` alone. Display-ing the compound key means the bijection
/// survives the closed-set extension without a label-disambiguation
/// dance.
impl std::fmt::Display for CompilerSpecIoStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.operation(), self.label())
    }
}

/// Decode a canonical [`CompilerSpecIoStage`] compound key
/// `"{operation}: {label}"` back into the typed variant — `Ok(stage)`
/// when the input matches the [`Display`] rendering of one of the four
/// variants in [`CompilerSpecIoStage::ALL`] byte-for-byte (case-sensitive
/// because the labels are the rendered diagnostic surface and any case
/// drift would silently bifurcate the round-trip), and
/// [`Err(UnknownCompilerSpecIoStage)`] for every other string.
///
/// Crucially the decode REJECTS the four conceivable-but-unreachable
/// cross-product pairs — `"realize_to_disk: read"`,
/// `"realize_to_disk: deserialize"`, `"load_from_disk: serialize"`,
/// `"load_from_disk: write"` — because none of those pairs appears in
/// [`CompilerSpecIoStage::ALL`]. The cross-product reachability
/// constraint, previously a Code-level invariant (only the four call
/// sites in `compiler_spec.rs` construct stages, each construction
/// site pairs the correct operation with the correct stage), becomes
/// a type-level invariant: the parse boundary refuses to deserialize
/// an unreachable pair.
///
/// Partial keys — `"serialize"` alone, `"realize_to_disk"` alone — also
/// reject. The `": "` separator must appear at least once for the
/// decode to consider any variant; otherwise the diagnostic substring
/// shape isn't a compound key at all.
///
/// Round-trip invariant pinned by
/// `compiler_spec_io_stage_compound_key_round_trips_through_from_str`:
/// for every variant `s` in [`CompilerSpecIoStage::ALL`],
/// `s.to_string().parse() == Ok(s)`. The compound-rendering site is
/// singular (the [`Display`] impl projects through [`Self::operation`]
/// and [`Self::label`]) so the round-trip is the only way the typed
/// surface and the rendered diagnostic literal can drift apart —
/// pinning it here means they cannot. Mirror of every sibling
/// closed-set round-trip in the workspace ([`SexpShape::from_str`],
/// [`ExpectedKwargShape::from_str`], [`MacroDefHead::from_str`],
/// [`UnquoteForm::from_str`], `RequestorKind::from_str`,
/// `ReceiptKind::from_str`, `ConditionKind::from_str`,
/// `ProcessPhase::from_str`, …) — the difference is the compound-key
/// shape rather than a single label, which sharpens the closed-set
/// constraint from "cardinality matches the variant count" to
/// "cardinality matches the variant count AND the projection product
/// is partial, not total."
impl std::str::FromStr for CompilerSpecIoStage {
    type Err = UnknownCompilerSpecIoStage;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let Some((op, lbl)) = s.split_once(": ") else {
            return Err(UnknownCompilerSpecIoStage(s.to_owned()));
        };
        for stage in Self::ALL {
            if op == stage.operation() && lbl == stage.label() {
                return Ok(stage);
            }
        }
        Err(UnknownCompilerSpecIoStage(s.to_owned()))
    }
}

// `pub struct UnknownCompilerSpecIoStage(pub String)` is generated by
// `#[derive(tatara_lisp_derive::ClosedSet)]` on the `CompilerSpecIoStage`
// declaration above through `#[closed_set(generate_unknown)]`. The
// auto-projected `#[error("unknown compiler spec io stage: {0}")]`
// annotation (via `pascal_to_spaced_lowercase("CompilerSpecIoStage")` →
// `"compiler spec io stage"` — pinned by
// `pascal_to_spaced_lowercase_tests::contiguous_uppercase_runs_collapse_to_lowercase_without_inner_spaces`)
// matches the pre-lift hand-rolled wording byte-for-byte; LSP / REPL
// substring-matching `"unknown compiler spec io stage: "` continues to
// filter this rejection class without binding to the specific input.
// The substrate-wide carrier shape (`Debug + Clone + PartialEq + Eq +
// thiserror::Error` derives, `pub struct UnknownX(pub String)` with the
// `#[error("unknown <thing>: {0}")]` annotation) — symmetric to every
// sibling `Unknown*` error in the workspace
// (`UnknownExpectedKwargShape`, `UnknownSexpShape`,
// `UnknownMacroDefHead`, `UnknownUnquoteForm`,
// `crate::ast::UnknownAtomKind`, `crate::ast::UnknownQuoteForm`,
// `tatara_process::allocation::UnknownRequestorKind`,
// `tatara_process::receipt::UnknownReceiptKind`,
// `tatara_process::phase::UnknownPhase`,
// `tatara_process::boundary::UnknownConditionKind`,
// `tatara_process::lifetime::UnknownTeardownPolicy`, …) — emits from
// the derive rather than from a per-declaration hand-roll.

/// Closed-set identifier for a bytecode-runtime invariant violation
/// surfaced by `macro_expand.rs::apply_compiled`. Encodes the four
/// reachable failure modes — Subst with an out-of-bounds param index,
/// Splice with an out-of-bounds param index, EndList against an empty
/// stack, and a final pop yielding no value — as a typed enum, so the
/// invalid combination of "stack-gate kind with an op-index payload"
/// (e.g. `EndListEmptyStack` carrying a `usize`) is structurally
/// unrepresentable: the index payload lives INSIDE the variants that
/// actually carry one (`SubstBadIndex(usize)` / `SpliceBadIndex(usize)`).
///
/// Same posture as `CompilerSpecIoStage`: the closed set becomes a
/// TYPE, not a free-form `message: String` slot inside the helper. The
/// `message()` projection feeds the `LispError::TemplateInvariant`
/// Display rendering directly via the `#[error(...)]` annotation;
/// adding a new bytecode-runtime invariant (e.g. a future `WrongOpcode`
/// gate that names a malformed bytecode header at the type level)
/// requires extending this enum, which rustc-enforces matching at the
/// projection site.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the closed set
/// of bytecode-invariant failure modes becomes a TYPE rather than a
/// runtime string-format dance. THEORY.md §VI.1 — generation over
/// composition; the typed enum lands the structural-completeness floor
/// for the bytecode-runtime surface, parallel to how `CompilerSpecIoStage`
/// lands the structural-completeness floor for the disk-persistence
/// surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateInvariantKind {
    /// `TemplateOp::Subst(idx)` referenced a param index that
    /// `args_by_index.get(idx)` returned `None` for — the compiled
    /// bytecode referenced an out-of-bounds required-param slot.
    SubstBadIndex(usize),
    /// `TemplateOp::Splice(idx)` referenced a param index that
    /// `args_by_index.get(idx)` returned `None` for — the compiled
    /// bytecode referenced an out-of-bounds splice-target param slot.
    SpliceBadIndex(usize),
    /// `TemplateOp::EndList` ran against an empty runtime stack —
    /// `stack.pop()` returned `None`, meaning the compiled bytecode
    /// emitted an `EndList` without a matching `BeginList`. The stack
    /// is the proof artifact; an unbalanced stack is the bytecode
    /// compiler's proof obligation having been silently dropped.
    EndListEmptyStack,
    /// The final `stack.pop()` after the bytecode loop yielded `None`
    /// — the compiled bytecode produced no value at all (an empty op
    /// list, or a body that consumes its own output). Distinct from
    /// `EndListEmptyStack`: that fires mid-loop on an explicit
    /// `EndList`; this fires after the loop on the implicit final
    /// pop.
    FinalNoValue,
}

impl TemplateInvariantKind {
    /// The `{message}` slot of the legacy `LispError::Compile { form:
    /// macro_name, message: <invariant> }` shape. Each variant projects
    /// to the canonical message string the pre-lift inline triples
    /// emitted — byte-for-byte equivalent so authoring-tool substring
    /// greps (`tatara-check`, REPL) see no drift across the lift.
    #[must_use]
    pub fn message(self) -> String {
        match self {
            Self::SubstBadIndex(idx) => {
                format!("compiled template referenced bad param index {idx}")
            }
            Self::SpliceBadIndex(idx) => {
                format!("compiled template referenced bad splice index {idx}")
            }
            Self::EndListEmptyStack => "compiled template: EndList with empty stack".into(),
            Self::FinalNoValue => "compiled template produced no value".into(),
        }
    }
}

/// Closed-set identifier for the head keyword of a `defmacro`-shape
/// rejection — the three canonical macro-definition heads
/// `defmacro` / `defpoint-template` / `defcheck`. Carried as a typed
/// slot on `LispError::DefmacroArity`, `LispError::DefmacroNonSymbolName`,
/// and `LispError::DefmacroNonListParams` so authoring tools (REPL, LSP,
/// `tatara-check`) bind to variant identity rather than substring-matching
/// the rendered `head` string.
///
/// Mirror at the macro-definition-head boundary of the prior-run
/// `CompilerSpecIoStage` (disk-persistence surface) and
/// `TemplateInvariantKind` (bytecode-runtime surface) closed-set lifts:
/// those variants key on a typed enum for the (operation, stage) pair
/// and the invariant kind respectively; this enum keys the three
/// `Defmacro*` variants on a typed head identity. Adding a new
/// macro-definition head requires extending this enum, which rustc-
/// enforces matching at every projection site (`keyword()`) — the
/// closed set becomes a TYPE rather than a `matches!` literal at the
/// `macro_def_from` gate plus three `match head` projections inside
/// each variant's helper.
///
/// `from_keyword(&str) -> Option<Self>` projects an arbitrary source
/// symbol into the typed enum; `keyword(self) -> &'static str` projects
/// back to the canonical literal for `LispError::Display` rendering.
/// The bidirection is the identity on the closed set —
/// `from_keyword(k).unwrap().keyword() == k` for every canonical `k`.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the closed set of
/// macro-definition heads becomes a TYPE rather than a runtime
/// string-comparison-and-format dance. THEORY.md §VI.1 — generation
/// over composition; the typed enum lands the structural-completeness
/// floor for the macro-definition-head surface, parallel to how
/// `CompilerSpecIoStage` lands it for the disk-persistence surface and
/// `TemplateInvariantKind` for the bytecode-runtime surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, tatara_lisp_derive::ClosedSet)]
#[closed_set(via = "keyword", display, generate_unknown = "macro definition head")]
pub enum MacroDefHead {
    /// `(defmacro NAME (PARAMS) BODY)` — the canonical Lisp-style macro
    /// definition.
    Defmacro,
    /// `(defpoint-template NAME (PARAMS) BODY)` — the K8s-as-processes
    /// authoring surface's macro form (see `tatara-process`).
    DefpointTemplate,
    /// `(defcheck NAME (PARAMS) BODY)` — the workspace-coherence
    /// authoring surface's macro form (see
    /// `tatara-reconciler/checks.lisp`).
    Defcheck,
}

impl MacroDefHead {
    /// The closed set of three macro-definition heads — single source
    /// of truth that drives the [`Self::keyword`] / [`fmt::Display`]
    /// projection AND the [`Self::from_keyword`] / [`FromStr`] decode
    /// sweeps keyed on [`Self::keyword`]. Adding a hypothetical fourth
    /// head (e.g. a `defpoint-fragment` partial-template surface, a
    /// `defrewrite` typed-rewriter authoring keyword) lands at one
    /// [`Self::ALL`] entry + one [`Self::keyword`] arm — exhaustively
    /// checked by the compiler (the `[Self; 3]` array literal forces
    /// the arity) AND by the per-variant truth-table tests below.
    ///
    /// Sibling closed-set lift to every other typed-shape enum in the
    /// crate ([`crate::ast::AtomKind::ALL`],
    /// [`crate::ast::QuoteForm::ALL`], [`SexpShape::ALL`],
    /// [`UnquoteForm::ALL`]) and across the workspace
    /// (`ConditionKind::ALL`, `ProcessPhase::ALL`,
    /// `ProcessSignal::ALL`, `ChannelKind::ALL`, `IntentKind::ALL`,
    /// `LifetimeKind::ALL`, `RequestorKind::ALL`, `ReceiptKind::ALL`,
    /// …) every one of which paired its typed projection with `ALL`
    /// before this lift.
    ///
    /// Future consumers that compose against `ALL`: LSP / REPL
    /// completion for the macro-definition head at point (every
    /// `(defma…` partial input expands through `Self::ALL.iter().
    /// map(MacroDefHead::keyword)`), `tatara-check` coverage assertions
    /// over which macro-definition heads reach a `DefmacroArity` /
    /// `DefmacroNonSymbolName` / `DefmacroNonListParams` arm at all
    /// (the typed sweep replaces the per-call-site vocabulary of three
    /// `&'static str` literals), any future audit-trail metric jointly
    /// labeled by [`Self::keyword`] (e.g.
    /// `tatara_lisp_defmacro_arity_total{head="defmacro"}` — the
    /// metric label set IS [`Self::ALL`] mapped through
    /// [`Self::keyword`]).
    pub const ALL: [Self; 3] = [Self::Defmacro, Self::DefpointTemplate, Self::Defcheck];

    /// Project a `head: &str` borrow (a `Sexp` symbol slice) into the
    /// typed `MacroDefHead`. Returns `None` if `head` is not one of the
    /// three canonical macro-definition head keywords; the caller
    /// (`macro_def_from`) then returns `Ok(None)` to mean "this form is
    /// not a defmacro form."
    ///
    /// Implemented as a linear sweep over [`Self::ALL`] keyed on
    /// [`Self::keyword`] so the three canonical keyword literals
    /// (`"defmacro"` / `"defpoint-template"` / `"defcheck"`) live at
    /// ONE site (the `keyword` arms) rather than at TWO sites
    /// (`keyword` + a per-variant `from_keyword` match arm). Adding a
    /// fourth variant extends only [`Self::ALL`] + [`Self::keyword`],
    /// NOT a third per-variant literal site. The `Option<Self>` face
    /// is the open-by-design projection [`crate::ast::Sexp::as_call_to_any`]
    /// composes against; [`FromStr`] is the typed-error face callers
    /// reaching for a parse-rejection diagnostic compose against.
    /// Cross-face contract pinned by
    /// `macro_def_head_from_keyword_matches_from_str_for_every_input`.
    #[must_use]
    pub fn from_keyword(head: &str) -> Option<Self> {
        head.parse().ok()
    }

    /// Project the typed `MacroDefHead` back to the canonical
    /// `&'static str` literal — feeds the `LispError::Defmacro*` Display
    /// rendering via the `#[error(...)]` annotation. The `&'static str`
    /// lifetime is load-bearing: it's what lets the variants project
    /// through this method into their `compile error in {head}:` prefix
    /// without an allocation, parallel to how
    /// `CompilerSpecIoStage::operation()` / `label()` feed
    /// `LispError::CompilerSpecIo`'s Display.
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Defmacro => "defmacro",
            Self::DefpointTemplate => "defpoint-template",
            Self::Defcheck => "defcheck",
        }
    }
}

// `impl std::fmt::Display for MacroDefHead` + `impl std::str::FromStr
// for MacroDefHead` + `impl crate::ClosedSet for MacroDefHead` +
// `pub struct UnknownMacroDefHead(pub String)` are generated by
// `#[derive(tatara_lisp_derive::ClosedSet)]` on the enum declaration
// above. `label` delegates to the inherent `MacroDefHead::keyword` via
// `#[closed_set(via = "keyword")]` so the domain-canonical
// reserved-word projection (`"defmacro"` / `"defpoint-template"` /
// `"defcheck"`) stays load-bearing at the inherent surface while the
// trait surface unifies every closed-set implementor's projection name
// onto `label`. The `display` flag emits the substrate-wide
// `f.write_str(Self::keyword(*self))` block.
// `#[closed_set(generate_unknown = "macro definition head")]` emits the
// typed parse-rejection carrier with the substrate-wide `Debug + Clone
// + PartialEq + Eq + thiserror::Error` derives and the `#[error("unknown
// macro definition head: {0}")]` annotation byte-for-byte; the explicit
// label overrides the auto-derived
// `pascal_to_spaced_lowercase("MacroDefHead")` (`"macro def head"`)
// which abbreviates `Def` rather than expanding it to `definition`,
// pinning the pre-lift operator-facing wording. The FromStr decode is
// a linear sweep over `MacroDefHead::ALL` keyed on `keyword`; round-trip
// + cross-axis rejection (`"defpoint"` / `"symbol"`) pinned by
// `macro_def_head_keyword_round_trips_through_from_str` +
// `macro_def_head_from_str_rejects_cross_axis_vocabularies`.

/// Closed-set identifier for the way a `Sexp::List` entry in a macro's
/// `&optional` section failed to match the canonical `(NAME DEFAULT)`
/// shape. Carried as a typed slot on `LispError::OptionalParamMalformed`
/// so authoring tools (REPL, LSP, `tatara-check`) bind to variant identity
/// rather than substring-matching the rendered suffix.
///
/// Mirror at the `parse_params` optional-section boundary of the prior-run
/// `MacroDefHead` (macro-definition-head closed set), `UnquoteForm`
/// (template-marker closed set), `CompilerSpecIoStage` (disk-persistence
/// surface), and `TemplateInvariantKind` (bytecode-runtime surface)
/// closed-set lifts: those enums key their respective rejection variants
/// on a typed identity; this enum keys the four reachable list-spec
/// rejection modes the optional-section gate can emit on a typed identity.
/// Adding a new mode (e.g., `SuppliedPNotYetSupported` once an evaluator
/// lands and the three-element `(name default supplied-p)` shape is
/// admitted) requires extending this enum, which rustc-enforces matching
/// at every projection site (`label()`).
///
/// `label(self) -> String` projects the typed reason to a short
/// human-readable clause (`"empty list"` / `"missing default"` / `"3
/// elements (need 2)"` / `"name not a symbol"`) that the
/// `LispError::OptionalParamMalformed` Display rendering threads through
/// `optional_param_malformed_suffix` into the parenthetical suffix —
/// parallel to how `TemplateInvariantKind::message()` feeds
/// `LispError::TemplateInvariant`'s Display.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the closed set of
/// optional-spec malformed shapes becomes a TYPE rather than a runtime
/// string-comparison-and-format dance. A typo like `reason: "empty list
/// "` (trailing space) is structurally unrepresentable; the four shapes
/// are an exhaustive `match`. THEORY.md §VI.1 — generation over
/// composition; the typed enum lands the structural-completeness floor
/// for the optional-section-malformed surface, parallel to how
/// `TemplateInvariantKind` lands it for the bytecode-runtime surface and
/// `MacroDefHead` for the macro-definition-head surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionalParamMalformedReason {
    /// `&optional ()` — the spec is a list of length zero, with no name
    /// and no default form.
    EmptyList,
    /// `&optional (x)` — the spec is a list of length one, naming an
    /// optional but supplying no default form. This is REJECTED rather
    /// than reinterpreted as `&optional x` because a bare-symbol spec
    /// IS the canonical "no default" shape; a parenthesized
    /// single-element spec is ambiguous and would silently DROP the
    /// extra parens at the surface.
    MissingDefault,
    /// `&optional (x default extra …)` — the spec is a list of length
    /// three or more. CL's `(name default supplied-p)` shape is NOT
    /// supported in v0 (no evaluator → no `supplied-p` variable
    /// binding), so any third element is structurally surplus. `length`
    /// is the actual element count (≥3).
    ExtraElements { length: usize },
    /// `&optional (5 default)` — the spec is a list of length two but
    /// the first element isn't a symbol. The name slot must be a symbol
    /// (the same gate the bare-symbol path enforces); a non-symbol head
    /// is rejected here so the `OptionalParam.name: String` slot cannot
    /// be populated from a `5` / `:foo` / `(nested)` value.
    NonSymbolName,
}

impl OptionalParamMalformedReason {
    /// Short human-readable clause for the parenthetical suffix of
    /// `LispError::OptionalParamMalformed`'s Display. The variants
    /// project to:
    ///
    ///   * `EmptyList`          → `"empty list"`
    ///   * `MissingDefault`     → `"missing default"`
    ///   * `ExtraElements{N}`   → `"N elements (need 2)"`
    ///   * `NonSymbolName`      → `"name not a symbol"`
    ///
    /// `label` returns `String` (rather than `&'static str`) because the
    /// `ExtraElements` arm formats its `length` payload — the other three
    /// arms produce literal `&'static str` values which `.to_string()`
    /// projects through. Mirror of `TemplateInvariantKind::message()`:
    /// both return `String`, both project the closed-set typed reason
    /// into the `LispError::Display` rendering via the variant's
    /// `#[error(...)]` annotation.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::EmptyList => "empty list".to_string(),
            Self::MissingDefault => "missing default".to_string(),
            Self::ExtraElements { length } => format!("{length} elements (need 2)"),
            Self::NonSymbolName => "name not a symbol".to_string(),
        }
    }
}

/// Closed-set identifier for the syntactic marker of a macro-template
/// unquote (`,`) or unquote-splice (`,@`). Carried as a typed slot on
/// `LispError::UnboundTemplateVar` and `LispError::NonSymbolUnquoteTarget`
/// so authoring tools (REPL, LSP, `tatara-check`) bind to variant identity
/// via `UnquoteForm::Splice` rather than substring-matching the rendered
/// `prefix` literal.
///
/// Mirror at the template-marker boundary of the prior-run `MacroDefHead`
/// (macro-definition-head closed set), `CompilerSpecIoStage`
/// (disk-persistence surface), and `TemplateInvariantKind` (bytecode-runtime
/// surface) closed-set lifts: those enums key their respective rejection
/// variants on a typed identity; this enum keys the two unquote-template
/// rejection variants (`UnboundTemplateVar`, `NonSymbolUnquoteTarget`) on a
/// typed marker identity. Adding a new unquote variant (e.g., a hypothetical
/// `,~` form) requires extending this enum, which rustc-enforces matching at
/// every projection site (`marker()`) — the closed set becomes a TYPE rather
/// than two `&'static str`-keyed slots that could drift independently.
///
/// `marker(self) -> &'static str` projects the typed `UnquoteForm` back to
/// the canonical literal for `LispError::Display` rendering. The `&'static
/// str` lifetime is load-bearing: it lets both variants project through this
/// method into their `compile error in {prefix}…:` prefix without an
/// allocation, parallel to how `MacroDefHead::keyword()` and
/// `CompilerSpecIoStage::operation()` / `label()` feed their respective
/// `LispError::*` Display impls.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the closed set of
/// unquote markers becomes a TYPE rather than a runtime
/// string-comparison-and-format dance. A typo like `prefix: ",,"` is
/// structurally unrepresentable rather than re-asserted at the helper
/// boundary. THEORY.md §VI.1 — generation over composition; the typed enum
/// lands the structural-completeness floor for the template-marker surface,
/// parallel to how `MacroDefHead` lands it for the macro-definition-head
/// surface and `CompilerSpecIoStage` for the disk-persistence surface.
/// THEORY.md §II.1 invariant 1 (typed entry): a non-symbol unquote target /
/// an unbound template var is exactly the failure mode the typed-entry gate
/// exists to reject, and the marker identity is part of the proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, tatara_lisp_derive::ClosedSet)]
#[closed_set(via = "marker", display, generate_unknown = "unquote form")]
pub enum UnquoteForm {
    /// `,x` — single-value substitution. The `,` marker; the inner symbol
    /// is substituted with its bound value at template expansion.
    Unquote,
    /// `,@x` — list-splice substitution. The `,@` marker; the inner symbol
    /// must be bound to a list, whose elements are flattened into the
    /// containing list at template expansion.
    Splice,
}

impl UnquoteForm {
    /// The closed set of two template-marker syntactic forms — single
    /// source of truth that drives the [`Self::marker`] / [`fmt::Display`]
    /// projection AND the [`FromStr`] decode sweep keyed on
    /// [`Self::marker`]. Adding a hypothetical third variant (e.g. a
    /// `,~` reverse-unquote, a `,?` conditional-unquote) lands at one
    /// [`Self::ALL`] entry + one [`Self::marker`] arm — exhaustively
    /// checked by the compiler (the `[Self; 2]` array literal forces
    /// the arity) AND by the per-variant truth-table tests below.
    ///
    /// Sibling closed-set lift to every other typed-shape enum the
    /// substrate carries: this crate's own [`SexpShape::ALL`] (the
    /// twelve reachable outer shapes — superset on the structural axis
    /// of the `Sexp` algebra), [`crate::ast::AtomKind::ALL`] (the six
    /// atomic-payload kinds — peer axis on the same algebra),
    /// [`crate::ast::QuoteForm`] (the four homoiconic prefix-wrappers
    /// — superset of THIS enum's two template markers via the 2-of-4
    /// projection [`crate::ast::QuoteForm::as_unquote_form`]), and the
    /// cross-crate `tatara-process` family (`ConditionKind::ALL`,
    /// `ProcessPhase::ALL`, `ProcessSignal::ALL`, `ChannelKind::ALL`,
    /// `IntentKind::ALL`, `LifetimeKind::ALL`, `RequestorKind::ALL`,
    /// `ReceiptKind::ALL`, …) every one of which paired its typed
    /// projection with `ALL` before this lift.
    ///
    /// Future consumers that compose against `ALL`: LSP / REPL
    /// completion for the operator-facing rendered template-marker
    /// (every `compile error in {prefix}…:` substring in `LispError`'s
    /// rendered diagnostics for a template-substitution rejection keys
    /// on this set's projection through [`Self::marker`]);
    /// `tatara-check` coverage assertions over which template markers
    /// reach a `NonSymbolUnquoteTarget` / `UnboundTemplateVar` arm at
    /// all — the typed sweep replaces a per-callsite vocabulary of two
    /// `&'static str` literals; any future audit-trail metric jointly
    /// labeled by [`Self::marker`] (e.g.
    /// `tatara_lisp_unbound_template_var_total{prefix=","}`) — the
    /// metric label set IS [`Self::ALL`] mapped through
    /// [`Self::marker`]; any future structural rewriter (typed
    /// analogue of MLIR's `op.walk<UnquoteFormOp>()`) that wants to
    /// sweep over every template marker in a typed sequence.
    pub const ALL: [Self; 2] = [Self::Unquote, Self::Splice];

    /// Project the typed `UnquoteForm` to the canonical `&'static str`
    /// literal — feeds the `LispError::UnboundTemplateVar` /
    /// `LispError::NonSymbolUnquoteTarget` Display rendering via the
    /// `#[error(...)]` annotation. The `&'static str` lifetime is
    /// load-bearing: it lets the variants project through this method into
    /// their `compile error in {prefix}…:` prefix without an allocation,
    /// parallel to how `MacroDefHead::keyword()` and
    /// `CompilerSpecIoStage::operation()` / `label()` feed their respective
    /// `LispError::*` Display impls.
    ///
    /// Composition law: `self.marker() == self.to_quote_form().prefix()`
    /// for every `self: UnquoteForm`. Pre-lift the body inlined the two
    /// literals (`","` / `",@"`) as a parallel match-table; the same two
    /// literals also lived at the matching Unquote/UnquoteSplice arms of
    /// [`crate::ast::QuoteForm::prefix`]. Post-lift the body composes
    /// `Self::to_quote_form()` (the typed 2-of-4 subset → superset
    /// projection) with `crate::ast::QuoteForm::prefix()` (the canonical
    /// 4-of-4 prefix-string projection), so the two `&'static str`
    /// literals live at ONE canonical site (`QuoteForm::prefix`'s
    /// Unquote/UnquoteSplice arms in `ast.rs`) and every consumer of
    /// `UnquoteForm::marker` (the `ClosedSet`-trait Display/FromStr
    /// surface via `#[closed_set(via = "marker")]`, the unbound-template
    /// `did you mean ,@args?` hint suffix in this module's
    /// `unbound_hint_suffix` helper, the `#[error("... {prefix}")]`
    /// annotation on `LispError::UnboundTemplateVar` /
    /// `LispError::NonSymbolUnquoteTarget`, the future LSP / REPL /
    /// `tatara-check` completion-bar projections that key on the typed
    /// marker) inherits the canonical vocabulary through the composition
    /// rather than through a parallel inline table that drifts at the
    /// next vocabulary rename.
    ///
    /// Sibling-shape lift to the prior-run `AtomKind::label` ⊂
    /// `SexpShape::label` composition (commit 1db697f): both pin the
    /// invariant that a typed-subset enum's projection on a closed-set
    /// vocabulary is structurally derived from its parent superset's
    /// projection, not a parallel literal table the type system happens
    /// to not catch when the vocabularies drift.
    ///
    /// The bidirectional contract is anchored by tests:
    /// `unquote_form_marker_projects_canonical_literal_for_each_variant`
    /// pins each variant's canonical literal so a typo in any arm
    /// fails-loudly,
    /// `unquote_form_display_renders_canonical_marker_for_each_variant`
    /// pins Display-equals-marker so any `#[error("... {prefix}")]`
    /// annotation that threads through this projection projects
    /// byte-for-byte,
    /// `unquote_form_marker_round_trips_through_from_str` pins the
    /// `marker` ↔ [`Self::FromStr`] round-trip for every variant in
    /// [`Self::ALL`] so the typed surface and the rendered diagnostic
    /// literal cannot drift, and the post-lift composition-routing
    /// pin
    /// `unquote_form_marker_routes_through_to_quote_form_prefix_via_composition`
    /// asserts pointer-equality between `self.marker()` and
    /// `self.to_quote_form().prefix()` for every variant — so a
    /// regression that re-inlines the literals as a parallel match-table
    /// fails the pointer pin even when the rendered bytes still agree
    /// (the inline-literal-table copy lives at a different `&'static str`
    /// address).
    #[must_use]
    pub fn marker(self) -> &'static str {
        self.to_quote_form().prefix()
    }

    /// Project the 2-of-4 template-substitution subset back to its
    /// parent [`crate::ast::QuoteForm`] superset — the structural
    /// inverse of [`crate::ast::QuoteForm::as_unquote_form`]. Returns
    /// the [`crate::ast::QuoteForm`] variant whose canonical prefix
    /// string matches this `UnquoteForm`'s marker:
    /// [`Self::Unquote`] → [`crate::ast::QuoteForm::Unquote`],
    /// [`Self::Splice`] → [`crate::ast::QuoteForm::UnquoteSplice`].
    ///
    /// Total (not `Option`) because every `UnquoteForm` IS a
    /// `QuoteForm` — the 2-of-4 subset is a subset by inclusion. The
    /// dual projection [`crate::ast::QuoteForm::as_unquote_form`]
    /// returns `Option<UnquoteForm>` because the two non-substitution
    /// variants ([`crate::ast::QuoteForm::Quote`] and
    /// [`crate::ast::QuoteForm::Quasiquote`]) lie outside the
    /// substitution subset.
    ///
    /// Round-trip identity: `self.to_quote_form().as_unquote_form() ==
    /// Some(self)` for every `self: UnquoteForm`, pinned by
    /// `unquote_form_to_quote_form_round_trips_through_as_unquote_form`.
    /// The dual identity `qf.as_unquote_form().map(|uf|
    /// uf.to_quote_form()) == qf.as_unquote_form().map(|_| qf)` holds
    /// by construction (the projection is a section of
    /// [`crate::ast::QuoteForm::as_unquote_form`] restricted to its
    /// image — the Unquote/UnquoteSplice variants).
    ///
    /// Lifts the (UnquoteForm variant, QuoteForm variant) pairing onto
    /// the typed algebra so consumers that need the parent superset
    /// from a substitution-subset value (the `marker` lift's
    /// composition root, the future hint-suffix pretty-printer's
    /// `prefix.to_quote_form().prefix()` routing, the future
    /// canonical-form interop tag that wants to route an `UnquoteForm`
    /// through `iac_forge_tag` via `to_quote_form().iac_forge_tag()`
    /// without re-deriving the pairing) bind at ONE typed projection
    /// rather than at parallel match arms each site re-derives.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the
    /// subset-to-superset projection becomes a TYPE projection on the
    /// substrate algebra. THEORY.md §VI.1 — generation over
    /// composition; the (UnquoteForm variant, QuoteForm variant)
    /// pairing emerges from this ONE primitive rather than from
    /// per-callsite per-variant literals. THEORY.md §II.1 invariant 1 —
    /// typed entry; the typed subset-to-superset projection IS a
    /// substrate-owned theorem rather than a hand-inlined match-table
    /// duplication discipline N sites had to keep in lockstep.
    #[must_use]
    pub fn to_quote_form(self) -> crate::ast::QuoteForm {
        match self {
            Self::Unquote => crate::ast::QuoteForm::Unquote,
            Self::Splice => crate::ast::QuoteForm::UnquoteSplice,
        }
    }

    /// Project the 2-of-4 template-substitution subset marker back into
    /// its matching [`crate::ast::Sexp`] wrapper variant applied to
    /// `inner` — the typed-CONSTRUCT face on the closed-set `UnquoteForm`
    /// algebra, sibling section-for-retraction of the existing typed-
    /// PROJECT face [`crate::ast::Sexp::as_unquote`] on the outer
    /// [`crate::ast::Sexp`] algebra. [`Self::Unquote`] yields
    /// [`crate::ast::Sexp::Unquote`], [`Self::Splice`] yields
    /// [`crate::ast::Sexp::UnquoteSplice`], each boxing `inner` into the
    /// corresponding tuple-variant constructor
    /// (`fn(Box<Sexp>) -> Sexp`).
    ///
    /// Closes the (construct, project) algebra dual on the closed-set
    /// `UnquoteForm` algebra — the substitution-subset peer of the
    /// closed-set superset algebra's already-closed dual pair
    /// ([`crate::ast::QuoteForm::wrap`] +
    /// [`crate::ast::Sexp::as_quote_form`]). Where the superset's
    /// `wrap` reaches all four homoiconic prefix-wrappers and its
    /// projection returns `Option<(QuoteForm, &Sexp)>` on the outer
    /// [`crate::ast::Sexp`] algebra, this subset's `wrap` reaches only
    /// the two template-substitution wrappers
    /// ([`crate::ast::Sexp::Unquote`] and
    /// [`crate::ast::Sexp::UnquoteSplice`]) and its projection sibling
    /// [`crate::ast::Sexp::as_unquote`] returns
    /// `Option<(UnquoteForm, &Sexp)>` — the (construct, project) pair on
    /// the subset algebra is symmetric with the (construct, project)
    /// pair on the superset algebra, each closed at one method per
    /// direction on the respective closed set.
    ///
    /// Composition law (forward): `self.wrap(inner) ==
    /// self.to_quote_form().wrap(inner)` for every `self: UnquoteForm`
    /// and every `inner: Sexp` — routes through the superset's
    /// [`crate::ast::QuoteForm::wrap`] at the single tuple-variant
    /// emission site so the (marker, `Sexp::*` tuple-variant
    /// constructor) pairing binds at ONE closed-set match on the
    /// superset algebra rather than at a parallel two-arm match table
    /// on this subset. Round-trip law (section-for-retraction with the
    /// soft-projection sibling): `self.wrap(inner).as_unquote() ==
    /// Some((self, &inner))` for every `self: UnquoteForm` and every
    /// `inner: Sexp` — the subset's typed constructor pairs
    /// section-for-retraction with the outer algebra's soft projection,
    /// and the marker + inner body cross-projection preserves identity
    /// on the substitution-subset closed set. Sibling-shape lift to the
    /// prior-run `UnquoteForm::marker` ⊂ `QuoteForm::prefix` composition
    /// (commit 250c001): both pin the invariant that a typed-subset
    /// enum's projection on a closed-set operation is structurally
    /// derived from its parent superset's projection through the
    /// canonical `to_quote_form` composition, not a parallel inline
    /// match-table the type system happens to not catch when the
    /// vocabularies drift.
    ///
    /// Pre-lift consumers that had an `UnquoteForm` marker in hand and
    /// wanted to build a fresh [`crate::ast::Sexp`] wrapper had to
    /// spell the two-step composition
    /// `uf.to_quote_form().wrap(inner)` — a future template rewriter
    /// consuming [`crate::ast::Sexp::as_unquote`]'s
    /// `Option<(UnquoteForm, &Sexp)>` projection and threading a
    /// rewritten inner body back into the matching wrapper on the
    /// substitution-subset closed set (a future
    /// `TypedRewriter<UnquoteFormOp>` sweep, a future macro-template
    /// canonicalizer that normalizes `,x` / `,@x` shapes, a future REPL
    /// pretty-printer that emits a fresh substitution wrapper from a
    /// borrowed marker + inner pair) would have re-derived the
    /// `.to_quote_form().wrap(_)` composition at every callsite. Post-
    /// lift the composition binds at ONE typed-algebra method on the
    /// closed-set `UnquoteForm` algebra, matching the section-for-
    /// retraction posture the superset's [`crate::ast::QuoteForm::wrap`]
    /// already carries on the outer [`crate::ast::Sexp`] algebra. The
    /// (marker, `Sexp::*` tuple-variant constructor) pairing continues
    /// to live at ONE site on the superset's `wrap` closed-set match
    /// (the single tuple-variant emission point the substrate owns),
    /// which is what this subset method routes through — no parallel
    /// match table, no per-arm drift risk on the two-variant closed set.
    ///
    /// The [`crate::ast::Sexp`] (owned) return type complements
    /// [`crate::ast::Sexp::as_unquote`]'s `&Sexp` (borrowed) —
    /// symmetric with the ([`crate::ast::QuoteForm::wrap`],
    /// [`crate::ast::Sexp::as_quote_form`]) asymmetry on the superset
    /// algebra: `wrap` consumes the inner body to build the new
    /// wrapper, the projection borrows the inner body from the existing
    /// wrapper. The typed `Box::new(inner)` allocation lives at ONE
    /// site on the superset's [`crate::ast::QuoteForm::wrap`] (the
    /// closed-set tuple-variant emission point), so a future
    /// allocation-policy change (e.g. arena-allocated wrappers for
    /// span-aware [`crate::ast::Sexp`]) lands as ONE edit at the
    /// superset's `wrap` and propagates through this subset method
    /// byte-for-byte.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
    /// (UnquoteForm variant, `Sexp::*` tuple-variant constructor)
    /// pairing binds at ONE typed-algebra method on the subset algebra,
    /// routed through the ONE closed-set match on the superset algebra
    /// the substrate already owns. THEORY.md §II.1 invariant 2 — free
    /// middle; every consumer that has an `UnquoteForm` marker and
    /// wants to build a wrapper `Sexp` routes through the SAME typed
    /// method, so a regression that drifts one consumer's pairing from
    /// the others cannot reach the substrate's runtime. THEORY.md §V.1
    /// — knowable platform; the typed-construct face becomes a TYPE
    /// projection on the subset algebra sitting next to the typed-
    /// project face [`crate::ast::Sexp::as_unquote`] on the outer
    /// [`crate::ast::Sexp`] algebra rather than a bare
    /// `to_quote_form().wrap(_)` two-step composition consumers had to
    /// re-derive per site. THEORY.md §VI.1 — generation over
    /// composition; the (subset marker, wrapper `Sexp`) pairing emerges
    /// from ONE typed-algebra composition on the subset algebra rather
    /// than from parallel per-consumer per-variant literals.
    ///
    /// Frontier inspiration: Racket's `syntax/parse` `~or* (~unquote
    /// stx) (~unquote-splice stx)` pattern paired one-for-one with a
    /// typed constructor face on the same subset shape — the (project,
    /// construct) algebra dual is closed at one method per direction
    /// on Racket's substitution-subset surface, and `UnquoteForm::wrap`
    /// / [`crate::ast::Sexp::as_unquote`] is the Rust-typed peer on
    /// the closed-set `UnquoteForm` algebra with
    /// [`crate::ast::QuoteForm::wrap`] standing in for Racket's typed
    /// dispatch through the superset. MLIR's typed factory
    /// `mlir::OpBuilder::create<UnquoteFamilyOp>(loc, inner)` paired
    /// with the projection sibling `mlir::dyn_cast<UnquoteFamilyOp>(op)`
    /// — the typed factory + typed downcast pair the IR algebra closes
    /// over on every wrapper op subset; `UnquoteForm::wrap` /
    /// [`crate::ast::Sexp::as_unquote`] is the unstructured-Rust peer
    /// on the outer [`crate::ast::Sexp`] algebra with the closed-set
    /// `UnquoteForm` standing in for MLIR's subset-of-`OperationName`
    /// taxonomy over the substitution-subset op family.
    #[must_use]
    pub fn wrap(self, inner: crate::ast::Sexp) -> crate::ast::Sexp {
        self.to_quote_form().wrap(inner)
    }
}

// `impl std::fmt::Display for UnquoteForm` + `impl std::str::FromStr
// for UnquoteForm` + `impl crate::ClosedSet for UnquoteForm` +
// `pub struct UnknownUnquoteForm(pub String)` are generated by
// `#[derive(tatara_lisp_derive::ClosedSet)]` on the enum declaration
// above. `label` delegates to the inherent `UnquoteForm::marker` via
// `#[closed_set(via = "marker")]` so the domain-canonical
// punctuation-marker projection (`","` / `",@"`) stays load-bearing at
// the inherent surface while the trait surface unifies every
// closed-set implementor's projection name onto `label`. The marker
// axis stays intentionally disjoint from the structural-axis
// `SexpShape` vocabulary (`"unquote"` / `"unquote-splice"`); the
// disjointness contract holds at the trait surface exactly because
// each implementor's `label` projects its own inherent
// axis-vocabulary. The `display` flag emits the substrate-wide
// `f.write_str(Self::marker(*self))` block.
// `#[closed_set(generate_unknown = "unquote form")]` emits the typed
// parse-rejection carrier with the substrate-wide `Debug + Clone +
// PartialEq + Eq + thiserror::Error` derives and the `#[error("unknown
// unquote form: {0}")]` annotation byte-for-byte; the explicit label
// matches the auto-derived `pascal_to_spaced_lowercase("UnquoteForm")`
// projection byte-for-byte but pins the pre-lift wording against any
// future change to the projection helper's behavior on this name. The
// FromStr decode is a linear sweep over `UnquoteForm::ALL` keyed on
// `marker`; round-trip + cross-axis rejection (`"unquote"` /
// `"unquote-splice"`) pinned by
// `unquote_form_marker_round_trips_through_from_str` +
// `unquote_form_from_str_rejects_sexp_shape_labels_on_template_marker_axis`.

/// Closed-set identifier for a kwargs-path projection — the `form:` label
/// shape that a typed-entry kwarg failure renders into the `compile error
/// in {form}:` prefix of a `LispError::TypeMismatch` diagnostic. Encodes
/// the three reachable path shapes the kwargs gate emits — `:<key>` for a
/// named kwarg (`extract_string` / `extract_int` / etc. failure),
/// `:<key>[<idx>]` for the Nth item of a list-typed kwarg
/// (`extract_string_list` per-item failure), and `kwargs[<idx>]` for an
/// even-position slot that failed the "this-position-must-be-a-keyword"
/// gate before a key was known (`parse_kwargs` direct call) — as a typed
/// borrowed enum, so authoring tools (REPL, LSP, `tatara-check`) bind to
/// path-shape identity (`KwargPath::Item { .. }` etc.) rather than
/// substring-matching the rendered prefix.
///
/// Mirror at the kwargs-path-shape boundary of the prior-run
/// `MacroDefHead` (macro-definition-head closed set),
/// `CompilerSpecIoStage` (disk-persistence surface),
/// `TemplateInvariantKind` (bytecode-runtime surface), and `UnquoteForm`
/// (template-marker syntactic forms) closed-set lifts: those enums key
/// their respective rejection variants on a typed identity carried inside
/// the variant's data shape; this enum keys the THREE distinct `form:`
/// label shapes emitted by the kwarg-gate's typed-entry chain on a typed
/// path identity. The three `format!` literals that used to live inline
/// in `domain.rs::kwarg_form` / `kwarg_item_form` / `kwargs_pos_form`
/// (three byte-identical `format!` shapes, one per helper) collapse into
/// ONE `Display` impl on this enum — the canonical literals (`":<key>"`
/// / `":<key>[<idx>]"` / `"kwargs[<idx>]"`) live in ONE place, so a typo
/// in any one of the three shapes can never drift independent of the
/// others (THEORY.md §VI.1 three-times rule). Adding a fourth path shape
/// (e.g., `:<key>.<field>` for nested-struct kwarg failures or
/// `:<key>::<variant>` for sum-typed kwarg failures) requires extending
/// this enum, which rustc-enforces matching at the `Display` projection
/// site.
///
/// `KwargPath` owns its `key` payload as `String` so it can inhabit
/// `LispError::TypeMismatch.form` (and any future error variant) without
/// a borrow constraint. The owned shape is the typed-slot promotion the
/// prior-run `KwargPath` landing pre-staged: every projection site that
/// used to produce a `String` via `KwargPath::Named(key).to_string()` (the
/// three sibling helpers `kwarg_form` / `kwarg_item_form` /
/// `kwargs_pos_form` and the fourth `kwarg_deserialize_form` helper) now
/// produces a typed `KwargPath` value directly; `type_mismatch` and every
/// `TypeMismatch.form` consumer pattern-match on the variant identity
/// (`KwargPath::Item { key, idx }`, `KwargPath::Slot(idx)`, etc.) instead
/// of substring-parsing the rendered prefix.
///
/// `Copy` is dropped because `String` is not `Copy`; `Clone + Debug +
/// PartialEq + Eq` are retained (same posture as every other owned-data
/// `LispError` field). The closed-set structural-completeness floor is
/// unchanged — only the data ownership changed.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the closed set of
/// kwargs-path shapes becomes a TYPE rather than three byte-identical
/// `format!` literals scattered across helper definitions. THEORY.md
/// §VI.1 — generation over composition; the typed enum lands the
/// structural-completeness floor for the kwargs-path surface, parallel
/// to how `CompilerSpecIoStage` lands it for the disk-persistence
/// surface, `MacroDefHead` for the macro-definition-head surface,
/// `TemplateInvariantKind` for the bytecode-runtime surface, and
/// `UnquoteForm` for the template-marker surface. THEORY.md §II.1
/// invariant 1 — typed entry; the kwargs-path's renderable identity is
/// part of the proof of WHICH kwarg-gate fired, and the typed enum makes
/// that identity first-class — now as load-bearing data on the
/// `TypeMismatch` variant rather than as a projection-to-String.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KwargPath {
    /// `:<key>` — failure at a named kwarg (`extract_string`,
    /// `extract_int`, `extract_float`, `extract_bool`, etc.). The `key`
    /// is the offending kwarg's identifier, owned so the variant lives
    /// independent of the call frame.
    Named(String),
    /// `:<key>[<idx>]` — failure at the Nth item of a list-typed kwarg
    /// (`extract_string_list` per-item failure). The `key` is the
    /// containing kwarg's identifier (owned); `idx` is the 0-based item
    /// index inside that kwarg's list value.
    Item { key: String, idx: usize },
    /// `kwargs[<idx>]` — failure at the Nth slot of the kwargs slice
    /// before a key was known (`parse_kwargs`'s
    /// "this-position-must-be-a-keyword" gate firing on an even-position
    /// slot). `idx` is the 0-based position into the raw kwargs slice
    /// (not into a particular kwarg's value).
    Slot(usize),
}

impl KwargPath {
    /// Owned constructor for the `:<key>` shape — used by every call site
    /// that has a `&str` borrow of the kwarg identifier and wants to lift
    /// it into the typed enum without an inline `.to_string()` projection.
    #[must_use]
    pub fn named(key: &str) -> Self {
        Self::Named(key.to_string())
    }

    /// Owned constructor for the `:<key>[<idx>]` shape — sibling of
    /// `named`, threading the per-item index alongside the kwarg key.
    #[must_use]
    pub fn item(key: &str, idx: usize) -> Self {
        Self::Item {
            key: key.to_string(),
            idx,
        }
    }

    /// Discriminator projection — strips the payload and returns the
    /// closed-set [`KwargPathKind`]. The same shape every sibling
    /// payload-carrying closed-set enum in the workspace projects through
    /// (e.g. `tatara_process::lifetime_clock::AutoTerminate::kind` →
    /// [`crate::error::KwargPath`]'s tatara-process cousin
    /// `AutoTerminateKind`, `TerminateReason::kind` →
    /// `TerminateReasonKind`, `crate::matrix::SelectStrategy::kind` →
    /// `SelectStrategyKind`).
    ///
    /// Consumers that group [`LispError::TypeMismatch`] /
    /// [`LispError::KwargDeserialize`] failures by path-shape
    /// CATEGORY rather than full path identity (failure-cluster metrics
    /// labelled `path_kind=named` / `path_kind=item` / `path_kind=slot`,
    /// an LSP that surfaces "this is a per-item failure" before drilling
    /// into the bracket-suffix, a future `tatara-check` diagnostic
    /// histogram that buckets by kind) project through this method
    /// instead of destructuring the variant and discarding the payload at
    /// every site. Adding a fourth path shape (e.g., `:<key>.<field>`
    /// for nested-struct kwarg failures or `:<key>::<variant>` for
    /// sum-typed kwarg failures) requires extending [`KwargPathKind`],
    /// which rustc-enforces matching at this projection.
    #[must_use]
    pub const fn kind(&self) -> KwargPathKind {
        match self {
            Self::Named(_) => KwargPathKind::Named,
            Self::Item { .. } => KwargPathKind::Item,
            Self::Slot(_) => KwargPathKind::Slot,
        }
    }
}

impl std::fmt::Display for KwargPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Named(key) => write!(f, ":{key}"),
            Self::Item { key, idx } => write!(f, ":{key}[{idx}]"),
            Self::Slot(idx) => write!(f, "kwargs[{idx}]"),
        }
    }
}

/// The closed set of [`KwargPath`] kinds — the discriminator view,
/// payload-stripped, that sibling closed-set lifts in this crate carry
/// (see [`SexpShape`], [`ExpectedKwargShape`], [`MacroDefHead`],
/// [`UnquoteForm`], [`CompilerSpecIoStage`], [`TemplateInvariantKind`]).
///
/// Mirrors the workspace-wide [`payload-carrier, payload-stripped kind]
/// pairing — `AutoTerminate` / `AutoTerminateKind`, `TerminateReason` /
/// `TerminateReasonKind`, `SelectStrategy` / `SelectStrategyKind`,
/// `ChannelVariant` / `ChannelKind`, `ArtifactVariant` / `ArtifactKind`,
/// `EncapsulationTarget` / `EncapsulationKind`. [`KwargPath`] owns the
/// per-variant payload (`key: String`, `idx: usize`); [`KwargPathKind`]
/// is the `Copy`-able discriminator view callers reach when they want
/// the CATEGORY without the payload.
///
/// Drives the `label` / [`Display`] / [`FromStr`] triad over [`Self::ALL`]
/// so a new variant added with an `ALL` entry automatically extends the
/// parser, the canonical wire-format projection, and any future
/// metrics-label / failure-cluster bucket that needs to enumerate the
/// kwargs-path categories. The `[Self; 3]` array literal forces the arity
/// so a fourth variant — a hypothetical `Field` for nested-struct kwarg
/// failures (`:<key>.<field>`) or `Variant` for sum-typed kwarg failures
/// (`:<key>::<variant>`) — cannot land without bumping the constant.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the
/// payload-stripped kind becomes a TYPE rather than three byte-identical
/// `matches!` discriminator literals scattered across consumers.
/// THEORY.md §VI.1 — generation over composition; the typed kind enum
/// lands the structural-completeness floor for the path-shape category
/// surface, parallel to how [`KwargPath`] lands it for the path-identity
/// surface, [`ExpectedKwargShape`] for the expected-shape surface, etc.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_lisp_derive::ClosedSet)]
#[closed_set(via = "label", display, generate_unknown = "kwarg path kind")]
pub enum KwargPathKind {
    /// The kind-view of [`KwargPath::Named`] — `:<key>` failures at a
    /// named kwarg (typed-atom extractors, `Option<T>` paths).
    Named,
    /// The kind-view of [`KwargPath::Item`] — `:<key>[<idx>]` failures
    /// at the Nth item of a list-typed kwarg
    /// (`extract_string_list` per-item).
    Item,
    /// The kind-view of [`KwargPath::Slot`] — `kwargs[<idx>]` failures
    /// at a kwargs slice slot before a key was known
    /// (`parse_kwargs`'s slot-must-be-a-keyword gate).
    Slot,
}

impl KwargPathKind {
    /// The closed set — single source of truth for [`Self::label`] /
    /// [`Display`] / [`FromStr`]. The `[Self; 3]` arity is forced at the
    /// declaration so a fourth variant cannot land without bumping the
    /// constant.
    pub const ALL: [Self; 3] = [Self::Named, Self::Item, Self::Slot];

    /// Project the typed [`KwargPathKind`] to the canonical `&'static str`
    /// literal — lowercase byte-equal to the variant name (`"named"` /
    /// `"item"` / `"slot"`). The labels are kept distinct from the
    /// [`KwargPath::Display`] renderings (`":<key>"` / `":<key>[<idx>]"`
    /// / `"kwargs[<idx>]"`) because this projection names the CATEGORY,
    /// not the rendered identity — a metrics label `path_kind="named"`
    /// makes more sense at the kind boundary than a path-prefix template
    /// would.
    ///
    /// Same shape every sibling kind-projection in the workspace uses
    /// (`AutoTerminateKind::as_str`, `TerminateReasonKind::as_str`,
    /// [`SexpShape::label`], [`ExpectedKwargShape::label`]).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Named => "named",
            Self::Item => "item",
            Self::Slot => "slot",
        }
    }
}

// `impl std::fmt::Display for KwargPathKind` + `impl std::str::FromStr
// for KwargPathKind` + `impl crate::ClosedSet for KwargPathKind` +
// `pub struct UnknownKwargPathKind(pub String)` are generated by
// `#[derive(tatara_lisp_derive::ClosedSet)]` on the enum declaration
// above. `label` delegates to the inherent `KwargPathKind::label` via
// `#[closed_set(via = "label")]` — the inherent name coincides with
// the trait method name here, but the delegation stays explicit so the
// SAME wiring shape applies whether the inherent projection is named
// `label` / `prefix` / `marker` / `keyword` / `as_str`. The `display`
// flag emits the substrate-wide `f.write_str(Self::label(*self))` block.
// `#[closed_set(generate_unknown = "kwarg path kind")]` emits the typed
// parse-rejection carrier with the substrate-wide `Debug + Clone +
// PartialEq + Eq + thiserror::Error` derives and the `#[error("unknown
// kwarg path kind: {0}")]` annotation byte-for-byte; the explicit label
// matches the auto-derived `pascal_to_spaced_lowercase("KwargPathKind")`
// projection byte-for-byte but pins the pre-lift wording. Round-trip +
// cross-axis rejection (path-rendering literals `":foo"` / `":foo[0]"` /
// `"kwargs[0]"`) pinned by
// `kwarg_path_kind_label_round_trips_through_from_str` +
// `unknown_kwarg_path_kind_carries_offending_input_verbatim`.

/// Closed-set identifier for the `expected:` slot of a
/// `LispError::TypeMismatch` diagnostic — the seven reachable
/// expected-shape labels the typed-entry kwarg gate emits:
/// `Keyword` (the `parse_kwargs` slot-must-be-a-keyword gate),
/// `String` / `Int` / `Number` / `Bool` (the typed-atom extractors —
/// `extract_string`, `extract_int`, `extract_float`, `extract_bool`,
/// and their `Option` siblings, plus `extract_string_list`'s per-item
/// `string` gate), `List` (the `extract_vec_via_serde` outer-shape
/// gate), and `ListOfStrings` (the `extract_string_list` outer-shape
/// gate). Encoded as a typed enum so the closed set becomes
/// load-bearing data on `LispError::TypeMismatch.expected` rather than
/// a `&'static str` literal scattered across eleven call sites in
/// `domain.rs`.
///
/// Mirror at the expected-shape boundary of the prior-run `KwargPath`
/// (kwargs-path-shape closed set), `MacroDefHead` (macro-definition-
/// head closed set), `CompilerSpecIoStage` (disk-persistence surface),
/// `TemplateInvariantKind` (bytecode-runtime surface), and
/// `UnquoteForm` (template-marker syntactic forms) closed-set lifts:
/// those enums key their respective rejection variants on a typed
/// identity carried inside the variant's data shape; this enum keys
/// the SECOND slot (`expected`) of every `LispError::TypeMismatch`
/// site on a typed expected-shape identity, alongside the
/// already-typed `form: KwargPath`. After this lift the type-mismatch
/// variant's identity is fully closed-set typed in TWO of its three
/// slots — only `got: &'static str` remains as a `&'static str`
/// projection, and that slot's compile-time guarantee is sourced from
/// `crate::domain::sexp_type_name`'s exhaustive `Sexp` match.
///
/// Adding a future expected-shape (e.g. `Float` once `extract_float`
/// stops accepting integers, `Symbol` if a future extractor accepts
/// only `Sexp::Atom(Symbol)`, or a parameterized `ListOf(Box<Self>)`
/// for nested-typed-vec extractors) requires extending this enum,
/// which rustc-enforces matching at every projection site
/// (`label()`).
///
/// `label(self) -> &'static str` projects the typed `ExpectedKwargShape`
/// back to the canonical literal for `LispError::Display` rendering.
/// The `&'static str` lifetime is load-bearing: it lets the variant
/// project through this method into the `expected {expected}` slot of
/// the `#[error(...)]` annotation without an allocation, parallel to
/// how `MacroDefHead::keyword()`, `UnquoteForm::marker()`, and
/// `CompilerSpecIoStage::operation()` / `label()` feed their
/// respective `LispError::*` Display impls.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the closed set
/// of expected-shape labels becomes a TYPE rather than eleven
/// `&'static str` literal call sites scattered across the kwarg
/// extractors. A typo in any literal can never drift into the
/// diagnostic at runtime; a regression that drifts the expected-shape
/// label (e.g. a typo `"strin"` for `"string"`) becomes a type error
/// at the call site, not a runtime substring drift. THEORY.md §VI.1 —
/// generation over composition; the typed enum lands the structural-
/// completeness floor for the expected-shape surface, parallel to how
/// `KwargPath` lands it for the kwargs-path surface, `MacroDefHead`
/// for the macro-definition-head surface, `UnquoteForm` for the
/// template-marker surface, `CompilerSpecIoStage` for the disk-
/// persistence surface, and `TemplateInvariantKind` for the bytecode-
/// runtime surface. THEORY.md §II.1 invariant 1 — typed entry; the
/// expected-shape identity is part of the proof of WHICH typed-entry
/// kwarg gate fired, and the typed enum makes that identity first-
/// class as load-bearing data on the variant rather than as a
/// projection-to-String at the helper boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, tatara_lisp_derive::ClosedSet)]
#[closed_set(via = "label", display, generate_unknown = "expected kwarg shape")]
pub enum ExpectedKwargShape {
    /// `"keyword"` — emitted by `parse_kwargs`'s
    /// "this-position-must-be-a-keyword" gate when an even-position
    /// slot in the kwargs slice isn't a `Sexp::Atom(Keyword(_))`.
    Keyword,
    /// `"string"` — emitted by `extract_string` /
    /// `extract_optional_string` (the kwarg's value isn't a
    /// `Sexp::Atom(Str(_))`) AND by `extract_string_list`'s per-item
    /// gate (an item inside a list-typed kwarg isn't a string).
    String,
    /// `"int"` — emitted by `extract_int` / `extract_optional_int`
    /// when the kwarg's value isn't a `Sexp::Atom(Int(_))`.
    Int,
    /// `"number"` — emitted by `extract_float` /
    /// `extract_optional_float` when the kwarg's value isn't a
    /// numeric atom. Wider than `Int`: `extract_float` accepts both
    /// `Sexp::Atom(Float(_))` and `Sexp::Atom(Int(_))` via
    /// `Sexp::as_float`, so the expected-shape label is the union
    /// "number" rather than the narrower "float".
    Number,
    /// `"bool"` — emitted by `extract_bool` / `extract_optional_bool`
    /// when the kwarg's value isn't a `Sexp::Atom(Bool(_))`.
    Bool,
    /// `"list"` — emitted by `extract_vec_via_serde`'s outer-shape
    /// gate when the kwarg's value isn't a `Sexp::List(_)`. Used by
    /// the universal-Deserialize fallthrough for any `Vec<T>` field.
    List,
    /// `"list of strings"` — emitted by `extract_string_list`'s
    /// outer-shape gate when the kwarg's value isn't a
    /// `Sexp::List(_)`. Wider than `List`: names the expected
    /// element-type so the diagnostic reads `expected list of
    /// strings, got string` instead of the bare `expected list, got
    /// string`. The per-item gate fires `String` (the narrower
    /// expected-shape for the element-type failure).
    ListOfStrings,
}

impl ExpectedKwargShape {
    /// The closed set of seven reachable expected-kwarg shapes — single
    /// source of truth that drives the [`Self::label`] / [`fmt::Display`]
    /// projection AND the [`FromStr`] decode sweep keyed on
    /// [`Self::label`]. Adding a hypothetical eighth variant (e.g.
    /// `Float` once `extract_float` stops accepting integers, `Symbol`
    /// if a future extractor accepts only `Sexp::Atom(Symbol)`, or a
    /// parameterized `ListOf(Box<Self>)` for nested-typed-vec
    /// extractors) lands at one [`Self::ALL`] entry + one [`Self::label`]
    /// arm — exhaustively checked by the compiler (the `[Self; 7]`
    /// array literal forces the arity) AND by the per-variant
    /// truth-table tests below.
    ///
    /// Sibling closed-set lift to every other typed-shape enum the
    /// substrate carries: this crate's own [`SexpShape::ALL`] (the
    /// twelve reachable outer shapes the reader can produce — peer
    /// axis on the same `Sexp` algebra, whose vocabulary overlaps
    /// with this set on five of seven entries — `"keyword"`,
    /// `"string"`, `"int"`, `"bool"`, `"list"` — and does NOT overlap
    /// on two — `"number"` ⊎ `"list of strings"`; the overlap is
    /// intentional and pinned by the cross-axis tests), and across
    /// the workspace ([`MacroDefHead::ALL`], [`UnquoteForm::ALL`],
    /// [`crate::ast::AtomKind::ALL`], [`crate::ast::QuoteForm::ALL`],
    /// `ConditionKind::ALL`, `ProcessPhase::ALL`,
    /// `ProcessSignal::ALL`, `ChannelKind::ALL`, `IntentKind::ALL`,
    /// `LifetimeKind::ALL`, `RequestorKind::ALL`, `ReceiptKind::ALL`,
    /// …) every one of which paired its typed projection with `ALL`
    /// before this lift.
    ///
    /// Future consumers that compose against `ALL`: LSP / REPL
    /// completion for the operator-facing rendered expected-shape
    /// label (every `expected X, ...` substring in `LispError`'s
    /// rendered diagnostics keys on this set's projection through
    /// [`Self::label`]); `tatara-check` coverage assertions over
    /// which expected-shape variants reach a `TypeMismatch.expected`
    /// arm at all — the typed sweep replaces the per-call-site
    /// vocabulary of seven `&'static str` literals; any future
    /// audit-trail metric jointly labeled by [`Self::label`] (e.g.
    /// `tatara_lisp_type_mismatch_total{expected="number"}`) — the
    /// metric label set IS [`Self::ALL`] mapped through
    /// [`Self::label`].
    pub const ALL: [Self; 7] = [
        Self::Keyword,
        Self::String,
        Self::Int,
        Self::Number,
        Self::Bool,
        Self::List,
        Self::ListOfStrings,
    ];

    /// Project the typed `ExpectedKwargShape` to the canonical
    /// `&'static str` literal — feeds the `LispError::TypeMismatch`
    /// Display rendering via the `#[error(...)]` annotation. The
    /// `&'static str` lifetime is load-bearing: it lets the variant
    /// project through this method into the `expected {expected}` slot
    /// of the `#[error(...)]` annotation without an allocation,
    /// parallel to how `MacroDefHead::keyword()`,
    /// `UnquoteForm::marker()`, [`SexpShape::label`], and
    /// `CompilerSpecIoStage::operation()` / `label()` feed their
    /// respective `LispError::*` Display impls.
    ///
    /// The bidirectional contract is anchored by tests:
    /// `label_renders_canonical_string_for_every_variant` pins each
    /// variant's canonical literal so a typo in any arm fails-loudly,
    /// `display_matches_label_for_every_variant` pins
    /// Display-equals-label so the `#[error(...)]` annotation's
    /// `{expected}` slot projects byte-for-byte through this method,
    /// and `expected_kwarg_shape_label_round_trips_through_from_str`
    /// pins the `label` ↔ [`FromStr`] round-trip for every variant in
    /// [`Self::ALL`] so the typed surface and the rendered diagnostic
    /// literal cannot drift.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::String => "string",
            Self::Int => "int",
            Self::Number => "number",
            Self::Bool => "bool",
            Self::List => "list",
            Self::ListOfStrings => "list of strings",
        }
    }
}

// `impl std::fmt::Display for ExpectedKwargShape` +
// `impl std::str::FromStr for ExpectedKwargShape` +
// `impl crate::ClosedSet for ExpectedKwargShape` +
// `pub struct UnknownExpectedKwargShape(pub String)` are generated by
// `#[derive(tatara_lisp_derive::ClosedSet)]` on the enum declaration
// above. `label` delegates to the inherent `ExpectedKwargShape::label`
// via `#[closed_set(via = "label")]` — the inherent name coincides
// with the trait method name here; the delegation stays explicit so
// the SAME wiring shape applies whether the inherent projection is
// `label` / `prefix` / `marker` / `keyword` / `as_str`. The `display`
// flag emits the substrate-wide `f.write_str(Self::label(*self))`
// block. `#[closed_set(generate_unknown = "expected kwarg shape")]`
// emits the typed parse-rejection carrier with the substrate-wide
// `Debug + Clone + PartialEq + Eq + thiserror::Error` derives and the
// `#[error("unknown expected kwarg shape: {0}")]` annotation
// byte-for-byte; the explicit label matches the auto-derived
// `pascal_to_spaced_lowercase("ExpectedKwargShape")` projection
// byte-for-byte but pins the pre-lift wording. Round-trip + cross-axis
// rejection (`SexpShape` structural labels `"nil"` / `"symbol"` /
// `"float"` / `"quote"` / `"quasiquote"` / `"unquote"` /
// `"unquote-splice"`) pinned by
// `expected_kwarg_shape_label_round_trips_through_from_str` +
// `expected_kwarg_shape_from_str_accepts_only_canonical_labels`.

/// Closed-set identifier for the outermost shape of a `Sexp` — the twelve
/// reachable shapes the reader can produce (`Nil` ⊎ `Symbol` ⊎ `Keyword` ⊎
/// `String` ⊎ `Int` ⊎ `Float` ⊎ `Bool` ⊎ `List` ⊎ `Quote` ⊎ `Quasiquote` ⊎
/// `Unquote` ⊎ `UnquoteSplice`). Carried as a typed slot on
/// `LispError::TypeMismatch.got` and `LispError::NamedFormNonSymbolName.got`
/// so authoring tools (REPL, LSP, `tatara-check`) bind to variant identity
/// (`SexpShape::Int` etc.) directly rather than substring-matching the
/// rendered `got` literal.
///
/// Mirror at the observed-shape boundary of the prior-run `KwargPath`
/// (kwargs-path-shape closed set), `ExpectedKwargShape` (kwarg-gate's
/// expected-shape closed set), `MacroDefHead` (macro-definition-head
/// closed set), `CompilerSpecIoStage` (disk-persistence surface),
/// `TemplateInvariantKind` (bytecode-runtime surface), and `UnquoteForm`
/// (template-marker syntactic forms) closed-set lifts: those enums key
/// their respective rejection variants on a typed identity carried inside
/// the variant's data shape; this enum keys the THIRD slot (`got`) of
/// every `LispError::TypeMismatch` site on a typed observed-shape identity
/// — alongside the already-typed `form: KwargPath` and
/// `expected: ExpectedKwargShape`. After this lift the type-mismatch
/// variant's identity is fully closed-set typed in ALL THREE of its slots
/// — no `&'static str` projection at any helper boundary, every reachable
/// identity encoded as a variant of a typed enum.
///
/// Adding a future `Sexp` variant (e.g. a hypothetical `Sexp::Vector` for
/// `#(...)` reader syntax, or `Sexp::Map` for `{...}`) requires extending
/// this enum, which rustc-enforces matching at every projection site
/// (`label()`, `crate::domain::sexp_shape`).
///
/// `label(self) -> &'static str` projects the typed `SexpShape` back to
/// the canonical literal for `LispError::Display` rendering. The
/// `&'static str` lifetime is load-bearing: it lets the variant project
/// through this method into the `got {got}` slot of the `#[error(...)]`
/// annotation without an allocation, parallel to how
/// `ExpectedKwargShape::label()`, `MacroDefHead::keyword()`,
/// `UnquoteForm::marker()`, and `CompilerSpecIoStage::operation()` /
/// `label()` feed their respective `LispError::*` Display impls.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the closed set of
/// observed-Sexp shapes becomes a TYPE rather than a `&'static str`
/// projection through a string-keyed helper. A regression that drifts the
/// observed-shape label (e.g. a typo `"strin"` for `"string"`) becomes a
/// type error at the call site, not a runtime substring drift. THEORY.md
/// §VI.1 — generation over composition; the typed enum lands the
/// structural-completeness floor for the observed-shape surface, parallel
/// to how `ExpectedKwargShape` lands it for the expected-shape surface,
/// `KwargPath` for the kwargs-path surface, `MacroDefHead` for the
/// macro-definition-head surface, `UnquoteForm` for the template-marker
/// surface, `CompilerSpecIoStage` for the disk-persistence surface, and
/// `TemplateInvariantKind` for the bytecode-runtime surface. THEORY.md
/// §II.1 invariant 1 — typed entry; the observed-shape identity is part
/// of the proof of WHAT the typed-entry gate observed, and the typed enum
/// makes that identity first-class as load-bearing data on the variant
/// rather than as a projection-to-`&'static str` at the helper boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, tatara_lisp_derive::ClosedSet)]
#[closed_set(via = "label", display, generate_unknown = "sexp shape")]
pub enum SexpShape {
    /// `"nil"` — `Sexp::Nil`.
    Nil,
    /// `"symbol"` — `Sexp::Atom(Symbol(_))`.
    Symbol,
    /// `"keyword"` — `Sexp::Atom(Keyword(_))`.
    Keyword,
    /// `"string"` — `Sexp::Atom(Str(_))`.
    String,
    /// `"int"` — `Sexp::Atom(Int(_))`.
    Int,
    /// `"float"` — `Sexp::Atom(Float(_))`.
    Float,
    /// `"bool"` — `Sexp::Atom(Bool(_))`.
    Bool,
    /// `"list"` — `Sexp::List(_)`.
    List,
    /// `"quote"` — `Sexp::Quote(_)`.
    Quote,
    /// `"quasiquote"` — `Sexp::Quasiquote(_)`.
    Quasiquote,
    /// `"unquote"` — `Sexp::Unquote(_)`.
    Unquote,
    /// `"unquote-splice"` — `Sexp::UnquoteSplice(_)`.
    UnquoteSplice,
}

impl SexpShape {
    /// The closed set of reachable `Sexp` outermost shapes — single
    /// source of truth that drives the [`Self::label`] / [`fmt::Display`]
    /// projection AND the [`FromStr`] decode sweep keyed on
    /// [`Self::label`]. Adding a hypothetical thirteenth variant (e.g.
    /// `Vector` for `#(...)` reader syntax, `Map` for `{...}`, or
    /// `Char` for `#\x`) lands at one `ALL` entry + one `label` arm —
    /// exhaustively checked by the compiler (the `[Self; 12]` array
    /// literal forces the arity) AND by the per-variant truth-table
    /// tests below. Sibling closed-set lift to every other typed-shape
    /// enum the substrate carries: this crate's own [`UnquoteForm`]
    /// (the four template markers — the only other closed set on the
    /// `Sexp` algebra with `Sexp variant ↔ enum variant` parity), and
    /// the cross-crate `tatara-process` family
    /// (`ConditionKind::ALL`, `ProcessPhase::ALL`,
    /// `ProcessSignal::ALL`, `ChannelKind::ALL`, `IntentKind::ALL`,
    /// …) every one of which paired its typed projection with `ALL`
    /// before this lift.
    ///
    /// Future consumers that compose against `ALL`:
    /// - LSP / REPL completion for the operator-facing rendered
    ///   shape label (every `expected X, got Y` substring in
    ///   `LispError`'s rendered diagnostics keys on this set);
    /// - `tatara-check` coverage assertions over which `SexpShape`
    ///   variants reach a `TypeMismatch.got` arm at all;
    /// - any future audit-trail metric jointly labeled by
    ///   `SexpShape::label` (e.g.
    ///   `tatara_lisp_type_mismatch_total{got="symbol"}`) — the
    ///   metric label set IS [`Self::ALL`] mapped through
    ///   [`Self::label`].
    pub const ALL: [Self; 12] = [
        Self::Nil,
        Self::Symbol,
        Self::Keyword,
        Self::String,
        Self::Int,
        Self::Float,
        Self::Bool,
        Self::List,
        Self::Quote,
        Self::Quasiquote,
        Self::Unquote,
        Self::UnquoteSplice,
    ];

    /// Project the typed `SexpShape` to the canonical `&'static str`
    /// literal — feeds the `LispError::TypeMismatch` /
    /// `LispError::NamedFormNonSymbolName` Display rendering via the
    /// `#[error(...)]` annotation. The `&'static str` lifetime is
    /// load-bearing: it lets the variant project through this method into
    /// the `got {got}` slot without an allocation, parallel to how
    /// `ExpectedKwargShape::label()`, `MacroDefHead::keyword()`,
    /// `UnquoteForm::marker()`, and `CompilerSpecIoStage::operation()` /
    /// `label()` feed their respective `LispError::*` Display impls.
    ///
    /// The bidirectional contract is anchored by tests:
    /// `sexp_shape_label_renders_canonical_string_for_every_variant` pins
    /// each variant's canonical literal so a typo in any arm fails-loudly,
    /// `sexp_shape_display_matches_label_for_every_variant` pins
    /// Display-equals-label so the `#[error(...)]` annotation's `{got}`
    /// slot projects byte-for-byte through this method, and
    /// `sexp_shape_label_round_trips_through_from_str` pins the
    /// `label` ↔ `FromStr` round-trip for every variant in
    /// [`Self::ALL`] so the typed surface and the rendered diagnostic
    /// literal cannot drift.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Nil => "nil",
            Self::Symbol => "symbol",
            Self::Keyword => "keyword",
            Self::String => "string",
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
            Self::List => "list",
            Self::Quote => "quote",
            Self::Quasiquote => "quasiquote",
            Self::Unquote => "unquote",
            Self::UnquoteSplice => "unquote-splice",
        }
    }

    /// Project the twelve-variant [`SexpShape`] back to its
    /// corresponding [`crate::ast::AtomKind`] iff the shape names an
    /// atomic-payload variant — `Symbol → Some(AtomKind::Symbol)`,
    /// `Keyword → Some(AtomKind::Keyword)`, `String → Some(AtomKind::Str)`,
    /// `Int → Some(AtomKind::Int)`, `Float → Some(AtomKind::Float)`,
    /// `Bool → Some(AtomKind::Bool)`, every other shape (`Nil`,
    /// `List`, every quote-family wrapper) `None`. The 6-of-12 carving
    /// of [`SexpShape`] that the inverse [`crate::ast::AtomKind::sexp_shape`]
    /// embed projection covers — naming the inverse closes the
    /// embed/project section on the (atomic-payload, outer-shape)
    /// algebra.
    ///
    /// Composition law (round-trip on the atom carving):
    /// `AtomKind::sexp_shape(k).as_atom_kind() == Some(k)` for every
    /// `k: AtomKind`. The non-atom shapes form the kernel of the
    /// projection — `SexpShape::List.as_atom_kind() == None`,
    /// `SexpShape::Nil.as_atom_kind() == None`, every quote-family
    /// variant also `None`. The two projections together form an
    /// `Iso(AtomKind, AtomShape ⊂ SexpShape)`: `as_atom_kind` is the
    /// section (every `AtomKind` round-trips through the embed),
    /// `sexp_shape` is the retraction (every atom-shape pre-image
    /// recovers the typed marker). Disjoint with [`Self::as_quote_form`]:
    /// for every variant in [`Self::ALL`], at most ONE of the two
    /// projections returns `Some` — the typed-shape lattice's two
    /// closed-set carvings partition the carve-able SexpShape variants
    /// (every variant is in exactly one of AtomShape, QuoteShape, or
    /// neither — the latter being `Nil` and `List`).
    ///
    /// Pre-lift the docstring on [`crate::ast::AtomKind::sexp_shape`]
    /// explicitly anticipated this dual:
    ///
    /// > "the dual projection `SexpShape::as_atom_kind(self) ->
    /// > Option<AtomKind>` is NOT currently provided because no
    /// > consumer needs it ... If a future authoring tool (LSP /
    /// > REPL / `tatara-check` typed-pattern matcher) wants to lift
    /// > the typed marker out of a diagnostic-side shape identity,
    /// > the dual lands as ONE new `match` over the closed set,
    /// > parallel to the structure here."
    ///
    /// Post-lift the dual lives at this method; the `AtomKind::sexp_shape`
    /// docstring's anticipation is satisfied. Two plausible future
    /// consumers the closed-set projection admits with no new
    /// boilerplate:
    ///   * **LSP / REPL typed-shape filter** — given a
    ///     `LispError::TypeMismatch { got: SexpShape, .. }`, project
    ///     the offending shape through `as_atom_kind()`; iff `Some(k)`,
    ///     the offending value was an atomic payload and the LSP can
    ///     route through `AtomKind`'s diagnostic / completion surface
    ///     (`AtomKind::label`, `AtomKind::ALL`-driven completion list)
    ///     without re-deriving the 6-of-12 carving inline. Iff `None`,
    ///     the surface dispatches to the structural (list / quote-family /
    ///     nil) branch instead.
    ///   * **`tatara-check` typed-pattern matcher** — a future
    ///     `(check-shape-projects-to-atom-kind …)` substrate primitive
    ///     binds to this projection rather than substring-matching the
    ///     rendered label or duplicating the 6-of-12 carving on its
    ///     own.
    ///   * **Diagnostic-side audit-trail metric** — a future
    ///     `tatara_lisp_type_mismatch_atom_kind_total{kind="symbol"}`
    ///     metric reads the typed `AtomKind` directly off the rejected
    ///     `SexpShape` via this projection, with the carving's
    ///     correctness pinned at ONE site (the typed match here)
    ///     rather than across N per-metric inline `match` arms.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the
    /// (SexpShape variant, AtomKind variant) inverse pairing becomes a
    /// TYPE projection on the substrate algebra rather than an inline
    /// 6-of-12 `match` re-derived per call site. THEORY.md §II.1
    /// invariant 2 — free middle; the (atomic-payload, outer-shape)
    /// algebra now binds at TWO typed sites (`AtomKind::sexp_shape`
    /// for the embed, `SexpShape::as_atom_kind` for the project), each
    /// rebuilding the same closed-set pairing — a regression that
    /// drifts ONE direction from the other fails the
    /// `atom_kind_sexp_shape_round_trips_through_as_atom_kind` round-
    /// trip pin below. THEORY.md §VI.1 — generation over composition;
    /// the inverse pairing's ONE typed `match` displaces the speculative
    /// per-consumer 6-of-12 carving (LSP, REPL, `tatara-check`,
    /// metrics) the substrate's authoring surface roadmap anticipates.
    ///
    /// Frontier inspiration: MLIR's `mlir::dyn_cast<AtomicAttr>(attr)`
    /// — the typed soft-downcast from a generic `Attribute` to a
    /// narrower typed `AtomicAttr` interface IS the closed-set
    /// project direction; `SexpShape::as_atom_kind` is the
    /// unstructured-Rust peer on the substrate's typed-shape algebra,
    /// with [`crate::ast::AtomKind`] standing in for MLIR's
    /// `AtomicAttr` interface. Racket's `(syntax->datum stx)` paired
    /// with a closed-set predicate (`number?`, `symbol?`, `string?`,
    /// `boolean?`) — the typed predicate face IS the project direction
    /// on Racket's syntax-datum taxonomy; the substrate's
    /// `as_atom_kind` is the Rust-typed peer where the predicate
    /// surfaces the typed witness alongside the predicate verdict in
    /// ONE `Option<AtomKind>` projection.
    #[must_use]
    pub fn as_atom_kind(self) -> Option<crate::ast::AtomKind> {
        use crate::ast::AtomKind;
        match self {
            Self::Symbol => Some(AtomKind::Symbol),
            Self::Keyword => Some(AtomKind::Keyword),
            Self::String => Some(AtomKind::Str),
            Self::Int => Some(AtomKind::Int),
            Self::Float => Some(AtomKind::Float),
            Self::Bool => Some(AtomKind::Bool),
            Self::Nil
            | Self::List
            | Self::Quote
            | Self::Quasiquote
            | Self::Unquote
            | Self::UnquoteSplice => None,
        }
    }

    /// Project the twelve-variant [`SexpShape`] back to its
    /// corresponding [`crate::ast::QuoteForm`] iff the shape names a
    /// homoiconic quote-family wrapper — `Quote → Some(QuoteForm::Quote)`,
    /// `Quasiquote → Some(QuoteForm::Quasiquote)`, `Unquote →
    /// Some(QuoteForm::Unquote)`, `UnquoteSplice →
    /// Some(QuoteForm::UnquoteSplice)`, every other shape (`Nil`,
    /// `List`, every atomic-payload variant) `None`. The 4-of-12
    /// carving of [`SexpShape`] that the inverse
    /// [`crate::ast::QuoteForm::sexp_shape`] embed projection covers —
    /// naming the inverse closes the embed/project section on the
    /// (quote-family, outer-shape) algebra, sibling to
    /// [`Self::as_atom_kind`] on the atomic-payload axis.
    ///
    /// Composition law (round-trip on the quote-family carving):
    /// `QuoteForm::sexp_shape(qf).as_quote_form() == Some(qf)` for
    /// every `qf: QuoteForm`. The non-quote-family shapes form the
    /// kernel of the projection — `SexpShape::List.as_quote_form() ==
    /// None`, `SexpShape::Nil.as_quote_form() == None`, every atomic-
    /// payload variant also `None`. The two projections together form
    /// an `Iso(QuoteForm, QuoteShape ⊂ SexpShape)`: `as_quote_form`
    /// is the section (every `QuoteForm` round-trips through the
    /// embed), `sexp_shape` is the retraction (every quote-shape
    /// pre-image recovers the typed marker). Disjoint with
    /// [`Self::as_atom_kind`]: for every variant in [`Self::ALL`], at
    /// most ONE of the two projections returns `Some` — the typed-
    /// shape lattice's two closed-set carvings partition the carve-
    /// able SexpShape variants (every variant is in exactly one of
    /// AtomShape, QuoteShape, or neither — the latter being `Nil`
    /// and `List`).
    ///
    /// Pre-lift the docstring on [`crate::ast::QuoteForm::sexp_shape`]
    /// explicitly anticipated this dual:
    ///
    /// > "the dual projection `SexpShape::as_quote_form(self) ->
    /// > Option<QuoteForm>` is NOT currently provided because no
    /// > consumer needs it ... If a future authoring tool (LSP /
    /// > REPL / `tatara-check` typed-pattern matcher) wants to lift
    /// > the typed marker out of a diagnostic-side shape identity,
    /// > the dual lands as ONE new match on the closed set, parallel
    /// > to the structure here."
    ///
    /// Post-lift the dual lives at this method; the
    /// `QuoteForm::sexp_shape` docstring's anticipation is satisfied.
    /// The same plausible future consumers [`Self::as_atom_kind`]
    /// documents apply on the quote-family axis — a typed-shape
    /// filter that narrows a diagnostic to "this rejection was on a
    /// homoiconic prefix-wrapper" iff `as_quote_form().is_some()`
    /// binds to ONE projection rather than re-deriving the 4-of-12
    /// carving inline. A future `tatara-check` predicate
    /// `(check-shape-projects-to-quote-form …)` reads the typed
    /// `QuoteForm` directly off a rejected `SexpShape` via this
    /// projection.
    ///
    /// Theory anchor: same as [`Self::as_atom_kind`]. THEORY.md §V.1
    /// (knowable platform; the inverse pairing is a TYPE projection
    /// rather than an inline 4-of-12 `match`), THEORY.md §II.1
    /// invariant 2 (free middle; the embed/project pair binds at TWO
    /// typed sites — `QuoteForm::sexp_shape` for the embed,
    /// `SexpShape::as_quote_form` for the project — both rebuilding
    /// the same closed-set pairing), THEORY.md §VI.1 (generation
    /// over composition; the inverse 4-of-12 carving lifts to ONE
    /// typed `match`).
    ///
    /// Frontier inspiration: same as [`Self::as_atom_kind`] — MLIR's
    /// `mlir::dyn_cast<QuoteAttr>(attr)` typed soft-downcast on the
    /// quote-family carving of a closed-set attribute union, with
    /// Racket's `(or (quote-syntax? stx) (quasiquote-syntax? stx)
    /// (unquote-syntax? stx) (unquote-splicing-syntax? stx))` as the
    /// closed-form predicate-family sibling whose Rust-typed peer
    /// surfaces the typed witness alongside the predicate verdict in
    /// ONE `Option<QuoteForm>` projection.
    #[must_use]
    pub fn as_quote_form(self) -> Option<crate::ast::QuoteForm> {
        use crate::ast::QuoteForm;
        match self {
            Self::Quote => Some(QuoteForm::Quote),
            Self::Quasiquote => Some(QuoteForm::Quasiquote),
            Self::Unquote => Some(QuoteForm::Unquote),
            Self::UnquoteSplice => Some(QuoteForm::UnquoteSplice),
            Self::Nil
            | Self::List
            | Self::Symbol
            | Self::Keyword
            | Self::String
            | Self::Int
            | Self::Float
            | Self::Bool => None,
        }
    }
}

// `impl std::fmt::Display for SexpShape` + `impl std::str::FromStr for
// SexpShape` + `impl crate::ClosedSet for SexpShape` +
// `pub struct UnknownSexpShape(pub String)` are generated by
// `#[derive(tatara_lisp_derive::ClosedSet)]` on the enum declaration
// above. `label` delegates to the inherent `SexpShape::label` via
// `#[closed_set(via = "label")]` — the inherent name coincides with
// the trait method name here; the delegation stays explicit so the
// SAME wiring shape applies whether the inherent projection is `label`
// / `prefix` / `marker` / `keyword` / `as_str`. The `display` flag
// emits the substrate-wide `f.write_str(Self::label(*self))` block.
// `#[closed_set(generate_unknown = "sexp shape")]` emits the typed
// parse-rejection carrier with the substrate-wide `Debug + Clone +
// PartialEq + Eq + thiserror::Error` derives and the `#[error("unknown
// sexp shape: {0}")]` annotation byte-for-byte; the explicit label
// matches the auto-derived `pascal_to_spaced_lowercase("SexpShape")`
// projection byte-for-byte but pins the pre-lift wording. Round-trip
// + cross-axis rejection (`ExpectedKwargShape` labels `"number"` /
// `"list of strings"` whose vocabulary partially overlaps SexpShape on
// five of seven entries) pinned by
// `sexp_shape_label_round_trips_through_from_str` +
// `sexp_shape_from_str_accepts_only_canonical_labels`.

/// Typed witness of an offending `Sexp` at a typed-entry rejection
/// boundary — the joint identity (shape + literal) the substrate's
/// diagnostic surface owes the operator. Pairs the closed-set
/// `SexpShape` projection (the twelve reachable Sexp outermost shapes
/// the reader can produce) with the `Sexp::Display` projection (the
/// literal value the operator wrote: `5`, `:foo`, `(list 1 2)`,
/// `notify-ref`, etc.).
///
/// Mirror at the offending-value boundary of the prior-run
/// `SexpShape` (typed-shape closed set), `ExpectedKwargShape`
/// (expected-shape closed set), `KwargPath` (kwargs-path shapes),
/// `MacroDefHead` (macro-definition-head closed set), `UnquoteForm`
/// (template-marker syntactic forms), `CompilerSpecIoStage`
/// (disk-persistence surface), and `TemplateInvariantKind`
/// (bytecode-runtime surface) closed-set lifts: those enums key
/// rejection variants on a typed identity carried inside the
/// variant's data shape; `SexpWitness` keys the OFFENDING-VALUE side
/// (the `got: String` Sexp::Display slots on `NonSymbolUnquoteTarget`,
/// `SpliceOutsideList`, `NonSymbolParam`, `RestParamMissingName`,
/// `DefmacroNonSymbolName`, `DefmacroNonListParams`,
/// `MissingHeadSymbol`, and any future variant taking a `&Sexp` at
/// the helper boundary) on a typed joint identity so authoring tools
/// (REPL, LSP, `tatara-check`) bind to BOTH `witness.shape` (the
/// structural identity — pattern-matchable on `SexpShape::List` etc.)
/// AND `witness.display` (the literal value — renderable verbatim)
/// without losing either side.
///
/// Before this struct landed, the six error-builder helpers in
/// `macro_expand.rs` (`non_symbol_unquote_target`, `splice_outside_list`,
/// `non_symbol_param`, `rest_param_missing_name`,
/// `defmacro_non_symbol_name`, `defmacro_non_list_params`) and one
/// in `domain.rs` (`missing_head_err`'s caller) each projected `&Sexp
/// → String` via `Sexp::to_string()` at the boundary — the structural
/// `SexpShape` was lost. After this primitive lands, every offending-
/// value variant slot that takes a `SexpWitness` carries the typed
/// shape AND the literal jointly in ONE owned value the variant lives
/// independent of the call frame on.
///
/// The byte-for-byte rendering contract is preserved: `Display` for
/// `SexpWitness` writes only the `display` field, so a variant whose
/// `#[error(...)]` annotation projects through `{got}` renders
/// byte-identically to the legacy `got: String` shape — every
/// downstream substring-grep consumer (`tatara-check`, REPL) passes
/// unchanged. The gain is structural: tools that pattern-match on
/// `witness.shape == SexpShape::List` now bind to the typed identity
/// directly instead of substring-parsing the rendered literal.
///
/// `Clone + Debug + PartialEq + Eq` are retained (same posture as
/// every other owned-data `LispError` field); `Copy` is dropped
/// because the `display: String` is not `Copy`. When a future run
/// gives `Sexp` source spans, `pos: Option<usize>` lands here in ONE
/// place and every offending-value site picks up positional rendering
/// with no per-variant edit — the same future-proofing posture
/// `KwargPath`, `SexpShape`, and `ExpectedKwargShape` already carry.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the offending-
/// value's joint identity (structural shape + renderable literal)
/// becomes a TYPE rather than a `String` projection at the helper
/// boundary that discards the shape. After this primitive lands the
/// substrate's understanding of "the offending Sexp at a typed-entry
/// rejection" lives in ONE typed struct the diagnostic promotions
/// hang off of. THEORY.md §VI.1 — generation over composition; seven
/// inline `got.to_string()` projections at error-builder boundaries
/// (six in `macro_expand.rs`, one in `domain.rs::missing_head_err`'s
/// caller) is past the three-times-rule trigger. THEORY.md §II.1
/// invariant 1 — typed entry; the offending Sexp's identity is part
/// of the proof of WHAT the typed-entry gate rejected, and the typed
/// witness makes both halves of that identity (shape + literal)
/// load-bearing data on the variant rather than the literal-only
/// `String` projection the legacy shape carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SexpWitness {
    /// Structural identity — the typed-shape projection (`SexpShape::Int`,
    /// `SexpShape::List`, etc.). Pattern-matchable; future `pos: Option<usize>`
    /// promotions land alongside this field once `Sexp` carries spans.
    pub shape: SexpShape,
    /// Renderable identity — the `Sexp::Display` projection (`"5"`,
    /// `"(list 1 2)"`, `":foo"`, etc.). Owned so the witness lives
    /// independent of the call frame and crosses thread boundaries
    /// cleanly. Feeds the `#[error(...)]` annotation's `{got}` slot
    /// via `SexpWitness`'s `Display` impl.
    pub display: String,
}

impl SexpWitness {
    /// Owned constructor — pairs a typed `SexpShape` with an owned
    /// `String` projection of the offending `Sexp::Display`. Used by
    /// the `sexp_witness(&Sexp)` projection helper in `domain.rs`;
    /// hand-written `TataraDomain` impls that need to construct a
    /// witness at their own call boundary route through this
    /// constructor.
    #[must_use]
    pub fn new(shape: SexpShape, display: impl Into<String>) -> Self {
        Self {
            shape,
            display: display.into(),
        }
    }
}

impl std::fmt::Display for SexpWitness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Render the literal only — the byte-for-byte legacy rendering
        // of every `got: String` variant slot that projected through
        // `Sexp::Display`. Authoring tools that substring-grep on the
        // rendered diagnostic see no drift; tools that pattern-match
        // on the variant's `SexpWitness`-shaped `got` slot bind to
        // `witness.shape` directly.
        f.write_str(&self.display)
    }
}

fn unbound_hint_suffix(prefix: UnquoteForm, hint: Option<&str>) -> String {
    match hint {
        Some(h) => format!("; did you mean {}{h}?", prefix.marker()),
        None => String::new(),
    }
}

/// Renders the bare parenthetical suffix shared by EVERY `*_suffix`
/// diagnostic helper: a single leading space, an open paren, the
/// already-formatted `body`, and a close paren — ` ({body})`.
///
/// This is the lowest layer of the suffix-wrapping algebra. Four helpers
/// append a parenthetical structural-detail clause to a `#[error(...)]`
/// prefix and ALL FOUR share this exact skeleton — the leading space and the
/// parens:
///   * `unknown_among_suffix` wraps a `did you mean …?; …` / bare candidate
///     clause (the kwarg + registry gates);
///   * `rest_param_missing_name_suffix` wraps the `rest marker at position …`
///     clause;
///   * `missing_head_symbol_suffix` wraps the `got …` / `empty list` clause.
///
/// Owning the leading-space-and-parens HERE means it cannot drift across the
/// four renderers: a regression that drops the leading space at one site,
/// moves a paren, or doubles a space is structurally impossible because there
/// is exactly ONE wrapping implementation. Each helper keeps ONLY its own
/// body-construction and binds it to this primitive.
fn paren_suffix(body: &str) -> String {
    format!(" ({body})")
}

/// Renders the parenthetical "unknown X among a known set" suffix shared by
/// the kwarg gate (`UnknownKwarg`) and the registry gate
/// (`UnknownDomainKeyword`). `hint` is the already-formatted near-miss
/// suggestion (`:foo` for kwargs, `(foo ...)` for registry keywords); `body`
/// is the already-formatted candidate clause (`allowed: :a, :b` /
/// `registered: x, y` / `no domains registered`).
///
/// This layer owns ONLY the `did you mean {hint}?; ` join when a hint is
/// present, so the two gates whose docs declare they "share ONE structural
/// shape" cannot drift apart in that join. The bare-parenthetical wrapping —
/// the leading space and the parens — is delegated to `paren_suffix`, the one
/// skeleton every `*_suffix` helper binds to.
fn unknown_among_suffix(hint: Option<&str>, body: &str) -> String {
    match hint {
        Some(h) => paren_suffix(&format!("did you mean {h}?; {body}")),
        None => paren_suffix(body),
    }
}

fn unknown_kwarg_suffix(hint: Option<&str>, allowed: &[String]) -> String {
    let allowed_list = allowed
        .iter()
        .map(|s| format!(":{s}"))
        .collect::<Vec<_>>()
        .join(", ");
    unknown_among_suffix(
        hint.map(|h| format!(":{h}")).as_deref(),
        &format!("allowed: {allowed_list}"),
    )
}

fn rest_param_missing_name_suffix(rest_position: usize, got: Option<&str>) -> String {
    let body = match got {
        Some(g) => format!("rest marker at position {rest_position}, got {g}"),
        None => format!("rest marker at position {rest_position}, none provided"),
    };
    paren_suffix(&body)
}

fn rest_param_trailing_tokens_suffix(rest_position: usize, extra: usize, first: &str) -> String {
    paren_suffix(&format!(
        "rest marker at position {rest_position}, {extra} trailing after name, first: {first}"
    ))
}

fn optional_marker_repeated_suffix(first_position: usize, second_position: usize) -> String {
    paren_suffix(&format!(
        "first &optional at position {first_position}, second at position {second_position}"
    ))
}

fn optional_param_malformed_suffix(
    position: usize,
    reason: OptionalParamMalformedReason,
) -> String {
    paren_suffix(&format!("position {position}, {}", reason.label()))
}

fn missing_head_symbol_suffix(got: Option<&str>) -> String {
    let body = match got {
        Some(g) => format!("got {g}"),
        None => "empty list".to_string(),
    };
    paren_suffix(&body)
}

fn unknown_domain_keyword_suffix(hint: Option<&str>, registered: &[String]) -> String {
    let body = if registered.is_empty() {
        "no domains registered".to_string()
    } else {
        format!("registered: {}", registered.join(", "))
    };
    unknown_among_suffix(hint.map(|h| format!("({h} ...)")).as_deref(), &body)
}

impl LispError {
    /// Byte offset of the failure into the source, when locatable.
    ///
    /// Variants without a position (`Type`, `Compile`, etc.) return `None`,
    /// so callers can render a snippet only when the substrate has the
    /// information to do so. New positional variants gain editor-ready
    /// rendering (via `crate::diagnostic::format_diagnostic`) by adding a
    /// branch here — no consumer changes required.
    #[must_use]
    pub fn position(&self) -> Option<usize> {
        match self {
            Self::UnexpectedChar(_, pos) | Self::UnterminatedString(pos) => Some(*pos),
            Self::UnmatchedParen { pos } | Self::UnmatchedOpenParen { pos } | Self::Eof { pos } => {
                Some(*pos)
            }
            Self::InvalidNumber(_)
            | Self::UnknownSymbol(_)
            | Self::Type { .. }
            | Self::Compile { .. }
            | Self::TypeMismatch { .. }
            | Self::HeadMismatch { .. }
            | Self::Unknown { .. }
            | Self::Missing(_)
            | Self::OddKwargs { .. }
            | Self::UnboundTemplateVar { .. }
            | Self::DuplicateKwarg { .. }
            | Self::MissingKwarg { .. }
            | Self::UnknownKwarg { .. }
            | Self::UnknownDomainKeyword { .. }
            | Self::NonSymbolUnquoteTarget { .. }
            | Self::SpliceOutsideList { .. }
            | Self::MissingMacroArg { .. }
            | Self::TooManyMacroArgs { .. }
            | Self::NonSymbolParam { .. }
            | Self::RestParamMissingName { .. }
            | Self::RestParamTrailingTokens { .. }
            | Self::OptionalMarkerRepeated { .. }
            | Self::OptionalParamMalformed { .. }
            | Self::DefmacroArity { .. }
            | Self::DefmacroNonSymbolName { .. }
            | Self::DefmacroNonListParams { .. }
            | Self::NotAListForm { .. }
            | Self::MissingHeadSymbol { .. }
            | Self::NamedFormMissingName { .. }
            | Self::NamedFormNonSymbolName { .. }
            | Self::RewriterNonList { .. }
            | Self::DomainSerialize { .. }
            | Self::KwargDeserialize { .. }
            | Self::CompilerSpecIo { .. }
            | Self::TemplateInvariant { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        missing_head_symbol_suffix, optional_marker_repeated_suffix,
        optional_param_malformed_suffix, paren_suffix, rest_param_missing_name_suffix,
        rest_param_trailing_tokens_suffix, unknown_among_suffix, unknown_domain_keyword_suffix,
        unknown_kwarg_suffix, CompilerSpecIoStage, ExpectedKwargShape, KwargPath, KwargPathKind,
        LispError, MacroDefHead, OptionalParamMalformedReason, SexpShape, SexpWitness,
        UnknownExpectedKwargShape, UnknownKwargPathKind, UnknownMacroDefHead, UnknownSexpShape,
        UnknownUnquoteForm, UnquoteForm,
    };

    #[test]
    fn position_extracts_offset_from_positional_variants() {
        assert_eq!(LispError::UnexpectedChar('?', 7).position(), Some(7));
        assert_eq!(LispError::UnterminatedString(11).position(), Some(11));
        assert_eq!(LispError::UnmatchedParen { pos: 3 }.position(), Some(3));
        assert_eq!(LispError::UnmatchedOpenParen { pos: 0 }.position(), Some(0));
        assert_eq!(LispError::Eof { pos: 42 }.position(), Some(42));
    }

    #[test]
    fn position_is_none_for_non_positional_variants() {
        assert_eq!(
            LispError::OddKwargs {
                dangling: ":query".into()
            }
            .position(),
            None
        );
        assert_eq!(LispError::Missing("name").position(), None);
        assert_eq!(LispError::InvalidNumber("nan".into()).position(), None);
        assert_eq!(LispError::UnknownSymbol("foo".into()).position(), None);
        assert_eq!(
            LispError::Type {
                expected: "int",
                got: "string".into()
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::Compile {
                form: ":x".into(),
                message: "bad".into()
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::TypeMismatch {
                form: KwargPath::named("x"),
                expected: ExpectedKwargShape::String,
                got: SexpShape::Int,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::HeadMismatch {
                keyword: "defmonitor",
                got: "not-a-monitor".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::Unknown {
                category: "domain",
                value: "defx".into()
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::UnboundTemplateVar {
                prefix: UnquoteForm::Unquote,
                name: "xx".into(),
                hint: Some("x".into()),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::UnboundTemplateVar {
                prefix: UnquoteForm::Splice,
                name: "ys".into(),
                hint: None,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DuplicateKwarg { key: "name".into() }.position(),
            None
        );
        assert_eq!(
            LispError::MissingKwarg { key: "name".into() }.position(),
            None
        );
        assert_eq!(
            LispError::UnknownKwarg {
                key: "tthreshold".into(),
                hint: Some("threshold".into()),
                allowed: vec!["name".into(), "threshold".into()],
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::UnknownDomainKeyword {
                keyword: "defmoniter".into(),
                hint: Some("defmonitor".into()),
                registered: vec!["defalertpolicy".into(), "defmonitor".into()],
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NonSymbolUnquoteTarget {
                prefix: UnquoteForm::Unquote,
                got: SexpWitness::new(SexpShape::List, "(list 1 2)"),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NonSymbolUnquoteTarget {
                prefix: UnquoteForm::Splice,
                got: SexpWitness::new(SexpShape::Int, "5"),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::SpliceOutsideList {
                got: SexpWitness::new(SexpShape::Symbol, "xs"),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::SpliceOutsideList {
                got: SexpWitness::new(SexpShape::List, "(list 1 2)"),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::MissingMacroArg {
                macro_name: "wrap".into(),
                param: "b".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::MissingMacroArg {
                macro_name: "call".into(),
                param: "f".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::TooManyMacroArgs {
                macro_name: "pair".into(),
                expected: 2,
                got: 3,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::TooManyMacroArgs {
                macro_name: "wrap".into(),
                expected: 1,
                got: 5,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NonSymbolParam {
                position: 0,
                got: SexpWitness::new(SexpShape::Int, "5"),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NonSymbolParam {
                position: 2,
                got: SexpWitness::new(SexpShape::List, "(nested)"),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::RestParamMissingName {
                rest_position: 1,
                got: None,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::RestParamMissingName {
                rest_position: 0,
                got: Some(SexpWitness::new(SexpShape::Int, "5")),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::OptionalParamMalformed {
                position: 1,
                got: SexpWitness::new(SexpShape::List, "()"),
                reason: OptionalParamMalformedReason::EmptyList,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::OptionalParamMalformed {
                position: 3,
                got: SexpWitness::new(SexpShape::List, "(x 1 2)"),
                reason: OptionalParamMalformedReason::ExtraElements { length: 3 },
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DefmacroArity {
                head: MacroDefHead::Defmacro,
                arity: 1,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DefmacroArity {
                head: MacroDefHead::Defcheck,
                arity: 3,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DefmacroNonSymbolName {
                head: MacroDefHead::Defmacro,
                got: SexpWitness::new(SexpShape::Int, "5"),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DefmacroNonListParams {
                head: MacroDefHead::Defmacro,
                got: SexpWitness::new(SexpShape::Symbol, "x"),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DefmacroNonListParams {
                head: MacroDefHead::Defcheck,
                got: SexpWitness::new(SexpShape::Keyword, ":foo"),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DefmacroNonSymbolName {
                head: MacroDefHead::DefpointTemplate,
                got: SexpWitness::new(SexpShape::Keyword, ":foo"),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NotAListForm {
                keyword: "defmonitor",
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NotAListForm {
                keyword: "defpoint",
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::MissingHeadSymbol {
                keyword: "defmonitor",
                got: None,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::MissingHeadSymbol {
                keyword: "defpoint",
                got: Some(SexpWitness::new(SexpShape::Int, "5")),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NamedFormMissingName {
                keyword: "defpoint",
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NamedFormMissingName {
                keyword: "defalertpolicy",
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NamedFormNonSymbolName {
                keyword: "defpoint",
                got: SexpShape::Int,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NamedFormNonSymbolName {
                keyword: "defalertpolicy",
                got: SexpShape::List,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::RewriterNonList {
                keyword: "defmonitor",
                got: SexpWitness::new(SexpShape::Int, "42"),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DomainSerialize {
                keyword: "defmonitor",
                message: "expected value at line 1 column 1".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DomainSerialize {
                keyword: "defalertpolicy",
                message: "key must be a string".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::KwargDeserialize {
                path: KwargPath::named("level"),
                message: "unknown variant `NotASeverity`".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::KwargDeserialize {
                path: KwargPath::item("steps", 1),
                message: "invalid type: integer `7`, expected a string".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::CompilerSpecIo {
                stage: super::CompilerSpecIoStage::RealizeToDiskSerialize,
                message: "expected struct CompilerSpec".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::CompilerSpecIo {
                stage: super::CompilerSpecIoStage::LoadFromDiskDeserialize,
                message: "expected value at line 1 column 1".into(),
            }
            .position(),
            None
        );
    }

    #[test]
    fn missing_head_symbol_display_with_empty_list_renders_legacy_prefix_and_empty_marker() {
        // `()` — list[0] doesn't exist. The variant renders the
        // legacy prefix `compile error in {keyword}: missing head
        // symbol` byte-for-byte AND names the structural reason
        // `(empty list)` parenthetically — same posture as how
        // `RestParamMissingName` renders `(rest marker at position
        // N, none provided)` and how `UnknownDomainKeyword` renders
        // `(no domains registered)` for the empty-side case. A
        // regression that drops either fragment fails-loudly here.
        let err = LispError::MissingHeadSymbol {
            keyword: "defmonitor",
            got: None,
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: missing head symbol (empty list)"
        );
    }

    #[test]
    fn missing_head_symbol_display_with_int_got_renders_legacy_prefix_and_got() {
        // `(5 …)` — list[0] is the int `5`, not a symbol. The variant
        // renders both the keyword AND the offending head's
        // `Sexp::Display` projection — both fields are first-class
        // structural data, not embedded substrings of `message`. The
        // prefix `compile error in {keyword}: missing head symbol`
        // matches the legacy `Compile { form: keyword.to_string(),
        // message: "missing head symbol" }` byte-for-byte; the
        // structural detail (`(got 5)`) is appended.
        let err = LispError::MissingHeadSymbol {
            keyword: "defmonitor",
            got: Some(SexpWitness::new(SexpShape::Int, "5")),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: missing head symbol (got 5)"
        );
    }

    #[test]
    fn missing_head_symbol_display_carries_keyword_atom_got_unchanged() {
        // `(:foo …)` — list[0] is a keyword atom. `Sexp::Display` for
        // `Atom::Keyword(s)` writes `:s`; pin that the variant's
        // Display passes the keyword form through unchanged so an
        // LSP that surfaces "you wrote `:foo` where a head symbol
        // was expected" gains the literal keyword value as data, no
        // re-parsing required.
        let err = LispError::MissingHeadSymbol {
            keyword: "defalertpolicy",
            got: Some(SexpWitness::new(SexpShape::Keyword, ":foo")),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defalertpolicy: missing head symbol (got :foo)"
        );
    }

    #[test]
    fn missing_head_symbol_display_carries_string_got_unchanged() {
        // `Sexp::Display` for `Atom::Str(s)` writes `"s"` (with quotes);
        // pin that the variant's Display passes the string form through
        // unchanged so an LSP that surfaces "you wrote `\"name\"` where
        // a head symbol was expected" gains the literal value as data,
        // no re-parsing required.
        let err = LispError::MissingHeadSymbol {
            keyword: "defmonitor",
            got: Some(SexpWitness::new(SexpShape::String, "\"name\"")),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: missing head symbol (got \"name\")"
        );
    }

    #[test]
    fn missing_head_symbol_display_carries_nested_list_got_unchanged() {
        // `((nested) …)` — list[0] is itself a list. The nested form
        // round-trips through `Sexp::Display` into the variant's `got`
        // slot unchanged so the operator sees what they wrote.
        let err = LispError::MissingHeadSymbol {
            keyword: "defmonitor",
            got: Some(SexpWitness::new(SexpShape::List, "(nested)")),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: missing head symbol (got (nested))"
        );
    }

    #[test]
    fn missing_head_symbol_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring — `"missing head symbol"` — as a
        // separate assertion so a regression that drifts the wording
        // (e.g., to "no head symbol" or "head must be a symbol")
        // fails-loudly here even if the appended parenthetical changes
        // shape. The substring is what consumers downstream
        // (`tatara-check`, the REPL) substring-match on today; the
        // prefix matches the legacy `Compile { form:
        // keyword.to_string(), message: "missing head symbol" }`
        // byte-for-byte.
        let err = LispError::MissingHeadSymbol {
            keyword: "defmonitor",
            got: None,
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("missing head symbol"),
            "expected legacy substring in message, got: {msg}"
        );
        assert!(
            msg.contains("compile error in defmonitor:"),
            "expected legacy form-label prefix in message, got: {msg}"
        );
    }

    #[test]
    fn missing_head_symbol_got_carries_typed_witness_through_variant_slot() {
        // Pin the structural binding AND the Display projection on
        // `LispError::MissingHeadSymbol.got`. After this lift the
        // variant's typed slot is `Option<SexpWitness>` — the joint
        // `SexpWitness` identity (the same primitive `SpliceOutsideList.got`,
        // `NonSymbolUnquoteTarget.got`, `NonSymbolParam.got`,
        // `DefmacroNonSymbolName.got`, `DefmacroNonListParams.got`,
        // and `RestParamMissingName.got` already carry) wrapped in
        // `Option` because the head slot bifurcates structurally
        // between "missing entirely" (`None`, empty list) and
        // "present but malformed" (`Some(SexpWitness)`, non-symbol
        // head). The typed witness lands on the `Some` arm only. A
        // regression that re-collapses to a free-form `Option<String>`
        // got slot (losing the rustc-enforced closed-set guarantee
        // on shape identity at the outer typed-entry gate's
        // non-symbol-head rejection variant) fails-loudly here.
        // Display via `SexpWitness::Display` writes only the `display`
        // field so the rendered `(got <display>)` clause is
        // byte-for-byte identical to the pre-lift `Option<String>`
        // shape; downstream substring-grep consumers (`tatara-check`,
        // REPL) see no drift.
        let err = LispError::MissingHeadSymbol {
            keyword: "defmonitor",
            got: Some(SexpWitness::new(SexpShape::Int, "5")),
        };
        match &err {
            LispError::MissingHeadSymbol { keyword, got } => {
                assert_eq!(*keyword, "defmonitor");
                let witness = got.as_ref().expect("got must be Some");
                assert_eq!(witness.shape, SexpShape::Int);
                assert_eq!(witness.display, "5");
            }
            other => panic!("expected MissingHeadSymbol, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: missing head symbol (got 5)"
        );
    }

    #[test]
    fn missing_head_symbol_got_distinguishes_int_from_keyword_at_variant_slot() {
        // Pin the typed-shape bifurcation at the variant slot — `5`
        // (int atom at the head slot) and `:foo` (keyword atom at
        // the head slot) BOTH route to `MissingHeadSymbol` on the
        // `Some` arm, but the typed `got.shape` slot distinguishes
        // them structurally — `SexpShape::Int` vs.
        // `SexpShape::Keyword`. Sibling pin for the same structural-
        // shape-bifurcation property
        // `rest_param_missing_name_got_distinguishes_int_from_keyword_at_variant_slot`
        // pins on `RestParamMissingName`. A regression that erases
        // the typed shape (e.g., reverts to `got: Option<String>`)
        // would lose this distinction — tooling that wants to surface
        // "you wrote an int `5` where a head symbol was expected" vs.
        // "you wrote a keyword `:foo` where a head symbol was
        // expected (did you mean `foo`?)" would have to substring-
        // grep the `display` field, brittle.
        let err_int = LispError::MissingHeadSymbol {
            keyword: "defmonitor",
            got: Some(SexpWitness::new(SexpShape::Int, "5")),
        };
        let err_kw = LispError::MissingHeadSymbol {
            keyword: "defalertpolicy",
            got: Some(SexpWitness::new(SexpShape::Keyword, ":foo")),
        };
        let (int_shape, kw_shape) = (
            match &err_int {
                LispError::MissingHeadSymbol { got: Some(w), .. } => w.shape,
                _ => unreachable!(),
            },
            match &err_kw {
                LispError::MissingHeadSymbol { got: Some(w), .. } => w.shape,
                _ => unreachable!(),
            },
        );
        assert_ne!(
            int_shape, kw_shape,
            "Int and Keyword witnesses must remain structurally distinct at the variant slot",
        );
        assert_eq!(int_shape, SexpShape::Int);
        assert_eq!(kw_shape, SexpShape::Keyword);
    }

    #[test]
    fn missing_head_symbol_and_rest_param_gate_share_one_witness_primitive() {
        // Pin that ALL SEVEN Sexp-display-source `got` slots in the
        // substrate (`SpliceOutsideList`, `NonSymbolUnquoteTarget`,
        // `NonSymbolParam`, `DefmacroNonSymbolName`,
        // `DefmacroNonListParams`, `RestParamMissingName`,
        // `MissingHeadSymbol`) carry the SAME typed `SexpWitness`
        // primitive — the closed set of "offending inner Sexp"
        // identities is bound by ONE typed primitive across SEVEN
        // rejection surfaces: the template-gate's `,X/,@X` pair, the
        // defmacro-syntax-gate's `parse_params` walker (BOTH
        // non-symbol-param AND post-`&rest`-non-symbol-follower
        // rejection points), BOTH of the defmacro-syntax-gate's outer
        // `macro_def_from` rejection points (name-symbol AND
        // param-list), AND the outer `compile_from_sexp` typed-entry
        // gate's non-symbol-head rejection point. With this lift
        // EVERY `Sexp::Display`-source `got` slot in the substrate is
        // structurally unified end-to-end. The `Option`-wrap on
        // `MissingHeadSymbol.got` and `RestParamMissingName.got` is
        // the bifurcation between "missing entirely" and "present but
        // malformed"; the typed witness rides on the `Some` arm and
        // is structurally identical to the other five variants' got
        // slots. A regression that diverges the slot type on any one
        // variant (e.g., re-collapses `MissingHeadSymbol.got` to
        // `Option<String>` while leaving the others typed) fails-
        // loudly here because the assignment round-trips the witness
        // across all seven slot types. Sibling pin to
        // `rest_param_missing_name_and_macro_def_gate_share_one_witness_primitive`
        // — extending the typed-identity unification contract from
        // six slots to seven, closing the contract.
        let same_witness = SexpWitness::new(SexpShape::List, "(nested)");
        let missing_head = LispError::MissingHeadSymbol {
            keyword: "defmonitor",
            got: Some(same_witness.clone()),
        };
        let rest_param_missing_name = LispError::RestParamMissingName {
            rest_position: 0,
            got: Some(same_witness.clone()),
        };
        let defmacro_non_list_params = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defmacro,
            got: same_witness.clone(),
        };
        let defmacro_non_symbol_name = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::Defmacro,
            got: same_witness.clone(),
        };
        let non_symbol_param = LispError::NonSymbolParam {
            position: 0,
            got: same_witness.clone(),
        };
        let non_symbol_target = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Unquote,
            got: same_witness.clone(),
        };
        let splice_outside = LispError::SpliceOutsideList {
            got: same_witness.clone(),
        };
        match (
            &missing_head,
            &rest_param_missing_name,
            &defmacro_non_list_params,
            &defmacro_non_symbol_name,
            &non_symbol_param,
            &non_symbol_target,
            &splice_outside,
        ) {
            (
                LispError::MissingHeadSymbol { got: Some(a), .. },
                LispError::RestParamMissingName { got: Some(b), .. },
                LispError::DefmacroNonListParams { got: c, .. },
                LispError::DefmacroNonSymbolName { got: d, .. },
                LispError::NonSymbolParam { got: e, .. },
                LispError::NonSymbolUnquoteTarget { got: f, .. },
                LispError::SpliceOutsideList { got: g },
            ) => {
                assert_eq!(a.shape, b.shape);
                assert_eq!(b.shape, c.shape);
                assert_eq!(c.shape, d.shape);
                assert_eq!(d.shape, e.shape);
                assert_eq!(e.shape, f.shape);
                assert_eq!(f.shape, g.shape);
                assert_eq!(a.display, b.display);
                assert_eq!(b.display, c.display);
                assert_eq!(c.display, d.display);
                assert_eq!(d.display, e.display);
                assert_eq!(e.display, f.display);
                assert_eq!(f.display, g.display);
                assert_eq!(*a, same_witness);
                assert_eq!(*b, same_witness);
                assert_eq!(*c, same_witness);
                assert_eq!(*d, same_witness);
                assert_eq!(*e, same_witness);
                assert_eq!(*f, same_witness);
                assert_eq!(*g, same_witness);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn not_a_list_form_display_renders_legacy_compile_shape() {
        // Pin the rendered diagnostic byte-for-byte against the
        // legacy `Compile { form: keyword.to_string(), message:
        // "expected list form" }` shape. Authoring tools that
        // substring-grep on the rendered message see no drift; tools
        // that pattern-match on the variant gain structural binding.
        let err = LispError::NotAListForm {
            keyword: "defmonitor",
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: expected list form"
        );
    }

    #[test]
    fn not_a_list_form_display_carries_keyword_unchanged() {
        // Pin path-uniformity across distinct keywords — every
        // `TataraDomain` impl funnels through `not_a_list_form_err`
        // with its own `Self::KEYWORD`, so the variant's `keyword`
        // slot must round-trip every literal the derive macro
        // accepts. A regression that drops or rewrites the keyword
        // (e.g., lowercasing, stripping the `def` prefix) fails-
        // loudly here.
        let err = LispError::NotAListForm {
            keyword: "defpoint",
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defpoint: expected list form"
        );
    }

    #[test]
    fn not_a_list_form_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring — `"expected list form"` — as a
        // separate assertion so a regression that drifts the
        // wording (e.g., to "expected list" or "must be a list")
        // fails-loudly here even if the prefix changes shape.
        // The substring is what consumers downstream
        // (`tatara-check`, the REPL) substring-match on today; the
        // prefix matches the legacy `Compile { form:
        // keyword.to_string(), message: "expected list form" }`
        // byte-for-byte.
        let err = LispError::NotAListForm {
            keyword: "defalertpolicy",
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("expected list form"),
            "expected legacy substring in message, got: {msg}"
        );
        assert!(
            msg.contains("compile error in defalertpolicy:"),
            "expected legacy form-label prefix in message, got: {msg}"
        );
    }

    #[test]
    fn unknown_kwarg_display_with_hint_renders_did_you_mean_then_allowed_list() {
        // The variant renders byte-for-byte the same string the legacy
        // `Compile { form: ":tthreshold", message: "unknown keyword (did
        // you mean :threshold?; allowed: :a, :b, :c)" }` shape produced,
        // so authoring tools (REPL, LSP, `tatara-check`) that
        // substring-match on the rendered diagnostic see no drift; tools
        // that pattern-match on the variant gain structural binding.
        let err = LispError::UnknownKwarg {
            key: "tthreshold".into(),
            hint: Some("threshold".into()),
            allowed: vec!["name".into(), "query".into(), "threshold".into()],
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :tthreshold: unknown keyword \
             (did you mean :threshold?; allowed: :name, :query, :threshold)"
        );
    }

    #[test]
    fn unknown_kwarg_display_without_hint_renders_allowed_list_only() {
        // No hint: the rendered message has the allowed list but no `did
        // you mean` clause. A wrong hint is worse than no hint — the
        // slot stays empty unless `suggest` ranks a candidate within
        // the bounded edit distance.
        let err = LispError::UnknownKwarg {
            key: "totally-unrelated".into(),
            hint: None,
            allowed: vec!["name".into(), "query".into(), "threshold".into()],
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :totally-unrelated: unknown keyword \
             (allowed: :name, :query, :threshold)"
        );
    }

    #[test]
    fn unknown_kwarg_display_carries_kebab_case_keys_unchanged() {
        // `:notify-ref`, `:window-seconds`, every kebab-cased kwarg name
        // round-trips through both the offending-key slot AND the
        // allowed-list slot unchanged. Pinning this contract means a
        // regression that camelCases or lowercases either side fails-
        // loudly here.
        let err = LispError::UnknownKwarg {
            key: "windou-seconds".into(),
            hint: Some("window-seconds".into()),
            allowed: vec!["notify-ref".into(), "window-seconds".into()],
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :windou-seconds: unknown keyword \
             (did you mean :window-seconds?; allowed: :notify-ref, :window-seconds)"
        );
    }

    #[test]
    fn duplicate_kwarg_display_matches_legacy_compile_shape() {
        // The variant renders byte-for-byte the same string the legacy
        // `Compile { form: ":name", message: "duplicate keyword" }` shape
        // produced, so authoring tools (REPL, LSP, `tatara-check`) that
        // substring-match on the rendered diagnostic see no drift; tools
        // that pattern-match on the variant gain structural binding.
        let err = LispError::DuplicateKwarg { key: "name".into() };
        assert_eq!(
            format!("{err}"),
            "compile error in :name: duplicate keyword"
        );
    }

    #[test]
    fn duplicate_kwarg_display_carries_kebab_case_keys_unchanged() {
        // `:notify-ref`, `:window-seconds`, every kebab-cased kwarg name
        // round-trips through the variant's Display unchanged. Pinning
        // this contract means a regression that camelCases or lowercases
        // the key in the rendered message fails-loudly.
        let err = LispError::DuplicateKwarg {
            key: "notify-ref".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :notify-ref: duplicate keyword"
        );
    }

    #[test]
    fn missing_kwarg_display_matches_legacy_compile_shape() {
        // The variant renders byte-for-byte the same string the legacy
        // `Compile { form: ":threshold", message: "required but not provided" }`
        // shape produced, so authoring tools (REPL, LSP, `tatara-check`) that
        // substring-match on the rendered diagnostic see no drift; tools
        // that pattern-match on the variant gain structural binding.
        let err = LispError::MissingKwarg {
            key: "threshold".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: required but not provided"
        );
    }

    #[test]
    fn missing_kwarg_display_carries_kebab_case_keys_unchanged() {
        // `:notify-ref`, `:window-seconds`, every kebab-cased kwarg name
        // round-trips through the variant's Display unchanged. Pinning
        // this contract means a regression that camelCases or lowercases
        // the key in the rendered message fails-loudly.
        let err = LispError::MissingKwarg {
            key: "notify-ref".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :notify-ref: required but not provided"
        );
    }

    #[test]
    fn unbound_template_var_display_without_hint_matches_legacy_compile_shape() {
        // Without a hint the variant renders byte-for-byte the same string
        // the legacy `Compile { form: ",x", message: "unbound" }` shape
        // produced, so authoring tools that substring-match on the rendered
        // diagnostic see no drift.
        let err = LispError::UnboundTemplateVar {
            prefix: UnquoteForm::Unquote,
            name: "y".into(),
            hint: None,
        };
        assert_eq!(format!("{err}"), "compile error in ,y: unbound");
    }

    #[test]
    fn unbound_template_var_display_appends_hint_suffix_when_present() {
        // With a hint the message gains a `"; did you mean ,X?"` suffix —
        // the prefix is preserved in the hint so the operator can copy-paste
        // the suggestion verbatim.
        let err = LispError::UnboundTemplateVar {
            prefix: UnquoteForm::Unquote,
            name: "xs".into(),
            hint: Some("x".into()),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,xs: unbound; did you mean ,x?"
        );
    }

    #[test]
    fn unknown_domain_keyword_display_with_hint_renders_did_you_mean_then_registered_list() {
        // The variant names the offending keyword's full call shape
        // (`(defmoniter ...)`), the structural near-miss in the same call
        // shape (`(defmonitor ...)`), and the sorted registered set.
        // Authoring tools that pattern-match on the variant gain
        // structural binding to `keyword` / `hint` / `registered` —
        // tools that substring-match on the rendered diagnostic see a
        // stable shape.
        let err = LispError::UnknownDomainKeyword {
            keyword: "defmoniter".into(),
            hint: Some("defmonitor".into()),
            registered: vec![
                "defalertpolicy".into(),
                "defmonitor".into(),
                "defnotify".into(),
            ],
        };
        assert_eq!(
            format!("{err}"),
            "unknown domain keyword: (defmoniter ...) \
             (did you mean (defmonitor ...)?; \
             registered: defalertpolicy, defmonitor, defnotify)"
        );
    }

    #[test]
    fn unknown_domain_keyword_display_without_hint_renders_registered_list_only() {
        // No near-miss within the bounded edit distance: the rendered
        // message has the registered list but no `did you mean` clause.
        // A wrong hint is worse than no hint — the slot stays empty
        // unless `suggest_keyword` ranks a candidate within the bound.
        let err = LispError::UnknownDomainKeyword {
            keyword: "totally-unrelated".into(),
            hint: None,
            registered: vec!["defalertpolicy".into(), "defmonitor".into()],
        };
        assert_eq!(
            format!("{err}"),
            "unknown domain keyword: (totally-unrelated ...) \
             (registered: defalertpolicy, defmonitor)"
        );
    }

    #[test]
    fn unknown_domain_keyword_display_with_empty_registry_renders_no_domains_registered() {
        // The substrate names the structural reason — the registry has
        // no candidates at all — instead of a misleading empty
        // `registered: ` suffix. A typo against an empty registry is
        // unambiguously a registry-seeding bug, not a near-miss.
        let err = LispError::UnknownDomainKeyword {
            keyword: "defmonitor".into(),
            hint: None,
            registered: vec![],
        };
        assert_eq!(
            format!("{err}"),
            "unknown domain keyword: (defmonitor ...) (no domains registered)"
        );
    }

    #[test]
    fn unknown_domain_keyword_display_carries_kebab_case_keywords_unchanged() {
        // Kebab-cased domain keywords (`defalert-policy`,
        // `defprocess-spec`) round-trip through the offending-keyword
        // slot AND the registered-list slot unchanged. Pinning this
        // contract means a regression that camelCases or lowercases
        // either side fails-loudly here.
        let err = LispError::UnknownDomainKeyword {
            keyword: "defalert-policiy".into(),
            hint: Some("defalert-policy".into()),
            registered: vec!["defalert-policy".into(), "defprocess-spec".into()],
        };
        assert_eq!(
            format!("{err}"),
            "unknown domain keyword: (defalert-policiy ...) \
             (did you mean (defalert-policy ...)?; \
             registered: defalert-policy, defprocess-spec)"
        );
    }

    #[test]
    fn unknown_among_suffix_owns_the_parenthetical_wrapping_skeleton() {
        // The Some-arm owns the `did you mean {hint}?; {body}` join; both arms
        // delegate the bare leading-space-and-parens to `paren_suffix`. Both
        // gates whose docs declare they "share ONE structural shape" wrap
        // through this skeleton, so a regression that drifts the join fails
        // here; one that drifts the bare wrapping fails in
        // `every_suffix_helper_wraps_through_one_paren_primitive`.
        assert_eq!(
            unknown_among_suffix(Some(":threshold"), "allowed: :name, :threshold"),
            " (did you mean :threshold?; allowed: :name, :threshold)"
        );
        assert_eq!(
            unknown_among_suffix(None, "allowed: :name, :threshold"),
            " (allowed: :name, :threshold)"
        );
    }

    #[test]
    fn optional_param_malformed_display_renders_typed_reason_in_suffix() {
        // The variant's Display threads BOTH the offending list literal
        // (`got` via SexpWitness's Display projection) AND the typed
        // `reason`'s label into the rendered message — the prefix names
        // the malformed-spec failure mode, the parenthetical suffix names
        // position + reason. Authoring tools (REPL, LSP, `tatara-check`)
        // pattern-match on the variant for structural binding; tools that
        // substring-match see a stable shape parallel to the existing
        // `OptionalMarkerRepeated` Display.
        let empty = LispError::OptionalParamMalformed {
            position: 1,
            got: SexpWitness::new(SexpShape::List, "()"),
            reason: OptionalParamMalformedReason::EmptyList,
        };
        assert_eq!(
            format!("{empty}"),
            "compile error in defmacro params: malformed &optional spec, got () (position 1, empty list)"
        );
        let missing = LispError::OptionalParamMalformed {
            position: 2,
            got: SexpWitness::new(SexpShape::List, "(x)"),
            reason: OptionalParamMalformedReason::MissingDefault,
        };
        assert_eq!(
            format!("{missing}"),
            "compile error in defmacro params: malformed &optional spec, got (x) (position 2, missing default)"
        );
        let extra = LispError::OptionalParamMalformed {
            position: 3,
            got: SexpWitness::new(SexpShape::List, "(x 1 2)"),
            reason: OptionalParamMalformedReason::ExtraElements { length: 3 },
        };
        assert_eq!(
            format!("{extra}"),
            "compile error in defmacro params: malformed &optional spec, got (x 1 2) (position 3, 3 elements (need 2))"
        );
        let nonsym = LispError::OptionalParamMalformed {
            position: 0,
            got: SexpWitness::new(SexpShape::List, "(5 default)"),
            reason: OptionalParamMalformedReason::NonSymbolName,
        };
        assert_eq!(
            format!("{nonsym}"),
            "compile error in defmacro params: malformed &optional spec, got (5 default) (position 0, name not a symbol)"
        );
    }

    #[test]
    fn paren_suffix_owns_the_bare_parenthetical_skeleton() {
        // The lowest layer of the suffix-wrapping algebra: one leading space,
        // open paren, body, close paren. A regression that drops the leading
        // space, doubles it, or moves a paren fails here.
        assert_eq!(paren_suffix("got 5"), " (got 5)");
        assert_eq!(paren_suffix(""), " ()");
    }

    #[test]
    fn every_suffix_helper_wraps_through_one_paren_primitive() {
        // All seven `*_suffix` helpers delegate their bare ` (…)` wrapping to
        // `paren_suffix`; only their body-construction stays helper-specific.
        // Pinning that each helper's output EQUALS `paren_suffix` applied with
        // that helper's body means a re-inlined divergent skeleton in ANY of
        // them (e.g. a dropped leading space, a moved paren) fails-loudly.
        // Covers both arms of the multi-arm helpers.

        // unknown_among_suffix — the `did you mean …?; …` join layer.
        assert_eq!(
            unknown_among_suffix(Some(":t"), "allowed: :name"),
            paren_suffix("did you mean :t?; allowed: :name")
        );
        assert_eq!(
            unknown_among_suffix(None, "allowed: :name"),
            paren_suffix("allowed: :name")
        );

        // rest_param_missing_name_suffix — the `rest marker at position …` body.
        assert_eq!(
            rest_param_missing_name_suffix(1, Some("5")),
            paren_suffix("rest marker at position 1, got 5")
        );
        assert_eq!(
            rest_param_missing_name_suffix(1, None),
            paren_suffix("rest marker at position 1, none provided")
        );

        // rest_param_trailing_tokens_suffix — the `… trailing after name` body.
        assert_eq!(
            rest_param_trailing_tokens_suffix(1, 2, "extra"),
            paren_suffix("rest marker at position 1, 2 trailing after name, first: extra")
        );

        // missing_head_symbol_suffix — the `got …` / `empty list` body.
        assert_eq!(missing_head_symbol_suffix(Some("5")), paren_suffix("got 5"));
        assert_eq!(missing_head_symbol_suffix(None), paren_suffix("empty list"));

        // optional_marker_repeated_suffix — the two-marker-position body.
        assert_eq!(
            optional_marker_repeated_suffix(1, 3),
            paren_suffix("first &optional at position 1, second at position 3")
        );

        // optional_param_malformed_suffix — the position+reason-label body.
        // Both `&'static` (the three string-only arms) and formatted
        // (the `ExtraElements{length}` arm) cases route through the same
        // `paren_suffix` wrapping.
        assert_eq!(
            optional_param_malformed_suffix(1, OptionalParamMalformedReason::EmptyList),
            paren_suffix("position 1, empty list")
        );
        assert_eq!(
            optional_param_malformed_suffix(
                2,
                OptionalParamMalformedReason::ExtraElements { length: 3 }
            ),
            paren_suffix("position 2, 3 elements (need 2)")
        );
    }

    #[test]
    fn unknown_kwarg_and_domain_suffixes_share_one_wrapping_primitive() {
        // Both gates delegate their parenthetical wrapping to
        // `unknown_among_suffix`; only the hint-formatting (`:foo` vs
        // `(foo ...)`) and the body-construction (`allowed:` vs `registered:`
        // / `no domains registered`) stay gate-specific. Pinning that each
        // gate's output EQUALS the primitive applied with that gate's
        // formatted hint + body means a re-inlined divergent skeleton in
        // either gate fails-loudly. Covers all four arms: kwarg-with-hint,
        // kwarg-without-hint, domain-with-hint, domain-empty-registry.
        let allowed = vec!["name".to_string(), "threshold".to_string()];
        assert_eq!(
            unknown_kwarg_suffix(Some("threshold"), &allowed),
            unknown_among_suffix(Some(":threshold"), "allowed: :name, :threshold")
        );
        assert_eq!(
            unknown_kwarg_suffix(None, &allowed),
            unknown_among_suffix(None, "allowed: :name, :threshold")
        );

        let registered = vec!["defmonitor".to_string(), "defnotify".to_string()];
        assert_eq!(
            unknown_domain_keyword_suffix(Some("defmonitor"), &registered),
            unknown_among_suffix(
                Some("(defmonitor ...)"),
                "registered: defmonitor, defnotify"
            )
        );
        assert_eq!(
            unknown_domain_keyword_suffix(None, &[]),
            unknown_among_suffix(None, "no domains registered")
        );
    }

    #[test]
    fn non_symbol_unquote_target_display_renders_canonical_type_mismatch_shape() {
        // `,(list 1 2)` — the inner is a list, not a symbol. The variant
        // names the syntactic marker (`,`), the expected shape (`symbol` —
        // the only form a no-evaluator template can substitute), and the
        // offending literal (`(list 1 2)`) as first-class fields. Authoring
        // tools that pattern-match on the variant gain structural binding;
        // tools that substring-match on the rendered diagnostic see a
        // stable shape parallel to the existing `TypeMismatch` variant.
        let err = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Unquote,
            got: SexpWitness::new(SexpShape::List, "(list 1 2)"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,: expected symbol, got (list 1 2)"
        );
    }

    #[test]
    fn non_symbol_unquote_target_display_preserves_splice_prefix() {
        // Splice marker rides through the `prefix` field; the rendered
        // diagnostic is `,@`, not `,`. The operator never has to translate
        // `,` ↔ `,@` mentally — same posture as `UnboundTemplateVar`.
        let err = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Splice,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@: expected symbol, got 5"
        );
    }

    #[test]
    fn non_symbol_unquote_target_display_carries_keyword_atom_unchanged() {
        // `,:foo` — the inner is a keyword atom. The `:foo` form
        // round-trips through `SexpWitness::Display` (writing the
        // `display` field verbatim) into the variant's `got` slot
        // unchanged, so the operator sees what they wrote. The typed
        // `got.shape` slot independently carries `SexpShape::Keyword`
        // so tooling that wants the structural identity binds without
        // re-parsing.
        let err = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Unquote,
            got: SexpWitness::new(SexpShape::Keyword, ":foo"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,: expected symbol, got :foo"
        );
    }

    #[test]
    fn splice_outside_list_display_renders_legacy_substring_with_offending_form() {
        // `,@xs` at the body's top level — there is no containing list to
        // splice into. The variant names the offending inner (`xs`) as a
        // first-class typed witness so authoring tools (REPL, LSP,
        // `tatara-check`) gain structural binding to BOTH `got.shape`
        // (typed `SexpShape::Symbol` here) AND `got.display` (the literal
        // `"xs"`); tools that substring-match on the rendered diagnostic
        // still see the legacy `"\`,@\` may only appear inside a list"`
        // substring verbatim because `SexpWitness::Display` writes only
        // the `display` field.
        let err = LispError::SpliceOutsideList {
            got: SexpWitness::new(SexpShape::Symbol, "xs"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@: `,@` may only appear inside a list (got ,@xs)"
        );
    }

    #[test]
    fn splice_outside_list_display_carries_list_literal_unchanged() {
        // The offending inner is a list literal — `,@(list 1 2)` — so the
        // operator sees the literal value they wrote in the parenthetical,
        // not just a type-name. The typed `SexpShape::List` rides the
        // variant slot alongside the `Sexp::Display` projection; tools
        // can now pattern-match on `got.shape == SexpShape::List`
        // without re-parsing the rendered diagnostic.
        let err = LispError::SpliceOutsideList {
            got: SexpWitness::new(SexpShape::List, "(list 1 2)"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@: `,@` may only appear inside a list (got ,@(list 1 2))"
        );
    }

    #[test]
    fn splice_outside_list_display_carries_kebab_case_symbol_unchanged() {
        // `,@notify-ref` — kebab-cased symbol round-trips through the
        // variant's `got.display` slot unchanged. Pinning this contract
        // means a regression that camelCases or lowercases the offending
        // form fails-loudly here.
        let err = LispError::SpliceOutsideList {
            got: SexpWitness::new(SexpShape::Symbol, "notify-ref"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@: `,@` may only appear inside a list (got ,@notify-ref)"
        );
    }

    #[test]
    fn splice_outside_list_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring as a separate assertion so a regression
        // that drifts the wording (e.g., to "outside a list" or "without a
        // containing list") fails-loudly here even if the parenthetical
        // changes shape. The substring is what consumers downstream
        // (tatara-check, the REPL) substring-match on today.
        let err = LispError::SpliceOutsideList {
            got: SexpWitness::new(SexpShape::Symbol, "xs"),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("`,@` may only appear inside a list"),
            "expected legacy substring in message, got: {msg}"
        );
        assert!(
            msg.contains("(got ,@xs)"),
            "expected offending-form parenthetical in message, got: {msg}"
        );
    }

    // ── SexpWitness: typed joint-identity lift for offending-Sexp slots ──
    //
    // The `SexpWitness { shape: SexpShape, display: String }` typed
    // primitive lands as the first promotion of a `got: String`
    // Sexp::Display-projection slot to a typed joint identity. Pins:
    // (a) `SexpWitness::Display` writes only the `display` field
    // (byte-for-byte legacy rendering preserved); (b) the struct's
    // `shape` field is pattern-matchable for tooling that wants
    // structural binding without re-parsing the literal; (c) the
    // `SpliceOutsideList.got` slot's typed shape is now load-bearing
    // — a regression that re-collapses `got` to a free-form `String`
    // loses the rustc-enforced typed-shape guarantee.

    #[test]
    fn sexp_witness_display_writes_only_the_display_field() {
        // Pin the byte-for-byte rendering contract: `SexpWitness`'s
        // `Display` impl writes ONLY the `display` field, NOT the
        // shape label. This is the load-bearing posture for the
        // `#[error("...(got ,@{got})")]` annotation on
        // `SpliceOutsideList` — the parenthetical reads `(got ,@xs)`
        // verbatim, not `(got ,@symbol xs)`. A regression that adds
        // a shape prefix to Display fails-loudly here AND at every
        // legacy-rendering test downstream.
        let w = SexpWitness::new(SexpShape::Symbol, "xs");
        assert_eq!(format!("{w}"), "xs");
        let w = SexpWitness::new(SexpShape::List, "(list 1 2)");
        assert_eq!(format!("{w}"), "(list 1 2)");
        let w = SexpWitness::new(SexpShape::Int, "5");
        assert_eq!(format!("{w}"), "5");
        let w = SexpWitness::new(SexpShape::Keyword, ":foo");
        assert_eq!(format!("{w}"), ":foo");
    }

    #[test]
    fn sexp_witness_carries_both_shape_and_display_jointly() {
        // Pin the joint-identity contract: `SexpWitness` carries BOTH
        // halves of the offending-value identity (typed `SexpShape`
        // AND literal `Sexp::Display` projection) in ONE owned value.
        // Tools that pattern-match on `witness.shape` bind to the
        // structural identity; tools that render via `{witness}` get
        // the literal value. Neither half is recoverable from the
        // other (a `Sexp::Display` projection of `"5"` could be Int
        // or Symbol — substring parsing can't recover the structural
        // identity reliably), so the typed witness is the canonical
        // source for both.
        let w = SexpWitness::new(SexpShape::Int, "5");
        assert_eq!(w.shape, SexpShape::Int);
        assert_eq!(w.display, "5");
        // The literal `"5"` would substring-grep the same as a hand-
        // written symbol named `5`, but the typed shape distinguishes
        // them structurally — a regression that drops the shape slot
        // would collapse this distinction.
        let w_sym = SexpWitness::new(SexpShape::Symbol, "5");
        assert_eq!(w_sym.shape, SexpShape::Symbol);
        assert_eq!(w_sym.display, "5");
        assert_ne!(
            w, w_sym,
            "Witnesses with same display but different shape must NOT be equal — typed shape is load-bearing data",
        );
    }

    #[test]
    fn splice_outside_list_got_carries_typed_witness_through_variant_slot() {
        // Pin the structural binding on `LispError::SpliceOutsideList.got`
        // — a regression that re-introduces a `String`-shaped got slot
        // (collapsing the typed witness back into a free-form literal)
        // fails-loudly here. After this lift the variant's typed slot
        // is the joint `SexpWitness` identity; the Display projection
        // through `SexpWitness::Display` writes only the `display`
        // field so the rendered `(got ,@<display>)` parenthetical is
        // byte-for-byte identical to the legacy `got: String` shape.
        let err = LispError::SpliceOutsideList {
            got: SexpWitness::new(SexpShape::List, "(list 1 2)"),
        };
        match &err {
            LispError::SpliceOutsideList { got } => {
                assert_eq!(got.shape, SexpShape::List);
                assert_eq!(got.display, "(list 1 2)");
            }
            other => panic!("expected SpliceOutsideList, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in ,@: `,@` may only appear inside a list (got ,@(list 1 2))"
        );
    }

    #[test]
    fn splice_outside_list_got_distinguishes_symbol_from_list_at_variant_slot() {
        // Pin the typed-shape bifurcation at the variant slot: a
        // `,@xs` (symbol unquote-splice outside list) and a `,@(list 1 2)`
        // (list-literal unquote-splice outside list) BOTH route to
        // `SpliceOutsideList`, but the typed `got.shape` slot
        // distinguishes them structurally — `SexpShape::Symbol` vs.
        // `SexpShape::List`. A regression that erases the typed
        // shape (e.g., reverts to `got: String`) would lose this
        // distinction — tooling that wants to surface "you wrote a
        // symbol `,@xs` outside a list; bind `xs` to a list first"
        // vs. "you wrote a list literal `,@(list 1 2)` outside a
        // list; nest it inside `(outer ,@(...))`" would have to
        // substring-grep the `display` field, brittle.
        let err_sym = LispError::SpliceOutsideList {
            got: SexpWitness::new(SexpShape::Symbol, "xs"),
        };
        let err_list = LispError::SpliceOutsideList {
            got: SexpWitness::new(SexpShape::List, "(list 1 2)"),
        };
        let (sym_shape, list_shape) = (
            match &err_sym {
                LispError::SpliceOutsideList { got } => got.shape,
                _ => unreachable!(),
            },
            match &err_list {
                LispError::SpliceOutsideList { got } => got.shape,
                _ => unreachable!(),
            },
        );
        assert_ne!(
            sym_shape, list_shape,
            "Symbol and List witnesses must remain structurally distinct at the variant slot",
        );
        assert_eq!(sym_shape, SexpShape::Symbol);
        assert_eq!(list_shape, SexpShape::List);
    }

    #[test]
    fn non_symbol_unquote_target_got_carries_typed_witness_through_variant_slot() {
        // Sibling pin to
        // `splice_outside_list_got_carries_typed_witness_through_variant_slot`
        // for the template-gate's OTHER `,X/,@X` rejection variant.
        // After this lift `LispError::NonSymbolUnquoteTarget.got` is
        // the typed joint `SexpWitness` identity — the same
        // primitive `SpliceOutsideList.got` already carries. The two
        // template-gate `,X/,@X` rejection variants now share ONE
        // typed witness identity at their `got` slot; authoring tools
        // bind on `got.shape` AND `got.display` jointly across both
        // sites rather than substring-grepping a free-form String on
        // each. A regression that re-collapses `got` to `String` loses
        // the rustc-enforced closed-set guarantee on shape identity
        // here.
        let err = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Unquote,
            got: SexpWitness::new(SexpShape::List, "(list 1 2)"),
        };
        match &err {
            LispError::NonSymbolUnquoteTarget { prefix, got } => {
                assert_eq!(*prefix, UnquoteForm::Unquote);
                assert_eq!(got.shape, SexpShape::List);
                assert_eq!(got.display, "(list 1 2)");
            }
            other => panic!("expected NonSymbolUnquoteTarget, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in ,: expected symbol, got (list 1 2)"
        );
    }

    #[test]
    fn non_symbol_unquote_target_got_distinguishes_int_from_keyword_at_variant_slot() {
        // Pin the typed-shape bifurcation at the variant slot — `,5`
        // (int atom in unquote slot) and `,:foo` (keyword atom in
        // unquote slot) BOTH route to `NonSymbolUnquoteTarget`, but
        // the typed `got.shape` slot distinguishes them structurally
        // — `SexpShape::Int` vs. `SexpShape::Keyword`. Sibling pin
        // for the same structural-shape-bifurcation property
        // `splice_outside_list_got_distinguishes_symbol_from_list_at_variant_slot`
        // pins on `SpliceOutsideList`. A regression that erases the
        // typed shape (e.g., reverts to `got: String`) would lose
        // this distinction — tooling that wants to surface "you wrote
        // an int `,5` where a symbol was expected; only symbols are
        // substitutable in templates" vs. "you wrote a keyword `,:foo`
        // where a symbol was expected; keywords aren't substitutable
        // (did you mean `,foo`?)" would have to substring-grep the
        // `display` field, brittle.
        let err_int = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Unquote,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        let err_kw = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Unquote,
            got: SexpWitness::new(SexpShape::Keyword, ":foo"),
        };
        let (int_shape, kw_shape) = (
            match &err_int {
                LispError::NonSymbolUnquoteTarget { got, .. } => got.shape,
                _ => unreachable!(),
            },
            match &err_kw {
                LispError::NonSymbolUnquoteTarget { got, .. } => got.shape,
                _ => unreachable!(),
            },
        );
        assert_ne!(
            int_shape, kw_shape,
            "Int and Keyword witnesses must remain structurally distinct at the variant slot",
        );
        assert_eq!(int_shape, SexpShape::Int);
        assert_eq!(kw_shape, SexpShape::Keyword);
    }

    #[test]
    fn non_symbol_unquote_target_and_splice_outside_list_share_one_witness_primitive() {
        // Pin that BOTH template-gate `,X/,@X` rejection variants
        // (`NonSymbolUnquoteTarget` AND `SpliceOutsideList`) carry
        // the SAME typed `SexpWitness` primitive at their `got` slot
        // — the closed set of "offending inner Sexp" identities is
        // bound by ONE typed primitive across both rejection
        // surfaces. A regression that diverges the slot type on one
        // variant (e.g., re-collapses NonSymbolUnquoteTarget.got to
        // String while leaving SpliceOutsideList.got as
        // SexpWitness) fails-loudly here because the assignment
        // round-trips the witness across both slot types.
        let same_witness = SexpWitness::new(SexpShape::List, "(list 1 2)");
        let non_symbol_target = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Splice,
            got: same_witness.clone(),
        };
        let splice_outside = LispError::SpliceOutsideList {
            got: same_witness.clone(),
        };
        match (&non_symbol_target, &splice_outside) {
            (
                LispError::NonSymbolUnquoteTarget { got: lhs_got, .. },
                LispError::SpliceOutsideList { got: rhs_got },
            ) => {
                assert_eq!(lhs_got.shape, rhs_got.shape);
                assert_eq!(lhs_got.display, rhs_got.display);
                assert_eq!(*lhs_got, same_witness);
                assert_eq!(*rhs_got, same_witness);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn non_symbol_param_got_carries_typed_witness_through_variant_slot() {
        // Pin the structural binding AND the Display projection on
        // `LispError::NonSymbolParam.got`. After this lift the variant's
        // typed slot is the joint `SexpWitness` identity — the same
        // primitive `SpliceOutsideList.got` and
        // `NonSymbolUnquoteTarget.got` already carry. A regression that
        // re-collapses `got` to `String` (losing the rustc-enforced
        // closed-set guarantee on shape identity) fails-loudly here.
        // The Display projection through `SexpWitness::Display` writes
        // only the `display` field so the rendered `at position X, got
        // <display>` clause is byte-for-byte identical to the legacy
        // `got: String` shape; downstream substring-grep consumers
        // (`tatara-check`, REPL) see no drift.
        let err = LispError::NonSymbolParam {
            position: 1,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        match &err {
            LispError::NonSymbolParam { position, got } => {
                assert_eq!(*position, 1);
                assert_eq!(got.shape, SexpShape::Int);
                assert_eq!(got.display, "5");
            }
            other => panic!("expected NonSymbolParam, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro params: expected symbol at \
             position 1, got 5"
        );
    }

    #[test]
    fn non_symbol_param_got_distinguishes_int_from_keyword_at_variant_slot() {
        // Pin the typed-shape bifurcation at the variant slot — `5`
        // (int atom at a param-list position) and `:foo` (keyword atom
        // at a param-list position) BOTH route to `NonSymbolParam`, but
        // the typed `got.shape` slot distinguishes them structurally —
        // `SexpShape::Int` vs. `SexpShape::Keyword`. Sibling pin for
        // the same structural-shape-bifurcation property
        // `non_symbol_unquote_target_got_distinguishes_int_from_keyword_at_variant_slot`
        // pins on `NonSymbolUnquoteTarget`. A regression that erases
        // the typed shape (e.g., reverts to `got: String`) would lose
        // this distinction — tooling that wants to surface "you wrote
        // an int `5` where a symbol was expected at param-list position
        // 0" vs. "you wrote a keyword `:foo` where a symbol was expected
        // at param-list position 0 (did you mean `foo`?)" would have to
        // substring-grep the `display` field, brittle.
        let err_int = LispError::NonSymbolParam {
            position: 0,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        let err_kw = LispError::NonSymbolParam {
            position: 0,
            got: SexpWitness::new(SexpShape::Keyword, ":foo"),
        };
        let (int_shape, kw_shape) = (
            match &err_int {
                LispError::NonSymbolParam { got, .. } => got.shape,
                _ => unreachable!(),
            },
            match &err_kw {
                LispError::NonSymbolParam { got, .. } => got.shape,
                _ => unreachable!(),
            },
        );
        assert_ne!(
            int_shape, kw_shape,
            "Int and Keyword witnesses must remain structurally distinct at the variant slot",
        );
        assert_eq!(int_shape, SexpShape::Int);
        assert_eq!(kw_shape, SexpShape::Keyword);
    }

    #[test]
    fn non_symbol_param_and_template_gate_share_one_witness_primitive() {
        // Pin that ALL THREE Sexp-display-source `got` slots in the
        // substrate (`NonSymbolParam`, `NonSymbolUnquoteTarget`,
        // `SpliceOutsideList`) carry the SAME typed `SexpWitness`
        // primitive — the closed set of "offending inner Sexp"
        // identities is bound by ONE typed primitive across the three
        // rejection surfaces (the defmacro-syntax-gate's `parse_params`
        // walker AND the template-gate's `,X/,@X` pair). A regression
        // that diverges the slot type on any one variant (e.g.,
        // re-collapses `NonSymbolParam.got` to `String` while leaving
        // the template-gate variants typed) fails-loudly here because
        // the assignment round-trips the witness across all three slot
        // types. Sibling pin to
        // `non_symbol_unquote_target_and_splice_outside_list_share_one_witness_primitive`
        // — extending the typed-identity unification contract from
        // two slots to three.
        let same_witness = SexpWitness::new(SexpShape::List, "(nested)");
        let non_symbol_param = LispError::NonSymbolParam {
            position: 0,
            got: same_witness.clone(),
        };
        let non_symbol_target = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Unquote,
            got: same_witness.clone(),
        };
        let splice_outside = LispError::SpliceOutsideList {
            got: same_witness.clone(),
        };
        match (&non_symbol_param, &non_symbol_target, &splice_outside) {
            (
                LispError::NonSymbolParam { got: a, .. },
                LispError::NonSymbolUnquoteTarget { got: b, .. },
                LispError::SpliceOutsideList { got: c },
            ) => {
                assert_eq!(a.shape, b.shape);
                assert_eq!(b.shape, c.shape);
                assert_eq!(a.display, b.display);
                assert_eq!(b.display, c.display);
                assert_eq!(*a, same_witness);
                assert_eq!(*b, same_witness);
                assert_eq!(*c, same_witness);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn defmacro_non_symbol_name_got_carries_typed_witness_through_variant_slot() {
        // Pin the structural binding AND the Display projection on
        // `LispError::DefmacroNonSymbolName.got`. After this lift the
        // variant's typed slot is the joint `SexpWitness` identity —
        // the same primitive `SpliceOutsideList.got`,
        // `NonSymbolUnquoteTarget.got`, and `NonSymbolParam.got`
        // already carry. A regression that re-collapses `got` to
        // `String` (losing the rustc-enforced closed-set guarantee on
        // shape identity at the defmacro-syntax-gate's name-slot
        // rejection variant) fails-loudly here. The Display projection
        // through `SexpWitness::Display` writes only the `display`
        // field so the rendered `compile error in {head}: expected
        // name symbol, got <display>` clause is byte-for-byte
        // identical to the legacy `got: String` shape; downstream
        // substring-grep consumers (`tatara-check`, REPL) see no
        // drift.
        let err = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::Defmacro,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        match &err {
            LispError::DefmacroNonSymbolName { head, got } => {
                assert_eq!(*head, MacroDefHead::Defmacro);
                assert_eq!(got.shape, SexpShape::Int);
                assert_eq!(got.display, "5");
            }
            other => panic!("expected DefmacroNonSymbolName, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro: expected name symbol, got 5"
        );
    }

    #[test]
    fn defmacro_non_symbol_name_got_distinguishes_int_from_keyword_at_variant_slot() {
        // Pin the typed-shape bifurcation at the variant slot — `5`
        // (int atom at the defmacro name slot) and `:foo` (keyword
        // atom at the defmacro name slot) BOTH route to
        // `DefmacroNonSymbolName`, but the typed `got.shape` slot
        // distinguishes them structurally — `SexpShape::Int`
        // vs. `SexpShape::Keyword`. Sibling pin for the same
        // structural-shape-bifurcation property
        // `non_symbol_param_got_distinguishes_int_from_keyword_at_variant_slot`
        // pins on `NonSymbolParam` and
        // `non_symbol_unquote_target_got_distinguishes_int_from_keyword_at_variant_slot`
        // pins on `NonSymbolUnquoteTarget`. A regression that erases
        // the typed shape (e.g., reverts to `got: String`) would lose
        // this distinction — tooling that wants to surface "you wrote
        // an int `5` where a name symbol was expected" vs. "you wrote
        // a keyword `:foo` where a name symbol was expected (did you
        // mean `foo`?)" would have to substring-grep the `display`
        // field, brittle.
        let err_int = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::Defmacro,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        let err_kw = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::Defmacro,
            got: SexpWitness::new(SexpShape::Keyword, ":foo"),
        };
        let (int_shape, kw_shape) = (
            match &err_int {
                LispError::DefmacroNonSymbolName { got, .. } => got.shape,
                _ => unreachable!(),
            },
            match &err_kw {
                LispError::DefmacroNonSymbolName { got, .. } => got.shape,
                _ => unreachable!(),
            },
        );
        assert_ne!(
            int_shape, kw_shape,
            "Int and Keyword witnesses must remain structurally distinct at the variant slot",
        );
        assert_eq!(int_shape, SexpShape::Int);
        assert_eq!(kw_shape, SexpShape::Keyword);
    }

    #[test]
    fn defmacro_non_symbol_name_and_param_gate_share_one_witness_primitive() {
        // Pin that ALL FOUR Sexp-display-source `got` slots in the
        // substrate (`SpliceOutsideList`, `NonSymbolUnquoteTarget`,
        // `NonSymbolParam`, `DefmacroNonSymbolName`) carry the SAME
        // typed `SexpWitness` primitive — the closed set of
        // "offending inner Sexp" identities is bound by ONE typed
        // primitive across FOUR rejection surfaces: the
        // template-gate's `,X/,@X` pair, the defmacro-syntax-gate's
        // `parse_params` walker, AND the defmacro-syntax-gate's
        // outer name-slot rejection. A regression that diverges the
        // slot type on any one variant (e.g., re-collapses
        // `DefmacroNonSymbolName.got` to `String` while leaving the
        // others typed) fails-loudly here because the assignment
        // round-trips the witness across all four slot types. Sibling
        // pin to `non_symbol_param_and_template_gate_share_one_witness_primitive`
        // — extending the typed-identity unification contract from
        // three slots to four.
        let same_witness = SexpWitness::new(SexpShape::List, "(nested)");
        let defmacro_non_symbol_name = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::Defmacro,
            got: same_witness.clone(),
        };
        let non_symbol_param = LispError::NonSymbolParam {
            position: 0,
            got: same_witness.clone(),
        };
        let non_symbol_target = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Unquote,
            got: same_witness.clone(),
        };
        let splice_outside = LispError::SpliceOutsideList {
            got: same_witness.clone(),
        };
        match (
            &defmacro_non_symbol_name,
            &non_symbol_param,
            &non_symbol_target,
            &splice_outside,
        ) {
            (
                LispError::DefmacroNonSymbolName { got: a, .. },
                LispError::NonSymbolParam { got: b, .. },
                LispError::NonSymbolUnquoteTarget { got: c, .. },
                LispError::SpliceOutsideList { got: d },
            ) => {
                assert_eq!(a.shape, b.shape);
                assert_eq!(b.shape, c.shape);
                assert_eq!(c.shape, d.shape);
                assert_eq!(a.display, b.display);
                assert_eq!(b.display, c.display);
                assert_eq!(c.display, d.display);
                assert_eq!(*a, same_witness);
                assert_eq!(*b, same_witness);
                assert_eq!(*c, same_witness);
                assert_eq!(*d, same_witness);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn defmacro_non_list_params_got_carries_typed_witness_through_variant_slot() {
        // Pin the structural binding AND the Display projection on
        // `LispError::DefmacroNonListParams.got`. After this lift the
        // variant's typed slot is the joint `SexpWitness` identity —
        // the same primitive `SpliceOutsideList.got`,
        // `NonSymbolUnquoteTarget.got`, `NonSymbolParam.got`, and
        // `DefmacroNonSymbolName.got` already carry. A regression that
        // re-collapses `got` to `String` (losing the rustc-enforced
        // closed-set guarantee on shape identity at the defmacro-
        // syntax-gate's param-list-slot rejection variant) fails-loudly
        // here. The Display projection through `SexpWitness::Display`
        // writes only the `display` field so the rendered `compile
        // error in {head}: expected param list, got <display>` clause
        // is byte-for-byte identical to the legacy `got: String`
        // shape; downstream substring-grep consumers (`tatara-check`,
        // REPL) see no drift.
        let err = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defmacro,
            got: SexpWitness::new(SexpShape::Symbol, "x"),
        };
        match &err {
            LispError::DefmacroNonListParams { head, got } => {
                assert_eq!(*head, MacroDefHead::Defmacro);
                assert_eq!(got.shape, SexpShape::Symbol);
                assert_eq!(got.display, "x");
            }
            other => panic!("expected DefmacroNonListParams, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro: expected param list, got x"
        );
    }

    #[test]
    fn defmacro_non_list_params_got_distinguishes_symbol_from_int_at_variant_slot() {
        // Pin the typed-shape bifurcation at the variant slot — `x`
        // (symbol atom at the defmacro param-list slot) and `5`
        // (int atom at the defmacro param-list slot) BOTH route to
        // `DefmacroNonListParams`, but the typed `got.shape` slot
        // distinguishes them structurally — `SexpShape::Symbol`
        // vs. `SexpShape::Int`. Sibling pin for the same
        // structural-shape-bifurcation property
        // `defmacro_non_symbol_name_got_distinguishes_int_from_keyword_at_variant_slot`
        // pins on `DefmacroNonSymbolName` and
        // `non_symbol_param_got_distinguishes_int_from_keyword_at_variant_slot`
        // pins on `NonSymbolParam`. A regression that erases the typed
        // shape (e.g., reverts to `got: String`) would lose this
        // distinction — tooling that wants to surface "you wrote a
        // symbol `x` where a param list was expected (did you mean
        // `(x)`?)" vs. "you wrote an int `5` where a param list was
        // expected" would have to substring-grep the `display` field,
        // brittle. The symbol-vs-int bifurcation matters at THIS slot
        // (not the int-vs-keyword bifurcation pinned at the
        // name-slot variant) because the most common authoring
        // mistake at the param-list slot is to forget the wrapping
        // parens — `(defmacro f x body)` instead of `(defmacro f (x)
        // body)` — so the symbol shape is the natural sibling to
        // distinguish from numeric typos.
        let err_sym = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defmacro,
            got: SexpWitness::new(SexpShape::Symbol, "x"),
        };
        let err_int = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defmacro,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        let (sym_shape, int_shape) = (
            match &err_sym {
                LispError::DefmacroNonListParams { got, .. } => got.shape,
                _ => unreachable!(),
            },
            match &err_int {
                LispError::DefmacroNonListParams { got, .. } => got.shape,
                _ => unreachable!(),
            },
        );
        assert_ne!(
            sym_shape, int_shape,
            "Symbol and Int witnesses must remain structurally distinct at the variant slot",
        );
        assert_eq!(sym_shape, SexpShape::Symbol);
        assert_eq!(int_shape, SexpShape::Int);
    }

    #[test]
    fn defmacro_non_list_params_and_name_gate_share_one_witness_primitive() {
        // Pin that ALL FIVE Sexp-display-source `got` slots in the
        // substrate (`SpliceOutsideList`, `NonSymbolUnquoteTarget`,
        // `NonSymbolParam`, `DefmacroNonSymbolName`,
        // `DefmacroNonListParams`) carry the SAME typed `SexpWitness`
        // primitive — the closed set of "offending inner Sexp"
        // identities is bound by ONE typed primitive across FIVE
        // rejection surfaces: the template-gate's `,X/,@X` pair, the
        // defmacro-syntax-gate's `parse_params` walker, AND BOTH of
        // the defmacro-syntax-gate's outer `macro_def_from` rejection
        // points (name-symbol AND param-list — the second and third of
        // the three `macro_def_from` gates). A regression that
        // diverges the slot type on any one variant (e.g., re-collapses
        // `DefmacroNonListParams.got` to `String` while leaving the
        // others typed) fails-loudly here because the assignment
        // round-trips the witness across all five slot types. Sibling
        // pin to `defmacro_non_symbol_name_and_param_gate_share_one_witness_primitive`
        // — extending the typed-identity unification contract from
        // four slots to five, completing structural unification of the
        // entire `macro_def_from` rejection chain at the
        // `Sexp::Display`-source `got` slot (every offending inner
        // Sexp value that `macro_def_from` rejects now carries the
        // SAME typed witness, regardless of which of the three gates
        // — arity, name-symbol, param-list — fired).
        let same_witness = SexpWitness::new(SexpShape::List, "(nested)");
        let defmacro_non_list_params = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defmacro,
            got: same_witness.clone(),
        };
        let defmacro_non_symbol_name = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::Defmacro,
            got: same_witness.clone(),
        };
        let non_symbol_param = LispError::NonSymbolParam {
            position: 0,
            got: same_witness.clone(),
        };
        let non_symbol_target = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Unquote,
            got: same_witness.clone(),
        };
        let splice_outside = LispError::SpliceOutsideList {
            got: same_witness.clone(),
        };
        match (
            &defmacro_non_list_params,
            &defmacro_non_symbol_name,
            &non_symbol_param,
            &non_symbol_target,
            &splice_outside,
        ) {
            (
                LispError::DefmacroNonListParams { got: a, .. },
                LispError::DefmacroNonSymbolName { got: b, .. },
                LispError::NonSymbolParam { got: c, .. },
                LispError::NonSymbolUnquoteTarget { got: d, .. },
                LispError::SpliceOutsideList { got: e },
            ) => {
                assert_eq!(a.shape, b.shape);
                assert_eq!(b.shape, c.shape);
                assert_eq!(c.shape, d.shape);
                assert_eq!(d.shape, e.shape);
                assert_eq!(a.display, b.display);
                assert_eq!(b.display, c.display);
                assert_eq!(c.display, d.display);
                assert_eq!(d.display, e.display);
                assert_eq!(*a, same_witness);
                assert_eq!(*b, same_witness);
                assert_eq!(*c, same_witness);
                assert_eq!(*d, same_witness);
                assert_eq!(*e, same_witness);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn rest_param_missing_name_got_carries_typed_witness_through_variant_slot() {
        // Pin the structural binding AND the Display projection on
        // `LispError::RestParamMissingName.got`. After this lift the
        // variant's typed slot is `Option<SexpWitness>` — the joint
        // `SexpWitness` identity (the same primitive `SpliceOutsideList.got`,
        // `NonSymbolUnquoteTarget.got`, `NonSymbolParam.got`,
        // `DefmacroNonSymbolName.got`, and `DefmacroNonListParams.got`
        // already carry) wrapped in `Option` because the post-`&rest`
        // follower slot bifurcates structurally between "missing
        // entirely" (`None`) and "present but malformed"
        // (`Some(SexpWitness)`). The typed witness lands on the `Some`
        // arm only. A regression that re-collapses to a free-form
        // `Option<String>` got slot (losing the rustc-enforced
        // closed-set guarantee on shape identity at the
        // `parse_params` walker's `&rest`-follower rejection variant)
        // fails-loudly here. The Display projection through
        // `SexpWitness::Display` writes only the `display` field so
        // the rendered `(rest marker at position {rest_position}, got
        // <display>)` clause is byte-for-byte identical to the
        // pre-lift `Option<String>` shape; downstream substring-grep
        // consumers (`tatara-check`, REPL) see no drift.
        let err = LispError::RestParamMissingName {
            rest_position: 1,
            got: Some(SexpWitness::new(SexpShape::Int, "5")),
        };
        match &err {
            LispError::RestParamMissingName { rest_position, got } => {
                assert_eq!(*rest_position, 1);
                let witness = got.as_ref().expect("got must be Some");
                assert_eq!(witness.shape, SexpShape::Int);
                assert_eq!(witness.display, "5");
            }
            other => panic!("expected RestParamMissingName, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro params: &rest needs a name \
             (rest marker at position 1, got 5)"
        );
    }

    #[test]
    fn rest_param_missing_name_got_distinguishes_int_from_keyword_at_variant_slot() {
        // Pin the typed-shape bifurcation at the variant slot — `5`
        // (int atom at the post-`&rest` follower slot) and `:foo`
        // (keyword atom at the post-`&rest` follower slot) BOTH route
        // to `RestParamMissingName` on the `Some` arm, but the typed
        // `got.shape` slot distinguishes them structurally —
        // `SexpShape::Int` vs. `SexpShape::Keyword`. Sibling pin for
        // the same structural-shape-bifurcation property
        // `non_symbol_param_got_distinguishes_int_from_keyword_at_variant_slot`
        // pins on `NonSymbolParam` and
        // `defmacro_non_symbol_name_got_distinguishes_int_from_keyword_at_variant_slot`
        // pins on `DefmacroNonSymbolName`. A regression that erases
        // the typed shape (e.g., reverts to `got: Option<String>`)
        // would lose this distinction — tooling that wants to surface
        // "you wrote an int `5` where a rest-name was expected" vs.
        // "you wrote a keyword `:foo` where a rest-name was expected
        // (did you mean `foo`?)" would have to substring-grep the
        // `display` field, brittle.
        let err_int = LispError::RestParamMissingName {
            rest_position: 0,
            got: Some(SexpWitness::new(SexpShape::Int, "5")),
        };
        let err_kw = LispError::RestParamMissingName {
            rest_position: 0,
            got: Some(SexpWitness::new(SexpShape::Keyword, ":foo")),
        };
        let (int_shape, kw_shape) = (
            match &err_int {
                LispError::RestParamMissingName { got: Some(w), .. } => w.shape,
                _ => unreachable!(),
            },
            match &err_kw {
                LispError::RestParamMissingName { got: Some(w), .. } => w.shape,
                _ => unreachable!(),
            },
        );
        assert_ne!(
            int_shape, kw_shape,
            "Int and Keyword witnesses must remain structurally distinct at the variant slot",
        );
        assert_eq!(int_shape, SexpShape::Int);
        assert_eq!(kw_shape, SexpShape::Keyword);
    }

    #[test]
    fn rest_param_missing_name_and_macro_def_gate_share_one_witness_primitive() {
        // Pin that ALL SIX Sexp-display-source `got` slots in the
        // substrate (`SpliceOutsideList`, `NonSymbolUnquoteTarget`,
        // `NonSymbolParam`, `DefmacroNonSymbolName`,
        // `DefmacroNonListParams`, `RestParamMissingName`) carry the
        // SAME typed `SexpWitness` primitive — the closed set of
        // "offending inner Sexp" identities is bound by ONE typed
        // primitive across SIX rejection surfaces: the template-gate's
        // `,X/,@X` pair, the defmacro-syntax-gate's `parse_params`
        // walker (BOTH non-symbol-param AND post-`&rest`-non-symbol-
        // follower rejection points), AND BOTH of the
        // defmacro-syntax-gate's outer `macro_def_from` rejection
        // points (name-symbol AND param-list). The `Option`-wrap on
        // `RestParamMissingName.got` is the bifurcation between
        // "missing entirely" and "present but malformed"; the typed
        // witness rides on the `Some` arm and is structurally identical
        // to the other five variants' got slots. A regression that
        // diverges the slot type on any one variant (e.g., re-collapses
        // `RestParamMissingName.got` to `Option<String>` while leaving
        // the others typed) fails-loudly here because the assignment
        // round-trips the witness across all six slot types. Sibling
        // pin to
        // `defmacro_non_list_params_and_name_gate_share_one_witness_primitive`
        // — extending the typed-identity unification contract from
        // five slots to six.
        let same_witness = SexpWitness::new(SexpShape::List, "(nested)");
        let rest_param_missing_name = LispError::RestParamMissingName {
            rest_position: 0,
            got: Some(same_witness.clone()),
        };
        let defmacro_non_list_params = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defmacro,
            got: same_witness.clone(),
        };
        let defmacro_non_symbol_name = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::Defmacro,
            got: same_witness.clone(),
        };
        let non_symbol_param = LispError::NonSymbolParam {
            position: 0,
            got: same_witness.clone(),
        };
        let non_symbol_target = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Unquote,
            got: same_witness.clone(),
        };
        let splice_outside = LispError::SpliceOutsideList {
            got: same_witness.clone(),
        };
        match (
            &rest_param_missing_name,
            &defmacro_non_list_params,
            &defmacro_non_symbol_name,
            &non_symbol_param,
            &non_symbol_target,
            &splice_outside,
        ) {
            (
                LispError::RestParamMissingName { got: Some(a), .. },
                LispError::DefmacroNonListParams { got: b, .. },
                LispError::DefmacroNonSymbolName { got: c, .. },
                LispError::NonSymbolParam { got: d, .. },
                LispError::NonSymbolUnquoteTarget { got: e, .. },
                LispError::SpliceOutsideList { got: f },
            ) => {
                assert_eq!(a.shape, b.shape);
                assert_eq!(b.shape, c.shape);
                assert_eq!(c.shape, d.shape);
                assert_eq!(d.shape, e.shape);
                assert_eq!(e.shape, f.shape);
                assert_eq!(a.display, b.display);
                assert_eq!(b.display, c.display);
                assert_eq!(c.display, d.display);
                assert_eq!(d.display, e.display);
                assert_eq!(e.display, f.display);
                assert_eq!(*a, same_witness);
                assert_eq!(*b, same_witness);
                assert_eq!(*c, same_witness);
                assert_eq!(*d, same_witness);
                assert_eq!(*e, same_witness);
                assert_eq!(*f, same_witness);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn missing_macro_arg_display_matches_legacy_compile_shape() {
        // The variant renders byte-for-byte the same string the legacy
        // `Compile { form: format!("call to {macro_name}"), message:
        // format!("missing required arg: {param}") }` shape produced, so
        // authoring tools (REPL, LSP, `tatara-check`) that substring-match
        // on the rendered diagnostic see no drift; tools that pattern-match
        // on the variant gain structural binding to `macro_name` and
        // `param`.
        let err = LispError::MissingMacroArg {
            macro_name: "wrap".into(),
            param: "b".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in call to wrap: missing required arg: b"
        );
    }

    #[test]
    fn missing_macro_arg_display_carries_kebab_case_names_unchanged() {
        // Both `macro_name` and `param` round-trip through the variant's
        // Display unchanged. Pinning this contract means a regression that
        // camelCases or lowercases either side fails-loudly here.
        let err = LispError::MissingMacroArg {
            macro_name: "wrap-twice".into(),
            param: "notify-ref".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in call to wrap-twice: \
             missing required arg: notify-ref"
        );
    }

    #[test]
    fn missing_macro_arg_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring as a separate assertion so a regression
        // that drifts the wording (e.g., to "missing arg" or "no arg
        // provided") fails-loudly here even if the head clause changes
        // shape. The substring is what consumers downstream
        // (tatara-check, the REPL) substring-match on today.
        let err = LispError::MissingMacroArg {
            macro_name: "f".into(),
            param: "x".into(),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("missing required arg: x"),
            "expected legacy substring in message, got: {msg}"
        );
        assert!(
            msg.contains("call to f"),
            "expected call-to clause in message, got: {msg}"
        );
    }

    #[test]
    fn non_symbol_param_display_carries_position_and_got() {
        // The variant renders both the failing position (0-based index
        // within the param list) AND the offending element via
        // `Sexp::Display` — both fields are first-class structural data,
        // not embedded substrings of `message`. A regression that drops
        // either field from the rendered diagnostic fails-loudly here.
        let err = LispError::NonSymbolParam {
            position: 1,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro params: \
             expected symbol at position 1, got 5"
        );
    }

    #[test]
    fn non_symbol_param_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substrings — `"defmacro params"` and `"expected
        // symbol"` — as separate assertions so a regression that drifts
        // either fragment fails-loudly here even if the appended position
        // / got clause changes shape. The substrings are what consumers
        // downstream substring-match on today; the prefix matches the
        // legacy `Compile { form: "defmacro params", message: "expected
        // symbol" }` byte-for-byte.
        let err = LispError::NonSymbolParam {
            position: 0,
            got: SexpWitness::new(SexpShape::List, "(nested)"),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("defmacro params"),
            "expected legacy form label in message, got: {msg}"
        );
        assert!(
            msg.contains("expected symbol"),
            "expected legacy substring in message, got: {msg}"
        );
    }

    #[test]
    fn non_symbol_param_display_carries_keyword_got_unchanged() {
        // `Sexp::Display` for `Atom::Keyword(s)` writes `:s`; pin that
        // the variant's Display passes the keyword form through
        // unchanged so an LSP that surfaces "you wrote `:k` where a
        // symbol was expected" gains the literal value as data, no
        // re-parsing required.
        let err = LispError::NonSymbolParam {
            position: 2,
            got: SexpWitness::new(SexpShape::Keyword, ":k"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro params: \
             expected symbol at position 2, got :k"
        );
    }

    #[test]
    fn rest_param_missing_name_display_with_got_renders_marker_position_and_got() {
        // `(defmacro f (a &rest 5) …)` — `&rest` at param-list position 1,
        // followed by `5` at position 2. The variant renders both the
        // marker's position AND the offending follower via `Sexp::Display`
        // — both are first-class structural data, not embedded substrings
        // of `message`. A regression that drops either field from the
        // rendered diagnostic fails-loudly here.
        let err = LispError::RestParamMissingName {
            rest_position: 1,
            got: Some(SexpWitness::new(SexpShape::Int, "5")),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro params: &rest needs a name \
             (rest marker at position 1, got 5)"
        );
    }

    #[test]
    fn rest_param_missing_name_display_without_got_renders_marker_position_only() {
        // `(defmacro f (a &rest))` — `&rest` at param-list position 1, no
        // follower at all. The variant renders the marker position and
        // names the absence structurally (`none provided`) instead of a
        // misleading empty / partial parenthetical. Sibling of how
        // `UnknownDomainKeyword` renders `(no domains registered)` for
        // the empty-registry case — the structural reason is named.
        let err = LispError::RestParamMissingName {
            rest_position: 1,
            got: None,
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro params: &rest needs a name \
             (rest marker at position 1, none provided)"
        );
    }

    #[test]
    fn rest_param_missing_name_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substrings — `"defmacro params"` and `"&rest
        // needs a name"` — as separate assertions so a regression that
        // drifts either fragment fails-loudly here even if the appended
        // marker / got clause changes shape. The substrings are what
        // consumers downstream substring-match on today; the prefix
        // matches the legacy `Compile { form: "defmacro params",
        // message: "&rest needs a name" }` byte-for-byte.
        let err = LispError::RestParamMissingName {
            rest_position: 0,
            got: None,
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("defmacro params"),
            "expected legacy form label in message, got: {msg}"
        );
        assert!(
            msg.contains("&rest needs a name"),
            "expected legacy substring in message, got: {msg}"
        );
    }

    #[test]
    fn rest_param_missing_name_display_carries_keyword_got_unchanged() {
        // `Sexp::Display` for `Atom::Keyword(s)` writes `:s`; pin that the
        // variant's Display passes the keyword form through unchanged so
        // an LSP that surfaces "you wrote `:foo` where a rest-name was
        // expected" gains the literal keyword value as data, no
        // re-parsing required.
        let err = LispError::RestParamMissingName {
            rest_position: 2,
            got: Some(SexpWitness::new(SexpShape::Keyword, ":foo")),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro params: &rest needs a name \
             (rest marker at position 2, got :foo)"
        );
    }

    #[test]
    fn defmacro_arity_display_with_defmacro_head_renders_arity_and_legacy_template() {
        // The variant renders both the head keyword AND the actual
        // arity — both fields are first-class structural data, not
        // embedded substrings of `message`. The example template
        // `(defmacro name (params) body)` stays the literal `defmacro`
        // (not the head) — matching the legacy form's behavior so
        // authoring tools that substring-grep on the rendered
        // diagnostic see no drift. A regression that drops either
        // field from the rendered diagnostic fails-loudly here.
        let err = LispError::DefmacroArity {
            head: MacroDefHead::Defmacro,
            arity: 1,
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro: (defmacro name (params) body) required \
             (got 1 elements, need 4)"
        );
    }

    #[test]
    fn defmacro_arity_display_carries_defpoint_template_head_unchanged() {
        // Pin that the head slot accepts every literal the call-site
        // matches! gate admits — `defpoint-template` is the second
        // head keyword `macro_def_from` recognizes. The example
        // template literal stays `(defmacro name (params) body)` even
        // for non-defmacro heads (matching the legacy behavior); the
        // prefix `compile error in defpoint-template:` carries the
        // actual head so an LSP that wants to point at "your
        // defpoint-template form is missing elements" gains the head
        // as data.
        let err = LispError::DefmacroArity {
            head: MacroDefHead::DefpointTemplate,
            arity: 2,
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defpoint-template: \
             (defmacro name (params) body) required \
             (got 2 elements, need 4)"
        );
    }

    #[test]
    fn defmacro_arity_display_carries_defcheck_head_unchanged() {
        // Sibling for the `defcheck` head; rounds out the three-head-
        // keyword coverage so the variant renders identically across
        // `defmacro` / `defpoint-template` / `defcheck` (modulo the
        // head literal in the prefix).
        let err = LispError::DefmacroArity {
            head: MacroDefHead::Defcheck,
            arity: 3,
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defcheck: (defmacro name (params) body) required \
             (got 3 elements, need 4)"
        );
    }

    #[test]
    fn defmacro_arity_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring — `"(defmacro name (params) body)
        // required"` — as a separate assertion so a regression that
        // drifts the example template fails-loudly here even if the
        // appended `got X elements, need 4` clause changes shape. The
        // substring is what consumers downstream substring-match on
        // today; the prefix matches the legacy `Compile { form:
        // head.to_string(), message: "(defmacro name (params) body)
        // required" }` byte-for-byte.
        let err = LispError::DefmacroArity {
            head: MacroDefHead::Defmacro,
            arity: 0,
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("(defmacro name (params) body) required"),
            "expected legacy template substring in message, got: {msg}"
        );
        assert!(
            msg.contains("compile error in defmacro:"),
            "expected legacy form-label prefix in message, got: {msg}"
        );
    }

    #[test]
    fn defmacro_non_symbol_name_display_with_int_got_renders_legacy_prefix_and_got() {
        // `(defmacro 5 () body)` — list[1] is `5`, not a symbol. The
        // variant renders both the head keyword AND the offending
        // `Sexp::Display` projection — both fields are first-class
        // structural data, not embedded substrings of `message`. The
        // prefix `compile error in defmacro: expected name symbol`
        // matches the legacy `Compile { form: "defmacro", message:
        // "expected name symbol" }` byte-for-byte; the structural
        // detail (`, got 5`) is appended. A regression that drops
        // either field from the rendered diagnostic fails-loudly here.
        let err = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::Defmacro,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro: expected name symbol, got 5"
        );
    }

    #[test]
    fn defmacro_non_symbol_name_display_carries_defpoint_template_head_unchanged() {
        // Pin that the head slot accepts every literal the call-site
        // matches! gate admits — `defpoint-template` is the second
        // head keyword `macro_def_from` recognizes. The prefix
        // `compile error in defpoint-template:` carries the actual
        // head so an LSP that wants to point at "your defpoint-
        // template form's name slot isn't a symbol" gains the head
        // as data.
        let err = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::DefpointTemplate,
            got: SexpWitness::new(SexpShape::Keyword, ":foo"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defpoint-template: expected name symbol, got :foo"
        );
    }

    #[test]
    fn defmacro_non_symbol_name_display_carries_defcheck_head_unchanged() {
        // Sibling for the `defcheck` head; rounds out the three-head-
        // keyword coverage so the variant renders identically across
        // `defmacro` / `defpoint-template` / `defcheck` (modulo the
        // head literal in the prefix).
        let err = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::Defcheck,
            got: SexpWitness::new(SexpShape::List, "(nested)"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defcheck: expected name symbol, got (nested)"
        );
    }

    #[test]
    fn defmacro_non_symbol_name_display_carries_string_got_unchanged() {
        // `Sexp::Display` for `Atom::String(s)` writes `"s"` (with
        // quotes); pin that the variant's Display passes the string
        // form through unchanged so an LSP that surfaces "you wrote
        // `\"name\"` where a name symbol was expected" gains the
        // literal value as data, no re-parsing required.
        let err = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::Defmacro,
            got: SexpWitness::new(SexpShape::String, "\"name\""),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro: expected name symbol, got \"name\""
        );
    }

    #[test]
    fn defmacro_non_symbol_name_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring — `"expected name symbol"` — as a
        // separate assertion so a regression that drifts the wording
        // (e.g., to "expected symbol" or "name must be a symbol")
        // fails-loudly here even if the appended `, got X` clause
        // changes shape. The substring is what consumers downstream
        // (tatara-check, the REPL) substring-match on today; the
        // prefix matches the legacy `Compile { form: head.to_string(),
        // message: "expected name symbol" }` byte-for-byte.
        let err = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::Defmacro,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("expected name symbol"),
            "expected legacy substring in message, got: {msg}"
        );
        assert!(
            msg.contains("compile error in defmacro:"),
            "expected legacy form-label prefix in message, got: {msg}"
        );
    }

    #[test]
    fn defmacro_non_list_params_display_with_symbol_got_renders_legacy_prefix_and_got() {
        // `(defmacro f x body)` — list[2] is the symbol `x`, not a
        // list. The variant renders both the head keyword AND the
        // offending `Sexp::Display` projection — both fields are
        // first-class structural data, not embedded substrings of
        // `message`. The prefix `compile error in defmacro: expected
        // param list` matches the legacy `Compile { form: "defmacro",
        // message: "expected param list" }` byte-for-byte; the
        // structural detail (`, got x`) is appended. A regression that
        // drops either field from the rendered diagnostic fails-loudly
        // here.
        let err = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defmacro,
            got: SexpWitness::new(SexpShape::Symbol, "x"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro: expected param list, got x"
        );
    }

    #[test]
    fn defmacro_non_list_params_display_carries_defpoint_template_head_unchanged() {
        // Pin that the head slot accepts every literal the call-site
        // matches! gate admits — `defpoint-template` is the second
        // head keyword `macro_def_from` recognizes. The prefix
        // `compile error in defpoint-template:` carries the actual
        // head so an LSP that wants to point at "your defpoint-
        // template form's param-list slot isn't a list" gains the
        // head as data.
        let err = LispError::DefmacroNonListParams {
            head: MacroDefHead::DefpointTemplate,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defpoint-template: expected param list, got 5"
        );
    }

    #[test]
    fn defmacro_non_list_params_display_carries_defcheck_head_unchanged() {
        // Sibling for the `defcheck` head; rounds out the three-head-
        // keyword coverage so the variant renders identically across
        // `defmacro` / `defpoint-template` / `defcheck` (modulo the
        // head literal in the prefix).
        let err = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defcheck,
            got: SexpWitness::new(SexpShape::Keyword, ":k"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defcheck: expected param list, got :k"
        );
    }

    #[test]
    fn defmacro_non_list_params_display_carries_string_got_unchanged() {
        // `Sexp::Display` for `Atom::String(s)` writes `"s"` (with
        // quotes); pin that the variant's Display passes the string
        // form through unchanged so an LSP that surfaces "you wrote
        // `\"params\"` where a param list was expected" gains the
        // literal value as data, no re-parsing required.
        let err = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defmacro,
            got: SexpWitness::new(SexpShape::String, "\"params\""),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro: expected param list, got \"params\""
        );
    }

    #[test]
    fn defmacro_non_list_params_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring — `"expected param list"` — as a
        // separate assertion so a regression that drifts the wording
        // (e.g., to "expected list" or "params must be a list")
        // fails-loudly here even if the appended `, got X` clause
        // changes shape. The substring is what consumers downstream
        // (tatara-check, the REPL) substring-match on today; the
        // prefix matches the legacy `Compile { form: head.to_string(),
        // message: "expected param list" }` byte-for-byte.
        let err = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defmacro,
            got: SexpWitness::new(SexpShape::Symbol, "x"),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("expected param list"),
            "expected legacy substring in message, got: {msg}"
        );
        assert!(
            msg.contains("compile error in defmacro:"),
            "expected legacy form-label prefix in message, got: {msg}"
        );
    }

    #[test]
    fn unbound_template_var_display_preserves_splice_prefix_in_hint() {
        // Splice marker rides through both the form and the suggestion; the
        // operator never has to translate `,` ↔ `,@` mentally.
        let err = LispError::UnboundTemplateVar {
            prefix: UnquoteForm::Splice,
            name: "rsts".into(),
            hint: Some("rest".into()),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@rsts: unbound; did you mean ,@rest?"
        );
    }

    #[test]
    fn named_form_missing_name_display_renders_legacy_compile_shape() {
        // `(defpoint)` — list.len() == 1 (just the keyword, no NAME). The
        // variant renders byte-for-byte the same string the legacy
        // `Compile { form: "defpoint", message: "expected (defpoint NAME …)"
        // }` shape produced, so authoring tools (REPL, LSP, `tatara-check`)
        // that substring-match on the rendered diagnostic see no drift;
        // tools that pattern-match on the variant gain structural binding
        // to `keyword`.
        let err = LispError::NamedFormMissingName {
            keyword: "defpoint",
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defpoint: expected (defpoint NAME …)"
        );
    }

    #[test]
    fn named_form_missing_name_display_carries_defalertpolicy_keyword_unchanged() {
        // Pin path-uniformity across distinct keywords — every
        // `compile_named` caller funnels through `NamedFormMissingName`
        // with its own `T::KEYWORD`, so the variant's `keyword` slot
        // must round-trip every literal the derive macro accepts. A
        // regression that drops or rewrites the keyword (e.g.,
        // lowercasing, stripping the `def` prefix) fails-loudly here.
        let err = LispError::NamedFormMissingName {
            keyword: "defalertpolicy",
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defalertpolicy: expected (defalertpolicy NAME …)"
        );
    }

    #[test]
    fn named_form_missing_name_display_carries_kebab_case_keyword_unchanged() {
        // Kebab-cased domain keywords (`defprocess-spec`, `defalert-policy`)
        // round-trip through both occurrences of the keyword in the rendered
        // diagnostic — the prefix `compile error in {keyword}:` AND the
        // example template `(... NAME …)`. Pinning this contract means a
        // regression that camelCases either occurrence fails-loudly here.
        let err = LispError::NamedFormMissingName {
            keyword: "defprocess-spec",
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defprocess-spec: expected (defprocess-spec NAME …)"
        );
    }

    #[test]
    fn named_form_missing_name_display_preserves_unicode_ellipsis_byte_for_byte() {
        // The legacy `format!("expected ({} NAME …)", T::KEYWORD)` shape used
        // the Unicode horizontal-ellipsis character (U+2026), not the ASCII
        // three-dot sequence `...`. Pin the codepoint exactly so a regression
        // that replaces `…` with `...` fails-loudly here — consumers
        // downstream that substring-match on `"…"` would silently miss every
        // future occurrence otherwise.
        let err = LispError::NamedFormMissingName {
            keyword: "defmonitor",
        };
        let msg = format!("{err}");
        assert!(
            msg.contains('\u{2026}'),
            "expected Unicode horizontal-ellipsis (U+2026) in message, got: {msg}"
        );
        assert!(
            !msg.contains("..."),
            "expected no ASCII three-dot sequence in message, got: {msg}"
        );
    }

    #[test]
    fn named_form_missing_name_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring — `"expected ({keyword} NAME …)"` — as a
        // separate assertion so a regression that drifts the wording (e.g.,
        // to "expected NAME after keyword" or "missing positional name")
        // fails-loudly here. The substring is what consumers downstream
        // (`tatara-check`, the REPL) substring-match on today; the prefix
        // matches the legacy `Compile { form: T::KEYWORD.to_string(),
        // message: format!("expected ({} NAME …)", T::KEYWORD) }`
        // byte-for-byte.
        let err = LispError::NamedFormMissingName {
            keyword: "defmonitor",
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("expected (defmonitor NAME"),
            "expected legacy form-label prefix in message, got: {msg}"
        );
        assert!(
            msg.contains("compile error in defmonitor:"),
            "expected legacy form-label prefix in message, got: {msg}"
        );
    }

    #[test]
    fn named_form_non_symbol_name_display_renders_legacy_prefix_with_int_got() {
        // `(defpoint 5 …)` — list[1] is the int `5`. The variant renders
        // the legacy prefix `compile error in {keyword}: positional NAME
        // must be a symbol or string` byte-for-byte AND appends the
        // structural detail `(got int)` parenthetically — same posture as
        // how `MissingHeadSymbol` appends `(got 5)` and how
        // `RestParamMissingName` appends `(rest marker at position N,
        // got X)`. The `got` slot is the typed `SexpShape` enum sourced
        // from `sexp_shape`; pin the Int-arm rendering (via
        // `SexpShape::Display` to the canonical `"int"` literal) as the
        // canonical example.
        let err = LispError::NamedFormNonSymbolName {
            keyword: "defpoint",
            got: SexpShape::Int,
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defpoint: positional NAME must be a symbol or string (got int)"
        );
    }

    #[test]
    fn named_form_non_symbol_name_display_carries_keyword_got_unchanged() {
        // `(defpoint :foo …)` — list[1] is a `:foo` keyword. Pin
        // path-uniformity across distinct `SexpShape` variants: the
        // `got` slot is `SexpShape::Keyword` (the typed projection from
        // `sexp_shape(Sexp::Atom(Atom::Keyword(_)))`), threaded into
        // the parenthetical via `SexpShape::Display` -> "keyword".
        let err = LispError::NamedFormNonSymbolName {
            keyword: "defpoint",
            got: SexpShape::Keyword,
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defpoint: positional NAME must be a symbol or string (got keyword)"
        );
    }

    #[test]
    fn named_form_non_symbol_name_display_carries_list_got_unchanged() {
        // `(defpoint (nested) …)` — list[1] is a nested list. Pin the
        // `SexpShape::List` variant round-trips into the variant's
        // `got` slot unchanged so an LSP that surfaces "you wrote a
        // nested list where a NAME symbol was expected" gains the
        // structural shape as data, no re-parsing required.
        let err = LispError::NamedFormNonSymbolName {
            keyword: "defalertpolicy",
            got: SexpShape::List,
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defalertpolicy: positional NAME must be a symbol or string (got list)"
        );
    }

    #[test]
    fn named_form_non_symbol_name_display_carries_kebab_case_keyword_unchanged() {
        // Kebab-cased domain keywords (`defprocess-spec`, `defalert-policy`)
        // round-trip through the rendered diagnostic's `compile error in
        // {keyword}:` prefix unchanged. A regression that camelCases the
        // keyword fails-loudly here.
        let err = LispError::NamedFormNonSymbolName {
            keyword: "defprocess-spec",
            got: SexpShape::Int,
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defprocess-spec: positional NAME must be a symbol or string (got int)"
        );
    }

    #[test]
    fn named_form_non_symbol_name_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring — `"positional NAME must be a symbol
        // or string"` — as a separate assertion so a regression that
        // drifts the wording (e.g., to "NAME must be a symbol", "NAME
        // slot wrong-typed") fails-loudly here even if the appended
        // parenthetical changes shape. The substring is what consumers
        // downstream (`tatara-check`, the REPL) substring-match on
        // today; the prefix matches the legacy `Compile { form:
        // T::KEYWORD.to_string(), message: "positional NAME must be a
        // symbol or string" }` byte-for-byte.
        let err = LispError::NamedFormNonSymbolName {
            keyword: "defmonitor",
            got: SexpShape::Int,
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("positional NAME must be a symbol or string"),
            "expected legacy substring in message, got: {msg}"
        );
        assert!(
            msg.contains("compile error in defmonitor:"),
            "expected legacy form-label prefix in message, got: {msg}"
        );
    }

    // ── RewriterNonList: typed-exit structural-variant lift ─────────
    //
    // `rewriter_non_list_err::<T>` (the typed-exit gate of
    // `rewrite_typed::<T>`'s round-trip) was promoted from the
    // `LispError::Compile`-shaped triple to the structural
    // `LispError::RewriterNonList { keyword, got }` variant. The
    // tests below pin: (a) Display matches the legacy `"compile error
    // in {keyword}: rewriter must return a list; got {got}"` shape
    // byte-for-byte across representative `got` renderings (int,
    // symbol, nil rendered as "()", quoted form); (b) the legacy
    // substring `"rewriter must return a list; got "` and the legacy
    // prefix `"compile error in {keyword}:"` both survive the lift
    // unchanged for substring-grep consumers; (c) kebab-case keywords
    // thread unchanged; (d) `position()` is `None` today (lands as
    // one branch when source spans arrive).

    #[test]
    fn rewriter_non_list_display_renders_legacy_shape_with_int_got() {
        // `Sexp::int(42)` projects to `Sexp::Display = "42"`. The variant
        // renders the legacy `"compile error in {keyword}: rewriter must
        // return a list; got {got}"` shape byte-for-byte — same wording
        // as the pre-lift `Compile`-shaped triple.
        let err = LispError::RewriterNonList {
            keyword: "defmonitor",
            got: SexpWitness::new(SexpShape::Int, "42"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: rewriter must return a list; got 42"
        );
    }

    #[test]
    fn rewriter_non_list_display_carries_symbol_got_unchanged() {
        // `Sexp::symbol("not-a-list")` projects to `"not-a-list"`. Pin
        // path-uniformity across distinct `Sexp::Display` outputs: the
        // typed `got` slot threads the value-rendering into the
        // diagnostic unchanged via `SexpWitness::Display` (which writes
        // only the `display` field).
        let err = LispError::RewriterNonList {
            keyword: "defmonitor",
            got: SexpWitness::new(SexpShape::Symbol, "not-a-list"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: rewriter must return a list; got not-a-list"
        );
    }

    #[test]
    fn rewriter_non_list_display_carries_nil_got_as_paren_paren() {
        // `Sexp::Nil` projects to `"()"` per the `Sexp::Display`
        // contract — NOT `"nil"`. Pin the contract so a regression
        // that drifts `Sexp::Nil`'s Display fails-loudly here even
        // before reaching the rewriter end-to-end test.
        let err = LispError::RewriterNonList {
            keyword: "defmonitor",
            got: SexpWitness::new(SexpShape::Nil, "()"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: rewriter must return a list; got ()"
        );
    }

    #[test]
    fn rewriter_non_list_display_carries_kebab_case_keyword_unchanged() {
        // Kebab-cased domain keywords (`defprocess-spec`,
        // `defalert-policy`) round-trip through the rendered
        // diagnostic's `compile error in {keyword}:` prefix unchanged.
        // A regression that camelCases the keyword fails-loudly here.
        let err = LispError::RewriterNonList {
            keyword: "defprocess-spec",
            got: SexpWitness::new(SexpShape::Int, "7"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defprocess-spec: rewriter must return a list; got 7"
        );
    }

    #[test]
    fn rewriter_non_list_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring — `"rewriter must return a list;
        // got "` — as a separate assertion so a regression that drifts
        // the wording (e.g., to "rewriter returned non-list", "expected
        // list output") fails-loudly here. The substring is what
        // consumers downstream (`tatara-check`, the REPL) substring-
        // match on; the prefix matches the legacy `Compile { form:
        // T::KEYWORD.to_string(), message: format!("rewriter must
        // return a list; got {other}") }` byte-for-byte.
        let err = LispError::RewriterNonList {
            keyword: "defmonitor",
            got: SexpWitness::new(SexpShape::Int, "42"),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("rewriter must return a list; got "),
            "expected legacy substring in message, got: {msg}"
        );
        assert!(
            msg.contains("compile error in defmonitor:"),
            "expected legacy form-label prefix in message, got: {msg}"
        );
    }

    #[test]
    fn rewriter_non_list_position_is_none_today() {
        // Until `Sexp` carries source positions, the variant's
        // `position()` returns `None`. Pin the contract: a future run
        // that adds `pos: Option<usize>` lands inside `SexpWitness` in
        // ONE place and `rewrite_typed`'s rewriter-output rejection
        // picks up the span automatically because it routes through
        // one helper (`rewriter_non_list_err`).
        let err = LispError::RewriterNonList {
            keyword: "defmonitor",
            got: SexpWitness::new(SexpShape::Int, "42"),
        };
        assert_eq!(err.position(), None);
    }

    #[test]
    fn rewriter_non_list_got_carries_typed_witness_through_variant_slot() {
        // Pin the structural binding on `LispError::RewriterNonList.got`
        // — a regression that re-introduces a `String`-shaped got slot
        // (collapsing the typed witness back into a free-form literal at
        // the typed-EXIT boundary) fails-loudly here. After this lift the
        // variant's typed slot is the joint `SexpWitness` identity — the
        // SAME primitive the SEVEN typed-ENTRY-side `got` slots already
        // carry (`SpliceOutsideList`, `NonSymbolUnquoteTarget`,
        // `NonSymbolParam`, `DefmacroNonSymbolName`,
        // `DefmacroNonListParams`, `RestParamMissingName`,
        // `MissingHeadSymbol`). This is the EIGHTH consumer and the FIRST
        // on the typed-EXIT boundary; the typed-identity unification
        // contract is now closed across BOTH boundaries of the typed-IR
        // algebra. The Display projection through `SexpWitness::Display`
        // writes only the `display` field so the rendered `got {display}`
        // suffix is byte-for-byte identical to the legacy `got: String`
        // shape.
        let err = LispError::RewriterNonList {
            keyword: "defmonitor",
            got: SexpWitness::new(SexpShape::Int, "42"),
        };
        match &err {
            LispError::RewriterNonList { keyword, got } => {
                assert_eq!(*keyword, "defmonitor");
                assert_eq!(got.shape, SexpShape::Int);
                assert_eq!(got.display, "42");
            }
            other => panic!("expected RewriterNonList, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: rewriter must return a list; got 42"
        );
    }

    #[test]
    fn rewriter_non_list_got_distinguishes_int_from_keyword_at_variant_slot() {
        // Pin the typed-shape bifurcation at the variant slot's `got`
        // slot — `42` (int) and `:foo` (keyword) BOTH route to
        // `RewriterNonList` (the rewriter returned a non-list typed-exit
        // rejection), but the typed `got.shape` slot distinguishes them
        // structurally as `SexpShape::Int` vs. `SexpShape::Keyword`.
        // Sibling pin for the same structural-shape-bifurcation property
        // `splice_outside_list_got_distinguishes_symbol_from_list_at_variant_slot`
        // pins on the typed-ENTRY-side `SpliceOutsideList` variant — the
        // same posture applied to the typed-EXIT-side rejection variant.
        // A regression that erases the typed shape (e.g., reverts to
        // `got: String`) would lose this distinction — tooling that
        // wants to surface "your rewriter returned the int `42` where a
        // kwargs list was expected" vs. "your rewriter returned the
        // keyword `:foo` where a kwargs list was expected" would have to
        // substring-grep the `display` field, brittle.
        let err_int = LispError::RewriterNonList {
            keyword: "defmonitor",
            got: SexpWitness::new(SexpShape::Int, "42"),
        };
        let err_kw = LispError::RewriterNonList {
            keyword: "defmonitor",
            got: SexpWitness::new(SexpShape::Keyword, ":foo"),
        };
        let (int_shape, kw_shape) = (
            match &err_int {
                LispError::RewriterNonList { got, .. } => got.shape,
                _ => unreachable!(),
            },
            match &err_kw {
                LispError::RewriterNonList { got, .. } => got.shape,
                _ => unreachable!(),
            },
        );
        assert_ne!(
            int_shape, kw_shape,
            "Int and Keyword witnesses must remain structurally distinct at the variant slot",
        );
        assert_eq!(int_shape, SexpShape::Int);
        assert_eq!(kw_shape, SexpShape::Keyword);
    }

    #[test]
    fn rewriter_non_list_and_typed_entry_gates_share_one_witness_primitive() {
        // Pin that ALL EIGHT Sexp-display-source `got` slots in the
        // substrate carry the SAME typed `SexpWitness` primitive — the
        // closed set of "offending inner Sexp" identities is bound by
        // ONE typed primitive across EIGHT rejection surfaces spanning
        // BOTH boundaries of the typed-IR algebra: the typed-ENTRY side
        // (seven slots — the template-gate's `,X/,@X` pair, the
        // defmacro-syntax-gate's `parse_params` walker (BOTH
        // non-symbol-param AND post-`&rest`-non-symbol-follower rejection
        // points), BOTH of the defmacro-syntax-gate's outer
        // `macro_def_from` rejection points (name-symbol AND
        // param-list), AND the outer `compile_from_sexp` typed-entry
        // gate's non-symbol-head rejection point) AND the typed-EXIT
        // side (ONE slot — `rewrite_typed`'s `Sexp::List`-contract gate
        // for the rewriter's output). With this lift EVERY
        // `Sexp::Display`-source `got` slot in the substrate is
        // structurally unified end-to-end across BOTH typed boundaries.
        // The `Option`-wrap on `MissingHeadSymbol.got` and
        // `RestParamMissingName.got` is the bifurcation between "missing
        // entirely" and "present but malformed"; the typed witness
        // rides on the `Some` arm and is structurally identical to the
        // other six variants' got slots. A regression that diverges the
        // slot type on any one variant (e.g., re-collapses
        // `RewriterNonList.got` to `String` while leaving the others
        // typed) fails-loudly here because the assignment round-trips
        // the witness across all eight slot types. Sibling pin to
        // `missing_head_symbol_and_rest_param_gate_share_one_witness_primitive`
        // — extending the typed-identity unification contract from
        // seven slots (typed-ENTRY only) to eight slots (typed-ENTRY +
        // typed-EXIT), CLOSING the contract across BOTH boundaries of
        // the typed-IR algebra (THEORY.md §II.1 invariant 1 +
        // invariant 3).
        let same_witness = SexpWitness::new(SexpShape::Int, "42");
        let rewriter_non_list = LispError::RewriterNonList {
            keyword: "defmonitor",
            got: same_witness.clone(),
        };
        let missing_head = LispError::MissingHeadSymbol {
            keyword: "defmonitor",
            got: Some(same_witness.clone()),
        };
        let rest_param_missing_name = LispError::RestParamMissingName {
            rest_position: 0,
            got: Some(same_witness.clone()),
        };
        let defmacro_non_list_params = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defmacro,
            got: same_witness.clone(),
        };
        let defmacro_non_symbol_name = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::Defmacro,
            got: same_witness.clone(),
        };
        let non_symbol_param = LispError::NonSymbolParam {
            position: 0,
            got: same_witness.clone(),
        };
        let non_symbol_target = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Unquote,
            got: same_witness.clone(),
        };
        let splice_outside = LispError::SpliceOutsideList {
            got: same_witness.clone(),
        };
        match (
            &rewriter_non_list,
            &missing_head,
            &rest_param_missing_name,
            &defmacro_non_list_params,
            &defmacro_non_symbol_name,
            &non_symbol_param,
            &non_symbol_target,
            &splice_outside,
        ) {
            (
                LispError::RewriterNonList { got: a, .. },
                LispError::MissingHeadSymbol { got: Some(b), .. },
                LispError::RestParamMissingName { got: Some(c), .. },
                LispError::DefmacroNonListParams { got: d, .. },
                LispError::DefmacroNonSymbolName { got: e, .. },
                LispError::NonSymbolParam { got: f, .. },
                LispError::NonSymbolUnquoteTarget { got: g, .. },
                LispError::SpliceOutsideList { got: h },
            ) => {
                assert_eq!(a.shape, b.shape);
                assert_eq!(b.shape, c.shape);
                assert_eq!(c.shape, d.shape);
                assert_eq!(d.shape, e.shape);
                assert_eq!(e.shape, f.shape);
                assert_eq!(f.shape, g.shape);
                assert_eq!(g.shape, h.shape);
                assert_eq!(a.display, b.display);
                assert_eq!(b.display, c.display);
                assert_eq!(c.display, d.display);
                assert_eq!(d.display, e.display);
                assert_eq!(e.display, f.display);
                assert_eq!(f.display, g.display);
                assert_eq!(g.display, h.display);
                assert_eq!(*a, same_witness);
                assert_eq!(*b, same_witness);
                assert_eq!(*c, same_witness);
                assert_eq!(*d, same_witness);
                assert_eq!(*e, same_witness);
                assert_eq!(*f, same_witness);
                assert_eq!(*g, same_witness);
                assert_eq!(*h, same_witness);
            }
            _ => unreachable!(),
        }
    }

    // ── DomainSerialize: typed-exit `to_value` structural-variant lift ──
    //
    // `serialize_to_json_err::<T>` (the `to_value`-side gate shared
    // between `register::<T>`'s registry-dispatch closure and
    // `rewrite_typed::<T>`'s round-trip prelude) was promoted from the
    // `LispError::Compile`-shaped triple to the structural
    // `LispError::DomainSerialize { keyword, message }` variant. The
    // tests below pin: (a) Display matches the legacy `"compile error
    // in {keyword}: serialize: {message}"` shape byte-for-byte across
    // representative `message` renderings (serde_json's stock
    // diagnostic, hand-crafted message); (b) the legacy substring
    // `"serialize: "` and the legacy prefix `"compile error in
    // {keyword}:"` both survive the lift unchanged for substring-grep
    // consumers; (c) kebab-case keywords thread through unchanged.
    // The `position()` floor is pinned in the main
    // `position_is_none_for_non_positional_variants` block above.

    #[test]
    fn domain_serialize_display_renders_legacy_shape_with_short_message() {
        // Hand-crafted `message` slot — the variant renders the legacy
        // `"compile error in {keyword}: serialize: {message}"` shape
        // byte-for-byte. Same wording as the pre-lift `Compile`-shaped
        // triple in `serialize_to_json_err`.
        let err = LispError::DomainSerialize {
            keyword: "defmonitor",
            message: "key must be a string".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: serialize: key must be a string"
        );
    }

    #[test]
    fn domain_serialize_display_carries_serde_json_diagnostic_unchanged() {
        // Use a real `serde_json::Error` so the test exercises a
        // representative `{e}` shape (`"expected value at line L column
        // C"`) and pins that the variant's Display rendering threads
        // the underlying diagnostic through unchanged.
        let raw = serde_json::from_str::<i32>("not-a-number")
            .expect_err("parse must fail")
            .to_string();
        let err = LispError::DomainSerialize {
            keyword: "defmonitor",
            message: raw.clone(),
        };
        assert_eq!(
            format!("{err}"),
            format!("compile error in defmonitor: serialize: {raw}")
        );
    }

    #[test]
    fn domain_serialize_display_carries_kebab_case_keyword_unchanged() {
        // Kebab-cased domain keywords (`defprocess-spec`,
        // `defalert-policy`) round-trip through the rendered
        // diagnostic's `compile error in {keyword}:` prefix unchanged.
        // A regression that camelCases the keyword fails-loudly here.
        let err = LispError::DomainSerialize {
            keyword: "defalert-policy",
            message: "expected struct".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defalert-policy: serialize: expected struct"
        );
    }

    #[test]
    fn domain_serialize_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring — `"serialize: "` — as a separate
        // assertion so a regression that drifts the wording (e.g., to
        // "to_json failed", "json encode error") fails-loudly here.
        // The substring is what consumers downstream (`tatara-check`,
        // the REPL) substring-match on; the prefix matches the legacy
        // `Compile { form: T::KEYWORD.to_string(), message:
        // format!("serialize: {e}") }` byte-for-byte.
        let err = LispError::DomainSerialize {
            keyword: "defmonitor",
            message: "boom".into(),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("serialize: "),
            "expected legacy substring in message, got: {msg}"
        );
        assert!(
            msg.contains("compile error in defmonitor:"),
            "expected legacy form-label prefix in message, got: {msg}"
        );
    }

    #[test]
    fn domain_serialize_display_empty_message_renders_bare_prefix() {
        // Edge case: an empty `message` slot renders as `"compile
        // error in {keyword}: serialize: "` — pin the trailing space
        // after the `serialize:` marker stays put. A regression that
        // strips trailing whitespace (e.g., via `.trim_end()`) or
        // drops the marker entirely fails-loudly here.
        let err = LispError::DomainSerialize {
            keyword: "defmonitor",
            message: String::new(),
        };
        assert_eq!(format!("{err}"), "compile error in defmonitor: serialize: ");
    }

    // ── KwargDeserialize: typed-entry `from_value` structural-variant lift ──
    //
    // `deserialize_err(key, err)` and `deserialize_item_err(key, idx,
    // err)` (the `from_value`-side gate shared between
    // `extract_via_serde`, `extract_optional_via_serde`, and
    // `extract_vec_via_serde`) were promoted from the
    // `LispError::Compile`-shaped triple to the structural
    // `LispError::KwargDeserialize { path: KwargPath, message }`
    // variant — the `(key: String, idx: Option<usize>)` bifurcation
    // collapsed into the typed `KwargPath` enum's `Named` vs. `Item`
    // variant identity. The tests below pin: (a) Display matches the
    // legacy `"compile error in :{key}: deserialize: {message}"` shape
    // byte-for-byte for the scalar path (`path: KwargPath::Named`); (b)
    // Display matches the indexed `"compile error in :{key}[{idx}]:
    // deserialize: {message}"` shape byte-for-byte for the per-item
    // path (`path: KwargPath::Item`); (c) the legacy substring
    // `"deserialize: "` and the legacy prefix `"compile error in :"`
    // both survive the lift unchanged for substring-grep consumers;
    // (d) kebab-case keys thread through unchanged; (e) the typed
    // `path` slot carries `KwargPath` data DIRECTLY (not as a projection
    // through a helper), structurally bound via pattern-match on the
    // typed enum's variant identity. The `position()` floor is pinned
    // in the main `position_is_none_for_non_positional_variants` block
    // above.

    #[test]
    fn kwarg_deserialize_display_scalar_path_renders_legacy_shape() {
        // `path: KwargPath::Named(_)` — scalar / `Option<T>` path. The
        // variant renders the legacy `"compile error in :{key}:
        // deserialize: {message}"` shape byte-for-byte. Same wording as
        // the pre-lift `Compile`-shaped triple in `deserialize_err`.
        let err = LispError::KwargDeserialize {
            path: KwargPath::named("level"),
            message: "unknown variant `NotASeverity`".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :level: deserialize: unknown variant `NotASeverity`"
        );
    }

    #[test]
    fn kwarg_deserialize_display_per_item_path_renders_indexed_shape() {
        // `path: KwargPath::Item { .. }` — per-item path. The variant
        // renders the legacy `"compile error in :{key}[{idx}]:
        // deserialize: {message}"` shape byte-for-byte. Same wording as
        // the pre-lift `Compile`-shaped triple in `deserialize_item_err`.
        let err = LispError::KwargDeserialize {
            path: KwargPath::item("steps", 1),
            message: "invalid type: integer `7`, expected a string".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :steps[1]: deserialize: invalid type: integer `7`, expected a string"
        );
    }

    #[test]
    fn kwarg_deserialize_display_carries_serde_json_diagnostic_unchanged() {
        // Use a real `serde_json::Error` so the test exercises a
        // representative `{e}` shape (`"expected value at line L
        // column C"`) and pins that the variant's Display rendering
        // threads the underlying diagnostic through unchanged.
        let raw = serde_json::from_str::<i32>("not-a-number")
            .expect_err("parse must fail")
            .to_string();
        let err = LispError::KwargDeserialize {
            path: KwargPath::named("count"),
            message: raw.clone(),
        };
        assert_eq!(
            format!("{err}"),
            format!("compile error in :count: deserialize: {raw}")
        );
    }

    #[test]
    fn kwarg_deserialize_display_carries_kebab_case_key_unchanged() {
        // Kebab-cased kwarg names (`notify-ref`, `wait-minutes`,
        // `window-seconds`) round-trip through the rendered diagnostic's
        // `compile error in :{key}:` prefix unchanged. A regression that
        // camelCases the key fails-loudly here.
        let err = LispError::KwargDeserialize {
            path: KwargPath::named("notify-ref"),
            message: "missing field `notify-ref`".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :notify-ref: deserialize: missing field `notify-ref`"
        );
    }

    #[test]
    fn kwarg_deserialize_display_carries_kebab_case_key_with_index_unchanged() {
        // Kebab-cased keys round-trip through the indexed path too —
        // `:notify-refs[2]` not `:notifyRefs[2]`. Pinning both paths
        // means a regression in either site (scalar or per-item) fails-
        // loudly here.
        let err = LispError::KwargDeserialize {
            path: KwargPath::item("wait-minutes", 2),
            message: "expected u64".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :wait-minutes[2]: deserialize: expected u64"
        );
    }

    #[test]
    fn kwarg_deserialize_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring — `"deserialize: "` — as a separate
        // assertion so a regression that drifts the wording (e.g., to
        // "from_json failed", "json decode error") fails-loudly here.
        // The substring is what consumers downstream (`tatara-check`,
        // the REPL) substring-match on; the prefix matches the legacy
        // `Compile { form: kwarg_form(key), message: format!("deserialize:
        // {e}") }` byte-for-byte. Both sub-modes (`KwargPath::Named`
        // AND `KwargPath::Item`) preserve the substring.
        let scalar = LispError::KwargDeserialize {
            path: KwargPath::named("level"),
            message: "boom".into(),
        };
        let scalar_msg = format!("{scalar}");
        assert!(
            scalar_msg.contains("deserialize: "),
            "expected legacy substring in scalar message, got: {scalar_msg}"
        );
        assert!(
            scalar_msg.contains("compile error in :level:"),
            "expected legacy form-label prefix in scalar message, got: {scalar_msg}"
        );

        let item = LispError::KwargDeserialize {
            path: KwargPath::item("steps", 3),
            message: "boom".into(),
        };
        let item_msg = format!("{item}");
        assert!(
            item_msg.contains("deserialize: "),
            "expected legacy substring in item message, got: {item_msg}"
        );
        assert!(
            item_msg.contains("compile error in :steps[3]:"),
            "expected indexed form-label prefix in item message, got: {item_msg}"
        );
    }

    #[test]
    fn kwarg_deserialize_display_zero_index_is_first_class() {
        // Edge case: `path: KwargPath::Item { idx: 0, .. }` must render
        // as `:steps[0]`, not `:steps` (which would collide with the
        // scalar path's `KwargPath::Named` rendering). Pin that the
        // bifurcation is by `KwargPath` variant identity, not by
        // `idx > 0`.
        let err = LispError::KwargDeserialize {
            path: KwargPath::item("steps", 0),
            message: "bad".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in :steps[0]: deserialize: bad"
        );
    }

    #[test]
    fn kwarg_deserialize_path_named_threads_typed_kwarg_path_through_variant_slot() {
        // Structural pin: the scalar / `Option<T>` path's
        // `LispError::KwargDeserialize` carries the typed
        // `KwargPath::Named(key)` value DIRECTLY in its `path` slot,
        // not a `(key: String, idx: None)` pair. Authoring tools (REPL,
        // LSP, `tatara-check`) bind on the typed enum's variant
        // identity (`KwargPath::Named(_)`) rather than substring-
        // matching the rendered `form:` prefix, parallel to how
        // `TypeMismatch.form` is bound. A regression that re-bifurcates
        // the variant into a `(key, idx: Option<usize>)` pair fails the
        // structural assertion here (the slot would no longer be a
        // typed `KwargPath`).
        let err = LispError::KwargDeserialize {
            path: KwargPath::named("level"),
            message: "boom".into(),
        };
        let LispError::KwargDeserialize {
            ref path,
            ref message,
        } = err
        else {
            panic!("expected KwargDeserialize, got {err:?}");
        };
        assert_eq!(*path, KwargPath::Named("level".into()));
        assert_eq!(message, "boom");
        assert_eq!(
            format!("{err}"),
            "compile error in :level: deserialize: boom"
        );
    }

    #[test]
    fn kwarg_deserialize_path_item_threads_typed_kwarg_path_through_variant_slot() {
        // Sibling structural pin to `…_path_named_…`: the per-item path
        // carries `KwargPath::Item { key, idx }` directly in its `path`
        // slot. The `(key, idx)` bifurcation lives inside the typed
        // enum's variant identity (`KwargPath::Named` vs.
        // `KwargPath::Item`), so the invalid sibling slot combination
        // `(key: "", idx: Some(_))` for a scalar / Optional path is
        // structurally unrepresentable in the variant's data shape.
        let err = LispError::KwargDeserialize {
            path: KwargPath::item("steps", 1),
            message: "bad".into(),
        };
        let LispError::KwargDeserialize {
            ref path,
            ref message,
        } = err
        else {
            panic!("expected KwargDeserialize, got {err:?}");
        };
        assert_eq!(
            *path,
            KwargPath::Item {
                key: "steps".into(),
                idx: 1
            }
        );
        assert_eq!(message, "bad");
        assert_eq!(
            format!("{err}"),
            "compile error in :steps[1]: deserialize: bad"
        );
    }

    #[test]
    fn kwarg_deserialize_display_prefix_matches_kwarg_path_display() {
        // End-to-end pin: the `LispError::KwargDeserialize` variant's
        // Display rendering threads its typed `path: KwargPath` slot
        // through `KwargPath`'s `Display` impl directly (via the
        // `#[error("compile error in {path}: ...")]` annotation, no
        // intermediate helper). The full rendered diagnostic MUST be
        // anchored on the canonical `KwargPath`-projected prefix
        // across BOTH variants of `KwargPath` — a regression that
        // drifts either projection (e.g., re-introducing an inline
        // `format!` literal in a `#[error(..., fmt_fn(path))]` annotation
        // that diverges from `KwargPath`'s Display arm) fails-loudly
        // here.
        let scalar = LispError::KwargDeserialize {
            path: KwargPath::named("level"),
            message: "boom".into(),
        };
        assert_eq!(
            format!("{scalar}"),
            format!(
                "compile error in {}: deserialize: boom",
                KwargPath::named("level")
            )
        );

        let item = LispError::KwargDeserialize {
            path: KwargPath::item("steps", 3),
            message: "boom".into(),
        };
        assert_eq!(
            format!("{item}"),
            format!(
                "compile error in {}: deserialize: boom",
                KwargPath::item("steps", 3)
            )
        );
    }

    // ── CompilerSpecIo: disk-persistence structural-variant lift ────
    //
    // `compiler_spec_io_err` (the helper shared by all four
    // `realize_to_disk` / `load_from_disk` call sites in
    // `compiler_spec.rs`) was promoted from the `LispError::Compile`-
    // shaped triple to the structural `LispError::CompilerSpecIo {
    // stage, message }` variant — closing the LAST
    // `LispError::Compile { ... }` construction site in
    // `tatara-lisp/src/compiler_spec.rs`. The `stage` slot is the
    // typed closed-set `CompilerSpecIoStage` enum, so the
    // (operation, stage) pair is structurally constrained — only the
    // four reachable pairs (`realize_to_disk` × {serialize, write}
    // ⊎ `load_from_disk` × {read, deserialize}) are representable
    // in the variant.
    //
    // The tests below pin: (a) Display matches the legacy `"compile
    // error in {operation}: {stage}: {message}"` shape byte-for-byte
    // across all four stages; (b) the closed-set `operation()` /
    // `label()` projections; (c) the `CompilerSpecIoStage` enum is
    // Copy + Eq + Debug (matches the `MacroDefHead` posture); (d) the
    // legacy substring `"realize_to_disk"`, `"load_from_disk"`,
    // `"serialize: "`, `"write: "`, `"read: "`, `"deserialize: "`
    // survive the lift unchanged for substring-grep consumers. The
    // `position()` floor for both representative stages is pinned in
    // the main `position_is_none_for_non_positional_variants` block
    // above.

    #[test]
    fn compiler_spec_io_stage_operation_projects_realize_for_serialize_and_write() {
        // Both `realize_to_disk` stages share the same `operation()`
        // projection. Pin the closed-set posture: the operation slot
        // of the legacy `Compile`-shaped triple is now a TYPED
        // projection from `CompilerSpecIoStage`, not an
        // independently-passed `&'static str` that could drift.
        assert_eq!(
            super::CompilerSpecIoStage::RealizeToDiskSerialize.operation(),
            "realize_to_disk"
        );
        assert_eq!(
            super::CompilerSpecIoStage::RealizeToDiskWrite.operation(),
            "realize_to_disk"
        );
    }

    #[test]
    fn compiler_spec_io_stage_operation_projects_load_for_read_and_deserialize() {
        // Both `load_from_disk` stages share the same `operation()`
        // projection. Sibling to the realize-side assertion: pins the
        // bifurcation of the closed set by `operation()` is exhaustive
        // and exactly 2-way (`realize_to_disk` ⊎ `load_from_disk`).
        assert_eq!(
            super::CompilerSpecIoStage::LoadFromDiskRead.operation(),
            "load_from_disk"
        );
        assert_eq!(
            super::CompilerSpecIoStage::LoadFromDiskDeserialize.operation(),
            "load_from_disk"
        );
    }

    #[test]
    fn compiler_spec_io_stage_label_projects_canonical_stage_strings() {
        // Each `CompilerSpecIoStage` projects to its canonical
        // `label()` — the `{stage}` slot of the legacy `"{stage}:
        // {error}"` message. Pin all four projections so a regression
        // that drifts ANY label (e.g., to "ser", "load", "decode",
        // "json-out") fails-loudly here. The four labels are the
        // surface that `tatara-check`'s diagnostic capture and the
        // REPL substring-grep on today.
        assert_eq!(
            super::CompilerSpecIoStage::RealizeToDiskSerialize.label(),
            "serialize"
        );
        assert_eq!(
            super::CompilerSpecIoStage::RealizeToDiskWrite.label(),
            "write"
        );
        assert_eq!(super::CompilerSpecIoStage::LoadFromDiskRead.label(), "read");
        assert_eq!(
            super::CompilerSpecIoStage::LoadFromDiskDeserialize.label(),
            "deserialize"
        );
    }

    #[test]
    fn compiler_spec_io_display_renders_legacy_shape_for_realize_serialize() {
        // `RealizeToDiskSerialize` — the `serde_json::to_string_pretty`
        // failure inside `realize_to_disk`. The variant renders the
        // legacy `"compile error in realize_to_disk: serialize:
        // {message}"` shape byte-for-byte — same wording as the
        // pre-lift `Compile { form: "realize_to_disk", message:
        // "serialize: {e}" }` triple.
        let err = LispError::CompilerSpecIo {
            stage: super::CompilerSpecIoStage::RealizeToDiskSerialize,
            message: "expected struct CompilerSpec".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in realize_to_disk: serialize: expected struct CompilerSpec"
        );
    }

    #[test]
    fn compiler_spec_io_display_renders_legacy_shape_for_realize_write() {
        // `RealizeToDiskWrite` — the `std::fs::write` failure inside
        // `realize_to_disk`. Pin path-uniformity across the second
        // stage of the realize-side operation: same operation prefix,
        // distinct stage label (`write` vs `serialize`).
        let err = LispError::CompilerSpecIo {
            stage: super::CompilerSpecIoStage::RealizeToDiskWrite,
            message: "No such file or directory (os error 2)".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in realize_to_disk: write: No such file or directory (os error 2)"
        );
    }

    #[test]
    fn compiler_spec_io_display_renders_legacy_shape_for_load_read() {
        // `LoadFromDiskRead` — the `std::fs::read_to_string` failure
        // inside `load_from_disk`. Pin path-uniformity across the
        // operation slot: `load_from_disk` vs `realize_to_disk` are
        // structurally distinct via the typed enum, both round-trip
        // through Display unchanged.
        let err = LispError::CompilerSpecIo {
            stage: super::CompilerSpecIoStage::LoadFromDiskRead,
            message: "No such file or directory (os error 2)".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in load_from_disk: read: No such file or directory (os error 2)"
        );
    }

    #[test]
    fn compiler_spec_io_display_renders_legacy_shape_for_load_deserialize() {
        // `LoadFromDiskDeserialize` — the `serde_json::from_str`
        // failure inside `load_from_disk`. Pin path-uniformity across
        // the fourth and final reachable stage. Together with the
        // three sibling tests, this closes the structural-completeness
        // floor of the closed-set `CompilerSpecIoStage` × Display
        // matrix — all four reachable pairs are pinned.
        let err = LispError::CompilerSpecIo {
            stage: super::CompilerSpecIoStage::LoadFromDiskDeserialize,
            message: "expected value at line 1 column 1".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in load_from_disk: deserialize: expected value at line 1 column 1"
        );
    }

    #[test]
    fn compiler_spec_io_display_carries_serde_json_diagnostic_unchanged() {
        // Use a real `serde_json::Error` so the test exercises a
        // representative `{e}` shape and pins that the variant's
        // Display rendering threads the underlying diagnostic through
        // unchanged. Same posture as `domain_serialize_display_carries_
        // serde_json_diagnostic_unchanged` and
        // `kwarg_deserialize_display_carries_serde_json_diagnostic_
        // unchanged`.
        let raw = serde_json::from_str::<i32>("not-a-number")
            .expect_err("parse must fail")
            .to_string();
        let err = LispError::CompilerSpecIo {
            stage: super::CompilerSpecIoStage::LoadFromDiskDeserialize,
            message: raw.clone(),
        };
        assert_eq!(
            format!("{err}"),
            format!("compile error in load_from_disk: deserialize: {raw}")
        );
    }

    #[test]
    fn compiler_spec_io_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring set — `"realize_to_disk"`,
        // `"load_from_disk"`, `"serialize: "`, `"write: "`,
        // `"read: "`, `"deserialize: "` — as a separate assertion so a
        // regression that drifts ANY of the six surface words (e.g.,
        // to "save", "load", "json-out", "json-in") fails-loudly here.
        // The substrings are what consumers downstream (`tatara-check`,
        // the REPL) substring-match on today; the prefix matches the
        // legacy `Compile { form: "{operation}", message: "{stage}:
        // {e}" }` byte-for-byte across all four reachable pairs.
        let realize_serialize = LispError::CompilerSpecIo {
            stage: super::CompilerSpecIoStage::RealizeToDiskSerialize,
            message: "boom".into(),
        };
        let msg = format!("{realize_serialize}");
        assert!(
            msg.contains("realize_to_disk"),
            "expected realize-side operation in message, got: {msg}"
        );
        assert!(
            msg.contains("serialize: "),
            "expected serialize-stage substring in message, got: {msg}"
        );

        let realize_write = LispError::CompilerSpecIo {
            stage: super::CompilerSpecIoStage::RealizeToDiskWrite,
            message: "boom".into(),
        };
        let msg = format!("{realize_write}");
        assert!(
            msg.contains("write: "),
            "expected write-stage substring in message, got: {msg}"
        );

        let load_read = LispError::CompilerSpecIo {
            stage: super::CompilerSpecIoStage::LoadFromDiskRead,
            message: "boom".into(),
        };
        let msg = format!("{load_read}");
        assert!(
            msg.contains("load_from_disk"),
            "expected load-side operation in message, got: {msg}"
        );
        assert!(
            msg.contains("read: "),
            "expected read-stage substring in message, got: {msg}"
        );

        let load_deserialize = LispError::CompilerSpecIo {
            stage: super::CompilerSpecIoStage::LoadFromDiskDeserialize,
            message: "boom".into(),
        };
        let msg = format!("{load_deserialize}");
        assert!(
            msg.contains("deserialize: "),
            "expected deserialize-stage substring in message, got: {msg}"
        );
    }

    #[test]
    fn compiler_spec_io_display_empty_message_renders_bare_stage_marker() {
        // Edge case: an empty `message` slot renders as `"compile
        // error in {operation}: {stage}: "` — pin the trailing space
        // after the stage marker stays put across all four pairs. A
        // regression that strips trailing whitespace (e.g., via
        // `.trim_end()`) or drops the marker entirely fails-loudly here.
        // Sibling of `domain_serialize_display_empty_message_renders_
        // bare_prefix`.
        let err = LispError::CompilerSpecIo {
            stage: super::CompilerSpecIoStage::RealizeToDiskWrite,
            message: String::new(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in realize_to_disk: write: "
        );
    }

    #[test]
    fn compiler_spec_io_stage_is_copy_and_partial_eq() {
        // Pin the closed-set posture: `CompilerSpecIoStage` derives
        // Copy + PartialEq + Eq + Debug so it composes ergonomically
        // in tests and in consumer pattern-matches (no clone-and-
        // own dance). Same posture as `MacroDefHead`. A regression
        // that drops Copy fails-loudly here (the let-binding would
        // move out instead of copy).
        let stage = super::CompilerSpecIoStage::LoadFromDiskRead;
        let copied = stage;
        assert_eq!(stage, copied);
        assert_eq!(stage, super::CompilerSpecIoStage::LoadFromDiskRead);
        assert_ne!(stage, super::CompilerSpecIoStage::RealizeToDiskWrite);
    }

    #[test]
    fn compiler_spec_io_stage_all_is_unique_and_complete() {
        // Closed-set posture: `ALL` enumerates every reachable variant
        // EXACTLY ONCE — no duplicates, no omissions. The `[Self; 4]`
        // array literal in the declaration forces the arity at compile
        // time; this test catches the orthogonal failure modes — a
        // future variant added at the type without being added to ALL
        // (silently dropped from every consumer's sweep), or a typo
        // that duplicates an entry (silently double-counted). Same
        // truth-table pinning every sibling closed-set lift in the
        // workspace uses (ExpectedKwargShape::ALL, SexpShape::ALL,
        // MacroDefHead::ALL, UnquoteForm::ALL, …).
        //
        // The asserted compound keys are the canonical (operation,
        // label) pairs the disk-persistence surface emits — the four
        // entries are the reachable cells of the `operation × label`
        // cross-product, and the four-out-of-eight partiality is the
        // load-bearing thing: `realize_to_disk × {read, deserialize}`
        // and `load_from_disk × {serialize, write}` are conceivable
        // but unreachable, and `FromStr` enforces that asymmetry at
        // the parse boundary.
        assert_eq!(super::CompilerSpecIoStage::ALL.len(), 4);
        let mut sorted: Vec<String> = super::CompilerSpecIoStage::ALL
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        sorted.sort_unstable();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(
            sorted, deduped,
            "CompilerSpecIoStage::ALL must not contain duplicates"
        );
        assert_eq!(
            sorted,
            vec![
                "load_from_disk: deserialize".to_string(),
                "load_from_disk: read".to_string(),
                "realize_to_disk: serialize".to_string(),
                "realize_to_disk: write".to_string(),
            ],
            "CompilerSpecIoStage::ALL must cover every reachable (operation, label) pair"
        );
    }

    #[test]
    fn compiler_spec_io_stage_display_matches_diagnostic_prefix() {
        // Pin standalone Display to the canonical compound key — the
        // exact substring that lands between `"compile error in "` and
        // `": {message}"` inside the LispError::CompilerSpecIo
        // rendering. Consumers that extract the (operation, label)
        // prefix from a rendered diagnostic (LSP code-actions, REPL
        // replay) round-trip the captured substring through `FromStr`
        // back into the typed variant exactly. A regression that
        // drifts either side (Display's separator, the LispError's
        // `#[error(...)]` annotation, the projection methods) fails
        // loudly here because both renderings must agree.
        for stage in super::CompilerSpecIoStage::ALL {
            let standalone = stage.to_string();
            let full = format!(
                "{}",
                LispError::CompilerSpecIo {
                    stage,
                    message: "MSG".into()
                }
            );
            let prefix = full
                .strip_prefix("compile error in ")
                .expect("legacy rendering prefix must hold");
            let extracted = prefix
                .strip_suffix(": MSG")
                .expect("legacy rendering suffix must hold");
            assert_eq!(
                extracted, standalone,
                "extracted prefix `{extracted}` must equal standalone Display `{standalone}`"
            );
        }
    }

    #[test]
    fn compiler_spec_io_stage_compound_key_round_trips_through_from_str() {
        // Bidirectional `Display` ↔ `FromStr` contract: for every
        // variant in ALL, `stage.to_string().parse() == Ok(stage)`. A
        // regression that drifts the (variant, compound-key) pairing
        // at the `Display` impl (typo, missing separator, swapped
        // projection order) OR at the `FromStr` decode body (off-by-
        // one, missing variant in the sweep) fails-loudly here. The
        // canonical-key site is singular (`Display` projects through
        // `operation()` and `label()`) so the round-trip is the only
        // way the typed surface and the rendered diagnostic literal
        // can drift apart — pinning it here means they cannot. Mirror
        // of `expected_kwarg_shape_label_round_trips_through_from_str`
        // and every sibling closed-set round-trip in the workspace —
        // the difference is the compound-key shape rather than a
        // single label.
        for stage in super::CompilerSpecIoStage::ALL {
            let key = stage.to_string();
            let parsed: super::CompilerSpecIoStage = key
                .parse()
                .expect("every ALL variant's compound key must round-trip through FromStr");
            assert_eq!(
                parsed, stage,
                "FromStr({key}) must round-trip to the same variant"
            );
        }
    }

    #[test]
    fn compiler_spec_io_stage_from_str_rejects_partial_and_unreachable_keys() {
        // The compound-key shape is load-bearing: `FromStr` rejects
        // partial keys (one of the two projection slots alone),
        // separator-less inputs, AND the four conceivable-but-
        // unreachable cross-product pairs that the disk-persistence
        // surface does NOT emit. The partial-rejection turns the
        // call-site invariant ("only the four call sites in
        // `compiler_spec.rs` construct stages, each pairs the correct
        // operation with the correct stage") into a parse-boundary
        // invariant ("the four reachable pairs are structurally
        // distinct from the four unreachable ones").
        //
        // Without this guard a future LSP that captures the prefix
        // `"load_from_disk: write"` from a hand-crafted (corrupted)
        // log and replays it through `FromStr` would silently round-
        // trip an unreachable identity into the typed enum.
        for partial in [
            "serialize",
            "write",
            "read",
            "deserialize",
            "realize_to_disk",
            "load_from_disk",
            "",
        ] {
            partial.parse::<super::CompilerSpecIoStage>().expect_err(
                "partial key (one projection slot alone, or empty) must NOT decode to a variant",
            );
        }
        for unreachable in [
            "realize_to_disk: read",
            "realize_to_disk: deserialize",
            "load_from_disk: serialize",
            "load_from_disk: write",
        ] {
            unreachable
                .parse::<super::CompilerSpecIoStage>()
                .expect_err(
                "conceivable-but-unreachable (operation, label) pair must NOT decode to a variant",
            );
        }
        // Sanity: each reachable pair still decodes.
        assert_eq!(
            "realize_to_disk: serialize"
                .parse::<super::CompilerSpecIoStage>()
                .unwrap(),
            super::CompilerSpecIoStage::RealizeToDiskSerialize
        );
        assert_eq!(
            "load_from_disk: deserialize"
                .parse::<super::CompilerSpecIoStage>()
                .unwrap(),
            super::CompilerSpecIoStage::LoadFromDiskDeserialize
        );
    }

    #[test]
    fn unknown_compiler_spec_io_stage_carries_offending_input_verbatim() {
        // Operator-facing diagnostic contract: the offending input
        // lands in the typed error verbatim — no normalization, no
        // case-folding, no truncation. Pin the exact `#[error(...)]`
        // rendering AND the typed `.0` field projection so a future
        // refactor that normalizes (e.g. `.to_lowercase()`) before
        // building the error or that drops the input fails-loudly
        // here. Symmetric to every sibling `Unknown*` carrier in the
        // workspace (`UnknownExpectedKwargShape`, `UnknownSexpShape`,
        // `UnknownMacroDefHead`, …).
        let err: super::UnknownCompilerSpecIoStage = "Realize_to_disk: serialize"
            .parse::<super::CompilerSpecIoStage>()
            .expect_err("capitalized operation must NOT decode — keys are byte-equal");
        assert_eq!(err.0, "Realize_to_disk: serialize");
        assert_eq!(
            format!("{err}"),
            "unknown compiler spec io stage: Realize_to_disk: serialize"
        );

        let err: super::UnknownCompilerSpecIoStage = "load_from_disk: write"
            .parse::<super::CompilerSpecIoStage>()
            .expect_err("unreachable cross-product pair must NOT decode");
        assert_eq!(err.0, "load_from_disk: write");
        assert_eq!(
            format!("{err}"),
            "unknown compiler spec io stage: load_from_disk: write"
        );

        let err: super::UnknownCompilerSpecIoStage = ""
            .parse::<super::CompilerSpecIoStage>()
            .expect_err("empty input must NOT decode to a CompilerSpecIoStage");
        assert_eq!(err.0, "");
        assert_eq!(format!("{err}"), "unknown compiler spec io stage: ");
    }

    // ── TemplateInvariantKind + TemplateInvariant variant ───────────
    //
    // Closed-set posture for the bytecode-runtime invariant surface in
    // `macro_expand.rs::apply_compiled`. The index payload of the Subst /
    // Splice gates lives INSIDE the variants (`SubstBadIndex(usize)` /
    // `SpliceBadIndex(usize)`), so the invalid combination "stack-gate
    // kind with an op-index" (e.g. `EndListEmptyStack` carrying a
    // `usize`) is structurally unrepresentable. Display matches the
    // legacy `Compile`-shaped diagnostic byte-for-byte through the
    // `TemplateInvariantKind::message()` projection so authoring-tool
    // substring greps see no drift across the lift.

    #[test]
    fn template_invariant_kind_message_for_subst_bad_idx() {
        // `SubstBadIndex(idx)` projects to the canonical
        // `"compiled template referenced bad param index {idx}"`
        // shape — byte-for-byte equivalent to the pre-lift inline
        // `format!()` at the Subst gate.
        assert_eq!(
            super::TemplateInvariantKind::SubstBadIndex(99).message(),
            "compiled template referenced bad param index 99"
        );
        assert_eq!(
            super::TemplateInvariantKind::SubstBadIndex(0).message(),
            "compiled template referenced bad param index 0"
        );
    }

    #[test]
    fn template_invariant_kind_message_for_splice_bad_idx() {
        // `SpliceBadIndex(idx)` projects to the canonical
        // `"compiled template referenced bad splice index {idx}"`
        // shape — byte-for-byte equivalent to the pre-lift inline
        // `format!()` at the Splice gate. Distinct word (`splice` vs
        // `param`) keeps the two gates legible in diagnostic output.
        assert_eq!(
            super::TemplateInvariantKind::SpliceBadIndex(42).message(),
            "compiled template referenced bad splice index 42"
        );
    }

    #[test]
    fn template_invariant_kind_message_for_endlist_empty_stack() {
        // `EndListEmptyStack` projects to the canonical static-string
        // shape — no dynamic payload, no `format!()` overhead. The
        // pre-lift inline `&'static str` literal at the EndList gate
        // is preserved verbatim.
        assert_eq!(
            super::TemplateInvariantKind::EndListEmptyStack.message(),
            "compiled template: EndList with empty stack"
        );
    }

    #[test]
    fn template_invariant_kind_message_for_final_no_value() {
        // `FinalNoValue` projects to the canonical static-string
        // shape for the post-loop final-pop gate. Preserves the
        // pre-lift inline `&'static str` literal verbatim.
        assert_eq!(
            super::TemplateInvariantKind::FinalNoValue.message(),
            "compiled template produced no value"
        );
    }

    #[test]
    fn template_invariant_display_renders_legacy_compile_shape_for_subst_bad_idx() {
        // End-to-end through the `LispError` Display impl — pins the
        // rendered diagnostic byte-for-byte: `"compile error in
        // {macro_name}: compiled template referenced bad param index
        // {idx}"`. Authoring tools that substring-grep the rendered
        // diagnostic (`tatara-check`, REPL substring-greps) see no
        // drift across the lift from the pre-lift `Compile { form:
        // macro_name, message: format!(...) }` shape.
        let err = LispError::TemplateInvariant {
            macro_name: "test-macro".into(),
            kind: super::TemplateInvariantKind::SubstBadIndex(99),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in test-macro: compiled template referenced bad param index 99"
        );
    }

    #[test]
    fn template_invariant_display_renders_legacy_compile_shape_for_splice_bad_idx() {
        // Sibling Display test for the Splice gate. Pins the message
        // byte-for-byte: `"compile error in call-macro: compiled
        // template referenced bad splice index 42"`.
        let err = LispError::TemplateInvariant {
            macro_name: "call-macro".into(),
            kind: super::TemplateInvariantKind::SpliceBadIndex(42),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in call-macro: compiled template referenced bad splice index 42"
        );
    }

    #[test]
    fn template_invariant_display_renders_legacy_compile_shape_for_endlist() {
        // Sibling Display test for the EndList gate. Pins the static-
        // message byte-for-byte: `"compile error in wrap: compiled
        // template: EndList with empty stack"`. Even though this gate
        // is currently guarded by `last_mut().unwrap()` and not
        // reachable through any single CompiledTemplate, the structural
        // variant carries the canonical message verbatim — defensive
        // against future changes to the stack discipline.
        let err = LispError::TemplateInvariant {
            macro_name: "wrap".into(),
            kind: super::TemplateInvariantKind::EndListEmptyStack,
        };
        assert_eq!(
            format!("{err}"),
            "compile error in wrap: compiled template: EndList with empty stack"
        );
    }

    #[test]
    fn template_invariant_display_renders_legacy_compile_shape_for_final_no_value() {
        // Sibling Display test for the final-no-value gate. Pins the
        // static-message byte-for-byte: `"compile error in id: compiled
        // template produced no value"`. Closes the structural-
        // completeness floor of the closed-set `TemplateInvariantKind`
        // × Display matrix — all four reachable kinds are pinned.
        let err = LispError::TemplateInvariant {
            macro_name: "id".into(),
            kind: super::TemplateInvariantKind::FinalNoValue,
        };
        assert_eq!(
            format!("{err}"),
            "compile error in id: compiled template produced no value"
        );
    }

    #[test]
    fn template_invariant_display_preserves_legacy_substring_for_message_grep() {
        // Pin the legacy substring set — `"compiled template"`,
        // `"bad param index"`, `"bad splice index"`, `"EndList with
        // empty stack"`, `"produced no value"` — as a separate
        // assertion so a regression that drifts ANY of the four
        // surface words (e.g., to "invalid", "missing", "no result")
        // fails-loudly here. The substrings are what consumers
        // downstream (`tatara-check`, REPL) substring-match on today.
        let subst = LispError::TemplateInvariant {
            macro_name: "m".into(),
            kind: super::TemplateInvariantKind::SubstBadIndex(0),
        };
        let msg = format!("{subst}");
        assert!(
            msg.contains("compiled template"),
            "expected `compiled template` prefix, got: {msg}"
        );
        assert!(
            msg.contains("bad param index"),
            "expected `bad param index` substring, got: {msg}"
        );

        let splice = LispError::TemplateInvariant {
            macro_name: "m".into(),
            kind: super::TemplateInvariantKind::SpliceBadIndex(0),
        };
        let msg = format!("{splice}");
        assert!(
            msg.contains("bad splice index"),
            "expected `bad splice index` substring, got: {msg}"
        );

        let endlist = LispError::TemplateInvariant {
            macro_name: "m".into(),
            kind: super::TemplateInvariantKind::EndListEmptyStack,
        };
        let msg = format!("{endlist}");
        assert!(
            msg.contains("EndList with empty stack"),
            "expected `EndList with empty stack` substring, got: {msg}"
        );

        let final_nv = LispError::TemplateInvariant {
            macro_name: "m".into(),
            kind: super::TemplateInvariantKind::FinalNoValue,
        };
        let msg = format!("{final_nv}");
        assert!(
            msg.contains("produced no value"),
            "expected `produced no value` substring, got: {msg}"
        );
    }

    #[test]
    fn template_invariant_kind_is_copy_and_partial_eq() {
        // Pin the closed-set posture: `TemplateInvariantKind` derives
        // Copy + PartialEq + Eq + Debug so it composes ergonomically
        // in tests and in consumer pattern-matches (no clone-and-own
        // dance). Same posture as `CompilerSpecIoStage` and
        // `MacroDefHead`. A regression that drops Copy fails-loudly
        // here (the let-binding would move out instead of copy).
        let kind = super::TemplateInvariantKind::SubstBadIndex(7);
        let copied = kind;
        assert_eq!(kind, copied);
        assert_eq!(kind, super::TemplateInvariantKind::SubstBadIndex(7));
        assert_ne!(kind, super::TemplateInvariantKind::SubstBadIndex(8));
        assert_ne!(kind, super::TemplateInvariantKind::SpliceBadIndex(7));
        assert_ne!(kind, super::TemplateInvariantKind::EndListEmptyStack);
    }

    #[test]
    fn template_invariant_kind_index_payload_is_structurally_scoped_to_index_carrying_variants() {
        // The closed-set invariant: only `SubstBadIndex` and
        // `SpliceBadIndex` carry a `usize` payload; `EndListEmptyStack`
        // and `FinalNoValue` are bare. This is enforced by the variant
        // shape itself — there is no way to construct
        // `EndListEmptyStack(7)` because the variant has no fields.
        // This test pins the structural shape: the four reachable
        // failure modes split 2+2 into "index-carrying" and "bare".
        // A regression that adds a payload to the bare variants (or
        // strips it from the index-carrying ones) fails to compile,
        // making this test redundant — but the test documents the
        // shape for readers walking the closed set.
        match super::TemplateInvariantKind::SubstBadIndex(5) {
            super::TemplateInvariantKind::SubstBadIndex(idx) => assert_eq!(idx, 5),
            other => panic!("expected SubstBadIndex, got {other:?}"),
        }
        match super::TemplateInvariantKind::EndListEmptyStack {
            super::TemplateInvariantKind::EndListEmptyStack => {}
            other => panic!("expected EndListEmptyStack, got {other:?}"),
        }
    }

    // --- MacroDefHead typed-slot lift (the closed-set promotion) ---
    //
    // The next eight tests pin the typed-slot promotion that closes
    // the three-times rule across the `LispError::Defmacro*` family.
    // Before this lift the three variants' `head` slot was
    // `&'static str`, projected from a `MacroDefHead` via
    // `head.keyword()` at the helper boundary; consumers had to
    // substring-compare against three string literals to recognize
    // a head identity. After the lift the slot IS the typed enum,
    // so authoring tools (REPL, LSP, `tatara-check`) pattern-match
    // on `MacroDefHead::Defmacro` etc. directly — same posture as
    // `CompilerSpecIoStage` for `LispError::CompilerSpecIo` and
    // `TemplateInvariantKind` for `LispError::TemplateInvariant`.

    #[test]
    fn defmacro_arity_head_slot_is_macro_def_head_not_static_str() {
        // Pin that the `head` slot of `LispError::DefmacroArity` is
        // `MacroDefHead` (the typed closed-set enum), not `&'static
        // str`. A regression that reverts the slot to `&'static str`
        // breaks the typed binding here at compile time; a
        // construction with a stringly-typed head would fail to
        // construct. This test is the structural-completeness pin
        // for the typed-slot promotion, parallel to how
        // `compiler_spec_io_carries_typed_stage_field` (if it
        // existed) would pin `LispError::CompilerSpecIo.stage`.
        let err = LispError::DefmacroArity {
            head: MacroDefHead::Defmacro,
            arity: 1,
        };
        match err {
            LispError::DefmacroArity { head, arity } => {
                assert_eq!(head, MacroDefHead::Defmacro);
                assert_eq!(arity, 1);
            }
            other => panic!("expected DefmacroArity, got {other:?}"),
        }
    }

    #[test]
    fn defmacro_non_symbol_name_head_slot_is_macro_def_head_not_static_str() {
        // Sibling pin of `defmacro_arity_head_slot_is_macro_def_head_not_static_str`
        // for the `LispError::DefmacroNonSymbolName` variant. The
        // `head` slot carries `MacroDefHead` directly so consumers
        // bind on variant identity (`MacroDefHead::DefpointTemplate`)
        // instead of substring-matching the rendered diagnostic.
        let err = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::DefpointTemplate,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        match err {
            LispError::DefmacroNonSymbolName { head, got } => {
                assert_eq!(head, MacroDefHead::DefpointTemplate);
                assert_eq!(got.shape, SexpShape::Int);
                assert_eq!(got.display, "5");
            }
            other => panic!("expected DefmacroNonSymbolName, got {other:?}"),
        }
    }

    #[test]
    fn defmacro_non_list_params_head_slot_is_macro_def_head_not_static_str() {
        // Sibling pin of `defmacro_arity_head_slot_is_macro_def_head_not_static_str`
        // for the `LispError::DefmacroNonListParams` variant. The
        // `head` slot carries `MacroDefHead` directly so consumers
        // pattern-match on `MacroDefHead::Defcheck` etc. for the
        // workspace-coherence authoring surface's third head
        // keyword.
        let err = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defcheck,
            got: SexpWitness::new(SexpShape::Int, "7"),
        };
        match err {
            LispError::DefmacroNonListParams { head, got } => {
                assert_eq!(head, MacroDefHead::Defcheck);
                assert_eq!(got.shape, SexpShape::Int);
                assert_eq!(got.display, "7");
            }
            other => panic!("expected DefmacroNonListParams, got {other:?}"),
        }
    }

    #[test]
    fn macro_def_head_display_renders_canonical_keyword_for_each_variant() {
        // Pin `MacroDefHead`'s Display impl — it must project through
        // `keyword()` so the `#[error(...)]` annotation on each
        // `LispError::Defmacro*` variant renders the canonical
        // `&'static str` literal byte-for-byte. The Display
        // bidirection is `MacroDefHead → &'static str`; the inverse
        // (`&str → Option<MacroDefHead>`) lives in `from_keyword`.
        // Together the two close the bidirectional projection on the
        // closed set.
        assert_eq!(format!("{}", MacroDefHead::Defmacro), "defmacro");
        assert_eq!(
            format!("{}", MacroDefHead::DefpointTemplate),
            "defpoint-template"
        );
        assert_eq!(format!("{}", MacroDefHead::Defcheck), "defcheck");
    }

    #[test]
    fn defmacro_arity_display_renders_legacy_prefix_via_macro_def_head_display() {
        // End-to-end through `LispError`'s Display: the typed `head:
        // MacroDefHead` slot projects to the canonical `&'static
        // str` literal at render time via `MacroDefHead`'s Display
        // impl, so the rendered diagnostic is byte-for-byte
        // identical to the pre-lift `head: &'static str` shape.
        // Authoring tools that substring-match on `"compile error
        // in defmacro:"` see no drift across the typed-slot
        // promotion.
        let err = LispError::DefmacroArity {
            head: MacroDefHead::Defmacro,
            arity: 2,
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defmacro: (defmacro name (params) body) required \
             (got 2 elements, need 4)"
        );
    }

    #[test]
    fn defmacro_non_symbol_name_display_renders_via_macro_def_head_display_for_defpoint_template() {
        // Sibling end-to-end test for the `defpoint-template` head:
        // pins that the typed-slot promotion preserves the
        // K8s-as-processes authoring surface's diagnostic shape
        // byte-for-byte. A regression that drifts `MacroDefHead`'s
        // Display impl (e.g. returns `"DefpointTemplate"` instead of
        // `"defpoint-template"`) fails-loudly here.
        let err = LispError::DefmacroNonSymbolName {
            head: MacroDefHead::DefpointTemplate,
            got: SexpWitness::new(SexpShape::Keyword, ":foo"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defpoint-template: expected name symbol, got :foo"
        );
    }

    #[test]
    fn defmacro_non_list_params_display_renders_via_macro_def_head_display_for_defcheck() {
        // Sibling end-to-end test for the `defcheck` head: pins that
        // the typed-slot promotion preserves the workspace-coherence
        // authoring surface's diagnostic shape byte-for-byte.
        let err = LispError::DefmacroNonListParams {
            head: MacroDefHead::Defcheck,
            got: SexpWitness::new(SexpShape::Symbol, "x"),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in defcheck: expected param list, got x"
        );
    }

    #[test]
    fn macro_def_head_is_copy_and_partial_eq_for_pattern_match_ergonomics() {
        // Pin that `MacroDefHead` derives `Copy + PartialEq + Eq +
        // Debug + Clone` — the posture every closed-set typed enum
        // in this module shares (`CompilerSpecIoStage`,
        // `TemplateInvariantKind`). Copy lets consumers pattern-match
        // on the variant without explicit cloning; `PartialEq + Eq`
        // makes `assert_eq!` and `matches!` ergonomic; `Debug` makes
        // the `other => panic!("got {other:?}")` shape ergonomic at
        // assertion sites. A regression that drops any of these
        // derives breaks compilation here.
        let h = MacroDefHead::Defmacro;
        let h_copy: MacroDefHead = h; // Copy
        assert_eq!(h, h_copy); // PartialEq
        assert!(matches!(h, MacroDefHead::Defmacro)); // exhaustive match
        let _: String = format!("{h:?}"); // Debug
    }

    // --- UnquoteForm typed-slot lift (the closed-set promotion) ---
    //
    // The next six tests pin the typed-slot promotion that closes the
    // three-times rule across `LispError::UnboundTemplateVar` /
    // `LispError::NonSymbolUnquoteTarget` — the two template-marker
    // rejection variants for the no-evaluator template language.
    // Before this lift each variant's `prefix` slot was `&'static
    // str`, set from the literal `","` / `",@"` at four (Unbound) +
    // four (NonSymbol) call sites; consumers had to substring-compare
    // against two string literals to recognize the marker identity.
    // After the lift the slot IS the typed `UnquoteForm` enum, so
    // authoring tools (REPL, LSP, `tatara-check`) pattern-match on
    // `UnquoteForm::Splice` etc. directly — same posture as
    // `MacroDefHead` for `LispError::Defmacro*`, `CompilerSpecIoStage`
    // for `LispError::CompilerSpecIo`, `TemplateInvariantKind` for
    // `LispError::TemplateInvariant`.

    #[test]
    fn unbound_template_var_prefix_slot_is_unquote_form_not_static_str() {
        // Pin that the `prefix` slot of `LispError::UnboundTemplateVar`
        // is `UnquoteForm` (the typed closed-set enum), not `&'static
        // str`. A regression that reverts the slot to `&'static str`
        // breaks the typed binding here at compile time; a
        // construction with a stringly-typed prefix would fail to
        // construct. This test is the structural-completeness pin
        // for the typed-slot promotion, parallel to how
        // `defmacro_arity_head_slot_is_macro_def_head_not_static_str`
        // pins `LispError::DefmacroArity.head`.
        let err = LispError::UnboundTemplateVar {
            prefix: UnquoteForm::Unquote,
            name: "xs".into(),
            hint: None,
        };
        match err {
            LispError::UnboundTemplateVar { prefix, name, hint } => {
                assert_eq!(prefix, UnquoteForm::Unquote);
                assert_eq!(name, "xs");
                assert_eq!(hint, None);
            }
            other => panic!("expected UnboundTemplateVar, got {other:?}"),
        }
    }

    #[test]
    fn non_symbol_unquote_target_prefix_slot_is_unquote_form_not_static_str() {
        // Sibling pin for `LispError::NonSymbolUnquoteTarget`. The
        // `prefix` slot carries `UnquoteForm` directly so consumers
        // bind on variant identity (`UnquoteForm::Splice`) instead
        // of substring-matching the rendered diagnostic. Together
        // with `unbound_template_var_prefix_slot_is_unquote_form_not_static_str`,
        // the two pin the typed-slot promotion across ALL template-
        // marker rejection variants — the two reachable rejection
        // identities (`UnboundTemplateVar`, `NonSymbolUnquoteTarget`)
        // share ONE typed marker identity.
        let err = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Splice,
            got: SexpWitness::new(SexpShape::List, "(list 1 2)"),
        };
        match err {
            LispError::NonSymbolUnquoteTarget { prefix, got } => {
                assert_eq!(prefix, UnquoteForm::Splice);
                assert_eq!(got.shape, SexpShape::List);
                assert_eq!(got.display, "(list 1 2)");
            }
            other => panic!("expected NonSymbolUnquoteTarget, got {other:?}"),
        }
    }

    #[test]
    fn unquote_form_marker_projects_canonical_literal_for_each_variant() {
        // Pin `UnquoteForm::marker()` — it must project each variant
        // to the canonical `&'static str` literal byte-for-byte. The
        // projection feeds the `#[error(...)]` annotation on
        // `LispError::UnboundTemplateVar` /
        // `LispError::NonSymbolUnquoteTarget` via the Display impl,
        // and the `unbound_hint_suffix` helper's `prefix.marker()`
        // call site. A regression that drifts the literal (e.g.,
        // returns `"un"` instead of `","`) fails-loudly here.
        assert_eq!(UnquoteForm::Unquote.marker(), ",");
        assert_eq!(UnquoteForm::Splice.marker(), ",@");
    }

    #[test]
    fn unquote_form_display_renders_canonical_marker_for_each_variant() {
        // Pin `UnquoteForm`'s Display impl — it must project through
        // `marker()` so the `#[error(...)]` annotation on each
        // affected `LispError::*` variant renders the canonical
        // `&'static str` literal byte-for-byte. Same posture as
        // `MacroDefHead`'s Display impl (which projects through
        // `keyword()`).
        assert_eq!(format!("{}", UnquoteForm::Unquote), ",");
        assert_eq!(format!("{}", UnquoteForm::Splice), ",@");
    }

    #[test]
    fn unbound_template_var_display_renders_canonical_marker_for_each_variant() {
        // End-to-end through `LispError`'s Display: the typed `prefix:
        // UnquoteForm` slot projects to the canonical `&'static str`
        // literal at render time via `UnquoteForm`'s Display impl,
        // so the rendered diagnostic is byte-for-byte identical to
        // the pre-lift `prefix: &'static str` shape. Authoring tools
        // that substring-match on `,xs` / `,@xs` see no drift across
        // the typed-slot promotion. Paired with
        // `non_symbol_unquote_target_display_renders_canonical_marker_for_each_variant`
        // to pin the lift end-to-end on BOTH affected variants.
        let unquote = LispError::UnboundTemplateVar {
            prefix: UnquoteForm::Unquote,
            name: "xs".into(),
            hint: None,
        };
        assert_eq!(format!("{unquote}"), "compile error in ,xs: unbound");

        let splice = LispError::UnboundTemplateVar {
            prefix: UnquoteForm::Splice,
            name: "argz".into(),
            hint: Some("args".into()),
        };
        assert_eq!(
            format!("{splice}"),
            "compile error in ,@argz: unbound; did you mean ,@args?"
        );
    }

    #[test]
    fn non_symbol_unquote_target_display_renders_canonical_marker_for_each_variant() {
        // Sibling end-to-end pin for `LispError::NonSymbolUnquoteTarget`.
        // Pins that the typed-slot promotion preserves the
        // template-marker diagnostic shape byte-for-byte for BOTH
        // variants of `UnquoteForm`.
        let unquote = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Unquote,
            got: SexpWitness::new(SexpShape::List, "(list 1 2)"),
        };
        assert_eq!(
            format!("{unquote}"),
            "compile error in ,: expected symbol, got (list 1 2)"
        );

        let splice = LispError::NonSymbolUnquoteTarget {
            prefix: UnquoteForm::Splice,
            got: SexpWitness::new(SexpShape::Int, "5"),
        };
        assert_eq!(
            format!("{splice}"),
            "compile error in ,@: expected symbol, got 5"
        );
    }

    #[test]
    fn unquote_form_is_copy_and_partial_eq_for_pattern_match_ergonomics() {
        // Pin that `UnquoteForm` derives `Copy + PartialEq + Eq +
        // Debug + Clone` — the posture every closed-set typed enum
        // in this module shares (`MacroDefHead`, `CompilerSpecIoStage`,
        // `TemplateInvariantKind`). Copy lets consumers pattern-match
        // on the variant without explicit cloning; `PartialEq + Eq`
        // makes `assert_eq!` and `matches!` ergonomic; `Debug` makes
        // the `other => panic!("got {other:?}")` shape ergonomic at
        // assertion sites. A regression that drops any of these
        // derives breaks compilation here.
        let f = UnquoteForm::Splice;
        let f_copy: UnquoteForm = f; // Copy
        assert_eq!(f, f_copy); // PartialEq
        assert!(matches!(f, UnquoteForm::Splice)); // exhaustive match
        assert_ne!(f, UnquoteForm::Unquote); // Eq/Ne
        let _: String = format!("{f:?}"); // Debug
    }

    #[test]
    fn kwarg_path_named_display_renders_legacy_colon_key_literal() {
        // `KwargPath::Named(":<key>")` Display must render the literal
        // `:<key>` byte-for-byte equivalent to the pre-lift
        // `format!(":{key}")` inline literal in `kwarg_form`. The
        // canonical literal lives in ONE place (the Display impl), so
        // a regression that drifts the prefix (e.g., to `key:` or
        // `<key>:`) fails-loudly here AND breaks every
        // `LispError::TypeMismatch.form` consumer that depends on the
        // `:<key>` shape (the substrate's hot path).
        assert_eq!(format!("{}", KwargPath::named("threshold")), ":threshold");
    }

    #[test]
    fn kwarg_path_item_display_renders_legacy_colon_key_bracket_idx_literal() {
        // `KwargPath::Item { key, idx }` Display must render
        // `:<key>[<idx>]` byte-for-byte equivalent to the pre-lift
        // `format!(":{key}[{idx}]")` inline literal in `kwarg_item_form`.
        // The bracketed-index suffix is what `extract_string_list`'s
        // per-item failure path emits; a regression that drifts the
        // bracket-shape (e.g., to `:steps.1` or `:steps#1`) breaks every
        // LSP underline that depends on the bracket shape.
        assert_eq!(format!("{}", KwargPath::item("steps", 1)), ":steps[1]");
    }

    #[test]
    fn kwarg_path_slot_display_renders_legacy_kwargs_bracket_idx_literal() {
        // `KwargPath::Slot(idx)` Display must render `kwargs[<idx>]`
        // byte-for-byte equivalent to the pre-lift
        // `format!("kwargs[{idx}]")` inline literal in `kwargs_pos_form`.
        // The `kwargs` prefix (no leading colon) is what `parse_kwargs`'s
        // "this-position-must-be-a-keyword" gate emits when the slot
        // failed BEFORE a key was known; the slot shape is structurally
        // distinct from the named-kwarg shape (`:<key>` vs `kwargs[i]`)
        // so consumers can bifurcate on path identity.
        assert_eq!(format!("{}", KwargPath::Slot(0)), "kwargs[0]");
    }

    #[test]
    fn kwarg_path_named_carries_kebab_case_keys_unchanged() {
        // Kebab-cased kwarg names (`:notify-ref`, `:window-seconds`)
        // round-trip through the Display unchanged — the path shape
        // doesn't transform the key, it just wraps it in the `:<…>`
        // prefix. Pinning this contract means a regression that
        // camelCases or lowercases the key in the rendered prefix
        // fails-loudly here.
        assert_eq!(format!("{}", KwargPath::named("notify-ref")), ":notify-ref");
        assert_eq!(
            format!("{}", KwargPath::item("window-seconds", 3)),
            ":window-seconds[3]"
        );
    }

    #[test]
    fn kwarg_path_is_clone_and_partial_eq_for_pattern_match_ergonomics() {
        // `KwargPath` derives Clone + Debug + PartialEq + Eq so that
        // pattern-matching call sites (REPL diagnostic capture,
        // `tatara-check`'s failure clustering, an LSP that surfaces
        // "your `:steps[3]` failed" with structural binding) compare
        // by reference cheaply AND inhabit the same kind of test
        // assertion shape as `MacroDefHead`, `UnquoteForm`,
        // `CompilerSpecIoStage`, and `TemplateInvariantKind`. `Copy`
        // is intentionally NOT derived because `String` is not `Copy`
        // — the owned key payload is the load-bearing property of the
        // typed-slot promotion onto `LispError::TypeMismatch.form`. A
        // regression that drops any of the retained derives or that
        // re-adds `Copy` breaks compilation here.
        let p = KwargPath::item("steps", 2);
        let p_clone = p.clone(); // Clone
        assert_eq!(p, p_clone); // PartialEq
        assert!(matches!(p, KwargPath::Item { idx: 2, .. })); // exhaustive match
        assert_ne!(p, KwargPath::Slot(2)); // Eq/Ne — Item and Slot are distinct path identities
        let _: String = format!("{p:?}"); // Debug
    }

    #[test]
    fn kwarg_path_named_and_slot_have_distinct_display_shapes() {
        // The bifurcation between named-kwarg failures (`:<key>`) and
        // pre-key slot failures (`kwargs[<idx>]`) is structural — same
        // failure surface (kwargs gate), different path identity. Pin
        // the structural-distinctness: even at the rendered-string
        // level the two shapes don't collide. Two consumers depend on
        // this:
        //   1. `tatara-check`'s diagnostic capture, which groups by
        //      path prefix — a slot failure must NOT be confused with
        //      a `:kwargs`-keyed named-kwarg failure.
        //   2. An LSP's structural binding — the `KwargPath::Slot`
        //      identity says "we don't know which kwarg yet"; the
        //      `KwargPath::Named` identity says "we know the kwarg
        //      identifier and it's this".
        let named = format!("{}", KwargPath::named("kwargs"));
        let slot = format!("{}", KwargPath::Slot(0));
        assert_eq!(named, ":kwargs");
        assert_eq!(slot, "kwargs[0]");
        assert_ne!(named, slot);
    }

    #[test]
    fn type_mismatch_form_carries_typed_kwarg_path_named_through_variant_slot() {
        // After the typed-slot promotion, `LispError::TypeMismatch.form`
        // is `KwargPath` — owned, structurally bound to the closed-set
        // typed enum. Consumers (REPL, LSP, `tatara-check`) pattern-match
        // on the variant identity `KwargPath::Named(_)` directly rather
        // than substring-matching a rendered prefix. Pin the structural
        // binding AND the Display projection so the byte-for-byte
        // rendering contract is anchored from both angles. A regression
        // that re-introduces a String-shaped form (collapsing the typed
        // enum back into a free-form label) fails-loudly here.
        let err = LispError::TypeMismatch {
            form: KwargPath::named("threshold"),
            expected: ExpectedKwargShape::Number,
            got: SexpShape::String,
        };
        match &err {
            LispError::TypeMismatch { form, .. } => {
                assert_eq!(*form, KwargPath::Named("threshold".into()));
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: expected number, got string"
        );
    }

    #[test]
    fn type_mismatch_form_carries_typed_kwarg_path_item_through_variant_slot() {
        // Sibling pin to `…_named_…` for the per-item path. The
        // `KwargPath::Item { key, idx }` shape names the offending kwarg
        // AND the failing item index in one structural variant; the
        // bracketed `:<key>[<idx>]` rendering is unchanged.
        let err = LispError::TypeMismatch {
            form: KwargPath::item("steps", 3),
            expected: ExpectedKwargShape::String,
            got: SexpShape::Int,
        };
        match &err {
            LispError::TypeMismatch { form, .. } => {
                assert_eq!(
                    *form,
                    KwargPath::Item {
                        key: "steps".into(),
                        idx: 3
                    }
                );
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in :steps[3]: expected string, got int"
        );
    }

    #[test]
    fn type_mismatch_form_carries_typed_kwarg_path_slot_through_variant_slot() {
        // Sibling pin to `…_named_…` for the pre-key slot path. The
        // `KwargPath::Slot(idx)` shape names the offending position
        // without binding a key — it's the
        // "this-position-must-be-a-keyword" gate firing before any
        // identifier is known. The rendered `kwargs[<idx>]` shape
        // (no leading colon) bifurcates structurally from
        // `KwargPath::Named`'s `:<key>` shape.
        let err = LispError::TypeMismatch {
            form: KwargPath::Slot(2),
            expected: ExpectedKwargShape::Keyword,
            got: SexpShape::String,
        };
        match &err {
            LispError::TypeMismatch { form, .. } => {
                assert_eq!(*form, KwargPath::Slot(2));
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in kwargs[2]: expected keyword, got string"
        );
    }

    // ── KwargPathKind closed-set lift ───────────────────────────────────
    //
    // Sibling discriminator view of `KwargPath` — the payload-stripped
    // closed set of path-shape CATEGORIES (`Named` / `Item` / `Slot`).
    // Same shape every sibling payload-carrying closed-set enum in the
    // workspace pairs with (`AutoTerminate` / `AutoTerminateKind`,
    // `TerminateReason` / `TerminateReasonKind`, `SelectStrategy` /
    // `SelectStrategyKind`, `ChannelVariant` / `ChannelKind`). Consumers
    // that bucket type-mismatch failures by path-shape category (metrics
    // labels `path_kind=named` etc., a future tatara-check failure
    // histogram, an LSP that switches UI before drilling into the
    // bracket suffix) project through `KwargPath::kind` instead of
    // pattern-matching the full payload and discarding it at every site.

    #[test]
    fn kwarg_path_kind_all_is_unique_and_complete() {
        // Closed-set posture: `ALL` enumerates every reachable variant
        // EXACTLY ONCE — no duplicates, no omissions. The `[Self; 3]`
        // array literal in the declaration forces the arity at compile
        // time; this test catches the orthogonal failure modes — a
        // future variant added at the type without being added to ALL
        // (silently dropped from every consumer's sweep), or a typo
        // that duplicates an entry (silently double-counted). Same
        // truth-table pinning every sibling closed-set lift in the
        // workspace uses (ExpectedKwargShape::ALL, SexpShape::ALL,
        // MacroDefHead::ALL, CompilerSpecIoStage::ALL,
        // AutoTerminateKind::ALL, SelectStrategyKind::ALL, …).
        //
        // The `iter+map+collect+sort_unstable` quadruple this test
        // inlined pre-lift now binds at `<KwargPathKind as
        // ClosedSet>::sorted_labels()` — the canonical-ordered
        // candidate-list projection on the trait. Distinctness of the
        // sorted result is covered by
        // `assert_closed_set_well_formed::<KwargPathKind>()`.
        assert_eq!(KwargPathKind::ALL.len(), 3);
        assert_eq!(
            <KwargPathKind as crate::ClosedSet>::sorted_labels(),
            vec!["item", "named", "slot"],
            "KwargPathKind::ALL must cover every reachable path-shape category"
        );
    }

    #[test]
    fn kwarg_path_kind_label_round_trips_through_from_str() {
        // Bidirectional `label` ↔ `FromStr` contract: for every variant
        // in ALL, `kind.label().parse() == Ok(kind)`. A regression that
        // drifts the (variant, literal) pairing at ONE arm of `label`
        // (typo, capitalization drift) OR at the `FromStr` decode body
        // (off-by-one, missing variant in the sweep) fails-loudly here.
        // The canonical-literal site is singular (`label`) so the
        // round-trip is the only way the typed surface and the rendered
        // category literal can drift apart — pinning it here means they
        // cannot. Mirror of `expected_kwarg_shape_label_round_trips_…`
        // and every sibling closed-set round-trip in the workspace.
        for kind in KwargPathKind::ALL {
            let parsed: KwargPathKind = kind
                .label()
                .parse()
                .expect("every ALL variant's label must round-trip through FromStr");
            assert_eq!(
                parsed,
                kind,
                "FromStr({}) must round-trip to the same variant",
                kind.label()
            );
        }
    }

    #[test]
    fn kwarg_path_kind_display_matches_label_for_every_variant() {
        // Display delegates to `label` — pin the byte-for-byte equality
        // for every variant so a future Display impl that diverges from
        // the canonical projection (e.g., re-adds a prefix like
        // `"kind=named"`) fails-loudly here. Same posture as
        // `expected_kwarg_shape_display_matches_label_for_every_variant`.
        for kind in KwargPathKind::ALL {
            assert_eq!(format!("{kind}"), kind.label());
        }
    }

    #[test]
    fn unknown_kwarg_path_kind_carries_offending_input_verbatim() {
        // Operator-facing diagnostic contract: the offending input lands
        // in the typed error verbatim — no normalization, no case-folding,
        // no truncation. Pin the exact `#[error(...)]` rendering AND the
        // typed `.0` field projection so a future refactor that
        // normalizes (e.g. `.to_lowercase()`) before building the error
        // or that drops the input fails-loudly here. Symmetric to every
        // sibling `Unknown*` carrier in the workspace.
        let err: UnknownKwargPathKind = "Named".parse::<KwargPathKind>().expect_err(
            "capitalized `Named` must NOT decode — labels are byte-equal case-sensitive",
        );
        assert_eq!(err.0, "Named");
        assert_eq!(format!("{err}"), "unknown kwarg path kind: Named");

        // A kwargs-path RENDERING (`:foo`) is NOT a kind label — the
        // CATEGORY axis is orthogonal to the IDENTITY axis; FromStr must
        // reject rendered identities, not silently coerce them.
        let err: UnknownKwargPathKind = ":foo"
            .parse::<KwargPathKind>()
            .expect_err("`:foo` is a KwargPath rendering, not a KwargPathKind label");
        assert_eq!(err.0, ":foo");
        assert_eq!(format!("{err}"), "unknown kwarg path kind: :foo");

        let err: UnknownKwargPathKind = ""
            .parse::<KwargPathKind>()
            .expect_err("empty input must NOT decode to a KwargPathKind");
        assert_eq!(err.0, "");
        assert_eq!(format!("{err}"), "unknown kwarg path kind: ");
    }

    #[test]
    fn kwarg_path_kind_projects_each_variant_to_canonical_kind() {
        // Load-bearing discriminator contract: `KwargPath::kind()` strips
        // the payload and projects to the canonical `KwargPathKind`
        // variant for each `KwargPath` shape. A regression that swaps
        // arms (e.g., `Named` → `KwargPathKind::Slot`) fails-loudly here
        // AND breaks every consumer that buckets by category. Symmetric
        // to `AutoTerminate::kind` and `TerminateReason::kind` in
        // tatara-process.
        assert_eq!(KwargPath::named("threshold").kind(), KwargPathKind::Named);
        assert_eq!(KwargPath::item("steps", 3).kind(), KwargPathKind::Item);
        assert_eq!(KwargPath::Slot(2).kind(), KwargPathKind::Slot);
    }

    #[test]
    fn kwarg_path_kind_is_copy_and_hash_for_metrics_label_ergonomics() {
        // `KwargPathKind` derives Clone + Copy + Debug + PartialEq + Eq
        // + Hash so consumers that use the kind as a metrics-label key
        // (failure-cluster histogram keyed by `path_kind`), a HashMap
        // key (per-category counter), or a Copy-able discriminator in a
        // hot loop (kind projection in a kwarg-gate batching loop) reach
        // for the type without `.clone()` overhead. `String`-carrying
        // `KwargPath` is intentionally NOT `Copy`; `KwargPathKind` IS —
        // the split is the whole point of the discriminator view. A
        // regression that drops Copy or Hash breaks compilation here.
        let k = KwargPathKind::Named;
        let k_copy = k; // Copy
        assert_eq!(k, k_copy);
        let _: String = format!("{k:?}"); // Debug
        let mut s: std::collections::HashSet<KwargPathKind> = std::collections::HashSet::new();
        s.insert(KwargPathKind::Named);
        s.insert(KwargPathKind::Item);
        s.insert(KwargPathKind::Slot);
        s.insert(KwargPathKind::Named); // duplicate insert is a no-op (Hash + Eq)
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn kwarg_path_kind_label_does_not_overlap_kwarg_path_display_renderings() {
        // Cross-axis guard: the CATEGORY labels (`"named"` / `"item"` /
        // `"slot"`) are intentionally disjoint from the IDENTITY
        // renderings (`":<key>"` / `":<key>[<idx>]"` / `"kwargs[<idx>]"`)
        // so a consumer that confuses the two surfaces (e.g., parses
        // `kind.label()` as a `KwargPath` rendering, or parses a rendered
        // path as a kind label) fails-loudly through `FromStr` rejection
        // in both directions. Pin the non-overlap so a future label
        // rename that drifts into the rendering vocabulary (e.g.,
        // renaming `Slot`'s label to `"kwargs"`) fails-loudly here.
        for kind in KwargPathKind::ALL {
            let label = kind.label();
            assert!(
                !label.starts_with(':'),
                "kind label {label:?} must not start with `:` (would collide with KwargPath::Named/Item rendering)"
            );
            assert!(
                !label.contains('['),
                "kind label {label:?} must not contain `[` (would collide with KwargPath::Item/Slot rendering)"
            );
        }
    }

    // ── ExpectedKwargShape closed-set lift ──────────────────────────────
    //
    // The `LispError::TypeMismatch.expected` slot was promoted from
    // `&'static str` to the typed closed-set `ExpectedKwargShape` enum.
    // The seven reachable expected-shape labels — `Keyword` /
    // `String` / `Int` / `Number` / `Bool` / `List` / `ListOfStrings`
    // — are now encoded as variant identities so authoring tools (REPL,
    // LSP, `tatara-check`) bind on `ExpectedKwargShape::Number` etc.
    // directly rather than substring-matching `expected == "number"`.
    // Same posture as `KwargPath`, `MacroDefHead`, `UnquoteForm`,
    // `CompilerSpecIoStage`, and `TemplateInvariantKind`.

    #[test]
    fn label_renders_canonical_string_for_every_variant() {
        // Pin every variant's canonical `&'static str` projection — a
        // regression that drifts any label (typo in `"strin"` for
        // `"string"`, swap of `"int"` ↔ `"number"`) fails-loudly here.
        // The seven labels are byte-for-byte identical to the pre-lift
        // `&'static str` literals scattered across `domain.rs` so
        // existing `format!("{err}").contains("expected string")`
        // / `expected int` / `expected number` / etc. assertions in
        // consumer crates pass unchanged across the lift.
        assert_eq!(ExpectedKwargShape::Keyword.label(), "keyword");
        assert_eq!(ExpectedKwargShape::String.label(), "string");
        assert_eq!(ExpectedKwargShape::Int.label(), "int");
        assert_eq!(ExpectedKwargShape::Number.label(), "number");
        assert_eq!(ExpectedKwargShape::Bool.label(), "bool");
        assert_eq!(ExpectedKwargShape::List.label(), "list");
        assert_eq!(ExpectedKwargShape::ListOfStrings.label(), "list of strings");
    }

    #[test]
    fn display_matches_label_for_every_variant() {
        // Pin Display-equals-label: the `#[error("... expected
        // {expected}, ...")]` annotation on `LispError::TypeMismatch`
        // projects through Display, and Display delegates to `label()`.
        // A regression that introduces a Display impl that deviates from
        // `label()` (e.g. capitalizing one variant) would drift the
        // diagnostic surface; this test pins the contract.
        assert_eq!(format!("{}", ExpectedKwargShape::Keyword), "keyword");
        assert_eq!(format!("{}", ExpectedKwargShape::String), "string");
        assert_eq!(format!("{}", ExpectedKwargShape::Int), "int");
        assert_eq!(format!("{}", ExpectedKwargShape::Number), "number");
        assert_eq!(format!("{}", ExpectedKwargShape::Bool), "bool");
        assert_eq!(format!("{}", ExpectedKwargShape::List), "list");
        assert_eq!(
            format!("{}", ExpectedKwargShape::ListOfStrings),
            "list of strings"
        );
    }

    #[test]
    fn type_mismatch_expected_carries_typed_shape_through_variant_slot() {
        // After the typed-slot promotion, `LispError::TypeMismatch.expected`
        // is `ExpectedKwargShape` — the closed-set typed enum.
        // Consumers (REPL, LSP, `tatara-check`) pattern-match on the
        // variant identity `ExpectedKwargShape::Number` directly rather
        // than substring-matching a rendered `"expected number"` prefix.
        // Pin the structural binding AND the Display projection so the
        // byte-for-byte rendering contract is anchored from both
        // angles. A regression that re-introduces a `&'static str`-
        // shaped expected slot (collapsing the typed enum back into a
        // free-form label) fails-loudly here.
        let err = LispError::TypeMismatch {
            form: KwargPath::named("threshold"),
            expected: ExpectedKwargShape::Number,
            got: SexpShape::String,
        };
        match &err {
            LispError::TypeMismatch { expected, .. } => {
                assert_eq!(*expected, ExpectedKwargShape::Number);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: expected number, got string"
        );
    }

    #[test]
    fn type_mismatch_expected_list_of_strings_bifurcates_from_list() {
        // The `extract_string_list` outer-shape gate emits
        // `ExpectedKwargShape::ListOfStrings` (`"list of strings"`),
        // bifurcating structurally from `extract_vec_via_serde`'s
        // outer-shape gate which emits `ExpectedKwargShape::List`
        // (`"list"`). Two related-but-distinct gates, two distinct
        // variant identities; the typed enum makes that bifurcation
        // load-bearing. A regression that collapses them into one
        // variant (e.g. `ExpectedKwargShape::AnyList`) would drift the
        // diagnostic message; this test pins both shapes.
        let list_of_strings = LispError::TypeMismatch {
            form: KwargPath::named("tags"),
            expected: ExpectedKwargShape::ListOfStrings,
            got: SexpShape::String,
        };
        let list = LispError::TypeMismatch {
            form: KwargPath::named("steps"),
            expected: ExpectedKwargShape::List,
            got: SexpShape::String,
        };
        assert_eq!(
            format!("{list_of_strings}"),
            "compile error in :tags: expected list of strings, got string"
        );
        assert_eq!(
            format!("{list}"),
            "compile error in :steps: expected list, got string"
        );
        match (&list_of_strings, &list) {
            (
                LispError::TypeMismatch { expected: a, .. },
                LispError::TypeMismatch { expected: b, .. },
            ) => {
                assert_ne!(a, b);
                assert_eq!(*a, ExpectedKwargShape::ListOfStrings);
                assert_eq!(*b, ExpectedKwargShape::List);
            }
            _ => panic!("both must be TypeMismatch"),
        }
    }

    #[test]
    fn expected_kwarg_shape_all_is_unique_and_complete() {
        // Closed-set posture: `ALL` enumerates every reachable variant
        // EXACTLY ONCE — no duplicates, no omissions. The `[Self; 7]`
        // array literal in the declaration forces the arity at compile
        // time; this test catches the orthogonal failure modes — a
        // future variant added at the type without being added to ALL
        // (silently dropped from every consumer's sweep), or a typo
        // that duplicates an entry (silently double-counted). Same
        // truth-table pinning every sibling closed-set lift in the
        // workspace uses (SexpShape::ALL, MacroDefHead::ALL,
        // UnquoteForm::ALL, RequestorKind::ALL, ReceiptKind::ALL,
        // ConditionKind::ALL, ProcessPhase::ALL, ChannelKind::ALL, …).
        //
        // The `iter+map+collect+sort_unstable` quadruple this test
        // inlined pre-lift now binds at `<ExpectedKwargShape as
        // ClosedSet>::sorted_labels()` — the canonical-ordered
        // candidate-list projection on the trait. Distinctness of the
        // sorted result is covered by
        // `assert_closed_set_well_formed::<ExpectedKwargShape>()`.
        assert_eq!(ExpectedKwargShape::ALL.len(), 7);
        assert_eq!(
            <ExpectedKwargShape as crate::ClosedSet>::sorted_labels(),
            vec![
                "bool",
                "int",
                "keyword",
                "list",
                "list of strings",
                "number",
                "string",
            ],
            "ExpectedKwargShape::ALL must cover every reachable expected-shape label"
        );
    }

    #[test]
    fn expected_kwarg_shape_label_round_trips_through_from_str() {
        // Bidirectional `label` ↔ `FromStr` contract: for every
        // variant in ALL, `shape.label().parse() == Ok(shape)`. A
        // regression that drifts the (variant, literal) pairing at
        // ONE arm of `label` (typo, capitalization drift) OR at the
        // `FromStr` decode body (off-by-one, missing variant in the
        // sweep) fails-loudly here. The canonical-literal site is
        // singular (`label`) so the round-trip is the only way the
        // typed surface and the rendered diagnostic literal can drift
        // apart — pinning it here means they cannot. Mirror of
        // `sexp_shape_label_round_trips_through_from_str` and every
        // sibling closed-set round-trip in the workspace.
        for shape in ExpectedKwargShape::ALL {
            let parsed: ExpectedKwargShape = shape
                .label()
                .parse()
                .expect("every ALL variant's label must round-trip through FromStr");
            assert_eq!(
                parsed,
                shape,
                "FromStr({}) must round-trip to the same variant",
                shape.label()
            );
        }
    }

    #[test]
    fn unknown_expected_kwarg_shape_carries_offending_input_verbatim() {
        // Operator-facing diagnostic contract: the offending input
        // lands in the typed error verbatim — no normalization, no
        // case-folding, no truncation. Pin the exact `#[error(...)]`
        // rendering AND the typed `.0` field projection so a future
        // refactor that normalizes (e.g. `.to_lowercase()`) before
        // building the error or that drops the input fails-loudly
        // here. Symmetric to every sibling `Unknown*` carrier in the
        // workspace.
        let err: UnknownExpectedKwargShape = "Number".parse::<ExpectedKwargShape>().expect_err(
            "capitalized `Number` must NOT decode — labels are byte-equal case-sensitive",
        );
        assert_eq!(err.0, "Number");
        assert_eq!(format!("{err}"), "unknown expected kwarg shape: Number");

        let err: UnknownExpectedKwargShape = "float"
            .parse::<ExpectedKwargShape>()
            .expect_err("`float` is SexpShape's vocabulary, not ExpectedKwargShape's");
        assert_eq!(err.0, "float");
        assert_eq!(format!("{err}"), "unknown expected kwarg shape: float");

        let err: UnknownExpectedKwargShape = ""
            .parse::<ExpectedKwargShape>()
            .expect_err("empty input must NOT decode to an ExpectedKwargShape");
        assert_eq!(err.0, "");
        assert_eq!(format!("{err}"), "unknown expected kwarg shape: ");
    }

    #[test]
    fn expected_kwarg_shape_from_str_accepts_only_canonical_labels() {
        // Cross-axis guard: `SexpShape::label()`'s vocabulary overlaps
        // with `ExpectedKwargShape::label()` on five of seven entries
        // (`keyword` / `string` / `int` / `bool` / `list`) and DOES
        // NOT overlap on the structural-only `nil` / `symbol` /
        // `float` / `quote` / `quasiquote` / `unquote` / `unquote-splice`
        // entries — those name Sexp identities the typed-entry kwarg
        // gate cannot `expect`. The overlap is intentional — both
        // axes are projections of the same `Sexp` algebra at typed-
        // entry gates — but the non-overlap is the load-bearing part:
        // a `FromStr` that silently accepts `"float"` as an
        // `ExpectedKwargShape` would corrupt the typed identity. Pin
        // BOTH directions: the overlap decodes successfully (and to
        // the matching `ExpectedKwargShape` variant), the non-overlap
        // rejects. Symmetric to `sexp_shape_from_str_accepts_only_
        // canonical_labels` from the other axis.
        assert_eq!(
            "keyword".parse::<ExpectedKwargShape>().unwrap(),
            ExpectedKwargShape::Keyword
        );
        assert_eq!(
            "string".parse::<ExpectedKwargShape>().unwrap(),
            ExpectedKwargShape::String
        );
        assert_eq!(
            "int".parse::<ExpectedKwargShape>().unwrap(),
            ExpectedKwargShape::Int
        );
        assert_eq!(
            "bool".parse::<ExpectedKwargShape>().unwrap(),
            ExpectedKwargShape::Bool
        );
        assert_eq!(
            "list".parse::<ExpectedKwargShape>().unwrap(),
            ExpectedKwargShape::List
        );
        // Non-overlap: SexpShape-only labels reject through FromStr.
        for sexp_only in ["nil", "symbol", "float", "quote", "quasiquote", "unquote"] {
            sexp_only.parse::<ExpectedKwargShape>().unwrap_err();
        }
        // ExpectedKwargShape-only labels: `number` and `list of strings`
        // decode here but reject through SexpShape::FromStr — the
        // non-overlap axis is symmetric.
        assert_eq!(
            "number".parse::<ExpectedKwargShape>().unwrap(),
            ExpectedKwargShape::Number
        );
        assert_eq!(
            "list of strings".parse::<ExpectedKwargShape>().unwrap(),
            ExpectedKwargShape::ListOfStrings
        );
        "number".parse::<SexpShape>().unwrap_err();
        "list of strings".parse::<SexpShape>().unwrap_err();
    }

    // ── SexpShape closed-set lift ───────────────────────────────────────
    //
    // The `LispError::TypeMismatch.got` and
    // `LispError::NamedFormNonSymbolName.got` slots were promoted from
    // `&'static str` to the typed closed-set `SexpShape` enum. The
    // twelve reachable Sexp outermost shapes — `Nil` / `Symbol` /
    // `Keyword` / `String` / `Int` / `Float` / `Bool` / `List` /
    // `Quote` / `Quasiquote` / `Unquote` / `UnquoteSplice` — are now
    // encoded as variant identities so authoring tools (REPL, LSP,
    // `tatara-check`) bind on `SexpShape::Int` etc. directly rather
    // than substring-matching `got == "int"`. Same posture as
    // `KwargPath`, `ExpectedKwargShape`, `MacroDefHead`, `UnquoteForm`,
    // `CompilerSpecIoStage`, and `TemplateInvariantKind`. After this
    // lift the `TypeMismatch` variant is fully closed-set typed in
    // ALL THREE of its slots — no `&'static str` projection remains
    // at any helper boundary.

    #[test]
    fn sexp_shape_label_renders_canonical_string_for_every_variant() {
        // Pin every variant's canonical `&'static str` projection — a
        // regression that drifts any label (typo in `"strin"` for
        // `"string"`, swap of `"int"` ↔ `"float"`, capitalization
        // drift `"Quote"` for `"quote"`) fails-loudly here. The twelve
        // labels are byte-for-byte identical to the pre-lift
        // `sexp_type_name` projection so existing
        // `format!("{err}").contains("got int")` /
        // `got string` / `got list` / etc. assertions in consumer
        // crates pass unchanged across the lift.
        assert_eq!(SexpShape::Nil.label(), "nil");
        assert_eq!(SexpShape::Symbol.label(), "symbol");
        assert_eq!(SexpShape::Keyword.label(), "keyword");
        assert_eq!(SexpShape::String.label(), "string");
        assert_eq!(SexpShape::Int.label(), "int");
        assert_eq!(SexpShape::Float.label(), "float");
        assert_eq!(SexpShape::Bool.label(), "bool");
        assert_eq!(SexpShape::List.label(), "list");
        assert_eq!(SexpShape::Quote.label(), "quote");
        assert_eq!(SexpShape::Quasiquote.label(), "quasiquote");
        assert_eq!(SexpShape::Unquote.label(), "unquote");
        assert_eq!(SexpShape::UnquoteSplice.label(), "unquote-splice");
    }

    #[test]
    fn sexp_shape_display_matches_label_for_every_variant() {
        // Pin Display-equals-label: the `#[error("... got {got}")]`
        // annotations on `LispError::TypeMismatch` and
        // `LispError::NamedFormNonSymbolName` project through Display,
        // and Display delegates to `label()`. A regression that
        // introduces a Display impl that deviates from `label()`
        // (e.g. capitalizing one variant) would drift the diagnostic
        // surface; this test pins the contract.
        assert_eq!(format!("{}", SexpShape::Nil), "nil");
        assert_eq!(format!("{}", SexpShape::Symbol), "symbol");
        assert_eq!(format!("{}", SexpShape::Keyword), "keyword");
        assert_eq!(format!("{}", SexpShape::String), "string");
        assert_eq!(format!("{}", SexpShape::Int), "int");
        assert_eq!(format!("{}", SexpShape::Float), "float");
        assert_eq!(format!("{}", SexpShape::Bool), "bool");
        assert_eq!(format!("{}", SexpShape::List), "list");
        assert_eq!(format!("{}", SexpShape::Quote), "quote");
        assert_eq!(format!("{}", SexpShape::Quasiquote), "quasiquote");
        assert_eq!(format!("{}", SexpShape::Unquote), "unquote");
        assert_eq!(format!("{}", SexpShape::UnquoteSplice), "unquote-splice");
    }

    #[test]
    fn type_mismatch_got_carries_typed_shape_through_variant_slot() {
        // After the typed-slot promotion, `LispError::TypeMismatch.got`
        // is `SexpShape` — the closed-set typed enum. Consumers
        // (REPL, LSP, `tatara-check`) pattern-match on the variant
        // identity `SexpShape::Int` directly rather than
        // substring-matching a rendered `"got int"` prefix. Pin the
        // structural binding AND the Display projection so the
        // byte-for-byte rendering contract is anchored from both
        // angles. A regression that re-introduces a `&'static str`-
        // shaped `got` slot (collapsing the typed enum back into a
        // free-form label) fails-loudly here.
        let err = LispError::TypeMismatch {
            form: KwargPath::named("threshold"),
            expected: ExpectedKwargShape::Number,
            got: SexpShape::String,
        };
        match &err {
            LispError::TypeMismatch { got, .. } => {
                assert_eq!(*got, SexpShape::String);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: expected number, got string"
        );
    }

    #[test]
    fn named_form_non_symbol_name_got_carries_typed_shape_through_variant_slot() {
        // Sibling pin to `type_mismatch_got_…` on the second `got`
        // slot that flows from `sexp_shape`. Both
        // `LispError::TypeMismatch.got` and
        // `LispError::NamedFormNonSymbolName.got` are typed
        // `SexpShape` now — one helper (`crate::domain::sexp_shape`)
        // is the single projection source, and rustc-enforces
        // matching at every projection site. A regression that
        // bifurcates the two slots (e.g. typed `SexpShape` on one,
        // `&'static str` on the other) fails-loudly here.
        let err = LispError::NamedFormNonSymbolName {
            keyword: "defpoint",
            got: SexpShape::List,
        };
        match &err {
            LispError::NamedFormNonSymbolName { got, .. } => {
                assert_eq!(*got, SexpShape::List);
            }
            other => panic!("expected NamedFormNonSymbolName, got {other:?}"),
        }
        assert_eq!(
            format!("{err}"),
            "compile error in defpoint: positional NAME must be a symbol or string (got list)"
        );
    }

    #[test]
    fn sexp_shape_all_is_unique_and_complete() {
        // Closed-set posture: `ALL` enumerates every reachable variant
        // EXACTLY ONCE — no duplicates, no omissions. The `[Self; 12]`
        // array literal in the declaration forces the arity at compile
        // time; this test catches the orthogonal failure modes — a
        // future variant added at the type without being added to ALL
        // (silently dropped from every consumer's sweep), or a typo
        // that duplicates an entry (silently double-counted). Same
        // truth-table pinning every sibling closed-set lift in the
        // workspace uses (RequestorKind::ALL, ReceiptKind::ALL,
        // ConditionKind::ALL, ProcessPhase::ALL, ChannelKind::ALL, …).
        //
        // The `iter+map+collect+sort_unstable` quadruple this test
        // inlined pre-lift now binds at `<SexpShape as
        // ClosedSet>::sorted_labels()` — the canonical-ordered
        // candidate-list projection on the trait. Distinctness of the
        // sorted result is covered by
        // `assert_closed_set_well_formed::<SexpShape>()`.
        assert_eq!(SexpShape::ALL.len(), 12);
        assert_eq!(
            <SexpShape as crate::ClosedSet>::sorted_labels(),
            vec![
                "bool",
                "float",
                "int",
                "keyword",
                "list",
                "nil",
                "quasiquote",
                "quote",
                "string",
                "symbol",
                "unquote",
                "unquote-splice",
            ],
            "SexpShape::ALL must cover every reachable Sexp outermost shape"
        );
    }

    #[test]
    fn sexp_shape_label_round_trips_through_from_str() {
        // Bidirectional `label` ↔ `FromStr` contract: for every
        // variant in ALL, `shape.label().parse() == Ok(shape)`. A
        // regression that drifts the (variant, literal) pairing at
        // ONE arm of `label` (typo, capitalization drift) OR at the
        // `FromStr` decode body (off-by-one, missing variant in the
        // sweep) fails-loudly here. The canonical-literal site is
        // singular (`label`) so the round-trip is the only way the
        // typed surface and the rendered diagnostic literal can
        // drift apart — pinning it here means they cannot.
        for shape in SexpShape::ALL {
            let parsed: SexpShape = shape
                .label()
                .parse()
                .expect("every ALL variant's label must round-trip through FromStr");
            assert_eq!(
                parsed,
                shape,
                "FromStr({}) must round-trip to the same variant",
                shape.label()
            );
        }
    }

    #[test]
    fn unknown_sexp_shape_carries_offending_input_verbatim() {
        // Operator-facing diagnostic contract: the offending input
        // lands in the typed error verbatim — no normalization, no
        // case-folding, no truncation. Pin the exact `#[error(...)]`
        // rendering AND the typed `.0` field projection so a future
        // refactor that normalizes (e.g. `.to_lowercase()`) before
        // building the error or that drops the input fails-loudly
        // here. Symmetric to every sibling `Unknown*` carrier in the
        // workspace.
        let err: UnknownSexpShape = "Symbol".parse::<SexpShape>().expect_err(
            "capitalized `Symbol` must NOT decode — labels are byte-equal case-sensitive",
        );
        assert_eq!(err.0, "Symbol");
        assert_eq!(format!("{err}"), "unknown sexp shape: Symbol");

        let err: UnknownSexpShape = "number"
            .parse::<SexpShape>()
            .expect_err("`number` is ExpectedKwargShape's vocabulary, not SexpShape's");
        assert_eq!(err.0, "number");
        assert_eq!(format!("{err}"), "unknown sexp shape: number");

        let err: UnknownSexpShape = ""
            .parse::<SexpShape>()
            .expect_err("empty input must NOT decode to a SexpShape");
        assert_eq!(err.0, "");
        assert_eq!(format!("{err}"), "unknown sexp shape: ");
    }

    #[test]
    fn sexp_shape_from_str_accepts_only_canonical_labels() {
        // Cross-axis guard: `ExpectedKwargShape::label()`'s vocabulary
        // overlaps with `SexpShape::label()` on five of seven entries
        // (`keyword` / `string` / `int` / `bool` / `list`) and DOES
        // NOT overlap on two (`number` / `list of strings`). The
        // overlap is intentional — both axes are projections of the
        // same `Sexp` algebra at typed-entry gates — but the
        // non-overlap is the load-bearing part: a `FromStr` that
        // silently accepts `"number"` as a `SexpShape` would corrupt
        // the typed identity. Pin BOTH directions: the overlap
        // decodes successfully (and to the matching `SexpShape`
        // variant), the non-overlap rejects.
        assert_eq!("keyword".parse::<SexpShape>().unwrap(), SexpShape::Keyword);
        assert_eq!("string".parse::<SexpShape>().unwrap(), SexpShape::String);
        assert_eq!("int".parse::<SexpShape>().unwrap(), SexpShape::Int);
        assert_eq!("bool".parse::<SexpShape>().unwrap(), SexpShape::Bool);
        assert_eq!("list".parse::<SexpShape>().unwrap(), SexpShape::List);

        assert!("number".parse::<SexpShape>().is_err());
        assert!("list of strings".parse::<SexpShape>().is_err());

        // Sanity: every UnquoteForm marker literal (`,` / `,@` / etc.)
        // is also NOT a SexpShape label — the marker projection lives
        // on a different axis (the rendered punctuation) than the
        // shape label (the structural identity).
        assert!(",".parse::<SexpShape>().is_err());
        assert!(",@".parse::<SexpShape>().is_err());
    }

    #[test]
    fn sexp_shape_int_bifurcates_from_float_through_variant_slot() {
        // `Int` and `Float` are distinct typed variants — a regression
        // that collapses them into a single `Number` variant (which
        // would drop the bifurcation that `Sexp::Atom(Int(_))` and
        // `Sexp::Atom(Float(_))` already carry at the AST layer) is
        // caught here. The two render distinct rendered labels and
        // hold distinct variant identities.
        let int_err = LispError::TypeMismatch {
            form: KwargPath::named("count"),
            expected: ExpectedKwargShape::String,
            got: SexpShape::Int,
        };
        let float_err = LispError::TypeMismatch {
            form: KwargPath::named("ratio"),
            expected: ExpectedKwargShape::String,
            got: SexpShape::Float,
        };
        assert_eq!(
            format!("{int_err}"),
            "compile error in :count: expected string, got int"
        );
        assert_eq!(
            format!("{float_err}"),
            "compile error in :ratio: expected string, got float"
        );
        match (&int_err, &float_err) {
            (LispError::TypeMismatch { got: a, .. }, LispError::TypeMismatch { got: b, .. }) => {
                assert_ne!(a, b);
                assert_eq!(*a, SexpShape::Int);
                assert_eq!(*b, SexpShape::Float);
            }
            _ => panic!("both must be TypeMismatch"),
        }
    }

    // ── SexpShape ↔ AtomKind / QuoteForm: typed-shape lattice inverses ──
    //
    // The forward embed projections [`crate::ast::AtomKind::sexp_shape`]
    // (6→12) and [`crate::ast::QuoteForm::sexp_shape`] (4→12) have
    // existed for prior runs (commits 121bb60 + b15-ish); their dual
    // projections [`SexpShape::as_atom_kind`] (12→6, partial) and
    // [`SexpShape::as_quote_form`] (12→4, partial) close the
    // embed/project section on the typed-shape lattice. The
    // composition laws below pin the (embed, project) pair is an
    // `Iso(AtomKind, AtomShape ⊂ SexpShape)` AND
    // `Iso(QuoteForm, QuoteShape ⊂ SexpShape)` — every typed marker
    // round-trips through the embed, every shape pre-image recovers
    // the typed marker. Pre-lift the typed-shape lattice's two
    // forward embeds had no dual projection naming the inverse 6-of-
    // 12 + 4-of-12 carvings; post-lift the carvings live at ONE site
    // each on the [`SexpShape`] algebra so a regression that drifts
    // the inverse from the embed surfaces at the round-trip pin
    // instead of at every speculative LSP / REPL / `tatara-check` /
    // metrics consumer's per-carving inline `match`.

    #[test]
    fn as_atom_kind_projects_each_atom_shape_to_canonical_atom_kind_and_rejects_non_atom_shapes() {
        // Per-variant truth-table sweep across every `SexpShape::ALL`
        // entry — pins each variant's canonical mapping (atomic-payload
        // arms project to the matching `AtomKind`; `Nil` / `List` /
        // every quote-family arm project to `None`) byte-for-byte so a
        // regression that drifts ONE arm (e.g. swaps `Symbol →
        // AtomKind::Keyword`, drops `Bool`'s arm to `None`, accepts
        // `List` as `Some(AtomKind::Str)`) fails loudly. The full
        // `SexpShape::ALL` sweep doubles as exhaustiveness — adding a
        // hypothetical thirteenth variant (e.g. `Vector` for `#(...)`)
        // forces the test author to extend BOTH this sweep AND the
        // typed `match` body, with rustc enforcing the match arm's
        // presence and this sweep enforcing the (variant, projected
        // mapping) pairing's canonical-form.
        use crate::ast::AtomKind;
        for shape in SexpShape::ALL {
            let projected = shape.as_atom_kind();
            let expected = match shape {
                SexpShape::Symbol => Some(AtomKind::Symbol),
                SexpShape::Keyword => Some(AtomKind::Keyword),
                SexpShape::String => Some(AtomKind::Str),
                SexpShape::Int => Some(AtomKind::Int),
                SexpShape::Float => Some(AtomKind::Float),
                SexpShape::Bool => Some(AtomKind::Bool),
                SexpShape::Nil
                | SexpShape::List
                | SexpShape::Quote
                | SexpShape::Quasiquote
                | SexpShape::Unquote
                | SexpShape::UnquoteSplice => None,
            };
            assert_eq!(
                projected, expected,
                "SexpShape::{shape:?}.as_atom_kind() drifted from canonical mapping"
            );
        }
    }

    #[test]
    fn atom_kind_sexp_shape_round_trips_through_as_atom_kind() {
        // The embed/project section law on the atomic carving:
        // `AtomKind::sexp_shape(k).as_atom_kind() == Some(k)` for every
        // `k: AtomKind::ALL`. Pinning the round-trip for every variant
        // in the closed set proves the (embed, project) pair is an
        // `Iso(AtomKind, AtomShape ⊂ SexpShape)` — the section is total
        // on `AtomKind`'s carving. A regression that drifts EITHER
        // direction (an `AtomKind::sexp_shape` arm that mis-maps OR a
        // `SexpShape::as_atom_kind` arm that mis-inverts) fails here
        // without depending on any per-consumer call site. Same posture
        // as `unquote_form_marker_routes_through_to_quote_form_prefix_
        // via_composition`'s round-trip on the 2-of-4 quote-family
        // subset.
        use crate::ast::AtomKind;
        for kind in AtomKind::ALL {
            let shape = kind.sexp_shape();
            let recovered = shape.as_atom_kind();
            assert_eq!(
                recovered,
                Some(kind),
                "AtomKind::{kind:?} did NOT round-trip — sexp_shape().as_atom_kind() must recover the typed marker"
            );
        }
    }

    #[test]
    fn as_quote_form_projects_each_quote_shape_to_canonical_quote_form_and_rejects_non_quote_shapes(
    ) {
        // Per-variant truth-table sweep across every `SexpShape::ALL`
        // entry — pins each variant's canonical mapping (quote-family
        // arms project to the matching `QuoteForm`; `Nil` / `List` /
        // every atomic-payload arm project to `None`) byte-for-byte.
        // Sibling sweep to
        // `as_atom_kind_projects_each_atom_shape_to_canonical_atom_kind_and_rejects_non_atom_shapes`
        // on the quote-family axis.
        use crate::ast::QuoteForm;
        for shape in SexpShape::ALL {
            let projected = shape.as_quote_form();
            let expected = match shape {
                SexpShape::Quote => Some(QuoteForm::Quote),
                SexpShape::Quasiquote => Some(QuoteForm::Quasiquote),
                SexpShape::Unquote => Some(QuoteForm::Unquote),
                SexpShape::UnquoteSplice => Some(QuoteForm::UnquoteSplice),
                SexpShape::Nil
                | SexpShape::List
                | SexpShape::Symbol
                | SexpShape::Keyword
                | SexpShape::String
                | SexpShape::Int
                | SexpShape::Float
                | SexpShape::Bool => None,
            };
            assert_eq!(
                projected, expected,
                "SexpShape::{shape:?}.as_quote_form() drifted from canonical mapping"
            );
        }
    }

    #[test]
    fn quote_form_sexp_shape_round_trips_through_as_quote_form() {
        // The embed/project section law on the quote-family carving:
        // `QuoteForm::sexp_shape(qf).as_quote_form() == Some(qf)` for
        // every `qf: QuoteForm::ALL`. Proves the (embed, project) pair
        // is an `Iso(QuoteForm, QuoteShape ⊂ SexpShape)` — the section
        // is total on `QuoteForm`'s carving. Sibling round-trip to
        // `atom_kind_sexp_shape_round_trips_through_as_atom_kind` on
        // the quote-family axis.
        use crate::ast::QuoteForm;
        for qf in QuoteForm::ALL {
            let shape = qf.sexp_shape();
            let recovered = shape.as_quote_form();
            assert_eq!(
                recovered,
                Some(qf),
                "QuoteForm::{qf:?} did NOT round-trip — sexp_shape().as_quote_form() must recover the typed marker"
            );
        }
    }

    #[test]
    fn as_atom_kind_and_as_quote_form_partition_carvable_sexp_shape_variants() {
        // Disjointness invariant: for every variant in
        // `SexpShape::ALL`, AT MOST ONE of `as_atom_kind()` and
        // `as_quote_form()` returns `Some` — the typed-shape lattice's
        // two closed-set carvings partition the carve-able SexpShape
        // variants. The two non-carved variants (`Nil`, `List`) project
        // to `None` through BOTH projections — the kernel of both
        // partial inverses. A regression that drifts the partition
        // (e.g. accepts `SexpShape::List` as an atom kind, or as a
        // quote form) fails here. Sibling to
        // `as_atom_kind_projects_each_atom_shape_to_canonical_atom_kind_and_rejects_non_atom_shapes`
        // and
        // `as_quote_form_projects_each_quote_shape_to_canonical_quote_form_and_rejects_non_quote_shapes`
        // — those pin the per-axis canonical mapping; this pins the
        // joint disjointness across both axes.
        for shape in SexpShape::ALL {
            let atom = shape.as_atom_kind().is_some();
            let quote = shape.as_quote_form().is_some();
            assert!(
                !(atom && quote),
                "SexpShape::{shape:?} projects as BOTH an atom kind AND a quote form — typed-shape carvings must be disjoint"
            );
            // Cross-axis closure: the only variants that project as
            // NEITHER are the non-carved structural shapes (`Nil` and
            // `List`). Every other variant must be in exactly ONE
            // carving — the substrate's typed-shape lattice's
            // structural completeness pin.
            let carved = atom || quote;
            let expected_carved = !matches!(shape, SexpShape::Nil | SexpShape::List);
            assert_eq!(
                carved, expected_carved,
                "SexpShape::{shape:?} must be carved iff it is neither Nil nor List"
            );
        }
    }

    #[test]
    fn as_atom_kind_composes_with_sexp_shape_via_atom_kind_label_round_trip() {
        // Cross-projection composition law: for every atom kind, the
        // diagnostic label round-trips through both directions of the
        // embed/project pair AND `AtomKind::label`:
        // `AtomKind::ALL[i].label() == AtomKind::ALL[i].sexp_shape()
        // .as_atom_kind().expect("...").label()`. This pin proves the
        // typed-shape lattice's two carvings are not only structural
        // inverses but ALSO label-coherent — a regression that drifts
        // the (AtomKind variant, SexpShape variant) pairing while
        // preserving the projection's structural inverseness (e.g. a
        // future refactor that renames the variants in lockstep but
        // leaves the per-variant labels stale) surfaces here. The
        // label-coherence binds the diagnostic surface to the typed
        // algebra at BOTH layers.
        use crate::ast::AtomKind;
        for kind in AtomKind::ALL {
            let via_round_trip = kind
                .sexp_shape()
                .as_atom_kind()
                .expect("every AtomKind round-trips through the embed/project pair")
                .label();
            assert_eq!(
                via_round_trip,
                kind.label(),
                "AtomKind::{kind:?}.label() drifted from sexp_shape().as_atom_kind().label() — embed/project must preserve label coherence"
            );
        }
    }

    #[test]
    fn as_quote_form_composes_with_sexp_shape_via_quote_form_prefix_round_trip() {
        // Cross-projection composition law (quote-family sibling of
        // `as_atom_kind_composes_with_sexp_shape_via_atom_kind_label_round_trip`):
        // for every `qf: QuoteForm::ALL`,
        // `qf.prefix() == qf.sexp_shape().as_quote_form().expect("...").prefix()`.
        // Pins the (QuoteForm variant, SexpShape variant) pairing
        // round-trips through the embed/project pair AND preserves
        // each variant's canonical homoiconic-prefix punctuation
        // (`"'"` / `` "`" `` / `","` / `",@"`) — a regression that
        // drifts the round-trip OR drifts the prefix surfaces here.
        use crate::ast::QuoteForm;
        for qf in QuoteForm::ALL {
            let via_round_trip = qf
                .sexp_shape()
                .as_quote_form()
                .expect("every QuoteForm round-trips through the embed/project pair")
                .prefix();
            assert_eq!(
                via_round_trip,
                qf.prefix(),
                "QuoteForm::{qf:?}.prefix() drifted from sexp_shape().as_quote_form().prefix() — embed/project must preserve prefix coherence"
            );
        }
    }

    // ── UnquoteForm: ALL closure + FromStr round-trip ──────────────────
    //
    // `UnquoteForm` (the two template-marker syntactic forms `,` and
    // `,@`) joins the substrate's closed-set algebra family —
    // `SexpShape::ALL` + `FromStr`, `AtomKind::ALL` + `FromStr`,
    // `RequestorKind::ALL` + `FromStr`, etc. — by lifting the canonical
    // `&'static str` marker literal vocabulary onto ONE site
    // (`Self::marker` keyed by `Self::ALL`) the operator-facing decode
    // path inverts. Pre-lift the punctuation vocabulary lived ONLY
    // in `marker()`'s match arms; post-lift the SAME vocabulary
    // round-trips through `FromStr` keyed on the closed set, so the
    // typed surface and the rendered diagnostic literal cannot drift.
    // Same posture as `sexp_shape_label_round_trips_through_from_str`
    // / `atom_kind_label_round_trips_through_from_str`.

    #[test]
    fn unquote_form_all_is_unique_and_complete() {
        // Closed-set posture: `ALL` enumerates every reachable variant
        // EXACTLY ONCE — no duplicates, no omissions. The `[Self; 2]`
        // array literal in the declaration forces the arity at compile
        // time; this test catches the orthogonal failure modes — a
        // future variant added at the type without being added to ALL
        // (silently dropped from every consumer's sweep), or a typo
        // that duplicates an entry (silently double-counted). Same
        // truth-table pinning every sibling closed-set lift in the
        // workspace uses (SexpShape::ALL, AtomKind::ALL,
        // RequestorKind::ALL, ReceiptKind::ALL, ConditionKind::ALL, …).
        //
        // The `iter+map+collect+sort_unstable` quadruple this test
        // inlined pre-lift now binds at `<UnquoteForm as
        // ClosedSet>::sorted_labels()` — the canonical-ordered
        // candidate-list projection on the trait. Distinctness of the
        // sorted result is covered by
        // `assert_closed_set_well_formed::<UnquoteForm>()`.
        assert_eq!(UnquoteForm::ALL.len(), 2);
        assert_eq!(
            <UnquoteForm as crate::ClosedSet>::sorted_labels(),
            vec![",", ",@"],
            "UnquoteForm::ALL must cover both template-marker syntactic forms"
        );
    }

    #[test]
    fn unquote_form_marker_round_trips_through_from_str() {
        // Bidirectional `marker` ↔ `FromStr` contract: for every
        // variant in ALL, `form.marker().parse() == Ok(form)`. A
        // regression that drifts the (variant, literal) pairing at
        // ONE arm of `marker` (typo, `,,` instead of `,`, `, @` with
        // a stray space) OR at the `FromStr` decode body (off-by-one,
        // missing variant in the sweep) fails-loudly here. The
        // canonical-literal site is singular (`marker`) so the
        // round-trip is the only way the typed surface and the
        // rendered diagnostic literal can drift apart — pinning it
        // here means they cannot.
        for form in UnquoteForm::ALL {
            let parsed: UnquoteForm = form
                .marker()
                .parse()
                .expect("every ALL variant's marker must round-trip through FromStr");
            assert_eq!(
                parsed,
                form,
                "FromStr({}) must round-trip to the same variant",
                form.marker()
            );
        }
    }

    #[test]
    fn unknown_unquote_form_carries_offending_input_verbatim() {
        // Operator-facing diagnostic contract: the offending input
        // lands in the typed error verbatim — no normalization, no
        // truncation, no whitespace coercion. Pin the exact
        // `#[error(...)]` rendering AND the typed `.0` field
        // projection so a future refactor that normalizes (e.g.
        // `.trim()`) before building the error or that drops the
        // input fails-loudly here. Symmetric to every sibling
        // `Unknown*` carrier in the workspace
        // ([`UnknownSexpShape`], [`crate::ast::UnknownAtomKind`],
        // `tatara_process::allocation::UnknownRequestorKind`, …).
        let err: UnknownUnquoteForm = ",,"
            .parse::<UnquoteForm>()
            .expect_err("doubled comma `,,` is not a canonical template marker");
        assert_eq!(err.0, ",,");
        assert_eq!(format!("{err}"), "unknown unquote form: ,,");

        let err: UnknownUnquoteForm = ",@@"
            .parse::<UnquoteForm>()
            .expect_err("doubled-at `,@@` is not a canonical template marker");
        assert_eq!(err.0, ",@@");
        assert_eq!(format!("{err}"), "unknown unquote form: ,@@");

        let err: UnknownUnquoteForm = ""
            .parse::<UnquoteForm>()
            .expect_err("empty input must NOT decode to an UnquoteForm");
        assert_eq!(err.0, "");
        assert_eq!(format!("{err}"), "unknown unquote form: ");
    }

    #[test]
    fn unquote_form_from_str_rejects_sexp_shape_labels_on_template_marker_axis() {
        // Cross-axis guard: [`SexpShape`] projects the SAME two
        // `Sexp::Unquote` / `Sexp::UnquoteSplice` constructors as
        // [`UnquoteForm`] does, but onto a DIFFERENT vocabulary —
        // `"unquote"` / `"unquote-splice"` (structural-identity labels)
        // vs `","` / `",@"` (punctuation markers). The two axes share
        // the same closed-set cardinality (2) but their vocabularies
        // are intentionally disjoint. A `FromStr` that silently
        // accepted `"unquote"` as an `UnquoteForm` would corrupt the
        // typed identity at the diagnostic boundary. Pin BOTH
        // directions: the SAME punctuation labels (`,` / `,@`) decode
        // through [`UnquoteForm`] but NOT through [`SexpShape`]; the
        // SAME structural labels (`"unquote"` / `"unquote-splice"`)
        // decode through [`SexpShape`] but NOT through [`UnquoteForm`].
        // Anchors the cross-axis disjointness from BOTH sides so a
        // regression that conflates the two axes' vocabularies fails
        // here.
        assert_eq!(",".parse::<UnquoteForm>().unwrap(), UnquoteForm::Unquote);
        assert_eq!(",@".parse::<UnquoteForm>().unwrap(), UnquoteForm::Splice);

        // The structural-identity labels project the SAME variants on
        // the SexpShape axis but are NOT canonical UnquoteForm markers.
        assert!("unquote".parse::<UnquoteForm>().is_err());
        assert!("unquote-splice".parse::<UnquoteForm>().is_err());

        // Sibling homoiconic-prefix-wrapper markers (`'` for quote,
        // `` ` `` for quasiquote) belong to the WIDER QuoteForm
        // superset on the SAME punctuation axis — they MUST reject
        // here because UnquoteForm carves the 2-of-4 template-
        // substitution subset of QuoteForm's 4-prefix closed set.
        assert!("'".parse::<UnquoteForm>().is_err());
        assert!("`".parse::<UnquoteForm>().is_err());

        // Whitespace-padded markers are NOT canonical — the
        // round-trip must be exact byte-for-byte against `marker()`.
        assert!(" ,".parse::<UnquoteForm>().is_err());
        assert!(", ".parse::<UnquoteForm>().is_err());
        assert!(", @".parse::<UnquoteForm>().is_err());
    }

    #[test]
    fn unquote_form_to_quote_form_round_trips_through_as_unquote_form() {
        // Pin the 2-of-4 subset → superset projection as a typed
        // section of [`crate::ast::QuoteForm::as_unquote_form`]: for
        // every `uf: UnquoteForm`, `uf.to_quote_form().as_unquote_form()
        // == Some(uf)`. Closes the (UnquoteForm, QuoteForm) pairing as
        // a round-trip identity on the typed algebra — pre-lift the
        // pairing only lived in the existing
        // `unquote_form_marker_subset_decodes_through_quote_form_from_str`
        // cross-axis test (which round-tripped through the rendered
        // marker string + FromStr); post-lift the pairing rides the
        // typed projection directly so a future regression that drifts
        // the (UnquoteForm variant, QuoteForm variant) pairing
        // (e.g., a future arm that maps `UnquoteForm::Splice →
        // QuoteForm::Unquote`) fails this assertion without depending
        // on the FromStr decoder sitting between the two.
        use crate::ast::QuoteForm;
        for uf in UnquoteForm::ALL {
            assert_eq!(
                uf.to_quote_form().as_unquote_form(),
                Some(uf),
                "UnquoteForm::{uf:?} → QuoteForm via to_quote_form does not invert through QuoteForm::as_unquote_form — the 2-of-4 subset projection is no longer a section",
            );
        }

        // Per-arm pinning of the canonical mapping (the byte-for-byte
        // pairing the composition `marker == to_quote_form().prefix()`
        // depends on).
        assert_eq!(UnquoteForm::Unquote.to_quote_form(), QuoteForm::Unquote);
        assert_eq!(
            UnquoteForm::Splice.to_quote_form(),
            QuoteForm::UnquoteSplice
        );
    }

    #[test]
    fn unquote_form_marker_routes_through_to_quote_form_prefix_via_composition() {
        // Post-lift composition pin: for every `uf: UnquoteForm`,
        // `uf.marker()` and `uf.to_quote_form().prefix()` agree on
        // BOTH axes — (a) byte equality (the rendered diagnostic
        // literal cannot drift between the two projections); (b)
        // pointer equality (the canonical `&'static str` literal lives
        // at ONE site — `QuoteForm::prefix`'s Unquote/UnquoteSplice
        // arms in `ast.rs` — and `UnquoteForm::marker` routes through
        // that ONE address via the typed composition).
        //
        // The pointer-equality axis is load-bearing: a regression that
        // re-inlines the literals at `UnquoteForm::marker` as a
        // parallel match-table fails the pointer pin even when the
        // rendered bytes still agree (the inline-literal-table copy
        // lives at a different `&'static str` address — rustc may
        // de-duplicate identical literals within a single Rust
        // compilation unit, but the contract this test pins is
        // routing-through-the-canonical-site, not deduplication-by-the-
        // optimizer; a future build flag that disables literal dedup
        // would unmask the regression that the bytes-only assertion
        // misses). Sibling-shape pin to commit 1db697f's
        // `atom_kind_label_routes_through_sexp_shape_label_via_sexp_shape_projection`
        // — both pin the subset's projection through the superset's
        // canonical site via pointer-equality, the structural invariant
        // the subset-to-superset composition was lifted to make
        // load-bearing on the type system rather than on per-callsite
        // discipline.
        for uf in UnquoteForm::ALL {
            let from_marker = uf.marker();
            let from_composition = uf.to_quote_form().prefix();
            assert_eq!(
                from_marker, from_composition,
                "UnquoteForm::{uf:?}.marker() bytes drifted from .to_quote_form().prefix() bytes — the subset's diagnostic vocabulary is no longer derived from the superset's canonical site",
            );
            assert!(
                std::ptr::eq(from_marker.as_ptr(), from_composition.as_ptr()),
                "UnquoteForm::{uf:?}.marker() and .to_quote_form().prefix() disagree on `&'static str` address — pointer drift means the lift composes through a parallel literal table rather than routing into the canonical QuoteForm::prefix site",
            );
        }
    }

    #[test]
    fn unquote_form_wrap_routes_through_to_quote_form_wrap_via_composition() {
        // Post-lift composition pin: for every `uf: UnquoteForm` and
        // every representative `inner: Sexp`, `uf.wrap(inner.clone())
        // == uf.to_quote_form().wrap(inner)` byte-for-byte. The
        // (UnquoteForm marker, `Sexp::*` tuple-variant constructor)
        // pairing on the substitution-subset closed set is derived
        // structurally from the superset's canonical
        // [`crate::ast::QuoteForm::wrap`] closed-set match rather than
        // from a parallel two-arm inline table on this subset. A
        // regression that re-inlines the two arms as a parallel
        // match-table (a future edit that spells `Self::Unquote =>
        // Sexp::Unquote(Box::new(inner))` / `Self::Splice =>
        // Sexp::UnquoteSplice(Box::new(inner))` directly at
        // `UnquoteForm::wrap` instead of routing through
        // `self.to_quote_form().wrap(inner)`) still passes the round-
        // trip and canonical-tuple-variant sweeps below but fails
        // THIS composition pin — the subset's construct-family
        // vocabulary is no longer derived from the superset's
        // canonical site. Sibling-shape pin to commit 250c001's
        // `unquote_form_marker_routes_through_to_quote_form_prefix_via_composition`:
        // both pin the subset's projection through the superset's
        // canonical site via structural equality, the invariant the
        // subset-to-superset composition was lifted to make load-
        // bearing on the type system rather than on per-callsite
        // discipline.
        use crate::ast::Sexp;
        let inners = [
            Sexp::Nil,
            Sexp::symbol("x"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(42),
            Sexp::float(1.5),
            Sexp::boolean(true),
            Sexp::List(vec![Sexp::symbol("f"), Sexp::int(1), Sexp::int(2)]),
        ];
        for uf in UnquoteForm::ALL {
            for inner in &inners {
                let via_wrap = uf.wrap(inner.clone());
                let via_composition = uf.to_quote_form().wrap(inner.clone());
                assert_eq!(
                    via_wrap, via_composition,
                    "UnquoteForm::{uf:?}.wrap(inner) drifted from .to_quote_form().wrap(inner) — the subset's construct vocabulary is no longer derived from the superset's canonical site",
                );
            }
        }
    }

    #[test]
    fn unquote_form_wrap_emits_canonical_tuple_variant_for_every_marker() {
        // Byte-for-byte tuple-variant emission pin: for every
        // `uf: UnquoteForm` and every representative `inner: Sexp`,
        // `uf.wrap(inner)` produces the canonical `Sexp::Unquote(
        // Box::new(inner))` / `Sexp::UnquoteSplice(Box::new(inner))`
        // shape byte-for-byte. Pins the (subset marker, `Sexp::*`
        // tuple-variant constructor) pairing at the observable
        // wrapper-shape boundary — a regression that maps
        // `UnquoteForm::Unquote → Sexp::UnquoteSplice` (a marker/
        // constructor swap that still routes through the superset's
        // `wrap`) surfaces here because the composition through
        // `to_quote_form()` picks up the swap at the subset-to-
        // superset projection. Sibling of the outer `Sexp` construct
        // family's
        // `sexp_quote_family_constructors_emit_canonical_tuple_variant_for_every_marker`
        // (commit 38f076b) — same posture on the subset closed set.
        use crate::ast::Sexp;
        let inners = [
            Sexp::Nil,
            Sexp::symbol("x"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(-7),
            Sexp::float(2.0),
            Sexp::boolean(false),
            Sexp::List(vec![Sexp::symbol("f"), Sexp::int(1)]),
        ];
        for inner in &inners {
            assert_eq!(
                UnquoteForm::Unquote.wrap(inner.clone()),
                Sexp::Unquote(Box::new(inner.clone())),
                "UnquoteForm::Unquote.wrap({inner:?}) drifted from Sexp::Unquote(Box::new({inner:?})) canonical tuple-variant shape",
            );
            assert_eq!(
                UnquoteForm::Splice.wrap(inner.clone()),
                Sexp::UnquoteSplice(Box::new(inner.clone())),
                "UnquoteForm::Splice.wrap({inner:?}) drifted from Sexp::UnquoteSplice(Box::new({inner:?})) canonical tuple-variant shape",
            );
        }
    }

    #[test]
    fn unquote_form_wrap_round_trips_through_sexp_as_unquote() {
        // Section-for-retraction pin: `uf.wrap(inner).as_unquote() ==
        // Some((uf, &inner))` for every `uf: UnquoteForm` and every
        // representative `inner: Sexp`. Closes the (construct,
        // project) algebra dual on the closed-set `UnquoteForm`
        // algebra — the substitution-subset peer of the closed-set
        // superset algebra's already-closed
        // ([`crate::ast::QuoteForm::wrap`],
        // [`crate::ast::Sexp::as_quote_form`]) dual. The typed
        // constructor + typed projection pair form an `Iso(inner,
        // Sexp::X_variant(inner))` on the subset closed set. A future
        // arm added to `UnquoteForm` extends [`UnquoteForm::ALL`] +
        // [`UnquoteForm::to_quote_form`] + this sweep in lockstep —
        // rustc-enforced through the closed-set exhaustiveness across
        // the `UnquoteForm` match. Sibling of the outer `Sexp`
        // construct family's
        // `sexp_quote_family_constructors_round_trip_through_as_quote_form`
        // (commit 38f076b) — same posture on the subset closed set,
        // routed through `Sexp::as_unquote` rather than
        // `Sexp::as_quote_form`.
        use crate::ast::Sexp;
        let inners = [
            Sexp::Nil,
            Sexp::symbol("x"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(1),
            Sexp::List(vec![Sexp::symbol("f")]),
        ];
        for uf in UnquoteForm::ALL {
            for inner in &inners {
                let wrapped = uf.wrap(inner.clone());
                assert_eq!(
                    wrapped.as_unquote(),
                    Some((uf, inner)),
                    "UnquoteForm::{uf:?}.wrap({inner:?}).as_unquote() failed to round-trip — the (construct, project) pair on the subset algebra is not a section-for-retraction of Sexp::as_unquote",
                );
            }
        }
    }

    #[test]
    fn unquote_form_wrap_composes_with_shape_via_to_quote_form_sexp_shape() {
        // Outer-shape composition pin: for every `uf: UnquoteForm`
        // and every representative `inner: Sexp`, `uf.wrap(inner)
        // .shape() == uf.to_quote_form().sexp_shape()` — the
        // (subset marker, outer [`crate::error::SexpShape`]) pairing
        // binds through the SAME closed-set composition
        // (`to_quote_form` → `QuoteForm::sexp_shape`) that
        // [`crate::ast::QuoteForm::wrap`]'s outer-shape composition
        // rides on. A regression that drifts ONE construct arm's
        // outer-shape from the superset's `sexp_shape` while
        // preserving the tuple-variant emission surfaces here
        // alongside the round-trip pin. Sibling of the outer `Sexp`
        // construct family's
        // `sexp_quote_family_constructors_compose_with_shape_via_quote_form_sexp_shape`
        // (commit 38f076b) — same posture on the subset closed set.
        use crate::ast::Sexp;
        let inners = [
            Sexp::Nil,
            Sexp::symbol("x"),
            Sexp::int(42),
            Sexp::List(vec![Sexp::symbol("f")]),
        ];
        for uf in UnquoteForm::ALL {
            for inner in &inners {
                assert_eq!(
                    uf.wrap(inner.clone()).shape(),
                    uf.to_quote_form().sexp_shape(),
                    "UnquoteForm::{uf:?}.wrap({inner:?}).shape() drifted from .to_quote_form().sexp_shape() — the (subset marker, outer SexpShape) pairing is no longer derived from the superset's canonical composition",
                );
            }
        }
    }

    // --- MacroDefHead closed-set FromStr + UnknownMacroDefHead lift ---
    //
    // Same posture as `unquote_form_*` / `sexp_shape_*` /
    // `atom_kind_*` / `quote_form_*`: pin the four contract laws of the
    // closed-set (ALL, projection, FromStr, Unknown*) quadruple so the
    // typed surface and the rendered diagnostic literal cannot drift,
    // and the open-by-design `from_keyword` face matches the
    // typed-error `FromStr` face byte-for-byte (the same closed-set
    // sweep, different rejection polarities).

    #[test]
    fn macro_def_head_all_is_unique_and_complete() {
        // Closed-set posture: `ALL` enumerates every reachable variant
        // EXACTLY ONCE — no duplicates, no omissions. The `[Self; 3]`
        // array literal in the declaration forces the arity at compile
        // time; this test catches the orthogonal failure modes — a
        // future variant added at the type without being added to ALL
        // (silently dropped from every consumer's sweep), or a typo
        // that duplicates an entry (silently double-counted). Same
        // truth-table pinning every sibling closed-set lift in the
        // workspace uses (SexpShape::ALL, AtomKind::ALL,
        // QuoteForm::ALL, UnquoteForm::ALL, RequestorKind::ALL,
        // ReceiptKind::ALL, ConditionKind::ALL, …).
        //
        // The `iter+map+collect+sort_unstable` quadruple this test
        // inlined pre-lift now binds at `<MacroDefHead as
        // ClosedSet>::sorted_labels()` — the canonical-ordered
        // candidate-list projection on the trait. Distinctness of the
        // sorted result is covered by
        // `assert_closed_set_well_formed::<MacroDefHead>()`.
        assert_eq!(MacroDefHead::ALL.len(), 3);
        assert_eq!(
            <MacroDefHead as crate::ClosedSet>::sorted_labels(),
            vec!["defcheck", "defmacro", "defpoint-template"],
            "MacroDefHead::ALL must cover all three macro-definition heads"
        );
    }

    #[test]
    fn macro_def_head_keyword_round_trips_through_from_str() {
        // Bidirectional `keyword` ↔ `FromStr` contract: for every
        // variant in ALL, `head.keyword().parse() == Ok(head)`. A
        // regression that drifts the (variant, literal) pairing at
        // ONE arm of `keyword` (e.g. typo `defpoint_template` with an
        // underscore instead of `defpoint-template` with a hyphen) OR
        // at the `FromStr` decode body (off-by-one, missing variant in
        // the sweep) fails-loudly here. The canonical-literal site is
        // singular (`keyword`) so the round-trip is the only way the
        // typed surface and the rendered diagnostic literal can drift
        // apart — pinning it here means they cannot.
        for head in MacroDefHead::ALL {
            let parsed: MacroDefHead = head
                .keyword()
                .parse()
                .expect("every ALL variant's keyword must round-trip through FromStr");
            assert_eq!(
                parsed,
                head,
                "FromStr({}) must round-trip to the same variant",
                head.keyword()
            );
        }
    }

    #[test]
    fn macro_def_head_from_keyword_matches_from_str_for_every_input() {
        // Cross-face contract: the `Option`-faced `from_keyword`
        // projection (`tatara_lisp::ast::Sexp::as_call_to_any`'s
        // decoder slot, signature `Fn(&str) -> Option<T>`) and the
        // typed-error-faced `FromStr` projection are the SAME closed-
        // set sweep with different rejection polarities. After the
        // lift `from_keyword` delegates to `parse().ok()`, so the
        // closed-set sweep lives at ONE site (the `FromStr` impl) and
        // both faces project the same accept/reject decision at every
        // input. Pinning this law means a future refactor that drifts
        // ONE face from the other (e.g., adding a fourth variant to
        // `keyword` but forgetting to bump `Self::ALL`, or branching
        // `from_keyword` against a hand-rolled match arm instead of
        // routing through the typed sweep) fails here.
        let inputs: &[&str] = &[
            // The three canonical heads — both faces accept.
            "defmacro",
            "defpoint-template",
            "defcheck",
            // Non-canonical capitalizations — both faces reject.
            "Defmacro",
            "DEFCHECK",
            "DefpointTemplate",
            // Near misses — both faces reject.
            "defmacroo",
            "defcheckk",
            "defpoint_template",
            "defpoint",
            "defpoint-templates",
            // Sibling authoring surfaces from other closed sets —
            // both faces reject (cross-set disjointness).
            "defmonitor",
            "defnotify",
            "defpoint",
            "defalertpolicy",
            // SexpShape labels (the structural-identity vocabulary on
            // a DIFFERENT axis) — both faces reject.
            "symbol",
            "list",
            "nil",
            // Punctuation from QuoteForm / UnquoteForm vocabularies —
            // both faces reject (closed sets live on disjoint axes).
            ",",
            ",@",
            "'",
            "`",
            // Edge cases — both faces reject.
            "",
            " ",
            " defmacro",
            "defmacro ",
        ];
        for s in inputs {
            let from_kw = MacroDefHead::from_keyword(s);
            let from_str = s.parse::<MacroDefHead>().ok();
            assert_eq!(
                from_kw,
                from_str,
                "`from_keyword` and `FromStr` must agree on {s:?}: from_keyword={from_kw:?}, FromStr={from_str:?}",
            );
        }
    }

    #[test]
    fn unknown_macro_def_head_carries_offending_input_verbatim() {
        // Operator-facing diagnostic contract: the offending input
        // lands in the typed error verbatim — no normalization, no
        // truncation, no whitespace coercion. Pin the exact
        // `#[error(...)]` rendering AND the typed `.0` field
        // projection so a future refactor that normalizes (e.g.
        // `.trim()` / `.to_ascii_lowercase()`) before building the
        // error or that drops the input fails-loudly here. Symmetric
        // to every sibling `Unknown*` carrier in the workspace
        // ([`UnknownSexpShape`], [`crate::ast::UnknownAtomKind`],
        // [`UnknownUnquoteForm`], [`crate::ast::UnknownQuoteForm`],
        // `tatara_process::allocation::UnknownRequestorKind`, …).
        let err: UnknownMacroDefHead = "Defmacro"
            .parse::<MacroDefHead>()
            .expect_err("capitalized `Defmacro` is not a canonical macro-definition head");
        assert_eq!(err.0, "Defmacro");
        assert_eq!(format!("{err}"), "unknown macro definition head: Defmacro");

        let err: UnknownMacroDefHead = "defmacroo"
            .parse::<MacroDefHead>()
            .expect_err("`defmacroo` is not a canonical macro-definition head");
        assert_eq!(err.0, "defmacroo");
        assert_eq!(format!("{err}"), "unknown macro definition head: defmacroo");

        let err: UnknownMacroDefHead = ""
            .parse::<MacroDefHead>()
            .expect_err("empty input must NOT decode to a MacroDefHead");
        assert_eq!(err.0, "");
        assert_eq!(format!("{err}"), "unknown macro definition head: ");

        // Whitespace-padded canonical keyword MUST reject — the
        // typed identity is byte-exact, the offending input is
        // returned verbatim with its padding intact (not trimmed).
        // The rendered diagnostic preserves the leading space, so
        // the bad value reaches the operator unmolested.
        let err: UnknownMacroDefHead = " defmacro"
            .parse::<MacroDefHead>()
            .expect_err("leading-space `defmacro` must reject — typed identity is byte-exact");
        assert_eq!(err.0, " defmacro");
        assert_eq!(
            format!("{err}"),
            "unknown macro definition head:  defmacro",
            "leading whitespace is preserved verbatim in the rendered diagnostic",
        );
    }

    #[test]
    fn macro_def_head_from_str_rejects_cross_axis_vocabularies() {
        // Cross-axis guard: a MacroDefHead is the head keyword of a
        // `(defmacro …)`-shaped form — distinct from every other
        // closed-set vocabulary in this crate. A `FromStr` that
        // silently accepted a SexpShape label, an UnquoteForm marker,
        // a QuoteForm prefix, or an AtomKind label would corrupt the
        // typed identity at the macro-definition-head boundary. Pin
        // BOTH directions: the three canonical keywords decode
        // through MacroDefHead, the sibling closed-set vocabularies
        // do NOT.
        assert_eq!(
            "defmacro".parse::<MacroDefHead>().unwrap(),
            MacroDefHead::Defmacro
        );
        assert_eq!(
            "defpoint-template".parse::<MacroDefHead>().unwrap(),
            MacroDefHead::DefpointTemplate
        );
        assert_eq!(
            "defcheck".parse::<MacroDefHead>().unwrap(),
            MacroDefHead::Defcheck
        );

        // SexpShape labels — the structural-identity vocabulary
        // (twelve outer Sexp shapes) — share NO labels with the
        // macro-definition-head vocabulary. All must reject.
        for shape in SexpShape::ALL {
            assert!(
                shape.label().parse::<MacroDefHead>().is_err(),
                "SexpShape::{shape:?} label `{}` must NOT decode as a MacroDefHead",
                shape.label(),
            );
        }

        // UnquoteForm punctuation markers (`,` / `,@`) belong to the
        // template-marker axis — they MUST reject here because
        // MacroDefHead is on the symbol-keyword axis.
        for form in UnquoteForm::ALL {
            assert!(
                form.marker().parse::<MacroDefHead>().is_err(),
                "UnquoteForm marker `{}` must NOT decode as a MacroDefHead",
                form.marker(),
            );
        }

        // The `defpoint` authoring-surface keyword names a
        // `(defpoint …)` definition form — NOT a definition-template
        // form. The two are intentionally disjoint: `defpoint` is a
        // tatara-process domain authoring head, `defpoint-template`
        // is a parameterized-template head on the same surface.
        // Pinning the rejection here keeps the two from drifting.
        assert!("defpoint".parse::<MacroDefHead>().is_err());
        assert!("defmonitor".parse::<MacroDefHead>().is_err());
        assert!("defnotify".parse::<MacroDefHead>().is_err());
        assert!("defalertpolicy".parse::<MacroDefHead>().is_err());

        // Common-Lisp-style aliases the substrate does NOT admit.
        // Pinning these rejects keeps a future refactor from
        // silently extending the vocabulary without bumping
        // `Self::ALL` first.
        assert!("def-macro".parse::<MacroDefHead>().is_err());
        assert!("defun".parse::<MacroDefHead>().is_err());
        assert!("define-syntax".parse::<MacroDefHead>().is_err());
    }

    #[test]
    fn macro_def_head_is_well_formed_closed_set() {
        // Structural contract: MacroDefHead's three variants are
        // pairwise distinct, round-trip through the trait's `label` ↔
        // `parse_label`, and reject the empty string — the
        // workspace-wide `assert_closed_set_well_formed::<T>()` testkit
        // pinned across every `tatara-process` closed-set implementor
        // AND every prior tatara-lisp retrofit (`AtomKind`, `QuoteForm`).
        // The substrate-level assertion runs on the auto-derived
        // `impl ClosedSet for MacroDefHead` emitted by
        // `#[derive(tatara_lisp_derive::ClosedSet)]` — a regression
        // that drifts the derive's `make_unknown` delegation, the
        // `via = "keyword"` projection (`"defmacro"` /
        // `"defpoint-template"` / `"defcheck"`), or the variant
        // listing forced through `Self::ALL` fails-loudly here in
        // isolation from the per-variant truth tables above.
        crate::assert_closed_set_well_formed::<MacroDefHead>();
    }

    #[test]
    fn unquote_form_is_well_formed_closed_set() {
        // Structural contract: UnquoteForm's two variants are pairwise
        // distinct, round-trip through the trait's `label` ↔
        // `parse_label`, and reject the empty string. The substrate-
        // level assertion runs on the auto-derived `impl ClosedSet
        // for UnquoteForm` emitted by
        // `#[derive(tatara_lisp_derive::ClosedSet)]` — a regression
        // that drifts the `via = "marker"` projection (`","` /
        // `",@"`) or conflates the punctuation-axis vocabulary with
        // the structural-axis SexpShape vocabulary
        // (`"unquote"` / `"unquote-splice"`) fails-loudly here. The
        // cross-axis disjointness check stays at
        // `unquote_form_from_str_rejects_sexp_shape_labels_on_template_marker_axis`;
        // THIS test pins the in-axis well-formedness floor.
        crate::assert_closed_set_well_formed::<UnquoteForm>();
    }

    #[test]
    fn kwarg_path_kind_is_well_formed_closed_set() {
        // Structural contract: KwargPathKind's three variants are
        // pairwise distinct, round-trip through the trait's `label` ↔
        // `parse_label`, and reject the empty string. The substrate-
        // level assertion runs on the auto-derived `impl ClosedSet
        // for KwargPathKind` emitted by
        // `#[derive(tatara_lisp_derive::ClosedSet)]` — a regression
        // that drifts the `via = "label"` projection (`"named"` /
        // `"item"` / `"slot"`) or the variant listing forced through
        // `Self::ALL` fails-loudly here.
        crate::assert_closed_set_well_formed::<KwargPathKind>();
    }

    #[test]
    fn expected_kwarg_shape_is_well_formed_closed_set() {
        // Structural contract: ExpectedKwargShape's seven variants are
        // pairwise distinct, round-trip through the trait's `label` ↔
        // `parse_label`, and reject the empty string. The substrate-
        // level assertion runs on the auto-derived `impl ClosedSet
        // for ExpectedKwargShape` emitted by
        // `#[derive(tatara_lisp_derive::ClosedSet)]` — a regression
        // that drifts the `via = "label"` projection (`"keyword"` /
        // `"string"` / `"int"` / `"number"` / `"bool"` / `"list"` /
        // `"list of strings"`) fails-loudly here.
        crate::assert_closed_set_well_formed::<ExpectedKwargShape>();
    }

    #[test]
    fn sexp_shape_is_well_formed_closed_set() {
        // Structural contract: SexpShape's twelve variants are
        // pairwise distinct, round-trip through the trait's `label` ↔
        // `parse_label`, and reject the empty string. The substrate-
        // level assertion runs on the auto-derived `impl ClosedSet
        // for SexpShape` emitted by
        // `#[derive(tatara_lisp_derive::ClosedSet)]` — a regression
        // that drifts the `via = "label"` projection (`"nil"` /
        // `"symbol"` / `"keyword"` / `"string"` / `"int"` / `"float"`
        // / `"bool"` / `"list"` / `"quote"` / `"quasiquote"` /
        // `"unquote"` / `"unquote-splice"`) or the variant listing
        // forced through `Self::ALL` (cardinality 12 = every reachable
        // outer `Sexp` shape) fails-loudly here.
        crate::assert_closed_set_well_formed::<SexpShape>();
    }

    #[test]
    fn compiler_spec_io_stage_is_well_formed_closed_set() {
        // Structural contract: CompilerSpecIoStage's four variants are
        // pairwise distinct, round-trip through the trait's `label` ↔
        // `parse_label`, and reject the empty string. The substrate-
        // level assertion runs on the auto-derived `impl ClosedSet
        // for CompilerSpecIoStage` emitted by
        // `#[derive(tatara_lisp_derive::ClosedSet)]` — a regression
        // that drifts the `via = "label"` projection (`"serialize"` /
        // `"write"` / `"read"` / `"deserialize"`) or the variant
        // listing forced through `Self::ALL` (cardinality 4 = every
        // reachable disk-persistence (operation, stage) pair) fails-
        // loudly here.
        //
        // Distinct from the sibling closed-set well-formedness assertions
        // above: this enum carries `#[closed_set(no_from_str)]`, so the
        // trait surface's `parse_label` keys on the SINGULAR label
        // (`"read"` → `LoadFromDiskRead`) while the inherent
        // `std::str::FromStr` impl below keys on the COMPOUND key
        // (`"load_from_disk: read"` → `LoadFromDiskRead`). The trait
        // surface's well-formedness floor — non-empty `ALL`, round-trip
        // through `label`, pairwise-distinct labels, empty-string
        // outside the set — STILL holds on the singular projection
        // because the four labels (`"serialize"` / `"write"` /
        // `"read"` / `"deserialize"`) ARE bijective with the four
        // variants by accident of the current closed set; the
        // compound-key shape is what `FromStr` enforces and is what
        // `compiler_spec_io_stage_compound_key_round_trips_through_from_str`
        // pins. The two surfaces are deliberately disjoint — the
        // `no_from_str` axis exists for exactly this case.
        crate::assert_closed_set_well_formed::<CompilerSpecIoStage>();
    }
}
