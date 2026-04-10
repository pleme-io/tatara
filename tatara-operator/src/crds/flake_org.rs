use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// FlakeOrg CRD — watches an entire GitHub organization, auto-discovers
/// repos with flake.nix, and creates FlakeSource CRs for each.
///
/// Non-flake repos are silently skipped. FlakeSource CRs are owned by the
/// FlakeOrg (garbage-collected on delete). Specific repos can be excluded.
///
/// ```yaml
/// apiVersion: tatara.pleme.io/v1alpha1
/// kind: FlakeOrg
/// metadata:
///   name: pleme-io
/// spec:
///   org: pleme-io
///   provider: github
///   poll_interval: 30m
///   auto_detect_flakes: true
///   skip_non_flake: true
///   default_system: x86_64-linux
///   default_attic_cache: main
///   exclude:
///     - ".github"
/// ```
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "tatara.pleme.io",
    version = "v1alpha1",
    kind = "FlakeOrg",
    namespaced,
    status = "FlakeOrgStatus",
    printcolumn = r#"{"name":"Org","type":"string","jsonPath":".spec.org"}"#,
    printcolumn = r#"{"name":"Provider","type":"string","jsonPath":".spec.provider"}"#,
    printcolumn = r#"{"name":"Repos","type":"integer","jsonPath":".status.discovered_repos"}"#,
    printcolumn = r#"{"name":"Flakes","type":"integer","jsonPath":".status.flake_repos"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
pub struct FlakeOrgSpec {
    /// GitHub organization name
    pub org: String,

    /// Git hosting provider (currently only "github")
    #[serde(default = "default_provider")]
    pub provider: String,

    /// How often to scan the org for new/removed repos
    #[serde(default = "default_poll_interval")]
    pub poll_interval: String,

    /// Whether to check each repo for flake.nix existence
    #[serde(default = "default_true")]
    pub auto_detect_flakes: bool,

    /// Skip repos that don't have flake.nix (no FlakeSource created)
    #[serde(default = "default_true")]
    pub skip_non_flake: bool,

    /// Default system for generated FlakeSource CRs
    #[serde(default = "default_system")]
    pub default_system: String,

    /// Default Attic cache name
    #[serde(default = "default_cache")]
    pub default_attic_cache: String,

    /// Default branch to watch
    #[serde(default = "default_branch")]
    pub default_branch: String,

    /// Default poll interval for generated FlakeSource CRs
    #[serde(default = "default_source_poll_interval")]
    pub default_source_poll_interval: String,

    /// Repos to exclude (exact match on repo name)
    #[serde(default)]
    pub exclude: Vec<String>,

    /// If set, only include these repos (overrides exclude)
    #[serde(default)]
    pub include: Option<Vec<String>>,

    /// Extra nix build args for all generated FlakeSource CRs
    #[serde(default)]
    pub extra_args: Vec<String>,
}

fn default_provider() -> String {
    "github".to_string()
}
fn default_poll_interval() -> String {
    "30m".to_string()
}
fn default_true() -> bool {
    true
}
fn default_system() -> String {
    "x86_64-linux".to_string()
}
fn default_cache() -> String {
    "main".to_string()
}
fn default_branch() -> String {
    "main".to_string()
}
fn default_source_poll_interval() -> String {
    "10m".to_string()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct FlakeOrgStatus {
    /// When the org was last scanned
    #[serde(default)]
    pub last_scanned: Option<DateTime<Utc>>,

    /// Total repos discovered in the org
    #[serde(default)]
    pub discovered_repos: u32,

    /// Repos that have flake.nix
    #[serde(default)]
    pub flake_repos: u32,

    /// Repos skipped (no flake.nix or excluded)
    #[serde(default)]
    pub skipped_repos: u32,

    /// Per-repo discovery status
    #[serde(default)]
    pub repo_statuses: Vec<OrgRepoStatus>,

    /// Any error from the last scan
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct OrgRepoStatus {
    /// Repository name
    pub repo: String,

    /// Whether flake.nix was found
    pub has_flake: bool,

    /// Name of the generated FlakeSource CR (if any)
    #[serde(default)]
    pub flake_source_ref: Option<String>,

    /// When this repo was last checked
    #[serde(default)]
    pub last_checked: Option<DateTime<Utc>>,
}
