//! `PackageSet` — the trait every package universe implements.

use thiserror::Error;

use tatara_nix::derivation::Derivation;

#[derive(Debug, Error)]
pub enum PackageSetError {
    #[error("unknown package: {0}")]
    Unknown(String),

    #[error("backend: {0}")]
    Backend(String),
}

/// Lookup result — either the derivation, or "I don't have it" (not an error).
pub type PackageLookup = Option<Derivation>;

/// A queryable set of packages. Implementations include:
/// - `NixpkgsBridge` — resolves names to nixpkgs attributes on disk
/// - `LispPackageSet` — tatara-lisp-authored derivations (future)
/// - `OverlayPackageSet` — one set composed over another
pub trait PackageSet: Send + Sync {
    /// Retrieve the derivation for a named package. `None` if not present.
    fn get(&self, name: &str) -> Result<PackageLookup, PackageSetError>;

    /// Short list of package names this set knows about. May be a finite
    /// enumeration (`LispPackageSet`) or a caller-provided seed
    /// (`NixpkgsBridge` doesn't enumerate nixpkgs by default — too large).
    fn names(&self) -> Vec<String>;

    /// Optional provenance label for logging / error messages.
    fn label(&self) -> &str {
        "anonymous"
    }
}
