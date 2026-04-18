# tatara-lisp — design notes + reuse boundary

## Why another S-expression type?

pleme-io already has S-expression machinery, but for different purposes. Before
adding **tatara-lisp's `Sexp`** we audited:

| Existing | Where | Purpose | Macros? | Overlaps with us? |
|---|---|---|---|---|
| `iac_forge::sexpr::SExpr` | `iac-forge/src/sexpr.rs:95` | Canonical interchange format for IR: BLAKE3 attestation, render cache, policy evaluation, sexpr diff | No | **Different layer** |
| `arch_synthesizer::Synthesizer` trait | `arch-synthesizer/src/traits.rs:11` | Abstract morphism `Input → Ast → Output`; each backend owns its AST | n/a | **Different level** |
| `ruby_synthesizer::RubyNode` | `ruby-synthesizer/src/ast.rs` | Ruby-language AST | No | No |
| Per-forge IRs (terraform, pulumi, ansible, crossplane) | various | Language-specific render targets | No | No |

tatara-lisp fills a specific gap none of the above occupies:

> **A homoiconic authoring AST** — humans author `defpoint`, `defcheck`,
> `defmacro` etc. as S-expressions; the reader preserves `Quote`, `Quasiquote`,
> `Unquote`, `UnquoteSplice` so macros can rewrite templates before the typed
> compile step.

The four homoiconic variants are deliberately absent from `iac_forge::SExpr` —
they'd corrupt its "canonical form" invariant used for attestation hashes.

## The three S-expression layers

```
┌────────────────────────────────────────────────────────────────┐
│  Authoring    tatara_lisp::Sexp      (with macros)             │
│  ───────      (defpoint …)(defmacro…)(defcheck …)              │
│               ↓ read · macroexpand · compile                   │
├────────────────────────────────────────────────────────────────┤
│  Typed        ProcessSpec, Check, etc.   (Rust sum types)      │
│  ─────        exhaustive match, no reflection                  │
│               ↓ serialize · hash · attest                      │
├────────────────────────────────────────────────────────────────┤
│  Canonical    iac_forge::sexpr::SExpr   (no macro variants)    │
│  ─────────    BLAKE3 attestation · render cache · policy eval  │
└────────────────────────────────────────────────────────────────┘
```

The conversion direction is one-way: **tatara-lisp → canonical** is lossy
(homoiconic variants encode as `(quote …)`, `(quasiquote …)` etc. — structure
preserved, macro-ness lost). Canonical → tatara-lisp is trivial.

## Interop

`cargo run -p tatara-lisp --features iac-forge` enables `src/interop.rs`, which
provides `From<tatara_lisp::Sexp> for iac_forge::sexpr::SExpr`. This lets tatara's
attestation pipeline plug into the same BLAKE3 canonical form that iac-forge
uses for IR render attestation. Off-by-default so tatara-lisp stays lean when
the interop isn't needed.

## How far can Lisp go as Rust's complementary language?

The ambition, articulated as layered commitments (each pays a cost, enables a
tier of functionality):

### Tier 0 — Authoring surface (shipped)
- Reader + AST + macroexpander = ~600 LOC Rust
- **Use**: `defpoint` → `ProcessSpec`, `checks.lisp` → typed coherence checks
- **Ceiling**: structured config with composable templates

### Tier 1 — Multi-domain authoring (natural next step)
- Same reader + macroexpander, new top-level forms:
  - `(defcheck …)` — already demonstrated via `checks.lisp`
  - `(defmonitor …)` — Datadog/Prometheus monitor templates → typed monitor
  - `(defmigration …)` — DB migration declarations
  - `(defcompliance-suite …)` — kensa binding bundles
  - `(defflow …)` — FluxCD Kustomization composition
- Each domain = one typed Rust compiler + one keyword set
- **Cost**: ~100–200 LOC per domain; no new Lisp machinery
- **Ceiling**: declarative surface for every tatara sub-system, each with its
  own proven macro library

### Tier 2 — Typed query DSL (~300 LOC)
- Tiny evaluator over a fixed set of operators — `let`, `match`, `if`,
  list/map/set literals, bound-name lookup
- No user-defined functions, no closures, no recursion
- **Use**: `(find-processes :where (and (phase Attested) (compliance fedramp-moderate)))`
  becomes a typed query executed against the controller cache
- Operator REPL: `kubectl tatara-query "(…)"` evaluates at the reconciler
- **Cost**: evaluator + typed primitive set
- **Ceiling**: term-rewriting engine useful for queries, routing, rule engines

### Tier 3 — Build-time Rust codegen (~500 LOC)
- `tatara-lispc --emit rust` — compiles certain forms to Rust source
- Useful for: typed MCP tool stubs from Lisp definitions, CRD status-subresource
  wrappers, boilerplate elimination
- Analogous to Rust proc macros, but user-extensible and textual
- **Cost**: code generation passes + cargo integration
- **Ceiling**: user-extensible Rust metaprogramming without modifying `rustc`

### Tier 4 — Full evaluator (~2000 LOC + safety work)
- Closures, recursion, tail-call handling, error values
- Lisp becomes a scripting alternative to Rhai/soushi
- **Cost**: substantial, and we'd drift toward a real language runtime
- **Ceiling**: general-purpose scripting; probably **don't go here** — Tier 2
  covers the observability/query need, and Tier 3 covers code generation

### The sweet spot: Tiers 0 + 1 + 2

Three tiers, each self-contained, each composable with Rust's type system:
- Tier 0: declarative surface (here today)
- Tier 1: declarative breadth — same machinery, more domains
- Tier 2: typed queries — enables runtime manipulation without full evaluator

This gives us Lisp as:
- **Rust's macro language** — Tier 0+1 replaces much of what `#[derive]` and
  `build.rs` would otherwise do
- **Rust's runtime manipulation language** — Tier 2 handles operator-facing
  queries and rule engines, without the runtime-safety burden of Tier 4

**Anti-goals:** a full Lisp evaluator, closure capture, garbage collection,
hygiene/gensym. Each of those is a large commitment that takes us further from
the "declarative surface on top of typed Rust" model. Keep Lisp as config,
keep Rust as engine.

## Crate boundaries

| Crate | Depends on | Consumers |
|---|---|---|
| `tatara-lisp` | `tatara-process` (types for compile target), optionally `iac-forge` (canonical sexpr) | `tatara-reconciler` (tatara-check, tatara-lispc), future `tatara-mcp` |
| `tatara-process` | `tatara-core` (domain types), `kube` | `tatara-lisp` (compile target), `tatara-reconciler`, `tatara-lattice` |
| `iac-forge` | `arch-synthesizer` | tatara-lisp (optional), tatara-process (optional for attestation) |

No cycles. Each crate's dependency direction honors the layer diagram above.
