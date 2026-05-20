# Tatara (粋) — Programmable Convergence Computer

> **★★★ CSE / Knowable Construction.** This repo operates under **Constructive Substrate Engineering** — canonical specification at [`pleme-io/theory/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md`](https://github.com/pleme-io/theory/blob/main/CONSTRUCTIVE-SUBSTRATE-ENGINEERING.md). The Compounding Directive (operational rules: solve once, load-bearing fixes only, idiom-first, models stay current, direction beats velocity) is in the org-level pleme-io/CLAUDE.md ★★★ section. Read both before non-trivial changes.

> **skip-format-ban: migration in progress.** The workspace has 81
> pre-existing `format!(…)` callsites that pre-date the ★★ Typed Emission
> directive — most are inside `anyhow!(…)` / `tracing::…!(…)` macro
> arguments or compose simple K8s annotation strings (`{ns}/{name}`).
> The audit lives in P7; until it's swept, no `clippy.toml` with
> `disallowed_macros` is wired at the workspace root. **New code in
> this repo should still avoid `format!(…)` of platform syntax (YAML,
> Nix, Go AST, Helm).** The ban is appropriate; the migration is
> mechanical but unfinished.


<!-- Blackmatter alignment: pillars 1, 6, 10 -->
<!-- See ~/code/github/pleme-io/BLACKMATTER.md for pillar definitions. -->

## Theory (canonical elsewhere)

The theoretical frame lives in
[`pleme-io/theory/THEORY.md`](../theory/THEORY.md):

- **§II.1** — the Rust + Lisp pattern, five invariants, six-line
  contract. Tatara IS the Lisp half; `#[derive(TataraDomain)]` is the
  boundary other repos plug into.
- **§II.2** — the Four Lisps. tatara-lisp is the strictest reference.
- **§III** — the typescape. Tatara's registry gives every new typed
  domain a deterministic BLAKE3 identity + workspace-wide coherence
  check.
- **§IV** — convergence (lattice and process), controllers, the
  eight-phase loop, Unix-process-cluster model, seven questions.
- **§V.3** — three-pillar attestation (`artifact_hash ⊕ control_hash ⊕
  intent_hash → BLAKE3 Merkle`). Tatara writes `ProcessAttestation`
  using this exact shape.

**Canonical cookbook:** [`docs/rust-lisp.md`](docs/rust-lisp.md) — the
manifesto + anti-patterns for the pattern.

## Blackmatter pillars upheld

- **Pillar 1** (Rust + tatara-lisp + WASM/WASI): Tatara IS the Lisp
  half of Pillar 1.
- **Pillar 6** (Typescape): `TataraDomain` registry gives every new
  typed domain a deterministic BLAKE3 identity + workspace-wide
  coherence check.
- **Pillar 10** (Proofs): The coherence checker is itself a proof —
  every registered domain's dispatch is verified at compile time.

## Workspace crates (14+)

### Core runtime (pre-existing)

| Crate | Purpose |
|-------|---------|
| `tatara-core` | Domain types: convergence state, WorkloadPhase, DAG, saga, idempotency, traced events |
| `tatara-engine` | Runtime: 7 drivers, Raft, gossip, convergence engine, scheduler, health probes, catalog, metrics, sui client |
| `tatara-net` | Networking plane: NetworkPlane trait, eBPF types, WASI types, mesh, flow observability |

### K8s-as-processes surface (v1alpha1 — Apr 2026)

| Crate | Purpose |
|-------|---------|
| `tatara-process` | **Process + ProcessTable CRDs** — K8s-as-Unix-processes wire format (`tatara.pleme.io/v1alpha1`). `ProcessSpec` derives `TataraDomain` so `(defpoint …)` in Lisp is a first-class authoring surface. Houses `compile_source` + `tatara-lispc` binary. Absorbs `ConvergenceProcess`, `ConvergenceService`, `NixBuild`. **Ephemeral surface** (P0, 2026-05): adds `Intent::Aplicacao`, `Lifetime::Ephemeral`, `ConditionKind::{JobAttested,ClosedLoopAuth}`, and a typed `EphemeralSpec` sugar (`(defephemeral …)`) that lowers to `ProcessSpec` via `From`. **Receipt envelope** (P8, 2026-05): `tatara-receipt/v1` schema as typed `ReceiptEnvelope` — fleet-wide reusable proof artifact emitted by any Job (closed-loop probes, shinka migrations, kenshi suites, nix builds). `parse_either(json|yaml)`, `verify_root`, `to_attestation` bridges into the Process attestation chain. **Shared lifetime clock** (2026-05): `lifetime_clock::evaluate(&Process, phase, now)` — pure typed decision (`AutoTerminate::{Skip, Now}`) that any controller respecting `Lifetime::Ephemeral` consumes. Lifted out of `tatara-reconciler` so kenshi + future Process controllers share one TTL/teardown implementation. |
| `tatara-lattice` | Lattice algebra over `Classification` — `meet` / `join` / `leq` / `Baseline`. Replaces `qualities_match`. |
| `tatara-lisp` | **Homoiconic S-expression surface.** Reader, AST, macroexpander (quasi-quote + unquote + splice + `&rest`), `TataraDomain` trait, domain registry, `TypedRewriter` (self-optimization primitive), generic `compile_typed`/`compile_named`, iac-forge canonical-form interop (feature-gated). |
| `tatara-lisp-derive` | **`#[derive(TataraDomain)]`** — proc macro that auto-generates a Lisp compiler for any struct with `serde::Deserialize`. Universal-Deserialize fallthrough handles enums, nested structs, `Vec<Nested>`. Honors `#[serde(default)]`. |
| `tatara-domains` | Reference typed domains (MonitorSpec, NotifySpec, Severity enum, EscalationStep, AlertPolicySpec) + `register_all()` registry seed. Demonstrates every derive kind. |
| `tatara-reconciler` | **FluxCD-adjacent K8s controller.** 10-phase Unix lifecycle. Owner-ref-emitted Kustomizations. Signal annotation ingestion. Finalizer-guarded termination. Three-pillar BLAKE3 attestation chain. `tatara-check` binary runs `checks.lisp`. Replaces `tatara-kube`. **P2 (2026-05):** `render::render_aplicacao` emits a FluxCD `HelmRelease` + (for `oci://` chart refs) an `OCIRepository`, both owned by the Process; `lifetime_clock` module enforces TTL expiry + `TeardownPolicy` transitions through Running/Attested/Failed — ephemeral envs auto-cascade to `Exiting → Zombie → Reaped` without operator intervention. **P4-reconciler (2026-05):** typed `JobAttested` + `ClosedLoopAuth` evaluators read `batch/v1` Job status + a `<job>-receipt` ConfigMap carrying a three-pillar BLAKE3 receipt (`version: tatara-receipt/v1`); the closed-loop postcondition holds iff the receipt's `composed_root` parses and (optionally) matches the operator's expected root. |

### Operational surfaces

| Crate | Purpose |
|-------|---------|
| `tatara-api` | REST (Axum) + GraphQL (async-graphql): jobs, allocations, nodes, catalog, health, metrics |
| `tatara-cli` | CLI + `tatara server` |
| `tatara-mcp` | MCP tool surface (will absorb convergence-controller's 15 tools) |
| `tatara-testing` | Test fixtures and helpers |
| `ro-cli` | Read-only CLI |

### Attestation tooling

| Crate | Purpose |
|-------|---------|
| `tatara-closed-loop-probe` | Closed-loop auth probe binary — verifies a system's bundled identity issuer authenticates its own bundled consumer, then emits a typed `tatara-receipt/v1` envelope to a ConfigMap. Pure Rust (reqwest + blake3 + kube-rs), NO SHELL. Consumed by `akeyless-closed-loop-probe-pleme` Helm chart; substrate primitive that any future closed-loop-testable consumer (databases, identity providers, message brokers) can shape their probe binary around. |

### Deprecated

| Crate | Replaced by |
|-------|-------------|
| `tatara-kube` | `tatara-reconciler` (FluxCD-adjacent, not bypassing) — see `tatara-kube/DEPRECATED.md` |
| `tatara-operator` | `Intent::Nix` field in `Process` (NixBuild semantics absorbed) — see `tatara-operator/DEPRECATED.md` |

## K8s-as-Processes model (v1alpha1 — repo specifics)

For the _why_ (Unix process model applied to clusters, hierarchical
PIDs, fork/exec/wait/kill semantics, inception isolation, self-hosting),
see [`theory/THEORY.md` §IV.4](../theory/THEORY.md). This section
documents what is specific to `tatara-reconciler`.

### State machine

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
   `convergence-controller/src/identity.rs`.
2. **Classification** — 6-axis lattice position (re-exports from `tatara-core`).
3. **Intent** — one of `nix` / `flux` / `lisp` / `container` / `aplicacao` /
   `guest`. The RENDER phase dispatches on the variant. `aplicacao` emits a
   FluxCD `HelmRelease` for a pleme-io typed Aplicacao chart — the canonical
   handoff from caixa `(defaplicacao …)` declarations.
4. **Boundary** — `preconditions` gate Running; `postconditions` gate Attested.
   `ConditionKind`: `ProcessPhase`, `KustomizationHealthy`, `HelmReleaseReleased`,
   `PromQL`, `Cel`, `NixEval`, `JobAttested`, `ClosedLoopAuth`. The
   `ClosedLoopAuth` kind turns "the gateway↔SaaS loop holds" from an
   assertion into a theorem provable on every ephemeral run.
5. **Compliance bindings** — verified at `PlanTime` | `AtBoundary` |
   `PostConvergence`.
6. **Signals** — `SIGHUP | SIGTERM | SIGKILL | SIGUSR1 | SIGUSR2 | SIGSTOP |
   SIGCONT` delivered via `tatara.pleme.io/signal` annotation.
7. **Lifetime** — `Permanent` (default; SIGHUP re-converges) or `Ephemeral`
   (auto-SIGTERM on `Attested` / `Failed` per `TeardownPolicy`, with TTL).
   Ephemeral envs are a `Process` with `:intent (:aplicacao …)` plus
   `:lifetime (:ephemeral …)` — no new CRD, no new controller. The
   `(defephemeral …)` keyword is sugar that `From<EphemeralSpec>` lowers
   into a `ProcessSpec` of this exact shape.

> **Lisp bools: Scheme `#t` / `#f`, not `true` / `false`.** The reader
> treats bare `true`/`false` as symbols → strings, which silently
> breaks `serde_json::Value` fields expecting bool. Use `#t`/`#f` in
> every `:values-overlay` payload.

### Ephemeral story — destination state (2026-05)

End-to-end loop, all on `main`, all tested:

```
(defephemeral …) Lisp form            (tatara-process / examples)
  → EphemeralSpec via TataraDomain    (tatara-process)
  → ProcessSpec via typed From        (tatara-process)
  → reconciler render::render_aplicacao emits:
       OCIRepository + FluxCD HelmRelease
  → chart deploys SaaS + Gateway + closed-loop-probe Job
       (helmworks-akeyless / akeyless-closed-loop-probe-pleme)
  → tatara-closed-loop-probe binary runs the 5-step probe:
       1. POST creds → issuer → JWT
       2. blake3(JWT)                                   → artifact_hash
       3. GET issuer JWKS → blake3(body)                → intent_hash
       4. POST JWT → consumer → verify                  → verdict
       5. blake3(verdict) ⊕ rest → composed_root
       Writes ReceiptEnvelope (tatara-receipt/v1) to ConfigMap.
  → tatara-reconciler ClosedLoopAuth evaluator reads ConfigMap,
       deserializes as typed ReceiptEnvelope, verifies shape + root.
  → Process → Attested
  → lifetime_clock fires teardown_policy=OnAttested
  → Exiting → Zombie → Reaped → ownerRefs cascade-delete
```

Reusable substrate primitives produced (NOT Akeyless-specific):

| Primitive | Crate | Who else uses it |
|-----------|-------|------------------|
| `Intent::Aplicacao` | tatara-process | any Helm-chart-driven Process |
| `Lifetime::{Permanent,Ephemeral}` + `TeardownPolicy` | tatara-process | any timed Process; shared decision via `lifetime_clock::evaluate` |
| `ConditionKind::JobAttested` | tatara-process | any Job-based postcondition |
| `ConditionKind::ClosedLoopAuth` | tatara-process | any self-testing system (DBs, IdPs, brokers) |
| `EphemeralSpec` + `(defephemeral …)` | tatara-process | typed authoring sugar |
| `ReceiptEnvelope` (`tatara-receipt/v1`) | tatara-process | every Job receipt — shinka, kenshi, nix-build, probes |
| `lifetime_clock::evaluate` | tatara-process | any controller respecting `Lifetime::Ephemeral` |
| `tatara_process::register_all()` | tatara-process | any binary using the Lisp dispatcher |
| `lisp-compiles :domain` | tatara-check | any new typed-domain coherence check |
| `render::render_aplicacao` | tatara-reconciler | the canonical Aplicacao→Flux emitter |
| `tatara-closed-loop-probe` binary | this workspace | any closed-loop probe; takes typed flags |
| `akeyless-closed-loop-probe-pleme` chart | helmworks-akeyless | Job + RBAC for the probe |

### Ephemeral story — deferred milestones + migration plans

These are NOT done; they're documented here so future agents pick up
exactly where this lands. Each can ship as a standalone session.

#### P1 — caixa-tatara renderer (arch-synthesizer)

**What:** `(defaplicacao …)` caixa Aplicacao with `:lifetime :ephemeral`
slot renders mechanically to a `Process` CR with `Intent::Aplicacao` +
`Lifetime::Ephemeral`. Today: operators author `(defephemeral …)`
directly. Destination: the higher-level Aplicacao surface lowers to
the same ProcessSpec via a typed renderer.

**Migration path:**
1. arch-synthesizer's caixa-mesh renderer already emits cluster
   artifacts for Aplicacao membros. Add a peer `caixa-tatara` renderer.
2. Input: `Aplicacao { lifetime: Ephemeral { ttl, teardown }, ... }`.
3. Output: a `Process` YAML with `intent.aplicacao` pointing at the
   chart caixa-helm emitted, `lifetime.ephemeral` populated, and
   `boundary.postconditions` derived from `:contratos` (typed
   ClosedLoopAuth/JobAttested per membro contract).
4. Snapshot test: render same Aplicacao through caixa-tatara, parse
   the YAML via `serde_yaml::from_str::<ProcessSpec>`, assert
   structural equality with a hand-authored reference Process.

**Not blocked on anything** — every typed primitive it needs already
exists. Estimated ~300 LoC of Ruby renderer + 1 RSpec synthesis test.

#### P3 — kenshi-runner library lift (kenshi)

**What:** `src/ephemeral/test_runner.rs` (772 LoC, tightly coupled to
kenshi's `TestEnvironment` CRD) becomes a reusable library crate
`kenshi-runner`. tatara-reconciler's `ConditionKind::JobAttested`
evaluator gains the option to *create* the test Job (not just verify
it) by calling into kenshi-runner.

**Migration path:**
1. Convert kenshi from single-crate to 2-crate workspace
   (`kenshi-runner` lib + `kenshi` bin that depends on it).
2. Move `src/ephemeral/test_runner.rs` to `kenshi-runner/src/lib.rs`.
3. Decouple from CRD types: replace `TestEnvironment` / `TestSuiteEntry`
   parameters with a `TestSuiteSpec` struct in kenshi-runner whose
   shape kenshi's CRD types convert into via `From`. tatara-reconciler
   can construct the same `TestSuiteSpec` from
   `boundary.Condition.params` JSON for `JobAttested`.
4. kenshi binary continues to work unchanged via the From bridge.
5. Mark kenshi's TestEnvironment CRD as deprecated alias for
   `Process { lifetime: Ephemeral, postconditions: [JobAttested...] }`
   per the destination-state design.

**Not blocked on anything.** Estimated ~500 LoC of refactoring +
~100 LoC of new From impls. The 9-state machine in kenshi maps
cleanly onto the existing 10-phase Process lifecycle.

#### P5 — shigoto Dag refactor of reconciler internal pipeline

**What:** Replace the hand-rolled `phase_machine.rs` enum dispatch
with a `shigoto::Dag` of typed `RecordingJob`s. Each phase becomes a
Job with Budget/Retry/Gate; the reconciler delegates wave execution
to `shigoto-scheduler::InProcessScheduler`. Satisfies criterion 1 for
shigoto's ★★ promotion (second production consumer after tend).

**Migration path:**
1. Add `shigoto-types`, `shigoto-scheduler`, `shigoto-emit` workspace
   deps to tatara-reconciler.
2. Wrap each existing `handle_*` function as a `RecordingJob` impl
   producing typed outputs (phase transitions are the output sink).
3. Compose the Dag: pending → forking → execing → running → attested
   with conditional edges for reconverging / exiting / failed.
4. Replace `kube::runtime::controller::Action::requeue(...)` with
   shigoto's typed retry/budget tree. SIGSTOP/SIGCONT map to
   `Scheduler::pause`/`resume`.
5. Audit emitter: write transitions to a typed `AuditFileEmitter`
   under `~/.local/state/tatara-reconciler/`.

**Largest of the three.** ~1500 LoC of refactor + extensive test
parity work. Cannot half-ship without leaving the FSM in two
implementations. Treat as its own focused milestone.

#### P6-remainder — shikumi defaults + HM/NixOS/Darwin module trio — ✓ DONE

Shipped: `tatara_reconciler::ephemeral_defaults::EphemeralDefaults` +
the three module files (`module/{home-manager,nixos,darwin}/
tatara-reconciler-ephemeral.nix`). See the **EphemeralDefaults**
section in this file for the schema + module surface.

Operator runtime UX (`feira ephemeral up|down|status|list|wait`)
shipped in `caixa-feira` — see the **Ephemeral story — destination
state** section above for the full closed-loop workflow.

#### Saguão vigia auto-Issuer — ✗ NOT REQUIRED

Earlier migration plans listed this as a follow-up. After re-checking
the closed-loop ephemeral pattern: it isn't needed. The closed-loop
runs ClusterIP + in-cluster HTTP between the bundled SaaS + Gateway;
no Ingress, no per-namespace TLS, no per-namespace Issuer to derive.
The reconciler's `EphemeralDefaults.root_ca_name` references a
cluster-wide `ClusterIssuer` for the rare case where an ephemeral
env wants externally-reachable TLS (operator-supplied; not
auto-derived). **Removed from the deferred list as a category error.**

#### Probe binary image — Nix builder wired, image not yet published

The substrate `rust-tool-image-flake.nix` build path is wired for
`tatara-closed-loop-probe`; `nix build .#closed-loop-probe-image-amd64`
produces the OCI tarball. To complete: run the substrate's release
pipeline (`nix run .#release-closed-loop-probe`) once and push the
result to `ghcr.io/pleme-io/closed-loop-probe:0.1.0`. That's a CI/CD
step, not a source-code change.

### FluxCD is `exec(2)`

`tatara-reconciler` does **not** replace source-controller /
kustomize-controller / helm-controller. It *emits* Flux CRs (annotated
with process metadata) and watches their status as part of the VERIFY
phase. A cluster running tatara-reconciler looks like a cluster running
FluxCD *plus* the `Process` CRD with three-pillar attestation
annotations on every owned resource.

### Four rendering surfaces, one type

```
Nix module      ──┐
YAML (kubectl)  ──┤
Rust builder    ──┼──►  ProcessSpec  ──►  tatara-reconciler
S-expr (lisp)   ──┘
```

Each surface produces the same `ProcessSpec`. The S-expr form is
homoiconic — macros can compose proven Process templates into new
Processes. All four surfaces are projections of the same typescape
slice (see [`theory/THEORY.md` §II.2](../theory/THEORY.md) for the
Four-Lisps framing).

### Three-pillar attestation (repo-specific composition)

```
composed_root = BLAKE3(
    "tatara-process/v1alpha1\n"
    ++ artifact_hash     // rendered resources + applied status
    ++ control_hash?     // compliance proof (empty iff no bindings)
    ++ intent_hash       // canonical spec + nix store path + lisp AST
    ++ previous_root?    // chain to prior attestation
)
```

`previous_root` chains each generation; `sekiban` + `kensa` consume the
composed root as the audit-trail anchor. Canonical three-pillar form
defined in [`theory/THEORY.md` §V.3](../theory/THEORY.md).

## Homoiconic Lisp surface — the authoring / rewriting layer

**`#[derive(TataraDomain)]`** is the one-liner that unlocks a Lisp
authoring surface for any `serde::Deserialize` struct. Applied to
`ProcessSpec` itself (and to MonitorSpec / NotifySpec / AlertPolicySpec
in tatara-domains).

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

The five invariants of the pattern (typed entry, free middle, typed
exit, deterministic identity, composition preserves proofs) are
canonical in [`theory/THEORY.md` §II.1](../theory/THEORY.md). Tatara
is the reference implementation.

### `checks.lisp` — workspace coherence, Lisp-driven

`cargo run --bin tatara-check -p tatara-reconciler` reads `checks.lisp`
at workspace root and dispatches each form through a typed Rust
executor:

- **Built-in primitives**: `crd-in-sync`, `yaml-parses`, `yaml-parses-as`,
  `lisp-compiles`, `file-contains`
- **User-defined macros**: `(defcheck name (params) `(do …primitive-calls))`
- **Registry fallthrough**: any `(defX …)` form whose keyword matches a
  registered `TataraDomain` is compiled typed — no built-in handler
  needed

11 runtime checks pass, including compiling `observability-stack.lisp`
to `ProcessSpec` via the derive + registry + `defalertpolicy` /
`defmonitor` / `defnotify` via the registry fallthrough. Zero shell.

### Reuse boundary with iac-forge

Three S-expression layers, non-overlapping:

| Layer | Type | Purpose |
|-------|------|---------|
| Authoring | `tatara_lisp::Sexp` | Homoiconic, macro-capable, human-written |
| Typed | `ProcessSpec`, etc. | Exhaustive sum types, compile-time proof |
| Canonical | `iac_forge::sexpr::SExpr` | BLAKE3 attestation + render cache |

`tatara-lisp --features iac-forge` provides `From<Sexp> for
iac_forge::SExpr` so tatara plugs into the existing attestation
pipeline.

## Key types (`tatara-core/src/domain/convergence_state.rs`)

- `ConvergenceDistance`: Converged | Partial | Diverged | Unknown (0.0 to 1.0)
- `ConvergenceState`: distance + rate + oscillation + damping per entity
- `ConvergencePoint`: named step with CALM classification + typed boundary
- `ConvergenceBoundary`: preconditions + postconditions + attestation chain
- `BoundaryPhase`: Pending → Preparing → Executing → Verifying → Attested | Failed
- `ClusterConvergence`: cluster-wide summary (is_fully_healthy + is_fully_converged)
- `CalmClassification`: Monotone | NonMonotone
- `ConvergenceMechanism`: Raft | Gossip | Local | Nats | FixedPoint | Feedback

## Measured performance

Three-layer expander (substitute / bytecode / bytecode+cache) — 1.40×
speedup on cache-friendly workloads. All layers optional, orthogonal,
proven-equivalent by test.

## 7 execution drivers

| Driver | Backend | Platform |
|--------|---------|----------|
| `exec` | Direct process (fork+exec) | Unix |
| `oci` | Docker/Podman/Apple Containers | All |
| `nix` | `nix run <flake_ref>` | All with Nix |
| `nix_build` | `nix build` + sui-cache push | All with Nix |
| `kasou` | Apple Virtualization.framework VMs | macOS |
| `kube` | Kubernetes Server-Side Apply | All with kubeconfig |
| `wasi` | wasmtime WASI Preview 2 | All with wasmtime |

## WorkloadPhase lifecycle

```rust
enum WorkloadPhase<W, E, C, T> {
    Initial,          // Defined but not active
    Warming(W),       // Acquiring resources, resolving deps
    Executing(E),     // Active, healthy, serving
    Contracting(C),   // Gracefully draining
    Terminal(T),      // Done
}
```

## Distributed state machine

- **Raft** (openraft): linearizable writes for placement, allocation lifecycle
- **Gossip** (chitchat): eventually-consistent metadata, failure detection
- **CQRS**: desired vs observed split in ClusterState
- **Leader-affinity**: only the leader schedules

## The Tatara/Sui split

| Concern | Sui | Tatara |
|---------|-----|--------|
| Role | Store + evaluator + planner | Engine + executor |
| Input | Nix expressions | Convergence derivations |
| Output | Derivation graph + store paths | Attested convergence state |
| State | Content-addressed (immutable) | Live convergence (mutable) |
| Distribution | sui-cache binary cache | Raft + gossip |
| API | REST + GraphQL + gRPC | REST + GraphQL + SSE |

For the duality, see [`theory/THEORY.md` §VIII.5](../theory/THEORY.md).

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

## Nix integration

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
| [rust-lisp.md](docs/rust-lisp.md) | The manifesto + cookbook + anti-patterns for the Rust+Lisp pattern | ~625 |
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
