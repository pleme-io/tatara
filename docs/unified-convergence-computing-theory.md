# Unified Convergence Computing Theory

## Abstract

The Unified Convergence Computing Theory establishes that **convergence is computation**. Every distributed system operation — from infrastructure provisioning to service deployment to health verification — is a convergence point: a function that drives observed state toward desired state until distance equals zero. These convergence points compose into directed acyclic graphs (DAGs) that represent complete computations. DAGs compose into DAGs-of-DAGs for multi-system orchestration. The computation terminates when all points report convergence.

This theory composes with the **Unified Infrastructure Theory** (Nix as universal system description language) to form a complete platform: the infrastructure theory says WHAT; the convergence theory says HOW.

## 1. Foundational Principles

### 1.1 Convergence IS Computation

Every computation in a distributed system can be expressed as convergence from an initial state to a target state. This is not an analogy — it is a mathematical identity rooted in fixed-point theory (Knaster-Tarski theorem): the result of a computation is the least fixed point of a functional, reached through iterative application.

**Formal definition**: A convergence point `P` is a tuple `(D, O, C)` where:
- `D` is the desired state (declared in Nix)
- `O` is the observed state (reported by the substrate)
- `C: (D, O) → O'` is the convergence function (idempotent, goal-seeking)
- The computation terminates when `distance(D, O') = 0`

### 1.2 The CALM Classification

Every operation in the system is classified per the CALM theorem (Hellerstein 2010):

- **Monotone operations** (only grow, never retract): Can be distributed WITHOUT coordination. Every node independently converges. Examples: health checks, metric collection, log aggregation, set unions, counters.

- **Non-monotone operations** (can retract or change direction): REQUIRE coordination via consensus. Examples: allocation placement, job deletion, policy changes, exclusive resource assignment.

This classification determines the convergence mechanism at each point:
- Monotone → gossip (O(log N) rounds, no leader)
- Non-monotone → Raft (leader-coordinated, linearizable)

### 1.3 Atomic Convergence Boundaries

Each convergence point is wrapped in a verified boundary with four phases:

```
┌─────────────────────────────────────────────────┐
│ 1. PREPARE                                       │
│    Verify input attestation from previous point  │
│    Check preconditions (environment ready?)       │
├─────────────────────────────────────────────────┤
│ 2. EXECUTE                                       │
│    Drive convergence: C(D, O) → O'               │
│    Track rate, detect oscillation, apply damping  │
├─────────────────────────────────────────────────┤
│ 3. VERIFY                                        │
│    Check postconditions (output correct?)         │
│    Cryptographic attestation (tameshi BLAKE3)     │
├─────────────────────────────────────────────────┤
│ 4. GATE                                          │
│    Produce output attestation hash               │
│    Open gate for next point in DAG               │
└─────────────────────────────────────────────────┘
```

Each boundary creates an **atomic checkpoint**:
- You cannot skip the preparation phase
- You cannot forge the attestation hash
- You cannot proceed without verification
- The entire chain is auditable after the fact

### 1.4 Convergence DAGs

A convergence DAG is a directed acyclic graph where:
- **Nodes** are convergence points
- **Edges** represent dependencies: "this point must be attested before that point can prepare"
- The DAG is the program
- The substrate is the hardware
- The computation is the traversal

Standard DAGs:
- **Allocation lifecycle**: NixEval → RaftReplicate → Schedule → PortAlloc → SecretResolve → VolumeMount → DriverStart → HealthCheck → CatalogRegister
- **Node lifecycle**: GossipJoin → RaftJoin → DriverDetect → Ready
- **Rolling update**: NewAlloc(Warm) → NewAlloc(Execute) → OldAlloc(Contract) → OldAlloc(Terminal)
- **Multi-tier deploy**: DB DAG → Cache DAG → API DAG → Frontend DAG

### 1.5 DAGs-of-DAGs

DAGs compose hierarchically:
- A DAG can contain sub-DAGs as nodes
- A sub-DAG is "converged" when all its internal points are converged
- The parent DAG sees each sub-DAG as a single convergence point

This enables multi-system orchestration:
- Deploy application across K8s + bare-metal + edge
- Each substrate has its own convergence DAG
- The parent DAG coordinates completion across all substrates

## 2. Convergence Metrics

### 2.1 Distance

`ConvergenceDistance` measures how far the current state is from the desired state:
- `Converged`: distance = 0.0 (computation complete)
- `Partial { matching, total }`: distance = 1.0 - (matching/total)
- `Diverged`: distance = 1.0 (computation needed)
- `Unknown`: distance = 1.0 (no observation yet)

### 2.2 Rate

`rate = (current_distance - previous_distance) / tick_duration`
- Negative rate: system is converging (approaching target)
- Positive rate: system is **diverging** (moving away — alert condition)
- Zero rate: system is stable (either converged or stuck)

### 2.3 Oscillation

When the rate alternates sign across ticks (converging → diverging → converging), the system is oscillating. Control theory damping is applied:
- Exponential backoff: damping factor increases 1.5x per oscillation
- Cap at 32x normal speed
- Gradual recovery (0.9x decay) when stable

### 2.4 Cluster Convergence

`ClusterConvergence` aggregates all entity states:
- `is_fully_converged()`: true when ALL entities at distance = 0
- `overall_distance`: weighted average across all entities
- Counts: converged / partial / diverged / unknown

## 3. Composition with Unified Infrastructure Theory

The two theories compose as layers:

| Layer | Theory | What It Does |
|-------|--------|-------------|
| **Intent** | Infrastructure | Nix declares abstract workload archetypes |
| **Rendering** | Infrastructure | Renderers translate to backend-specific resources |
| **Convergence** | Computing | Each resource becomes a convergence point |
| **Execution** | Computing | Points drive state toward target on distributed nodes |
| **Verification** | Computing | Atomic boundaries with tameshi attestation |
| **Audit** | Computing | BLAKE3 Merkle chain proves every step |

### Migration as Re-Rendering + Re-Convergence

Because the convergence DAG is substrate-independent:
- **Migration** = re-render the same archetypes to a new target + re-converge
- The convergence types don't change
- The boundary attestations still chain
- The CALM classifications still hold
- Only the backend-specific execution changes

This means K8s → tatara, AWS → GCP, Docker → WASI, cloud → bare-metal are all the same operation: re-render + re-converge.

## 4. Implementation in Tatara

### Core Types (tatara-core/src/domain/convergence_state.rs)

- `ConvergenceDistance` — how far from desired state
- `ConvergenceState` — distance + rate + oscillation + damping
- `ConvergencePoint` — named step with CALM classification + boundary
- `ConvergenceBoundary` — preconditions + postconditions + attestation chain
- `BoundaryCheck` — individual pass/fail check
- `BoundaryPhase` — Pending → Preparing → Executing → Verifying → Attested
- `ClusterConvergence` — cluster-wide summary
- `CalmClassification` — Monotone | NonMonotone

### Academic Foundations

| Concept | Source | Application |
|---------|--------|-------------|
| Fixed-point computation | Knaster-Tarski theorem | Convergence = computation to least fixed point |
| CALM theorem | Hellerstein 2010 | Monotone ops need no coordination |
| CRDTs | Shapiro et al. 2011 | Health/metrics as convergent replicated types |
| Self-stabilization | Dijkstra 1974 | Converge from ANY state |
| Control theory | PID controllers | Damping, oscillation detection |
| Level-triggered logic | K8s controller pattern | Compare state each tick |
| Lattice computation | Conway et al. 2012 | Monotone ops form join-semilattice |
| Merkle trees | Anti-entropy protocols | Efficient divergence detection |
| Saga pattern | Garcia-Molina & Salem 1987 | Compensation for multi-step provisioning |
