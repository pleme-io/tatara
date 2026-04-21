# Tatara (粋) — Programmable Convergence Computer

<!-- Blackmatter alignment: pillars 1, 6, 10 -->
<!-- See ~/code/github/pleme-io/BLACKMATTER.md for pillar definitions. -->

## Blackmatter pillars upheld

- **Pillar 1** (Rust + tatara-lisp + WASM/WASI): Tatara IS the Lisp half of Pillar 1 — the fluid runtime + macro composition + `#[derive(TataraDomain)]` boundary other repos plug into.
- **Pillar 6** (Typescape): `TataraDomain` registry gives every new typed domain a deterministic BLAKE3 identity + workspace-wide coherence check.
- **Pillar 10** (Proofs): Tatara's coherence checker is itself a proof — every registered domain's dispatch is verified at compile time.

> ## ★ The primary architectural pattern: **Rust + Lisp**
>
> Every new typed domain in pleme-io should follow the Rust + Lisp pattern.
> Rust owns types, invariants, memory safety. Lisp owns authoring,
> composition, runtime IR rewriting. The boundary is one proc macro —
> `#[derive(TataraDomain)]`. Six lines of ceremony per domain buys Lisp
> authoring, macro composition, registry dispatch, and BLAKE3 attestation.
>
> **Canonical reference:** [`docs/rust-lisp.md`](docs/rust-lisp.md) — the
> manifesto + cookbook + anti-patterns. Read it before adding a new domain
> anywhere in pleme-io.
>
> **Measured performance**: three-layer expander (substitute / bytecode /
> bytecode+cache) — 1.40× speedup on cache-friendly workloads. All layers
> optional, orthogonal, proven-equivalent by test.



Three theories compose into one platform:

- **Unified Infrastructure Theory** (substrate): WHAT — Nix declares intent
- **Unified Convergence Computing Theory** (tatara): HOW — typed DAGs converge
- **Compliant Computing Theory** (tameshi + kensa): WHETHER — compliance gates

## Core Concepts

**Convergence IS computation.** Every operation is a convergence point.
Points compose into typed DAGs. DAGs compose into DAGs-of-DAGs.

**Five-layer pipeline**: DECLARE (Nix + substrate) → PLAN (sui) → GATE
(tameshi + kensa) → EXECUTE (tatara) → STORE (sui store + sui-cache)

**Five invariants** (always true):
1. Every operation is a convergence point
2. Every point has typed atomic boundary (prepare → execute → verify → attest)
3. Every attestation is a content-addressed store path (sui)
4. Compliance binds to point types, verified before/during execution
5. Dependency closure is statically computable at plan time

**Six classification dimensions** (every point classified along all six):
- **Horizon**: Bounded (terminates) | Asymptotic (runs forever)
- **Structure**: Transform | Fork | Join | Gate | Select | Broadcast | Reduce | Observe
- **Substrate**: Financial | Compute | Network | Storage | Security | Identity | Observability | Regulatory
- **Coordination**: Monotone (gossip) | NonMonotone (Raft)
- **Trust**: PlanTime | AtBoundary | PostConvergence
- **Intelligence**: Mechanical | AiAssisted | Hybrid

**Intent/Outcome duality**:
- Intent leaves = asymptotic roots (the WHY, runs forever, produces outcomes)
- Outcome leaves = bounded terminals (the WHAT, verified, converged = true)

**Absorption principle**: every external system becomes convergence points
(observe → type → converge → attest → comply → package)

## Architecture

```
Nix Declaration (intent)           ← Unified Infrastructure Theory
  ↓ archetype rendering
Any Backend (K8s, tatara, WASI)    ← Rendered by substrate
  ↓ sui evaluates
Convergence Derivation Graph       ← Content-addressed, closures computable
  ↓ compliance binding
Type-Level Compliance Controls     ← Compliant Computing Theory
  ↓ convergence DAG
Verified Atomic Checkpoints        ← Unified Convergence Computing Theory
  ↓ distributed execution
Tatara Nodes (CALM-classified)     ← Raft (non-monotone) + gossip (monotone)
  ↓ cryptographic attestation
Tameshi CertificationArtifact      ← BLAKE3 Merkle: artifact + controls + intent
  ↓ store
Sui Store (content-addressed)      ← Generational store paths, sui-cache distribution
  ↓ AI interface
MCP + REST + GraphQL + gRPC        ← Mechanical + AI mix at any point
```

## Workspace Crates (14+)

### Core runtime (pre-existing)
| Crate | Purpose |
|-------|---------|
| `tatara-core` | Domain types: convergence state, WorkloadPhase, DAG, saga, idempotency, traced events |
| `tatara-engine` | Runtime: 7 drivers, Raft, gossip, convergence engine, scheduler, health probes, catalog, metrics, sui client |
| `tatara-net` | Networking plane: NetworkPlane trait, eBPF types, WASI types, mesh, flow observability |

### K8s-as-processes surface (v1alpha1 — Apr 2026)
| Crate | Purpose |
|-------|---------|
| `tatara-process` | **Process + ProcessTable CRDs** — K8s-as-Unix-processes wire format (`tatara.pleme.io/v1alpha1`). `ProcessSpec` derives `TataraDomain` so `(defpoint …)` in Lisp is a first-class authoring surface. Houses `compile_source` + `tatara-lispc` binary. Absorbs `ConvergenceProcess`, `ConvergenceService`, `NixBuild`. |
| `tatara-lattice` | Lattice algebra over `Classification` — `meet` / `join` / `leq` / `Baseline`. Replaces `qualities_match`. |
| `tatara-lisp` | **Homoiconic S-expression surface.** Reader, AST, macroexpander (quasi-quote + unquote + splice + `&rest`), `TataraDomain` trait, domain registry, `TypedRewriter` (self-optimization primitive), generic `compile_typed`/`compile_named`, iac-forge canonical-form interop (feature-gated). |
| `tatara-lisp-derive` | **`#[derive(TataraDomain)]`** — proc macro that auto-generates a Lisp compiler for any struct with `serde::Deserialize`. Universal-Deserialize fallthrough handles enums, nested structs, `Vec<Nested>`. Honors `#[serde(default)]`. |
| `tatara-domains` | Reference typed domains (MonitorSpec, NotifySpec, Severity enum, EscalationStep, AlertPolicySpec) + `register_all()` registry seed. Demonstrates every derive kind. |
| `tatara-reconciler` | **FluxCD-adjacent K8s controller.** 10-phase Unix lifecycle. Owner-ref-emitted Kustomizations. Signal annotation ingestion. Finalizer-guarded termination. Three-pillar BLAKE3 attestation chain. `tatara-check` binary runs `checks.lisp`. Replaces `tatara-kube`. |

### Operational surfaces
| Crate | Purpose |
|-------|---------|
| `tatara-api` | REST (Axum) + GraphQL (async-graphql): jobs, allocations, nodes, catalog, health, metrics |
| `tatara-cli` | CLI + `tatara server` |
| `tatara-mcp` | MCP tool surface (will absorb convergence-controller's 15 tools) |
| `tatara-testing` | Test fixtures and helpers |
| `ro-cli` | Read-only CLI |

### Deprecated
| Crate | Replaced by |
|-------|-------------|
| `tatara-kube` | `tatara-reconciler` (FluxCD-adjacent, not bypassing) — see `tatara-kube/DEPRECATED.md` |
| `tatara-operator` | `Intent::Nix` field in `Process` (NixBuild semantics absorbed) — see `tatara-operator/DEPRECATED.md` |

## K8s-as-Processes Model (v1alpha1)

Every reconciled object is a **Unix process** in the tatara convergence lattice.
Clusters, HelmReleases, DB migrations, compliance checks, tests — a single
`Process` CRD expresses all of them. The reconciler's state machine literally
implements Unix semantics:

```
Pending → Forking → Execing → Running → Attested
                                       ↘ Failed
Attested → Reconverging → Execing              (SIGHUP path, no zombie)
Running  → Exiting      → Zombie → Reaped     (SIGTERM path)
```

### One CRD, three realities

A single `Process` carries:

1. **Identity** — hierarchical PID in a cluster-scoped `ProcessTable` (`/proc`).
   Content-addressable BLAKE3 (128-bit, 26-char base32) — ported from
   convergence-controller/src/identity.rs.
2. **Classification** — 6-axis lattice position (re-exports from `tatara-core`).
3. **Intent** — one of `nix` / `flux` / `lisp` / `container`. The RENDER phase
   dispatches on the variant.
4. **Boundary** — `preconditions` gate Running; `postconditions` gate Attested.
   `ConditionKind`: `ProcessPhase`, `KustomizationHealthy`, `HelmReleaseReleased`,
   `PromQL`, `Cel`, `NixEval`.
5. **Compliance bindings** — verified at `PlanTime` | `AtBoundary` | `PostConvergence`.
6. **Signals** — `SIGHUP | SIGTERM | SIGKILL | SIGUSR1 | SIGUSR2 | SIGSTOP | SIGCONT`
   delivered via `tatara.pleme.io/signal` annotation.

### FluxCD is `exec(2)`

`tatara-reconciler` does **not** replace source-controller / kustomize-controller /
helm-controller. It *emits* Flux CRs (annotated with process metadata) and watches
their status as part of the VERIFY phase. A cluster running tatara-reconciler
looks like a cluster running FluxCD *plus* the `Process` CRD with three-pillar
attestation annotations on every owned resource.

### Four rendering surfaces, one type

```
Nix module      ──┐
YAML (kubectl)  ──┤
Rust builder    ──┼──►  ProcessSpec  ──►  tatara-reconciler
S-expr (lisp)   ──┘
```

Each surface produces the same `ProcessSpec`. The S-expr form is homoiconic —
macros can compose proven Process templates into new Processes.

### Three-pillar attestation (BLAKE3 Merkle)

Every convergence cycle writes a `ProcessAttestation`:

```
composed_root = BLAKE3(
    "tatara-process/v1alpha1\n"
    ++ artifact_hash     // rendered resources + applied status
    ++ control_hash?     // compliance proof (empty iff no bindings)
    ++ intent_hash       // canonical spec + nix store path + lisp AST
    ++ previous_root?    // chain to prior attestation
)
```

`previous_root` chains each generation; `sekiban` + `kensa` consume the composed
root as the audit-trail anchor.

## Homoiconic Lisp Surface — the authoring / rewriting layer

**`#[derive(TataraDomain)]`** is the one-liner that unlocks a Lisp authoring
surface for any `serde::Deserialize` struct. Applied to `ProcessSpec` itself
(and to MonitorSpec / NotifySpec / AlertPolicySpec in tatara-domains).

```rust
#[derive(CustomResource, DeriveTataraDomain, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(group = "tatara.pleme.io", version = "v1alpha1", kind = "Process", ...)]
#[tatara(keyword = "defpoint")]
pub struct ProcessSpec { ... }
```

Then:
```lisp
(defpoint observability-stack
  :identity       (:parent "seph.1")
  :classification (:point-type Gate :substrate Observability)
  :intent         (:nix (:flake-ref "github:…" :attribute "observability"))
  ...)
```

Compiles to typed `NamedDefinition<ProcessSpec>` via
`tatara_process::compile_source`. The `tatara-lispc` binary pipes
Lisp → Process YAML → kubectl.

### The Rust-bordered, Lisp-mutable, self-optimizing model

Three invariants proven by working code:

1. **Typed entry** — `T::compile_from_sexp(sexp)` is the only way to produce a
   `T` from Lisp. Any ill-typed input errors before the `T` exists.
2. **Rewriting freedom inside** — `rewrite_typed(t, f)` lets `f` mutate the
   Sexp IR arbitrarily. No mid-rewrite type checks needed.
3. **Typed exit** — re-entering `compile_from_args` on the rewritten Sexp
   guarantees the output is a well-typed `T`, or the rewrite is rejected.

Combined: any Lisp-level optimization policy can run full-speed without risking
type invariants. Rust's type system is the floor; Lisp's rewriting is free
within it.

### `checks.lisp` — workspace coherence, Lisp-driven

`cargo run --bin tatara-check -p tatara-reconciler` reads `checks.lisp` at
workspace root and dispatches each form through a typed Rust executor:

- **Built-in primitives**: `crd-in-sync`, `yaml-parses`, `yaml-parses-as`,
  `lisp-compiles`, `file-contains`
- **User-defined macros**: `(defcheck name (params) `(do …primitive-calls))`
- **Registry fallthrough**: any `(defX …)` form whose keyword matches a
  registered `TataraDomain` is compiled typed — no built-in handler needed

11 runtime checks pass, including compiling `observability-stack.lisp` to
`ProcessSpec` via the derive + registry + `defalertpolicy` / `defmonitor` /
`defnotify` via the registry fallthrough. Zero shell.

### Reuse boundary with iac-forge

Three S-expression layers, non-overlapping:

| Layer | Type | Purpose |
|-------|------|---------|
| Authoring | `tatara_lisp::Sexp` | Homoiconic, macro-capable, human-written |
| Typed | `ProcessSpec`, etc. | Exhaustive sum types, compile-time proof |
| Canonical | `iac_forge::sexpr::SExpr` | BLAKE3 attestation + render cache |

`tatara-lisp --features iac-forge` provides `From<Sexp> for iac_forge::SExpr`
so tatara plugs into the existing attestation pipeline.

## Key Types (tatara-core/src/domain/convergence_state.rs)

- `ConvergenceDistance`: Converged | Partial | Diverged | Unknown (0.0 to 1.0)
- `ConvergenceState`: distance + rate + oscillation + damping per entity
- `ConvergencePoint`: named step with CALM classification + typed boundary
- `ConvergenceBoundary`: preconditions + postconditions + attestation chain
- `BoundaryPhase`: Pending → Preparing → Executing → Verifying → Attested | Failed
- `ClusterConvergence`: cluster-wide summary (is_fully_healthy + is_fully_converged)
- `CalmClassification`: Monotone | NonMonotone
- `ConvergenceMechanism`: Raft | Gossip | Local | Nats | FixedPoint | Feedback

## Convergence Horizons

- **Bounded**: has a fixed point, terminates at distance = 0
- **Asymptotic**: runs forever, rate is the health signal (never converged, always improving)
- **Bounded preferred**: maximize bounded points, asymptotic points emit bounded DAGs from emission schemas
- **Emission schema**: catalog of bounded DAG templates an asymptotic point can instantiate

## 7 Execution Drivers

| Driver | Backend | Platform |
|--------|---------|----------|
| `exec` | Direct process (fork+exec) | Unix |
| `oci` | Docker/Podman/Apple Containers | All |
| `nix` | `nix run <flake_ref>` | All with Nix |
| `nix_build` | `nix build` + sui-cache push | All with Nix |
| `kasou` | Apple Virtualization.framework VMs | macOS |
| `kube` | Kubernetes Server-Side Apply | All with kubeconfig |
| `wasi` | wasmtime WASI Preview 2 | All with wasmtime |

## WorkloadPhase Lifecycle

```rust
enum WorkloadPhase<W, E, C, T> {
    Initial,          // Defined but not active
    Warming(W),       // Acquiring resources, resolving deps
    Executing(E),     // Active, healthy, serving
    Contracting(C),   // Gracefully draining
    Terminal(T),      // Done
}
```

## Distributed State Machine

- **Raft** (openraft): linearizable writes for placement, allocation lifecycle
- **Gossip** (chitchat): eventually-consistent metadata, failure detection
- **CQRS**: desired vs observed split in ClusterState
- **Leader-affinity**: only the leader schedules

## The Tatara/Sui Split

| Concern | Sui | Tatara |
|---------|-----|--------|
| Role | Store + evaluator + planner | Engine + executor |
| Input | Nix expressions | Convergence derivations |
| Output | Derivation graph + store paths | Attested convergence state |
| State | Content-addressed (immutable) | Live convergence (mutable) |
| Distribution | sui-cache binary cache | Raft + gossip |
| API | REST + GraphQL + gRPC | REST + GraphQL + SSE |

## REST API

| Endpoint | Purpose |
|----------|---------|
| `GET /health` | Health check |
| `GET/POST /api/v1/jobs` | List/submit jobs |
| `GET /api/v1/allocations` | List allocations |
| `GET /api/v1/nodes` | List cluster nodes |
| `GET /api/v1/events/stream` | SSE event stream |
| `GET /v1/catalog/services` | List service names |
| `GET /v1/health/service/{name}?passing=true` | Healthy instances |
| `GET /metrics` | Prometheus text format |

## Nix Integration

```nix
# HM module for macOS/Linux service
services.tatara.server = {
  enable = true;
  httpAddr = "127.0.0.1:4646";
  nats.enable = true;
  sui.daemonAddr = "127.0.0.1:8080";
};

# Declarative workloads
services.tatara.workloads.my-service = {
  enable = true;
  groups.main.tasks.app = {
    driver = "wasi";
    flakeRef = "github:pleme-io/my-service";
  };
};
```

## Documentation

| Document | Sections | Lines |
|----------|----------|-------|
| [Unified Platform Architecture](docs/unified-platform-architecture.md) | 14 sections: pipeline, dimensions, invariants, envelope, territory, architecture, types, duality, absorption, optimizer, AI | ~1400 |
| [Unified Convergence Computing Theory](docs/unified-convergence-computing-theory.md) | 13 sections: foundations, metrics, composition, cost, algebra, substrates, analysis, store, compliance, implementation, meta, frontiers, summary | ~2000 |
| [Theory Realization Map](docs/theory-realization-map.md) | Technology → theory mapping for every pleme-io component | ~180 |

## Build

```bash
cargo check          # Workspace check
cargo test           # All tests
cargo build          # Debug build
nix build            # Release via substrate
```

## Conventions

- Edition 2021, MIT license
- `clippy::pedantic` on tatara-kube and tatara-net
- Release: codegen-units=1, lto=true, opt-level="z", strip=true
- Pure Rust — no C, no Go
- All state changes through Raft (except gossip-only health/metrics per CALM theorem)
