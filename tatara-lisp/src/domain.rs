//! `TataraDomain` — a Rust type authorable as a Lisp `(<keyword> :k v …)` form.
//!
//! Apply `#[derive(TataraDomain)]` (from `tatara-lisp-derive`) and a plain
//! struct gains a full Lisp compiler: keyword dispatch, kwarg parsing, typed
//! field extraction.
//!
//! Also exposes a `DomainRegistry` + `linkme`-free `register_domain!` macro
//! so any crate that derives `TataraDomain` can auto-register itself; the
//! dispatcher then looks up unknown top-level forms by keyword at runtime.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::de::DeserializeOwned;

use crate::ast::{Atom, Sexp};
use crate::error::{
    ExpectedKwargShape, KwargPath, LispError, NumericLiteral, NumericWidth, Result, SexpShape,
    SexpWitness,
};

/// A Rust type compilable from a Lisp form.
pub trait TataraDomain: Sized {
    /// The Lisp keyword (e.g., `"defmonitor"`).
    const KEYWORD: &'static str;

    /// Parse the argument list (everything after the keyword) into Self.
    fn compile_from_args(args: &[Sexp]) -> Result<Self>;

    /// Parse a complete form; validates the head symbol matches `KEYWORD`.
    fn compile_from_sexp(form: &Sexp) -> Result<Self> {
        let list = form
            .as_list()
            .ok_or_else(|| not_a_list_form_err(Self::KEYWORD))?;
        // The two sub-modes of "head can't be projected to a symbol" — empty
        // list (`first()` is `None`) vs. present-but-not-a-symbol
        // (`as_symbol()` is `None`) — share ONE structural variant
        // (`MissingHeadSymbol { keyword, got }`) but bind to distinct
        // `got` payloads (`None` vs. `Some(<sexp display>)`). This lets
        // an authoring tool render "your form is empty" vs. "your
        // form's head is `5`, not a symbol" without re-parsing the
        // source — the legacy `Compile`-shaped diagnostic collapsed
        // both into one message.
        let head_sexp = list
            .first()
            .ok_or_else(|| missing_head_err(Self::KEYWORD, None))?;
        let head = head_sexp
            .as_symbol()
            .ok_or_else(|| missing_head_err(Self::KEYWORD, Some(head_sexp.witness())))?;
        if head != Self::KEYWORD {
            return Err(head_mismatch(Self::KEYWORD, head.to_string()));
        }
        Self::compile_from_args(&list[1..])
    }
}

// ── compile_from_sexp diagnostics — the form-shape gate primitives ─
//
// `compile_from_sexp` (the trait default) gates every `TataraDomain`
// invocation that takes a complete `(KEYWORD …)` form: ProcessSpec,
// MonitorSpec, AlertPolicySpec, every hand-written impl. Three failure
// modes — not a list, missing head symbol, wrong head — used to be
// inline `LispError::Compile { form: KEYWORD.to_string(), message: …}`
// triples in the trait default. The three-times-rule signal
// (THEORY.md §VI.1) calls for one named primitive per shape; these
// are them.
//
// All three are now structural: `not_a_list_form_err` returns
// `LispError::NotAListForm`, `missing_head_err` returns
// `LispError::MissingHeadSymbol { keyword, got }` (`got: None` for
// empty list, `got: Some(<sexp display>)` for present-but-not-symbol),
// and `head_mismatch` returns `LispError::HeadMismatch`. Each carries
// its distinguishing data (the offending head's display projection,
// the keyword) as first-class variant fields so authoring tools
// pattern-match structurally instead of substring-grepping the
// rendered message. The entire `compile_from_sexp` rejection chain
// — bare-atom → empty/not-symbol head → wrong-keyword head — is
// closed: every distinct typed-entry rejection at the form-shape
// gate binds to ONE structural variant of `LispError`.

/// `T::compile_from_sexp` was passed something that isn't a list.
/// One named primitive every TataraDomain impl shares — returns the
/// dedicated `LispError::NotAListForm { keyword }` variant so
/// authoring surfaces (REPL, LSP, `tatara-check`) bind to the
/// first-class `keyword` field instead of substring-parsing the
/// rendered message. Display matches the legacy `Compile`-shaped
/// diagnostic byte-for-byte (`"compile error in {keyword}: expected
/// list form"`), so existing `format!("{err}").contains("expected
/// list form")` assertions pass unchanged.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform. The legacy
/// `Compile { form, message }` shape required consumers to
/// pattern-match on `message == "expected list form"` to recognize
/// this specific gate (versus the sibling `missing head symbol`
/// gate, which produces the same `Compile` shape with a different
/// message). After this lift the discriminator is the variant
/// itself — a regression that drifts the message string can no
/// longer drift the gate's identity. THEORY.md §II.1 invariant 1 —
/// typed entry; a non-list form is exactly the failure mode the
/// typed-entry gate exists to reject, and the gate's identity is
/// now load-bearing in the type system.
#[must_use]
pub fn not_a_list_form_err(keyword: &'static str) -> LispError {
    LispError::NotAListForm { keyword }
}

/// `T::compile_from_sexp` was passed `()` or a list whose first
/// element isn't a symbol — there's nothing to dispatch on. One named
/// primitive every `TataraDomain` impl shares; returns the dedicated
/// `LispError::MissingHeadSymbol { keyword, got }` variant so authoring
/// surfaces (REPL, LSP, `tatara-check`) bind to the first-class
/// `keyword` and `got` fields instead of substring-parsing the
/// rendered message. `got: None` for the empty-list case (`()`),
/// `got: Some(SexpWitness)` for the present-but-not-symbol case
/// (`(5 …)`, `(:foo …)`, `("x" …)`, `((nested) …)`) — the legacy
/// `Compile`-shaped diagnostic collapsed both into one message; this
/// builder bifurcates them structurally so the renderable detail
/// names which sub-mode fired. The `Some` arm carries the typed
/// joint identity (`SexpShape` + `Sexp::Display`) routed through
/// `sexp_witness(_)` so authoring tools that want to surface a
/// structural autofix — "you wrote `:foo` at the head slot where a
/// symbol was expected (did you mean `foo`?)" — bind on
/// `got.shape == SexpShape::Keyword` directly, no substring-grep on
/// the rendered display required.
///
/// Display matches the legacy `Compile`-shaped diagnostic byte-for-
/// byte for the prefix (`"compile error in {keyword}: missing head
/// symbol"`); the structural detail is appended in a parenthetical
/// (`(empty list)` for `None`, `(got {g})` for `Some(g)`), parallel
/// to how `RestParamMissingName` appends `(rest marker at position
/// {n}, {got|none provided})` and how `SpliceOutsideList` appends
/// `(got ,@{got})`. The `{g}` slot flows through `SexpWitness::Display`,
/// which writes only the `display` field, so existing
/// `format!("{err}").contains("missing head symbol")` assertions pass
/// unchanged.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform. The legacy
/// `Compile { form, message }` shape required consumers to
/// pattern-match on `message == "missing head symbol"` to recognize
/// this specific gate (versus the sibling `expected list form` and
/// head-mismatch gates, which produced different `message` strings
/// in the same `Compile` shape). After this lift the discriminator
/// is the variant itself — a regression that drifts the message
/// string can no longer drift the gate's identity, AND the two
/// distinct sub-modes (empty vs. present-but-not-symbol) are
/// structurally addressable. THEORY.md §II.1 invariant 1 — typed
/// entry; an empty form / non-symbol-head form is exactly the
/// failure mode the typed-entry gate exists to reject, and the
/// gate's identity is now load-bearing in the type system.
#[must_use]
pub fn missing_head_err(keyword: &'static str, got: Option<SexpWitness>) -> LispError {
    LispError::MissingHeadSymbol { keyword, got }
}

/// Structural head-mismatch builder. Returns the dedicated
/// `LispError::HeadMismatch` variant so authoring surfaces (REPL, LSP,
/// `tatara-check`) bind to first-class `keyword`/`got` fields instead
/// of substring-parsing the rendered message. Display matches the
/// legacy `Compile`-shaped diagnostic byte-for-byte, so existing
/// `format!("{err}").contains("expected ({KEYWORD}")` assertions pass
/// unchanged.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform. A diagnostic
/// whose `got` is embedded in a free-form message is structurally
/// incomplete; an authoring surface that wants to render
/// "did-you-mean" suggestions on the offending head must re-parse
/// the message. After this lift the slot exists in the variant's
/// data shape itself.
#[must_use]
pub fn head_mismatch(keyword: &'static str, got: String) -> LispError {
    LispError::HeadMismatch { keyword, got }
}

/// The substrate-wide [`TataraDomain`] well-formedness testkit — closes
/// the four typed-entry rejection gates on the trait's default
/// [`TataraDomain::compile_from_sexp`], three [`TataraDomain::KEYWORD`]
/// grammar invariants, AND the reader round-trip theorem at ONE call
/// every implementor's test module reaches for.
///
/// Peer of [`crate::closed_set::assert_closed_set_well_formed`] on the
/// sibling [`crate::ClosedSet`] contract — after this lift both
/// homoiconic-authoring contracts (the closed-set enum idiom AND the
/// derived-domain idiom) carry ONE substrate-wide structural checker
/// each, and every downstream implementor's test module reduces to a
/// single-line invocation instead of re-deriving the invariants
/// per-implementor.
///
/// ## The four `compile_from_sexp` rejection gates
///
///   1. `NotAListForm { keyword }` on a bare atom — the typed-entry
///      gate rejects the form-shape mismatch before descending into
///      the list.
///   2. `MissingHeadSymbol { keyword, got: None }` on the empty list
///      `()` — `list.first()` returns `None`, there's no head to
///      project.
///   3. `MissingHeadSymbol { keyword, got: Some(_) }` on a list whose
///      first element is not a symbol — `list.first().as_symbol()`
///      returns `None`, and the offending element's typed identity
///      threads into the `got` slot.
///   4. `HeadMismatch { keyword, got }` on a list headed by a symbol
///      other than `T::KEYWORD` — the substring-free structural
///      discriminator.
///
/// ## The three `KEYWORD` grammar invariants
///
///   5. `KEYWORD` is non-empty — a keyword-less form cannot be
///      dispatched.
///   6. `KEYWORD` classifies as [`Atom::Symbol`] through the substrate's
///      typed-entry classifier [`Atom::from_lexeme`] — the ONE
///      projection every bare-atom lexeme routes through inside the
///      reader's parse arm. Subsumes the pre-lift "no leading ASCII
///      digit" heuristic (a KEYWORD `"42"` decodes as [`Atom::Int`],
///      `"1.5"` as [`Atom::Float`]) AND catches the two shapes the
///      pre-lift check silently accepted: a leading `:` (KEYWORD
///      `":foo"` decodes as [`Atom::Keyword`]) and the two boolean
///      literals (`"#t"` / `"#f"` decode as [`Atom::Bool`]) — none of
///      which the trait's `as_symbol()` head-match would fire on. Binds
///      the invariant to the substrate's typed reader-classifier
///      algebra so a future seventh [`Atom`] variant (e.g. `Char` for
///      `#\x` reader syntax, `Bigint` for arbitrary-precision integers)
///      strengthens the check ONCE at [`Atom::from_lexeme`] rather than
///      re-heuristicing per implementor's test module.
///   7. `KEYWORD` contains no [`Sexp::is_bare_atom_boundary`] char —
///      the ONE typed projection on the outer [`Sexp`] algebra that
///      names "this char breaks the reader's bare-atom accumulator."
///      Subsumes the pre-lift "no ASCII whitespace" heuristic (via
///      `char::is_whitespace()` covering the Unicode whitespace surface
///      the reader also splits on — NBSP `\u{00A0}`, ideographic space
///      `\u{3000}`, and every other codepoint the pre-lift ASCII-only
///      check silently accepted) AND catches the seven non-whitespace
///      terminators the pre-lift check ignored:
///      [`Sexp::LIST_OPEN`] `(`, [`Sexp::LIST_CLOSE`] `)`,
///      [`crate::ast::QuoteForm::QUOTE_LEAD`] `'`,
///      [`crate::ast::QuoteForm::QUASIQUOTE_LEAD`] `` ` ``,
///      [`crate::ast::QuoteForm::UNQUOTE_LEAD`] `,`,
///      [`Atom::STR_DELIMITER`] `"`, [`Sexp::COMMENT_LEAD`] `;` — every
///      char that would tokenize a KEYWORD like `"def(x"` / `"def;x"` /
///      `"def\"x"` into TWO tokens, breaking the trait's head-match
///      structurally. Binds the invariant to the substrate's typed
///      reader-boundary algebra so a future eighth outer-dispatch
///      category (e.g. `#|…|#` block-comment lead byte) strengthens the
///      check ONCE at [`Sexp::is_bare_atom_boundary`] rather than
///      per-implementor re-derivation.
///
/// ## The round-trip theorem
///
///   8. `read(KEYWORD)` produces exactly one form, and that form's
///      [`Sexp::as_symbol`] projection returns `Some(KEYWORD)`. This is
///      the STRUCTURAL condition the trait's default
///      [`TataraDomain::compile_from_sexp`] head-match depends on: the
///      reader tokenizes the head slot, projects through
///      [`Atom::from_lexeme`], and the head-match calls `as_symbol()`
///      on the resulting [`Sexp`]. If the round-trip holds, the
///      head-match fires on the intended keyword; if it fails, no
///      other invariant matters. Invariants (6) and (7) together
///      *entail* this theorem — (6) closes the classifier axis,
///      (7) closes the tokenizer axis — so a KEYWORD that passes both
///      structural checks always passes the round-trip; pinning the
///      theorem explicitly closes the LOOP at the verification site
///      and catches drift outside the closed-set structural surface
///      (e.g. a future reader-level input transformation that couldn't
///      be reduced to either axis).
///
/// A hand-written implementor that overrides
/// [`TataraDomain::compile_from_sexp`] and drifts any of the four gates
/// from the substrate-wide structural variants surfaces here rather
/// than as a mystery integration failure downstream — the same posture
/// [`crate::closed_set::assert_closed_set_well_formed`] takes on the
/// override-prone `parse_label` / `parse_label_with_hint` /
/// `labels_joined` axes.
///
/// ## Usage
///
/// ```ignore
/// #[test]
/// fn my_spec_is_well_formed_tatara_domain() {
///     tatara_lisp::assert_tatara_domain_well_formed::<MySpec>();
/// }
/// ```
///
/// ## Theory grounding
///
/// THEORY.md §II.1 invariant 1 — typed entry; the four structural
/// gates ARE the typed-entry boundary of the derived-domain idiom, and
/// the testkit makes their identity load-bearing at the per-implementor
/// test surface.
///
/// THEORY.md §V.1 — knowable platform; the four structural rejections
/// were previously re-derived per implementor with
/// `matches!(err, LispError::NotAListForm { ... })` scaffolds. The
/// testkit collapses the scaffolds onto ONE substrate entry so future
/// implementors inherit the contract by calling one line — mirrors the
/// `assert_closed_set_well_formed` posture that closed the closed-set
/// enum idiom's 36+ per-implementor test modules onto ONE checker.
///
/// THEORY.md §VI.1 — generation over composition; the four gate
/// primitives ([`not_a_list_form_err`], [`missing_head_err`],
/// [`head_mismatch`]) already compose the structural rejections at the
/// GENERATION site. This testkit closes the LOOP at the VERIFICATION
/// site so the two ends of the substrate meet at ONE structural
/// witness — every implementor's test module inherits both halves
/// through ONE call rather than restating the four `matches!` arms
/// per-implementor.
#[track_caller]
pub fn assert_tatara_domain_well_formed<T>()
where
    T: TataraDomain,
{
    let type_name = core::any::type_name::<T>();
    let keyword = T::KEYWORD;

    // (1) — KEYWORD is non-empty. A keyword-less form has no head
    // symbol for the dispatch to key on; the trait's contract is
    // structurally degenerate without a discriminating lexeme.
    assert!(
        !keyword.is_empty(),
        "{type_name}: TataraDomain::KEYWORD is empty — the head symbol has no lexeme to dispatch on",
    );

    // (2) — KEYWORD classifies as `Atom::Symbol` through the substrate's
    // typed-entry classifier `Atom::from_lexeme`. The reader routes every
    // bare-atom lexeme through this ONE projection; anything that decodes
    // as `Bool` / `Keyword` / `Int` / `Float` / `Str` never reaches the
    // trait's head-match as a symbol. Subsumes the pre-lift "no leading
    // ASCII digit" heuristic (`"42"` → `Int`, `"1.5"` → `Float`) AND
    // catches the two shapes the pre-lift check silently accepted:
    // `":foo"` → `Keyword` and `"#t"` / `"#f"` → `Bool`. Binding to the
    // substrate's classifier means a future seventh `Atom` variant
    // (`Char`, `Bigint`) strengthens the check ONCE.
    match Atom::from_lexeme(keyword) {
        Atom::Symbol(s) if s == keyword => {}
        classified => panic!(
            "{type_name}: KEYWORD {keyword:?} classifies as {classified:?} via Atom::from_lexeme — the Lisp reader would not project the head as a symbol at the trait's head-match",
        ),
    }

    // (3) — KEYWORD contains no `Sexp::is_bare_atom_boundary` char. The
    // substrate's typed reader-boundary projection covers BOTH the
    // Unicode-whitespace surface (via `char::is_whitespace`) AND the
    // seven non-whitespace terminators (`(` `)` `'` `` ` `` `,` `"` `;`)
    // that would tokenize the KEYWORD into two tokens, breaking the
    // trait's head-match structurally. Subsumes the pre-lift
    // "no ASCII whitespace" heuristic; binding to the substrate's typed
    // reader-boundary algebra means a future eighth outer-dispatch
    // category (`#|` block-comment lead) strengthens the check ONCE.
    if let Some(ch) = keyword.chars().find(|&c| Sexp::is_bare_atom_boundary(c)) {
        panic!(
            "{type_name}: KEYWORD {keyword:?} contains reader-boundary char {ch:?} (Sexp::is_bare_atom_boundary → true) — the Lisp reader would split it into multiple tokens, breaking the head-match structurally",
        );
    }

    // (4) — a bare-atom form rejects with `NotAListForm { keyword }`.
    // The typed-entry gate rejects the form-shape mismatch before
    // descending into the list's head; the variant carries the keyword
    // as structural data so authoring surfaces bind on
    // `LispError::NotAListForm { keyword }` rather than substring-
    // parsing the rendered message.
    let bare_atom = Sexp::int(0);
    match T::compile_from_sexp(&bare_atom) {
        Err(LispError::NotAListForm { keyword: k }) => assert_eq!(
            k, keyword,
            "{type_name}: NotAListForm.keyword {k:?} drifted from T::KEYWORD {keyword:?}",
        ),
        Ok(_) => panic!(
            "{type_name}: compile_from_sexp accepted a bare-atom form — the typed-entry gate would let a non-list form silently reach the kwargs decoder",
        ),
        Err(other) => panic!(
            "{type_name}: compile_from_sexp on a bare-atom form emitted {other:?}, expected LispError::NotAListForm {{ keyword: {keyword:?} }}",
        ),
    }

    // (5) — the empty list `()` rejects with
    // `MissingHeadSymbol { keyword, got: None }`. `list.first()`
    // returns `None`, so no head-witness is threaded through the
    // rejection — the `got: None` arm names the empty-list sub-mode
    // structurally.
    let empty_list = Sexp::List(Vec::new());
    match T::compile_from_sexp(&empty_list) {
        Err(LispError::MissingHeadSymbol {
            keyword: k,
            got: None,
        }) => assert_eq!(
            k, keyword,
            "{type_name}: MissingHeadSymbol.keyword {k:?} drifted from T::KEYWORD {keyword:?} on the empty-list arm",
        ),
        Ok(_) => panic!(
            "{type_name}: compile_from_sexp accepted the empty list `()` — the typed-entry gate would let a headless form silently reach the head-match",
        ),
        Err(other) => panic!(
            "{type_name}: compile_from_sexp on the empty list `()` emitted {other:?}, expected LispError::MissingHeadSymbol {{ keyword: {keyword:?}, got: None }}",
        ),
    }

    // (6) — a list whose head is a non-symbol atom rejects with
    // `MissingHeadSymbol { keyword, got: Some(_) }`. The offending
    // element's typed identity threads through `SexpWitness` into the
    // `got` slot so authoring surfaces can render "your form's head
    // is `0`, an int, not a symbol" without re-parsing the source.
    let non_symbol_head = Sexp::List(vec![Sexp::int(0)]);
    match T::compile_from_sexp(&non_symbol_head) {
        Err(LispError::MissingHeadSymbol {
            keyword: k,
            got: Some(_),
        }) => assert_eq!(
            k, keyword,
            "{type_name}: MissingHeadSymbol.keyword {k:?} drifted from T::KEYWORD {keyword:?} on the non-symbol-head arm",
        ),
        Ok(_) => panic!(
            "{type_name}: compile_from_sexp accepted a form with a non-symbol head — the typed-entry gate would let a numeric-head form silently reach the head-match",
        ),
        Err(other) => panic!(
            "{type_name}: compile_from_sexp on a non-symbol-head form emitted {other:?}, expected LispError::MissingHeadSymbol {{ keyword: {keyword:?}, got: Some(_) }}",
        ),
    }

    // (7) — a symbol-headed list whose head is NOT `T::KEYWORD`
    // rejects with `HeadMismatch { keyword, got }`. The probe symbol
    // is chosen to be lexically distinct from every conceivable
    // canonical keyword across the substrate so no real implementor
    // can accidentally match it. A hard equality assertion rules out
    // the degenerate case where an implementor's KEYWORD collides
    // with the reserved probe.
    let probe = "__assert_tatara_domain_well_formed_probe__";
    assert_ne!(
        keyword, probe,
        "{type_name}: T::KEYWORD collides with the reserved probe {probe:?} — the wrong-head arm cannot rule out an implementor whose KEYWORD equals the probe; rename either side",
    );
    let wrong_head = Sexp::List(vec![Sexp::symbol(probe)]);
    match T::compile_from_sexp(&wrong_head) {
        Err(LispError::HeadMismatch { keyword: k, got }) => {
            assert_eq!(
                k, keyword,
                "{type_name}: HeadMismatch.keyword {k:?} drifted from T::KEYWORD {keyword:?}",
            );
            assert_eq!(
                got, probe,
                "{type_name}: HeadMismatch.got {got:?} drifted from the offending head {probe:?}",
            );
        }
        Ok(_) => panic!(
            "{type_name}: compile_from_sexp accepted a form headed by the reserved probe {probe:?} — the typed-entry gate would let a wrong-head form silently reach the kwargs decoder",
        ),
        Err(other) => panic!(
            "{type_name}: compile_from_sexp on the wrong-head form emitted {other:?}, expected LispError::HeadMismatch {{ keyword: {keyword:?}, got: {probe:?} }}",
        ),
    }

    // (8) — reader round-trip theorem. `read(KEYWORD)` produces exactly
    // one form, and that form's `as_symbol()` projection returns
    // `Some(KEYWORD)`. This is the SUFFICIENT condition invariants
    // (6) + (7) together entail: (6) closes the classifier axis (the
    // token, once assembled, classifies as `Atom::Symbol`), (7) closes
    // the tokenizer axis (the KEYWORD arrives at the classifier as ONE
    // token). Pinning the composition explicitly closes the LOOP at
    // the verification site — a substrate-owned theorem the two
    // structural checks compose into — and catches drift outside the
    // closed-set structural surface (e.g. a future reader-level input
    // transformation that couldn't be reduced to either axis).
    match crate::reader::read(keyword) {
        Ok(forms) if forms.len() == 1 && forms[0].as_symbol() == Some(keyword) => {}
        Ok(forms) => panic!(
            "{type_name}: KEYWORD {keyword:?} did not round-trip through read → as_symbol — read produced {forms:?} (expected one form projecting to Some({keyword:?}))",
        ),
        Err(err) => panic!(
            "{type_name}: KEYWORD {keyword:?} failed to tokenize at all — read returned {err:?}",
        ),
    }
}

// ── kwarg parsing + typed extractors used by the derive macro ──────

pub type Kwargs<'a> = HashMap<String, &'a Sexp>;

/// Parse `:k v :k v …` into a kwargs map. Rejects duplicate keywords so the
/// typed-entry gate fires on `(defX :name "a" :name "b")` instead of silently
/// keeping the last value — same posture `reject_unknown_kwargs` takes for
/// typo'd kwargs. A duplicate is ill-typed input: the author either meant
/// distinct keys (typo) or a list (`:tags ("a" "b")`).
///
/// Odd-length kwargs lists fail with `LispError::OddKwargs { dangling }`,
/// where `dangling` is the offending element's `Sexp::Display` projection
/// — `:query` for a keyword whose value got lost, or the literal form of a
/// stray non-keyword. Naming the dangling element keeps the diagnostic
/// structurally complete instead of merely flagging "odd number"; authoring
/// surfaces (REPL, LSP, `tatara-check`) render the mismatch without
/// re-reading the source.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 — "Typed entry. Ill-typed input
/// errors before the value exists." THEORY.md §V.1 — "knowable platform"
/// requires the diagnostic to name what was passed, not only what was
/// expected.
pub fn parse_kwargs(args: &[Sexp]) -> Result<Kwargs<'_>> {
    let mut kw = HashMap::new();
    let mut i = 0;
    while i + 1 < args.len() {
        let key = args[i].as_keyword().ok_or_else(|| {
            type_mismatch(kwargs_pos_form(i), ExpectedKwargShape::Keyword, &args[i])
        })?;
        if kw.insert(key.to_string(), &args[i + 1]).is_some() {
            return Err(duplicate_kwarg(key));
        }
        i += 2;
    }
    if i < args.len() {
        return Err(LispError::OddKwargs {
            dangling: args[i].to_string(),
        });
    }
    Ok(kw)
}

/// Reject any keyword in `kw` that isn't in `allowed`. Closes the typed-entry
/// hole where typos like `:tthreshold 0.99` would otherwise parse silently
/// with the field unset. Emitted by `#[derive(TataraDomain)]` after
/// `parse_kwargs` so every derived domain rejects unknown kwargs by default.
///
/// When the offending keyword is a near-miss of an allowed kwarg (bounded
/// edit distance via `suggest`), the diagnostic prepends a `did you mean
/// :X?` hint so the operator goes straight to the fix without scanning the
/// allowed-list. The hint is purely additive — `unknown keyword` and the
/// full allowed list still appear — so existing assertions
/// (`msg.contains("unknown keyword")`, `msg.contains(":threshold")`) pass
/// unchanged.
///
/// Returns the structural `LispError::UnknownKwarg { key, hint, allowed }`
/// variant — same posture as the `OddKwargs` / `DuplicateKwarg` /
/// `MissingKwarg` siblings. After this lift every distinct typed-entry
/// kwarg-gate failure mode binds to ONE structural variant of `LispError`,
/// not a `Compile`-shaped substring.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 (typed entry — "Ill-typed input
/// errors before the value exists"); §V.1 ("knowable platform … Render
/// Anywhere" — naming the likely intended keyword is the floor of a
/// constructive diagnostic).
pub fn reject_unknown_kwargs(kw: &Kwargs<'_>, allowed: &[&str]) -> Result<()> {
    for key in kw.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(unknown_kwarg(key, allowed));
        }
    }
    Ok(())
}

/// Parse `:k v :k v …` AND gate the result against a closed allowed-key set —
/// the fused typed-entry kwargs gate. ONE named primitive every
/// `TataraDomain` impl shares for "compile-from-args header": every
/// `#[derive(TataraDomain)]`-generated `compile_from_args` body emitted by
/// `tatara-lisp-derive` begins with this single call, and every hand-
/// written impl in the forge / lattice / tameshi crates that wants the
/// substrate's closed-set kwargs posture binds to ONE function instead of
/// remembering to call [`parse_kwargs`] AND [`reject_unknown_kwargs`] in
/// that order.
///
/// Before this lift the derive emitted the two-call sequence
/// `let kw = parse_kwargs(args)?; reject_unknown_kwargs(&kw, ALLOWED)?;`
/// verbatim at every consumer's `compile_from_args` body — well past the
/// ≥2 PRIME-DIRECTIVE trigger once the fleet's seven-plus
/// `#[derive(TataraDomain)]` consumers (ProcessSpec, EphemeralSpec,
/// MonitorSpec, NotifySpec, AlertPolicySpec, EscalationStep, CompilerSpec,
/// and every future derived domain) inline the same two lines through the
/// proc-macro emitter. The two-call sequence is structurally one
/// operation — "parse the keyword/value run, then assert every key sits
/// in the static allowed-set" — and a regression that drifts ONE
/// consumer's gate from the others (e.g. the derive emits one call but a
/// hand-written impl emits only the other, or a future emitter swaps the
/// order so `reject_unknown_kwargs` runs against an unparsed slice) is
/// the silent typed-entry hole this primitive closes by construction.
///
/// The two stages are composed in the canonical order:
///   1. [`parse_kwargs`] runs first — odd-length input, non-keyword at a
///      key position, and duplicate keys surface as their structural
///      variants ([`LispError::OddKwargs`] / [`LispError::TypeMismatch`]
///      with `form = kwargs_pos_form(i)` / [`LispError::DuplicateKwarg`]).
///   2. Only on `Ok(kw)` does [`reject_unknown_kwargs`] run — keys
///      outside `allowed` surface as [`LispError::UnknownKwarg`] with the
///      typed `hint` / `allowed` slots populated.
///
/// This ordering is structural: `reject_unknown_kwargs` cannot inspect
/// an unparsed `&[Sexp]`, so parse-stage rejection MUST precede
/// reject-stage rejection. A call with BOTH an odd-length tail AND an
/// unknown kwarg surfaces as `OddKwargs` (parse-stage), never as
/// `UnknownKwarg` (reject-stage) — the gate is single-pass and the
/// stages compose in exactly one order. Naming the composition makes
/// that order load-bearing data on the substrate, not a discipline the
/// derive's emit template happens to encode correctly.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 — "Typed entry. Ill-typed
/// input errors before the value exists." The kwargs gate is the
/// typed-entry boundary for every derived domain; closing the gate
/// behind ONE primitive lifts the closed-set posture from the derive's
/// emit template to the substrate's typed surface. THEORY.md §VI.1 —
/// generation over composition; the two-call sequence in the derive's
/// emit template, multiplied across every consumer in the fleet, is
/// well past the three-times rule once the structural shape is named.
/// THEORY.md §V.1 — knowable platform; authoring tools (REPL, LSP,
/// `tatara-check`) that want to surface "this form's kwargs gate
/// rejected because …" bind to the unified primitive's call site
/// instead of guessing which of the two component functions the
/// rejection came from. THEORY.md §II.1 invariant 2 (free middle) —
/// every consumer routes through the SAME composition, so a regression
/// that drifts the order or skips a stage on one path can never reach
/// the substrate's runtime: the type system binds every consumer to
/// the fused primitive's single emission shape.
///
/// Lifetime: the returned [`Kwargs<'a>`] borrows from `args` (the typed
/// alias is `HashMap<String, &'a Sexp>`), so the call site keeps the
/// `&[Sexp]` slice alive for the lifetime of the parsed map — same
/// posture as [`parse_kwargs`]. The fused primitive does not allocate
/// beyond [`parse_kwargs`]'s map: [`reject_unknown_kwargs`] is a pure
/// `O(allowed.len() · kw.len())` scan that returns `Ok(())` on success.
pub fn parse_kwargs_strict<'a>(args: &'a [Sexp], allowed: &[&str]) -> Result<Kwargs<'a>> {
    let kw = parse_kwargs(args)?;
    reject_unknown_kwargs(&kw, allowed)?;
    Ok(kw)
}

/// Structural unknown-kwarg builder. Returns the dedicated
/// `LispError::UnknownKwarg` variant so authoring surfaces (REPL, LSP,
/// `tatara-check`) bind to first-class `key` / `hint` / `allowed`
/// fields instead of substring-parsing the rendered message. Display
/// matches the legacy `Compile { form: kwarg_form(key), message:
/// "unknown keyword (...)" }` rendering byte-for-byte
/// (`"compile error in :{key}: unknown keyword (did you mean :{hint}?;
/// allowed: :a, :b, :c)"` with a hint, `"compile error in :{key}:
/// unknown keyword (allowed: :a, :b, :c)"` without), so existing
/// `msg.contains("unknown keyword")` / `msg.contains(":threshold")` /
/// `msg.contains("did you mean :threshold?")` assertions keep
/// passing.
///
/// Encapsulates the three otherwise-inline steps every unknown-kwarg
/// site shares: (1) ranking the near-miss via `suggest`, (2) sorting
/// the allowed-set lexicographically so two operators on two machines
/// see the same message for the same input — diagnostics are
/// deterministic, (3) materializing the allowed-set as owned
/// `Vec<String>` so the variant lives independent of the call frame
/// and crosses thread boundaries cleanly. A future "registry-aware
/// near-miss for unknown registry-dispatched forms" path
/// (`tatara-check`'s unknown-keyword fallthrough) binds to this
/// helper rather than re-formatting the shape per call site.
///
/// `reject_unknown_kwargs` is the first consumer; hand-written
/// `TataraDomain` impls in the forge / lattice / tameshi crates that
/// don't fit the derive's closed-field-type set bind to the
/// substrate's primitive instead of inline `LispError::Compile { … }`
/// assembly. After this lift `reject_unknown_kwargs` is no longer the
/// last `LispError::Compile { ... }` site in the kwarg-gate's
/// diagnostic surface — every distinct kwarg-gate failure mode is now
/// a structural variant of `LispError`.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render
/// Anywhere." A diagnostic whose offending `key` / hint / allowed-set
/// are embedded in a free-form message is structurally incomplete; an
/// authoring surface that wants to render a squiggly under the typo
/// or surface the allowed-set as completions must re-parse the
/// message. After this lift the slots exist in the variant's data
/// shape itself. THEORY.md §II.1 invariant 1 (typed entry) — an
/// unknown kwarg is exactly the failure mode the typed-entry gate
/// exists to reject; naming it structurally is the typed posture for
/// that gate's diagnostic. THEORY.md §VI.1 (generation over
/// composition — one named primitive per structural shape).
#[must_use]
pub fn unknown_kwarg(key: &str, allowed: &[&str]) -> LispError {
    let hint = suggest(key, allowed).map(String::from);
    let mut sorted: Vec<String> = allowed.iter().map(|s| (*s).to_string()).collect();
    sorted.sort();
    LispError::UnknownKwarg {
        key: key.to_string(),
        hint,
        allowed: sorted,
    }
}

/// The typed-entry kwargs-gate's OPTIONAL lookup primitive — `Some(&Sexp)`
/// when `key` is present in `kw`, `None` when absent. ONE named projection
/// on the substrate's `Kwargs<'a>` algebra every optional-kwarg consumer
/// (`extract_optional_atom`, `extract_list`, `extract_optional_via_serde`)
/// routes through, and the sibling [`required`](self::required) composes
/// directly atop it as `optional(kw, key).ok_or_else(|| missing_kwarg(key))`.
/// Before this lift the same `kw.get(key).copied()` projection — turning
/// `Option<&&'a Sexp>` (the raw `HashMap::get` return) into the consumer-
/// shaped `Option<&'a Sexp>` — was inlined verbatim at FOUR sites: once
/// inside `required`'s composition, and once inside each of the three
/// optional consumers' absence-handling preludes. After this lift the
/// projection lives in ONE place; `required` becomes the closed-form
/// composition `optional + ok_or_else(missing_kwarg)`, and the three
/// optional consumers read through `optional(kw, key)` without re-stating
/// the `Option<&&Sexp>` → `Option<&Sexp>` projection at each call site.
///
/// Sibling pair with [`required`](self::required): together the two close
/// the substrate's typed-entry kwargs-LOOKUP surface — `required` is the
/// mandatory-presence path returning `Result<&Sexp>` (absence → typed
/// `LispError::MissingKwarg`); `optional` is the may-be-absent path
/// returning `Option<&Sexp>` (absence → `None`, the consumer decides
/// what default behavior absence triggers — `None` for atoms, empty `Vec`
/// for lists, `Sexp::Nil` for params). The TWO primitives between them
/// cover every consumer's kwargs-lookup posture; a third would be a
/// structural extension the type system would surface at every call site.
/// The composition `required = optional + ok_or_else(missing_kwarg)` is
/// the structural identity binding the two — `required(kw, key)` and
/// `optional(kw, key).ok_or_else(|| missing_kwarg(key))` are
/// observationally identical, and naming the composition makes the
/// identity a substrate-owned theorem rather than a hand-inlined
/// duplication discipline four sites had to keep in lockstep.
///
/// The returned `&'a Sexp` carries the SAME lifetime contract as
/// [`required`](self::required)'s `Ok(&'a Sexp)` — the projection borrows
/// from the kwargs map's value slot via `.copied()`, so the optional
/// consumers can hold the reference through their absence-arm match
/// without an intermediate clone. `'a` is the outer borrow lifetime
/// (mirroring `required`); the inner `'_` is free so call sites with
/// `Kwargs<'a>` (the typical `parse_kwargs` output binding) and
/// `Kwargs<'static>` (a future static-bound shape) both type-check
/// uniformly.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; four
/// inline copies of one structural projection past the three-times rule
/// once the structural shape is named. THEORY.md §V.1 — knowable
/// platform; the substrate's typed-entry kwargs-lookup surface is now
/// the named PAIR `{required, optional}` — authoring tools (REPL, LSP,
/// `tatara-check`) that want to surface "this domain reads kwarg X as
/// optional" bind to the `optional` primitive's signature, not the
/// HashMap-level `get` chain. THEORY.md §II.1 invariant 1 — typed entry;
/// the kwargs-lookup gate's two postures (required vs. optional) are
/// now structurally named, so a future fourth posture (e.g. "required
/// with non-empty constraint") extends the pair as a peer rather than
/// silently piggybacking on the inlined `get(key).copied()` chain.
/// THEORY.md §II.1 invariant 2 — free middle; the typed-entry kwargs
/// gate's lookup shape is uniform across every derived domain (and
/// every hand-written `TataraDomain` impl), so a future emitter that
/// wants to instrument the lookup (a span-aware lookup, a debug-mode
/// lookup logger) wraps ONE function rather than four inline sites.
#[must_use]
pub fn optional<'a>(kw: &'a Kwargs<'_>, key: &str) -> Option<&'a Sexp> {
    kw.get(key).copied()
}

/// The typed-entry kwargs-gate's REQUIRED lookup primitive — `Ok(&Sexp)`
/// when `key` is present in `kw`, `Err(LispError::MissingKwarg)` when
/// absent. Composes [`optional`](self::optional) (the may-be-absent
/// lookup) with [`missing_kwarg`](self::missing_kwarg) (the canonical
/// rejection on absence) so the substrate's typed-entry kwargs-lookup
/// surface is named as the PAIR `{required, optional}` with `required`
/// expressed as the closed-form composition of its two sibling
/// primitives. Sibling pair documented in [`optional`](self::optional).
pub fn required<'a>(kw: &'a Kwargs<'_>, key: &str) -> Result<&'a Sexp> {
    optional(kw, key).ok_or_else(|| missing_kwarg(key))
}

/// Canonical typed `form:` value for a kwarg-level `LispError::TypeMismatch`.
/// Every typed-entry diagnostic that names a kwarg (`required`, `type_err`,
/// `deserialize_err`, the duplicate-keyword paths in `parse_kwargs` and
/// `sexp_to_json`, the unknown-keyword path in `reject_unknown_kwargs`,
/// the non-list path in `extract_vec_via_serde`) routes through this one
/// helper, so authoring surfaces (REPL, LSP, `tatara-check`) bind to a
/// single named primitive rather than seven inline `format!(":{key}")`
/// copies.
///
/// Returns the typed `crate::error::KwargPath::Named(key.to_string())` value
/// directly — consumers feed it into `LispError::TypeMismatch.form: KwargPath`
/// where it is structurally bound via pattern-match (`KwargPath::Named(_)`),
/// not substring-matched. The canonical `:<key>` literal lives in ONE place
/// (`KwargPath`'s Display match arm) alongside its sibling shapes
/// `kwarg_item_form` / `kwargs_pos_form`, so a typo in any of the three
/// can never drift independent of the others.
///
/// Theory anchor: THEORY.md §VI.1 — "Generation over composition.
/// Three-times rule: when a pattern repeats three times, extract an
/// archetype/backend/synthesizer and generate from it." Seven inline
/// copies in one module is the textbook signal. THEORY.md §V.1 —
/// knowable platform; the typed `KwargPath` enum encodes the closed set
/// of three reachable path shapes at the type level so authoring tools
/// bind to path-shape identity rather than substring-matching the
/// rendered prefix. THEORY.md §II.1 invariant 1 (typed entry) — the
/// kwargs-path identity is now load-bearing data on the variant rather
/// than a projection-to-String.
#[must_use]
pub fn kwarg_form(key: &str) -> crate::error::KwargPath {
    crate::error::KwargPath::named(key)
}

/// Canonical `form:` label for a failure inside the Nth item of a
/// list-typed kwarg — `:steps[1]` when the second item of `:steps` fails
/// to deserialize, `:tags[2]` when the third tag isn't a string. The
/// substrate names the item-path so the operator sees both *which kwarg*
/// and *which element* misfired without re-counting from the source.
///
/// Frontier inspiration: JSON Pointer (`/steps/1`) and jq path
/// expressions — lossless paths through value projections so downstream
/// tooling (LSP underlines, structural rewrites) bind to the path
/// instead of parsing the diagnostic message. Translation through
/// pleme-io primitives: the surface syntax authors already write
/// (`:<key>` + `[idx]`), no new error variant, no new IR layer. When a
/// future run gives `Sexp` source spans, the indexed form gains a
/// position the same way `kwarg_form` will — one helper, every consumer
/// inherits.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render
/// Anywhere." A diagnostic that names the kwarg but loses the item index
/// is structurally incomplete; the path completes it.
///
/// Returns the typed `crate::error::KwargPath::Item { key, idx }` value
/// directly — consumers feed it into `LispError::TypeMismatch.form: KwargPath`
/// where it is structurally bound via pattern-match (`KwargPath::Item { .. }`),
/// not substring-matched. The canonical `:<key>[<idx>]` literal lives in ONE
/// place alongside `kwarg_form` / `kwargs_pos_form`. See `kwarg_form` for the
/// typed-enum's role.
#[must_use]
pub fn kwarg_item_form(key: &str, idx: usize) -> crate::error::KwargPath {
    crate::error::KwargPath::item(key, idx)
}

/// Canonical `form:` label for a kwargs-list slot whose key position is
/// not yet known — the slot itself failed the
/// "this-position-must-be-a-keyword" gate, so there is no `:<key>` to
/// hang the path off. Renders `kwargs[<idx>]` — parallel to
/// `kwarg_item_form`'s `:<key>[<idx>]` shape, rooted at the kwargs
/// slice rather than at a named kwarg.
///
/// Used by `parse_kwargs` to label the structural type-mismatch when
/// the element at an even position isn't a `Sexp::Atom(Keyword(_))`.
/// Pairing this label with the existing `LispError::TypeMismatch`
/// variant (`expected: "keyword"`, `got: sexp_type_name(_)`) means
/// authoring surfaces (REPL, LSP, `tatara-check`) bind to ONE variant
/// identity for every typed-entry mismatch — `:<key>` for kwarg-level
/// failures, `:<key>[<idx>]` for per-item failures, and now
/// `kwargs[<idx>]` for not-a-keyword-yet failures. When a future run
/// gives `Sexp` source spans, the slot-form gains a position the same
/// way `kwarg_form` / `kwarg_item_form` will — one helper, every
/// consumer inherits.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// fourth `form:`-label primitive after `kwarg_form`,
/// `kwarg_item_form`, and the registry-keyword path; one helper per
/// distinct path shape so the substrate's diagnostic surface stays
/// structurally complete).
///
/// Returns the typed `crate::error::KwargPath::Slot(idx)` value directly —
/// consumers feed it into `LispError::TypeMismatch.form: KwargPath` where it
/// is structurally bound via pattern-match (`KwargPath::Slot(_)`), not
/// substring-matched. The canonical `kwargs[<idx>]` literal lives in ONE
/// place alongside `kwarg_form` / `kwarg_item_form`. See `kwarg_form` for
/// the typed-enum's role.
#[must_use]
pub fn kwargs_pos_form(idx: usize) -> crate::error::KwargPath {
    crate::error::KwargPath::Slot(idx)
}

/// Typed projection of a `Sexp`'s outermost shape into the closed-set
/// `SexpShape` enum — the twelve reachable shapes the reader can produce.
/// Used by the typed extractors to thread the observed shape into
/// `LispError::TypeMismatch.got: SexpShape` /
/// `LispError::NamedFormNonSymbolName.got: SexpShape` so a typed-entry
/// gate's rejection-shape identity is load-bearing data in the type
/// system, not a `&'static str` projection at the helper boundary.
/// Consumers (REPL, LSP, `tatara-check`) pattern-match on
/// `SexpShape::Int` etc. directly rather than substring-matching the
/// rendered `got` literal.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform. An error that names
/// only the expected side leaves the operator to guess what was passed;
/// naming both is the floor of constructive diagnostics. The typed
/// projection extends that posture: not just naming both sides, but
/// encoding the observed shape's identity as a TYPE so a regression that
/// drifts the label becomes a compile error, not a runtime substring
/// drift. When a future run gives `Sexp` source spans, this helper is
/// the single site that learns to thread `got Y at <pos>`; today's call
/// sites pick up the span automatically.
/// Free-function delegate to the [`Sexp::shape`] inherent method on the
/// `Sexp` algebra. Retained for backwards compatibility with consumers
/// that import this helper by name (no callers reach in through the
/// module path post-lift); the inherent method is the canonical site
/// for the (Sexp variant, SexpShape variant) projection family —
/// `Atom::kind().sexp_shape()` (atomic axis), `as_quote_form().map(|(qf,
/// _)| qf.sexp_shape())` (quote-family axis), with `Nil` / `List`
/// arms projecting to their own `SexpShape` variants directly. See
/// [`Sexp::shape`]'s docstring for the closed-set composition law and
/// the THEORY anchors.
#[must_use]
pub fn sexp_shape(s: &Sexp) -> SexpShape {
    s.shape()
}

/// Thin delegate to [`Sexp::type_name`] retained for callers that
/// want the free-function reach — the canonical site is now the
/// inherent method on the [`Sexp`] algebra. Stable, human-readable
/// name of a `Sexp`'s outermost shape — the `&'static str`
/// projection of `s.shape().label()`. Retained for callers that
/// want the canonical literal directly (e.g. test assertions on the
/// rendered `expected X, got Y` substring); new code constructing
/// `LispError::TypeMismatch` / `NamedFormNonSymbolName` passes
/// through `sexp_shape` directly so the typed identity rides the
/// variant slot rather than collapsing through the literal at the
/// helper boundary.
///
/// Composition law: `sexp_type_name(s) == s.type_name() ==
/// s.shape().label()` for every `s: &Sexp`. Pre-lift the dispatcher
/// lived here as the canonical site; post-lift the inherent method
/// [`Sexp::type_name`] is the canonical site and this free function
/// delegates so existing callers continue to compile. Same lift
/// posture as [`super::domain::sexp_shape`] → [`Sexp::shape`]
/// (commit 121bb60), [`super::domain::sexp_witness`] →
/// [`Sexp::witness`] (commit a427e3b), [`super::domain::sexp_to_json`]
/// → [`Sexp::to_json`] (commit 875ee3b), and
/// [`super::domain::json_to_sexp`] → [`Sexp::from_json`] (commit
/// 4a467eb): the algebra-level projection sits on the value, the
/// free function is a one-line thin delegate. The
/// `LispError::TypeMismatch.got` projection at
/// `compile::compile_typed`'s typed-entry rejection site and every
/// legacy substring-grep rejection-message test routes through
/// `s.type_name()` after this lift.
///
/// Sibling of [`sexp_shape`] (the typed-shape projection feeding
/// `TypeMismatch.expected` typed slot) and [`sexp_witness`] (the
/// joint typed-shape + renderable-literal projection feeding
/// `NamedFormNonSymbolName.got` / `NonSymbolUnquoteTarget.got` /
/// etc.). [`Sexp::type_name`] is the canonical-label-only
/// projection — the `&'static str` literal flattened from the
/// typed identity for substring-grep callers and the
/// `TypeMismatch.got` slot.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform / constructive
/// diagnostics. The canonical-label projection becomes a NAMED
/// primitive on the substrate's `Sexp` algebra rather than a free
/// function consumers reach across module boundaries to call.
/// THEORY.md §VI.1 — generation over composition; the projection now
/// lives on the typed `Sexp` algebra alongside `Sexp::shape` /
/// `Sexp::witness` / `Sexp::to_json` / `Sexp::from_json`, so a
/// future `Sexp` variant lands at the algebra's match site (via
/// `Sexp::shape`'s exhaustive arm) without a module-path
/// indirection. THEORY.md §II.1 invariant 1 — typed entry; the
/// offending Sexp's canonical-label identity is part of the proof
/// of WHAT the typed-entry gate rejected.
#[must_use]
pub fn sexp_type_name(s: &Sexp) -> &'static str {
    s.type_name()
}

/// Thin delegate to [`Sexp::witness`] retained for callers that want
/// the free-function reach — the canonical site is now the inherent
/// method on the [`Sexp`] algebra. Pairs the typed [`SexpShape`]
/// (structural identity) with the renderable [`Sexp::Display`]
/// projection in ONE owned [`SexpWitness`] value so the variant lives
/// independent of the call frame and crosses thread boundaries
/// cleanly.
///
/// Composition law: `sexp_witness(s) == s.witness()` for every
/// `s: &Sexp`. Pre-lift the dispatcher lived here as the canonical
/// site; post-lift the inherent method [`Sexp::witness`] is the
/// canonical site and this free function delegates so existing
/// callers continue to compile. Same lift posture as
/// [`super::domain::sexp_shape`] → [`Sexp::shape`] (commit 121bb60):
/// the algebra-level projection sits on the value, the free
/// function is a one-line thin delegate. The 8 typed-entry
/// rejection-builder callers in `macro_expand.rs`
/// (`non_symbol_unquote_target`, `splice_outside_list`,
/// `non_symbol_param`, `rest_param_missing_name`,
/// `rest_param_trailing_tokens`, `optional_param_malformed`,
/// `defmacro_non_symbol_name`, `defmacro_non_list_params`), the
/// `missing_head_err` invocation in the `TataraDomain` blanket impl
/// at line 46, and the typed-exit `rewriter_non_list_err` builder
/// all route through `s.witness()` after this lift.
///
/// Sibling of [`sexp_shape`] (the shape-only projection feeding
/// `TypeMismatch.got` / `NamedFormNonSymbolName.got`) and
/// [`sexp_type_name`] (the `&'static str`-only projection feeding
/// legacy substring-grep consumers). [`Sexp::witness`] is the
/// typed JOINT projection — both halves of the identity bundled
/// into ONE owned `SexpWitness` value.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform / constructive
/// diagnostics. An error that names only the shape leaves the operator
/// to guess what they wrote; an error that names only the literal
/// withholds the structural identity tools want to pattern-match on.
/// The witness names both. THEORY.md §VI.1 — generation over
/// composition; the projection now lives on the typed `Sexp` algebra
/// alongside `Sexp::shape`, so a future `Sexp` variant lands at the
/// algebra's match site (via `Sexp::shape`'s exhaustive arm) without
/// a module-path indirection. THEORY.md §II.1 invariant 1 — typed
/// entry; the offending Sexp's identity is part of the proof of WHAT
/// the typed-entry gate rejected.
#[must_use]
pub fn sexp_witness(s: &Sexp) -> SexpWitness {
    s.witness()
}

/// Suggest the candidate closest to `needle` by Levenshtein distance,
/// when the closest candidate is within a bounded edit distance.
///
/// The bound scales with `needle`'s character length:
///   - len ≤ 3: bound 1 (single-character typo on a short identifier)
///   - len ≤ 7: bound 2 (insertion + transposition, two typos)
///   - len ≥ 8: bound 3 (longer identifiers absorb more drift)
///
/// Returns the closest candidate within the bound. Ties are broken
/// lexicographically so two operators on two machines see the same hint
/// for the same input — diagnostics are deterministic. An exact match in
/// `candidates` is excluded (the caller already has the keyword; the
/// suggestion exists for near-misses only). Empty `candidates` returns
/// `None`.
///
/// One named primitive lifts the substrate's understanding of "near-match
/// across a candidate set" out of any per-call-site implementation. The
/// unknown-kwarg diagnostic in `reject_unknown_kwargs` is the first
/// consumer; future consumers — `LispError::HeadMismatch`'s "did you
/// mean a registered domain?" hint, `tatara-check`'s registry-dispatch
/// suggestions, the LSP's completion-failure fallback — bind to one
/// helper rather than re-implementing edit distance.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render Anywhere."
/// Naming the likely intended candidate is the floor of a constructive
/// diagnostic. THEORY.md §VI.1 — generation over composition: every
/// near-match suggestion in the substrate routes through ONE primitive.
///
/// Frontier inspiration: rustc's `find_best_match_for_name`, Idris's
/// "did you mean …?" elaborator hint, Roslyn's `SymbolMatcher` — bounded
/// edit distance over a symbol table. Translation through pleme-io
/// primitives: a pure function over `&[&str]`, no new error variant, no
/// new IR layer, no new dep.
#[must_use]
pub fn suggest<'a>(needle: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let bound = suggestion_bound(needle);
    let mut best: Option<(usize, &'a str)> = None;
    for &candidate in candidates {
        if candidate == needle {
            continue;
        }
        let dist = levenshtein(needle, candidate);
        if dist > bound {
            continue;
        }
        match best {
            None => best = Some((dist, candidate)),
            Some((bd, bc)) if dist < bd || (dist == bd && candidate < bc) => {
                best = Some((dist, candidate));
            }
            _ => {}
        }
    }
    best.map(|(_, c)| c)
}

fn suggestion_bound(needle: &str) -> usize {
    let n = needle.chars().count();
    if n <= 3 {
        1
    } else if n <= 7 {
        2
    } else {
        3
    }
}

/// Classic two-row Levenshtein. Operates on `char`s so multibyte input
/// (e.g. a domain authored with non-ASCII identifiers) measures
/// character-distance, not byte-distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Structural duplicate-kwarg builder. Returns the dedicated
/// `LispError::DuplicateKwarg` variant so authoring surfaces (REPL, LSP,
/// `tatara-check`) bind to a first-class `key` field instead of
/// substring-parsing the rendered message. Display matches the legacy
/// `Compile { form: kwarg_form(key), message: "duplicate keyword" }`
/// rendering byte-for-byte (`"compile error in :{key}: duplicate
/// keyword"`), so existing `msg.contains("duplicate keyword")` /
/// `msg.contains(":name")` assertions keep passing.
///
/// Two inline copies of the same triple — `parse_kwargs`'s top-level
/// duplicate-keyword path and `sexp_to_json`'s nested-kwargs duplicate-
/// keyword path — used to assemble this shape by hand. One named
/// primitive lifts both into the substrate's structural-variant surface,
/// so every `parse_kwargs` failure mode (`OddKwargs` for odd length,
/// `TypeMismatch` for not-a-keyword-at-position, `DuplicateKwarg` for
/// duplicate key) is now a structural variant of `LispError`, not a
/// `Compile`-shaped substring.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render
/// Anywhere." A diagnostic whose offending `key` is embedded in a
/// free-form message is structurally incomplete; an authoring surface
/// that wants to render a squiggly under the duplicate or hint a fix
/// must re-parse the message. After this lift the slot exists in the
/// variant's data shape itself. THEORY.md §II.1 invariant 1 (typed
/// entry — "Ill-typed input errors before the value exists") — a
/// duplicate kwarg is exactly the failure mode the typed-entry gate
/// exists to reject; naming it structurally is the typed posture for
/// that gate's diagnostic.
#[must_use]
pub fn duplicate_kwarg(key: &str) -> LispError {
    LispError::DuplicateKwarg {
        key: key.to_string(),
    }
}

/// Structural missing-kwarg builder. Returns the dedicated
/// `LispError::MissingKwarg` variant so authoring surfaces (REPL, LSP,
/// `tatara-check`) bind to a first-class `key` field instead of
/// substring-parsing the rendered message. Display matches the legacy
/// `Compile { form: kwarg_form(key), message: "required but not
/// provided" }` rendering byte-for-byte (`"compile error in :{key}:
/// required but not provided"`), so existing
/// `msg.contains("required")` / `msg.contains(":threshold")` assertions
/// keep passing.
///
/// `required` (the kwarg lookup helper that fronts every typed
/// extractor — `extract_string`, `extract_int`, `extract_float`,
/// `extract_bool`, `extract_via_serde`, plus every hand-written
/// `TataraDomain` impl in the forge / lattice / tameshi crates) used
/// to assemble this shape inline. One named primitive lifts that into
/// the substrate's structural-variant surface, so every kwarg-level
/// "required-but-absent" failure routes through ONE function instead
/// of re-formatting the shape per call site. After this lift every
/// distinct `parse_kwargs` + `required` typed-entry kwarg failure mode
/// (odd length, not-a-keyword-at-position, duplicate key, missing
/// required key) is now a structural variant of `LispError`, not a
/// `Compile`-shaped substring.
///
/// Sibling of the pre-existing `Missing(&'static str)` variant —
/// `MissingKwarg` covers the runtime-key path the kwargs extractors
/// share (every derive-generated extractor and every hand-written
/// `TataraDomain` impl); `Missing` stays for compile-time-known names.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render
/// Anywhere." A diagnostic whose offending `key` is embedded in a
/// free-form message is structurally incomplete; an authoring surface
/// that wants to render a squiggly under the missing kwarg slot or
/// render a "did you mean :X?" hint must re-parse the message. After
/// this lift the slot exists in the variant's data shape itself.
/// THEORY.md §II.1 invariant 1 (typed entry — "Ill-typed input errors
/// before the value exists") — a missing required kwarg is exactly the
/// failure mode the typed-entry gate exists to reject; naming it
/// structurally is the typed posture for that gate's diagnostic.
#[must_use]
pub fn missing_kwarg(key: &str) -> LispError {
    LispError::MissingKwarg {
        key: key.to_string(),
    }
}

/// Structural type-mismatch builder. Pairs a typed `form: KwargPath`
/// (typically `kwarg_form(_)` / `kwarg_item_form(_, _)` /
/// `kwargs_pos_form(_)`) with the static `expected` label and the `got`
/// projection of the offending `Sexp` through `sexp_type_name`. Returns
/// the dedicated `LispError::TypeMismatch` variant so authoring surfaces
/// (REPL, LSP, `tatara-check`) bind to first-class `form`/`expected`/`got`
/// fields — pattern-matching on `KwargPath::Item { .. }` etc. directly —
/// instead of substring-parsing the rendered message.
///
/// Three inline `format!("expected {X}, got {}", sexp_type_name(_))`
/// copies in this module (`type_err`, `extract_string_list` per-item,
/// `extract_vec_via_serde` non-list) used to assemble the same shape by
/// hand; the three-times rule (THEORY.md §VI.1) calls for one named
/// primitive. This is it. Future runs that thread `pos: Option<usize>`
/// from `Sexp` spans add ONE field to the variant; every type-mismatch
/// site inherits positional rendering with no consumer changes.
#[must_use]
pub fn type_mismatch(
    form: crate::error::KwargPath,
    expected: ExpectedKwargShape,
    got: &Sexp,
) -> LispError {
    LispError::TypeMismatch {
        form,
        expected,
        got: got.shape(),
    }
}

fn type_err(key: &str, expected: ExpectedKwargShape, got: &Sexp) -> LispError {
    type_mismatch(kwarg_form(key), expected, got)
}

/// Item-indexed sibling of `type_err` — pairs `kwarg_item_form` with
/// `type_mismatch` so a per-item failure inside a list-typed kwarg names
/// `KwargPath::Item { key, idx }` plus the structural `expected`/`got` shape.
/// Bound through the atom-family per-axis shape gate
/// [`AtomKwarg::project_at`] (the one owner of the
/// `Self::project(sexp).ok_or_else(|| type_err_at(key, idx, Self::SHAPE,
/// sexp))` composition every atom-projection list-family extractor —
/// `extract_string_list`, `extract_narrowed_list` — routes through).
/// Future per-item type-mismatch sites bind through [`AtomKwarg::project_at`]
/// rather than re-inlining the composition; direct calls stay for the
/// bespoke element-shape paths outside the atom family.
fn type_err_at(key: &str, idx: usize, expected: ExpectedKwargShape, got: &Sexp) -> LispError {
    type_mismatch(kwarg_item_form(key, idx), expected, got)
}

/// Structural range-mismatch builder. Pairs a typed `form: KwargPath`
/// (typically `kwarg_form(_)` / `kwarg_item_form(_, _)` /
/// `kwargs_pos_form(_)`) with the typed `target: NumericWidth` narrow
/// axis identity and the author's literal `value: NumericLiteral`.
/// Returns the dedicated [`LispError::KwargOutOfRange`] variant so
/// authoring surfaces (REPL, LSP, `tatara-check`) bind to first-class
/// `form`/`target`/`value` fields — pattern-matching on
/// `KwargPath::Item { .. }` etc. directly — instead of substring-
/// parsing the rendered message.
///
/// KwargPath-parameterized peer of [`type_mismatch`] on the numeric-
/// narrowing axis. Pre-lift the [`LispError::KwargOutOfRange`] struct-
/// literal lived inline at TWO sites in this module — [`range_err`]
/// (the `KwargPath::Named(key)` scalar-kwarg path) and [`range_err_at`]
/// (the `KwargPath::Item { key, idx }` per-item path) — while its
/// sibling axis (the shape-gate rejection surface) had ONE lifted
/// [`type_mismatch`] primitive both [`type_err`] / [`type_err_at`]
/// delegated through. Post-lift the two axes emit symmetrically:
/// [`type_mismatch`] owns every [`LispError::TypeMismatch`] struct-
/// literal construction; this primitive owns every
/// [`LispError::KwargOutOfRange`] struct-literal construction. The
/// scaffold difference between the axes collapses to zero — a reader
/// finding one now finds the other, and future span promotions on
/// [`KwargPath`] land at ONE primitive per axis rather than at four
/// wrapper sites.
///
/// Two consumers in this module route through this primitive:
/// [`range_err`] (composing `kwarg_form(key)` + this primitive) and
/// [`range_err_at`] (composing `kwarg_item_form(key, idx)` + this
/// primitive). Public visibility mirrors [`type_mismatch`]'s public
/// visibility so hand-written [`TataraDomain`] impls that construct a
/// [`LispError::KwargOutOfRange`] with a custom [`KwargPath`]
/// (`KwargPath::Slot(_)` for a not-yet-keyed kwargs slot, or a future
/// third path shape) have ONE substrate entry to route through rather
/// than re-inlining the struct-literal at their own call site — the
/// same posture the [`type_mismatch`] sibling already carries for the
/// shape-mismatch axis.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// [`LispError::KwargOutOfRange`] struct-literal lived at two sites,
/// crossing the ≥2 duplication threshold on the substrate's rejection-
/// construction surface. THEORY.md §V.1 — knowable platform; the
/// numeric-narrowing gate's rejection-shape identity lives in ONE
/// primitive so authoring surfaces pick up the diagnostic-shape
/// promotion mechanically once the variant is structurally extended
/// (e.g. threading `pos: Option<usize>` from `Sexp` spans, adding a
/// `source: NarrowingCause` chain). THEORY.md §II.1 invariant 3
/// (typed exit) — the range-rejection boundary lives at ONE primitive
/// whose axis identity is the type-parameter payload
/// ([`NumericWidth`] + [`NumericLiteral`] carriers), not a per-site
/// literal constructor.
#[must_use]
pub fn range_mismatch(form: KwargPath, target: NumericWidth, value: NumericLiteral) -> LispError {
    LispError::KwargOutOfRange {
        form,
        target,
        value,
    }
}

/// Range-axis sibling of [`type_err`] — the kwarg's `Sexp` shape was
/// RIGHT but the value does not fit the field's Rust width. Pairs the
/// same `kwarg_form(_)` typed path with the typed `NumericWidth` target
/// and the author's literal, returning [`LispError::KwargOutOfRange`]
/// via the KwargPath-parameterized [`range_mismatch`] primitive.
///
/// Named beside `type_err` / `type_err_at` deliberately: the three are
/// the typed-entry kwarg gate's whole rejection vocabulary — shape,
/// per-item shape, and range — so a reader who finds one finds all
/// three, and a future span lift touches one neighbourhood.
///
/// `#[cfg(test)]`: the two production narrowing consumers
/// ([`narrow_or_range_err`], [`narrow_or_range_err_at`]) now bind
/// directly to the KwargPath-parameterized narrowing gate
/// [`narrow_or_range_mismatch`] one abstraction level up. This wrapper
/// stays as a test-only pinning point for
/// `range_mismatch_binds_wrappers_and_the_typed_payload_to_one_substrate_entry`
/// so a regression that hand-rolled a `LispError::KwargOutOfRange`
/// struct-literal outside [`range_mismatch`] would still register at
/// that test — but no production path calls it. Post-lift the
/// production narrowing surface has ONE named substrate entry
/// ([`narrow_or_range_mismatch`]), and every axis (scalar, per-item,
/// slot) rides its [`KwargPath`] parameter, not per-wrapper sugar.
#[cfg(test)]
fn range_err(key: &str, target: NumericWidth, value: NumericLiteral) -> LispError {
    range_mismatch(kwarg_form(key), target, value)
}

/// Item-indexed sibling of [`range_err`] — the per-item narrowing gate's
/// rejection shape for a list-typed kwarg whose i-th element carried
/// the right `Sexp` shape (an int atom on `:ports (list 80 70000)`)
/// but the value does not fit the field's narrower Rust width
/// (`70000` has no `u16`). Pairs `kwarg_item_form(key, idx)` (the
/// `:<key>[<idx>]` typed path already used by [`type_err_at`]) with
/// the same typed [`NumericWidth`] / [`NumericLiteral`] pair
/// [`range_err`] carries, returning [`LispError::KwargOutOfRange`]
/// with `form: KwargPath::Item { key, idx }` via the shared
/// KwargPath-parameterized [`range_mismatch`] primitive.
///
/// This is the second-to-last member of the typed-entry kwarg gate's
/// per-item rejection vocabulary — shape-per-item lives at
/// [`type_err_at`], and this range-per-item peer completes the axis on
/// the list-typed cousin of [`range_err`]'s scalar-kwarg case. A
/// future span lift (once `Sexp` carries source positions) lands
/// alongside the four `type_err` / `type_err_at` / `range_err` /
/// `range_err_at` primitives in ONE place — every consumer (the
/// scalar narrowing gate, the per-item narrowing gate, the shape gate
/// on scalar kwargs, the shape gate on per-item kwargs) inherits
/// positional rendering mechanically.
///
/// `#[cfg(test)]`: same posture as its scalar sibling [`range_err`]
/// above — the production per-item narrowing wrapper
/// [`narrow_or_range_err_at`] now binds directly to
/// [`narrow_or_range_mismatch`], and this wrapper stays only for the
/// pinning test that asserts the KwargPath-parameterized struct-
/// literal primitive [`range_mismatch`] has one construction site.
#[cfg(test)]
fn range_err_at(key: &str, idx: usize, target: NumericWidth, value: NumericLiteral) -> LispError {
    range_mismatch(kwarg_item_form(key, idx), target, value)
}

/// Required atomic-kwarg extractor — fronts every typed-atom public
/// `extract_X` helper (`extract_string`, `extract_int`, `extract_float`,
/// `extract_bool`). The four byte-identical inline shapes —
///
/// ```ignore
/// let v = required(kw, key)?;
/// v.as_X().ok_or_else(|| type_err(key, "<X-name>", v))
/// ```
///
/// — collapse to ONE generic primitive parameterized by the projection
/// function `project: FnOnce(&'a Sexp) -> Option<T>` and the typed-name
/// label `expected: &'static str`. The four-times rule (THEORY.md §VI.1)
/// is decisively crossed; lifting it into ONE primitive means the next
/// change to the typed-atom failure-projection shape (e.g. threading
/// `pos: Option<usize>` once `Sexp` carries spans, attaching a structural
/// `source: SexpTypeMismatch` chain) lands as ONE signature change inside
/// `extract_atom`, and all four public extractors pick up the upgrade
/// mechanically — no per-extractor edit, no per-extractor test drift.
///
/// `T` is generic so the helper handles both owned (`i64`, `f64`, `bool`)
/// and borrowed (`&'a str`) projections uniformly — the lifetime
/// threading `&'a Sexp → Option<&'a str>` works because every
/// `Sexp::as_*` method is `for<'b> fn(&'b Self) -> Option<…&'b str…>`;
/// the helper inherits that lifetime quantification through
/// `FnOnce(&'a Sexp) -> Option<T>`. Calling `extract_atom(kw, key,
/// "string", Sexp::as_string)` infers `T = &'a str`; calling
/// `extract_atom(kw, key, "int", Sexp::as_int)` infers `T = i64`.
///
/// Sibling of `extract_optional_atom` for the optional kwarg path —
/// together the two close every distinct typed-atom kwarg extractor's
/// shape: required vs. optional, returning `Result<T>` vs.
/// `Result<Option<T>>` from the same underlying projection. Future
/// extension to additional atomic types (e.g. `Atom::Bytes` if/when
/// added) is ONE one-line public delegate plus ONE call site — no
/// new error-path duplication.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition;
/// three-times rule decisively crossed (four byte-identical
/// extract+project+type-err shapes across `extract_string`,
/// `extract_int`, `extract_float`, `extract_bool`). THEORY.md §V.1 —
/// knowable platform / constructive diagnostics: the typed-atom
/// kwarg-failure projection lives in ONE primitive so authoring
/// surfaces (`tatara-check`, REPL, LSP) pick up the diagnostic-shape
/// promotion mechanically once the variant is structurally extended.
/// THEORY.md §II.1 invariant 1 — typed entry; the typed-atom
/// extractor IS the rust-level typed-entry gate for primitive kwargs,
/// and naming its single shape lifts the gate from four-site
/// duplication to one rust function the substrate's diagnostic
/// promotions hang off of.
fn extract_atom<'a, T, F>(
    kw: &'a Kwargs<'a>,
    key: &str,
    expected: ExpectedKwargShape,
    project: F,
) -> Result<T>
where
    F: FnOnce(&'a Sexp) -> Option<T>,
{
    let v = required(kw, key)?;
    project(v).ok_or_else(|| type_err(key, expected, v))
}

/// Optional sibling of `extract_atom` — collapses the four byte-identical
/// inline shapes of `extract_optional_string`, `extract_optional_int`,
/// `extract_optional_float`, `extract_optional_bool`:
///
/// ```ignore
/// match kw.get(key) {
///     None => Ok(None),
///     Some(v) => v.as_X().map(Some).ok_or_else(|| type_err(key, "<X-name>", v)),
/// }
/// ```
///
/// into ONE generic primitive. Same `T`/`project`/`expected` shape as
/// `extract_atom`; the difference is the present-vs-absent short-circuit
/// at the `None` arm — an absent kwarg is not an error for optional
/// extractors, only a malformed-present one is.
///
/// Post-lift the primitive delegates through [`optional_from_required`]
/// with [`extract_atom`] in the required slot — the SEVENTH consumer
/// of the substrate's present-vs-absent bifurcation primitive after
/// the four list-family peers ([`extract_optional_string_list`] /
/// [`extract_optional_bool_list`] / [`extract_optional_narrowed_list`]
/// / [`extract_optional_vec_via_serde`]) and the two owned-scalar
/// peers ([`extract_optional_via_serde`] on the universal-serde axis,
/// [`extract_optional_narrowed`] on the numeric-narrowed axis). The
/// borrowed-`T` shape (`T = &'a str` on the string axis, via
/// [`AtomKwarg`]'s `'a` trait lifetime threading through
/// [`AtomKwarg::project`]) rides the primitive's `'a` lifetime through
/// the `F: FnOnce(&'a Kwargs<'a>, &str) -> Result<T>` bound — the same
/// bound the six owned-`T` peers coerce their fn-item `for<'x>
/// fn(&'x Kwargs<'x>, &str) -> Result<T>` HRTB signatures to at each
/// call site.
///
/// Pre-lift the primitive spelled the two-arm bifurcation inline
/// (`match optional(kw, key) { None => Ok(None); Some(v) =>
/// project(v).map(Some).ok_or_else(|| type_err(key, expected, v)) }`)
/// — the LAST inline present-vs-absent bifurcation on the
/// extract_optional_ family. Post-lift the substrate primitive owns
/// the bifurcation at ONE named site across every axis (list-family
/// plus scalar universal-serde plus scalar numeric-narrowed plus
/// scalar atom-shape); a future diagnostic promotion on the present-
/// vs-absent gate (a probe, a metric, a span) lands at ONE owner
/// and flows to the atom-family scalar peer here without a per-
/// caller edit.
///
/// The delegate wraps [`extract_atom`] in a closure so the primitive's
/// `FnOnce(&'a Kwargs<'a>, &str) -> Result<T>` extractor slot receives
/// the atom-family's `(expected, project)` pair through the closure's
/// capture. The wrap re-runs the kwarg lookup inside [`extract_atom`]'s
/// [`required`] call on the present arm — a second HashMap probe past
/// the primitive's outer `kw.contains_key(key)` gate — matching the
/// double-lookup posture the six pre-existing owned-`T` peers already
/// accept in exchange for the SAME uniform bifurcation site every
/// diagnostic-promotion consumer binds to. A future
/// [`optional_from_required`] optimization that threads the resolved
/// `&Sexp` through the required extractor's slot (collapsing the
/// double-lookup at ONE substrate primitive) lands here and every
/// existing peer inherits the improvement mechanically.
///
/// Future structural promotion of the type-mismatch diagnostic lands
/// at ONE call site inside [`extract_atom`] — same property as
/// [`extract_atom`]'s scalar-required peer — AND a future promotion
/// of the present-vs-absent gate lands at ONE call site inside
/// [`optional_from_required`], flowing through this atom-family
/// scalar peer alongside every other extract_optional_ consumer.
fn extract_optional_atom<'a, T, F>(
    kw: &'a Kwargs<'a>,
    key: &str,
    expected: ExpectedKwargShape,
    project: F,
) -> Result<Option<T>>
where
    F: FnOnce(&'a Sexp) -> Option<T>,
{
    optional_from_required(kw, key, |kw, key| extract_atom(kw, key, expected, project))
}

/// List-typed kwarg extractor — fronts every public `extract_*` helper
/// that reads a kwarg as a `Sexp::List` and projects each element to an
/// owned `T`. The two byte-identical inline skeletons —
///
/// ```ignore
/// let Some(v) = kw.get(key).copied() else { return Ok(Vec::new()) };
/// let list = v.as_list().ok_or_else(|| type_err(key, <list-shape>, v))?;
/// list.iter().enumerate().map(<per-item>).collect()
/// ```
///
/// — `extract_string_list` (each item projected via `as_string`, per-item
/// failure via `type_err_at`) and `extract_vec_via_serde` (each item via
/// `from_value_with_path`, per-item failure carrying `KwargPath::item`) —
/// collapse to ONE generic primitive parameterized by the outer-shape
/// label `list_shape: ExpectedKwargShape` and the per-element projection
/// `item: FnMut(usize, &Sexp) -> Result<T>`. The skeleton owns the three
/// fixed decisions both extractors share: absent kwarg → `Ok(Vec::new())`
/// (an absent list kwarg is the empty list, never an error — same posture
/// `extract_optional_atom` takes for absent atoms); present-but-not-a-list
/// → `type_err(key, list_shape, v)` (the outer-shape gate, labeled by the
/// caller-supplied `list_shape` so `ListOfStrings` vs. `List` stays a
/// per-caller decision, not baked into the skeleton); and the
/// `iter().enumerate().map(item).collect()` per-element walk that threads
/// the element index into the projection so per-item diagnostics can name
/// `:<key>[<idx>]` without re-counting from the source.
///
/// This is the list-family sibling of `extract_atom` / `extract_optional_atom`
/// (the atom-family generic projection primitives). Together the three close
/// every distinct typed-kwarg extractor's outer skeleton: required atom,
/// optional atom, and list. The per-element projection is `FnMut(usize,
/// &Sexp) -> Result<T>` — generic over `T` so it handles both the owned-
/// `String` (`extract_string_list`) and `DeserializeOwned`-`T`
/// (`extract_vec_via_serde`) element shapes uniformly, and threading the
/// `usize` index lets the projection construct the item-keyed
/// `KwargPath::Item { key, idx }` / `type_err_at` path the per-item gate
/// reports through.
///
/// Future structural promotion of the outer not-a-list diagnostic, or a
/// move to a fallible-streaming collect that short-circuits on the first
/// bad element with its position, lands at ONE site inside this helper —
/// both public list extractors pick up the upgrade mechanically, same
/// property `extract_atom` gives the four atom extractors.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// list-typed extractor skeleton recurs at two sites (the PRIME-DIRECTIVE
/// ≥2 trigger) and is lifted to one owner, exactly as the atom skeleton was.
/// THEORY.md §V.1 — knowable platform; the list-kwarg outer gate + per-item
/// path live in ONE primitive so authoring surfaces (`tatara-check`, REPL,
/// LSP) pick up diagnostic-shape promotions once, not per-extractor.
/// THEORY.md §II.1 invariant 1 — typed entry; the list extractor IS the
/// rust-level typed-entry gate for list-shaped kwargs, and naming its single
/// skeleton lifts the gate from two-site duplication to one function the
/// substrate's diagnostic promotions hang off of.
fn extract_list<T, F>(
    kw: &Kwargs<'_>,
    key: &str,
    list_shape: ExpectedKwargShape,
    mut item: F,
) -> Result<Vec<T>>
where
    F: FnMut(usize, &Sexp) -> Result<T>,
{
    let Some(v) = optional(kw, key) else {
        return Ok(Vec::new());
    };
    let list = v.as_list().ok_or_else(|| type_err(key, list_shape, v))?;
    list.iter()
        .enumerate()
        .map(|(idx, e)| item(idx, e))
        .collect()
}

/// Present-vs-absent bifurcation over an absent-tolerant required
/// extractor — the SAME `if kw.contains_key(key) {
/// extract_required(kw, key).map(Some) } else { Ok(None) }` shape that
/// lived inline at THREE call sites past this lift
/// ([`extract_optional_string_list`] / [`extract_optional_bool_list`] /
/// [`extract_optional_narrowed_list`]), collapsed to ONE substrate
/// primitive.
///
/// Semantic role: convert an absent-tolerant required extractor
/// (one whose absent-kwarg posture must not leak into the optional
/// peer — [`extract_list`]'s `Ok(Vec::new())` outer contract on the
/// list-family axis, [`extract_atom`]'s `LispError::MissingKwarg`
/// rejection on the scalar-atom axis via [`required`]) into its
/// `Option<T>`-shaped optional peer (whose absent-kwarg posture is
/// `Ok(None)`). On the list-family axis this preserves the
/// load-bearing distinction between `None` (absent) and
/// `Some(vec![])` (a PRESENT empty list) — the required peer
/// collapses "no items" and "no kwarg" to the same empty vec, which
/// fits `Vec<T>` fields where the two are semantically equivalent
/// but loses the operator's intent on `Option<Vec<T>>` fields where
/// "the operator did not name this kwarg" and "the operator
/// explicitly bound the kwarg to an empty list" are DISTINCT. On the
/// scalar-atom axis (via [`extract_optional_atom`], which delegates
/// through this primitive) it flips the required peer's typed-entry
/// rejection on absence (`LispError::MissingKwarg`) into the
/// operator-intentional `Ok(None)` on the optional axis — the
/// missing-kwarg gate is a REQUIRED-peer contract, not an
/// OPTIONAL-peer contract.
///
/// The primitive's contract:
///   1. Absent kwarg → `Ok(None)`. `extract_required` is NOT invoked,
///      so neither the list-family peer's absent-tolerant posture
///      (`Ok(Vec::new())`) nor the scalar-atom peer's absent-rejecting
///      posture (`LispError::MissingKwarg`) leaks into the optional
///      peer's result.
///   2. Present kwarg → `extract_required(kw, key).map(Some)`. Every
///      rejection variant the required extractor emits (outer-shape
///      mismatch on `:tags 7`, per-item shape/narrowing rejection on
///      `:tags (list "a" 7)` / `:ports (list 80 70000)`, scalar-atom
///      shape mismatch on `:name 5` on the atom-family axis)
///      propagates unchanged, wrapped in `Err(_)`; a successful
///      decode wraps in `Some(_)`. The two peers on every axis
///      share ONE rejection vocabulary.
///
/// The `'a` lifetime rides both the `kw` parameter (`&'a Kwargs<'a>`,
/// matching the `&'a Kwargs<'a>` shape [`extract_optional_atom`] +
/// [`AtomKwarg::extract_optional_kwarg`] already carry) AND the
/// `extract_required` bound (`F: FnOnce(&'a Kwargs<'a>, &str) ->
/// Result<T>`) so `T` may borrow from `kw` — an
/// [`extract_optional_atom`]-shape caller whose `T = &'a str` on the
/// string axis threads its borrowed slot through the primitive
/// without an intermediate copy or an axis-only inline duplication.
/// Owned-`T` callers (the six list-family + scalar-serde peers
/// listed below) pass `fn` items with a `for<'x> fn(&'x Kwargs<'x>,
/// &str) -> Result<T>` HRTB signature; the compiler coerces each
/// item to `FnOnce(&'a Kwargs<'a>, &str) -> Result<T>` at the
/// specific `'a` chosen by the call site, so no per-caller signature
/// change is needed on the pre-existing consumers.
///
/// Peer to [`extract_optional_atom`] on the atom-family (scalar) axis:
/// both encode the present-vs-absent bifurcation as a private
/// substrate primitive, differing only in the shape of the required
/// half — [`extract_optional_atom`] takes an `(expected, project)`
/// pair and delegates through this primitive with `extract_atom` in
/// the required slot; the six list-family + scalar-serde peers
/// listed below take a full required-shape extractor closure
/// directly. Post-lift the two shapes route through the SAME
/// primitive, so every axis of the extract_optional_ family binds
/// its present-vs-absent gate at ONE substrate site.
///
/// A future new optional peer — a hypothetical
/// `extract_optional_symbol_list` (paired with a `Symbol` atom
/// impl of [`AtomKwarg`]), or an `extract_optional_vec_domain<D>`
/// for `Option<Vec<Nested>>` fields once the derive's
/// `Kind::VecDeserialize` catch-all sharpens into a typed nested-
/// domain arm — plugs in as a one-line delegate to this primitive,
/// same posture the SEVEN existing optional peers (four list-family
/// — [`extract_optional_string_list`] /
/// [`extract_optional_bool_list`] /
/// [`extract_optional_narrowed_list`] /
/// [`extract_optional_vec_via_serde`] — plus the scalar-family
/// universal-serde peer [`extract_optional_via_serde`], the
/// scalar-family numeric-narrowed peer
/// [`extract_optional_narrowed`], and the scalar-family atom-shape
/// peer [`extract_optional_atom`]) now take. Post-lift the
/// extract_optional_ family reaches its structural terminus: every
/// public optional peer routes its present-vs-absent bifurcation
/// through ONE named substrate primitive, with no inline
/// exceptions.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// present-vs-absent + delegate-to-required shape recurs at SEVEN
/// peer sites past the three-times-rule trigger, four on the list-
/// family axis + three on the scalar family (universal-serde +
/// numeric-narrowed + atom-shape), all sharing the byte-identical
/// `if kw.contains_key(key) {...} else { Ok(None) }` present-vs-
/// absent contract pre-lift (the pre-lift shape lived as an inline
/// `contains_key` bifurcation at the four list-family sites, as an
/// [`extract_optional_atom`]-composed shape at the two owned-scalar
/// sites, and as an inline `match optional(kw, key)` bifurcation at
/// [`extract_optional_atom`] itself — every variant short-circuits
/// at the SAME `kw.contains_key` moment, only the substrate-
/// primitive owning the check differs); lifting the shape to ONE
/// primitive closes them at rustc time). THEORY.md §II.1 invariant
/// 2 (free middle — the optional-vs-required peer distinction lives
/// at ONE substrate primitive, not restated at every optional-peer
/// callsite; a future diagnostic promotion on the present-vs-absent
/// gate — a probe, a metric, a span — lands at THIS one owner and
/// flows to every existing + future optional peer with no per-
/// caller edit). THEORY.md §V.1 (knowable platform — every optional
/// peer's present-vs-absent bifurcation (list-family + scalar
/// universal-serde + scalar numeric-narrowed + scalar atom-shape)
/// now routes through ONE named substrate primitive, so a future
/// audit-trail metric jointly labeled by "which optional peer fired"
/// and "was the kwarg present" binds mechanically without a per-peer
/// bifurcation re-implementation).
fn optional_from_required<'a, F, T>(
    kw: &'a Kwargs<'a>,
    key: &str,
    extract_required: F,
) -> Result<Option<T>>
where
    F: FnOnce(&'a Kwargs<'a>, &str) -> Result<T>,
{
    if kw.contains_key(key) {
        extract_required(kw, key).map(Some)
    } else {
        Ok(None)
    }
}

/// Atom-typed kwarg trait — the non-numeric peer of [`WideNumeric`]
/// on the atom-family kwarg extraction axis. Bundles the two
/// per-atom primitives every non-numeric atom extractor used to
/// spell inline ([`Self::SHAPE`] — the axis-typed rejection label;
/// [`Self::project`] — the `&Sexp -> Option<Self>` projection) and
/// provides [`Self::extract_kwarg`] / [`Self::extract_optional_kwarg`]
/// as defaults composing them through the shared [`extract_atom`] /
/// [`extract_optional_atom`] atom-family skeletons.
///
/// Pre-lift: four hand-rolled two-argument `extract_atom` /
/// `extract_optional_atom` calls at [`extract_string`] /
/// [`extract_optional_string`] / [`extract_bool`] /
/// [`extract_optional_bool`], each spelling its atom's
/// `ExpectedKwargShape` and `Sexp::as_<projection>` pair by hand.
/// Post-lift: four one-line delegates to the trait dispatch, with
/// the per-atom pair pinned as ONE trait impl per axis
/// (`AtomKwarg<'a> for &'a str` and `AtomKwarg<'a> for bool`). A
/// regression that silently swapped one extractor's projection (a
/// string axis silently reading `Sexp::as_bool`, so `:name #t`
/// would land as `"true"` in a `String` field) cannot survive the
/// trait dispatch — the impl's return type pins the axis at rustc
/// time.
///
/// Peer to [`WideNumeric`] on the numeric axes: both traits share
/// the same skeleton (SHAPE + projection → default extract_kwarg /
/// extract_optional_kwarg composing extract_atom); the difference
/// is that `WideNumeric` also bundles the numeric-narrowing wide-
/// into-literal lift ([`WideNumeric::as_literal`]) needed by the
/// narrowing pipeline. A future run that lifts `WideNumeric` to
/// extend `AtomKwarg` as a supertrait (`WideNumeric: for<'a>
/// AtomKwarg<'a>`) would collapse the two atom-family skeletons to
/// ONE, keeping `WideNumeric` as the numeric-specific extension of
/// the shared atom-kwarg contract — no per-caller change needed on
/// the numeric-narrowing side, because the SHAPE + projection pair
/// numeric callers reach through today would flow via inheritance.
///
/// The `'a` lifetime rides the trait so `&'a str` (whose atom
/// projection borrows from the sexp) can implement it alongside
/// `bool` (which is owned). The `Self: Sized` bound holds trivially
/// for both.
///
/// A new atom-shaped kwarg type — a future `Symbol<'a>` primitive
/// paired with `Sexp::as_symbol_str`, or a `Duration` primitive
/// pairing an `ExpectedKwargShape::Duration` extension with a
/// bespoke `Sexp` projection — plugs in via ONE `impl AtomKwarg`
/// block (the SHAPE + projection pair) and picks up
/// [`Self::extract_kwarg`] + [`Self::extract_optional_kwarg`]
/// mechanically. No new public extractor names, no re-hand-rolled
/// `extract_atom` skeleton.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 (typed entry — the
/// atom-typed kwarg gate IS the rust-level typed-entry rejection
/// for primitive kwargs). THEORY.md §VI.1 (generation over
/// composition — the SHAPE + projection pair recurs at FOUR sites
/// past the three-times-rule trigger; lifting it to one trait
/// skeleton closes them at rustc time). THEORY.md §V.1 (knowable
/// platform — the four public extractors share ONE trait dispatch,
/// so a future diagnostic promotion at [`extract_atom`] /
/// [`extract_optional_atom`] flows through the trait defaults into
/// every atom-kwarg extractor without per-extractor edits).
pub trait AtomKwarg<'a>: Sized {
    /// The axis's typed [`ExpectedKwargShape`] — the rejection
    /// label this atom's outer type-gate quotes on a shape
    /// mismatch. `<&'a str as AtomKwarg<'a>>::SHAPE` IS
    /// [`ExpectedKwargShape::String`]; `<bool as AtomKwarg<'a>>::SHAPE`
    /// IS [`ExpectedKwargShape::Bool`]. A regression that silently
    /// swapped an axis's rejection label (a string axis returning
    /// `Bool` on `:name 5`, letting the operator see the wrong
    /// expected shape) would fail to compile the impl's const
    /// clause.
    const SHAPE: ExpectedKwargShape;

    /// The axis's typed OUTER-LIST [`ExpectedKwargShape`] — the
    /// rejection label the LIST-family peer of [`Self::SHAPE`]
    /// quotes when a list-typed kwarg's outer value isn't a
    /// `Sexp::List(_)` at all. Bundled here so the four list-family
    /// extractors ([`extract_string_list`] on the `<&str>` axis,
    /// [`extract_bool_list`] on the `<bool>` axis, and
    /// [`extract_narrowed_list<W, T>`] on both wide-numeric axes
    /// through `W: WideNumeric: AtomKwarg`) bind their outer-shape
    /// gate through ONE per-axis trait dispatch rather than
    /// as a per-extractor inline `ExpectedKwargShape` literal.
    ///
    /// Defaults to [`ExpectedKwargShape::List`] — the bare-list
    /// rejection label a hypothetical new atom-family axis (a
    /// `Symbol` primitive, say) would inherit before authoring its
    /// own override. All four axes that ship today override this
    /// default: `<&'a str>` → [`ExpectedKwargShape::ListOfStrings`],
    /// `<bool>` → [`ExpectedKwargShape::ListOfBools`],
    /// `<i64>` → [`ExpectedKwargShape::ListOfInts`],
    /// `<f64>` → [`ExpectedKwargShape::ListOfNumbers`]. Post-lift no
    /// list-family extractor emits the ambiguous bare-list label on
    /// any reachable axis; a `:tags "solo"` kwarg rejects as
    /// `expected list of strings, got string`, a `:flags #t` kwarg
    /// as `expected list of bools, got bool`, a `:ports 80` kwarg
    /// into `Vec<u16>` as `expected list of ints, got int`, and a
    /// `:scales 1.0` kwarg into `Vec<f32>` as `expected list of
    /// numbers, got number` — each through the SAME per-axis trait
    /// dispatch mechanism. A future new atom-family axis binding an
    /// element-typed refinement variant lands as ONE `LIST_SHAPE`
    /// override on the new axis's impl, and every list-family
    /// extractor already reaching through the trait dispatch picks
    /// it up mechanically — no per-extractor edit.
    ///
    /// Peer to [`Self::SHAPE`] on the axis-typed rejection-label
    /// surface: `SHAPE` names the SCALAR-atom shape the extractor
    /// emits at the scalar-kwarg gate ([`extract_atom`] on
    /// `:name 5`) AND at the LIST-family per-item gate
    /// ([`Self::project_at`] on `:tags (list "a" 5)`); `LIST_SHAPE`
    /// names the OUTER-LIST shape the LIST-family extractor emits
    /// when the kwarg's value isn't a list at all
    /// ([`extract_list`] on `:tags "solo"`). The two rejection
    /// labels are on structurally different sites (per-item vs.
    /// outer-shape) and stay decoupled; the axis identity binds
    /// through the `Self` type parameter on both.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 2 (free middle —
    /// the outer-list rejection label lives at ONE per-axis
    /// substrate primitive, not restated at every list-extractor
    /// callsite; a future axis-typed refinement on the closed set
    /// lands at ONE trait override per axis and flows to every
    /// existing + future list extractor with no per-caller edit).
    /// THEORY.md §VI.1 (generation over composition — the outer-
    /// list shape recurs at THREE list-family sites past the
    /// three-times-rule trigger; lifting it to ONE per-axis trait
    /// const closes them at rustc time).
    const LIST_SHAPE: ExpectedKwargShape = ExpectedKwargShape::List;

    /// The axis's per-[`Sexp`] atom projection — the SAME
    /// `Option<Self>` a hand-written [`extract_atom`] call would
    /// pass as its `project` function. `<&'a str as
    /// AtomKwarg<'a>>::project` IS [`Sexp::as_string`]; `<bool as
    /// AtomKwarg<'_>>::project` IS [`Sexp::as_bool`].
    fn project(sexp: &'a Sexp) -> Option<Self>;

    /// Required atom-kwarg extractor — reads the kwarg at `key`,
    /// projects it via [`Self::project`], and lifts a `None` (present-
    /// but-not-this-axis or missing) into the atom-family
    /// `LispError::TypeMismatch { expected: Self::SHAPE }`
    /// rejection through the shared [`extract_atom`] skeleton.
    /// Provided as a trait default composing `(Self::SHAPE,
    /// Self::project)` — per-atom impls do not override this.
    fn extract_kwarg(kw: &'a Kwargs<'a>, key: &str) -> Result<Self> {
        extract_atom(kw, key, Self::SHAPE, Self::project)
    }

    /// `Option` sibling of [`Self::extract_kwarg`] — same per-axis
    /// dispatch shape, absent kwarg short-circuits to `Ok(None)`
    /// via the shared [`extract_optional_atom`] skeleton.
    fn extract_optional_kwarg(kw: &'a Kwargs<'a>, key: &str) -> Result<Option<Self>> {
        extract_optional_atom(kw, key, Self::SHAPE, Self::project)
    }

    /// Item-indexed shape gate — the LIST-family per-item peer of
    /// [`Self::extract_kwarg`] on the same `(SHAPE, project)` per-axis
    /// primitive pair. Projects `sexp` via [`Self::project`] and lifts
    /// a `None` (present-but-not-this-axis) into the atom-family
    /// `LispError::TypeMismatch { form: KwargPath::Item { key, idx },
    /// expected: Self::SHAPE }` rejection through the shared
    /// [`type_err_at`] item-indexed rejection primitive.
    ///
    /// Where [`Self::extract_kwarg`] composes `(SHAPE, project)`
    /// through the SCALAR-kwarg outer skeleton [`extract_atom`],
    /// [`Self::project_at`] composes the SAME per-axis primitives
    /// through the LIST-item outer skeleton [`type_err_at`]. The
    /// same axis rides both scalar and per-item shape gates, so a
    /// caller reaching for the atom-family shape rejection at either
    /// path lands on ONE per-axis primitive pair.
    ///
    /// Peer to [`narrow_or_range_err_at`] on the shape (not range)
    /// axis: both are per-item rejection primitives sharing a
    /// `KwargPath::Item` typed path and an axis-typed target,
    /// differing only in which axis-typed target rides the
    /// rejection — [`Self::project_at`] rides [`Self::SHAPE`] on the
    /// shape gate; [`narrow_or_range_err_at`] rides `T::WIDTH` on
    /// the range gate.
    ///
    /// THREE per-item projection sites route through this default —
    /// [`extract_string_list`]'s per-item body on the `<&'a str>`
    /// axis, [`extract_narrowed_list`]'s per-item body on the
    /// `<W as WideNumeric>` axis (`W` inheriting [`AtomKwarg`]'s
    /// shape gate through the supertrait bound), and
    /// [`extract_bool_list`]'s per-item body on the `<bool>` axis.
    /// The first two sites used to inline this compose shape
    /// (`Self::project(s).ok_or_else(|| type_err_at(key, idx,
    /// Self::SHAPE, s))`); the third routed `Vec<bool>` through
    /// [`extract_vec_via_serde`]'s serde bridge, so its per-item
    /// shape mismatch surfaced as a
    /// [`LispError::KwargDeserialize`] substring rather than a typed
    /// atom-family rejection. Post-lift the axis identity rides the
    /// `Self` type parameter through the SAME atom-family
    /// shape-gate composition at all three sites rather than
    /// restated as a per-site pair (or leaking through the serde
    /// bridge on the bool axis).
    ///
    /// Provided as a trait default composing `(Self::SHAPE,
    /// Self::project)` through [`type_err_at`] — per-atom impls do
    /// not override this. A future diagnostic promotion at
    /// [`type_err_at`] (a span, a suggested-value hint) flows
    /// through ONE trait default into every atom-family per-item
    /// projection site with no per-caller edit.
    fn project_at(key: &str, idx: usize, sexp: &'a Sexp) -> Result<Self> {
        Self::project(sexp).ok_or_else(|| type_err_at(key, idx, Self::SHAPE, sexp))
    }
}

impl<'a> AtomKwarg<'a> for &'a str {
    const SHAPE: ExpectedKwargShape = ExpectedKwargShape::String;

    /// String-axis override of the [`AtomKwarg::LIST_SHAPE`] trait
    /// default — the element-typed refinement
    /// [`ExpectedKwargShape::ListOfStrings`] that
    /// [`extract_string_list`] emits at its outer-shape gate. Peer
    /// to the three sibling axis overrides that close the atom-
    /// family list surface: `<bool>` → `ListOfBools`,
    /// `<i64>` → `ListOfInts`, `<f64>` → `ListOfNumbers`. Post-lift
    /// all four atom-family list-family axes carry an element-
    /// typed refinement label, and no list-family extractor
    /// emits the ambiguous trait default
    /// [`ExpectedKwargShape::List`] on any reachable axis.
    const LIST_SHAPE: ExpectedKwargShape = ExpectedKwargShape::ListOfStrings;

    fn project(sexp: &'a Sexp) -> Option<Self> {
        sexp.as_string()
    }
}

impl<'a> AtomKwarg<'a> for bool {
    const SHAPE: ExpectedKwargShape = ExpectedKwargShape::Bool;

    /// Bool-axis override of the [`AtomKwarg::LIST_SHAPE`] trait
    /// default — the element-typed refinement
    /// [`ExpectedKwargShape::ListOfBools`] that
    /// [`extract_bool_list`] emits at its outer-shape gate through
    /// the per-axis trait dispatch. Peer to the three sibling axis
    /// overrides that close the atom-family list surface:
    /// `<&'a str>` → `ListOfStrings`, `<i64>` → `ListOfInts`,
    /// `<f64>` → `ListOfNumbers`. Post-lift the four overrides
    /// collapse the pre-lift ambiguous `expected list, got X`
    /// diagnostic into the axis-typed `expected list of bools, got
    /// X` / `expected list of strings, got X` / `expected list of
    /// ints, got X` / `expected list of numbers, got X` refinements
    /// at EVERY atom-family list-family site through ONE per-axis
    /// trait-const dispatch each — no axis falls through to the
    /// bare-list label any more, on the two non-numeric axes or on
    /// the two wide-numeric axes.
    const LIST_SHAPE: ExpectedKwargShape = ExpectedKwargShape::ListOfBools;

    fn project(sexp: &'a Sexp) -> Option<Self> {
        sexp.as_bool()
    }
}

// Numeric-axis impls of the shared atom-family contract — the wide-int
// (`i64`) and wide-float (`f64`) axes ride the SAME `SHAPE + project`
// primitive pair the non-numeric atom axes (`&str`, `bool`) close.
// Post-supertrait-lift these two impls carry the axis-typed
// `ExpectedKwargShape` + per-atom projection the [`WideNumeric`] trait
// used to name inline; the [`WideNumeric`] impls below then extend the
// shared contract with the numeric-only `as_literal` method that maps a
// wide value into its [`NumericLiteral`] carrier for the
// `narrow_or_range_err` rejection primitive. The lift pins one owner
// for the atom-family composition (`extract_atom(kw, key, SHAPE,
// project)`) across BOTH the numeric and non-numeric axes: the
// composition lives on [`AtomKwarg::extract_kwarg`] /
// [`AtomKwarg::extract_optional_kwarg`] alone, no longer restated on
// [`WideNumeric`].
impl<'a> AtomKwarg<'a> for i64 {
    const SHAPE: ExpectedKwargShape = ExpectedKwargShape::Int;

    /// Wide-int-axis override of the [`AtomKwarg::LIST_SHAPE`] trait
    /// default — the element-typed refinement
    /// [`ExpectedKwargShape::ListOfInts`] that
    /// [`extract_narrowed_list::<i64, T>`] emits at its outer-shape
    /// gate through the per-axis trait dispatch (reached from the
    /// generic list extractor via the
    /// `W: WideNumeric: for<'a> AtomKwarg<'a>` supertrait bound).
    /// Every narrow-int width the extractor is instantiated at
    /// (`u16` / `i32` / `usize` / …) inherits the sharpened outer-
    /// shape label mechanically — no per-extractor edit, no per-
    /// width bookkeeping. Sibling to the `<f64>` axis's
    /// `ListOfNumbers` override and the two non-numeric axes'
    /// `<&'a str>::LIST_SHAPE = ListOfStrings` /
    /// `<bool>::LIST_SHAPE = ListOfBools` overrides; together the
    /// four axis overrides close every reachable atom-family list-
    /// surface axis at the trait-dispatch layer, leaving no axis
    /// on the ambiguous trait default [`ExpectedKwargShape::List`].
    const LIST_SHAPE: ExpectedKwargShape = ExpectedKwargShape::ListOfInts;

    fn project(sexp: &'a Sexp) -> Option<Self> {
        sexp.as_int()
    }
}

impl<'a> AtomKwarg<'a> for f64 {
    const SHAPE: ExpectedKwargShape = ExpectedKwargShape::Number;

    /// Wide-float-axis override of the [`AtomKwarg::LIST_SHAPE`]
    /// trait default — the element-typed refinement
    /// [`ExpectedKwargShape::ListOfNumbers`] that
    /// [`extract_narrowed_list::<f64, T>`] emits at its outer-shape
    /// gate through the per-axis trait dispatch. The `"numbers"`
    /// (not `"floats"`) naming mirrors the scalar-axis
    /// [`Self::SHAPE = Number`] posture: the per-item gate
    /// [`Self::project = Sexp::as_float`] accepts BOTH
    /// `Sexp::Atom(Float(_))` and `Sexp::Atom(Int(_))`, so the
    /// outer-shape label names the union rather than the narrower
    /// `"floats"`. Every narrow-float width (`f32` / …) inherits
    /// the sharpened outer-shape label mechanically through the
    /// generic list extractor's supertrait bound.
    ///
    /// Peer to the `<i64>::LIST_SHAPE = ListOfInts` override on the
    /// wide-int axis — together the two wide-numeric overrides
    /// close the last two atom-family list-surface axes that used
    /// to fall through to the ambiguous trait default
    /// [`ExpectedKwargShape::List`]. Post-lift the four
    /// atom-family list extractors' outer-shape rejection labels
    /// form a total axis-typed carving: `<&str>` → `ListOfStrings`,
    /// `<bool>` → `ListOfBools`, `<i64>` → `ListOfInts`,
    /// `<f64>` → `ListOfNumbers`.
    const LIST_SHAPE: ExpectedKwargShape = ExpectedKwargShape::ListOfNumbers;

    fn project(sexp: &'a Sexp) -> Option<Self> {
        sexp.as_float()
    }
}

/// Required string kwarg — one-line delegate to `<&'a str as
/// AtomKwarg<'a>>::extract_kwarg`, whose default composes
/// `(<&'a str>::SHAPE, <&'a str>::project)` through [`extract_atom`].
pub fn extract_string<'a>(kw: &'a Kwargs<'a>, key: &str) -> Result<&'a str> {
    <&'a str as AtomKwarg<'a>>::extract_kwarg(kw, key)
}

/// `Option` sibling of [`extract_string`] — one-line delegate to
/// `<&'a str as AtomKwarg<'a>>::extract_optional_kwarg`.
pub fn extract_optional_string<'a>(kw: &'a Kwargs<'a>, key: &str) -> Result<Option<&'a str>> {
    <&'a str as AtomKwarg<'a>>::extract_optional_kwarg(kw, key)
}

pub fn extract_string_list(kw: &Kwargs<'_>, key: &str) -> Result<Vec<String>> {
    extract_list(kw, key, <&str as AtomKwarg<'_>>::LIST_SHAPE, |idx, s| {
        <&str as AtomKwarg<'_>>::project_at(key, idx, s).map(String::from)
    })
}

/// `Option<Vec<String>>` sibling of [`extract_string_list`] — the
/// list-family peer of [`extract_optional_string`] on the
/// present-vs-absent axis. Distinguishes an ABSENT kwarg (`Ok(None)`)
/// from a PRESENT empty list (`Ok(Some(Vec::new()))`) — the
/// [`extract_string_list`]-flavored posture collapses both cases to
/// `Ok(Vec::new())`, which fits `Vec<String>` fields (where "no items"
/// and "no kwarg" are semantically the same) but loses the present-vs-
/// absent distinction a `Option<Vec<String>>` field needs (where
/// `None` and `Some(vec![])` are DISTINCT operator intents — "the
/// operator did not name this kwarg" vs. "the operator explicitly
/// bound the kwarg to an empty list").
///
/// A present-but-non-list kwarg (`:tags "solo"`) rejects with the
/// SAME typed [`LispError::TypeMismatch`] variant the required peer
/// [`extract_string_list`] emits (`expected list of strings, got
/// string`); a per-item non-string element inside a present list
/// (`:tags (list "a" 5 "b")`) rejects with the SAME
/// [`LispError::TypeMismatch { form: Item { key, idx }, expected:
/// String, got: Int }`] variant its per-item peer emits through the
/// shared [`AtomKwarg::project_at`] atom-family shape gate. The two
/// peers ([`extract_string_list`] and this one) delegate to the SAME
/// per-item gate — only the outer present-vs-absent axis's return
/// shape differs (`Vec<String>` vs. `Option<Vec<String>>`).
///
/// Sibling posture to [`extract_optional_string`] on the atom-family
/// non-numeric axis: both distinguish `None` (absent) from `Some(_)`
/// (present-and-decoded) at the `Option` layer, and both delegate
/// their present-branch decode to their required peer
/// ([`extract_string`] / [`extract_string_list`]). Peer to
/// [`extract_optional_via_serde`] on the universal-serde-fallthrough
/// axis: both share the `kw.contains_key(key)` present-vs-absent
/// bifurcation; the differences are that this extractor rides the
/// typed atom-family per-item shape gate rather than the serde
/// bridge's per-item `from_value_with_path` decode, so a per-item
/// shape mismatch surfaces as a pattern-matchable
/// [`LispError::TypeMismatch`] variant rather than as a
/// [`LispError::KwargDeserialize`] substring.
///
/// Pre-lift the derive routed `Option<Vec<String>>` through the
/// universal-serde bridge ([`extract_optional_via_serde`] via
/// [`crate::domain::from_value_with_path`]) — [`classify_option`] in
/// [`tatara_lisp_derive`] had no arm for `Option<Vec<T>>` and fell
/// through to `Kind::OptionalDeserialize`, matching the pre-lift
/// posture that [`Vec<bool>`] / [`Vec<u16>`] / etc. shared before
/// their per-axis routes landed. So a per-item shape mismatch inside
/// `Option<Vec<String>>` surfaced as a mystery
/// `LispError::KwargDeserialize { message: "invalid type: integer
/// ..., expected a string at path .1" }` substring rather than as
/// the typed
/// `LispError::TypeMismatch { form: Item { key, idx }, expected:
/// String, got: Int }` its required peer's per-item gate already
/// emits — the SAME class of gate-leak the recent per-axis
/// [`extract_bool_list`] / [`extract_narrowed_list`] / [`Kind::VecBool`] /
/// [`Kind::VecInt`] closures already closed on the required-side
/// `Vec<T>` surface. Post-lift the two `Option<Vec<T>>` / `Vec<T>`
/// peers on the string axis share ONE rejection vocabulary.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 (typed entry — a
/// per-item shape failure on an `Option<Vec<String>>` kwarg IS a
/// typed-entry gate the optional-string-vec surface used to leak
/// past through the serde bridge). THEORY.md §VI.1 (generation over
/// composition — the present-vs-absent + delegate-to-required
/// posture recurs at THREE sites now — [`extract_optional_string`] /
/// [`extract_optional_bool`] / this — all routing through the SAME
/// [`optional`] short-circuit + required-peer delegation). THEORY.md
/// §V.1 (knowable platform — the per-item string rejection now
/// surfaces the same pattern-matchable
/// [`LispError::TypeMismatch`] variant every authoring surface (LSP,
/// `tatara-check`, REPL) already binds to for the required peer).
pub fn extract_optional_string_list(kw: &Kwargs<'_>, key: &str) -> Result<Option<Vec<String>>> {
    optional_from_required(kw, key, extract_string_list)
}

/// Required integer kwarg — one-line delegate to `<i64 as
/// WideNumeric>::extract_kwarg`, whose default composes
/// `(<i64>::SHAPE, <i64>::as_wide)` through [`extract_atom`]. The
/// axis identity rides the `<i64>` type parameter through ONE trait
/// dispatch; a regression that silently swapped the axis (an
/// int-axis extractor accidentally routing through
/// `ExpectedKwargShape::Number` / `Sexp::as_float`) cannot survive
/// the trait dispatch — the associated const's type + the impl's
/// return type both pin the axis at rustc time.
pub fn extract_int(kw: &Kwargs<'_>, key: &str) -> Result<i64> {
    <i64 as WideNumeric>::extract_kwarg(kw, key)
}

/// `Option` sibling of [`extract_int`] — one-line delegate to
/// `<i64 as WideNumeric>::extract_optional_kwarg`.
pub fn extract_optional_int(kw: &Kwargs<'_>, key: &str) -> Result<Option<i64>> {
    <i64 as WideNumeric>::extract_optional_kwarg(kw, key)
}

/// Float-axis sibling of [`extract_int`] — one-line delegate to
/// `<f64 as WideNumeric>::extract_kwarg`, whose default composes
/// `(<f64>::SHAPE, <f64>::as_wide)` through [`extract_atom`].
pub fn extract_float(kw: &Kwargs<'_>, key: &str) -> Result<f64> {
    <f64 as WideNumeric>::extract_kwarg(kw, key)
}

/// `Option` sibling of [`extract_float`] — one-line delegate to
/// `<f64 as WideNumeric>::extract_optional_kwarg`.
pub fn extract_optional_float(kw: &Kwargs<'_>, key: &str) -> Result<Option<f64>> {
    <f64 as WideNumeric>::extract_optional_kwarg(kw, key)
}

// ── The narrowing axis: a wide reader value → the field's Rust width ──
//
// `extract_int` returns `i64` and `extract_float` returns `f64` — the
// widest thing the reader can hand back on each axis. A field declared
// `u32` / `i32` / `usize` / `f32` is NARROWER, and the projection from
// the reader's width to the field's is a partial function: `70000` has
// no `u16`, `-1` has no `u32`, `1e300` has no `f32`.
//
// `#[derive(TataraDomain)]` used to close that gap with a raw Rust `as`
// cast appended to the extractor call — `extract_int(&kw, "port")? as
// u32`. `as` is TOTAL by truncating: it wraps, sign-flips, or saturates
// to `inf` and reports nothing. So an author who wrote a number the
// field could not hold got a DIFFERENT number in the struct, silently,
// with a green build and a green parse. That is precisely the class the
// typed-entry gate exists to reject, leaking through the one hole the
// gate did not cover.
//
// The fix is one typed primitive, here, rather than a check at each of
// the derive's four numeric arms — same lift as the `extract_via_serde`
// family below, and for the same reason: a hand-written `TataraDomain`
// impl and a derived one must take the identical error path, and the
// next upgrade (a span, a suggested-value hint) has to land in ONE
// place.
//
// The target width rides the TYPE, not an argument. `NarrowNumeric`'s
// associated `WIDTH` const means the derive emits
// `extract_int_narrowed::<u32>(&kw, key)?` and cannot mislabel the
// diagnostic, because it never names the width at all — the impl does.
// A future `classify` arm for a width with no impl is a compile error
// at the consumer, not a mislabeled runtime message.

/// A narrower numeric type reachable from the reader's wide `Wide`
/// value — `i64` on the int axis, `f64` on the float axis.
///
/// Implemented for exactly the nine widths `#[derive(TataraDomain)]`
/// recognises, which is what makes [`NumericWidth`] a genuinely closed
/// set: the enum's variants and this trait's impls are the same list,
/// generated from the same macro invocation below.
pub trait NarrowNumeric<Wide>: Sized + Copy {
    /// This type's identity in the typed diagnostic — the value that
    /// rides [`LispError::KwargOutOfRange`]'s `target` slot.
    const WIDTH: NumericWidth;

    /// The partial projection. `None` means "this wide value has no
    /// representation at this width" and becomes a typed rejection;
    /// it never means "here is a nearby value instead".
    fn narrow(wide: Wide) -> Option<Self>;
}

/// Emit the `NarrowNumeric<i64>` impls for the integer widths. Each is
/// `TryFrom<i64>` verbatim — the std conversion is already the exact
/// partial function we want (rejects too-large AND negative-into-
/// unsigned), so the impl delegates rather than re-deriving bounds
/// arithmetic that could disagree with std.
macro_rules! impl_narrow_int {
    ($($ty:ty => $width:ident),+ $(,)?) => {$(
        impl NarrowNumeric<i64> for $ty {
            const WIDTH: NumericWidth = NumericWidth::$width;
            fn narrow(wide: i64) -> Option<Self> {
                <$ty as ::core::convert::TryFrom<i64>>::try_from(wide).ok()
            }
        }
    )+};
}

impl_narrow_int! {
    i8 => I8,
    i16 => I16,
    i32 => I32,
    i64 => I64,
    u8 => U8,
    u16 => U16,
    u32 => U32,
    u64 => U64,
    usize => Usize,
    isize => Isize,
}

impl NarrowNumeric<f64> for f64 {
    const WIDTH: NumericWidth = NumericWidth::F64;
    fn narrow(wide: f64) -> Option<Self> {
        Some(wide)
    }
}

impl NarrowNumeric<f64> for f32 {
    const WIDTH: NumericWidth = NumericWidth::F32;
    /// Rejects exactly one thing: a FINITE `f64` whose magnitude
    /// overflows to `inf` at `f32`. Precision loss inside the range is
    /// accepted — an `f32` field asked for `f32` precision, and
    /// rejecting `0.1` would make the type unusable. An input that was
    /// already `inf` / `NaN` passes through unchanged, because `as`
    /// preserved it faithfully; the corruption case is only the finite
    /// value that becomes infinite.
    fn narrow(wide: f64) -> Option<Self> {
        #[allow(clippy::cast_possible_truncation)]
        let narrowed = wide as f32;
        if narrowed.is_finite() || !wide.is_finite() {
            Some(narrowed)
        } else {
            None
        }
    }
}

/// The reader's widest type on each numeric axis — `i64` on the int
/// column, `f64` on the float column — carrying its typed lift into
/// [`NumericLiteral`]'s matching variant.
///
/// Companion to [`NarrowNumeric`] on the OTHER end of the narrowing
/// pipeline: `NarrowNumeric<Wide>` projects a wide value INTO a
/// narrower Rust field type; `WideNumeric` lifts the SAME wide value
/// out again into the typed diagnostic carrier the rejection quotes.
/// The two traits sandwich the four `extract_*_narrowed` extractors:
/// EVERY narrowing site reads a wide (`extract_int` / `extract_float`),
/// tries the narrow projection (`T::narrow`), and on failure lifts the
/// wide back into a `NumericLiteral` so the operator-facing diagnostic
/// echoes the author's own literal (never the width-cast peer).
///
/// The set is closed by CONSTRUCTION: the trait is `pub` but its impls
/// live in this module and cover the TWO wide axes the reader can
/// produce (`Sexp::as_int` → `i64`, `Sexp::as_float` → `f64`). A
/// hypothetical third wide axis is not reachable until the reader
/// grows a new atom projection AND [`NumericLiteral`] grows the
/// matching variant AND this trait gains the impl — at which point the
/// substrate's narrowing pipeline binds the new axis mechanically,
/// with no per-extractor edit.
///
/// Pre-lift the wide-into-literal wrap lived at FOUR call sites — each
/// of `extract_int_narrowed` / `extract_optional_int_narrowed` /
/// `extract_float_narrowed` / `extract_optional_float_narrowed` spelled
/// `NumericLiteral::<Variant>(wide)` byte-for-byte, threading the wide
/// axis identity through a per-site literal constructor rather than
/// through a typed method dispatched on the wide type itself. Post-lift
/// the wrap lives in ONE `WideNumeric::as_literal` method dispatched
/// through the `narrow_or_range_err` primitive; the four extractors
/// name the axis exactly once (through the `W` type parameter on their
/// `NarrowNumeric<W>` bound) and never spell the `NumericLiteral`
/// variant at all. A regression that silently re-labeled one variant
/// (`NumericLiteral::Int` → `NumericLiteral::Float` on the int axis's
/// wrap) cannot survive the trait dispatch — the impl's return type is
/// pinned by `NumericLiteral` itself, and the width identity riding
/// [`NarrowNumeric::WIDTH`] cross-checks with the axis-typed literal at
/// the operator-facing display boundary (`NumericWidth::U16` paired
/// with `NumericLiteral::Int(70_000)` is coherent; `NumericLiteral::
/// Float(70_000.0)` would be a shape drift the trait's per-axis impls
/// forbid at rustc time).
///
/// Theory anchor: THEORY.md §II.1 invariant 3 (typed exit) — the
/// wide-into-literal wrap IS the typed-exit projection at the numeric
/// axis's rejection boundary; naming it as a trait method makes the
/// axis identity load-bearing data rather than a per-site constructor
/// call. THEORY.md §VI.1 (generation over composition) — the wrap
/// recurred at FOUR sites (well past the three-times-rule trigger) and
/// is lifted to one owner; adding a fifth extractor shape (e.g. a
/// `Vec<T>` numeric variant, once the derive's `Kind::VecInt` /
/// `Kind::VecFloat` arms exist) picks up the trait dispatch
/// mechanically with no new per-site literal-constructor call.
pub trait WideNumeric: Copy + for<'a> AtomKwarg<'a> {
    /// This wide value's typed lift into [`NumericLiteral`] — the
    /// per-axis variant the operator-facing rejection quotes. This is
    /// the ONE numeric-specific per-axis primitive that keeps
    /// [`WideNumeric`] as an extension trait over the shared
    /// [`AtomKwarg`] atom-family contract; every other per-axis
    /// primitive ([`AtomKwarg::SHAPE`], [`AtomKwarg::project`], and
    /// the composed [`AtomKwarg::extract_kwarg`] /
    /// [`AtomKwarg::extract_optional_kwarg`] defaults) rides the
    /// supertrait's contract, on the SAME footing the non-numeric
    /// atom axes (`&str`, `bool`) already close.
    fn as_literal(self) -> NumericLiteral;

    /// Axis-typed [`ExpectedKwargShape`] rejection label — one-line
    /// delegate to the shared [`AtomKwarg`] supertrait's [`AtomKwarg::
    /// SHAPE`] associated const, so a caller reaching for the wide
    /// axis's label through the `<i64 as WideNumeric>::SHAPE` /
    /// `<f64 as WideNumeric>::SHAPE` path lands on the SAME
    /// axis-typed constant the shared atom-family contract already
    /// owns. Provided as a default that hard-codes the equivalence to
    /// the supertrait const; per-axis [`WideNumeric`] impls (`i64` /
    /// `f64`) no longer restate SHAPE — the associated const lives on
    /// the [`AtomKwarg`] impls that carry every atom axis's
    /// per-axis rejection label.
    const SHAPE: ExpectedKwargShape = <Self as AtomKwarg<'static>>::SHAPE;

    /// Per-[`Sexp`] atom projection — one-line delegate to the
    /// [`AtomKwarg`] supertrait's [`AtomKwarg::project`] method, so a
    /// caller reaching for the wide axis's projection through the
    /// `<i64 as WideNumeric>::as_wide` / `<f64 as WideNumeric>::
    /// as_wide` path lands on the SAME per-atom `Option<Self>` the
    /// shared atom-family contract already owns. Provided as a
    /// default that hard-codes the equivalence to the supertrait
    /// method; per-axis [`WideNumeric`] impls (`i64` / `f64`) no
    /// longer restate the projection — it lives on the [`AtomKwarg`]
    /// impls that carry every atom axis's per-atom projection.
    ///
    /// The PER-ELEMENT primitive [`extract_narrowed_list<W, T>`]
    /// composes through the atom-family shape gate
    /// [`AtomKwarg::project_at`] — the wide-into-item projection
    /// lifted out of `extract_int` / `extract_float` so a per-item
    /// narrowing walk (Vec of narrowed numerics) reaches its axis's
    /// atom projection uniformly at both axes, through the SAME
    /// atom-family shape-gate composition the non-numeric per-item
    /// atom-projection extractors (`extract_string_list`) already
    /// bind. This method stays as the direct-projection convenience
    /// for callers that want the raw `Option<Self>` (a hypothetical
    /// tolerant reader that keeps walking past a per-item shape
    /// mismatch); the rejecting shape-gate composition lives on the
    /// supertrait's [`AtomKwarg::project_at`] default rather than
    /// restated per callsite.
    fn as_wide(sexp: &Sexp) -> Option<Self> {
        <Self as AtomKwarg<'_>>::project(sexp)
    }

    /// Wide-axis kwarg extractor — one-line delegate to the
    /// [`AtomKwarg`] supertrait's [`AtomKwarg::extract_kwarg`]
    /// method. Pre-supertrait-lift this method restated the
    /// `extract_atom(kw, key, Self::SHAPE, Self::as_wide)`
    /// composition inline, duplicating the atom-family skeleton the
    /// non-numeric [`AtomKwarg`] impls (`&str`, `bool`) already
    /// carry. Post-supertrait-lift the composition lives on ONE
    /// owner — [`AtomKwarg::extract_kwarg`] — and this method is a
    /// per-axis convenience that keeps `<i64 as WideNumeric>::
    /// extract_kwarg` / `<f64 as WideNumeric>::extract_kwarg` as
    /// stable authoring surfaces while routing every caller through
    /// the SAME shared composition the atom-family axes bind.
    fn extract_kwarg(kw: &Kwargs<'_>, key: &str) -> Result<Self> {
        <Self as AtomKwarg<'_>>::extract_kwarg(kw, key)
    }

    /// The `Option` sibling of [`WideNumeric::extract_kwarg`] —
    /// one-line delegate to the [`AtomKwarg`] supertrait's
    /// [`AtomKwarg::extract_optional_kwarg`] method on the same
    /// footing as [`Self::extract_kwarg`]. Pre-supertrait-lift this
    /// restated the `extract_optional_atom(kw, key, Self::SHAPE,
    /// Self::as_wide)` composition; post-lift the composition lives
    /// on ONE owner — [`AtomKwarg::extract_optional_kwarg`] — and
    /// this method routes through the SAME shared composition the
    /// atom-family axes bind.
    fn extract_optional_kwarg(kw: &Kwargs<'_>, key: &str) -> Result<Option<Self>> {
        <Self as AtomKwarg<'_>>::extract_optional_kwarg(kw, key)
    }
}

// Numeric-axis [`WideNumeric`] impls — post-supertrait-lift these
// impls carry ONLY the numeric-specific [`as_literal`] method; the
// atom-family per-axis primitives (SHAPE + project + the composed
// extract_kwarg / extract_optional_kwarg defaults) ride the
// [`AtomKwarg`] supertrait's impls above, on the SAME footing the
// non-numeric atom axes (`&str`, `bool`) already close. A future
// third wide axis lands as ONE `impl<'a> AtomKwarg<'a> for <NewWide>`
// closing the atom-family shape + projection + composed extractor
// contract, plus ONE `impl WideNumeric for <NewWide>` adding the
// numeric-only wide-into-literal lift.
impl WideNumeric for i64 {
    fn as_literal(self) -> NumericLiteral {
        NumericLiteral::Int(self)
    }
}

impl WideNumeric for f64 {
    fn as_literal(self) -> NumericLiteral {
        NumericLiteral::Float(self)
    }
}

/// The scalar-kwarg narrowing rejection wrapper — `T::narrow(wide)`
/// gated at `KwargPath::Named(key)`. FOUR sites used to inline this
/// three-line shape (`T::narrow(wide).ok_or_else(|| range_err(key,
/// T::WIDTH, NumericLiteral::<Variant>(wide)))`), each spelling its
/// axis's `NumericLiteral` variant constructor by hand. Post-lift the
/// four extractors bind through this wrapper; the axis identity
/// rides the `W` type parameter (dispatched to the correct
/// `NumericLiteral` variant by `WideNumeric`), and no extractor names
/// `NumericLiteral::Int` or `NumericLiteral::Float` at all.
///
/// One-line delegate to the KwargPath-parameterized
/// [`narrow_or_range_mismatch`] primitive, composing `kwarg_form(key)`
/// as the failure locus. The `T: NarrowNumeric<W>` / `W: WideNumeric`
/// bound pair and the axis-coherence property they buy live at that
/// primitive; this wrapper contributes ONLY the scalar `KwargPath`
/// choice. Its per-item sibling [`narrow_or_range_err_at`] composes
/// `kwarg_item_form(key, idx)` against the SAME primitive, so the two
/// wrappers differ in exactly one argument.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// four-times rule decisively crossed). THEORY.md §II.1 invariant 3
/// (typed exit — the rejection boundary lives at ONE primitive whose
/// axis identity is the type-parameter, not a per-site literal
/// constructor).
fn narrow_or_range_err<W, T>(key: &str, wide: W) -> Result<T>
where
    W: WideNumeric,
    T: NarrowNumeric<W>,
{
    narrow_or_range_mismatch(kwarg_form(key), wide)
}

/// The KwargPath-parameterized narrowing gate — `T::narrow(wide)`
/// followed by [`range_mismatch`] wrapping the wide value in its typed
/// [`NumericLiteral`] carrier via [`WideNumeric::as_literal`], at a
/// caller-supplied [`KwargPath`].
///
/// Path-parameterized peer of [`type_mismatch`]'s sibling on the
/// numeric-narrowing axis, one abstraction level above
/// [`range_mismatch`]: [`range_mismatch`] owns the
/// [`LispError::KwargOutOfRange`] STRUCT-LITERAL construction; this
/// primitive owns the `T::narrow(wide).ok_or_else(|| …)` NARROWING-GATE
/// composition that pairs the partial narrowing function with that
/// rejection. Pre-lift the composition lived inline at TWO sites in
/// this module — [`narrow_or_range_err`] (the `KwargPath::Named(key)`
/// scalar-kwarg path, consumed by [`extract_narrowed`]) and
/// [`narrow_or_range_err_at`] (the `KwargPath::Item { key, idx }`
/// per-item path, consumed by [`extract_narrowed_list`]) — each
/// restating the same `T::narrow` / `T::WIDTH` / `as_literal` triple
/// against a different path builder. Post-lift the two are one-line
/// delegates that differ ONLY in the [`KwargPath`] they hand this
/// primitive, and the gate composition lives at ONE named site.
///
/// Both bounds ride the same `W` type parameter — `T: NarrowNumeric<W>`
/// pins the narrowing partial function and `W: WideNumeric` pins the
/// wide-into-literal wrap on the SAME wide type — so a caller cannot
/// pass an axis's wide value into a narrowing gate typed for the other
/// axis, and the diagnostic's `(target, value)` pair is coherent by
/// construction (`NumericWidth::U16` alongside `NumericLiteral::Int(_)`,
/// never `NumericLiteral::Float(_)`). A future third wide axis lands as
/// ONE `impl WideNumeric for <NewWide>` plus ONE `impl
/// NarrowNumeric<NewWide> for <NewNarrow>` per new narrow width; this
/// primitive picks up the axis mechanically with no signature change.
///
/// Public visibility mirrors [`range_mismatch`] / [`type_mismatch`] on
/// the same rejection-surface layer, so a hand-written [`TataraDomain`]
/// impl narrowing at a path shape the substrate's own extractors do not
/// walk (a `KwargPath::Slot(_)` positional narrowing gate, or a future
/// fourth path shape) has ONE substrate entry to route through rather
/// than re-inlining the `T::narrow` composition at its own call site.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// narrowing-gate composition lived at two sites, crossing the ≥2
/// duplication threshold on the substrate's narrowing surface.
/// THEORY.md §II.1 invariant 3 (typed exit) — the narrowing boundary
/// lives at ONE primitive whose axis identity is the type parameter and
/// whose failure locus is the [`KwargPath`] argument, not a per-site
/// path-builder choice baked into the gate.
pub fn narrow_or_range_mismatch<W, T>(form: KwargPath, wide: W) -> Result<T>
where
    W: WideNumeric,
    T: NarrowNumeric<W>,
{
    T::narrow(wide).ok_or_else(|| range_mismatch(form, T::WIDTH, wide.as_literal()))
}

/// The GENERIC narrowing kwarg extractor — the axis identity rides
/// the `W: WideNumeric` type parameter, so `extract_narrowed::<i64, T>`
/// dispatches through [`WideNumeric::extract_kwarg`] to [`extract_int`]
/// and `extract_narrowed::<f64, T>` dispatches to [`extract_float`],
/// both threaded through the SAME [`narrow_or_range_err`] rejection
/// primitive. Pre-lift the FOUR public `extract_*_narrowed` extractors
/// each spelled `extract_int` / `extract_optional_int` / `extract_
/// float` / `extract_optional_float` inline; post-lift the four are
/// one-line delegates to this pair and the axis identity is a trait
/// dispatch rather than a hand-written call. A future third wide axis
/// lands as ONE `impl WideNumeric for <NewWide>` (adding
/// `as_literal`, `extract_kwarg`, and `extract_optional_kwarg` for
/// that axis) plus ONE `impl NarrowNumeric<NewWide> for <NewNarrow>`
/// per new narrow width; the two generic primitives bind mechanically
/// with no new public wrapper.
pub fn extract_narrowed<W, T>(kw: &Kwargs<'_>, key: &str) -> Result<T>
where
    W: WideNumeric,
    T: NarrowNumeric<W>,
{
    narrow_or_range_err(key, <W as WideNumeric>::extract_kwarg(kw, key)?)
}

/// `Option` sibling of [`extract_narrowed`]. An ABSENT kwarg stays
/// `None`; a PRESENT but out-of-range one is a rejection, never a
/// `None` — silently dropping a value the author wrote would be the
/// same corruption in a different costume.
///
/// Delegates to [`optional_from_required`] with [`extract_narrowed::<W, T>`]
/// as its required extractor — the SIXTH consumer of the primitive
/// after the four list-family peers ([`extract_optional_string_list`] /
/// [`extract_optional_bool_list`] / [`extract_optional_narrowed_list`] /
/// [`extract_optional_vec_via_serde`]) and the scalar universal-serde
/// peer [`extract_optional_via_serde`]. Pre-lift this extractor
/// composed `<W as WideNumeric>::extract_optional_kwarg` (the atom-family
/// present-vs-absent bifurcation on the WIDE side of the narrowing)
/// with a post-hoc [`narrow_or_range_err`] on the `Some(_)` arm — the
/// LAST scalar `extract_optional_*` extractor whose present-vs-absent
/// gate did NOT route through [`optional_from_required`]. Post-lift the
/// substrate primitive owns the bifurcation and every scalar-narrowed
/// caller inherits its "absent → `Ok(None)`, present → delegate through
/// required peer" contract mechanically; a future diagnostic promotion
/// on the present-vs-absent gate (a probe, a metric, a span) lands at
/// ONE owner and flows to the scalar-narrowed peer here without a
/// per-caller edit — the same lift the list-family
/// [`extract_optional_narrowed_list`] cousin already took on its
/// present-vs-absent axis.
///
/// The refactor is semantic-preserving on every axis-mode ×
/// input-shape input: an ABSENT kwarg (Ok(None), no required-peer
/// invocation), a PRESENT wrong-shape kwarg (the SAME
/// [`LispError::TypeMismatch`] variant [`extract_narrowed`] emits at
/// the WideNumeric shape gate), a PRESENT out-of-range kwarg (the SAME
/// [`LispError::KwargOutOfRange`] variant [`extract_narrowed`] emits
/// at the [`narrow_or_range_err`] narrowing gate), and a PRESENT
/// in-range kwarg (`Ok(Some(_))` wrapping the same narrowed value).
/// The two paths bifurcate on WHICH atom-family present-vs-absent
/// gate owns the short-circuit ([`extract_optional_atom`] on the
/// WideNumeric side pre-lift, [`optional_from_required`]'s
/// [`kw.contains_key`] gate post-lift) — both check the SAME key,
/// both short-circuit at the SAME moment, only the substrate-primitive
/// identity differs.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// present-vs-absent + delegate-to-required posture now recurs at
/// SEVEN peer sites all routing through the SAME
/// [`optional_from_required`] substrate primitive; the three-times-rule
/// trigger keeps holding as the scalar-narrowed axis lands as the
/// sixth consumer alongside the atom-family scalar peer
/// [`extract_optional_atom`] as the seventh). THEORY.md §II.1
/// invariant 2 (free middle — the optional-vs-required peer distinction
/// lives at ONE substrate primitive, not restated at every optional-
/// peer callsite; a future diagnostic promotion on the present-vs-
/// absent gate flows to the scalar-narrowed peer here mechanically).
/// THEORY.md §V.1 (knowable platform — the rejection on `Option<T>`
/// on the scalar-narrowed axis now surfaces through the SAME
/// [`optional_from_required`] primitive every list-family + scalar-
/// serde + scalar-atom peer already binds to, so a future audit-trail
/// metric jointly labeled by "which optional peer fired" and "was the
/// kwarg present" covers the scalar-narrowed peer without a per-peer
/// bifurcation re-implementation).
pub fn extract_optional_narrowed<W, T>(kw: &Kwargs<'_>, key: &str) -> Result<Option<T>>
where
    W: WideNumeric,
    T: NarrowNumeric<W>,
{
    optional_from_required(kw, key, extract_narrowed::<W, T>)
}

/// Required integer kwarg projected into the field's own width —
/// one-line delegate to [`extract_narrowed`] at the `W = i64` axis.
/// The narrowing replacement for `extract_int(&kw, key)? as T`.
pub fn extract_int_narrowed<T: NarrowNumeric<i64>>(kw: &Kwargs<'_>, key: &str) -> Result<T> {
    extract_narrowed::<i64, T>(kw, key)
}

/// `Option` sibling of [`extract_int_narrowed`] — one-line delegate
/// to [`extract_optional_narrowed`] at the `W = i64` axis.
pub fn extract_optional_int_narrowed<T: NarrowNumeric<i64>>(
    kw: &Kwargs<'_>,
    key: &str,
) -> Result<Option<T>> {
    extract_optional_narrowed::<i64, T>(kw, key)
}

/// Float-axis sibling of [`extract_int_narrowed`] — one-line delegate
/// to [`extract_narrowed`] at the `W = f64` axis. The narrowing
/// replacement for `extract_float(&kw, key)? as T`.
pub fn extract_float_narrowed<T: NarrowNumeric<f64>>(kw: &Kwargs<'_>, key: &str) -> Result<T> {
    extract_narrowed::<f64, T>(kw, key)
}

/// `Option` sibling of [`extract_float_narrowed`] — one-line delegate
/// to [`extract_optional_narrowed`] at the `W = f64` axis.
pub fn extract_optional_float_narrowed<T: NarrowNumeric<f64>>(
    kw: &Kwargs<'_>,
    key: &str,
) -> Result<Option<T>> {
    extract_optional_narrowed::<f64, T>(kw, key)
}

/// Item-indexed narrowing rejection primitive — sibling of
/// [`narrow_or_range_err`] for the per-element path a list-typed
/// narrowing extractor walks. Threads the same `W: WideNumeric` +
/// `T: NarrowNumeric<W>` pair the scalar peer binds, plus the failing
/// item index the [`extract_list`] skeleton hands each per-element
/// projection, and wraps the wide-into-literal projection through
/// [`range_err_at`]'s `KwargPath::Item { key, idx }` typed path so a
/// per-item narrowing failure inside `:ports (list 80 70000)` renders
/// `LispError::KwargOutOfRange { form: Item { key: "ports", idx: 1 },
/// target: U16, value: Int(70_000) }` — pattern-matchable by the same
/// `KwargPath::Item` variant every peer per-item rejection surface
/// (`type_err_at`, `extract_vec_via_serde`'s per-item bridge) carries.
///
/// One-line delegate to the KwargPath-parameterized
/// [`narrow_or_range_mismatch`] primitive, composing
/// `kwarg_item_form(key, idx)` as the failure locus. The `W` type
/// parameter rides both bounds at that primitive so the narrowing
/// partial function and the wide-into-literal wrap are pinned on the
/// SAME wide type — a caller cannot pass an axis's wide value into a
/// narrowing gate typed for the other axis. Peer to
/// [`narrow_or_range_err`] on the same wide-into-literal projection
/// axis; post-lift the only shape delta between the two wrappers is
/// the [`KwargPath`] argument each hands the shared primitive
/// (`kwarg_form(key)` vs. `kwarg_item_form(key, idx)`).
fn narrow_or_range_err_at<W, T>(key: &str, idx: usize, wide: W) -> Result<T>
where
    W: WideNumeric,
    T: NarrowNumeric<W>,
{
    narrow_or_range_mismatch(kwarg_item_form(key, idx), wide)
}

/// The GENERIC narrowing list-kwarg extractor — the list-family peer of
/// [`extract_narrowed`] on the per-item path a `Vec<T>` numeric-narrowed
/// kwarg walks. The axis identity rides the `W: WideNumeric` type
/// parameter, so `extract_narrowed_list::<i64, T>` dispatches through
/// the atom-family per-item shape gate [`AtomKwarg::project_at`]
/// (composing `<i64>::SHAPE = Int` + `<i64>::project = Sexp::as_int`)
/// plus [`NarrowNumeric::narrow`] (`TryFrom<i64>::try_from`) and
/// `extract_narrowed_list::<f64, T>` dispatches through
/// [`AtomKwarg::project_at`] (composing `<f64>::SHAPE = Number` +
/// `<f64>::project = Sexp::as_float`) plus its `T::narrow` peer,
/// both threaded through the SAME [`extract_list`] outer-shape
/// skeleton and the SAME per-item rejection primitives
/// ([`AtomKwarg::project_at`] on a per-item shape mismatch,
/// [`narrow_or_range_err_at`] on a per-item range mismatch). Pre-lift the derive routed `Vec<u16>` /
/// `Vec<i32>` / `Vec<f32>` / etc. through [`extract_vec_via_serde`] —
/// the universal serde bridge — so a per-item narrowing failure on
/// `:ports (list 80 70000)` surfaced as a mystery `KwargDeserialize
/// { message: "invalid value: integer ...", .. }` diagnostic keyed
/// off a substring rather than as the typed `KwargOutOfRange { target:
/// U16, value: Int(70_000), .. }` its scalar peer already emits;
/// post-lift the per-item narrowing gate matches the scalar gate's
/// rejection shape byte-for-byte modulo the `KwargPath::Item { key,
/// idx }` → `KwargPath::Named(key)` per-path shift, and the two
/// gates share ONE rejection vocabulary.
///
/// The outer-shape label routes through the [`AtomKwarg::LIST_SHAPE`]
/// trait const on the wide axis (`<W as AtomKwarg<'_>>::LIST_SHAPE`,
/// reached through the `W: WideNumeric: for<'a> AtomKwarg<'a>`
/// supertrait bound) rather than baked as an inline
/// `ExpectedKwargShape` literal. Both wide-numeric axes override
/// the trait default: `<i64>::LIST_SHAPE = ListOfInts` and
/// `<f64>::LIST_SHAPE = ListOfNumbers` — the element-typed
/// refinement labels this extractor emits on a scalar-typed kwarg
/// through the per-axis trait dispatch. Every narrow width the
/// extractor is instantiated at (`u16` / `i32` / `usize` / `f32` /
/// …) inherits the sharpened outer-shape label mechanically from
/// its wide axis's ONE trait override — no per-extractor edit, no
/// per-width bookkeeping. A `:ports 80` kwarg into `Vec<u16>` now
/// rejects as `expected list of ints, got int` rather than the
/// pre-lift ambiguous `expected list, got int`; a
/// `:scales 1.0` kwarg into `Vec<f32>` as `expected list of
/// numbers, got number` (the `"numbers"` naming mirrors the
/// scalar-axis Number vs Float posture — the per-item gate
/// accepts BOTH float and int atoms, so the outer-shape label
/// names the union). The per-item shape gate keeps
/// [`WideNumeric::SHAPE`]'s axis-typed rejection label
/// (`ExpectedKwargShape::Int` on `<i64>`, `ExpectedKwargShape::Number`
/// on `<f64>`) — the same label the scalar-kwarg peer's shape gate
/// emits — so a per-item shape mismatch on the int axis reads
/// `expected int, got string`, not the wider `expected list of ints,
/// got string`.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 (typed entry — a
/// per-item narrowing failure on a list-typed kwarg IS a typed-entry
/// gate the numeric-vec surface used to leak past through the serde
/// bridge). THEORY.md §VI.1 (generation over composition — the
/// narrowing-list shape recurs at the (two axes × Vec<T>) product of
/// the substrate's numeric-vec surface; lifting it to ONE primitive
/// closes both axes at rustc time). THEORY.md §V.1 (knowable
/// platform — the per-item narrowing rejection now surfaces the same
/// pattern-matchable [`LispError::KwargOutOfRange`] variant every
/// authoring surface (LSP, `tatara-check`, REPL) already binds to for
/// the scalar peer, so `NumericWidth::U16` histogram bucketing over
/// per-item failures binds mechanically).
pub fn extract_narrowed_list<W, T>(kw: &Kwargs<'_>, key: &str) -> Result<Vec<T>>
where
    W: WideNumeric,
    T: NarrowNumeric<W>,
{
    extract_list(kw, key, <W as AtomKwarg<'_>>::LIST_SHAPE, |idx, s| {
        let wide = <W as AtomKwarg<'_>>::project_at(key, idx, s)?;
        narrow_or_range_err_at::<W, T>(key, idx, wide)
    })
}

/// `Option<Vec<T>>` numeric-narrowed list-typed kwarg — the
/// WideNumeric/NarrowNumeric-axis peer of [`extract_optional_bool_list`]
/// / [`extract_optional_string_list`] on the non-numeric atom-family
/// list surface, and the present-vs-absent bifurcated peer of
/// [`extract_narrowed_list<W, T>`] on the same numeric-narrowing axis.
/// Distinguishes an ABSENT kwarg (`Ok(None)`) from a PRESENT empty
/// list (`Ok(Some(Vec::new()))`) — the required peer
/// [`extract_narrowed_list`] collapses both cases to `Ok(Vec::new())`,
/// which fits `Vec<T>` fields (where "no items" and "no kwarg" are
/// semantically the same) but loses the present-vs-absent distinction
/// an `Option<Vec<T>>` field needs (where `None` and `Some(vec![])`
/// are DISTINCT operator intents — "the operator did not name this
/// kwarg" vs. "the operator explicitly bound the kwarg to an empty
/// list").
///
/// A present-but-non-list kwarg (`:ports 80`) rejects with the SAME
/// typed [`LispError::TypeMismatch`] variant the required peer
/// [`extract_narrowed_list`] emits (`expected list of ints, got int`
/// through the `<i64>::LIST_SHAPE = ListOfInts` per-axis trait-const
/// override, or `expected list of numbers, got X` on the wide-float
/// axis peer); a per-item non-numeric element inside a present list
/// (`:ports (list 80 "yes" 443)`) rejects with the SAME
/// [`LispError::TypeMismatch { form: Item { key, idx }, expected:
/// Int, got: String }`] variant its per-item peer emits through the
/// shared [`AtomKwarg::project_at`] atom-family shape gate; a per-
/// item out-of-range narrowing failure inside a present list
/// (`:ports (list 80 70000)` into `Option<Vec<u16>>`) rejects with
/// the SAME [`LispError::KwargOutOfRange { form: Item { key, idx },
/// target, value }`] variant its per-item peer emits through the
/// shared [`narrow_or_range_err_at`] narrowing gate. The two peers
/// ([`extract_narrowed_list`] and this one) delegate to the SAME
/// per-item gate — only the outer present-vs-absent axis's return
/// shape differs (`Vec<T>` vs. `Option<Vec<T>>`).
///
/// Sibling posture to [`extract_optional_bool_list`] +
/// [`extract_optional_string_list`] on the atom-family non-numeric
/// list surface: all three distinguish `None` (absent) from
/// `Some(_)` (present-and-decoded) at the `Option` layer via the
/// same `kw.contains_key(key)` short-circuit, and all three delegate
/// their present-branch decode to their required peer
/// ([`extract_narrowed_list`] / [`extract_bool_list`] /
/// [`extract_string_list`]). Peer to [`extract_optional_via_serde`]
/// on the universal-serde-fallthrough axis: both share the
/// `kw.contains_key(key)` present-vs-absent bifurcation; the
/// differences are that this extractor rides the typed WideNumeric
/// per-item shape gate PLUS the NarrowNumeric per-item narrowing
/// gate rather than the serde bridge's per-item
/// `from_value_with_path` decode, so a per-item shape mismatch
/// surfaces as a pattern-matchable [`LispError::TypeMismatch`]
/// variant and a per-item narrowing failure surfaces as a pattern-
/// matchable [`LispError::KwargOutOfRange`] variant, rather than as
/// [`LispError::KwargDeserialize`] substrings.
///
/// Pre-lift the derive routed `Option<Vec<u16>>` / `Option<Vec<f32>>`
/// / etc. through the universal-serde bridge
/// ([`extract_optional_via_serde`] via
/// [`crate::domain::from_value_with_path`]) — [`classify_option`]
/// in [`tatara_lisp_derive`] had no arm for `Option<Vec<T>>` on the
/// numeric axes and fell through to `Kind::OptionalDeserialize`,
/// matching the pre-lift posture that [`Option<Vec<String>>`] and
/// [`Option<Vec<bool>>`] shared before their per-axis routes landed.
/// So a per-item shape mismatch inside `Option<Vec<u16>>` surfaced
/// as a mystery
/// `LispError::KwargDeserialize { message: "invalid value: integer
/// 70000, expected u16 at path .1" }` substring rather than as the
/// typed
/// `LispError::KwargOutOfRange { form: Item { key, idx: 1 },
/// target: U16, value: Int(70_000) }` its required peer's per-item
/// narrowing gate already emits. Post-lift the two
/// `Option<Vec<T>>` / `Vec<T>` peers on the numeric-narrowing axis
/// share ONE rejection vocabulary — the same closure the bool-axis
/// [`extract_optional_bool_list`] and the string-axis
/// [`extract_optional_string_list`] peers already made on their own
/// pairs. Completes the Cartesian product across
/// {scalar, optional-scalar, vec, optional-vec} ×
/// {String, Bool, Int, Float} for the atom-family axes — every
/// scalar-typed field on the atom-family surface now composes with
/// its `Option` / `Vec` / `Option<Vec>` wrappers through the same
/// typed rejection vocabulary its scalar peer emits, and no
/// atom-family list-family axis-mode combination still falls
/// through the universal-serde bridge.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 (typed entry — a
/// per-item narrowing failure on an `Option<Vec<T>>` numeric kwarg
/// IS a typed-entry gate the optional-numeric-vec surface used to
/// leak past through the serde bridge). THEORY.md §VI.1 (generation
/// over composition — the present-vs-absent + delegate-to-required
/// posture now recurs at FOUR sites ([`extract_optional_string`] /
/// [`extract_optional_bool`] / [`extract_optional_string_list`] /
/// [`extract_optional_bool_list`] / this), all routing through the
/// SAME `kw.contains_key(key)` short-circuit paired with required-
/// peer delegation; the numeric-narrowing skeleton recurs at the
/// (two axes × Option<Vec<T>>) product of the substrate's optional-
/// numeric-vec surface, and lifting it to ONE primitive closes both
/// axes at rustc time). THEORY.md §V.1 (knowable platform — the
/// per-item narrowing rejection now surfaces the same pattern-
/// matchable [`LispError::KwargOutOfRange`] variant every authoring
/// surface (LSP, `tatara-check`, REPL) already binds to for the
/// required peer, so `NumericWidth::U16` histogram bucketing over
/// per-item failures binds mechanically for `Option<Vec<T>>` fields
/// too).
pub fn extract_optional_narrowed_list<W, T>(kw: &Kwargs<'_>, key: &str) -> Result<Option<Vec<T>>>
where
    W: WideNumeric,
    T: NarrowNumeric<W>,
{
    optional_from_required(kw, key, extract_narrowed_list::<W, T>)
}

/// `Vec<T>` integer-list field projected into the item's own width —
/// one-line delegate to [`extract_narrowed_list`] at the `W = i64`
/// axis. The list-family peer of [`extract_int_narrowed`] on the
/// per-item numeric-narrowing path. Absent kwarg returns
/// `Ok(Vec::new())` (the same posture [`extract_list`] gives every
/// list-typed kwarg — an absent list is the empty list, never an
/// error); a present list with a per-item out-of-range value rejects
/// with `LispError::KwargOutOfRange { form: KwargPath::Item { key,
/// idx }, .. }`.
pub fn extract_int_list_narrowed<T: NarrowNumeric<i64>>(
    kw: &Kwargs<'_>,
    key: &str,
) -> Result<Vec<T>> {
    extract_narrowed_list::<i64, T>(kw, key)
}

/// Float-axis sibling of [`extract_int_list_narrowed`] — one-line
/// delegate to [`extract_narrowed_list`] at the `W = f64` axis. The
/// list-family peer of [`extract_float_narrowed`] on the per-item
/// numeric-narrowing path. Same absent-list / present-list semantics
/// as [`extract_int_list_narrowed`]; the rejection shape on a
/// per-item lossy-to-inf overflow (`1.0e300 → f32`) is
/// `LispError::KwargOutOfRange { form: KwargPath::Item { key, idx },
/// target: NumericWidth::F32, value: NumericLiteral::Float(1.0e300),
/// .. }`.
pub fn extract_float_list_narrowed<T: NarrowNumeric<f64>>(
    kw: &Kwargs<'_>,
    key: &str,
) -> Result<Vec<T>> {
    extract_narrowed_list::<f64, T>(kw, key)
}

/// `Option<Vec<T>>` integer-list field projected into the item's own
/// width — one-line delegate to [`extract_optional_narrowed_list`]
/// at the `W = i64` axis. The list-family peer of
/// [`extract_optional_int_narrowed`] on the present-vs-absent
/// axis, and the numeric-narrowing peer of
/// [`extract_optional_bool_list`] on the optional-vec atom-family
/// surface. Absent kwarg returns `Ok(None)`; present empty list
/// returns `Ok(Some(Vec::new()))`; present list with a per-item
/// out-of-range value rejects with
/// `LispError::KwargOutOfRange { form: KwargPath::Item { key, idx },
/// target, value }` — the SAME rejection shape the required peer
/// [`extract_int_list_narrowed`] emits, only wrapped in the
/// `Option` layer for a present-vs-decoded return path.
pub fn extract_optional_int_list_narrowed<T: NarrowNumeric<i64>>(
    kw: &Kwargs<'_>,
    key: &str,
) -> Result<Option<Vec<T>>> {
    extract_optional_narrowed_list::<i64, T>(kw, key)
}

/// Float-axis sibling of [`extract_optional_int_list_narrowed`] —
/// one-line delegate to [`extract_optional_narrowed_list`] at the
/// `W = f64` axis. Same absent / present-empty / present-decoded
/// semantics as its int-axis peer; the rejection shape on a per-
/// item lossy-to-inf overflow (`1.0e300 → f32`) is
/// `LispError::KwargOutOfRange { form: KwargPath::Item { key, idx },
/// target: NumericWidth::F32, value: NumericLiteral::Float(1.0e300),
/// .. }` — the SAME rejection shape the required peer
/// [`extract_float_list_narrowed`] emits, only wrapped in the
/// `Option` layer.
pub fn extract_optional_float_list_narrowed<T: NarrowNumeric<f64>>(
    kw: &Kwargs<'_>,
    key: &str,
) -> Result<Option<Vec<T>>> {
    extract_optional_narrowed_list::<f64, T>(kw, key)
}

/// Required bool kwarg — one-line delegate to `<bool as
/// AtomKwarg<'_>>::extract_kwarg`, whose default composes
/// `(<bool>::SHAPE, <bool>::project)` through [`extract_atom`].
pub fn extract_bool(kw: &Kwargs<'_>, key: &str) -> Result<bool> {
    <bool as AtomKwarg<'_>>::extract_kwarg(kw, key)
}

/// `Option` sibling of [`extract_bool`] — one-line delegate to
/// `<bool as AtomKwarg<'_>>::extract_optional_kwarg`.
pub fn extract_optional_bool(kw: &Kwargs<'_>, key: &str) -> Result<Option<bool>> {
    <bool as AtomKwarg<'_>>::extract_optional_kwarg(kw, key)
}

/// `Vec<bool>` list-typed kwarg — the list-family peer of
/// [`extract_bool`] on the per-item atom-family shape gate. Composes
/// [`extract_list`]'s outer-shape skeleton (absent kwarg →
/// `Ok(Vec::new())`, present-but-not-a-list →
/// `type_err(key, ExpectedKwargShape::ListOfBools, v)` via the
/// `<bool>::LIST_SHAPE` per-axis trait-const override) with
/// [`AtomKwarg::project_at`]'s per-item shape gate on the `bool` axis
/// — a per-item non-bool element inside `:flags (list #t 5 #f)`
/// rejects as `LispError::TypeMismatch { form: Item { key: "flags",
/// idx: 1 }, expected: Bool, got: Int }`, the SAME axis-typed
/// diagnostic variant its scalar peer [`extract_bool`] emits at the
/// atom shape gate.
///
/// Pre-lift the derive routed `Vec<bool>` fields through
/// [`extract_vec_via_serde`] — the universal serde bridge — so a
/// per-item shape mismatch on `:flags (list #t "yes")` surfaced as a
/// mystery `LispError::KwargDeserialize { message: "invalid type:
/// string \"yes\", expected a boolean at path .1", .. }` diagnostic
/// keyed off a substring rather than as the typed
/// `TypeMismatch { form: Item { key: "flags", idx: 1 }, expected:
/// Bool, got: String }` its scalar peer [`extract_bool`] already
/// emits at the atom shape gate. Post-lift the per-item bool gate
/// matches the scalar bool gate's rejection shape byte-for-byte
/// modulo the `KwargPath::Item { key, idx }` → `KwargPath::Named(key)`
/// per-path shift, and the two gates share ONE rejection vocabulary.
///
/// The outer-shape label routes through the [`AtomKwarg::LIST_SHAPE`]
/// trait const on the `<bool>` axis, which post-lift carries the
/// axis-typed override [`ExpectedKwargShape::ListOfBools`] — the
/// element-typed refinement paired with the sibling
/// `<&str>::LIST_SHAPE = ListOfStrings` override on the atom-family
/// non-numeric list-of-atoms surface. The two overrides collapse the
/// pre-lift ambiguous `expected list, got X` bytes into the axis-
/// typed `expected list of bools, got X` / `expected list of strings,
/// got X` refinements at BOTH non-numeric atom-family list-family
/// sites through ONE per-axis trait-const dispatch each. The
/// wide-numeric peers [`extract_narrowed_list`] still inherit the
/// trait default [`ExpectedKwargShape::List`] pending their
/// per-axis `ListOfInts` / `ListOfNumbers` refinements, which land
/// as ONE new override per axis alongside these two with no per-
/// extractor edit.
///
/// The per-item shape gate keeps `<bool>::SHAPE`'s axis-typed
/// rejection label ([`ExpectedKwargShape::Bool`]) — the same label
/// the scalar-kwarg peer's shape gate emits — so a per-item shape
/// mismatch on the bool axis reads `expected bool, got string`, not
/// the wider `expected list of bools, got string`.
///
/// Sibling posture to [`extract_string_list`] on the non-numeric
/// atom-family list surface: both share ONE
/// [`AtomKwarg::project_at`] per-item shape-gate composition, the
/// only shape delta being the axis identity riding through the
/// `Self` type parameter (`<&str>::SHAPE` = `String` vs.
/// `<bool>::SHAPE` = `Bool`) and the return-type ownership
/// (`String::from`-lifted for the borrow-off-Sexp `&'a str` axis vs.
/// direct-return for the `Copy` `bool` axis).
///
/// Theory anchor: THEORY.md §II.1 invariant 1 (typed entry — a
/// per-item shape failure on a list-typed kwarg IS a typed-entry
/// gate the bool-vec surface used to leak past through the serde
/// bridge). THEORY.md §VI.1 (generation over composition — the
/// atom-family per-item shape-gate composition now recurs at THREE
/// list-family sites (string / numeric / bool) all routing through
/// ONE [`AtomKwarg::project_at`] trait default). THEORY.md §V.1
/// (knowable platform — the per-item bool rejection now surfaces
/// the same pattern-matchable [`LispError::TypeMismatch`] variant
/// every authoring surface (LSP, `tatara-check`, REPL) already binds
/// to for the scalar peer).
pub fn extract_bool_list(kw: &Kwargs<'_>, key: &str) -> Result<Vec<bool>> {
    extract_list(kw, key, <bool as AtomKwarg<'_>>::LIST_SHAPE, |idx, s| {
        <bool as AtomKwarg<'_>>::project_at(key, idx, s)
    })
}

/// `Option<Vec<bool>>` sibling of [`extract_bool_list`] — the
/// list-family peer of [`extract_optional_bool`] on the
/// present-vs-absent axis, and the bool-axis peer of
/// [`extract_optional_string_list`] on the atom-family non-numeric
/// list surface. Distinguishes an ABSENT kwarg (`Ok(None)`) from a
/// PRESENT empty list (`Ok(Some(Vec::new()))`) — the required peer
/// [`extract_bool_list`] collapses both cases to `Ok(Vec::new())`,
/// which fits `Vec<bool>` fields (where "no items" and "no kwarg"
/// are semantically the same) but loses the present-vs-absent
/// distinction an `Option<Vec<bool>>` field needs (where `None` and
/// `Some(vec![])` are DISTINCT operator intents — "the operator did
/// not name this kwarg" vs. "the operator explicitly bound the
/// kwarg to an empty list").
///
/// A present-but-non-list kwarg (`:flags #t`) rejects with the SAME
/// typed [`LispError::TypeMismatch`] variant the required peer
/// [`extract_bool_list`] emits (`expected list of bools, got bool` —
/// via the `<bool>::LIST_SHAPE = ListOfBools` per-axis trait-const
/// override sharpening the pre-lift ambiguous `expected list, got
/// bool` bytes); a per-item non-bool element inside a present list
/// (`:flags (list #t "yes" #f)`) rejects with the SAME
/// [`LispError::TypeMismatch { form: Item { key, idx }, expected:
/// Bool, got: String }`] variant its per-item peer emits through
/// the shared [`AtomKwarg::project_at`] atom-family shape gate. The
/// two peers ([`extract_bool_list`] and this one) delegate to the
/// SAME per-item gate — only the outer present-vs-absent axis's
/// return shape differs (`Vec<bool>` vs. `Option<Vec<bool>>`).
///
/// Sibling posture to [`extract_optional_string_list`] on the
/// atom-family non-numeric list surface: both distinguish `None`
/// (absent) from `Some(_)` (present-and-decoded) at the `Option`
/// layer via the same `kw.contains_key(key)` short-circuit, and
/// both delegate their present-branch decode to their required peer
/// ([`extract_bool_list`] / [`extract_string_list`]). Peer to
/// [`extract_optional_via_serde`] on the universal-serde-fallthrough
/// axis: both share the `kw.contains_key(key)` present-vs-absent
/// bifurcation; the differences are that this extractor rides the
/// typed atom-family per-item shape gate rather than the serde
/// bridge's per-item `from_value_with_path` decode, so a per-item
/// shape mismatch surfaces as a pattern-matchable
/// [`LispError::TypeMismatch`] variant rather than as a
/// [`LispError::KwargDeserialize`] substring.
///
/// Pre-lift the derive routed `Option<Vec<bool>>` through the
/// universal-serde bridge ([`extract_optional_via_serde`] via
/// [`crate::domain::from_value_with_path`]) — [`classify_option`]
/// in [`tatara_lisp_derive`] had no arm for `Option<Vec<bool>>` and
/// fell through to `Kind::OptionalDeserialize`, matching the pre-
/// lift posture that [`Option<Vec<String>>`] shared before its per-
/// axis route landed. So a per-item shape mismatch inside
/// `Option<Vec<bool>>` surfaced as a mystery
/// `LispError::KwargDeserialize { message: "invalid type: string
/// \"yes\", expected a boolean at path .1" }` substring rather than
/// as the typed
/// `LispError::TypeMismatch { form: Item { key, idx }, expected:
/// Bool, got: String }` its required peer's per-item gate already
/// emits. Post-lift the two `Option<Vec<bool>>` / `Vec<bool>` peers
/// on the bool axis share ONE rejection vocabulary — the same
/// closure the string-axis optional-vec peer already made on its
/// own pair.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 (typed entry — a
/// per-item shape failure on an `Option<Vec<bool>>` kwarg IS a
/// typed-entry gate the optional-bool-vec surface used to leak past
/// through the serde bridge). THEORY.md §VI.1 (generation over
/// composition — the present-vs-absent + delegate-to-required
/// posture now recurs at FOUR sites (`extract_optional_string` /
/// `extract_optional_bool` / `extract_optional_string_list` / this
/// — all routing through the SAME [`optional`] short-circuit paired
/// with required-peer delegation). THEORY.md §V.1 (knowable
/// platform — the per-item bool rejection now surfaces the same
/// pattern-matchable [`LispError::TypeMismatch`] variant every
/// authoring surface (LSP, `tatara-check`, REPL) already binds to
/// for the required peer).
pub fn extract_optional_bool_list(kw: &Kwargs<'_>, key: &str) -> Result<Option<Vec<bool>>> {
    optional_from_required(kw, key, extract_bool_list)
}

// ── Universal serde-Deserialize fallthrough (enums, nested structs, …) ──
//
// `#[derive(TataraDomain)]` covers `String` / numeric / `bool` / their
// `Option` and `Vec<String>` shapes with the typed extractors above. Any
// field type outside that closed set falls through to these helpers, which
// project the kwarg `Sexp` to canonical JSON via `sexp_to_json` and feed
// it to `serde_json::from_value` — works for any `serde::Deserialize`.
//
// The shape used to live inline in three `quote!` blocks in the derive
// macro (`Kind::Deserialize`, `Kind::OptionalDeserialize`,
// `Kind::VecDeserialize`). Lifting them here means:
//   - Hand-written `TataraDomain` impls share the same error path.
//   - Future diagnostic upgrades (attaching a source position once `Sexp`
//     carries spans, richer field-path traces) happen in ONE function,
//     not three macro-emitted copies.
//   - The `:<key> deserialize: …` message is a single named primitive in
//     the substrate — `tatara-check` / LSP / REPL render it uniformly.
//
// Both helpers below funnel through the structural
// `LispError::KwargDeserialize { path: KwargPath, message }` variant —
// the typed-entry-side `from_value` mirror of the typed-exit-side
// `to_value` `LispError::DomainSerialize { keyword, message }` lift. The
// two sites bifurcate via the typed `KwargPath` enum's variant identity:
// `KwargPath::Named(key)` for kwarg-keyed failures (the
// `extract_via_serde` / `extract_optional_via_serde` path),
// `KwargPath::Item { key, idx }` for kwarg-AND-index-keyed failures (the
// `extract_vec_via_serde` per-item path). After this lift the
// `from_value` boundary's two distinct rejection modes BOTH bind to ONE
// structural variant of `LispError`, not a `Compile`-shaped substring;
// the `(key, idx: Option<usize>)` bifurcation collapses into
// `KwargPath`'s `Named` vs. `Item` variant identity, so the invalid
// sibling-slot combination `(key: "", idx: Some(_))` for a scalar path
// is structurally unrepresentable rather than re-asserted at the helper
// boundary via runtime `Option::is_some` comparison. Together with
// `DomainSerialize`, every distinct `serde_json` failure mode at the
// typed-domain JSON boundary — both directions of the round-trip — is
// now structurally typed. This is the LAST `LispError::Compile { ... }`
// construction site in this file.
//
// Theory anchor: THEORY.md §VI.1 (generation over composition — the
// generator must lean on the library, not duplicate the library inline).
// THEORY.md §II.1 invariant 1 (typed entry) — `from_value` failures are
// exactly the failure mode the typed-entry JSON gate exists to reject;
// naming them structurally is the typed posture for that gate's
// diagnostic.

/// Project a single `&Sexp` through the typed-entry JSON boundary —
/// `sexp_to_json` canonical-JSON projection + `serde_json::from_value::<T>`
/// + structural `LispError::KwargDeserialize { path, message }` on failure.
///
/// THREE call sites in this module used to assemble this shape inline:
/// `extract_via_serde` (required scalar kwarg path), `extract_optional_via_serde`
/// (optional scalar kwarg path), and `extract_vec_via_serde`'s per-item
/// closure (each item in a `Vec<T>` kwarg). The three byte-identical
/// `let json = sexp_to_json(sexp)?; serde_json::from_value(json).map_err(|e|
/// deserialize_*_err(<path-args>, &e))` shapes — modulo the typed
/// `KwargPath` constructor (`KwargPath::Named` vs. `KwargPath::Item`) —
/// collapse to ONE primitive parameterized by `path: KwargPath`. The
/// path's variant identity bifurcates scalar-vs-item rendering inside
/// `KwargPath`'s Display impl (`:<key>` vs. `:<key>[<idx>]`) so the helper
/// is shape-of-typed-entry-JSON-boundary, not shape-of-call-site.
///
/// After this lift the three-times-rule on the `from_value` projection
/// shape is decisively crossed; the two prior-run thin `deserialize_err`
/// / `deserialize_item_err` shims — which encapsulated only the
/// `KwargPath::named(_)` / `KwargPath::item(_,_)` constructor projection
/// over an already-extant `serde_json::Error` reference — are subsumed
/// by this primitive's `map_err` closure. The three extractor entry
/// points now bind on `from_value_with_path::<T>` directly with their
/// `KwargPath` constructed at the call boundary; the JSON-boundary's
/// rejection shape (`LispError::KwargDeserialize { path, message }`)
/// lives in ONE place — the `map_err` arm here — instead of being
/// re-asserted at three site-specific shims.
///
/// `<T: DeserializeOwned>` is generic so the helper handles every serde-
/// projectable typed-domain field uniformly — scalar `i64` / `String` /
/// nested struct / `Vec<Nested>` / enum-by-symbol — same posture as the
/// `extract_atom` / `extract_optional_atom` generic-projection primitives
/// for the atom-typed kwarg path. `path: KwargPath` flows into the
/// variant's typed slot directly (owned), parallel to how `type_mismatch`
/// threads `KwargPath` into `LispError::TypeMismatch.form`. A future
/// fourth path shape (e.g. `:<key>.<field>` for nested-struct kwarg
/// failures) extends `KwargPath` ONCE and rustc-enforces matching at
/// every projection site; this helper picks up the new shape mechanically
/// with no signature change.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// three-times rule's load-bearing trigger. THEORY.md §V.1 — knowable
/// platform; the typed-entry JSON-projection boundary's rejection shape
/// lives in ONE primitive so authoring surfaces (`tatara-check`, REPL,
/// LSP) pick up the diagnostic-shape promotion mechanically once the
/// variant is structurally extended. THEORY.md §II.1 invariant 1 (typed
/// entry) — a `from_value` failure is exactly the failure mode the
/// typed-entry JSON gate exists to reject; naming its single shape lifts
/// the gate from three-site duplication to one rust function the
/// substrate's diagnostic promotions hang off of.
fn from_value_with_path<T: DeserializeOwned>(sexp: &Sexp, path: KwargPath) -> Result<T> {
    let json = sexp.to_json()?;
    serde_json::from_value(json).map_err(|e| LispError::KwargDeserialize {
        path,
        message: e.to_string(),
    })
}

/// Required field — feeds the kwarg's canonical-JSON projection to
/// `serde_json::from_value::<T>` via `from_value_with_path` with a
/// `KwargPath::Named(key)` path slot. Errors carry `:key` so authoring
/// tools can point at the offending kwarg.
pub fn extract_via_serde<T: DeserializeOwned>(kw: &Kwargs<'_>, key: &str) -> Result<T> {
    from_value_with_path(required(kw, key)?, KwargPath::named(key))
}

/// `Option<T>` sibling of [`extract_via_serde`] — the SCALAR peer of
/// [`extract_optional_vec_via_serde`] on the universal-serde-
/// fallthrough axis, and the present-vs-absent bifurcated peer of
/// [`extract_via_serde`] on the same scalar-serde-bridge axis.
/// Absent kwarg → `Ok(None)`; a present kwarg flows through
/// [`extract_via_serde`] (the required peer) and wraps the decoded
/// value in `Some(_)` on success. A present-but-non-decodable kwarg
/// (`:level 5` for an enum `Severity`) rejects with the SAME
/// [`LispError::KwargDeserialize { path: KwargPath::Named(key), .. }`]
/// variant the required peer emits through [`from_value_with_path`]
/// at [`KwargPath::named(key)`] — the two peers on the scalar-serde
/// axis share ONE rejection vocabulary, byte-identical modulo the
/// `Ok`-side `Some(_)` wrap.
///
/// Delegates to [`optional_from_required`] with
/// [`extract_via_serde::<T>`] as its required extractor. The scalar
/// universal-serde peer of the just-lifted list-family
/// [`extract_optional_vec_via_serde`] and the scalar-family
/// counterpart of the atom-family list peers
/// ([`extract_optional_string_list`] /
/// [`extract_optional_bool_list`] /
/// [`extract_optional_narrowed_list`]) — all five now route their
/// present-vs-absent bifurcation through the SAME
/// [`optional_from_required`] substrate primitive.
///
/// Pre-lift this extractor spelled the two-arm bifurcation inline
/// (`let Some(sexp) = optional(kw, key) else { return Ok(None); };
/// from_value_with_path(sexp, KwargPath::named(key)).map(Some)`) —
/// the LAST inline present-vs-absent bifurcation on the extract_
/// optional_ scalar family whose T is owned (not lifetime-bound to
/// `kw`, which pre-lift blocked delegation through
/// [`optional_from_required`]'s erased-lifetime `F` bound and kept
/// the inline shape at [`extract_optional_atom`] as a design
/// invariant of the primitive). Post-lift the substrate primitive's
/// `F` bound rides an explicit `'a` lifetime (`for<'a> F: FnOnce
/// (&'a Kwargs<'a>, &str) -> Result<T>` at the call site, coerced
/// from every fn-item consumer's HRTB signature; specific `'a` for
/// the borrowed-`T` closure caller) so the atom-family scalar peer
/// [`extract_optional_atom`] delegates through the SAME primitive
/// too — the substrate now owns the bifurcation at ONE named site
/// across every axis of the extract_optional_ family with no inline
/// exceptions; a future diagnostic promotion on the present-vs-
/// absent gate (a probe, a metric, a span) lands at ONE owner and
/// flows to the scalar peer here without a per-caller edit.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 (typed entry — a
/// scalar `Option<T>` serde-decode failure IS a typed-entry gate
/// the present-vs-absent bifurcation lifts through the primitive).
/// THEORY.md §VI.1 (generation over composition — the present-vs-
/// absent + delegate-to-required posture now recurs at SEVEN peer
/// sites all routing through the SAME [`optional_from_required`]
/// substrate primitive; the three-times-rule trigger keeps holding
/// as the scalar-narrowed peer [`extract_optional_narrowed`] and the
/// atom-family scalar peer [`extract_optional_atom`] land as the
/// sixth and seventh consumers). THEORY.md §V.1 (knowable platform
/// — the rejection on `Option<T>` now surfaces the same pattern-
/// matchable [`LispError::KwargDeserialize { path: KwargPath::Named
/// (key), .. }`] variant its required peer emits, so a future
/// span-carrying promotion of [`KwargPath::Named`] binds
/// mechanically for the scalar peer too).
pub fn extract_optional_via_serde<T: DeserializeOwned>(
    kw: &Kwargs<'_>,
    key: &str,
) -> Result<Option<T>> {
    optional_from_required(kw, key, extract_via_serde::<T>)
}

/// `Vec<T>` field — empty vec if the kwarg is absent; otherwise the kwarg
/// must be a `Sexp::List` and each item flows through `from_value_with_path`
/// with a `KwargPath::Item { key, idx }` path slot, naming both the outer
/// kwarg AND the failing item index in any per-item rejection.
pub fn extract_vec_via_serde<T: DeserializeOwned>(kw: &Kwargs<'_>, key: &str) -> Result<Vec<T>> {
    extract_list(kw, key, ExpectedKwargShape::List, |idx, item| {
        from_value_with_path(item, KwargPath::item(key, idx))
    })
}

/// `Option<Vec<T>>` sibling of [`extract_vec_via_serde`] — the
/// present-vs-absent bifurcated peer of the required-vec serde bridge
/// on the universal-serde-fallthrough list-family surface.
/// Distinguishes an ABSENT kwarg (`Ok(None)`) from a PRESENT empty
/// list (`Ok(Some(Vec::new()))`) — the required peer's posture
/// collapses both cases to `Ok(Vec::new())`, which fits `Vec<T>`
/// fields (where "no items" and "no kwarg" are semantically the same)
/// but loses the present-vs-absent distinction an `Option<Vec<T>>`
/// field needs (where `None` and `Some(vec![])` are DISTINCT operator
/// intents — "the operator did not name this kwarg" vs. "the operator
/// explicitly bound the kwarg to an empty list").
///
/// A present-but-non-list kwarg (`:steps "scalar"`) rejects with the
/// SAME typed [`LispError::TypeMismatch`] variant the required peer
/// [`extract_vec_via_serde`] emits (`expected list, got string`) via
/// the shared [`extract_list`] outer-shape gate; a per-item serde
/// decode failure inside a present list (`:steps ((:notify-ref "ok")
/// (:notify-ref 7))`) rejects with the SAME structural
/// [`LispError::KwargDeserialize { path: KwargPath::Item { key, idx },
/// message }`] variant its per-item peer emits through
/// [`from_value_with_path`] at [`KwargPath::item(key, idx)`]. The
/// two peers ([`extract_vec_via_serde`] and this one) delegate to the
/// SAME per-item bridge — only the outer present-vs-absent axis's
/// return shape differs (`Vec<T>` vs. `Option<Vec<T>>`).
///
/// Sibling posture to the four atom-family optional-vec peers
/// ([`extract_optional_string_list`] / [`extract_optional_bool_list`] /
/// [`extract_optional_narrowed_list`] on the string / bool / numeric
/// axes) on the universal-serde-fallthrough axis: all five share the
/// present-vs-absent bifurcation the [`optional_from_required`]
/// substrate primitive owns, and each delegates its present-branch
/// per-item decode to its own axis-typed required peer. The differences
/// are that this extractor rides the universal serde bridge's per-item
/// [`from_value_with_path`] decode rather than an atom-family
/// [`AtomKwarg::project_at`] shape gate or a numeric-narrowing
/// [`NarrowNumeric::narrow`] projection, so a per-item shape/decode
/// mismatch surfaces as a pattern-matchable
/// [`LispError::KwargDeserialize { path: KwargPath::Item { key, idx },
/// .. }`] variant — the SAME rejection shape its required peer emits,
/// where the atom-family peers surface the typed
/// [`LispError::TypeMismatch { form: KwargPath::Item { key, idx },
/// .. }`] variant of the axis-typed shape gate.
///
/// Pre-lift the derive routed `Option<Vec<Nested>>` (any non-atomic
/// inner type — a struct, an enum, a nested `Vec<T>`) through the
/// universal `extract_optional_via_serde::<Vec<Nested>>` bridge —
/// [`classify_option`] in [`tatara_lisp_derive`] had no arm for
/// `Kind::VecDeserialize` and fell through to
/// `Kind::OptionalDeserialize`, matching the pre-lift posture that
/// [`Vec<bool>`] / [`Vec<u16>`] / [`Option<Vec<String>>`] /
/// [`Option<Vec<u16>>`] / etc. shared before their per-axis routes
/// landed. So a per-item shape mismatch inside `Option<Vec<Nested>>`
/// surfaced as a serde-substring
/// `LispError::KwargDeserialize { path: KwargPath::Named(key),
/// message: "invalid type: ..., expected ..., at path .1" }`
/// diagnostic keyed off the substring inside the message rather than
/// as the typed
/// `LispError::KwargDeserialize { path: KwargPath::Item { key, idx },
/// .. }` its required peer [`extract_vec_via_serde`] already emits
/// through the per-item bridge — the SAME class of gate-leak the
/// recent per-axis [`extract_bool_list`] / [`extract_narrowed_list`] /
/// [`extract_optional_bool_list`] / [`extract_optional_narrowed_list`]
/// closures already closed on the atom-family surface. Post-lift the
/// two `Option<Vec<T>>` / `Vec<T>` peers on the universal-serde
/// fallthrough axis share ONE rejection vocabulary, closing the LAST
/// atom-family × mode Cartesian-product hole in the derive's typed-
/// entry surface for `Option<Vec<T>>`-shaped fields — every
/// `Option<Vec<T>>` field now surfaces per-item rejections through a
/// [`KwargPath::Item { key, idx }`] path root rather than a
/// [`KwargPath::Named(key)`] one.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 (typed entry — a
/// per-item shape failure on an `Option<Vec<Nested>>` kwarg IS a
/// typed-entry gate the optional-nested-vec surface used to leak past
/// through the universal serde bridge). THEORY.md §VI.1 (generation
/// over composition — the present-vs-absent + delegate-to-required
/// posture now recurs at SEVEN peer sites (four list-family —
/// [`extract_optional_string_list`] / [`extract_optional_bool_list`] /
/// [`extract_optional_narrowed_list`] / this — plus three scalar-
/// family — [`extract_optional_via_serde`] on the universal-serde
/// axis, [`extract_optional_narrowed`] on the numeric-narrowed axis,
/// and [`extract_optional_atom`] on the atom-shape axis), all
/// routing through the SAME [`optional_from_required`] substrate
/// primitive). THEORY.md
/// §V.1 (knowable platform — the per-item rejection on
/// `Option<Vec<T>>` now surfaces the same pattern-matchable
/// [`LispError::KwargDeserialize { path: KwargPath::Item { .. }, ..
/// }`] variant every authoring surface (LSP, `tatara-check`, REPL)
/// already binds to for the required peer, so a future span-carrying
/// promotion of [`KwargPath::Item`] binds mechanically for
/// `Option<Vec<T>>` fields too).
pub fn extract_optional_vec_via_serde<T: DeserializeOwned>(
    kw: &Kwargs<'_>,
    key: &str,
) -> Result<Option<Vec<T>>> {
    optional_from_required(kw, key, extract_vec_via_serde::<T>)
}

// ── Domain registry (runtime-registered, callable by keyword) ───────

/// Erased handler that knows how to compile a form and hand back a typed
/// serde-JSON representation. JSON is the least-common-denominator typed
/// surface — every `TataraDomain` derives `serde::Serialize` by convention.
pub struct DomainHandler {
    pub keyword: &'static str,
    pub compile: fn(args: &[Sexp]) -> Result<serde_json::Value>,
}

static REGISTRY: OnceLock<Mutex<HashMap<&'static str, DomainHandler>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<&'static str, DomainHandler>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a `TataraDomain` type with the global dispatcher.
/// Idempotent — repeated registrations overwrite.
pub fn register<T>()
where
    T: TataraDomain + serde::Serialize,
{
    let handler = DomainHandler {
        keyword: T::KEYWORD,
        compile: |args| {
            let v = T::compile_from_args(args)?;
            serde_json::to_value(&v).map_err(serialize_to_json_err::<T>)
        },
    };
    registry().lock().unwrap().insert(T::KEYWORD, handler);
}

/// Look up a handler by keyword.
pub fn lookup(keyword: &str) -> Option<DomainHandler> {
    let reg = registry().lock().unwrap();
    reg.get(keyword).map(|h| DomainHandler {
        keyword: h.keyword,
        compile: h.compile,
    })
}

/// List currently registered keywords.
pub fn registered_keywords() -> Vec<&'static str> {
    registry().lock().unwrap().keys().copied().collect()
}

/// Suggest the registered domain keyword closest to `needle`, when the
/// closest one is within a bounded edit distance (`suggest`'s contract).
///
/// Wraps `suggest` over `registered_keywords()` so consumers don't repeat
/// the candidate-set assembly per call site. Authoring surfaces with an
/// unknown registry-dispatched form (`tatara-check`'s unknown-keyword
/// fallthrough, future LSP completion-failure paths, REPL hints) bind to
/// ONE primitive instead of pulling the keyword set themselves and
/// re-implementing edit-distance ranking. The result is `&'static str`
/// because every registered keyword is itself `'static` (the trait's
/// `KEYWORD` const), so the substrate hands back the exact same pointer
/// the registry stores — no allocation, no lifetime juggling.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render
/// Anywhere." A diagnostic that says "unknown form: `defmoniter`" but
/// withholds the registered near-miss forces the operator to scan the
/// registry's keyword list visually; naming the candidate is the floor
/// of a constructive diagnostic. THEORY.md §VI.1 — generation over
/// composition: every "near-miss across the registry" lookup routes
/// through ONE primitive.
#[must_use]
pub fn suggest_keyword(needle: &str) -> Option<&'static str> {
    let keywords = registered_keywords();
    suggest(needle, &keywords)
}

/// Structural unknown-domain-keyword builder. Returns the dedicated
/// `LispError::UnknownDomainKeyword` variant so authoring surfaces
/// (`tatara-check`, the REPL, the LSP) bind to first-class
/// `keyword` / `hint` / `registered` fields instead of substring-parsing
/// the rendered message. The shape mirrors `unknown_kwarg`: the
/// kwarg-gate's unknown-allowed-set rejection and the registry-gate's
/// unknown-registered-set rejection share ONE structural primitive shape
/// across two structural variants — the substrate's diagnostic surface
/// stays uniform.
///
/// Encapsulates the three otherwise-inline steps every unknown-domain-
/// keyword site shares: (1) ranking the near-miss via `suggest_keyword`
/// over `registered_keywords()`, (2) sorting the registered set
/// lexicographically so two operators on two machines see the same
/// message for the same input — diagnostics are deterministic regardless
/// of HashMap iteration order, (3) materializing the registered set as
/// owned `Vec<String>` so the variant lives independent of the call frame
/// and crosses thread boundaries cleanly.
///
/// `tatara-check`'s registry-dispatch fallthrough is the first consumer;
/// hand-written authoring surfaces (LSP completion-failure fallback, REPL
/// hints, future multi-error collectors that name every unregistered
/// `(defX …)` form in one pass) bind to ONE function instead of
/// re-formatting the shape per call site.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render Anywhere."
/// A diagnostic whose offending `keyword` / `hint` / `registered`-set are
/// embedded in a free-form message is structurally incomplete; an
/// authoring surface that wants to render a squiggly under the typo or
/// surface the registered set as completions must re-parse the message.
/// After this lift the slots exist in the variant's data shape itself.
/// THEORY.md §VI.1 — generation over composition: every "near-miss across
/// the registry" lookup routes through `suggest_keyword`, every "diagnose
/// an unregistered head against the registry" routes through this
/// primitive.
#[must_use]
pub fn unknown_domain_keyword(keyword: &str) -> LispError {
    let hint = suggest_keyword(keyword).map(String::from);
    let mut registered: Vec<String> = registered_keywords()
        .into_iter()
        .map(String::from)
        .collect();
    registered.sort();
    LispError::UnknownDomainKeyword {
        keyword: keyword.to_string(),
        hint,
        registered,
    }
}

// ── Sexp ↔ serde_json bridge (universal type support) ──────────────
//
// Lets the derive macro fall through to `serde_json::from_value` for any
// field type implementing `Deserialize`. Handles enums (via symbol→string),
// nested structs (via kwargs→object), and `Vec<T>` of either.

use serde_json::Value as JValue;

/// Thin delegate to [`Sexp::to_json`] retained for callers that want
/// the free-function reach — the canonical site is now the inherent
/// method on the [`Sexp`] algebra (sibling-lift posture to
/// [`super::domain::sexp_shape`] → [`Sexp::shape`] (commit 121bb60)
/// and [`super::domain::sexp_witness`] → [`Sexp::witness`] (commit
/// a427e3b)). Rules + round-trip semantics live at
/// [`Sexp::to_json`]'s docstring.
///
/// Composition law: `sexp_to_json(s) == s.to_json()` for every `s:
/// &Sexp`. Pre-lift the dispatcher lived here as the canonical site;
/// post-lift the inherent method [`Sexp::to_json`] is the canonical
/// site and this free function delegates so existing callers
/// continue to compile.
pub fn sexp_to_json(s: &Sexp) -> Result<JValue> {
    s.to_json()
}

/// Thin delegate to [`Sexp::from_json`] retained for callers that want
/// the free-function reach — the canonical site is now the inherent
/// associated function on the [`Sexp`] algebra (sibling-lift posture to
/// [`super::domain::sexp_to_json`] → [`Sexp::to_json`] (commit 875ee3b),
/// [`super::domain::sexp_witness`] → [`Sexp::witness`] (commit a427e3b),
/// and [`super::domain::sexp_shape`] → [`Sexp::shape`] (commit
/// 121bb60)). Rules + round-trip semantics live at
/// [`Sexp::from_json`]'s docstring.
///
/// Composition law: `json_to_sexp(v) == Sexp::from_json(v)` for every
/// `v: &JValue`. Pre-lift the dispatcher lived here as the canonical
/// site; post-lift the inherent associated function
/// [`Sexp::from_json`] is the canonical site and this free function
/// delegates so existing callers continue to compile. With this lift the
/// substrate's `Sexp` ↔ `serde_json::Value` round-trip closure
/// ([`Sexp::to_json`] + [`Sexp::from_json`]) lives entirely on the
/// [`Sexp`] algebra; the four free functions that pre-dated the lift
/// chain (`sexp_to_json`, `json_to_sexp`, `sexp_shape`, `sexp_witness`)
/// are all delegates now — the canonical-form / structural-projection
/// surface is structurally on the algebra.
pub fn json_to_sexp(v: &JValue) -> Sexp {
    Sexp::from_json(v)
}

/// `must-reach` → `mustReach`, `point-type` → `pointType`.
pub(crate) fn kebab_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for c in s.chars() {
        if c == '-' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// `mustReach` → `must-reach` (inverse of `kebab_to_camel`).
///
/// `pub(crate)` because [`Sexp::from_json`] threads JSON object keys
/// through this projection to recover the `:k`'s kebab-case authoring
/// shape — the inverse of [`Sexp::to_json`]'s [`kebab_to_camel`] arm.
/// Promoted from private to crate-visible in the same lift that moved
/// the `json_to_sexp` dispatcher onto the [`Sexp`] algebra; the
/// projection's call site moves with it, the visibility follows.
pub(crate) fn camel_to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('-');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

// ── TypedRewriter — the self-optimization primitive ────────────────
//
// Takes a typed value, converts to Sexp, applies a Lisp rewrite, then
// re-enters the typed boundary via `compile_from_args`. Any rewrite that
// passes the typed re-validation is safe by construction — the Rust type
// system is the floor.

/// Promote the previously `LispError::Compile`-shaped helper into the
/// structural `LispError::DomainSerialize { keyword, message }` variant
/// — the typed-exit-side `to_value` mirror of the typed-entry-side
/// `NamedFormNonSymbolName` / typed-exit-side `RewriterNonList` lifts.
/// The gate fires when `serde_json::to_value` of a typed `T` value
/// errors at two byte-identical sites: `register::<T>`'s registry-
/// dispatch closure (serializes the just-typed value to JSON for the
/// dispatcher) and `rewrite_typed::<T>`'s round-trip prelude
/// (serializes the input to JSON before projecting to a `Sexp::List`
/// for the rewriter closure). Both share the exact same failure mode
/// and the exact same `keyword` projection from `T::KEYWORD`; the lift
/// promotes them to ONE structural variant so authoring tools (REPL,
/// LSP, `tatara-check`) bind on variant identity rather than
/// substring-grepping the rendered diagnostic.
///
/// `<T: TataraDomain>` is the type-level boundary: the `T::KEYWORD`
/// projection is mechanically applied to the variant's `keyword: &'static
/// str` slot, so a typo can never drift across the two call sites — the
/// type system is the floor, same posture as `RewriterNonList.keyword`,
/// `NamedFormMissingName.keyword`, `NamedFormNonSymbolName.keyword`,
/// `NotAListForm.keyword`, `MissingHeadSymbol.keyword`,
/// `HeadMismatch.keyword`, and the `Defmacro*.head` family. The helper
/// takes `serde_json::Error` by value so
/// `map_err(serialize_to_json_err::<T>)` composes point-free at every
/// site — no `.into()` boilerplate, no `&e` borrow at the call site.
/// The `serde_json::Error::Display` projection is materialized into the
/// variant's `message: String` slot at the boundary so the variant
/// lives independent of the call frame and the original error chain
/// (other variants in this enum are also `String`-carrying;
/// participating in the same Display contract keeps every consumer's
/// rendering pipeline uniform).
///
/// Display matches the legacy `Compile`-shaped diagnostic byte-for-byte
/// — `"compile error in {keyword}: serialize: {message}"` — so
/// existing substring-grep consumers (`tatara-check`'s diagnostic
/// capture, REPL substring-greps that match on `"serialize: "`) pass
/// unchanged across the lift. The redundant-keyword `"serialize
/// {KEYWORD}: …"` shape that `rewrite_typed` carried pre-canonicalize
/// is already gone (the canonicalize step landed before the structural
/// lift); both sites render the cleaner `"serialize: …"` shape now.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// three-times rule, decisively crossed across two functions in this
/// file (`register::<T>` + `rewrite_typed::<T>`, two sites; the third
/// `to_value`-side gate `rewriter_non_list_err::<T>` immediately below
/// is the typed-exit-list sibling). After this lift the `to_value`
/// boundary's two distinct rejection modes BOTH bind to structural
/// variants of `LispError` keyed on `T::KEYWORD` —
/// `DomainSerialize { keyword, message }` (serialize-failed) +
/// `RewriterNonList { keyword, got }` (output-wrong-shape) — so the
/// substrate's typed-exit JSON surface is structurally complete on the
/// emission side. The `from_value` direction (the typed-entry JSON
/// boundary, kwargs-path-keyed via `deserialize_err` /
/// `deserialize_item_err`) now binds to the sibling
/// `LispError::KwargDeserialize { path: KwargPath, message }` variant,
/// closing the round-trip's last `LispError::Compile { ... }` site in
/// this file; both directions of the JSON-projection boundary are
/// structural. THEORY.md §II.1 invariant 1
/// (typed entry) + invariant 3 (typed exit): the JSON-projection
/// round-trip is the proof; the helper names its rejection shape at
/// the type level so authoring surfaces bind to a uniform "serialize-
/// failed" structural shape regardless of whether the failure
/// originated at registry-dispatch time or rewriter time.
fn serialize_to_json_err<T: TataraDomain>(e: serde_json::Error) -> LispError {
    LispError::DomainSerialize {
        keyword: T::KEYWORD,
        message: e.to_string(),
    }
}

/// Promote the previously `LispError::Compile`-shaped helper into the
/// structural `LispError::RewriterNonList { keyword, got }` variant —
/// the typed-exit-side mirror of the typed-entry-side
/// `NamedFormNonSymbolName` lift. The gate enforces the rewriter's
/// `Sexp::List` contract: the round-trip projects a typed value to
/// `Sexp::List` via `json_to_sexp`, hands that list to the rewriter
/// `F`, and re-enters `T::compile_from_args` via the list's items. A
/// non-list result violates the round-trip's structural promise — this
/// helper names that violation at the type level so authoring tools
/// (REPL, LSP, `tatara-check`) bind on variant identity rather than
/// substring-grepping the rendered diagnostic.
///
/// After this lift the self-optimization primitive's rejection chain is
/// structurally typed at BOTH boundaries: typed-entry
/// (`NamedFormMissingName`, `NamedFormNonSymbolName`) AND typed-exit
/// (this variant). The typed-entry chain rejects a wrong-shaped author-
/// supplied form before `compile_from_args` runs; the typed-exit chain
/// rejects a wrong-shaped rewriter output before `compile_from_args`
/// re-runs on the round-tripped representation. Every distinct rejection
/// mode in `rewrite_typed::<T>` is now a pattern-matchable variant.
///
/// `<T: TataraDomain>` carries the keyword projection — `T::KEYWORD`
/// (`&'static str`) flows into the variant's `keyword` slot at the
/// boundary so a typo in the keyword can never drift into the diagnostic
/// at runtime, same posture as `NamedFormNonSymbolName.keyword`,
/// `NamedFormMissingName.keyword`, `MissingHeadSymbol.keyword`,
/// `HeadMismatch.keyword`, `NotAListForm.keyword`, and the
/// `Defmacro*.head` family. The helper takes `got: &Sexp` and projects
/// it through `sexp_witness(got)` at the boundary so the variant's
/// `got: SexpWitness` slot carries the rewriter's offending output as
/// the typed joint identity — BOTH the structural `SexpShape` AND the
/// `Sexp::Display` literal in ONE owned value, parallel to the seven
/// typed-ENTRY-side lifts (`SpliceOutsideList.got: SexpWitness`,
/// `NonSymbolUnquoteTarget.got: SexpWitness`,
/// `NonSymbolParam.got: SexpWitness`,
/// `DefmacroNonSymbolName.got: SexpWitness`,
/// `DefmacroNonListParams.got: SexpWitness`,
/// `RestParamMissingName.got: Option<SexpWitness>`,
/// `MissingHeadSymbol.got: Option<SexpWitness>`). This is the EIGHTH
/// `SexpWitness` consumer and the FIRST on the typed-EXIT boundary —
/// before this lift the typed-exit-side `got` slot projected through
/// `Sexp::to_string()`, discarding the `SexpShape` identity at the
/// variant boundary the way the seven entry-side slots used to. The
/// value (not just its sexp-type) is the actionable diagnostic detail
/// for a typed-exit rejection — authoring a rewriter that returns the
/// wrong value is the failure mode being named — AND the structural
/// shape is now load-bearing alongside it.
///
/// Display preserves the legacy `"compile error in {keyword}: rewriter
/// must return a list; got {got}"` shape byte-for-byte so authoring
/// tools that pattern-matched on the pre-lift rendered string see no
/// drift across the lift; tools that pattern-match on the variant
/// gain structural binding to `keyword` AND `got` (BOTH the typed
/// shape via `got.shape` AND the literal via `got.display`). The
/// `{got}` slot flows through `SexpWitness::Display`, which writes
/// only the `display` field, so the rendering is byte-for-byte
/// identical to the pre-lift `got: String` shape.
///
/// Theory anchor: THEORY.md §II.1 invariant 3 (typed exit) —
/// `rewrite_typed` IS the typed-exit gate of the self-optimization
/// primitive; any rewrite that survives the gate is well-typed by
/// construction, AND now the rejection mode's offending-value identity
/// is itself structurally typed at the variant slot, the same posture
/// the seven typed-ENTRY-side lifts established for invariant 1.
/// THEORY.md §V.1 — knowable platform; the typed witness exposes BOTH
/// `got.shape` AND `got.display` as first-class fields so authoring
/// tools bind to the joint identity instead of substring-parsing the
/// rendered diagnostic to recover the shape. THEORY.md §VI.1 —
/// generation over composition; the one inline `got.to_string()`
/// projection at the helper boundary collapses into
/// `sexp_witness(got)` — the typed joint primitive — extending the
/// typed-identity unification contract from the seven entry-side
/// `Sexp::Display`-source `got` slots to the eighth (exit-side) slot.
/// After this lift EVERY `Sexp::Display`-source `got` slot in the
/// substrate, ENTRY-side OR EXIT-side, carries the SAME typed
/// `SexpWitness` primitive — the typed-identity unification contract
/// is closed across BOTH boundaries of the typed-IR algebra.
fn rewriter_non_list_err<T: TataraDomain>(got: &Sexp) -> LispError {
    LispError::RewriterNonList {
        keyword: T::KEYWORD,
        got: got.witness(),
    }
}

/// Rewrite a typed `T` through Lisp form and re-validate on the way back.
///
/// The rewriter receives the value's kwargs representation (a `Sexp::List`
/// of alternating keywords + values) and returns a modified kwargs list.
/// `T::compile_from_args` validates the result — any ill-formed rewrite
/// produces a typed error; any well-formed rewrite produces a valid `T`.
pub fn rewrite_typed<T, F>(input: T, rewrite: F) -> Result<T>
where
    T: TataraDomain + serde::Serialize,
    F: FnOnce(Sexp) -> Result<Sexp>,
{
    let json = serde_json::to_value(&input).map_err(serialize_to_json_err::<T>)?;
    let sexp = json_to_sexp(&json);
    let rewritten = rewrite(sexp)?;
    let args = match rewritten {
        Sexp::List(items) => items,
        other => return Err(rewriter_non_list_err::<T>(&other)),
    };
    T::compile_from_args(&args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::read;
    use serde::{Deserialize, Serialize};
    use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

    /// Example domain authorable as Lisp — proves derive macro, trait, and
    /// registry all agree end-to-end.
    #[derive(DeriveTataraDomain, Serialize, Debug, PartialEq)]
    #[tatara(keyword = "defmonitor")]
    struct MonitorSpec {
        name: String,
        query: String,
        threshold: f64,
        window_seconds: Option<i64>,
        tags: Vec<String>,
        enabled: Option<bool>,
    }

    #[test]
    fn derive_emits_correct_keyword() {
        assert_eq!(MonitorSpec::KEYWORD, "defmonitor");
    }

    #[test]
    fn derive_compiles_full_form() {
        let forms = read(
            r#"(defmonitor
                 :name "prom-up"
                 :query "up{job='prometheus'}"
                 :threshold 0.99
                 :window-seconds 300
                 :tags ("prod" "observability")
                 :enabled #t)"#,
        )
        .unwrap();
        let spec = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(
            spec,
            MonitorSpec {
                name: "prom-up".into(),
                query: "up{job='prometheus'}".into(),
                threshold: 0.99,
                window_seconds: Some(300),
                tags: vec!["prod".into(), "observability".into()],
                enabled: Some(true),
            }
        );
    }

    #[test]
    fn derive_accepts_missing_optionals() {
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold 0.5)"#).unwrap();
        let spec = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(spec.name, "x");
        assert!(spec.window_seconds.is_none());
        assert!(spec.enabled.is_none());
        assert!(spec.tags.is_empty());
    }

    #[test]
    fn derive_errors_on_missing_required() {
        let forms = read(r#"(defmonitor :name "x" :query "q")"#).unwrap();
        assert!(MonitorSpec::compile_from_sexp(&forms[0]).is_err());
    }

    // ── G3 — the RANGE gate on the four numeric arms ────────────────
    //
    // The range gate, sibling to the duplicate-kwarg and
    // unknown-kwarg gates: every form in this block
    // used to COMPILE SUCCESSFULLY and put a number in the struct that
    // the author never wrote. The derive emitted `extract_int(&kw,
    // key)? as u32`, and Rust's `as` is total by truncating — it wraps,
    // sign-flips, and saturates to `inf` without a word. So the failure
    // was not a bad diagnostic; it was silent data corruption behind a
    // green build.
    //
    // Each test below names the exact wrong value the pre-fix code
    // produced, so the test doubles as the record of what regressed if
    // the narrowing is ever backed out: restore the `as` casts and
    // every one of these fails at `expect_err`.

    /// A domain whose numeric fields are all NARROWER than the reader's
    /// wide `i64` / `f64` — the shape the `as` cast corrupted. Covers
    /// both axes and both required/optional arms, i.e. all four numeric
    /// branches of `extractor_for`.
    #[derive(DeriveTataraDomain, Serialize, Debug, PartialEq)]
    #[tatara(keyword = "defnarrow")]
    struct NarrowSpec {
        port: u32,
        offset: i32,
        scale: f32,
        retries: Option<u32>,
        ratio: Option<f32>,
    }

    fn narrow_form(body: &str) -> LispError {
        let forms = read(body).expect("reads");
        NarrowSpec::compile_from_sexp(&forms[0])
            .expect_err("an out-of-range numeric literal must not parse")
    }

    /// The in-range case must still parse — a range gate that rejects
    /// valid input is worse than the truncation it replaced. Includes a
    /// value that LOSES PRECISION at `f32` (`0.1`) to pin that lossy-
    /// but-representable is accepted: an `f32` field asked for `f32`.
    #[test]
    fn narrowing_accepts_every_in_range_value_including_lossy_f32() {
        let forms = read(r"(defnarrow :port 8080 :offset -42 :scale 0.1 :retries 3 :ratio 2.5)")
            .expect("reads");
        let spec = NarrowSpec::compile_from_sexp(&forms[0]).expect("in-range values must parse");
        assert_eq!(
            spec,
            NarrowSpec {
                port: 8080,
                offset: -42,
                #[allow(clippy::cast_possible_truncation)]
                scale: 0.1_f64 as f32,
                retries: Some(3),
                ratio: Some(2.5),
            }
        );
    }

    /// `u32` overflow. Pre-fix: `4294967296 as u32` == `0`, and the
    /// struct came back holding a port of zero.
    #[test]
    fn int_above_the_target_width_is_rejected_not_truncated() {
        let err = narrow_form(r"(defnarrow :port 4294967296 :offset 0 :scale 1.0)");
        let LispError::KwargOutOfRange {
            form,
            target,
            value,
        } = &err
        else {
            panic!("expected KwargOutOfRange, got {err:?}");
        };
        assert_eq!(form, &KwargPath::named("port"));
        assert_eq!(*target, NumericWidth::U32);
        assert_eq!(*value, NumericLiteral::Int(4_294_967_296));
    }

    /// A NEGATIVE literal into an unsigned width. Pre-fix: `-1 as u32`
    /// == `4294967295` — not a truncation but a sign flip, and the
    /// worst of the family because the resulting number looks plausible.
    #[test]
    fn negative_int_into_an_unsigned_width_is_rejected_not_sign_flipped() {
        let err = narrow_form(r"(defnarrow :port -1 :offset 0 :scale 1.0)");
        assert!(
            matches!(
                &err,
                LispError::KwargOutOfRange {
                    target: NumericWidth::U32,
                    value: NumericLiteral::Int(-1),
                    ..
                }
            ),
            "expected a u32 range rejection of -1, got {err:?}"
        );
    }

    /// `i32` overflow — the signed sibling. Pre-fix: `2147483648 as i32`
    /// == `-2147483648`, a positive literal landing as a negative field.
    #[test]
    fn int_above_the_signed_target_width_is_rejected_not_wrapped() {
        let err = narrow_form(r"(defnarrow :port 0 :offset 2147483648 :scale 1.0)");
        assert!(
            matches!(
                &err,
                LispError::KwargOutOfRange {
                    target: NumericWidth::I32,
                    value: NumericLiteral::Int(2_147_483_648),
                    ..
                }
            ),
            "expected an i32 range rejection, got {err:?}"
        );
    }

    /// `f32` overflow. Pre-fix: `1e300 as f32` == `inf`, so a finite
    /// input became a non-finite field and every downstream arithmetic
    /// on it produced `inf`/`NaN`.
    #[test]
    fn float_above_the_target_width_is_rejected_not_saturated_to_infinity() {
        let err = narrow_form(r"(defnarrow :port 0 :offset 0 :scale 1.0e300)");
        let LispError::KwargOutOfRange { target, value, .. } = &err else {
            panic!("expected KwargOutOfRange, got {err:?}");
        };
        assert_eq!(*target, NumericWidth::F32);
        assert!(
            matches!(value, NumericLiteral::Float(x) if (*x - 1.0e300).abs() < f64::EPSILON),
            "the diagnostic must echo the author's own literal, got {value:?}"
        );
    }

    /// The OPTIONAL arms carry the same gate. A present-but-out-of-range
    /// optional is a rejection, never a quiet `None` — dropping the
    /// value would be the same corruption in a different costume.
    #[test]
    fn optional_numeric_arms_reject_out_of_range_rather_than_yielding_none() {
        let int_err = narrow_form(r"(defnarrow :port 0 :offset 0 :scale 1.0 :retries 4294967296)");
        assert!(
            matches!(
                &int_err,
                LispError::KwargOutOfRange {
                    target: NumericWidth::U32,
                    ..
                }
            ),
            "expected an Option<u32> range rejection, got {int_err:?}"
        );

        let float_err = narrow_form(r"(defnarrow :port 0 :offset 0 :scale 1.0 :ratio 1.0e300)");
        assert!(
            matches!(
                &float_err,
                LispError::KwargOutOfRange {
                    target: NumericWidth::F32,
                    ..
                }
            ),
            "expected an Option<f32> range rejection, got {float_err:?}"
        );
    }

    /// An ABSENT optional is still `None` — the gate fires on presence,
    /// not on the field's existence.
    #[test]
    fn absent_optional_numeric_kwargs_stay_none_under_the_range_gate() {
        let forms = read(r"(defnarrow :port 1 :offset 1 :scale 1.0)").expect("reads");
        let spec = NarrowSpec::compile_from_sexp(&forms[0]).expect("absent optionals are legal");
        assert_eq!(spec.retries, None);
        assert_eq!(spec.ratio, None);
    }

    /// The rendered diagnostic names the kwarg, the value, and the
    /// width — the three facts an author needs to fix the source. Pinned
    /// because this is the operator-facing surface, not just the variant.
    #[test]
    fn the_range_diagnostic_names_kwarg_value_and_target_width() {
        let err = narrow_form(r"(defnarrow :port 4294967296 :offset 0 :scale 1.0)");
        assert_eq!(
            err.to_string(),
            "compile error in :port: 4294967296 is out of range for u32"
        );
    }

    /// The IDENTITY widths must not have become rejections: `i64` and
    /// `f64` fields route through the same `NarrowNumeric` projection
    /// with a total impl, so `i64::MIN` still parses. `MonitorSpec`'s
    /// `threshold: f64` / `window_seconds: Option<i64>` are the fields
    /// under test — the same two the pre-fix code cast with a no-op
    /// `as i64` / `as f64`.
    #[test]
    fn identity_widths_stay_total_under_the_range_gate() {
        let forms = read(
            r#"(defmonitor :name "n" :query "q" :threshold 1.0 :window-seconds -9223372036854775808)"#,
        )
        .expect("reads");
        let spec = MonitorSpec::compile_from_sexp(&forms[0])
            .expect("i64::MIN is representable at the identity width");
        assert_eq!(spec.window_seconds, Some(i64::MIN));
    }

    /// The typed width identity is sourced from the TYPE, not from a
    /// literal the derive interpolated — `NarrowNumeric::WIDTH` is the
    /// single producer, so a mislabeled diagnostic is unconstructible
    /// rather than merely untested.
    #[test]
    fn every_supported_width_reports_its_own_typed_identity() {
        assert_eq!(<i8 as NarrowNumeric<i64>>::WIDTH, NumericWidth::I8);
        assert_eq!(<i16 as NarrowNumeric<i64>>::WIDTH, NumericWidth::I16);
        assert_eq!(<i32 as NarrowNumeric<i64>>::WIDTH, NumericWidth::I32);
        assert_eq!(<i64 as NarrowNumeric<i64>>::WIDTH, NumericWidth::I64);
        assert_eq!(<u8 as NarrowNumeric<i64>>::WIDTH, NumericWidth::U8);
        assert_eq!(<u16 as NarrowNumeric<i64>>::WIDTH, NumericWidth::U16);
        assert_eq!(<u32 as NarrowNumeric<i64>>::WIDTH, NumericWidth::U32);
        assert_eq!(<u64 as NarrowNumeric<i64>>::WIDTH, NumericWidth::U64);
        assert_eq!(<usize as NarrowNumeric<i64>>::WIDTH, NumericWidth::Usize);
        assert_eq!(<isize as NarrowNumeric<i64>>::WIDTH, NumericWidth::Isize);
        assert_eq!(<f32 as NarrowNumeric<f64>>::WIDTH, NumericWidth::F32);
        assert_eq!(<f64 as NarrowNumeric<f64>>::WIDTH, NumericWidth::F64);
    }

    /// `f32`'s narrowing rejects exactly one thing — a FINITE input that
    /// overflows — and passes an already-non-finite input through. A
    /// naive `is_finite()` guard would have rejected an authored `inf`
    /// that `as` had preserved correctly, which would be a regression
    /// dressed as a fix.
    #[test]
    fn f32_narrowing_rejects_only_finite_overflow() {
        assert_eq!(<f32 as NarrowNumeric<f64>>::narrow(1.0), Some(1.0_f32));
        assert_eq!(<f32 as NarrowNumeric<f64>>::narrow(1.0e300), None);
        assert_eq!(<f32 as NarrowNumeric<f64>>::narrow(-1.0e300), None);
        assert_eq!(
            <f32 as NarrowNumeric<f64>>::narrow(f64::INFINITY),
            Some(f32::INFINITY)
        );
        assert!(<f32 as NarrowNumeric<f64>>::narrow(f64::NAN)
            .expect("NaN passes through")
            .is_nan());
    }

    /// `WideNumeric` is the companion trait of `NarrowNumeric` on the
    /// OTHER end of the narrowing pipeline: the wide value the reader
    /// hands back gets lifted into its typed [`NumericLiteral`]
    /// variant so the operator-facing rejection echoes exactly what
    /// the author wrote. Pre-lift the wrap was spelled at FOUR call
    /// sites as `NumericLiteral::<Variant>(wide)` byte-for-byte; post-
    /// lift the wrap is a trait method dispatched on the wide type
    /// itself, so a regression that silently re-labeled the axis
    /// (`i64` wrapping through `NumericLiteral::Float`) is
    /// unconstructible rather than merely untested. Pin the two-axis
    /// coverage on the TWO wide types the substrate binds
    /// (`i64` / `f64`), and pin the round-trip through a representative
    /// negative-signed / positive-signed / fractional-float triple.
    #[test]
    fn wide_numeric_lifts_every_wide_axis_into_its_own_typed_literal_variant() {
        assert_eq!(0_i64.as_literal(), NumericLiteral::Int(0));
        assert_eq!(i64::MIN.as_literal(), NumericLiteral::Int(i64::MIN));
        assert_eq!(i64::MAX.as_literal(), NumericLiteral::Int(i64::MAX));
        assert_eq!(70_000_i64.as_literal(), NumericLiteral::Int(70_000));
        assert_eq!(0.0_f64.as_literal(), NumericLiteral::Float(0.0));
        assert!(
            matches!(1.0e300_f64.as_literal(), NumericLiteral::Float(x) if (x - 1.0e300).abs() < f64::EPSILON),
            "the wide axis lift must echo the author's own literal, unchanged",
        );
    }

    /// `WideNumeric::SHAPE` + `WideNumeric::as_wide` are the TWO
    /// per-axis primitives the trait's default [`WideNumeric::
    /// extract_kwarg`] / [`WideNumeric::extract_optional_kwarg`]
    /// compose through the shared [`extract_atom`] /
    /// [`extract_optional_atom`] atom-family skeletons. Pre-lift each
    /// axis's `(ExpectedKwargShape, Sexp::as_X)` pair lived in a
    /// hand-written `extract_int` / `extract_float` function body;
    /// post-lift the pair is an associated const + an associated
    /// function on the wide type itself, so the axis identity rides
    /// ONE trait dispatch and a regression that silently swapped one
    /// axis's rejection label (`ExpectedKwargShape::Int` → `Number`)
    /// or one axis's atom projection (`Sexp::as_int` → `Sexp::as_
    /// float`) surfaces at rustc time as an impl-return / const-type
    /// mismatch rather than as a silent runtime drift. Pin both axes:
    /// (1) the axis-typed `SHAPE` per wide type (`Int` on `<i64>`,
    /// `Number` on `<f64>`), (2) the per-atom `as_wide` projection
    /// through a representative int atom, a representative float atom
    /// (which the int axis rejects and the float axis accepts, per
    /// `Sexp::as_int` / `Sexp::as_float` semantics), and a type-
    /// mismatched bool atom (which BOTH axes reject as `None`,
    /// exactly the case the outer atom-family skeleton lifts into a
    /// `TypeMismatch { expected: Self::SHAPE }` rejection).
    ///
    /// This test also pins the PER-ELEMENT primitive a future
    /// `extract_narrowed_list<W, T>` binds — the atom projection
    /// lifted out of the kwarg extractor so a per-item narrowing
    /// walk (Vec of narrowed numerics) can call `W::as_wide` on
    /// each `Sexp` element directly, without threading it through
    /// the per-kwarg outer skeleton.
    #[test]
    fn wide_numeric_shape_and_as_wide_are_the_two_per_axis_primitives_the_extractor_default_composes(
    ) {
        // (1) SHAPE per axis — the associated const's type pins the
        //     axis-typed rejection label at rustc time; a regression
        //     that silently swapped `Int` and `Number` would fail
        //     to compile the impl's const clause.
        assert_eq!(<i64 as WideNumeric>::SHAPE, ExpectedKwargShape::Int);
        assert_eq!(<f64 as WideNumeric>::SHAPE, ExpectedKwargShape::Number);

        // (2) as_wide per axis — the per-atom projection lifted out
        //     of `extract_int` / `extract_float` bodies so the same
        //     projection is reusable per-element for a future
        //     `extract_narrowed_list<W, T>`.
        let int_atom = Sexp::Atom(Atom::Int(8080));
        let float_atom = Sexp::Atom(Atom::Float(1.5));
        let bool_atom = Sexp::Atom(Atom::Bool(false));

        // Int axis: accepts an int atom byte-for-byte identical to
        // `Sexp::as_int`; rejects the float atom (an int-typed field
        // must not silently truncate `1.5` — the outer skeleton
        // lifts this `None` into a `TypeMismatch { expected: Int }`
        // rejection with the caller's kwarg path); rejects the bool
        // atom.
        assert_eq!(<i64 as WideNumeric>::as_wide(&int_atom), Some(8080_i64));
        assert_eq!(<i64 as WideNumeric>::as_wide(&int_atom), int_atom.as_int());
        assert_eq!(<i64 as WideNumeric>::as_wide(&float_atom), None);
        assert_eq!(<i64 as WideNumeric>::as_wide(&bool_atom), None);

        // Float axis: accepts both a float atom AND an int atom
        // byte-for-byte identical to `Sexp::as_float` (the `Number`
        // shape is the union of int + float atoms — a `:threshold 1`
        // field must accept both `1` and `1.0` per the pre-existing
        // `Sexp::as_float` contract); rejects the bool atom.
        let read_float = <f64 as WideNumeric>::as_wide(&float_atom).expect("float atom reads");
        assert!((read_float - 1.5_f64).abs() < f64::EPSILON);
        let read_int_as_float = <f64 as WideNumeric>::as_wide(&int_atom)
            .expect("int atom widens through Sexp::as_float");
        assert!((read_int_as_float - 8080.0_f64).abs() < f64::EPSILON);
        assert_eq!(
            <f64 as WideNumeric>::as_wide(&float_atom),
            float_atom.as_float()
        );
        assert_eq!(
            <f64 as WideNumeric>::as_wide(&int_atom),
            int_atom.as_float()
        );
        assert_eq!(<f64 as WideNumeric>::as_wide(&bool_atom), None);
    }

    /// The four public `extract_int` / `extract_optional_int` /
    /// `extract_float` / `extract_optional_float` extractors are now
    /// one-line delegates to the [`WideNumeric`] trait dispatch, so
    /// their verdicts MUST be byte-identical to the trait-method call
    /// they delegate to on every input (TOTAL, TYPE-MISMATCH with the
    /// same axis-typed `ExpectedKwargShape`, ABSENT). Pin the
    /// delegation identity at the operator-visible level: for every
    /// input each function accepts / rejects, the corresponding trait
    /// method must produce the SAME `Ok` value or the SAME
    /// `LispError::TypeMismatch` variant with the SAME
    /// `ExpectedKwargShape`. A regression that silently swapped one
    /// delegate's axis (`extract_int` accidentally routing through
    /// `<f64>::extract_kwarg`, so `:port "seven"` failed with the
    /// `Number` rejection label instead of `Int`) surfaces here as
    /// an axis-typed identity mismatch, not as a silent drift.
    #[test]
    fn extract_int_and_extract_float_delegate_to_the_wide_numeric_trait_dispatch() {
        // TOTAL — every axis on a matching literal.
        let int_args = kwargs_of("(_ :port 8080)");
        let int_kw = parse_kwargs(&int_args).unwrap();
        assert_eq!(
            extract_int(&int_kw, "port").unwrap(),
            <i64 as WideNumeric>::extract_kwarg(&int_kw, "port").unwrap(),
        );
        assert_eq!(
            extract_optional_int(&int_kw, "port").unwrap(),
            <i64 as WideNumeric>::extract_optional_kwarg(&int_kw, "port").unwrap(),
        );

        let float_args = kwargs_of("(_ :scale 1.5)");
        let float_kw = parse_kwargs(&float_args).unwrap();
        let via_delegate = extract_float(&float_kw, "scale").unwrap();
        let via_trait = <f64 as WideNumeric>::extract_kwarg(&float_kw, "scale").unwrap();
        assert!((via_delegate - via_trait).abs() < f64::EPSILON);
        let via_delegate_opt = extract_optional_float(&float_kw, "scale").unwrap();
        let via_trait_opt =
            <f64 as WideNumeric>::extract_optional_kwarg(&float_kw, "scale").unwrap();
        assert!(
            matches!((via_delegate_opt, via_trait_opt), (Some(a), Some(b)) if (a - b).abs() < f64::EPSILON),
        );

        // TYPE-MISMATCH — the delegate and the trait method both
        // fail with the SAME axis-typed `ExpectedKwargShape`.
        let bool_args = kwargs_of("(_ :n #f)");
        let bool_kw = parse_kwargs(&bool_args).unwrap();

        let int_delegate_err = extract_int(&bool_kw, "n").unwrap_err();
        let int_trait_err = <i64 as WideNumeric>::extract_kwarg(&bool_kw, "n").unwrap_err();
        let expected_from_delegate = match &int_delegate_err {
            LispError::TypeMismatch { expected, .. } => *expected,
            other => panic!("expected TypeMismatch on int delegate, got {other:?}"),
        };
        let expected_from_trait = match &int_trait_err {
            LispError::TypeMismatch { expected, .. } => *expected,
            other => panic!("expected TypeMismatch on int trait, got {other:?}"),
        };
        assert_eq!(expected_from_delegate, ExpectedKwargShape::Int);
        assert_eq!(expected_from_delegate, expected_from_trait);

        let float_delegate_err = extract_float(&bool_kw, "n").unwrap_err();
        let float_trait_err = <f64 as WideNumeric>::extract_kwarg(&bool_kw, "n").unwrap_err();
        let expected_from_delegate = match &float_delegate_err {
            LispError::TypeMismatch { expected, .. } => *expected,
            other => panic!("expected TypeMismatch on float delegate, got {other:?}"),
        };
        let expected_from_trait = match &float_trait_err {
            LispError::TypeMismatch { expected, .. } => *expected,
            other => panic!("expected TypeMismatch on float trait, got {other:?}"),
        };
        assert_eq!(expected_from_delegate, ExpectedKwargShape::Number);
        assert_eq!(expected_from_delegate, expected_from_trait);

        // ABSENT — the optional delegates short-circuit to the same
        // `Ok(None)` as the trait's optional method.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        assert_eq!(
            extract_optional_int(&absent_kw, "missing").unwrap(),
            <i64 as WideNumeric>::extract_optional_kwarg(&absent_kw, "missing").unwrap(),
        );
        assert_eq!(
            extract_optional_float(&absent_kw, "missing").unwrap(),
            <f64 as WideNumeric>::extract_optional_kwarg(&absent_kw, "missing").unwrap(),
        );
    }

    /// The narrowing rejection primitive `narrow_or_range_err` binds
    /// `T::WIDTH` on the NARROW type parameter AND `wide.as_literal()`
    /// on the WIDE type parameter through ONE trait dispatch — so the
    /// diagnostic's `(target, value)` pair is coherent by construction
    /// (`NumericWidth::U16` alongside `NumericLiteral::Int(70_000)`,
    /// never `NumericLiteral::Float(70_000.0)`). Pin both axes: the
    /// `port 70000` case on the int column reaches `narrow_or_range_
    /// err::<i64, u16>` with the two identities the LispError variant
    /// carries; the `scale 1.0e300` case on the float column reaches
    /// `narrow_or_range_err::<f64, f32>` with the peer identities on
    /// the peer axis. Pre-lift the two rejections lived at DIFFERENT
    /// per-site literal-constructor spellings; post-lift they share
    /// ONE primitive whose axis identity rides the type parameter.
    #[test]
    fn narrow_or_range_err_lifts_the_wide_literal_wrap_out_of_the_four_extractors() {
        let int_err: LispError = narrow_or_range_err::<i64, u16>("port", 70_000)
            .expect_err("70000 is out of range for u16");
        let LispError::KwargOutOfRange {
            form,
            target,
            value,
        } = &int_err
        else {
            panic!("expected KwargOutOfRange, got {int_err:?}");
        };
        assert_eq!(form, &KwargPath::named("port"));
        assert_eq!(*target, NumericWidth::U16);
        assert_eq!(*value, NumericLiteral::Int(70_000));

        let float_err: LispError =
            narrow_or_range_err::<f64, f32>("scale", 1.0e300).expect_err("1.0e300 overflows f32");
        let LispError::KwargOutOfRange { target, value, .. } = &float_err else {
            panic!("expected KwargOutOfRange, got {float_err:?}");
        };
        assert_eq!(*target, NumericWidth::F32);
        assert!(
            matches!(value, NumericLiteral::Float(x) if (*x - 1.0e300).abs() < f64::EPSILON),
            "the diagnostic must echo the author's own literal, got {value:?}",
        );

        // The in-range path stays total — a narrowing primitive that
        // rejected valid input would be worse than the truncation it
        // replaced.
        let ok_int: u16 =
            narrow_or_range_err::<i64, u16>("port", 8080).expect("in-range int narrows through");
        assert_eq!(ok_int, 8080);
        let ok_float: f32 =
            narrow_or_range_err::<f64, f32>("scale", 1.0).expect("in-range float narrows through");
        assert!((ok_float - 1.0_f32).abs() < f32::EPSILON);
    }

    /// The KwargPath-parameterized narrowing gate
    /// [`narrow_or_range_mismatch`] is the axis-symmetric sibling of
    /// [`type_mismatch`] on the narrowing composition layer — the SAME
    /// posture the neighbouring [`range_mismatch`] primitive already
    /// takes on the [`LispError::KwargOutOfRange`] STRUCT-LITERAL
    /// construction layer, one level down. Pre-lift the `T::narrow`
    /// composition lived inline at TWO sites — [`narrow_or_range_err`]
    /// (the `KwargPath::Named(key)` scalar-kwarg wrapper, consumed by
    /// [`extract_narrowed`]) and [`narrow_or_range_err_at`] (the
    /// `KwargPath::Item { key, idx }` per-item wrapper, consumed by
    /// [`extract_narrowed_list`]) — each restating the same
    /// `T::narrow(wide).ok_or_else(|| range_err_*(…, T::WIDTH,
    /// wide.as_literal()))` triple. Post-lift the two are one-line
    /// delegates that differ ONLY in the [`KwargPath`] they hand this
    /// primitive; the narrowing composition lives at ONE named site.
    ///
    /// Pin every axis the primitive owns against both wrappers plus
    /// the third path shape its public surface admits:
    ///   1. `KwargPath::Named` path — feeding `kwarg_form("port")`
    ///      yields the SAME rejection [`narrow_or_range_err`]
    ///      constructs (byte-identical `form` / `target` / `value`).
    ///   2. `KwargPath::Item` path — feeding `kwarg_item_form("ports",
    ///      1)` yields the SAME rejection [`narrow_or_range_err_at`]
    ///      constructs.
    ///   3. `KwargPath::Slot` path — feeding `kwargs_pos_form(2)`
    ///      yields a `KwargOutOfRange` with a `KwargPath::Slot(2)`
    ///      form (the not-yet-keyed-slot path the two wrappers do not
    ///      thread today, but the primitive's public surface admits —
    ///      same posture [`range_mismatch`] / [`type_mismatch`] admit
    ///      `kwargs_pos_form` at their axes).
    ///   4. Wide-axis coherence — the `T: NarrowNumeric<W>` /
    ///      `W: WideNumeric` bound pair pins both the narrowing partial
    ///      function and the wide-into-literal wrap on the SAME `W`, so
    ///      the diagnostic's `(target, value)` pair is coherent by
    ///      construction (`NumericWidth::U16` alongside
    ///      `NumericLiteral::Int(_)`, `NumericWidth::F32` alongside
    ///      `NumericLiteral::Float(_)`, never the cross-axis swap).
    ///   5. In-range totality — the narrowing partial function stays
    ///      total on both axes; a primitive that rejected valid input
    ///      would be worse than the truncation it replaced. Pin one
    ///      in-range narrow on each axis (`8080_i64 → u16`, `1.0_f64
    ///      → f32`) against both scalar and item paths.
    ///
    /// A regression that drifted [`narrow_or_range_err`] /
    /// [`narrow_or_range_err_at`] from the primitive (or hand-rolled a
    /// THIRD `T::narrow(wide).ok_or_else(|| …)` composition somewhere
    /// in the substrate) would register as a byte-difference between
    /// the primitive's output and the wrappers' outputs at this test —
    /// the SAME safety net [`range_mismatch`]'s sibling test carries
    /// on the struct-literal construction layer one level down.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the primitive is the ONE substrate entry both wrappers route
    /// through. THEORY.md §II.1 invariant 3 (typed exit — the
    /// narrowing boundary lives at ONE primitive whose failure locus
    /// is the [`KwargPath`] argument, not a per-wrapper path-builder
    /// choice). THEORY.md §V.1 — knowable platform; the axis-typed
    /// `(target, value)` payload rides ONE primitive so a future
    /// span-carrying promotion of [`KwargPath`] lands at ONE site.
    #[test]
    fn narrow_or_range_mismatch_binds_wrappers_and_the_typed_payload_to_one_substrate_entry() {
        fn parts(err: &LispError) -> (&KwargPath, NumericWidth, NumericLiteral) {
            let LispError::KwargOutOfRange {
                form,
                target,
                value,
            } = err
            else {
                panic!("expected KwargOutOfRange, got {err:?}");
            };
            (form, *target, *value)
        }

        // (1) `KwargPath::Named` — scalar-kwarg wrapper delegates.
        let via_primitive: Result<u16> =
            narrow_or_range_mismatch::<i64, u16>(kwarg_form("port"), 70_000);
        let via_wrapper: Result<u16> = narrow_or_range_err::<i64, u16>("port", 70_000);
        let (p_form, p_target, p_value) = parts(via_primitive.as_ref().unwrap_err());
        let (w_form, w_target, w_value) = parts(via_wrapper.as_ref().unwrap_err());
        assert_eq!(p_form, w_form);
        assert_eq!(p_target, w_target);
        assert_eq!(p_value, w_value);
        assert_eq!(p_form, &KwargPath::named("port"));
        assert_eq!(p_target, NumericWidth::U16);
        assert_eq!(p_value, NumericLiteral::Int(70_000));

        // (2) `KwargPath::Item` — per-item wrapper delegates.
        let via_primitive_at: Result<u16> =
            narrow_or_range_mismatch::<i64, u16>(kwarg_item_form("ports", 1), 70_000);
        let via_wrapper_at: Result<u16> = narrow_or_range_err_at::<i64, u16>("ports", 1, 70_000);
        let (p_form, p_target, p_value) = parts(via_primitive_at.as_ref().unwrap_err());
        let (w_form, w_target, w_value) = parts(via_wrapper_at.as_ref().unwrap_err());
        assert_eq!(p_form, w_form);
        assert_eq!(p_target, w_target);
        assert_eq!(p_value, w_value);
        assert_eq!(p_form, &KwargPath::item("ports", 1));

        // (3) `KwargPath::Slot` — the not-yet-keyed-slot path the two
        //     wrappers do not thread today, admitted by the primitive's
        //     public surface (mirrors `range_mismatch` / `type_mismatch`).
        let via_slot: Result<u16> =
            narrow_or_range_mismatch::<i64, u16>(kwargs_pos_form(2), 70_000);
        let (form, target, value) = parts(via_slot.as_ref().unwrap_err());
        assert_eq!(form, &KwargPath::Slot(2));
        assert_eq!(target, NumericWidth::U16);
        assert_eq!(value, NumericLiteral::Int(70_000));

        // (4) Wide-axis coherence — the float axis rides
        //     `NumericLiteral::Float` verbatim, no coercion to `Int`.
        let float_via_primitive: Result<f32> =
            narrow_or_range_mismatch::<f64, f32>(kwarg_form("scale"), 1.0e300);
        let float_via_wrapper: Result<f32> = narrow_or_range_err::<f64, f32>("scale", 1.0e300);
        let (p_form, p_target, p_value) = parts(float_via_primitive.as_ref().unwrap_err());
        let (w_form, w_target, w_value) = parts(float_via_wrapper.as_ref().unwrap_err());
        assert_eq!(p_form, w_form);
        assert_eq!(p_target, w_target);
        assert_eq!(p_target, NumericWidth::F32);
        assert!(
            matches!(p_value, NumericLiteral::Float(x) if (x - 1.0e300).abs() < f64::EPSILON),
            "float-axis payload must ride NumericLiteral::Float verbatim, got {p_value:?}",
        );
        assert!(
            matches!(w_value, NumericLiteral::Float(_)),
            "wrapper must also route through the Float variant, got {w_value:?}",
        );

        // (5) In-range totality holds through the primitive on both
        //     axes and both path shapes.
        let ok_int_named: u16 = narrow_or_range_mismatch::<i64, u16>(kwarg_form("port"), 8080)
            .expect("in-range int narrows through the primitive at the scalar path");
        assert_eq!(ok_int_named, 8080);
        let ok_int_item: u16 =
            narrow_or_range_mismatch::<i64, u16>(kwarg_item_form("ports", 0), 8080)
                .expect("in-range int narrows through the primitive at the per-item path");
        assert_eq!(ok_int_item, 8080);
        let ok_float_named: f32 = narrow_or_range_mismatch::<f64, f32>(kwarg_form("scale"), 1.0)
            .expect("in-range float narrows through the primitive at the scalar path");
        assert!((ok_float_named - 1.0_f32).abs() < f32::EPSILON);
    }

    /// The KwargPath-parameterized [`range_mismatch`] primitive is
    /// the axis-symmetric sibling of [`type_mismatch`] on the
    /// [`LispError::KwargOutOfRange`] axis. Pre-lift the struct-
    /// literal `LispError::KwargOutOfRange { form, target, value }`
    /// lived inline at TWO sites in `domain.rs` — the scalar-kwarg
    /// [`range_err`] wrapper (composing `kwarg_form(key)` into the
    /// slot) and the per-item [`range_err_at`] wrapper (composing
    /// `kwarg_item_form(key, idx)` into the slot). Post-lift both
    /// wrappers delegate through the KwargPath-parameterized primitive
    /// and the struct-literal construction lives at ONE named
    /// substrate entry.
    ///
    /// Pin every rejection axis the primitive owns:
    ///   1. `KwargPath::Named` path — feeding `kwarg_form("port")`
    ///      yields the SAME variant `range_err("port", U16, Int(70000))`
    ///      constructs.
    ///   2. `KwargPath::Item` path — feeding `kwarg_item_form("ports",
    ///      1)` yields the SAME variant `range_err_at("ports", 1, U16,
    ///      Int(70000))` constructs.
    ///   3. `KwargPath::Slot` path — feeding `kwargs_pos_form(3)`
    ///      yields a `KwargOutOfRange` with a `KwargPath::Slot(3)`
    ///      form slot (the not-yet-keyed-slot path the wrappers do
    ///      not thread today, but the primitive's public surface
    ///      admits — same posture the sibling [`type_mismatch`]
    ///      admits `kwargs_pos_form` at the shape-mismatch axis).
    ///   4. Axis-typed payload — the `target: NumericWidth` and
    ///      `value: NumericLiteral` carriers are stored verbatim; a
    ///      regression that dropped one field or reordered them would
    ///      fail the struct-destructure assertion.
    ///
    /// A regression that drifted [`range_err`] / [`range_err_at`] from
    /// the primitive (or hand-rolled a THIRD `LispError::KwargOutOfRange
    /// { ... }` struct-literal somewhere in the substrate) would
    /// register as a byte-difference between the primitive's output
    /// and the wrappers' outputs at this test — the SAME safety net
    /// [`type_mismatch`]'s sibling test suite already carries for the
    /// TypeMismatch axis.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the primitive is the ONE substrate entry both wrappers route
    /// through. THEORY.md §V.1 — knowable platform; the axis-typed
    /// `(target, value)` payload rides ONE primitive so a future
    /// span-carrying promotion of [`KwargPath`] lands at ONE site
    /// and every consumer inherits mechanically.
    #[test]
    fn range_mismatch_binds_wrappers_and_the_typed_payload_to_one_substrate_entry() {
        // Helper: pattern-match against `LispError::KwargOutOfRange`
        // and return the three typed slots by value. `LispError` does
        // not derive `PartialEq` (the `Sexp` payload in
        // [`LispError::TypeMismatch`]-family variants precludes it in
        // general), so the test compares the STRUCTURAL slots rather
        // than the variants directly.
        fn parts(err: &LispError) -> (&KwargPath, NumericWidth, NumericLiteral) {
            let LispError::KwargOutOfRange {
                form,
                target,
                value,
            } = err
            else {
                panic!("expected KwargOutOfRange, got {err:?}");
            };
            (form, *target, *value)
        }

        // (1) `KwargPath::Named` — the scalar-kwarg path
        //     `range_err("port", ...)` composes.
        let via_primitive = range_mismatch(
            kwarg_form("port"),
            NumericWidth::U16,
            NumericLiteral::Int(70_000),
        );
        let via_wrapper = range_err("port", NumericWidth::U16, NumericLiteral::Int(70_000));
        let (p_form, p_target, p_value) = parts(&via_primitive);
        let (w_form, w_target, w_value) = parts(&via_wrapper);
        assert_eq!(p_form, w_form);
        assert_eq!(p_target, w_target);
        assert_eq!(p_value, w_value);
        assert_eq!(p_form, &KwargPath::named("port"));
        assert_eq!(p_target, NumericWidth::U16);
        assert_eq!(p_value, NumericLiteral::Int(70_000));

        // (2) `KwargPath::Item` — the per-item path
        //     `range_err_at("ports", 1, ...)` composes.
        let via_primitive_at = range_mismatch(
            kwarg_item_form("ports", 1),
            NumericWidth::U16,
            NumericLiteral::Int(70_000),
        );
        let via_wrapper_at =
            range_err_at("ports", 1, NumericWidth::U16, NumericLiteral::Int(70_000));
        let (p_form, p_target, p_value) = parts(&via_primitive_at);
        let (w_form, w_target, w_value) = parts(&via_wrapper_at);
        assert_eq!(p_form, w_form);
        assert_eq!(p_target, w_target);
        assert_eq!(p_value, w_value);
        assert_eq!(p_form, &KwargPath::item("ports", 1));

        // (3) `KwargPath::Slot` — the not-yet-keyed-slot path the
        //     primitive's public surface admits by design (mirrors
        //     `type_mismatch`'s admission of `kwargs_pos_form`).
        let via_slot = range_mismatch(
            kwargs_pos_form(3),
            NumericWidth::I32,
            NumericLiteral::Int(-1),
        );
        let (form, target, value) = parts(&via_slot);
        assert_eq!(form, &KwargPath::Slot(3));
        assert_eq!(target, NumericWidth::I32);
        assert_eq!(value, NumericLiteral::Int(-1));

        // (4) Float-axis payload variant — the primitive stores the
        //     `NumericLiteral::Float` carrier verbatim, no coercion to
        //     `Int`. A regression that swapped the payload's variant
        //     (an int-axis rejection accidentally storing the value
        //     as `Float`) would surface here.
        let float_via_primitive = range_mismatch(
            kwarg_form("scale"),
            NumericWidth::F32,
            NumericLiteral::Float(1.0e300),
        );
        let (_, _, value) = parts(&float_via_primitive);
        assert!(
            matches!(value, NumericLiteral::Float(x) if (x - 1.0e300).abs() < f64::EPSILON),
            "float-axis payload carrier must ride NumericLiteral::Float verbatim, got {value:?}",
        );
    }

    /// `WideNumeric::extract_kwarg` / `WideNumeric::extract_optional_
    /// kwarg` are the wide-axis kwarg extractor pinned as trait
    /// methods dispatched on the wide type itself, so the axis
    /// identity rides ONE trait dispatch rather than a hand-written
    /// `extract_int` / `extract_float` call at each of the four
    /// `extract_*_narrowed` extractors. Pin both axes on the wide
    /// extractor's THREE promises: (1) the total path echoes the
    /// axis's `extract_atom`-based projection unchanged (an in-range
    /// integer literal reads back as the same `i64` on `<i64 as
    /// WideNumeric>::extract_kwarg` and as the same `f64` on
    /// `<f64 as WideNumeric>::extract_kwarg`), (2) the axis-typed
    /// [`ExpectedKwargShape`] rides the rejection unchanged on a
    /// type-mismatch (`ExpectedKwargShape::Int` on `<i64 as
    /// WideNumeric>` — a regression that silently rerouted through
    /// `extract_float` would widen the rejection to
    /// `ExpectedKwargShape::Number` and let float literals through
    /// on integer-typed fields), and (3) the `Option` sibling
    /// short-circuits an absent kwarg to `Ok(None)` rather than
    /// treating it as an error. Pre-lift the axis identity was
    /// per-site hand-written across the four narrowed extractors;
    /// post-lift a regression that silently swapped an int-axis
    /// extractor for a float-axis extractor inside one narrowed
    /// extractor's body is unconstructible — the trait's per-axis
    /// impls forbid the cross-axis combination at rustc time.
    #[test]
    fn wide_numeric_extract_kwarg_pins_the_per_axis_wide_extractor_at_the_trait_dispatch() {
        // (1) TOTAL path — the wide extractor round-trips the
        //     author's literal at every axis unchanged.
        let int_args = kwargs_of("(_ :port 8080)");
        let int_kw = parse_kwargs(&int_args).unwrap();
        assert_eq!(
            <i64 as WideNumeric>::extract_kwarg(&int_kw, "port").unwrap(),
            8080_i64,
        );
        assert_eq!(
            <i64 as WideNumeric>::extract_optional_kwarg(&int_kw, "port").unwrap(),
            Some(8080_i64),
        );

        let float_args = kwargs_of("(_ :scale 1.5)");
        let float_kw = parse_kwargs(&float_args).unwrap();
        let read_float = <f64 as WideNumeric>::extract_kwarg(&float_kw, "scale").unwrap();
        assert!((read_float - 1.5_f64).abs() < f64::EPSILON);
        let read_optional_float =
            <f64 as WideNumeric>::extract_optional_kwarg(&float_kw, "scale").unwrap();
        assert!(
            matches!(read_optional_float, Some(x) if (x - 1.5_f64).abs() < f64::EPSILON),
            "optional float axis round-trips the wide value unchanged",
        );

        // (2) TYPE-MISMATCH path — the axis-typed
        //     `ExpectedKwargShape` rides the rejection identity.
        //     A regression that silently rerouted `<i64 as
        //     WideNumeric>::extract_kwarg` through `extract_float`
        //     would surface here as `ExpectedKwargShape::Number`.
        let bool_args = kwargs_of("(_ :n #f)");
        let bool_kw = parse_kwargs(&bool_args).unwrap();
        let int_type_mismatch = <i64 as WideNumeric>::extract_kwarg(&bool_kw, "n").unwrap_err();
        assert!(
            matches!(
                &int_type_mismatch,
                LispError::TypeMismatch { expected, .. }
                    if *expected == ExpectedKwargShape::Int,
            ),
            "int axis's ExpectedKwargShape is Int, got {int_type_mismatch:?}",
        );

        let float_type_mismatch = <f64 as WideNumeric>::extract_kwarg(&bool_kw, "n").unwrap_err();
        assert!(
            matches!(
                &float_type_mismatch,
                LispError::TypeMismatch { expected, .. }
                    if *expected == ExpectedKwargShape::Number,
            ),
            "float axis's ExpectedKwargShape is Number, got {float_type_mismatch:?}",
        );

        // (3) ABSENT path on the optional wide extractor — an absent
        //     kwarg short-circuits to `Ok(None)`, not an error.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        assert_eq!(
            <i64 as WideNumeric>::extract_optional_kwarg(&absent_kw, "missing").unwrap(),
            None,
        );
        assert_eq!(
            <f64 as WideNumeric>::extract_optional_kwarg(&absent_kw, "missing").unwrap(),
            None,
        );
    }

    /// The [`AtomKwarg`] trait bundles the two per-atom primitives
    /// (`SHAPE` + `project`) every non-numeric atom extractor used
    /// to spell inline through [`extract_atom`] /
    /// [`extract_optional_atom`]. Pin the two per-axis primitives
    /// AND their default composition at the trait dispatch level so
    /// a regression that silently swapped one axis's SHAPE or
    /// projection is a rustc-time failure, not a runtime drift:
    ///
    /// (1) `SHAPE` per axis — the associated const's type pins the
    ///     axis-typed rejection label. A regression that swapped
    ///     `<&str>::SHAPE = Bool` (so `:name 5` reported the wrong
    ///     expected shape) would fail to compile the impl's const
    ///     clause.
    ///
    /// (2) `project` per axis — the per-atom projection lifted out
    ///     of `extract_string` / `extract_bool` bodies. `<&str>::
    ///     project` MUST be byte-identical to [`Sexp::as_string`]
    ///     and `<bool>::project` byte-identical to [`Sexp::as_bool`]
    ///     on every atom shape. A regression that silently rerouted
    ///     `<&str>::project` through `Sexp::as_bool` (so `:name #t`
    ///     landed as `"true"` in a `String` field) surfaces here as
    ///     a per-atom mismatch.
    ///
    /// (3) The trait DEFAULT `extract_kwarg` /
    ///     `extract_optional_kwarg` — composes `(Self::SHAPE,
    ///     Self::project)` through the shared atom-family
    ///     skeleton. Its verdict on TOTAL / TYPE-MISMATCH / ABSENT
    ///     inputs MUST match a hand-rolled `extract_atom(kw, key,
    ///     Self::SHAPE, Self::project)` call byte-for-byte at both
    ///     axes.
    #[test]
    fn atom_kwarg_shape_and_project_are_the_two_per_axis_primitives_the_extractor_default_composes()
    {
        // (1) SHAPE per axis.
        assert_eq!(<&str as AtomKwarg<'_>>::SHAPE, ExpectedKwargShape::String);
        assert_eq!(<bool as AtomKwarg<'_>>::SHAPE, ExpectedKwargShape::Bool);

        // (2) project per axis — byte-identical to the pre-lift
        //     `Sexp::as_string` / `Sexp::as_bool` calls at every
        //     atom shape (accept its own, reject the other axes).
        let string_atom = Sexp::Atom(Atom::Str("hello".to_string()));
        let bool_atom = Sexp::Atom(Atom::Bool(true));
        let int_atom = Sexp::Atom(Atom::Int(5));

        assert_eq!(
            <&str as AtomKwarg<'_>>::project(&string_atom),
            string_atom.as_string(),
        );
        assert_eq!(<&str as AtomKwarg<'_>>::project(&bool_atom), None);
        assert_eq!(<&str as AtomKwarg<'_>>::project(&int_atom), None);

        assert_eq!(
            <bool as AtomKwarg<'_>>::project(&bool_atom),
            bool_atom.as_bool(),
        );
        assert_eq!(<bool as AtomKwarg<'_>>::project(&string_atom), None);
        assert_eq!(<bool as AtomKwarg<'_>>::project(&int_atom), None);
    }

    /// The four public `extract_string` / `extract_optional_string`
    /// / `extract_bool` / `extract_optional_bool` extractors are
    /// now one-line delegates to the [`AtomKwarg`] trait dispatch,
    /// so their verdicts MUST be byte-identical to the trait-method
    /// call they delegate to on every input (TOTAL, TYPE-MISMATCH
    /// with the same axis-typed `ExpectedKwargShape`, ABSENT on the
    /// optional peer). Pin the delegation identity at the operator-
    /// visible level: for every input each function accepts /
    /// rejects, the corresponding trait method must produce the
    /// SAME `Ok` value or the SAME `LispError::TypeMismatch`
    /// variant with the SAME `ExpectedKwargShape`. A regression
    /// that silently swapped one delegate's axis (`extract_string`
    /// accidentally routing through `<bool>::extract_kwarg`, so
    /// `:name "seven"` failed with the `Bool` rejection label
    /// instead of `String`) surfaces here as an axis-typed identity
    /// mismatch, not as a silent drift in the operator diagnostic.
    #[test]
    fn extract_string_and_extract_bool_delegate_to_the_atom_kwarg_trait_dispatch() {
        // (1) TOTAL path — each public wrapper round-trips the
        //     author's atom unchanged, identical to the trait
        //     dispatch.
        let string_args = kwargs_of("(_ :name \"alice\")");
        let string_kw = parse_kwargs(&string_args).unwrap();
        assert_eq!(extract_string(&string_kw, "name").unwrap(), "alice");
        assert_eq!(
            <&str as AtomKwarg<'_>>::extract_kwarg(&string_kw, "name").unwrap(),
            extract_string(&string_kw, "name").unwrap(),
        );
        assert_eq!(
            <&str as AtomKwarg<'_>>::extract_optional_kwarg(&string_kw, "name").unwrap(),
            extract_optional_string(&string_kw, "name").unwrap(),
        );

        let bool_args = kwargs_of("(_ :enabled #t)");
        let bool_kw = parse_kwargs(&bool_args).unwrap();
        assert!(extract_bool(&bool_kw, "enabled").unwrap());
        assert_eq!(
            <bool as AtomKwarg<'_>>::extract_kwarg(&bool_kw, "enabled").unwrap(),
            extract_bool(&bool_kw, "enabled").unwrap(),
        );
        assert_eq!(
            <bool as AtomKwarg<'_>>::extract_optional_kwarg(&bool_kw, "enabled").unwrap(),
            extract_optional_bool(&bool_kw, "enabled").unwrap(),
        );

        // (2) TYPE-MISMATCH path — the axis-typed
        //     `ExpectedKwargShape` rides the rejection identity.
        //     Feeding the wrong atom shape at each axis MUST land
        //     with the axis's own `SHAPE`; a swapped delegate
        //     would surface here as the wrong axis's label.
        let cross_args = kwargs_of("(_ :name 5 :enabled \"maybe\")");
        let cross_kw = parse_kwargs(&cross_args).unwrap();

        let string_mismatch = extract_string(&cross_kw, "name").unwrap_err();
        let string_trait_mismatch =
            <&str as AtomKwarg<'_>>::extract_kwarg(&cross_kw, "name").unwrap_err();
        assert!(
            matches!(
                &string_mismatch,
                LispError::TypeMismatch { expected, .. }
                    if *expected == ExpectedKwargShape::String,
            ),
            "string axis's ExpectedKwargShape is String, got {string_mismatch:?}",
        );
        assert!(
            matches!(
                &string_trait_mismatch,
                LispError::TypeMismatch { expected, .. }
                    if *expected == ExpectedKwargShape::String,
            ),
            "trait dispatch must match wrapper axis, got {string_trait_mismatch:?}",
        );

        let bool_mismatch = extract_bool(&cross_kw, "enabled").unwrap_err();
        let bool_trait_mismatch =
            <bool as AtomKwarg<'_>>::extract_kwarg(&cross_kw, "enabled").unwrap_err();
        assert!(
            matches!(
                &bool_mismatch,
                LispError::TypeMismatch { expected, .. }
                    if *expected == ExpectedKwargShape::Bool,
            ),
            "bool axis's ExpectedKwargShape is Bool, got {bool_mismatch:?}",
        );
        assert!(
            matches!(
                &bool_trait_mismatch,
                LispError::TypeMismatch { expected, .. }
                    if *expected == ExpectedKwargShape::Bool,
            ),
            "trait dispatch must match wrapper axis, got {bool_trait_mismatch:?}",
        );

        // (3) ABSENT path on the optional wrapper — an absent
        //     kwarg short-circuits to `Ok(None)`, matched by the
        //     trait's optional default. The required wrapper
        //     rejects with `MissingRequired`; the trait's required
        //     default MUST reject in the same shape.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        assert_eq!(
            extract_optional_string(&absent_kw, "missing").unwrap(),
            None
        );
        assert_eq!(
            <&str as AtomKwarg<'_>>::extract_optional_kwarg(&absent_kw, "missing").unwrap(),
            None,
        );
        assert_eq!(extract_optional_bool(&absent_kw, "missing").unwrap(), None);
        assert_eq!(
            <bool as AtomKwarg<'_>>::extract_optional_kwarg(&absent_kw, "missing").unwrap(),
            None,
        );

        let required_string_missing = extract_string(&absent_kw, "missing").unwrap_err();
        let trait_string_missing =
            <&str as AtomKwarg<'_>>::extract_kwarg(&absent_kw, "missing").unwrap_err();
        assert!(matches!(
            &required_string_missing,
            LispError::MissingKwarg { .. },
        ));
        assert!(matches!(
            &trait_string_missing,
            LispError::MissingKwarg { .. },
        ));

        let required_bool_missing = extract_bool(&absent_kw, "missing").unwrap_err();
        let trait_bool_missing =
            <bool as AtomKwarg<'_>>::extract_kwarg(&absent_kw, "missing").unwrap_err();
        assert!(matches!(
            &required_bool_missing,
            LispError::MissingKwarg { .. },
        ));
        assert!(matches!(
            &trait_bool_missing,
            LispError::MissingKwarg { .. },
        ));
    }

    /// The atom-family per-item shape gate lives on ONE owner —
    /// [`AtomKwarg::project_at`] — for every atom axis the substrate
    /// carries: the two non-numeric axes ([`&'a str`], [`bool`]) that
    /// close the shared `SHAPE + project` primitive pair directly, and
    /// the two numeric axes ([`i64`], [`f64`]) that inherit the same
    /// gate through the [`WideNumeric: for<'a> AtomKwarg<'a>`]
    /// supertrait bound. Pre-lift TWO per-item projection sites
    /// (`extract_string_list`'s per-item body on the string axis,
    /// `extract_narrowed_list`'s per-item body on the two numeric
    /// axes) inlined the compose shape `Self::project(s).ok_or_else(||
    /// type_err_at(key, idx, Self::SHAPE, s))`, each spelling its
    /// axis's `SHAPE` and `project` pair by hand; post-lift both
    /// sites route through this ONE trait default, with the axis
    /// identity riding the `Self` type parameter through the SAME
    /// atom-family shape-gate composition.
    ///
    /// Pin FOUR promises the gate owns at its emission boundary:
    ///
    /// (1) TOTAL at every atom axis — `<T as AtomKwarg<'_>>::project_at
    ///     (key, idx, sexp)` returns `Ok(v)` on a shape-match at every
    ///     supported axis (str, bool, i64, f64), byte-identical to the
    ///     scalar `Self::project` per-atom projection wrapped in `Ok`.
    /// (2) SHAPE-ERR at every atom axis — `<T as AtomKwarg<'_>>::
    ///     project_at(key, idx, sexp)` on a shape-mismatch rejects
    ///     with `LispError::TypeMismatch { form: Item { key, idx },
    ///     expected: Self::SHAPE, got: <sexp shape> }` — the exact
    ///     variant every per-item shape rejection surface
    ///     (`extract_string_list`, `extract_narrowed_list`,
    ///     `extract_vec_via_serde` outer) pattern-matches on.
    /// (3) EXTRACTOR IDENTITY — the two atom-family per-item list
    ///     extractors (`extract_string_list`, `extract_narrowed_list`)
    ///     that route through the gate emit the SAME
    ///     `LispError::TypeMismatch` variant on a per-item shape
    ///     mismatch as a direct `<T as AtomKwarg<'_>>::project_at`
    ///     call at the same axis + key + idx. A regression that
    ///     silently reintroduced an inline `type_err_at(key, idx,
    ///     ExpectedKwargShape::String, s)` body on the extractor
    ///     side, restating the composition alongside the trait
    ///     default's, surfaces here as a variant-identity mismatch
    ///     between the two routes.
    /// (4) AXIS IDENTITY — the `<W as AtomKwarg<'_>>::project_at`
    ///     path at the two numeric axes rides the numeric-axis
    ///     `SHAPE` (`ExpectedKwargShape::Int` on `<i64>`,
    ///     `ExpectedKwargShape::Number` on `<f64>`) through the
    ///     supertrait's shape gate, on the SAME footing the two
    ///     non-numeric axes do. A regression that split the gate
    ///     per-axis-family (a numeric-only `WideNumeric::project_at`
    ///     alongside the atom-family `AtomKwarg::project_at`) would
    ///     surface here as axis identity drift.
    #[test]
    fn atom_kwarg_project_at_is_the_one_owner_of_the_atom_family_per_item_shape_gate() {
        let string_atom = Sexp::Atom(Atom::Str("hello".to_string()));
        let bool_atom = Sexp::Atom(Atom::Bool(true));
        let int_atom = Sexp::Atom(Atom::Int(42));
        let float_atom = Sexp::Atom(Atom::Float(2.5));

        // (1) TOTAL at every atom axis — accept own atom, byte-identical
        //     to the scalar Self::project projection lifted to Ok.
        assert_eq!(
            <&str as AtomKwarg<'_>>::project_at("xs", 0, &string_atom).unwrap(),
            <&str as AtomKwarg<'_>>::project(&string_atom).unwrap(),
        );
        assert_eq!(
            <bool as AtomKwarg<'_>>::project_at("flags", 3, &bool_atom).unwrap(),
            <bool as AtomKwarg<'_>>::project(&bool_atom).unwrap(),
        );
        assert_eq!(
            <i64 as AtomKwarg<'_>>::project_at("ports", 1, &int_atom).unwrap(),
            <i64 as AtomKwarg<'_>>::project(&int_atom).unwrap(),
        );
        // Int widens to Float via Sexp::as_float — same posture the
        // scalar Sexp::as_float consumer already gives on the int axis.
        assert_eq!(
            <f64 as AtomKwarg<'_>>::project_at("scales", 2, &float_atom).unwrap(),
            <f64 as AtomKwarg<'_>>::project(&float_atom).unwrap(),
        );

        // (2) SHAPE-ERR at every atom axis — sweep the four cells
        //     (str/bool/i64/f64) individually so each per-axis
        //     rejection identity is pinned at rustc time; the SHAPE
        //     label lifted rides the axis-Self through the trait
        //     dispatch and cannot silently swap.

        // (2a) The <&str> axis rejects an int atom with SHAPE::String.
        let string_axis_err = <&str as AtomKwarg<'_>>::project_at("xs", 1, &int_atom).unwrap_err();
        match string_axis_err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(
                    form,
                    KwargPath::Item {
                        key: "xs".into(),
                        idx: 1,
                    }
                );
                assert_eq!(expected, ExpectedKwargShape::String);
                assert_eq!(got, SexpShape::Int);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }

        // (2b) The <bool> axis rejects a string atom with SHAPE::Bool.
        let bool_axis_err =
            <bool as AtomKwarg<'_>>::project_at("flags", 0, &string_atom).unwrap_err();
        match bool_axis_err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(
                    form,
                    KwargPath::Item {
                        key: "flags".into(),
                        idx: 0,
                    }
                );
                assert_eq!(expected, ExpectedKwargShape::Bool);
                assert_eq!(got, SexpShape::String);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }

        // (2c) The <i64> axis inherits the shape gate through the
        //      WideNumeric supertrait — rejects a string atom with
        //      SHAPE::Int (the axis-typed label WideNumeric::SHAPE
        //      delegates to on the supertrait).
        let int_axis_err =
            <i64 as AtomKwarg<'_>>::project_at("ports", 2, &string_atom).unwrap_err();
        match int_axis_err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(
                    form,
                    KwargPath::Item {
                        key: "ports".into(),
                        idx: 2,
                    }
                );
                assert_eq!(expected, ExpectedKwargShape::Int);
                assert_eq!(got, SexpShape::String);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }

        // (2d) The <f64> axis inherits the shape gate the same way —
        //      rejects a bool atom with SHAPE::Number.
        let float_axis_err =
            <f64 as AtomKwarg<'_>>::project_at("scales", 4, &bool_atom).unwrap_err();
        match float_axis_err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(
                    form,
                    KwargPath::Item {
                        key: "scales".into(),
                        idx: 4,
                    }
                );
                assert_eq!(expected, ExpectedKwargShape::Number);
                assert_eq!(got, SexpShape::Bool);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }

        // (3) EXTRACTOR IDENTITY — extract_string_list's per-item
        //     rejection at a non-string element MUST match the direct
        //     <&str as AtomKwarg>::project_at call at the same
        //     axis/key/idx byte-for-byte modulo the projected T.
        let mixed_args = kwargs_of(r#"(_ :xs ("ok" 9 "never"))"#);
        let mixed_kw = parse_kwargs(&mixed_args).unwrap();
        let list_err = extract_string_list(&mixed_kw, "xs").unwrap_err();
        let direct_err =
            <&str as AtomKwarg<'_>>::project_at("xs", 1, &Sexp::Atom(Atom::Int(9))).unwrap_err();
        match (&list_err, &direct_err) {
            (
                LispError::TypeMismatch {
                    form: form_a,
                    expected: exp_a,
                    got: got_a,
                },
                LispError::TypeMismatch {
                    form: form_b,
                    expected: exp_b,
                    got: got_b,
                },
            ) => {
                assert_eq!(form_a, form_b, "path variant identity");
                assert_eq!(exp_a, exp_b, "axis-typed SHAPE identity");
                assert_eq!(got_a, got_b, "actual-shape witness identity");
            }
            other => panic!("both routes must produce TypeMismatch, got {other:?}"),
        }

        // (3, numeric axis) — extract_narrowed_list's per-item
        //     rejection at a non-int element MUST match the direct
        //     <i64 as AtomKwarg>::project_at call at the same axis
        //     byte-for-byte. Distinct from the range-err path (Int(70000)
        //     rejects with KwargOutOfRange, not TypeMismatch); this
        //     pins the SHAPE-err arm alone.
        let bad_shape_args = kwargs_of(r#"(_ :ports (80 "nope"))"#);
        let bad_shape_kw = parse_kwargs(&bad_shape_args).unwrap();
        let narrowed_err = extract_narrowed_list::<i64, u16>(&bad_shape_kw, "ports").unwrap_err();
        let numeric_direct_err = <i64 as AtomKwarg<'_>>::project_at(
            "ports",
            1,
            &Sexp::Atom(Atom::Str("nope".to_string())),
        )
        .unwrap_err();
        match (&narrowed_err, &numeric_direct_err) {
            (
                LispError::TypeMismatch {
                    form: form_a,
                    expected: exp_a,
                    got: got_a,
                },
                LispError::TypeMismatch {
                    form: form_b,
                    expected: exp_b,
                    got: got_b,
                },
            ) => {
                assert_eq!(form_a, form_b);
                assert_eq!(exp_a, exp_b);
                assert_eq!(*exp_a, ExpectedKwargShape::Int);
                assert_eq!(got_a, got_b);
            }
            other => panic!("both numeric-axis routes must produce TypeMismatch, got {other:?}"),
        }

        // (4) AXIS IDENTITY — the numeric-axis project_at return
        //     value is byte-identical to the WideNumeric::as_wide
        //     projection lifted to Ok, so the shape gate does NOT
        //     drift between the two atom-family paths a numeric
        //     caller can reach through.
        assert_eq!(
            <i64 as AtomKwarg<'_>>::project_at("ports", 0, &int_atom).unwrap(),
            <i64 as WideNumeric>::as_wide(&int_atom).unwrap(),
        );
        assert_eq!(
            <f64 as AtomKwarg<'_>>::project_at("scales", 0, &float_atom).unwrap(),
            <f64 as WideNumeric>::as_wide(&float_atom).unwrap(),
        );
    }

    /// [`AtomKwarg::LIST_SHAPE`] is the per-axis outer-list rejection
    /// label the three list-family extractors ([`extract_string_list`]
    /// on `<&str>`, [`extract_bool_list`] on `<bool>`,
    /// [`extract_narrowed_list<W, T>`] on both wide-numeric axes
    /// through the `W: AtomKwarg` supertrait bound) bind through
    /// ONE per-axis trait const rather than a per-extractor inline
    /// literal. All four atom-family axes override the trait
    /// default: `<&str>` → [`ExpectedKwargShape::ListOfStrings`],
    /// `<bool>` → [`ExpectedKwargShape::ListOfBools`],
    /// `<i64>` → [`ExpectedKwargShape::ListOfInts`],
    /// `<f64>` → [`ExpectedKwargShape::ListOfNumbers`]. Post-lift
    /// every list-family extractor rejects a scalar-typed kwarg
    /// with an element-typed refinement label, on any reachable
    /// axis — the ambiguous bare-list default
    /// [`ExpectedKwargShape::List`] never surfaces from these
    /// extractors any more.
    ///
    /// Pin the axis-typed identity at the four impls: every axis
    /// carries its element-typed refinement variant. A regression
    /// that reverted one extractor to an inline literal (e.g.
    /// `extract_string_list(kw, key, ExpectedKwargShape::
    /// ListOfStrings, ...)` restated inline alongside the trait
    /// override) would still compile but silently break the
    /// single-source-of-truth contract this pin anchors; a
    /// regression that reverted ONE of the four axis overrides to
    /// the trait default surfaces here as the axis's rejection
    /// label degrading from the axis-typed refinement to the bare
    /// `List` label.
    #[test]
    fn atom_kwarg_list_shape_is_the_one_per_axis_outer_list_rejection_label() {
        // (1) Per-axis identity — all four axes carry their axis-
        //     typed element-typed refinement variant. Rustc-level
        //     const evaluation, so a regression that flipped any
        //     override back to the trait default fails to compile
        //     the enclosing arm.
        assert_eq!(
            <&str as AtomKwarg<'_>>::LIST_SHAPE,
            ExpectedKwargShape::ListOfStrings,
        );
        assert_eq!(
            <bool as AtomKwarg<'_>>::LIST_SHAPE,
            ExpectedKwargShape::ListOfBools,
        );
        assert_eq!(
            <i64 as AtomKwarg<'_>>::LIST_SHAPE,
            ExpectedKwargShape::ListOfInts,
        );
        assert_eq!(
            <f64 as AtomKwarg<'_>>::LIST_SHAPE,
            ExpectedKwargShape::ListOfNumbers,
        );

        // (2) Extractor identity — extract_string_list's outer-shape
        //     rejection at a scalar kwarg (`:xs "solo"`) MUST carry
        //     the SAME axis-typed label the trait const owns. A
        //     regression that restated the label inline alongside
        //     the trait override would drift the moment the closed
        //     set gained a per-axis refinement variant; this pin
        //     catches that today, before the drift can manifest.
        let string_args = kwargs_of(r#"(_ :xs "solo")"#);
        let string_kw = parse_kwargs(&string_args).unwrap();
        let string_err = extract_string_list(&string_kw, "xs").unwrap_err();
        match string_err {
            LispError::TypeMismatch { form, expected, .. } => {
                assert_eq!(form, KwargPath::named("xs"));
                assert_eq!(expected, <&str as AtomKwarg<'_>>::LIST_SHAPE);
                assert_eq!(expected, ExpectedKwargShape::ListOfStrings);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }

        // (3) Bool-axis extractor identity — extract_bool_list's
        //     outer-shape rejection at a scalar kwarg (`:flags #t`)
        //     carries the axis-typed refinement `ListOfBools` through
        //     the `<bool>::LIST_SHAPE` override, sibling to the
        //     `<&str>::LIST_SHAPE = ListOfStrings` override at case
        //     (2) — both non-numeric atom-family list-family axes
        //     now surface the axis-typed refinement label rather
        //     than the ambiguous trait default `List`.
        let bool_args = kwargs_of("(_ :flags #t)");
        let bool_kw = parse_kwargs(&bool_args).unwrap();
        let bool_err = extract_bool_list(&bool_kw, "flags").unwrap_err();
        match bool_err {
            LispError::TypeMismatch { form, expected, .. } => {
                assert_eq!(form, KwargPath::named("flags"));
                assert_eq!(expected, <bool as AtomKwarg<'_>>::LIST_SHAPE);
                assert_eq!(expected, ExpectedKwargShape::ListOfBools);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }

        // (4) Wide-int-axis extractor identity — extract_narrowed_list
        //     inherits `<W>::LIST_SHAPE` through the WideNumeric:
        //     AtomKwarg supertrait bound. Rejection at a scalar
        //     kwarg (`:ports 80`) into a narrowed Vec<u16> carries
        //     the axis-typed refinement `ListOfInts` through the
        //     `<i64>::LIST_SHAPE = ListOfInts` per-axis trait-const
        //     override; every narrow-int width (u16 / i32 / usize /
        //     …) picks up the sharpened outer-shape label
        //     mechanically at ONE trait override on the wide axis
        //     rather than at each narrow-width instantiation.
        let numeric_args = kwargs_of("(_ :ports 80)");
        let numeric_kw = parse_kwargs(&numeric_args).unwrap();
        let numeric_err = extract_narrowed_list::<i64, u16>(&numeric_kw, "ports").unwrap_err();
        match numeric_err {
            LispError::TypeMismatch { form, expected, .. } => {
                assert_eq!(form, KwargPath::named("ports"));
                assert_eq!(expected, <i64 as AtomKwarg<'_>>::LIST_SHAPE);
                assert_eq!(expected, ExpectedKwargShape::ListOfInts);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }

        // (5) Wide-float-axis extractor identity — same shape as (4)
        //     on the wide-float axis, carrying the peer refinement
        //     `ListOfNumbers` through the
        //     `<f64>::LIST_SHAPE = ListOfNumbers` override. The
        //     `"numbers"` naming (not `"floats"`) mirrors the
        //     scalar-axis Number vs Float posture — the per-item
        //     gate accepts BOTH float atoms and int atoms through
        //     Sexp::as_float, so the outer-shape label names the
        //     union rather than the narrower "floats". A regression
        //     that silently split `<i64>::LIST_SHAPE` from
        //     `<f64>::LIST_SHAPE` (e.g. by demoting one override to
        //     the trait default during a partial refactor) surfaces
        //     here as a per-axis label drift.
        let float_args = kwargs_of("(_ :scales 1.0)");
        let float_kw = parse_kwargs(&float_args).unwrap();
        let float_err = extract_narrowed_list::<f64, f32>(&float_kw, "scales").unwrap_err();
        match float_err {
            LispError::TypeMismatch { form, expected, .. } => {
                assert_eq!(form, KwargPath::named("scales"));
                assert_eq!(expected, <f64 as AtomKwarg<'_>>::LIST_SHAPE);
                assert_eq!(expected, ExpectedKwargShape::ListOfNumbers);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    /// [`WideNumeric`] is a `for<'a> AtomKwarg<'a>` supertrait
    /// extension: post-lift the atom-family per-axis primitives
    /// (`SHAPE` + `project` + the composed `extract_kwarg` /
    /// `extract_optional_kwarg` defaults) live on the shared
    /// [`AtomKwarg`] contract, and [`WideNumeric`] adds ONE numeric-
    /// specific per-axis primitive on top: [`WideNumeric::as_literal`],
    /// the wide-into-`NumericLiteral` lift the
    /// [`narrow_or_range_err`] rejection primitive quotes. Every
    /// other per-axis primitive [`WideNumeric`] exposes (`SHAPE`,
    /// `as_wide`, `extract_kwarg`, `extract_optional_kwarg`) is now
    /// a one-line delegate to the [`AtomKwarg`] supertrait's own
    /// contract, so a caller reaching for either the numeric-axis
    /// (`<i64>`, `<f64>`) or the non-numeric-axis (`<&str>`,
    /// `<bool>`) path lands on the SAME shared atom-family
    /// composition — the `extract_atom(kw, key, SHAPE, project)`
    /// skeleton lives on ONE owner rather than restated on both
    /// traits.
    ///
    /// Pin the delegate identity at the four supertrait-owned
    /// surfaces per numeric axis so a regression that silently
    /// unmoored [`WideNumeric`]'s delegate from the supertrait's
    /// composition (e.g. reintroducing a `WideNumeric::extract_kwarg`
    /// body that inlined `extract_atom(kw, key, Self::SHAPE,
    /// Self::as_wide)` alongside the [`AtomKwarg::extract_kwarg`]
    /// body already carrying the composition) surfaces here as a
    /// byte-identity mismatch between the two routes:
    ///
    /// (1) `SHAPE` per axis — `<i64 as WideNumeric>::SHAPE` IS
    ///     `<i64 as AtomKwarg<'_>>::SHAPE`, and same for `<f64>`.
    /// (2) `as_wide` per axis — `<i64 as WideNumeric>::as_wide(s)`
    ///     IS `<i64 as AtomKwarg<'_>>::project(s)` at every atom
    ///     shape (accept own axis, reject the other atoms with
    ///     `None`), and same for `<f64>`.
    /// (3) `extract_kwarg` per axis — verdicts on TOTAL /
    ///     TYPE-MISMATCH (with the axis-typed `ExpectedKwargShape`)
    ///     / ABSENT match the supertrait's [`AtomKwarg::
    ///     extract_kwarg`] byte-for-byte at both axes.
    /// (4) `extract_optional_kwarg` per axis — same delegate
    ///     shape; an absent kwarg short-circuits to `Ok(None)`
    ///     through both routes.
    #[test]
    fn wide_numeric_delegates_every_atom_family_primitive_to_the_atom_kwarg_supertrait() {
        // (1) SHAPE per axis — the supertrait delegate matches the
        //     [`AtomKwarg`] impl's associated const byte-for-byte at
        //     both wide axes.
        assert_eq!(<i64 as WideNumeric>::SHAPE, <i64 as AtomKwarg<'_>>::SHAPE,);
        assert_eq!(<i64 as WideNumeric>::SHAPE, ExpectedKwargShape::Int);
        assert_eq!(<f64 as WideNumeric>::SHAPE, <f64 as AtomKwarg<'_>>::SHAPE,);
        assert_eq!(<f64 as WideNumeric>::SHAPE, ExpectedKwargShape::Number);

        // (2) as_wide per axis — the supertrait delegate matches the
        //     [`AtomKwarg::project`] method at every atom shape (own
        //     axis accepted; other atoms rejected with `None`).
        let int_atom = Sexp::Atom(Atom::Int(8080));
        let float_atom = Sexp::Atom(Atom::Float(1.5));
        let bool_atom = Sexp::Atom(Atom::Bool(false));
        let string_atom = Sexp::Atom(Atom::Str("hi".to_string()));

        for sexp in [&int_atom, &float_atom, &bool_atom, &string_atom] {
            assert_eq!(
                <i64 as WideNumeric>::as_wide(sexp),
                <i64 as AtomKwarg<'_>>::project(sexp),
                "int-axis as_wide delegate diverged from AtomKwarg::project",
            );
            // Float delegate identity — NaN would not compare equal
            // under `Some(_) == Some(_)`, but no test atom here is
            // NaN, so `Option<f64>` equality holds bit-for-bit.
            assert_eq!(
                <f64 as WideNumeric>::as_wide(sexp),
                <f64 as AtomKwarg<'_>>::project(sexp),
                "float-axis as_wide delegate diverged from AtomKwarg::project",
            );
        }

        // (3) extract_kwarg per axis — TOTAL, TYPE-MISMATCH,
        //     axis-typed rejection label all match the supertrait
        //     dispatch byte-for-byte.
        let int_args = kwargs_of("(_ :port 8080)");
        let int_kw = parse_kwargs(&int_args).unwrap();
        assert_eq!(
            <i64 as WideNumeric>::extract_kwarg(&int_kw, "port").unwrap(),
            <i64 as AtomKwarg<'_>>::extract_kwarg(&int_kw, "port").unwrap(),
        );

        let float_args = kwargs_of("(_ :scale 1.5)");
        let float_kw = parse_kwargs(&float_args).unwrap();
        let via_wide = <f64 as WideNumeric>::extract_kwarg(&float_kw, "scale").unwrap();
        let via_atom = <f64 as AtomKwarg<'_>>::extract_kwarg(&float_kw, "scale").unwrap();
        assert!((via_wide - via_atom).abs() < f64::EPSILON);

        // TYPE-MISMATCH — both routes emit the SAME axis-typed
        // `ExpectedKwargShape` (`Int` on `<i64>`, `Number` on
        // `<f64>`). A regression that unmoored the delegate would
        // let one route quote a different axis label than the other.
        let bool_args = kwargs_of("(_ :n #t)");
        let bool_kw = parse_kwargs(&bool_args).unwrap();
        let int_wide_err = <i64 as WideNumeric>::extract_kwarg(&bool_kw, "n").unwrap_err();
        let int_atom_err = <i64 as AtomKwarg<'_>>::extract_kwarg(&bool_kw, "n").unwrap_err();
        let (
            LispError::TypeMismatch {
                expected: wide_int_exp,
                ..
            },
            LispError::TypeMismatch {
                expected: atom_int_exp,
                ..
            },
        ) = (&int_wide_err, &int_atom_err)
        else {
            panic!("expected TypeMismatch on both routes, got {int_wide_err:?} / {int_atom_err:?}");
        };
        assert_eq!(wide_int_exp, atom_int_exp);
        assert_eq!(*wide_int_exp, ExpectedKwargShape::Int);

        let float_wide_err = <f64 as WideNumeric>::extract_kwarg(&bool_kw, "n").unwrap_err();
        let float_atom_err = <f64 as AtomKwarg<'_>>::extract_kwarg(&bool_kw, "n").unwrap_err();
        let (
            LispError::TypeMismatch {
                expected: wide_float_exp,
                ..
            },
            LispError::TypeMismatch {
                expected: atom_float_exp,
                ..
            },
        ) = (&float_wide_err, &float_atom_err)
        else {
            panic!(
                "expected TypeMismatch on both routes, got {float_wide_err:?} / {float_atom_err:?}",
            );
        };
        assert_eq!(wide_float_exp, atom_float_exp);
        assert_eq!(*wide_float_exp, ExpectedKwargShape::Number);

        // (4) extract_optional_kwarg per axis — ABSENT short-
        //     circuits to `Ok(None)` through both routes.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        assert_eq!(
            <i64 as WideNumeric>::extract_optional_kwarg(&absent_kw, "missing").unwrap(),
            <i64 as AtomKwarg<'_>>::extract_optional_kwarg(&absent_kw, "missing").unwrap(),
        );
        assert_eq!(
            <f64 as WideNumeric>::extract_optional_kwarg(&absent_kw, "missing").unwrap(),
            <f64 as AtomKwarg<'_>>::extract_optional_kwarg(&absent_kw, "missing").unwrap(),
        );
    }

    /// `extract_narrowed<W, T>` / `extract_optional_narrowed<W, T>`
    /// are the two GENERIC narrowing primitives every
    /// `extract_*_narrowed` extractor now delegates to as a one-line
    /// wrapper. Pre-lift the four extractors each spelled the
    /// wide-axis extractor call inline (`extract_int` /
    /// `extract_optional_int` / `extract_float` /
    /// `extract_optional_float`); post-lift the axis identity rides
    /// the `W: WideNumeric` type parameter and the four extractors
    /// resolve to `extract_narrowed::<i64, _>` /
    /// `extract_optional_narrowed::<i64, _>` / their `<f64, _>`
    /// peers. Pin the two-axis closed-set coverage at the generic
    /// primitive: the canonical `port 70000 → u16` case reaches
    /// `extract_narrowed::<i64, u16>` with the typed
    /// `NumericWidth::U16` / `NumericLiteral::Int(70_000)` pair
    /// carried on `LispError::KwargOutOfRange`, and the peer
    /// `scale 1.0e300 → f32` case reaches `extract_narrowed::<f64,
    /// f32>` with the peer identities on the peer axis; the
    /// in-range totals round-trip unchanged at both axes and the
    /// optional peer short-circuits an absent kwarg to `Ok(None)`.
    /// A regression that swapped the two primitives inside one of
    /// the four `extract_*_narrowed` delegates would fail at rustc
    /// time — `extract_narrowed::<i64, T: NarrowNumeric<i64>>` and
    /// `extract_narrowed::<f64, T: NarrowNumeric<f64>>` are two
    /// disjoint typed slots.
    #[test]
    fn extract_narrowed_generic_primitives_close_the_two_axes_at_the_type_parameter() {
        // TOTAL, int axis.
        let int_ok_args = kwargs_of("(_ :port 8080)");
        let int_ok_kw = parse_kwargs(&int_ok_args).unwrap();
        assert_eq!(
            extract_narrowed::<i64, u16>(&int_ok_kw, "port").unwrap(),
            8080
        );
        assert_eq!(
            extract_optional_narrowed::<i64, u16>(&int_ok_kw, "port").unwrap(),
            Some(8080),
        );

        // TOTAL, float axis.
        let float_ok_args = kwargs_of("(_ :scale 1.0)");
        let float_ok_kw = parse_kwargs(&float_ok_args).unwrap();
        let ok_f32 = extract_narrowed::<f64, f32>(&float_ok_kw, "scale").unwrap();
        assert!((ok_f32 - 1.0_f32).abs() < f32::EPSILON);
        let ok_optional_f32 = extract_optional_narrowed::<f64, f32>(&float_ok_kw, "scale").unwrap();
        assert!(
            matches!(ok_optional_f32, Some(x) if (x - 1.0_f32).abs() < f32::EPSILON),
            "optional float axis round-trips the wide value unchanged",
        );

        // RANGE-ERR, int axis — the canonical `port 70000 → u16` case.
        let int_bad_args = kwargs_of("(_ :port 70000)");
        let int_bad_kw = parse_kwargs(&int_bad_args).unwrap();
        let int_err =
            extract_narrowed::<i64, u16>(&int_bad_kw, "port").expect_err("70000 overflows u16");
        let LispError::KwargOutOfRange { target, value, .. } = &int_err else {
            panic!("expected KwargOutOfRange, got {int_err:?}");
        };
        assert_eq!(*target, NumericWidth::U16);
        assert_eq!(*value, NumericLiteral::Int(70_000));

        // RANGE-ERR, float axis — the peer `scale 1.0e300 → f32` case.
        let float_bad_args = kwargs_of("(_ :scale 1.0e300)");
        let float_bad_kw = parse_kwargs(&float_bad_args).unwrap();
        let float_err = extract_narrowed::<f64, f32>(&float_bad_kw, "scale")
            .expect_err("1.0e300 overflows f32");
        let LispError::KwargOutOfRange { target, value, .. } = &float_err else {
            panic!("expected KwargOutOfRange, got {float_err:?}");
        };
        assert_eq!(*target, NumericWidth::F32);
        assert!(
            matches!(value, NumericLiteral::Float(x) if (*x - 1.0e300).abs() < f64::EPSILON),
            "the diagnostic must echo the author's own literal, got {value:?}",
        );

        // ABSENT, both axes — the optional peer short-circuits.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        assert_eq!(
            extract_optional_narrowed::<i64, u16>(&absent_kw, "missing").unwrap(),
            None,
        );
        assert_eq!(
            extract_optional_narrowed::<f64, f32>(&absent_kw, "missing").unwrap(),
            None,
        );
    }

    /// The four public `extract_*_narrowed` extractors are now
    /// one-line delegates to [`extract_narrowed`] /
    /// [`extract_optional_narrowed`]. Pin the delegation identity
    /// at the operator-visible level: for every input a
    /// caller-shaped `extract_int_narrowed::<T>` / peer accepts or
    /// rejects, the corresponding generic call
    /// `extract_narrowed::<i64, T>` / peer must produce the same
    /// verdict, and vice versa. Sweep the four extractor / generic
    /// pairs over both a total input and a rejecting input to lock
    /// the delegation shape — a regression that silently swapped a
    /// delegate's axis parameter (`extract_int_narrowed` accidentally
    /// binding to `extract_narrowed::<f64, T>`) would fail to compile
    /// AT the delegate site (the `NarrowNumeric<i64>` bound wouldn't
    /// resolve against `<f64, T: NarrowNumeric<f64>>`) AND, if that
    /// somehow compiled, would surface here as a diverging verdict.
    #[test]
    fn extract_star_narrowed_delegates_agree_with_the_generic_primitives_at_both_verdicts() {
        // Total, int axis.
        let int_ok_args = kwargs_of("(_ :port 8080)");
        let int_ok_kw = parse_kwargs(&int_ok_args).unwrap();
        assert_eq!(
            extract_int_narrowed::<u16>(&int_ok_kw, "port").unwrap(),
            extract_narrowed::<i64, u16>(&int_ok_kw, "port").unwrap(),
        );
        assert_eq!(
            extract_optional_int_narrowed::<u16>(&int_ok_kw, "port").unwrap(),
            extract_optional_narrowed::<i64, u16>(&int_ok_kw, "port").unwrap(),
        );

        // Total, float axis.
        let float_ok_args = kwargs_of("(_ :scale 1.0)");
        let float_ok_kw = parse_kwargs(&float_ok_args).unwrap();
        let (a, b) = (
            extract_float_narrowed::<f32>(&float_ok_kw, "scale").unwrap(),
            extract_narrowed::<f64, f32>(&float_ok_kw, "scale").unwrap(),
        );
        assert!((a - b).abs() < f32::EPSILON);

        // Rejecting, int axis — same `KwargOutOfRange { target, value }`
        // pair through either surface.
        let int_bad_args = kwargs_of("(_ :port 70000)");
        let int_bad_kw = parse_kwargs(&int_bad_args).unwrap();
        let via_wrapper = extract_int_narrowed::<u16>(&int_bad_kw, "port").unwrap_err();
        let via_generic = extract_narrowed::<i64, u16>(&int_bad_kw, "port").unwrap_err();
        match (&via_wrapper, &via_generic) {
            (
                LispError::KwargOutOfRange {
                    target: t1,
                    value: v1,
                    ..
                },
                LispError::KwargOutOfRange {
                    target: t2,
                    value: v2,
                    ..
                },
            ) => {
                assert_eq!(t1, t2);
                assert_eq!(v1, v2);
                assert_eq!(*t1, NumericWidth::U16);
                assert_eq!(*v1, NumericLiteral::Int(70_000));
            }
            _ => panic!("both must be KwargOutOfRange, got {via_wrapper:?} vs {via_generic:?}"),
        }

        // Rejecting, float axis — peer identity on the peer axis.
        let float_bad_args = kwargs_of("(_ :scale 1.0e300)");
        let float_bad_kw = parse_kwargs(&float_bad_args).unwrap();
        let via_wrapper = extract_float_narrowed::<f32>(&float_bad_kw, "scale").unwrap_err();
        let via_generic = extract_narrowed::<f64, f32>(&float_bad_kw, "scale").unwrap_err();
        match (&via_wrapper, &via_generic) {
            (
                LispError::KwargOutOfRange {
                    target: t1,
                    value: v1,
                    ..
                },
                LispError::KwargOutOfRange {
                    target: t2,
                    value: v2,
                    ..
                },
            ) => {
                assert_eq!(t1, t2);
                assert_eq!(v1, v2);
                assert_eq!(*t1, NumericWidth::F32);
            }
            _ => panic!("both must be KwargOutOfRange, got {via_wrapper:?} vs {via_generic:?}"),
        }
    }

    /// [`extract_optional_narrowed<W, T>`] is now a one-line delegate
    /// to `optional_from_required(kw, key, extract_narrowed::<W, T>)`
    /// — the SIXTH consumer of the [`optional_from_required`] present-
    /// vs-absent substrate primitive (after the four list-family peers
    /// plus [`extract_optional_via_serde`] on the universal-serde
    /// axis). Pin the delegation-identity contract at the operator-
    /// visible level: for every input a caller-shaped
    /// [`extract_optional_narrowed::<W, T>`] accepts or rejects, the
    /// hand-composed [`optional_from_required(kw, key, extract_narrowed::<W, T>)`]
    /// must produce the byte-identical verdict — same `Ok(Some(_))` /
    /// `Ok(None)` / `Err(_)` shape, same [`LispError`] variant on
    /// rejection, same typed `NumericWidth` / `NumericLiteral` payload
    /// on out-of-range, same [`ExpectedKwargShape`] payload on
    /// shape-mismatch. Sweep the pair across the FOUR canonical
    /// verdicts × BOTH axes ({int, float} × {absent, present-wrong-
    /// shape, present-out-of-range, present-in-range}) to lock the
    /// delegation shape — a regression that swapped the extractor's
    /// body back to the pre-lift inline
    /// `let Some(wide) = <W as WideNumeric>::extract_optional_kwarg(...)? ...`
    /// composition (byte-equivalent today but reaching for the
    /// atom-family primitive rather than [`optional_from_required`]
    /// — a diagnostic-promotion divergence at the substrate primitive
    /// layer) would still pass THIS test (both paths produce the same
    /// diagnostic bytes on every input listed above); the load-bearing
    /// proof this test carries is the FORWARD compatibility of the
    /// delegation across a `optional_from_required` diagnostic
    /// promotion (a probe, a metric, a span on the present-vs-absent
    /// gate — every future promotion at that primitive flows to the
    /// scalar-narrowed peer here through this delegate, sight-unseen
    /// by every caller).
    ///
    /// Peer to
    /// [`extract_star_narrowed_delegates_agree_with_the_generic_primitives_at_both_verdicts`]
    /// on the narrowed-extractor family — that test pins the four
    /// public `extract_*_narrowed` wrappers as one-line delegates to
    /// [`extract_narrowed`] / [`extract_optional_narrowed`] at the
    /// axis-parameter level; this test pins the newly-composed
    /// [`extract_optional_narrowed`] delegate to
    /// [`optional_from_required`] at the present-vs-absent primitive
    /// level, closing the family loop on the substrate primitive
    /// every other optional peer already binds through.
    #[test]
    fn extract_optional_narrowed_delegates_through_optional_from_required_across_the_four_verdicts()
    {
        // (1) ABSENT, int axis — both paths short-circuit to
        //     `Ok(None)` without invoking the required extractor's
        //     shape gate.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        assert_eq!(
            extract_optional_narrowed::<i64, u16>(&absent_kw, "missing").unwrap(),
            optional_from_required(&absent_kw, "missing", extract_narrowed::<i64, u16>).unwrap(),
        );
        assert_eq!(
            extract_optional_narrowed::<i64, u16>(&absent_kw, "missing").unwrap(),
            None,
        );

        // (1) ABSENT, float axis — peer identity on the peer axis.
        assert_eq!(
            extract_optional_narrowed::<f64, f32>(&absent_kw, "missing").unwrap(),
            optional_from_required(&absent_kw, "missing", extract_narrowed::<f64, f32>).unwrap(),
        );

        // (2) PRESENT, in range, int axis — both paths return
        //     `Ok(Some(narrow))` wrapping the same value.
        let int_ok_args = kwargs_of("(_ :port 8080)");
        let int_ok_kw = parse_kwargs(&int_ok_args).unwrap();
        assert_eq!(
            extract_optional_narrowed::<i64, u16>(&int_ok_kw, "port").unwrap(),
            optional_from_required(&int_ok_kw, "port", extract_narrowed::<i64, u16>).unwrap(),
        );
        assert_eq!(
            extract_optional_narrowed::<i64, u16>(&int_ok_kw, "port").unwrap(),
            Some(8080_u16),
        );

        // (2) PRESENT, in range, float axis — peer identity on the
        //     peer axis.
        let float_ok_args = kwargs_of("(_ :scale 1.0)");
        let float_ok_kw = parse_kwargs(&float_ok_args).unwrap();
        let via_wrapper = extract_optional_narrowed::<f64, f32>(&float_ok_kw, "scale").unwrap();
        let via_primitive =
            optional_from_required(&float_ok_kw, "scale", extract_narrowed::<f64, f32>).unwrap();
        match (via_wrapper, via_primitive) {
            (Some(a), Some(b)) => assert!((a - b).abs() < f32::EPSILON),
            other => panic!("both must be Some(f32), got {other:?}"),
        }

        // (3) PRESENT, wrong shape, int axis — both paths surface the
        //     SAME `LispError::TypeMismatch` variant with the SAME
        //     axis-typed `ExpectedKwargShape::Int` label and the SAME
        //     `KwargPath::Named` form.
        let int_shape_args = kwargs_of(r#"(_ :port "hello")"#);
        let int_shape_kw = parse_kwargs(&int_shape_args).unwrap();
        let via_wrapper = extract_optional_narrowed::<i64, u16>(&int_shape_kw, "port")
            .expect_err("string is not an int");
        let via_primitive =
            optional_from_required(&int_shape_kw, "port", extract_narrowed::<i64, u16>)
                .expect_err("string is not an int");
        match (&via_wrapper, &via_primitive) {
            (
                LispError::TypeMismatch {
                    form: f1,
                    expected: e1,
                    got: g1,
                },
                LispError::TypeMismatch {
                    form: f2,
                    expected: e2,
                    got: g2,
                },
            ) => {
                assert_eq!(f1, f2);
                assert_eq!(e1, e2);
                assert_eq!(g1, g2);
                assert_eq!(*e1, ExpectedKwargShape::Int);
                assert_eq!(*g1, SexpShape::String);
                assert_eq!(f1, &KwargPath::named("port"));
            }
            _ => panic!("both must be TypeMismatch, got {via_wrapper:?} vs {via_primitive:?}"),
        }

        // (3) PRESENT, wrong shape, float axis — peer identity on the
        //     peer axis; `ExpectedKwargShape::Number` (the wider
        //     numeric-union label the WideNumeric<f64> shape gate
        //     emits).
        let float_shape_args = kwargs_of(r#"(_ :scale "hello")"#);
        let float_shape_kw = parse_kwargs(&float_shape_args).unwrap();
        let via_wrapper = extract_optional_narrowed::<f64, f32>(&float_shape_kw, "scale")
            .expect_err("string is not a float");
        let via_primitive =
            optional_from_required(&float_shape_kw, "scale", extract_narrowed::<f64, f32>)
                .expect_err("string is not a float");
        match (&via_wrapper, &via_primitive) {
            (
                LispError::TypeMismatch {
                    form: f1,
                    expected: e1,
                    got: g1,
                },
                LispError::TypeMismatch {
                    form: f2,
                    expected: e2,
                    got: g2,
                },
            ) => {
                assert_eq!(f1, f2);
                assert_eq!(e1, e2);
                assert_eq!(g1, g2);
                assert_eq!(*e1, ExpectedKwargShape::Number);
            }
            _ => panic!("both must be TypeMismatch, got {via_wrapper:?} vs {via_primitive:?}"),
        }

        // (4) PRESENT, out of range, int axis — canonical
        //     `port 70000 → u16` case. Both paths surface the SAME
        //     `LispError::KwargOutOfRange` variant with the SAME
        //     `NumericWidth::U16` target and `NumericLiteral::Int(70_000)`
        //     value.
        let int_range_args = kwargs_of("(_ :port 70000)");
        let int_range_kw = parse_kwargs(&int_range_args).unwrap();
        let via_wrapper = extract_optional_narrowed::<i64, u16>(&int_range_kw, "port")
            .expect_err("70000 overflows u16");
        let via_primitive =
            optional_from_required(&int_range_kw, "port", extract_narrowed::<i64, u16>)
                .expect_err("70000 overflows u16");
        match (&via_wrapper, &via_primitive) {
            (
                LispError::KwargOutOfRange {
                    target: t1,
                    value: v1,
                    ..
                },
                LispError::KwargOutOfRange {
                    target: t2,
                    value: v2,
                    ..
                },
            ) => {
                assert_eq!(t1, t2);
                assert_eq!(v1, v2);
                assert_eq!(*t1, NumericWidth::U16);
                assert_eq!(*v1, NumericLiteral::Int(70_000));
            }
            _ => panic!("both must be KwargOutOfRange, got {via_wrapper:?} vs {via_primitive:?}"),
        }

        // (4) PRESENT, out of range, float axis — peer identity on
        //     the peer axis. `1.0e300 → f32` overflows to infinity.
        let float_range_args = kwargs_of("(_ :scale 1.0e300)");
        let float_range_kw = parse_kwargs(&float_range_args).unwrap();
        let via_wrapper = extract_optional_narrowed::<f64, f32>(&float_range_kw, "scale")
            .expect_err("1.0e300 overflows f32");
        let via_primitive =
            optional_from_required(&float_range_kw, "scale", extract_narrowed::<f64, f32>)
                .expect_err("1.0e300 overflows f32");
        match (&via_wrapper, &via_primitive) {
            (
                LispError::KwargOutOfRange {
                    target: t1,
                    value: v1,
                    ..
                },
                LispError::KwargOutOfRange {
                    target: t2,
                    value: v2,
                    ..
                },
            ) => {
                assert_eq!(t1, t2);
                assert_eq!(v1, v2);
                assert_eq!(*t1, NumericWidth::F32);
            }
            _ => panic!("both must be KwargOutOfRange, got {via_wrapper:?} vs {via_primitive:?}"),
        }
    }

    /// `extract_narrowed_list<W, T>` is the list-family peer of
    /// `extract_narrowed<W, T>` on the per-item numeric-narrowing
    /// path. Pin FOUR promises the new primitive owns:
    ///
    /// (1) TOTAL — an in-range list at each axis collects into a
    ///     `Vec<T>` at the field's own width.
    /// (2) PER-ITEM RANGE-ERR — a list with one out-of-range item
    ///     rejects with `LispError::KwargOutOfRange { form:
    ///     KwargPath::Item { key, idx }, target, value }` — the item
    ///     path (not the scalar `KwargPath::Named(key)`) plus the
    ///     axis-typed target width plus the author's own literal.
    /// (3) PER-ITEM SHAPE-ERR — a list with one shape-wrong item
    ///     rejects with `LispError::TypeMismatch { form:
    ///     KwargPath::Item { key, idx }, expected: <axis's SHAPE>,
    ///     got: <sexp shape> }` — same axis-typed `SHAPE`
    ///     [`WideNumeric::SHAPE`] the scalar peer's shape gate
    ///     emits (`Int` on the int axis, `Number` on the float
    ///     axis).
    /// (4) ABSENT — an absent list-typed kwarg short-circuits to
    ///     `Ok(Vec::new())`, matching every peer list extractor's
    ///     posture (`extract_string_list`, `extract_vec_via_serde`).
    ///
    /// A regression that (a) swapped the per-item rejection path to
    /// `KwargPath::Named(key)` (losing the item index), (b) widened
    /// the per-item shape label to `List` (leaking the outer-shape
    /// label into the per-item slot), (c) silently promoted an
    /// out-of-range item to `None` (the exact corruption the scalar
    /// `extract_optional_narrowed` docstring warns against, on the
    /// list-typed cousin), or (d) rejected an absent kwarg (breaking
    /// every downstream `Vec<T>` field's absent-optional posture)
    /// would fail at least one arm of this sweep.
    #[test]
    fn extract_narrowed_list_walks_the_per_item_narrowing_gate_at_both_axes() {
        // (1) TOTAL, int axis — `Vec<u16>` from a list of ints.
        let int_ok_args = kwargs_of("(_ :ports (80 443 8080))");
        let int_ok_kw = parse_kwargs(&int_ok_args).unwrap();
        assert_eq!(
            extract_narrowed_list::<i64, u16>(&int_ok_kw, "ports").unwrap(),
            vec![80u16, 443, 8080],
        );

        // (1) TOTAL, float axis — `Vec<f32>` from a list of floats.
        let float_ok_args = kwargs_of("(_ :scales (1.0 2.5))");
        let float_ok_kw = parse_kwargs(&float_ok_args).unwrap();
        let scales = extract_narrowed_list::<f64, f32>(&float_ok_kw, "scales").unwrap();
        assert_eq!(scales.len(), 2);
        assert!((scales[0] - 1.0_f32).abs() < f32::EPSILON);
        assert!((scales[1] - 2.5_f32).abs() < f32::EPSILON);

        // (2) PER-ITEM RANGE-ERR, int axis — second item `70000`
        //     overflows u16. Rejection carries the item path (idx: 1),
        //     the typed width (U16), and the author's own literal
        //     (Int(70_000)) — same shape the scalar peer emits, plus
        //     the item-index axis.
        let int_bad_args = kwargs_of("(_ :ports (80 70000))");
        let int_bad_kw = parse_kwargs(&int_bad_args).unwrap();
        let int_err = extract_narrowed_list::<i64, u16>(&int_bad_kw, "ports")
            .expect_err("second item overflows u16");
        let LispError::KwargOutOfRange {
            form,
            target,
            value,
        } = &int_err
        else {
            panic!("expected KwargOutOfRange, got {int_err:?}");
        };
        assert!(
            matches!(form, KwargPath::Item { key, idx: 1 } if key == "ports"),
            "per-item narrowing rejection must carry KwargPath::Item {{ key: \"ports\", idx: 1 }}, got {form:?}"
        );
        assert_eq!(*target, NumericWidth::U16);
        assert_eq!(*value, NumericLiteral::Int(70_000));

        // (2) PER-ITEM RANGE-ERR, float axis — third item `1.0e300`
        //     overflows f32.
        let float_bad_args = kwargs_of("(_ :scales (1.0 2.0 1.0e300))");
        let float_bad_kw = parse_kwargs(&float_bad_args).unwrap();
        let float_err = extract_narrowed_list::<f64, f32>(&float_bad_kw, "scales")
            .expect_err("third item overflows f32");
        let LispError::KwargOutOfRange {
            form,
            target,
            value,
        } = &float_err
        else {
            panic!("expected KwargOutOfRange, got {float_err:?}");
        };
        assert!(
            matches!(form, KwargPath::Item { key, idx: 2 } if key == "scales"),
            "float-axis per-item narrowing rejection must carry KwargPath::Item {{ key: \"scales\", idx: 2 }}, got {form:?}"
        );
        assert_eq!(*target, NumericWidth::F32);
        assert!(
            matches!(value, NumericLiteral::Float(x) if (*x - 1.0e300).abs() < f64::EPSILON),
            "the per-item diagnostic must echo the author's own literal, got {value:?}",
        );

        // (3) PER-ITEM SHAPE-ERR, int axis — second item is a string,
        //     not an int atom. The axis-typed SHAPE label (`Int`) rides
        //     through, the item path names the failing element.
        let int_shape_args = kwargs_of(r#"(_ :ports (80 "not-int"))"#);
        let int_shape_kw = parse_kwargs(&int_shape_args).unwrap();
        let shape_err = extract_narrowed_list::<i64, u16>(&int_shape_kw, "ports")
            .expect_err("second item is not an int atom");
        let LispError::TypeMismatch { form, expected, .. } = &shape_err else {
            panic!("expected TypeMismatch, got {shape_err:?}");
        };
        assert!(
            matches!(form, KwargPath::Item { key, idx: 1 } if key == "ports"),
            "per-item shape rejection must carry KwargPath::Item {{ key: \"ports\", idx: 1 }}, got {form:?}"
        );
        assert_eq!(
            *expected,
            ExpectedKwargShape::Int,
            "int-axis per-item shape gate must emit the axis-typed SHAPE label, not the outer-shape List",
        );

        // (3) PER-ITEM SHAPE-ERR, float axis — the peer `Number` label.
        let float_shape_args = kwargs_of(r#"(_ :scales (1.0 "not-num"))"#);
        let float_shape_kw = parse_kwargs(&float_shape_args).unwrap();
        let float_shape_err = extract_narrowed_list::<f64, f32>(&float_shape_kw, "scales")
            .expect_err("second item is not a numeric atom");
        let LispError::TypeMismatch { form, expected, .. } = &float_shape_err else {
            panic!("expected TypeMismatch, got {float_shape_err:?}");
        };
        assert!(
            matches!(form, KwargPath::Item { key, idx: 1 } if key == "scales"),
            "float-axis per-item shape rejection must carry KwargPath::Item {{ key: \"scales\", idx: 1 }}, got {form:?}"
        );
        assert_eq!(
            *expected,
            ExpectedKwargShape::Number,
            "float-axis per-item shape gate must emit the axis-typed SHAPE label",
        );

        // (4) ABSENT — an absent list-typed kwarg is the empty list, at
        //     both axes.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        assert_eq!(
            extract_narrowed_list::<i64, u16>(&absent_kw, "missing").unwrap(),
            Vec::<u16>::new(),
        );
        assert_eq!(
            extract_narrowed_list::<f64, f32>(&absent_kw, "missing").unwrap(),
            Vec::<f32>::new(),
        );
    }

    /// The two public `extract_*_list_narrowed` wrappers are one-line
    /// delegates to [`extract_narrowed_list`] at their axis. Pin the
    /// delegation identity at the operator-visible level: for every
    /// input the caller-shaped wrapper accepts or rejects, the generic
    /// call at the matching axis must produce the same verdict — same
    /// pattern the peer scalar
    /// `extract_star_narrowed_delegates_agree_with_the_generic_primitives_at_both_verdicts`
    /// test pins for the atomic-narrowing primitives.
    #[test]
    fn extract_star_list_narrowed_delegates_agree_with_the_generic_list_primitive() {
        // Total, int axis.
        let int_ok_args = kwargs_of("(_ :ports (80 443))");
        let int_ok_kw = parse_kwargs(&int_ok_args).unwrap();
        assert_eq!(
            extract_int_list_narrowed::<u16>(&int_ok_kw, "ports").unwrap(),
            extract_narrowed_list::<i64, u16>(&int_ok_kw, "ports").unwrap(),
        );

        // Total, float axis — element-wise equality on the round-tripped Vec.
        let float_ok_args = kwargs_of("(_ :scales (1.0 2.5))");
        let float_ok_kw = parse_kwargs(&float_ok_args).unwrap();
        let via_wrapper = extract_float_list_narrowed::<f32>(&float_ok_kw, "scales").unwrap();
        let via_generic = extract_narrowed_list::<f64, f32>(&float_ok_kw, "scales").unwrap();
        assert_eq!(via_wrapper.len(), via_generic.len());
        for (a, b) in via_wrapper.iter().zip(via_generic.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }

        // Rejecting, int axis — same `KwargOutOfRange { form, target,
        // value }` triple through either surface.
        let int_bad_args = kwargs_of("(_ :ports (80 70000))");
        let int_bad_kw = parse_kwargs(&int_bad_args).unwrap();
        let via_wrapper = extract_int_list_narrowed::<u16>(&int_bad_kw, "ports").unwrap_err();
        let via_generic = extract_narrowed_list::<i64, u16>(&int_bad_kw, "ports").unwrap_err();
        match (&via_wrapper, &via_generic) {
            (
                LispError::KwargOutOfRange {
                    form: f1,
                    target: t1,
                    value: v1,
                },
                LispError::KwargOutOfRange {
                    form: f2,
                    target: t2,
                    value: v2,
                },
            ) => {
                assert_eq!(f1, f2);
                assert_eq!(t1, t2);
                assert_eq!(v1, v2);
                assert!(
                    matches!(f1, KwargPath::Item { key, idx: 1 } if key == "ports"),
                    "wrapper delegation must carry the same item-path shape, got {f1:?}"
                );
                assert_eq!(*t1, NumericWidth::U16);
                assert_eq!(*v1, NumericLiteral::Int(70_000));
            }
            _ => panic!("both must be KwargOutOfRange, got {via_wrapper:?} vs {via_generic:?}"),
        }
    }

    /// Optional-vec sibling of
    /// [`extract_star_list_narrowed_delegates_agree_with_the_generic_list_primitive`]
    /// — pin the delegation identity between the caller-shaped
    /// `extract_optional_{int,float}_list_narrowed::<T>` wrappers and
    /// the generic `extract_optional_narrowed_list::<W, T>` primitive
    /// at the axis. For every input the wrapper accepts or rejects,
    /// the generic call at the matching axis must produce the same
    /// verdict — same pattern the required-vec peer test pins for the
    /// numeric-narrowing primitive, only lifted to the outer
    /// present-vs-absent axis.
    #[test]
    fn extract_star_optional_list_narrowed_delegates_agree_with_the_generic_primitive() {
        // Absent kwarg, int axis — both routes must return `None`.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        assert_eq!(
            extract_optional_int_list_narrowed::<u16>(&absent_kw, "ports").unwrap(),
            extract_optional_narrowed_list::<i64, u16>(&absent_kw, "ports").unwrap(),
        );
        assert_eq!(
            extract_optional_int_list_narrowed::<u16>(&absent_kw, "ports").unwrap(),
            None,
        );

        // Present empty, float axis — both routes must return
        // `Some(Vec::new())` and the load-bearing present-vs-absent
        // distinction must survive the delegation.
        let empty_args = kwargs_of("(_ :scales ())");
        let empty_kw = parse_kwargs(&empty_args).unwrap();
        assert_eq!(
            extract_optional_float_list_narrowed::<f32>(&empty_kw, "scales").unwrap(),
            extract_optional_narrowed_list::<f64, f32>(&empty_kw, "scales").unwrap(),
        );
        assert_eq!(
            extract_optional_float_list_narrowed::<f32>(&empty_kw, "scales").unwrap(),
            Some(Vec::<f32>::new()),
        );

        // Present, in-range, int axis — same decoded `Some(vec![…])`
        // through either surface.
        let int_ok_args = kwargs_of("(_ :ports (80 443))");
        let int_ok_kw = parse_kwargs(&int_ok_args).unwrap();
        assert_eq!(
            extract_optional_int_list_narrowed::<u16>(&int_ok_kw, "ports").unwrap(),
            extract_optional_narrowed_list::<i64, u16>(&int_ok_kw, "ports").unwrap(),
        );

        // Rejecting, int axis — same `KwargOutOfRange { form, target,
        // value }` triple through either surface.
        let int_bad_args = kwargs_of("(_ :ports (80 70000))");
        let int_bad_kw = parse_kwargs(&int_bad_args).unwrap();
        let via_wrapper =
            extract_optional_int_list_narrowed::<u16>(&int_bad_kw, "ports").unwrap_err();
        let via_generic =
            extract_optional_narrowed_list::<i64, u16>(&int_bad_kw, "ports").unwrap_err();
        match (&via_wrapper, &via_generic) {
            (
                LispError::KwargOutOfRange {
                    form: f1,
                    target: t1,
                    value: v1,
                },
                LispError::KwargOutOfRange {
                    form: f2,
                    target: t2,
                    value: v2,
                },
            ) => {
                assert_eq!(f1, f2);
                assert_eq!(t1, t2);
                assert_eq!(v1, v2);
                assert!(
                    matches!(f1, KwargPath::Item { key, idx: 1 } if key == "ports"),
                    "wrapper delegation must carry the same item-path shape, got {f1:?}"
                );
                assert_eq!(*t1, NumericWidth::U16);
                assert_eq!(*v1, NumericLiteral::Int(70_000));
            }
            _ => panic!("both must be KwargOutOfRange, got {via_wrapper:?} vs {via_generic:?}"),
        }
    }

    /// A domain whose numeric fields are the pointer-width pair —
    /// `isize` is the SIGNED peer of `usize` on the pointer-width column.
    /// Pin the full (classify → extract → narrow → derive) chain for the
    /// new peer: an in-range signed authored literal parses through the
    /// same `NarrowNumeric` primitive `usize` already binds to, and the
    /// derive routes `isize` to `extract_int_narrowed::<isize>` (not the
    /// serde bridge) so the rejection shape on out-of-range on a 32-bit
    /// target would surface as `LispError::KwargOutOfRange { target:
    /// NumericWidth::Isize, .. }` rather than the mystery serde message
    /// the `Kind::Deserialize` fallthrough emitted before the classify
    /// arm existed.
    #[derive(DeriveTataraDomain, Serialize, Debug, PartialEq)]
    #[tatara(keyword = "defpointer")]
    struct PointerSpec {
        offset: isize,
        capacity: Option<isize>,
    }

    /// The peer of `narrowing_accepts_every_in_range_value_including_
    /// lossy_f32` on the pointer-width signed column — an authored
    /// negative literal parses through `extract_int_narrowed::<isize>`
    /// and a `None` for the optional isize slot stays absent. On any
    /// target the range `[-4096, 4096]` fits in `isize`, so the test
    /// is architecture-independent.
    #[test]
    fn pointer_width_signed_isize_field_parses_in_range_literal_through_the_narrowing_gate() {
        let forms = read(r"(defpointer :offset -42 :capacity 4096)").expect("reads");
        let spec = PointerSpec::compile_from_sexp(&forms[0]).expect("in-range values must parse");
        assert_eq!(
            spec,
            PointerSpec {
                offset: -42,
                capacity: Some(4096),
            }
        );

        let absent = read(r"(defpointer :offset 0)").expect("reads");
        let spec_absent =
            PointerSpec::compile_from_sexp(&absent[0]).expect("absent optionals are legal");
        assert_eq!(
            spec_absent,
            PointerSpec {
                offset: 0,
                capacity: None,
            }
        );
    }

    /// A domain whose numeric fields exercise the NARROWEST-unsigned
    /// column — `u16` is the canonical `port` case named in the
    /// [`LispError::KwargOutOfRange`] docstring. Pin the full
    /// (classify → extract → narrow → derive) chain: an in-range port
    /// parses through `extract_int_narrowed::<u16>`, an out-of-range
    /// `70000` rejects as `LispError::KwargOutOfRange { target:
    /// NumericWidth::U16, .. }` (the case whose diagnostic the docstring
    /// promises), a NEGATIVE literal `-1` rejects on the sign gate
    /// (same rejection shape as `-1` on `U32`, on the peer BIT-WIDTH),
    /// and the optional u16 arm carries the same three gates on its
    /// own axis. Before this lift the (`port: u16`, `alt_port: Option<u16>`)
    /// shape routed through the serde bridge, so the rejection surfaced
    /// as a mystery `KwargDeserialize { message: "invalid value:
    /// integer ...", .. }` — the exact substring-parse trap the typed
    /// [`NumericWidth`] identity exists to eliminate.
    #[derive(DeriveTataraDomain, Serialize, Debug, PartialEq)]
    #[tatara(keyword = "defport")]
    struct PortSpec {
        port: u16,
        alt_port: Option<u16>,
    }

    #[test]
    fn narrowest_unsigned_u16_field_parses_in_range_literal_through_the_narrowing_gate() {
        let forms = read(r"(defport :port 8080 :alt-port 65535)").expect("reads");
        let spec = PortSpec::compile_from_sexp(&forms[0]).expect("in-range values must parse");
        assert_eq!(
            spec,
            PortSpec {
                port: 8080,
                alt_port: Some(65535),
            }
        );

        let absent = read(r"(defport :port 0)").expect("reads");
        let spec_absent =
            PortSpec::compile_from_sexp(&absent[0]).expect("absent optionals are legal");
        assert_eq!(
            spec_absent,
            PortSpec {
                port: 0,
                alt_port: None,
            }
        );
    }

    /// The canonical `port 70000` case the [`LispError::KwargOutOfRange`]
    /// docstring names. Pre-lift the derive routed `port: u16` through
    /// the serde bridge, so an author's `70000` came back as a mystery
    /// `KwargDeserialize { message: "invalid value: integer ...", .. }`.
    /// Post-lift the rejection is typed as `NumericWidth::U16` and the
    /// author's own literal rides through unchanged.
    #[test]
    fn u16_field_rejects_the_canonical_port_70000_case_with_the_typed_width_identity() {
        let forms = read(r"(defport :port 70000)").expect("reads");
        let err = PortSpec::compile_from_sexp(&forms[0])
            .expect_err("70000 is out of range for u16 and must not parse");
        let LispError::KwargOutOfRange {
            form,
            target,
            value,
        } = &err
        else {
            panic!("expected KwargOutOfRange, got {err:?}");
        };
        assert_eq!(form, &KwargPath::named("port"));
        assert_eq!(*target, NumericWidth::U16);
        assert_eq!(*value, NumericLiteral::Int(70_000));
        assert_eq!(
            err.to_string(),
            "compile error in :port: 70000 is out of range for u16"
        );
    }

    /// The NEGATIVE-into-unsigned gate on the narrowest-unsigned peer.
    /// Same rejection shape as `-1` on `U32` in
    /// `negative_int_into_an_unsigned_width_is_rejected_not_sign_flipped`
    /// — pin here that `u16`'s sign-gate lands on
    /// `NumericWidth::U16` rather than drifting through the serde
    /// bridge (as it did before the classify arm existed).
    #[test]
    fn negative_int_into_u16_is_rejected_on_the_sign_gate_not_the_serde_bridge() {
        let forms = read(r"(defport :port 0 :alt-port -1)").expect("reads");
        let err = PortSpec::compile_from_sexp(&forms[0])
            .expect_err("-1 is out of range for u16 and must not parse");
        assert!(
            matches!(
                &err,
                LispError::KwargOutOfRange {
                    target: NumericWidth::U16,
                    value: NumericLiteral::Int(-1),
                    ..
                }
            ),
            "expected an Option<u16> sign-gate rejection, got {err:?}"
        );
    }

    /// The narrowest-integer trio (`i8` / `i16` / `u8`) is the newest
    /// classify cohort. Pre-lift a `u8` field routed through the serde
    /// bridge — the operator's `:count 300` came back as a mystery
    /// `KwargDeserialize { message: "invalid value: integer ..." }`.
    /// Post-lift the rejection is typed as `NumericWidth::U8`, the
    /// author's own literal rides through unchanged, and the derive
    /// names the width exactly once (as a type on the emitted turbofish).
    #[derive(DeriveTataraDomain, Serialize, Debug, PartialEq)]
    #[tatara(keyword = "defbyte")]
    struct ByteSpec {
        count: u8,
        delta: i8,
        offset: i16,
        maybe_count: Option<u8>,
    }

    #[test]
    fn narrowest_integer_trio_parses_in_range_literal_through_the_narrowing_gate() {
        let forms = read(r"(defbyte :count 200 :delta -42 :offset -30000 :maybe-count 255)")
            .expect("reads");
        let spec = ByteSpec::compile_from_sexp(&forms[0]).expect("in-range values must parse");
        assert_eq!(
            spec,
            ByteSpec {
                count: 200,
                delta: -42,
                offset: -30_000,
                maybe_count: Some(255),
            }
        );
    }

    #[test]
    fn u8_field_rejects_the_canonical_count_300_case_with_the_typed_width_identity() {
        let forms = read(r"(defbyte :count 300 :delta 0 :offset 0)").expect("reads");
        let err = ByteSpec::compile_from_sexp(&forms[0])
            .expect_err("300 is out of range for u8 and must not parse");
        let LispError::KwargOutOfRange {
            form,
            target,
            value,
        } = &err
        else {
            panic!("expected KwargOutOfRange, got {err:?}");
        };
        assert_eq!(form, &KwargPath::named("count"));
        assert_eq!(*target, NumericWidth::U8);
        assert_eq!(*value, NumericLiteral::Int(300));
        assert_eq!(
            err.to_string(),
            "compile error in :count: 300 is out of range for u8"
        );
    }

    #[test]
    fn negative_int_into_u8_is_rejected_on_the_sign_gate_not_the_serde_bridge() {
        let forms = read(r"(defbyte :count 0 :delta 0 :offset 0 :maybe-count -1)").expect("reads");
        let err = ByteSpec::compile_from_sexp(&forms[0])
            .expect_err("-1 is out of range for u8 and must not parse");
        assert!(
            matches!(
                &err,
                LispError::KwargOutOfRange {
                    target: NumericWidth::U8,
                    value: NumericLiteral::Int(-1),
                    ..
                }
            ),
            "expected an Option<u8> sign-gate rejection, got {err:?}"
        );
    }

    #[test]
    fn i8_field_rejects_the_out_of_range_128_case_with_the_typed_width_identity() {
        let forms = read(r"(defbyte :count 0 :delta 128 :offset 0)").expect("reads");
        let err = ByteSpec::compile_from_sexp(&forms[0])
            .expect_err("128 is out of range for i8 (max 127) and must not parse");
        assert!(
            matches!(
                &err,
                LispError::KwargOutOfRange {
                    target: NumericWidth::I8,
                    value: NumericLiteral::Int(128),
                    ..
                }
            ),
            "expected an i8 range-gate rejection, got {err:?}"
        );
    }

    #[test]
    fn i16_field_rejects_the_out_of_range_case_with_the_typed_width_identity() {
        let forms = read(r"(defbyte :count 0 :delta 0 :offset 40000)").expect("reads");
        let err = ByteSpec::compile_from_sexp(&forms[0])
            .expect_err("40000 is out of range for i16 (max 32767) and must not parse");
        assert!(
            matches!(
                &err,
                LispError::KwargOutOfRange {
                    target: NumericWidth::I16,
                    value: NumericLiteral::Int(40_000),
                    ..
                }
            ),
            "expected an i16 range-gate rejection, got {err:?}"
        );
    }

    /// A domain whose numeric-vec fields exercise the newly opened
    /// `Kind::VecInt` / `Kind::VecFloat` classify + emit arms.
    /// Pre-lift `ports: Vec<u16>` and `scales: Vec<f32>` fell through
    /// to `Kind::VecDeserialize`, so a per-item out-of-range value on
    /// `:ports (list 80 70000)` surfaced as `KwargDeserialize
    /// { message: "invalid value: integer 70000, expected u16", .. }`
    /// — a substring the operator had to parse. Post-lift the derive
    /// routes `Vec<u16>` to `extract_int_list_narrowed::<u16>` and
    /// `Vec<f32>` to `extract_float_list_narrowed::<f32>`, so the
    /// per-item rejection matches the scalar peer byte-for-byte
    /// modulo the `KwargPath::Item { key, idx }` → `Named(key)`
    /// per-path shift, and the two gates on the same numeric width
    /// (scalar `port: u16` vs. per-item `ports: Vec<u16>`) speak the
    /// same typed rejection vocabulary.
    #[derive(DeriveTataraDomain, Serialize, Debug, PartialEq)]
    #[tatara(keyword = "defvecports")]
    struct VecPortsSpec {
        ports: Vec<u16>,
        scales: Vec<f32>,
    }

    #[test]
    fn vec_u16_field_parses_in_range_list_through_the_per_item_narrowing_gate() {
        let forms = read(r"(defvecports :ports (80 443 8080) :scales (1.0 2.5))").expect("reads");
        let spec = VecPortsSpec::compile_from_sexp(&forms[0])
            .expect("in-range per-item values must parse");
        assert_eq!(spec.ports, vec![80u16, 443, 8080]);
        assert_eq!(spec.scales.len(), 2);
        assert!((spec.scales[0] - 1.0_f32).abs() < f32::EPSILON);
        assert!((spec.scales[1] - 2.5_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn vec_u16_field_rejects_per_item_out_of_range_with_typed_width_and_item_path() {
        // The per-item cousin of the scalar `u16` canonical rejection
        // (`port 70000` on `PortSpec`). The failure carries the item
        // index (1) alongside the same `NumericWidth::U16` /
        // `NumericLiteral::Int(70_000)` pair the scalar peer emits —
        // authoring surfaces can point directly at the failing
        // element via `KwargPath::Item { key: "ports", idx: 1 }`.
        let forms = read(r"(defvecports :ports (80 70000) :scales (1.0))").expect("reads");
        let err = VecPortsSpec::compile_from_sexp(&forms[0])
            .expect_err("70000 is out of range for u16 and must not parse");
        let LispError::KwargOutOfRange {
            form,
            target,
            value,
        } = &err
        else {
            panic!("expected KwargOutOfRange, got {err:?}");
        };
        assert_eq!(form, &KwargPath::item("ports", 1));
        assert_eq!(*target, NumericWidth::U16);
        assert_eq!(*value, NumericLiteral::Int(70_000));
        assert_eq!(
            err.to_string(),
            "compile error in :ports[1]: 70000 is out of range for u16",
        );
    }

    /// Bool-axis peer of [`VecPortsSpec`] on the non-numeric atom-
    /// family list surface: `flags: Vec<bool>` used to fall through
    /// `Kind::VecDeserialize` (the universal serde bridge), so a
    /// per-item non-bool element on `:flags (list #t "yes")`
    /// surfaced as a mystery
    /// `KwargDeserialize { message: "invalid type: string \"yes\",
    /// expected a boolean at path .1" }` substring rather than as
    /// the typed
    /// `TypeMismatch { form: Item { key: "flags", idx: 1 },
    /// expected: Bool, got: String }` its scalar peer
    /// (`enabled: bool`) already emits at the atom shape gate. Post-
    /// lift the derive routes `Vec<bool>` to `extract_bool_list`,
    /// which composes `<bool as AtomKwarg<'_>>::project_at` per
    /// item; the two gates on the same axis (scalar `enabled: bool`
    /// vs. per-item `flags: Vec<bool>`) now speak the same typed
    /// rejection vocabulary.
    #[derive(DeriveTataraDomain, Serialize, Debug, PartialEq)]
    #[tatara(keyword = "defvecflags")]
    struct VecFlagsSpec {
        flags: Vec<bool>,
    }

    #[test]
    fn vec_bool_field_parses_all_bool_list_through_the_per_item_atom_gate() {
        let forms = read(r"(defvecflags :flags (#t #f #t))").expect("reads");
        let spec = VecFlagsSpec::compile_from_sexp(&forms[0])
            .expect("all-bool per-item values must parse");
        assert_eq!(spec.flags, vec![true, false, true]);
    }

    #[test]
    fn vec_bool_field_rejects_per_item_non_bool_with_typed_shape_and_item_path() {
        // The per-item bool-axis cousin of the scalar `enabled: bool`
        // canonical rejection (`extract_bool` on `:enabled 1` naming
        // `expected bool, got int`). Post-lift the failure carries
        // the item index (1) alongside the same
        // `ExpectedKwargShape::Bool` + `SexpShape::String` pair the
        // scalar peer emits — authoring surfaces pattern-match on
        // the SAME `LispError::TypeMismatch { form: Item {..},
        // expected: Bool, got: String }` variant they already bind
        // for the scalar bool gate, not on a serde substring.
        let forms = read(r#"(defvecflags :flags (#t "yes" #f))"#).expect("reads");
        let err = VecFlagsSpec::compile_from_sexp(&forms[0])
            .expect_err("\"yes\" is not a bool and must not parse");
        let LispError::TypeMismatch {
            form,
            expected,
            got,
        } = &err
        else {
            panic!("expected TypeMismatch (typed atom-family gate), got {err:?}");
        };
        assert_eq!(form, &KwargPath::item("flags", 1));
        assert_eq!(*expected, ExpectedKwargShape::Bool);
        assert_eq!(*got, SexpShape::String);
        assert_eq!(
            err.to_string(),
            "compile error in :flags[1]: expected bool, got string",
        );
    }

    /// Optional-vec-string derive fixture — closes the
    /// present-vs-absent hole on the string-vec axis. Pre-lift the
    /// derive routed `Option<Vec<String>>` through
    /// `Kind::OptionalDeserialize` (the universal serde bridge —
    /// `classify_option` had no arm for `Option<Vec<T>>` and fell
    /// through to the catch-all), so a per-item non-string element
    /// on `:tags (list "ok" 5)` into an `Option<Vec<String>>` field
    /// surfaced as a mystery
    /// `KwargDeserialize { message: "invalid type: integer 5,
    /// expected a string at path .1" }` substring rather than as
    /// the typed
    /// `TypeMismatch { form: Item { key: "tags", idx: 1 },
    /// expected: String, got: Int }` its REQUIRED peer
    /// (`tags: Vec<String>`) already emits at the atom shape gate.
    /// Post-lift the derive routes `Option<Vec<String>>` to
    /// `extract_optional_string_list`, which delegates its present-
    /// branch decode to the SAME `extract_string_list` the required
    /// peer binds; the two peers on the same axis now speak the
    /// SAME typed rejection vocabulary. Sibling posture to
    /// `VecFlagsSpec` on the required-vec-bool axis and
    /// `VecPortsSpec` on the required-vec-numeric axis — all three
    /// substrate primitives closed the SAME class of per-item serde-
    /// bridge leak.
    #[derive(DeriveTataraDomain, Serialize, Debug, PartialEq)]
    #[tatara(keyword = "defoptvectags")]
    struct OptVecTagsSpec {
        tags: Option<Vec<String>>,
    }

    #[test]
    fn optional_vec_string_field_absent_kwarg_parses_as_none() {
        // `:tags` absent → `Ok(None)` — distinct from a present empty
        // list (`:tags ()` → `Ok(Some(vec![]))`). The load-bearing
        // present-vs-absent bifurcation an `Option<Vec<T>>` field
        // preserves that a `Vec<T>` field collapses.
        let forms = read(r"(defoptvectags)").expect("reads");
        let spec = OptVecTagsSpec::compile_from_sexp(&forms[0])
            .expect("absent optional list must parse as None");
        assert_eq!(spec.tags, None);
    }

    #[test]
    fn optional_vec_string_field_present_empty_parses_as_some_empty_vec() {
        // `:tags ()` → `Ok(Some(Vec::new()))` — a PRESENT empty list
        // is Some(vec![]), not None. Sibling to the absent-kwarg pin.
        let forms = read(r"(defoptvectags :tags ())").expect("reads");
        let spec = OptVecTagsSpec::compile_from_sexp(&forms[0])
            .expect("present empty list must parse as Some(vec![])");
        assert_eq!(spec.tags, Some(Vec::<String>::new()));
    }

    #[test]
    fn optional_vec_string_field_parses_all_string_list_through_the_per_item_atom_gate() {
        // Happy-path: a present list of strings decodes byte-identically
        // to the required peer `Vec<String>` field's decode, wrapped in
        // `Some`.
        let forms = read(r#"(defoptvectags :tags ("alpha" "beta" "gamma"))"#).expect("reads");
        let spec = OptVecTagsSpec::compile_from_sexp(&forms[0])
            .expect("all-string per-item values must parse");
        assert_eq!(
            spec.tags,
            Some(vec![
                "alpha".to_string(),
                "beta".to_string(),
                "gamma".to_string(),
            ])
        );
    }

    #[test]
    fn optional_vec_string_field_rejects_per_item_non_string_with_typed_shape_and_item_path() {
        // The per-item string-axis cousin of the required-peer
        // `VecFlagsSpec::vec_bool_field_rejects_per_item_non_bool_...`
        // canonical rejection. Post-lift the failure carries the
        // item index (1) alongside the same
        // `ExpectedKwargShape::String` + `SexpShape::Int` pair the
        // required peer emits — authoring surfaces pattern-match on
        // the SAME `LispError::TypeMismatch { form: Item {..},
        // expected: String, got: Int }` variant they already bind
        // for the required peer's per-item gate, not on a serde
        // substring. Pre-lift the derive routed
        // `Option<Vec<String>>` through `Kind::OptionalDeserialize`
        // and the same failure surfaced as
        // `KwargDeserialize { message: "invalid type: integer 5,
        // expected a string at path .1" }` — a regression that
        // reverted `Kind::OptionalVecString` back to
        // `Kind::OptionalDeserialize` would surface here as the
        // substrate-typed `TypeMismatch` variant absent from the
        // error's tag (replaced by `KwargDeserialize`).
        let forms = read(r#"(defoptvectags :tags ("ok" 5))"#).expect("reads");
        let err = OptVecTagsSpec::compile_from_sexp(&forms[0])
            .expect_err("5 is not a string and must not parse");
        let LispError::TypeMismatch {
            form,
            expected,
            got,
        } = &err
        else {
            panic!("expected TypeMismatch (typed atom-family gate), got {err:?}");
        };
        assert_eq!(form, &KwargPath::item("tags", 1));
        assert_eq!(*expected, ExpectedKwargShape::String);
        assert_eq!(*got, SexpShape::Int);
        assert_eq!(
            err.to_string(),
            "compile error in :tags[1]: expected string, got int",
        );
    }

    #[test]
    fn optional_vec_string_field_rejects_present_scalar_with_typed_outer_shape() {
        // A PRESENT non-list kwarg (`:tags "solo"`) rejects with the
        // SAME `expected list of strings` outer-shape diagnostic the
        // required peer `extract_string_list` emits — the present-vs-
        // absent bifurcation happens BEFORE the required peer's outer
        // gate, so the outer-shape rejection variant is byte-identical
        // to the required peer's.
        let forms = read(r#"(defoptvectags :tags "solo")"#).expect("reads");
        let err = OptVecTagsSpec::compile_from_sexp(&forms[0])
            .expect_err("a scalar string is not a list of strings and must not parse");
        let LispError::TypeMismatch {
            form,
            expected,
            got,
        } = &err
        else {
            panic!("expected TypeMismatch (outer atom-family gate), got {err:?}");
        };
        assert_eq!(form, &KwargPath::named("tags"));
        assert_eq!(*expected, ExpectedKwargShape::ListOfStrings);
        assert_eq!(*got, SexpShape::String);
    }

    /// Optional-vec-bool derive fixture — closes the
    /// present-vs-absent hole on the bool-vec axis. Pre-lift the
    /// derive routed `Option<Vec<bool>>` through
    /// `Kind::OptionalDeserialize` (the universal serde bridge —
    /// `classify_option` had no arm for `Option<Vec<bool>>` and fell
    /// through to the catch-all), so a per-item non-bool element on
    /// `:flags (list #t "yes")` into an `Option<Vec<bool>>` field
    /// surfaced as a mystery
    /// `KwargDeserialize { message: "invalid type: string \"yes\",
    /// expected a boolean at path .1" }` substring rather than as
    /// the typed
    /// `TypeMismatch { form: Item { key: "flags", idx: 1 },
    /// expected: Bool, got: String }` its REQUIRED peer
    /// (`flags: Vec<bool>`) already emits at the atom shape gate.
    /// Post-lift the derive routes `Option<Vec<bool>>` to
    /// `extract_optional_bool_list`, which delegates its present-
    /// branch decode to the SAME `extract_bool_list` the required
    /// peer binds; the two peers on the same axis now speak the
    /// SAME typed rejection vocabulary. Sibling posture to
    /// `OptVecTagsSpec` on the optional-vec-string axis and
    /// `VecFlagsSpec` on the required-vec-bool axis — all three
    /// substrate primitives closed the SAME class of per-item serde-
    /// bridge leak.
    #[derive(DeriveTataraDomain, Serialize, Debug, PartialEq)]
    #[tatara(keyword = "defoptvecflags")]
    struct OptVecFlagsSpec {
        flags: Option<Vec<bool>>,
    }

    #[test]
    fn optional_vec_bool_field_absent_kwarg_parses_as_none() {
        // `:flags` absent → `Ok(None)` — distinct from a present empty
        // list (`:flags ()` → `Ok(Some(vec![]))`). The load-bearing
        // present-vs-absent bifurcation an `Option<Vec<T>>` field
        // preserves that a `Vec<T>` field collapses.
        let forms = read(r"(defoptvecflags)").expect("reads");
        let spec = OptVecFlagsSpec::compile_from_sexp(&forms[0])
            .expect("absent optional list must parse as None");
        assert_eq!(spec.flags, None);
    }

    #[test]
    fn optional_vec_bool_field_present_empty_parses_as_some_empty_vec() {
        // `:flags ()` → `Ok(Some(Vec::new()))` — a PRESENT empty list
        // is Some(vec![]), not None. Sibling to the absent-kwarg pin.
        let forms = read(r"(defoptvecflags :flags ())").expect("reads");
        let spec = OptVecFlagsSpec::compile_from_sexp(&forms[0])
            .expect("present empty list must parse as Some(vec![])");
        assert_eq!(spec.flags, Some(Vec::<bool>::new()));
    }

    #[test]
    fn optional_vec_bool_field_parses_all_bool_list_through_the_per_item_atom_gate() {
        // Happy-path: a present list of bools decodes byte-identically
        // to the required peer `Vec<bool>` field's decode, wrapped in
        // `Some`.
        let forms = read(r"(defoptvecflags :flags (#t #f #t))").expect("reads");
        let spec = OptVecFlagsSpec::compile_from_sexp(&forms[0])
            .expect("all-bool per-item values must parse");
        assert_eq!(spec.flags, Some(vec![true, false, true]));
    }

    #[test]
    fn optional_vec_bool_field_rejects_per_item_non_bool_with_typed_shape_and_item_path() {
        // The per-item bool-axis cousin of the required-peer
        // `VecFlagsSpec::vec_bool_field_rejects_per_item_non_bool_...`
        // canonical rejection. Post-lift the failure carries the
        // item index (1) alongside the same
        // `ExpectedKwargShape::Bool` + `SexpShape::String` pair the
        // required peer emits — authoring surfaces pattern-match on
        // the SAME `LispError::TypeMismatch { form: Item {..},
        // expected: Bool, got: String }` variant they already bind
        // for the required peer's per-item gate, not on a serde
        // substring. Pre-lift the derive routed
        // `Option<Vec<bool>>` through `Kind::OptionalDeserialize`
        // and the same failure surfaced as
        // `KwargDeserialize { message: "invalid type: string \"yes\",
        // expected a boolean at path .1" }` — a regression that
        // reverted `Kind::OptionalVecBool` back to
        // `Kind::OptionalDeserialize` would surface here as the
        // substrate-typed `TypeMismatch` variant absent from the
        // error's tag (replaced by `KwargDeserialize`).
        let forms = read(r#"(defoptvecflags :flags (#t "yes" #f))"#).expect("reads");
        let err = OptVecFlagsSpec::compile_from_sexp(&forms[0])
            .expect_err("\"yes\" is not a bool and must not parse");
        let LispError::TypeMismatch {
            form,
            expected,
            got,
        } = &err
        else {
            panic!("expected TypeMismatch (typed atom-family gate), got {err:?}");
        };
        assert_eq!(form, &KwargPath::item("flags", 1));
        assert_eq!(*expected, ExpectedKwargShape::Bool);
        assert_eq!(*got, SexpShape::String);
        assert_eq!(
            err.to_string(),
            "compile error in :flags[1]: expected bool, got string",
        );
    }

    #[test]
    fn optional_vec_bool_field_rejects_present_scalar_with_typed_outer_shape() {
        // A PRESENT non-list kwarg (`:flags #t`) rejects with the
        // SAME axis-typed `expected list of bools` outer-shape
        // diagnostic the required peer `extract_bool_list` emits
        // through its `<bool>::LIST_SHAPE` per-axis trait-const
        // override — the present-vs-absent bifurcation happens
        // BEFORE the required peer's outer gate, so the outer-shape
        // rejection variant is byte-identical to the required peer's.
        // The outer label is [`ExpectedKwargShape::ListOfBools`] —
        // the element-typed refinement paired with the sibling
        // [`ExpectedKwargShape::ListOfStrings`] on the atom-family
        // non-numeric list-of-atoms surface; the two overrides
        // together sharpen the outer diagnostic at BOTH non-numeric
        // atom-family list-family sites past the pre-lift ambiguous
        // `expected list, got bool` bytes into the axis-typed
        // `expected list of bools, got bool`.
        let forms = read(r"(defoptvecflags :flags #t)").expect("reads");
        let err = OptVecFlagsSpec::compile_from_sexp(&forms[0])
            .expect_err("a scalar bool is not a list of bools and must not parse");
        let LispError::TypeMismatch {
            form,
            expected,
            got,
        } = &err
        else {
            panic!("expected TypeMismatch (outer atom-family gate), got {err:?}");
        };
        assert_eq!(form, &KwargPath::named("flags"));
        assert_eq!(*expected, ExpectedKwargShape::ListOfBools);
        assert_eq!(*got, SexpShape::Bool);
        assert_eq!(
            err.to_string(),
            "compile error in :flags: expected list of bools, got bool",
        );
    }

    /// Optional-vec-numeric derive fixture — closes the present-vs-
    /// absent hole on both numeric-narrowing axes (int + float). Pre-
    /// lift the derive routed `Option<Vec<u16>>` / `Option<Vec<f32>>`
    /// through `Kind::OptionalDeserialize` (the universal serde bridge
    /// — `classify_option` had no arm for `Option<Vec<T>>` on the
    /// numeric axes and fell through to the catch-all), so a per-item
    /// out-of-range value on `:ports (list 80 70000)` into an
    /// `Option<Vec<u16>>` field surfaced as a mystery
    /// `KwargDeserialize { message: "invalid value: integer 70000,
    /// expected u16 at path .1" }` substring rather than as the typed
    /// `KwargOutOfRange { form: Item { key: "ports", idx: 1 },
    /// target: U16, value: Int(70_000) }` its REQUIRED peer
    /// (`ports: Vec<u16>` on `VecPortsSpec`) already emits at the
    /// per-item narrowing gate.
    ///
    /// Post-lift the derive routes `Option<Vec<u16>>` to
    /// `extract_optional_int_list_narrowed::<u16>` and
    /// `Option<Vec<f32>>` to `extract_optional_float_list_narrowed
    /// ::<f32>`, both one-line delegates onto the SAME
    /// `extract_optional_narrowed_list::<W, T>` combinator whose
    /// present-branch decode routes through the SAME
    /// `extract_narrowed_list::<W, T>` primitive the required peer
    /// `VecPortsSpec` binds; the two peers on the same axis now speak
    /// the SAME typed rejection vocabulary. Sibling posture to
    /// `OptVecFlagsSpec` on the optional-vec-bool axis, `OptVecTagsSpec`
    /// on the optional-vec-string axis, and `VecPortsSpec` on the
    /// required-vec-numeric axis — the four together complete the
    /// Cartesian product across {scalar, optional-scalar, vec,
    /// optional-vec} × {String, Bool, Int, Float} for the atom-family
    /// axes at the derive dispatch surface.
    #[derive(DeriveTataraDomain, Serialize, Debug, PartialEq)]
    #[tatara(keyword = "defoptvecports")]
    struct OptVecPortsSpec {
        ports: Option<Vec<u16>>,
        scales: Option<Vec<f32>>,
    }

    #[test]
    fn optional_vec_numeric_fields_absent_kwarg_parses_as_none() {
        // Both fields absent → both `None`. The load-bearing present-
        // vs-absent bifurcation an `Option<Vec<T>>` field preserves
        // that a `Vec<T>` field collapses. Sibling to the bool-axis /
        // string-axis absent-kwarg tests.
        let forms = read(r"(defoptvecports)").expect("reads");
        let spec = OptVecPortsSpec::compile_from_sexp(&forms[0])
            .expect("absent optional numeric-vec fields must parse as None");
        assert_eq!(spec.ports, None);
        assert_eq!(spec.scales, None);
    }

    #[test]
    fn optional_vec_numeric_fields_present_empty_parses_as_some_empty_vec() {
        // Both fields present as empty lists → both
        // `Some(Vec::new())`. The `Vec<T>` peer would collapse this to
        // `Ok(Vec::new())`, indistinguishable from the absent case;
        // the `Option<Vec<T>>` peer preserves the operator's intent
        // ("this kwarg is bound to an empty list"). Sibling to the
        // bool-axis / string-axis present-empty pins.
        let forms = read(r"(defoptvecports :ports () :scales ())").expect("reads");
        let spec = OptVecPortsSpec::compile_from_sexp(&forms[0])
            .expect("present empty optional numeric-vec fields must parse as Some(vec![])");
        assert_eq!(spec.ports, Some(Vec::<u16>::new()));
        assert_eq!(spec.scales, Some(Vec::<f32>::new()));
    }

    #[test]
    fn optional_vec_numeric_fields_parse_in_range_lists_through_the_per_item_narrowing_gate() {
        // Happy-path: both fields present as in-range lists decode
        // byte-identically to their required-peer `Vec<T>` fields'
        // decodes, wrapped in `Some`. Sibling to
        // `VecPortsSpec::vec_u16_field_parses_in_range_list_through_
        // the_per_item_narrowing_gate` at the outer present-vs-
        // absent axis.
        let forms =
            read(r"(defoptvecports :ports (80 443 8080) :scales (1.0 2.5))").expect("reads");
        let spec = OptVecPortsSpec::compile_from_sexp(&forms[0])
            .expect("in-range per-item values must parse");
        assert_eq!(spec.ports, Some(vec![80u16, 443, 8080]));
        let scales = spec.scales.as_ref().expect("present list must be Some");
        assert_eq!(scales.len(), 2);
        assert!((scales[0] - 1.0_f32).abs() < f32::EPSILON);
        assert!((scales[1] - 2.5_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn optional_vec_u16_field_rejects_per_item_out_of_range_with_typed_width_and_item_path() {
        // The per-item cousin of the scalar `u16` canonical rejection
        // and the required-peer
        // `VecPortsSpec::vec_u16_field_rejects_per_item_out_of_range_
        // with_typed_width_and_item_path` at the outer present-vs-
        // absent axis. Post-lift the failure carries the item index
        // (1) alongside the same `NumericWidth::U16` /
        // `NumericLiteral::Int(70_000)` pair the required peer emits
        // — authoring surfaces pattern-match on the SAME
        // `LispError::KwargOutOfRange { form: Item {..}, target,
        // value }` variant they already bind for the required peer's
        // per-item narrowing gate, not on a serde substring.
        //
        // Pre-lift the derive routed `Option<Vec<u16>>` through
        // `Kind::OptionalDeserialize` (the universal serde bridge —
        // `classify_option` had no arm for `Option<Vec<u16>>` and
        // fell through to the catch-all), and the same failure
        // surfaced as `KwargDeserialize { message: "invalid value:
        // integer 70000, expected u16 at path .1" }` — a regression
        // that reverted `Kind::OptionalVecInt("u16")` back to
        // `Kind::OptionalDeserialize` would surface here as the
        // substrate-typed `KwargOutOfRange` variant absent from the
        // error's tag (replaced by `KwargDeserialize`).
        let forms = read(r"(defoptvecports :ports (80 70000) :scales (1.0))").expect("reads");
        let err = OptVecPortsSpec::compile_from_sexp(&forms[0])
            .expect_err("70000 is out of range for u16 and must not parse");
        let LispError::KwargOutOfRange {
            form,
            target,
            value,
        } = &err
        else {
            panic!("expected KwargOutOfRange (typed narrowing gate), got {err:?}");
        };
        assert_eq!(form, &KwargPath::item("ports", 1));
        assert_eq!(*target, NumericWidth::U16);
        assert_eq!(*value, NumericLiteral::Int(70_000));
        assert_eq!(
            err.to_string(),
            "compile error in :ports[1]: 70000 is out of range for u16",
        );
    }

    #[test]
    fn optional_vec_f32_field_rejects_per_item_lossy_to_inf_with_typed_width_and_item_path() {
        // Float-axis peer of the u16 optional-vec test above — the
        // third `:scales` item lossy-to-inf overflows f32 (finite
        // input, infinite output). Same `KwargPath::Item` +
        // `NumericWidth::F32` + `NumericLiteral::Float(_)` rejection
        // shape the required-peer
        // `VecPortsSpec::vec_f32_field_rejects_per_item_lossy_to_inf_
        // with_typed_width_and_item_path` and the scalar
        // `extract_float_narrowed::<f32>` peer emit — the outer
        // `Option` wrap does not drift the per-item narrowing gate.
        let forms = read(r"(defoptvecports :ports (80) :scales (1.0 2.0 1.0e300))").expect("reads");
        let err = OptVecPortsSpec::compile_from_sexp(&forms[0])
            .expect_err("1.0e300 overflows f32 and must not parse");
        let LispError::KwargOutOfRange {
            form,
            target,
            value,
        } = &err
        else {
            panic!("expected KwargOutOfRange (typed narrowing gate), got {err:?}");
        };
        assert_eq!(form, &KwargPath::item("scales", 2));
        assert_eq!(*target, NumericWidth::F32);
        assert!(
            matches!(value, NumericLiteral::Float(x) if (*x - 1.0e300).abs() < f64::EPSILON),
            "diagnostic must echo the author's own literal, got {value:?}",
        );
    }

    #[test]
    fn optional_vec_numeric_field_rejects_present_scalar_with_typed_outer_shape() {
        // A PRESENT non-list kwarg (`:ports 80`) rejects with the
        // SAME axis-typed outer-shape diagnostic the required peer
        // `extract_int_list_narrowed` emits through the
        // `<i64>::LIST_SHAPE = ListOfInts` per-axis trait-const
        // override — the present-vs-absent bifurcation happens
        // BEFORE the required peer's outer gate, so the outer-shape
        // rejection variant is byte-identical to the required peer's.
        // The outer label is `ExpectedKwargShape::ListOfInts` (the
        // element-typed refinement), NOT the ambiguous bare `List`
        // default any wide-numeric list-family site emitted pre-lift.
        // A regression that reverted the `<i64>::LIST_SHAPE` override
        // to the trait default surfaces here as `expected =
        // ExpectedKwargShape::List`, and the failure-mode names the
        // exact axis whose override degraded — sibling to the bool
        // axis's peer pin
        // `optional_vec_bool_field_rejects_present_scalar_with_typed_outer_shape`.
        let forms = read(r"(defoptvecports :ports 80 :scales ())").expect("reads");
        let err = OptVecPortsSpec::compile_from_sexp(&forms[0])
            .expect_err("a scalar int is not a list of ints and must not parse");
        let LispError::TypeMismatch {
            form,
            expected,
            got,
        } = &err
        else {
            panic!("expected TypeMismatch (outer atom-family gate), got {err:?}");
        };
        assert_eq!(form, &KwargPath::named("ports"));
        assert_eq!(*expected, ExpectedKwargShape::ListOfInts);
        assert_eq!(*got, SexpShape::Int);
        assert_eq!(
            err.to_string(),
            "compile error in :ports: expected list of ints, got int",
            "the axis-typed refinement label must surface in the rendered \
             diagnostic through the <i64>::LIST_SHAPE = ListOfInts per-axis \
             trait-const dispatch — a regression that degrades this label to \
             the bare `List` default surfaces here as a byte drift",
        );
    }

    #[test]
    fn optional_vec_scales_field_rejects_present_scalar_with_typed_outer_shape() {
        // Wide-float-axis peer of
        // `optional_vec_numeric_field_rejects_present_scalar_with_typed_outer_shape`
        // — a PRESENT scalar-float kwarg (`:scales 1.0`) rejects
        // through the `<f64>::LIST_SHAPE = ListOfNumbers` per-axis
        // trait-const override, emitting the axis-typed refinement
        // `ExpectedKwargShape::ListOfNumbers` at the outer-shape
        // gate. The `"numbers"` (not `"floats"`) naming mirrors the
        // scalar-axis Number vs Float posture — the per-item gate
        // accepts BOTH float atoms and int atoms through
        // Sexp::as_float, so the outer-shape label names the union
        // rather than the narrower `"floats"`. Together with the
        // int-axis peer this pins the FOUR-axis carving square on
        // the atom-family list-family outer-shape surface
        // (ListOfStrings on <&str>, ListOfBools on <bool>,
        // ListOfInts on <i64>, ListOfNumbers on <f64>).
        let forms = read(r"(defoptvecports :ports (80) :scales 1.0)").expect("reads");
        let err = OptVecPortsSpec::compile_from_sexp(&forms[0])
            .expect_err("a scalar float is not a list of numbers and must not parse");
        let LispError::TypeMismatch {
            form,
            expected,
            got,
        } = &err
        else {
            panic!("expected TypeMismatch (outer atom-family gate), got {err:?}");
        };
        assert_eq!(form, &KwargPath::named("scales"));
        assert_eq!(*expected, ExpectedKwargShape::ListOfNumbers);
        assert_eq!(*got, SexpShape::Float);
        assert_eq!(
            err.to_string(),
            "compile error in :scales: expected list of numbers, got float",
            "the axis-typed refinement label must surface in the rendered \
             diagnostic through the <f64>::LIST_SHAPE = ListOfNumbers per-axis \
             trait-const dispatch — a regression that degrades this label to \
             the bare `List` default surfaces here as a byte drift",
        );
    }

    #[test]
    fn vec_f32_field_rejects_per_item_lossy_to_inf_with_typed_width_and_item_path() {
        // Float-axis peer of
        // `vec_u16_field_rejects_per_item_out_of_range_with_typed_width_and_item_path`
        // — the third item lossy-to-inf overflows f32 (finite input,
        // infinite output). Same `KwargPath::Item` +
        // `NumericWidth::F32` + `NumericLiteral::Float(_)` rejection
        // shape the scalar `extract_float_narrowed::<f32>` peer emits
        // on `:scale 1.0e300`.
        let forms = read(r"(defvecports :ports (80) :scales (1.0 2.0 1.0e300))").expect("reads");
        let err = VecPortsSpec::compile_from_sexp(&forms[0])
            .expect_err("1.0e300 overflows f32 and must not parse");
        let LispError::KwargOutOfRange {
            form,
            target,
            value,
        } = &err
        else {
            panic!("expected KwargOutOfRange, got {err:?}");
        };
        assert_eq!(form, &KwargPath::item("scales", 2));
        assert_eq!(*target, NumericWidth::F32);
        assert!(
            matches!(value, NumericLiteral::Float(x) if (*x - 1.0e300).abs() < f64::EPSILON),
            "diagnostic must echo the author's own literal, got {value:?}",
        );
    }

    #[test]
    fn derive_errors_on_wrong_head() {
        let forms = read(r#"(not-a-monitor :name "x")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(format!("{err}").contains("expected (defmonitor"));
    }

    #[test]
    fn derive_rejects_unknown_keyword() {
        // Typed-entry invariant (THEORY.md §II.1.1) — a typo'd keyword
        // must surface as an error before the value exists, not parse
        // silently with the field unset.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tthreshold 0.99)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("tthreshold"),
            "error must name the offending keyword, got: {msg}"
        );
        assert!(
            msg.contains("unknown keyword"),
            "error must label the failure mode, got: {msg}"
        );
    }

    #[test]
    fn derive_unknown_keyword_lists_allowed_set() {
        // The error message includes the allowed-keyword set so the
        // operator can fix the typo without consulting the source.
        let forms = read(r#"(defmonitor :name "x" :ttreshold 0.99)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":threshold"),
            "expected :threshold listed: {msg}"
        );
        assert!(msg.contains(":query"), "expected :query listed: {msg}");
        assert!(msg.contains(":name"), "expected :name listed: {msg}");
    }

    #[test]
    fn reject_unknown_kwargs_helper_passes_when_all_known() {
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold 0.5)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &[
            "name",
            "query",
            "threshold",
            "window-seconds",
            "tags",
            "enabled",
        ];
        assert!(reject_unknown_kwargs(&kw, allowed).is_ok());
    }

    #[test]
    fn reject_unknown_kwargs_helper_errors_on_extra() {
        let forms = read(r#"(defmonitor :name "x" :ghost "boo")"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &["name"];
        let err = reject_unknown_kwargs(&kw, allowed).unwrap_err();
        assert!(format!("{err}").contains("ghost"));
    }

    #[test]
    fn registry_dispatches_by_keyword() {
        register::<MonitorSpec>();
        assert!(registered_keywords().contains(&"defmonitor"));
        let handler = lookup("defmonitor").expect("registered");
        assert_eq!(handler.keyword, "defmonitor");
        let forms = read(r#"(ignored :name "prom" :query "q" :threshold 0.5)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let json = (handler.compile)(&args[1..]).unwrap();
        assert_eq!(json["name"], "prom");
        assert_eq!(json["query"], "q");
        assert_eq!(json["threshold"], 0.5);
    }

    // ── extract_via_serde / extract_optional_via_serde / extract_vec_via_serde ──
    //
    // These helpers used to live as three inline `quote!` blocks in
    // tatara-lisp-derive. Pinning their behavior here means a hand-written
    // `TataraDomain` impl can rely on the same contract the derive uses,
    // and a regression that re-inlines the boilerplate fails-loudly here
    // before it fans out.

    #[derive(Deserialize, Debug, PartialEq)]
    enum Severity {
        Info,
        Warning,
        Critical,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct EscalationStep {
        notify_ref: String,
        wait_minutes: Option<i64>,
    }

    fn kwargs_of(src: &str) -> Vec<Sexp> {
        // `(_ :k v :k v …)` — strip the head, return the kwargs slice.
        let forms = read(src).unwrap();
        let list = forms[0].as_list().unwrap();
        list[1..].to_vec()
    }

    #[test]
    fn extract_via_serde_parses_enum_from_symbol() {
        // `:level Critical` — bare symbol → enum discriminant via the
        // sexp_to_json bridge → serde Deserialize.
        let args = kwargs_of("(_ :level Critical)");
        let kw = parse_kwargs(&args).unwrap();
        let s: Severity = extract_via_serde(&kw, "level").unwrap();
        assert_eq!(s, Severity::Critical);
    }

    #[test]
    fn extract_via_serde_parses_nested_struct_from_kwargs_list() {
        let args = kwargs_of(r#"(_ :step (:notify-ref "oncall" :wait-minutes 5))"#);
        let kw = parse_kwargs(&args).unwrap();
        let s: EscalationStep = extract_via_serde(&kw, "step").unwrap();
        assert_eq!(
            s,
            EscalationStep {
                notify_ref: "oncall".into(),
                wait_minutes: Some(5),
            }
        );
    }

    #[test]
    fn extract_via_serde_missing_required_errors() {
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<Severity>(&kw, "level").unwrap_err();
        let msg = format!("{err}");
        // The `required` helper supplies the missing-kwarg message — same
        // path the typed extractors use, so authoring tools render
        // missing kwargs uniformly across both fallthroughs.
        assert!(
            msg.contains(":level"),
            "missing-kwarg error must name the kwarg, got: {msg}"
        );
        assert!(
            msg.contains("required"),
            "expected 'required' in missing-kwarg error, got: {msg}"
        );
    }

    #[test]
    fn extract_via_serde_deserialize_failure_labels_keyword() {
        // `:level NotASeverity` — well-formed Sexp, ill-formed enum.
        // The error must point at `:level` so the operator can fix the
        // typo without inspecting the source twice. Bind on the
        // structural `LispError::KwargDeserialize { key, idx: None,
        // message }` variant — pinning the variant identity AND `idx:
        // None` (no item index for the scalar path) makes the
        // typed-entry `from_value` rejection mode load-bearing in the
        // type system; the legacy `Compile`-shaped substring-match on
        // `":level"` / `"deserialize:"` is preserved as a separate
        // assertion below for substring-grep consumers.
        let args = kwargs_of("(_ :level NotASeverity)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<Severity>(&kw, "level").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::KwargDeserialize {
                    path: KwargPath::Named(ref key),
                    ref message,
                } if key == "level" && !message.is_empty()
            ),
            "expected KwargDeserialize {{ path: KwargPath::Named(\"level\"), .. }}, got {err:?}"
        );
        let msg = format!("{err}");
        assert!(
            msg.contains(":level"),
            "deserialize error must name the kwarg, got: {msg}"
        );
        assert!(
            msg.contains("deserialize:"),
            "expected 'deserialize:' label, got: {msg}"
        );
    }

    #[test]
    fn extract_optional_via_serde_returns_none_when_absent() {
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let s: Option<Severity> = extract_optional_via_serde(&kw, "level").unwrap();
        assert!(s.is_none());
    }

    #[test]
    fn extract_optional_via_serde_returns_some_when_present() {
        let args = kwargs_of("(_ :level Warning)");
        let kw = parse_kwargs(&args).unwrap();
        let s: Option<Severity> = extract_optional_via_serde(&kw, "level").unwrap();
        assert_eq!(s, Some(Severity::Warning));
    }

    // ── extract_optional_via_serde delegation-identity pins ─────────
    //
    // Post-lift `extract_optional_via_serde` is `optional_from_required
    // (kw, key, extract_via_serde::<T>)` — the SCALAR universal-serde
    // peer of `extract_optional_vec_via_serde` on the same substrate
    // primitive. The two peers on the universal-serde axis (scalar +
    // list) share ONE rejection vocabulary; the two pins below pin the
    // shared-primitive delegation at two behavioral corners the earlier
    // returns_none / returns_some tests don't:
    //   * present-arm output matches `extract_via_serde` byte-
    //     identically modulo the `Some(_)` wrap (proves the present
    //     branch delegates to the required peer verbatim);
    //   * present-but-non-decodable output matches
    //     `extract_via_serde`'s error rendering byte-identically
    //     (proves the two peers on the scalar-serde axis speak the
    //     SAME rejection vocabulary at the `KwargPath::Named(key)`
    //     path root).

    #[test]
    fn extract_optional_via_serde_present_arm_matches_extract_via_serde_byte_identically() {
        // A present, well-formed kwarg on the scalar universal-serde
        // axis must decode byte-identically to what the required peer
        // `extract_via_serde` returns, wrapped in `Some(_)`. Delegating
        // to the required peer proves the wrapping is transparent —
        // the primitive does NOT decorate, transform, or filter the
        // required extractor's output; it only wraps. A regression
        // that spliced a per-peer projection into the present arm
        // (e.g. a `.filter(|_| ...)` that suppressed some values into
        // `None`) would surface here as inequality.
        let args = kwargs_of("(_ :level Warning)");
        let kw = parse_kwargs(&args).unwrap();
        let via_primitive: Option<Severity> = extract_optional_via_serde(&kw, "level").unwrap();
        let via_required: Severity = extract_via_serde(&kw, "level").unwrap();
        assert_eq!(via_primitive, Some(via_required));
    }

    #[test]
    fn extract_optional_via_serde_present_arm_forwards_required_rejection_on_decode_failure() {
        // Present-but-non-decodable `:level 5` (an int where the enum
        // `Severity` expects a symbol) — the primitive must forward the
        // SAME `LispError::KwargDeserialize { path: KwargPath::Named
        // ("level"), .. }` variant the required peer emits at the
        // shared `from_value_with_path` bridge. Delegating to
        // `extract_via_serde` and comparing the rendered message pins
        // that the two peers on the scalar universal-serde axis share
        // ONE rejection vocabulary — a regression that wrapped a
        // present sexp in `Some(_)` without decoding it (a permissive
        // posture that bypassed the required extractor's from_value
        // bridge) would surface here as `Ok(Some(_))` instead of an
        // error; a regression that projected the failure through a
        // different `KwargPath` variant (e.g. `KwargPath::Item {
        // idx: 0 }` from a stray list-family shim) would surface as
        // inequality of the rendered messages.
        let args = kwargs_of("(_ :level 5)");
        let kw = parse_kwargs(&args).unwrap();
        let via_primitive_err =
            extract_optional_via_serde::<Severity>(&kw, "level").expect_err("5 is not a Severity");
        let via_required_err =
            extract_via_serde::<Severity>(&kw, "level").expect_err("5 is not a Severity");
        assert!(matches!(
            via_primitive_err,
            LispError::KwargDeserialize {
                path: KwargPath::Named(ref k),
                ..
            } if k == "level"
        ));
        assert_eq!(
            type_err_message(via_primitive_err),
            type_err_message(via_required_err),
        );
    }

    #[test]
    fn extract_vec_via_serde_returns_empty_when_absent() {
        // Absent-kwarg → empty `Vec` — same semantics `Vec<String>` gets
        // through `extract_string_list`. Authoring surfaces can rely on
        // "no entry == empty list" without a `#[serde(default)]` dance.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let v: Vec<EscalationStep> = extract_vec_via_serde(&kw, "steps").unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn extract_vec_via_serde_collects_nested_structs() {
        let args = kwargs_of(
            r#"(_ :steps (
                  (:notify-ref "a" :wait-minutes 0)
                  (:notify-ref "b" :wait-minutes 5)
                  (:notify-ref "c")))"#,
        );
        let kw = parse_kwargs(&args).unwrap();
        let v: Vec<EscalationStep> = extract_vec_via_serde(&kw, "steps").unwrap();
        assert_eq!(
            v,
            vec![
                EscalationStep {
                    notify_ref: "a".into(),
                    wait_minutes: Some(0),
                },
                EscalationStep {
                    notify_ref: "b".into(),
                    wait_minutes: Some(5),
                },
                EscalationStep {
                    notify_ref: "c".into(),
                    wait_minutes: None,
                },
            ]
        );
    }

    #[test]
    fn extract_vec_via_serde_rejects_non_list_kwarg() {
        // `:steps "scalar"` — a list-typed kwarg given a scalar must fail
        // with the kwarg name in the form, so the operator sees what to
        // change.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":steps"), "got: {msg}");
        assert!(msg.contains("expected list"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_item_failure_labels_keyword() {
        // First item is well-formed; second item has a typo'd field.
        // The error must still point at `:steps`, even though the
        // failure is inside an item. Bind on the structural
        // `LispError::KwargDeserialize { key, idx: Some(1), message }`
        // variant — pinning `idx: Some(1)` (the failing item index)
        // makes the per-item rejection path structurally distinct from
        // the scalar / `Option<T>` path (`idx: None`); the legacy
        // substring-match on `":steps"` / `"deserialize:"` is preserved
        // as a separate assertion below for substring-grep consumers.
        let args = kwargs_of(
            r#"(_ :steps (
                  (:notify-ref "ok")
                  (:notify-ref 7)))"#,
        );
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::KwargDeserialize {
                    path: KwargPath::Item { ref key, idx: 1 },
                    ref message,
                } if key == "steps" && !message.is_empty()
            ),
            "expected KwargDeserialize {{ path: KwargPath::Item {{ key: \"steps\", idx: 1 }}, .. }}, got {err:?}"
        );
        let msg = format!("{err}");
        assert!(msg.contains(":steps"), "got: {msg}");
        assert!(msg.contains("deserialize:"), "got: {msg}");
    }

    // ── extract_optional_vec_via_serde — the present-vs-absent peer of
    //    extract_vec_via_serde on the universal-serde-fallthrough
    //    list-family surface. Pins the four load-bearing contract
    //    corners the peer inherits from the shared
    //    `optional_from_required` + `extract_vec_via_serde`
    //    composition: absent → `Ok(None)` (no per-item bridge invoked),
    //    present-empty → `Ok(Some(Vec::new()))` (distinguishable from
    //    absent), present-non-list → outer-shape `TypeMismatch`
    //    (matching the required peer at the shared `extract_list`
    //    gate), present-per-item-failure → typed
    //    `KwargDeserialize { path: KwargPath::Item { key, idx }, .. }`
    //    (matching the required peer at the shared per-item bridge).

    #[test]
    fn extract_optional_vec_via_serde_returns_none_when_kwarg_absent() {
        // Absent `:steps` — the primitive must return `Ok(None)`
        // without invoking the per-item serde bridge. A regression that
        // collapsed absent into the required peer's absent-tolerant
        // posture (`Ok(Vec::new())` wrapped in `Some`) would surface
        // here as `Some(vec![])` instead of `None`, silently losing the
        // `None` / `Some(vec![])` distinction the field carries.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let v: Option<Vec<EscalationStep>> = extract_optional_vec_via_serde(&kw, "steps").unwrap();
        assert_eq!(v, None);
    }

    #[test]
    fn extract_optional_vec_via_serde_returns_some_empty_vec_when_kwarg_is_empty_list() {
        // Load-bearing sub-case of the present-arm contract: a PRESENT
        // empty list (`:steps ()`) must wrap as `Some(vec![])`, NOT
        // collapse to `None`. This is the exact distinction the outer
        // `optional_from_required` gate exists to preserve on the
        // universal-serde-fallthrough axis — a regression that inspected
        // the required extractor's result-shape (e.g. `.filter(|v|
        // !v.is_empty())` before wrapping) would surface here as `None`
        // where the operator wrote `()` explicitly.
        let args = kwargs_of("(_ :steps ())");
        let kw = parse_kwargs(&args).unwrap();
        let v: Option<Vec<EscalationStep>> = extract_optional_vec_via_serde(&kw, "steps").unwrap();
        assert_eq!(v, Some(Vec::<EscalationStep>::new()));
    }

    #[test]
    fn extract_optional_vec_via_serde_collects_nested_structs_when_kwarg_present() {
        // Happy path: a present, well-formed list decodes byte-
        // identically to what the required peer `extract_vec_via_serde`
        // returns, wrapped in `Some`. Delegating to the required peer
        // proves the wrapping is transparent — the primitive does NOT
        // decorate, transform, or filter the required extractor's
        // output; it only wraps.
        let args = kwargs_of(
            r#"(_ :steps (
                  (:notify-ref "a" :wait-minutes 0)
                  (:notify-ref "b" :wait-minutes 5)
                  (:notify-ref "c")))"#,
        );
        let kw = parse_kwargs(&args).unwrap();
        let via_primitive: Option<Vec<EscalationStep>> =
            extract_optional_vec_via_serde(&kw, "steps").unwrap();
        let via_required: Vec<EscalationStep> = extract_vec_via_serde(&kw, "steps").unwrap();
        assert_eq!(via_primitive, Some(via_required));
    }

    #[test]
    fn extract_optional_vec_via_serde_rejects_present_non_list_kwarg_via_shared_shape_gate() {
        // Present-but-non-list `:steps "scalar"` — the primitive must
        // forward the SAME `LispError::TypeMismatch` variant the
        // required peer emits at the shared `extract_list` outer-shape
        // gate (`expected list, got string`). Delegating to
        // `extract_vec_via_serde` and comparing the rendered message
        // pins that the two peers share ONE rejection vocabulary on
        // the outer-shape axis — a regression that wrapped a present
        // sexp in `Some(_)` without decoding it (a permissive posture
        // that bypassed the required extractor's outer gate) would
        // surface here as `Ok(Some(...))` instead of an error.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let via_primitive_err = extract_optional_vec_via_serde::<EscalationStep>(&kw, "steps")
            .expect_err("scalar :steps is not a list");
        let via_required_err = extract_vec_via_serde::<EscalationStep>(&kw, "steps")
            .expect_err("scalar :steps is not a list");
        assert!(matches!(via_primitive_err, LispError::TypeMismatch { .. }));
        assert_eq!(
            type_err_message(via_primitive_err),
            type_err_message(via_required_err),
        );
    }

    #[test]
    fn extract_optional_vec_via_serde_present_per_item_shape_mismatch_carries_kwarg_path_item() {
        // The load-bearing gate-closure this extractor exists to
        // enforce: a per-item serde-decode failure inside a PRESENT
        // list on an `Option<Vec<Nested>>` field surfaces as the SAME
        // structural `LispError::KwargDeserialize { path:
        // KwargPath::Item { key, idx }, .. }` variant its required
        // peer emits through the per-item bridge — NOT as a
        // `KwargPath::Named(key)` root with the failing index buried
        // in a serde-substring `at path .1.notifyRef`. Pre-lift the
        // derive routed `Option<Vec<EscalationStep>>` through the
        // universal `extract_optional_via_serde::<Vec<EscalationStep>>`
        // bridge, so the outer serde round-trip landed the path at
        // `KwargPath::Named("steps")` and the per-item index rode a
        // substring inside the message. Post-lift the two peers on the
        // same axis speak the same typed rejection vocabulary, with
        // the failing item's index rides `KwargPath::Item { idx: 1 }`
        // as a pattern-matchable slot.
        //
        // A regression that dropped the per-item path root back to
        // `KwargPath::Named` (e.g. by delegating to
        // `extract_optional_via_serde::<Vec<T>>` instead of
        // `extract_vec_via_serde::<T>` inside the primitive) would
        // surface here as the wrong `KwargPath` variant.
        let args = kwargs_of(
            r#"(_ :steps (
                  (:notify-ref "ok")
                  (:notify-ref 7)))"#,
        );
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_optional_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::KwargDeserialize {
                    path: KwargPath::Item { ref key, idx: 1 },
                    ref message,
                } if key == "steps" && !message.is_empty()
            ),
            "expected KwargDeserialize {{ path: KwargPath::Item {{ key: \"steps\", idx: 1 }}, .. }}, got {err:?}"
        );
    }

    // ── Duplicate-keyword rejection (typed-entry hardening) ─────────────
    //
    // A typo like `:name "x" :name "y"` used to silently overwrite — the
    // last value wins, the operator gets no signal. Same bug class
    // `reject_unknown_kwargs` (commit 2750f39) closed for typo'd kwargs;
    // this closes the dual hole for duplicate kwargs at every nesting
    // level (top-level args, nested struct kwargs, vec item kwargs).
    //
    // Theory anchor: THEORY.md §II.1 invariant 1 (typed entry —
    // "Ill-typed input errors before the value exists").

    #[test]
    fn parse_kwargs_rejects_duplicate_top_level_keyword() {
        let args = kwargs_of(r#"(_ :name "x" :name "y")"#);
        let err = parse_kwargs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":name"),
            "error must name the keyword, got: {msg}"
        );
        assert!(
            msg.contains("duplicate keyword"),
            "expected 'duplicate keyword' label, got: {msg}"
        );
    }

    #[test]
    fn parse_kwargs_accepts_distinct_keywords() {
        // Negative-control: pre-existing flow is preserved.
        let args = kwargs_of(r#"(_ :name "x" :query "q" :threshold 0.5)"#);
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(kw.len(), 3);
    }

    #[test]
    fn extract_via_serde_rejects_duplicate_in_nested_struct() {
        // `:step (:notify-ref "a" :notify-ref "b")` — the duplicate fires
        // during the `sexp_to_json` projection, before serde sees a value.
        let args = kwargs_of(r#"(_ :step (:notify-ref "a" :notify-ref "b"))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<EscalationStep>(&kw, "step").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":notify-ref"),
            "duplicate-in-nested error must name the inner kwarg, got: {msg}"
        );
        assert!(
            msg.contains("duplicate keyword"),
            "expected 'duplicate keyword' label, got: {msg}"
        );
    }

    #[test]
    fn extract_vec_via_serde_rejects_duplicate_in_item() {
        // `:steps ((:notify-ref "a" :notify-ref "b"))` — the duplicate is
        // inside one vec item. Authors get the same diagnostic shape
        // whether the duplicate is at the top level, in a nested struct,
        // or inside a vec item.
        let args = kwargs_of(r#"(_ :steps ((:notify-ref "a" :notify-ref "b")))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":notify-ref"), "got: {msg}");
        assert!(msg.contains("duplicate keyword"), "got: {msg}");
    }

    #[test]
    fn derive_rejects_duplicate_top_level_kwarg() {
        // End-to-end through `#[derive(TataraDomain)]` — silent overwrite
        // is exactly the bug class the typed-entry gate exists to prevent,
        // and every derived domain inherits the rejection by sharing
        // `parse_kwargs`.
        let forms = read(r#"(defmonitor :name "x" :name "y" :query "q" :threshold 0.5)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":name"), "got: {msg}");
        assert!(msg.contains("duplicate"), "got: {msg}");
    }

    #[test]
    fn json_to_sexp_round_trip_does_not_trip_duplicate_check() {
        // The round-trip path used by `rewrite_typed`: a typed value
        // → `serde_json::Value` (unique-keyed) → `Sexp` via `json_to_sexp`
        // → top-level kwargs slice → `parse_kwargs`. The duplicate-check
        // gate must NOT false-positive on this canonical input.
        let original = MonitorSpec {
            name: "x".into(),
            query: "q".into(),
            threshold: 0.5,
            window_seconds: None,
            tags: vec![],
            enabled: None,
        };
        let json = serde_json::to_value(&original).unwrap();
        let sexp = json_to_sexp(&json);
        let args = sexp.as_list().expect("object → kwargs list").to_vec();
        let _kw = parse_kwargs(&args).expect("round-trip kwargs are unique by construction");
    }

    #[test]
    fn sexp_to_json_round_trip_array_unaffected_by_duplicate_check() {
        // Arrays-of-objects round-trip: each object is unique-keyed by
        // virtue of being authored as a `serde_json::Map`. The strict
        // duplicate check must not false-positive on this shape.
        let json = serde_json::json!([
            { "notifyRef": "a", "waitMinutes": 0 },
            { "notifyRef": "b", "waitMinutes": 5 },
        ]);
        let sexp = json_to_sexp(&json);
        let back = sexp_to_json(&sexp).expect("round-trip array must not trip duplicate check");
        // The array is preserved (object key order is stable inside each
        // element because `json_to_sexp` writes kwargs in iteration order
        // and `sexp_to_json` reads them back in the same order).
        assert_eq!(back, json);
    }

    #[test]
    fn sexp_to_json_routes_quote_family_arms_through_as_quote_form_typed_marker() {
        // PATH-UNIFORMITY CONTRACT: the lifted `sexp_to_json` routes
        // its four quote-family arms through `Sexp::as_quote_form()`,
        // discarding the marker and recursing on the inner — the
        // same typed-marker dispatch `sexp_shape` lifts at line 602.
        // Pin the new boundary three ways across `QuoteForm::ALL` so
        // a regression that drifts ONE variant's recurse-on-inner
        // shape from the others (e.g. an arm that mis-routes the
        // marker as the recursion subject, or drops the recursion
        // entirely returning the inner verbatim with the wrapper
        // collapsed to JSON null) fails-loudly here:
        //
        //   (1) sweep `QuoteForm::ALL`, wrapping a non-trivial
        //       kwargs inner (`(:name "payload")`) — the inner
        //       MUST project to the same JSON object regardless of
        //       which quote-family wrapper sits at the outer node.
        //       Catches a regression that mis-routes ONE variant
        //       (e.g. `Sexp::Quote(_)` arm returns `JValue::Null`
        //       instead of recursing) without the others.
        //   (2) sweep `QuoteForm::ALL`, asserting `as_quote_form`'s
        //       typed-marker projection AGREES with the constructor
        //       branch — proves the lifted arm and the algebra
        //       projection share ONE pairing of (Sexp variant,
        //       QuoteForm variant) and a regression that drifts the
        //       pairing surfaces here, not in production.
        //   (3) sweep `QuoteForm::ALL`, asserting
        //       `sexp_to_json(wrap_qf(inner)) ==
        //       sexp_to_json(as_quote_form(wrap_qf(inner)).inner)`
        //       — proves the lifted recursion target IS the
        //       `as_quote_form`-projected inner (not a clone, not a
        //       stale closure binding, not the outer node itself).
        //
        // Sibling posture to `sexp_shape`'s path-uniformity test at
        // line 2203 (the canonical reference shape for this lift).
        use crate::ast::QuoteForm;
        let inner = Sexp::List(vec![Sexp::keyword("name"), Sexp::string("payload")]);
        let expected = sexp_to_json(&inner).expect("inner must serialize cleanly");

        for qf in QuoteForm::ALL {
            let wrapped = qf.wrap(inner.clone());
            let via_lifted =
                sexp_to_json(&wrapped).expect("quote-family wrapper must serialize cleanly");
            assert_eq!(
                via_lifted, expected,
                "sexp_to_json drifted from `sexp_to_json(inner)` at quote-family marker {qf:?}"
            );
            let (marker, projected_inner) = wrapped
                .as_quote_form()
                .expect("quote-family wrapper must project through as_quote_form");
            assert_eq!(
                marker, qf,
                "as_quote_form drifted the typed marker at {qf:?}"
            );
            let via_composed =
                sexp_to_json(projected_inner).expect("projected inner must serialize cleanly");
            assert_eq!(
                via_lifted, via_composed,
                "sexp_to_json drifted from as_quote_form + recurse(inner) at {qf:?}"
            );
        }
    }

    #[test]
    fn sexp_to_json_quote_family_arms_recurse_on_inner_not_outer() {
        // INTENT-PIN: pre-lift the four quote-family arms each
        // pattern-bound `inner` and recursed on `inner`, NEVER on the
        // outer wrapper. Post-lift the recursion target comes from
        // `as_quote_form`'s projection tuple. Pin that this binding
        // semantic is observable end-to-end: a `,@'inner-form` shape
        // — `UnquoteSplice` wrapping `Quote` wrapping a kwargs list
        // — MUST collapse through BOTH wrappers and project the
        // innermost kwargs list as the JSON object, NOT either
        // wrapper's JSON-Null projection. A regression that lifted
        // the recursion onto `s` (the outer wrapper) instead of the
        // `as_quote_form`-projected inner would infinite-loop here
        // (or, with the stack-overflow guard, produce a stack
        // overflow); a regression that collapsed the wrappers
        // independently would skip the inner recursion and emit
        // partial JSON. The double-wrapper exercises BOTH the
        // outermost `as_quote_form` projection AND the recursive
        // step's projection — the same shape `compile_node`'s
        // bytecode emission exercises when a quasi-quote template
        // nests inside another quasi-quote.
        let inner_payload = Sexp::List(vec![Sexp::keyword("k"), Sexp::int(42)]);
        let expected = serde_json::json!({ "k": 42 });
        // ,@'(...) — UnquoteSplice wraps Quote wraps the kwargs list.
        let doubly_wrapped =
            Sexp::UnquoteSplice(Box::new(Sexp::Quote(Box::new(inner_payload.clone()))));
        let via_lifted =
            sexp_to_json(&doubly_wrapped).expect("double-wrapper must serialize cleanly");
        assert_eq!(
            via_lifted, expected,
            "sexp_to_json must recurse THROUGH every quote-family wrapper and \
             project the innermost shape — a regression that lifted recursion \
             onto the outer wrapper would diverge or emit JSON null here"
        );
    }

    #[test]
    fn sexp_to_json_atom_arms_route_through_atom_to_json() {
        // LIFTED-BOUNDARY CONTRACT: pin that the lifted `sexp_to_json`
        // routes its six atomic-payload arms through the typed-algebra
        // method [`crate::ast::Atom::to_json`]. Pre-lift the per-variant
        // body lived inline at six `Sexp::Atom(Atom::<variant>(payload))
        // => JValue::<…>(…)` arms; post-lift the outer arm delegates to
        // `a.to_json()` and the per-variant rendering binds at ONE
        // typed-algebra projection on the `Atom` algebra. A regression
        // that drifts the outer arm (e.g. re-inlines ONE variant's
        // rendering without updating `Atom::to_json`, or returns a
        // wrapping `JValue::Array` instead of delegating) surfaces as
        // an inequality here. The cases sweep all six [`AtomKind`]
        // variants. Sibling-arm shape to the quote-family routing test
        // `sexp_to_json_routes_quote_family_arms_through_as_quote_form_typed_marker`
        // and the Display-axis routing test
        // `sexp_atom_display_arm_routes_through_atom_display_for_every_variant`
        // — all three pin the analogous `Sexp` outer arm routing
        // through a typed algebra projection.
        use crate::ast::Atom;
        let cases: &[Atom] = &[
            Atom::Symbol("name".into()),
            Atom::Keyword("kw".into()),
            Atom::Str("body".into()),
            Atom::Int(7),
            Atom::Int(-3),
            Atom::Float(2.5),
            Atom::Float(1.0),
            Atom::Bool(true),
            Atom::Bool(false),
        ];
        for atom in cases {
            let via_sexp = sexp_to_json(&Sexp::Atom(atom.clone()))
                .expect("atom must serialize cleanly through sexp_to_json");
            let via_atom = atom.to_json();
            assert_eq!(
                via_sexp, via_atom,
                "sexp_to_json drifted from Atom::to_json for {atom:?}"
            );
        }
    }

    #[test]
    fn sexp_to_json_float_nan_propagates_atom_to_json_null_branch() {
        // PATH-UNIFORMITY PIN: the float NaN/∞ → `JValue::Null` branch
        // lives at the typed-algebra primitive `Atom::to_json` post-
        // lift; pin that `sexp_to_json` composes through it without
        // an additional wrapping or short-circuit. A regression that
        // added a separate NaN-handling arm at `sexp_to_json`'s outer
        // dispatch (re-introducing the per-callsite branch the lift
        // retires) would diverge here only if the new arm produced a
        // different value than `Atom::to_json` — by sharing the SAME
        // expected output the test catches both kinds of drift
        // (different value at the outer arm; bypassed delegation).
        use crate::ast::Atom;
        for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let atom = Atom::Float(f);
            let via_sexp = sexp_to_json(&Sexp::Atom(atom.clone()))
                .expect("non-finite float atom must serialize cleanly through sexp_to_json");
            assert_eq!(
                via_sexp,
                atom.to_json(),
                "sexp_to_json NaN/∞ branch drifted from Atom::to_json for {atom:?}"
            );
            assert_eq!(
                via_sexp,
                serde_json::Value::Null,
                "non-finite float MUST collapse to JSON Null at the lifted boundary"
            );
        }
    }

    // ── Type-mismatch diagnostics name both expected and got ───────────
    //
    // Every typed extractor's `expected X` message used to leave the operator
    // to inspect the source to discover what kind of value was actually
    // passed. The `expected X, got Y` shape closes that gap: the diagnostic
    // is structurally complete so an authoring surface (REPL, LSP,
    // tatara-check) can render the mismatch without re-reading the input.
    //
    // `sexp_type_name` is the named primitive doing the projection; pinning
    // its outputs here keeps downstream tooling that matches on the names
    // (e.g., "expected string, got int" → squiggly under the int) safe
    // across versions.

    #[test]
    fn sexp_type_name_covers_every_variant() {
        assert_eq!(sexp_type_name(&Sexp::Nil), "nil");
        assert_eq!(sexp_type_name(&Sexp::symbol("foo")), "symbol");
        assert_eq!(sexp_type_name(&Sexp::keyword("k")), "keyword");
        assert_eq!(sexp_type_name(&Sexp::string("s")), "string");
        assert_eq!(sexp_type_name(&Sexp::int(7)), "int");
        assert_eq!(sexp_type_name(&Sexp::float(7.5)), "float");
        assert_eq!(sexp_type_name(&Sexp::boolean(true)), "bool");
        assert_eq!(sexp_type_name(&Sexp::List(vec![])), "list");
        assert_eq!(sexp_type_name(&Sexp::Quote(Box::new(Sexp::Nil))), "quote");
        assert_eq!(
            sexp_type_name(&Sexp::Quasiquote(Box::new(Sexp::Nil))),
            "quasiquote"
        );
        assert_eq!(
            sexp_type_name(&Sexp::Unquote(Box::new(Sexp::Nil))),
            "unquote"
        );
        assert_eq!(
            sexp_type_name(&Sexp::UnquoteSplice(Box::new(Sexp::Nil))),
            "unquote-splice"
        );
    }

    #[test]
    fn sexp_shape_covers_every_variant() {
        // The typed sister to `sexp_type_name_covers_every_variant` —
        // every `Sexp` variant projects to exactly one `SexpShape`
        // variant. After the typed-slot lift the projection's identity
        // is load-bearing data on `LispError::TypeMismatch.got` and
        // `LispError::NamedFormNonSymbolName.got`; pinning the
        // projection here means a regression that drops a `Sexp`
        // variant's typed `SexpShape` mapping fails-loudly. A future
        // `Sexp` extension (e.g. `Sexp::Vector` for `#(...)` reader
        // syntax) would force a `SexpShape` extension AND a new arm
        // in this test, parallel to how `sexp_type_name_covers_every
        // _variant` pins the legacy `&'static str` projection.
        assert_eq!(sexp_shape(&Sexp::Nil), SexpShape::Nil);
        assert_eq!(sexp_shape(&Sexp::symbol("foo")), SexpShape::Symbol);
        assert_eq!(sexp_shape(&Sexp::keyword("k")), SexpShape::Keyword);
        assert_eq!(sexp_shape(&Sexp::string("s")), SexpShape::String);
        assert_eq!(sexp_shape(&Sexp::int(7)), SexpShape::Int);
        assert_eq!(sexp_shape(&Sexp::float(7.5)), SexpShape::Float);
        assert_eq!(sexp_shape(&Sexp::boolean(true)), SexpShape::Bool);
        assert_eq!(sexp_shape(&Sexp::List(vec![])), SexpShape::List);
        assert_eq!(
            sexp_shape(&Sexp::Quote(Box::new(Sexp::Nil))),
            SexpShape::Quote
        );
        assert_eq!(
            sexp_shape(&Sexp::Quasiquote(Box::new(Sexp::Nil))),
            SexpShape::Quasiquote
        );
        assert_eq!(
            sexp_shape(&Sexp::Unquote(Box::new(Sexp::Nil))),
            SexpShape::Unquote
        );
        assert_eq!(
            sexp_shape(&Sexp::UnquoteSplice(Box::new(Sexp::Nil))),
            SexpShape::UnquoteSplice
        );
    }

    #[test]
    fn sexp_shape_routes_quote_family_arms_through_quote_form_sexp_shape_projection() {
        // PATH-UNIFORMITY CONTRACT: the lifted `sexp_shape` routes its
        // four quote-family arms through `Sexp::as_quote_form()` +
        // `QuoteForm::sexp_shape()`. Pin that the legacy per-arm
        // pairing and the typed-projection composition AGREE bit-for-bit
        // across every quote-family `Sexp` shape — a regression in
        // EITHER projection direction (an `as_quote_form` arm that
        // swaps markers, or a `QuoteForm::sexp_shape` arm that drifts
        // its `SexpShape` mapping) surfaces here immediately. Non-
        // quote-family shapes project to `None` from `as_quote_form`
        // and are out of scope for this contract.
        use crate::ast::QuoteForm;
        let samples = [
            (
                Sexp::Quote(Box::new(Sexp::symbol("payload"))),
                QuoteForm::Quote,
            ),
            (
                Sexp::Quasiquote(Box::new(Sexp::symbol("payload"))),
                QuoteForm::Quasiquote,
            ),
            (
                Sexp::Unquote(Box::new(Sexp::symbol("payload"))),
                QuoteForm::Unquote,
            ),
            (
                Sexp::UnquoteSplice(Box::new(Sexp::symbol("payload"))),
                QuoteForm::UnquoteSplice,
            ),
        ];
        for (sexp, expected_qf) in &samples {
            let via_lifted = sexp_shape(sexp);
            let (qf, _) = sexp
                .as_quote_form()
                .expect("quote-family sample must project through as_quote_form");
            assert_eq!(
                qf, *expected_qf,
                "as_quote_form drifted typed marker at {sexp:?}"
            );
            let via_composed = qf.sexp_shape();
            assert_eq!(
                via_lifted, via_composed,
                "sexp_shape drifted from as_quote_form + QuoteForm::sexp_shape at {sexp:?}"
            );
        }
    }

    #[test]
    fn sexp_type_name_delegates_to_sexp_shape_label_for_every_variant() {
        // Pin that the legacy `&'static str` projection and the typed
        // `SexpShape::label()` projection AGREE on every variant —
        // `sexp_type_name(s) == sexp_shape(s).label()` is the
        // bidirection contract that lets the legacy entry point stay
        // pub for tests that match on rendered substrings while new
        // code constructing `LispError::TypeMismatch` /
        // `NamedFormNonSymbolName` passes through `sexp_shape`
        // directly. A regression that drifts either projection
        // (e.g. typo in `SexpShape::label()` arm, change in
        // `sexp_type_name`'s match) fails-loudly here.
        let samples = [
            Sexp::Nil,
            Sexp::symbol("foo"),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::int(7),
            Sexp::float(7.5),
            Sexp::boolean(true),
            Sexp::List(vec![]),
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::Quasiquote(Box::new(Sexp::Nil)),
            Sexp::Unquote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for s in &samples {
            assert_eq!(
                sexp_type_name(s),
                sexp_shape(s).label(),
                "sexp_type_name and sexp_shape(_).label() must agree for {s:?}"
            );
        }
    }

    #[test]
    fn sexp_witness_pairs_typed_shape_with_display_projection() {
        // Pin the typed joint-identity contract: `sexp_witness(&sexp)`
        // produces a `SexpWitness` whose `shape` is `sexp_shape(&sexp)`
        // AND whose `display` is `sexp.to_string()`. The helper is the
        // single primitive that bundles both halves of the offending-
        // value identity into one owned typed value — every variant
        // slot that takes a `SexpWitness` (currently
        // `SpliceOutsideList.got`; future moves: `NonSymbolUnquoteTarget`,
        // `NonSymbolParam`, `RestParamMissingName`, `DefmacroNonSymbolName`,
        // `DefmacroNonListParams`, `MissingHeadSymbol`) routes through
        // this primitive at the helper boundary. A regression that
        // drops either projection (shape or display) at the helper
        // boundary fails-loudly here.
        let w = sexp_witness(&Sexp::int(5));
        assert_eq!(w.shape, SexpShape::Int);
        assert_eq!(w.display, "5");

        let w = sexp_witness(&Sexp::symbol("notify-ref"));
        assert_eq!(w.shape, SexpShape::Symbol);
        assert_eq!(w.display, "notify-ref");

        let w = sexp_witness(&Sexp::keyword("foo"));
        assert_eq!(w.shape, SexpShape::Keyword);
        assert_eq!(w.display, ":foo");

        let w = sexp_witness(&Sexp::List(vec![
            Sexp::symbol("list"),
            Sexp::int(1),
            Sexp::int(2),
        ]));
        assert_eq!(w.shape, SexpShape::List);
        assert_eq!(w.display, "(list 1 2)");

        let w = sexp_witness(&Sexp::Nil);
        assert_eq!(w.shape, SexpShape::Nil);
        assert_eq!(w.display, "()");
    }

    #[test]
    fn sexp_witness_distinguishes_int_atom_from_symbol_with_same_display() {
        // Pin the structural bifurcation between two `Sexp`s whose
        // `Display` projection is the same string but whose typed
        // `SexpShape` differs. `Sexp::int(5).to_string() == "5"`
        // AND `Sexp::symbol("5").to_string() == "5"` (the reader
        // would reject the symbol `5`, but the AST allows it — the
        // bifurcation here pins that `sexp_witness` carries the
        // structural shape so tools can distinguish them even when
        // the rendered literal is identical). A regression that
        // drops the typed shape from `SexpWitness` would collapse
        // this distinction.
        let w_int = sexp_witness(&Sexp::int(5));
        let w_sym = sexp_witness(&Sexp::symbol("5"));
        assert_eq!(w_int.display, w_sym.display);
        assert_ne!(w_int.shape, w_sym.shape);
        assert_eq!(w_int.shape, SexpShape::Int);
        assert_eq!(w_sym.shape, SexpShape::Symbol);
    }

    fn type_err_message(err: LispError) -> String {
        format!("{err}")
    }

    // ── optional_from_required — the primitive under the SEVEN
    // optional peers (four list-family — `extract_optional_string_list`
    // / `extract_optional_bool_list` / `extract_optional_narrowed_list`
    // / `extract_optional_vec_via_serde` — plus the three scalar-family
    // peers `extract_optional_via_serde` on the universal-serde axis,
    // `extract_optional_narrowed` on the numeric-narrowed axis, and
    // `extract_optional_atom` on the atom-shape axis) tests
    // ────────────────────────────────────────────────────────────────────
    //
    // The primitive owns the "present-vs-absent bifurcation over an
    // absent-tolerant required list-family extractor" shape. The three
    // observable properties every optional-list peer inherits from it:
    //   (1) absent kwarg → `Ok(None)`, and the required extractor is NOT
    //       invoked — so its absent-tolerant `Ok(Vec::new())` posture
    //       cannot leak into the optional peer's result and collapse the
    //       `None` / `Some(vec![])` distinction.
    //   (2) present kwarg → the required extractor's result is wrapped in
    //       `Some(_)` on success.
    //   (3) present kwarg + required extractor rejects → the required's
    //       rejection propagates through unchanged (byte-identical
    //       variant identity + payload), so both peers on the same axis
    //       share ONE rejection vocabulary.
    //
    // Pinning the three properties at the primitive itself — rather than
    // only at the three peer sites through their behavior sweeps — means
    // a regression that changes the primitive's shape (a swap to
    // `optional(kw, key).is_some()` posture — semantically equivalent
    // today but a substrate primitive to name; a hypothetical fault-
    // injection point on the bifurcation gate; a diagnostic promotion
    // that fires the required extractor even on absence) surfaces HERE
    // rather than only at the three peer sites' behavior tests where the
    // primitive is one layer down.

    #[test]
    fn optional_from_required_returns_none_when_kwarg_absent_and_never_invokes_required() {
        // (1) Absent kwarg (`:tags` is NOT present in `(_ :other 1)`) —
        // the primitive must return `Ok(None)` WITHOUT invoking the
        // required extractor. Prove non-invocation with a closure that
        // would panic if called: reaching the closure body is a
        // regression that would collapse an absent kwarg into the
        // required peer's absent-tolerant posture (`Ok(Vec::new())`
        // wrapped in `Some`, losing the `None` / `Some(vec![])`
        // distinction).
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let result: Result<Option<Vec<String>>> = optional_from_required(&kw, "tags", |_, _| {
            panic!("required extractor invoked on absent kwarg")
        });
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn optional_from_required_wraps_required_success_in_some_when_kwarg_present() {
        // (2) Present kwarg (`:tags ("a" "b")` is present) — the
        // primitive must invoke the required extractor and wrap its
        // successful result in `Some(_)`. Delegate to
        // [`extract_string_list`] to prove the wrapping is byte-identical
        // to the required peer's own result — the primitive does NOT
        // decorate, transform, or filter the required extractor's
        // output; it only wraps.
        let args = kwargs_of(r#"(_ :tags ("a" "b"))"#);
        let kw = parse_kwargs(&args).unwrap();
        let via_primitive: Option<Vec<String>> =
            optional_from_required(&kw, "tags", extract_string_list).unwrap();
        let via_required: Vec<String> = extract_string_list(&kw, "tags").unwrap();
        assert_eq!(via_primitive, Some(via_required));
    }

    #[test]
    fn optional_from_required_wraps_present_empty_list_in_some_empty_vec_not_none() {
        // (2) Load-bearing sub-case of the present-arm contract: a
        // PRESENT empty list (`:tags ()`) must wrap as `Some(vec![])`,
        // NOT collapse to `None`. This is the exact distinction the
        // outer `contains_key` gate exists to preserve — a regression
        // that inspected the required extractor's result-shape (e.g.
        // `.filter(|v| !v.is_empty())` before wrapping) would surface
        // here as `None` where the operator wrote `()` explicitly.
        let args = kwargs_of("(_ :tags ())");
        let kw = parse_kwargs(&args).unwrap();
        let result: Option<Vec<String>> =
            optional_from_required(&kw, "tags", extract_string_list).unwrap();
        assert_eq!(result, Some(Vec::<String>::new()));
    }

    #[test]
    fn optional_from_required_propagates_required_error_unchanged() {
        // (3) Present kwarg + required extractor rejects → the required
        // extractor's rejection variant propagates through the
        // primitive byte-identically. Feed a synthetic required
        // extractor that returns a pinned `LispError::MissingKwarg`
        // (regardless of what's actually in the kwarg map — the
        // primitive shouldn't inspect the required extractor's
        // internals, only wire its result); pin that the exact same
        // `LispError` variant + payload comes out. Any wrapping that
        // altered the error path (`.map_err(|e| some_transform(e))`,
        // structural repackaging) would drift the two peers' rejection
        // vocabularies apart.
        let args = kwargs_of("(_ :tags 1)");
        let kw = parse_kwargs(&args).unwrap();
        let injected = missing_kwarg("synthetic");
        let injected_display = format!("{injected}");
        let result: Result<Option<Vec<String>>> =
            optional_from_required(&kw, "tags", |_, _| Err(injected));
        let err =
            result.expect_err("required extractor injected an error; primitive must propagate");
        assert!(
            matches!(err, LispError::MissingKwarg { .. }),
            "expected LispError::MissingKwarg propagated verbatim, got {err:?}",
        );
        assert_eq!(format!("{err}"), injected_display);
    }

    #[test]
    fn optional_from_required_present_arm_forwards_required_rejection_on_shape_mismatch() {
        // (3) End-to-end propagation with a real required extractor:
        // `:tags 7` is present but not a list. `extract_string_list`
        // rejects with `LispError::TypeMismatch { expected:
        // ListOfStrings, got: Int }`; the primitive must forward that
        // exact variant + rendered message. Pinning here rather than
        // only at `extract_optional_string_list_type_err_on_scalar_names_got_int`
        // means a future regression that bypassed the required
        // extractor's outer gate (e.g., a permissive posture that
        // wrapped any present sexp in `Some` without decoding it)
        // surfaces at the primitive layer directly.
        let args = kwargs_of("(_ :tags 7)");
        let kw = parse_kwargs(&args).unwrap();
        let via_primitive_err = optional_from_required(&kw, "tags", extract_string_list)
            .expect_err("scalar :tags is not a list");
        let via_required_err =
            extract_string_list(&kw, "tags").expect_err("scalar :tags is not a list");
        assert!(matches!(via_primitive_err, LispError::TypeMismatch { .. }));
        assert_eq!(
            type_err_message(via_primitive_err),
            type_err_message(via_required_err),
        );
    }

    #[test]
    fn extract_string_type_err_names_got_int() {
        let args = kwargs_of("(_ :name 42)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_string(&kw, "name").unwrap_err());
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
        assert!(msg.contains(":name"), "got: {msg}");
    }

    #[test]
    fn extract_optional_string_type_err_names_got_bool() {
        let args = kwargs_of("(_ :name #t)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_string(&kw, "name").unwrap_err());
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got bool"), "got: {msg}");
    }

    #[test]
    fn extract_int_type_err_names_got_string() {
        let args = kwargs_of(r#"(_ :n "seven")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_int(&kw, "n").unwrap_err());
        assert!(msg.contains("expected int"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_float_type_err_names_got_bool() {
        let args = kwargs_of("(_ :ratio #f)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_float(&kw, "ratio").unwrap_err());
        assert!(msg.contains("expected number"), "got: {msg}");
        assert!(msg.contains("got bool"), "got: {msg}");
    }

    #[test]
    fn extract_bool_type_err_names_got_int() {
        let args = kwargs_of("(_ :enabled 1)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_bool(&kw, "enabled").unwrap_err());
        assert!(msg.contains("expected bool"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_string_list_type_err_on_scalar_names_got_string() {
        // `:tags "scalar"` — list-typed kwarg given a scalar. The error
        // names the actual shape so the operator sees the mismatch
        // structurally.
        let args = kwargs_of(r#"(_ :tags "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_string_list(&kw, "tags").unwrap_err());
        assert!(msg.contains("expected list of strings"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_string_list_type_err_on_non_string_item_names_index_and_got_int() {
        // `:tags ("ok" 7)` — outer is a list, the second item isn't a
        // string. Diagnostic names BOTH the item path (`:tags[1]`) and the
        // narrower per-item expectation (`expected string`, not the outer
        // `expected list of strings`) so authors see structurally where
        // the failure is, not just which kwarg.
        let args = kwargs_of(r#"(_ :tags ("ok" 7))"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_string_list(&kw, "tags").unwrap_err());
        assert!(
            msg.contains(":tags[1]"),
            "expected indexed item path, got: {msg}"
        );
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_optional_string_list_absent_kwarg_returns_none() {
        // `:tags` absent — the OPTIONAL list-family posture bifurcates
        // absent from present-empty. `extract_string_list` collapses
        // both cases to `Ok(Vec::new())`; the optional peer distinguishes
        // them: absent → `Ok(None)`, present-empty → `Ok(Some(vec![]))`.
        // Pin the None arm here; the sibling
        // `extract_optional_string_list_present_empty_returns_some_empty_vec`
        // test pins the other end of the bifurcation.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(extract_optional_string_list(&kw, "tags").unwrap(), None);
    }

    #[test]
    fn extract_optional_string_list_present_empty_returns_some_empty_vec() {
        // `:tags ()` — a PRESENT empty list is Some(Vec::new()), not
        // None. The load-bearing distinction between "the operator did
        // not name this kwarg" and "the operator explicitly bound the
        // kwarg to an empty list" — a `Vec<String>` field collapses
        // both to the empty vec, but an `Option<Vec<String>>` field
        // preserves the operator's intent.
        let args = kwargs_of("(_ :tags ())");
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(
            extract_optional_string_list(&kw, "tags").unwrap(),
            Some(Vec::<String>::new())
        );
    }

    #[test]
    fn extract_optional_string_list_returns_some_vec_when_all_items_are_strings() {
        // Happy-path: `(list "a" "b" "c")` under a present kwarg on the
        // string axis projects to `Some(vec!["a", "b", "c"])`
        // byte-identically to what the required peer `extract_string_list`
        // would return, wrapped in `Some`.
        let args = kwargs_of(r#"(_ :tags ("a" "b" "c"))"#);
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(
            extract_optional_string_list(&kw, "tags").unwrap(),
            Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
    }

    #[test]
    fn extract_optional_string_list_type_err_on_scalar_names_got_int() {
        // `:tags 7` — a PRESENT kwarg with a non-list scalar rejects
        // with the SAME outer `expected list of strings` diagnostic
        // the required peer `extract_string_list` emits, and names the
        // actual outer shape (`int`). The present-vs-absent
        // bifurcation happens BEFORE the required peer's outer gate,
        // so the outer-shape rejection variant is byte-identical to
        // the required peer's.
        let args = kwargs_of("(_ :tags 7)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_string_list(&kw, "tags").unwrap_err());
        assert!(msg.contains("expected list of strings"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_optional_string_list_type_err_on_non_string_item_names_index_and_got_int() {
        // `:tags ("ok" 7)` — outer is a list, the second item isn't a
        // string. Diagnostic names BOTH the item path (`:tags[1]`) and
        // the narrower per-item expectation (`expected string`), the
        // SAME typed `LispError::TypeMismatch { form: Item { key: "tags",
        // idx: 1 }, expected: String, got: Int }` variant its required
        // peer `extract_string_list` emits at the per-item atom-family
        // shape gate. Pre-lift the derive routed `Option<Vec<String>>`
        // through `extract_optional_via_serde` (the universal serde
        // bridge — `classify_option` had no arm for `Option<Vec<T>>`
        // and fell through to `Kind::OptionalDeserialize`), so a
        // per-item shape mismatch surfaced as a mystery
        // `KwargDeserialize { message: "invalid type: integer ..., expected
        // a string at path .1" }` substring rather than as the typed
        // atom-family rejection. Post-lift the optional peer rides the
        // SAME atom-family gate its required peer already binds through
        // `<&str as AtomKwarg<'_>>::project_at`, so a per-item shape
        // mismatch surfaces as the SAME pattern-matchable typed variant
        // — the two peers on the string axis speak ONE rejection
        // vocabulary.
        let args = kwargs_of(r#"(_ :tags ("ok" 7))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_optional_string_list(&kw, "tags").unwrap_err();
        let msg = type_err_message(err);
        assert!(
            msg.contains(":tags[1]"),
            "expected indexed item path, got: {msg}"
        );
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");

        // Extractor identity — extract_optional_string_list's per-item
        // rejection at a non-string element MUST match the required-peer
        // extract_string_list's rejection at the same input byte-for-byte,
        // so the atom-family shape gate does NOT drift between the two
        // peers on the string axis. The two paths differ only at the
        // outer present-vs-absent axis (`Option<Vec<String>>` wrapper vs.
        // bare `Vec<String>`), never at the per-item shape gate.
        let mixed_args = kwargs_of(r#"(_ :tags ("ok" 7))"#);
        let mixed_kw = parse_kwargs(&mixed_args).unwrap();
        let optional_err = extract_optional_string_list(&mixed_kw, "tags").unwrap_err();
        let required_err = extract_string_list(&mixed_kw, "tags").unwrap_err();
        match (&optional_err, &required_err) {
            (
                LispError::TypeMismatch {
                    form: form_a,
                    expected: exp_a,
                    got: got_a,
                },
                LispError::TypeMismatch {
                    form: form_b,
                    expected: exp_b,
                    got: got_b,
                },
            ) => {
                assert_eq!(form_a, form_b, "path variant identity");
                assert_eq!(exp_a, exp_b, "axis-typed SHAPE identity");
                assert_eq!(*exp_a, ExpectedKwargShape::String);
                assert_eq!(got_a, got_b, "actual-shape witness identity");
            }
            other => panic!("both routes must produce TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_bool_list_absent_kwarg_returns_empty_vec() {
        // `:flags` absent — an absent list-typed kwarg is the empty
        // list, never an error. Same posture `extract_list` gives every
        // list-typed extractor (`extract_string_list`,
        // `extract_narrowed_list`, `extract_vec_via_serde`).
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        assert!(extract_bool_list(&kw, "flags").unwrap().is_empty());
    }

    #[test]
    fn extract_bool_list_returns_bools_when_all_items_are_bools() {
        // Happy-path: `(list #t #f #t)` on the bool axis projects to
        // `Vec<bool>` byte-identically to the per-item
        // `<bool as AtomKwarg<'_>>::project` verdicts.
        let args = kwargs_of("(_ :flags (#t #f #t))");
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(
            extract_bool_list(&kw, "flags").unwrap(),
            vec![true, false, true]
        );
    }

    #[test]
    fn extract_bool_list_type_err_on_scalar_names_got_bool() {
        // `:flags #t` — list-typed kwarg given a scalar. The outer-
        // shape gate rejects with the axis-typed refinement
        // `ExpectedKwargShape::ListOfBools` (via the
        // `<bool>::LIST_SHAPE` per-axis trait-const override — sibling
        // to the `<&str>::LIST_SHAPE = ListOfStrings` override the
        // string-axis peer `extract_string_list` binds), and names
        // the actual outer shape (`bool` in this case). The full
        // rendered diagnostic reads
        // `expected list of bools, got bool` — sharper than the
        // pre-lift ambiguous `expected list, got bool` bytes the
        // wide-numeric peers `extract_narrowed_list` still emit
        // pending their per-axis refinements.
        let args = kwargs_of("(_ :flags #t)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_bool_list(&kw, "flags").unwrap_err();
        assert_eq!(
            err.to_string(),
            "compile error in :flags: expected list of bools, got bool",
        );
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(form, KwargPath::named("flags"));
                assert_eq!(expected, ExpectedKwargShape::ListOfBools);
                assert_eq!(got, SexpShape::Bool);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_bool_list_type_err_on_non_bool_item_names_index_and_got_string() {
        // `:flags (#t "yes" #f)` — outer is a list, the second item
        // isn't a bool. Diagnostic names BOTH the item path
        // (`:flags[1]`) and the narrower per-item expectation
        // (`expected bool`, not the outer `expected list`) so authors
        // see structurally where the failure is. Pre-lift this routed
        // through `extract_vec_via_serde` and surfaced as a serde
        // substring like `invalid type: string ..., expected a boolean`;
        // post-lift it rides the typed atom-family gate
        // `<bool as AtomKwarg<'_>>::project_at`, matching the shape
        // its peer `extract_string_list` emits on the string axis
        // byte-for-byte modulo axis identity.
        let args = kwargs_of(r#"(_ :flags (#t "yes" #f))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_bool_list(&kw, "flags").unwrap_err();
        let msg = type_err_message(err);
        assert!(
            msg.contains(":flags[1]"),
            "expected indexed item path, got: {msg}"
        );
        assert!(msg.contains("expected bool"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");

        // Extractor identity — extract_bool_list's per-item rejection
        // at a non-bool element MUST match the direct
        // <bool as AtomKwarg>::project_at call at the same
        // axis/key/idx byte-for-byte, so the atom-family shape gate
        // does NOT drift between the two paths a bool-list caller can
        // reach through.
        let mixed_args = kwargs_of(r#"(_ :flags (#t "yes" #f))"#);
        let mixed_kw = parse_kwargs(&mixed_args).unwrap();
        let list_err = extract_bool_list(&mixed_kw, "flags").unwrap_err();
        let direct_err = <bool as AtomKwarg<'_>>::project_at(
            "flags",
            1,
            &Sexp::Atom(Atom::Str("yes".to_string())),
        )
        .unwrap_err();
        match (&list_err, &direct_err) {
            (
                LispError::TypeMismatch {
                    form: form_a,
                    expected: exp_a,
                    got: got_a,
                },
                LispError::TypeMismatch {
                    form: form_b,
                    expected: exp_b,
                    got: got_b,
                },
            ) => {
                assert_eq!(form_a, form_b, "path variant identity");
                assert_eq!(exp_a, exp_b, "axis-typed SHAPE identity");
                assert_eq!(*exp_a, ExpectedKwargShape::Bool);
                assert_eq!(got_a, got_b, "actual-shape witness identity");
            }
            other => panic!("both routes must produce TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_optional_bool_list_absent_kwarg_returns_none() {
        // `:flags` absent — the OPTIONAL list-family posture bifurcates
        // absent from present-empty. `extract_bool_list` collapses
        // both cases to `Ok(Vec::new())`; the optional peer distinguishes
        // them: absent → `Ok(None)`, present-empty → `Ok(Some(vec![]))`.
        // Pin the None arm here; the sibling
        // `extract_optional_bool_list_present_empty_returns_some_empty_vec`
        // test pins the other end of the bifurcation.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(extract_optional_bool_list(&kw, "flags").unwrap(), None);
    }

    #[test]
    fn extract_optional_bool_list_present_empty_returns_some_empty_vec() {
        // `:flags ()` — a PRESENT empty list is Some(Vec::new()), not
        // None. The load-bearing distinction between "the operator did
        // not name this kwarg" and "the operator explicitly bound the
        // kwarg to an empty list" — a `Vec<bool>` field collapses
        // both to the empty vec, but an `Option<Vec<bool>>` field
        // preserves the operator's intent.
        let args = kwargs_of("(_ :flags ())");
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(
            extract_optional_bool_list(&kw, "flags").unwrap(),
            Some(Vec::<bool>::new())
        );
    }

    #[test]
    fn extract_optional_bool_list_returns_some_vec_when_all_items_are_bools() {
        // Happy-path: `(list #t #f #t)` under a present kwarg on the
        // bool axis projects to `Some(vec![true, false, true])`
        // byte-identically to what the required peer `extract_bool_list`
        // would return, wrapped in `Some`.
        let args = kwargs_of("(_ :flags (#t #f #t))");
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(
            extract_optional_bool_list(&kw, "flags").unwrap(),
            Some(vec![true, false, true])
        );
    }

    #[test]
    fn extract_optional_bool_list_type_err_on_scalar_names_got_bool() {
        // `:flags #t` — a PRESENT kwarg with a non-list scalar rejects
        // with the SAME axis-typed `expected list of bools` outer-
        // shape diagnostic the required peer `extract_bool_list`
        // emits through its `<bool>::LIST_SHAPE` per-axis trait-const
        // override, and names the actual outer shape (`bool`). The
        // present-vs-absent bifurcation happens BEFORE the required
        // peer's outer gate, so the outer-shape rejection variant is
        // byte-identical to the required peer's — including the
        // sharpened element-typed refinement label.
        let args = kwargs_of("(_ :flags #t)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_optional_bool_list(&kw, "flags").unwrap_err();
        assert_eq!(
            err.to_string(),
            "compile error in :flags: expected list of bools, got bool",
        );
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(form, KwargPath::named("flags"));
                assert_eq!(expected, ExpectedKwargShape::ListOfBools);
                assert_eq!(got, SexpShape::Bool);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_optional_bool_list_type_err_on_non_bool_item_names_index_and_got_string() {
        // `:flags (#t "yes" #f)` — outer is a list, the second item
        // isn't a bool. Diagnostic names BOTH the item path
        // (`:flags[1]`) and the narrower per-item expectation
        // (`expected bool`), the SAME typed `LispError::TypeMismatch
        // { form: Item { key: "flags", idx: 1 }, expected: Bool, got:
        // String }` variant its required peer `extract_bool_list` emits
        // at the per-item atom-family shape gate. Pre-lift the derive
        // routed `Option<Vec<bool>>` through `extract_optional_via_serde`
        // (the universal serde bridge — `classify_option` had no arm
        // for `Option<Vec<bool>>` and fell through to
        // `Kind::OptionalDeserialize`), so a per-item shape mismatch
        // surfaced as a mystery
        // `KwargDeserialize { message: "invalid type: string ..., expected
        // a boolean at path .1" }` substring rather than as the typed
        // atom-family rejection. Post-lift the optional peer rides the
        // SAME atom-family gate its required peer already binds through
        // `<bool as AtomKwarg<'_>>::project_at`, so a per-item shape
        // mismatch surfaces as the SAME pattern-matchable typed variant
        // — the two peers on the bool axis speak ONE rejection
        // vocabulary.
        let args = kwargs_of(r#"(_ :flags (#t "yes" #f))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_optional_bool_list(&kw, "flags").unwrap_err();
        let msg = type_err_message(err);
        assert!(
            msg.contains(":flags[1]"),
            "expected indexed item path, got: {msg}"
        );
        assert!(msg.contains("expected bool"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");

        // Extractor identity — extract_optional_bool_list's per-item
        // rejection at a non-bool element MUST match the required-peer
        // extract_bool_list's rejection at the same input byte-for-byte,
        // so the atom-family shape gate does NOT drift between the two
        // peers on the bool axis. The two paths differ only at the
        // outer present-vs-absent axis (`Option<Vec<bool>>` wrapper vs.
        // bare `Vec<bool>`), never at the per-item shape gate.
        let mixed_args = kwargs_of(r#"(_ :flags (#t "yes" #f))"#);
        let mixed_kw = parse_kwargs(&mixed_args).unwrap();
        let optional_err = extract_optional_bool_list(&mixed_kw, "flags").unwrap_err();
        let required_err = extract_bool_list(&mixed_kw, "flags").unwrap_err();
        match (&optional_err, &required_err) {
            (
                LispError::TypeMismatch {
                    form: form_a,
                    expected: exp_a,
                    got: got_a,
                },
                LispError::TypeMismatch {
                    form: form_b,
                    expected: exp_b,
                    got: got_b,
                },
            ) => {
                assert_eq!(form_a, form_b, "path variant identity");
                assert_eq!(exp_a, exp_b, "axis-typed SHAPE identity");
                assert_eq!(*exp_a, ExpectedKwargShape::Bool);
                assert_eq!(got_a, got_b, "actual-shape witness identity");
            }
            other => panic!("both routes must produce TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_optional_int_list_narrowed_absent_kwarg_returns_none() {
        // `:ports` absent — the OPTIONAL numeric-list posture bifurcates
        // absent from present-empty. `extract_int_list_narrowed`
        // collapses both cases to `Ok(Vec::new())`; the optional peer
        // distinguishes them: absent → `Ok(None)`, present-empty →
        // `Ok(Some(vec![]))`. Sibling to the peer bool-axis /
        // string-axis absent-kwarg tests.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(
            extract_optional_int_list_narrowed::<u16>(&kw, "ports").unwrap(),
            None
        );
    }

    #[test]
    fn extract_optional_int_list_narrowed_present_empty_returns_some_empty_vec() {
        // `:ports ()` — a PRESENT empty list is Some(Vec::new()), not
        // None. The load-bearing distinction between "the operator did
        // not name this kwarg" and "the operator explicitly bound the
        // kwarg to an empty list" — a `Vec<u16>` field collapses both
        // to the empty vec, but an `Option<Vec<u16>>` field preserves
        // the operator's intent.
        let args = kwargs_of("(_ :ports ())");
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(
            extract_optional_int_list_narrowed::<u16>(&kw, "ports").unwrap(),
            Some(Vec::<u16>::new())
        );
    }

    #[test]
    fn extract_optional_int_list_narrowed_returns_some_vec_when_all_items_in_range() {
        // Happy-path: `(list 80 443 8080)` under a present kwarg on the
        // int axis projects to `Some(vec![80u16, 443, 8080])` byte-
        // identically to what the required peer
        // `extract_int_list_narrowed::<u16>` returns, wrapped in `Some`.
        let args = kwargs_of("(_ :ports (80 443 8080))");
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(
            extract_optional_int_list_narrowed::<u16>(&kw, "ports").unwrap(),
            Some(vec![80u16, 443, 8080])
        );
    }

    #[test]
    fn extract_optional_int_list_narrowed_range_err_on_per_item_names_index_and_target() {
        // `:ports (list 80 70000)` on `Option<Vec<u16>>` — the second
        // item overflows u16. Diagnostic names BOTH the item path
        // (`:ports[1]`) and the axis-typed width (`NumericWidth::U16`
        // / `NumericLiteral::Int(70_000)`), the SAME typed
        // `LispError::KwargOutOfRange { form: Item { key, idx },
        // target, value }` variant its required peer
        // `extract_int_list_narrowed::<u16>` emits at the per-item
        // narrowing gate. Pre-lift the derive routed
        // `Option<Vec<u16>>` through `extract_optional_via_serde` (the
        // universal serde bridge — `classify_option` had no arm for
        // `Option<Vec<u16>>` and fell through to
        // `Kind::OptionalDeserialize`), so a per-item out-of-range
        // value surfaced as a mystery
        // `KwargDeserialize { message: "invalid value: integer 70000,
        // expected u16 at path .1" }` substring rather than as the
        // typed axis-typed rejection. Post-lift the optional peer
        // rides the SAME narrowing gate its required peer already
        // binds through `narrow_or_range_err_at::<W, T>`, so a per-
        // item narrowing failure surfaces as the SAME pattern-
        // matchable typed variant — the two peers on the int axis
        // speak ONE rejection vocabulary.
        let args = kwargs_of("(_ :ports (80 70000))");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_optional_int_list_narrowed::<u16>(&kw, "ports").unwrap_err();
        let LispError::KwargOutOfRange {
            form,
            target,
            value,
        } = &err
        else {
            panic!("expected KwargOutOfRange (typed narrowing gate), got {err:?}");
        };
        assert_eq!(form, &KwargPath::item("ports", 1));
        assert_eq!(*target, NumericWidth::U16);
        assert_eq!(*value, NumericLiteral::Int(70_000));

        // Extractor identity — extract_optional_int_list_narrowed's
        // per-item rejection at an out-of-range element MUST match the
        // required-peer extract_int_list_narrowed's rejection at the
        // same input byte-for-byte, so the narrowing gate does NOT
        // drift between the two peers on the int axis. The two paths
        // differ only at the outer present-vs-absent axis
        // (`Option<Vec<u16>>` wrapper vs. bare `Vec<u16>`), never at
        // the per-item narrowing gate.
        let optional_err = extract_optional_int_list_narrowed::<u16>(&kw, "ports").unwrap_err();
        let required_err = extract_int_list_narrowed::<u16>(&kw, "ports").unwrap_err();
        match (&optional_err, &required_err) {
            (
                LispError::KwargOutOfRange {
                    form: f_a,
                    target: t_a,
                    value: v_a,
                },
                LispError::KwargOutOfRange {
                    form: f_b,
                    target: t_b,
                    value: v_b,
                },
            ) => {
                assert_eq!(f_a, f_b, "path variant identity");
                assert_eq!(t_a, t_b, "axis-typed WIDTH identity");
                assert_eq!(v_a, v_b, "wide-value witness identity");
            }
            other => panic!("both routes must produce KwargOutOfRange, got {other:?}"),
        }
    }

    #[test]
    fn extract_optional_float_list_narrowed_absent_present_and_narrowing_gate() {
        // Float-axis peer of the four int-axis pins collapsed to one
        // — the axis identity riding `<f64>` is what changes, not the
        // scaffold. Absent kwarg → None; present empty → Some(vec![]);
        // present all-in-range → Some(decoded); present per-item
        // lossy-to-inf overflow → typed `KwargOutOfRange { target:
        // F32, value: Float(_) }` (the SAME variant the required peer
        // `extract_float_list_narrowed::<f32>` emits, the SAME variant
        // the scalar `extract_float_narrowed::<f32>` emits on
        // `:scale 1.0e300`).
        let absent_args = kwargs_of("(_ :other 1)");
        let absent = parse_kwargs(&absent_args).unwrap();
        assert_eq!(
            extract_optional_float_list_narrowed::<f32>(&absent, "scales").unwrap(),
            None
        );

        let empty_args = kwargs_of("(_ :scales ())");
        let present_empty = parse_kwargs(&empty_args).unwrap();
        assert_eq!(
            extract_optional_float_list_narrowed::<f32>(&present_empty, "scales").unwrap(),
            Some(Vec::<f32>::new())
        );

        let ok_args = kwargs_of("(_ :scales (1.0 2.5))");
        let present_ok = parse_kwargs(&ok_args).unwrap();
        let decoded = extract_optional_float_list_narrowed::<f32>(&present_ok, "scales")
            .unwrap()
            .expect("present list must return Some");
        assert_eq!(decoded.len(), 2);
        assert!((decoded[0] - 1.0_f32).abs() < f32::EPSILON);
        assert!((decoded[1] - 2.5_f32).abs() < f32::EPSILON);

        let overflow_args = kwargs_of("(_ :scales (1.0 1.0e300))");
        let overflow = parse_kwargs(&overflow_args).unwrap();
        let err = extract_optional_float_list_narrowed::<f32>(&overflow, "scales").unwrap_err();
        let LispError::KwargOutOfRange { form, target, .. } = &err else {
            panic!("expected KwargOutOfRange (typed narrowing gate), got {err:?}");
        };
        assert_eq!(form, &KwargPath::item("scales", 1));
        assert_eq!(*target, NumericWidth::F32);
    }

    #[test]
    fn extract_optional_int_type_err_names_got_string() {
        let args = kwargs_of(r#"(_ :n "seven")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_int(&kw, "n").unwrap_err());
        assert!(msg.contains("expected int"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_optional_float_type_err_names_got_string() {
        let args = kwargs_of(r#"(_ :ratio "half")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_float(&kw, "ratio").unwrap_err());
        assert!(msg.contains("expected number"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_optional_bool_type_err_names_got_int() {
        let args = kwargs_of("(_ :enabled 1)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_bool(&kw, "enabled").unwrap_err());
        assert!(msg.contains("expected bool"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_non_list_kwarg_names_got_string() {
        // `:steps "scalar"` — the vec-fallthrough's "expected list" used
        // to be a bare label; now it also reports the actual outer shape.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg =
            type_err_message(extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err());
        assert!(msg.contains("expected list"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn derive_type_err_end_to_end_names_got_string_for_threshold() {
        // End-to-end through `#[derive(TataraDomain)]`. A misspelled-as-
        // string `:threshold "tight"` used to surface as "expected
        // number" with no signal what was actually passed; now the
        // diagnostic carries `got string` so authoring surfaces have
        // structural info to render without re-reading the source.
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold "tight")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":threshold"), "got: {msg}");
        assert!(msg.contains("expected number"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    // ── Odd-kwargs dangling-element naming ─────────────────────────────
    //
    // `(defX :name "x" :query)` used to surface as the bare "odd number of
    // keyword arguments" message — operator could not tell whether
    // `:query`'s value got lost or whether the form was malformed. The
    // structural fix names the dangling element via `Sexp::Display`:
    //   - keyword case (`:query` with no value) → `:query`
    //   - non-keyword case (stray `5` at tail)  → `5`
    // Both halves of the failure are now structurally complete: the gate
    // names the failure mode AND the offending element. Pinning each case
    // here keeps `tatara-check` / LSP / REPL renderings safe across
    // versions, and means a future run that gives `Sexp` source spans
    // attaches a position to the same single primitive (`OddKwargs`)
    // mechanically.
    //
    // Theory anchor: THEORY.md §II.1 invariant 1 (typed entry); §V.1
    // (knowable platform — diagnostic names both expected and actual).

    #[test]
    fn parse_kwargs_names_dangling_keyword() {
        // `:name "x" :query` — `:query` has no value. The error variant
        // carries the dangling kwarg's display, so the author sees which
        // keyword lost its value.
        let args = kwargs_of(r#"(_ :name "x" :query)"#);
        let err = parse_kwargs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            matches!(err, LispError::OddKwargs { ref dangling } if dangling == ":query"),
            "expected OddKwargs {{ dangling: \":query\" }}, got {err:?}"
        );
        assert!(
            msg.contains(":query"),
            "error must name the dangling keyword, got: {msg}"
        );
        assert!(
            msg.contains("dangling"),
            "expected 'dangling' in the message, got: {msg}"
        );
    }

    #[test]
    fn parse_kwargs_names_dangling_non_keyword_scalar() {
        // `:name "x" :query "q" 5` — a stray scalar at the tail. The
        // dangling element's `Sexp::Display` is `5`; the diagnostic must
        // name it so the author knows what to delete (or which kwarg key
        // to add in front of it).
        let args = kwargs_of(r#"(_ :name "x" :query "q" 5)"#);
        let err = parse_kwargs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            matches!(err, LispError::OddKwargs { ref dangling } if dangling == "5"),
            "expected OddKwargs {{ dangling: \"5\" }}, got {err:?}"
        );
        assert!(
            msg.contains('5'),
            "error must name the dangling scalar, got: {msg}"
        );
    }

    #[test]
    fn parse_kwargs_names_dangling_string_scalar() {
        // `:name "x" "stray"` — a stray string at the tail. The Sexp
        // Display projects strings through `{:?}`, so the diagnostic
        // contains the quoted form `"stray"` — preserves the typed shape.
        let args = kwargs_of(r#"(_ :name "x" "stray")"#);
        let err = parse_kwargs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            matches!(err, LispError::OddKwargs { ref dangling } if dangling == "\"stray\""),
            "expected OddKwargs {{ dangling: \"\\\"stray\\\"\" }}, got {err:?}"
        );
        assert!(
            msg.contains("stray"),
            "error must name the dangling string, got: {msg}"
        );
    }

    #[test]
    fn parse_kwargs_single_dangling_keyword() {
        // `(_ :only)` — a single dangling keyword with nothing else. The
        // gate must name it the same way as the multi-kwarg case;
        // structural completeness should not depend on list length.
        let args = kwargs_of("(_ :only)");
        let err = parse_kwargs(&args).unwrap_err();
        assert!(
            matches!(err, LispError::OddKwargs { ref dangling } if dangling == ":only"),
            "expected OddKwargs {{ dangling: \":only\" }}, got {err:?}"
        );
    }

    #[test]
    fn derive_odd_kwargs_end_to_end_names_dangling_keyword() {
        // End-to-end through `#[derive(TataraDomain)]`. A truncated
        // authoring form `(defmonitor :name "x" :query)` used to surface
        // as a bare "odd number" message; now every derived domain
        // inherits the named-dangling-element diagnostic for free
        // because they all funnel through `parse_kwargs`.
        let forms = read(r#"(defmonitor :name "x" :query)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":query"),
            "derived odd-kwargs error must name the dangling kwarg, got: {msg}"
        );
        assert!(
            msg.contains("dangling"),
            "expected 'dangling' label end-to-end, got: {msg}"
        );
    }

    // ── Indexed-item form labels for list-typed kwargs ─────────────────
    //
    // `kwarg_form` and `kwarg_item_form` are the two named primitives
    // that build the `form:` field of every typed-entry diagnostic. The
    // base helper consolidates seven inline `format!(":{key}")` copies
    // (parse_kwargs duplicate, reject_unknown_kwargs, required, type_err,
    // deserialize_err, sexp_to_json's nested-duplicate, the non-list
    // path in extract_vec_via_serde) into one site; the indexed helper
    // adds the structural slot for *which item* failed.
    //
    // Pinning the canonical shapes here keeps downstream tooling
    // (`tatara-check`, LSP, REPL) safe across versions, and means a
    // future run that gives `Sexp` source spans threads `pos` through
    // ONE primitive instead of every macro emit. Frontier inspiration:
    // JSON Pointer (`/steps/1`), jq paths.

    #[test]
    fn kwarg_form_renders_canonical_shape() {
        // After the typed-slot promotion the helpers return `KwargPath`
        // (the typed enum, structurally bound) rather than `String`;
        // Display projects each variant to its canonical literal
        // byte-for-byte equivalent to the legacy `format!` shape. Pin
        // both the structural identity AND the rendered literal so the
        // dual contract (typed-binding + byte-for-byte display) is
        // anchored from both angles.
        assert_eq!(
            kwarg_form("threshold"),
            crate::error::KwargPath::Named("threshold".into())
        );
        assert_eq!(kwarg_form("threshold").to_string(), ":threshold");
        assert_eq!(kwarg_form("notify-ref").to_string(), ":notify-ref");
        // No transformation of the key — the surface name is what the
        // author sees in the source. `kebab_to_camel` happens elsewhere.
        assert_eq!(kwarg_form("").to_string(), ":");
    }

    #[test]
    fn kwarg_item_form_renders_canonical_indexed_shape() {
        assert_eq!(
            kwarg_item_form("tags", 0),
            crate::error::KwargPath::Item {
                key: "tags".into(),
                idx: 0
            }
        );
        assert_eq!(kwarg_item_form("tags", 0).to_string(), ":tags[0]");
        assert_eq!(kwarg_item_form("steps", 1).to_string(), ":steps[1]");
        assert_eq!(kwarg_item_form("steps", 17).to_string(), ":steps[17]");
    }

    #[test]
    fn kwargs_pos_form_renders_canonical_slot_shape() {
        // Sibling of `kwarg_form` / `kwarg_item_form` — used when the
        // kwargs slot itself failed the keyword gate, so there is no
        // `:<key>` to root the path. Pin both the structural identity
        // (`KwargPath::Slot(i)`) AND the rendered literal
        // (`kwargs[<idx>]`) so `tatara-check` / LSP / REPL match either
        // surface directly.
        assert_eq!(kwargs_pos_form(0), crate::error::KwargPath::Slot(0));
        assert_eq!(kwargs_pos_form(0).to_string(), "kwargs[0]");
        assert_eq!(kwargs_pos_form(2).to_string(), "kwargs[2]");
        assert_eq!(kwargs_pos_form(42).to_string(), "kwargs[42]");
    }

    #[test]
    fn extract_string_list_outer_failure_keeps_unindexed_form() {
        // Negative-control: the outer-shape failure (`:tags "scalar"`)
        // is at the kwarg level, not the item level — its form must NOT
        // pick up an `[idx]` suffix, and the message keeps the wider
        // `expected list of strings`.
        let args = kwargs_of(r#"(_ :tags "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string_list(&kw, "tags").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":tags"), "got: {msg}");
        assert!(
            !msg.contains(":tags["),
            "outer failure must not gain an item index, got: {msg}"
        );
        assert!(msg.contains("expected list of strings"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_string_list_indexes_each_failing_item() {
        // The first non-string item wins (collect short-circuits on the
        // first Err). Pin the index math: a failure at position 2 must
        // surface as `:tags[2]`, not `:tags[0]` or `:tags[1]`.
        let args = kwargs_of(r#"(_ :tags ("ok" "also-ok" 7))"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = format!("{}", extract_string_list(&kw, "tags").unwrap_err());
        assert!(msg.contains(":tags[2]"), "got: {msg}");
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_indexes_failing_item() {
        // Second item has a non-string `:notify-ref`. The serde error
        // must surface under `:steps[1]` so the operator goes straight
        // to the bad item — previously the index was lost and the
        // diagnostic only named `:steps`. Bind on the structural
        // variant: `idx: Some(1)` makes the index addressable as
        // first-class data, not a substring of the rendered message.
        let args = kwargs_of(
            r#"(_ :steps (
                  (:notify-ref "ok")
                  (:notify-ref 7)))"#,
        );
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::KwargDeserialize {
                    path: KwargPath::Item { ref key, idx: 1 },
                    ..
                } if key == "steps"
            ),
            "expected KwargDeserialize {{ path: KwargPath::Item {{ key: \"steps\", idx: 1 }}, .. }}, got {err:?}"
        );
        let msg = format!("{err}");
        assert!(msg.contains(":steps[1]"), "got: {msg}");
        assert!(msg.contains("deserialize:"), "got: {msg}");
    }

    #[test]
    fn extract_optional_via_serde_deserialize_failure_emits_kwarg_deserialize_variant() {
        // `:level NotASeverity` — well-formed Sexp, ill-formed enum.
        // The optional path must NOT short-circuit when the kwarg IS
        // present but malformed; it must produce the same structural
        // `LispError::KwargDeserialize { path: KwargPath::Named(_), .. }`
        // variant the required path produces, so the typed-entry
        // `from_value` rejection mode is uniform across the required +
        // optional pair — `extract_via_serde` and
        // `extract_optional_via_serde` share ONE error path via
        // `deserialize_err`.
        let args = kwargs_of("(_ :level NotASeverity)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_optional_via_serde::<Severity>(&kw, "level").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::KwargDeserialize {
                    path: KwargPath::Named(ref key),
                    ref message,
                } if key == "level" && !message.is_empty()
            ),
            "expected KwargDeserialize {{ path: KwargPath::Named(\"level\"), .. }}, got {err:?}"
        );
    }

    #[test]
    fn from_value_with_path_threads_typed_kwarg_path_into_kwarg_deserialize_variant() {
        // The three-times-rule lift's load-bearing pin:
        // `from_value_with_path::<T>(sexp, path)` is THE primitive every
        // extractor that crosses the typed-entry JSON boundary funnels
        // through. The primitive's variant slot is the typed
        // `LispError::KwargDeserialize { path: KwargPath, message }` — the
        // path identity threads from the caller verbatim into the
        // variant's typed slot, so `KwargPath::Named` from
        // `extract_via_serde` / `extract_optional_via_serde` AND
        // `KwargPath::Item { key, idx }` from `extract_vec_via_serde`'s
        // per-item closure both ride ONE primitive's `map_err` arm, not
        // three site-specific shims. Pin both path-shape arms in one
        // test so the load-bearing data-shape symmetry is anchored at
        // the primitive's boundary (not just at the three call sites
        // separately, which the prior tests already cover end-to-end).
        // A regression that re-introduces a sibling shim (collapsing
        // the typed `KwargPath` slot back into a `(key, idx:
        // Option<usize>)` pair at the helper boundary, the pre-33c64c9
        // shape) fails-loudly here AND at the existing extractor-
        // boundary tests.

        // Named-path arm: a malformed enum value flows through
        // `from_value_with_path` with `KwargPath::named("level")` into
        // the typed variant slot — same path identity an
        // `extract_via_serde::<Severity>(kw, "level")` would thread.
        let bad = Sexp::symbol("NotASeverity");
        let err = from_value_with_path::<Severity>(&bad, KwargPath::named("level"))
            .expect_err("malformed enum value must error");
        assert!(
            matches!(
                err,
                LispError::KwargDeserialize {
                    path: KwargPath::Named(ref key),
                    ref message,
                } if key == "level" && !message.is_empty()
            ),
            "expected KwargDeserialize {{ path: KwargPath::Named(\"level\"), .. }}, got {err:?}"
        );

        // Item-path arm: a per-item failure flows through the SAME
        // primitive with `KwargPath::item("steps", 1)` — the per-item
        // sub-mode of the same JSON-projection rejection chain. The
        // primitive's `map_err` arm threads the typed `KwargPath::Item
        // { key, idx }` into the variant's typed slot byte-for-byte,
        // bifurcating from the Named-arm above by variant identity (not
        // by a sibling `idx: Option<usize>` slot).
        let bad_item = Sexp::int(7);
        let err_item =
            from_value_with_path::<EscalationStep>(&bad_item, KwargPath::item("steps", 1))
                .expect_err("malformed item must error");
        assert!(
            matches!(
                err_item,
                LispError::KwargDeserialize {
                    path: KwargPath::Item { ref key, idx: 1 },
                    ..
                } if key == "steps"
            ),
            "expected KwargDeserialize {{ path: KwargPath::Item {{ key: \"steps\", idx: 1 }}, .. }}, got {err_item:?}"
        );

        // Display preserves the legacy byte-for-byte shape across both
        // path identities — `compile error in :level: deserialize: …`
        // for the named arm, `compile error in :steps[1]: deserialize: …`
        // for the item arm. The substring-grep contract that
        // `tatara-check` / REPL relied on pre-lift passes through the
        // new primitive's `LispError::Display` projection unchanged.
        let msg = format!("{err}");
        assert!(
            msg.contains(":level"),
            "named display must name kwarg, got: {msg}"
        );
        assert!(msg.contains("deserialize:"), "got: {msg}");
        let msg_item = format!("{err_item}");
        assert!(
            msg_item.contains(":steps[1]"),
            "item display must name kwarg+idx, got: {msg_item}"
        );
        assert!(msg_item.contains("deserialize:"), "got: {msg_item}");
    }

    #[test]
    fn kwarg_deserialize_helpers_share_variant_across_scalar_and_per_item_paths() {
        // Type-bound symmetry: `extract_via_serde` (scalar / required)
        // AND `extract_vec_via_serde` (per-item) BOTH funnel through
        // the SAME structural variant — `LispError::KwargDeserialize` —
        // bifurcated by `KwargPath::Named` vs. `KwargPath::Item`
        // variant identity. Pin both paths in ONE test so the symmetry
        // is load-bearing in the type system: a regression that drifts
        // either site to a different variant fails-loudly here. Mirror
        // at the typed-entry-side of the typed-exit-side
        // `helpers_are_type_bound_via_t_keyword` symmetry test (which
        // pins `register::<T>` AND `rewrite_typed::<T>` BOTH route
        // through `DomainSerialize`).
        let args = kwargs_of("(_ :level NotASeverity)");
        let kw = parse_kwargs(&args).unwrap();
        let scalar_err = extract_via_serde::<Severity>(&kw, "level").unwrap_err();
        assert!(
            matches!(
                scalar_err,
                LispError::KwargDeserialize {
                    path: KwargPath::Named(_),
                    ..
                }
            ),
            "scalar path must produce KwargDeserialize with KwargPath::Named, got {scalar_err:?}"
        );

        let args = kwargs_of(r#"(_ :steps ((:notify-ref 7)))"#);
        let kw = parse_kwargs(&args).unwrap();
        let item_err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        assert!(
            matches!(
                item_err,
                LispError::KwargDeserialize {
                    path: KwargPath::Item { idx: 0, .. },
                    ..
                }
            ),
            "per-item path must produce KwargDeserialize with KwargPath::Item, got {item_err:?}"
        );
    }

    #[test]
    fn extract_vec_via_serde_outer_failure_keeps_unindexed_form() {
        // Negative-control: the outer kwarg-isn't-a-list failure stays
        // at `:steps` (no `[N]`). The wider `expected list` message is
        // preserved.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = format!(
            "{}",
            extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err()
        );
        assert!(msg.contains(":steps"), "got: {msg}");
        assert!(
            !msg.contains(":steps["),
            "outer failure must not gain an item index, got: {msg}"
        );
        assert!(msg.contains("expected list"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_propagates_inner_duplicate_with_inner_form() {
        // Inner `(:notify-ref "a" :notify-ref "b")` fails inside
        // `sexp_to_json` BEFORE `serde_json::from_value` runs — that
        // path's error already carries its own `form: ":notify-ref"`,
        // and the item-level wrapper must not clobber it with
        // `:steps[0]`. Pin the propagation: the operator sees the
        // duplicated inner kwarg, not just the item index.
        let args = kwargs_of(r#"(_ :steps ((:notify-ref "a" :notify-ref "b")))"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = format!(
            "{}",
            extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err()
        );
        assert!(msg.contains(":notify-ref"), "got: {msg}");
        assert!(msg.contains("duplicate keyword"), "got: {msg}");
    }

    #[test]
    fn derive_indexed_item_failure_e2e_via_monitor_tags() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // `:tags ("prod" 7)` must surface as `:tags[1]` so every
        // derived domain inherits the indexed-item diagnostic by
        // sharing `extract_string_list` — no per-derive macro change.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tags ("prod" 7))"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":tags[1]"),
            "derived item-failure error must name the index, got: {msg}"
        );
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn parse_kwargs_well_formed_input_is_unaffected() {
        // Negative-control: even-length kwargs lists with no duplicates
        // and no unknowns continue to parse identically. The dangling-
        // element gate must NOT false-positive on canonical input.
        let args = kwargs_of(r#"(_ :name "x" :query "q" :threshold 0.5)"#);
        let kw = parse_kwargs(&args).expect("well-formed kwargs must parse");
        assert_eq!(kw.len(), 3);
    }

    // ── Structural TypeMismatch for not-a-keyword-at-position ──────────
    //
    // `parse_kwargs` used to raise a `LispError::Compile { form: "kwargs",
    // message: format!("expected keyword at position {i}") }` triple when
    // an even-position element wasn't a keyword. Three problems:
    //   1. `form: "kwargs"` is a generic label — operators couldn't tell
    //      which slot misfired without re-counting.
    //   2. The actual got-type was lost; `(_ "x" 5)` and `(_ 5 "x")` and
    //      `(_ #t 5)` all rendered the same "expected keyword at position
    //      0" message.
    //   3. The diagnostic was structurally distinct from every other
    //      typed-entry mismatch in the substrate (`TypeMismatch`),
    //      forcing authoring tools to substring-parse instead of binding
    //      to the variant.
    //
    // Lifting into `type_mismatch(kwargs_pos_form(i), "keyword",
    // &args[i])` collapses all three: the form is `kwargs[<idx>]`, the
    // got-type is the structural `sexp_type_name(_)` projection, and the
    // variant is the same `LispError::TypeMismatch` that
    // `extract_string` / `extract_int` / etc. already produce.
    //
    // Theory anchor: THEORY.md §V.1 (knowable platform — the diagnostic
    // names both expected AND actual); §VI.1 (generation over
    // composition — one `LispError::TypeMismatch` variant for every
    // kwarg-shape failure mode).

    #[test]
    fn parse_kwargs_non_keyword_at_position_0_emits_type_mismatch_variant() {
        // `(_ "x" 5)` — args[0] is a string, not a keyword. The variant
        // must be `TypeMismatch`, not the legacy `Compile`. `expected`
        // is the typed `ExpectedKwargShape` enum, so a typo in the static
        // label can never drift; `got` is the typed `SexpShape` enum
        // sourced from `sexp_shape(_)`'s exhaustive projection over
        // `Sexp`'s closed set of 12 outermost shapes.
        let args = kwargs_of(r#"(_ "x" 5)"#);
        let err = parse_kwargs(&args).expect_err("non-keyword position must error");
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    form: crate::error::KwargPath::Slot(0),
                    expected: ExpectedKwargShape::Keyword,
                    got: SexpShape::String,
                }
            ),
            "expected TypeMismatch {{ form: KwargPath::Slot(0), expected: Keyword, got: SexpShape::String }}, got {err:?}"
        );
    }

    #[test]
    fn parse_kwargs_non_keyword_at_position_2_emits_type_mismatch_variant() {
        // `(_ :name "x" "y" 5)` — first pair `:name "x"` succeeds; second
        // pair starts at position 2 with a string. The form must name
        // `kwargs[2]` so the operator goes straight to the slot — pin the
        // index math via the typed `KwargPath::Slot(2)` identity.
        let args = kwargs_of(r#"(_ :name "x" "y" 5)"#);
        let err = parse_kwargs(&args).expect_err("non-keyword at later position must error");
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    form: crate::error::KwargPath::Slot(2),
                    expected: ExpectedKwargShape::Keyword,
                    got: SexpShape::String,
                }
            ),
            "expected indexed TypeMismatch at KwargPath::Slot(2), got {err:?}"
        );
    }

    #[test]
    fn parse_kwargs_non_keyword_routes_got_through_sexp_type_name() {
        // The got-type is the structural `sexp_shape(_)` projection,
        // not a free-form string — pinning this contract for ints, bools,
        // and symbols means a regression that re-inlines the diagnostic
        // (with `format!("got {}", _)`) fails-loudly here. Three shapes
        // covered: int, bool, symbol — each routes through the typed
        // projection.
        let args = kwargs_of(r#"(_ 5 "v")"#);
        let err = parse_kwargs(&args).expect_err("int at position 0 must error");
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    got: SexpShape::Int,
                    ..
                }
            ),
            "expected got: SexpShape::Int, got {err:?}"
        );

        let args = kwargs_of(r#"(_ #t "v")"#);
        let err = parse_kwargs(&args).expect_err("bool at position 0 must error");
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    got: SexpShape::Bool,
                    ..
                }
            ),
            "expected got: SexpShape::Bool, got {err:?}"
        );

        let args = kwargs_of(r#"(_ symbolic "v")"#);
        let err = parse_kwargs(&args).expect_err("symbol at position 0 must error");
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    got: SexpShape::Symbol,
                    ..
                }
            ),
            "expected got: SexpShape::Symbol, got {err:?}"
        );
    }

    #[test]
    fn parse_kwargs_non_keyword_message_renders_canonical_type_mismatch_shape() {
        // Display matches the standard TypeMismatch render — `compile
        // error in kwargs[0]: expected keyword, got string` — so
        // authoring tools that already substring-match on `expected …,
        // got …` (`tatara-check` / LSP / REPL) light up uniformly for
        // this slot the way they do for kwarg-level type mismatches.
        let args = kwargs_of(r#"(_ "x" 5)"#);
        let err = parse_kwargs(&args).expect_err("must error");
        assert_eq!(
            format!("{err}"),
            "compile error in kwargs[0]: expected keyword, got string"
        );
    }

    #[test]
    fn derive_non_keyword_at_position_e2e_via_monitor() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // `(defmonitor "stray" :name …)` — first kwargs element is a
        // stray string, not a keyword. The derived path inherits the lift
        // for free because every derived domain funnels through
        // `parse_kwargs`; no per-derive macro change.
        let forms = read(r#"(defmonitor "stray" :name "x" :query "q" :threshold 0.5)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    form: crate::error::KwargPath::Slot(0),
                    expected: ExpectedKwargShape::Keyword,
                    got: SexpShape::String,
                }
            ),
            "expected derived TypeMismatch at KwargPath::Slot(0), got {err:?}"
        );
    }

    #[test]
    fn parse_kwargs_non_keyword_position_is_none_today() {
        // Negative-control for the future-spans move: until `Sexp`
        // carries source positions, the variant's `position()` returns
        // `None`. Pinning this contract means a future run that adds
        // `pos: Option<usize>` to `TypeMismatch` does so with a fail-
        // before/pass-after delta — and the not-a-keyword path picks up
        // the span automatically because it routes through the same
        // primitive (`type_mismatch`) as every other `TypeMismatch` site.
        let args = kwargs_of(r#"(_ "x" 5)"#);
        let err = parse_kwargs(&args).expect_err("must error");
        assert_eq!(err.position(), None);
    }

    // ── Structural TypeMismatch variant ────────────────────────────────
    //
    // The three "expected X, got Y" sites in this module — `type_err`,
    // `extract_string_list` per-item, `extract_vec_via_serde` non-list —
    // used to assemble the message inline via three near-identical
    // `format!("expected {expected}, got {}", sexp_type_name(_))` copies.
    // Three copies is the THEORY.md §VI.1 three-times-rule signal.
    //
    // `LispError::TypeMismatch { form, expected, got }` collapses the
    // shape into one structural variant: `form` is the path slot
    // (`kwarg_form` or `kwarg_item_form`), `expected` is the static
    // expectation, `got` is the static `sexp_type_name` projection.
    // Authoring tools (REPL, LSP, `tatara-check`) bind to the variant
    // directly instead of substring-parsing a rendered message; rendered
    // text matches the legacy `Compile`-shaped diagnostic byte-for-byte,
    // so existing `msg.contains("expected …")` assertions pass.
    //
    // Pinning the variant identity here keeps the structural binding
    // safe across versions, and means a future run that gives `Sexp`
    // source spans threads `pos: Option<usize>` through ONE primitive
    // (`type_mismatch`) — every type-mismatch site picks up positional
    // rendering with no consumer changes.

    #[test]
    fn type_mismatch_helper_emits_structured_variant() {
        // `type_mismatch` now takes a typed `KwargPath` for `form` AND
        // a typed `ExpectedKwargShape` for `expected` — pin the
        // structural identity of every slot, including that BOTH typed
        // enums are threaded into the variant byte-identically (not
        // coerced through a String round-trip).
        let err = type_mismatch(kwarg_form("ctx"), ExpectedKwargShape::String, &Sexp::int(7));
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(form, crate::error::KwargPath::Named("ctx".into()));
                assert_eq!(expected, ExpectedKwargShape::String);
                assert_eq!(got, SexpShape::Int);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn type_mismatch_display_matches_legacy_compile_shape() {
        // The user-visible string is byte-for-byte equivalent to the
        // pre-lift `LispError::Compile { message: format!("expected …, got …") }`
        // rendering. Authoring surfaces that pattern-match on the message
        // text continue to work; tools that pattern-match on the variant
        // gain structural binding.
        let err = type_mismatch(
            kwarg_form("threshold"),
            ExpectedKwargShape::Number,
            &Sexp::string("tight"),
        );
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: expected number, got string"
        );
    }

    #[test]
    fn extract_string_returns_type_mismatch_variant() {
        // The kwarg-level `expected X, got Y` site now produces the
        // structural variant. Pin the variant identity AND the rendered
        // message so the substrate's contract is locked from both
        // angles.
        let args = kwargs_of("(_ :name 42)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string(&kw, "name").unwrap_err();
        assert!(
            matches!(
                &err,
                LispError::TypeMismatch {
                    form,
                    expected: ExpectedKwargShape::String,
                    got: SexpShape::Int,
                } if matches!(form, crate::error::KwargPath::Named(k) if k == "name")
            ),
            "expected TypeMismatch {{ form: KwargPath::Named(\"name\"), expected: String, got: SexpShape::Int }}, got {err:?}"
        );
        assert_eq!(
            format!("{err}"),
            "compile error in :name: expected string, got int"
        );
    }

    #[test]
    fn extract_string_list_per_item_returns_indexed_type_mismatch() {
        // Per-item failure in a `Vec<String>` kwarg flows through
        // `type_err_at` → `kwarg_item_form` + `type_mismatch`. Pin the
        // typed `KwargPath::Item { key: "tags", idx: 1 }` identity
        // directly (no String round-trip).
        let args = kwargs_of(r#"(_ :tags ("ok" 7))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string_list(&kw, "tags").unwrap_err();
        assert!(
            matches!(
                &err,
                LispError::TypeMismatch {
                    form,
                    expected: ExpectedKwargShape::String,
                    got: SexpShape::Int,
                } if matches!(form, crate::error::KwargPath::Item { key, idx: 1 } if key == "tags")
            ),
            "expected indexed TypeMismatch at KwargPath::Item {{ key: \"tags\", idx: 1 }}, got {err:?}"
        );
    }

    #[test]
    fn extract_vec_via_serde_non_list_returns_type_mismatch() {
        // The vec-fallthrough's "expected list" path lifts into the
        // same variant — `:steps "scalar"` no longer produces
        // `LispError::Compile`; it produces `TypeMismatch` with
        // `form: KwargPath::Named("steps")`, `expected: "list"`,
        // `got: "string"`. Authoring tools see the same shape regardless
        // of which extractor reported the mismatch.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        assert!(
            matches!(
                &err,
                LispError::TypeMismatch {
                    form,
                    expected: ExpectedKwargShape::List,
                    got: SexpShape::String,
                } if matches!(form, crate::error::KwargPath::Named(k) if k == "steps")
            ),
            "expected list-shape TypeMismatch at KwargPath::Named(\"steps\"), got {err:?}"
        );
    }

    #[test]
    fn extract_string_list_outer_failure_returns_list_of_strings_type_mismatch() {
        // The outer-shape failure (`:tags "scalar"`) is at the kwarg
        // level — its `expected` stays `"list of strings"` (wider than
        // the per-item case's `"string"`) and the form has no `[idx]`
        // suffix (`KwargPath::Named`, not `KwargPath::Item`). Same
        // variant; different `expected` + path-shape.
        let args = kwargs_of(r#"(_ :tags "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string_list(&kw, "tags").unwrap_err();
        assert!(
            matches!(
                &err,
                LispError::TypeMismatch {
                    form,
                    expected: ExpectedKwargShape::ListOfStrings,
                    got: SexpShape::String,
                } if matches!(form, crate::error::KwargPath::Named(k) if k == "tags")
            ),
            "expected outer-shape TypeMismatch at KwargPath::Named(\"tags\"), got {err:?}"
        );
    }

    #[test]
    fn type_mismatch_position_is_none_today() {
        // Negative-control: until `Sexp` carries spans, `position()`
        // returns `None` for the variant — `format_diagnostic` falls
        // through to single-line rendering, no caret emitted. Pinning
        // this contract means a future run that adds `pos: Option<usize>`
        // does so deliberately, with a fail-before/pass-after delta.
        let err = type_mismatch(kwarg_form("x"), ExpectedKwargShape::String, &Sexp::int(0));
        assert_eq!(err.position(), None);
    }

    #[test]
    fn derive_type_mismatch_e2e_via_monitor_threshold() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // a misspelled-as-string `:threshold "tight"` surfaces the
        // structural variant. Every derived domain inherits the lift —
        // no per-derive macro change.
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold "tight")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(
                &err,
                LispError::TypeMismatch {
                    form,
                    expected: ExpectedKwargShape::Number,
                    got: SexpShape::String,
                } if matches!(form, crate::error::KwargPath::Named(k) if k == "threshold")
            ),
            "expected derived TypeMismatch, got {err:?}"
        );
    }

    // ── compile_from_sexp form-shape primitives ───────────────────────

    #[test]
    fn head_mismatch_emits_structural_variant() {
        let err = head_mismatch("defmonitor", "not-a-monitor".into());
        assert!(
            matches!(
                err,
                LispError::HeadMismatch {
                    keyword: "defmonitor",
                    ref got,
                } if got == "not-a-monitor"
            ),
            "expected HeadMismatch variant, got {err:?}"
        );
    }

    #[test]
    fn head_mismatch_display_matches_legacy_compile_shape() {
        // Legacy shape (before this lift):
        //   "compile error in defmonitor: expected (defmonitor ...), got (X ...)"
        // The structural variant must render byte-for-byte the same so
        // existing consumer assertions (e.g. `tatara-check`, the
        // `derive_errors_on_wrong_head` test) pass unchanged.
        let err = head_mismatch("defmonitor", "not-a-monitor".into());
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: expected (defmonitor ...), got (not-a-monitor ...)"
        );
    }

    #[test]
    fn not_a_list_form_err_emits_structural_variant() {
        // After the structural lift the helper returns
        // `LispError::NotAListForm { keyword }`, not the legacy
        // `Compile { form, message }` triple. Pinning variant
        // identity (rather than substring-matching on `message ==
        // "expected list form"`) means a regression that revives
        // the `Compile`-shaped construction fails-loudly here.
        let err = not_a_list_form_err("defmonitor");
        assert!(
            matches!(
                err,
                LispError::NotAListForm {
                    keyword: "defmonitor"
                }
            ),
            "expected NotAListForm variant, got {err:?}"
        );
    }

    #[test]
    fn not_a_list_form_err_display_matches_legacy_compile_shape() {
        // Legacy shape (before this lift):
        //   "compile error in defmonitor: expected list form"
        // The structural variant must render byte-for-byte the
        // same so existing consumer assertions (e.g., the
        // `compile_from_sexp_emits_*_for_non_list_form` tests
        // against `MonitorSpec`, `tatara-check`'s diagnostic
        // capture, REPL substring matchers) pass unchanged.
        let err = not_a_list_form_err("defmonitor");
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: expected list form"
        );
    }

    #[test]
    fn missing_head_err_with_no_got_returns_missing_head_symbol_for_empty_list() {
        // The empty-list case (`()`) — `list.first()` returns `None`,
        // so the call site passes `got: None`. The builder returns
        // `LispError::MissingHeadSymbol { keyword, got: None }`
        // structurally — a regression that re-collapsed both sub-
        // modes into the legacy `Compile` shape would fail-loudly
        // here. Display-side coverage of the rendered message lives
        // in `tatara-lisp/src/error.rs`'s test module.
        let err = missing_head_err("defmonitor", None);
        assert!(
            matches!(
                err,
                LispError::MissingHeadSymbol {
                    keyword: "defmonitor",
                    got: None,
                }
            ),
            "expected MissingHeadSymbol {{ got: None }}, got {err:?}"
        );
    }

    #[test]
    fn missing_head_err_with_got_returns_missing_head_symbol_for_non_symbol_head() {
        // The present-but-not-symbol case (`(5 …)`, `(:foo …)`) —
        // `list.first()` returns `Some(non-symbol-sexp)`, so the
        // call site passes `got: Some(SexpWitness)`. The builder
        // returns `LispError::MissingHeadSymbol { keyword, got:
        // Some(_) }` structurally so the renderable detail names
        // the offending head, parallel to how
        // `RestParamMissingName.got: Some(_)` names the offending
        // post-`&rest` follower. The typed witness carries the
        // joint (`SexpShape::Int`, "5") identity so authoring tools
        // bind to `got.shape` directly across the rejection slot.
        let err = missing_head_err("defmonitor", Some(SexpWitness::new(SexpShape::Int, "5")));
        assert!(
            matches!(
                err,
                LispError::MissingHeadSymbol {
                    keyword: "defmonitor",
                    ref got,
                } if got.as_ref().map(|w| (w.shape, w.display.as_str())) == Some((SexpShape::Int, "5"))
            ),
            "expected MissingHeadSymbol {{ got: Some(SexpWitness {{ Int, \"5\" }}) }}, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_head_mismatch_for_wrong_head() {
        // End-to-end through the trait default: a `(not-a-monitor …)`
        // form fed to `MonitorSpec::compile_from_sexp` surfaces the
        // structural HeadMismatch — every derived domain (and every
        // hand-written impl that uses the trait default) inherits.
        let forms = read(r#"(not-a-monitor :name "x")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::HeadMismatch {
                    keyword: "defmonitor",
                    ref got,
                } if got == "not-a-monitor"
            ),
            "expected HeadMismatch, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_not_a_list_form_for_bare_atom() {
        // End-to-end through the trait default: a bare-atom form
        // (no parens) fed to `MonitorSpec::compile_from_sexp`
        // surfaces the structural `NotAListForm` variant — every
        // derived domain (and every hand-written impl that uses
        // the trait default) inherits the structural gate.
        let err = MonitorSpec::compile_from_sexp(&Sexp::int(7)).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NotAListForm {
                    keyword: "defmonitor"
                }
            ),
            "expected NotAListForm, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_not_a_list_form_for_keyword_atom() {
        // A keyword atom (`:foo`) is also a non-list — pin path-
        // uniformity across atom kinds. The keyword projection in
        // the variant doesn't change with the offending atom's
        // type because `NotAListForm` carries no `got` slot — the
        // failure mode IS "not a list", regardless of what kind
        // of atom was supplied.
        let err = MonitorSpec::compile_from_sexp(&Sexp::keyword("foo")).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NotAListForm {
                    keyword: "defmonitor"
                }
            ),
            "expected NotAListForm, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_not_a_list_form_display_matches_legacy() {
        // End-to-end Display rendering: a non-list form fed to
        // `compile_from_sexp` produces the byte-identical legacy
        // string that `tatara-check`, the REPL, and downstream
        // substring-matchers grep on.
        let err = MonitorSpec::compile_from_sexp(&Sexp::int(7)).unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: expected list form"
        );
    }

    #[test]
    fn compile_from_sexp_emits_missing_head_symbol_for_empty_list() {
        // `()` is a list whose first element doesn't exist — head can't
        // be projected to a symbol. The diagnostic names the failure
        // mode AND the structural reason (`(empty list)`) without
        // inventing a "got X" that isn't there. The variant carries
        // `got: None` so an authoring tool can render "your form is
        // empty" without re-parsing the source.
        let err = MonitorSpec::compile_from_sexp(&Sexp::List(vec![])).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::MissingHeadSymbol {
                    keyword: "defmonitor",
                    got: None,
                }
            ),
            "expected MissingHeadSymbol {{ got: None }}, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_missing_head_symbol_for_non_symbol_head() {
        // `(5 :name "x")` — list[0] is `5`, an int, not a symbol. The
        // gate fires AFTER the `as_list` projection succeeds and BEFORE
        // the keyword-equality check; the variant carries `got:
        // Some(SexpWitness { SexpShape::Int, "5" })` so an authoring
        // tool that wants to surface "your form's head is `5`, an int,
        // not a symbol" gains BOTH the typed shape (pattern-matchable)
        // AND the literal value as data, no re-parsing required. The
        // two sub-modes (`()` → `got: None`, `(5 …)` →
        // `got: Some(SexpWitness)`) bind to ONE structural variant —
        // same posture as `RestParamMissingName.got:
        // Option<SexpWitness>`.
        let forms = read(r#"(5 :name "x")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::MissingHeadSymbol {
                    keyword: "defmonitor",
                    ref got,
                } if got.as_ref().map(|w| (w.shape, w.display.as_str())) == Some((SexpShape::Int, "5"))
            ),
            "expected MissingHeadSymbol {{ got: Some(SexpWitness {{ Int, \"5\" }}) }}, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_missing_head_symbol_for_keyword_atom_head() {
        // `(:foo :name "x")` — list[0] is the keyword atom `:foo`, not
        // a symbol. The variant's `got` slot carries the typed witness
        // pairing `SexpShape::Keyword` with `Sexp::Display`'s
        // projection of the offending atom (`":foo"`) so the operator
        // sees what they wrote AND tools bind on the typed shape.
        // Pinning across atom kinds (int, keyword) demonstrates that
        // the structural binding is uniform for every non-symbol head.
        let forms = read(r#"(:foo :name "x")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::MissingHeadSymbol {
                    keyword: "defmonitor",
                    ref got,
                } if got.as_ref().map(|w| (w.shape, w.display.as_str())) == Some((SexpShape::Keyword, ":foo"))
            ),
            "expected MissingHeadSymbol {{ got: Some(SexpWitness {{ Keyword, \":foo\" }}) }}, got {err:?}"
        );
    }

    #[test]
    fn compile_from_sexp_emits_missing_head_symbol_display_matches_legacy_for_empty_list() {
        // End-to-end Display rendering for the empty-list case: the
        // legacy `Compile { form: "defmonitor", message: "missing head
        // symbol" }` substring (`"compile error in defmonitor:
        // missing head symbol"`) is preserved as the prefix
        // byte-for-byte; the structural detail (`(empty list)`) is
        // appended. Authoring tools (`tatara-check`, the REPL) that
        // substring-grep on the legacy rendering see no drift.
        let err = MonitorSpec::compile_from_sexp(&Sexp::List(vec![])).unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: missing head symbol (empty list)"
        );
    }

    #[test]
    fn compile_from_sexp_emits_missing_head_symbol_display_matches_legacy_for_non_symbol_head() {
        // End-to-end Display rendering for the non-symbol-head case:
        // the legacy substring is preserved as the prefix
        // byte-for-byte; the structural detail (`(got 5)`) names
        // the offending head's `Sexp::Display` projection.
        let forms = read(r#"(5 :name "x")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: missing head symbol (got 5)"
        );
    }

    #[test]
    fn head_mismatch_position_is_none_today() {
        // Negative-control: until `Sexp` carries spans, `position()`
        // returns `None` — `format_diagnostic` falls through to
        // single-line rendering. A future run that adds
        // `pos: Option<usize>` to `HeadMismatch` does so deliberately
        // with a fail-before/pass-after delta.
        let err = head_mismatch("defmonitor", "not-a-monitor".into());
        assert_eq!(err.position(), None);
    }

    // ── assert_tatara_domain_well_formed — the substrate-wide testkit ──

    #[test]
    fn assert_tatara_domain_well_formed_passes_on_the_derive_reference_impl() {
        // The reference implementor `MonitorSpec` inherits the trait
        // default `compile_from_sexp` through the derive; every one of
        // the four rejection gates (bare atom, empty list, non-symbol
        // head, wrong-head symbol) MUST fire with the substrate-wide
        // structural `LispError` variant, AND its KEYWORD `"defmonitor"`
        // MUST pass the three grammar invariants (non-empty; classifies
        // as `Atom::Symbol` via `Atom::from_lexeme`; contains no
        // `Sexp::is_bare_atom_boundary` char) AND the round-trip
        // theorem (`read("defmonitor")` projects to
        // `Some("defmonitor")`). The single line below pins all EIGHT
        // at once — every future `#[derive(TataraDomain)]` implementor
        // reduces to the same one-line check in its test module,
        // mirroring the `assert_closed_set_well_formed` deployment
        // across 44+ closed-set implementor test sites.
        assert_tatara_domain_well_formed::<MonitorSpec>();
    }

    #[test]
    fn assert_tatara_domain_well_formed_panics_on_empty_keyword() {
        // Negative arm on invariant (1) — a hand-written impl whose
        // KEYWORD is the empty string tries to be a keyword-less
        // dispatch target. The trait can't discriminate `(some-form
        // …)` from `(other-form …)` without a lexeme, so the testkit
        // MUST fire on this degenerate shape. Uses `catch_unwind` to
        // observe the panic without terminating the test process —
        // same posture the closed-set testkit's negative-arm tests
        // take (see `closed_set.rs`).
        struct EmptyKeyword;
        impl TataraDomain for EmptyKeyword {
            const KEYWORD: &'static str = "";
            fn compile_from_args(_: &[Sexp]) -> Result<Self> {
                unreachable!("compile_from_args unreachable — invariant (1) trips first")
            }
        }
        let result = std::panic::catch_unwind(|| {
            assert_tatara_domain_well_formed::<EmptyKeyword>();
        });
        let payload = result.expect_err("expected empty-KEYWORD invariant to panic");
        let msg = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&'static str>().copied())
            .unwrap_or("");
        assert!(
            msg.contains("KEYWORD is empty"),
            "expected empty-KEYWORD panic message to name the invariant, got {msg:?}",
        );
    }

    /// Extract the panic message of the closure so the test module's
    /// negative-arm sweep binds to ONE substrate-owned decode instead of
    /// re-inlining the `catch_unwind` + `downcast_ref::<String>` + fallback
    /// to `&'static str` cascade at every arm.
    fn assert_panic_msg_contains(needle: &str, f: impl FnOnce() + std::panic::UnwindSafe) {
        let result = std::panic::catch_unwind(f);
        let payload = result.expect_err("expected invariant to panic");
        let msg = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&'static str>().copied())
            .unwrap_or("");
        assert!(
            msg.contains(needle),
            "expected panic message to contain {needle:?}, got {msg:?}",
        );
    }

    #[test]
    fn assert_tatara_domain_well_formed_panics_on_ascii_whitespace_keyword() {
        // Negative arm on invariant (7) — a KEYWORD like `"def foo"`
        // would arrive at the trait's head-match as two tokens because
        // `Sexp::is_bare_atom_boundary(' ') == true` via
        // `char::is_whitespace`. The testkit MUST catch this before an
        // integration surface silently drops the trailing word. The
        // pre-lift ASCII-only heuristic caught the same case; the
        // sharpened invariant catches it via the substrate's typed
        // reader-boundary projection.
        struct WhitespaceKeyword;
        impl TataraDomain for WhitespaceKeyword {
            const KEYWORD: &'static str = "def foo";
            fn compile_from_args(_: &[Sexp]) -> Result<Self> {
                unreachable!("compile_from_args unreachable — invariant (7) trips first")
            }
        }
        assert_panic_msg_contains("reader-boundary char", || {
            assert_tatara_domain_well_formed::<WhitespaceKeyword>();
        });
    }

    #[test]
    fn assert_tatara_domain_well_formed_panics_on_unicode_whitespace_keyword() {
        // Negative arm on invariant (7) — a KEYWORD carrying the
        // no-break-space codepoint `\u{00A0}` (a Unicode-whitespace
        // char the pre-lift `is_ascii_whitespace()` check silently
        // accepted). The reader's outer-dispatch calls
        // `char::is_whitespace()` (Unicode-aware) via
        // `Sexp::is_bare_atom_boundary`, so a KEYWORD `"def\u{00A0}foo"`
        // would split into two tokens. Binding the invariant to the
        // substrate's typed reader-boundary projection closes this hole
        // that the pre-lift ASCII-only heuristic left open.
        struct NbspKeyword;
        impl TataraDomain for NbspKeyword {
            const KEYWORD: &'static str = "def\u{00A0}foo";
            fn compile_from_args(_: &[Sexp]) -> Result<Self> {
                unreachable!("compile_from_args unreachable — invariant (7) trips first")
            }
        }
        assert_panic_msg_contains("reader-boundary char", || {
            assert_tatara_domain_well_formed::<NbspKeyword>();
        });
    }

    #[test]
    fn assert_tatara_domain_well_formed_panics_on_list_open_char_keyword() {
        // Negative arm on invariant (7) — a KEYWORD like `"def(x"`
        // embeds `Sexp::LIST_OPEN` mid-lexeme; the reader's bare-atom
        // terminator disjunct fires on `(`, splitting the token so the
        // trait's head-match would see `"def"` followed by an opening
        // paren — the head-match would fire on `"def"`, silently
        // matching a DIFFERENT keyword. This is the reader-boundary
        // hole the pre-lift ASCII-whitespace heuristic silently
        // accepted; binding to `Sexp::is_bare_atom_boundary` catches
        // it structurally.
        struct ListOpenKeyword;
        impl TataraDomain for ListOpenKeyword {
            const KEYWORD: &'static str = "def(x";
            fn compile_from_args(_: &[Sexp]) -> Result<Self> {
                unreachable!("compile_from_args unreachable — invariant (7) trips first")
            }
        }
        assert_panic_msg_contains("reader-boundary char", || {
            assert_tatara_domain_well_formed::<ListOpenKeyword>();
        });
    }

    #[test]
    fn assert_tatara_domain_well_formed_panics_on_comment_lead_char_keyword() {
        // Negative arm on invariant (7) — a KEYWORD like `"def;bad"`
        // embeds `Sexp::COMMENT_LEAD` mid-lexeme; the reader's outer
        // dispatch would treat `;` as the start of a line comment,
        // discarding everything after it up to newline. The trait's
        // head-match would fire on `"def"` (the token before `;`),
        // silently matching a DIFFERENT keyword. Sibling coverage to
        // the list-open arm above on the seven-terminator disjunction
        // `Sexp::NON_WHITESPACE_BARE_ATOM_TERMINATORS`.
        struct CommentLeadKeyword;
        impl TataraDomain for CommentLeadKeyword {
            const KEYWORD: &'static str = "def;bad";
            fn compile_from_args(_: &[Sexp]) -> Result<Self> {
                unreachable!("compile_from_args unreachable — invariant (7) trips first")
            }
        }
        assert_panic_msg_contains("reader-boundary char", || {
            assert_tatara_domain_well_formed::<CommentLeadKeyword>();
        });
    }

    #[test]
    fn assert_tatara_domain_well_formed_panics_on_keyword_marker_prefix() {
        // Negative arm on invariant (6) — a KEYWORD `":foo"` classifies
        // as `Atom::Keyword` via the reader's `Atom::from_lexeme`
        // classifier (the `:` prefix is stripped and the remainder
        // becomes the keyword payload). The pre-lift "no leading ASCII
        // digit" heuristic silently accepted this shape; the sharpened
        // invariant binds to the substrate's typed classifier so the
        // shape rejects structurally.
        struct KeywordMarkerKeyword;
        impl TataraDomain for KeywordMarkerKeyword {
            const KEYWORD: &'static str = ":foo";
            fn compile_from_args(_: &[Sexp]) -> Result<Self> {
                unreachable!("compile_from_args unreachable — invariant (6) trips first")
            }
        }
        assert_panic_msg_contains("Atom::from_lexeme", || {
            assert_tatara_domain_well_formed::<KeywordMarkerKeyword>();
        });
    }

    #[test]
    fn assert_tatara_domain_well_formed_panics_on_bool_literal_keyword() {
        // Negative arm on invariant (6) — a KEYWORD `"#t"` classifies
        // as `Atom::Bool(true)` via `Atom::from_lexeme`'s bool-literal
        // arm. The pre-lift heuristic silently accepted this shape
        // (starts with `#`, not a digit); the sharpened invariant binds
        // to the substrate's typed classifier so the shape rejects
        // structurally. Peer coverage to the `:foo` arm above on the
        // classifier's non-`Symbol` decode paths.
        struct BoolLiteralKeyword;
        impl TataraDomain for BoolLiteralKeyword {
            const KEYWORD: &'static str = "#t";
            fn compile_from_args(_: &[Sexp]) -> Result<Self> {
                unreachable!("compile_from_args unreachable — invariant (6) trips first")
            }
        }
        assert_panic_msg_contains("Atom::from_lexeme", || {
            assert_tatara_domain_well_formed::<BoolLiteralKeyword>();
        });
    }

    #[test]
    fn assert_tatara_domain_well_formed_panics_on_numeric_keyword() {
        // Negative arm on invariant (6) — a KEYWORD `"42"` classifies
        // as `Atom::Int(42)` via `Atom::from_lexeme`'s `parse::<i64>`
        // arm. The pre-lift heuristic caught this via the leading-
        // digit check; the sharpened invariant catches it via the
        // classifier's typed decode — a stricter check with a
        // structurally-named diagnostic.
        struct NumericKeyword;
        impl TataraDomain for NumericKeyword {
            const KEYWORD: &'static str = "42";
            fn compile_from_args(_: &[Sexp]) -> Result<Self> {
                unreachable!("compile_from_args unreachable — invariant (6) trips first")
            }
        }
        assert_panic_msg_contains("Atom::from_lexeme", || {
            assert_tatara_domain_well_formed::<NumericKeyword>();
        });
    }

    #[test]
    fn assert_tatara_domain_well_formed_panics_on_drifted_compile_from_sexp() {
        // Negative arm on invariant (4) — an override that swallows
        // the bare-atom form and returns `Ok(_)` drifts the trait's
        // typed-entry gate; the testkit MUST fire on this drift so
        // the substrate-wide `NotAListForm` contract stays enforced
        // across every implementor rather than only across those that
        // keep the trait default.
        #[derive(Debug, PartialEq)]
        struct SwallowsBareAtom;
        impl TataraDomain for SwallowsBareAtom {
            const KEYWORD: &'static str = "defbogus";
            fn compile_from_args(_: &[Sexp]) -> Result<Self> {
                Ok(SwallowsBareAtom)
            }
            fn compile_from_sexp(_form: &Sexp) -> Result<Self> {
                // Intentionally-broken: this override accepts EVERY
                // form, including a bare atom — the drift the testkit
                // catches.
                Ok(SwallowsBareAtom)
            }
        }
        let result = std::panic::catch_unwind(|| {
            assert_tatara_domain_well_formed::<SwallowsBareAtom>();
        });
        let payload = result.expect_err("expected drifted-override invariant to panic");
        let msg = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&'static str>().copied())
            .unwrap_or("");
        assert!(
            msg.contains("accepted a bare-atom form"),
            "expected drifted-override panic message to name the invariant, got {msg:?}",
        );
    }

    // ── suggest — bounded edit-distance over a candidate set ──────────

    #[test]
    fn suggest_picks_single_typo_within_bound() {
        // `tthreshold` differs from `threshold` by one insertion (distance
        // 1). Length 10 → bound 3. The substrate names the likely intended
        // keyword.
        let allowed: &[&str] = &["name", "query", "threshold", "tags", "enabled"];
        assert_eq!(suggest("tthreshold", allowed), Some("threshold"));
    }

    #[test]
    fn suggest_picks_transposition_within_bound() {
        // `htreshold` is one transposition from `threshold` (distance 2 in
        // plain Levenshtein — one delete + one insert). Length 9 → bound 3.
        let allowed: &[&str] = &["name", "query", "threshold"];
        assert_eq!(suggest("htreshold", allowed), Some("threshold"));
    }

    #[test]
    fn suggest_returns_none_when_no_candidate_within_bound() {
        // `garbage` (length 7 → bound 2) is not within distance 2 of any
        // allowed kwarg. The substrate refuses to invent a hint when the
        // distance signal isn't there — a wrong hint is worse than none.
        let allowed: &[&str] = &["name", "query", "threshold", "tags", "enabled"];
        assert_eq!(suggest("garbage", allowed), None);
    }

    #[test]
    fn suggest_excludes_exact_match() {
        // An exact match means the caller already has the keyword; the
        // suggestion exists for near-misses only. Without this guard the
        // primitive would happily echo the input back.
        let allowed: &[&str] = &["name", "query", "threshold"];
        assert_eq!(suggest("name", allowed), None);
    }

    #[test]
    fn suggest_picks_lexicographically_smaller_on_distance_tie() {
        // Two candidates at the same distance — pick the lexicographically
        // smaller one so two operators on two machines see the same hint
        // for the same input. Diagnostics must be deterministic.
        let allowed: &[&str] = &["abc", "abd"]; // both distance 1 from "abe"
        assert_eq!(suggest("abe", allowed), Some("abc"));
    }

    #[test]
    fn suggest_handles_empty_candidates() {
        let allowed: &[&str] = &[];
        assert_eq!(suggest("anything", allowed), None);
    }

    #[test]
    fn suggest_bound_for_short_strings_rejects_distance_two() {
        // Needle length ≤ 3 → bound 1. `abc` vs `xyz` is distance 3 (full
        // replacement); short identifiers are too close to noise to trust
        // a multi-character hint. The bound floor stops false-positives
        // like `:to` matching `:do`.
        let allowed: &[&str] = &["xyz"];
        assert_eq!(suggest("abc", allowed), None);
    }

    #[test]
    fn suggest_bound_for_short_strings_accepts_distance_one() {
        // Within the short-string bound: a single character drift on a
        // 3-character identifier is suggestible.
        let allowed: &[&str] = &["abc"];
        assert_eq!(suggest("abd", allowed), Some("abc"));
    }

    #[test]
    fn suggest_handles_unicode_identifiers() {
        // `levenshtein` operates on chars, not bytes, so a multibyte typo
        // on a multibyte identifier measures character-distance — `é` is
        // one character, not two bytes. Tatara naming is Brazilian ×
        // Japanese (THEORY.md §II.3) so the substrate must not treat
        // non-ASCII as foreign.
        let allowed: &[&str] = &["forjé"];
        assert_eq!(suggest("forje", allowed), Some("forjé"));
    }

    #[test]
    fn reject_unknown_kwargs_includes_did_you_mean_for_near_miss() {
        // End-to-end: a near-miss in the typed-entry gate produces a hint
        // ahead of the allowed-list. The full allowed-list is still in
        // the message — the hint is purely additive.
        let forms = read(r#"(defmonitor :name "x" :tthreshold 0.99)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &[
            "name",
            "query",
            "threshold",
            "window-seconds",
            "tags",
            "enabled",
        ];
        let err = reject_unknown_kwargs(&kw, allowed).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("did you mean :threshold?"),
            "message must hint the near-match, got: {msg}"
        );
        assert!(
            msg.contains("allowed: "),
            "message must still list the allowed set, got: {msg}"
        );
        assert!(
            msg.contains("unknown keyword"),
            "message must still label the failure, got: {msg}"
        );
    }

    #[test]
    fn reject_unknown_kwargs_omits_did_you_mean_when_no_close_match() {
        // Negative control: when the offending keyword isn't within the
        // edit-distance bound of any allowed kwarg, no hint is fabricated.
        // A wrong hint is worse than no hint.
        let forms = read(r#"(defmonitor :name "x" :totally-unrelated 1)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &["name", "query", "threshold"];
        let err = reject_unknown_kwargs(&kw, allowed).unwrap_err();
        let msg = format!("{err}");
        assert!(
            !msg.contains("did you mean"),
            "message must not hint when no close match exists, got: {msg}"
        );
        assert!(
            msg.contains("unknown keyword"),
            "message must still label the failure, got: {msg}"
        );
    }

    #[test]
    fn derive_unknown_keyword_hints_near_miss() {
        // Every derived domain inherits the hint by sharing
        // `reject_unknown_kwargs` — no derive-emit change required.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tthreshold 0.99)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("did you mean :threshold?"),
            "derived domain must inherit the hint, got: {msg}"
        );
    }

    // ── suggest_keyword — registry-aware near-miss primitive ───────────
    //
    // Wraps `suggest` over `registered_keywords()`. Pinning behavior
    // here covers the substrate-side guarantee every consumer with an
    // unknown registry-dispatched form binds to: ONE primitive, not a
    // per-call-site `registered_keywords()` + `suggest` duplication.

    #[test]
    fn suggest_keyword_picks_near_miss_from_registry() {
        // Register MonitorSpec (idempotent — `register::<T>()` overwrites)
        // so the registry definitely contains `defmonitor` when this
        // test runs, regardless of test ordering.
        register::<MonitorSpec>();
        let hint: Option<&'static str> = suggest_keyword("defmoniter");
        assert_eq!(
            hint,
            Some("defmonitor"),
            "registry-aware near-miss must resolve `defmoniter` to `defmonitor`"
        );
    }

    #[test]
    fn suggest_keyword_excludes_exact_match() {
        // When the needle IS a registered keyword, no hint — the
        // suggestion is for near-misses only. Same posture `suggest`
        // takes for general candidate sets.
        register::<MonitorSpec>();
        assert_eq!(
            suggest_keyword("defmonitor"),
            None,
            "exact registry hits must not echo as suggestions"
        );
    }

    #[test]
    fn suggest_keyword_returns_none_when_no_close_match() {
        // Needle far enough from any plausible domain keyword that no
        // registered keyword (now or in the future) lands within the
        // bounded edit distance — no false-positive hint.
        register::<MonitorSpec>();
        assert_eq!(
            suggest_keyword("xyzqrstuvwx"),
            None,
            "needle outside the bound must not produce a hint"
        );
    }

    // ── unknown_domain_keyword — structural variant + named primitive ─
    //
    // Pairs `LispError::UnknownDomainKeyword { keyword, hint, registered }`
    // with `unknown_domain_keyword(keyword)` so the registry-dispatch
    // fallthrough (`tatara-check`'s unknown `(defX …)` path) binds to ONE
    // primitive instead of inline `format!("did you mean ({m} ...)? ")` +
    // `format!("Registered domains: {:?}", registered_keywords())` +
    // `report.fail(label, detail)` triples. The shape mirrors
    // `unknown_kwarg`: same three slots (offending key + optional hint +
    // sorted candidate set), same deterministic-ordering posture, same
    // owned-data lifetime contract — the substrate's unknown-something-
    // against-a-set diagnostic surface is now a single shape.
    //
    // Tests pin: variant identity, hint resolution, hint absence, sorted
    // determinism, kebab-case round-trip, end-to-end Display.

    #[test]
    fn unknown_domain_keyword_emits_structural_variant_with_hint() {
        register::<MonitorSpec>();
        let err = unknown_domain_keyword("defmoniter");
        match err {
            LispError::UnknownDomainKeyword {
                keyword,
                hint,
                registered,
            } => {
                assert_eq!(keyword, "defmoniter");
                assert_eq!(hint.as_deref(), Some("defmonitor"));
                assert!(
                    registered.contains(&"defmonitor".to_string()),
                    "registered set must include the registered keyword(s); got {registered:?}"
                );
            }
            other => panic!("expected UnknownDomainKeyword, got {other:?}"),
        }
    }

    #[test]
    fn unknown_domain_keyword_emits_structural_variant_without_hint_when_no_close_match() {
        // Needle far from any registered keyword — the hint slot stays
        // empty (a wrong hint is worse than no hint). This is the
        // structural counterpart to `suggest_keyword_returns_none_when_no_close_match`
        // — `unknown_domain_keyword` carries the absence into the variant.
        register::<MonitorSpec>();
        let err = unknown_domain_keyword("xyzqrstuvwx");
        match err {
            LispError::UnknownDomainKeyword {
                keyword,
                hint,
                registered,
            } => {
                assert_eq!(keyword, "xyzqrstuvwx");
                assert!(
                    hint.is_none(),
                    "needle outside the bound must produce no hint"
                );
                assert!(!registered.is_empty());
            }
            other => panic!("expected UnknownDomainKeyword, got {other:?}"),
        }
    }

    #[test]
    fn unknown_domain_keyword_sorts_registered_set_lexicographically() {
        // Registry iteration order is HashMap-derived (non-deterministic),
        // so the helper sorts the registered set before placing it in the
        // variant. A regression that drops the sort and lets HashMap
        // iteration order leak into the diagnostic fails-loudly here.
        register::<MonitorSpec>();
        let err = unknown_domain_keyword("totally-unrelated-form");
        match err {
            LispError::UnknownDomainKeyword { registered, .. } => {
                let mut expected = registered.clone();
                expected.sort();
                assert_eq!(
                    registered, expected,
                    "registered keyword set must be sorted lexicographically"
                );
            }
            other => panic!("expected UnknownDomainKeyword, got {other:?}"),
        }
    }

    #[test]
    fn unknown_domain_keyword_display_matches_structural_shape_with_hint() {
        // End-to-end Display from the helper: the offending head's call
        // shape, the structural near-miss in the same call shape, and
        // the registered set. The shape is byte-stable so authoring
        // surfaces that substring-match on the rendered diagnostic see
        // no drift across registry mutations (modulo the registered
        // set itself).
        register::<MonitorSpec>();
        let err = unknown_domain_keyword("defmoniter");
        let rendered = format!("{err}");
        assert!(
            rendered.starts_with("unknown domain keyword: (defmoniter ...)"),
            "rendered diagnostic must lead with the offending head: {rendered}"
        );
        assert!(
            rendered.contains("did you mean (defmonitor ...)?"),
            "rendered diagnostic must surface the structural near-miss: {rendered}"
        );
        assert!(
            rendered.contains("registered: "),
            "rendered diagnostic must include the registered set: {rendered}"
        );
    }

    #[test]
    fn unknown_domain_keyword_display_carries_kebab_case_keywords_unchanged() {
        // Kebab-cased domain keywords (a future `defalert-policy`,
        // `defprocess-spec`) round-trip through the offending-keyword
        // slot AND the registered-list slot unchanged. The substrate's
        // diagnostic surface respects the author's casing.
        let err = LispError::UnknownDomainKeyword {
            keyword: "defalert-policiy".into(),
            hint: Some("defalert-policy".into()),
            registered: vec!["defalert-policy".into(), "defprocess-spec".into()],
        };
        assert!(format!("{err}").contains("(defalert-policiy ...)"));
        assert!(format!("{err}").contains("(defalert-policy ...)?"));
        assert!(format!("{err}").contains("registered: defalert-policy, defprocess-spec"));
    }

    // ── Structural DuplicateKwarg variant ─────────────────────────────
    //
    // `parse_kwargs`'s top-level duplicate path and `sexp_to_json`'s
    // nested-kwargs duplicate path used to emit identical inline triples:
    //   `LispError::Compile { form: kwarg_form(k), message: "duplicate
    //    keyword".into() }`.
    // Two copies in one module is the prime-directive precursor to the
    // three-times rule (THEORY.md §VI.1) — and the diagnostic *category*
    // ("a kwargs slice contained `:k` twice") is structurally distinct
    // from every other typed-entry mismatch shape, so it deserves its
    // own structural variant the same way `OddKwargs` does.
    //
    // After this lift `parse_kwargs`'s diagnostic surface is structurally
    // complete — every distinct failure mode binds to ONE structural
    // variant of `LispError`:
    //   * odd length        → `LispError::OddKwargs { dangling }`
    //   * not-a-keyword-pos → `LispError::TypeMismatch { form, … }`
    //   * duplicate key     → `LispError::DuplicateKwarg { key }`
    // No `parse_kwargs` failure produces an unstructured `Compile` shape.
    //
    // Display matches the legacy `Compile`-shaped diagnostic byte-for-byte
    // so existing `msg.contains("duplicate keyword")` /
    // `msg.contains(":name")` assertions pass; the gain is structural —
    // authoring surfaces (REPL, LSP, `tatara-check`) bind to the variant.

    #[test]
    fn duplicate_kwarg_emits_structural_variant() {
        let err = duplicate_kwarg("name");
        match err {
            LispError::DuplicateKwarg { key } => assert_eq!(key, "name"),
            other => panic!("expected DuplicateKwarg, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_kwarg_display_matches_legacy_compile_shape() {
        // The user-visible string is byte-for-byte equivalent to the
        // pre-lift `LispError::Compile { form: ":name", message:
        // "duplicate keyword" }` rendering. Authoring surfaces that
        // pattern-match on the message text continue to work; tools that
        // pattern-match on the variant gain structural binding.
        let err = duplicate_kwarg("threshold");
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: duplicate keyword"
        );
    }

    #[test]
    fn duplicate_kwarg_preserves_kebab_case_keys() {
        // Multi-segment kebab-cased keys (`:notify-ref`, `:window-seconds`)
        // ride through unchanged. A regression that camelCases or
        // lowercases the key in the rendered diagnostic fails-loudly.
        let err = duplicate_kwarg("notify-ref");
        assert_eq!(
            format!("{err}"),
            "compile error in :notify-ref: duplicate keyword"
        );
    }

    #[test]
    fn parse_kwargs_top_level_duplicate_emits_structural_variant() {
        // `(_ :name "x" :name "y")` — top-level duplicate. Replaces the
        // legacy `Compile { form: ":name", message: "duplicate keyword" }`
        // shape with the structural `DuplicateKwarg { key: "name" }`.
        let args = kwargs_of(r#"(_ :name "x" :name "y")"#);
        let err = parse_kwargs(&args).unwrap_err();
        assert!(
            matches!(err, LispError::DuplicateKwarg { ref key } if key == "name"),
            "expected DuplicateKwarg {{ key: \"name\" }}, got {err:?}"
        );
    }

    #[test]
    fn parse_kwargs_duplicate_message_renders_canonical_shape() {
        // Pin the rendered Display shape so authoring tools that already
        // substring-match `duplicate keyword` (and `tatara-check`'s
        // user-defined `defcheck` macros) light up uniformly. A
        // regression that drifts the separator (e.g. `kwargs.name`) or
        // the label (e.g. `repeated key`) fails-loudly here.
        let args = kwargs_of(r#"(_ :threshold 0.1 :threshold 0.2)"#);
        let err = parse_kwargs(&args).unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: duplicate keyword"
        );
    }

    #[test]
    fn sexp_to_json_nested_duplicate_emits_structural_variant() {
        // `:step (:notify-ref "a" :notify-ref "b")` — the duplicate fires
        // during the `sexp_to_json` projection, before serde sees a
        // value. The lift gives the nested path the SAME structural
        // variant as the top-level path; the operator sees one shape
        // regardless of which depth misfired.
        let args = kwargs_of(r#"(_ :step (:notify-ref "a" :notify-ref "b"))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<EscalationStep>(&kw, "step").unwrap_err();
        assert!(
            matches!(err, LispError::DuplicateKwarg { ref key } if key == "notify-ref"),
            "expected DuplicateKwarg {{ key: \"notify-ref\" }} from nested kwargs, got {err:?}"
        );
    }

    #[test]
    fn extract_vec_via_serde_inner_duplicate_emits_structural_variant() {
        // `:steps ((:notify-ref "a" :notify-ref "b"))` — the duplicate is
        // inside one vec item. The `sexp_to_json` path fires before the
        // per-item serde wrapper sees a value, so the inner
        // `DuplicateKwarg` variant propagates with the inner kwarg's key
        // — not clobbered by `:steps[0]`. Pinning this means the
        // operator can pattern-match on `key == "notify-ref"` regardless
        // of vec nesting.
        let args = kwargs_of(r#"(_ :steps ((:notify-ref "a" :notify-ref "b")))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        assert!(
            matches!(err, LispError::DuplicateKwarg { ref key } if key == "notify-ref"),
            "expected DuplicateKwarg {{ key: \"notify-ref\" }} from vec-item kwargs, got {err:?}"
        );
    }

    #[test]
    fn derive_duplicate_kwarg_e2e_emits_structural_variant() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // every derived domain inherits the structural variant by
        // sharing `parse_kwargs`. No per-derive macro change is
        // required.
        let forms = read(r#"(defmonitor :name "x" :name "y" :query "q" :threshold 0.5)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(err, LispError::DuplicateKwarg { ref key } if key == "name"),
            "derived domain must surface DuplicateKwarg, got {err:?}"
        );
    }

    #[test]
    fn duplicate_kwarg_position_is_none_today() {
        // Negative-control: until `Sexp` carries spans, `position()`
        // returns `None` for the variant — `format_diagnostic` falls
        // through to single-line rendering, no caret emitted. Pinning
        // this contract means a future run that adds `pos: Option<usize>`
        // does so deliberately, with a fail-before/pass-after delta.
        let err = duplicate_kwarg("name");
        assert_eq!(err.position(), None);
    }

    #[test]
    fn suggest_keyword_result_is_static_str() {
        // The substrate hands back the SAME `&'static str` the registry
        // stores — every registered keyword is `'static` (the trait's
        // `KEYWORD` const), so `suggest_keyword` borrows from `'static`,
        // not from a temporary `Vec`. Pinning the lifetime here keeps
        // future consumers (LSP / REPL / forge) safe to embed the hint
        // in a `&'static str`-typed slot without an allocation.
        register::<MonitorSpec>();
        let hint: Option<&'static str> = suggest_keyword("defmoniter");
        // Force the result through a `'static`-bound slot — if the
        // signature ever drops `'static`, this fails to compile, which
        // is exactly the safety net we want.
        fn requires_static(_s: &'static str) {}
        if let Some(s) = hint {
            requires_static(s);
        }
        assert!(hint.is_some());
    }

    // ── Structural MissingKwarg variant ───────────────────────────────
    //
    // `required` is the kwarg-lookup helper that fronts every typed
    // extractor (`extract_string`, `extract_int`, `extract_float`,
    // `extract_bool`, `extract_via_serde`) and every hand-written
    // `TataraDomain` impl that needs a kwarg-by-runtime-key. It used to
    // assemble the "required but absent" diagnostic inline:
    //   `LispError::Compile { form: kwarg_form(key), message: "required
    //    but not provided".into() }`.
    // The diagnostic *category* ("a required kwarg :k was not provided")
    // is structurally distinct from every other typed-entry mismatch —
    // it has no `expected/got` axis, no item index, no near-miss hint —
    // so it deserves its own structural variant the same way `OddKwargs`
    // and `DuplicateKwarg` do.
    //
    // After this lift `parse_kwargs` + `required` cover every
    // typed-entry kwarg failure mode with a structural variant of
    // `LispError`:
    //   * odd length        → `LispError::OddKwargs { dangling }`
    //   * not-a-keyword-pos → `LispError::TypeMismatch { form, … }`
    //   * duplicate key     → `LispError::DuplicateKwarg { key }`
    //   * missing required  → `LispError::MissingKwarg { key }`
    // No kwarg-lookup failure produces an unstructured `Compile` shape.
    //
    // `MissingKwarg` is the runtime-key sibling of the pre-existing
    // `Missing(&'static str)` variant — `Missing` stays for compile-
    // time-known names; `MissingKwarg` covers the runtime-key path
    // every kwargs extractor shares.
    //
    // Display matches the legacy `Compile`-shaped diagnostic byte-for-
    // byte so existing `msg.contains("required")` /
    // `msg.contains(":threshold")` assertions pass unchanged; the gain
    // is structural — authoring surfaces (REPL, LSP, `tatara-check`)
    // bind to the variant.

    #[test]
    fn missing_kwarg_emits_structural_variant() {
        let err = missing_kwarg("name");
        match err {
            LispError::MissingKwarg { key } => assert_eq!(key, "name"),
            other => panic!("expected MissingKwarg, got {other:?}"),
        }
    }

    #[test]
    fn missing_kwarg_display_matches_legacy_compile_shape() {
        // The user-visible string is byte-for-byte equivalent to the
        // pre-lift `LispError::Compile { form: ":threshold", message:
        // "required but not provided" }` rendering. Authoring surfaces
        // that pattern-match on the message text continue to work; tools
        // that pattern-match on the variant gain structural binding.
        let err = missing_kwarg("threshold");
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: required but not provided"
        );
    }

    #[test]
    fn missing_kwarg_preserves_kebab_case_keys() {
        // Multi-segment kebab-cased keys (`:notify-ref`, `:window-seconds`)
        // ride through unchanged. A regression that camelCases or
        // lowercases the key in the rendered diagnostic fails-loudly.
        let err = missing_kwarg("notify-ref");
        assert_eq!(
            format!("{err}"),
            "compile error in :notify-ref: required but not provided"
        );
    }

    #[test]
    fn required_emits_structural_variant_when_absent() {
        // `(_ :other 1)` looking up `:level` — the kwarg is not in the
        // map. `required` must surface the structural `MissingKwarg`,
        // not the legacy `Compile`. Pin the variant identity AND the
        // key so a regression that re-inlines the inline shape fails-
        // loudly here.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = required(&kw, "level").unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "level"),
            "expected MissingKwarg {{ key: \"level\" }}, got {err:?}"
        );
    }

    #[test]
    fn required_present_kwarg_returns_value_unchanged() {
        // Negative-control: when the kwarg IS present, `required`
        // returns its value — the structural-variant lift is for the
        // absent-key path only.
        let args = kwargs_of(r#"(_ :level "info")"#);
        let kw = parse_kwargs(&args).unwrap();
        let v = required(&kw, "level").expect("present kwarg must return Ok");
        assert_eq!(v.as_string(), Some("info"));
    }

    #[test]
    fn required_message_renders_canonical_shape() {
        // Pin the rendered Display shape so authoring tools that already
        // substring-match `required` (and `tatara-check`'s
        // user-defined `defcheck` macros) light up uniformly. A
        // regression that drifts the separator or the label fails-
        // loudly here.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = required(&kw, "threshold").unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: required but not provided"
        );
    }

    // ── optional: the may-be-absent kwargs-lookup primitive ──────────────
    //
    // `optional(kw, key)` is the typed-entry kwargs-gate's sibling of
    // `required`: present → `Some(&Sexp)`, absent → `None`. Before this
    // lift the same `kw.get(key).copied()` projection was inlined at
    // FOUR sites — `required` (composed atop `optional + ok_or_else`),
    // `extract_optional_atom` (absence → `Ok(None)`), `extract_list`
    // (absence → `Ok(Vec::new())`), and `extract_optional_via_serde`
    // (absence → `Ok(None)`). The five tests below pin: (a) absent →
    // `None`; (b) present → `Some(&Sexp)` with value equality; (c) the
    // returned `&Sexp` borrows from the kwargs map (lifetime contract);
    // (d) the sibling composition `required = optional + ok_or_else`
    // is structurally observable; (e) the three optional-path
    // extractor consumers route through it (path-uniformity across
    // the lift). Together they pin the named PAIR `{required,
    // optional}` as the substrate's typed-entry kwargs-lookup
    // surface.

    #[test]
    fn optional_returns_none_when_key_absent() {
        // Negative-control: the kwarg is not in the map, so `optional`
        // surfaces `None` — the consumer's absence-arm input. No
        // diagnostic, no allocation, no `Result` indirection.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        assert!(optional(&kw, "level").is_none());
    }

    #[test]
    fn optional_returns_some_value_when_key_present() {
        // Positive-control: the kwarg IS in the map, so `optional`
        // surfaces `Some(&Sexp)` with the bound value. The returned
        // reference carries the same value `parse_kwargs` parked at
        // the key — no copying, no normalization.
        let args = kwargs_of(r#"(_ :level "info")"#);
        let kw = parse_kwargs(&args).unwrap();
        let v = optional(&kw, "level").expect("present kwarg must return Some");
        assert_eq!(v.as_string(), Some("info"));
    }

    #[test]
    fn optional_borrow_lifetime_outlives_map_lookup() {
        // The returned `&'a Sexp` borrows from the kwargs map's value
        // slot via `.copied()`, so a consumer can hold the reference
        // through its absence-arm match without an intermediate
        // clone — same lifetime contract as `required`'s `Ok(&Sexp)`
        // return. Pin it by reading the value AFTER the
        // `Option::expect`, against a freshly-bound reference whose
        // lifetime is tied to the outer `kw` binding.
        let args = kwargs_of(r#"(_ :name "obs" :threshold 0.99)"#);
        let kw = parse_kwargs(&args).unwrap();
        let name_ref: &Sexp = optional(&kw, "name").expect("present");
        let thr_ref: &Sexp = optional(&kw, "threshold").expect("present");
        assert_eq!(name_ref.as_string(), Some("obs"));
        assert_eq!(thr_ref.as_float(), Some(0.99));
    }

    #[test]
    fn required_is_optional_composed_with_missing_kwarg() {
        // The sibling composition `required = optional +
        // ok_or_else(missing_kwarg)` is structurally observable: on
        // the absent path, `required(kw, key).unwrap_err()` and
        // `optional(kw, key).ok_or_else(|| missing_kwarg(key))
        // .unwrap_err()` must produce structurally-equal errors;
        // on the present path, both projections name the SAME `&Sexp`
        // pointer (via `Result::ok` / `Option`). Pin both directions.
        let args = kwargs_of(r#"(_ :name "obs")"#);
        let kw = parse_kwargs(&args).unwrap();

        // Present-path identity: required returns the same &Sexp the
        // optional lookup found.
        let via_required = required(&kw, "name").expect("present");
        let via_optional = optional(&kw, "name").expect("present");
        assert!(
            std::ptr::eq(via_required, via_optional),
            "required and optional must surface the SAME &Sexp pointer for a present kwarg"
        );

        // Absent-path identity: required's error matches the closed-
        // form composition's error.
        let err_required = required(&kw, "absent").unwrap_err();
        let err_composed = optional(&kw, "absent")
            .ok_or_else(|| missing_kwarg("absent"))
            .unwrap_err();
        assert_eq!(format!("{err_required}"), format!("{err_composed}"));
        assert!(matches!(
            err_required,
            LispError::MissingKwarg { ref key } if key == "absent"
        ));
    }

    #[test]
    fn extract_optional_atom_routes_through_optional() {
        // Path-uniformity: `extract_optional_string` (which fronts
        // `extract_optional_atom`) now reads the kwarg through
        // `optional`. Absent → `Ok(None)` (no rejection); present →
        // `Ok(Some(value))`. Pin both arms so a regression that
        // re-inlines the `kw.get(key).copied()` projection at the
        // call site fails loudly here.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        assert_eq!(
            extract_optional_string(&absent_kw, "name").unwrap(),
            None,
            "absent optional kwarg must surface as Ok(None)"
        );

        let present_args = kwargs_of(r#"(_ :name "obs")"#);
        let present_kw = parse_kwargs(&present_args).unwrap();
        assert_eq!(
            extract_optional_string(&present_kw, "name").unwrap(),
            Some("obs"),
            "present optional kwarg must surface as Ok(Some(value))"
        );
    }

    #[test]
    fn extract_list_routes_through_optional_on_absent_key() {
        // Path-uniformity: `extract_string_list` (which fronts
        // `extract_list`) now reads the kwarg through `optional`.
        // Absent → `Ok(Vec::new())` (the empty-list absence floor —
        // never an error, parallel to `extract_optional_atom`'s
        // `Ok(None)`).
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(
            extract_string_list(&kw, "tags").unwrap(),
            Vec::<String>::new(),
            "absent list kwarg must surface as Ok(Vec::new())"
        );
    }

    #[test]
    fn extract_optional_via_serde_routes_through_optional() {
        // Path-uniformity: `extract_optional_via_serde` now reads the
        // kwarg through `optional`. Absent → `Ok(None)`; present →
        // `Ok(Some(value))` after the canonical-JSON round-trip.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        let absent: Option<i64> = extract_optional_via_serde(&absent_kw, "n").unwrap();
        assert_eq!(absent, None, "absent serde-fallthrough kwarg → Ok(None)");

        let present_args = kwargs_of("(_ :n 42)");
        let present_kw = parse_kwargs(&present_args).unwrap();
        let present: Option<i64> = extract_optional_via_serde(&present_kw, "n").unwrap();
        assert_eq!(
            present,
            Some(42),
            "present serde-fallthrough kwarg → Ok(Some(value))"
        );
    }

    #[test]
    fn extract_string_missing_emits_structural_variant() {
        // `extract_string` fronts every other typed extractor by
        // routing through `required`. A missing kwarg must produce
        // `MissingKwarg`, not `TypeMismatch` (no value to type-check)
        // and not `Compile` (legacy shape). Pin the routing.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string(&kw, "name").unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "name"),
            "expected MissingKwarg from extract_string, got {err:?}"
        );
    }

    #[test]
    fn extract_int_missing_emits_structural_variant() {
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_int(&kw, "n").unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "n"),
            "expected MissingKwarg from extract_int, got {err:?}"
        );
    }

    #[test]
    fn extract_float_missing_emits_structural_variant() {
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_float(&kw, "ratio").unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "ratio"),
            "expected MissingKwarg from extract_float, got {err:?}"
        );
    }

    #[test]
    fn extract_bool_missing_emits_structural_variant() {
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_bool(&kw, "enabled").unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "enabled"),
            "expected MissingKwarg from extract_bool, got {err:?}"
        );
    }

    #[test]
    fn extract_via_serde_missing_emits_structural_variant() {
        // The serde-fallthrough path also routes through `required`, so
        // every typed `Deserialize` field (enums, nested structs, vecs
        // of nested structs) inherits the structural variant for the
        // absent-key case — uniform shape across the typed-extractor
        // and the serde-fallthrough surfaces.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<Severity>(&kw, "level").unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "level"),
            "expected MissingKwarg from extract_via_serde, got {err:?}"
        );
    }

    #[test]
    fn derive_missing_required_kwarg_e2e_emits_structural_variant() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // omitting the required `:threshold` must surface the structural
        // `MissingKwarg { key: "threshold" }` — every derived domain
        // inherits the lift by sharing `required`. No per-derive macro
        // change required.
        let forms = read(r#"(defmonitor :name "x" :query "q")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(err, LispError::MissingKwarg { ref key } if key == "threshold"),
            "derived domain must surface MissingKwarg, got {err:?}"
        );
    }

    #[test]
    fn missing_kwarg_position_is_none_today() {
        // Negative-control for the future-spans move: until `Sexp`
        // carries source positions, the variant's `position()` returns
        // `None`. Pinning this contract means a future run that adds
        // `pos: Option<usize>` to `MissingKwarg` does so deliberately —
        // the missing-kwarg path picks up the span automatically because
        // it routes through the same primitive (`missing_kwarg`) as
        // every other call site.
        let err = missing_kwarg("name");
        assert_eq!(err.position(), None);
    }

    // ── Structural UnknownKwarg variant ───────────────────────────────
    //
    // `reject_unknown_kwargs` used to assemble its diagnostic inline:
    //   `LispError::Compile { form: kwarg_form(key), message: format!(
    //       "unknown keyword (did you mean :{hint}?; allowed: ...)"
    //    ) }`
    // — the offending key, the near-miss hint, and the allowed-set
    // were all welded into a free-form `message` string. After this
    // lift the three slots are first-class fields on
    // `LispError::UnknownKwarg { key, hint, allowed }`, so authoring
    // surfaces (REPL, LSP, `tatara-check`) bind to the variant
    // structurally instead of substring-parsing the rendered message.
    //
    // This is the FIFTH and LAST structural-variant lift on the
    // typed-entry kwarg-gate's diagnostic surface — every distinct
    // failure mode is now a structural variant of `LispError`:
    //   * odd length        → `LispError::OddKwargs { dangling }`
    //   * not-a-keyword-pos → `LispError::TypeMismatch { form, … }`
    //   * duplicate key     → `LispError::DuplicateKwarg { key }`
    //   * missing required  → `LispError::MissingKwarg { key }`
    //   * unknown keyword   → `LispError::UnknownKwarg { key, hint,
    //                                                   allowed }`
    // No kwarg-gate failure produces an unstructured `Compile` shape.
    //
    // Display matches the legacy `Compile`-shaped diagnostic byte-
    // for-byte so existing `msg.contains("unknown keyword")` /
    // `msg.contains(":threshold")` / `msg.contains("did you mean
    // :threshold?")` / `msg.contains("allowed: ")` assertions pass;
    // the gain is structural — authoring surfaces bind to the variant.

    #[test]
    fn unknown_kwarg_emits_structural_variant_with_hint() {
        // `tthreshold` is a near-miss of `threshold`; `suggest` ranks
        // it within the bounded edit distance, so `unknown_kwarg`
        // populates the `hint` slot with the allowed candidate.
        let allowed: &[&str] = &["name", "query", "threshold"];
        let err = unknown_kwarg("tthreshold", allowed);
        match err {
            LispError::UnknownKwarg {
                key,
                hint,
                allowed: alw,
            } => {
                assert_eq!(key, "tthreshold");
                assert_eq!(hint.as_deref(), Some("threshold"));
                // `unknown_kwarg` sorts the allowed set lexicographically
                // so two operators on two machines see the same
                // diagnostic for the same input — diagnostics are
                // deterministic regardless of HashMap iteration order.
                assert_eq!(alw, vec!["name", "query", "threshold"]);
            }
            other => panic!("expected UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn unknown_kwarg_emits_structural_variant_without_hint_when_no_close_match() {
        // Negative control: when the offending keyword isn't within the
        // edit-distance bound of any allowed kwarg, no hint is
        // fabricated. A wrong hint is worse than no hint.
        let allowed: &[&str] = &["name", "query", "threshold"];
        let err = unknown_kwarg("totally-unrelated", allowed);
        match err {
            LispError::UnknownKwarg {
                key,
                hint,
                allowed: alw,
            } => {
                assert_eq!(key, "totally-unrelated");
                assert!(hint.is_none(), "no near-miss must produce no hint");
                assert_eq!(alw, vec!["name", "query", "threshold"]);
            }
            other => panic!("expected UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn unknown_kwarg_sorts_allowed_set_lexicographically() {
        // `unknown_kwarg` is the single named primitive that materializes
        // the allowed-set as owned `Vec<String>` and sorts it
        // lexicographically. Pin the sort so a regression that drops it
        // (and thus drifts the rendered message order across HashMap
        // iteration ordering) fails-loudly here.
        let allowed: &[&str] = &["zeta", "alpha", "mu", "beta"];
        let err = unknown_kwarg("xx", allowed);
        match err {
            LispError::UnknownKwarg { allowed: alw, .. } => {
                assert_eq!(alw, vec!["alpha", "beta", "mu", "zeta"]);
            }
            other => panic!("expected UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn unknown_kwarg_display_with_hint_matches_legacy_compile_shape() {
        // The user-visible string is byte-for-byte equivalent to the
        // pre-lift `LispError::Compile { form: ":tthreshold", message:
        // "unknown keyword (did you mean :threshold?; allowed: :name,
        // :query, :threshold)" }` rendering. Authoring surfaces that
        // pattern-match on the message text continue to work; tools
        // that pattern-match on the variant gain structural binding.
        let allowed: &[&str] = &["name", "query", "threshold"];
        let err = unknown_kwarg("tthreshold", allowed);
        assert_eq!(
            format!("{err}"),
            "compile error in :tthreshold: unknown keyword \
             (did you mean :threshold?; allowed: :name, :query, :threshold)"
        );
    }

    #[test]
    fn unknown_kwarg_display_without_hint_matches_legacy_compile_shape() {
        let allowed: &[&str] = &["name", "query", "threshold"];
        let err = unknown_kwarg("totally-unrelated", allowed);
        assert_eq!(
            format!("{err}"),
            "compile error in :totally-unrelated: unknown keyword \
             (allowed: :name, :query, :threshold)"
        );
    }

    #[test]
    fn unknown_kwarg_preserves_kebab_case_keys() {
        // `:notify-ref`, `:window-seconds`, every kebab-cased kwarg
        // name round-trips through both the offending-key slot AND the
        // allowed-list slot unchanged. A regression that camelCases or
        // lowercases either side fails-loudly here.
        let allowed: &[&str] = &["notify-ref", "window-seconds"];
        let err = unknown_kwarg("windou-seconds", allowed);
        assert_eq!(
            format!("{err}"),
            "compile error in :windou-seconds: unknown keyword \
             (did you mean :window-seconds?; allowed: :notify-ref, :window-seconds)"
        );
    }

    #[test]
    fn reject_unknown_kwargs_emits_structural_variant_for_typo() {
        // End-to-end: `reject_unknown_kwargs` must surface the
        // structural `UnknownKwarg`, not the legacy `Compile`. Pin the
        // variant identity AND the key so a regression that re-inlines
        // the inline shape fails-loudly here.
        let forms = read(r#"(defmonitor :name "x" :tthreshold 0.99)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &[
            "name",
            "query",
            "threshold",
            "window-seconds",
            "tags",
            "enabled",
        ];
        let err = reject_unknown_kwargs(&kw, allowed).unwrap_err();
        match err {
            LispError::UnknownKwarg {
                key,
                hint,
                allowed: alw,
            } => {
                assert_eq!(key, "tthreshold");
                assert_eq!(hint.as_deref(), Some("threshold"));
                assert!(
                    alw.contains(&"threshold".to_string()),
                    "allowed-set must include `threshold`, got {alw:?}"
                );
                assert_eq!(
                    alw,
                    vec![
                        "enabled",
                        "name",
                        "query",
                        "tags",
                        "threshold",
                        "window-seconds"
                    ],
                    "allowed-set must be lexicographically sorted"
                );
            }
            other => panic!("expected UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn reject_unknown_kwargs_passes_when_all_known_returns_ok() {
        // Negative control: when every kwarg IS in the allowed set,
        // `reject_unknown_kwargs` returns `Ok(())` — the structural-
        // variant lift is for the unknown path only.
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold 0.5)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &["name", "query", "threshold"];
        assert!(reject_unknown_kwargs(&kw, allowed).is_ok());
    }

    #[test]
    fn derive_unknown_kwarg_e2e_emits_structural_variant() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // a typo'd `:tthreshold` must surface the structural
        // `UnknownKwarg { key: "tthreshold", hint: Some("threshold"),
        // allowed }` — every derived domain inherits the lift by
        // sharing `reject_unknown_kwargs`. No per-derive macro change
        // required.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tthreshold 0.99)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        match err {
            LispError::UnknownKwarg { key, hint, .. } => {
                assert_eq!(key, "tthreshold");
                assert_eq!(hint.as_deref(), Some("threshold"));
            }
            other => panic!("derived domain must surface UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn unknown_kwarg_position_is_none_today() {
        // Negative-control for the future-spans move: until `Sexp`
        // carries source positions, the variant's `position()` returns
        // `None`. Pinning this contract means a future run that adds
        // `pos: Option<usize>` to `UnknownKwarg` does so deliberately —
        // the unknown-kwarg path picks up the span automatically
        // because it routes through the same primitive
        // (`unknown_kwarg`) as every other call site.
        let allowed: &[&str] = &["name"];
        let err = unknown_kwarg("xx", allowed);
        assert_eq!(err.position(), None);
    }

    // ── parse_kwargs_strict: the fused typed-entry kwargs gate ──────────
    //
    // `parse_kwargs_strict(args, allowed)` is the substrate-level
    // composition of `parse_kwargs` + `reject_unknown_kwargs`, the
    // two-call sequence every `#[derive(TataraDomain)]`-generated
    // `compile_from_args` emits at its header and every hand-written
    // impl in the forge / lattice / tameshi crates inlines verbatim.
    // After this lift the fleet's seven-plus consumers (and every
    // future derived domain) route through ONE function the substrate
    // owns, instead of two functions every consumer must remember to
    // call in the canonical parse-then-reject order.
    //
    // The tests below pin the fused primitive's contract: (a) on
    // well-formed input it produces the same `Kwargs<'_>` map as the
    // two-step inlined call; (b)-(d) every parse-stage failure mode
    // surfaces as the same structural variant `parse_kwargs` would
    // raise (`OddKwargs` / `DuplicateKwarg` / `TypeMismatch`);
    // (e) every reject-stage failure surfaces as the same `UnknownKwarg`
    // `reject_unknown_kwargs` would raise; (f)-(g) parse-stage rejection
    // STRICTLY precedes reject-stage rejection — calls that violate
    // BOTH stages surface the parse-stage variant, never the reject-
    // stage variant; (h) an empty allowed-set rejects every parsed
    // kwarg as unknown (negative control on the closed-set posture);
    // (i) end-to-end through `MonitorSpec::compile_from_args` — the
    // derive's emit now routes through `parse_kwargs_strict`, so a
    // derived domain's diagnostics inherit the fused primitive's
    // single-call-site identity.

    #[test]
    fn parse_kwargs_strict_well_formed_input_matches_two_step_path() {
        // Path-uniformity: on a well-formed kwargs run with every key
        // in the allowed set, `parse_kwargs_strict` returns the same
        // map `parse_kwargs` would, with `reject_unknown_kwargs` having
        // returned `Ok(())` against it. The fused primitive is the
        // substrate-level composition of the two stages; on the
        // happy path the composition is observationally identical to
        // the two-step inlined call.
        let args = [
            Sexp::keyword("name"),
            Sexp::string("x"),
            Sexp::keyword("query"),
            Sexp::string("q"),
            Sexp::keyword("threshold"),
            Sexp::float(0.5),
        ];
        let allowed: &[&str] = &["name", "query", "threshold"];

        let fused = parse_kwargs_strict(&args, allowed).expect("well-formed must parse strictly");
        let staged = parse_kwargs(&args).expect("well-formed must parse");
        assert!(reject_unknown_kwargs(&staged, allowed).is_ok());

        // The fused map has the same keys + structurally-equal values
        // as the two-step map. (We compare via the sorted key list +
        // per-key Sexp equality because `&Sexp` borrows from `args` on
        // both sides — same lifetime, same source slice.)
        let mut fused_keys: Vec<&str> = fused.keys().map(String::as_str).collect();
        let mut staged_keys: Vec<&str> = staged.keys().map(String::as_str).collect();
        fused_keys.sort();
        staged_keys.sort();
        assert_eq!(fused_keys, staged_keys);
        for k in fused_keys {
            assert_eq!(fused.get(k), staged.get(k));
        }
    }

    #[test]
    fn parse_kwargs_strict_routes_odd_length_to_parse_stage_variant() {
        // Parse-stage rejection: an odd-length kwargs tail must surface
        // as `LispError::OddKwargs` — the same structural variant
        // `parse_kwargs` would raise. The reject-stage never runs
        // because the parse stage short-circuits on `Err`.
        let args = [
            Sexp::keyword("name"),
            Sexp::string("x"),
            Sexp::keyword("query"),
        ];
        let allowed: &[&str] = &["name", "query"];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("odd-length args must reject at parse stage");
        match err {
            LispError::OddKwargs { dangling } => {
                assert_eq!(dangling, ":query");
            }
            other => panic!("expected OddKwargs, got {other:?}"),
        }
    }

    #[test]
    fn parse_kwargs_strict_routes_duplicate_key_to_parse_stage_variant() {
        // Parse-stage rejection: a repeated `:name` key must surface as
        // `LispError::DuplicateKwarg` — same posture as `parse_kwargs`.
        let args = [
            Sexp::keyword("name"),
            Sexp::string("a"),
            Sexp::keyword("name"),
            Sexp::string("b"),
        ];
        let allowed: &[&str] = &["name"];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("duplicate-key args must reject at parse stage");
        match err {
            LispError::DuplicateKwarg { key } => {
                assert_eq!(key, "name");
            }
            other => panic!("expected DuplicateKwarg, got {other:?}"),
        }
    }

    #[test]
    fn parse_kwargs_strict_routes_non_keyword_position_to_type_mismatch_variant() {
        // Parse-stage rejection: an integer where a keyword was expected
        // (position 0) must surface as `LispError::TypeMismatch` with
        // `form = kwargs_pos_form(0)` and `expected = Keyword` — same
        // posture as `parse_kwargs`'s direct slot-must-be-a-keyword
        // rejection.
        let args = [Sexp::int(5), Sexp::string("x")];
        let allowed: &[&str] = &["name"];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("non-keyword at key position must reject at parse stage");
        match err {
            LispError::TypeMismatch { expected, got, .. } => {
                assert_eq!(expected, ExpectedKwargShape::Keyword);
                assert_eq!(got, SexpShape::Int);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn parse_kwargs_strict_routes_unknown_kwarg_to_reject_stage_variant() {
        // Reject-stage rejection: a well-formed parse with a key
        // OUTSIDE the allowed set surfaces as `LispError::UnknownKwarg`
        // with the typed `hint` / `allowed` slots — same posture as
        // `reject_unknown_kwargs`.
        let args = [
            Sexp::keyword("name"),
            Sexp::string("x"),
            Sexp::keyword("tthreshold"),
            Sexp::float(0.99),
        ];
        let allowed: &[&str] = &["name", "threshold"];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("unknown kwarg must reject at reject stage");
        match err {
            LispError::UnknownKwarg {
                key,
                hint,
                allowed: alw,
            } => {
                assert_eq!(key, "tthreshold");
                assert_eq!(hint.as_deref(), Some("threshold"));
                assert_eq!(alw, vec!["name", "threshold"]);
            }
            other => panic!("expected UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn parse_kwargs_strict_parse_stage_fires_before_reject_stage_on_odd_length() {
        // Stage-ordering: a call whose tail is BOTH odd-length AND
        // contains an unknown kwarg surfaces as `OddKwargs` (parse
        // stage), NOT `UnknownKwarg` (reject stage). The fused
        // primitive's composition order is load-bearing: parse runs
        // first, reject runs second, and the second stage cannot
        // observe an `Err` from the first.
        let args = [
            Sexp::keyword("ghost"),
            Sexp::string("boo"),
            Sexp::keyword("orphan"),
        ];
        let allowed: &[&str] = &["name"];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("odd-length + unknown must reject at parse stage");
        match err {
            LispError::OddKwargs { dangling } => {
                assert_eq!(dangling, ":orphan");
            }
            other => panic!("expected OddKwargs (parse stage fires first), got {other:?}",),
        }
    }

    #[test]
    fn parse_kwargs_strict_parse_stage_fires_before_reject_stage_on_duplicate() {
        // Stage-ordering, sibling case: a duplicate-key kwargs tail with
        // an extra unknown key still surfaces as `DuplicateKwarg`
        // (parse stage). The parse-stage walk reaches the duplicate
        // BEFORE the reject stage ever inspects the keyset.
        let args = [
            Sexp::keyword("name"),
            Sexp::string("a"),
            Sexp::keyword("ghost"),
            Sexp::string("boo"),
            Sexp::keyword("name"),
            Sexp::string("b"),
        ];
        let allowed: &[&str] = &["name"];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("duplicate + unknown must reject at parse stage");
        match err {
            LispError::DuplicateKwarg { key } => {
                assert_eq!(key, "name");
            }
            other => panic!("expected DuplicateKwarg (parse stage fires first), got {other:?}",),
        }
    }

    #[test]
    fn parse_kwargs_strict_empty_allowed_set_rejects_every_parsed_kwarg() {
        // Closed-set posture floor: an empty `allowed: &[]` means the
        // domain admits NO kwargs at all. Any well-formed kwarg
        // parses successfully but the reject stage rejects the first
        // key it sees as `UnknownKwarg`. The allowed-set lives at
        // ONE call site (`parse_kwargs_strict`), so a future "domain
        // with no kwargs" emits `parse_kwargs_strict(args, &[])` and
        // inherits the rejection posture without re-deriving it.
        let args = [Sexp::keyword("name"), Sexp::string("x")];
        let allowed: &[&str] = &[];
        let err = parse_kwargs_strict(&args, allowed)
            .expect_err("empty allowed-set must reject any parsed kwarg");
        match err {
            LispError::UnknownKwarg {
                key,
                hint,
                allowed: alw,
            } => {
                assert_eq!(key, "name");
                // No allowed candidates → no near-miss hint possible.
                assert_eq!(hint, None);
                assert!(
                    alw.is_empty(),
                    "empty allowed-set surfaces verbatim, got {alw:?}"
                );
            }
            other => panic!("expected UnknownKwarg, got {other:?}"),
        }
    }

    #[test]
    fn parse_kwargs_strict_powers_the_derive_emit_end_to_end() {
        // End-to-end path-uniformity: `#[derive(TataraDomain)]` on
        // `MonitorSpec` now emits ONE `parse_kwargs_strict` call in its
        // `compile_from_args` body in place of the prior two-call
        // sequence. The diagnostic identity of an unknown-kwarg
        // rejection from a derived domain MUST equal the diagnostic
        // identity of a direct `parse_kwargs_strict` call on the same
        // args — that's the substrate guarantee the lift establishes:
        // every consumer routes through ONE function. A regression
        // that drifts the derive's emit to re-inline the two-call
        // sequence (or worse, swap them) is structurally observable
        // here as a divergence between the two diagnostic paths.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tthreshold 0.99)"#).unwrap();
        let derive_err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();

        let args = forms[0].as_list().unwrap();
        let allowed: &[&str] = &[
            "name",
            "query",
            "threshold",
            "window-seconds",
            "tags",
            "enabled",
        ];
        let strict_err = parse_kwargs_strict(&args[1..], allowed)
            .expect_err("strict call must reject the unknown kwarg");

        match (derive_err, strict_err) {
            (
                LispError::UnknownKwarg {
                    key: dk,
                    hint: dh,
                    allowed: da,
                },
                LispError::UnknownKwarg {
                    key: sk,
                    hint: sh,
                    allowed: sa,
                },
            ) => {
                assert_eq!(dk, sk);
                assert_eq!(dh, sh);
                assert_eq!(da, sa);
            }
            (dother, sother) => panic!(
                "expected matching UnknownKwarg variants, got derive={dother:?} strict={sother:?}",
            ),
        }
    }

    // ── domain-keyed serialize / rewriter-output emission shape ────────
    //
    // The two byte-identical inline `LispError::Compile { form:
    // T::KEYWORD.to_string(), message: format!("serialize…: {e}") }`
    // sites — `register::<T>` (registry-dispatch closure) and
    // `rewrite_typed::<T>` (round-trip prelude) — funnel through
    // `serialize_to_json_err::<T>`. The lone inline non-list-rewriter
    // gate in `rewrite_typed::<T>` funnels through
    // `rewriter_non_list_err::<T>`. These tests pin: (a) the
    // serialize helper produces the structural
    // `LispError::DomainSerialize { keyword: T::KEYWORD, message }`
    // variant — fail-before-pass-after: pre-lift this assertion
    // matched on `LispError::Compile { form, message }` with
    // `form = T::KEYWORD.to_string()`; post-lift the variant identity
    // IS the diagnostic, no substring parse required;
    // (b) the non-list-rewriter helper produces the structural
    // `LispError::RewriterNonList { keyword, got }` variant with
    // `keyword = T::KEYWORD`;
    // (c) Display renders the canonical
    // `"compile error in <keyword>: serialize: …"` / `"compile error
    // in <keyword>: rewriter must return a list; got …"` shape
    // byte-for-byte across the lift so substring-grep consumers see no
    // drift; (d) end-to-end through `rewrite_typed` — a rewriter
    // returning a non-list `Sexp` routes through the helper with the
    // right shape.
    //
    // The redundant-keyword `"serialize {KEYWORD}: …"` shape that
    // `rewrite_typed` used pre-lift is dropped; both sites now render
    // the cleaner `"serialize: …"` shape. The test pins the new
    // canonical form so a regression that re-inlines the old shape
    // fails loudly.

    fn make_serde_err() -> serde_json::Error {
        // Hand-craft a `serde_json::Error` via a known-failing parse so
        // the test exercises the helper's `{e}` Display projection
        // without needing a `Serialize` impl that panics on a real T.
        serde_json::from_str::<i32>("not-a-number").unwrap_err()
    }

    #[test]
    fn serialize_to_json_err_produces_structural_domain_serialize_variant() {
        // Post-lift the helper emits the structural
        // `LispError::DomainSerialize { keyword, message }` variant,
        // not the `Compile`-shaped triple it used to. Fail-before-
        // pass-after: pre-lift this same input emitted
        // `LispError::Compile { form: "defmonitor", message:
        // "serialize: <e>" }` and authoring tools had to substring-
        // grep the rendered diagnostic to recognize this specific
        // gate; post-lift the gate IS its variant identity. `keyword`
        // carries `T::KEYWORD` verbatim (compile-time guarantee load-
        // bearing in the type system); `message` carries the
        // `serde_json::Error::Display` projection unchanged — no
        // `"serialize: "` prefix in the field, the prefix is in
        // `LispError::Display` so consumers binding on the field get
        // the raw underlying message.
        let e = make_serde_err();
        let raw = format!("{e}");
        let err = serialize_to_json_err::<MonitorSpec>(e);
        match err {
            LispError::DomainSerialize { keyword, message } => {
                assert_eq!(keyword, "defmonitor", "keyword must be T::KEYWORD verbatim");
                assert_eq!(
                    message, raw,
                    "message must be the serde_json::Error::Display projection verbatim",
                );
            }
            other => panic!("expected LispError::DomainSerialize, got {other:?}"),
        }
    }

    #[test]
    fn serialize_to_json_err_display_renders_canonical_string() {
        // The Display impl renders `"compile error in <keyword>:
        // serialize: <e>"` — `tatara-check` / REPL / future LSP that
        // substring-grep this shape see no drift across the structural
        // lift, and the redundant keyword repetition (`"serialize
        // defmonitor: …"`) that `rewrite_typed` used pre-canonicalize
        // is gone.
        let e = make_serde_err();
        let raw = format!("{e}");
        let err = serialize_to_json_err::<MonitorSpec>(e);
        let rendered = format!("{err}");
        assert_eq!(
            rendered,
            format!("compile error in defmonitor: serialize: {raw}"),
        );
        // Negative: the pre-canonicalize `"serialize defmonitor: …"`
        // redundant-keyword shape must NOT appear in the new render.
        assert!(
            !rendered.contains("serialize defmonitor:"),
            "redundant-keyword shape must be gone, got: {rendered}"
        );
    }

    #[test]
    fn rewriter_non_list_err_produces_structural_variant() {
        // Post-lift the helper emits the structural
        // `LispError::RewriterNonList { keyword, got: SexpWitness }`
        // variant. `got` is the typed joint identity (`SexpShape` +
        // `Sexp::Display`) — the EIGHTH consumer of the `SexpWitness`
        // primitive, and the FIRST on the typed-EXIT boundary. Tools
        // pattern-match on `got.shape` (structurally) AND read
        // `got.display` (literal) jointly. A regression that re-
        // collapses `got` to a free-form `String` fails-loudly here.
        let got = Sexp::int(42);
        let err = rewriter_non_list_err::<MonitorSpec>(&got);
        match err {
            LispError::RewriterNonList { keyword, got } => {
                assert_eq!(keyword, "defmonitor", "keyword must be T::KEYWORD verbatim");
                assert_eq!(
                    (got.shape, got.display.as_str()),
                    (SexpShape::Int, "42"),
                    "got must carry the typed (SexpShape, Sexp::Display) joint identity",
                );
            }
            other => panic!("expected LispError::RewriterNonList, got {other:?}"),
        }
    }

    #[test]
    fn rewriter_non_list_err_display_renders_canonical_string() {
        // The legacy `"rewriter must return a list; got …"` substring
        // shape is preserved byte-for-byte so authoring-tool grep over
        // the rendered diagnostic sees no drift across the lift.
        let got = Sexp::symbol("not-a-list");
        let err = rewriter_non_list_err::<MonitorSpec>(&got);
        assert_eq!(
            format!("{err}"),
            "compile error in defmonitor: rewriter must return a list; got not-a-list",
        );
    }

    #[test]
    fn rewriter_non_list_err_includes_got_sexp_display() {
        // The `got` payload is projected via the `Sexp` Display impl —
        // pinning a few representative variants keeps the diagnostic's
        // failing-value-naming surface stable across versions. Lists
        // never reach this gate (they short-circuit into the
        // `Sexp::List(items) => items` arm of `rewrite_typed`), but the
        // helper is shape-of-arm — it accepts any non-list `Sexp` the
        // caller hands it. Render strings track the `Sexp::Display`
        // contract verbatim (`Sexp::Nil` → `"()"`, not `"nil"`).
        let cases: &[(Sexp, &str)] = &[
            (Sexp::int(7), "7"),
            (Sexp::string("hi"), "\"hi\""),
            (Sexp::symbol("foo"), "foo"),
            (Sexp::keyword("k"), ":k"),
            (Sexp::Nil, "()"),
        ];
        for (sexp, want_render) in cases {
            let err = rewriter_non_list_err::<MonitorSpec>(sexp);
            let got = match err {
                LispError::RewriterNonList { got, .. } => got,
                other => panic!("expected LispError::RewriterNonList, got {other:?}"),
            };
            assert_eq!(
                got.display, *want_render,
                "Sexp Display projection must thread through unchanged for {sexp:?}"
            );
        }
    }

    #[test]
    fn rewrite_typed_routes_non_list_output_through_helper_e2e() {
        // End-to-end through `rewrite_typed::<MonitorSpec>`: a
        // rewriter returning a non-list `Sexp` (here, an int) MUST
        // route through `rewriter_non_list_err::<MonitorSpec>` and
        // emit a `LispError::RewriterNonList { keyword: "defmonitor",
        // got: "42" }`. Fail-before-pass-after: pre-lift this path
        // emitted `LispError::Compile { ... }` and a regression that
        // re-inlines the shape (or drifts the keyword/got) fails
        // loudly here.
        let input = MonitorSpec {
            name: "x".into(),
            query: "q".into(),
            threshold: 0.5,
            window_seconds: None,
            tags: vec![],
            enabled: None,
        };
        let err = rewrite_typed(input, |_sexp| Ok(Sexp::int(42))).unwrap_err();
        match err {
            LispError::RewriterNonList { keyword, got } => {
                assert_eq!(keyword, "defmonitor");
                assert_eq!((got.shape, got.display.as_str()), (SexpShape::Int, "42"));
            }
            other => panic!("expected LispError::RewriterNonList, got {other:?}"),
        }
    }

    #[test]
    fn rewrite_typed_routes_non_list_output_for_every_non_list_variant() {
        // The non-list gate covers EVERY non-list `Sexp` shape — pin
        // a representative sample (atom, quote, unquote-splice)
        // through the gate to confirm the helper is shape-of-arm,
        // not shape-of-some-variants. `Sexp::Nil` renders as `()` per
        // the `Sexp::Display` contract.
        let input = MonitorSpec {
            name: "x".into(),
            query: "q".into(),
            threshold: 0.5,
            window_seconds: None,
            tags: vec![],
            enabled: None,
        };
        let non_lists = [
            Sexp::int(0),
            Sexp::string("bad"),
            Sexp::symbol("not-a-list"),
            Sexp::Nil,
            Sexp::Quote(Box::new(Sexp::Nil)),
            Sexp::UnquoteSplice(Box::new(Sexp::Nil)),
        ];
        for bad in non_lists {
            // Each iteration consumes input by cloning the prelude
            // (rewrite_typed takes input by value).
            let clone = MonitorSpec {
                name: input.name.clone(),
                query: input.query.clone(),
                threshold: input.threshold,
                window_seconds: input.window_seconds,
                tags: input.tags.clone(),
                enabled: input.enabled,
            };
            let bad_disp = format!("{bad}");
            let bad_shape = sexp_shape(&bad);
            let err = rewrite_typed(clone, |_sexp| Ok(bad.clone())).unwrap_err();
            match err {
                LispError::RewriterNonList { keyword, got } => {
                    assert_eq!(keyword, "defmonitor");
                    assert_eq!(got.display, bad_disp);
                    assert_eq!(
                        got.shape, bad_shape,
                        "typed SexpShape must thread through for {bad:?}",
                    );
                }
                other => panic!("expected LispError::RewriterNonList, got {other:?}"),
            }
        }
    }

    #[test]
    fn rewrite_typed_well_formed_list_routes_past_non_list_gate() {
        // Positive control — a well-formed list `Sexp` returned by the
        // rewriter routes PAST `rewriter_non_list_err::<T>` cleanly
        // into `T::compile_from_args`. The helper is precisely scoped
        // to non-list `Sexp` outputs; identity-rewriting through the
        // gate preserves the typed value end-to-end. Uses a local
        // single-field domain so the round-trip needs no
        // `#[serde(rename_all)]` plumbing — the production-side
        // round-trip case is covered by
        // `tatara_domains::rewrite_typed_end_to_end`.
        #[derive(DeriveTataraDomain, Serialize, Deserialize, Debug)]
        #[tatara(keyword = "defroundtrip")]
        struct RoundTripSpec {
            name: String,
        }
        let input = RoundTripSpec { name: "x".into() };
        let out = rewrite_typed(input, |sexp| {
            assert!(
                sexp.is_list(),
                "rewriter receives a `Sexp::List` of alternating kwargs"
            );
            Ok(sexp)
        })
        .expect("identity-rewrite of a well-formed typed value must round-trip");
        assert_eq!(out.name, "x");
    }

    #[test]
    fn helpers_are_type_bound_via_t_keyword() {
        // Type-bound symmetry: both helpers project `T::KEYWORD` at the
        // type level — `<T: TataraDomain>` is the boundary, so a typo
        // can never drift the `form` slot across the two call sites in
        // `register::<T>` + `rewrite_typed::<T>`. Pin the projection by
        // exercising the helpers against TWO domains in this module
        // (`MonitorSpec` — defmonitor — and a local domain with a
        // different keyword) and confirm each helper emits the
        // domain's KEYWORD verbatim.
        #[derive(DeriveTataraDomain, Serialize, Debug)]
        #[tatara(keyword = "deflocal")]
        struct LocalSpec {
            name: String,
        }
        // Reference the field so clippy `dead_code` doesn't trip.
        let _local = LocalSpec {
            name: "z".to_string(),
        };
        let e1 = make_serde_err();
        let m_err = serialize_to_json_err::<MonitorSpec>(e1);
        let e2 = make_serde_err();
        let l_err = serialize_to_json_err::<LocalSpec>(e2);
        match m_err {
            LispError::DomainSerialize { keyword, .. } => assert_eq!(keyword, "defmonitor"),
            other => panic!("expected LispError::DomainSerialize, got {other:?}"),
        }
        match l_err {
            LispError::DomainSerialize { keyword, .. } => assert_eq!(keyword, "deflocal"),
            other => panic!("expected LispError::DomainSerialize, got {other:?}"),
        }
        let got = Sexp::int(0);
        match rewriter_non_list_err::<MonitorSpec>(&got) {
            LispError::RewriterNonList { keyword, .. } => assert_eq!(keyword, "defmonitor"),
            other => panic!("expected LispError::RewriterNonList, got {other:?}"),
        }
        match rewriter_non_list_err::<LocalSpec>(&got) {
            LispError::RewriterNonList { keyword, .. } => assert_eq!(keyword, "deflocal"),
            other => panic!("expected LispError::RewriterNonList, got {other:?}"),
        }
    }

    // ── extract_atom / extract_optional_atom: typed-atom dedup lift ───
    //
    // The eight inline `extract_X` / `extract_optional_X` shapes
    // (`extract_string`, `extract_int`, `extract_float`, `extract_bool`
    // + their optional siblings) all funneled through one of two
    // byte-identical inline `required + project + type_err` triples
    // (required path) or `kw.get + project + type_err` quadruples
    // (optional path). The lift collapses each four-site cluster to
    // ONE named generic primitive (`extract_atom`, `extract_optional_atom`)
    // parameterized by the typed-name label + projection function.
    //
    // The tests below pin: (a) each generic helper's failure-routing
    // surface — missing-required → `MissingKwarg`, present-but-
    // wrong-type → `TypeMismatch` (required path); absent → `Ok(None)`,
    // present-and-correct → `Ok(Some)`, present-but-wrong-type →
    // `TypeMismatch` (optional path); (b) every public delegate
    // (`extract_string`, `extract_int`, `extract_float`, `extract_bool`
    // + optional siblings) routes through the generic helper with the
    // canonical typed-name label intact; (c) Display byte-identity is
    // preserved across the dedup — a regression that drifts the
    // typed-name label (e.g. lowercases `"number"` → `"float"`) fails-
    // loudly at the Display assertion; (d) the borrowed-return path
    // (`extract_string` returns `&'a str` from `&'a Sexp`) round-trips
    // its lifetime through `FnOnce(&'a Sexp) -> Option<&'a str>`
    // cleanly — a regression that breaks the borrow threading fails-
    // to-compile.

    #[test]
    fn extract_atom_propagates_missing_kwarg_via_required() {
        // The required path's first gate — absent kwarg routes through
        // `required` which emits `LispError::MissingKwarg { key }`. Pin
        // the canonical `MissingKwarg` shape and key verbatim; a
        // regression that swallows the gate (e.g. silent `Ok(default)`)
        // or drifts the key slot fails-loudly here. Distinct from
        // `extract_atom_emits_type_mismatch_for_wrong_type` — that
        // pins the second gate.
        let kw: Kwargs<'_> = HashMap::new();
        let err = extract_atom(&kw, "missing", ExpectedKwargShape::Int, Sexp::as_int)
            .expect_err("absent required kwarg must error");
        match err {
            LispError::MissingKwarg { key } => assert_eq!(key, "missing"),
            other => panic!("expected MissingKwarg, got {other:?}"),
        }
    }

    #[test]
    fn extract_atom_emits_type_mismatch_for_wrong_type() {
        // The required path's second gate — present-but-wrong-type
        // kwarg routes through `type_err` which emits
        // `LispError::TypeMismatch { form, expected, got }`. Pin all
        // three slots: `form` is `kwarg_form(key)` (`:wrongkey`),
        // `expected` is the typed-name label fed in verbatim
        // (`"int"`), `got` is `Sexp::Display`'s projection of the
        // offending atom's type (`"string"`). A regression that
        // drifts the typed-name label fails-loudly here.
        let string_sexp = Sexp::string("not-an-int");
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("wrongkey".to_string(), &string_sexp);
        let err = extract_atom(&kw, "wrongkey", ExpectedKwargShape::Int, Sexp::as_int)
            .expect_err("present-but-wrong-type kwarg must error");
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(form, crate::error::KwargPath::Named("wrongkey".into()));
                assert_eq!(expected, ExpectedKwargShape::Int);
                assert_eq!(got, SexpShape::String);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_atom_returns_value_on_match() {
        // Positive control for `extract_atom` — present and correctly-
        // typed kwarg returns the projected value. Distinct from the
        // two negative paths above; closes the closed set of three
        // outcomes (missing, wrong-type, ok) for the required path.
        let int_sexp = Sexp::int(42);
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("count".to_string(), &int_sexp);
        let v = extract_atom(&kw, "count", ExpectedKwargShape::Int, Sexp::as_int)
            .expect("present-and-correct kwarg must succeed");
        assert_eq!(v, 42);
    }

    #[test]
    fn extract_optional_atom_returns_none_for_absent_kwarg() {
        // The optional path's first arm — absent kwarg returns
        // `Ok(None)`, NOT an error. Pin the structural distinction
        // from the required path (which errors on absent) by
        // exercising the same key against both paths; the optional
        // sibling must NEVER call `required` and must NEVER emit
        // `MissingKwarg`. A regression that mistakenly routes the
        // absent arm through `required` would surface here as an
        // `Err(MissingKwarg)` instead of `Ok(None)`.
        let kw: Kwargs<'_> = HashMap::new();
        let v =
            extract_optional_atom::<i64, _>(&kw, "absent", ExpectedKwargShape::Int, Sexp::as_int)
                .expect("absent optional kwarg must succeed with None");
        assert!(v.is_none());
    }

    #[test]
    fn extract_optional_atom_emits_type_mismatch_for_wrong_type() {
        // The optional path's second arm — present-but-wrong-type
        // kwarg errors via `type_err` with the same `TypeMismatch`
        // shape as the required path. Distinct from `extract_atom
        // _emits_type_mismatch_for_wrong_type` only in which kwarg
        // path emitted the error — same variant, same slot
        // semantics. Pins that the optional path does NOT silently
        // swallow type mismatches by returning `Ok(None)` for a
        // present-but-wrong-type kwarg — that would be a typed-entry
        // gate failure.
        let string_sexp = Sexp::string("not-a-bool");
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("flag".to_string(), &string_sexp);
        let err =
            extract_optional_atom::<bool, _>(&kw, "flag", ExpectedKwargShape::Bool, Sexp::as_bool)
                .expect_err("present-but-wrong-type optional kwarg must error");
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(form, crate::error::KwargPath::Named("flag".into()));
                assert_eq!(expected, ExpectedKwargShape::Bool);
                assert_eq!(got, SexpShape::String);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_optional_atom_returns_some_on_match() {
        // The optional path's third arm — present and correctly-
        // typed kwarg returns `Ok(Some(value))`. Closes the closed
        // set of three outcomes (absent, wrong-type, ok) for the
        // optional path; together with the required-path tests,
        // every distinct extractor outcome is covered.
        let float_sexp = Sexp::float(3.5);
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("ratio".to_string(), &float_sexp);
        let v = extract_optional_atom(&kw, "ratio", ExpectedKwargShape::Number, Sexp::as_float)
            .expect("present-and-correct optional kwarg must succeed");
        assert_eq!(v, Some(3.5));
    }

    /// [`extract_optional_atom`] is now a one-line delegate to
    /// `optional_from_required(kw, key, |kw, key| extract_atom(kw, key,
    /// expected, project))` — the SEVENTH consumer of the
    /// [`optional_from_required`] present-vs-absent substrate primitive
    /// (after the four list-family peers plus
    /// [`extract_optional_via_serde`] on the universal-serde axis and
    /// [`extract_optional_narrowed`] on the numeric-narrowed axis).
    /// Post-lift the primitive's `F: FnOnce(&'a Kwargs<'a>, &str) ->
    /// Result<T>` bound rides an explicit `'a` lifetime so `T` may
    /// borrow from `kw` — the closure-wrapped [`extract_atom`] extractor
    /// on the `<&'a str as AtomKwarg<'a>>` string axis threads its
    /// borrowed slot through the primitive without an intermediate
    /// copy or an axis-only inline duplication.
    ///
    /// Pin the delegation-identity contract at the operator-visible
    /// level: for every input a caller-shaped [`extract_optional_atom`]
    /// accepts or rejects, the hand-composed
    /// `optional_from_required(kw, key, |kw, key| extract_atom(kw, key,
    /// expected, project))` must produce the byte-identical verdict —
    /// same `Ok(Some(_))` / `Ok(None)` / `Err(_)` shape, same
    /// [`LispError`] variant on rejection, same axis-typed
    /// [`ExpectedKwargShape`] payload on shape mismatch, same
    /// [`KwargPath::Named`] form on the failing kwarg. Sweep the pair
    /// across the THREE canonical verdicts (absent, present-wrong-shape,
    /// present-correct-shape) × BOTH ownership axes (borrowed `<&'a
    /// str>` on the string axis, owned `<bool>` on the bool axis) to
    /// lock the delegation shape across both the atom-family
    /// borrowed-T and owned-T cases the primitive's `'a`-lifted `F`
    /// bound admits.
    ///
    /// A regression that swapped the extractor's body back to the
    /// pre-lift inline `match optional(kw, key) { None => Ok(None);
    /// Some(v) => project(v).map(Some).ok_or_else(|| type_err(...)) }`
    /// composition (byte-equivalent today but bypassing
    /// [`optional_from_required`] — a diagnostic-promotion divergence
    /// at the substrate primitive layer) would still pass THIS test on
    /// present inputs (both paths produce the same diagnostic bytes on
    /// the shape mismatch); the load-bearing proof this test carries
    /// is the FORWARD compatibility of the delegation across a
    /// `optional_from_required` diagnostic promotion (a probe, a
    /// metric, a span on the present-vs-absent gate — every future
    /// promotion at that primitive flows to the atom-family scalar
    /// peer here through this delegate, sight-unseen by every caller).
    ///
    /// Peer to
    /// [`extract_optional_narrowed_delegates_through_optional_from_required_across_the_four_verdicts`]
    /// on the numeric-narrowed axis — that test pins the scalar-
    /// narrowed peer's delegation through the same primitive at the
    /// numeric-width axis level; this test pins the atom-family
    /// scalar peer's delegation at the ownership axis level (borrowed
    /// vs. owned `T`), closing the family loop on the substrate
    /// primitive every extract_optional_ consumer now binds through
    /// with no inline exceptions.
    #[test]
    fn extract_optional_atom_delegates_through_optional_from_required_across_borrowed_and_owned_axes(
    ) {
        // (1) ABSENT, string (borrowed-T) axis — both paths short-
        //     circuit to `Ok(None)` without invoking the required
        //     extractor's shape gate.
        let absent_args = kwargs_of("(_ :other 1)");
        let absent_kw = parse_kwargs(&absent_args).unwrap();
        assert_eq!(
            extract_optional_atom::<&str, _>(
                &absent_kw,
                "missing",
                ExpectedKwargShape::String,
                Sexp::as_string,
            )
            .unwrap(),
            optional_from_required(&absent_kw, "missing", |kw, key| extract_atom(
                kw,
                key,
                ExpectedKwargShape::String,
                Sexp::as_string,
            ))
            .unwrap(),
        );
        assert_eq!(
            extract_optional_atom::<&str, _>(
                &absent_kw,
                "missing",
                ExpectedKwargShape::String,
                Sexp::as_string,
            )
            .unwrap(),
            None,
        );

        // (1) ABSENT, bool (owned-T) axis — peer identity on the peer
        //     axis; the primitive's `'a`-lifted `F` bound admits BOTH
        //     the borrowed and owned atom-family projections through
        //     ONE signature.
        assert_eq!(
            extract_optional_atom::<bool, _>(
                &absent_kw,
                "missing",
                ExpectedKwargShape::Bool,
                Sexp::as_bool,
            )
            .unwrap(),
            optional_from_required(&absent_kw, "missing", |kw, key| extract_atom(
                kw,
                key,
                ExpectedKwargShape::Bool,
                Sexp::as_bool,
            ))
            .unwrap(),
        );

        // (2) PRESENT, correct shape, string axis — both paths return
        //     `Ok(Some(&'a str))` wrapping the same borrowed slot;
        //     equality on `&str` proves the delegate's lifetime
        //     threading matches the pre-lift inline shape.
        let str_ok_args = kwargs_of(r#"(_ :name "prom-up")"#);
        let str_ok_kw = parse_kwargs(&str_ok_args).unwrap();
        assert_eq!(
            extract_optional_atom::<&str, _>(
                &str_ok_kw,
                "name",
                ExpectedKwargShape::String,
                Sexp::as_string,
            )
            .unwrap(),
            optional_from_required(&str_ok_kw, "name", |kw, key| extract_atom(
                kw,
                key,
                ExpectedKwargShape::String,
                Sexp::as_string,
            ))
            .unwrap(),
        );
        assert_eq!(
            extract_optional_atom::<&str, _>(
                &str_ok_kw,
                "name",
                ExpectedKwargShape::String,
                Sexp::as_string,
            )
            .unwrap(),
            Some("prom-up"),
        );

        // (2) PRESENT, correct shape, bool axis — peer identity on
        //     the owned-T axis; both paths return `Ok(Some(true))`.
        let bool_ok_args = kwargs_of("(_ :enabled #t)");
        let bool_ok_kw = parse_kwargs(&bool_ok_args).unwrap();
        assert_eq!(
            extract_optional_atom::<bool, _>(
                &bool_ok_kw,
                "enabled",
                ExpectedKwargShape::Bool,
                Sexp::as_bool,
            )
            .unwrap(),
            optional_from_required(&bool_ok_kw, "enabled", |kw, key| extract_atom(
                kw,
                key,
                ExpectedKwargShape::Bool,
                Sexp::as_bool,
            ))
            .unwrap(),
        );
        assert_eq!(
            extract_optional_atom::<bool, _>(
                &bool_ok_kw,
                "enabled",
                ExpectedKwargShape::Bool,
                Sexp::as_bool,
            )
            .unwrap(),
            Some(true),
        );

        // (3) PRESENT, wrong shape, string axis — both paths surface
        //     the SAME `LispError::TypeMismatch` variant with the SAME
        //     axis-typed `ExpectedKwargShape::String` label, the SAME
        //     `KwargPath::Named("name")` form, and the SAME
        //     `SexpShape::Int` got-payload.
        let str_shape_args = kwargs_of("(_ :name 42)");
        let str_shape_kw = parse_kwargs(&str_shape_args).unwrap();
        let via_wrapper = extract_optional_atom::<&str, _>(
            &str_shape_kw,
            "name",
            ExpectedKwargShape::String,
            Sexp::as_string,
        )
        .expect_err("int is not a string");
        let via_primitive = optional_from_required(&str_shape_kw, "name", |kw, key| {
            extract_atom(kw, key, ExpectedKwargShape::String, Sexp::as_string)
        })
        .expect_err("int is not a string");
        match (&via_wrapper, &via_primitive) {
            (
                LispError::TypeMismatch {
                    form: f1,
                    expected: e1,
                    got: g1,
                },
                LispError::TypeMismatch {
                    form: f2,
                    expected: e2,
                    got: g2,
                },
            ) => {
                assert_eq!(f1, f2);
                assert_eq!(e1, e2);
                assert_eq!(g1, g2);
                assert_eq!(*e1, ExpectedKwargShape::String);
                assert_eq!(*g1, SexpShape::Int);
                assert_eq!(f1, &KwargPath::named("name"));
            }
            _ => panic!("both must be TypeMismatch, got {via_wrapper:?} vs {via_primitive:?}"),
        }
        // Rendered diagnostic parity — a `type_err`-shape divergence at
        // the primitive layer that preserved variant identity but
        // drifted the rendered path would surface here.
        assert_eq!(
            type_err_message(via_wrapper),
            type_err_message(via_primitive),
        );

        // (3) PRESENT, wrong shape, bool axis — peer identity on the
        //     owned-T axis; `ExpectedKwargShape::Bool` label and
        //     `SexpShape::String` got-payload.
        let bool_shape_args = kwargs_of(r#"(_ :enabled "yes")"#);
        let bool_shape_kw = parse_kwargs(&bool_shape_args).unwrap();
        let via_wrapper = extract_optional_atom::<bool, _>(
            &bool_shape_kw,
            "enabled",
            ExpectedKwargShape::Bool,
            Sexp::as_bool,
        )
        .expect_err("string is not a bool");
        let via_primitive = optional_from_required(&bool_shape_kw, "enabled", |kw, key| {
            extract_atom(kw, key, ExpectedKwargShape::Bool, Sexp::as_bool)
        })
        .expect_err("string is not a bool");
        match (&via_wrapper, &via_primitive) {
            (
                LispError::TypeMismatch {
                    form: f1,
                    expected: e1,
                    got: g1,
                },
                LispError::TypeMismatch {
                    form: f2,
                    expected: e2,
                    got: g2,
                },
            ) => {
                assert_eq!(f1, f2);
                assert_eq!(e1, e2);
                assert_eq!(g1, g2);
                assert_eq!(*e1, ExpectedKwargShape::Bool);
                assert_eq!(*g1, SexpShape::String);
                assert_eq!(f1, &KwargPath::named("enabled"));
            }
            _ => panic!("both must be TypeMismatch, got {via_wrapper:?} vs {via_primitive:?}"),
        }
        assert_eq!(
            type_err_message(via_wrapper),
            type_err_message(via_primitive),
        );
    }

    #[test]
    fn extract_string_borrows_lifetime_through_extract_atom() {
        // The borrowed-return path — `extract_string` returns `&'a str`
        // borrowed from the kwarg `&'a Sexp`. Pins that the lift's
        // `FnOnce(&'a Sexp) -> Option<&'a str>` boundary threads the
        // lifetime correctly: a regression that breaks the
        // higher-ranked lifetime would fail-to-compile (not a runtime
        // assertion). The runtime assertion below pins that the
        // returned `&str` round-trips the kwarg's literal content.
        let s_sexp = Sexp::string("prom-up");
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("name".to_string(), &s_sexp);
        let got = extract_string(&kw, "name").expect("present string must succeed");
        assert_eq!(got, "prom-up");
    }

    #[test]
    fn public_extract_delegates_inherit_canonical_type_labels() {
        // Path-uniformity across all four public typed-name labels —
        // `extract_int` (`Int`), `extract_float` (`Number`),
        // `extract_bool` (`Bool`), `extract_string` (`String`). Each
        // delegate must route through `extract_atom` with the
        // canonical typed `ExpectedKwargShape` variant intact; a
        // regression that drifts a label (e.g. `extract_float`'s
        // `Number` → `Int`, or `extract_int`'s `Int` → `Number`)
        // would surface as a `TypeMismatch.expected` variant-identity
        // drift when the extractor is fed a wrong-typed kwarg. After
        // the closed-set lift the typed-enum check is a rustc-enforced
        // contract — a typo in any label literal is unreachable
        // because the variants are the literals.
        let s = Sexp::string("not-typed");
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("x".to_string(), &s);
        for (extractor_name, expected_shape, err) in [
            (
                "extract_int",
                ExpectedKwargShape::Int,
                extract_int(&kw, "x").expect_err("must error"),
            ),
            (
                "extract_float",
                ExpectedKwargShape::Number,
                extract_float(&kw, "x").expect_err("must error"),
            ),
            (
                "extract_bool",
                ExpectedKwargShape::Bool,
                extract_bool(&kw, "x").expect_err("must error"),
            ),
        ] {
            match err {
                LispError::TypeMismatch { expected, .. } => assert_eq!(
                    expected, expected_shape,
                    "{extractor_name} must thread the canonical shape {expected_shape:?}",
                ),
                other => panic!("{extractor_name}: expected TypeMismatch, got {other:?}"),
            }
        }
        // `extract_string` against a non-string keyword sexp: same
        // shape, different label. Pinned separately because the
        // string extractor's signature carries a borrow lifetime that
        // doesn't match the tuple shape of the loop above.
        let kw_sexp = Sexp::keyword("not-a-string");
        let mut kw2: Kwargs<'_> = HashMap::new();
        kw2.insert("x".to_string(), &kw_sexp);
        let err = extract_string(&kw2, "x").expect_err("must error");
        match err {
            LispError::TypeMismatch { expected, .. } => {
                assert_eq!(expected, ExpectedKwargShape::String);
            }
            other => panic!("extract_string: expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_atom_renders_legacy_type_mismatch_display() {
        // End-to-end through the `LispError` Display impl — pins that
        // the dedup preserves the legacy `TypeMismatch`-shaped
        // diagnostic byte-for-byte. Authoring tools (`tatara-check`,
        // REPL) that substring-grep on the rendered diagnostic see
        // no drift across the lift. Parallel to how
        // `compile_named_named_form_missing_name_renders_legacy
        // _compile_shape` (compile.rs) pins the lifted helper's
        // Display contract.
        let s = Sexp::string("not-an-int");
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("threshold".to_string(), &s);
        let err = extract_int(&kw, "threshold").expect_err("type-mismatch must error");
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: expected int, got string"
        );
    }

    #[test]
    fn full_monitor_round_trips_through_extract_atom_dedup() {
        // End-to-end positive control: a well-formed defmonitor
        // exercises every typed-atom extractor (`extract_string` on
        // `:name`/`:query`, `extract_float` on `:threshold`,
        // `extract_optional_int` on `:window-seconds`,
        // `extract_optional_bool` on `:enabled`). Pins that the
        // dedup doesn't regress any of the public delegates'
        // semantic — a `MonitorSpec` compiled before and after the
        // lift must produce byte-identical values. Same posture as
        // `derive_compiles_full_form` (the pre-existing positive
        // control); duplicated here to lock the helper-routing
        // invariant.
        let forms = read(
            r#"(defmonitor
                 :name "prom-up"
                 :query "up{job='prometheus'}"
                 :threshold 0.99
                 :window-seconds 300
                 :tags ("prod" "observability")
                 :enabled #t)"#,
        )
        .unwrap();
        let spec = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(spec.name, "prom-up");
        assert_eq!(spec.threshold, 0.99);
        assert_eq!(spec.window_seconds, Some(300));
        assert_eq!(spec.enabled, Some(true));
    }

    // ── extract_list: list-typed-kwarg dedup lift ────────────────────
    //
    // `extract_string_list` (each item via `as_string` + `type_err_at`)
    // and `extract_vec_via_serde` (each item via `from_value_with_path`
    // carrying `KwargPath::item`) used to inline the SAME list-extractor
    // skeleton — absent → empty vec, present-but-not-a-list → `type_err`,
    // `iter().enumerate().map(per-item).collect()`. The lift collapses
    // both to ONE generic primitive (`extract_list`) parameterized by the
    // outer-shape label + the per-element projection, the list-family
    // sibling of `extract_atom` / `extract_optional_atom`.
    //
    // The tests below pin the three fixed decisions the skeleton owns:
    // (a) absent kwarg short-circuits to `Ok(Vec::new())` BEFORE any
    // per-item work (the projection is a `panic!` proving it never runs);
    // (b) present-but-not-a-list routes through `type_err` with the
    // CALLER-supplied `list_shape` (tested with `ListOfStrings`, NOT the
    // skeleton-baked `List`, so a regression hardcoding the shape fails
    // loudly) and the per-item projection again never runs; (c) the
    // per-element walk threads the 0-based `enumerate` index into the
    // projection in order; (d) a per-item rejection short-circuits the
    // collect at the FIRST failing element with that element's index in
    // the `KwargPath::Item` slot. The existing `extract_string_list` /
    // `extract_vec_via_serde` suites are the path-uniformity guards
    // proving both public extractors route through it with zero drift.

    #[test]
    fn extract_list_returns_empty_vec_for_absent_kwarg() {
        // Absent list kwarg is the empty list, never an error — same
        // posture `extract_optional_atom` takes for absent atoms. The
        // `panic!` projection proves the absent arm short-circuits
        // BEFORE any per-item work: a regression that walked a (missing)
        // list before the absent check would fire the panic.
        let kw: Kwargs<'_> = HashMap::new();
        let out: Vec<i64> = extract_list(&kw, "absent", ExpectedKwargShape::List, |_, _| {
            panic!("per-item projection must not run for an absent list kwarg")
        })
        .expect("absent list kwarg must succeed with an empty vec");
        assert!(out.is_empty());
    }

    #[test]
    fn extract_list_emits_type_err_with_caller_supplied_list_shape() {
        // Present-but-not-a-list routes through the outer-shape gate with
        // the CALLER's `list_shape` (`ListOfStrings`), not a skeleton-baked
        // `List` — a regression hardcoding the shape fails here. The
        // `panic!` projection also proves the per-item walk never starts
        // when the outer gate rejects.
        let scalar = Sexp::int(5);
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("tags".to_string(), &scalar);
        let err =
            extract_list::<String, _>(&kw, "tags", ExpectedKwargShape::ListOfStrings, |_, _| {
                panic!("per-item projection must not run when the outer shape gate fails")
            })
            .expect_err("present-but-not-a-list kwarg must error");
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(form, crate::error::KwargPath::Named("tags".into()));
                assert_eq!(expected, ExpectedKwargShape::ListOfStrings);
                assert_eq!(got, SexpShape::Int);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn extract_list_threads_enumerate_index_into_projection_in_order() {
        // The per-element walk threads the 0-based `enumerate` index into
        // the projection and collects results in order. Pin both the index
        // sequence (0, 1, 2) and the element order so a regression that
        // dropped `.enumerate()` or reordered the walk fails loudly.
        let items = Sexp::List(vec![
            Sexp::string("a"),
            Sexp::string("b"),
            Sexp::string("c"),
        ]);
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("xs".to_string(), &items);
        let out: Vec<(usize, String)> =
            extract_list(&kw, "xs", ExpectedKwargShape::List, |idx, e| {
                Ok((
                    idx,
                    e.as_string().expect("test items are strings").to_string(),
                ))
            })
            .expect("well-formed list must collect");
        assert_eq!(
            out,
            vec![
                (0, "a".to_string()),
                (1, "b".to_string()),
                (2, "c".to_string()),
            ]
        );
    }

    #[test]
    fn extract_list_short_circuits_at_first_failing_item_with_its_index() {
        // A per-item rejection short-circuits the collect at the FIRST
        // failing element, carrying that element's index in the
        // `KwargPath::Item` slot. The third element (`"never"`) is a valid
        // string but must never be reached — index 1 (the int) fails first.
        let items = Sexp::List(vec![
            Sexp::string("ok"),
            Sexp::int(9),
            Sexp::string("never"),
        ]);
        let mut kw: Kwargs<'_> = HashMap::new();
        kw.insert("xs".to_string(), &items);
        let err =
            extract_list::<String, _>(&kw, "xs", ExpectedKwargShape::ListOfStrings, |idx, e| {
                e.as_string()
                    .map(String::from)
                    .ok_or_else(|| type_err_at("xs", idx, ExpectedKwargShape::String, e))
            })
            .expect_err("a non-string item must error");
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(
                    form,
                    crate::error::KwargPath::Item {
                        key: "xs".into(),
                        idx: 1,
                    }
                );
                assert_eq!(expected, ExpectedKwargShape::String);
                assert_eq!(got, SexpShape::Int);
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }
}
