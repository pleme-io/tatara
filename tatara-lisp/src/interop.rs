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
    use crate::ast::Sexp;
    use iac_forge::sexpr::SExpr;

    impl From<&Sexp> for SExpr {
        fn from(s: &Sexp) -> Self {
            match s {
                Sexp::Nil => SExpr::Nil,
                // The atomic-payload rendering lives at the typed
                // [`Atom::to_iac_forge_sexpr`] projection in `ast.rs` —
                // the six inline sub-arms (`Symbol → SExpr::Symbol(s)`,
                // `Keyword → SExpr::Symbol(":{s}")`, `Str →
                // SExpr::String(s)`, `Int → SExpr::Integer(n)`, `Float →
                // SExpr::Float(n)`, `Bool → SExpr::Bool(b)`) all bind at
                // ONE site on the closed-set `Atom` algebra rather than
                // at this outer arm. Completes the three-surface sweep
                // (`Display for Atom` for Lisp canonical form,
                // `Atom::to_json` for JSON, `Atom::to_iac_forge_sexpr`
                // for iac-forge attestation form) — a future seventh
                // atomic kind extends the `Atom` projection family ONCE
                // and rustc enforces matching across every consumer.
                Sexp::Atom(a) => a.to_iac_forge_sexpr(),
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
        use crate::ast::{Atom, AtomKind, QuoteForm};

        #[test]
        fn atom_to_iac_forge_sexpr_projects_each_variant_to_canonical_sexpr() {
            // CANONICAL-MAPPING CONTRACT: pin that `Atom::to_iac_forge_sexpr`
            // produces byte-identical `iac_forge::sexpr::SExpr` outputs for
            // each `AtomKind` variant as the pre-lift inline arms inside
            // `From<&Sexp> for SExpr` did. Sweeps a representative atom of
            // each variant so a regression that drifts ONE arm (e.g. swaps
            // `Symbol`'s mapping to a `String`, drops `Keyword`'s `:`
            // prefix that downstream BLAKE3 attestation keys hash, or
            // renames `Str → Integer`) fails loudly. Sibling-arm sweep to
            // `atom_display_renders_each_variant_to_canonical_form` (the
            // Lisp canonical-form peer) and
            // `atom_to_json_projects_each_variant_to_canonical_json_value`
            // (the JSON canonical-form peer in `ast.rs`) — all three pin
            // the typed-algebra rendering of the atomic payload at its
            // canonical projection across the three production surfaces
            // the substrate carries.
            assert_eq!(
                Atom::Symbol("name".into()).to_iac_forge_sexpr(),
                SExpr::Symbol("name".to_string()),
            );
            assert_eq!(
                Atom::Keyword("parent".into()).to_iac_forge_sexpr(),
                SExpr::Symbol(":parent".to_string()),
            );
            assert_eq!(
                Atom::Str("body".into()).to_iac_forge_sexpr(),
                SExpr::String("body".to_string()),
            );
            assert_eq!(Atom::Int(42).to_iac_forge_sexpr(), SExpr::Integer(42));
            assert_eq!(Atom::Int(-7).to_iac_forge_sexpr(), SExpr::Integer(-7));
            assert_eq!(Atom::Float(1.5).to_iac_forge_sexpr(), SExpr::Float(1.5));
            assert_eq!(Atom::Bool(true).to_iac_forge_sexpr(), SExpr::Bool(true));
            assert_eq!(Atom::Bool(false).to_iac_forge_sexpr(), SExpr::Bool(false));
        }

        #[test]
        fn sexp_atom_iac_forge_arm_routes_through_atom_to_iac_forge_sexpr() {
            // LIFTED-BOUNDARY CONTRACT: pin that the outer
            // `From<&Sexp> for SExpr` impl's Atom arm produces
            // byte-identical output to direct `Atom::to_iac_forge_sexpr`
            // invocation for every atomic payload variant. Catches a
            // regression where the outer arm re-inlines ONE variant's
            // rendering without updating the typed projection, or vice
            // versa — the lifted boundary is exactly the invariant
            // `SExpr::from(&Sexp::Atom(a)) == a.to_iac_forge_sexpr()`.
            // Sweeps `AtomKind::ALL` so adding a future seventh atomic
            // kind (e.g. `Char`, `Bigint`) forces the test author to
            // extend both the closed-set sweep AND the per-variant
            // projection at the typed-algebra layer; rustc's
            // exhaustiveness on `Atom`'s match in
            // `Atom::to_iac_forge_sexpr` enforces the per-variant body
            // is named, and this test enforces the outer arm's
            // delegation stays uniform under the addition.
            //
            // Sibling-shape pin to
            // `sexp_to_json_atom_arms_route_through_atom_to_json` (in
            // `domain.rs`) and the implicit
            // `Sexp::Atom(a).to_string() == a.to_string()` invariant
            // pinned by
            // `sexp_atom_display_arm_routes_through_atom_display_for_every_variant`
            // (in `ast.rs`) — all three pin the lifted-boundary
            // identity at the corresponding production surface so a
            // future drift surfaces at the typed-algebra layer rather
            // than at every downstream consumer.
            for kind in AtomKind::ALL {
                let atom = sample_atom(kind);
                let via_impl: SExpr = (&Sexp::Atom(atom.clone())).into();
                let via_projection = atom.to_iac_forge_sexpr();
                assert_eq!(
                    via_impl, via_projection,
                    "From<&Sexp> for SExpr Atom arm drifted from \
                     Atom::to_iac_forge_sexpr for variant {kind:?}"
                );
            }
        }

        /// Canonical per-variant atomic sample mirroring the shape
        /// `atom_display_round_trips_through_reader_preserving_typed_identity`
        /// in `ast.rs` uses — one representative payload for each
        /// `AtomKind` variant so the boundary tests above can sweep
        /// the closed set without re-deriving sample literals.
        fn sample_atom(kind: AtomKind) -> Atom {
            match kind {
                AtomKind::Symbol => Atom::Symbol("name".into()),
                AtomKind::Keyword => Atom::Keyword("parent".into()),
                AtomKind::Str => Atom::Str("body".into()),
                AtomKind::Int => Atom::Int(42),
                AtomKind::Float => Atom::Float(1.5),
                AtomKind::Bool => Atom::Bool(true),
            }
        }

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
