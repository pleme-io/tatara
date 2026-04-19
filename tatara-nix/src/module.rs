//! `Module` — NixOS's module system, typed.
//!
//! A module is a triple: `imports` + `options` + `config`. Modules compose via
//! fixpoint evaluation: every module contributes options (types + defaults)
//! and config values (merged via `mkIf` / `mkForce` / `mkMerge` semantics).
//!
//! The lattice-theoretic interpretation: each `MkExpr` is a point in a typed
//! config lattice. `mkMerge [a b]` is the join; `mkForce x` is the top-priority
//! assignment; `mkIf cond x` is conditional membership. This IS
//! `tatara-lattice`'s territory — we represent the operators typed so merge
//! semantics are proven by construction.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

/// A typed option — what a module says it accepts.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleOption {
    pub name: String,
    /// Shape of the value accepted. Mirrors Nix's `lib.types`.
    pub option_type: OptionType,
    /// Default value, if any. Stored as JSON for type-erased comparison.
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub description: Option<String>,
    /// If true, this option may only be assigned with `mkForce`.
    #[serde(default)]
    pub read_only: bool,
}

/// Concrete option type. Matches the most-used `lib.types` in nixpkgs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum OptionType {
    Bool,
    Int,
    Str,
    Float,
    Path,
    Package,
    ListOf {
        item: Box<OptionType>,
    },
    AttrsOf {
        value: Box<OptionType>,
    },
    Enum {
        choices: Vec<String>,
    },
    Submodule {
        options: Vec<ModuleOption>,
    },
    /// Escape hatch: anything serde can deserialize.
    Any,
}

/// An import of another module.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleImport {
    pub path: String,
    /// Arguments passed through at import time.
    #[serde(default)]
    pub args: BTreeMap<String, serde_json::Value>,
}

/// A lattice-typed config assignment — the `mkIf`/`mkForce`/`mkMerge` family.
/// Each variant is a typed operation; composition follows `tatara-lattice`
/// semantics.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum MkExpr {
    /// Unconditional value at normal priority.
    Set { value: serde_json::Value },
    /// Gate on condition — equivalent of `lib.mkIf`.
    If {
        condition: serde_json::Value,
        value: serde_json::Value,
    },
    /// Highest-priority override — equivalent of `lib.mkForce`.
    Force { value: serde_json::Value },
    /// Low-priority fallback — equivalent of `lib.mkDefault`. Applied only
    /// when nothing at normal-or-higher priority provides a value.
    Default { value: serde_json::Value },
    /// Merge multiple contributions — equivalent of `lib.mkMerge`.
    Merge { values: Vec<serde_json::Value> },
    /// Order hint: before/after named anchor — `lib.mkBefore`/`lib.mkAfter`.
    Order {
        placement: Placement,
        value: serde_json::Value,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Placement {
    Before,
    After,
}

/// A module — imports + options + config assignments.
///
/// ```lisp
/// (defmodule observability-stack
///   :imports  ((:path "./prometheus.lisp")
///              (:path "./grafana.lisp"))
///   :options  ((:name "enable"
///               :option-type (:kind Bool)
///               :default false)
///              (:name "retention-days"
///               :option-type (:kind Int)
///               :default 30))
///   :config   ((:path "services.prometheus.enable"
///               :expr (:op Set :value true))
///              (:path "services.prometheus.retention"
///               :expr (:op If
///                      :condition true
///                      :value "30d"))))
/// ```
#[derive(DeriveTataraDomain, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defmodule")]
pub struct Module {
    pub name: String,
    #[serde(default)]
    pub imports: Vec<ModuleImport>,
    #[serde(default)]
    pub options: Vec<ModuleOption>,
    #[serde(default)]
    pub config: Vec<ConfigAssignment>,
    #[serde(default)]
    pub description: Option<String>,
}

/// One typed assignment: a dotted option path + a `MkExpr`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigAssignment {
    /// Dotted path, e.g., `services.nginx.enable`.
    pub path: String,
    pub expr: MkExpr,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_lisp::{domain::TataraDomain, read};

    #[test]
    fn minimal_module_compiles() {
        let forms = read(
            r#"(defmodule
                  :name "stub"
                  :description "minimal module for tests")"#,
        )
        .unwrap();
        let m = Module::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(m.name, "stub");
        assert!(m.imports.is_empty());
        assert!(m.options.is_empty());
        assert!(m.config.is_empty());
    }

    #[test]
    fn module_with_options_and_config() {
        let forms = read(
            r#"(defmodule
                  :name "observability"
                  :options ((:name "enable" :option-type (:kind Bool) :default true)
                            (:name "retention-days" :option-type (:kind Int) :default 30))
                  :config  ((:path "services.prometheus.enable"
                             :expr (:op Set :value true))))"#,
        )
        .unwrap();
        let m = Module::compile_from_sexp(&forms[0]).unwrap();
        assert_eq!(m.options.len(), 2);
        assert!(matches!(m.options[0].option_type, OptionType::Bool));
        assert_eq!(m.config.len(), 1);
        assert_eq!(m.config[0].path, "services.prometheus.enable");
    }
}
