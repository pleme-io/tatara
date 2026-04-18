# tatara-kube — DEPRECATED

This crate is replaced by **`tatara-reconciler`**.

## Why

`tatara-kube` bypassed FluxCD by doing Server-Side Apply directly from `nix eval`
output. That worked but produced duplicated work with the existing FluxCD
pipeline used throughout `blackmatter-kubernetes` and the `k8s` GitOps repo.
The new `tatara-reconciler` is *FluxCD-adjacent* — it emits Kustomization /
HelmRelease CRs and lets source-controller + kustomize-controller + helm-controller
do the actual apply, while layering:

1. Unix process semantics over every reconciled object (`tatara-process`)
2. Three-pillar BLAKE3 attestation (`tatara-core::ConvergenceAttestation`)
3. Lattice-ordered compliance / classification gating (`tatara-lattice`)
4. Multi-surface intent rendering: Nix, Lisp, Flux-passthrough, Container

## Migration

A prior `tatara-kube` reconciliation target:

```nix
# imperative tatara-kube
services.tatara.workloads.observability.driver = "kube";
services.tatara.workloads.observability.flakeRef = "github:pleme-io/k8s";
```

becomes a `Process` CRD:

```yaml
apiVersion: tatara.pleme.io/v1alpha1
kind: Process
metadata:
  name: observability-stack
  namespace: seph
spec:
  classification:
    pointType: Gate
    substrate: Observability
  intent:
    nix:
      flakeRef: "github:pleme-io/k8s"
      attribute: "observability"
  compliance:
    baseline: fedramp-moderate
```

The `intent.nix` variant delegates to `tatara-engine`'s `nix_eval` driver (or
to a `NixBuild` sibling CRD via `delegate_to_nix_build: true`).

## Scheduled for removal

After the reconciler's Process CRD lands in production clusters this crate will
be removed from the workspace. No consumers exist yet outside this repo.
