//! `tatara-check` — workspace coherence checker.
//!
//! Reads `checks.lisp` at the workspace root and runs each declared check.
//! Check list is data (Lisp); executors are typed Rust. No shell.
//!
//! Invoke: `cargo run --bin tatara-check -p tatara-reconciler`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kube::CustomResourceExt;
use tatara_lisp::{domain, read, Expander, Sexp};
use tatara_process::prelude::{Process, ProcessTable};

#[derive(Default)]
struct Report {
    passes: Vec<String>,
    failures: Vec<String>,
}

impl Report {
    fn pass(&mut self, label: impl Into<String>) {
        self.passes.push(label.into());
    }
    fn fail(&mut self, label: impl std::fmt::Display, detail: impl std::fmt::Display) {
        self.failures.push(format!("{label}: {detail}"));
    }
    fn is_ok(&self) -> bool {
        self.failures.is_empty()
    }
}

fn main() -> ExitCode {
    // Seed the global domain registry with example typed domains authored as Lisp.
    tatara_domains::register_all();

    let root = match workspace_root() {
        Some(r) => r,
        None => {
            eprintln!(
                "tatara-check: could not locate workspace root (looked for Cargo.toml + checks.lisp)"
            );
            return ExitCode::from(2);
        }
    };
    let checks_path = root.join("checks.lisp");
    let src = match fs::read_to_string(&checks_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("tatara-check: read {}: {e}", checks_path.display());
            return ExitCode::from(2);
        }
    };
    let raw = match read(&src) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("tatara-check: parse {}: {e}", checks_path.display());
            return ExitCode::from(2);
        }
    };

    // Tier 1: checks.lisp may contain (defcheck …) macros + macro calls.
    // Run the expander first so primitives authored via defcheck materialize
    // as primitive check forms the dispatcher understands.
    let mut expander = Expander::new();
    let forms = match expander.expand_program(raw) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("tatara-check: macroexpand: {e}");
            return ExitCode::from(2);
        }
    };

    let mut report = Report::default();
    if !expander.is_empty() {
        report.pass(format!(
            "macroexpander loaded {} user-defined check macro(s)",
            expander.len()
        ));
    }
    for form in &forms {
        dispatch(form, &root, &mut report);
    }

    for p in &report.passes {
        println!("✓ {p}");
    }
    for f in &report.failures {
        eprintln!("✗ {f}");
    }
    println!();
    if report.is_ok() {
        println!("{} checks passed", report.passes.len());
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "{} failures, {} passes",
            report.failures.len(),
            report.passes.len()
        );
        ExitCode::FAILURE
    }
}

/// Walk up from CWD looking for a directory that has both `Cargo.toml` and
/// `checks.lisp`. That's the workspace root.
fn workspace_root() -> Option<PathBuf> {
    let mut cur = std::env::current_dir().ok()?;
    loop {
        if cur.join("Cargo.toml").is_file() && cur.join("checks.lisp").is_file() {
            return Some(cur);
        }
        if !cur.pop() {
            return None;
        }
    }
}

fn dispatch(form: &Sexp, root: &Path, report: &mut Report) {
    let Some(list) = form.as_list() else {
        report.fail("dispatch", "top-level form is not a list");
        return;
    };
    let Some(head) = list.first().and_then(|s| s.as_symbol()) else {
        report.fail("dispatch", "top-level form has no head symbol");
        return;
    };

    match head {
        // Sequencing — used by defcheck macros that expand to multiple primitives.
        "do" | "begin" | "progn" => {
            for child in &list[1..] {
                dispatch(child, root, report);
            }
        }
        "crd-in-sync" => check_crd_in_sync(&list[1..], root, report),
        "yaml-parses" => check_yaml_parses(&list[1..], root, report),
        "yaml-parses-as" => check_yaml_parses_as(&list[1..], root, report),
        "lisp-compiles" => check_lisp_compiles(&list[1..], root, report),
        "file-contains" => check_file_contains(&list[1..], root, report),
        other => {
            // Fallthrough: if the keyword matches a `#[derive(TataraDomain)]`
            // type registered at startup, compile the form via its derived
            // Lisp compiler and report success. Proves that any Rust type
            // with `TataraDomain` is authorable via checks.lisp for free.
            if let Some(handler) = domain::lookup(other) {
                match (handler.compile)(&list[1..]) {
                    Ok(value) => {
                        let summary = summarize_value(&value);
                        report.pass(format!("{other}: {summary}"));
                    }
                    Err(e) => report.fail(other.to_string(), format!("{e}")),
                }
            } else {
                // Bind the registry-aware near-miss to the substrate's
                // `suggest_keyword` primitive — one helper, not a per-
                // call-site `registered_keywords()` + Levenshtein.
                let hint = domain::suggest_keyword(other)
                    .map(|m| format!("did you mean ({m} ...)? "))
                    .unwrap_or_default();
                report.fail(
                    format!("unknown check: ({other} ...)"),
                    format!(
                        "{hint}no built-in handler, no registered domain, no `defcheck` macro. \
                         Registered domains: {:?}",
                        domain::registered_keywords()
                    ),
                );
            }
        }
    }
}

fn summarize_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(map) => {
            let name = map
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("<unnamed>");
            format!("compiled '{name}' ({} fields)", map.len())
        }
        _ => "compiled to non-object value".into(),
    }
}

// ── executors ────────────────────────────────────────────────────────

fn check_crd_in_sync(args: &[Sexp], root: &Path, report: &mut Report) {
    let kind = args
        .first()
        .and_then(Sexp::as_symbol)
        .unwrap_or("<missing>");
    let path = match args.get(1).and_then(Sexp::as_string) {
        Some(p) => root.join(p),
        None => return report.fail("crd-in-sync", "expected (crd-in-sync <Kind> \"path\")"),
    };
    let current = match kind {
        "Process" => serde_yaml::to_string(&Process::crd()),
        "ProcessTable" => serde_yaml::to_string(&ProcessTable::crd()),
        other => return report.fail(format!("crd-in-sync {other}"), "unknown CRD kind"),
    };
    let current = match current {
        Ok(s) => s,
        Err(e) => return report.fail(format!("crd-in-sync {kind}"), format!("serialize: {e}")),
    };
    let committed = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            return report.fail(
                format!("crd-in-sync {kind}"),
                format!("read {}: {e}", path.display()),
            )
        }
    };
    if normalize(&current) == normalize(&committed) {
        report.pass(format!("{kind} CRD in sync ({})", path.display()));
    } else {
        report.fail(
            format!("crd-in-sync {kind}"),
            format!(
                "{} diverges from current Rust types; regenerate via \
                 ./chart/tatara-reconciler/scripts/regenerate-crds.sh",
                path.display()
            ),
        );
    }
}

fn check_yaml_parses(args: &[Sexp], root: &Path, report: &mut Report) {
    let Some(rel) = args.first().and_then(Sexp::as_string) else {
        return report.fail("yaml-parses", "expected (yaml-parses \"path\")");
    };
    let path = root.join(rel);
    let label = format!("YAML parses: {rel}");
    match fs::read_to_string(&path) {
        Ok(src) => match serde_yaml::from_str::<serde_yaml::Value>(&src) {
            Ok(_) => report.pass(label),
            Err(e) => report.fail(label, format!("YAML: {e}")),
        },
        Err(e) => report.fail(label, format!("read: {e}")),
    }
}

fn check_yaml_parses_as(args: &[Sexp], root: &Path, report: &mut Report) {
    let kind = args
        .first()
        .and_then(Sexp::as_symbol)
        .unwrap_or("<missing>");
    let rel = match args.get(1).and_then(Sexp::as_string) {
        Some(s) => s,
        None => {
            return report.fail(
                "yaml-parses-as",
                "expected (yaml-parses-as <Kind> \"path\")",
            )
        }
    };
    let path = root.join(rel);
    let label = format!("YAML parses as {kind}: {rel}");
    let src = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => return report.fail(label, format!("read: {e}")),
    };
    let result: Result<serde_yaml::Value, _> = match kind {
        "Process" => match serde_yaml::from_str::<Process>(&src) {
            Ok(_) => Ok(serde_yaml::Value::Null),
            Err(e) => Err(e),
        },
        "ProcessTable" => match serde_yaml::from_str::<ProcessTable>(&src) {
            Ok(_) => Ok(serde_yaml::Value::Null),
            Err(e) => Err(e),
        },
        other => return report.fail(label, format!("unknown kind: {other}")),
    };
    match result {
        Ok(_) => report.pass(label),
        Err(e) => report.fail(label, format!("parse: {e}")),
    }
}

fn check_lisp_compiles(args: &[Sexp], root: &Path, report: &mut Report) {
    let Some(rel) = args.first().and_then(Sexp::as_string) else {
        return report.fail("lisp-compiles", "expected (lisp-compiles \"path\" ...)");
    };
    let path = root.join(rel);
    let label = format!("Lisp compiles: {rel}");

    let kw = parse_kwargs(&args[1..]);
    let min_defs = kw
        .iter()
        .find_map(|(k, v)| {
            if k == "min-definitions" {
                v.as_int()
            } else {
                None
            }
        })
        .unwrap_or(1) as usize;
    let requires: Vec<String> = kw
        .iter()
        .find_map(|(k, v)| {
            if k == "requires" {
                v.as_list().map(|xs| {
                    xs.iter()
                        .filter_map(|s| s.as_symbol().map(String::from))
                        .collect()
                })
            } else {
                None
            }
        })
        .unwrap_or_default();

    let src = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => return report.fail(label, format!("read: {e}")),
    };
    let defs = match tatara_process::compile_source(&src) {
        Ok(d) => d,
        Err(e) => {
            return report.fail(label, tatara_lisp::format_diagnostic(&src, &e, Some(rel)));
        }
    };
    if defs.len() < min_defs {
        return report.fail(
            label,
            format!("expected ≥ {} definitions, got {}", min_defs, defs.len()),
        );
    }
    let first = &defs[0];
    for req in &requires {
        let ok = match req.as_str() {
            "intent-nix" => first.spec.intent.nix.is_some(),
            "intent-flux" => first.spec.intent.flux.is_some(),
            "intent-lisp" => first.spec.intent.lisp.is_some(),
            "intent-container" => first.spec.intent.container.is_some(),
            "depends-on" => !first.spec.depends_on.is_empty(),
            "boundary-pre" => !first.spec.boundary.preconditions.is_empty(),
            "boundary-post" => !first.spec.boundary.postconditions.is_empty(),
            "compliance" => !first.spec.compliance.bindings.is_empty(),
            "signals" => first.spec.signals.sigterm_grace_seconds > 0,
            other => {
                return report.fail(label, format!("unknown :requires tag: {other}"));
            }
        };
        if !ok {
            return report.fail(label, format!("definition missing required: {req}"));
        }
    }
    report.pass(format!(
        "{label} ({} defs, {} checks)",
        defs.len(),
        requires.len()
    ));
}

fn check_file_contains(args: &[Sexp], root: &Path, report: &mut Report) {
    let Some(rel) = args.first().and_then(Sexp::as_string) else {
        return report.fail(
            "file-contains",
            "expected (file-contains \"path\" :strings (...))",
        );
    };
    let path = root.join(rel);
    let label = format!("File contains: {rel}");

    let kw = parse_kwargs(&args[1..]);
    let strings: Vec<String> = kw
        .iter()
        .find_map(|(k, v)| {
            if k == "strings" {
                v.as_list().map(|xs| {
                    xs.iter()
                        .filter_map(|s| s.as_string().map(String::from))
                        .collect()
                })
            } else {
                None
            }
        })
        .unwrap_or_default();
    if strings.is_empty() {
        return report.fail(label, ":strings (...) missing or empty");
    }

    let src = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => return report.fail(label, format!("read: {e}")),
    };
    let missing: Vec<&String> = strings
        .iter()
        .filter(|s| !src.contains(s.as_str()))
        .collect();
    if missing.is_empty() {
        report.pass(format!("{label} ({} substrings)", strings.len()));
    } else {
        report.fail(
            label,
            format!(
                "missing {} substring(s): {}",
                missing.len(),
                missing
                    .iter()
                    .map(|s| format!("{s:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
}

// ── helpers ──────────────────────────────────────────────────────────

fn parse_kwargs(rest: &[Sexp]) -> Vec<(String, Sexp)> {
    let mut out = Vec::with_capacity(rest.len() / 2);
    let mut i = 0;
    while i + 1 < rest.len() {
        if let Some(k) = rest[i].as_keyword() {
            out.push((k.to_string(), rest[i + 1].clone()));
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

fn normalize(s: &str) -> String {
    s.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
