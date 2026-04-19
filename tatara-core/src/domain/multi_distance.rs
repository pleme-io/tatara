//! Multi-dimensional convergence distance.
//!
//! Every substrate is its own convergence DAG. The overall distance is a
//! vector — one component per substrate. The system is converged only when
//! ALL substrates are at zero (max component, not average).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::convergence_state::SubstrateType;

/// Per-substrate convergence distance vector.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MultiDimensionalDistance {
    /// Distance per substrate (0.0 = converged on that dimension).
    pub distances: BTreeMap<SubstrateType, f64>,
}

impl MultiDimensionalDistance {
    /// Create a new multi-dimensional distance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set distance for a substrate.
    pub fn set(&mut self, substrate: SubstrateType, distance: f64) {
        self.distances.insert(substrate, distance);
    }

    /// Get distance for a substrate (1.0 if unknown).
    pub fn get(&self, substrate: &SubstrateType) -> f64 {
        self.distances.get(substrate).copied().unwrap_or(1.0)
    }

    /// Overall distance = max component (the worst substrate).
    /// The system is only converged when ALL substrates are at zero.
    pub fn overall(&self) -> f64 {
        self.distances.values().copied().fold(0.0_f64, f64::max)
    }

    /// The substrate with the worst (highest) distance.
    pub fn worst_substrate(&self) -> Option<(SubstrateType, f64)> {
        self.distances
            .iter()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(s, d)| (*s, *d))
    }

    /// Is the system fully converged across all substrates?
    pub fn is_converged(&self) -> bool {
        !self.distances.is_empty() && self.distances.values().all(|d| *d == 0.0)
    }

    /// Number of substrates tracked.
    pub fn substrate_count(&self) -> usize {
        self.distances.len()
    }
}

/// Maximum convergence velocity for a substrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConvergenceBandwidth {
    /// Local computation, in-memory — effectively instant.
    Instant,
    /// API calls, cache lookups.
    Seconds(u64),
    /// Provisioning, cert issuance.
    Minutes(u64),
    /// Large deployments, data migration.
    Hours(u64),
    /// Compliance, procurement.
    Days(u64),
    /// Human decisions, external processes — no upper bound.
    Unbounded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_distance() {
        let d = MultiDimensionalDistance::new();
        assert_eq!(d.overall(), 0.0);
        assert!(!d.is_converged()); // empty is not converged
    }

    #[test]
    fn test_single_substrate() {
        let mut d = MultiDimensionalDistance::new();
        d.set(SubstrateType::Compute, 0.5);
        assert_eq!(d.overall(), 0.5);
        assert!(!d.is_converged());
    }

    #[test]
    fn test_all_converged() {
        let mut d = MultiDimensionalDistance::new();
        d.set(SubstrateType::Compute, 0.0);
        d.set(SubstrateType::Network, 0.0);
        d.set(SubstrateType::Security, 0.0);
        assert_eq!(d.overall(), 0.0);
        assert!(d.is_converged());
    }

    #[test]
    fn test_worst_substrate() {
        let mut d = MultiDimensionalDistance::new();
        d.set(SubstrateType::Compute, 0.1);
        d.set(SubstrateType::Network, 0.8);
        d.set(SubstrateType::Security, 0.3);
        let (worst, dist) = d.worst_substrate().unwrap();
        assert_eq!(worst, SubstrateType::Network);
        assert_eq!(dist, 0.8);
        assert_eq!(d.overall(), 0.8);
    }

    #[test]
    fn test_overall_is_max_not_average() {
        let mut d = MultiDimensionalDistance::new();
        d.set(SubstrateType::Compute, 0.0);
        d.set(SubstrateType::Security, 1.0);
        // Max is 1.0, not average 0.5
        assert_eq!(d.overall(), 1.0);
    }

    #[test]
    fn test_unknown_substrate_defaults_to_1() {
        let d = MultiDimensionalDistance::new();
        assert_eq!(d.get(&SubstrateType::Financial), 1.0);
    }

    #[test]
    fn test_bandwidth_serde() {
        for bw in [
            ConvergenceBandwidth::Instant,
            ConvergenceBandwidth::Seconds(30),
            ConvergenceBandwidth::Minutes(5),
            ConvergenceBandwidth::Hours(2),
            ConvergenceBandwidth::Days(7),
            ConvergenceBandwidth::Unbounded,
        ] {
            let json = serde_json::to_string(&bw).unwrap();
            let parsed: ConvergenceBandwidth = serde_json::from_str(&json).unwrap();
            assert_eq!(bw, parsed);
        }
    }
}
