# K8s as Unix Processes

Every reconciled Kubernetes object in tatara is a **Unix process** in the
convergence lattice. Clusters, HelmReleases, DB migrations, compliance checks,
tests — all are `Process` CRDs. The reconciler's state machine literally
implements Unix semantics.

## One CRD, three realities

```yaml
apiVersion: tatara.pleme.io/v1alpha1
kind: Process
metadata:
  name: observability-stack
  namespace: seph
spec:
  identity:
    parent: seph.1                        # PID tree
  classification:                         # lattice position (6 dims)
    pointType: Gate
    substrate: Observability
    horizon: { kind: Bounded }
    calm: Monotone
    dataClassification: Internal
  intent:                                  # where rendered artifacts come from
    nix: { flakeRef: "github:…", attribute: observability }
  boundary:
    postconditions:                       # predicates that gate Attested
      - kind: KustomizationHealthy
        params: { name: observability-stack, namespace: flux-system }
      - kind: PromQL
        params: { query: "up{job='prometheus'} == 1" }
    timeout: 15m
  compliance:
    baseline: fedramp-moderate
    bindings:
      - { framework: nist-800-53, controlId: SC-7, phase: AtBoundary }
  dependsOn:
    - { name: akeyless-injection, mustReach: Attested }
  signals:
    sigtermGraceSeconds: 480
    sighupStrategy: Reconverge
```

The same `Process` carries:
1. **Identity** — hierarchical PID in `ProcessTable` (`/proc`). Content-addressable
   BLAKE3 (128-bit, 26-char Crockford base32).
2. **Classification** — 6-axis lattice position via `tatara-core` types.
3. **Intent** — one of `nix`, `flux`, `lisp`, `container`. Dispatched at RENDER.
4. **Boundary** — preconditions gate Running; postconditions gate Attested.
5. **Compliance** — bindings verified at PlanTime | AtBoundary | PostConvergence.
6. **Dependencies** — `dependsOn` enforced in Forking via `tatara-lattice::check_depends_on`.
7. **Signals** — first-class CRD verbs via annotations.

## The 10-phase state machine

```
Pending → Forking → Execing → Running → Attested
                                       ↘ Failed
Attested → Reconverging → Execing               (SIGHUP path, no zombie)
Running  → Exiting      → Zombie → Reaped       (SIGTERM path)
```

| Phase | Unix analog | What happens |
|-------|-------------|--------------|
| `Pending` | admitted, not scheduled | canonicalize spec, install finalizer, derive identity |
| `Forking` | `fork()` | allocate PID from `ProcessTable.nextSequence`; check `dependsOn` |
| `Execing` | `exec()` | SIMULATE + PROVE (preconditions) + RENDER → emit FluxCD CRs |
| `Running` | `waitpid()` | poll Flux CRs Ready; evaluate postconditions |
| `Attested` | `exit(0)` | compose three-pillar BLAKE3; heartbeat; drift detection |
| `Reconverging` | SIGHUP response | re-enter Execing without teardown |
| `Exiting` | SIGTERM response | cascade-delete children; drain |
| `Failed` | non-zero exit | transition to Zombie for reap |
| `Zombie` | exited, unreaped | await finalizer release |
| `Reaped` | fully terminated | release finalizer, K8s GC cascade-deletes owned Flux CRs |

## FluxCD is `exec(2)`

`tatara-reconciler` does **not** replace source-controller /
kustomize-controller / helm-controller. It *emits* their CRs (annotated with
Process metadata) and watches their `status.conditions[Ready]` as part of
VERIFY. A cluster running tatara-reconciler looks like a cluster running FluxCD
*plus* the Process CRD and attestation annotations on every owned resource.

Emitted Kustomizations are placed in the Process's namespace (not `flux-system`)
so that K8s-native `ownerReferences` can cascade GC owned resources when the
Process is deleted. Flux 2 multi-tenancy supports this.

## First-class signals

Delivered via annotation:

```sh
kubectl annotate process/observability-stack tatara.pleme.io/signal=SIGHUP --overwrite
```

| Signal | Effect |
|--------|--------|
| `SIGHUP` | Reconverge — re-enter Execing without termination |
| `SIGTERM` | Graceful terminate; children drain first |
| `SIGKILL` | Force terminate (`grace_period_seconds: 0` on owned resources) |
| `SIGUSR1` | Force re-attestation without spec change |
| `SIGUSR2` | Invoke remediation hooks |
| `SIGSTOP` | Pause reconciliation |
| `SIGCONT` | Resume reconciliation |

The reconciler's top-level loop reads the annotation (one-shot), strips it,
parses via `ProcessSignal::FromStr`, and dispatches through
`tatara_reconciler::signals::apply()` which maps (phase × signal × strategy) to
a `SignalEffect`.

## Three-pillar BLAKE3 attestation

Every convergence cycle writes a `ProcessAttestation` to `status.attestation`:

```
composed_root = BLAKE3(
    "tatara-process/v1alpha1\n"
    ++ artifact_hash     // rendered resources + applied status
    ++ control_hash?     // compliance verification proof (None for now)
    ++ intent_hash       // canonical spec + nix store path + lisp AST
    ++ previous_root?    // chain to prior attestation
)
```

`previous_root` chains each generation (starts at 0, increments on each
Reconverging cycle). Downstream tools:
- **sekiban** — K8s admission webhook verifies `composed_root` at admit time
- **kensa** — compliance engine consumes `control_hash` + metadata
- **inshou** — Nix gate CLI verifies `intent_hash` pre-rebuild

Every emitted Kustomization / HelmRelease carries annotations that expose the
chain:
- `tatara.pleme.io/process: <ns>/<name>`
- `tatara.pleme.io/pid: <hierarchical path>`
- `tatara.pleme.io/content-hash: <26-char base32>`
- `tatara.pleme.io/generation: <u64>`
- `tatara.pleme.io/attestation-root: <hex BLAKE3>`

## Finalizer-guarded lifecycle

Every Process installs `tatara.pleme.io/process-finalizer` on first reconcile
(in Pending). `kubectl delete proc/x`:

1. K8s sets `deletionTimestamp` — reconciler sees it, forces phase to Exiting
2. Exiting cascades: enumerates children by `parent_pid`, deletes each
3. Once children gone, → Zombie → Reaped on next tick
4. Reaped releases the finalizer → K8s GC cascade-deletes owned Flux CRs

This ensures:
- No race with Flux GC (owner refs + finalizer make sequencing deterministic)
- Children always die before parents (Unix-consistent)
- Deletion is observable from the outside (phase transitions are visible in
  `kubectl get proc -w`)

## `ProcessTable` — the `/proc` singleton

Cluster-scoped CRD. Aggregates every `Process` status across all namespaces.
Hands out hierarchical PIDs (`seph.1.7.3`). Stores policy: `max_depth`,
`max_children`, `sigterm_timeout_seconds`, `zombie_timeout_seconds`,
`orphan_reaping_enabled`.

Shown in `kubectl get pt` (short name). Acts as the global scheduler state —
cluster matrix scheduling reads `status.processes[].qualities` and matches
against `WorkloadRequirements` via `tatara-lattice::qualities_match`.

## Related crates

| Crate | Responsibility |
|---|---|
| `tatara-process` | `Process` + `ProcessTable` CRDs, phase enum, signal enum, attestation type, derive on ProcessSpec |
| `tatara-reconciler` | The controller — 8-phase state machine, FluxCD emission, boundary evaluators |
| `tatara-lattice` | `meet`/`join`/`leq` over Classification; compliance baseline ordering; `qualities_match` |
| `tatara-lisp` | Reader + macroexpander + `TataraDomain` trait + derive integration |
