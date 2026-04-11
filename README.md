# Tatara (粋) — Programmable Convergence Computer

A distributed computing platform where **convergence IS computation**.

Three theories compose into one platform:

| Theory | Question | Implemented By |
|--------|----------|---------------|
| **Unified Infrastructure Theory** | WHAT to compute | Nix + substrate (archetypes, renderers) |
| **Unified Convergence Computing Theory** | HOW to compute | Tatara (typed DAGs, boundaries, substrates) |
| **Compliant Computing Theory** | WHETHER to compute | Tameshi + kensa (attestation, compliance) |

Declare any system in Nix. Plan it as a typed convergence DAG. Gate it through
compliance verification. Compute it into existence through verified convergence
on any substrate. Prove every step cryptographically. Store attestations in the
Nix store. Mix mechanical and AI-assisted convergence at any point.

## Quick Start

```bash
tatara server                              # Start single-node cluster
tatara job run spec.json                   # Submit a workload
tatara job list                            # Monitor convergence
tatara top                                 # TUI dashboard
curl http://localhost:4646/metrics         # Prometheus metrics
curl http://localhost:4646/v1/catalog/services  # Service discovery
```

## The Five-Layer Pipeline

```
DECLARE  →  PLAN  →  GATE  →  EXECUTE  →  STORE
  Nix       sui     tameshi   tatara      sui store
substrate  eval+vm  + kensa   engine      + sui-cache
```

Each layer is itself convergence. Each layer's output feeds the next.

## The Convergence Computing Model

Every operation is a convergence point with four verified phases:

```
PREPARE → EXECUTE → VERIFY → ATTEST ──hash──→ next point
```

Points are typed (Transform, Fork, Join, Gate, Select, Broadcast, Reduce, Observe)
and classified along six dimensions:

| Dimension | Values |
|-----------|--------|
| **Horizon** | Bounded (terminates) or Asymptotic (runs forever) |
| **Structure** | Transform, Fork, Join, Gate, Select, Broadcast, Reduce, Observe |
| **Substrate** | Financial, Compute, Network, Storage, Security, Identity, Observability, Regulatory |
| **Coordination** | Monotone (gossip, no coordination) or NonMonotone (Raft consensus) |
| **Trust** | PlanTime, AtBoundary, PostConvergence |
| **Intelligence** | Mechanical, AiAssisted (MCP/REST/GraphQL/gRPC), Hybrid |

## Five Invariants

1. Every operation is a convergence point
2. Every point has a typed atomic boundary (prepare → execute → verify → attest)
3. Every attestation is a content-addressed store path
4. Compliance binds to types, verified before or during execution
5. The dependency closure is statically computable at plan time

## 10 Workspace Crates

| Crate | Purpose |
|-------|---------|
| `tatara-core` | Domain types: convergence state, lifecycle, DAG, saga, idempotency, traced events |
| `tatara-engine` | Drivers, Raft, gossip, convergence engine, scheduler, health probes, catalog, metrics |
| `tatara-api` | REST (Axum) + GraphQL (async-graphql): jobs, allocations, nodes, catalog, health |
| `tatara-cli` | CLI + `tatara server` |
| `tatara-kube` | Nix-native K8s reconciler (Server-Side Apply) |
| `tatara-net` | Networking plane: NetworkPlane trait, eBPF types, WASI types, mesh |
| `tatara-operator` | K8s operator: NixBuild/FlakeSource/FlakeOrg CRDs |
| `tatara-testing` | Test fixtures and helpers |
| `ro-cli` | Read-only CLI |

## 7 Execution Drivers

| Driver | Substrate | Platform |
|--------|-----------|----------|
| `exec` | Direct process (fork+exec) | Unix |
| `oci` | Docker/Podman/Apple Containers | All |
| `nix` | Nix flake packages | All with Nix |
| `nix_build` | Nix build + Attic cache | All with Nix |
| `kasou` | Apple Virtualization VMs | macOS |
| `kube` | Kubernetes (Server-Side Apply) | All with kubeconfig |
| `wasi` | wasmtime (WASI Preview 2) | All with wasmtime |

## Documentation

| Document | What It Covers |
|----------|---------------|
| [Unified Platform Architecture](docs/unified-platform-architecture.md) | Master composition: pipeline, invariants, duality, absorption, AI, territory |
| [Unified Convergence Computing Theory](docs/unified-convergence-computing-theory.md) | Formal treatment: 13 sections, 22 principles, 35+ academic references |
| [Theory Realization Map](docs/theory-realization-map.md) | Every pleme-io technology mapped to its role in the theory |
| [CLAUDE.md](CLAUDE.md) | Architecture reference for AI assistants and developers |

## Build & Test

```bash
cargo check          # Workspace check
cargo test           # All tests
cargo clippy         # Lint
nix build            # Release build via substrate
```

## License

MIT
