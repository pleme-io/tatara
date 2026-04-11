//! The six classification dimensions.
//!
//! Every convergence point is classified along six orthogonal axes.
//! Together, these determine scheduling, coordination, verification,
//! lifetime, and intelligence participation.

use serde::{Deserialize, Serialize};

// ── Dimension 1: Structure — How Data Flows ───────────────────

/// Structural type of a convergence point — how data flows through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvergencePointType {
    /// 1 input → 1 output (linear conversion).
    Transform,
    /// 1 input → N outputs (fan-out, spawns downstream DAGs).
    Fork,
    /// N inputs → 1 output (fan-in, merges upstream results).
    Join,
    /// N inputs → 1 output (barrier, waits for all inputs).
    Gate,
    /// N inputs → 1 output (choice, picks best by policy).
    Select,
    /// 1 input → N outputs same type (replicate signal).
    Broadcast,
    /// N inputs → 1 output (fold/aggregate).
    Reduce,
    /// 1 input → 1 output + side-channel (tap for observation).
    Observe,
}

// ── Dimension 2: Substrate — What Dimension ───────────────────

/// Which operational substrate a convergence point belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubstrateType {
    /// Cost optimization, billing, budgets, spot markets.
    Financial,
    /// CPU, GPU, memory, WASI runtimes.
    Compute,
    /// Connectivity, DNS, TLS, routing, mesh.
    Network,
    /// Volumes, caches, replication, backups.
    Storage,
    /// Secrets, certificates, policies.
    Security,
    /// Authentication, authorization, RBAC.
    Identity,
    /// Metrics, logs, traces, alerting.
    Observability,
    /// Compliance frameworks, data residency, audit.
    Regulatory,
}

// ── Dimension 3: Horizon — How Long ───────────────────────────

/// How long a convergence point runs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceHorizon {
    /// Has a fixed point — distance CAN reach 0. Terminates.
    Bounded,
    /// Runs in perpetuity. Rate is the health signal, not distance.
    Asymptotic {
        /// What metric is being optimized.
        metric: String,
        /// Minimize or maximize.
        direction: OptimizationDirection,
        /// Rate threshold considered healthy.
        healthy_rate_threshold: f64,
    },
}

/// Direction of asymptotic optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationDirection {
    /// Cost, latency, error rate — lower is better.
    Minimize,
    /// Revenue, throughput, coverage — higher is better.
    Maximize,
}

// ── Dimension 4: Coordination — How Nodes Agree ───────────────

/// CALM theorem classification for an operation.
/// Determines whether the operation can be distributed without coordination.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CalmClassification {
    /// Monotone: can be distributed without coordination.
    /// Examples: health checks, metrics, flow logs, set unions.
    Monotone,
    /// Non-monotone: requires coordination (Raft).
    /// Examples: allocation placement, job deletion, policy changes.
    NonMonotone,
}

// ── Dimension 5: Trust — When Compliance Is Verified ──────────
//    (VerificationPhase lives in compliance_binding.rs)

// ── Dimension 6: Intelligence — Who Drives Convergence ────────

/// Whether intelligence (AI/LLM) participates in convergence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ComputationMode {
    /// Deterministic, no AI. Fully automated, fully reproducible.
    Mechanical,
    /// An LLM participates through an interface.
    AiAssisted {
        /// What role the AI plays.
        role: AiRole,
        /// Through which interface.
        interface: AiInterface,
    },
    /// Mechanical execution with AI at specific boundary phases.
    Hybrid {
        /// Phases driven mechanically.
        mechanical_phases: Vec<String>,
        /// Phases driven by AI.
        ai_phases: Vec<String>,
    },
}

/// The role an AI plays at a convergence point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiRole {
    /// Reads convergence state, produces analysis.
    Observer,
    /// Recommends actions, system/human decides.
    Advisor,
    /// Takes bounded actions within emission catalogs.
    Actor,
    /// Reviews convergence correctness, attests.
    Verifier,
    /// Generates compliance/performance reports.
    Reporter,
}

/// The interface through which AI accesses convergence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiInterface {
    /// Model Context Protocol — structured tool access.
    Mcp,
    /// REST API.
    Rest,
    /// GraphQL.
    GraphQl,
    /// gRPC.
    Grpc,
}

/// The outcome of a convergence point execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceOutcome {
    /// Converged successfully — distance = 0.
    Converged,
    /// Cannot converge — permanent failure.
    Failed { reason: String },
    /// Partially converged — degraded operation.
    Degraded {
        /// What was achieved.
        achieved: super::convergence_state::ConvergenceDistance,
        /// What's missing.
        missing: Vec<String>,
    },
}

/// The mechanism that drives convergence at a given point.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceMechanism {
    /// Raft consensus (leader coordinates).
    Raft,
    /// Gossip protocol (eventually consistent, no coordination).
    Gossip,
    /// Local computation (no network, single-node).
    Local,
    /// NATS event bus (fire-and-forget, append-only).
    Nats,
    /// Fixed-point iteration (recursive evaluation until stable).
    FixedPoint,
    /// Control feedback loop (PID-like).
    Feedback,
}
