# Tatara (粋) — Programmable Convergence Computer

A distributed computing platform where **convergence IS computation**.

Tatara implements two composing theories:
- **Unified Infrastructure Theory**: Nix declares abstract intent, renderers translate to any backend
- **Unified Convergence Computing Theory**: every operation is a convergence point in a verified DAG

Declare any system in Nix. Compute it into existence through verified convergence on any substrate. Prove every step cryptographically via tameshi attestation.

## Quick Start

```bash
tatara server                              # Start single-node cluster
tatara job run spec.json                   # Submit a workload
tatara job list                            # Monitor convergence
curl http://localhost:4646/metrics         # Prometheus metrics
curl http://localhost:4646/v1/catalog/services  # Service discovery
```

## The Convergence Computing Model

Every operation is a convergence point with four verified phases:

```
PREPARE → EXECUTE → VERIFY → ATTEST ──hash──→ next point
```

Points compose into DAGs. DAGs compose into DAGs-of-DAGs. The system terminates when all points report distance = 0.

```
Convergence DAG:
  NixEval → RaftReplicate → Schedule → PortAlloc → SecretResolve →
    VolumeMount → DriverStart → HealthCheck → CatalogRegister
```

CALM theorem applied: monotone operations (health, metrics) need NO coordination. Non-monotone operations (scheduling, deletion) go through Raft.

## 10 Workspace Crates

| Crate | Purpose |
|-------|---------|
| `tatara-core` | Domain types, convergence state, lifecycle, DAG, saga, idempotency, traced events |
| `tatara-engine` | Drivers, Raft, gossip, convergence engine, scheduler, health probes, catalog, metrics |
| `tatara-api` | REST + GraphQL: jobs, allocations, nodes, catalog, health, metrics |
| `tatara-cli` | CLI + `tatara server` |
| `tatara-kube` | Nix-native K8s reconciler (Server-Side Apply, replaces FluxCD) |
| `tatara-net` | Networking plane: NetworkPlane trait, eBPF types, WASI types, mesh |
| `tatara-operator` | K8s operator: NixBuild/FlakeSource/FlakeOrg CRDs |
| `tatara-testing` | Test fixtures |
| `ro-cli` | Read-only CLI |

## 7 Execution Drivers

| Driver | Substrate | Platform |
|--------|-----------|----------|
| `exec` | Direct process | Unix |
| `oci` | Docker/Podman/Apple Containers | All |
| `nix` | Nix flake packages | All with Nix |
| `nix_build` | Nix build + cache | All with Nix |
| `kasou` | Apple Virtualization VMs | macOS |
| `kube` | Kubernetes (Server-Side Apply) | All with kubeconfig |
| `wasi` | wasmtime (WASI Preview 2) | All with wasmtime |

## Convergence State Types

- `ConvergenceDistance`: Converged | Partial | Diverged | Unknown (0.0 to 1.0)
- `ConvergenceState`: distance + rate + oscillation + damping per entity
- `ConvergencePoint`: step in DAG with CALM classification + atomic boundary
- `ConvergenceBoundary`: preconditions + postconditions + attestation hash chain
- `ClusterConvergence`: cluster-wide summary (is_fully_converged())

## Documentation

- [Unified Convergence Computing Theory](docs/unified-convergence-computing-theory.md)
- [CLAUDE.md](CLAUDE.md) — architecture reference for AI assistants

## Build & Test

```bash
cargo check          # Workspace check
cargo test           # 117 tests
cargo clippy         # Lint
nix build            # Release build via substrate
```

## License

MIT
