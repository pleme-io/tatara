# tatara-lisp

A small homoiconic S-expression language that serves as the **authoring
surface** and **runtime manipulation language** for every typed domain in
tatara. Rust owns types and invariants; Lisp owns declarative surface, macro
composition, and IR rewriting.

## Three things live here

1. **Reader + macroexpander** — tokenize + parse + quasi-quote rewriting
2. **`TataraDomain` trait** + registry — typed dispatch from keyword → Rust struct
3. **Generic compile helpers** — `compile_typed<T>` + `compile_named<T>` drive derive-backed compilers

## At a glance

```rust
use tatara_lisp::{compile_typed, TataraDomain};
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

#[derive(DeriveTataraDomain, serde::Serialize, serde::Deserialize)]
#[tatara(keyword = "defmonitor")]
struct MonitorSpec { name: String, query: String, threshold: f64 }

let monitors = compile_typed::<MonitorSpec>(r#"
    (defmonitor :name "prom-up" :query "up{job='prometheus'}" :threshold 0.99)
    (defmonitor :name "prom-ready" :query "prometheus_ready" :threshold 1.0)
"#)?;
assert_eq!(monitors.len(), 2);
```

## Three rendering surfaces

### `compile_typed<T>(src: &str) -> Result<Vec<T>>`

Every top-level `(T::KEYWORD :k v …)` form compiles to a `T`. Use when the
keyword carries no positional name.

### `compile_named<T>(src: &str) -> Result<Vec<NamedDefinition<T>>>`

Every top-level `(T::KEYWORD NAME :k v …)` form compiles to
`NamedDefinition<T> { name, spec }`. Use when the keyword carries a positional
identifier — e.g., `(defpoint observability-stack :class … :intent …)`.

### Registry dispatch

```rust
tatara_lisp::domain::register::<MonitorSpec>();
let handler = tatara_lisp::domain::lookup("defmonitor").unwrap();
let json = (handler.compile)(sexp_args)?;  // returns serde_json::Value
```

Use when you don't know the target type at compile time — the tatara-check
binary uses this to dispatch `checks.lisp` forms to whichever typed domain is
registered at startup.

## Macros

User-defined macros via `defmacro` / `defpoint-template` / `defcheck`:

```lisp
(defmacro observability-fedramp (name)
  `(defpoint ,name
     :classification (:point-type Gate :substrate Observability)
     :compliance (:baseline "fedramp-moderate"
                  :bindings ((:framework "nist-800-53"
                              :control-id "SC-7"
                              :phase AtBoundary)))))

(observability-fedramp grafana-stack)  ; expands before typed compile
```

Quasi-quote (`` ` ``), unquote (`,`), unquote-splice (`,@`), `&rest` params.
Nested macros expand iteratively to a fixed point. No evaluator, no closures —
just typed term rewriting.

## The `TataraDomain` trait

```rust
pub trait TataraDomain: Sized {
    const KEYWORD: &'static str;
    fn compile_from_args(args: &[Sexp]) -> Result<Self>;
    fn compile_from_sexp(form: &Sexp) -> Result<Self>;   // default impl
}
```

Apply `#[derive(TataraDomain)]` from `tatara-lisp-derive` to get the impl for
free. Every field is extracted via a type-directed strategy:

| Rust type | Lisp shape | Extractor |
|---|---|---|
| `String` | `:field "value"` | `extract_string` |
| `i64` / `i32` / `u32` / `u64` / `usize` | `:field 42` | `extract_int` + cast |
| `f64` / `f32` | `:field 0.99` | `extract_float` |
| `bool` | `:field #t` | `extract_bool` |
| `Option<basic>` | missing ⇒ None | `extract_optional_*` |
| `Vec<String>` | `:field ("a" "b")` | `extract_string_list` |
| **Anything else that's `serde::Deserialize`** | any shape serde accepts | `sexp_to_json` + `serde_json::from_value` |

The fallthrough unlocks enums (as bare symbols), nested structs (as kwargs
lists), `Vec<Nested>` (as list-of-kwargs-lists), and anything else serde can
deserialize. `#[serde(default)]` on a field makes it optional.

## `rewrite_typed` — the self-optimization primitive

```rust
pub fn rewrite_typed<T, F>(input: T, rewrite: F) -> Result<T>
where
    T: TataraDomain + serde::Serialize,
    F: FnOnce(Sexp) -> Result<Sexp>
```

Serializes a typed `T` to Sexp, applies a user-supplied Lisp-level rewriter,
re-enters `T::compile_from_args` for typed re-validation. Any rewrite that type-
checks is safe by construction — Rust is the floor, Lisp is the ceiling. Test
in `src/domain.rs::tests` bumps a monitor's `threshold` via a walker.

## Tiered roadmap

The DESIGN.md lays out the full ambition: Tier 0 (authoring — shipped), Tier 1
(multi-domain — shipped), Tier 2 (typed query DSL), Tier 3 (build-time Rust
codegen), Tier 4 anti-goal (full evaluator). See [DESIGN.md](DESIGN.md).

## Modules

| File | Purpose |
|------|---------|
| `src/ast.rs` | `Sexp` + `Atom` — the homoiconic AST |
| `src/reader.rs` | Tokenizer + parser (handles `'`, `` ` ``, `,`, `,@`, strings, comments) |
| `src/macro_expand.rs` | `Expander` — register `defmacro` / `defpoint-template` / `defcheck`, rewrite calls |
| `src/domain.rs` | `TataraDomain` trait, kwargs parsing, extractors, sexp↔json, registry, `rewrite_typed` |
| `src/compile.rs` | Generic `compile_typed<T>` + `compile_named<T>` — drives derive-backed compilers |
| `src/env.rs` | Lexical env for a future evaluator (Tier 2) |
| `src/error.rs` | `LispError` + `Result` |
| `src/interop.rs` | `From<Sexp> for iac_forge::sexpr::SExpr` (feature-gated `iac-forge`) |

## Consumers

- `tatara-process` — derives `TataraDomain` on `ProcessSpec`; provides `compile_source` + `tatara-lispc` binary
- `tatara-domains` — reference demo domains (MonitorSpec, NotifySpec, AlertPolicySpec)
- `tatara-reconciler` — `tatara-check` binary reads `checks.lisp` and dispatches via the registry

## Reuse boundary

tatara-lisp does **not** duplicate other pleme-io S-expression work:
- `iac_forge::sexpr::SExpr` — canonical serialization form (no macro variants by design)
- `arch_synthesizer::Synthesizer` trait — abstract morphism over typed domains (no shared AST)
- Per-forge IRs (RubyNode, HCL AST, etc.) — language-specific render targets

tatara-lisp fills a distinct slot: homoiconic **authoring** with macro-based
rewriting. See [DESIGN.md](DESIGN.md) for the three-layer S-expression topology.
