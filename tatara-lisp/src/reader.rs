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
        match c {
            ws if ws.is_whitespace() => {
                chars.next();
            }
            ';' => {
                while let Some(&(_, ch)) = chars.peek() {
                    chars.next();
                    if ch == '\n' {
                        break;
                    }
                }
            }
            '(' => {
                chars.next();
                out.push((Token::LParen, pos));
            }
            ')' => {
                chars.next();
                out.push((Token::RParen, pos));
            }
            '\'' => {
                chars.next();
                out.push((Token::Quoted(QuoteForm::Quote), pos));
            }
            '`' => {
                chars.next();
                out.push((Token::Quoted(QuoteForm::Quasiquote), pos));
            }
            ',' => {
                chars.next();
                // `,@` is splicing unquote; bare `,` is unquote.
                if chars.peek().map(|&(_, c)| c) == Some('@') {
                    chars.next();
                    out.push((Token::Quoted(QuoteForm::UnquoteSplice), pos));
                } else {
                    out.push((Token::Quoted(QuoteForm::Unquote), pos));
                }
            }
            '"' => {
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
                                    '"' => '"',
                                    '\\' => '\\',
                                    other => other,
                                });
                            }
                        }
                        Some((_, '"')) => break,
                        Some((_, ch)) => s.push(ch),
                        None => return Err(LispError::UnterminatedString(pos)),
                    }
                }
                out.push((Token::Str(s), pos));
            }
            _ => {
                let mut s = String::new();
                while let Some(&(_, ch)) = chars.peek() {
                    if ch.is_whitespace()
                        || ch == '('
                        || ch == ')'
                        || ch == '\''
                        || ch == '`'
                        || ch == ','
                        || ch == '"'
                        || ch == ';'
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
        Some((Token::Atom(s), _)) => Ok(atom_from_str(&s)),
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

fn atom_from_str(s: &str) -> Sexp {
    if s == "#t" {
        return Sexp::boolean(true);
    }
    if s == "#f" {
        return Sexp::boolean(false);
    }
    if let Some(rest) = s.strip_prefix(':') {
        return Sexp::keyword(rest);
    }
    if let Ok(n) = s.parse::<i64>() {
        return Sexp::int(n);
    }
    if let Ok(n) = s.parse::<f64>() {
        return Sexp::float(n);
    }
    Sexp::symbol(s)
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
}
