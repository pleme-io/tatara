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
                // The four quote-family wrappers share the
                // `tagged(<canonical-tag>, inner)` canonical-form shape — all
                // route through `as_quote_form`'s typed-marker projection so
                // the per-variant tag string binds at ONE site on the
                // closed-set `QuoteForm` algebra (`QuoteForm::iac_forge_tag`)
                // rather than four inline literal strings across the arms.
                // The (Sexp variant, canonical iac-forge tag) pairing now
                // binds bit-for-bit through the typed algebra, so a future
                // homoiconic prefix-wrapper extension picks up the canonical
                // form mechanically via the closed-set match. The `.expect(_)`
                // is a static-invariant statement (the outer pattern guarantees
                // the projection lands `Some`) — a future quote-family
                // extension that drifts `Sexp` AND `QuoteForm` apart fails at
                // rustc, not at runtime.
                Sexp::Quote(_)
                | Sexp::Quasiquote(_)
                | Sexp::Unquote(_)
                | Sexp::UnquoteSplice(_) => {
                    let (qf, inner) = s.expect_quote_form();
                    tagged(qf.iac_forge_tag(), inner)
                }
            }
        }
    }

    impl From<Sexp> for SExpr {
        fn from(s: Sexp) -> Self {
            (&s).into()
        }
    }

    fn tagged(tag: &str, inner: &Sexp) -> SExpr {
        SExpr::List(vec![SExpr::Symbol(tag.to_string()), inner.into()])
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::ast::QuoteForm;

        #[test]
        fn quote_family_canonical_form_routes_through_quote_form_iac_forge_tag() {
            // CANONICAL-FORM CONTRACT (end-to-end): pin that the lifted
            // `From<&Sexp> for SExpr` impl produces byte-identical
            // canonical 2-element-list `(<tag> <inner>)` shapes for the
            // four quote-family variants as the pre-lift implementation,
            // routing through `QuoteForm::iac_forge_tag` so the
            // (Sexp variant, canonical tag string) pairing is structurally
            // bound to the algebra rather than threaded through inline
            // literals. A regression that drifts the canonical tag at the
            // interop arm (e.g. drops the `-splicing` suffix that
            // distinguishes the CL canonical form from the substrate's
            // diagnostic label) fails loudly here. Sibling-arm sweep so
            // the four pairings stay load-bearing under reordering
            // refactors.
            let inner = Sexp::symbol("payload");
            let expected_inner: SExpr = (&inner).into();

            for (variant_label, sexp, expected_tag) in [
                (
                    "quote",
                    Sexp::Quote(Box::new(inner.clone())),
                    QuoteForm::Quote.iac_forge_tag(),
                ),
                (
                    "quasiquote",
                    Sexp::Quasiquote(Box::new(inner.clone())),
                    QuoteForm::Quasiquote.iac_forge_tag(),
                ),
                (
                    "unquote",
                    Sexp::Unquote(Box::new(inner.clone())),
                    QuoteForm::Unquote.iac_forge_tag(),
                ),
                (
                    "unquote-splicing",
                    Sexp::UnquoteSplice(Box::new(inner.clone())),
                    QuoteForm::UnquoteSplice.iac_forge_tag(),
                ),
            ] {
                let via_impl: SExpr = (&sexp).into();
                let via_legacy = SExpr::List(vec![
                    SExpr::Symbol(expected_tag.to_string()),
                    expected_inner.clone(),
                ]);
                assert_eq!(
                    via_impl, via_legacy,
                    "From<&Sexp> for SExpr drifted from \
                     canonical (tag={expected_tag}, inner) shape at {variant_label}"
                );
            }
        }

        #[test]
        fn unquote_splice_canonical_form_uses_cl_idiomatic_unquote_splicing_tag() {
            // INTENT-PIN: the `,@x` form's canonical iac-forge encoding
            // MUST use the Common Lisp idiomatic `(unquote-splicing x)`
            // tag, NOT the substrate's shorter `unquote-splice` diagnostic
            // label. This boundary distinction is load-bearing: the
            // iac-forge ecosystem's canonical-form BLAKE3 hashes depend
            // on the exact tag spelling, and a consolidation PR that
            // homogenizes the two projections would silently break the
            // canonical-form round-trip with every iac-forge consumer
            // already keyed on `(unquote-splicing ...)`. Pin the tag at
            // the interop boundary directly so the intent is enforced
            // even if a future refactor renames `QuoteForm::iac_forge_tag`.
            let splice = Sexp::UnquoteSplice(Box::new(Sexp::symbol("xs")));
            let canonical: SExpr = (&splice).into();
            match canonical {
                SExpr::List(items) => {
                    assert_eq!(items.len(), 2);
                    assert_eq!(items[0], SExpr::Symbol("unquote-splicing".to_string()));
                }
                other => panic!("expected canonical list shape, got {other:?}"),
            }
        }
    }
}
