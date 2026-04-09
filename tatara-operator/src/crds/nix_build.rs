use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// NixBuild CRD — declarative Nix build request.
///
/// ```yaml
/// apiVersion: tatara.pleme.io/v1alpha1
/// kind: NixBuild
/// metadata:
///   name: akeyless-auth
/// spec:
///   flakeRef: "github:pleme-io/blackmatter-akeyless#akeyless-backend-auth"
///   system: x86_64-linux
///   atticCache: main
/// ```
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "tatara.pleme.io",
    version = "v1alpha1",
    kind = "NixBuild",
    namespaced,
    status = "NixBuildStatus",
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#,
    printcolumn = r#"{"name":"Store Path","type":"string","jsonPath":".status.storePath"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
pub struct NixBuildSpec {
    /// Nix flake reference (e.g., "github:pleme-io/repo#package" or ".#package")
    pub flake_ref: String,

    /// Target system (e.g., "x86_64-linux", "aarch64-linux")
    #[serde(default = "default_system")]
    pub system: String,

    /// Attic cache name to push results to
    #[serde(default)]
    pub attic_cache: Option<String>,

    /// Additional nix build arguments (e.g., ["--impure"])
    #[serde(default)]
    pub extra_args: Vec<String>,

    /// Priority (higher = built first). Default: 0
    #[serde(default)]
    pub priority: i32,
}

fn default_system() -> String {
    "x86_64-linux".to_string()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct NixBuildStatus {
    /// Current phase of the build
    #[serde(default)]
    pub phase: NixBuildPhase,

    /// Unique build ID (maps to NATS message)
    #[serde(default)]
    pub build_id: Option<String>,

    /// Nix store output path (set on completion)
    #[serde(default)]
    pub store_path: Option<String>,

    /// Node that executed the build
    #[serde(default)]
    pub builder_node: Option<String>,

    /// When the build started
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,

    /// When the build completed
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,

    /// Error message (set on failure)
    #[serde(default)]
    pub error: Option<String>,

    /// Build log reference (e.g., log stream URL)
    #[serde(default)]
    pub log_ref: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq)]
pub enum NixBuildPhase {
    #[default]
    Pending,
    Queued,
    Building,
    Pushing,
    Complete,
    Failed,
}
