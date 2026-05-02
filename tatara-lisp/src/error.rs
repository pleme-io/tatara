use thiserror::Error;

pub type Result<T> = std::result::Result<T, LispError>;

#[derive(Debug, Error)]
pub enum LispError {
    #[error("unexpected character {0:?} at position {1}")]
    UnexpectedChar(char, usize),
    #[error("unterminated string literal at position {0}")]
    UnterminatedString(usize),
    #[error("unmatched closing paren at position {pos}")]
    UnmatchedParen { pos: usize },
    #[error("unmatched opening paren at position {pos}")]
    UnmatchedOpenParen { pos: usize },
    #[error("unexpected end of input at position {pos}")]
    Eof { pos: usize },
    #[error("invalid number literal {0:?}")]
    InvalidNumber(String),
    #[error("unknown symbol: {0}")]
    UnknownSymbol(String),
    #[error("type error: expected {expected}, got {got}")]
    Type { expected: &'static str, got: String },
    #[error("compile error in {form}: {message}")]
    Compile { form: String, message: String },
    /// Structured type mismatch — both sides are first-class fields, not
    /// embedded substrings of `message`. Rendered as `"compile error in
    /// {form}: expected {expected}, got {got}"` so the user-facing string
    /// matches the legacy `Compile`-shaped diagnostic byte-for-byte; the
    /// gain is structural — authoring tools (REPL, LSP, `tatara-check`)
    /// pattern-match on the variant and bind directly to `expected` /
    /// `got` instead of substring-parsing the rendered message.
    ///
    /// `expected` is `&'static str` so a typo can never drift into the
    /// diagnostic at runtime; `got` is `&'static str` because it is
    /// always the output of `crate::domain::sexp_type_name`, whose match
    /// is exhaustive over `Sexp` at compile time. When a future run gives
    /// `Sexp` source spans, `pos: Option<usize>` lands here in ONE place
    /// and every type-mismatch site picks up positional rendering via
    /// `crate::diagnostic::format_diagnostic` mechanically.
    #[error("compile error in {form}: expected {expected}, got {got}")]
    TypeMismatch {
        form: String,
        expected: &'static str,
        got: &'static str,
    },
    #[error("unknown {category}: {value}")]
    Unknown {
        category: &'static str,
        value: String,
    },
    #[error("missing required field: {0}")]
    Missing(&'static str),
    /// A kwargs list of odd length: the last element has no partner. The
    /// `dangling` field holds the offending element's `Sexp::Display`
    /// projection — `:query` for a keyword whose value got lost, or the
    /// literal form of a stray non-keyword. Naming both halves of the
    /// failure (the failure mode AND the offending element) is the
    /// typed-entry gate's structural-completeness floor (THEORY.md §V.1):
    /// without it the operator must re-read the source to find what
    /// actually misfired.
    #[error("odd keyword arguments: dangling element `{dangling}`")]
    OddKwargs { dangling: String },
}

impl LispError {
    /// Byte offset of the failure into the source, when locatable.
    ///
    /// Variants without a position (`Type`, `Compile`, etc.) return `None`,
    /// so callers can render a snippet only when the substrate has the
    /// information to do so. New positional variants gain editor-ready
    /// rendering (via `crate::diagnostic::format_diagnostic`) by adding a
    /// branch here — no consumer changes required.
    #[must_use]
    pub fn position(&self) -> Option<usize> {
        match self {
            Self::UnexpectedChar(_, pos) | Self::UnterminatedString(pos) => Some(*pos),
            Self::UnmatchedParen { pos } | Self::UnmatchedOpenParen { pos } | Self::Eof { pos } => {
                Some(*pos)
            }
            Self::InvalidNumber(_)
            | Self::UnknownSymbol(_)
            | Self::Type { .. }
            | Self::Compile { .. }
            | Self::TypeMismatch { .. }
            | Self::Unknown { .. }
            | Self::Missing(_)
            | Self::OddKwargs { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LispError;

    #[test]
    fn position_extracts_offset_from_positional_variants() {
        assert_eq!(LispError::UnexpectedChar('?', 7).position(), Some(7));
        assert_eq!(LispError::UnterminatedString(11).position(), Some(11));
        assert_eq!(LispError::UnmatchedParen { pos: 3 }.position(), Some(3));
        assert_eq!(LispError::UnmatchedOpenParen { pos: 0 }.position(), Some(0));
        assert_eq!(LispError::Eof { pos: 42 }.position(), Some(42));
    }

    #[test]
    fn position_is_none_for_non_positional_variants() {
        assert_eq!(
            LispError::OddKwargs {
                dangling: ":query".into()
            }
            .position(),
            None
        );
        assert_eq!(LispError::Missing("name").position(), None);
        assert_eq!(LispError::InvalidNumber("nan".into()).position(), None);
        assert_eq!(LispError::UnknownSymbol("foo".into()).position(), None);
        assert_eq!(
            LispError::Type {
                expected: "int",
                got: "string".into()
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::Compile {
                form: ":x".into(),
                message: "bad".into()
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::TypeMismatch {
                form: ":x".into(),
                expected: "string",
                got: "int",
            }
            .position(),
            None
        );
        assert_eq!(
            LispError::Unknown {
                category: "domain",
                value: "defx".into()
            }
            .position(),
            None
        );
    }
}
