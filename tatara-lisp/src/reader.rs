//! S-expression reader — tokenize + parse into `Sexp`.

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

/// Read a full program (sequence of top-level forms) into a `Vec<Sexp>`.
pub fn read(src: &str) -> Result<Vec<Sexp>> {
    let tokens = tokenize(src)?;
    let mut it = tokens.into_iter().peekable();
    let mut forms = Vec::new();
    while it.peek().is_some() {
        forms.push(parse(&mut it)?);
    }
    Ok(forms)
}

fn tokenize(src: &str) -> Result<Vec<Token>> {
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
                out.push(Token::LParen);
            }
            ')' => {
                chars.next();
                out.push(Token::RParen);
            }
            '\'' => {
                chars.next();
                out.push(Token::Quote);
            }
            '`' => {
                chars.next();
                out.push(Token::Quasiquote);
            }
            ',' => {
                chars.next();
                // `,@` is splicing unquote; bare `,` is unquote.
                if chars.peek().map(|&(_, c)| c) == Some('@') {
                    chars.next();
                    out.push(Token::UnquoteSplice);
                } else {
                    out.push(Token::Unquote);
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
                out.push(Token::Str(s));
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
                out.push(Token::Atom(s));
            }
        }
    }
    Ok(out)
}

fn parse<I: Iterator<Item = Token>>(it: &mut std::iter::Peekable<I>) -> Result<Sexp> {
    match it.next() {
        Some(Token::LParen) => {
            let mut xs = Vec::new();
            loop {
                match it.peek() {
                    Some(Token::RParen) => {
                        it.next();
                        return Ok(Sexp::List(xs));
                    }
                    Some(_) => xs.push(parse(it)?),
                    None => return Err(LispError::UnmatchedOpenParen),
                }
            }
        }
        Some(Token::RParen) => Err(LispError::UnmatchedParen(0)),
        Some(Token::Quote) => {
            let inner = parse(it)?;
            Ok(Sexp::Quote(Box::new(inner)))
        }
        Some(Token::Quasiquote) => {
            let inner = parse(it)?;
            Ok(Sexp::Quasiquote(Box::new(inner)))
        }
        Some(Token::Unquote) => {
            let inner = parse(it)?;
            Ok(Sexp::Unquote(Box::new(inner)))
        }
        Some(Token::UnquoteSplice) => {
            let inner = parse(it)?;
            Ok(Sexp::UnquoteSplice(Box::new(inner)))
        }
        Some(Token::Str(s)) => Ok(Sexp::Atom(Atom::Str(s))),
        Some(Token::Atom(s)) => Ok(atom_from_str(&s)),
        None => Err(LispError::Eof),
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
        let forms = read("foo 42 3.14 \"hello\" :kw #t #f").unwrap();
        assert_eq!(forms.len(), 7);
        assert_eq!(forms[0].as_symbol(), Some("foo"));
        assert_eq!(forms[1], Sexp::int(42));
        assert_eq!(forms[2], Sexp::float(3.14));
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
}
