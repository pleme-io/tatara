//! S-expression reader — tokenize + parse into `Sexp`.
//!
//! Source positions are byte offsets into the original `&str`. Every token
//! carries the offset of its first character; reader-level errors
//! (`UnmatchedParen`, `UnmatchedOpenParen`, `Eof`) report that offset so
//! downstream tools (`tatara-lispc`, `tatara-check`, REPL, future LSP) can
//! pinpoint the failure in the source.

use crate::ast::{Atom, Sexp};
use crate::error::{LispError, Result};

#[derive(Clone, Debug, PartialEq)]
enum Token {
    LParen,
    RParen,
    Quote,
    Quasiquote,
    Unquote,
    UnquoteSplice,
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
                out.push((Token::Quote, pos));
            }
            '`' => {
                chars.next();
                out.push((Token::Quasiquote, pos));
            }
            ',' => {
                chars.next();
                // `,@` is splicing unquote; bare `,` is unquote.
                if chars.peek().map(|&(_, c)| c) == Some('@') {
                    chars.next();
                    out.push((Token::UnquoteSplice, pos));
                } else {
                    out.push((Token::Unquote, pos));
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
        Some((Token::Quote, _)) => read_quoted(it, eof_pos, Sexp::Quote),
        Some((Token::Quasiquote, _)) => read_quoted(it, eof_pos, Sexp::Quasiquote),
        Some((Token::Unquote, _)) => read_quoted(it, eof_pos, Sexp::Unquote),
        Some((Token::UnquoteSplice, _)) => read_quoted(it, eof_pos, Sexp::UnquoteSplice),
        Some((Token::Str(s), _)) => Ok(Sexp::Atom(Atom::Str(s))),
        Some((Token::Atom(s), _)) => Ok(atom_from_str(&s)),
        None => Err(LispError::Eof { pos: eof_pos }),
    }
}

/// Parse the datum following a quote-like prefix token (`'`, `` ` ``, `,`,
/// `,@`) and wrap it in the corresponding `Sexp` constructor.
///
/// Centralizes the four byte-identical "read-inner-and-box" arms in
/// `parse` — one per homoiconic prefix — into ONE emission site. Each
/// `Sexp::Quote` / `Sexp::Quasiquote` / `Sexp::Unquote` /
/// `Sexp::UnquoteSplice` tuple-variant constructor is itself a
/// `fn(Box<Sexp>) -> Sexp` (Rust tuple-variant ctors are first-class
/// function pointers), passed in via the `wrap` parameter so the helper
/// is shape-of-arm, not shape-of-variant — adding a fifth homoiconic
/// marker becomes ONE new arm, not three lines.
///
/// Theory anchor: THEORY.md §VI.1 — three-times rule. The four
/// `Some((Token::Quote*, _))` arms in `parse` each ran `let inner =
/// parse(it, eof_pos)?; Ok(Sexp::*(Box::new(inner)))` — byte-identical
/// modulo the `Sexp::*` constructor. Lifted into one helper so the
/// next change to the read-inner shape (e.g. threading source spans
/// when `Sexp` carries `pos: Option<usize>`) lands as ONE edit, not
/// four. Parallel posture to `tagged()` in `interop.rs` — the
/// canonical-form interop's analogous closed-set wrap helper across
/// the same four homoiconic variants.
fn read_quoted<I: Iterator<Item = Spanned>>(
    it: &mut std::iter::Peekable<I>,
    eof_pos: usize,
    wrap: fn(Box<Sexp>) -> Sexp,
) -> Result<Sexp> {
    let inner = parse(it, eof_pos)?;
    Ok(wrap(Box::new(inner)))
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
