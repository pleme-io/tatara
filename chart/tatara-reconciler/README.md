# tatara-reconciler Helm chart

Deploys the [`tatara-reconciler`](../../tatara-reconciler) controller: the
FluxCD-adjacent Kubernetes reconciler that runs `Process` CRDs as Unix processes
in the tatara convergence lattice.

## Install

```sh
kubectl create namespace tatara-system
helm install tatara-reconciler \
  oci://ghcr.io/pleme-io/charts/tatara-reconciler \
  --namespace tatara-system \
  --version 0.2.0
```

Or via local checkout:

```sh
helm install tatara-reconciler ./chart/tatara-reconciler \
  --namespace tatara-system --create-namespace
```

The chart installs two CRDs from `./crds` on first install:
- `processes.tatara.pleme.io` (namespaced — shortname `proc`)
- `processtables.tatara.pleme.io` (cluster-scoped — shortname `pt`)

Helm does **not** update CRDs on upgrade (intentional limitation). To refresh
them after bumping the chart:

```sh
kubectl apply -f chart/tatara-reconciler/crds/
```

## Install via FluxCD

```yaml
apiVersion: source.toolkit.fluxcd.io/v1
kind: GitRepository
metadata:
  name: pleme-tatara
  namespace: flux-system
spec:
  interval: 5m
  url: https://github.com/pleme-io/tatara
  ref:
    tag: v0.2.0
---
apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: tatara-reconciler
  namespace: tatara-system
spec:
  interval: 5m
  chart:
    spec:
      chart: ./chart/tatara-reconciler
      sourceRef:
        kind: GitRepository
        name: pleme-tatara
        namespace: flux-system
  install:
    crds: Create
  upgrade:
    crds: CreateReplace
  values:
    watchNamespace: ""
    logLevel: info
```

## Configuration surface

| Key                          | Default                                | Purpose                                                       |
|------------------------------|----------------------------------------|---------------------------------------------------------------|
| `image.repository`           | `ghcr.io/pleme-io/tatara-reconciler`   | Container image                                                |
| `image.tag`                  | `.Chart.AppVersion`                    | Image tag                                                      |
| `watchNamespace`             | `""`                                   | Empty = all namespaces                                         |
| `controllerNamespace`        | `tatara-system`                        | Where the ProcessTable singleton lookups happen                |
| `processTableName`           | `proc`                                 | Name of the cluster-scoped ProcessTable singleton              |
| `heartbeatSeconds`           | `30`                                   | Attested-phase heartbeat interval                              |
| `logLevel`                   | `info`                                 | `RUST_LOG` value                                               |
| `replicaCount`               | `1`                                    | Keep at 1 until leader election lands                          |
| `resources.requests.cpu`     | `100m`                                 |                                                                |
| `resources.requests.memory`  | `128Mi`                                |                                                                |
| `resources.limits.cpu`       | `1000m`                                |                                                                |
| `resources.limits.memory`    | `512Mi`                                |                                                                |
| `rbac.create`                | `true`                                 | Create ClusterRole + ClusterRoleBinding                        |
| `crds.install`               | `true`                                 | Install CRDs from `./crds` (first-install only; see above)     |
| `serviceAccount.create`      | `true`                                 |                                                                |

## Permissions granted

The ClusterRole grants:
- Full R/W on `processes` + `processtables` (+ `/status` + `/finalizers`)
- Full R/W on `kustomizations` and `helmreleases` (tatara emits these)
- Read-only on Flux source kinds (`gitrepositories`, `helmrepositories`, `ocirepositories`, `buckets`) — to validate `intent.flux.gitRepository` references
- `create`/`patch` on `events` — audit trail
- Read-only on `configmaps` and `namespaces`
- R/W on `coordination.k8s.io/leases` — leader election slot for future multi-replica mode

## Verifying a live install

```sh
# CRDs present
kubectl get crd processes.tatara.pleme.io processtables.tatara.pleme.io

# Controller running
kubectl -n tatara-system get pods -l app.kubernetes.io/name=tatara-reconciler

# /proc singleton auto-created on first Process
kubectl get processtable

# Apply an example Process
kubectl apply -f ../examples/process/observability-stack.yaml
kubectl get proc
```

## Regenerating CRDs

CRDs are pre-generated from the Rust types via `tatara-crd-gen`. When the
`ProcessSpec` / `ProcessTableSpec` schema changes, regenerate:

```sh
./chart/tatara-reconciler/scripts/regenerate-crds.sh
```

## Companion

- Controller binary:   `../../tatara-reconciler`
- Types crate:         `../../tatara-process`
- Lisp surface:        `../../tatara-lisp` (use `tatara-lispc foo.lisp | kubectl apply -f -`)
- CRD generator:       `../../tatara-reconciler/src/bin/crd_gen.rs`
