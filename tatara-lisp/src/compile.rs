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
use crate::domain::{sexp_type_name, TataraDomain};
use crate::error::{LispError, Result};
use crate::macro_expand::Expander;
use crate::reader::read;

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
        got: sexp_type_name(got),
    }
}

/// Read + macroexpand + compile every `(T::KEYWORD :k v …)` form into `T`.
pub fn compile_typed<T: TataraDomain>(src: &str) -> Result<Vec<T>> {
    let forms = read(src)?;
    let mut exp = Expander::new();
    let expanded = exp.expand_program(forms)?;
    let mut out = Vec::new();
    for form in &expanded {
        if let Some(list) = form.as_list() {
            if list.first().and_then(|s| s.as_symbol()) == Some(T::KEYWORD) {
                out.push(T::compile_from_args(&list[1..])?);
            }
        }
    }
    Ok(out)
}

/// Read + macroexpand + compile every `(T::KEYWORD NAME :k v …)` form into
/// `NamedDefinition<T>`. The positional `NAME` is captured separately from
/// the `:kw v` arguments that feed `compile_from_args`.
pub fn compile_named<T: TataraDomain>(src: &str) -> Result<Vec<NamedDefinition<T>>> {
    compile_named_from_forms::<T>(read(src)?)
}

/// Same as `compile_named` but operates on already-parsed forms. Useful when
/// the caller has done its own reading (e.g., from a string, a Sexp loaded
/// from disk, a macro-expanded subform).
pub fn compile_named_from_forms<T: TataraDomain>(
    forms: Vec<Sexp>,
) -> Result<Vec<NamedDefinition<T>>> {
    let mut exp = Expander::new();
    let expanded = exp.expand_program(forms)?;
    let mut out = Vec::new();
    for form in &expanded {
        let Some(list) = form.as_list() else { continue };
        if list.first().and_then(|s| s.as_symbol()) != Some(T::KEYWORD) {
            continue;
        }
        if list.len() < 2 {
            return Err(LispError::NamedFormMissingName {
                keyword: T::KEYWORD,
            });
        }
        let name = list[1]
            .as_symbol_or_string()
            .ok_or_else(|| named_form_non_symbol_name::<T>(&list[1]))?
            .to_string();
        let spec = T::compile_from_args(&list[2..])?;
        out.push(NamedDefinition { name, spec });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::compile_named;
    use crate::compiler_spec::CompilerSpec;
    use crate::error::LispError;

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
    // shape is projected through `sexp_type_name` at the boundary so
    // the variant's `got` slot is `&'static str` (sourced from the
    // exhaustive match over `Sexp`'s closed set of 12 type names —
    // same posture as `TypeMismatch.got`). Display preserves the
    // legacy `"compile error in {keyword}: positional NAME must be a
    // symbol or string"` prefix byte-for-byte AND appends the
    // structural detail `(got {got})` parenthetically.
    //
    // The tests below pin: (a) each malformed NAME-slot input (int,
    // keyword, nested list) routes through the helper to the
    // structural `LispError::NamedFormNonSymbolName` variant with the
    // canonical keyword and `sexp_type_name`-projected `got`; (b) the
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
                    got: "int",
                }
            ),
            "expected NamedFormNonSymbolName {{ got: \"int\" }}, got: {err:?}"
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
                    got: "keyword",
                }
            ),
            "expected NamedFormNonSymbolName {{ got: \"keyword\" }}, got: {err:?}"
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
                    got: "list",
                }
            ),
            "expected NamedFormNonSymbolName {{ got: \"list\" }}, got: {err:?}"
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
}
