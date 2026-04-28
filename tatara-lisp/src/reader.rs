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
        Some((Token::Quote, _)) => {
            let inner = parse(it, eof_pos)?;
            Ok(Sexp::Quote(Box::new(inner)))
        }
        Some((Token::Quasiquote, _)) => {
            let inner = parse(it, eof_pos)?;
            Ok(Sexp::Quasiquote(Box::new(inner)))
        }
        Some((Token::Unquote, _)) => {
            let inner = parse(it, eof_pos)?;
            Ok(Sexp::Unquote(Box::new(inner)))
        }
        Some((Token::UnquoteSplice, _)) => {
            let inner = parse(it, eof_pos)?;
            Ok(Sexp::UnquoteSplice(Box::new(inner)))
        }
        Some((Token::Str(s), _)) => Ok(Sexp::Atom(Atom::Str(s))),
        Some((Token::Atom(s), _)) => Ok(atom_from_str(&s)),
        None => Err(LispError::Eof { pos: eof_pos }),
    }
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
}
