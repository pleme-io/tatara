# Theory Realization Map

Every pleme-io technology occupies a specific position in the unified platform
architecture. This document maps concrete implementations to theoretical
concepts.

## Pipeline Layer Mapping

### DECLARE Layer (Intent)

| Technology | Role in Theory | What It Does |
|-----------|---------------|-------------|
| **Nix language** | Universal specification language | Pure functional declarations of desired state |
| **substrate** | Infrastructure Theory implementation | 7 abstract archetypes + backend renderers + policies |
| `workload-archetypes.nix` | Abstract intent | mkHttpService, mkWorker, mkCronJob, mkGateway, mkStatefulService, mkFunction, mkFrontend |
| `renderers/*.nix` | Backend translation | Archetype → K8s manifests, tatara JobSpec, WASI config, compose |
| `policies.nix` | Governance at declaration time | mkPolicy, evaluateAll — violations are errors |
| `nix-kube` (substrate) | K8s resource algebra | 29 pure builders + 9 compositions |
| **flake.nix** (per repo) | Workload + compliance declaration | User's entry point into the pipeline |
| **shikumi** | Configuration convergence | Nix option → YAML file → app reads config (hot-reload via ArcSwap) |

### PLAN Layer (Analysis)

| Technology | Role in Theory | What It Does |
|-----------|---------------|-------------|
| **sui** | Convergence computer runtime (store + evaluator) | Pure-Rust Nix replacement, 10 crates |
| `sui-eval` | Nix evaluator | Tree-walk + bytecode VM, 90+ builtins, flake resolution |
| `sui-bytecode` | High-performance evaluation | NaN-boxed 8B VM, 100+ opcodes |
| `sui-compat` | Store path algebra | Content-addressing, derivations, ATerm format |
| `sui` (API) | Plan queries | REST + GraphQL + gRPC for convergence plan access |
| `convergence builtins` (planned) | DAG declaration | builtins.convergencePoint, convergenceDAG, convergenceGraph |

### GATE Layer (Trust)

| Technology | Role in Theory | What It Does |
|-----------|---------------|-------------|
| **tameshi** | Attestation foundation | BLAKE3 Merkle trees, 24 LayerTypes, CertificationArtifact (artifact + controls + intent), DFC signing, heartbeat chain |
| **kensa** | Compliance verification | 14 frameworks (NIST, SOC2, FedRAMP, PCI, OSCAL, ...), ComplianceRunner trait |
| **sekiban** | K8s deployment gate | Admission webhook, SignatureGate CRD, LayerHashCollector |
| **inshou** | Nix rebuild gate | NixStoreOps trait, profile history, sekiban endpoint integration |
| **pangea-architectures** | Zero-cost compliance | 14-layer network stack, 358 RSpec tests on synthesis output |
| **pangea-core** | IaC DSL foundation | ResourceBuilder, typed validation |
| **pangea-aws/azure/gcp/...** | Provider compliance | Auto-generated typed resource functions |
| **inspec-*** | Live verification | Post-convergence probes on running infrastructure |
| **iac-test-runner** | Test orchestration | Bringup/verify/teardown lifecycle |

### EXECUTE Layer (Convergence)

| Technology | Role in Theory | What It Does |
|-----------|---------------|-------------|
| **tatara-core** | Convergence type system | ConvergenceDistance, ConvergencePoint, BoundaryPhase, CalmClassification |
| **tatara-engine** | Convergence execution | Drivers, Raft, gossip, scheduler, health probes, catalog, metrics |
| **tatara-api** | Convergence queries | REST + GraphQL for live convergence state |
| **tatara-cli** | Convergence operations | Job/node/alloc/event commands, `tatara server` |
| **tatara-kube** | K8s convergence | Server-Side Apply reconciler, dependency ordering |
| **tatara-net** | Network convergence | NetworkPlane trait, eBPF, WASI networking, mesh |
| **tatara-operator** | K8s operator convergence | NixBuild/FlakeSource CRDs, NATS bridge |
| **Rust** | Implementation language | All convergence engine code, pure Rust, no C/Go |
| **WASI/wasmtime** | Sandboxed convergence functions | Convergence functions as WASI components |
| **OCI/Docker** | Container convergence | OCI driver for containerized workloads |
| **eBPF/aya** | Kernel-level convergence | XDP/TC for network plane enforcement |

### STORE Layer (Persistence)

| Technology | Role in Theory | What It Does |
|-----------|---------------|-------------|
| **sui-store** | Convergence store | Store trait, LocalStore (fs + SQLite), BinaryCacheStore |
| **sui-build** | Convergence builder | Builder trait, LocalBuilder + ConvergenceBuilder (planned) |
| **sui-cache** | Attestation distribution | Binary cache server (Axum, S3/object_store) |
| **sui-daemon** | Store daemon | Unix socket, worker protocol, peer credentials |
| **sui-orchestrate** | Fleet convergence | System rebuild, profile management, fleet deployment |
| **sui-cache** | Distributed attestation | Binary cache replication across cluster nodes |
| **NATS** | Event distribution | Event bus for convergence state changes |

## Cross-Cutting Technologies

### AI-Assisted Computing (Intelligence Dimension)

| Technology | Role in Theory | What It Does |
|-----------|---------------|-------------|
| **MCP** (Model Context Protocol) | AI interface to convergence ether | 30+ structured tools for observe/analyze/influence/verify/report |
| **kaname** | MCP scaffold | MCP server framework (tool registry, response helpers) |
| **curupira** | Browser convergence via MCP | Chrome DevTools convergence points |
| **Claude Code** | AI-assisted development | Skills for convergence computing development |
| **guardrail** | AI safety | 2468 rules for constraining AI actions |

### Observability (Observability Substrate)

| Technology | Role in Theory | What It Does |
|-----------|---------------|-------------|
| **Shinryu** | Observability data plane | ANSI SQL over all signals (logs, metrics, flows) |
| **Vector** | Signal routing | Pipeline configuration for log/metric shipping |
| **Datadog** | Metrics convergence | APM, dashboards, monitors via pangea-datadog |
| **Prometheus** | Tatara metrics | 22+ gauges/counters at `/metrics` |

### Code Generation (Territory Expansion)

| Technology | Role in Theory | What It Does |
|-----------|---------------|-------------|
| **forge-gen** | Unified codegen | OpenAPI → SDKs, MCP servers, IaC, completions |
| **iac-forge** | IaC generation | TOML specs → Terraform, Pulumi, Crossplane, Ansible, Pangea, Steampipe |
| **mcp-forge** | MCP generation | OpenAPI → Rust MCP server |
| **completion-forge** | Shell completions | OpenAPI → skim-tab YAML + fish |
| **sekkei** | OpenAPI types | Canonical serde types for spec loading |
| **takumi** | OpenAPI → IR | FieldType, ResolvedSpec, CRUD grouping |
| **meimei** | Naming conventions | Case converters for code generation |

### Networking (Network Substrate)

| Technology | Role in Theory | What It Does |
|-----------|---------------|-------------|
| **mamorigami** | VPN convergence | WireGuard platform (tunnels, peers, keys) |
| **kakuremino** | Anonymous transport | Tor/SOCKS5 convergence |
| **kurayami** | Privacy DNS | DoT/DoH/DoQ convergence |
| **hanabi** | Traffic convergence | L7 proxy + L4 LB + cache + circuit breaker |

### Identity & Secrets (Security + Identity Substrates)

| Technology | Role in Theory | What It Does |
|-----------|---------------|-------------|
| **Akeyless** | Secret convergence | Secret resolution via DFC signing |
| **akeyless-nix** | Nix secret convergence | Drop-in sops-nix replacement |
| **kenshou** | Auth convergence | OAuth2/OIDC providers, token validation, sessions |
| **SOPS** | Encrypted state | Secret values encrypted at rest |

## Absorption Examples

When a new external system is encountered, it maps to this pattern:

```
External System → Observe → Type → Converge → Attest → Comply → Package
```

| Absorbed System | Substrate | Convergence Points | Absorption Package |
|----------------|-----------|-------------------|-------------------|
| AWS EC2 | Compute | instance_running, sg_applied | pangea-aws |
| Akeyless | Security | secret_resolved, target_synced | akeyless-nix + pangea-akeyless |
| Datadog | Observability | monitor_created, dashboard_synced | pangea-datadog |
| K8s cluster | Compute | resource_applied, pod_healthy | tatara-kube + nix-kube |
| Stripe | Financial | payment_processed, subscription_active | (future absorption) |
| WireGuard | Network | tunnel_established, peer_authed | mamorigami |
| Tor | Network | circuit_established, onion_published | kakuremino + kakureyado |
| PostgreSQL | Storage | migration_applied, replica_synced | shinka |
| NIST 800-53 | Regulatory | control_satisfied | kensa compliance package |

## How the Pieces Compose

```
User intent (Nix flake)
  → substrate renders archetype (7 types × N backends)
    → sui evaluates → convergence derivation graph
      → tameshi + kensa verify compliance (14 frameworks)
        → tatara drives convergence (7 drivers × M substrates)
          → tameshi attests (BLAKE3 Merkle chain)
            → sui stores (content-addressed, generational)
              → sui-cache distributes (binary cache)
                → AI observes/analyzes/reports (MCP, 30+ tools)

Every step: typed, attested, compliant, queryable, cacheable.
```
