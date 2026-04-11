# Tatara (粋) — Programmable Convergence Computer

A distributed computing platform where **convergence IS computation**.
Every system, cloud, bare-metal node, VM, or container becomes a substrate.
Tatara lays down a convergence layer, and you program it with convergence
DAGs that drive any system toward declared state. DAGs compose into
DAGs-of-DAGs for multi-system coordination.

Declared in Nix. Compiled in Rust. Sandboxed via WASI. Enforced via eBPF.
Distributed via Raft consensus + gossip. Every node is identical.

## The Convergence Computing Model

**Convergence IS computation.** Every operation in tatara is a convergence
point in a DAG. The system computes by driving each point from diverged
(distance > 0) to converged (distance = 0). The computation terminates
when all points report distance = 0.

```
Convergence DAG (the program):
  NixEval ──→ RaftReplicate ──→ Schedule ──→ PortAlloc ──→ SecretResolve
                                                              ↓
  CatalogRegister ←── HealthCheck ←── Execute ←── VolumeMount
```

Each point converges independently. Raft coordinates non-monotone points
(placement, deletion). Gossip handles monotone points (health, metrics).
Every tatara node runs the same code — the DAG determines what each node
works on. DAGs compose into DAGs-of-DAGs for multi-system orchestration.

**CALM theorem applied**: monotone operations (health, metrics, logs) need
NO coordination. Non-monotone operations (scheduling, deletion) go through
Raft. This maximizes what can be distributed.

### Atomic Convergence Boundaries

Each convergence point has four verified phases:
```
Prepare → Execute → Verify → Attest ──hash──→ Next Point
```
- **Prepare**: verify input environment + previous point's attestation hash
- **Execute**: drive toward target state (the convergence itself)
- **Verify**: prove output is correct (postcondition checks)
- **Gate**: produce attestation hash (tameshi BLAKE3), open gate for next point

This creates provably secure computation — each step cryptographically
bound to the previous. Audit the entire chain after the fact. Lays down
on any substrate: cloud, K8s, bare-metal, WASI, pure tatara.

### Key Types

- `ConvergenceDistance`: Converged | Partial | Diverged | Unknown (0.0 to 1.0)
- `ConvergenceState`: distance + rate + oscillation + damping per entity
- `ConvergencePoint`: named step in the DAG with CALM classification + boundary
- `ConvergenceBoundary`: preconditions + postconditions + attestation chain
- `BoundaryPhase`: Pending → Preparing → Executing → Verifying → Attested | Failed
- `ClusterConvergence`: cluster-wide summary (is_fully_converged())

## Architecture

```
User declares workload in Nix (tataraJobs / workload archetypes)
  → Raft replicates desired state to all nodes
  → Scheduler (leader-only) creates allocations
  → Convergence engine drives desired → observed (the convergence DAG)
  → Driver executes workload (exec/oci/nix/kasou/wasi/kube)
  → Health probes verify liveness
  → Service catalog registers healthy instances
  → Metrics + traced events for observability
```

## Workspace Crates (10)

| Crate | Purpose |
|-------|---------|
| `tatara-core` | Domain types: Job, Allocation, WorkloadPhase, ServiceEntry, secrets, volumes, DAG, saga, idempotency, traced events |
| `tatara-engine` | Runtime: drivers, Raft cluster, gossip, reconciler, convergence engine, scheduler, executor, health probes, port allocator, catalog registry, volume manager, secret resolver, NATS event bus, metrics, Nix evaluator, sui client |
| `tatara-api` | REST (Axum) + GraphQL (async-graphql): jobs, allocations, nodes, sources, releases, catalog, health, metrics |
| `tatara-cli` | CLI: job/node/alloc/source/context/forge/event/release commands + `tatara server` |
| `tatara-kube` | Nix-native K8s reconciler: Server-Side Apply, dependency ordering, pruning, health checks. Replaces FluxCD. |
| `tatara-net` | Networking plane: NetworkPlane trait, eBPF types, WASI types, mesh peer info, flow observability, service routing |
| `tatara-operator` | K8s operator: NixBuild/FlakeSource/FlakeOrg CRDs, NATS JetStream bridge |
| `tatara-testing` | Test fixtures and helpers |
| `ro-cli` | Read-only CLI |

## WorkloadPhase Lifecycle

Every workload follows: **Initial → Warming → Executing → Contracting → Terminal**

```rust
enum WorkloadPhase<W, E, C, T> {
    Initial,          // Defined but not active
    Warming(W),       // Acquiring resources, resolving deps
    Executing(E),     // Active, healthy, serving
    Contracting(C),   // Gracefully draining
    Terminal(T),      // Done
}
```

Concrete types: `TaskPhase`, `AllocationPhase`, `NodePhase`.
Valid transitions enforced by `is_valid_transition()`.

## 7 Execution Drivers

| Driver | Backend | Platform |
|--------|---------|----------|
| `exec` | Direct process (fork+exec) | Unix |
| `oci` | Docker/Podman/Apple Containers | All |
| `nix` | `nix run <flake_ref>` | All with Nix |
| `nix_build` | `nix build` + Attic cache push | All with Nix |
| `kasou` | Apple Virtualization.framework VMs | macOS |
| `kube` | Kubernetes Server-Side Apply | All with kubeconfig |
| `wasi` | wasmtime WASI Preview 2 | All with wasmtime |

## Distributed State Machine

- **Raft** (openraft): linearizable writes for job placement, allocation lifecycle
- **Gossip** (chitchat): eventually-consistent metadata, failure detection
- **Desired vs Observed**: CQRS split in ClusterState
- **Generation counter**: optimistic concurrency for scheduling
- **Leader-affinity**: only the leader schedules (prevents duplicates)
- **Executor feedback**: reports observations through Raft

## Subsystems

| Subsystem | Purpose |
|-----------|---------|
| **Convergence engine** | Compares desired vs observed, drives transitions |
| **Service catalog** | Consul-like registry with health-aware queries |
| **Health probes** | HTTP/TCP/Exec checks via ProbeExecutor |
| **Port allocator** | Dynamic 20000-32000 range, conflict detection |
| **Volume manager** | Local/HostPath/NFS lifecycle |
| **Secret resolver** | Env + SOPS providers (Akeyless planned) |
| **NATS event bus** | Events, logs, health, catalog changes (graceful no-op) |
| **Prometheus metrics** | 22+ gauges/counters at `/metrics` |
| **Sui client** | Build/eval/cache via sui-daemon REST |
| **Idempotency store** | Dedup with TTL for Raft commands |
| **Saga types** | Compensation logic for multi-step provisioning |
| **Traced events** | Correlation IDs for workload lifecycle tracing |
| **Store adapter** | ClusterStore→Evaluator bridge |

## REST API

| Endpoint | Purpose |
|----------|---------|
| `GET /health` | Health check |
| `GET/POST /api/v1/jobs` | List/submit jobs |
| `GET /api/v1/allocations` | List allocations |
| `GET /api/v1/nodes` | List cluster nodes |
| `GET /api/v1/events/stream` | SSE event stream |
| `GET /v1/catalog/services` | List service names |
| `GET /v1/catalog/service/{name}` | Service instances |
| `GET /v1/health/service/{name}?passing=true` | Healthy instances |
| `GET /metrics` | Prometheus text format |

## Nix Integration

```nix
# Job definitions via flake outputs
tataraJobs.<system>.<name> = { id, job_type, groups, constraints, meta };

# HM module for macOS/Linux service
services.tatara.server = {
  enable = true;
  httpAddr = "127.0.0.1:4646";
  nats.enable = true;
  sui.daemonAddr = "127.0.0.1:8080";
  ports = { rangeStart = 20000; rangeEnd = 32000; };
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

## Key Commands

```bash
tatara server                    # Start single-node cluster
tatara job list                  # List jobs
tatara job run spec.json         # Submit job
tatara node list                 # List cluster nodes
tatara source add infra github:pleme-io/infra  # Add GitOps source
tatara top                       # TUI dashboard
```

## Build

```bash
cargo check          # Workspace check
cargo test           # All tests (98 passing)
cargo build          # Debug build
nix build            # Release via substrate
```

## Conventions

- Edition 2021, MIT license
- `clippy::pedantic` on tatara-kube and tatara-net
- Release: codegen-units=1, lto=true, opt-level="z", strip=true
- Pure Rust — no C, no Go
- All state changes through Raft (except gossip-only health/metrics per CALM theorem)
