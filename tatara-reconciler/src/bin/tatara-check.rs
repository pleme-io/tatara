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
use tatara_process::boundary::{ConditionKind, ConditionSliceExt};
use tatara_process::classification::{
    CalmClassification, ConvergencePointType, DataClassification, HorizonKind,
    OptimizationDirection, SubstrateType,
};
use tatara_process::compliance::{ComplianceBindingSliceExt, VerificationPhase};
use tatara_process::encapsulates::EncapsulationMode;
use tatara_process::export::{
    ArtifactKind, ChannelKind, ExportSpecSliceExt, ExportTrigger, ReportFormat,
};
use tatara_process::intent::IntentKind;
use tatara_process::lifetime::{LifetimeKind, TeardownPolicy};
use tatara_process::routing::RoutingForm;
use tatara_process::signal::SighupStrategy;
use tatara_process::spec::{DependsOnSliceExt, MustReachPhase};
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
/// Twelve closed-set-driven prefix families dispatch through the
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
/// - `condition-<kind>` — [`ConditionKind`] closed set →
///   [`tatara_process::boundary::Boundary::has_condition_kind`] (an
///   inherent presence probe that unions `preconditions ∪
///   postconditions`, so the operator's `:requires (condition-<kind>)`
///   answers "does this spec name this boundary predicate anywhere"
///   without threading the pre/post side through the tag. Third
///   instance in the workspace-wide closed-set-driven presence-probe
///   algebra, opened alongside the fixed `boundary-pre` /
///   `boundary-post` tags which pin the presence of ANY condition on
///   their side; `condition-<kind>` pins the presence of a SPECIFIC
///   condition kind across both sides — the two surfaces answer
///   distinct questions and coexist.
/// - `must-reach-<kind>` — [`MustReachPhase`] closed set →
///   [`tatara_process::spec::DependsOnSliceExt::has_must_reach`] (a
///   slice-level extension-trait probe over `spec.depends_on`, so the
///   operator's `:requires (must-reach-Attested)` pins the presence of
///   a SPECIFIC gating checkpoint the Process waits on before
///   proceeding past Forking. Fourth instance in the workspace-wide
///   closed-set-driven presence-probe algebra, opened alongside the
///   fixed `depends-on` tag which pins the presence of ANY dependency
///   regardless of checkpoint; `must-reach-<kind>` pins the presence
///   of a SPECIFIC checkpoint across the dependency vector — the two
///   surfaces answer distinct questions and coexist. Second instance
///   in the slice-level closed-set-driven presence-probe algebra
///   (sibling to [`tatara_process::boundary::ConditionSliceExt::has_kind`]
///   on `&[Condition]`).
/// - `sighup-<kind>` — [`SighupStrategy`] closed set →
///   [`tatara_process::spec::SignalPolicy::has_sighup_strategy`] (a
///   scalar-carrier variant-equality probe on
///   `spec.signals.sighup_strategy`, so the operator's `:requires
///   (sighup-Reconverge)` pins that the Process handles SIGHUP by
///   re-converging without teardown, `:requires (sighup-Restart)`
///   pins full-respawn semantics, and `:requires (sighup-Noop)` pins
///   the deliberate ignore-the-signal posture. Fifth closed-set-
///   driven prefix family in the point-domain require-tag vocabulary
///   and FIRST instance on the scalar-carrier axis of the presence-
///   probe algebra — the field is non-Option, non-Vec, so on a
///   default [`tatara_process::spec::SignalPolicy`] the probe returns
///   `true` on the [`SighupStrategy::default`] variant
///   ([`SighupStrategy::Reconverge`]) rather than `false` for every
///   kind. Coexists with the coarse `signals` fixed tag (which
///   answers "is the sigterm-grace-seconds substrate default present"
///   without touching the SIGHUP-side of the policy) — the two tags
///   answer distinct questions.
/// - `verification-phase-<kind>` — [`VerificationPhase`] closed set →
///   [`tatara_process::compliance::ComplianceBindingSliceExt::has_verification_phase`]
///   (a slice-level extension-trait probe over
///   `spec.compliance.bindings`, so the operator's `:requires
///   (verification-phase-PlanTime)` pins the presence of a SPECIFIC
///   compliance-verification checkpoint the Process binds. Sixth
///   closed-set-driven prefix family in the point-domain require-tag
///   vocabulary and THIRD instance in the slice-level closed-set-
///   driven presence-probe algebra (sibling to
///   [`tatara_process::boundary::ConditionSliceExt::has_kind`] on
///   `&[Condition]` and
///   [`tatara_process::spec::DependsOnSliceExt::has_must_reach`] on
///   `&[DependsOn]`). Coexists with the coarse `compliance` fixed
///   tag (which answers "does the spec carry ANY compliance binding"
///   regardless of phase); `verification-phase-<kind>` pins the
///   presence of a SPECIFIC verification checkpoint across the
///   bindings vector — the two surfaces answer distinct questions.
/// - `export-when-<kind>` — [`ExportTrigger`] closed set →
///   [`tatara_process::export::ExportSpecSliceExt::has_when`] (a
///   slice-level extension-trait probe over
///   `spec.lifetime.resolved_ephemeral().map(|e| &e.exports[..])`, so
///   the operator's `:requires (export-when-OnAttested)` pins the
///   presence of a SPECIFIC export trigger the ephemeral Process
///   declares. Seventh closed-set-driven prefix family in the point-
///   domain require-tag vocabulary and FOURTH instance in the slice-
///   level closed-set-driven presence-probe algebra (sibling to
///   [`tatara_process::boundary::ConditionSliceExt::has_kind`] on
///   `&[Condition]`,
///   [`tatara_process::spec::DependsOnSliceExt::has_must_reach`] on
///   `&[DependsOn]`, and
///   [`tatara_process::compliance::ComplianceBindingSliceExt::has_verification_phase`]
///   on `&[ComplianceBinding]`). A permanent Process (or one with an
///   ambiguous `Lifetime` or an empty `exports` vector) returns
///   `false` for every trigger kind — the `resolved_ephemeral` gate
///   on the parent [`tatara_process::lifetime::Lifetime`] short-
///   circuits the walk. Coexists with the coarse `lifetime-ephemeral`
///   presence probe (which answers "is this Process ephemeral at
///   all" without discriminating on export triggers);
///   `export-when-<kind>` pins the presence of a SPECIFIC trigger
///   across the exports vector — the two surfaces answer distinct
///   questions.
/// - `channel-<kind>` — [`ChannelKind`] closed set →
///   [`tatara_process::export::ExportSpecSliceExt::has_channel_kind`]
///   (a second slice-level extension-trait probe over the SAME
///   `spec.lifetime.resolved_ephemeral().map(|e| &e.exports[..])`
///   projection the `export-when-<kind>` family walks, so the
///   operator's `:requires (channel-natsSubject)` pins the presence
///   of a SPECIFIC delivery channel across the exports vector. Eighth
///   closed-set-driven prefix family in the point-domain require-tag
///   vocabulary and SECOND method on [`ExportSpecSliceExt`] — the
///   first slice whose slice-level probe surface carries TWO closed-
///   set-driven presence probes on distinct axes (the `when` trigger
///   axis + the `channel` tagged-union axis) rather than a single one.
///   Same `resolved_ephemeral` parent gate as `export-when-<kind>`:
///   a permanent Process (or one with an ambiguous `Lifetime` or an
///   empty `exports` vector) returns `false` for every channel kind.
///   Coexists with `export-when-<kind>` on the SAME parent projection
///   (both walk the ephemeral `exports` slice) and answers a distinct
///   axis: `export-when-OnFailed` and `channel-natsSubject`
///   independently probe the trigger and the sink slot, so an audit
///   like "every `OnFailed` export ships through JetStream" reads as
///   the conjunction of the two require-tags at the checks.lisp
///   surface. Both walks compose the closed-set discriminator on the
///   raw field ([`ExportTrigger`] equality on `when`,
///   [`ChannelKind::select`] population on `channel`) rather than
///   the compound-projection `has_applicable_exports` walk on
///   `(when, phase) → fires_on(phase)` — the audit tag answers "is
///   this kind DECLARED" not "will it FIRE at some phase".
/// - `report-format-<kind>` — [`ReportFormat`] closed set →
///   [`tatara_process::export::ExportSpecSliceExt::has_report_format`]
///   (a THIRD slice-level extension-trait probe over the SAME
///   `spec.lifetime.resolved_ephemeral().map(|e| &e.exports[..])`
///   projection the `export-when-<kind>` and `channel-<kind>`
///   families walk, so the operator's `:requires
///   (report-format-Junit)` pins the presence of a SPECIFIC
///   test-report payload format across the exports vector. Ninth
///   closed-set-driven prefix family in the point-domain require-tag
///   vocabulary and THIRD method on [`ExportSpecSliceExt`] — the
///   first slice whose slice-level probe surface carries THREE
///   closed-set-driven presence probes on distinct axes (the `when`
///   trigger axis + the `channel` tagged-union axis + the
///   `source.test_report.format` NESTED-Option scalar axis). Same
///   `resolved_ephemeral` parent gate as `export-when-<kind>` +
///   `channel-<kind>`: a permanent Process (or one with an ambiguous
///   `Lifetime` or an empty `exports` vector) returns `false` for
///   every report format. Distinct from the two prior families in
///   ONE further dimension: an export whose `source` slot carries a
///   non-`test_report` variant (`receipts`, `process_snapshot`,
///   `run_marker`) contributes `false` for EVERY [`ReportFormat`],
///   INCLUDING the default [`ReportFormat::Raw`] that a naive
///   `unwrap_or_default()` projection would spuriously match — the
///   outer nested-Option projection collapses before the equality on
///   `format` fires. Coexists with `export-when-<kind>` and
///   `channel-<kind>` on the SAME parent projection and answers a
///   distinct axis: `export-when-OnAttested` + `channel-natsSubject`
///   + `report-format-Junit` independently probe the trigger, the
///   sink slot, and the payload format on the SAME `&[ExportSpec]`
///   slice, so an audit like "every `OnAttested` JUnit report ships
///   through JetStream" reads as the three-way conjunction of the
///   three require-tags at the checks.lisp surface.
/// - `artifact-<kind>` — [`ArtifactKind`] closed set →
///   [`tatara_process::export::ExportSpecSliceExt::has_artifact_kind`]
///   (a FOURTH slice-level extension-trait probe over the SAME
///   `spec.lifetime.resolved_ephemeral().map(|e| &e.exports[..])`
///   projection the `export-when-<kind>` / `channel-<kind>` /
///   `report-format-<kind>` families walk, so the operator's
///   `:requires (artifact-receipts)` pins the presence of a SPECIFIC
///   artifact-source variant across the exports vector. Tenth
///   closed-set-driven prefix family in the point-domain require-tag
///   vocabulary and FOURTH method on [`ExportSpecSliceExt`] — the
///   first slice whose slice-level probe surface carries FOUR
///   closed-set-driven presence probes on distinct axes (the `when`
///   trigger axis + the `channel` tagged-union axis + the
///   `source.test_report.format` NESTED-Option scalar axis + the
///   `source` OUTER tagged-union axis). Same `resolved_ephemeral`
///   parent gate as the three prior slice-level families. Distinct
///   from `report-format-<kind>` on ONE further dimension:
///   `has_report_format` reads a NESTED-Option scalar past the
///   `test_report` slot while `has_artifact_kind` reads the OUTER
///   tagged-union carrier directly, so a receipts-only export answers
///   `true` for `artifact-receipts` but `false` for every
///   `report-format-<kind>`. Coexists with the three prior slice-level
///   families on the SAME parent projection: `artifact-receipts` +
///   `export-when-OnAttested` + `channel-natsSubject` +
///   `report-format-Junit` independently probe the four axes on the
///   SAME `&[ExportSpec]` slice.
/// - `encapsulation-mode-<kind>` — [`EncapsulationMode`] closed set →
///   [`tatara_process::encapsulates::EncapsulatesSpec::has_mode`] (a
///   scalar-carrier variant-equality probe on
///   `spec.encapsulates.as_ref().map(|e| e.mode)`, so the operator's
///   `:requires (encapsulation-mode-Adopt)` pins that a Process is in
///   the `Adopt` mode of its wrapped pre-existing state, `:requires
///   (encapsulation-mode-Observe)` pins the read-only observing
///   posture, and `:requires (encapsulation-mode-Manage)` pins the
///   default full-ownership posture. Eleventh closed-set-driven prefix
///   family in the point-domain require-tag vocabulary and SECOND
///   instance on the scalar-carrier axis of the presence-probe algebra
///   (sibling to `sighup-<kind>` on
///   [`tatara_process::spec::SignalPolicy::has_sighup_strategy`]). Same
///   variant-equality semantics as `sighup-<kind>` but gated on the
///   parent `Option<EncapsulatesSpec>` presence via
///   `spec.encapsulates.as_ref().is_some_and(…)`: a greenfield Process
///   with `encapsulates: None` returns `false` for every mode —
///   including the default [`EncapsulationMode::Manage`] — because the
///   operator DECLINED the encapsulation surface entirely rather than
///   defaulting into it. This distinguishes the encapsulation-mode
///   axis from the sighup-strategy axis (which defaults into
///   [`SighupStrategy::Reconverge`] on a bare `SignalPolicy`): the
///   parent gate composes an Option-slot precondition onto a scalar-
///   carrier probe, closing the (Option-parent × scalar-child) corner
///   of the presence-probe algebra. Coexists with a hypothetical
///   coarse `encapsulates` fixed tag (not present today; would pin
///   "does the spec wrap ANY pre-existing state" without touching the
///   mode axis); the two answers are distinct.
/// - `point-type-<kind>` — [`ConvergencePointType`] closed set →
///   [`tatara_process::classification::Classification::has_point_type`]
///   (a scalar-carrier variant-equality probe on
///   `spec.classification.point_type`, so the operator's `:requires
///   (point-type-Gate)` pins that a Process names a `Gate` barrier
///   point on the six-axis classification lattice, `:requires
///   (point-type-Fork)` pins fan-out topology, `:requires
///   (point-type-Join)` pins fan-in topology, and so on across the
///   eight typed topology buckets. Twelfth closed-set-driven prefix
///   family in the point-domain require-tag vocabulary and THIRD
///   instance on the scalar-carrier axis of the presence-probe
///   algebra (sibling to `sighup-<kind>` on
///   [`tatara_process::spec::SignalPolicy::has_sighup_strategy`] and
///   `encapsulation-mode-<kind>` on
///   [`tatara_process::encapsulates::EncapsulatesSpec::has_mode`]).
///   Distinct from BOTH prior scalar-carrier peers on the
///   (parent-shape × child-shape) axis: `sighup-<kind>` lives on a
///   defaulted non-Option parent with a defaulted scalar child so a
///   substrate-default carrier reads `true` on the default variant;
///   `encapsulation-mode-<kind>` lives on an Option parent with a
///   defaulted scalar child so a bare `None` parent reads `false` on
///   every variant. `point-type-<kind>` lives on a REQUIRED,
///   non-Option, NON-DEFAULT parent
///   ([`tatara_process::classification::Classification`] has no
///   `impl Default` because its two required axes `point_type`
///   /`substrate` carry no default) with a NON-DEFAULT scalar child
///   ([`ConvergencePointType`] has no `impl Default`) — every
///   well-formed [`tatara_process::crd::ProcessSpec`] carries a
///   `Classification` whose `point_type` slot was deliberately chosen
///   by the operator, so exactly ONE of the eight `point-type-<kind>`
///   probes returns `true` and the other seven return `false`, with
///   no default-arm short-circuit that the two prior scalar-carrier
///   axes both publish. This closes the (required-parent ×
///   required-scalar-child) corner of the workspace-wide presence-
///   probe algebra at its first substrate primitive. Coexists with
///   every prior family — a `point-type-Gate` conjunction with
///   `sighup-Reconverge`, `encapsulation-mode-Manage`,
///   `condition-KustomizationHealthy`, etc. reads directly at the
///   checks.lisp surface as the four-way probe of the point's
///   topology axis, signal axis, encapsulation axis, and boundary
///   axis on ONE ProcessSpec.
/// - `substrate-<kind>` — [`SubstrateType`] closed set →
///   [`tatara_process::classification::Classification::has_substrate`]
///   (a scalar-carrier variant-equality probe on
///   `spec.classification.substrate`, so the operator's `:requires
///   (substrate-Compute)` pins that a Process names the `Compute`
///   plane on the six-axis classification lattice, `:requires
///   (substrate-Storage)` pins the storage plane, `:requires
///   (substrate-Observability)` pins the telemetry plane, and so on
///   across the eight typed substrate planes. Thirteenth closed-set-
///   driven prefix family in the point-domain require-tag vocabulary
///   and FOURTH instance on the scalar-carrier axis of the presence-
///   probe algebra (sibling to `sighup-<kind>` on
///   [`tatara_process::spec::SignalPolicy::has_sighup_strategy`],
///   `encapsulation-mode-<kind>` on
///   [`tatara_process::encapsulates::EncapsulatesSpec::has_mode`],
///   and `point-type-<kind>` on
///   [`tatara_process::classification::Classification::has_point_type`]).
///   FIRST co-tenant on the (required-parent × required-scalar-child)
///   corner of the presence-probe algebra with `point-type-<kind>`:
///   both probe REQUIRED, non-Option, NON-DEFAULT slots on the same
///   [`tatara_process::classification::Classification`] parent (whose
///   two required axes `point_type`/`substrate` carry no default)
///   with NON-DEFAULT scalar children ([`ConvergencePointType`] +
///   [`SubstrateType`] have no `impl Default`) — every well-formed
///   [`tatara_process::crd::ProcessSpec`] carries a `Classification`
///   whose `substrate` slot was deliberately chosen by the operator,
///   so exactly ONE of the eight `substrate-<kind>` probes returns
///   `true` and the other seven return `false`, with no default-arm
///   short-circuit that the two lower scalar-carrier axes
///   (`sighup-<kind>`, `encapsulation-mode-<kind>`) both publish.
///   This POPULATES the (required-parent × required-scalar-child)
///   corner of the workspace-wide presence-probe algebra at its
///   SECOND substrate primitive after `point-type-<kind>` opened the
///   corner, pinning the corner as a proven-repeatable primitive
///   shape rather than a single-example curiosity. Coexists with
///   every prior family — a `substrate-Storage` conjunction with
///   `point-type-Fork`, `sighup-Restart`,
///   `encapsulation-mode-Adopt`, `condition-KustomizationHealthy`,
///   etc. reads directly at the checks.lisp surface as the
///   independent-axis probe of the point's TWO required
///   classification axes against every optional/defaulted axis on
///   ONE ProcessSpec.
/// - `calm-<kind>` — [`CalmClassification`] closed set →
///   [`tatara_process::classification::Classification::has_calm`]
///   (a scalar-carrier variant-equality probe on
///   `spec.classification.calm`, so the operator's `:requires
///   (calm-Monotone)` pins that a Process's CALM axis names a
///   monotone operation (Hellerstein's theorem ⇒ distributable
///   without coordination), `:requires (calm-NonMonotone)` pins a
///   coordination-requiring operation, and the reconciler's future
///   dispatch between Raft writes and gossip propagation reads the
///   probe as the typed image of the theorem itself. Fourteenth
///   closed-set-driven prefix family in the point-domain
///   require-tag vocabulary and FIFTH instance on the scalar-carrier
///   axis of the presence-probe algebra (sibling to `sighup-<kind>`
///   on [`tatara_process::spec::SignalPolicy::has_sighup_strategy`],
///   `encapsulation-mode-<kind>` on
///   [`tatara_process::encapsulates::EncapsulatesSpec::has_mode`],
///   `point-type-<kind>` on
///   [`tatara_process::classification::Classification::has_point_type`],
///   and `substrate-<kind>` on
///   [`tatara_process::classification::Classification::has_substrate`]).
///   FIRST occupant on a FRESH corner of the (parent-shape ×
///   child-shape) axis: the required non-Option non-Default
///   [`tatara_process::classification::Classification`] parent
///   combined with a DEFAULTED scalar child
///   ([`CalmClassification`] defaults to
///   [`CalmClassification::Monotone`] via `#[default]`). Distinct
///   from every prior scalar-carrier peer on the (parent-shape ×
///   child-shape) axis: `sighup-<kind>` lives on a defaulted
///   non-Option parent with a defaulted child (a bare `SignalPolicy`
///   reads `true` on the default variant); `encapsulation-mode-<kind>`
///   lives on an Option parent with a defaulted child (a bare `None`
///   parent reads `false` on every variant); `point-type-<kind>` +
///   `substrate-<kind>` both live on the required parent with a
///   REQUIRED child (an operator must name the variant deliberately
///   to answer `true`). `calm-<kind>` lives on the required parent
///   with a DEFAULTED child — a bare `Classification` filled via
///   `..Default::default()` on the defaulted axes reads `true` for
///   the default variant ([`CalmClassification::Monotone`]) and
///   `false` for the other, and the default-arm short-circuit is
///   present (the operator can DECLINE to name the CALM axis and
///   the spec still answers `true` on the default variant). This
///   OPENS the (required-parent × defaulted-scalar-child) corner of
///   the workspace-wide presence-probe algebra at its first
///   substrate primitive — a corner distinct from all four prior
///   scalar-carrier corners. Coexists with every prior family — a
///   `calm-NonMonotone` conjunction with `point-type-Fork`,
///   `substrate-Storage`, `sighup-Restart`, etc. reads directly at
///   the checks.lisp surface as the three-axis probe of the point's
///   TWO required + ONE defaulted classification axes on ONE
///   ProcessSpec.
/// - `data-classification-<kind>` — [`DataClassification`] closed set
///   → [`tatara_process::classification::Classification::has_data_classification`]
///   (a scalar-carrier variant-equality probe on
///   `spec.classification.data_classification`, so the operator's
///   `:requires (data-classification-Pii)` pins that a Process's
///   data-sensitivity axis names PII (regulated data ⇒ implies
///   access control), `:requires (data-classification-Public)` pins
///   freely-distributable data, `:requires
///   (data-classification-Internal)` pins access-controlled but
///   non-regulated data, and the reconciler's future dispatch
///   between per-classification compliance overlays reads the probe
///   as the typed image of the sensitivity axis itself. Fifteenth
///   closed-set-driven prefix family in the point-domain require-tag
///   vocabulary and SIXTH instance on the scalar-carrier axis of
///   the presence-probe algebra (sibling to `sighup-<kind>` on
///   [`tatara_process::spec::SignalPolicy::has_sighup_strategy`],
///   `encapsulation-mode-<kind>` on
///   [`tatara_process::encapsulates::EncapsulatesSpec::has_mode`],
///   `point-type-<kind>` on
///   [`tatara_process::classification::Classification::has_point_type`],
///   `substrate-<kind>` on
///   [`tatara_process::classification::Classification::has_substrate`],
///   and `calm-<kind>` on
///   [`tatara_process::classification::Classification::has_calm`]).
///   SECOND occupant on the (required-parent × defaulted-scalar-
///   child) corner alongside `calm-<kind>` — both probe a defaulted
///   scalar child ([`DataClassification`] defaults to
///   [`DataClassification::Internal`] via `#[default]`, sibling to
///   [`CalmClassification::Monotone`]'s `#[default]`) on the same
///   required [`tatara_process::classification::Classification`]
///   parent, so a bare `Classification` filled via
///   `..Default::default()` on the `data_classification` axis reads
///   `true` on the default variant
///   ([`DataClassification::Internal`]) and `false` on every other,
///   and the default-arm short-circuit is present (the operator can
///   DECLINE to name the data-classification axis and the spec
///   still answers `true` on the default variant). This POPULATES
///   the (required-parent × defaulted-scalar-child) corner at its
///   SECOND substrate primitive after `calm-<kind>` opened it —
///   pinning the corner as a proven-repeatable primitive shape
///   rather than a single-example curiosity AND closing the four-
///   scalar-carrier corner-coverage contract on the six-axis
///   classification lattice (all four scalar closed-set
///   discriminator axes on [`tatara_process::classification::Classification`]
///   now publish independent presence probes through the same
///   shape: two on the required-child corner, two on the defaulted-
///   child corner). Coexists with every prior family — a
///   `data-classification-Pii` conjunction with `point-type-Fork`,
///   `substrate-Storage`, `calm-NonMonotone`, `sighup-Restart`, etc.
///   reads directly at the checks.lisp surface as the four-axis
///   probe of the point's TWO required + TWO defaulted
///   classification axes on ONE ProcessSpec.
/// - `horizon-<kind>` — [`HorizonKind`] closed set →
///   [`tatara_process::classification::Classification::has_horizon_kind`]
///   (a nested-struct-scalar-carrier variant-equality probe on
///   `spec.classification.horizon.kind`, so the operator's
///   `:requires (horizon-Bounded)` pins that a Process's horizon
///   shape names a terminating fixed-point run (distance ⇒ 0 ⇒
///   `Reaped`), `:requires (horizon-Asymptotic)` pins a perpetual
///   run with a rate-window health signal, and the reconciler's
///   future dispatch between the two scheduler paths reads the probe
///   as the typed image of the horizon axis itself. SIXTEENTH
///   closed-set-driven prefix family in the point-domain require-tag
///   vocabulary and FIRST instance on the NESTED-STRUCT-scalar-child
///   corner of the (parent-shape × child-shape) presence-probe
///   algebra — distinct from all four corner-property-exhaustive
///   scalar-carrier peers on
///   [`tatara_process::classification::Classification`]
///   (`point-type-<kind>` / `substrate-<kind>` on the required-child
///   corner, `calm-<kind>` / `data-classification-<kind>` on the
///   defaulted-child corner, all four reading a closed-set
///   discriminator DIRECTLY off a scalar `Classification` slot
///   without an intermediate struct hop). `has_horizon_kind` threads
///   through the nested defaulted [`Horizon`] intermediary to reach
///   the scalar [`HorizonKind`] discriminator on `horizon.kind`. The
///   parent-shape half of the corner is required (a `Classification`
///   has no `impl Default`); the child-shape half is a nested
///   defaulted struct (`Horizon: Default`) threading a defaulted
///   scalar ([`HorizonKind::Bounded`] via `#[default]`), so a bare
///   `Classification` filled via `..Default::default()` on the
///   `horizon` axis reads `true` on the default kind
///   [`HorizonKind::Bounded`] and `false` on
///   [`HorizonKind::Asymptotic`], and the default-arm short-circuit
///   is present (the operator can DECLINE to name the horizon axis
///   and the spec still answers `true` on the default kind).
///   Coexists with every prior family — a `horizon-Asymptotic`
///   conjunction with `point-type-Fork`, `substrate-Storage`,
///   `calm-NonMonotone`, `data-classification-Pii`, `sighup-Restart`,
///   etc. reads directly at the checks.lisp surface as the five-axis
///   probe of the point's TWO required-scalar + TWO defaulted-scalar
///   + ONE nested-struct-scalar classification axes on ONE ProcessSpec.
/// - `optimization-direction-<kind>` — [`OptimizationDirection`]
///   closed set →
///   [`tatara_process::classification::Classification::has_optimization_direction`]
///   (a nested-struct-Option-scalar-carrier variant-equality probe
///   on `spec.classification.horizon.direction.unwrap_or_default()`,
///   so the operator's `:requires (optimization-direction-Minimize)`
///   pins that a Process's asymptotic-horizon optimization polarity
///   is cost/latency/error-rate-oriented — the rate-window
///   evaluator treats decreasing samples as healthy — and
///   `:requires (optimization-direction-Maximize)` pins the
///   throughput/coverage/revenue-oriented polarity. SEVENTEENTH
///   closed-set-driven prefix family in the point-domain require-
///   tag vocabulary and SECOND occupant on the (required-parent ×
///   nested-struct-scalar-child) corner of the (parent-shape ×
///   child-shape) presence-probe algebra — direct successor of
///   `horizon-<kind>` on the same fresh corner, threading through
///   the SAME nested [`Horizon`] intermediary but with an
///   `Option`-hop past `direction: Option<OptimizationDirection>`
///   via `Option::unwrap_or_default`. The Option-hop is soft-mapped
///   to the closed set's `#[default] Minimize` variant, so a bare
///   `Classification` filled via `..Default::default()` on the
///   `horizon.direction` axis (which the workspace-baseline
///   [`tatara_process::crd::ProcessSpec::gate_compute_defaults`]
///   emits) reads `true` on `optimization-direction-Minimize` and
///   `false` on `optimization-direction-Maximize`, and the
///   default-arm short-circuit is present (the operator can DECLINE
///   to name the direction axis and the spec still answers `true`
///   on the closed set's default). Coexists with every prior family
///   — an `optimization-direction-Maximize` conjunction with
///   `horizon-Asymptotic`, `point-type-Fork`, `substrate-Storage`,
///   `calm-NonMonotone`, `data-classification-Pii`, `sighup-Restart`,
///   etc. reads directly at the checks.lisp surface as the six-axis
///   probe of the point's TWO required-scalar + TWO defaulted-scalar
///   + TWO nested-struct-scalar classification axes on ONE
///   ProcessSpec — closing the SIX-axis corner-coverage contract
///   on the six-dimensional classification lattice.
/// - `teardown-policy-<kind>` — [`TeardownPolicy`] closed set →
///   [`tatara_process::lifetime::EphemeralLifetime::has_teardown_policy`]
///   (a defaulted-scalar-carrier variant-equality probe on
///   `spec.lifetime.resolved_ephemeral()
///     .is_some_and(|e| e.teardown_policy == kind)`, so the operator's
///   `:requires (teardown-policy-OnAttested)` pins that an ephemeral
///   Process auto-terminates only on `Attested` (leave `Failed`
///   Processes for forensic inspection), `:requires
///   (teardown-policy-OnFailed)` pins the symmetric "leave `Attested`
///   running until TTL / manual SIGTERM" posture, `:requires
///   (teardown-policy-Never)` pins the TTL-only lifetime, and the
///   reconciler's `lifetime_clock::evaluate` reads the SAME closed-set
///   discriminator when firing `AutoTerminate::Now`. EIGHTEENTH
///   closed-set-driven prefix family in the point-domain require-tag
///   vocabulary and FIRST occupant on the (Option-parent ×
///   defaulted-scalar-child) corner of the (parent-shape × child-
///   shape) presence-probe algebra — distinct from every prior corner:
///   the parent-shape half is Option-typed (`resolved_ephemeral()`
///   returns `None` on a `Permanent` lifetime or an ambiguous
///   `Lifetime`); the child-shape half is a defaulted scalar
///   ([`TeardownPolicy::Always`] via `#[default]`). A `Permanent`
///   lifetime short-circuits to `false` for EVERY kind (the Option-
///   parent gate fires), while an [`EphemeralLifetime::default`] reads
///   `true` on `teardown-policy-Always` and `false` on every other
///   variant (the defaulted scalar child publishes the closed set's
///   default). The dual short-circuit pins the (Option-parent ×
///   defaulted-scalar-child) corner as a distinct primitive shape:
///   the Option-parent arm silences EVERY kind, while the reachable-
///   default-child arm honors the closed set's own `#[default]`.
///   Coexists with every prior lifetime-adjacent family — a
///   `teardown-policy-OnAttested` conjunction with `lifetime-ephemeral`
///   (coarse `Lifetime` variant), `export-when-OnAttested` (slice-level
///   sibling on the SAME resolved-ephemeral Option-parent), etc. reads
///   directly at the checks.lisp surface as the three-axis probe of
///   the point's ephemeral lifetime shape.
/// - `routing-form-<kind>` — [`RoutingForm`] closed set →
///   [`tatara_process::routing::RoutingSpec::has_form`] (a derived-
///   scalar-carrier variant-equality probe on
///   `spec.routing.as_ref().is_some_and(|r| r.form() == kind)`, where
///   `RoutingSpec::form()` composes through the ONE substrate
///   projection [`RoutingForm::from_is_stable`] over the
///   `stable_name_claim` bool). The operator's `:requires
///   (routing-form-stable)` pins that a Process declares intent to
///   hold the ProcessTable claim for `(cluster, app)` and emit the
///   unprefixed `${app}.${cluster}.${loc}.${domain}` FQDN, `:requires
///   (routing-form-instance)` pins the default per-instance FQDN
///   shape (`${app}.${eph_id}.${cluster}.${loc}.${domain}`), and the
///   reconciler's `render_routing` reads the SAME closed-set
///   discriminator when stamping the [`crate::annotations::ROUTING_FORM`]
///   annotation / label on every emitted routing edge. NINETEENTH
///   closed-set-driven prefix family in the point-domain require-tag
///   vocabulary and SECOND occupant on the (Option-parent × defaulted-
///   scalar-child) corner opened by `teardown-policy-<kind>` — the
///   parent-shape half is Option-typed (`spec.routing` is
///   `Option<RoutingSpec>`, `None` on an in-cluster-only Process),
///   and the child-shape half is a defaulted scalar
///   ([`RoutingForm::Instance`] via `#[serde(default)]` on
///   `stable_name_claim: bool` composed through
///   [`RoutingForm::from_is_stable`]). Distinct from `teardown-policy-<kind>`
///   in that the child is DERIVED from a raw bool through
///   [`RoutingForm::from_is_stable`], not stored as-is — a first
///   demonstration that the (Option-parent × defaulted-scalar-child)
///   corner admits both stored-child and derived-child traversals
///   through the SAME `has(kind)` shape. An absent `spec.routing`
///   short-circuits to `false` for EVERY kind (the Option-parent gate
///   fires), while a `RoutingSpec` with `stable_name_claim: false`
///   (the serde default) reads `true` on `routing-form-instance` and
///   `false` on `routing-form-stable` (the derived defaulted-scalar
///   child publishes `RoutingForm::from_is_stable(false) = Instance`).
///   Coexists with every prior family — a `routing-form-stable`
///   conjunction with `lifetime-ephemeral` reads directly as "an
///   ephemeral Process that declares intent to hold the stable-name
///   claim," a common posture for canary previews of a stable-name
///   Process. Semantics — DECLARED intent, not RESOLVED emission: a
///   `stable_name_claim: true` spec that loses the ProcessTable claim
///   to a higher-priority peer still reads `routing-form-stable`
///   as true (the *declared* intent); the reconciler's claim
///   arbitration decides the actual emission downstream at
///   `render_routing`.
///
/// Every other tag is a fixed match on a non-closed-set spec field —
/// `depends-on`, `boundary-pre`, `boundary-post`, `compliance`,
/// `signals`. These stay as hand-authored arms until a matching
/// closed-set surface opens for them (each addresses a slot whose
/// carrier isn't a closed-set discriminator today).
///
/// A future new `IntentKind` / `LifetimeKind` / `ConditionKind` /
/// `MustReachPhase` / `SighupStrategy` / `VerificationPhase` /
/// `ExportTrigger` / `ChannelKind` / `ReportFormat` / `ArtifactKind` /
/// `EncapsulationMode` / `ConvergencePointType` / `SubstrateType` /
/// `CalmClassification` / `DataClassification` / `HorizonKind` /
/// `OptimizationDirection` / `TeardownPolicy` / `RoutingForm` variant
/// lands at ONE `ALL` entry on its parent's closed set — no per-caller
/// edit here. A future new prefix family (e.g. a `phase-<kind>` for
/// [`tatara_process::phase::ProcessPhase`], a hypothetical
/// `max-concurrent-tier-<kind>` reaching through
/// `spec.lifetime.resolved_ephemeral().map(|e|
/// tier_of(e.max_concurrent))`, a third co-tenant on the
/// (Option-parent × defaulted-scalar-child) corner now doubly
/// populated by `teardown-policy-<kind>` (stored child) and
/// `routing-form-<kind>` (derived child), or another co-tenant on the
/// (required-parent × nested-struct-scalar-child) corner doubly
/// populated by `horizon-<kind>` + `optimization-direction-<kind>`)
/// lands as ONE more `if let Some(res) =
/// strip_and_classify_prefixed_kind::<NewKind, _>(tag, "prefix-", |k|
/// spec.<field>.has(k)) { return res; }` branch that reads the same
/// three-step (strip_prefix + parse + has) shape all nineteen existing
/// families publish.
///
/// Pinned by [`tests::evaluate_point_require_tag_returns_true_on_populated_lifetime_slot_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_default_lifetime_for_every_kind`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_lifetime_suffix`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_bare_lifetime_prefix`],
/// [`tests::evaluate_point_require_tag_returns_true_on_populated_condition_slot_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_empty_boundary_for_every_condition_kind`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_condition_suffix`],
/// [`tests::evaluate_point_require_tag_unions_pre_and_post_conditions_for_condition_prefix`],
/// [`tests::evaluate_point_require_tag_returns_true_on_populated_must_reach_slot_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_empty_depends_on_for_every_must_reach_kind`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_must_reach_suffix`],
/// [`tests::evaluate_point_require_tag_returns_true_iff_sighup_strategy_matches_variant_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_true_on_default_signals_for_sighup_reconverge_only`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_sighup_suffix`],
/// [`tests::evaluate_point_require_tag_returns_true_on_populated_verification_phase_slot_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_empty_compliance_for_every_verification_phase_kind`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_verification_phase_suffix`],
/// [`tests::evaluate_point_require_tag_verification_phase_and_compliance_coexist`],
/// [`tests::evaluate_point_require_tag_returns_true_on_populated_channel_slot_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_permanent_lifetime_for_every_channel_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_empty_exports_for_every_channel_kind`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_channel_suffix`],
/// [`tests::evaluate_point_require_tag_channel_and_export_when_coexist`],
/// [`tests::evaluate_point_require_tag_returns_true_on_populated_report_format_slot_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_permanent_lifetime_for_every_report_format_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_empty_exports_for_every_report_format_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_non_test_report_source_for_every_report_format_kind`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_report_format_suffix`],
/// [`tests::evaluate_point_require_tag_report_format_export_when_and_channel_coexist`],
/// [`tests::evaluate_point_require_tag_returns_true_on_populated_artifact_slot_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_permanent_lifetime_for_every_artifact_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_empty_exports_for_every_artifact_kind`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_artifact_suffix`],
/// [`tests::evaluate_point_require_tag_artifact_coexists_with_prior_export_axes`],
/// [`tests::evaluate_point_require_tag_returns_true_iff_encapsulation_mode_matches_variant_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_absent_encapsulates_for_every_mode`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_encapsulation_mode_suffix`],
/// [`tests::evaluate_point_require_tag_encapsulation_mode_and_sighup_scalar_carriers_coexist`],
/// [`tests::evaluate_point_require_tag_returns_true_iff_point_type_matches_variant_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_point_type_suffix`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_bare_point_type_prefix`],
/// [`tests::evaluate_point_require_tag_point_type_coexists_with_prior_scalar_carriers`],
/// [`tests::evaluate_point_require_tag_returns_true_iff_substrate_matches_variant_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_substrate_suffix`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_bare_substrate_prefix`],
/// [`tests::evaluate_point_require_tag_substrate_coexists_with_point_type`],
/// [`tests::evaluate_point_require_tag_returns_true_iff_calm_matches_variant_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_true_on_default_calm_for_monotone_only`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_calm_suffix`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_bare_calm_prefix`],
/// [`tests::evaluate_point_require_tag_calm_coexists_with_prior_classification_axes`],
/// [`tests::evaluate_point_require_tag_returns_true_iff_data_classification_matches_variant_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_true_on_default_data_classification_for_internal_only`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_data_classification_suffix`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_bare_data_classification_prefix`],
/// [`tests::evaluate_point_require_tag_data_classification_coexists_with_prior_classification_axes`],
/// [`tests::evaluate_point_require_tag_returns_true_iff_horizon_kind_matches_variant_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_true_on_default_horizon_kind_for_bounded_only`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_horizon_kind_suffix`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_bare_horizon_kind_prefix`],
/// [`tests::evaluate_point_require_tag_horizon_kind_coexists_with_prior_classification_axes`],
/// [`tests::evaluate_point_require_tag_returns_true_iff_optimization_direction_matches_variant_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_true_on_default_optimization_direction_for_minimize_only`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_optimization_direction_suffix`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_bare_optimization_direction_prefix`],
/// [`tests::evaluate_point_require_tag_optimization_direction_coexists_with_prior_classification_axes`],
/// [`tests::evaluate_point_require_tag_returns_true_on_populated_teardown_policy_slot_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_permanent_lifetime_for_every_teardown_policy_kind`],
/// [`tests::evaluate_point_require_tag_returns_true_on_default_ephemeral_teardown_policy_for_always_only`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_teardown_policy_suffix`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_bare_teardown_policy_prefix`],
/// [`tests::evaluate_point_require_tag_teardown_policy_coexists_with_export_when_and_lifetime_ephemeral`],
/// [`tests::evaluate_point_require_tag_returns_true_iff_routing_form_matches_variant_per_kind`],
/// [`tests::evaluate_point_require_tag_returns_false_on_absent_routing_for_every_routing_form_kind`],
/// [`tests::evaluate_point_require_tag_returns_true_on_default_routing_form_for_instance_only`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_unknown_routing_form_suffix`],
/// [`tests::evaluate_point_require_tag_returns_unknown_on_bare_routing_form_prefix`],
/// and [`tests::evaluate_point_require_tag_routing_form_coexists_with_lifetime_ephemeral_and_teardown_policy`].
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
    if let Some(res) =
        strip_and_classify_prefixed_kind::<ConditionKind, _>(tag, "condition-", |kind| {
            spec.boundary.has_condition_kind(kind)
        })
    {
        return res;
    }
    if let Some(res) =
        strip_and_classify_prefixed_kind::<MustReachPhase, _>(tag, "must-reach-", |kind| {
            spec.depends_on.has_must_reach(kind)
        })
    {
        return res;
    }
    if let Some(res) =
        strip_and_classify_prefixed_kind::<SighupStrategy, _>(tag, "sighup-", |kind| {
            spec.signals.has_sighup_strategy(kind)
        })
    {
        return res;
    }
    if let Some(res) = strip_and_classify_prefixed_kind::<VerificationPhase, _>(
        tag,
        "verification-phase-",
        |kind| spec.compliance.bindings.has_verification_phase(kind),
    ) {
        return res;
    }
    if let Some(res) =
        strip_and_classify_prefixed_kind::<ExportTrigger, _>(tag, "export-when-", |kind| {
            spec.lifetime
                .resolved_ephemeral()
                .is_some_and(|e| e.exports.has_when(kind))
        })
    {
        return res;
    }
    if let Some(res) = strip_and_classify_prefixed_kind::<ChannelKind, _>(tag, "channel-", |kind| {
        spec.lifetime
            .resolved_ephemeral()
            .is_some_and(|e| e.exports.has_channel_kind(kind))
    }) {
        return res;
    }
    if let Some(res) =
        strip_and_classify_prefixed_kind::<ReportFormat, _>(tag, "report-format-", |kind| {
            spec.lifetime
                .resolved_ephemeral()
                .is_some_and(|e| e.exports.has_report_format(kind))
        })
    {
        return res;
    }
    if let Some(res) =
        strip_and_classify_prefixed_kind::<ArtifactKind, _>(tag, "artifact-", |kind| {
            spec.lifetime
                .resolved_ephemeral()
                .is_some_and(|e| e.exports.has_artifact_kind(kind))
        })
    {
        return res;
    }
    if let Some(res) = strip_and_classify_prefixed_kind::<EncapsulationMode, _>(
        tag,
        "encapsulation-mode-",
        |kind| spec.encapsulates.as_ref().is_some_and(|e| e.has_mode(kind)),
    ) {
        return res;
    }
    if let Some(res) =
        strip_and_classify_prefixed_kind::<ConvergencePointType, _>(tag, "point-type-", |kind| {
            spec.classification.has_point_type(kind)
        })
    {
        return res;
    }
    if let Some(res) =
        strip_and_classify_prefixed_kind::<SubstrateType, _>(tag, "substrate-", |kind| {
            spec.classification.has_substrate(kind)
        })
    {
        return res;
    }
    if let Some(res) =
        strip_and_classify_prefixed_kind::<CalmClassification, _>(tag, "calm-", |kind| {
            spec.classification.has_calm(kind)
        })
    {
        return res;
    }
    if let Some(res) = strip_and_classify_prefixed_kind::<DataClassification, _>(
        tag,
        "data-classification-",
        |kind| spec.classification.has_data_classification(kind),
    ) {
        return res;
    }
    if let Some(res) = strip_and_classify_prefixed_kind::<HorizonKind, _>(tag, "horizon-", |kind| {
        spec.classification.has_horizon_kind(kind)
    }) {
        return res;
    }
    if let Some(res) = strip_and_classify_prefixed_kind::<OptimizationDirection, _>(
        tag,
        "optimization-direction-",
        |kind| spec.classification.has_optimization_direction(kind),
    ) {
        return res;
    }
    if let Some(res) =
        strip_and_classify_prefixed_kind::<TeardownPolicy, _>(tag, "teardown-policy-", |kind| {
            spec.lifetime
                .resolved_ephemeral()
                .is_some_and(|e| e.has_teardown_policy(kind))
        })
    {
        return res;
    }
    if let Some(res) =
        strip_and_classify_prefixed_kind::<RoutingForm, _>(tag, "routing-form-", |kind| {
            spec.routing.as_ref().is_some_and(|r| r.has_form(kind))
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
/// shape the nineteen closed-set-driven prefix families in
/// [`evaluate_point_require_tag`] (`intent-<kind>` on [`IntentKind`],
/// `lifetime-<kind>` on [`LifetimeKind`], `condition-<kind>` on
/// [`ConditionKind`], `must-reach-<kind>` on [`MustReachPhase`],
/// `sighup-<kind>` on [`SighupStrategy`], `verification-phase-<kind>`
/// on [`VerificationPhase`], `export-when-<kind>` on
/// [`ExportTrigger`], `channel-<kind>` on [`ChannelKind`],
/// `report-format-<kind>` on [`ReportFormat`], `artifact-<kind>` on
/// [`ArtifactKind`], `encapsulation-mode-<kind>` on
/// [`EncapsulationMode`], `point-type-<kind>` on
/// [`ConvergencePointType`], `substrate-<kind>` on
/// [`SubstrateType`], `calm-<kind>` on [`CalmClassification`],
/// `data-classification-<kind>` on [`DataClassification`],
/// `horizon-<kind>` on [`HorizonKind`],
/// `optimization-direction-<kind>` on [`OptimizationDirection`],
/// `teardown-policy-<kind>` on [`TeardownPolicy`],
/// `routing-form-<kind>` on [`RoutingForm`]) each
/// dispatch through past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// threshold.
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
/// A future twentieth closed-set prefix family — a `phase-<kind>`
/// on [`tatara_process::phase::ProcessPhase`], a hypothetical
/// `max-concurrent-tier-<kind>` reaching through
/// `spec.lifetime.resolved_ephemeral().map(|e|
/// tier_of(e.max_concurrent))`, a third co-tenant on the
/// (Option-parent × defaulted-scalar-child) corner now doubly
/// populated by `teardown-policy-<kind>` (stored-child) and
/// `routing-form-<kind>` (derived-child), a peer probe on a
/// different Option-parent nested defaulted-scalar field like
/// `spec.encapsulates.as_ref().map(|e| e.mode)` if a defaulted-scalar
/// mode axis is added there, or a further co-tenant on the
/// (required-parent × nested-struct-scalar-child) corner doubly
/// populated by `horizon-<kind>` + `optimization-direction-<kind>` —
/// lands as ONE more `if let Some(res) =
/// strip_and_classify_prefixed_kind::<NewKind, _>(tag, "prefix-", |k|
/// spec.<field>.has(k)) { return res; }` branch that reads the same
/// three-step shape the nineteen existing families publish. No
/// per-caller `strip_prefix + parse + match { Ok(_) => …, Err(_) =>
/// Err(UnknownRequireTag) }` restatement.
///
/// A future diagnostic shift (attaching the offending suffix to
/// [`UnknownRequireTag`], promoting the sentinel to carry a
/// `<K as ClosedSet>::labels_joined("/")` near-miss list) lands at
/// THIS ONE substrate owner and both current prefix families plus
/// every future closed-set prefix family inherit the shift by
/// construction.
///
/// Theory anchor: THEORY.md §VI.1 — generation over composition; the
/// three-step chain now dispatches NINETEEN closed-set prefix
/// families past the ≥2 PRIME-DIRECTIVE trigger through ONE
/// substrate owner. THEORY.md §II.1 invariant 2 — free middle; the
/// caller composes the closed-set choice (via the generic `K`) and
/// the presence probe (via the `probe` closure) independently, so a
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
/// One closed-set-driven prefix family dispatches through the
/// autoderived `FromStr` + the substrate presence probe on its
/// discriminator's parent:
///
/// - `condition-<kind>` — [`ConditionKind`] closed set →
///   [`tatara_process::ephemeral::EphemeralSpec::has_condition_kind`]
///   (an inherent presence probe that unions `preconditions ∪
///   postconditions`, so the operator's `:requires (condition-<kind>)`
///   answers "does this ephemeral spec name this boundary predicate
///   anywhere" without threading the pre/post side through the tag).
///   Byte-for-byte symmetrical with the peer `condition-<kind>` family
///   on the point surface via
///   [`tatara_process::boundary::Boundary::has_condition_kind`] — both
///   union bodies compose through the SAME slice-level substrate
///   primitive
///   [`tatara_process::boundary::ConditionSliceExt::has_kind`]. First
///   closed-set prefix family in the ephemeral require-tag vocabulary.
///
/// Every other tag is a fixed match on an [`EphemeralSpec`] slot; the
/// remaining sugar-surface knobs (`aplicacao`, `ttl`, `teardown`,
/// `postconditions`, `preconditions`, `closed-loop-auth`) aren't
/// discriminators of a closed set on `EphemeralSpec`, so they stay as
/// hand-authored arms until a matching closed-set surface opens for
/// them. The remaining vocabulary:
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
///   Composes through
///   [`tatara_process::boundary::ConditionSliceExt::has_kind`] — the
///   ONE substrate primitive that owns the
///   `(&[Condition], ConditionKind) -> bool` walk shape both this
///   ephemeral arm (on `spec.postconditions` alone) and the new
///   `condition-<kind>` prefix family above (on the pre ∪ post union)
///   compose against. The two tags coexist: `closed-loop-auth` pins
///   the post-only presence probe on ONE specific kind (the
///   destination-state boundary theorem's canonical postcondition);
///   `condition-ClosedLoopAuth` pins the pre ∪ post union answer on
///   the same kind. Both queries are useful — a spec asserting the
///   theorem via a pre-condition against a fixture issuer satisfies
///   `condition-ClosedLoopAuth` but not `closed-loop-auth`.
///
/// A future closed-set prefix family lands as one
/// `if let Some(res) = strip_and_classify_prefixed_kind::<NewKind, _>(
/// tag, "prefix-", |k| spec.<field>.has(k)) { return res; }` branch
/// that reads the same three-step (strip_prefix + parse + has) shape
/// [`evaluate_point_require_tag`] publishes.
///
/// Pinned by [`tests::evaluate_ephemeral_require_tag_routes_populated_slots_true`],
/// [`tests::evaluate_ephemeral_require_tag_returns_false_on_empty_slots`],
/// [`tests::evaluate_ephemeral_require_tag_returns_unknown_on_out_of_vocabulary_tag`],
/// [`tests::evaluate_ephemeral_require_tag_closed_loop_auth_reads_postcondition_kind`],
/// [`tests::evaluate_ephemeral_require_tag_returns_true_on_populated_condition_slot_per_kind`],
/// [`tests::evaluate_ephemeral_require_tag_returns_false_on_empty_slots_for_every_condition_kind`],
/// [`tests::evaluate_ephemeral_require_tag_returns_unknown_on_unknown_condition_suffix`],
/// and [`tests::evaluate_ephemeral_require_tag_unions_pre_and_post_conditions_for_condition_prefix`].
fn evaluate_ephemeral_require_tag(
    spec: &tatara_process::ephemeral::EphemeralSpec,
    tag: &str,
) -> Result<bool, UnknownRequireTag> {
    if let Some(res) =
        strip_and_classify_prefixed_kind::<ConditionKind, _>(tag, "condition-", |kind| {
            spec.has_condition_kind(kind)
        })
    {
        return res;
    }
    match tag {
        "aplicacao" => Ok(!spec.aplicacao.chart_ref.is_empty()),
        "ttl" => Ok(!spec.ttl.is_empty()),
        "teardown" => Ok(true),
        "postconditions" => Ok(!spec.postconditions.is_empty()),
        "preconditions" => Ok(!spec.preconditions.is_empty()),
        "closed-loop-auth" => Ok(spec.postconditions.has_kind(ConditionKind::ClosedLoopAuth)),
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
    use tatara_process::classification::{
        CalmClassification, ConvergencePointType, DataClassification, Horizon, HorizonKind,
        OptimizationDirection, SubstrateType,
    };
    use tatara_process::compliance::{ComplianceBinding, VerificationPhase};
    use tatara_process::crd::ProcessSpec;
    use tatara_process::encapsulates::{
        BareWorkload, EncapsulatesSpec, EncapsulationKind, EncapsulationMode,
    };
    use tatara_process::ephemeral::EphemeralSpec;
    use tatara_process::export::{
        ArtifactKind, ArtifactSource, ChannelKind, ExportSpec, ExportTrigger, HttpEventChannel,
        NatsSubjectChannel, ProcessSnapshotSource, ReceiptsSource, ReportFormat, RunMarkerSource,
        StdoutChannel, TestReportSource, VectorChannel,
    };
    use tatara_process::intent::{AplicacaoIntent, IntentKind};
    use tatara_process::lifetime::{EphemeralLifetime, Lifetime, LifetimeKind, TeardownPolicy};
    use tatara_process::routing::{RoutingBackend, RoutingForm, RoutingHostname, RoutingSpec};
    use tatara_process::signal::SighupStrategy;
    use tatara_process::spec::MustReachPhase;

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

    // ── condition-<kind> prefix family pins ──────────────────────────
    //
    // Fail-before-pass-after granularity: the `condition-<kind>` prefix
    // family did not exist before this commit — the require-tag
    // vocabulary carried only `boundary-pre` / `boundary-post` fixed
    // tags that answered "does the boundary carry ANY condition on this
    // side", never "does the boundary carry THIS SPECIFIC condition
    // kind anywhere". The lift adds the third closed-set-driven prefix
    // family symmetrical with `intent-<kind>` + `lifetime-<kind>`,
    // routing through the newly-opened
    // [`tatara_process::boundary::Boundary::has_condition_kind`]
    // substrate primitive via `strip_and_classify_prefixed_kind`.

    fn condition_with(kind: ConditionKind) -> Condition {
        Condition {
            kind,
            params: serde_json::json!({}),
        }
    }

    /// POPULATED-slot pin — `condition-<kind>` dispatches through the
    /// autoderived [`ConditionKind`] `FromStr` + the substrate
    /// [`Boundary::has_condition_kind`] primitive, returning `true`
    /// only when the boundary carries at least one Condition with
    /// this kind. Sweep the [`ConditionKind::ALL`] × ALL cross so a
    /// regression that hard-coded the arm to a single kind (silently
    /// returning `true` for every populated boundary regardless of
    /// which kind was queried) or wired the closure to a fixed
    /// unrelated field fails HERE at the classifier before landing at
    /// the operator-facing checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_populated_condition_slot_per_kind() {
        for populated in ConditionKind::ALL {
            let mut spec = ProcessSpec::gate_compute_defaults();
            spec.boundary.postconditions.push(condition_with(populated));
            for query in ConditionKind::ALL {
                let tag = format!("condition-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "condition populated={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// EMPTY-BOUNDARY pin — a default [`ProcessSpec`] (empty
    /// preconditions, empty postconditions) returns `false` for every
    /// `condition-<kind>` tag. Locks the write-side / read-side split
    /// on the presence-probe boundary so an operator authoring
    /// `:requires (condition-JobAttested)` against a Process whose
    /// boundary lists no such predicate gets the
    /// `definition missing required` diagnostic, not a false-positive
    /// pass.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_empty_boundary_for_every_condition_kind() {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in ConditionKind::ALL {
            let tag = format!("condition-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "default boundary must return false for {tag:?}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `condition-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. A regression that fell through to `Ok(false)`
    /// (matching the pre-lift fixed-tag `_ => Err(UnknownRequireTag)`
    /// tail) would silently reclassify a `condition-jobAttested`
    /// casing typo (PascalCase-only closed set) as
    /// `definition missing required`, which reads as "the spec is
    /// wrong" rather than "your check is wrong". Pin the distinction.
    /// The empty-suffix boundary is pinned by the shared substrate
    /// primitive's [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_condition_suffix() {
        let spec = ProcessSpec::gate_compute_defaults();
        for garbage in [
            "condition-",
            "condition-jobAttested",
            "condition-CLOSEDLOOPAUTH",
            "condition-typo",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// UNION pin — `condition-<kind>` unions preconditions ∪
    /// postconditions so a kind that appears on preconditions ONLY
    /// resolves through the same tag as one on postconditions.
    /// A regression that dropped the pre-condition arm of the OR
    /// (probing only postconditions) silently reclassifies every
    /// pre-only boundary predicate as absent — the pre-lift ephemeral
    /// arm's post-only shape must NOT be re-inherited at the point-
    /// domain surface. Pin the semantic split so a future callsite
    /// that adds a `precondition-<kind>` / `postcondition-<kind>`
    /// finer-grained tag family lands additively without ambiguity
    /// on the coarse-grained `condition-<kind>` union answer.
    #[test]
    fn evaluate_point_require_tag_unions_pre_and_post_conditions_for_condition_prefix() {
        let mut spec = ProcessSpec::gate_compute_defaults();
        spec.boundary
            .preconditions
            .push(condition_with(ConditionKind::KustomizationHealthy));
        spec.boundary
            .postconditions
            .push(condition_with(ConditionKind::ClosedLoopAuth));
        assert_eq!(
            evaluate_point_require_tag(&spec, "condition-KustomizationHealthy"),
            Ok(true),
            "pre-only kind must resolve through the union",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "condition-ClosedLoopAuth"),
            Ok(true),
            "post-only kind must resolve through the union",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "condition-PromQL"),
            Ok(false),
            "an absent kind must return false even with populated halves",
        );
    }

    // ── must-reach-<kind> prefix family pins ─────────────────────────
    //
    // Fail-before-pass-after granularity: the `must-reach-<kind>` prefix
    // family did not exist before this commit — the require-tag
    // vocabulary carried only `depends-on` fixed tag that answered
    // "does the spec carry ANY dependency", never "does the spec carry
    // a dependency gating on THIS SPECIFIC checkpoint". The lift adds
    // the fourth closed-set-driven prefix family symmetrical with
    // `intent-<kind>` / `lifetime-<kind>` / `condition-<kind>`, routing
    // through the newly-opened
    // [`tatara_process::spec::DependsOnSliceExt::has_must_reach`]
    // substrate primitive via `strip_and_classify_prefixed_kind`.

    fn dep_with(must_reach: MustReachPhase) -> tatara_process::spec::DependsOn {
        tatara_process::spec::DependsOn {
            name: "target".to_string(),
            namespace: None,
            must_reach,
        }
    }

    /// POPULATED-slot pin — `must-reach-<kind>` dispatches through the
    /// autoderived [`MustReachPhase`] `FromStr` + the substrate
    /// [`DependsOnSliceExt::has_must_reach`] primitive, returning
    /// `true` only when the depends-on vector carries at least one
    /// dependency gating on this checkpoint. Sweep the
    /// [`MustReachPhase::ALL`] × ALL cross so a regression that
    /// hard-coded the arm to a single kind (silently returning `true`
    /// for every populated depends-on regardless of query kind), or
    /// wired the closure to a fixed unrelated field (e.g.
    /// [`DependsOn::name`]) fails HERE at the classifier before landing
    /// at the operator-facing checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_populated_must_reach_slot_per_kind() {
        for populated in MustReachPhase::ALL {
            let mut spec = ProcessSpec::gate_compute_defaults();
            spec.depends_on.push(dep_with(populated));
            for query in MustReachPhase::ALL {
                let tag = format!("must-reach-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "depends-on populated={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// EMPTY-slot pin — a default [`ProcessSpec`] (empty depends-on
    /// vector) returns `false` for every `must-reach-<kind>` tag.
    /// Locks the write-side / read-side split on the presence-probe
    /// boundary so an operator authoring `:requires (must-reach-Attested)`
    /// against a Process with no `:depends-on` slot gets the
    /// `definition missing required` diagnostic, not a false-positive
    /// pass.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_empty_depends_on_for_every_must_reach_kind() {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in MustReachPhase::ALL {
            let tag = format!("must-reach-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "default depends-on (empty vector) must return false for {tag:?}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `must-reach-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. A regression that fell through to `Ok(false)`
    /// (matching the pre-lift fixed-tag `_ => Err(UnknownRequireTag)`
    /// tail) would silently reclassify a `must-reach-attested` casing
    /// typo (PascalCase-only closed set) as `definition missing
    /// required`, which reads as "the spec is wrong" rather than "your
    /// check is wrong". Pin the distinction. `Pending` / `Failed` /
    /// `Reaped` are legal [`ProcessPhase`]s but NOT valid
    /// [`MustReachPhase`] checkpoints — the closed subset's own
    /// FROM-STR rejection is inherited here at the prefix-family
    /// boundary.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_must_reach_suffix() {
        let spec = ProcessSpec::gate_compute_defaults();
        for garbage in [
            "must-reach-",
            "must-reach-running",
            "must-reach-ATTESTED",
            "must-reach-Pending",
            "must-reach-Failed",
            "must-reach-Reaped",
            "must-reach-typo",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// COARSE / FINE COEXISTENCE pin — a Process with a single
    /// `depends_on` entry gating on `Attested` MUST satisfy BOTH the
    /// coarse `depends-on` fixed tag AND the fine
    /// `must-reach-Attested` prefix tag AND simultaneously fail the
    /// off-diagonal `must-reach-Running` probe. Locks the semantic
    /// split between the two surfaces so a regression that (a)
    /// collapsed `must-reach-<kind>` to the coarse "any dependency"
    /// answer, or (b) drifted the fixed `depends-on` arm to match on
    /// checkpoint kind fails HERE at ONE narrow site.
    #[test]
    fn evaluate_point_require_tag_must_reach_and_depends_on_coexist() {
        let mut spec = ProcessSpec::gate_compute_defaults();
        spec.depends_on.push(dep_with(MustReachPhase::Attested));
        assert_eq!(
            evaluate_point_require_tag(&spec, "depends-on"),
            Ok(true),
            "coarse `depends-on` must be true when any dependency is present",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "must-reach-Attested"),
            Ok(true),
            "fine `must-reach-Attested` must be true when a matching dep is present",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "must-reach-Running"),
            Ok(false),
            "fine `must-reach-Running` must be false when no dep gates on Running",
        );
    }

    // ── sighup-<kind> prefix family pins ─────────────────────────────
    //
    // Fail-before-pass-after granularity: the `sighup-<kind>` prefix
    // family did not exist before this commit — the point-domain
    // require-tag vocabulary carried only the coarse `signals` fixed
    // tag which answered "does the signal policy carry a positive
    // sigterm-grace-seconds" without touching the SIGHUP side of the
    // policy. The lift adds the fifth closed-set-driven prefix family
    // symmetrical with `intent-<kind>` + `lifetime-<kind>` +
    // `condition-<kind>` + `must-reach-<kind>`, routing through the
    // newly-opened
    // [`tatara_process::spec::SignalPolicy::has_sighup_strategy`]
    // substrate primitive via `strip_and_classify_prefixed_kind`.
    // First instance on the SCALAR-CARRIER axis of the closed-set-
    // driven presence-probe algebra (peer of the Option-slot Intent::has
    // / Lifetime::has and the slice-level ConditionSliceExt::has_kind /
    // DependsOnSliceExt::has_must_reach primitives).

    /// VARIANT-MATCH pin — `sighup-<kind>` dispatches through the
    /// autoderived [`SighupStrategy`] `FromStr` + the substrate
    /// [`SignalPolicy::has_sighup_strategy`] primitive, returning
    /// `true` only when the policy's `sighup_strategy` field equals
    /// this variant. Sweep the [`SighupStrategy::ALL`] × ALL cross so
    /// a regression that hard-coded the arm to a single kind
    /// (silently returning `true` for every populated policy
    /// regardless of query kind) or wired the closure to a fixed
    /// unrelated field (e.g. `signals.start_suspended`) fails HERE at
    /// the classifier before landing at the operator-facing
    /// checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_iff_sighup_strategy_matches_variant_per_kind() {
        for populated in SighupStrategy::ALL {
            let mut spec = ProcessSpec::gate_compute_defaults();
            spec.signals.sighup_strategy = populated;
            for query in SighupStrategy::ALL {
                let tag = format!("sighup-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "sighup_strategy={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// DEFAULT / SCALAR-vs-OPTION pin — a default [`ProcessSpec`]
    /// carries `signals.sighup_strategy: SighupStrategy::default() =
    /// Reconverge`, so `sighup-Reconverge` classifies `Ok(true)` and
    /// the other two variants classify `Ok(false)`. Locks the scalar-
    /// carrier axis's semantic split against the Option-slot axis at
    /// the classifier boundary: pre-lift the reader might assume
    /// "default spec must return false for every closed-set prefix
    /// query" (which holds for `intent-<kind>` / `lifetime-<kind>` /
    /// `condition-<kind>` / `must-reach-<kind>`, each of which
    /// probes an Option-slot or a Vec that is empty on a default
    /// spec). `sighup-<kind>` returns `true` on the
    /// [`SighupStrategy::default`] variant precisely because the
    /// field is non-Option — the operator hasn't "left the slot
    /// empty," they've picked (perhaps by omission) the substrate
    /// default. Pin the distinction so a regression that special-
    /// cased the default policy to return `false` for every kind (to
    /// preserve the pre-lift "default returns false" symmetry) fails
    /// HERE.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_default_signals_for_sighup_reconverge_only() {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in SighupStrategy::ALL {
            let tag = format!("sighup-{}", kind.as_str());
            let expected = kind == SighupStrategy::Reconverge;
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(expected),
                "default signals (sighup_strategy=Reconverge): tag {tag:?} \
                 must return {expected}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `sighup-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. A regression that fell through to `Ok(false)`
    /// (matching the pre-lift fixed-tag `_ => Err(UnknownRequireTag)`
    /// tail) would silently reclassify a `sighup-reconverge` casing
    /// typo (PascalCase-only closed set) as `definition missing
    /// required`, which reads as "the spec is wrong" rather than
    /// "your check is wrong". Pin the distinction. The empty-suffix
    /// boundary is pinned by the shared substrate primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_sighup_suffix() {
        let spec = ProcessSpec::gate_compute_defaults();
        for garbage in [
            "sighup-",
            "sighup-reconverge",
            "sighup-RECONVERGE",
            "sighup-Suspend",
            "sighup-typo",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// COARSE / FINE COEXISTENCE pin — a default [`ProcessSpec`]
    /// (positive `sigterm_grace_seconds`, `sighup_strategy: Reconverge`)
    /// MUST satisfy BOTH the coarse `signals` fixed tag AND the fine
    /// `sighup-Reconverge` prefix tag AND simultaneously fail the
    /// off-diagonal `sighup-Restart` probe. Locks the semantic split
    /// between the two surfaces so a regression that (a) collapsed
    /// `sighup-<kind>` to the coarse `signals` fixed answer
    /// (returning `true` for every kind on any policy that satisfies
    /// `signals`), or (b) drifted the fixed `signals` arm to match on
    /// SIGHUP strategy kind, fails HERE at ONE narrow site.
    #[test]
    fn evaluate_point_require_tag_sighup_and_signals_coexist() {
        let spec = ProcessSpec::gate_compute_defaults();
        assert_eq!(
            evaluate_point_require_tag(&spec, "signals"),
            Ok(true),
            "coarse `signals` must be true on default policy (positive grace)",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "sighup-Reconverge"),
            Ok(true),
            "fine `sighup-Reconverge` must be true on default sighup strategy",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "sighup-Restart"),
            Ok(false),
            "fine `sighup-Restart` must be false when strategy is Reconverge",
        );
    }

    // ── verification-phase-<kind> prefix family pins ─────────────────
    //
    // Fail-before-pass-after granularity: the `verification-phase-<kind>`
    // prefix family did not exist before this commit — the point-domain
    // require-tag vocabulary carried only the coarse `compliance` fixed
    // tag which answered "does the spec carry ANY compliance binding"
    // without discriminating on the verification checkpoint. The lift
    // adds the SIXTH closed-set-driven prefix family symmetrical with
    // `intent-<kind>` + `lifetime-<kind>` + `condition-<kind>` +
    // `must-reach-<kind>` + `sighup-<kind>`, routing through the
    // newly-opened
    // [`tatara_process::compliance::ComplianceBindingSliceExt::has_verification_phase`]
    // substrate primitive via `strip_and_classify_prefixed_kind`. Third
    // instance in the SLICE-LEVEL axis of the closed-set-driven
    // presence-probe algebra (peer of `ConditionSliceExt::has_kind` on
    // `&[Condition]` and `DependsOnSliceExt::has_must_reach` on
    // `&[DependsOn]`).

    fn binding_at(phase: VerificationPhase) -> ComplianceBinding {
        ComplianceBinding {
            framework: "nist-800-53".into(),
            control_id: "SC-7".into(),
            phase,
            description: None,
        }
    }

    /// POPULATED-slot pin — `verification-phase-<kind>` dispatches
    /// through the autoderived [`VerificationPhase`] `FromStr` + the
    /// substrate
    /// [`tatara_process::compliance::ComplianceBindingSliceExt::has_verification_phase`]
    /// primitive, returning `true` only when the compliance bindings
    /// vector carries at least one binding verifying at this
    /// checkpoint. Sweep the [`VerificationPhase::ALL`] × ALL cross so
    /// a regression that hard-coded the arm to a single kind (silently
    /// returning `true` for every populated compliance vector
    /// regardless of query kind) or wired the closure to a fixed
    /// unrelated field (e.g. [`ComplianceBinding::framework`]) fails
    /// HERE at the classifier before landing at the operator-facing
    /// checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_populated_verification_phase_slot_per_kind() {
        for populated in VerificationPhase::ALL {
            let mut spec = ProcessSpec::gate_compute_defaults();
            spec.compliance.bindings.push(binding_at(populated));
            for query in VerificationPhase::ALL {
                let tag = format!("verification-phase-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "compliance populated={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// EMPTY-slot pin — a default [`ProcessSpec`] (empty
    /// `compliance.bindings` vector) returns `false` for every
    /// `verification-phase-<kind>` tag. Locks the write-side /
    /// read-side split on the presence-probe boundary so an operator
    /// authoring `:requires (verification-phase-AtBoundary)` against a
    /// Process with no `:compliance` bindings gets the
    /// `definition missing required` diagnostic, not a false-positive
    /// pass. Distinct from `sighup-<kind>` (which returns `true` on
    /// the [`SighupStrategy::default`] variant because its carrier is
    /// scalar-not-Vec); this family reads a Vec so a default spec is
    /// unambiguously empty across every kind.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_empty_compliance_for_every_verification_phase_kind(
    ) {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in VerificationPhase::ALL {
            let tag = format!("verification-phase-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "default compliance (empty vector) must return false for {tag:?}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `verification-phase-<garbage>` classifies
    /// as [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. A regression that fell through to `Ok(false)`
    /// (matching the pre-lift fixed-tag `_ => Err(UnknownRequireTag)`
    /// tail) would silently reclassify a `verification-phase-planTime`
    /// casing typo (PascalCase-only closed set) as `definition
    /// missing required`, which reads as "the spec is wrong" rather
    /// than "your check is wrong". Pin the distinction. The empty-
    /// suffix boundary is pinned by the shared substrate primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_verification_phase_suffix() {
        let spec = ProcessSpec::gate_compute_defaults();
        for garbage in [
            "verification-phase-",
            "verification-phase-plantime",
            "verification-phase-ATBOUNDARY",
            "verification-phase-Continuous",
            "verification-phase-typo",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// COARSE / FINE COEXISTENCE pin — a Process with a single
    /// `ComplianceBinding` at `PlanTime` MUST satisfy BOTH the coarse
    /// `compliance` fixed tag AND the fine
    /// `verification-phase-PlanTime` prefix tag AND simultaneously
    /// fail the off-diagonal `verification-phase-AtBoundary` /
    /// `verification-phase-PostConvergence` probes. Locks the
    /// semantic split between the two surfaces so a regression that
    /// (a) collapsed `verification-phase-<kind>` to the coarse
    /// `compliance` fixed answer (returning `true` for every kind on
    /// any spec that satisfies `compliance`), or (b) drifted the
    /// fixed `compliance` arm to match on verification-phase kind,
    /// fails HERE at ONE narrow site.
    #[test]
    fn evaluate_point_require_tag_verification_phase_and_compliance_coexist() {
        let mut spec = ProcessSpec::gate_compute_defaults();
        spec.compliance
            .bindings
            .push(binding_at(VerificationPhase::PlanTime));
        assert_eq!(
            evaluate_point_require_tag(&spec, "compliance"),
            Ok(true),
            "coarse `compliance` must be true when any binding is present",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "verification-phase-PlanTime"),
            Ok(true),
            "fine `verification-phase-PlanTime` must be true when a matching binding is present",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "verification-phase-AtBoundary"),
            Ok(false),
            "fine `verification-phase-AtBoundary` must be false when no binding gates at that phase",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "verification-phase-PostConvergence"),
            Ok(false),
            "fine `verification-phase-PostConvergence` must be false when no binding gates there",
        );
    }

    // ── export-when-<kind> prefix family (evaluate_point_require_tag) ─
    //
    // Fail-before-pass-after granularity: the `export-when-<kind>`
    // prefix family did not exist before this commit — the point-domain
    // require-tag vocabulary carried only the coarse
    // `lifetime-ephemeral` presence probe which answered "is this
    // Process ephemeral at all" without discriminating on the declared
    // export trigger. The lift adds the SEVENTH closed-set-driven
    // prefix family symmetrical with `intent-<kind>` +
    // `lifetime-<kind>` + `condition-<kind>` + `must-reach-<kind>` +
    // `sighup-<kind>` + `verification-phase-<kind>`, routing through
    // the newly-opened
    // [`tatara_process::export::ExportSpecSliceExt::has_when`]
    // substrate primitive via `strip_and_classify_prefixed_kind`.
    // Fourth instance in the SLICE-LEVEL axis of the closed-set-driven
    // presence-probe algebra (peer of `ConditionSliceExt::has_kind` on
    // `&[Condition]`, `DependsOnSliceExt::has_must_reach` on
    // `&[DependsOn]`, and
    // `ComplianceBindingSliceExt::has_verification_phase` on
    // `&[ComplianceBinding]`).

    /// Fixture: a minimal `ExportSpec` with a chosen `when` trigger
    /// and a single-slot receipts source + stdout channel. The trigger
    /// is the only axis this test module discriminates on; the source
    /// + channel are fixed at valid single-slot pairs so
    /// `evaluate_point_require_tag` reads the `when` field in
    /// isolation.
    fn export_at(when: ExportTrigger) -> ExportSpec {
        ExportSpec {
            source: ArtifactSource {
                receipts: Some(ReceiptsSource::default()),
                ..ArtifactSource::default()
            },
            channel: VectorChannel {
                stdout: Some(StdoutChannel::default()),
                ..VectorChannel::default()
            },
            when,
            experiment_id_override: None,
        }
    }

    /// Compose a `ProcessSpec` whose ephemeral lifetime carries the
    /// supplied export triggers. Routes through
    /// [`Lifetime::ephemeral`] so the composer contract stays
    /// byte-identical to every other ephemeral-fixture consumer.
    fn ephemeral_spec_with_exports(exports: Vec<ExportSpec>) -> ProcessSpec {
        ProcessSpec {
            lifetime: Lifetime::ephemeral(EphemeralLifetime {
                exports,
                ..EphemeralLifetime::default()
            }),
            ..ProcessSpec::gate_compute_defaults()
        }
    }

    /// POPULATED-slot pin — `export-when-<kind>` dispatches through
    /// the autoderived [`ExportTrigger`] `FromStr` + the substrate
    /// [`tatara_process::export::ExportSpecSliceExt::has_when`]
    /// primitive, returning `true` only when the resolved ephemeral
    /// lifetime carries at least one export whose `when` matches this
    /// trigger. Sweep the [`ExportTrigger::ALL`] × ALL cross so a
    /// regression that hard-coded the arm to a single kind (silently
    /// returning `true` for every populated export vector regardless
    /// of query kind) or wired the closure to a fixed unrelated field
    /// (a stray `experiment_id_override.is_some()`) fails HERE at the
    /// classifier before landing at the operator-facing checks.lisp
    /// surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_populated_export_when_slot_per_kind() {
        for populated in ExportTrigger::ALL {
            let spec = ephemeral_spec_with_exports(vec![export_at(populated)]);
            for query in ExportTrigger::ALL {
                let tag = format!("export-when-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "export populated={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// PERMANENT-lifetime pin — a default (`Permanent`) [`ProcessSpec`]
    /// returns `false` for every `export-when-<kind>` tag because the
    /// `resolved_ephemeral` gate on the parent [`Lifetime`] short-
    /// circuits the walk. Locks the compound-projection contract
    /// (`resolved_ephemeral().is_some_and(|e| e.exports.has_when(k))`)
    /// so a regression that dropped the ephemeral gate (probing an
    /// absent `exports` slot as if it were the empty vector) would
    /// still return `false` at this test but a regression that ROUTED
    /// through the permanent-side default `EphemeralLifetime` (a
    /// hypothetical `unwrap_or_default` on the projection) would
    /// return `false` on `OnAttested`/`OnFailed` but `true` on
    /// `Always` (which `fires_on` would honor on a synthetic terminal
    /// phase) at a peer test — the pair pins the semantics from both
    /// sides.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_permanent_lifetime_for_every_export_when_kind() {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in ExportTrigger::ALL {
            let tag = format!("export-when-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "permanent lifetime must return false for {tag:?}",
            );
        }
    }

    /// EMPTY-EXPORTS pin — an ephemeral [`ProcessSpec`] whose
    /// `exports` vector is empty returns `false` for every
    /// `export-when-<kind>` tag. Distinct from the permanent-lifetime
    /// case above: the resolved-ephemeral gate DOES fire (the parent
    /// projection returns `Some(&EphemeralLifetime)`), the walk over
    /// the empty vector then returns `false` for every kind. Locks
    /// the "reachable-but-empty" corner so a regression that
    /// short-circuited on `resolved_ephemeral().is_some()` alone
    /// (ignoring the exports contents) would return `true` here for
    /// every kind and fail — the two corners (unreachable-parent vs
    /// reachable-empty-child) both compose to `false` but through
    /// different arms of the closed-set-driven projection.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_empty_exports_for_every_export_when_kind() {
        let spec = ephemeral_spec_with_exports(vec![]);
        for kind in ExportTrigger::ALL {
            let tag = format!("export-when-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "resolved-ephemeral with empty exports must return false for {tag:?}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `export-when-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. A regression that fell through to `Ok(false)`
    /// (matching the pre-lift fixed-tag `_ => Err(UnknownRequireTag)`
    /// tail) would silently reclassify a `export-when-onattested`
    /// casing typo (PascalCase-only closed set) as `definition
    /// missing required`, which reads as "the spec is wrong" rather
    /// than "your check is wrong". Pin the distinction. The empty-
    /// suffix boundary is pinned by the shared substrate primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_export_when_suffix() {
        let spec = ephemeral_spec_with_exports(vec![]);
        for garbage in [
            "export-when-",
            "export-when-onattested",
            "export-when-ALWAYS",
            "export-when-OnSuccess",
            "export-when-typo",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// COARSE / FINE COEXISTENCE pin — a Process with a single
    /// ephemeral export at `OnAttested` MUST satisfy BOTH the coarse
    /// `lifetime-ephemeral` presence probe AND the fine
    /// `export-when-OnAttested` prefix tag AND simultaneously fail
    /// the off-diagonal `export-when-OnFailed` / `export-when-Always`
    /// probes. Locks the semantic split between the two surfaces so
    /// a regression that (a) collapsed `export-when-<kind>` to the
    /// coarse `lifetime-ephemeral` fixed answer (returning `true` for
    /// every kind on any ephemeral Process), or (b) drifted the
    /// `lifetime-ephemeral` arm to match on export-trigger kind,
    /// fails HERE at ONE narrow site.
    #[test]
    fn evaluate_point_require_tag_export_when_and_lifetime_ephemeral_coexist() {
        let spec = ephemeral_spec_with_exports(vec![export_at(ExportTrigger::OnAttested)]);
        assert_eq!(
            evaluate_point_require_tag(&spec, "lifetime-ephemeral"),
            Ok(true),
            "coarse `lifetime-ephemeral` must be true on an ephemeral Process",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "export-when-OnAttested"),
            Ok(true),
            "fine `export-when-OnAttested` must be true when a matching export is present",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "export-when-OnFailed"),
            Ok(false),
            "fine `export-when-OnFailed` must be false when no export declares that trigger",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "export-when-Always"),
            Ok(false),
            "fine `export-when-Always` must be false when no export declares that trigger",
        );
    }

    // ── channel-<kind> prefix family (evaluate_point_require_tag) ────
    //
    // Fail-before-pass-after granularity: the `channel-<kind>` prefix
    // family did not exist before this commit — the point-domain
    // require-tag vocabulary carried the seven prior closed-set-driven
    // families (`intent-<kind>`, `lifetime-<kind>`, `condition-<kind>`,
    // `must-reach-<kind>`, `sighup-<kind>`, `verification-phase-<kind>`,
    // `export-when-<kind>`) but had no way to distinguish which VECTOR
    // CHANNEL an ephemeral export ships through — a JetStream-backed
    // guarantee vs a bare-stdout log lift. The lift adds the EIGHTH
    // closed-set-driven prefix family symmetrical with the seven prior
    // ones, routing through the newly-opened
    // [`tatara_process::export::ExportSpecSliceExt::has_channel_kind`]
    // substrate primitive via `strip_and_classify_prefixed_kind`. Fifth
    // instance in the workspace slice-level closed-set-driven presence-
    // probe algebra (peer of `has_when` on the SAME slice — the first
    // slice whose extension trait carries two probes on distinct axes).
    // Same `resolved_ephemeral` parent gate as `export-when-<kind>`:
    // the two prefix families share the exact same projection so the
    // permanent-lifetime + empty-exports corners answer `false` for
    // every channel kind through the same short-circuit path
    // `export-when-<kind>` already publishes.

    /// Fixture: a minimal `ExportSpec` with a chosen [`ChannelKind`]
    /// populated on its `channel` slot and a fixed single-slot
    /// receipts source + default `when` trigger. The channel kind is
    /// the only axis this test module discriminates on; the source +
    /// trigger are fixed at valid pairs so the classifier reads the
    /// `channel` slot in isolation. Sweeps [`ChannelKind::ALL`] via
    /// exhaustive `match` on the closed set so a future fourth variant
    /// reaches this fixture at rustc's exhaustiveness gate.
    fn export_with_channel(kind: ChannelKind) -> ExportSpec {
        let channel = match kind {
            ChannelKind::HttpEvent => VectorChannel {
                http_event: Some(HttpEventChannel::signal("test-report")),
                ..VectorChannel::default()
            },
            ChannelKind::NatsSubject => VectorChannel {
                nats_subject: Some(NatsSubjectChannel::publish("s", "STREAM")),
                ..VectorChannel::default()
            },
            ChannelKind::Stdout => VectorChannel {
                stdout: Some(StdoutChannel::default()),
                ..VectorChannel::default()
            },
        };
        ExportSpec {
            source: ArtifactSource {
                receipts: Some(ReceiptsSource::default()),
                ..ArtifactSource::default()
            },
            channel,
            when: ExportTrigger::default(),
            experiment_id_override: None,
        }
    }

    /// POPULATED-slot pin — `channel-<kind>` dispatches through the
    /// autoderived [`ChannelKind`] `FromStr` + the substrate
    /// [`tatara_process::export::ExportSpecSliceExt::has_channel_kind`]
    /// primitive, returning `true` only when the resolved ephemeral
    /// lifetime carries at least one export whose `channel` slot for
    /// this kind is populated. Sweep the [`ChannelKind::ALL`] × ALL
    /// cross so a regression that hard-coded the arm to a single kind
    /// (silently returning `true` for every populated export vector
    /// regardless of query kind) or wired the closure to a fixed
    /// unrelated field (a stray `experiment_id_override.is_some()`, a
    /// probe on `when` or `source`) fails HERE at the classifier
    /// before landing at the operator-facing checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_populated_channel_slot_per_kind() {
        for populated in ChannelKind::ALL {
            let spec = ephemeral_spec_with_exports(vec![export_with_channel(populated)]);
            for query in ChannelKind::ALL {
                let tag = format!("channel-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "channel populated={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// PERMANENT-lifetime pin — a default (`Permanent`) [`ProcessSpec`]
    /// returns `false` for every `channel-<kind>` tag because the
    /// `resolved_ephemeral` gate on the parent [`Lifetime`] short-
    /// circuits the walk. Byte-for-byte symmetric with the
    /// `evaluate_point_require_tag_returns_false_on_permanent_lifetime_for_every_export_when_kind`
    /// pin on the seventh family — the two prefix families share the
    /// SAME parent gate, so a regression that dropped or narrowed the
    /// gate on `channel-<kind>` (probing an absent `exports` slot as
    /// if it were the empty vector, or worse routing through the
    /// permanent-side default `EphemeralLifetime`) fails HERE for every
    /// channel kind and the peer test still passes — the pair pins the
    /// contract from both sides of the closed-set axis.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_permanent_lifetime_for_every_channel_kind() {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in ChannelKind::ALL {
            let tag = format!("channel-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "permanent lifetime must return false for {tag:?}",
            );
        }
    }

    /// EMPTY-EXPORTS pin — an ephemeral [`ProcessSpec`] whose
    /// `exports` vector is empty returns `false` for every
    /// `channel-<kind>` tag. Distinct from the permanent-lifetime case
    /// above: the resolved-ephemeral gate DOES fire, the walk over the
    /// empty vector then returns `false` for every kind. Locks the
    /// "reachable-but-empty" corner so a regression that short-
    /// circuited on `resolved_ephemeral().is_some()` alone (ignoring
    /// the exports contents) would return `true` here for every kind
    /// and fail — the two corners (unreachable-parent vs reachable-
    /// empty-child) both compose to `false` but through different arms
    /// of the closed-set-driven projection.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_empty_exports_for_every_channel_kind() {
        let spec = ephemeral_spec_with_exports(vec![]);
        for kind in ChannelKind::ALL {
            let tag = format!("channel-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "resolved-ephemeral with empty exports must return false for {tag:?}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `channel-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. The canonical [`ChannelKind`] labels are camelCase
    /// (`httpEvent`, `natsSubject`, `stdout`) — matching the serde
    /// `rename_all = "camelCase"` field name on `VectorChannel` — so
    /// PascalCase spellings (`HttpEvent`, `NatsSubject`) are UNKNOWN
    /// suffixes, a distinct contract from the PascalCase closed sets
    /// [`ExportTrigger`] / [`ConditionKind`] / [`IntentKind`] carry.
    /// Pin the case-sensitivity axis so a regression that ASCIIfolded
    /// or PascalCased on parse (a hypothetical `to_lower_camel`
    /// normalization at the tag layer) would fail HERE.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_channel_suffix() {
        let spec = ephemeral_spec_with_exports(vec![]);
        for garbage in [
            "channel-",
            "channel-HttpEvent",
            "channel-NatsSubject",
            "channel-STDOUT",
            "channel-typo",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// SAME-EXPORT AXIS COEXISTENCE pin — a Process with a SINGLE
    /// ephemeral export whose `when` is `OnFailed` and whose `channel`
    /// is `NatsSubject` MUST satisfy BOTH the fine
    /// `export-when-OnFailed` tag (from the seventh prefix family) AND
    /// the fine `channel-natsSubject` tag (from the eighth) AND
    /// simultaneously fail the off-diagonal `export-when-OnAttested` /
    /// `channel-stdout` probes. Locks the semantic split between the
    /// two probes on the SAME `&[ExportSpec]` slice so a regression
    /// that (a) collapsed `channel-<kind>` to the seventh family
    /// (matching on `when` instead of `channel`), or (b) collapsed
    /// `export-when-<kind>` to the eighth (matching on `channel`
    /// instead of `when`), fails HERE at ONE narrow site. The audit
    /// `every OnFailed export ships through JetStream` reads as this
    /// exact conjunction at the checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_channel_and_export_when_coexist() {
        let mut export = export_with_channel(ChannelKind::NatsSubject);
        export.when = ExportTrigger::OnFailed;
        let spec = ephemeral_spec_with_exports(vec![export]);
        assert_eq!(
            evaluate_point_require_tag(&spec, "export-when-OnFailed"),
            Ok(true),
            "fine `export-when-OnFailed` must be true when the sole export declares that trigger",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "channel-natsSubject"),
            Ok(true),
            "fine `channel-natsSubject` must be true when the sole export ships through NATS",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "export-when-OnAttested"),
            Ok(false),
            "off-diagonal `export-when-OnAttested` must be false: the export declares OnFailed",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "channel-stdout"),
            Ok(false),
            "off-diagonal `channel-stdout` must be false: the export ships through NATS",
        );
    }

    // ── report-format-<kind> prefix family (evaluate_point_require_tag)
    //
    // Fail-before-pass-after granularity: the `report-format-<kind>`
    // prefix family did not exist before this commit — the point-domain
    // require-tag vocabulary carried the eight prior closed-set-driven
    // families (`intent-<kind>`, `lifetime-<kind>`, `condition-<kind>`,
    // `must-reach-<kind>`, `sighup-<kind>`, `verification-phase-<kind>`,
    // `export-when-<kind>`, `channel-<kind>`) but had no way to
    // distinguish which PAYLOAD FORMAT a test-report export ships out
    // — a JUnit XML vs a TAP v13 stream vs a raw ndjson dump. The lift
    // adds the NINTH closed-set-driven prefix family symmetrical with
    // the eight prior ones, routing through the newly-opened
    // [`tatara_process::export::ExportSpecSliceExt::has_report_format`]
    // substrate primitive via `strip_and_classify_prefixed_kind`. SIXTH
    // instance in the workspace slice-level closed-set-driven presence-
    // probe algebra (third method on `ExportSpecSliceExt` — the first
    // slice-level extension trait carrying probes on THREE distinct
    // axes: the `when` trigger axis, the `channel` tagged-union axis,
    // and the `source.test_report.format` nested-Option scalar axis).
    // Same `resolved_ephemeral` parent gate as `export-when-<kind>` +
    // `channel-<kind>`, so the permanent-lifetime + empty-exports
    // corners answer `false` for every report format through the same
    // short-circuit path the two prior slice-level families publish.
    // Distinct from those families in ONE further dimension: an export
    // whose `source` slot carries a non-`test_report` variant
    // (`receipts`, `process_snapshot`, `run_marker`) contributes
    // `false` for EVERY [`ReportFormat`] kind INCLUDING the default
    // [`ReportFormat::Raw`] — the outer nested-Option projection
    // collapses before the equality on `format` fires. This
    // nested-Option-collapse corner is a distinct arm from the
    // permanent-lifetime and reachable-empty-child corners the seventh
    // + eighth families pin, so the fourth "reachable-but-non-
    // test-report-source" corner is pinned as its own arm below.

    /// Fixture: a minimal `ExportSpec` whose `source` carries a
    /// [`TestReportSource`] tagged with the chosen [`ReportFormat`] and
    /// a fixed single-slot stdout channel + default `when` trigger.
    /// The report format is the only axis this test module
    /// discriminates on; the channel + trigger + configmap/key strings
    /// are fixed at valid pairs so `evaluate_point_require_tag` reads
    /// the `source.test_report.format` slot in isolation. Sweeps
    /// [`ReportFormat::ALL`] so a future fifth variant reaches this
    /// fixture at rustc's exhaustiveness gate on the `ALL` literal.
    fn export_with_report_format(kind: ReportFormat) -> ExportSpec {
        ExportSpec {
            source: ArtifactSource {
                test_report: Some(TestReportSource {
                    configmap: "junit-results".into(),
                    key: "junit.xml".into(),
                    format: kind,
                    namespace: None,
                }),
                ..ArtifactSource::default()
            },
            channel: VectorChannel {
                stdout: Some(StdoutChannel::default()),
                ..VectorChannel::default()
            },
            when: ExportTrigger::default(),
            experiment_id_override: None,
        }
    }

    /// POPULATED-slot pin — `report-format-<kind>` dispatches through
    /// the autoderived [`ReportFormat`] `FromStr` + the substrate
    /// [`tatara_process::export::ExportSpecSliceExt::has_report_format`]
    /// primitive, returning `true` only when the resolved ephemeral
    /// lifetime carries at least one export whose
    /// `source.test_report.format` slot matches this kind. Sweep the
    /// [`ReportFormat::ALL`] × ALL cross so a regression that hard-
    /// coded the arm to a single kind (silently returning `true` for
    /// every populated export vector regardless of query kind), or
    /// wired the closure to a fixed unrelated field (a stray
    /// `experiment_id_override.is_some()`, a probe on `when` /
    /// `channel`), or collapsed the outer nested-Option projection
    /// (probing `test_report.is_some()` alone and treating the empty
    /// case as `ReportFormat::default() == Raw`) fails HERE at the
    /// classifier before landing at the operator-facing checks.lisp
    /// surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_populated_report_format_slot_per_kind() {
        for populated in ReportFormat::ALL {
            let spec = ephemeral_spec_with_exports(vec![export_with_report_format(populated)]);
            for query in ReportFormat::ALL {
                let tag = format!("report-format-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "report format populated={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// PERMANENT-lifetime pin — a default (`Permanent`) [`ProcessSpec`]
    /// returns `false` for every `report-format-<kind>` tag because the
    /// `resolved_ephemeral` gate on the parent [`Lifetime`] short-
    /// circuits the walk. Byte-for-byte symmetric with the peer
    /// permanent-lifetime pins on the seventh (`export-when-<kind>`)
    /// and eighth (`channel-<kind>`) families — all three prefix
    /// families share the SAME parent gate, so a regression that
    /// dropped or narrowed the gate on `report-format-<kind>` (probing
    /// an absent `exports` slot as if it were the empty vector, or
    /// worse routing through the permanent-side default
    /// `EphemeralLifetime`) fails HERE for every report format and the
    /// peer tests still pass — the trio pins the contract from three
    /// sides of the closed-set axis.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_permanent_lifetime_for_every_report_format_kind()
    {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in ReportFormat::ALL {
            let tag = format!("report-format-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "permanent lifetime must return false for {tag:?}",
            );
        }
    }

    /// EMPTY-EXPORTS pin — an ephemeral [`ProcessSpec`] whose
    /// `exports` vector is empty returns `false` for every
    /// `report-format-<kind>` tag. Distinct from the permanent-
    /// lifetime case above: the resolved-ephemeral gate DOES fire, the
    /// walk over the empty vector then returns `false` for every kind.
    /// Locks the "reachable-but-empty" corner so a regression that
    /// short-circuited on `resolved_ephemeral().is_some()` alone
    /// (ignoring the exports contents) would return `true` here for
    /// every kind and fail.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_empty_exports_for_every_report_format_kind() {
        let spec = ephemeral_spec_with_exports(vec![]);
        for kind in ReportFormat::ALL {
            let tag = format!("report-format-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "resolved-ephemeral with empty exports must return false for {tag:?}",
            );
        }
    }

    /// NESTED-OPTION-COLLAPSE pin — an ephemeral [`ProcessSpec`] whose
    /// `exports` vector carries a non-`test_report` export (a
    /// receipts-only source) returns `false` for EVERY
    /// `report-format-<kind>` tag INCLUDING the default
    /// [`ReportFormat::Raw`] that a naive `unwrap_or_default()`
    /// projection would spuriously match. This is the FOURTH corner
    /// of the closed-set walk (peer of the unreachable-parent case,
    /// the reachable-empty-child case, and the populated-slot case):
    /// reachable-populated-child whose `source.test_report` slot is
    /// EMPTY. Locks the outer nested-`Option` short-circuit contract
    /// so a regression that dropped the `.as_ref().is_some_and(…)`
    /// gate on the substrate primitive fails HERE at ONE narrow
    /// classifier site — the peer permanent-lifetime and
    /// empty-exports pins would still pass (they exercise different
    /// arms of the closed-set-driven projection). Sweeps
    /// [`ReportFormat::ALL`] so the collapse contract is pinned
    /// symmetrically across every format the closed set names.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_non_test_report_source_for_every_report_format_kind(
    ) {
        let receipts_only = ExportSpec {
            source: ArtifactSource {
                receipts: Some(ReceiptsSource::default()),
                ..ArtifactSource::default()
            },
            channel: VectorChannel {
                stdout: Some(StdoutChannel::default()),
                ..VectorChannel::default()
            },
            when: ExportTrigger::default(),
            experiment_id_override: None,
        };
        let spec = ephemeral_spec_with_exports(vec![receipts_only]);
        for kind in ReportFormat::ALL {
            let tag = format!("report-format-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "receipts-only export must return false for {tag:?} (including Raw)",
            );
        }
    }

    /// UNKNOWN-suffix pin — `report-format-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. The canonical [`ReportFormat`] labels are
    /// PascalCase (`Junit`, `TapV13`, `NdJson`, `Raw`) — matching the
    /// serde `rename_all = "PascalCase"` output verbatim — so
    /// camelCase / all-lowercase / all-caps spellings are UNKNOWN
    /// suffixes, a distinct contract from the camelCase closed set
    /// [`ChannelKind`] carries. Pin the case-sensitivity axis so a
    /// regression that ASCIIfolded or camel-cased on parse (a
    /// hypothetical `to_lower_camel` normalization at the tag layer)
    /// would fail HERE.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_report_format_suffix() {
        let spec = ephemeral_spec_with_exports(vec![]);
        for garbage in [
            "report-format-",
            "report-format-junit",
            "report-format-tapV13",
            "report-format-NDJSON",
            "report-format-typo",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// SAME-EXPORT AXIS TRIPLE-COEXISTENCE pin — a Process with a
    /// SINGLE ephemeral export whose `when` is `OnAttested`, whose
    /// `channel` is `NatsSubject`, and whose `source.test_report`
    /// slot is populated with `format = Junit` MUST satisfy ALL
    /// THREE fine tags (`export-when-OnAttested` from the seventh
    /// family, `channel-natsSubject` from the eighth, and
    /// `report-format-Junit` from the ninth) AND simultaneously fail
    /// each off-diagonal probe (`export-when-OnFailed`,
    /// `channel-stdout`, `report-format-TapV13`). Locks the semantic
    /// split between the three probes on the SAME `&[ExportSpec]`
    /// slice — the first triple-family conjunction in the point-
    /// domain require-tag vocabulary — so a regression that (a)
    /// collapsed `report-format-<kind>` to the seventh family
    /// (matching on `when` instead of `source.test_report.format`),
    /// (b) collapsed it to the eighth (matching on `channel` instead
    /// of `source.test_report.format`), or (c) collapsed either of
    /// the older two onto the ninth's nested-Option projection, fails
    /// HERE at ONE narrow site. The audit `every OnAttested JUnit
    /// report ships through JetStream` reads as this exact three-way
    /// conjunction at the checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_report_format_export_when_and_channel_coexist() {
        let mut export = export_with_report_format(ReportFormat::Junit);
        export.channel = VectorChannel {
            nats_subject: Some(NatsSubjectChannel::publish(
                "pleme.pleme-dev.ephemeral.r1.test-report",
                "EPHEMERAL_TEST_REPORTS",
            )),
            ..VectorChannel::default()
        };
        export.when = ExportTrigger::OnAttested;
        let spec = ephemeral_spec_with_exports(vec![export]);
        assert_eq!(
            evaluate_point_require_tag(&spec, "export-when-OnAttested"),
            Ok(true),
            "fine `export-when-OnAttested` must be true when the sole export declares that trigger",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "channel-natsSubject"),
            Ok(true),
            "fine `channel-natsSubject` must be true when the sole export ships through NATS",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "report-format-Junit"),
            Ok(true),
            "fine `report-format-Junit` must be true when the sole export declares that format",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "export-when-OnFailed"),
            Ok(false),
            "off-diagonal `export-when-OnFailed` must be false: the export declares OnAttested",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "channel-stdout"),
            Ok(false),
            "off-diagonal `channel-stdout` must be false: the export ships through NATS",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "report-format-TapV13"),
            Ok(false),
            "off-diagonal `report-format-TapV13` must be false: the export declares Junit",
        );
    }

    // ── artifact-<kind> prefix family (evaluate_point_require_tag) ───
    //
    // Fail-before-pass-after granularity: the `artifact-<kind>` prefix
    // family did not exist before this commit — the point-domain
    // require-tag vocabulary carried the nine prior closed-set-driven
    // families (`intent-<kind>`, `lifetime-<kind>`, `condition-<kind>`,
    // `must-reach-<kind>`, `sighup-<kind>`, `verification-phase-<kind>`,
    // `export-when-<kind>`, `channel-<kind>`, `report-format-<kind>`)
    // but had no way to distinguish which ARTIFACT SOURCE an ephemeral
    // export ships out — a `receipts` chain vs a `test_report`
    // ConfigMap vs a `process_snapshot` bundle vs a `run_marker`
    // synthetic event. The lift adds the TENTH closed-set-driven
    // prefix family symmetrical with the nine prior ones, routing
    // through the newly-opened
    // [`tatara_process::export::ExportSpecSliceExt::has_artifact_kind`]
    // substrate primitive via `strip_and_classify_prefixed_kind`.
    // SEVENTH instance in the workspace slice-level closed-set-driven
    // presence-probe algebra (FOURTH method on `ExportSpecSliceExt` —
    // the first slice-level extension trait carrying probes on FOUR
    // distinct axes: the `when` trigger axis, the `channel` tagged-
    // union axis, the `source.test_report.format` nested-Option scalar
    // axis, and the `source` OUTER tagged-union axis).
    //
    // Sibling to `channel-<kind>` in probe shape (tagged-union outer
    // carrier, `.select(&e.<slot>).is_some()`), distinct from
    // `report-format-<kind>` (nested-Option scalar past the outer
    // carrier). A receipts-only export answers `true` for
    // `artifact-receipts` but `false` for every `report-format-<kind>`
    // — the axes commute on the SAME `&[ExportSpec]` slice and the
    // coexistence pin below locks that split.

    /// Fixture: a minimal `ExportSpec` with a chosen [`ArtifactKind`]
    /// populated on its `source` slot and a fixed single-slot stdout
    /// channel + default `when` trigger. The artifact kind is the only
    /// axis this test module discriminates on; the channel + trigger
    /// are fixed at valid pairs so `evaluate_point_require_tag` reads
    /// the `source` slot in isolation. Sweeps [`ArtifactKind::ALL`]
    /// via `match` on the closed set so a future fifth variant added
    /// to `ALL` reaches this fixture at rustc's exhaustiveness gate.
    fn export_with_artifact(kind: ArtifactKind) -> ExportSpec {
        let source = match kind {
            ArtifactKind::Receipts => ArtifactSource {
                receipts: Some(ReceiptsSource::default()),
                ..ArtifactSource::default()
            },
            ArtifactKind::TestReport => ArtifactSource {
                test_report: Some(TestReportSource {
                    configmap: "junit-results".into(),
                    key: "junit.xml".into(),
                    format: ReportFormat::Junit,
                    namespace: None,
                }),
                ..ArtifactSource::default()
            },
            ArtifactKind::ProcessSnapshot => ArtifactSource {
                process_snapshot: Some(ProcessSnapshotSource::default()),
                ..ArtifactSource::default()
            },
            ArtifactKind::RunMarker => ArtifactSource {
                run_marker: Some(RunMarkerSource::default()),
                ..ArtifactSource::default()
            },
        };
        ExportSpec {
            source,
            channel: VectorChannel {
                stdout: Some(StdoutChannel::default()),
                ..VectorChannel::default()
            },
            when: ExportTrigger::default(),
            experiment_id_override: None,
        }
    }

    /// POPULATED-slot pin — `artifact-<kind>` dispatches through the
    /// autoderived [`ArtifactKind`] `FromStr` + the substrate
    /// [`tatara_process::export::ExportSpecSliceExt::has_artifact_kind`]
    /// primitive, returning `true` only when the resolved ephemeral
    /// lifetime carries at least one export whose `source` slot
    /// matches this kind. Sweep the [`ArtifactKind::ALL`] × ALL cross
    /// so a regression that hard-coded the arm to a single kind, or
    /// wired the closure to a fixed unrelated field (a stray
    /// `experiment_id_override.is_some()`, a probe on `when` /
    /// `channel` / `source.test_report.format`) fails HERE at the
    /// classifier before landing at the operator-facing checks.lisp
    /// surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_populated_artifact_slot_per_kind() {
        for populated in ArtifactKind::ALL {
            let spec = ephemeral_spec_with_exports(vec![export_with_artifact(populated)]);
            for query in ArtifactKind::ALL {
                let tag = format!("artifact-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "artifact populated={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// PERMANENT-lifetime pin — a default (`Permanent`) [`ProcessSpec`]
    /// returns `false` for every `artifact-<kind>` tag because the
    /// `resolved_ephemeral` gate on the parent [`Lifetime`] short-
    /// circuits the walk. Byte-for-byte symmetric with the peer
    /// permanent-lifetime pins on the seventh (`export-when-<kind>`),
    /// eighth (`channel-<kind>`), and ninth (`report-format-<kind>`)
    /// families — all four prefix families share the SAME parent
    /// gate, so a regression that dropped or narrowed the gate on
    /// `artifact-<kind>` (probing an absent `exports` slot as if it
    /// were the empty vector) fails HERE for every artifact kind and
    /// the peer tests still pass — the quartet pins the contract from
    /// four sides of the closed-set axis.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_permanent_lifetime_for_every_artifact_kind() {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in ArtifactKind::ALL {
            let tag = format!("artifact-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "permanent lifetime must return false for {tag:?}",
            );
        }
    }

    /// EMPTY-EXPORTS pin — an ephemeral [`ProcessSpec`] whose
    /// `exports` vector is empty returns `false` for every
    /// `artifact-<kind>` tag. Distinct from the permanent-lifetime
    /// case above: the resolved-ephemeral gate DOES fire, the walk
    /// over the empty vector then returns `false` for every kind.
    /// Locks the "reachable-but-empty" corner so a regression that
    /// short-circuited on `resolved_ephemeral().is_some()` alone
    /// (ignoring the exports contents) would return `true` here for
    /// every kind and fail.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_empty_exports_for_every_artifact_kind() {
        let spec = ephemeral_spec_with_exports(vec![]);
        for kind in ArtifactKind::ALL {
            let tag = format!("artifact-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "resolved-ephemeral with empty exports must return false for {tag:?}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `artifact-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. The canonical [`ArtifactKind`] labels are the
    /// camelCase wire-format keys (`receipts`, `testReport`,
    /// `processSnapshot`, `runMarker`) — matching the serde
    /// `rename_all = "camelCase"` field names on [`ArtifactSource`]
    /// verbatim — so PascalCase / snake_case / all-caps spellings are
    /// UNKNOWN suffixes. Pin the case-sensitivity axis so a regression
    /// that ASCIIfolded or PascalCased on parse (a hypothetical
    /// `to_upper_camel` normalization at the tag layer) would fail
    /// HERE. The empty-suffix boundary is pinned by the shared
    /// substrate primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_artifact_suffix() {
        let spec = ephemeral_spec_with_exports(vec![]);
        for garbage in [
            "artifact-",
            "artifact-Receipts",
            "artifact-test_report",
            "artifact-RUNMARKER",
            "artifact-typo",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// SAME-EXPORT-AXIS QUADRUPLE-COEXISTENCE pin — a Process with a
    /// SINGLE ephemeral export whose `source.test_report` slot is
    /// populated with `format = Junit`, whose `channel` is
    /// `NatsSubject`, and whose `when` is `OnAttested` MUST satisfy
    /// ALL FOUR fine tags (`artifact-testReport` from the tenth
    /// family, `export-when-OnAttested` from the seventh,
    /// `channel-natsSubject` from the eighth, `report-format-Junit`
    /// from the ninth) AND simultaneously fail each off-diagonal
    /// probe (`artifact-receipts`, `export-when-OnFailed`,
    /// `channel-stdout`, `report-format-TapV13`). Locks the semantic
    /// split between the four probes on the SAME `&[ExportSpec]`
    /// slice — the first quadruple-family conjunction in the point-
    /// domain require-tag vocabulary — so a regression that (a)
    /// collapsed `artifact-<kind>` to the eighth family (matching on
    /// `channel` instead of `source`), (b) collapsed it to the ninth
    /// (probing `source.test_report.format` instead of the outer
    /// tagged-union carrier), or (c) drifted any of the other three
    /// families onto the tenth's outer-carrier walk, fails HERE at
    /// ONE narrow site. The audit `every OnAttested JUnit test-report
    /// artifact ships through JetStream` reads as this exact four-way
    /// conjunction at the checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_artifact_coexists_with_prior_export_axes() {
        let export = ExportSpec {
            source: ArtifactSource {
                test_report: Some(TestReportSource {
                    configmap: "junit-results".into(),
                    key: "junit.xml".into(),
                    format: ReportFormat::Junit,
                    namespace: None,
                }),
                ..ArtifactSource::default()
            },
            channel: VectorChannel {
                nats_subject: Some(NatsSubjectChannel::publish(
                    "pleme.pleme-dev.ephemeral.r1.test-report",
                    "EPHEMERAL_TEST_REPORTS",
                )),
                ..VectorChannel::default()
            },
            when: ExportTrigger::OnAttested,
            experiment_id_override: None,
        };
        let spec = ephemeral_spec_with_exports(vec![export]);
        assert_eq!(
            evaluate_point_require_tag(&spec, "artifact-testReport"),
            Ok(true),
            "fine `artifact-testReport` must be true when the sole export declares that source",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "export-when-OnAttested"),
            Ok(true),
            "fine `export-when-OnAttested` must be true when the sole export declares that trigger",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "channel-natsSubject"),
            Ok(true),
            "fine `channel-natsSubject` must be true when the sole export ships through NATS",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "report-format-Junit"),
            Ok(true),
            "fine `report-format-Junit` must be true when the sole export declares that format",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "artifact-receipts"),
            Ok(false),
            "off-diagonal `artifact-receipts` must be false: the export ships a test-report",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "export-when-OnFailed"),
            Ok(false),
            "off-diagonal `export-when-OnFailed` must be false: the export declares OnAttested",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "channel-stdout"),
            Ok(false),
            "off-diagonal `channel-stdout` must be false: the export ships through NATS",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "report-format-TapV13"),
            Ok(false),
            "off-diagonal `report-format-TapV13` must be false: the export declares Junit",
        );
    }

    // ── encapsulation-mode-<kind> prefix family (evaluate_point_require_tag) ─
    //
    // Fail-before-pass-after granularity: the `encapsulation-mode-<kind>`
    // prefix family did not exist before this commit — the point-domain
    // require-tag vocabulary carried the ten prior closed-set-driven
    // families but had no way to discriminate which
    // [`EncapsulationMode`] a Process wraps its pre-existing state
    // through (`Manage` full-ownership vs `Adopt` in-place takeover vs
    // `Observe` read-only). The lift adds the ELEVENTH closed-set-
    // driven prefix family symmetrical with the ten prior ones, routing
    // through the newly-opened
    // [`tatara_process::encapsulates::EncapsulatesSpec::has_mode`]
    // substrate primitive via `strip_and_classify_prefixed_kind`.
    // SECOND instance on the SCALAR-CARRIER axis of the closed-set-
    // driven presence-probe algebra (peer of the sighup-strategy probe
    // on [`tatara_process::spec::SignalPolicy::has_sighup_strategy`]),
    // gated on the parent `Option<EncapsulatesSpec>` presence — the
    // FIRST (Option-parent × scalar-child) corner of the algebra.
    //
    // Distinct from every prior scalar-carrier probe: the sighup-
    // strategy field lives on a non-Option, `Default`-carrying parent
    // ([`tatara_process::spec::SignalPolicy`]), so on a default spec the
    // probe returns `true` on the [`SighupStrategy::default`] variant;
    // the encapsulation-mode field lives on an OPTIONAL parent
    // (`spec.encapsulates: Option<EncapsulatesSpec>`), so on a default
    // spec (no encapsulates set) the probe returns `false` for EVERY
    // mode — the operator DECLINED the encapsulation surface entirely
    // rather than defaulting into it. Locks the two-tier distinction
    // (Option-parent, scalar-child) at ONE narrow site.

    /// Fixture: a minimal [`EncapsulatesSpec`] with a chosen
    /// [`EncapsulationMode`] populated on its `mode` slot and a
    /// bare-workload payload on `kind` so `EncapsulationKind::variant`
    /// resolves (an ambiguous / empty `kind` would fail the parent
    /// CRD validation but is not what this family probes). The mode
    /// axis is the only one this test module discriminates on; the
    /// `kind` slot is fixed at a single valid variant so
    /// `evaluate_point_require_tag` reads the `mode` slot in isolation.
    fn encapsulates_with_mode(mode: EncapsulationMode) -> EncapsulatesSpec {
        EncapsulatesSpec {
            kind: EncapsulationKind {
                bare_workload: Some(BareWorkload {
                    namespace: "demo-ns".into(),
                    selector: std::collections::BTreeMap::from([("app".into(), "demo".into())]),
                }),
                ..EncapsulationKind::default()
            },
            mode,
        }
    }

    /// POPULATED-slot pin — `encapsulation-mode-<kind>` dispatches
    /// through the autoderived [`EncapsulationMode`] `FromStr` + the
    /// substrate
    /// [`tatara_process::encapsulates::EncapsulatesSpec::has_mode`]
    /// primitive, returning `true` only when the resolved
    /// [`EncapsulatesSpec`] on `spec.encapsulates` carries the queried
    /// mode. Sweep the [`EncapsulationMode::ALL`] × ALL cross so a
    /// regression that hard-coded the arm to a single variant, or
    /// wired the closure to a fixed unrelated field (a stray probe on
    /// `spec.encapsulates.as_ref().map(|e| e.kind.bare_workload
    /// .is_some())`) fails HERE at the classifier before landing at
    /// the operator-facing checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_iff_encapsulation_mode_matches_variant_per_kind() {
        for populated in EncapsulationMode::ALL {
            let mut spec = ProcessSpec::gate_compute_defaults();
            spec.encapsulates = Some(encapsulates_with_mode(populated));
            for query in EncapsulationMode::ALL {
                let tag = format!("encapsulation-mode-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "encapsulation mode={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// ABSENT-PARENT pin — a default (`encapsulates: None`)
    /// [`ProcessSpec`] returns `false` for every
    /// `encapsulation-mode-<kind>` tag because the parent
    /// `Option<EncapsulatesSpec>` gate on
    /// `spec.encapsulates.as_ref().is_some_and(…)` short-circuits the
    /// probe. Byte-for-byte symmetric with the peer permanent-lifetime
    /// pins on the four export-side families — all five gated
    /// families share the SAME shape: a parent projection produces
    /// `None` (or an `Option::None`-equivalent Lifetime resolution),
    /// the closure short-circuits, every kind reads `false`. Locks
    /// the (Option-parent × scalar-child) two-tier gate at ONE site so
    /// a regression that dropped the parent gate (probing an absent
    /// `encapsulates` slot as if it were the default
    /// [`EncapsulationMode::Manage`] via `unwrap_or_default()`) would
    /// return `true` here for `Manage` and fail. Locks the semantic
    /// split against the sighup-strategy family (which returns `true`
    /// on the [`SighupStrategy::default`] variant on a default spec
    /// because its parent [`tatara_process::spec::SignalPolicy`] is
    /// non-Option): the two scalar-carrier peers publish OPPOSITE
    /// default-spec answers on their default variants precisely
    /// because the parent shapes differ.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_absent_encapsulates_for_every_mode() {
        let spec = ProcessSpec::gate_compute_defaults();
        assert!(
            spec.encapsulates.is_none(),
            "gate_compute_defaults baseline must be greenfield (encapsulates: None)",
        );
        for kind in EncapsulationMode::ALL {
            let tag = format!("encapsulation-mode-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "absent encapsulates must return false for {tag:?}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `encapsulation-mode-<garbage>` classifies
    /// as [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. The canonical [`EncapsulationMode`] labels are the
    /// PascalCase wire-format keys (`Manage`, `Adopt`, `Observe`) —
    /// matching the serde `rename_all = "PascalCase"` external-tag
    /// form on the wire verbatim — so lowercased / all-caps / typo
    /// spellings are UNKNOWN suffixes. Pin the case-sensitivity axis
    /// so a regression that ASCIIfolded or lowercased on parse (a
    /// hypothetical `to_lower` normalization at the tag layer) would
    /// fail HERE. The empty-suffix boundary is pinned by the shared
    /// substrate primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_encapsulation_mode_suffix() {
        let mut spec = ProcessSpec::gate_compute_defaults();
        spec.encapsulates = Some(encapsulates_with_mode(EncapsulationMode::Adopt));
        for garbage in [
            "encapsulation-mode-",
            "encapsulation-mode-manage",
            "encapsulation-mode-ADOPT",
            "encapsulation-mode-Observed",
            "encapsulation-mode-Wrap",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// SCALAR-CARRIER PEER COEXISTENCE pin — a Process with
    /// `spec.signals.sighup_strategy = Restart` AND
    /// `spec.encapsulates = Some({ mode: Adopt, … })` MUST satisfy
    /// BOTH fine scalar-carrier tags (`sighup-Restart` from the fifth
    /// family, `encapsulation-mode-Adopt` from the eleventh)
    /// simultaneously AND fail each off-diagonal probe
    /// (`sighup-Reconverge`, `encapsulation-mode-Observe`,
    /// `encapsulation-mode-Manage`). Locks the SECOND scalar-carrier
    /// peer's independence from the FIRST at ONE narrow site — the
    /// two peers walk distinct parent shapes (`SignalPolicy` non-
    /// Option vs `Option<EncapsulatesSpec>`), so a regression that
    /// collapsed either onto the other's field (a stray probe of
    /// `sighup-<kind>` against `spec.encapsulates` or of
    /// `encapsulation-mode-<kind>` against `spec.signals`) would fail
    /// HERE. The audit `every Adopt-mode Process handles SIGHUP by
    /// Restart` reads as this exact two-way conjunction at the
    /// checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_encapsulation_mode_and_sighup_scalar_carriers_coexist() {
        let mut spec = ProcessSpec::gate_compute_defaults();
        spec.signals.sighup_strategy = SighupStrategy::Restart;
        spec.encapsulates = Some(encapsulates_with_mode(EncapsulationMode::Adopt));
        assert_eq!(
            evaluate_point_require_tag(&spec, "sighup-Restart"),
            Ok(true),
            "fine `sighup-Restart` must be true when the policy declares Restart",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "encapsulation-mode-Adopt"),
            Ok(true),
            "fine `encapsulation-mode-Adopt` must be true when the spec declares Adopt",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "sighup-Reconverge"),
            Ok(false),
            "off-diagonal `sighup-Reconverge` must be false: the policy declares Restart",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "encapsulation-mode-Observe"),
            Ok(false),
            "off-diagonal `encapsulation-mode-Observe` must be false: the spec declares Adopt",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "encapsulation-mode-Manage"),
            Ok(false),
            "off-diagonal `encapsulation-mode-Manage` must be false: the spec declares Adopt",
        );
    }

    // ── evaluate_point_require_tag / point-type-<kind> substrate pins ──
    //
    // Fail-before-pass-after granularity: the `point-type-<kind>` prefix
    // family did not exist before this commit — a `:requires
    // (point-type-Gate)` entry at the checks.lisp surface classified as
    // `UnknownRequireTag`. Post-lift the twelve closed-set-driven prefix
    // families in `evaluate_point_require_tag` include the classification-
    // axis scalar-carrier probe on `spec.classification.point_type`, so a
    // regression that (a) dropped the arm, (b) wired the closure to a
    // fixed unrelated field (a stray probe on `spec.classification
    // .substrate` or `spec.intent`), or (c) misspelled the prefix key
    // fails HERE at ONE narrow classifier site before propagating to
    // the operator-facing checks.lisp surface.

    /// POPULATED-slot pin — `point-type-<kind>` dispatches through the
    /// autoderived [`ConvergencePointType`] `FromStr` + the substrate
    /// [`tatara_process::classification::Classification::has_point_type`]
    /// primitive, returning `true` only when
    /// `spec.classification.point_type` matches the queried variant.
    /// Sweep the [`ConvergencePointType::ALL`] × ALL cross so a
    /// regression that hard-coded the arm to a single variant, or wired
    /// the closure to a fixed unrelated field (a stray probe on
    /// `spec.classification.substrate` or `spec.intent`) fails HERE at
    /// the classifier before landing at the operator-facing checks.lisp
    /// surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_iff_point_type_matches_variant_per_kind() {
        for populated in ConvergencePointType::ALL {
            let mut spec = ProcessSpec::gate_compute_defaults();
            spec.classification.point_type = populated;
            for query in ConvergencePointType::ALL {
                let tag = format!("point-type-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "point_type={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// UNKNOWN-suffix pin — `point-type-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. The canonical [`ConvergencePointType`] labels are
    /// the PascalCase wire-format keys (`Transform`, `Fork`, `Join`,
    /// `Gate`, `Select`, `Broadcast`, `Reduce`, `Observe`) — matching
    /// the serde `rename_all = "PascalCase"` external-tag form on the
    /// wire verbatim — so lowercased / all-caps / typo spellings are
    /// UNKNOWN suffixes. Pin the case-sensitivity axis so a regression
    /// that ASCIIfolded or lowercased on parse would fail HERE. The
    /// empty-suffix boundary is pinned by the shared substrate
    /// primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_point_type_suffix() {
        let spec = ProcessSpec::gate_compute_defaults();
        for garbage in [
            "point-type-gate",
            "point-type-FORK",
            "point-type-Joined",
            "point-type-Sink",
            "point-type-Bogus",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// BARE-PREFIX pin — the empty-suffix boundary at `point-type-`
    /// classifies as [`UnknownRequireTag`], mirroring every prior
    /// closed-set prefix family. The empty suffix hits the substrate
    /// primitive's canonical empty-string arm rather than short-
    /// circuiting to `Ok(true)` on any populated `point_type` slot.
    /// Locks the empty-suffix ↔ unknown-suffix correspondence at ONE
    /// narrow classifier site for the twelfth family so a regression
    /// that special-cased the bare prefix (treating it as "any
    /// point-type populated") would fail HERE. Byte-symmetric with
    /// the peer `evaluate_point_require_tag_returns_unknown_on_bare_lifetime_prefix`
    /// pin on the second family.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_bare_point_type_prefix() {
        let spec = ProcessSpec::gate_compute_defaults();
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-"),
            Err(UnknownRequireTag),
            "bare `point-type-` must classify as UnknownRequireTag",
        );
    }

    /// SCALAR-CARRIER THREE-WAY COEXISTENCE pin — a Process with
    /// `spec.classification.point_type = Fork`, `spec.signals
    /// .sighup_strategy = Restart`, AND
    /// `spec.encapsulates = Some({ mode: Adopt, … })` MUST satisfy the
    /// three fine scalar-carrier tags (`point-type-Fork` from the
    /// twelfth family, `sighup-Restart` from the fifth,
    /// `encapsulation-mode-Adopt` from the eleventh) simultaneously
    /// AND fail each off-diagonal probe (`point-type-Gate`,
    /// `sighup-Reconverge`, `encapsulation-mode-Manage`). Locks the
    /// THIRD scalar-carrier peer's independence from the FIRST + SECOND
    /// at ONE narrow site — the three peers walk THREE distinct parent
    /// shapes ([`tatara_process::classification::Classification`]
    /// required non-Option non-Default vs
    /// [`tatara_process::spec::SignalPolicy`] non-Option Default vs
    /// [`tatara_process::encapsulates::EncapsulatesSpec`] Option
    /// Default), so a regression that collapsed any of the three onto
    /// another's field (a stray probe of `point-type-<kind>` against
    /// `spec.intent`, of `sighup-<kind>` against `spec.classification`,
    /// or of `encapsulation-mode-<kind>` against `spec.signals`) would
    /// fail HERE. The audit `every Fork-topology Adopt-mode Process
    /// handles SIGHUP by Restart` reads as this exact three-way
    /// conjunction at the checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_point_type_coexists_with_prior_scalar_carriers() {
        let mut spec = ProcessSpec::gate_compute_defaults();
        spec.classification.point_type = ConvergencePointType::Fork;
        spec.signals.sighup_strategy = SighupStrategy::Restart;
        spec.encapsulates = Some(encapsulates_with_mode(EncapsulationMode::Adopt));
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-Fork"),
            Ok(true),
            "fine `point-type-Fork` must be true when the classification declares Fork",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "sighup-Restart"),
            Ok(true),
            "fine `sighup-Restart` must be true when the policy declares Restart",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "encapsulation-mode-Adopt"),
            Ok(true),
            "fine `encapsulation-mode-Adopt` must be true when the spec declares Adopt",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-Gate"),
            Ok(false),
            "off-diagonal `point-type-Gate` must be false: the classification declares Fork",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "sighup-Reconverge"),
            Ok(false),
            "off-diagonal `sighup-Reconverge` must be false: the policy declares Restart",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "encapsulation-mode-Manage"),
            Ok(false),
            "off-diagonal `encapsulation-mode-Manage` must be false: the spec declares Adopt",
        );
    }

    // ── evaluate_point_require_tag / substrate-<kind> substrate pins ──
    //
    // Fail-before-pass-after granularity: the `substrate-<kind>` prefix
    // family did not exist before this commit — a `:requires
    // (substrate-Compute)` entry at the checks.lisp surface classified
    // as `UnknownRequireTag`. Post-lift the thirteen closed-set-driven
    // prefix families in `evaluate_point_require_tag` include the
    // classification-axis scalar-carrier probe on
    // `spec.classification.substrate`, so a regression that (a) dropped
    // the arm, (b) wired the closure to a fixed unrelated field (a
    // stray probe on `spec.classification.point_type` or `spec.intent`),
    // or (c) misspelled the prefix key fails HERE at ONE narrow
    // classifier site before propagating to the operator-facing
    // checks.lisp surface. SECOND family on the (required-parent ×
    // required-scalar-child) corner of the presence-probe algebra
    // after `point-type-<kind>` opened it — the two co-tenants walk
    // TWO independent required scalars on the SAME
    // [`tatara_process::classification::Classification`] parent.

    /// POPULATED-slot pin — `substrate-<kind>` dispatches through the
    /// autoderived [`SubstrateType`] `FromStr` + the substrate
    /// [`tatara_process::classification::Classification::has_substrate`]
    /// primitive, returning `true` only when
    /// `spec.classification.substrate` matches the queried variant.
    /// Sweep the [`SubstrateType::ALL`] × ALL cross so a regression that
    /// hard-coded the arm to a single variant, or wired the closure to
    /// a fixed unrelated field (a stray probe on
    /// `spec.classification.point_type` or `spec.intent`) fails HERE at
    /// the classifier before landing at the operator-facing checks.lisp
    /// surface. The two-axis independence from `point-type-<kind>` is
    /// pinned at
    /// [`evaluate_point_require_tag_substrate_coexists_with_point_type`].
    #[test]
    fn evaluate_point_require_tag_returns_true_iff_substrate_matches_variant_per_kind() {
        for populated in SubstrateType::ALL {
            let mut spec = ProcessSpec::gate_compute_defaults();
            spec.classification.substrate = populated;
            for query in SubstrateType::ALL {
                let tag = format!("substrate-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "substrate={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// UNKNOWN-suffix pin — `substrate-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. The canonical [`SubstrateType`] labels are the
    /// PascalCase wire-format keys (`Financial`, `Compute`, `Network`,
    /// `Storage`, `Security`, `Identity`, `Observability`, `Regulatory`)
    /// — matching the serde `rename_all = "PascalCase"` external-tag
    /// form on the wire verbatim — so lowercased / all-caps / typo
    /// spellings are UNKNOWN suffixes. Pin the case-sensitivity axis
    /// so a regression that ASCIIfolded or lowercased on parse would
    /// fail HERE. The empty-suffix boundary is pinned by the shared
    /// substrate primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_substrate_suffix() {
        let spec = ProcessSpec::gate_compute_defaults();
        for garbage in [
            "substrate-compute",
            "substrate-STORAGE",
            "substrate-Networking",
            "substrate-Cache",
            "substrate-Bogus",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// BARE-PREFIX pin — the empty-suffix boundary at `substrate-`
    /// classifies as [`UnknownRequireTag`], mirroring every prior
    /// closed-set prefix family. The empty suffix hits the substrate
    /// primitive's canonical empty-string arm rather than short-
    /// circuiting to `Ok(true)` on any populated `substrate` slot.
    /// Locks the empty-suffix ↔ unknown-suffix correspondence at ONE
    /// narrow classifier site for the thirteenth family so a
    /// regression that special-cased the bare prefix (treating it as
    /// "any substrate populated") would fail HERE. Byte-symmetric
    /// with the peer
    /// `evaluate_point_require_tag_returns_unknown_on_bare_point_type_prefix`
    /// pin on the twelfth family — the two co-tenants on the required-
    /// parent × required-scalar-child corner walk the same empty-suffix
    /// contract.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_bare_substrate_prefix() {
        let spec = ProcessSpec::gate_compute_defaults();
        assert_eq!(
            evaluate_point_require_tag(&spec, "substrate-"),
            Err(UnknownRequireTag),
            "bare `substrate-` must classify as UnknownRequireTag",
        );
    }

    /// TWO-AXIS INDEPENDENCE pin — a Process with
    /// `spec.classification.point_type = Fork` AND
    /// `spec.classification.substrate = Storage` MUST satisfy both
    /// fine tags (`point-type-Fork` from the twelfth family,
    /// `substrate-Storage` from the thirteenth) simultaneously AND
    /// fail each off-diagonal probe (`point-type-Gate`,
    /// `substrate-Compute`). Locks the THIRTEENTH family's independence
    /// from the TWELFTH at ONE narrow site — the two co-tenants on the
    /// (required-parent × required-scalar-child) corner probe DISTINCT
    /// required scalar slots on the SAME
    /// [`tatara_process::classification::Classification`] parent, so a
    /// regression that crossed the wires (a stray probe of
    /// `substrate-<kind>` against `spec.classification.point_type`, or
    /// of `point-type-<kind>` against `spec.classification.substrate`)
    /// would fail HERE. The audit `every Fork-topology point on the
    /// Storage plane declares a JobAttested postcondition` reads as
    /// this exact conjunction of the two classification-axis required
    /// scalars at the checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_substrate_coexists_with_point_type() {
        let mut spec = ProcessSpec::gate_compute_defaults();
        spec.classification.point_type = ConvergencePointType::Fork;
        spec.classification.substrate = SubstrateType::Storage;
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-Fork"),
            Ok(true),
            "fine `point-type-Fork` must be true when the classification declares Fork",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "substrate-Storage"),
            Ok(true),
            "fine `substrate-Storage` must be true when the classification declares Storage",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-Gate"),
            Ok(false),
            "off-diagonal `point-type-Gate` must be false: the classification declares Fork",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "substrate-Compute"),
            Ok(false),
            "off-diagonal `substrate-Compute` must be false: the classification declares Storage",
        );
    }

    // ── evaluate_point_require_tag / calm-<kind> substrate pins ──────
    //
    // Fail-before-pass-after granularity: the `calm-<kind>` prefix
    // family did not exist before this commit — a `:requires
    // (calm-Monotone)` entry at the checks.lisp surface classified as
    // `UnknownRequireTag`. Post-lift the fourteen closed-set-driven
    // prefix families in `evaluate_point_require_tag` include the
    // classification-axis scalar-carrier probe on
    // `spec.classification.calm`, so a regression that (a) dropped the
    // arm, (b) wired the closure to a fixed unrelated field (a stray
    // probe on `spec.classification.point_type`,
    // `spec.classification.substrate`, or `spec.intent`), or (c)
    // misspelled the prefix key fails HERE at ONE narrow classifier
    // site before propagating to the operator-facing checks.lisp
    // surface. FIRST occupant on the (required-parent × defaulted-
    // scalar-child) corner of the presence-probe algebra — a fresh
    // corner distinct from the (required-parent × required-scalar-
    // child) corner `point-type-<kind>` + `substrate-<kind>` share:
    // the [`CalmClassification`] child carries `#[default]` on
    // [`CalmClassification::Monotone`], so a `Classification` filled
    // via `..Default::default()` on the CALM axis answers `true` on
    // the default variant (whereas a bare `Classification` on the
    // required-child corner would have to name the variant
    // deliberately to answer `true`).

    /// POPULATED-slot pin — `calm-<kind>` dispatches through the
    /// autoderived [`CalmClassification`] `FromStr` + the substrate
    /// [`tatara_process::classification::Classification::has_calm`]
    /// primitive, returning `true` only when
    /// `spec.classification.calm` matches the queried variant. Sweep
    /// the [`CalmClassification::ALL`] × ALL cross so a regression
    /// that hard-coded the arm to a single variant, or wired the
    /// closure to a fixed unrelated field (a stray probe on
    /// `spec.classification.point_type`,
    /// `spec.classification.substrate`, or `spec.intent`) fails HERE
    /// at the classifier before landing at the operator-facing
    /// checks.lisp surface. The three-axis independence from
    /// `point-type-<kind>` + `substrate-<kind>` is pinned at
    /// [`evaluate_point_require_tag_calm_coexists_with_prior_classification_axes`].
    #[test]
    fn evaluate_point_require_tag_returns_true_iff_calm_matches_variant_per_kind() {
        for populated in CalmClassification::ALL {
            let mut spec = ProcessSpec::gate_compute_defaults();
            spec.classification.calm = populated;
            for query in CalmClassification::ALL {
                let tag = format!("calm-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "calm={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// DEFAULT-ARM SHORT-CIRCUIT pin — a Process built through
    /// `ProcessSpec::gate_compute_defaults` (which carries
    /// `calm: CalmClassification::default()` =
    /// [`CalmClassification::Monotone`] via `#[default]`) answers
    /// `true` on `calm-Monotone` and `false` on `calm-NonMonotone`
    /// WITHOUT the operator naming the CALM axis on the ProcessSpec.
    /// This is the characteristic behavior of the (required-parent ×
    /// DEFAULTED-scalar-child) corner: the default-arm short-circuit
    /// is present (the operator can DECLINE to name the CALM axis
    /// and the spec still answers `true` on the default variant),
    /// distinct from the peer (required-parent × required-scalar-
    /// child) corner where an operator MUST name the variant
    /// deliberately to answer `true`. Locks the corner-property
    /// contract at ONE narrow classifier site: a regression that
    /// promoted [`CalmClassification::NonMonotone`] to `#[default]`
    /// (or wired the arm to a fixed variant answer) would fail HERE
    /// before drifting through every unadorned Process's baseline
    /// CALM answer. Peer to
    /// [`evaluate_point_require_tag_returns_true_on_default_signals_for_sighup_reconverge_only`]
    /// on the fifth scalar-carrier family (which pins the same
    /// default-arm short-circuit shape on the defaulted-parent ×
    /// defaulted-child corner) — the two corners share the "bare
    /// spec reads `true` on the default variant" contract but on
    /// distinct parent shapes.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_default_calm_for_monotone_only() {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in CalmClassification::ALL {
            let tag = format!("calm-{}", kind.as_str());
            let expected = kind == CalmClassification::Monotone;
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(expected),
                "default (calm=Monotone) baseline: tag {tag:?} must be {expected}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `calm-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. The canonical [`CalmClassification`] labels are
    /// the PascalCase wire-format keys (`Monotone`, `NonMonotone`) —
    /// matching the serde `rename_all = "PascalCase"` external-tag
    /// form on the wire verbatim — so lowercased / all-caps / typo
    /// spellings are UNKNOWN suffixes. Pin the case-sensitivity axis
    /// so a regression that ASCIIfolded or lowercased on parse would
    /// fail HERE. The empty-suffix boundary is pinned by the shared
    /// substrate primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_calm_suffix() {
        let spec = ProcessSpec::gate_compute_defaults();
        for garbage in [
            "calm-monotone",
            "calm-NONMONOTONE",
            "calm-Nonmonotone",
            "calm-ConditionallyMonotone",
            "calm-Bogus",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// BARE-PREFIX pin — the empty-suffix boundary at `calm-`
    /// classifies as [`UnknownRequireTag`], mirroring every prior
    /// closed-set prefix family. The empty suffix hits the substrate
    /// primitive's canonical empty-string arm rather than short-
    /// circuiting to `Ok(true)` on any populated `calm` slot (which
    /// would silently read as "any CALM axis present" for every
    /// Process — every Process has a CALM slot filled via the
    /// `#[serde(default)]` on the field, so the wrong short-circuit
    /// here would return `Ok(true)` universally). Locks the empty-
    /// suffix ↔ unknown-suffix correspondence at ONE narrow
    /// classifier site for the fourteenth family so a regression
    /// that special-cased the bare prefix (treating it as "any calm
    /// populated") would fail HERE. Byte-symmetric with the peer
    /// `evaluate_point_require_tag_returns_unknown_on_bare_substrate_prefix`
    /// pin on the thirteenth family and the
    /// `evaluate_point_require_tag_returns_unknown_on_bare_point_type_prefix`
    /// pin on the twelfth — the three co-tenants on the
    /// [`tatara_process::classification::Classification`] parent walk
    /// the same empty-suffix contract across two distinct corners of
    /// the (parent-shape × child-shape) algebra.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_bare_calm_prefix() {
        let spec = ProcessSpec::gate_compute_defaults();
        assert_eq!(
            evaluate_point_require_tag(&spec, "calm-"),
            Err(UnknownRequireTag),
            "bare `calm-` must classify as UnknownRequireTag",
        );
    }

    /// THREE-AXIS INDEPENDENCE pin — a Process with
    /// `spec.classification.point_type = Fork` AND
    /// `spec.classification.substrate = Storage` AND
    /// `spec.classification.calm = NonMonotone` MUST satisfy the
    /// three fine tags (`point-type-Fork` from the twelfth family,
    /// `substrate-Storage` from the thirteenth,
    /// `calm-NonMonotone` from the fourteenth) simultaneously AND
    /// fail each off-diagonal probe (`point-type-Gate`,
    /// `substrate-Compute`, `calm-Monotone`). Locks the FOURTEENTH
    /// family's independence from the TWELFTH + THIRTEENTH at ONE
    /// narrow site — the three co-tenants on the
    /// [`tatara_process::classification::Classification`] parent
    /// probe DISTINCT scalar slots on the SAME parent AND straddle
    /// TWO distinct corners of the (parent-shape × child-shape)
    /// algebra (the required-child corner `point-type-<kind>` +
    /// `substrate-<kind>` share, and the defaulted-child corner
    /// `calm-<kind>` opens). A regression that crossed the wires (a
    /// stray probe of `calm-<kind>` against
    /// `spec.classification.point_type` or
    /// `spec.classification.substrate`, or of either required-axis
    /// probe against `spec.classification.calm`) would fail HERE.
    /// The audit `every Fork-topology Storage-plane NonMonotone-CALM
    /// point declares a Raft-guarded write path` reads as this exact
    /// three-way conjunction of the two required + one defaulted
    /// classification-axis scalars at the checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_calm_coexists_with_prior_classification_axes() {
        let mut spec = ProcessSpec::gate_compute_defaults();
        spec.classification.point_type = ConvergencePointType::Fork;
        spec.classification.substrate = SubstrateType::Storage;
        spec.classification.calm = CalmClassification::NonMonotone;
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-Fork"),
            Ok(true),
            "fine `point-type-Fork` must be true when the classification declares Fork",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "substrate-Storage"),
            Ok(true),
            "fine `substrate-Storage` must be true when the classification declares Storage",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "calm-NonMonotone"),
            Ok(true),
            "fine `calm-NonMonotone` must be true when the classification declares NonMonotone",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-Gate"),
            Ok(false),
            "off-diagonal `point-type-Gate` must be false: the classification declares Fork",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "substrate-Compute"),
            Ok(false),
            "off-diagonal `substrate-Compute` must be false: the classification declares Storage",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "calm-Monotone"),
            Ok(false),
            "off-diagonal `calm-Monotone` must be false: the classification declares NonMonotone",
        );
    }

    // ── evaluate_point_require_tag / data-classification-<kind> substrate pins ──
    //
    // Fail-before-pass-after granularity: the `data-classification-<kind>`
    // prefix family did not exist before this commit — a `:requires
    // (data-classification-Pii)` entry at the checks.lisp surface
    // classified as `UnknownRequireTag`. Post-lift the fifteen closed-
    // set-driven prefix families in `evaluate_point_require_tag`
    // include the classification-axis scalar-carrier probe on
    // `spec.classification.data_classification`, so a regression that
    // (a) dropped the arm, (b) wired the closure to a fixed unrelated
    // field (a stray probe on `spec.classification.point_type`,
    // `spec.classification.substrate`, `spec.classification.calm`, or
    // `spec.intent`), or (c) misspelled the prefix key fails HERE at
    // ONE narrow classifier site before propagating to the operator-
    // facing checks.lisp surface. SECOND occupant on the (required-
    // parent × defaulted-scalar-child) corner of the presence-probe
    // algebra after `calm-<kind>` opened it — closes the four-scalar-
    // carrier corner-coverage contract on the six-axis classification
    // lattice at the classifier surface (its two required-scalar-
    // child slots AND its two defaulted-scalar-child slots all
    // publish independent presence probes through the classifier).

    /// POPULATED-slot pin — `data-classification-<kind>` dispatches
    /// through the autoderived [`DataClassification`] `FromStr` + the
    /// substrate
    /// [`tatara_process::classification::Classification::has_data_classification`]
    /// primitive, returning `true` only when
    /// `spec.classification.data_classification` matches the queried
    /// variant. Sweep the [`DataClassification::ALL`] × ALL cross so
    /// a regression that hard-coded the arm to a single variant, or
    /// wired the closure to a fixed unrelated field (a stray probe on
    /// `spec.classification.point_type` /
    /// `spec.classification.substrate` /
    /// `spec.classification.calm` or `spec.intent`) fails HERE at the
    /// classifier before landing at the operator-facing checks.lisp
    /// surface. The four-axis independence from `point-type-<kind>` +
    /// `substrate-<kind>` + `calm-<kind>` is pinned at
    /// [`evaluate_point_require_tag_data_classification_coexists_with_prior_classification_axes`].
    #[test]
    fn evaluate_point_require_tag_returns_true_iff_data_classification_matches_variant_per_kind() {
        for populated in DataClassification::ALL {
            let mut spec = ProcessSpec::gate_compute_defaults();
            spec.classification.data_classification = populated;
            for query in DataClassification::ALL {
                let tag = format!("data-classification-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "data_classification={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// DEFAULT-ARM SHORT-CIRCUIT pin — a Process built through
    /// `ProcessSpec::gate_compute_defaults` (which carries
    /// `data_classification: DataClassification::default()` =
    /// [`DataClassification::Internal`] via `#[default]`) answers
    /// `true` on `data-classification-Internal` and `false` on every
    /// other `data-classification-<kind>` (`Public`, `Confidential`,
    /// `Pii`, `Phi`, `Pci`) WITHOUT the operator naming the
    /// data-classification axis on the ProcessSpec. This is the
    /// characteristic behavior of the (required-parent × DEFAULTED-
    /// scalar-child) corner: the default-arm short-circuit is
    /// present (the operator can DECLINE to name the axis and the
    /// spec still answers `true` on the default variant). Locks the
    /// corner-property contract at ONE narrow classifier site for
    /// the SECOND occupant of the corner: a regression that promoted
    /// a different [`DataClassification`] variant to `#[default]`
    /// (or wired the arm to a fixed variant answer) would fail HERE
    /// before drifting through every unadorned Process's baseline
    /// data-classification answer. Peer to
    /// [`evaluate_point_require_tag_returns_true_on_default_calm_for_monotone_only`]
    /// on the corner's FIRST occupant — the two peers pin the same
    /// default-arm short-circuit shape on two distinct defaulted-
    /// scalar-child slots of the SAME [`tatara_process::classification::Classification`]
    /// parent.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_default_data_classification_for_internal_only() {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in DataClassification::ALL {
            let tag = format!("data-classification-{}", kind.as_str());
            let expected = kind == DataClassification::Internal;
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(expected),
                "default (data_classification=Internal) baseline: tag {tag:?} must be {expected}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `data-classification-<garbage>`
    /// classifies as [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. The canonical [`DataClassification`] labels are
    /// the PascalCase wire-format keys (`Public`, `Internal`,
    /// `Confidential`, `Pii`, `Phi`, `Pci`) — matching the serde
    /// `rename_all = "PascalCase"` external-tag form on the wire
    /// verbatim — so lowercased / all-caps / typo spellings are
    /// UNKNOWN suffixes. Pin the case-sensitivity axis so a
    /// regression that ASCIIfolded or lowercased on parse would fail
    /// HERE. The empty-suffix boundary is pinned by the shared
    /// substrate primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_data_classification_suffix() {
        let spec = ProcessSpec::gate_compute_defaults();
        for garbage in [
            "data-classification-public",
            "data-classification-INTERNAL",
            "data-classification-PII",
            "data-classification-TradeSecret",
            "data-classification-Bogus",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// BARE-PREFIX pin — the empty-suffix boundary at
    /// `data-classification-` classifies as [`UnknownRequireTag`],
    /// mirroring every prior closed-set prefix family. The empty
    /// suffix hits the substrate primitive's canonical empty-string
    /// arm rather than short-circuiting to `Ok(true)` on any
    /// populated `data_classification` slot (which would silently
    /// read as "any data-classification axis present" for every
    /// Process — every Process has a `data_classification` slot
    /// filled via the `#[serde(default)]` on the field, so the
    /// wrong short-circuit here would return `Ok(true)`
    /// universally). Locks the empty-suffix ↔ unknown-suffix
    /// correspondence at ONE narrow classifier site for the
    /// fifteenth family so a regression that special-cased the bare
    /// prefix (treating it as "any data-classification populated")
    /// would fail HERE. Byte-symmetric with the peer
    /// `evaluate_point_require_tag_returns_unknown_on_bare_calm_prefix`
    /// on the fourteenth family — the two corner co-tenants walk the
    /// same empty-suffix contract on the (required-parent ×
    /// defaulted-scalar-child) corner.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_bare_data_classification_prefix() {
        let spec = ProcessSpec::gate_compute_defaults();
        assert_eq!(
            evaluate_point_require_tag(&spec, "data-classification-"),
            Err(UnknownRequireTag),
            "bare `data-classification-` must classify as UnknownRequireTag",
        );
    }

    /// FOUR-AXIS INDEPENDENCE pin — a Process with
    /// `spec.classification.point_type = Fork` AND
    /// `spec.classification.substrate = Storage` AND
    /// `spec.classification.calm = NonMonotone` AND
    /// `spec.classification.data_classification = Pii` MUST satisfy
    /// the four fine tags (`point-type-Fork` from the twelfth
    /// family, `substrate-Storage` from the thirteenth,
    /// `calm-NonMonotone` from the fourteenth,
    /// `data-classification-Pii` from the fifteenth) simultaneously
    /// AND fail each off-diagonal probe (`point-type-Gate`,
    /// `substrate-Compute`, `calm-Monotone`,
    /// `data-classification-Internal`). Locks the FIFTEENTH family's
    /// independence from the TWELFTH + THIRTEENTH + FOURTEENTH at
    /// ONE narrow site — the four co-tenants on the
    /// [`tatara_process::classification::Classification`] parent
    /// probe DISTINCT scalar slots on the SAME parent AND straddle
    /// TWO distinct corners of the (parent-shape × child-shape)
    /// algebra (the required-child corner `point-type-<kind>` +
    /// `substrate-<kind>` share, and the defaulted-child corner
    /// `calm-<kind>` + `data-classification-<kind>` share). A
    /// regression that crossed the wires (a stray probe of
    /// `data-classification-<kind>` against
    /// `spec.classification.point_type` /
    /// `spec.classification.substrate` /
    /// `spec.classification.calm`, or of any prior classification
    /// probe against `spec.classification.data_classification`) would
    /// fail HERE. The audit `every Fork-topology Storage-plane
    /// NonMonotone-CALM Pii-classification point declares a
    /// Raft-guarded write path AND a downstream PII-scrub sink`
    /// reads as this exact four-way conjunction of the two required
    /// plus two defaulted classification-axis scalars at the
    /// checks.lisp surface — closes the four-scalar-carrier
    /// corner-coverage contract on
    /// [`tatara_process::classification::Classification`] at the
    /// classifier boundary.
    #[test]
    fn evaluate_point_require_tag_data_classification_coexists_with_prior_classification_axes() {
        let mut spec = ProcessSpec::gate_compute_defaults();
        spec.classification.point_type = ConvergencePointType::Fork;
        spec.classification.substrate = SubstrateType::Storage;
        spec.classification.calm = CalmClassification::NonMonotone;
        spec.classification.data_classification = DataClassification::Pii;
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-Fork"),
            Ok(true),
            "fine `point-type-Fork` must be true when the classification declares Fork",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "substrate-Storage"),
            Ok(true),
            "fine `substrate-Storage` must be true when the classification declares Storage",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "calm-NonMonotone"),
            Ok(true),
            "fine `calm-NonMonotone` must be true when the classification declares NonMonotone",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "data-classification-Pii"),
            Ok(true),
            "fine `data-classification-Pii` must be true when the classification declares Pii",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-Gate"),
            Ok(false),
            "off-diagonal `point-type-Gate` must be false: the classification declares Fork",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "substrate-Compute"),
            Ok(false),
            "off-diagonal `substrate-Compute` must be false: the classification declares Storage",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "calm-Monotone"),
            Ok(false),
            "off-diagonal `calm-Monotone` must be false: the classification declares NonMonotone",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "data-classification-Internal"),
            Ok(false),
            "off-diagonal `data-classification-Internal` must be false: the classification declares Pii",
        );
    }

    // ── evaluate_point_require_tag / horizon-<kind> substrate pins ───
    //
    // Fail-before-pass-after granularity: the `horizon-<kind>` prefix
    // family did not exist before this commit — a `:requires
    // (horizon-Asymptotic)` entry at the checks.lisp surface
    // classified as `UnknownRequireTag`. Post-lift the sixteen closed-
    // set-driven prefix families in `evaluate_point_require_tag`
    // include the classification-axis nested-struct-scalar-carrier
    // probe on `spec.classification.horizon.kind`, so a regression
    // that (a) dropped the arm, (b) wired the closure to a fixed
    // unrelated field (a stray probe on
    // `spec.classification.point_type`,
    // `spec.classification.substrate`, `spec.classification.calm`,
    // `spec.classification.data_classification`, or through the
    // wrong nested struct), or (c) misspelled the prefix key fails
    // HERE at ONE narrow classifier site before propagating to the
    // operator-facing checks.lisp surface. FIRST occupant on the
    // (required-parent × nested-struct-scalar-child) corner of the
    // presence-probe algebra — a fresh corner distinct from the
    // four corner-property-exhaustive scalar-carrier peers on
    // [`tatara_process::classification::Classification`] (whose
    // arms read a closed-set discriminator DIRECTLY off a scalar
    // slot without an intermediate struct hop).

    /// POPULATED-slot pin — `horizon-<kind>` dispatches through the
    /// autoderived [`HorizonKind`] `FromStr` + the substrate
    /// [`tatara_process::classification::Classification::has_horizon_kind`]
    /// primitive, returning `true` only when
    /// `spec.classification.horizon.kind` matches the queried variant.
    /// Sweep the [`HorizonKind::ALL`] × ALL cross so a regression
    /// that hard-coded the arm to a single variant, or wired the
    /// closure to a fixed unrelated field (a stray probe on
    /// `spec.classification.point_type` /
    /// `spec.classification.substrate` /
    /// `spec.classification.calm` /
    /// `spec.classification.data_classification`, or through the
    /// wrong nested struct field) fails HERE at the classifier
    /// before landing at the operator-facing checks.lisp surface.
    /// The five-axis independence from `point-type-<kind>` +
    /// `substrate-<kind>` + `calm-<kind>` +
    /// `data-classification-<kind>` is pinned at
    /// [`evaluate_point_require_tag_horizon_kind_coexists_with_prior_classification_axes`].
    #[test]
    fn evaluate_point_require_tag_returns_true_iff_horizon_kind_matches_variant_per_kind() {
        for populated in HorizonKind::ALL {
            let mut spec = ProcessSpec::gate_compute_defaults();
            spec.classification.horizon = Horizon {
                kind: populated,
                ..Horizon::default()
            };
            for query in HorizonKind::ALL {
                let tag = format!("horizon-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "horizon.kind={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// DEFAULT-ARM SHORT-CIRCUIT pin — a Process built through
    /// `ProcessSpec::gate_compute_defaults` (which carries
    /// `horizon: Horizon::default()` whose `kind` field defaults to
    /// [`HorizonKind::Bounded`] via `#[default]`) answers `true` on
    /// `horizon-Bounded` and `false` on `horizon-Asymptotic` WITHOUT
    /// the operator naming the horizon axis on the ProcessSpec. This
    /// is the characteristic behavior of the (required-parent ×
    /// nested-struct-scalar-child) corner: the default-arm short-
    /// circuit reaches through the nested [`Horizon`] struct's own
    /// [`Default`] impl to the scalar [`HorizonKind`] default (the
    /// operator can DECLINE to name the axis and the spec still
    /// answers `true` on the default variant). Locks the corner-
    /// property contract at ONE narrow classifier site for the FIRST
    /// occupant of the corner: a regression that promoted
    /// [`HorizonKind::Asymptotic`] to `#[default]` (or wired the arm
    /// to a fixed variant answer, or crossed the wires through the
    /// wrong nested struct) would fail HERE before drifting through
    /// every unadorned Process's baseline horizon answer. Peer to
    /// [`evaluate_point_require_tag_returns_true_on_default_calm_for_monotone_only`]
    /// and
    /// [`evaluate_point_require_tag_returns_true_on_default_data_classification_for_internal_only`]
    /// on the direct-scalar defaulted-child corner — this pin walks
    /// the same default-arm short-circuit shape ACROSS a struct hop.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_default_horizon_kind_for_bounded_only() {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in HorizonKind::ALL {
            let tag = format!("horizon-{}", kind.as_str());
            let expected = kind == HorizonKind::Bounded;
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(expected),
                "default (horizon.kind=Bounded) baseline: tag {tag:?} must be {expected}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `horizon-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. The canonical [`HorizonKind`] labels are the
    /// PascalCase wire-format keys (`Bounded`, `Asymptotic`) —
    /// matching the serde `rename_all = "PascalCase"` external-tag
    /// form on the wire verbatim — so lowercased / all-caps / typo
    /// spellings are UNKNOWN suffixes. Pin the case-sensitivity axis
    /// so a regression that ASCIIfolded or lowercased on parse would
    /// fail HERE. The empty-suffix boundary is pinned by the shared
    /// substrate primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_horizon_kind_suffix() {
        let spec = ProcessSpec::gate_compute_defaults();
        for garbage in [
            "horizon-bounded",
            "horizon-BOUNDED",
            "horizon-ASYMPTOTIC",
            "horizon-Periodic",
            "horizon-Bogus",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// BARE-PREFIX pin — the empty-suffix boundary at `horizon-`
    /// classifies as [`UnknownRequireTag`], mirroring every prior
    /// closed-set prefix family. The empty suffix hits the substrate
    /// primitive's canonical empty-string arm rather than
    /// short-circuiting to `Ok(true)` on any populated `horizon`
    /// nested struct (which would silently read as "any horizon
    /// axis present" for every Process — every Process has a
    /// `horizon` slot filled via the `#[serde(default)]` on the
    /// field, so the wrong short-circuit here would return `Ok(true)`
    /// universally). Locks the empty-suffix ↔ unknown-suffix
    /// correspondence at ONE narrow classifier site for the
    /// sixteenth family so a regression that special-cased the bare
    /// prefix (treating it as "any horizon populated") would fail
    /// HERE. Distinct from the peer pins on the direct-scalar
    /// defaulted-child corner (`calm-` and `data-classification-`) —
    /// this pin walks the same empty-suffix contract on the nested-
    /// struct-scalar-child corner.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_bare_horizon_kind_prefix() {
        let spec = ProcessSpec::gate_compute_defaults();
        assert_eq!(
            evaluate_point_require_tag(&spec, "horizon-"),
            Err(UnknownRequireTag),
            "bare `horizon-` must classify as UnknownRequireTag",
        );
    }

    /// FIVE-AXIS INDEPENDENCE pin — a Process with
    /// `spec.classification.point_type = Fork` AND
    /// `spec.classification.substrate = Storage` AND
    /// `spec.classification.calm = NonMonotone` AND
    /// `spec.classification.data_classification = Pii` AND
    /// `spec.classification.horizon.kind = Asymptotic` MUST satisfy
    /// the five fine tags (`point-type-Fork` from the twelfth family,
    /// `substrate-Storage` from the thirteenth, `calm-NonMonotone`
    /// from the fourteenth, `data-classification-Pii` from the
    /// fifteenth, `horizon-Asymptotic` from the sixteenth)
    /// simultaneously AND fail each off-diagonal probe
    /// (`point-type-Gate`, `substrate-Compute`, `calm-Monotone`,
    /// `data-classification-Internal`, `horizon-Bounded`). Locks the
    /// SIXTEENTH family's independence from the TWELFTH + THIRTEENTH
    /// + FOURTEENTH + FIFTEENTH at ONE narrow site — the FIVE
    /// co-tenants on the
    /// [`tatara_process::classification::Classification`] parent
    /// probe DISTINCT slots on the SAME parent AND straddle THREE
    /// distinct corners of the (parent-shape × child-shape) algebra
    /// (the required-child corner `point-type-<kind>` +
    /// `substrate-<kind>` share, the defaulted-child corner
    /// `calm-<kind>` + `data-classification-<kind>` share, and the
    /// nested-struct-child corner `horizon-<kind>` opens). A
    /// regression that crossed the wires (a stray probe of
    /// `horizon-<kind>` against `spec.classification.point_type` /
    /// `spec.classification.substrate` /
    /// `spec.classification.calm` /
    /// `spec.classification.data_classification`, or of any prior
    /// classification probe against `spec.classification.horizon.kind`)
    /// would fail HERE. The audit `every Fork-topology Storage-plane
    /// NonMonotone-CALM Pii-classification Asymptotic-horizon point
    /// declares a Raft-guarded write path AND a downstream PII-scrub
    /// sink AND a rate-window healthy-threshold metric` reads as
    /// this exact five-way conjunction of the five classification-
    /// axis discriminators at the checks.lisp surface — opens the
    /// five-way corner-coverage contract on
    /// [`tatara_process::classification::Classification`] at the
    /// classifier boundary.
    #[test]
    fn evaluate_point_require_tag_horizon_kind_coexists_with_prior_classification_axes() {
        let mut spec = ProcessSpec::gate_compute_defaults();
        spec.classification.point_type = ConvergencePointType::Fork;
        spec.classification.substrate = SubstrateType::Storage;
        spec.classification.calm = CalmClassification::NonMonotone;
        spec.classification.data_classification = DataClassification::Pii;
        spec.classification.horizon = Horizon {
            kind: HorizonKind::Asymptotic,
            ..Horizon::default()
        };
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-Fork"),
            Ok(true),
            "fine `point-type-Fork` must be true when the classification declares Fork",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "substrate-Storage"),
            Ok(true),
            "fine `substrate-Storage` must be true when the classification declares Storage",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "calm-NonMonotone"),
            Ok(true),
            "fine `calm-NonMonotone` must be true when the classification declares NonMonotone",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "data-classification-Pii"),
            Ok(true),
            "fine `data-classification-Pii` must be true when the classification declares Pii",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "horizon-Asymptotic"),
            Ok(true),
            "fine `horizon-Asymptotic` must be true when the classification declares Asymptotic",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-Gate"),
            Ok(false),
            "off-diagonal `point-type-Gate` must be false: the classification declares Fork",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "substrate-Compute"),
            Ok(false),
            "off-diagonal `substrate-Compute` must be false: the classification declares Storage",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "calm-Monotone"),
            Ok(false),
            "off-diagonal `calm-Monotone` must be false: the classification declares NonMonotone",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "data-classification-Internal"),
            Ok(false),
            "off-diagonal `data-classification-Internal` must be false: the classification declares Pii",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "horizon-Bounded"),
            Ok(false),
            "off-diagonal `horizon-Bounded` must be false: the classification declares Asymptotic",
        );
    }

    // ── evaluate_point_require_tag / optimization-direction-<kind> substrate pins ───
    //
    // Fail-before-pass-after granularity: the
    // `optimization-direction-<kind>` prefix family did not exist
    // before this commit — a `:requires (optimization-direction-
    // Maximize)` entry at the checks.lisp surface classified as
    // `UnknownRequireTag`. Post-lift the seventeen closed-set-driven
    // prefix families in `evaluate_point_require_tag` include the
    // classification-axis nested-struct-Option-scalar-carrier probe
    // on `spec.classification.horizon.direction.unwrap_or_default()`,
    // so a regression that (a) dropped the arm, (b) wired the
    // closure to a fixed unrelated field (a stray probe on
    // `spec.classification.point_type`,
    // `spec.classification.substrate`, `spec.classification.calm`,
    // `spec.classification.data_classification`,
    // `spec.classification.horizon.kind`, or through the wrong
    // Option-slot), or (c) misspelled the prefix key fails HERE at
    // ONE narrow classifier site before propagating to the operator-
    // facing checks.lisp surface. SECOND occupant on the (required-
    // parent × nested-struct-scalar-child) corner of the presence-
    // probe algebra — the FIRST occupant `horizon-<kind>` opened
    // the corner with a direct nested-scalar traversal
    // (`horizon.kind: HorizonKind`); this family adds the Option-hop
    // through `horizon.direction: Option<OptimizationDirection>` via
    // `Option::unwrap_or_default`, pinning the corner as a proven-
    // repeatable primitive shape rather than a single-example
    // curiosity.

    /// POPULATED-slot pin — `optimization-direction-<kind>` dispatches
    /// through the autoderived [`OptimizationDirection`] `FromStr`
    /// + the substrate
    /// [`tatara_process::classification::Classification::has_optimization_direction`]
    /// primitive, returning `true` only when
    /// `spec.classification.horizon.direction.unwrap_or_default()`
    /// matches the queried variant. Sweep the
    /// [`OptimizationDirection::ALL`] × ALL cross so a regression
    /// that hard-coded the arm to a single variant, or wired the
    /// closure to a fixed unrelated field (a stray probe on
    /// `spec.classification.point_type` /
    /// `spec.classification.substrate` /
    /// `spec.classification.calm` /
    /// `spec.classification.data_classification` /
    /// `spec.classification.horizon.kind`, or through the wrong
    /// Option-slot on the nested struct) fails HERE at the
    /// classifier before landing at the operator-facing checks.lisp
    /// surface. The six-axis independence from `point-type-<kind>`
    /// + `substrate-<kind>` + `calm-<kind>` +
    /// `data-classification-<kind>` + `horizon-<kind>` is pinned at
    /// [`evaluate_point_require_tag_optimization_direction_coexists_with_prior_classification_axes`].
    #[test]
    fn evaluate_point_require_tag_returns_true_iff_optimization_direction_matches_variant_per_kind()
    {
        for populated in OptimizationDirection::ALL {
            let mut spec = ProcessSpec::gate_compute_defaults();
            spec.classification.horizon = Horizon {
                kind: HorizonKind::Asymptotic,
                direction: Some(populated),
                ..Horizon::default()
            };
            for query in OptimizationDirection::ALL {
                let tag = format!("optimization-direction-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "horizon.direction=Some({populated:?}): tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// DEFAULT-ARM SHORT-CIRCUIT pin — a Process built through
    /// `ProcessSpec::gate_compute_defaults` (which carries
    /// `horizon: Horizon::default()` whose `direction` field defaults
    /// to `None`) answers `true` on `optimization-direction-Minimize`
    /// and `false` on `optimization-direction-Maximize` WITHOUT the
    /// operator naming the direction axis on the ProcessSpec. This
    /// is the characteristic behavior of the (required-parent ×
    /// nested-struct-scalar-child) corner extended with an Option-
    /// hop: the Option `None` folds onto the closed set's
    /// [`OptimizationDirection::Minimize`] default via
    /// [`Option::unwrap_or_default`], so the default-arm short-
    /// circuit reaches through TWO hops — the nested [`Horizon`]
    /// struct's own [`Default`] AND the closed set's own
    /// `#[default] Minimize` — to answer `true` on the substrate
    /// default (the operator can DECLINE to name the direction axis
    /// and the spec still answers `true` on the closed set's
    /// default variant). Locks the corner-property contract at ONE
    /// narrow classifier site for the SECOND occupant of the corner:
    /// a regression that promoted [`OptimizationDirection::Maximize`]
    /// to `#[default]` (or wired the arm to a fixed variant answer,
    /// or dropped the `unwrap_or_default` in favor of `.map(…).
    /// unwrap_or(false)` which would silently invert the corner's
    /// default-arm short-circuit shape) would fail HERE before
    /// drifting through every unadorned Process's baseline
    /// direction answer. Peer to
    /// [`evaluate_point_require_tag_returns_true_on_default_horizon_kind_for_bounded_only`]
    /// on the direct-nested-scalar corner — this pin walks the same
    /// default-arm short-circuit shape THROUGH an Option-hop.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_default_optimization_direction_for_minimize_only()
    {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in OptimizationDirection::ALL {
            let tag = format!("optimization-direction-{}", kind.as_str());
            let expected = kind == OptimizationDirection::Minimize;
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(expected),
                "default (horizon.direction=None ⇒ Minimize) baseline: tag {tag:?} must be {expected}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `optimization-direction-<garbage>`
    /// classifies as [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. The canonical [`OptimizationDirection`] labels
    /// are the PascalCase wire-format keys (`Minimize`, `Maximize`) —
    /// matching the serde `rename_all = "PascalCase"` external-tag
    /// form on the wire verbatim — so lowercased / all-caps / typo
    /// spellings are UNKNOWN suffixes. Pin the case-sensitivity axis
    /// so a regression that ASCIIfolded or lowercased on parse would
    /// fail HERE. The empty-suffix boundary is pinned by the shared
    /// substrate primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_optimization_direction_suffix() {
        let spec = ProcessSpec::gate_compute_defaults();
        for garbage in [
            "optimization-direction-minimize",
            "optimization-direction-MINIMIZE",
            "optimization-direction-MAXIMIZE",
            "optimization-direction-Stabilize",
            "optimization-direction-Bogus",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// BARE-PREFIX pin — the empty-suffix boundary at
    /// `optimization-direction-` classifies as
    /// [`UnknownRequireTag`], mirroring every prior closed-set
    /// prefix family. The empty suffix hits the substrate primitive's
    /// canonical empty-string arm rather than short-circuiting to
    /// `Ok(true)` on any populated `horizon.direction` slot (which
    /// would silently read as "any horizon.direction axis present"
    /// for every Process — every Process reaches the closed set's
    /// default arm via `unwrap_or_default`, so the wrong short-
    /// circuit here would return `Ok(true)` universally). Locks the
    /// empty-suffix ↔ unknown-suffix correspondence at ONE narrow
    /// classifier site for the seventeenth family so a regression
    /// that special-cased the bare prefix (treating it as "any
    /// horizon.direction populated" or "any horizon populated")
    /// would fail HERE. Distinct from the peer pin on the direct-
    /// nested-scalar corner (`horizon-`) — this pin walks the same
    /// empty-suffix contract on the Option-nested-scalar corner.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_bare_optimization_direction_prefix() {
        let spec = ProcessSpec::gate_compute_defaults();
        assert_eq!(
            evaluate_point_require_tag(&spec, "optimization-direction-"),
            Err(UnknownRequireTag),
            "bare `optimization-direction-` must classify as UnknownRequireTag",
        );
    }

    /// SIX-AXIS INDEPENDENCE pin — a Process with
    /// `spec.classification.point_type = Fork` AND
    /// `spec.classification.substrate = Storage` AND
    /// `spec.classification.calm = NonMonotone` AND
    /// `spec.classification.data_classification = Pii` AND
    /// `spec.classification.horizon.kind = Asymptotic` AND
    /// `spec.classification.horizon.direction = Some(Maximize)` MUST
    /// satisfy the six fine tags (`point-type-Fork` from the twelfth
    /// family, `substrate-Storage` from the thirteenth,
    /// `calm-NonMonotone` from the fourteenth,
    /// `data-classification-Pii` from the fifteenth,
    /// `horizon-Asymptotic` from the sixteenth,
    /// `optimization-direction-Maximize` from the seventeenth)
    /// simultaneously AND fail each off-diagonal probe
    /// (`point-type-Gate`, `substrate-Compute`, `calm-Monotone`,
    /// `data-classification-Internal`, `horizon-Bounded`,
    /// `optimization-direction-Minimize`). Locks the SEVENTEENTH
    /// family's independence from the TWELFTH + THIRTEENTH +
    /// FOURTEENTH + FIFTEENTH + SIXTEENTH at ONE narrow site — the
    /// SIX co-tenants on the
    /// [`tatara_process::classification::Classification`] parent
    /// probe DISTINCT slots on the SAME parent AND straddle THREE
    /// distinct corners of the (parent-shape × child-shape) algebra
    /// (the required-child corner `point-type-<kind>` +
    /// `substrate-<kind>` share, the defaulted-child corner
    /// `calm-<kind>` + `data-classification-<kind>` share, and the
    /// nested-struct-child corner now doubly populated by
    /// `horizon-<kind>` + `optimization-direction-<kind>`). A
    /// regression that crossed the wires (a stray probe of
    /// `optimization-direction-<kind>` against
    /// `spec.classification.point_type` /
    /// `spec.classification.substrate` /
    /// `spec.classification.calm` /
    /// `spec.classification.data_classification` /
    /// `spec.classification.horizon.kind`, or of any prior
    /// classification probe against
    /// `spec.classification.horizon.direction`) would fail HERE.
    /// The audit `every Fork-topology Storage-plane NonMonotone-CALM
    /// Pii-classification Asymptotic-horizon Maximize-direction
    /// point declares a Raft-guarded write path AND a downstream
    /// PII-scrub sink AND a rate-window healthy-threshold metric
    /// oriented for throughput` reads as this exact six-way
    /// conjunction of the six classification-axis discriminators at
    /// the checks.lisp surface — closes the SIX-axis corner-
    /// coverage contract on the six-dimensional classification
    /// lattice at the classifier boundary.
    #[test]
    fn evaluate_point_require_tag_optimization_direction_coexists_with_prior_classification_axes() {
        let mut spec = ProcessSpec::gate_compute_defaults();
        spec.classification.point_type = ConvergencePointType::Fork;
        spec.classification.substrate = SubstrateType::Storage;
        spec.classification.calm = CalmClassification::NonMonotone;
        spec.classification.data_classification = DataClassification::Pii;
        spec.classification.horizon = Horizon {
            kind: HorizonKind::Asymptotic,
            direction: Some(OptimizationDirection::Maximize),
            ..Horizon::default()
        };
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-Fork"),
            Ok(true),
            "fine `point-type-Fork` must be true when the classification declares Fork",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "substrate-Storage"),
            Ok(true),
            "fine `substrate-Storage` must be true when the classification declares Storage",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "calm-NonMonotone"),
            Ok(true),
            "fine `calm-NonMonotone` must be true when the classification declares NonMonotone",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "data-classification-Pii"),
            Ok(true),
            "fine `data-classification-Pii` must be true when the classification declares Pii",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "horizon-Asymptotic"),
            Ok(true),
            "fine `horizon-Asymptotic` must be true when the classification declares Asymptotic",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "optimization-direction-Maximize"),
            Ok(true),
            "fine `optimization-direction-Maximize` must be true when the classification declares Maximize",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "point-type-Gate"),
            Ok(false),
            "off-diagonal `point-type-Gate` must be false: the classification declares Fork",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "substrate-Compute"),
            Ok(false),
            "off-diagonal `substrate-Compute` must be false: the classification declares Storage",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "calm-Monotone"),
            Ok(false),
            "off-diagonal `calm-Monotone` must be false: the classification declares NonMonotone",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "data-classification-Internal"),
            Ok(false),
            "off-diagonal `data-classification-Internal` must be false: the classification declares Pii",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "horizon-Bounded"),
            Ok(false),
            "off-diagonal `horizon-Bounded` must be false: the classification declares Asymptotic",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "optimization-direction-Minimize"),
            Ok(false),
            "off-diagonal `optimization-direction-Minimize` must be false: the classification declares Maximize",
        );
    }

    // ── teardown-policy-<kind> prefix family (evaluate_point_require_tag) ─
    //
    // Fail-before-pass-after granularity: the `teardown-policy-<kind>`
    // prefix family did not exist before this commit — the point-domain
    // require-tag vocabulary carried the seventeen prior closed-set-
    // driven families but had no way to distinguish which
    // [`TeardownPolicy`] variant an ephemeral Process declares — the
    // "SIGTERM on Attested only" posture (leave `Failed` for forensic
    // inspection) vs the "SIGTERM on both" default vs the "TTL-only,
    // never auto-SIGTERM" opt-out. The lift adds the EIGHTEENTH
    // closed-set-driven prefix family symmetrical with the seventeen
    // prior ones, routing through the newly-opened
    // [`tatara_process::lifetime::EphemeralLifetime::has_teardown_policy`]
    // substrate primitive via `strip_and_classify_prefixed_kind`. FIRST
    // occupant on the (Option-parent × defaulted-scalar-child) corner
    // of the (parent-shape × child-shape) presence-probe algebra — the
    // parent-shape half is Option-typed (`resolved_ephemeral()` returns
    // `None` on a `Permanent` lifetime or an ambiguous `Lifetime`); the
    // child-shape half is a defaulted scalar
    // ([`TeardownPolicy::Always`] via `#[default]`), distinct from
    // every prior corner. The dual short-circuit — Option-parent
    // silences EVERY kind while the reachable-default-child arm honors
    // the closed set's own `#[default]` — pins the corner as a
    // proven-repeatable primitive shape rather than a single-example
    // curiosity.

    /// POPULATED-slot pin — `teardown-policy-<kind>` dispatches through
    /// the autoderived [`TeardownPolicy`] `FromStr` + the substrate
    /// [`tatara_process::lifetime::EphemeralLifetime::has_teardown_policy`]
    /// primitive, returning `true` only when the resolved ephemeral
    /// lifetime's `teardown_policy` field matches the queried variant.
    /// Sweep the [`TeardownPolicy::ALL`] × ALL cross so a regression
    /// that hard-coded the arm to a single variant (silently returning
    /// `true` on every populated ephemeral regardless of query kind)
    /// or wired the closure to a fixed unrelated field (a stray probe
    /// on `ttl` / `max_concurrent` / `exports`) fails HERE at the
    /// classifier before landing at the operator-facing checks.lisp
    /// surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_populated_teardown_policy_slot_per_kind() {
        for populated in TeardownPolicy::ALL {
            let spec = ProcessSpec {
                lifetime: Lifetime::ephemeral(EphemeralLifetime {
                    teardown_policy: populated,
                    ..EphemeralLifetime::default()
                }),
                ..ProcessSpec::gate_compute_defaults()
            };
            for query in TeardownPolicy::ALL {
                let tag = format!("teardown-policy-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "teardown_policy={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// PERMANENT-lifetime pin — a default (`Permanent`) [`ProcessSpec`]
    /// returns `false` for every `teardown-policy-<kind>` tag because
    /// the `resolved_ephemeral` gate on the parent [`Lifetime`] short-
    /// circuits the walk. Locks the Option-parent silencing contract
    /// (`resolved_ephemeral().is_some_and(|e| e.has_teardown_policy(k))`)
    /// so a regression that dropped the ephemeral gate (probing an
    /// absent `teardown_policy` slot as if it were the closed set's
    /// default) would return `true` on
    /// `teardown-policy-Always` here for every Process — the two
    /// corners (unreachable-parent vs reachable-default-child) both
    /// answer `false` on non-Always kinds but through different arms
    /// of the closed-set-driven projection, and the Always-arm splits
    /// them: this pin returns `false` for EVERY kind including
    /// `Always`, while the default-ephemeral pin below returns `true`
    /// on `Always` alone. The pair pins the semantics from both sides.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_permanent_lifetime_for_every_teardown_policy_kind(
    ) {
        let spec = ProcessSpec::gate_compute_defaults();
        for kind in TeardownPolicy::ALL {
            let tag = format!("teardown-policy-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "permanent lifetime must return false for {tag:?}",
            );
        }
    }

    /// DEFAULT-ARM SHORT-CIRCUIT pin — a Process whose lifetime is
    /// [`EphemeralLifetime::default`] (`teardown_policy:
    /// TeardownPolicy::default() = Always`) answers `true` on
    /// `teardown-policy-Always` and `false` on every other variant
    /// WITHOUT the operator naming the teardown-policy axis in the
    /// ephemeral spec. This is the characteristic behavior of the
    /// (Option-parent × defaulted-scalar-child) corner reachable arm:
    /// the resolved-ephemeral gate DOES fire (the parent projection
    /// returns `Some(&EphemeralLifetime)`), the scalar comparison then
    /// picks up the closed set's `#[default] Always`. Peer to
    /// [`evaluate_point_require_tag_returns_true_on_default_calm_for_monotone_only`]
    /// on the (required-parent × defaulted-scalar-child) corner —
    /// this pin walks the same default-arm short-circuit shape
    /// THROUGH the Option-parent hop. Locks the corner-property
    /// contract at ONE narrow classifier site: a regression that
    /// promoted [`TeardownPolicy::OnAttested`] to `#[default]` (or
    /// wired the arm to a fixed variant answer, or dropped the direct
    /// scalar comparison in favor of a `.map(...).unwrap_or(false)`
    /// which would silently invert the corner's default-arm short-
    /// circuit shape) would fail HERE before drifting through every
    /// unadorned ephemeral Process's baseline teardown answer.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_default_ephemeral_teardown_policy_for_always_only(
    ) {
        let spec = ProcessSpec {
            lifetime: Lifetime::ephemeral(EphemeralLifetime::default()),
            ..ProcessSpec::gate_compute_defaults()
        };
        for kind in TeardownPolicy::ALL {
            let tag = format!("teardown-policy-{}", kind.as_str());
            let expected = kind == TeardownPolicy::Always;
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(expected),
                "default ephemeral (teardown_policy=Always) baseline: tag {tag:?} must be {expected}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `teardown-policy-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. The canonical [`TeardownPolicy`] labels are the
    /// PascalCase wire-format keys (`Always`, `OnAttested`,
    /// `OnFailed`, `Never`) — matching the serde `rename_all =
    /// "PascalCase"` external-tag form on the wire verbatim — so
    /// lowercased / all-caps / typo spellings are UNKNOWN suffixes.
    /// Pin the case-sensitivity axis so a regression that ASCIIfolded
    /// or lowercased on parse would fail HERE. The empty-suffix
    /// boundary is pinned by the shared substrate primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_teardown_policy_suffix() {
        let spec = ProcessSpec {
            lifetime: Lifetime::ephemeral(EphemeralLifetime::default()),
            ..ProcessSpec::gate_compute_defaults()
        };
        for garbage in [
            "teardown-policy-always",
            "teardown-policy-ALWAYS",
            "teardown-policy-onattested",
            "teardown-policy-OnAtested",
            "teardown-policy-Bogus",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// BARE-PREFIX pin — the empty-suffix boundary at
    /// `teardown-policy-` classifies as [`UnknownRequireTag`],
    /// mirroring every prior closed-set prefix family. The empty
    /// suffix hits the substrate primitive's canonical empty-string
    /// arm rather than short-circuiting to `Ok(true)` on any populated
    /// ephemeral (which would silently read as "any teardown-policy
    /// axis present" for every ephemeral Process — every ephemeral
    /// spec carries a defaulted teardown_policy, so the wrong short-
    /// circuit here would return `Ok(true)` universally on every
    /// ephemeral). Locks the empty-suffix ↔ unknown-suffix
    /// correspondence at ONE narrow classifier site for the eighteenth
    /// family so a regression that special-cased the bare prefix
    /// (treating it as "any resolved-ephemeral") would fail HERE. On
    /// the permanent-lifetime side the same bare prefix is ALSO
    /// unknown — the substrate primitive rejects the empty suffix
    /// before the ephemeral gate fires — pinning the (Option-parent ×
    /// defaulted-scalar-child) corner's empty-suffix contract as
    /// independent of the parent-arm branch.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_bare_teardown_policy_prefix() {
        let ephemeral_spec = ProcessSpec {
            lifetime: Lifetime::ephemeral(EphemeralLifetime::default()),
            ..ProcessSpec::gate_compute_defaults()
        };
        assert_eq!(
            evaluate_point_require_tag(&ephemeral_spec, "teardown-policy-"),
            Err(UnknownRequireTag),
            "bare `teardown-policy-` must classify as UnknownRequireTag on an ephemeral spec",
        );
        let permanent_spec = ProcessSpec::gate_compute_defaults();
        assert_eq!(
            evaluate_point_require_tag(&permanent_spec, "teardown-policy-"),
            Err(UnknownRequireTag),
            "bare `teardown-policy-` must classify as UnknownRequireTag on a permanent spec",
        );
    }

    /// COARSE / FINE / SIBLING COEXISTENCE pin — a Process with an
    /// ephemeral lifetime whose `teardown_policy = OnAttested` AND at
    /// least one export at `OnAttested` MUST satisfy the coarse
    /// `lifetime-ephemeral` presence probe, the fine
    /// `teardown-policy-OnAttested` prefix tag (this family), the
    /// sibling `export-when-OnAttested` prefix tag (the SEVENTH family
    /// on the SAME resolved-ephemeral Option-parent, one axis over on
    /// the slice-child arm) AND simultaneously fail the off-diagonal
    /// `teardown-policy-Always` / `teardown-policy-OnFailed` /
    /// `teardown-policy-Never` probes. Locks the semantic split
    /// between the three surfaces at ONE narrow site so a regression
    /// that (a) collapsed `teardown-policy-<kind>` to the coarse
    /// `lifetime-ephemeral` fixed answer (returning `true` for every
    /// kind on any ephemeral Process), (b) crossed the wires between
    /// `teardown-policy-<kind>` and `export-when-<kind>` (probing the
    /// `exports` slice for a teardown-policy query), or (c) drifted
    /// the `lifetime-ephemeral` arm to match on teardown-policy kind,
    /// fails HERE. The three arms of the resolved-ephemeral projection
    /// — coarse presence, scalar-child kind, slice-child kind — all
    /// share the SAME Option-parent gate; this pin verifies they read
    /// distinct slots through it.
    #[test]
    fn evaluate_point_require_tag_teardown_policy_coexists_with_export_when_and_lifetime_ephemeral()
    {
        let spec = ProcessSpec {
            lifetime: Lifetime::ephemeral(EphemeralLifetime {
                teardown_policy: TeardownPolicy::OnAttested,
                exports: vec![export_at(ExportTrigger::OnAttested)],
                ..EphemeralLifetime::default()
            }),
            ..ProcessSpec::gate_compute_defaults()
        };
        assert_eq!(
            evaluate_point_require_tag(&spec, "lifetime-ephemeral"),
            Ok(true),
            "coarse `lifetime-ephemeral` must be true on an ephemeral Process",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "teardown-policy-OnAttested"),
            Ok(true),
            "fine `teardown-policy-OnAttested` must be true when the ephemeral declares OnAttested",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "export-when-OnAttested"),
            Ok(true),
            "sibling `export-when-OnAttested` must be true when a matching export is present",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "teardown-policy-Always"),
            Ok(false),
            "off-diagonal `teardown-policy-Always` must be false: the ephemeral declares OnAttested",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "teardown-policy-OnFailed"),
            Ok(false),
            "off-diagonal `teardown-policy-OnFailed` must be false: the ephemeral declares OnAttested",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "teardown-policy-Never"),
            Ok(false),
            "off-diagonal `teardown-policy-Never` must be false: the ephemeral declares OnAttested",
        );
    }

    // ── routing-form-<kind> prefix family (evaluate_point_require_tag) ─
    //
    // Fail-before-pass-after granularity: the `routing-form-<kind>`
    // prefix family did not exist before this commit — the point-domain
    // require-tag vocabulary carried the eighteen prior closed-set-
    // driven families but had no way to distinguish which
    // [`RoutingForm`] a Process declares intent for — the "hold the
    // ProcessTable claim, emit the unprefixed `${app}.${cluster}` FQDN"
    // stable-name posture vs the default per-instance
    // `${app}.${eph_id}.${cluster}` FQDN. The lift adds the NINETEENTH
    // closed-set-driven prefix family symmetrical with the eighteen
    // prior ones, routing through the newly-opened
    // [`tatara_process::routing::RoutingSpec::has_form`] substrate
    // primitive via `strip_and_classify_prefixed_kind`. SECOND occupant
    // on the (Option-parent × defaulted-scalar-child) corner of the
    // (parent-shape × child-shape) presence-probe algebra opened by
    // `teardown-policy-<kind>` — the parent-shape half is Option-typed
    // (`spec.routing` is `Option<RoutingSpec>`, `None` on an in-cluster-
    // only Process); the child-shape half is a defaulted scalar
    // ([`RoutingForm::Instance`] via [`RoutingForm::from_is_stable`]
    // over the `#[serde(default)]` false bool `stable_name_claim`).
    // Distinct from `teardown-policy-<kind>` in that the child is
    // DERIVED from a raw bool through the ONE substrate composer
    // [`RoutingForm::from_is_stable`], not stored as-is — the (Option-
    // parent × defaulted-scalar-child) corner now admits BOTH
    // stored-child (`teardown-policy-<kind>`) and derived-child
    // (`routing-form-<kind>`) traversals through the SAME `has(kind)`
    // shape.

    /// POPULATED-slot pin — `routing-form-<kind>` dispatches through
    /// the autoderived [`RoutingForm`] `FromStr` + the substrate
    /// [`tatara_process::routing::RoutingSpec::has_form`] primitive,
    /// returning `true` only when the resolved routing spec's derived
    /// [`RoutingForm`] matches the queried variant. Sweep the
    /// [`RoutingForm::ALL`] × ALL cross so a regression that (a) hard-
    /// coded the arm to a single variant (silently returning `true`
    /// on every populated routing spec regardless of query kind), (b)
    /// dropped the composition through [`RoutingForm::from_is_stable`]
    /// (drifting from every other consumer of the `stable_name_claim
    /// → RoutingForm` projection), or (c) wired the closure to an
    /// unrelated field (a stray probe on `priority` /
    /// `hostnames.len()`) fails HERE at the classifier before landing
    /// at the operator-facing checks.lisp surface.
    #[test]
    fn evaluate_point_require_tag_returns_true_iff_routing_form_matches_variant_per_kind() {
        for is_stable in [true, false] {
            let populated = RoutingForm::from_is_stable(is_stable);
            let spec = ProcessSpec {
                routing: Some(RoutingSpec {
                    hostnames: vec![RoutingHostname::content_hashed("api")],
                    backend: RoutingBackend::plain("svc", 80),
                    stable_name_claim: is_stable,
                    priority: 0,
                }),
                ..ProcessSpec::gate_compute_defaults()
            };
            for query in RoutingForm::ALL {
                let tag = format!("routing-form-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_point_require_tag(&spec, &tag),
                    Ok(expected),
                    "stable_name_claim={is_stable} populated={populated:?}: tag {tag:?} classification drifted",
                );
            }
        }
    }

    /// OPTION-PARENT SHORT-CIRCUIT pin — a default (`routing: None`)
    /// [`ProcessSpec`] returns `false` for every `routing-form-<kind>`
    /// tag because the `spec.routing.as_ref()` gate short-circuits the
    /// walk. Locks the Option-parent silencing contract
    /// (`spec.routing.as_ref().is_some_and(|r| r.has_form(k))`) so a
    /// regression that dropped the Option gate (probing an absent
    /// routing slot as if it carried the defaulted `Instance` form)
    /// would return `true` on `routing-form-instance` here for every
    /// Process. Splits the two corners of the (Option-parent ×
    /// defaulted-scalar-child) shape from the OTHER side: the
    /// Option-parent arm silences EVERY kind (including the closed-
    /// set's default), while the reachable-default-child arm honors
    /// the default (see the DEFAULT-ARM pin below). The two pins
    /// bracket the corner semantics from both sides.
    #[test]
    fn evaluate_point_require_tag_returns_false_on_absent_routing_for_every_routing_form_kind() {
        let spec = ProcessSpec::gate_compute_defaults();
        assert!(spec.routing.is_none());
        for kind in RoutingForm::ALL {
            let tag = format!("routing-form-{}", kind.as_str());
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(false),
                "absent routing must return false for {tag:?}",
            );
        }
    }

    /// DEFAULT-ARM SHORT-CIRCUIT pin — a Process whose `routing` slot
    /// is a [`RoutingSpec`] with `stable_name_claim` at its
    /// `#[serde(default)]` (bool default = `false`) answers `true` on
    /// `routing-form-instance` and `false` on every other variant
    /// WITHOUT the operator naming the routing-form axis. This is the
    /// characteristic behavior of the (Option-parent × defaulted-
    /// scalar-child) corner reachable arm — traversed through a
    /// DERIVED child (via [`RoutingForm::from_is_stable`]) rather than
    /// the stored child `has_teardown_policy` walks. The Option-parent
    /// gate DOES fire (`spec.routing.as_ref()` returns `Some(&_)`), the
    /// derived scalar comparison then picks up the closed set's
    /// derived default (`from_is_stable(false) = Instance`). Peer to
    /// [`evaluate_point_require_tag_returns_true_on_default_ephemeral_teardown_policy_for_always_only`]
    /// on the SAME corner — that pin walks the STORED-child arm; this
    /// pin walks the DERIVED-child arm; the corner admits both
    /// traversals through the SAME `has(kind)` shape.
    #[test]
    fn evaluate_point_require_tag_returns_true_on_default_routing_form_for_instance_only() {
        let spec = ProcessSpec {
            routing: Some(RoutingSpec {
                hostnames: vec![RoutingHostname::content_hashed("api")],
                backend: RoutingBackend::plain("svc", 80),
                stable_name_claim: bool::default(),
                priority: 0,
            }),
            ..ProcessSpec::gate_compute_defaults()
        };
        for kind in RoutingForm::ALL {
            let tag = format!("routing-form-{}", kind.as_str());
            let expected = kind == RoutingForm::Instance;
            assert_eq!(
                evaluate_point_require_tag(&spec, &tag),
                Ok(expected),
                "default routing (stable_name_claim=false → Instance) baseline: tag {tag:?} must be {expected}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `routing-form-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag: <verbatim>` diagnostic
    /// path fires. The canonical [`RoutingForm`] labels are the
    /// lower-case wire-format keys (`stable`, `instance`) — matching
    /// the [`crate::annotations::ROUTING_FORM`] annotation / label
    /// values the reconciler stamps verbatim — so PascalCase / typo
    /// spellings are UNKNOWN suffixes. Pin the case-sensitivity axis
    /// so a regression that ASCIIfolded or Pascal-cased on parse
    /// would fail HERE. The empty-suffix boundary is pinned by the
    /// shared substrate primitive's
    /// [`strip_and_classify_prefixed_kind_returns_unknown_on_empty_suffix`]
    /// so no per-family duplicate here.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_unknown_routing_form_suffix() {
        let spec = ProcessSpec {
            routing: Some(RoutingSpec {
                hostnames: vec![RoutingHostname::content_hashed("api")],
                backend: RoutingBackend::plain("svc", 80),
                stable_name_claim: true,
                priority: 0,
            }),
            ..ProcessSpec::gate_compute_defaults()
        };
        for garbage in [
            "routing-form-Stable",
            "routing-form-STABLE",
            "routing-form-Instance",
            "routing-form-stble",
            "routing-form-Gateway",
        ] {
            assert_eq!(
                evaluate_point_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// BARE-PREFIX pin — the empty-suffix boundary at `routing-form-`
    /// classifies as [`UnknownRequireTag`], mirroring every prior
    /// closed-set prefix family. The empty suffix hits the substrate
    /// primitive's canonical empty-string arm rather than short-
    /// circuiting to `Ok(true)` on any populated routing (which would
    /// silently read as "any routing axis present" for every Process
    /// with a routing slot). Locks the empty-suffix ↔ unknown-suffix
    /// correspondence at ONE narrow classifier site for the nineteenth
    /// family so a regression that special-cased the bare prefix
    /// would fail HERE. On the absent-routing side the same bare
    /// prefix is ALSO unknown — the substrate primitive rejects the
    /// empty suffix before the Option-parent gate fires — pinning the
    /// (Option-parent × defaulted-scalar-child) corner's empty-suffix
    /// contract as independent of the parent-arm branch.
    #[test]
    fn evaluate_point_require_tag_returns_unknown_on_bare_routing_form_prefix() {
        let routed_spec = ProcessSpec {
            routing: Some(RoutingSpec {
                hostnames: vec![RoutingHostname::content_hashed("api")],
                backend: RoutingBackend::plain("svc", 80),
                stable_name_claim: true,
                priority: 0,
            }),
            ..ProcessSpec::gate_compute_defaults()
        };
        assert_eq!(
            evaluate_point_require_tag(&routed_spec, "routing-form-"),
            Err(UnknownRequireTag),
            "bare `routing-form-` must classify as UnknownRequireTag on a routed spec",
        );
        let unrouted_spec = ProcessSpec::gate_compute_defaults();
        assert_eq!(
            evaluate_point_require_tag(&unrouted_spec, "routing-form-"),
            Err(UnknownRequireTag),
            "bare `routing-form-` must classify as UnknownRequireTag on an unrouted spec",
        );
    }

    /// COEXISTENCE pin — a Process with an ephemeral lifetime whose
    /// `teardown_policy = OnAttested` AND a routing spec with
    /// `stable_name_claim = true` (a canary-of-a-stable-name preview
    /// posture) MUST simultaneously satisfy the fine
    /// `routing-form-stable` prefix tag (this family), the sibling
    /// `teardown-policy-OnAttested` prefix tag (the EIGHTEENTH family
    /// on the SAME (Option-parent × defaulted-scalar-child) corner,
    /// stored-child arm), the coarse `lifetime-ephemeral` presence
    /// probe, AND simultaneously fail the off-diagonal
    /// `routing-form-instance` probe. Locks the semantic split
    /// between the three surfaces at ONE narrow site so a regression
    /// that (a) collapsed `routing-form-<kind>` to the coarse
    /// `lifetime-ephemeral` fixed answer, (b) crossed the wires
    /// between `routing-form-<kind>` and `teardown-policy-<kind>`
    /// (probing the wrong Option-parent for a routing-form query),
    /// or (c) drifted the derived-child arm to match the stored-child
    /// projection, fails HERE. The two corners of the (Option-parent
    /// × defaulted-scalar-child) shape — stored-child on
    /// `resolved_ephemeral()`, derived-child on `routing.as_ref()` —
    /// read distinct Option-parents through the same
    /// `strip_and_classify_prefixed_kind` shape; this pin verifies
    /// they read distinct slots through distinct parents without
    /// cross-talk.
    #[test]
    fn evaluate_point_require_tag_routing_form_coexists_with_lifetime_ephemeral_and_teardown_policy(
    ) {
        let spec = ProcessSpec {
            lifetime: Lifetime::ephemeral(EphemeralLifetime {
                teardown_policy: TeardownPolicy::OnAttested,
                ..EphemeralLifetime::default()
            }),
            routing: Some(RoutingSpec {
                hostnames: vec![RoutingHostname::content_hashed("api")],
                backend: RoutingBackend::plain("svc", 80),
                stable_name_claim: true,
                priority: 0,
            }),
            ..ProcessSpec::gate_compute_defaults()
        };
        assert_eq!(
            evaluate_point_require_tag(&spec, "lifetime-ephemeral"),
            Ok(true),
            "coarse `lifetime-ephemeral` must be true on an ephemeral Process",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "teardown-policy-OnAttested"),
            Ok(true),
            "sibling `teardown-policy-OnAttested` must be true when the ephemeral declares OnAttested",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "routing-form-stable"),
            Ok(true),
            "fine `routing-form-stable` must be true when stable_name_claim is set",
        );
        assert_eq!(
            evaluate_point_require_tag(&spec, "routing-form-instance"),
            Ok(false),
            "off-diagonal `routing-form-instance` must be false: the routing declares Stable",
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

    // ── ephemeral condition-<kind> prefix family pins ────────────────
    //
    // Fail-before-pass-after granularity: the ephemeral surface's
    // `condition-<kind>` prefix family + the
    // `tatara_process::ephemeral::EphemeralSpec::has_condition_kind`
    // inherent method did not exist before this commit — the ephemeral
    // require-tag vocabulary carried only fixed match arms, so an
    // operator authoring `:requires (condition-ClosedLoopAuth)` in a
    // `(lisp-compiles … :domain ephemeral)` check would classify as
    // `UnknownRequireTag`. The lift adds the fifth closed-set-driven
    // prefix family in the workspace-wide require-tag algebra
    // (byte-for-byte symmetrical with the point surface's
    // `condition-<kind>` family), routing through the newly-opened
    // [`tatara_process::ephemeral::EphemeralSpec::has_condition_kind`]
    // substrate primitive via `strip_and_classify_prefixed_kind`.

    /// POPULATED-slot pin — `condition-<kind>` dispatches through the
    /// autoderived [`ConditionKind`] `FromStr` + the substrate
    /// [`tatara_process::ephemeral::EphemeralSpec::has_condition_kind`]
    /// primitive, returning `true` only when the ephemeral spec
    /// carries at least one [`Condition`] with this kind on the pre
    /// ∪ post union. Sweep the [`ConditionKind::ALL`] × ALL cross so
    /// a regression that hard-coded the arm to a single kind or wired
    /// the closure to a fixed unrelated field fails HERE at the
    /// ephemeral classifier before landing at the operator-facing
    /// checks.lisp surface. Byte-for-byte peer of
    /// [`evaluate_point_require_tag_returns_true_on_populated_condition_slot_per_kind`]
    /// on the point surface.
    #[test]
    fn evaluate_ephemeral_require_tag_returns_true_on_populated_condition_slot_per_kind() {
        for populated in ConditionKind::ALL {
            let mut spec = ephemeral_fixture();
            spec.postconditions.push(Condition {
                kind: populated,
                params: serde_json::Value::Null,
            });
            for query in ConditionKind::ALL {
                let tag = format!("condition-{}", query.as_str());
                let expected = query == populated;
                assert_eq!(
                    evaluate_ephemeral_require_tag(&spec, &tag),
                    Ok(expected),
                    "ephemeral condition populated={populated:?}: tag {tag:?} drifted",
                );
            }
        }
    }

    /// EMPTY-SPEC pin — a default ephemeral fixture (empty
    /// preconditions, empty postconditions) returns `Ok(false)` for
    /// every `condition-<kind>` tag. Locks the write-side / read-side
    /// split on the presence-probe boundary so an operator authoring
    /// `:requires (condition-JobAttested)` against an ephemeral env
    /// whose boundary lists no such predicate gets the
    /// `definition missing required` diagnostic, not a false-positive
    /// pass.
    #[test]
    fn evaluate_ephemeral_require_tag_returns_false_on_empty_slots_for_every_condition_kind() {
        let spec = ephemeral_fixture();
        for kind in ConditionKind::ALL {
            let tag = format!("condition-{}", kind.as_str());
            assert_eq!(
                evaluate_ephemeral_require_tag(&spec, &tag),
                Ok(false),
                "empty ephemeral boundary must return false for {tag:?}",
            );
        }
    }

    /// UNKNOWN-suffix pin — `condition-<garbage>` classifies as
    /// [`UnknownRequireTag`] via the shared
    /// `strip_and_classify_prefixed_kind` primitive so the caller's
    /// operator-facing `unknown :requires tag for ephemeral domain:
    /// <verbatim>` diagnostic path fires. A regression that fell
    /// through to `Ok(false)` (matching the pre-lift fixed-tag
    /// `_ => Err(UnknownRequireTag)` tail on a mis-typed prefix
    /// suffix) would silently reclassify a `condition-jobAttested`
    /// casing typo (PascalCase-only closed set) as `definition missing
    /// required`, which reads as "the spec is wrong" rather than
    /// "your check is wrong". Pin the distinction — parity with the
    /// point-classifier's
    /// [`evaluate_point_require_tag_returns_unknown_on_unknown_condition_suffix`]
    /// pin.
    #[test]
    fn evaluate_ephemeral_require_tag_returns_unknown_on_unknown_condition_suffix() {
        let spec = ephemeral_fixture();
        for garbage in [
            "condition-",
            "condition-jobAttested",
            "condition-CLOSEDLOOPAUTH",
            "condition-typo",
        ] {
            assert_eq!(
                evaluate_ephemeral_require_tag(&spec, garbage),
                Err(UnknownRequireTag),
                "unknown suffix in {garbage:?} must classify as UnknownRequireTag",
            );
        }
    }

    /// UNION pin — the ephemeral `condition-<kind>` prefix family
    /// unions `preconditions ∪ postconditions` (via the peer method
    /// [`tatara_process::ephemeral::EphemeralSpec::has_condition_kind`])
    /// so a kind that appears on preconditions ONLY resolves through
    /// the same tag as one on postconditions. A regression that
    /// dropped either arm of the OR (probing only one side of the
    /// union) silently reclassifies pre-only or post-only boundary
    /// predicates as absent. Byte-for-byte peer of
    /// [`evaluate_point_require_tag_unions_pre_and_post_conditions_for_condition_prefix`]
    /// on the point surface — the two-surface symmetry is what lets
    /// operators author identical `condition-<kind>` semantics under
    /// either `:domain point` or `:domain ephemeral` slot without a
    /// per-surface behavioral gotcha. Also cross-checks the semantic
    /// split against the fixed `closed-loop-auth` arm (post-only) —
    /// a spec whose ClosedLoopAuth predicate lives on preconditions
    /// satisfies `condition-ClosedLoopAuth` but NOT `closed-loop-auth`,
    /// so the two coexisting tags publish distinct answers.
    #[test]
    fn evaluate_ephemeral_require_tag_unions_pre_and_post_conditions_for_condition_prefix() {
        let mut spec = ephemeral_fixture();
        spec.preconditions.push(Condition {
            kind: ConditionKind::KustomizationHealthy,
            params: serde_json::Value::Null,
        });
        spec.postconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: serde_json::Value::Null,
        });
        assert_eq!(
            evaluate_ephemeral_require_tag(&spec, "condition-KustomizationHealthy"),
            Ok(true),
            "pre-only kind must resolve through the union",
        );
        assert_eq!(
            evaluate_ephemeral_require_tag(&spec, "condition-ClosedLoopAuth"),
            Ok(true),
            "post-only kind must resolve through the union",
        );
        assert_eq!(
            evaluate_ephemeral_require_tag(&spec, "condition-PromQL"),
            Ok(false),
            "an absent kind must return false even with populated halves",
        );

        // SEMANTIC-SPLIT cross-check: the two coexisting ClosedLoopAuth
        // tags publish distinct answers on a pre-only ClosedLoopAuth
        // spec. `condition-ClosedLoopAuth` (union) reads Ok(true);
        // `closed-loop-auth` (post-only) reads Ok(false). A regression
        // that collapsed the two tags into a single answer fails HERE.
        let mut pre_only = ephemeral_fixture();
        pre_only.preconditions.push(Condition {
            kind: ConditionKind::ClosedLoopAuth,
            params: serde_json::Value::Null,
        });
        assert_eq!(
            evaluate_ephemeral_require_tag(&pre_only, "condition-ClosedLoopAuth"),
            Ok(true),
            "pre-only ClosedLoopAuth must satisfy the union tag",
        );
        assert_eq!(
            evaluate_ephemeral_require_tag(&pre_only, "closed-loop-auth"),
            Ok(false),
            "pre-only ClosedLoopAuth must NOT satisfy the post-only tag",
        );
    }

    // ── EphemeralSpec::has_condition_kind substrate pins ─────────────
    //
    // Fail-before-pass-after granularity:
    // `EphemeralSpec::has_condition_kind` did not exist before this
    // commit — the (preconditions ∪ postconditions
    // .iter().any(|c| c.kind == K)) union-probe shape lived at ONE
    // struct-level site (`Boundary::has_condition_kind` on the point
    // surface). The lift adds the peer inherent method on the
    // [`EphemeralSpec`] surface so both struct-level union callers
    // compose against the SAME slice-level substrate primitive
    // `ConditionSliceExt::has_kind` in lockstep.

    fn ephemeral_condition(kind: ConditionKind) -> Condition {
        Condition {
            kind,
            params: serde_json::json!({}),
        }
    }

    /// EMPTY-SPEC pin — a default ephemeral fixture (empty
    /// preconditions, empty postconditions) returns `false` for EVERY
    /// [`ConditionKind`]. Sweep `ConditionKind::ALL` so a new variant
    /// added without a matching arm in the presence probe surfaces at
    /// rustc's exhaustiveness gate on the ALL literal (arity forced by
    /// `[Self; 8]`) rather than as a silent false-positive at every
    /// downstream `condition-<kind>` ephemeral require-tag callsite.
    /// Byte-for-byte peer of
    /// `has_condition_kind_returns_false_on_empty_boundary_for_every_kind`
    /// on the [`tatara_process::boundary::Boundary`] surface.
    #[test]
    fn ephemeral_has_condition_kind_returns_false_on_empty_spec_for_every_kind() {
        let spec = ephemeral_fixture();
        for kind in ConditionKind::ALL {
            assert!(
                !spec.has_condition_kind(kind),
                "empty ephemeral spec must return false for {kind:?}",
            );
        }
    }

    /// POSTCONDITION-only pin — an ephemeral spec that carries the
    /// kind on ONLY postconditions returns `true` for that kind,
    /// `false` for every other variant. Sweep the ALL × ALL cross
    /// so a regression that hard-coded the arm to a single kind or
    /// probed the wrong slot fails HERE at the substrate primitive.
    #[test]
    fn ephemeral_has_condition_kind_reads_postconditions_per_kind() {
        for populated in ConditionKind::ALL {
            let mut spec = ephemeral_fixture();
            spec.postconditions.push(ephemeral_condition(populated));
            for query in ConditionKind::ALL {
                let expected = query == populated;
                assert_eq!(
                    spec.has_condition_kind(query),
                    expected,
                    "postcondition populated={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// PRECONDITION-only pin — mirrors the postcondition sweep on the
    /// other half of the union. Locks the union semantics on both
    /// halves separately so a regression that dropped the
    /// pre-condition side of the OR fails here even though the
    /// postcondition-side pin above passes.
    #[test]
    fn ephemeral_has_condition_kind_reads_preconditions_per_kind() {
        for populated in ConditionKind::ALL {
            let mut spec = ephemeral_fixture();
            spec.preconditions.push(ephemeral_condition(populated));
            for query in ConditionKind::ALL {
                let expected = query == populated;
                assert_eq!(
                    spec.has_condition_kind(query),
                    expected,
                    "precondition populated={populated:?}: query {query:?} drifted",
                );
            }
        }
    }

    /// UNION pin — a kind that appears on preconditions returns
    /// `true` even when postconditions carries a DIFFERENT kind, and
    /// vice versa. Pins the OR-composition of the two halves so a
    /// regression that collapsed the union to an intersection (AND)
    /// silently reclassifies pre-only or post-only kinds as absent.
    /// Byte-for-byte peer of
    /// `has_condition_kind_unions_pre_and_post_condition_arms` on the
    /// [`tatara_process::boundary::Boundary`] surface.
    #[test]
    fn ephemeral_has_condition_kind_unions_pre_and_post_condition_arms() {
        let mut spec = ephemeral_fixture();
        spec.preconditions
            .push(ephemeral_condition(ConditionKind::KustomizationHealthy));
        spec.postconditions
            .push(ephemeral_condition(ConditionKind::ClosedLoopAuth));
        assert!(
            spec.has_condition_kind(ConditionKind::KustomizationHealthy),
            "pre-only kind must resolve through the union",
        );
        assert!(
            spec.has_condition_kind(ConditionKind::ClosedLoopAuth),
            "post-only kind must resolve through the union",
        );
        assert!(
            !spec.has_condition_kind(ConditionKind::PromQL),
            "an absent kind must return false even with populated halves",
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
