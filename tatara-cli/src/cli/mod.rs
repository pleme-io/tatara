pub mod alloc;
pub mod context;
pub mod deploy;
pub mod event;
pub mod forge;
pub mod job;
pub mod node;
pub mod output;
pub mod release;
pub mod source;
pub mod top;

use clap::{Parser, Subcommand};
use output::OutputFormat;

#[derive(Parser)]
#[command(name = "tatara", about = "Nix-native workload orchestrator", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output format
    #[arg(long, short, global = true, default_value = "table")]
    pub output: OutputFormat,

    /// Server endpoint (overrides active context)
    #[arg(long, global = true)]
    pub endpoint: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Manage jobs
    Job {
        #[command(subcommand)]
        command: JobCmd,
    },

    /// Manage nodes
    Node {
        #[command(subcommand)]
        command: NodeCmd,
    },

    /// Manage allocations
    Alloc {
        #[command(subcommand)]
        command: AllocCmd,
    },

    /// Evaluate a Nix job spec to JSON
    Eval {
        /// Nix file to evaluate
        nix_file: String,
    },

    /// Deploy a flake-based workload
    Deploy {
        /// Flake reference or local path
        flake_ref: String,

        /// Override values (key=value)
        #[arg(long = "set", value_parser = parse_key_val)]
        set_values: Vec<(String, String)>,

        /// Job name override
        #[arg(long)]
        name: Option<String>,

        /// Show plan without deploying
        #[arg(long)]
        dry_run: bool,
    },

    /// Run the scheduler + API server
    Server {
        /// Path to server config file
        #[arg(long)]
        config: Option<String>,
    },

    /// Run a node executor (connects to remote server)
    Client {
        /// Server gRPC address
        #[arg(long)]
        server: Option<String>,
        /// Path to client config file
        #[arg(long)]
        config: Option<String>,
    },

    /// Real-time cluster resource dashboard
    Top {
        /// Filter to specific node
        #[arg(long)]
        node: Option<String>,

        /// Refresh interval in seconds
        #[arg(long, default_value = "2")]
        refresh: u64,
    },

    /// Manage contexts (server endpoints)
    Context {
        #[command(subcommand)]
        command: ContextCmd,
    },

    /// View cluster events
    Event {
        #[command(subcommand)]
        command: EventCmd,
    },

    /// Manage releases
    Release {
        #[command(subcommand)]
        command: ReleaseCmd,
    },

    /// Manage forges (Nix workload packages)
    Forge {
        #[command(subcommand)]
        command: ForgeCmd,
    },

    /// Manage sources (GitOps flake watchers)
    Source {
        #[command(subcommand)]
        command: SourceCmd,
    },

    // ── Backwards-compatible aliases ──
    /// Submit a job (alias for `job run`)
    #[command(hide = true)]
    Run {
        /// Job file (JSON, YAML, or Nix with --eval)
        job_file: String,
        /// Evaluate as Nix expression
        #[arg(long)]
        eval: bool,
        /// Server HTTP address
        #[arg(long, default_value = "127.0.0.1:4646")]
        server: String,
    },

    /// Show job/node status (alias for `job list` or `job get`)
    #[command(hide = true)]
    Status {
        /// Job ID (omit to list all jobs)
        job_id: Option<String>,
        /// Server HTTP address
        #[arg(long, default_value = "127.0.0.1:4646")]
        server: String,
    },

    /// Stop a job (alias for `job stop`)
    #[command(hide = true)]
    Stop {
        /// Job ID
        job_id: String,
        /// Server HTTP address
        #[arg(long, default_value = "127.0.0.1:4646")]
        server: String,
    },

    /// Stream task logs (alias for `alloc logs`)
    #[command(hide = true)]
    Logs {
        /// Allocation ID
        alloc_id: String,
        /// Task name
        #[arg(long)]
        task: Option<String>,
        /// Follow log output
        #[arg(long)]
        follow: bool,
        /// Server HTTP address
        #[arg(long, default_value = "127.0.0.1:4646")]
        server: String,
    },
}

#[derive(Subcommand)]
pub enum JobCmd {
    /// List all jobs
    List,
    /// Get job details
    Get {
        /// Job ID
        job_id: String,
    },
    /// Submit a job
    Run {
        /// Job file (JSON, YAML, or Nix with --eval)
        job_file: String,
        /// Evaluate as Nix expression
        #[arg(long)]
        eval: bool,
    },
    /// Stop a job
    Stop {
        /// Job ID
        job_id: String,
    },
    /// Show job version history
    History {
        /// Job ID
        job_id: String,
    },
    /// Diff two job versions
    Diff {
        /// Job ID
        job_id: String,
        /// First version
        v1: u64,
        /// Second version
        v2: u64,
    },
    /// Rollback to a previous version
    Rollback {
        /// Job ID
        job_id: String,
        /// Version to rollback to
        version: u64,
    },
}

#[derive(Subcommand)]
pub enum NodeCmd {
    /// List nodes
    List,
    /// Show node status
    Status {
        /// Node ID (omit for all nodes)
        node_id: Option<String>,
    },
    /// Drain a node (migrate allocations away)
    Drain {
        /// Node ID
        node_id: String,
        /// Deadline in seconds
        #[arg(long)]
        deadline: Option<u64>,
    },
    /// Set node scheduling eligibility
    Eligibility {
        /// Node ID
        node_id: String,
        /// Enable scheduling
        #[arg(long, conflicts_with = "disable")]
        enable: bool,
        /// Disable scheduling
        #[arg(long, conflicts_with = "enable")]
        disable: bool,
    },
}

#[derive(Subcommand)]
pub enum AllocCmd {
    /// List all allocations
    List,
    /// Get allocation details
    Get {
        /// Allocation ID
        alloc_id: String,
    },
    /// Stream allocation logs
    Logs {
        /// Allocation ID
        alloc_id: String,
        /// Task name
        #[arg(long)]
        task: Option<String>,
        /// Follow log output
        #[arg(long)]
        follow: bool,
    },
}

#[derive(Subcommand)]
pub enum ContextCmd {
    /// List all contexts
    List,
    /// Switch active context
    Use {
        /// Context name
        name: String,
    },
    /// Show current context
    Current,
}

#[derive(Subcommand)]
pub enum EventCmd {
    /// List events
    List {
        /// Filter by event kind
        #[arg(long)]
        kind: Option<String>,
        /// Show events since duration (e.g., "5m", "1h")
        #[arg(long)]
        since: Option<String>,
    },
    /// Stream events in real-time (SSE)
    Stream {
        /// Filter by event kind
        #[arg(long)]
        kind: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ReleaseCmd {
    /// List releases
    List,
    /// Get release details
    Get {
        /// Release ID
        release_id: String,
    },
    /// Promote a release to active
    Promote {
        /// Release ID
        release_id: String,
    },
    /// Rollback a release
    Rollback {
        /// Release ID
        release_id: String,
    },
}

#[derive(Subcommand)]
pub enum ForgeCmd {
    /// Initialize a new forge from template
    Init {
        /// Forge name
        name: String,
    },
    /// Validate a forge's outputs
    Validate {
        /// Path to forge directory
        #[arg(default_value = ".")]
        path: String,
    },
    /// Inspect a forge's metadata and job specs
    Inspect {
        /// Flake reference or local path
        flake_ref: String,
    },
}

#[derive(Subcommand)]
pub enum SourceCmd {
    /// List all sources
    List,
    /// Get source details
    Get {
        /// Source name or ID
        name_or_id: String,
    },
    /// Add a new source
    Add {
        /// Source name
        name: String,
        /// Flake reference (e.g., "github:pleme-io/tatara-infra")
        flake_ref: String,
        /// Source kind
        #[arg(long, default_value = "git-flake")]
        kind: String,
    },
    /// Delete a source and its managed jobs
    Delete {
        /// Source name or ID
        name_or_id: String,
    },
    /// Force immediate sync
    Sync {
        /// Source name or ID
        name_or_id: String,
    },
    /// Suspend reconciliation
    Suspend {
        /// Source name or ID
        name_or_id: String,
    },
    /// Resume reconciliation
    Resume {
        /// Source name or ID
        name_or_id: String,
    },
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}
