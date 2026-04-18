# tatara-domains

Reference typed domains that demonstrate `#[derive(TataraDomain)]` and seed the
global Lisp-dispatch registry. Deliberately tiny — this crate is the onboarding
example + the default registry payload, not a kitchen sink.

## Contents

| Domain | Purpose | Complexity |
|---|---|---|
| `MonitorSpec` | Prometheus-style alert monitor | basic (String, f64, Option, Vec<String>) |
| `NotifySpec` | Notification channel config | basic (String, Option<String>) |
| `Severity` | Enum: `Info` / `Warning` / `Critical` / `Page` | standalone enum |
| `EscalationStep` | Nested alert escalation rule | nested struct with enum |
| `AlertPolicySpec` | Composite policy: enum + `Vec<EscalationStep>` | **exercises every derive kind** |

## Authoring in Lisp

```lisp
(defmonitor :name "prom-up"
            :query "up{job='prometheus'}"
            :threshold 0.99
            :window-seconds 300
            :tags ("prod" "observability")
            :enabled #t)

(defalertpolicy
  :name "prod-outage"
  :monitor-ref "prom-up"
  :severity Critical
  :mute-minutes 30.0
  :mute-on-deploy #t
  :labels ("prod" "pager")
  :escalations (
    (:notify-ref "oncall"       :wait-minutes 0 :severity Page)
    (:notify-ref "slack-alerts" :wait-minutes 5 :severity Warning)))
```

Both compile to typed Rust values via `<Type>::compile_from_sexp`.
`AlertPolicySpec` is the proof point: **one derive, all kinds covered** —
enums via serde fallthrough, nested structs, `Vec<nested-struct-with-enum>`.

## Registering with the global dispatcher

```rust
tatara_domains::register_all();   // call once at binary startup
```

After this, `tatara_lisp::domain::lookup("defmonitor")` returns a handler that
compiles any `(defmonitor …)` form to a serde JSON object. The tatara-check
binary calls this at startup; any `defX` form in `checks.lisp` whose keyword
matches a registered type gets compiled and reported as a passing check.

## How to add a new domain

1. Create a Rust struct with `Serialize + Deserialize`.
2. Apply `#[derive(DeriveTataraDomain)]` + `#[tatara(keyword = "defX")]`.
3. Add the type to `register_all()`.
4. Author `(defX …)` forms in your `.lisp` files.

That's it. The derive handles every field type serde can handle. `#[serde(default)]`
on a field makes it optional.

## Tests

6 unit tests — one end-to-end roundtrip per domain + the `rewrite_typed` self-
optimization test on MonitorSpec (Lisp walker bumps a typed field's threshold;
Rust re-validates; returns a well-typed `MonitorSpec` with the new value).

## Relationship to tatara-process

This crate is for **example** domains — the MonitorSpec style. `tatara-process`
owns the real `Process` CRD and its derive. Together they illustrate:

- `tatara-domains::MonitorSpec` — small example showing the pattern
- `tatara-process::ProcessSpec` — the real thing, 8 fields, deeply nested
