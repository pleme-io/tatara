# tatara-operator

Kubernetes operator that bridges NixBuild CRDs to the tatara bare-metal build cluster via NATS JetStream.

## Architecture

```
kubectl apply NixBuild CR
    ↓
tatara-operator (this)
    ↓ publishes BUILD.request to NATS
NATS JetStream
    ↓ tatara agents consume
tatara bare-metal nodes (nix build)
    ↓ publishes BUILD.complete
tatara-operator (subscribes)
    ↓ updates NixBuild CR status
kubectl get nixbuilds → Complete
```

## CRDs

### NixBuild

```yaml
apiVersion: tatara.pleme.io/v1alpha1
kind: NixBuild
metadata:
  name: my-package
spec:
  flakeRef: "github:pleme-io/repo#package"
  system: x86_64-linux
  atticCache: main
  extraArgs: ["--impure"]
  priority: 0
status:
  phase: Complete    # Pending → Queued → Building → Pushing → Complete / Failed
  storePath: /nix/store/...
  buildId: "uuid"
```

### NixBuildPool

```yaml
apiVersion: tatara.pleme.io/v1alpha1
kind: NixBuildPool
metadata:
  name: default
spec:
  minNodes: 0
  maxNodes: 4
  systems: [x86_64-linux]
  idleTimeoutSecs: 300
```

## Build

```bash
cargo check
cargo build --release
```

## Dependencies

- kube-rs 0.99 (K8s client + runtime)
- async-nats 0.38 (NATS JetStream)
- axum (not used directly — API is via K8s CRDs)
