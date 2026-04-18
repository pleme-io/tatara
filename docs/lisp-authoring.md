# Lisp Authoring Surface

tatara-lisp is the **declarative surface** for every typed domain in tatara.
Rust owns types, invariants, reconciliation, and attestation. Lisp owns
authoring, macro composition, and runtime IR rewriting. Together: a
Rust-bordered, Lisp-mutable, self-optimizing environment.

## The four authoring surfaces (all compile to the same typed value)

```
YAML   (kubectl)             (serde_yaml → typed)
Nix    (HM/NixOS module)     (lib.generators.toYAML → YAML → typed)
Rust   (programmatic)        (typed from the start)
Lisp   (homoiconic + macros) (read → macroexpand → compile_from_args → typed)
                                       ↓
                          same typed value in every case
```

For every surface: Rust validates the final value. Nothing reaches the
reconciler / attestation / boundary evaluator unless it type-checks.

## Reader

S-expression syntax with the usual primitives + quasi-quote:

```lisp
; comments
42 3.14 "string" :keyword #t #f   ; atoms
(a b c)                           ; list
'(a b c)                          ; quote
`(a ,b ,@rest)                    ; quasi-quote / unquote / unquote-splice
```

Implemented in `tatara-lisp/src/reader.rs` — tokenizer + parser → `Vec<Sexp>`.

## `Sexp` AST

```rust
pub enum Sexp {
    Nil,
    Atom(Atom),          // Symbol | Keyword | Str | Int | Float | Bool
    List(Vec<Sexp>),
    Quote(Box<Sexp>),
    Quasiquote(Box<Sexp>),
    Unquote(Box<Sexp>),
    UnquoteSplice(Box<Sexp>),
}
```

Homoiconic — code is data, data is code. Unlike `iac_forge::sexpr::SExpr` (which
is canonical/non-homoiconic for attestation), this carries the quote variants
so macros can rewrite freely.

## Macros

User-defined via `defmacro` / `defpoint-template` / `defcheck`:

```lisp
(defmacro observability-fedramp (name)
  `(defpoint ,name
     :classification (:point-type Gate :substrate Observability)
     :compliance (:baseline "fedramp-moderate"
                  :bindings ((:framework "nist-800-53"
                              :control-id "SC-7"
                              :phase AtBoundary)))))

(observability-fedramp grafana-stack)
; macroexpands to:
; (defpoint grafana-stack
;   :classification (:point-type Gate :substrate Observability)
;   :compliance (:baseline "fedramp-moderate"
;                :bindings ((:framework "nist-800-53"
;                            :control-id "SC-7"
;                            :phase AtBoundary))))
```

Semantics (v0):
- Quasi-quote + `,x` (unquote) + `,@x` (splice) rewrite the template
- `&rest name` collects remaining args into a list
- Nested macro expansion runs to a fixed point
- No evaluator, no closures, no recursion — pure term rewriting

Implemented in `tatara-lisp/src/macro_expand.rs`.

## `#[derive(TataraDomain)]` — the one-liner

Any Rust struct with `serde::Deserialize` gains a Lisp authoring surface:

```rust
use tatara_lisp::DeriveTataraDomain;

#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug)]
#[tatara(keyword = "defmonitor")]
pub struct MonitorSpec {
    pub name: String,
    pub query: String,
    pub threshold: f64,
    pub window_seconds: Option<i64>,
    #[serde(default)]
    pub tags: Vec<String>,
}
```

```lisp
(defmonitor :name "prom-up"
            :query "up{job='prometheus'}"
            :threshold 0.99
            :window-seconds 300
            :tags ("prod" "observability"))
```

The derive generates a `TataraDomain` impl whose `compile_from_args` extracts
each field via a type-directed strategy. Snake-case field names become
kebab-case Lisp keywords (`window_seconds` → `:window-seconds`).

### Supported field types

**First-class:** `String` / `bool` / `i{32,64}` / `u{32,64,size}` / `f{32,64}` +
their `Option<_>` and (for String) `Vec<_>`.

**Universal fallthrough** (via `sexp_to_json` + `serde_json::from_value`): any
type that implements `serde::Deserialize`:
- Enums — bare Lisp symbol (`Gate`, `Critical`, …) → JSON string → enum
- Nested structs — Lisp kwargs list → JSON object → struct
- `Vec<Nested>` — Lisp list of kwargs lists → JSON array → `Vec<T>`
- `Option<T>` — missing keyword → `None`

### `#[serde(default)]`

Fields with `#[serde(default)]` are optional: missing keyword → `T::default()`.
The derive inspects the attribute list; ~1 LoC per field is all the user pays
for optional fields.

## Compilation entry points

```rust
// Top-level forms like (defmonitor :name … :query …) → Vec<MonitorSpec>.
let monitors = tatara_lisp::compile_typed::<MonitorSpec>(src)?;

// Top-level forms like (defpoint NAME :k v …) → Vec<NamedDefinition<ProcessSpec>>.
// NamedDefinition carries both the positional name and the compiled spec.
let processes = tatara_lisp::compile_named::<ProcessSpec>(src)?;

// ProcessSpec-specific convenience wrapper in tatara-process:
let defs = tatara_process::compile_source(src)?;
```

Both `compile_typed` and `compile_named` run the macroexpander first, so
user-defined macros expand before typed compilation.

## Registry dispatch — runtime typed lookup

For domains not known at compile time (e.g., when `tatara-check` walks
`checks.lisp` with arbitrary keywords), the registry provides typed dispatch:

```rust
tatara_lisp::domain::register::<MonitorSpec>();
tatara_lisp::domain::register::<AlertPolicySpec>();

if let Some(handler) = tatara_lisp::domain::lookup("defmonitor") {
    let json = (handler.compile)(args)?;   // returns serde_json::Value
}
```

The `tatara-check` binary calls `tatara_domains::register_all()` at startup;
any `(defX …)` form in `checks.lisp` whose keyword matches a registered type
is compiled and reported as a passing check.

## `rewrite_typed` — self-optimization

The Rust-bordered, Lisp-mutable primitive:

```rust
pub fn rewrite_typed<T, F>(input: T, rewrite: F) -> Result<T>
where
    T: TataraDomain + serde::Serialize,
    F: FnOnce(Sexp) -> Result<Sexp>,
```

Flow:
1. Serialize typed `T` → `serde_json::Value` → `Sexp` (via `json_to_sexp`).
2. Apply the user-supplied rewriter — arbitrary Lisp-level mutation.
3. Re-enter `T::compile_from_args` on the rewritten Sexp — typed re-validation.
4. Return typed `T` or error.

Any rewrite that type-checks is safe by construction. Rust's type system is the
floor; Lisp's rewriting is free within it. Covered by a test in
`tatara-lisp/src/domain.rs::tests::rewrite_typed_end_to_end`.

## Workspace coherence via Lisp — `checks.lisp`

`cargo run --bin tatara-check -p tatara-reconciler` reads `checks.lisp` at the
workspace root and dispatches each form through a typed Rust executor:

```lisp
;; Built-in primitives:
(crd-in-sync Process      "chart/tatara-reconciler/crds/processes.yaml")
(yaml-parses              "chart/tatara-reconciler/Chart.yaml")
(yaml-parses-as Process   "examples/process/observability-stack.yaml")
(lisp-compiles            "examples/process/observability-stack.lisp"
                          :min-definitions 1
                          :requires (intent-nix depends-on boundary-post compliance))
(file-contains            "examples/process/observability-stack.nix"
                          :strings ("services.tatara.processes" "pointType"))

;; User-defined check-templates via defcheck:
(defcheck process-example-triple (yaml-path lisp-path nix-path)
  `(do (yaml-parses-as Process ,yaml-path)
       (lisp-compiles ,lisp-path
                      :min-definitions 1
                      :requires (intent-nix depends-on boundary-post compliance))
       (file-contains ,nix-path :strings ("services.tatara.processes"))))

(process-example-triple "observability-stack.yaml"
                        "observability-stack.lisp"
                        "observability-stack.nix")

;; Registry fallthrough — any derived TataraDomain auto-dispatches:
(defmonitor :name "prometheus-up" :query "up{…}" :threshold 0.99 …)
(defalertpolicy :name "prod-outage" :severity Critical …)
```

Zero shell. The CI-replacement surface when you leverage `nix run apps` + fleet
instead of GitHub Actions.

## Tiered roadmap

The DESIGN.md tiers the ambition:

| Tier | Status | LOC cost |
|------|--------|----------|
| Tier 0 — reader + AST + macros | shipped | ~600 |
| Tier 1 — multi-domain authoring via `defcheck` / `defmonitor` / etc. | shipped | ~100 per new domain (just a struct + derive) |
| Tier 2 — typed query DSL (`let`/`match`/`if` + typed primitives) | future | ~300 |
| Tier 3 — build-time Rust codegen (`tatara-lispc --emit rust`) | future | ~500 |
| Tier 4 — full evaluator (closures, recursion) | **anti-goal** | ~2000+ |

See [tatara-lisp/DESIGN.md](../tatara-lisp/DESIGN.md) for the full analysis.

## Reuse boundary

Three S-expression layers coexist in pleme-io without duplication:

| Layer | Type | Where | Purpose |
|-------|------|-------|---------|
| Authoring | `tatara_lisp::Sexp` | this crate | Homoiconic, macro-capable, for humans |
| Typed | `ProcessSpec`, `MonitorSpec`, etc. | various | Exhaustive sum types, compile-time proof |
| Canonical | `iac_forge::sexpr::SExpr` | iac-forge | BLAKE3 attestation + render cache |

Feature-gated interop via `tatara-lisp --features iac-forge` provides
`From<Sexp> for iac_forge::SExpr` so tatara authoring plugs into the existing
attestation pipeline.

## Related

- [tatara-lisp/README.md](../tatara-lisp/README.md) — user-facing crate overview
- [tatara-lisp/DESIGN.md](../tatara-lisp/DESIGN.md) — design notes + reuse boundary + tiered roadmap
- [tatara-lisp-derive/README.md](../tatara-lisp-derive/README.md) — proc macro details
- [tatara-domains/README.md](../tatara-domains/README.md) — reference demo domains + registration pattern
- [k8s-as-processes.md](k8s-as-processes.md) — the Process CRD that uses the derive at full scale
