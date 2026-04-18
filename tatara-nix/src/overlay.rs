//! `Overlay` — nixpkgs's `self: super: …` pattern, typed.
//!
//! An overlay extends or replaces definitions in a package set. In Nix, it's
//! an anonymous function; here it's a named typed record of changes. Composing
//! overlays is the lattice join at the package-set level.

use serde::{Deserialize, Serialize};
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

use crate::derivation::Derivation;

/// What an overlay targets — the scope of its mutations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OverlayTarget {
    /// Top-level packages (`pkgs.foo`).
    #[default]
    PackageSet,
    /// Per-system subset (`pkgs.aarch64-linux.foo`).
    PerSystem,
    /// A specific module's options.
    Module,
}

/// An overlay — a named bundle of additions + replacements.
///
/// ```lisp
/// (defoverlay add-gnu-patches
///   :target PackageSet
///   :adds   ((:name "hello-enhanced" :version "2.12.1-plus-patches"))
///   :replaces (("hello" (:name "hello" :version "2.12.1-ga-patched"))))
/// ```
#[derive(DeriveTataraDomain, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defoverlay")]
pub struct Overlay {
    pub name: String,
    #[serde(default)]
    pub target: OverlayTarget,
    /// Brand-new packages introduced by this overlay.
    #[serde(default)]
    pub adds: Vec<Derivation>,
    /// Replacements: each entry is `(upstream-name, new-derivation)`.
    #[serde(default)]
    pub replaces: Vec<Replacement>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Replace an existing package's derivation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Replacement {
    pub upstream_name: String,
    pub with: Derivation,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_lisp::{domain::TataraDomain, read};

    #[test]
    fn minimal_overlay_compiles() {
        let forms = read(
            r#"(defoverlay
                  :name "patched"
                  :target PackageSet
                  :description "carries a local patch")"#,
        )
        .unwrap();
        let o = Overlay::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(o.name, "patched");
        assert_eq!(o.target, OverlayTarget::PackageSet);
        assert!(o.adds.is_empty());
    }
}
