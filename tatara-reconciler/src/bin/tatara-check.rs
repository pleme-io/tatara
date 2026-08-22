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
use tatara_process::allocation::EphemeralAllocation;
use tatara_process::pool::EphemeralPool;
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
    // Seed the global domain registry with every domain authored as Lisp.
    // - tatara-process: defpoint (ProcessSpec) + defephemeral (EphemeralSpec)
    // - tatara-domains: defmonitor, defnotify, defalertpolicy (demo set)
    tatara_process::register_all();
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
                // Bind the registry-dispatch fallthrough to the
                // substrate's structural variant + named primitive —
                // ONE call into `unknown_domain_keyword` materializes
                // the offending keyword, the near-miss hint (when one
                // exists within the bounded edit distance), and the
                // sorted registered set as first-class fields of
                // `LispError::UnknownDomainKeyword`. Authoring surfaces
                // that pattern-match on the variant gain structural
                // binding to `keyword` / `hint` / `registered`; tools
                // that substring-match on the rendered diagnostic see
                // a stable shape. The contextual prose (`no defcheck
                // macro`) is tatara-check-specific framing — the
                // variant doesn't carry it because not every
                // unknown-domain-keyword consumer has a `defcheck`
                // macroexpander on the same path.
                let err = domain::unknown_domain_keyword(other);
                report.fail(
                    "unknown check",
                    format!("{err}; no `defcheck` macro on the path"),
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
        "EphemeralPool" => serde_yaml::to_string(&EphemeralPool::crd()),
        "EphemeralAllocation" => serde_yaml::to_string(&EphemeralAllocation::crd()),
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
    let min_defs = find_kw(&kw, "min-definitions")
        .and_then(Sexp::as_int)
        .unwrap_or(1) as usize;
    let requires: Vec<String> = find_kw(&kw, "requires")
        .and_then(Sexp::as_list)
        .map(|xs| {
            xs.iter()
                .filter_map(|s| s.as_symbol().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    // Optional `:domain <name>` — selects which typed surface to compile.
    // Default `point` (ProcessSpec via `(defpoint …)`). New: `ephemeral`
    // (EphemeralSpec via `(defephemeral …)`).
    let domain = find_kw(&kw, "domain")
        .and_then(|v| v.as_symbol().or_else(|| v.as_string()))
        .map(String::from)
        .unwrap_or_else(|| "point".into());

    let src = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => return report.fail(label, format!("read: {e}")),
    };

    let n_defs = match domain.as_str() {
        "point" => {
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
                    "intent-aplicacao" => first.spec.intent.aplicacao.is_some(),
                    "lifetime-ephemeral" => first.spec.lifetime.is_ephemeral(),
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
            defs.len()
        }
        "ephemeral" => {
            let defs = match tatara_process::ephemeral::compile_ephemeral_source(&src) {
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
                    "aplicacao" => !first.spec.aplicacao.chart_ref.is_empty(),
                    "ttl" => !first.spec.ttl.is_empty(),
                    "teardown" => true, // typed enum — always present
                    "postconditions" => !first.spec.postconditions.is_empty(),
                    "preconditions" => !first.spec.preconditions.is_empty(),
                    "closed-loop-auth" => first.spec.postconditions.iter().any(|c| {
                        matches!(
                            c.kind,
                            tatara_process::boundary::ConditionKind::ClosedLoopAuth
                        )
                    }),
                    other => {
                        return report.fail(
                            label,
                            format!("unknown :requires tag for ephemeral domain: {other}"),
                        );
                    }
                };
                if !ok {
                    return report.fail(label, format!("definition missing required: {req}"));
                }
            }
            defs.len()
        }
        other => {
            return report.fail(
                label,
                format!("unknown :domain {other:?} (known: point, ephemeral)"),
            );
        }
    };

    report.pass(format!(
        "{label} ({n_defs} defs, {} checks)",
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
    let strings: Vec<String> = find_kw(&kw, "strings")
        .and_then(Sexp::as_list)
        .map(|xs| {
            xs.iter()
                .filter_map(|s| s.as_string().map(String::from))
                .collect()
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

/// First-match lookup on the [`parse_kwargs`]-produced kwargs slice.
///
/// Collapses the four sibling `kw.iter().find_map(|(k, v)| if k == NAME
/// { <extract v> } else { None })` sites (three in
/// [`check_lisp_compiles`], one in [`check_file_contains`]) onto ONE
/// name-equality gate — the (equality check, first-match short-circuit)
/// discipline binds at ONE site on the parsed-kwargs algebra, and each
/// caller composes its own projection (`Sexp::as_int`, `Sexp::as_list`,
/// `Sexp::as_symbol`.or_else, etc.) onto the returned `Option<&Sexp>`
/// through `and_then`. The extractor closure in the pre-lift shape used
/// `find_map` — the outer combinator kept scanning if the extractor
/// returned `None`. Post-lift the primitive short-circuits on the first
/// name match regardless of what the caller's downstream projection
/// yields, matching how `parse_kwargs` already collapses duplicate names
/// silently (first pair wins) at the collection boundary.
///
/// A future new `(<check> ... :NEW-KW value)` slot the check executors
/// want to read lands as ONE `find_kw(&kw, "new-kw").and_then(Sexp::as_<T>)`
/// delegate rather than as a fresh copy of the pre-lift four-line
/// `find_map` shape at yet another `check_*` body.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition. The
/// (name-equality gate, `Option<&Sexp>` return) pair is a substrate
/// primitive on the parsed-kwargs slice; callers compose their per-slot
/// projection through the shared `Option` type rather than restating the
/// gate at each callsite.
fn find_kw<'a>(kw: &'a [(String, Sexp)], name: &str) -> Option<&'a Sexp> {
    kw.iter().find_map(|(k, v)| (k == name).then_some(v))
}

fn normalize(s: &str) -> String {
    s.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::{find_kw, parse_kwargs};
    use tatara_lisp::{read, Sexp};

    // Re-parse a `(list …)` source through the reader and hand its
    // interior slice to `parse_kwargs`, so every test exercises the
    // same tokenizer + Sexp-shape pipeline the `check_*` executors do
    // rather than constructing `Sexp` values by hand at each test.
    fn kwargs_from(src: &str) -> Vec<(String, Sexp)> {
        let forms = read(src).expect("test source must parse");
        let outer = forms[0].as_list().expect("wrap source in a list");
        // Skip the head symbol; kwargs live in the tail.
        parse_kwargs(&outer[1..])
    }

    #[test]
    fn find_kw_returns_the_value_when_the_name_is_present() {
        // Pin the happy path: the name matches, the extractor sees the
        // borrowed `&Sexp` value, and the caller's downstream projection
        // (`Sexp::as_int`) yields the wrapped payload. Fail-before-pass-
        // after: `find_kw` does not exist on the pre-lift binary — this
        // test does not compile before the lift.
        let kw = kwargs_from("(check :min-definitions 3)");
        assert_eq!(
            find_kw(&kw, "min-definitions").and_then(Sexp::as_int),
            Some(3),
        );
    }

    #[test]
    fn find_kw_returns_none_when_the_name_is_absent() {
        // Pin the miss path: an unknown name yields `None`, and the
        // caller's `.unwrap_or_default()` / `.unwrap_or(<default>)` chain
        // fills in the caller's typed default. Together with the happy-
        // path pin, this closes the two-cell (present, absent) matrix at
        // the substrate primitive's return-shape boundary.
        let kw = kwargs_from("(check :min-definitions 3)");
        assert!(find_kw(&kw, "missing-key").is_none());
    }

    #[test]
    fn find_kw_returns_the_first_matching_value_when_the_name_repeats() {
        // Pin the first-match short-circuit: on a duplicate name (which
        // `parse_kwargs` silently keeps at the first pair), `find_kw`
        // returns the first pair's value, not the last. A regression
        // that swapped `find_map` for a `filter` + `last` (silently
        // reordering the two duplicates) would fail here.
        let kw = kwargs_from("(check :min-definitions 3 :min-definitions 9)");
        assert_eq!(
            find_kw(&kw, "min-definitions").and_then(Sexp::as_int),
            Some(3),
        );
    }

    #[test]
    fn find_kw_returns_none_on_empty_kwargs() {
        // Boundary pin: an empty `parse_kwargs` output (a check called
        // with no `:key value` tail — `(file-contains "path")` before the
        // caller composes the extractor's `.unwrap_or_default()`)
        // returns `None` for every lookup. A regression that panicked
        // on the empty slice (e.g. an unchecked `kw[0].0 == name`) would
        // fail here.
        let kw: Vec<(String, Sexp)> = Vec::new();
        assert!(find_kw(&kw, "any-name").is_none());
    }
}
