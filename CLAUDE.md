# Tatara (粋) — Programmable Convergence Computer

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

## Workspace Crates (10)

| Crate | Purpose |
|-------|---------|
| `tatara-core` | Domain types: convergence state, WorkloadPhase, DAG, saga, idempotency, traced events |
| `tatara-engine` | Runtime: 7 drivers, Raft, gossip, convergence engine, scheduler, health probes, catalog, metrics, sui client |
| `tatara-api` | REST (Axum) + GraphQL (async-graphql): jobs, allocations, nodes, catalog, health, metrics |
| `tatara-cli` | CLI + `tatara server` |
| `tatara-kube` | Nix-native K8s reconciler: Server-Side Apply, dependency ordering, pruning |
| `tatara-net` | Networking plane: NetworkPlane trait, eBPF types, WASI types, mesh, flow observability |
| `tatara-operator` | K8s operator: NixBuild/FlakeSource/FlakeOrg CRDs, NATS JetStream bridge |
| `tatara-testing` | Test fixtures and helpers |
| `ro-cli` | Read-only CLI |

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
