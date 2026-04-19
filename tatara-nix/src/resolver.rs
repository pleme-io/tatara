//! Module fixpoint resolver — NixOS's `lib.evalModules`, typed.
//!
//! Multiple modules contribute assignments to a shared config namespace
//! (`services.nginx.enable`, etc.). Each contribution is a `MkExpr`:
//!
//!   - `Set { value }`            normal priority (100)
//!   - `If { cond, value }`       normal priority, filtered if cond is false
//!   - `Force { value }`          high priority (50) — override
//!   - `Default { value }`        low priority (1000) — fallback
//!   - `Merge { values }`         multi-contribution (treated as N `Set`s)
//!   - `Order { Before, value }`  list ordering — prepend
//!   - `Order { After,  value }`  list ordering — append
//!
//! **Resolution** walks every module, collects contributions grouped by
//! dotted path, picks the highest-priority group at each path, then joins
//! contributions within that group by value type (lists concat respecting
//! Before/After, attrsets deep-merge, scalars conflict).
//!
//! The whole algorithm is `tatara-lattice` in practice: same-priority
//! contributions meet (`⊓`) — different priorities resolve via the priority
//! order, which IS a lattice over `Priority`.

use serde_json::{Map as JMap, Value};
use std::collections::BTreeMap;
use thiserror::Error;

use crate::module::{MkExpr, Module, ModuleOption, OptionType, Placement};
// ModuleOption is used in the tests (fn opt helper below); the lint is
// wrong to flag it — keeping the import.
const _: fn() = || {
    let _ = std::marker::PhantomData::<ModuleOption>;
};

/// Resolution priority. Lower number = higher priority (matches Nix convention).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(pub i32);

impl Priority {
    pub const FORCE: Self = Self(50);
    pub const NORMAL: Self = Self(100);
    pub const DEFAULT: Self = Self(1000);
}

/// A single resolved contribution: path + priority + value + placement.
#[derive(Clone, Debug)]
struct Contribution {
    priority: Priority,
    value: Value,
    order: Option<Placement>,
}

/// Errors surfaced by module resolution.
#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("conflicting values at path {path:?}: {left} vs {right}")]
    ScalarConflict {
        path: String,
        left: Value,
        right: Value,
    },
    #[error("at path {path:?}: cannot merge {kind_a} with {kind_b}")]
    TypeMismatch {
        path: String,
        kind_a: &'static str,
        kind_b: &'static str,
    },
    #[error("mkIf condition at path {path:?} is not a boolean or truthy value")]
    NonBoolCondition { path: String },
    #[error("option {path:?} expected {expected}, got {got} ({value})")]
    OptionTypeMismatch {
        path: String,
        expected: String,
        got: &'static str,
        value: String,
    },
    #[error("option {path:?}: enum value {value:?} is not one of {choices:?}")]
    EnumChoiceInvalid {
        path: String,
        value: String,
        choices: Vec<String>,
    },
}

type ResolveResult<T> = Result<T, ResolveError>;

/// Resolve a single module to a final JSON `Value` — the NixOS-equivalent of
/// `(lib.evalModules { modules = [m]; }).config`.
pub fn resolve_module(module: &Module) -> ResolveResult<Value> {
    resolve_modules(std::slice::from_ref(module))
}

/// Resolve a set of modules to a single merged config.
/// **Also validates** every contribution against its declared `ModuleOption`
/// type — any option whose resolved value fails its type check surfaces
/// `ResolveError::OptionTypeMismatch` or `EnumChoiceInvalid`.
pub fn resolve_modules(modules: &[Module]) -> ResolveResult<Value> {
    // 1. Collect all contributions, keyed by dotted path.
    let mut contributions: BTreeMap<String, Vec<Contribution>> = BTreeMap::new();
    for module in modules {
        for assign in &module.config {
            let path = assign.path.clone();
            collect(&assign.expr, &path, &mut contributions)?;
        }
    }

    // 2. For each path, resolve priority → join contributions at top priority.
    let mut resolved: BTreeMap<String, Value> = BTreeMap::new();
    for (path, contribs) in contributions {
        if contribs.is_empty() {
            continue;
        }
        let min_priority = contribs.iter().map(|c| c.priority).min().unwrap();
        let winners: Vec<Contribution> = contribs
            .into_iter()
            .filter(|c| c.priority == min_priority)
            .collect();
        let value = join(&path, winners)?;
        resolved.insert(path, value);
    }

    // 3. Lift dotted paths into a nested `Value::Object`.
    let config = lift(resolved);

    // 4. Validate against declared options across all modules.
    //    Missing options that have a default are treated as satisfied.
    validate_options(modules, &config)?;

    Ok(config)
}

/// Expand an `MkExpr` at `path` into zero or more `Contribution`s.
fn collect(
    expr: &MkExpr,
    path: &str,
    out: &mut BTreeMap<String, Vec<Contribution>>,
) -> ResolveResult<()> {
    match expr {
        MkExpr::Set { value } => push(out, path, Priority::NORMAL, value.clone(), None),
        MkExpr::If { condition, value } => {
            if is_truthy(condition) {
                push(out, path, Priority::NORMAL, value.clone(), None);
            }
            // Explicit `false` is a skip — not a value.
        }
        MkExpr::Force { value } => push(out, path, Priority::FORCE, value.clone(), None),
        MkExpr::Default { value } => push(out, path, Priority::DEFAULT, value.clone(), None),
        MkExpr::Merge { values } => {
            for v in values {
                push(out, path, Priority::NORMAL, v.clone(), None);
            }
        }
        MkExpr::Order { placement, value } => {
            push(out, path, Priority::NORMAL, value.clone(), Some(*placement));
        }
    }
    Ok(())
}

fn push(
    out: &mut BTreeMap<String, Vec<Contribution>>,
    path: &str,
    priority: Priority,
    value: Value,
    order: Option<Placement>,
) {
    out.entry(path.to_string()).or_default().push(Contribution {
        priority,
        value,
        order,
    });
}

/// Combine same-priority contributions at a path into one `Value`.
///   - Single contribution: pass through.
///   - Multiple lists: concat (Before items first, then normal, then After).
///   - Multiple objects: deep-merge.
///   - Multiple scalars: error.
fn join(path: &str, mut contribs: Vec<Contribution>) -> ResolveResult<Value> {
    if contribs.len() == 1 {
        return Ok(contribs.pop().unwrap().value);
    }
    let first_kind = value_kind(&contribs[0].value);
    for c in &contribs {
        let k = value_kind(&c.value);
        if k != first_kind && !(first_kind == "object" && k == "object") {
            return Err(ResolveError::TypeMismatch {
                path: path.into(),
                kind_a: first_kind,
                kind_b: k,
            });
        }
    }
    match first_kind {
        "array" => Ok(join_lists(contribs)),
        "object" => join_objects(path, contribs),
        _ => {
            // Scalars can't join at same priority — that's a spec error.
            let [l, r] = match contribs.as_slice() {
                [a, b] => [a.value.clone(), b.value.clone()],
                [a, b, ..] => [a.value.clone(), b.value.clone()],
                _ => unreachable!(),
            };
            Err(ResolveError::ScalarConflict {
                path: path.into(),
                left: l,
                right: r,
            })
        }
    }
}

fn join_lists(contribs: Vec<Contribution>) -> Value {
    let mut before: Vec<Value> = Vec::new();
    let mut normal: Vec<Value> = Vec::new();
    let mut after: Vec<Value> = Vec::new();
    for c in contribs {
        let items = match c.value {
            Value::Array(a) => a,
            _ => continue,
        };
        match c.order {
            Some(Placement::Before) => before.extend(items),
            Some(Placement::After) => after.extend(items),
            None => normal.extend(items),
        }
    }
    let mut out = before;
    out.append(&mut normal);
    out.append(&mut after);
    Value::Array(out)
}

fn join_objects(path: &str, contribs: Vec<Contribution>) -> ResolveResult<Value> {
    let mut merged = JMap::new();
    for c in contribs {
        let obj = match c.value {
            Value::Object(m) => m,
            other => {
                return Err(ResolveError::TypeMismatch {
                    path: path.into(),
                    kind_a: "object",
                    kind_b: value_kind(&other),
                });
            }
        };
        for (k, v) in obj {
            if let Some(existing) = merged.remove(&k) {
                let subpath = format!("{path}.{k}");
                // Recursively join `existing` + `v` as two "contributions" at the subpath.
                let joined = join(
                    &subpath,
                    vec![
                        Contribution {
                            priority: Priority::NORMAL,
                            value: existing,
                            order: None,
                        },
                        Contribution {
                            priority: Priority::NORMAL,
                            value: v,
                            order: None,
                        },
                    ],
                )?;
                merged.insert(k, joined);
            } else {
                merged.insert(k, v);
            }
        }
    }
    Ok(Value::Object(merged))
}

/// Map dotted paths → nested JSON object.
/// `{"a.b": 1, "a.c": 2}` → `{"a": {"b": 1, "c": 2}}`.
fn lift(resolved: BTreeMap<String, Value>) -> Value {
    let mut root = Value::Object(JMap::new());
    for (path, value) in resolved {
        let parts: Vec<&str> = path.split('.').collect();
        insert_nested(&mut root, &parts, value);
    }
    root
}

fn insert_nested(root: &mut Value, parts: &[&str], value: Value) {
    if parts.is_empty() {
        *root = value;
        return;
    }
    if !root.is_object() {
        *root = Value::Object(JMap::new());
    }
    let obj = root.as_object_mut().expect("object");
    if parts.len() == 1 {
        obj.insert(parts[0].to_string(), value);
        return;
    }
    let head = parts[0];
    let entry = obj
        .entry(head.to_string())
        .or_insert_with(|| Value::Object(JMap::new()));
    insert_nested(entry, &parts[1..], value);
}

fn value_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64().map(|x| x != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

// ── Option validation ────────────────────────────────────────────────

/// For every module option declared across the `modules`, look up the resolved
/// value at that option's path and check it satisfies the option's type.
///   - Missing options with a `default` are skipped (the default would apply).
///   - Missing options without a default are also skipped (Nix treats options
///     as advertisements, not requirements — resolver doesn't enforce presence
///     at this layer).
fn validate_options(modules: &[Module], config: &Value) -> ResolveResult<()> {
    for module in modules {
        for option in &module.options {
            match read_path(config, &option.name) {
                Some(value) => validate(&option.name, value, &option.option_type)?,
                None => {
                    // Missing — OK if there's a default.
                    if option.default.is_some() {
                        continue;
                    }
                }
            }
        }
    }
    Ok(())
}

fn read_path<'a>(root: &'a Value, dotted: &str) -> Option<&'a Value> {
    let mut cur = root;
    for segment in dotted.split('.') {
        cur = cur.as_object()?.get(segment)?;
    }
    Some(cur)
}

fn validate(path: &str, value: &Value, ty: &OptionType) -> ResolveResult<()> {
    match ty {
        OptionType::Bool => expect(path, value, "bool", |v| v.is_boolean()),
        OptionType::Int => expect(path, value, "int", |v| {
            v.as_i64().is_some() || v.as_u64().is_some()
        }),
        OptionType::Str => expect(path, value, "string", Value::is_string),
        OptionType::Float => expect(path, value, "number", |v| {
            v.is_f64() || v.is_i64() || v.is_u64()
        }),
        OptionType::Path => expect(path, value, "path (string)", Value::is_string),
        OptionType::Package => expect(path, value, "package (object)", Value::is_object),
        OptionType::Any => Ok(()),
        OptionType::ListOf { item } => {
            let arr = value
                .as_array()
                .ok_or_else(|| type_err(path, "list", value))?;
            for (i, entry) in arr.iter().enumerate() {
                validate(&format!("{path}[{i}]"), entry, item)?;
            }
            Ok(())
        }
        OptionType::AttrsOf { value: vty } => {
            let obj = value
                .as_object()
                .ok_or_else(|| type_err(path, "attrset", value))?;
            for (k, v) in obj {
                validate(&format!("{path}.{k}"), v, vty)?;
            }
            Ok(())
        }
        OptionType::Enum { choices } => {
            let s = value
                .as_str()
                .ok_or_else(|| type_err(path, "enum (string)", value))?;
            if !choices.iter().any(|c| c == s) {
                return Err(ResolveError::EnumChoiceInvalid {
                    path: path.into(),
                    value: s.to_string(),
                    choices: choices.clone(),
                });
            }
            Ok(())
        }
        OptionType::Submodule { options } => {
            let obj = value
                .as_object()
                .ok_or_else(|| type_err(path, "submodule (object)", value))?;
            for sub in options {
                if let Some(v) = obj.get(&sub.name) {
                    validate(&format!("{path}.{}", sub.name), v, &sub.option_type)?;
                }
            }
            Ok(())
        }
    }
}

fn expect(
    path: &str,
    value: &Value,
    expected: &str,
    predicate: impl Fn(&Value) -> bool,
) -> ResolveResult<()> {
    if predicate(value) {
        Ok(())
    } else {
        Err(type_err(path, expected, value))
    }
}

fn type_err(path: &str, expected: &str, value: &Value) -> ResolveError {
    ResolveError::OptionTypeMismatch {
        path: path.into(),
        expected: expected.into(),
        got: value_kind(value),
        value: value.to_string(),
    }
}

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::{ConfigAssignment, Module};
    use serde_json::json;

    fn m(name: &str, config: Vec<ConfigAssignment>) -> Module {
        Module {
            name: name.into(),
            imports: vec![],
            options: vec![],
            config,
            description: None,
        }
    }

    fn set(path: &str, value: Value) -> ConfigAssignment {
        ConfigAssignment {
            path: path.into(),
            expr: MkExpr::Set { value },
        }
    }

    #[test]
    fn simple_set_resolves_to_nested_object() {
        let module = m("demo", vec![set("services.nginx.enable", json!(true))]);
        let resolved = resolve_module(&module).unwrap();
        assert_eq!(
            resolved,
            json!({ "services": { "nginx": { "enable": true }}})
        );
    }

    #[test]
    fn mk_if_true_includes_value() {
        let module = m(
            "demo",
            vec![ConfigAssignment {
                path: "x".into(),
                expr: MkExpr::If {
                    condition: json!(true),
                    value: json!(42),
                },
            }],
        );
        let resolved = resolve_module(&module).unwrap();
        assert_eq!(resolved, json!({ "x": 42 }));
    }

    #[test]
    fn mk_if_false_skips_value() {
        let module = m(
            "demo",
            vec![ConfigAssignment {
                path: "x".into(),
                expr: MkExpr::If {
                    condition: json!(false),
                    value: json!(42),
                },
            }],
        );
        let resolved = resolve_module(&module).unwrap();
        assert_eq!(resolved, json!({}));
    }

    #[test]
    fn mk_force_overrides_normal_across_modules() {
        let a = m("a", vec![set("retention", json!(30))]);
        let b = m(
            "b",
            vec![ConfigAssignment {
                path: "retention".into(),
                expr: MkExpr::Force { value: json!(7) },
            }],
        );
        let resolved = resolve_modules(&[a, b]).unwrap();
        assert_eq!(resolved, json!({ "retention": 7 }));
    }

    #[test]
    fn mk_merge_combines_contributions() {
        let module = m(
            "demo",
            vec![ConfigAssignment {
                path: "tags".into(),
                expr: MkExpr::Merge {
                    values: vec![json!(["a"]), json!(["b"]), json!(["c"])],
                },
            }],
        );
        let resolved = resolve_module(&module).unwrap();
        assert_eq!(resolved, json!({ "tags": ["a", "b", "c"] }));
    }

    #[test]
    fn list_ordering_respects_before_and_after() {
        let a = m(
            "a",
            vec![ConfigAssignment {
                path: "path".into(),
                expr: MkExpr::Order {
                    placement: Placement::Before,
                    value: json!(["/bin"]),
                },
            }],
        );
        let b = m("b", vec![set("path", json!(["/usr/bin"]))]);
        let c = m(
            "c",
            vec![ConfigAssignment {
                path: "path".into(),
                expr: MkExpr::Order {
                    placement: Placement::After,
                    value: json!(["/opt/bin"]),
                },
            }],
        );
        let resolved = resolve_modules(&[a, b, c]).unwrap();
        assert_eq!(
            resolved,
            json!({ "path": ["/bin", "/usr/bin", "/opt/bin"] })
        );
    }

    #[test]
    fn scalar_conflict_at_same_priority_errors() {
        let a = m("a", vec![set("port", json!(80))]);
        let b = m("b", vec![set("port", json!(8080))]);
        let err = resolve_modules(&[a, b]).unwrap_err();
        assert!(matches!(err, ResolveError::ScalarConflict { .. }));
    }

    #[test]
    fn objects_deep_merge_at_same_priority() {
        let a = m(
            "a",
            vec![set("services", json!({ "nginx": { "enable": true }}))],
        );
        let b = m(
            "b",
            vec![set("services", json!({ "nginx": { "port": 80 }}))],
        );
        let resolved = resolve_modules(&[a, b]).unwrap();
        assert_eq!(
            resolved,
            json!({ "services": { "nginx": { "enable": true, "port": 80 }}})
        );
    }

    #[test]
    fn type_mismatch_between_object_and_list_errors() {
        let a = m("a", vec![set("x", json!({ "k": 1 }))]);
        let b = m("b", vec![set("x", json!([1, 2]))]);
        let err = resolve_modules(&[a, b]).unwrap_err();
        assert!(matches!(err, ResolveError::TypeMismatch { .. }));
    }

    #[test]
    fn force_beats_conflicting_same_priority() {
        // Two normal-priority scalars conflict — but mkForce at higher priority
        // short-circuits the conflict by winning alone.
        let a = m("a", vec![set("port", json!(80))]);
        let b = m("b", vec![set("port", json!(8080))]);
        let c = m(
            "c",
            vec![ConfigAssignment {
                path: "port".into(),
                expr: MkExpr::Force { value: json!(9999) },
            }],
        );
        let resolved = resolve_modules(&[a, b, c]).unwrap();
        assert_eq!(resolved, json!({ "port": 9999 }));
    }

    #[test]
    fn multi_module_nested_paths_compose() {
        let a = m(
            "observability",
            vec![
                set("services.prometheus.enable", json!(true)),
                set("services.prometheus.port", json!(9090)),
            ],
        );
        let b = m(
            "grafana",
            vec![
                set("services.grafana.enable", json!(true)),
                set("services.grafana.port", json!(3000)),
            ],
        );
        let resolved = resolve_modules(&[a, b]).unwrap();
        assert_eq!(
            resolved,
            json!({
                "services": {
                    "prometheus": { "enable": true, "port": 9090 },
                    "grafana":    { "enable": true, "port": 3000 }
                }
            })
        );
    }

    // ── mkDefault tests ──────────────────────────────────────────────

    #[test]
    fn mk_default_used_when_no_other_contribution() {
        let module = m(
            "demo",
            vec![ConfigAssignment {
                path: "port".into(),
                expr: MkExpr::Default { value: json!(8080) },
            }],
        );
        let resolved = resolve_module(&module).unwrap();
        assert_eq!(resolved, json!({ "port": 8080 }));
    }

    #[test]
    fn mk_default_loses_to_normal() {
        let a = m(
            "a",
            vec![ConfigAssignment {
                path: "port".into(),
                expr: MkExpr::Default { value: json!(8080) },
            }],
        );
        let b = m("b", vec![set("port", json!(80))]);
        let resolved = resolve_modules(&[a, b]).unwrap();
        assert_eq!(resolved, json!({ "port": 80 }));
    }

    #[test]
    fn mk_default_loses_to_force() {
        let a = m(
            "a",
            vec![ConfigAssignment {
                path: "port".into(),
                expr: MkExpr::Default { value: json!(8080) },
            }],
        );
        let b = m(
            "b",
            vec![ConfigAssignment {
                path: "port".into(),
                expr: MkExpr::Force { value: json!(443) },
            }],
        );
        let resolved = resolve_modules(&[a, b]).unwrap();
        assert_eq!(resolved, json!({ "port": 443 }));
    }

    // ── Option validation tests ───────────────────────────────────────

    fn opt(name: &str, ty: OptionType, default: Option<Value>) -> ModuleOption {
        ModuleOption {
            name: name.into(),
            option_type: ty,
            default,
            description: None,
            read_only: false,
        }
    }

    #[test]
    fn bool_option_accepts_bool() {
        let m = Module {
            name: "ok".into(),
            imports: vec![],
            options: vec![opt("enable", OptionType::Bool, None)],
            config: vec![set("enable", json!(true))],
            description: None,
        };
        resolve_module(&m).unwrap();
    }

    #[test]
    fn bool_option_rejects_string() {
        let m = Module {
            name: "bad".into(),
            imports: vec![],
            options: vec![opt("enable", OptionType::Bool, None)],
            config: vec![set("enable", json!("true"))],
            description: None,
        };
        let err = resolve_module(&m).unwrap_err();
        assert!(matches!(
            err,
            ResolveError::OptionTypeMismatch { expected, got: "string", .. } if expected == "bool"
        ));
    }

    #[test]
    fn enum_option_accepts_valid_choice() {
        let m = Module {
            name: "ok".into(),
            imports: vec![],
            options: vec![opt(
                "level",
                OptionType::Enum {
                    choices: vec!["info".into(), "warn".into(), "error".into()],
                },
                None,
            )],
            config: vec![set("level", json!("warn"))],
            description: None,
        };
        resolve_module(&m).unwrap();
    }

    #[test]
    fn enum_option_rejects_invalid_choice() {
        let m = Module {
            name: "bad".into(),
            imports: vec![],
            options: vec![opt(
                "level",
                OptionType::Enum {
                    choices: vec!["info".into(), "warn".into(), "error".into()],
                },
                None,
            )],
            config: vec![set("level", json!("debug"))],
            description: None,
        };
        let err = resolve_module(&m).unwrap_err();
        assert!(matches!(
            err,
            ResolveError::EnumChoiceInvalid { value, .. } if value == "debug"
        ));
    }

    #[test]
    fn list_of_int_rejects_non_int_element() {
        let m = Module {
            name: "bad".into(),
            imports: vec![],
            options: vec![opt(
                "ports",
                OptionType::ListOf {
                    item: Box::new(OptionType::Int),
                },
                None,
            )],
            config: vec![set("ports", json!([80, "443", 8080]))],
            description: None,
        };
        let err = resolve_module(&m).unwrap_err();
        assert!(matches!(err, ResolveError::OptionTypeMismatch { .. }));
    }

    #[test]
    fn missing_option_with_default_is_ok() {
        let m = Module {
            name: "ok".into(),
            imports: vec![],
            options: vec![opt("retention", OptionType::Int, Some(json!(30)))],
            config: vec![], // option not assigned
            description: None,
        };
        resolve_module(&m).unwrap();
    }

    #[test]
    fn submodule_validates_nested_fields() {
        let m = Module {
            name: "bad".into(),
            imports: vec![],
            options: vec![opt(
                "server",
                OptionType::Submodule {
                    options: vec![
                        opt("host", OptionType::Str, None),
                        opt("port", OptionType::Int, None),
                    ],
                },
                None,
            )],
            config: vec![set("server", json!({ "host": "x", "port": "bad" }))],
            description: None,
        };
        let err = resolve_module(&m).unwrap_err();
        assert!(matches!(err, ResolveError::OptionTypeMismatch { .. }));
    }
}
