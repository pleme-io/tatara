//! Opt-in system builtins for the interpreter.
//!
//! Kept separate from the pure `builtins` module so a `Terreiro` used as a
//! sandbox stays I/O-free by default. `tatara-init --eval` and any other
//! host-side embedder calls `Interpreter::new_with_system()` to enable
//! these side-effecting primitives:
//!
//!   - `(print v)`         — write stringified v to stdout
//!   - `(println v)`       — print + newline
//!   - `(eprint v)`        — stderr
//!   - `(eprintln v)`      — stderr + newline
//!   - `(sleep seconds)`   — block for `seconds` (integer or float)
//!   - `(exit code)`       — std::process::exit
//!   - `(env name)`        — std::env::var
//!   - `(env-or name def)` — env with fallback
//!   - `(read-file path)`  — returns string
//!   - `(write-file path content)` — returns nil
//!   - `(shell cmd)`       — run `/bin/sh -c cmd`, return exit code as int
//!   - `(forever body…)`   — loop the body forever (no natural exit; SIGINT)
//!
//! `forever` is a special form: it doesn't eagerly evaluate its arguments
//! (otherwise the first invocation would wedge at eval time). Handled
//! inside the Interpreter's list dispatch, not here.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::{EvalError, Result};
use crate::value::{Arity, Builtin, BuiltinFn, Value};

pub fn system_builtin_table() -> BTreeMap<String, Value> {
    let mut m: BTreeMap<String, Value> = BTreeMap::new();
    for b in all_system_builtins() {
        let name = b.name.clone();
        m.insert(name, Value::Builtin(Arc::new(b)));
    }
    m
}

fn all_system_builtins() -> Vec<Builtin> {
    vec![
        mk("print", Arity::Exact(1), Arc::new(print_)),
        mk("println", Arity::Exact(1), Arc::new(println_)),
        mk("eprint", Arity::Exact(1), Arc::new(eprint_)),
        mk("eprintln", Arity::Exact(1), Arc::new(eprintln_)),
        mk("sleep", Arity::Exact(1), Arc::new(sleep_)),
        mk("exit", Arity::Exact(1), Arc::new(exit_)),
        mk("env", Arity::Exact(1), Arc::new(env_)),
        mk("env-or", Arity::Exact(2), Arc::new(env_or_)),
        mk("read-file", Arity::Exact(1), Arc::new(read_file_)),
        mk("write-file", Arity::Exact(2), Arc::new(write_file_)),
        mk("shell", Arity::Exact(1), Arc::new(shell_)),
    ]
}

fn mk(name: &str, arity: Arity, f: Arc<BuiltinFn>) -> Builtin {
    Builtin {
        name: name.into(),
        arity,
        func: f,
    }
}

fn to_display_string(v: &Value) -> String {
    v.coerce_to_string().unwrap_or_else(|| format!("{v:?}"))
}

fn print_(args: &[Value]) -> Result<Value> {
    print!("{}", to_display_string(&args[0]));
    use std::io::Write;
    let _ = std::io::stdout().flush();
    Ok(Value::Nil)
}

fn println_(args: &[Value]) -> Result<Value> {
    println!("{}", to_display_string(&args[0]));
    Ok(Value::Nil)
}

fn eprint_(args: &[Value]) -> Result<Value> {
    eprint!("{}", to_display_string(&args[0]));
    use std::io::Write;
    let _ = std::io::stderr().flush();
    Ok(Value::Nil)
}

fn eprintln_(args: &[Value]) -> Result<Value> {
    eprintln!("{}", to_display_string(&args[0]));
    Ok(Value::Nil)
}

fn sleep_(args: &[Value]) -> Result<Value> {
    let seconds = match &args[0] {
        Value::Int(n) if *n >= 0 => *n as f64,
        Value::Float(f) if *f >= 0.0 => *f,
        v => {
            return Err(EvalError::Type {
                expected: "non-negative int or float (seconds)".into(),
                found: v.type_name().into(),
            })
        }
    };
    let duration = std::time::Duration::from_secs_f64(seconds);
    std::thread::sleep(duration);
    Ok(Value::Nil)
}

fn exit_(args: &[Value]) -> Result<Value> {
    let code = args[0].as_int().unwrap_or(0) as i32;
    std::process::exit(code);
}

fn env_(args: &[Value]) -> Result<Value> {
    let name = args[0].as_str().ok_or_else(|| EvalError::Type {
        expected: "string".into(),
        found: args[0].type_name().into(),
    })?;
    match std::env::var(name) {
        Ok(v) => Ok(Value::Str(v)),
        Err(_) => Ok(Value::Nil),
    }
}

fn env_or_(args: &[Value]) -> Result<Value> {
    let name = args[0].as_str().ok_or_else(|| EvalError::Type {
        expected: "string (env var name)".into(),
        found: args[0].type_name().into(),
    })?;
    let default = args[1].clone();
    match std::env::var(name) {
        Ok(v) => Ok(Value::Str(v)),
        Err(_) => Ok(default),
    }
}

fn read_file_(args: &[Value]) -> Result<Value> {
    let path = args[0]
        .as_path()
        .map(|p| p.clone())
        .or_else(|| args[0].as_str().map(std::path::PathBuf::from))
        .ok_or_else(|| EvalError::Type {
            expected: "string or path".into(),
            found: args[0].type_name().into(),
        })?;
    let s = std::fs::read_to_string(&path)?;
    Ok(Value::Str(s))
}

fn write_file_(args: &[Value]) -> Result<Value> {
    let path = args[0]
        .as_path()
        .map(|p| p.clone())
        .or_else(|| args[0].as_str().map(std::path::PathBuf::from))
        .ok_or_else(|| EvalError::Type {
            expected: "string or path (write-file first arg)".into(),
            found: args[0].type_name().into(),
        })?;
    let content = args[1].coerce_to_string().ok_or_else(|| EvalError::Type {
        expected: "string-coercible".into(),
        found: args[1].type_name().into(),
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, content)?;
    Ok(Value::Nil)
}

fn shell_(args: &[Value]) -> Result<Value> {
    let cmd = args[0].as_str().ok_or_else(|| EvalError::Type {
        expected: "string".into(),
        found: args[0].type_name().into(),
    })?;
    let status = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(cmd)
        .status()?;
    Ok(Value::Int(status.code().unwrap_or(-1) as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_returns_nil_for_missing() {
        let r = env_(&[Value::Str(
            "TATARA_THIS_VAR_DEFINITELY_DOES_NOT_EXIST_12345".into(),
        )])
        .unwrap();
        assert!(matches!(r, Value::Nil));
    }

    #[test]
    fn env_or_returns_default_for_missing() {
        let r = env_or_(&[
            Value::Str("TATARA_MISSING_VAR_54321".into()),
            Value::Str("fallback".into()),
        ])
        .unwrap();
        assert!(matches!(r, Value::Str(s) if s == "fallback"));
    }

    #[test]
    fn read_file_and_write_file_round_trip() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("hi.txt");
        write_file_(&[
            Value::Str(path.to_string_lossy().into_owned()),
            Value::Str("hello lisp".into()),
        ])
        .unwrap();
        let r = read_file_(&[Value::Str(path.to_string_lossy().into_owned())]).unwrap();
        assert!(matches!(r, Value::Str(s) if s == "hello lisp"));
    }

    #[test]
    fn shell_returns_exit_code() {
        let r = shell_(&[Value::Str("true".into())]).unwrap();
        assert!(matches!(r, Value::Int(0)));
        let r = shell_(&[Value::Str("false".into())]).unwrap();
        assert!(matches!(r, Value::Int(n) if n != 0));
    }

    #[test]
    fn sleep_rejects_negative_seconds() {
        let r = sleep_(&[Value::Int(-5)]);
        assert!(r.is_err());
    }
}
