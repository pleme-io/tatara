# tatara-operator — DEPRECATED

The `NixBuild`, `FlakeSource`, `FlakeOrg`, and `NixBuildPool` CRDs are replaced
by a single **`Process`** (from `tatara-process`).

## Why

`Process` is the unified K8s-as-Unix-processes surface; a Nix build is just a
`Process { pointType: Transform, substrate: Compute }` with `intent.nix`.
Maintaining a second operator adds no information — the same NATS BUILD stream
is still used on the backend; only the CRD facade changes.

## Migration

```yaml
# Before — tatara-operator/v1alpha1
apiVersion: tatara.pleme.io/v1alpha1
kind: NixBuild
metadata:
  name: akeyless-auth
spec:
  flakeRef: "github:pleme-io/blackmatter-akeyless#akeyless-backend-auth"
  system: x86_64-linux
  atticCache: main
```

```yaml
# After — tatara-process/v1alpha1
apiVersion: tatara.pleme.io/v1alpha1
kind: Process
metadata:
  name: akeyless-auth
  namespace: builds
spec:
  classification:
    pointType: Transform
    substrate: Compute
    horizon:
      kind: Bounded
  intent:
    nix:
      flakeRef: "github:pleme-io/blackmatter-akeyless#akeyless-backend-auth"
      system: x86_64-linux
      atticCache: main
      delegateToNixBuild: true    # hand off to bare-metal builder via NATS
```

`delegateToNixBuild: true` tells `tatara-reconciler` to enqueue the build on the
same NATS `BUILD.>` stream `tatara-operator` consumed. Once removed, the
reconciler will drive the NATS client directly.

## Scheduled for removal

After `tatara-reconciler` owns the NATS BUILD bridge directly. No production
consumers exist.
