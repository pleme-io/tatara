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
            Self::Quote(inner) => {
                3u8.hash(h);
                inner.hash(h);
            }
            Self::Quasiquote(inner) => {
                4u8.hash(h);
                inner.hash(h);
            }
            Self::Unquote(inner) => {
                5u8.hash(h);
                inner.hash(h);
            }
            Self::UnquoteSplice(inner) => {
                6u8.hash(h);
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
        match self {
            Self::Unquote(inner) => Some((UnquoteForm::Unquote, inner)),
            Self::UnquoteSplice(inner) => Some((UnquoteForm::Splice, inner)),
            _ => None,
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
            Self::Quote(inner) => write!(f, "'{inner}"),
            Self::Quasiquote(inner) => write!(f, "`{inner}"),
            Self::Unquote(inner) => write!(f, ",{inner}"),
            Self::UnquoteSplice(inner) => write!(f, ",@{inner}"),
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
