//! Example typed domains authored as Lisp forms.
//!
//! Each type in this crate derives `TataraDomain`, which auto-generates the
//! Lisp → Rust compile function. Binaries that want to accept these domains
//! via Lisp call `tatara_domains::register_all()` at startup, after which
//! `tatara_lisp::domain::lookup(keyword)` resolves any registered form.

use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

pub mod prelude {
    pub use super::{AlertPolicySpec, MonitorSpec, NotifySpec, Severity};
}

// ── basic demo (String, numbers, bool, Option, Vec<String>) ──────

/// A Prometheus-style alert monitor — the canonical tiny demo domain.
///
/// ```lisp
/// (defmonitor :name "prom-up"
///             :query "up{job='prometheus'}"
///             :threshold 0.99
///             :window-seconds 300
///             :tags ("prod" "observability")
///             :enabled #t)
/// ```
#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defmonitor")]
pub struct MonitorSpec {
    pub name: String,
    pub query: String,
    pub threshold: f64,
    pub window_seconds: Option<i64>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub enabled: Option<bool>,
}

/// A notification config — proves multiple types coexist in the registry.
#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defnotify")]
pub struct NotifySpec {
    pub name: String,
    pub channel: String,
    pub target: String,
    pub severity: Option<String>,
}

// ── richer demo: enum + nested struct + Vec<struct> ──────────────

/// A standalone enum — proves the derive's serde-Deserialize fallthrough.
/// In Lisp this appears as a bare symbol: `:severity High`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Severity {
    Info,
    Warning,
    Critical,
    Page,
}

/// An escalation step — nested struct referenced inside `AlertPolicySpec`.
/// In Lisp: `(:notify-ref "oncall" :wait-minutes 5 :severity Page)`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EscalationStep {
    pub notify_ref: String,
    pub wait_minutes: Option<i64>,
    pub severity: Severity,
}

/// Composite alerting policy — exercises every derive kind at once:
///   - `String`, `f64`, `Option<f64>`, `Option<bool>`  (basic kinds)
///   - `Severity` enum                                  (Deserialize fallthrough)
///   - `Option<String>`, `Vec<String>`                  (basic containers)
///   - `Vec<EscalationStep>`                            (Vec-of-nested fallthrough)
///
/// ```lisp
/// (defalertpolicy
///   :name "prod-outage"
///   :monitor-ref "prometheus-up"
///   :severity Critical
///   :mute-minutes 30
///   :mute-on-deploy #t
///   :labels ("prod" "pager")
///   :escalations (
///     (:notify-ref "oncall" :wait-minutes 0 :severity Page)
///     (:notify-ref "slack-alerts" :wait-minutes 5 :severity Warning)))
/// ```
#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defalertpolicy")]
pub struct AlertPolicySpec {
    pub name: String,
    pub monitor_ref: String,
    pub severity: Severity,
    pub mute_minutes: Option<f64>,
    pub mute_on_deploy: Option<bool>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub escalations: Vec<EscalationStep>,
}

/// Register every domain in this crate with the global dispatcher.
/// Call once per binary, typically near the top of `main`.
pub fn register_all() {
    tatara_lisp::domain::register::<MonitorSpec>();
    tatara_lisp::domain::register::<NotifySpec>();
    tatara_lisp::domain::register::<AlertPolicySpec>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_lisp::{domain::TataraDomain, read};

    #[test]
    fn monitor_round_trips() {
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
        let m = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(m.name, "prom-up");
        assert_eq!(m.threshold, 0.99);
        assert_eq!(m.window_seconds, Some(300));
        assert_eq!(
            m.tags,
            vec!["prod".to_string(), "observability".to_string()]
        );
        assert_eq!(m.enabled, Some(true));
    }

    #[test]
    fn notify_minimal() {
        let forms =
            read(r##"(defnotify :name "oncall" :channel "slack" :target "#alerts")"##).unwrap();
        let n = NotifySpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(n.name, "oncall");
        assert_eq!(n.channel, "slack");
        assert_eq!(n.target, "#alerts");
        assert!(n.severity.is_none());
    }

    #[test]
    fn alert_policy_with_enum_and_nested_vec() {
        // Exercises: bare-symbol enum, Vec of nested structs, Option, Vec<String>.
        let forms = read(
            r#"(defalertpolicy
                  :name "prod-outage"
                  :monitor-ref "prometheus-up"
                  :severity Critical
                  :mute-minutes 30.0
                  :mute-on-deploy #t
                  :labels ("prod" "pager")
                  :escalations (
                    (:notify-ref "oncall" :wait-minutes 0 :severity Page)
                    (:notify-ref "slack-alerts" :wait-minutes 5 :severity Warning)))"#,
        )
        .unwrap();
        let p = AlertPolicySpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(p.name, "prod-outage");
        assert_eq!(p.severity, Severity::Critical);
        assert_eq!(p.mute_minutes, Some(30.0));
        assert_eq!(p.mute_on_deploy, Some(true));
        assert_eq!(p.labels, vec!["prod".to_string(), "pager".to_string()]);
        assert_eq!(p.escalations.len(), 2);
        assert_eq!(p.escalations[0].notify_ref, "oncall");
        assert_eq!(p.escalations[0].severity, Severity::Page);
        assert_eq!(p.escalations[1].wait_minutes, Some(5));
        assert_eq!(p.escalations[1].severity, Severity::Warning);
    }

    #[test]
    fn alert_policy_defaults() {
        let forms = read(
            r#"(defalertpolicy
                  :name "basic"
                  :monitor-ref "x"
                  :severity Info)"#,
        )
        .unwrap();
        let p = AlertPolicySpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(p.severity, Severity::Info);
        assert!(p.mute_minutes.is_none());
        assert!(p.labels.is_empty());
        assert!(p.escalations.is_empty());
    }

    #[test]
    fn register_all_populates_registry() {
        register_all();
        let kws = tatara_lisp::domain::registered_keywords();
        assert!(kws.contains(&"defmonitor"));
        assert!(kws.contains(&"defnotify"));
        assert!(kws.contains(&"defalertpolicy"));
    }

    // ── Error paths (derive-generated error handling) ────────────────
    //
    // compile_from_sexp returns Result. Before these tests, only the
    // happy paths were exercised — a regression in the derive's
    // "missing-required-field" / "unknown-variant" handling could
    // accept malformed Lisp silently.

    #[test]
    fn monitor_rejects_missing_required_name() {
        // `name` is required (no #[serde(default)]). Omitting it must
        // produce an error, not a default-filled MonitorSpec.
        let forms = read(r#"(defmonitor :query "up{job='x'}" :threshold 0.5)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            format!("{err:?}").to_lowercase().contains("name")
                || format!("{err:?}").to_lowercase().contains("missing")
                || format!("{err:?}").to_lowercase().contains("required"),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn monitor_rejects_missing_required_query() {
        let forms = read(r#"(defmonitor :name "x" :threshold 0.5)"#).unwrap();
        assert!(MonitorSpec::compile_from_sexp(&forms[0]).is_err());
    }

    #[test]
    fn monitor_rejects_missing_required_threshold() {
        let forms = read(r#"(defmonitor :name "x" :query "y")"#).unwrap();
        assert!(MonitorSpec::compile_from_sexp(&forms[0]).is_err());
    }

    #[test]
    fn notify_rejects_missing_required_channel() {
        let forms = read(r##"(defnotify :name "oncall" :target "#alerts")"##).unwrap();
        assert!(NotifySpec::compile_from_sexp(&forms[0]).is_err());
    }

    #[test]
    fn alert_policy_rejects_unknown_severity() {
        // Severity has variants Info / Warning / Critical / Page. "Fatal"
        // isn't one — the serde-Deserialize fallthrough in the derive
        // must reject it.
        let forms = read(r#"(defalertpolicy :name "x" :monitor-ref "m" :severity Fatal)"#).unwrap();
        assert!(AlertPolicySpec::compile_from_sexp(&forms[0]).is_err());
    }

    #[test]
    fn monitor_rejects_typoed_keyword() {
        // Typed-entry invariant (THEORY.md §II.1.1) — a misspelled keyword
        // (`:tthreshold` instead of `:threshold`) must error, not parse
        // silently with `threshold` defaulted/missing.
        let forms =
            read(r#"(defmonitor :name "x" :query "q" :threshold 0.5 :tthreshold 0.99)"#).unwrap();
        let err = MonitorSpec::compile_from_sexp(&forms[0]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("tthreshold"), "must name the typo: {msg}");
        assert!(
            msg.contains("unknown keyword"),
            "must label the failure: {msg}"
        );
    }

    #[test]
    fn alert_policy_rejects_typoed_keyword() {
        // Same strictness applies to every derive site, including ones with
        // enum + nested-Vec fields.
        let forms = read(
            r#"(defalertpolicy :name "x" :monitor-ref "m" :severity Info :wrong-field "oops")"#,
        )
        .unwrap();
        let err = AlertPolicySpec::compile_from_sexp(&forms[0]).unwrap_err();
        assert!(
            format!("{err}").contains("wrong-field"),
            "must name the offending keyword, got: {err}"
        );
    }

    #[test]
    fn alert_policy_rejects_integer_in_severity_slot() {
        // serde's enum Deserialize accepts both bare symbols and
        // strings for unit variants, so both `Critical` and "Critical"
        // work. What it MUST reject is non-string/non-symbol payloads
        // like integers, which can't identify a variant.
        let forms = read(r#"(defalertpolicy :name "x" :monitor-ref "m" :severity 42)"#).unwrap();
        assert!(AlertPolicySpec::compile_from_sexp(&forms[0]).is_err());
    }

    // ── Default / optional behaviour ──────────────────────────────────

    #[test]
    fn monitor_defaults_tags_to_empty_and_window_enabled_to_none() {
        // `tags` has #[serde(default)] → empty Vec. Option fields are
        // None by default. Pin the exact empty/None payload so a future
        // derive refactor that starts inserting Some("") or vec![""]
        // (a plausible regression in deserialization fallback)
        // surfaces here.
        let forms = read(r#"(defmonitor :name "x" :query "y" :threshold 0.1)"#).unwrap();
        let m = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(m.name, "x");
        assert_eq!(m.query, "y");
        assert!(m.tags.is_empty());
        assert!(m.window_seconds.is_none());
        assert!(m.enabled.is_none());
    }

    #[test]
    fn monitor_explicit_empty_tags_list_parses() {
        // `:tags ()` must parse to an empty Vec, not error out on the
        // empty list.
        let forms = read(r#"(defmonitor :name "x" :query "y" :threshold 0.5 :tags ())"#).unwrap();
        let m = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
        assert!(m.tags.is_empty());
    }

    #[test]
    fn alert_policy_empty_escalations_parses() {
        // Explicit `:escalations ()` parses to empty Vec — distinct
        // from omission, which also yields empty via
        // #[serde(default)] — both paths must reach the same shape.
        let forms = read(
            r#"(defalertpolicy
                  :name "p"
                  :monitor-ref "m"
                  :severity Info
                  :escalations ())"#,
        )
        .unwrap();
        let p = AlertPolicySpec::compile_from_sexp(&forms[0]).unwrap();
        assert!(p.escalations.is_empty());
    }

    // ── camelCase ↔ kebab-case conversion ─────────────────────────────

    #[test]
    fn kebab_case_keywords_map_to_snake_case_rust_fields() {
        // Every spec in this crate uses `#[serde(rename_all =
        // "camelCase")]`, and the Lisp convention is kebab-case. The
        // derive normalizes kebab → camelCase before serde
        // deserializes. Pin both translations via fields whose Rust
        // name differs from their keyword form.
        let forms = read(
            r#"(defmonitor
                  :name "x"
                  :query "q"
                  :threshold 0.0
                  :window-seconds 42)"#,
        )
        .unwrap();
        let m = MonitorSpec::compile_from_sexp(&forms[0]).unwrap();
        // `:window-seconds` → struct field `window_seconds`
        assert_eq!(m.window_seconds, Some(42));
    }

    #[test]
    fn alert_policy_nested_kebab_case_works() {
        // Nested EscalationStep has `:notify-ref` and `:wait-minutes`
        // — kebab-case conversion must propagate into the Vec<Nested>
        // fallthrough code path.
        let forms = read(
            r#"(defalertpolicy
                  :name "p"
                  :monitor-ref "m"
                  :severity Info
                  :escalations (
                    (:notify-ref "slack" :wait-minutes 10 :severity Warning)))"#,
        )
        .unwrap();
        let p = AlertPolicySpec::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(p.escalations.len(), 1);
        assert_eq!(p.escalations[0].notify_ref, "slack");
        assert_eq!(p.escalations[0].wait_minutes, Some(10));
    }

    // ── Registry ──────────────────────────────────────────────────────

    #[test]
    fn register_all_is_idempotent() {
        // register_all() may be called in each test binary; the
        // registry must tolerate repeat inserts without blowing up or
        // producing duplicate keywords.
        register_all();
        register_all();
        let kws = tatara_lisp::domain::registered_keywords();
        let monitor_count = kws.iter().filter(|k| **k == "defmonitor").count();
        assert_eq!(monitor_count, 1, "registry should dedupe re-registrations");
    }

    #[test]
    fn prelude_re_exports_the_documented_types() {
        // The prelude promises these four names. A rename upstream
        // would break every downstream binary that wrote
        // `use tatara_domains::prelude::*;` — pin the names here by
        // constructing a minimum value of each.
        let _sev: super::prelude::Severity = super::prelude::Severity::Info;
        let _m: Option<super::prelude::MonitorSpec> = None;
        let _n: Option<super::prelude::NotifySpec> = None;
        let _p: Option<super::prelude::AlertPolicySpec> = None;
    }

    #[test]
    fn rewrite_typed_end_to_end() {
        use tatara_lisp::ast::{Atom, Sexp};
        use tatara_lisp::domain::rewrite_typed;

        let m0 = MonitorSpec {
            name: "prom-up".into(),
            query: "up{j='x'}".into(),
            threshold: 0.95,
            window_seconds: Some(60),
            tags: vec!["prod".into()],
            enabled: Some(true),
        };

        // Lisp-level rewrite: bump threshold by looking at the kwargs list.
        let m1 = rewrite_typed(m0, |sexp| {
            let mut items = match sexp {
                Sexp::List(xs) => xs,
                other => {
                    return Err(tatara_lisp::LispError::Compile {
                        form: "rewrite".into(),
                        message: format!("expected kwargs list, got {other}"),
                    })
                }
            };
            // Walk keyword/value pairs; bump :threshold.
            let mut i = 0;
            while i + 1 < items.len() {
                if items[i].as_keyword() == Some("threshold") {
                    if let Sexp::Atom(Atom::Float(n)) = &items[i + 1] {
                        items[i + 1] = Sexp::float(n + 0.04);
                    }
                }
                i += 2;
            }
            Ok(Sexp::List(items))
        })
        .unwrap();

        // Rust re-validated the rewritten Sexp — we know the result is a
        // well-typed MonitorSpec with the new threshold.
        assert!((m1.threshold - 0.99).abs() < 1e-9);
        assert_eq!(m1.name, "prom-up");
        assert_eq!(m1.tags, vec!["prod".to_string()]);
    }
}
