//! # tatara-pkgs — nixpkgs, fully expressed in tatara-lisp
//!
//! Two things live together:
//!
//! 1. **`PackageSet` trait** — backend-agnostic view of a package universe.
//!    Every backend (nixpkgs bridge, tatara-lisp-authored set, overlay of
//!    either) answers `get(name) -> Option<Derivation>` + `names() ->
//!    Vec<String>`.
//!
//! 2. **Synthesizer** — a `MultiSynthesizer` that walks a `PackageSet` and
//!    emits one tatara-lisp `.tl` file per package. `tend` drives this: on a
//!    nixpkgs commit bump, the generator regenerates the mirror tree; each
//!    package becomes a typed `(defderivation …)` that realizes to the same
//!    `/nix/store/...` path nixpkgs would produce.
//!
//! ```text
//! nixpkgs ──tend sync──►  PackageSet (NixpkgsBridge)
//!                              │
//!                              ▼ MultiSynthesizer::generate_all
//!                         Vec<Artifact>  (`hello.tl`, `bash.tl`, …)
//!                              │
//!                              ▼ disk
//!                         pleme-io/nixpkgs-tl/
//!                              │
//!                              ▼ tatara-eval + tatara-nix::Realizer
//!                         /nix/store/<hash>-<name>  (same path nixpkgs builds)
//! ```
//!
//! The gradient: today a tatara-lisp file is a bridge wrapper; tomorrow the
//! hot-path packages (stdenv, coreutils, bash, kernel) transliterate to pure
//! tatara-lisp and stop bridging. The typed surface doesn't change.

pub mod bridge;
pub mod generator;
pub mod overlay;
pub mod set;

pub use bridge::NixpkgsBridge;
pub use generator::{NixpkgsMirror, NixpkgsMirrorError};
pub use overlay::OverlayPackageSet;
pub use set::{PackageLookup, PackageSet, PackageSetError};
