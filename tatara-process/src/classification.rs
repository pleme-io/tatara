//! The six classification dimensions — CRD-facing with `JsonSchema`,
//! `From`/`Into` bridges to `tatara_core::domain::classification`.

use std::fmt;

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
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown)]
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

// `impl FromStr for ConvergencePointType` +
// `impl tatara_lisp::ClosedSet for ConvergencePointType` +
// `pub struct UnknownConvergencePointType(pub String)` are all generated
// by `#[derive(tatara_lisp::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown)]` on the enum
// declaration above. `label` delegates to the inherent
// `ConvergencePointType::as_str` — the inherent name (PascalCase
// `as_str`) stays the load-bearing wire-vocabulary projection that
// matches the serde `rename_all = "PascalCase"` output AND the CRD
// `enum:` enumeration the Process schema stamps on
// `spec.classification.pointType` verbatim, while generic
// `T: ClosedSet` consumers reach the STABLE workspace-wide name
// (`label`). The auto-derived carrier label "convergence point type"
// matches the prior hand-rolled `#[error("unknown convergence point
// type: {0}")]` annotation byte-for-byte. Symmetric to the other five
// classification-axis closed-sets in this file
// (`SubstrateType` / `HorizonKind` / `OptimizationDirection` /
// `CalmClassification` / `DataClassification`) AND every other
// `#[derive(DeriveClosedSet)]` implementor across the workspace
// (`crate::pool::{ReplacementPolicy,MemberState,PoolPhase,ReturnPolicy}`,
// `crate::export::{ArtifactKind,ReportFormat,ChannelKind,ExportTrigger}`).

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
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown)]
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

// `impl FromStr for SubstrateType` +
// `impl tatara_lisp::ClosedSet for SubstrateType` +
// `pub struct UnknownSubstrateType(pub String)` are all generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown)]` on the enum
// declaration above. The auto-derived carrier label "substrate type"
// matches the prior hand-rolled `#[error("unknown substrate type:
// {0}")]` annotation byte-for-byte. See the retrofit comment block on
// [`ConvergencePointType`] for the canonical narrative.

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

/// The shape of a convergence horizon's lifetime — does the point
/// run toward a fixed point and terminate, or run in perpetuity with
/// a rate signal?
///
/// Closed-set sibling on the classification axis algebra; the `ALL` /
/// `as_str` / Display / `FromStr` triad mirrors
/// [`ConvergencePointType::ALL`], [`SubstrateType::ALL`],
/// [`DataClassification::ALL`], [`CalmClassification::ALL`],
/// [`OptimizationDirection::ALL`], [`crate::pool::PoolPhase::ALL`],
/// [`crate::pool::MemberState::ALL`],
/// [`crate::pool::ReplacementPolicy::ALL`],
/// [`crate::pool::ReturnPolicy::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`]. The [`Self::terminates`]
/// predicate is the load-bearing horizon-shape primitive — schedulers
/// asking "will this Process ever reach `Reaped` via natural
/// termination?" read it as the typed image of the lattice ordering
/// (`Bounded ≤ Asymptotic` because the bounded horizon strictly
/// refines the asymptotic one by also terminating) rather than
/// re-deriving from the variant name. The
/// [`Self::requires_metric_axes`] predicate is the typed validity
/// witness for the [`Horizon`] struct's three `Option<…>` fields
/// (`metric`, `direction`, `healthy_rate_threshold`) — they're
/// `Some(_)` iff the kind requires them, so the implicit invariant
/// the optionality encodes becomes a checkable per-kind predicate
/// instead of operator folklore.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Default,
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown)]
pub enum HorizonKind {
    /// Has a fixed point — distance reaches 0 and terminates.
    #[default]
    Bounded,
    /// Runs in perpetuity — rate is the health signal, not distance.
    Asymptotic,
}

impl HorizonKind {
    /// The closed set of horizon kinds — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad AND the
    /// `terminates` predicate AND the `requires_metric_axes` shape-
    /// validity witness. Adding a third variant (e.g. a `Periodic`
    /// sentinel for "terminates on each window boundary then
    /// re-arms", which neither perpetually-running nor singularly-
    /// terminating names) lands at one `ALL` entry + one `as_str`
    /// arm + one `terminates` arm + one `requires_metric_axes` arm —
    /// exhaustively checked by the compiler (the `[Self; 2]` array
    /// literal forces the arity) AND by the per-variant truth-table
    /// tests (a new variant must declare its own termination AND
    /// metric-axes requirement, or every scheduler / horizon-shape
    /// validator will silently bucket it). Closes the load-bearing
    /// classification sub-axis that the `Horizon.kind` field threads
    /// through every `Classification.horizon` field on every
    /// Process — the last open sibling on the classification axis
    /// algebra after `OptimizationDirection` (980a318),
    /// `CalmClassification` (da3430c), `SubstrateType` (b9d7b3b),
    /// `ConvergencePointType` (7941527), `Arity`, and
    /// `DataClassification` (81bffa0).
    pub const ALL: [Self; 2] = [Self::Bounded, Self::Asymptotic];

    /// Canonical PascalCase wire-format projection — matches the
    /// serde `rename_all = "PascalCase"` output verbatim AND the CRD
    /// `enum:` enumeration the Process schema stamps on
    /// `spec.classification.horizon.kind`. Pinned by
    /// `horizon_kind_as_str_matches_serde` so a variant rename
    /// can't drift between the typed surface, the CRD enum, the
    /// YAML wire format AND any future operator-facing diagnostic
    /// composing `horizon.kind={kind}` via Display rather than a
    /// hard-coded literal. Display + FromStr triad over `ALL`
    /// mirrors every sibling closed-set enum in this crate.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bounded => "Bounded",
            Self::Asymptotic => "Asymptotic",
        }
    }

    /// LOAD-BEARING HORIZON-SHAPE PRIMITIVE: does this kind terminate
    /// naturally — i.e. does it have a fixed point that
    /// `ConvergenceDistance` can reach? Closed-set match (not
    /// `matches!`) so a future variant triggers the compiler's
    /// exhaustiveness check rather than silently defaulting to
    /// `false` (which would silently mis-route a terminating
    /// variant through the asymptotic rate-window evaluator) or
    /// `true` (which would silently invent a fixed point for a
    /// perpetual variant). `Bounded ⇒ true`, `Asymptotic ⇒ false`
    /// is the typed image of the documented lattice ordering
    /// `Bounded ≤ Asymptotic` — the bounded horizon strictly refines
    /// the asymptotic one BY ALSO TERMINATING. Future schedulers
    /// asking "will this Process reach `Reaped` via natural
    /// termination?" read this predicate, and the tatara-lattice
    /// `Lattice for Horizon` impl (which currently dispatches on
    /// `self.kind == HorizonKind::Bounded` at three sites) can be
    /// recast in a future run to read `self.kind.terminates()` so
    /// the lattice basis is the typed primitive rather than a
    /// variant-name comparison.
    pub const fn terminates(self) -> bool {
        match self {
            Self::Bounded => true,
            Self::Asymptotic => false,
        }
    }

    /// LOAD-BEARING SHAPE-VALIDITY WITNESS: does this kind require
    /// the three asymptotic-only [`Horizon`] axes (`metric`,
    /// `direction`, `healthy_rate_threshold`) to be `Some(_)`?
    /// Closed-set match (not `matches!`) so a future variant
    /// triggers the compiler's exhaustiveness check rather than
    /// silently defaulting to `false` (which would silently let an
    /// asymptotic-shaped variant ship with missing metric axes and
    /// trip the rate-window evaluator at runtime). `Bounded ⇒
    /// false`, `Asymptotic ⇒ true` is the typed image of the
    /// optionality the [`Horizon`] struct encodes via three
    /// `Option<…>` fields — the implicit invariant ("Asymptotic
    /// only" in the field docs) is now a checkable per-kind
    /// predicate. Future horizon-shape validators (CRD admission,
    /// `tatara-check` form linter, Lisp authoring-time predicate)
    /// read this rather than re-deriving from variant names.
    /// Pinned as the antisymmetric partner of [`Self::terminates`]
    /// — exactly one of `(terminates, requires_metric_axes)` is
    /// true per variant — by
    /// `horizon_kind_terminate_xor_requires_metric_axes`.
    pub const fn requires_metric_axes(self) -> bool {
        match self {
            Self::Bounded => false,
            Self::Asymptotic => true,
        }
    }
}

impl fmt::Display for HorizonKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// `impl FromStr for HorizonKind` +
// `impl tatara_lisp::ClosedSet for HorizonKind` +
// `pub struct UnknownHorizonKind(pub String)` are all generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown)]` on the enum
// declaration above. The auto-derived carrier label "horizon kind"
// matches the prior hand-rolled `#[error("unknown horizon kind:
// {0}")]` annotation byte-for-byte. See the retrofit comment block on
// [`ConvergencePointType`] for the canonical narrative.

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

/// Direction of asymptotic optimization — does the metric trend
/// downward (cost / latency / error rate) or upward
/// (throughput / coverage / revenue)?
///
/// Closed-set sibling on the classification axis algebra; the `ALL` /
/// `as_str` / Display / `FromStr` triad mirrors
/// [`ConvergencePointType::ALL`], [`SubstrateType::ALL`],
/// [`DataClassification::ALL`], [`CalmClassification::ALL`],
/// [`crate::pool::PoolPhase::ALL`], [`crate::pool::MemberState::ALL`],
/// [`crate::pool::ReplacementPolicy::ALL`],
/// [`crate::pool::ReturnPolicy::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`]. The
/// [`Self::is_improvement`] predicate is the load-bearing
/// optimization primitive — `Asymptotic` horizons read it as the
/// typed image of "did this metric sample improve over the last
/// one?" rather than re-deriving `<` vs `>` from the variant name
/// at every consumer site (rate-window evaluators, breathe-band
/// regression detectors, asymptotic-health probes).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Default,
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown)]
pub enum OptimizationDirection {
    /// Cost / latency / error rate — lower is better. The default for
    /// an under-specified `Asymptotic` horizon so an unannotated
    /// metric can't silently flip the rate-window evaluator's polarity
    /// (a future `Maximize`-default-via-rename would silently invert
    /// every existing alert that treats decreasing rate as healthy).
    #[default]
    Minimize,
    /// Throughput / coverage / revenue — higher is better.
    Maximize,
}

impl OptimizationDirection {
    /// The closed set of optimization directions — single source of
    /// truth that drives the `as_str` / Display / `FromStr` triad AND
    /// the `prefers_lower` partition AND the `is_improvement`
    /// load-bearing primitive AND both `From` bridge arms. Adding a
    /// third variant (e.g. a `Stabilize` sentinel for "drive toward
    /// a target value", which neither minimization nor maximization
    /// names) lands at one `ALL` entry + one `as_str` arm + one
    /// `prefers_lower` arm + one `is_improvement` arm + two bridge
    /// arms — exhaustively checked by the compiler (the `[Self; 2]`
    /// array literal forces the arity) AND by the per-variant
    /// truth-table tests (a new variant must declare its own
    /// improvement semantics, or every asymptotic-health probe will
    /// silently bucket it). Closes the load-bearing classification
    /// sub-axis that the `Horizon.direction` field threads through
    /// every `Asymptotic` Process.
    pub const ALL: [Self; 2] = [Self::Minimize, Self::Maximize];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim AND the CRD `enum:`
    /// enumeration the Process schema stamps on
    /// `spec.classification.horizon.direction`. Pinned by
    /// `optimization_direction_as_str_matches_serde` so a variant
    /// rename can't drift between the typed surface, the CRD enum, the
    /// YAML wire format AND any future operator-facing diagnostic
    /// composed as `direction={kind}` via Display rather than a
    /// hard-coded literal. Display + `FromStr` triad over `ALL`
    /// mirrors every sibling closed-set enum in this crate.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimize => "Minimize",
            Self::Maximize => "Maximize",
        }
    }

    /// Does this direction prefer numerically lower values?
    /// Closed-set match (not `matches!`) so a future variant triggers
    /// the compiler's exhaustiveness check at this site rather than
    /// silently defaulting to `false` (which would mis-bucket a
    /// `Stabilize`-style variant onto the maximization path). The
    /// boolean partition is the algebraic shape of an optimization
    /// direction: `Minimize ⇒ true`, `Maximize ⇒ false`. Mirrors
    /// [`CalmClassification::requires_coordination`] — a two-variant
    /// truth-table that any future dispatch on a per-direction policy
    /// (rate-window evaluator polarity, breathe-band regression
    /// detector sign, asymptotic-health threshold direction) reads
    /// once rather than re-deriving from the variant name.
    pub const fn prefers_lower(self) -> bool {
        match self {
            Self::Minimize => true,
            Self::Maximize => false,
        }
    }

    /// LOAD-BEARING OPTIMIZATION PRIMITIVE: under this direction, is
    /// `after` strictly better than `before`? Closed-set match so a
    /// future variant triggers the compiler's exhaustiveness check
    /// rather than silently defaulting to `false` (which would
    /// silently mark every sample as a regression). For `Minimize`,
    /// improvement means `after < before`; for `Maximize`, `after >
    /// before`. Strict inequality so a no-op sample (equal values) is
    /// NOT counted as improvement — pinned by
    /// `optimization_direction_no_op_is_not_improvement`, which
    /// guarantees a flatlined rate-window evaluator doesn't silently
    /// keep claiming "still improving" forever and skipping the
    /// healthy-rate-threshold gate. NaN on either operand short-
    /// circuits to `false` (no improvement claim from indeterminate
    /// data) via the standard `PartialOrd` behavior — pinned by
    /// `optimization_direction_nan_is_not_improvement`. The
    /// asymmetry contract (`is_improvement(a, b)` xor
    /// `is_improvement(b, a)` for distinct finite samples) is pinned
    /// by `optimization_direction_is_improvement_is_antisymmetric`,
    /// the algebraic shape that every asymptotic-health rate-window
    /// evaluator depends on to avoid double-counting an improvement
    /// as a regression on the reverse traversal.
    pub fn is_improvement(self, before: f64, after: f64) -> bool {
        match self {
            Self::Minimize => after < before,
            Self::Maximize => after > before,
        }
    }
}

impl fmt::Display for OptimizationDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// `impl FromStr for OptimizationDirection` +
// `impl tatara_lisp::ClosedSet for OptimizationDirection` +
// `pub struct UnknownOptimizationDirection(pub String)` are all
// generated by `#[derive(tatara_lisp::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown)]` on the enum
// declaration above. The auto-derived carrier label
// "optimization direction" matches the prior hand-rolled
// `#[error("unknown optimization direction: {0}")]` annotation
// byte-for-byte. See the retrofit comment block on
// [`ConvergencePointType`] for the canonical narrative.

/// CALM theorem classification — determines whether coordination is required.
///
/// Closed-set sibling on the classification axis algebra; the `ALL` /
/// `as_str` / Display / `FromStr` triad mirrors
/// [`ConvergencePointType::ALL`], [`SubstrateType::ALL`],
/// [`DataClassification::ALL`], [`crate::pool::PoolPhase::ALL`],
/// [`crate::pool::MemberState::ALL`], [`crate::pool::ReplacementPolicy::ALL`],
/// [`crate::pool::ReturnPolicy::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`]. The
/// [`Self::requires_coordination`] predicate is the CALM theorem
/// keystone — Hellerstein's "Consistency As Logical Monotonicity"
/// states that a program can be distributed without coordination iff
/// it computes a monotone function, so `Monotone ⇒ no coordination`
/// and `NonMonotone ⇒ requires coordination` is a typed image of the
/// theorem itself rather than a runtime convention. Future reconciler
/// dispatch on `calm.requires_coordination()` (Raft for non-monotone
/// writes; gossip for monotone ones) reads this projection rather
/// than re-deriving from variant names.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Default,
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown)]
pub enum CalmClassification {
    /// Can be distributed without coordination (CALM ⇒ the program
    /// computes a monotone function).
    #[default]
    Monotone,
    /// Requires coordination (CALM ⇒ the program is not monotone).
    NonMonotone,
}

impl CalmClassification {
    /// The closed set of CALM classifications — single source of truth
    /// that drives the `as_str` / Display / `FromStr` triad AND the
    /// `requires_coordination` predicate. Adding a third variant
    /// (e.g. a `ConditionallyMonotone` sentinel for ops that are
    /// monotone under a witness, like CRDT joins under a fixed
    /// schema) lands at one `ALL` entry + one `as_str` arm + one
    /// predicate arm + one bridge-pair arm — exhaustively checked by
    /// the compiler (the `[Self; 2]` array literal forces the arity)
    /// AND by the per-variant predicate truth-table test (a new
    /// variant must declare its own coordination requirement or any
    /// future reconciler-side dispatch will silently bucket it).
    /// Closes the load-bearing classification-axis enum that the
    /// `Classification.calm` field exposes to every Process and that
    /// [`tatara_lattice`]'s boolean-lattice `Lattice for
    /// CalmClassification` impl reads via [`Self::requires_coordination`]
    /// as the lattice's `top()` predicate.
    pub const ALL: [Self; 2] = [Self::Monotone, Self::NonMonotone];

    /// Canonical PascalCase wire-format projection — matches the
    /// serde `rename_all = "PascalCase"` output verbatim AND the CRD
    /// `enum:` enumeration that the Process schema stamps on
    /// `spec.classification.calm`. Pinned by
    /// `calm_classification_as_str_matches_serde` so a variant rename
    /// can't drift between the typed surface, the CRD enum, the YAML
    /// wire format AND any future operator-facing diagnostic that
    /// composes `calm={kind}` via Display rather than a hard-coded
    /// literal that would silently rot. Display + FromStr triad over
    /// `ALL` mirrors every sibling closed-set enum in this crate.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Monotone => "Monotone",
            Self::NonMonotone => "NonMonotone",
        }
    }

    /// CALM-THEOREM KEYSTONE: does this classification require
    /// distributed coordination? Closed-set match (not `matches!`) so
    /// a future variant triggers the compiler's exhaustiveness check
    /// at this site rather than silently defaulting to `false` and
    /// shipping a non-monotone operation onto the no-coordination
    /// path. The theorem (Hellerstein 2010) states that a program can
    /// be distributed without coordination iff it computes a monotone
    /// function — `Monotone ⇒ false` and `NonMonotone ⇒ true` is the
    /// typed image of that biconditional. Consumers (future reconciler
    /// dispatch between Raft writes and gossip propagation; current
    /// `tatara_lattice` boolean-lattice ordering where `Monotone ≤
    /// NonMonotone`) read this predicate rather than re-deriving from
    /// variant names.
    pub const fn requires_coordination(self) -> bool {
        match self {
            Self::Monotone => false,
            Self::NonMonotone => true,
        }
    }
}

impl fmt::Display for CalmClassification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// `impl FromStr for CalmClassification` +
// `impl tatara_lisp::ClosedSet for CalmClassification` +
// `pub struct UnknownCalmClassification(pub String)` are all generated
// by `#[derive(tatara_lisp::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown)]` on the enum
// declaration above. The auto-derived carrier label
// "calm classification" matches the prior hand-rolled
// `#[error("unknown calm classification: {0}")]` annotation
// byte-for-byte. See the retrofit comment block on
// [`ConvergencePointType`] for the canonical narrative.

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
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown)]
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

// `impl FromStr for DataClassification` +
// `impl tatara_lisp::ClosedSet for DataClassification` +
// `pub struct UnknownDataClassification(pub String)` are all generated
// by `#[derive(tatara_lisp::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown)]` on the enum
// declaration above. The auto-derived carrier label
// "data classification" matches the prior hand-rolled
// `#[error("unknown data classification: {0}")]` annotation
// byte-for-byte. See the retrofit comment block on
// [`ConvergencePointType`] for the canonical narrative.

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

impl From<core::OptimizationDirection> for OptimizationDirection {
    fn from(v: core::OptimizationDirection) -> Self {
        use core::OptimizationDirection as C;
        match v {
            C::Minimize => Self::Minimize,
            C::Maximize => Self::Maximize,
        }
    }
}

impl From<Horizon> for core::ConvergenceHorizon {
    fn from(v: Horizon) -> Self {
        match v.kind {
            HorizonKind::Bounded => Self::Bounded,
            HorizonKind::Asymptotic => Self::Asymptotic {
                metric: v.metric.unwrap_or_default(),
                direction: v.direction.unwrap_or_default().into(),
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

impl From<core::CalmClassification> for CalmClassification {
    fn from(v: core::CalmClassification) -> Self {
        use core::CalmClassification as C;
        match v {
            C::Monotone => Self::Monotone,
            C::NonMonotone => Self::NonMonotone,
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
    // The closed-set tests below call `T::from_str(bad)` via the
    // derive-generated `FromStr` impls — bring the trait into scope at
    // the test module so the lib body doesn't carry an otherwise-unused
    // `use std::str::FromStr;` at the file head.
    use std::str::FromStr;

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

    /// Structural well-formedness of [`DataClassification`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `data_classification_all_is_unique_and_complete` +
    /// `data_classification_roundtrip_via_as_str` + the empty-input arm
    /// of `unknown_data_classification_errors`. `FromStr` delegates to
    /// `<Self as tatara_lisp::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the reconciler hits when parsing a
    /// CRD `enum:`-validated `dataClassification` value back to the
    /// typed classification.
    #[test]
    fn data_classification_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<DataClassification>();
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

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / cross-axis-leaked — and the
    /// error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    /// The empty-input arm is pinned by
    /// [`data_classification_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownDataClassification`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_data_classification_errors() {
        for bad in [
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

    /// AUTO-DERIVED LABEL CONTRACT: the `#[closed_set(generate_unknown)]`
    /// attribute emits the carrier with the substrate-wide
    /// `#[error("unknown data classification: {0}")]` annotation
    /// auto-derived from the PascalCase enum name (via
    /// `pascal_to_spaced_lowercase`). Pins the projection
    /// byte-for-byte against the prior hand-rolled annotation so a
    /// regression in the derive's label helper would surface here
    /// rather than silently drifting the operator-facing diagnostic.
    /// Mirrors the matching tests on
    /// `crate::export::{ChannelKind,ReportFormat,ExportTrigger}` (commit
    /// b487465).
    #[test]
    fn unknown_data_classification_message_matches_substrate_convention() {
        let err = UnknownDataClassification("foo".to_string());
        assert_eq!(err.to_string(), "unknown data classification: foo");
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

    /// Structural well-formedness of [`ConvergencePointType`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through `label ↔
    /// parse_label`, labels are pairwise distinct, `""` is outside
    /// the closed set) at ONE call site. Replaces the hand-derived
    /// `convergence_point_type_all_is_unique_and_complete` +
    /// `convergence_point_type_roundtrip_via_as_str` + the empty-
    /// input arm of `unknown_convergence_point_type_errors`.
    /// `FromStr` delegates to `<Self as tatara_lisp::ClosedSet>::parse_label`,
    /// so this helper exercises the same code path the reconciler
    /// hits when parsing a CRD `enum:`-validated value back to the
    /// typed point-type. The forced `[Self; 8]` array literal on
    /// `ConvergencePointType::ALL` still pins the cardinality at the
    /// declaration site.
    #[test]
    fn convergence_point_type_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<ConvergencePointType>();
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

    /// `FromStr` rejects strings outside the canonical projection —
    /// lowercased / typo / cross-axis-leaked — and the error echoes
    /// the input verbatim so the operator-facing diagnostic surfaces
    /// the bad value, not a normalized form. The empty-input arm is
    /// pinned by [`convergence_point_type_is_well_formed_closed_set`]
    /// via the `tatara_lisp::ClosedSet` testkit; the cases here pin
    /// the verbatim-echo contract on the
    /// [`UnknownConvergencePointType`] newtype, which the trait's
    /// `make_unknown` can't see.
    #[test]
    fn unknown_convergence_point_type_errors() {
        for bad in [
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

    /// AUTO-DERIVED LABEL CONTRACT: pins the auto-derived carrier
    /// label "convergence point type" against the prior hand-rolled
    /// `#[error("unknown convergence point type: {0}")]` annotation
    /// byte-for-byte. See `unknown_data_classification_message_matches_substrate_convention`
    /// for the contract rationale.
    #[test]
    fn unknown_convergence_point_type_message_matches_substrate_convention() {
        let err = UnknownConvergencePointType("foo".to_string());
        assert_eq!(err.to_string(), "unknown convergence point type: foo");
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

    /// Structural well-formedness of [`SubstrateType`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — see
    /// [`convergence_point_type_is_well_formed_closed_set`] for the
    /// canonical lift narrative. Replaces
    /// `substrate_type_all_is_unique_and_complete` +
    /// `substrate_type_roundtrip_via_as_str` + the empty-input arm
    /// of `unknown_substrate_type_errors`.
    #[test]
    fn substrate_type_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<SubstrateType>();
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

    /// `FromStr` rejects strings outside the canonical projection —
    /// lowercased / typo / cross-axis-leaked — and the error echoes
    /// the input verbatim so the operator-facing diagnostic surfaces
    /// the bad value, not a normalized form. The empty-input arm is
    /// pinned by [`substrate_type_is_well_formed_closed_set`] via
    /// the `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownSubstrateType`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_substrate_type_errors() {
        for bad in [
            "compute",  // lowercased
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

    /// AUTO-DERIVED LABEL CONTRACT: pins the auto-derived carrier
    /// label "substrate type" against the prior hand-rolled
    /// `#[error("unknown substrate type: {0}")]` annotation
    /// byte-for-byte. See `unknown_data_classification_message_matches_substrate_convention`
    /// for the contract rationale.
    #[test]
    fn unknown_substrate_type_message_matches_substrate_convention() {
        let err = UnknownSubstrateType("foo".to_string());
        assert_eq!(err.to_string(), "unknown substrate type: foo");
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

    // ── closed-set algebra contracts for CalmClassification
    //    (ALL × as_str × FromStr × requires_coordination × bridge) ─────

    /// Structural well-formedness of [`CalmClassification`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — see
    /// [`convergence_point_type_is_well_formed_closed_set`] for the
    /// canonical lift narrative. Replaces
    /// `calm_classification_all_is_unique_and_complete` +
    /// `calm_classification_roundtrip_via_as_str` + the empty-input
    /// arm of `unknown_calm_classification_errors`.
    #[test]
    fn calm_classification_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<CalmClassification>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the
    /// YAML wire format the reconciler reads from
    /// `spec.classification.calm`.
    #[test]
    fn calm_classification_as_str_matches_serde() {
        for c in CalmClassification::ALL {
            let serialized = serde_json::to_string(&c).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                c.as_str(),
                "as_str drift for {c:?}: as_str={} serde={unquoted}",
                c.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. Any
    /// operator-facing `calm={kind}` diagnostic that composes
    /// through Display inherits the canonical wire-format string
    /// automatically.
    #[test]
    fn calm_classification_display_matches_as_str() {
        for c in CalmClassification::ALL {
            assert_eq!(c.to_string(), c.as_str());
        }
    }

    /// `FromStr` rejects strings outside the canonical projection —
    /// lowercased / typo / cross-axis-leaked — and the error echoes
    /// the input verbatim so the operator-facing diagnostic surfaces
    /// the bad value, not a normalized form. The empty-input arm is
    /// pinned by [`calm_classification_is_well_formed_closed_set`]
    /// via the `tatara_lisp::ClosedSet` testkit; the cases here pin
    /// the verbatim-echo contract on the
    /// [`UnknownCalmClassification`] newtype, which the trait's
    /// `make_unknown` can't see.
    #[test]
    fn unknown_calm_classification_errors() {
        for bad in [
            "monotone",     // lowercased
            "MONOTONE",     // uppercased
            "Mono",         // typo
            "non_monotone", // core's snake_case form (must not cross axes)
            "non-monotone", // dashed
            "Monotonic",    // close-typo
            "Steady",       // PoolPhase-axis leak
            "Pii",          // DataClassification-axis leak
            "Attested",     // ProcessPhase-axis leak
            "Compute",      // SubstrateType-axis leak
            "Gate",         // ConvergencePointType-axis leak
            "PromQL",       // ConditionKind-axis leak
        ] {
            let err = CalmClassification::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// AUTO-DERIVED LABEL CONTRACT: pins the auto-derived carrier
    /// label "calm classification" against the prior hand-rolled
    /// `#[error("unknown calm classification: {0}")]` annotation
    /// byte-for-byte. See `unknown_data_classification_message_matches_substrate_convention`
    /// for the contract rationale.
    #[test]
    fn unknown_calm_classification_message_matches_substrate_convention() {
        let err = UnknownCalmClassification("foo".to_string());
        assert_eq!(err.to_string(), "unknown calm classification: foo");
    }

    /// CALM-THEOREM TRUTH-TABLE CONTRACT: `requires_coordination`
    /// implements the biconditional half of Hellerstein's CALM
    /// theorem — `Monotone ⇒ false` and `NonMonotone ⇒ true`.
    /// Pinning this table at one site means any future reconciler
    /// dispatch that picks between Raft writes and gossip
    /// propagation reads the same projection the lattice ordering
    /// (`Monotone ≤ NonMonotone`) does. A future variant that
    /// flipped this mapping would have to renumber every consumer
    /// deliberately rather than silently shipping a non-monotone
    /// operation onto the no-coordination path.
    #[test]
    fn calm_classification_requires_coordination_truth_table() {
        assert!(!CalmClassification::Monotone.requires_coordination());
        assert!(CalmClassification::NonMonotone.requires_coordination());
    }

    /// COVERAGE CONTRACT: every variant lands in exactly one of two
    /// coordination buckets — no-coordination (`Monotone`) or
    /// requires-coordination (`NonMonotone`). Pins the two buckets
    /// at their declared cardinalities (1, 1 — sum to `ALL.len()`)
    /// so a future variant lands somewhere deliberately. The
    /// biconditional structure of the CALM theorem makes this
    /// partition exhaustive by construction.
    #[test]
    fn calm_classification_buckets_cover_every_variant() {
        let mut no_coord = 0u32;
        let mut coord = 0u32;
        for c in CalmClassification::ALL {
            if c.requires_coordination() {
                coord += 1;
            } else {
                no_coord += 1;
            }
        }
        assert_eq!(no_coord, 1, "no-coordination bucket: Monotone");
        assert_eq!(coord, 1, "requires-coordination bucket: NonMonotone");
        assert_eq!(no_coord + coord, CalmClassification::ALL.len() as u32);
    }

    /// DEFAULT-AGREEMENT CONTRACT: `CalmClassification::default()`
    /// returns `Monotone` (the variant tagged `#[default]`) AND that
    /// variant lands in the no-coordination bucket. A future
    /// `#[default]` rename without flipping the predicate fails
    /// here — the default for an under-specified Process must
    /// remain the no-coordination side so that an unannotated
    /// Process can't silently demand Raft writes the reconciler
    /// isn't configured to provide.
    #[test]
    fn calm_classification_default_is_monotone_no_coordination() {
        let c = CalmClassification::default();
        assert_eq!(c, CalmClassification::Monotone);
        assert!(!c.requires_coordination());
    }

    /// BRIDGE ROUND-TRIP CONTRACT: every variant survives the
    /// CRD-facing (`PascalCase`) ↔ tatara-core (`snake_case`)
    /// `From` hop. Today the bridge is two hand-written 2-arm
    /// matches in this file; pinning the round-trip over `ALL`
    /// means a future variant added without extending the bridge
    /// fails here at one site instead of drifting between the CRD
    /// wire format and the `core::CalmClassification` selector
    /// axis. Closes the asymmetry that pre-lift had a
    /// `From<CalmClassification> for core::CalmClassification`
    /// forward bridge but no reverse — symmetric to every other
    /// classification-axis bridge in this file.
    #[test]
    fn calm_classification_bridge_roundtrip_over_all() {
        for c in CalmClassification::ALL {
            let core_c: core::CalmClassification = c.into();
            let back: CalmClassification = core_c.into();
            assert_eq!(back, c, "bridge round-trip failed for {c:?}");
        }
    }

    // ── closed-set algebra contracts for OptimizationDirection
    //    (ALL × as_str × FromStr × prefers_lower × is_improvement) ───

    /// Structural well-formedness of [`OptimizationDirection`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — see
    /// [`convergence_point_type_is_well_formed_closed_set`] for the
    /// canonical lift narrative. Replaces
    /// `optimization_direction_all_is_unique_and_complete` +
    /// `optimization_direction_roundtrip_via_as_str` + the empty-
    /// input arm of `unknown_optimization_direction_errors`.
    #[test]
    fn optimization_direction_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<OptimizationDirection>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the
    /// YAML wire format the reconciler reads from
    /// `spec.classification.horizon.direction`.
    #[test]
    fn optimization_direction_as_str_matches_serde() {
        for d in OptimizationDirection::ALL {
            let serialized = serde_json::to_string(&d).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                d.as_str(),
                "as_str drift for {d:?}: as_str={} serde={unquoted}",
                d.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. Any
    /// operator-facing `direction={kind}` diagnostic that composes
    /// through Display inherits the canonical wire-format string
    /// automatically.
    #[test]
    fn optimization_direction_display_matches_as_str() {
        for d in OptimizationDirection::ALL {
            assert_eq!(d.to_string(), d.as_str());
        }
    }

    /// `FromStr` rejects strings outside the canonical projection —
    /// lowercased / typo / cross-axis-leaked — and the error echoes
    /// the input verbatim so the operator-facing diagnostic surfaces
    /// the bad value, not a normalized form. The empty-input arm is
    /// pinned by [`optimization_direction_is_well_formed_closed_set`]
    /// via the `tatara_lisp::ClosedSet` testkit; the cases here pin
    /// the verbatim-echo contract on the
    /// [`UnknownOptimizationDirection`] newtype, which the trait's
    /// `make_unknown` can't see.
    #[test]
    fn unknown_optimization_direction_errors() {
        for bad in [
            "minimize", // lowercased
            "MINIMIZE", // uppercased
            "Minimze",  // typo
            "Lower",    // synonym, not canonical
            "Higher",   // synonym, not canonical
            "Asc",      // wire-leak from sort-order axis
            "Desc",     // wire-leak from sort-order axis
            "Bounded",  // HorizonKind-axis leak
            "Monotone", // CalmClassification-axis leak
            "Steady",   // PoolPhase-axis leak
            "Pii",      // DataClassification-axis leak
            "Attested", // ProcessPhase-axis leak
            "Compute",  // SubstrateType-axis leak
            "Gate",     // ConvergencePointType-axis leak
            "PromQL",   // ConditionKind-axis leak
        ] {
            let err = OptimizationDirection::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// AUTO-DERIVED LABEL CONTRACT: pins the auto-derived carrier
    /// label "optimization direction" against the prior hand-rolled
    /// `#[error("unknown optimization direction: {0}")]` annotation
    /// byte-for-byte. See `unknown_data_classification_message_matches_substrate_convention`
    /// for the contract rationale.
    #[test]
    fn unknown_optimization_direction_message_matches_substrate_convention() {
        let err = UnknownOptimizationDirection("foo".to_string());
        assert_eq!(err.to_string(), "unknown optimization direction: foo");
    }

    /// TRUTH-TABLE CONTRACT: `prefers_lower` is the boolean
    /// partition `Minimize ⇒ true`, `Maximize ⇒ false`. Pinning this
    /// table at one site means any future dispatch on per-direction
    /// polarity (rate-window evaluator, breathe-band regression
    /// detector) reads the same projection rather than re-deriving
    /// from the variant name. Mirrors
    /// [`CalmClassification::requires_coordination`]'s truth-table
    /// shape.
    #[test]
    fn optimization_direction_prefers_lower_truth_table() {
        assert!(OptimizationDirection::Minimize.prefers_lower());
        assert!(!OptimizationDirection::Maximize.prefers_lower());
    }

    /// COVERAGE CONTRACT: every variant lands in exactly one of two
    /// polarity buckets — prefers-lower (`Minimize`) or
    /// prefers-higher (`Maximize`). Pins the two buckets at their
    /// declared cardinalities (1, 1 — sum to `ALL.len()`) so a
    /// future variant lands somewhere deliberately.
    #[test]
    fn optimization_direction_buckets_cover_every_variant() {
        let mut lower = 0u32;
        let mut higher = 0u32;
        for d in OptimizationDirection::ALL {
            if d.prefers_lower() {
                lower += 1;
            } else {
                higher += 1;
            }
        }
        assert_eq!(lower, 1, "prefers-lower bucket: Minimize");
        assert_eq!(higher, 1, "prefers-higher bucket: Maximize");
        assert_eq!(lower + higher, OptimizationDirection::ALL.len() as u32);
    }

    /// LOAD-BEARING TRUTH-TABLE: `is_improvement` answers "is `after`
    /// strictly better than `before` under this direction?" for the
    /// canonical samples. Pins the strict-improvement semantic at
    /// one site so a future rate-window evaluator or breathe-band
    /// regression detector reads the same projection that the
    /// asymptotic-health probe writes.
    #[test]
    fn optimization_direction_is_improvement_truth_table() {
        // Minimize: lower-is-better
        assert!(OptimizationDirection::Minimize.is_improvement(10.0, 5.0));
        assert!(!OptimizationDirection::Minimize.is_improvement(5.0, 10.0));

        // Maximize: higher-is-better
        assert!(OptimizationDirection::Maximize.is_improvement(5.0, 10.0));
        assert!(!OptimizationDirection::Maximize.is_improvement(10.0, 5.0));
    }

    /// NO-OP CONTRACT: a sample equal to the previous one is NOT an
    /// improvement under either direction. Pinning this guarantees
    /// a flatlined rate-window evaluator doesn't silently keep
    /// claiming "still improving" forever and skipping the
    /// healthy-rate-threshold gate.
    #[test]
    fn optimization_direction_no_op_is_not_improvement() {
        for d in OptimizationDirection::ALL {
            assert!(
                !d.is_improvement(7.0, 7.0),
                "{d:?}: equal samples must not count as improvement",
            );
            assert!(
                !d.is_improvement(0.0, 0.0),
                "{d:?}: zero/zero must not count as improvement",
            );
        }
    }

    /// NaN CONTRACT: NaN on either operand short-circuits to `false`
    /// (no improvement claim from indeterminate data) via the
    /// standard `PartialOrd` behavior. Without this, a rate-window
    /// evaluator that sampled a NaN partway through (a transient
    /// metric-scrape failure) would either panic on an `Ord`
    /// comparison or — worse — silently claim improvement on the
    /// next valid sample by treating NaN as the worst case.
    #[test]
    fn optimization_direction_nan_is_not_improvement() {
        let nan = f64::NAN;
        for d in OptimizationDirection::ALL {
            assert!(
                !d.is_improvement(nan, 1.0),
                "{d:?}: NaN before must not count as improvement",
            );
            assert!(
                !d.is_improvement(1.0, nan),
                "{d:?}: NaN after must not count as improvement",
            );
            assert!(
                !d.is_improvement(nan, nan),
                "{d:?}: NaN/NaN must not count as improvement",
            );
        }
    }

    /// ANTISYMMETRY CONTRACT: for distinct finite samples,
    /// `is_improvement(a, b)` xor `is_improvement(b, a)` —
    /// exactly one direction of the pair counts as improvement.
    /// This is the algebraic shape every asymptotic-health
    /// rate-window evaluator depends on to avoid double-counting
    /// an improvement as a regression on the reverse traversal.
    /// A future variant that returned `true` for both directions
    /// (or `false` for both, the equal-sample case) would FAIL
    /// here, forcing the author to extend the consumer dispatch
    /// deliberately.
    #[test]
    fn optimization_direction_is_improvement_is_antisymmetric() {
        let pairs = [(1.0_f64, 2.0_f64), (0.0, 100.0), (-3.5, 3.5), (1e9, 1e-9)];
        for d in OptimizationDirection::ALL {
            for (a, b) in pairs {
                assert!(a != b, "test fixture requires distinct samples");
                assert!(
                    d.is_improvement(a, b) ^ d.is_improvement(b, a),
                    "{d:?}: antisymmetry violated on ({a}, {b})",
                );
            }
        }
    }

    /// DEFAULT-AGREEMENT CONTRACT:
    /// `OptimizationDirection::default()` returns `Minimize` (the
    /// variant tagged `#[default]`), AND that variant lands in the
    /// prefers-lower bucket. A future `#[default]` rename without
    /// flipping the predicate fails here — `Minimize` is the
    /// canonical default for distributed-systems asymptotic
    /// optimization (cost / latency / error rate), so an
    /// unannotated metric must not silently flip the rate-window
    /// evaluator's polarity. This is also the same value the
    /// `Horizon → ConvergenceHorizon` bridge falls back to when
    /// `direction` is unset, so pinning the default here pins the
    /// bridge's behavior at one site.
    #[test]
    fn optimization_direction_default_is_minimize_prefers_lower() {
        let d = OptimizationDirection::default();
        assert_eq!(d, OptimizationDirection::Minimize);
        assert!(d.prefers_lower());
    }

    /// BRIDGE ROUND-TRIP CONTRACT: every variant survives the
    /// CRD-facing (`PascalCase`) ↔ tatara-core (`snake_case`)
    /// `From` hop. Pre-lift the bridge was a one-way
    /// `From<OptimizationDirection> for core::OptimizationDirection`
    /// with no reverse — asymmetric to every other classification-
    /// axis bridge in this file. Pinning the round-trip over `ALL`
    /// means a future variant added without extending the bridge
    /// fails here at one site instead of drifting between the CRD
    /// wire format and `core::OptimizationDirection`.
    #[test]
    fn optimization_direction_bridge_roundtrip_over_all() {
        for d in OptimizationDirection::ALL {
            let core_d: core::OptimizationDirection = d.into();
            let back: OptimizationDirection = core_d.into();
            assert_eq!(back, d, "bridge round-trip failed for {d:?}");
        }
    }

    // ── closed-set algebra contracts for HorizonKind
    //    (ALL × as_str × FromStr × terminates × requires_metric_axes) ──

    /// Structural well-formedness of [`HorizonKind`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — see
    /// [`convergence_point_type_is_well_formed_closed_set`] for the
    /// canonical lift narrative. Replaces
    /// `horizon_kind_all_is_unique_and_complete` +
    /// `horizon_kind_roundtrip_via_as_str` + the empty-input arm of
    /// `unknown_horizon_kind_errors`.
    #[test]
    fn horizon_kind_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<HorizonKind>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the
    /// YAML wire format the reconciler stamps on
    /// `spec.classification.horizon.kind`.
    #[test]
    fn horizon_kind_as_str_matches_serde() {
        for k in HorizonKind::ALL {
            let serialized = serde_json::to_string(&k).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                k.as_str(),
                "as_str drift for {k:?}: as_str={} serde={unquoted}",
                k.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. Any
    /// operator-facing `horizon.kind={kind}` diagnostic that
    /// composes through Display inherits the canonical wire-format
    /// string automatically.
    #[test]
    fn horizon_kind_display_matches_as_str() {
        for k in HorizonKind::ALL {
            assert_eq!(k.to_string(), k.as_str());
        }
    }

    /// `FromStr` rejects strings outside the canonical projection —
    /// lowercased / typo / cross-axis-leaked — and the error echoes
    /// the input verbatim so the operator-facing diagnostic surfaces
    /// the bad value, not a normalized form. The empty-input arm is
    /// pinned by [`horizon_kind_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownHorizonKind`] newtype,
    /// which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_horizon_kind_errors() {
        for bad in [
            "bounded",   // lowercased
            "BOUNDED",   // uppercased
            "Boundd",    // typo
            "Finite",    // synonym, not canonical
            "Perpetual", // synonym, not canonical
            "Infinite",  // synonym, not canonical
            "Minimize",  // OptimizationDirection-axis leak
            "Monotone",  // CalmClassification-axis leak
            "Pii",       // DataClassification-axis leak
            "Steady",    // PoolPhase-axis leak
            "Attested",  // ProcessPhase-axis leak
            "Compute",   // SubstrateType-axis leak
            "Gate",      // ConvergencePointType-axis leak
            "PromQL",    // ConditionKind-axis leak
        ] {
            let err = HorizonKind::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// AUTO-DERIVED LABEL CONTRACT: pins the auto-derived carrier
    /// label "horizon kind" against the prior hand-rolled
    /// `#[error("unknown horizon kind: {0}")]` annotation byte-for-byte.
    /// See `unknown_data_classification_message_matches_substrate_convention`
    /// for the contract rationale.
    #[test]
    fn unknown_horizon_kind_message_matches_substrate_convention() {
        let err = UnknownHorizonKind("foo".to_string());
        assert_eq!(err.to_string(), "unknown horizon kind: foo");
    }

    /// LOAD-BEARING TRUTH-TABLE: `terminates` is the boolean
    /// partition `Bounded ⇒ true`, `Asymptotic ⇒ false`. Pinning
    /// this table at one site means any future scheduler asking
    /// "will this Process reach `Reaped` via natural termination?"
    /// reads the same projection that the lattice ordering encodes
    /// (Bounded ≤ Asymptotic BECAUSE the bounded horizon strictly
    /// refines the asymptotic one by also terminating).
    #[test]
    fn horizon_kind_terminates_truth_table() {
        assert!(HorizonKind::Bounded.terminates());
        assert!(!HorizonKind::Asymptotic.terminates());
    }

    /// LOAD-BEARING TRUTH-TABLE: `requires_metric_axes` is the
    /// boolean partition `Bounded ⇒ false`, `Asymptotic ⇒ true` —
    /// the typed image of the optionality the [`Horizon`] struct
    /// encodes via its three `Option<…>` fields (`metric`,
    /// `direction`, `healthy_rate_threshold`). The implicit
    /// "Asymptotic only" invariant in the field docs is now a
    /// checkable per-kind predicate. Pinning this table at one site
    /// means any future horizon-shape validator (CRD admission,
    /// `tatara-check` form linter, Lisp authoring-time predicate)
    /// reads the same projection.
    #[test]
    fn horizon_kind_requires_metric_axes_truth_table() {
        assert!(!HorizonKind::Bounded.requires_metric_axes());
        assert!(HorizonKind::Asymptotic.requires_metric_axes());
    }

    /// COVERAGE CONTRACT: every variant lands in exactly one of two
    /// termination buckets — terminating (`Bounded`) or perpetual
    /// (`Asymptotic`). Pins the two buckets at their declared
    /// cardinalities (1, 1 — sum to `ALL.len()`) so a future variant
    /// lands somewhere deliberately.
    #[test]
    fn horizon_kind_buckets_cover_every_variant() {
        let mut terminating = 0u32;
        let mut perpetual = 0u32;
        for k in HorizonKind::ALL {
            if k.terminates() {
                terminating += 1;
            } else {
                perpetual += 1;
            }
        }
        assert_eq!(terminating, 1, "terminating bucket: Bounded");
        assert_eq!(perpetual, 1, "perpetual bucket: Asymptotic");
        assert_eq!(terminating + perpetual, HorizonKind::ALL.len() as u32);
    }

    /// ANTISYMMETRY CONTRACT: for every variant, exactly one of
    /// `(terminates, requires_metric_axes)` is true — the two
    /// predicates carve the variants into complementary buckets
    /// (terminating ↔ no metric axes; perpetual ↔ requires metric
    /// axes). A future variant that returned `true` for both (a
    /// terminating horizon that nonetheless tracks an asymptotic
    /// metric) or `false` for both (an inert horizon with no
    /// termination AND no metric signal — there'd be nothing to
    /// observe) would fail here, forcing the author to extend
    /// either the predicates or the [`Horizon`] struct's
    /// optionality contract deliberately.
    #[test]
    fn horizon_kind_terminate_xor_requires_metric_axes() {
        for k in HorizonKind::ALL {
            assert!(
                k.terminates() ^ k.requires_metric_axes(),
                "{k:?}: terminates() XOR requires_metric_axes() must hold",
            );
        }
    }

    /// DEFAULT-AGREEMENT CONTRACT: `HorizonKind::default()` returns
    /// `Bounded` (the variant tagged `#[default]`), AND that
    /// variant lands in the terminating bucket. A future
    /// `#[default]` rename without flipping the predicate fails
    /// here — `Bounded` is the canonical default for a convergence
    /// horizon (a point with no asymptotic axes declared should
    /// terminate naturally, not silently flip into a perpetual
    /// rate-window evaluator with zero threshold). This is also
    /// the same value `Horizon::default()` carries, so pinning the
    /// default here pins the struct-default behavior at one site.
    #[test]
    fn horizon_kind_default_is_bounded_terminates() {
        let k = HorizonKind::default();
        assert_eq!(k, HorizonKind::Bounded);
        assert!(k.terminates());
        assert!(!k.requires_metric_axes());
    }

    /// HORIZON ↔ KIND AGREEMENT: every variant in `HorizonKind::ALL`
    /// composes with the existing [`Horizon::bounded`] /
    /// [`Horizon::asymptotic`] constructors to produce a `Horizon`
    /// whose `kind` matches AND whose `Option<…>` fields agree
    /// with `requires_metric_axes`. Pins the implicit contract
    /// between the kind discriminator and the optionality at one
    /// site — a future kind added without extending either the
    /// constructors or `requires_metric_axes` fails here before
    /// drifting between the typed surface and the documented
    /// "Asymptotic only" field invariant.
    #[test]
    fn horizon_kind_agrees_with_struct_optionality() {
        let bounded = Horizon::bounded();
        assert_eq!(bounded.kind, HorizonKind::Bounded);
        assert!(!bounded.kind.requires_metric_axes());
        assert!(bounded.metric.is_none());
        assert!(bounded.direction.is_none());
        assert!(bounded.healthy_rate_threshold.is_none());

        let asymp = Horizon::asymptotic("p99_latency", OptimizationDirection::Minimize, 0.1);
        assert_eq!(asymp.kind, HorizonKind::Asymptotic);
        assert!(asymp.kind.requires_metric_axes());
        assert!(asymp.metric.is_some());
        assert!(asymp.direction.is_some());
        assert!(asymp.healthy_rate_threshold.is_some());
    }
}
