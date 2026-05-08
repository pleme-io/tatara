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
    /// `expected` is `&'static str` so a typo can never drift into the
    /// diagnostic at runtime; `got` is `&'static str` because it is
    /// always the output of `crate::domain::sexp_type_name`, whose match
    /// is exhaustive over `Sexp` at compile time. When a future run gives
    /// `Sexp` source spans, `pos: Option<usize>` lands here in ONE place
    /// and every type-mismatch site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {form}: expected {expected}, got {got}")]
    TypeMismatch {
        form: String,
        expected: &'static str,
        got: &'static str,
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
    /// `prefix` is `&'static str` because every call site passes a literal
    /// (`","` or `",@"`); a typo in the marker can never drift into the
    /// diagnostic at runtime — the type system is the floor. `name` and
    /// `hint` are `String` because they come from arbitrary source / the
    /// live bindings set. When a future run gives `Sexp` source spans, `pos:
    /// Option<usize>` lands here in ONE place and every unbound-template-var
    /// site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    ///
    /// Display matches the legacy `Compile`-shaped diagnostic byte-for-byte
    /// when `hint` is `None` — `"compile error in {prefix}{name}: unbound"`
    /// — so existing consumer assertions that pattern-match on the message
    /// substring keep passing. With a hint, the suffix `"; did you mean
    /// {prefix}{hint}?"` is appended; the prefix is preserved in the hint so
    /// the operator can copy-paste the suggestion verbatim.
    #[error(
        "compile error in {prefix}{name}: unbound{}",
        unbound_hint_suffix(prefix, hint.as_deref())
    )]
    UnboundTemplateVar {
        prefix: &'static str,
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
    /// `prefix` is `&'static str` because every call site passes a
    /// literal (`","` / `",@"`); a typo in the marker can never drift
    /// into the diagnostic at runtime — the type system is the floor.
    /// `got` is `String` because it comes from arbitrary source via
    /// `Sexp::Display`. When a future run gives `Sexp` source spans,
    /// `pos: Option<usize>` lands here in ONE place and every
    /// non-symbol-unquote-target site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {prefix}: expected symbol, got {got}")]
    NonSymbolUnquoteTarget { prefix: &'static str, got: String },
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
    /// `got` is `String` because it comes from arbitrary source via
    /// `Sexp::Display` (the offending inner — `xs`, `(list 1 2)`, `5`,
    /// `:foo`, etc.). Naming both the failure mode AND the offending element
    /// is the typed-entry gate's structural-completeness floor (THEORY.md
    /// §V.1) — without it the operator must re-read the source to find what
    /// actually misfired. When a future run gives `Sexp` source spans, `pos:
    /// Option<usize>` lands here in ONE place and every splice-outside-list
    /// site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    ///
    /// Display renders `"compile error in ,@: \`,@\` may only appear inside
    /// a list (got ,@{got})"` — the legacy substring `"\`,@\` may only
    /// appear inside a list"` is preserved verbatim so authoring tools that
    /// substring-match on the rendered diagnostic see no drift; the
    /// parenthetical `(got ,@{got})` names the offending form so an LSP
    /// quick-fix that surfaces "the splice has no containing list; you
    /// wrote `,@xs`" gains the literal value as data, no message re-parsing
    /// required.
    #[error("compile error in ,@: `,@` may only appear inside a list (got ,@{got})")]
    SpliceOutsideList { got: String },
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
    /// `parse_params`; `got` is `String` because it comes from arbitrary
    /// source via `Sexp::Display`. Display preserves the legacy
    /// `"compile error in defmacro params: expected symbol"` prefix
    /// byte-for-byte so authoring tools that substring-grep on the
    /// rendered diagnostic see no drift; the structural detail (`at
    /// position {position}, got {got}`) is appended. When a future run
    /// gives `Sexp` source spans, `pos: Option<usize>` lands here in ONE
    /// place and every non-symbol-param site picks up positional
    /// rendering via `crate::diagnostic::format_diagnostic`
    /// mechanically.
    #[error(
        "compile error in defmacro params: expected symbol at position \
         {position}, got {got}"
    )]
    NonSymbolParam { position: usize, got: String },
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
    /// `Sexp::Display` projection (`Some("5")`, `Some(":foo")`,
    /// `Some("(nested)")`) or `None` when the marker was the last
    /// element in the list and nothing followed at all. Naming both
    /// the marker position AND the offending follower (or its absence)
    /// is the typed-entry gate's structural-completeness floor
    /// (THEORY.md §V.1) — without both, an LSP that wants to surface
    /// "your `&rest` at param-list position 1 has no name; you wrote
    /// `5` instead of a symbol" must re-parse the source.
    ///
    /// Sibling of `NonSymbolParam { position, got }` for the
    /// defmacro-syntax-gate's other definition-site failure mode —
    /// that variant fires when a NON-`&rest` element at a param
    /// position isn't a symbol; this variant fires specifically on the
    /// post-`&rest` follower slot, where the failure mode bifurcates
    /// into "missing entirely" vs. "present but not a symbol". Both
    /// modes share ONE structural variant via `got: Option<String>`
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
    /// `got` is `Option<String>` because the follower comes from
    /// arbitrary source via `Sexp::Display` (when present) or doesn't
    /// exist at all (when the marker was the param list's last
    /// element). Display preserves the legacy `"compile error in
    /// defmacro params: &rest needs a name"` prefix byte-for-byte so
    /// authoring tools that substring-grep on the rendered diagnostic
    /// see no drift; the structural detail (`(rest marker at position
    /// {rest_position}, got {got})` when present, `(rest marker at
    /// position {rest_position}, none provided)` when absent) is
    /// appended. When a future run gives `Sexp` source spans, `pos:
    /// Option<usize>` lands here in ONE place and every
    /// rest-param-missing-name site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error(
        "compile error in defmacro params: &rest needs a name{}",
        rest_param_missing_name_suffix(*rest_position, got.as_deref())
    )]
    RestParamMissingName {
        rest_position: usize,
        got: Option<String>,
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
    /// `head` is `&'static str` because every call site projects
    /// through the `matches!("defmacro" | "defpoint-template" |
    /// "defcheck")` gate immediately above — the head is always one
    /// of three known literals at that point; using a static slot
    /// makes that compile-time guarantee load-bearing in the type
    /// system (a typo in the head literal can never drift into the
    /// diagnostic at runtime — the type system is the floor, same
    /// posture as `TypeMismatch.expected` and `HeadMismatch.keyword`).
    /// `arity` is `usize` because it is always `list.len()` at the
    /// call site (the length of the form including the head
    /// element).
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
    DefmacroArity { head: &'static str, arity: usize },
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
    /// `head` is `&'static str` because every call site projects
    /// through the `matches!("defmacro" | "defpoint-template" |
    /// "defcheck")` gate immediately above — the head is always one
    /// of three known literals at that point; using a static slot
    /// makes that compile-time guarantee load-bearing in the type
    /// system (a typo in the head literal can never drift into the
    /// diagnostic at runtime — the type system is the floor, same
    /// posture as `TypeMismatch.expected`, `HeadMismatch.keyword`,
    /// and `DefmacroArity.head`). `got` is `String` because it
    /// comes from arbitrary source via `Sexp::Display` (e.g. `5`,
    /// `:foo`, `"name"`, `(nested)`).
    ///
    /// Display preserves the legacy `"expected name symbol"` substring
    /// byte-for-byte: the prefix `compile error in {head}:` matches
    /// the legacy `Compile { form: head.to_string(), message:
    /// "expected name symbol" }` shape; the structural detail (`,
    /// got {got}`) is appended. When a future run gives `Sexp` source
    /// spans, `pos: Option<usize>` lands here in ONE place and every
    /// non-symbol-name site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {head}: expected name symbol, got {got}")]
    DefmacroNonSymbolName { head: &'static str, got: String },
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
    /// `head` is `&'static str` because every call site projects
    /// through the `matches!("defmacro" | "defpoint-template" |
    /// "defcheck")` gate immediately above — the head is always one of
    /// three known literals at that point; using a static slot makes
    /// that compile-time guarantee load-bearing in the type system (a
    /// typo in the head literal can never drift into the diagnostic at
    /// runtime — the type system is the floor, same posture as
    /// `TypeMismatch.expected`, `HeadMismatch.keyword`,
    /// `DefmacroArity.head`, and `DefmacroNonSymbolName.head`). `got`
    /// is `String` because it comes from arbitrary source via
    /// `Sexp::Display` (e.g. `x`, `5`, `:foo`, `"params"`).
    ///
    /// Display preserves the legacy `"expected param list"` substring
    /// byte-for-byte: the prefix `compile error in {head}:` matches
    /// the legacy `Compile { form: head.to_string(), message:
    /// "expected param list" }` shape; the structural detail (`, got
    /// {got}`) is appended. When a future run gives `Sexp` source
    /// spans, `pos: Option<usize>` lands here in ONE place and every
    /// non-list-params site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {head}: expected param list, got {got}")]
    DefmacroNonListParams { head: &'static str, got: String },
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
    /// `Defmacro*.head` family). `got` is `Option<String>` because
    /// the offending head comes from arbitrary source via
    /// `Sexp::Display` (when present) or doesn't exist at all (when
    /// the list is empty).
    ///
    /// Display preserves the legacy `"missing head symbol"` substring
    /// AND the `"compile error in {keyword}:"` prefix byte-for-byte —
    /// `"compile error in {keyword}: missing head symbol"` is the
    /// stable prefix; the structural detail (`(empty list)` for
    /// `None`, `(got {g})` for `Some(g)`) is appended in a
    /// parenthetical, parallel to how `RestParamMissingName` appends
    /// `(rest marker at position {n}, got {g})` /
    /// `(rest marker at position {n}, none provided)` and how
    /// `SpliceOutsideList` appends `(got ,@{got})`. When a future
    /// run gives `Sexp` source spans, `pos: Option<usize>` lands
    /// here in ONE place and every missing-head-symbol site picks up
    /// positional rendering via `crate::diagnostic::format_diagnostic`
    /// mechanically.
    #[error(
        "compile error in {keyword}: missing head symbol{}",
        missing_head_symbol_suffix(got.as_deref())
    )]
    MissingHeadSymbol {
        keyword: &'static str,
        got: Option<String>,
    },
}

fn unbound_hint_suffix(prefix: &str, hint: Option<&str>) -> String {
    match hint {
        Some(h) => format!("; did you mean {prefix}{h}?"),
        None => String::new(),
    }
}

fn unknown_kwarg_suffix(hint: Option<&str>, allowed: &[String]) -> String {
    let allowed_list = allowed
        .iter()
        .map(|s| format!(":{s}"))
        .collect::<Vec<_>>()
        .join(", ");
    match hint {
        Some(h) => format!(" (did you mean :{h}?; allowed: {allowed_list})"),
        None => format!(" (allowed: {allowed_list})"),
    }
}

fn rest_param_missing_name_suffix(rest_position: usize, got: Option<&str>) -> String {
    match got {
        Some(g) => format!(" (rest marker at position {rest_position}, got {g})"),
        None => format!(" (rest marker at position {rest_position}, none provided)"),
    }
}

fn missing_head_symbol_suffix(got: Option<&str>) -> String {
    match got {
        Some(g) => format!(" (got {g})"),
        None => " (empty list)".into(),
    }
}

fn unknown_domain_keyword_suffix(hint: Option<&str>, registered: &[String]) -> String {
    if registered.is_empty() {
        return match hint {
            Some(h) => format!(" (did you mean ({h} ...)?; no domains registered)"),
            None => " (no domains registered)".into(),
        };
    }
    let registered_list = registered.join(", ");
    match hint {
        Some(h) => format!(" (did you mean ({h} ...)?; registered: {registered_list})"),
        None => format!(" (registered: {registered_list})"),
    }
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
            | Self::NonSymbolParam { .. }
            | Self::RestParamMissingName { .. }
            | Self::DefmacroArity { .. }
            | Self::DefmacroNonSymbolName { .. }
            | Self::DefmacroNonListParams { .. }
            | Self::NotAListForm { .. }
            | Self::MissingHeadSymbol { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LispError;

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
                form: ":x".into(),
                expected: "string",
                got: "int",
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
                prefix: ",",
                name: "xx".into(),
                hint: Some("x".into()),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::UnboundTemplateVar {
                prefix: ",@",
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
                prefix: ",",
                got: "(list 1 2)".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NonSymbolUnquoteTarget {
                prefix: ",@",
                got: "5".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::SpliceOutsideList { got: "xs".into() }.position(),
            None
        );
        assert_eq!(
            LispError::SpliceOutsideList {
                got: "(list 1 2)".into(),
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
            LispError::NonSymbolParam {
                position: 0,
                got: "5".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::NonSymbolParam {
                position: 2,
                got: "(nested)".into(),
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
                got: Some("5".into()),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DefmacroArity {
                head: "defmacro",
                arity: 1,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DefmacroArity {
                head: "defcheck",
                arity: 3,
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DefmacroNonSymbolName {
                head: "defmacro",
                got: "5".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DefmacroNonListParams {
                head: "defmacro",
                got: "x".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DefmacroNonListParams {
                head: "defcheck",
                got: ":foo".into(),
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::DefmacroNonSymbolName {
                head: "defpoint-template",
                got: ":foo".into(),
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
                got: Some("5".into()),
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
            got: Some("5".into()),
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
            got: Some(":foo".into()),
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
            got: Some("\"name\"".into()),
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
            got: Some("(nested)".into()),
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
            prefix: ",",
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
            prefix: ",",
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
    fn non_symbol_unquote_target_display_renders_canonical_type_mismatch_shape() {
        // `,(list 1 2)` — the inner is a list, not a symbol. The variant
        // names the syntactic marker (`,`), the expected shape (`symbol` —
        // the only form a no-evaluator template can substitute), and the
        // offending literal (`(list 1 2)`) as first-class fields. Authoring
        // tools that pattern-match on the variant gain structural binding;
        // tools that substring-match on the rendered diagnostic see a
        // stable shape parallel to the existing `TypeMismatch` variant.
        let err = LispError::NonSymbolUnquoteTarget {
            prefix: ",",
            got: "(list 1 2)".into(),
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
            prefix: ",@",
            got: "5".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@: expected symbol, got 5"
        );
    }

    #[test]
    fn non_symbol_unquote_target_display_carries_keyword_atom_unchanged() {
        // `,:foo` — the inner is a keyword atom. The `:foo` form
        // round-trips through `Sexp::Display` into the variant's `got`
        // slot unchanged, so the operator sees what they wrote.
        let err = LispError::NonSymbolUnquoteTarget {
            prefix: ",",
            got: ":foo".into(),
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
        // first-class field so authoring tools (REPL, LSP, `tatara-check`)
        // gain structural binding; tools that substring-match on the
        // rendered diagnostic still see the legacy `"\`,@\` may only appear
        // inside a list"` substring verbatim.
        let err = LispError::SpliceOutsideList { got: "xs".into() };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@: `,@` may only appear inside a list (got ,@xs)"
        );
    }

    #[test]
    fn splice_outside_list_display_carries_list_literal_unchanged() {
        // The offending inner is a list literal — `,@(list 1 2)` — so the
        // operator sees the literal value they wrote in the parenthetical,
        // not just a type-name. Round-trips through `Sexp::Display`.
        let err = LispError::SpliceOutsideList {
            got: "(list 1 2)".into(),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@: `,@` may only appear inside a list (got ,@(list 1 2))"
        );
    }

    #[test]
    fn splice_outside_list_display_carries_kebab_case_symbol_unchanged() {
        // `,@notify-ref` — kebab-cased symbol round-trips through the
        // variant's `got` slot unchanged. Pinning this contract means a
        // regression that camelCases or lowercases the offending form fails
        // -loudly here.
        let err = LispError::SpliceOutsideList {
            got: "notify-ref".into(),
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
        let err = LispError::SpliceOutsideList { got: "xs".into() };
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
            got: "5".into(),
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
            got: "(nested)".into(),
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
            got: ":k".into(),
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
            got: Some("5".into()),
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
            got: Some(":foo".into()),
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
            head: "defmacro",
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
            head: "defpoint-template",
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
            head: "defcheck",
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
            head: "defmacro",
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
            head: "defmacro",
            got: "5".into(),
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
            head: "defpoint-template",
            got: ":foo".into(),
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
            head: "defcheck",
            got: "(nested)".into(),
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
            head: "defmacro",
            got: "\"name\"".into(),
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
            head: "defmacro",
            got: "5".into(),
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
            head: "defmacro",
            got: "x".into(),
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
            head: "defpoint-template",
            got: "5".into(),
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
            head: "defcheck",
            got: ":k".into(),
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
            head: "defmacro",
            got: "\"params\"".into(),
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
            head: "defmacro",
            got: "x".into(),
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
            prefix: ",@",
            name: "rsts".into(),
            hint: Some("rest".into()),
        };
        assert_eq!(
            format!("{err}"),
            "compile error in ,@rsts: unbound; did you mean ,@rest?"
        );
    }
}
