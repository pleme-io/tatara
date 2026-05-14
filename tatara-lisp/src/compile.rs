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

/// Lift the inline `LispError::Compile { form: T::KEYWORD.to_string(),
/// message: "positional NAME must be a symbol or string" }` triple at
/// the NAME-slot validation site in `compile_named_from_forms::<T>`
/// into ONE named primitive. Sibling of `LispError::NamedFormMissingName`
/// (the structural variant already in flight on the rejection chain):
/// that variant fires when the form has no NAME slot at all
/// (`(defpoint)`); this helper fires AFTER the arity gate has passed
/// but BEFORE `T::compile_from_args` runs — at the second of two
/// `compile_named_from_forms` rejection points
/// (arity → name-symbol-or-string → compile_from_args).
///
/// Emission stays `LispError::Compile`-shaped (byte-identical Display)
/// so authoring-tool substring greps (`tatara-check`, REPL) see no
/// drift across the lift. Naming the failure mode at the helper
/// boundary instead of inline at the call site:
///
/// 1. Centralizes the canonical legacy message
///    `"positional NAME must be a symbol or string"` in ONE place —
///    a typo can never drift if the lift is ever extended to a
///    second site (e.g. a future `compile_named_from_sexp` entry
///    point that shares the gate).
/// 2. Names the failure mode at the type level — the helper is the
///    one place to land a future promotion to a structural variant
///    (e.g. `LispError::NamedFormNonSymbolName { keyword, got }`).
///    Every call site picks up the promotion mechanically.
/// 3. Closes the "compile.rs has zero inline `LispError::Compile {
///    ... }` triples" milestone — every emission site in this file
///    now funnels through either a structural variant
///    (`NamedFormMissingName`) or a named helper (this one).
///
/// `T` is the typed-domain witness; the helper projects to
/// `T::KEYWORD.to_string()` at the boundary so the variant's `form`
/// slot carries the authoring-surface keyword verbatim, parallel to
/// how `NamedFormMissingName { keyword: T::KEYWORD }` carries the
/// same identifier as a `&'static str`.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; one
/// inline copy still earns a named primitive once the structural
/// shape is named (the test count gives this the fail-before/pass-
/// after edge, parallel to how `splice_outside_list`,
/// `non_symbol_param`, and `rest_param_missing_name` were lifted
/// from a single site for the structural-completeness payoff).
/// THEORY.md §V.1 — knowable platform; naming the failure mode at
/// the helper boundary is the first step toward exposing
/// `keyword` / `got` as first-class fields on a future structural
/// variant. THEORY.md §II.1 invariant 1 — typed entry; a NAME slot
/// that isn't a symbol or string is exactly the failure mode the
/// typed-entry gate exists to reject.
fn named_form_non_symbol_name<T: TataraDomain>() -> LispError {
    LispError::Compile {
        form: T::KEYWORD.to_string(),
        message: "positional NAME must be a symbol or string".into(),
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
            .ok_or_else(named_form_non_symbol_name::<T>)?
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

    // ── named_form_non_symbol_name: helper lift ─────────────────────
    //
    // The lone inline `LispError::Compile { form: T::KEYWORD.to_string(),
    // message: "positional NAME must be a symbol or string" }` triple in
    // `compile_named_from_forms::<T>` (the second gate in the
    // arity → name-symbol-or-string → compile_from_args rejection chain)
    // was lifted to `named_form_non_symbol_name::<T>()`. Emission stays
    // `LispError::Compile`-shaped (byte-identical Display) so authoring-
    // tool substring greps see no drift across the lift; the gain is the
    // named primitive at the helper boundary plus the future-promotion
    // landing site for a structural `LispError::NamedFormNonSymbolName
    // { keyword, got }` variant.
    //
    // The tests below pin: (a) each malformed NAME-slot input (int,
    // keyword, nested list) routes through the helper to the
    // `LispError::Compile` variant with the canonical keyword and
    // message; (b) the helper threads `T::KEYWORD` verbatim through
    // the `form` slot; (c) end-to-end through the `LispError` Display
    // impl matches the legacy rendering byte-for-byte; (d) the helper
    // is precisely scoped — a symbol NAME slot AND a string NAME slot
    // both pass through to `compile_from_args` cleanly, NOT through
    // the helper.

    #[test]
    fn compile_named_emits_compile_variant_for_int_name_slot_via_helper() {
        // `(defcompiler 5 :name "x")` — list[1] is an int literal, not
        // a symbol or string. The `as_symbol_or_string` ok_or_else
        // chain routes through `named_form_non_symbol_name::<T>()`,
        // which emits a `LispError::Compile { form: T::KEYWORD,
        // message: "positional NAME must be a symbol or string" }`-
        // shaped variant. A regression that drifts the variant (e.g.,
        // to a different shape) or the form/message slots fails-loudly
        // here. Same posture as the `compile_named_emits_named_form
        // _missing_name_for_keyword_only_form` test (the arity-gate
        // sibling): both pin the rejection-chain step at its emission
        // shape.
        let err = compile_named::<CompilerSpec>("(defcompiler 5 :name \"x\")").unwrap_err();
        match err {
            LispError::Compile { form, message } => {
                assert_eq!(form, "defcompiler");
                assert_eq!(message, "positional NAME must be a symbol or string");
            }
            other => panic!("expected LispError::Compile, got {other:?}"),
        }
    }

    #[test]
    fn compile_named_emits_compile_variant_for_keyword_name_slot_via_helper() {
        // `(defcompiler :foo :name "x")` — list[1] is a keyword, not a
        // symbol or string. Sibling negative path to the int-NAME case;
        // pins the helper covers every non-symbol-or-string Sexp shape
        // at the NAME slot, not just integers.
        let err = compile_named::<CompilerSpec>("(defcompiler :foo :name \"x\")").unwrap_err();
        match err {
            LispError::Compile { form, message } => {
                assert_eq!(form, "defcompiler");
                assert_eq!(message, "positional NAME must be a symbol or string");
            }
            other => panic!("expected LispError::Compile, got {other:?}"),
        }
    }

    #[test]
    fn compile_named_emits_compile_variant_for_nested_list_name_slot_via_helper() {
        // `(defcompiler (nested) :name "x")` — list[1] is a list, not
        // a symbol or string. Third negative path; closes the
        // "non-symbol-or-string at NAME slot" failure-mode set across
        // the three distinct Sexp shapes (atom-int, atom-keyword,
        // list) the helper rejects.
        let err = compile_named::<CompilerSpec>("(defcompiler (nested) :name \"x\")").unwrap_err();
        match err {
            LispError::Compile { form, message } => {
                assert_eq!(form, "defcompiler");
                assert_eq!(message, "positional NAME must be a symbol or string");
            }
            other => panic!("expected LispError::Compile, got {other:?}"),
        }
    }

    #[test]
    fn compile_named_non_symbol_name_renders_legacy_compile_shape() {
        // The lifted helper's Display matches the legacy `Compile`-shaped
        // diagnostic byte-for-byte — `"compile error in defcompiler:
        // positional NAME must be a symbol or string"` — so authoring
        // tools (`tatara-check`'s diagnostic capture, REPL substring-
        // greps) that pattern-matched on the rendered string see no
        // drift across the lift. Parallel to how
        // `compile_named_named_form_missing_name_renders_legacy_compile
        // _shape` pins the arity-gate sibling's Display contract.
        let err = compile_named::<CompilerSpec>("(defcompiler 5)").unwrap_err();
        assert_eq!(
            format!("{err}"),
            "compile error in defcompiler: positional NAME must be a symbol or string"
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