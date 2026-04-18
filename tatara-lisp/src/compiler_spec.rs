//! `CompilerSpec` — Lisp compilers as first-class typed Lisp data.
//!
//! This is the self-bootstrapping seam. A `CompilerSpec` is a declarative
//! recipe for a Lisp compiler: its preloaded macro library, its registered
//! domains, its optimization profile. Every `CompilerSpec` is itself
//! authorable as `(defcompiler …)` — so *Lisp specifies Lisp compilers*.
//!
//! Realizing a `CompilerSpec` produces a working compiler. You can realize:
//!   - **in memory** — a `RealizedCompiler` you call `.compile(src)` on, same
//!     process, no codegen.
//!   - **to disk** — serialize the spec as JSON alongside your source;
//!     `load_from_disk` materializes the same compiler later.
//!
//! ## The diminishing-returns theorem
//!
//! When Lisp can produce variant Lisp compilers (each specialized — different
//! macro library, different domain focus, different optimization profile),
//! optimizing the *base* compiler pays less than producing good generated
//! compilers. The base tatara-lisp Rust compiler becomes bootstrap; most
//! real-world compilation happens via specialized `RealizedCompiler`s.
//!
//! ```rust,ignore
//! use tatara_lisp::compiler_spec::{realize_in_memory, CompilerSpec};
//!
//! // Author in Lisp:
//! //   (defcompiler my-fast-lisp
//! //     :name        "my-fast-lisp"
//! //     :macros      ("(defmacro when (c x) `(if ,c ,x))")
//! //     :domains     ("defmonitor" "defalertpolicy"))
//! //
//! // Compile CompilerSpec from the Lisp source (via the registry):
//! // let specs = tatara_lisp::compile_typed::<CompilerSpec>(src)?;
//! // let my_compiler = realize_in_memory(specs[0].clone())?;
//! // let expanded = my_compiler.compile("(when #t (foo))")?;
//! ```

use serde::{Deserialize, Serialize};
use std::path::Path;
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

use crate::ast::Sexp;
use crate::error::{LispError, Result};
use crate::macro_expand::Expander;
use crate::reader::read;

/// Declarative recipe for a Lisp compiler. Authorable as `(defcompiler …)`.
#[derive(DeriveTataraDomain, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defcompiler")]
pub struct CompilerSpec {
    pub name: String,
    /// Reader dialect — `"standard"` by default. Reserved for future variants
    /// (`"strict"`, `"scheme"`, `"case-insensitive"`, etc.).
    #[serde(default = "default_dialect")]
    pub dialect: String,
    /// Preloaded macro definitions — each entry is a Lisp source string
    /// that `defmacro` / `defpoint-template` / `defcheck` forms.
    #[serde(default)]
    pub macros: Vec<String>,
    /// Domain keywords this compiler knows about. Must be registered in the
    /// global `tatara_lisp::domain` registry at realization time.
    #[serde(default)]
    pub domains: Vec<String>,
    /// Optimization profile — currently all compilers use `"tree-walk"`.
    /// Reserved values: `"tree-walk"`, `"bytecode"`, `"aot"`.
    #[serde(default = "default_optimization")]
    pub optimization: String,
    #[serde(default)]
    pub description: Option<String>,
}

fn default_dialect() -> String {
    "standard".into()
}

fn default_optimization() -> String {
    "tree-walk".into()
}

/// A compiler realized from a `CompilerSpec`. Holds a preloaded `Expander`
/// with the spec's macro library already registered. Thread-safe via `Clone`.
#[derive(Clone)]
pub struct RealizedCompiler {
    pub spec: CompilerSpec,
    preloaded: Expander,
}

impl RealizedCompiler {
    /// Parse + macroexpand a source string, returning the expanded top-level
    /// forms. Consumers dispatch from the forms to their typed compilers
    /// (via `tatara_lisp::domain::lookup` or `compile_typed::<T>`).
    pub fn compile(&self, src: &str) -> Result<Vec<Sexp>> {
        let forms = read(src)?;
        let mut exp = self.preloaded.clone();
        exp.expand_program(forms)
    }

    /// Macroexpand a single form (testing / REPL helper).
    pub fn expand(&self, form: &Sexp) -> Result<Sexp> {
        self.preloaded.expand(form)
    }

    /// How many macros the preloaded library registered.
    pub fn macro_count(&self) -> usize {
        self.preloaded.len()
    }
}

/// Realize a `CompilerSpec` in memory.
///
/// Steps:
/// 1. Start an empty `Expander`.
/// 2. For each macro source in the spec: parse + `expand_program` (which
///    registers every `defmacro` / `defpoint-template` / `defcheck` seen).
/// 3. Return a `RealizedCompiler` wrapping the loaded expander.
pub fn realize_in_memory(spec: CompilerSpec) -> Result<RealizedCompiler> {
    let mut preloaded = Expander::new();
    for macro_src in &spec.macros {
        let forms = read(macro_src)?;
        let _expanded = preloaded.expand_program(forms)?;
    }
    Ok(RealizedCompiler { spec, preloaded })
}

/// Serialize a `CompilerSpec` to a JSON file.
/// Pair with `load_from_disk` to round-trip.
pub fn realize_to_disk(spec: &CompilerSpec, path: impl AsRef<Path>) -> Result<()> {
    let json = serde_json::to_string_pretty(spec).map_err(|e| LispError::Compile {
        form: "realize_to_disk".into(),
        message: format!("serialize: {e}"),
    })?;
    std::fs::write(path, json).map_err(|e| LispError::Compile {
        form: "realize_to_disk".into(),
        message: format!("write: {e}"),
    })
}

/// Read a serialized `CompilerSpec` from disk and realize it. Inverse of
/// `realize_to_disk`.
pub fn load_from_disk(path: impl AsRef<Path>) -> Result<RealizedCompiler> {
    let json = std::fs::read_to_string(path).map_err(|e| LispError::Compile {
        form: "load_from_disk".into(),
        message: format!("read: {e}"),
    })?;
    let spec: CompilerSpec = serde_json::from_str(&json).map_err(|e| LispError::Compile {
        form: "load_from_disk".into(),
        message: format!("deserialize: {e}"),
    })?;
    realize_in_memory(spec)
}

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TataraDomain;

    #[test]
    fn defcompiler_form_compiles_to_spec() {
        let forms = read(
            r#"(defcompiler
                  :name "my-fast-lisp"
                  :dialect "standard"
                  :macros ("(defmacro when (c x) `(if ,c ,x))")
                  :domains ("defmonitor" "defalertpolicy")
                  :optimization "tree-walk"
                  :description "opinionated compiler for alerting")"#,
        )
        .unwrap();
        let spec = CompilerSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(spec.name, "my-fast-lisp");
        assert_eq!(spec.dialect, "standard");
        assert_eq!(spec.macros.len(), 1);
        assert_eq!(spec.domains, vec!["defmonitor".to_string(), "defalertpolicy".into()]);
    }

    #[test]
    fn realize_in_memory_preloads_macros() {
        let spec = CompilerSpec {
            name: "demo".into(),
            dialect: "standard".into(),
            macros: vec![
                "(defmacro when (c x) `(if ,c ,x))".into(),
                "(defmacro unless (c x) `(if ,c () ,x))".into(),
            ],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        };
        let compiler = realize_in_memory(spec).unwrap();
        assert_eq!(compiler.macro_count(), 2);
    }

    #[test]
    fn realized_compiler_expands_user_source() {
        let spec = CompilerSpec {
            name: "demo".into(),
            dialect: "standard".into(),
            macros: vec!["(defmacro when (c x) `(if ,c ,x))".into()],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        };
        let compiler = realize_in_memory(spec).unwrap();
        let expanded = compiler.compile("(when #t (foo))").unwrap();
        assert_eq!(expanded.len(), 1);
        // (when #t (foo)) → (if #t (foo))
        let list = expanded[0].as_list().unwrap();
        assert_eq!(list[0].as_symbol(), Some("if"));
        assert_eq!(list[1], Sexp::boolean(true));
    }

    #[test]
    fn nested_macro_expands_through_preloaded() {
        // The preloaded compiler has `when`; the user's source defines `unless`
        // in terms of `when`. Both should participate in a single expansion.
        let spec = CompilerSpec {
            name: "demo".into(),
            dialect: "standard".into(),
            macros: vec!["(defmacro when (c x) `(if ,c ,x))".into()],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        };
        let compiler = realize_in_memory(spec).unwrap();
        let expanded = compiler
            .compile("(defmacro unless (c x) `(when (not ,c) ,x)) (unless #f (foo))")
            .unwrap();
        // Final form should be fully expanded: (if (not #f) (foo))
        let final_form = expanded.last().unwrap().as_list().unwrap();
        assert_eq!(final_form[0].as_symbol(), Some("if"));
    }

    #[test]
    fn realize_to_disk_and_load_round_trips() {
        let tmp = std::env::temp_dir().join(format!(
            "tatara-compiler-{}.json",
            std::process::id()
        ));
        let spec = CompilerSpec {
            name: "disk-test".into(),
            dialect: "standard".into(),
            macros: vec!["(defmacro id (x) `,x)".into()],
            domains: vec!["defmonitor".into()],
            optimization: "tree-walk".into(),
            description: Some("persistence smoke test".into()),
        };
        realize_to_disk(&spec, &tmp).unwrap();
        let compiler = load_from_disk(&tmp).unwrap();
        assert_eq!(compiler.spec.name, "disk-test");
        assert_eq!(compiler.macro_count(), 1);
        // Realized compiler works exactly like the in-memory one.
        let out = compiler.compile("(id 42)").unwrap();
        assert_eq!(out[0], Sexp::int(42));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn empty_compiler_expands_nothing_but_reads_source() {
        let spec = CompilerSpec {
            name: "empty".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        };
        let compiler = realize_in_memory(spec).unwrap();
        assert_eq!(compiler.macro_count(), 0);
        let out = compiler.compile("(foo bar)").unwrap();
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn self_bootstrapping_compiler_generates_another_compiler() {
        // Use the base compiler to turn a (defcompiler …) form into a
        // CompilerSpec, realize THAT compiler, and use it.
        let base = realize_in_memory(CompilerSpec {
            name: "base".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();

        let source_of_child = r#"(defcompiler
            :name "child"
            :dialect "standard"
            :macros ("(defmacro twice (x) `(list ,x ,x))")
            :optimization "tree-walk")"#;

        // Base compiler expands the source (no macros involved here since the
        // source has no calls — just definitions).
        let forms = base.compile(source_of_child).unwrap();
        assert_eq!(forms.len(), 1);

        // Use the derive-generated compiler to turn the Sexp → typed CompilerSpec.
        let child_spec = CompilerSpec::compile_from_sexp(&forms[0]).unwrap();

        // Realize the child compiler.
        let child = realize_in_memory(child_spec).unwrap();
        assert_eq!(child.macro_count(), 1);

        // Child compiler can expand its own `twice`.
        let final_form = child.compile("(twice hello)").unwrap();
        let list = final_form[0].as_list().unwrap();
        assert_eq!(list[0].as_symbol(), Some("list"));
        assert_eq!(list[1].as_symbol(), Some("hello"));
        assert_eq!(list[2].as_symbol(), Some("hello"));
    }
}
