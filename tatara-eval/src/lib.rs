//! tatara-eval — an optimized in-memory Lisp interpreter.
//!
//! Takes `Sexp` from `tatara-lisp`, evaluates to `Value`. Produces typed
//! `Derivation` values via the `derivation` builtin, which `tatara-nix::realize`
//! then builds into concrete store paths (either our own store or the live
//! `/nix/store` on disk).
//!
//! ```text
//! source  ──read──►  Sexp  ──eval──►  Value
//!                                      │
//!                                      ▼ (if Derivation)
//!                            tatara-nix::realize
//!                                      │
//!                                      ▼
//!                                 StorePath
//! ```
//!
//! The interpreter is Rust-bordered (you get a typed `Derivation` or a typed
//! error, never a partially-built thing), Lisp-authorable, and cheap to
//! clone thanks to `Arc`-backed `Value` cells. A future `&'arena` form can
//! replace the `Arc`s without changing the public API.

pub mod builtins;
pub mod env;
pub mod error;
pub mod interpreter;
pub mod value;

pub use env::Env;
pub use error::{EvalError, Result};
pub use interpreter::Interpreter;
pub use value::{Arity, Builtin, Lambda, Thunk, ThunkState, Value};
