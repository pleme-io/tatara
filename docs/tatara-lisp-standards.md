# tatara-lisp ecosystem standards

> Ruthlessly enforced. Every crate, caixa, plugin, service, operator, and
> module follows this. Read `rust-lisp.md` first for the philosophy; this
> document is the mechanics.

---

## 1. Naming

### 1.1 Language primitives

- **Japanese** for base-layer concepts that already exist
  (`tatara`, `sui`, `forge`, `substrate`, `garasu`, `irodori`, `madori`,
  `egaku`, `shikumi`, `hayai`, `mojiban`, `awase`).
- **Brazilian-Portuguese** for new primitives introduced by this ecosystem
  (`terreiro`, `forja`, `cerrado`, `cordel`, `jabuti`, `samba`, `caixa`,
  `feira`, `lacre`, `selo`, `teia`, `provedor`, `oficina`, `escriba`).
- No English portmanteaux. No gerunds. No acronyms for domain primitives.
- Technical jargon stays English when it's universally understood
  (`ast`, `fmt`, `lsp`, `ts`, `api`, `spec`, `vm`).

### 1.2 Identifiers

| Surface              | Case           | Example                       |
|----------------------|----------------|-------------------------------|
| Rust type            | PascalCase     | `ProcessSpec`, `TeiaManifest` |
| Rust field           | snake_case     | `window_seconds`              |
| Lisp keyword arg     | kebab-case     | `:window-seconds`             |
| Lisp enum variant    | PascalCase sym | `Biblioteca`, `Critical`      |
| Lisp keyword head    | kebab-case     | `defprocess`, `defmajor-mode` |
| Crate / caixa name   | kebab-case     | `tatara-lisp`, `caixa-core`   |
| Git repo name        | kebab-case     | `pleme-io/caixa`              |
| Filename             | kebab-case     | `observability-stack.lisp`    |
| OpenAPI schema       | PascalCase     | `CaixaSpec`                   |

The sexp→JSON bridge in `tatara_lisp::domain::sexp_to_json` converts
kebab→camelCase, so every `#[serde(rename_all = "camelCase")]` struct
round-trips cleanly. **Do not fight the bridge**; place the attribute and
move on.

### 1.3 Reserved Lisp head keywords

The following head symbols are reserved for TataraDomain-registered types.
Crates that invent new keywords must begin with `def`, collide with none of
these, and register via `tatara_lisp::domain::register::<T>()`:

| Keyword              | Type                  | Crate            |
|----------------------|-----------------------|------------------|
| `defpoint`           | `ProcessSpec`         | tatara-process   |
| `defmonitor`         | `MonitorSpec`         | tatara-domains   |
| `defnotify`          | `NotifySpec`          | tatara-domains   |
| `defalertpolicy`     | `AlertPolicySpec`     | tatara-domains   |
| `defcaixa`           | `Caixa`               | caixa-core       |
| `deflacre`           | `Lacre`               | caixa-lacre      |
| `defflake`           | `FlakeLisp`           | caixa-flake      |
| `defteia`            | `TeiaInstance`        | caixa-teia       |
| `defteia-schema`     | (emitted by forge)    | caixa-teia-forge |
| `defarquitetura`     | (planned)             | caixa-teia       |
| `defresolver-config` | `ResolverConfigLisp`  | caixa-resolver   |
| `deffmt-config`      | `FmtConfigLisp`       | caixa-fmt        |
| `deflint-config`     | `LintConfigLisp`      | caixa-lint       |
| `defregra`           | `CustomRule`          | caixa-lint       |
| `defescriba`         | `EscribaConfig`       | escriba-config   |
| `defkeymap`          | `KeymapDecl`          | escriba-config   |
| `defcommand`         | `CommandDecl`         | escriba-config   |
| `defplugin`          | `PluginDecl`          | escriba-config   |
| `defmajor-mode`      | `MajorMode`           | escriba-config   |
| `defminor-mode`      | `MinorMode`           | escriba-config   |
| `defcheck`           | (macro, not type)     | tatara-reconciler|

Add new entries here when a crate registers a new keyword. Collisions are
caught by the single global registry.

---

## 2. Rust — every crate

### 2.1 Cargo.toml

```toml
[package]
name         = "caixa-core"
description  = "one-line purpose"
version      .workspace = true
edition      .workspace = true
rust-version .workspace = true
license      .workspace = true
repository   .workspace = true
authors      .workspace = true

[dependencies]
# ordered: workspace-internal → shared-pleme-io → third-party alphabetical
# workspace = { workspace = true }  # all version pinning in the root

[dev-dependencies]
# tests use pretty_assertions / proptest / tempfile as needed

[lints]
workspace = true
```

### 2.2 Workspace Cargo.toml

```toml
[workspace.package]
version       = "0.1.0"
edition       = "2024"
rust-version  = "1.89.0"
license       = "MIT"
repository    = "https://github.com/pleme-io/<repo>"
authors       = ["pleme-io"]

[workspace.lints.clippy]
pedantic              = { level = "warn", priority = -1 }
module_name_repetitions = "allow"
missing_errors_doc    = "allow"
missing_panics_doc    = "allow"

[profile.release]
codegen-units = 1
lto           = true
opt-level     = "z"
strip         = true
```

### 2.3 Every public type

Requirements, in priority order:

1. `#[derive(serde::Serialize, serde::Deserialize)]` — non-negotiable for
   types crossing crate boundaries.
2. `#[derive(schemars::JsonSchema)]` — so escriba-api / forge-gen can emit
   OpenAPI / JSON Schema for it.
3. `#[derive(Debug, Clone)]` — standard for public domain types.
4. `#[derive(PartialEq, Eq)]` when sensible — makes tests a one-liner.
5. `#[derive(tatara_lisp::DeriveTataraDomain)]` when the type should be
   authorable from Lisp. Six lines of ceremony buys Lisp authoring + the
   global registry.
6. `#[must_use]` on every constructor that returns `Self`.
7. Every field `pub` — if it shouldn't be public, factor it into a private
   helper type.

### 2.4 Errors

- Every crate has **one** top-level `Error` enum per module, derived via
  `thiserror::Error`.
- Error variants are typed (no stringly-typed `anyhow::Error` crossing
  crate boundaries).
- Binaries wrap with `anyhow` at the main() boundary only.

### 2.5 Tests

- Unit tests inline via `#[cfg(test)] mod tests { use super::*; … }`.
- Integration tests under `tests/`. Real-world subprocess work (git, tofu)
  uses `tempfile` + a locally-constructed fixture.
- `proptest` for any invariant provable generatively — formatters, parsers,
  hashers, resolvers. One `proptest` block per invariant, named after the
  invariant.
- Minimum test coverage: every public fn has at least one test. Fallible
  fns have a success test **and** at least one failure test.
- CI gates: `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace --all-features`,
  `nix flake check`.

### 2.6 Lints

- `clippy::pedantic` as `warn` at the workspace level.
- Ergonomic allows baked into the workspace (module_name_repetitions,
  missing_errors_doc, missing_panics_doc). No per-crate overrides without
  comment explaining why.
- No `#[allow(…)]` on individual items without a justification comment.
- `unsafe` is forbidden unless gated behind a feature flag with a written
  safety argument. `#![forbid(unsafe_code)]` at the top of every crate
  that doesn't need it (almost all of them).

### 2.7 Release profile

`codegen-units=1`, `lto=true`, `opt-level="z"`, `strip=true`. Verified by
every release build — a feira binary should be ≤ 1 MB, a caixa-lsp ≤ 1.5
MB, an escriba ≤ 3 MB.

### 2.8 path deps vs git deps

- Within a workspace: `{ path = "…", version = "0.1.0" }`.
- Cross-workspace, same-user (pleme-io sibling directories):
  `{ path = "../<sibling>" }`. Matches the `sui-eval` + `caixa` convention.
- Cross-workspace, external consumers: `{ git = "https://github.com/…" }`
  only if the crate is **not** on crates.io. Published crates use
  crates.io. No mixed strategies within a single Cargo.toml.

### 2.9 `.cargo/config.toml`

```toml
[net]
git-fetch-with-cli = true
```

Matches SSH-gated pleme-io mirrors. Required when any dep resolves over
`git@github.com:pleme-io/…`.

---

## 3. tatara-lisp — every domain

### 3.1 Shape

```rust
#[derive(DeriveTataraDomain, Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defmydomain")]
pub struct MyDomainSpec {
    pub nome: String,                      // always required first
    pub versao: String,                    // semver string
    pub kind: MyKind,                      // enum, authored as bare PascalCase symbol
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descricao: Option<String>,         // optional, skipped when None
    #[serde(default)]
    pub deps: Vec<Dep>,                    // Vec<Nested> — sexp→JSON fallthrough
}

impl MyDomainSpec {
    pub fn register() { tatara_lisp::domain::register::<Self>(); }
    pub fn from_lisp(src: &str) -> Result<Self, tatara_lisp::LispError> { … }
}
```

### 3.2 Rules

1. **Every domain registers itself**. Every crate that defines TataraDomains
   ships a `register_all()` that binaries call once at startup.
2. **Optional fields skip serialization** when `None` so round-trips don't
   emit `Nil` values that confuse `extract_optional_string`.
3. **`#[serde(default)]`** on every `Option` and every `Vec` to keep YAML /
   JSON loading forgiving.
4. **Enum variants as bare PascalCase symbols** in Lisp
   (`:kind Biblioteca`, not `:kind :biblioteca` or `:kind "Biblioteca"`).
5. **Kebab-case keywords** in Lisp (`:window-seconds`, not `:windowSeconds`).
   The sexp→JSON bridge does the conversion.
6. **Every domain has a `from_lisp(src)` method** — same shape across the
   ecosystem so tools (LSP, linter, tatara-check) dispatch identically.

### 3.3 Tests per domain

```rust
#[test] fn parses_happy_path() { … }
#[test] fn errors_on_missing_required() { … }
#[test] fn errors_on_wrong_head_symbol() { … }
#[test] fn register_populates_registry() { … }
```

Optional tests: `round_trips_through_lisp`, `to_lisp_then_parse_preserves_equality`.

### 3.4 Lisp style (what caixa-lint enforces)

- **kebab-case keywords**: `:my-field`, not `:myField`.
- **PascalCase enum variants** as bare symbols: `:kind Biblioteca`.
- **Paired kwargs**: every `:key` has a value right after it.
- **Descriptive `:descricao`**: no `FIXME` placeholders in committed code.
- **Git deps pinned**: `:tag "v1"` or `:rev "abc…"`, never `:branch "main"`.
- **Small forms**: ≤ 60 source lines per `(defX …)`.
- **Single quote style**: never mix `'x` reader quote with `(quote x)`.
- **Explicit `:kind`**: every caixa's `:kind` is set, no defaults.

---

## 4. OpenAPI — every public API

### 4.1 Source of truth

Every public API is emitted from Rust types annotated with `JsonSchema`
(and `TataraDomain` where applicable). The spec is **generated**, never
hand-written:

```rust
// escriba-api / caixa-api / <x>-api / a *-api crate in every workspace
let spec = schemars::schema_for!(MyType);
```

### 4.2 Spec file

- Format: OpenAPI 3.1 (`openapi: "3.1.0"`).
- Committed in-tree under `<repo>-spec/` or `docs/openapi.{json,yaml}`.
- Regenerated by a `*-spec-dump` binary or a `cargo run` command.
- CI gate: regen, diff against committed; fail on drift.

### 4.3 Downstream artifacts

The spec drives (via forge-gen):

1. **SDKs** — Rust / TypeScript / Python / Go / Ruby.
2. **MCP server** — autogen for LLM-driven control.
3. **Documentation site** — autogen docs/reference.
4. **Shell completions** — skim-tab + fish via completion-forge.
5. **Typed clients** — for other services in the ecosystem.

Nothing downstream is hand-written. The generators are the source of
truth, fed by the spec, fed by the Rust types.

---

## 5. Nix — every repo

### 5.1 Structure

```
<repo>/
├── flake.nix                # rustPlatform.buildRustPackage — NO crate2nix
├── Cargo.toml               # workspace root
├── Cargo.lock               # committed
├── LICENSE                  # MIT
├── .cargo/config.toml       # net.git-fetch-with-cli = true
├── .gitignore               # /target /result /.direnv
├── .github/workflows/ci.yml # fmt + clippy + test + nix-flake-check
├── deploy.yaml              # forge-native CI/CD spec
└── <crate-dirs>/…
```

### 5.2 flake.nix shape

```nix
{
  description = "<crate> — one-line purpose";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, substrate, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        src  = pkgs.lib.cleanSourceWith { src = ./.; filter = …; };
        mkBin = { pname, package }: pkgs.rustPlatform.buildRustPackage {
          inherit pname src;
          version = "0.1.0";
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" package ];
          …
        };
      in {
        packages = { default = mkBin { … }; … };
        devShells.default = pkgs.mkShell { … };
        checks = { cargo-fmt = …; workspace-tests = …; };
      })
    // {
      homeManagerModules.default = import ./<crate>/module { };
      nixosModules.default       = { config, lib, pkgs, … }: { … };
    };
}
```

### 5.3 No crate2nix

The whole workspace goes through `rustPlatform.buildRustPackage` with
`cargoLock.lockFile = ./Cargo.lock`. **No crate2nix compat layer.** When
git deps appear, declare `outputHashes` explicitly. The lisp-native `caixa`
ecosystem should integrate with Nix so tightly that crate2nix becomes
unnecessary — this rule enforces it.

### 5.4 Home-Manager modules

Every user-facing binary ships a `module/default.nix` under its crate,
wired through `homeManagerModules.default`. Options follow the pattern:

```nix
programs.<name> = {
  enable       = lib.mkEnableOption "<name> CLI";
  enableLsp    = lib.mkOption { type = bool; default = true; };
  theme        = lib.mkOption { type = enum ["dark" "light"]; default = "dark"; };
  defaultHost  = lib.mkOption { type = str;  default = "github:pleme-io"; };
};
```

XDG config emission via `lib.generators.toYAML` (or `lib.generators.toLisp`
once tatara-lisp ships one). Never hardcode paths; always go through
`config.home.homeDirectory` + `xdg.configFile`.

### 5.5 NixOS modules (services)

Every service ships a `nixosModules.default` alongside its
`homeManagerModules.default`. Services declare systemd units with:

- `DynamicUser = true` by default
- `NoNewPrivileges = true`
- `ProtectSystem = "strict"`
- `ProtectHome = true`
- `Restart = "on-failure"` + `RestartSec = "5s"`

Mirror Kubernetes hardening — same defaults, different runtime.

### 5.6 `nix flake update` cadence

- Weekly at minimum for consumer workspaces (nix repo).
- Before every release for library workspaces (caixa, escriba, tatara).
- Run `nix flake check` after every update; failing? revert + file issue.

---

## 6. Kubernetes — every stateful thing

### 6.1 CRD per concept

Every mutable, observable, long-lived concept gets a CRD under the
`<domain>.pleme.io/v1alpha1` API group:

```rust
#[derive(CustomResource, Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[kube(
    group       = "caixa.pleme.io",
    version     = "v1alpha1",
    kind        = "Caixa",
    plural      = "caixas",
    singular    = "caixa",
    shortname   = "cxa",
    namespaced,
    status      = "CaixaStatus",
    printcolumn = r#"{"name":"Versao","type":"string","jsonPath":".spec.versao"}"#,
)]
#[serde(rename_all = "camelCase")]
pub struct CaixaSpec { … }
```

Required: `plural`, `singular`, `shortname`, `namespaced` or `clustered`,
`status`, one `printcolumn` per field a human would want in `kubectl get`
output. Include age and health as the last two columns always.

### 6.2 Operator pattern

Every CRD has a kube-rs operator at `<crate>-operator` following the
caixa-operator shape:

1. Watch the CR via `kube::runtime::Controller`.
2. Reconcile via `async fn reconcile(cr: Arc<Cr>, ctx: Arc<Ctx>)
   -> Result<Action, Error>`.
3. Patch status via `api.patch_status(&name, &PatchParams::apply("name"),
   &Patch::Apply(&patch))`.
4. Emit K8s events + conditions per ReconcilePolicy's interval.
5. Binary logs via `tracing` with `--log={text,json}` flag.
6. Security: blackmatter-hardened (runAsNonRoot, readOnlyRootFS, drop-ALL
   caps, RuntimeDefault seccomp, NetworkPolicy egress-gated to API server +
   DNS + required hosts).

### 6.3 CRD YAML generation

Every `<crate>-crd` crate ships a `dump-crds` binary:

```bash
cargo run -p <crate>-crd --bin dump-crds > <crate>-helm/templates/crds/<group>_crds.yaml
```

CI regenerates and fails on drift. Never hand-edit the committed YAML.

---

## 7. Helm + FluxCD + Kustomization — every deployable

### 7.1 Helm chart shape

```
<crate>-helm/
├── Chart.yaml               # type: application, kubeVersion ">=1.28.0-0"
├── values.yaml              # all knobs, all documented
└── templates/
    ├── _helpers.tpl         # labels, selectorLabels, fullname, serviceAccountName
    ├── crds/*.yaml          # generated from dump-crds
    ├── serviceaccount.yaml
    ├── rbac.yaml            # ClusterRole + ClusterRoleBinding, scoped to own group
    ├── deployment.yaml      # with blackmatter-hardened podSecurityContext
    ├── networkpolicy.yaml   # egress-gated, with ingress allow-list
    └── servicemonitor.yaml  # gated on .Values.serviceMonitor.enabled
```

### 7.2 Blackmatter security baseline

In every `values.yaml`:

```yaml
podSecurityContext:
  runAsNonRoot: true
  runAsUser: 65532
  runAsGroup: 65532
  fsGroup: 65532
  seccompProfile: { type: RuntimeDefault }
securityContext:
  allowPrivilegeEscalation: false
  readOnlyRootFilesystem: true
  runAsNonRoot: true
  capabilities: { drop: ["ALL"] }
```

These are not overridable per-cluster. Only per-cluster overlays (size,
replicas, metrics-enabled) go in Kustomize patches.

### 7.3 FluxCD manifests

Every deployable ships a `<crate>-flux/` directory:

```
<crate>-flux/
├── namespace.yaml                    # namespace + PSA labels
├── gitrepository.yaml                # source
├── helmrelease.yaml                  # chart pinning
├── kustomization.yaml                # Flux-type with healthChecks
└── kustomization-overlay.yaml        # kustomize.config.k8s.io Kustomization
```

Flux `Kustomization`s use `healthChecks` that include the CRDs + the
deployment:

```yaml
healthChecks:
  - { apiVersion: apiextensions.k8s.io/v1, kind: CustomResourceDefinition, name: caixas.caixa.pleme.io }
  - { apiVersion: apps/v1, kind: Deployment, name: caixa, namespace: caixa-system }
timeout: 5m
```

### 7.4 Namespaces

One namespace per subsystem: `caixa-system`, `escriba-system`,
`tatara-system`. Per-cluster overlays adjust labels / annotations, never
the namespace name. Namespaces carry pod-security-admission labels:

```yaml
pod-security.kubernetes.io/enforce: restricted
pod-security.kubernetes.io/audit:   restricted
pod-security.kubernetes.io/warn:    restricted
```

### 7.5 Kustomize overlays

Per-cluster overlays go in the cluster's GitOps repo (`k8s/`), never in the
service's repo. They only vary: `replicaCount`, `resources`, `tolerations`,
`nodeSelector`, image tag override, environment-specific secrets via SOPS.

Labels applied in overlays follow the shape:

```yaml
commonLabels:
  app.kubernetes.io/managed-by: flux
  app.kubernetes.io/part-of:    <service>
  cluster: <cluster-name>
  environment: <staging|production>
```

### 7.6 Deploy spec

Every service ships a `deploy.yaml` driving forge:

```yaml
apiVersion: forge.pleme.io/v1
kind: Deployment
metadata:
  name: <service>
  repo: pleme-io/<repo>
spec:
  services:
    - name: <service>
      kind: rust-service   # or rust-tool
      flakeOutput: <service>-image
      registry:
        host: ghcr.io
        path: pleme-io/<service>
      platforms: [linux-amd64, linux-arm64]
  artifacts:
    - { name: <service>-helm, kind: helm-chart, path: <service>-helm, ociRegistry: ghcr.io/pleme-io/charts }
  targets:
    - { name: staging,    cluster: zek, onBranch: main, autoPromote: true,  fluxPath: <service>-flux }
    - { name: production, cluster: plo, onTag: "v*",   autoPromote: false, fluxPath: <service>-flux }
  tests:
    pre:  [cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings, cargo fmt --all -- --check]
    post: [helm lint <service>-helm, kubeval --strict <service>-flux/*.yaml]
```

---

## 8. Forge + substrate patterns

### 8.1 substrate consumption

Every build leans on substrate's builders:

- `rust-tool-release-flake` for single-crate CLIs (4 targets)
- `rust-workspace-release-flake` for multi-crate workspaces
- `rust-service-flake` for dockerized services
- `rust-library.nix` for crates.io libraries

When the workspace pattern doesn't fit (e.g. our "no crate2nix"
requirement), inline `rustPlatform.buildRustPackage` directly but keep
substrate as a flake input for its helper modules.

### 8.2 forge CI/CD

- `deploy.yaml` declares what/where/when for every deployable.
- forge picks up on PR merge, builds via Nix, pushes to Attic + GHCR.
- Staging deploys auto; production gates on tag.
- Failed post-deploy tests roll back via FluxCD `remediation.retries`.

### 8.3 Attic cache

Every pleme-io repo's CI authenticates to the org Attic and pushes build
outputs. Downstream consumers (other workspaces, developer machines) pull
from Attic, never rebuild from scratch.

---

## 9. Testing pyramid

Layer by layer, cheapest first:

1. **Unit tests** (inline `#[cfg(test)]`) — nanoseconds. Every public fn.
2. **proptest** invariants — milliseconds. Every round-trip, every hash,
   every parser. 256 cases/property minimum.
3. **Integration tests** (`tests/`) — single-digit seconds. Real git
   remotes via tempfile, real tatara-lisp parsing, no network.
4. **Synthesis tests** (RSpec-style in Pangea, Rust proptest elsewhere) —
   tens of seconds. Zero cloud cost, proves invariants before deploy.
5. **InSpec + ami-test** — post-deploy verification against real
   infrastructure. Minutes.
6. **End-to-end pipeline** — `iac-test-runner` full bringup/verify/
   teardown. Hours, run on PR merge only.

CI must run layers 1–4 on every PR, layer 5 on main-branch push, layer 6
nightly.

---

## 10. Documentation

- **Code comments explain WHY, not WHAT.** Names explain what. Comments
  document invariants, non-obvious tradeoffs, references to incidents.
- **CLAUDE.md per repo** — top-level architecture + crate map. Keep it
  short; point to the code for detail.
- **`docs/` directory** for narrative documents (theory, manifestos,
  standards). One subject per file.
- **Rustdoc on every public item.** `missing_docs = "warn"` in CI.
- **OpenAPI is the public API reference.** No hand-written API docs;
  generate from the spec.

---

## 11. Naming the next primitive

When you're about to introduce a new concept and want to name it:

1. Is there an existing Japanese name that already fits? Reuse.
2. Is it a new Tier-2 concept (enclosed space, flow, growth, craft)?
   Use Brazilian-Portuguese. Pick one of: `vila`, `rio`, `raiz`, `floresta`,
   `bordado`, `peneira`, `teia`, `varanda`, `quintal`, `laje`, `selo`,
   `correio`, `farol`, `oficina`, `forja`, `caldeira`, `cordel`.
3. Is it a technical-jargon concept that's universally understood? Use
   English (`ast`, `lsp`, `fmt`, `ir`).
4. Crates named by concept, not by technology. `caixa-feira` not
   `caixa-cli-binary-with-git-resolver`.

---

## 12. Ruthless enforcement

The standards above are enforced by:

- `cargo fmt` — format. Non-negotiable.
- `cargo clippy -D warnings` — lint. Non-negotiable.
- `caixa-lint` — 10+ rules over Lisp sources. Can't merge with errors.
- `tatara-check` — workspace coherence. CI runs it on every PR.
- `helm lint` + `kubeval` — Helm + CRD validity.
- `cargo fmt`, `taplo fmt`, `nixfmt`, `biome format` — one formatter per
  language, zero options to argue about.
- `nix flake check` — the final gate.

When a standard conflicts with reality: file an issue, propose the
change, update this document, update the enforcer. Never ship a silent
exception.

---

## Appendix A — single-page cheatsheet

- Rust: edition 2024, `clippy::pedantic`, MIT, release profile as above.
- Lisp: kebab keywords, PascalCase enum symbols, `#[derive(TataraDomain)]`.
- Tests: inline + proptest + integration + synthesis + inspec + e2e.
- OpenAPI: schemars → spec → SDKs/MCP/docs. Never hand-write.
- Nix: rustPlatform, no crate2nix. HM + NixOS modules per service.
- K8s: CRDs under `*.pleme.io/v1alpha1`, kube-rs operators, dump-crds.
- Helm: blackmatter security baseline, ServiceMonitor gated, CRDs from
  dump-crds.
- FluxCD: Namespace + GitRepository + HelmRelease + Kustomization per
  deployable, healthChecks on CRDs + Deployment.
- forge: `deploy.yaml` per repo, staging auto-promote, production tag-gated.
- Naming: Japanese for base, Brazilian-Portuguese for new primitives,
  English for universal technical jargon.
