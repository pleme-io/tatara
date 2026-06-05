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
use crate::compile::NamedDefinition;
use crate::domain::TataraDomain;
use crate::error::{CompilerSpecIoStage, LispError, Result};
use crate::macro_expand::Expander;

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
    /// Per-call clone of the preloaded expander — the single named projection
    /// every dispatcher method on [`RealizedCompiler`] threads through to
    /// reach the expander surface, in ONE place on the struct's algebra.
    ///
    /// The clone semantics are load-bearing across every dispatcher:
    ///   * `self.preloaded.cache` is `Arc<Mutex<HashMap<…>>>`; `.clone()` of
    ///     the [`Expander`] shares the cache by Arc, so repeated calls
    ///     through the SAME [`RealizedCompiler`] hit the same memoization
    ///     table — realizations of the same [`CompilerSpec`] benefit from
    ///     each other's expansion cache hits across `.compile*()`
    ///     invocations.
    ///   * `self.preloaded.macros` is owned `HashMap`; `.clone()` of the
    ///     `Expander` deep-clones the table, so `defmacro` / `defpoint-template`
    ///     / `defcheck` heads absorbed during THIS call's [`Expander::expand_program`]
    ///     step land in the returned clone, NOT in the persistent realized
    ///     compiler. A `(defmacro foo …)` in the user's source therefore
    ///     does NOT leak across `RealizedCompiler` calls — every dispatch
    ///     starts from the spec's original `:macros` library.
    ///
    /// Before this lift the same `self.preloaded.clone()` projection lived
    /// inline at six sites — three from-forms dispatchers
    /// ([`Self::compile_from_forms`], [`Self::compile_typed_from_forms`],
    /// [`Self::compile_named_from_forms`]) and three from-source dispatchers
    /// ([`Self::compile`], [`Self::compile_typed`], [`Self::compile_named`]).
    /// After the companion lift (from-source dispatchers delegate to their
    /// from-forms siblings via `read(src)? + <self.from_forms_sibling>(forms)`)
    /// the projection narrows to THREE sites at the from-forms row, all of
    /// which route through this helper — a future change to the clone
    /// posture (sharing strategy, eager-vs-lazy template recompile, audit
    /// hook on every per-call materialization) lands in ONE method body
    /// the entire dispatcher matrix inherits, instead of being re-derived
    /// at every from-forms primitive's call site.
    ///
    /// `pub(crate)` because the realized compiler's `preloaded` field is
    /// private (the [`Expander`] surface is an implementation detail of the
    /// dispatcher matrix); exposing the clone publicly would leak the
    /// implementation through the type signature without enabling any
    /// substrate consumer the public dispatcher methods don't already
    /// serve. Tests inside this crate consume the helper directly to pin
    /// the clone semantics; external consumers reach the same per-call
    /// clone posture through the public dispatcher methods.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition; six
    /// byte-identical inline copies of the `self.preloaded.clone()`
    /// projection across the dispatcher matrix is past the ≥2
    /// PRIME-DIRECTIVE trigger once the structural shape is named.
    /// THEORY.md §V.1 — knowable platform; the per-call clone discipline
    /// becomes a NAMED primitive on the `RealizedCompiler` algebra rather
    /// than a per-dispatcher inline projection that future emitters would
    /// have had to re-derive. THEORY.md §II.1 invariant 2 — free middle;
    /// every dispatcher routes through the SAME clone primitive, so a
    /// regression that drifts ONE dispatcher's clone posture from the
    /// others cannot reach the substrate's runtime.
    pub(crate) fn cloned_preloaded(&self) -> Expander {
        self.preloaded.clone()
    }

    /// Parse + macroexpand a source string, returning the expanded top-level
    /// forms. Consumers dispatch from the forms to their typed compilers
    /// (via `tatara_lisp::domain::lookup` or `compile_typed::<T>`).
    ///
    /// Routes through [`Self::compile_from_forms`] on the parsed top-level
    /// forms — the from-source posture of the untyped-expansion dispatcher
    /// inherits the per-call clone discipline through delegation, NOT by
    /// re-deriving [`Self::cloned_preloaded`] at this call site. Sibling of
    /// [`Self::compile_typed`] / [`Self::compile_named`] — those methods stack
    /// a typed-keyword projection on top of the from-source primitive; this
    /// method exposes the bare untyped expansion for consumers
    /// (`tatara-check`'s per-form dispatcher) that walk every form regardless
    /// of keyword. The from-source-delegates-to-from-forms shape mirrors
    /// [`Expander::expand_source_program`]'s delegation to [`Expander::expand_program`]
    /// at the expander boundary, so the `read(src)? + <from_forms_sibling>(forms)`
    /// composition lives in ONE place per form-shape (the from-forms row of
    /// the dispatcher matrix) at BOTH the expander boundary AND the
    /// realized-compiler boundary.
    pub fn compile(&self, src: &str) -> Result<Vec<Sexp>> {
        self.compile_from_forms(crate::reader::read(src)?)
    }

    /// Macroexpand a pre-parsed program through THIS `RealizedCompiler`'s
    /// preloaded macro library — the from-forms posture of [`Self::compile`].
    ///
    /// Routes through [`Expander::expand_program`] on a clone of the
    /// preloaded expander: the cloned expander walks every supplied form,
    /// absorbing any `defmacro` / `defpoint-template` / `defcheck` head into
    /// the clone (NOT the persistent realized compiler, mirroring
    /// [`Self::compile`]'s per-call clone isolation) and expanding every
    /// macro call against the spec's `:macros` library plus the in-source
    /// `defmacro` definitions of THIS call. Non-`defmacro` forms surface in
    /// source order as the returned `Vec<Sexp>` for downstream consumers
    /// (`tatara-check`'s per-form dispatcher, an LSP's "show me the
    /// expanded program" handler that operates on already-parsed AST
    /// fragments).
    ///
    /// Sibling of [`Self::compile`] — that method stacks a [`crate::reader::read`]
    /// step on top of this one, so the from-source / from-forms pair on
    /// `RealizedCompiler` for untyped expansion routes through the SAME
    /// [`Expander::expand_program`] primitive ([`Self::compile`] composes
    /// it with `crate::reader::read` via [`Expander::expand_source_program`];
    /// this method binds it directly). The 2×2 cells (untyped vs. typed,
    /// from-forms vs. from-source) close on `RealizedCompiler` with
    /// [`Self::compile_typed`] / [`Self::compile_named`] (from-source typed
    /// / named) plus [`Self::compile_typed_from_forms`] /
    /// [`Self::compile_named_from_forms`] (from-forms typed / named).
    ///
    /// The future change that benefits: a consumer that has already parsed
    /// `Vec<Sexp>` through another `Sexp`-producing surface (a macro-expanded
    /// subform, a REPL's already-quoted top-level buffer, an LSP cache of
    /// partially-edited forms) and wants to dispatch through the preloaded
    /// library without re-rendering source.
    pub fn compile_from_forms(&self, forms: Vec<Sexp>) -> Result<Vec<Sexp>> {
        self.cloned_preloaded().expand_program(forms)
    }

    /// Macroexpand a single form (testing / REPL helper).
    pub fn expand(&self, form: &Sexp) -> Result<Sexp> {
        self.preloaded.expand(form)
    }

    /// How many macros the preloaded library registered.
    pub fn macro_count(&self) -> usize {
        self.preloaded.len()
    }

    /// Compile every `(T::KEYWORD :k v …)` form in `src` into a typed `T`
    /// through THIS `RealizedCompiler`'s preloaded macro library — the
    /// preloaded-expander posture of [`crate::compile_typed`].
    ///
    /// Routes through the substrate's composed expand-then-keyword-project
    /// primitive [`Expander::expand_and_collect_calls_to`] on a clone of
    /// the preloaded expander: source is read, the preloaded clone walks
    /// every top-level form (expanding macro calls and absorbing any new
    /// `defmacro` definitions in the source into the per-call clone, not
    /// the persistent realized compiler), and every expanded form whose
    /// head matches `T::KEYWORD` flows through `T::compile_from_args` in
    /// source order. Non-matching forms are skipped silently
    /// (soft-projection posture inherited from
    /// [`crate::ast::iter_calls_to`]).
    ///
    /// Sibling of the fresh-expander posture
    /// [`crate::compile_typed`] — both consumers route through the SAME
    /// `Expander::expand_and_collect_calls_to` primitive, each binding
    /// the per-form projection `T::compile_from_args` directly, with the
    /// SAME `T::KEYWORD` constant filtering the expanded forms. They
    /// differ in expander posture: `compile_typed` uses a fresh
    /// `Expander::new()` (no preloaded macros, ideal for one-shot typed
    /// compilation with no macro library); this method uses the
    /// realized compiler's preloaded `Expander` (the macro library
    /// authored via the spec's `:macros` slot participates in the
    /// expansion). A `(defcompiler …)` form in the user's source that
    /// invokes a preloaded `defmacro` (e.g. `(mk-compiler "name")`
    /// expanding to `(defcompiler "name" :dialect "standard")`)
    /// compiles successfully through THIS method and fails silently
    /// through `compile_typed` (the macro call is unknown to the fresh
    /// expander, so the form's head is not `T::KEYWORD` and the form
    /// is skipped).
    ///
    /// The preloaded expander is cloned per call so the cache
    /// (`Arc<Mutex<HashMap>>`) is SHARED across calls (realizations of
    /// the same `CompilerSpec` benefit from each other's expansion
    /// cache hits — `Expander::cache` is wrapped in `Arc<Mutex>`
    /// precisely for this), while the per-call `defmacro` absorption
    /// (which lands in `self.preloaded.macros`'s clone, not the
    /// original) stays local to the call. Per-call clone isolation +
    /// shared cache mirrors the existing [`compile`](Self::compile)
    /// method's posture verbatim.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the diminishing-returns theorem (`compiler_spec.rs`'s module
    /// preamble) says optimizing the base compiler pays less than
    /// producing good generated compilers — and lands as a typed
    /// dispatcher on `RealizedCompiler` rather than as an inline
    /// `iter_calls_to + map + collect` walk every consumer re-derives
    /// after calling `compile(src)?`. THEORY.md §II.1 invariant 1 —
    /// typed entry; the typed-keyword filter over the preloaded-
    /// expanded program IS the typed-entry-batch gate, and naming its
    /// preloaded posture lifts the gate from a per-consumer inline
    /// derivation to ONE method on `RealizedCompiler` the substrate's
    /// diagnostic promotions hang off of. THEORY.md §II.1 invariant 2
    /// — free middle; both the fresh-expander posture and the
    /// preloaded-expander posture route through the SAME
    /// `expand_and_collect_calls_to` primitive, so a regression that
    /// drifts ONE posture's pipeline from the other cannot reach the
    /// substrate's runtime — the type system binds both consumers to
    /// the projection's single emission shape.
    ///
    /// Frontier inspiration: Racket's `make-compiler` /
    /// `(eval-syntax stx ns)` against a namespace populated with the
    /// preloaded compiler's `require`d macros — typed compilation
    /// inside a NAMESPACE that carries the macro library is the
    /// Racket idiom; the substrate's preloaded-expander posture is
    /// the Rust-typed peer of that, lifted onto the `Expander`
    /// surface with the typed-keyword projection as the per-form
    /// visitor.
    pub fn compile_typed<T: TataraDomain>(&self, src: &str) -> Result<Vec<T>> {
        self.compile_typed_from_forms::<T>(crate::reader::read(src)?)
    }

    /// Compile every `(T::KEYWORD :k v …)` form in `forms` into a typed `T`
    /// through THIS `RealizedCompiler`'s preloaded macro library — the
    /// from-forms posture of [`Self::compile_typed`].
    ///
    /// Routes through [`Expander::expand_to_typed::<T>`] on a clone of the
    /// preloaded expander — the SAME typed primitive [`Self::compile_typed`]'s
    /// from-source delegation ultimately threads through ([`Self::compile_typed`]
    /// is `self.preloaded.clone().expand_source_to_typed::<T>(src)` =
    /// `read(src)? + expand_to_typed::<T>(forms)`). This method surfaces the
    /// second leg of that composition as ONE preloaded-posture primitive
    /// rather than asking every from-forms typed consumer of a realized
    /// compiler to write `realized.preloaded.clone().expand_to_typed::<T>(forms)`
    /// itself (and the `preloaded` field is private, so they'd have to
    /// `realized.compile(... rendered_back_to_source ...)?` round-trip
    /// through source first, defeating the whole point of from-forms entry).
    ///
    /// Sibling of [`Self::compile_typed`] (the from-source posture's
    /// preloaded-typed dispatcher) and of [`crate::compile_typed_from_forms`]
    /// (the from-forms posture's fresh-expander dispatcher). Together with
    /// those two — plus [`Self::compile_typed`]'s fresh-expander free-function
    /// sibling [`crate::compile_typed`] — this method closes the
    /// typed-dispatcher matrix across BOTH axes — expander posture
    /// (fresh + preloaded) × input posture (from-forms + from-source) — at
    /// the public dispatcher boundary. The matrix is symmetric: each cell
    /// routes through the SAME typed primitive on `Expander`
    /// ([`Expander::expand_to_typed::<T>`] for from-forms,
    /// [`Expander::expand_source_to_typed::<T>`] for from-source — which
    /// itself delegates to the from-forms primitive through
    /// `read(src)? + expand_to_typed(forms)`). A regression that drifts ONE
    /// cell's `(T::KEYWORD, T::compile_from_args)` binding from the others
    /// is structurally impossible — the type parameter binds both
    /// substitutions to the SAME concrete type at the call boundary inside
    /// the typed primitive every cell delegates through.
    ///
    /// Per-call posture matches [`Self::compile_typed`]: the preloaded
    /// expander is cloned per call so cache is shared via `Arc<Mutex>` and
    /// per-call `defmacro` absorption stays local to the clone. A
    /// `defmacro` in a pre-parsed form lands in the clone exactly as it
    /// does in [`Self::compile_typed`]'s from-source posture — both
    /// postures route through `expand_to_typed::<T>` which composes
    /// `expand_program(forms)` (the defmacro-absorption + macro-expansion
    /// step) with the typed-keyword projection.
    ///
    /// The future change that benefits: an LSP that maintains a partial
    /// `Vec<Sexp>` AST cache across edits and wants to re-run typed
    /// dispatch through a preloaded library against a modified subtree,
    /// a `tatara-check` runner that walks every typed `(defX …)` form
    /// inside a `(defcheck …)` macro body that's already been expanded
    /// once, a REPL `:dispatch <T> (form …)` command that takes
    /// already-quoted forms against the active compiler's preloaded
    /// expander — binds to ONE method on `RealizedCompiler`
    /// (`compile_typed_from_forms::<T>(forms)`) instead of round-tripping
    /// through source serialization first.
    ///
    /// Theory anchor: same as [`Self::compile_typed`]. THEORY.md §VI.1
    /// (generation over composition; the (preloaded × from-forms × typed)
    /// cell of the dispatcher matrix is bound in ONE place rather than
    /// re-derived inline at every from-forms preloaded consumer's call
    /// site), THEORY.md §II.1 invariant 1 (typed entry; the typed-keyword
    /// filter paired with `T::compile_from_args` IS the from-forms
    /// typed-entry-batch gate at the preloaded boundary), THEORY.md §II.1
    /// invariant 2 (free middle; all four cells of the dispatcher matrix
    /// route through the SAME typed primitive on `Expander`).
    pub fn compile_typed_from_forms<T: TataraDomain>(&self, forms: Vec<Sexp>) -> Result<Vec<T>> {
        self.cloned_preloaded().expand_to_typed::<T>(forms)
    }

    /// Compile every `(T::KEYWORD NAME :k v …)` form in `src` into a typed
    /// [`NamedDefinition<T>`] through THIS `RealizedCompiler`'s preloaded
    /// macro library — the preloaded-expander posture of
    /// [`crate::compile_named`].
    ///
    /// Routes through the substrate's composed expand-then-keyword-project
    /// primitive [`Expander::expand_and_collect_calls_to`] on a clone of
    /// the preloaded expander, binding the per-form projection
    /// [`named_form_projection::<T>`](crate::compile::named_form_projection)
    /// — the SAME projection [`crate::compile_named_from_forms`] (the
    /// fresh-expander posture) routes through. Both consumers thread
    /// the same NAME-then-`T::compile_from_args` split through ONE named
    /// projection function, and the structural rejection chain
    /// (`NamedFormMissingName` for the missing NAME slot,
    /// `NamedFormNonSymbolName` for the non-symbol NAME slot,
    /// `T::compile_from_args`'s typed-entry kwargs gate) fires in the
    /// same order under both postures.
    ///
    /// Sibling of [`Self::compile_typed`] — that method compiles
    /// `(T::KEYWORD :k v …)` forms (no positional NAME slot) into a
    /// typed `T`; this method compiles `(T::KEYWORD NAME :k v …)` forms
    /// (positional NAME slot) into a typed [`NamedDefinition<T>`]. The
    /// two together close the typed-dispatcher family at the
    /// `RealizedCompiler` boundary, parallel to how
    /// [`crate::compile_typed`] / [`crate::compile_named`] close the
    /// family at the fresh-expander boundary. Together with the
    /// existing [`Self::compile`] (returns expanded `Vec<Sexp>` for
    /// untyped consumers like `tatara-check`'s per-form dispatcher),
    /// the three methods name the canonical surfaces a
    /// `RealizedCompiler` exposes: untyped expansion, typed bare-kwargs
    /// compilation, typed NAME-then-kwargs compilation.
    ///
    /// Per-call posture matches [`Self::compile_typed`]: the preloaded
    /// expander is cloned per call so cache is shared and per-call
    /// `defmacro` absorption stays local. The cloned expander's
    /// `expand_program` step runs FIRST (registering any `defmacro` in
    /// the source into the clone AND expanding macro calls), THEN the
    /// typed-keyword filter walks the expanded forms — exactly the
    /// pipeline `expand_and_collect_calls_to` composes.
    ///
    /// Theory anchor: same as [`Self::compile_typed`]. THEORY.md §VI.1
    /// (diminishing-returns theorem + generation over composition),
    /// THEORY.md §II.1 invariant 1 (typed entry on the preloaded
    /// posture), THEORY.md §II.1 invariant 2 (free middle; both
    /// postures route through the SAME projection).
    pub fn compile_named<T: TataraDomain>(&self, src: &str) -> Result<Vec<NamedDefinition<T>>> {
        self.compile_named_from_forms::<T>(crate::reader::read(src)?)
    }

    /// Compile every `(T::KEYWORD NAME :k v …)` form in `forms` into a typed
    /// [`NamedDefinition<T>`] through THIS `RealizedCompiler`'s preloaded
    /// macro library — the from-forms posture of [`Self::compile_named`].
    ///
    /// Routes through [`Expander::expand_to_named::<T>`] on a clone of the
    /// preloaded expander — the SAME typed primitive [`Self::compile_named`]'s
    /// from-source delegation ultimately threads through, and the SAME
    /// primitive [`crate::compile_named_from_forms`] (the fresh-expander
    /// posture's from-forms sibling) routes through. The named-form
    /// structural rejection chain (`NamedFormMissingName` for the missing
    /// NAME slot, `NamedFormNonSymbolName` for the non-symbol NAME slot,
    /// `T::compile_from_args`'s typed-entry kwargs gate) fires in source
    /// order under all three consumers — fresh from-forms, preloaded
    /// from-source, and preloaded from-forms — because the
    /// [`named_form_projection::<T>`](crate::compile::named_form_projection)
    /// helper is bound at ONE site that every cell of the matrix delegates
    /// through.
    ///
    /// Sibling of [`Self::compile_typed_from_forms`] — together the two
    /// methods close the from-forms row of the dispatcher matrix at the
    /// preloaded boundary, parallel to how
    /// [`crate::compile_typed_from_forms`] /
    /// [`crate::compile_named_from_forms`] close the from-forms row at the
    /// fresh-expander free-function boundary. Per-call posture mirrors
    /// [`Self::compile_named`]: cloned expander, shared cache, local
    /// defmacro absorption.
    pub fn compile_named_from_forms<T: TataraDomain>(
        &self,
        forms: Vec<Sexp>,
    ) -> Result<Vec<NamedDefinition<T>>> {
        self.cloned_preloaded().expand_to_named::<T>(forms)
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
        // Per-spec-macro `:macros` source absorption routes through the
        // substrate's composed read-then-expand primitive
        // [`Expander::expand_source_program`]: source is read into top-level
        // forms and `expand_program` registers every `defmacro` /
        // `defpoint-template` / `defcheck` head into `preloaded.macros` as
        // a side-effect (the returned `Vec<Sexp>` of non-defmacro forms is
        // discarded — spec macros are libraries, not programs). Sibling
        // consumer to `RealizedCompiler::compile` — both routes thread
        // their per-site expander posture (`&mut preloaded` here for the
        // shared build-up, `self.preloaded.clone()` there for the per-call
        // clone) through the SAME read-then-expand primitive rather than
        // re-deriving the two-step `read(src)? + expand_program(forms)`
        // chain at every consumer.
        preloaded.expand_source_program(macro_src)?;
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
    use crate::reader::read;

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

    // ── RealizedCompiler::compile_typed / compile_named ────────────────
    //
    // The preloaded-expander posture of the typed-dispatcher family. Both
    // methods route through the SAME `Expander::expand_and_collect_calls_to`
    // primitive that the fresh-expander posture (`compile_typed`,
    // `compile_named_from_forms`) routes through, with two differences:
    // (1) the expander is a clone of THIS `RealizedCompiler`'s preloaded
    // expander, not a fresh `Expander::new()`, so the spec's `:macros`
    // library participates in the expansion; (2) the cache is shared
    // across calls via `Arc<Mutex<...>>`. Tests below pin both halves:
    // the bare typed dispatch through the preloaded posture (positive
    // controls) AND the preloaded-macro participation (the key
    // compounding property — a macro authored in the spec's `:macros`
    // slot is invoked in the user's source and the typed dispatcher
    // resolves it through the SAME preloaded library that powers
    // `compile`).

    #[test]
    fn realized_compiler_compile_typed_dispatches_to_typed_t_with_empty_preloaded() {
        // Positive control: a `RealizedCompiler` with NO preloaded macros
        // can still compile a `(defcompiler …)` form through the typed
        // dispatcher — the method is a strict generalization of
        // `crate::compile_typed::<CompilerSpec>(src)` in the
        // empty-preloaded posture.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let specs = parent
            .compile_typed::<CompilerSpec>(r#"(defcompiler :name "child" :dialect "standard")"#)
            .unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "child");
    }

    #[test]
    fn realized_compiler_compile_typed_routes_preloaded_macros_into_typed_dispatch() {
        // The key compounding property: the preloaded macro library
        // participates in the typed dispatch. The parent compiler has a
        // `(defmacro mk-compiler-spec …)` registered in its preloaded
        // expander; the user's source invokes the macro, which expands
        // to a `(defcompiler :name "lifted-by-macro" …)` form; the typed
        // dispatcher then routes the expanded form through
        // `CompilerSpec::compile_from_args` and yields the
        // structurally-named child spec.
        //
        // The fresh-expander posture (`crate::compile_typed::<CompilerSpec>`)
        // sees the SAME user source and yields an EMPTY `Vec<CompilerSpec>`
        // because the head `mk-compiler-spec` is unknown to the fresh
        // expander and doesn't match `CompilerSpec::KEYWORD`. The
        // divergence between the two postures IS the compounding
        // property: which expansion strategy you picked (the
        // generation-time `compile_typed` vs. the realization-time
        // `RealizedCompiler::compile_typed`) changes whether the
        // preloaded library participates.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![
                "(defmacro mk-compiler-spec (n) `(defcompiler :name ,n :dialect \"standard\"))"
                    .into(),
            ],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let src = r#"(mk-compiler-spec "lifted-by-macro")"#;
        let specs = parent.compile_typed::<CompilerSpec>(src).unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "lifted-by-macro");

        // Pin the posture divergence: the fresh-expander dispatcher sees
        // no `(defcompiler …)` form (the unknown macro call survives as
        // `(mk-compiler-spec "lifted-by-macro")`, whose head doesn't
        // match `CompilerSpec::KEYWORD`) and yields an empty Vec.
        let fresh = crate::compile::compile_typed::<CompilerSpec>(src).unwrap();
        assert!(
            fresh.is_empty(),
            "fresh-expander posture must NOT see the preloaded macro, got: {fresh:?}"
        );
    }

    #[test]
    fn realized_compiler_compile_named_dispatches_to_named_definition() {
        // Positive control for the named-form posture: a `RealizedCompiler`
        // with empty preloaded macros can still compile a
        // `(defcompiler NAME :k v …)` form into a typed
        // `NamedDefinition<CompilerSpec>` through the preloaded-typed
        // dispatcher. Same shape as `compile_named::<CompilerSpec>(src)`
        // in the fresh-expander posture, but routed through THIS
        // `RealizedCompiler`'s preloaded expander.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let defs = parent
            .compile_named::<CompilerSpec>(
                r#"(defcompiler my-compiler :name "x" :dialect "standard")"#,
            )
            .unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "my-compiler");
        assert_eq!(defs[0].spec.name, "x");
    }

    #[test]
    fn realized_compiler_compile_named_routes_preloaded_macros_into_named_dispatch() {
        // The named-form sibling of the typed-dispatch participation
        // test. A preloaded `(defmacro mk-named …)` expands to a
        // `(defcompiler NAME :k v …)` form, which the typed
        // dispatcher routes through `named_form_projection::<CompilerSpec>`
        // to yield `NamedDefinition<CompilerSpec>`. Pins that the
        // preloaded-expander posture's named-form dispatcher routes
        // through the SAME `named_form_projection` helper as the
        // fresh-expander posture's named-form dispatcher
        // (`compile_named_from_forms`).
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![
                "(defmacro mk-named (slug) `(defcompiler ,slug :name \"x\" :dialect \"standard\"))"
                    .into(),
            ],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let defs = parent
            .compile_named::<CompilerSpec>("(mk-named child-compiler)")
            .unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "child-compiler");
        assert_eq!(defs[0].spec.name, "x");
    }

    #[test]
    fn realized_compiler_compile_named_rejects_missing_name_through_named_form_projection() {
        // Path-uniformity with the fresh-expander posture's structural
        // rejection chain: the preloaded posture goes through the SAME
        // `named_form_projection<T>` helper as `compile_named_from_forms`,
        // so the structural `NamedFormMissingName` variant fires
        // identically here for the missing-NAME case
        // (`(defcompiler)` — head matches but no NAME slot). A
        // regression that drifts the preloaded posture's rejection
        // chain from the fresh posture's (e.g. emits a `Compile`-shaped
        // diagnostic instead of the structural variant, or fires a
        // different variant at the missing-NAME gate) fails loudly
        // here. The structural-completeness floor (every named-form
        // dispatcher emits the SAME rejection variant at the SAME
        // gate) extends from the fresh posture to the preloaded
        // posture through ONE shared projection function.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let err = parent
            .compile_named::<CompilerSpec>("(defcompiler)")
            .unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormMissingName {
                    keyword: "defcompiler",
                }
            ),
            "expected NamedFormMissingName, got: {err:?}"
        );
    }

    #[test]
    fn realized_compiler_compile_typed_does_not_mutate_preloaded_state() {
        // Per-call clone isolation pin: the preloaded expander is cloned
        // per call, so a `defmacro` defined in the user's source lands
        // in the clone, not in the persistent realized compiler's
        // expander. The SAME `RealizedCompiler` invoked twice must NOT
        // accumulate macros across calls — each call sees only the
        // spec's original `:macros` library plus the in-source
        // `defmacro` forms of THAT call.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        // Call 1: defines `mk-x` in the source, uses it; the clone
        // absorbs the defmacro for the duration of the call.
        let specs1 = parent
            .compile_typed::<CompilerSpec>(
                r#"(defmacro mk-x (n) `(defcompiler :name ,n :dialect "standard"))
                   (mk-x "first")"#,
            )
            .unwrap();
        assert_eq!(specs1.len(), 1);
        assert_eq!(specs1[0].name, "first");
        // Call 2: the SAME `parent` invoked WITHOUT defining `mk-x` —
        // the preloaded expander did NOT absorb the previous call's
        // defmacro, so `(mk-x "second")` is unknown and the form's
        // head doesn't match `CompilerSpec::KEYWORD`; the result is
        // empty.
        let specs2 = parent
            .compile_typed::<CompilerSpec>(r#"(mk-x "second")"#)
            .unwrap();
        assert!(
            specs2.is_empty(),
            "per-call defmacro absorption must NOT leak into the realized compiler's preloaded expander, got: {specs2:?}"
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

    // ── RealizedCompiler::compile_from_forms / compile_typed_from_forms /
    //    compile_named_from_forms — close the from-forms row on the preloaded
    //    boundary
    //
    // The preloaded-expander posture's from-forms cells were missing pre-lift.
    // The free-function family closed the from-forms × {typed, named} cells at
    // the fresh-expander boundary (`compile_typed_from_forms` /
    // `compile_named_from_forms`); the `Expander` surface closed the
    // from-forms × {typed, named} cells through the typed-pair primitives
    // (`expand_to_typed` / `expand_to_named`). The preloaded boundary on
    // `RealizedCompiler` had only the from-source cells (`compile_typed` /
    // `compile_named`), and the untyped from-forms cell paired with `compile`
    // was missing too. After this lift the matrix is symmetric across all
    // three axes: expander posture (fresh + preloaded) × input posture
    // (from-forms + from-source) × form shape (untyped + typed + named).
    //
    // Tests below pin: (a) parity with the from-source sibling on parse(src),
    // (b) path-uniformity through the same typed primitive on `Expander` that
    // the from-source sibling delegates to, (c) the preloaded-macro
    // participation property (the key compounding promise — a macro authored
    // in the spec's `:macros` slot expands inside the from-forms dispatcher
    // through the SAME preloaded library that powers the from-source
    // sibling), (d) per-call clone isolation (the preloaded expander is NOT
    // mutated across calls), (e) the named-form structural rejection chain
    // fires identically through the from-forms preloaded dispatcher.

    #[test]
    fn realized_compiler_compile_typed_from_forms_yields_same_vec_as_compile_typed_on_parse_src() {
        // Pin parity at the preloaded boundary: feeding pre-read forms
        // through `RealizedCompiler::compile_typed_from_forms::<T>` is
        // byte-identical to feeding the source through `compile_typed::<T>`
        // on the same realized compiler. Both postures route through the
        // SAME typed primitive on the SAME preloaded expander clone — the
        // from-source method is `read(src)? + expand_to_typed(forms)` and
        // the from-forms method is the second leg of that composition
        // surfaced as ONE preloaded primitive.
        // Fail-before-pass-after: the new method must exist AND yield the
        // same Vec<T> the from-source sibling does — pre-lift there was no
        // from-forms typed method on RealizedCompiler.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let src = r#"(defcompiler :name "alpha" :dialect "standard")
                     (defcompiler :name "beta" :dialect "standard")"#;
        let forms = crate::reader::read(src).expect("read must succeed");
        let via_forms = parent
            .compile_typed_from_forms::<CompilerSpec>(forms)
            .expect("from-forms typed preloaded method must yield Vec<T>");
        let via_source = parent
            .compile_typed::<CompilerSpec>(src)
            .expect("from-source typed preloaded method must yield Vec<T>");
        assert_eq!(via_forms.len(), 2);
        assert_eq!(via_forms.len(), via_source.len());
        assert_eq!(via_forms[0].name, via_source[0].name);
        assert_eq!(via_forms[0].name, "alpha");
        assert_eq!(via_forms[1].name, via_source[1].name);
        assert_eq!(via_forms[1].name, "beta");
    }

    #[test]
    fn realized_compiler_compile_named_from_forms_yields_same_vec_as_compile_named_on_parse_src() {
        // Sibling parity pin for the named-form row at the preloaded
        // boundary: feeding pre-read forms through
        // `RealizedCompiler::compile_named_from_forms::<T>` is byte-identical
        // to feeding the source through `compile_named::<T>` on the same
        // realized compiler.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let src = r#"(defcompiler alpha-compiler :name "x" :dialect "standard")
                     (defcompiler beta-compiler  :name "y" :dialect "standard")"#;
        let forms = crate::reader::read(src).expect("read must succeed");
        let via_forms = parent
            .compile_named_from_forms::<CompilerSpec>(forms)
            .expect("from-forms named preloaded method must yield Vec<NamedDefinition<T>>");
        let via_source = parent
            .compile_named::<CompilerSpec>(src)
            .expect("from-source named preloaded method must yield Vec<NamedDefinition<T>>");
        assert_eq!(via_forms.len(), 2);
        assert_eq!(via_forms.len(), via_source.len());
        assert_eq!(via_forms[0].name, via_source[0].name);
        assert_eq!(via_forms[0].name, "alpha-compiler");
        assert_eq!(via_forms[0].spec.name, "x");
        assert_eq!(via_forms[1].name, via_source[1].name);
        assert_eq!(via_forms[1].name, "beta-compiler");
        assert_eq!(via_forms[1].spec.name, "y");
    }

    #[test]
    fn realized_compiler_compile_from_forms_yields_same_vec_as_compile_on_parse_src() {
        // Untyped sibling parity pin: feeding pre-read forms through
        // `RealizedCompiler::compile_from_forms` yields the same expanded
        // `Vec<Sexp>` `RealizedCompiler::compile` does on parse(src). The
        // from-source `compile` is `read(src)? + expand_program(forms)`;
        // this method binds the second leg directly.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec!["(defmacro when (c x) `(if ,c ,x))".into()],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let src = "(when #t (foo))";
        let forms = crate::reader::read(src).expect("read must succeed");
        let via_forms = parent
            .compile_from_forms(forms)
            .expect("from-forms untyped preloaded method must yield Vec<Sexp>");
        let via_source = parent
            .compile(src)
            .expect("from-source untyped preloaded method must yield Vec<Sexp>");
        assert_eq!(via_forms.len(), 1);
        assert_eq!(via_forms.len(), via_source.len());
        // Both postures expanded `(when …)` to `(if …)` through the SAME
        // preloaded library — proves the from-forms primitive sees the
        // spec's `:macros` slot.
        let list = via_forms[0].as_list().unwrap();
        assert_eq!(list[0].as_symbol(), Some("if"));
        let list_src = via_source[0].as_list().unwrap();
        assert_eq!(list_src[0].as_symbol(), Some("if"));
    }

    #[test]
    fn realized_compiler_compile_typed_from_forms_routes_preloaded_macros_into_typed_dispatch() {
        // The compounding property: the from-forms preloaded dispatcher
        // sees the spec's `:macros` library, identically to the from-source
        // sibling. A `(mk-compiler-spec "lifted")` form pre-parsed and fed
        // through `compile_typed_from_forms` expands through the preloaded
        // `(defmacro mk-compiler-spec …)` into `(defcompiler :name "lifted"
        // …)` and the typed dispatcher yields the structurally-named child
        // spec — the SAME outcome as feeding the source through
        // `compile_typed`.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![
                "(defmacro mk-compiler-spec (n) `(defcompiler :name ,n :dialect \"standard\"))"
                    .into(),
            ],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let src = r#"(mk-compiler-spec "lifted-by-macro-via-forms")"#;
        let forms = crate::reader::read(src).expect("read must succeed");
        let specs = parent
            .compile_typed_from_forms::<CompilerSpec>(forms)
            .expect("preloaded from-forms typed primitive must dispatch through the macro");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "lifted-by-macro-via-forms");

        // Posture-divergence control: the fresh-expander from-forms free
        // function does NOT see the macro and skips the form silently.
        // Pins that the from-forms preloaded dispatcher's expander posture
        // is THIS realized compiler's preloaded clone, not a hard-coded
        // fresh expander.
        let fresh_forms = crate::reader::read(src).expect("read must succeed");
        let fresh = crate::compile::compile_typed_from_forms::<CompilerSpec>(fresh_forms)
            .expect("fresh-expander from-forms free function must succeed");
        assert!(
            fresh.is_empty(),
            "fresh-expander posture must NOT see the preloaded macro, got: {fresh:?}"
        );
    }

    #[test]
    fn realized_compiler_compile_typed_from_forms_does_not_mutate_preloaded_state() {
        // Per-call clone isolation pin at the from-forms boundary: a
        // `defmacro` in a pre-parsed form lands in the per-call clone, not
        // in the persistent realized compiler's expander. The SAME
        // `RealizedCompiler` invoked twice through `compile_typed_from_forms`
        // must NOT accumulate macros across calls — same posture as the
        // from-source sibling's `realized_compiler_compile_typed_does_not_mutate_preloaded_state`.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let src1 = r#"(defmacro mk-y (n) `(defcompiler :name ,n :dialect "standard"))
                      (mk-y "first")"#;
        let forms1 = crate::reader::read(src1).expect("read must succeed");
        let specs1 = parent
            .compile_typed_from_forms::<CompilerSpec>(forms1)
            .unwrap();
        assert_eq!(specs1.len(), 1);
        assert_eq!(specs1[0].name, "first");
        // Call 2: the SAME `parent` invoked WITHOUT defining `mk-y` — the
        // preloaded expander did NOT absorb the previous call's defmacro.
        let forms2 = crate::reader::read(r#"(mk-y "second")"#).expect("read must succeed");
        let specs2 = parent
            .compile_typed_from_forms::<CompilerSpec>(forms2)
            .unwrap();
        assert!(
            specs2.is_empty(),
            "per-call defmacro absorption must NOT leak into the realized compiler's preloaded expander, got: {specs2:?}"
        );
    }

    #[test]
    fn realized_compiler_compile_named_from_forms_rejects_missing_name_through_named_form_projection(
    ) {
        // Path-uniformity with every other consumer of `named_form_projection`:
        // the from-forms preloaded named dispatcher emits the structural
        // `NamedFormMissingName` variant identically to the from-source
        // sibling (`RealizedCompiler::compile_named`), the fresh-expander
        // from-source sibling (`crate::compile_named`), AND the
        // fresh-expander from-forms sibling (`crate::compile_named_from_forms`).
        // The structural-completeness floor extends from the four prior
        // cells of the dispatcher matrix to this fifth one through ONE
        // shared projection function.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let forms = crate::reader::read("(defcompiler)").expect("read must succeed");
        let err = parent
            .compile_named_from_forms::<CompilerSpec>(forms)
            .unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormMissingName {
                    keyword: "defcompiler",
                }
            ),
            "expected NamedFormMissingName through preloaded from-forms primitive, got: {err:?}"
        );
    }

    #[test]
    fn realized_compiler_compile_typed_from_forms_routes_through_expand_to_typed_primitive() {
        // Compounding property: `RealizedCompiler::compile_typed_from_forms`
        // routes through the SAME `Expander::expand_to_typed::<T>` primitive
        // that every other typed-pair dispatcher in the family routes
        // through. Pin parity: the result is byte-identical to invoking
        // `expand_to_typed` directly on a clone of the SAME preloaded
        // expander with the same forms. A regression that drifts this
        // method's binding from the typed primitive (e.g. re-derives the
        // inline `expand_and_collect_calls_to(forms, T::KEYWORD,
        // T::compile_from_args)` triple at the method's call site) would
        // fail loudly here.
        //
        // We can't reach `parent.preloaded` directly (it's private), so
        // we reproduce the posture by constructing a sibling
        // RealizedCompiler with the same `:macros` library and feeding the
        // same forms through the typed primitive on its preloaded clone.
        // The two postures are observationally identical when the macro
        // library is the same and the input forms are the same.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let src = r#"(defcompiler :name "p" :dialect "standard")
                     (defcompiler :name "q" :dialect "standard")"#;
        let forms_a = crate::reader::read(src).expect("read must succeed");
        let forms_b = crate::reader::read(src).expect("read must succeed");
        let via_method = parent
            .compile_typed_from_forms::<CompilerSpec>(forms_a)
            .expect("preloaded method must yield Vec<T>");
        let via_fresh_expander_through_primitive = Expander::new()
            .expand_to_typed::<CompilerSpec>(forms_b)
            .expect("Expander primitive must yield Vec<T>");
        assert_eq!(via_method.len(), 2);
        assert_eq!(via_method.len(), via_fresh_expander_through_primitive.len());
        assert_eq!(
            via_method[0].name,
            via_fresh_expander_through_primitive[0].name
        );
        assert_eq!(via_method[0].name, "p");
        assert_eq!(
            via_method[1].name,
            via_fresh_expander_through_primitive[1].name
        );
        assert_eq!(via_method[1].name, "q");
    }

    // ── cloned_preloaded: the per-call clone projection ─────────────────
    //
    // `cloned_preloaded(&self) -> Expander` lifts the `self.preloaded.clone()`
    // projection that lived inline at six sites (three from-forms dispatchers
    // and three from-source dispatchers) into ONE named primitive on the
    // `RealizedCompiler` algebra. The companion lift (from-source dispatchers
    // delegate to their from-forms siblings) narrows the projection to THREE
    // sites at the from-forms row, all of which route through this helper.
    //
    // Tests below pin the two load-bearing clone semantics:
    //   (a) per-call clone ISOLATION — `defmacro` heads absorbed into the
    //       returned clone do NOT leak into the persistent `preloaded`
    //       expander, so two consecutive calls start from the spec's
    //       original `:macros` library.
    //   (b) per-call clone INHERITS the spec's preloaded macro library —
    //       a `:macros` entry registered at realization time is visible
    //       through the clone the first time AND every subsequent time.
    //
    // Pre-lift the projection had no name; these tests pin the named
    // primitive's contract directly. The existing parity tests
    // (`realized_compiler_compile_*_yields_same_vec_as_*_on_parse_src`) are
    // the path-uniformity guards proving every dispatcher routes through this
    // helper without behavior drift.

    #[test]
    fn cloned_preloaded_isolates_per_call_defmacro_absorption() {
        // Pin clone semantic (a) — the returned clone's macro table is a deep
        // copy, so a `defmacro` registered into the clone does NOT mutate the
        // persistent `preloaded` expander. A second call to `cloned_preloaded`
        // yields a fresh clone that does NOT see the first call's absorption.
        // This is exactly the property that lets `RealizedCompiler::compile*`
        // be safe to call repeatedly with user source containing `defmacro` —
        // each call's absorption stays local to that call's clone.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let mut clone1 = parent.cloned_preloaded();
        assert!(
            !clone1.has("foo"),
            "first clone must not have user-defined foo yet"
        );
        clone1
            .expand_program(crate::reader::read("(defmacro foo (x) x)").unwrap())
            .expect("registering defmacro into the clone must succeed");
        assert!(
            clone1.has("foo"),
            "first clone must absorb the defmacro locally"
        );
        let clone2 = parent.cloned_preloaded();
        assert!(
            !clone2.has("foo"),
            "second clone must NOT see clone1's absorbed defmacro — per-call clones are isolated"
        );
    }

    #[test]
    fn cloned_preloaded_carries_spec_macros_into_every_clone() {
        // Pin clone semantic (b) — the clone inherits the spec's `:macros`
        // library, so every dispatcher invocation through the helper sees
        // the realization-time-registered macros. Pin across TWO clones to
        // prove the spec library is in `preloaded` (not in a one-shot
        // construction-time path that the clone would miss).
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec!["(defmacro when (c x) `(if ,c ,x))".into()],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        assert!(
            parent.cloned_preloaded().has("when"),
            "first clone must carry the spec's :macros library"
        );
        assert!(
            parent.cloned_preloaded().has("when"),
            "second clone must also carry the spec's :macros library"
        );
    }

    #[test]
    fn cloned_preloaded_shares_cache_arc_across_clones() {
        // Pin clone semantic (a)'s complement — the cache is `Arc<Mutex<…>>`
        // so two clones share the SAME memoization table. Realizations of the
        // same `CompilerSpec` benefit from each other's cache hits across
        // `.compile*()` invocations — this is the property that makes the
        // shared-cache + isolated-macros split coherent.
        //
        // We pin shared-cache by emptying through one clone and observing
        // through another. `clear_cache` operates through the Arc, so a
        // clear on clone1 is visible to clone2.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec!["(defmacro id (x) x)".into()],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let mut clone1 = parent.cloned_preloaded();
        // Drive a single expansion through clone1; cache is populated through
        // the shared Arc.
        clone1
            .expand_program(crate::reader::read("(id 42)").unwrap())
            .expect("expansion through clone1 must succeed");
        let clone2 = parent.cloned_preloaded();
        // Clearing through clone2 affects the shared cache visible to clone1.
        clone2.clear_cache();
        assert_eq!(
            clone1.cache_size(),
            0,
            "clear through clone2 must drain clone1's cache — shared Arc"
        );
    }

    // ── from-source-delegates-to-from-forms routing on RealizedCompiler ──
    //
    // The companion lift to `cloned_preloaded`. Each from-source dispatcher
    // (`compile` / `compile_typed` / `compile_named`) now routes through its
    // from-forms sibling — `<from_forms_sibling>(crate::reader::read(src)?)`
    // — so the per-call clone discipline lives in ONE place per form-shape
    // (the from-forms row of the dispatcher matrix) rather than being
    // re-derived at every dispatcher's call site. Mirrors the
    // `Expander::expand_source_program → expand_program` /
    // `expand_source_to_typed → expand_to_typed` delegation pattern at the
    // expander boundary, so the `read(src)? + <from_forms_sibling>(forms)`
    // composition is the canonical from-source shape at BOTH the expander
    // boundary AND the realized-compiler boundary.
    //
    // The tests below pin: feeding pre-read forms through the from-forms
    // primitive yields the SAME `Vec<Sexp>` / `Vec<T>` / `Vec<NamedDefinition<T>>`
    // the from-source primitive yields on the source those forms came from.
    // Pre-lift the from-source primitives bypassed the from-forms primitives
    // entirely (each routed directly through its `expand_source_*` peer on
    // `Expander`), so a future emitter that added side-effects only to the
    // from-forms primitive would silently miss the from-source path. Post-lift
    // the from-source path IS the from-forms path under delegation — any
    // future side-effect added at the from-forms boundary inherits structurally.

    #[test]
    fn realized_compiler_compile_routes_through_compile_from_forms_under_delegation() {
        // Pin the new routing for the untyped untyped-expansion dispatcher.
        // The from-source `compile(src)` now == `compile_from_forms(read(src)?)`.
        // A pre-parsed form list fed through `compile_from_forms` yields the
        // SAME `Vec<Sexp>` that `compile(src)` yields on the source those
        // forms came from. Together with the existing
        // `realized_compiler_compile_from_forms_yields_same_vec_as_compile_on_parse_src`
        // test, this pins both halves of the delegation: behavior parity AND
        // routing identity.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec!["(defmacro when (c x) `(if ,c ,x))".into()],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let src = "(when #t (foo)) (when #t (bar))";
        let forms = crate::reader::read(src).expect("read must succeed");
        let via_source = parent
            .compile(src)
            .expect("from-source must yield Vec<Sexp>");
        let via_forms = parent
            .compile_from_forms(forms)
            .expect("from-forms must yield Vec<Sexp>");
        assert_eq!(via_source.len(), 2);
        assert_eq!(via_source.len(), via_forms.len());
        // Pin Sexp-level equality: both routes go through the SAME preloaded
        // macro library and produce structurally identical expanded forms.
        assert_eq!(via_source[0], via_forms[0]);
        assert_eq!(via_source[1], via_forms[1]);
    }

    #[test]
    fn realized_compiler_compile_typed_routes_through_compile_typed_from_forms_under_delegation() {
        // Pin the new routing for the typed bare-kwargs dispatcher.
        // `compile_typed::<T>(src)` now == `compile_typed_from_forms::<T>(read(src)?)`.
        // A pre-parsed form list fed through `compile_typed_from_forms` yields
        // the SAME `Vec<T>` that `compile_typed(src)` yields on the source
        // those forms came from. The typed-pair `(T::KEYWORD,
        // T::compile_from_args)` binding lives in ONE place per posture (the
        // from-forms primitive on `Expander`); the from-source dispatcher
        // inherits the binding through TWO delegation hops
        // (RealizedCompiler from-source → RealizedCompiler from-forms →
        // Expander from-forms).
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let src = r#"(defcompiler :name "alpha" :dialect "standard")
                     (defcompiler :name "beta"  :dialect "standard")"#;
        let forms = crate::reader::read(src).expect("read must succeed");
        let via_source = parent
            .compile_typed::<CompilerSpec>(src)
            .expect("from-source typed must yield Vec<T>");
        let via_forms = parent
            .compile_typed_from_forms::<CompilerSpec>(forms)
            .expect("from-forms typed must yield Vec<T>");
        assert_eq!(via_source.len(), 2);
        assert_eq!(via_source.len(), via_forms.len());
        assert_eq!(via_source[0].name, via_forms[0].name);
        assert_eq!(via_source[0].name, "alpha");
        assert_eq!(via_source[1].name, via_forms[1].name);
        assert_eq!(via_source[1].name, "beta");
    }

    #[test]
    fn realized_compiler_compile_named_routes_through_compile_named_from_forms_under_delegation() {
        // Pin the new routing for the named NAME-then-kwargs dispatcher.
        // `compile_named::<T>(src)` now ==
        // `compile_named_from_forms::<T>(read(src)?)`. A pre-parsed form list
        // fed through `compile_named_from_forms` yields the SAME
        // `Vec<NamedDefinition<T>>` that `compile_named(src)` yields on the
        // source those forms came from. The named-form structural rejection
        // chain (`NamedFormMissingName`, `NamedFormNonSymbolName`,
        // `T::compile_from_args` typed-entry gate) is sourced from ONE
        // projection function (`named_form_projection::<T>`) and reaches the
        // from-source dispatcher via TWO delegation hops.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        let src = r#"(defcompiler alpha-compiler :name "x" :dialect "standard")
                     (defcompiler beta-compiler  :name "y" :dialect "standard")"#;
        let forms = crate::reader::read(src).expect("read must succeed");
        let via_source = parent
            .compile_named::<CompilerSpec>(src)
            .expect("from-source named must yield Vec<NamedDefinition<T>>");
        let via_forms = parent
            .compile_named_from_forms::<CompilerSpec>(forms)
            .expect("from-forms named must yield Vec<NamedDefinition<T>>");
        assert_eq!(via_source.len(), 2);
        assert_eq!(via_source.len(), via_forms.len());
        assert_eq!(via_source[0].name, via_forms[0].name);
        assert_eq!(via_source[0].name, "alpha-compiler");
        assert_eq!(via_source[0].spec.name, via_forms[0].spec.name);
        assert_eq!(via_source[1].name, via_forms[1].name);
        assert_eq!(via_source[1].name, "beta-compiler");
    }

    #[test]
    fn realized_compiler_compile_typed_short_circuits_at_reader_error_before_clone() {
        // Pin the structural ordering preserved by the delegation: a reader
        // error (lexer / parser / unbalanced-paren / unterminated-string)
        // short-circuits BEFORE `cloned_preloaded` runs. The from-source
        // dispatcher's first step is `crate::reader::read(src)?`; if that
        // fails the `?` propagates and the per-call clone is never
        // materialized. Pre-lift the same property held (the `?` lived inside
        // `expand_source_to_typed`); post-lift it holds at the `compile_typed`
        // call boundary directly. The test pins the rejection variant
        // identity (`LispError::Reader`) so a future emitter that drifts the
        // delegation order (e.g. clone-then-read, which would materialize a
        // clone that's then immediately discarded) would still produce a
        // reader-error result — but pin the variant identity so the
        // delegation order stays observable through the structural rejection.
        let parent = realize_in_memory(CompilerSpec {
            name: "parent".into(),
            dialect: "standard".into(),
            macros: vec![],
            domains: vec![],
            optimization: "tree-walk".into(),
            description: None,
        })
        .unwrap();
        // Unbalanced open paren — the reader rejects before any expander step.
        let err = parent
            .compile_typed::<CompilerSpec>("(defcompiler :name \"x\"")
            .unwrap_err();
        assert!(
            matches!(err, LispError::UnmatchedOpenParen { .. }),
            "expected LispError::UnmatchedOpenParen through from-source delegation, got: {err:?}"
        );
    }
}
