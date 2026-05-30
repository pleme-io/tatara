//! S-expression AST.

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
    pub fn as_call_to(&self, keyword: &str) -> Option<&[Sexp]> {
        let (head, args) = self.as_call()?;
        (head == keyword).then_some(args)
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
                Atom::Float(n) => write!(f, "{n}"),
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
}
