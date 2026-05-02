//! `TataraDomain` — a Rust type authorable as a Lisp `(<keyword> :k v …)` form.
//!
//! Apply `#[derive(TataraDomain)]` (from `tatara-lisp-derive`) and a plain
//! struct gains a full Lisp compiler: keyword dispatch, kwarg parsing, typed
//! field extraction.
//!
//! Also exposes a `DomainRegistry` + `linkme`-free `register_domain!` macro
//! so any crate that derives `TataraDomain` can auto-register itself; the
//! dispatcher then looks up unknown top-level forms by keyword at runtime.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::de::DeserializeOwned;

use crate::ast::{Atom, Sexp};
use crate::error::{LispError, Result};

/// A Rust type compilable from a Lisp form.
pub trait TataraDomain: Sized {
    /// The Lisp keyword (e.g., `"defmonitor"`).
    const KEYWORD: &'static str;

    /// Parse the argument list (everything after the keyword) into Self.
    fn compile_from_args(args: &[Sexp]) -> Result<Self>;

    /// Parse a complete form; validates the head symbol matches `KEYWORD`.
    fn compile_from_sexp(form: &Sexp) -> Result<Self> {
        let list = form.as_list().ok_or_else(|| LispError::Compile {
            form: Self::KEYWORD.to_string(),
            message: "expected list form".into(),
        })?;
        let head = list
            .first()
            .and_then(|s| s.as_symbol())
            .ok_or_else(|| LispError::Compile {
                form: Self::KEYWORD.to_string(),
                message: "missing head symbol".into(),
            })?;
        if head != Self::KEYWORD {
            return Err(LispError::Compile {
                form: Self::KEYWORD.to_string(),
                message: format!("expected ({} ...), got ({} ...)", Self::KEYWORD, head),
            });
        }
        Self::compile_from_args(&list[1..])
    }
}

// ── kwarg parsing + typed extractors used by the derive macro ──────

pub type Kwargs<'a> = HashMap<String, &'a Sexp>;

/// Parse `:k v :k v …` into a kwargs map. Rejects duplicate keywords so the
/// typed-entry gate fires on `(defX :name "a" :name "b")` instead of silently
/// keeping the last value — same posture `reject_unknown_kwargs` takes for
/// typo'd kwargs. A duplicate is ill-typed input: the author either meant
/// distinct keys (typo) or a list (`:tags ("a" "b")`).
///
/// Odd-length kwargs lists fail with `LispError::OddKwargs { dangling }`,
/// where `dangling` is the offending element's `Sexp::Display` projection
/// — `:query` for a keyword whose value got lost, or the literal form of a
/// stray non-keyword. Naming the dangling element keeps the diagnostic
/// structurally complete instead of merely flagging "odd number"; authoring
/// surfaces (REPL, LSP, `tatara-check`) render the mismatch without
/// re-reading the source.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 — "Typed entry. Ill-typed input
/// errors before the value exists." THEORY.md §V.1 — "knowable platform"
/// requires the diagnostic to name what was passed, not only what was
/// expected.
pub fn parse_kwargs(args: &[Sexp]) -> Result<Kwargs<'_>> {
    let mut kw = HashMap::new();
    let mut i = 0;
    while i + 1 < args.len() {
        let key = args[i].as_keyword().ok_or_else(|| LispError::Compile {
            form: "kwargs".into(),
            message: format!("expected keyword at position {i}"),
        })?;
        if kw.insert(key.to_string(), &args[i + 1]).is_some() {
            return Err(LispError::Compile {
                form: kwarg_form(key),
                message: "duplicate keyword".into(),
            });
        }
        i += 2;
    }
    if i < args.len() {
        return Err(LispError::OddKwargs {
            dangling: args[i].to_string(),
        });
    }
    Ok(kw)
}

/// Reject any keyword in `kw` that isn't in `allowed`. Closes the typed-entry
/// hole where typos like `:tthreshold 0.99` would otherwise parse silently
/// with the field unset. Emitted by `#[derive(TataraDomain)]` after
/// `parse_kwargs` so every derived domain rejects unknown kwargs by default.
///
/// Theory anchor: THEORY.md §II.1 invariant 1 (typed entry — "Ill-typed input
/// errors before the value exists").
pub fn reject_unknown_kwargs(kw: &Kwargs<'_>, allowed: &[&str]) -> Result<()> {
    for key in kw.keys() {
        if !allowed.contains(&key.as_str()) {
            let mut sorted: Vec<&&str> = allowed.iter().collect();
            sorted.sort();
            let allowed_list = sorted
                .iter()
                .map(|s| format!(":{s}"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(LispError::Compile {
                form: kwarg_form(key),
                message: format!("unknown keyword (allowed: {allowed_list})"),
            });
        }
    }
    Ok(())
}

pub fn required<'a>(kw: &'a Kwargs<'_>, key: &str) -> Result<&'a Sexp> {
    kw.get(key).copied().ok_or_else(|| LispError::Compile {
        form: kwarg_form(key),
        message: "required but not provided".into(),
    })
}

/// Canonical `form:` label for a kwarg-level `LispError::Compile`. Every
/// typed-entry diagnostic that names a kwarg (`required`, `type_err`,
/// `deserialize_err`, the duplicate-keyword paths in `parse_kwargs` and
/// `sexp_to_json`, the unknown-keyword path in `reject_unknown_kwargs`,
/// the non-list path in `extract_vec_via_serde`) routes through this one
/// helper, so authoring surfaces (REPL, LSP, `tatara-check`) bind to a
/// single named primitive rather than seven inline `format!(":{key}")`
/// copies.
///
/// Theory anchor: THEORY.md §VI.1 — "Generation over composition.
/// Three-times rule: when a pattern repeats three times, extract an
/// archetype/backend/synthesizer and generate from it." Seven inline
/// copies in one module is the textbook signal.
#[must_use]
pub fn kwarg_form(key: &str) -> String {
    format!(":{key}")
}

/// Canonical `form:` label for a failure inside the Nth item of a
/// list-typed kwarg — `:steps[1]` when the second item of `:steps` fails
/// to deserialize, `:tags[2]` when the third tag isn't a string. The
/// substrate names the item-path so the operator sees both *which kwarg*
/// and *which element* misfired without re-counting from the source.
///
/// Frontier inspiration: JSON Pointer (`/steps/1`) and jq path
/// expressions — lossless paths through value projections so downstream
/// tooling (LSP underlines, structural rewrites) bind to the path
/// instead of parsing the diagnostic message. Translation through
/// pleme-io primitives: the surface syntax authors already write
/// (`:<key>` + `[idx]`), no new error variant, no new IR layer. When a
/// future run gives `Sexp` source spans, the indexed form gains a
/// position the same way `kwarg_form` will — one helper, every consumer
/// inherits.
///
/// Theory anchor: THEORY.md §V.1 — "Knowable platform … Render
/// Anywhere." A diagnostic that names the kwarg but loses the item index
/// is structurally incomplete; the path completes it.
#[must_use]
pub fn kwarg_item_form(key: &str, idx: usize) -> String {
    format!(":{key}[{idx}]")
}

/// Stable, human-readable name of a `Sexp`'s outermost shape. Used by the
/// typed extractors to render `expected X, got Y` diagnostics so a
/// type-mismatched kwarg names both sides of the failure, not just the
/// expected side. Names are part of the public surface — `tatara-check`,
/// the LSP, and the REPL are expected to match on them — so they don't
/// drift across versions.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform. An error that names
/// only the expected side leaves the operator to guess what was passed;
/// naming both is the floor of constructive diagnostics. When a future
/// run gives `Sexp` source spans, this helper is the single site that
/// learns to thread `got Y at <pos>`; today's call sites pick up the
/// span automatically.
#[must_use]
pub fn sexp_type_name(s: &Sexp) -> &'static str {
    match s {
        Sexp::Nil => "nil",
        Sexp::Atom(Atom::Symbol(_)) => "symbol",
        Sexp::Atom(Atom::Keyword(_)) => "keyword",
        Sexp::Atom(Atom::Str(_)) => "string",
        Sexp::Atom(Atom::Int(_)) => "int",
        Sexp::Atom(Atom::Float(_)) => "float",
        Sexp::Atom(Atom::Bool(_)) => "bool",
        Sexp::List(_) => "list",
        Sexp::Quote(_) => "quote",
        Sexp::Quasiquote(_) => "quasiquote",
        Sexp::Unquote(_) => "unquote",
        Sexp::UnquoteSplice(_) => "unquote-splice",
    }
}

/// Structural type-mismatch builder. Pairs a path-shaped `form` (typically
/// `kwarg_form(_)` or `kwarg_item_form(_, _)`) with the static `expected`
/// label and the `got` projection of the offending `Sexp` through
/// `sexp_type_name`. Returns the dedicated `LispError::TypeMismatch`
/// variant so authoring surfaces (REPL, LSP, `tatara-check`) bind to
/// first-class `expected`/`got` fields instead of substring-parsing the
/// rendered message.
///
/// Three inline `format!("expected {X}, got {}", sexp_type_name(_))`
/// copies in this module (`type_err`, `extract_string_list` per-item,
/// `extract_vec_via_serde` non-list) used to assemble the same shape by
/// hand; the three-times rule (THEORY.md §VI.1) calls for one named
/// primitive. This is it. Future runs that thread `pos: Option<usize>`
/// from `Sexp` spans add ONE field to the variant; every type-mismatch
/// site inherits positional rendering with no consumer changes.
#[must_use]
pub fn type_mismatch(form: String, expected: &'static str, got: &Sexp) -> LispError {
    LispError::TypeMismatch {
        form,
        expected,
        got: sexp_type_name(got),
    }
}

fn type_err(key: &str, expected: &'static str, got: &Sexp) -> LispError {
    type_mismatch(kwarg_form(key), expected, got)
}

/// Item-indexed sibling of `type_err` — pairs `kwarg_item_form` with
/// `type_mismatch` so a per-item failure inside a list-typed kwarg names
/// `:<key>[<idx>]` plus the structural `expected`/`got` shape. Used by
/// `extract_string_list`'s per-item path; future per-item type-mismatch
/// sites (e.g. typed enums-of-strings, typed numeric vecs) bind here
/// rather than re-inlining the shape.
fn type_err_at(key: &str, idx: usize, expected: &'static str, got: &Sexp) -> LispError {
    type_mismatch(kwarg_item_form(key, idx), expected, got)
}

pub fn extract_string<'a>(kw: &'a Kwargs<'a>, key: &str) -> Result<&'a str> {
    let v = required(kw, key)?;
    v.as_string().ok_or_else(|| type_err(key, "string", v))
}

pub fn extract_optional_string<'a>(kw: &'a Kwargs<'a>, key: &str) -> Result<Option<&'a str>> {
    match kw.get(key) {
        None => Ok(None),
        Some(v) => match v.as_string() {
            Some(s) => Ok(Some(s)),
            None => Err(type_err(key, "string", v)),
        },
    }
}

pub fn extract_string_list(kw: &Kwargs<'_>, key: &str) -> Result<Vec<String>> {
    let Some(v) = kw.get(key).copied() else {
        return Ok(vec![]);
    };
    let list = v
        .as_list()
        .ok_or_else(|| type_err(key, "list of strings", v))?;
    list.iter()
        .enumerate()
        .map(|(idx, s)| {
            s.as_string()
                .map(String::from)
                .ok_or_else(|| type_err_at(key, idx, "string", s))
        })
        .collect()
}

pub fn extract_int(kw: &Kwargs<'_>, key: &str) -> Result<i64> {
    let v = required(kw, key)?;
    v.as_int().ok_or_else(|| type_err(key, "int", v))
}

pub fn extract_optional_int(kw: &Kwargs<'_>, key: &str) -> Result<Option<i64>> {
    match kw.get(key) {
        None => Ok(None),
        Some(v) => v.as_int().map(Some).ok_or_else(|| type_err(key, "int", v)),
    }
}

pub fn extract_float(kw: &Kwargs<'_>, key: &str) -> Result<f64> {
    let v = required(kw, key)?;
    v.as_float().ok_or_else(|| type_err(key, "number", v))
}

pub fn extract_optional_float(kw: &Kwargs<'_>, key: &str) -> Result<Option<f64>> {
    match kw.get(key) {
        None => Ok(None),
        Some(v) => v
            .as_float()
            .map(Some)
            .ok_or_else(|| type_err(key, "number", v)),
    }
}

pub fn extract_bool(kw: &Kwargs<'_>, key: &str) -> Result<bool> {
    let v = required(kw, key)?;
    v.as_bool().ok_or_else(|| type_err(key, "bool", v))
}

pub fn extract_optional_bool(kw: &Kwargs<'_>, key: &str) -> Result<Option<bool>> {
    match kw.get(key) {
        None => Ok(None),
        Some(v) => v
            .as_bool()
            .map(Some)
            .ok_or_else(|| type_err(key, "bool", v)),
    }
}

// ── Universal serde-Deserialize fallthrough (enums, nested structs, …) ──
//
// `#[derive(TataraDomain)]` covers `String` / numeric / `bool` / their
// `Option` and `Vec<String>` shapes with the typed extractors above. Any
// field type outside that closed set falls through to these helpers, which
// project the kwarg `Sexp` to canonical JSON via `sexp_to_json` and feed
// it to `serde_json::from_value` — works for any `serde::Deserialize`.
//
// The shape used to live inline in three `quote!` blocks in the derive
// macro (`Kind::Deserialize`, `Kind::OptionalDeserialize`,
// `Kind::VecDeserialize`). Lifting them here means:
//   - Hand-written `TataraDomain` impls share the same error path.
//   - Future diagnostic upgrades (attaching a source position once `Sexp`
//     carries spans, richer field-path traces) happen in ONE function,
//     not three macro-emitted copies.
//   - The `:<key> deserialize: …` message is a single named primitive in
//     the substrate — `tatara-check` / LSP / REPL render it uniformly.
//
// Theory anchor: THEORY.md §VI.1 (generation over composition — the
// generator must lean on the library, not duplicate the library inline).

fn deserialize_err(key: &str, err: &serde_json::Error) -> LispError {
    LispError::Compile {
        form: kwarg_form(key),
        message: format!("deserialize: {err}"),
    }
}

/// Item-indexed serde failure inside a `Vec<T>` kwarg. Pairs with
/// `kwarg_item_form` so the diagnostic names both the outer kwarg and the
/// failing item index — `:steps[1]` — instead of dropping the index.
fn deserialize_item_err(key: &str, idx: usize, err: &serde_json::Error) -> LispError {
    LispError::Compile {
        form: kwarg_item_form(key, idx),
        message: format!("deserialize: {err}"),
    }
}

/// Required field — feeds the kwarg's canonical-JSON projection to
/// `serde_json::from_value::<T>`. Errors carry `:key` so authoring tools
/// can point at the offending kwarg.
pub fn extract_via_serde<T: DeserializeOwned>(kw: &Kwargs<'_>, key: &str) -> Result<T> {
    let sexp = required(kw, key)?;
    let json = sexp_to_json(sexp)?;
    serde_json::from_value(json).map_err(|e| deserialize_err(key, &e))
}

/// Optional field — `None` if the kwarg is absent; `Some(T)` after a
/// successful `serde_json::from_value::<T>`.
pub fn extract_optional_via_serde<T: DeserializeOwned>(
    kw: &Kwargs<'_>,
    key: &str,
) -> Result<Option<T>> {
    let Some(sexp) = kw.get(key).copied() else {
        return Ok(None);
    };
    let json = sexp_to_json(sexp)?;
    serde_json::from_value(json)
        .map(Some)
        .map_err(|e| deserialize_err(key, &e))
}

/// `Vec<T>` field — empty vec if the kwarg is absent; otherwise the kwarg
/// must be a `Sexp::List` and each item is deserialized independently.
pub fn extract_vec_via_serde<T: DeserializeOwned>(kw: &Kwargs<'_>, key: &str) -> Result<Vec<T>> {
    let Some(sexp) = kw.get(key).copied() else {
        return Ok(Vec::new());
    };
    let list = sexp.as_list().ok_or_else(|| type_err(key, "list", sexp))?;
    list.iter()
        .enumerate()
        .map(|(idx, item)| {
            let json = sexp_to_json(item)?;
            serde_json::from_value(json).map_err(|e| deserialize_item_err(key, idx, &e))
        })
        .collect()
}

// ── Domain registry (runtime-registered, callable by keyword) ───────

/// Erased handler that knows how to compile a form and hand back a typed
/// serde-JSON representation. JSON is the least-common-denominator typed
/// surface — every `TataraDomain` derives `serde::Serialize` by convention.
pub struct DomainHandler {
    pub keyword: &'static str,
    pub compile: fn(args: &[Sexp]) -> Result<serde_json::Value>,
}

static REGISTRY: OnceLock<Mutex<HashMap<&'static str, DomainHandler>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<&'static str, DomainHandler>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a `TataraDomain` type with the global dispatcher.
/// Idempotent — repeated registrations overwrite.
pub fn register<T>()
where
    T: TataraDomain + serde::Serialize,
{
    let handler = DomainHandler {
        keyword: T::KEYWORD,
        compile: |args| {
            let v = T::compile_from_args(args)?;
            serde_json::to_value(&v).map_err(|e| LispError::Compile {
                form: T::KEYWORD.to_string(),
                message: format!("serialize: {e}"),
            })
        },
    };
    registry().lock().unwrap().insert(T::KEYWORD, handler);
}

/// Look up a handler by keyword.
pub fn lookup(keyword: &str) -> Option<DomainHandler> {
    let reg = registry().lock().unwrap();
    reg.get(keyword).map(|h| DomainHandler {
        keyword: h.keyword,
        compile: h.compile,
    })
}

/// List currently registered keywords.
pub fn registered_keywords() -> Vec<&'static str> {
    registry().lock().unwrap().keys().copied().collect()
}

// ── Sexp ↔ serde_json bridge (universal type support) ──────────────
//
// Lets the derive macro fall through to `serde_json::from_value` for any
// field type implementing `Deserialize`. Handles enums (via symbol→string),
// nested structs (via kwargs→object), and `Vec<T>` of either.

use serde_json::Value as JValue;

/// Convert a Sexp to its canonical JSON form.
///
/// Rules:
///   - Symbols + Keywords → `Value::String`
///     (symbols are enum discriminants; keywords prefix with `:`)
///   - Strings, ints, floats, bools → their JSON counterpart
///   - Lists that look like `:k v :k v …` → `Value::Object`
///   - Other lists → `Value::Array`
///   - Quote/Quasiquote/Unquote/UnquoteSplice → convert the inner (strips quote)
///
/// Fails on a duplicate keyword inside any nested kwargs-list (e.g.
/// `(:notify-ref "a" :notify-ref "b")`) — same typed-entry posture
/// `parse_kwargs` takes at the top level. The round-trip path
/// (`json_to_sexp` → `sexp_to_json`) is unaffected because
/// `serde_json::Map` is unique-keyed by construction.
pub fn sexp_to_json(s: &Sexp) -> Result<JValue> {
    Ok(match s {
        Sexp::Nil => JValue::Null,
        Sexp::Atom(Atom::Symbol(s)) => JValue::String(s.clone()),
        Sexp::Atom(Atom::Keyword(s)) => JValue::String(format!(":{s}")),
        Sexp::Atom(Atom::Str(s)) => JValue::String(s.clone()),
        Sexp::Atom(Atom::Int(n)) => JValue::Number((*n).into()),
        Sexp::Atom(Atom::Float(n)) => serde_json::Number::from_f64(*n)
            .map(JValue::Number)
            .unwrap_or(JValue::Null),
        Sexp::Atom(Atom::Bool(b)) => JValue::Bool(*b),
        Sexp::List(items) => {
            if is_kwargs_list(items) {
                let mut map = serde_json::Map::with_capacity(items.len() / 2);
                let mut i = 0;
                while i + 1 < items.len() {
                    if let Some(k) = items[i].as_keyword() {
                        let value = sexp_to_json(&items[i + 1])?;
                        if map.insert(kebab_to_camel(k), value).is_some() {
                            return Err(LispError::Compile {
                                form: kwarg_form(k),
                                message: "duplicate keyword".into(),
                            });
                        }
                        i += 2;
                    } else {
                        break;
                    }
                }
                JValue::Object(map)
            } else {
                JValue::Array(items.iter().map(sexp_to_json).collect::<Result<Vec<_>>>()?)
            }
        }
        Sexp::Quote(inner)
        | Sexp::Quasiquote(inner)
        | Sexp::Unquote(inner)
        | Sexp::UnquoteSplice(inner) => sexp_to_json(inner)?,
    })
}

/// Convert serde_json back to Sexp — inverse of `sexp_to_json`.
/// Used by `rewrite_typed` to round-trip a typed value through Lisp forms.
pub fn json_to_sexp(v: &JValue) -> Sexp {
    match v {
        JValue::Null => Sexp::Nil,
        JValue::Bool(b) => Sexp::boolean(*b),
        JValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Sexp::int(i)
            } else if let Some(f) = n.as_f64() {
                Sexp::float(f)
            } else {
                Sexp::int(0)
            }
        }
        JValue::String(s) => Sexp::string(s.clone()),
        JValue::Array(items) => Sexp::List(items.iter().map(json_to_sexp).collect()),
        JValue::Object(map) => {
            let mut out = Vec::with_capacity(map.len() * 2);
            for (k, v) in map {
                out.push(Sexp::keyword(camel_to_kebab(k)));
                out.push(json_to_sexp(v));
            }
            Sexp::List(out)
        }
    }
}

fn is_kwargs_list(items: &[Sexp]) -> bool {
    !items.is_empty()
        && items.len().is_multiple_of(2)
        && items.iter().step_by(2).all(|s| s.as_keyword().is_some())
}

/// `must-reach` → `mustReach`, `point-type` → `pointType`.
fn kebab_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for c in s.chars() {
        if c == '-' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// `mustReach` → `must-reach` (inverse of `kebab_to_camel`).
fn camel_to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('-');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

// ── TypedRewriter — the self-optimization primitive ────────────────
//
// Takes a typed value, converts to Sexp, applies a Lisp rewrite, then
// re-enters the typed boundary via `compile_from_args`. Any rewrite that
// passes the typed re-validation is safe by construction — the Rust type
// system is the floor.

/// Rewrite a typed `T` through Lisp form and re-validate on the way back.
///
/// The rewriter receives the value's kwargs representation (a `Sexp::List`
/// of alternating keywords + values) and returns a modified kwargs list.
/// `T::compile_from_args` validates the result — any ill-formed rewrite
/// produces a typed error; any well-formed rewrite produces a valid `T`.
pub fn rewrite_typed<T, F>(input: T, rewrite: F) -> Result<T>
where
    T: TataraDomain + serde::Serialize,
    F: FnOnce(Sexp) -> Result<Sexp>,
{
    let json = serde_json::to_value(&input).map_err(|e| LispError::Compile {
        form: T::KEYWORD.to_string(),
        message: format!("serialize {}: {e}", T::KEYWORD),
    })?;
    let sexp = json_to_sexp(&json);
    let rewritten = rewrite(sexp)?;
    let args = match rewritten {
        Sexp::List(items) => items,
        other => {
            return Err(LispError::Compile {
                form: T::KEYWORD.to_string(),
                message: format!("rewriter must return a list; got {other}"),
            })
        }
    };
    T::compile_from_args(&args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::read;
    use serde::{Deserialize, Serialize};
    use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

    /// Example domain authorable as Lisp — proves derive macro, trait, and
    /// registry all agree end-to-end.
    #[derive(DeriveTataraDomain, Serialize, Debug, PartialEq)]
    #[tatara(keyword = "defmonitor")]
    struct MonitorSpec {
        name: String,
        query: String,
        threshold: f64,
        window_seconds: Option<i64>,
        tags: Vec<String>,
        enabled: Option<bool>,
    }

    #[test]
    fn derive_emits_correct_keyword() {
        assert_eq!(MonitorSpec::KEYWORD, "defmonitor");
    }

    #[test]
    fn derive_compiles_full_form() {
        let forms = read(
            r#"(defmonitor
                 :name "prom-up"
                 :query "up{job='prometheus'}"
                 :threshold 0.99
                 :window-seconds 300
                 :tags ("prod" "observability")
                 :enabled #t)"#,
        )
        .unwrap();
        let spec = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(
            spec,
            MonitorSpec {
                name: "prom-up".into(),
                query: "up{job='prometheus'}".into(),
                threshold: 0.99,
                window_seconds: Some(300),
                tags: vec!["prod".into(), "observability".into()],
                enabled: Some(true),
            }
        );
    }

    #[test]
    fn derive_accepts_missing_optionals() {
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold 0.5)"#).unwrap();
        let spec = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(spec.name, "x");
        assert!(spec.window_seconds.is_none());
        assert!(spec.enabled.is_none());
        assert!(spec.tags.is_empty());
    }

    #[test]
    fn derive_errors_on_missing_required() {
        let forms = read(r#"(defmonitor :name "x" :query "q")"#).unwrap();
        assert!(MonitorSpec::compile_from_sexp(&forms[0]).is_err());
    }

    #[test]
    fn derive_errors_on_wrong_head() {
        let forms = read(r#"(not-a-monitor :name "x")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(format!("{err}").contains("expected (defmonitor"));
    }

    #[test]
    fn derive_rejects_unknown_keyword() {
        // Typed-entry invariant (THEORY.md §II.1.1) — a typo'd keyword
        // must surface as an error before the value exists, not parse
        // silently with the field unset.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tthreshold 0.99)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("tthreshold"),
            "error must name the offending keyword, got: {msg}"
        );
        assert!(
            msg.contains("unknown keyword"),
            "error must label the failure mode, got: {msg}"
        );
    }

    #[test]
    fn derive_unknown_keyword_lists_allowed_set() {
        // The error message includes the allowed-keyword set so the
        // operator can fix the typo without consulting the source.
        let forms = read(r#"(defmonitor :name "x" :ttreshold 0.99)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":threshold"),
            "expected :threshold listed: {msg}"
        );
        assert!(msg.contains(":query"), "expected :query listed: {msg}");
        assert!(msg.contains(":name"), "expected :name listed: {msg}");
    }

    #[test]
    fn reject_unknown_kwargs_helper_passes_when_all_known() {
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold 0.5)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &[
            "name",
            "query",
            "threshold",
            "window-seconds",
            "tags",
            "enabled",
        ];
        assert!(reject_unknown_kwargs(&kw, allowed).is_ok());
    }

    #[test]
    fn reject_unknown_kwargs_helper_errors_on_extra() {
        let forms = read(r#"(defmonitor :name "x" :ghost "boo")"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let kw = parse_kwargs(&args[1..]).unwrap();
        let allowed: &[&str] = &["name"];
        let err = reject_unknown_kwargs(&kw, allowed).unwrap_err();
        assert!(format!("{err}").contains("ghost"));
    }

    #[test]
    fn registry_dispatches_by_keyword() {
        register::<MonitorSpec>();
        assert!(registered_keywords().contains(&"defmonitor"));
        let handler = lookup("defmonitor").expect("registered");
        assert_eq!(handler.keyword, "defmonitor");
        let forms = read(r#"(ignored :name "prom" :query "q" :threshold 0.5)"#).unwrap();
        let args = forms[0].as_list().unwrap();
        let json = (handler.compile)(&args[1..]).unwrap();
        assert_eq!(json["name"], "prom");
        assert_eq!(json["query"], "q");
        assert_eq!(json["threshold"], 0.5);
    }

    // ── extract_via_serde / extract_optional_via_serde / extract_vec_via_serde ──
    //
    // These helpers used to live as three inline `quote!` blocks in
    // tatara-lisp-derive. Pinning their behavior here means a hand-written
    // `TataraDomain` impl can rely on the same contract the derive uses,
    // and a regression that re-inlines the boilerplate fails-loudly here
    // before it fans out.

    #[derive(Deserialize, Debug, PartialEq)]
    enum Severity {
        Info,
        Warning,
        Critical,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct EscalationStep {
        notify_ref: String,
        wait_minutes: Option<i64>,
    }

    fn kwargs_of(src: &str) -> Vec<Sexp> {
        // `(_ :k v :k v …)` — strip the head, return the kwargs slice.
        let forms = read(src).unwrap();
        let list = forms[0].as_list().unwrap();
        list[1..].to_vec()
    }

    #[test]
    fn extract_via_serde_parses_enum_from_symbol() {
        // `:level Critical` — bare symbol → enum discriminant via the
        // sexp_to_json bridge → serde Deserialize.
        let args = kwargs_of("(_ :level Critical)");
        let kw = parse_kwargs(&args).unwrap();
        let s: Severity = extract_via_serde(&kw, "level").unwrap();
        assert_eq!(s, Severity::Critical);
    }

    #[test]
    fn extract_via_serde_parses_nested_struct_from_kwargs_list() {
        let args = kwargs_of(r#"(_ :step (:notify-ref "oncall" :wait-minutes 5))"#);
        let kw = parse_kwargs(&args).unwrap();
        let s: EscalationStep = extract_via_serde(&kw, "step").unwrap();
        assert_eq!(
            s,
            EscalationStep {
                notify_ref: "oncall".into(),
                wait_minutes: Some(5),
            }
        );
    }

    #[test]
    fn extract_via_serde_missing_required_errors() {
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<Severity>(&kw, "level").unwrap_err();
        let msg = format!("{err}");
        // The `required` helper supplies the missing-kwarg message — same
        // path the typed extractors use, so authoring tools render
        // missing kwargs uniformly across both fallthroughs.
        assert!(
            msg.contains(":level"),
            "missing-kwarg error must name the kwarg, got: {msg}"
        );
        assert!(
            msg.contains("required"),
            "expected 'required' in missing-kwarg error, got: {msg}"
        );
    }

    #[test]
    fn extract_via_serde_deserialize_failure_labels_keyword() {
        // `:level NotASeverity` — well-formed Sexp, ill-formed enum.
        // The error must point at `:level` so the operator can fix the
        // typo without inspecting the source twice.
        let args = kwargs_of("(_ :level NotASeverity)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<Severity>(&kw, "level").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":level"),
            "deserialize error must name the kwarg, got: {msg}"
        );
        assert!(
            msg.contains("deserialize:"),
            "expected 'deserialize:' label, got: {msg}"
        );
    }

    #[test]
    fn extract_optional_via_serde_returns_none_when_absent() {
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let s: Option<Severity> = extract_optional_via_serde(&kw, "level").unwrap();
        assert!(s.is_none());
    }

    #[test]
    fn extract_optional_via_serde_returns_some_when_present() {
        let args = kwargs_of("(_ :level Warning)");
        let kw = parse_kwargs(&args).unwrap();
        let s: Option<Severity> = extract_optional_via_serde(&kw, "level").unwrap();
        assert_eq!(s, Some(Severity::Warning));
    }

    #[test]
    fn extract_vec_via_serde_returns_empty_when_absent() {
        // Absent-kwarg → empty `Vec` — same semantics `Vec<String>` gets
        // through `extract_string_list`. Authoring surfaces can rely on
        // "no entry == empty list" without a `#[serde(default)]` dance.
        let args = kwargs_of("(_ :other 1)");
        let kw = parse_kwargs(&args).unwrap();
        let v: Vec<EscalationStep> = extract_vec_via_serde(&kw, "steps").unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn extract_vec_via_serde_collects_nested_structs() {
        let args = kwargs_of(
            r#"(_ :steps (
                  (:notify-ref "a" :wait-minutes 0)
                  (:notify-ref "b" :wait-minutes 5)
                  (:notify-ref "c")))"#,
        );
        let kw = parse_kwargs(&args).unwrap();
        let v: Vec<EscalationStep> = extract_vec_via_serde(&kw, "steps").unwrap();
        assert_eq!(
            v,
            vec![
                EscalationStep {
                    notify_ref: "a".into(),
                    wait_minutes: Some(0),
                },
                EscalationStep {
                    notify_ref: "b".into(),
                    wait_minutes: Some(5),
                },
                EscalationStep {
                    notify_ref: "c".into(),
                    wait_minutes: None,
                },
            ]
        );
    }

    #[test]
    fn extract_vec_via_serde_rejects_non_list_kwarg() {
        // `:steps "scalar"` — a list-typed kwarg given a scalar must fail
        // with the kwarg name in the form, so the operator sees what to
        // change.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":steps"), "got: {msg}");
        assert!(msg.contains("expected list"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_item_failure_labels_keyword() {
        // First item is well-formed; second item has a typo'd field.
        // The error must still point at `:steps`, even though the
        // failure is inside an item.
        let args = kwargs_of(
            r#"(_ :steps (
                  (:notify-ref "ok")
                  (:notify-ref 7)))"#,
        );
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":steps"), "got: {msg}");
        assert!(msg.contains("deserialize:"), "got: {msg}");
    }

    // ── Duplicate-keyword rejection (typed-entry hardening) ─────────────
    //
    // A typo like `:name "x" :name "y"` used to silently overwrite — the
    // last value wins, the operator gets no signal. Same bug class
    // `reject_unknown_kwargs` (commit 2750f39) closed for typo'd kwargs;
    // this closes the dual hole for duplicate kwargs at every nesting
    // level (top-level args, nested struct kwargs, vec item kwargs).
    //
    // Theory anchor: THEORY.md §II.1 invariant 1 (typed entry —
    // "Ill-typed input errors before the value exists").

    #[test]
    fn parse_kwargs_rejects_duplicate_top_level_keyword() {
        let args = kwargs_of(r#"(_ :name "x" :name "y")"#);
        let err = parse_kwargs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":name"),
            "error must name the keyword, got: {msg}"
        );
        assert!(
            msg.contains("duplicate keyword"),
            "expected 'duplicate keyword' label, got: {msg}"
        );
    }

    #[test]
    fn parse_kwargs_accepts_distinct_keywords() {
        // Negative-control: pre-existing flow is preserved.
        let args = kwargs_of(r#"(_ :name "x" :query "q" :threshold 0.5)"#);
        let kw = parse_kwargs(&args).unwrap();
        assert_eq!(kw.len(), 3);
    }

    #[test]
    fn extract_via_serde_rejects_duplicate_in_nested_struct() {
        // `:step (:notify-ref "a" :notify-ref "b")` — the duplicate fires
        // during the `sexp_to_json` projection, before serde sees a value.
        let args = kwargs_of(r#"(_ :step (:notify-ref "a" :notify-ref "b"))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_via_serde::<EscalationStep>(&kw, "step").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":notify-ref"),
            "duplicate-in-nested error must name the inner kwarg, got: {msg}"
        );
        assert!(
            msg.contains("duplicate keyword"),
            "expected 'duplicate keyword' label, got: {msg}"
        );
    }

    #[test]
    fn extract_vec_via_serde_rejects_duplicate_in_item() {
        // `:steps ((:notify-ref "a" :notify-ref "b"))` — the duplicate is
        // inside one vec item. Authors get the same diagnostic shape
        // whether the duplicate is at the top level, in a nested struct,
        // or inside a vec item.
        let args = kwargs_of(r#"(_ :steps ((:notify-ref "a" :notify-ref "b")))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":notify-ref"), "got: {msg}");
        assert!(msg.contains("duplicate keyword"), "got: {msg}");
    }

    #[test]
    fn derive_rejects_duplicate_top_level_kwarg() {
        // End-to-end through `#[derive(TataraDomain)]` — silent overwrite
        // is exactly the bug class the typed-entry gate exists to prevent,
        // and every derived domain inherits the rejection by sharing
        // `parse_kwargs`.
        let forms = read(r#"(defmonitor :name "x" :name "y" :query "q" :threshold 0.5)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":name"), "got: {msg}");
        assert!(msg.contains("duplicate"), "got: {msg}");
    }

    #[test]
    fn json_to_sexp_round_trip_does_not_trip_duplicate_check() {
        // The round-trip path used by `rewrite_typed`: a typed value
        // → `serde_json::Value` (unique-keyed) → `Sexp` via `json_to_sexp`
        // → top-level kwargs slice → `parse_kwargs`. The duplicate-check
        // gate must NOT false-positive on this canonical input.
        let original = MonitorSpec {
            name: "x".into(),
            query: "q".into(),
            threshold: 0.5,
            window_seconds: None,
            tags: vec![],
            enabled: None,
        };
        let json = serde_json::to_value(&original).unwrap();
        let sexp = json_to_sexp(&json);
        let args = sexp.as_list().expect("object → kwargs list").to_vec();
        let _kw = parse_kwargs(&args).expect("round-trip kwargs are unique by construction");
    }

    #[test]
    fn sexp_to_json_round_trip_array_unaffected_by_duplicate_check() {
        // Arrays-of-objects round-trip: each object is unique-keyed by
        // virtue of being authored as a `serde_json::Map`. The strict
        // duplicate check must not false-positive on this shape.
        let json = serde_json::json!([
            { "notifyRef": "a", "waitMinutes": 0 },
            { "notifyRef": "b", "waitMinutes": 5 },
        ]);
        let sexp = json_to_sexp(&json);
        let back = sexp_to_json(&sexp).expect("round-trip array must not trip duplicate check");
        // The array is preserved (object key order is stable inside each
        // element because `json_to_sexp` writes kwargs in iteration order
        // and `sexp_to_json` reads them back in the same order).
        assert_eq!(back, json);
    }

    // ── Type-mismatch diagnostics name both expected and got ───────────
    //
    // Every typed extractor's `expected X` message used to leave the operator
    // to inspect the source to discover what kind of value was actually
    // passed. The `expected X, got Y` shape closes that gap: the diagnostic
    // is structurally complete so an authoring surface (REPL, LSP,
    // tatara-check) can render the mismatch without re-reading the input.
    //
    // `sexp_type_name` is the named primitive doing the projection; pinning
    // its outputs here keeps downstream tooling that matches on the names
    // (e.g., "expected string, got int" → squiggly under the int) safe
    // across versions.

    #[test]
    fn sexp_type_name_covers_every_variant() {
        assert_eq!(sexp_type_name(&Sexp::Nil), "nil");
        assert_eq!(sexp_type_name(&Sexp::symbol("foo")), "symbol");
        assert_eq!(sexp_type_name(&Sexp::keyword("k")), "keyword");
        assert_eq!(sexp_type_name(&Sexp::string("s")), "string");
        assert_eq!(sexp_type_name(&Sexp::int(7)), "int");
        assert_eq!(sexp_type_name(&Sexp::float(7.5)), "float");
        assert_eq!(sexp_type_name(&Sexp::boolean(true)), "bool");
        assert_eq!(sexp_type_name(&Sexp::List(vec![])), "list");
        assert_eq!(sexp_type_name(&Sexp::Quote(Box::new(Sexp::Nil))), "quote");
        assert_eq!(
            sexp_type_name(&Sexp::Quasiquote(Box::new(Sexp::Nil))),
            "quasiquote"
        );
        assert_eq!(
            sexp_type_name(&Sexp::Unquote(Box::new(Sexp::Nil))),
            "unquote"
        );
        assert_eq!(
            sexp_type_name(&Sexp::UnquoteSplice(Box::new(Sexp::Nil))),
            "unquote-splice"
        );
    }

    fn type_err_message(err: LispError) -> String {
        format!("{err}")
    }

    #[test]
    fn extract_string_type_err_names_got_int() {
        let args = kwargs_of("(_ :name 42)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_string(&kw, "name").unwrap_err());
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
        assert!(msg.contains(":name"), "got: {msg}");
    }

    #[test]
    fn extract_optional_string_type_err_names_got_bool() {
        let args = kwargs_of("(_ :name #t)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_string(&kw, "name").unwrap_err());
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got bool"), "got: {msg}");
    }

    #[test]
    fn extract_int_type_err_names_got_string() {
        let args = kwargs_of(r#"(_ :n "seven")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_int(&kw, "n").unwrap_err());
        assert!(msg.contains("expected int"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_float_type_err_names_got_bool() {
        let args = kwargs_of("(_ :ratio #f)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_float(&kw, "ratio").unwrap_err());
        assert!(msg.contains("expected number"), "got: {msg}");
        assert!(msg.contains("got bool"), "got: {msg}");
    }

    #[test]
    fn extract_bool_type_err_names_got_int() {
        let args = kwargs_of("(_ :enabled 1)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_bool(&kw, "enabled").unwrap_err());
        assert!(msg.contains("expected bool"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_string_list_type_err_on_scalar_names_got_string() {
        // `:tags "scalar"` — list-typed kwarg given a scalar. The error
        // names the actual shape so the operator sees the mismatch
        // structurally.
        let args = kwargs_of(r#"(_ :tags "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_string_list(&kw, "tags").unwrap_err());
        assert!(msg.contains("expected list of strings"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_string_list_type_err_on_non_string_item_names_index_and_got_int() {
        // `:tags ("ok" 7)` — outer is a list, the second item isn't a
        // string. Diagnostic names BOTH the item path (`:tags[1]`) and the
        // narrower per-item expectation (`expected string`, not the outer
        // `expected list of strings`) so authors see structurally where
        // the failure is, not just which kwarg.
        let args = kwargs_of(r#"(_ :tags ("ok" 7))"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_string_list(&kw, "tags").unwrap_err());
        assert!(
            msg.contains(":tags[1]"),
            "expected indexed item path, got: {msg}"
        );
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_optional_int_type_err_names_got_string() {
        let args = kwargs_of(r#"(_ :n "seven")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_int(&kw, "n").unwrap_err());
        assert!(msg.contains("expected int"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_optional_float_type_err_names_got_string() {
        let args = kwargs_of(r#"(_ :ratio "half")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_float(&kw, "ratio").unwrap_err());
        assert!(msg.contains("expected number"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_optional_bool_type_err_names_got_int() {
        let args = kwargs_of("(_ :enabled 1)");
        let kw = parse_kwargs(&args).unwrap();
        let msg = type_err_message(extract_optional_bool(&kw, "enabled").unwrap_err());
        assert!(msg.contains("expected bool"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_non_list_kwarg_names_got_string() {
        // `:steps "scalar"` — the vec-fallthrough's "expected list" used
        // to be a bare label; now it also reports the actual outer shape.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg =
            type_err_message(extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err());
        assert!(msg.contains("expected list"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn derive_type_err_end_to_end_names_got_string_for_threshold() {
        // End-to-end through `#[derive(TataraDomain)]`. A misspelled-as-
        // string `:threshold "tight"` used to surface as "expected
        // number" with no signal what was actually passed; now the
        // diagnostic carries `got string` so authoring surfaces have
        // structural info to render without re-reading the source.
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold "tight")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":threshold"), "got: {msg}");
        assert!(msg.contains("expected number"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    // ── Odd-kwargs dangling-element naming ─────────────────────────────
    //
    // `(defX :name "x" :query)` used to surface as the bare "odd number of
    // keyword arguments" message — operator could not tell whether
    // `:query`'s value got lost or whether the form was malformed. The
    // structural fix names the dangling element via `Sexp::Display`:
    //   - keyword case (`:query` with no value) → `:query`
    //   - non-keyword case (stray `5` at tail)  → `5`
    // Both halves of the failure are now structurally complete: the gate
    // names the failure mode AND the offending element. Pinning each case
    // here keeps `tatara-check` / LSP / REPL renderings safe across
    // versions, and means a future run that gives `Sexp` source spans
    // attaches a position to the same single primitive (`OddKwargs`)
    // mechanically.
    //
    // Theory anchor: THEORY.md §II.1 invariant 1 (typed entry); §V.1
    // (knowable platform — diagnostic names both expected and actual).

    #[test]
    fn parse_kwargs_names_dangling_keyword() {
        // `:name "x" :query` — `:query` has no value. The error variant
        // carries the dangling kwarg's display, so the author sees which
        // keyword lost its value.
        let args = kwargs_of(r#"(_ :name "x" :query)"#);
        let err = parse_kwargs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            matches!(err, LispError::OddKwargs { ref dangling } if dangling == ":query"),
            "expected OddKwargs {{ dangling: \":query\" }}, got {err:?}"
        );
        assert!(
            msg.contains(":query"),
            "error must name the dangling keyword, got: {msg}"
        );
        assert!(
            msg.contains("dangling"),
            "expected 'dangling' in the message, got: {msg}"
        );
    }

    #[test]
    fn parse_kwargs_names_dangling_non_keyword_scalar() {
        // `:name "x" :query "q" 5` — a stray scalar at the tail. The
        // dangling element's `Sexp::Display` is `5`; the diagnostic must
        // name it so the author knows what to delete (or which kwarg key
        // to add in front of it).
        let args = kwargs_of(r#"(_ :name "x" :query "q" 5)"#);
        let err = parse_kwargs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            matches!(err, LispError::OddKwargs { ref dangling } if dangling == "5"),
            "expected OddKwargs {{ dangling: \"5\" }}, got {err:?}"
        );
        assert!(
            msg.contains('5'),
            "error must name the dangling scalar, got: {msg}"
        );
    }

    #[test]
    fn parse_kwargs_names_dangling_string_scalar() {
        // `:name "x" "stray"` — a stray string at the tail. The Sexp
        // Display projects strings through `{:?}`, so the diagnostic
        // contains the quoted form `"stray"` — preserves the typed shape.
        let args = kwargs_of(r#"(_ :name "x" "stray")"#);
        let err = parse_kwargs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(
            matches!(err, LispError::OddKwargs { ref dangling } if dangling == "\"stray\""),
            "expected OddKwargs {{ dangling: \"\\\"stray\\\"\" }}, got {err:?}"
        );
        assert!(
            msg.contains("stray"),
            "error must name the dangling string, got: {msg}"
        );
    }

    #[test]
    fn parse_kwargs_single_dangling_keyword() {
        // `(_ :only)` — a single dangling keyword with nothing else. The
        // gate must name it the same way as the multi-kwarg case;
        // structural completeness should not depend on list length.
        let args = kwargs_of("(_ :only)");
        let err = parse_kwargs(&args).unwrap_err();
        assert!(
            matches!(err, LispError::OddKwargs { ref dangling } if dangling == ":only"),
            "expected OddKwargs {{ dangling: \":only\" }}, got {err:?}"
        );
    }

    #[test]
    fn derive_odd_kwargs_end_to_end_names_dangling_keyword() {
        // End-to-end through `#[derive(TataraDomain)]`. A truncated
        // authoring form `(defmonitor :name "x" :query)` used to surface
        // as a bare "odd number" message; now every derived domain
        // inherits the named-dangling-element diagnostic for free
        // because they all funnel through `parse_kwargs`.
        let forms = read(r#"(defmonitor :name "x" :query)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":query"),
            "derived odd-kwargs error must name the dangling kwarg, got: {msg}"
        );
        assert!(
            msg.contains("dangling"),
            "expected 'dangling' label end-to-end, got: {msg}"
        );
    }

    // ── Indexed-item form labels for list-typed kwargs ─────────────────
    //
    // `kwarg_form` and `kwarg_item_form` are the two named primitives
    // that build the `form:` field of every typed-entry diagnostic. The
    // base helper consolidates seven inline `format!(":{key}")` copies
    // (parse_kwargs duplicate, reject_unknown_kwargs, required, type_err,
    // deserialize_err, sexp_to_json's nested-duplicate, the non-list
    // path in extract_vec_via_serde) into one site; the indexed helper
    // adds the structural slot for *which item* failed.
    //
    // Pinning the canonical shapes here keeps downstream tooling
    // (`tatara-check`, LSP, REPL) safe across versions, and means a
    // future run that gives `Sexp` source spans threads `pos` through
    // ONE primitive instead of every macro emit. Frontier inspiration:
    // JSON Pointer (`/steps/1`), jq paths.

    #[test]
    fn kwarg_form_renders_canonical_shape() {
        assert_eq!(kwarg_form("threshold"), ":threshold");
        assert_eq!(kwarg_form("notify-ref"), ":notify-ref");
        // No transformation of the key — the surface name is what the
        // author sees in the source. `kebab_to_camel` happens elsewhere.
        assert_eq!(kwarg_form(""), ":");
    }

    #[test]
    fn kwarg_item_form_renders_canonical_indexed_shape() {
        assert_eq!(kwarg_item_form("tags", 0), ":tags[0]");
        assert_eq!(kwarg_item_form("steps", 1), ":steps[1]");
        assert_eq!(kwarg_item_form("steps", 17), ":steps[17]");
    }

    #[test]
    fn extract_string_list_outer_failure_keeps_unindexed_form() {
        // Negative-control: the outer-shape failure (`:tags "scalar"`)
        // is at the kwarg level, not the item level — its form must NOT
        // pick up an `[idx]` suffix, and the message keeps the wider
        // `expected list of strings`.
        let args = kwargs_of(r#"(_ :tags "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string_list(&kw, "tags").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":tags"), "got: {msg}");
        assert!(
            !msg.contains(":tags["),
            "outer failure must not gain an item index, got: {msg}"
        );
        assert!(msg.contains("expected list of strings"), "got: {msg}");
        assert!(msg.contains("got string"), "got: {msg}");
    }

    #[test]
    fn extract_string_list_indexes_each_failing_item() {
        // The first non-string item wins (collect short-circuits on the
        // first Err). Pin the index math: a failure at position 2 must
        // surface as `:tags[2]`, not `:tags[0]` or `:tags[1]`.
        let args = kwargs_of(r#"(_ :tags ("ok" "also-ok" 7))"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = format!("{}", extract_string_list(&kw, "tags").unwrap_err());
        assert!(msg.contains(":tags[2]"), "got: {msg}");
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_indexes_failing_item() {
        // Second item has a non-string `:notify-ref`. The serde error
        // must surface under `:steps[1]` so the operator goes straight
        // to the bad item — previously the index was lost and the
        // diagnostic only named `:steps`.
        let args = kwargs_of(
            r#"(_ :steps (
                  (:notify-ref "ok")
                  (:notify-ref 7)))"#,
        );
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains(":steps[1]"), "got: {msg}");
        assert!(msg.contains("deserialize:"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_outer_failure_keeps_unindexed_form() {
        // Negative-control: the outer kwarg-isn't-a-list failure stays
        // at `:steps` (no `[N]`). The wider `expected list` message is
        // preserved.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = format!(
            "{}",
            extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err()
        );
        assert!(msg.contains(":steps"), "got: {msg}");
        assert!(
            !msg.contains(":steps["),
            "outer failure must not gain an item index, got: {msg}"
        );
        assert!(msg.contains("expected list"), "got: {msg}");
    }

    #[test]
    fn extract_vec_via_serde_propagates_inner_duplicate_with_inner_form() {
        // Inner `(:notify-ref "a" :notify-ref "b")` fails inside
        // `sexp_to_json` BEFORE `serde_json::from_value` runs — that
        // path's error already carries its own `form: ":notify-ref"`,
        // and the item-level wrapper must not clobber it with
        // `:steps[0]`. Pin the propagation: the operator sees the
        // duplicated inner kwarg, not just the item index.
        let args = kwargs_of(r#"(_ :steps ((:notify-ref "a" :notify-ref "b")))"#);
        let kw = parse_kwargs(&args).unwrap();
        let msg = format!(
            "{}",
            extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err()
        );
        assert!(msg.contains(":notify-ref"), "got: {msg}");
        assert!(msg.contains("duplicate keyword"), "got: {msg}");
    }

    #[test]
    fn derive_indexed_item_failure_e2e_via_monitor_tags() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // `:tags ("prod" 7)` must surface as `:tags[1]` so every
        // derived domain inherits the indexed-item diagnostic by
        // sharing `extract_string_list` — no per-derive macro change.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tags ("prod" 7))"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(":tags[1]"),
            "derived item-failure error must name the index, got: {msg}"
        );
        assert!(msg.contains("expected string"), "got: {msg}");
        assert!(msg.contains("got int"), "got: {msg}");
    }

    #[test]
    fn parse_kwargs_well_formed_input_is_unaffected() {
        // Negative-control: even-length kwargs lists with no duplicates
        // and no unknowns continue to parse identically. The dangling-
        // element gate must NOT false-positive on canonical input.
        let args = kwargs_of(r#"(_ :name "x" :query "q" :threshold 0.5)"#);
        let kw = parse_kwargs(&args).expect("well-formed kwargs must parse");
        assert_eq!(kw.len(), 3);
    }

    // ── Structural TypeMismatch variant ────────────────────────────────
    //
    // The three "expected X, got Y" sites in this module — `type_err`,
    // `extract_string_list` per-item, `extract_vec_via_serde` non-list —
    // used to assemble the message inline via three near-identical
    // `format!("expected {expected}, got {}", sexp_type_name(_))` copies.
    // Three copies is the THEORY.md §VI.1 three-times-rule signal.
    //
    // `LispError::TypeMismatch { form, expected, got }` collapses the
    // shape into one structural variant: `form` is the path slot
    // (`kwarg_form` or `kwarg_item_form`), `expected` is the static
    // expectation, `got` is the static `sexp_type_name` projection.
    // Authoring tools (REPL, LSP, `tatara-check`) bind to the variant
    // directly instead of substring-parsing a rendered message; rendered
    // text matches the legacy `Compile`-shaped diagnostic byte-for-byte,
    // so existing `msg.contains("expected …")` assertions pass.
    //
    // Pinning the variant identity here keeps the structural binding
    // safe across versions, and means a future run that gives `Sexp`
    // source spans threads `pos: Option<usize>` through ONE primitive
    // (`type_mismatch`) — every type-mismatch site picks up positional
    // rendering with no consumer changes.

    #[test]
    fn type_mismatch_helper_emits_structured_variant() {
        let err = type_mismatch("ctx".to_string(), "string", &Sexp::int(7));
        match err {
            LispError::TypeMismatch {
                form,
                expected,
                got,
            } => {
                assert_eq!(form, "ctx");
                assert_eq!(expected, "string");
                assert_eq!(got, "int");
            }
            other => panic!("expected TypeMismatch, got {other:?}"),
        }
    }

    #[test]
    fn type_mismatch_display_matches_legacy_compile_shape() {
        // The user-visible string is byte-for-byte equivalent to the
        // pre-lift `LispError::Compile { message: format!("expected …, got …") }`
        // rendering. Authoring surfaces that pattern-match on the message
        // text continue to work; tools that pattern-match on the variant
        // gain structural binding.
        let err = type_mismatch(":threshold".to_string(), "number", &Sexp::string("tight"));
        assert_eq!(
            format!("{err}"),
            "compile error in :threshold: expected number, got string"
        );
    }

    #[test]
    fn extract_string_returns_type_mismatch_variant() {
        // The kwarg-level `expected X, got Y` site now produces the
        // structural variant. Pin the variant identity AND the rendered
        // message so the substrate's contract is locked from both
        // angles.
        let args = kwargs_of("(_ :name 42)");
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string(&kw, "name").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    ref form,
                    expected: "string",
                    got: "int",
                } if form == ":name"
            ),
            "expected TypeMismatch {{ form: \":name\", expected: \"string\", got: \"int\" }}, got {err:?}"
        );
        assert_eq!(
            format!("{err}"),
            "compile error in :name: expected string, got int"
        );
    }

    #[test]
    fn extract_string_list_per_item_returns_indexed_type_mismatch() {
        // Per-item failure in a `Vec<String>` kwarg flows through
        // `type_err_at` → `kwarg_item_form` + `type_mismatch`.
        let args = kwargs_of(r#"(_ :tags ("ok" 7))"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string_list(&kw, "tags").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    ref form,
                    expected: "string",
                    got: "int",
                } if form == ":tags[1]"
            ),
            "expected indexed TypeMismatch, got {err:?}"
        );
    }

    #[test]
    fn extract_vec_via_serde_non_list_returns_type_mismatch() {
        // The vec-fallthrough's "expected list" path lifts into the
        // same variant — `:steps "scalar"` no longer produces
        // `LispError::Compile`; it produces `TypeMismatch` with
        // `expected: "list"`, `got: "string"`. Authoring tools see the
        // same shape regardless of which extractor reported the
        // mismatch.
        let args = kwargs_of(r#"(_ :steps "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_vec_via_serde::<EscalationStep>(&kw, "steps").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    ref form,
                    expected: "list",
                    got: "string",
                } if form == ":steps"
            ),
            "expected list-shape TypeMismatch, got {err:?}"
        );
    }

    #[test]
    fn extract_string_list_outer_failure_returns_list_of_strings_type_mismatch() {
        // The outer-shape failure (`:tags "scalar"`) is at the kwarg
        // level — its `expected` stays `"list of strings"` (wider than
        // the per-item case's `"string"`) and the form has no `[idx]`
        // suffix. Same variant; different `expected` + form.
        let args = kwargs_of(r#"(_ :tags "scalar")"#);
        let kw = parse_kwargs(&args).unwrap();
        let err = extract_string_list(&kw, "tags").unwrap_err();
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    ref form,
                    expected: "list of strings",
                    got: "string",
                } if form == ":tags"
            ),
            "expected outer-shape TypeMismatch, got {err:?}"
        );
    }

    #[test]
    fn type_mismatch_position_is_none_today() {
        // Negative-control: until `Sexp` carries spans, `position()`
        // returns `None` for the variant — `format_diagnostic` falls
        // through to single-line rendering, no caret emitted. Pinning
        // this contract means a future run that adds `pos: Option<usize>`
        // does so deliberately, with a fail-before/pass-after delta.
        let err = type_mismatch(":x".to_string(), "string", &Sexp::int(0));
        assert_eq!(err.position(), None);
    }

    #[test]
    fn derive_type_mismatch_e2e_via_monitor_threshold() {
        // End-to-end through `#[derive(TataraDomain)]` on `MonitorSpec`:
        // a misspelled-as-string `:threshold "tight"` surfaces the
        // structural variant. Every derived domain inherits the lift —
        // no per-derive macro change.
        let forms = read(r#"(defmonitor :name "x" :query "q" :threshold "tight")"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            matches!(
                err,
                LispError::TypeMismatch {
                    ref form,
                    expected: "number",
                    got: "string",
                } if form == ":threshold"
            ),
            "expected derived TypeMismatch, got {err:?}"
        );
    }
}
