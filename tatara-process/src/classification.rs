//! The six classification dimensions — CRD-facing with `JsonSchema`,
//! `From`/`Into` bridges to `tatara_core::domain::classification`.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use tatara_core::domain::classification as core;
use tatara_core::domain::compliance_binding as core_compl;

/// Lattice position of a Process — six orthogonal axes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub point_type: ConvergencePointType,
    pub substrate: SubstrateType,
    #[serde(default)]
    pub horizon: Horizon,
    #[serde(default)]
    pub calm: CalmClassification,
    #[serde(default)]
    pub data_classification: DataClassification,
}

/// Structural type — how data flows through the point.
///
/// Closed-set sibling on the classification axis algebra; the `ALL` /
/// `as_str` / Display / `FromStr` triad mirrors
/// [`DataClassification::ALL`], [`crate::pool::PoolPhase::ALL`],
/// [`crate::pool::MemberState::ALL`], [`crate::pool::ReplacementPolicy::ALL`],
/// [`crate::pool::ReturnPolicy::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`]. The
/// `(input_arity, output_arity)` projection (via [`Arity`]) closes the
/// graph-topology contract: each variant lands in exactly one of the
/// three structural buckets — endomorphic (1→1), diffusive (1→N), or
/// convergent (N→1) — so future DAG composition / edge-cardinality
/// validators dispatch on a typed projection rather than re-deriving
/// from variant names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
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

impl ConvergencePointType {
    /// The closed set of point types — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad AND the
    /// `(input_arity, output_arity)` typed pair (via [`Arity`]) AND the
    /// `is_endomorphic` / `is_diffusive` / `is_convergent` predicate
    /// triple. Adding a ninth variant lands at one `ALL` entry + one
    /// `as_str` arm + one `input_arity` arm + one `output_arity` arm +
    /// one arm per predicate — exhaustively checked by the compiler
    /// (the `[Self; 8]` array literal forces the arity) AND by the
    /// per-variant truth-table contract test (a new variant must
    /// declare its own `(input, output)` arity pair or any future
    /// DAG composition validator that dispatches on
    /// `(input_arity, output_arity)` will silently mis-wire it).
    /// Closes the load-bearing classification-axis enum that
    /// `tatara_core::domain::compliance_binding::PointSelector::ByType`
    /// already dispatches against and that every `Process`'s
    /// `Classification.point_type` reads as the topological identity
    /// of the convergence point.
    pub const ALL: [Self; 8] = [
        Self::Transform,
        Self::Fork,
        Self::Join,
        Self::Gate,
        Self::Select,
        Self::Broadcast,
        Self::Reduce,
        Self::Observe,
    ];

    /// Canonical PascalCase wire-format projection — matches the
    /// serde `rename_all = "PascalCase"` output verbatim AND the CRD
    /// `enum:` enumeration that the Process schema stamps on
    /// `spec.classification.pointType`. Pinned by
    /// `convergence_point_type_as_str_matches_serde` so a variant
    /// rename can't drift between the typed surface, the CRD enum,
    /// the YAML wire format AND any future operator-facing
    /// diagnostic that composes `pointType={kind}` via Display
    /// rather than a hard-coded literal that would silently rot.
    /// Display + FromStr triad over `ALL` mirrors `DataClassification`
    /// / `PoolPhase` / `MemberState` / `ReplacementPolicy` /
    /// `ReturnPolicy` / `TeardownPolicy` / `ConditionKind` /
    /// `ProcessPhase` / `ProcessSignal`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transform => "Transform",
            Self::Fork => "Fork",
            Self::Join => "Join",
            Self::Gate => "Gate",
            Self::Select => "Select",
            Self::Broadcast => "Broadcast",
            Self::Reduce => "Reduce",
            Self::Observe => "Observe",
        }
    }

    /// Cardinality of the input edge into this point — `One` for
    /// `Transform | Fork | Broadcast | Observe` (single-source
    /// projections), `Many` for `Join | Gate | Select | Reduce`
    /// (multi-source convergent reductions). Closed-set match (not
    /// `matches!`) so a future variant triggers the compiler's
    /// exhaustiveness check at this site rather than silently
    /// defaulting to `One`. Paired with [`Self::output_arity`] they
    /// form the typed `(input, output)` projection that future
    /// DAG composition validators (edge-cardinality checks: "you
    /// can't connect a Fork's output to a Transform's input
    /// without a Join in between") dispatch against — a single
    /// projection per variant means a future `Demux` / `Mux` /
    /// `Pipeline` point lands in exactly one cell of the
    /// `Arity × Arity` topology table rather than rotting against
    /// open-coded `== ConvergencePointType::Fork` checks.
    pub const fn input_arity(self) -> Arity {
        match self {
            Self::Transform | Self::Fork | Self::Broadcast | Self::Observe => Arity::One,
            Self::Join | Self::Gate | Self::Select | Self::Reduce => Arity::Many,
        }
    }

    /// Cardinality of the output edge from this point — `Many` for
    /// `Fork | Broadcast` (fan-out), `One` for everything else.
    /// Closed-set match so a future variant triggers the compiler's
    /// exhaustiveness check. See [`Self::input_arity`] for the
    /// arity-pair contract + bucket definitions.
    pub const fn output_arity(self) -> Arity {
        match self {
            Self::Fork | Self::Broadcast => Arity::Many,
            Self::Transform
            | Self::Join
            | Self::Gate
            | Self::Select
            | Self::Reduce
            | Self::Observe => Arity::One,
        }
    }

    /// Does this point preserve the single-input single-output
    /// shape? `(input, output) == (One, One)` — `Transform`
    /// (identity-shaped reshape) and `Observe` (passthrough +
    /// side-channel tap). Closed-set match so a future variant
    /// triggers the compiler's exhaustiveness check. Paired with
    /// `is_diffusive` and `is_convergent` they form the three-way
    /// disjoint bucket carving sealed by
    /// `convergence_point_type_buckets_cover_every_variant` AND
    /// `convergence_point_type_arity_pair_agrees_with_bucket` —
    /// the bridge that lets the bucket predicates and the arity
    /// pair name the same topology partition from two angles.
    pub const fn is_endomorphic(self) -> bool {
        match self {
            Self::Transform | Self::Observe => true,
            Self::Fork
            | Self::Join
            | Self::Gate
            | Self::Select
            | Self::Broadcast
            | Self::Reduce => false,
        }
    }

    /// Does this point fan out — single input replicated/split
    /// across many outputs? `(input, output) == (One, Many)` —
    /// `Fork` and `Broadcast`. Closed-set match so a future variant
    /// triggers the compiler's exhaustiveness check. See
    /// `is_endomorphic` for the bucket-carving contract.
    pub const fn is_diffusive(self) -> bool {
        match self {
            Self::Fork | Self::Broadcast => true,
            Self::Transform
            | Self::Join
            | Self::Gate
            | Self::Select
            | Self::Reduce
            | Self::Observe => false,
        }
    }

    /// Does this point reduce — many inputs collapsed to one
    /// output? `(input, output) == (Many, One)` — `Join`, `Gate`,
    /// `Select`, `Reduce`. Closed-set match so a future variant
    /// triggers the compiler's exhaustiveness check. See
    /// `is_endomorphic` for the bucket-carving contract. The
    /// impossible `(Many, Many)` topology bucket is pinned empty
    /// by `convergence_point_type_arity_pair_agrees_with_bucket`
    /// — a `(Many, Many)` point would mean "many independent
    /// inputs replicated across many independent outputs", which
    /// has no convergence semantics: every DAG-composition
    /// validator would have to special-case it. A future variant
    /// that wants `(Many, Many)` must first extend the bucket
    /// carving deliberately.
    pub const fn is_convergent(self) -> bool {
        match self {
            Self::Join | Self::Gate | Self::Select | Self::Reduce => true,
            Self::Transform | Self::Fork | Self::Broadcast | Self::Observe => false,
        }
    }
}

impl fmt::Display for ConvergencePointType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ConvergencePointType {
    type Err = UnknownConvergencePointType;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for t in Self::ALL {
            if s == t.as_str() {
                return Ok(t);
            }
        }
        Err(UnknownConvergencePointType(s.to_string()))
    }
}

/// Typed parse failure carrying the offending input verbatim so the
/// operator-facing diagnostic surfaces the bad value, not a normalized
/// form. Symmetric to [`UnknownDataClassification`],
/// [`crate::pool::UnknownMemberState`], [`crate::pool::UnknownPoolPhase`],
/// [`crate::pool::UnknownReplacementPolicy`],
/// [`crate::lifetime::UnknownTeardownPolicy`],
/// [`crate::boundary::UnknownConditionKind`],
/// [`crate::phase::UnknownPhase`],
/// [`crate::signal::UnknownSighupStrategy`].
#[derive(Debug, thiserror::Error)]
#[error("unknown convergence point type: {0}")]
pub struct UnknownConvergencePointType(pub String);

/// Edge cardinality of a [`ConvergencePointType`]'s input or output.
///
/// Typed projection used by [`ConvergencePointType::input_arity`] and
/// [`ConvergencePointType::output_arity`] so DAG composition validators
/// reach for a closed-set enum rather than re-deriving the in/out
/// cardinality from variant names. `Many` is the "≥1, could be N"
/// cardinality — it carries no upper bound because the convergence
/// point's variant tag is already the structural identity; the
/// number itself is a runtime property of the DAG, not the typescape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Arity {
    /// Single edge — exactly one input or one output.
    One,
    /// Multiple edges — any number ≥ 1.
    Many,
}

impl Arity {
    /// The closed set of arities — single source of truth that
    /// drives `as_str` / Display AND the `is_one` predicate. Adding
    /// a third variant (e.g. `Arity::Zero` for sinks) lands at one
    /// `ALL` entry + one `as_str` arm + one predicate arm —
    /// exhaustively checked by the compiler.
    pub const ALL: [Self; 2] = [Self::One, Self::Many];

    /// Canonical projection — `"One" | "Many"`. Pinned by
    /// `arity_display_matches_as_str` so a future Display impl
    /// can't drift from the canonical string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::One => "One",
            Self::Many => "Many",
        }
    }

    /// Is this the single-edge cardinality? Closed-set match (not
    /// `matches!`) so a future variant triggers the compiler's
    /// exhaustiveness check.
    pub const fn is_one(self) -> bool {
        match self {
            Self::One => true,
            Self::Many => false,
        }
    }
}

impl fmt::Display for Arity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Operational substrate.
///
/// Closed-set sibling on the classification axis algebra; the `ALL` /
/// `as_str` / Display / `FromStr` triad mirrors
/// [`ConvergencePointType::ALL`], [`DataClassification::ALL`],
/// [`crate::pool::PoolPhase::ALL`], [`crate::pool::MemberState::ALL`],
/// [`crate::pool::ReplacementPolicy::ALL`],
/// [`crate::pool::ReturnPolicy::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`]. The
/// `is_resource` / `is_policy` / `is_telemetry` predicate triple
/// carves the eight variants into three structurally-disjoint
/// substrate planes — resource (you allocate from it), policy (it
/// gates access for other workloads), telemetry (it observes other
/// workloads) — so future compliance-baseline selectors that
/// dispatch on a substrate's plane (resource budgets only apply to
/// resource substrates; policy substrates inherit baselines from
/// what they govern; telemetry substrates inherit baselines from
/// what they observe) read a typed projection rather than
/// re-deriving from variant names.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "PascalCase")]
pub enum SubstrateType {
    Financial,
    Compute,
    Network,
    Storage,
    Security,
    Identity,
    Observability,
    Regulatory,
}

impl SubstrateType {
    /// The closed set of substrates — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad AND the
    /// `is_resource` / `is_policy` / `is_telemetry` predicate triple.
    /// Adding a ninth variant lands at one `ALL` entry + one
    /// `as_str` arm + one arm per predicate — exhaustively checked
    /// by the compiler (the `[Self; 8]` array literal forces the
    /// arity) AND by the per-variant plane-bucket contract test (a
    /// new variant must declare its own plane or any future
    /// compliance-baseline selector that dispatches on
    /// `(is_resource, is_policy, is_telemetry)` will silently
    /// mis-classify it). Closes the load-bearing classification-axis
    /// enum that
    /// `tatara_core::domain::compliance_binding::PointSelector::BySubstrate`
    /// already dispatches against and that every `Process`'s
    /// `Classification.substrate` reads as the operational
    /// substrate the convergence point lives on.
    pub const ALL: [Self; 8] = [
        Self::Financial,
        Self::Compute,
        Self::Network,
        Self::Storage,
        Self::Security,
        Self::Identity,
        Self::Observability,
        Self::Regulatory,
    ];

    /// Canonical PascalCase wire-format projection — matches the
    /// serde `rename_all = "PascalCase"` output verbatim AND the CRD
    /// `enum:` enumeration that the Process schema stamps on
    /// `spec.classification.substrate`. Pinned by
    /// `substrate_type_as_str_matches_serde` so a variant rename
    /// can't drift between the typed surface, the CRD enum, the YAML
    /// wire format AND any future operator-facing diagnostic that
    /// composes `substrate={kind}` via Display rather than a
    /// hard-coded literal that would silently rot. Display + FromStr
    /// triad over `ALL` mirrors `ConvergencePointType` /
    /// `DataClassification` / `PoolPhase` / `MemberState` /
    /// `ReplacementPolicy` / `ReturnPolicy` / `TeardownPolicy` /
    /// `ConditionKind` / `ProcessPhase` / `ProcessSignal`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Financial => "Financial",
            Self::Compute => "Compute",
            Self::Network => "Network",
            Self::Storage => "Storage",
            Self::Security => "Security",
            Self::Identity => "Identity",
            Self::Observability => "Observability",
            Self::Regulatory => "Regulatory",
        }
    }

    /// Is this a resource substrate — one you allocate budgets from
    /// to run workloads? `Financial | Compute | Network | Storage`.
    /// Closed-set match (not `matches!`) so a future variant
    /// triggers the compiler's exhaustiveness check at this site
    /// rather than silently defaulting to `false`. Paired with
    /// `is_policy` and `is_telemetry` they form the three-way
    /// disjoint plane carving sealed by
    /// `substrate_type_buckets_cover_every_variant` — the bridge
    /// that lets future compliance-baseline selectors dispatch on
    /// plane without re-deriving from variant names.
    pub const fn is_resource(self) -> bool {
        match self {
            Self::Financial | Self::Compute | Self::Network | Self::Storage => true,
            Self::Security | Self::Identity | Self::Observability | Self::Regulatory => false,
        }
    }

    /// Is this a policy substrate — one that gates access or
    /// compliance for other workloads rather than carrying their
    /// payload? `Security | Identity | Regulatory`. Closed-set match
    /// so a future variant triggers the compiler's exhaustiveness
    /// check. See `is_resource` for the bucket-carving contract.
    pub const fn is_policy(self) -> bool {
        match self {
            Self::Security | Self::Identity | Self::Regulatory => true,
            Self::Financial
            | Self::Compute
            | Self::Network
            | Self::Storage
            | Self::Observability => false,
        }
    }

    /// Is this a telemetry substrate — one that passively observes
    /// other workloads (metrics, logs, traces) without carrying
    /// their payload or gating their access? `Observability` only.
    /// Closed-set match so a future variant triggers the compiler's
    /// exhaustiveness check. See `is_resource` for the
    /// bucket-carving contract. A telemetry substrate's compliance
    /// baseline is inherited from what it observes — the singleton
    /// bucket is intentional, not a placeholder.
    pub const fn is_telemetry(self) -> bool {
        match self {
            Self::Observability => true,
            Self::Financial
            | Self::Compute
            | Self::Network
            | Self::Storage
            | Self::Security
            | Self::Identity
            | Self::Regulatory => false,
        }
    }
}

impl fmt::Display for SubstrateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SubstrateType {
    type Err = UnknownSubstrateType;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for t in Self::ALL {
            if s == t.as_str() {
                return Ok(t);
            }
        }
        Err(UnknownSubstrateType(s.to_string()))
    }
}

/// Typed parse failure carrying the offending input verbatim so the
/// operator-facing diagnostic surfaces the bad value, not a normalized
/// form. Symmetric to [`UnknownConvergencePointType`],
/// [`UnknownDataClassification`], [`crate::pool::UnknownMemberState`],
/// [`crate::pool::UnknownPoolPhase`],
/// [`crate::pool::UnknownReplacementPolicy`],
/// [`crate::lifetime::UnknownTeardownPolicy`],
/// [`crate::boundary::UnknownConditionKind`],
/// [`crate::phase::UnknownPhase`],
/// [`crate::signal::UnknownSighupStrategy`].
#[derive(Debug, thiserror::Error)]
#[error("unknown substrate type: {0}")]
pub struct UnknownSubstrateType(pub String);

/// How long the point runs. Flattened struct-of-optionals so the OpenAPI
/// schema carries a single `kind` discriminator without per-variant merge.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Horizon {
    #[serde(default)]
    pub kind: HorizonKind,
    /// Metric being optimized (Asymptotic only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    /// Whether to minimize or maximize the metric (Asymptotic only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<OptimizationDirection>,
    /// Rate threshold considered healthy (Asymptotic only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthy_rate_threshold: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
pub enum HorizonKind {
    /// Has a fixed point — distance reaches 0 and terminates.
    #[default]
    Bounded,
    /// Runs in perpetuity — rate is the health signal, not distance.
    Asymptotic,
}

impl Horizon {
    pub fn bounded() -> Self {
        Self::default()
    }

    pub fn asymptotic(
        metric: impl Into<String>,
        direction: OptimizationDirection,
        threshold: f64,
    ) -> Self {
        Self {
            kind: HorizonKind::Asymptotic,
            metric: Some(metric.into()),
            direction: Some(direction),
            healthy_rate_threshold: Some(threshold),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum OptimizationDirection {
    Minimize,
    Maximize,
}

/// CALM theorem classification — determines whether coordination is required.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
pub enum CalmClassification {
    #[default]
    Monotone,
    NonMonotone,
}

/// Data sensitivity, drives compliance baseline selection.
///
/// Sibling closed-set on the classification axis algebra; the `ALL` /
/// `as_str` / Display / `FromStr` triad mirrors
/// [`crate::pool::PoolPhase::ALL`], [`crate::pool::MemberState::ALL`],
/// [`crate::pool::ReplacementPolicy::ALL`],
/// [`crate::pool::ReturnPolicy::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`].
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Default,
)]
#[serde(rename_all = "PascalCase")]
pub enum DataClassification {
    Public,
    #[default]
    Internal,
    Confidential,
    Pii,
    Phi,
    Pci,
}

impl DataClassification {
    /// The closed set of data classifications — single source of truth
    /// that drives the `as_str` / Display / `FromStr` triad AND the
    /// `sensitivity_rank` total-order projection AND the
    /// `is_restricted` / `is_regulated` predicate pair. Adding a
    /// seventh variant lands at one `ALL` entry + one `as_str` arm +
    /// one `sensitivity_rank` arm + one arm per predicate —
    /// exhaustively checked by the compiler (the `[Self; 6]` array
    /// literal forces the arity) AND by the per-variant truth-table
    /// contract test (a new variant must declare its own
    /// `(is_restricted, is_regulated)` bucket or any future
    /// compliance-baseline auto-selector that dispatches on the pair
    /// will silently bucket it into the wrong sensitivity column).
    /// This closes the sixth classification-axis enum and the closure
    /// is consumed by [`tatara_lattice`]'s total-order `Lattice` impl
    /// via [`Self::sensitivity_rank`] so the lattice ordering no
    /// longer rides silently on declaration order.
    pub const ALL: [Self; 6] = [
        Self::Public,
        Self::Internal,
        Self::Confidential,
        Self::Pii,
        Self::Phi,
        Self::Pci,
    ];

    /// Canonical PascalCase wire-format projection — matches the
    /// serde `rename_all = "PascalCase"` output verbatim AND the CRD
    /// `enum:` enumeration that the Process schema stamps on
    /// `spec.classification.dataClassification`. Pinned by
    /// `data_classification_as_str_matches_serde` so a variant rename
    /// can't drift between the typed surface, the CRD enum, the YAML
    /// wire format AND any future operator-facing diagnostic that
    /// composes `dataClassification={class}` via Display rather than
    /// a hard-coded literal that would silently rot. Display +
    /// FromStr triad over `ALL` mirrors `PoolPhase` / `MemberState` /
    /// `ReplacementPolicy` / `ReturnPolicy` / `TeardownPolicy` /
    /// `ConditionKind` / `ProcessPhase` / `ProcessSignal`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "Public",
            Self::Internal => "Internal",
            Self::Confidential => "Confidential",
            Self::Pii => "Pii",
            Self::Phi => "Phi",
            Self::Pci => "Pci",
        }
    }

    /// Explicit total-order rank, sealed at one site so the lattice
    /// ordering stops riding silently on declaration order. Pre-lift
    /// the tatara-lattice `Lattice for DataClassification` impl
    /// compared variants via `(*self as u8) <= (*other as u8)`, so a
    /// future variant inserted in the middle of the enum (say a
    /// `Restricted` between `Internal` and `Confidential`) would
    /// silently shift every subsequent variant's `as u8` value AND
    /// the lattice's `leq` relation — no compile error, no test
    /// failure, but every compliance-baseline comparison
    /// downstream would have moved by one slot. Post-lift the rank
    /// is declared explicitly per variant; an insertion forces the
    /// author to pick a rank deliberately (and
    /// `data_classification_rank_is_strictly_monotone_over_all`
    /// pins the existing six variants at 0..6 so the lattice's
    /// total order remains the documented
    /// `Public < Internal < Confidential < Pii < Phi < Pci`).
    pub const fn sensitivity_rank(self) -> u8 {
        match self {
            Self::Public => 0,
            Self::Internal => 1,
            Self::Confidential => 2,
            Self::Pii => 3,
            Self::Phi => 4,
            Self::Pci => 5,
        }
    }

    /// Is this classification subject to external regulatory regime
    /// (HIPAA / PCI-DSS / GDPR-style data-subject controls)?
    /// Closed-set match (not `matches!`) so a future variant triggers
    /// the compiler's exhaustiveness check at this site rather than
    /// silently defaulting to `false`. Paired with `is_restricted`
    /// they form the two-axis projection that future
    /// compliance-baseline auto-selectors dispatch against —
    /// `(false, false)` ⇒ freely distributable (`Public`);
    /// `(false, true)` ⇒ access-controlled but not regulated
    /// (`Internal | Confidential`); `(true, true)` ⇒ regulated data
    /// that implies access control (`Pii | Phi | Pci`). The
    /// impossible bucket `(true, false)` — regulated data without
    /// access control — is pinned empty by
    /// `data_classification_regulated_implies_restricted`.
    pub const fn is_regulated(self) -> bool {
        match self {
            Self::Pii | Self::Phi | Self::Pci => true,
            Self::Public | Self::Internal | Self::Confidential => false,
        }
    }

    /// Does this classification require access controls beyond
    /// freely-distributable? Closed-set match so a future variant
    /// triggers the compiler's exhaustiveness check. See
    /// `is_regulated` for the predicate-pair contract + bucket
    /// definitions.
    pub const fn is_restricted(self) -> bool {
        match self {
            Self::Public => false,
            Self::Internal | Self::Confidential | Self::Pii | Self::Phi | Self::Pci => true,
        }
    }
}

impl fmt::Display for DataClassification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DataClassification {
    type Err = UnknownDataClassification;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for class in Self::ALL {
            if s == class.as_str() {
                return Ok(class);
            }
        }
        Err(UnknownDataClassification(s.to_string()))
    }
}

/// Typed parse failure carrying the offending input verbatim so the
/// operator-facing diagnostic surfaces the bad value, not a normalized
/// form. Symmetric to [`crate::pool::UnknownMemberState`],
/// [`crate::pool::UnknownPoolPhase`],
/// [`crate::pool::UnknownReplacementPolicy`],
/// [`crate::lifetime::UnknownTeardownPolicy`],
/// [`crate::boundary::UnknownConditionKind`],
/// [`crate::phase::UnknownPhase`],
/// [`crate::signal::UnknownSighupStrategy`].
#[derive(Debug, thiserror::Error)]
#[error("unknown data classification: {0}")]
pub struct UnknownDataClassification(pub String);

// ───────────────────────────── bridges to tatara-core ─────────────────

impl From<ConvergencePointType> for core::ConvergencePointType {
    fn from(v: ConvergencePointType) -> Self {
        use ConvergencePointType::*;
        match v {
            Transform => Self::Transform,
            Fork => Self::Fork,
            Join => Self::Join,
            Gate => Self::Gate,
            Select => Self::Select,
            Broadcast => Self::Broadcast,
            Reduce => Self::Reduce,
            Observe => Self::Observe,
        }
    }
}

impl From<core::ConvergencePointType> for ConvergencePointType {
    fn from(v: core::ConvergencePointType) -> Self {
        use core::ConvergencePointType as C;
        match v {
            C::Transform => Self::Transform,
            C::Fork => Self::Fork,
            C::Join => Self::Join,
            C::Gate => Self::Gate,
            C::Select => Self::Select,
            C::Broadcast => Self::Broadcast,
            C::Reduce => Self::Reduce,
            C::Observe => Self::Observe,
        }
    }
}

impl From<SubstrateType> for core::SubstrateType {
    fn from(v: SubstrateType) -> Self {
        use SubstrateType::*;
        match v {
            Financial => Self::Financial,
            Compute => Self::Compute,
            Network => Self::Network,
            Storage => Self::Storage,
            Security => Self::Security,
            Identity => Self::Identity,
            Observability => Self::Observability,
            Regulatory => Self::Regulatory,
        }
    }
}

impl From<core::SubstrateType> for SubstrateType {
    fn from(v: core::SubstrateType) -> Self {
        use core::SubstrateType as C;
        match v {
            C::Financial => Self::Financial,
            C::Compute => Self::Compute,
            C::Network => Self::Network,
            C::Storage => Self::Storage,
            C::Security => Self::Security,
            C::Identity => Self::Identity,
            C::Observability => Self::Observability,
            C::Regulatory => Self::Regulatory,
        }
    }
}

impl From<OptimizationDirection> for core::OptimizationDirection {
    fn from(v: OptimizationDirection) -> Self {
        match v {
            OptimizationDirection::Minimize => Self::Minimize,
            OptimizationDirection::Maximize => Self::Maximize,
        }
    }
}

impl From<Horizon> for core::ConvergenceHorizon {
    fn from(v: Horizon) -> Self {
        match v.kind {
            HorizonKind::Bounded => Self::Bounded,
            HorizonKind::Asymptotic => Self::Asymptotic {
                metric: v.metric.unwrap_or_default(),
                direction: v
                    .direction
                    .unwrap_or(OptimizationDirection::Minimize)
                    .into(),
                healthy_rate_threshold: v.healthy_rate_threshold.unwrap_or_default(),
            },
        }
    }
}

impl From<CalmClassification> for core::CalmClassification {
    fn from(v: CalmClassification) -> Self {
        match v {
            CalmClassification::Monotone => Self::Monotone,
            CalmClassification::NonMonotone => Self::NonMonotone,
        }
    }
}

impl From<DataClassification> for core_compl::DataClassification {
    fn from(v: DataClassification) -> Self {
        use DataClassification::*;
        match v {
            Public => Self::Public,
            Internal => Self::Internal,
            Confidential => Self::Confidential,
            Pii => Self::Pii,
            Phi => Self::Phi,
            Pci => Self::Pci,
        }
    }
}

impl From<core_compl::DataClassification> for DataClassification {
    fn from(v: core_compl::DataClassification) -> Self {
        use core_compl::DataClassification as C;
        match v {
            C::Public => Self::Public,
            C::Internal => Self::Internal,
            C::Confidential => Self::Confidential,
            C::Pii => Self::Pii,
            C::Phi => Self::Phi,
            C::Pci => Self::Pci,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_roundtrip() {
        let pt: core::ConvergencePointType = ConvergencePointType::Gate.into();
        let back: ConvergencePointType = pt.into();
        assert_eq!(back, ConvergencePointType::Gate);

        let sub: core::SubstrateType = SubstrateType::Observability.into();
        let back: SubstrateType = sub.into();
        assert_eq!(back, SubstrateType::Observability);
    }

    #[test]
    fn data_classification_ordering() {
        assert!(DataClassification::Public < DataClassification::Pii);
        assert!(DataClassification::Internal < DataClassification::Confidential);
    }

    #[test]
    fn horizon_default_is_bounded() {
        assert_eq!(Horizon::default().kind, HorizonKind::Bounded);
    }

    // ── closed-set algebra contracts for DataClassification
    //    (ALL × as_str × FromStr × rank × predicate pair) ────────────

    /// `ALL` is the source of truth — pin its closure so a variant
    /// added without an `ALL` entry fails here via the uniqueness check
    /// before drifting `FromStr` or the sweep tests below. The arity is
    /// asserted by the `[Self; 6]` array type itself.
    #[test]
    fn data_classification_all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for class in DataClassification::ALL {
            assert!(seen.insert(class), "duplicate variant in ALL: {class:?}");
        }
        assert_eq!(seen.len(), DataClassification::ALL.len());
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the YAML
    /// wire format the reconciler stamps on
    /// `spec.classification.dataClassification`.
    #[test]
    fn data_classification_as_str_matches_serde() {
        for class in DataClassification::ALL {
            let serialized = serde_json::to_string(&class).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                class.as_str(),
                "as_str drift for {class:?}: as_str={} serde={unquoted}",
                class.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. Any operator-facing
    /// "dataClassification={class}" diagnostic that composes through
    /// Display inherits the canonical wire-format string automatically.
    #[test]
    fn data_classification_display_matches_as_str() {
        for class in DataClassification::ALL {
            assert_eq!(class.to_string(), class.as_str());
        }
    }

    /// Every variant in ALL round-trips through `as_str` ↔ `FromStr`.
    /// Adding a variant without extending `as_str` / `FromStr`'s sweep
    /// of `ALL` fails here.
    #[test]
    fn data_classification_roundtrip_via_as_str() {
        for class in DataClassification::ALL {
            assert_eq!(
                DataClassification::from_str(class.as_str()).unwrap(),
                class,
                "round-trip failed for {class:?}"
            );
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — empty / lowercased / typo / cross-axis-leaked — and
    /// the error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    #[test]
    fn unknown_data_classification_errors() {
        for bad in [
            "",
            "pii",          // lowercased
            "PII",          // uppercased
            "PersonalData", // typo
            "internal_data",
            "Steady",   // PoolPhase-axis leak
            "Replace",  // ReturnPolicy-axis leak
            "Attested", // ProcessPhase-axis leak
            "Compute",  // SubstrateType-axis leak
            "Gate",     // ConvergencePointType-axis leak
            "Monotone", // CalmClassification-axis leak
        ] {
            let err = DataClassification::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// TRUTH-TABLE CONTRACT: the predicate pair agrees with the
    /// documented per-variant compliance role. Pinning this table at
    /// one site means any future compliance-baseline auto-selector
    /// reads the same projection that the reconciler writes.
    #[test]
    fn data_classification_predicate_truth_tables() {
        assert!(!DataClassification::Public.is_restricted());
        assert!(!DataClassification::Public.is_regulated());

        assert!(DataClassification::Internal.is_restricted());
        assert!(!DataClassification::Internal.is_regulated());

        assert!(DataClassification::Confidential.is_restricted());
        assert!(!DataClassification::Confidential.is_regulated());

        assert!(DataClassification::Pii.is_restricted());
        assert!(DataClassification::Pii.is_regulated());

        assert!(DataClassification::Phi.is_restricted());
        assert!(DataClassification::Phi.is_regulated());

        assert!(DataClassification::Pci.is_restricted());
        assert!(DataClassification::Pci.is_regulated());
    }

    /// IMPLICATION CONTRACT: every regulated classification is also
    /// restricted. The impossible bucket (regulated AND
    /// freely-distributable) is pinned empty so a future variant that
    /// returned `(true, false)` from the predicate pair would FAIL
    /// here, forcing the author to either flip `is_restricted` or
    /// extend the consumer dispatch sites (compliance-baseline
    /// auto-selector, audit-log mandatory-fields validator)
    /// deliberately rather than silently producing a regulated class
    /// the API server would accept as freely-distributable. Encoded as
    /// material implication `is_regulated → is_restricted` so the
    /// boolean reads as the documented contract, not its NAND form.
    #[test]
    fn data_classification_regulated_implies_restricted() {
        for class in DataClassification::ALL {
            assert!(
                !class.is_regulated() || class.is_restricted(),
                "{class:?} is regulated but not restricted — \
                 regulated data is by definition not freely distributable",
            );
        }
    }

    /// COVERAGE CONTRACT: every variant lands in exactly one of three
    /// compliance buckets — freely distributable (`Public`),
    /// restricted-only (`Internal | Confidential`), or regulated
    /// (`Pii | Phi | Pci`). Pins the three buckets at their declared
    /// cardinalities (1, 2, 3 — sum to `ALL.len()`) so a future
    /// variant lands somewhere deliberately.
    #[test]
    fn data_classification_buckets_cover_every_variant() {
        let mut free = 0u32;
        let mut restricted_only = 0u32;
        let mut regulated = 0u32;
        for class in DataClassification::ALL {
            match (class.is_restricted(), class.is_regulated()) {
                (false, false) => free += 1,
                (true, false) => restricted_only += 1,
                (true, true) => regulated += 1,
                (false, true) => {
                    panic!("regulated_implies_restricted already pins this empty for {class:?}")
                }
            }
        }
        assert_eq!(free, 1, "free bucket: Public");
        assert_eq!(
            restricted_only, 2,
            "restricted-only bucket: Internal + Confidential"
        );
        assert_eq!(regulated, 3, "regulated bucket: Pii + Phi + Pci");
        assert_eq!(
            free + restricted_only + regulated,
            DataClassification::ALL.len() as u32
        );
    }

    /// MONOTONE-RANK CONTRACT: `sensitivity_rank` is strictly
    /// monotone over `ALL`'s declared order, so the lattice ordering
    /// `Public < Internal < Confidential < Pii < Phi < Pci` is sealed
    /// at one site (this enum's projection) instead of riding on the
    /// silent `as u8` cast in [`tatara_lattice`]. A future variant
    /// inserted in the middle would either preserve strict monotonicity
    /// here (and the lattice keeps working) or FAIL here at compile or
    /// test time (and the author has to renumber deliberately). Also
    /// pins the rank codomain at `0..ALL.len()` so no variant can
    /// silently outrank the documented top.
    #[test]
    fn data_classification_rank_is_strictly_monotone_over_all() {
        let ranks: Vec<u8> = DataClassification::ALL
            .into_iter()
            .map(DataClassification::sensitivity_rank)
            .collect();
        for win in ranks.windows(2) {
            assert!(win[0] < win[1], "ranks not strictly monotone: {ranks:?}");
        }
        assert_eq!(*ranks.first().unwrap(), 0, "bottom rank must be 0");
        assert_eq!(
            *ranks.last().unwrap(),
            (DataClassification::ALL.len() as u8) - 1,
            "top rank must be ALL.len() - 1"
        );
    }

    /// RANK-AGREES-WITH-ORD CONTRACT: the typed `sensitivity_rank`
    /// projection agrees with the derived `PartialOrd` / `Ord` for
    /// every pair in `ALL × ALL`. This is the bridge that lets
    /// [`tatara_lattice`]'s total-order `Lattice for DataClassification`
    /// impl call `sensitivity_rank` instead of `as u8` without changing
    /// any observable lattice behavior — and it lets a future
    /// reordering of the enum's variant declarations land at this test
    /// site (forcing the rank arms to be renumbered) rather than
    /// silently shifting the lattice's `leq` relation.
    #[test]
    fn data_classification_rank_agrees_with_partial_ord() {
        for a in DataClassification::ALL {
            for b in DataClassification::ALL {
                assert_eq!(
                    a.sensitivity_rank() <= b.sensitivity_rank(),
                    a <= b,
                    "rank vs. PartialOrd drift on ({a:?}, {b:?})"
                );
            }
        }
    }

    /// DEFAULT-AGREEMENT CONTRACT: `DataClassification::default()`
    /// returns `Internal` (the variant tagged `#[default]`), AND that
    /// variant lands in the restricted-only bucket — neither freely
    /// distributable nor externally regulated. A future `#[default]`
    /// rename without flipping the predicates fails here.
    #[test]
    fn data_classification_default_is_internal_in_restricted_only_bucket() {
        let d = DataClassification::default();
        assert_eq!(d, DataClassification::Internal);
        assert!(d.is_restricted());
        assert!(!d.is_regulated());
        assert_eq!(d.sensitivity_rank(), 1);
    }

    /// BRIDGE ROUND-TRIP CONTRACT: every variant survives the
    /// CRD-facing (`PascalCase`) ↔ tatara-core (`snake_case`)
    /// `From` hop. Today the bridge is two hand-written 6-arm matches
    /// in this file; pinning the round-trip over `ALL` means a future
    /// variant added without extending the bridge fails here at one
    /// site instead of drifting between the CRD wire format and the
    /// `core_compl::DataClassification` selector axis.
    #[test]
    fn data_classification_bridge_roundtrip_over_all() {
        for class in DataClassification::ALL {
            let core: core_compl::DataClassification = class.into();
            let back: DataClassification = core.into();
            assert_eq!(back, class, "bridge round-trip failed for {class:?}");
        }
    }

    // ── closed-set algebra contracts for ConvergencePointType
    //    (ALL × as_str × FromStr × arity-pair × predicate triple) ────

    /// `ALL` is the source of truth — pin its closure so a variant
    /// added without an `ALL` entry fails here via the uniqueness
    /// check before drifting `FromStr` or the sweep tests below. The
    /// arity is asserted by the `[Self; 8]` array type itself.
    #[test]
    fn convergence_point_type_all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for t in ConvergencePointType::ALL {
            assert!(seen.insert(t), "duplicate variant in ALL: {t:?}");
        }
        assert_eq!(seen.len(), ConvergencePointType::ALL.len());
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the YAML
    /// wire format the reconciler reads from
    /// `spec.classification.pointType`.
    #[test]
    fn convergence_point_type_as_str_matches_serde() {
        for t in ConvergencePointType::ALL {
            let serialized = serde_json::to_string(&t).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                t.as_str(),
                "as_str drift for {t:?}: as_str={} serde={unquoted}",
                t.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift.
    #[test]
    fn convergence_point_type_display_matches_as_str() {
        for t in ConvergencePointType::ALL {
            assert_eq!(t.to_string(), t.as_str());
        }
    }

    /// Every variant in ALL round-trips through `as_str` ↔ `FromStr`.
    /// Adding a variant without extending `as_str` / `FromStr`'s sweep
    /// of `ALL` fails here.
    #[test]
    fn convergence_point_type_roundtrip_via_as_str() {
        for t in ConvergencePointType::ALL {
            assert_eq!(
                ConvergencePointType::from_str(t.as_str()).unwrap(),
                t,
                "round-trip failed for {t:?}"
            );
        }
    }

    /// `FromStr` rejects strings outside the canonical projection —
    /// empty / lowercased / typo / cross-axis-leaked — and the error
    /// echoes the input verbatim so the operator-facing diagnostic
    /// surfaces the bad value, not a normalized form.
    #[test]
    fn unknown_convergence_point_type_errors() {
        for bad in [
            "",
            "gate",       // lowercased
            "GATE",       // uppercased
            "Transformr", // typo
            "Filter",
            "Steady",   // PoolPhase-axis leak
            "Pii",      // DataClassification-axis leak
            "Attested", // ProcessPhase-axis leak
            "Compute",  // SubstrateType-axis leak
            "Monotone", // CalmClassification-axis leak
            "PromQL",   // ConditionKind-axis leak
        ] {
            let err = ConvergencePointType::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// TRUTH-TABLE CONTRACT: the predicate triple agrees with the
    /// documented per-variant topology role. Pinning this table at
    /// one site means any future DAG validator reads the same
    /// projection that compliance bindings dispatch against.
    #[test]
    fn convergence_point_type_predicate_truth_tables() {
        // Endomorphic: 1→1
        assert!(ConvergencePointType::Transform.is_endomorphic());
        assert!(!ConvergencePointType::Transform.is_diffusive());
        assert!(!ConvergencePointType::Transform.is_convergent());

        assert!(ConvergencePointType::Observe.is_endomorphic());
        assert!(!ConvergencePointType::Observe.is_diffusive());
        assert!(!ConvergencePointType::Observe.is_convergent());

        // Diffusive: 1→N
        assert!(!ConvergencePointType::Fork.is_endomorphic());
        assert!(ConvergencePointType::Fork.is_diffusive());
        assert!(!ConvergencePointType::Fork.is_convergent());

        assert!(!ConvergencePointType::Broadcast.is_endomorphic());
        assert!(ConvergencePointType::Broadcast.is_diffusive());
        assert!(!ConvergencePointType::Broadcast.is_convergent());

        // Convergent: N→1
        for t in [
            ConvergencePointType::Join,
            ConvergencePointType::Gate,
            ConvergencePointType::Select,
            ConvergencePointType::Reduce,
        ] {
            assert!(!t.is_endomorphic(), "{t:?} should not be endomorphic");
            assert!(!t.is_diffusive(), "{t:?} should not be diffusive");
            assert!(t.is_convergent(), "{t:?} should be convergent");
        }
    }

    /// COVERAGE CONTRACT: every variant lands in *exactly one* of the
    /// three topology buckets — endomorphic, diffusive, or convergent.
    /// Pins the three buckets at their declared cardinalities (2, 2, 4
    /// — sum to `ALL.len()`) so a future variant lands somewhere
    /// deliberately. No variant returns true from more than one
    /// predicate; no variant returns false from all three.
    #[test]
    fn convergence_point_type_buckets_cover_every_variant() {
        let mut endomorphic = 0u32;
        let mut diffusive = 0u32;
        let mut convergent = 0u32;
        for t in ConvergencePointType::ALL {
            let buckets = [t.is_endomorphic(), t.is_diffusive(), t.is_convergent()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "{t:?} landed in {hits} buckets: {buckets:?} (must be exactly one)"
            );
            if t.is_endomorphic() {
                endomorphic += 1;
            }
            if t.is_diffusive() {
                diffusive += 1;
            }
            if t.is_convergent() {
                convergent += 1;
            }
        }
        assert_eq!(endomorphic, 2, "endomorphic bucket: Transform + Observe");
        assert_eq!(diffusive, 2, "diffusive bucket: Fork + Broadcast");
        assert_eq!(
            convergent, 4,
            "convergent bucket: Join + Gate + Select + Reduce"
        );
        assert_eq!(
            endomorphic + diffusive + convergent,
            ConvergencePointType::ALL.len() as u32
        );
    }

    /// ARITY-PAIR ⇔ BUCKET CONTRACT: the `(input_arity, output_arity)`
    /// projection names the same topology partition as the
    /// `is_endomorphic` / `is_diffusive` / `is_convergent` predicate
    /// triple. `(One, One) ⇒ endomorphic`; `(One, Many) ⇒ diffusive`;
    /// `(Many, One) ⇒ convergent`. The impossible `(Many, Many)`
    /// bucket is pinned empty here — a `(Many, Many)` point would
    /// have no convergence semantics (many independent inputs
    /// replicated across many independent outputs) and every future
    /// DAG-composition validator would have to special-case it. This
    /// seal is the bridge that lets a future graph validator dispatch
    /// on either projection (arity pair OR bucket predicates) without
    /// drift — and a future variant that wants `(Many, Many)` must
    /// extend the bucket carving deliberately rather than silently
    /// shipping a fourth topology class.
    #[test]
    fn convergence_point_type_arity_pair_agrees_with_bucket() {
        for t in ConvergencePointType::ALL {
            match (t.input_arity(), t.output_arity()) {
                (Arity::One, Arity::One) => assert!(
                    t.is_endomorphic(),
                    "{t:?} has (One, One) arity but is not endomorphic"
                ),
                (Arity::One, Arity::Many) => assert!(
                    t.is_diffusive(),
                    "{t:?} has (One, Many) arity but is not diffusive"
                ),
                (Arity::Many, Arity::One) => assert!(
                    t.is_convergent(),
                    "{t:?} has (Many, One) arity but is not convergent"
                ),
                (Arity::Many, Arity::Many) => panic!(
                    "{t:?} has (Many, Many) arity — pinned empty; \
                     extend the topology carving before adding a variant here"
                ),
            }
        }
    }

    /// BRIDGE ROUND-TRIP CONTRACT: every variant survives the
    /// CRD-facing (`PascalCase`) ↔ tatara-core (`snake_case`)
    /// `From` hop. Today the bridge is two hand-written 8-arm
    /// matches in this file; pinning the round-trip over `ALL`
    /// means a future variant added without extending the bridge
    /// fails here at one site instead of drifting between the CRD
    /// wire format and the
    /// `core::ConvergencePointType` selector axis that
    /// `compliance_binding::PointSelector::ByType` already
    /// dispatches against.
    #[test]
    fn convergence_point_type_bridge_roundtrip_over_all() {
        for t in ConvergencePointType::ALL {
            let core_t: core::ConvergencePointType = t.into();
            let back: ConvergencePointType = core_t.into();
            assert_eq!(back, t, "bridge round-trip failed for {t:?}");
        }
    }

    // ── closed-set algebra contracts for Arity ───────────────────

    /// `ALL` is the source of truth — pin its closure so a variant
    /// added without an `ALL` entry fails here. The arity is asserted
    /// by the `[Self; 2]` array type itself.
    #[test]
    fn arity_all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for a in Arity::ALL {
            assert!(seen.insert(a), "duplicate variant in ALL: {a:?}");
        }
        assert_eq!(seen.len(), Arity::ALL.len());
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. No serde
    /// matching here because `Arity` is a typed projection, not a
    /// CRD-facing enum — it never crosses the wire.
    #[test]
    fn arity_display_matches_as_str() {
        for a in Arity::ALL {
            assert_eq!(a.to_string(), a.as_str());
        }
    }

    /// PREDICATE CONTRACT: `is_one` is true exactly for `Arity::One`.
    /// The disjointness against `Many` is structural (only two
    /// variants) but pinning the codomain here means a future
    /// `Arity::Zero` variant must declare its own `is_one` arm
    /// deliberately rather than silently defaulting through a
    /// non-closed-set match.
    #[test]
    fn arity_is_one_predicate_truth_table() {
        assert!(Arity::One.is_one());
        assert!(!Arity::Many.is_one());
    }

    // ── closed-set algebra contracts for SubstrateType
    //    (ALL × as_str × FromStr × predicate triple × bridge) ─────────

    /// `ALL` is the source of truth — pin its closure so a variant
    /// added without an `ALL` entry fails here via the uniqueness
    /// check before drifting `FromStr` or the sweep tests below. The
    /// arity is asserted by the `[Self; 8]` array type itself.
    #[test]
    fn substrate_type_all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for t in SubstrateType::ALL {
            assert!(seen.insert(t), "duplicate variant in ALL: {t:?}");
        }
        assert_eq!(seen.len(), SubstrateType::ALL.len());
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the
    /// YAML wire format the reconciler reads from
    /// `spec.classification.substrate`.
    #[test]
    fn substrate_type_as_str_matches_serde() {
        for t in SubstrateType::ALL {
            let serialized = serde_json::to_string(&t).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                t.as_str(),
                "as_str drift for {t:?}: as_str={} serde={unquoted}",
                t.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. Any
    /// operator-facing `substrate={kind}` diagnostic that composes
    /// through Display inherits the canonical wire-format string
    /// automatically.
    #[test]
    fn substrate_type_display_matches_as_str() {
        for t in SubstrateType::ALL {
            assert_eq!(t.to_string(), t.as_str());
        }
    }

    /// Every variant in ALL round-trips through `as_str` ↔ `FromStr`.
    /// Adding a variant without extending `as_str` / `FromStr`'s
    /// sweep of `ALL` fails here.
    #[test]
    fn substrate_type_roundtrip_via_as_str() {
        for t in SubstrateType::ALL {
            assert_eq!(
                SubstrateType::from_str(t.as_str()).unwrap(),
                t,
                "round-trip failed for {t:?}"
            );
        }
    }

    /// `FromStr` rejects strings outside the canonical projection —
    /// empty / lowercased / typo / cross-axis-leaked — and the error
    /// echoes the input verbatim so the operator-facing diagnostic
    /// surfaces the bad value, not a normalized form.
    #[test]
    fn unknown_substrate_type_errors() {
        for bad in [
            "", "compute",  // lowercased
            "COMPUTE",  // uppercased
            "Computte", // typo
            "Database", "Steady",   // PoolPhase-axis leak
            "Pii",      // DataClassification-axis leak
            "Attested", // ProcessPhase-axis leak
            "Gate",     // ConvergencePointType-axis leak
            "Monotone", // CalmClassification-axis leak
            "PromQL",   // ConditionKind-axis leak
        ] {
            let err = SubstrateType::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// TRUTH-TABLE CONTRACT: the predicate triple agrees with the
    /// documented per-variant plane role. Pinning this table at one
    /// site means any future compliance-baseline selector reads the
    /// same projection that the reconciler stamps on the CRD.
    #[test]
    fn substrate_type_predicate_truth_tables() {
        // Resource plane: you allocate budgets from it.
        for t in [
            SubstrateType::Financial,
            SubstrateType::Compute,
            SubstrateType::Network,
            SubstrateType::Storage,
        ] {
            assert!(t.is_resource(), "{t:?} should be a resource substrate");
            assert!(!t.is_policy(), "{t:?} should not be a policy substrate");
            assert!(
                !t.is_telemetry(),
                "{t:?} should not be a telemetry substrate"
            );
        }

        // Policy plane: it gates access for other workloads.
        for t in [
            SubstrateType::Security,
            SubstrateType::Identity,
            SubstrateType::Regulatory,
        ] {
            assert!(!t.is_resource(), "{t:?} should not be a resource substrate");
            assert!(t.is_policy(), "{t:?} should be a policy substrate");
            assert!(
                !t.is_telemetry(),
                "{t:?} should not be a telemetry substrate"
            );
        }

        // Telemetry plane: it observes other workloads.
        assert!(!SubstrateType::Observability.is_resource());
        assert!(!SubstrateType::Observability.is_policy());
        assert!(SubstrateType::Observability.is_telemetry());
    }

    /// COVERAGE CONTRACT: every variant lands in *exactly one* of
    /// the three plane buckets — resource, policy, or telemetry.
    /// Pins the three buckets at their declared cardinalities (4,
    /// 3, 1 — sum to `ALL.len()`) so a future variant lands
    /// somewhere deliberately. No variant returns true from more
    /// than one predicate; no variant returns false from all three.
    #[test]
    fn substrate_type_buckets_cover_every_variant() {
        let mut resource = 0u32;
        let mut policy = 0u32;
        let mut telemetry = 0u32;
        for t in SubstrateType::ALL {
            let buckets = [t.is_resource(), t.is_policy(), t.is_telemetry()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "{t:?} landed in {hits} buckets: {buckets:?} (must be exactly one)"
            );
            if t.is_resource() {
                resource += 1;
            }
            if t.is_policy() {
                policy += 1;
            }
            if t.is_telemetry() {
                telemetry += 1;
            }
        }
        assert_eq!(
            resource, 4,
            "resource bucket: Financial + Compute + Network + Storage"
        );
        assert_eq!(policy, 3, "policy bucket: Security + Identity + Regulatory");
        assert_eq!(telemetry, 1, "telemetry bucket: Observability");
        assert_eq!(
            resource + policy + telemetry,
            SubstrateType::ALL.len() as u32
        );
    }

    /// BRIDGE ROUND-TRIP CONTRACT: every variant survives the
    /// CRD-facing (`PascalCase`) ↔ tatara-core (`snake_case`)
    /// `From` hop. Today the bridge is two hand-written 8-arm
    /// matches in this file; pinning the round-trip over `ALL`
    /// means a future variant added without extending the bridge
    /// fails here at one site instead of drifting between the CRD
    /// wire format and the `core::SubstrateType` selector axis
    /// that `compliance_binding::PointSelector::BySubstrate`
    /// already dispatches against.
    #[test]
    fn substrate_type_bridge_roundtrip_over_all() {
        for t in SubstrateType::ALL {
            let core_t: core::SubstrateType = t.into();
            let back: SubstrateType = core_t.into();
            assert_eq!(back, t, "bridge round-trip failed for {t:?}");
        }
    }
}
