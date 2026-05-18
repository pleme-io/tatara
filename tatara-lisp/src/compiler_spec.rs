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
use crate::error::{CompilerSpecIoStage, LispError, Result};
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

/// Promote the previously `LispError::Compile`-shaped helper into the
/// structural `LispError::CompilerSpecIo { stage, message }` variant.
/// Eliminates four byte-identical `Compile`-shaped triples across
/// `realize_to_disk` (serialize / write) and `load_from_disk` (read /
/// deserialize), funneling the four call sites through ONE emission
/// site keyed on the closed-set `CompilerSpecIoStage` enum.
///
/// `stage` is `CompilerSpecIoStage` (the typed closed-set enum). The
/// helper projects `stage.operation()` and `stage.label()` into the
/// variant's Display rendering mechanically, so the compile-time
/// guarantee on BOTH slots is load-bearing in the type system — a
/// typo in either component can never drift into the diagnostic at
/// runtime AND the (operation, stage) pair is structurally constrained
/// to the four reachable pairs (`realize_to_disk` × {serialize, write}
/// ⊎ `load_from_disk` × {read, deserialize}). Same posture as how
/// `defmacro_arity` threads `MacroDefHead` straight into
/// `LispError::DefmacroArity.head`. Returns `LispError` directly
/// (not `Result`), so call sites compose with `map_err` / `ok_or_else`
/// without an extra `?`, parallel to the pre-lift signature.
///
/// After this lift the four call sites bind on variant identity
/// (`LispError::CompilerSpecIo { stage: CompilerSpecIoStage::_, … }`)
/// instead of substring-grepping the rendered `Compile`-shaped
/// diagnostic; closes the LAST `LispError::Compile { ... }`
/// construction site in `tatara-lisp/src/compiler_spec.rs`.
fn compiler_spec_io_err(stage: CompilerSpecIoStage, e: impl std::fmt::Display) -> LispError {
    LispError::CompilerSpecIo {
        stage,
        message: e.to_string(),
    }
}

/// Serialize a `CompilerSpec` to a JSON file.
/// Pair with `load_from_disk` to round-trip.
pub fn realize_to_disk(spec: &CompilerSpec, path: impl AsRef<Path>) -> Result<()> {
    let json = serde_json::to_string_pretty(spec)
        .map_err(|e| compiler_spec_io_err(CompilerSpecIoStage::RealizeToDiskSerialize, e))?;
    std::fs::write(path, json)
        .map_err(|e| compiler_spec_io_err(CompilerSpecIoStage::RealizeToDiskWrite, e))
}

/// Read a serialized `CompilerSpec` from disk and realize it. Inverse of
/// `realize_to_disk`.
pub fn load_from_disk(path: impl AsRef<Path>) -> Result<RealizedCompiler> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| compiler_spec_io_err(CompilerSpecIoStage::LoadFromDiskRead, e))?;
    let spec: CompilerSpec = serde_json::from_str(&json)
        .map_err(|e| compiler_spec_io_err(CompilerSpecIoStage::LoadFromDiskDeserialize, e))?;
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
        assert_eq!(
            spec.domains,
            vec!["defmonitor".to_string(), "defalertpolicy".into()]
        );
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
        let tmp = std::env::temp_dir().join(format!("tatara-compiler-{}.json", std::process::id()));
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
    fn compiler_spec_io_err_emits_structural_variant_with_typed_stage() {
        // Pin the helper's post-lift emission shape: it now returns the
        // structural `LispError::CompilerSpecIo { stage, message }`
        // variant directly, with `stage` typed as the closed-set
        // `CompilerSpecIoStage` enum and `message` carrying the raw
        // underlying-error `Display` projection (NO `{stage}: ` prefix
        // in the field — the prefix is in the Display impl, parallel
        // to how `DomainSerialize.message` and `KwargDeserialize.message`
        // carry raw `serde_json` projections). Pre-lift, the same call
        // returned `LispError::Compile { form: "realize_to_disk",
        // message: "serialize: boom" }`; fail-before-pass-after: this
        // assert is contradicted by the pre-lift code path, ratifies
        // the post-lift one.
        let err = super::compiler_spec_io_err(CompilerSpecIoStage::RealizeToDiskSerialize, "boom");
        match err {
            LispError::CompilerSpecIo { stage, message } => {
                assert_eq!(stage, CompilerSpecIoStage::RealizeToDiskSerialize);
                assert_eq!(message, "boom");
            }
            other => panic!("expected LispError::CompilerSpecIo, got {other:?}"),
        }
    }

    #[test]
    fn compiler_spec_io_err_threads_each_stage_through_unchanged() {
        // Path-uniformity: pin all four reachable `CompilerSpecIoStage`
        // values round-trip through the helper unchanged. A regression
        // that swaps two stages' identities or hard-codes a single
        // stage at the helper boundary fails-loudly here. Together
        // with the call-site tests below, this closes the
        // (helper × stage) matrix end-to-end.
        for stage in [
            CompilerSpecIoStage::RealizeToDiskSerialize,
            CompilerSpecIoStage::RealizeToDiskWrite,
            CompilerSpecIoStage::LoadFromDiskRead,
            CompilerSpecIoStage::LoadFromDiskDeserialize,
        ] {
            let err = super::compiler_spec_io_err(stage, "boom");
            match err {
                LispError::CompilerSpecIo {
                    stage: got_stage,
                    message,
                } => {
                    assert_eq!(got_stage, stage, "stage round-trip drifted");
                    assert_eq!(message, "boom", "message slot mutated unexpectedly");
                }
                other => panic!("expected LispError::CompilerSpecIo, got {other:?}"),
            }
        }
    }

    #[test]
    fn realize_to_disk_propagates_write_failure_via_compiler_spec_io_err() {
        // Path-uniformity: every persistence-failure call site funnels
        // through the same helper. `realize_to_disk` of a spec to a
        // path under a non-existent parent directory triggers the
        // `std::fs::write` failure path, which the helper wraps as
        // `CompilerSpecIo { stage: RealizeToDiskWrite, message: ... }`.
        // The structural variant binds tools on the typed enum
        // (`stage == CompilerSpecIoStage::RealizeToDiskWrite`) instead
        // of substring-greppying `"write: "` out of `message`.
        let spec = CompilerSpec {
            name: "io-fail".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        };
        // A path whose parent directory does not exist forces
        // `std::fs::write` to fail.
        let bogus =
            std::path::PathBuf::from("/nonexistent-dir-that-cannot-exist-tatara-routine/spec.json");
        let err = realize_to_disk(&spec, &bogus).unwrap_err();
        match err {
            LispError::CompilerSpecIo { stage, message } => {
                assert_eq!(stage, CompilerSpecIoStage::RealizeToDiskWrite);
                assert!(
                    !message.is_empty(),
                    "expected non-empty underlying-error message"
                );
            }
            other => panic!("expected LispError::CompilerSpecIo, got {other:?}"),
        }
    }

    #[test]
    fn load_from_disk_propagates_read_failure_via_compiler_spec_io_err() {
        // Sibling negative path: `load_from_disk` of a path that doesn't
        // exist triggers the `std::fs::read_to_string` failure path,
        // which the helper wraps as `CompilerSpecIo { stage:
        // LoadFromDiskRead, message: ... }`. Pinning the typed stage
        // identity `LoadFromDiskRead` distinct from `RealizeToDiskWrite`
        // proves the helper threads the stage slot through correctly
        // per call site — a regression that hard-codes a single stage
        // label or swaps two sites' labels fails-loudly here.
        let bogus =
            std::path::PathBuf::from("/nonexistent-dir-that-cannot-exist-tatara-routine/spec.json");
        // RealizedCompiler is not Debug so we manually destructure the Result
        // instead of calling .unwrap_err().
        let err = match load_from_disk(&bogus) {
            Ok(_) => panic!("expected load_from_disk failure on nonexistent path"),
            Err(e) => e,
        };
        match err {
            LispError::CompilerSpecIo { stage, message } => {
                assert_eq!(stage, CompilerSpecIoStage::LoadFromDiskRead);
                assert!(
                    !message.is_empty(),
                    "expected non-empty underlying-error message"
                );
            }
            other => panic!("expected LispError::CompilerSpecIo, got {other:?}"),
        }
    }

    #[test]
    fn load_from_disk_propagates_deserialize_failure_via_compiler_spec_io_err() {
        // Pin the deserialize-stage path: a file whose contents are not
        // valid JSON triggers `serde_json::from_str` failure, which the
        // helper wraps as `CompilerSpecIo { stage:
        // LoadFromDiskDeserialize, message: ... }`. Pinning the typed
        // stage identity `LoadFromDiskDeserialize` separately from
        // `LoadFromDiskRead` proves the helper distinguishes
        // sequential failure sites within ONE entry point structurally
        // — invalid combinations like `(LoadFromDisk, Write)` are
        // unrepresentable at the type level.
        let tmp = std::env::temp_dir().join(format!("tatara-bad-spec-{}.json", std::process::id()));
        std::fs::write(&tmp, "not-json").unwrap();
        // RealizedCompiler is not Debug so we manually destructure the Result.
        let err = match load_from_disk(&tmp) {
            Ok(_) => panic!("expected load_from_disk failure on malformed json"),
            Err(e) => e,
        };
        let _ = std::fs::remove_file(&tmp);
        match err {
            LispError::CompilerSpecIo { stage, message } => {
                assert_eq!(stage, CompilerSpecIoStage::LoadFromDiskDeserialize);
                assert!(
                    !message.is_empty(),
                    "expected non-empty underlying-error message"
                );
            }
            other => panic!("expected LispError::CompilerSpecIo, got {other:?}"),
        }
    }

    #[test]
    fn realize_to_disk_call_site_end_to_end_renders_legacy_diagnostic_byte_for_byte() {
        // End-to-end pin of the typed-exit-to-Display projection: the
        // `realize_to_disk` write-failure path renders as the legacy
        // `"compile error in realize_to_disk: write: {os-error}"` shape
        // byte-for-byte (modulo the OS-specific message tail, which
        // we substring-match on). The rendering is what downstream
        // consumers (`tatara-check`'s diagnostic capture, REPL substring-
        // greps) see — a regression that drifts the operation prefix
        // or the stage marker fails-loudly here AND in the unit
        // `compiler_spec_io_display_*` tests, so the contract is
        // pinned at BOTH the variant-construction boundary AND the
        // end-to-end call-site boundary.
        let spec = CompilerSpec {
            name: "io-fail".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        };
        let bogus =
            std::path::PathBuf::from("/nonexistent-dir-that-cannot-exist-tatara-routine/spec.json");
        let err = realize_to_disk(&spec, &bogus).unwrap_err();
        let rendered = format!("{err}");
        assert!(
            rendered.starts_with("compile error in realize_to_disk: write: "),
            "expected legacy operation-and-stage prefix, got: {rendered}"
        );
    }

    #[test]
    fn load_from_disk_call_site_end_to_end_renders_legacy_diagnostic_byte_for_byte() {
        // Sibling end-to-end pin for the deserialize-stage path: a
        // file whose contents aren't valid JSON renders as the legacy
        // `"compile error in load_from_disk: deserialize: {serde-error}"`
        // shape byte-for-byte. Pins the contract at the load-side
        // call-site boundary, mirroring the realize-side sibling test.
        let tmp = std::env::temp_dir().join(format!(
            "tatara-bad-spec-end2end-{}.json",
            std::process::id()
        ));
        std::fs::write(&tmp, "not-json").unwrap();
        let err = match load_from_disk(&tmp) {
            Ok(_) => panic!("expected load_from_disk failure on malformed json"),
            Err(e) => e,
        };
        let _ = std::fs::remove_file(&tmp);
        let rendered = format!("{err}");
        assert!(
            rendered.starts_with("compile error in load_from_disk: deserialize: "),
            "expected legacy operation-and-stage prefix, got: {rendered}"
        );
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
