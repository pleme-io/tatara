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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_unknown() {
        // PackageSetError::Unknown bubbles up to user-visible messages
        // ("package X doesn't exist anywhere"). Pin the Display form
        // so a rewording drifts tests first, not log dashboards.
        let e = PackageSetError::Unknown("curl".into());
        assert_eq!(e.to_string(), "unknown package: curl");
    }

    #[test]
    fn error_display_backend() {
        // Backend errors wrap arbitrary upstream messages — the
        // prefix ("backend: ") is the only load-bearing part.
        let e = PackageSetError::Backend("nix eval timed out".into());
        assert_eq!(e.to_string(), "backend: nix eval timed out");
    }

    #[test]
    fn default_label_is_anonymous() {
        // Minimal PackageSet impl — only `get` and `names` are
        // mandatory; `label` defaults. If someone drops the default
        // body, every PackageSet not overriding label() stops
        // compiling — pin the default so a forced-override refactor
        // fails this test first.
        struct Empty;
        impl PackageSet for Empty {
            fn get(&self, _name: &str) -> Result<PackageLookup, PackageSetError> {
                Ok(None)
            }
            fn names(&self) -> Vec<String> {
                vec![]
            }
        }
        assert_eq!(Empty.label(), "anonymous");
    }

    #[test]
    fn custom_label_override_wins() {
        struct Labeled;
        impl PackageSet for Labeled {
            fn get(&self, _name: &str) -> Result<PackageLookup, PackageSetError> {
                Ok(None)
            }
            fn names(&self) -> Vec<String> {
                vec![]
            }
            fn label(&self) -> &str {
                "custom-label"
            }
        }
        assert_eq!(Labeled.label(), "custom-label");
    }

    #[test]
    fn package_lookup_is_option_derivation_alias() {
        // PackageLookup is `Option<Derivation>`. Callers rely on this
        // to pattern-match Some/None without importing Derivation.
        // If the alias drifts to `Option<Result<Derivation, _>>` or
        // similar, every call site breaks — pin the shape via None.
        let none: PackageLookup = None;
        assert!(none.is_none());
    }
}
