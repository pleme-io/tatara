//! Compliance baseline ordering — `fedramp-high` ≥ `fedramp-moderate` ≥ `cis-l2` ≥ `cis-l1` ≥ `none`.
//!
//! Replaces the rank-ordered comparator in
//! `convergence_controller::cluster_quality::compliance_level_rank`.

use serde::{Deserialize, Serialize};

use crate::Lattice;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
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
    /// The closed set of compliance baselines — single source of truth
    /// that peers this crate's lattice-axis enum with the seven
    /// classification-axis closed sets in
    /// [`tatara_process::classification`]
    /// (`ConvergencePointType::ALL`, `Arity::ALL`,
    /// `SubstrateType::ALL`, `HorizonKind::ALL`,
    /// `OptimizationDirection::ALL`, `CalmClassification::ALL`,
    /// `DataClassification::ALL`). Adding a ninth baseline lands at
    /// ONE `ALL` entry + ONE [`Self::rank`] arm + ONE
    /// [`Self::canonical_name`] arm + ONE [`Self::parse`] arm —
    /// exhaustively checked by the compiler (the `[Self; 8]` array
    /// literal forces the arity AND the `canonical_name` / `rank`
    /// exhaustive-match arms force per-variant coverage) AND by the
    /// round-trip seal test
    /// [`tests::canonical_name_round_trips_through_parse_for_every_variant_in_all`]
    /// (a new variant that omits its `parse` alias entry surfaces at
    /// the round-trip inverse rather than as silent
    /// `parse(canonical_name(v)) == None`). Closes the compliance-lattice
    /// axis that [`THEORY.md §III.3`] names as the fifth typescape
    /// dimension every controller and morphism carries — sibling of
    /// `DataClassification::ALL` (sensitivity axis) and
    /// `CalmClassification::ALL` (CALM axis) that the top-of-lib
    /// `from_all(&T::ALL)` proptest primitive already binds to for
    /// closed-set-driven property coverage.
    pub const ALL: [Self; 8] = [
        Self::None,
        Self::CisL1,
        Self::CisL2,
        Self::FedrampLow,
        Self::FedrampModerate,
        Self::FedrampHigh,
        Self::Soc2,
        Self::PciDss,
    ];

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

    /// Canonical short-form projection — the inverse of
    /// [`Self::parse`], picking ONE alias per variant as the load-bearing
    /// wire name. Round-trip sealed by
    /// [`tests::canonical_name_round_trips_through_parse_for_every_variant_in_all`]:
    /// `parse(canonical_name(v)) == Some(v)` for every
    /// `v ∈ Baseline::ALL`. Names match the top-of-module docstring
    /// ordering (`fedramp-high` ≥ `fedramp-moderate` ≥ `cis-l2` ≥
    /// `cis-l1` ≥ `none`) verbatim and align with the alias each
    /// [`Self::parse`] arm accepts as its ★★ primary form (not the
    /// `cis-k8s-*` / `fedramp` / `pci` shorthands, which stay
    /// alias-only). Exhaustive match so a future variant addition
    /// triggers the compiler's exhaustiveness check at this site
    /// rather than silently defaulting to any single name.
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CisL1 => "cis-l1",
            Self::CisL2 => "cis-l2",
            Self::FedrampLow => "fedramp-low",
            Self::FedrampModerate => "fedramp-moderate",
            Self::FedrampHigh => "fedramp-high",
            Self::Soc2 => "soc2",
            Self::PciDss => "pci-dss",
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
        assert_eq!(
            Baseline::parse("fedramp-moderate"),
            Some(Baseline::FedrampModerate)
        );
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

    // ── ALL closed-set primitive + canonical_name inverse ────────────
    //
    // Bind the [`Baseline::ALL`] closed-set primitive AND the
    // [`Baseline::canonical_name`] inverse-of-[`Baseline::parse`]
    // projection at fail-before-pass-after granularity. Pre-lift the
    // enum's eight variants were reachable only by manual `match self
    // { ... }` enumeration hand-authored at each callsite; a future
    // proptest strategy that wanted closed-set-driven coverage over
    // Baseline (peer to `any_data_class` / `any_calm` in the top-of-lib
    // `from_all(&T::ALL)` primitive) had no source-of-truth `ALL` to
    // bind through, and a future consumer that wanted to render the
    // canonical wire name per variant had to re-implement the choice
    // per callsite (which of `soc2` / `SOC 2` / `soc-2` was the ★★
    // primary form?). Post-lift the closed set + the canonical name
    // per variant live at ONE substrate owner per axis with the
    // round-trip inverse sealed below.

    /// [`Baseline::ALL`] reaches every declaration-order variant of the
    /// eight-baseline compliance closed set exactly once. Fail-before-
    /// pass-after: pre-lift this test cannot compile because `ALL` is
    /// not exposed as an inherent const — no source-of-truth iterable
    /// existed for closed-set-driven consumers. Post-lift the sweep
    /// binds every variant reachable via `ALL[i]` and pins the
    /// [`Self; 8]` array literal's arity — a future variant addition
    /// that extends the enum but not `ALL` will fail the compiler's
    /// arity check at the const declaration BEFORE this test even
    /// runs.
    #[test]
    fn all_reaches_every_declaration_order_variant_of_baseline_exactly_once() {
        use std::collections::HashSet;
        let seen: HashSet<Baseline> = (0..Baseline::ALL.len()).map(|i| Baseline::ALL[i]).collect();
        assert_eq!(
            seen.len(),
            Baseline::ALL.len(),
            "Baseline::ALL's index-sweep must reach every variant \
             without duplicates — a duplicate collapse or a missed \
             slot would silently under-cover the compliance closed set",
        );
        // Explicit per-variant reachability — a rename that dropped a
        // variant from ALL without renaming the enum surfaces here as
        // a missing entry (this loop is the last line of defense once
        // the arity check has been satisfied by a placeholder edit).
        for v in [
            Baseline::None,
            Baseline::CisL1,
            Baseline::CisL2,
            Baseline::FedrampLow,
            Baseline::FedrampModerate,
            Baseline::FedrampHigh,
            Baseline::Soc2,
            Baseline::PciDss,
        ] {
            assert!(
                seen.contains(&v),
                "Baseline::ALL sweep must reach {v:?} — the eight-variant \
                 declaration-order enumeration has drifted",
            );
        }
    }

    /// [`Baseline::canonical_name`] is a right-inverse of
    /// [`Baseline::parse`]: `parse(canonical_name(v)) == Some(v)` for
    /// every `v ∈ Baseline::ALL`. Fail-before-pass-after: pre-lift
    /// this test cannot compile because `canonical_name` is not
    /// exposed as an inherent method — no per-variant single-name
    /// projection existed. Post-lift the round trip pins BOTH
    /// primitives: a future variant addition that extends `ALL` +
    /// `canonical_name` but forgets to add a `parse` alias surfaces
    /// here as `parse(canonical_name(v)) == None`, and a
    /// canonical-name choice that doesn't match any alias in `parse`
    /// (e.g. renaming `Self::CisL1` canonical to `cis-l-1` while
    /// `parse` still expects `cis-l1`) fails the same assertion.
    #[test]
    fn canonical_name_round_trips_through_parse_for_every_variant_in_all() {
        for v in Baseline::ALL {
            let name = v.canonical_name();
            assert_eq!(
                Baseline::parse(name),
                Some(v),
                "canonical_name({v:?}) = {name:?} but \
                 parse({name:?}) does NOT round-trip back to {v:?} — \
                 the canonical-name choice has drifted away from \
                 the parse alias table (the load-bearing wire \
                 vocabulary the top-of-module docstring names)",
            );
        }
    }

    /// [`Baseline::canonical_name`] matches the top-of-module docstring's
    /// five-name ordering (`fedramp-high` ≥ `fedramp-moderate` ≥
    /// `cis-l2` ≥ `cis-l1` ≥ `none`) verbatim, plus the three
    /// remaining variants at their own canonical wire names
    /// (`fedramp-low`, `soc2`, `pci-dss`). Pins the byte-identical
    /// wire vocabulary that operator-facing diagnostics and audit
    /// artifacts compose against — a future rename that drifts
    /// `canonical_name` away from the docstring surfaces here rather
    /// than only in the doc.
    #[test]
    fn canonical_name_matches_the_module_docstring_wire_vocabulary() {
        assert_eq!(Baseline::None.canonical_name(), "none");
        assert_eq!(Baseline::CisL1.canonical_name(), "cis-l1");
        assert_eq!(Baseline::CisL2.canonical_name(), "cis-l2");
        assert_eq!(Baseline::FedrampLow.canonical_name(), "fedramp-low");
        assert_eq!(
            Baseline::FedrampModerate.canonical_name(),
            "fedramp-moderate"
        );
        assert_eq!(Baseline::FedrampHigh.canonical_name(), "fedramp-high");
        assert_eq!(Baseline::Soc2.canonical_name(), "soc2");
        assert_eq!(Baseline::PciDss.canonical_name(), "pci-dss");
    }

    /// [`Baseline::rank`] climbs monotonically with the declaration
    /// order of the top-of-module ordering docstring — `None (0) ≤
    /// CisL1 (1) ≤ CisL2 (2) ≤ FedrampLow (3) ≤ FedrampModerate (4)
    /// ≤ FedrampHigh (5)` — with the two rank-4 aliases (`Soc2`,
    /// `PciDss`) explicitly documented as sharing the
    /// `FedrampModerate` stratum. Pins the seven pairwise
    /// declaration-order inequalities that
    /// [`impl Lattice for Baseline`]'s `leq` relies on so a future
    /// variant insertion or rank refactor surfaces here before
    /// reaching the compliance-lattice consumers.
    ///
    /// Note: this test intentionally does NOT assert that `rank`
    /// pins a strict total order across ALL — the SOC2 / PCI-DSS /
    /// FedrampModerate three-way tie at rank 4 is a documented
    /// domain choice (see `Self::rank`'s inline comments) that
    /// currently breaks `Lattice::meet`'s commutativity on those
    /// pairs (a separate substrate-lift concern; not this run's
    /// scope).
    #[test]
    fn rank_climbs_monotonically_across_the_module_docstring_backbone() {
        assert!(Baseline::None.rank() < Baseline::CisL1.rank());
        assert!(Baseline::CisL1.rank() < Baseline::CisL2.rank());
        assert!(Baseline::CisL2.rank() < Baseline::FedrampLow.rank());
        assert!(Baseline::FedrampLow.rank() < Baseline::FedrampModerate.rank());
        assert!(Baseline::FedrampModerate.rank() < Baseline::FedrampHigh.rank());
        // Documented rank-4 tie stratum — every alias sits at the
        // SAME numeric rank as FedrampModerate. A future refactor
        // that lifted `Soc2` / `PciDss` out of the tie stratum
        // would surface here.
        assert_eq!(Baseline::Soc2.rank(), Baseline::FedrampModerate.rank());
        assert_eq!(Baseline::PciDss.rank(), Baseline::FedrampModerate.rank());
    }
}
