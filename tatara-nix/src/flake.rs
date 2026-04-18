//! `Flake` — hermetic inputs + outputs, typed.
//!
//! A flake is Nix's hermeticity unit: declared inputs (other flakes, pinned
//! by hash in a lockfile), declared outputs (derivations, modules, overlays,
//! apps), deterministic composition. We express it as a typed Rust record
//! with a Lisp authoring surface.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

use crate::derivation::Derivation;
use crate::module::Module;
use crate::overlay::Overlay;

/// One input to a flake — another flake identified by URL + optional pin.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlakeInput {
    pub name: String,
    pub url: String,
    /// Concrete hash of the input — populated at lock time.
    #[serde(default)]
    pub locked_hash: Option<String>,
    /// Inputs of this input that should follow other inputs in the parent.
    #[serde(default)]
    pub follows: BTreeMap<String, String>,
}

/// Outputs of a flake — named bundles of typed artifacts, keyed by system.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlakeOutputs {
    /// `packages.<system>.<name> → Derivation`.
    #[serde(default)]
    pub packages: BTreeMap<String, Vec<Derivation>>,
    /// `modules.<name> → Module`.
    #[serde(default)]
    pub modules: Vec<Module>,
    /// `overlays.<name> → Overlay`.
    #[serde(default)]
    pub overlays: Vec<Overlay>,
    /// `apps.<system>.<name>` — executable aliases keyed by system.
    #[serde(default)]
    pub apps: BTreeMap<String, Vec<App>>,
    /// `checks.<system>.<name>` — build checks gated by system.
    #[serde(default)]
    pub checks: BTreeMap<String, Vec<Derivation>>,
}

/// A flake app — a named executable reference.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    pub name: String,
    /// Either a derivation that produces a binary, or a string path.
    pub program: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// A flake — hermetic input/output declaration.
///
/// ```lisp
/// (defflake my-system
///   :description "tatara programmable convergence computer"
///   :inputs   ((:name "nixpkgs" :url "github:NixOS/nixpkgs/25.11")
///              (:name "substrate"
///               :url "github:pleme-io/substrate"
///               :follows (:nixpkgs "nixpkgs")))
///   :outputs  (:packages    (:x86_64-linux ((:name "tatara-reconciler")))
///              :apps        (:x86_64-linux ((:name "release-reconciler"
///                                            :program ".#release-reconciler")))
///              :checks      (:x86_64-linux ((:name "helm-lint")))))
/// ```
#[derive(DeriveTataraDomain, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defflake")]
pub struct Flake {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub inputs: Vec<FlakeInput>,
    #[serde(default)]
    pub outputs: FlakeOutputs,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_lisp::{domain::TataraDomain, read};

    #[test]
    fn minimal_flake_compiles() {
        let forms = read(
            r#"(defflake
                  :name "my-system"
                  :description "tatara programmable convergence computer"
                  :inputs ((:name "nixpkgs" :url "github:NixOS/nixpkgs/25.11")))"#,
        )
        .unwrap();
        let f = Flake::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(f.name, "my-system");
        assert_eq!(f.inputs.len(), 1);
        assert_eq!(f.inputs[0].name, "nixpkgs");
    }
}
