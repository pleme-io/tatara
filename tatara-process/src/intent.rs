//! Intent — where the rendered artifacts come from.
//!
//! Exactly one field on `Intent` must be set. The reconciler's RENDER phase
//! selects a driver based on which variant is present:
//!   - `nix`:       tatara-engine `nix_eval` → resources
//!   - `flux`:      pass through an existing `GitRepository`
//!   - `lisp`:      tatara-lisp reader + macroexpander → resources
//!   - `container`: emit Deployment/StatefulSet/etc directly (no Helm)

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Intent — exactly one variant should be populated.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nix: Option<NixIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flux: Option<FluxIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lisp: Option<LispIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<ContainerIntent>,
}

/// Enum view over the populated variant — convenience for the reconciler.
#[derive(Clone, Debug)]
pub enum IntentVariant<'a> {
    Nix(&'a NixIntent),
    Flux(&'a FluxIntent),
    Lisp(&'a LispIntent),
    Container(&'a ContainerIntent),
}

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum IntentError {
    #[error("intent has no variant set (one of nix/flux/lisp/container required)")]
    Empty,
    #[error("intent has multiple variants set; exactly one required")]
    Ambiguous,
}

impl Intent {
    /// Resolve to exactly one variant. Errors on zero or many.
    pub fn variant(&self) -> Result<IntentVariant<'_>, IntentError> {
        let count = [
            self.nix.is_some(),
            self.flux.is_some(),
            self.lisp.is_some(),
            self.container.is_some(),
        ]
        .into_iter()
        .filter(|b| *b)
        .count();
        match count {
            0 => Err(IntentError::Empty),
            1 => Ok(if let Some(n) = &self.nix {
                IntentVariant::Nix(n)
            } else if let Some(f) = &self.flux {
                IntentVariant::Flux(f)
            } else if let Some(l) = &self.lisp {
                IntentVariant::Lisp(l)
            } else if let Some(c) = &self.container {
                IntentVariant::Container(c)
            } else {
                unreachable!()
            }),
            _ => Err(IntentError::Ambiguous),
        }
    }
}

/// Nix-sourced intent — tatara-engine's nix_eval driver produces resources.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NixIntent {
    /// Flake reference, e.g., `github:pleme-io/k8s?dir=shared/infrastructure`.
    pub flake_ref: String,
    /// Attribute path within the flake (e.g., `observability`).
    pub attribute: String,
    /// Target system. Defaults to the controller host's system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Attic cache to push the resulting store path into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attic_cache: Option<String>,
    /// Additional `nix build` arguments (e.g., `["--impure"]`).
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// Delegate the actual build to a sibling NixBuild CRD
    /// (bridges to tatara-operator NATS bare-metal builder path).
    #[serde(default)]
    pub delegate_to_nix_build: bool,
}

/// FluxCD passthrough intent — reuse an existing GitRepository.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FluxIntent {
    /// Name of an existing `GitRepository` (typically in `flux-system`).
    pub git_repository: String,
    /// Path inside the repository that the Kustomization will apply.
    pub path: String,
    /// Optional namespace of the GitRepository CR (defaults to `flux-system`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_repository_namespace: Option<String>,
    /// Optional target namespace for the emitted Kustomization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_namespace: Option<String>,
    /// SOPS decryption — defaults to true to match pleme-io conventions.
    #[serde(default = "default_true")]
    pub decrypt_sops: bool,
    /// If set, additionally emit a HelmRelease for this chart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helm_chart: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helm_values: Option<BTreeMap<String, serde_json::Value>>,
}

fn default_true() -> bool {
    true
}

/// Lisp-sourced intent — tatara-lisp reader + macroexpander produces resources.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LispIntent {
    /// Raw S-expression source, OR `include:<path>` / `configmap:<name>/<key>` pointer.
    pub source: String,
    /// Reader dialect / version tag.
    #[serde(default = "default_reader")]
    pub reader: String,
    /// Macro form version.
    #[serde(default = "default_version")]
    pub version: String,
    /// Symbols injected into the reader env (e.g., `cluster`, `region`).
    #[serde(default)]
    pub bindings: BTreeMap<String, serde_json::Value>,
}

fn default_reader() -> String {
    "tatara-lisp".to_string()
}
fn default_version() -> String {
    "v1".to_string()
}

/// Container intent — direct Deployment/StatefulSet/etc, no Helm.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContainerIntent {
    pub image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<i32>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub workload_kind: WorkloadKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
pub enum WorkloadKind {
    #[default]
    Deployment,
    StatefulSet,
    DaemonSet,
    Job,
    CronJob,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_intent_errors() {
        let i = Intent::default();
        assert_eq!(i.variant().unwrap_err(), IntentError::Empty);
    }

    #[test]
    fn exactly_one_ok() {
        let i = Intent {
            nix: Some(NixIntent {
                flake_ref: "github:a/b".into(),
                attribute: "x".into(),
                system: None,
                attic_cache: None,
                extra_args: vec![],
                delegate_to_nix_build: false,
            }),
            ..Intent::default()
        };
        assert!(matches!(i.variant().unwrap(), IntentVariant::Nix(_)));
    }

    #[test]
    fn two_variants_ambiguous() {
        let i = Intent {
            nix: Some(NixIntent {
                flake_ref: "a".into(),
                attribute: "b".into(),
                system: None,
                attic_cache: None,
                extra_args: vec![],
                delegate_to_nix_build: false,
            }),
            flux: Some(FluxIntent {
                git_repository: "g".into(),
                path: "p".into(),
                git_repository_namespace: None,
                target_namespace: None,
                decrypt_sops: true,
                helm_chart: None,
                helm_values: None,
            }),
            ..Intent::default()
        };
        assert_eq!(i.variant().unwrap_err(), IntentError::Ambiguous);
    }
}
