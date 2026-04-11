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

## 9. Compliant Computing: The Third Theory

### 9.1 The Composition of Three Theories

The platform is built on three composing theories:

| Theory | Concern | Question Answered |
|--------|---------|------------------|
| **Unified Infrastructure Theory** (substrate) | WHAT to compute | What workloads exist, on what substrates? |
| **Unified Convergence Computing Theory** (tatara) | HOW to compute | How does each resource converge to desired state? |
| **Compliant Computing Theory** (tameshi + kensa) | WHETHER to compute | Is this computation permitted by policy and law? |

The first two theories establish what gets built and how it converges. The third
theory adds a constraint: **not all convergence is permitted**. Some computations
require compliance verification before, during, or after execution. The compliant
computing theory makes this a static, plannable, attestable property of the
convergence graph — not a post-hoc audit.

### 9.2 Compliance as a Static DAG Property

Because convergence DAGs are Nix derivations (section 5.3), and Nix derivations
have statically known dependency closures, **compliance requirements are computable
at plan time**:

```
tatara plan --compliance my-workload.nix

Compliance Plan: my-workload
├── Convergence Points: 14 across 5 substrates
├── Compliance Controls Required:
│   ├── NIST SC-7(5): Network boundary protection
│   │   └── Applies to: Network.RouteConfig, Network.MeshJoin
│   ├── NIST AC-6: Least privilege
│   │   └── Applies to: Security.SecretResolve, Identity.RBACGate
│   ├── NIST AU-2: Auditable events
│   │   └── Applies to: ALL convergence points (attestation chain)
│   ├── SOC 2 CC6.1: Logical and physical access controls
│   │   └── Applies to: Compute.NodeSelect, Security.*
│   ├── PCI DSS 3.4: Render PAN unreadable
│   │   └── Applies to: Storage.VolMount (if handling cardholder data)
│   └── FedRAMP AC-17: Remote access
│       └── Applies to: Network.*, Compute.* (if in FedRAMP boundary)
├── Verification Method:
│   ├── Pre-execution (plan-time): 8 controls (static analysis of DAG structure)
│   ├── At-boundary (per point): 4 controls (runtime attestation)
│   └── Post-convergence: 2 controls (live verification via InSpec)
└── Compliance Closure: 14 points × 6 frameworks = 84 control bindings
    Cache hits: 31 (attestation unchanged from previous generation)
    New verifications needed: 53
```

This is not hypothetical — the machinery already exists:

- **Tameshi** computes three-pillar CertificationArtifacts (artifact + controls + intent)
- **Kensa** runs compliance checks against 14 frameworks
- **Sekiban** gates K8s deploys on attestation signatures
- **Inshou** gates Nix rebuilds on attestation signatures
- **Pangea-architectures** validates compliance on synthesis output (zero cloud cost)

The compliant computing theory unifies these into the convergence graph.

### 9.3 Type-Level Compliance

Compliance controls attach to convergence point **types**, not individual instances.
This means entire categories and classes of computing are gated by compliance
requirements:

```rust
struct ComplianceBinding {
    /// Which convergence point types this control applies to.
    point_selector: PointSelector,
    /// Which compliance framework and control.
    control: ComplianceControl,
    /// When this control is verified.
    verification_phase: VerificationPhase,
    /// The kensa runner that evaluates this control.
    runner: ComplianceRunnerRef,
}

enum PointSelector {
    /// All points on a specific substrate.
    Substrate(SubstrateType),
    /// All points of a specific type.
    PointType(ConvergencePointType),
    /// All points matching a substrate + type combination.
    SubstrateAndType(SubstrateType, ConvergencePointType),
    /// All points in a specific environment.
    Environment(String),
    /// All points handling data of a specific classification.
    DataClassification(DataClassification),
    /// All points (universal control).
    All,
}

enum VerificationPhase {
    /// Verified at plan time by static analysis of the DAG structure.
    /// No execution needed. Zero cost.
    PlanTime,
    /// Verified at convergence boundary (prepare or verify phase).
    /// Runs inline with convergence execution.
    AtBoundary,
    /// Verified after convergence by live verification (InSpec).
    /// Runs against the converged state.
    PostConvergence,
}
```

Example bindings:

```nix
compliancePolicy = {
  # ALL convergence points must maintain audit trail
  auditTrail = {
    selector = "all";
    control = { framework = "nist-800-53"; id = "AU-2"; };
    phase = "at-boundary";  # attestation chain satisfies this inherently
  };

  # All Security substrate points must enforce least privilege
  leastPrivilege = {
    selector = { substrate = "security"; };
    control = { framework = "nist-800-53"; id = "AC-6"; };
    phase = "plan-time";  # verify RBAC config in the derivation
  };

  # All Network Transform points must have boundary protection
  networkBoundary = {
    selector = { substrate = "network"; pointType = "transform"; };
    control = { framework = "nist-800-53"; id = "SC-7"; };
    phase = "post-convergence";  # InSpec verifies live network config
  };

  # All points handling PII must encrypt at rest
  piiEncryption = {
    selector = { dataClassification = "pii"; };
    control = { framework = "pci-dss"; id = "3.4"; };
    phase = "at-boundary";  # verify encryption before attestation
  };

  # Production environment requires FedRAMP controls
  fedRampProduction = {
    selector = { environment = "production"; };
    control = { framework = "fedramp-moderate"; id = "all"; };
    phase = "plan-time";  # all FedRAMP controls verified before execution
  };
};
```

### 9.4 Tameshi CertificationArtifact as Convergence Attestation

Tameshi's three-pillar CertificationArtifact maps directly to convergence
boundary attestation:

```
┌──────────────────────────────────────────────────────────────────┐
│ Tameshi CertificationArtifact    Convergence Boundary Attestation│
│                                                                  │
│ artifact_hash ─────────────────→ convergence_function_hash       │
│   (hash of deployment artifact)   (hash of convergence function  │
│                                    + its WASI binary)            │
│                                                                  │
│ control_hash ──────────────────→ compliance_verification_hash    │
│   (hash of compliance results)    (kensa assessment results for  │
│                                    all bound controls)           │
│                                                                  │
│ intent_hash ───────────────────→ desired_state_hash              │
│   (hash of infrastructure code)   (hash of Nix-declared desired  │
│                                    state)                        │
│                                                                  │
│ composed_root ─────────────────→ boundary_attestation            │
│   (BLAKE3 Merkle of all three)    (BLAKE3 Merkle of all three)   │
│                                                                  │
│ Two-phase signature:             Two-phase convergence:          │
│   Phase 1: untested root          Phase 1: converge first        │
│   Phase 2: + compliance hash      Phase 2: + compliance verified │
│   Master: combined signature      Final: combined attestation    │
└──────────────────────────────────┘────────────────────────────────┘
```

This means every convergence point's attestation is a CertificationArtifact.
The attestation chain is simultaneously:
- A convergence proof (each point converged correctly)
- A compliance proof (each point satisfied its bound controls)
- An intent proof (each point did what was declared in Nix)

All three are cryptographically bound. You cannot forge one without invalidating
the others.

### 9.5 Compliance Verification as Convergence DAGs

Compliance verification itself is a convergence DAG. Each compliance check is a
bounded convergence point that converges to "control satisfied" (distance = 0)
or "control violated" (failed):

```
Compliance DAG for NIST 800-53:
  AU-2.collect ──→ AU-2.verify ──→ AU-2.attest
  AC-6.collect ──→ AC-6.verify ──→ AC-6.attest
  SC-7.collect ──→ SC-7.verify ──→ SC-7.attest
          ↓               ↓              ↓
  FrameworkGate (Join: all controls must be attested)
          ↓
  ComplianceAttestation (feeds into CertificationArtifact.control_hash)
```

Because compliance DAGs are convergence DAGs, they get everything for free:
- **Content-addressed** — same inputs → same compliance result → cache hit
- **Dependency-tracked** — compliance closure is computable
- **Distributed** — compliance checks run on any tatara node
- **Attested** — compliance results are tameshi-attested store paths
- **Generational** — compliance re-verification produces new generations

### 9.6 The Compliance Closure

Just as `nix-store --requisites` computes the dependency closure of a store path,
the compliance closure computes every control that must be satisfied for a
convergence DAG to be compliant:

```
tatara query --compliance-closure my-workload

Compliance Closure: my-workload
├── Framework: NIST 800-53 Rev 5
│   ├── AU-2 (Auditable Events) — bound to: ALL points [at-boundary]
│   ├── AC-6 (Least Privilege) — bound to: Security.* [plan-time]
│   ├── SC-7 (Boundary Protection) — bound to: Network.* [post-convergence]
│   ├── IA-5 (Authenticator Management) — bound to: Identity.* [at-boundary]
│   └── CP-9 (Information System Backup) — bound to: Storage.* [at-boundary]
├── Framework: SOC 2 Type II
│   ├── CC6.1 (Logical Access) — bound to: Compute.*, Security.* [plan-time]
│   └── CC7.2 (System Monitoring) — bound to: Observability.* [at-boundary]
├── Framework: FedRAMP Moderate
│   └── (inherits NIST 800-53 bindings for production environment)
└── Total: 42 unique controls across 3 frameworks
    Verified at plan-time: 18 (static, zero cost)
    Verified at boundary: 16 (inline with convergence)
    Verified post-convergence: 8 (live InSpec probes)
```

The compliance closure is computed at plan time, before any convergence begins.
If a control cannot be satisfied (no runner configured, missing prerequisite),
the plan fails before any resources are provisioned — just like a Nix build
failing at evaluation time, not at build time.

### 9.7 Gating as Convergence Boundaries

The existing gating tools (sekiban, inshou) are convergence boundaries:

| Tool | Convergence Boundary | Gate Type |
|------|---------------------|-----------|
| **Sekiban** | K8s admission webhook — a Gate convergence point that verifies CertificationArtifact before allowing K8s resource creation | At-boundary (kube driver) |
| **Inshou** | Nix rebuild gate — a Gate convergence point that verifies store path attestation before allowing system profile switch | At-boundary (nix driver) |
| **Kensa** | Compliance verification — a Join convergence point that merges all framework results into a single compliance hash | At-boundary (all drivers) |
| **Tameshi** | Attestation composition — a Reduce convergence point that folds all layer hashes into a CertificationArtifact | At-boundary (all drivers) |
| **Pangea RSpec** | Synthesis validation — a Transform convergence point that verifies infrastructure code against compliance controls at plan time | Plan-time (pre-execution) |
| **InSpec profiles** | Live verification — Transform convergence points that verify running infrastructure against compliance controls | Post-convergence |

These are not separate systems bolted onto the convergence engine. They ARE
convergence points in the compliance substrate DAG. The compliance substrate
runs in parallel with all other substrates and produces attestations that feed
into the master CertificationArtifact.

### 9.8 Tameshi Layer Types as Convergence Point Classifications

Tameshi's 24 LayerType variants map to convergence point classifications:

| LayerType | Convergence Point | Substrate |
|-----------|------------------|-----------|
| `Nix` | NixEval convergence point | Compute |
| `Oci` | OCI driver start point | Compute |
| `RenderedHelm` | K8s resource rendering | Compute |
| `Kubernetes` | K8s state convergence | Compute |
| `Tofu` | Terraform state convergence | Compute |
| `FluxCD` | GitOps convergence | Compute |
| `LiveAkeyless` | Secret resolution | Security |
| `LiveAkeylessTarget` | Target-specific secrets | Security |
| `PangeaSynthesis` | IaC synthesis verification | Regulatory (plan-time) |
| `RSpecResult` | Compliance test results | Regulatory (plan-time) |
| `InSpecResult` | Live compliance verification | Regulatory (post-convergence) |
| Agent layers (10) | Agent convergence points | Compute (agent substrate) |

Every convergence point produces a tameshi layer hash. The layer hashes compose
into the CertificationArtifact. The CertificationArtifact IS the convergence
boundary attestation. The chain is unbroken from Nix declaration through
convergence execution through compliance verification to cryptographic proof.

### 9.9 Zero-Cost Compliance Verification

The pangea-architectures pattern — RSpec tests on synthesized Terraform JSON,
zero cloud cost — extends to all convergence DAGs:

1. **Plan-time verification** (zero cost): the convergence plan IS a Nix
   derivation graph. Compliance controls bound with `phase = "plan-time"` are
   evaluated on the plan, not the execution. This catches violations before
   any resources are provisioned.

2. **Cached compliance** (near-zero cost): because compliance verification
   results are content-addressed store paths, identical inputs produce cache
   hits. If the convergence function, desired state, and compliance runner
   haven't changed, the compliance result from the previous generation is
   reused. No re-verification needed.

3. **Incremental compliance** (minimal cost): when inputs change, only the
   affected compliance controls are re-evaluated. The compliance closure
   identifies exactly which controls need re-verification — like incremental
   Nix builds.

The result: **compliance verification scales with changes, not with system size**.
A system with 10,000 convergence points across 50 substrates does not run 10,000
compliance checks on every tick. It runs compliance checks only on the points
whose inputs have changed since the last attestation.

### 9.10 Packaged Compliance

Because compliance verification is convergence DAGs stored in the Nix store
(via sui), compliance itself is **packageable**:

```nix
# A compliance package: a complete set of controls for a framework
nist-800-53-moderate = builtins.compliancePackage {
  framework = "nist-800-53";
  baseline = "moderate";
  controls = import ./frameworks/nist-800-53-moderate.nix;
  runners = {
    planTime = import ./runners/plan-time.nix;
    atBoundary = import ./runners/boundary.nix;
    postConvergence = import ./runners/inspec.nix;
  };
  bindings = import ./bindings/nist-800-53-moderate.nix;
};

# Apply a compliance package to a workload
myWorkload = builtins.convergenceGraph {
  substrates = { ... };
  compliance = [ nist-800-53-moderate soc2-type2 pci-dss-4 ];
};
```

Compliance packages are:
- **Versioned** — `nist-800-53-moderate-v2` replaces `v1` with a new derivation
- **Composable** — apply multiple packages to the same workload
- **Cacheable** — a compliance package applied to the same convergence graph
  produces the same attestation (content-addressed)
- **Distributable** — push compliance packages to Attic, pull on any node
- **Auditable** — the compliance package itself is a store path with a known hash

An organization can package its entire compliance posture as a Nix expression,
version it, distribute it, and apply it to any convergence graph. Compliance
becomes infrastructure-as-code, verified at plan time, attested at execution
time, and proven cryptographically after the fact.

## 10. Implementation in Tatara (Current + Planned)

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

### Planned Types — Compliant Computing (from section 9)

- `ComplianceBinding` — links a control to convergence point types via PointSelector
- `PointSelector` — Substrate | PointType | SubstrateAndType | Environment | DataClassification | All
- `VerificationPhase` — PlanTime | AtBoundary | PostConvergence
- `ComplianceControl` — framework + control ID (e.g., NIST AC-6, SOC2 CC6.1)
- `ComplianceClosure` — all controls bound to a convergence DAG, computed at plan time
- `CompliancePackage` — Nix expression packaging a complete compliance framework (versioned, composable)
- `DataClassification` — PII | PHI | PCI | Public | Internal | Confidential | TopSecret

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

## 11. Meta-Convergence: The System Optimizing Itself (Future)

### 11.1 Convergence on Convergence

The theory permits a natural but dangerous extension: **asymptotic convergence
points that optimize the execution of other convergence DAGs**. This is
convergence on the convergence topology itself — the system improving how it
converges, not just what it converges toward.

Examples:
- An asymptotic point that observes DAG execution telemetry and **fuses** sequential
  points that always run together into a single point (reducing boundary overhead)
- A point that detects false dependencies and **parallelizes** points that were
  sequenced unnecessarily
- A point that identifies hot paths and **caches** intermediate attestations that
  rarely change
- A point that reorders DAG evaluation for better resource utilization
- A point that detects recurring emission patterns from asymptotic points and
  **pre-instantiates** bounded DAGs before their triggers fire

This is essentially a **JIT compiler for convergence** — an optimizer that watches
the convergence engine run and restructures the computation while preserving
correctness.

### 11.2 Why This Requires Solid Primitives First

Meta-convergence is theoretically valid — it's just another asymptotic point with
an emission schema. But it is **unsafe without rock-solid foundational primitives**:

1. **Attestation chain preservation**: any DAG restructuring must preserve the
   integrity of the attestation chain. If you fuse two points, the fused point
   must produce an attestation that is verifiably equivalent to both originals.

2. **Semantic equivalence**: you must prove that the optimized DAG produces the
   same converged state as the original. This is the convergence analogue of
   compiler correctness proofs.

3. **Rollback safety**: if an optimization makes things worse (increased
   convergence time, new oscillations), the system must be able to revert to
   the unoptimized topology instantly.

4. **Halting guarantees for bounded points**: meta-convergence must not transform
   a bounded point into an asymptotic one (or vice versa). Horizon classification
   must be preserved.

5. **CALM classification preservation**: an optimization must not change a monotone
   operation into a non-monotone one, or the coordination guarantees break.

Until the type system, attestation chain, and DAG algebra are implemented and
proven correct, meta-convergence should remain theoretical. The foundation must
be unshakeable before we let the system modify its own structure.

### 11.3 The Meta-Convergence Emission Schema

When the primitives are ready, meta-convergence is expressed like any other
asymptotic point:

```nix
dagOptimizer = builtins.convergencePoint {
  type = "asymptotic";
  substrate = "compute";        # it optimizes compute topology
  direction = "minimize";       # minimize convergence time / resource usage
  metric = "convergence_time_p99_ms";

  emissionSchema = {
    fusePoints = {
      dagTemplate = ./meta/fuse-sequential.nix;
      trigger = { condition = "sequential_points_always_coexecute"; samples = 100; };
    };
    parallelizePoints = {
      dagTemplate = ./meta/parallelize-independent.nix;
      trigger = { condition = "false_dependency_detected"; confidence = 0.99; };
    };
    cacheAttestation = {
      dagTemplate = ./meta/cache-stable-attestation.nix;
      trigger = { condition = "attestation_unchanged_across_generations"; generations = 10; };
    };
    preInstantiate = {
      dagTemplate = ./meta/pre-instantiate-bounded.nix;
      trigger = { condition = "emission_pattern_predictable"; confidence = 0.95; };
    };
  };
};
```

Each emitted bounded DAG is a specific, typed, testable optimization operation.
The optimizer doesn't improvise — it applies known transformations from a catalog.
Schema gaps signal that a new optimization type should be formally defined.

## 12. Theoretical Frontiers

The following frontiers represent the edges of the theory — areas where convergence
computing either extends naturally, requires new primitives, or encounters fundamental
limits. Each frontier is classified:

- **Covered**: the current theory handles this
- **Extends**: the theory handles the concept but needs new types or mechanisms
- **Open**: requires fundamental new work
- **Limit**: a hard boundary the theory cannot cross

### 12.1 Temporal Convergence (Extends)

**Problem**: A TLS certificate is valid for 30 more days. Right now, distance = 0.
But we *know* it will diverge. The theory is currently reactive — observe divergence,
then converge. Should it be proactive?

**Extension**: Time becomes a convergence dimension. Every convergence state carries
a **time-to-divergence** (TTD) estimate. A cert with 30 days left has distance = 0
but TTD = 30d. When TTD drops below a threshold, the system pre-emptively
re-converges — not because the state IS diverged, but because it WILL BE.

```rust
struct ConvergenceState {
    // ... existing fields ...
    /// Estimated time until this point diverges (None = stable indefinitely).
    time_to_divergence: Option<Duration>,
    /// Threshold: re-converge proactively when TTD drops below this.
    proactive_threshold: Option<Duration>,
}
```

This is covered by the existing theory's asymptotic points — a cert-rotation
optimizer is an asymptotic point that emits bounded cert-renewal DAGs before
expiry. The extension is making TTD a first-class metric alongside distance
and rate.

### 12.2 Adversarial Convergence (Extends)

**Problem**: An attacker actively pushes the system away from convergence. DDoS
floods, supply chain compromises, infrastructure sabotage. The convergence
function fights an intelligent adversary, not just entropy.

**Extension**: Adversarial convergence requires:
- **Threat modeling as a substrate**: a Security substrate DAG that models active
  threats and adjusts convergence strategies (e.g., shift to hardened nodes when
  under attack)
- **Convergence under duress**: the system must distinguish between "diverged
  because of normal entropy" and "diverged because under attack" — the response
  is different (re-converge normally vs. activate defensive topology)
- **Game-theoretic convergence**: the convergence function accounts for an
  adversary's optimal strategy. This changes the CALM classification — adversarial
  operations are inherently non-monotone (the adversary can retract progress)

The theory handles this through substrate DAGs and CALM classification, but needs
new trigger conditions in emission schemas: `under_attack`, `anomalous_divergence`,
`attestation_chain_tampered`.

### 12.3 Resource Contention Between Substrates (Extends)

**Problem**: Minimizing cost (financial substrate) conflicts with maximizing
performance (compute substrate). You can't optimize all dimensions simultaneously.
Converging on one dimension may degrade another.

**Extension**: **Pareto-optimal convergence**. The multi-dimensional distance
vector (section 6.3) is subject to constraints:

```rust
struct SubstrateConstraints {
    /// Priority ordering when substrates conflict.
    priorities: Vec<SubstrateType>,
    /// Hard constraints: these dimensions MUST be converged regardless of cost.
    hard: Vec<SubstrateType>,     // e.g., Security, Regulatory
    /// Soft constraints: optimize these within budget.
    soft: Vec<SubstrateType>,     // e.g., Financial, Observability
    /// Trade-off policy: how to resolve conflicts between soft constraints.
    trade_off: TradeOffPolicy,    // e.g., WeightedSum, LexicographicOrder, Satisficing
}
```

This is not a new mechanism — it's a policy layer over the existing substrate DAG
system. The theory supports it; the implementation needs constraint solvers.

### 12.4 Observation Limits (Limit)

**Problem**: Convergence requires observation. Observation has latency (polling
interval) and cost (API calls, compute, bandwidth). The system cannot converge
faster than it can observe.

**Hard limits**:
- **Nyquist for convergence**: the observation rate must be ≥ 2× the rate of
  change in the substrate. If spot prices change every 30s, you must poll at
  least every 15s or you miss transitions.
- **Observer effect**: probes consume resources. Health checks add latency.
  Monitoring adds CPU load. Observation perturbs the system being observed.
- **Stale reads**: in a distributed system, observations are always slightly
  stale. The convergence function operates on past state, not current state.
  Convergence is always chasing a moving target.

The theory acknowledges these limits but does not solve them — they are
fundamental. The practical mitigation is: design convergence functions that
are tolerant of stale observations (already guaranteed by idempotency and
CALM classification), and set observation intervals based on substrate
volatility.

### 12.5 Trust Boundary Federation (Extends)

**Problem**: Convergence spans organizations, clouds, and regulatory domains.
How does organization A verify organization B's attestation? How do convergence
DAGs cross trust boundaries?

**Extension**: **Federated convergence stores**. Each organization runs its own
sui instance with its own attestation chain. Cross-boundary edges carry
**federated attestations** — attestation hashes signed by the foreign
organization's key, verified by the local organization's trust policy.

```
Org A convergence store ←──federated attestation──→ Org B convergence store
     (sui instance A)                                    (sui instance B)
```

This composes with tameshi's existing multi-layer attestation — the federation
layer is just another attestation layer in the Merkle tree. The theory supports
it; the implementation needs cross-store attestation verification in sui.

### 12.6 Human-in-the-Loop Convergence (Covered)

**Problem**: Some convergence points require human decisions — approval gates,
architecture reviews, compliance sign-offs. These are bounded points with
unpredictable latency.

**Status**: Already covered. A human approval is a bounded convergence point
with `mechanism: Manual`. The DAG blocks at the Gate point until the human
attests. The convergence engine tracks this point's `time_in_current_state`
and can escalate if it exceeds a threshold. No new theory needed — humans
are just slow convergence functions.

### 12.7 Convergence Failure and Degradation (Extends)

**Problem**: What if a point CAN'T converge? The hardware doesn't exist, the
budget is exhausted, a regulatory prohibition blocks the operation. How does
failure propagate through the DAG?

**Extension**: **Failure semantics** for convergence points:

```rust
enum ConvergenceOutcome {
    /// Converged successfully — distance = 0.
    Converged,
    /// Cannot converge — permanent failure.
    Failed { reason: String, compensating: Option<CompensatingAction> },
    /// Can partially converge — degraded operation.
    Degraded { achieved: ConvergenceDistance, missing: Vec<String> },
}
```

Failure propagation follows the DAG:
- A failed point's downstream dependents are **blocked** (cannot prepare)
- The system can invoke **compensating actions** (saga pattern, already in the
  theory) to roll back upstream points
- **Degraded convergence** allows the system to operate with partial convergence
  on some substrates — e.g., running on a more expensive node because the cheap
  one is unavailable, with cost_distance > 0 but all other dimensions converged

### 12.8 Bootstrapping: What Converges the Convergence Engine? (Open)

**Problem**: The convergence engine (tatara + sui) is itself a distributed system
that must be running before it can converge anything. But deploying the convergence
engine IS a convergence operation. Chicken and egg.

**Status**: This is an open problem. Practical mitigations:
- **Static bootstrap**: the first tatara node starts with a hard-coded DAG
  (no dynamic convergence planning), then transitions to full convergence once
  the engine is operational
- **External bootstrap**: a simpler system (nix-darwin, systemd, cloud-init)
  brings up the first node, then tatara takes over
- **Self-hosting horizon**: eventually, tatara converges its own upgrades
  (tatara-on-tatara), but this requires the meta-convergence primitives from
  section 10 to be stable

The theory does not claim self-bootstrap. It requires an external initial
condition, just as the Knaster-Tarski theorem requires an initial application
of the functional before the fixed point iteration begins.

### 12.9 Emergent Behavior and Cascade Failure (Extends)

**Problem**: Many convergence points interacting can produce behavior none of
them individually express. A cost optimizer migrates a workload → the network
substrate re-converges → DNS propagation causes a latency spike → the latency
optimizer migrates the workload back → the cost optimizer fires again. Cascade.

**Extension**: **Convergence circuit breakers**. The existing oscillation
detection (section 2.3) handles single-point oscillation. Cross-point cascades
need a graph-level detector:

- Track **causal chains**: if point A's convergence triggers point B's divergence
  triggers point A's divergence, that's a cycle in the causal graph (even though
  the DAG itself is acyclic — the causality spans multiple DAG generations)
- **Circuit breaker**: when a causal cycle is detected, freeze the lower-priority
  substrate and allow the higher-priority one to stabilize
- **Dampening across substrates**: apply control-theory damping not just per-point
  but per-substrate-pair when cross-substrate oscillation is detected

### 12.10 Scale Limits (Extends)

**Problem**: At extreme scale (millions of convergence points across thousands
of nodes), the convergence plan itself becomes a bottleneck. DAG evaluation,
closure computation, and attestation verification have computational cost.

**Extension**: **Sharded convergence graphs**. The convergence graph is partitioned
by substrate and by scope (cluster, region, global). Each shard has its own sui
instance. Cross-shard edges are federated (section 11.5). The parent DAG-of-DAGs
coordinates shards without needing the full graph in memory.

This is analogous to how Nix handles large package sets — nixpkgs doesn't evaluate
everything at once; it lazily evaluates the closure of what you requested. Sui's
lazy evaluation (thunks) already supports this pattern.

### 12.11 Non-Determinism and Concurrent Observation (Covered)

**Problem**: In a distributed system, two nodes may observe different states
simultaneously. Network partitions, clock skew, and concurrent mutations mean
observations are inherently non-deterministic.

**Status**: Already covered by the CALM classification (section 1.2). Monotone
operations converge regardless of observation order (eventual consistency via
gossip). Non-monotone operations go through Raft (linearizable consensus).
The theory doesn't require deterministic observations — it requires convergence
functions that are idempotent and goal-seeking, which tolerate stale or
partial observations by design.

### 12.12 Convergence Velocity Constraints (Extends)

**Problem**: Different substrates have hard limits on how fast state can change:

| Substrate | Velocity Limit | Why |
|-----------|---------------|-----|
| DNS | Minutes (TTL) | Propagation through resolver caches |
| TLS certificates | Minutes | CA issuance + ACME challenge |
| Cloud provisioning | Minutes to hours | VM boot, image pull, network config |
| Legal/compliance | Days to weeks | Human review, regulatory process |
| Hardware procurement | Weeks to months | Supply chain, shipping |

**Extension**: Each substrate DAG carries a **convergence bandwidth** — the
maximum rate at which convergence can proceed. The convergence planner uses
bandwidth constraints to compute realistic critical-path estimates. A DAG
that includes a compliance approval gate cannot estimate sub-minute completion
regardless of how fast the compute points converge.

```rust
struct SubstrateDAG {
    // ... existing fields ...
    /// Maximum convergence velocity for this substrate.
    bandwidth: ConvergenceBandwidth,
}

enum ConvergenceBandwidth {
    Instant,                    // local computation, in-memory
    Seconds(u64),               // API calls, cache lookups
    Minutes(u64),               // provisioning, cert issuance
    Hours(u64),                 // large deployments, data migration
    Days(u64),                  // compliance, procurement
    Unbounded,                  // human decisions, external processes
}
```

### 12.13 Information Loss and Convergence Archaeology (Extends)

**Problem**: Generational store paths accumulate. GC destroys old generations.
But six months from now, someone asks: "Why did the system migrate from node-a
to node-b on March 15?" The attestation that explains this may have been GC'd.

**Extension**: **Convergence archival**. Before GC, attestation summaries
(point ID, generation, trigger, outcome, timestamp) are appended to an
append-only log outside the store. The full attestation content is GC'd, but
the summary chain persists indefinitely. This is the convergence equivalent of
git reflog — the detailed objects may be pruned, but the decisions are recorded.

### 12.14 External Systems That Don't Speak Convergence (Covered)

**Problem**: AWS, GCP, third-party SaaS — they don't have convergence DAGs.
How does the system converge state in systems that have no concept of convergence?

**Status**: Already covered. This is exactly what the convergence function does —
it takes an external system's observed state (via API polling) and drives it
toward desired state (via API mutation). The external system doesn't need to
understand convergence. The convergence point is the adapter. This is the
level-triggered controller pattern (section 1, K8s controller pattern reference):
compare desired vs. observed each tick, act on the diff. The external system
is just a substrate.

### 12.15 Convergence and Consciousness (Limit)

**Problem**: If an asymptotic convergence point can observe its own execution,
optimize its own topology (meta-convergence), and emit new convergence points
in response to novel stimuli (schema gap → define new type), at what point does
the convergence graph exhibit properties that resemble autonomous goal-seeking?

**Status**: This is a philosophical limit, not a technical one. The theory models
goal-seeking computation (convergence toward desired state), self-modification
(meta-convergence), and adaptation (emission schemas evolving over time). But
the goals are always externally specified (declared in Nix by a human). The
system never invents its own goals — it converges toward goals it is given.

The theory explicitly does NOT cross this boundary. Meta-convergence optimizes
HOW the system converges, not WHAT it converges toward. Emission schemas define
the catalog of possible actions, but a human defines the catalog. Schema gaps
are escalated to humans, not resolved autonomously.

This is a design choice, not a theoretical limitation. The theory could model
autonomous goal generation (an asymptotic point whose emission schema includes
"define new asymptotic points"), but we deliberately choose not to. The
convergence engine is a tool, not an agent. It computes what it is told to
compute, and it computes it well.

## 13. Theory Summary

The Unified Convergence Computing Theory establishes:

| # | Principle | Section |
|---|-----------|---------|
| 1 | Convergence IS computation (fixed-point, CALM, CRDTs) | §1 |
| 2 | Every computation has atomic verified boundaries (prepare→execute→verify→attest) | §1.3 |
| 3 | Convergence points compose into typed DAGs, DAGs into DAGs-of-DAGs | §1.4–1.5, §5 |
| 4 | Some points are bounded (terminate), others asymptotic (run forever) | §1.6 |
| 5 | Bounded is preferred; asymptotic points are factories for bounded work | §1.6 |
| 6 | Convergence distance, rate, and oscillation are the metrics | §2 |
| 7 | Infrastructure theory says WHAT, convergence theory says HOW | §3 |
| 8 | Cost optimization is continuous re-convergence on the financial substrate | §4 |
| 9 | Convergence points are typed (Transform/Fork/Join/Gate/Select/...) | §5.1 |
| 10 | DAGs have a closed algebra (sequence, parallel, fork, join, select, nest) | §5.2 |
| 11 | Convergence DAGs ARE Nix derivations (content-addressed, cached, closures) | §5.3 |
| 12 | Every substrate is its own convergence DAG; distance is a vector | §6 |
| 13 | Full static analysis at plan time (closures, impact, diff, cache hits) | §7 |
| 14 | The Nix store (via sui) IS the convergence store | §8 |
| 15 | Compliance is a static DAG property, verifiable at plan time | §9 |
| 16 | Three theories compose: infrastructure (WHAT), convergence (HOW), compliance (WHETHER) | §9.1 |
| 17 | Compliance controls bind to point types, not instances (type-level compliance) | §9.3 |
| 18 | Tameshi CertificationArtifact IS convergence boundary attestation | §9.4 |
| 19 | Compliance verification is itself convergence DAGs (cacheable, attestable) | §9.5 |
| 20 | Compliance is packageable as Nix expressions (versioned, composable, distributable) | §9.10 |
| 21 | Meta-convergence (system optimizing itself) is valid but needs solid primitives | §11 |
| 22 | The theory has known frontiers and deliberate limits | §12 |

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
| Game theory | von Neumann & Morgenstern 1944 | Adversarial convergence against intelligent opponents |
| Pareto optimality | Edgeworth 1881 / Pareto 1906 | Multi-substrate conflict resolution |
| Circuit breakers | Nygard 2007 (Release It!) | Cross-substrate cascade failure prevention |
| Nyquist-Shannon sampling | Shannon 1949 | Observation rate limits on convergence velocity |
| JIT compilation | Aycock 2003 | Meta-convergence as runtime DAG optimization |
| Reflective systems | Smith 1984 (procedural reflection) | System reasoning about its own convergence topology |
| Compensating transactions | Garcia-Molina & Salem 1987 | Failure recovery and degraded convergence |
| NIST 800-53 | NIST 2020 (Rev 5) | Security and privacy control families |
| OSCAL | NIST 2021 | Machine-readable compliance assessment language |
| Merkle attestation | Certificate Transparency (RFC 9162) | Append-only compliance proof via domain-separated leaves |
| Static compliance analysis | Terraform Sentinel / OPA | Policy-as-code evaluated at plan time |
| Zero-cost testing | Pangea synthesis pattern | Compliance on derivation structure, not execution |
