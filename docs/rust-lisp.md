# The Rust + Lisp Pattern — pleme-io's primary architectural concept

> Rust-bordered, Lisp-mutable, self-optimizing environments.
>
> Rust owns types, invariants, memory safety, and the proof floor.
> Lisp owns declarative authoring, macro composition, and runtime IR rewriting.
> The boundary is a single proc macro.
> Every new typed domain joins the system with one line of `#[derive(TataraDomain)]`.

This is the canonical pattern for every new domain authored in pleme-io. The
rest of this document is: why it matters, what it guarantees, how to apply it,
what to avoid, and where it goes from here.

---

## Why this is THE pattern

1. **Rust's type system is the floor.** Invalid states are unrepresentable.
   Every Rust value we surface is a theorem: "the invariants declared on this
   type hold." No ad-hoc parsing. No runtime-discovered shape errors past the
   type boundary. The ground is solid.
2. **Lisp's homoiconicity is the ceiling.** S-expressions are both code and
   data, so macros can rewrite the IR freely. There is no parser step between
   authoring and AST — the AST is already the value.
3. **The boundary is a proc macro.** `#[derive(TataraDomain)]` is one line.
   Every typed Rust struct in the organization can become a Lisp-authorable
   domain immediately. Nothing to re-solve per domain.
4. **Self-bootstrapping.** A `CompilerSpec` is itself a typed Lisp domain. Lisp
   authors Lisp compilers. The base compiler is infrastructure; specialized
   compilers are generated.
5. **Lattice-composable.** Modules, overlays, options — every composition
   obeys the same lattice algebra (`meet` / `join` / `leq`). Composition is
   a proof, not a convention.
6. **Content-addressable.** BLAKE3 over canonical representations gives us
   identity-by-hash throughout the stack: store paths, attestations, module
   resolutions, cache keys. Deterministic by construction.

Everything else falls out of these six: the K8s-as-processes model, the
FluxCD-adjacent reconciler, the Nix re-expression, the compiler factories,
the typed query roadmap. They are all projections of Rust + Lisp with one
derive macro in the middle.

---

## The five invariants

1. **Typed entry.** The only way to produce a Rust value from Lisp is through
   `TataraDomain::compile_from_sexp`. Ill-typed input errors before the value
   exists.
2. **Free middle.** Inside the boundary, Lisp macros can rewrite IR
   arbitrarily. No mid-rewrite type checks; no loss of flexibility.
3. **Typed exit.** Re-entering `compile_from_args` on any rewritten Sexp
   guarantees the output is well-typed. Any rewrite that type-checks is safe
   by construction.
4. **Deterministic identity.** Every value has a BLAKE3 content hash over its
   canonical serialization. Identical inputs produce identical identities.
5. **Composition preserves proofs.** `meet` / `join` of two values inherits
   the invariants of each. No proof repetition across composites.

These five hold by construction in tatara-lisp and tatara-nix today. They
are the contract that makes the pattern safe to apply to any new domain.

---

## The architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                       AUTHORING (humans + LLMs)                       │
│                                                                       │
│   YAML        Nix            Rust          Lisp                       │
│   kubectl     HM module      builder       (defX :k v …) + macros     │
│       ↘       ↓              ↓             ↙                          │
│        ──────────── one typed value ────────────                      │
│                           ProcessSpec | MonitorSpec | Module | …      │
└──────────────────────────────────────────────────────────────────────┘
                              ↕  (single bridge)
┌──────────────────────────────────────────────────────────────────────┐
│                    THE BOUNDARY (ONE MACRO)                           │
│                                                                       │
│   #[derive(TataraDomain)]                                             │
│   #[tatara(keyword = "defX")]                                         │
│   struct XSpec { … fields with serde derives … }                      │
│                                                                       │
│   ⇒  impl TataraDomain for XSpec { compile_from_sexp(…) }             │
│   ⇒  tatara_lisp::domain::register::<XSpec>()   (opt-in runtime)      │
└──────────────────────────────────────────────────────────────────────┘
                              ↕
┌──────────────────────────────────────────────────────────────────────┐
│                        RUST KERNEL                                    │
│                                                                       │
│   Types (exhaustive sum types, newtypes, proofs)                     │
│   Memory safety (ownership + borrow)                                 │
│   Traits (typed dispatch)                                            │
│   Lattice algebra (tatara-lattice — meet/join/leq/Baseline)          │
│   Attestation (tatara-core — three-pillar BLAKE3 Merkle)             │
│   Lisp compiler (tatara-lisp — reader + macros + bytecode + cache)   │
└──────────────────────────────────────────────────────────────────────┘
                              ↕
┌──────────────────────────────────────────────────────────────────────┐
│                   EXECUTION SUBSTRATES                                │
│                                                                       │
│   K8s reconciler    sui Nix eval    OCI containers    NATS streams   │
│   (tatara-reconciler) (via adapter) (tatara-engine) (tatara-engine)  │
└──────────────────────────────────────────────────────────────────────┘
```

The kernel is narrow and fixed. The authoring surface is infinite — every new
domain costs one line of derive + one line of registration. The execution
substrates are plugin-oriented. All three layers speak the same typed IR.

---

## The five primitive surfaces

Each item below is shipped, tested, and load-bearing in production
tatara-lisp code.

### 1. Types as the ground truth

```rust
#[derive(TataraDomain, Serialize, Deserialize, Debug, Clone)]
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

The derive accepts anything that implements `serde::Deserialize`:
- Basic types (`String`, `i64`, `f64`, `bool`)
- Their `Option<_>` / `Vec<_>` siblings
- Any enum (authored in Lisp as a bare symbol — `Critical`, `Gate`, …)
- Any nested struct (authored as kwargs sublist — `(:k v :k v)`)
- `Vec<Nested>` (authored as list of kwargs lists)
- `#[serde(default)]` on a field makes the keyword optional in Lisp

### 2. Lisp forms as authoring surface

```lisp
(defmonitor :name "prom-up"
            :query "up{job='prometheus'}"
            :threshold 0.99
            :window-seconds 300
            :tags ("prod" "observability"))
```

Field names snake_case → Lisp keyword kebab-case: `window_seconds` ↔
`:window-seconds`. Enum values are bare Lisp symbols (`Gate`). Nested structs
use kwargs sublists. Vec<Nested> uses lists of kwargs lists.

### 3. Macros as composition

```lisp
(defmacro observability-fedramp (name)
  `(defpoint ,name
     :classification (:point-type Gate :substrate Observability)
     :compliance (:baseline "fedramp-moderate"
                  :bindings ((:framework "nist-800-53"
                              :control-id "SC-7"
                              :phase AtBoundary)))))

(observability-fedramp grafana-stack)   ; fully-typed template expansion
```

Quasi-quote + unquote + splice + `&rest` — every composition pattern Common
Lisp supports at the structural level. No evaluator needed: pure term
rewriting, no closures.

### 4. Registry as dispatch

```rust
tatara_lisp::domain::register::<MonitorSpec>();
tatara_lisp::domain::register::<AlertPolicySpec>();
// Later, at dispatch time:
if let Some(handler) = tatara_lisp::domain::lookup("defmonitor") {
    let typed_json = (handler.compile)(args)?;
}
```

Global handler table lets polymorphic dispatchers (like `tatara-check`) accept
any registered domain without knowing its concrete type at compile time.

### 5. `CompilerSpec` as factory

```lisp
(defcompiler my-ops-repl
  :name "my-ops-repl"
  :macros ("(defmacro when (c x) `(if ,c ,x))")
  :domains ("defmonitor" "defalertpolicy")
  :optimization "tree-walk")
```

```rust
let spec: CompilerSpec = compile_typed::<CompilerSpec>(src)?.pop().unwrap();
let my_compiler = tatara_lisp::realize_in_memory(spec)?;
let expanded = my_compiler.compile("(when #t (foo))")?;
```

Lisp compilers as Lisp data. Realize in memory, on disk, or both. Every
specialized compiler costs 1 Lisp form + 1 call to `realize_in_memory`.

---

## How to apply the pattern to a new domain

**The template is always the same.** Follow in order:

### Step 1 — define the Rust struct

```rust
use serde::{Serialize, Deserialize};
use tatara_lisp::DeriveTataraDomain;

#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]  // idiomatic for JSON <-> kebab <-> camel round-trip
#[tatara(keyword = "defmysystem")]
pub struct MySystemSpec {
    pub name: String,
    #[serde(default)]
    pub extra_tags: Vec<String>,
    #[serde(default)]
    pub threshold: Option<f64>,
    pub kind: MyKind,          // enum
    pub defaults: MyDefaults,  // nested struct
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MyKind { Observer, Guardian, Sentinel }

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MyDefaults {
    pub retention_days: Option<i64>,
    pub page_oncall: bool,
}
```

### Step 2 — wire it into your crate

```rust
// src/lib.rs
pub mod my_system;
pub use my_system::MySystemSpec;

// Called by your binary's main:
pub fn register() {
    tatara_lisp::domain::register::<MySystemSpec>();
}
```

### Step 3 — author in Lisp

```lisp
(defmysystem :name "perimeter-watch"
             :kind Sentinel
             :extra-tags ("prod" "edge")
             :threshold 0.99
             :defaults (:retention-days 30 :page-oncall #t))
```

### Step 4 — compile

```rust
// Typed top-level collection:
let specs = tatara_lisp::compile_typed::<MySystemSpec>(src)?;

// OR: typed + positional name (for (defmysystem NAME :k v) shape):
let defs = tatara_lisp::compile_named::<MySystemSpec>(src)?;
```

### Step 5 — (optional) write Lisp macros for domain ergonomics

```lisp
(defmacro sentinel-edge (name &rest overrides)
  `(defmysystem :name ,name :kind Sentinel :extra-tags ("edge") ,@overrides))

(sentinel-edge "perimeter-watch" :threshold 0.99)
```

Same typed result. Shorter surface.

### Step 6 — (optional) add to `checks.lisp`

Once registered, the `tatara-check` dispatcher compiles any `(defmysystem …)`
form in `checks.lisp` automatically.

---

## The canonical pleme-io pattern applied

Every typed domain in pleme-io (current + future) should follow this
template:

| Layer | What you provide | What you get |
|---|---|---|
| **Rust struct** | Fields with serde derives | Compile-time type safety |
| **Derive macro** | `#[derive(TataraDomain)] #[tatara(keyword = "defX")]` | Lisp authoring surface |
| **Registration** | One line in main(): `register::<XSpec>()` | Runtime dispatch via registry |
| **Lisp authoring** | `(defX :k v …)` forms in `.lisp` files | Human-readable, macro-composable |
| **Optional macros** | `defmacro` / `defpoint-template` / `defcheck` | Domain-specific ergonomics |
| **Optional rewriter** | `rewrite_typed::<XSpec>` with a Lisp-level policy | Self-optimization, type-preserving |

That's the whole pattern. Six lines of ceremony per new domain. Any
Rust struct you'd write for any other purpose is already 90% done — add the
derive, register it, author in Lisp.

---

## Patterns

### ✓ Do apply the derive to every user-facing typed struct

If you define a struct users might configure, apply `#[derive(TataraDomain)]`.
Cost: zero (one line). Value: infinite surface — YAML, JSON, Nix, Lisp all
compile to it via serde + derive. No specialized config parsers.

### ✓ Do push composition into Lisp macros

If you need "the observability-fedramp bundle" or "the EU-tenant compliance
preset" — write a macro. The macro expands into the typed form. No new Rust
required.

### ✓ Do use the registry for polymorphic dispatchers

Tools that handle heterogeneous forms (like `tatara-check`) should dispatch
via `tatara_lisp::domain::lookup`. Don't hand-roll a match statement on
keywords.

### ✓ Do use `rewrite_typed` for optimization policies

Lisp-level rewrites over typed values. Rust re-validates — any rewrite that
type-checks is safe by construction.

### ✓ Do derive `Serialize + Deserialize + Clone` on every struct

All three are required for the derive's universal `Deserialize` fallthrough
path (nested types, enums, `Vec<nested>`). Clone is needed for the registry's
handler closure.

### ✓ Do use `#[serde(default)]` for optional fields

The derive honors it — missing keyword in Lisp → `T::default()`. Matches
serde's canonical semantics for JSON/YAML/everywhere else.

### ✓ Do write a `register_all()` in your domain crate

Mirrors `tatara-domains::register_all`. Any binary that needs your domains
calls this once at startup.

---

## Anti-patterns

### ✗ Don't hand-roll a keyword parser

If you find yourself writing "match on symbol head, extract kwargs, parse per
field" — stop. The derive has already solved this, with universal type
support, for every field type serde understands.

### ✗ Don't skip `Deserialize` to "save a dep"

Every Rust value in pleme-io should be `Deserialize`-able. It's the single
boundary where "I accept external input" happens. The derive depends on it.

### ✗ Don't build a second S-expression AST

`iac_forge::sexpr::SExpr` is canonical serialization form (for attestation).
`tatara_lisp::Sexp` is homoiconic authoring form (for macros). These two
exist for fundamentally different reasons. Don't invent a third — bridge to
one or both via trait impls.

### ✗ Don't pursue a full Lisp evaluator

Tier 4 (closures, recursion, garbage collection) is an explicit anti-goal.
Tiers 0-2 (authoring, multi-domain, typed queries) cover the actual needs;
Tier 4 pays a massive cost for scripting-language baggage we don't want.

### ✗ Don't bypass the registry for "performance"

The registry is an `Arc<Mutex<HashMap>>` — lookup is ~200ns, roughly the cost
of one allocation. Any call that would benefit from skipping it almost
certainly also doesn't need the dynamism that motivates having a registry.

### ✗ Don't add BLAKE3 as a cache key hasher

SipHash (via `DefaultHasher`) is the correct choice for in-process cache
keys. BLAKE3 is for attestation + store paths where collision resistance
matters.

### ✗ Don't assume expansion-order semantics

The macroexpander runs to a fixed point; nested macros expand iteratively.
Write macros that are correct regardless of the visit order of the expander.

---

## Performance layers

All three layers are shipped, independently toggleable, composed by default:

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 3: Expansion cache (SipHash key, Arc<Mutex<HashMap>>) │  ← 1.29× win
├─────────────────────────────────────────────────────────────┤
│ Layer 2: Compiled-template bytecode (linear op stream)      │  ← 1.08× win
├─────────────────────────────────────────────────────────────┤
│ Layer 1: Substitute walker (name-keyed HashMap fallback)    │  ← baseline
└─────────────────────────────────────────────────────────────┘
```

Measured on a 10k-call benchmark across 10 unique (macro, args) pairs:
- substitute only: 10.2 ms
- bytecode no cache: 9.4 ms
- **bytecode + cache: 7.3 ms (1.40× speedup)**

All three produce byte-identical output on identical input. Proven by test.

**Next layers** (orthogonal, additive):
- **Arena allocation** (`bumpalo`) — expected ~1.5–2× additional win
- **Lazy expansion** — defer expansion of macro call arguments until consumed
- **AOT Rust codegen** — `tatara-lispc --emit rust` generates specialized
  compilers as Rust modules

Each layer is opt-in; each delivers on top of the previous. The compound
ceiling for a typical workload is ~3× vs. substitute, or ~5× against hand-
rolled expansion with poor abstraction boundaries.

---

## Tiered ambition — where this goes

```
Tier 0 — AUTHORING                            ✓ SHIPPED
  Reader + AST + macroexpander + derive
  `defpoint`, `defcheck`, `defmonitor`, `defcompiler`, `defderivation`, …
  11 domains today, each costing 1 line of derive to add

Tier 1 — MULTI-DOMAIN VIA SAME MACHINERY      ✓ SHIPPED
  Register-and-dispatch pattern
  ~100 LOC per new domain (just a struct)

Tier 2 — TYPED QUERY DSL                      ⟳ DESIGN READY
  (find-processes :where (and (phase Attested) (substrate Observability)))
  Tiny evaluator over let/match/if + bound-name lookup
  No closures, no recursion — typed term rewriting with typed primitives

Tier 3 — BUILD-TIME RUST CODEGEN              ⟳ SCOPED
  tatara-lispc --emit rust
  Compiles CompilerSpec → specialized Rust module
  User-extensible metaprogramming without modifying rustc

Tier 4 — FULL EVALUATOR                       ✗ ANTI-GOAL
  Closures, recursion, mutable state, GC
  Tiers 2+3 cover real needs at ~1/10 the cost
```

Every tier is composable with the ones below it. Tier 2 builds on Tier 1's
registration. Tier 3 builds on Tier 2's typed operators. Tier 4 adds no
capability we want.

---

## Self-bootstrapping — the diminishing-returns theorem

Once `CompilerSpec` exists (Tier 0), optimizing the base tatara-lisp Rust
compiler competes against producing specialized `RealizedCompiler`s. Usually
the specialized compiler wins: it has a tighter macro library, a narrower
registered domain set, a fixed optimization profile.

> **The base compiler is bootstrap infrastructure. Most real-world
> compilation happens through specialized compilers.**

Examples of the specialization space:
- **Ops REPL compiler** — preloads `defprocess` / `defcheck` / `defsignal`
  macros; strict dialect; rejects unbound symbols.
- **Compliance authoring compiler** — preloads `nist-control` / `cis-control`
  / `fedramp-bundle` macros; domains restricted to `defcompliance-suite`.
- **Per-tenant compiler** — preloads tenant-specific overrides + defaults.
- **Monitoring DSL compiler** — preloads `defmonitor` / `defalertpolicy` /
  `defnotify` aliases; no `defpoint` support (not in scope for operators).

Each specialization is a CompilerSpec. Each compiles faster than the base on
its domain because it has smaller macro libraries and specialized dispatch
paths. Each can be realized in memory or on disk, shipped independently,
evolved on its own cadence. **The base compiler's job is to be the kernel
these specializations are built on top of — nothing more.**

---

## When NOT to use this pattern

The Rust + Lisp pattern is the default for **typed domain authoring**.
It's not appropriate for:

- **Opaque binary data** (images, audio, tarballs). Use `Vec<u8>` + hash.
- **High-frequency event streams** (traces, metrics at >1MHz). Use a typed
  ringbuffer.
- **External protocol buffers** (gRPC, protobuf). The generated types are
  already Rust; compose with the derive where they're `Deserialize`.
- **One-off scripts** with no composition needs. A Rust binary is fine.
- **Interactive session state** that mutates across calls. Use normal
  typed state; expose a MCP/CLI surface.

If the domain is: typed configuration, declarative spec, compositional
authoring, multi-surface rendering — apply the pattern. Otherwise don't
reach for it.

---

## Crate map — where things live

| Crate | Role |
|---|---|
| `tatara-lisp-derive` | `#[derive(TataraDomain)]` proc macro — 3 deps (syn, quote, proc-macro2) |
| `tatara-lisp` | Reader + AST + macroexpander + registry + `rewrite_typed` + `CompilerSpec` + performance layers |
| `tatara-process` | Canonical complex domain: `ProcessSpec` with 8 fields, all nested, all via the derive |
| `tatara-domains` | Reference demo domains: `MonitorSpec`, `NotifySpec`, `AlertPolicySpec` |
| `tatara-nix` | Nix academic primitives as typed Lisp domains: `Derivation`, `Module`, `Overlay`, `Flake`, `StorePath` |
| `tatara-lattice` | `meet` / `join` / `leq` / `Baseline` — composition algebra |
| `tatara-core` | Shared domain types + `ConvergenceAttestation` (three-pillar BLAKE3 Merkle) |
| `iac-forge` (external) | Canonical `SExpr` for attestation + render cache — interop via feature flag |

Every repo in pleme-io should either use these crates directly or follow the
same pattern internally. Duplication of S-expression ASTs, ad-hoc config
parsers, or hand-rolled kwargs matchers are explicit anti-patterns.

---

## References

- [`tatara-lisp/DESIGN.md`](../tatara-lisp/DESIGN.md) — three-layer S-expression
  topology + tiered roadmap
- [`tatara-lisp/README.md`](../tatara-lisp/README.md) — user-facing tour of
  reader + macros + derive + registry
- [`tatara-lisp-derive/README.md`](../tatara-lisp-derive/README.md) — proc
  macro attribute reference, supported field types
- [`tatara-domains/README.md`](../tatara-domains/README.md) — three reference
  typed domains + register_all() pattern
- [`docs/k8s-as-processes.md`](k8s-as-processes.md) — the largest single
  application of the pattern: Process CRD + full lifecycle + attestation
- [`docs/lisp-authoring.md`](lisp-authoring.md) — surface-level user guide
  for authoring in Lisp
- Tests:
  - `tatara-lisp::macro_expand::tests::expansion_layers_agree_on_output_and_cache_wins`
    — the three-way benchmark
  - `tatara-lisp::compiler_spec::tests::self_bootstrapping_compiler_generates_another_compiler`
    — proof of the factory pattern
  - `tatara-process::compile_tests::full_processspec_round_trip_via_derive`
    — proves the derive works on the most complex typed struct in the system

---

## Naming — Brazilian × pleme-io

New concepts in this lineage are named with a Brazilian-Portuguese flavor
layered onto the existing Japanese-idiomatic pleme base. The blend matches
the system's personality: the Japanese (`tatara` 粋 = furnace, `sui`, `sekkei`,
`takumi`) names the discipline and precision; Brazilian Portuguese
(`terreiro`, `forja`, `cordel`, `cerrado`) names the enclosed ritual spaces,
forges, rhythmic flows, and ecosystems in which things grow.

Reference table — apply to new primitives:

| Concept | Brazilian-flavored name | Rationale |
|---|---|---|
| **Arena / enclosed Lisp VM** | `terreiro` | In Candomblé, the sacred compound where rituals are valid. A bounded memory region with deterministic semantics. |
| **Compiler factory / furnace** | `forja` | Forge (pairs with tatara = furnace). Produces specialized compilers. |
| **Bytecode stream** | `cordel` | Folk-poetry string-verse form. Linear sequence of typed units. |
| **Typed substrate ecosystem** | `cerrado` | Brazilian savanna. Deep-rooted, slow-growing, regeneratively persistent. |
| **Slow-persistent bootstrap** | `jabuti` | Tortoise. Builds slowly, keeps its form. |
| **Rhythmic composition** | `samba` | Layered rhythmic structure. Module composition. |
| **Content-addressable field** | `roça` | Cleared agricultural plot. Things are grown here and indexed by where. |
| **Overlay / layered pattern** | `bordado` | Embroidery. Stitch on top without disturbing the base. |
| **Registry / notebook** | `caderneta` | Small book of records. Operator's working memory. |
| **Validator / inspector** | `apurador` | Examiner. Runs checks. |

Japanese names stay on existing crates (`tatara`, `sui`, `sekkei`) and
organizational roles (the discipline layer). Brazilian-Portuguese names
attach to new primitives that evoke **enclosed spaces**, **flows**,
**growth**, or **craft** — Tier 2+ concepts in particular.

The two traditions do not clash: they name orthogonal axes. Japanese
= precision and craft. Portuguese = ritual space, tropical cultivation,
rhythmic composition. Together the naming communicates the system's whole
self-understanding: disciplined kernel, generative surface.

**First new primitive under this convention:** `tatara-terreiro` — the
arena-enclosed Lisp virtual environment with sealable bytecode deployment.
See [`../tatara-terreiro/README.md`](../tatara-terreiro/README.md) once
shipped.

---

## Summary — the six-line contract

For every typed domain you introduce in pleme-io:

```rust
#[derive(TataraDomain, Serialize, Deserialize, Debug, Clone)]      // 1
#[tatara(keyword = "defmydomain")]                                  // 2
pub struct MyDomainSpec { /* fields */ }                            // 3

pub fn register() {                                                 // 4
    tatara_lisp::domain::register::<MyDomainSpec>();                // 5
}                                                                   // 6
```

Six lines buys you: Lisp authoring, YAML authoring, Nix authoring, Rust
programmatic authoring, typed compilation, macro composition, self-
optimization via `rewrite_typed`, deterministic identity via BLAKE3,
registry dispatch, and participation in the workspace-wide coherence
checker.

**This is the primary architectural pattern for pleme-io from this point
forward.** If you are building a new domain and you're not using it,
explain why in a design doc or comment. If you're building infrastructure
that every domain will touch, optimize the pattern itself — that's the
diminishing-returns theorem at work.

Rust is the kernel. Lisp is the interface. The derive is the bridge.
Everything else compounds on top.
