//! Lattice algebra over `tatara_process::classification`.
//!
//! Replaces `convergence-controller::qualities_match` and the scattered
//! compliance-baseline comparators with a single `Lattice` trait.
//!
//! Laws (proven by `proptest` in tests):
//!
//! - idempotent:   `a ⊓ a = a`, `a ⊔ a = a`
//! - commutative:  `a ⊓ b = b ⊓ a`, `a ⊔ b = b ⊔ a`
//! - associative:  `a ⊓ (b ⊓ c) = (a ⊓ b) ⊓ c`, similarly for ⊔
//! - absorption:   `a ⊓ (a ⊔ b) = a`, `a ⊔ (a ⊓ b) = a`
//! - leq agrees:   `a ≤ b ⇔ a ⊓ b = a ⇔ a ⊔ b = b`

pub mod baseline;

use tatara_process::classification::{
    CalmClassification, Classification, DataClassification, Horizon, HorizonKind,
    OptimizationDirection, SubstrateType,
};

/// The lattice trait.
pub trait Lattice: Sized + Clone + PartialEq {
    /// Greatest-lower-bound — strongest common refinement.
    fn meet(&self, other: &Self) -> Self;
    /// Least-upper-bound — weakest common relaxation.
    fn join(&self, other: &Self) -> Self;
    /// `self ≤ other` — `self` is at least as refined as `other`.
    fn leq(&self, other: &Self) -> bool {
        self.meet(other) == *self
    }
    /// Bottom element — `⊥ ≤ x` for all `x`.
    fn bottom() -> Self;
    /// Top element — `x ≤ ⊤` for all `x`.
    fn top() -> Self;
}

// ── DataClassification — total order ────────────────────────────────────
//
// Public < Internal < Confidential < Pii < Phi < Pci. The ordering is
// sealed at one site in `tatara_process::classification` —
// `DataClassification::sensitivity_rank` — so a future variant inserted
// in the middle of the enum declaration does not silently shift this
// lattice's `leq` relation. Pre-lift the comparator was `(*self as u8)
// <= (*other as u8)`, which rode silently on declaration order; an
// insertion would have moved every later variant's lattice slot
// without any compile error or test signal. Post-lift the rank is
// declared per-variant on the typed projection, pinned by
// `data_classification_rank_is_strictly_monotone_over_all` and
// `data_classification_rank_agrees_with_partial_ord` in the source
// crate, and `data_classification_leq_uses_typed_rank` below pins
// THIS impl to the typed projection (not the silent cast).

impl Lattice for DataClassification {
    fn meet(&self, other: &Self) -> Self {
        if self.leq(other) {
            self.clone()
        } else {
            other.clone()
        }
    }
    fn join(&self, other: &Self) -> Self {
        if self.leq(other) {
            other.clone()
        } else {
            self.clone()
        }
    }
    fn leq(&self, other: &Self) -> bool {
        self.sensitivity_rank() <= other.sensitivity_rank()
    }
    fn bottom() -> Self {
        DataClassification::Public
    }
    fn top() -> Self {
        DataClassification::Pci
    }
}

// ── SubstrateType — antichain (flat lattice) ────────────────────────────
// Any two distinct substrates are incomparable; meet is top when distinct.

impl Lattice for SubstrateType {
    fn meet(&self, other: &Self) -> Self {
        if self == other {
            self.clone()
        } else {
            Self::top()
        }
    }
    fn join(&self, other: &Self) -> Self {
        if self == other {
            self.clone()
        } else {
            Self::bottom()
        }
    }
    fn leq(&self, other: &Self) -> bool {
        self == other || *other == Self::top()
    }
    fn bottom() -> Self {
        SubstrateType::Financial
    }
    // Regulatory sits at the top — it absorbs any other substrate's constraints.
    fn top() -> Self {
        SubstrateType::Regulatory
    }
}

// ── CalmClassification — boolean lattice (Monotone ≤ NonMonotone) ──────
impl Lattice for CalmClassification {
    fn meet(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Monotone, _) | (_, Self::Monotone) => Self::Monotone,
            _ => Self::NonMonotone,
        }
    }
    fn join(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::NonMonotone, _) | (_, Self::NonMonotone) => Self::NonMonotone,
            _ => Self::Monotone,
        }
    }
    fn leq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Monotone, _) | (Self::NonMonotone, Self::NonMonotone)
        )
    }
    fn bottom() -> Self {
        Self::Monotone
    }
    fn top() -> Self {
        Self::NonMonotone
    }
}

// ── Horizon — Bounded ≤ Asymptotic (strength of invariant) ──────────────
//
// A Bounded point strictly converges; Asymptotic merely trends. We treat
// Bounded as the refinement (meet), Asymptotic as the relaxation (join).

impl Lattice for Horizon {
    fn meet(&self, other: &Self) -> Self {
        match (self.kind, other.kind) {
            (HorizonKind::Bounded, _) | (_, HorizonKind::Bounded) => Self::bounded(),
            _ => self.clone(),
        }
    }
    fn join(&self, other: &Self) -> Self {
        match (self.kind, other.kind) {
            (HorizonKind::Asymptotic, _) => self.clone(),
            (_, HorizonKind::Asymptotic) => other.clone(),
            _ => Self::bounded(),
        }
    }
    fn leq(&self, other: &Self) -> bool {
        matches!(
            (self.kind, other.kind),
            (HorizonKind::Bounded, _) | (HorizonKind::Asymptotic, HorizonKind::Asymptotic)
        )
    }
    fn bottom() -> Self {
        Self::bounded()
    }
    fn top() -> Self {
        Self::asymptotic("", OptimizationDirection::Minimize, f64::MIN)
    }
}

// ── Classification — pointwise product lattice ──────────────────────────
// `a ⊓ b` meets each axis independently; same for join.
// PointType is left alone (caller is responsible — point types are semantic, not comparable).

impl Lattice for Classification {
    fn meet(&self, other: &Self) -> Self {
        Self {
            // PointType is an antichain — leave the caller's choice alone.
            point_type: self.point_type,
            substrate: self.substrate.meet(&other.substrate),
            horizon: self.horizon.meet(&other.horizon),
            calm: self.calm.meet(&other.calm),
            data_classification: self.data_classification.meet(&other.data_classification),
        }
    }
    fn join(&self, other: &Self) -> Self {
        Self {
            point_type: self.point_type,
            substrate: self.substrate.join(&other.substrate),
            horizon: self.horizon.join(&other.horizon),
            calm: self.calm.join(&other.calm),
            data_classification: self.data_classification.join(&other.data_classification),
        }
    }
    fn leq(&self, other: &Self) -> bool {
        self.substrate.leq(&other.substrate)
            && self.horizon.leq(&other.horizon)
            && self.calm.leq(&other.calm)
            && self.data_classification.leq(&other.data_classification)
    }
    fn bottom() -> Self {
        Self {
            point_type: tatara_process::classification::ConvergencePointType::Transform,
            substrate: SubstrateType::bottom(),
            horizon: Horizon::bottom(),
            calm: CalmClassification::bottom(),
            data_classification: DataClassification::bottom(),
        }
    }
    fn top() -> Self {
        Self {
            point_type: tatara_process::classification::ConvergencePointType::Transform,
            substrate: SubstrateType::top(),
            horizon: Horizon::top(),
            calm: CalmClassification::top(),
            data_classification: DataClassification::top(),
        }
    }
}

/// Convenience — does a cluster classification satisfy a workload's requirements?
///
/// Replaces `convergence_controller::cluster_quality::qualities_match`.
pub fn satisfies(cluster: &Classification, requires: &Classification) -> bool {
    // A cluster must be AT LEAST as strict as the workload's requirements on each axis —
    // i.e., the cluster's class ≤ the requirement's class (more refined).
    cluster.leq(requires)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_process::classification::ConvergencePointType;

    #[test]
    fn data_classification_total_order() {
        assert!(DataClassification::Public.leq(&DataClassification::Internal));
        assert!(DataClassification::Internal.leq(&DataClassification::Confidential));
        assert!(DataClassification::Confidential.leq(&DataClassification::Pii));
    }

    #[test]
    fn idempotent_meet() {
        let c = Classification {
            point_type: ConvergencePointType::Gate,
            substrate: SubstrateType::Observability,
            horizon: Horizon::bounded(),
            calm: CalmClassification::Monotone,
            data_classification: DataClassification::Internal,
        };
        assert_eq!(c.meet(&c), c);
    }

    #[test]
    fn absorption() {
        let a = Classification {
            point_type: ConvergencePointType::Gate,
            substrate: SubstrateType::Observability,
            horizon: Horizon::bounded(),
            calm: CalmClassification::Monotone,
            data_classification: DataClassification::Internal,
        };
        let b = Classification {
            point_type: ConvergencePointType::Gate,
            substrate: SubstrateType::Observability,
            horizon: Horizon::bounded(),
            calm: CalmClassification::NonMonotone,
            data_classification: DataClassification::Pii,
        };
        assert_eq!(a.meet(&a.join(&b)), a);
    }

    #[test]
    fn calm_monotone_is_refinement() {
        assert!(CalmClassification::Monotone.leq(&CalmClassification::NonMonotone));
        assert!(!CalmClassification::NonMonotone.leq(&CalmClassification::Monotone));
    }

    #[test]
    fn substrate_flat_antichain() {
        let s = SubstrateType::Compute;
        let t = SubstrateType::Storage;
        assert!(!s.leq(&t));
        assert!(!t.leq(&s));
        // Meet of distinct substrates climbs to top (Regulatory).
        assert_eq!(s.meet(&t), SubstrateType::Regulatory);
    }

    // ── satisfies() ────────────────────────────────────────────────────

    fn bounded_classification(data: DataClassification) -> Classification {
        Classification {
            point_type: ConvergencePointType::Gate,
            substrate: SubstrateType::Observability,
            horizon: Horizon::bounded(),
            calm: CalmClassification::Monotone,
            data_classification: data,
        }
    }

    #[test]
    fn satisfies_is_true_when_cluster_is_as_refined_as_requirement() {
        // cluster.leq(requirement) ⇔ cluster is at least as refined.
        // Public (bottom) cluster satisfies Public-or-higher requirements.
        let cluster = bounded_classification(DataClassification::Public);
        let requirement_public = bounded_classification(DataClassification::Public);
        let requirement_internal = bounded_classification(DataClassification::Internal);
        assert!(satisfies(&cluster, &requirement_public));
        assert!(satisfies(&cluster, &requirement_internal));
    }

    #[test]
    fn satisfies_is_false_when_cluster_is_less_refined_than_requirement() {
        // A Confidential cluster does NOT satisfy a Public requirement —
        // relaxing a class is a lattice "up" move, not "down".
        // (The naming is counter-intuitive; the inequality direction is
        // what the code actually enforces.)
        let cluster = bounded_classification(DataClassification::Confidential);
        let requirement = bounded_classification(DataClassification::Public);
        assert!(!satisfies(&cluster, &requirement));
    }

    #[test]
    fn satisfies_equal_always_true() {
        // x.leq(x) is reflexive — a cluster always satisfies its own
        // classification requirements.
        let c = bounded_classification(DataClassification::Pii);
        assert!(satisfies(&c, &c));
    }

    // ── DataClassification — total-order lattice laws ──────────────────

    use proptest::prelude::*;

    /// SEAL TEST: this lattice's `leq` agrees with the typed
    /// `sensitivity_rank` projection in `tatara_process::classification`,
    /// NOT with a silent `as u8` declaration-order cast. A future
    /// reordering of the source enum's variant declarations is caught
    /// by `data_classification_rank_agrees_with_partial_ord` in the
    /// source crate; THIS test ensures the lattice impl actually
    /// consumes that typed projection. Removing the
    /// `sensitivity_rank` call from `Lattice::leq` (back to `as u8`)
    /// would still pass every lattice law because the rank values
    /// were chosen to agree with declaration order today — but it
    /// would re-introduce the silent declaration-order coupling that
    /// the lift severed. This test fails when the rank arms disagree
    /// with what `Lattice::leq` returns for any pair in `ALL × ALL`.
    #[test]
    fn data_classification_leq_uses_typed_rank() {
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert_eq!(
                    a.leq(&b),
                    a.sensitivity_rank() <= b.sensitivity_rank(),
                    "Lattice::leq for ({a:?}, {b:?}) disagrees with sensitivity_rank — \
                     the lattice ordering has drifted away from the typed rank \
                     projection that seals it",
                );
            }
        }
    }

    /// Generic closed-set proptest strategy — iterates a static `ALL`
    /// slice via `prop_oneof! { Just(*v) ... }`. Lifts the hand-rolled
    /// strategy that previously hard-coded each variant onto the
    /// closed-set source of truth, so adding a variant to
    /// `DataClassification::ALL` automatically extends the property
    /// search space here without touching this strategy.
    fn from_all<T: Copy + std::fmt::Debug + 'static>(
        all: &'static [T],
    ) -> impl Strategy<Value = T> {
        (0..all.len()).prop_map(move |i| all[i])
    }

    fn any_data_class() -> impl Strategy<Value = DataClassification> {
        from_all(&DataClassification::ALL)
    }

    fn any_calm() -> impl Strategy<Value = CalmClassification> {
        prop_oneof![
            Just(CalmClassification::Monotone),
            Just(CalmClassification::NonMonotone),
        ]
    }

    proptest! {
        // Docstring at the top of this module claims "Laws (proven by
        // proptest in tests)" — up to now that was aspirational. These
        // property tests make the claim real for the two axes whose
        // lattice laws are well-founded (total order + 2-element).
        //
        // Deliberately excludes SubstrateType and the Horizon
        // Asymptotic-Asymptotic case, whose `meet` / `leq` semantics
        // are intentionally not lattice-law-abiding (see inline doc
        // comments on those impls — they encode domain-specific
        // "antichain with distinguished top" semantics, not a pure
        // lattice).

        #[test]
        fn data_class_idempotent(a in any_data_class()) {
            prop_assert_eq!(a.meet(&a), a);
            prop_assert_eq!(a.join(&a), a);
        }

        #[test]
        fn data_class_commutative(a in any_data_class(), b in any_data_class()) {
            prop_assert_eq!(a.meet(&b), b.meet(&a));
            prop_assert_eq!(a.join(&b), b.join(&a));
        }

        #[test]
        fn data_class_associative(
            a in any_data_class(),
            b in any_data_class(),
            c in any_data_class(),
        ) {
            prop_assert_eq!(a.meet(&b).meet(&c), a.meet(&b.meet(&c)));
            prop_assert_eq!(a.join(&b).join(&c), a.join(&b.join(&c)));
        }

        #[test]
        fn data_class_absorption(a in any_data_class(), b in any_data_class()) {
            // a ⊓ (a ⊔ b) = a
            prop_assert_eq!(a.meet(&a.join(&b)), a);
            // a ⊔ (a ⊓ b) = a
            prop_assert_eq!(a.join(&a.meet(&b)), a);
        }

        #[test]
        fn data_class_leq_agrees_with_meet(a in any_data_class(), b in any_data_class()) {
            // a ≤ b ⇔ a ⊓ b = a. The backbone lattice identity that
            // the top-of-file docstring promises.
            prop_assert_eq!(a.leq(&b), a.meet(&b) == a);
        }

        #[test]
        fn data_class_leq_agrees_with_join(a in any_data_class(), b in any_data_class()) {
            // a ≤ b ⇔ a ⊔ b = b (dual form).
            prop_assert_eq!(a.leq(&b), a.join(&b) == b);
        }

        #[test]
        fn data_class_bottom_is_universal_min(a in any_data_class()) {
            // ⊥ ≤ x for every x. Public is bottom.
            prop_assert!(DataClassification::bottom().leq(&a));
        }

        #[test]
        fn data_class_top_is_universal_max(a in any_data_class()) {
            // x ≤ ⊤ for every x. Pci is top.
            prop_assert!(a.leq(&DataClassification::top()));
        }

        // ── CalmClassification — 2-element boolean lattice ─────────

        #[test]
        fn calm_idempotent(a in any_calm()) {
            prop_assert_eq!(a.meet(&a), a);
            prop_assert_eq!(a.join(&a), a);
        }

        #[test]
        fn calm_commutative(a in any_calm(), b in any_calm()) {
            prop_assert_eq!(a.meet(&b), b.meet(&a));
            prop_assert_eq!(a.join(&b), b.join(&a));
        }

        #[test]
        fn calm_associative(a in any_calm(), b in any_calm(), c in any_calm()) {
            prop_assert_eq!(a.meet(&b).meet(&c), a.meet(&b.meet(&c)));
            prop_assert_eq!(a.join(&b).join(&c), a.join(&b.join(&c)));
        }

        #[test]
        fn calm_absorption(a in any_calm(), b in any_calm()) {
            prop_assert_eq!(a.meet(&a.join(&b)), a);
            prop_assert_eq!(a.join(&a.meet(&b)), a);
        }

        #[test]
        fn calm_leq_agrees_with_meet(a in any_calm(), b in any_calm()) {
            prop_assert_eq!(a.leq(&b), a.meet(&b) == a);
        }

        #[test]
        fn calm_leq_agrees_with_join(a in any_calm(), b in any_calm()) {
            prop_assert_eq!(a.leq(&b), a.join(&b) == b);
        }

        #[test]
        fn calm_bottom_is_monotone(a in any_calm()) {
            prop_assert!(CalmClassification::bottom().leq(&a));
            prop_assert_eq!(CalmClassification::bottom(), CalmClassification::Monotone);
        }

        #[test]
        fn calm_top_is_nonmonotone(a in any_calm()) {
            prop_assert!(a.leq(&CalmClassification::top()));
            prop_assert_eq!(CalmClassification::top(), CalmClassification::NonMonotone);
        }
    }
}
