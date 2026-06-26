//! tatara-lisp — a small homoiconic S-expression language.
//!
//! The surface is homoiconic: the *reader* produces an AST (`Sexp`) that is
//! itself S-expressions. Macros operate on `Sexp` and yield `Sexp`.
//!
//! Scope of v0 (this scaffold): lexer, reader, `Sexp` AST, environment,
//! plus a minimal evaluator shell (special forms `quote`, `if`, `let`, `lambda`).
//! The `ProcessSpec` compiler (defpoint macro + flattening to
//! `tatara_process::ProcessSpec`) lands in `compile.rs` in the next pass.
//!
//! ```lisp
//! (defpoint observability-stack
//!   :parent seph.1
//!   :class (Gate Observability Bounded Monotone Internal)
//!   :intent (nix "github:pleme-io/k8s" :attr "observability")
//!   :compliance (baseline fedramp-moderate
//!                :at-boundary (nist SC-7)
//!                :post        (cis-k8s-v1.8))
//!   :depends-on (akeyless-injection))
//! ```

// Allow the derive macro's `::tatara_lisp::...` paths to resolve when
// `#[derive(TataraDomain)]` is applied inside tatara-lisp itself
// (`CompilerSpec` + test modules).
extern crate self as tatara_lisp;

pub mod ast;
pub mod closed_set;
pub mod compile;
pub mod compiler_spec;
pub mod diagnostic;
pub mod domain;
pub mod env;
pub mod error;
pub mod macro_expand;
pub mod reader;

#[cfg(feature = "iac-forge")]
pub mod interop;

pub use compiler_spec::{
    load_from_disk, realize_in_memory, realize_to_disk, CompilerSpec, RealizedCompiler,
};
pub use domain::{DomainHandler, TataraDomain};
// Derive macro — same name as the trait, different namespace (procedural
// macros vs. types), so they coexist cleanly under one import.
pub use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

pub use ast::{iter_calls_to, Atom, AtomKind, QuoteForm, Sexp, UnknownAtomKind, UnknownQuoteForm};
pub use closed_set::{assert_closed_set_well_formed, ClosedSet};
pub use compile::{
    compile_named, compile_named_from_forms, compile_typed, compile_typed_any,
    compile_typed_any_from_forms, compile_typed_from_forms, NamedDefinition,
};
pub use diagnostic::{format_diagnostic, line_col, LineCol};
pub use env::Env;
pub use error::{
    CompilerSpecIoStage, ExpectedKwargShape, KwargPath, KwargPathKind, LispError, MacroDefHead,
    OptionalParamMalformedReason, Result, SexpShape, TemplateInvariantKind,
    UnknownCompilerSpecIoStage, UnknownExpectedKwargShape, UnknownKwargPathKind,
    UnknownMacroDefHead, UnknownSexpShape, UnknownUnquoteForm, UnquoteForm,
};
pub use macro_expand::{Expander, MacroDef, MacroParams, OptionalParam};
pub use reader::read;
// `#[derive(ClosedSet)]` proc-macro — same name as the trait, different
// namespace (proc-macros vs. types), so they coexist cleanly under one
// import. Mirrors `DeriveTataraDomain` sitting next to the trait
// `TataraDomain`; emits the 4-line `impl ClosedSet` + 4-line
// `impl FromStr` plumbing the workspace-wide closed-set-enum cohort
// re-derives byte-for-byte.
pub use tatara_lisp_derive::ClosedSet as DeriveClosedSet;
