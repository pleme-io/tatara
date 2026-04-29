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
        form: format!(":{key}"),
        message: format!("deserialize: {err}"),
    }
}

/// Required field — feeds the kwarg's canonical-JSON projection to
/// `serde_json::from_value::<T>`. Errors carry `:key` so authoring tools
/// can point at the offending kwarg.
pub fn extract_via_serde<T: DeserializeOwned>(kw: &Kwargs<'_>, key: &str) -> Result<T> {
    let sexp = required(kw, key)?;
    let json = sexp_to_json(sexp);
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
    let json = sexp_to_json(sexp);
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
    let list = sexp.as_list().ok_or_else(|| LispError::Compile {
        form: format!(":{key}"),
        message: "expected list".into(),
    })?;
    list.iter()
        .map(|item| {
            let json = sexp_to_json(item);
            serde_json::from_value(json).map_err(|e| deserialize_err(key, &e))
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
}
