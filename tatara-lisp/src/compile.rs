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
            .ok_or_else(|| LispError::NamedFormNonSymbolName {
                keyword: T::KEYWORD,
                got: sexp_type_name(&list[1]),
            })?
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

    #[test]
    fn compile_named_emits_named_form_non_symbol_name_for_int_name_slot() {
        // `(defcompiler 5 :path "x")` — list[0] matches
        // `CompilerSpec::KEYWORD` and list.len() >= 2 (so the
        // arity gate passes); list[1] is the int `5`, which is
        // not a symbol or string. The non-symbol-name gate fires
        // and binds to the lifted structural variant
        // `NamedFormNonSymbolName { keyword: "defcompiler", got:
        // "int" }`. Pre-lift this same input emitted
        // `LispError::Compile { form: "defcompiler", message:
        // "positional NAME must be a symbol or string" }` and
        // authoring tools had to substring-grep the rendered
        // diagnostic AND lost the actual sexp-type name of the
        // offending slot. Fail-before-pass-after: this assert is
        // contradicted by the pre-lift code path, ratifies the
        // post-lift one.
        let err = compile_named::<CompilerSpec>("(defcompiler 5 :path \"x\")").unwrap_err();
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
        // `(defcompiler :foo :path "x")` — list[1] is `:foo`, a
        // keyword. Pin path-uniformity across distinct non-symbol-
        // non-string shapes: the `got` slot carries the
        // `sexp_type_name(_)` projection so authoring tools bind
        // structurally to the actual offending shape instead of
        // having to substring-grep the rendered diagnostic.
        let err = compile_named::<CompilerSpec>("(defcompiler :foo :path \"x\")").unwrap_err();
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
        // `(defcompiler (nested) :path "x")` — list[1] is a
        // nested list. The non-symbol-name gate fires with
        // `got: "list"`; the inner list is NOT recursively
        // descended (the gate is single-level — `as_symbol_or_string`
        // is a shallow projection).
        let err = compile_named::<CompilerSpec>("(defcompiler (nested) :path \"x\")").unwrap_err();
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
    fn compile_named_emits_named_form_non_symbol_name_for_int_name_slot_renders_full_shape() {
        // The lifted variant's Display matches the legacy `Compile`-shaped
        // diagnostic byte-for-byte across the stable prefix `"compile error
        // in defcompiler: positional NAME must be a symbol or string"`
        // AND appends the structural detail `" (got int)"` parallel to how
        // `MissingHeadSymbol` appends `(got 5)`. Authoring tools that
        // pattern-matched on the pre-lift rendered string see the legacy
        // substring unchanged; tools that pattern-match on the variant
        // gain structural binding to `keyword` AND `got`.
        let err = compile_named::<CompilerSpec>("(defcompiler 5 :path \"x\")").unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in defcompiler: positional NAME must be a symbol or string (got int)"
        );
    }

    #[test]
    fn compile_named_accepts_symbol_name_slot_without_firing_non_symbol_gate() {
        // `(defcompiler good-name :name "good-name")` — list[1] is the
        // symbol `good-name`, which passes the gate via `as_symbol`.
        // The non-symbol-name gate must NOT fire on the happy path.
        // Pin precise gate scope: the gate rejects ONLY non-symbol-
        // non-string NAME slots, never symbol-shaped ones.
        let defs =
            compile_named::<CompilerSpec>("(defcompiler good-name :name \"good-name\")").unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "good-name");
    }

    #[test]
    fn compile_named_accepts_string_name_slot_without_firing_non_symbol_gate() {
        // `(defcompiler "literal-name" …)` — list[1] is the string
        // `"literal-name"`, which passes the gate via `as_string`
        // (the `.or_else(|| self.as_string())` arm of
        // `as_symbol_or_string`). Pin path-uniformity: both
        // symbol AND string NAME slots flow through the same
        // accept path, neither triggers
        // `NamedFormNonSymbolName`.
        let defs =
            compile_named::<CompilerSpec>("(defcompiler \"literal-name\" :name \"literal-name\")")
                .unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "literal-name");
    }
}
