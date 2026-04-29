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

use crate::ast::Sexp;
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

pub fn parse_kwargs(args: &[Sexp]) -> Result<Kwargs<'_>> {
    let mut kw = HashMap::new();
    let mut i = 0;
    while i + 1 < args.len() {
        let key = args[i].as_keyword().ok_or_else(|| LispError::Compile {
            form: "kwargs".into(),
            message: format!("expected keyword at position {i}"),
        })?;
        kw.insert(key.to_string(), &args[i + 1]);
        i += 2;
    }
    if i < args.len() {
        return Err(LispError::OddKwargs);
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
                form: format!(":{key}"),
                message: format!("unknown keyword (allowed: {allowed_list})"),
            });
        }
    }
    Ok(())
}

pub fn required<'a>(kw: &'a Kwargs<'_>, key: &str) -> Result<&'a Sexp> {
    kw.get(key).copied().ok_or_else(|| LispError::Compile {
        form: format!(":{key}"),
        message: "required but not provided".into(),
    })
}

fn type_err(key: &str, expected: &str) -> LispError {
    LispError::Compile {
        form: format!(":{key}"),
        message: format!("expected {expected}"),
    }
}

pub fn extract_string<'a>(kw: &'a Kwargs<'a>, key: &str) -> Result<&'a str> {
    required(kw, key)?
        .as_string()
        .ok_or_else(|| type_err(key, "string"))
}

pub fn extract_optional_string<'a>(kw: &'a Kwargs<'a>, key: &str) -> Result<Option<&'a str>> {
    match kw.get(key) {
        None => Ok(None),
        Some(v) => match v.as_string() {
            Some(s) => Ok(Some(s)),
            None => Err(type_err(key, "string")),
        },
    }
}

pub fn extract_string_list(kw: &Kwargs<'_>, key: &str) -> Result<Vec<String>> {
    let v = kw.get(key).copied();
    let Some(v) = v else {
        return Ok(vec![]);
    };
    let list = v
        .as_list()
        .ok_or_else(|| type_err(key, "list of strings"))?;
    list.iter()
        .map(|s| {
            s.as_string()
                .map(String::from)
                .ok_or_else(|| type_err(key, "list of strings"))
        })
        .collect()
}

pub fn extract_int(kw: &Kwargs<'_>, key: &str) -> Result<i64> {
    required(kw, key)?
        .as_int()
        .ok_or_else(|| type_err(key, "int"))
}

pub fn extract_optional_int(kw: &Kwargs<'_>, key: &str) -> Result<Option<i64>> {
    match kw.get(key) {
        None => Ok(None),
        Some(v) => v.as_int().map(Some).ok_or_else(|| type_err(key, "int")),
    }
}

pub fn extract_float(kw: &Kwargs<'_>, key: &str) -> Result<f64> {
    required(kw, key)?
        .as_float()
        .ok_or_else(|| type_err(key, "number"))
}

pub fn extract_optional_float(kw: &Kwargs<'_>, key: &str) -> Result<Option<f64>> {
    match kw.get(key) {
        None => Ok(None),
        Some(v) => v
            .as_float()
            .map(Some)
            .ok_or_else(|| type_err(key, "number")),
    }
}

pub fn extract_bool(kw: &Kwargs<'_>, key: &str) -> Result<bool> {
    required(kw, key)?
        .as_bool()
        .ok_or_else(|| type_err(key, "bool"))
}

pub fn extract_optional_bool(kw: &Kwargs<'_>, key: &str) -> Result<Option<bool>> {
    match kw.get(key) {
        None => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| type_err(key, "bool")),
    }
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

use crate::ast::Atom;
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
pub fn sexp_to_json(s: &Sexp) -> JValue {
    match s {
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
                        map.insert(kebab_to_camel(k), sexp_to_json(&items[i + 1]));
                        i += 2;
                    } else {
                        break;
                    }
                }
                JValue::Object(map)
            } else {
                JValue::Array(items.iter().map(sexp_to_json).collect())
            }
        }
        Sexp::Quote(inner)
        | Sexp::Quasiquote(inner)
        | Sexp::Unquote(inner)
        | Sexp::UnquoteSplice(inner) => sexp_to_json(inner),
    }
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
    use serde::Serialize;
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
}
