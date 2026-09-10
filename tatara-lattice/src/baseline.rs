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

    /// Declaration-order position in [`Self::ALL`] — the injective
    /// tie-breaking discriminator that lifts [`Self::rank`] from a
    /// rank-only pre-order (which admits the documented three-way
    /// `Soc2` / `PciDss` / `FedrampModerate` tie at rank 4) onto a
    /// strict total order via [`Self::total_key`]. Exhaustive match so
    /// a future variant addition triggers the compiler's exhaustiveness
    /// check at this site rather than silently defaulting to any single
    /// index; kept in lockstep with [`Self::ALL`] by
    /// [`tests::all_index_matches_declaration_order_position_in_ALL_for_every_variant`],
    /// which iterates the closed set and asserts
    /// `Self::ALL[i].all_index() == i as u8` at every slot.
    pub const fn all_index(self) -> u8 {
        match self {
            Self::None => 0,
            Self::CisL1 => 1,
            Self::CisL2 => 2,
            Self::FedrampLow => 3,
            Self::FedrampModerate => 4,
            Self::FedrampHigh => 5,
            Self::Soc2 => 6,
            Self::PciDss => 7,
        }
    }

    /// Total-order tie-breaking key over `(rank, all_index)` — the
    /// strict total order the [`Lattice`] impl routes `meet` / `join`
    /// / `leq` through. Pre-lift the lattice impl used
    /// `self.rank() <= other.rank()` as its ordering test, which broke
    /// [`Lattice::meet`] / [`Lattice::join`] commutativity on the
    /// documented three-way rank-4 tie stratum (`FedrampModerate`,
    /// `Soc2`, `PciDss` all sharing `rank() == 4`): `Soc2.meet(&PciDss)`
    /// returned `Soc2` by self-preference while `PciDss.meet(&Soc2)`
    /// returned `PciDss` by the same test, violating the top-of-lib
    /// [`Lattice`]-trait docstring's `a ⊓ b = b ⊓ a` law. Post-lift
    /// pairing `rank()` with [`Self::all_index`] (an injection over
    /// [`Self::ALL`] by declaration order) yields a strict total order
    /// on which `min` / `max` are commutative by construction — the
    /// rank stratum stays documented at [`Self::rank`] as the
    /// operator-facing semantic AND the lattice impl now satisfies
    /// every law the top-of-lib docstring promises. At the tie stratum,
    /// declaration-order breaks the tie: `FedrampModerate` (index 4) <
    /// `Soc2` (index 6) < `PciDss` (index 7); consumers that need to
    /// probe "same rank stratum" should compare `rank()` directly
    /// rather than reading `leq` into a pre-order semantic. Round-trip
    /// injectivity over [`Self::ALL`] is sealed by
    /// [`tests::total_key_is_injective_over_baseline_ALL`].
    pub const fn total_key(self) -> (u8, u8) {
        (self.rank(), self.all_index())
    }
}

impl Lattice for Baseline {
    fn meet(&self, other: &Self) -> Self {
        // Total-order min via `total_key` — commutative + idempotent +
        // associative by construction on any total order; consumes the
        // rank tie-break through the declaration-order discriminator so
        // the operator-facing `rank()` stratum stays intact while the
        // lattice impl satisfies the `a ⊓ b = b ⊓ a` law the top-of-lib
        // docstring promises. Pinned exhaustively over `Baseline::ALL`
        // by the `meet_and_join_are_commutative_over_baseline_ALL_*`
        // test family.
        if self.total_key() <= other.total_key() {
            *self
        } else {
            *other
        }
    }
    fn join(&self, other: &Self) -> Self {
        // Total-order max via `total_key` — dual of `meet` on the same
        // total order.
        if self.total_key() >= other.total_key() {
            *self
        } else {
            *other
        }
    }
    fn leq(&self, other: &Self) -> bool {
        // Consistent with `meet` / `join` via the same `total_key`
        // projection — pinned by
        // `leq_agrees_with_meet_and_join_over_baseline_ALL`. At the
        // rank-4 tie stratum this is antisymmetric (declaration-order
        // breaks the tie) rather than symmetric (both directions true
        // under rank-only) — see `Self::total_key` for the semantic
        // note.
        self.total_key() <= other.total_key()
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
    /// `FedrampModerate` stratum. Pins the five pairwise
    /// declaration-order inequalities that
    /// [`impl Lattice for Baseline`]'s `leq` relies on (via the
    /// primary axis of [`Baseline::total_key`]) so a future variant
    /// insertion or rank refactor surfaces here before reaching the
    /// compliance-lattice consumers.
    ///
    /// Note: this test intentionally does NOT assert that `rank` pins
    /// a strict total order across ALL — the SOC2 / PCI-DSS /
    /// FedrampModerate three-way tie at rank 4 is a documented
    /// domain choice (see [`Baseline::rank`]'s inline comments). The
    /// lattice-law commutativity `meet` / `join` need across that
    /// stratum is delivered by [`Baseline::total_key`]'s
    /// (rank, all_index) pairing, pinned by the
    /// `meet_is_commutative_at_the_rank_4_tie_stratum` +
    /// `join_is_commutative_at_the_rank_4_tie_stratum` sibling tests
    /// below and by the exhaustive `..._over_baseline_ALL` sweeps.
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

    // ── total_key substrate + lattice-law seals ──────────────────────
    //
    // Bind [`Baseline::all_index`] + [`Baseline::total_key`] and pin the
    // [`Lattice for Baseline`] impl's law-abiding shape at fail-before-
    // pass-after granularity over the closed set [`Baseline::ALL`].
    //
    // Pre-lift the lattice impl used `self.rank() <= other.rank()` as
    // its ordering test, which broke [`Lattice::meet`] / [`Lattice::join`]
    // commutativity on the documented three-way rank-4 tie stratum
    // (`FedrampModerate`, `Soc2`, `PciDss`): `Soc2.meet(&PciDss)`
    // returned `Soc2` by self-preference while `PciDss.meet(&Soc2)`
    // returned `PciDss` by the same test, violating the top-of-lib
    // `Lattice`-trait docstring's `a ⊓ b = b ⊓ a` law. Post-lift the
    // lattice routes through `total_key` (a total order over `(rank,
    // all_index)`) so `meet` / `join` / `leq` satisfy every lattice law
    // by construction on the closed set. The tests below cover:
    //   1. `all_index` matches [`Baseline::ALL`]'s declaration order
    //      at every slot — the injection substrate the total-order lift
    //      rests on.
    //   2. `total_key` is injective over [`Baseline::ALL`] — no two
    //      variants share a key, so `min`/`max` on the total order
    //      return a UNIQUE element per input pair.
    //   3. The rank-4 tie-stratum commutativity fix, pinned per-pair as
    //      explicit before/after regression seals.
    //   4. Exhaustive lattice laws over `Baseline::ALL` — idempotence,
    //      commutativity, associativity, absorption, `leq` × `meet` /
    //      `join` agreement, bottom / top universality.

    /// [`Baseline::all_index`] matches [`Baseline::ALL`]'s
    /// declaration-order position at every slot. Fail-before-pass-
    /// after: pre-lift `all_index` did not exist as an inherent
    /// method — the total-order tie-break the [`Lattice`] impl now
    /// routes through had no source-of-truth injection to bind to.
    /// Post-lift this test iterates the closed set index-by-index and
    /// asserts `Baseline::ALL[i].all_index() == i as u8` at every
    /// slot; a future variant insertion that extends `ALL` but not
    /// `all_index` (or vice versa) will surface here as a slot
    /// mismatch rather than as silent lattice-law drift downstream.
    #[test]
    #[allow(non_snake_case)]
    fn all_index_matches_declaration_order_position_in_ALL_for_every_variant() {
        for (i, v) in Baseline::ALL.iter().enumerate() {
            assert_eq!(
                v.all_index(),
                i as u8,
                "Baseline::ALL[{i}] = {v:?} has all_index() = {} — the \
                 declaration-order discriminator has drifted away from \
                 the closed set's index, silently breaking the total \
                 order Baseline::total_key routes meet / join / leq \
                 through",
                v.all_index(),
            );
        }
    }

    /// [`Baseline::total_key`] is injective over [`Baseline::ALL`] —
    /// no two variants share a `(rank, all_index)` key, so `min` /
    /// `max` on the total order return a UNIQUE element per input
    /// pair. Consequence: [`Lattice::meet`] / [`Lattice::join`] are
    /// commutative by construction (min / max on any total order are
    /// commutative) AND `a.meet(&b) == a || a.meet(&b) == b` for every
    /// `(a, b)` in `ALL × ALL` (the meet is always one of the inputs,
    /// never a third element).
    #[test]
    #[allow(non_snake_case)]
    fn total_key_is_injective_over_baseline_ALL() {
        use std::collections::HashSet;
        let keys: HashSet<(u8, u8)> = Baseline::ALL.iter().map(|v| v.total_key()).collect();
        assert_eq!(
            keys.len(),
            Baseline::ALL.len(),
            "Baseline::total_key must be injective over Baseline::ALL — \
             two variants sharing a key would collapse the total order \
             back into a pre-order and re-introduce the rank-tie \
             commutativity break the lift severed",
        );
    }

    /// Explicit before/after commutativity seal on the `Soc2` × `PciDss`
    /// pair — the pair the prior-commit follow-up note ("out of scope
    /// for this run") named as breaking `Lattice::meet` commutativity
    /// under the pre-lift rank-only ordering. Post-lift both directions
    /// must return the same variant. The `total_key`-driven `meet`
    /// picks the smaller-`all_index` variant at ties — `Soc2` (index 6)
    /// < `PciDss` (index 7) — so both directions collapse to `Soc2`.
    #[test]
    fn meet_is_commutative_at_the_rank_4_tie_stratum() {
        assert_eq!(
            Baseline::Soc2.meet(&Baseline::PciDss),
            Baseline::PciDss.meet(&Baseline::Soc2),
            "meet must be commutative on the rank-4 tie stratum — \
             pre-lift Soc2.meet(&PciDss)=Soc2 while PciDss.meet(&Soc2)\
             =PciDss under the rank-only ordering test",
        );
        assert_eq!(
            Baseline::FedrampModerate.meet(&Baseline::Soc2),
            Baseline::Soc2.meet(&Baseline::FedrampModerate),
        );
        assert_eq!(
            Baseline::FedrampModerate.meet(&Baseline::PciDss),
            Baseline::PciDss.meet(&Baseline::FedrampModerate),
        );
    }

    /// Sibling of [`meet_is_commutative_at_the_rank_4_tie_stratum`]
    /// on the `join` axis — same three-way tie stratum, same commutativity
    /// obligation, same total-order-driven fix.
    #[test]
    fn join_is_commutative_at_the_rank_4_tie_stratum() {
        assert_eq!(
            Baseline::Soc2.join(&Baseline::PciDss),
            Baseline::PciDss.join(&Baseline::Soc2),
        );
        assert_eq!(
            Baseline::FedrampModerate.join(&Baseline::Soc2),
            Baseline::Soc2.join(&Baseline::FedrampModerate),
        );
        assert_eq!(
            Baseline::FedrampModerate.join(&Baseline::PciDss),
            Baseline::PciDss.join(&Baseline::FedrampModerate),
        );
    }

    /// Exhaustive coverage of the lattice laws (idempotence,
    /// commutativity, absorption, `leq` × `meet` / `join` agreement)
    /// over `Baseline::ALL × Baseline::ALL` — 64 ordered pairs pinned
    /// in one sweep. Bottom / top universality is pinned in the sibling
    /// test below. Associativity is pinned in its own sibling
    /// (`Baseline::ALL × ALL × ALL` = 512 triples) so a regression in
    /// the pairwise laws does not mask under the triple sweep's
    /// aggregation.
    #[test]
    #[allow(non_snake_case)]
    fn meet_join_and_leq_satisfy_pairwise_lattice_laws_over_baseline_ALL() {
        for a in Baseline::ALL {
            // Idempotence — `a ⊓ a = a`, `a ⊔ a = a`.
            assert_eq!(a.meet(&a), a, "meet idempotence failed at {a:?}");
            assert_eq!(a.join(&a), a, "join idempotence failed at {a:?}");
            for b in Baseline::ALL {
                // Commutativity — `a ⊓ b = b ⊓ a`, `a ⊔ b = b ⊔ a`.
                assert_eq!(
                    a.meet(&b),
                    b.meet(&a),
                    "meet commutativity failed at ({a:?}, {b:?})",
                );
                assert_eq!(
                    a.join(&b),
                    b.join(&a),
                    "join commutativity failed at ({a:?}, {b:?})",
                );
                // Meet / join return one of the inputs — a consequence
                // of `total_key`'s injectivity plus the min / max
                // ordering test.
                assert!(
                    a.meet(&b) == a || a.meet(&b) == b,
                    "meet at ({a:?}, {b:?}) returned {:?}, which is neither input",
                    a.meet(&b),
                );
                assert!(
                    a.join(&b) == a || a.join(&b) == b,
                    "join at ({a:?}, {b:?}) returned {:?}, which is neither input",
                    a.join(&b),
                );
                // Absorption — `a ⊓ (a ⊔ b) = a`, `a ⊔ (a ⊓ b) = a`.
                assert_eq!(
                    a.meet(&a.join(&b)),
                    a,
                    "absorption a ⊓ (a ⊔ b) = a failed at ({a:?}, {b:?})",
                );
                assert_eq!(
                    a.join(&a.meet(&b)),
                    a,
                    "absorption a ⊔ (a ⊓ b) = a failed at ({a:?}, {b:?})",
                );
                // `leq` agrees with `meet` and `join` — the backbone
                // identity the top-of-lib docstring promises.
                assert_eq!(
                    a.leq(&b),
                    a.meet(&b) == a,
                    "leq × meet agreement failed at ({a:?}, {b:?})",
                );
                assert_eq!(
                    a.leq(&b),
                    a.join(&b) == b,
                    "leq × join agreement failed at ({a:?}, {b:?})",
                );
            }
        }
    }

    /// Associativity of `meet` / `join` over `Baseline::ALL^3` — 512
    /// ordered triples pinned in one sweep. Kept as its own test so a
    /// regression in the triple sweep does not mask a pairwise-law
    /// regression the sibling
    /// [`meet_join_and_leq_satisfy_pairwise_lattice_laws_over_baseline_ALL`]
    /// covers.
    #[test]
    #[allow(non_snake_case)]
    fn meet_and_join_are_associative_over_baseline_ALL() {
        for a in Baseline::ALL {
            for b in Baseline::ALL {
                for c in Baseline::ALL {
                    assert_eq!(
                        a.meet(&b).meet(&c),
                        a.meet(&b.meet(&c)),
                        "meet associativity failed at ({a:?}, {b:?}, {c:?})",
                    );
                    assert_eq!(
                        a.join(&b).join(&c),
                        a.join(&b.join(&c)),
                        "join associativity failed at ({a:?}, {b:?}, {c:?})",
                    );
                }
            }
        }
    }

    /// Bottom / top universality over `Baseline::ALL` — `bottom() ≤ x`
    /// and `x ≤ top()` for every `x` in the closed set. Pins
    /// `Baseline::None` as the least element and `Baseline::FedrampHigh`
    /// as the greatest under the `total_key`-driven `leq`.
    #[test]
    #[allow(non_snake_case)]
    fn bottom_and_top_are_universal_over_baseline_ALL() {
        for v in Baseline::ALL {
            assert!(
                <Baseline as Lattice>::bottom().leq(&v),
                "bottom ({:?}) must be ≤ every variant — failed at {v:?}",
                <Baseline as Lattice>::bottom(),
            );
            assert!(
                v.leq(&<Baseline as Lattice>::top()),
                "every variant must be ≤ top ({:?}) — failed at {v:?}",
                <Baseline as Lattice>::top(),
            );
        }
    }
}
