/// Errors produced by tatara-kube operations.
#[derive(Debug, thiserror::Error)]
pub enum KubeError {
    #[error("nix eval failed for '{flake_ref}': {reason}")]
    NixEvalFailed { flake_ref: String, reason: String },

    #[error("nix eval timeout for '{flake_ref}' after {timeout_secs}s")]
    NixEvalTimeout {
        flake_ref: String,
        timeout_secs: u64,
    },

    #[error("flake metadata fetch failed for '{flake_ref}': {reason}")]
    MetadataFetchFailed { flake_ref: String, reason: String },

    #[error("kubernetes API error: {0}")]
    Kube(#[from] kube::Error),

    #[error("server-side apply failed for {kind}/{name}: {reason}")]
    ApplyFailed {
        kind: String,
        name: String,
        reason: String,
    },

    #[error("health check timeout for {kind}/{name} after {timeout_secs}s")]
    HealthCheckTimeout {
        kind: String,
        name: String,
        timeout_secs: u64,
    },

    #[error("resource parsing failed: {reason}")]
    ResourceParseFailed { reason: String },

    #[error("cluster '{name}' not reachable: {reason}")]
    ClusterUnreachable { name: String, reason: String },

    #[error("pruning failed for {kind}/{name}: {reason}")]
    PruneFailed {
        kind: String,
        name: String,
        reason: String,
    },

    #[error("api discovery failed for {api_version}/{kind}: {reason}")]
    DiscoveryFailed {
        api_version: String,
        kind: String,
        reason: String,
    },

    #[error("helm template failed for chart '{chart}': {reason}")]
    HelmTemplateFailed { chart: String, reason: String },

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
