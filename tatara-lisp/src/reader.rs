//! S-expression reader — tokenize + parse into `Sexp`.
//!
//! Source positions are byte offsets into the original `&str`. Every token
//! carries the offset of its first character; reader-level errors
//! (`UnmatchedParen`, `UnmatchedOpenParen`, `Eof`) report that offset so
//! downstream tools (`tatara-lispc`, `tatara-check`, REPL, future LSP) can
//! pinpoint the failure in the source.

use crate::ast::{Atom, QuoteForm, Sexp};
use crate::error::{LispError, Result};

// The four homoiconic prefix-wrappers (`'`, `` ` ``, `,`, `,@`) collapse
// onto ONE `Token::Quoted(QuoteForm)` variant carrying the substrate's
// typed `QuoteForm` marker. Pre-lift the reader carried its own parallel
// closed set (`Token::{Quote, Quasiquote, Unquote, UnquoteSplice}`) paired
// with the matching `Sexp::*` tuple-variant constructors threaded as
// `fn(Box<Sexp>) -> Sexp` arguments to `read_quoted` — the FIFTH consumer
// site of the quote-family closed set the prior `QuoteForm` lifts did not
// reach. Post-lift the reader binds to the substrate algebra: tokenizer
// arms construct `Token::Quoted(QuoteForm::*)` directly, the parser
// collapses its four `Some((Token::Quote*, _))` arms to ONE
// `Some((Token::Quoted(qf), _))` arm, and `read_quoted` routes through
// `QuoteForm::wrap` so the (marker, Sexp::* constructor) pairing binds
// at ONE site rather than per-arm. Adding a fifth homoiconic prefix
// extends `QuoteForm` AND the tokenizer arm AND `QuoteForm::wrap`'s arm
// in lockstep — rustc binds the reader's wrap step to the substrate
// algebra through exhaustiveness over the closed enum.
#[derive(Clone, Debug, PartialEq)]
enum Token {
    LParen,
    RParen,
    Quoted(QuoteForm),
    Atom(String),
    Str(String),
}

/// A token paired with the byte offset of its first character in the source.
type Spanned = (Token, usize);

/// Read a full program (sequence of top-level forms) into a `Vec<Sexp>`.
pub fn read(src: &str) -> Result<Vec<Sexp>> {
    let tokens = tokenize(src)?;
    let eof_pos = src.len();
    let mut it = tokens.into_iter().peekable();
    let mut forms = Vec::new();
    while it.peek().is_some() {
        forms.push(parse(&mut it, eof_pos)?);
    }
    Ok(forms)
}

fn tokenize(src: &str) -> Result<Vec<Spanned>> {
    let mut out = Vec::new();
    let mut chars = src.char_indices().peekable();
    while let Some(&(pos, c)) = chars.peek() {
        // Quote-family outer dispatch — the (lead char, `QuoteForm`
        // marker) pairing binds at ONE site on the closed-set
        // [`QuoteForm`] algebra via [`QuoteForm::from_lead_char`].
        // Pre-lift the four homoiconic prefixes (`'`, `` ` ``, `,`,
        // `,@`) dispatched from four inline `char`-literal arms of
        // this outer match (`'\''` / `` '`' `` / `','` with the
        // `Token::Quoted(QuoteForm::UnquoteSplice)` construction
        // buried inside the `','`-arm's peek branch); post-lift the
        // reader pre-checks `QuoteForm::from_lead_char(c)` — which
        // returns `Some(Unquote)` on the shared `,` lead char and
        // `Some(Quote)` / `Some(Quasiquote)` on the two distinct
        // singleton lead chars — then promotes the decoded
        // `QuoteForm::Unquote` to `QuoteForm::UnquoteSplice` on
        // second-char `@`, and emits ONE `Token::Quoted(final_qf)`.
        // Adding a fifth homoiconic prefix extends [`QuoteForm`] AND
        // [`QuoteForm::from_lead_char`] in lockstep — rustc binds
        // the extension through exhaustiveness over the closed enum.
        if let Some(qf_head) = QuoteForm::from_lead_char(c) {
            chars.next();
            let qf = if matches!(qf_head, QuoteForm::Unquote)
                && chars.peek().map(|&(_, c)| c) == Some('@')
            {
                chars.next();
                QuoteForm::UnquoteSplice
            } else {
                qf_head
            };
            out.push((Token::Quoted(qf), pos));
            continue;
        }
        match c {
            ws if ws.is_whitespace() => {
                chars.next();
            }
            // Line-comment arm — the canonical `;` byte routed through
            // the [`Sexp::COMMENT_LEAD`] constant on the closed-set outer
            // [`Sexp`] algebra. Outer-structural peer of the
            // [`Sexp::LIST_OPEN`] / [`Sexp::LIST_CLOSE`] list-delimiter
            // lifts on the reader-discard axis: where those two constants
            // shape a `Sexp::List` payload, this constant is the ONE `;`
            // byte the reader's outer-dispatch arm AND the bare-atom
            // terminator disjunct below both bind to on the closed-set
            // outer [`Sexp`] algebra. The trailing `\n` disjunct is
            // absorbed by the whitespace check in the outer match's
            // guard arm; this loop consumes every byte up to (and
            // including) the newline so the discarded run emits NO
            // token.
            Sexp::COMMENT_LEAD => {
                while let Some(&(_, ch)) = chars.peek() {
                    chars.next();
                    if ch == '\n' {
                        break;
                    }
                }
            }
            // List-opening arm — the canonical `(` byte routed through
            // the [`Sexp::LIST_OPEN`] constant on the closed-set outer
            // [`Sexp`] algebra. Outer-structural peer of the
            // [`Atom::STR_DELIMITER`] Str-payload lift on the atomic
            // axis, and of the [`QuoteForm::from_lead_char`] outer
            // quote-family dispatch above — the (structural role,
            // canonical byte) pairing binds at ONE typed constant on
            // the substrate algebra rather than at an inline `char`
            // literal at this arm AND the bare-atom terminator's
            // disjunct below AND `Sexp`'s Display impl's opener/closer
            // arms in `ast.rs`.
            Sexp::LIST_OPEN => {
                chars.next();
                out.push((Token::LParen, pos));
            }
            // List-closing arm — the canonical `)` byte routed through
            // the [`Sexp::LIST_CLOSE`] constant on the closed-set outer
            // [`Sexp`] algebra. Section-for-retraction sibling of the
            // list-opening arm above; the paired-delimiter round-trip
            // holds iff both arms bind to the same closed-set constants.
            Sexp::LIST_CLOSE => {
                chars.next();
                out.push((Token::RParen, pos));
            }
            // String-opening arm — the canonical `"` byte routed through
            // the [`Atom::STR_DELIMITER`] constant on the closed-set
            // [`Atom`] algebra. The closing arm below AND the self-
            // escape arm inside the escape table AND the bare-atom
            // terminator disjunct all bind to the SAME constant so
            // the four `"`-round-trip sites cannot drift.
            Atom::STR_DELIMITER => {
                chars.next();
                let mut s = String::new();
                loop {
                    match chars.next() {
                        Some((_, '\\')) => {
                            if let Some((_, esc)) = chars.next() {
                                s.push(match esc {
                                    'n' => '\n',
                                    't' => '\t',
                                    'r' => '\r',
                                    // Self-escape: `\"` unescapes to the
                                    // canonical delimiter byte. Pattern
                                    // AND value both bind to
                                    // `Atom::STR_DELIMITER` so a future
                                    // delimiter swap flips both sides in
                                    // lockstep at ONE constant.
                                    Atom::STR_DELIMITER => Atom::STR_DELIMITER,
                                    '\\' => '\\',
                                    other => other,
                                });
                            }
                        }
                        // String-closing arm — the canonical `"` byte
                        // that terminates the current `Token::Str` run.
                        // Same constant as the opener; a delimiter
                        // swap flips both arms in lockstep.
                        Some((_, Atom::STR_DELIMITER)) => break,
                        Some((_, ch)) => s.push(ch),
                        None => return Err(LispError::UnterminatedString(pos)),
                    }
                }
                out.push((Token::Str(s), pos));
            }
            _ => {
                let mut s = String::new();
                while let Some(&(_, ch)) = chars.peek() {
                    // Bare-atom terminator disjunct — the five typed
                    // gates (whitespace, `(`, `)`, `Atom::STR_DELIMITER`,
                    // `;`) plus ONE quote-family gate that end a
                    // `Token::Atom` run. Pre-lift the three
                    // quote-family disjuncts (`ch == '\''` / `ch == '`'`
                    // / `ch == ','`) were three parallel `char`-literal
                    // checks scattered across this predicate; post-
                    // lift they collapse to ONE
                    // `QuoteForm::from_lead_char(ch).is_some()` gate
                    // on the closed-set [`QuoteForm`] algebra so a
                    // regression that drifts ONE bare-atom terminator
                    // disjunct from the outer-dispatch's quote-family
                    // arm becomes structurally impossible — there is
                    // exactly ONE decode both sites consume, and
                    // adding a fifth homoiconic prefix extends
                    // [`QuoteForm::from_lead_char`] which propagates
                    // through this predicate automatically.
                    // Bare-atom terminator disjunct — the two paired
                    // list-delimiter gates ([`Sexp::LIST_OPEN`] and
                    // [`Sexp::LIST_CLOSE`]) bind to the SAME closed-
                    // set outer [`Sexp`] algebra constants the outer-
                    // dispatch arms above AND the Display impl in
                    // `ast.rs` route through. Pre-lift the two
                    // list-delimiter disjuncts (`ch == '('` /
                    // `ch == ')'`) were two parallel `char`-literal
                    // checks scattered across this predicate; post-
                    // lift they collapse to ONE constant per side on
                    // the outer algebra so a regression that drifts
                    // ONE disjunct from the outer-dispatch's list-
                    // delimiter arms becomes structurally impossible
                    // — the two paired constants are the ONE typed
                    // channel both sides consume.
                    // Bare-atom terminator disjunct — the canonical `;`
                    // byte routed through the [`Sexp::COMMENT_LEAD`]
                    // constant on the closed-set outer [`Sexp`] algebra.
                    // Pre-lift the `ch == ';'` disjunct was an inline
                    // `char`-literal check scattered alongside the two
                    // list-delimiter disjuncts; post-lift it collapses
                    // onto ONE constant on the outer algebra so a
                    // regression that drifts ONE of the two comment-
                    // boundary sites (this terminator OR the outer-
                    // dispatch arm above) from the other becomes
                    // structurally impossible — the paired constants
                    // are the ONE typed channel both sites consume.
                    if ch.is_whitespace()
                        || ch == Sexp::LIST_OPEN
                        || ch == Sexp::LIST_CLOSE
                        || QuoteForm::from_lead_char(ch).is_some()
                        || ch == Atom::STR_DELIMITER
                        || ch == Sexp::COMMENT_LEAD
                    {
                        break;
                    }
                    s.push(ch);
                    chars.next();
                }
                out.push((Token::Atom(s), pos));
            }
        }
    }
    Ok(out)
}

fn parse<I: Iterator<Item = Spanned>>(
    it: &mut std::iter::Peekable<I>,
    eof_pos: usize,
) -> Result<Sexp> {
    match it.next() {
        Some((Token::LParen, open_pos)) => {
            let mut xs = Vec::new();
            loop {
                match it.peek() {
                    Some((Token::RParen, _)) => {
                        it.next();
                        return Ok(Sexp::List(xs));
                    }
                    Some(_) => xs.push(parse(it, eof_pos)?),
                    None => return Err(LispError::UnmatchedOpenParen { pos: open_pos }),
                }
            }
        }
        Some((Token::RParen, pos)) => Err(LispError::UnmatchedParen { pos }),
        // The four pre-lift `Some((Token::Quote*, _))` arms collapse to
        // ONE arm routing through the typed `QuoteForm` marker — the
        // (Token variant, Sexp::* constructor) pairing now binds at the
        // closed-set algebra (`QuoteForm::wrap`) rather than threaded as
        // per-arm constructor literals. Adding a fifth homoiconic prefix
        // extends `QuoteForm` AND the tokenizer arm AND `QuoteForm::wrap`'s
        // match arm in lockstep — rustc binds the extension through
        // exhaustiveness over the closed enum.
        Some((Token::Quoted(qf), _)) => read_quoted(it, eof_pos, qf),
        Some((Token::Str(s), _)) => Ok(Sexp::Atom(Atom::Str(s))),
        // The five-statement classification cascade that lived in
        // `atom_from_str` lifts onto `Atom::from_lexeme` — the typed-
        // entry mirror of `fmt::Display for Atom`, `Atom::to_json`, and
        // `Atom::to_iac_forge_sexpr` on the closed-set `Atom` algebra.
        // The (lexeme prefix/suffix discipline, typed `Atom` variant)
        // pairing now binds at ONE site on the algebra rather than at
        // this reader's free function; adding a fifth structural prefix
        // (e.g. `"#["` for vector literals, `"#\\x"` for char literals)
        // extends `Atom::from_lexeme` + the matching `Atom` variant +
        // each sibling typed-exit projection in lockstep — rustc binds
        // the four projection families through exhaustiveness over the
        // closed enum. See `Atom::from_lexeme`'s docstring for the
        // composition law and the test surface.
        Some((Token::Atom(s), _)) => Ok(Sexp::Atom(Atom::from_lexeme(&s))),
        None => Err(LispError::Eof { pos: eof_pos }),
    }
}

/// Parse the datum following a quote-like prefix token (`'`, `` ` ``, `,`,
/// `,@`) and wrap it in the matching `Sexp::*` constructor projected from
/// the typed [`QuoteForm`] marker.
///
/// Centralizes the four byte-identical "read-inner-and-box" arms in
/// `parse` — one per homoiconic prefix — into ONE emission site. The
/// per-prefix (Token variant, Sexp::* constructor) pairing now binds at
/// ONE site on the substrate's `QuoteForm` closed-set algebra: the
/// parser dispatches on `Token::Quoted(qf)`, this helper reads the
/// inner datum, and [`QuoteForm::wrap`] projects the typed marker back
/// into its `Sexp::*` wrapper variant. The (marker, constructor) pair
/// lives at the typed projection ([`QuoteForm::wrap`]) rather than
/// threaded as a per-arm `fn(Box<Sexp>) -> Sexp` constructor literal
/// passed in by the caller — adding a fifth homoiconic prefix extends
/// `QuoteForm` AND its tokenizer arm AND [`QuoteForm::wrap`]'s match arm
/// in lockstep, with rustc binding the extension through exhaustiveness
/// over the closed enum.
///
/// Theory anchor: THEORY.md §VI.1 — three-times rule. The four
/// `Some((Token::Quote*, _))` arms in `parse` each ran `let inner =
/// parse(it, eof_pos)?; Ok(Sexp::*(Box::new(inner)))` — byte-identical
/// modulo the `Sexp::*` constructor. Lifted into ONE helper that routes
/// through the substrate's closed-set `QuoteForm` algebra so the next
/// change to the read-inner shape (e.g. threading source spans when
/// `Sexp` carries `pos: Option<usize>`) lands as ONE edit, not four.
/// Parallel posture to `tagged()` in `interop.rs` — the canonical-form
/// interop's analogous closed-set wrap helper across the same four
/// homoiconic variants, now also lifted onto [`QuoteForm::iac_forge_tag`].
/// Both interop helpers project from `QuoteForm` to a per-consumer
/// surface; this helper closes the FIFTH consumer site of the algebra.
fn read_quoted<I: Iterator<Item = Spanned>>(
    it: &mut std::iter::Peekable<I>,
    eof_pos: usize,
    qf: QuoteForm,
) -> Result<Sexp> {
    let inner = parse(it, eof_pos)?;
    Ok(qf.wrap(inner))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_atoms() {
        let forms = read("foo 42 2.5 \"hello\" :kw #t #f").unwrap();
        assert_eq!(forms.len(), 7);
        assert_eq!(forms[0].as_symbol(), Some("foo"));
        assert_eq!(forms[1], Sexp::int(42));
        assert_eq!(forms[2], Sexp::float(2.5));
        assert_eq!(forms[3].as_string(), Some("hello"));
        assert_eq!(forms[4].as_keyword(), Some("kw"));
        assert_eq!(forms[5], Sexp::boolean(true));
        assert_eq!(forms[6], Sexp::boolean(false));
    }

    #[test]
    fn reads_nested_lists() {
        let f = read("(defpoint obs :class (Gate Observability))").unwrap();
        assert_eq!(f.len(), 1);
        let outer = f[0].as_list().unwrap();
        assert_eq!(outer[0].as_symbol(), Some("defpoint"));
        assert_eq!(outer[1].as_symbol(), Some("obs"));
        assert_eq!(outer[2].as_keyword(), Some("class"));
        let inner = outer[3].as_list().unwrap();
        assert_eq!(inner[0].as_symbol(), Some("Gate"));
        assert_eq!(inner[1].as_symbol(), Some("Observability"));
    }

    #[test]
    fn handles_comments() {
        let f = read("; top-level comment\n(a b) ; inline\n(c)").unwrap();
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn string_escapes() {
        let f = read(r#""line\nbreak\ttab""#).unwrap();
        assert_eq!(f[0].as_string(), Some("line\nbreak\ttab"));
    }

    #[test]
    fn quote_form() {
        let f = read("'(a b)").unwrap();
        match &f[0] {
            Sexp::Quote(inner) => assert!(inner.is_list()),
            _ => panic!("expected quote"),
        }
    }

    #[test]
    fn unmatched_paren_errors() {
        assert!(read("(a b").is_err());
        assert!(read(")").is_err());
    }

    // ── Source-position fidelity ────────────────────────────────────────
    //
    // The three reader-level errors carry byte offsets so authoring tools
    // can render them at the right place. Pin the offsets here so a
    // regression that re-loses position info (e.g. by reverting tokens to
    // unspanned values) surfaces immediately.

    #[test]
    fn unmatched_closing_paren_reports_byte_offset() {
        // `   )` — the stray `)` is at byte 3.
        let err = read("   )").unwrap_err();
        match err {
            LispError::UnmatchedParen { pos } => assert_eq!(pos, 3),
            other => panic!("expected UnmatchedParen, got {other:?}"),
        }
    }

    #[test]
    fn unmatched_opening_paren_reports_offset_of_open() {
        // `(a (b c` — the inner `(` is at byte 3 and stays unclosed; the
        // outer `(` at byte 0 is also unclosed but the deepest unclosed
        // open is what the parser hits first.
        let err = read("(a (b c").unwrap_err();
        match err {
            LispError::UnmatchedOpenParen { pos } => assert_eq!(pos, 3),
            other => panic!("expected UnmatchedOpenParen, got {other:?}"),
        }
    }

    #[test]
    fn outer_unmatched_open_reports_outer_offset() {
        // `(a b` — only the outer `(` at byte 0 is open.
        let err = read("(a b").unwrap_err();
        match err {
            LispError::UnmatchedOpenParen { pos } => assert_eq!(pos, 0),
            other => panic!("expected UnmatchedOpenParen, got {other:?}"),
        }
    }

    #[test]
    fn dangling_quote_reports_eof_at_input_length() {
        // A trailing `'` with no datum to quote — parse runs off the end.
        let src = "(a b) '";
        let err = read(src).unwrap_err();
        match err {
            LispError::Eof { pos } => assert_eq!(pos, src.len()),
            other => panic!("expected Eof, got {other:?}"),
        }
    }

    #[test]
    fn error_display_includes_position() {
        // The user-facing string must mention the position so downstream
        // tools and humans can act on it without inspecting the variant.
        let err = read(")  ").unwrap_err();
        let rendered = format!("{err}");
        assert!(
            rendered.contains("position 0"),
            "expected position in display, got {rendered:?}"
        );
    }

    // ── read_quoted helper: closed-set quote-prefix dispatch ────────────
    //
    // The four homoiconic prefix tokens (`'`, `` ` ``, `,`, `,@`) each
    // funnel through `read_quoted`, which takes the matching `Sexp::*`
    // tuple-variant constructor as a `fn(Box<Sexp>) -> Sexp` and wraps
    // the parsed inner. The arms below pin each prefix's identity AND
    // path-uniformity: every prefix produces ONE corresponding `Sexp`
    // variant, with the inner Sexp unchanged. A regression that swaps
    // two constructors (e.g. `Sexp::Quasiquote` instead of `Sexp::Quote`)
    // OR that drops the `Box::new` round-trip fails loudly here.

    #[test]
    fn quote_prefix_round_trips_through_read_quoted_into_sexp_quote() {
        // `'foo` — the standalone quote prefix wraps the next datum.
        let f = read("'foo").unwrap();
        assert_eq!(f.len(), 1);
        match &f[0] {
            Sexp::Quote(inner) => assert_eq!(inner.as_symbol(), Some("foo")),
            other => panic!("expected Sexp::Quote, got {other:?}"),
        }
    }

    #[test]
    fn quasiquote_prefix_round_trips_through_read_quoted_into_sexp_quasiquote() {
        // `` `foo `` — the quasiquote prefix is the macro-template entry.
        let f = read("`foo").unwrap();
        assert_eq!(f.len(), 1);
        match &f[0] {
            Sexp::Quasiquote(inner) => assert_eq!(inner.as_symbol(), Some("foo")),
            other => panic!("expected Sexp::Quasiquote, got {other:?}"),
        }
    }

    #[test]
    fn unquote_prefix_round_trips_through_read_quoted_into_sexp_unquote() {
        // `,foo` — the unquote prefix is the substitution marker inside a
        // quasiquote. Pin that the bare `,` (not `,@`) dispatches to
        // `Sexp::Unquote`, NOT to `Sexp::UnquoteSplice` — the tokenizer's
        // `,`-then-peek-`@` discriminator must round-trip cleanly through
        // the `read_quoted` helper without crossing wires.
        let f = read(",foo").unwrap();
        assert_eq!(f.len(), 1);
        match &f[0] {
            Sexp::Unquote(inner) => assert_eq!(inner.as_symbol(), Some("foo")),
            other => panic!("expected Sexp::Unquote, got {other:?}"),
        }
    }

    #[test]
    fn unquote_splice_prefix_round_trips_through_read_quoted_into_sexp_unquote_splice() {
        // `,@foo` — the unquote-splice prefix flattens a bound list into
        // its containing list. Pin that the tokenizer's two-char marker
        // `,@` dispatches to `Sexp::UnquoteSplice`, distinct from
        // `Sexp::Unquote` above — both share the helper's read-and-wrap
        // shape but differ on the constructor passed in. A regression
        // that conflates the two fails loudly here.
        let f = read(",@xs").unwrap();
        assert_eq!(f.len(), 1);
        match &f[0] {
            Sexp::UnquoteSplice(inner) => assert_eq!(inner.as_symbol(), Some("xs")),
            other => panic!("expected Sexp::UnquoteSplice, got {other:?}"),
        }
    }

    #[test]
    fn quote_prefix_recursively_wraps_via_read_quoted_for_nested_homoiconic_forms() {
        // `',foo` — quote of an unquote. The outer prefix invokes
        // `read_quoted` with `Sexp::Quote`; the inner prefix invokes
        // `read_quoted` with `Sexp::Unquote`. Pin recursion: the helper's
        // single emission site is reentrant via the recursive `parse`
        // call, so nested homoiconic forms compose without each variant
        // needing its own special-case re-entry. A regression that flattens
        // or short-circuits the recursion fails here.
        let f = read("',foo").unwrap();
        assert_eq!(f.len(), 1);
        match &f[0] {
            Sexp::Quote(outer) => match outer.as_ref() {
                Sexp::Unquote(inner) => assert_eq!(inner.as_symbol(), Some("foo")),
                other => panic!("expected inner Sexp::Unquote, got {other:?}"),
            },
            other => panic!("expected outer Sexp::Quote, got {other:?}"),
        }
    }

    #[test]
    fn read_quoted_propagates_inner_parse_error_unchanged() {
        // `'` with no following datum — the helper's inner `parse` call
        // returns `LispError::Eof`. Pin that `read_quoted` propagates
        // that error verbatim (no rewrap, no swallow) so the user-facing
        // diagnostic still names the EOF byte offset; a regression that
        // catches-and-translates the inner error here would surface as
        // a degraded position-less diagnostic.
        let src = "'";
        let err = read(src).unwrap_err();
        match err {
            LispError::Eof { pos } => assert_eq!(pos, src.len()),
            other => panic!("expected Eof, got {other:?}"),
        }
    }

    #[test]
    fn reader_threads_each_prefix_through_quote_form_wrap_dual_of_as_quote_form() {
        // END-TO-END CLOSED-SET CONTRACT: pin that the reader's
        // prefix-dispatch routes through the substrate's typed
        // `QuoteForm` algebra at every step. For each of the four
        // homoiconic prefixes (`'`, `` ` ``, `,`, `,@`) the read path
        // produces a `Sexp::*` value byte-identical to what
        // `expected_qf.wrap(inner)` builds — pinning that the reader's
        // `Token::Quoted(qf) → read_quoted(it, eof_pos, qf) →
        // qf.wrap(inner)` pipeline binds to the same algebra every
        // other consumer (Hash, Display, as_unquote, iac-forge interop)
        // routes through. A regression that bypasses `QuoteForm::wrap`
        // (e.g. reverts to per-arm `Sexp::*` constructor literals) AND
        // accidentally swaps two constructors silently corrupts every
        // program's quote-family parse; this test catches the drift via
        // the typed marker projection.
        let inner = Sexp::symbol("payload");
        for (src, expected_qf) in [
            ("'payload", QuoteForm::Quote),
            ("`payload", QuoteForm::Quasiquote),
            (",payload", QuoteForm::Unquote),
            (",@payload", QuoteForm::UnquoteSplice),
        ] {
            let forms = read(src).expect(src);
            assert_eq!(forms.len(), 1, "{src} must produce one form");

            // (1) Read result equals `expected_qf.wrap(inner.clone())` —
            // pin the reader→wrap dual end-to-end.
            assert_eq!(
                forms[0],
                expected_qf.wrap(inner.clone()),
                "{src} drifted from QuoteForm::wrap dual"
            );

            // (2) Project the read form back through `as_quote_form` to
            // confirm the typed marker matches `expected_qf` and the
            // inner body is preserved — pin the round-trip law
            // `read(qf.prefix() + inner.repr) →[as_quote_form] (qf, inner)`.
            let (qf, body) = forms[0]
                .as_quote_form()
                .unwrap_or_else(|| panic!("{src} must project through as_quote_form"));
            assert_eq!(qf, expected_qf, "{src} produced wrong typed marker");
            assert_eq!(body, &inner, "{src} drifted inner body");
        }
    }

    #[test]
    fn token_quoted_arms_carry_typed_quote_form_marker_for_every_prefix() {
        // CLOSED-SET TOKENIZATION CONTRACT: pin that each homoiconic
        // prefix tokenizes to `Token::Quoted(QuoteForm::*)` with the
        // matching closed-set variant. The pre-lift reader carried a
        // parallel `Token::{Quote, Quasiquote, Unquote, UnquoteSplice}`
        // closed set; post-lift the tokenizer binds directly to the
        // substrate's `QuoteForm` algebra. A regression that drifts the
        // (prefix char, QuoteForm variant) pairing inside the tokenizer
        // (e.g. routes `'` to `QuoteForm::Quasiquote`) surfaces here
        // because the spanned-token output exposes the typed marker
        // directly — no intermediate constructor function to obscure
        // the (prefix → marker) intent.
        for (src, expected_qf, expected_span) in [
            ("'", QuoteForm::Quote, 0usize),
            ("`", QuoteForm::Quasiquote, 0),
            (",", QuoteForm::Unquote, 0),
            (",@", QuoteForm::UnquoteSplice, 0),
        ] {
            let tokens = tokenize(src).expect(src);
            assert_eq!(tokens.len(), 1, "{src} must produce one token");
            match &tokens[0] {
                (Token::Quoted(qf), pos) => {
                    assert_eq!(*qf, expected_qf, "{src} drifted typed marker");
                    assert_eq!(*pos, expected_span, "{src} drifted span position");
                }
                other => panic!("{src} expected Token::Quoted, got {other:?}"),
            }
        }
    }

    #[test]
    fn token_quoted_unquote_splice_two_char_marker_collapses_to_single_token() {
        // TOKEN-MERGE CONTRACT: pin that the two-char `,@` prefix
        // collapses to ONE `Token::Quoted(QuoteForm::UnquoteSplice)`
        // token, not two adjacent tokens — and that the bare `,` (with
        // no following `@`) projects to `QuoteForm::Unquote`. The
        // tokenizer's peek-then-consume `@`-discriminator is load-
        // bearing for the closed-set dispatch; a regression that
        // emits two tokens for `,@` would route through `Unquote`
        // followed by an `@` atom, silently re-shaping every splice
        // form's parse.
        let tokens = tokenize(",@xs").expect(",@xs");
        assert_eq!(tokens.len(), 2, ",@xs must tokenize as splice + atom");
        assert!(
            matches!(tokens[0], (Token::Quoted(QuoteForm::UnquoteSplice), 0)),
            "expected ,@ token at position 0, got {:?}",
            tokens[0]
        );
        assert!(
            matches!(tokens[1], (Token::Atom(_), 2)),
            "expected atom token at position 2, got {:?}",
            tokens[1]
        );

        let tokens_bare = tokenize(",xs").expect(",xs");
        assert_eq!(tokens_bare.len(), 2, ",xs must tokenize as unquote + atom");
        assert!(
            matches!(tokens_bare[0], (Token::Quoted(QuoteForm::Unquote), 0)),
            "expected , token at position 0, got {:?}",
            tokens_bare[0]
        );
    }

    #[test]
    fn read_quoted_propagates_unmatched_open_paren_for_quoted_list() {
        // `'(a b` — the helper recurses into a list-parse that hits
        // `UnmatchedOpenParen`. Pin that the inner error propagates
        // unchanged. Same posture as
        // `read_quoted_propagates_inner_parse_error_unchanged` but for
        // a different inner failure mode — the helper's wrap step is
        // strictly typed-OK over Result, and any inner error short-
        // circuits without re-wrapping.
        let err = read("'(a b").unwrap_err();
        match err {
            LispError::UnmatchedOpenParen { pos } => assert_eq!(pos, 1),
            other => panic!("expected UnmatchedOpenParen, got {other:?}"),
        }
    }

    #[test]
    fn reader_atom_token_arm_routes_through_atom_from_lexeme_for_every_kind() {
        // LIFTED-BOUNDARY CONTRACT: pin that the reader's
        // `Token::Atom(s)` arm produces the SAME `Sexp::Atom(_)` value
        // for every canonical bare-atom source lexeme that
        // `Sexp::Atom(Atom::from_lexeme(s))` would construct directly.
        // Pre-lift the per-variant classification cascade lived inline
        // at the reader's private `atom_from_str` helper; post-lift
        // the cascade lives on the typed `Atom` algebra as
        // `Atom::from_lexeme`, and the reader's arm delegates through
        // ONE `Sexp::Atom(Atom::from_lexeme(&s))` call. A regression
        // that drifts the outer arm (e.g. wraps the typed `Atom`
        // through a different `Sexp` constructor, or short-circuits
        // one variant inline at the reader rather than delegating
        // through the algebra) surfaces as an inequality here.
        //
        // Sweeps every `AtomKind` variant the bare-atom branch can
        // produce — `Symbol`, `Keyword`, `Int`, `Float`, `Bool` —
        // alongside the structural-prefix non-matches (default-arm
        // Symbol classification) AND the load-bearing
        // `i64`-before-`f64` cascade order. `AtomKind::Str` is absent
        // because string literals take the reader's distinct
        // `Token::Str(_)` branch, NOT the `Token::Atom(_)` branch this
        // arm covers. Sibling-shape pin to
        // `sexp_atom_display_arm_routes_through_atom_display_for_every_variant`
        // (in `crate::ast::tests`) — that pins the analogous
        // typed-exit Display arm routing through `Atom::Display`; this
        // pins the analogous typed-entry classification arm routing
        // through `Atom::from_lexeme`. The two together complete the
        // bidirectional sweep across all FOUR per-`Atom`-variant
        // production-site projection arms (Display, JSON,
        // iac-forge canonical attestation, AND now lexeme
        // classification) onto the closed-set `Atom` algebra.
        let cases: &[&str] = &[
            "foo", "defpoint", "seph.1", ":parent", ":kw", "42", "-7", "0", "1", "1.0", "1.5",
            "-2.5", "1e3", "#t", "#f",
            "true",  // CLAUDE.md "Lisp bools" — must classify to Symbol, not Bool.
            "false", // CLAUDE.md "Lisp bools" — must classify to Symbol, not Bool.
            "+", "a-b",
        ];
        for src in cases {
            let forms = read(src).unwrap_or_else(|e| panic!("reader rejected {src:?}: {e}"));
            assert_eq!(
                forms.len(),
                1,
                "{src:?} must read as exactly one form, got {forms:?}"
            );
            let via_reader = &forms[0];
            let via_algebra = Sexp::Atom(Atom::from_lexeme(src));
            assert_eq!(
                via_reader, &via_algebra,
                "{src:?}: reader's bare-atom arm drifted from Sexp::Atom(Atom::from_lexeme(_))"
            );
        }
    }

    // ── `Atom::STR_DELIMITER` — the reader's four `"`-round-trip sites
    // bind to ONE canonical `char` constant on the closed-set [`Atom`]
    // algebra. The composition pins below anchor each of the four sites
    // (opener, self-escape mapping, closer, bare-atom terminator) at
    // the constant so a regression that re-inlines one site's byte
    // fails at rustc / test time rather than as a silent tokenizer
    // drift.

    #[test]
    fn reader_str_open_close_arms_bind_to_atom_str_delimiter() {
        // TOKENIZE-BOUNDARY CONTRACT: the outer-match string-opening
        // arm AND the inner-loop string-closing arm both bind to
        // `Atom::STR_DELIMITER`. A source string composed of the
        // constant + payload + constant tokenizes to ONE
        // `Token::Str(payload)` — pinning that the opener/closer
        // pairing routes through the same algebra constant. A
        // regression that swaps one of the two arms to a different
        // char fails HERE at the token-count assertion (opener
        // without matching closer would return `UnterminatedString`
        // or over-read into the next token).
        let payload = "hello world";
        let source = format!("{}{payload}{}", Atom::STR_DELIMITER, Atom::STR_DELIMITER,);
        let tokens = tokenize(&source).unwrap_or_else(|e| {
            panic!("tokenize rejected STR_DELIMITER-wrapped source `{source}`: {e}")
        });
        assert_eq!(
            tokens.len(),
            1,
            "STR_DELIMITER-wrapped payload must tokenize as exactly one \
             Token::Str, got {tokens:?}",
        );
        match &tokens[0] {
            (Token::Str(s), 0) => assert_eq!(
                s, payload,
                "Token::Str body drifted from STR_DELIMITER-wrapped payload"
            ),
            other => panic!(
                "expected Token::Str at position 0, got {other:?} — the \
                 opener/closer arms in tokenize must both bind to \
                 Atom::STR_DELIMITER"
            ),
        }
    }

    #[test]
    fn reader_str_escape_self_escape_arm_routes_through_atom_str_delimiter() {
        // ESCAPE-HANDLER SELF-ESCAPE CONTRACT: the reader's five-arm
        // escape table (`\n`, `\t`, `\r`, `\"`, `\\`, plus a
        // passthrough default) carries a single self-escape arm on
        // the Str-delimiter axis — `\"` unescapes to the constant's
        // byte. Both the pattern AND the mapped value bind to
        // `Atom::STR_DELIMITER` so a delimiter swap flips both sides
        // in lockstep. Pin the composition end-to-end: a source
        // containing `\"` inside a wrapped payload must round-trip
        // through the tokenizer AND the reader to a
        // `Sexp::Atom(Atom::string(str_delim.to_string()))` value.
        let escape_source = format!(
            "{}\\{}{}",
            Atom::STR_DELIMITER,
            Atom::STR_DELIMITER,
            Atom::STR_DELIMITER,
        );
        let forms = read(&escape_source)
            .unwrap_or_else(|e| panic!("reader rejected escape source `{escape_source}`: {e}"));
        assert_eq!(forms.len(), 1, "escape source must read as one form");
        assert_eq!(
            forms[0],
            Sexp::Atom(Atom::string(Atom::STR_DELIMITER.to_string())),
            "self-escape `\\\"` inside STR_DELIMITER-wrapped payload \
             drifted from Sexp::Atom(Atom::string(str_delim)) — the \
             escape-handler's self-escape arm's pattern AND mapped value \
             must both route through Atom::STR_DELIMITER",
        );
    }

    #[test]
    fn tokenizer_quote_family_outer_dispatch_routes_through_quote_form_from_lead_char() {
        // OUTER-DISPATCH CONTRACT: the reader's pre-lift four inline
        // `char`-literal arms (`'\''` / `` '`' `` / `','`) collapsed
        // to ONE pre-match `QuoteForm::from_lead_char(c)` decode; the
        // splice promotion moved to the reader's peek arm inside the
        // shared `,`-decoded `QuoteForm::Unquote` branch. Pin the
        // composition end-to-end by sweeping every `QuoteForm` variant
        // through the tokenizer: for each variant, construct a source
        // string starting with the variant's rendered `prefix()`
        // followed by a bare `xs` atom, and assert the tokenizer emits
        // `Token::Quoted(qf)` at the expected typed marker. A regression
        // that drifted the outer-dispatch (e.g. reverted to inline
        // `char`-literal arms and got the `,@` / `,` order backwards)
        // fails HERE at the typed-marker assertion.
        for qf in QuoteForm::ALL {
            let source = format!("{}xs", qf.prefix());
            let tokens = tokenize(&source).unwrap_or_else(|e| {
                panic!("tokenize rejected `{source}` for QuoteForm::{qf:?}: {e}")
            });
            assert!(
                !tokens.is_empty(),
                "QuoteForm::{qf:?} — tokenizer must emit at least one token for `{source}`",
            );
            match &tokens[0] {
                (Token::Quoted(marker), 0) => assert_eq!(
                    *marker, qf,
                    "QuoteForm::{qf:?} — outer-dispatch marker drifted from decoded variant"
                ),
                other => {
                    panic!("QuoteForm::{qf:?} — expected Token::Quoted at pos 0, got {other:?}")
                }
            }
        }
    }

    #[test]
    fn tokenizer_shared_comma_lead_char_disambiguates_at_peek_arm_not_at_from_lead_char() {
        // SHARED-LEAD-CHAR PROMOTION CONTRACT: `,` is the sole shared
        // lead char across the closed set — `QuoteForm::from_lead_char`
        // returns `Some(QuoteForm::Unquote)` on `,`, and the
        // tokenizer's peek-then-consume `@` arm PROMOTES the decoded
        // `Unquote` to `UnquoteSplice` when the second char is `@`.
        // Pin the promotion asymmetry by tokenizing all three forms
        // side-by-side:
        //   * `,x`  — bare unquote, one `Token::Quoted(Unquote)`.
        //   * `,@x` — splice, one `Token::Quoted(UnquoteSplice)`.
        //   * `,@`  — splice alone, one `Token::Quoted(UnquoteSplice)`.
        // A regression that pushed the splice promotion INTO
        // `from_lead_char` (a natural but wrong refactor once the
        // outer dispatch collapses onto a single arm) would silently
        // re-route every bare `,` through the splice arm; this triple
        // catches the drift by pinning each shape's typed marker.
        let bare = tokenize(",x").unwrap_or_else(|e| panic!("tokenize `,x` failed: {e}"));
        assert!(
            matches!(bare[0], (Token::Quoted(QuoteForm::Unquote), 0)),
            "`,x` — first token must be Token::Quoted(Unquote) at pos 0, got {:?}",
            bare[0],
        );

        let splice = tokenize(",@x").unwrap_or_else(|e| panic!("tokenize `,@x` failed: {e}"));
        assert!(
            matches!(splice[0], (Token::Quoted(QuoteForm::UnquoteSplice), 0)),
            "`,@x` — first token must be Token::Quoted(UnquoteSplice) at pos 0, got {:?}",
            splice[0],
        );

        let splice_alone = tokenize(",@").unwrap_or_else(|e| panic!("tokenize `,@` failed: {e}"));
        assert_eq!(splice_alone.len(), 1, "`,@` must tokenize to one token");
        assert!(
            matches!(
                splice_alone[0],
                (Token::Quoted(QuoteForm::UnquoteSplice), 0)
            ),
            "`,@` — token must be Token::Quoted(UnquoteSplice) at pos 0, got {:?}",
            splice_alone[0],
        );
    }

    #[test]
    fn tokenizer_bare_atom_terminator_disjunct_routes_through_quote_form_from_lead_char() {
        // BARE-ATOM TERMINATOR CONTRACT: the pre-lift three parallel
        // `char`-literal disjuncts (`ch == '\''` / `ch == '`'` /
        // `ch == ','`) collapsed to ONE
        // `QuoteForm::from_lead_char(ch).is_some()` gate. Pin the
        // composition by sweeping every quote-family lead char and
        // asserting a bare-atom lexeme followed by that lead char
        // tokenizes as TWO distinct tokens — first the atom lexeme,
        // then the `Token::Quoted(qf)` at the exact byte offset the
        // atom terminator broke at. A regression that dropped one
        // disjunct (e.g. re-inlined only two of the three lead chars,
        // or drifted one from the substrate's `from_lead_char`
        // projection) would silently absorb the lead char into the
        // bare-atom accumulator and swallow the subsequent quote-family
        // token — this sweep catches the drift for every arm.
        for qf in [QuoteForm::Quote, QuoteForm::Quasiquote, QuoteForm::Unquote] {
            let source = format!("foo{}xs", qf.prefix());
            let tokens = tokenize(&source).unwrap_or_else(|e| {
                panic!(
                    "tokenize rejected `{source}` for terminator sweep of QuoteForm::{qf:?}: {e}"
                )
            });
            assert!(
                tokens.len() >= 2,
                "QuoteForm::{qf:?} — bare atom + prefix must tokenize as at least TWO \
                 tokens, got {tokens:?}",
            );
            assert!(
                matches!(&tokens[0], (Token::Atom(s), 0) if s == "foo"),
                "QuoteForm::{qf:?} — first token must be Token::Atom(\"foo\") at pos 0, \
                 got {:?}",
                tokens[0],
            );
            assert!(
                matches!(&tokens[1], (Token::Quoted(marker), 3) if *marker == qf),
                "QuoteForm::{qf:?} — second token must be Token::Quoted(qf) at pos 3, \
                 got {:?}",
                tokens[1],
            );
        }
    }

    // ── `Sexp::LIST_OPEN` / `Sexp::LIST_CLOSE` — the paired canonical
    // `(` / `)` chars routed through the FOUR outer-structural round-
    // trip sites: two `crate::reader::tokenize` outer-dispatch arms
    // (`Token::LParen`, `Token::RParen`) AND two bare-atom terminator
    // disjuncts. The composition pins below anchor each site at the
    // typed constant so a regression that re-inlines one site's byte
    // fails at rustc / test time rather than as a silent tokenizer
    // drift. Sibling-shape tests to the `reader_str_*` block above
    // (Str-payload delimiter axis), lifted onto the outer-structural
    // [`Sexp`] algebra.

    #[test]
    fn tokenizer_list_open_close_arms_bind_to_sexp_list_delimiter_constants() {
        // OUTER-DISPATCH CONTRACT: pin that a source composed of the
        // two typed constants + payload atoms tokenizes to the
        // expected `Token::LParen` + payload-token(s) + `Token::RParen`
        // sequence. A regression that drifts ONE of the two arms to a
        // different `char` fails HERE at the outer-token identity
        // and byte-offset assertions. Sibling-shape pin to
        // `reader_str_open_close_arms_bind_to_atom_str_delimiter`
        // (the Str-payload delimiter axis).
        let source = format!("{}foo bar{}", Sexp::LIST_OPEN, Sexp::LIST_CLOSE);
        let tokens = tokenize(&source).unwrap_or_else(|e| {
            panic!("tokenize rejected LIST_OPEN/CLOSE-wrapped source `{source}`: {e}")
        });
        assert_eq!(
            tokens.len(),
            4,
            "LIST_OPEN/CLOSE-wrapped `foo bar` payload must tokenize as \
             LParen + Atom(foo) + Atom(bar) + RParen (4 tokens), got \
             {tokens:?}",
        );
        assert!(
            matches!(tokens[0], (Token::LParen, 0)),
            "expected Token::LParen at position 0 (from LIST_OPEN outer \
             arm), got {:?}",
            tokens[0],
        );
        assert!(
            matches!(tokens[3], (Token::RParen, _)),
            "expected Token::RParen at last position (from LIST_CLOSE \
             outer arm), got {:?}",
            tokens[3],
        );
    }

    #[test]
    fn tokenizer_bare_atom_terminator_disjunct_binds_to_sexp_list_delimiter_constants() {
        // BARE-ATOM TERMINATOR CONTRACT: pin that a bare atom lexeme
        // followed by either list delimiter tokenizes as TWO distinct
        // tokens — first the atom lexeme, then the corresponding
        // paren token at the byte offset the atom terminator broke
        // at. A regression that drops one of the two disjuncts (e.g.
        // re-inlines `'('` at one site and misses the terminator
        // update on the other) would silently absorb the paren into
        // the bare-atom accumulator and swallow the subsequent
        // structural token — this pair catches the drift for both
        // sides at once. Sibling-shape pin to
        // `reader_bare_atom_terminator_disjunct_binds_to_atom_str_delimiter`
        // (the Str-payload delimiter axis).
        for (delim, expected_second_tok_matcher_name) in
            [(Sexp::LIST_OPEN, "LParen"), (Sexp::LIST_CLOSE, "RParen")]
        {
            let source = format!("foo{delim}");
            let tokens = tokenize(&source).unwrap_or_else(|e| {
                panic!("tokenize rejected `{source}` for LIST delimiter `{delim}`: {e}")
            });
            assert_eq!(
                tokens.len(),
                2,
                "bare atom + list delimiter `{delim}` must tokenize as \
                 TWO distinct tokens, got {tokens:?}",
            );
            assert!(
                matches!(&tokens[0], (Token::Atom(s), 0) if s == "foo"),
                "first token must be Token::Atom(\"foo\") at position 0, \
                 got {:?} — the bare-atom terminator disjunct did NOT \
                 break at LIST delimiter `{delim}`",
                tokens[0],
            );
            let second_is_expected = match delim {
                Sexp::LIST_OPEN => matches!(&tokens[1], (Token::LParen, 3)),
                Sexp::LIST_CLOSE => matches!(&tokens[1], (Token::RParen, 3)),
                _ => unreachable!("delim must be LIST_OPEN or LIST_CLOSE"),
            };
            assert!(
                second_is_expected,
                "second token must be Token::{expected_second_tok_matcher_name} at \
                 position 3, got {:?}",
                tokens[1],
            );
        }
    }

    #[test]
    fn reader_bare_atom_terminator_disjunct_binds_to_atom_str_delimiter() {
        // BARE-ATOM TERMINATOR CONTRACT: the bare-atom tokenizer's
        // termination disjunct (`ch == Atom::STR_DELIMITER`) ensures
        // a bare atom followed by a string (e.g. `foo"body"`)
        // tokenizes as TWO distinct tokens — a `Token::Atom` for the
        // symbol payload preceding the delimiter, then a
        // `Token::Str` for the delimited payload. A regression that
        // drops the disjunct (e.g. reverting `Atom::STR_DELIMITER`
        // to a stale `'"'` literal, or renaming the constant without
        // updating the reader arm) would silently absorb the `"`
        // byte into the bare-atom accumulator and never emit the
        // `Token::Str`.
        let source = format!("foo{}body{}", Atom::STR_DELIMITER, Atom::STR_DELIMITER);
        let tokens =
            tokenize(&source).unwrap_or_else(|e| panic!("tokenize rejected `{source}`: {e}"));
        assert_eq!(
            tokens.len(),
            2,
            "bare atom + STR_DELIMITER-wrapped payload must tokenize as \
             TWO distinct tokens, got {tokens:?}",
        );
        assert!(
            matches!(&tokens[0], (Token::Atom(s), 0) if s == "foo"),
            "first token must be Token::Atom(\"foo\") at position 0, got {:?}",
            tokens[0],
        );
        assert!(
            matches!(&tokens[1], (Token::Str(s), 3) if s == "body"),
            "second token must be Token::Str(\"body\") at position 3, got {:?}",
            tokens[1],
        );
    }

    // ── `Sexp::COMMENT_LEAD` — the reader's TWO comment-boundary sites
    // bind to ONE canonical `char` constant on the closed-set outer
    // [`Sexp`] algebra. The composition pins below anchor each of the
    // two sites (line-comment outer-dispatch arm, bare-atom terminator
    // disjunct) at the constant so a regression that re-inlines one
    // site's byte fails at rustc / test time rather than as a silent
    // tokenizer drift. Sibling-shape tests to the
    // `tokenizer_list_open_close_arms_*` /
    // `tokenizer_bare_atom_terminator_disjunct_binds_to_sexp_list_delimiter_constants`
    // block above (outer-structural paired-delimiter axis), lifted onto
    // the reader-discard axis of the closed-set outer [`Sexp`] algebra.

    #[test]
    fn tokenizer_line_comment_outer_dispatch_arm_binds_to_sexp_comment_lead() {
        // OUTER-DISPATCH CONTRACT: pin that a source composed of the
        // typed constant + comment body + `\n` + trailing atom
        // tokenizes to exactly the trailing atom — the discarded
        // line-comment run consumes every byte between the constant AND
        // (up to and including) the `\n` and emits NO token. A
        // regression that drifts the arm to a different `char` (e.g.
        // re-inlines `';'` at the arm while the terminator migrates to
        // a new byte) fails HERE at the token-count assertion (the
        // comment body would leak into the token stream as one or more
        // `Token::Atom`s).
        let source = format!("{}comment body here\nfoo", Sexp::COMMENT_LEAD,);
        let tokens = tokenize(&source).unwrap_or_else(|e| {
            panic!("tokenize rejected COMMENT_LEAD-led source `{source}`: {e}")
        });
        assert_eq!(
            tokens.len(),
            1,
            "COMMENT_LEAD-led line-comment must be discarded, leaving ONLY \
             the trailing atom in the token stream, got {tokens:?}",
        );
        assert!(
            matches!(&tokens[0], (Token::Atom(s), _) if s == "foo"),
            "sole surviving token must be Token::Atom(\"foo\"), got {:?} \
             — the line-comment outer-dispatch arm did NOT discard the \
             COMMENT_LEAD-led run",
            tokens[0],
        );
    }

    #[test]
    fn tokenizer_bare_atom_terminator_disjunct_binds_to_sexp_comment_lead() {
        // BARE-ATOM TERMINATOR CONTRACT: pin that a bare atom lexeme
        // immediately followed by the typed constant tokenizes as
        // exactly ONE `Token::Atom` — the atom lexeme preceding the
        // byte — with the subsequent line-comment run discarded by the
        // outer-dispatch arm. A regression that drops the disjunct
        // (e.g. reverts `Sexp::COMMENT_LEAD` to a stale `';'` literal,
        // or renames the constant without updating the reader arm)
        // would silently absorb the `;` byte into the bare-atom
        // accumulator and consume every char up to whitespace / paren /
        // string / quote-family / eof as ONE `Token::Atom` payload.
        // Sibling-shape pin to
        // `reader_bare_atom_terminator_disjunct_binds_to_atom_str_delimiter`
        // on the Str-payload delimiter axis, AND to
        // `tokenizer_bare_atom_terminator_disjunct_binds_to_sexp_list_delimiter_constants`
        // on the outer-structural paired-delimiter axis.
        let source = format!("foo{}bar rest", Sexp::COMMENT_LEAD);
        let tokens =
            tokenize(&source).unwrap_or_else(|e| panic!("tokenize rejected `{source}`: {e}"));
        assert_eq!(
            tokens.len(),
            1,
            "bare atom + COMMENT_LEAD + line-comment body must tokenize \
             as EXACTLY one token (the leading atom; the comment body is \
             discarded), got {tokens:?}",
        );
        assert!(
            matches!(&tokens[0], (Token::Atom(s), 0) if s == "foo"),
            "sole surviving token must be Token::Atom(\"foo\") at position \
             0, got {:?} — the bare-atom terminator disjunct did NOT \
             break at COMMENT_LEAD",
            tokens[0],
        );
    }
}
