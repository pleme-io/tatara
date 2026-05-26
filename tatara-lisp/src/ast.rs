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
    pub fn head_symbol(&self) -> Option<&str> {
        self.as_list()?.first()?.as_symbol()
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
}
