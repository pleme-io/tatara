//! `NixpkgsBridge` — expose any attribute in an already-installed nixpkgs as
//! a tatara `Derivation`. All heavy lifting (fetch, build, cache, store) stays
//! in Nix; we only own the typed name.

use tatara_nix::derivation::{BridgeTarget, Derivation};

use crate::set::{PackageLookup, PackageSet, PackageSetError};

/// Bridges a list of names through to an existing Nix expression universe.
/// By default that's `import <nixpkgs> {}`, but any expression that yields an
/// attribute set will do (flake revisions, release.nix, a private overlay).
pub struct NixpkgsBridge {
    /// Nix expression evaluating to the attribute root. Default: `"import <nixpkgs> {}"`.
    pub pkg_set: Option<String>,
    /// Names this bridge claims to know. Empty list = "open universe" — any
    /// name resolves. Non-empty list = closed universe (used for mirror gen).
    pub known_names: Vec<String>,
    /// Human label for logs.
    pub label: String,
}

impl Default for NixpkgsBridge {
    fn default() -> Self {
        Self {
            pkg_set: None,
            known_names: vec![],
            label: "nixpkgs-bridge".into(),
        }
    }
}

impl NixpkgsBridge {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pkg_set(mut self, expr: impl Into<String>) -> Self {
        self.pkg_set = Some(expr.into());
        self
    }

    pub fn with_names(mut self, names: Vec<String>) -> Self {
        self.known_names = names;
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Build a `Derivation` for a name without consulting `known_names`.
    /// Useful when you're sure the attr exists upstream.
    pub fn derivation(&self, attr_path: impl Into<String>) -> Derivation {
        let attr = attr_path.into();
        Derivation {
            name: attr.clone(),
            version: None,
            inputs: vec![],
            source: Default::default(),
            builder: Default::default(),
            outputs: Default::default(),
            env: vec![],
            sandbox: Default::default(),
            bridge: Some(BridgeTarget {
                attr_path: attr,
                pkg_set: self.pkg_set.clone(),
            }),
        }
    }
}

impl PackageSet for NixpkgsBridge {
    fn get(&self, name: &str) -> Result<PackageLookup, PackageSetError> {
        // Open universe: always yes.
        if self.known_names.is_empty() {
            return Ok(Some(self.derivation(name)));
        }
        // Closed universe: only names we've been given.
        if self.known_names.iter().any(|n| n == name) {
            Ok(Some(self.derivation(name)))
        } else {
            Ok(None)
        }
    }

    fn names(&self) -> Vec<String> {
        self.known_names.clone()
    }

    fn label(&self) -> &str {
        &self.label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_bridge_resolves_anything() {
        let b = NixpkgsBridge::new();
        let d = b.get("hello").unwrap().expect("should resolve");
        assert_eq!(d.name, "hello");
        assert!(d.bridge.is_some());
        assert_eq!(d.bridge.as_ref().unwrap().attr_path, "hello");
        assert!(d.bridge.as_ref().unwrap().pkg_set.is_none());
    }

    #[test]
    fn closed_bridge_only_resolves_known_names() {
        let b = NixpkgsBridge::new().with_names(vec!["hello".into(), "bash".into()]);
        assert!(b.get("hello").unwrap().is_some());
        assert!(b.get("nonexistent").unwrap().is_none());
        assert_eq!(b.names().len(), 2);
    }

    #[test]
    fn custom_pkg_set_carries_through() {
        let b = NixpkgsBridge::new().with_pkg_set("import ./release.nix {}");
        let d = b.get("mypkg").unwrap().unwrap();
        assert_eq!(
            d.bridge.as_ref().unwrap().pkg_set.as_deref(),
            Some("import ./release.nix {}")
        );
    }

    #[test]
    fn derivation_exposes_dotted_attr_path() {
        let b = NixpkgsBridge::new();
        let d = b.derivation("python3Packages.requests");
        assert_eq!(d.name, "python3Packages.requests");
        assert_eq!(d.bridge.as_ref().unwrap().attr_path, "python3Packages.requests");
    }
}
