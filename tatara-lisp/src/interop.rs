//! Cross-crate interop — bridges to neighbouring pleme-io S-expression types.
//!
//! Currently:
//!   - `iac_forge::sexpr::SExpr` (feature `iac-forge`) — canonical serialization
//!     AST for the IaC forge ecosystem. Used for BLAKE3 attestation + render cache.
//!
//! The mapping `tatara_lisp::Sexp → iac_forge::SExpr` is lossy: the homoiconic
//! variants (`Quote`, `Quasiquote`, `Unquote`, `UnquoteSplice`) have no
//! canonical-form equivalents. We encode them as 2-element lists headed by a
//! distinguishing symbol so round-tripping preserves structure.

#[cfg(feature = "iac-forge")]
mod iac_forge_impl {
    use crate::ast::{Atom, Sexp};
    use iac_forge::sexpr::SExpr;

    impl From<&Sexp> for SExpr {
        fn from(s: &Sexp) -> Self {
            match s {
                Sexp::Nil => SExpr::Nil,
                Sexp::Atom(a) => match a {
                    Atom::Symbol(s) => SExpr::Symbol(s.clone()),
                    // Keywords encoded as `:name` symbols in canonical form.
                    Atom::Keyword(s) => SExpr::Symbol(format!(":{s}")),
                    Atom::Str(s) => SExpr::String(s.clone()),
                    Atom::Int(n) => SExpr::Integer(*n),
                    Atom::Float(n) => SExpr::Float(*n),
                    Atom::Bool(b) => SExpr::Bool(*b),
                },
                Sexp::List(xs) => SExpr::List(xs.iter().map(Self::from).collect()),
                Sexp::Quote(inner) => tagged("quote", inner),
                Sexp::Quasiquote(inner) => tagged("quasiquote", inner),
                Sexp::Unquote(inner) => tagged("unquote", inner),
                Sexp::UnquoteSplice(inner) => tagged("unquote-splicing", inner),
            }
        }
    }

    impl From<Sexp> for SExpr {
        fn from(s: Sexp) -> Self {
            (&s).into()
        }
    }

    fn tagged(tag: &str, inner: &Sexp) -> SExpr {
        SExpr::List(vec![
            SExpr::Symbol(tag.to_string()),
            inner.as_ref().into(),
        ])
    }
}
