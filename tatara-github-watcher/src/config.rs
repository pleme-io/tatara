//! Typed runtime config for the watcher.

use clap::Parser;

/// Typed watcher config — CLI flags + env-var overrides.
#[derive(Parser, Debug, Clone)]
#[command(name = "tatara-github-watcher")]
pub struct WatcherConfig {
    /// HTTP listen address (host:port). The webhook URL operators
    /// configure in GitHub points here.
    #[arg(long, env = "TATARA_WATCHER_LISTEN", default_value = "0.0.0.0:8080")]
    pub listen: String,

    /// GitHub webhook secret. Operator-supplied; must match what
    /// they set in the GitHub org webhook config.
    #[arg(long, env = "TATARA_WATCHER_SECRET")]
    pub secret: String,

    /// Namespace where EphemeralAllocation CRs are created. Defaults
    /// to `"ephemeral-pools"`.
    #[arg(long, env = "TATARA_WATCHER_NAMESPACE", default_value = "ephemeral-pools")]
    pub namespace: String,

    /// Pin all allocations to a named pool (skips selector routing).
    /// Useful for single-pool deployments.
    #[arg(long, env = "TATARA_WATCHER_PIN_POOL")]
    pub pin_pool: Option<String>,

    /// Whether draft PRs receive allocations. Default: false.
    #[arg(long, env = "TATARA_WATCHER_INCLUDE_DRAFTS")]
    pub include_drafts: bool,

    /// If set, restrict the watcher to PRs from this allowlist of
    /// `org/*` or `org/repo` patterns (comma-separated). Empty = accept
    /// every repo.
    #[arg(long, env = "TATARA_WATCHER_ALLOW_REPOS", value_delimiter = ',')]
    pub allow_repos: Vec<String>,
}
