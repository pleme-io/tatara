# Unified Convergence Computing Theory

## Abstract

The Unified Convergence Computing Theory establishes that **convergence is computation**. Every distributed system operation — from infrastructure provisioning to service deployment to business optimization — is a convergence point: a function that drives observed state toward desired state. Some points have reachable fixed points (distance reaches zero). Others are **asymptotic** — they represent boundless efforts like cost optimization, revenue growth, or security hardening where the target is a direction, not a destination. Both are convergence. These convergence points compose into directed acyclic graphs (DAGs) that represent complete computations. DAGs compose into DAGs-of-DAGs for multi-system orchestration.

This theory composes with the **Unified Infrastructure Theory** (Nix as universal system description language) to form a complete platform: the infrastructure theory says WHAT; the convergence theory says HOW.

## 1. Foundational Principles

### 1.1 Convergence IS Computation

Every computation in a distributed system can be expressed as convergence from an initial state to a target state. This is not an analogy — it is a mathematical identity rooted in fixed-point theory (Knaster-Tarski theorem): the result of a computation is the least fixed point of a functional, reached through iterative application.

**Formal definition**: A convergence point `P` is a tuple `(D, O, C, H)` where:
- `D` is the desired state (declared in Nix) — may be a fixed target OR a direction
- `O` is the observed state (reported by the substrate)
- `C: (D, O) → O'` is the convergence function (idempotent, goal-seeking)
- `H` is the convergence horizon — **Bounded** or **Asymptotic**

For bounded points, the computation terminates when `distance(D, O') = 0`.
For asymptotic points, the computation **never terminates** — it runs in perpetuity,
continuously driving toward a direction that has no final destination.

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

### 1.6 Convergence Horizons

Not all convergence points have a reachable fixed point. The theory distinguishes
two fundamental horizons:

#### Bounded Convergence

A bounded convergence point has a **fixed target** — a concrete desired state that
the system can reach. Once reached, distance = 0 and the computation at that point
is complete (until the environment changes and re-convergence is needed).

Examples:
- "Is the container running?" — yes/no, reachable
- "Are health checks passing?" — yes/no, reachable
- "Is the TLS certificate valid?" — yes/no, reachable
- "Is the DNS record correct?" — yes/no, reachable
- "Is the secret resolved?" — yes/no, reachable

Bounded points can terminate. They may re-activate when inputs change, but they
have a resting state of distance = 0.

#### Asymptotic Convergence

An asymptotic convergence point has a **direction** but no final destination. The
desired state is not a point — it is a gradient. The system is always improving,
always optimizing, and the computation **runs in perpetuity**.

Examples:
- "Minimize infrastructure cost" — always seeking cheaper substrate
- "Maximize revenue" — always seeking more territory
- "Harden security posture" — threat landscape evolves, never done
- "Optimize latency" — always seeking faster paths
- "Grow market share" — no terminal state, pure expansion
- "Reduce technical debt" — code is always changing
- A running SaaS product — the business IS an asymptotic convergence point

For asymptotic points:
- Distance is **not** absolute distance to a target — it is a **performance metric**
  on the optimization dimension (cost in dollars, latency in ms, revenue per month)
- The target is not "reach zero" but "improve continuously"
- **Rate** (not distance) is the primary health indicator:
  - Negative rate = improving (healthy)
  - Positive rate = deteriorating (alert)
  - Zero rate = stalled (investigate)
- The point is "healthy" when the rate is negative, even though distance > 0
- The point NEVER reports `Converged` — its natural state is `Partial` with a
  healthy rate, running forever

#### Perpetual Points as DAG Emitters and Receivers

Asymptotic convergence points exist in perpetuity and act as **continuous DAG
sources and sinks**. Because they never terminate, they are fundamentally different
from bounded points in how they participate in the DAG:

1. **Continuous emission**: An asymptotic point continuously spawns new bounded
   DAGs as it optimizes. A cost-optimization point might emit a migration DAG
   every time a cheaper substrate appears. A security-hardening point emits
   cert-rotation DAGs, policy-update DAGs, vulnerability-patch DAGs — each
   bounded, each completing on its own, while the parent point runs forever.

2. **Continuous reception**: An asymptotic point continuously merges results from
   completed bounded DAGs. The revenue-growth point absorbs market signals,
   pricing changes, churn data — each arriving as a completed bounded DAG
   whose attestation feeds into the next optimization cycle.

3. **Parallel and sequential**: These emissions and receptions happen both in
   parallel (multiple optimization DAGs running concurrently) and in sequence
   (each completed DAG's result informs the next emission). The point is a
   **living node** in the graph — it persists while bounded DAGs flow through it.

```
                        ┌─────────────────────────┐
  completed DAGs ──────→│                         │──────→ new bounded DAGs
  (market signals,      │   Asymptotic Point      │       (migration, scaling,
   price changes,       │   (runs in perpetuity)  │        rotation, patching)
   health reports)      │                         │
                        │   Rate: -0.03/tick ✓    │
  completed DAGs ──────→│   Direction: minimize   │──────→ new bounded DAGs
  (audit results,       │   Horizon: ∞            │       (compliance fixes,
   compliance scans)    │                         │        policy updates)
                        └─────────────────────────┘
```

4. **DAGs-of-DAGs with mixed horizons**: A real system is a DAG-of-DAGs where
   asymptotic points sit at the top (business goals, optimization loops) and
   bounded points fill the interior (infrastructure provisioning, deployment,
   health checks). The asymptotic points are the generators — they continuously
   produce and consume bounded DAGs:

```
┌─────────────────────────────────────────────────────────────┐
│ Business (asymptotic — runs forever)                        │
│                                                             │
│  ┌─────────────┐      ┌──────────────┐     ┌────────────┐ │
│  │ Minimize     │      │ Maximize      │     │ Harden     │ │
│  │ Cost ∞      │      │ Revenue ∞     │     │ Security ∞ │ │
│  └──┬──┬──┬────┘      └──┬──┬──┬─────┘     └──┬──┬─────┘ │
│     │  │  │               │  │  │              │  │        │
│     │  │  └──→ MigrationDAG (bounded, completes)           │
│     │  └─────→ ScalingDAG (bounded, completes)             │
│     └────────→ SpotBidDAG (bounded, completes)             │
│                           │  │                  │          │
│                           │  └──→ FeatureDAG (bounded)     │
│                           └─────→ PricingDAG (bounded)     │
│                                                 │          │
│                                        CertRotateDAG (bounded)
│                                                            │
│  Infrastructure (bounded — converges and rests)            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ NixEval → Replicate → Schedule → Warm → Execute →   │  │
│  │ HealthCheck → CatalogRegister    (all bounded, d=0)  │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

#### The Bounded Preference Principle

**Bounded convergence is always preferred.** Bounded points are predictable (they
terminate), testable (you can assert distance = 0), cacheable (attested results
can be reused), and composable (downstream points know when to start). The system
should maximize the number of bounded convergence points.

Asymptotic points are acknowledged when they genuinely represent unbounded effort —
a business, a continuous optimization loop, an evolving threat landscape. But the
theory demands that we are **intelligent about what they produce**. An asymptotic
point's primary computational output is **bounded convergence points**. The question
is not "what does this optimizer do?" but "what bounded convergence points does
this optimizer instantiate, and do we already have types for them?"

#### Emission Schemas: Knowing What Will Be Produced

Every asymptotic convergence point declares an **emission schema** — the catalog of
bounded convergence point types it can produce. This is known at plan time, before
execution:

```rust
struct EmissionSchema {
    /// The bounded point types this asymptotic point can instantiate.
    catalog: Vec<BoundedPointTemplate>,
    /// Trigger conditions: when does each type get instantiated?
    triggers: Vec<EmissionTrigger>,
    /// Concurrency limits: how many of each type can run in parallel?
    limits: HashMap<String, usize>,
}

struct BoundedPointTemplate {
    /// Name of this bounded point type (e.g., "migration", "cert_rotation").
    name: String,
    /// The convergence DAG this template instantiates.
    dag_template: ConvergenceDAGTemplate,
    /// Input type signature — what the asymptotic point passes in.
    input_sig: TypeSignature,
    /// Output type signature — what feeds back into the asymptotic point.
    output_sig: TypeSignature,
}

struct EmissionTrigger {
    /// Which bounded point type to emit.
    template: String,
    /// Condition that triggers emission (e.g., "cheaper_substrate_available").
    condition: TriggerCondition,
    /// How the trigger is evaluated (poll interval, event-driven, threshold).
    evaluation: TriggerEvaluation,
}
```

Example — a cost-optimization asymptotic point declares its emissions:

```nix
costOptimizer = builtins.convergencePoint {
  type = "asymptotic";
  substrate = "financial";
  direction = "minimize";
  metric = "cost_dollars_per_hour";

  # The bounded point types this optimizer can produce
  emissionSchema = {
    migration = {
      dagTemplate = ./dags/substrate-migration.nix;
      trigger = { condition = "cheaper_substrate_available"; threshold = 0.10; };
      maxConcurrent = 2;
    };
    spotBid = {
      dagTemplate = ./dags/spot-bid.nix;
      trigger = { condition = "spot_price_below_threshold"; };
      maxConcurrent = 5;
    };
    scaling = {
      dagTemplate = ./dags/horizontal-scale.nix;
      trigger = { condition = "utilization_below_threshold"; threshold = 0.3; };
      maxConcurrent = 1;
    };
    drain = {
      dagTemplate = ./dags/node-drain.nix;
      trigger = { condition = "node_cost_exceeds_alternative"; };
      maxConcurrent = 1;
    };
  };
};
```

At plan time, the system knows: "this asymptotic point CAN produce migration DAGs,
spot bid DAGs, scaling DAGs, and drain DAGs. Here are the templates. Here are the
triggers. Here are the concurrency limits." Nothing is ad hoc. Every bounded DAG
that will ever be emitted is a known type.

#### The Instantiation Decision

When an asymptotic point's convergence function detects a trigger condition, it
makes an **instantiation decision**: should this become a bounded convergence
point we know and instantiate?

The decision has three outcomes:

1. **Instantiate from catalog** — the trigger matches a known template. Create
   the bounded DAG from the template, fill in the runtime parameters, and submit
   it to the convergence engine. This is the common case.

2. **Defer** — the trigger fired but conditions aren't right (too many concurrent
   DAGs, recent oscillation, budget exhausted). Log the trigger, don't instantiate.
   The asymptotic point will re-evaluate on the next tick.

3. **Escalate** — the trigger doesn't match any known template. This means the
   asymptotic point encountered a situation it wasn't designed for. This is a
   signal that the emission schema needs to be extended — a new bounded point
   type should be defined. The system logs this as a **schema gap** and alerts
   the operator.

```
Asymptotic Point (runs forever)
    │
    ├── trigger fires ──→ matches template? ──→ YES ──→ instantiate bounded DAG
    │                           │
    │                           └──→ NO ──→ schema gap (alert: define new template)
    │
    └── completed DAG returns ──→ merge result ──→ update optimization state
                                                    │
                                                    └── re-evaluate triggers
```

The goal is to **minimize schema gaps over time**. A mature system has a complete
catalog of bounded point types for every asymptotic point. Every optimization cycle
produces known, typed, bounded work. Nothing is improvised.

#### Why This Matters

This principle — maximize bounded, catalog emissions, decide at instantiation —
has concrete consequences:

1. **Predictability**: because every emitted DAG is a known type with a known
   template, the system can estimate completion time, resource cost, and blast
   radius before launching it.

2. **Testability**: bounded point templates can be tested in isolation. You can
   unit-test a migration DAG template without running the cost optimizer.

3. **Cacheability**: if a bounded DAG with the same inputs was already attested,
   skip it (content-addressed, like Nix cache hits).

4. **Auditability**: every instantiation decision is logged with the trigger
   condition, the template used, and the runtime parameters. The asymptotic
   point's entire history of emitted DAGs is traceable.

5. **Evolvability**: schema gaps are surfaced explicitly. When the system
   encounters something new, it doesn't silently improvise — it tells you
   "define a new bounded point type for this." The catalog grows intentionally.

#### Horizon Classification

```rust
enum ConvergenceHorizon {
    /// Has a fixed point — distance CAN reach 0.
    /// The computation terminates when the fixed point is reached.
    Bounded,

    /// Has a direction but no fixed point — runs in perpetuity.
    /// Health is measured by rate, not distance.
    Asymptotic {
        /// What metric is being optimized (cost_dollars, latency_ms, revenue_monthly).
        metric: String,
        /// What direction is "better" (Minimize or Maximize).
        direction: OptimizationDirection,
        /// What rate of improvement is considered healthy.
        healthy_rate_threshold: f64,
    },
}

enum OptimizationDirection {
    Minimize,  // cost, latency, error rate
    Maximize,  // revenue, throughput, coverage
}
```

#### Cluster Health with Mixed Horizons

`ClusterConvergence` must account for both horizons:
- **Bounded points**: healthy when distance = 0
- **Asymptotic points**: healthy when rate is in the right direction and above
  the healthy threshold
- **System health**: ALL bounded points converged AND ALL asymptotic points
  showing healthy rates
- A system is never "fully converged" if it contains asymptotic points — it is
  **"fully healthy"**, meaning all bounded points are at rest and all asymptotic
  points are actively improving

## 2. Convergence Metrics

### 2.1 Distance

`ConvergenceDistance` measures how far the current state is from the desired state:
- `Converged`: distance = 0.0 (computation complete — only reachable for bounded points)
- `Partial { matching, total }`: distance = 1.0 - (matching/total)
- `Diverged`: distance = 1.0 (computation needed)
- `Unknown`: distance = 1.0 (no observation yet)

For asymptotic points, distance is a **performance metric** (e.g., current cost in
dollars), not distance to a fixed target. It may decrease over time but never reaches
zero. The interpretation depends on the horizon.

### 2.2 Rate

`rate = (current_distance - previous_distance) / tick_duration`
- Negative rate: system is converging / improving (approaching target or optimizing)
- Positive rate: system is **diverging / deteriorating** (alert condition)
- Zero rate: system is stable (converged for bounded, stalled for asymptotic)

For asymptotic points, rate is the **primary health signal**. A negative rate means
the optimization is working. A zero rate means it has stalled and needs investigation.
A positive rate means things are getting worse.

### 2.3 Oscillation

When the rate alternates sign across ticks (converging → diverging → converging), the system is oscillating. Control theory damping is applied:
- Exponential backoff: damping factor increases 1.5x per oscillation
- Cap at 32x normal speed
- Gradual recovery (0.9x decay) when stable

### 2.4 Cluster Convergence

`ClusterConvergence` aggregates all entity states:
- `is_fully_healthy()`: true when ALL bounded entities at distance = 0 AND all
  asymptotic entities have healthy rates
- `is_fully_converged()`: true when ALL entities at distance = 0 (only possible
  when there are no asymptotic points — a purely infrastructure system)
- `overall_distance`: weighted average across all entities
- Counts: converged / partial / diverged / unknown / asymptotic_healthy / asymptotic_stalled

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

## 4. Continuous Cost-Optimal Convergence

### 4.1 The Default Operating Mode

The **default state** of a tatara cluster is to continuously optimize for cost while satisfying workload constraints. This is not an optional feature — it is the natural consequence of the convergence model.

If migration is just re-convergence on a new substrate, then cost optimization is just **continuously re-converging on the cheapest substrate that satisfies constraints**. The system's resting state is always converging toward the lowest-cost configuration that meets all requirements.

### 4.2 Workload Fluidity

Because every tatara node is identical and the convergence DAG is substrate-independent:

- **Workloads shift freely** between nodes based on cost signals
- **No application disturbance** — convergence boundaries ensure atomic handoff (new instance attested before old deregistered)
- **Continuous migration** — the system is always moving toward optimal placement, not just at deploy time
- **Constraint satisfaction** — workloads specify requirements (CPU, memory, GPU, latency, compliance), the system finds the cheapest substrate that satisfies all of them

### 4.3 Spot/Auction Integration

Cloud providers offer heavily discounted compute through auctions and spot markets:
- AWS Spot Instances (up to 90% discount)
- GCP Preemptible VMs
- Azure Spot VMs
- Bare-metal auction markets

Tatara natively integrates with these markets:

1. **Receive notifications** — subscribe to spot price feeds and auction events
2. **Evaluate constraints** — does this discounted node satisfy any running workload's requirements?
3. **Bid/claim** — acquire the node if cost is lower than current placement
4. **Converge** — re-converge the workload onto the new node (new convergence DAG, verified boundaries)
5. **Drain old** — contract the old allocation after new is attested
6. **Release** — return the old node to the pool

The convergence boundary model makes this safe:
- The new node's allocation must pass PREPARE (preconditions met)
- Must pass EXECUTE (workload running)
- Must pass VERIFY (health checks passing)
- Must be ATTESTED (tameshi hash chained)
- Only THEN does the old allocation begin CONTRACTING

If any step fails, the old allocation remains active. Zero disruption.

### 4.4 Hardware Auction for Workload Requirements

Beyond receiving spot notifications, tatara can **actively auction for hardware**:

1. Workload declares requirements: `{ cpu: "4", memory: "16Gi", gpu: "1", latency: "<10ms", compliance: ["soc2"] }`
2. Tatara publishes a **resource request** to connected substrate providers
3. Providers bid: "I can satisfy this for $X/hour on substrate Y"
4. Tatara evaluates bids against current cost and selects the cheapest that satisfies all constraints
5. If a cheaper bid arrives for a running workload, tatara initiates migration (re-convergence)
6. The workload is always running on the cheapest available substrate

This creates a **market-driven placement engine** where:
- The workload never knows what hardware it's running on
- The platform continuously shops for the best price
- Migration is invisible to the application (atomic convergence boundaries)
- The system's natural equilibrium is minimum cost

### 4.5 Cost as a Convergence Metric

Cost becomes a dimension of the convergence state:

```
ConvergenceDistance = f(
  phase_distance,       # are all phases converged?
  health_distance,      # are health checks passing?
  cost_distance,        # is this the cheapest available substrate?
)
```

The system is "fully converged" not just when the workload is running and healthy, but when it's running on the **optimal substrate for its constraints**. If a cheaper option becomes available, the system is "partially converged" on the cost dimension and begins migrating.

### 4.6 Preemption Safety

When a spot instance is reclaimed:
1. The cloud provider sends a termination notice (typically 2 minutes)
2. Tatara's convergence engine receives the signal
3. A new convergence DAG is immediately started on alternative substrate
4. The PREPARE phase of the new DAG runs in parallel with the CONTRACT phase of the old
5. Traffic shifts when the new allocation is ATTESTED
6. The old allocation completes TERMINAL before the spot instance dies

The convergence boundary model guarantees this is safe — the attestation chain ensures no gap in service.

## 5. Typed Convergence Points and DAG Algebra

### 5.1 Convergence Points as Typed Primitives

A convergence point is not just a function — it is a **typed computational primitive**
with a signature that determines what it accepts, how it merges, and what it emits:

```
ConvergencePoint<I, O> {
    id:          PointId,               // content-addressed (like a Nix store path)
    point_type:  ConvergencePointType,  // determines merge and emit semantics
    inputs:      Vec<TypedEdge<I>>,     // what this point accepts
    convergence: Fn(I, Observed) → O,   // the convergence function
    outputs:     Vec<TypedEdge<O>>,     // what this point emits (can be multiple)
    boundary:    ConvergenceBoundary,   // prepare → execute → verify → attest
}
```

The **type** of a convergence point determines its behavior at the DAG level:

| Point Type | Accepts | Emits | Semantics |
|-----------|---------|-------|-----------|
| `Transform` | 1 input | 1 output | Linear — converts one state to another |
| `Fork` | 1 input | N outputs | Fan-out — spawns N downstream DAGs |
| `Join` | N inputs | 1 output | Fan-in — merges N upstream results |
| `Gate` | N inputs | 1 output | Barrier — waits for all inputs before proceeding |
| `Select` | N inputs | 1 output | Choice — picks best input by policy |
| `Broadcast` | 1 input | N outputs (same type) | Replicate — same signal to multiple consumers |
| `Reduce` | N inputs | 1 output | Aggregate — fold inputs into a summary |
| `Observe` | 1 input | 1 output + side-channel | Tap — emits to the DAG + an observation stream |

A `Fork` point is where a single convergence event spawns independent sub-problems.
For example, a workload placement decision might fork into:

```
PlacementDecision (Fork)
    ├──→ FinancialOptimization DAG (find cheapest substrate)
    ├──→ LatencyOptimization DAG (find lowest-latency region)
    └──→ ComplianceVerification DAG (verify regulatory constraints)
```

Each forked DAG has its own convergence points, its own boundary attestations, and
converges independently. A downstream `Join` or `Select` point merges the results.

### 5.2 The DAG Algebra

Convergence DAGs compose through a small set of algebraic operations:

**Sequence**: `A ; B` — B's input is A's output. B cannot prepare until A is attested.

**Parallel**: `A | B` — A and B run concurrently. No data dependency between them.

**Fork**: `A ⊳ (B, C, D)` — A emits typed outputs that spawn B, C, D as independent DAGs.

**Join**: `(A, B, C) ⊲ D` — D waits for A, B, C to all reach Attested, then merges their outputs.

**Select**: `(A, B, C) ▷ D` — D takes the first (or best, by policy) result from A, B, C.

**Nest**: `A[sub-DAG]` — A contains an entire sub-DAG. A is Attested when its sub-DAG
is fully converged. The parent DAG sees A as a single point.

These compose arbitrarily:

```
NixEval ; RaftReplicate ; (Schedule ⊳ (
    FinancialDAG | LatencyDAG | ComplianceDAG
) ⊲ PlacementSelect) ; Warm ; Execute ; HealthCheck ; CatalogRegister
```

The algebra is **closed**: every composition of convergence points produces a valid
convergence DAG. Every valid DAG can be decomposed into these primitives.

### 5.3 Convergence DAGs as Derivations

Convergence DAGs share a deep structural identity with Nix derivations. This is not
analogy — it is the same computational pattern:

| Nix Derivation | Convergence DAG |
|---------------|-----------------|
| Store path (hash of inputs + builder) | Point ID (hash of inputs + convergence function) |
| `inputDrvs` (dependency derivations) | `inputs` (upstream convergence points) |
| Builder script | Convergence function `C(D, O) → O'` |
| Output store paths | Output attestation hashes + typed state |
| `nix-store --query --requisites` | "What does this flow depend on?" (full closure) |
| `nix-store --query --referrers` | "What flows depend on this?" (reverse closure) |
| Content-addressable store | Attestation chain integrity |
| Build plan (before execution) | Convergence plan (before execution) |
| Cached build result | Converged state (skip re-convergence if already attested) |

The critical consequence: **at plan time, before any execution begins, we know the
complete dependency closure of every convergence flow.** We can:

1. **Analyze** — compute the full graph, detect cycles, identify bottlenecks
2. **Cost** — estimate the total convergence cost across all substrates
3. **Impact** — "if this point re-converges, what else must re-converge?"
4. **Prune** — skip points whose input attestations haven't changed (like Nix cache hits)
5. **Rollback** — because every point's inputs and outputs are hashed, we can replay
   any prefix of the DAG from a known-good state
6. **Parallelize** — identify independent branches and converge them concurrently

Just as Nix computes the build plan before running any builders, tatara computes the
convergence plan before driving any state. The plan IS the program. The execution IS
the convergence.

### 5.4 Content-Addressed Convergence

Because each point's ID is derived from its inputs and convergence function (like a
Nix store path is derived from its inputs and builder), convergence is
**content-addressed**:

```
point_id = blake3(
    convergence_function_hash,
    input_attestation_hashes...,
    desired_state_hash,
)
```

If two different DAGs arrive at the same point with the same inputs and desired state,
they produce the **same point ID**. This means:

- **Deduplication**: identical convergence work is never repeated
- **Sharing**: two workloads needing the same secret resolution share one convergence
- **Caching**: a point that was attested with the same inputs can be skipped
- **Verification**: anyone can recompute the point ID and verify the chain

This is the Nix store for convergence. The attestation chain IS the store.

## 6. Substrate Convergence Flows

### 6.1 Every Substrate is a Convergence DAG

The key insight: every operational substrate — financial, compute, network, storage,
security, identity, observability — is itself a convergence DAG. The system is not
a single DAG with a cost dimension bolted on. It is a **typed DAG-of-DAGs where each
substrate has its own convergence flow**.

```
┌─────────────────────────────────────────────────────────────────┐
│                    Workload Convergence                         │
│                    (top-level DAG)                               │
│                                                                 │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │  Financial    │  │  Compute     │  │  Network             │  │
│  │  Substrate    │  │  Substrate   │  │  Substrate           │  │
│  │  DAG          │  │  DAG         │  │  DAG                 │  │
│  │              │  │              │  │                      │  │
│  │  SpotPrice   │  │  NodeReady   │  │  DNSResolve          │  │
│  │  → BidEval   │  │  → CpuAlloc  │  │  → RouteConfig      │  │
│  │  → CostConv  │  │  → MemAlloc  │  │  → TLSCert          │  │
│  │  → BudgetGate│  │  → GpuAlloc  │  │  → MeshJoin         │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
│                                                                 │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │  Storage      │  │  Security    │  │  Observability       │  │
│  │  Substrate    │  │  Substrate   │  │  Substrate           │  │
│  │  DAG          │  │  DAG         │  │  DAG                 │  │
│  │              │  │              │  │                      │  │
│  │  VolMount    │  │  SecretFetch │  │  MetricSink          │  │
│  │  → CacheWarm │  │  → CertIssue│  │  → LogShipper        │  │
│  │  → Replicate │  │  → PolicyGate│  │  → TraceSampler      │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
│                                                                 │
│  Cross-substrate edges enforce ordering:                        │
│    Security.SecretFetch ──→ Compute.CpuAlloc (secrets before    │
│                              process start)                     │
│    Financial.BudgetGate ──→ Compute.NodeReady (budget approved  │
│                              before provisioning)               │
│    Network.TLSCert ──→ Observability.MetricSink (TLS before     │
│                              metric export)                     │
└─────────────────────────────────────────────────────────────────┘
```

Each substrate DAG converges independently on its own dimension. The workload is
"fully converged" when ALL substrate DAGs report distance = 0. If any substrate
shifts (spot price changes, cert expires, node fails), only the affected substrate
DAG re-converges — the others remain stable.

### 6.2 Substrate Types

Every substrate convergence flow has a type that classifies its domain:

```rust
enum SubstrateType {
    Financial,      // cost optimization, billing, budgets, spot markets
    Compute,        // CPU, GPU, memory, WASI runtimes
    Network,        // connectivity, DNS, TLS, routing, mesh
    Storage,        // volumes, caches, replication, backups
    Security,       // secrets, certificates, policies, compliance
    Identity,       // authentication, authorization, RBAC
    Observability,  // metrics, logs, traces, alerting
    Regulatory,     // compliance frameworks, data residency, audit
}
```

Each type carries its own convergence semantics:
- **Financial** substrates converge toward **minimum cost** given constraints
- **Compute** substrates converge toward **resource satisfaction** (requested ≤ available)
- **Network** substrates converge toward **connectivity** (all paths established)
- **Security** substrates converge toward **policy compliance** (all checks passing)
- **Observability** substrates converge toward **signal completeness** (all sinks connected)

### 6.3 Multi-Dimensional Convergence Distance

With substrate-typed DAGs, the convergence distance becomes a **vector**, not a scalar:

```
ConvergenceDistance = {
    financial:     0.0,   // on cheapest substrate ✓
    compute:       0.15,  // 85% of requested resources allocated
    network:       0.0,   // all routes established ✓
    storage:       0.3,   // volume replication in progress
    security:      0.0,   // all secrets resolved, certs valid ✓
    identity:      0.0,   // RBAC policies applied ✓
    observability: 0.5,   // log shipper not yet connected
    regulatory:    0.0,   // SOC2 controls satisfied ✓
}
```

The **overall distance** is the maximum component (not the average) — the workload is
only fully converged when ALL dimensions are at zero. This is a lattice join operation:
the system converges along each dimension independently, and the meet point is when
all dimensions have reached their fixed points.

### 6.4 Substrate DAGs as First-Class Citizens

Because each substrate is a typed convergence DAG, substrate optimization is not a
special feature — it is the **natural behavior of the system**:

- The financial substrate DAG is ALWAYS running, continuously re-evaluating cost
- The security substrate DAG is ALWAYS running, continuously checking cert expiry
- The network substrate DAG is ALWAYS running, continuously verifying connectivity
- The observability substrate DAG is ALWAYS running, continuously ensuring signal flow

The system's resting state is **all substrate DAGs converged**. Any external change
(price shift, cert rotation, node failure) perturbs one or more substrate DAGs, and
they re-converge. The workload never stops running — only the substrate beneath it
shifts.

## 7. Static Analysis and Full Visibility

### 7.1 The Convergence Plan

Because convergence DAGs are typed and their dependencies are known at plan time
(like Nix derivations), we can compute a **convergence plan** before execution:

```
tatara plan my-workload.nix

Convergence Plan: my-workload
├── Financial Substrate (3 points, est. 200ms)
│   ├── SpotPriceQuery [Transform] → float
│   ├── BidEvaluation [Select] → Placement
│   └── BudgetGate [Gate] → Approved
├── Security Substrate (2 points, est. 500ms)
│   ├── SecretResolve [Transform] → SecretBundle
│   └── CertIssue [Transform] → TLSCert
├── Compute Substrate (4 points, est. 2s)
│   ├── NodeSelect [Select] → NodeId        (depends: Financial.BudgetGate)
│   ├── ResourceAlloc [Transform] → Alloc    (depends: Security.SecretResolve)
│   ├── DriverStart [Transform] → Process
│   └── HealthCheck [Observe] → HealthStatus
├── Network Substrate (3 points, est. 1s)
│   ├── DNSRegister [Transform] → DnsRecord
│   ├── RouteConfig [Transform] → Route
│   └── MeshJoin [Transform] → MeshPeer     (depends: Compute.HealthCheck)
└── Observability Substrate (2 points, est. 300ms)
    ├── MetricSink [Transform] → SinkConfig
    └── LogShipper [Transform] → ShipperConfig

Total: 14 convergence points across 5 substrates
Critical path: Financial → Compute → Network (est. 3.2s)
Parallelizable: Security ‖ Financial, Observability ‖ Network
Cache hits: SecretResolve (attestation unchanged), MetricSink (same config)
```

### 7.2 Closure Queries

Like `nix-store --query`, convergence DAGs support closure queries:

**Forward closure** — "What does this point depend on?"
```
tatara query --requisites Compute.DriverStart
→ Financial.BudgetGate, Security.SecretResolve, Compute.NodeSelect, Compute.ResourceAlloc
```

**Reverse closure** — "What depends on this point?"
```
tatara query --referrers Security.SecretResolve
→ Compute.ResourceAlloc, Compute.DriverStart, Compute.HealthCheck, Network.MeshJoin
```

**Impact analysis** — "If this point re-converges, what else must re-converge?"
```
tatara query --impact Financial.SpotPriceQuery
→ Financial.BidEvaluation, Financial.BudgetGate, Compute.NodeSelect,
  Compute.ResourceAlloc, Compute.DriverStart, Compute.HealthCheck,
  Network.MeshJoin
  (7 points across 3 substrates must re-converge)
```

**Diff** — "What changed between two convergence states?"
```
tatara query --diff state-v1 state-v2
→ Financial.SpotPriceQuery: input changed (price $0.12 → $0.08)
→ Compute.NodeSelect: output changed (node-a → node-b)
→ 5 downstream points re-attested
```

### 7.3 Complete Operational Visibility

Because every operation is a typed convergence point in a known DAG, the system
provides **total visibility** over all operations across all substrates:

1. **What is converging right now?** — query all substrate DAGs for points with
   distance > 0
2. **What is blocked?** — query for points in `Preparing` phase whose input
   attestations are not yet available
3. **What is the critical path?** — compute the longest chain of sequential
   dependencies across all substrate DAGs
4. **What is the blast radius?** — reverse closure from any point shows everything
   that would be affected by a failure
5. **What can be cached?** — points whose input attestations match a previous run
   can skip re-execution (like Nix cache hits)
6. **What is oscillating?** — points with oscillation detected, across any substrate
7. **What is the convergence rate?** — per-point, per-substrate, and cluster-wide

This is not monitoring bolted onto a system — the monitoring IS the system. Every
convergence point produces attestation, distance, and rate data as a byproduct of
execution. The observability substrate DAG captures and routes this data, but the
data exists whether or not anyone is watching.

### 7.4 Data Structures for Full Visibility

The convergence graph is a concrete data structure that can be serialized, diffed,
queried, and transmitted:

```rust
/// The complete convergence graph for a workload across all substrates.
struct ConvergenceGraph {
    /// All points, keyed by content-addressed PointId.
    points: HashMap<PointId, TypedConvergencePoint>,
    /// Typed edges between points (intra- and inter-substrate).
    edges:  Vec<TypedEdge>,
    /// Substrate DAGs (each is a subgraph of the full graph).
    substrates: HashMap<SubstrateType, SubstrateDAG>,
    /// The convergence plan (computed at plan time, before execution).
    plan: ConvergencePlan,
}

/// A typed edge between convergence points.
struct TypedEdge {
    from:       PointId,
    to:         PointId,
    edge_type:  EdgeType,           // Data | Control | Attestation
    type_sig:   TypeSignature,      // the type flowing across this edge
}

/// A convergence point with its type signature.
struct TypedConvergencePoint {
    point:      ConvergencePoint,
    point_type: ConvergencePointType, // Transform | Fork | Join | Gate | ...
    input_sig:  Vec<TypeSignature>,   // types accepted
    output_sig: Vec<TypeSignature>,   // types emitted
    substrate:  SubstrateType,        // which substrate this belongs to
    closure:    Vec<PointId>,         // forward closure (all dependencies)
}
```

With these structures, the entire convergence state of the system — across all
substrates, all workloads, all nodes — is a single queryable, diffable, attestable
data structure. This is the convergence store, analogous to the Nix store.

## 8. The Nix Store as Convergence Store

### 8.1 The Unification

The structural identity between convergence DAGs and Nix derivations (section 5.3)
is not just theoretical — it means we can use the **actual Nix store** as the
convergence store. Sui (our pure-Rust Nix replacement) already provides:

- Content-addressed store paths (`blake3(inputs + builder) → /nix/store/...`)
- Dependency tracking (`input_derivations` in every derivation)
- Closure queries (`--requisites`, `--referrers`)
- Distributed caching (binary cache / Attic)
- Async `Store` trait with pluggable backends
- Async `Builder` trait with pluggable execution strategies
- Triple API server (REST, GraphQL, gRPC) for queries

Every one of these maps directly to a convergence primitive. The convergence store
is not a new system — it is the Nix store extended with convergence semantics.

### 8.2 Convergence Derivations

A convergence point IS a derivation:

```
┌──────────────────────────────────────────────────────────────────┐
│ Traditional Nix Derivation          Convergence Derivation       │
│                                                                  │
│ input_derivations: [dep.drv]        input_derivations: [prev.drv]│
│ builder: /nix/store/..-bash         builder: "convergence"       │
│ args: ["-c", "make install"]        args: [convergence_fn.wasm]  │
│ env: { src = "..."; }               env: {                       │
│                                       desired_state = "...";     │
│                                       substrate_type = "compute";│
│                                       point_type = "transform";  │
│                                     }                            │
│ outputs: { out = "..-result" }      outputs: { out = "..-attest"}│
│                                                                  │
│ Store path = hash(inputs + builder) Point ID = hash(inputs + fn) │
│ Cached if output exists             Cached if attestation valid  │
│ nix-store --requisites → closure    sui query --requisites → DAG │
└──────────────────────────────────────────────────────────────────┘
```

The output of a convergence derivation is an **attestation store path** — a content-
addressed artifact containing the attested state, the boundary hashes, and the
tameshi BLAKE3 Merkle proof. Because it's a store path, it inherits everything
the Nix store provides: content verification, garbage collection, remote
substitution, closure computation.

### 8.3 Sui Extensions for Convergence

Sui needs a small set of new primitives. These extend, not replace, existing
functionality:

#### 8.3.1 New Builder Type: `convergence`

The `Builder` trait in `sui-build` currently supports `LocalBuilder` (fork+exec in
a sandbox). A new `ConvergenceBuilder` implements the same trait but drives
convergence instead of running a shell script:

```rust
// sui-build/src/convergence_builder.rs

/// A builder that drives convergence instead of running a shell script.
/// Implements the existing sui-build Builder trait.
struct ConvergenceBuilder {
    engine: TataraConvergenceEngine,
}

#[async_trait]
impl Builder for ConvergenceBuilder {
    async fn build(&self, drv: &Derivation, ...) -> BuildResult {
        let desired = drv.env.get("desired_state");
        let substrate = drv.env.get("substrate_type");
        let point_type = drv.env.get("point_type");
        let fn_ref = &drv.args[0]; // WASI convergence function

        // Drive: prepare → execute → verify → attest
        let attestation = self.engine.converge(desired, substrate, fn_ref).await?;

        // Output: attestation store path (content-addressed)
        write_attestation_to_store(attestation)
    }
}
```

This builder type is dispatched when `drv.builder == "convergence"`. Traditional
builds continue to use `LocalBuilder`. The `Builder` trait doesn't change.

#### 8.3.2 Convergence Builtins

New builtins extend sui's evaluator (90+ builtins already exist). These are added
to `sui-eval/src/builtins/` following the existing module pattern:

```nix
# builtins.convergencePoint — declare a typed convergence point
spotPrice = builtins.convergencePoint {
  type = "transform";          # Transform | Fork | Join | Gate | Select | ...
  substrate = "financial";     # Financial | Compute | Network | ...
  inputs = [];                 # upstream convergence points
  desired = { maxCost = 0.10; region = "us-east-1"; };
  convergence = ./financial/spot-price.wasm;  # WASI convergence function
  preconditions = [ "budget_approved" ];
  postconditions = [ "price_within_range" ];
};

# builtins.convergenceDAG — compose points into a typed DAG
financialDAG = builtins.convergenceDAG {
  substrate = "financial";
  points = { inherit spotPrice bidEval budgetGate; };
  edges = [
    { from = "spotPrice"; to = "bidEval"; type = "data"; }
    { from = "bidEval"; to = "budgetGate"; type = "control"; }
  ];
};

# builtins.convergenceGraph — compose substrate DAGs into the full graph
workloadGraph = builtins.convergenceGraph {
  substrates = { inherit financialDAG computeDAG networkDAG securityDAG; };
  crossEdges = [
    { from = "financial.budgetGate"; to = "compute.nodeSelect"; }
    { from = "security.secretResolve"; to = "compute.resourceAlloc"; }
  ];
};
```

Each builtin produces a derivation (or set of derivations) that sui can evaluate,
plan, and build. The convergence graph becomes a Nix expression that sui evaluates
into a derivation graph — the convergence plan.

#### 8.3.3 Live Attestations (Generational Store Paths)

Traditional Nix store paths are build-once — once the output exists, the derivation
is never rebuilt. Convergence points re-converge continuously. This requires
**generational store paths**:

```
/nix/store/<hash>-spotPrice-gen0    # initial convergence
/nix/store/<hash>-spotPrice-gen1    # re-converged (price changed)
/nix/store/<hash>-spotPrice-gen2    # re-converged (new spot bid)
```

Each generation is a new store path (new content → new hash). The convergence engine
tracks the **latest generation** for each point. Older generations are GC-eligible
once no downstream points reference them. This reuses sui's existing garbage
collector — convergence state cleanup is just Nix GC.

The generation chain is itself a Merkle chain: each generation's attestation hash
includes the previous generation's hash, creating an append-only convergence log
stored entirely in the Nix store.

#### 8.3.4 Convergence Store Queries

Sui's `Store` trait is extended with convergence-aware queries. These compose with
the existing `query_path_info()` and `query_referrers()`:

```rust
// sui-store/src/convergence.rs

#[async_trait]
trait ConvergenceStore: Store {
    /// Forward closure: all convergence points this point depends on.
    async fn convergence_requisites(&self, point: &PointId) -> Vec<PointId>;

    /// Reverse closure: all points that depend on this point.
    async fn convergence_referrers(&self, point: &PointId) -> Vec<PointId>;

    /// Impact analysis: what must re-converge if this point changes?
    async fn convergence_impact(&self, point: &PointId) -> Vec<PointId>;

    /// Current convergence distance across all substrates.
    async fn convergence_distance(&self) -> MultiDimensionalDistance;

    /// The full convergence graph (typed DAG across all substrates).
    async fn convergence_graph(&self) -> ConvergenceGraph;

    /// Latest generation for a convergence point.
    async fn latest_generation(&self, point: &PointId) -> Generation;

    /// Convergence history for a point (all generations).
    async fn convergence_history(&self, point: &PointId) -> Vec<Attestation>;
}
```

These are exposed through sui's existing triple API:
- **REST**: `GET /api/v1/convergence/graph`, `GET /api/v1/convergence/impact/{point}`
- **GraphQL**: `query { convergenceGraph { substrates { points { distance } } } }`
- **gRPC**: `rpc GetConvergenceGraph(Empty) returns (ConvergenceGraph)`

### 8.4 The Complete Flow

```
1. DECLARE  (Nix)
   User writes workload + convergence constraints in Nix.

2. EVALUATE (sui evaluator)
   sui evaluates the Nix expression → convergence derivation graph.
   The plan is visible before any execution: `sui eval --convergence-plan`.

3. PLAN     (sui store)
   sui computes the derivation closure, identifies cache hits,
   determines critical path, estimates convergence time.

4. EXECUTE  (tatara engine via ConvergenceBuilder)
   tatara drives each convergence derivation through
   prepare → execute → verify → attest.
   Each attested result is written to the sui store as a store path.

5. STORE    (sui store)
   Attestations are content-addressed store paths.
   sui tracks generations, computes closures, manages GC.
   Attic distributes attestations across the cluster.

6. QUERY    (sui API)
   Any node can query convergence state through sui's triple API.
   Forward/reverse closures, impact analysis, convergence distance —
   all backed by the store's dependency graph.

7. RE-CONVERGE (continuous)
   When a substrate shifts (price change, node failure, cert expiry),
   the affected convergence derivation's inputs change.
   New input hash → new derivation → new convergence → new attestation.
   Only the affected subgraph re-converges (like incremental Nix builds).
```

### 8.5 Why This Works

The Nix store is the correct convergence store because the invariants align:

| Nix Store Invariant | Convergence Need |
|---------------------|-----------------|
| Content-addressed (no forgery) | Attestation integrity |
| Dependency tracking | Convergence closures |
| Garbage collection | Stale state cleanup |
| Remote substitution (Attic) | Distributed convergence state |
| Closure computation | Impact analysis |
| Incremental builds | Incremental re-convergence |
| Build reproducibility | Convergence determinism |
| Store path immutability | Attestation permanence (per generation) |

No new storage system is needed. No new query engine. No new distribution mechanism.
The convergence store is the Nix store. Sui is the convergence computer's runtime.

## 9. Implementation in Tatara

### Core Types (tatara-core/src/domain/convergence_state.rs)

- `ConvergenceDistance` — how far from desired state (scalar today, vector with substrates)
- `ConvergenceState` — distance + rate + oscillation + damping per entity
- `ConvergencePoint` — named step with CALM classification + boundary
- `ConvergenceBoundary` — preconditions + postconditions + attestation chain
- `BoundaryCheck` — individual pass/fail check
- `BoundaryPhase` — Pending → Preparing → Executing → Verifying → Attested
- `ClusterConvergence` — cluster-wide summary (is_fully_healthy + is_fully_converged)
- `CalmClassification` — Monotone | NonMonotone
- `ConvergenceHorizon` — Bounded | Asymptotic { metric, direction, threshold }
- `OptimizationDirection` — Minimize | Maximize
- `EmissionSchema` — catalog of bounded point types an asymptotic point can produce
- `BoundedPointTemplate` — pre-defined DAG template with typed input/output signatures
- `EmissionTrigger` — condition + evaluation strategy for when to instantiate
- `TriggerCondition` — the predicate that fires an emission
- `InstantiationDecision` — Instantiate | Defer | Escalate (schema gap)

### Planned Types — DAG Algebra (from sections 5–7)

- `ConvergencePointType` — Transform | Fork | Join | Gate | Select | Broadcast | Reduce | Observe
- `SubstrateType` — Financial | Compute | Network | Storage | Security | Identity | Observability | Regulatory
- `TypedEdge` — typed connection between points (Data | Control | Attestation)
- `TypeSignature` — the type flowing across an edge
- `ConvergenceGraph` — complete typed DAG across all substrates
- `SubstrateDAG` — a substrate-scoped subgraph of the full convergence graph
- `ConvergencePlan` — static analysis output computed before execution
- `PointId` — content-addressed identifier (blake3 of inputs + convergence function)

### Planned Types — Sui Convergence Store (from section 8)

- `ConvergenceBuilder` — `impl Builder` that drives convergence instead of shell scripts
- `ConvergenceStore` — `trait` extending sui's `Store` with convergence queries
- `Generation` — monotonic counter for convergence point re-attestations
- `Attestation` — store path content: attested state + boundary hashes + tameshi proof
- `MultiDimensionalDistance` — per-substrate distance vector (max component = overall)

### Sui Extensions Required

| Crate | Extension | Existing Hook |
|-------|-----------|---------------|
| `sui-build` | `ConvergenceBuilder` impl | `Builder` async trait |
| `sui-eval` | `builtins/convergence.rs` (convergencePoint, convergenceDAG, convergenceGraph) | Builtin module registration pattern |
| `sui-store` | `ConvergenceStore` trait + convergence queries | `Store` async trait |
| `sui-compat` | Convergence derivation env keys (`substrate_type`, `point_type`, `desired_state`) | `Derivation.env: BTreeMap<String, String>` |
| `sui` (API) | `/api/v1/convergence/*` endpoints | Axum router, GraphQL schema, gRPC service |

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
| Nix build model | Dolstra 2004 | Content-addressed dependency closures, cacheable computation |
| Nix store as computation store | Dolstra 2006 (PhD thesis) | Store paths = attestation artifacts, GC = stale state cleanup |
| Typed dataflow | Naiad / Timely Dataflow | Typed edges between operators, progress tracking |
| DAG algebras | Category theory (monoidal categories) | Composition of typed computational primitives |
| Multi-dimensional optimization | Pareto optimality | Convergence across independent substrate dimensions |
| Static analysis / abstract interpretation | Cousot & Cousot 1977 | Plan-time analysis of convergence graphs |
| Generational references | Epoch-based reclamation | Convergence generations as append-only Merkle chains |
| Asymptotic optimization | Online convex optimization (Zinkevich 2003) | Perpetual convergence points with no fixed point |
| Anytime algorithms | Zilberstein 1996 | Improve solution quality with more time, no terminal state |
| Gradient descent | Cauchy 1847 / modern ML | Direction-based optimization without fixed target |
