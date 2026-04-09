use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// NixBuildPool CRD — defines a pool of tatara build nodes.
///
/// ```yaml
/// apiVersion: tatara.pleme.io/v1alpha1
/// kind: NixBuildPool
/// metadata:
///   name: default
/// spec:
///   minNodes: 0
///   maxNodes: 4
///   nodeGroupRef: scale-test-tatara-builders
///   systems: [x86_64-linux, aarch64-linux]
/// ```
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "tatara.pleme.io",
    version = "v1alpha1",
    kind = "NixBuildPool",
    namespaced,
    status = "NixBuildPoolStatus",
    printcolumn = r#"{"name":"Ready","type":"integer","jsonPath":".status.readyNodes"}"#,
    printcolumn = r#"{"name":"Active Builds","type":"integer","jsonPath":".status.activeBuilds"}"#
)]
pub struct NixBuildPoolSpec {
    /// Minimum number of tatara nodes (0 = scale to zero)
    #[serde(default)]
    pub min_nodes: u32,

    /// Maximum number of tatara nodes
    #[serde(default = "default_max_nodes")]
    pub max_nodes: u32,

    /// Reference to the EKS/ASG node group name
    #[serde(default)]
    pub node_group_ref: Option<String>,

    /// Supported build systems
    #[serde(default = "default_systems")]
    pub systems: Vec<String>,

    /// Idle timeout before scaling down (seconds)
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
}

fn default_max_nodes() -> u32 {
    4
}

fn default_systems() -> Vec<String> {
    vec!["x86_64-linux".to_string()]
}

fn default_idle_timeout() -> u64 {
    300
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct NixBuildPoolStatus {
    /// Number of ready tatara nodes
    #[serde(default)]
    pub ready_nodes: u32,

    /// Number of currently active builds
    #[serde(default)]
    pub active_builds: u32,

    /// Number of queued builds waiting
    #[serde(default)]
    pub queued_builds: u32,
}
