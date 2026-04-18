//! `Derivation` — Nix's canonical build primitive, typed.
//!
//! A derivation is a sealed description: declared inputs, declared source,
//! declared build steps, declared outputs. Evaluating it (sui's job) produces
//! an actual store path. Two derivations with identical canonical JSON produce
//! identical store paths — that's the content-addressing guarantee.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

use crate::store::{StoreHash, StorePath};

/// A reference to another derivation, either by name (to be resolved) or by a
/// concrete store path.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputRef {
    pub name: String,
    /// Semantic version constraint, e.g., `"^1.2.0"` or `"=2.12.1"`.
    #[serde(default)]
    pub version: Option<String>,
    /// Concrete pinned store path, if already resolved.
    #[serde(default)]
    pub pinned: Option<StorePath>,
}

/// Where the source comes from — the only part of a derivation that reaches
/// outside the typed world. Hashed into StoreHash so changes to source =
/// changes to output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Source {
    /// Inline literal — e.g., a small text file.
    Inline { content: String },
    /// Local path relative to the Lisp source file.
    Path { path: String },
    /// Git reference with rev pinning.
    Git {
        url: String,
        rev: String,
        #[serde(default)]
        submodules: bool,
    },
    /// Tarball URL with hash pinning.
    Tarball { url: String, hash: String },
    /// Another derivation's output.
    Derivation { input: InputRef },
}

impl Default for Source {
    fn default() -> Self {
        Self::Inline {
            content: String::new(),
        }
    }
}

/// One phase in the canonical Nix build flow. Maps to stdenv phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuilderPhase {
    Unpack,
    Patch,
    Configure,
    Build,
    Check,
    Install,
    Fixup,
    InstallCheck,
    Dist,
}

/// Ordered phase list — the builder executes them sequentially in a sandbox.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderPhases {
    #[serde(default)]
    pub phases: Vec<BuilderPhase>,
    /// Per-phase shell commands — indexed by phase name.
    #[serde(default)]
    pub commands: BTreeMap<String, Vec<String>>,
}

/// Declared outputs — every derivation has at least a primary output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outputs {
    /// Output name for the primary artifact. Default: `"out"`.
    #[serde(default = "default_primary")]
    pub primary: String,
    /// Additional named outputs — e.g., `doc`, `dev`, `lib`, `bin`.
    #[serde(default)]
    pub extra: Vec<String>,
}

impl Default for Outputs {
    fn default() -> Self {
        Self {
            primary: default_primary(),
            extra: Vec::new(),
        }
    }
}

fn default_primary() -> String {
    "out".to_string()
}

/// One environment variable key → value binding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

/// A Derivation — the canonical unit of build.
///
/// ```lisp
/// (defderivation hello
///   :version    "2.12.1"
///   :inputs     ((:name "gcc" :version "^13")
///                (:name "glibc"))
///   :source     (:kind Git
///                :url "https://github.com/gnu/hello.git"
///                :rev "v2.12.1")
///   :builder    (:phases (Unpack Configure Build Install))
///   :outputs    (:primary "out" :extra ("doc"))
///   :env        ((:name "CFLAGS" :value "-O2")))
/// ```
#[derive(DeriveTataraDomain, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defderivation")]
pub struct Derivation {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub inputs: Vec<InputRef>,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub builder: BuilderPhases,
    #[serde(default)]
    pub outputs: Outputs,
    #[serde(default)]
    pub env: Vec<EnvVar>,
    /// Build sandbox controls — what the builder can see.
    #[serde(default)]
    pub sandbox: Sandbox,
}

/// Hermeticity controls. Nix guarantees these implicitly; we name them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sandbox {
    /// Network access permitted. Default: false (fully hermetic).
    #[serde(default)]
    pub allow_network: bool,
    /// Extra paths to make available in the sandbox.
    #[serde(default)]
    pub extra_paths: Vec<String>,
    /// Impure env vars to pass through (discouraged; escape hatch).
    #[serde(default)]
    pub impure_env: Vec<String>,
}

impl Derivation {
    /// Compute the content-addressed store path of this derivation.
    /// Deterministic — identical Rust value → identical path.
    pub fn store_path(&self) -> StorePath {
        StorePath::new(
            StoreHash::of(self),
            self.name.clone(),
            self.version.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_lisp::{domain::TataraDomain, read};

    #[test]
    fn derivation_store_path_is_deterministic() {
        let d1 = Derivation {
            name: "hello".into(),
            version: Some("2.12.1".into()),
            ..default_derivation()
        };
        let d2 = Derivation {
            name: "hello".into(),
            version: Some("2.12.1".into()),
            ..default_derivation()
        };
        assert_eq!(d1.store_path(), d2.store_path());
    }

    #[test]
    fn derivation_store_path_varies_with_version() {
        let d1 = Derivation {
            name: "hello".into(),
            version: Some("2.12.1".into()),
            ..default_derivation()
        };
        let d2 = Derivation {
            name: "hello".into(),
            version: Some("2.12.2".into()),
            ..default_derivation()
        };
        assert_ne!(d1.store_path(), d2.store_path());
    }

    #[test]
    fn minimal_derivation_compiles_from_lisp() {
        let forms = read(
            r#"(defderivation
                  :name "hello"
                  :version "2.12.1")"#,
        )
        .unwrap();
        let d = Derivation::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(d.name, "hello");
        assert_eq!(d.version.as_deref(), Some("2.12.1"));
        assert_eq!(d.outputs.primary, "out");
    }

    #[test]
    fn full_derivation_compiles_from_lisp() {
        let forms = read(
            r#"(defderivation
                  :name "hello"
                  :version "2.12.1"
                  :inputs ((:name "gcc" :version "^13")
                           (:name "glibc"))
                  :source (:kind Git
                           :url "https://github.com/gnu/hello.git"
                           :rev "v2.12.1")
                  :builder (:phases (Unpack Configure Build Install))
                  :outputs (:primary "out" :extra ("doc"))
                  :env ((:name "CFLAGS" :value "-O2")))"#,
        )
        .unwrap();
        let d = Derivation::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(d.inputs.len(), 2);
        assert!(matches!(&d.source, Source::Git { .. }));
        assert_eq!(d.builder.phases.len(), 4);
        assert_eq!(d.outputs.extra, vec!["doc".to_string()]);
        assert_eq!(d.env[0].name, "CFLAGS");
    }

    fn default_derivation() -> Derivation {
        Derivation {
            name: String::new(),
            version: None,
            inputs: vec![],
            source: Source::default(),
            builder: BuilderPhases::default(),
            outputs: Outputs::default(),
            env: vec![],
            sandbox: Sandbox::default(),
        }
    }
}
