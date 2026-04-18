//! **Nix's academic essence, re-expressed in Rust + Lisp.**
//!
//! Nix's innovation is a theory, not a language:
//!   1. Purely functional package management
//!   2. Content-addressed storage (hash of declared inputs → store path)
//!   3. Atomic upgrades + rollbacks
//!   4. Hermeticity (sandboxed builds)
//!   5. Laziness (evaluate only what's needed)
//!   6. Composable modules + overlays
//!   7. Hermetic flake inputs + outputs
//!
//! Every one is a *type discipline*, not a language feature. Nix's DSL is one
//! projection of these types; Lisp + Rust is another.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    tatara-lisp (authoring)                   │
//! │   (defderivation hello …)    (defmodule observability …)     │
//! │   (defflake my-system …)     (defoverlay add-patches …)      │
//! └──────────────────────────────────────────────────────────────┘
//!                              ↓ compile_from_sexp (tatara-lisp-derive)
//! ┌──────────────────────────────────────────────────────────────┐
//! │                   tatara-nix (typed IR)                      │
//! │   Derivation · StorePath · Module · Overlay · Flake           │
//! └──────────────────────────────────────────────────────────────┘
//!                              ↓ evaluate
//! ┌──────────────────────────────────────────────────────────────┐
//! │                   sui (pure-Rust evaluator)                  │
//! │   lazy semantics · store · build sandbox · cache             │
//! └──────────────────────────────────────────────────────────────┘
//!                              ↓ attest
//! ┌──────────────────────────────────────────────────────────────┐
//! │    tatara-core::ConvergenceAttestation (BLAKE3 Merkle)       │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! The core types in this crate — `Derivation`, `StorePath`, `Module`,
//! `Overlay`, `Flake` — all derive `TataraDomain`, so every one has a Lisp
//! authoring surface for free. sui (separate crate) is the pure-Rust
//! evaluator; tatara-nix is the typed IR it consumes.

pub mod derivation;
pub mod evaluator;
pub mod flake;
pub mod module;
pub mod overlay;
pub mod overlay_compose;
pub mod resolver;
pub mod store;

pub use derivation::{BuilderPhase, BuilderPhases, Derivation, EnvVar, InputRef, Outputs, Source};
pub use evaluator::{DryRun, EvaluationResult, Evaluator, Plan};
pub use flake::{Flake, FlakeInput, FlakeOutputs};
pub use module::{Module, ModuleImport, ModuleOption, MkExpr, OptionType};
pub use overlay::{Overlay, OverlayTarget};
pub use overlay_compose::{apply, apply_chain, compose, ComposeError, PackageSet};
pub use resolver::{resolve_module, resolve_modules, Priority, ResolveError};
pub use store::{StoreHash, StorePath};

/// Register every tatara-nix domain with the global Lisp dispatcher.
/// Call once at binary startup to make `(defderivation …)`, `(defmodule …)`,
/// etc. resolvable via `tatara_lisp::domain::lookup`.
pub fn register_all() {
    tatara_lisp::domain::register::<Derivation>();
    tatara_lisp::domain::register::<Module>();
    tatara_lisp::domain::register::<Flake>();
    tatara_lisp::domain::register::<Overlay>();
}
