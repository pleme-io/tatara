use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Labels and annotations applied to every managed resource.
pub const LABEL_MANAGED_BY: &str = "tatara.pleme.io/managed-by";
pub const LABEL_MANAGED_BY_VALUE: &str = "tatara-kube";
pub const LABEL_CLUSTER: &str = "tatara.pleme.io/cluster";
pub const LABEL_GENERATION: &str = "tatara.pleme.io/generation";
pub const ANNOTATION_CONTENT_HASH: &str = "tatara.pleme.io/content-hash";
pub const ANNOTATION_APPLIED_AT: &str = "tatara.pleme.io/applied-at";

/// Unique identity for a Kubernetes resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceIdentity {
    pub api_version: String,
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
}

impl std::fmt::Display for ResourceIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ns) = &self.namespace {
            write!(f, "{}/{}/{}", ns, self.kind, self.name)
        } else {
            write!(f, "{}/{}", self.kind, self.name)
        }
    }
}

/// A Kubernetes resource deserialized from Nix eval output.
#[derive(Debug, Clone)]
pub struct ManagedResource {
    /// The full K8s resource as a JSON value.
    pub manifest: serde_json::Value,

    /// Extracted identity for fast lookups.
    pub identity: ResourceIdentity,

    /// SHA-256 hash of the canonical JSON manifest.
    pub content_hash: String,
}

impl ManagedResource {
    /// Parse a JSON value into a `ManagedResource`.
    pub fn from_value(value: serde_json::Value) -> Result<Self, crate::KubeError> {
        let api_version = value["apiVersion"]
            .as_str()
            .ok_or_else(|| crate::KubeError::ResourceParseFailed {
                reason: "missing apiVersion".to_string(),
            })?
            .to_string();

        let kind = value["kind"]
            .as_str()
            .ok_or_else(|| crate::KubeError::ResourceParseFailed {
                reason: "missing kind".to_string(),
            })?
            .to_string();

        let name = value["metadata"]["name"]
            .as_str()
            .ok_or_else(|| crate::KubeError::ResourceParseFailed {
                reason: "missing metadata.name".to_string(),
            })?
            .to_string();

        let namespace = value["metadata"]["namespace"].as_str().map(String::from);

        let identity = ResourceIdentity {
            api_version,
            kind,
            namespace,
            name,
        };

        let canonical = serde_json::to_string(&value).unwrap_or_default();
        let content_hash = format!("{:x}", Sha256::digest(canonical.as_bytes()));

        Ok(Self {
            manifest: value,
            identity,
            content_hash,
        })
    }
}

/// The full desired state for a cluster, as evaluated from Nix.
#[derive(Debug, Clone)]
pub struct DesiredState {
    /// All resources to apply, in dependency order.
    pub resources: Vec<ManagedResource>,

    /// Generation hash — SHA-256 of the entire Nix eval output.
    pub generation_hash: String,

    /// Source flake revision (git commit).
    pub source_rev: Option<String>,
}

/// Result of comparing desired state to cluster state.
#[derive(Debug, Clone, Default)]
pub struct ReconcilePlan {
    /// Resources to create or update.
    pub to_apply: Vec<ManagedResource>,

    /// Resources to prune (present in cluster with our labels, not in desired state).
    pub to_prune: Vec<ResourceIdentity>,

    /// Resources unchanged.
    pub unchanged: Vec<ResourceIdentity>,
}

impl ReconcilePlan {
    pub fn summary(&self) -> String {
        format!(
            "apply={} prune={} unchanged={}",
            self.to_apply.len(),
            self.to_prune.len(),
            self.unchanged.len()
        )
    }
}
