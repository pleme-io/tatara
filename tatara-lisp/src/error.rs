use thiserror::Error;

pub type Result<T> = std::result::Result<T, LispError>;

#[derive(Debug, Error)]
pub enum LispError {
    #[error("unexpected character {0:?} at position {1}")]
    UnexpectedChar(char, usize),
    #[error("unterminated string literal at position {0}")]
    UnterminatedString(usize),
    #[error("unmatched closing paren at position {0}")]
    UnmatchedParen(usize),
    #[error("unmatched opening paren")]
    UnmatchedOpenParen,
    #[error("unexpected end of input")]
    Eof,
    #[error("invalid number literal {0:?}")]
    InvalidNumber(String),
    #[error("unknown symbol: {0}")]
    UnknownSymbol(String),
    #[error("type error: expected {expected}, got {got}")]
    Type { expected: &'static str, got: String },
    #[error("compile error in {form}: {message}")]
    Compile { form: String, message: String },
    #[error("unknown {category}: {value}")]
    Unknown { category: &'static str, value: String },
    #[error("missing required field: {0}")]
    Missing(&'static str),
    #[error("odd number of keyword arguments")]
    OddKwargs,
}
