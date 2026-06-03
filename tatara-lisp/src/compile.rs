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
use crate::domain::{sexp_shape, TataraDomain};
use crate::error::{LispError, Result};
use crate::macro_expand::Expander;

/// Typed-keyword dispatchers on the `Expander` surface — the
/// `T: TataraDomain`-shaped sibling family of
/// [`Expander::expand_source_and_collect_calls_to`].
///
/// Before this lift the SAME `(src, T::KEYWORD, <projection-for-T>)` triple
/// was bound at FOUR sites against the untyped primitive: the two
/// fresh-expander free functions ([`compile_typed`] / [`compile_named`])
/// AND the two preloaded-expander methods
/// ([`RealizedCompiler::compile_typed`](crate::compiler_spec::RealizedCompiler::compile_typed)
/// / [`RealizedCompiler::compile_named`](crate::compiler_spec::RealizedCompiler::compile_named)).
/// Each pair pinned the SAME `(T::KEYWORD, T::compile_from_args)` triple
/// (typed, two sites) or `(T::KEYWORD, named_form_projection::<T>)` triple
/// (named, two sites) — past the ≥2 PRIME-DIRECTIVE trigger.
///
/// After this lift the `(keyword, projection)` pair is bound at the type
/// level through `T`: [`Expander::expand_source_to_typed::<T>`] composes
/// `(T::KEYWORD, T::compile_from_args)` and
/// [`Expander::expand_source_to_named::<T>`] composes
/// `(T::KEYWORD, named_form_projection::<T>)`. All four dispatchers
/// collapse to ONE method call per posture × form-shape pair, and the
/// pairing of "keyword filter" with "per-form projection" is no longer a
/// two-arg discipline at four sites — a regression that mis-pairs
/// `T::KEYWORD` with `U::compile_from_args` (where `T != U`) at any site
/// is now structurally impossible: the type parameter binds both
/// substitutions together inside ONE method body.
impl Expander {
    /// Read + macroexpand + project every `(T::KEYWORD :k v …)` form in
    /// `src` into a typed `T` — the bare-kwargs sibling of
    /// [`Self::expand_source_to_named`].
    ///
    /// Composes [`Self::expand_source_and_collect_calls_to`] with
    /// `T::KEYWORD` as the keyword filter and `T::compile_from_args` as
    /// the per-form projection. The expander posture (fresh
    /// [`Expander::new()`](crate::macro_expand::Expander::new) for one-shot
    /// typed compilation, preloaded
    /// [`self.preloaded.clone()`](crate::compiler_spec::RealizedCompiler)
    /// for compilation inside a CompilerSpec's macro library) is the
    /// caller's choice — this method binds the typed `(keyword,
    /// projection)` pair through `T` and dispatches on whichever
    /// `Expander` value the caller materialized.
    ///
    /// Sibling of [`Self::expand_source_to_named`] — both methods route
    /// through the SAME [`Self::expand_source_and_collect_calls_to`]
    /// primitive, each binding the per-form projection that fits its
    /// typed entry shape (bare-kwargs `(T::KEYWORD :k v …)` here,
    /// NAME-then-kwargs `(T::KEYWORD NAME :k v …)` there).
    ///
    /// Theory anchor: THEORY.md §VI.1 — generation over composition;
    /// four inline `(T::KEYWORD, T::compile_from_args)` bindings across
    /// the two fresh-expander free functions ([`compile_typed`]) and the
    /// two preloaded-expander methods
    /// ([`RealizedCompiler::compile_typed`](crate::compiler_spec::RealizedCompiler::compile_typed))
    /// — split across this method and its named sibling — is past the
    /// ≥2 trigger once the typed shape `(T::KEYWORD,
    /// T::compile_from_args)` is named. THEORY.md §II.1 invariant 1 —
    /// typed entry; the typed-keyword filter paired with the
    /// typed-entry projection IS the typed-entry-batch gate, and naming
    /// the pair on `Expander` collapses the per-consumer two-arg
    /// discipline to ONE typed method body the substrate's diagnostic
    /// promotions hang off of. THEORY.md §II.1 invariant 2 — free
    /// middle; both expander postures (fresh + preloaded) route through
    /// the SAME typed-pair primitive, so a regression that drifts ONE
    /// posture's `(T::KEYWORD, T::compile_from_args)` pairing from the
    /// other becomes structurally impossible — the type system binds
    /// both substitutions through `T` at the SINGLE composition point.
    ///
    /// Frontier inspiration: Idris's elaborator-reflection
    /// `Elab.checkType : Term -> Elab Type` — the typed-elaboration
    /// pipeline binds the typed-entry `(domain, projection)` pair at
    /// the type level so consumers can't mis-pair them; the substrate's
    /// `expand_source_to_typed` is the Rust-typed peer on the
    /// `Expander` surface, with the `TataraDomain` trait standing in
    /// for Idris's `Elab` monad as the typed-entry witness.
    pub fn expand_source_to_typed<T: TataraDomain>(&mut self, src: &str) -> Result<Vec<T>> {
        self.expand_source_and_collect_calls_to(src, T::KEYWORD, T::compile_from_args)
    }

    /// Read + macroexpand + project every `(T::KEYWORD NAME :k v …)` form
    /// in `src` into a typed [`NamedDefinition<T>`] — the NAME-then-kwargs
    /// sibling of [`Self::expand_source_to_typed`].
    ///
    /// Composes [`Self::expand_source_and_collect_calls_to`] with
    /// `T::KEYWORD` as the keyword filter and [`named_form_projection::<T>`]
    /// as the per-form projection. The named projection splits the
    /// post-keyword tail into `(NAME, spec_args)` via
    /// [`Sexp::as_symbol_or_string`](crate::ast::Sexp::as_symbol_or_string)
    /// (structurally typed rejection through
    /// [`LispError::NamedFormMissingName`] for the missing-NAME slot
    /// and [`LispError::NamedFormNonSymbolName`] for the non-symbol
    /// NAME slot) before running `T::compile_from_args` on the spec
    /// args tail.
    ///
    /// Sibling of [`Self::expand_source_to_typed`] — both methods route
    /// through the SAME [`Self::expand_source_and_collect_calls_to`]
    /// primitive, each binding the per-form projection that fits its
    /// typed entry shape. Together they close the typed-from-source
    /// family on the `Expander` surface: bare-kwargs typed entry +
    /// NAME-then-kwargs named entry, each available to both fresh and
    /// preloaded expander postures through ONE method call.
    ///
    /// Theory anchor: see [`Self::expand_source_to_typed`] — the
    /// named sibling shares the same lift posture, threading the
    /// NAME-then-kwargs projection through `T` instead of the
    /// bare-kwargs one.
    pub fn expand_source_to_named<T: TataraDomain>(
        &mut self,
        src: &str,
    ) -> Result<Vec<NamedDefinition<T>>> {
        self.expand_source_and_collect_calls_to(src, T::KEYWORD, named_form_projection::<T>)
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

/// Promote the previously `LispError::Compile`-shaped helper into the
/// structural `LispError::NamedFormNonSymbolName { keyword, got }`
/// variant. Sibling of `NamedFormMissingName { keyword }` — that variant
/// fires when the form has no NAME slot at all (`(defpoint)`); this
/// helper fires AFTER the arity gate has passed but BEFORE
/// `T::compile_from_args` runs — at the second of two
/// `compile_named_from_forms` rejection points
/// (arity → name-symbol-or-string → compile_from_args). After this lift
/// the entire `compile_named_from_forms::<T>` rejection chain is
/// structurally typed end-to-end — every gate in the named-form
/// authoring surface (`(defpoint NAME …)`, `(defalertpolicy NAME …)`,
/// `(defcompiler NAME …)`) emits a pattern-matchable variant.
///
/// `T` is the typed-domain witness; the helper projects to
/// `T::KEYWORD` (`&'static str`) at the boundary so the variant's
/// `keyword` slot encodes the compile-time guarantee in the type
/// system — a typo in the keyword can never drift into the
/// diagnostic at runtime, same posture as `NamedFormMissingName.
/// keyword`, `NotAListForm.keyword`, `MissingHeadSymbol.keyword`,
/// `HeadMismatch.keyword`, `TypeMismatch.expected`, and the
/// `Defmacro*.head` family. `got: &Sexp` is projected through
/// `sexp_type_name` at the boundary so the variant's `got` slot is
/// also `&'static str` (sourced from `sexp_type_name`'s exhaustive
/// match over `Sexp` — same posture as `TypeMismatch.got`); a future
/// `Sexp` variant gets named in `sexp_type_name` once and every
/// consumer inherits.
///
/// Display preserves the legacy `"compile error in {keyword}:
/// positional NAME must be a symbol or string"` prefix byte-for-byte
/// AND appends the structural detail `(got {got})` parenthetically
/// — same posture as `MissingHeadSymbol`'s `(got {g})` /
/// `(empty list)` and `RestParamMissingName`'s `(rest marker at
/// position {n}, …)`. Authoring tools that pattern-matched on the
/// pre-lift rendered string see the legacy substring unchanged;
/// tools that pattern-match on the variant gain structural binding
/// to `keyword` AND `got`.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition;
/// the helper boundary lands the structural-variant promotion
/// (parallel to how `MissingHeadSymbol` / `HeadMismatch` /
/// `NamedFormMissingName` promoted prior `Compile`-shaped sites
/// into structural variants). THEORY.md §II.1 invariant 1 — typed
/// entry; a NAME slot that isn't a symbol or string is exactly the
/// failure mode the typed-entry gate exists to reject, AND now the
/// failure mode is itself structurally typed (operators / authoring
/// tools can pattern-match on identity, no substring parse
/// required). THEORY.md §V.1 — knowable platform; the structural
/// variant exposes `keyword` / `got` as first-class fields so
/// authoring tools (LSP, REPL, `tatara-check`) bind to the data
/// shape instead of substring-parsing the rendered diagnostic.
fn named_form_non_symbol_name<T: TataraDomain>(got: &Sexp) -> LispError {
    LispError::NamedFormNonSymbolName {
        keyword: T::KEYWORD,
        got: sexp_shape(got),
    }
}

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

/// Same as `compile_named` but operates on already-parsed forms. Useful when
/// the caller has done its own reading (e.g., from a string, a Sexp loaded
/// from disk, a macro-expanded subform).
///
/// Routes the program-level walk through the substrate's composed
/// expand-then-keyword-project primitive
/// [`Expander::expand_and_collect_calls_to`]: a fresh `Expander::new()`
/// expands `forms` and walks every expanded form whose head matches
/// `T::KEYWORD` through the NAME-then-`T::compile_from_args` split in
/// source order; non-matching forms are skipped silently (soft-projection
/// posture inherited from [`iter_calls_to`](crate::ast::iter_calls_to)).
/// Sibling consumer to [`compile_typed`] — both dispatchers route through
/// the SAME `Expander::expand_and_collect_calls_to` primitive, each binding
/// the per-form projection that fits its call site: the bare-kwargs
/// `T::compile_from_args(args)` inside `compile_typed`, and the
/// NAME-then-`T::compile_from_args` split here. The `Result::collect`
/// short-circuits on the first error (mirroring the pre-lift for-loop
/// `?`-then-return semantics), so the structurally-named rejection
/// chain — `NamedFormMissingName` for the missing NAME slot,
/// `NamedFormNonSymbolName` for the non-symbol NAME slot,
/// `T::compile_from_args`'s typed-entry kwargs gate — fires in the
/// same order under the new shape.
pub fn compile_named_from_forms<T: TataraDomain>(
    forms: Vec<Sexp>,
) -> Result<Vec<NamedDefinition<T>>> {
    Expander::new().expand_and_collect_calls_to(forms, T::KEYWORD, named_form_projection::<T>)
}

/// Project a `(<T::KEYWORD> NAME :k v …)` form's argument tail to a typed
/// [`NamedDefinition<T>`] — the per-form NAME-then-`T::compile_from_args`
/// split lifted out of [`compile_named_from_forms`]'s inline closure into
/// ONE named primitive on the substrate's per-form projection algebra.
///
/// Before this lift the same three-step chain — `rest.split_first()` arity
/// gate → `as_symbol_or_string()` NAME shape gate → `T::compile_from_args`
/// typed-entry kwargs gate — lived as an inline closure inside the
/// fresh-expander dispatcher [`compile_named_from_forms`]. After this lift
/// the closure becomes a named `pub(crate) fn`, threading `T:
/// TataraDomain` through its type parameters so EVERY named-form
/// dispatcher (fresh-expander on a free function, preloaded-expander on a
/// [`RealizedCompiler`] method) binds to ONE projection function rather
/// than re-deriving the closure body per posture.
///
/// Sibling of [`compile_typed`]'s per-form projection `T::compile_from_args`
/// — that closure is a single typed-entry kwargs gate, this projection
/// composes the NAME extraction with it. Both projections feed
/// [`Expander::expand_and_collect_calls_to`](crate::macro_expand::Expander::expand_and_collect_calls_to)'s
/// `F: FnMut(&[Sexp]) -> Result<R>` slot; passing a named `fn` (free
/// function item) coerces to `FnMut` cleanly so the call boundary stays
/// identical to the closure-form. The `Result::collect` short-circuit
/// inside `expand_and_collect_calls_to` preserves the pre-lift
/// `?`-then-return semantics: `NamedFormMissingName` for the missing
/// NAME slot, `NamedFormNonSymbolName` for the non-symbol NAME slot, and
/// `T::compile_from_args`'s typed-entry kwargs gate fire in source order.
///
/// `pub(crate)` because [`RealizedCompiler::compile_named`](crate::compiler_spec::RealizedCompiler::compile_named)
/// — the preloaded-expander posture's named-form dispatcher — is the
/// second consumer; both consumers live inside this crate, and the
/// public-facing typed-dispatcher surface is the two posture-specific
/// entry points (`compile_named` for fresh, `RealizedCompiler::compile_named`
/// for preloaded), not this projection.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// inline closure became a named primitive once a second consumer
/// (the preloaded-expander posture) arrived. THEORY.md §V.1 — knowable
/// platform; the named-form NAME-extraction-then-typed-entry sequence is
/// a NAMED primitive on the substrate's per-form projection algebra,
/// not a re-derived closure body at every named-form dispatcher.
/// THEORY.md §II.1 invariant 2 — free middle; both postures (fresh
/// `Expander::new()` and preloaded `RealizedCompiler.preloaded.clone()`)
/// route through the SAME projection, so a regression that drifts ONE
/// posture's NAME-or-spec rejection chain from the other becomes
/// structurally impossible — there is exactly one implementation both
/// postures route through.
pub(crate) fn named_form_projection<T: TataraDomain>(rest: &[Sexp]) -> Result<NamedDefinition<T>> {
    let (name_form, spec_args) = rest.split_first().ok_or(LispError::NamedFormMissingName {
        keyword: T::KEYWORD,
    })?;
    let name = name_form
        .as_symbol_or_string()
        .ok_or_else(|| named_form_non_symbol_name::<T>(name_form))?
        .to_string();
    let spec = T::compile_from_args(spec_args)?;
    Ok(NamedDefinition { name, spec })
}

#[cfg(test)]
mod tests {
    use super::compile_named;
    use crate::compiler_spec::CompilerSpec;
    use crate::error::{LispError, SexpShape};

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
}
