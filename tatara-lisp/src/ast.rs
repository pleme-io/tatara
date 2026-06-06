//! S-expression AST.

use crate::error::UnquoteForm;
use std::fmt;
use std::hash::{Hash, Hasher};

// `Sexp` is `PartialEq` but not `Eq` (Float contains NaN). We implement Hash
// manually so cache keys can hash a borrowed `&[Sexp]` directly — avoids the
// serde_json serialization that would otherwise dominate cache overhead on
// cheap macro calls.
impl Hash for Sexp {
    fn hash<H: Hasher>(&self, h: &mut H) {
        match self {
            Self::Nil => 0u8.hash(h),
            Self::Atom(a) => {
                1u8.hash(h);
                a.hash(h);
            }
            Self::List(items) => {
                2u8.hash(h);
                items.len().hash(h);
                for i in items {
                    i.hash(h);
                }
            }
            // The four quote-family variants share the (discriminator, inner)
            // hash shape — all route through `as_quote_form`'s typed-marker
            // projection so the per-variant discriminator bytes (3/4/5/6) and
            // the recursive `inner.hash(h)` body bind at ONE site on the
            // closed-set `QuoteForm` algebra. The `.expect(_)` is a
            // static-invariant statement (the outer pattern guarantees the
            // projection lands `Some`) that a future quote-family extension
            // can't drift across — adding a fifth `Sexp` wrapper variant
            // forces a corresponding `QuoteForm` extension AND
            // `as_quote_form` arm, with rustc binding the three together.
            Self::Quote(_) | Self::Quasiquote(_) | Self::Unquote(_) | Self::UnquoteSplice(_) => {
                let (qf, inner) = self
                    .as_quote_form()
                    .expect("matched quote-family variant must project to Some via as_quote_form");
                qf.hash_discriminator().hash(h);
                inner.hash(h);
            }
        }
    }
}

impl Hash for Atom {
    fn hash<H: Hasher>(&self, h: &mut H) {
        match self {
            Self::Symbol(s) => {
                0u8.hash(h);
                s.hash(h);
            }
            Self::Keyword(s) => {
                1u8.hash(h);
                s.hash(h);
            }
            Self::Str(s) => {
                2u8.hash(h);
                s.hash(h);
            }
            Self::Int(n) => {
                3u8.hash(h);
                n.hash(h);
            }
            // Float: hash the bit pattern. NaN != NaN so PartialEq is broken,
            // but cache lookups use PartialEq-by-hash which this satisfies
            // modulo a NaN collision risk we accept for template args.
            Self::Float(f) => {
                4u8.hash(h);
                f.to_bits().hash(h);
            }
            Self::Bool(b) => {
                5u8.hash(h);
                b.hash(h);
            }
        }
    }
}

/// An S-expression — the homoiconic value + program representation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Sexp {
    Nil,
    Atom(Atom),
    List(Vec<Sexp>),
    /// `'x` — literal; does not participate in macro substitution.
    Quote(Box<Sexp>),
    /// `` `x `` — quasi-quotation; substitution happens inside.
    Quasiquote(Box<Sexp>),
    /// `,x` — substitute the binding named `x`. Only valid inside a quasi-quote.
    Unquote(Box<Sexp>),
    /// `,@x` — splice the list `x` into the containing list.
    UnquoteSplice(Box<Sexp>),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Atom {
    /// Plain symbol (`foo`, `defpoint`, `seph.1`).
    Symbol(String),
    /// Keyword (`:parent`, `:attr`) — a symbol bound to itself.
    Keyword(String),
    /// String literal.
    Str(String),
    /// Integer literal.
    Int(i64),
    /// Floating literal.
    Float(f64),
    /// Boolean literal (`#t`, `#f`).
    Bool(bool),
}

impl Sexp {
    pub fn symbol(s: impl Into<String>) -> Self {
        Self::Atom(Atom::Symbol(s.into()))
    }
    pub fn keyword(s: impl Into<String>) -> Self {
        Self::Atom(Atom::Keyword(s.into()))
    }
    pub fn string(s: impl Into<String>) -> Self {
        Self::Atom(Atom::Str(s.into()))
    }
    pub fn int(n: i64) -> Self {
        Self::Atom(Atom::Int(n))
    }
    pub fn float(n: f64) -> Self {
        Self::Atom(Atom::Float(n))
    }
    pub fn boolean(b: bool) -> Self {
        Self::Atom(Atom::Bool(b))
    }

    pub fn is_list(&self) -> bool {
        matches!(self, Self::List(_))
    }
    pub fn as_list(&self) -> Option<&[Sexp]> {
        match self {
            Self::List(xs) => Some(xs),
            _ => None,
        }
    }
    pub fn as_symbol(&self) -> Option<&str> {
        match self {
            Self::Atom(Atom::Symbol(s)) => Some(s),
            _ => None,
        }
    }
    pub fn as_keyword(&self) -> Option<&str> {
        match self {
            Self::Atom(Atom::Keyword(s)) => Some(s),
            _ => None,
        }
    }
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::Atom(Atom::Str(s)) => Some(s),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Atom(Atom::Int(n)) => Some(*n),
            _ => None,
        }
    }
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Atom(Atom::Float(n)) => Some(*n),
            Self::Atom(Atom::Int(n)) => Some(*n as f64),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Atom(Atom::Bool(b)) => Some(*b),
            _ => None,
        }
    }
    /// `foo` or `"foo"` — useful for names that may be authored either way.
    pub fn as_symbol_or_string(&self) -> Option<&str> {
        self.as_symbol().or_else(|| self.as_string())
    }

    /// The symbol in operator position — `Some(s)` iff this is a non-empty
    /// list whose first element is a symbol (`(defpoint …)` → `Some("defpoint")`).
    /// `None` for every other shape: a non-list (`foo`, `5`, `:kw`), the
    /// empty list `()`, and a list whose head is not a symbol (`(5 …)`,
    /// `(:kw …)`, `((nested) …)`).
    ///
    /// This is the *operator-position projection* — the structural query
    /// every form-dispatch site in the substrate keys on: "what operator
    /// does this form invoke?" Macroexpansion (`Expander::expand` looks up
    /// the head against the macro table; `macro_def_from` reads it to
    /// recognize a `defmacro` head) and the typed compilers
    /// (`compile_typed` / `compile_named_from_forms` match it against
    /// `T::KEYWORD`) all asked the same `self.as_list()?.first()?.as_symbol()`
    /// question inline. Naming it once makes "operator position" a primitive
    /// of the `Sexp` algebra rather than four byte-identical inline chains.
    ///
    /// This is the SOFT face of operator-position dispatch — it answers
    /// "is this form an invocation of some operator?" and yields `None`
    /// (skip / fall through) for everything that isn't, with no diagnostic.
    /// Its STRICT sibling is `TataraDomain::compile_from_sexp`, which on a
    /// matched-arity form distinguishes the empty-list and
    /// present-but-not-a-symbol head sub-modes to emit a rich
    /// `MissingHeadSymbol` rejection. The two are the dispatch (`head_symbol`)
    /// and the gate (`compile_from_sexp`) faces of the same projection;
    /// keeping both lets a site choose "skip silently" or "reject loudly"
    /// without re-deriving the head.
    ///
    /// `head_symbol` is the operator projection of [`Sexp::as_call`]: it
    /// keeps the head and discards the argument tail. The
    /// `as_list()?.first()?.as_symbol()` chain lives in ONE place
    /// (`as_call`); this is its first component.
    pub fn head_symbol(&self) -> Option<&str> {
        self.as_call().map(|(head, _)| head)
    }

    /// Decompose a call form into its operator and argument tail —
    /// `Some((op, args))` iff this is a non-empty list whose first element
    /// is a symbol, where `op` is that head symbol and `args` is the
    /// remaining elements (`&self[1..]`, possibly empty). `None` for every
    /// shape `head_symbol` rejects: a non-list, the empty list, and a list
    /// whose head is present but not a symbol.
    ///
    /// This is the *call-form decomposition* — the structural shape of a
    /// Lisp invocation: an operator applied to an argument tail. It pairs
    /// the operator-position projection (`head_symbol`) with the argument
    /// tail every dispatch site reads immediately after matching the
    /// operator. Macroexpansion (`Expander::expand`) applies the matched
    /// macro to `&list[1..]`; the typed compilers (`compile_typed`,
    /// `compile_named_from_forms`) feed `&list[1..]` into
    /// `T::compile_from_args`. Before this query each site bound
    /// `as_list()` for the tail AND independently called `head_symbol()`
    /// (which itself re-derives `as_list().first()`) for the operator —
    /// two traversals of the same list, two projections. `as_call` yields
    /// both from one match, so the operator and its arguments can never
    /// drift out of agreement at a dispatch site.
    ///
    /// Soft face, like `head_symbol`: it answers "is this an invocation of
    /// some operator, and what are its arguments?" and yields `None` (skip
    /// / fall through) for everything that isn't, with no diagnostic. The
    /// strict gate sibling is `TataraDomain::compile_from_sexp`, which
    /// distinguishes the empty-list and non-symbol-head sub-modes to reject
    /// loudly.
    pub fn as_call(&self) -> Option<(&str, &[Sexp])> {
        let list = self.as_list()?;
        let head = list.first()?.as_symbol()?;
        Some((head, &list[1..]))
    }

    /// Decompose a call form into its argument tail IFF the head matches the
    /// supplied `keyword` — `Some(args)` iff this is a non-empty list whose
    /// first element is a symbol equal to `keyword`, where `args` is the
    /// remaining elements (`&self[1..]`, possibly empty). `None` for every
    /// shape `as_call` rejects AND for every call whose head is present but
    /// differs from `keyword`.
    ///
    /// This is the *keyword-typed call decomposition* — the natural
    /// extension of [`Sexp::as_call`] for the "is this a call to ONE
    /// specific operator?" question every typed-domain dispatch site asks
    /// after macroexpansion. [`compile_typed`](crate::compile::compile_typed)
    /// and [`compile_named_from_forms`](crate::compile::compile_named_from_forms)
    /// both opened the same two-step chain inline —
    /// `if let Some((head, args)) = form.as_call() { if head == T::KEYWORD { … } }`
    /// — at every form they walked; the chain IS this projection. Naming
    /// it lifts "is this form a call to T?" from a two-step inline pattern
    /// to ONE structural query on the `Sexp` algebra. A regression that
    /// drifts one consumer's comparison from `==` to `!= `, or that
    /// compares against a different label than `T::KEYWORD` (e.g.
    /// substring-grepping the rendered head), becomes structurally
    /// impossible: there is exactly one implementation both dispatchers
    /// route through.
    ///
    /// Soft face, like the rest of the `as_*` family: it answers "is this
    /// a call to `keyword`, and what are its arguments?" and yields `None`
    /// for everything that isn't (skip / fall through), with no
    /// diagnostic. The strict gate sibling is
    /// `TataraDomain::compile_from_sexp`, which distinguishes the
    /// not-a-list / empty-list / non-symbol-head / wrong-keyword
    /// sub-modes to reject loudly. The two are the dispatch
    /// (`as_call_to`) and the gate (`compile_from_sexp`) faces of the
    /// same projection; keeping both lets a site choose "skip silently"
    /// or "reject loudly" without re-deriving the head.
    ///
    /// Structural identity binding it to its siblings:
    ///   * `as_call_to(keyword) == as_call().and_then(|(h, args)| (h == keyword).then_some(args))`
    ///   * `as_call_to(keyword).is_some() == (head_symbol() == Some(keyword))`
    ///
    /// The returned `&[Sexp]` borrows from the list's tail verbatim — no
    /// copy, no allocation, same lifetime as [`Sexp::as_call`]'s tail.
    ///
    /// Slice-side sibling: [`iter_calls_to`] lifts this per-form projection
    /// onto a `&[Sexp]`, yielding the args slices of every matching form in
    /// source order — the substrate's typed-keyword filter over a batch of
    /// forms, structurally bound to this per-form projection via the
    /// closed-form composition
    /// `iter_calls_to(forms, k) == forms.iter().filter_map(|f| f.as_call_to(k))`.
    pub fn as_call_to(&self, keyword: &str) -> Option<&[Sexp]> {
        let (head, args) = self.as_call()?;
        (head == keyword).then_some(args)
    }

    /// Decompose a call form whose head decodes through a caller-supplied
    /// classifier — `Some((decoded, args))` iff this is a non-empty list
    /// whose first element is a symbol AND `decode(head)` returns
    /// `Some(decoded)`, where `args` is the remaining elements
    /// (`&self[1..]`, possibly empty). `None` for every shape
    /// [`Sexp::as_call`] rejects AND for every call whose head is present
    /// but `decode` rejects.
    ///
    /// This is the *typed-decoded call decomposition* — the closure-typed
    /// extension of [`Sexp::as_call_to`] for the "is this a call whose head
    /// belongs to a CLOSED SET (or a LIVE REGISTRY) that decodes to a typed
    /// witness?" question. Where [`Sexp::as_call_to`] filters by ONE
    /// constant keyword, `as_call_to_any` filters AND TYPES by a caller-
    /// supplied projection — every dispatch site that asks "is this form
    /// an invocation of any of N operators, decoded as a typed enum or
    /// resolved against a runtime table?" binds to ONE structural query
    /// on the `Sexp` algebra. Two consumers route through it:
    ///
    ///   * The macro-expander's `macro_def_from` — closed-set classifier:
    ///     `as_call_to_any(MacroDefHead::from_keyword)` decides which of
    ///     `{defmacro, defpoint-template, defcheck}` a top-level form
    ///     invokes, decoded to the typed `MacroDefHead` enum. Pre-lift the
    ///     site opened the same three-step chain inline — `let Some(list)
    ///     = form.as_list()…; let Some(head) = form.head_symbol()…; let
    ///     Some(decoded) = MacroDefHead::from_keyword(head)…`.
    ///   * The macro-expander's `Expander::expand` — live-registry
    ///     classifier: `as_call_to_any(|h| self.macros.get(h))` decides
    ///     which of the registered macros (a `HashMap<String, MacroDef>`
    ///     populated by `expand_program`'s `defmacro` recognition) a form
    ///     invokes, decoded to `&MacroDef`. Pre-lift the site opened the
    ///     same `as_list() + as_call() + self.macros.get(head)` chain
    ///     inline — `as_list()` for the children-walk fallthrough,
    ///     `as_call()` for the (head, args) pair (which itself re-derives
    ///     `as_list()` internally), and `self.macros.get(head)` for the
    ///     registry lookup.
    ///
    /// Naming the projection lifts "is this form a call to any of N
    /// operators, decoded to T?" from the three-step inline pattern to
    /// ONE structural query — closed-set enum classifier OR live-registry
    /// HashMap classifier, the family primitive is uniform under both.
    ///
    /// Soft face, like the rest of the `as_*` family: it answers "is this
    /// a call whose head decodes through `F`, and what are its arguments?"
    /// and yields `None` for everything that isn't (skip / fall through),
    /// with no diagnostic. The strict gate sibling stays
    /// `TataraDomain::compile_from_sexp` — that distinguishes the
    /// not-a-list / empty-list / non-symbol-head / wrong-keyword sub-modes
    /// to reject loudly for a single-keyword consumer. The two are the
    /// closed-set-decoded dispatch (`as_call_to_any`) and the
    /// single-keyword gate (`compile_from_sexp`) faces of the typed-domain
    /// recognition problem; keeping both lets a site choose "skip
    /// silently if the head isn't ours" or "reject loudly if the head
    /// isn't the exact keyword" without re-deriving the head.
    ///
    /// Structural identity binding it to its siblings:
    ///   * `as_call_to_any(decode) == as_call().and_then(|(h, args)| decode(h).map(|d| (d, args)))`
    ///   * `as_call_to(k) == as_call_to_any(|h| (h == k).then_some(())).map(|(_, a)| a)` (modulo the discarded `()`)
    ///   * `as_call_to_any(decode).is_some() == as_call().map_or(false, |(h, _)| decode(h).is_some())`
    ///
    /// The returned `&[Sexp]` borrows from the list's tail verbatim — no
    /// copy, no allocation, same lifetime as [`Sexp::as_call`]'s tail.
    /// `T` is owned because `decode` is `FnOnce(&str) -> Option<T>` and a
    /// `&'_ str` borrow into the head symbol would not outlive the helper
    /// boundary; consumers projecting to a typed `Copy` enum (e.g.
    /// `MacroDefHead`) get the value directly, consumers projecting to a
    /// borrowed `&'static str` (a closed-set head) project to
    /// `&'static str` and inherit the static lifetime through the
    /// classifier.
    pub fn as_call_to_any<F, T>(&self, decode: F) -> Option<(T, &[Sexp])>
    where
        F: FnOnce(&str) -> Option<T>,
    {
        let (head, args) = self.as_call()?;
        decode(head).map(|d| (d, args))
    }

    /// Decompose an unquote-family form into its typed marker and inner
    /// expression — `Some((UnquoteForm::Unquote, inner))` iff this is `,x`
    /// (a [`Sexp::Unquote`] wrapper), `Some((UnquoteForm::Splice, inner))`
    /// iff this is `,@x` (a [`Sexp::UnquoteSplice`] wrapper), `None` for
    /// every other shape (Quote, Quasiquote, Nil, Atom, List).
    ///
    /// This is the *unquote-family projection* — the typed-marker peer of
    /// [`Sexp::as_call`] for the macro-template substitution surface. Where
    /// [`Sexp::as_call`] decomposes `(op args …)` into a `(head, args)`
    /// pair, `as_unquote` decomposes `,x` / `,@x` into a `(form, inner)`
    /// pair where `form: UnquoteForm` is the closed-set typed marker
    /// (`Unquote` for `,`, `Splice` for `,@`) and `inner: &Sexp` is the
    /// borrowed body. The pairing of `Sexp::Unquote ↔ UnquoteForm::Unquote`
    /// and `Sexp::UnquoteSplice ↔ UnquoteForm::Splice` is the structural
    /// invariant the macro-expander's substitution path keys every
    /// rejection on — naming the projection lifts the pair from
    /// per-callsite discipline (two `Sexp::Unquote(inner)` arms paired
    /// with two `UnquoteForm::Unquote` literals at distinct sites, two
    /// `Sexp::UnquoteSplice(inner)` arms paired with two
    /// `UnquoteForm::Splice` literals at distinct sites) into ONE typed
    /// projection both expansion strategies route through.
    ///
    /// Three consumers in [`macro_expand`](crate::macro_expand) route
    /// through this primitive:
    ///   * `compile_node` (bytecode-template compile path) — `,x` becomes
    ///     `TemplateOp::Subst(idx)`, `,@x` becomes `TemplateOp::Splice(idx)`;
    ///     both arms share the gate-1+gate-2 composition
    ///     `resolve_unquote_in_params(inner, params, form)?` keyed on the
    ///     typed `form` projection.
    ///   * `substitute` top-level (substitute fallback path) — `,x` resolves
    ///     to its bound value, `,@x` rejects with
    ///     `LispError::SpliceOutsideList` (a splice form with no containing
    ///     list to flatten into).
    ///   * `substitute` list-inner (substitute fallback path's per-item
    ///     walk) — `,@x` items splice their bound list/nil/scalar value
    ///     into the assembled list builder via
    ///     [`crate::macro_expand::splice_value_into`]; non-splice items
    ///     recurse into `substitute`.
    ///
    /// Pre-lift each site opened the same per-variant match arms —
    /// `Sexp::Unquote(inner) => … UnquoteForm::Unquote …` and
    /// `Sexp::UnquoteSplice(inner) => … UnquoteForm::Splice …` —
    /// independently. The (Sexp variant, UnquoteForm variant) pairing was
    /// load-bearing across distinct sites yet only enforced by callsite
    /// discipline. Post-lift the pair binds at ONE projection function the
    /// type system threads through `(UnquoteForm, &Sexp)`: a regression
    /// that drifts ONE site's pairing (e.g. a future emitter that matches
    /// `Sexp::Unquote(_)` but threads `UnquoteForm::Splice` into
    /// `unquote_target_symbol` — type-checks but renders a misleading
    /// diagnostic) becomes structurally impossible.
    ///
    /// Soft face, like the rest of the `as_*` family on `Sexp`: it answers
    /// "is this form an unquote-family marker, and what does it wrap?" and
    /// yields `None` for everything that isn't (skip / fall through), with
    /// no diagnostic. The strict siblings —
    /// [`crate::macro_expand::splice_value_into`] for the bound-list
    /// coercion, `non_symbol_unquote_target` /
    /// `splice_outside_list` for the per-failure-mode rejections — keep
    /// their loud-reject posture; this projection is the dispatch face the
    /// soft pre-rejection walk binds to.
    ///
    /// Structural identity binding it to the unquote-family variants:
    ///   * `as_unquote() == Some((UnquoteForm::Unquote, inner))` iff `self == Sexp::Unquote(inner)`
    ///   * `as_unquote() == Some((UnquoteForm::Splice, inner))`  iff `self == Sexp::UnquoteSplice(inner)`
    ///   * `as_unquote().is_some() == matches!(self, Sexp::Unquote(_) | Sexp::UnquoteSplice(_))`
    ///
    /// The returned `&Sexp` borrows the inner box's body verbatim — no
    /// clone, no allocation — same lifetime as `&self`. The closed-set
    /// guarantee on [`UnquoteForm`] (exactly `Unquote ⊎ Splice`) is
    /// threaded through this projection's return tuple, so consumers that
    /// pattern-match on `form: UnquoteForm` get rustc-enforced
    /// exhaustiveness — a future `Sexp` variant must extend `UnquoteForm`
    /// AND this match arm together (or stay outside the unquote family
    /// and project to `None`), eliminating the silent two-site
    /// extension-drift this lift was already designed to forbid.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; the
    /// `(Sexp::Unquote, UnquoteForm::Unquote)` and
    /// `(Sexp::UnquoteSplice, UnquoteForm::Splice)` pairings appear ≥3
    /// times across `compile_node` (2 arms) + `substitute` (top-level +
    /// list-inner) — past the PRIME-DIRECTIVE trigger once the structural
    /// shape is named. THEORY.md §V.1 — knowable platform; the
    /// unquote-family projection becomes a NAMED primitive on the
    /// substrate's `Sexp` algebra rather than per-site `Sexp::Unquote(_)
    /// | Sexp::UnquoteSplice(_)` inline matches paired with per-site
    /// `UnquoteForm::Unquote` / `UnquoteForm::Splice` literals.
    /// THEORY.md §II.1 invariant 1 — typed entry; the macro-template
    /// substitution surface's typed-marker projection IS the rust-level
    /// typed-entry gate's structural component, lifted from per-site
    /// duplication onto ONE rust method the substrate's diagnostic
    /// promotions hang off of. THEORY.md §II.1 invariant 2 — free middle;
    /// both expansion strategies (bytecode `compile_node` and substitute
    /// fallback `substitute`) route through the SAME projection, so a
    /// regression that drifts ONE strategy's (Sexp variant, UnquoteForm
    /// variant) pairing from the other cannot reach the substrate's
    /// runtime — the type system binds both strategies to the
    /// projection's single emission shape.
    ///
    /// Frontier inspiration: Racket's `syntax-parse` `~or* (~unquote stx)
    /// (~unquote-splice stx)` pattern — every macro-template pattern over
    /// `,id` / `,@id` binds to ONE typed decomposition that surfaces the
    /// marker identity alongside the inner expression; the substrate's
    /// `as_unquote` is the Rust-typed peer of that pattern, lifted onto
    /// the `Sexp` algebra with [`UnquoteForm`] standing in for Racket's
    /// pattern-class identity. MLIR's typed-IR projection
    /// `mlir::dyn_cast<UnquoteFamilyOp>(op)` — the typed downcast from a
    /// polymorphic IR node onto a closed-set op family is the MLIR idiom;
    /// `as_unquote` is the unstructured-projection peer on the substrate's
    /// `Sexp` algebra, with `Option<(UnquoteForm, &Sexp)>` standing in for
    /// MLIR's typed downcast result.
    pub fn as_unquote(&self) -> Option<(UnquoteForm, &Sexp)> {
        let (qf, inner) = self.as_quote_form()?;
        qf.as_unquote_form().map(|uf| (uf, inner))
    }

    /// Decompose a quote-family form into its typed marker and inner
    /// expression — `Some((QuoteForm::Quote, inner))` iff this is `'x`
    /// (a [`Sexp::Quote`] wrapper), `Some((QuoteForm::Quasiquote, inner))`
    /// iff this is `` `x `` (a [`Sexp::Quasiquote`] wrapper),
    /// `Some((QuoteForm::Unquote, inner))` iff this is `,x` (a
    /// [`Sexp::Unquote`] wrapper), `Some((QuoteForm::UnquoteSplice, inner))`
    /// iff this is `,@x` (a [`Sexp::UnquoteSplice`] wrapper), `None` for
    /// every other shape (Nil, Atom, List).
    ///
    /// This is the *quote-family projection* — the typed-marker peer of
    /// [`Sexp::as_unquote`] generalized across all four homoiconic
    /// prefix-wrappers. Where [`Sexp::as_unquote`] keys the macro-template
    /// SUBSTITUTION surface on the closed pair `{Unquote, Splice}` (the
    /// two prefixes whose template-time semantic is substitution),
    /// `as_quote_form` keys the WIRE-SHAPE surfaces (Display rendering,
    /// Hash discrimination, canonical-form interop) on the closed superset
    /// `{Quote, Quasiquote, Unquote, UnquoteSplice}` — all four prefixes
    /// the reader can tokenize and the writer must round-trip. The
    /// `Sexp::as_unquote` projection now derives structurally from
    /// `as_quote_form`'s output via [`QuoteForm::as_unquote_form`] — the
    /// 2-of-4 subset gate — so the two projections share a SINGLE
    /// implementation site on the `Sexp` algebra and the
    /// (Sexp variant, QuoteForm variant) pairing binds at ONE rust
    /// function regardless of whether the consumer wants the substitution
    /// subset or the wire-shape superset.
    ///
    /// Three consumers in this file route through this primitive:
    ///   * `Hash for Sexp` — the four `Quote`/`Quasiquote`/`Unquote`/
    ///     `UnquoteSplice` arms (pre-lift each carrying its own
    ///     `<discr>.hash(h); inner.hash(h)` body) collapse to ONE arm
    ///     that routes through `as_quote_form` and reads the
    ///     discriminator via [`QuoteForm::hash_discriminator`].
    ///   * `Display for Sexp` — the four `write!(f, "<prefix>{inner}")`
    ///     arms (pre-lift each carrying its own literal prefix string)
    ///     collapse to ONE arm that routes through `as_quote_form` and
    ///     reads the prefix via [`QuoteForm::prefix`].
    ///   * [`Sexp::as_unquote`] — derives `Option<(UnquoteForm, &Sexp)>`
    ///     by composing `as_quote_form` with [`QuoteForm::as_unquote_form`]
    ///     (the 2-of-4 subset projection), so the macro-template
    ///     substitution surface inherits the (Sexp variant, marker)
    ///     pairing through this projection's typed dispatch rather than
    ///     re-deriving its own arm-based match.
    ///
    /// The closed-set guarantee on [`QuoteForm`] (exactly
    /// `Quote ⊎ Quasiquote ⊎ Unquote ⊎ UnquoteSplice`) is threaded through
    /// this projection's return tuple, so consumers that pattern-match on
    /// `form: QuoteForm` get rustc-enforced exhaustiveness — a future
    /// `Sexp` wrapper variant must extend `QuoteForm` AND this match arm
    /// together (or stay outside the quote family and project to `None`),
    /// eliminating the silent multi-site extension-drift this lift was
    /// designed to forbid.
    ///
    /// Soft face, like the rest of the `as_*` family on `Sexp`: it
    /// answers "is this form a quote-family marker, and what does it
    /// wrap?" and yields `None` for everything that isn't (skip / fall
    /// through), with no diagnostic.
    ///
    /// Structural identity binding it to the quote-family variants and
    /// its `as_unquote` subset sibling:
    ///   * `as_quote_form() == Some((QuoteForm::Quote, inner))`         iff `self == Sexp::Quote(inner)`
    ///   * `as_quote_form() == Some((QuoteForm::Quasiquote, inner))`    iff `self == Sexp::Quasiquote(inner)`
    ///   * `as_quote_form() == Some((QuoteForm::Unquote, inner))`       iff `self == Sexp::Unquote(inner)`
    ///   * `as_quote_form() == Some((QuoteForm::UnquoteSplice, inner))` iff `self == Sexp::UnquoteSplice(inner)`
    ///   * `as_quote_form().is_some() == matches!(self, Sexp::Quote(_) | Sexp::Quasiquote(_) | Sexp::Unquote(_) | Sexp::UnquoteSplice(_))`
    ///   * `as_unquote() == as_quote_form().and_then(|(qf, inner)| qf.as_unquote_form().map(|uf| (uf, inner)))`
    ///
    /// The returned `&Sexp` borrows the inner box's body verbatim — no
    /// clone, no allocation — same lifetime as `&self` and same posture
    /// as [`Sexp::as_unquote`]'s tail.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; the
    /// quote-family (Sexp variant, prefix string, hash discriminator)
    /// triple appeared inline at three sites (`Hash for Sexp`,
    /// `Display for Sexp`, `as_unquote`) — well past the ≥2 PRIME-DIRECTIVE
    /// trigger once the structural shape is named. THEORY.md §V.1 —
    /// knowable platform; the quote-family typed-marker projection becomes
    /// a NAMED primitive on the substrate's `Sexp` algebra rather than
    /// per-site inline matches paired with per-site discriminator literals
    /// and prefix literals. THEORY.md §II.1 invariant 1 — typed entry; the
    /// reader's prefix-to-variant dispatch ([`crate::reader::read_quoted`])
    /// AND the Display impl's variant-to-prefix dispatch are dual
    /// typed-entry / typed-exit gates over the same closed set; the
    /// `QuoteForm` algebra threads BOTH gates through ONE typed enum so a
    /// regression that drifts one side's prefix from the other (e.g. the
    /// reader gains a fifth prefix but the Display impl doesn't) is no
    /// longer a silent two-site divergence — rustc binds both sides to
    /// the same closed-set enum. THEORY.md §II.1 invariant 2 — free
    /// middle; the three consumers (Hash, Display, `as_unquote`) route
    /// through the SAME projection, so a regression that drifts ONE
    /// consumer's (Sexp variant, marker) pairing from the others cannot
    /// reach the substrate's runtime.
    ///
    /// Frontier inspiration: Racket's `syntax-parse` `~or* (~quote stx)
    /// (~quasiquote stx) (~unquote stx) (~unquote-splice stx)` pattern —
    /// every macro-template pattern over `'`/`` ` ``/`,`/`,@` binds to
    /// ONE typed decomposition that surfaces the marker identity
    /// alongside the inner expression; the substrate's `as_quote_form` is
    /// the Rust-typed peer of that pattern, lifted onto the `Sexp`
    /// algebra with `QuoteForm` standing in for Racket's pattern-class
    /// identity at the homoiconic prefix surface. MLIR's typed-IR
    /// projection `mlir::dyn_cast<QuoteFamilyOp>(op)` — the typed downcast
    /// from a polymorphic IR node onto a closed-set op family is the MLIR
    /// idiom; `as_quote_form` is the unstructured-projection peer on the
    /// substrate's `Sexp` algebra, with `Option<(QuoteForm, &Sexp)>`
    /// standing in for MLIR's typed downcast result.
    pub fn as_quote_form(&self) -> Option<(QuoteForm, &Sexp)> {
        match self {
            Self::Quote(inner) => Some((QuoteForm::Quote, inner)),
            Self::Quasiquote(inner) => Some((QuoteForm::Quasiquote, inner)),
            Self::Unquote(inner) => Some((QuoteForm::Unquote, inner)),
            Self::UnquoteSplice(inner) => Some((QuoteForm::UnquoteSplice, inner)),
            _ => None,
        }
    }
}

/// Closed-set typed identifier for the four homoiconic prefix-wrappers in
/// the substrate's `Sexp` algebra — `'x` ([`Sexp::Quote`]), `` `x ``
/// ([`Sexp::Quasiquote`]), `,x` ([`Sexp::Unquote`]), `,@x`
/// ([`Sexp::UnquoteSplice`]) — paired with the projections each consumer
/// surface needs ([`Self::prefix`] for [`crate::ast::Sexp`]'s `Display`
/// impl AND the reader's prefix dispatch dual, [`Self::hash_discriminator`]
/// for [`crate::ast::Sexp`]'s `Hash` impl, [`Self::as_unquote_form`] for
/// the 2-of-4 subset gate the template-substitution surface keys on).
///
/// Mirror at the homoiconic-prefix-wrapper boundary of the prior-run
/// `UnquoteForm` (template-marker subset, 2 variants),
/// `CompilerSpecIoStage` (disk-persistence surface),
/// `TemplateInvariantKind` (bytecode-runtime surface), `MacroDefHead`
/// (macro-definition-head closed set), and `KwargPath` (kwargs-path-shape
/// surface) closed-set lifts: those enums key their respective rejection
/// or projection variants on a typed identity carried inside the variant's
/// data shape; this enum keys the FOUR distinct quote-family rendering /
/// hashing / template-substitution sites on a typed marker identity.
/// Adding a fifth homoiconic prefix-wrapper (e.g., a hypothetical `,~`
/// reverse-unquote) requires extending this enum, which rustc-enforces
/// matching at every projection site (`prefix`, `hash_discriminator`,
/// `as_unquote_form`, plus `Sexp::as_quote_form`'s match arm) — the closed
/// set becomes a TYPE rather than four `&'static str` / `u8` literals that
/// could drift independently across `Sexp::Display`'s prefix arm and
/// `Sexp::Hash`'s discriminator arm and the reader's prefix dispatch.
///
/// Subset-gate relationship to [`UnquoteForm`]: the template-substitution
/// surface's [`Sexp::as_unquote`] is now `as_quote_form().and_then(|(qf,
/// inner)| qf.as_unquote_form().map(|uf| (uf, inner)))` — the 2-of-4
/// projection lives at ONE site on this algebra ([`Self::as_unquote_form`])
/// rather than being re-derived at every consumer that wants only the
/// `{Unquote, UnquoteSplice}` subset. A future enum variant that joins
/// the template-substitution subset (e.g. a typed `defalias`-projected
/// fifth marker) extends [`UnquoteForm`] AND
/// [`Self::as_unquote_form`]'s arm together, with rustc binding the
/// extension through the projection's `Option` return type.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
/// homoiconic-prefix-wrapper dispatch (the reader's prefix-to-variant
/// gate AND the Display impl's variant-to-prefix dual) IS the rust-level
/// typed-entry / typed-exit gate, and naming its closed-set identity
/// lifts the gate from per-site literal-pair discipline to ONE typed
/// enum the substrate's diagnostic promotions hang off of.
/// THEORY.md §V.1 — knowable platform; the closed set of homoiconic
/// prefix-wrappers becomes a TYPE rather than four `&'static str` / `u8`
/// literals scattered across Hash / Display / interop / sexp_shape — a
/// typo in any one site is no longer a runtime drift but a compile error
/// against the typed projection. THEORY.md §VI.1 — generation over
/// composition; the typed enum lands the structural-completeness floor
/// for the quote-family surface, parallel to how `UnquoteForm` lands it
/// for the template-marker subset and `MacroDefHead` for the
/// macro-definition-head surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteForm {
    /// `'x` — literal-quote prefix. The `'` marker; the inner expression
    /// is NOT subject to macro substitution. Projects to NO
    /// `UnquoteForm` (the template-substitution surface ignores quote).
    Quote,
    /// `` `x `` — quasi-quote prefix. The `` ` `` marker; the inner
    /// expression is the template body inside which `,` and `,@` mark
    /// substitution points. Projects to NO `UnquoteForm` (a quasi-quote
    /// is the substitution SCOPE, not a substitution itself).
    Quasiquote,
    /// `,x` — single-value substitution. The `,` marker; the inner
    /// symbol is substituted with its bound value at template
    /// expansion. Projects to `UnquoteForm::Unquote` for the
    /// template-substitution surface.
    Unquote,
    /// `,@x` — list-splice substitution. The `,@` marker; the inner
    /// symbol must be bound to a list, whose elements are flattened
    /// into the containing list at template expansion. Projects to
    /// `UnquoteForm::Splice` for the template-substitution surface.
    UnquoteSplice,
}

impl QuoteForm {
    /// Canonical `&'static str` prefix that paired with the variant
    /// renders the homoiconic form — `"'"` for [`Self::Quote`],
    /// `` "`" `` for [`Self::Quasiquote`], `","` for [`Self::Unquote`],
    /// `",@"` for [`Self::UnquoteSplice`]. Threaded through
    /// [`crate::ast::Sexp`]'s `Display` impl so the per-variant prefix
    /// rendering lives at ONE site on this algebra rather than four
    /// inline literal strings across the Display arms.
    ///
    /// Structural dual of the reader's [`crate::reader::read_quoted`]
    /// dispatch: the reader maps prefix-tokens to `Sexp::{Quote,
    /// Quasiquote, Unquote, UnquoteSplice}` constructors; this method
    /// maps the typed `QuoteForm` marker back to its canonical prefix
    /// string. Adding a fifth prefix extends both sides — the reader's
    /// tokenizer + dispatch AND this method — with rustc enforcing
    /// the pair through the closed-set enum. Round-trip:
    /// `read(format!("{}{inner}", qf.prefix()))` produces the
    /// `Sexp::*` variant matching `qf`, by construction.
    ///
    /// The `&'static str` lifetime is load-bearing: it lets every
    /// consumer (Display arm, future format strings, future interop
    /// canonical-form taggers) project through this method without
    /// an allocation, parallel to how [`UnquoteForm::marker`]
    /// projects its 2-of-4 subset surface.
    #[must_use]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Quote => "'",
            Self::Quasiquote => "`",
            Self::Unquote => ",",
            Self::UnquoteSplice => ",@",
        }
    }

    /// Stable, per-variant byte discriminator that paired with the
    /// recursive inner hash builds the substrate's `Hash for Sexp`
    /// projection — `3` for [`Self::Quote`], `4` for
    /// [`Self::Quasiquote`], `5` for [`Self::Unquote`], `6` for
    /// [`Self::UnquoteSplice`]. The byte values are load-bearing
    /// because the expansion cache (`Expander::cache`) keys on the
    /// hash of `(macro_name, args)` — changing a discriminator silently
    /// invalidates every cached expansion AND mis-collides with the
    /// reserved bytes the non-quote-family Hash arms use (`0` for
    /// `Nil`, `1` for `Atom`, `2` for `List`). The closed set ensures
    /// the four arms partition `{3, 4, 5, 6}` injectively against the
    /// reserved bytes — a future quote-family extension must extend
    /// this method AND the non-quote-family arms in lockstep, with
    /// rustc binding the consistency through exhaustiveness over the
    /// closed enum.
    ///
    /// `pub(crate)` because the byte-discriminator surface is an
    /// implementation detail of the substrate's `Hash for Sexp` cache-
    /// key contract; exposing it publicly would leak the cache-key
    /// shape through the API without enabling any external consumer
    /// the public projections (`Sexp::as_quote_form`, `Self::prefix`,
    /// `Self::as_unquote_form`) don't already serve.
    #[must_use]
    pub(crate) fn hash_discriminator(self) -> u8 {
        match self {
            Self::Quote => 3,
            Self::Quasiquote => 4,
            Self::Unquote => 5,
            Self::UnquoteSplice => 6,
        }
    }

    /// Project the 4-of-4 quote-family marker into the 2-of-4
    /// template-substitution subset — `Some(UnquoteForm::Unquote)` for
    /// [`Self::Unquote`], `Some(UnquoteForm::Splice)` for
    /// [`Self::UnquoteSplice`], `None` for [`Self::Quote`] /
    /// [`Self::Quasiquote`] (the literal-quote and quasi-quote
    /// prefixes are wrappers, NOT substitution points). ONE projection
    /// on this algebra the [`crate::ast::Sexp::as_unquote`] derivation
    /// routes through — the (Sexp variant, UnquoteForm marker) pairing
    /// now binds at the typed [`crate::ast::Sexp::as_quote_form`]
    /// projection's output composed with this method's output, instead
    /// of being re-derived per-arm inside `Sexp::as_unquote`.
    ///
    /// The closed-set guarantee on [`UnquoteForm`] (exactly
    /// `Unquote ⊎ Splice`) AND on [`Self`] (exactly
    /// `Quote ⊎ Quasiquote ⊎ Unquote ⊎ UnquoteSplice`) ensures that the
    /// 2-of-4 subset is structurally fixed: a future variant joining
    /// the template-substitution surface extends both enums AND this
    /// method's match arm together, with rustc binding the extension
    /// through the projection's `Option` return type.
    #[must_use]
    pub fn as_unquote_form(self) -> Option<UnquoteForm> {
        match self {
            Self::Unquote => Some(UnquoteForm::Unquote),
            Self::UnquoteSplice => Some(UnquoteForm::Splice),
            Self::Quote | Self::Quasiquote => None,
        }
    }

    /// Canonical iac-forge interop tag — the symbol head the canonical
    /// 2-element-list encoding of a quote-family wrapper uses when
    /// projecting `tatara_lisp::Sexp` into `iac_forge::sexpr::SExpr`:
    /// `"quote"` for [`Self::Quote`], `"quasiquote"` for
    /// [`Self::Quasiquote`], `"unquote"` for [`Self::Unquote`],
    /// `"unquote-splicing"` for [`Self::UnquoteSplice`].
    ///
    /// The mapping is Common-Lisp-canonical: a `,@x` form encodes as
    /// `(unquote-splicing x)` rather than `(unquote-splice x)`. That
    /// tag-string choice is INTENTIONALLY DISTINCT from the substrate's
    /// shorter diagnostic label projected by
    /// [`crate::error::SexpShape::label`] (which renders
    /// `[`Self::UnquoteSplice`]` as `"unquote-splice"` — the shorter
    /// idiom appropriate for `expected …, got unquote-splice` error
    /// surfaces). The two projections key the SAME closed set on TWO
    /// distinct boundaries:
    ///
    /// * `iac_forge_tag` — cross-crate canonical form, BLAKE3 attestation
    ///   keys, render-cache shape (load-bearing for byte-identical
    ///   inter-crate compatibility with the iac-forge ecosystem).
    /// * `SexpShape::label` — operator-facing diagnostic label,
    ///   `LispError::TypeMismatch.got` rendering, REPL/LSP
    ///   shape-of-witness surface.
    ///
    /// Pre-lift the four canonical iac-forge tag strings lived inline
    /// across four arms in [`crate::interop`]'s
    /// `From<&Sexp> for iac_forge::sexpr::SExpr` impl, paired with the
    /// matching `Sexp::{Quote, Quasiquote, Unquote, UnquoteSplice}`
    /// patterns. The pairing was load-bearing yet only enforced by
    /// callsite discipline at a FOURTH consumer site (alongside `Hash`,
    /// `Display`, and `Sexp::as_unquote`) the prior closed-set
    /// `QuoteForm` lift did not reach (the `iac-forge` feature gate
    /// kept that site's drift risk silent in the default build). After
    /// this lift the interop arms collapse to ONE arm routing through
    /// [`crate::ast::Sexp::as_quote_form`] + this method, so the
    /// (Sexp variant, canonical tag string) pairing binds at ONE site
    /// on the substrate algebra regardless of which consumer surface
    /// (`Hash`, `Display`, `Sexp::as_unquote`, iac-forge interop)
    /// needs it.
    ///
    /// The `&'static str` lifetime is load-bearing: every iac-forge
    /// consumer projects through this method into the canonical
    /// 2-element-list head without an allocation, parallel to how
    /// [`Self::prefix`], [`UnquoteForm::marker`], and
    /// [`crate::error::SexpShape::label`] project their respective
    /// closed-set surfaces. A future homoiconic prefix-wrapper (e.g.
    /// hypothetical `,~` reverse-unquote) extends [`Self`] AND this
    /// method's match arm together — rustc binds the iac-forge
    /// canonical-form surface to the algebra through exhaustiveness.
    ///
    /// Theory anchor: THEORY.md §V.1 — knowable platform; the
    /// quote-family canonical-form tag set becomes a TYPE projection
    /// on the substrate algebra rather than four `&'static str`
    /// literals scattered across the `interop` arms (parallel to how
    /// `Self::prefix` lifts the Display↔reader prefix and
    /// `Self::hash_discriminator` lifts the cache-key bytes).
    /// THEORY.md §VI.1 — generation over composition; the (Sexp
    /// variant, iac-forge tag) pairing appeared at the four
    /// `interop.rs` arms — past the ≥2 PRIME-DIRECTIVE trigger once
    /// the structural shape is named. THEORY.md §II.1 invariant 1 —
    /// typed entry; the cross-crate canonical-form projection IS the
    /// typed-exit gate at the iac-forge boundary, and naming its
    /// closed-set tag identity lifts the gate from per-site literal
    /// discipline to ONE method the iac-forge round-trip discipline
    /// binds against.
    #[must_use]
    pub fn iac_forge_tag(self) -> &'static str {
        match self {
            Self::Quote => "quote",
            Self::Quasiquote => "quasiquote",
            Self::Unquote => "unquote",
            Self::UnquoteSplice => "unquote-splicing",
        }
    }

    /// Project the typed marker back into its matching `Sexp::*` wrapper
    /// variant applied to `inner` — the structural inverse of
    /// [`crate::ast::Sexp::as_quote_form`]. [`Self::Quote`] yields
    /// [`Sexp::Quote`], [`Self::Quasiquote`] yields [`Sexp::Quasiquote`],
    /// [`Self::Unquote`] yields [`Sexp::Unquote`], [`Self::UnquoteSplice`]
    /// yields [`Sexp::UnquoteSplice`], each boxing `inner` into the
    /// corresponding tuple-variant constructor (`fn(Box<Sexp>) -> Sexp`).
    ///
    /// Round-trip identity with [`crate::ast::Sexp::as_quote_form`] — the
    /// structural law every consumer can pin against:
    ///
    /// ```ignore
    /// // for every (qf, inner): qf.wrap(inner.clone()).as_quote_form() == Some((qf, &inner))
    /// // for every Sexp s matching the quote family:
    /// //     let (qf, inner) = s.as_quote_form().unwrap();
    /// //     qf.wrap(inner.clone()) == s
    /// ```
    ///
    /// Consumer: [`crate::reader::read_quoted`] — the FIFTH consumer site
    /// of the closed-set `QuoteForm` algebra (sibling to `Hash for Sexp`'s
    /// `hash_discriminator` arm, `Display for Sexp`'s `prefix` arm,
    /// `Sexp::as_unquote`'s `as_unquote_form` subset-gate composition, and
    /// the feature-gated `From<&Sexp> for iac_forge::SExpr`'s
    /// `iac_forge_tag` arm). Pre-lift the reader's parse dispatch carried
    /// its own parallel closed set: a local `Token::{Quote, Quasiquote,
    /// Unquote, UnquoteSplice}` enum paired with the matching `Sexp::*`
    /// tuple-variant constructors threaded as `fn(Box<Sexp>) -> Sexp`
    /// arguments to `read_quoted`. The (Token variant, Sexp::* constructor)
    /// pairing was load-bearing yet only enforced by callsite discipline
    /// at the FIFTH consumer site the prior `QuoteForm` lifts did not
    /// reach — a regression that swapped `Sexp::Quote` and
    /// `Sexp::Quasiquote` between the parser arms type-checked but
    /// silently corrupted every program's quote-family parse.
    ///
    /// Post-lift the reader's `Token` collapses to ONE typed variant
    /// `Token::Quoted(QuoteForm)`, the parser's four prefix arms collapse
    /// to ONE arm `Some((Token::Quoted(qf), _)) => read_quoted(it,
    /// eof_pos, qf)`, and `read_quoted` routes through this projection to
    /// produce the matching `Sexp::*` variant. The (QuoteForm variant,
    /// Sexp::* constructor) pairing now binds at ONE site on the typed
    /// algebra — rustc enforces exhaustiveness across [`Self`]'s closed
    /// set, so a regression that drifts the (marker, constructor) pair
    /// becomes a typed compile error rather than a silent program-text
    /// corruption.
    ///
    /// The `Sexp` (owned) return type complements [`Sexp::as_quote_form`]'s
    /// `&Sexp` (borrowed) — `wrap` consumes the inner body to build the
    /// new wrapper, `as_quote_form` borrows the inner body from the
    /// existing wrapper. The asymmetry is intentional: at the reader's
    /// parse-then-wrap boundary the inner is fresh from `parse(...)?` and
    /// has no caller-owned binding; the typed `Box::new(inner)` allocation
    /// lives at ONE site rather than four (one per pre-lift parser arm),
    /// so a future allocation-policy change (e.g. arena-allocated wrappers
    /// for span-aware Sexp) lands as ONE edit.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
    /// reader's prefix-token → Sexp-wrapper gate IS the rust-level
    /// typed-entry gate at the source-text boundary, and naming the
    /// typed projection from [`QuoteForm`] back to the `Sexp::*` wrapper
    /// lifts the gate from per-arm constructor literals to ONE method
    /// the closed-set algebra owns — parallel to how [`Self::prefix`]
    /// lifts the Display↔reader prefix-string surface. THEORY.md §II.1
    /// invariant 2 — free middle; ALL FIVE consumers (Hash, Display,
    /// as_unquote, iac-forge interop, reader's parse) now route through
    /// the SAME closed-set algebra so a regression that drifts ONE
    /// consumer's pairing from the others cannot reach the substrate's
    /// runtime. THEORY.md §V.1 — knowable platform; the (QuoteForm
    /// variant, Sexp::* constructor) pairing becomes a TYPE projection on
    /// the substrate algebra rather than four `fn(Box<Sexp>) -> Sexp`
    /// function pointers threaded as call arguments. A typo or
    /// swap is no longer a runtime drift but a compile error against the
    /// typed projection. THEORY.md §VI.1 — generation over composition;
    /// the (QuoteForm variant, Sexp::* constructor) pairing appeared at
    /// the four reader arms — past the ≥2 PRIME-DIRECTIVE trigger once
    /// the structural shape is named. The typed projection lands the
    /// structural-completeness floor for the reader's quote-family
    /// surface, completing the FIVE-consumer closure of the
    /// `QuoteForm` algebra.
    #[must_use]
    pub fn wrap(self, inner: Sexp) -> Sexp {
        let boxed = Box::new(inner);
        match self {
            Self::Quote => Sexp::Quote(boxed),
            Self::Quasiquote => Sexp::Quasiquote(boxed),
            Self::Unquote => Sexp::Unquote(boxed),
            Self::UnquoteSplice => Sexp::UnquoteSplice(boxed),
        }
    }
}

/// Iterate over the argument tails of every form in `forms` whose call head
/// matches `keyword` — the *slice-side* sibling of [`Sexp::as_call_to`].
/// Where [`Sexp::as_call_to`] answers "is THIS form a call to `K`, and what
/// are its arguments?" on ONE form, `iter_calls_to` answers "which forms
/// in this SLICE are calls to `K`, and what are their arguments?" on a
/// `&[Sexp]`. Yields `&[Sexp]` for each matching form's argument tail
/// (`&form_list[1..]`, the empty slice for a singleton call like `(K)`);
/// non-matching forms — every shape [`Sexp::as_call_to`] rejects — are
/// skipped silently, matching the soft-projection posture the per-form
/// sibling carries.
///
/// Two consumers in [`compile.rs`](crate::compile) route through this
/// primitive:
///   * [`compile_typed::<T>`](crate::compile::compile_typed) — walks every
///     expanded top-level form and compiles every `(T::KEYWORD :k v …)`
///     form into a typed `T`.
///   * [`compile_named_from_forms::<T>`](crate::compile::compile_named_from_forms)
///     — walks every expanded form and compiles every
///     `(T::KEYWORD NAME :k v …)` form into a [`NamedDefinition<T>`](crate::compile::NamedDefinition).
///
/// Before this lift both consumers opened the same `for form in &expanded
/// { if let Some(args) = form.as_call_to(T::KEYWORD) { … } }` walk inline
/// — well past the ≥2 PRIME-DIRECTIVE trigger once the per-form sibling
/// had a name. After this lift the walk lives in ONE function the two
/// dispatchers route through; a regression that drifts ONE consumer's
/// walk from the other (a future emitter that inlines a partial filter,
/// a debug-mode logger that loses track of non-matching forms, a span-
/// aware walk that threads a borrowed `&Sexp` position alongside the
/// tail) becomes structurally impossible because there is exactly ONE
/// implementation both dispatchers consume. A future authoring tool
/// (LSP / REPL / `tatara-check`) that wants to surface "which forms in
/// this program invoke `K`?" binds to ONE function on the slice algebra
/// instead of re-deriving the walk per consumer.
///
/// Closes the soft-dispatch family at the slice level: the per-form
/// projections `{head_symbol, as_call, as_call_to, as_call_to_any}` each
/// answer "what does THIS form's head say?", and the slice-side
/// `iter_calls_to` extends them to "what do THESE forms' heads say,
/// projected through one keyword?". The closed-form composition binding
/// the slice-side projection to its per-form sibling is the structural
/// identity every consumer can pin against:
///
/// ```ignore
/// iter_calls_to(forms, k) == forms.iter().filter_map(|f| f.as_call_to(k))
/// ```
///
/// The yielded `&[Sexp]` slices borrow `&forms[i][1..]` verbatim — no
/// copy, no allocation, same lifetime as [`Sexp::as_call_to`]'s tail.
/// The iterator's lifetime `'a` is the unified outer lifetime of `forms`
/// AND `keyword`: the keyword string must outlive the iterator's borrow
/// of the slice (typical caller passes `T::KEYWORD: &'static str`, which
/// unifies trivially; a caller passing a locally-allocated `&str` ties
/// the iterator to that local). The closure captures `keyword` by move
/// (the `move` keyword on the `filter_map` closure), so each invocation
/// re-derives the head comparison via [`Sexp::as_call_to`]'s `head ==
/// keyword` check at every form — no shared-state, fully Iterator-fused.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// two-site `for + as_call_to` inline walk is past the ≥2 PRIME-DIRECTIVE
/// trigger once the per-form sibling has a name. THEORY.md §V.1 —
/// knowable platform / "make invalid states unrepresentable"; the
/// slice-side projection becomes a NAMED primitive on the substrate's
/// `&[Sexp]` algebra rather than a re-derived for-loop at every consumer
/// site, so authoring tools (REPL, LSP, `tatara-check`) bind to ONE
/// function instead of re-implementing the walk. THEORY.md §II.1
/// invariant 1 — typed entry; the typed-keyword filter on a slice IS the
/// rust-level typed-entry-batch gate (the batch sibling of `as_call_to`'s
/// per-form gate), and naming its single shape lifts the gate from
/// two-site duplication to one rust function the substrate's diagnostic
/// promotions hang off of. THEORY.md §II.1 invariant 2 — free middle;
/// both dispatchers route through the SAME projection, so a regression
/// that drifts one consumer's walk from the other cannot reach the
/// substrate's runtime: the type system binds every consumer to the
/// projection's single emission shape.
///
/// Frontier inspiration: MLIR's `op.getOps<NamedOp>()` — every rewrite
/// pattern over a typed-op block binds to ONE typed-filter iterator
/// regardless of whether it's matching one op kind or batching across a
/// region's contents; the substrate's `iter_calls_to` is the
/// unstructured-projection peer of that iterator, lifted onto the
/// substrate's typed `&[Sexp]` algebra. Racket's `syntax-parse`
/// `~seq (defmacro id args …) …` ellipsis-form — the slice-level
/// matched-keyword filter is the closed-form sibling of `~seq`'s
/// repeated-pattern matcher, translated through pleme-io primitives as
/// ONE `iter_calls_to(forms, keyword)` projection. Tree-sitter's
/// `Query::matches` over a node sequence — the same "iterate the
/// matched forms in a parent" projection, inherited here for the typed
/// `Sexp` algebra without a new IR layer.
pub fn iter_calls_to<'a>(
    forms: &'a [Sexp],
    keyword: &'a str,
) -> impl Iterator<Item = &'a [Sexp]> + 'a {
    forms.iter().filter_map(move |f| f.as_call_to(keyword))
}

/// Render an `Atom::Float`'s `f64` value to a form that re-reads as
/// `Atom::Float` — preserves the float-vs-int typed identity across the
/// `Sexp::Display` → [`crate::reader::read`] round-trip.
///
/// Rust's stdlib `Display` impl for `f64` elides the trailing `.0` for
/// finite integral values: `format!("{}", 1.0_f64) == "1"`,
/// `format!("{}", 100.0_f64) == "100"`. The substrate's reader
/// ([`crate::reader::atom_from_str`]) tries `i64::parse` BEFORE
/// `f64::parse`, so a bare `1` re-reads as `Atom::Int(1)` — NOT as
/// `Atom::Float(1.0)`. The default Display rendering therefore drifts the
/// typed identity at the Display→read boundary: `Float(1.0)` round-trips
/// to `Int(1)` and a regression silently coerces an authoring-surface
/// `1.0` slot into the typed `Int` track.
///
/// This helper emits `1.0` for `1.0_f64` and `1.5` for `1.5_f64` — the
/// `.0` suffix is appended IFF the value is finite AND already integral
/// (`n == n.trunc()`). Non-integral values render through the default
/// `f64` Display impl, which already preserves the fractional component
/// (`1.5`, `0.99`, etc.) round-trippably. Non-finite values (`NaN`,
/// `inf`, `-inf`) also fall through to the default impl — they cannot be
/// reliably round-tripped through the reader regardless (the Hash impl
/// already warns about NaN's PartialEq irregularity at the cache-key
/// boundary), so the helper does not paper over that prior limitation.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 — typed entry; the
/// substrate's typed-entry gate distinguishes `Atom::Int` from
/// `Atom::Float`, and the Display→read round-trip is the typed-exit-side
/// mirror that must preserve the distinction. Pre-lift the
/// `Float(integral) → Int(integral)` collapse silently violated the
/// invariant at the round-trip boundary; post-lift the typed identity is
/// preserved. THEORY.md §V.1 — knowable platform; diagnostics that
/// project a `Float(1.0)` slot through `SexpWitness::display` (sourced
/// from `Sexp::to_string()`) used to surface as `got 1` — confusingly
/// identical to the typed `Int(1)` projection. Post-lift the diagnostic
/// shape names the offender's typed identity (`got 1.0`) so operators
/// distinguish "you wrote 1.0 in an int slot" from "you wrote 1 in a
/// kwarg slot the kwarg gate rejected" without re-reading source.
fn fmt_float(n: f64, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if n.is_finite() && n == n.trunc() {
        write!(f, "{n}.0")
    } else {
        write!(f, "{n}")
    }
}

impl fmt::Display for Sexp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => f.write_str("()"),
            Self::Atom(a) => match a {
                Atom::Symbol(s) => f.write_str(s),
                Atom::Keyword(s) => write!(f, ":{s}"),
                Atom::Str(s) => write!(f, "{s:?}"),
                Atom::Int(n) => write!(f, "{n}"),
                Atom::Float(n) => fmt_float(*n, f),
                Atom::Bool(true) => f.write_str("#t"),
                Atom::Bool(false) => f.write_str("#f"),
            },
            Self::List(xs) => {
                f.write_str("(")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{x}")?;
                }
                f.write_str(")")
            }
            // The four quote-family variants share the
            // `write!(f, "<prefix>{inner}")` Display shape — all route
            // through `as_quote_form`'s typed-marker projection so the
            // per-variant prefix (`'`, `` ` ``, `,`, `,@`) binds at ONE
            // site on the closed-set `QuoteForm` algebra and the
            // recursive `inner` rendering composes through the unified
            // Display arm. The (prefix, variant) pairing IS the structural
            // dual of the reader's `read_quoted` (prefix, variant-ctor)
            // dispatch — naming it once threads the round-trip discipline
            // through ONE rust function the reader and the Display impl
            // both bind against.
            Self::Quote(_) | Self::Quasiquote(_) | Self::Unquote(_) | Self::UnquoteSplice(_) => {
                let (qf, inner) = self
                    .as_quote_form()
                    .expect("matched quote-family variant must project to Some via as_quote_form");
                write!(f, "{}{inner}", qf.prefix())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── head_symbol: the operator-position projection ───────────────────
    //
    // `head_symbol` lifts the `self.as_list()?.first()?.as_symbol()` chain
    // that recurred at four soft-dispatch sites (compile.rs `compile_typed`
    // + `compile_named_from_forms`, macro_expand.rs `Expander::expand` +
    // `macro_def_from`) into ONE named query on the Sexp algebra. These
    // tests pin its contract directly; the existing dispatch tests in
    // compile.rs / macro_expand.rs are the path-uniformity guards proving
    // the four sites route through it without behavior drift.

    #[test]
    fn head_symbol_returns_operator_for_list_form() {
        // `(defpoint obs :class x)` — the operator is the head symbol.
        let form = Sexp::List(vec![
            Sexp::symbol("defpoint"),
            Sexp::symbol("obs"),
            Sexp::keyword("class"),
            Sexp::symbol("x"),
        ]);
        assert_eq!(form.head_symbol(), Some("defpoint"));
    }

    #[test]
    fn head_symbol_none_for_non_list_shapes() {
        // A bare atom is not an invocation — there is no operator position.
        assert_eq!(Sexp::symbol("foo").head_symbol(), None);
        assert_eq!(Sexp::int(5).head_symbol(), None);
        assert_eq!(Sexp::keyword("k").head_symbol(), None);
        assert_eq!(Sexp::string("s").head_symbol(), None);
        assert_eq!(Sexp::boolean(true).head_symbol(), None);
        assert_eq!(Sexp::float(1.5).head_symbol(), None);
        assert_eq!(Sexp::Nil.head_symbol(), None);
        // Quote-family wrappers are not lists at the outer layer either.
        assert_eq!(Sexp::Quote(Box::new(Sexp::symbol("x"))).head_symbol(), None);
    }

    #[test]
    fn head_symbol_none_for_empty_list() {
        // `()` has no first element to read an operator from.
        assert_eq!(Sexp::List(vec![]).head_symbol(), None);
    }

    #[test]
    fn head_symbol_none_for_non_symbol_head() {
        // A list whose head is present but not a symbol is not a dispatchable
        // invocation — the soft projection yields None (the STRICT sibling
        // `compile_from_sexp` is the one that rejects these loudly).
        assert_eq!(
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]).head_symbol(),
            None
        );
        assert_eq!(
            Sexp::List(vec![Sexp::keyword("kw"), Sexp::symbol("a")]).head_symbol(),
            None
        );
        assert_eq!(
            Sexp::List(vec![Sexp::string("s"), Sexp::symbol("a")]).head_symbol(),
            None
        );
        assert_eq!(
            Sexp::List(vec![
                Sexp::List(vec![Sexp::symbol("nested")]),
                Sexp::symbol("a")
            ])
            .head_symbol(),
            None
        );
        assert_eq!(
            Sexp::List(vec![Sexp::Nil, Sexp::symbol("a")]).head_symbol(),
            None
        );
    }

    #[test]
    fn head_symbol_reads_singleton_list_operator() {
        // `(defcompiler)` — a keyword-only form still has an operator head;
        // this is exactly the arity-gate input compile_named dispatches on
        // before rejecting the missing NAME.
        assert_eq!(
            Sexp::List(vec![Sexp::symbol("defcompiler")]).head_symbol(),
            Some("defcompiler")
        );
    }

    #[test]
    fn head_symbol_borrows_the_actual_head_string() {
        // The returned &str borrows the head atom's contents verbatim — no
        // copy, no normalization. Pin that a multi-segment symbol round-trips
        // unchanged so the dispatch comparison against `T::KEYWORD` is exact.
        let form = Sexp::List(vec![Sexp::symbol("defalert-policy"), Sexp::symbol("p")]);
        assert_eq!(form.head_symbol(), Some("defalert-policy"));
    }

    // ── as_call: the call-form decomposition ────────────────────────────
    //
    // `as_call` pairs `head_symbol` (the operator projection) with the
    // argument tail every dispatch site reads right after matching the
    // operator — `Some((op, &args))` for a symbol-headed list, `None` for
    // everything else. It lifts the `as_list()`-for-the-tail +
    // `head_symbol()`-for-the-operator pairing that recurred at the three
    // soft-dispatch sites (compile.rs `compile_typed` + `compile_named_
    // from_forms`, macro_expand.rs `Expander::expand`) into ONE match.
    // `head_symbol` now delegates to it, so the `as_list()?.first()?.
    // as_symbol()` chain lives in exactly one place. These tests pin the
    // decomposition's contract directly; the existing dispatch tests in
    // compile.rs / macro_expand.rs are the path-uniformity guards proving
    // the three sites route through it without behavior drift.

    #[test]
    fn as_call_decomposes_list_form_into_operator_and_args() {
        // `(defpoint obs :class x)` — the operator is the head symbol and
        // the args are everything after it.
        let args = [
            Sexp::symbol("obs"),
            Sexp::keyword("class"),
            Sexp::symbol("x"),
        ];
        let form = Sexp::List(
            std::iter::once(Sexp::symbol("defpoint"))
                .chain(args.iter().cloned())
                .collect(),
        );
        assert_eq!(form.as_call(), Some(("defpoint", &args[..])));
    }

    #[test]
    fn as_call_none_for_non_call_shapes() {
        // Every shape `head_symbol` rejects, `as_call` rejects identically:
        // non-lists, the empty list, and non-symbol heads have no operator
        // to apply, hence no call decomposition.
        assert_eq!(Sexp::symbol("foo").as_call(), None);
        assert_eq!(Sexp::int(5).as_call(), None);
        assert_eq!(Sexp::keyword("k").as_call(), None);
        assert_eq!(Sexp::string("s").as_call(), None);
        assert_eq!(Sexp::Nil.as_call(), None);
        assert_eq!(Sexp::Quote(Box::new(Sexp::symbol("x"))).as_call(), None);
        assert_eq!(Sexp::List(vec![]).as_call(), None);
        assert_eq!(
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]).as_call(),
            None
        );
        assert_eq!(
            Sexp::List(vec![Sexp::keyword("kw"), Sexp::symbol("a")]).as_call(),
            None
        );
    }

    #[test]
    fn as_call_yields_empty_args_for_singleton_list() {
        // `(defcompiler)` — a keyword-only form decomposes to its operator
        // with an EMPTY argument tail. This is exactly the arity-gate input
        // `compile_named_from_forms` dispatches on before rejecting the
        // missing NAME via `rest.split_first()` returning `None`.
        assert_eq!(
            Sexp::List(vec![Sexp::symbol("defcompiler")]).as_call(),
            Some(("defcompiler", &[][..]))
        );
    }

    #[test]
    fn as_call_args_are_exactly_the_tail_after_the_operator() {
        // The args slice borrows `&list[1..]` verbatim — the head is
        // excluded, every following element is included in order.
        let form = Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::symbol("cpu"),
            Sexp::keyword("threshold"),
            Sexp::int(90),
        ]);
        let (op, args) = form.as_call().expect("symbol-headed list decomposes");
        assert_eq!(op, "defmonitor");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], Sexp::symbol("cpu"));
        assert_eq!(args[2], Sexp::int(90));
    }

    #[test]
    fn head_symbol_is_the_operator_projection_of_as_call() {
        // The structural relationship the lift establishes: `head_symbol`
        // is `as_call().map(|(h, _)| h)`. Pin it across every shape so a
        // regression that drifts one query's head-recognition from the
        // other — e.g. `as_call` accepting a keyword head that `head_symbol`
        // still rejects — fails loudly. The two share ONE chain.
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::symbol("defpoint"), Sexp::symbol("p")]),
            Sexp::List(vec![Sexp::symbol("solo")]),
        ];
        for s in &shapes {
            assert_eq!(
                s.head_symbol(),
                s.as_call().map(|(h, _)| h),
                "head_symbol must equal the operator component of as_call for {s}"
            );
        }
    }

    // ── as_call_to: the keyword-typed call decomposition ────────────────
    //
    // `as_call_to(keyword)` answers "is this a call to ONE specific
    // operator, and what are its arguments?" — the keyword-aware sibling
    // of `as_call`. It lifts the `as_call() + head == T::KEYWORD` two-step
    // chain that recurred at the two `compile.rs` dispatch sites
    // (`compile_typed` and `compile_named_from_forms`) into ONE structural
    // query on the Sexp algebra. The tests below pin its contract
    // directly; the existing `compile_*` tests are the path-uniformity
    // guards proving the two production sites route through it without
    // behavior drift.

    #[test]
    fn as_call_to_returns_args_for_matching_head() {
        // `(defmonitor :name "x")` — head is the exact symbol `defmonitor`,
        // so `as_call_to("defmonitor")` returns `Some(args)` with the tail
        // after the head verbatim.
        let form = Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::keyword("name"),
            Sexp::string("x"),
        ]);
        let args = form
            .as_call_to("defmonitor")
            .expect("matching head must yield Some(args)");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], Sexp::keyword("name"));
        assert_eq!(args[1], Sexp::string("x"));
    }

    #[test]
    fn as_call_to_returns_none_for_mismatched_head() {
        // `(defmonitor …)` against keyword `"defpoint"` — same form is a
        // call (so `as_call().is_some()`), but the head doesn't equal the
        // requested keyword. `as_call_to` is the keyword-typed projection,
        // so it yields `None` exactly when the head doesn't match. Pin the
        // gate: the two pre-lift inline sites both rejected this case via
        // `if head != T::KEYWORD { continue }` / `if head == T::KEYWORD`,
        // and the lifted primitive must reject identically.
        let form = Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::keyword("name"),
            Sexp::string("x"),
        ]);
        assert!(form.as_call().is_some());
        assert_eq!(form.as_call_to("defpoint"), None);
        assert_eq!(form.as_call_to(""), None);
        assert_eq!(form.as_call_to("DEFMONITOR"), None);
    }

    #[test]
    fn as_call_to_yields_empty_args_for_singleton_matching_call() {
        // `(defcompiler)` against keyword `"defcompiler"` — the head
        // matches and the argument tail is the empty slice. Pin the
        // empty-tail posture: this is exactly the input
        // `compile_named_from_forms` dispatches on before rejecting the
        // missing NAME via `rest.split_first()` returning `None`, so the
        // lifted primitive must yield `Some(&[])` here (NOT `None`) so
        // the downstream split-first gate fires structurally.
        let form = Sexp::List(vec![Sexp::symbol("defcompiler")]);
        assert_eq!(form.as_call_to("defcompiler"), Some(&[][..]));
    }

    #[test]
    fn as_call_to_returns_none_for_non_call_shapes() {
        // Every shape `as_call` rejects, `as_call_to` rejects identically
        // regardless of the requested keyword: non-lists, the empty list,
        // and non-symbol heads have no operator to compare to. Pin
        // path-uniformity with the `as_call` sibling so a regression that
        // narrows the keyword-typed projection to admit a shape the bare
        // soft projection rejected (e.g. accepting a keyword head when
        // `keyword` matches the keyword's symbol-string projection) fails
        // here.
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::boolean(true),
            Sexp::float(1.5),
            Sexp::Nil,
            Sexp::Quote(Box::new(Sexp::symbol("foo"))),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::keyword("foo"), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::string("foo"), Sexp::symbol("a")]),
        ];
        for s in &shapes {
            assert_eq!(
                s.as_call_to("foo"),
                None,
                "non-call shape must yield None for any keyword, got Some for {s}"
            );
            assert_eq!(s.as_call_to("anything"), None);
        }
    }

    #[test]
    fn as_call_to_args_borrow_is_same_pointer_as_as_call_tail() {
        // The structural identity binding `as_call_to` to its `as_call`
        // sibling: on the matching-head path, the returned `args` slice IS
        // the same `&[Sexp]` slice `as_call` would return as the tail
        // component. Pin pointer equality so a regression that
        // re-allocates or copies the tail in the keyword-typed projection
        // fails loudly — the soft-projection contract is borrow, not
        // clone, AND `as_call_to` inherits the contract verbatim from
        // `as_call`.
        let form = Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::keyword("name"),
            Sexp::string("x"),
        ]);
        let (_, via_as_call) = form.as_call().expect("call shape");
        let via_as_call_to = form
            .as_call_to("defmonitor")
            .expect("matching keyword shape");
        assert!(
            std::ptr::eq(via_as_call.as_ptr(), via_as_call_to.as_ptr()),
            "as_call_to args must borrow the SAME slice as as_call's tail"
        );
        assert_eq!(via_as_call.len(), via_as_call_to.len());
    }

    #[test]
    fn as_call_to_is_the_keyword_typed_projection_of_as_call() {
        // The structural identity the lift establishes:
        //   `as_call_to(k) == as_call().and_then(|(h, args)| (h == k).then_some(args))`
        //   `as_call_to(k).is_some() == (head_symbol() == Some(k))`
        // Pin both across every shape so a regression that drifts the
        // keyword-typed projection from its closed-form definition fails
        // loudly. The three soft-projection primitives — `head_symbol`,
        // `as_call`, `as_call_to` — must agree on operator-position
        // recognition at every shape they share.
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::symbol("defpoint"), Sexp::symbol("p")]),
            Sexp::List(vec![Sexp::symbol("defmonitor"), Sexp::keyword("name")]),
            Sexp::List(vec![Sexp::symbol("solo")]),
        ];
        for s in &shapes {
            for k in ["defpoint", "defmonitor", "solo", "foo", ""] {
                let via_chain = s.as_call().and_then(|(h, args)| (h == k).then_some(args));
                assert_eq!(
                    s.as_call_to(k),
                    via_chain,
                    "as_call_to({k:?}) must equal as_call+filter for {s}"
                );
                assert_eq!(
                    s.as_call_to(k).is_some(),
                    s.head_symbol() == Some(k),
                    "as_call_to({k:?}).is_some() must equal (head_symbol() == Some({k:?})) for {s}"
                );
            }
        }
    }

    // ── as_call_to_any: the typed-decoded call decomposition ────────────
    //
    // `as_call_to_any(decode)` answers "is this a call whose head decodes
    // through `decode`, and what are its arguments?" — the closure-typed
    // sibling of `as_call_to`. It lifts the
    // `as_list() + head_symbol() + decode(head)` three-step chain that
    // recurred at the macro-expander's `macro_def_from` site (the typed
    // `MacroDefHead::from_keyword` dispatch surface) into ONE structural
    // query on the Sexp algebra. The tests below pin its contract
    // directly; the existing macro-expansion tests are the path-
    // uniformity guards proving the production site routes through it
    // without behavior drift.
    //
    // The test classifier `Op::from_keyword` mirrors `MacroDefHead::from_keyword`
    // — a closed-set typed enum projection from a `&str` head — so the
    // tests cover the macro-expander's real consumer shape rather than a
    // synthetic predicate.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Op {
        Quote,
        If,
        Let,
    }
    impl Op {
        fn from_keyword(head: &str) -> Option<Self> {
            match head {
                "quote" => Some(Self::Quote),
                "if" => Some(Self::If),
                "let" => Some(Self::Let),
                _ => None,
            }
        }
    }

    #[test]
    fn as_call_to_any_returns_decoded_head_and_args_for_matching_head() {
        // `(if c t e)` — head `if` decodes to `Op::If`, args are the
        // three-element tail verbatim. Pin both halves of the returned
        // tuple: the decoded typed witness AND the borrowed args slice.
        let form = Sexp::List(vec![
            Sexp::symbol("if"),
            Sexp::symbol("c"),
            Sexp::symbol("t"),
            Sexp::symbol("e"),
        ]);
        let (op, args) = form
            .as_call_to_any(Op::from_keyword)
            .expect("matching head must yield Some((decoded, args))");
        assert_eq!(op, Op::If);
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], Sexp::symbol("c"));
        assert_eq!(args[2], Sexp::symbol("e"));
    }

    #[test]
    fn as_call_to_any_returns_none_when_decoder_rejects_head() {
        // `(defmonitor :name "x")` — head `defmonitor` is a valid symbol
        // (so `as_call().is_some()`), but `Op::from_keyword` rejects it
        // (it's not one of the closed `{quote, if, let}` set). Pin the
        // gate: `as_call_to_any` yields `None` exactly when the decoder
        // rejects the head, mirroring how the pre-lift inline chain in
        // `macro_def_from` returned `Ok(None)` when
        // `MacroDefHead::from_keyword(head_str)` returned `None`.
        let form = Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::keyword("name"),
            Sexp::string("x"),
        ]);
        assert!(form.as_call().is_some());
        assert!(form.as_call_to_any(Op::from_keyword).is_none());
    }

    #[test]
    fn as_call_to_any_yields_empty_args_for_singleton_decoded_call() {
        // `(quote)` against the classifier — head decodes to `Op::Quote`
        // and the argument tail is the empty slice. Pin the empty-tail
        // posture: a downstream arity gate (analogous to
        // `if list.len() < 4` inside `macro_def_from`) dispatches on
        // `args.is_empty()` AFTER the decoder accepts the head; the
        // helper must yield `Some((decoded, &[]))` (NOT `None`) so that
        // gate fires structurally.
        let form = Sexp::List(vec![Sexp::symbol("quote")]);
        let (op, args) = form
            .as_call_to_any(Op::from_keyword)
            .expect("singleton matching call must decompose");
        assert_eq!(op, Op::Quote);
        assert_eq!(args.len(), 0);
    }

    #[test]
    fn as_call_to_any_returns_none_for_non_call_shapes() {
        // Every shape `as_call` rejects, `as_call_to_any` rejects
        // identically regardless of the decoder: non-lists, the empty
        // list, and non-symbol heads have no operator string to feed
        // the decoder. Pin path-uniformity with the `as_call` sibling so
        // a regression that admits a non-call shape (e.g. accepting a
        // bare symbol via a permissive decoder) fails here. Pass
        // `Some` for every input to prove the call-shape gate fires
        // BEFORE the decoder runs — the decoder cannot rescue a
        // non-call.
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::boolean(true),
            Sexp::float(1.5),
            Sexp::Nil,
            Sexp::Quote(Box::new(Sexp::symbol("foo"))),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::keyword("foo"), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::string("foo"), Sexp::symbol("a")]),
        ];
        for s in &shapes {
            // The promiscuous decoder accepts every &str head, so the
            // only way to see `None` here is if the call-shape gate
            // rejects the shape upstream of the decoder.
            assert_eq!(
                s.as_call_to_any(|h: &str| Some(h.to_string())),
                None,
                "non-call shape must yield None even for a promiscuous decoder, got Some for {s}"
            );
        }
    }

    #[test]
    fn as_call_to_any_args_borrow_is_same_pointer_as_as_call_tail() {
        // The structural identity binding `as_call_to_any` to its
        // `as_call` sibling: on the decoded path, the returned `args`
        // slice IS the same `&[Sexp]` slice `as_call` would return as
        // the tail component. Pin pointer equality so a regression that
        // re-allocates or copies the tail in the typed-decoded
        // projection fails loudly — the soft-projection contract is
        // borrow, not clone, AND `as_call_to_any` inherits the contract
        // verbatim from `as_call`. Parallel to the
        // `as_call_to_args_borrow_is_same_pointer_as_as_call_tail` pin
        // for `as_call_to`.
        let form = Sexp::List(vec![
            Sexp::symbol("if"),
            Sexp::symbol("c"),
            Sexp::symbol("t"),
        ]);
        let (_, via_as_call) = form.as_call().expect("call shape");
        let (_, via_as_call_to_any) = form
            .as_call_to_any(Op::from_keyword)
            .expect("decoded shape");
        assert!(
            std::ptr::eq(via_as_call.as_ptr(), via_as_call_to_any.as_ptr()),
            "as_call_to_any args must borrow the SAME slice as as_call's tail"
        );
        assert_eq!(via_as_call.len(), via_as_call_to_any.len());
    }

    #[test]
    fn as_call_to_any_is_the_decoded_projection_of_as_call() {
        // The structural identity the lift establishes:
        //   `as_call_to_any(decode) == as_call().and_then(|(h, args)| decode(h).map(|d| (d, args)))`
        //   `as_call_to_any(decode).is_some() == as_call().map_or(false, |(h, _)| decode(h).is_some())`
        // Pin both across every shape so a regression that drifts the
        // typed-decoded projection from its closed-form definition fails
        // loudly. The four soft-projection primitives — `head_symbol`,
        // `as_call`, `as_call_to`, `as_call_to_any` — must agree on
        // operator-position recognition at every shape they share.
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::symbol("if"), Sexp::symbol("c")]),
            Sexp::List(vec![Sexp::symbol("quote"), Sexp::symbol("x")]),
            Sexp::List(vec![Sexp::symbol("let"), Sexp::List(vec![])]),
            Sexp::List(vec![Sexp::symbol("defpoint"), Sexp::symbol("p")]),
            Sexp::List(vec![Sexp::symbol("solo")]),
        ];
        for s in &shapes {
            let via_chain = s
                .as_call()
                .and_then(|(h, args)| Op::from_keyword(h).map(|d| (d, args)));
            assert_eq!(
                s.as_call_to_any(Op::from_keyword),
                via_chain,
                "as_call_to_any(Op::from_keyword) must equal as_call+decode for {s}"
            );
        }
    }

    #[test]
    fn as_call_to_any_subsumes_as_call_to_via_unit_decoder() {
        // The closed-form composition `as_call_to(k) == as_call_to_any
        // (|h| (h == k).then_some(())).map(|(_, a)| a)` (modulo the
        // discarded `()` decoded witness). Pin it across every shape ×
        // keyword pair so a regression that drifts the typed-decoded
        // projection from its single-keyword sibling fails loudly. This
        // makes the family closure: `as_call_to` is the trivial-decoder
        // instance of `as_call_to_any`, and naming both lets each
        // consumer pick the projection that fits its call site.
        let shapes = [
            Sexp::symbol("foo"),
            Sexp::Nil,
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::symbol("if"), Sexp::symbol("c")]),
            Sexp::List(vec![Sexp::symbol("defpoint"), Sexp::symbol("p")]),
        ];
        for s in &shapes {
            for k in ["if", "defpoint", "let", "foo", "", "DEFPOINT"] {
                let via_unit_decoder = s
                    .as_call_to_any(|h: &str| (h == k).then_some(()))
                    .map(|(_, args)| args);
                assert_eq!(
                    s.as_call_to(k),
                    via_unit_decoder,
                    "as_call_to({k:?}) must equal as_call_to_any+unit-decoder for {s}"
                );
            }
        }
    }

    // ── iter_calls_to: the slice-side projection of as_call_to ──────────
    //
    // `iter_calls_to(forms, keyword)` lifts the per-form projection
    // `as_call_to` onto a `&[Sexp]`, yielding the args tails of every
    // matching form in source order — the substrate's typed-keyword
    // filter over a batch of forms. The two inline `for form in
    // &expanded { if let Some(args) = form.as_call_to(T::KEYWORD) { … } }`
    // walks at the `compile_typed` + `compile_named_from_forms` dispatch
    // sites (compile.rs) collapse to ONE `iter_calls_to(&expanded,
    // T::KEYWORD)` call. Tests pin the slice-side primitive's contract
    // directly; the existing dispatch tests in compile.rs are the
    // path-uniformity guards proving the two consumers route through it
    // without behavior drift.

    #[test]
    fn iter_calls_to_yields_args_for_every_matching_form_in_slice() {
        // Three forms: two match "defmonitor", one matches "defalert".
        // `iter_calls_to("defmonitor")` yields the two matching args
        // slices in source order — the matched forms' tails verbatim,
        // skipping the non-matching `defalert` form silently.
        let forms = vec![
            Sexp::List(vec![
                Sexp::symbol("defmonitor"),
                Sexp::keyword("name"),
                Sexp::string("a"),
            ]),
            Sexp::List(vec![
                Sexp::symbol("defalert"),
                Sexp::keyword("name"),
                Sexp::string("p"),
            ]),
            Sexp::List(vec![
                Sexp::symbol("defmonitor"),
                Sexp::keyword("name"),
                Sexp::string("b"),
            ]),
        ];
        let args: Vec<&[Sexp]> = iter_calls_to(&forms, "defmonitor").collect();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], &[Sexp::keyword("name"), Sexp::string("a")][..]);
        assert_eq!(args[1], &[Sexp::keyword("name"), Sexp::string("b")][..]);
    }

    #[test]
    fn iter_calls_to_skips_every_non_call_shape_silently() {
        // Every shape `as_call_to` rejects, `iter_calls_to` skips: non-
        // lists (atoms across all 6 atom kinds, Nil, quote-family
        // wrapper), the empty list, and non-symbol-head lists. Pin
        // path-uniformity with the per-form sibling: passing ANY keyword
        // against a slice of non-call shapes yields zero items. Closes
        // the soft-projection posture at the slice level — a regression
        // that admits a non-call shape (e.g. accepting a bare symbol
        // whose name matches the keyword) fails here.
        let forms = vec![
            Sexp::symbol("foo"),
            Sexp::int(5),
            Sexp::keyword("k"),
            Sexp::string("s"),
            Sexp::boolean(true),
            Sexp::float(1.5),
            Sexp::Nil,
            Sexp::Quote(Box::new(Sexp::symbol("foo"))),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::int(5), Sexp::symbol("a")]),
            Sexp::List(vec![Sexp::keyword("foo"), Sexp::symbol("a")]),
        ];
        for k in ["foo", "anything", "", "defpoint"] {
            let args: Vec<&[Sexp]> = iter_calls_to(&forms, k).collect();
            assert!(
                args.is_empty(),
                "non-call slice must yield zero items for keyword {k:?}, got {} items",
                args.len()
            );
        }
    }

    #[test]
    fn iter_calls_to_yields_empty_args_slice_for_singleton_matching_call() {
        // `(defcompiler)` — the head matches and the args tail is the
        // empty slice. Pin the empty-tail posture: `iter_calls_to` must
        // yield `Some(&[])` for the matching singleton (NOT skip it),
        // mirroring `as_call_to`'s contract — the (possibly-empty) args
        // slice on a match, NOT `None` on an empty tail. This is exactly
        // the input `compile_named_from_forms` dispatches on before
        // rejecting the missing NAME via `rest.split_first()`'s `None`.
        let forms = vec![Sexp::List(vec![Sexp::symbol("defcompiler")])];
        let args: Vec<&[Sexp]> = iter_calls_to(&forms, "defcompiler").collect();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0], &[][..]);
    }

    #[test]
    fn iter_calls_to_yields_nothing_for_empty_slice() {
        // An empty forms slice yields zero items regardless of keyword.
        // Pin the slice-side primitive's degenerate boundary: empty in,
        // empty out — the iterator is fused-empty without consulting
        // `as_call_to` at all.
        let forms: Vec<Sexp> = vec![];
        let mut iter = iter_calls_to(&forms, "anything");
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_calls_to_yields_nothing_when_keyword_matches_no_form() {
        // A slice of valid call forms whose heads none match the
        // requested keyword yields zero items. Pin path-uniformity with
        // the per-form sibling: every form's `as_call_to(missing)` is
        // `None`, so the slice-side iterator yields nothing — the filter
        // fires uniformly across the batch.
        let forms = vec![
            Sexp::List(vec![Sexp::symbol("defmonitor"), Sexp::int(1)]),
            Sexp::List(vec![Sexp::symbol("defalert"), Sexp::int(2)]),
            Sexp::List(vec![Sexp::symbol("defpoint"), Sexp::int(3)]),
        ];
        let args: Vec<&[Sexp]> = iter_calls_to(&forms, "missing").collect();
        assert!(args.is_empty());
    }

    #[test]
    fn iter_calls_to_args_borrow_is_same_pointer_as_per_form_as_call_to_tail() {
        // The structural identity binding `iter_calls_to` to its per-form
        // sibling: each yielded `&[Sexp]` IS the same slice `as_call_to`
        // would return as the tail component for the corresponding form
        // (pinned via `std::ptr::eq` on `as_ptr()`). The soft-projection
        // contract is borrow, not clone, AND `iter_calls_to` inherits the
        // contract verbatim from `as_call_to`. Parallel to the
        // `as_call_to_args_borrow_is_same_pointer_as_as_call_tail` pin
        // for `as_call_to`.
        let forms = vec![Sexp::List(vec![
            Sexp::symbol("defmonitor"),
            Sexp::keyword("name"),
            Sexp::string("a"),
        ])];
        let via_iter: &[Sexp] = iter_calls_to(&forms, "defmonitor")
            .next()
            .expect("one match");
        let via_per_form: &[Sexp] = forms[0].as_call_to("defmonitor").expect("one match");
        assert!(
            std::ptr::eq(via_iter.as_ptr(), via_per_form.as_ptr()),
            "iter_calls_to args must borrow the SAME slice as as_call_to's tail"
        );
        assert_eq!(via_iter.len(), via_per_form.len());
    }

    #[test]
    fn iter_calls_to_is_the_slice_side_projection_of_as_call_to() {
        // The structural identity the lift establishes:
        //   `iter_calls_to(forms, k) == forms.iter().filter_map(|f| f.as_call_to(k))`
        // Pin shape AND ordering AND pointer-identity across mixed inputs
        // and a range of keywords (including matching, non-matching, and
        // edge-case empty/case-mismatched keywords) so a regression that
        // drifts the slice-side projection from its closed-form
        // definition fails loudly. The five soft-projection primitives —
        // `head_symbol`, `as_call`, `as_call_to`, `as_call_to_any`, AND
        // `iter_calls_to` — must agree on operator-position recognition
        // at every shape/slice they share.
        let forms = vec![
            Sexp::symbol("foo"),
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(1)]),
            Sexp::Nil,
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(2)]),
            Sexp::int(99),
            Sexp::List(vec![Sexp::symbol("b"), Sexp::int(3)]),
            Sexp::List(vec![Sexp::symbol("a")]),
            Sexp::List(vec![]),
            Sexp::List(vec![Sexp::keyword("a"), Sexp::int(4)]),
        ];
        for k in ["a", "b", "c", "", "A"] {
            let via_iter: Vec<&[Sexp]> = iter_calls_to(&forms, k).collect();
            let via_chain: Vec<&[Sexp]> = forms.iter().filter_map(|f| f.as_call_to(k)).collect();
            assert_eq!(
                via_iter.len(),
                via_chain.len(),
                "len drift for keyword {k:?}"
            );
            for (a, b) in via_iter.iter().zip(via_chain.iter()) {
                assert!(
                    std::ptr::eq(a.as_ptr(), b.as_ptr()),
                    "ptr drift at keyword {k:?}: iter slice does not borrow the SAME tail as the per-form chain"
                );
                assert_eq!(a.len(), b.len(), "len drift at keyword {k:?}");
            }
        }
    }

    // ── as_unquote: the unquote-family projection ───────────────────────
    //
    // `as_unquote` lifts the per-callsite `Sexp::Unquote(inner) /
    // Sexp::UnquoteSplice(inner)` arms paired with their `UnquoteForm::
    // Unquote / UnquoteForm::Splice` literals — three sites pre-lift
    // (`compile_node` 2 arms + `substitute` top-level + `substitute`
    // list-inner) — into ONE typed projection on the `Sexp` algebra.
    // These tests pin its contract; the existing path tests in
    // macro_expand.rs are the path-uniformity guards proving the three
    // sites route through it without behavior drift.

    #[test]
    fn as_unquote_decomposes_unquote_into_typed_marker_and_inner() {
        // `,x` — Sexp::Unquote wrapping a symbol. Pin Some((Unquote, &inner)).
        let inner = Sexp::symbol("x");
        let form = Sexp::Unquote(Box::new(inner.clone()));
        let (marker, body) = form
            .as_unquote()
            .expect("`,x` must project to Some((Unquote, _))");
        assert_eq!(marker, UnquoteForm::Unquote);
        assert_eq!(body, &inner);
    }

    #[test]
    fn as_unquote_decomposes_unquote_splice_into_typed_marker_and_inner() {
        // `,@xs` — Sexp::UnquoteSplice wrapping a symbol. Pin
        // Some((Splice, &inner)). Sibling positive control to the Unquote
        // arm: pins BOTH unquote-family variants project to their typed
        // closed-set UnquoteForm pair through ONE projection function.
        let inner = Sexp::symbol("xs");
        let form = Sexp::UnquoteSplice(Box::new(inner.clone()));
        let (marker, body) = form
            .as_unquote()
            .expect("`,@xs` must project to Some((Splice, _))");
        assert_eq!(marker, UnquoteForm::Splice);
        assert_eq!(body, &inner);
    }

    #[test]
    fn as_unquote_none_for_non_unquote_shapes() {
        // Every Sexp shape OUTSIDE the unquote family — atoms, lists, nil,
        // and the OTHER quote-family variants (Quote `'x`, Quasiquote ``x`) —
        // yields None. Pins the projection's exhaustive negative coverage:
        // a regression that drifts the matched-variant set (e.g. a future
        // emitter that projects `'x` into Some((Unquote, _))) would fail
        // here, even before any downstream dispatcher tests fire.
        assert_eq!(Sexp::symbol("foo").as_unquote(), None);
        assert_eq!(Sexp::int(5).as_unquote(), None);
        assert_eq!(Sexp::keyword("k").as_unquote(), None);
        assert_eq!(Sexp::string("s").as_unquote(), None);
        assert_eq!(Sexp::boolean(true).as_unquote(), None);
        assert_eq!(Sexp::float(1.5).as_unquote(), None);
        assert_eq!(Sexp::Nil.as_unquote(), None);
        assert_eq!(Sexp::List(vec![]).as_unquote(), None);
        assert_eq!(
            Sexp::List(vec![Sexp::symbol("a"), Sexp::int(1)]).as_unquote(),
            None
        );
        // `'x` — Quote-family but NOT unquote-family. The closed-set
        // UnquoteForm projection covers only `,` and `,@`; `'` and `` ` ``
        // are siblings that this projection does NOT match.
        assert_eq!(Sexp::Quote(Box::new(Sexp::symbol("x"))).as_unquote(), None);
        assert_eq!(
            Sexp::Quasiquote(Box::new(Sexp::symbol("x"))).as_unquote(),
            None
        );
    }

    #[test]
    fn as_unquote_is_some_iff_matches_unquote_family() {
        // Structural identity: as_unquote().is_some() agrees with the
        // pre-lift `matches!(self, Sexp::Unquote(_) | Sexp::UnquoteSplice(_))`
        // discriminant across the closed Sexp variant set. Sweep every
        // representative Sexp shape and pin equality of the two discriminants
        // — a regression that drifts ONE shape's projection (e.g. adds
        // Quasiquote to the matched set) becomes a typed test failure.
        let shapes: Vec<(&str, Sexp, bool)> = vec![
            ("nil", Sexp::Nil, false),
            ("symbol", Sexp::symbol("x"), false),
            ("keyword", Sexp::keyword("k"), false),
            ("string", Sexp::string("s"), false),
            ("int", Sexp::int(7), false),
            ("float", Sexp::float(2.5), false),
            ("bool", Sexp::boolean(true), false),
            ("empty list", Sexp::List(vec![]), false),
            (
                "non-empty list",
                Sexp::List(vec![Sexp::symbol("op")]),
                false,
            ),
            ("quote", Sexp::Quote(Box::new(Sexp::symbol("x"))), false),
            (
                "quasiquote",
                Sexp::Quasiquote(Box::new(Sexp::symbol("x"))),
                false,
            ),
            ("unquote", Sexp::Unquote(Box::new(Sexp::symbol("x"))), true),
            (
                "unquote-splice",
                Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
                true,
            ),
        ];
        for (label, sexp, expect_some) in &shapes {
            let via_proj = sexp.as_unquote().is_some();
            let via_pat = matches!(sexp, Sexp::Unquote(_) | Sexp::UnquoteSplice(_));
            assert_eq!(
                via_proj, *expect_some,
                "as_unquote().is_some() drifted from expected at {label}"
            );
            assert_eq!(
                via_proj, via_pat,
                "as_unquote().is_some() != pre-lift `matches!(_, Unquote | UnquoteSplice)` at {label}"
            );
        }
    }

    #[test]
    fn as_unquote_inner_pointer_is_the_boxed_body() {
        // The returned `&Sexp` borrows the inner box's body verbatim — no
        // clone, no allocation, same lifetime as `&self`. Pin pointer
        // identity: the returned `&Sexp` shares its address with the
        // contents of the original Box, proving no intermediate copy fires
        // at the projection boundary (so consumers walking deeply nested
        // template bodies pay zero allocation per unquote node).
        let inner = Sexp::symbol("payload");
        let boxed = Box::new(inner);
        let inner_ptr: *const Sexp = boxed.as_ref();
        let form = Sexp::Unquote(boxed);
        let (_, body) = form
            .as_unquote()
            .expect("Sexp::Unquote must project to Some");
        assert!(
            std::ptr::eq(body, inner_ptr),
            "as_unquote inner pointer drifted from the boxed body — projection allocates or clones"
        );

        let inner_splice = Sexp::symbol("payload-splice");
        let boxed_splice = Box::new(inner_splice);
        let inner_splice_ptr: *const Sexp = boxed_splice.as_ref();
        let form_splice = Sexp::UnquoteSplice(boxed_splice);
        let (_, body_splice) = form_splice
            .as_unquote()
            .expect("Sexp::UnquoteSplice must project to Some");
        assert!(
            std::ptr::eq(body_splice, inner_splice_ptr),
            "as_unquote inner pointer drifted from the boxed body (splice arm)"
        );
    }

    // ── fmt_float: Display→read round-trip preserves Float identity ──────
    //
    // Rust's stdlib Display for f64 elides trailing `.0` on integral
    // floats — `format!("{}", 1.0_f64) == "1"` — and the substrate's
    // reader tries `i64::parse` before `f64::parse`, so a bare `1` re-reads
    // as `Atom::Int(1)`, NOT `Atom::Float(1.0)`. The Display→read
    // round-trip pre-lift dropped the typed Float identity on every
    // integral float: `Float(1.0)` displayed as `"1"`, re-read as `Int(1)`,
    // and downstream consumers silently typed the slot as Int. The
    // `fmt_float` helper appends `.0` for finite integral values so the
    // round-trip preserves the typed identity. Tests below pin:
    //   (a) Display of `Float(1.0)` is `"1.0"` (fail-before-pass-after);
    //   (b) the Display→read round-trip lands as `Float(1.0)`, NOT
    //       `Int(1)` (the typed-identity preservation contract);
    //   (c) non-integral floats render unchanged through the default
    //       impl (`Float(1.5)` is still `"1.5"`);
    //   (d) negative integral floats inherit the `.0` suffix
    //       (`Float(-2.0)` is `"-2.0"`);
    //   (e) integer Display is unaffected (`Int(1)` is still `"1"`) —
    //       pin path-uniformity so the helper is precisely scoped to
    //       the Float arm.

    #[test]
    fn fmt_float_renders_integral_float_with_trailing_zero() {
        // Fail-before-pass-after: pre-lift `Sexp::float(1.0).to_string()`
        // was `"1"`; post-lift the typed Float identity is preserved by
        // the `.0` suffix.
        assert_eq!(Sexp::float(1.0).to_string(), "1.0");
        assert_eq!(Sexp::float(100.0).to_string(), "100.0");
        assert_eq!(Sexp::float(0.0).to_string(), "0.0");
    }

    #[test]
    fn fmt_float_round_trips_integral_float_through_reader_as_float() {
        // The structural contract the lift establishes: a `Float`
        // serialized via `Display` re-reads as `Float`, NOT `Int`. Pin
        // the round-trip via the reader so a regression that drops the
        // `.0` suffix (or that re-orders the reader's i64/f64 parse
        // attempts to drop the float arm) surfaces here.
        let orig = Sexp::float(1.0);
        let rendered = orig.to_string();
        let forms =
            crate::reader::read(&rendered).expect("integral float must round-trip through reader");
        assert_eq!(forms.len(), 1);
        match &forms[0] {
            Sexp::Atom(Atom::Float(n)) => assert_eq!(*n, 1.0),
            other => panic!("Display->read round-trip dropped the Float identity, got: {other:?}"),
        }
        // Sibling-shape control: a SECOND integral magnitude reinforces
        // that the round-trip preserves the value, not only the type.
        let orig2 = Sexp::float(-42.0);
        let rendered2 = orig2.to_string();
        let forms2 = crate::reader::read(&rendered2)
            .expect("negative integral float must round-trip through reader");
        match &forms2[0] {
            Sexp::Atom(Atom::Float(n)) => assert_eq!(*n, -42.0),
            other => panic!(
                "Display->read of negative integral float dropped Float identity, got: {other:?}"
            ),
        }
    }

    #[test]
    fn fmt_float_preserves_non_integral_float_display() {
        // Path-uniformity: non-integral floats (the case the stdlib impl
        // already handled correctly) must render unchanged. A regression
        // that always-appends `.0` would write `"1.5.0"` and fail
        // here AND fail the reader round-trip below.
        assert_eq!(Sexp::float(1.5).to_string(), "1.5");
        assert_eq!(Sexp::float(0.99).to_string(), "0.99");
        assert_eq!(Sexp::float(-2.75).to_string(), "-2.75");

        // Round-trip control for the non-integral case stays valid: the
        // helper is precisely scoped, so the fractional component is
        // preserved verbatim through the reader.
        let orig = Sexp::float(0.99);
        let forms = crate::reader::read(&orig.to_string())
            .expect("non-integral float must round-trip through reader");
        match &forms[0] {
            Sexp::Atom(Atom::Float(n)) => assert_eq!(*n, 0.99),
            other => panic!("non-integral float round-trip drift, got: {other:?}"),
        }
    }

    // ── QuoteForm + as_quote_form: closed-set quote-family projection ─────
    //
    // `as_quote_form` lifts the per-callsite `Sexp::Quote(inner)
    // / Sexp::Quasiquote(inner) / Sexp::Unquote(inner) /
    // Sexp::UnquoteSplice(inner)` arm-set paired with their
    // per-variant prefix string (`'`, `` ` ``, `,`, `,@`) and
    // discriminator byte (3, 4, 5, 6) into ONE typed projection on
    // the `Sexp` algebra. Three consumers in this file route through
    // it (`Hash for Sexp`, `Display for Sexp`, `Sexp::as_unquote`)
    // so the (Sexp variant, marker, prefix, discriminator) tuple
    // binds at ONE site. Tests below pin:
    //   (a) the projection lands `Some((QuoteForm::*, inner))` for
    //       each of the four wrapper variants AND `None` for every
    //       non-quote-family shape;
    //   (b) `QuoteForm::prefix` returns the canonical reader-token
    //       prefix for each variant — load-bearing for the round-trip
    //       property the `Display`→reader dual encodes;
    //   (c) `QuoteForm::hash_discriminator` returns the same byte
    //       values the pre-lift Hash arms emitted (3, 4, 5, 6) — pin
    //       the cache-key contract so a regression that drifts a
    //       discriminator silently invalidates every cached expansion
    //       fails loudly here;
    //   (d) `QuoteForm::as_unquote_form` projects the 2-of-4 subset
    //       `{Unquote → UnquoteForm::Unquote, UnquoteSplice →
    //       UnquoteForm::Splice}` and yields `None` for `{Quote,
    //       Quasiquote}` — the structural-subset gate the
    //       `Sexp::as_unquote` derivation routes through;
    //   (e) `Sexp::as_unquote` derived from `as_quote_form +
    //       QuoteForm::as_unquote_form` agrees with the pre-lift
    //       arm-based semantic across every Sexp shape — path
    //       uniformity across the subset gate;
    //   (f) the four homoiconic prefixes round-trip through the
    //       reader via `read(format!("{prefix}{inner}"))` into the
    //       matching `Sexp::*` variant — the typed dual of the
    //       reader's prefix dispatch, pinned end-to-end on the four
    //       wrappers (sibling to `fmt_float`'s Float round-trip pin
    //       at the Display→read boundary).

    #[test]
    fn as_quote_form_projects_each_wrapper_variant_to_typed_marker_and_inner() {
        // `'foo` — Sexp::Quote wrapping a symbol. Pin Some((Quote, &inner))
        // with the typed marker AND the borrowed inner body.
        let inner = Sexp::symbol("foo");
        let form = Sexp::Quote(Box::new(inner.clone()));
        let (qf, body) = form.as_quote_form().expect("Sexp::Quote must project");
        assert_eq!(qf, QuoteForm::Quote);
        assert_eq!(body, &inner);

        // `` `foo `` — Sexp::Quasiquote wrapping a symbol.
        let form_qq = Sexp::Quasiquote(Box::new(inner.clone()));
        let (qf_qq, body_qq) = form_qq
            .as_quote_form()
            .expect("Sexp::Quasiquote must project");
        assert_eq!(qf_qq, QuoteForm::Quasiquote);
        assert_eq!(body_qq, &inner);

        // `,foo` — Sexp::Unquote wrapping a symbol.
        let form_u = Sexp::Unquote(Box::new(inner.clone()));
        let (qf_u, body_u) = form_u.as_quote_form().expect("Sexp::Unquote must project");
        assert_eq!(qf_u, QuoteForm::Unquote);
        assert_eq!(body_u, &inner);

        // `,@xs` — Sexp::UnquoteSplice wrapping a symbol.
        let form_us = Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs")));
        let (qf_us, body_us) = form_us
            .as_quote_form()
            .expect("Sexp::UnquoteSplice must project");
        assert_eq!(qf_us, QuoteForm::UnquoteSplice);
        assert_eq!(body_us, &Sexp::symbol("xs"));
    }

    #[test]
    fn as_quote_form_none_for_non_quote_family_shapes() {
        // Every shape OUTSIDE the closed quote-family must project to
        // None: Nil, every Atom variant, and List (empty + populated).
        // Pin the closed-set boundary so a regression that accidentally
        // promotes a non-wrapper variant into the quote family becomes
        // a typed test failure.
        assert_eq!(Sexp::Nil.as_quote_form(), None);
        assert_eq!(Sexp::symbol("x").as_quote_form(), None);
        assert_eq!(Sexp::keyword("k").as_quote_form(), None);
        assert_eq!(Sexp::string("s").as_quote_form(), None);
        assert_eq!(Sexp::int(7).as_quote_form(), None);
        assert_eq!(Sexp::float(2.5).as_quote_form(), None);
        assert_eq!(Sexp::boolean(true).as_quote_form(), None);
        assert_eq!(Sexp::List(vec![]).as_quote_form(), None);
        assert_eq!(
            Sexp::List(vec![Sexp::symbol("op"), Sexp::int(1)]).as_quote_form(),
            None
        );
    }

    #[test]
    fn as_quote_form_inner_pointer_is_the_boxed_body() {
        // The returned `&Sexp` borrows the inner box's body verbatim —
        // no clone, no allocation, same lifetime as `&self`. Pin
        // pointer identity for each of the four wrapper variants so a
        // regression that adds an intermediate copy at the projection
        // boundary surfaces here. Same posture as
        // `as_unquote_inner_pointer_is_the_boxed_body` for its 2-of-4
        // subset.
        let payload = Sexp::symbol("payload");
        let boxed = Box::new(payload);
        let inner_ptr: *const Sexp = boxed.as_ref();
        let form = Sexp::Quote(boxed);
        let (_, body) = form.as_quote_form().expect("Sexp::Quote must project");
        assert!(
            std::ptr::eq(body, inner_ptr),
            "as_quote_form inner pointer drifted from the boxed body — projection allocates or clones"
        );

        let payload_qq = Sexp::symbol("payload-qq");
        let boxed_qq = Box::new(payload_qq);
        let inner_ptr_qq: *const Sexp = boxed_qq.as_ref();
        let form_qq = Sexp::Quasiquote(boxed_qq);
        let (_, body_qq) = form_qq
            .as_quote_form()
            .expect("Sexp::Quasiquote must project");
        assert!(
            std::ptr::eq(body_qq, inner_ptr_qq),
            "as_quote_form inner pointer drifted (quasiquote arm)"
        );
    }

    #[test]
    fn quote_form_prefix_pins_canonical_reader_tokens_for_every_variant() {
        // Pin every prefix string load-bearing for the Display→read
        // round-trip. A regression that drifts the prefix (e.g. swaps
        // `'` and `` ` `` between Quote and Quasiquote) silently
        // re-routes every renderer through the wrong variant; this
        // test fails loudly. Sibling-arm sweep so the (variant,
        // prefix) pair stays load-bearing under reordering refactors.
        assert_eq!(QuoteForm::Quote.prefix(), "'");
        assert_eq!(QuoteForm::Quasiquote.prefix(), "`");
        assert_eq!(QuoteForm::Unquote.prefix(), ",");
        assert_eq!(QuoteForm::UnquoteSplice.prefix(), ",@");
    }

    #[test]
    fn quote_form_hash_discriminator_pins_legacy_cache_key_bytes() {
        // CACHE-KEY CONTRACT: pre-lift `Hash for Sexp` used the literal
        // byte values 3/4/5/6 for Quote/Quasiquote/Unquote/UnquoteSplice
        // as the per-variant discriminator. The expansion cache
        // (`Expander::cache`) keys on Hash; ANY change to a
        // discriminator byte silently invalidates every cached
        // expansion across the substrate AND risks collision with the
        // reserved bytes the non-quote-family Hash arms use (0=Nil,
        // 1=Atom, 2=List). Pin the four legacy values explicitly so a
        // regression that re-numbers them surfaces immediately — the
        // `QuoteForm` algebra MUST preserve the prior byte mapping
        // bit-for-bit.
        assert_eq!(QuoteForm::Quote.hash_discriminator(), 3);
        assert_eq!(QuoteForm::Quasiquote.hash_discriminator(), 4);
        assert_eq!(QuoteForm::Unquote.hash_discriminator(), 5);
        assert_eq!(QuoteForm::UnquoteSplice.hash_discriminator(), 6);
    }

    #[test]
    fn quote_form_as_unquote_form_projects_two_of_four_subset() {
        // The structural-subset gate: only `{Unquote, UnquoteSplice}`
        // are template-substitution markers; `{Quote, Quasiquote}` are
        // wrappers whose semantic does NOT include substitution. Pin
        // the 2-of-4 partition so the `Sexp::as_unquote` derivation's
        // closed-set arithmetic stays correct.
        assert_eq!(
            QuoteForm::Unquote.as_unquote_form(),
            Some(UnquoteForm::Unquote)
        );
        assert_eq!(
            QuoteForm::UnquoteSplice.as_unquote_form(),
            Some(UnquoteForm::Splice)
        );
        assert_eq!(QuoteForm::Quote.as_unquote_form(), None);
        assert_eq!(QuoteForm::Quasiquote.as_unquote_form(), None);
    }

    #[test]
    fn quote_form_iac_forge_tag_pins_canonical_lisp_tag_strings_for_every_variant() {
        // CROSS-CRATE CANONICAL-FORM CONTRACT: the four canonical
        // iac-forge tags are load-bearing for inter-crate compatibility
        // — `iac_forge::sexpr::SExpr` consumers (BLAKE3 attestation,
        // render cache) key on the canonical 2-element-list shape
        // `(<tag> <inner>)`. A regression that drifts ONE tag silently
        // invalidates every cached canonical form across the substrate
        // AND mis-collides with the legacy `SexpShape::label` projection
        // that uses the shorter `"unquote-splice"` for the diagnostic
        // surface. Pin the four legacy tag values explicitly so a
        // regression that re-spells them surfaces immediately.
        assert_eq!(QuoteForm::Quote.iac_forge_tag(), "quote");
        assert_eq!(QuoteForm::Quasiquote.iac_forge_tag(), "quasiquote");
        assert_eq!(QuoteForm::Unquote.iac_forge_tag(), "unquote");
        assert_eq!(QuoteForm::UnquoteSplice.iac_forge_tag(), "unquote-splicing");
    }

    #[test]
    fn quote_form_iac_forge_tag_diverges_from_sexp_shape_label_for_unquote_splice() {
        // BOUNDARY-DISTINCT CONTRACT: the iac-forge canonical tag for
        // `UnquoteSplice` is `"unquote-splicing"` (Common Lisp idiom,
        // load-bearing for canonical-form round-trip with the iac-forge
        // ecosystem), distinct from `SexpShape::label`'s shorter
        // `"unquote-splice"` (the substrate's diagnostic label idiom).
        // The two projections key the SAME closed-set on TWO distinct
        // boundaries — pinning the divergence here documents the
        // intent: a future "consolidation" PR that homogenizes them
        // would silently break either the iac-forge canonical-form
        // round-trip OR the operator-facing diagnostic surface. The
        // three other variants (Quote, Quasiquote, Unquote) DO match
        // across both projections — pin that path-uniformity too so a
        // regression that drifts one of the three matched arms surfaces
        // immediately. Sibling-arm sweep so the (variant, tag) AND
        // (variant, label) pairings stay load-bearing under reordering
        // refactors.
        use crate::error::SexpShape;
        assert_eq!(
            QuoteForm::Quote.iac_forge_tag(),
            SexpShape::Quote.label(),
            "quote tag/label agreement"
        );
        assert_eq!(
            QuoteForm::Quasiquote.iac_forge_tag(),
            SexpShape::Quasiquote.label(),
            "quasiquote tag/label agreement"
        );
        assert_eq!(
            QuoteForm::Unquote.iac_forge_tag(),
            SexpShape::Unquote.label(),
            "unquote tag/label agreement"
        );
        // The intentional divergence — load-bearing for the iac-forge
        // canonical form vs the substrate's diagnostic label.
        assert_eq!(QuoteForm::UnquoteSplice.iac_forge_tag(), "unquote-splicing");
        assert_eq!(SexpShape::UnquoteSplice.label(), "unquote-splice");
        assert_ne!(
            QuoteForm::UnquoteSplice.iac_forge_tag(),
            SexpShape::UnquoteSplice.label(),
            "the two projections must disagree at UnquoteSplice — the CL canonical \
             form requires '-splicing' while the substrate's diagnostic label uses \
             the shorter '-splice'; consolidating them would break either side",
        );
    }

    #[test]
    fn as_unquote_derives_from_as_quote_form_composed_with_subset_gate() {
        // Path-uniformity: `Sexp::as_unquote` is now derived from
        // `as_quote_form().and_then(|(qf, inner)| qf.as_unquote_form()
        // .map(|uf| (uf, inner)))`. Pin that the derived semantic
        // agrees with the pre-lift arm-based one across the closed
        // Sexp variant set — every shape's projection through
        // `as_unquote` must equal the manual composition through
        // `as_quote_form` + `QuoteForm::as_unquote_form`. A regression
        // that drifts ONE projection's posture from the composition
        // becomes a typed test failure.
        let shapes: Vec<(&str, Sexp)> = vec![
            ("nil", Sexp::Nil),
            ("symbol", Sexp::symbol("x")),
            ("keyword", Sexp::keyword("k")),
            ("string", Sexp::string("s")),
            ("int", Sexp::int(7)),
            ("float", Sexp::float(2.5)),
            ("bool", Sexp::boolean(true)),
            ("empty list", Sexp::List(vec![])),
            ("non-empty list", Sexp::List(vec![Sexp::symbol("op")])),
            ("quote", Sexp::Quote(Box::new(Sexp::symbol("x")))),
            ("quasiquote", Sexp::Quasiquote(Box::new(Sexp::symbol("x")))),
            ("unquote", Sexp::Unquote(Box::new(Sexp::symbol("x")))),
            (
                "unquote-splice",
                Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs"))),
            ),
        ];
        for (label, sexp) in &shapes {
            let via_direct = sexp.as_unquote();
            let via_composed = sexp
                .as_quote_form()
                .and_then(|(qf, inner)| qf.as_unquote_form().map(|uf| (uf, inner)));
            assert_eq!(
                via_direct, via_composed,
                "as_unquote drifted from composed as_quote_form+as_unquote_form at {label}"
            );
        }
    }

    #[test]
    fn hash_for_sexp_preserves_legacy_quote_family_discriminator_bytes() {
        // CACHE-KEY CONTRACT (Hash side): pin that the lifted
        // `Hash for Sexp` impl produces byte-identical hashes for the
        // four quote-family variants as the pre-lift implementation.
        // We compute the expected hash via a SECOND hasher that
        // manually drives the pre-lift `<discr>.hash(h); inner.hash(h)`
        // sequence, then compare. A regression that drifts the
        // discriminator OR re-orders the (discr, inner) sequence
        // surfaces here as a hash-value mismatch.
        use std::collections::hash_map::DefaultHasher;
        let inner = Sexp::symbol("payload");
        for (label, sexp, expected_discr) in [
            ("quote", Sexp::Quote(Box::new(inner.clone())), 3u8),
            ("quasiquote", Sexp::Quasiquote(Box::new(inner.clone())), 4u8),
            ("unquote", Sexp::Unquote(Box::new(inner.clone())), 5u8),
            (
                "unquote-splice",
                Sexp::UnquoteSplice(Box::new(inner.clone())),
                6u8,
            ),
        ] {
            let mut via_impl = DefaultHasher::new();
            sexp.hash(&mut via_impl);

            let mut via_legacy = DefaultHasher::new();
            expected_discr.hash(&mut via_legacy);
            inner.hash(&mut via_legacy);

            assert_eq!(
                via_impl.finish(),
                via_legacy.finish(),
                "Hash for Sexp drifted from legacy (discr={expected_discr}, inner) sequence at {label}"
            );
        }
    }

    #[test]
    fn display_for_sexp_renders_each_quote_family_variant_with_canonical_prefix() {
        // Pin the post-lift Display rendering: every wrapper variant
        // renders as `<prefix><inner>` with the prefix sourced from
        // `QuoteForm::prefix`. A regression that drifts the prefix
        // arm-routing (e.g. routes Quote through `` ` `` instead of
        // `'`) fails loudly here. The literal `inner` rendering is
        // the symbol `foo` so the prefix is the only diff between
        // arms — pin path-uniformity across the closed set.
        let inner = Sexp::symbol("foo");
        assert_eq!(Sexp::Quote(Box::new(inner.clone())).to_string(), "'foo");
        assert_eq!(
            Sexp::Quasiquote(Box::new(inner.clone())).to_string(),
            "`foo"
        );
        assert_eq!(Sexp::Unquote(Box::new(inner.clone())).to_string(), ",foo");
        assert_eq!(Sexp::UnquoteSplice(Box::new(inner)).to_string(), ",@foo");
    }

    #[test]
    fn display_for_sexp_round_trips_each_quote_family_variant_through_reader() {
        // ROUND-TRIP CONTRACT: every wrapper variant's Display →
        // reader path produces the matching `Sexp::*` variant. The
        // reader's prefix-dispatch (in `reader::parse`) consumes the
        // canonical `'` / `` ` `` / `,` / `,@` tokens and produces
        // the corresponding wrapper; the Display impl emits the same
        // tokens via `QuoteForm::prefix`. Pin the round-trip
        // end-to-end so a regression that drifts the prefix on
        // either side (Display or reader) fails loudly here. Sibling
        // posture to `fmt_float_round_trips_integral_float_through
        // _reader_as_float` — the Float round-trip pin at the
        // Display→read boundary; this test pins the four
        // quote-family round-trips at the same boundary.
        let inner_body = Sexp::symbol("payload");

        let quote = Sexp::Quote(Box::new(inner_body.clone()));
        let forms = crate::reader::read(&quote.to_string()).expect("quote must round-trip");
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0], quote);

        let quasiquote = Sexp::Quasiquote(Box::new(inner_body.clone()));
        let forms =
            crate::reader::read(&quasiquote.to_string()).expect("quasiquote must round-trip");
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0], quasiquote);

        let unquote = Sexp::Unquote(Box::new(inner_body.clone()));
        let forms = crate::reader::read(&unquote.to_string()).expect("unquote must round-trip");
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0], unquote);

        let splice = Sexp::UnquoteSplice(Box::new(inner_body));
        let forms =
            crate::reader::read(&splice.to_string()).expect("unquote-splice must round-trip");
        assert_eq!(forms.len(), 1);
        assert_eq!(forms[0], splice);
    }

    #[test]
    fn quote_form_wrap_projects_each_typed_marker_into_matching_sexp_wrapper() {
        // CLOSED-SET CONSTRUCTOR CONTRACT: pin that `QuoteForm::wrap` is
        // the structural inverse of `Sexp::as_quote_form` at the
        // marker→wrapper boundary. Every variant of the closed-set
        // `QuoteForm` algebra projects to its matching `Sexp::*` wrapper
        // applied to the supplied inner — `Quote → Sexp::Quote`,
        // `Quasiquote → Sexp::Quasiquote`, `Unquote → Sexp::Unquote`,
        // `UnquoteSplice → Sexp::UnquoteSplice`. A regression that swaps
        // two arms (e.g. `Self::Quote → Sexp::Quasiquote`) type-checks
        // but silently corrupts every consumer that constructs a quote-
        // family Sexp through the projection — fails loudly here.
        // Sibling-arm sweep so the (marker, constructor) pair stays
        // load-bearing under reordering refactors.
        let inner = Sexp::symbol("payload");
        assert_eq!(
            QuoteForm::Quote.wrap(inner.clone()),
            Sexp::Quote(Box::new(inner.clone()))
        );
        assert_eq!(
            QuoteForm::Quasiquote.wrap(inner.clone()),
            Sexp::Quasiquote(Box::new(inner.clone()))
        );
        assert_eq!(
            QuoteForm::Unquote.wrap(inner.clone()),
            Sexp::Unquote(Box::new(inner.clone()))
        );
        assert_eq!(
            QuoteForm::UnquoteSplice.wrap(inner.clone()),
            Sexp::UnquoteSplice(Box::new(inner))
        );
    }

    #[test]
    fn quote_form_wrap_round_trips_through_as_quote_form_for_every_variant() {
        // ROUND-TRIP CONTRACT: pin the structural identity
        // `qf.wrap(inner.clone()).as_quote_form() == Some((qf, &inner))`
        // for every variant of the closed-set `QuoteForm` algebra. This
        // is the canonical law binding the marker→wrapper projection
        // (`wrap`) to its wrapper→marker dual (`as_quote_form`) on the
        // substrate's `Sexp` algebra. A regression that drifts the
        // (marker, constructor) pair on EITHER side — `wrap` routing
        // `Quote` to `Sexp::Quasiquote`, OR `as_quote_form` routing
        // `Sexp::Quote(_)` to `QuoteForm::Quasiquote` — surfaces as a
        // round-trip mismatch here. Sweep all four variants so the
        // round-trip stays load-bearing across the closed set. Same
        // posture as the `display_for_sexp_round_trips_each_quote_family
        // _variant_through_reader` round-trip pin at the Display→read
        // boundary; this test pins the round-trip at the marker→Sexp
        // projection boundary.
        let inner_body = Sexp::symbol("payload");
        for qf in [
            QuoteForm::Quote,
            QuoteForm::Quasiquote,
            QuoteForm::Unquote,
            QuoteForm::UnquoteSplice,
        ] {
            let wrapped = qf.wrap(inner_body.clone());
            let projected = wrapped
                .as_quote_form()
                .expect("wrap output must project back through as_quote_form");
            assert_eq!(
                projected.0, qf,
                "wrap→as_quote_form drifted at marker for variant {qf:?}"
            );
            assert_eq!(
                projected.1, &inner_body,
                "wrap→as_quote_form drifted at inner body for variant {qf:?}"
            );
        }
    }

    #[test]
    fn quote_form_wrap_derives_each_arm_to_its_pre_lift_box_new_form() {
        // PATH-UNIFORMITY CONTRACT: pin that `QuoteForm::wrap` is
        // observably equivalent to the pre-lift four-arm reader pattern
        // `Sexp::<Variant>(Box::new(inner))` across every variant of the
        // closed set. The reader's pre-lift parse arms each constructed
        // their corresponding wrapper inline; post-lift the parse routes
        // through `QuoteForm::wrap`. A regression that drifts the
        // projection's allocation posture (e.g. wraps in an extra layer,
        // or skips the `Box::new`) fails loudly here. Companion to the
        // `wrap` projection test above — that test pins the (marker,
        // constructor) pairing; this test pins the structural shape of
        // each wrap output bit-for-bit against the pre-lift inline form.
        let inner = Sexp::List(vec![Sexp::symbol("inner"), Sexp::int(7)]);
        for (qf, expected) in [
            (QuoteForm::Quote, Sexp::Quote(Box::new(inner.clone()))),
            (
                QuoteForm::Quasiquote,
                Sexp::Quasiquote(Box::new(inner.clone())),
            ),
            (QuoteForm::Unquote, Sexp::Unquote(Box::new(inner.clone()))),
            (
                QuoteForm::UnquoteSplice,
                Sexp::UnquoteSplice(Box::new(inner.clone())),
            ),
        ] {
            assert_eq!(
                qf.wrap(inner.clone()),
                expected,
                "wrap drifted from pre-lift Sexp::<Variant>(Box::new(inner)) form for {qf:?}"
            );
        }
    }

    #[test]
    fn fmt_float_leaves_int_display_unchanged() {
        // Path-uniformity sibling: `Atom::Int` Display is unaffected by
        // the `fmt_float` introduction — the helper is wired only into
        // the `Atom::Float` arm of the Display match. A regression that
        // accidentally routes `Atom::Int` through `fmt_float` would
        // render `"1.0"` here and break every consumer that authored an
        // int kwarg expecting the bare-integer rendering.
        assert_eq!(Sexp::int(1).to_string(), "1");
        assert_eq!(Sexp::int(0).to_string(), "0");
        assert_eq!(Sexp::int(-42).to_string(), "-42");
    }
}
