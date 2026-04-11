# Unified Platform Architecture

## Abstract

Three theories compose into one platform. The **Unified Infrastructure Theory**
(substrate) declares WHAT. The **Unified Convergence Computing Theory** (tatara)
computes HOW. The **Compliant Computing Theory** (tameshi + kensa) gates WHETHER.

But these are not three separate concerns bolted together. They are three
expressions of a single principle: **convergence is the universal computational
primitive**. Writing Nix code is convergence (thought → declaration). Evaluating
Nix is convergence (modules → configuration via fixed-point). Rendering archetypes
is convergence (abstract → concrete). Planning is convergence (unknown → analyzed).
Verifying compliance is convergence (unverified → attested). Executing workloads
is convergence (diverged → converged). Optimizing cost is convergence (current →
cheaper). Growing a business is convergence (now → better).

The entire system, from human intent to business outcome, is a single convergence
pipeline where each stage is itself convergence. This document defines the
architecture, abstractions, and invariants that make this work, and maps the
complete territory of what can be reliably expressed.

---

## 1. The Universal Computation Pipeline

Every operation flows through five layers. Each layer is convergence.

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│                    ┌───────────────┐                             │
│                    │   DECLARE     │  Nix + Substrate            │
│                    │   (Intent)    │  Infrastructure Theory      │
│                    └───────┬───────┘                             │
│                            ↓                                    │
│                    ┌───────────────┐                             │
│                    │    PLAN       │  Sui evaluator              │
│                    │   (Analysis)  │  Derivation graph           │
│                    └───────┬───────┘                             │
│                            ↓                                    │
│                    ┌───────────────┐                             │
│                    │    GATE       │  Tameshi + Kensa            │
│                    │   (Trust)     │  Compliant Computing        │
│                    └───────┬───────┘                             │
│                            ↓                                    │
│                    ┌───────────────┐                             │
│                    │   EXECUTE     │  Tatara engine              │
│                    │ (Convergence) │  Convergence Computing      │
│                    └───────┬───────┘                             │
│                            ↓                                    │
│                    ┌───────────────┐                             │
│                    │    STORE      │  Sui store + sui-cache          │
│                    │ (Persistence) │  Content-addressed state    │
│                    └───────────────┘                             │
│                                                                 │
│   Each layer's output feeds the next.                           │
│   Each layer's result is content-addressed.                     │
│   Each layer's operation is itself convergence.                 │
│   The pipeline IS the theory applied recursively.               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 1.1 DECLARE — The Intent Layer

**Theory**: Unified Infrastructure Theory (substrate)
**Runtime**: Nix language (evaluated by sui)

The human expresses intent using abstract workload archetypes:

```nix
mkHttpService { name = "auth"; source = self; ports = [8080]; health = { path = "/health"; }; }
```

This produces an abstract spec that is backend-independent. Renderers translate
the spec to any target (K8s, tatara, WASI, compose). Policies evaluate at
render time — violations are errors, not warnings.

**What converges here**: human thought → Nix expression → abstract spec.
The Nix module system is itself a fixed-point computation (convergence).

### 1.2 PLAN — The Analysis Layer

**Theory**: Convergence Computing Theory (sections 5.3, 7)
**Runtime**: Sui evaluator + convergence builtins

Sui evaluates the Nix expression into a **convergence derivation graph** — a
DAG of content-addressed convergence points with typed edges. Before any
execution:

- Dependency closures are computed (like `nix-store --requisites`)
- Compliance controls are bound to point types
- Cache hits are identified (attestation unchanged → skip)
- Critical path and parallelism are determined
- Convergence time and resource cost are estimated
- The full compliance closure is computed

**What converges here**: unknown analysis → complete convergence plan.
The plan IS a Nix derivation graph. It can be diffed, queried, and cached.

### 1.3 GATE — The Trust Layer

**Theory**: Compliant Computing Theory (section 9)
**Runtime**: Tameshi + Kensa + Sekiban + Inshou

Compliance verification runs at three phases:

| Phase | When | Cost | What |
|-------|------|------|------|
| **Plan-time** | Before execution | Zero | Static analysis of DAG structure against controls |
| **At-boundary** | During convergence | Inline | Verification at each convergence point's boundary |
| **Post-convergence** | After convergence | Probes | Live InSpec verification of running state |

Each verification produces a tameshi CertificationArtifact — a BLAKE3 Merkle
tree binding artifact hash + control hash + intent hash. This three-pillar
binding means you cannot forge one without invalidating the others.

**What converges here**: unverified computation → attested computation.
Compliance is convergence from "unchecked" to "proven."

### 1.4 EXECUTE — The Convergence Layer

**Theory**: Convergence Computing Theory (sections 1–6)
**Runtime**: Tatara convergence engine

Each convergence point is driven through its atomic boundary:

```
Prepare → Execute → Verify → Attest ──hash──→ Next Point
```

Points are classified along five dimensions (section 2 below). The engine
dispatches monotone operations to gossip, non-monotone to Raft. Substrate
DAGs run in parallel. Bounded points terminate; asymptotic points run
forever, emitting new bounded DAGs from their emission schemas.

**What converges here**: diverged state → desired state, per point, per
substrate, continuously.

### 1.5 STORE — The Persistence Layer

**Theory**: Convergence Computing Theory (section 8)
**Runtime**: Sui store + sui-cache binary cache

Every attestation is a content-addressed store path in the Nix store (via sui).
Generational store paths form append-only Merkle chains. The store supports
closure queries (forward, reverse, impact), garbage collection, and
distributed replication via sui-cache.

**What converges here**: local state → distributed state. The store itself
converges across the cluster through binary cache synchronization.

---

## 2. The Six Classification Dimensions

Every convergence point is classified along six orthogonal axes. Together,
these fully determine how the point is scheduled, coordinated, verified,
how long it lives, and whether intelligence participates.

```
                    ┌─────────────┐
                    │ Convergence │
                    │   Point     │
                    └─┬──┬──┬──┬─┘
                      │  │  │  │
         ┌────────────┘  │  │  └────────────┐
         ↓               ↓  ↓               ↓
   ┌──────────┐   ┌─────────────┐   ┌────────────┐
   │ Horizon  │   │  Structure  │   │ Substrate  │
   │          │   │             │   │            │
   │ Bounded  │   │ Transform   │   │ Financial  │
   │ Asymptot │   │ Fork        │   │ Compute    │
   │          │   │ Join        │   │ Network    │
   └──────────┘   │ Gate        │   │ Storage    │
                  │ Select      │   │ Security   │
   ┌──────────┐   │ Broadcast   │   │ Identity   │
   │ Coord.   │   │ Reduce      │   │ Observ.    │
   │          │   │ Observe     │   │ Regulatory │
   │ Monotone │   └─────────────┘   └────────────┘
   │ Non-mono │
   └──────────┘   ┌──────────────┐   ┌──────────────┐
                  │    Trust     │   │ Intelligence │
                  │              │   │              │
                  │ PlanTime     │   │ Mechanical   │
                  │ AtBoundary   │   │ AiAssisted   │
                  │ PostConvg    │   │ Hybrid       │
                  └──────────────┘   └──────────────┘
```

### Dimension 1: Horizon — How Long Does This Run?

```rust
enum ConvergenceHorizon {
    Bounded,                    // terminates at distance = 0
    Asymptotic {                // runs in perpetuity
        metric: String,         // what is optimized
        direction: Minimize | Maximize,
        healthy_rate: f64,      // rate threshold for health
    },
}
```

Bounded points converge and rest. Asymptotic points converge forever and
continuously emit bounded DAGs from their emission schemas. Bounded is
preferred — maximize bounded, catalog what asymptotic points can emit.

### Dimension 2: Structure — How Does Data Flow?

```rust
enum ConvergencePointType {
    Transform,   // 1 input → 1 output (linear)
    Fork,        // 1 input → N outputs (fan-out, spawns downstream DAGs)
    Join,        // N inputs → 1 output (fan-in, merges upstream results)
    Gate,        // N inputs → 1 output (barrier, waits for all)
    Select,      // N inputs → 1 output (choice, picks best by policy)
    Broadcast,   // 1 input → N outputs same type (replicate)
    Reduce,      // N inputs → 1 output (fold/aggregate)
    Observe,     // 1 input → 1 output + side-channel (tap)
}
```

These compose via a closed algebra: sequence (;), parallel (|), fork (⊳),
join (⊲), select (▷), nest ([]). Any composition of valid points is a valid DAG.

### Dimension 3: Substrate — What Dimension Is Being Converged?

```rust
enum SubstrateType {
    Financial,      // cost, billing, spot markets, budgets
    Compute,        // CPU, GPU, memory, WASI runtimes
    Network,        // connectivity, DNS, TLS, routing, mesh
    Storage,        // volumes, caches, replication, backups
    Security,       // secrets, certificates, policies
    Identity,       // authentication, authorization, RBAC
    Observability,  // metrics, logs, traces, alerting
    Regulatory,     // compliance frameworks, data residency, audit
}
```

Each substrate is its own convergence DAG. Cross-substrate edges enforce
ordering. The overall distance is a vector (max component, not average) —
convergence requires ALL substrates at zero.

### Dimension 4: Coordination — How Do Nodes Agree?

```rust
enum CalmClassification {
    Monotone,       // gossip — no coordination needed (CALM theorem)
    NonMonotone,    // Raft — leader-coordinated consensus required
}
```

This determines whether the convergence mechanism is eventually consistent
(O(log N) gossip rounds) or linearizable (Raft leader write). Maximizing
monotone operations maximizes what can be distributed without coordination.

### Dimension 5: Trust — When Is Compliance Verified?

```rust
enum VerificationPhase {
    PlanTime,           // static analysis of DAG structure (zero cost)
    AtBoundary,         // inline with convergence boundary
    PostConvergence,    // live probes on running state
}
```

Controls bind to point types, not instances. "All Security substrate points
must satisfy NIST AC-6" is a type-level constraint verified at the phase
specified by the binding.

### Dimension 6: Intelligence — Who or What Drives Convergence?

```rust
enum ComputationMode {
    /// Deterministic, no AI. A script, a function, a reconciler.
    /// Fully automated, fully reproducible.
    Mechanical,

    /// An LLM participates through MCP or API interfaces.
    /// Stochastic — outputs need stronger verification.
    AiAssisted {
        role: AiRole,
        interface: AiInterface,
    },

    /// Mechanical execution with AI participating at specific
    /// boundary phases. Most common production pattern.
    Hybrid {
        mechanical_phases: Vec<BoundaryPhase>,  // e.g., [Execute]
        ai_phases: Vec<BoundaryPhase>,          // e.g., [Verify]
    },
}

enum AiRole {
    Observer,    // reads convergence state, produces analysis
    Advisor,     // recommends actions, system/human decides
    Actor,       // takes bounded actions within emission catalogs
    Verifier,    // reviews convergence correctness, attests
    Reporter,    // generates compliance/performance reports
}

enum AiInterface {
    Mcp,         // Model Context Protocol — structured tool access
    Rest,        // REST API — programmatic access
    GraphQl,     // GraphQL — query-based access
    Grpc,        // gRPC — high-performance streaming access
}
```

This dimension determines whether a convergence point is driven by
deterministic code, by an LLM, or by a mix of both. The six dimensions
are fully orthogonal — any combination is valid:

- Bounded + Transform + Compute + Monotone + AtBoundary + **Mechanical**
  = a health check (deterministic, automated, no AI)
- Asymptotic + Select + Financial + NonMonotone + PlanTime + **AiAssisted(Advisor)**
  = an AI cost optimizer that recommends substrate migrations
- Bounded + Gate + Regulatory + NonMonotone + AtBoundary + **Hybrid(Execute:Mechanical, Verify:AI)**
  = a compliance gate where execution is mechanical but an LLM reviews the result

### Point Identity

A convergence point is uniquely identified by:

```
PointId = blake3(
    convergence_function_hash,
    input_attestation_hashes...,
    desired_state_hash,
)
```

Same inputs + same function = same point ID = cache hit. This is the Nix store
path formula applied to convergence.

---

## 3. The Five Invariants

These are the axioms of the system. They are ALWAYS true, without exception.
Together they define the boundary of the reliability envelope.

### Invariant 1: Every Operation Is a Convergence Point

No operation exists outside the convergence model. Infrastructure provisioning,
health checks, compliance verification, cost optimization, secret resolution,
certificate rotation, business metrics — all are convergence points with
distance, rate, and boundary.

This means: everything is observable, everything is attestable, everything has
known dependencies, and everything can be queried through the convergence store.

### Invariant 2: Every Convergence Point Has a Typed Atomic Boundary

No convergence point can skip its boundary phases:

```
Prepare → Execute → Verify → Attest
```

Prepare validates inputs and attestations from upstream. Execute drives
convergence. Verify proves output correctness. Attest produces a BLAKE3
hash that feeds downstream. The boundary cannot be bypassed or forged.

### Invariant 3: Every Attestation Is a Content-Addressed Store Path

Every boundary attestation is a store path in sui:

```
/nix/store/<blake3-hash>-<point-name>-gen<N>
```

This means: attestations are verifiable (recompute the hash), cacheable (same
inputs → same hash → skip), distributable (sui-cache binary cache), and
garbage-collectable (unreferenced generations are pruned).

### Invariant 4: Compliance Binds to Types and Is Verified Before or During Execution

Compliance controls are never only post-hoc. Every control is bound to a
convergence point type (via PointSelector) and verified at a declared phase:

- **PlanTime**: verified on the derivation graph before any execution
- **AtBoundary**: verified inline during the convergence boundary
- **PostConvergence**: verified live, but always ALSO backed by a plan-time
  or at-boundary check (defense in depth, never alone)

This means: violations are caught at the earliest possible moment, and the
compliance posture of any convergence DAG is computable before execution.

### Invariant 5: The Dependency Closure Is Statically Computable

At plan time, before any execution, the system knows:

- **Forward closure**: every point that must converge before this one
- **Reverse closure**: every point that depends on this one
- **Impact**: if this point re-converges, what else must re-converge
- **Compliance closure**: every control that applies to this DAG
- **Critical path**: the longest sequential chain across all substrates
- **Cache hits**: which points can skip re-execution

This means: the complete cost, blast radius, and compliance posture of any
operation are known before it begins. No surprises at execution time.

---

## 4. The Reliability Envelope

The five invariants create a **reliability envelope** — the set of guarantees
that hold for any system expressed within the territory:

| Guarantee | What It Means | What Provides It |
|-----------|--------------|-----------------|
| **Liveness** | Any declared state WILL be converged to | Goal-seeking convergence functions + level-triggered ticks |
| **Integrity** | Every step is cryptographically proven | Tameshi BLAKE3 Merkle chains + content-addressed store |
| **Compliance** | Every control is verified at the right phase | Type-level compliance bindings + kensa frameworks |
| **Safety** | Failures are detected and compensated | Convergence boundaries + saga pattern + circuit breakers |
| **Visibility** | Complete state is queryable at all times | Convergence store + closure queries + triple API |
| **Determinism** | Same inputs produce same plan | Content-addressed derivations (like Nix builds) |
| **Incrementality** | Only what changed re-converges | Content-addressed attestations → cache hits |
| **Composability** | Any DAG can be a node in a larger DAG | Closed DAG algebra + DAGs-of-DAGs |

These guarantees hold regardless of:
- What substrate the workload runs on (cloud, bare-metal, WASI, K8s)
- How many nodes are in the cluster (1 to thousands)
- What compliance frameworks are applied (NIST, SOC2, FedRAMP, PCI, any)
- Whether points are bounded or asymptotic
- Whether the DAG spans one system or many

---

## 5. The Territory of Reliable Expression

The **territory** is the set of all computations that can be reliably expressed
in this system. It is defined by the five invariants and bounded by the
theoretical frontiers.

### 5.1 What Is Within the Territory

Everything that can be described as `(Desired, Observed, Function, Horizon)`
falls within the territory:

| Domain | Example | How It's Expressed |
|--------|---------|-------------------|
| **Infrastructure** | Deploy a microservice | Bounded DAG: NixEval → Schedule → Execute → Health |
| **Multi-cloud** | Run on AWS + GCP | DAG-of-DAGs: one sub-DAG per cloud, same archetypes |
| **Cost optimization** | Minimize spend | Asymptotic point on Financial substrate, emits migration DAGs |
| **Compliance** | Satisfy NIST 800-53 | Compliance package applied to convergence graph |
| **Security** | Rotate certificates | Bounded DAG emitted by asymptotic Security substrate point |
| **Observability** | Ship metrics + logs | Observability substrate DAG, runs in parallel |
| **Business operations** | SaaS platform | Top-level asymptotic point emitting bounded operational DAGs |
| **Multi-org** | Federated deployments | Federated convergence stores with cross-org attestation |
| **Mixed workloads** | GPU + CPU + WASI | Compute substrate DAG with heterogeneous drivers |
| **Self-optimization** | DAG restructuring | Meta-convergence (future: when primitives are solid) |
| **Human workflows** | Approval gates | Bounded point with `mechanism: Manual`, unbounded wait |
| **Data pipelines** | ETL orchestration | Bounded DAG with Fork/Join/Reduce point types |
| **Disaster recovery** | Failover + restore | Preemption safety pattern (section 4.6) |
| **Regulatory audit** | Prove compliance | Compliance closure + attestation chain = audit trail |

### 5.2 What Is Outside the Territory

The theoretical frontiers (section 12 of the convergence theory) mark the edges:

| Frontier | Why It's Outside | Status |
|----------|-----------------|--------|
| **Autonomous goal generation** | Goals are always human-specified in Nix | Deliberate design choice |
| **Faster-than-observation convergence** | Can't converge faster than you can observe | Fundamental (Nyquist) |
| **Self-bootstrapping** | Convergence engine needs an external initial condition | Open problem |

Everything between the invariants and these frontiers is reliably expressible.

### 5.3 Extending the Territory

The territory grows in two ways:

1. **New substrate types**: adding a new SubstrateType extends what dimensions
   can be converged. The algebra, invariants, and compliance bindings work
   unchanged — only new convergence functions are needed.

2. **New compliance packages**: adding a new compliance framework (as a Nix
   expression) extends what can be verified. The framework binds to existing
   point types — no engine changes needed.

3. **New backend renderers**: adding a new renderer to the infrastructure
   theory extends what targets can be deployed to. The convergence DAG is
   the same — only the execution driver changes.

4. **New convergence point types**: adding a new structural type to the
   algebra (if needed) extends how data can flow. The algebra must remain
   closed — any composition of valid types must produce a valid DAG.

---

## 6. The Software Architecture

### 6.1 Component Map

```
┌─────────────────────────────────────────────────────────────────┐
│                         DECLARE                                 │
│                                                                 │
│  substrate/lib/infra/                                           │
│    workload-archetypes.nix     7 abstract archetypes            │
│    renderers/*.nix             K8s, tatara, WASI, compose       │
│    policies.nix                Governance at render time         │
│                                                                 │
│  substrate/lib/kube/                                            │
│    primitives/                 29 pure K8s resource builders     │
│    compositions/               9 service archetypes              │
│                                                                 │
│  User's flake.nix              Workload + compliance packages   │
└─────────────────────────────────┬───────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────────┐
│                           PLAN                                  │
│                                                                 │
│  sui-eval          Nix evaluator (tree-walk + bytecode VM)      │
│  sui-compat        Store paths, derivations, content-addressing │
│  sui-bytecode      8B NaN-boxed VM, 100+ opcodes                │
│  sui (API)         REST + GraphQL + gRPC                        │
│                                                                 │
│  + convergence builtins (planned)                               │
│    builtins.convergencePoint                                    │
│    builtins.convergenceDAG                                      │
│    builtins.convergenceGraph                                    │
│    builtins.compliancePackage                                   │
└─────────────────────────────────┬───────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────────┐
│                           GATE                                  │
│                                                                 │
│  tameshi           BLAKE3 Merkle attestation (1446 tests)       │
│                    24 LayerTypes, CertificationArtifact          │
│                    Two-phase signatures, heartbeat chain         │
│                    DFC signing (Akeyless), forensic ledger       │
│                                                                 │
│  kensa             14 compliance frameworks (538 tests)          │
│                    ComplianceRunner/Store/SignatureComputer       │
│                    NIST 800-53, SOC2, FedRAMP, PCI, OSCAL, ...  │
│                                                                 │
│  sekiban           K8s admission webhook (365 tests)             │
│                    SignatureGate, Certification, CompliancePolicy│
│                    LayerHashCollector, federation                │
│                                                                 │
│  inshou            Nix rebuild gate (81 tests)                   │
│                    NixStoreOps, VerificationClient               │
│                                                                 │
│  pangea-arch.      14-layer network stack (358 RSpec tests)      │
│                    Zero-cost compliance on synthesis output      │
└─────────────────────────────────┬───────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────────┐
│                          EXECUTE                                │
│                                                                 │
│  tatara-core       Domain types: convergence state, boundaries  │
│                    ConvergenceDistance, ConvergencePoint,        │
│                    BoundaryPhase, CalmClassification             │
│                                                                 │
│  tatara-engine     Convergence engine, Raft, gossip, scheduler  │
│                    7 drivers (exec, OCI, Nix, WASI, kube, ...)  │
│                    Health probes, port allocator, catalog        │
│                                                                 │
│  tatara-api        REST (Axum) + GraphQL (async-graphql)        │
│  tatara-cli        CLI: job/node/alloc/source/event/release     │
│  tatara-kube       Nix-native K8s reconciler (SSA)              │
│  tatara-net        Networking plane (eBPF, WASI, mesh)          │
│  tatara-operator   K8s operator (FlakeSource, NixBuild CRDs)   │
└─────────────────────────────────┬───────────────────────────────┘
                                  ↓
┌─────────────────────────────────────────────────────────────────┐
│                          STORE                                  │
│                                                                 │
│  sui-store         Store trait (async, pluggable backends)       │
│                    LocalStore (fs + SQLite), BinaryCacheStore    │
│                    + ConvergenceStore trait (planned)            │
│                                                                 │
│  sui-build         Builder trait (async, pluggable)              │
│                    LocalBuilder, + ConvergenceBuilder (planned)  │
│                                                                 │
│  sui-cache         Binary cache server (Axum, S3/object_store)  │
│  sui-daemon        Nix daemon replacement (Unix socket)         │
│  sui-orchestrate   System rebuild, fleet deployment             │
└─────────────────────────────────────────────────────────────────┘
```

### 6.2 Data Flow

```
User writes flake.nix
  ↓
sui evaluates Nix → convergence derivation graph (PointIds)
  ↓
sui computes closures + binds compliance controls
  ↓
Plan-time compliance: kensa verifies static controls (zero cost)
  ↓
For each convergence point in topological order:
  ↓
  tatara PREPARE: verify input attestations + preconditions
    ↓
  At-boundary compliance: kensa verifies inline controls
    ↓
  tatara EXECUTE: drive convergence function C(D, O) → O'
    ↓
  tatara VERIFY: check postconditions
    ↓
  tameshi ATTEST: produce CertificationArtifact
    ↓
  sui STORE: write attestation as content-addressed store path
    ↓
  Gate opens → next point can PREPARE
  ↓
Post-convergence: InSpec verifies live state
  ↓
sui-cache distributes attestations across cluster
  ↓
Convergence store queryable via sui triple API
```

### 6.3 The Two Halves of the Convergence Computer

Sui and tatara are two halves of one machine:

| Concern | Sui | Tatara |
|---------|-----|--------|
| **Role** | The store + evaluator + planner | The engine + executor |
| **Analogy** | The compiler + filesystem | The CPU + runtime |
| **Input** | Nix expressions | Convergence derivations |
| **Output** | Derivation graph + store paths | Attested convergence state |
| **State** | Content-addressed store (immutable) | Live convergence state (mutable) |
| **Distribution** | sui-cache binary cache | Raft + gossip |
| **API** | REST + GraphQL + gRPC | REST + GraphQL + SSE |

Together: sui computes the plan, tatara executes it, sui stores the results.

---

## 7. The Complete Type Hierarchy

### 7.1 Core Convergence Types

```
ConvergencePoint<I, O>
├── id: PointId                      # blake3(inputs + function)
├── point_type: ConvergencePointType # Transform | Fork | Join | ...
├── horizon: ConvergenceHorizon      # Bounded | Asymptotic
├── substrate: SubstrateType         # Financial | Compute | ...
├── calm: CalmClassification         # Monotone | NonMonotone
├── inputs: Vec<TypedEdge<I>>        # upstream dependencies
├── outputs: Vec<TypedEdge<O>>       # downstream emissions
├── convergence: Fn(D, O) → O'      # the convergence function
├── boundary: ConvergenceBoundary    # prepare → execute → verify → attest
├── state: ConvergenceState          # distance + rate + oscillation + damping
├── compliance: Vec<ComplianceBinding> # bound controls + verification phase
└── emission_schema: Option<EmissionSchema>  # (asymptotic only)
```

### 7.2 State Types

```
ConvergenceState
├── distance: ConvergenceDistance     # Converged | Partial | Diverged | Unknown
├── rate: f64                        # negative = converging, positive = diverging
├── oscillating: bool                # rate alternating sign
├── damping: f64                     # backoff factor (1.0 = normal)
├── ticks: u64                       # convergence iterations applied
├── time_to_divergence: Option<Duration>  # proactive convergence trigger
└── horizon_health: HorizonHealth    # Healthy | Stalled | Deteriorating
```

### 7.3 Boundary Types

```
ConvergenceBoundary
├── preconditions: Vec<BoundaryCheck>
├── postconditions: Vec<BoundaryCheck>
├── input_attestation: Option<Blake3Hash>    # from upstream point
├── output_attestation: Option<Blake3Hash>   # produced after verify
├── certification: Option<CertificationArtifact>  # tameshi three-pillar
└── phase: BoundaryPhase                     # Pending → ... → Attested | Failed
```

### 7.4 Compliance Types

```
ComplianceBinding
├── selector: PointSelector          # what point types this applies to
├── control: ComplianceControl       # framework + control ID
├── phase: VerificationPhase         # PlanTime | AtBoundary | PostConvergence
└── runner: ComplianceRunnerRef      # kensa runner that evaluates this

CompliancePackage                    # Nix expression packaging a framework
├── framework: String                # "nist-800-53", "soc2", "fedramp"
├── baseline: String                 # "moderate", "type2", etc.
├── controls: Vec<ComplianceControl>
├── bindings: Vec<ComplianceBinding>
└── runners: HashMap<VerificationPhase, ComplianceRunnerRef>

ComplianceClosure                    # computed at plan time
├── dag: ConvergenceGraph
├── bindings: Vec<(PointId, ComplianceControl, VerificationPhase)>
├── plan_time_verifiable: usize      # count of zero-cost checks
├── cache_hits: usize                # controls already attested
└── new_verifications: usize         # controls needing fresh verification
```

### 7.5 Graph Types

```
ConvergenceGraph
├── points: HashMap<PointId, ConvergencePoint>
├── edges: Vec<TypedEdge>            # Data | Control | Attestation
├── substrates: HashMap<SubstrateType, SubstrateDAG>
├── plan: ConvergencePlan            # static analysis results
├── compliance: ComplianceClosure    # all bound controls
└── distance: MultiDimensionalDistance  # per-substrate vector

SubstrateDAG
├── substrate: SubstrateType
├── points: Vec<PointId>             # points in this substrate
├── bandwidth: ConvergenceBandwidth  # velocity limit for this substrate
└── cross_edges: Vec<TypedEdge>      # edges to other substrates
```

### 7.6 Attestation Types

```
CertificationArtifact               # tameshi three-pillar binding
├── artifact_hash: Blake3Hash        # convergence function + binary
├── control_hash: Blake3Hash         # compliance verification results
├── intent_hash: Blake3Hash          # Nix-declared desired state
├── composed_root: Blake3Hash        # Merkle root of all three
└── proof_paths: ArtifactProofPaths  # verification paths

Generation                           # convergence re-attestation
├── point_id: PointId
├── generation: u64                  # monotonic counter
├── attestation: CertificationArtifact
├── previous: Option<Blake3Hash>     # chain to prior generation
└── store_path: StorePath            # content-addressed location
```

---

## 8. Maximizing the Territory

### 8.1 The Principle of Maximum Bounded Expression

The system maximizes its territory of reliable expression through a
hierarchy of preferences:

1. **Prefer bounded over asymptotic** — bounded points are predictable,
   testable, cacheable, composable. Maximize them. Asymptotic points
   exist only when genuinely unbounded.

2. **Prefer plan-time over runtime verification** — every control that
   CAN be verified at plan time SHOULD be. This catches violations before
   any resources are provisioned, at zero cost.

3. **Prefer content-addressed over mutable state** — content-addressing
   gives deduplication, caching, and verification for free. Every
   attestation, every compliance result, every convergence plan is a
   store path.

4. **Prefer type-level over instance-level** — binding compliance to
   types instead of instances means entire categories of computing are
   gated automatically. New instances inherit their type's controls.

5. **Prefer closed algebra over ad hoc composition** — the DAG algebra
   is closed: any composition of valid points produces a valid DAG. This
   means the system never encounters a composition it can't handle.

### 8.2 How Territory Expands

The territory is not fixed — it grows as new types are added:

```
Territory = f(
  substrate_types,      # add new dimensions to converge
  compliance_packages,  # add new frameworks to verify
  backend_renderers,    # add new targets to deploy to
  point_types,          # add new data flow patterns (keep algebra closed)
  emission_schemas,     # add new bounded DAG templates to catalogs
)
```

Each addition extends the territory without changing the invariants or the
pipeline. The five invariants are the constitutional law. New types and
packages are legislation under that constitution.

### 8.3 The Expression Completeness Argument

The system is **expression-complete** for convergent computation:

1. Any desired state expressible in Nix can be declared (infinite declaration space)
2. Any convergence function can be packaged as WASI (infinite function space)
3. Any compliance framework can be packaged as a Nix expression (infinite policy space)
4. The DAG algebra composes arbitrarily (infinite structural space)
5. DAGs-of-DAGs nest to any depth (infinite compositional space)

The only limits are the theoretical frontiers (autonomous goals, observation
rate, bootstrapping). Within those limits, if you can describe desired state,
observe current state, and write a function to drive one toward the other,
it's in the territory.

---

## 9. The Intent/Outcome Duality

Every convergence graph has two fundamental leaf types. They are duals of each
other — opposite ends of the same computation.

### 9.1 Outcome Leaves (Bounded)

An outcome leaf is a convergence point at the terminus of a DAG. It has a
**definitive end state** — a concrete condition that, when matched, means
`converged = true`:

- "The container is running" — yes/no
- "The health check passes" — yes/no
- "The migration completed" — yes/no
- "The certificate is valid" — yes/no
- "The compliance control is satisfied" — yes/no

Outcome leaves are **verifiable**. You can assert their end state. You can
test them. You can cache their attestation. They terminate.

### 9.2 Intent Leaves (Asymptotic)

An intent leaf is a convergence point at the root of a DAG. It represents
a **perpetually running purpose** — the reason the system exists:

- "Minimize infrastructure cost"
- "Maximize revenue"
- "Harden security posture"
- "Serve customers with < 100ms latency"
- "Maintain SOC2 compliance"

Intent leaves never terminate. They run forever. As they run, they produce
outcome leaves — bounded DAGs that achieve concrete results. The intent
leaf is the factory; outcome leaves are the products.

### 9.3 The Duality

```
Intent (root, asymptotic, never ends)
  │
  ├──→ produces Outcome DAG A (bounded, ends at converged = true)
  ├──→ produces Outcome DAG B (bounded, ends at converged = true)
  ├──→ produces Outcome DAG C (bounded, ends at converged = true)
  │         │
  │         └── each outcome's attestation feeds back into intent
  │
  └──→ consumes completed outcomes → adjusts strategy → produces more

The duality:
  Intent  = direction without destination (the WHY)
  Outcome = destination without direction (the WHAT, verified)

  Intent mirrors the perpetually running business/optimizer/policy engine.
  Outcome mirrors the concrete, verifiable, cacheable result.

  Intent produces outcomes.
  Outcomes feed back into intent.
  The cycle never ends. The business runs.
```

Every convergence graph, no matter how complex, is a tree with intent leaves
at the roots and outcome leaves at the terminals. The interior is the
computation that translates purpose into results.

### 9.4 Verification Flows from Outcomes

Verification is an outcome-side concern. Every outcome leaf has a definitive
end state that can be checked:

| Concern | Outcome Leaf | End State |
|---------|-------------|-----------|
| **Expression** | "Resource exists in declared form" | Nix desired state matches observed |
| **Provability** | "Attestation chain is valid" | BLAKE3 Merkle root verifiable |
| **Compliance** | "Control is satisfied" | Kensa framework check passes |
| **Performance** | "Latency is within SLA" | Measured value ≤ threshold |
| **Observability** | "Signals are flowing" | Metrics/logs/traces arriving at sinks |

All five concerns — expression, provability, compliance, performance,
observability — are substrate types with outcome leaves. They're all expressed
in the same framework, all verified the same way, all attestable, all
composable.

---

## 10. The Absorption Principle

### 10.1 Every External System Becomes Convergence Points

When the platform encounters a new system — a cloud provider, a SaaS API, a
legacy database, a compliance framework, a monitoring tool — it doesn't
integrate with it. It **absorbs** it. The external system becomes convergence
points in the graph.

```
External System                    Absorbed Into Platform
─────────────────                  ──────────────────────
AWS EC2                    →       Compute substrate convergence points
                                   (instance_running, security_group_applied, ...)

Stripe billing             →       Financial substrate convergence points
                                   (payment_processed, subscription_active, ...)

Datadog monitoring         →       Observability substrate convergence points
                                   (dashboard_configured, alert_rule_active, ...)

NIST 800-53               →       Compliance package (Nix expression)
                                   (control_AU-2_satisfied, control_AC-6_verified, ...)

WireGuard VPN              →       Network substrate convergence points
                                   (tunnel_established, peer_authenticated, ...)

PostgreSQL                 →       Storage substrate convergence points
                                   (migration_applied, replica_synced, ...)
```

### 10.2 The Absorption Pattern

Absorbing a system follows a standard pattern:

```
1. OBSERVE   — Write an observer that polls the external system's state
               (API client, CLI wrapper, SDK binding)

2. TYPE      — Map the system's resources to convergence point types
               and classify along the five dimensions
               (substrate, structure, horizon, coordination, trust)

3. CONVERGE  — Write convergence functions that drive the external
               system from observed → desired state
               (API calls, CLI commands, SDK operations)

4. ATTEST    — The convergence boundary attestation proves the external
               system's state matches desired state
               (tameshi BLAKE3 hash of observed state)

5. COMPLY    — Bind compliance controls to the new convergence point types
               (compliance package maps controls to new types)

6. PACKAGE   — Package the absorption as a Nix expression
               (substrate definition + convergence functions + compliance bindings)
```

After absorption, the external system is indistinguishable from any other
substrate. The platform schedules, observes, converges, attests, and verifies
it using the same pipeline that handles everything else.

### 10.3 Why Absorption Maximizes Territory

Every absorbed system expands the territory of reliable expression:

```
Territory(t+1) = Territory(t) + absorbed_system_capabilities

Each absorption adds:
  + new convergence point types (what can be converged)
  + new substrate dimensions (what can be optimized)
  + new compliance bindings (what can be verified)
  + new emission templates (what bounded DAGs can be produced)
```

The cost of absorption is writing the observer + convergence functions. The
benefit is that the entire platform's guarantees (liveness, integrity,
compliance, safety, visibility, determinism, incrementality, composability)
immediately apply to the absorbed system.

### 10.4 Absorption Compositions

Absorbed systems compose freely because they share the convergence type system:

```nix
# After absorbing AWS, Stripe, and Datadog:
myService = builtins.convergenceGraph {
  substrates = {
    compute = awsEc2Substrate { instanceType = "t3.large"; };
    financial = stripeSubstrate { plan = "pro"; billingCycle = "monthly"; };
    observability = datadogSubstrate { dashboards = [ "./dashboards/" ]; };
    security = akeylessSubstrate { secrets = [ "db-password" ]; };
  };
  compliance = [ nist-800-53-moderate soc2-type2 ];
};
```

AWS, Stripe, Datadog, and Akeyless were separate systems. After absorption,
they're substrate DAGs in a single convergence graph with a unified compliance
posture. A single `tatara query --compliance-closure myService` shows every
control across every absorbed system.

---

## 11. The Convergence Optimizer as Independent System

### 11.1 Separation of Concerns

The system that optimizes convergence DAGs is **separate** from the DAGs
it optimizes. This is critical:

```
┌──────────────────────────────────────────────────────┐
│                  Convergence Optimizer                │
│              (independent, observes all)              │
│                                                      │
│  Observes: DAG execution telemetry, convergence      │
│            rates, attestation cache hits, resource    │
│            utilization, substrate costs               │
│                                                      │
│  Produces: DAG restructuring recommendations         │
│            (fuse, parallelize, cache, reorder)        │
│                                                      │
│  Constraint: NEVER changes what converges,           │
│              ONLY how it converges                    │
└──────────────┬───────────────────────────────────────┘
               │ observes ↑     │ recommends ↓
┌──────────────┴───────────────────────────────────────┐
│              Convergence Engine (tatara)              │
│                                                      │
│  Executes convergence DAGs as planned                │
│  Applies optimizer recommendations (if safe)         │
│  Reports telemetry back to optimizer                 │
└──────────────────────────────────────────────────────┘
```

The optimizer is:
- **Independent**: it runs as its own process/service, not embedded in tatara
- **Read-mostly**: it observes the convergence store, it doesn't mutate it
- **Advisory**: it produces recommendations, not commands. Tatara applies
  them only if safety checks pass (attestation preservation, CALM preservation,
  horizon preservation)
- **Itself a convergence point**: the optimizer is an asymptotic intent leaf
  on the Compute substrate, continuously optimizing how the system converges

### 11.2 What the Optimizer Can Do

| Optimization | Input | Output | Safety Check |
|-------------|-------|--------|-------------|
| **Fuse** | Two sequential points that always co-execute | One merged point | Attestation equivalence |
| **Parallelize** | Two sequenced points with no real dependency | Parallel execution | DAG validity |
| **Cache** | Point with unchanged inputs across generations | Skip re-execution | Attestation match |
| **Reorder** | DAG with suboptimal evaluation order | Reordered DAG | Same convergence result |
| **Pre-instantiate** | Predictable emission pattern | Pre-created bounded DAGs | Template match |
| **Prune** | Dead convergence points (never diverge) | Removed from active graph | No downstream deps |

### 11.3 What the Optimizer Cannot Do

The optimizer has hard boundaries:
- Cannot change WHAT converges (desired state is declared in Nix by humans)
- Cannot change compliance bindings (controls are type-level, declared in packages)
- Cannot change convergence horizons (bounded stays bounded, asymptotic stays asymptotic)
- Cannot change CALM classification (monotone stays monotone)
- Cannot break attestation chains (every optimization must produce equivalent attestations)
- Cannot create new intent leaves (goals are human-specified)

The optimizer makes the system faster and cheaper. It does not change what the
system does or what it proves.

---

## 12. AI-Assisted Convergence Computing

### 12.1 The Differentiation

Every convergence point in the graph falls on a spectrum:

```
Mechanical ◄──────────────────────────────────────► AI-Assisted
(deterministic)          (hybrid)              (LLM-driven)

deploy container    AI reviews compliance     AI diagnoses oscillation
rotate certificate  AI verifies attestation   AI recommends migration
apply K8s manifest  mechanical + AI gate      AI writes convergence plan
health check        AI-reviewed audit report  AI analyzes cost patterns
```

This is not a binary — it's a continuum. The boundary phases within a single
convergence point can independently be mechanical or AI-assisted:

```
┌──────────────────────────────────────────────────────────┐
│ Convergence Point: production-deploy                      │
│                                                          │
│ PREPARE:  Mechanical  (verify input attestations)        │
│ EXECUTE:  Mechanical  (apply K8s manifest via SSA)       │
│ VERIFY:   AI-Assisted (LLM reviews deployment health,   │
│                        analyzes logs for anomalies)      │
│ ATTEST:   Mechanical  (produce BLAKE3 attestation)       │
│                                                          │
│ Mode: Hybrid { mechanical: [Prepare, Execute, Attest],   │
│                ai: [Verify] }                             │
└──────────────────────────────────────────────────────────┘
```

### 12.2 The MCP Surface: AI's Interface to the Convergence Ether

MCP (Model Context Protocol) is the AI-native interface to the convergence
graph. It gives LLMs structured, typed, purpose-built tool access — not raw
database queries, but curated operations that map directly to the theory's
concepts.

The MCP surface is organized into five categories:

#### 12.2.1 Observe — Read Convergence State

These tools give AI visibility into the convergence ether:

| MCP Tool | Returns | Maps To |
|----------|---------|---------|
| `convergence_graph` | Full typed DAG across all substrates | §5, §6 ConvergenceGraph |
| `convergence_distance` | Per-substrate distance vector | §6.3 MultiDimensionalDistance |
| `convergence_rate` | Per-point and cluster-wide rates | §2.2 ConvergenceState.rate |
| `convergence_plan` | Pre-execution plan for a workload | §7.1 ConvergencePlan |
| `convergence_closure` | Forward/reverse/impact closure | §7.2 Closure queries |
| `compliance_closure` | All bound controls for a DAG | §9.6 ComplianceClosure |
| `attestation_history` | Generation chain for a point | §8.3.3 Generational store paths |
| `substrate_status` | Per-substrate DAG health summary | §6.4 SubstrateDAG |
| `emission_catalog` | Available bounded DAG templates | §1.6 EmissionSchema |
| `cluster_health` | Bounded converged + asymptotic rates | §1.6 ClusterHealth |

#### 12.2.2 Analyze — Reason About Convergence

These tools enable AI to perform deeper analysis:

| MCP Tool | Purpose | AI Role |
|----------|---------|---------|
| `diagnose_oscillation` | Explain why a point is oscillating, recommend damping | Advisor |
| `identify_bottleneck` | Find slowest convergence path across substrates | Advisor |
| `compliance_gap_analysis` | Identify unbound controls or schema gaps | Advisor |
| `cost_opportunity` | Find cheaper substrate alternatives for running workloads | Advisor |
| `blast_radius` | Show impact of a point failing or re-converging | Advisor |
| `convergence_anomaly` | Detect unusual convergence patterns or rates | Observer |
| `dependency_risk` | Identify fragile dependency chains in the DAG | Advisor |
| `substrate_contention` | Find conflicting optimization directions between substrates | Advisor |

#### 12.2.3 Influence — Take Bounded Actions

These tools let AI affect convergence within strict safety bounds:

| MCP Tool | Action | Constraints |
|----------|--------|-------------|
| `emit_bounded_dag` | Instantiate a bounded DAG from emission catalog | Must match existing template |
| `adjust_substrate_priority` | Change priority ordering between substrates | Soft substrates only |
| `defer_convergence` | Pause a convergence point temporarily | Bounded duration, logged |
| `resume_convergence` | Unpause a deferred point | Must have been deferred |
| `escalate_schema_gap` | Flag a pattern needing a new bounded template | Advisory only |
| `recommend_dag_optimization` | Suggest restructuring to the optimizer | Advisory, optimizer decides |
| `trigger_re_convergence` | Force a point to re-evaluate | Must be idempotent |

**Hard safety boundaries for AI actors:**
- Cannot modify desired state (declared in Nix by humans)
- Cannot modify compliance bindings (declared in compliance packages)
- Cannot change convergence horizons (bounded stays bounded)
- Cannot bypass attestation boundaries
- Cannot create new intent leaves (goals are human-specified)
- Can only instantiate from known emission catalogs (no improvisation)
- All actions are attested and auditable through the same BLAKE3 chain

#### 12.2.4 Verify — AI-Assisted Verification

These tools let AI participate in the trust layer:

| MCP Tool | Purpose | Phase |
|----------|---------|-------|
| `verify_convergence_plan` | Review a plan before execution for risks | PlanTime |
| `verify_attestation_chain` | Validate attestation integrity for a DAG | AtBoundary |
| `verify_compliance_posture` | Review compliance bindings for completeness | PlanTime |
| `verify_convergence_result` | Analyze convergence output for correctness | PostConvergence |
| `verify_security_posture` | Review security substrate for vulnerabilities | PostConvergence |

AI verification is **additive** — it adds a verification layer ON TOP of
mechanical verification, never replaces it. The attestation chain always
includes both mechanical and AI verification results.

#### 12.2.5 Report — Compliance and Performance Reporting

These tools let AI generate and maintain reports:

| MCP Tool | Output | Audience |
|----------|--------|----------|
| `compliance_report` | Framework-specific report (NIST, SOC2, FedRAMP) | Auditors |
| `convergence_health_report` | Overall system convergence status | Operations |
| `cost_optimization_report` | Financial substrate analysis + recommendations | Finance |
| `attestation_audit_trail` | Trace attestation chain for a specific operation | Compliance |
| `incident_analysis` | Analyze a divergence event with root cause | Engineering |
| `performance_trend` | Convergence rate trends over time by substrate | Management |

### 12.3 How AI Mixes Across the Landscape

AI-assisted and mechanical convergence mix freely across the entire
convergence graph. The mixing follows patterns:

#### Pattern 1: Mechanical Core, AI Edges

The most common production pattern. Core convergence is mechanical
(deterministic, fast, reproducible). AI participates at the edges —
observing, analyzing, recommending, reporting:

```
                      AI (MCP)
                    ┌─────────┐
                    │ observe │
                    │ analyze │
                    │ report  │
                    └────┬────┘
                         │ reads
                         ↓
┌────────────────────────────────────────────────┐
│        Mechanical Convergence Core              │
│                                                │
│  NixEval → Replicate → Schedule → Execute →    │
│  HealthCheck → CatalogRegister                 │
│  (all mechanical, deterministic)               │
└────────────────────────────────────────────────┘
                         │
                         ↓ writes
                    ┌─────────┐
                    │ AI (MCP)│
                    │ advise  │
                    │ verify  │
                    └─────────┘
```

#### Pattern 2: AI-in-the-Loop

AI participates at specific boundary phases within the convergence DAG.
Mechanical execution with AI verification gates:

```
Point A [PREPARE: mechanical] [EXECUTE: mechanical]
  [VERIFY: AI analyzes result via MCP] [ATTEST: mechanical]
    ↓
Point B [PREPARE: AI reviews plan via MCP] [EXECUTE: mechanical]
  [VERIFY: mechanical] [ATTEST: mechanical]
```

#### Pattern 3: AI-Driven Asymptotic Points

Asymptotic optimization points where the convergence function IS an LLM.
The AI continuously observes the convergence ether and emits bounded DAGs:

```
┌────────────────────────────────────────────────────┐
│ Asymptotic Point: cost-optimizer                    │
│ Mode: AiAssisted(Actor, Mcp)                        │
│                                                    │
│ LLM observes:                                      │
│   - Financial substrate distance                    │
│   - Spot market prices via absorption               │
│   - Convergence rates across compute substrate      │
│                                                    │
│ LLM decides (within emission catalog):              │
│   - emit_bounded_dag("substrate-migration", {...})  │
│   - emit_bounded_dag("horizontal-scale", {...})     │
│   - defer_convergence("expensive-node-drain")       │
│                                                    │
│ All emitted DAGs are mechanical (bounded, verified) │
└────────────────────────────────────────────────────┘
```

#### Pattern 4: AI Compliance Officer

An AI-assisted asymptotic point on the Regulatory substrate that continuously
monitors compliance posture:

```
┌────────────────────────────────────────────────────┐
│ Asymptotic Point: compliance-monitor                │
│ Mode: AiAssisted(Verifier, Mcp)                     │
│                                                    │
│ LLM continuously:                                   │
│   - compliance_closure() → reviews all bindings     │
│   - compliance_gap_analysis() → finds gaps          │
│   - verify_compliance_posture() → validates         │
│   - compliance_report() → generates for auditors    │
│                                                    │
│ When gap found:                                     │
│   - escalate_schema_gap("new-control-needed")       │
│   - emit_bounded_dag("remediation", {...})          │
│                                                    │
│ Rate: healthy when compliance_gap_count decreasing  │
└────────────────────────────────────────────────────┘
```

### 12.4 The Interface Hierarchy

The convergence ether is accessible through four interfaces, each serving
different consumers:

```
┌─────────────────────────────────────────────────────────────┐
│                  Convergence Ether                           │
│          (convergence state across all substrates)           │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │   MCP    │  │   REST   │  │ GraphQL  │  │   gRPC   │   │
│  │          │  │          │  │          │  │          │   │
│  │ AI/LLM  │  │ Programs │  │ UIs      │  │ Services │   │
│  │ agents   │  │ scripts  │  │ dashbds  │  │ internal │   │
│  │ Claude   │  │ curl     │  │ browsers │  │ tatara   │   │
│  │ assistnts│  │ SDKs     │  │ Grafana  │  │ engine   │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
│                                                             │
│  All four interfaces expose the SAME convergence state.     │
│  MCP is structured for AI reasoning (tools + context).      │
│  REST is structured for programmatic access (CRUD).         │
│  GraphQL is structured for flexible queries (UI-driven).    │
│  gRPC is structured for performance (streaming, internal).  │
│                                                             │
│  The interface does not change what's exposed — only HOW.   │
│  The convergence graph, attestations, closures, compliance  │
│  state, and distance vectors are the same through all four. │
└─────────────────────────────────────────────────────────────┘
```

### 12.5 AI Safety Within the Convergence Model

The convergence model provides natural safety boundaries for AI:

1. **Attestation chain is unforgeable**: every AI action goes through
   the same PREPARE → EXECUTE → VERIFY → ATTEST boundary. AI cannot
   skip verification or forge attestations. Its actions are
   cryptographically auditable.

2. **Emission catalogs constrain AI actors**: an AI Actor can only
   instantiate bounded DAGs from known templates. It cannot improvise
   new convergence flows. The catalog is defined in Nix by humans.

3. **Desired state is human-specified**: the Nix declarations that
   define WHAT the system converges toward are written by humans. AI
   influences HOW convergence proceeds, never WHAT it converges toward.

4. **Mechanical attestation wraps AI output**: even when an AI drives
   the EXECUTE phase, the ATTEST phase is always mechanical (BLAKE3
   hash computation). AI outputs are hashed into the attestation chain,
   not trusted blindly.

5. **Stochastic outputs get stronger verification**: AI-assisted points
   automatically receive additional verification. The theory recognizes
   that LLM outputs are stochastic — the boundary attestation becomes
   MORE important, not less.

6. **All AI actions are convergence points**: there is no "AI side
   channel." Every AI observation, analysis, action, and report is itself
   a convergence point with distance, rate, boundary, and attestation.
   AI is not outside the system — it is inside it, subject to the same
   five invariants.

### 12.6 Architectural Implications

Adding the Intelligence dimension has concrete architectural consequences:

| Concern | Implication |
|---------|------------|
| **MCP server** | Tatara exposes an MCP server implementing the 30+ tools described above. Built with kaname (pleme-io MCP scaffold library). |
| **Tool registry** | Each MCP tool maps to a specific convergence operation. Tools are curated — no raw store access, no untyped queries. |
| **AI attestation layer** | Tameshi LayerType gains AI-specific variants (AiVerification, AiRecommendation) to track AI's participation in the attestation chain. |
| **Computation mode in store** | Sui convergence derivations carry `computation_mode` in their env, so the store knows which points involved AI. |
| **Audit trail** | The heartbeat chain (tameshi) records AI decisions: which model, what it observed, what it recommended, what was enacted. |
| **Rate limiting** | AI Actor operations are rate-limited per emission catalog to prevent runaway DAG emission. |
| **Model versioning** | AI-assisted points record which model version drove convergence, for reproducibility and audit. |

---

## 13. Architecture Summary

```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│  Intent Leaves (asymptotic, perpetual, the WHY)              │
│    "minimize cost" · "maximize revenue" · "harden security"  │
│         │              │              │                       │
│         ├─→ bounded DAG ├─→ bounded DAG ├─→ bounded DAG     │
│         │              │              │                       │
│  ┌──────┴──────────────┴──────────────┴───────────────────┐  │
│  │  DECLARE  │  PLAN  │  GATE  │  EXECUTE  │  STORE       │  │
│  │ substrate │  sui   │tameshi │  tatara   │  sui store   │  │
│  │  + nix    │eval+vm │+ kensa │  engine   │  + attic     │  │
│  └──────────────────────────────────────────────────────────┘  │
│         │              │              │                       │
│         └─→ outcome ←──┘─→ outcome ←──┘─→ outcome            │
│                                                              │
│  Outcome Leaves (bounded, verified, the WHAT)                │
│    converged=true · attested · compliant · cached            │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │  Convergence Optimizer (independent, observes all)     │  │
│  │  Changes HOW, never WHAT. Advisory. Itself a leaf.     │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │  AI Layer (MCP + REST + GraphQL + gRPC)                │  │
│  │  Observes, analyzes, verifies, reports, acts within    │  │
│  │  emission catalogs. Mechanical + AI mix at any point.  │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  Five Invariants:                                            │
│    1. Every operation is a convergence point                 │
│    2. Every point has a typed atomic boundary                │
│    3. Every attestation is content-addressed                 │
│    4. Compliance binds to types, verified before execution   │
│    5. Dependency closure is statically computable            │
│                                                              │
│  Six Classification Dimensions:                              │
│    Horizon · Structure · Substrate · Coordination ·          │
│    Trust · Intelligence                                      │
│                                                              │
│  Eight Guarantees:                                           │
│    Liveness · Integrity · Compliance · Safety                │
│    Visibility · Determinism · Incrementality · Composability │
│                                                              │
│  Territory: everything expressible as (D, O, C, H)           │
│  Absorption: every external system becomes convergence       │
│  AI: mechanical + AI-assisted mix across entire landscape    │
│  Expansion: new types, packages, renderers extend territory  │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

## 14. Theory Cross-References

| Document | Location | Concerns |
|----------|----------|----------|
| Unified Infrastructure Theory | `substrate/docs/unified-infrastructure-theory.md` | Intent, archetypes, renderers, policies |
| Unified Convergence Computing Theory | `tatara/docs/unified-convergence-computing-theory.md` | Convergence, DAGs, substrates, horizons, store, compliance, meta-convergence, frontiers |
| Unified Platform Architecture | `tatara/docs/unified-platform-architecture.md` | (This document) Pipeline, invariants, duality, absorption, AI-assisted computing, territory, optimizer, type hierarchy |

The convergence computing theory document contains the full formal treatment
(13 sections, 22 principles, 35+ academic references). This architecture
document is the composition layer — how the theories, tools, and types fit
together into one machine that maximizes the territory of reliable expression.

AI-assisted convergence computing (section 12) establishes that intelligence
is a classification dimension, not a bolt-on. Mechanical and AI-driven
convergence mix at any point, at any boundary phase, across the entire
landscape. The MCP surface provides 30+ curated tools mapping directly to
convergence theory concepts. This creates a permanent architectural
expression of AI assisting computing as it happens — a bedrock on which
further formalization can build.
