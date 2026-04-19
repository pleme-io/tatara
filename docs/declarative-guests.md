# Declarative guests

> Every isolated execution environment — full Linux VM, WASI/WASM
> component, bare Linux process on a remote node — is a **Guest**.
> Same tatara-lisp authoring surface. Same typed Rust floor. Different
> runtime backends.

This document is the charter. It's the source of truth for what a
Guest is, how guests are authored, how they're built (locally or on the
quero.lol Nix fleet), how they're run (HVF primary, VZ fallback,
wasmtime + WasmEdge + Wasmer + wasmi + WAMR for WASM), and how they
integrate with tatara's Process/Reconciler model.

Locked in now so it survives the cross-session implementation work.

---

## 1. The shape

```
                   authored in tatara-lisp
        ┌──────────────────────────────────────────────┐
        │  (defguest name …                            │
        │     :kind    (:vm :backend :hvf)             │
        │     :build   (:flake "github:…#kernel")      │
        │     :build-on (:attic "quero.lol"            │
        │                :remote "ssh://builder.quero.lol" │
        │                :local #t))                   │
        └──────────────────────────┬───────────────────┘
                                   │
                      safe socket: compile_from_args
                                   │
                                   ▼
                           Rust-typed GuestSpec
                                   │
               ┌───────────────────┴───────────────────┐
               │                                       │
        GuestKind::Vm(VmSpec)                  GuestKind::Wasm(WasmSpec)
               │                                       │
     ┌─────────┴──────────┐             ┌──────────────┼────────────────────┐
     │                    │             │              │                    │
Backend::Hvf        Backend::Vz    Runtime::Wasmtime  WasmEdge  Wasmer  Wasmi  Wamr
(primary —         (fallback —       (Rust-native)    (C++)    (Rust)  (emb)  (micro)
 tatara-hvf)        kasou)
```

## 2. The three scope axes

**Axis A — what kind of guest.**

- **VM** — a full Linux guest. Kernel, initrd, rootfs, possibly shared
  folders, virtio devices.
- **WASM** — a WASI Preview 2 component. Sandboxed by the runtime, with
  declared WASI imports (fs, sockets, clocks, random, etc.) and a
  `main` export.

Both share `:network`, `:mounts`, `:services`, `:cmdline`, `:resource-limits`
in the spec. Only the `:kind`-specific block differs.

**Axis B — which backend hosts the guest.**

For VMs:
- **HVF (primary)** — Apple Hypervisor.framework via `tatara-hvf`.
  Full vCPU + memory control, own virtio-blk / virtio-net backends,
  no extra process overhead.
- **VZ (fallback)** — Apple Virtualization.framework via `kasou`
  (already shipped). Higher-level Apple objects, Rosetta 2 support.
  Used when HVF backend hits a gap or user explicitly opts in.

For WASM — multi-runtime, all first-class:
- **wasmtime** — Bytecode Alliance, Rust-native, WASI p2 reference impl.
- **WasmEdge** — CNCF, C++, strong compat, K8s adjacent.
- **Wasmer** — Rust-native, rich embedding targets.
- **wasmi** — pure Rust interpreter, embedded/no-alloc friendly.
- **WAMR** (WebAssembly Micro Runtime) — Bytecode Alliance, tiny footprint,
  AOT-capable.

The `WasmSpec::runtime` enum drives a trait dispatch. Consumers pick
the runtime per workload:

```lisp
(defguest fast-startup
  :kind (:wasm :runtime :wasmtime :wasi-preview "p2"))

(defguest embedded-control
  :kind (:wasm :runtime :wamr :wasi-preview "p1"
               :features (:aot #t :no-std #t)))
```

**Axis C — where the guest is built.**

Every guest's artifacts are Nix derivations: a VM has a `{kernel,
initrd, rootfs}` closure; a WASM guest has a `{component}` closure.
Builds go through a layered transport chain — first match wins:

1. **Attic cache** — `attic pull` from the `quero.lol` cache (shared
   Attic instance, public keys already configured in `nodes/cid/*.nix`).
2. **Remote builder** — `nix copy --from|--to 'ssh-ng://builder.quero.lol'`
   over the SSH key at `~/.ssh/pangea-builder` (already provisioned by
   `cid/pangea-builder.nix`).
3. **Local build** — `nix build` locally as last resort.

The Lisp surface is declarative about the full chain:

```lisp
:build-on (:attic "quero.lol"                    ; try cache first
           :remote "ssh://builder.quero.lol"     ; then remote build
           :local #t)                            ; fallback to local

:build-on "quero.lol"                            ; shorthand: all three
:build-on "local"                                ; shorthand: local only
:build-on (:remote-only "ssh://builder.quero.lol") ; refuse local fallback
```

Missing any lower-priority transport is a declared-error, not a surprise.

## 3. The Rust type model

```rust
#[derive(TataraDomain, Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defguest")]
pub struct GuestSpec {
    pub name: String,
    pub kind: GuestKind,
    pub build: BuildRef,
    #[serde(default)]
    pub build_on: BuildTransport,
    #[serde(default)]
    pub network: Vec<NetworkAttachment>,
    #[serde(default)]
    pub mounts: Vec<Mount>,
    #[serde(default)]
    pub services: Vec<ServiceDecl>,
    #[serde(default)]
    pub cmdline: Vec<String>,
    #[serde(default)]
    pub resources: ResourceLimits,
}

pub enum GuestKind {
    Vm(VmSpec),        // existing tatara-vm::VmSpec, extended with `backend`
    Wasm(WasmSpec),
}

pub struct VmSpec {
    pub backend: VmBackend,   // Hvf (primary) | Vz (fallback) | Qemu (portability)
    pub cpus: u32,
    pub memory_mib: u32,
    pub kernel: BuildRef,
    pub initrd: Option<BuildRef>,
    pub rootfs: BuildRef,
    // …
}

pub enum VmBackend { Hvf, Vz, Qemu }

pub struct WasmSpec {
    pub runtime: WasmRuntime,
    pub component: BuildRef,
    #[serde(default = "default_preview")]
    pub wasi_preview: WasiPreview,     // P1 | P2
    #[serde(default)]
    pub features: WasmFeatures,
}

pub enum WasmRuntime { Wasmtime, WasmEdge, Wasmer, Wasmi, Wamr }

pub struct WasmFeatures {
    pub aot: bool,
    pub jit: bool,
    pub threads: bool,
    pub simd: bool,
    pub no_std: bool,
    pub wasi_nn: bool,      // neural network interface
    pub wasi_http: bool,    // HTTP imports
}

pub enum BuildRef {
    Flake { url: String, attr: String },
    Nix   { expr: String },
    StorePath(String),      // pre-built, content-addressed
    Oci   { image: String, tag: String },
}

pub struct BuildTransport {
    pub attic: Option<String>,   // cache name (e.g. "quero.lol")
    pub remote: Option<String>,  // ssh URI
    pub local: bool,             // allow local fallback
}
```

## 4. Backends — the `GuestEngine` trait

```rust
pub trait GuestEngine {
    type Handle;
    fn boot(&self, spec: &GuestSpec, artifacts: &GuestArtifacts) -> Result<Self::Handle>;
    fn shutdown(&self, handle: &Self::Handle, grace: Duration) -> Result<()>;
    fn pause(&self, handle: &Self::Handle) -> Result<()>;
    fn resume(&self, handle: &Self::Handle) -> Result<()>;
    fn status(&self, handle: &Self::Handle) -> GuestStatus;
}
```

Implementations (one per Rust crate):

| Backend | Crate | Phase |
|---|---|---|
| HVF VM | `tatara-hvf` | Session 2 |
| VZ VM | `kasou` (exists) | shipped |
| Qemu VM | `tatara-vm-qemu` (tiny shell-out) | shipped via boot-gen |
| wasmtime | `tatara-wasm` (feature `wasmtime`) | Session 3 |
| WasmEdge | `tatara-wasm` (feature `wasmedge`) | Session 4 |
| Wasmer | `tatara-wasm` (feature `wasmer`) | Session 4 |
| wasmi | `tatara-wasm` (feature `wasmi`) | Session 4 |
| WAMR | `tatara-wasm` (feature `wamr`) | Session 4 |

`tatara-wasm` is one crate with Cargo feature flags per runtime — each
is pluggable. Default features: `wasmtime`. A consumer who wants WAMR
for embedded builds does `tatara-wasm = { …, default-features = false,
features = ["wamr"] }`.

## 5. hospedeiro — the runtime orchestrator

**`tatara-hospedeiro`** is the daemon/library that holds the live set
of running guests. It:

- Reads `GuestSpec` from Lisp, an MCP call, or the Process reconciler.
- Picks the right `GuestEngine` based on `GuestKind` + `Backend`.
- Routes `:build-on` through `tatara-build-remote` (Attic → ssh → local).
- Supervises lifecycle — restart, health probes, graceful shutdown.
- Publishes status via MCP + REST for `tatara-api` consumers.

Naming: Brazilian-Portuguese for "host" — the thing that **hosts**
guests. Per the pleme-io naming convention (Japanese for base primitives,
Brazilian-Portuguese for new Tier 2+ concepts), this is Tier 2.

## 6. Guest as Process::Intent variant

The Kubernetes-as-Unix-processes model in tatara-process stays intact.
Guests don't become a second top-level CRD — they become a variant of
`Intent`:

```rust
pub enum Intent {
    Nix(NixBuild),
    Flux(FluxTarget),
    Lisp(LispSource),
    Container(ContainerSpec),
    Guest(GuestSpec),   // NEW — wraps VM + WASM
}
```

`tatara-reconciler` gains an `execing_guest` phase that delegates to
hospedeiro. Lifecycle stays the same: `Pending → Forking → Execing →
Running → Attested`. Every guest gets:

- A PID in the ProcessTable.
- A BLAKE3 content-addressed identity.
- Three-pillar attestation (`artifact_hash` over the GuestArtifacts,
  `control_hash` over compliance bindings, `intent_hash` over the
  canonicalized GuestSpec).
- Signals via `tatara.pleme.io/signal` annotation — `SIGTERM` triggers
  graceful shutdown in both HVF and WASM backends.

## 7. Build transport — `tatara-build-remote`

```rust
pub trait BuildTransport {
    fn fetch(&self, r: &BuildRef) -> Result<StorePath>;
}

pub struct AtticTransport { cache: String }
pub struct SshRemoteTransport { host: String, key: PathBuf }
pub struct LocalTransport;

pub struct LayeredTransport(Vec<Box<dyn BuildTransport>>);
impl BuildTransport for LayeredTransport {
    fn fetch(&self, r: &BuildRef) -> Result<StorePath> {
        for t in &self.0 {
            if let Ok(p) = t.fetch(r) { return Ok(p); }
        }
        Err(BuildError::AllTransportsFailed)
    }
}
```

Every spec's `:build-on` compiles to a `LayeredTransport` with the
declared priority. quero.lol is the default remote + Attic target
because that's what cid already has SSH + cache config for.

## 8. Lisp surface examples

### A plex-like Linux VM on HVF, built remotely, Attic-cached

```lisp
(defguest plex
  :kind  (:vm :backend :hvf
              :cpus 4 :memory-mib 4096
              :kernel (:flake "github:pleme-io/tatara-os#kernel")
              :initrd (:flake "github:pleme-io/tatara-os#initrd")
              :rootfs (:flake "github:pleme-io/tatara-os#plex-rootfs"))
  :build-on (:attic "quero.lol"
             :remote "ssh://builder.quero.lol"
             :local #t)
  :network ((:kind :nat :subnet "10.200.0.0/24"))
  :mounts  ((:host "/Users/drzzln/media" :guest "/media" :ro #f))
  :services ((:name "plex" :kind :launchd
              :exec "/run/current-system/sw/bin/plex-media-server"))
  :cmdline ("console=hvc0" "init=/bin/tatara-init"))
```

### A fast cold-start WASI p2 component on wasmtime

```lisp
(defguest cors-proxy
  :kind  (:wasm :runtime :wasmtime :wasi-preview "p2"
                :component (:flake "github:pleme-io/cors-proxy#wasi")
                :features (:simd #t :wasi-http #t))
  :build-on "quero.lol"
  :network ((:kind :passthrough :listen "0.0.0.0:8080"))
  :resources (:memory-mib 64 :cpu-ms-budget 100))
```

### An embedded-profile guest on WAMR (no-std, AOT-compiled)

```lisp
(defguest blinky
  :kind  (:wasm :runtime :wamr :wasi-preview "p1"
                :component (:flake "github:pleme-io/blinky#wasi")
                :features (:aot #t :no-std #t))
  :build-on "quero.lol"
  :resources (:memory-mib 2))
```

### A Linux VM on VZ (fallback backend)

```lisp
(defguest legacy-builder
  :kind (:vm :backend :vz :cpus 2 :memory-mib 2048
             :kernel (:store-path "/nix/store/…-linux-kernel")
             :rootfs (:store-path "/nix/store/…-builder-rootfs"))
  :build-on "local")   ; already in the store
```

## 9. Safety invariants

1. **No guest runs against unbuilt artifacts.** hospedeiro refuses to
   boot until the BuildTransport has resolved every `BuildRef` to a
   concrete `StorePath`.
2. **No guest runs with mismatched architecture.** Guest `rootfs` /
   `component` must match the host arch OR be declared cross-arch
   (`:cross-arch #t`); runtime checks enforce.
3. **No authored file leakage.** hospedeiro never mutates on-disk host
   state outside `~/.local/state/tatara/guests/<name>/` and the Nix
   store.
4. **Signal propagation is typed.** `SIGTERM` → graceful
   `shutdown(grace = :default)`; `SIGKILL` → immediate `destroy()`;
   `SIGHUP` → `reload()` (re-read cmdline, re-mount shares).
5. **Build transport is honest.** If `:local` is `#f` and no remote
   transport resolves, hospedeiro errors at `Pending → Forking`. No
   silent local fallback.

## 10. Phasing

| Phase | Deliverable | Exit criterion |
|---|---|---|
| **H.1** | This doc; `GuestSpec` types in `tatara-vm`; stub crates `tatara-hvf`, `tatara-wasm`, `tatara-build-remote`, `tatara-hospedeiro`; `(defguest plex …)` compiles through and round-trips JSON | `cargo test -p tatara-vm` green; `(defguest …)` parses |
| **H.2** | `tatara-hvf` implements `GuestEngine` for VMs via Hypervisor.framework. Virtio-blk + virtio-net backends. | A plex-like guest boots on HVF and exits cleanly |
| **H.3** | `tatara-wasm` with `wasmtime` feature. First WASI p2 component runs via hospedeiro | `repo-forge new --archetype wasi-component …` → `hospedeiro run` → "hello world" |
| **H.4** | `tatara-wasm` fans out to WasmEdge + Wasmer + wasmi + WAMR as Cargo features | Same component runs on all five runtimes; benchmark table committed |
| **H.5** | `tatara-build-remote` implements layered Attic → ssh-ng → local transport targeting quero.lol | `:build-on "quero.lol"` actually routes; Attic cache hits measurable |
| **H.6** | `tatara-hospedeiro` daemon supervises the live guest set; MCP tools exposed | `kubectl get processes.tatara.pleme.io` shows live VMs + WASM components |
| **H.7** | `Intent::Guest(GuestSpec)` added to `tatara-process`; reconciler wires guest lifecycle to hospedeiro | A `Process` CR with `intent.guest: …` transitions through the standard phases |
| **H.8** | Fleet absorption — `repo-forge` archetypes for `tatara-workspace-member`, `wasi-component-wit`, `linux-guest-nix-flake` so new guest repos snap into the catalog | `repo-forge list-archetypes` shows them |

## 11. Non-goals

- **No KVM.** Mac-only primary target. Linux KVM support may come via
  cross-host builder but not as a first-class backend.
- **No Windows guests.** Linux VMs + WASM only, for now.
- **No custom kernel build system.** Guest kernels come through Nix
  (`tatara-os`) and flake refs. No in-tree kernel trees.
- **No byte-identical WASM output across runtimes.** Each runtime has
  its own JIT/AOT caches; the guarantee is observable behavior, not
  bit-exact compilation artifacts.
- **No guest-side agents that mutate the host.** Guests run sandboxed.

## 12. Related documents

- `tatara/docs/rust-lisp.md` — the Rust+Lisp pattern manifesto.
- `tatara/docs/k8s-as-processes.md` — Process CRD model; Guest is a new Intent variant.
- `tatara/docs/tatara-lisp-standards.md` — §4.5 (ratatui canonical), §4.6 (arnes).
- `pleme-io/repo-forge/docs/principles.md` — "we can't lose work" — same
  discipline applies to guests (never delete host state; authored
  artifacts outside the guest are untouched).

## 13. Sign-off

Every phase commit must reference this doc. Changes to the type model
or backend surface are type-system-level changes and propagate through
`cargo test --workspace`. Tatara's `checks.lisp` gains an entry
verifying the authored guest catalog compiles cleanly.

*Last updated: 2026-04-18.*
