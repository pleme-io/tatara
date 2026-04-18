//! `Value` — runtime values produced by the interpreter.
//!
//! Layered carefully so a future arena-backed representation can replace the
//! `Arc<...>` inner pointers without changing the public API. Today: ergonomic
//! reference-counted variants; tomorrow: `&'arena Value` allocated from
//! `bumpalo::Bump` held by `tatara-terreiro`.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use tatara_lisp::Sexp;
use tatara_nix::derivation::Derivation;
use tatara_nix::store::StorePath;

use crate::env::Env;

/// Every runtime value flows through this enum. Variants match a Nix-ish
/// ontology (atoms, lists, attrsets, functions, thunks, paths, derivations) so
/// the Lisp surface can carry Nix's academic semantics verbatim.
#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Symbol(String),
    Keyword(String),
    Path(PathBuf),
    List(Arc<Vec<Value>>),
    Attrs(Arc<BTreeMap<String, Value>>),
    Lambda(Arc<Lambda>),
    Builtin(Arc<Builtin>),
    Thunk(Arc<Thunk>),
    Derivation(Arc<Derivation>),
    StorePath(StorePath),
    /// A quoted Sexp — lets Lisp programs manipulate code-as-data.
    Quoted(Arc<Sexp>),
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(n) => write!(f, "{n}"),
            Self::Float(n) => write!(f, "{n}"),
            Self::Str(s) => write!(f, "{s:?}"),
            Self::Symbol(s) => write!(f, "sym:{s}"),
            Self::Keyword(s) => write!(f, ":{s}"),
            Self::Path(p) => write!(f, "path:{}", p.display()),
            Self::List(xs) => {
                write!(f, "(")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{x:?}")?;
                }
                write!(f, ")")
            }
            Self::Attrs(m) => {
                write!(f, "{{")?;
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{k} = {v:?}")?;
                }
                write!(f, "}}")
            }
            Self::Lambda(l) => write!(f, "<lambda/{}>", l.params.len()),
            Self::Builtin(b) => write!(f, "<builtin {}>", b.name),
            Self::Thunk(_) => write!(f, "<thunk>"),
            Self::Derivation(d) => write!(f, "<deriv {}>", d.name),
            Self::StorePath(p) => write!(f, "<store-path {p}>"),
            Self::Quoted(s) => write!(f, "'{s}"),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => write!(f, ""),
            Self::Str(s) => write!(f, "{s}"),
            Self::Int(n) => write!(f, "{n}"),
            Self::Float(n) => write!(f, "{n}"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Symbol(s) => write!(f, "{s}"),
            Self::Keyword(s) => write!(f, ":{s}"),
            Self::Path(p) => write!(f, "{}", p.display()),
            Self::List(xs) => {
                write!(f, "(")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, ")")
            }
            other => write!(f, "{other:?}"),
        }
    }
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Nil => "nil",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Str(_) => "string",
            Self::Symbol(_) => "symbol",
            Self::Keyword(_) => "keyword",
            Self::Path(_) => "path",
            Self::List(_) => "list",
            Self::Attrs(_) => "attrs",
            Self::Lambda(_) => "lambda",
            Self::Builtin(_) => "builtin",
            Self::Thunk(_) => "thunk",
            Self::Derivation(_) => "derivation",
            Self::StorePath(_) => "store-path",
            Self::Quoted(_) => "quoted",
        }
    }

    pub fn is_truthy(&self) -> bool {
        !matches!(self, Self::Nil | Self::Bool(false))
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) | Self::Symbol(s) | Self::Keyword(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Self::List(xs) => Some(xs),
            _ => None,
        }
    }

    pub fn as_attrs(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Self::Attrs(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_derivation(&self) -> Option<&Derivation> {
        match self {
            Self::Derivation(d) => Some(d),
            _ => None,
        }
    }

    pub fn as_path(&self) -> Option<&PathBuf> {
        match self {
            Self::Path(p) => Some(p),
            _ => None,
        }
    }

    /// Coerce this value to a plain `String` if that's a meaningful operation.
    /// Matches Nix's `toString` semantics: strings/numbers/paths/derivations
    /// have a canonical string form; lists/attrs/lambdas do not.
    pub fn coerce_to_string(&self) -> Option<String> {
        match self {
            Self::Str(s) => Some(s.clone()),
            Self::Symbol(s) | Self::Keyword(s) => Some(s.clone()),
            Self::Int(n) => Some(n.to_string()),
            Self::Float(n) => Some(n.to_string()),
            Self::Bool(b) => Some(b.to_string()),
            Self::Path(p) => Some(p.to_string_lossy().into_owned()),
            Self::StorePath(p) => Some(p.render()),
            Self::Derivation(d) => Some(d.store_path().render()),
            _ => None,
        }
    }
}

// ── function types ───────────────────────────────────────────────────────

/// A user-defined closure — captures its lexical environment.
pub struct Lambda {
    pub params: Vec<String>,
    /// If present, extra args bind to this single name as a list (rest-args).
    pub rest: Option<String>,
    pub body: Vec<Sexp>,
    pub env: Env,
    pub name: Option<String>,
}

/// A native builtin. Takes already-evaluated args + current env + interpreter
/// handle so it can evaluate Sexp if it needs to (for short-circuit forms, the
/// interpreter uses special-form dispatch instead).
pub type BuiltinFn = dyn Fn(&[Value]) -> crate::error::Result<Value> + Send + Sync;

pub struct Builtin {
    pub name: String,
    pub arity: Arity,
    pub func: Arc<BuiltinFn>,
}

#[derive(Clone, Copy, Debug)]
pub enum Arity {
    Exact(usize),
    AtLeast(usize),
    Any,
}

impl Arity {
    pub fn check(&self, got: usize) -> bool {
        match *self {
            Self::Exact(n) => got == n,
            Self::AtLeast(n) => got >= n,
            Self::Any => true,
        }
    }

    pub fn describe(&self) -> String {
        match *self {
            Self::Exact(n) => format!("{n}"),
            Self::AtLeast(n) => format!("at least {n}"),
            Self::Any => "any".into(),
        }
    }
}

// ── laziness ─────────────────────────────────────────────────────────────

/// A deferred computation. Forced once; result cached. Enables Nix-style
/// non-strict evaluation: `(let ((x big-compute)) small)` skips big-compute
/// unless something forces `x`.
pub struct Thunk {
    pub cell: std::sync::Mutex<ThunkState>,
}

pub enum ThunkState {
    Unevaluated { body: Sexp, env: Env },
    Evaluating, // detects cycles
    Forced(Value),
}

impl Thunk {
    pub fn new(body: Sexp, env: Env) -> Arc<Self> {
        Arc::new(Self {
            cell: std::sync::Mutex::new(ThunkState::Unevaluated { body, env }),
        })
    }
}
