//! Compliance baseline ordering — `fedramp-high` ≥ `fedramp-moderate` ≥ `cis-l2` ≥ `cis-l1` ≥ `none`.
//!
//! Replaces the rank-ordered comparator in
//! `convergence_controller::cluster_quality::compliance_level_rank`.

use serde::{Deserialize, Serialize};

use crate::Lattice;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Baseline {
    #[default]
    None,
    CisL1,
    CisL2,
    FedrampLow,
    FedrampModerate,
    FedrampHigh,
    Soc2,
    PciDss,
}

impl Baseline {
    /// Canonical rank — higher = stricter.
    pub const fn rank(self) -> u8 {
        match self {
            Self::None => 0,
            Self::CisL1 => 1,
            Self::CisL2 => 2,
            Self::FedrampLow => 3,
            Self::FedrampModerate => 4,
            Self::FedrampHigh => 5,
            Self::Soc2 => 4,   // same stratum as Moderate
            Self::PciDss => 4, // same stratum as Moderate
        }
    }

    /// Parse a canonical baseline name.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().replace('_', "-").as_str() {
            "none" | "" => Some(Self::None),
            "cis-l1" | "cis-k8s-l1" => Some(Self::CisL1),
            "cis-l2" | "cis-k8s-l2" | "cis-k8s-v1.8" => Some(Self::CisL2),
            "fedramp-low" => Some(Self::FedrampLow),
            "fedramp-moderate" | "fedramp" => Some(Self::FedrampModerate),
            "fedramp-high" => Some(Self::FedrampHigh),
            "soc2" => Some(Self::Soc2),
            "pci-dss" | "pci" => Some(Self::PciDss),
            _ => None,
        }
    }
}

impl Lattice for Baseline {
    fn meet(&self, other: &Self) -> Self {
        if self.rank() <= other.rank() {
            *self
        } else {
            *other
        }
    }
    fn join(&self, other: &Self) -> Self {
        if self.rank() >= other.rank() {
            *self
        } else {
            *other
        }
    }
    fn leq(&self, other: &Self) -> bool {
        self.rank() <= other.rank()
    }
    fn bottom() -> Self {
        Self::None
    }
    fn top() -> Self {
        Self::FedrampHigh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_aliases() {
        assert_eq!(Baseline::parse("fedramp-moderate"), Some(Baseline::FedrampModerate));
        assert_eq!(Baseline::parse("CIS-L2"), Some(Baseline::CisL2));
        assert_eq!(Baseline::parse("none"), Some(Baseline::None));
        assert_eq!(Baseline::parse(""), Some(Baseline::None));
        assert_eq!(Baseline::parse("bogus"), None);
    }

    #[test]
    fn meet_picks_lower() {
        assert_eq!(
            Baseline::FedrampHigh.meet(&Baseline::CisL1),
            Baseline::CisL1
        );
    }

    #[test]
    fn join_picks_higher() {
        assert_eq!(
            Baseline::FedrampHigh.join(&Baseline::CisL1),
            Baseline::FedrampHigh
        );
    }
}
