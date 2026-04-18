# tatara-process

`Process` + `ProcessTable` — the K8s-as-Unix-processes wire format for tatara.

```
GROUP:    tatara.pleme.io
VERSION:  v1alpha1
KINDS:    Process (namespaced)  |  ProcessTable (cluster-scoped)
```

## What's a Process?

Everything a cluster converges toward is a `Process`:

- A **cluster** is `Process { pointType: Fork, substrate: Compute }`
- A **HelmRelease** is `Process { pointType: Gate, substrate: Observability }`
- A **DB migration** is `Process { pointType: Transform, substrate: Storage }`
- A **Nix build** is `Process { pointType: Transform, substrate: Compute, intent: Nix { delegateToNixBuild: true } }`

Every Process has a hierarchical PID in the `ProcessTable` (cluster-scoped
`/proc`), a BLAKE3 content-addressable identity, and transitions through Unix
phases:

```
Pending → Forking → Execing → Running → Attested
                                      ↘ Failed
Attested → Reconverging → Execing               (SIGHUP path)
Running  → Exiting      → Zombie → Reaped       (SIGTERM path)
```

## Minimal spec

```yaml
apiVersion: tatara.pleme.io/v1alpha1
kind: Process
metadata:
  name: observability-stack
  namespace: seph
spec:
  identity:
    parent: seph.1
  classification:
    pointType: Gate
    substrate: Observability
    horizon:
      kind: Bounded
    calm: Monotone
    dataClassification: Internal
  intent:
    nix:
      flakeRef: "github:pleme-io/k8s?dir=shared/infrastructure"
      attribute: "observability"
  boundary:
    postconditions:
      - kind: KustomizationHealthy
        params:
          name: "observability-stack"
      - kind: PromQL
        params:
          query: "up{job='prometheus'} == 1"
  compliance:
    baseline: fedramp-moderate
    bindings:
      - { framework: nist-800-53, controlId: SC-7, phase: AtBoundary }
  dependsOn:
    - { name: akeyless-injection, mustReach: Attested }
```

## Printer columns (`kubectl get proc`)

| Column      | Source                                            |
|-------------|---------------------------------------------------|
| `PID`       | `status.pid` — hierarchical path (`seph.1.7`)     |
| `Phase`     | `status.phase`                                    |
| `Type`      | `spec.classification.pointType`                   |
| `Substrate` | `spec.classification.substrate`                   |
| `Gen`       | `status.attestation.generation`                   |
| `Age`       | `metadata.creationTimestamp`                      |

## Signals

Deliver by annotation:

```sh
kubectl annotate process/observability-stack tatara.pleme.io/signal=SIGHUP --overwrite
```

| Signal      | Effect                                                        |
|-------------|---------------------------------------------------------------|
| `SIGHUP`    | Reconverge — re-enter `Execing` without termination           |
| `SIGTERM`   | Graceful terminate (children drain first)                     |
| `SIGKILL`   | Force terminate (`grace_period_seconds: 0` on owned resources)|
| `SIGUSR1`   | Force re-attestation without spec change                      |
| `SIGUSR2`   | Invoke remediation hooks                                      |
| `SIGSTOP`   | Pause reconciliation                                          |
| `SIGCONT`   | Resume reconciliation                                         |

## Rendering surfaces

The same `ProcessSpec` authors in four languages:

- **YAML** — what `kubectl` sees (above)
- **Rust** — `ProcessSpec { … }` built programmatically
- **Nix** — `services.tatara.processes.<name> = { … }` in a HM/NixOS module
- **S-expr** — `(defpoint <name> …)` via `tatara-lisp` (homoiconic + macroexpandable)

See `examples/process/` in the repo root.

## Lisp compilation

`ProcessSpec` derives `TataraDomain` (from `tatara-lisp-derive`) with
`keyword = "defpoint"`. Every field — nested structs (IdentitySpec,
Classification, Intent, Boundary, ComplianceSpec, SignalPolicy), nested enums
(ConvergencePointType, SubstrateType, VerificationPhase, MustReachPhase,
SighupStrategy), `Vec<DependsOn>` — is handled by the derive via serde's
Deserialize fallthrough. Zero hand-rolled parsing.

```sh
tatara-lispc examples/process/observability-stack.lisp | kubectl apply -f -
```

Or programmatically:

```rust
use tatara_process::compile_source;

let defs = compile_source(r#"
    (defpoint observability-stack
      :classification (:point-type Gate :substrate Observability)
      :intent (:nix (:flake-ref "github:pleme-io/k8s" :attribute "observability")))
"#)?;
// defs: Vec<NamedDefinition<ProcessSpec>>
```

The `tatara-lispc` binary (`cargo run --bin tatara-lispc`) takes a `.lisp` file
and prints one or more `Process` YAML blocks to stdout. See the integration
test at `src/lib.rs::compile_tests::full_processspec_round_trip_via_derive`.
