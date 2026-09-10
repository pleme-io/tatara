//! `tatara-check` — workspace coherence checker.
//!
//! Reads `checks.lisp` at the workspace root and runs each declared check.
//! Check list is data (Lisp); executors are typed Rust. No shell.
//!
//! Invoke: `cargo run --bin tatara-check -p tatara-reconciler`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;

use tatara_lisp::{domain, read, Expander, Sexp};
use tatara_process::intent::IntentKind;
use tatara_process::lifetime::LifetimeKind;
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

    // Startup-side `eprintln!("tatara-check: <detail>"); return
    // ExitCode::from(2);` two-step bail ride the ONE substrate primitive
    // `startup_bail` — pre-lift the SAME shape was hand-authored at 4
    // sites (workspace-root miss, checks.lisp read fail, checks.lisp
    // parse fail, macroexpand fail) past the ★★ PRIME-DIRECTIVE ≥ 2
    // duplication threshold. Post-lift the (`"tatara-check:"` prefix
    // sink, `ExitCode::from(2)` config-failure exit) pair lives at ONE
    // substrate owner.
    let root = match workspace_root() {
        Some(r) => r,
        None => {
            return startup_bail(
                "could not locate workspace root (looked for Cargo.toml + checks.lisp)",
            )
        }
    };
    let checks_path = root.join("checks.lisp");
    let src = match fs::read_to_string(&checks_path) {
        Ok(s) => s,
        Err(e) => return startup_bail(format_args!("read {}: {e}", checks_path.display())),
    };
    let raw = match read(&src) {
        Ok(f) => f,
        Err(e) => return startup_bail(format_args!("parse {}: {e}", checks_path.display())),
    };

    // Tier 1: checks.lisp may contain (defcheck …) macros + macro calls.
    // Run the expander first so primitives authored via defcheck materialize
    // as primitive check forms the dispatcher understands.
    let mut expander = Expander::new();
    let forms = match expander.expand_program(raw) {
        Ok(f) => f,
        Err(e) => return startup_bail(format_args!("macroexpand: {e}")),
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
    // Positional-`"path"` decode + `expected …` failure-arm shape rides
    // the ONE substrate primitive `required_positional_string` — sibling
    // to the identical decode at the four peer executors below (yaml-
    // parses / yaml-parses-as / lisp-compiles / file-contains). Post-lift
    // the (positional-string decode, report failure on absent slot,
    // early-return control-flow) triad lives at ONE substrate owner.
    let Some(rel) = required_positional_string(
        args,
        1,
        "crd-in-sync",
        "expected (crd-in-sync <Kind> \"path\")",
        report,
    ) else {
        return;
    };
    let path = root.join(rel);
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
    // Positional-`"path"` decode + `expected …` failure-arm shape rides
    // the ONE substrate primitive `required_positional_string` — one of
    // five sibling `check_*` executors that share the (positional-string
    // decode, report failure on absent slot, early-return control-flow)
    // triad on the same substrate owner.
    let Some(rel) = required_positional_string(
        args,
        0,
        "yaml-parses",
        "expected (yaml-parses \"path\")",
        report,
    ) else {
        return;
    };
    let path = root.join(rel);
    let label = format!("YAML parses: {rel}");
    // Read-side dispatch rides the ONE substrate primitive
    // `read_or_fail` — pre-lift this executor hand-authored a `match
    // fs::read_to_string(&path) { Ok(src) => <inline-pass>, Err(e) =>
    // report.fail(label, format!("read: {e}")) }` chain whose failure
    // arm was byte-identical to the three peer executors
    // (`check_yaml_parses_as`, `check_lisp_compiles`,
    // `check_file_contains`) already routing through the substrate.
    // The pre-lift match wrapped the pass path inline rather than
    // early-returning, so the prior lift missed this fourth consumer;
    // post-lift the `check_*` family's read + `"read: {e}"` failure-
    // arm shape lives at ONE substrate owner across all FOUR
    // executors.
    let Some(src) = read_or_fail(&path, &label, report) else {
        return;
    };
    // Terminal `Result<T, E: Display>` sink rides the ONE substrate
    // primitive `report_result_prefixed` — pre-lift this executor
    // hand-authored a `match RESULT { Ok(_) => report.pass(label),
    // Err(e) => report.fail(label, format!("<prefix>: {e}")) }` chain
    // byte-identical to the peer `check_yaml_parses_as` executor's
    // terminal shape modulo the prefix literal (`"YAML"` here vs
    // `"parse"` there). Post-lift the (pass on `Ok`, fail with
    // `<prefix>: {e}` on `Err`, label consumed by both arms) triad
    // lives at ONE substrate owner across BOTH yaml-executor family
    // members past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold.
    report_result_prefixed(
        serde_yaml::from_str::<serde_yaml::Value>(&src),
        label,
        "YAML",
        report,
    );
}

fn check_yaml_parses_as(args: &[Sexp], root: &Path, report: &mut Report) {
    let kind = head_symbol_or_missing(args);
    // Positional-`"path"` decode + `expected …` failure-arm shape rides
    // the ONE substrate primitive `required_positional_string` — one of
    // five sibling `check_*` executors that share the (positional-string
    // decode, report failure on absent slot, early-return control-flow)
    // triad on the same substrate owner.
    let Some(rel) = required_positional_string(
        args,
        1,
        "yaml-parses-as",
        "expected (yaml-parses-as <Kind> \"path\")",
        report,
    ) else {
        return;
    };
    let path = root.join(rel);
    let label = format!("YAML parses as {kind}: {rel}");
    // Read-side dispatch rides the ONE substrate primitive
    // `read_or_fail` — pre-lift this was a hand-authored 4-link
    // `match fs::read_to_string(&path) { Ok(s) => s, Err(e) => return
    // report.fail(label, format!("read: {e}")) }` chain, one of FOUR
    // workspace-wide restatements past the ★★ PRIME-DIRECTIVE ≥ 2
    // duplication threshold (peers at `check_yaml_parses` +
    // `check_lisp_compiles` + `check_file_contains`). Post-lift the
    // read + `"read: {e}"` failure-arm shape lives at ONE substrate
    // owner.
    let Some(src) = read_or_fail(&path, &label, report) else {
        return;
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
    // Terminal `Result<T, E: Display>` sink rides the ONE substrate
    // primitive `report_result_prefixed` — pre-lift this executor
    // hand-authored a `match result { Ok(()) => report.pass(label),
    // Err(e) => report.fail(label, format!("parse: {e}")) }` chain
    // byte-identical to the peer `check_yaml_parses` executor's
    // terminal shape modulo the prefix literal (`"parse"` here vs
    // `"YAML"` there). Post-lift the (pass on `Ok`, fail with
    // `<prefix>: {e}` on `Err`, label consumed by both arms) triad
    // lives at ONE substrate owner across BOTH yaml-executor family
    // members past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold.
    report_result_prefixed(result, label, "parse", report);
}

fn check_lisp_compiles(args: &[Sexp], root: &Path, report: &mut Report) {
    // Positional-`"path"` decode + `expected …` failure-arm shape rides
    // the ONE substrate primitive `required_positional_string` — one of
    // five sibling `check_*` executors that share the (positional-string
    // decode, report failure on absent slot, early-return control-flow)
    // triad on the same substrate owner.
    let Some(rel) = required_positional_string(
        args,
        0,
        "lisp-compiles",
        "expected (lisp-compiles \"path\" ...)",
        report,
    ) else {
        return;
    };
    let path = root.join(rel);
    let label = format!("Lisp compiles: {rel}");

    let kw = parse_kwargs(&args[1..]);
    let min_defs = find_kw(&kw, "min-definitions")
        .and_then(Sexp::as_int)
        .unwrap_or(1) as usize;
    let requires: Vec<String> = find_kw_string_list(&kw, "requires", Sexp::as_symbol);
    // Optional `:domain <name>` — selects which typed surface to compile.
    // Default `point` (ProcessSpec via `(defpoint …)`). Known peer:
    // `ephemeral` (EphemeralSpec via `(defephemeral …)`). Both arms
    // dispatch through the ONE `RequireTagDomain` trait rather than as
    // a two-arm `match` over the same (compile → min-defs → requires-
    // loop) pipeline restated per domain.
    let domain_name = find_kw(&kw, "domain")
        .and_then(|v| v.as_symbol().or_else(|| v.as_string()))
        .map(String::from)
        .unwrap_or_else(|| "point".into());

    let domain = match require_tag_domain_by_name(&domain_name) {
        Some(d) => d,
        None => {
            return report.fail(
                label,
                format!(
                    "unknown :domain {domain_name:?} (known: {})",
                    known_require_tag_domain_names()
                ),
            );
        }
    };

    // Read-side dispatch rides the ONE substrate primitive
    // `read_or_fail` — pre-lift this was a hand-authored 4-link
    // `match fs::read_to_string(&path) { Ok(s) => s, Err(e) => return
    // report.fail(label, format!("read: {e}")) }` chain, one of FOUR
    // workspace-wide restatements past the ★★ PRIME-DIRECTIVE ≥ 2
    // duplication threshold (peers at `check_yaml_parses` +
    // `check_yaml_parses_as` + `check_file_contains`). Post-lift the
    // read + `"read: {e}"` failure-arm shape lives at ONE substrate
    // owner.
    let Some(src) = read_or_fail(&path, &label, report) else {
        return;
    };

    let compiled = match domain.compile(&src) {
        Ok(c) => c,
        Err(e) => return report.fail(label, tatara_lisp::format_diagnostic(&src, &e, Some(rel))),
    };
    if let Some(err) = min_defs_shortfall_msg(compiled.count, min_defs) {
        return report.fail(label, err);
    }
    for req in &requires {
        let ok = match (compiled.classify)(req) {
            Ok(matched) => matched,
            Err(UnknownRequireTag) => {
                return report.fail(label, domain.unknown_tag_diagnostic(req));
            }
        };
        if !ok {
            return report.fail(label, format!("definition missing required: {req}"));
        }
    }

    report.pass(format!(
        "{label} ({} defs, {} checks)",
        compiled.count,
        requires.len()
    ));
}

fn check_file_contains(args: &[Sexp], root: &Path, report: &mut Report) {
    // Positional-`"path"` decode + `expected …` failure-arm shape rides
    // the ONE substrate primitive `required_positional_string` — one of
    // five sibling `check_*` executors that share the (positional-string
    // decode, report failure on absent slot, early-return control-flow)
    // triad on the same substrate owner.
    let Some(rel) = required_positional_string(
        args,
        0,
        "file-contains",
        "expected (file-contains \"path\" :strings (...))",
        report,
    ) else {
        return;
    };
    let path = root.join(rel);
    let label = format!("File contains: {rel}");

    let kw = parse_kwargs(&args[1..]);
    let strings: Vec<String> = find_kw_string_list(&kw, "strings", Sexp::as_string);
    if strings.is_empty() {
        return report.fail(label, ":strings (...) missing or empty");
    }

    // Read-side dispatch rides the ONE substrate primitive
    // `read_or_fail` — pre-lift this was a hand-authored 4-link
    // `match fs::read_to_string(&path) { Ok(s) => s, Err(e) => return
    // report.fail(label, format!("read: {e}")) }` chain, one of FOUR
    // workspace-wide restatements past the ★★ PRIME-DIRECTIVE ≥ 2
    // duplication threshold (peers at `check_yaml_parses` +
    // `check_yaml_parses_as` + `check_lisp_compiles`). Post-lift the
    // read + `"read: {e}"` failure-arm shape lives at ONE substrate
    // owner.
    let Some(src) = read_or_fail(&path, &label, report) else {
        return;
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

/// Sentinel returned by [`evaluate_point_require_tag`] when the tag
/// isn't one the point-domain require-tag surface understands. The
/// caller composes the operator-facing diagnostic (which echoes the
/// offending tag verbatim); the substrate owns only the classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UnknownRequireTag;

/// Classify one `:requires <tag>` entry against a compiled
/// [`tatara_process::crd::ProcessSpec`] and return whether the spec
/// satisfies it. `Ok(true)` — satisfied; `Ok(false)` — the spec
/// compiled but a required slot is missing (caller's
/// `definition missing required` diagnostic path); `Err(UnknownRequireTag)`
/// — the tag isn't in the point-domain vocabulary (caller's
/// `unknown :requires tag` diagnostic path).
///
/// # Vocabulary
///
/// Two closed-set-driven prefix families dispatch through the
/// autoderived `FromStr` + the substrate presence probe on their
/// respective parent:
///
/// - `intent-<kind>` — [`IntentKind`] closed set →
///   [`tatara_process::intent::Intent::has`] (a tagged-union
///   trait-default forwarder that pre-lift restated five hand-authored
///   `first.spec.intent.<field>.is_some()` arms; the sixth variant
///   `Guest` had no `intent-guest` arm at all before the lift).
/// - `lifetime-<kind>` — [`LifetimeKind`] closed set →
///   [`tatara_process::lifetime::Lifetime::has`] (an inherent peer that
///   mirrors the trait default verbatim; `Lifetime` deliberately doesn't
///   impl `TaggedUnion` because its resolver defaults empty to
///   `Ok(Permanent)`, not to an error). The `lifetime-<kind>` family
///   subsumes the pre-lift hand-authored `"lifetime-ephemeral" =>
///   first.spec.lifetime.is_ephemeral()` arm AND publishes the
///   `lifetime-permanent` tag for free — the symmetry gap against the
///   intent side is closed.
///
/// Every other tag is a fixed match on a non-closed-set spec field —
/// `depends-on`, `boundary-pre`, `boundary-post`, `compliance`,
/// `signals`. These stay as hand-authored arms until a matching
/// closed-set surface opens for them (each addresses a slot whose
/// carrier isn't a closed-set discriminator today).
///
/// A future third `IntentKind` or `LifetimeKind` variant lands at ONE
/// `ALL` entry + ONE `select` arm on its parent's closed set — no per-
/// caller edit here. A future new prefix family (e.g.
/// `signal-<kind>` for [`tatara_process::signal::SignalKind`]) lands
/// as ONE more `else if let Some(suffix) = req.strip_prefix("<prefix>-")`
/// branch that reads the same three-step (strip_prefix + parse + has)
/// shape both existing families publish.
///
/// Pinned by [`tests::evaluate_point_require_tag_returns_true_on_populated_lifetime_slot_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_default_lifetime_for_every_kind`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_lifetime_suffix`],
/// and [`tests::evaluate_point_require_tag_returns_unknown_on_bare_lifetime_prefix`].
fn evaluate_point_require_tag(
    spec: &tatara_process::crd::ProcessSpec,
    tag: &str,
) -> Result<bool, UnknownRequireTag> {
    if let Some(res) = strip_and_classify_prefixed_kind::<IntentKind, _>(tag, "intent-", |kind| {
        spec.intent.has(kind)
    }) {
        return res;
    }
    if let Some(res) =
        strip_and_classify_prefixed_kind::<LifetimeKind, _>(tag, "lifetime-", |kind| {
            spec.lifetime.has(kind)
        })
    {
        return res;
    }
    match tag {
        "depends-on" => Ok(!spec.depends_on.is_empty()),
        "boundary-pre" => Ok(!spec.boundary.preconditions.is_empty()),
        "boundary-post" => Ok(!spec.boundary.postconditions.is_empty()),
        "compliance" => Ok(!spec.compliance.bindings.is_empty()),
        "signals" => Ok(spec.signals.sigterm_grace_seconds > 0),
        _ => Err(UnknownRequireTag),
    }
}

/// Parse a `<prefix>-<suffix>` tag against a closed-set discriminator
/// `K` and hand the parsed kind to `probe` — the ONE substrate owner
/// of the `strip_prefix + parse::<K> + Ok/Err mapping` three-step
/// shape both closed-set-driven prefix families in
/// [`evaluate_point_require_tag`] (`intent-<kind>` on [`IntentKind`],
/// `lifetime-<kind>` on [`LifetimeKind`]) hand-authored past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold.
///
/// # Return shape
///
/// * `None` — `tag` does not begin with `<prefix>`; the caller falls
///   through to the next prefix family or the fixed-tag match tail.
/// * `Some(Ok(bool))` — prefix matched, suffix parsed to `K`,
///   `probe(kind)` yielded the presence answer.
/// * `Some(Err(UnknownRequireTag))` — prefix matched but the suffix
///   is not a canonical `K` label (a `lifetime-burst` typo, a bare
///   `lifetime-` with an empty suffix, an `intent-<misspelled>`).
///
/// # Why lift
///
/// The (strip_prefix + parse + Ok/Err) triad is a substrate primitive
/// on the (`&str`, closed-set `K`) algebra: the outer `Option`
/// discriminates "prefix hit" from "prefix miss" so the caller's
/// `if let Some(res) = …` early-return chain composes multiple prefix
/// families without restating the strip/parse boilerplate at each
/// branch. Both `IntentKind` and `LifetimeKind` implement
/// [`std::str::FromStr`] through
/// `#[derive(DeriveClosedSet)]`, so the bound `K: FromStr` catches
/// every workspace-wide closed-set discriminator by construction —
/// the parse-error type `<K as FromStr>::Err` is soft-mapped to
/// [`UnknownRequireTag`] so callers do not thread a per-K carrier
/// through the diagnostic prose.
///
/// # Compounding
///
/// A future third closed-set prefix family — `signal-<kind>` on
/// [`tatara_process::signal::SignalKind`], `phase-<kind>` on
/// [`tatara_process::phase::ProcessPhase`], `condition-<kind>` on
/// [`tatara_process::boundary::ConditionKind`] — lands as ONE more
/// `if let Some(res) = strip_and_classify_prefixed_kind::<NewKind, _>(
/// tag, "prefix-", |k| spec.<field>.has(k)) { return res; }` branch
/// that reads the same three-step shape both existing families
/// publish. No per-caller `strip_prefix + parse + match { Ok(_) =>
/// …, Err(_) => Err(UnknownRequireTag) }` restatement.
///
/// A future diagnostic shift (attaching the offending suffix to
/// [`UnknownRequireTag`], promoting the sentinel to carry a
/// `<K as ClosedSet>::labels_joined("/")` near-miss list) lands at
/// THIS ONE substrate owner and both current prefix families plus
/// every future closed-set prefix family inherit the shift by
/// construction.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// three-step chain recurred at TWO closed-set prefix families past
/// the ≥2 PRIME-DIRECTIVE trigger, and is lifted to ONE substrate
/// owner here. THEORY.md §II.1 invariant 2 — free middle; the caller
/// composes the closed-set choice (via the generic `K`) and the
/// presence probe (via the `probe` closure) independently, so a
/// regression that drifted one prefix family's strip/parse discipline
/// from the other becomes structurally impossible.
///
/// Pinned by [`tests::strip_and_classify_prefixed_kind_returns_none_when_prefix_does_not_match`],
/// [`tests::strip_and_classify_prefixed_kind_returns_ok_when_suffix_is_canonical`],
/// [`tests::strip_and_classify_prefixed_kind_returns_unknown_on_out_of_vocabulary_suffix`],
/// and [`tests::strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`].
fn strip_and_classify_prefixed_kind<K, F>(
    tag: &str,
    prefix: &str,
    probe: F,
) -> Option<Result<bool, UnknownRequireTag>>
where
    K: FromStr,
    F: FnOnce(K) -> bool,
{
    let suffix = tag.strip_prefix(prefix)?;
    Some(match suffix.parse::<K>() {
        Ok(kind) => Ok(probe(kind)),
        Err(_) => Err(UnknownRequireTag),
    })
}

/// Classify one `:requires <tag>` entry against a compiled
/// [`tatara_process::ephemeral::EphemeralSpec`] and return whether the
/// spec satisfies it. Peer of [`evaluate_point_require_tag`] on the
/// ephemeral domain axis — same `Result<bool, UnknownRequireTag>`
/// return shape so both surfaces route through the SAME classification-
/// error taxonomy at the check-executor boundary. `Ok(true)` —
/// satisfied; `Ok(false)` — a required slot is empty (caller's
/// `definition missing required` diagnostic path);
/// `Err(UnknownRequireTag)` — the tag isn't in the ephemeral-domain
/// vocabulary (caller's `unknown :requires tag for ephemeral domain`
/// diagnostic path).
///
/// # Vocabulary
///
/// Every tag is a fixed match on an [`EphemeralSpec`] slot; the
/// ephemeral surface deliberately doesn't have a closed-set prefix
/// family today (the sugar's own knobs — `aplicacao`, `ttl`,
/// `teardown`, `postconditions`, `preconditions` — aren't
/// discriminators of a closed set on `EphemeralSpec`). The vocabulary:
///
/// - `aplicacao` — the chart reference slot is populated
///   (`!spec.aplicacao.chart_ref.is_empty()`).
/// - `ttl` — the TTL slot is populated
///   (`!spec.ttl.is_empty()`). Pre-lift a default ephemeral (via
///   [`crate::lifetime::default_ephemeral_ttl`]'s `"1h"`) always
///   satisfies this; a hand-authored empty TTL is the failure path.
/// - `teardown` — always `Ok(true)`; [`TeardownPolicy`] is a typed
///   enum with a `Default` impl (there is no absent state to detect).
/// - `postconditions` / `preconditions` — the corresponding slot
///   carries at least one [`crate::boundary::Condition`].
/// - `closed-loop-auth` — at least one postcondition's `kind` is
///   [`ConditionKind::ClosedLoopAuth`]. A finer-grained pin than
///   `postconditions` — an ephemeral env with a HelmRelease-only
///   postcondition list passes the coarse tag but fails this one.
///
/// A future closed-set prefix family lands as one `else if let Some(
/// suffix) = tag.strip_prefix("<prefix>-")` branch that reads the same
/// three-step (strip_prefix + parse + has) shape
/// [`evaluate_point_require_tag`] publishes.
///
/// Pinned by [`tests::evaluate_ephemeral_require_tag_routes_populated_slots_true`],
/// [`tests::evaluate_ephemeral_require_tag_returns_false_on_empty_slots`],
/// [`tests::evaluate_ephemeral_require_tag_returns_unknown_on_out_of_vocabulary_tag`],
/// and [`tests::evaluate_ephemeral_require_tag_closed_loop_auth_reads_postcondition_kind`].
fn evaluate_ephemeral_require_tag(
    spec: &tatara_process::ephemeral::EphemeralSpec,
    tag: &str,
) -> Result<bool, UnknownRequireTag> {
    match tag {
        "aplicacao" => Ok(!spec.aplicacao.chart_ref.is_empty()),
        "ttl" => Ok(!spec.ttl.is_empty()),
        "teardown" => Ok(true),
        "postconditions" => Ok(!spec.postconditions.is_empty()),
        "preconditions" => Ok(!spec.preconditions.is_empty()),
        "closed-loop-auth" => Ok(spec.postconditions.iter().any(|c| {
            matches!(
                c.kind,
                tatara_process::boundary::ConditionKind::ClosedLoopAuth
            )
        })),
        _ => Err(UnknownRequireTag),
    }
}

/// Compile output handed back by [`RequireTagDomain::compile`] — the
/// typed-erased carrier both current domain impls and every future
/// `(lisp-compiles :domain <name>)` peer produce for the shared
/// (min-defs gate → requires-loop) pipeline in [`check_lisp_compiles`].
///
/// * `count` — the compiled-definitions count, consumed by
///   [`min_defs_shortfall_msg`] at the caller's shortfall rung + by
///   the caller's `"({count} defs, {N} checks)"` pass-summary prose.
/// * `classify` — the per-tag classifier closure, applied to the FIRST
///   compiled definition. Captures the owned typed vec by move so the
///   caller's classifier-invocation site is domain-agnostic; the
///   `Fn(&str) → Result<bool, UnknownRequireTag>` shape mirrors the
///   two peer `evaluate_<point,ephemeral>_require_tag` free functions'
///   return shape byte-for-byte at the ONE boundary the caller reads.
///
/// If `count == 0` and the caller invokes the closure, it panics on
/// the domain's `defs[0]` indexing — matches the pre-lift shape's
/// `let first = &defs[0]` semantics byte-for-byte. In practice the
/// [`min_defs_shortfall_msg`] gate short-circuits the caller before
/// classification whenever `min_defs >= 1` (the executor's default).
struct CompiledSource {
    count: usize,
    classify: Box<dyn Fn(&str) -> Result<bool, UnknownRequireTag>>,
}

/// The per-domain slice of the `(lisp-compiles ... :domain <name>)`
/// executor — a substrate primitive that owns three domain-specific
/// steps the shared pipeline in [`check_lisp_compiles`] threads through
/// ONE `&dyn RequireTagDomain` reference:
///
///   1. `name` — the domain's operator-facing keyword, matched against
///      the executor's `:domain <name>` slot at
///      [`require_tag_domain_by_name`] dispatch. Pinned literals:
///      `"point"` (ProcessSpec), `"ephemeral"` (EphemeralSpec).
///   2. `compile` — the Lisp-source → typed-spec-vec compile step, plus
///      the per-tag classifier closure that captures the first compiled
///      spec by move. Wraps [`tatara_process::compile_source`] for the
///      point domain and [`tatara_process::ephemeral::compile_ephemeral_source`]
///      for the ephemeral domain. Returns a [`CompiledSource`] that
///      exposes the domain-agnostic (count, classify) shape the outer
///      pipeline consumes.
///   3. `unknown_tag_diagnostic` — the operator-facing
///      `"unknown :requires tag[...]: <tag>"` prose composed when the
///      classifier returns [`UnknownRequireTag`]. Point-domain reads
///      `"unknown :requires tag: <tag>"`; ephemeral-domain reads
///      `"unknown :requires tag for ephemeral domain: <tag>"` — the
///      per-domain diagnostic-prose diverges here and here alone.
///
/// Object-safe: the return of [`compile`] is a concrete erased
/// [`CompiledSource`], not a generic-associated type — so a
/// `&'static dyn RequireTagDomain` is dispatchable at
/// [`require_tag_domain_by_name`]'s registry match.
///
/// Adding a NEW `(lisp-compiles :domain <new>)` peer domain is now
/// ONE trait impl + ONE arm on [`require_tag_domain_by_name`] — no
/// per-domain restatement of the (compile → min-defs → requires-loop)
/// pipeline in [`check_lisp_compiles`]. The peer classifier
/// (`evaluate_<new>_require_tag`) is already the substrate primitive
/// this trait's `compile` step wraps; the shared `UnknownRequireTag`
/// error type carries through unchanged. Pre-lift the two arms
/// restated the (five-line compile match, three-line min-defs gate,
/// nine-line requires-loop) shape verbatim past the ★★ PRIME-DIRECTIVE
/// ≥ 2 duplication threshold — post-lift both arms + every future
/// peer route through ONE substrate owner.
///
/// Theory anchor: THEORY.md §II.1 invariant 2 — free middle; the
/// domain-specific compile + classify + diagnostic-prose steps are
/// pure functions on `(&str, &str) → …`, so a future consumer
/// (a documentation generator that lists every domain's require-tag
/// vocabulary, an editor completion provider suggesting known domain
/// names, a linter that flags a `:requires <tag>` against the domain's
/// known vocabulary before submission) routes through the SAME
/// primitive rather than restating the dispatch table. THEORY.md
/// §II.1 invariant 5 — composition preserves proofs; the shared
/// [`CompiledSource`] return shape means the executor's pipeline
/// discipline (min-defs gate, requires-loop, pass-summary prose)
/// binds ONCE for every current AND future domain.
trait RequireTagDomain: Sync {
    fn name(&self) -> &'static str;
    fn compile(&self, src: &str) -> tatara_lisp::Result<CompiledSource>;
    fn unknown_tag_diagnostic(&self, tag: &str) -> String;
}

/// [`RequireTagDomain`] impl for the point (ProcessSpec) surface —
/// wraps [`tatara_process::compile_source`] + [`evaluate_point_require_tag`]
/// + the pre-lift `"unknown :requires tag: <tag>"` diagnostic prose.
struct PointDomain;

impl RequireTagDomain for PointDomain {
    fn name(&self) -> &'static str {
        "point"
    }
    fn compile(&self, src: &str) -> tatara_lisp::Result<CompiledSource> {
        let defs = tatara_process::compile_source(src)?;
        let count = defs.len();
        Ok(CompiledSource {
            count,
            classify: Box::new(move |tag| evaluate_point_require_tag(&defs[0].spec, tag)),
        })
    }
    fn unknown_tag_diagnostic(&self, tag: &str) -> String {
        format!("unknown :requires tag: {tag}")
    }
}

/// [`RequireTagDomain`] impl for the ephemeral (EphemeralSpec) surface
/// — wraps [`tatara_process::ephemeral::compile_ephemeral_source`] +
/// [`evaluate_ephemeral_require_tag`] + the pre-lift
/// `"unknown :requires tag for ephemeral domain: <tag>"` diagnostic
/// prose.
struct EphemeralDomain;

impl RequireTagDomain for EphemeralDomain {
    fn name(&self) -> &'static str {
        "ephemeral"
    }
    fn compile(&self, src: &str) -> tatara_lisp::Result<CompiledSource> {
        let defs = tatara_process::ephemeral::compile_ephemeral_source(src)?;
        let count = defs.len();
        Ok(CompiledSource {
            count,
            classify: Box::new(move |tag| evaluate_ephemeral_require_tag(&defs[0].spec, tag)),
        })
    }
    fn unknown_tag_diagnostic(&self, tag: &str) -> String {
        format!("unknown :requires tag for ephemeral domain: {tag}")
    }
}

static POINT_DOMAIN: PointDomain = PointDomain;
static EPHEMERAL_DOMAIN: EphemeralDomain = EphemeralDomain;

/// Closed-set registry of every [`RequireTagDomain`] the
/// [`check_lisp_compiles`] executor recognizes. Iterating this slice
/// is how [`require_tag_domain_by_name`] resolves a name AND how the
/// executor composes the operator-facing "known: <names>" diagnostic
/// suffix on an unknown `:domain` slot. A future peer domain lands as
/// ONE static + ONE entry in this slice — the registry, the name-based
/// dispatch, AND the diagnostic-suffix all inherit the new domain in
/// lockstep. Ordering here defines the diagnostic-suffix ordering
/// (point first, ephemeral second) — matches the pre-lift
/// hand-authored `"known: point, ephemeral"` literal byte-for-byte.
static ALL_REQUIRE_TAG_DOMAINS: &[&'static dyn RequireTagDomain] =
    &[&POINT_DOMAIN, &EPHEMERAL_DOMAIN];

/// Resolve a `(lisp-compiles ... :domain <name>)` slot value to a
/// `&'static dyn RequireTagDomain` by name-equality sweep over
/// [`ALL_REQUIRE_TAG_DOMAINS`]. Returns `None` for a name outside the
/// closed set; the caller composes the operator-facing
/// `"unknown :domain <name> (known: <names>)"` diagnostic — where the
/// known-names list is itself computed from [`ALL_REQUIRE_TAG_DOMAINS`]
/// — around the `None` return, so the two surfaces (dispatch + known-
/// list diagnostic) can never drift out of sync.
fn require_tag_domain_by_name(name: &str) -> Option<&'static dyn RequireTagDomain> {
    ALL_REQUIRE_TAG_DOMAINS
        .iter()
        .copied()
        .find(|d| d.name() == name)
}

/// Comma-separated join of every registered domain's [`RequireTagDomain::name`]
/// for the operator-facing `"(known: <names>)"` diagnostic suffix. A
/// future peer domain shows up in this suffix mechanically through the
/// [`ALL_REQUIRE_TAG_DOMAINS`] iteration — no per-suffix restatement of
/// the closed-set names literal.
fn known_require_tag_domain_names() -> String {
    ALL_REQUIRE_TAG_DOMAINS
        .iter()
        .map(|d| d.name())
        .collect::<Vec<_>>()
        .join(", ")
}

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

/// Soft-borrow the string atom at positional `index` on an executor's
/// `args` slice — the ONE substrate owner of the two-token
/// `args.<get|first>(N).and_then(Sexp::as_string)` chain the five
/// `check_*` executors ([`check_crd_in_sync`] pos 1, [`check_yaml_parses`]
/// pos 0, [`check_yaml_parses_as`] pos 1, [`check_lisp_compiles`] pos 0,
/// [`check_file_contains`] pos 0) hand-authored past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold to decode a `"path"` slot
/// from a positional argument.
///
/// Semantics — byte-identical to the pre-lift chain:
///
/// * `index` out of bounds (`args.len() <= index`) → [`None`] (soft
///   failure through `args.get(index)`).
/// * `args[index]` present but NOT a [`Sexp::Str`] (a bare symbol, a
///   nested list, an integer, a keyword) → [`None`] (soft failure
///   through `and_then(Sexp::as_string)`).
/// * `args[index]` is a string atom → borrowed `&str` payload with the
///   input slice's lifetime.
///
/// The return-lifetime is the input slice's lifetime — every current
/// caller's downstream `root.join(rel)` path composition, per-check
/// `format!("<check>: {rel}", ...)` label building, and
/// `report.fail(...)` diagnostic embedding consume the borrowed `&str`
/// without an intervening allocation.
///
/// Positional-string peer of [`head_symbol_or_missing`] on the
/// (positional × [`Sexp`] projection) family — where
/// [`head_symbol_or_missing`] is the substrate primitive for
/// "resolve position 0 to a `<Kind>` symbol with a sentinel fallback",
/// THIS primitive is the peer on the STRING axis at arbitrary
/// position, projecting to `Option<&str>` with soft failure so the
/// caller composes its own per-check `report.fail(label, expected-form)`
/// diagnostic. Closes the (symbol × head, string × any-position)
/// corner of the positional-arg algebra so a future primitive on the
/// third corner (a hypothetical `head_int_or_default` for an integer
/// head-arg, a `positional_symbol` for a symbol at position N) lands
/// as a peer at THIS module without further duplication on the
/// `check_*` executor surface. Sibling of [`find_kw`] on the
/// kwarg-slot axis: [`find_kw`] resolves a `:name` slot to
/// `Option<&Sexp>`; THIS primitive resolves a positional slot to
/// `Option<&str>`; the two together cover both positional AND
/// keyword-slot decodes for every `check_*` executor.
///
/// A future normalization (a promotion of the return to a typed
/// `Result<&str, PositionalDecodeError>` shape that forces every
/// caller to path through a structured error, a switch to a projection
/// param `proj: fn(&Sexp) -> Option<&str>` that would fold this
/// primitive with [`head_symbol_or_missing`] on the shared
/// (positional × projection) axis, a near-miss hint when the slot is
/// present but the wrong shape) lands at THIS ONE substrate primitive
/// and every current caller plus every future positional-string
/// executor inherits the upgrade mechanically.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// two-token `args.<get|first>(N).and_then(Sexp::as_string)` chain
/// recurred at FIVE executor bodies past the ≥2 PRIME-DIRECTIVE
/// trigger, and is lifted to ONE substrate owner here.
/// THEORY.md §II.1 invariant 5 — composition preserves proofs; all
/// five callers routing through the SAME substrate primitive means a
/// future projection or lifetime shift lands at ONE site and every
/// downstream positional-string consumer inherits the shift by
/// construction.
fn positional_string(args: &[Sexp], index: usize) -> Option<&str> {
    args.get(index).and_then(Sexp::as_string)
}

/// Decode the string atom at positional `index` on an executor's `args`
/// slice AND report a `<label>: <expected>` failure through `report` when
/// the slot is absent or non-string — the ONE substrate owner of the
/// let-else + `report.fail(<label>, <expected-usage>)` three-line early-
/// return shape all five `check_*` executors hand-authored past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold on top of the
/// [`positional_string`] soft-borrow primitive:
///
/// * [`check_crd_in_sync`] pos 1 → label `"crd-in-sync"`, expected form
///   `"expected (crd-in-sync <Kind> \"path\")"`.
/// * [`check_yaml_parses`] pos 0 → label `"yaml-parses"`, expected form
///   `"expected (yaml-parses \"path\")"`.
/// * [`check_yaml_parses_as`] pos 1 → label `"yaml-parses-as"`, expected
///   form `"expected (yaml-parses-as <Kind> \"path\")"`.
/// * [`check_lisp_compiles`] pos 0 → label `"lisp-compiles"`, expected
///   form `"expected (lisp-compiles \"path\" ...)"`.
/// * [`check_file_contains`] pos 0 → label `"file-contains"`, expected
///   form `"expected (file-contains \"path\" :strings (...))"`.
///
/// All FIVE sites walked the SAME three-link chain — call
/// [`positional_string`] at the executor's positional slot, match `None`
/// through `return report.fail(<per-check label>, <per-check expected
/// form>)`, and forward the borrowed `&str` on the `Some` arm to the
/// downstream `root.join(rel)` / `format!("<check>: {rel}")` label
/// composition — differing only in the constant (label, expected-form)
/// pair and the positional index each check reads. Post-lift each
/// callsite reads
/// `let Some(rel) = required_positional_string(args, N, LABEL, EXPECTED, report) else { return; };`
/// and the (positional-string decode, report failure on absent slot,
/// early-return control-flow) triad lives at ONE substrate owner.
///
/// Semantics — byte-identical to the pre-lift shape:
///
/// * [`positional_string`] returns [`Some`] → forward the borrowed `&str`
///   verbatim; no `report` side-effect on the path.
/// * [`positional_string`] returns [`None`] → hang `report.fail(label,
///   expected)` off the failure arm (the pre-lift static-string label +
///   static-string expected-usage message reach [`Report::fail`]'s
///   `impl Display` slots byte-for-byte through `&str`'s [`Display`]
///   impl) and return [`None`] as the control-flow signal every caller
///   consumes through the let-else idiom to early-return.
///
/// Sibling of [`read_or_fail`] on the (side-effecting-decode + let-else
/// early-return) axis: [`read_or_fail`] owns the read-side "read file
/// or report a `read: {e}` failure" shape; THIS primitive owns the
/// arg-decode-side "borrow a positional `\"path\"` slot or report an
/// `expected …` failure" shape. Both pair a side-effecting soft-failure
/// decode with a `Report::fail` side-effect and an [`Option`] return
/// the caller unwraps through the same `let Some(...) else { return; };`
/// idiom — the (side-effecting-decode, side-effecting-report,
/// [`None`]-as-control-flow) triad becomes the load-bearing shape of the
/// tatara-check executor family. Pure-diagnostic peers
/// ([`min_defs_shortfall_msg`], [`head_symbol_or_missing`]) sit on the
/// (compose-diagnostic-without-side-effect) side of the same axis.
///
/// A future refinement (a near-miss hint composed off the missing slot's
/// actual [`Sexp`] shape — `"expected \"path\", got symbol"` when the
/// operator quoted the wrong atom kind, a swap to a typed
/// [`Result<&str, PositionalDecodeError>`] shape carrying both the
/// per-check label AND the observed `Sexp` variant, a `tracing`-
/// annotated span covering the decode attempt) lands at THIS ONE
/// substrate primitive and every current caller plus every future
/// positional-`"path"`-decoding executor (a `check_json_parses`, a
/// `check_toml_parses`, a `check_nix_evaluates` reading a `.nix` source
/// at a positional slot) inherits the upgrade mechanically. No per-
/// site restatement of the `positional_string + report.fail + return`
/// shape at any of the five current callers or at future consumers.
///
/// The `label` + `expected` slots are `&str` matching each caller's
/// static-string constants (every one of the five migrated callers
/// passes a `&'static str` literal today) via deref-coercion so a
/// future caller composing a runtime label (e.g. an executor whose
/// label carries the resolved path prefix) can still hand its owned
/// [`String`] through the `&str` slot without an intervening
/// allocation — the primitive doesn't need to own either slot.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// let-else + `report.fail(label, expected)` early-return chain
/// recurred at 5 hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication trigger and is lifted onto ONE substrate owner here).
/// THEORY.md §II.1 invariant 5 (composition preserves proofs — the
/// pin block below binds the primitive at fail-before-pass-after
/// granularity so a regression that drifted the `Report::fail`
/// side-effect on the `None` arm, dropped the borrowed-lifetime
/// forwarding on the `Some` arm, or flipped the return polarity
/// surfaces HERE rather than as silent operator-facing diagnostic
/// skew at each of the five consumer sites).
fn required_positional_string<'a>(
    args: &'a [Sexp],
    index: usize,
    label: &str,
    expected: &str,
    report: &mut Report,
) -> Option<&'a str> {
    match positional_string(args, index) {
        Some(s) => Some(s),
        None => {
            report.fail(label, expected);
            None
        }
    }
}

/// Render the [`check_lisp_compiles`] `:min-definitions` shortfall
/// diagnostic when `defs_len < min_defs`, else `None` — the ONE
/// substrate owner of the four-line
/// `if defs.len() < min_defs { return report.fail(label, format!("expected ≥ {} definitions, got {}", min_defs, defs.len())); }`
/// chain both the `point` (ProcessSpec) and `ephemeral` (EphemeralSpec)
/// arms of [`check_lisp_compiles`] hand-authored past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold to enforce the
/// operator-declared minimum on the compiled-definitions count.
///
/// Semantics — byte-identical to the pre-lift shape:
///
/// * `defs_len >= min_defs` → [`None`] (the caller falls through to
///   the next verification rung; no `report.fail` on the path).
/// * `defs_len < min_defs`  → [`Some`] carrying the exact pre-lift
///   `format!("expected ≥ {} definitions, got {}", min_defs, defs_len)`
///   diagnostic string. The caller composes its own `report.fail(label,
///   ...)` around it so the per-arm label prose (`"Lisp compiles: {rel}"`
///   built earlier in [`check_lisp_compiles`]) stays where it is.
///
/// Sibling of [`head_symbol_or_missing`] on the report-primitive axis:
/// [`head_symbol_or_missing`] extracts a `<Kind>` slug for downstream
/// error prose; THIS primitive composes a numeric shortfall diagnostic
/// for a domain-agnostic gate. Both are byte-identical-parity lifts on
/// hand-authored chains that were restated verbatim past the ≥ 2
/// PRIME-DIRECTIVE trigger across sibling `check_*` executor arms.
///
/// A future refinement (a companion `:max-definitions` gate emitting
/// `"expected ≤ N definitions, got M"`, an exact-count gate, a swap to
/// a typed [`Result<usize, ShortfallReason>`] shape so each caller
/// picks off the numeric axis, an operator-hint suffix like
/// `"; add another (def<kind> …) form to <path>"`) lands at THIS ONE
/// substrate owner and both current arms plus every future `:domain`
/// arm added to [`check_lisp_compiles`] inherit the shift by
/// construction — no per-arm restatement of the shortfall-format
/// discipline.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; two
/// byte-identical four-line inline compositions collapse onto ONE
/// substrate owner past the ≥ 2 PRIME-DIRECTIVE trigger. THEORY.md
/// §II.1 invariant 5 — composition preserves proofs; both current
/// arms routing through the SAME substrate primitive means a future
/// diagnostic-shape shift lands at ONE site and every downstream
/// definition-count consumer inherits it by construction.
fn min_defs_shortfall_msg(defs_len: usize, min_defs: usize) -> Option<String> {
    (defs_len < min_defs).then(|| format!("expected ≥ {min_defs} definitions, got {defs_len}"))
}

/// Read `path` as UTF-8 text, or report a `<label>: read: {io_error}`
/// failure on the `Err(io::Error)` arm and return [`None`] so the
/// caller can early-return through the standard let-else idiom.
///
/// The ONE substrate owner of the 4-link chain
/// `let src = match fs::read_to_string(&path) { Ok(s) => s, Err(e) =>
/// return report.fail(label, format!("read: {e}")) }` every check
/// executor that reads its input file walks. The prior lift routed
/// three consumer sites through the primitive; this run closes the
/// symmetry gap on the fourth ([`check_yaml_parses`]) whose pre-lift
/// match wrapped the pass path inline rather than early-returning,
/// so all FOUR read-consuming executors now share ONE substrate
/// owner past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold:
///
/// * [`check_yaml_parses`] — the `(yaml-parses "path")` executor.
///   Reads the operator-supplied YAML file before handing the source
///   to `serde_yaml::from_str::<serde_yaml::Value>` for a schema-free
///   parse. Pre-lift wrapped the pass arm inline through
///   `match fs::read_to_string(&path) { Ok(src) => <inline-pass>,
///   Err(e) => report.fail(label, format!("read: {e}")) }` — the
///   fourth site the earlier lift missed because the match shape
///   diverged from the peer sites' let-else early-return shape.
///   Post-lift the executor early-returns through the substrate and
///   its pass path (`serde_yaml::from_str` + `report.pass` /
///   `report.fail(label, format!("YAML: {e}"))`) stays inline.
/// * [`check_yaml_parses_as`] — the `(yaml-parses-as <Kind> "path")`
///   executor. Reads the operator-supplied YAML file before handing
///   the source to [`KnownCrd::parse_yaml_as`] for the typed-CRD parse.
/// * [`check_lisp_compiles`] — the `(lisp-compiles "path" ...)`
///   executor. Reads the operator-supplied Lisp source file before
///   handing it to the [`RequireTagDomain::compile`] dispatch (either
///   the `point` `ProcessSpec` arm or the `ephemeral` `EphemeralSpec`
///   arm).
/// * [`check_file_contains`] — the `(file-contains "path" :strings
///   (...))` executor. Reads the target file before the substring-
///   presence sweep across the operator-declared `:strings` list.
///
/// All FOUR sites walked the SAME four-link chain — read the file
/// as UTF-8, then match `Ok(s) => s` in the pass arm and `Err(e) =>
/// return report.fail(<pre-composed label>, format!("read: {e}"))`
/// in the fail arm — differing only in the `<path>` operand and in
/// the caller's pre-computed `<label>` string. Post-lift each
/// callsite reads
/// `let Some(src) = read_or_fail(&path, &label, report) else {
/// return; };` and the read + `"read: {e}"` failure-arm shape lives
/// at ONE substrate owner.
///
/// Sibling of [`min_defs_shortfall_msg`] on the report-primitive
/// axis: [`min_defs_shortfall_msg`] composes a numeric shortfall
/// diagnostic without touching the [`Report`] itself (the caller
/// hangs the `report.fail(label, msg)` off its `Some` arm); this
/// primitive owns BOTH the read attempt AND the [`Report::fail`]
/// side-effect on the `Err(io::Error)` arm, returning [`None`] as
/// the control-flow signal the caller consumes through the let-else
/// idiom to early-return. The two primitives partition the report-
/// side helper axis at the (pure-diagnostic-composer, side-effecting-
/// I/O-with-side-effecting-report) split — every future check
/// executor that needs the "read source or report a `read: {e}`
/// failure" shape composes through this primitive without restating
/// the four-link chain, and every future numeric shortfall gate
/// composes through the sibling.
///
/// A future normalization of the read-side chain (a per-fleet
/// max-file-size guard that refuses inputs larger than a policy
/// bound before parsing; a per-file mtime cache that skips
/// re-reading unchanged sources across an incremental
/// `tatara-check` pass; a swap to `fs::read` + `String::from_utf8`
/// with a caller-visible "not UTF-8" diagnostic distinct from the
/// current `io::Error`-wrapped path; a `tracing`-annotated span
/// carrying the label + path for post-hoc audit) lands at THIS ONE
/// substrate owner and every downstream check executor — the three
/// current callsites plus every future `check_<name>` executor —
/// inherits the upgrade mechanically. No per-site edit at any of
/// the THREE listed callers or at future consumers (a
/// `check_json_parses` executor for JSON coherence, a
/// `check_nix_evaluates` executor for a Nix-side `nix-eval` gate,
/// a `check_toml_parses` for a `Cargo.toml` schema drift probe).
///
/// The `label: &str` slot is `&str` matching the caller's pre-
/// composed `String` labels (`format!("YAML parses as {kind}: {rel}")`
/// / `format!("Lisp compiles: {rel}")` / `format!("File contains:
/// {rel}")`) via deref-coercion so the caller can still consume its
/// owned `String` label on the caller-side success path (the
/// `report.pass(label)` / `report.fail(label, ...)` calls that
/// follow the read).
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the four-link read + fail chain recurred at 4 hand-authored
/// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and
/// is lifted onto ONE substrate owner here). THEORY.md §II.1
/// invariant 5 (composition preserves proofs — the pin block below
/// binds the primitive at fail-before-pass-after granularity so a
/// regression that drifted the `"read: {e}"` prefix, dropped the
/// `report.fail` side-effect on the `Err` arm, or flipped the
/// return polarity surfaces HERE rather than as silent operator-
/// facing diagnostic skew at each of the four consumer sites).
fn read_or_fail(path: &Path, label: &str, report: &mut Report) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) => {
            report.fail(label, format!("read: {e}"));
            None
        }
    }
}

/// Terminal `Result<T, E: Display>` sink that owns the (pass on `Ok`,
/// fail with `<prefix>: {e}` on `Err`, label consumed by both arms)
/// triad every `check_*` executor walks after routing its typed-parse
/// result through the shared pass/fail report boundary.
///
/// The ONE substrate owner of the 4-link chain
/// `match RESULT { Ok(_) => report.pass(label), Err(e) =>
/// report.fail(label, format!("<prefix>: {e}")) }` every
/// yaml-executor-family check hand-authored past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold:
///
/// * [`check_yaml_parses`] — the `(yaml-parses "path")` executor.
///   Consumes `Result<serde_yaml::Value, serde_yaml::Error>` from
///   `serde_yaml::from_str`. Prefix literal `"YAML"`. Pre-lift
///   the `Err(e)` arm hand-authored `format!("YAML: {e}")`.
/// * [`check_yaml_parses_as`] — the `(yaml-parses-as <Kind> "path")`
///   executor. Consumes `Result<(), Box<dyn std::error::Error>>`
///   (`E` erased through the trait object) from
///   [`KnownCrd::parse_yaml_as`]. Prefix literal `"parse"`. Pre-lift
///   the `Err(e)` arm hand-authored `format!("parse: {e}")`.
///
/// Both sites walked the SAME four-link chain — match the typed
/// result, `report.pass(label)` on the `Ok` arm, `report.fail(label,
/// format!("<prefix>: {e}"))` on the `Err` arm — differing only in
/// the caller's `<T>` payload (`serde_yaml::Value` vs `()`), the
/// caller's `<E>` carrier (`serde_yaml::Error` vs the boxed error
/// [`KnownCrd::parse_yaml_as`] returns), and the diagnostic prefix
/// literal (`"YAML"` vs `"parse"`). Post-lift each callsite reads
/// `report_result_prefixed(<result>, label, "<prefix>", report);`
/// and the (pass on `Ok`, fail with `<prefix>: {e}` on `Err`, label
/// consumed by both arms) triad lives at ONE substrate owner.
///
/// # Ownership contract
///
/// `label: String` is consumed by the primitive — both terminal arms
/// need it exactly once (the `Ok` arm passes it into
/// [`Report::pass`] via `impl Into<String>`, the `Err` arm passes
/// it into [`Report::fail`] via `impl Display`), so the caller
/// hands over ownership at the substrate boundary. The peer
/// substrate primitive [`read_or_fail`] takes its label by `&str`
/// because BOTH the read side AND the subsequent
/// pass/fail-on-parse-result site need to consume the label — the
/// caller's `let label = format!(…)` slot lives across both
/// substrate calls. This primitive is the terminal sink; no
/// further site needs the label after it lands.
///
/// # `T` payload is discarded
///
/// The `Ok` arm ignores the payload — every yaml-executor-family
/// consumer's success path composes its pass entry from the label
/// alone (no data from the successful parse leaks into the pass
/// prose). The generic `<T>` slot exists to accept every consumer's
/// concrete payload type without forcing the caller to
/// `.map(|_| ())` at the call boundary; the primitive discards it
/// inside the `Ok(_)` arm. If a future consumer needs the payload
/// on the pass side (a pass-prose that embeds a count or a name
/// from the parsed value), it walks the peer substrate primitive
/// [`Report::pass`] directly rather than this one — the two
/// primitives partition the terminal-report axis at the
/// (payload-carrying, payload-discarding) split.
///
/// # `E` carrier
///
/// `E: std::fmt::Display` — accepts both consumer's carriers:
/// `serde_yaml::Error` implements [`std::fmt::Display`] directly,
/// and the `Box<dyn std::error::Error>` [`KnownCrd::parse_yaml_as`]
/// returns implements it through the [`std::error::Error`] super-
/// trait bound. The `format!("{err_prefix}: {e}")` composition
/// materializes at the primitive without allocating an
/// intermediate `String` at the caller.
///
/// # Compounding
///
/// A future normalization of the terminal-report chain (a
/// `tracing`-annotated failure span carrying label + prefix + error
/// for post-hoc audit; a structured-log emit alongside the pass /
/// fail entry; a per-executor histogram of pass vs fail counts; a
/// swap to a typed error taxonomy that distinguishes
/// serde_yaml::Error kinds instead of the current
/// stringly-typed Display) lands at THIS ONE substrate owner and
/// every downstream yaml-executor consumer — the two current
/// callsites plus every future `check_<name>` executor whose
/// terminal shape matches the (pass on `Ok`, fail with
/// `<prefix>: {e}` on `Err`) triad — inherits the upgrade
/// mechanically. Candidate future consumers: a `check_json_parses`
/// executor for JSON coherence (`serde_json::Error` on the `Err`
/// arm, `"JSON"` prefix), a `check_toml_parses` executor for
/// `Cargo.toml` schema drift (`toml::de::Error`, `"TOML"`), a
/// `check_ron_parses` executor for a Rusty-Object-Notation gate,
/// or any future typed-parse coherence probe whose diagnostic
/// prose follows the `<label>: <prefix>: <error>` shape.
///
/// # Sibling to [`read_or_fail`]
///
/// [`read_or_fail`] owns the (read attempt, [`Report::fail`]
/// side-effect on I/O error, `None` control-flow signal for
/// let-else early-return) triad on the read-side of the
/// executor pipeline. `report_result_prefixed` owns the peer
/// (pass on `Ok`, fail with `<prefix>: {e}` on `Err`, label
/// consumed by both arms) triad on the terminal-report side. The
/// two primitives partition the executor pipeline at the
/// (read-side early-return, terminal-side pass/fail) split — every
/// current + future check executor whose shape matches
/// `read → parse → pass/fail` composes through both.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the four-link terminal `Result` sink recurred at 2 hand-
/// authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger and is lifted onto ONE substrate owner here).
/// THEORY.md §II.1 invariant 5 (composition preserves proofs —
/// both consumers routing through the SAME substrate primitive
/// means a future diagnostic-shape shift lands at ONE site and
/// every downstream terminal-report consumer inherits the shift
/// by construction).
fn report_result_prefixed<T, E>(
    result: Result<T, E>,
    label: String,
    err_prefix: &str,
    report: &mut Report,
) where
    E: std::fmt::Display,
{
    match result {
        Ok(_) => report.pass(label),
        Err(e) => report.fail(label, format!("{err_prefix}: {e}")),
    }
}

/// The pure `"tatara-check: {detail}"` startup-diagnostic prefix — the
/// ONE substrate owner of the exact stderr-facing prose every
/// [`main`]-side startup bail writes, split off from [`startup_bail`]
/// so the byte shape can be pinned without capturing stderr in tests.
///
/// # Prefix
///
/// `"tatara-check: "` — the binary's name, colon, single space. Every
/// pre-lift `eprintln!("tatara-check: <detail>")` chain in [`main`]
/// used this exact prefix, and every downstream check-log grep keys
/// on it as the "startup-side, not per-check" sentinel (per-check
/// failures reach stderr through [`Report`]'s `"✗ <label>: <detail>"`
/// shape instead). Byte-preserving through the sibling
/// [`startup_bail`]'s `eprintln!("{}", startup_diagnostic(...))`
/// composition means a future prefix shift (a
/// `"tatara-check[<version>]:"` build-stamped prefix, a swap to
/// `"[tatara-check] "` for structured-log parity) lands at THIS ONE
/// substrate owner and every startup-side bail inherits it by
/// construction.
///
/// # Detail forwarding
///
/// The `impl std::fmt::Display` bound accepts every carrier the four
/// pre-lift callsites walked:
///
/// * A bare `&'static str` literal (workspace-root miss:
///   `"could not locate workspace root (looked for Cargo.toml + checks.lisp)"`).
/// * A `format_args!(...)` inline composition (checks.lisp read /
///   parse / macroexpand fails: `format_args!("read {}: {e}",
///   checks_path.display())` etc.). `std::fmt::Arguments` implements
///   `Display` and can be passed through `impl Display` at the call
///   boundary without allocating an intermediate `String`.
/// * An owned `String` composed by the caller (a future startup gate
///   that pre-computes its detail before the bail decision).
///
/// The primitive owns exactly the (prefix, separator) sink; each caller
/// composes its detail's own shape through its own `format_args!` /
/// `format!` at the callsite. That keeps the primitive domain-agnostic
/// — it doesn't know about `checks_path.display()`, `chrono::Duration`,
/// or any other detail-side type — while still collapsing the shared
/// prefix + colon + space sink onto ONE owner.
///
/// # Sibling to [`Report::fail`]
///
/// [`Report::fail`] owns the per-check `"<label>: <detail>"` shape for
/// executor-side failures (`"crd-in-sync Process: <detail>"`,
/// `"YAML parses as Process: <detail>"`, etc.) that reach the workspace
/// report and print through the `"✗ <line>"` iteration in [`main`].
/// [`startup_diagnostic`] owns the peer `"tatara-check: <detail>"`
/// shape for the startup-side failures that must reach stderr BEFORE
/// the [`Report`] exists (workspace-root discovery, checks.lisp read /
/// parse, macroexpand) — the two shapes partition the stderr surface
/// at the (before-Report, after-Report) split. A per-check failure and
/// a startup-side failure remain distinguishable by prefix alone at
/// the reader.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// `"tatara-check: "` prefix + colon separator recurred at 4 hand-
/// authored `eprintln!` sites in [`main`] past the ★★ PRIME-DIRECTIVE
/// ≥ 2 duplication trigger and is lifted onto ONE substrate owner
/// here). THEORY.md §II.1 invariant 5 (composition preserves proofs —
/// the pins bind the byte shape at fail-before-pass-after granularity
/// so a regression that drifted the prefix, dropped the colon-space
/// separator, or dropped the detail forwarding surfaces HERE rather
/// than as silent operator-visible drift across every downstream
/// startup-log grep).
#[must_use]
fn startup_diagnostic(detail: impl std::fmt::Display) -> String {
    format!("tatara-check: {detail}")
}

/// The startup-side bail primitive — write a `"tatara-check: {detail}"`
/// diagnostic to stderr and return the [`ExitCode::from(2)`]
/// configuration-failure exit code the shell reads as "the binary
/// could not even reach the per-check dispatcher, treat this as an
/// operator-side setup fault distinct from a `1` per-check-failure
/// exit". The ONE substrate owner of the two-step
/// `eprintln!("tatara-check: <detail>"); return ExitCode::from(2);`
/// chain [`main`] hand-authored at 4 sites pre-lift past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold:
///
/// * `workspace_root()` returned `None` — no `Cargo.toml + checks.lisp`
///   pair on the ancestor path from CWD.
/// * `fs::read_to_string(&checks_path)` failed — the discovered
///   `checks.lisp` file could not be read as UTF-8.
/// * `read(&src)` failed — the read `checks.lisp` source did not parse
///   as a stream of S-expressions.
/// * `expander.expand_program(raw)` failed — the parsed forms did not
///   macroexpand (a `(defcheck …)` shape violation, an unbound
///   macro-tail keyword, a `&rest` shape mismatch).
///
/// All 4 sites walked the SAME two-step chain — write a
/// `"tatara-check: <detail>"` line to stderr, then return
/// `ExitCode::from(2)` — differing only in the detail's own shape
/// (`&'static str` for the workspace-root miss; `format_args!` inline
/// composition for the three per-error-arm cases). Post-lift each
/// callsite reads `return startup_bail(<detail>);` and the (prefix,
/// separator, exit-code) triad lives at ONE substrate owner.
///
/// # Delegation to [`startup_diagnostic`]
///
/// The `"tatara-check: {detail}"` byte shape lives at the pure formatter
/// [`startup_diagnostic`]; this primitive supplies the stderr write +
/// the config-failure exit code and delegates. The delegation split
/// makes the byte shape fully testable at [`startup_diagnostic`]
/// without needing to capture stderr; this primitive's own body reduces
/// to a two-line `eprintln!` + `ExitCode::from(2)` shape that is
/// inspection-checkable.
///
/// # Exit code
///
/// `ExitCode::from(2)` — the configuration-failure code. Distinct from
/// [`ExitCode::SUCCESS`] (all checks passed) and [`ExitCode::FAILURE`]
/// (at least one check failed but the dispatcher itself ran). A wrapper
/// script grepping the exit code (`if [ $? -eq 2 ]; then ...`) can
/// distinguish "the binary bailed before running any checks — fix your
/// checks.lisp / workspace layout" from "some checks failed — read the
/// `✗` lines and fix the failing checks".
///
/// # Compounding
///
/// A future startup-side gate (a `Cargo.toml` `[workspace]` header
/// probe, a `checks.lisp` schema-version gate, a
/// `tatara_process::register_all()` fault detector, a future
/// `--dry-run` argument that requires early-exit before the dispatcher
/// runs) reads `return startup_bail(<detail>);` at ONE line and
/// inherits the prefix + exit-code discipline by construction — no
/// per-gate restatement of the pre-lift `eprintln!` + `return
/// ExitCode::from(2)` two-step chain. A future normalization (a
/// `tracing`-annotated stderr write instead of a bare `eprintln!`, a
/// version-stamped prefix, a distinct config-failure exit code for a
/// specific gate) lands at THIS ONE substrate primitive and every
/// current + future startup-side bail inherits the upgrade
/// mechanically.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// two-step `eprintln!("tatara-check: <detail>"); return
/// ExitCode::from(2);` chain recurred at 4 hand-authored sites in
/// [`main`] past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger and
/// is lifted onto ONE substrate owner here). THEORY.md §II.1
/// invariant 5 (composition preserves proofs — every startup-side
/// bail routing through the SAME substrate primitive means a future
/// diagnostic shape or exit-code shift lands at ONE site and every
/// downstream bail consumer inherits the shift by construction).
#[must_use]
fn startup_bail(detail: impl std::fmt::Display) -> ExitCode {
    eprintln!("{}", startup_diagnostic(detail));
    ExitCode::from(2)
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
        check_yaml_parses, evaluate_ephemeral_require_tag, evaluate_point_require_tag, find_kw,
        find_kw_string_list, head_symbol_or_missing, known_require_tag_domain_names,
        min_defs_shortfall_msg, parse_kwargs, positional_string, read_or_fail,
        report_result_prefixed, require_tag_domain_by_name, required_positional_string,
        startup_diagnostic, strip_and_classify_prefixed_kind, Report, UnknownRequireTag,
        ALL_REQUIRE_TAG_DOMAINS, MISSING_ARG_SLUG,
    };
    use tatara_lisp::{read, Sexp};
    use tatara_process::boundary::{Condition, ConditionKind};
    use tatara_process::crd::ProcessSpec;
    use tatara_process::ephemeral::EphemeralSpec;
    use tatara_process::intent::{AplicacaoIntent, IntentKind};
    use tatara_process::lifetime::{EphemeralLifetime, Lifetime, LifetimeKind, TeardownPolicy};

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

    // ── min_defs_shortfall_msg substrate pins ────────────────────────
    //
    // Fail-before-pass-after granularity: the `min_defs_shortfall_msg`
    // free function did not exist before this commit, so each test
    // below fails to compile pre-lift. Post-lift they collectively
    // pin the (numeric compare, per-shortfall format) shape at ONE
    // substrate owner — a regression that swapped `<` for `<=`
    // (a min-defs of N would then reject a defs-count of N, breaking
    // every `checks.lisp` `:min-definitions N` slot whose source
    // authored exactly N definitions), swapped the operator glyph
    // (`>=` in the prose so the diagnostic reads inverted), or
    // reordered the two numeric slots (rendering `"expected ≥
    // {defs_len} definitions, got {min_defs}"` — a silent inversion
    // that would confuse every operator looking at the failure
    // report) surfaces HERE rather than as silent drift between the
    // two `check_lisp_compiles` domain arms.
    //
    // Sibling shape to the `head_symbol_or_missing` pins above and
    // the `find_kw_string_list` pins earlier: each substrate lift on
    // the `check_*` executor surface gets fail-before-pass-after
    // corner coverage that closes the primitive's boundary at ONE
    // site so callers can compose on top through the shared return
    // type (`Option<String>` here, `&str` there, `Vec<String>`
    // above) without restating the composition discipline.

    #[test]
    fn min_defs_shortfall_msg_returns_none_when_defs_meet_the_minimum() {
        // Happy-path pin: a source that compiles to exactly `min_defs`
        // definitions (the boundary — `<` not `<=`) yields `None`, so
        // the caller falls through to the `:requires`-tag verification
        // rung. Load-bearing: a regression that swapped `<` for `<=`
        // would silently reject every `checks.lisp` form that authored
        // exactly `min_defs` definitions — the observability-stack
        // check today authors exactly 1 `(defpoint …)` form at
        // `min_defs = 1`, so an off-by-one shift lands as a full
        // tatara-check failure at CI time.
        assert!(min_defs_shortfall_msg(1, 1).is_none());
        assert!(min_defs_shortfall_msg(3, 3).is_none());
        assert!(min_defs_shortfall_msg(5, 1).is_none());
    }

    #[test]
    fn min_defs_shortfall_msg_returns_the_shortfall_diagnostic_when_defs_are_short() {
        // Shortfall-path pin: a source that compiles to fewer than
        // `min_defs` definitions yields the exact pre-lift `format!`
        // shape. Pin the byte-identical parity with both current
        // callers' hand-authored diagnostic — a regression that
        // reordered the two numeric slots or dropped the `≥` glyph
        // surfaces HERE.
        assert_eq!(
            min_defs_shortfall_msg(0, 1),
            Some("expected ≥ 1 definitions, got 0".to_string()),
        );
        assert_eq!(
            min_defs_shortfall_msg(2, 5),
            Some("expected ≥ 5 definitions, got 2".to_string()),
        );
    }

    #[test]
    fn min_defs_shortfall_msg_matches_pre_lift_chain_bytewise() {
        // Byte-identical parity pin with the pre-lift four-line
        // `if defs.len() < min_defs { return report.fail(label,
        // format!("expected ≥ {} definitions, got {}", min_defs,
        // defs.len())); }` chain both current callers restated. Sweeps
        // the boundary corners: (a) empty defs against non-zero min,
        // (b) exact-boundary defs-count matching min, (c) defs
        // strictly above min, (d) large-integer corner. A regression
        // in the primitive that broke byte identity with the pre-lift
        // shape at ANY corner surfaces HERE rather than as silent
        // check-executor drift across the two `check_lisp_compiles`
        // domain arms.
        for (defs_len, min_defs) in [(0usize, 0usize), (0, 1), (1, 1), (1, 2), (5, 3), (100, 99)] {
            let via_primitive: Option<String> = min_defs_shortfall_msg(defs_len, min_defs);
            let via_pre_lift: Option<String> = (defs_len < min_defs)
                .then(|| format!("expected ≥ {} definitions, got {}", min_defs, defs_len));
            assert_eq!(
                via_primitive, via_pre_lift,
                "drift on (defs_len={defs_len}, min_defs={min_defs})",
            );
        }
    }

    #[test]
    fn min_defs_shortfall_msg_treats_zero_min_as_satisfied_by_zero_defs() {
        // Zero-corner pin: a `:min-definitions 0` slot (an operator
        // authoring a coherence check that only cares about compile
        // success, not definition count) accepts an empty defs vec —
        // matches the pre-lift `<` (not `<=`) comparator. Guards
        // against a regression that promoted the comparator to `<=`
        // and started rejecting the "no floor" case.
        assert!(min_defs_shortfall_msg(0, 0).is_none());
    }

    // ── positional_string substrate pins ─────────────────────────────
    //
    // Fail-before-pass-after granularity: the `positional_string` free
    // function did not exist before this commit, so each test below
    // fails to compile pre-lift. Post-lift they collectively pin the
    // (positional index, string-projection, soft-failure return) shape
    // at ONE substrate owner — a regression that dropped the
    // `Sexp::as_string` projection (silently reaching into a bare
    // symbol / list / integer / keyword payload and returning a
    // non-None slug), swapped the soft-failure return for a panic
    // (aborting the whole `tatara-check` run rather than reporting a
    // soft per-check failure), or misread the index (returning
    // `args[0]` regardless of the requested slot) surfaces HERE
    // rather than as silent operator-facing drift at any of the five
    // `check_*` executors that decode a `"path"` slot from a
    // positional argument.

    #[test]
    fn positional_string_returns_the_borrowed_string_payload_at_the_requested_index() {
        // Byte-identical parity with the pre-lift `check_yaml_parses`
        // head-arg decode: `(yaml-parses "chart/foo.yaml")` yields the
        // string payload `"chart/foo.yaml"` at position 0. Pin the
        // happy-path shape at BOTH positional indices both current
        // callers walk (position 0 for yaml-parses / lisp-compiles /
        // file-contains; position 1 for crd-in-sync / yaml-parses-as).
        let args0 = args_from(r#"(yaml-parses "chart/foo.yaml")"#);
        assert_eq!(positional_string(&args0, 0), Some("chart/foo.yaml"));
        let args1 = args_from(r#"(yaml-parses-as Process "chart/Process.yaml")"#);
        assert_eq!(positional_string(&args1, 1), Some("chart/Process.yaml"));
    }

    #[test]
    fn positional_string_returns_none_when_the_index_is_out_of_bounds() {
        // Out-of-bounds pin: an executor called with fewer args than
        // the requested index (`(yaml-parses)` decoded at position 0,
        // `(crd-in-sync Process)` decoded at position 1) returns
        // `None` — matches the pre-lift `.get(N).<chain>` soft-failure
        // shape. Load-bearing: every current caller composes its own
        // `report.fail` around the `None` return; a regression that
        // panicked on the OOB index would abort the whole
        // `tatara-check` run.
        let empty: Vec<Sexp> = Vec::new();
        assert!(positional_string(&empty, 0).is_none());
        let one = args_from(r#"(crd-in-sync Process)"#);
        assert!(positional_string(&one, 1).is_none());
        // Way-out-of-bounds still soft-fails, not panics.
        assert!(positional_string(&one, 42).is_none());
    }

    #[test]
    fn positional_string_returns_none_on_non_string_shape_at_the_requested_index() {
        // Non-string-shape pin: an operator typo that put a symbol /
        // integer / nested list / keyword where a string was expected
        // yields `None` — matches the pre-lift `.and_then(Sexp::as_string)`
        // soft-failure shape. A regression that reached inside a
        // symbol payload (silently promoting a bare `foo` into the
        // returned `"foo"` slug) would either surface here as a
        // wrong `Some(...)` or as a panic on the projection arm.
        for src in [
            r#"(yaml-parses bare-symbol)"#,
            r#"(yaml-parses 42)"#,
            r#"(yaml-parses (nested "list"))"#,
            r#"(yaml-parses :keyword)"#,
        ] {
            let args = args_from(src);
            assert!(
                positional_string(&args, 0).is_none(),
                "non-string slot must fall through to None on {src}",
            );
        }
    }

    #[test]
    fn positional_string_matches_pre_lift_chain_bytewise() {
        // Byte-identical parity pin with the pre-lift two-token chain
        // all five callers hand-authored. Sweeps the five corners
        // every pre-lift consumer visited: (a) present string at
        // position 0, (b) present string at position 1 with a leading
        // non-string head, (c) empty args, (d) non-string at the
        // requested position, (e) present args but requested index
        // past the end. A regression in the primitive that broke byte
        // identity with the pre-lift shape at ANY corner surfaces
        // HERE rather than as silent check-executor drift between the
        // migrated callers and any future caller that reads the
        // primitive's output.
        for (src, index) in [
            (r#"(check "just-one")"#, 0),
            (r#"(check Process "path")"#, 1),
            (r#"(check "leading" "trailing")"#, 1),
            (r#"(check)"#, 0),
            (r#"(check bare-sym)"#, 0),
            (r#"(check "only-one")"#, 5),
        ] {
            let args = args_from(src);
            let via_primitive: Option<&str> = positional_string(&args, index);
            let via_pre_lift: Option<&str> = args.get(index).and_then(Sexp::as_string);
            assert_eq!(
                via_primitive, via_pre_lift,
                "drift on ({src}, index={index})",
            );
        }
    }

    #[test]
    fn positional_string_only_reads_the_requested_index() {
        // Positional-scope pin: the primitive returns the payload at
        // the exact requested index, ignoring every other position —
        // a regression that scanned past the requested slot (e.g.
        // walking `.iter().skip(index).find_map(Sexp::as_string)`,
        // silently succeeding on a string at a later position when
        // the requested slot is non-string) would fail HERE. Peers
        // with the equivalent pin on [`head_symbol_or_missing`] at
        // position 0.
        let args = args_from(r#"(check bare-sym "hidden-string" "another")"#);
        assert!(
            positional_string(&args, 0).is_none(),
            "primitive must not scan past requested index 0",
        );
        assert_eq!(positional_string(&args, 1), Some("hidden-string"));
        assert_eq!(positional_string(&args, 2), Some("another"));
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

    // ── required_positional_string substrate pins ────────────────────
    //
    // Fail-before-pass-after granularity: the `required_positional_string`
    // free function did not exist before this commit, so each test below
    // fails to compile pre-lift. Post-lift they collectively pin the
    // (positional-string decode, report failure on absent slot, early-
    // return control-flow) triad at ONE substrate owner — a regression
    // that dropped the `report.fail(label, expected)` side-effect on the
    // `None` arm (silently swallowing an absent `"path"` slot and
    // returning `None` with no diagnostic), swapped the `Some` arm's
    // borrowed-lifetime forwarding for an owned-`String` allocation
    // (breaking the caller's downstream `root.join(rel)` zero-copy path),
    // or flipped the return polarity (returning `Some` on the `None`
    // arm so every caller's let-else early-return fell through into the
    // pass path with a bogus decoded slot) surfaces HERE rather than as
    // silent operator-facing drift at any of the five `check_*`
    // executors that decode a `"path"` slot through the primitive.

    #[test]
    fn required_positional_string_returns_borrowed_slot_and_leaves_report_clean_on_present_slot() {
        // Happy-path pin: a present string slot at the requested index
        // returns `Some(<borrowed &str>)` byte-for-byte AND does not
        // touch the `Report`. Load-bearing: every current caller's
        // `Some` arm forwards the borrowed slice to `root.join(rel)`
        // and to the `format!("<check>: {rel}")` label composition
        // without an intervening allocation; a regression that
        // side-effected the report on the pass path would leak a
        // spurious failure through the `Report::is_ok` gate at
        // executor exit.
        let mut report = Report::default();
        let args0 = args_from(r#"(check "chart/foo.yaml")"#);
        assert_eq!(
            required_positional_string(
                &args0,
                0,
                "check",
                "expected (check \"path\")",
                &mut report
            ),
            Some("chart/foo.yaml"),
        );
        assert!(
            report.is_ok(),
            "present slot must not touch report; got failures = {:?}",
            report.failures,
        );
        let args1 = args_from(r#"(check Process "chart/Process.yaml")"#);
        assert_eq!(
            required_positional_string(
                &args1,
                1,
                "check",
                "expected (check <Kind> \"path\")",
                &mut report
            ),
            Some("chart/Process.yaml"),
        );
        assert!(
            report.is_ok(),
            "present slot at position 1 must not touch report; got failures = {:?}",
            report.failures,
        );
    }

    #[test]
    fn required_positional_string_reports_expected_failure_and_returns_none_on_absent_slot() {
        // OOB-slot pin: an absent slot (fewer args than the requested
        // index) hangs `report.fail(label, expected)` off the failure
        // arm — the pre-lift diagnostic prose reaches `Report::fail`'s
        // `impl Display` slots byte-for-byte through `&str`'s
        // `Display` impl — AND returns `None` as the control-flow
        // signal the caller's let-else early-return consumes. A
        // regression that returned `Some("")` or panicked on the OOB
        // slot would abort the `tatara-check` run rather than
        // reporting a soft per-check failure.
        let mut report = Report::default();
        let empty: Vec<Sexp> = Vec::new();
        let ret = required_positional_string(
            &empty,
            0,
            "yaml-parses",
            "expected (yaml-parses \"path\")",
            &mut report,
        );
        assert!(
            ret.is_none(),
            "absent slot must return None so caller's let-else early-returns",
        );
        assert_eq!(
            report.failures,
            vec!["yaml-parses: expected (yaml-parses \"path\")".to_string()],
            "failure-arm must hang exactly one `<label>: <expected>` line",
        );
    }

    #[test]
    fn required_positional_string_reports_expected_failure_and_returns_none_on_non_string_slot() {
        // Non-string-shape pin: an operator typo that put a symbol /
        // integer / nested list / keyword where the primitive expected
        // a string reports the SAME `<label>: <expected>` failure as
        // the OOB corner (soft failure through
        // `and_then(Sexp::as_string)` inside `positional_string`) —
        // matches the pre-lift shape where every caller collapsed both
        // corners onto the same `expected …` diagnostic. A regression
        // that reported a per-shape diagnostic here without lifting
        // the same discipline through every current caller would drift
        // the (OOB, non-string) equivalence the pre-lift shape
        // preserved.
        for src in [
            r#"(check bare-symbol)"#,
            r#"(check 42)"#,
            r#"(check (nested "list"))"#,
            r#"(check :keyword)"#,
        ] {
            let mut report = Report::default();
            let args = args_from(src);
            let ret = required_positional_string(
                &args,
                0,
                "check",
                "expected (check \"path\")",
                &mut report,
            );
            assert!(ret.is_none(), "non-string slot must return None on {src}",);
            assert_eq!(
                report.failures,
                vec!["check: expected (check \"path\")".to_string()],
                "non-string slot must hang the same failure as OOB on {src}",
            );
        }
    }

    #[test]
    fn required_positional_string_matches_pre_lift_chain_bytewise() {
        // Byte-identical parity pin with the pre-lift three-link chain
        // all five callers hand-authored around `positional_string`.
        // For each (args source, positional index, per-check label,
        // per-check expected form) tuple that covers ALL five callsite
        // shapes (crd-in-sync pos 1, yaml-parses pos 0, yaml-parses-as
        // pos 1, lisp-compiles pos 0, file-contains pos 0), assert
        // that the primitive's (return, report-state) pair matches the
        // pre-lift `let Some(rel) = positional_string(&args, index)
        // else { return report.fail(label, expected); };` shape
        // byte-for-byte. A regression that broke byte identity at ANY
        // corner surfaces HERE rather than as silent check-executor
        // drift between the migrated callers.
        let sites: &[(&str, usize, &str, &str)] = &[
            (
                "(crd-in-sync Process)",
                1,
                "crd-in-sync",
                "expected (crd-in-sync <Kind> \"path\")",
            ),
            (
                "(yaml-parses)",
                0,
                "yaml-parses",
                "expected (yaml-parses \"path\")",
            ),
            (
                "(yaml-parses-as Process)",
                1,
                "yaml-parses-as",
                "expected (yaml-parses-as <Kind> \"path\")",
            ),
            (
                "(lisp-compiles)",
                0,
                "lisp-compiles",
                "expected (lisp-compiles \"path\" ...)",
            ),
            (
                "(file-contains)",
                0,
                "file-contains",
                "expected (file-contains \"path\" :strings (...))",
            ),
        ];
        for (src, index, label, expected) in sites {
            let args = args_from(src);
            // Primitive path.
            let mut prim_report = Report::default();
            let via_primitive =
                required_positional_string(&args, *index, label, expected, &mut prim_report);
            // Pre-lift path — the three-link inline chain every caller
            // walked before the lift.
            let mut inline_report = Report::default();
            let via_inline = match positional_string(&args, *index) {
                Some(s) => Some(s),
                None => {
                    inline_report.fail(*label, *expected);
                    None
                }
            };
            assert_eq!(
                via_primitive, via_inline,
                "return drift on ({src}, index={index})",
            );
            assert_eq!(
                prim_report.failures, inline_report.failures,
                "failure drift on ({src}, index={index})",
            );
            assert!(
                prim_report.passes.is_empty() && inline_report.passes.is_empty(),
                "neither path touches passes on ({src}, index={index})",
            );
        }
    }

    // ── evaluate_point_require_tag substrate pins ────────────────────
    //
    // Fail-before-pass-after granularity: `evaluate_point_require_tag`
    // did not exist before this commit — the require-tag dispatch lived
    // inline inside `check_lisp_compiles`. The extraction lifts the
    // (prefix-family, closed-set, has) shape to ONE testable owner that
    // both closed-set-driven prefix families (`intent-<kind>`,
    // `lifetime-<kind>`) and every non-closed-set fixed match tag
    // route through, so a regression that dropped a prefix branch,
    // silently rewired a `has(kind)` presence probe, or misclassified
    // an unknown suffix as "matched" fails HERE at ONE narrow
    // substrate site rather than requiring an end-to-end
    // `cargo run --bin tatara-check` sweep to catch.

    /// POPULATED-slot pin — `lifetime-<kind>` dispatches through the
    /// autoderived [`LifetimeKind`] `FromStr` + the substrate
    /// [`Lifetime::has`] primitive, returning `true` only when the
    /// slot addressed by the suffix is populated. Sweep the two-slot
    /// cross (diagonal + off-diagonal) so a future refactor that
    /// swapped the arm-to-slot mapping (e.g. `Ephemeral` reading the
    /// `permanent` field) fails here before landing at the operator-
    /// facing checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_populated_lifetime_slot_per_kind() {
        for populated in LifetimeKind::ALL {
            let mut spec = ProcessSpec::gate_compute_defaults();
            spec.lifetime = match populated {
                LifetimeKind::Permanent => Lifetime::permanent(),
                LifetimeKind::Ephemeral => Lifetime::ephemeral(EphemeralLifetime::default()),
            };
            for query in LifetimeKind::ALL {
                let tag = format!("lifetime-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "lifetime slot populated={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// EMPTY-slot pin — a default [`Lifetime`] (no slot populated,
    /// resolver picks Permanent as fallback) returns `false` for
    /// every `lifetime-<kind>` tag. Locks the write-side / read-side
    /// semantic split ([`Lifetime::has`]'s doc-comment) at the
    /// classifier boundary so an operator authoring
    /// `:requires (lifetime-permanent)` against a Process with an
    /// unset `:lifetime` slot gets the `definition missing required`
    /// diagnostic, not a false-positive pass.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_default_lifetime_for_every_kind() {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in LifetimeKind::ALL {
            let tag = format!("lifetime-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "default lifetime (no slot populated) must return false for {tag:?}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `lifetime-<garbage>` classifies as
    /// [`UnknownRequireTag`] so the caller's operator-facing
    /// `unknown :requires tag: <verbatim>` diagnostic path fires. A
    /// regression that fell through to `Ok(false)` (matching the pre-
    /// lift `match req.as_str() { ... other => report.fail(...) }`
    /// arm) would silently reclassify an unknown suffix as
    /// `definition missing required: <tag>`, which reads as "the
    /// spec is wrong" rather than "your check is wrong". Pin the
    /// distinction.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_lifetime_suffix() {
        let spec = ProcessSpec::gate_compute_defaults();
        assert_eq!(
            evaluate_point_require_tag(&spec, "lifetime-burst"),
            Err(UnknownRequireTag),
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "lifetime-typo"),
            Err(UnknownRequireTag),
        );
    }

    /// BOUNDARY pin — a bare `lifetime-` (no suffix) parses through
    /// the same `strip_prefix + parse` chain and lands at the
    /// [`LifetimeKind::from_str`] error arm (empty string ∉ the
    /// closed-set vocabulary), yielding [`UnknownRequireTag`]. A
    /// regression that special-cased the empty suffix (e.g. treating
    /// it as "any populated") would fail here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_bare_lifetime_prefix() {
        let spec = ProcessSpec::gate_compute_defaults();
        assert_eq!(
            evaluate_point_require_tag(&spec, "lifetime-"),
            Err(UnknownRequireTag),
        );
    }

    /// INTENT-side parity pin — the pre-existing `intent-<kind>`
    /// prefix family behaves the same way through the same extracted
    /// classifier. A default [`ProcessSpec`] has `Intent::default()`
    /// (no slot populated), so every `intent-<kind>` tag returns
    /// `Ok(false)`. Cross-check pin so a regression that lost the
    /// intent-side branch in the extraction fails here alongside the
    /// lifetime-side pins.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_default_intent_for_every_kind() {
        use tatara_process::intent::IntentKind;
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in IntentKind::ALL {
            let tag = format!("intent-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "default intent (no slot populated) must return false for {tag:?}",
            );
        }
    }

    /// NON-CLOSED-SET fixed-match pin — the existing hand-authored
    /// tags (`depends-on`, `boundary-pre`, `boundary-post`,
    /// `compliance`) address empty-collection slots that read false on
    /// a default [`ProcessSpec`]; `signals` addresses
    /// `sigterm_grace_seconds > 0`, which the substrate default
    /// (`tatara_process::serde_defaults::default_sigterm_grace_seconds`
    /// → 480) reads as `true`. An unknown non-prefixed tag returns
    /// [`UnknownRequireTag`]. Together with the closed-set pins this
    /// closes the (prefix-family, fixed-match, unknown) matrix at the
    /// classifier's return-shape boundary and pins the sibling-default
    /// correspondence with the SignalPolicy substrate primitive so a
    /// future default-signal-grace normalization surfaces here.
    #[test]
    fn evaluate_point_require_tag_routes_fixed_tags_and_unknown_tail() {
        let spec = ProcessSpec::gate_compute_defaults();
        for tag in ["depends-on", "boundary-pre", "boundary-post", "compliance"] {
            assert_eq!(
                evaluate_point_require_tag(&spec, tag),
                Ok(false),
                "default spec must return false for empty-collection fixed tag {tag:?}",
            );
        }
        // `signals` reads the substrate default's positive grace,
        // matching how an operator authoring `:requires (signals)`
        // against a Process that left `signals:` at defaults still
        // passes the gate.
        assert_eq!(
            evaluate_point_require_tag(&spec, "signals"),
            Ok(true),
            "default spec's sigterm_grace_seconds > 0 must satisfy the `signals` tag",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "totally-unknown"),
            Err(UnknownRequireTag),
        );
    }

    // ── strip_and_classify_prefixed_kind substrate pins ──────────────
    //
    // Fail-before-pass-after granularity: `strip_and_classify_prefixed_kind`
    // did not exist before this commit — the (strip_prefix + parse::<K>
    // + Ok/Err mapping) three-step shape lived inline at both closed-
    // set prefix families (`intent-<kind>`, `lifetime-<kind>`) in
    // `evaluate_point_require_tag`, restated byte-for-byte across TWO
    // branches past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold.
    // The lift places the shape on ONE testable owner so a regression
    // that (a) swapped the `strip_prefix` semantics for a `contains`
    // (dropping the anchoring), (b) collapsed the parse-error arm to
    // `Ok(false)` (silently reclassifying an out-of-vocabulary suffix
    // as "spec missing slot"), or (c) special-cased the empty suffix
    // (treating a bare `intent-` as "any populated") fails HERE at ONE
    // narrow substrate site rather than propagating through both
    // per-family test cohorts before catching.

    /// PREFIX-MISS pin — a tag that does not begin with the prefix
    /// yields `None`, so the caller's `if let Some(res) = …` early-
    /// return chain falls through to the next prefix family or the
    /// fixed-tag match tail. A regression that fired the parse arm on
    /// a non-matching prefix (e.g. an unchecked `.get(prefix.len()..)`
    /// call) would trip the `LifetimeKind::from_str` error path and
    /// return `Some(Err(UnknownRequireTag))`, silently masking every
    /// fixed tag (`depends-on`, `boundary-pre`, …) as unknown.
    #[test]
    fn strip_and_classify_prefixed_kind_returns_none_when_prefix_does_not_match() {
        // Fixed-tag shape — no prefix match anywhere.
        assert_eq!(
            strip_and_classify_prefixed_kind::<LifetimeKind, _>(
                "depends-on",
                "lifetime-",
                |_kind| true,
            ),
            None,
        );
        // Sibling prefix — `intent-<kind>` does NOT match the
        // `lifetime-` family. Locks the per-family branching
        // discipline: each `if let Some(res) = …` step tests its own
        // prefix independently and never bleeds across families.
        assert_eq!(
            strip_and_classify_prefixed_kind::<LifetimeKind, _>(
                "intent-nix",
                "lifetime-",
                |_kind| true,
            ),
            None,
        );
        // Empty tag — degenerate boundary; nothing to strip.
        assert_eq!(
            strip_and_classify_prefixed_kind::<LifetimeKind, _>("", "lifetime-", |_kind| true),
            None,
        );
    }

    /// PREFIX-HIT-PARSE-OK pin — a tag matching the prefix whose
    /// suffix parses to a canonical `K` label hands the parsed kind to
    /// `probe` and wraps the probe's return in `Some(Ok(…))`. Sweep
    /// every [`LifetimeKind`] variant so a regression that hard-coded
    /// a single arm (silently returning `Ok(true)` for every canonical
    /// suffix regardless of which variant parsed) fails here.
    #[test]
    fn strip_and_classify_prefixed_kind_returns_ok_when_suffix_is_canonical() {
        for canonical in LifetimeKind::ALL {
            let tag = format!("lifetime-{}", canonical.as_str());
            // Identity probe — `Ok(kind == canonical)` ↔ the parsed
            // kind equals the one this iteration expects. Pins that
            // the parsed suffix reaches the probe under the SAME
            // discriminator the caller composed, not a positional
            // swap or a default fallback.
            assert_eq!(
                strip_and_classify_prefixed_kind::<LifetimeKind, _>(&tag, "lifetime-", |parsed| {
                    parsed == canonical
                },),
                Some(Ok(true)),
                "suffix {tag:?} must parse to {canonical:?} and reach the probe",
            );
        }
        // Cross-family variant: an `intent-` tag routes through the
        // `IntentKind` bound at the same primitive, sharing the exact
        // (strip + parse + probe) discipline the lifetime-side cohort
        // uses. Sibling coverage so both closed-set prefix families
        // route through ONE primitive.
        for canonical in IntentKind::ALL {
            let tag = format!("intent-{}", canonical.as_str());
            assert_eq!(
                strip_and_classify_prefixed_kind::<IntentKind, _>(&tag, "intent-", |parsed| parsed
                    == canonical,),
                Some(Ok(true)),
                "suffix {tag:?} must parse to {canonical:?} and reach the probe",
            );
        }
    }

    /// PREFIX-HIT-PARSE-FAIL pin — a tag matching the prefix whose
    /// suffix is NOT a canonical `K` label yields
    /// `Some(Err(UnknownRequireTag))`, so the caller's operator-facing
    /// `unknown :requires tag: <verbatim>` diagnostic path fires. A
    /// regression that fell through to `Some(Ok(false))` would
    /// silently reclassify a `lifetime-burst` typo as
    /// `definition missing required: lifetime-burst` — the operator
    /// reads "the spec is wrong" instead of "your check is wrong".
    /// This pin closes the two-cell (parse-ok, parse-fail) matrix at
    /// the substrate primitive's return-shape boundary.
    #[test]
    fn strip_and_classify_prefixed_kind_returns_unknown_on_out_of_vocabulary_suffix() {
        // Never-fire probe — parse failure must short-circuit before
        // the closure runs, so a regression that inverted the Ok/Err
        // arms would panic here.
        let unreachable_probe = |_kind: LifetimeKind| unreachable!("probe fires only on Ok");
        for typo in ["burst", "typo", "PERMANENT", "Ephemeral", "ephemeral-2"] {
            let tag = format!("lifetime-{typo}");
            assert_eq!(
                strip_and_classify_prefixed_kind::<LifetimeKind, _>(
                    &tag,
                    "lifetime-",
                    unreachable_probe,
                ),
                Some(Err(UnknownRequireTag)),
                "out-of-vocabulary suffix {tag:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// EMPTY-SUFFIX pin — a bare `<prefix>` (no suffix) routes through
    /// the SAME (parse-fail → UnknownRequireTag) arm because the empty
    /// string is not a canonical `K` label. Locks the boundary shared
    /// with [`evaluate_point_require_tag_returns_unknown_on_bare_lifetime_prefix`]
    /// so a regression that special-cased the empty suffix (e.g.
    /// treating `lifetime-` as "any populated") fails at BOTH the
    /// substrate primitive AND the caller-side pin — the double pin
    /// keeps the (primitive, caller) pair proven-equivalent.
    #[test]
    fn strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix() {
        let unreachable_probe = |_kind: LifetimeKind| unreachable!("probe fires only on Ok");
        assert_eq!(
            strip_and_classify_prefixed_kind::<LifetimeKind, _>(
                "lifetime-",
                "lifetime-",
                unreachable_probe,
            ),
            Some(Err(UnknownRequireTag)),
        );
    }

    // ── evaluate_ephemeral_require_tag substrate pins ────────────────
    //
    // Fail-before-pass-after granularity: `evaluate_ephemeral_require_tag`
    // did not exist before this commit — the ephemeral require-tag
    // dispatch lived inline inside `check_lisp_compiles` as a
    // hand-authored `match req.as_str()` restating its own error-flow
    // through `return report.fail(...)` and unreachable from tests. The
    // extraction lifts the six-tag vocabulary to ONE testable owner
    // whose return-shape (`Result<bool, UnknownRequireTag>`) is
    // byte-identical to the [`evaluate_point_require_tag`] peer, so
    // both classifiers now route through the SAME classification-error
    // taxonomy at the check-executor boundary. A regression that
    // dropped a vocabulary arm, silently reclassified a known slot's
    // presence probe, or misclassified an out-of-vocabulary tag as
    // `Ok(false)` fails HERE at ONE narrow substrate site rather than
    // requiring an end-to-end `cargo run --bin tatara-check` sweep
    // driving a `(defephemeral …)` form through the executor to catch.

    fn ephemeral_fixture() -> EphemeralSpec {
        EphemeralSpec {
            aplicacao: AplicacaoIntent::chart_only("oci://ghcr.io/x", "1"),
            ttl: "1h".into(),
            teardown: TeardownPolicy::Always,
            max_concurrent: 0,
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: None,
            exports: vec![],
            routing: None,
        }
    }

    /// POPULATED-slot pin — every vocabulary tag whose truth depends
    /// on a mutable slot reads `Ok(true)` on a fully populated
    /// [`EphemeralSpec`]. Locks the six-tag vocabulary at the
    /// classifier boundary so a regression that dropped a slot arm
    /// (e.g. silently rewrote `postconditions` to read a different
    /// slot) fails here before landing at the operator-facing
    /// checks.lisp surface.
    #[test]
    fn evaluate_ephemeral_require_tag_routes_populated_slots_true() {
        let mut spec = ephemeral_fixture();
        spec.postconditions = vec![Condition {
            kind: ConditionKind::HelmReleaseReleased,
            params: serde_json::Value::Null,
        }];
        spec.preconditions = vec![Condition {
            kind: ConditionKind::KustomizationHealthy,
            params: serde_json::Value::Null,
        }];
        for tag in [
            "aplicacao",
            "ttl",
            "teardown",
            "postconditions",
            "preconditions",
        ] {
            assert_eq!(
                evaluate_ephemeral_require_tag(&spec, tag),
                Ok(true),
                "populated slot must classify as satisfied for tag {tag:?}",
            );
        }
    }

    /// EMPTY-slot pin — an ephemeral spec with empty collections and
    /// empty strings reads `Ok(false)` for every emptiness-driven tag.
    /// `teardown` reads `Ok(true)` unconditionally (typed enum with a
    /// `Default` impl; the substrate has no absent-teardown state to
    /// detect). A regression that flipped the always-satisfied
    /// `teardown` arm or the emptiness polarity on the four
    /// collection/string slots fails here.
    #[test]
    fn evaluate_ephemeral_require_tag_returns_false_on_empty_slots() {
        let spec = EphemeralSpec {
            aplicacao: AplicacaoIntent::chart_only("", ""),
            ttl: String::new(),
            teardown: TeardownPolicy::default(),
            max_concurrent: 0,
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: None,
            exports: vec![],
            routing: None,
        };
        for tag in [
            "aplicacao",
            "ttl",
            "postconditions",
            "preconditions",
            "closed-loop-auth",
        ] {
            assert_eq!(
                evaluate_ephemeral_require_tag(&spec, tag),
                Ok(false),
                "empty slot must classify as unsatisfied for tag {tag:?}",
            );
        }
        assert_eq!(
            evaluate_ephemeral_require_tag(&spec, "teardown"),
            Ok(true),
            "teardown is a typed enum with Default — no absent state to detect",
        );
    }

    /// UNKNOWN-tag pin — an out-of-vocabulary tag classifies as
    /// [`UnknownRequireTag`] so the caller's operator-facing
    /// `unknown :requires tag for ephemeral domain: <verbatim>`
    /// diagnostic path fires. A regression that fell through to
    /// `Ok(false)` (matching the pre-lift `match req.as_str() { ...
    /// other => report.fail(...) }` arm's inverse) would silently
    /// reclassify a misspelled tag as `definition missing required:
    /// <tag>`, which reads as "the spec is wrong" rather than "your
    /// check is wrong". Pin the distinction — parity with the point-
    /// classifier's [`evaluate_point_require_tag_routes_fixed_tags_and_unknown_tail`]
    /// unknown-tail pin.
    #[test]
    fn evaluate_ephemeral_require_tag_returns_unknown_on_out_of_vocabulary_tag() {
        let spec = ephemeral_fixture();
        for tag in ["totally-unknown", "aplicaca", "cl-auth", ""] {
            assert_eq!(
                evaluate_ephemeral_require_tag(&spec, tag),
                Err(UnknownRequireTag),
                "out-of-vocabulary tag {tag:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// FINE-GRAINED-DISCRIMINATOR pin — `closed-loop-auth` reads
    /// `Ok(true)` only when at least one postcondition's `kind` is
    /// [`ConditionKind::ClosedLoopAuth`]. Cross-check that an
    /// ephemeral spec whose postconditions carry a
    /// [`ConditionKind::HelmReleaseReleased`] entry (satisfying the
    /// coarse `postconditions` tag) still reads `Ok(false)` for
    /// `closed-loop-auth`. A regression that widened the discriminator
    /// to "any postcondition" — the same regression that would collapse
    /// the closed-loop-auth destination-state's boundary theorem into
    /// a plain "has some postcondition" assertion — fails here.
    #[test]
    fn evaluate_ephemeral_require_tag_closed_loop_auth_reads_postcondition_kind() {
        let mut spec = ephemeral_fixture();
        spec.postconditions = vec![Condition {
            kind: ConditionKind::HelmReleaseReleased,
            params: serde_json::Value::Null,
        }];
        assert_eq!(
            evaluate_ephemeral_require_tag(&spec, "closed-loop-auth"),
            Ok(false),
            "a HelmReleaseReleased postcondition must not satisfy the closed-loop-auth tag",
        );
        assert_eq!(
            evaluate_ephemeral_require_tag(&spec, "postconditions"),
            Ok(true),
            "the coarse postconditions tag must still be satisfied by any kind",
        );
        spec.postconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: serde_json::Value::Null,
        });
        assert_eq!(
            evaluate_ephemeral_require_tag(&spec, "closed-loop-auth"),
            Ok(true),
            "at least one ClosedLoopAuth postcondition must satisfy the closed-loop-auth tag",
        );
    }

    /// PARITY pin — the ephemeral classifier's return shape is
    /// `Result<bool, UnknownRequireTag>`, byte-identical to the peer
    /// [`evaluate_point_require_tag`] classifier. A regression that
    /// diverged the two classifiers' error type (e.g. returning
    /// `Option<bool>` here, or a domain-specific `UnknownEphemeralTag`
    /// sentinel) fails here — parity is what lets a future
    /// `RequireTagClassifier` trait unify the two arms of
    /// `check_lisp_compiles` behind ONE typed dispatcher.
    #[test]
    fn evaluate_ephemeral_require_tag_shares_error_taxonomy_with_point_peer() {
        let eph = ephemeral_fixture();
        let point = ProcessSpec::gate_compute_defaults();
        // Both classifiers hand back the same `UnknownRequireTag`
        // sentinel; a shared error type is the compile-time proof of
        // parity. A future trait binding will consume both.
        assert_eq!(
            evaluate_ephemeral_require_tag(&eph, "nope"),
            Err(UnknownRequireTag),
        );
        assert_eq!(
            evaluate_point_require_tag(&point, "nope"),
            Err(UnknownRequireTag),
        );
    }

    // ── RequireTagDomain trait dispatch pins ─────────────────────────
    //
    // Fail-before-pass-after granularity: the `RequireTagDomain` trait,
    // the `POINT_DOMAIN` / `EPHEMERAL_DOMAIN` statics, the
    // `ALL_REQUIRE_TAG_DOMAINS` registry, `require_tag_domain_by_name`,
    // and `known_require_tag_domain_names` did not exist before this
    // commit, so each test below fails to compile pre-lift. Post-lift
    // they collectively pin the (name-based dispatch, compile-carrier
    // shape, per-domain unknown-tag prose) trait boundary at ONE
    // substrate owner — a regression that dropped a registry entry
    // (silently making a formerly-known `:domain` slot value resolve
    // to `None`), reordered the registry (breaking the diagnostic
    // suffix ordering downstream operators grep for), swapped a
    // domain's `.name()` for a typo (silently reclassifying every
    // `(lisp-compiles ... :domain <old>)` slot to unknown), or
    // diverged either impl's `.unknown_tag_diagnostic` from its pre-
    // lift `format!` shape surfaces HERE rather than as silent
    // operator-facing drift at the sole `check_lisp_compiles` caller.

    /// KNOWN-NAMES pin — every domain in [`ALL_REQUIRE_TAG_DOMAINS`]
    /// resolves through [`require_tag_domain_by_name`] to itself (by
    /// pointer identity). Locks the registry ↔ dispatch invariant:
    /// the closed-set names published on the operator-facing
    /// `"(known: <names>)"` diagnostic suffix are EXACTLY the names
    /// the executor's per-slot dispatch accepts. A regression that
    /// added a domain to the registry but forgot to route its name,
    /// or vice versa, fails HERE.
    #[test]
    fn require_tag_domain_by_name_resolves_every_registered_domain() {
        for domain in ALL_REQUIRE_TAG_DOMAINS {
            let looked_up = require_tag_domain_by_name(domain.name())
                .expect("every registered domain must resolve by its own name");
            // Pointer identity — the dispatch returns the SAME static
            // trait object, not a fresh clone or a peer with the same
            // name. Load-bearing: the executor's per-request `&dyn
            // RequireTagDomain` reference is a static borrow — the
            // dispatch never allocates per-check.
            assert!(
                std::ptr::eq(*domain as *const _, looked_up as *const _),
                "dispatch of {:?} must return the same static object",
                domain.name(),
            );
        }
    }

    /// UNKNOWN-NAME pin — a `:domain` slot value outside the closed
    /// set returns `None`, and the caller composes its per-check
    /// `"unknown :domain <name> (known: <names>)"` diagnostic around
    /// the `None`. A regression that fell through to a default
    /// (silently rerouting every unknown domain-name to `point`) or
    /// that panicked on the unknown name would fail HERE before
    /// landing at the operator-facing surface.
    #[test]
    fn require_tag_domain_by_name_returns_none_on_unknown_name() {
        for name in ["totally-unknown", "poin", "Point", "", "aplicacao"] {
            assert!(
                require_tag_domain_by_name(name).is_none(),
                "out-of-vocabulary domain name {name:?} must not resolve",
            );
        }
    }

    /// KNOWN-NAMES-DIAGNOSTIC pin — the composed
    /// `"known: <names>"` suffix reads `"point, ephemeral"` byte-for-
    /// byte against the pre-lift hand-authored `"known: point,
    /// ephemeral"` literal. Load-bearing: operators that grep for the
    /// pre-lift substring in the `tatara-check` failure log inherit
    /// the SAME suffix post-lift. A regression that reordered the
    /// registry or swapped the join separator fails HERE. A future
    /// peer domain added to [`ALL_REQUIRE_TAG_DOMAINS`] mechanically
    /// updates the expected suffix — this pin will need to grow with
    /// the closed set, and that's the point.
    #[test]
    fn known_require_tag_domain_names_matches_pre_lift_literal() {
        assert_eq!(known_require_tag_domain_names(), "point, ephemeral");
    }

    /// NAMES-CLOSED-SET pin — the registered domains' `.name()`
    /// values are EXACTLY the closed set `{"point", "ephemeral"}`.
    /// Sibling of [`known_require_tag_domain_names_matches_pre_lift_literal`]
    /// on the set-membership axis: the format-ordered join pin catches
    /// re-ordering; THIS pin catches a rename or an off-set variant.
    /// A regression that renamed a domain's `.name()` (e.g. `"point"`
    /// → `"pointspec"`) would break every operator's pre-lift
    /// `(lisp-compiles ... :domain point)` slot and fail HERE.
    #[test]
    fn require_tag_domain_names_are_the_pre_lift_closed_set() {
        let names: std::collections::BTreeSet<&str> =
            ALL_REQUIRE_TAG_DOMAINS.iter().map(|d| d.name()).collect();
        let expected: std::collections::BTreeSet<&str> =
            ["point", "ephemeral"].into_iter().collect();
        assert_eq!(names, expected);
    }

    /// POINT-COMPILE pin — the point domain's `compile` step on a
    /// well-formed `(defpoint …)` source returns a [`CompiledSource`]
    /// whose `count` matches the underlying `compile_source` output
    /// AND whose `classify` closure agrees with the direct
    /// [`evaluate_point_require_tag`] classifier byte-for-byte on
    /// every peer tag. A regression that fanned the classifier out to
    /// a wrong spec index, silently swapped in the ephemeral
    /// classifier, or lost the closure's captured spec would fail
    /// HERE.
    #[test]
    fn point_domain_compile_carries_count_and_classifier_over_first_definition() {
        let src = r#"
            (defpoint p1
              :identity       (:parent "seph.1")
              :classification (:point-type Gate :substrate Compute)
              :intent         (:nix (:flake-ref "github:pleme-io/x" :attribute "y")))
        "#;
        let domain = require_tag_domain_by_name("point").expect("point domain registered");
        let compiled = domain
            .compile(src)
            .expect("well-formed source must compile");
        assert_eq!(compiled.count, 1, "one defpoint form → one definition");

        // Parity: routing through the trait's classifier reads the
        // SAME classification for a peer tag as the direct primitive.
        let first_spec = &tatara_process::compile_source(src).unwrap()[0].spec;
        for tag in ["intent-nix", "intent-flux", "boundary-post", "depends-on"] {
            assert_eq!(
                (compiled.classify)(tag),
                evaluate_point_require_tag(first_spec, tag),
                "trait-routed classification must agree with direct primitive on tag {tag:?}",
            );
        }
        assert_eq!(
            (compiled.classify)("not-a-tag"),
            Err(UnknownRequireTag),
            "out-of-vocabulary tag must classify as UnknownRequireTag through the trait",
        );
    }

    /// POINT-COMPILE-ERR pin — the point domain's `compile` step on a
    /// malformed source surfaces the underlying [`tatara_lisp::LispError`]
    /// via `Result::Err`, so the caller's downstream
    /// `tatara_lisp::format_diagnostic` prose emits unchanged. A
    /// regression that swallowed the error into an `Ok(CompiledSource
    /// { count: 0, .. })` shape would silently reclassify every parse-
    /// broken source as a min-defs shortfall rather than as a parse
    /// error at the operator-facing diagnostic surface.
    #[test]
    fn point_domain_compile_propagates_lisp_errors() {
        let domain = require_tag_domain_by_name("point").expect("point domain registered");
        // A source with an unclosed list is a reader error; the trait
        // must surface it as `Err` for the executor's per-check
        // diagnostic path to fire.
        assert!(domain.compile("(defpoint p1 :identity").is_err());
    }

    /// EPHEMERAL-COMPILE pin — peer of the point-compile pin on the
    /// ephemeral (EphemeralSpec) surface: the compile step's `count`
    /// matches the underlying `compile_ephemeral_source` output AND
    /// the `classify` closure agrees with the direct
    /// [`evaluate_ephemeral_require_tag`] classifier on every peer
    /// tag. A regression that crossed the classifiers (silently
    /// routing the point classifier onto the ephemeral spec) would
    /// misclassify every ephemeral-domain check as unknown-tag and
    /// fail HERE.
    #[test]
    fn ephemeral_domain_compile_carries_count_and_classifier_over_first_definition() {
        let src = r#"
            (defephemeral e1
              :aplicacao (:chart-ref "oci://ghcr.io/foo" :version "0.1.0")
              :ttl       "30m"
              :teardown  Always
              :postconditions ((:kind ClosedLoopAuth :params ())))
        "#;
        let domain = require_tag_domain_by_name("ephemeral").expect("ephemeral domain registered");
        let compiled = domain
            .compile(src)
            .expect("well-formed source must compile");
        assert_eq!(compiled.count, 1, "one defephemeral form → one definition");

        let first_spec = &tatara_process::ephemeral::compile_ephemeral_source(src).unwrap()[0].spec;
        for tag in [
            "aplicacao",
            "ttl",
            "teardown",
            "postconditions",
            "preconditions",
            "closed-loop-auth",
        ] {
            assert_eq!(
                (compiled.classify)(tag),
                evaluate_ephemeral_require_tag(first_spec, tag),
                "trait-routed classification must agree with direct primitive on tag {tag:?}",
            );
        }
        assert_eq!((compiled.classify)("not-a-tag"), Err(UnknownRequireTag),);
    }

    /// UNKNOWN-TAG-PROSE pin — each domain's `unknown_tag_diagnostic`
    /// composes the exact pre-lift `format!` shape both pre-lift
    /// arms restated inside `check_lisp_compiles`. Load-bearing: an
    /// operator that greps for the pre-lift substring
    /// `"unknown :requires tag"` sees the SAME prose post-lift; the
    /// ephemeral-domain-specific `" for ephemeral domain:"` mid-clause
    /// stays byte-identical. A regression that dropped the mid-clause
    /// or renamed the two prose shapes surfaces HERE.
    #[test]
    fn require_tag_domain_unknown_tag_diagnostic_matches_pre_lift_shape() {
        let point = require_tag_domain_by_name("point").expect("point domain registered");
        assert_eq!(
            point.unknown_tag_diagnostic("intent-frobnicate"),
            "unknown :requires tag: intent-frobnicate",
        );
        let eph = require_tag_domain_by_name("ephemeral").expect("ephemeral domain registered");
        assert_eq!(
            eph.unknown_tag_diagnostic("aplicaca"),
            "unknown :requires tag for ephemeral domain: aplicaca",
        );
    }

    // ─── read_or_fail substrate pins ───────────────────────────────────
    //
    // Fail-before-pass-after granularity: the `read_or_fail` free
    // function did not exist on the pre-lift binary — the tests below
    // do not compile before the lift. Post-lift they bind the read +
    // `"read: {e}"` failure-arm shape at ONE substrate owner so a
    // regression that drifted the `"read: "` prefix, dropped the
    // `report.fail` side-effect on the `Err` arm, or flipped the
    // return polarity (`Some` on `Err` / `None` on `Ok`) surfaces
    // HERE rather than as silent operator-facing diagnostic skew at
    // each of the four consumer sites `check_yaml_parses` /
    // `check_yaml_parses_as` / `check_lisp_compiles` /
    // `check_file_contains`. The fourth site
    // (`check_yaml_parses`, added this run) closes the executor-
    // family symmetry gap the earlier lift missed because its
    // pre-lift match wrapped the pass path inline rather than
    // early-returning; the executor-level seal
    // `check_yaml_parses_read_failure_prose_routes_through_read_or_fail_substrate`
    // pins the diagnostic prose at the callsite so a regression that
    // reintroduced the pre-lift inline match at THIS executor (rather
    // than the shared substrate) surfaces there rather than as silent
    // drift across the fourth site's operator-facing diagnostic.

    fn scratch_path(tag: &str) -> std::path::PathBuf {
        // A per-test-name scratch path under the OS temp dir, keyed
        // on both the process id and the caller's `tag` so parallel
        // tests never collide on the same inode.
        std::env::temp_dir().join(format!(
            "tatara-check-read-or-fail-{}-{tag}.tmp",
            std::process::id(),
        ))
    }

    #[test]
    fn read_or_fail_returns_some_carrying_the_file_bytes_on_utf8_read() {
        // Happy-path pin: an existing UTF-8 source file rides through
        // and the callsite receives `Some(<contents>)` — the same
        // shape all three pre-lift sites bound their `let src` slot to
        // through the `Ok(s) => s` arm of the pre-lift match. A
        // regression that widened the return to `Result<Option<...>,
        // _>` or drifted the encoding (a `String::from_utf8_lossy`
        // corner that would silently substitute replacement chars for
        // non-UTF-8 bytes) surfaces HERE rather than as silent drift
        // at each downstream check executor.
        let path = scratch_path("happy");
        std::fs::write(&path, "canonical source").expect("scratch write");
        let mut report = Report::default();
        let got = read_or_fail(&path, "sample label", &mut report);
        let _ = std::fs::remove_file(&path);
        assert_eq!(got.as_deref(), Some("canonical source"));
        assert!(
            report.is_ok(),
            "happy-path read must not push a failure entry — a regression that reported EVEN on the OK arm would surface here"
        );
    }

    #[test]
    fn read_or_fail_reports_read_failure_and_returns_none_on_missing_file() {
        // Miss-path pin: a nonexistent path yields `None` AND pushes a
        // `<label>: read: <io error>` entry onto the Report — the two
        // sinks the pre-lift chain composed simultaneously through the
        // `Err(e) => return report.fail(label, format!("read: {e}"))`
        // arm. A regression that dropped the side-effect (returning
        // `None` silently, leaving the Report unaware of the read
        // failure) or dropped the `"read: "` prefix (drifting the
        // operator-facing diagnostic prose that every downstream
        // check-log grep keys on) fires HERE.
        let missing = scratch_path("missing");
        // Belt-and-braces: ensure the path really does not exist.
        let _ = std::fs::remove_file(&missing);
        let mut report = Report::default();
        let got = read_or_fail(&missing, "missing label", &mut report);
        assert!(
            got.is_none(),
            "miss-path read must yield None so the caller's let-else early-returns",
        );
        assert!(
            !report.is_ok(),
            "miss-path read must push a failure entry so the check registers as failed at the workspace level",
        );
        let failure = report
            .failures
            .last()
            .expect("miss-path read must push exactly one failure");
        assert!(
            failure.starts_with("missing label: read: "),
            "miss-path failure prose must start with `<label>: read: ` — pin the label prefix + the `read: ` sentinel prose downstream check-log grep keys on; got {failure:?}"
        );
    }

    // ─── report_result_prefixed substrate pins ─────────────────────────
    //
    // Fail-before-pass-after granularity: the `report_result_prefixed`
    // free function did not exist on the pre-lift binary — the tests
    // below do not compile before the lift. Post-lift they bind the
    // terminal `Result<T, E>` sink at ONE substrate owner so a
    // regression that drifted the pass entry (dropped the label, hung
    // the payload off the pass prose), drifted the fail entry
    // (dropped the `<prefix>: ` separator, swapped the arm-order,
    // dropped the `report.fail` side-effect), or flipped the polarity
    // (pass on `Err` / fail on `Ok`) surfaces HERE rather than as
    // silent operator-facing diagnostic skew at each of the two
    // consumer sites `check_yaml_parses` (prefix `"YAML"`) +
    // `check_yaml_parses_as` (prefix `"parse"`).

    #[test]
    fn report_result_prefixed_pushes_pass_entry_on_ok_result_and_consumes_label() {
        // Happy-path pin: an `Ok(_)` result lands as a pass entry
        // carrying the label verbatim — the same shape both pre-lift
        // callsites bound to through the `Ok(_) => report.pass(label)`
        // arm of the pre-lift terminal match. A regression that
        // decorated the pass prose (a `{label} (ok)` shape, a
        // trailing whitespace strip, a `to_uppercase` transform) or
        // dropped the pass push altogether would surface here rather
        // than as silent drift at each downstream yaml-executor
        // callsite. Uses `Result<serde_yaml::Value, serde_yaml::Error>`
        // to exercise the same `<T, E>` axis
        // [`check_yaml_parses`] threads through in production.
        let mut report = Report::default();
        let result: Result<serde_yaml::Value, serde_yaml::Error> =
            serde_yaml::from_str("kind: Process\n");
        assert!(result.is_ok(), "test fixture must construct an Ok result");
        report_result_prefixed(result, "sample label".to_owned(), "YAML", &mut report);
        assert!(
            report.is_ok(),
            "Ok result must not push a failure entry — a regression that inverted the arm polarity would surface here",
        );
        let pass = report
            .passes
            .last()
            .expect("Ok result must push exactly one pass entry");
        assert_eq!(
            pass, "sample label",
            "pass entry must carry the label verbatim — a regression that decorated the pass prose or dropped the label would surface here",
        );
    }

    #[test]
    fn report_result_prefixed_pushes_prefixed_fail_entry_on_err_result_composed_with_display() {
        // Miss-path pin: an `Err(e)` result lands as a fail entry
        // carrying the exact `<label>: <prefix>: <e>` byte shape both
        // pre-lift callsites hand-authored through the `Err(e) =>
        // report.fail(label, format!("<prefix>: {e}"))` arm. A
        // regression that dropped the `<prefix>: ` mid-clause,
        // swapped the separator, or dropped the `report.fail` side-
        // effect surfaces here. Uses `serde_yaml::from_str` on a
        // malformed source so the `E` carrier is the same
        // `serde_yaml::Error` production sees at
        // [`check_yaml_parses`].
        let mut report = Report::default();
        let result: Result<serde_yaml::Value, serde_yaml::Error> =
            serde_yaml::from_str("{unbalanced: [");
        assert!(result.is_err(), "test fixture must construct an Err result",);
        let displayed = match &result {
            Ok(_) => unreachable!(),
            Err(e) => e.to_string(),
        };
        report_result_prefixed(result, "sample label".to_owned(), "YAML", &mut report);
        assert!(
            !report.is_ok(),
            "Err result must push a failure entry — a regression that inverted the arm polarity or dropped the report.fail side-effect would surface here",
        );
        let failure = report
            .failures
            .last()
            .expect("Err result must push exactly one failure entry");
        let expected = format!("sample label: YAML: {displayed}");
        assert_eq!(
            failure, &expected,
            "failure entry must match `<label>: <prefix>: <Display error>` byte-for-byte — a regression that dropped the `<prefix>: ` mid-clause or drifted the separator would surface here",
        );
    }

    #[test]
    fn report_result_prefixed_matches_pre_lift_chain_bytewise_across_both_consumers() {
        // Sweep both pre-lift callsite prefix literals (`"YAML"` for
        // [`check_yaml_parses`], `"parse"` for
        // [`check_yaml_parses_as`]) and assert the substrate's fail
        // entry is byte-identical to the pre-lift hand-authored
        // `format!("<prefix>: {e}")` composition for each. Uses a
        // `&'static str` `E` carrier so the `Display` output is
        // deterministic across runs and the pin does not depend on
        // any error-crate's internal prose. Load-bearing: an
        // operator that greps for the pre-lift substring
        // `": YAML: "` or `": parse: "` sees the SAME sentinel
        // post-lift; a regression that reshuffled either prefix
        // fires HERE at ONE substrate site rather than as silent
        // drift across the two consumer callsites.
        for (prefix, err) in [
            ("YAML", "unbalanced brace"),
            ("parse", "unknown field `foo`"),
        ] {
            let mut report = Report::default();
            let result: Result<(), &'static str> = Err(err);
            report_result_prefixed(result, "sample label".to_owned(), prefix, &mut report);
            let failure = report
                .failures
                .last()
                .expect("Err result must push exactly one failure entry");
            let expected = format!("sample label: {prefix}: {err}");
            assert_eq!(
                failure, &expected,
                "sweep for prefix {prefix:?}: substrate output must match pre-lift `format!(\"{{prefix}}: {{e}}\")` byte shape verbatim",
            );
        }
    }

    #[test]
    fn report_result_prefixed_discards_the_ok_payload_and_pushes_the_label_alone() {
        // Ownership + payload-discard pin: the `Ok(_)` arm ignores the
        // payload — every yaml-executor-family consumer's success path
        // composes its pass entry from the label alone (no data from
        // the successful parse leaks into the pass prose). A
        // regression that swapped `Ok(_) => report.pass(label)` for
        // `Ok(v) => report.pass(format!("{label}: {v:?}"))` or
        // similar would fail here — an `Ok(String)` payload with a
        // deterministic string that would visibly appear in the pass
        // prose if the payload were captured.
        let mut report = Report::default();
        let result: Result<String, &'static str> = Ok("payload-that-must-not-leak".to_owned());
        report_result_prefixed(result, "sample label".to_owned(), "YAML", &mut report);
        let pass = report
            .passes
            .last()
            .expect("Ok result must push exactly one pass entry");
        assert_eq!(
            pass, "sample label",
            "pass entry must NOT contain the payload — a regression that captured the Ok payload into the pass prose would surface here",
        );
        assert!(
            !pass.contains("payload-that-must-not-leak"),
            "pass entry must NOT leak the Ok payload into the operator-facing prose",
        );
    }

    // ─── startup_diagnostic substrate pins ─────────────────────────────
    //
    // Fail-before-pass-after granularity: the `startup_diagnostic` free
    // function did not exist on the pre-lift binary — the tests below
    // do not compile before the lift. Post-lift they bind the exact
    // `"tatara-check: {detail}"` byte shape at ONE substrate owner so
    // a regression that drifted the `"tatara-check: "` prefix, dropped
    // the colon-space separator, or dropped the detail forwarding
    // surfaces HERE rather than as silent operator-visible drift
    // across every downstream startup-log grep at each of the four
    // pre-lift `eprintln!` sites in `main` (workspace-root miss,
    // checks.lisp read fail, checks.lisp parse fail, macroexpand fail).

    #[test]
    fn startup_diagnostic_prefixes_the_pipeline_name_verbatim() {
        // Primary shape: the returned string starts with the exact
        // `"tatara-check: "` prefix — binary name, colon, single space.
        // Every downstream startup-log grep keys on this sentinel to
        // distinguish startup-side diagnostics from per-check `"✗ "`
        // lines. A regression that swapped the prefix (a `[tatara-check]`
        // structured-log style, a versioned `tatara-check[0.2]:`, a
        // stripped colon) would fail here at ONE substrate site rather
        // than as silent drift across every `main`-side bail.
        let out = startup_diagnostic("anything");
        assert!(
            out.starts_with("tatara-check: "),
            "startup diagnostic must start with the `tatara-check: ` prefix (binary-name, colon, single space); got {out:?}"
        );
    }

    #[test]
    fn startup_diagnostic_forwards_a_static_string_literal_verbatim_after_the_prefix() {
        // Byte-identical parity with the pre-lift `eprintln!("tatara-
        // check: could not locate workspace root (looked for Cargo.toml
        // + checks.lisp)")` site. The primitive owns exactly the prefix
        // + colon + space sink; the caller's detail forwards verbatim.
        // A regression that transformed the detail (a `.to_uppercase()`,
        // an implicit `.trim()`, a re-wrapped format) would fail here.
        assert_eq!(
            startup_diagnostic(
                "could not locate workspace root (looked for Cargo.toml + checks.lisp)"
            ),
            "tatara-check: could not locate workspace root (looked for Cargo.toml + checks.lisp)",
        );
    }

    #[test]
    fn startup_diagnostic_forwards_a_format_args_composition_verbatim_after_the_prefix() {
        // Byte-identical parity with the pre-lift `eprintln!("tatara-
        // check: read {}: {e}", checks_path.display())` shape three of
        // the four `main`-side sites walked (`read`, `parse`,
        // `macroexpand`). The `format_args!` composition materializes
        // at the `impl Display` call boundary without allocating an
        // intermediate `String` — this pin proves the substrate accepts
        // the same shape the pre-lift callsites used, so the migration
        // is byte-identical AND allocation-parity-preserving on the
        // detail forwarding side.
        let e = "No such file or directory (os error 2)";
        let path = std::path::PathBuf::from("/tmp/checks.lisp");
        assert_eq!(
            startup_diagnostic(format_args!("read {}: {e}", path.display())),
            format!("tatara-check: read {}: {e}", path.display()),
        );
    }

    #[test]
    fn startup_diagnostic_forwards_an_owned_string_detail_verbatim_after_the_prefix() {
        // Ergonomic contract pin: a caller that pre-composes its detail
        // as an owned `String` (a future startup gate whose detail
        // depends on state assembled before the bail decision) can
        // still hand the value through the `impl Display` slot. Pre-lift
        // this shape did not exist in `main` — the four current sites
        // all used `&'static str` or `format_args!` — but the primitive
        // accepts it by construction because `String: Display`.
        // Post-lift the same primitive covers every future startup gate
        // regardless of how the detail was assembled.
        let owned: String = format!("macroexpand: {}", "unbound tail keyword");
        assert_eq!(
            startup_diagnostic(&owned),
            "tatara-check: macroexpand: unbound tail keyword",
        );
    }

    #[test]
    fn startup_diagnostic_matches_pre_lift_chain_bytewise_across_every_main_side_site() {
        // Sweep the four pre-lift `main`-side detail shapes and assert
        // the substrate's output is byte-identical to the pre-lift
        // hand-authored `format!("tatara-check: <detail>")` composition
        // every `eprintln!` site walked at its own callsite. Together
        // with the `starts_with` prefix pin above, this closes the
        // (prefix, separator, detail) triad at fail-before-pass-after
        // granularity — a regression at any one of the three sinks
        // fires HERE at ONE substrate site rather than as silent
        // operator-visible drift at the four downstream `main` sites.
        //
        // The sites and their exact pre-lift detail shapes:
        //
        // * workspace-root miss: bare literal.
        // * checks.lisp read fail: `format!("read {}: {e}", <path>)`.
        // * checks.lisp parse fail: `format!("parse {}: {e}", <path>)`.
        // * macroexpand fail: `format!("macroexpand: {e}")`.
        let path = std::path::PathBuf::from("/tmp/checks.lisp");
        let io_err = "No such file or directory (os error 2)";
        let parse_err = "unexpected EOF in list";
        let macro_err = "unbound macro tail keyword `:missing`";

        let pre_lift_workspace_root =
            "tatara-check: could not locate workspace root (looked for Cargo.toml + checks.lisp)"
                .to_string();
        let pre_lift_read = format!("tatara-check: read {}: {io_err}", path.display());
        let pre_lift_parse = format!("tatara-check: parse {}: {parse_err}", path.display());
        let pre_lift_macroexpand = format!("tatara-check: macroexpand: {macro_err}");

        assert_eq!(
            startup_diagnostic(
                "could not locate workspace root (looked for Cargo.toml + checks.lisp)"
            ),
            pre_lift_workspace_root,
            "workspace-root-miss detail must match the pre-lift `eprintln!` byte shape verbatim",
        );
        assert_eq!(
            startup_diagnostic(format_args!("read {}: {io_err}", path.display())),
            pre_lift_read,
            "checks.lisp-read detail must match the pre-lift `eprintln!` byte shape verbatim",
        );
        assert_eq!(
            startup_diagnostic(format_args!("parse {}: {parse_err}", path.display())),
            pre_lift_parse,
            "checks.lisp-parse detail must match the pre-lift `eprintln!` byte shape verbatim",
        );
        assert_eq!(
            startup_diagnostic(format_args!("macroexpand: {macro_err}")),
            pre_lift_macroexpand,
            "macroexpand detail must match the pre-lift `eprintln!` byte shape verbatim",
        );
    }

    #[test]
    fn startup_diagnostic_preserves_an_empty_detail_at_the_boundary() {
        // Boundary corner: an empty detail yields exactly the prefix
        // with nothing after the trailing space. Pin the shape so a
        // future normalization that trimmed trailing whitespace,
        // rejected empty details, or emitted a "no detail" sentinel
        // has to explicitly move THIS pin rather than silently changing
        // the byte shape for the corner. No pre-lift `main` site walks
        // this corner today, but the primitive's contract must accept
        // it (an `impl Display` slot with a zero-width value).
        assert_eq!(startup_diagnostic(""), "tatara-check: ");
    }

    #[test]
    fn read_or_fail_borrows_label_without_consuming_the_caller_side_string() {
        // Ergonomic contract pin: `read_or_fail(&path, &label, report)`
        // takes the label by borrow so the caller can still consume
        // its owned `String` label on the caller-side success path
        // (`report.pass(label)` or a subsequent `report.fail(label,
        // ...)` on a downstream error). A regression that moved the
        // label into the primitive (e.g. `impl Into<String>` on the
        // label slot) would surface HERE as a workspace-wide compile
        // error at every downstream `report.pass(label)` after a
        // successful read.
        let path = scratch_path("borrow");
        std::fs::write(&path, "x").expect("scratch write");
        let mut report = Report::default();
        let label: String = format!("borrow-test label: {}", path.display());
        let _ = read_or_fail(&path, &label, &mut report);
        let _ = std::fs::remove_file(&path);
        // Caller still owns `label` — consume it here to prove the
        // primitive did not move it.
        report.pass(label);
        assert_eq!(report.passes.len(), 1);
    }

    // ─── check_yaml_parses read-side substrate seal ─────────────────────
    //
    // Fail-before-pass-after granularity: pre-lift this pin cannot
    // compile because [`check_yaml_parses`] was not exposed to the
    // tests module — the pre-lift executor hand-authored a `match
    // fs::read_to_string(&path) { Ok(src) => <inline-pass>, Err(e) =>
    // report.fail(label, format!("read: {e}")) }` chain whose failure
    // arm was byte-identical to the substrate's owned prose but whose
    // pass arm wrapped `serde_yaml::from_str` INSIDE the `Ok(src)`
    // branch rather than early-returning through a let-else. That
    // shape divergence hid the fourth consumer from the earlier lift.
    // Post-lift the executor routes through [`read_or_fail`] and the
    // seal below observes the substrate's canonical
    // `<label>: read: <io_error>` prose landing at this executor's
    // failure entry — a regression that reintroduced the pre-lift
    // inline match at THIS executor (rather than the shared
    // substrate) would either produce a different label prefix, drop
    // the `"read: "` sentinel, or omit the failure altogether on the
    // miss path, and each such drift fires HERE at the executor
    // callsite rather than as silent operator-facing diagnostic skew
    // in a downstream check-log grep.

    #[test]
    fn check_yaml_parses_read_failure_prose_routes_through_read_or_fail_substrate() {
        // End-to-end: hand [`check_yaml_parses`] a nonexistent path
        // via the `(yaml-parses "…")` executor entry point and pin
        // that the failure prose carries the exact substrate-owned
        // shape `"<label>: read: "` where `<label> == "YAML parses:
        // <rel>"`. Rendering the source through the reader (rather
        // than composing `Sexp` values by hand) exercises the same
        // tokenizer + Sexp-shape pipeline the `dispatch` entry
        // reaches through in production, so a regression that
        // decoupled the reader's string-literal decode from the
        // executor's `required_positional_string` slot surfaces here
        // too.
        //
        // The path is a `<scratch_path>-derived leaf under the OS
        // temp dir keyed on the test-tag + PID so parallel test runs
        // never collide on the same inode; `remove_file` up front is
        // belt-and-braces so a residue from a previous failed run
        // can't accidentally satisfy the read.
        let missing_scratch = scratch_path("check-yaml-parses-missing");
        let _ = std::fs::remove_file(&missing_scratch);
        let rel = missing_scratch
            .file_name()
            .and_then(|s| s.to_str())
            .expect("scratch path has a UTF-8 leaf");
        let root = std::env::temp_dir();
        let src = format!("(yaml-parses \"{rel}\")");
        let forms = read(&src).expect("test source must parse");
        let outer = forms[0].as_list().expect("wrap source in a list");
        let mut report = Report::default();
        check_yaml_parses(&outer[1..], &root, &mut report);

        assert!(
            !report.is_ok(),
            "missing-path read must push a failure entry on the Report — a regression that dropped the substrate's `report.fail(label, format!(\"read: {{e}}\"))` side-effect on the Err arm would surface here as a silently-passing check",
        );
        let failure = report
            .failures
            .last()
            .expect("missing-path check must push exactly one failure");
        let expected_prefix = format!("YAML parses: {rel}: read: ");
        assert!(
            failure.starts_with(&expected_prefix),
            "check_yaml_parses miss-path prose must route through the read_or_fail substrate — the diagnostic prefix MUST match `<label>: read: <io_error>` where `<label> = \"YAML parses: <rel>\"`; got {failure:?}, expected prefix {expected_prefix:?}",
        );
        // Belt-and-braces: NO `YAML: ` sentinel — that prefix means
        // `serde_yaml::from_str` errored, i.e. the executor mis-
        // routed a read failure through the yaml-parse path.
        assert!(
            !failure.contains(": YAML: "),
            "check_yaml_parses miss-path prose must NOT carry the `: YAML: ` sentinel — that would mean the substrate's `Err(io)` arm was mis-routed through the yaml-parse fail path; got {failure:?}",
        );
    }

    #[test]
    fn check_yaml_parses_pass_path_still_matches_yaml_parse_prose_on_valid_source() {
        // Pass-path pin: an existing UTF-8 file with valid YAML must
        // still ride through both substrate primitives cleanly. Pre-
        // lift the pass path lived inside `Ok(src) => match
        // serde_yaml::from_str(&src) { ... }`; post-lift it lives on
        // the callsite after the substrate's let-else. A regression
        // that swapped the pass-path shape (drifted the `YAML parses:
        // <rel>` label prose, dropped the pass push, or accidentally
        // reported `YAML: <err>` on a well-formed input) surfaces
        // here rather than at the four-executor read-side seal
        // above.
        let happy_scratch = scratch_path("check-yaml-parses-happy");
        std::fs::write(&happy_scratch, "kind: Process\nmetadata:\n  name: probe\n")
            .expect("scratch write");
        let rel = happy_scratch
            .file_name()
            .and_then(|s| s.to_str())
            .expect("scratch path has a UTF-8 leaf")
            .to_owned();
        let root = std::env::temp_dir();
        let src = format!("(yaml-parses \"{rel}\")");
        let forms = read(&src).expect("test source must parse");
        let outer = forms[0].as_list().expect("wrap source in a list");
        let mut report = Report::default();
        check_yaml_parses(&outer[1..], &root, &mut report);
        let _ = std::fs::remove_file(&happy_scratch);
        assert!(
            report.is_ok(),
            "valid-YAML pass path must not push a failure entry — a regression that mis-routed the substrate's `Ok(src)` arm through the yaml-parse fail path would surface here",
        );
        let pass = report
            .passes
            .last()
            .expect("valid-YAML pass path must push exactly one pass");
        assert_eq!(
            pass,
            &format!("YAML parses: {rel}"),
            "pass-path label must carry the `YAML parses: <rel>` prose byte-for-byte — pre-lift + post-lift shapes must agree on the pass label at ONE substrate composition site",
        );
    }
}
