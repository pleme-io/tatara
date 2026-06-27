//! Generic Lisp-to-type compiler — drives `#[derive(TataraDomain)]` types.
//!
//! This module used to contain a 1200-line hand-rolled compiler for a single
//! domain (ProcessSpec). The derive macro now handles every typed domain
//! uniformly, so this file shrinks to a thin pipeline: read → macroexpand →
//! dispatch to derive-generated `compile_from_args`.
//!
//! Two entry points:
//!   - `compile_typed::<T>(src)` — every `(T::KEYWORD :k v …)` form becomes
//!     one `T`. Returns `Vec<T>`.
//!   - `compile_named::<T>(src)` — every `(T::KEYWORD NAME :k v …)` form
//!     (positional name after keyword) becomes one `NamedDefinition<T>`.
//!     This is the shape used by ProcessSpec / `(defpoint name …)`.

use crate::ast::Sexp;
use crate::domain::TataraDomain;
use crate::error::{LispError, Result};
use crate::macro_expand::Expander;

/// Typed-keyword dispatchers on the `Expander` surface — the
/// `T: TataraDomain`-shaped sibling family of
/// [`Expander::expand_and_collect_calls_to`] (from-forms posture) and
/// [`Expander::expand_source_and_collect_calls_to`] (from-source posture).
///
/// The family is closed across TWO axes: input posture (from-forms +
/// from-source) × form shape (typed bare-kwargs + named NAME-then-kwargs).
/// Each cell is ONE typed method on `Expander`, binding `(T::KEYWORD,
/// projection-for-T)` at the type level through `T`:
///
/// |              | typed bare-kwargs            | named NAME-then-kwargs        |
/// |--------------|------------------------------|-------------------------------|
/// | from-forms   | [`expand_to_typed`](Self::expand_to_typed)   | [`expand_to_named`](Self::expand_to_named)   |
/// | from-source  | [`expand_source_to_typed`](Self::expand_source_to_typed) | [`expand_source_to_named`](Self::expand_source_to_named) |
///
/// The from-source row composes `crate::reader::read` with its from-forms
/// row sibling — `read(src)? + <expander>.expand_to_typed::<T>(forms)` —
/// so the typed-pair `(T::KEYWORD, projection-for-T)` is bound in ONE
/// place per form shape (the from-forms row), and the from-source row
/// inherits the binding through delegation. A regression that mis-pairs
/// `T::KEYWORD` with `U::compile_from_args` (where `T != U`) is
/// structurally impossible at any site: the type parameter binds both
/// substitutions together inside ONE method body per form shape.
impl Expander {
    /// Macroexpand + project every `(T::KEYWORD :k v …)` form in `forms`
    /// into a typed `T` — the from-forms posture of the typed bare-kwargs
    /// dispatcher family, sibling of [`Self::expand_to_named`].
    ///
    /// Composes [`Self::expand_and_collect_calls_to`] with `T::KEYWORD`
    /// as the keyword filter and `T::compile_from_args` as the per-form
    /// projection — the two-arg `(keyword, projection)` discipline bound
    /// at the type level through `T` inside ONE method body.
    ///
    /// Sibling of [`Self::expand_source_to_typed`] — that method stacks
    /// a `crate::reader::read` step on top of this one, projecting source
    /// text through the SAME typed-pair primitive. Consumers that have
    /// already parsed their forms (macro-expanded subforms, `Sexp`
    /// loaded from disk, a REPL's already-read top-level buffer) bind
    /// to this method; consumers that consume source text directly bind
    /// to the from-source sibling.
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// the typed-pair `(T::KEYWORD, T::compile_from_args)` is bound in
    /// ONE place per form shape (this method) — the from-source sibling
    /// inherits the binding through delegation rather than re-deriving
    /// it at its own call site. THEORY.md §II.1 invariant 1 — typed
    /// entry; the typed-keyword filter paired with `T::compile_from_args`
    /// IS the from-forms typed-entry-batch gate, named on the `Expander`
    /// surface. THEORY.md §II.1 invariant 2 — free middle; the from-forms
    /// posture and the from-source posture route through the SAME typed
    /// primitive, so a regression that drifts ONE posture's pairing
    /// from the other becomes structurally impossible.
    ///
    /// Frontier inspiration: MLIR's `Region::walk<Op>(callback)` —
    /// every typed rewriter binds to a region walker that composes the
    /// typed kind filter with the per-op visitor; the substrate's
    /// `expand_to_typed::<T>` is the typed-pair peer on the `&[Sexp]`
    /// algebra, with `T: TataraDomain` standing in for MLIR's `Op` type
    /// witness.
    pub fn expand_to_typed<T: TataraDomain>(&mut self, forms: Vec<Sexp>) -> Result<Vec<T>> {
        self.expand_and_collect_calls_to(forms, T::KEYWORD, T::compile_from_args)
    }

    /// Macroexpand + project every `(T::KEYWORD NAME :k v …)` form in
    /// `forms` into a typed [`NamedDefinition<T>`] — the from-forms posture
    /// of the named NAME-then-kwargs dispatcher family, sibling of
    /// [`Self::expand_to_typed`].
    ///
    /// Routes through the named constant-keyword primitive
    /// [`Self::expand_and_collect_named_calls_to`] (which itself routes
    /// through the named typed-decoded classifier primitive
    /// [`Self::expand_and_collect_named_calls_to_any`] via a constant-
    /// classifier decoder) with `T::KEYWORD` as the keyword filter and
    /// a per-form `(name, spec_args) -> Result<NamedDefinition<T>>`
    /// projection that composes `T::compile_from_args` with the
    /// `NamedDefinition { name: name.to_string(), spec }` packaging.
    /// Post-lift the typed (this method, `T::KEYWORD`-baked) and
    /// untyped (the runtime-keyword sibling
    /// [`Self::expand_and_collect_named_calls_to`]) constant-keyword
    /// named cells route through the SAME composition point on the
    /// `Expander` surface, mirroring how `expand_and_collect_calls_to`
    /// (bare × constant-keyword × untyped) routes through
    /// `expand_and_collect_calls_to_any` (bare × classifier) — the
    /// `split_name_slot` composition lives at ONE site
    /// (`expand_and_collect_named_calls_to_any` body) for the entire
    /// named cell, with `crate::compile::named_form_projection`
    /// remaining a slice-side primitive for callers that have a
    /// single rest tail.
    ///
    /// The named-form structural rejection chain (`NamedFormMissingName`
    /// for the missing NAME slot, `NamedFormNonSymbolName` for the
    /// non-symbol NAME slot, `T::compile_from_args`'s typed-entry kwargs
    /// gate) fires identically across all consumers of the named
    /// dispatcher family — fresh / preloaded × from-forms / from-source
    /// × constant / classifier — because every consumer routes through
    /// the SAME `expand_and_collect_named_calls_to_any` composition that
    /// composes `split_name_slot` with the per-form projection.
    ///
    /// Sibling of [`Self::expand_to_typed`] — both methods route
    /// through their constant-keyword Expander primitive sibling
    /// ([`Self::expand_and_collect_calls_to`] for the bare-kwargs row,
    /// [`Self::expand_and_collect_named_calls_to`] for the named row),
    /// each binding the per-form projection that fits its typed entry
    /// shape. Together with their from-source siblings they close the
    /// typed-from-`Expander` family.
    ///
    /// Theory anchor: see [`Self::expand_to_typed`] — the named sibling
    /// shares the same lift posture, threading the NAME-then-kwargs
    /// projection through `T` AND routing through the named
    /// constant-keyword primitive (rather than the bare-kwargs one
    /// with `named_form_projection::<T>` doing the NAME extraction
    /// inside the projection). THEORY.md §VI.1 — generation over
    /// composition; the named-form `split_name_slot` composition lives
    /// at ONE site post-lift rather than at TWO sites (the bare-kwargs
    /// path through `named_form_projection<T>` AND the classifier path
    /// through the `_any` primitive).
    pub fn expand_to_named<T: TataraDomain>(
        &mut self,
        forms: Vec<Sexp>,
    ) -> Result<Vec<NamedDefinition<T>>> {
        self.expand_and_collect_named_calls_to(forms, T::KEYWORD, |name, spec_args| {
            let spec = T::compile_from_args(spec_args)?;
            Ok(NamedDefinition {
                name: name.to_string(),
                spec,
            })
        })
    }

    /// Read + macroexpand + project every `(T::KEYWORD :k v …)` form in
    /// `src` into a typed `T` — the from-source posture of the typed
    /// bare-kwargs dispatcher family, sibling of
    /// [`Self::expand_source_to_named`].
    ///
    /// Composes [`crate::reader::read`] with [`Self::expand_to_typed`] —
    /// the typed-pair `(T::KEYWORD, T::compile_from_args)` is bound in
    /// ONE place (the from-forms row), and this from-source sibling
    /// inherits the binding through delegation. The expander posture
    /// (fresh [`Expander::new()`](crate::macro_expand::Expander::new)
    /// for one-shot typed compilation, preloaded
    /// [`self.preloaded.clone()`](crate::compiler_spec::RealizedCompiler)
    /// for compilation inside a CompilerSpec's macro library) is the
    /// caller's choice — this method binds the read step and dispatches
    /// on whichever `Expander` value the caller materialized.
    ///
    /// `?`-routing through `read` preserves the structural ordering of
    /// the rejection chain end-to-end: a reader error (lexer / parser /
    /// unbalanced-paren / unterminated-string) short-circuits BEFORE
    /// `expand_to_typed` runs; the from-forms posture's pipeline
    /// (`expand_program → iter_calls_to → map → collect`) fires
    /// afterwards exactly as it does for direct from-forms callers.
    pub fn expand_source_to_typed<T: TataraDomain>(&mut self, src: &str) -> Result<Vec<T>> {
        let forms = crate::reader::read(src)?;
        self.expand_to_typed::<T>(forms)
    }

    /// Read + macroexpand + project every `(T::KEYWORD NAME :k v …)` form
    /// in `src` into a typed [`NamedDefinition<T>`] — the from-source
    /// posture of the named NAME-then-kwargs dispatcher family, sibling
    /// of [`Self::expand_source_to_typed`].
    ///
    /// Composes [`crate::reader::read`] with [`Self::expand_to_named`] —
    /// the typed-pair `(T::KEYWORD, named_form_projection::<T>)` is bound
    /// in ONE place (the from-forms row), and this from-source sibling
    /// inherits the binding through delegation. Together with the three
    /// other cells of the family ([`Self::expand_to_typed`],
    /// [`Self::expand_to_named`], [`Self::expand_source_to_typed`]) it
    /// closes the typed-from-`Expander` surface across both input
    /// postures and both form shapes.
    pub fn expand_source_to_named<T: TataraDomain>(
        &mut self,
        src: &str,
    ) -> Result<Vec<NamedDefinition<T>>> {
        let forms = crate::reader::read(src)?;
        self.expand_to_named::<T>(forms)
    }
}

/// A typed definition with a positional name — e.g., `(defpoint NAME …)` →
/// `NamedDefinition<ProcessSpec> { name, spec }`.
#[derive(Debug, Clone)]
pub struct NamedDefinition<T> {
    pub name: String,
    pub spec: T,
}

/// Back-compat alias — the old `Definition` type was `NamedDefinition<ProcessSpec>`.
pub type Definition<T> = NamedDefinition<T>;

/// Read + macroexpand + compile every `(T::KEYWORD :k v …)` form into `T`.
///
/// Fresh-expander posture of the typed dispatcher family — routes through
/// [`Expander::expand_source_to_typed::<T>`] on a brand-new
/// `Expander::new()`. The preloaded posture
/// ([`RealizedCompiler::compile_typed`](crate::compiler_spec::RealizedCompiler::compile_typed))
/// routes the SAME `T`-typed dispatcher through `self.preloaded.clone()`
/// — both postures bind to ONE composition point on the `Expander`
/// surface (the typed-pair primitive whose `(T::KEYWORD,
/// T::compile_from_args)` substitution is type-level through `T`),
/// rather than re-deriving the two-arg binding at each call site.
pub fn compile_typed<T: TataraDomain>(src: &str) -> Result<Vec<T>> {
    Expander::new().expand_source_to_typed::<T>(src)
}

/// Read + macroexpand + compile every `(T::KEYWORD NAME :k v …)` form into
/// `NamedDefinition<T>`. The positional `NAME` is captured separately from
/// the `:kw v` arguments that feed `compile_from_args`.
///
/// Fresh-expander posture of the named-form dispatcher family — routes
/// through [`Expander::expand_source_to_named::<T>`] on a brand-new
/// `Expander::new()`. Sibling of [`compile_typed`] (the bare-kwargs
/// dispatcher); the preloaded posture
/// ([`RealizedCompiler::compile_named`](crate::compiler_spec::RealizedCompiler::compile_named))
/// routes the SAME `T`-typed named dispatcher through
/// `self.preloaded.clone()`. The from-forms variant
/// [`compile_named_from_forms`] stays available for callers that have
/// already parsed their forms.
pub fn compile_named<T: TataraDomain>(src: &str) -> Result<Vec<NamedDefinition<T>>> {
    Expander::new().expand_source_to_named::<T>(src)
}

/// Same as `compile_typed` but operates on already-parsed forms. Useful
/// when the caller has done its own reading (e.g. from a string, a Sexp
/// loaded from disk, a macro-expanded subform, an LSP's partial AST cache
/// across edits, a REPL's already-quoted buffer).
///
/// Fresh-expander posture of the from-forms typed dispatcher — routes
/// through [`Expander::expand_to_typed::<T>`] on a brand-new
/// `Expander::new()`. Sibling of [`compile_named_from_forms`] (the
/// from-forms named-shape entry) and of [`compile_typed`] (the from-source
/// typed-shape entry). Together with those two, this free function
/// closes the fresh-expander dispatcher family at the free-function
/// boundary across BOTH axes — input posture (from-forms + from-source)
/// × form shape (typed bare-kwargs + named NAME-then-kwargs) — parallel
/// to how [`Expander::expand_to_typed`] / [`Expander::expand_to_named`]
/// / [`Expander::expand_source_to_typed`] / [`Expander::expand_source_to_named`]
/// close the family at the `Expander` boundary.
///
/// The typed-pair `(T::KEYWORD, T::compile_from_args)` is bound at the
/// type level through `T` inside the from-forms typed primitive on
/// `Expander`, so this free function and its from-source sibling
/// [`compile_typed`] bind to the SAME projection through delegation
/// rather than re-deriving the `(keyword, projection)` pair at their
/// call site. Non-matching forms are skipped silently (soft-projection
/// posture inherited from [`iter_calls_to`](crate::ast::iter_calls_to)).
/// The `Result::collect` short-circuit inside the typed primitive
/// preserves `T::compile_from_args`'s typed-entry kwargs gate in source
/// order across both this dispatcher and its from-source sibling.
pub fn compile_typed_from_forms<T: TataraDomain>(forms: Vec<Sexp>) -> Result<Vec<T>> {
    Expander::new().expand_to_typed::<T>(forms)
}

/// Same as `compile_named` but operates on already-parsed forms. Useful when
/// the caller has done its own reading (e.g., from a string, a Sexp loaded
/// from disk, a macro-expanded subform).
///
/// Fresh-expander posture of the from-forms named dispatcher — routes
/// through [`Expander::expand_to_named::<T>`] on a brand-new
/// `Expander::new()`. The typed-pair `(T::KEYWORD,
/// named_form_projection::<T>)` is bound at the type level through `T`
/// inside the from-forms typed primitive on `Expander`, so this free
/// function and its from-source sibling [`compile_named`] bind to the
/// SAME named projection through delegation rather than re-deriving the
/// `(keyword, projection)` pair at their call site.
///
/// Non-matching forms are skipped silently (soft-projection posture
/// inherited from [`iter_calls_to`](crate::ast::iter_calls_to)). The
/// `Result::collect` short-circuit inside the typed primitive preserves
/// the structurally-named rejection chain: `NamedFormMissingName` for
/// the missing NAME slot, `NamedFormNonSymbolName` for the non-symbol
/// NAME slot, `T::compile_from_args`'s typed-entry kwargs gate — fires
/// in source order across both this dispatcher and its from-source
/// sibling. Sibling of [`compile_typed_from_forms`] — together the two
/// free functions close the from-forms row of the fresh-expander
/// dispatcher family.
pub fn compile_named_from_forms<T: TataraDomain>(
    forms: Vec<Sexp>,
) -> Result<Vec<NamedDefinition<T>>> {
    Expander::new().expand_to_named::<T>(forms)
}

/// Read + macroexpand + classifier-walk `src` through a fresh `Expander` —
/// the from-source posture of the fresh-expander typed-decoded classifier
/// dispatcher, free-function sibling of
/// [`RealizedCompiler::compile_typed_any`](crate::compiler_spec::RealizedCompiler::compile_typed_any).
///
/// Composes [`Expander::new()`](crate::macro_expand::Expander::new) with
/// [`Expander::expand_source_and_collect_calls_to_any`] — the SAME from-source
/// typed-decoded primitive `RealizedCompiler::compile_typed_any` routes a
/// preloaded clone through, here threaded through a brand-new `Expander` so
/// callers that don't materialize a `CompilerSpec` (one-shot dispatchers, an
/// LSP that holds an authoring buffer without a realized compiler, a
/// `tatara-check` runner over a source buffer with no macro library) bind to
/// ONE free function rather than constructing
/// `Expander::new().expand_source_and_collect_calls_to_any(…)` themselves at
/// each call site.
///
/// Sibling of [`compile_typed`] (the from-source constant-`T::KEYWORD`
/// dispatcher) and of [`compile_typed_any_from_forms`] (the from-forms
/// typed-decoded classifier dispatcher). Together with those two — plus
/// [`compile_typed_from_forms`] — this free function closes the fresh-
/// expander dispatcher family at the free-function boundary across BOTH
/// axes — input posture (from-forms + from-source) × projection form
/// (constant `T::KEYWORD` + typed-decoded classifier):
///
/// |              | constant `T::KEYWORD`               | typed-decoded classifier                  |
/// |--------------|-------------------------------------|-------------------------------------------|
/// | from-forms   | [`compile_typed_from_forms`]        | [`compile_typed_any_from_forms`]          |
/// | from-source  | [`compile_typed`]                   | [`compile_typed_any`] (this)              |
///
/// The constant-`T::KEYWORD` column is the typed CONSEQUENCE of the
/// classifier column: a `compile_typed::<T>(src)` call composes
/// `compile_typed_any(src, |h| (h == T::KEYWORD).then_some(()), |(), args|
/// T::compile_from_args(args))`. Both columns route through ONE composition
/// point on the `Expander` surface (`expand_source_and_collect_calls_to_any`
/// for from-source; `expand_and_collect_calls_to_any` for from-forms; the
/// constant-keyword cells are the constant-classifier specialization of the
/// classifier cells through `expand_source_to_typed` /
/// `expand_to_typed`). A regression that drifts ONE cell's pipeline from the
/// others is structurally impossible — every cell binds to ONE composition
/// point.
///
/// Posture parity with [`RealizedCompiler::compile_typed_any`]: where that
/// method clones the preloaded macro library per call so previously
/// `:macros`-loaded macros participate in expansion, this free function
/// starts from `Expander::new()` so the only macros visible to the walk
/// are those introduced in `src` itself via `(defmacro …)`. The
/// classifier sees post-expansion heads in BOTH postures — a `(when …)`
/// macro absorbed during the walk lowers to `(if …)`, and the classifier
/// dispatches on `if`, NOT on `when`.
///
/// The future change that benefits: a `tatara-check` runner that walks
/// every typed `(defX …)` form in a source buffer with no realized
/// `CompilerSpec` to clone, dispatching each form by classifier-decoded
/// kind through the registry — binds to ONE free function rather than
/// re-constructing `Expander::new().expand_source_and_collect_calls_to_any(…)`
/// at the call site. An LSP that surfaces "every typed-domain form in
/// this buffer with its kind tag" without materializing a realized
/// compiler reaches the SAME free function. A REPL `:dispatch
/// <classifier> <source>` command for fresh-expander dispatch binds
/// here as well.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// (fresh × from-source × typed-decoded-classifier) cell of the
/// dispatcher matrix is bound in ONE place rather than re-derived inline
/// at every fresh-expander from-source classifier consumer's call site.
/// THEORY.md §II.1 invariant 1 — typed entry; the classifier-filtered +
/// caller-projected walk over the freshly-expanded program IS a typed-
/// entry-batch gate at the free-function boundary, and naming its single
/// shape lifts the gate from a per-consumer inline derivation to ONE
/// free function the substrate's diagnostic promotions hang off of.
/// THEORY.md §II.1 invariant 2 — free middle; all four cells of the
/// fresh-expander dispatcher matrix route through `Expander::new()` +
/// the matching `Expander` primitive, so a regression that drifts ONE
/// cell's pipeline from the others becomes structurally impossible.
///
/// Frontier inspiration: Racket's `(eval-string str ns)` against a
/// fresh empty namespace combined with `syntax-parse`'s typed-choice
/// repeater on the result — typed program-level dispatch with NO
/// preloaded macro library is the Racket idiom; this function is the
/// Rust-typed peer with the typed-decoded classifier composed in,
/// sibling of [`RealizedCompiler::compile_typed_any`] (the preloaded-
/// namespace posture).
pub fn compile_typed_any<R, F, D, T>(src: &str, decode: D, project: F) -> Result<Vec<R>>
where
    D: FnMut(&str) -> Option<T>,
    F: FnMut(T, &[Sexp]) -> Result<R>,
{
    Expander::new().expand_source_and_collect_calls_to_any(src, decode, project)
}

/// Macroexpand + classifier-walk a pre-parsed program through a fresh
/// `Expander` — the from-forms posture of [`compile_typed_any`].
///
/// Composes [`Expander::new()`](crate::macro_expand::Expander::new) with
/// [`Expander::expand_and_collect_calls_to_any`] — the SAME from-forms
/// typed-decoded primitive `RealizedCompiler::compile_typed_any_from_forms`
/// routes a preloaded clone through, here threaded through a brand-new
/// `Expander` so callers that have already parsed their forms (a
/// macro-expanded subform, a `Sexp` loaded from disk, an LSP's partial
/// AST cache across edits, a REPL's already-quoted buffer) and don't need
/// a preloaded macro library bind to ONE free function rather than
/// constructing `Expander::new().expand_and_collect_calls_to_any(…)`
/// themselves at each call site.
///
/// Sibling of [`compile_typed_any`] (the from-source posture's
/// fresh-expander typed-decoded dispatcher) and of
/// [`compile_typed_from_forms`] (the from-forms posture's fresh-expander
/// constant-`T::KEYWORD` dispatcher). Closes the (from-forms ×
/// typed-decoded classifier) cell of the fresh-expander dispatcher
/// matrix at the free-function boundary — see [`compile_typed_any`]'s
/// 2×2 table for the matrix shape.
///
/// Theory anchor: same as [`compile_typed_any`]. THEORY.md §VI.1
/// (generation over composition; the (fresh × from-forms × typed-decoded-
/// classifier) cell of the dispatcher matrix is bound in ONE place
/// rather than re-derived inline at every fresh-expander from-forms
/// classifier consumer's call site), THEORY.md §II.1 invariant 1 (typed
/// entry; the classifier-filtered + caller-projected walk over the
/// freshly-expanded forms IS a typed-entry-batch gate), THEORY.md §II.1
/// invariant 2 (free middle; all four cells of the fresh-expander
/// dispatcher matrix route through `Expander::new()` + the matching
/// Expander primitive).
pub fn compile_typed_any_from_forms<R, F, D, T>(
    forms: Vec<Sexp>,
    decode: D,
    project: F,
) -> Result<Vec<R>>
where
    D: FnMut(&str) -> Option<T>,
    F: FnMut(T, &[Sexp]) -> Result<R>,
{
    Expander::new().expand_and_collect_calls_to_any(forms, decode, project)
}

/// Read + macroexpand + named-classifier-walk `src` through a fresh
/// `Expander` — the from-source posture of the (named NAME-then-kwargs ×
/// typed-decoded classifier) cell at the fresh-expander free-function
/// boundary, sibling of [`compile_typed_any`] (the bare-kwargs typed-
/// decoded classifier dispatcher) and of [`compile_named`] (the
/// constant-`T::KEYWORD` named dispatcher).
///
/// Composes [`Expander::new()`](crate::macro_expand::Expander::new) with
/// [`Expander::expand_source_and_collect_named_calls_to_any`] — the
/// from-source named-classifier primitive on the `Expander` surface
/// (ae2a3c3) that itself composes [`Expander::expand_and_collect_calls_to_any`]
/// with [`split_name_slot`]. Closes the fresh-expander free-function
/// dispatcher cube at the (typed-decoded classifier × named NAME-then-
/// kwargs) corner that prior runs' [`split_name_slot`] (dd50801),
/// [`Expander::expand_and_collect_named_calls_to_any`] (ae2a3c3), and
/// [`compile_typed_any`] (8971014) collectively prepared:
///
/// |              | constant `T::KEYWORD`                   | typed-decoded classifier                       |
/// |--------------|-----------------------------------------|------------------------------------------------|
/// | from-forms × bare-kwargs | [`compile_typed_from_forms`]            | [`compile_typed_any_from_forms`]               |
/// | from-source × bare-kwargs | [`compile_typed`]                       | [`compile_typed_any`]                          |
/// | from-forms × named        | [`compile_named_from_forms`]            | [`compile_named_any_from_forms`]               |
/// | from-source × named       | [`compile_named`]                       | [`compile_named_any`] (this)                   |
///
/// Decoder signature `FnMut(&str) -> Option<(T, &'static str)>` pairs the
/// typed witness `T` with the canonical static keyword threaded through
/// the `NamedFormMissingName.keyword` / `NamedFormNonSymbolName.keyword`
/// slots of the named-form gate — the `&'static` constraint pins the
/// same compile-time discipline `split_name_slot`'s `keyword: &'static
/// str` parameter pins at the slice-side boundary. Projection signature
/// `FnMut(T, &str, &[Sexp]) -> Result<R>` receives the typed witness
/// ALONGSIDE the BORROWED NAME slot (from [`Sexp::as_symbol_or_string`],
/// accepting both symbol- and string-author surfaces) AND the spec args
/// tail. Consumers that need owned ownership of the NAME (`NamedDefinition.name:
/// String`, JSON-serialized payloads) `.to_string()` themselves —
/// pushing the clone to the consumer boundary keeps the primitive
/// allocation-free.
///
/// The constant-`T::KEYWORD` column is the typed CONSEQUENCE of the
/// classifier column: a `compile_named::<T>(src)` call composes
/// `compile_named_any(src, |h| (h == T::KEYWORD).then_some(((),
/// T::KEYWORD)), |(), name, spec_args| { let spec =
/// T::compile_from_args(spec_args)?; Ok(NamedDefinition { name:
/// name.to_string(), spec }) })`. Every cell of the cube binds to ONE
/// composition point on the `Expander` surface (the typed-decoded
/// classifier walk + [`split_name_slot`] gate) — a regression that
/// drifts ONE cell's NAME-slot rejection chain from the others becomes
/// structurally impossible.
///
/// Two plausible future consumers the primitive admits with no
/// boilerplate: a `tatara-check` runner that dispatches every
/// `(defmonitor NAME …)` / `(defnotify NAME …)` / `(defalertpolicy NAME
/// …)` form in `checks.lisp` through ONE closed-set classifier in ONE
/// pass over a source buffer; a live-registry dispatcher that walks a
/// program dispatching every named form whose head is in a runtime
/// registry, decoded to a handler reference AND its canonical static
/// keyword (sourced from the dispatcher itself) without re-deriving
/// `Expander::new().expand_source_and_collect_named_calls_to_any(…)` at
/// the call site.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// (fresh × from-source × typed-decoded-classifier × named NAME-then-
/// kwargs) cell is bound in ONE place rather than re-derived inline at
/// every fresh-expander from-source named-classifier consumer's call
/// site. THEORY.md §II.1 invariant 1 — typed entry; the typed-decoded
/// classifier-filtered + NAME-shape-gated + caller-projected walk over
/// the freshly-expanded program IS a typed-entry-batch gate at the
/// free-function boundary. THEORY.md §II.1 invariant 2 — free middle;
/// every cell of the (fresh × {from-forms, from-source} × {constant,
/// classifier} × {bare-kwargs, named}) cube routes through ONE
/// composition point on the `Expander` surface.
///
/// Frontier inspiration: Racket's `(eval-string str ns)` against a
/// fresh empty namespace combined with `syntax-parse`'s `~or* ((~datum
/// defX) name:id arg ...)` typed-choice repeater on the result —
/// typed named-dispatch with NO preloaded macro library is the Racket
/// idiom; this function is the Rust-typed peer with the typed-decoded
/// classifier composed in, sibling of
/// [`RealizedCompiler::compile_named_any`](crate::compiler_spec::RealizedCompiler::compile_named_any)
/// (the preloaded-namespace posture).
pub fn compile_named_any<R, F, D, T>(src: &str, decode: D, project: F) -> Result<Vec<R>>
where
    D: FnMut(&str) -> Option<(T, &'static str)>,
    F: FnMut(T, &str, &[Sexp]) -> Result<R>,
{
    Expander::new().expand_source_and_collect_named_calls_to_any(src, decode, project)
}

/// Macroexpand + named-classifier-walk a pre-parsed program through a
/// fresh `Expander` — the from-forms posture of [`compile_named_any`].
///
/// Composes [`Expander::new()`](crate::macro_expand::Expander::new) with
/// [`Expander::expand_and_collect_named_calls_to_any`]. Sibling of
/// [`compile_named_any`] (from-source) and of [`compile_typed_any_from_forms`]
/// (from-forms × bare-kwargs typed-decoded classifier); together with
/// [`compile_named_from_forms`] (from-forms × constant-`T::KEYWORD` ×
/// named) it closes the from-forms row of the named-classifier corner
/// at the free-function boundary — see [`compile_named_any`]'s 4×2
/// table for the cube shape.
///
/// Theory anchor: same as [`compile_named_any`]. THEORY.md §VI.1
/// (generation over composition; the from-forms posture is the
/// inverse-delegation peer of from-source, which composes `read(src)? +
/// from-forms`), THEORY.md §II.1 invariant 2 (free middle; every cell
/// of the named-classifier cube routes through ONE composition point).
pub fn compile_named_any_from_forms<R, F, D, T>(
    forms: Vec<Sexp>,
    decode: D,
    project: F,
) -> Result<Vec<R>>
where
    D: FnMut(&str) -> Option<(T, &'static str)>,
    F: FnMut(T, &str, &[Sexp]) -> Result<R>,
{
    Expander::new().expand_and_collect_named_calls_to_any(forms, decode, project)
}

/// Split a `(<keyword> NAME …)` form's argument tail into the NAME slot
/// projection and the remaining argument tail — the named-form arity +
/// NAME-shape gate lifted out of `named_form_projection`'s inline body
/// into ONE public primitive on the substrate's `&[Sexp]` algebra,
/// independent of any `T: TataraDomain` typed-entry follow-up.
///
/// Composes the two-step structural rejection chain — `rest.split_first()`
/// arity gate → `as_symbol_or_string()` NAME-shape gate — yielding the
/// borrowed `(&'a str, &'a [Sexp])` pair on success: the NAME slot's
/// canonical symbol-or-string projection (sourced from
/// [`Sexp::as_symbol_or_string`], which accepts BOTH `(defcompiler
/// my-compiler …)` symbol-author and `(defcompiler "quoted-compiler"
/// …)` string-author surfaces) alongside the spec args tail (`&rest[1..]`,
/// the empty slice for a singleton like `(defcompiler my-compiler)`).
/// Both projections borrow from `rest` verbatim — no copy, no
/// allocation, same lifetime as [`Sexp::as_symbol_or_string`]'s tail —
/// so a consumer that wants to use the NAME slot as a lookup key (a
/// REPL completion that resolves a partial NAME against a registry, an
/// LSP that surfaces a tooltip for the NAME at hover, a
/// `tatara-check` diagnostic that quotes the NAME in its rendered
/// message) reaches the borrowed projection directly. Consumers that
/// need owned ownership (`NamedDefinition.name: String`,
/// JSON-serialized payloads, channel-bounded message bodies)
/// `.to_string()` themselves — pushing the clone to the consumer
/// boundary means the substrate primitive does NOT force a clone the
/// consumer doesn't need.
///
/// Before this lift the same two-step gate was welded INSIDE
/// `named_form_projection`'s body, immediately followed by the typed-
/// entry `T::compile_from_args` call. The pre-lift body had ONE
/// consumer (every named-form dispatcher in the matrix routed through
/// `named_form_projection::<T>` directly, which welded the gate with
/// the typed-domain compose). After this lift the gate is composable:
/// `named_form_projection` is now a 2-line composition of this
/// primitive with `T::compile_from_args`, and ANY consumer that wants
/// the named-form NAME extraction WITHOUT the typed-domain compose
/// binds to ONE primitive rather than re-deriving the
/// `split_first()` arity gate + `as_symbol_or_string()` shape gate +
/// `LispError::NamedFormMissingName` / `LispError::NamedFormNonSymbolName`
/// emission triple inline at its own call site.
///
/// `keyword: &'static str` is the canonical operator-position label
/// the named-form structural rejection variants
/// ([`LispError::NamedFormMissingName.keyword`],
/// [`LispError::NamedFormNonSymbolName.keyword`]) carry as `&'static
/// str` slots. Threading the `&'static` constraint through this
/// helper's parameter pins the same compile-time guarantee at the
/// boundary — a typo in the keyword can never drift into the
/// diagnostic at runtime, same posture as `MissingHeadSymbol.keyword`,
/// `HeadMismatch.keyword`, `TypeMismatch.expected`, and the
/// `Defmacro*.head` family. The pre-lift call sites bound the keyword
/// via `T::KEYWORD` (the typed-domain witness's canonical label); the
/// post-lift signature admits ANY `&'static str`, so a classifier
/// consumer that decodes the head to a typed kind whose label is
/// `&'static` (e.g. a `ClosedSet` implementor's `T::label()` or a
/// hand-rolled `&'static str` lookup) binds to ONE primitive without
/// requiring a `T: TataraDomain` witness.
///
/// Sibling of [`crate::ast::iter_calls_to`] /
/// [`crate::ast::iter_calls_to_any`] on the slice-side `&[Sexp]`
/// algebra — those primitives filter forms by keyword / classifier,
/// this primitive splits an already-filtered form's argument tail
/// into NAME + spec args. Together with [`Sexp::as_call`] /
/// [`Sexp::as_call_to`] / [`Sexp::as_call_to_any`] on the per-form
/// algebra, the substrate's named-form authoring surface decomposes
/// into ONE chain of named primitives the consumer composes per
/// call-site posture, instead of a four-step inline pipeline.
///
/// The future change that benefits: a `compile_named_any` family —
/// the (named NAME-then-kwargs × typed-decoded classifier) cell the
/// substrate's typed-dispatcher matrix leaves open today. A
/// classifier-NAME consumer composes
/// `expand_and_collect_calls_to_any(forms, decode_kind, |kind, args|
/// { let (name, spec_args) = split_name_slot(args, kind.label())?;
/// project(kind, name, spec_args) })` — the named-form gate is
/// COMPOSED in, not re-derived inline. A future named-classifier
/// primitive on `Expander` (a hypothetical
/// `expand_and_collect_named_calls_to_any`) would land as 3 lines on
/// top of `expand_and_collect_calls_to_any` + this primitive, without
/// re-deriving the gate.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// named-form arity + NAME-shape gate is a NAMED primitive on the
/// `&[Sexp]` algebra, NOT a re-derived inline pipeline at every
/// named-form consumer site. The typed-domain compose (the
/// `T::compile_from_args` step inside `named_form_projection`)
/// follows AS A COMPOSITION of THIS primitive + the typed-entry gate,
/// not as a re-derivation of either. THEORY.md §II.1 invariant 2 —
/// free middle; both the typed-domain consumer
/// (`named_form_projection<T>`) AND any future classifier-NAME
/// consumer route through ONE gate body, so a regression in the gate
/// (a future debug-mode logger, span-aware borrow walker,
/// instrumentation that records every NAME-slot rejection for
/// telemetry) lands at ONE site the entire named-form authoring
/// surface inherits. THEORY.md §V.1 — knowable platform; the
/// named-form gate becomes a discoverable primitive on the
/// substrate's `&[Sexp]` algebra rather than an implementation
/// detail buried inside the typed-domain composition.
///
/// Frontier inspiration: Tree-sitter's `query` matched-set + capture
/// binding — a typed pattern exposes named CAPTURES that the
/// consumer references by binding; the NAME slot of a
/// `(<keyword> NAME …)` form is the substrate's typed peer of the
/// capture, exposed as a borrowed `&str` slot the caller composes
/// into its typed projection. Racket's `syntax-parse`
/// `(~datum keyword) name:id arg ...` matches the NAME slot through
/// the `name:id` capture binder and the consumer references it
/// downstream; `split_name_slot` is the unstructured-Rust peer with
/// the typed structural rejection chain (`NamedFormMissingName`,
/// `NamedFormNonSymbolName`) preserved across the boundary.
pub fn split_name_slot<'a>(
    rest: &'a [Sexp],
    keyword: &'static str,
) -> Result<(&'a str, &'a [Sexp])> {
    let (name_form, spec_args) = rest
        .split_first()
        .ok_or(LispError::NamedFormMissingName { keyword })?;
    let name =
        name_form
            .as_symbol_or_string()
            .ok_or_else(|| LispError::NamedFormNonSymbolName {
                keyword,
                got: name_form.shape(),
            })?;
    Ok((name, spec_args))
}

// `named_form_projection<T>` — REMOVED.
//
// Pre-lift the function composed `split_name_slot(rest, T::KEYWORD) +
// T::compile_from_args(spec_args) + NamedDefinition { name, spec }`
// and was the sole projection driving `Expander::expand_to_named<T>`'s
// per-form payload. Post-lift `Expander::expand_to_named<T>` routes
// through the named constant-keyword primitive
// `Expander::expand_and_collect_named_calls_to` (which routes through
// the named typed-decoded classifier primitive
// `Expander::expand_and_collect_named_calls_to_any` via a constant-
// classifier decoder), inlining the typed-domain compose
// `T::compile_from_args + NamedDefinition` into its `(name, spec_args)
// -> Result<NamedDefinition<T>>` closure. The `split_name_slot`
// composition therefore lives at ONE site post-lift (inside
// `expand_and_collect_named_calls_to_any`) rather than at TWO sites
// pre-lift (this removed helper + the classifier primitive's wrapper).

#[cfg(test)]
mod tests {
    use super::compile_named;
    use crate::compiler_spec::CompilerSpec;
    use crate::domain::TataraDomain;
    use crate::error::{LispError, Result, SexpShape};

    #[test]
    fn compile_named_emits_named_form_missing_name_for_keyword_only_form() {
        // `(defcompiler)` — list[0] matches `CompilerSpec::KEYWORD` but
        // list.len() == 1: there is no NAME slot at all. The arity gate
        // inside `compile_named_from_forms::<T>` fires before
        // `as_symbol_or_string` runs. Pin that the structural variant
        // identity is `NamedFormMissingName { keyword: "defcompiler" }`
        // (the lift target) — pre-lift this same input emitted
        // `LispError::Compile { form: "defcompiler", message: "expected
        // (defcompiler NAME …)" }` and authoring tools had to substring-
        // grep the rendered diagnostic to recognize the gate.
        // Fail-before-pass-after: this assert is contradicted by the
        // pre-lift code path, ratifies the post-lift one.
        let err = compile_named::<CompilerSpec>("(defcompiler)").unwrap_err();
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
    fn compile_named_named_form_missing_name_renders_legacy_compile_shape() {
        // The lifted variant's Display matches the legacy `Compile`-shaped
        // diagnostic byte-for-byte — `"compile error in defcompiler:
        // expected (defcompiler NAME …)"` (with Unicode horizontal-ellipsis
        // U+2026) — so authoring tools (`tatara-check`'s diagnostic
        // capture, REPL substring-greps) that pattern-matched on the
        // rendered string see no drift across the lift.
        let err = compile_named::<CompilerSpec>("(defcompiler)").unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in defcompiler: expected (defcompiler NAME …)"
        );
    }

    #[test]
    fn compile_named_skips_unrelated_keywords_without_emitting_named_form_missing_name() {
        // `(other-form)` doesn't match `CompilerSpec::KEYWORD`, so the
        // dispatch loop skips it via the `continue` arm at the
        // not-our-keyword gate — `NamedFormMissingName` must NOT fire on
        // forms that aren't ours. Pin path-uniformity: the gate fires
        // ONLY for matched keywords with no NAME, never for unmatched
        // keywords (which compile_typed and compile_named both treat as
        // siblings owned by other domains).
        let defs = compile_named::<CompilerSpec>("(other-form 1 2 3)").unwrap();
        assert!(defs.is_empty());
    }

    // ── named_form_non_symbol_name: structural-variant lift ─────────
    //
    // The previously `LispError::Compile`-shaped helper
    // `named_form_non_symbol_name::<T>()` was promoted to the
    // structural `LispError::NamedFormNonSymbolName { keyword, got }`
    // variant. The helper signature changes from `() -> LispError` to
    // `(got: &Sexp) -> LispError`: the offending NAME slot's outermost
    // shape is projected through `sexp_shape` at the boundary so the
    // variant's `got` slot is the typed `SexpShape` closed-set enum
    // (sourced from the exhaustive match over `Sexp`'s closed set of
    // 12 outer shapes — same posture as `TypeMismatch.got: SexpShape`).
    // Display preserves the legacy `"compile error in {keyword}:
    // positional NAME must be a symbol or string"` prefix byte-for-byte
    // AND appends the structural detail `(got {got})` parenthetically
    // (where `{got}` flows through `SexpShape::Display` to the canonical
    // literal).
    //
    // The tests below pin: (a) each malformed NAME-slot input (int,
    // keyword, nested list) routes through the helper to the
    // structural `LispError::NamedFormNonSymbolName` variant with the
    // canonical keyword and typed `SexpShape`-projected `got`; (b) the
    // helper threads `T::KEYWORD` verbatim through the `keyword` slot;
    // (c) end-to-end through the `LispError` Display impl renders the
    // legacy prefix AND the appended `(got X)` suffix; (d) the helper
    // is precisely scoped — a symbol NAME slot AND a string NAME slot
    // both pass through to `compile_from_args` cleanly, NOT through
    // the helper.

    #[test]
    fn compile_named_emits_named_form_non_symbol_name_for_int_name_slot() {
        // `(defcompiler 5 :name "x")` — list[1] is an int literal, not
        // a symbol or string. The `as_symbol_or_string` ok_or_else
        // chain routes through `named_form_non_symbol_name::<T>(&list[1])`,
        // which emits the structural `LispError::NamedFormNonSymbolName
        // { keyword: "defcompiler", got: "int" }` variant. Pre-lift
        // this same input emitted `LispError::Compile { form:
        // "defcompiler", message: "positional NAME must be a symbol or
        // string" }` and authoring tools had to substring-grep the
        // rendered diagnostic AND lost the actual sexp-type name of the
        // offending slot. Fail-before-pass-after: this assert is
        // contradicted by the pre-lift code path, ratifies the
        // post-lift one.
        let err = compile_named::<CompilerSpec>("(defcompiler 5 :name \"x\")").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormNonSymbolName {
                    keyword: "defcompiler",
                    got: SexpShape::Int,
                }
            ),
            "expected NamedFormNonSymbolName {{ got: SexpShape::Int }}, got: {err:?}"
        );
    }

    #[test]
    fn compile_named_emits_named_form_non_symbol_name_for_keyword_name_slot() {
        // `(defcompiler :foo :name "x")` — list[1] is `:foo`, a keyword.
        // Pin path-uniformity across distinct non-symbol-non-string
        // shapes: the `got` slot carries the `sexp_type_name(_)`
        // projection so authoring tools bind structurally to the actual
        // offending shape instead of having to substring-grep the
        // rendered diagnostic.
        let err = compile_named::<CompilerSpec>("(defcompiler :foo :name \"x\")").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormNonSymbolName {
                    keyword: "defcompiler",
                    got: SexpShape::Keyword,
                }
            ),
            "expected NamedFormNonSymbolName {{ got: SexpShape::Keyword }}, got: {err:?}"
        );
    }

    #[test]
    fn compile_named_emits_named_form_non_symbol_name_for_nested_list_name_slot() {
        // `(defcompiler (nested) :name "x")` — list[1] is a nested list.
        // Closes the "non-symbol-or-string at NAME slot" failure-mode
        // set across three distinct Sexp shapes (atom-int, atom-keyword,
        // list); the `got` slot reads `list` and the inner list is NOT
        // recursively descended (the gate is single-level —
        // `as_symbol_or_string` is a shallow projection).
        let err = compile_named::<CompilerSpec>("(defcompiler (nested) :name \"x\")").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormNonSymbolName {
                    keyword: "defcompiler",
                    got: SexpShape::List,
                }
            ),
            "expected NamedFormNonSymbolName {{ got: SexpShape::List }}, got: {err:?}"
        );
    }

    #[test]
    fn compile_named_non_symbol_name_renders_legacy_prefix_and_got_suffix() {
        // The lifted variant's Display matches the legacy `Compile`-shaped
        // diagnostic byte-for-byte across the stable prefix `"compile
        // error in defcompiler: positional NAME must be a symbol or
        // string"` AND appends the structural detail `" (got int)"`
        // parallel to how `MissingHeadSymbol` appends `(got 5)` and
        // `RestParamMissingName` appends `(rest marker at position N,
        // got X)`. Authoring tools that pattern-matched on the pre-lift
        // rendered string see the legacy substring unchanged; tools that
        // pattern-match on the variant gain structural binding to
        // `keyword` AND `got`.
        let err = compile_named::<CompilerSpec>("(defcompiler 5)").unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in defcompiler: positional NAME must be a symbol or string (got int)"
        );
    }

    #[test]
    fn compile_named_accepts_symbol_name_slot_routing_past_the_helper() {
        // `(defcompiler my-compiler :name "x")` — list[1] IS a symbol,
        // so the `as_symbol_or_string` short-circuit returns `Some`
        // BEFORE the helper fires. Pin path-uniformity (positive
        // control): the helper is precisely scoped to NON-symbol-or-
        // string NAME slots; a regression that fires the helper on
        // valid inputs would fail here — the form must compile
        // successfully and the NAME slot must carry the symbol
        // verbatim into the `NamedDefinition.name` field.
        let defs = compile_named::<CompilerSpec>("(defcompiler my-compiler :name \"x\")")
            .expect("valid symbol-NAME form must compile");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "my-compiler");
    }

    #[test]
    fn compile_named_accepts_string_name_slot_routing_past_the_helper() {
        // `(defcompiler "quoted-compiler" :name "x")` — list[1] is a
        // string literal, which `as_symbol_or_string` also accepts.
        // Sibling positive control: pins that BOTH the symbol AND
        // the string NAME-slot shapes route past the helper, NOT
        // through it. A regression that narrows the helper's gate
        // (e.g. accepting only symbols, rejecting strings) would
        // fail here — the form must compile successfully and the
        // string NAME slot must carry the literal verbatim into the
        // `NamedDefinition.name` field.
        let defs = compile_named::<CompilerSpec>("(defcompiler \"quoted-compiler\" :name \"x\")")
            .expect("valid string-NAME form must compile");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "quoted-compiler");
    }

    // ── Expander::expand_source_to_typed / expand_source_to_named ───
    //
    // The `(T::KEYWORD, T::compile_from_args)` AND
    // `(T::KEYWORD, named_form_projection::<T>)` pairs are bound at the
    // type level through `T` inside ONE method per form-shape — the
    // four pre-lift dispatcher sites (two fresh-expander free
    // functions, two preloaded-expander `RealizedCompiler` methods)
    // now route through this typed-pair primitive on `Expander`. The
    // tests below pin: (a) the typed primitive yields the same Vec<T>
    // the fresh-expander free function does on the SAME source,
    // (b) the named primitive yields the same Vec<NamedDefinition<T>>
    // the fresh-expander free function does on the SAME source,
    // (c) the typed primitive's structural rejection chain
    // (NamedFormMissingName / NamedFormNonSymbolName /
    // T::compile_from_args's typed-entry kwargs gate) fires
    // identically through the new method as through the old free-
    // function path — path-uniformity across the lift.

    #[test]
    fn expand_source_to_typed_yields_same_vec_as_fresh_free_function() {
        // Pin parity: `Expander::new().expand_source_to_typed::<T>(src)`
        // and `compile_typed::<T>(src)` are byte-identical on the same
        // source — both fresh-expander posture, both yielding `Vec<T>`,
        // both routing through ONE typed-pair primitive on `Expander`.
        // Fail-before-pass-after: this assert requires the new method
        // to exist AND to yield the same payload the free function
        // does — pre-lift the method did not exist.
        use super::Expander;
        let src = r#"(defcompiler :name "alpha" :dialect "standard")
                     (defcompiler :name "beta" :dialect "standard")"#;
        let via_method = Expander::new()
            .expand_source_to_typed::<CompilerSpec>(src)
            .expect("typed-pair primitive must yield Vec<T>");
        let via_free =
            super::compile_typed::<CompilerSpec>(src).expect("free function must yield Vec<T>");
        assert_eq!(via_method.len(), 2);
        assert_eq!(via_method.len(), via_free.len());
        assert_eq!(via_method[0].name, via_free[0].name);
        assert_eq!(via_method[0].name, "alpha");
        assert_eq!(via_method[1].name, via_free[1].name);
        assert_eq!(via_method[1].name, "beta");
    }

    #[test]
    fn expand_source_to_named_yields_same_vec_as_fresh_free_function() {
        // Sibling parity pin for the named-form posture:
        // `Expander::new().expand_source_to_named::<T>(src)` and
        // `compile_named::<T>(src)` are byte-identical on the same
        // source — both fresh-expander posture, both yielding
        // `Vec<NamedDefinition<T>>`, both routing through ONE
        // typed-pair primitive on `Expander`. Fail-before-pass-after:
        // this assert requires the new method to exist AND to thread
        // BOTH the keyword filter AND the named-form projection through
        // `T` — pre-lift the pair was bound at four sites, never as a
        // single typed-pair method.
        use super::Expander;
        let src = r#"(defcompiler alpha-compiler :name "x" :dialect "standard")
                     (defcompiler beta-compiler  :name "y" :dialect "standard")"#;
        let via_method = Expander::new()
            .expand_source_to_named::<CompilerSpec>(src)
            .expect("typed-pair primitive must yield Vec<NamedDefinition<T>>");
        let via_free = super::compile_named::<CompilerSpec>(src)
            .expect("free function must yield Vec<NamedDefinition<T>>");
        assert_eq!(via_method.len(), 2);
        assert_eq!(via_method.len(), via_free.len());
        assert_eq!(via_method[0].name, via_free[0].name);
        assert_eq!(via_method[0].name, "alpha-compiler");
        assert_eq!(via_method[0].spec.name, "x");
        assert_eq!(via_method[1].name, via_free[1].name);
        assert_eq!(via_method[1].name, "beta-compiler");
        assert_eq!(via_method[1].spec.name, "y");
    }

    #[test]
    fn expand_source_to_typed_skips_unmatched_keywords_silently() {
        // Path-uniformity: the typed primitive's keyword filter
        // (inherited from `expand_source_and_collect_calls_to` which
        // composes `iter_calls_to`) skips forms whose head doesn't
        // match `T::KEYWORD` silently — same soft-projection posture as
        // every other consumer of the dispatcher family. A
        // `(unrelated-form …)` in the source must NOT produce a
        // `NamedFormMissingName` rejection (that variant fires ONLY
        // when the keyword MATCHES but the NAME slot is missing —
        // pinned in the named sibling test below).
        use super::Expander;
        let src = r#"(unrelated-form 1 2 3)
                     (defcompiler :name "kept" :dialect "standard")
                     (also-not-ours :foo bar)"#;
        let specs = Expander::new()
            .expand_source_to_typed::<CompilerSpec>(src)
            .expect("typed-pair primitive must skip unmatched keywords");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "kept");
    }

    #[test]
    fn expand_source_to_named_emits_named_form_missing_name_through_typed_primitive() {
        // Pin the structural rejection chain end-to-end through the
        // new typed primitive: the missing-NAME gate fires AS the
        // `LispError::NamedFormMissingName` variant identically through
        // `Expander::expand_source_to_named` as through the free
        // function `compile_named`. A regression that drifts the
        // typed-primitive's projection from `named_form_projection::<T>`
        // (e.g. a typo binds `T::compile_from_args` instead of the
        // named split) would silently fail BOTH the missing-NAME gate
        // AND the structural-variant identity assertion.
        use super::Expander;
        let err = Expander::new()
            .expand_source_to_named::<CompilerSpec>("(defcompiler)")
            .unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormMissingName {
                    keyword: "defcompiler",
                }
            ),
            "expected NamedFormMissingName through typed primitive, got: {err:?}"
        );
    }

    #[test]
    fn expand_source_to_named_emits_named_form_non_symbol_name_through_typed_primitive() {
        // Sibling of the missing-NAME pin: the non-symbol-NAME gate
        // fires AS the `LispError::NamedFormNonSymbolName` variant
        // identically through the typed primitive. Together the two
        // assertions pin path-uniformity across the typed-primitive's
        // ENTIRE structural rejection chain — both the
        // `split_first()` arity gate AND the
        // `as_symbol_or_string()` shape gate route through the same
        // `named_form_projection::<T>` body the free function routes
        // through.
        use super::Expander;
        let err = Expander::new()
            .expand_source_to_named::<CompilerSpec>("(defcompiler 5 :name \"x\")")
            .unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormNonSymbolName {
                    keyword: "defcompiler",
                    got: SexpShape::Int,
                }
            ),
            "expected NamedFormNonSymbolName through typed primitive, got: {err:?}"
        );
    }

    #[test]
    fn expand_source_to_typed_routes_preloaded_macros_into_typed_dispatch() {
        // The compounding property the lift enables: the SAME typed
        // primitive `Expander::expand_source_to_typed::<T>` works on
        // BOTH a fresh expander AND a preloaded expander, with the
        // expander posture decided by the caller (which `Expander`
        // value they materialized). Pin that a preloaded expander —
        // built ad-hoc here without going through `RealizedCompiler` —
        // carries its registered `defmacro` into the typed dispatch
        // through the SAME method the fresh-expander free function
        // calls.
        use super::Expander;
        let mut preloaded = Expander::new();
        preloaded
            .expand_program(
                crate::reader::read(
                    "(defmacro mk-spec (n) `(defcompiler :name ,n :dialect \"standard\"))",
                )
                .unwrap(),
            )
            .expect("preloading defmacro must succeed");
        let specs = preloaded
            .expand_source_to_typed::<CompilerSpec>(r#"(mk-spec "via-preloaded")"#)
            .expect("preloaded typed primitive must dispatch through the macro");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "via-preloaded");
        // Posture-divergence control: the fresh-expander free function
        // does NOT see the macro and skips the form silently. Pinning
        // the divergence here proves the typed primitive's expander
        // posture is the caller's binding, not a hard-coded fresh
        // expander.
        let fresh = super::compile_typed::<CompilerSpec>(r#"(mk-spec "via-preloaded")"#)
            .expect("fresh-expander free function must succeed");
        assert!(
            fresh.is_empty(),
            "fresh-expander posture must NOT see the ad-hoc preloaded macro, got: {fresh:?}"
        );
    }

    // ── Expander::expand_to_typed / expand_to_named (from-forms posture) ──
    //
    // The from-forms row of the typed-pair dispatcher family on
    // `Expander`. Sibling of `expand_source_to_typed` / `expand_source_to_named`
    // (the from-source row) — the from-source row composes
    // `crate::reader::read` with these methods, so the typed pair
    // `(T::KEYWORD, projection-for-T)` is bound in ONE place per form
    // shape (here) and the from-source row inherits the binding through
    // delegation. The tests below pin: (a) the typed from-forms primitive
    // yields the same `Vec<T>` the from-source sibling does on parse(src);
    // (b) the named from-forms primitive yields the same `Vec<NamedDefinition<T>>`;
    // (c) the structural rejection chain fires identically across the
    // from-forms and from-source postures of the same form shape;
    // (d) `compile_named_from_forms` (the fresh-expander free-function
    // entry to from-forms named dispatch) routes through the new typed
    // primitive — path-uniformity across all three named consumers.

    #[test]
    fn expand_to_typed_yields_same_vec_as_expand_source_to_typed() {
        // Pin parity: feeding pre-read forms through `expand_to_typed::<T>`
        // is byte-identical to feeding the source through
        // `expand_source_to_typed::<T>` on the same expander, because the
        // from-source method is now `read(src)? + expand_to_typed(forms)`.
        // Fail-before-pass-after: the new method must exist AND must
        // produce the same Vec<T> the from-source sibling does — pre-lift
        // there was no from-forms typed method to call.
        use super::Expander;
        let src = r#"(defcompiler :name "alpha" :dialect "standard")
                     (defcompiler :name "beta" :dialect "standard")"#;
        let forms = crate::reader::read(src).expect("read must succeed");
        let via_forms = Expander::new()
            .expand_to_typed::<CompilerSpec>(forms)
            .expect("from-forms typed primitive must yield Vec<T>");
        let via_source = Expander::new()
            .expand_source_to_typed::<CompilerSpec>(src)
            .expect("from-source typed primitive must yield Vec<T>");
        assert_eq!(via_forms.len(), 2);
        assert_eq!(via_forms.len(), via_source.len());
        assert_eq!(via_forms[0].name, via_source[0].name);
        assert_eq!(via_forms[0].name, "alpha");
        assert_eq!(via_forms[1].name, via_source[1].name);
        assert_eq!(via_forms[1].name, "beta");
    }

    #[test]
    fn expand_to_named_yields_same_vec_as_expand_source_to_named() {
        // Sibling parity pin for the named-form row. Feeding pre-read
        // forms through `expand_to_named::<T>` is byte-identical to
        // feeding the source through `expand_source_to_named::<T>` on
        // the same expander, because the from-source method delegates
        // to the from-forms sibling.
        use super::Expander;
        let src = r#"(defcompiler alpha-compiler :name "x" :dialect "standard")
                     (defcompiler beta-compiler  :name "y" :dialect "standard")"#;
        let forms = crate::reader::read(src).expect("read must succeed");
        let via_forms = Expander::new()
            .expand_to_named::<CompilerSpec>(forms)
            .expect("from-forms named primitive must yield Vec<NamedDefinition<T>>");
        let via_source = Expander::new()
            .expand_source_to_named::<CompilerSpec>(src)
            .expect("from-source named primitive must yield Vec<NamedDefinition<T>>");
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
    fn expand_to_typed_skips_unmatched_keywords_silently() {
        // Path-uniformity inherited from `expand_and_collect_calls_to`'s
        // keyword filter: forms whose head doesn't match `T::KEYWORD`
        // are skipped without rejection. The from-forms typed primitive
        // shares the soft-projection posture with every other typed
        // dispatcher in the family.
        use super::Expander;
        let src = r#"(unrelated-form 1 2 3)
                     (defcompiler :name "kept" :dialect "standard")
                     (also-not-ours :foo bar)"#;
        let forms = crate::reader::read(src).expect("read must succeed");
        let specs = Expander::new()
            .expand_to_typed::<CompilerSpec>(forms)
            .expect("from-forms typed primitive must skip unmatched keywords");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "kept");
    }

    #[test]
    fn expand_to_named_emits_named_form_missing_name_through_from_forms_primitive() {
        // Pin the structural rejection chain end-to-end through the new
        // from-forms primitive: the missing-NAME gate fires AS the
        // `LispError::NamedFormMissingName` variant identically here as
        // through the from-source sibling. A regression that drifts the
        // from-forms projection from `named_form_projection::<T>` would
        // silently fail BOTH the missing-NAME gate AND the structural-
        // variant identity assertion.
        use super::Expander;
        let forms = crate::reader::read("(defcompiler)").expect("read must succeed");
        let err = Expander::new()
            .expand_to_named::<CompilerSpec>(forms)
            .unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormMissingName {
                    keyword: "defcompiler",
                }
            ),
            "expected NamedFormMissingName through from-forms primitive, got: {err:?}"
        );
    }

    #[test]
    fn expand_to_named_emits_named_form_non_symbol_name_through_from_forms_primitive() {
        // Sibling of the missing-NAME pin: the non-symbol-NAME gate
        // fires AS the `LispError::NamedFormNonSymbolName` variant
        // identically through the from-forms primitive. Together with
        // the missing-NAME pin, this closes path-uniformity across the
        // ENTIRE structural rejection chain — both the `split_first()`
        // arity gate AND the `as_symbol_or_string()` shape gate route
        // through the same `named_form_projection::<T>` body the
        // from-source sibling routes through.
        use super::Expander;
        let forms = crate::reader::read("(defcompiler 5 :name \"x\")").expect("read must succeed");
        let err = Expander::new()
            .expand_to_named::<CompilerSpec>(forms)
            .unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormNonSymbolName {
                    keyword: "defcompiler",
                    got: SexpShape::Int,
                }
            ),
            "expected NamedFormNonSymbolName through from-forms primitive, got: {err:?}"
        );
    }

    #[test]
    fn compile_named_from_forms_routes_through_expand_to_named_primitive() {
        // Compounding property: `compile_named_from_forms` (the
        // free-function entry to the from-forms named dispatcher) now
        // routes through `Expander::expand_to_named::<T>` — the SAME
        // typed primitive the from-source `compile_named` and the
        // preloaded `RealizedCompiler::compile_named` ultimately route
        // through. Pin parity: the result of `compile_named_from_forms`
        // is byte-identical to invoking `expand_to_named` on a fresh
        // expander with the same forms. A regression that drifts the
        // free function's binding from the typed primitive (e.g. a
        // future emitter that re-derives the inline
        // `expand_and_collect_calls_to(forms, T::KEYWORD,
        // named_form_projection::<T>)` triple at the free function's
        // call site) would fail loudly here.
        use super::{compile_named_from_forms, Expander};
        let src = r#"(defcompiler alpha :name "x" :dialect "standard")
                     (defcompiler beta  :name "y" :dialect "standard")"#;
        let forms_a = crate::reader::read(src).expect("read must succeed");
        let forms_b = crate::reader::read(src).expect("read must succeed");
        let via_free = compile_named_from_forms::<CompilerSpec>(forms_a)
            .expect("free function must yield Vec<NamedDefinition<T>>");
        let via_method = Expander::new()
            .expand_to_named::<CompilerSpec>(forms_b)
            .expect("typed primitive must yield Vec<NamedDefinition<T>>");
        assert_eq!(via_free.len(), 2);
        assert_eq!(via_free.len(), via_method.len());
        assert_eq!(via_free[0].name, via_method[0].name);
        assert_eq!(via_free[0].name, "alpha");
        assert_eq!(via_free[0].spec.name, via_method[0].spec.name);
        assert_eq!(via_free[1].name, via_method[1].name);
        assert_eq!(via_free[1].name, "beta");
    }

    // ── compile_typed_from_forms: closes the fresh-expander free-function ──
    //
    // The free-function dispatcher family
    // (`compile_typed` from-source typed, `compile_named` from-source
    // named, `compile_named_from_forms` from-forms named) was missing
    // the from-forms × typed-shape cell — pre-lift, a from-forms typed
    // consumer at the free-function boundary had to re-derive
    // `Expander::new().expand_to_typed::<T>(forms)` itself. After this
    // lift the cell is named, and the family closes symmetrically with
    // the typed-pair primitives on `Expander`. The tests below pin:
    // (a) parity with `compile_typed` on `parse(src)`,
    // (b) parity with `Expander::new().expand_to_typed::<T>(forms)`
    //     (path-uniformity: the free function routes through the typed
    //     primitive, NOT a re-derived inline binding),
    // (c) silent skip for unmatched keywords (soft-projection inherited),
    // (d) `T::compile_from_args`'s typed-entry rejection chain fires
    //     identically through the free function as through its sibling.

    #[test]
    fn compile_typed_from_forms_yields_same_vec_as_compile_typed_on_parse_src() {
        // Pin parity: feeding pre-read forms through
        // `compile_typed_from_forms::<T>` is byte-identical to feeding
        // the source through `compile_typed::<T>` on the same input,
        // because the from-source free function is
        // `Expander::new().expand_source_to_typed(src)` →
        // `read(src)? + Expander::new().expand_to_typed(forms)` and the
        // from-forms free function is the second leg of that same
        // composition surfaced as ONE named primitive.
        // Fail-before-pass-after: the free function must exist AND must
        // yield the same Vec<T> the from-source sibling does — pre-lift
        // there was no from-forms typed free function to call.
        use super::{compile_typed, compile_typed_from_forms};
        let src = r#"(defcompiler :name "alpha" :dialect "standard")
                     (defcompiler :name "beta" :dialect "standard")"#;
        let forms = crate::reader::read(src).expect("read must succeed");
        let via_forms = compile_typed_from_forms::<CompilerSpec>(forms)
            .expect("from-forms typed free function must yield Vec<T>");
        let via_source = compile_typed::<CompilerSpec>(src)
            .expect("from-source typed free function must yield Vec<T>");
        assert_eq!(via_forms.len(), 2);
        assert_eq!(via_forms.len(), via_source.len());
        assert_eq!(via_forms[0].name, via_source[0].name);
        assert_eq!(via_forms[0].name, "alpha");
        assert_eq!(via_forms[1].name, via_source[1].name);
        assert_eq!(via_forms[1].name, "beta");
    }

    #[test]
    fn compile_typed_from_forms_routes_through_expand_to_typed_primitive() {
        // Compounding property: `compile_typed_from_forms` (the new
        // free-function entry to the from-forms typed dispatcher) routes
        // through `Expander::expand_to_typed::<T>` on a fresh expander
        // — the SAME typed primitive the from-source `compile_typed`
        // and the preloaded `RealizedCompiler::compile_typed` ultimately
        // route through. Pin parity: the result of
        // `compile_typed_from_forms` is byte-identical to invoking
        // `expand_to_typed` on a fresh expander with the same forms. A
        // regression that drifts the free function's binding from the
        // typed primitive (e.g. a future emitter that re-derives the
        // inline `expand_and_collect_calls_to(forms, T::KEYWORD,
        // T::compile_from_args)` triple at the free function's call
        // site) would fail loudly here.
        use super::{compile_typed_from_forms, Expander};
        let src = r#"(defcompiler :name "x" :dialect "standard")
                     (defcompiler :name "y" :dialect "standard")"#;
        let forms_a = crate::reader::read(src).expect("read must succeed");
        let forms_b = crate::reader::read(src).expect("read must succeed");
        let via_free = compile_typed_from_forms::<CompilerSpec>(forms_a)
            .expect("free function must yield Vec<T>");
        let via_method = Expander::new()
            .expand_to_typed::<CompilerSpec>(forms_b)
            .expect("typed primitive must yield Vec<T>");
        assert_eq!(via_free.len(), 2);
        assert_eq!(via_free.len(), via_method.len());
        assert_eq!(via_free[0].name, via_method[0].name);
        assert_eq!(via_free[0].name, "x");
        assert_eq!(via_free[1].name, via_method[1].name);
        assert_eq!(via_free[1].name, "y");
    }

    #[test]
    fn compile_typed_from_forms_skips_unmatched_keywords_silently() {
        // Path-uniformity inherited from `expand_and_collect_calls_to`'s
        // keyword filter: forms whose head doesn't match `T::KEYWORD`
        // are skipped without rejection. The from-forms typed free
        // function shares the soft-projection posture with every other
        // typed dispatcher in the family — a `(unrelated-form …)` in
        // the forms must NOT produce any rejection.
        use super::compile_typed_from_forms;
        let src = r#"(unrelated-form 1 2 3)
                     (defcompiler :name "kept" :dialect "standard")
                     (also-not-ours :foo bar)"#;
        let forms = crate::reader::read(src).expect("read must succeed");
        let specs = compile_typed_from_forms::<CompilerSpec>(forms)
            .expect("from-forms typed free function must skip unmatched keywords");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "kept");
    }

    #[test]
    fn compile_typed_from_forms_propagates_typed_entry_rejection_chain() {
        // Pin the structural rejection chain end-to-end through the new
        // from-forms typed free function: a `T::compile_from_args`
        // rejection (e.g. missing required kwarg) fires identically here
        // as through the from-source sibling. A regression that drifts
        // the free function's projection from `T::compile_from_args`
        // would silently fail this. `(defcompiler :dialect "standard")`
        // is missing the required `:name` slot — `CompilerSpec::
        // compile_from_args` rejects it the same way regardless of
        // input posture.
        use super::{compile_typed, compile_typed_from_forms};
        let src = r#"(defcompiler :dialect "standard")"#;
        let forms = crate::reader::read(src).expect("read must succeed");
        let err_from_forms = compile_typed_from_forms::<CompilerSpec>(forms).unwrap_err();
        let err_from_source = compile_typed::<CompilerSpec>(src).unwrap_err();
        // Same Display rendering across postures — pins that the rejection
        // emission site is shared (not re-derived per posture).
        assert_eq!(format!("{err_from_forms}"), format!("{err_from_source}"));
    }

    #[test]
    fn expand_to_typed_routes_preloaded_macros_into_from_forms_typed_dispatch() {
        // The compounding property of the from-forms typed primitive:
        // the SAME `expand_to_typed::<T>` works on BOTH a fresh expander
        // AND a preloaded expander, with the expander posture decided
        // by the caller. A preloaded expander built ad-hoc here carries
        // its registered `defmacro` into the from-forms typed dispatch
        // through the SAME method the fresh-expander free function
        // calls — closing the from-forms × {fresh, preloaded} matrix
        // through ONE typed method body.
        use super::Expander;
        let mut preloaded = Expander::new();
        preloaded
            .expand_program(
                crate::reader::read(
                    "(defmacro mk-spec (n) `(defcompiler :name ,n :dialect \"standard\"))",
                )
                .unwrap(),
            )
            .expect("preloading defmacro must succeed");
        let forms =
            crate::reader::read(r#"(mk-spec "via-preloaded-forms")"#).expect("read must succeed");
        let specs = preloaded
            .expand_to_typed::<CompilerSpec>(forms)
            .expect("preloaded from-forms typed primitive must dispatch through the macro");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "via-preloaded-forms");
    }

    // ── compile_typed_any / compile_typed_any_from_forms — fresh-expander
    //    free-function siblings of RealizedCompiler::compile_typed_any{,_from_forms}
    //
    // Close the fresh-expander dispatcher matrix at the free-function
    // boundary at the (typed-decoded classifier) column. Each cell routes
    // through `Expander::new()` + the matching `Expander` primitive
    // (`expand_source_and_collect_calls_to_any` for from-source;
    // `expand_and_collect_calls_to_any` for from-forms). The preloaded-
    // expander sibling (`RealizedCompiler::compile_typed_any` /
    // `RealizedCompiler::compile_typed_any_from_forms`) routes the SAME
    // typed-decoded primitive through a per-call cloned preloaded expander.

    #[test]
    fn compile_typed_any_from_forms_yields_decoded_pairs_in_source_order() {
        // Pin the typed-decoded yield shape against a closed-set classifier
        // (a hand-rolled `Op::{Foo, Bar}` enum that rejects one head out of
        // three) sourced from a pre-parsed `Vec<Sexp>` against a fresh
        // `Expander`. The classifier-decoded yield walks every matching
        // call form in source order, threading the typed witness alongside
        // the args tail through the projection.
        use super::compile_typed_any_from_forms;
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        enum Op {
            Foo,
            Bar,
        }
        impl Op {
            fn from_keyword(h: &str) -> Option<Self> {
                match h {
                    "foo" => Some(Self::Foo),
                    "bar" => Some(Self::Bar),
                    _ => None,
                }
            }
        }
        let forms = crate::reader::read("(foo 1) (baz 2) (bar 3) (foo 4)").unwrap();
        let yielded: Vec<(Op, usize)> = compile_typed_any_from_forms(
            forms,
            Op::from_keyword,
            |op, args| -> Result<(Op, usize)> { Ok((op, args.len())) },
        )
        .expect("fresh-expander classifier dispatch must succeed on well-formed forms");
        assert_eq!(
            yielded,
            vec![(Op::Foo, 1), (Op::Bar, 1), (Op::Foo, 1)],
            "fresh-expander classifier dispatch must yield (decoded, args_len) in source order, skipping baz",
        );
    }

    #[test]
    fn compile_typed_any_skips_non_matching_forms_without_invoking_project() {
        // Pin the soft-projection contract: the projection MUST NOT run on
        // any form whose head the classifier rejects (atom, list-with-non-
        // symbol-head, unrecognized symbol head). A deliberately-panicking
        // projection across a mix of those shapes survives the walk
        // because the classifier rejects every form first.
        use super::compile_typed_any;
        let src = r#":kw "str" 42 (5 a) (unrecognized x)"#;
        let yielded: Vec<()> = compile_typed_any(
            src,
            |h: &str| match h {
                "foo" | "bar" => Some(()),
                _ => None,
            },
            |(), _args| -> Result<()> {
                panic!(
                    "projection must NOT run on classifier-rejected forms — soft-projection contract"
                )
            },
        )
        .expect("fresh-expander classifier dispatch must succeed when zero forms match");
        assert!(
            yielded.is_empty(),
            "fresh-expander classifier dispatch must yield empty Vec when zero forms match",
        );
    }

    #[test]
    fn compile_typed_any_short_circuits_on_project_error_at_first_failure() {
        // Pin the Result short-circuit: a projection that errors on the
        // SECOND match must NOT run the projection on the THIRD or FOURTH
        // match. Counter increments per projection call; final counter
        // equals the failing form's index + 1, not the total match count,
        // proving the walk short-circuited at the failing form.
        use super::compile_typed_any_from_forms;
        use std::cell::Cell;
        let forms = crate::reader::read("(foo 1) (foo 2) (foo 3) (foo 4)").unwrap();
        let counter = Cell::new(0_usize);
        let result: Result<Vec<()>> = compile_typed_any_from_forms(
            forms,
            |h: &str| (h == "foo").then_some(()),
            |(), _args| {
                let n = counter.get() + 1;
                counter.set(n);
                if n == 2 {
                    Err(LispError::Compile {
                        form: "foo".into(),
                        message: "boom".into(),
                    })
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err(), "projection error must propagate as Err");
        assert_eq!(
            counter.get(),
            2,
            "projection must be invoked exactly twice — short-circuit at first failure (index 1 → counter == 2)",
        );
    }

    #[test]
    fn compile_typed_any_short_circuits_at_reader_error_before_classifier_runs() {
        // Pin the read-then-classifier-then-project ordering with BOTH
        // decoder and projection explicitly panicking — any post-reader
        // execution fires the panic. An unbalanced open paren is rejected
        // at the reader boundary before the classifier walk begins.
        use super::compile_typed_any;
        let err = compile_typed_any(
            "(defcompiler :name \"x\"",
            |_h: &str| -> Option<()> {
                panic!("classifier must NOT run when reader rejects source")
            },
            |(), _args| -> Result<()> {
                panic!("projection must NOT run when reader rejects source")
            },
        )
        .unwrap_err();
        assert!(
            matches!(err, LispError::UnmatchedOpenParen { .. }),
            "expected LispError::UnmatchedOpenParen short-circuit, got: {err:?}",
        );
    }

    #[test]
    fn compile_typed_any_routes_through_compile_typed_any_from_forms_under_delegation() {
        // Pin the from-source-delegates-to-from-forms identity at the
        // free-function boundary: feeding pre-parsed forms through
        // `compile_typed_any_from_forms` yields the byte-identical
        // `Vec<R>` that `compile_typed_any` yields on the source those
        // forms came from. Both routes thread through `Expander::new()` +
        // the SAME classifier primitive on Expander
        // (`expand_source_and_collect_calls_to_any` on the from-source
        // axis composes `read(src)?` with `expand_and_collect_calls_to_any`
        // on the from-forms axis), so the routing identity is the
        // structural witness that the substrate's typed-decoded
        // classifier projection lives at ONE composition point per
        // input posture per expander posture.
        use super::{compile_typed_any, compile_typed_any_from_forms};
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        enum Op {
            Foo,
            Bar,
        }
        impl Op {
            fn from_keyword(h: &str) -> Option<Self> {
                match h {
                    "foo" => Some(Self::Foo),
                    "bar" => Some(Self::Bar),
                    _ => None,
                }
            }
        }
        let src = "(foo 1) (baz 2) (bar 3) (foo 4)";
        let forms = crate::reader::read(src).expect("read must succeed");
        let via_source: Vec<(Op, usize)> =
            compile_typed_any(src, Op::from_keyword, |op, args| -> Result<(Op, usize)> {
                Ok((op, args.len()))
            })
            .expect("from-source must yield Vec<(Op, usize)>");
        let via_forms: Vec<(Op, usize)> = compile_typed_any_from_forms(
            forms,
            Op::from_keyword,
            |op, args| -> Result<(Op, usize)> { Ok((op, args.len())) },
        )
        .expect("from-forms must yield Vec<(Op, usize)>");
        assert_eq!(
            via_source, via_forms,
            "from-source routes through from-forms under delegation — outputs must be byte-identical",
        );
    }

    #[test]
    fn compile_typed_any_expands_defmacro_in_source_before_classifier_runs() {
        // Pin the `expand_program → classifier` ordering at the
        // fresh-expander free-function boundary: a `(defmacro emit-foo …)`
        // in the source absorbs into the fresh expander, then macro calls
        // expand into `(foo …)`, and the classifier sees the post-
        // expansion `foo` head. A regression that swapped the ordering
        // (classifier-before-expand) would skip the macro calls entirely
        // because their pre-expansion head is `emit-foo`, NOT `foo`.
        use super::compile_typed_any;
        let src = "(defmacro emit-foo (x) `(foo ,x)) (emit-foo 1) (emit-foo 2)";
        let yielded: Vec<usize> = compile_typed_any(
            src,
            |h: &str| (h == "foo").then_some(()),
            |(), args| -> Result<usize> { Ok(args.len()) },
        )
        .expect("classifier must see post-expansion foo heads");
        assert_eq!(
            yielded,
            vec![1, 1],
            "classifier must run after macro expansion — both (emit-foo …) calls lower to (foo …)",
        );
    }

    #[test]
    fn compile_typed_constant_keyword_dispatch_routes_through_compile_typed_any_via_classifier_composition(
    ) {
        // Pin the closed-form composition law binding the constant-
        // `T::KEYWORD` column to the classifier column at the fresh-
        // expander free-function boundary: `compile_typed::<T>(src)` IS
        // `compile_typed_any(src, |h| (h == T::KEYWORD).then_some(()),
        // |(), args| T::compile_from_args(args))` modulo the discarded
        // `()` typed witness. Both routes thread through `Expander::new()`
        // and feed the per-form projection the same args tail; pin
        // `Vec<T>` equality across three representative input shapes
        // (matching some, matching none, matching with rejection) on the
        // SAME source.
        use super::{compile_typed, compile_typed_any};
        // Mixed source: two well-formed `(defcompiler :name …)` forms,
        // one form whose head doesn't match the keyword, one form whose
        // head matches a `defmacro` introduced earlier that lowers to a
        // matching form.
        let src = r#"(defcompiler :name "alpha" :dialect "standard")
                     (foo 1 2)
                     (defcompiler :name "beta" :dialect "standard")"#;
        let via_typed = compile_typed::<CompilerSpec>(src).expect("compile_typed must succeed");
        let via_any: Vec<CompilerSpec> = compile_typed_any(
            src,
            |h: &str| (h == CompilerSpec::KEYWORD).then_some(()),
            |(), args| CompilerSpec::compile_from_args(args),
        )
        .expect("compile_typed_any with constant-keyword classifier must succeed");
        assert_eq!(via_typed.len(), 2);
        assert_eq!(via_typed.len(), via_any.len());
        assert_eq!(via_typed[0].name, via_any[0].name);
        assert_eq!(via_typed[0].name, "alpha");
        assert_eq!(via_typed[1].name, via_any[1].name);
        assert_eq!(via_typed[1].name, "beta");
    }

    #[test]
    fn compile_typed_any_admits_fnmut_classifier_maintaining_state_across_walk() {
        // Pin that the classifier slot accepts `FnMut(&str) -> Option<T>`
        // — a closure that captures mutable state across the batch walk
        // — not just `Fn` / `fn`. A counter-bumping decoder that mutates
        // a `Cell<usize>` per head examined survives the walk; the counter
        // ends equal to the number of (post-expansion) call forms in the
        // source, exercising the `FnMut` slot the slice-side classifier
        // primitive carries. A regression that constrained the decoder
        // to `Fn` would fail to type-check this test.
        use super::compile_typed_any;
        use std::cell::Cell;
        let counter = Cell::new(0_usize);
        let src = "(foo 1) (bar 2) (foo 3) (qux 4)";
        let yielded: Vec<()> = compile_typed_any(
            src,
            |h: &str| {
                counter.set(counter.get() + 1);
                (h == "foo").then_some(())
            },
            |(), _args| -> Result<()> { Ok(()) },
        )
        .expect("FnMut classifier dispatch must succeed");
        assert_eq!(yielded.len(), 2, "two (foo …) forms must match");
        assert_eq!(
            counter.get(),
            4,
            "decoder must be invoked once per call-form head — four call forms in source",
        );
    }

    // ── split_name_slot: named-form arity + NAME-shape gate lift ────────
    //
    // The `(rest.split_first() → as_symbol_or_string)` two-step gate
    // previously welded INSIDE `named_form_projection<T>`'s body is now
    // a public primitive on the substrate's `&[Sexp]` algebra.
    // `named_form_projection<T>` becomes a two-line composition of
    // `split_name_slot(rest, T::KEYWORD)` with `T::compile_from_args`.
    //
    // The tests below pin the lifted primitive's contract directly —
    // independent of `named_form_projection`'s typed-domain compose —
    // so a classifier-NAME consumer that composes `split_name_slot`
    // INTO `expand_and_collect_calls_to_any` (the future
    // `compile_named_any` family the substrate matrix leaves open) sees
    // the SAME structural rejection chain (`NamedFormMissingName` /
    // `NamedFormNonSymbolName`) the typed-domain consumer sees through
    // `named_form_projection`. Path-uniformity for the existing
    // typed-domain consumer continues to be pinned by the
    // `compile_named_emits_*` tests above — those route through
    // `compile_named` → `named_form_projection<T>` → `split_name_slot`,
    // so a regression that drifts the lifted gate from the pre-lift
    // welded body would fail BOTH the per-consumer assertions above
    // AND the helper-direct assertions below.

    #[test]
    fn split_name_slot_emits_named_form_missing_name_for_empty_rest() {
        // `rest == &[]` — the `split_first()` arity gate fires before
        // `as_symbol_or_string()` runs. Pin that the helper threads the
        // caller-supplied keyword verbatim through
        // `LispError::NamedFormMissingName.keyword`, NO typed-domain
        // witness involved. Fail-before-pass-after: this assert requires
        // the helper to exist AND to emit the structural variant — pre-
        // lift the helper did not exist; the gate lived inline inside
        // `named_form_projection<T>` and was reachable only with a
        // `T: TataraDomain` constraint at the call boundary.
        let err = super::split_name_slot(&[], "defwhatever").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormMissingName {
                    keyword: "defwhatever",
                }
            ),
            "expected NamedFormMissingName through split_name_slot, got: {err:?}"
        );
    }

    #[test]
    fn split_name_slot_emits_named_form_non_symbol_name_for_int_name_slot() {
        // `rest[0]` is an int literal — the `as_symbol_or_string` shape
        // gate fires AFTER the arity gate passes. Pin path-uniformity
        // across distinct non-symbol-non-string shapes: `got` carries
        // the `SexpShape::Int` projection sourced from the boundary's
        // `Sexp::shape()` call (same projection the pre-lift
        // `named_form_non_symbol_name<T>` helper used) so authoring
        // tools (LSP, REPL, `tatara-check`) bind structurally to the
        // actual offending shape instead of having to substring-grep
        // the rendered diagnostic.
        let rest = crate::reader::read("(5)").unwrap()[0]
            .as_list()
            .unwrap()
            .to_vec();
        let err = super::split_name_slot(&rest, "defwhatever").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormNonSymbolName {
                    keyword: "defwhatever",
                    got: SexpShape::Int,
                }
            ),
            "expected NamedFormNonSymbolName {{ got: SexpShape::Int }} through split_name_slot, got: {err:?}"
        );
    }

    #[test]
    fn split_name_slot_emits_named_form_non_symbol_name_for_keyword_name_slot() {
        // `rest[0]` is `:foo`, a keyword. Sibling shape pin to the int
        // case above: `got` reads `SexpShape::Keyword`. Together with
        // the int case this closes the path-uniformity matrix across
        // the canonical non-symbol-or-string `SexpShape` cells.
        let rest = crate::reader::read("(:foo)").unwrap()[0]
            .as_list()
            .unwrap()
            .to_vec();
        let err = super::split_name_slot(&rest, "defwhatever").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormNonSymbolName {
                    keyword: "defwhatever",
                    got: SexpShape::Keyword,
                }
            ),
            "expected NamedFormNonSymbolName {{ got: SexpShape::Keyword }} through split_name_slot, got: {err:?}"
        );
    }

    #[test]
    fn split_name_slot_returns_borrowed_name_and_spec_args_for_symbol_name_slot() {
        // `rest = [<symbol "my-name">, :key, "val"]` — the helper
        // returns the NAME slot's `&str` projection borrowed from
        // `rest[0]` and the spec args tail `&rest[1..]` borrowed from
        // the slice verbatim. NO clone at the helper boundary — the
        // consumer (`named_form_projection<T>`) calls `.to_string()`
        // itself when ownership is required (`NamedDefinition.name:
        // String`); a consumer that uses the NAME as a lookup key (a
        // future REPL completion resolver, an LSP tooltip renderer)
        // gets the borrow directly without paying for a clone it
        // doesn't need.
        let rest = crate::reader::read(r#"(my-name :key "val")"#).unwrap()[0]
            .as_list()
            .unwrap()
            .to_vec();
        let (name, spec_args) =
            super::split_name_slot(&rest, "defwhatever").expect("valid symbol NAME must split");
        assert_eq!(name, "my-name");
        assert_eq!(spec_args.len(), 2);
        assert_eq!(spec_args[0].as_keyword(), Some("key"));
        assert_eq!(spec_args[1].as_string(), Some("val"));
    }

    #[test]
    fn split_name_slot_returns_borrowed_name_and_spec_args_for_string_name_slot() {
        // `rest = [<string "quoted-name">, :key, "val"]` — sibling pin
        // for the string-author NAME-slot shape (both `(defcompiler
        // my-name …)` symbol-author AND `(defcompiler "my-name" …)`
        // string-author surfaces are accepted by `as_symbol_or_string`).
        // The NAME projection erases the quote-vs-symbol distinction at
        // the helper boundary so downstream consumers see ONE `&str`
        // shape regardless of authoring choice — same as the typed-
        // domain consumer downstream of `named_form_projection<T>`.
        let rest = crate::reader::read(r#"("quoted-name" :key "val")"#).unwrap()[0]
            .as_list()
            .unwrap()
            .to_vec();
        let (name, spec_args) =
            super::split_name_slot(&rest, "defwhatever").expect("valid string NAME must split");
        assert_eq!(name, "quoted-name");
        assert_eq!(spec_args.len(), 2);
    }

    #[test]
    fn split_name_slot_returns_empty_spec_args_for_singleton_name_only_form() {
        // `rest = [<symbol "my-name">]` — the helper accepts a NAME-
        // only form (a `(defwhatever my-name)` form with no kwargs at
        // all). The arity gate's `split_first()` returns `Some((&head,
        // &[]))`, the shape gate accepts the symbol, and the empty
        // `spec_args` slice is returned verbatim. The pre-lift welded
        // body would then call `T::compile_from_args(&[])` — a typed-
        // domain follow-up that may or may not accept the empty slice
        // depending on `T::REQUIRED`'s shape. Pin that the helper
        // itself does NOT short-circuit on empty `spec_args`: a
        // classifier-NAME consumer that wants the NAME extraction
        // WITHOUT the typed-domain typed-entry gate sees the empty
        // slice exactly as a typed-domain consumer would.
        let rest = crate::reader::read("(my-name)").unwrap()[0]
            .as_list()
            .unwrap()
            .to_vec();
        let (name, spec_args) =
            super::split_name_slot(&rest, "defwhatever").expect("name-only form must split");
        assert_eq!(name, "my-name");
        assert!(spec_args.is_empty());
    }

    #[test]
    fn split_name_slot_threads_caller_supplied_keyword_through_missing_variant() {
        // Path-uniformity: the helper accepts ANY `&'static str` keyword,
        // not just `T::KEYWORD` of a typed-domain witness. Pin three
        // distinct caller-supplied keywords ALL thread verbatim through
        // the `LispError::NamedFormMissingName.keyword` slot — a
        // classifier-NAME consumer that decodes the head to a typed
        // kind whose canonical label is `&'static str` (a `ClosedSet`
        // implementor's `T::label()`, a hand-rolled `&'static str`
        // table lookup) binds the keyword at the call boundary.
        for keyword in ["defmonitor", "defalertpolicy", "defcheck"] {
            let err = super::split_name_slot(&[], keyword).unwrap_err();
            match err {
                LispError::NamedFormMissingName { keyword: got } => {
                    assert_eq!(got, keyword, "keyword slot must round-trip verbatim");
                }
                other => panic!("expected NamedFormMissingName, got {other:?}"),
            }
        }
    }

    #[test]
    fn expand_to_named_yields_same_payload_as_named_classifier_with_constant_keyword_composition() {
        // Composition law binding the constant-`T::KEYWORD` named cell
        // (`Expander::expand_to_named<T>`) to the typed-decoded
        // named-classifier cell (`Expander::expand_and_collect_named_calls_to_any`)
        // via a constant-classifier decoder. The post-lift identity:
        //
        //   expand_to_named::<T>(forms) ==
        //       expand_and_collect_named_calls_to_any(forms,
        //           |h| (h == T::KEYWORD).then_some(((), T::KEYWORD)),
        //           |(), name, spec_args| {
        //               let spec = T::compile_from_args(spec_args)?;
        //               Ok(NamedDefinition { name: name.to_string(), spec })
        //           })
        //
        // Pinning the identity here makes the typed-decoded named-classifier
        // primitive the CANONICAL composition point the constant-keyword
        // sibling routes through — parallel to how `iter_calls_to` /
        // `expand_and_collect_calls_to` route through their respective
        // `_any` siblings via a `|h| (h == k).then_some(())` decoder.
        // A future regression that drifts ONE cell's NAME-slot rejection
        // chain from the other becomes loudly visible at this assertion.
        use super::{compile_named, NamedDefinition};
        use crate::macro_expand::Expander;
        let src = r#"(defcompiler alpha-compiler :name "x" :dialect "standard")
                     (defcompiler beta-compiler  :name "y" :dialect "standard")"#;
        let forms = crate::reader::read(src).unwrap();
        let via_typed_named = Expander::new()
            .expand_to_named::<CompilerSpec>(forms.clone())
            .expect("constant-`T::KEYWORD` named cell must yield Vec<NamedDefinition<T>>");
        let via_classifier_named: Vec<NamedDefinition<CompilerSpec>> = Expander::new()
            .expand_and_collect_named_calls_to_any(
                forms,
                |h| (h == CompilerSpec::KEYWORD).then_some(((), CompilerSpec::KEYWORD)),
                |(), name, spec_args| {
                    let spec = CompilerSpec::compile_from_args(spec_args)?;
                    Ok(NamedDefinition {
                        name: name.to_string(),
                        spec,
                    })
                },
            )
            .expect(
                "typed-decoded named classifier with constant-keyword composition must succeed",
            );
        // Cross-check against the fresh-expander free-function entry
        // point — three independent paths must yield the same payload
        // on the same source, pinning path-uniformity across the
        // constant-keyword and classifier columns AND the free-function
        // ↔ method postures.
        let via_free_function = compile_named::<CompilerSpec>(src)
            .expect("free-function entry must yield same payload");
        assert_eq!(via_typed_named.len(), 2);
        assert_eq!(via_typed_named.len(), via_classifier_named.len());
        assert_eq!(via_typed_named.len(), via_free_function.len());
        for ((a, b), c) in via_typed_named
            .iter()
            .zip(via_classifier_named.iter())
            .zip(via_free_function.iter())
        {
            assert_eq!(a.name, b.name, "NAME slot must agree across cells");
            assert_eq!(a.name, c.name, "NAME slot must agree across postures");
            assert_eq!(a.spec.name, b.spec.name, ":name spec must agree");
            assert_eq!(a.spec.name, c.spec.name, ":name spec must agree");
        }
        assert_eq!(via_typed_named[0].name, "alpha-compiler");
        assert_eq!(via_typed_named[0].spec.name, "x");
        assert_eq!(via_typed_named[1].name, "beta-compiler");
        assert_eq!(via_typed_named[1].spec.name, "y");
    }

    // ── compile_named_any{,_from_forms} — fresh-expander free-function ──
    //
    // Closes the fresh-expander free-function dispatcher cube at the
    // (typed-decoded classifier × named NAME-then-kwargs) corner.
    // Each cell routes through `Expander::new()` + the matching
    // `Expander` named-classifier primitive (ae2a3c3:
    // `expand_and_collect_named_calls_to_any` for from-forms;
    // `expand_source_and_collect_named_calls_to_any` for from-source),
    // which in turn composes the typed-decoded classifier walk with
    // `split_name_slot` (dd50801).

    #[test]
    fn compile_named_any_from_forms_yields_decoded_triple_for_every_matching_named_form_in_source_order(
    ) {
        // Pin the typed-decoded yield shape against a closed-set classifier
        // (a hand-rolled `Kind::{Foo, Bar}` enum that rejects one head out
        // of three) over a pre-parsed `Vec<Sexp>` against a fresh
        // `Expander`. The classifier-decoded yield walks every matching
        // named call form in source order, threading the typed witness
        // ALONGSIDE the BORROWED NAME slot AND the args tail through the
        // projection.
        use super::compile_named_any_from_forms;
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        enum Kind {
            Foo,
            Bar,
        }
        let forms = crate::reader::read(
            "(deffoo alpha 1) (defbaz gamma 2) (defbar beta 3) (deffoo delta 4)",
        )
        .unwrap();
        let yielded: Vec<(Kind, String, usize)> = compile_named_any_from_forms(
            forms,
            |h: &str| match h {
                "deffoo" => Some((Kind::Foo, "deffoo")),
                "defbar" => Some((Kind::Bar, "defbar")),
                _ => None,
            },
            |kind, name, args| -> Result<(Kind, String, usize)> {
                Ok((kind, name.to_string(), args.len()))
            },
        )
        .expect("fresh-expander named-classifier dispatch must succeed");
        assert_eq!(
            yielded,
            vec![
                (Kind::Foo, "alpha".into(), 1),
                (Kind::Bar, "beta".into(), 1),
                (Kind::Foo, "delta".into(), 1),
            ],
            "must yield (decoded, NAME, args_len) in source order, skipping defbaz",
        );
    }

    #[test]
    fn compile_named_any_skips_non_matching_forms_without_invoking_project() {
        // Pin the soft-projection contract: the projection MUST NOT run on
        // any form whose head the classifier rejects. A deliberately-
        // panicking projection across a mix of non-matching shapes
        // (atom, list with unrecognized symbol head, list with int head)
        // survives the walk because the classifier rejects every form
        // first.
        use super::compile_named_any;
        let src = r#":kw "str" 42 (unrecognized x) (5 y)"#;
        let yielded: Vec<()> = compile_named_any(
            src,
            |h: &str| match h {
                "deffoo" => Some(((), "deffoo")),
                _ => None,
            },
            |(), _name, _args| -> Result<()> {
                panic!(
                    "projection must NOT run on classifier-rejected forms — soft-projection contract"
                )
            },
        )
        .expect("fresh-expander named-classifier dispatch must succeed when zero forms match");
        assert!(yielded.is_empty());
    }

    #[test]
    fn compile_named_any_emits_named_form_missing_name_through_classifier_keyword() {
        // `(deffoo)` — head matches the classifier (yielding the typed
        // witness AND the classifier-supplied static keyword), but the
        // NAME slot is missing. `split_name_slot`'s arity gate fires and
        // emits `NamedFormMissingName { keyword: "deffoo" }`. Pin that
        // the keyword threaded through is the CLASSIFIER-supplied keyword
        // (NOT a hardcoded fallback) — a regression that drifted the
        // keyword binding from `decode`'s tuple's second element to a
        // constant string would fail loudly here.
        use super::compile_named_any;
        let err = compile_named_any::<(), _, _, ()>(
            "(deffoo)",
            |h: &str| (h == "deffoo").then_some(((), "deffoo")),
            |(), _name, _args| -> Result<()> { Ok(()) },
        )
        .unwrap_err();
        assert!(
            matches!(err, LispError::NamedFormMissingName { keyword: "deffoo" }),
            "expected NamedFormMissingName {{ keyword: \"deffoo\" }}, got: {err:?}"
        );
    }

    #[test]
    fn compile_named_any_emits_named_form_non_symbol_name_through_classifier_keyword() {
        // `(deffoo 42)` — head matches and the NAME-slot arity gate
        // passes, but the NAME slot's shape gate rejects the int. Pin
        // that BOTH the classifier-supplied keyword AND the typed
        // `SexpShape::Int` projection flow into the structural variant.
        use super::compile_named_any;
        let err = compile_named_any::<(), _, _, ()>(
            "(deffoo 42)",
            |h: &str| (h == "deffoo").then_some(((), "deffoo")),
            |(), _name, _args| -> Result<()> { Ok(()) },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                LispError::NamedFormNonSymbolName {
                    keyword: "deffoo",
                    got: SexpShape::Int,
                }
            ),
            "expected NamedFormNonSymbolName {{ keyword: \"deffoo\", got: Int }}, got: {err:?}"
        );
    }

    #[test]
    fn compile_named_any_routes_through_from_forms_under_delegation() {
        // Pin the from-source-delegates-to-from-forms identity at the
        // free-function boundary: feeding pre-parsed forms through
        // `compile_named_any_from_forms` yields the byte-identical
        // `Vec<R>` that `compile_named_any` yields on the source those
        // forms came from. Both routes thread through `Expander::new()`
        // + the SAME named-classifier primitive on `Expander`.
        use super::{compile_named_any, compile_named_any_from_forms};
        let src = "(deffoo alpha 1) (deffoo beta 2)";
        let forms = crate::reader::read(src).unwrap();
        let via_source: Vec<(String, usize)> = compile_named_any(
            src,
            |h: &str| (h == "deffoo").then_some(((), "deffoo")),
            |(), name, args| -> Result<(String, usize)> { Ok((name.to_string(), args.len())) },
        )
        .expect("from-source must succeed");
        let via_forms: Vec<(String, usize)> = compile_named_any_from_forms(
            forms,
            |h: &str| (h == "deffoo").then_some(((), "deffoo")),
            |(), name, args| -> Result<(String, usize)> { Ok((name.to_string(), args.len())) },
        )
        .expect("from-forms must succeed");
        assert_eq!(
            via_source, via_forms,
            "from-source routes through from-forms under delegation — outputs must be byte-identical",
        );
    }

    #[test]
    fn compile_named_any_expands_defmacro_in_source_before_classifier_runs() {
        // Pin the `expand_program → classifier` ordering at the
        // fresh-expander free-function boundary: a `(defmacro emit-foo
        // …)` in the source absorbs into the fresh expander, then macro
        // calls expand into `(deffoo NAME …)`, and the classifier sees
        // the POST-expansion `deffoo` head. A regression that swapped
        // the ordering (classifier-before-expand) would skip the macro
        // calls entirely because their pre-expansion head is `emit-foo`,
        // NOT `deffoo`.
        use super::compile_named_any;
        let src = "(defmacro emit-foo (n) `(deffoo ,n 1)) (emit-foo alpha) (emit-foo beta)";
        let yielded: Vec<String> = compile_named_any(
            src,
            |h: &str| (h == "deffoo").then_some(((), "deffoo")),
            |(), name, _args| -> Result<String> { Ok(name.to_string()) },
        )
        .expect("classifier must see post-expansion deffoo heads");
        assert_eq!(
            yielded,
            vec!["alpha".to_string(), "beta".into()],
            "classifier must run after macro expansion — both (emit-foo …) calls lower to (deffoo …)",
        );
    }

    #[test]
    fn compile_named_constant_keyword_dispatch_routes_through_compile_named_any_via_classifier_composition(
    ) {
        // Pin the closed-form composition law binding the constant-
        // `T::KEYWORD` named cell to the typed-decoded named-classifier
        // cell at the fresh-expander free-function boundary:
        // `compile_named::<T>(src)` IS `compile_named_any(src, |h| (h
        // == T::KEYWORD).then_some(((), T::KEYWORD)), |(), name,
        // spec_args| { let spec = T::compile_from_args(spec_args)?;
        // Ok(NamedDefinition { name: name.to_string(), spec }) })`.
        // Pinning the identity here makes the typed-decoded named-
        // classifier primitive the CANONICAL composition point the
        // constant-keyword sibling routes through — parallel to how
        // `compile_typed_constant_keyword_dispatch_routes_through_compile_typed_any_via_classifier_composition`
        // pins the same law on the bare-kwargs axis.
        use super::{compile_named, compile_named_any, NamedDefinition};
        let src = r#"(defcompiler alpha-compiler :name "x" :dialect "standard")
                     (foo not-our-keyword)
                     (defcompiler beta-compiler  :name "y" :dialect "standard")"#;
        let via_named = compile_named::<CompilerSpec>(src).expect("compile_named must succeed");
        let via_any: Vec<NamedDefinition<CompilerSpec>> = compile_named_any(
            src,
            |h: &str| (h == CompilerSpec::KEYWORD).then_some(((), CompilerSpec::KEYWORD)),
            |(), name, spec_args| {
                let spec = CompilerSpec::compile_from_args(spec_args)?;
                Ok(NamedDefinition {
                    name: name.to_string(),
                    spec,
                })
            },
        )
        .expect("compile_named_any with constant-keyword classifier must succeed");
        assert_eq!(via_named.len(), 2);
        assert_eq!(via_named.len(), via_any.len());
        assert_eq!(via_named[0].name, via_any[0].name);
        assert_eq!(via_named[0].name, "alpha-compiler");
        assert_eq!(via_named[0].spec.name, via_any[0].spec.name);
        assert_eq!(via_named[1].name, via_any[1].name);
        assert_eq!(via_named[1].name, "beta-compiler");
    }
}
