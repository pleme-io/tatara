//! Tree-walking interpreter — `Sexp` → `Value`.
//!
//! Minimal but complete enough to evaluate real Lisp programs that produce
//! typed `Derivation` values. Special forms handled inline (to respect
//! evaluation order / lazy arms); everything else dispatches via `apply` over
//! evaluated arguments.
//!
//! Special forms:
//!   - `quote`   — return the literal Sexp as Value
//!   - `if`      — short-circuit branches
//!   - `let`     — non-recursive bindings
//!   - `letrec`  — recursive bindings via thunks
//!   - `lambda`  — closure capture
//!   - `define`  — bind in top-level env
//!   - `begin`   — sequential evaluation
//!   - `set!`    — mutate a top-level binding (restricted; see docs)

use std::collections::BTreeMap;
use std::sync::Arc;

use tatara_lisp::{read, Atom, Sexp};

use crate::builtins::builtin_table;
use crate::env::Env;
use crate::error::{EvalError, Result};
use crate::value::{Lambda, Thunk, ThunkState, Value};

pub struct Interpreter {
    /// Root environment — seeded with builtins, mutated by `define`.
    root: std::sync::RwLock<Env>,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub fn new() -> Self {
        let mut env = Env::new();
        for (name, value) in builtin_table() {
            env = env.extend(name, value);
        }
        Self {
            root: std::sync::RwLock::new(env),
        }
    }

    /// Evaluate a single source string. Returns the value of the last form.
    pub fn eval_source(&self, src: &str) -> Result<Value> {
        let forms = read(src)?;
        self.eval_forms(&forms)
    }

    pub fn eval_forms(&self, forms: &[Sexp]) -> Result<Value> {
        let mut last = Value::Nil;
        for f in forms {
            last = self.eval(f, &self.root_env())?;
        }
        Ok(last)
    }

    pub fn root_env(&self) -> Env {
        self.root.read().unwrap().clone()
    }

    /// Install a binding in the top-level environment.
    pub fn define(&self, name: impl Into<String>, value: Value) {
        let mut root = self.root.write().unwrap();
        *root = root.extend(name, value);
    }

    // ── evaluator ──────────────────────────────────────────────────────

    pub fn eval(&self, s: &Sexp, env: &Env) -> Result<Value> {
        match s {
            Sexp::Nil => Ok(Value::Nil),
            Sexp::Atom(a) => self.eval_atom(a, env),
            Sexp::Quote(inner) => Ok(self.sexp_to_value(inner)),
            Sexp::Quasiquote(inner) => self.eval_quasiquote(inner, env),
            Sexp::Unquote(_) | Sexp::UnquoteSplice(_) => Err(EvalError::Malformed {
                form: "unquote".into(),
                reason: "unquote outside quasiquote".into(),
            }),
            Sexp::List(items) => {
                if items.is_empty() {
                    return Ok(Value::Nil);
                }
                // Check for special forms before evaluating the head.
                if let Some(head) = items[0].as_symbol() {
                    match head {
                        "quote" => return self.sf_quote(items),
                        "if" => return self.sf_if(items, env),
                        "let" => return self.sf_let(items, env),
                        "letrec" => return self.sf_letrec(items, env),
                        "lambda" | "fn" => return self.sf_lambda(items, env),
                        "define" => return self.sf_define(items, env),
                        "begin" | "do" => return self.sf_begin(items, env),
                        "and" => return self.sf_and(items, env),
                        "or" => return self.sf_or(items, env),
                        _ => {}
                    }
                }
                // Function application: evaluate head + args, apply.
                let head_val = self.eval(&items[0], env)?;
                let mut args = Vec::with_capacity(items.len() - 1);
                for a in &items[1..] {
                    args.push(self.eval(a, env)?);
                }
                self.apply(&head_val, &args)
            }
        }
    }

    fn eval_atom(&self, a: &Atom, env: &Env) -> Result<Value> {
        Ok(match a {
            Atom::Int(n) => Value::Int(*n),
            Atom::Float(n) => Value::Float(*n),
            Atom::Str(s) => Value::Str(s.clone()),
            Atom::Bool(b) => Value::Bool(*b),
            Atom::Keyword(k) => Value::Keyword(k.clone()),
            Atom::Symbol(name) => env
                .lookup(name)
                .ok_or_else(|| EvalError::Unbound(name.clone()))?,
        })
    }

    fn sexp_to_value(&self, s: &Sexp) -> Value {
        match s {
            Sexp::Nil => Value::Nil,
            Sexp::Atom(a) => match a {
                Atom::Int(n) => Value::Int(*n),
                Atom::Float(n) => Value::Float(*n),
                Atom::Str(s) => Value::Str(s.clone()),
                Atom::Bool(b) => Value::Bool(*b),
                Atom::Keyword(k) => Value::Keyword(k.clone()),
                Atom::Symbol(name) => Value::Symbol(name.clone()),
            },
            Sexp::List(xs) => Value::List(Arc::new(
                xs.iter().map(|x| self.sexp_to_value(x)).collect(),
            )),
            other => Value::Quoted(Arc::new(other.clone())),
        }
    }

    fn eval_quasiquote(&self, inner: &Sexp, env: &Env) -> Result<Value> {
        match inner {
            Sexp::Unquote(x) => self.eval(x, env),
            Sexp::List(xs) => {
                let mut out = Vec::with_capacity(xs.len());
                for item in xs {
                    match item {
                        Sexp::UnquoteSplice(x) => {
                            let v = self.eval(x, env)?;
                            match v {
                                Value::List(items) => {
                                    out.extend(items.iter().cloned());
                                }
                                other => {
                                    return Err(EvalError::Type {
                                        expected: "list (for ,@)".into(),
                                        found: other.type_name().into(),
                                    })
                                }
                            }
                        }
                        other => out.push(self.eval_quasiquote(other, env)?),
                    }
                }
                Ok(Value::List(Arc::new(out)))
            }
            Sexp::Atom(_) | Sexp::Nil => Ok(self.sexp_to_value(inner)),
            Sexp::Quote(x) | Sexp::Quasiquote(x) => {
                Ok(Value::Quoted(Arc::new((**x).clone())))
            }
            Sexp::UnquoteSplice(_) => Err(EvalError::Malformed {
                form: "quasiquote".into(),
                reason: "bare ,@ outside of list".into(),
            }),
        }
    }

    // ── special forms ──────────────────────────────────────────────────

    fn sf_quote(&self, items: &[Sexp]) -> Result<Value> {
        if items.len() != 2 {
            return Err(EvalError::Malformed {
                form: "quote".into(),
                reason: "expected (quote x)".into(),
            });
        }
        Ok(self.sexp_to_value(&items[1]))
    }

    fn sf_if(&self, items: &[Sexp], env: &Env) -> Result<Value> {
        if items.len() < 3 || items.len() > 4 {
            return Err(EvalError::Malformed {
                form: "if".into(),
                reason: "expected (if c t [e])".into(),
            });
        }
        let cond = self.eval(&items[1], env)?;
        if cond.is_truthy() {
            self.eval(&items[2], env)
        } else if items.len() == 4 {
            self.eval(&items[3], env)
        } else {
            Ok(Value::Nil)
        }
    }

    fn sf_let(&self, items: &[Sexp], env: &Env) -> Result<Value> {
        // (let ((a 1) (b 2)) body ...)
        if items.len() < 3 {
            return Err(EvalError::Malformed {
                form: "let".into(),
                reason: "expected (let ((name val)...) body...)".into(),
            });
        }
        let bindings = items[1].as_list().ok_or_else(|| EvalError::Malformed {
            form: "let".into(),
            reason: "bindings must be a list".into(),
        })?;
        let mut new_env = env.clone();
        for b in bindings {
            let pair = b.as_list().ok_or_else(|| EvalError::Malformed {
                form: "let".into(),
                reason: "each binding must be (name val)".into(),
            })?;
            if pair.len() != 2 {
                return Err(EvalError::Malformed {
                    form: "let".into(),
                    reason: "each binding must be (name val)".into(),
                });
            }
            let name = pair[0].as_symbol().ok_or_else(|| EvalError::Malformed {
                form: "let".into(),
                reason: "binding name must be a symbol".into(),
            })?;
            let value = self.eval(&pair[1], env)?;
            new_env = new_env.extend(name, value);
        }
        self.eval_body(&items[2..], &new_env)
    }

    fn sf_letrec(&self, items: &[Sexp], env: &Env) -> Result<Value> {
        if items.len() < 3 {
            return Err(EvalError::Malformed {
                form: "letrec".into(),
                reason: "expected (letrec ((name val)...) body...)".into(),
            });
        }
        let bindings = items[1].as_list().ok_or_else(|| EvalError::Malformed {
            form: "letrec".into(),
            reason: "bindings must be a list".into(),
        })?;
        // Pre-allocate thunks so bindings can reference each other.
        let mut names: Vec<String> = Vec::new();
        let mut exprs: Vec<Sexp> = Vec::new();
        for b in bindings {
            let pair = b.as_list().ok_or_else(|| EvalError::Malformed {
                form: "letrec".into(),
                reason: "each binding must be (name val)".into(),
            })?;
            let name = pair[0]
                .as_symbol()
                .ok_or_else(|| EvalError::Malformed {
                    form: "letrec".into(),
                    reason: "binding name must be a symbol".into(),
                })?
                .to_string();
            names.push(name);
            exprs.push(pair[1].clone());
        }
        // Build an env whose bindings are thunks pointing at the same frame
        // — naive approach: extend sequentially, force lazily in builtins.
        let mut new_env = env.clone();
        let thunks: Vec<Arc<Thunk>> = exprs
            .iter()
            .map(|e| Thunk::new(e.clone(), new_env.clone()))
            .collect();
        for (name, t) in names.iter().zip(&thunks) {
            new_env = new_env.extend(name.clone(), Value::Thunk(t.clone()));
        }
        // Re-parent each thunk's env to the fully-bound env so each can see
        // all its siblings.
        for t in &thunks {
            let mut state = t.cell.lock().unwrap();
            if let ThunkState::Unevaluated { env: e, .. } = &mut *state {
                *e = new_env.clone();
            }
        }
        self.eval_body(&items[2..], &new_env)
    }

    fn sf_lambda(&self, items: &[Sexp], env: &Env) -> Result<Value> {
        if items.len() < 3 {
            return Err(EvalError::Malformed {
                form: "lambda".into(),
                reason: "expected (lambda (params...) body...)".into(),
            });
        }
        let (params, rest) = parse_params(&items[1])?;
        let body = items[2..].to_vec();
        Ok(Value::Lambda(Arc::new(Lambda {
            params,
            rest,
            body,
            env: env.clone(),
            name: None,
        })))
    }

    fn sf_define(&self, items: &[Sexp], env: &Env) -> Result<Value> {
        if items.len() != 3 {
            return Err(EvalError::Malformed {
                form: "define".into(),
                reason: "expected (define name expr)".into(),
            });
        }
        let name = items[1]
            .as_symbol()
            .ok_or_else(|| EvalError::Malformed {
                form: "define".into(),
                reason: "name must be a symbol".into(),
            })?
            .to_string();
        let value = self.eval(&items[2], env)?;
        self.define(name.clone(), value.clone());
        Ok(value)
    }

    fn sf_begin(&self, items: &[Sexp], env: &Env) -> Result<Value> {
        self.eval_body(&items[1..], env)
    }

    fn sf_and(&self, items: &[Sexp], env: &Env) -> Result<Value> {
        let mut last = Value::Bool(true);
        for e in &items[1..] {
            last = self.eval(e, env)?;
            if !last.is_truthy() {
                return Ok(last);
            }
        }
        Ok(last)
    }

    fn sf_or(&self, items: &[Sexp], env: &Env) -> Result<Value> {
        for e in &items[1..] {
            let v = self.eval(e, env)?;
            if v.is_truthy() {
                return Ok(v);
            }
        }
        Ok(Value::Bool(false))
    }

    fn eval_body(&self, forms: &[Sexp], env: &Env) -> Result<Value> {
        let mut last = Value::Nil;
        for f in forms {
            last = self.eval(f, env)?;
        }
        Ok(last)
    }

    // ── application ────────────────────────────────────────────────────

    pub fn apply(&self, f: &Value, args: &[Value]) -> Result<Value> {
        // Force any thunk arguments before use? Nix is lazy; we stay strict at
        // call boundaries for now but force on use. Force here for ergonomics.
        let forced_args: Vec<Value> =
            args.iter().map(|v| self.force(v.clone())).collect::<Result<_>>()?;

        match self.force(f.clone())? {
            Value::Builtin(b) => {
                if !b.arity.check(forced_args.len()) {
                    return Err(EvalError::Arity {
                        name: b.name.clone(),
                        expected: b.arity.describe(),
                        got: forced_args.len(),
                    });
                }
                (b.func)(&forced_args)
            }
            Value::Lambda(l) => self.apply_lambda(&l, &forced_args),
            other => Err(EvalError::Type {
                expected: "callable".into(),
                found: other.type_name().into(),
            }),
        }
    }

    fn apply_lambda(&self, l: &Lambda, args: &[Value]) -> Result<Value> {
        let mut env = l.env.clone();
        match &l.rest {
            None => {
                if args.len() != l.params.len() {
                    return Err(EvalError::Arity {
                        name: l.name.clone().unwrap_or_else(|| "<lambda>".into()),
                        expected: format!("{}", l.params.len()),
                        got: args.len(),
                    });
                }
                for (p, a) in l.params.iter().zip(args.iter()) {
                    env = env.extend(p.clone(), a.clone());
                }
            }
            Some(rest_name) => {
                if args.len() < l.params.len() {
                    return Err(EvalError::Arity {
                        name: l.name.clone().unwrap_or_else(|| "<lambda>".into()),
                        expected: format!("at least {}", l.params.len()),
                        got: args.len(),
                    });
                }
                for (p, a) in l.params.iter().zip(args.iter()) {
                    env = env.extend(p.clone(), a.clone());
                }
                let rest = args[l.params.len()..].to_vec();
                env = env.extend(rest_name.clone(), Value::List(Arc::new(rest)));
            }
        }
        self.eval_body(&l.body, &env)
    }

    /// Force a thunk, or return the value unchanged if it already is one.
    pub fn force(&self, v: Value) -> Result<Value> {
        let t = match v {
            Value::Thunk(t) => t,
            other => return Ok(other),
        };
        let mut state = t.cell.lock().unwrap();
        match std::mem::replace(&mut *state, ThunkState::Evaluating) {
            ThunkState::Forced(v) => {
                *state = ThunkState::Forced(v.clone());
                Ok(v)
            }
            ThunkState::Evaluating => Err(EvalError::Other(
                "thunk cycle: forcing an in-progress thunk".into(),
            )),
            ThunkState::Unevaluated { body, env } => {
                drop(state);
                let v = self.eval(&body, &env)?;
                let v = self.force(v)?;
                *t.cell.lock().unwrap() = ThunkState::Forced(v.clone());
                Ok(v)
            }
        }
    }
}

fn parse_params(s: &Sexp) -> Result<(Vec<String>, Option<String>)> {
    let items = s.as_list().ok_or_else(|| EvalError::Malformed {
        form: "lambda".into(),
        reason: "params must be a list".into(),
    })?;
    let mut params = Vec::new();
    let mut rest = None;
    let mut saw_rest = false;
    for (i, p) in items.iter().enumerate() {
        let name = p.as_symbol().ok_or_else(|| EvalError::Malformed {
            form: "lambda".into(),
            reason: "param must be a symbol".into(),
        })?;
        if name == "&rest" || name == "&" {
            if i + 2 != items.len() {
                return Err(EvalError::Malformed {
                    form: "lambda".into(),
                    reason: "&rest must be followed by exactly one name".into(),
                });
            }
            saw_rest = true;
            continue;
        }
        if saw_rest {
            rest = Some(name.to_string());
            break;
        }
        params.push(name.to_string());
    }
    let _ = BTreeMap::<String, Value>::new(); // quell "unused" in future tweaks
    Ok((params, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_literals() {
        let i = Interpreter::new();
        assert!(matches!(i.eval_source("42").unwrap(), Value::Int(42)));
        assert!(matches!(i.eval_source("#t").unwrap(), Value::Bool(true)));
        match i.eval_source(r#""hi""#).unwrap() {
            Value::Str(s) => assert_eq!(s, "hi"),
            _ => panic!(),
        }
    }

    #[test]
    fn arithmetic() {
        let i = Interpreter::new();
        assert!(matches!(i.eval_source("(+ 1 2 3)").unwrap(), Value::Int(6)));
        assert!(matches!(i.eval_source("(* 2 3 4)").unwrap(), Value::Int(24)));
        assert!(matches!(i.eval_source("(- 10 3)").unwrap(), Value::Int(7)));
        assert!(matches!(i.eval_source("(- 5)").unwrap(), Value::Int(-5)));
    }

    #[test]
    fn if_and_booleans() {
        let i = Interpreter::new();
        assert!(matches!(
            i.eval_source("(if (< 1 2) 'yes 'no)").unwrap(),
            Value::Symbol(s) if s == "yes"
        ));
    }

    #[test]
    fn let_binds() {
        let i = Interpreter::new();
        let v = i.eval_source("(let ((x 10) (y 20)) (+ x y))").unwrap();
        assert!(matches!(v, Value::Int(30)));
    }

    #[test]
    fn lambda_and_apply() {
        let i = Interpreter::new();
        let v = i
            .eval_source("((lambda (x y) (+ x y)) 3 4)")
            .unwrap();
        assert!(matches!(v, Value::Int(7)));
    }

    #[test]
    fn closures_capture_env() {
        let i = Interpreter::new();
        let v = i
            .eval_source(
                "(let ((make-add (lambda (x) (lambda (y) (+ x y))))) \
                 ((make-add 3) 4))",
            )
            .unwrap();
        assert!(matches!(v, Value::Int(7)));
    }

    #[test]
    fn letrec_enables_mutual_reference() {
        let i = Interpreter::new();
        let v = i
            .eval_source(
                "(letrec ((even? (lambda (n) (if (= n 0) #t (odd? (- n 1))))) \
                          (odd?  (lambda (n) (if (= n 0) #f (even? (- n 1)))))) \
                   (even? 10))",
            )
            .unwrap();
        assert!(matches!(v, Value::Bool(true)));
    }

    #[test]
    fn quasiquote_and_unquote() {
        let i = Interpreter::new();
        let v = i.eval_source("(let ((x 5)) `(1 ,x 3))").unwrap();
        match v {
            Value::List(xs) => {
                assert_eq!(xs.len(), 3);
                assert!(matches!(xs[1], Value::Int(5)));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn string_append_and_tostring() {
        let i = Interpreter::new();
        let v = i
            .eval_source(r#"(string-append "hello-" (toString 42))"#)
            .unwrap();
        assert!(matches!(v, Value::Str(s) if s == "hello-42"));
    }

    #[test]
    fn derivation_builtin_produces_typed_value() {
        let i = Interpreter::new();
        let v = i
            .eval_source(
                r#"(derivation
                     (attrs
                       "name"    "hello"
                       "version" "1.0"))"#,
            )
            .unwrap();
        match v {
            Value::Derivation(d) => {
                assert_eq!(d.name, "hello");
                assert_eq!(d.version.as_deref(), Some("1.0"));
            }
            _ => panic!("expected Derivation, got {v:?}"),
        }
    }

    #[test]
    fn derivation_store_path_is_deterministic() {
        let i = Interpreter::new();
        let v1 = i
            .eval_source(r#"(derivation (attrs "name" "x" "version" "1"))"#)
            .unwrap();
        let v2 = i
            .eval_source(r#"(derivation (attrs "name" "x" "version" "1"))"#)
            .unwrap();
        let (Value::Derivation(a), Value::Derivation(b)) = (v1, v2) else {
            panic!("expected derivations");
        };
        assert_eq!(a.store_path(), b.store_path());
    }
}
