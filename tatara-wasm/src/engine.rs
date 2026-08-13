//! The `WasmEngine` trait — polymorphic over runtime — and the typed
//! capability border over it.
//!
//! Each runtime (wasmtime, WasmEdge, Wasmer, wasmi, WAMR) lives behind
//! a Cargo feature flag and implements this trait. Consumers pick a
//! runtime per workload via `WasmSpec::runtime`; `boot()` dispatches
//! to the right implementation.
//!
//! ## One table, three defects
//!
//! [`facts_of`] is the single source of truth for *what each engine can
//! actually do*. Before it, that knowledge was scattered into three places
//! that disagreed with each other and with the code:
//!
//! - the crate header claimed all five runtimes were production-grade while
//!   its own Status block called them empty stubs (`lib.rs`, both wrong, in
//!   opposite directions);
//! - the "does this engine accept this preview?" rule was a hand-copied
//!   `if boot.preview == P2` in each of the three impls;
//! - nothing at all recorded which engines wire WASI, so `wasi_snapshot_preview1`
//!   imports failed with an opaque link error on the two that do not.
//!
//! All three now read one exhaustive `match`. [`facts_of`] has **no wildcard
//! arm**, so a sixth runtime landing without a facts row is an `E0004`, not a
//! silent default.
//!
//! ## The capability border
//!
//! `WasmBoot` used to carry no capability declaration at all, so the surface a
//! guest got was *whatever the impl happened to be written to grant* —
//! `wasmtime_impl` hardcoded stdout+stderr, `wasmi_impl` instantiated against a
//! literally empty `Linker`. [`WasmCapabilities`] replaces that with a
//! declaration the caller makes and the engine derives from, and its `Default`
//! is **deny-all**: no WASI, no host imports.
//!
//! The load-bearing property, which is why the grant is `Option` and not a set
//! of booleans: when `wasi` is `None` the WASI linker is **never added**, so a
//! `wasi_snapshot_preview1` import has no symbol to resolve. That is absence,
//! not a policy check — the distinction `theory/BLUE-EXECUTION.md` §V rests its
//! `truly-unrep` capability rows on.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{WasiPreview, WasmFeatures, WasmRuntime};

/// The WASI preview-1 import module name.
pub const WASI_P1_MODULE: &str = "wasi_snapshot_preview1";

/// The input to `WasmEngine::boot`. Self-contained: bytes of a WASM
/// module (or a WAT source string), a spec, and an artifact path.
#[derive(Debug, Clone)]
pub struct WasmBoot {
    /// Either raw WASM bytes (preferred) or a WAT text source. Engines
    /// decide how to ingest.
    pub module: WasmModuleSource,

    /// Runtime demanded by the spec.
    pub runtime: WasmRuntime,

    /// WASI preview version.
    pub preview: WasiPreview,

    /// Feature toggles.
    pub features: WasmFeatures,

    /// What the host grants this guest. **Deny-all by default** — see
    /// [`WasmCapabilities`].
    pub capabilities: WasmCapabilities,

    /// Name used for logging + handle identification.
    pub name: String,
}

/// What the host grants a guest.
///
/// **`Default` is deny-all**: no WASI context is built and no host import is
/// permitted. That is the posture a capability border has to start from — the
/// previous behaviour (stdio hardcoded into each impl) could not express
/// "this guest gets nothing", which is the only safe default for an untrusted
/// module.
///
/// ## Granularity, stated rather than guessed
///
/// `host_imports` is exact: one entry per `(module, field)` the embedder will
/// supply, and an import outside that set is refused by name.
///
/// The WASI grant is **coarse at the module boundary and fine at the context**:
/// granting `wasi` admits `wasi_snapshot_preview1` imports as a group, and what
/// the guest can then actually *do* is bounded by the context built from this
/// declaration — no preopen means `path_open` resolves but opens nothing, no
/// stdout grant means `fd_write` to fd 1 goes to a sink. Mapping each of WASI's
/// ~45 functions onto a finer capability is a claim about an ABI, and this
/// crate does not have one to make yet. Splitting the grant further is a later
/// variant and the `Option` shape leaves room for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmCapabilities {
    /// The WASI grant. `None` means the WASI linker is **never added**, so
    /// every `wasi_snapshot_preview1` import is unresolvable.
    #[serde(default)]
    pub wasi: Option<WasiGrant>,

    /// Non-WASI host imports the embedder undertakes to supply — e.g. blue's
    /// `blue:host` table, derived from a frame by `blue_lang_waku::imports_of`.
    ///
    /// A `BTreeSet` so the declaration has one canonical order and comparing
    /// two grants is comparing two sets.
    #[serde(default)]
    pub host_imports: BTreeSet<HostImport>,
}

impl WasmCapabilities {
    /// Deny-all: no WASI, no host imports. Same as `Default`, named so a
    /// caller can say what it means at the call site.
    #[must_use]
    pub fn closed() -> Self {
        Self::default()
    }

    /// A WASI grant of captured stdout + stderr and nothing else.
    ///
    /// This is what the three engines used to hardcode. It exists so the
    /// migration from "implicit" to "declared" is one call rather than a
    /// struct literal, **not** because it is a good default — it is not the
    /// default, deliberately.
    #[must_use]
    pub fn stdio() -> Self {
        Self {
            wasi: Some(WasiGrant {
                stdout: true,
                stderr: true,
                ..WasiGrant::default()
            }),
            host_imports: BTreeSet::new(),
        }
    }

    /// Declare the non-WASI host imports, replacing any already declared.
    #[must_use]
    pub fn with_host_imports<I, M, F>(mut self, imports: I) -> Self
    where
        I: IntoIterator<Item = (M, F)>,
        M: Into<String>,
        F: Into<String>,
    {
        self.host_imports = imports
            .into_iter()
            .map(|(module, field)| HostImport {
                module: module.into(),
                field: field.into(),
            })
            .collect();
        self
    }
}

/// The WASI half of a grant. Every field is something the guest would
/// otherwise have taken implicitly.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WasiGrant {
    /// Capture the guest's stdout into `WasmHandle::stdout`.
    #[serde(default)]
    pub stdout: bool,
    /// Capture the guest's stderr into `WasmHandle::stderr`.
    #[serde(default)]
    pub stderr: bool,
    /// Environment variables visible to the guest. Empty means none — the
    /// host's own environment is never inherited.
    #[serde(default)]
    pub env: Vec<(String, String)>,
    /// The guest's argument vector, `argv[0]` included. Empty means none.
    #[serde(default)]
    pub argv: Vec<String>,
    /// Directories mapped into the guest. Empty means the guest has no
    /// filesystem at all, which is the default.
    #[serde(default)]
    pub preopens: Vec<Preopen>,
}

/// One directory mapped from host to guest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preopen {
    /// The directory on the host.
    pub host_path: PathBuf,
    /// The path the guest sees it at.
    pub guest_path: String,
    /// Whether the guest may write. Read-only unless asked for.
    #[serde(default)]
    pub writable: bool,
}

/// One `(module, field)` pair the embedder undertakes to supply.
///
/// Owned `String`s rather than `&'static str` because a declaration can be
/// deserialized or derived at run time; blue's `Import` (whose fields are
/// `&'static str`) converts into this in one `map`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostImport {
    /// The wasm import module — e.g. blue's `blue:host`.
    pub module: String,
    /// The field within it.
    pub field: String,
}

impl HostImport {
    /// Build one from any two string-likes.
    pub fn new(module: impl Into<String>, field: impl Into<String>) -> Self {
        Self {
            module: module.into(),
            field: field.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum WasmModuleSource {
    /// Compiled WASM binary bytes — the canonical path.
    Bytes(Vec<u8>),
    /// WebAssembly Text Format source. Engines that don't support WAT
    /// directly must reject this variant or convert via `wat`.
    Wat(String),
    /// A file path on disk. Reader runs inside the engine.
    Path(std::path::PathBuf),
}

/// A live WASM guest handle.
#[derive(Debug)]
pub struct WasmHandle {
    pub name: String,
    pub runtime: WasmRuntime,
    /// Accumulated captured stdout, for tests + replay.
    pub stdout: String,
    /// Accumulated captured stderr.
    pub stderr: String,
    /// The guest's exit code, if it has terminated.
    pub exit_code: Option<i32>,
}

impl WasmHandle {
    #[must_use]
    pub fn new(name: impl Into<String>, runtime: WasmRuntime) -> Self {
        Self {
            name: name.into(),
            runtime,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
        }
    }
}

/// What one runtime can actually do, as data.
///
/// The point of this type is that there is exactly one of it per runtime and
/// everything else reads it: the preview check, the WASI check, the crate's
/// own status string, and the tests that keep all three honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineFacts {
    /// The runtime these facts describe.
    pub runtime: WasmRuntime,
    /// Whether an implementation exists in this crate at all. Independent of
    /// Cargo features — `Wamr` and `WasmEdge` have no body to compile in.
    pub implemented: bool,
    /// The WASI previews this engine accepts. Empty means it takes no WASI
    /// module at all.
    pub previews: &'static [WasiPreview],
    /// Whether this engine wires a WASI context. `false` means a
    /// `wasi_snapshot_preview1` import cannot be satisfied here.
    pub provides_wasi: bool,
}

impl EngineFacts {
    /// Does this engine accept `preview`?
    #[must_use]
    pub fn accepts(&self, preview: WasiPreview) -> bool {
        self.previews.contains(&preview)
    }
}

/// The one table. **No wildcard arm** — a sixth `WasmRuntime` is an `E0004`
/// here, which is the whole reason the knowledge lives in one place.
///
/// `engine_for` below still needs a catch-all because its arms are
/// `cfg`-gated and a fully-featureless build would leave it empty; this match
/// is where the exhaustiveness is enforced instead.
#[must_use]
pub const fn facts_of(runtime: WasmRuntime) -> EngineFacts {
    match runtime {
        // Wires `wasmtime_wasi::preview1`, so it takes P1 and provides WASI.
        // P2 (component model) is deliberately absent: the impl builds a
        // `WasiP1Ctx` and calls `_start`, which is the p1 entry convention.
        WasmRuntime::Wasmtime => EngineFacts {
            runtime,
            implemented: true,
            previews: &[WasiPreview::P1],
            provides_wasi: true,
        },
        // Compiles + runs, but instantiates against an empty import object:
        // no WASI bridge. Pure-compute guests only.
        WasmRuntime::Wasmer => EngineFacts {
            runtime,
            implemented: true,
            previews: &[WasiPreview::P1],
            provides_wasi: false,
        },
        // Same shape as wasmer: a real interpreter, an empty `Linker`.
        WasmRuntime::Wasmi => EngineFacts {
            runtime,
            implemented: true,
            previews: &[WasiPreview::P1],
            provides_wasi: false,
        },
        // No body in this crate — needs C++ SDK linking.
        WasmRuntime::WasmEdge => EngineFacts {
            runtime,
            implemented: false,
            previews: &[],
            provides_wasi: false,
        },
        // No body in this crate — needs a C build harness.
        WasmRuntime::Wamr => EngineFacts {
            runtime,
            implemented: false,
            previews: &[],
            provides_wasi: false,
        },
    }
}

/// Every runtime this crate actually implements, derived from [`facts_of`].
///
/// Used by the crate's status string so a prose claim about maturity cannot
/// drift from the code that decides it.
#[must_use]
pub fn implemented_runtimes() -> Vec<WasmRuntime> {
    ALL_RUNTIMES
        .iter()
        .copied()
        .filter(|r| facts_of(*r).implemented)
        .collect()
}

/// Every `WasmRuntime` variant.
///
/// A hand-written list is exactly the thing that goes stale, so it is not
/// trusted on its own: [`next_runtime`] is an exhaustive `match` walking one
/// variant to the next, and `tests::all_runtimes_covers_every_variant` walks it
/// and compares. Adding a sixth variant is an `E0004` in `next_runtime` *and*
/// in [`facts_of`]; once those are filled in, the walk contains the new variant
/// and this list does not — so the test goes red until it does.
pub const ALL_RUNTIMES: &[WasmRuntime] = &[
    WasmRuntime::Wasmtime,
    WasmRuntime::WasmEdge,
    WasmRuntime::Wasmer,
    WasmRuntime::Wasmi,
    WasmRuntime::Wamr,
];

/// The variant after this one, in [`ALL_RUNTIMES`] order.
///
/// **No wildcard arm.** This exists only so the list above has an independent
/// witness the compiler maintains — the same device
/// `blue_lang_waku::Capability` uses to keep its `ALL` honest.
#[must_use]
pub const fn next_runtime(runtime: WasmRuntime) -> Option<WasmRuntime> {
    match runtime {
        WasmRuntime::Wasmtime => Some(WasmRuntime::WasmEdge),
        WasmRuntime::WasmEdge => Some(WasmRuntime::Wasmer),
        WasmRuntime::Wasmer => Some(WasmRuntime::Wasmi),
        WasmRuntime::Wasmi => Some(WasmRuntime::Wamr),
        WasmRuntime::Wamr => None,
    }
}

/// Walk [`next_runtime`] from the first variant, yielding every variant.
#[must_use]
pub fn walk_runtimes() -> Vec<WasmRuntime> {
    let mut out = vec![WasmRuntime::Wasmtime];
    while let Some(next) = next_runtime(*out.last().expect("non-empty")) {
        out.push(next);
    }
    out
}

/// The polymorphic-over-runtime engine trait.
pub trait WasmEngine {
    /// Which runtime this engine represents. Matches `WasmSpec::runtime`.
    fn runtime(&self) -> WasmRuntime;

    /// What this engine can do. Derived from [`facts_of`] — an impl should not
    /// override this, and none does.
    fn facts(&self) -> EngineFacts {
        facts_of(self.runtime())
    }

    /// Boot the module and run it to completion (synchronous today).
    /// Captures stdout/stderr into the returned handle.
    ///
    /// # Errors
    /// Returns `WasmEngineError` on compile / instantiate / runtime failure.
    fn run(&self, boot: &WasmBoot) -> Result<WasmHandle, WasmEngineError>;

    /// Graceful shutdown — runtimes with long-running guests honor this.
    /// The default synchronous `run()` path is already terminated, so the
    /// default impl is a no-op.
    ///
    /// # Errors
    /// Implementations that have background resources may error.
    fn shutdown(
        &self,
        _handle: &mut WasmHandle,
        _grace: Duration,
    ) -> Result<(), WasmEngineError> {
        Ok(())
    }
}

/// Validate a boot against an engine's facts, before any compilation.
///
/// One implementation of a rule that used to be a hand-copied `if` in each of
/// the three backends — and which each of them stated as `preview == P2`,
/// a literal that would have had to be edited in three places the moment a
/// third preview landed.
///
/// # Errors
/// - [`WasmEngineError::PreviewNotSupported`] if the engine does not accept the
///   requested preview.
/// - [`WasmEngineError::WasiNotProvided`] if WASI was granted to an engine that
///   wires no WASI context — a grant that would otherwise be silently ignored.
pub fn check_boot(facts: &EngineFacts, boot: &WasmBoot) -> Result<(), WasmEngineError> {
    if !facts.accepts(boot.preview) {
        return Err(WasmEngineError::PreviewNotSupported(boot.preview));
    }
    if boot.capabilities.wasi.is_some() && !facts.provides_wasi {
        return Err(WasmEngineError::WasiNotProvided(facts.runtime));
    }
    Ok(())
}

/// Refuse any import the capability declaration does not grant.
///
/// Called by each backend with that engine's own import iterator, so the
/// *policy* has one implementation and only the *enumeration* is per-engine.
///
/// Note what this does and does not add. A module importing something absent
/// from the linker already fails to instantiate — that is the structural
/// property, and it holds with or without this function. What this adds is a
/// *typed, early, named* refusal: `CapabilityNotGranted { module, field }`
/// before compilation, instead of an engine-specific link-error string after.
///
/// # Errors
/// [`WasmEngineError::CapabilityNotGranted`] naming the first ungranted import.
pub fn check_imports_granted<M, F>(
    caps: &WasmCapabilities,
    imports: impl Iterator<Item = (M, F)>,
) -> Result<(), WasmEngineError>
where
    M: AsRef<str>,
    F: AsRef<str>,
{
    let wasi_granted = caps.wasi.is_some();
    for (module, field) in imports {
        let (module, field) = (module.as_ref(), field.as_ref());
        if module == WASI_P1_MODULE {
            if wasi_granted {
                continue;
            }
            return Err(WasmEngineError::CapabilityNotGranted {
                module: module.to_string(),
                field: field.to_string(),
            });
        }
        let granted = caps
            .host_imports
            .iter()
            .any(|h| h.module == module && h.field == field);
        if !granted {
            return Err(WasmEngineError::CapabilityNotGranted {
                module: module.to_string(),
                field: field.to_string(),
            });
        }
    }
    Ok(())
}

/// Read a module source into bytes.
///
/// Lifted here from the three backends, which each carried a byte-identical
/// copy of it.
///
/// # Errors
/// [`WasmEngineError::Compile`] on a WAT parse failure, [`WasmEngineError::Io`]
/// if a path cannot be read.
pub fn read_module(src: &WasmModuleSource) -> Result<Vec<u8>, WasmEngineError> {
    match src {
        WasmModuleSource::Bytes(b) => Ok(b.clone()),
        WasmModuleSource::Wat(text) => {
            wat::parse_str(text).map_err(|e| WasmEngineError::Compile(format!("WAT parse: {e}")))
        }
        WasmModuleSource::Path(p) => {
            let mut buf = Vec::new();
            std::fs::File::open(p)
                .and_then(|mut f| f.read_to_end(&mut buf))
                .map_err(|e| WasmEngineError::Io(format!("{}: {e}", p.display())))?;
            Ok(buf)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WasmEngineError {
    #[error("runtime not compiled in: {0:?}")]
    RuntimeNotCompiled(WasmRuntime),

    #[error("preview not supported by this runtime: {0:?}")]
    PreviewNotSupported(WasiPreview),

    #[error("runtime {0:?} wires no WASI context, but the boot granted WASI")]
    WasiNotProvided(WasmRuntime),

    #[error("capability not granted: import {module}::{field} is not in the declaration")]
    CapabilityNotGranted { module: String, field: String },

    #[error("module compile failed: {0}")]
    Compile(String),

    #[error("instantiate failed: {0}")]
    Instantiate(String),

    #[error("run failed: {0}")]
    Run(String),

    #[error("io: {0}")]
    Io(String),
}

/// Factory: return a boxed engine for the requested runtime. Errors if
/// the runtime's feature flag is not compiled in.
///
/// # Errors
/// Returns `WasmEngineError::RuntimeNotCompiled` when the requested
/// runtime's feature isn't enabled at compile time.
pub fn engine_for(runtime: WasmRuntime) -> Result<Box<dyn WasmEngine>, WasmEngineError> {
    match runtime {
        #[cfg(feature = "runtime-wasmtime")]
        WasmRuntime::Wasmtime => Ok(Box::new(crate::wasmtime_impl::WasmtimeEngine::default())),

        #[cfg(feature = "runtime-wasmer")]
        WasmRuntime::Wasmer => Ok(Box::new(crate::wasmer_impl::WasmerEngine::default())),

        #[cfg(feature = "runtime-wasmi")]
        WasmRuntime::Wasmi => Ok(Box::new(crate::wasmi_impl::WasmiEngine::default())),

        // WasmEdge + WAMR land in H.4 follow-up (C/C++ SDK linking).
        other => Err(WasmEngineError::RuntimeNotCompiled(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate the P2 default defect needed and did not have.
    ///
    /// `WasiPreview::default()` was `P2`, and every engine in this crate
    /// rejected P2 — so a `WasmBoot` built from defaults failed on every
    /// backend. Asserting `default() == P1` would only pin a new constant;
    /// this asserts the *invariant* that the constant has to satisfy, so it
    /// keeps holding when a third preview lands or the default moves again.
    ///
    /// RED RUN (2026-08-13): with `WasiPreview`'s `#[default]` moved back to
    /// `P2`, this fails —
    ///   `default preview P2 is rejected by Wasmtime, whose previews are [P1]`.
    /// Restored to P1: green.
    #[test]
    fn default_preview_is_accepted_by_every_implemented_engine() {
        let default = WasiPreview::default();
        for runtime in ALL_RUNTIMES.iter().copied() {
            let facts = facts_of(runtime);
            if !facts.implemented {
                continue;
            }
            assert!(
                facts.accepts(default),
                "default preview {default:?} is rejected by {runtime:?}, \
                 whose previews are {:?}",
                facts.previews
            );
        }
    }

    /// An implemented engine that accepts nothing would pass the gate above
    /// vacuously — `accepts` over an empty slice is trivially unreachable, so
    /// the loop body would never assert. This is the guard on that guard.
    ///
    /// RED RUN (2026-08-13): `previews: &[]` on the Wasmtime row →
    ///   `Wasmtime is implemented but accepts no preview at all`.
    #[test]
    fn every_implemented_engine_accepts_at_least_one_preview() {
        for runtime in ALL_RUNTIMES.iter().copied() {
            let facts = facts_of(runtime);
            if facts.implemented {
                assert!(
                    !facts.previews.is_empty(),
                    "{runtime:?} is implemented but accepts no preview at all"
                );
            }
        }
    }

    /// A runtime with no body must not claim a capability.
    ///
    /// RED RUN (2026-08-13): `implemented: false, provides_wasi: true` on the
    /// Wamr row → `Wamr has no implementation but claims capabilities`.
    #[test]
    fn unimplemented_runtimes_claim_nothing() {
        for runtime in ALL_RUNTIMES.iter().copied() {
            let facts = facts_of(runtime);
            if !facts.implemented {
                assert!(
                    facts.previews.is_empty() && !facts.provides_wasi,
                    "{runtime:?} has no implementation but claims capabilities"
                );
            }
        }
    }

    /// `ALL_RUNTIMES` is a hand-written list, so it is the one place a sixth
    /// variant could be forgotten. `next_runtime` is the independent witness:
    /// it is an exhaustive `match`, so the compiler forces an arm for a new
    /// variant, and walking it produces the true variant set.
    ///
    /// The first draft of this test asserted `ALL_RUNTIMES.len() == 5` against
    /// a hand-written `5` — which is the tautology this repo's testing
    /// discipline names: a new variant would have left both the list and the
    /// literal at five and the gate green. Replaced with the walk.
    ///
    /// RED RUN (2026-08-13): dropping `Wamr` from `ALL_RUNTIMES` (leaving
    /// `next_runtime` intact, as a forgetful edit would) →
    ///   `ALL_RUNTIMES is missing variants the walk found: [Wamr]`.
    #[test]
    fn all_runtimes_covers_every_variant() {
        let walked = walk_runtimes();
        let listed: BTreeSet<String> = ALL_RUNTIMES.iter().map(|r| format!("{r:?}")).collect();
        assert_eq!(
            listed.len(),
            ALL_RUNTIMES.len(),
            "ALL_RUNTIMES contains a duplicate"
        );
        let missing: Vec<_> = walked
            .iter()
            .filter(|r| !listed.contains(&format!("{r:?}")))
            .collect();
        assert!(
            missing.is_empty(),
            "ALL_RUNTIMES is missing variants the walk found: {missing:?}"
        );
        assert_eq!(
            walked.len(),
            ALL_RUNTIMES.len(),
            "ALL_RUNTIMES has {} entries, the walk found {}",
            ALL_RUNTIMES.len(),
            walked.len()
        );
    }

    /// The default capability declaration must grant nothing.
    ///
    /// RED RUN (2026-08-13): `WasmCapabilities::default()` replaced with
    /// `Self::stdio()` → `the default capability declaration granted WASI`.
    #[test]
    fn default_capabilities_are_deny_all() {
        let caps = WasmCapabilities::default();
        assert!(
            caps.wasi.is_none(),
            "the default capability declaration granted WASI"
        );
        assert!(
            caps.host_imports.is_empty(),
            "the default capability declaration granted host imports"
        );
    }

    /// The refusal itself, on the exact shape blue produces: a `blue:host`
    /// import that the frame did not grant.
    ///
    /// RED RUN (2026-08-13): `check_imports_granted` returning `Ok(())`
    /// unconditionally → `an ungranted host import was admitted`.
    #[test]
    fn ungranted_host_import_is_refused_by_name() {
        let caps = WasmCapabilities::default()
            .with_host_imports([("blue:host", "clock")]);

        // Granted: passes.
        check_imports_granted(&caps, [("blue:host", "clock")].into_iter())
            .expect("a declared import must be admitted");

        // Ungranted, same module: refused, and the error names it.
        let err = check_imports_granted(&caps, [("blue:host", "fs")].into_iter())
            .expect_err("an ungranted host import was admitted");
        match err {
            WasmEngineError::CapabilityNotGranted { module, field } => {
                assert_eq!(module, "blue:host");
                assert_eq!(field, "fs");
            }
            other => panic!("wrong error: {other:?}"),
        }
    }

    /// WASI is refused as a group when the grant is absent, which is the
    /// differential `BLUE-EXECUTION.md` M2's gate asks for: the same module,
    /// admitted or refused purely by what the declaration carries.
    ///
    /// RED RUN (2026-08-13): `wasi_granted` forced to `true` →
    ///   `a WASI import was admitted with no WASI grant`.
    #[test]
    fn wasi_import_is_refused_without_a_grant() {
        let imports = || [(WASI_P1_MODULE, "fd_write")].into_iter();

        let closed = WasmCapabilities::closed();
        check_imports_granted(&closed, imports())
            .expect_err("a WASI import was admitted with no WASI grant");

        let open = WasmCapabilities::stdio();
        check_imports_granted(&open, imports())
            .expect("a WASI import must be admitted under a WASI grant");
    }

    /// A WASI grant handed to an engine that wires no WASI context is a
    /// declaration that would otherwise be silently dropped.
    ///
    /// RED RUN (2026-08-13): the `provides_wasi` clause deleted from
    /// `check_boot` → `wasmi accepted a WASI grant it cannot honour`.
    #[test]
    fn wasi_grant_on_a_non_wasi_engine_is_refused() {
        let boot = WasmBoot {
            module: WasmModuleSource::Wat(String::new()),
            runtime: WasmRuntime::Wasmi,
            preview: WasiPreview::P1,
            features: WasmFeatures::default(),
            capabilities: WasmCapabilities::stdio(),
            name: "probe".into(),
        };
        let err = check_boot(&facts_of(WasmRuntime::Wasmi), &boot)
            .expect_err("wasmi accepted a WASI grant it cannot honour");
        assert!(matches!(err, WasmEngineError::WasiNotProvided(WasmRuntime::Wasmi)));

        // The same boot with no grant passes — proving the refusal is about
        // the grant and not about wasmi rejecting everything.
        let closed = WasmBoot {
            capabilities: WasmCapabilities::closed(),
            ..boot
        };
        check_boot(&facts_of(WasmRuntime::Wasmi), &closed)
            .expect("wasmi must accept a boot that grants nothing");
    }
}
