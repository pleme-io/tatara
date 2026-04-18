//! Evaluator errors — cover unknown identifiers, type mismatches, arity
//! mismatches, and propagated reader/macroexpander errors from tatara-lisp.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("unbound identifier: {0}")]
    Unbound(String),

    #[error("type error: expected {expected}, got {found}")]
    Type { expected: String, found: String },

    #[error("arity error: {name} expects {expected} args, got {got}")]
    Arity {
        name: String,
        expected: String,
        got: usize,
    },

    #[error("malformed special form `{form}`: {reason}")]
    Malformed { form: String, reason: String },

    #[error("unknown builtin: {0}")]
    UnknownBuiltin(String),

    #[error("division by zero")]
    DivByZero,

    #[error("derivation error: {0}")]
    Derivation(String),

    #[error("attribute `{0}` not present")]
    MissingAttr(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Lisp(#[from] tatara_lisp::LispError),

    #[error("other: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, EvalError>;
