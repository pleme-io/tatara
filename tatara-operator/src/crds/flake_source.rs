use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// FlakeSource CRD — declares a Git repo with Nix flake outputs to watch and auto-build.
///
/// The operator polls the repo for new commits on the specified branch. When a change
/// is detected, it creates NixBuild CRs for each declared output, which triggers
/// builds on tatara nodes and populates the Attic cache.
///
/// Clients never trigger builds — the cache is always warm.
///
/// ```yaml
/// apiVersion: tatara.pleme.io/v1alpha1
/// kind: FlakeSource
/// metadata:
///   name: blackmatter-akeyless
/// spec:
///   repo: github:pleme-io/blackmatter-akeyless
///   branch: main
///   pollInterval: 5m
///   outputs:
///     - attr: packages.x86_64-linux.akeyless-backend-auth
///       system: x86_64-linux
///     - attr: packages.x86_64-linux.akeyless-backend-kfm
///       system: x86_64-linux
///   atticCache: main
/// ```
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "tatara.pleme.io",
    version = "v1alpha1",
    kind = "FlakeSource",
    namespaced,
    status = "FlakeSourceStatus",
    printcolumn = r#"{"name":"Repo","type":"string","jsonPath":".spec.repo"}"#,
    printcolumn = r#"{"name":"Branch","type":"string","jsonPath":".spec.branch"}"#,
    printcolumn = r#"{"name":"Last Commit","type":"string","jsonPath":".status.lastCommit"}"#,
    printcolumn = r#"{"name":"Outputs","type":"integer","jsonPath":".status.cachedOutputs"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
pub struct FlakeSourceSpec {
    /// Git repository reference (e.g., "github:pleme-io/blackmatter-akeyless")
    pub repo: String,

    /// Branch to watch (default: main)
    #[serde(default = "default_branch")]
    pub branch: String,

    /// How often to poll for changes as fallback (default: 5m).
    /// Primary trigger is GitHub webhooks — polling is backup only.
    #[serde(default = "default_poll_interval")]
    pub poll_interval: String,

    /// GitHub webhook secret for validating push events.
    /// When set, the operator exposes a webhook endpoint at
    /// /webhooks/github/{namespace}/{name} that GitHub calls on push.
    /// This eliminates polling — builds fire within seconds of a commit.
    #[serde(default)]
    pub webhook_secret_ref: Option<WebhookSecretRef>,

    /// Flake outputs to build and cache on each change
    pub outputs: Vec<FlakeOutput>,

    /// Attic cache name to push results to
    #[serde(default = "default_cache")]
    pub attic_cache: String,

    /// Whether to also build on initial creation (not just on changes)
    #[serde(default = "default_true")]
    pub build_on_create: bool,

    /// Extra nix build args applied to all outputs (e.g., ["--impure"])
    #[serde(default)]
    pub extra_args: Vec<String>,
}

/// A specific flake output to build.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct FlakeOutput {
    /// Flake attribute path (e.g., "packages.x86_64-linux.akeyless-backend-auth")
    pub attr: String,

    /// Target system (e.g., "x86_64-linux")
    #[serde(default = "default_system")]
    pub system: String,

    /// Extra args for this specific output
    #[serde(default)]
    pub extra_args: Vec<String>,
}

fn default_branch() -> String {
    "main".to_string()
}

fn default_poll_interval() -> String {
    "5m".to_string()
}

fn default_cache() -> String {
    "main".to_string()
}

fn default_system() -> String {
    "x86_64-linux".to_string()
}

fn default_true() -> bool {
    true
}

/// Reference to a K8s Secret containing the GitHub webhook secret.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct WebhookSecretRef {
    /// Secret name in the same namespace
    pub name: String,
    /// Key in the secret containing the webhook secret string
    #[serde(default = "default_webhook_key")]
    pub key: String,
}

fn default_webhook_key() -> String {
    "webhook-secret".to_string()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct FlakeSourceStatus {
    /// When the repo was last polled
    #[serde(default)]
    pub last_polled: Option<DateTime<Utc>>,

    /// Latest commit SHA on the watched branch
    #[serde(default)]
    pub last_commit: Option<String>,

    /// Previous commit SHA (before the current one)
    #[serde(default)]
    pub previous_commit: Option<String>,

    /// Number of outputs currently cached in Attic
    #[serde(default)]
    pub cached_outputs: u32,

    /// Total number of declared outputs
    #[serde(default)]
    pub total_outputs: u32,

    /// Per-output build status
    #[serde(default)]
    pub output_statuses: Vec<OutputStatus>,

    /// Any error from the last poll
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct OutputStatus {
    /// Flake attribute path
    pub attr: String,

    /// Current state
    pub state: OutputState,

    /// Nix store path (if built)
    #[serde(default)]
    pub store_path: Option<String>,

    /// Reference to the NixBuild CR name (if building)
    #[serde(default)]
    pub build_ref: Option<String>,

    /// When this output was last built
    #[serde(default)]
    pub last_built: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
pub enum OutputState {
    /// Not yet built for the current commit
    Pending,
    /// NixBuild CR created, waiting for completion
    Building,
    /// Successfully built and cached in Attic
    Cached,
    /// Build failed
    Failed,
}
