//! `tatara-check` — workspace coherence checker.
//!
//! Reads `checks.lisp` at the workspace root and runs each declared check.
//! Check list is data (Lisp); executors are typed Rust. No shell.
//!
//! Invoke: `cargo run --bin tatara-check -p tatara-reconciler`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tatara_lisp::{domain, read, Expander, Sexp};
use tatara_process::intent::IntentKind;
use tatara_reconciler::known_crd::KnownCrd;

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
    let kind = head_symbol_or_missing(args);
    let path = match args.get(1).and_then(Sexp::as_string) {
        Some(p) => root.join(p),
        None => return report.fail("crd-in-sync", "expected (crd-in-sync <Kind> \"path\")"),
    };
    // Kind-string → typed CRD dispatch rides through the ONE substrate
    // primitive `tatara_reconciler::known_crd::KnownCrd` — sibling to
    // the identical dispatch in `check_yaml_parses_as` below AND in
    // `tatara-crd-gen::main`; see [`KnownCrd`]'s docstring for the
    // full pre-lift rationale and the coverage-drift closure the lift
    // provides.
    let current = match KnownCrd::from_kind(kind) {
        Some(k) => k.emit_crd_yaml(),
        None => return report.fail(format!("crd-in-sync {kind}"), "unknown CRD kind"),
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
    let kind = head_symbol_or_missing(args);
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
    // Kind-string → typed-CRD parse rides through the ONE substrate
    // primitive `tatara_reconciler::known_crd::KnownCrd::parse_yaml_as`
    // — pre-lift the dispatcher hand-authored a TWO-arm match on only
    // `"Process"` + `"ProcessTable"`, so an operator authoring
    // `(yaml-parses-as EphemeralPool "…")` got a runtime
    // `"unknown kind"` even though the CRD ships in this workspace.
    // Post-lift the closed set on [`KnownCrd::ALL`] covers all FOUR
    // CRDs by construction — `EphemeralPool` + `EphemeralAllocation`
    // are now first-class targets for this check without a per-arm
    // edit here.
    let result = match KnownCrd::from_kind(kind) {
        Some(k) => k.parse_yaml_as(&src),
        None => return report.fail(label, format!("unknown kind: {kind}")),
    };
    match result {
        Ok(()) => report.pass(label),
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
    let requires: Vec<String> = find_kw_string_list(&kw, "requires", Sexp::as_symbol);
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
                // `intent-<kind>` require-tags dispatch through the
                // typed [`IntentKind`] closed set — the suffix is
                // parsed via the autoderived `FromStr` (whose
                // vocabulary is `IntentKind::as_str`, byte-identical
                // to the serde `rename_all = "camelCase"` field names
                // on `Intent`) and the presence probe fans out via
                // the substrate primitive [`Intent::has`]. Pre-lift
                // the dispatcher restated five hand-authored
                // `first.spec.intent.<field>.is_some()` arms that
                // drifted from `IntentKind::ALL` (the sixth variant
                // `Guest` had no `intent-guest` arm at all); post-
                // lift adding a seventh variant lands at ONE
                // `IntentKind` `ALL` entry + ONE `select` arm and
                // the check-tag surface picks up the new
                // `intent-<kind>` tag for free.
                let ok = if let Some(suffix) = req.strip_prefix("intent-") {
                    match suffix.parse::<IntentKind>() {
                        Ok(kind) => first.spec.intent.has(kind),
                        Err(_) => {
                            return report.fail(label, format!("unknown :requires tag: {req}"));
                        }
                    }
                } else {
                    match req.as_str() {
                        "lifetime-ephemeral" => first.spec.lifetime.is_ephemeral(),
                        "depends-on" => !first.spec.depends_on.is_empty(),
                        "boundary-pre" => !first.spec.boundary.preconditions.is_empty(),
                        "boundary-post" => !first.spec.boundary.postconditions.is_empty(),
                        "compliance" => !first.spec.compliance.bindings.is_empty(),
                        "signals" => first.spec.signals.sigterm_grace_seconds > 0,
                        other => {
                            return report.fail(label, format!("unknown :requires tag: {other}"));
                        }
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
    let strings: Vec<String> = find_kw_string_list(&kw, "strings", Sexp::as_string);
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

/// Collect the items under a `:name` keyword slot as `Vec<String>`,
/// projecting each item through `proj` and skipping non-matching items —
/// the ONE substrate owner of the `find_kw(&kw, NAME).and_then(Sexp::as_list)
/// .map(|xs| xs.iter().filter_map(|s| s.<PROJ>().map(String::from)).collect())
/// .unwrap_or_default()` five-line chain both [`check_lisp_compiles`]
/// (`:requires` slot with [`Sexp::as_symbol`]) and [`check_file_contains`]
/// (`:strings` slot with [`Sexp::as_string`]) hand-authored past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold.
///
/// The projection is a `fn(&Sexp) -> Option<&str>` — every atomic
/// soft-projection on the substrate's [`Sexp`] algebra fits the bound
/// ([`Sexp::as_symbol`], [`Sexp::as_string`], [`Sexp::as_keyword`],
/// [`Sexp::as_symbol_or_string`]), so a future `:name-list` slot on a
/// new keyword-family check axis (e.g. `:keywords` for a check that
/// listed keyword literals, `:heads` for a hybrid symbol-or-string
/// slot) binds through the SAME primitive with the matching typed
/// projection — no per-slot restatement of the five-line
/// find/list/filter/collect chain.
///
/// Semantics — byte-identical to the pre-lift chain:
///
/// * `:name` slot absent (`find_kw` returns `None`) → empty [`Vec`].
/// * `:name` slot present but NOT a [`Sexp::List`] (a bare atom, a
///   quote-family wrapper) → empty [`Vec`] (soft failure through
///   `and_then(Sexp::as_list)`).
/// * `:name` slot is a list → each item projected through `proj`;
///   items whose projection returns `None` are silently skipped (a
///   mixed list `(foo 42 "bar")` with [`Sexp::as_symbol`] collects
///   just `["foo"]`), matching the pre-lift `filter_map` discipline.
///
/// Sibling of [`find_kw`] on the parsed-kwargs algebra: where
/// [`find_kw`] is the substrate primitive for "resolve a `:name` slot
/// to `Option<&Sexp>`", THIS primitive composes that lookup with the
/// canonical list-of-atoms projection every executor that reads a
/// list-shaped `:name-list` slot walks. Callers that need a
/// non-string-shaped list projection (a `Vec<i64>` for a hypothetical
/// `:thresholds` slot, a `Vec<PathBuf>` for a `:paths` slot) still
/// compose their own `.map(...)` on top of `find_kw(&kw,
/// NAME).and_then(Sexp::as_list)` — this primitive names the
/// string-shape corner (the only shape both current callers walked).
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; two
/// byte-identical five-line inline compositions collapse onto ONE
/// substrate owner past the ≥2 PRIME-DIRECTIVE trigger. THEORY.md
/// §II.1 invariant 2 — free middle; both current executors AND every
/// future `:name-list` slot on a new check axis route through the
/// SAME parsed-kwargs-to-owned-string-list projection, so a
/// regression that drifts one caller's discipline from the others
/// becomes structurally impossible.
fn find_kw_string_list(
    kw: &[(String, Sexp)],
    name: &str,
    proj: fn(&Sexp) -> Option<&str>,
) -> Vec<String> {
    find_kw(kw, name)
        .and_then(Sexp::as_list)
        .map(|xs| xs.iter().filter_map(proj).map(String::from).collect())
        .unwrap_or_default()
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

/// Sentinel slug substituted for a missing / non-symbol positional
/// head-arg on an executor's `args` slice — the ONE substrate owner of
/// the `"<missing>"` literal both [`check_crd_in_sync`] and
/// [`check_yaml_parses_as`] restated verbatim on their head-arg
/// fallback rung past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// threshold. See [`head_symbol_or_missing`] for the composed chain.
const MISSING_ARG_SLUG: &str = "<missing>";

/// Read the head positional arg on an executor's `args` slice as a
/// symbol, falling back to [`MISSING_ARG_SLUG`] when the slot is
/// absent or non-symbol — the ONE substrate owner of the
/// `args.first().and_then(Sexp::as_symbol).unwrap_or("<missing>")`
/// three-line chain both [`check_crd_in_sync`] and
/// [`check_yaml_parses_as`] hand-authored past the ★★ PRIME-DIRECTIVE
/// ≥ 2 duplication threshold to extract the `<Kind>` slot for the
/// executor's error-label / dispatch prose.
///
/// Semantics — byte-identical to the pre-lift chain:
///
/// * `args` empty → [`MISSING_ARG_SLUG`].
/// * `args[0]` present but NOT a [`Sexp::Symbol`] (a bare string, a
///   nested list, a keyword) → [`MISSING_ARG_SLUG`] (soft failure
///   through `and_then(Sexp::as_symbol)`).
/// * `args[0]` is a symbol → the borrowed `&str` payload.
///
/// The return-lifetime is the input slice's lifetime — both callers'
/// downstream `format!("<check> {kind}", ...)` label composition and
/// [`KnownCrd::from_kind`] dispatch consume the borrowed `&str`
/// without an intervening allocation.
///
/// Sibling of [`find_kw`] on the parsed-args algebra: [`find_kw`] is
/// the substrate primitive for "resolve a `:name` kwarg slot to
/// `Option<&Sexp>`"; THIS primitive is the peer on the POSITIONAL
/// (head-arg) axis, projecting to `&str` with a domain-specific
/// sentinel fallback so the pre-lift `unwrap_or("<missing>")` corner
/// binds at ONE site rather than at each executor's head-arg decode.
///
/// A future normalization (a near-miss hint composed from
/// [`KnownCrd::ALL`] when the head-arg is present but unknown, a
/// promotion of the sentinel to a [`Result`]-shaped return that
/// forces every caller to path-through `report.fail` explicitly, a
/// swap of the sentinel spelling for a per-executor slug) lands at
/// THIS ONE substrate primitive and both current callers plus every
/// future positional-symbol executor (a `(kubectl-crd-in-sync <Kind>
/// ...)` peer, a `(kustomization-parses-as <Kind> ...)` peer, a
/// hypothetical `(helmrelease-shape <Kind> ...)` shape check) inherit
/// the upgrade mechanically.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// three-line `args.first().and_then(...).unwrap_or(...)` chain
/// recurred at TWO executor bodies past the ≥2 PRIME-DIRECTIVE
/// trigger, and is lifted to ONE substrate owner here.
/// THEORY.md §II.1 invariant 5 — composition preserves proofs; both
/// callers routing through the SAME substrate primitive means a
/// future sentinel or projection change lands at ONE site and every
/// downstream head-arg consumer inherits the shift by construction.
fn head_symbol_or_missing(args: &[Sexp]) -> &str {
    args.first()
        .and_then(Sexp::as_symbol)
        .unwrap_or(MISSING_ARG_SLUG)
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
    use super::{
        find_kw, find_kw_string_list, head_symbol_or_missing, parse_kwargs, MISSING_ARG_SLUG,
    };
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

    // ── find_kw_string_list substrate pins ───────────────────────────
    //
    // Fail-before-pass-after granularity: the `find_kw_string_list`
    // free function did not exist before this commit, so each test
    // below fails to compile pre-lift. Post-lift they collectively
    // pin the (find, list, filter, collect) chain semantics at ONE
    // substrate owner — a regression that swapped the `filter_map`
    // for a `map` (turning silent-skip of non-matching items into an
    // implicit None-in-Vec), dropped the `find_kw` short-circuit
    // (allowing empty output where `:name` was absent), or changed
    // the `.unwrap_or_default()` corner (panicking on missing keys)
    // surfaces HERE rather than as silent drift at both check
    // executor callers.

    #[test]
    fn find_kw_string_list_collects_symbol_items_via_as_symbol_projection() {
        // Byte-identical parity with the pre-lift `check_lisp_compiles`
        // `:requires` decode: `(lisp-compiles ... :requires (intent-nix
        // depends-on))` collects `["intent-nix", "depends-on"]` through
        // `Sexp::as_symbol`. Pin the happy-path shape both current
        // callers walk.
        let kw = kwargs_from("(check :requires (intent-nix depends-on boundary-post))");
        assert_eq!(
            find_kw_string_list(&kw, "requires", Sexp::as_symbol),
            vec![
                "intent-nix".to_string(),
                "depends-on".to_string(),
                "boundary-post".to_string(),
            ],
        );
    }

    #[test]
    fn find_kw_string_list_collects_string_items_via_as_string_projection() {
        // Byte-identical parity with the pre-lift `check_file_contains`
        // `:strings` decode: `(file-contains ... :strings ("foo" "bar"))`
        // collects `["foo", "bar"]` through `Sexp::as_string`. Pin the
        // second current caller's shape so a projection swap (e.g. an
        // accidental `Sexp::as_symbol` reroute that would collect an
        // empty vec from a string-only source) fails HERE.
        let kw = kwargs_from(r#"(check :strings ("services.tatara.processes" "pointType"))"#);
        assert_eq!(
            find_kw_string_list(&kw, "strings", Sexp::as_string),
            vec![
                "services.tatara.processes".to_string(),
                "pointType".to_string(),
            ],
        );
    }

    #[test]
    fn find_kw_string_list_returns_empty_when_the_name_is_absent() {
        // Miss-path pin: an unknown `:name` slot (a `(check :other 42)`
        // form with no `:strings` slot) collects an empty Vec, matching
        // the pre-lift `find_kw(...).<chain>.unwrap_or_default()` shape.
        // Load-bearing: `check_file_contains` follows the empty-Vec
        // return with `if strings.is_empty() { return report.fail(...) }`
        // — a regression that panicked on the missing slot rather than
        // returning empty would fail HERE before reaching that gate.
        let kw = kwargs_from("(check :other 42)");
        assert!(find_kw_string_list(&kw, "strings", Sexp::as_string).is_empty());
    }

    #[test]
    fn find_kw_string_list_returns_empty_when_the_value_is_not_a_list() {
        // Non-list value pin: a `:strings` slot bound to a bare atom
        // (`(check :strings "just-one")`, a plausible operator typo
        // meaning `(check :strings ("just-one"))`) collects an empty
        // Vec, matching the pre-lift `.and_then(Sexp::as_list)`
        // soft-failure shape. A regression that reached inside a bare
        // atom would either surface here as a wrong non-empty Vec or
        // as a panic on the missing list-shape.
        let kw = kwargs_from(r#"(check :strings "just-one")"#);
        assert!(find_kw_string_list(&kw, "strings", Sexp::as_string).is_empty());
    }

    #[test]
    fn find_kw_string_list_silently_skips_items_the_projection_rejects() {
        // Mixed-shape pin: a list with items of the "wrong" shape for
        // the projection (integers and strings in a `:requires` slot
        // expecting symbols) collects only the projection-accepted
        // items, matching the pre-lift `filter_map` discipline. A
        // regression that swapped `filter_map` for `map` (silently
        // producing `Vec<Option<String>>` or panicking on the first
        // rejection) fails HERE. Sibling-shape pin to the pre-lift
        // behavior every operator authoring a mixed list relied on.
        let kw = kwargs_from(r#"(check :requires (intent-nix 42 "quoted-str" depends-on))"#);
        assert_eq!(
            find_kw_string_list(&kw, "requires", Sexp::as_symbol),
            vec!["intent-nix".to_string(), "depends-on".to_string()],
        );
    }

    #[test]
    fn find_kw_string_list_collects_empty_when_the_list_is_empty() {
        // Empty-list pin: an explicit `:strings ()` slot collects an
        // empty Vec — distinct from the "slot absent" case at
        // `find_kw_string_list_returns_empty_when_the_name_is_absent`,
        // but reaching the SAME empty output. Both paths must land on
        // the same Vec shape so `check_file_contains`'s downstream
        // `is_empty()` gate fires uniformly whether the operator wrote
        // `(file-contains "path")` (slot absent) or `(file-contains
        // "path" :strings ())` (slot present but empty).
        let kw = kwargs_from("(check :strings ())");
        assert!(find_kw_string_list(&kw, "strings", Sexp::as_string).is_empty());
    }

    #[test]
    fn find_kw_string_list_matches_pre_lift_chain_bytewise_on_symbol_axis() {
        // Byte-identical parity pin with the pre-lift five-line chain
        // both current callers hand-authored, on the SYMBOL projection
        // axis (`check_lisp_compiles`'s `:requires` slot). A regression
        // in the primitive that broke byte identity with the pre-lift
        // shape at ANY corner surfaces HERE rather than as silent
        // check-executor drift.
        for src in [
            "(check :requires ())",
            "(check :requires (foo))",
            "(check :requires (foo bar baz))",
            "(check :requires (foo 42 baz))",
            "(check :other 1)",
        ] {
            let kw = kwargs_from(src);
            let via_primitive: Vec<String> = find_kw_string_list(&kw, "requires", Sexp::as_symbol);
            let via_pre_lift: Vec<String> = find_kw(&kw, "requires")
                .and_then(Sexp::as_list)
                .map(|xs| {
                    xs.iter()
                        .filter_map(|s| s.as_symbol().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            assert_eq!(via_primitive, via_pre_lift, "drift on {src}");
        }
    }

    #[test]
    fn find_kw_string_list_matches_pre_lift_chain_bytewise_on_string_axis() {
        // Byte-identical parity pin with the pre-lift five-line chain
        // both current callers hand-authored, on the STRING projection
        // axis (`check_file_contains`'s `:strings` slot). Sibling-shape
        // pin to `find_kw_string_list_matches_pre_lift_chain_bytewise_on_symbol_axis`
        // — the two together anchor the primitive at both current
        // caller-visible projection axes so a regression at either
        // surface fails HERE rather than as silent drift downstream.
        for src in [
            r#"(check :strings ())"#,
            r#"(check :strings ("only"))"#,
            r#"(check :strings ("a" "b" "c"))"#,
            r#"(check :strings ("a" 42 "b"))"#,
            r#"(check :other 1)"#,
        ] {
            let kw = kwargs_from(src);
            let via_primitive: Vec<String> = find_kw_string_list(&kw, "strings", Sexp::as_string);
            let via_pre_lift: Vec<String> = find_kw(&kw, "strings")
                .and_then(Sexp::as_list)
                .map(|xs| {
                    xs.iter()
                        .filter_map(|s| s.as_string().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            assert_eq!(via_primitive, via_pre_lift, "drift on {src}");
        }
    }

    // ── head_symbol_or_missing substrate pins ────────────────────────
    //
    // Fail-before-pass-after granularity: the `head_symbol_or_missing`
    // free function and the [`MISSING_ARG_SLUG`] const did not exist
    // before this commit, so each test below fails to compile pre-lift.
    // Post-lift they collectively pin the (positional first-arg,
    // symbol projection, sentinel fallback) shape at ONE substrate
    // owner — a regression that dropped the `Sexp::as_symbol`
    // projection (silently promoting a bare string or nested list into
    // the returned slug), swapped the sentinel spelling (`"<unknown>"`,
    // `"?"`, `""`) so the two pre-lift callers' error-label prose drifts
    // apart, or panicked on the empty-args corner surfaces HERE rather
    // than as silent operator-facing drift at the two `check_*`
    // executors that build their `format!("<check> {kind}", ...)` label
    // from this primitive's return.

    // Re-parse a bare positional source through the reader and hand
    // its interior tail slice to the primitive — the same tokenizer +
    // Sexp-shape pipeline the `check_*` executors walk when parsing a
    // `(<check-name> <head-arg> ...)` form.
    fn args_from(src: &str) -> Vec<Sexp> {
        let forms = read(src).expect("test source must parse");
        let outer = forms[0].as_list().expect("wrap source in a list");
        outer[1..].to_vec()
    }

    #[test]
    fn head_symbol_or_missing_returns_the_symbol_payload_at_position_zero() {
        // Byte-identical parity with the pre-lift `check_crd_in_sync`
        // head-arg decode: `(crd-in-sync Process "path")` yields the
        // symbol payload `"Process"`. Pin the happy-path shape both
        // current callers walk — a regression that dropped the symbol
        // projection (returning a bare `Sexp` `Debug`-render, or
        // failing to strip the `Symbol` newtype) would fail HERE.
        let args =
            args_from(r#"(crd-in-sync Process "chart/tatara-reconciler/crds/Process.yaml")"#);
        assert_eq!(head_symbol_or_missing(&args), "Process");
    }

    #[test]
    fn head_symbol_or_missing_returns_sentinel_when_args_is_empty() {
        // Empty-args pin: an executor called with no positional args
        // (`(crd-in-sync)`) returns the [`MISSING_ARG_SLUG`] sentinel.
        // Load-bearing: both pre-lift callers proceed to `report.fail`
        // downstream with the sentinel embedded in the error label —
        // a regression that panicked on the empty slice would abort
        // the whole `tatara-check` run rather than reporting a soft
        // failure per check.
        let args: Vec<Sexp> = Vec::new();
        assert_eq!(head_symbol_or_missing(&args), MISSING_ARG_SLUG);
    }

    #[test]
    fn head_symbol_or_missing_returns_sentinel_on_non_symbol_head_arg() {
        // Non-symbol head-arg pin: an operator typo like
        // `(crd-in-sync "Process" "path")` (quoted string where a
        // symbol was expected) yields the [`MISSING_ARG_SLUG`]
        // sentinel — matches the pre-lift `.and_then(Sexp::as_symbol)`
        // soft-failure shape. A regression that reached inside a
        // string / list / integer payload would either surface here
        // as a wrong non-sentinel slug or as a panic on the projection
        // arm.
        for src in [
            r#"(crd-in-sync "Process" "path")"#,
            r#"(crd-in-sync 42 "path")"#,
            r#"(crd-in-sync (nested) "path")"#,
            r#"(crd-in-sync :Process "path")"#,
        ] {
            let args = args_from(src);
            assert_eq!(
                head_symbol_or_missing(&args),
                MISSING_ARG_SLUG,
                "non-symbol head-arg must fall through to sentinel on {src}",
            );
        }
    }

    #[test]
    fn head_symbol_or_missing_matches_pre_lift_chain_bytewise() {
        // Byte-identical parity pin with the pre-lift three-line chain
        // both current callers hand-authored. Sweeps the four corners
        // every pre-lift consumer visited: (a) present symbol, (b)
        // empty args, (c) non-symbol head, (d) a symbol with a full
        // trailing kwargs tail (the real-world shape). A regression in
        // the primitive that broke byte identity with the pre-lift
        // shape at ANY corner surfaces HERE rather than as silent
        // check-executor drift between the migrated callers and any
        // future caller that reads the primitive's output.
        for src in [
            "(check)",
            "(check Process)",
            r#"(check Process "path")"#,
            r#"(check "Process" "path")"#,
            r#"(check EphemeralPool "path" :extra 1)"#,
        ] {
            let args = args_from(src);
            let via_primitive: &str = head_symbol_or_missing(&args);
            let via_pre_lift: &str = args
                .first()
                .and_then(Sexp::as_symbol)
                .unwrap_or("<missing>");
            assert_eq!(via_primitive, via_pre_lift, "drift on {src}");
        }
    }

    #[test]
    fn missing_arg_slug_is_the_pre_lift_sentinel_literal() {
        // Value pin binding the [`MISSING_ARG_SLUG`] const to the
        // exact pre-lift literal both callers hand-authored — a
        // regression that renamed the sentinel (`"<unknown>"`, `"?"`,
        // `""`) would silently break every operator's grep on the
        // sentinel-embedded error label the two pre-lift callers
        // emitted. Surfaces at THIS pin rather than as silent
        // sentinel-string drift.
        assert_eq!(MISSING_ARG_SLUG, "<missing>");
    }

    #[test]
    fn head_symbol_or_missing_only_reads_position_zero() {
        // Positional-scope pin: the primitive ignores every arg past
        // position 0 — a regression that scanned past the head (e.g.
        // walking `.iter().find_map(Sexp::as_symbol)`, silently
        // succeeding on a symbol at position 2 when position 0 is
        // non-symbol) would fail HERE. Load-bearing: both current
        // callers separately decode `args.get(1)` as their path slot,
        // so the primitive must NOT reach past the head or the two
        // callers' positional discipline drifts.
        let args = args_from(r#"(check "leading-string" NotTheHead "path")"#);
        assert_eq!(
            head_symbol_or_missing(&args),
            MISSING_ARG_SLUG,
            "primitive must not scan past position 0",
        );
    }
}
