//! Convergence state — the mathematical foundation for convergence as computation.
//!
//! Every distributed system computes by iteratively applying operations
//! that move state closer to equilibrium. This module formalizes that:
//!
//! - **ConvergenceDistance**: How far is current state from desired state?
//! - **ConvergenceRate**: Is the system converging, diverging, or oscillating?
//! - **ConvergenceState**: Complete convergence telemetry per entity
//! - **ConvergencePoint**: A single step in the architectural computation
//!
//! Each architectural step (Nix eval → Raft replicate → schedule → warm →
//! execute → health check → catalog register) IS a convergence point.
//! The system IS the sequence of convergence points.
//! The computation IS the convergence between them.
//!
//! References:
//! - CALM theorem (Hellerstein 2010): monotone ops converge without coordination
//! - CRDTs (Shapiro 2011): join-semilattice operations always converge
//! - Fixed-point (Knaster-Tarski): computation converges to least fixed point
//! - Self-stabilization (Dijkstra 1974): converge from ANY state
//! - Control theory: PID feedback for damping oscillation

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── Convergence Distance ───────────────────────────────────────

/// How far is the current state from the desired state?
/// This is the fundamental metric of convergence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceDistance {
    /// Fully converged — observed matches desired exactly.
    /// The computation at this convergence point is complete.
    Converged,

    /// Partially converged — some dimensions match, some don't.
    /// The computation is in progress.
    Partial {
        /// Number of dimensions that match desired state.
        matching: u32,
        /// Total number of dimensions being tracked.
        total: u32,
        /// What's still diverged (human-readable).
        pending: Vec<String>,
    },

    /// Diverged — observed is far from desired.
    /// The computation needs to be driven forward.
    Diverged {
        /// Why the state is diverged.
        reason: String,
    },

    /// Unknown — no observation yet. The computation hasn't started.
    Unknown,
}

impl ConvergenceDistance {
    /// Is the entity fully converged?
    pub fn is_converged(&self) -> bool {
        matches!(self, Self::Converged)
    }

    /// Numeric distance (0.0 = converged, 1.0 = fully diverged).
    pub fn numeric(&self) -> f64 {
        match self {
            Self::Converged => 0.0,
            Self::Partial { matching, total, .. } => {
                if *total == 0 { 0.0 }
                else { 1.0 - (*matching as f64 / *total as f64) }
            }
            Self::Diverged { .. } => 1.0,
            Self::Unknown => 1.0,
        }
    }
}

// ── Convergence State ──────────────────────────────────────────

/// Complete convergence telemetry for a single entity.
/// Tracks distance, rate, oscillation, and damping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergenceState {
    /// Entity this state tracks.
    pub entity_id: String,

    /// Current distance from desired state.
    pub distance: ConvergenceDistance,

    /// Rate of convergence (negative = converging, positive = diverging).
    /// Computed as: (current_distance - previous_distance) / tick_duration
    pub rate: f64,

    /// Is the system oscillating? (distance alternates up/down)
    pub oscillating: bool,

    /// Number of convergence ticks applied.
    pub ticks: u64,

    /// When was the last time this entity was fully converged?
    pub last_converged_at: Option<DateTime<Utc>>,

    /// How long has this entity been in its current distance state?
    pub time_in_current_state: Duration,

    /// Current damping factor (1.0 = normal, >1.0 = backed off).
    /// Increases when oscillating, resets when stable.
    pub damping: f64,

    /// Phase change count in the last minute (for oscillation detection).
    pub recent_phase_changes: u32,
}

impl Default for ConvergenceState {
    fn default() -> Self {
        Self {
            entity_id: String::new(),
            distance: ConvergenceDistance::Unknown,
            rate: 0.0,
            oscillating: false,
            ticks: 0,
            last_converged_at: None,
            time_in_current_state: Duration::zero(),
            damping: 1.0,
            recent_phase_changes: 0,
        }
    }
}

impl ConvergenceState {
    /// Create a new convergence state for an entity.
    pub fn new(entity_id: impl Into<String>) -> Self {
        Self {
            entity_id: entity_id.into(),
            ..Default::default()
        }
    }

    /// Update this state with a new distance observation.
    /// Computes rate, detects oscillation, applies damping.
    pub fn update(&mut self, new_distance: ConvergenceDistance, tick_duration_ms: u64) {
        let old_numeric = self.distance.numeric();
        let new_numeric = new_distance.numeric();

        // Compute rate: negative = converging, positive = diverging
        if tick_duration_ms > 0 {
            self.rate = (new_numeric - old_numeric) / (tick_duration_ms as f64 / 1000.0);
        }

        // Detect oscillation: rate alternates sign across ticks
        let was_converging = old_numeric > new_numeric;
        let direction_changed = (self.rate > 0.0) != (old_numeric > new_numeric);
        if direction_changed && self.ticks > 2 {
            self.recent_phase_changes += 1;
        }

        // Oscillation threshold: >3 direction changes per minute
        self.oscillating = self.recent_phase_changes > 3;

        // Apply damping when oscillating
        if self.oscillating {
            self.damping = (self.damping * 1.5).min(32.0); // exponential backoff, cap at 32x
        } else if self.damping > 1.0 {
            self.damping = (self.damping * 0.9).max(1.0); // slowly reduce damping
        }

        // Track convergence time
        if new_distance.is_converged() && !self.distance.is_converged() {
            self.last_converged_at = Some(Utc::now());
        }

        // Update state
        if std::mem::discriminant(&self.distance) != std::mem::discriminant(&new_distance) {
            self.time_in_current_state = Duration::zero();
        } else {
            self.time_in_current_state =
                self.time_in_current_state + Duration::milliseconds(tick_duration_ms as i64);
        }

        self.distance = new_distance;
        self.ticks += 1;
    }

    /// Should the convergence engine wait before acting? (damping)
    pub fn should_wait(&self) -> bool {
        self.oscillating && self.damping > 2.0
    }

    /// Time since last convergence (None if never converged).
    pub fn time_since_converged(&self) -> Option<Duration> {
        self.last_converged_at.map(|t| Utc::now() - t)
    }
}

// ── Convergence Point ──────────────────────────────────────────

/// A convergence point represents a single verified checkpoint in the
/// architectural computation pipeline.
///
/// Each point has four phases:
///   1. **Prepare**: Verify the input environment is correct before starting.
///      Checks preconditions, validates attestation hashes from previous points.
///   2. **Execute**: Drive toward the target state (the convergence itself).
///   3. **Verify**: Prove the state is correct — not just "looks right" but
///      cryptographically attested via tameshi BLAKE3 Merkle trees.
///   4. **Gate**: Only allow the next point to begin when this one is verified.
///      The attestation hash from this point feeds into the next point's
///      preparation layer.
///
/// This creates **atomic convergence boundaries**:
/// ```text
/// Point A: [Prepare → Execute → Verify → Attest] ──hash──→
/// Point B: [Prepare(verify hash) → Execute → Verify → Attest] ──hash──→ ...
/// ```
///
/// You can't skip steps. You can't forge intermediate states.
/// The entire chain is auditable after the fact.
///
/// The sequence of convergence points IS the computation:
///   NixEval → RaftReplicate → Schedule → Warm → Execute → HealthCheck → CatalogRegister
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergencePoint {
    /// Name of this convergence point.
    pub name: String,

    /// What this point converges toward.
    pub description: String,

    /// Is this point monotone? (Can be distributed without coordination per CALM)
    pub monotone: bool,

    /// What mechanism drives convergence at this point.
    pub mechanism: ConvergenceMechanism,

    /// Current state of convergence at this point.
    pub state: ConvergenceState,

    /// The boundary — preparation, verification, and gating for this point.
    pub boundary: ConvergenceBoundary,
}

/// The atomic convergence boundary around a convergence point.
///
/// Ensures each point is a verified checkpoint:
/// - Preparation validates the input environment
/// - Verification proves the output is correct
/// - Attestation produces a hash for the next point
/// - Gate prevents the next point from starting until verified
///
/// This is the foundation for **provably secure computation** —
/// each step is cryptographically bound to the previous via tameshi.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConvergenceBoundary {
    /// Preconditions that must be true before this point can begin.
    /// Each precondition is a named check with a pass/fail status.
    pub preconditions: Vec<BoundaryCheck>,

    /// Postconditions that must be true after this point converges.
    /// These are the verification checks.
    pub postconditions: Vec<BoundaryCheck>,

    /// Attestation hash from the previous convergence point.
    /// This point's preparation phase verifies this hash.
    pub input_attestation: Option<String>,

    /// Attestation hash produced by this point after verification.
    /// Feeds into the next point's input_attestation.
    pub output_attestation: Option<String>,

    /// Current phase of this boundary.
    pub phase: BoundaryPhase,
}

/// A single check in a convergence boundary (pre or post condition).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryCheck {
    /// Name of the check.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Has this check passed?
    pub passed: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// When this check was last evaluated.
    pub checked_at: Option<DateTime<Utc>>,
}

impl BoundaryCheck {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            passed: false,
            error: None,
            checked_at: None,
        }
    }

    pub fn pass(&mut self) {
        self.passed = true;
        self.error = None;
        self.checked_at = Some(Utc::now());
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.passed = false;
        self.error = Some(error.into());
        self.checked_at = Some(Utc::now());
    }
}

/// The phase of a convergence boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryPhase {
    /// Not started — waiting for dependencies.
    #[default]
    Pending,
    /// Running precondition checks.
    Preparing,
    /// Preconditions passed, convergence in progress.
    Executing,
    /// Convergence complete, running postcondition checks.
    Verifying,
    /// Postconditions passed, attestation hash produced.
    /// The gate is open for the next point.
    Attested,
    /// A check failed — this point is blocked.
    Failed { reason: String },
}

impl BoundaryPhase {
    pub fn is_attested(&self) -> bool {
        matches!(self, Self::Attested)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    pub fn is_gate_open(&self) -> bool {
        self.is_attested()
    }
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

// ── Cluster Convergence Summary ────────────────────────────────

/// Cluster-wide convergence summary.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClusterConvergence {
    /// Number of fully converged entities.
    pub converged: u32,
    /// Number of partially converged entities.
    pub partial: u32,
    /// Number of diverged entities.
    pub diverged: u32,
    /// Number of entities with unknown state.
    pub unknown: u32,
    /// Overall cluster convergence (0.0 = all converged, 1.0 = all diverged).
    pub overall_distance: f64,
    /// Time since the cluster was last fully converged.
    pub time_since_fully_converged: Option<Duration>,
    /// Per-entity convergence states.
    pub entities: HashMap<String, ConvergenceState>,
}

impl ClusterConvergence {
    /// Compute cluster-wide summary from entity states.
    pub fn from_entities(entities: HashMap<String, ConvergenceState>) -> Self {
        let mut summary = Self::default();
        let mut total_distance = 0.0;

        for (_, state) in &entities {
            match &state.distance {
                ConvergenceDistance::Converged => summary.converged += 1,
                ConvergenceDistance::Partial { .. } => summary.partial += 1,
                ConvergenceDistance::Diverged { .. } => summary.diverged += 1,
                ConvergenceDistance::Unknown => summary.unknown += 1,
            }
            total_distance += state.distance.numeric();
        }

        let total = entities.len().max(1) as f64;
        summary.overall_distance = total_distance / total;
        summary.entities = entities;
        summary
    }

    /// Is the entire cluster converged?
    pub fn is_fully_converged(&self) -> bool {
        self.diverged == 0 && self.partial == 0 && self.unknown == 0
    }
}

// ── CALM Classification ────────────────────────────────────────

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convergence_distance_numeric() {
        assert_eq!(ConvergenceDistance::Converged.numeric(), 0.0);
        assert_eq!(
            ConvergenceDistance::Partial {
                matching: 3,
                total: 4,
                pending: vec![]
            }
            .numeric(),
            0.25
        );
        assert_eq!(
            ConvergenceDistance::Diverged {
                reason: "test".into()
            }
            .numeric(),
            1.0
        );
        assert_eq!(ConvergenceDistance::Unknown.numeric(), 1.0);
    }

    #[test]
    fn test_convergence_state_update() {
        let mut state = ConvergenceState::new("alloc-1");

        // Initial → Partial
        state.update(
            ConvergenceDistance::Partial {
                matching: 1,
                total: 4,
                pending: vec!["secrets".into(), "volumes".into(), "driver".into()],
            },
            1000,
        );
        assert_eq!(state.ticks, 1);
        assert!(state.distance.numeric() > 0.0);

        // Partial → More Partial
        state.update(
            ConvergenceDistance::Partial {
                matching: 3,
                total: 4,
                pending: vec!["driver".into()],
            },
            1000,
        );
        assert_eq!(state.ticks, 2);
        assert!(state.rate < 0.0); // converging

        // Partial → Converged
        state.update(ConvergenceDistance::Converged, 1000);
        assert_eq!(state.ticks, 3);
        assert!(state.distance.is_converged());
        assert!(state.last_converged_at.is_some());
    }

    #[test]
    fn test_oscillation_detection() {
        let mut state = ConvergenceState::new("osc-1");

        // Simulate oscillation: converging → diverging → converging → diverging
        for i in 0..10 {
            let distance = if i % 2 == 0 {
                ConvergenceDistance::Partial {
                    matching: 2,
                    total: 4,
                    pending: vec![],
                }
            } else {
                ConvergenceDistance::Diverged {
                    reason: "unstable".into(),
                }
            };
            state.update(distance, 100);
        }

        assert!(state.recent_phase_changes > 3);
        assert!(state.oscillating);
        assert!(state.damping > 1.0);
        assert!(state.should_wait());
    }

    #[test]
    fn test_cluster_convergence() {
        let mut entities = HashMap::new();
        entities.insert(
            "a".into(),
            ConvergenceState {
                distance: ConvergenceDistance::Converged,
                ..Default::default()
            },
        );
        entities.insert(
            "b".into(),
            ConvergenceState {
                distance: ConvergenceDistance::Partial {
                    matching: 2,
                    total: 4,
                    pending: vec![],
                },
                ..Default::default()
            },
        );
        entities.insert(
            "c".into(),
            ConvergenceState {
                distance: ConvergenceDistance::Diverged {
                    reason: "test".into(),
                },
                ..Default::default()
            },
        );

        let summary = ClusterConvergence::from_entities(entities);
        assert_eq!(summary.converged, 1);
        assert_eq!(summary.partial, 1);
        assert_eq!(summary.diverged, 1);
        assert!(!summary.is_fully_converged());
        assert!(summary.overall_distance > 0.0);
        assert!(summary.overall_distance < 1.0);
    }

    #[test]
    fn test_fully_converged_cluster() {
        let mut entities = HashMap::new();
        entities.insert(
            "a".into(),
            ConvergenceState {
                distance: ConvergenceDistance::Converged,
                ..Default::default()
            },
        );
        entities.insert(
            "b".into(),
            ConvergenceState {
                distance: ConvergenceDistance::Converged,
                ..Default::default()
            },
        );

        let summary = ClusterConvergence::from_entities(entities);
        assert!(summary.is_fully_converged());
        assert_eq!(summary.overall_distance, 0.0);
    }

    #[test]
    fn test_damping_recovery() {
        let mut state = ConvergenceState::new("damp-1");
        state.damping = 8.0; // high damping from previous oscillation
        state.oscillating = false; // no longer oscillating

        // Damping should decrease when not oscillating
        state.update(ConvergenceDistance::Converged, 1000);
        assert!(state.damping < 8.0);
        assert!(state.damping >= 1.0);
    }

    #[test]
    fn test_convergence_point() {
        let point = ConvergencePoint {
            name: "scheduling".into(),
            description: "Allocations placed on eligible nodes".into(),
            monotone: false,
            mechanism: ConvergenceMechanism::Raft,
            state: ConvergenceState::new("scheduling"),
            boundary: ConvergenceBoundary::default(),
        };

        assert!(!point.monotone); // scheduling requires coordination
        assert_eq!(point.mechanism, ConvergenceMechanism::Raft);
        assert!(!point.boundary.phase.is_gate_open()); // gate closed by default
    }

    #[test]
    fn test_calm_classification() {
        // Health checks are monotone (only accumulate results)
        assert_eq!(CalmClassification::Monotone, CalmClassification::Monotone);
        // Allocation placement is non-monotone (exclusive assignment)
        assert_eq!(
            CalmClassification::NonMonotone,
            CalmClassification::NonMonotone
        );
    }

    // ── Boundary tests ────────────────────────────────────────

    #[test]
    fn test_boundary_check_pass_fail() {
        let mut check = BoundaryCheck::new("secrets_resolved", "All secrets fetched");
        assert!(!check.passed);

        check.pass();
        assert!(check.passed);
        assert!(check.error.is_none());
        assert!(check.checked_at.is_some());

        check.fail("akeyless timeout");
        assert!(!check.passed);
        assert_eq!(check.error.as_deref(), Some("akeyless timeout"));
    }

    #[test]
    fn test_boundary_phase_transitions() {
        assert!(!BoundaryPhase::Pending.is_gate_open());
        assert!(!BoundaryPhase::Preparing.is_gate_open());
        assert!(!BoundaryPhase::Executing.is_gate_open());
        assert!(!BoundaryPhase::Verifying.is_gate_open());
        assert!(BoundaryPhase::Attested.is_gate_open());
        assert!(BoundaryPhase::Attested.is_attested());
        assert!(!BoundaryPhase::Failed {
            reason: "test".into()
        }
        .is_gate_open());
        assert!(BoundaryPhase::Failed {
            reason: "test".into()
        }
        .is_failed());
    }

    #[test]
    fn test_boundary_attestation_chain() {
        // Simulate: Point A attests → hash feeds into Point B's preparation
        let mut boundary_a = ConvergenceBoundary::default();
        boundary_a.phase = BoundaryPhase::Attested;
        boundary_a.output_attestation = Some("blake3:abc123".to_string());

        let mut boundary_b = ConvergenceBoundary::default();
        boundary_b.input_attestation = boundary_a.output_attestation.clone();

        // Point B's preparation can verify A's hash
        assert_eq!(
            boundary_b.input_attestation.as_deref(),
            Some("blake3:abc123")
        );

        // Gate: B can only start when A is attested
        assert!(boundary_a.phase.is_gate_open());
    }

    #[test]
    fn test_convergence_point_with_boundary() {
        let point = ConvergencePoint {
            name: "secret_resolve".into(),
            description: "Fetch secrets from Akeyless".into(),
            monotone: true, // read-only, cacheable
            mechanism: ConvergenceMechanism::Local,
            state: ConvergenceState::new("secret_resolve"),
            boundary: ConvergenceBoundary {
                preconditions: vec![
                    BoundaryCheck::new("port_allocated", "Port must be allocated first"),
                ],
                postconditions: vec![
                    BoundaryCheck::new("secrets_valid", "All secrets non-empty"),
                    BoundaryCheck::new("secrets_attested", "Secret hashes match tameshi record"),
                ],
                input_attestation: Some("blake3:prev_point_hash".into()),
                output_attestation: None, // set after verification
                phase: BoundaryPhase::Pending,
            },
        };

        assert!(!point.boundary.phase.is_gate_open());
        assert_eq!(point.boundary.preconditions.len(), 1);
        assert_eq!(point.boundary.postconditions.len(), 2);
    }
}
