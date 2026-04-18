//! Builtin functions — the thin standard library exposed to the interpreter.
//!
//! Philosophy: only builtins that are awkward to implement in Lisp. Arithmetic,
//! list basics, string joining, attrs access, path construction, and the
//! `derivation` primitive. Everything else (map, filter, fold, …) can be
//! written in Lisp on top.

use std::collections::BTreeMap;
use std::sync::Arc;

use tatara_nix::derivation::{
    BuilderPhase, BuilderPhases, Derivation, EnvVar, InputRef, Outputs, Sandbox, Source,
};

use crate::error::{EvalError, Result};
use crate::value::{Arity, Builtin, BuiltinFn, Value};

/// Register all builtins into a name→Value map suitable for seeding the root
/// environment.
pub fn builtin_table() -> BTreeMap<String, Value> {
    let mut m: BTreeMap<String, Value> = BTreeMap::new();
    for b in all_builtins() {
        let name = b.name.clone();
        m.insert(name, Value::Builtin(Arc::new(b)));
    }
    m
}

fn all_builtins() -> Vec<Builtin> {
    vec![
        mk("+", Arity::AtLeast(0), Arc::new(add)),
        mk("-", Arity::AtLeast(1), Arc::new(sub)),
        mk("*", Arity::AtLeast(0), Arc::new(mul)),
        mk("/", Arity::AtLeast(2), Arc::new(div)),
        mk("=", Arity::Exact(2), Arc::new(eq)),
        mk("<", Arity::Exact(2), Arc::new(lt)),
        mk(">", Arity::Exact(2), Arc::new(gt)),
        mk("<=", Arity::Exact(2), Arc::new(le)),
        mk(">=", Arity::Exact(2), Arc::new(ge)),
        mk("not", Arity::Exact(1), Arc::new(not_)),
        mk("null?", Arity::Exact(1), Arc::new(null_q)),
        mk("list", Arity::Any, Arc::new(list_)),
        mk("cons", Arity::Exact(2), Arc::new(cons)),
        mk("car", Arity::Exact(1), Arc::new(car)),
        mk("cdr", Arity::Exact(1), Arc::new(cdr)),
        mk("length", Arity::Exact(1), Arc::new(length_)),
        mk("string-append", Arity::AtLeast(0), Arc::new(string_append)),
        mk("toString", Arity::Exact(1), Arc::new(to_string_)),
        mk("path", Arity::Exact(1), Arc::new(path_)),
        mk("attrs", Arity::Any, Arc::new(attrs_)),
        mk("attr", Arity::Exact(2), Arc::new(attr_)),
        mk("has-attr", Arity::Exact(2), Arc::new(has_attr_)),
        mk("attr-names", Arity::Exact(1), Arc::new(attr_names_)),
        mk("derivation", Arity::Exact(1), Arc::new(derivation_)),
        mk("store-path", Arity::Exact(1), Arc::new(store_path_)),
    ]
}

fn mk(name: &str, arity: Arity, f: Arc<BuiltinFn>) -> Builtin {
    Builtin {
        name: name.into(),
        arity,
        func: f,
    }
}

// ── arithmetic ──────────────────────────────────────────────────────────

fn add(args: &[Value]) -> Result<Value> {
    let mut acc: i64 = 0;
    for a in args {
        acc += coerce_int(a, "+")?;
    }
    Ok(Value::Int(acc))
}

fn sub(args: &[Value]) -> Result<Value> {
    let first = coerce_int(&args[0], "-")?;
    if args.len() == 1 {
        return Ok(Value::Int(-first));
    }
    let mut acc = first;
    for a in &args[1..] {
        acc -= coerce_int(a, "-")?;
    }
    Ok(Value::Int(acc))
}

fn mul(args: &[Value]) -> Result<Value> {
    let mut acc: i64 = 1;
    for a in args {
        acc *= coerce_int(a, "*")?;
    }
    Ok(Value::Int(acc))
}

fn div(args: &[Value]) -> Result<Value> {
    let mut acc = coerce_int(&args[0], "/")?;
    for a in &args[1..] {
        let n = coerce_int(a, "/")?;
        if n == 0 {
            return Err(EvalError::DivByZero);
        }
        acc /= n;
    }
    Ok(Value::Int(acc))
}

fn coerce_int(v: &Value, op: &str) -> Result<i64> {
    match v {
        Value::Int(n) => Ok(*n),
        Value::Float(f) => Ok(*f as i64),
        _ => Err(EvalError::Type {
            expected: format!("int (for {op})"),
            found: v.type_name().into(),
        }),
    }
}

// ── comparisons ─────────────────────────────────────────────────────────

fn eq(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(value_eq(&args[0], &args[1])))
}

fn lt(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(
        coerce_int(&args[0], "<")? < coerce_int(&args[1], "<")?,
    ))
}

fn gt(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(
        coerce_int(&args[0], ">")? > coerce_int(&args[1], ">")?,
    ))
}

fn le(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(
        coerce_int(&args[0], "<=")? <= coerce_int(&args[1], "<=")?,
    ))
}

fn ge(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(
        coerce_int(&args[0], ">=")? >= coerce_int(&args[1], ">=")?,
    ))
}

fn not_(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(!args[0].is_truthy()))
}

fn null_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(args[0], Value::Nil)))
}

fn value_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Nil, Value::Nil) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Symbol(a), Value::Symbol(b)) => a == b,
        (Value::Keyword(a), Value::Keyword(b)) => a == b,
        (Value::Path(a), Value::Path(b)) => a == b,
        (Value::List(a), Value::List(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| value_eq(x, y))
        }
        _ => false,
    }
}

// ── lists ────────────────────────────────────────────────────────────────

fn list_(args: &[Value]) -> Result<Value> {
    Ok(Value::List(Arc::new(args.to_vec())))
}

fn cons(args: &[Value]) -> Result<Value> {
    let tail = match &args[1] {
        Value::List(xs) => xs.clone(),
        _ => {
            return Err(EvalError::Type {
                expected: "list (for cons second arg)".into(),
                found: args[1].type_name().into(),
            })
        }
    };
    let mut out = Vec::with_capacity(tail.len() + 1);
    out.push(args[0].clone());
    out.extend(tail.iter().cloned());
    Ok(Value::List(Arc::new(out)))
}

fn car(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::List(xs) if !xs.is_empty() => Ok(xs[0].clone()),
        Value::List(_) => Err(EvalError::Other("car of empty list".into())),
        v => Err(EvalError::Type {
            expected: "list".into(),
            found: v.type_name().into(),
        }),
    }
}

fn cdr(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::List(xs) if !xs.is_empty() => Ok(Value::List(Arc::new(xs[1..].to_vec()))),
        Value::List(_) => Ok(Value::List(Arc::new(vec![]))),
        v => Err(EvalError::Type {
            expected: "list".into(),
            found: v.type_name().into(),
        }),
    }
}

fn length_(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::List(xs) => Ok(Value::Int(xs.len() as i64)),
        Value::Str(s) => Ok(Value::Int(s.len() as i64)),
        Value::Attrs(m) => Ok(Value::Int(m.len() as i64)),
        v => Err(EvalError::Type {
            expected: "list | string | attrs".into(),
            found: v.type_name().into(),
        }),
    }
}

// ── strings ──────────────────────────────────────────────────────────────

fn string_append(args: &[Value]) -> Result<Value> {
    let mut out = String::new();
    for a in args {
        match a.coerce_to_string() {
            Some(s) => out.push_str(&s),
            None => {
                return Err(EvalError::Type {
                    expected: "string-coercible".into(),
                    found: a.type_name().into(),
                })
            }
        }
    }
    Ok(Value::Str(out))
}

fn to_string_(args: &[Value]) -> Result<Value> {
    match args[0].coerce_to_string() {
        Some(s) => Ok(Value::Str(s)),
        None => Err(EvalError::Type {
            expected: "string-coercible".into(),
            found: args[0].type_name().into(),
        }),
    }
}

fn path_(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Str(s) => Ok(Value::Path(std::path::PathBuf::from(s))),
        v => Err(EvalError::Type {
            expected: "string".into(),
            found: v.type_name().into(),
        }),
    }
}

// ── attrs ────────────────────────────────────────────────────────────────

fn attrs_(args: &[Value]) -> Result<Value> {
    if args.len() % 2 != 0 {
        return Err(EvalError::Arity {
            name: "attrs".into(),
            expected: "even number of k v pairs".into(),
            got: args.len(),
        });
    }
    let mut m = BTreeMap::new();
    for pair in args.chunks_exact(2) {
        let key = match &pair[0] {
            Value::Str(s) | Value::Symbol(s) | Value::Keyword(s) => s.clone(),
            v => {
                return Err(EvalError::Type {
                    expected: "string key".into(),
                    found: v.type_name().into(),
                })
            }
        };
        m.insert(key, pair[1].clone());
    }
    Ok(Value::Attrs(Arc::new(m)))
}

fn attr_(args: &[Value]) -> Result<Value> {
    let name = match &args[0] {
        Value::Str(s) | Value::Symbol(s) | Value::Keyword(s) => s.clone(),
        v => {
            return Err(EvalError::Type {
                expected: "string".into(),
                found: v.type_name().into(),
            })
        }
    };
    match &args[1] {
        Value::Attrs(m) => m
            .get(&name)
            .cloned()
            .ok_or(EvalError::MissingAttr(name)),
        v => Err(EvalError::Type {
            expected: "attrs".into(),
            found: v.type_name().into(),
        }),
    }
}

fn has_attr_(args: &[Value]) -> Result<Value> {
    let name = match &args[0] {
        Value::Str(s) | Value::Symbol(s) | Value::Keyword(s) => s.clone(),
        v => {
            return Err(EvalError::Type {
                expected: "string".into(),
                found: v.type_name().into(),
            })
        }
    };
    match &args[1] {
        Value::Attrs(m) => Ok(Value::Bool(m.contains_key(&name))),
        v => Err(EvalError::Type {
            expected: "attrs".into(),
            found: v.type_name().into(),
        }),
    }
}

fn attr_names_(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Attrs(m) => Ok(Value::List(Arc::new(
            m.keys().cloned().map(Value::Str).collect(),
        ))),
        v => Err(EvalError::Type {
            expected: "attrs".into(),
            found: v.type_name().into(),
        }),
    }
}

// ── store-path (handy for composing StorePath values in Lisp) ───────────

fn store_path_(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Derivation(d) => Ok(Value::StorePath(d.store_path())),
        v => Err(EvalError::Type {
            expected: "derivation".into(),
            found: v.type_name().into(),
        }),
    }
}

// ── derivation — the headline builtin ───────────────────────────────────

/// `(derivation attrs)` — convert an attrset into a typed `Derivation`.
///
/// The attrset keys mirror the `Derivation` struct fields:
///
/// - `name`    (string, required)
/// - `version` (string, optional)
/// - `inputs`  (list of attrs with `name` / `version` / `pinned`)
/// - `source`  (attrs with `kind` and kind-specific keys)
/// - `builder` (attrs with `phases` list and `commands` attrs)
/// - `outputs` (attrs with `primary` / `extra`)
/// - `env`     (list of attrs with `name` / `value`)
/// - `sandbox` (attrs with `allow-network` / `extra-paths` / `impure-env`)
pub fn derivation_(args: &[Value]) -> Result<Value> {
    let attrs = match &args[0] {
        Value::Attrs(m) => m.clone(),
        v => {
            return Err(EvalError::Type {
                expected: "attrs".into(),
                found: v.type_name().into(),
            })
        }
    };
    let d = attrs_to_derivation(&attrs)?;
    Ok(Value::Derivation(Arc::new(d)))
}

fn attrs_to_derivation(m: &BTreeMap<String, Value>) -> Result<Derivation> {
    let name = require_str(m, "name")?.to_string();
    let version = optional_str(m, "version").map(str::to_string);

    let inputs = match m.get("inputs") {
        Some(Value::List(xs)) => xs
            .iter()
            .map(input_ref_from_value)
            .collect::<Result<Vec<_>>>()?,
        Some(v) => {
            return Err(EvalError::Type {
                expected: "list (for inputs)".into(),
                found: v.type_name().into(),
            })
        }
        None => vec![],
    };

    let source = match m.get("source") {
        Some(Value::Attrs(s)) => source_from_attrs(s)?,
        Some(v) => {
            return Err(EvalError::Type {
                expected: "attrs (for source)".into(),
                found: v.type_name().into(),
            })
        }
        None => Source::default(),
    };

    let builder = match m.get("builder") {
        Some(Value::Attrs(b)) => builder_from_attrs(b)?,
        Some(v) => {
            return Err(EvalError::Type {
                expected: "attrs (for builder)".into(),
                found: v.type_name().into(),
            })
        }
        None => BuilderPhases::default(),
    };

    let outputs = match m.get("outputs") {
        Some(Value::Attrs(o)) => outputs_from_attrs(o),
        Some(v) => {
            return Err(EvalError::Type {
                expected: "attrs (for outputs)".into(),
                found: v.type_name().into(),
            })
        }
        None => Outputs::default(),
    };

    let env = match m.get("env") {
        Some(Value::List(xs)) => xs
            .iter()
            .map(env_var_from_value)
            .collect::<Result<Vec<_>>>()?,
        Some(v) => {
            return Err(EvalError::Type {
                expected: "list (for env)".into(),
                found: v.type_name().into(),
            })
        }
        None => vec![],
    };

    let sandbox = match m.get("sandbox") {
        Some(Value::Attrs(s)) => sandbox_from_attrs(s),
        Some(v) => {
            return Err(EvalError::Type {
                expected: "attrs (for sandbox)".into(),
                found: v.type_name().into(),
            })
        }
        None => Sandbox::default(),
    };

    let bridge = match m.get("bridge") {
        Some(Value::Attrs(b)) => Some(bridge_from_attrs(b)?),
        Some(Value::Str(attr)) => Some(tatara_nix::derivation::BridgeTarget {
            attr_path: attr.clone(),
            pkg_set: None,
        }),
        Some(v) => {
            return Err(EvalError::Type {
                expected: "attrs | string (for bridge)".into(),
                found: v.type_name().into(),
            })
        }
        None => None,
    };

    Ok(Derivation {
        name,
        version,
        inputs,
        source,
        builder,
        outputs,
        env,
        sandbox,
        bridge,
        nix_expr: None,
    })
}

fn bridge_from_attrs(
    m: &BTreeMap<String, Value>,
) -> Result<tatara_nix::derivation::BridgeTarget> {
    Ok(tatara_nix::derivation::BridgeTarget {
        attr_path: require_str(m, "attr-path")
            .or_else(|_| require_str(m, "attrPath"))?
            .to_string(),
        pkg_set: optional_str(m, "pkg-set")
            .or_else(|| optional_str(m, "pkgSet"))
            .map(str::to_string),
    })
}

fn input_ref_from_value(v: &Value) -> Result<InputRef> {
    match v {
        Value::Attrs(m) => Ok(InputRef {
            name: require_str(m, "name")?.to_string(),
            version: optional_str(m, "version").map(str::to_string),
            pinned: match m.get("pinned") {
                Some(Value::StorePath(sp)) => Some(sp.clone()),
                Some(Value::Derivation(d)) => Some(d.store_path()),
                _ => None,
            },
        }),
        v => Err(EvalError::Type {
            expected: "attrs".into(),
            found: v.type_name().into(),
        }),
    }
}

fn source_from_attrs(m: &BTreeMap<String, Value>) -> Result<Source> {
    let kind = require_str(m, "kind")?;
    match kind {
        "Inline" => Ok(Source::Inline {
            content: require_str(m, "content")?.to_string(),
        }),
        "Path" => Ok(Source::Path {
            path: require_str(m, "path")?.to_string(),
        }),
        "Git" => Ok(Source::Git {
            url: require_str(m, "url")?.to_string(),
            rev: require_str(m, "rev")?.to_string(),
            submodules: optional_bool(m, "submodules").unwrap_or(false),
        }),
        "Tarball" => Ok(Source::Tarball {
            url: require_str(m, "url")?.to_string(),
            hash: require_str(m, "hash")?.to_string(),
        }),
        "Derivation" => {
            let input = match m.get("input") {
                Some(v) => input_ref_from_value(v)?,
                None => {
                    return Err(EvalError::Malformed {
                        form: "source".into(),
                        reason: "Derivation source needs :input".into(),
                    })
                }
            };
            Ok(Source::Derivation { input })
        }
        other => Err(EvalError::Malformed {
            form: "source".into(),
            reason: format!("unknown source kind: {other}"),
        }),
    }
}

fn builder_from_attrs(m: &BTreeMap<String, Value>) -> Result<BuilderPhases> {
    let phases = match m.get("phases") {
        Some(Value::List(xs)) => xs
            .iter()
            .map(phase_from_value)
            .collect::<Result<Vec<_>>>()?,
        _ => vec![],
    };
    let commands: BTreeMap<String, Vec<String>> = match m.get("commands") {
        Some(Value::Attrs(c)) => c
            .iter()
            .map(|(k, v)| {
                let cmds = match v {
                    Value::List(xs) => xs
                        .iter()
                        .map(|x| {
                            x.coerce_to_string().ok_or_else(|| EvalError::Type {
                                expected: "string".into(),
                                found: x.type_name().into(),
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                    Value::Str(s) => vec![s.clone()],
                    other => {
                        return Err(EvalError::Type {
                            expected: "list | string".into(),
                            found: other.type_name().into(),
                        })
                    }
                };
                Ok((k.clone(), cmds))
            })
            .collect::<Result<_>>()?,
        _ => BTreeMap::new(),
    };
    Ok(BuilderPhases { phases, commands })
}

fn phase_from_value(v: &Value) -> Result<BuilderPhase> {
    let s = v.as_str().ok_or_else(|| EvalError::Type {
        expected: "symbol (phase name)".into(),
        found: v.type_name().into(),
    })?;
    Ok(match s {
        "Unpack" => BuilderPhase::Unpack,
        "Patch" => BuilderPhase::Patch,
        "Configure" => BuilderPhase::Configure,
        "Build" => BuilderPhase::Build,
        "Check" => BuilderPhase::Check,
        "Install" => BuilderPhase::Install,
        "Fixup" => BuilderPhase::Fixup,
        "InstallCheck" => BuilderPhase::InstallCheck,
        "Dist" => BuilderPhase::Dist,
        other => {
            return Err(EvalError::Malformed {
                form: "builder".into(),
                reason: format!("unknown phase: {other}"),
            })
        }
    })
}

fn outputs_from_attrs(m: &BTreeMap<String, Value>) -> Outputs {
    Outputs {
        primary: optional_str(m, "primary")
            .map(str::to_string)
            .unwrap_or_else(|| "out".into()),
        extra: match m.get("extra") {
            Some(Value::List(xs)) => xs
                .iter()
                .filter_map(Value::coerce_to_string)
                .collect(),
            _ => vec![],
        },
    }
}

fn env_var_from_value(v: &Value) -> Result<EnvVar> {
    match v {
        Value::Attrs(m) => Ok(EnvVar {
            name: require_str(m, "name")?.to_string(),
            value: require_str(m, "value")?.to_string(),
        }),
        v => Err(EvalError::Type {
            expected: "attrs".into(),
            found: v.type_name().into(),
        }),
    }
}

fn sandbox_from_attrs(m: &BTreeMap<String, Value>) -> Sandbox {
    Sandbox {
        allow_network: optional_bool(m, "allow-network").unwrap_or(false),
        extra_paths: match m.get("extra-paths") {
            Some(Value::List(xs)) => xs
                .iter()
                .filter_map(Value::coerce_to_string)
                .collect(),
            _ => vec![],
        },
        impure_env: match m.get("impure-env") {
            Some(Value::List(xs)) => xs
                .iter()
                .filter_map(Value::coerce_to_string)
                .collect(),
            _ => vec![],
        },
    }
}

// ── helpers ─────────────────────────────────────────────────────────────

fn require_str<'a>(m: &'a BTreeMap<String, Value>, key: &str) -> Result<&'a str> {
    match m.get(key) {
        Some(v) => v.as_str().ok_or_else(|| EvalError::Type {
            expected: format!("string (for {key})"),
            found: v.type_name().into(),
        }),
        None => Err(EvalError::MissingAttr(key.into())),
    }
}

fn optional_str<'a>(m: &'a BTreeMap<String, Value>, key: &str) -> Option<&'a str> {
    m.get(key).and_then(Value::as_str)
}

fn optional_bool(m: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match m.get(key) {
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}
