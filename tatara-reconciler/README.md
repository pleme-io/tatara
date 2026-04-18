# tatara-reconciler

The FluxCD-adjacent Kubernetes controller. Reconciles `Process` through the
8-phase Unix lifecycle, composes three-pillar BLAKE3 attestation, and emits
FluxCD `Kustomization` / `HelmRelease` CRs for source-controller +
kustomize-controller + helm-controller to apply.

## What it does

```
Process.spec  ─┐
               ├──▶  8-phase state machine  ─┬─▶  Kustomization    ─┐
ProcessTable ──┘                             ├─▶  HelmRelease      ├──▶  FluxCD controllers apply
                                             └─▶  (other K8s CRs)   ┘              │
                                                                                   ▼
                                         ┌───────────────────────────────  status ready
                                         │
ProcessStatus  ◀─── VERIFY (poll Flux) ──┘
       │
       └───▶  ATTEST  ─▶  BLAKE3(artifact || control || intent || prev)  ─▶  status.attestation
```

## What it does NOT do

- **Does not** replace FluxCD. It emits Flux CRs and watches their status.
- **Does not** ssh, kubectl, or helm install. All control is via CRD.
- **Does not** bypass `encrypted_regex: "^(data|stringData)$"` SOPS metadata
  rules used in `blackmatter-kubernetes` / `k8s`.

## Install

Build:

```sh
cargo build --release -p tatara-reconciler
```

Generate CRDs:

```sh
cargo run --release --bin tatara-crd-gen > deploy/crds.yaml
```

Run (in-cluster):

```sh
tatara-reconciler \
  --watch-namespace "" \
  --controller-namespace tatara-system \
  --heartbeat-seconds 30
```

Env var equivalents: `TATARA_WATCH_NAMESPACE`, `TATARA_CONTROLLER_NAMESPACE`,
`TATARA_HEARTBEAT_SECONDS`, `TATARA_HEALTH_ADDR`.

## Phase dispatch

The top-level reconcile function is a phase dispatcher. Every phase has a
handler in `src/phase_machine.rs`:

| Phase         | Handler                    | Loop step                                  |
|---------------|----------------------------|--------------------------------------------|
| `Pending`     | `handle_pending`           | DECLARE — canonicalize spec, hash content  |
| `Forking`     | `handle_forking`           | PID assign; check `dependsOn`              |
| `Execing`     | `handle_execing`           | SIMULATE + PROVE + RENDER                  |
| `Running`     | `handle_running`           | DEPLOY + VERIFY preconditions              |
| `Attested`    | `handle_attested`          | VERIFY postconditions + ATTEST (heartbeat) |
| `Reconverging`| `handle_reconverging`      | RECONVERGE — re-enter `Execing`            |
| `Exiting`     | `handle_exiting`           | Cascade SIGTERM, drain children            |
| `Failed`      | `handle_failed`            | Record exit, transition to `Zombie`        |
| `Zombie`      | `handle_zombie`            | Wait for reap; force-reap on timeout       |
| `Reaped`      | `handle_reaped`            | Finalizer released                         |

## Annotations it writes

Every emitted FluxCD resource is annotated:

```
tatara.pleme.io/managed-by: tatara-reconciler
tatara.pleme.io/process: <namespace>/<name>
tatara.pleme.io/pid: <hierarchical pid path>
tatara.pleme.io/content-hash: <26-char base32 BLAKE3>
tatara.pleme.io/generation: <u64>
tatara.pleme.io/attestation-root: <hex BLAKE3>
```

These make it possible for `sekiban` (admission webhook) to verify integrity
at admit time without looking at the `Process` CRD itself.

## Health

`GET /healthz` → `200 ok` (controller bound).
`GET /readyz`  → `200 ok` (once kube client is connected).
