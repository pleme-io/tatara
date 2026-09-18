//! The six classification dimensions — CRD-facing with `JsonSchema`,
//! `From`/`Into` bridges to `tatara_core::domain::classification`.

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

impl Classification {
    /// The workspace-baseline classification — a [`ConvergencePointType::Gate`]
    /// point on the [`SubstrateType::Compute`] substrate with every other axis
    /// at its [`Default`]. The `(Gate, Compute)` pair names an unremarkable
    /// barrier point in the Compute plane: no domain-specific structural
    /// claim (no fan-out / fan-in / broadcast / observation semantics beyond
    /// the barrier gate) and no domain-specific substrate claim (no
    /// `Financial` / `Network` / `Storage` / `Security` / `Identity` /
    /// `Observability` / `Regulatory` plane bringing in its own compliance
    /// baselines). The three defaulted axes ride at the intentional
    /// workspace baseline the sibling closed-set primitives already own:
    /// [`Horizon`] at [`HorizonKind::Bounded`] (terminates naturally, no
    /// asymptotic metric axes required), [`CalmClassification::Monotone`]
    /// (no coordination required per CALM), and
    /// [`DataClassification::Internal`] (access-controlled but not
    /// externally regulated).
    ///
    /// Pre-lift the six-line `Classification { point_type: Gate, substrate:
    /// Compute, horizon: Default::default(), calm: Default::default(),
    /// data_classification: Default::default() }` struct-literal recurred
    /// at TEN sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold
    /// — one production consumer plus nine test-fixture callsites spread
    /// across four crates, each restating the SAME `(Gate, Compute)`
    /// baseline verbatim:
    /// * `crate::ephemeral::default_ephemeral_class` — the substitute
    ///   [`EphemeralSpec::into::<crate::crd::ProcessSpec>`] fills into
    ///   [`crate::crd::ProcessSpec::classification`] when the operator
    ///   omits an explicit `:classification` slot on `(defephemeral …)`.
    ///   The one PRODUCTION consumer of the shape — a regression that
    ///   drifted its point-type or substrate axis silently retargets every
    ///   unadorned ephemeral to a different plane.
    /// * `crate::crd`'s + `crate::lib`'s + `crate::lifetime_clock`'s +
    ///   `tatara_reconciler::{claim,render}`'s + `tatara_pool_reconciler::
    ///   controller_pool`'s `empty_spec` / `empty_process_spec` /
    ///   `ephemeral_process` / `permanent_process` test-fixture helpers +
    ///   inline `ProcessSpec` literals — nine test-fixture callsites
    ///   restating the SAME six-line struct-literal at the same shape.
    ///
    /// Post-lift every callsite reads `Classification::gate_compute()`;
    /// a future workspace-wide baseline shift (a new [`Horizon`] default,
    /// a promotion of `Compute` to a compound baseline that pre-fills a
    /// canonical [`CalmClassification`], a per-baseline compliance overlay
    /// stamping through the classification, or a rename of either axis
    /// enum) lands at ONE substrate function here and every downstream
    /// consumer inherits the upgrade mechanically. The current pin ties
    /// the three defaulted axes to the sibling closed-set defaults
    /// ([`HorizonKind::Bounded`], [`CalmClassification::Monotone`],
    /// [`DataClassification::Internal`]) so a future change to any sibling
    /// default surfaces at this primitive's tests rather than as silent
    /// drift across ten independent callsites.
    ///
    /// Sibling to the `_or_default` / `_or_placeholder` primitive family on
    /// [`crate::prelude::Process`] on the (return-form × axis) axis — those
    /// primitives own the borrow-form projections off a live `Process`;
    /// this one owns the construction shape for a fresh
    /// [`crate::crd::ProcessSpec`] whose classification axis is
    /// unremarkable. A future peer `Classification::observe_observability()`
    /// or similar named variant lands as a sibling method here when a
    /// second unremarkable-baseline shape opens.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition — the
    /// six-line struct-literal shape recurred at TEN hand-authored sites
    /// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger and is lifted
    /// onto ONE workspace-wide owner here). THEORY.md §II.1 invariant 5
    /// (composition preserves proofs — a regression that drifted the
    /// baseline axis choice at only one consumer, or that broke the
    /// sibling-default correspondence, surfaces at this primitive's tests
    /// rather than as silent operator-visible skew between the ephemeral
    /// sugar substitute and the ten downstream test-fixtures whose
    /// assertions depend on the shape).
    #[must_use]
    pub fn gate_compute() -> Self {
        Self {
            point_type: ConvergencePointType::Gate,
            substrate: SubstrateType::Compute,
            horizon: Horizon::default(),
            calm: CalmClassification::default(),
            data_classification: DataClassification::default(),
        }
    }

    /// Closed-set-driven presence probe — does this [`Classification`]
    /// carry the given [`ConvergencePointType`] discriminator on its
    /// [`Self::point_type`] slot? The ONE substrate primitive that owns
    /// the `(Classification, ConvergencePointType) -> bool`
    /// scalar-carrier walk shape.
    ///
    /// # Third scalar-carrier peer on the presence-probe axis
    ///
    /// Peer of [`crate::spec::SignalPolicy::has_sighup_strategy`] and
    /// [`crate::encapsulates::EncapsulatesSpec::has_mode`] — all three
    /// probe a scalar closed-set-discriminator field on an inner
    /// [`crate::crd::ProcessSpec`] struct via a one-line
    /// `self.<field> == kind` body. Together they compose the
    /// SCALAR-CARRIER stratum of the workspace-wide closed-set-driven
    /// presence-probe algebra (the workspace-wide algebra spans three
    /// underlying representation kinds — Option-slot, slice, scalar —
    /// see the [`crate::spec::SignalPolicy::has_sighup_strategy`]
    /// docstring for the full-shape rundown; this method is the third
    /// scalar-carrier instance).
    ///
    /// # Semantics — VARIANT match, not POPULATED slot
    ///
    /// `has_point_type(kind)` returns `true` iff `self.point_type ==
    /// kind`. Distinct from BOTH prior scalar-carrier peers on the
    /// (parent-shape × child-shape) axis:
    ///
    /// * [`crate::spec::SignalPolicy::has_sighup_strategy`] lives on a
    ///   non-Option, DEFAULTED parent (`SignalPolicy: Default`) with a
    ///   defaulted scalar child (`SighupStrategy: Default =
    ///   Reconverge`) — a default carrier reads `true` for the default
    ///   variant only.
    /// * [`crate::encapsulates::EncapsulatesSpec::has_mode`] lives on
    ///   an OPTION parent (`spec.encapsulates:
    ///   Option<EncapsulatesSpec>`) with a defaulted scalar child
    ///   (`EncapsulationMode: Default = Manage`) — a bare `None`
    ///   parent reads `false` for every variant.
    /// * `has_point_type` lives on a REQUIRED, non-Option, NON-DEFAULT
    ///   parent ([`Classification`] has no `impl Default`) with a
    ///   NON-DEFAULT scalar child ([`ConvergencePointType`] has no
    ///   `impl Default`) — every well-formed [`crate::crd::ProcessSpec`]
    ///   carries a `Classification` whose `point_type` slot is
    ///   deliberately chosen by the operator, so the probe returns
    ///   `true` on exactly ONE variant per spec and `false` on the
    ///   other seven, with no default-arm short-circuit shortcut.
    ///
    /// This closes the (required-parent × required-scalar-child) corner
    /// of the workspace-wide closed-set-driven presence-probe algebra
    /// at its first substrate primitive.
    ///
    /// # Compounding
    ///
    /// A future closed-set-discriminator scalar field on
    /// [`Classification`] (a peer `has_substrate`, `has_calm`,
    /// `has_data_classification` — the four remaining
    /// classification-axis closed sets) lands as ONE peer inherent
    /// method with the same one-line `self.<field> == kind` body and
    /// routes through the same `strip_and_classify_prefixed_kind::<K,
    /// _>` shape in `tatara-check`. A future
    /// [`ConvergencePointType`] variant (a hypothetical `Demux` /
    /// `Mux` / `Pipeline` for finer topology carving) reaches every
    /// downstream through ONE `ALL` entry on the closed set with the
    /// probe body untouched.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition preserves
    /// proofs; the scalar-carrier presence-probe body lives at ONE
    /// substrate site so every downstream (`point-type-<kind>`
    /// require-tag family in `tatara-check`, closed-set audit
    /// dispatchers, future variant additions on
    /// [`ConvergencePointType`]) binds through the SAME shape rather
    /// than restating the `classification.point_type == kind` closure
    /// body at each callsite. THEORY.md §VI.1 — generation over
    /// composition; a future [`ConvergencePointType`] variant lands at
    /// ONE `ALL` entry + ONE `as_str` arm on the closed set and the
    /// probe picks it up mechanically without further per-consumer
    /// edits.
    #[must_use]
    pub fn has_point_type(&self, kind: ConvergencePointType) -> bool {
        self.point_type == kind
    }

    /// Closed-set-driven presence probe — does this [`Classification`]
    /// carry the given [`SubstrateType`] discriminator on its
    /// [`Self::substrate`] slot? The ONE substrate primitive that
    /// owns the `(Classification, SubstrateType) -> bool`
    /// scalar-carrier walk shape.
    ///
    /// # Fourth scalar-carrier peer on the presence-probe axis
    ///
    /// Peer of [`crate::spec::SignalPolicy::has_sighup_strategy`],
    /// [`crate::encapsulates::EncapsulatesSpec::has_mode`], and
    /// [`Self::has_point_type`] — all four probe a scalar closed-set-
    /// discriminator field on an inner [`crate::crd::ProcessSpec`]
    /// struct via a one-line `self.<field> == kind` body. Together
    /// they compose the SCALAR-CARRIER stratum of the workspace-wide
    /// closed-set-driven presence-probe algebra (the workspace-wide
    /// algebra spans three underlying representation kinds — Option-
    /// slot, slice, scalar — see the
    /// [`crate::spec::SignalPolicy::has_sighup_strategy`] docstring
    /// for the full-shape rundown; this method is the fourth scalar-
    /// carrier instance).
    ///
    /// # Semantics — VARIANT match, not POPULATED slot
    ///
    /// `has_substrate(kind)` returns `true` iff `self.substrate ==
    /// kind`. FIRST co-tenant on the (required-parent × required-
    /// scalar-child) corner of the algebra with [`Self::has_point_type`]
    /// — both probe REQUIRED, non-Option, NON-DEFAULT slots on the
    /// same [`Classification`] parent whose two required axes carry
    /// no [`Default`] impl, so exactly ONE of the eight [`SubstrateType`]
    /// variants and exactly ONE of the eight [`ConvergencePointType`]
    /// variants answer `true` per well-formed [`crate::crd::ProcessSpec`],
    /// with no default-arm short-circuit shortcut. Distinct from the
    /// two prior scalar-carrier peers on the (parent-shape × child-
    /// shape) axis: `has_sighup_strategy` lives on a non-Option,
    /// DEFAULTED parent ([`crate::spec::SignalPolicy`] carries
    /// `#[derive(Default)]`) with a defaulted scalar child
    /// ([`crate::signal::SighupStrategy`] defaults to
    /// [`crate::signal::SighupStrategy::Reconverge`]); `has_mode`
    /// lives on an OPTION parent (`spec.encapsulates:
    /// Option<EncapsulatesSpec>`) with a defaulted scalar child
    /// ([`crate::encapsulates::EncapsulationMode`] defaults to
    /// [`crate::encapsulates::EncapsulationMode::Manage`]).
    ///
    /// This POPULATES the (required-parent × required-scalar-child)
    /// corner of the workspace-wide closed-set-driven presence-probe
    /// algebra at its SECOND substrate primitive after
    /// [`Self::has_point_type`] opened the corner, pinning the corner
    /// as a proven-repeatable primitive shape rather than a single-
    /// example curiosity.
    ///
    /// # Compounding
    ///
    /// A future closed-set-discriminator scalar field on
    /// [`Classification`] (a peer `has_calm` on [`CalmClassification`],
    /// `has_data_classification` on [`DataClassification`] — the two
    /// remaining defaulted-scalar-child classification-axis closed
    /// sets) lands as ONE peer inherent method with the same one-line
    /// `self.<field> == kind` body and routes through the same
    /// `strip_and_classify_prefixed_kind::<K, _>` shape in
    /// `tatara-check`. A future [`SubstrateType`] variant (a
    /// hypothetical `Consensus` for governance substrates, `Physical`
    /// for hardware substrates, `Cache` for ephemeral memoization
    /// substrates) reaches every downstream through ONE `ALL` entry
    /// on the closed set with the probe body untouched.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition preserves
    /// proofs; the scalar-carrier presence-probe body lives at ONE
    /// substrate site so every downstream (`substrate-<kind>`
    /// require-tag family in `tatara-check`, closed-set audit
    /// dispatchers, future variant additions on [`SubstrateType`])
    /// binds through the SAME shape rather than restating the
    /// `classification.substrate == kind` closure body at each
    /// callsite. THEORY.md §VI.1 — generation over composition; a
    /// future [`SubstrateType`] variant lands at ONE `ALL` entry +
    /// ONE `as_str` arm on the closed set and the probe picks it up
    /// mechanically without further per-consumer edits.
    #[must_use]
    pub fn has_substrate(&self, kind: SubstrateType) -> bool {
        self.substrate == kind
    }

    /// Closed-set-driven presence probe — does this [`Classification`]
    /// carry the given [`CalmClassification`] discriminator on its
    /// [`Self::calm`] slot? The ONE substrate primitive that owns the
    /// `(Classification, CalmClassification) -> bool` scalar-carrier
    /// walk shape.
    ///
    /// # Fifth scalar-carrier peer on the presence-probe axis
    ///
    /// Peer of [`crate::spec::SignalPolicy::has_sighup_strategy`],
    /// [`crate::encapsulates::EncapsulatesSpec::has_mode`],
    /// [`Self::has_point_type`], and [`Self::has_substrate`] — all
    /// five probe a scalar closed-set-discriminator field on an inner
    /// [`crate::crd::ProcessSpec`] struct via a one-line
    /// `self.<field> == kind` body. Together they compose the
    /// SCALAR-CARRIER stratum of the workspace-wide closed-set-driven
    /// presence-probe algebra (the workspace-wide algebra spans three
    /// underlying representation kinds — Option-slot, slice, scalar
    /// — see the [`crate::spec::SignalPolicy::has_sighup_strategy`]
    /// docstring for the full-shape rundown; this method is the
    /// fifth scalar-carrier instance).
    ///
    /// # Semantics — VARIANT match, not POPULATED slot
    ///
    /// `has_calm(kind)` returns `true` iff `self.calm == kind`. FIRST
    /// occupant on a FRESH corner of the (parent-shape × child-shape)
    /// axis: a REQUIRED, non-Option, NON-DEFAULT parent
    /// ([`Classification`] has no `impl Default` because its two
    /// required axes `point_type`/`substrate` carry no default)
    /// combined with a DEFAULTED scalar child
    /// ([`CalmClassification`] defaults to
    /// [`CalmClassification::Monotone`] via `#[default]`). Distinct
    /// from every prior scalar-carrier peer on the (parent-shape ×
    /// child-shape) axis:
    ///
    /// * [`crate::spec::SignalPolicy::has_sighup_strategy`] lives on
    ///   a non-Option, DEFAULTED parent
    ///   ([`crate::spec::SignalPolicy`] carries `#[derive(Default)]`)
    ///   with a defaulted scalar child
    ///   ([`crate::signal::SighupStrategy`] defaults to
    ///   [`crate::signal::SighupStrategy::Reconverge`]) — a bare
    ///   `SignalPolicy` reads `true` on the default variant only.
    /// * [`crate::encapsulates::EncapsulatesSpec::has_mode`] lives on
    ///   an OPTION parent (`spec.encapsulates:
    ///   Option<EncapsulatesSpec>`) with a defaulted scalar child
    ///   ([`crate::encapsulates::EncapsulationMode`] defaults to
    ///   [`crate::encapsulates::EncapsulationMode::Manage`]) — a
    ///   bare `None` parent reads `false` for every variant.
    /// * [`Self::has_point_type`] + [`Self::has_substrate`] both live
    ///   on the REQUIRED, non-Option, NON-DEFAULT [`Classification`]
    ///   parent with a NON-DEFAULT scalar child — every well-formed
    ///   [`crate::crd::ProcessSpec`] carries a `Classification` whose
    ///   corresponding slot was deliberately chosen by the operator,
    ///   so exactly ONE of the eight variants answers `true` per
    ///   spec.
    /// * `has_calm` lives on the REQUIRED, non-Option, NON-DEFAULT
    ///   [`Classification`] parent with a DEFAULTED scalar child
    ///   ([`CalmClassification::Monotone`] is the [`Default`] via
    ///   `#[default]`) — a bare `Classification` filled via
    ///   `..Default::default()` on the defaulted axes reads `true`
    ///   for the default variant ([`CalmClassification::Monotone`])
    ///   and `false` for every other. Exactly ONE of the two variants
    ///   answers `true` per spec, and the default-arm short-circuit
    ///   is present (the operator can DECLINE to name the CALM axis
    ///   and the spec still answers `true` on the default variant).
    ///
    /// This OPENS the (required-parent × defaulted-scalar-child)
    /// corner of the workspace-wide closed-set-driven presence-probe
    /// algebra at its first substrate primitive — a corner distinct
    /// from all four prior scalar-carrier peers (which sit on the
    /// three prior corners: defaulted-parent × defaulted-child,
    /// Option-parent × defaulted-child, required-parent ×
    /// required-child).
    ///
    /// # Compounding
    ///
    /// A future closed-set-discriminator scalar field on
    /// [`Classification`] whose child carries `#[derive(Default)]`
    /// (a peer `has_data_classification` on [`DataClassification`],
    /// whose default is [`DataClassification::Internal`] via
    /// `#[default]` — the remaining classification-axis closed set
    /// on a defaulted-scalar-child slot) lands as ONE peer inherent
    /// method with the same one-line `self.<field> == kind` body and
    /// routes through the same `strip_and_classify_prefixed_kind::<K,
    /// _>` shape in `tatara-check`. A future [`CalmClassification`]
    /// variant (a hypothetical `ConditionallyMonotone` for ops that
    /// are monotone under a witness, like CRDT joins under a fixed
    /// schema) reaches every downstream through ONE `ALL` entry on
    /// the closed set with the probe body untouched.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the scalar-carrier presence-probe body
    /// lives at ONE substrate site so every downstream
    /// (`calm-<kind>` require-tag family in `tatara-check`,
    /// closed-set audit dispatchers, future variant additions on
    /// [`CalmClassification`]) binds through the SAME shape rather
    /// than restating the `classification.calm == kind` closure body
    /// at each callsite. THEORY.md §VI.1 — generation over
    /// composition; a future [`CalmClassification`] variant lands at
    /// ONE `ALL` entry + ONE `as_str` arm on the closed set and the
    /// probe picks it up mechanically without further per-consumer
    /// edits.
    #[must_use]
    pub fn has_calm(&self, kind: CalmClassification) -> bool {
        self.calm == kind
    }

    /// Closed-set-driven presence probe — does this [`Classification`]
    /// carry the given [`DataClassification`] discriminator on its
    /// [`Self::data_classification`] slot? The ONE substrate primitive
    /// that owns the `(Classification, DataClassification) -> bool`
    /// scalar-carrier walk shape.
    ///
    /// # Sixth scalar-carrier peer on the presence-probe axis
    ///
    /// Peer of [`crate::spec::SignalPolicy::has_sighup_strategy`],
    /// [`crate::encapsulates::EncapsulatesSpec::has_mode`],
    /// [`Self::has_point_type`], [`Self::has_substrate`], and
    /// [`Self::has_calm`] — all six probe a scalar closed-set-
    /// discriminator field on an inner [`crate::crd::ProcessSpec`]
    /// struct via a one-line `self.<field> == kind` body. Together
    /// they compose the SCALAR-CARRIER stratum of the workspace-wide
    /// closed-set-driven presence-probe algebra (the workspace-wide
    /// algebra spans three underlying representation kinds — Option-
    /// slot, slice, scalar — see the
    /// [`crate::spec::SignalPolicy::has_sighup_strategy`] docstring
    /// for the full-shape rundown; this method is the sixth scalar-
    /// carrier instance).
    ///
    /// # Semantics — VARIANT match, not POPULATED slot
    ///
    /// `has_data_classification(kind)` returns `true` iff
    /// `self.data_classification == kind`. SECOND co-tenant on the
    /// (required-parent × defaulted-scalar-child) corner of the
    /// algebra alongside [`Self::has_calm`] — both probe REQUIRED,
    /// non-Option, NON-DEFAULT [`Classification`] parent slots with
    /// a DEFAULTED scalar child ([`DataClassification`] defaults to
    /// [`DataClassification::Internal`] via `#[default]`, sibling to
    /// [`CalmClassification::Monotone`]'s `#[default]`), so exactly
    /// ONE of the six [`DataClassification`] variants answers `true`
    /// per spec AND the default-arm short-circuit is present (a
    /// `Classification` filled via `..Default::default()` on the
    /// `data_classification` axis reads `true` on the default
    /// variant [`DataClassification::Internal`] and `false` on every
    /// other).
    ///
    /// This POPULATES the (required-parent × defaulted-scalar-child)
    /// corner of the workspace-wide closed-set-driven presence-probe
    /// algebra at its SECOND substrate primitive after
    /// [`Self::has_calm`] opened the corner, pinning the corner as a
    /// proven-repeatable primitive shape rather than a single-example
    /// curiosity. The corner-property contract ("bare
    /// [`Classification`] reads `true` on the default variant")
    /// now walks TWO independent defaulted-scalar-child slots on the
    /// SAME [`Classification`] parent — a regression that promoted
    /// a different [`DataClassification`] variant to `#[default]`
    /// (or wired the arm to a fixed variant answer) fails HERE at
    /// ONE narrow substrate site before drifting through every
    /// unadorned Process's baseline data-classification answer.
    ///
    /// # Compounding
    ///
    /// This method exhausts the four scalar closed-set-discriminator
    /// axes on [`Classification`] ([`Self::has_point_type`],
    /// [`Self::has_substrate`], [`Self::has_calm`], and
    /// [`Self::has_data_classification`]) — the six-axis classification
    /// lattice publishes ALL FOUR of its scalar-carrier presence
    /// probes at ONE substrate site each. The remaining two axes
    /// (`horizon` — a nested struct threading [`HorizonKind`] through
    /// `horizon.kind`; the sixth axis is variant-dependent on the
    /// [`HorizonKind::Asymptotic`] arm) live on nested-struct-scalar
    /// slots rather than the direct-scalar corner the four current
    /// peers span — the [`Self::has_horizon_kind`] peer opens that
    /// fresh (required-parent × nested-struct-scalar-child) corner
    /// with the same `has(kind)` shape composed through one struct
    /// hop. A future [`DataClassification`] variant (a
    /// hypothetical seventh variant beyond `Public / Internal /
    /// Confidential / Pii / Phi / Pci` — say a `TradeSecret` bucket
    /// for competitive-sensitive data, or an `Anonymized` bucket for
    /// pseudonymized-PII whose regulatory posture differs) reaches
    /// every downstream through ONE `ALL` entry on the closed set +
    /// ONE `as_str` arm + ONE `sensitivity_rank` arm + one arm per
    /// predicate with the probe body untouched.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the scalar-carrier presence-probe body
    /// lives at ONE substrate site so every downstream
    /// (`data-classification-<kind>` require-tag family in
    /// `tatara-check`, closed-set audit dispatchers, future variant
    /// additions on [`DataClassification`]) binds through the SAME
    /// shape rather than restating the
    /// `classification.data_classification == kind` closure body at
    /// each callsite. THEORY.md §VI.1 — generation over composition;
    /// a future [`DataClassification`] variant lands at ONE `ALL`
    /// entry + ONE `as_str` arm on the closed set and the probe
    /// picks it up mechanically without further per-consumer edits.
    #[must_use]
    pub fn has_data_classification(&self, kind: DataClassification) -> bool {
        self.data_classification == kind
    }

    /// Closed-set-driven presence probe — does this [`Classification`]
    /// carry the given [`HorizonKind`] discriminator on its
    /// [`Self::horizon`]`.kind` slot? The ONE substrate primitive that
    /// owns the `(Classification, HorizonKind) -> bool` nested-struct-
    /// scalar-carrier walk shape.
    ///
    /// # Seventh peer on the presence-probe axis — first on a fresh corner
    ///
    /// Peer of [`crate::spec::SignalPolicy::has_sighup_strategy`],
    /// [`crate::encapsulates::EncapsulatesSpec::has_mode`],
    /// [`Self::has_point_type`], [`Self::has_substrate`],
    /// [`Self::has_calm`], and [`Self::has_data_classification`] on the
    /// workspace-wide closed-set-driven presence-probe algebra — the
    /// four scalar-carrier peers on [`Classification`] all read a
    /// closed-set discriminator DIRECTLY off a scalar `Classification`
    /// slot (`point_type`, `substrate`, `calm`, `data_classification`).
    /// `has_horizon_kind` instead threads through a NESTED-STRUCT
    /// intermediary ([`Horizon`], the defaulted nested struct owning
    /// the `horizon` axis on the six-axis classification lattice) to
    /// reach a scalar [`HorizonKind`] discriminator on
    /// `horizon.kind`. This OPENS the (required-parent × nested-
    /// struct-scalar-child) corner of the algebra at its FIRST
    /// substrate primitive — a fresh corner distinct from all four
    /// corner-property-exhaustive scalar-carrier peers on
    /// [`Classification`].
    ///
    /// # Semantics — VARIANT match on the nested scalar, not POPULATED nested struct
    ///
    /// `has_horizon_kind(kind)` returns `true` iff
    /// `self.horizon.kind == kind`. The nested [`Horizon`] struct
    /// carries [`Default`] via `#[derive(Default)]` and its `kind`
    /// field defaults to [`HorizonKind::Bounded`] via `#[default]`, so
    /// a [`Classification`] filled via `..Default::default()` on the
    /// `horizon` axis reads `true` on the default kind
    /// [`HorizonKind::Bounded`] and `false` on
    /// [`HorizonKind::Asymptotic`]. The default-arm short-circuit is
    /// therefore present at this corner too — but through the extra
    /// struct hop the peer scalar-carrier peers on the defaulted-
    /// child corner (`has_calm`, `has_data_classification`) walk
    /// directly. A regression that dropped `#[default]` on
    /// [`HorizonKind`], or that replaced `Horizon::default()` in
    /// [`Classification::gate_compute`] with an explicit non-`Bounded`
    /// kind, surfaces at this primitive's tests before drifting
    /// through every unadorned Process's baseline horizon answer.
    ///
    /// # Compounding
    ///
    /// This method OPENS the (required-parent × nested-struct-scalar-
    /// child) corner of the workspace-wide closed-set-driven
    /// presence-probe algebra, distinct from the four corner-property-
    /// exhaustive scalar-carrier peers on [`Classification`]
    /// ([`Self::has_point_type`], [`Self::has_substrate`],
    /// [`Self::has_calm`], [`Self::has_data_classification`]) whose
    /// bodies read a closed-set discriminator directly off a scalar
    /// slot. A future co-tenant on this fresh corner (a peer probe on
    /// another nested-struct's scalar discriminator, e.g. a
    /// hypothetical `has_optimization_direction` reaching
    /// `spec.classification.horizon.direction.unwrap_or_default()`, or
    /// a nested-struct-scalar discriminator on a different `ProcessSpec`
    /// field's inner struct) lands as ONE peer inherent method with
    /// the same two-hop `self.<outer>.<inner> == kind` body and routes
    /// through the same `strip_and_classify_prefixed_kind::<K, _>`
    /// shape in `tatara-check`. A future [`HorizonKind`] variant (a
    /// hypothetical `Periodic` sentinel for "terminates on each
    /// window boundary then re-arms", pre-flagged on the closed set's
    /// `ALL` docstring) reaches every downstream through ONE `ALL`
    /// entry + one `as_str` arm + one `terminates` arm + one
    /// `requires_metric_axes` arm on the closed set with the probe
    /// body untouched.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the nested-struct-scalar-carrier presence-
    /// probe body lives at ONE substrate site so every downstream
    /// (`horizon-<kind>` require-tag family in `tatara-check`, future
    /// audit dispatchers walking [`HorizonKind::ALL`], future variant
    /// additions on [`HorizonKind`]) binds through the SAME
    /// `has(kind)` shape rather than restating the
    /// `classification.horizon.kind == kind` closure body at each
    /// callsite. THEORY.md §VI.1 — generation over composition; a
    /// future [`HorizonKind`] variant lands at ONE `ALL` entry + ONE
    /// `as_str` arm on the closed set and the probe picks it up
    /// mechanically without further per-consumer edits.
    #[must_use]
    pub fn has_horizon_kind(&self, kind: HorizonKind) -> bool {
        self.horizon.kind == kind
    }

    /// Closed-set-driven presence probe — does this [`Classification`]
    /// carry the given [`OptimizationDirection`] discriminator on its
    /// [`Self::horizon`]`.direction` slot (with the substrate
    /// `Option::unwrap_or_default` treating `None` as the closed set's
    /// `#[default] Minimize`)? The ONE substrate primitive that owns
    /// the `(Classification, OptimizationDirection) -> bool` nested-
    /// struct-Option-scalar-carrier walk shape.
    ///
    /// # Second occupant on the nested-struct-scalar-child corner —
    /// the Option-hop co-tenant
    ///
    /// Peer of [`Self::has_horizon_kind`] on the (required-parent ×
    /// nested-struct-scalar-child) corner opened by that method — same
    /// two-hop composition through the nested defaulted [`Horizon`]
    /// intermediary, but the inner scalar slot is `direction:
    /// Option<OptimizationDirection>` (an `Option`-hop past the same
    /// nested [`Horizon`]) rather than a bare scalar. The corner
    /// therefore admits BOTH direct nested-scalar shapes ([`Horizon`]
    /// carries `kind: HorizonKind` directly, [`Self::has_horizon_kind`]
    /// walks it) AND Option-nested-scalar shapes ([`Horizon`] carries
    /// `direction: Option<OptimizationDirection>`, this method walks
    /// it through `Option::unwrap_or_default`), pinning the corner as
    /// a proven-repeatable primitive shape rather than a single-
    /// example curiosity. Every prior scalar-carrier peer on
    /// [`Classification`] (`has_point_type`, `has_substrate`,
    /// `has_calm`, `has_data_classification`) reads a closed-set
    /// discriminator DIRECTLY off a scalar `Classification` slot; this
    /// method (like [`Self::has_horizon_kind`]) threads through the
    /// nested [`Horizon`] intermediary, and additionally traverses the
    /// `Option`-slot with `unwrap_or_default` so the operator's
    /// `:requires (optimization-direction-Minimize)` on an unadorned
    /// baseline still answers `true` on the closed set's default arm.
    ///
    /// # Semantics — VARIANT match on the Option-defaulted nested
    /// scalar, not POPULATED Option
    ///
    /// `has_optimization_direction(kind)` returns `true` iff
    /// `self.horizon.direction.unwrap_or_default() == kind`.
    /// [`OptimizationDirection`] carries `#[default] Minimize` via
    /// the derived [`Default`] impl, so a Process filled through
    /// [`Horizon::bounded`] (which leaves `direction: None`) or
    /// through `Horizon::default()` (same shape, `direction: None`)
    /// answers `true` on [`OptimizationDirection::Minimize`] and
    /// `false` on [`OptimizationDirection::Maximize`]. This mirrors
    /// the default-arm short-circuit contract every other closed-set-
    /// defaulted-child probe on [`Classification`] publishes
    /// (`has_calm`, `has_data_classification`, `has_horizon_kind`) —
    /// the `Option`-hop is soft-mapped to the closed set's default
    /// arm rather than surfaced as a distinct presence axis. A
    /// regression that flipped [`OptimizationDirection`]'s
    /// `#[default]` off `Minimize` (which would silently invert every
    /// unadorned `Asymptotic` Process's rate-window evaluator
    /// polarity — see the [`OptimizationDirection::Minimize`] variant
    /// docstring) fails at this probe's default-arm tests before
    /// drifting through every downstream consumer.
    ///
    /// # Semantics rationale — Option-hop as default vs presence
    ///
    /// The `direction: Option<OptimizationDirection>` slot on
    /// [`Horizon`] is documented as "Asymptotic only" — a `Bounded`
    /// horizon has no meaningful direction so the operator leaves
    /// it `None`. Yet the closed set carries `#[default] Minimize`,
    /// so a bare `Bounded` Process's optimization direction reads
    /// as `Minimize` at every consumer downstream via
    /// [`Option::unwrap_or_default`]. That default IS the substrate's
    /// operator-facing answer for "what direction would this Process
    /// optimize toward if it became Asymptotic without further
    /// annotation?", and a `:requires (optimization-direction-
    /// Minimize)` audit at the checks.lisp surface correctly matches
    /// every unadorned Process — matching the corner-property contract
    /// every other defaulted-child probe publishes. An operator who
    /// wants a strict presence axis (`is direction *actually* set?`)
    /// gets that answer through a distinct future primitive
    /// (`has_optimization_direction_set`) that would read the
    /// `is_some` bit alone — orthogonal to this variant-equality
    /// probe. This method commits to the variant-equality
    /// interpretation so the corner-property contract stays uniform
    /// with the four scalar-carrier peers.
    ///
    /// # Compounding
    ///
    /// This method POPULATES the (required-parent × nested-struct-
    /// scalar-child) corner at its SECOND substrate primitive after
    /// [`Self::has_horizon_kind`] opened it — pinning the corner as
    /// a proven-repeatable primitive shape rather than a single-
    /// example curiosity, and DEMONSTRATING that the corner admits
    /// both direct-scalar and Option-scalar traversals through the
    /// same nested-struct intermediary via the closed set's default.
    /// A future co-tenant on this corner (a peer probe on another
    /// nested-struct's scalar or Option-scalar discriminator, e.g.
    /// a hypothetical `has_backend_port_family` reaching
    /// `spec.routing.as_ref().and_then(|r| r.backend.tls_issuer.as_ref()).is_some()`
    /// or a nested-struct-scalar discriminator on
    /// `spec.encapsulates.<some-inner>.kind`) lands as ONE peer
    /// inherent method with the same two-hop `self.<outer>.<inner>`
    /// walk (with or without an Option-hop threading through the
    /// closed set's `Default`) and routes through the same
    /// `strip_and_classify_prefixed_kind::<K, _>` shape in
    /// `tatara-check`. A future [`OptimizationDirection`] variant
    /// (a hypothetical `Stabilize` sentinel for "drive toward a
    /// target value", pre-flagged on the closed set's `ALL`
    /// docstring) reaches every downstream through ONE `ALL` entry
    /// + one `as_str` arm + one `prefers_lower` arm + one
    /// `is_improvement` arm on the closed set with the probe body
    /// untouched.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the nested-struct-Option-scalar-carrier
    /// presence-probe body lives at ONE substrate site so every
    /// downstream (`optimization-direction-<kind>` require-tag family
    /// in `tatara-check`, future audit dispatchers walking
    /// [`OptimizationDirection::ALL`], future variant additions on
    /// [`OptimizationDirection`]) binds through the SAME
    /// `has(kind)` shape rather than restating the
    /// `classification.horizon.direction.unwrap_or_default() == kind`
    /// closure body at each callsite. THEORY.md §VI.1 — generation
    /// over composition; a future [`OptimizationDirection`] variant
    /// lands at ONE `ALL` entry + ONE `as_str` arm on the closed set
    /// and the probe picks it up mechanically without further
    /// per-consumer edits.
    #[must_use]
    pub fn has_optimization_direction(&self, kind: OptimizationDirection) -> bool {
        self.horizon.direction.unwrap_or_default() == kind
    }

    /// Closed-set-driven presence probe — does this [`Classification`]
    /// carry a [`ConvergencePointType`] whose typed input-edge
    /// cardinality projection ([`ConvergencePointType::input_arity`])
    /// matches the given [`Arity`] discriminator? The ONE substrate
    /// primitive that owns the `(Classification, Arity) -> bool`
    /// derived-typed-projection walk shape.
    ///
    /// # Third occupant on the (required-parent × nested-struct-scalar-child) corner — first via a derived-typed-projection
    ///
    /// Peer of [`Self::has_horizon_kind`] and
    /// [`Self::has_optimization_direction`] on the (required-parent ×
    /// nested-struct-scalar-child) corner. Distinct from the two on
    /// ONE dimension: those two read a raw discriminator directly off
    /// the nested [`Horizon`] slot (`self.horizon.kind` /
    /// `self.horizon.direction.unwrap_or_default()`) so the child's
    /// closed set IS the field's type; this probe threads through the
    /// closed-set typed projection [`ConvergencePointType::input_arity`]
    /// (a `const fn` many-to-one collapse `Transform | Fork |
    /// Broadcast | Observe → One`, `Join | Gate | Select | Reduce →
    /// Many`) so the child's closed set is REACHED THROUGH a typed
    /// projection layer, not read raw off a scalar. Byte-for-byte
    /// symmetric with the derived-typed-projection precedent set by
    /// [`crate::export::ExportSpecSliceExt::has_report_payload_shape`]
    /// on the (Option-parent × Vec-child × nested-Option-carrier ×
    /// derived-typed-projection) corner — that peer routes through
    /// [`crate::export::ReportFormat::payload_shape`] the same way
    /// this method routes through [`ConvergencePointType::input_arity`].
    /// FIRST occupant of the derived-typed-projection variant on the
    /// (required-parent × nested-struct-scalar-child) corner —
    /// widening the corner from "raw discriminator only" to "raw
    /// discriminator OR typed projection over the child" and pinning
    /// the corner as a proven-repeatable primitive shape rather than a
    /// direct-field-equality curiosity.
    ///
    /// # Semantics — VARIANT match on the projected image, not on the source
    ///
    /// `has_input_arity(kind)` returns `true` iff
    /// `self.point_type.input_arity() == kind`. [`Arity`] carries no
    /// `Default` impl (the `Arity::ALL` closed set is a bare 2-arm
    /// enum with no `#[default]`), so exactly ONE of the two arms
    /// answers `true` per well-formed [`crate::crd::ProcessSpec`],
    /// with no default-arm short-circuit shortcut. The many-to-one
    /// projection shape means the answer is invariant under intra-
    /// bucket point-type swaps (`Transform ↔ Fork ↔ Broadcast ↔
    /// Observe` all keep `input-arity-One = true`) and flips at
    /// bucket boundaries (`Transform ↔ Join` flips `input-arity-One`
    /// from `true` to `false`). A regression that (a) probed
    /// [`ConvergencePointType`] directly (dropping the
    /// `.input_arity()` call), (b) inverted the projection (`One ↔
    /// Many`), or (c) crossed the wires with the sibling
    /// [`ConvergencePointType::output_arity`] projection (which
    /// disagrees on the fan-out arms `Fork | Broadcast → Many` vs.
    /// `input_arity`'s `Fork | Broadcast → One`) fails at this probe's
    /// substrate site before drifting through every downstream
    /// consumer.
    ///
    /// # Compounding
    ///
    /// This method POPULATES the (required-parent × nested-struct-
    /// scalar-child) corner at its THIRD substrate primitive after
    /// [`Self::has_horizon_kind`] opened it (direct-nested-scalar) and
    /// [`Self::has_optimization_direction`] populated it
    /// (Option-nested-scalar). Together the three demonstrate the
    /// corner admits three traversal shapes through the SAME
    /// two-hop `self.<field>.<projection>` walk: direct-scalar,
    /// Option-scalar-with-default, and derived-typed-projection. A
    /// future co-tenant reading a projected value off the same
    /// [`ConvergencePointType`] (a peer `has_output_arity` reading
    /// `self.point_type.output_arity() == kind` — the natural fourth
    /// occupant, opening the pair for DAG-composition axis coverage;
    /// a hypothetical `has_topology_bucket` reading `.is_preserving()`
    /// / `.is_diffusive()` / `.is_convergent()`) lands as ONE peer
    /// inherent method with the same one-line
    /// `self.point_type.<projection>() == kind` body and routes
    /// through the same `strip_and_classify_prefixed_kind::<K, _>`
    /// shape in `tatara-check`. A future [`ConvergencePointType`]
    /// variant (a hypothetical `Demux` for `One → Many` or `Mux` for
    /// `Many → One`) reaches every downstream through ONE `ALL`
    /// entry + one `as_str` arm + one `input_arity` arm + one
    /// `output_arity` arm on the closed set with THIS probe body
    /// untouched — the many-to-one projection means the bucket
    /// membership shift lands exactly at the projection's own site.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-typed-projection presence-probe
    /// body lives at ONE substrate site so every downstream
    /// (`input-arity-<kind>` require-tag family in `tatara-check`,
    /// future DAG-composition validators, future variant additions
    /// on [`ConvergencePointType`]) binds through the SAME
    /// `has(kind)` shape rather than restating the
    /// `classification.point_type.input_arity() == kind` closure body
    /// at each callsite. THEORY.md §VI.1 — generation over
    /// composition; a future [`Arity`] variant (a hypothetical `Zero`
    /// for sinks) lands at ONE `ALL` entry + ONE `as_str` arm on the
    /// closed set + ONE arm on each `input_arity`/`output_arity`
    /// projection and the probe picks it up mechanically without
    /// further per-consumer edits.
    #[must_use]
    pub fn has_input_arity(&self, kind: Arity) -> bool {
        self.point_type.input_arity() == kind
    }

    /// Closed-set-driven presence probe — does this [`Classification`]
    /// carry a [`ConvergencePointType`] whose typed output-edge
    /// cardinality projection ([`ConvergencePointType::output_arity`])
    /// matches the given [`Arity`] discriminator? The ONE substrate
    /// primitive that owns the `(Classification, Arity) -> bool`
    /// output-side derived-typed-projection walk shape.
    ///
    /// # Fourth occupant on the (required-parent × nested-struct-scalar-child) corner — second via a derived-typed-projection; closes the DAG-composition arity pair
    ///
    /// Peer of [`Self::has_horizon_kind`],
    /// [`Self::has_optimization_direction`], and
    /// [`Self::has_input_arity`] on the (required-parent ×
    /// nested-struct-scalar-child) corner. Byte-for-byte symmetric with
    /// [`Self::has_input_arity`]: this method walks the SAME
    /// `self.point_type` scalar carrier through the SAME `Arity`
    /// closed set — the sole distinction is the typed projection
    /// composed on the walk. `has_input_arity` composes
    /// [`ConvergencePointType::input_arity`] (`Transform | Fork |
    /// Broadcast | Observe → One`, `Join | Gate | Select | Reduce →
    /// Many`); this method composes
    /// [`ConvergencePointType::output_arity`] (`Fork | Broadcast →
    /// Many`, everything else → `One`). Together the two probes close
    /// the DAG-composition arity pair — the `(input_arity,
    /// output_arity)` typed projection that pins each variant to
    /// exactly one cell of the `Arity × Arity` topology table
    /// (endomorphic `(One, One)`, diffusive `(One, Many)`, convergent
    /// `(Many, One)`) so future DAG-composition validators dispatch on
    /// a typed projection rather than re-deriving from variant names.
    /// SECOND derived-typed-projection occupant on the
    /// (required-parent × nested-struct-scalar-child) corner — pinning
    /// the corner's "one carrier, N typed-projection probes" property
    /// with a second projection over the same source closed set.
    ///
    /// # Semantics — VARIANT match on the OUTPUT-projected image
    ///
    /// `has_output_arity(kind)` returns `true` iff
    /// `self.point_type.output_arity() == kind`. [`Arity`] carries no
    /// `Default` impl, so exactly ONE of the two arms answers `true`
    /// per well-formed [`crate::crd::ProcessSpec`], with no default-
    /// arm short-circuit. The many-to-one projection shape means the
    /// answer is invariant under intra-bucket swaps (`Fork ↔
    /// Broadcast` both keep `output-arity-Many = true`; `Transform ↔
    /// Join ↔ Gate ↔ Select ↔ Reduce ↔ Observe` all keep
    /// `output-arity-One = true`) and flips at bucket boundaries
    /// (`Fork ↔ Transform` flips `output-arity-Many` from `true` to
    /// `false`). CROSS-PROJECTION DIAGONAL: `Fork | Broadcast` have
    /// `(input_arity, output_arity) = (One, Many)` so `has_input_arity`
    /// and `has_output_arity` DISAGREE on those two variants (the
    /// diffusive bucket is the unique cell where the two projections
    /// answer opposite `Arity` values); `Join | Gate | Select |
    /// Reduce` have `(Many, One)` so the two probes disagree there too
    /// (the convergent bucket is the mirror cell); `Transform |
    /// Observe` have `(One, One)` so the two probes AGREE (the
    /// endomorphic bucket). A regression that (a) probed
    /// [`ConvergencePointType`] directly (dropping the
    /// `.output_arity()` call), (b) inverted the projection (`One ↔
    /// Many`), or (c) crossed the wires with
    /// [`ConvergencePointType::input_arity`] (which disagrees on the
    /// four arms in the diffusive + convergent cells) fails at this
    /// probe's substrate site before drifting through every downstream
    /// consumer.
    ///
    /// # Compounding
    ///
    /// This method POPULATES the DAG-composition arity pair for full
    /// axis coverage — the natural fourth occupant the
    /// [`Self::has_input_arity`] docstring names as the next
    /// derived-typed-projection co-tenant on the same
    /// `self.point_type` carrier. Operators authoring `(defpoint …
    /// :requires (output-arity-Many))` in `checks.lisp` now get typed
    /// access to the fan-out axis (edge-cardinality checks: "every
    /// diffusive topology point emits fan-out" — the exact fleet-wide
    /// property the (`Fork | Broadcast`, `Many`) projection composition
    /// is designed to name) as the mirror of the input-side family,
    /// and the two conjoined (`input-arity-One AND
    /// output-arity-Many`) names the diffusive bucket exactly through
    /// the two typed projections rather than through the OR of raw
    /// `point-type-<Fork | Broadcast>` conjuncts. A future
    /// [`ConvergencePointType`] variant (a hypothetical `Demux` for
    /// `One → Many` or `Mux` for `Many → One`) reaches every downstream
    /// through ONE `ALL` entry + one `as_str` arm + one `input_arity`
    /// arm + one `output_arity` arm on the closed set with THIS probe
    /// body untouched — the many-to-one projection means the bucket
    /// membership shift lands exactly at each projection's own site,
    /// not at every consumer that previously restated the bucket in
    /// code.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the second derived-typed-projection presence-
    /// probe body over the SAME `self.point_type` carrier lives at ONE
    /// substrate site so every downstream (`output-arity-<kind>`
    /// require-tag family in `tatara-check`, future DAG-composition
    /// validators, future variant additions on
    /// [`ConvergencePointType`]) binds through the SAME `has(kind)`
    /// shape. THEORY.md §VI.1 — generation over composition; a future
    /// [`Arity`] variant lands at ONE `ALL` entry + ONE `as_str` arm
    /// on the closed set + ONE arm on each `input_arity`/`output_arity`
    /// projection and both probes pick it up mechanically.
    #[must_use]
    pub fn has_output_arity(&self, kind: Arity) -> bool {
        self.point_type.output_arity() == kind
    }

    /// Derived-boolean predicate — does this [`Classification`] carry a
    /// [`Horizon`] whose kind projects to `true` under
    /// [`HorizonKind::terminates`]? The ONE substrate primitive that
    /// owns the `(Classification) -> bool` derived-nullary-predicate
    /// walk shape on the `horizon.kind` slot.
    ///
    /// # First occupant on the (required-parent × nested-struct-derived-nullary-bool) corner
    ///
    /// Distinct from every prior presence-probe method on
    /// [`Classification`] — those all admit a closed-set `kind`
    /// argument that the probe compares against the stored /
    /// projected discriminator ([`Self::has_horizon_kind`] walks
    /// `horizon.kind == kind`, [`Self::has_optimization_direction`]
    /// walks `horizon.direction.unwrap_or_default() == kind`,
    /// [`Self::has_input_arity`] / [`Self::has_output_arity`] walk
    /// `point_type.<projection>() == kind`). This probe has NO
    /// argument at all: it collapses [`HorizonKind::ALL`] onto a
    /// single boolean question ("does this horizon terminate?") via
    /// the closed set's own [`HorizonKind::terminates`] predicate,
    /// so callers asking the workspace-wide scheduler-facing
    /// question "will this Process ever reach [`crate::phase::ProcessPhase::Reaped`]
    /// via natural termination" reach the answer through a nullary
    /// substrate call rather than restating
    /// `classification.horizon.kind.terminates()` at every consumer.
    ///
    /// # Semantics — derived nullary boolean, not variant equality
    ///
    /// `horizon_terminates()` returns `true` iff
    /// `self.horizon.kind.terminates()`. The two-variant
    /// [`HorizonKind`] closed set publishes the truth table:
    /// [`HorizonKind::Bounded`] → `true` (has a fixed point,
    /// distance reaches 0, terminates naturally);
    /// [`HorizonKind::Asymptotic`] → `false` (runs in perpetuity,
    /// rate is the health signal, never terminates on its own). A
    /// [`Classification::gate_compute`] baseline (which uses
    /// [`Horizon::default`] with `kind = HorizonKind::Bounded` via
    /// `#[default]`) answers `true` — the substrate's default-arm
    /// short-circuit propagates through the nested [`Horizon`]
    /// struct's own [`Default`] impl to this predicate's answer
    /// the same way it propagates through
    /// [`Self::has_horizon_kind`]'s `HorizonKind::Bounded` arm.
    ///
    /// A future third [`HorizonKind`] variant (a hypothetical
    /// `Periodic` sentinel for "terminates on each window boundary
    /// then re-arms" — pre-flagged on the closed set's `ALL`
    /// docstring) reaches this probe through ONE `terminates` arm
    /// on the closed set with the probe body untouched — the
    /// nullary-predicate shape defers every per-variant policy
    /// decision to the closed set's own truth table
    /// ([`HorizonKind::terminates`]) rather than duplicating the
    /// discriminator sweep here.
    ///
    /// # Compounding
    ///
    /// This method OPENS the (required-parent ×
    /// nested-struct-derived-nullary-bool) corner of the workspace-
    /// wide closed-set-driven presence-probe algebra at its FIRST
    /// substrate primitive — distinct from every prior corner
    /// occupant on [`Classification`] (which all take a closed-set
    /// `kind` argument). A future co-tenant on this fresh corner (a
    /// peer nullary predicate on another nested-struct's derived
    /// boolean projection — a hypothetical `horizon_requires_metric_axes`
    /// composing [`HorizonKind::requires_metric_axes`] as the
    /// antisymmetric partner of `horizon_terminates`; a hypothetical
    /// `intent_is_helm_driven` composing over the tagged-union
    /// intent variants; a peer collapsing a routing form's
    /// [`crate::routing::RoutingForm::ALL`] → bool) lands as ONE
    /// peer inherent method with the same nullary derived body and
    /// routes through the same fixed-tag substrate in
    /// [`tatara-check`]'s classifier — no per-consumer restatement
    /// of the `classification.<field>.<projection>()` chain.
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `terminating-horizon` on
    /// [`POINT_FIXED_TAG_ARMS`] — byte-for-byte peer of the fixed
    /// tags [`FixedTagArm`] already publishes (`depends-on`,
    /// `boundary-pre`, `boundary-post`, `compliance`, `signals`).
    /// The ephemeral surface publishes the same tag via
    /// [`crate::ephemeral::EphemeralSpec::horizon_terminates`], which
    /// composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (terminating-horizon)` audit answers the same
    /// question on both surfaces.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `terminating-horizon` fixed tag in [`tatara-check`], future
    /// scheduler / termination-shape validators, future variant
    /// additions on [`HorizonKind`]) binds through the SAME
    /// `horizon_terminates()` shape rather than restating the
    /// `classification.horizon.kind.terminates()` chain at each
    /// callsite. THEORY.md §VI.1 — generation over composition; a
    /// future [`HorizonKind`] variant lands at ONE `ALL` entry +
    /// ONE `terminates` arm on the closed set and this probe picks
    /// it up mechanically.
    #[must_use]
    pub fn horizon_terminates(&self) -> bool {
        self.horizon.kind.terminates()
    }

    /// Derived-boolean predicate — does this [`Classification`] carry a
    /// [`Horizon`] whose kind projects to `true` under
    /// [`HorizonKind::requires_metric_axes`]? The ONE substrate
    /// primitive that owns the `(Classification) -> bool` derived-
    /// nullary-predicate walk shape on the `horizon.kind` slot for
    /// the metric-axes-required question.
    ///
    /// # Second occupant on the (required-parent × nested-struct-derived-nullary-bool) corner
    ///
    /// Byte-for-byte peer of [`Self::horizon_terminates`] via the SAME
    /// closed set [`HorizonKind`] reached through the SAME nested
    /// [`Horizon`] struct: [`Self::horizon_terminates`] composes
    /// [`HorizonKind::terminates`] as `self.horizon.kind.terminates()`;
    /// this method composes the ANTISYMMETRIC partner
    /// [`HorizonKind::requires_metric_axes`] as
    /// `self.horizon.kind.requires_metric_axes()`. The closed set
    /// pins the XOR contract
    /// `terminates() ^ requires_metric_axes()` on every variant (see
    /// `horizon_kind_terminate_xor_requires_metric_axes` on the closed
    /// set itself), so exactly ONE of the two derived-nullary probes
    /// answers `true` per [`Classification`] and the two probes
    /// together partition [`HorizonKind::ALL`] into two disjoint
    /// buckets. This POPULATES the (required-parent × nested-struct-
    /// derived-nullary-bool) corner of the workspace-wide closed-set-
    /// driven presence-probe algebra at its SECOND substrate primitive
    /// after [`Self::horizon_terminates`] opened the corner, pinning
    /// the corner as a proven-repeatable primitive shape rather than
    /// a single-example curiosity.
    ///
    /// # Semantics — derived nullary boolean, not variant equality
    ///
    /// `horizon_requires_metric_axes()` returns `true` iff
    /// `self.horizon.kind.requires_metric_axes()`. The two-variant
    /// [`HorizonKind`] closed set publishes the truth table:
    /// [`HorizonKind::Bounded`] → `false` (has a fixed point, no
    /// asymptotic metric axes required); [`HorizonKind::Asymptotic`]
    /// → `true` (runs in perpetuity, `rate` and `oscillation` are the
    /// health signal and must be measured). A
    /// [`Classification::gate_compute`] baseline (which uses
    /// [`Horizon::default`] with `kind = HorizonKind::Bounded` via
    /// `#[default]`) answers `false` — the substrate's default-arm
    /// short-circuit propagates through the nested [`Horizon`]
    /// struct's own [`Default`] impl to this predicate's answer, the
    /// mirror image of [`Self::horizon_terminates`]'s default-arm
    /// answer.
    ///
    /// A future third [`HorizonKind`] variant (a hypothetical
    /// `Periodic` sentinel for "terminates on each window boundary
    /// then re-arms" — pre-flagged on the closed set's `ALL`
    /// docstring) reaches this probe through ONE `requires_metric_axes`
    /// arm on the closed set with the probe body untouched — the
    /// nullary-predicate shape defers every per-variant policy
    /// decision to the closed set's own truth table
    /// ([`HorizonKind::requires_metric_axes`]) rather than duplicating
    /// the discriminator sweep here.
    ///
    /// # Compounding
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `metric-axes-required` on
    /// `POINT_FIXED_TAG_ARMS` — byte-for-byte antisymmetric peer of
    /// the sibling `terminating-horizon` fixed tag. The ephemeral
    /// surface publishes the same tag via
    /// [`crate::ephemeral::EphemeralSpec::horizon_requires_metric_axes`],
    /// which composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (metric-axes-required)` audit answers the same
    /// question on both surfaces.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `metric-axes-required` fixed tag in `tatara-check`, future
    /// scheduler / metric-provisioning validators, future variant
    /// additions on [`HorizonKind`]) binds through the SAME
    /// `horizon_requires_metric_axes()` shape rather than restating
    /// the `classification.horizon.kind.requires_metric_axes()` chain
    /// at each callsite. THEORY.md §VI.1 — generation over
    /// composition; a future [`HorizonKind`] variant lands at ONE
    /// `ALL` entry + ONE `requires_metric_axes` arm on the closed set
    /// and this probe picks it up mechanically.
    #[must_use]
    pub fn horizon_requires_metric_axes(&self) -> bool {
        self.horizon.kind.requires_metric_axes()
    }

    /// Derived-boolean predicate — does this [`Classification`] carry a
    /// [`CalmClassification`] whose variant projects to `true` under
    /// [`CalmClassification::requires_coordination`]? The ONE substrate
    /// primitive that owns the `(Classification) -> bool` derived-
    /// nullary-predicate walk shape on the `calm` slot.
    ///
    /// # Third occupant on the (parent × derived-nullary-bool) corner
    ///
    /// Peer of [`Self::horizon_terminates`] and
    /// [`Self::horizon_requires_metric_axes`] on the workspace-wide
    /// (parent × derived-nullary-bool) corner of the closed-set-driven
    /// presence-probe algebra — the FIRST occupant threading the
    /// `calm` axis rather than the `horizon.kind` sub-axis. Distinct
    /// from the two `horizon.*` peers by ONE structural degree: this
    /// probe reads a DIRECT scalar closed-set field
    /// ([`Self::calm`]) rather than the NESTED-STRUCT projection
    /// (`self.horizon.kind`) both `horizon_*` peers walk; the derived-
    /// nullary shape and the truth-table composition style match
    /// exactly. Populates the corner as a proven-repeatable primitive
    /// shape across TWO distinct closed-set axes (`HorizonKind`,
    /// `CalmClassification`) rather than an axis-local curiosity.
    ///
    /// # Semantics — derived nullary boolean, not variant equality
    ///
    /// `calm_requires_coordination()` returns `true` iff
    /// `self.calm.requires_coordination()`. The two-variant
    /// [`CalmClassification`] closed set publishes the truth table
    /// (the CALM theorem's typed image, Hellerstein 2010):
    /// [`CalmClassification::Monotone`] → `false` (can be distributed
    /// without coordination); [`CalmClassification::NonMonotone`] →
    /// `true` (requires coordination). A
    /// [`Classification::gate_compute`] baseline (which uses
    /// [`CalmClassification::default = Monotone`] via `#[default]`)
    /// answers `false` — the substrate's default-arm short-circuit
    /// propagates through the scalar closed-set field's own
    /// [`Default`] impl to this predicate's answer. The mirror-image
    /// distinguishing feature vs the two `horizon_*` peers: those
    /// short-circuit through TWO layers of `Default`
    /// ([`Horizon::default`] → [`HorizonKind::default`]); this probe
    /// short-circuits through ONE layer of `Default`
    /// ([`CalmClassification::default`]) because `Self::calm` is a
    /// direct scalar rather than a nested struct wrapper.
    ///
    /// A future third [`CalmClassification`] variant (a hypothetical
    /// `ConditionallyMonotone` sentinel — pre-flagged on the closed
    /// set's `ALL` docstring) reaches this probe through ONE
    /// `requires_coordination` arm on the closed set with the probe
    /// body untouched — the nullary-predicate shape defers every
    /// per-variant policy decision to the closed set's own truth
    /// table ([`CalmClassification::requires_coordination`]) rather
    /// than duplicating the discriminator sweep here.
    ///
    /// # Compounding
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `coordination-required` on
    /// `POINT_FIXED_TAG_ARMS` — byte-for-byte peer of the sibling
    /// `terminating-horizon` and `metric-axes-required` fixed tags on
    /// the (parent × derived-nullary-bool) corner. The ephemeral
    /// surface publishes the same tag via
    /// [`crate::ephemeral::EphemeralSpec::calm_requires_coordination`],
    /// which composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (coordination-required)` audit answers the same
    /// question on both surfaces.
    ///
    /// Future scheduler dispatch between Raft writes and gossip
    /// propagation (documented on
    /// [`CalmClassification::requires_coordination`] itself) reads
    /// THIS predicate rather than re-deriving from the variant name
    /// at each callsite — the classification-axis lattice-typed
    /// image of the CALM theorem lives at ONE substrate site and
    /// every scheduler / coordination-mode chooser downstream binds
    /// through the SAME `calm_requires_coordination()` shape.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `coordination-required` fixed tag in `tatara-check`, future
    /// scheduler / coordination-mode validators, future variant
    /// additions on [`CalmClassification`]) binds through the SAME
    /// `calm_requires_coordination()` shape rather than restating the
    /// `classification.calm.requires_coordination()` chain at each
    /// callsite. THEORY.md §VI.1 — generation over composition; a
    /// future [`CalmClassification`] variant lands at ONE `ALL` entry +
    /// ONE `requires_coordination` arm on the closed set and this probe
    /// picks it up mechanically.
    #[must_use]
    pub fn calm_requires_coordination(&self) -> bool {
        self.calm.requires_coordination()
    }

    /// Derived-boolean predicate — does this [`Classification`] carry a
    /// [`DataClassification`] whose variant projects to `true` under
    /// [`DataClassification::is_regulated`]? The ONE substrate primitive
    /// that owns the `(Classification) -> bool` derived-nullary-
    /// predicate walk shape on the `data_classification` slot for the
    /// regulated-data question.
    ///
    /// # Fourth occupant on the (parent × derived-nullary-bool) corner
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`], and
    /// [`Self::calm_requires_coordination`] on the workspace-wide
    /// (parent × derived-nullary-bool) corner of the closed-set-driven
    /// presence-probe algebra — the FIRST occupant threading the
    /// classification-data axis rather than the horizon or calm
    /// sub-axes. Populates the corner across THREE distinct closed-set
    /// axes (`HorizonKind`, `CalmClassification`, `DataClassification`)
    /// rather than two — pinning the corner as a proven-repeatable
    /// primitive shape across the substrate's three classification-
    /// axis closed sets that publish a `#[default]` variant, not a
    /// single-axis or two-axis curiosity. Byte-for-byte structural
    /// peer of [`Self::calm_requires_coordination`]: both walk a
    /// DIRECT scalar closed-set field (`self.calm` /
    /// `self.data_classification`) on the [`Classification`] parent —
    /// TWO layers of `Default` short-circuit (`Classification::gate_compute`
    /// → the direct scalar child's `#[default]`) — distinct from the
    /// two `horizon_*` peers which walk a NESTED-STRUCT projection
    /// (`self.horizon.kind`) with THREE layers of `Default`
    /// (`Classification::gate_compute` → `Horizon::default` →
    /// `HorizonKind::default`). SECOND direct-scalar peer on the
    /// corner: `calm_requires_coordination` opened the direct-scalar
    /// variant, this method populates it, pinning "direct-scalar
    /// derived-nullary-bool" as a proven-repeatable structural
    /// sub-corner rather than a single-example curiosity.
    ///
    /// # Semantics — derived nullary boolean, not variant equality
    ///
    /// `data_is_regulated()` returns `true` iff
    /// `self.data_classification.is_regulated()`. The six-variant
    /// [`DataClassification`] closed set publishes the truth table:
    /// [`DataClassification::Public`] / [`DataClassification::Internal`]
    /// / [`DataClassification::Confidential`] → `false` (not subject
    /// to external regulatory regime); [`DataClassification::Pii`] /
    /// [`DataClassification::Phi`] / [`DataClassification::Pci`] →
    /// `true` (HIPAA / PCI-DSS / GDPR-style data-subject controls
    /// apply). A [`Classification::gate_compute`] baseline (which
    /// uses [`DataClassification::default = Internal`] via
    /// `#[default]`) answers `false` — the substrate's default-arm
    /// short-circuit propagates through the scalar closed-set field's
    /// own [`Default`] impl to this predicate's answer, mirror image
    /// of [`Self::calm_requires_coordination`]'s Monotone-default
    /// short-circuit through the same structural depth.
    ///
    /// The closed-set-internal pin
    /// `data_classification_regulated_implies_restricted` seals the
    /// implication `is_regulated() ⇒ is_restricted()` on every
    /// variant, so a `true` answer here implies the sibling
    /// (`data_is_restricted`, when it lands) also answers `true`;
    /// the reverse does not hold (`Internal | Confidential` are
    /// restricted but not regulated).
    ///
    /// A future seventh [`DataClassification`] variant (a hypothetical
    /// `TradeSecret` bucket for competitive-sensitive data, or an
    /// `Anonymized` bucket for pseudonymized-PII whose regulatory
    /// posture differs from raw PII) reaches this probe through ONE
    /// `is_regulated` arm on the closed set with the probe body
    /// untouched — the nullary-predicate shape defers every per-
    /// variant policy decision to the closed set's own truth table
    /// ([`DataClassification::is_regulated`]) rather than duplicating
    /// the discriminator sweep here.
    ///
    /// # Compounding
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `data-regulated` on `POINT_FIXED_TAG_ARMS` —
    /// byte-for-byte peer of the sibling `terminating-horizon`,
    /// `metric-axes-required`, and `coordination-required` fixed tags
    /// on the (parent × derived-nullary-bool) corner. The ephemeral
    /// surface publishes the same tag via
    /// [`crate::ephemeral::EphemeralSpec::data_is_regulated`], which
    /// composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (data-regulated)` audit answers the same question
    /// on both surfaces.
    ///
    /// Future compliance-baseline auto-selectors dispatching on the
    /// `(is_regulated, is_restricted)` two-axis projection
    /// (documented on [`DataClassification::is_regulated`] itself)
    /// read THIS predicate rather than re-deriving from the variant
    /// name at each callsite — the classification-data-axis lattice-
    /// typed image of the regulated-data question lives at ONE
    /// substrate site and every compliance-mode chooser downstream
    /// binds through the SAME `data_is_regulated()` shape.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `data-regulated` fixed tag in `tatara-check`, future
    /// compliance-baseline / regulatory-regime validators, future
    /// variant additions on [`DataClassification`]) binds through
    /// the SAME `data_is_regulated()` shape rather than restating the
    /// `classification.data_classification.is_regulated()` chain at
    /// each callsite. THEORY.md §VI.1 — generation over composition;
    /// a future [`DataClassification`] variant lands at ONE `ALL`
    /// entry + ONE `is_regulated` arm on the closed set and this
    /// probe picks it up mechanically.
    #[must_use]
    pub fn data_is_regulated(&self) -> bool {
        self.data_classification.is_regulated()
    }

    /// Derived-boolean predicate — does this [`Classification`] carry a
    /// [`DataClassification`] whose variant projects to `true` under
    /// [`DataClassification::is_restricted`]? The ONE substrate primitive
    /// that owns the `(Classification) -> bool` derived-nullary-
    /// predicate walk shape on the `data_classification` slot for the
    /// restricted-data question.
    ///
    /// # Fifth occupant on the (parent × derived-nullary-bool) corner
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], and
    /// [`Self::data_is_regulated`] on the workspace-wide (parent ×
    /// derived-nullary-bool) corner of the closed-set-driven presence-
    /// probe algebra — the SECOND peer threading the classification-
    /// data axis after [`Self::data_is_regulated`] opened it, pinning
    /// the classification-data axis as a proven-repeatable structural
    /// sub-corner across TWO sibling closed-set projections
    /// (`DataClassification::is_regulated` / `is_restricted`) rather
    /// than a single-projection curiosity. Byte-for-byte structural
    /// peer of [`Self::data_is_regulated`]: both walk the SAME
    /// direct scalar closed-set field (`self.data_classification`) on
    /// the [`Classification`] parent through TWO layers of `Default`
    /// short-circuit (`Classification::gate_compute` →
    /// [`DataClassification::default = Internal`]) — distinct from the
    /// two `horizon_*` peers by ONE structural degree (they walk a
    /// NESTED-STRUCT projection with THREE layers of `Default`). THIRD
    /// direct-scalar peer on the corner after
    /// [`Self::calm_requires_coordination`] opened +
    /// [`Self::data_is_regulated`] populated the sub-corner: seals
    /// "direct-scalar derived-nullary-bool" as the substrate's third
    /// occupant on the sub-corner and the FIRST corner peer whose
    /// gate-compute baseline projects to `true` rather than `false`,
    /// mirror-image of the `Bounded`-default `terminating-horizon`
    /// baseline on the horizon-axis nested sub-corner.
    ///
    /// # Semantics — derived nullary boolean, not variant equality
    ///
    /// `data_is_restricted()` returns `true` iff
    /// `self.data_classification.is_restricted()`. The six-variant
    /// [`DataClassification`] closed set publishes the truth table:
    /// [`DataClassification::Public`] → `false` (freely distributable);
    /// [`DataClassification::Internal`] / [`DataClassification::Confidential`]
    /// / [`DataClassification::Pii`] / [`DataClassification::Phi`] /
    /// [`DataClassification::Pci`] → `true` (access controls beyond
    /// freely-distributable apply). A [`Classification::gate_compute`]
    /// baseline (which uses [`DataClassification::default = Internal`]
    /// via `#[default]`) answers `true` — the substrate's default-arm
    /// short-circuit propagates through the scalar closed-set field's
    /// own [`Default`] impl to this predicate's answer, distinct from
    /// [`Self::data_is_regulated`]'s `false` baseline (which projects
    /// the SAME `Internal` default through the antisymmetric arm of
    /// the closed set's predicate pair). This baseline-flip is the
    /// FIRST direct-scalar corner peer where the gate-compute baseline
    /// answers `true`, not `false`.
    ///
    /// The closed-set-internal pin
    /// `data_classification_regulated_implies_restricted` seals the
    /// implication `is_regulated() ⇒ is_restricted()` on every
    /// variant, so `data_is_regulated()` returning `true` implies THIS
    /// predicate also returns `true`; the reverse does not hold
    /// (`Internal | Confidential` are restricted but not regulated).
    /// This is the FIRST substrate-primitive pair on the (parent ×
    /// derived-nullary-bool) corner whose two predicates carry a non-
    /// trivial closed-set-internal implication relationship — a
    /// future compliance-baseline auto-selector can rely on
    /// `data_is_regulated() ⇒ data_is_restricted()` by construction
    /// rather than restating the implication at every callsite.
    ///
    /// A future seventh [`DataClassification`] variant (a hypothetical
    /// `TradeSecret` bucket for competitive-sensitive data, or an
    /// `Anonymized` bucket for pseudonymized-PII whose access posture
    /// differs from raw PII) reaches this probe through ONE
    /// `is_restricted` arm on the closed set with the probe body
    /// untouched — the nullary-predicate shape defers every per-
    /// variant policy decision to the closed set's own truth table
    /// ([`DataClassification::is_restricted`]) rather than duplicating
    /// the discriminator sweep here.
    ///
    /// # Compounding
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `data-restricted` on `POINT_FIXED_TAG_ARMS` —
    /// byte-for-byte peer of the sibling `terminating-horizon`,
    /// `metric-axes-required`, `coordination-required`, and
    /// `data-regulated` fixed tags on the (parent × derived-nullary-
    /// bool) corner. The ephemeral surface publishes the same tag via
    /// [`crate::ephemeral::EphemeralSpec::data_is_restricted`], which
    /// composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (data-restricted)` audit answers the same question
    /// on both surfaces.
    ///
    /// Future compliance-baseline auto-selectors dispatching on the
    /// `(is_regulated, is_restricted)` two-axis projection
    /// (documented on [`DataClassification::is_regulated`] itself)
    /// read THIS predicate rather than re-deriving from the variant
    /// name at each callsite — the classification-data-axis lattice-
    /// typed image of the restricted-data question lives at ONE
    /// substrate site and every compliance-mode chooser downstream
    /// binds through the SAME `data_is_restricted()` shape.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `data-restricted` fixed tag in `tatara-check`, future
    /// compliance-baseline / access-control-mandatory validators,
    /// future variant additions on [`DataClassification`]) binds
    /// through the SAME `data_is_restricted()` shape rather than
    /// restating the
    /// `classification.data_classification.is_restricted()` chain at
    /// each callsite. THEORY.md §VI.1 — generation over composition;
    /// a future [`DataClassification`] variant lands at ONE `ALL`
    /// entry + ONE `is_restricted` arm on the closed set and this
    /// probe picks it up mechanically.
    #[must_use]
    pub fn data_is_restricted(&self) -> bool {
        self.data_classification.is_restricted()
    }

    /// Derived-boolean predicate — does this [`Classification`]'s
    /// [`ConvergencePointType`] project to `true` under
    /// [`ConvergencePointType::is_endomorphic`]? The ONE substrate
    /// primitive that owns the `(Classification) -> bool` derived-
    /// nullary-predicate walk shape on the `point_type` slot for the
    /// 1→1 topology-bucket question.
    ///
    /// # Sixth occupant on the (parent × derived-nullary-bool) corner
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`],
    /// [`Self::data_is_regulated`], and [`Self::data_is_restricted`]
    /// on the workspace-wide (parent × derived-nullary-bool) corner of
    /// the closed-set-driven presence-probe algebra — the SIXTH
    /// occupant on the corner and the FIRST peer threading the
    /// classification-`point_type` axis rather than the horizon,
    /// calm, or data axes. Direct-scalar peer of
    /// [`Self::calm_requires_coordination`] /
    /// [`Self::data_is_regulated`] / [`Self::data_is_restricted`]:
    /// walks a DIRECT scalar closed-set field's derived projection on
    /// the [`Classification`] parent (no nested-struct hop like the
    /// two `horizon_*` peers), but distinct from all three by ONE
    /// structural degree — [`ConvergencePointType`] has NO
    /// [`Default`] impl, so the derived-nullary answer here does NOT
    /// carry a substrate default-arm short-circuit through the
    /// parent's `#[default]` chain. The [`Self::gate_compute`]
    /// baseline still fixes an answer (`Gate.is_endomorphic() =
    /// false`), pinned by
    /// `classification_gate_compute_point_is_endomorphic_is_false`,
    /// but that answer is chosen deliberately by the baseline's
    /// `point_type: Gate` field rather than reached through a
    /// closed-set-side `#[default]`. Populates the corner as a
    /// proven-repeatable primitive shape across FOUR distinct
    /// classification-axis closed sets ([`HorizonKind`],
    /// [`CalmClassification`], [`DataClassification`],
    /// [`ConvergencePointType`]) rather than a three-axis curiosity.
    ///
    /// # Semantics — derived nullary boolean, not variant equality
    ///
    /// `point_is_endomorphic()` returns `true` iff
    /// `self.point_type.is_endomorphic()`. The eight-variant
    /// [`ConvergencePointType`] closed set publishes the truth table
    /// (via the shape-preserving-topology (1,1) arity partition):
    /// [`ConvergencePointType::Transform`] /
    /// [`ConvergencePointType::Observe`] → `true` (1→1 shape);
    /// [`ConvergencePointType::Fork`] /
    /// [`ConvergencePointType::Broadcast`] → `false` (1→N diffusive);
    /// [`ConvergencePointType::Join`] / [`ConvergencePointType::Gate`]
    /// / [`ConvergencePointType::Select`] /
    /// [`ConvergencePointType::Reduce`] → `false` (N→1 convergent).
    /// A [`Classification::gate_compute`] baseline (which uses
    /// [`ConvergencePointType::Gate`] deliberately as the baseline
    /// convergent barrier point) answers `false` — this is NOT a
    /// [`Default`]-arm short-circuit (unlike the four earlier
    /// direct-scalar / nested-struct corner peers), because
    /// [`ConvergencePointType`] has no `impl Default`; the baseline
    /// is a chosen field value, not a defaulted one.
    ///
    /// A future ninth [`ConvergencePointType`] variant lands at ONE
    /// `ALL` entry + ONE `is_endomorphic` arm on the closed set with
    /// the probe body untouched — the nullary-predicate shape defers
    /// every per-variant policy decision to the closed set's own
    /// truth table ([`ConvergencePointType::is_endomorphic`]) rather
    /// than duplicating the discriminator sweep here.
    ///
    /// # Compounding
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `endomorphic-point` on
    /// `POINT_FIXED_TAG_ARMS` — byte-for-byte peer of the sibling
    /// `terminating-horizon` / `metric-axes-required` /
    /// `coordination-required` / `data-regulated` / `data-restricted`
    /// fixed tags on the (parent × derived-nullary-bool) corner. The
    /// ephemeral surface publishes the same tag via
    /// [`crate::ephemeral::EphemeralSpec::point_is_endomorphic`],
    /// which composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (endomorphic-point)` audit answers the same
    /// question on both surfaces. Sibling projections
    /// [`ConvergencePointType::is_diffusive`] and
    /// [`ConvergencePointType::is_convergent`] compose byte-
    /// identically as future seventh + eighth corner occupants; when
    /// all three land the three-way partition contract
    /// `is_endomorphic ⊕ is_diffusive ⊕ is_convergent` sealed on the
    /// closed set by `convergence_point_type_buckets_cover_every_variant`
    /// composes through the parent-composed layer as a substrate-
    /// wide theorem exactly as the closed-set XOR pair
    /// `terminates ^ requires_metric_axes` composes through this
    /// corner today.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `endomorphic-point` fixed tag in `tatara-check`, future DAG
    /// composition / edge-cardinality validators, future variant
    /// additions on [`ConvergencePointType`]) binds through the SAME
    /// `point_is_endomorphic()` shape rather than restating the
    /// `classification.point_type.is_endomorphic()` chain at each
    /// callsite. THEORY.md §VI.1 — generation over composition; a
    /// future [`ConvergencePointType`] variant lands at ONE `ALL`
    /// entry + ONE `is_endomorphic` arm on the closed set and this
    /// probe picks it up mechanically.
    #[must_use]
    pub fn point_is_endomorphic(&self) -> bool {
        self.point_type.is_endomorphic()
    }

    /// Derived-boolean predicate — does this [`Classification`]'s
    /// [`ConvergencePointType`] project to `true` under
    /// [`ConvergencePointType::is_diffusive`]? The ONE substrate
    /// primitive that owns the `(Classification) -> bool` derived-
    /// nullary-predicate walk shape on the `point_type` slot for the
    /// 1→N fan-out topology-bucket question.
    ///
    /// # Seventh occupant on the (parent × derived-nullary-bool) corner
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`],
    /// [`Self::data_is_regulated`], [`Self::data_is_restricted`], and
    /// [`Self::point_is_endomorphic`] on the workspace-wide (parent ×
    /// derived-nullary-bool) corner of the closed-set-driven presence-
    /// probe algebra — the SEVENTH occupant on the corner and the
    /// SECOND peer threading the classification-`point_type` axis,
    /// pinning that axis as a proven-repeatable structural sub-corner
    /// across TWO sibling projections rather than a one-off. Direct-
    /// scalar peer of [`Self::point_is_endomorphic`]: the two share
    /// the SAME parent slot (`self.point_type`), the SAME closed-set
    /// carrier ([`ConvergencePointType`]), and the SAME chosen-field
    /// baseline discipline ([`ConvergencePointType`] has no
    /// [`Default`] impl, so the derived-nullary answer here does NOT
    /// carry a substrate default-arm short-circuit through the
    /// parent's `#[default]` chain — [`Self::gate_compute`] fixes
    /// `point_type: Gate` deliberately, and `Gate.is_diffusive() =
    /// false`).
    ///
    /// # Semantics — derived nullary boolean, disjoint from endomorphic
    ///
    /// `point_is_diffusive()` returns `true` iff
    /// `self.point_type.is_diffusive()`. The eight-variant
    /// [`ConvergencePointType`] closed set publishes the truth table
    /// (via the (One, Many) arity cell): [`ConvergencePointType::Fork`]
    /// / [`ConvergencePointType::Broadcast`] → `true` (1→N fan-out);
    /// every other variant → `false` (endomorphic or convergent).
    /// A [`Classification::gate_compute`] baseline answers `false`
    /// deliberately (Gate is a convergent barrier, not a diffusive
    /// fan-out).
    ///
    /// A future ninth [`ConvergencePointType`] variant lands at ONE
    /// `ALL` entry + ONE `is_diffusive` arm on the closed set with the
    /// probe body untouched — the nullary-predicate shape defers every
    /// per-variant policy decision to the closed set's own truth
    /// table ([`ConvergencePointType::is_diffusive`]) rather than
    /// duplicating the discriminator sweep here.
    ///
    /// # Compounding — first corner-peer mutex on the `point_type` axis
    ///
    /// This is the FIRST corner-peer pair on the `point_type` axis
    /// (with [`Self::point_is_endomorphic`]) whose two projections
    /// carry a non-trivial closed-set-internal MUTEX relationship
    /// (`point_is_endomorphic ⇒ ¬point_is_diffusive` — no variant
    /// lands in both buckets, sealed on the closed set by
    /// `convergence_point_type_buckets_cover_every_variant`). Distinct
    /// from the FIRST corner-peer implication pair on the `data`
    /// axis (`data_is_regulated ⇒ data_is_restricted`) by the
    /// implication direction — regulated-⇒-restricted has one bucket
    /// contained in the other, while endomorphic-vs-diffusive has
    /// two disjoint buckets partitioning a common universe. When the
    /// third sibling [`Self::point_is_convergent`] lands, the mutex
    /// closes into the three-way XOR partition contract
    /// `point_is_endomorphic ⊕ point_is_diffusive ⊕
    /// point_is_convergent` sealed on the closed set by
    /// `convergence_point_type_buckets_cover_every_variant` — a
    /// substrate-wide theorem that composes through this corner
    /// exactly as the closed-set XOR pair `terminates ^
    /// requires_metric_axes` composes today.
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `diffusive-point` on `POINT_FIXED_TAG_ARMS` —
    /// byte-for-byte peer of the sibling `endomorphic-point` fixed
    /// tag. The ephemeral surface publishes the same tag via
    /// [`crate::ephemeral::EphemeralSpec::point_is_diffusive`], which
    /// composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`] so
    /// the two-surface parity contract holds — the operator's
    /// `:requires (diffusive-point)` audit answers the same question
    /// on both surfaces.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `diffusive-point` fixed tag in `tatara-check`, future DAG
    /// composition / edge-cardinality validators, future variant
    /// additions on [`ConvergencePointType`]) binds through the SAME
    /// `point_is_diffusive()` shape rather than restating the
    /// `classification.point_type.is_diffusive()` chain at each
    /// callsite. THEORY.md §VI.1 — generation over composition; a
    /// future [`ConvergencePointType`] variant lands at ONE `ALL`
    /// entry + ONE `is_diffusive` arm on the closed set and this
    /// probe picks it up mechanically.
    #[must_use]
    pub fn point_is_diffusive(&self) -> bool {
        self.point_type.is_diffusive()
    }

    /// Derived-boolean predicate — does this [`Classification`]'s
    /// [`ConvergencePointType`] project to `true` under
    /// [`ConvergencePointType::is_convergent`]? The ONE substrate
    /// primitive that owns the `(Classification) -> bool` derived-
    /// nullary-predicate walk shape on the `point_type` slot for the
    /// N→1 fan-in topology-bucket question.
    ///
    /// # Eighth occupant on the (parent × derived-nullary-bool) corner
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`],
    /// [`Self::data_is_regulated`], [`Self::data_is_restricted`],
    /// [`Self::point_is_endomorphic`], and [`Self::point_is_diffusive`]
    /// on the workspace-wide (parent × derived-nullary-bool) corner of
    /// the closed-set-driven presence-probe algebra — the EIGHTH
    /// occupant on the corner and the THIRD peer threading the
    /// classification-`point_type` axis. Direct-scalar peer of
    /// [`Self::point_is_endomorphic`] / [`Self::point_is_diffusive`]:
    /// the three share the SAME parent slot (`self.point_type`), the
    /// SAME closed-set carrier ([`ConvergencePointType`]), and the
    /// SAME chosen-field baseline discipline
    /// ([`ConvergencePointType`] has no [`Default`] impl, so the
    /// derived-nullary answer here does NOT carry a substrate default-
    /// arm short-circuit through the parent's `#[default]` chain).
    /// Distinct from the two sibling probes on ONE structural degree
    /// — the [`Self::gate_compute`] baseline's `point_type: Gate`
    /// answer projects to `true` HERE (`Gate.is_convergent() = true`),
    /// mirror-inverted from the two siblings' `false` answers, so
    /// this is the FIRST direct-scalar corner peer whose parent-
    /// composed gate-compute baseline projects `true` through a
    /// chosen-field (rather than defaulted) answer.
    ///
    /// # Semantics — derived nullary boolean, closes the three-way carving
    ///
    /// `point_is_convergent()` returns `true` iff
    /// `self.point_type.is_convergent()`. The eight-variant
    /// [`ConvergencePointType`] closed set publishes the truth table
    /// (via the (Many, One) arity cell): [`ConvergencePointType::Join`]
    /// / [`ConvergencePointType::Gate`] /
    /// [`ConvergencePointType::Select`] / [`ConvergencePointType::Reduce`]
    /// → `true` (N→1 fan-in); every other variant → `false`
    /// (endomorphic or diffusive). A [`Classification::gate_compute`]
    /// baseline answers `true` deliberately (Gate is the canonical
    /// convergent barrier point of the workspace baseline).
    ///
    /// A future ninth [`ConvergencePointType`] variant lands at ONE
    /// `ALL` entry + ONE `is_convergent` arm on the closed set with
    /// the probe body untouched — the nullary-predicate shape defers
    /// every per-variant policy decision to the closed set's own truth
    /// table ([`ConvergencePointType::is_convergent`]) rather than
    /// duplicating the discriminator sweep here.
    ///
    /// # Compounding — closes the three-way XOR partition on the `point_type` axis
    ///
    /// This is the THIRD sibling on the `point_type` axis closing the
    /// mutex pair [`Self::point_is_endomorphic`] /
    /// [`Self::point_is_diffusive`] (which sealed
    /// `point_is_endomorphic ⇒ ¬point_is_diffusive`) into the FULL
    /// three-way XOR partition contract
    /// `point_is_endomorphic ⊕ point_is_diffusive ⊕
    /// point_is_convergent = true` for every
    /// [`ConvergencePointType`] variant. Sealed on the closed set by
    /// `convergence_point_type_buckets_cover_every_variant` (which
    /// pins each variant lands in EXACTLY ONE bucket) and now
    /// composed through the parent-composed layer as a substrate-wide
    /// theorem. THREE-way XOR is a stricter contract than the closed-
    /// set XOR pair `terminates ^ requires_metric_axes` that composes
    /// through this corner today via the two `horizon_*` peers — this
    /// axis carries a partition of THREE non-empty buckets rather
    /// than TWO, so the ternary XOR is the natural generalization
    /// composed through the corner.
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `convergent-point` on `POINT_FIXED_TAG_ARMS` —
    /// byte-for-byte peer of the sibling `endomorphic-point` /
    /// `diffusive-point` fixed tags. The ephemeral surface publishes
    /// the same tag via
    /// [`crate::ephemeral::EphemeralSpec::point_is_convergent`], which
    /// composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`] so
    /// the two-surface parity contract holds — the operator's
    /// `:requires (convergent-point)` audit answers the same question
    /// on both surfaces.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `convergent-point` fixed tag in `tatara-check`, future DAG
    /// composition / edge-cardinality validators, future variant
    /// additions on [`ConvergencePointType`]) binds through the SAME
    /// `point_is_convergent()` shape rather than restating the
    /// `classification.point_type.is_convergent()` chain at each
    /// callsite. THEORY.md §VI.1 — generation over composition; a
    /// future [`ConvergencePointType`] variant lands at ONE `ALL`
    /// entry + ONE `is_convergent` arm on the closed set and this
    /// probe picks it up mechanically.
    #[must_use]
    pub fn point_is_convergent(&self) -> bool {
        self.point_type.is_convergent()
    }

    /// Derived-boolean predicate — does this [`Classification`]'s
    /// [`SubstrateType`] project to `true` under
    /// [`SubstrateType::is_resource`]? The ONE substrate primitive
    /// that owns the `(Classification) -> bool` derived-nullary-
    /// predicate walk shape on the `substrate` slot for the
    /// resource-plane bucket question.
    ///
    /// # Ninth occupant on the (parent × derived-nullary-bool) corner
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`],
    /// [`Self::data_is_regulated`], [`Self::data_is_restricted`],
    /// [`Self::point_is_endomorphic`], [`Self::point_is_diffusive`],
    /// and [`Self::point_is_convergent`] on the workspace-wide
    /// (parent × derived-nullary-bool) corner of the closed-set-
    /// driven presence-probe algebra — the NINTH occupant on the
    /// corner and the FIRST peer threading the classification-
    /// `substrate` axis (the fourth of six classification axes,
    /// after `horizon`, `calm`, `data_classification`, and
    /// `point_type`). Direct-scalar peer of
    /// [`Self::point_is_endomorphic`] /
    /// [`Self::point_is_diffusive`] /
    /// [`Self::point_is_convergent`]: all four share the shape
    /// (direct scalar closed-set field with no [`Default`] impl on
    /// the child, so no default-arm short-circuit through the
    /// child's `#[default]` chain). Distinct from the three
    /// `point_type`-axis siblings on the parent slot walked
    /// (`self.substrate` vs `self.point_type`) and on the closed set
    /// carried ([`SubstrateType`] vs [`ConvergencePointType`]) — the
    /// [`Classification::gate_compute`] baseline's chosen field
    /// (`substrate: Compute`) projects `true` HERE
    /// (`Compute.is_resource() = true`), mirror-aligned with the
    /// [`Self::point_is_convergent`] sibling's `true`-on-baseline
    /// answer and mirror-inverted from the two other `point_type`
    /// peers.
    ///
    /// # Semantics — derived nullary boolean over the closed-set plane
    ///
    /// `substrate_is_resource()` returns `true` iff
    /// `self.substrate.is_resource()`. The eight-variant
    /// [`SubstrateType`] closed set publishes the truth table (via
    /// the plane partition):
    /// [`SubstrateType::Financial`] / [`SubstrateType::Compute`] /
    /// [`SubstrateType::Network`] / [`SubstrateType::Storage`] →
    /// `true` (resource plane — you allocate budgets from it);
    /// [`SubstrateType::Security`] / [`SubstrateType::Identity`] /
    /// [`SubstrateType::Observability`] /
    /// [`SubstrateType::Regulatory`] → `false` (policy or telemetry
    /// plane). A [`Classification::gate_compute`] baseline answers
    /// `true` because its `substrate: Compute` field is deliberately
    /// resource-plane.
    ///
    /// A future ninth [`SubstrateType`] variant lands at ONE `ALL`
    /// entry + ONE `is_resource` arm on the closed set with the
    /// probe body untouched — the nullary-predicate shape defers
    /// every per-variant policy decision to the closed set's own
    /// truth table ([`SubstrateType::is_resource`]) rather than
    /// duplicating the discriminator sweep here.
    ///
    /// # Compounding — first substrate-axis peer, opens the three-way plane partition
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `resource-substrate` on
    /// `POINT_FIXED_TAG_ARMS` — byte-for-byte structural peer of the
    /// sibling `terminating-horizon` / `metric-axes-required` /
    /// `coordination-required` / `data-regulated` / `data-restricted`
    /// / `endomorphic-point` / `diffusive-point` / `convergent-point`
    /// fixed tags on the (parent × derived-nullary-bool) corner. The
    /// ephemeral surface publishes the same tag via
    /// [`crate::ephemeral::EphemeralSpec::substrate_is_resource`],
    /// which composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (resource-substrate)` audit answers the same
    /// question on both surfaces. Sibling projections
    /// [`SubstrateType::is_policy`] and
    /// [`SubstrateType::is_telemetry`] compose byte-identically as
    /// future tenth + eleventh corner occupants; when all three land
    /// the three-way partition contract
    /// `is_resource ⊕ is_policy ⊕ is_telemetry` sealed on the closed
    /// set by `substrate_type_buckets_cover_every_variant` composes
    /// through the parent-composed layer as a substrate-wide theorem
    /// — the exact ternary lift already sealed on the sibling
    /// `point_type` axis by
    /// `classification_point_type_probes_form_three_way_xor_partition_over_all`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `resource-substrate` fixed tag in `tatara-check`, future
    /// plane-baseline / compliance-baseline selectors, future
    /// variant additions on [`SubstrateType`]) binds through the
    /// SAME `substrate_is_resource()` shape rather than restating
    /// the `classification.substrate.is_resource()` chain at each
    /// callsite. THEORY.md §VI.1 — generation over composition; a
    /// future [`SubstrateType`] variant lands at ONE `ALL` entry +
    /// ONE `is_resource` arm on the closed set and this probe picks
    /// it up mechanically.
    #[must_use]
    pub fn substrate_is_resource(&self) -> bool {
        self.substrate.is_resource()
    }

    /// Derived-boolean predicate — does this [`Classification`]'s
    /// [`SubstrateType`] project to `true` under
    /// [`SubstrateType::is_policy`]? The ONE substrate primitive
    /// that owns the `(Classification) -> bool` derived-nullary-
    /// predicate walk shape on the `substrate` slot for the
    /// policy-plane bucket question.
    ///
    /// # Tenth occupant on the (parent × derived-nullary-bool) corner
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`],
    /// [`Self::data_is_regulated`], [`Self::data_is_restricted`],
    /// [`Self::point_is_endomorphic`], [`Self::point_is_diffusive`],
    /// [`Self::point_is_convergent`], and
    /// [`Self::substrate_is_resource`] on the workspace-wide
    /// (parent × derived-nullary-bool) corner of the closed-set-
    /// driven presence-probe algebra — the TENTH occupant on the
    /// corner and the SECOND peer threading the classification-
    /// `substrate` axis, promoting that axis from a proven-repeatable
    /// one-off (`substrate_is_resource` alone) to a proven-repeatable
    /// pair. FIRST corner-peer pair on the `substrate` axis whose
    /// two projections carry a non-trivial closed-set-internal
    /// MUTEX relationship (`substrate_is_resource ⇒ ¬substrate_is_policy`
    /// — the eight-variant [`SubstrateType`] closed set carves its
    /// variants into THREE disjoint buckets sealed by
    /// `substrate_type_buckets_cover_every_variant`), structural
    /// twin of the sibling `point_type`-axis MUTEX pair sealed by
    /// `classification_point_is_endomorphic_and_point_is_diffusive_are_mutex_over_all`.
    /// Direct-scalar peer of [`Self::substrate_is_resource`] and the
    /// three sibling `point_type`-axis arms: all five share the
    /// shape (direct scalar closed-set field with no [`Default`]
    /// impl on the child, so no default-arm short-circuit through
    /// the child's `#[default]` chain). The
    /// [`Classification::gate_compute`] baseline's chosen field
    /// (`substrate: Compute`) projects `false` HERE
    /// (`Compute.is_policy() = false`), mirror-inverted from the
    /// sibling `substrate_is_resource` baseline's `true`.
    ///
    /// # Semantics — derived nullary boolean over the closed-set plane
    ///
    /// `substrate_is_policy()` returns `true` iff
    /// `self.substrate.is_policy()`. The eight-variant
    /// [`SubstrateType`] closed set publishes the truth table (via
    /// the plane partition):
    /// [`SubstrateType::Security`] / [`SubstrateType::Identity`] /
    /// [`SubstrateType::Regulatory`] → `true` (policy plane — you
    /// enforce constraints on it); [`SubstrateType::Financial`] /
    /// [`SubstrateType::Compute`] / [`SubstrateType::Network`] /
    /// [`SubstrateType::Storage`] / [`SubstrateType::Observability`]
    /// → `false` (resource or telemetry plane). A
    /// [`Classification::gate_compute`] baseline answers `false`
    /// because its `substrate: Compute` field is deliberately
    /// resource-plane, not policy-plane.
    ///
    /// A future ninth [`SubstrateType`] variant lands at ONE `ALL`
    /// entry + ONE `is_policy` arm on the closed set with the
    /// probe body untouched — the nullary-predicate shape defers
    /// every per-variant policy decision to the closed set's own
    /// truth table ([`SubstrateType::is_policy`]) rather than
    /// duplicating the discriminator sweep here.
    ///
    /// # Compounding — second substrate-axis peer, opens the substrate MUTEX pair
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `policy-substrate` on
    /// `POINT_FIXED_TAG_ARMS` — byte-for-byte structural peer of the
    /// sibling `resource-substrate` / `terminating-horizon` /
    /// `metric-axes-required` / `coordination-required` /
    /// `data-regulated` / `data-restricted` / `endomorphic-point` /
    /// `diffusive-point` / `convergent-point` fixed tags on the
    /// (parent × derived-nullary-bool) corner. The ephemeral surface
    /// publishes the same tag via
    /// [`crate::ephemeral::EphemeralSpec::substrate_is_policy`],
    /// which composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (policy-substrate)` audit answers the same
    /// question on both surfaces. Sibling projection
    /// [`SubstrateType::is_telemetry`] composes byte-identically as
    /// a future eleventh corner occupant; when it lands the
    /// three-way partition contract
    /// `is_resource ⊕ is_policy ⊕ is_telemetry` sealed on the closed
    /// set by `substrate_type_buckets_cover_every_variant` composes
    /// through the parent-composed layer as a substrate-wide theorem
    /// — the exact ternary lift already sealed on the sibling
    /// `point_type` axis by
    /// `classification_point_type_probes_form_three_way_xor_partition_over_all`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `policy-substrate` fixed tag in `tatara-check`, future
    /// plane-baseline / compliance-baseline selectors, future
    /// variant additions on [`SubstrateType`]) binds through the
    /// SAME `substrate_is_policy()` shape rather than restating
    /// the `classification.substrate.is_policy()` chain at each
    /// callsite. THEORY.md §VI.1 — generation over composition; a
    /// future [`SubstrateType`] variant lands at ONE `ALL` entry +
    /// ONE `is_policy` arm on the closed set and this probe picks
    /// it up mechanically.
    #[must_use]
    pub fn substrate_is_policy(&self) -> bool {
        self.substrate.is_policy()
    }

    /// Derived-boolean predicate — does this [`Classification`]'s
    /// [`SubstrateType`] project to `true` under
    /// [`SubstrateType::is_telemetry`]? The ONE substrate primitive
    /// that owns the `(Classification) -> bool` derived-nullary-
    /// predicate walk shape on the `substrate` slot for the
    /// telemetry-plane bucket question.
    ///
    /// # Eleventh occupant on the (parent × derived-nullary-bool) corner — CLOSES the substrate axis
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`],
    /// [`Self::data_is_regulated`], [`Self::data_is_restricted`],
    /// [`Self::point_is_endomorphic`], [`Self::point_is_diffusive`],
    /// [`Self::point_is_convergent`], [`Self::substrate_is_resource`],
    /// and [`Self::substrate_is_policy`] on the workspace-wide
    /// (parent × derived-nullary-bool) corner of the closed-set-
    /// driven presence-probe algebra — the ELEVENTH occupant on the
    /// corner and the THIRD peer threading the classification-
    /// `substrate` axis. This peer CLOSES the substrate axis on the
    /// corner into the FULL three-way XOR partition contract
    /// `substrate_is_resource ⊕ substrate_is_policy ⊕
    /// substrate_is_telemetry` — the closed-set partition already
    /// sealed on [`SubstrateType`] by
    /// `substrate_type_buckets_cover_every_variant` now composes
    /// through the parent-composed layer as a substrate-wide theorem
    /// pinned by
    /// `classification_substrate_probes_form_three_way_xor_partition_over_all`.
    /// The ternary lift on the substrate axis is the structural
    /// twin of the sibling `point_type`-axis ternary lift sealed by
    /// `classification_point_type_probes_form_three_way_xor_partition_over_all`.
    /// Direct-scalar peer of [`Self::substrate_is_resource`],
    /// [`Self::substrate_is_policy`], and the three sibling
    /// `point_type`-axis arms: all six share the shape (direct scalar
    /// closed-set field with no [`Default`] impl on the child, so no
    /// default-arm short-circuit through the child's `#[default]`
    /// chain). The [`Classification::gate_compute`] baseline's chosen
    /// field (`substrate: Compute`) projects `false` HERE
    /// (`Compute.is_telemetry() = false`), mirror-inverted from the
    /// sibling `substrate_is_resource` baseline's `true` and aligned
    /// with the sibling `substrate_is_policy` baseline's `false`.
    ///
    /// # Semantics — derived nullary boolean over the closed-set plane
    ///
    /// `substrate_is_telemetry()` returns `true` iff
    /// `self.substrate.is_telemetry()`. The eight-variant
    /// [`SubstrateType`] closed set publishes the truth table (via
    /// the plane partition): [`SubstrateType::Observability`] →
    /// `true` (telemetry plane — the singleton bucket that passively
    /// observes other workloads without carrying their payload or
    /// gating their access); every other variant → `false`
    /// (resource or policy plane). A [`Classification::gate_compute`]
    /// baseline answers `false` because its `substrate: Compute`
    /// field is deliberately resource-plane, not telemetry-plane.
    ///
    /// A future ninth [`SubstrateType`] variant lands at ONE `ALL`
    /// entry + ONE `is_telemetry` arm on the closed set with the
    /// probe body untouched — the nullary-predicate shape defers
    /// every per-variant telemetry decision to the closed set's own
    /// truth table ([`SubstrateType::is_telemetry`]) rather than
    /// duplicating the discriminator sweep here.
    ///
    /// # Compounding — CLOSES the substrate axis into a three-way XOR partition
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `telemetry-substrate` on
    /// `POINT_FIXED_TAG_ARMS` — byte-for-byte structural peer of the
    /// sibling `resource-substrate` / `policy-substrate` /
    /// `terminating-horizon` / `metric-axes-required` /
    /// `coordination-required` / `data-regulated` / `data-restricted`
    /// / `endomorphic-point` / `diffusive-point` / `convergent-point`
    /// fixed tags on the (parent × derived-nullary-bool) corner. The
    /// ephemeral surface publishes the same tag via
    /// [`crate::ephemeral::EphemeralSpec::substrate_is_telemetry`],
    /// which composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (telemetry-substrate)` audit answers the same
    /// question on both surfaces. THIRD substrate-axis peer CLOSES
    /// the three-way XOR partition contract
    /// `is_resource ⊕ is_policy ⊕ is_telemetry` sealed on the closed
    /// set by `substrate_type_buckets_cover_every_variant` through
    /// the parent-composed layer as a substrate-wide theorem — the
    /// exact ternary lift already sealed on the sibling `point_type`
    /// axis by
    /// `classification_point_type_probes_form_three_way_xor_partition_over_all`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `telemetry-substrate` fixed tag in `tatara-check`, future
    /// plane-baseline / compliance-baseline selectors, future
    /// variant additions on [`SubstrateType`]) binds through the
    /// SAME `substrate_is_telemetry()` shape rather than restating
    /// the `classification.substrate.is_telemetry()` chain at each
    /// callsite. THEORY.md §VI.1 — generation over composition; a
    /// future [`SubstrateType`] variant lands at ONE `ALL` entry +
    /// ONE `is_telemetry` arm on the closed set and this probe picks
    /// it up mechanically.
    #[must_use]
    pub fn substrate_is_telemetry(&self) -> bool {
        self.substrate.is_telemetry()
    }

    /// Derived-boolean predicate — does this [`Classification`]'s
    /// [`CalmClassification`] project to `true` under
    /// [`CalmClassification::is_monotone`]? The ONE substrate
    /// primitive that owns the `(Classification) -> bool` derived-
    /// nullary-predicate walk shape on the `calm` slot for the
    /// CALM-monotone-plane question — the positive framing peer of
    /// [`Self::calm_requires_coordination`].
    ///
    /// # Twelfth occupant on the (parent × derived-nullary-bool) corner — CLOSES the calm axis
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// and [`Self::substrate_is_telemetry`] on the workspace-wide
    /// (parent × derived-nullary-bool) corner of the closed-set-driven
    /// presence-probe algebra — the TWELFTH occupant on the corner and
    /// the SECOND peer threading the classification-`calm` axis. This
    /// peer CLOSES the calm axis on the corner into the FULL binary
    /// XOR partition contract `calm_is_monotone ⊕
    /// calm_requires_coordination` — the closed-set partition sealed
    /// on [`CalmClassification`] by
    /// `calm_classification_monotone_xor_requires_coordination` now
    /// composes through the parent-composed layer as a substrate-wide
    /// theorem pinned by
    /// `classification_calm_probes_form_binary_xor_partition_over_all`.
    /// Structural twin of the sibling horizon-axis binary XOR
    /// partition sealed on the closed set by
    /// `horizon_kind_terminate_xor_requires_metric_axes` (which
    /// composes at the closed-set layer today — the parent-composed
    /// lift lands here as the calm axis's counterpart). The calm axis
    /// becomes the THIRD classification axis (after `point_type`,
    /// `substrate`) to reach the closed XOR partition landmark on
    /// this corner, promoting the axis-closure milestone from a
    /// twin (ternary on `point_type` + `substrate`) to a triple
    /// (adding binary on `calm`). Direct-scalar peer of
    /// [`Self::calm_requires_coordination`]: both walk the same
    /// scalar `calm` slot on the parent — TWO layers of `Default`
    /// short-circuit reaching the derived-nullary predicate
    /// ([`Classification::gate_compute`] → [`CalmClassification::default`]).
    /// The [`Classification::gate_compute`] baseline's default-arm
    /// answer projects `true` HERE (Monotone default →
    /// `is_monotone() = true`), mirror-inverted from
    /// [`Self::calm_requires_coordination`]'s Monotone-default `false`.
    ///
    /// # Semantics — derived nullary boolean over the closed-set plane
    ///
    /// `calm_is_monotone()` returns `true` iff
    /// `self.calm.is_monotone()`. The two-variant
    /// [`CalmClassification`] closed set publishes the truth table:
    /// [`CalmClassification::Monotone`] → `true` (CALM ⇒ can be
    /// distributed without coordination); [`CalmClassification::NonMonotone`]
    /// → `false` (CALM ⇒ requires coordination). A
    /// [`Classification::gate_compute`] baseline answers `true`
    /// because its `calm: CalmClassification::default() = Monotone`
    /// field defaults via [`CalmClassification`]'s `#[default]`, so
    /// every unadorned Process reads as gossip-eligible (safe under
    /// the CALM theorem: monotone operations distribute without
    /// coordination).
    ///
    /// A future third [`CalmClassification`] variant (a hypothetical
    /// `ConditionallyMonotone` sentinel for CRDT joins under a fixed
    /// schema) lands at ONE `ALL` entry + ONE `is_monotone` arm on
    /// the closed set with the probe body untouched — the nullary-
    /// predicate shape defers every per-variant monotonicity decision
    /// to the closed set's own truth table
    /// ([`CalmClassification::is_monotone`]) rather than duplicating
    /// the discriminator sweep here.
    ///
    /// # Compounding — CLOSES the calm axis into a binary XOR partition
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `monotone-calm` on `POINT_FIXED_TAG_ARMS` —
    /// byte-for-byte structural peer of the sibling
    /// `coordination-required` fixed tag (the antisymmetric partner
    /// on the same axis) and of every other `(parent × derived-
    /// nullary-bool)` corner arm. The ephemeral surface publishes the
    /// same tag via
    /// [`crate::ephemeral::EphemeralSpec::calm_is_monotone`], which
    /// composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (monotone-calm)` audit answers the same question on
    /// both surfaces. SECOND calm-axis peer CLOSES the binary XOR
    /// partition contract `is_monotone ⊕ requires_coordination`
    /// sealed on the closed set by
    /// `calm_classification_monotone_xor_requires_coordination`
    /// through the parent-composed layer as a substrate-wide theorem
    /// — the exact binary lift already sealed on the sibling
    /// `horizon` axis at the closed-set layer by
    /// `horizon_kind_terminate_xor_requires_metric_axes`, now with
    /// the parent-composed layer's own XOR partition test on the
    /// calm axis.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `monotone-calm` fixed tag in `tatara-check`, future scheduler
    /// / coordination-mode validators reading the positive CALM
    /// framing, future variant additions on [`CalmClassification`])
    /// binds through the SAME `calm_is_monotone()` shape rather than
    /// restating either `!self.calm_requires_coordination()` or
    /// `self.calm.is_monotone()` at the callsite. THEORY.md §VI.1 —
    /// generation over composition; a future [`CalmClassification`]
    /// variant lands at ONE `ALL` entry + ONE `is_monotone` arm on
    /// the closed set and this probe picks it up mechanically.
    #[must_use]
    pub fn calm_is_monotone(&self) -> bool {
        self.calm.is_monotone()
    }

    /// Derived-boolean predicate — does this [`Classification`]'s
    /// [`DataClassification`] project to `true` under
    /// [`DataClassification::is_public`]? The ONE substrate primitive
    /// that owns the `(Classification) -> bool` derived-nullary-
    /// predicate walk shape on the `data_classification` slot for the
    /// freely-distributable-data question — the positive framing peer
    /// of [`Self::data_is_restricted`].
    ///
    /// # Thirteenth occupant on the (parent × derived-nullary-bool) corner — CLOSES the data axis
    ///
    /// Peer of [`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// [`Self::substrate_is_telemetry`], and [`Self::calm_is_monotone`]
    /// on the workspace-wide (parent × derived-nullary-bool) corner of
    /// the closed-set-driven presence-probe algebra — the THIRTEENTH
    /// occupant on the corner and the THIRD peer threading the
    /// classification-`data_classification` axis. This peer CLOSES
    /// the data axis on the corner into the FULL binary XOR partition
    /// contract `data_is_public ⊕ data_is_restricted` — the closed-set
    /// partition sealed on [`DataClassification`] by
    /// `data_classification_public_xor_restricted` now composes
    /// through the parent-composed layer as a substrate-wide theorem
    /// pinned by
    /// `classification_data_probes_form_binary_xor_partition_over_all`.
    /// Structural twin of the calm-axis binary XOR partition sealed at
    /// the parent-composed layer by
    /// `classification_calm_probes_form_binary_xor_partition_over_all`
    /// on the sibling `calm` axis — both axes carve into a `is_X /
    /// requires_X` (positive/negative-framing) complementary bucket
    /// pair whose union covers every closed-set variant. The data axis
    /// becomes the FOURTH classification axis (after `point_type`,
    /// `substrate`, `calm`) to reach the closed XOR partition landmark
    /// on this corner, promoting the axis-closure milestone from a
    /// proven-repeatable triple (ternary on `point_type` + `substrate`
    /// plus binary on `calm`) to a proven-repeatable quadruple (adding
    /// a SECOND binary on `data_classification`). Direct-scalar peer
    /// of [`Self::data_is_regulated`] and [`Self::data_is_restricted`]:
    /// all three walk the same scalar `data_classification` slot on the
    /// parent — TWO layers of `Default` short-circuit reaching the
    /// derived-nullary predicate ([`Classification::gate_compute`] →
    /// [`DataClassification::default`]). The
    /// [`Classification::gate_compute`] baseline's default-arm answer
    /// projects `false` HERE (Internal default →
    /// `is_public() = false`), mirror-inverted from
    /// [`Self::data_is_restricted`]'s Internal-default `true` on the
    /// SAME defaulted `data_classification` slot.
    ///
    /// # Semantics — derived nullary boolean over the closed-set plane
    ///
    /// `data_is_public()` returns `true` iff
    /// `self.data_classification.is_public()`. The six-variant
    /// [`DataClassification`] closed set publishes the truth table:
    /// [`DataClassification::Public`] → `true` (freely distributable —
    /// no access control required); [`DataClassification::Internal`] /
    /// [`DataClassification::Confidential`] / [`DataClassification::Pii`]
    /// / [`DataClassification::Phi`] / [`DataClassification::Pci`] →
    /// `false` (some access-control regime applies). A
    /// [`Classification::gate_compute`] baseline answers `false`
    /// because its `data_classification:
    /// DataClassification::default() = Internal` field defaults via
    /// [`DataClassification`]'s `#[default]`, so every unadorned
    /// Process reads as access-controlled (safe under the compliance
    /// baseline: an operator must deliberately opt into public
    /// distribution).
    ///
    /// The closed-set-internal pin
    /// `data_classification_regulated_implies_not_public` seals the
    /// implication `is_regulated() ⇒ ¬is_public()` on every variant, so
    /// [`Self::data_is_regulated`] returning `true` implies THIS
    /// predicate returns `false`; this is the ANTISYMMETRIC pair to
    /// the sibling `data_classification_regulated_implies_restricted`
    /// on the same closed set, and the FIRST substrate-primitive pair
    /// on the (parent × derived-nullary-bool) corner whose two
    /// predicates project ONE closed set into the pair of
    /// complementary buckets whose union is a full binary XOR
    /// partition AND whose intersection is empty on every variant.
    ///
    /// A future seventh [`DataClassification`] variant (a hypothetical
    /// `TradeSecret` bucket for competitive-sensitive data, or an
    /// `Anonymized` bucket for pseudonymized-PII whose regulatory
    /// posture differs from raw PII) reaches this probe through ONE
    /// `is_public` arm on the closed set with the probe body untouched
    /// — the nullary-predicate shape defers every per-variant policy
    /// decision to the closed set's own truth table
    /// ([`DataClassification::is_public`]) rather than duplicating the
    /// discriminator sweep here.
    ///
    /// # Compounding — CLOSES the data axis into a binary XOR partition
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `public-data` on `POINT_FIXED_TAG_ARMS` —
    /// byte-for-byte structural peer of the sibling `data-restricted`
    /// fixed tag (the antisymmetric partner on the same axis) and of
    /// every other `(parent × derived-nullary-bool)` corner arm. The
    /// ephemeral surface publishes the same tag via
    /// [`crate::ephemeral::EphemeralSpec::data_is_public`], which
    /// composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (public-data)` audit answers the same question on
    /// both surfaces. THIRD data-axis peer CLOSES the binary XOR
    /// partition contract `is_public ⊕ is_restricted` sealed on the
    /// closed set by `data_classification_public_xor_restricted`
    /// through the parent-composed layer as a substrate-wide theorem
    /// — mirror of the calm axis's parent-composed binary XOR closure
    /// `classification_calm_probes_form_binary_xor_partition_over_all`.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `public-data` fixed tag in `tatara-check`, future compliance-
    /// baseline / audit-log-optional validators reading the positive
    /// distribution framing, future variant additions on
    /// [`DataClassification`]) binds through the SAME
    /// `data_is_public()` shape rather than restating either
    /// `!self.data_is_restricted()` or `self.data_classification.is_public()`
    /// at the callsite. THEORY.md §VI.1 — generation over composition;
    /// a future [`DataClassification`] variant lands at ONE `ALL`
    /// entry + ONE `is_public` arm on the closed set and this probe
    /// picks it up mechanically.
    #[must_use]
    pub fn data_is_public(&self) -> bool {
        self.data_classification.is_public()
    }

    /// Derived-boolean predicate — does this [`Classification`]'s
    /// [`Horizon::direction`] slot (defaulted through
    /// [`OptimizationDirection::default = Minimize`] on absence) project
    /// to `true` under [`OptimizationDirection::prefers_lower`]? The ONE
    /// substrate primitive that owns the `(Classification) -> bool`
    /// derived-nullary-predicate walk on the `horizon.direction` slot
    /// for the lower-is-better optimization-polarity question.
    ///
    /// # Fourteenth occupant on the (parent × derived-nullary-bool) corner — first via the optimization-direction axis
    ///
    /// Peer of the thirteen prior nullary-bool substrate primitives on
    /// [`Classification`] ([`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// [`Self::substrate_is_telemetry`], [`Self::calm_is_monotone`],
    /// [`Self::data_is_public`]) on the workspace-wide (parent ×
    /// derived-nullary-bool) corner of the closed-set-driven presence-
    /// probe algebra. FIRST occupant threading the classification-
    /// `horizon.direction` axis — opens the SIXTH classification axis
    /// into the fixed-tag algebra after the horizon, calm, data, point,
    /// and substrate axes; distinct from every prior corner peer on ONE
    /// structural degree: the source carrier is `Option<OptimizationDirection>`
    /// nested inside the [`Horizon`] struct rather than a direct scalar
    /// or a nested direct-scalar. Byte-for-byte peer of the sibling
    /// [`Self::has_optimization_direction`] on the Option-carrier hop
    /// (both unwrap the [`Horizon::direction`] slot through
    /// [`Option::unwrap_or_default`] against the closed-set-level
    /// [`OptimizationDirection::default = Minimize`]); this method is
    /// the derived-nullary-bool projection over the same defaulted
    /// scalar, mirroring the (has-variant, is-projection) pair on the
    /// sibling `point_type` axis.
    ///
    /// # Semantics — derived nullary boolean, defaulted through Option
    ///
    /// `direction_prefers_lower()` returns `true` iff
    /// `self.horizon.direction.unwrap_or_default().prefers_lower()`. The
    /// two-variant [`OptimizationDirection`] closed set publishes the
    /// truth table: [`OptimizationDirection::Minimize`] → `true` (cost /
    /// latency / error rate — lower is better);
    /// [`OptimizationDirection::Maximize`] → `false` (throughput /
    /// coverage / revenue — higher is better). A
    /// [`Classification::gate_compute`] baseline (which carries
    /// `horizon: Horizon::default()` whose `direction` field is `None`)
    /// answers `true` deliberately — an unadorned Process's polarity
    /// reads as lower-is-better, matching the substrate
    /// [`OptimizationDirection::default = Minimize`] chosen precisely so
    /// an under-specified `Asymptotic` horizon can't silently flip the
    /// rate-window evaluator's polarity onto the Maximize path (a
    /// future `Maximize`-default-via-rename would silently invert every
    /// existing alert that treats decreasing rate as healthy).
    ///
    /// A future third [`OptimizationDirection`] variant (a hypothetical
    /// `Stabilize` sentinel for "drive toward a target value", which
    /// neither minimization nor maximization names) reaches this probe
    /// through ONE `prefers_lower` arm on the closed set with the probe
    /// body untouched — the nullary-predicate shape defers every
    /// per-variant policy decision to the closed set's own truth table
    /// ([`OptimizationDirection::prefers_lower`]) rather than
    /// duplicating the discriminator sweep here.
    ///
    /// # Compounding — opens the optimization-direction axis on the corner
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `prefers-lower-direction` on `POINT_FIXED_TAG_ARMS`
    /// — the SIXTH classification axis to reach the fixed-tag corner.
    /// The ephemeral surface publishes the same tag via
    /// [`crate::ephemeral::EphemeralSpec::direction_prefers_lower`],
    /// which composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`] so
    /// the two-surface parity contract holds — the operator's
    /// `:requires (prefers-lower-direction)` audit answers the same
    /// question on both surfaces. A future antisymmetric peer
    /// (`direction_prefers_higher` reading
    /// `!self.horizon.direction.unwrap_or_default().prefers_lower()`,
    /// or a projection through a peer `OptimizationDirection::prefers_higher`
    /// closed-set arm) closes the binary XOR partition on this axis —
    /// mirror of the calm-axis (`monotone-calm ⊕ coordination-required`)
    /// and data-axis (`public-data ⊕ data-restricted`) closures — as
    /// the SECOND occupant on the axis.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body lives
    /// at ONE substrate site so every downstream (the
    /// `prefers-lower-direction` fixed tag in `tatara-check`, future
    /// asymptotic-health rate-window / regression-detector evaluators
    /// keying on the optimization-polarity, future variant additions on
    /// [`OptimizationDirection`]) binds through the SAME
    /// `direction_prefers_lower()` shape rather than restating the
    /// `classification.horizon.direction.unwrap_or_default().prefers_lower()`
    /// chain at each callsite. THEORY.md §VI.1 — generation over
    /// composition; a future [`OptimizationDirection`] variant lands at
    /// ONE `ALL` entry + ONE `prefers_lower` arm on the closed set and
    /// this probe picks it up mechanically.
    #[must_use]
    pub fn direction_prefers_lower(&self) -> bool {
        self.horizon.direction.unwrap_or_default().prefers_lower()
    }

    /// POSITIVE-FRAMING PEER of [`Self::direction_prefers_lower`] —
    /// does this [`Classification`]'s [`Horizon::direction`] slot
    /// (defaulted through [`OptimizationDirection::default = Minimize`]
    /// on absence) project to `true` under
    /// [`OptimizationDirection::prefers_higher`]? The ONE substrate
    /// primitive that owns the `(Classification) -> bool` derived-
    /// nullary-predicate walk on the `horizon.direction` slot for the
    /// higher-is-better optimization-polarity question.
    ///
    /// # Fifteenth occupant on the (parent × derived-nullary-bool) corner — CLOSES the optimization-direction axis
    ///
    /// Peer of the fourteen prior nullary-bool substrate primitives on
    /// [`Classification`] ([`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// [`Self::substrate_is_telemetry`], [`Self::calm_is_monotone`],
    /// [`Self::data_is_public`], [`Self::direction_prefers_lower`]) on
    /// the workspace-wide (parent × derived-nullary-bool) corner of
    /// the closed-set-driven presence-probe algebra. FIFTEENTH
    /// occupant on the corner and SECOND peer threading the
    /// classification-`horizon.direction` axis — CLOSES the axis into
    /// the FULL binary XOR partition contract `direction_prefers_lower
    /// ⊕ direction_prefers_higher` sealed on the closed set by
    /// `optimization_direction_prefers_lower_xor_prefers_higher` and
    /// composed through the parent-composed layer by
    /// `classification_direction_probes_form_binary_xor_partition_over_all`.
    /// ALL SIX classification axes (horizon, calm, data, point,
    /// substrate, optimization-direction) now have their partitions
    /// closed at the corner — the fixed-tag algebra reaches full
    /// axis-coverage on the classification lattice.
    ///
    /// Direct byte-for-byte structural peer of
    /// [`Self::direction_prefers_lower`]: both walk the SAME
    /// [`Horizon::direction`] slot through TWO layers of `Default`
    /// (`Horizon::default` → `direction: None`; then
    /// [`OptimizationDirection::default = Minimize`]) to reach the
    /// closed-set-level projection. The [`Classification::gate_compute`]
    /// baseline's default-arm answer projects `false` HERE (Minimize
    /// default → `prefers_higher() = false`), mirror-inverted from
    /// [`Self::direction_prefers_lower`]'s Minimize-default `true` on
    /// the SAME defaulted `horizon.direction` slot — the antisymmetric
    /// twin on the substrate polarity default.
    ///
    /// # Semantics — derived nullary boolean over the closed-set plane
    ///
    /// `direction_prefers_higher()` returns `true` iff
    /// `self.horizon.direction.unwrap_or_default().prefers_higher()`.
    /// The two-variant [`OptimizationDirection`] closed set publishes
    /// the truth table: [`OptimizationDirection::Minimize`] → `false`
    /// (cost / latency / error rate — decreasing values improve);
    /// [`OptimizationDirection::Maximize`] → `true` (throughput /
    /// coverage / revenue — increasing values improve). A
    /// [`Classification::gate_compute`] baseline (which carries
    /// `horizon: Horizon::default()` whose `direction` field is `None`)
    /// answers `false` because [`OptimizationDirection::default =
    /// Minimize`] projects `prefers_higher = false`, so every
    /// unadorned Process reads under the lower-is-better polarity —
    /// matching the substrate polarity default (safe under the
    /// asymptotic-health rate-window evaluator's convention: an
    /// operator must deliberately opt into Maximize polarity rather
    /// than the substrate silently flipping every unadorned Process
    /// onto the higher-is-better path).
    ///
    /// A future third [`OptimizationDirection`] variant (a hypothetical
    /// `Stabilize` sentinel for "drive toward a target value", which
    /// neither minimization nor maximization names) reaches this probe
    /// through ONE `prefers_higher` arm on the closed set with the
    /// probe body untouched — the nullary-predicate shape defers every
    /// per-variant policy decision to the closed set's own truth table
    /// ([`OptimizationDirection::prefers_higher`]) rather than
    /// duplicating the discriminator sweep here. The XOR pin on the
    /// closed set forces such a variant to answer `false` on BOTH
    /// `prefers_lower` AND `prefers_higher` unless a deliberate
    /// extension carves the closed set into a ternary partition.
    ///
    /// # Compounding — CLOSES the optimization-direction axis into a binary XOR partition
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `prefers-higher-direction` on `POINT_FIXED_TAG_ARMS`
    /// — byte-for-byte structural peer of the sibling
    /// `prefers-lower-direction` fixed tag (the antisymmetric partner
    /// on the same axis) and of every other `(parent × derived-
    /// nullary-bool)` corner arm. The ephemeral surface publishes the
    /// same tag via
    /// [`crate::ephemeral::EphemeralSpec::direction_prefers_higher`],
    /// which composes THIS method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`] so
    /// the two-surface parity contract holds — the operator's
    /// `:requires (prefers-higher-direction)` audit answers the same
    /// question on both surfaces. SECOND optimization-direction-axis
    /// peer CLOSES the binary XOR partition contract
    /// `direction_prefers_lower ⊕ direction_prefers_higher` sealed on
    /// the closed set by
    /// `optimization_direction_prefers_lower_xor_prefers_higher`
    /// through the parent-composed layer as a substrate-wide theorem
    /// — mirror of the calm-axis (`monotone-calm ⊕ coordination-required`)
    /// and data-axis (`public-data ⊕ data-restricted`) closures
    /// already landed on the corner, and the SIXTH (and final)
    /// classification axis to reach the closed XOR partition landmark
    /// at this corner.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the
    /// `prefers-higher-direction` fixed tag in `tatara-check`, future
    /// asymptotic-health rate-window / regression-detector evaluators
    /// keying on the positive higher-is-better polarity framing,
    /// future variant additions on [`OptimizationDirection`]) binds
    /// through the SAME `direction_prefers_higher()` shape rather
    /// than restating either `!self.direction_prefers_lower()` or
    /// `self.horizon.direction.unwrap_or_default().prefers_higher()`
    /// at the callsite. THEORY.md §VI.1 — generation over composition;
    /// a future [`OptimizationDirection`] variant lands at ONE `ALL`
    /// entry + ONE `prefers_higher` arm on the closed set and this
    /// probe picks it up mechanically.
    #[must_use]
    pub fn direction_prefers_higher(&self) -> bool {
        self.horizon.direction.unwrap_or_default().prefers_higher()
    }

    /// Derived-boolean predicate — does this [`Classification`]'s
    /// `point_type` slot project to `Arity::One` under
    /// [`ConvergencePointType::input_arity`]? The ONE substrate
    /// primitive that owns the `(Classification) -> bool` derived-
    /// nullary-predicate walk on the DAG-composition input-arity
    /// projection.
    ///
    /// # First derived-nullary-bool corner occupant on the input-arity axis
    ///
    /// Peer of the fifteen prior nullary-bool substrate primitives on
    /// [`Classification`] ([`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// [`Self::substrate_is_telemetry`], [`Self::calm_is_monotone`],
    /// [`Self::data_is_public`], [`Self::direction_prefers_lower`],
    /// [`Self::direction_prefers_higher`]) on the workspace-wide
    /// (parent × derived-nullary-bool) corner of the closed-set-driven
    /// presence-probe algebra. SIXTEENTH occupant on the corner and
    /// FIRST occupant threading the classification-`point_type`-
    /// derived input-arity axis — opens the SEVENTH classification
    /// axis into the fixed-tag algebra after the six axes (horizon,
    /// calm, data, point-type, substrate, optimization-direction)
    /// already closed at the corner. The input-arity axis is a
    /// derived typed projection through
    /// [`ConvergencePointType::input_arity`] rather than a stored
    /// classification slot — so this predicate composes an extra
    /// closed-set-level projection hop compared to the sibling
    /// `point_is_*` triple that walks the raw `point_type` slot.
    ///
    /// # Semantics — derived nullary boolean over the input-arity projection
    ///
    /// `input_arity_is_one()` returns `true` iff
    /// `self.point_type.input_arity().is_one()`. The eight-variant
    /// [`ConvergencePointType`] closed set publishes the truth table
    /// through [`ConvergencePointType::input_arity`]: `Transform |
    /// Fork | Broadcast | Observe → One → true`; `Join | Gate |
    /// Select | Reduce → Many → false`. A
    /// [`Classification::gate_compute`] baseline (which carries
    /// `point_type: Gate`) answers `false` — `Gate.input_arity() =
    /// Many`, so the multi-input bucket carves the workspace-wide
    /// baseline into the multi-input cell.
    ///
    /// A future [`ConvergencePointType`] variant (a hypothetical
    /// `Demux` for `One → Many` or `Mux` for `Many → One`) reaches
    /// this probe through ONE `input_arity` arm on
    /// [`ConvergencePointType`] with the probe body untouched — the
    /// many-to-one projection means the bucket membership shift lands
    /// exactly at [`ConvergencePointType::input_arity`], not at every
    /// consumer that previously restated the bucket in code.
    ///
    /// # Compounding — opens the input-arity axis at the parent-composed corner
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` composes this primitive
    /// as a fixed tag `single-input-arity` on `POINT_FIXED_TAG_ARMS`
    /// — byte-for-byte structural peer of every other `(parent ×
    /// derived-nullary-bool)` corner arm. The antisymmetric partner
    /// [`Self::input_arity_is_many`] closes the input-arity axis into
    /// the FULL binary XOR partition contract sealed on the closed
    /// set by `arity_is_one_xor_is_many_over_all` — mirror of the
    /// binary XOR closures on the calm axis (`monotone-calm ⊕
    /// coordination-required`), the data axis (`public-data ⊕
    /// data-restricted`), and the optimization-direction axis
    /// (`prefers-lower-direction ⊕ prefers-higher-direction`). A
    /// future ephemeral-surface peer
    /// (`EphemeralSpec::input_arity_is_one`) will compose THIS method
    /// through [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (single-input-arity)` audit answers the same
    /// question on both surfaces.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the future
    /// `single-input-arity` fixed tag in `tatara-check`, DAG
    /// composition validators keying on the single-input framing,
    /// future variant additions on [`ConvergencePointType`]) binds
    /// through the SAME `input_arity_is_one()` shape rather than
    /// restating the two-hop `self.point_type.input_arity().is_one()`
    /// chain at each callsite. THEORY.md §VI.1 — generation over
    /// composition; a future [`ConvergencePointType`] variant lands
    /// at ONE `ALL` entry + ONE `input_arity` arm on the closed set
    /// and this probe picks it up mechanically.
    #[must_use]
    pub fn input_arity_is_one(&self) -> bool {
        self.point_type.input_arity().is_one()
    }

    /// ANTISYMMETRIC PEER of [`Self::input_arity_is_one`] — does
    /// this [`Classification`]'s `point_type` slot project to
    /// `Arity::Many` under [`ConvergencePointType::input_arity`]?
    /// The ONE substrate primitive that owns the `(Classification) ->
    /// bool` derived-nullary-predicate walk on the multi-input side
    /// of the DAG-composition input-arity projection.
    ///
    /// # Seventeenth corner occupant — CLOSES the input-arity axis into a binary XOR partition
    ///
    /// SEVENTEENTH occupant on the (parent × derived-nullary-bool)
    /// corner of the workspace-wide closed-set-driven presence-probe
    /// algebra and SECOND peer threading the classification-
    /// `point_type`-derived input-arity axis — CLOSES the SEVENTH
    /// classification axis into the FULL binary XOR partition
    /// contract `input_arity_is_one ⊕ input_arity_is_many` sealed on
    /// the closed set by `arity_is_one_xor_is_many_over_all` and
    /// composed through the parent-composed layer by
    /// `classification_input_arity_probes_form_binary_xor_partition_over_all`.
    /// Structural mirror of the calm-axis binary XOR partition
    /// (`monotone-calm ⊕ coordination-required`), the data-axis
    /// binary XOR partition (`public-data ⊕ data-restricted`), and
    /// the optimization-direction-axis binary XOR partition
    /// (`prefers-lower-direction ⊕ prefers-higher-direction`) — the
    /// FOURTH parent-composed binary XOR partition on the corner.
    ///
    /// # Semantics — derived nullary boolean over the multi-input projection
    ///
    /// `input_arity_is_many()` returns `true` iff
    /// `self.point_type.input_arity().is_many()`. The eight-variant
    /// [`ConvergencePointType`] closed set publishes the truth table
    /// through [`ConvergencePointType::input_arity`]: `Transform |
    /// Fork | Broadcast | Observe → One → false`; `Join | Gate |
    /// Select | Reduce → Many → true`. A
    /// [`Classification::gate_compute`] baseline (which carries
    /// `point_type: Gate`) answers `true` — `Gate.input_arity() =
    /// Many`, so the multi-input bucket carves the workspace-wide
    /// baseline. Direct antisymmetric mirror of
    /// [`Self::input_arity_is_one`] on the SAME projection through
    /// the SAME closed set.
    ///
    /// # Compounding — CLOSES the input-arity axis into a binary XOR partition
    ///
    /// The point-domain require-tag surface will compose this
    /// primitive as a fixed tag `multi-input-arity` on
    /// `POINT_FIXED_TAG_ARMS` — antisymmetric peer of the sibling
    /// `single-input-arity` fixed tag. Together with the sibling
    /// [`Self::input_arity_is_one`] the two predicates seal the
    /// input-arity axis into a binary XOR partition on the parent-
    /// composed layer as a substrate-wide theorem, closing the
    /// axis at the SEVENTH-classification-axis landmark.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the future
    /// `multi-input-arity` fixed tag, DAG composition validators
    /// keying on multi-input fan-in semantics, future variant
    /// additions on [`ConvergencePointType`]) binds through the SAME
    /// `input_arity_is_many()` shape rather than restating either
    /// `!self.input_arity_is_one()` or
    /// `self.point_type.input_arity().is_many()` at each callsite.
    /// THEORY.md §VI.1 — generation over composition; a future
    /// [`ConvergencePointType`] variant lands at ONE `ALL` entry +
    /// ONE `input_arity` arm on the closed set and this probe picks
    /// it up mechanically.
    #[must_use]
    pub fn input_arity_is_many(&self) -> bool {
        self.point_type.input_arity().is_many()
    }

    /// Derived-boolean predicate — does this [`Classification`]'s
    /// `point_type` slot project to `Arity::One` under
    /// [`ConvergencePointType::output_arity`]? The ONE substrate
    /// primitive that owns the `(Classification) -> bool` derived-
    /// nullary-predicate walk on the DAG-composition OUTPUT-arity
    /// projection — antisymmetric partner (on the DAG-composition
    /// arity PAIR) of the sibling [`Self::input_arity_is_one`] that
    /// walks the SAME `point_type` slot through the SAME `Arity`
    /// closed set but composes a DIFFERENT typed projection
    /// ([`ConvergencePointType::output_arity`] rather than
    /// [`ConvergencePointType::input_arity`]).
    ///
    /// # Eighteenth (parent × derived-nullary-bool) corner occupant — opens the EIGHTH classification axis
    ///
    /// Peer of the seventeen prior nullary-bool substrate primitives
    /// on [`Classification`] ([`Self::horizon_terminates`],
    /// [`Self::horizon_requires_metric_axes`],
    /// [`Self::calm_requires_coordination`], [`Self::data_is_regulated`],
    /// [`Self::data_is_restricted`], [`Self::point_is_endomorphic`],
    /// [`Self::point_is_diffusive`], [`Self::point_is_convergent`],
    /// [`Self::substrate_is_resource`], [`Self::substrate_is_policy`],
    /// [`Self::substrate_is_telemetry`], [`Self::calm_is_monotone`],
    /// [`Self::data_is_public`], [`Self::direction_prefers_lower`],
    /// [`Self::direction_prefers_higher`],
    /// [`Self::input_arity_is_one`], [`Self::input_arity_is_many`]) on
    /// the workspace-wide (parent × derived-nullary-bool) corner of
    /// the closed-set-driven presence-probe algebra. EIGHTEENTH
    /// occupant on the corner and FIRST occupant threading the
    /// classification-`point_type`-derived output-arity axis — opens
    /// the EIGHTH classification axis into the fixed-tag algebra
    /// after the seven axes (horizon, calm, data, point-type,
    /// substrate, optimization-direction, input-arity) already opened
    /// at the corner. The output-arity axis is the SECOND derived
    /// typed projection ([`ConvergencePointType::output_arity`],
    /// after the input-arity axis's [`ConvergencePointType::input_arity`])
    /// rather than a stored classification slot — so this predicate
    /// composes an extra closed-set-level projection hop compared to
    /// the sibling `point_is_*` triple that walks the raw `point_type`
    /// slot.
    ///
    /// # Distinctness from the input-arity axis
    ///
    /// The input-arity and output-arity axes carve the eight-variant
    /// [`ConvergencePointType`] closed set into DISTINCT partitions —
    /// six of the eight variants (`Fork | Broadcast | Join | Gate |
    /// Select | Reduce`) DISAGREE between the two projections, and
    /// only the two endomorphic variants (`Transform | Observe` —
    /// both `(One, One)`) agree. So `output_arity_is_one` is NOT a
    /// redundant restatement of `input_arity_is_one`; the two together
    /// name the `(input_arity, output_arity)` typed pair contract
    /// canonically already carried on [`ConvergencePointType`] by the
    /// `is_endomorphic | is_diffusive | is_convergent` triple — but
    /// as SEPARATE nullary predicates on the parent-composed layer
    /// rather than as a bucket dispatcher.
    ///
    /// # Semantics — derived nullary boolean over the output-arity projection
    ///
    /// `output_arity_is_one()` returns `true` iff
    /// `self.point_type.output_arity().is_one()`. The eight-variant
    /// [`ConvergencePointType`] closed set publishes the truth table
    /// through [`ConvergencePointType::output_arity`]: `Transform |
    /// Join | Gate | Select | Reduce | Observe → One → true`; `Fork |
    /// Broadcast → Many → false`. A [`Classification::gate_compute`]
    /// baseline (which carries `point_type: Gate`) answers `true` —
    /// `Gate.output_arity() = One`, so the single-output bucket
    /// carves the workspace-wide baseline into the single-output
    /// cell. Note the workspace-baseline answer FLIPS between the
    /// input-arity and output-arity axes on the exact same baseline:
    /// `input_arity_is_one` is `false` on `gate_compute`, but
    /// `output_arity_is_one` is `true` — direct evidence that the two
    /// axes carve the closed set into structurally different
    /// partitions.
    ///
    /// A future [`ConvergencePointType`] variant (a hypothetical
    /// `Demux` for `One → Many` or `Mux` for `Many → One`) reaches
    /// this probe through ONE `output_arity` arm on
    /// [`ConvergencePointType`] with the probe body untouched — the
    /// many-to-one projection means the bucket membership shift lands
    /// exactly at [`ConvergencePointType::output_arity`], not at
    /// every consumer that previously restated the bucket in code.
    ///
    /// # Compounding — opens the output-arity axis at the parent-composed corner
    ///
    /// The point-domain require-tag surface in
    /// `tatara-reconciler::bin::tatara-check` will compose this
    /// primitive as a fixed tag `single-output-arity` on
    /// `POINT_FIXED_TAG_ARMS` — byte-for-byte structural peer of
    /// every other `(parent × derived-nullary-bool)` corner arm. The
    /// antisymmetric partner [`Self::output_arity_is_many`] closes
    /// the output-arity axis into the FULL binary XOR partition
    /// contract sealed on the closed set by
    /// `arity_is_one_xor_is_many_over_all` — mirror of the binary
    /// XOR closures on the calm axis (`monotone-calm ⊕
    /// coordination-required`), the data axis (`public-data ⊕
    /// data-restricted`), the optimization-direction axis
    /// (`prefers-lower-direction ⊕ prefers-higher-direction`), and
    /// the input-arity axis (`input_arity_is_one ⊕
    /// input_arity_is_many`). A future ephemeral-surface peer
    /// (`EphemeralSpec::output_arity_is_one`) will compose THIS
    /// method through
    /// [`crate::ephemeral::EphemeralSpec::resolved_classification`]
    /// so the two-surface parity contract holds — the operator's
    /// `:requires (single-output-arity)` audit answers the same
    /// question on both surfaces.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the future
    /// `single-output-arity` fixed tag in `tatara-check`, DAG
    /// composition validators keying on the single-output framing,
    /// future variant additions on [`ConvergencePointType`]) binds
    /// through the SAME `output_arity_is_one()` shape rather than
    /// restating the two-hop `self.point_type.output_arity().is_one()`
    /// chain at each callsite. THEORY.md §VI.1 — generation over
    /// composition; a future [`ConvergencePointType`] variant lands
    /// at ONE `ALL` entry + ONE `output_arity` arm on the closed set
    /// and this probe picks it up mechanically.
    #[must_use]
    pub fn output_arity_is_one(&self) -> bool {
        self.point_type.output_arity().is_one()
    }

    /// ANTISYMMETRIC PEER of [`Self::output_arity_is_one`] — does
    /// this [`Classification`]'s `point_type` slot project to
    /// `Arity::Many` under [`ConvergencePointType::output_arity`]?
    /// The ONE substrate primitive that owns the `(Classification) ->
    /// bool` derived-nullary-predicate walk on the multi-output side
    /// of the DAG-composition output-arity projection.
    ///
    /// # Nineteenth corner occupant — CLOSES the output-arity axis into a binary XOR partition
    ///
    /// NINETEENTH occupant on the (parent × derived-nullary-bool)
    /// corner of the workspace-wide closed-set-driven presence-probe
    /// algebra and SECOND peer threading the classification-
    /// `point_type`-derived output-arity axis — CLOSES the EIGHTH
    /// classification axis into the FULL binary XOR partition
    /// contract `output_arity_is_one ⊕ output_arity_is_many` sealed
    /// on the closed set by `arity_is_one_xor_is_many_over_all` and
    /// composed through the parent-composed layer by
    /// `classification_output_arity_probes_form_binary_xor_partition_over_all`.
    /// Structural mirror of the input-arity-axis binary XOR partition
    /// (`input_arity_is_one ⊕ input_arity_is_many`), the calm-axis
    /// binary XOR partition (`monotone-calm ⊕ coordination-required`),
    /// the data-axis binary XOR partition
    /// (`public-data ⊕ data-restricted`), and the optimization-
    /// direction-axis binary XOR partition (`prefers-lower-direction
    /// ⊕ prefers-higher-direction`) — the FIFTH parent-composed
    /// binary XOR partition on the corner and the SECOND on the
    /// derived-typed-projection stratum (after the input-arity
    /// closure).
    ///
    /// # Semantics — derived nullary boolean over the multi-output projection
    ///
    /// `output_arity_is_many()` returns `true` iff
    /// `self.point_type.output_arity().is_many()`. The eight-variant
    /// [`ConvergencePointType`] closed set publishes the truth table
    /// through [`ConvergencePointType::output_arity`]: `Transform |
    /// Join | Gate | Select | Reduce | Observe → One → false`; `Fork
    /// | Broadcast → Many → true`. A [`Classification::gate_compute`]
    /// baseline (which carries `point_type: Gate`) answers `false` —
    /// `Gate.output_arity() = One`, so the single-output bucket
    /// carves the workspace-wide baseline. Direct antisymmetric
    /// mirror of [`Self::output_arity_is_one`] on the SAME projection
    /// through the SAME closed set.
    ///
    /// # Compounding — CLOSES the output-arity axis into a binary XOR partition
    ///
    /// The point-domain require-tag surface will compose this
    /// primitive as a fixed tag `multi-output-arity` on
    /// `POINT_FIXED_TAG_ARMS` — antisymmetric peer of the sibling
    /// `single-output-arity` fixed tag. Together with the sibling
    /// [`Self::output_arity_is_one`] the two predicates seal the
    /// output-arity axis into a binary XOR partition on the parent-
    /// composed layer as a substrate-wide theorem, closing the axis
    /// at the EIGHTH-classification-axis landmark. Together with the
    /// four sibling closed binary XOR partitions (input-arity, calm,
    /// data, optimization-direction) the (parent × derived-nullary-
    /// bool) corner now carries FIVE closed binary XOR partitions —
    /// the derived-typed-projection stratum grows the corner from
    /// stored-slot walks into DAG-composition typed projections
    /// systematically.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 — composition
    /// preserves proofs; the derived-nullary-bool predicate body
    /// lives at ONE substrate site so every downstream (the future
    /// `multi-output-arity` fixed tag, DAG composition validators
    /// keying on multi-output fan-out semantics, future variant
    /// additions on [`ConvergencePointType`]) binds through the SAME
    /// `output_arity_is_many()` shape rather than restating either
    /// `!self.output_arity_is_one()` or
    /// `self.point_type.output_arity().is_many()` at each callsite.
    /// THEORY.md §VI.1 — generation over composition; a future
    /// [`ConvergencePointType`] variant lands at ONE `ALL` entry +
    /// ONE `output_arity` arm on the closed set and this probe picks
    /// it up mechanically.
    #[must_use]
    pub fn output_arity_is_many(&self) -> bool {
        self.point_type.output_arity().is_many()
    }

    /// Compose the workspace-baseline [`Self::gate_compute`] with a
    /// single-axis mutation — return `Self::gate_compute()` with the
    /// axis slot carrying `axis`'s classification-axis type overwritten
    /// by `axis`. The ONE substrate primitive that owns the
    /// (Classification, single-axis variant) → Classification
    /// baseline-with-axis-mutated composition shape.
    ///
    /// # Substrate ergonomics
    ///
    /// Pre-lift the shape `Classification::gate_compute()` with a
    /// single axis slot overwritten by a per-test swept variant
    /// recurred at ≥ 40 hand-authored test-fixture callsites past the
    /// ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold, each restating the
    /// SAME six-line struct-literal that names FOUR baseline slots
    /// verbatim and mutates ONE. Post-lift every callsite reads
    /// `Classification::gate_compute_with_axis(populated)` — one line,
    /// ONE substrate primitive owns the four-baseline-slot restatement,
    /// and a future workspace-wide baseline shift lands at ONE site
    /// via [`Self::gate_compute`] rather than at every downstream
    /// test fixture that names the four unmutated slots explicitly.
    ///
    /// # Compounding
    ///
    /// A future SIXTH classification axis (foreshadowed by the
    /// six-axis lattice language on the CRD-facing prose) lands as
    /// ONE peer `impl ClassificationAxis` on the new axis's closed
    /// set + ONE new slot on [`Classification`] itself — every test
    /// fixture using `gate_compute_with_axis` picks up the sixth axis
    /// mechanically without touching the callsite. A future audit
    /// dispatcher walking every classification axis (the "walk every
    /// classification axis through its XOR partition landmark" shape
    /// the FIFTH-axis-closure commit `2c74fab` explicitly named as
    /// the next-lift target) binds through the SAME trait rather than
    /// a five-arm dispatch on axis identity.
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the four-baseline-slot restatement is the
    /// composition proof `gate_compute` already carries at ONE site;
    /// this primitive extends the ONE-site composition guarantee
    /// through the per-axis mutation shape). THEORY.md §VI.1
    /// (generation over composition — a future sixth axis lands as
    /// ONE ClassificationAxis impl and every test fixture picks it
    /// up mechanically).
    #[must_use]
    pub fn gate_compute_with_axis<A: ClassificationAxis>(axis: A) -> Self {
        let mut c = Self::gate_compute();
        axis.overlay(&mut c);
        c
    }

    /// Fluent per-axis overlay — post-composes ONE additional
    /// [`ClassificationAxis`] variant on top of `self`, returning
    /// the mutated [`Classification`] by value. Sibling to
    /// [`Self::gate_compute_with_axis`] on the (baseline-composer ×
    /// per-axis-overlay) axis: `gate_compute_with_axis(a)` is the
    /// (start-from-baseline, overlay-one-axis) shape; `with_axis(a)`
    /// is the (start-from-arbitrary-classification, overlay-one-more-
    /// axis) shape. Together they compose the workspace-wide
    /// (Classification, N-axis-conjunction) construction algebra:
    /// `Classification::gate_compute_with_axis(a).with_axis(b).with_axis(c)`
    /// chains an arbitrary N-axis conjunction onto the [`Self::gate_compute`]
    /// baseline through ONE substrate primitive per axis rather than
    /// restating the FIVE-field struct-literal (`point_type`,
    /// `substrate`, `horizon`, `calm`, `data_classification`) verbatim
    /// at every N-axis-conjunction test-fixture callsite.
    ///
    /// # Substrate ergonomics
    ///
    /// Pre-lift the shape `Classification { <mutated-axes>,
    /// ..(remaining-baselines) }` recurred at ≥ 5 hand-authored
    /// multi-axis-conjunction test-fixture callsites in this file
    /// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold,
    /// each restating the FIVE-field struct-literal with distinct
    /// (Fork+Storage), (Fork+Storage+NonMonotone),
    /// (Fork+Storage+NonMonotone+Pii),
    /// (Fork+Storage+NonMonotone+Pii+HorizonKind::Asymptotic), and
    /// (Fork+Storage+NonMonotone+Pii+HorizonKind::Asymptotic+
    /// direction=Maximize) conjunctions. Post-lift each callsite
    /// reads
    /// `Classification::gate_compute_with_axis(Fork).with_axis(Storage)`
    /// … (chained per axis), and the four/three/two/one-baseline-slot
    /// restatement binds through the ONE substrate composer at every
    /// callsite. The prior lift onto [`Self::gate_compute_with_axis`]
    /// (`76d469c` + `08714f6` + `7f14656`) closed the SINGLE-axis-
    /// overlay shape; this primitive extends the same trait dispatch
    /// through arbitrary N-axis conjunctions without introducing a
    /// variadic-tuple-overlay dispatch path.
    ///
    /// # Compounding
    ///
    /// A future SIXTH classification axis (foreshadowed by the
    /// six-axis lattice language on the CRD-facing prose) lands as
    /// ONE peer `impl ClassificationAxis` on the new axis's closed
    /// set — every multi-axis-conjunction test fixture using
    /// `.with_axis(...)` picks up the sixth axis mechanically by
    /// appending ONE more `.with_axis(new_variant)` call rather than
    /// growing an N-field struct-literal to N+1 fields at every
    /// site. A future audit dispatcher walking a fixed N-axis
    /// conjunction on every classification axis binds through the
    /// SAME chained-overlay shape rather than a per-N-arity
    /// composer family (`gate_compute_with_axes2`,
    /// `gate_compute_with_axes3`, …).
    ///
    /// Theory anchor: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the per-axis overlay is the axis-local
    /// composition proof [`ClassificationAxis::overlay`] owns at ONE
    /// site; this primitive lifts the ONE-axis composition guarantee
    /// through arbitrary chaining without a per-arity dispatch
    /// path). THEORY.md §VI.1 (generation over composition — a
    /// future N-axis-conjunction test lands as ONE chained
    /// `.with_axis(...)` sequence versus a fresh N+1-field struct-
    /// literal per callsite).
    #[must_use]
    pub fn with_axis<A: ClassificationAxis>(mut self, axis: A) -> Self {
        axis.overlay(&mut self);
        self
    }
}

/// Test/audit helper — a closed-set variant that can overlay its
/// classification-axis slot onto a base [`Classification`]. Unifies
/// the five per-axis assignments (`horizon = Horizon { kind: self, ..
/// default() }`, `calm = self`, `data_classification = self`,
/// `point_type = self`, `substrate = self`) under ONE substrate
/// shape so [`Classification::gate_compute_with_axis`] can compose
/// the workspace baseline with a single-axis mutation generically
/// over any of the five classification axes.
///
/// # Compounding
///
/// A future SIXTH classification axis lands as ONE peer `impl
/// ClassificationAxis` on the new axis's closed set — every
/// downstream test fixture and audit dispatcher that binds through
/// [`Classification::gate_compute_with_axis`] picks up the sixth
/// axis mechanically without a five-arm-becomes-six-arm dispatch
/// edit. A future workspace-wide "walk every classification axis"
/// audit primitive binds through the SAME trait rather than
/// restating the five-per-axis assignment shape at its own body.
///
/// Theory anchor: THEORY.md §II.1 invariant 5 — composition
/// preserves proofs. The five per-axis assignments each carry an
/// axis-local composition proof (nested-struct `horizon.kind` gets
/// a fresh `Horizon::default()` around the mutation; direct-scalar
/// axes get a bare `self` assignment); this trait promotes the
/// five proofs to ONE shape so a downstream composer binds through
/// the same primitive regardless of which axis it targets.
pub trait ClassificationAxis {
    /// Overlay this axis-variant onto `c`, replacing the corresponding
    /// classification-axis slot with `self`. Leaves every other slot
    /// on `c` untouched.
    fn overlay(self, c: &mut Classification);
}

impl ClassificationAxis for HorizonKind {
    fn overlay(self, c: &mut Classification) {
        // Sub-slot overlay — set ONLY the `kind` field on the nested
        // `Horizon` struct, preserving any `direction` / `metric` /
        // `healthy_rate_threshold` a prior [`Classification::with_axis`]
        // overlay may have populated. Byte-symmetric with the previous
        // whole-`Horizon` reset shape (`c.horizon = Horizon { kind: self,
        // ..Horizon::default() }`) when the base carrier is
        // [`Classification::gate_compute`] (whose `horizon` is
        // `Horizon::default()` — every sub-slot already `None`), but
        // order-independent under chaining: a downstream
        // `.with_axis(OptimizationDirection::Maximize).with_axis(HorizonKind::Asymptotic)`
        // no longer stomps the prior `direction: Some(Maximize)` overlay.
        c.horizon.kind = self;
    }
}

impl ClassificationAxis for OptimizationDirection {
    fn overlay(self, c: &mut Classification) {
        // Nested-struct-Option sub-slot overlay — set ONLY the
        // `direction` field on the nested `Horizon` struct as
        // `Some(self)`, preserving `kind` / `metric` /
        // `healthy_rate_threshold`. Peer to the direct-nested-scalar
        // [`ClassificationAxis for HorizonKind`] overlay: both hop into
        // the nested `Horizon` struct, but this overlay wraps its assign
        // in `Some(...)` per the `Horizon::direction: Option<OptimizationDirection>`
        // typed slot. Chain
        // `.with_axis(HorizonKind::Asymptotic).with_axis(OptimizationDirection::Maximize)`
        // to compose the (kind, direction) pair the
        // [`crate::export`]-facing rate-window Asymptotic-horizon
        // fixtures otherwise restate as `Horizon { kind: Asymptotic,
        // direction: Some(Maximize), ..Horizon::default() }` inline.
        c.horizon.direction = Some(self);
    }
}

impl ClassificationAxis for CalmClassification {
    fn overlay(self, c: &mut Classification) {
        c.calm = self;
    }
}

impl ClassificationAxis for DataClassification {
    fn overlay(self, c: &mut Classification) {
        c.data_classification = self;
    }
}

impl ClassificationAxis for ConvergencePointType {
    fn overlay(self, c: &mut Classification) {
        c.point_type = self;
    }
}

impl ClassificationAxis for SubstrateType {
    fn overlay(self, c: &mut Classification) {
        c.substrate = self;
    }
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
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
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

// `impl FromStr for ConvergencePointType` +
// `impl tatara_lisp::ClosedSet for ConvergencePointType` +
// `impl std::fmt::Display for ConvergencePointType` +
// `pub struct UnknownConvergencePointType(pub String)` are all generated
// by `#[derive(tatara_closed_set::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown, display)]` on the
// enum declaration above. `label` delegates to the inherent
// `ConvergencePointType::as_str` — the inherent name (PascalCase
// `as_str`) stays the load-bearing wire-vocabulary projection that
// matches the serde `rename_all = "PascalCase"` output AND the CRD
// `enum:` enumeration the Process schema stamps on
// `spec.classification.pointType` verbatim, while generic
// `T: ClosedSet` consumers reach the STABLE workspace-wide name
// (`label`). The `display` flag emits the
// `f.write_str(self.as_str())` delegation block at the same
// proc-macro site rather than a hand-rolled `fmt::Display` block per
// implementor. The auto-derived carrier label "convergence point
// type" matches the prior hand-rolled `#[error("unknown convergence
// point type: {0}")]` annotation byte-for-byte. Symmetric to the
// other five classification-axis closed-sets in this file
// (`SubstrateType` / `HorizonKind` / `OptimizationDirection` /
// `CalmClassification` / `DataClassification`) AND every other
// `#[derive(DeriveClosedSet)]` implementor across the workspace
// (`crate::pool::{ReplacementPolicy,MemberState,PoolPhase,ReturnPolicy}`,
// `crate::export::{ArtifactKind,ReportFormat,ChannelKind,ExportTrigger}`,
// `crate::allocation::{RequestorKind,AllocationPhase}`).

/// Edge cardinality of a [`ConvergencePointType`]'s input or output.
///
/// Typed projection used by [`ConvergencePointType::input_arity`] and
/// [`ConvergencePointType::output_arity`] so DAG composition validators
/// reach for a closed-set enum rather than re-deriving the in/out
/// cardinality from variant names. `Many` is the "≥1, could be N"
/// cardinality — it carries no upper bound because the convergence
/// point's variant tag is already the structural identity; the
/// number itself is a runtime property of the DAG, not the typescape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_closed_set::DeriveClosedSet)]
#[closed_set(via = "as_str", display, generate_unknown)]
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

    /// POSITIVE-FRAMING PEER of [`Self::is_one`] — is this the
    /// multi-edge cardinality? Closed-set match (not `matches!`) so a
    /// future variant triggers the compiler's exhaustiveness check at
    /// this site rather than silently defaulting to `false` (which
    /// would mis-bucket a `Zero`-style sink variant onto the multi-
    /// edge path). The boolean partition is the antisymmetric image
    /// of [`Self::is_one`]: `One ⇒ false`, `Many ⇒ true`. Exactly
    /// one of `(is_one, is_many)` is true per variant on the current
    /// two-variant closed set — pinned by
    /// `arity_is_one_xor_is_many_over_all` — exactly the binary XOR
    /// partition already sealed on the sibling optimization-direction
    /// axis by `optimization_direction_prefers_lower_xor_prefers_higher`,
    /// on the calm axis by
    /// `calm_classification_monotone_xor_requires_coordination`, and
    /// on the data axis by
    /// `data_classification_public_xor_restricted`. Structural mirror
    /// of [`OptimizationDirection::prefers_higher`] as the positive-
    /// framing peer that any future dispatch on the multi-edge
    /// cardinality (DAG fan-out validators, edge-cardinality checks:
    /// "every diffusive topology point emits fan-out") reads once
    /// rather than re-deriving from the variant name or the
    /// `!is_one()` inversion at each callsite.
    ///
    /// A future third variant (a hypothetical `Zero` sentinel for
    /// pure sinks with no edge, which neither single nor multi
    /// cardinality names) MUST answer `false` here — matching the
    /// antisymmetric complement on [`Self::is_one`] so the binary
    /// XOR partition either extends into a ternary partition
    /// deliberately (adding a third derived-nullary predicate on the
    /// closed set) OR the author flips one of the existing predicates
    /// to reclaim the XOR. The exhaustiveness check plus the XOR pin
    /// force the decision at the closed set rather than silently
    /// bucketing the new variant onto an existing cardinality.
    pub const fn is_many(self) -> bool {
        match self {
            Self::One => false,
            Self::Many => true,
        }
    }
}

// `impl fmt::Display for Arity` + `impl std::str::FromStr for Arity` +
// `impl tatara_lisp::ClosedSet for Arity` + `pub struct UnknownArity(pub
// String)` are all generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", display, generate_unknown)]` on the enum
// declaration above. The inherent `as_str` projection stays load-bearing
// — the canonical `"One" | "Many"` string every DAG composition
// validator reads; `via = "as_str"` binds `ClosedSet::label` to the same
// projection so the substrate-wide `assert_display_matches_label` /
// `assert_closed_set_well_formed` primitives dispatch through the same
// byte-identical shape every other closed-set implementor across the
// crate publishes. Aligns `Arity` with the substrate-wide
// `#[derive(DeriveClosedSet)]` idiom that every other closed-set enum on
// this classification axis (`ConvergencePointType`, `SubstrateType`,
// `HorizonKind`, `OptimizationDirection`, `CalmClassification`,
// `DataClassification`) already carries — the last hand-rolled
// `impl fmt::Display` on the axis is closed at ONE substrate site.

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
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
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

// `impl FromStr for SubstrateType` +
// `impl tatara_lisp::ClosedSet for SubstrateType` +
// `impl std::fmt::Display for SubstrateType` +
// `pub struct UnknownSubstrateType(pub String)` are all generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown, display)]` on the
// enum declaration above. The auto-derived carrier label "substrate
// type" matches the prior hand-rolled `#[error("unknown substrate
// type: {0}")]` annotation byte-for-byte. See the retrofit comment
// block on [`ConvergencePointType`] for the canonical narrative.

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
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
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

// `impl FromStr for HorizonKind` +
// `impl tatara_lisp::ClosedSet for HorizonKind` +
// `impl std::fmt::Display for HorizonKind` +
// `pub struct UnknownHorizonKind(pub String)` are all generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown, display)]` on the
// enum declaration above. The auto-derived carrier label "horizon
// kind" matches the prior hand-rolled `#[error("unknown horizon
// kind: {0}")]` annotation byte-for-byte. See the retrofit comment
// block on [`ConvergencePointType`] for the canonical narrative.

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
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
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

    /// POSITIVE-FRAMING PEER of [`Self::prefers_lower`] — does this
    /// direction prefer numerically higher values? Closed-set match
    /// (not `matches!`) so a future variant triggers the compiler's
    /// exhaustiveness check at this site rather than silently
    /// defaulting to `false` (which would mis-bucket a `Stabilize`-
    /// style variant onto the minimization path). The boolean
    /// partition is the antisymmetric image of [`Self::prefers_lower`]:
    /// `Minimize ⇒ false`, `Maximize ⇒ true`. Exactly one of
    /// `(prefers_lower, prefers_higher)` is true per variant on the
    /// current two-variant closed set — pinned by
    /// `optimization_direction_prefers_lower_xor_prefers_higher` —
    /// exactly the binary XOR partition already sealed on the sibling
    /// calm axis by `calm_classification_monotone_xor_requires_coordination`
    /// and on the sibling data axis by `data_classification_public_xor_restricted`.
    /// Structural mirror of [`CalmClassification::is_monotone`] as the
    /// positive-framing peer that any future dispatch on the higher-
    /// is-better polarity (throughput / coverage / revenue rate-window
    /// evaluator, breathe-band regression detector's positive sign)
    /// reads once rather than re-deriving from either the variant name
    /// or the `!prefers_lower()` inversion at each callsite.
    ///
    /// A future third variant (a hypothetical `Stabilize` sentinel for
    /// "drive toward a target value", which neither minimization nor
    /// maximization names) MUST answer `false` here — matching the
    /// antisymmetric complement on [`Self::prefers_lower`] so the
    /// binary XOR partition either extends into a ternary partition
    /// deliberately (adding a third derived-nullary predicate on the
    /// closed set) OR the author flips one of the existing predicates
    /// to reclaim the XOR. The exhaustiveness check plus the XOR pin
    /// force the decision at the closed set rather than silently
    /// bucketing the new variant onto an existing polarity.
    pub const fn prefers_higher(self) -> bool {
        match self {
            Self::Minimize => false,
            Self::Maximize => true,
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

// `impl FromStr for OptimizationDirection` +
// `impl tatara_lisp::ClosedSet for OptimizationDirection` +
// `impl std::fmt::Display for OptimizationDirection` +
// `pub struct UnknownOptimizationDirection(pub String)` are all
// generated by `#[derive(tatara_closed_set::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown, display)]` on the
// enum declaration above. The auto-derived carrier label
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
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
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

    /// CALM-THEOREM POSITIVE FRAMING: is this classification monotone
    /// — i.e. can it be distributed WITHOUT coordination per the
    /// biconditional half of Hellerstein's CALM theorem
    /// (Consistency As Logical Monotonicity)? Closed-set match (not
    /// `matches!`) so a future variant triggers the compiler's
    /// exhaustiveness check at this site rather than silently
    /// defaulting to `false` (which would silently mark a genuinely
    /// monotone operation as coordination-required and pay the Raft
    /// tax indefinitely) or `true` (which would silently ship a non-
    /// monotone operation onto the no-coordination path). The typed
    /// image of the theorem's LOAD-BEARING half: `Monotone ⇒ true`
    /// and `NonMonotone ⇒ false` is the antisymmetric partner of
    /// [`Self::requires_coordination`] — exactly one of
    /// `(is_monotone, requires_coordination)` is true per variant —
    /// pinned by `calm_classification_monotone_xor_requires_coordination`.
    /// Mirror of [`HorizonKind::terminates`] /
    /// [`HorizonKind::requires_metric_axes`] on the horizon axis:
    /// both closed sets are binary and both publish their two
    /// derived-nullary-bool projections at ONE site each so the axis
    /// carves into complementary buckets by construction. The
    /// positive framing is the substrate primitive Hellerstein
    /// himself names ("Consistency As Logical Monotonicity"); a
    /// future consumer asking "can this Process participate in
    /// gossip-only writes?" reads [`Self::is_monotone`] rather than
    /// re-deriving via `!requires_coordination()` at the callsite.
    pub const fn is_monotone(self) -> bool {
        match self {
            Self::Monotone => true,
            Self::NonMonotone => false,
        }
    }
}

// `impl FromStr for CalmClassification` +
// `impl tatara_lisp::ClosedSet for CalmClassification` +
// `impl std::fmt::Display for CalmClassification` +
// `pub struct UnknownCalmClassification(pub String)` are all generated
// by `#[derive(tatara_closed_set::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown, display)]` on the
// enum declaration above. The auto-derived carrier label
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
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
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

    /// POSITIVE-FRAMING PEER of [`Self::is_restricted`] — is this
    /// classification freely distributable (i.e. bearing no access-
    /// control requirement)? Closed-set match (not `matches!`) so a
    /// future variant triggers the compiler's exhaustiveness check at
    /// this site rather than silently defaulting to `false` (silently
    /// marking a genuinely-public dataset as restricted and paying the
    /// access-control tax indefinitely) or `true` (silently shipping a
    /// restricted or regulated dataset onto the freely-distributable
    /// path — a compliance-catastrophic mislabel). The typed image of
    /// the "freely distributable?" question: `Public ⇒ true` and every
    /// other variant `⇒ false` is the antisymmetric partner of
    /// [`Self::is_restricted`] — exactly one of
    /// `(is_public, is_restricted)` is true per variant — pinned by
    /// `data_classification_public_xor_restricted`. Mirror of
    /// [`CalmClassification::is_monotone`] /
    /// [`CalmClassification::requires_coordination`] on the calm axis
    /// and [`HorizonKind::terminates`] /
    /// [`HorizonKind::requires_metric_axes`] on the horizon axis: each
    /// closed set publishes its two derived-nullary-bool projections
    /// at ONE site each so the axis carves into complementary buckets
    /// by construction. The positive framing is the substrate primitive
    /// a compliance auditor asks first ("is this dataset publicly
    /// distributable?"); a future consumer answering that question
    /// reads [`Self::is_public`] rather than re-deriving via
    /// `!is_restricted()` at the callsite. Sealed further against
    /// [`Self::is_regulated`] by
    /// `data_classification_regulated_implies_not_public` — regulated
    /// data is by definition not publicly distributable, the exact
    /// closed-set-internal implication that composes forward through
    /// both the parent-composed and resolver-hop layers on this axis.
    pub const fn is_public(self) -> bool {
        match self {
            Self::Public => true,
            Self::Internal | Self::Confidential | Self::Pii | Self::Phi | Self::Pci => false,
        }
    }
}

// `impl FromStr for DataClassification` +
// `impl tatara_lisp::ClosedSet for DataClassification` +
// `impl std::fmt::Display for DataClassification` +
// `pub struct UnknownDataClassification(pub String)` are all generated
// by `#[derive(tatara_closed_set::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown, display)]` on the
// enum declaration above. The auto-derived carrier label
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

    // ── Classification::gate_compute substrate pins ─────────────────────
    //
    // The six-line `Classification { point_type: Gate, substrate: Compute,
    // horizon: Default::default(), calm: Default::default(),
    // data_classification: Default::default() }` struct-literal was
    // open-coded verbatim at ten hand-authored callsites before the
    // primitive closed it. These pins bind the composed shape at
    // fail-before-pass-after granularity so a regression that flipped a
    // baseline axis, drifted a sibling default, or leaked a non-baseline
    // slot into the substrate composer surfaces HERE rather than as
    // silent operator-visible drift at every unadorned ephemeral env
    // (the one production consumer, `default_ephemeral_class`) AND every
    // downstream test fixture that keys assertions on the shape.

    #[test]
    fn gate_compute_composes_the_five_baseline_axes() {
        // Primary shape: every axis parked at the workspace baseline.
        // A regression that flipped `point_type` off `Gate` or
        // `substrate` off `Compute` — the two axes with no `Default` —
        // surfaces here.
        let c = Classification::gate_compute();
        assert_eq!(c.point_type, ConvergencePointType::Gate);
        assert_eq!(c.substrate, SubstrateType::Compute);
        assert_eq!(c.horizon, Horizon::default());
        assert_eq!(c.calm, CalmClassification::default());
        assert_eq!(c.data_classification, DataClassification::default());
    }

    #[test]
    fn gate_compute_defaulted_axes_ride_sibling_closed_set_defaults() {
        // Pins the sibling-default correspondence the doc comment
        // names — a regression that flipped a sibling default (a new
        // `HorizonKind` variant promoted to `#[default]`, a rename of
        // `CalmClassification::Monotone`, a promotion of `Pii` above
        // `Internal` in the `DataClassification` ordering) would move
        // the baseline HERE rather than at every downstream consumer.
        let c = Classification::gate_compute();
        assert_eq!(c.horizon.kind, HorizonKind::Bounded);
        assert_eq!(c.calm, CalmClassification::Monotone);
        assert_eq!(c.data_classification, DataClassification::Internal);
    }

    #[test]
    fn gate_compute_matches_hand_authored_pre_lift_bytewise() {
        // Byte-identical parity with the pre-lift six-line struct-literal
        // that recurred at ten hand-authored sites. A regression that
        // reshaped the primitive would diverge from the pre-lift block
        // HERE rather than at every downstream fixture that keys on the
        // shape.
        let composed = Classification::gate_compute();
        let hand_authored = Classification {
            point_type: ConvergencePointType::Gate,
            substrate: SubstrateType::Compute,
            horizon: Horizon::default(),
            calm: CalmClassification::default(),
            data_classification: DataClassification::default(),
        };
        assert_eq!(composed, hand_authored);
    }

    #[test]
    fn gate_compute_is_call_time_construction_not_a_shared_singleton() {
        // Two independent calls produce structurally-equal but distinct
        // values — pins that the primitive is a plain constructor
        // rather than a `lazy_static` clone (which would leak a shared
        // singleton whose in-place mutation at one consumer would
        // silently mutate the shape at every other consumer). The `!=`
        // check on `&mut _`-obtained pointer addresses is intentional:
        // a shared singleton would collide, and the pin catches the
        // regression at the primitive rather than at the operator-facing
        // shape-drift downstream.
        let a = Classification::gate_compute();
        let b = Classification::gate_compute();
        assert_eq!(a, b);
        assert!(!std::ptr::eq(&a, &b));
    }

    // ── Classification::gate_compute_with_axis substrate pins ────────
    //
    // Fail-before-pass-after granularity: `gate_compute_with_axis` did
    // not exist before this commit — the (`gate_compute()` with ONE
    // axis slot overwritten by a per-test swept variant) shape recurred
    // at ≥ 40 hand-authored test-fixture callsites, each restating the
    // SAME six-line struct-literal that names FOUR baseline slots
    // verbatim and mutates ONE. Post-lift the shape lives at ONE
    // substrate primitive that composes `Self::gate_compute` with a
    // per-axis overlay through the [`ClassificationAxis`] trait. The
    // two pins below fence the primitive's contract:
    // (1) at every axis, feeding the baseline-of-that-axis variant
    //     reconstructs exactly `gate_compute()` byte-for-byte, so the
    //     overlay is the IDENTITY under baseline input;
    // (2) at every axis, feeding a variant mutates ONLY that axis
    //     slot and leaves the other four at their baseline.
    // A regression that crossed the wires between the five per-axis
    // impls (silently overlaying the wrong slot) or that broke the
    // identity under baseline input (silently drifting the baseline
    // slot on a non-baseline overlay) fails HERE at the substrate
    // primitive rather than at each of the ≥ 40 downstream test
    // callsites that would otherwise silently key an assertion on the
    // wrong axis's variant.

    #[test]
    fn gate_compute_with_axis_is_identity_under_axis_baseline_input() {
        // For each axis, feeding the baseline-of-that-axis variant
        // reconstructs exactly `gate_compute()`. On the two axes with
        // no `Default` (`ConvergencePointType`, `SubstrateType`) the
        // baseline is the `gate_compute` chosen value (`Gate`,
        // `Compute`); on the three defaulted axes the baseline is the
        // sibling closed-set `#[default]` (`Bounded`, `Monotone`,
        // `Internal`).
        let baseline = Classification::gate_compute();
        assert_eq!(
            Classification::gate_compute_with_axis(HorizonKind::Bounded),
            baseline,
        );
        assert_eq!(
            Classification::gate_compute_with_axis(CalmClassification::Monotone),
            baseline,
        );
        assert_eq!(
            Classification::gate_compute_with_axis(DataClassification::Internal),
            baseline,
        );
        assert_eq!(
            Classification::gate_compute_with_axis(ConvergencePointType::Gate),
            baseline,
        );
        assert_eq!(
            Classification::gate_compute_with_axis(SubstrateType::Compute),
            baseline,
        );
    }

    #[test]
    fn gate_compute_with_axis_mutates_only_the_named_axis_slot() {
        // For each axis, sweep every variant and pin that the four
        // sibling axis slots stay at their `gate_compute` baseline
        // while only the named axis slot carries the swept variant.
        // A regression that crossed the per-axis impls (a
        // `ClassificationAxis for HorizonKind` body that mutated
        // `c.calm` instead of `c.horizon.kind`, or a swap between
        // `data_classification` and `calm` impls) fails HERE.
        let baseline = Classification::gate_compute();
        for populated in HorizonKind::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(c.horizon.kind, populated);
            assert_eq!(c.calm, baseline.calm);
            assert_eq!(c.data_classification, baseline.data_classification);
            assert_eq!(c.point_type, baseline.point_type);
            assert_eq!(c.substrate, baseline.substrate);
        }
        for populated in CalmClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(c.calm, populated);
            assert_eq!(c.horizon, baseline.horizon);
            assert_eq!(c.data_classification, baseline.data_classification);
            assert_eq!(c.point_type, baseline.point_type);
            assert_eq!(c.substrate, baseline.substrate);
        }
        for populated in DataClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(c.data_classification, populated);
            assert_eq!(c.horizon, baseline.horizon);
            assert_eq!(c.calm, baseline.calm);
            assert_eq!(c.point_type, baseline.point_type);
            assert_eq!(c.substrate, baseline.substrate);
        }
        for populated in ConvergencePointType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(c.point_type, populated);
            assert_eq!(c.horizon, baseline.horizon);
            assert_eq!(c.calm, baseline.calm);
            assert_eq!(c.data_classification, baseline.data_classification);
            assert_eq!(c.substrate, baseline.substrate);
        }
        for populated in SubstrateType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(c.substrate, populated);
            assert_eq!(c.horizon, baseline.horizon);
            assert_eq!(c.calm, baseline.calm);
            assert_eq!(c.data_classification, baseline.data_classification);
            assert_eq!(c.point_type, baseline.point_type);
        }
    }

    // ── Classification::with_axis chaining primitive pins ────────────
    //
    // Fail-before-pass-after granularity: `with_axis` did not exist
    // before this commit — the (Classification, N-axis-conjunction)
    // construction shape recurred at ≥ 5 hand-authored test-fixture
    // callsites in this file (Fork+Storage, Fork+Storage+NonMonotone,
    // Fork+Storage+NonMonotone+Pii, +HorizonKind::Asymptotic,
    // +direction=Maximize) each restating the FIVE-field struct-
    // literal (`point_type`, `substrate`, `horizon`, `calm`,
    // `data_classification`) verbatim with distinct axis conjunctions.
    // Post-lift the shape lives at ONE substrate primitive that
    // post-composes ONE additional [`ClassificationAxis`] overlay onto
    // an arbitrary [`Classification`] carrier through the SAME trait
    // dispatch [`Classification::gate_compute_with_axis`] uses for the
    // start-from-baseline single-axis overlay. The pins below fence
    // the primitive's contract:
    // (1) chaining N distinct-slot axes onto `gate_compute_with_axis(x)`
    //     produces the SAME [`Classification`] as populating every
    //     slot at once via a hand-authored struct-literal;
    // (2) chaining any permutation of a fixed set of distinct-slot
    //     axes produces the SAME [`Classification`] (order-independent
    //     across distinct slots);
    // (3) the [`OptimizationDirection`] sub-slot overlay PRESERVES the
    //     [`HorizonKind`] sub-slot's prior overlay (both slots live on
    //     the nested `Horizon` struct — a stomping overlay would drop
    //     `direction` to `None` on `.with_axis(HorizonKind::_)`).

    #[test]
    fn with_axis_chains_multi_axis_overlays_matching_open_coded_struct_literal() {
        // Chain the six-axis conjunction (point_type + substrate +
        // calm + data_classification + horizon.kind + horizon.direction)
        // through `with_axis` and pin byte-identical parity with the
        // open-coded FIVE-field struct-literal + nested `Horizon`
        // struct-literal that recurred at the six-axis-independence
        // test-fixture callsite. A regression that (a) mis-routed one
        // `ClassificationAxis::overlay` impl (a stray slot assignment),
        // (b) stomped a prior overlay (the [`HorizonKind`] impl
        // resetting the whole nested `Horizon`), or (c) collapsed the
        // fluent chain onto a single-axis overlay (only the last axis
        // takes effect) would drop parity here.
        let composed = Classification::gate_compute_with_axis(ConvergencePointType::Fork)
            .with_axis(SubstrateType::Storage)
            .with_axis(CalmClassification::NonMonotone)
            .with_axis(DataClassification::Pii)
            .with_axis(HorizonKind::Asymptotic)
            .with_axis(OptimizationDirection::Maximize);
        let hand_authored = Classification {
            point_type: ConvergencePointType::Fork,
            substrate: SubstrateType::Storage,
            horizon: Horizon {
                kind: HorizonKind::Asymptotic,
                direction: Some(OptimizationDirection::Maximize),
                ..Horizon::default()
            },
            calm: CalmClassification::NonMonotone,
            data_classification: DataClassification::Pii,
        };
        assert_eq!(composed, hand_authored);
    }

    #[test]
    fn with_axis_is_order_independent_across_distinct_slot_axes() {
        // Chain the same set of five distinct-slot axes in TWO
        // permutations and pin byte-identical parity. A regression
        // that leaked cross-slot dependency into an overlay impl (a
        // stray [`ConvergencePointType`] impl mutating `c.substrate`,
        // a stray [`DataClassification`] impl mutating `c.calm`, or
        // any impl that read from another slot before writing its
        // own) would drop parity here — an order-dependent overlay
        // means the impls are not commutative, which the
        // (distinct-slot × per-slot-overlay) contract requires. The
        // horizon-nested (`HorizonKind`, `OptimizationDirection`)
        // pair is deliberately NOT included in either permutation
        // here — that pair lives on the SAME nested-struct slot and
        // has an ordering constraint tested by
        // `with_axis_optimization_direction_overlay_preserves_horizon_kind_overlay`
        // below.
        let axes_forward = Classification::gate_compute_with_axis(ConvergencePointType::Fork)
            .with_axis(SubstrateType::Storage)
            .with_axis(CalmClassification::NonMonotone)
            .with_axis(DataClassification::Pii)
            .with_axis(HorizonKind::Asymptotic);
        let axes_reversed = Classification::gate_compute_with_axis(HorizonKind::Asymptotic)
            .with_axis(DataClassification::Pii)
            .with_axis(CalmClassification::NonMonotone)
            .with_axis(SubstrateType::Storage)
            .with_axis(ConvergencePointType::Fork);
        assert_eq!(axes_forward, axes_reversed);
    }

    #[test]
    fn with_axis_optimization_direction_overlay_preserves_horizon_kind_overlay() {
        // Chain `HorizonKind::Asymptotic` THEN
        // `OptimizationDirection::Maximize` — both overlays live on
        // the SAME nested `Horizon` struct's distinct sub-slots
        // (`kind`, `direction`). Pin that the second overlay
        // PRESERVES the first. A regression that either (a) reverted
        // [`ClassificationAxis for HorizonKind`] to the pre-change
        // whole-`Horizon`-reset shape (which would drop `direction`
        // to `None` if that overlay ran after `direction` was set) —
        // pinned via `.with_axis(Maximize).with_axis(Asymptotic)`
        // below — or (b) wrote `OptimizationDirection::overlay`
        // through a whole-`Horizon` reset (which would drop `kind`
        // to `HorizonKind::default()` — Bounded — on this call
        // sequence) would fail HERE. Also pins the byte-symmetric
        // reverse ordering `.with_axis(Maximize).with_axis(Asymptotic)`
        // preserves `direction: Some(Maximize)` — the sub-slot
        // overlays are commutative on the nested struct.
        let forward = Classification::gate_compute_with_axis(HorizonKind::Asymptotic)
            .with_axis(OptimizationDirection::Maximize);
        assert_eq!(forward.horizon.kind, HorizonKind::Asymptotic);
        assert_eq!(
            forward.horizon.direction,
            Some(OptimizationDirection::Maximize),
        );

        let reversed = Classification::gate_compute_with_axis(OptimizationDirection::Maximize)
            .with_axis(HorizonKind::Asymptotic);
        assert_eq!(reversed.horizon.kind, HorizonKind::Asymptotic);
        assert_eq!(
            reversed.horizon.direction,
            Some(OptimizationDirection::Maximize),
        );

        // Byte-parity across the two orderings — both produce the
        // identical (kind, direction) pair AND both preserve every
        // other slot at its `gate_compute` baseline.
        assert_eq!(forward, reversed);
        let baseline = Classification::gate_compute();
        assert_eq!(forward.point_type, baseline.point_type);
        assert_eq!(forward.substrate, baseline.substrate);
        assert_eq!(forward.calm, baseline.calm);
        assert_eq!(forward.data_classification, baseline.data_classification);
        assert_eq!(forward.horizon.metric, baseline.horizon.metric);
        assert_eq!(
            forward.horizon.healthy_rate_threshold,
            baseline.horizon.healthy_rate_threshold,
        );
    }

    #[test]
    fn with_axis_optimization_direction_overlay_wraps_variant_in_some() {
        // For every `OptimizationDirection` variant, `with_axis`
        // sets `horizon.direction = Some(variant)`. A regression
        // that dropped the `Some(...)` wrap (a stray `c.horizon.direction
        // = self.into()` that only compiles because
        // [`Option<OptimizationDirection>: From<OptimizationDirection>`]
        // is derived, but which would silently answer `None` on
        // some variants), or that wrote through the wrong nested
        // slot, fails HERE.
        for populated in OptimizationDirection::ALL {
            let c = Classification::gate_compute_with_axis(HorizonKind::Asymptotic)
                .with_axis(populated);
            assert_eq!(
                c.horizon.direction,
                Some(populated),
                "OptimizationDirection::{populated:?} overlay must set horizon.direction = Some({populated:?})",
            );
            assert_eq!(
                c.horizon.kind,
                HorizonKind::Asymptotic,
                "OptimizationDirection::{populated:?} overlay must preserve prior HorizonKind::Asymptotic overlay",
            );
        }
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
    /// `<Self as tatara_closed_set::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the reconciler hits when parsing a
    /// CRD `enum:`-validated `dataClassification` value back to the
    /// typed classification.
    #[test]
    fn data_classification_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<DataClassification>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the YAML
    /// wire format the reconciler stamps on
    /// `spec.classification.dataClassification`.
    #[test]
    fn data_classification_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<DataClassification>();
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. Any operator-facing
    /// "dataClassification={class}" diagnostic that composes through
    /// Display inherits the canonical wire-format string automatically.
    #[test]
    fn data_classification_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<DataClassification>();
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

    // `unknown_data_classification_message_matches_substrate_convention`
    // removed — clause (5) of
    // `tatara_closed_set::assert_closed_set_well_formed::<DataClassification>()`
    // verifies the substrate-wide `"unknown {SET_LABEL}: {input}"`
    // shape generically (called from
    // `data_classification_is_well_formed_closed_set` above); the
    // `SET_LABEL` projection is pinned by
    // `tatara_lisp_derive::pascal_to_spaced_lowercase_tests`.

    /// TRUTH-TABLE CONTRACT: the predicate pair agrees with the
    /// documented per-variant compliance role. Pinning this table at
    /// one site means any future compliance-baseline auto-selector
    /// reads the same projection that the reconciler writes.
    #[test]
    fn data_classification_predicate_truth_tables() {
        assert!(DataClassification::Public.is_public());
        assert!(!DataClassification::Public.is_restricted());
        assert!(!DataClassification::Public.is_regulated());

        assert!(!DataClassification::Internal.is_public());
        assert!(DataClassification::Internal.is_restricted());
        assert!(!DataClassification::Internal.is_regulated());

        assert!(!DataClassification::Confidential.is_public());
        assert!(DataClassification::Confidential.is_restricted());
        assert!(!DataClassification::Confidential.is_regulated());

        assert!(!DataClassification::Pii.is_public());
        assert!(DataClassification::Pii.is_restricted());
        assert!(DataClassification::Pii.is_regulated());

        assert!(!DataClassification::Phi.is_public());
        assert!(DataClassification::Phi.is_restricted());
        assert!(DataClassification::Phi.is_regulated());

        assert!(!DataClassification::Pci.is_public());
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

    /// POSITIVE-FRAMING TRUTH-TABLE CONTRACT: `is_public` implements
    /// the antisymmetric partner of `is_restricted` — `Public ⇒ true`
    /// and every other variant `⇒ false`. Pinning this table at one
    /// site means any future consumer asking the positive
    /// distribution framing ("is this dataset publicly distributable?")
    /// reads the same projection the compliance auditor reads. A
    /// future variant that flipped this mapping would have to renumber
    /// every consumer deliberately rather than silently promoting an
    /// access-controlled dataset onto the freely-distributable path.
    #[test]
    fn data_classification_is_public_truth_table() {
        assert!(DataClassification::Public.is_public());
        assert!(!DataClassification::Internal.is_public());
        assert!(!DataClassification::Confidential.is_public());
        assert!(!DataClassification::Pii.is_public());
        assert!(!DataClassification::Phi.is_public());
        assert!(!DataClassification::Pci.is_public());
    }

    /// XOR PARTITION CONTRACT: for every [`DataClassification`]
    /// variant, EXACTLY ONE of `is_public` / `is_restricted` is true
    /// — the two predicates carve the closed set into COMPLEMENTARY
    /// buckets (publicly distributable ↔ no access-control regime
    /// applies; access-controlled ↔ some regime applies), the exact
    /// binary partition already sealed on the sibling calm axis by
    /// `calm_classification_monotone_xor_requires_coordination` on the
    /// two-variant closed set, now lifted through the projection layer
    /// to the six-variant data axis. A future variant that returned
    /// `true` for both (publicly distributable AND access-controlled —
    /// a category error) or `false` for both (an inert variant with
    /// no distribution classification — nothing to dispatch on) would
    /// fail here, forcing the author to extend either the predicates
    /// or the [`DataClassification`] enum deliberately. Structural
    /// twin of `calm_classification_monotone_xor_requires_coordination`
    /// and `horizon_kind_terminate_xor_requires_metric_axes` on the
    /// sibling calm + horizon axes — all three binary XOR partitions
    /// publish their two derived-nullary-bool projections as
    /// complementary XOR pairs at ONE site each so the axis carves
    /// into disjoint buckets by construction.
    #[test]
    fn data_classification_public_xor_restricted() {
        for class in DataClassification::ALL {
            assert!(
                class.is_public() ^ class.is_restricted(),
                "{class:?}: is_public() XOR is_restricted() must hold",
            );
        }
    }

    /// ANTISYMMETRIC IMPLICATION CONTRACT: every regulated
    /// classification is NEVER publicly distributable — the impossible
    /// bucket (regulated AND public) is pinned empty on the closed
    /// set. Paired with `data_classification_regulated_implies_restricted`
    /// this is the antisymmetric MUTEX pin against the positive
    /// framing peer — a future variant that returned `(true, true)`
    /// from `(is_regulated, is_public)` would fail HERE, forcing the
    /// author to either flip `is_public` or extend the consumer
    /// dispatch sites (compliance-baseline auto-selector, audit-log
    /// mandatory-fields validator) deliberately rather than silently
    /// producing a regulated class the API server would accept as
    /// freely distributable.
    #[test]
    fn data_classification_regulated_implies_not_public() {
        for class in DataClassification::ALL {
            assert!(
                !class.is_regulated() || !class.is_public(),
                "{class:?} is regulated AND public — \
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
    /// `FromStr` delegates to `<Self as tatara_closed_set::ClosedSet>::parse_label`,
    /// so this helper exercises the same code path the reconciler
    /// hits when parsing a CRD `enum:`-validated value back to the
    /// typed point-type. The forced `[Self; 8]` array literal on
    /// `ConvergencePointType::ALL` still pins the cardinality at the
    /// declaration site.
    #[test]
    fn convergence_point_type_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<ConvergencePointType>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename (or
    /// an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the YAML
    /// wire format the reconciler reads from
    /// `spec.classification.pointType`.
    #[test]
    fn convergence_point_type_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<ConvergencePointType>();
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift.
    #[test]
    fn convergence_point_type_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<ConvergencePointType>();
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

    // `unknown_convergence_point_type_message_matches_substrate_convention`
    // removed — clause (5) of
    // `tatara_closed_set::assert_closed_set_well_formed::<ConvergencePointType>()`
    // verifies the substrate-wide `"unknown {SET_LABEL}: {input}"`
    // shape generically (called from
    // `convergence_point_type_is_well_formed_closed_set` above); the
    // `SET_LABEL` projection is pinned by
    // `tatara_lisp_derive::pascal_to_spaced_lowercase_tests`.

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
    /// CRD-facing enum — it never crosses the wire. Routed through
    /// the substrate-wide [`crate::tagged_union::assert_display_matches_label`]
    /// primitive so the sweep body lives at ONE substrate site rather
    /// than restated per-implementor. Also exercised through the
    /// substrate-wide `every_production_display_impl_binds_through_the_testkit_primitive`
    /// sweep so a per-crate test-site drop cannot silently disable the
    /// check.
    #[test]
    fn arity_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<Arity>();
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

    /// PREDICATE CONTRACT: `is_many` is true exactly for `Arity::Many`
    /// — the antisymmetric image of `is_one` on the current two-
    /// variant closed set. Pins the codomain here means a future
    /// `Arity::Zero` (or any other) variant must declare its own
    /// `is_many` arm deliberately rather than silently defaulting
    /// through a non-closed-set match.
    #[test]
    fn arity_is_many_predicate_truth_table() {
        assert!(!Arity::One.is_many());
        assert!(Arity::Many.is_many());
    }

    /// BINARY XOR PARTITION pin — for every [`Arity`] variant, EXACTLY
    /// ONE of `is_one` / `is_many` holds. Structural twin of the
    /// optimization-direction axis
    /// (`optimization_direction_prefers_lower_xor_prefers_higher`),
    /// the calm axis
    /// (`calm_classification_monotone_xor_requires_coordination`), and
    /// the data axis (`data_classification_public_xor_restricted`) —
    /// all four binary XOR partitions publish their two derived-
    /// nullary-bool projections as complementary XOR pairs at ONE
    /// site each so the closed set carves into disjoint buckets by
    /// construction. A future variant that returned `true` for both
    /// (single AND multi — a category error) or `false` for both (an
    /// inert cardinality with no edge count: a hypothetical `Zero`
    /// sink sentinel MUST answer `false` on BOTH here, forcing the
    /// author to add a third derived-nullary predicate on the closed
    /// set deliberately rather than silently bucketing it onto an
    /// existing cardinality) would fail here, forcing the author to
    /// extend either the predicates or the [`Arity`] enum
    /// deliberately.
    #[test]
    fn arity_is_one_xor_is_many_over_all() {
        for a in Arity::ALL {
            assert!(
                a.is_one() ^ a.is_many(),
                "{a:?}: is_one() XOR is_many() must hold",
            );
        }
    }

    /// COVERAGE CONTRACT: every [`Arity`] variant lands in exactly one
    /// of two cardinality buckets — single (`One`) or multi (`Many`).
    /// Pins the two buckets at their declared cardinalities (1, 1 —
    /// sum to `ALL.len()`) so a future variant lands somewhere
    /// deliberately. Structural mirror of
    /// `optimization_direction_buckets_cover_every_variant` on the
    /// sibling optimization-direction axis.
    #[test]
    fn arity_buckets_cover_every_variant() {
        let mut single = 0u32;
        let mut multi = 0u32;
        for a in Arity::ALL {
            if a.is_one() {
                single += 1;
            } else {
                multi += 1;
            }
        }
        assert_eq!(single, 1, "single-edge bucket: One");
        assert_eq!(multi, 1, "multi-edge bucket: Many");
        assert_eq!(single + multi, Arity::ALL.len() as u32);
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
        tatara_closed_set::assert_closed_set_well_formed::<SubstrateType>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the
    /// YAML wire format the reconciler reads from
    /// `spec.classification.substrate`.
    #[test]
    fn substrate_type_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<SubstrateType>();
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. Any
    /// operator-facing `substrate={kind}` diagnostic that composes
    /// through Display inherits the canonical wire-format string
    /// automatically.
    #[test]
    fn substrate_type_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<SubstrateType>();
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

    // `unknown_substrate_type_message_matches_substrate_convention`
    // removed — clause (5) of
    // `tatara_closed_set::assert_closed_set_well_formed::<SubstrateType>()`
    // verifies the substrate-wide `"unknown {SET_LABEL}: {input}"`
    // shape generically (called from
    // `substrate_type_is_well_formed_closed_set` above); the
    // `SET_LABEL` projection is pinned by
    // `tatara_lisp_derive::pascal_to_spaced_lowercase_tests`.

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
        tatara_closed_set::assert_closed_set_well_formed::<CalmClassification>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the
    /// YAML wire format the reconciler reads from
    /// `spec.classification.calm`.
    #[test]
    fn calm_classification_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<CalmClassification>();
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. Any
    /// operator-facing `calm={kind}` diagnostic that composes
    /// through Display inherits the canonical wire-format string
    /// automatically.
    #[test]
    fn calm_classification_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<CalmClassification>();
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

    // `unknown_calm_classification_message_matches_substrate_convention`
    // removed — clause (5) of
    // `tatara_closed_set::assert_closed_set_well_formed::<CalmClassification>()`
    // verifies the substrate-wide `"unknown {SET_LABEL}: {input}"`
    // shape generically (called from
    // `calm_classification_is_well_formed_closed_set` above); the
    // `SET_LABEL` projection is pinned by
    // `tatara_lisp_derive::pascal_to_spaced_lowercase_tests`.

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

    /// CALM-THEOREM POSITIVE-FRAMING TRUTH-TABLE CONTRACT:
    /// `is_monotone` implements the antisymmetric partner of
    /// `requires_coordination` — `Monotone ⇒ true` and
    /// `NonMonotone ⇒ false`. Pinning this table at one site means
    /// any future consumer asking the positive CALM framing "can
    /// this Process participate in gossip-only writes?" reads the
    /// same projection the lattice ordering and the antisymmetric
    /// `requires_coordination` peer read. A future variant that
    /// flipped this mapping would have to renumber every consumer
    /// deliberately rather than silently promoting a non-monotone
    /// operation onto the gossip path.
    #[test]
    fn calm_classification_is_monotone_truth_table() {
        assert!(CalmClassification::Monotone.is_monotone());
        assert!(!CalmClassification::NonMonotone.is_monotone());
    }

    /// XOR PARTITION CONTRACT: for every [`CalmClassification`]
    /// variant, EXACTLY ONE of `is_monotone` /
    /// `requires_coordination` is true — the two predicates carve
    /// the closed set into COMPLEMENTARY buckets (monotone ↔ no
    /// coordination; non-monotone ↔ requires coordination), the
    /// biconditional half of Hellerstein's CALM theorem as a
    /// closed-set-driven proof. A future variant that returned
    /// `true` for both (a monotone operation that nonetheless
    /// requires coordination — a category error under CALM) or
    /// `false` for both (an inert variant with no monotonicity
    /// classification — nothing to dispatch on) would fail here,
    /// forcing the author to extend either the predicates or the
    /// [`CalmClassification`] enum deliberately. Structural twin of
    /// [`horizon_kind_terminate_xor_requires_metric_axes`] on the
    /// horizon axis — both binary closed sets publish their two
    /// derived-nullary-bool projections as complementary XOR pairs
    /// at ONE site each so the axis carves into disjoint buckets
    /// by construction.
    #[test]
    fn calm_classification_monotone_xor_requires_coordination() {
        for c in CalmClassification::ALL {
            assert!(
                c.is_monotone() ^ c.requires_coordination(),
                "{c:?}: is_monotone() XOR requires_coordination() must hold",
            );
        }
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
        tatara_closed_set::assert_closed_set_well_formed::<OptimizationDirection>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the
    /// YAML wire format the reconciler reads from
    /// `spec.classification.horizon.direction`.
    #[test]
    fn optimization_direction_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<OptimizationDirection>();
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. Any
    /// operator-facing `direction={kind}` diagnostic that composes
    /// through Display inherits the canonical wire-format string
    /// automatically.
    #[test]
    fn optimization_direction_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<OptimizationDirection>();
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

    // `unknown_optimization_direction_message_matches_substrate_convention`
    // removed — clause (5) of
    // `tatara_closed_set::assert_closed_set_well_formed::<OptimizationDirection>()`
    // verifies the substrate-wide `"unknown {SET_LABEL}: {input}"`
    // shape generically (called from
    // `optimization_direction_is_well_formed_closed_set` above); the
    // `SET_LABEL` projection is pinned by
    // `tatara_lisp_derive::pascal_to_spaced_lowercase_tests`.

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

    /// POSITIVE-FRAMING TRUTH-TABLE CONTRACT: `prefers_higher`
    /// implements the antisymmetric partner of `prefers_lower` —
    /// `Minimize ⇒ false` and `Maximize ⇒ true`. Pinning this table
    /// at one site means any future consumer asking the positive
    /// higher-is-better framing ("does this direction reward
    /// throughput / coverage / revenue rate?") reads the same
    /// projection every asymptotic-health probe writes. Mirrors
    /// [`CalmClassification::is_monotone`]'s positive-framing shape
    /// on the sibling binary closed set.
    #[test]
    fn optimization_direction_prefers_higher_truth_table() {
        assert!(!OptimizationDirection::Minimize.prefers_higher());
        assert!(OptimizationDirection::Maximize.prefers_higher());
    }

    /// XOR PARTITION CONTRACT: for every [`OptimizationDirection`]
    /// variant, EXACTLY ONE of `prefers_lower` / `prefers_higher` is
    /// true — the two predicates carve the closed set into
    /// COMPLEMENTARY buckets (lower-is-better ↔ higher-is-better),
    /// the exact binary partition already sealed on the sibling
    /// [`CalmClassification`] axis by
    /// `calm_classification_monotone_xor_requires_coordination` and
    /// on the sibling [`DataClassification`] axis (through the
    /// projection layer) by `data_classification_public_xor_restricted`,
    /// now lifted to the two-variant optimization-direction axis. A
    /// future variant that returned `true` for both (lower AND higher —
    /// a category error) or `false` for both (an inert direction with
    /// no polarity — nothing to dispatch on: a hypothetical `Stabilize`
    /// sentinel MUST answer `false` on BOTH here, forcing the author
    /// to add a third derived-nullary predicate on the closed set
    /// deliberately rather than silently bucketing it onto an existing
    /// polarity) would fail here, forcing the author to extend either
    /// the predicates or the [`OptimizationDirection`] enum deliberately.
    /// Structural twin of `calm_classification_monotone_xor_requires_coordination`
    /// and `data_classification_public_xor_restricted` on the sibling
    /// calm + data axes — all three binary XOR partitions publish
    /// their two derived-nullary-bool projections as complementary
    /// XOR pairs at ONE site each so the axis carves into disjoint
    /// buckets by construction.
    #[test]
    fn optimization_direction_prefers_lower_xor_prefers_higher() {
        for d in OptimizationDirection::ALL {
            assert!(
                d.prefers_lower() ^ d.prefers_higher(),
                "{d:?}: prefers_lower() XOR prefers_higher() must hold",
            );
        }
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
        tatara_closed_set::assert_closed_set_well_formed::<HorizonKind>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, and the
    /// YAML wire format the reconciler stamps on
    /// `spec.classification.horizon.kind`.
    #[test]
    fn horizon_kind_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<HorizonKind>();
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. Any
    /// operator-facing `horizon.kind={kind}` diagnostic that
    /// composes through Display inherits the canonical wire-format
    /// string automatically.
    #[test]
    fn horizon_kind_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<HorizonKind>();
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

    // `unknown_horizon_kind_message_matches_substrate_convention`
    // removed — clause (5) of
    // `tatara_closed_set::assert_closed_set_well_formed::<HorizonKind>()`
    // verifies the substrate-wide `"unknown {SET_LABEL}: {input}"`
    // shape generically (called from
    // `horizon_kind_is_well_formed_closed_set` above); the
    // `SET_LABEL` projection is pinned by
    // `tatara_lisp_derive::pascal_to_spaced_lowercase_tests`.

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

    // ── scalar-carrier presence probe on Classification × ConvergencePointType ──
    //
    // Fail-before-pass-after granularity: [`Classification::has_point_type`]
    // did not exist before this commit — every consumer of the
    // `(Classification, ConvergencePointType) -> bool` scalar-carrier
    // probe shape restated the `classification.point_type == kind`
    // equality body at its own callsite. Post-lift the shape lives at
    // ONE substrate owner and every downstream (the `point-type-<kind>`
    // require-tag family in `tatara-check`, future audit dispatchers
    // walking [`ConvergencePointType::ALL`], any future CRD-facing
    // closed-set discriminator on a required scalar `ProcessSpec` field
    // such as `has_substrate`/`has_calm`/`has_data_classification`)
    // binds through the SAME `has(kind)` shape the Option-slot
    // (`Intent::has`, `Lifetime::has`), slice-level
    // (`ConditionSliceExt::has_kind`, `DependsOnSliceExt::has_must_reach`,
    // `ComplianceBindingSliceExt::has_verification_phase`,
    // `ExportSpecSliceExt::has_{when,channel_kind,report_format,artifact_kind}`),
    // and prior scalar-carrier
    // (`SignalPolicy::has_sighup_strategy`,
    // `EncapsulatesSpec::has_mode`) peers publish.

    /// DIAGONAL — for every [`ConvergencePointType`] variant, a
    /// [`Classification`] whose `point_type` field is set to that
    /// variant returns `true` from `has_point_type` on that same
    /// variant AND `false` on every other variant. Sweep the
    /// [`ConvergencePointType::ALL`] × ALL cross so a regression that
    /// hard-coded the arm to a single variant (silently returning
    /// `true` on every populated classification regardless of query
    /// kind) or wired the equality to a fixed unrelated field fails
    /// HERE at the substrate primitive before landing at the
    /// operator-facing checks.lisp surface.
    #[test]
    fn classification_has_point_type_returns_true_iff_variant_matches() {
        for populated in ConvergencePointType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            for query in ConvergencePointType::ALL {
                assert_eq!(
                    c.has_point_type(query),
                    query == populated,
                    "point_type={populated:?}: query {query:?} classification drifted",
                );
            }
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `point_type: Gate`, so `has_point_type` returns `true` on
    /// [`ConvergencePointType::Gate`] and `false` on every other of
    /// the eight variants. Pins the composition of the substrate's
    /// baseline-constructor primitive with the scalar-carrier
    /// presence probe — a regression that flipped
    /// `gate_compute().point_type` off `Gate` (or wired
    /// `has_point_type` to a fixed variant answer) fails here at ONE
    /// narrow site before drifting across every unadorned ephemeral
    /// env (`default_ephemeral_class`) and every downstream test
    /// fixture that keys assertions on the shape.
    #[test]
    fn classification_gate_compute_has_point_type_gate_only() {
        let c = Classification::gate_compute();
        for kind in ConvergencePointType::ALL {
            let expected = kind == ConvergencePointType::Gate;
            assert_eq!(
                c.has_point_type(kind),
                expected,
                "gate_compute (point_type=Gate) must return {expected} for {kind:?}",
            );
        }
    }

    // ── scalar-carrier presence probe on Classification × SubstrateType ──
    //
    // Fail-before-pass-after granularity: [`Classification::has_substrate`]
    // did not exist before this commit — every consumer of the
    // `(Classification, SubstrateType) -> bool` scalar-carrier probe
    // shape restated the `classification.substrate == kind` equality
    // body at its own callsite. Post-lift the shape lives at ONE
    // substrate owner and every downstream (the `substrate-<kind>`
    // require-tag family in `tatara-check`, future audit dispatchers
    // walking [`SubstrateType::ALL`], any future CRD-facing closed-set
    // discriminator on a required scalar `ProcessSpec` field such as
    // `has_calm`/`has_data_classification`) binds through the SAME
    // `has(kind)` shape the Option-slot (`Intent::has`, `Lifetime::has`),
    // slice-level (`ConditionSliceExt::has_kind`,
    // `DependsOnSliceExt::has_must_reach`,
    // `ComplianceBindingSliceExt::has_verification_phase`,
    // `ExportSpecSliceExt::has_{when,channel_kind,report_format,artifact_kind}`),
    // and prior scalar-carrier
    // (`SignalPolicy::has_sighup_strategy`,
    // `EncapsulatesSpec::has_mode`, `Classification::has_point_type`)
    // peers publish.

    /// DIAGONAL — for every [`SubstrateType`] variant, a
    /// [`Classification`] whose `substrate` field is set to that
    /// variant returns `true` from `has_substrate` on that same
    /// variant AND `false` on every other variant. Sweep the
    /// [`SubstrateType::ALL`] × ALL cross so a regression that
    /// hard-coded the arm to a single variant (silently returning
    /// `true` on every populated classification regardless of query
    /// kind) or wired the equality to a fixed unrelated field (a
    /// stray probe on `classification.point_type`) fails HERE at the
    /// substrate primitive before landing at the operator-facing
    /// checks.lisp surface.
    #[test]
    fn classification_has_substrate_returns_true_iff_variant_matches() {
        for populated in SubstrateType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            for query in SubstrateType::ALL {
                assert_eq!(
                    c.has_substrate(query),
                    query == populated,
                    "substrate={populated:?}: query {query:?} classification drifted",
                );
            }
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `substrate: Compute`, so `has_substrate` returns `true` on
    /// [`SubstrateType::Compute`] and `false` on every other of the
    /// eight variants. Pins the composition of the substrate's
    /// baseline-constructor primitive with the fourth scalar-carrier
    /// presence probe — a regression that flipped
    /// `gate_compute().substrate` off `Compute` (or wired
    /// `has_substrate` to a fixed variant answer, or crossed the
    /// wires to `point_type`) fails here at ONE narrow site before
    /// drifting across every unadorned ephemeral env
    /// (`default_ephemeral_class`) and every downstream test fixture
    /// that keys assertions on the shape. Byte-symmetric with the
    /// peer `classification_gate_compute_has_point_type_gate_only`
    /// pin on the third scalar-carrier — the two co-tenants on the
    /// (required-parent × required-scalar-child) corner walk their
    /// own required axis independently.
    #[test]
    fn classification_gate_compute_has_substrate_compute_only() {
        let c = Classification::gate_compute();
        for kind in SubstrateType::ALL {
            let expected = kind == SubstrateType::Compute;
            assert_eq!(
                c.has_substrate(kind),
                expected,
                "gate_compute (substrate=Compute) must return {expected} for {kind:?}",
            );
        }
    }

    /// TWO-AXIS INDEPENDENCE — the two co-tenants on the (required-
    /// parent × required-scalar-child) corner of the presence-probe
    /// algebra ([`Classification::has_point_type`] and
    /// [`Classification::has_substrate`]) probe distinct required
    /// scalar slots on the SAME [`Classification`] parent, so a
    /// carrier with `point_type: Fork` AND `substrate: Storage`
    /// answers `true` on both fine tags simultaneously and `false`
    /// on every off-diagonal probe of either axis. Pins the two
    /// probes' independence at ONE narrow site — a regression that
    /// collapsed either onto the other's field (a stray probe of
    /// `has_substrate` reading `self.point_type`, or of
    /// `has_point_type` reading `self.substrate`) would fail HERE
    /// before landing at any consumer. The audit `every Fork-topology
    /// Storage-plane point handles SIGHUP by Restart` composes this
    /// exact two-axis conjunction on the required scalars of the
    /// six-axis classification lattice.
    #[test]
    fn classification_has_point_type_and_has_substrate_are_independent() {
        let c = Classification::gate_compute_with_axis(ConvergencePointType::Fork)
            .with_axis(SubstrateType::Storage);
        assert!(c.has_point_type(ConvergencePointType::Fork));
        assert!(c.has_substrate(SubstrateType::Storage));
        assert!(!c.has_point_type(ConvergencePointType::Gate));
        assert!(!c.has_substrate(SubstrateType::Compute));
        // Cross-wiring probe: `has_point_type(Storage-as-if-Point)` and
        // `has_substrate(Fork-as-if-Substrate)` cannot even typecheck
        // — the closed-set enums are disjoint types — but a stray
        // implementation reading the WRONG required field would flip
        // both diagonal answers off. The four asserts above pin the
        // independence at ONE narrow site.
    }

    // ── scalar-carrier presence probe on Classification × CalmClassification ──
    //
    // Fail-before-pass-after granularity: [`Classification::has_calm`]
    // did not exist before this commit — every consumer of the
    // `(Classification, CalmClassification) -> bool` scalar-carrier
    // probe shape restated the `classification.calm == kind` equality
    // body at its own callsite. Post-lift the shape lives at ONE
    // substrate owner and every downstream (the `calm-<kind>`
    // require-tag family in `tatara-check`, future audit dispatchers
    // walking [`CalmClassification::ALL`], any future CRD-facing
    // closed-set discriminator on a defaulted scalar `ProcessSpec`
    // field such as `has_data_classification`) binds through the SAME
    // `has(kind)` shape the Option-slot (`Intent::has`, `Lifetime::has`),
    // slice-level (`ConditionSliceExt::has_kind`,
    // `DependsOnSliceExt::has_must_reach`,
    // `ComplianceBindingSliceExt::has_verification_phase`,
    // `ExportSpecSliceExt::has_{when,channel_kind,report_format,artifact_kind}`),
    // and prior scalar-carrier
    // (`SignalPolicy::has_sighup_strategy`,
    // `EncapsulatesSpec::has_mode`, `Classification::has_point_type`,
    // `Classification::has_substrate`) peers publish. FIRST occupant
    // on the (required-parent × defaulted-scalar-child) corner of the
    // presence-probe algebra — a fresh corner distinct from all four
    // prior scalar-carrier peers.

    /// DIAGONAL — for every [`CalmClassification`] variant, a
    /// [`Classification`] whose `calm` field is set to that variant
    /// returns `true` from `has_calm` on that same variant AND
    /// `false` on every other variant. Sweep the
    /// [`CalmClassification::ALL`] × ALL cross so a regression that
    /// hard-coded the arm to a single variant (silently returning
    /// `true` on every populated classification regardless of query
    /// kind) or wired the equality to a fixed unrelated field (a
    /// stray probe on `classification.point_type` or
    /// `classification.substrate`) fails HERE at the substrate
    /// primitive before landing at the operator-facing checks.lisp
    /// surface.
    #[test]
    fn classification_has_calm_returns_true_iff_variant_matches() {
        for populated in CalmClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            for query in CalmClassification::ALL {
                assert_eq!(
                    c.has_calm(query),
                    query == populated,
                    "calm={populated:?}: query {query:?} classification drifted",
                );
            }
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `calm: CalmClassification::default()` which is
    /// [`CalmClassification::Monotone`] via `#[default]`, so
    /// `has_calm` returns `true` on [`CalmClassification::Monotone`]
    /// and `false` on [`CalmClassification::NonMonotone`]. Pins the
    /// composition of the substrate's baseline-constructor primitive
    /// with the FIFTH scalar-carrier presence probe AND the sibling-
    /// default correspondence documented on [`Classification::gate_compute`]
    /// (which pins the three defaulted axes to the sibling closed-set
    /// defaults `HorizonKind::Bounded` / `CalmClassification::Monotone`
    /// / `DataClassification::Internal`) — a regression that flipped
    /// `gate_compute().calm` off `Monotone` (or promoted a different
    /// variant to `#[default]` on the closed set, or wired `has_calm`
    /// to a fixed variant answer, or crossed the wires to
    /// `point_type` / `substrate`) fails here at ONE narrow site
    /// before drifting across every unadorned ephemeral env
    /// (`default_ephemeral_class`) and every downstream test fixture
    /// that keys assertions on the shape. FIRST occupant on the
    /// (required-parent × defaulted-scalar-child) corner — locks the
    /// corner's characteristic "default-arm short-circuit" property
    /// at ONE narrow classifier site: a bare classification answers
    /// `true` on the default variant (distinct from the
    /// required-child corner peers, where a bare classification must
    /// name a variant deliberately to answer `true`).
    #[test]
    fn classification_gate_compute_has_calm_monotone_only() {
        let c = Classification::gate_compute();
        for kind in CalmClassification::ALL {
            let expected = kind == CalmClassification::Monotone;
            assert_eq!(
                c.has_calm(kind),
                expected,
                "gate_compute (calm=Monotone) must return {expected} for {kind:?}",
            );
        }
    }

    /// THREE-AXIS INDEPENDENCE — the three co-tenants on the
    /// [`Classification`] parent
    /// ([`Classification::has_point_type`] +
    /// [`Classification::has_substrate`] on the (required-parent ×
    /// required-scalar-child) corner AND [`Classification::has_calm`]
    /// on the fresh (required-parent × defaulted-scalar-child)
    /// corner) probe distinct scalar slots on the SAME parent, so a
    /// carrier with `point_type: Fork` AND `substrate: Storage` AND
    /// `calm: NonMonotone` answers `true` on all three fine tags
    /// simultaneously and `false` on every off-diagonal probe of any
    /// axis. Pins the three probes' independence at ONE narrow site
    /// — a regression that collapsed any of the three onto another's
    /// field (a stray probe of `has_calm` reading `self.point_type`
    /// or `self.substrate`, or of either required-axis probe reading
    /// `self.calm`) would fail HERE before landing at any consumer.
    /// The audit `every Fork-topology Storage-plane NonMonotone-CALM
    /// point declares a Raft-guarded write path` composes this exact
    /// three-axis conjunction on the required + defaulted scalars of
    /// the six-axis classification lattice.
    #[test]
    fn classification_has_point_type_and_has_substrate_and_has_calm_are_independent() {
        let c = Classification::gate_compute_with_axis(ConvergencePointType::Fork)
            .with_axis(SubstrateType::Storage)
            .with_axis(CalmClassification::NonMonotone);
        assert!(c.has_point_type(ConvergencePointType::Fork));
        assert!(c.has_substrate(SubstrateType::Storage));
        assert!(c.has_calm(CalmClassification::NonMonotone));
        assert!(!c.has_point_type(ConvergencePointType::Gate));
        assert!(!c.has_substrate(SubstrateType::Compute));
        assert!(!c.has_calm(CalmClassification::Monotone));
    }

    // ── scalar-carrier presence probe on Classification × DataClassification ──
    //
    // Fail-before-pass-after granularity:
    // [`Classification::has_data_classification`] did not exist before
    // this commit — every consumer of the
    // `(Classification, DataClassification) -> bool` scalar-carrier
    // probe shape would have to restate the
    // `classification.data_classification == kind` equality body at
    // its own callsite. Post-lift the shape lives at ONE substrate
    // owner and every downstream (the `data-classification-<kind>`
    // require-tag family in `tatara-check`, future audit dispatchers
    // walking [`DataClassification::ALL`], any future CRD-facing
    // closed-set discriminator on a defaulted scalar `ProcessSpec`
    // field) binds through the SAME `has(kind)` shape the four prior
    // scalar-carrier peers on [`Classification`]
    // ([`Classification::has_point_type`],
    // [`Classification::has_substrate`],
    // [`Classification::has_calm`]) plus
    // [`crate::spec::SignalPolicy::has_sighup_strategy`] and
    // [`crate::encapsulates::EncapsulatesSpec::has_mode`] publish.
    // SECOND occupant on the (required-parent × defaulted-scalar-
    // child) corner of the presence-probe algebra after
    // [`Classification::has_calm`] opened it — pins the corner as a
    // proven-repeatable primitive shape rather than a single-example
    // curiosity and closes the four-scalar-carrier corner-coverage
    // contract on the six-axis classification lattice.

    /// DIAGONAL — for every [`DataClassification`] variant, a
    /// [`Classification`] whose `data_classification` field is set to
    /// that variant returns `true` from `has_data_classification` on
    /// that same variant AND `false` on every other variant. Sweep
    /// the [`DataClassification::ALL`] × ALL cross so a regression
    /// that hard-coded the arm to a single variant (silently returning
    /// `true` on every populated classification regardless of query
    /// kind) or wired the equality to a fixed unrelated field (a
    /// stray probe on `classification.point_type` /
    /// `classification.substrate` / `classification.calm`) fails HERE
    /// at the substrate primitive before landing at the operator-
    /// facing checks.lisp surface.
    #[test]
    fn classification_has_data_classification_returns_true_iff_variant_matches() {
        for populated in DataClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            for query in DataClassification::ALL {
                assert_eq!(
                    c.has_data_classification(query),
                    query == populated,
                    "data_classification={populated:?}: query {query:?} classification drifted",
                );
            }
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `data_classification: DataClassification::default()` which is
    /// [`DataClassification::Internal`] via `#[default]`, so
    /// `has_data_classification` returns `true` on
    /// [`DataClassification::Internal`] and `false` on every other
    /// variant ([`DataClassification::Public`],
    /// [`DataClassification::Confidential`],
    /// [`DataClassification::Pii`], [`DataClassification::Phi`],
    /// [`DataClassification::Pci`]). Pins the composition of the
    /// substrate's baseline-constructor primitive with the SIXTH
    /// scalar-carrier presence probe AND the sibling-default
    /// correspondence documented on [`Classification::gate_compute`]
    /// (which pins the three defaulted axes to the sibling closed-set
    /// defaults `HorizonKind::Bounded` / `CalmClassification::Monotone`
    /// / `DataClassification::Internal`) — a regression that flipped
    /// `gate_compute().data_classification` off `Internal` (or
    /// promoted a different variant to `#[default]` on the closed
    /// set, or wired `has_data_classification` to a fixed variant
    /// answer, or crossed the wires to `point_type` / `substrate` /
    /// `calm`) fails here at ONE narrow site before drifting across
    /// every unadorned ephemeral env (`default_ephemeral_class`) and
    /// every downstream test fixture that keys assertions on the
    /// shape. SECOND occupant on the (required-parent × defaulted-
    /// scalar-child) corner — pins the corner's characteristic
    /// "default-arm short-circuit" property on its second occupant
    /// (peer to `classification_gate_compute_has_calm_monotone_only`
    /// which pins the same shape on the corner's first occupant).
    #[test]
    fn classification_gate_compute_has_data_classification_internal_only() {
        let c = Classification::gate_compute();
        for kind in DataClassification::ALL {
            let expected = kind == DataClassification::Internal;
            assert_eq!(
                c.has_data_classification(kind),
                expected,
                "gate_compute (data_classification=Internal) must return {expected} for {kind:?}",
            );
        }
    }

    /// FOUR-AXIS INDEPENDENCE — the four scalar-carrier co-tenants
    /// on the [`Classification`] parent
    /// ([`Classification::has_point_type`] plus
    /// [`Classification::has_substrate`] on the (required-parent ×
    /// required-scalar-child) corner AND [`Classification::has_calm`]
    /// plus [`Classification::has_data_classification`] on the
    /// (required-parent × defaulted-scalar-child) corner) probe
    /// distinct scalar slots on the SAME parent, so a carrier with
    /// `point_type: Fork` AND `substrate: Storage` AND
    /// `calm: NonMonotone` AND `data_classification: Pii` answers
    /// `true` on all four fine tags simultaneously and `false` on
    /// every off-diagonal probe of any axis. Pins the four probes'
    /// independence at ONE narrow site — a regression that collapsed
    /// any of the four onto another's field (a stray probe of
    /// `has_data_classification` reading `self.point_type` /
    /// `self.substrate` / `self.calm`, or of any prior probe reading
    /// `self.data_classification`) would fail HERE before landing at
    /// any consumer. The audit `every Fork-topology Storage-plane
    /// NonMonotone-CALM Pii-classification point declares a
    /// Raft-guarded write path AND a downstream PII-scrub sink`
    /// composes this exact four-axis conjunction on the required +
    /// defaulted scalars of the six-axis classification lattice.
    /// Closes the four-scalar-carrier corner-coverage contract on
    /// [`Classification`] — its two required-scalar-child slots
    /// (`point_type`, `substrate`) AND its two defaulted-scalar-
    /// child slots (`calm`, `data_classification`) all publish
    /// independent presence probes through the same shape.
    #[test]
    fn classification_four_scalar_carrier_probes_are_independent() {
        let c = Classification::gate_compute_with_axis(ConvergencePointType::Fork)
            .with_axis(SubstrateType::Storage)
            .with_axis(CalmClassification::NonMonotone)
            .with_axis(DataClassification::Pii);
        assert!(c.has_point_type(ConvergencePointType::Fork));
        assert!(c.has_substrate(SubstrateType::Storage));
        assert!(c.has_calm(CalmClassification::NonMonotone));
        assert!(c.has_data_classification(DataClassification::Pii));
        assert!(!c.has_point_type(ConvergencePointType::Gate));
        assert!(!c.has_substrate(SubstrateType::Compute));
        assert!(!c.has_calm(CalmClassification::Monotone));
        assert!(!c.has_data_classification(DataClassification::Internal));
        assert!(!c.has_data_classification(DataClassification::Public));
        assert!(!c.has_data_classification(DataClassification::Phi));
    }

    // ── nested-struct-scalar-carrier presence probe on Classification × HorizonKind ──
    //
    // Fail-before-pass-after granularity:
    // [`Classification::has_horizon_kind`] did not exist before this
    // commit — every consumer of the `(Classification, HorizonKind) ->
    // bool` two-hop `self.horizon.kind == kind` probe shape would have
    // to restate the nested-struct field walk at its own callsite.
    // Post-lift the shape lives at ONE substrate owner and every
    // downstream (the `horizon-<kind>` require-tag family in
    // `tatara-check`, future audit dispatchers walking
    // [`HorizonKind::ALL`], any future CRD-facing nested-struct-scalar
    // discriminator on `ProcessSpec`) binds through the SAME
    // `has(kind)` shape the four prior scalar-carrier peers on
    // [`Classification`] ([`Classification::has_point_type`],
    // [`Classification::has_substrate`], [`Classification::has_calm`],
    // [`Classification::has_data_classification`]) plus
    // [`crate::spec::SignalPolicy::has_sighup_strategy`] and
    // [`crate::encapsulates::EncapsulatesSpec::has_mode`] publish.
    // FIRST occupant on the (required-parent × nested-struct-scalar-
    // child) corner of the presence-probe algebra — a fresh corner
    // distinct from the four corner-property-exhaustive scalar-carrier
    // peers on [`Classification`] (whose bodies read a closed-set
    // discriminator directly off a scalar slot without an intermediate
    // struct hop).

    /// DIAGONAL — for every [`HorizonKind`] variant, a
    /// [`Classification`] whose `horizon.kind` field is set to that
    /// variant returns `true` from `has_horizon_kind` on that same
    /// variant AND `false` on every other variant. Sweep the
    /// [`HorizonKind::ALL`] × ALL cross so a regression that
    /// hard-coded the arm to a single variant (silently returning
    /// `true` on every populated classification regardless of query
    /// kind) or wired the equality to a fixed unrelated field (a
    /// stray probe on `classification.point_type` /
    /// `classification.substrate` / `classification.calm` /
    /// `classification.data_classification`, or a direct probe on the
    /// nested [`Horizon`] struct that ignored the discriminator arm)
    /// fails HERE at the substrate primitive before landing at the
    /// operator-facing checks.lisp surface. The nested-struct hop
    /// distinguishes this corner from the four scalar-carrier peers:
    /// the probe walks `self.horizon.kind` not `self.<field>`, so a
    /// regression that mis-routed the field walk (a stray
    /// `self.horizon == kind` that could not typecheck, or a stray
    /// `self.horizon.direction == kind` that would trip a different
    /// closed-set discriminator) fails at the compiler before the
    /// runtime diagonal even runs.
    #[test]
    fn classification_has_horizon_kind_returns_true_iff_variant_matches() {
        for populated in HorizonKind::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            for query in HorizonKind::ALL {
                assert_eq!(
                    c.has_horizon_kind(query),
                    query == populated,
                    "horizon.kind={populated:?}: query {query:?} classification drifted",
                );
            }
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `horizon: Horizon::default()` whose `kind` field defaults to
    /// [`HorizonKind::Bounded`] via `#[default]`, so `has_horizon_kind`
    /// returns `true` on [`HorizonKind::Bounded`] and `false` on
    /// [`HorizonKind::Asymptotic`]. Pins the composition of the
    /// substrate's baseline-constructor primitive with the SEVENTH
    /// presence-probe peer AND the sibling-default correspondence
    /// documented on [`Classification::gate_compute`] (which pins the
    /// three defaulted axes to the sibling closed-set defaults
    /// `HorizonKind::Bounded` / `CalmClassification::Monotone` /
    /// `DataClassification::Internal`) — a regression that flipped
    /// `Horizon::default().kind` off `Bounded` (or promoted
    /// `Asymptotic` to `#[default]` on [`HorizonKind`], or wired
    /// `has_horizon_kind` to a fixed variant answer, or crossed the
    /// wires through the wrong nested struct) fails here at ONE
    /// narrow site before drifting across every unadorned ephemeral
    /// env (`default_ephemeral_class`) and every downstream test
    /// fixture that keys assertions on the shape. FIRST occupant on
    /// the (required-parent × nested-struct-scalar-child) corner —
    /// locks the corner's characteristic "default-arm short-circuit
    /// reaches through the nested struct's own default" property at
    /// ONE narrow site.
    #[test]
    fn classification_gate_compute_has_horizon_kind_bounded_only() {
        let c = Classification::gate_compute();
        for kind in HorizonKind::ALL {
            let expected = kind == HorizonKind::Bounded;
            assert_eq!(
                c.has_horizon_kind(kind),
                expected,
                "gate_compute (horizon.kind=Bounded) must return {expected} for {kind:?}",
            );
        }
    }

    /// FIVE-AXIS INDEPENDENCE — the FIVE presence-probe co-tenants on
    /// the [`Classification`] parent
    /// ([`Classification::has_point_type`] plus
    /// [`Classification::has_substrate`] on the (required-parent ×
    /// required-scalar-child) corner AND
    /// [`Classification::has_calm`] plus
    /// [`Classification::has_data_classification`] on the (required-
    /// parent × defaulted-scalar-child) corner AND
    /// [`Classification::has_horizon_kind`] on the fresh (required-
    /// parent × nested-struct-scalar-child) corner) probe distinct
    /// slots on the SAME parent, so a carrier with `point_type: Fork`
    /// AND `substrate: Storage` AND `calm: NonMonotone` AND
    /// `data_classification: Pii` AND `horizon.kind: Asymptotic`
    /// answers `true` on all five fine tags simultaneously and
    /// `false` on every off-diagonal probe of any axis. Pins the five
    /// probes' independence at ONE narrow site — a regression that
    /// collapsed any of the five onto another's field (a stray probe
    /// of `has_horizon_kind` reading `self.point_type` /
    /// `self.substrate` / `self.calm` / `self.data_classification`,
    /// or of any prior probe reading through `self.horizon.kind`)
    /// would fail HERE before landing at any consumer. The audit
    /// `every Fork-topology Storage-plane NonMonotone-CALM
    /// Pii-classification Asymptotic-horizon point declares a
    /// Raft-guarded write path AND a downstream PII-scrub sink AND a
    /// rate-window healthy-threshold metric` composes this exact
    /// five-axis conjunction on the five classification-axis
    /// discriminators of the six-axis classification lattice — opens
    /// the five-way corner-coverage contract on [`Classification`],
    /// straddling THREE distinct corners of the (parent-shape ×
    /// child-shape) algebra (the required-child corner
    /// `has_point_type` + `has_substrate` share, the defaulted-child
    /// corner `has_calm` + `has_data_classification` share, and the
    /// nested-struct-child corner `has_horizon_kind` opens).
    #[test]
    fn classification_five_presence_probes_are_independent() {
        let c = Classification::gate_compute_with_axis(ConvergencePointType::Fork)
            .with_axis(SubstrateType::Storage)
            .with_axis(CalmClassification::NonMonotone)
            .with_axis(DataClassification::Pii)
            .with_axis(HorizonKind::Asymptotic);
        assert!(c.has_point_type(ConvergencePointType::Fork));
        assert!(c.has_substrate(SubstrateType::Storage));
        assert!(c.has_calm(CalmClassification::NonMonotone));
        assert!(c.has_data_classification(DataClassification::Pii));
        assert!(c.has_horizon_kind(HorizonKind::Asymptotic));
        assert!(!c.has_point_type(ConvergencePointType::Gate));
        assert!(!c.has_substrate(SubstrateType::Compute));
        assert!(!c.has_calm(CalmClassification::Monotone));
        assert!(!c.has_data_classification(DataClassification::Internal));
        assert!(!c.has_horizon_kind(HorizonKind::Bounded));
    }

    // ── nested-struct-Option-scalar-carrier presence probe on Classification × OptimizationDirection ──
    //
    // Fail-before-pass-after granularity:
    // [`Classification::has_optimization_direction`] did not exist
    // before this commit — every consumer of the
    // `(Classification, OptimizationDirection) -> bool` two-hop
    // `self.horizon.direction.unwrap_or_default() == kind` probe
    // shape would have to restate the nested-struct-Option field
    // walk at its own callsite. Post-lift the shape lives at ONE
    // substrate owner and every downstream (the
    // `optimization-direction-<kind>` require-tag family in
    // `tatara-check`, future audit dispatchers walking
    // [`OptimizationDirection::ALL`], any future CRD-facing nested-
    // struct-Option-scalar discriminator on `ProcessSpec`) binds
    // through the SAME `has(kind)` shape the six prior presence
    // probes on [`Classification`] plus its cousins on
    // [`crate::spec::SignalPolicy`] and
    // [`crate::encapsulates::EncapsulatesSpec`] publish. SECOND
    // occupant on the (required-parent × nested-struct-scalar-
    // child) corner of the presence-probe algebra — the FIRST
    // occupant [`Classification::has_horizon_kind`] read the nested
    // scalar `horizon.kind: HorizonKind` DIRECTLY; this probe adds
    // the `Option`-hop through `direction: Option<OptimizationDirection>`
    // via `Option::unwrap_or_default`, pinning the corner as a
    // proven-repeatable primitive shape rather than a single-example
    // curiosity.

    /// DIAGONAL — for every [`OptimizationDirection`] variant, a
    /// [`Classification`] whose `horizon.direction` field is set to
    /// `Some(that variant)` returns `true` from
    /// `has_optimization_direction` on that same variant AND
    /// `false` on every other variant. Sweep the
    /// [`OptimizationDirection::ALL`] × ALL cross so a regression
    /// that hard-coded the arm to a single variant (silently
    /// returning `true` on every populated classification regardless
    /// of query kind) or wired the equality to a fixed unrelated
    /// field (a stray probe on `classification.point_type` /
    /// `classification.substrate` / `classification.calm` /
    /// `classification.data_classification` /
    /// `classification.horizon.kind`, or a direct probe on the
    /// nested [`Horizon`] struct that ignored the `direction` arm)
    /// fails HERE at the substrate primitive before landing at the
    /// operator-facing checks.lisp surface. The `Option`-hop
    /// distinguishes this method from the direct-nested-scalar
    /// peer [`Classification::has_horizon_kind`]: the probe walks
    /// `self.horizon.direction.unwrap_or_default()` not
    /// `self.horizon.kind`, so a regression that mis-routed the
    /// field walk (a stray `self.horizon.kind == kind` that could
    /// not typecheck, or a stray `self.horizon == kind` that also
    /// could not typecheck) fails at the compiler before the
    /// runtime diagonal even runs.
    #[test]
    fn classification_has_optimization_direction_returns_true_iff_variant_matches() {
        for populated in OptimizationDirection::ALL {
            let c = Classification::gate_compute_with_axis(HorizonKind::Asymptotic)
                .with_axis(populated);
            for query in OptimizationDirection::ALL {
                assert_eq!(
                    c.has_optimization_direction(query),
                    query == populated,
                    "horizon.direction=Some({populated:?}): query {query:?} classification drifted",
                );
            }
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `horizon: Horizon::default()` whose `direction` field defaults
    /// to `None`. Under [`Option::unwrap_or_default`] the probe
    /// answers as if the field were `OptimizationDirection::default()`
    /// = [`OptimizationDirection::Minimize`] via `#[default]`, so
    /// `has_optimization_direction` returns `true` on
    /// [`OptimizationDirection::Minimize`] and `false` on
    /// [`OptimizationDirection::Maximize`]. Pins the composition of
    /// the substrate's baseline-constructor primitive with the
    /// EIGHTH presence-probe peer AND the closed-set-default
    /// correspondence documented on [`OptimizationDirection`] —
    /// a regression that flipped `OptimizationDirection::default()`
    /// off `Minimize` (which would silently invert every unadorned
    /// `Asymptotic` Process's rate-window evaluator polarity), or
    /// wired `has_optimization_direction` to a fixed variant answer,
    /// or crossed the wires through the wrong nested struct or the
    /// wrong Option-slot, fails here at ONE narrow site before
    /// drifting across every unadorned ephemeral env
    /// (`default_ephemeral_class`) and every downstream test fixture
    /// that keys assertions on the shape. SECOND occupant on the
    /// (required-parent × nested-struct-scalar-child) corner —
    /// locks the corner's Option-hop default-arm short-circuit
    /// property at ONE narrow site (the Option `None` folds onto
    /// the closed set's `#[default]` via `unwrap_or_default`,
    /// mirroring the direct-nested-scalar's default-arm short-
    /// circuit through the nested struct's own default).
    #[test]
    fn classification_gate_compute_has_optimization_direction_minimize_only() {
        let c = Classification::gate_compute();
        for kind in OptimizationDirection::ALL {
            let expected = kind == OptimizationDirection::Minimize;
            assert_eq!(
                c.has_optimization_direction(kind),
                expected,
                "gate_compute (horizon.direction=None ⇒ default Minimize) must return {expected} for {kind:?}",
            );
        }
    }

    /// SIX-AXIS INDEPENDENCE — the SIX presence-probe co-tenants on
    /// the [`Classification`] parent
    /// ([`Classification::has_point_type`] plus
    /// [`Classification::has_substrate`] on the (required-parent ×
    /// required-scalar-child) corner AND
    /// [`Classification::has_calm`] plus
    /// [`Classification::has_data_classification`] on the (required-
    /// parent × defaulted-scalar-child) corner AND
    /// [`Classification::has_horizon_kind`] plus
    /// [`Classification::has_optimization_direction`] on the
    /// (required-parent × nested-struct-scalar-child) corner) probe
    /// distinct slots on the SAME parent, so a carrier with
    /// `point_type: Fork` AND `substrate: Storage` AND
    /// `calm: NonMonotone` AND `data_classification: Pii` AND
    /// `horizon.kind: Asymptotic` AND
    /// `horizon.direction: Some(Maximize)` answers `true` on all six
    /// fine tags simultaneously and `false` on every off-diagonal
    /// probe of any axis. Pins the six probes' independence at ONE
    /// narrow site — a regression that collapsed any of the six
    /// onto another's field (a stray probe of
    /// `has_optimization_direction` reading `self.point_type` /
    /// `self.substrate` / `self.calm` /
    /// `self.data_classification` / `self.horizon.kind`, or of any
    /// prior probe reading through `self.horizon.direction`) would
    /// fail HERE before landing at any consumer. The audit
    /// `every Fork-topology Storage-plane NonMonotone-CALM
    /// Pii-classification Asymptotic-horizon Maximize-direction
    /// point declares a rate-window healthy-threshold metric and a
    /// throughput-oriented SLO` composes this exact six-axis
    /// conjunction on the six classification-axis discriminators of
    /// the six-axis classification lattice — populates the six-way
    /// corner-coverage contract on [`Classification`], now
    /// straddling THREE distinct corners of the (parent-shape ×
    /// child-shape) algebra with TWO co-tenants each on the
    /// nested-struct-child corner: direct-nested-scalar
    /// (`has_horizon_kind`) and Option-nested-scalar
    /// (`has_optimization_direction`).
    #[test]
    fn classification_six_presence_probes_are_independent() {
        let c = Classification::gate_compute_with_axis(ConvergencePointType::Fork)
            .with_axis(SubstrateType::Storage)
            .with_axis(CalmClassification::NonMonotone)
            .with_axis(DataClassification::Pii)
            .with_axis(HorizonKind::Asymptotic)
            .with_axis(OptimizationDirection::Maximize);
        assert!(c.has_point_type(ConvergencePointType::Fork));
        assert!(c.has_substrate(SubstrateType::Storage));
        assert!(c.has_calm(CalmClassification::NonMonotone));
        assert!(c.has_data_classification(DataClassification::Pii));
        assert!(c.has_horizon_kind(HorizonKind::Asymptotic));
        assert!(c.has_optimization_direction(OptimizationDirection::Maximize));
        assert!(!c.has_point_type(ConvergencePointType::Gate));
        assert!(!c.has_substrate(SubstrateType::Compute));
        assert!(!c.has_calm(CalmClassification::Monotone));
        assert!(!c.has_data_classification(DataClassification::Internal));
        assert!(!c.has_horizon_kind(HorizonKind::Bounded));
        assert!(!c.has_optimization_direction(OptimizationDirection::Minimize));
    }

    // ── derived-typed-projection presence probe on Classification × Arity ──
    //
    // Fail-before-pass-after granularity:
    // [`Classification::has_input_arity`] did not exist before this
    // commit — every consumer of the `(Classification, Arity) -> bool`
    // two-hop `self.point_type.input_arity() == kind` probe shape
    // would have to restate the derived-typed-projection walk at its
    // own callsite. Post-lift the shape lives at ONE substrate owner
    // and every downstream (the `input-arity-<kind>` require-tag
    // family in `tatara-check`, future DAG-composition validators
    // walking [`Arity::ALL`], any future consumer keying on the
    // input-edge cardinality of a Process's convergence point) binds
    // through the SAME `has(kind)` shape the two prior nested-struct-
    // scalar-child peers on [`Classification`]
    // ([`Classification::has_horizon_kind`] and
    // [`Classification::has_optimization_direction`]) publish. FIRST
    // occupant of the DERIVED-TYPED-PROJECTION variant on the
    // (required-parent × nested-struct-scalar-child) corner — widening
    // the corner from "raw discriminator only" to "raw discriminator
    // OR typed projection over the child", mirroring the derived-typed-
    // projection precedent
    // [`crate::export::ExportSpecSliceExt::has_report_payload_shape`]
    // set on the (Option-parent × Vec-child × nested-Option-carrier ×
    // derived-typed-projection) corner.

    /// PROJECTION-TRUTH-TABLE — for every [`ConvergencePointType`]
    /// variant, `has_input_arity` on a [`Classification`] whose
    /// `point_type` field is set to that variant returns `true` on
    /// EXACTLY the [`Arity`] variant that
    /// [`ConvergencePointType::input_arity`] projects to (and `false`
    /// on every other variant). Sweep the
    /// [`ConvergencePointType::ALL`] × [`Arity::ALL`] cross so a
    /// regression that (a) probed [`ConvergencePointType`] directly
    /// (dropping the `.input_arity()` call, silently answering `true`
    /// on the populated slot only when the query happens to name the
    /// same variant), (b) inverted the projection (`One ↔ Many`), (c)
    /// crossed the wires with the sibling
    /// [`ConvergencePointType::output_arity`] projection (which
    /// disagrees on the fan-out arms), (d) hard-coded the arm to a
    /// single [`Arity`] (silently returning `true` for every
    /// populated classification regardless of query kind), or (e)
    /// wired the equality to a fixed unrelated field fails HERE at
    /// the substrate primitive before landing at the operator-facing
    /// checks.lisp surface. The projection's many-to-one shape is
    /// pinned SYMMETRICALLY on both sides of the cross: `Transform`,
    /// `Fork`, `Broadcast`, `Observe` populated arms answer `true`
    /// only for `Arity::One`; `Join`, `Gate`, `Select`, `Reduce`
    /// populated arms answer `true` only for `Arity::Many`.
    #[test]
    fn classification_has_input_arity_returns_true_iff_projection_matches_per_kind() {
        for populated in ConvergencePointType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            let expected_arity = populated.input_arity();
            for query in Arity::ALL {
                assert_eq!(
                    c.has_input_arity(query),
                    query == expected_arity,
                    "point_type={populated:?} → input_arity={expected_arity:?}: query {query:?} classification drifted",
                );
            }
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `point_type: ConvergencePointType::Gate`, and
    /// [`ConvergencePointType::input_arity`] projects `Gate → Many`,
    /// so `has_input_arity` returns `true` on [`Arity::Many`] and
    /// `false` on [`Arity::One`]. Pins the composition of the
    /// substrate's baseline-constructor primitive with this
    /// presence-probe peer and the sibling-projection correspondence
    /// (which pins `Gate` to the fan-in `Many` bucket at ONE
    /// projection site) — a regression that flipped `Gate`'s
    /// `input_arity` bucket (silently mis-classifying every Gate as a
    /// `One`-input point at every downstream DAG-composition
    /// validator + this require-tag family), or that wired
    /// `has_input_arity` to a fixed arity answer, or that crossed the
    /// wires with `output_arity` (which sends `Gate → One`, the
    /// opposite bucket) fails here at ONE narrow site before drifting
    /// across every downstream fixture that keys assertions on the
    /// shape. FIRST derived-typed-projection occupant on the
    /// (required-parent × nested-struct-scalar-child) corner — locks
    /// the corner's characteristic "projection propagates through a
    /// bucket collapse consistently" property at ONE narrow site.
    #[test]
    fn classification_gate_compute_has_input_arity_many_only() {
        let c = Classification::gate_compute();
        for kind in Arity::ALL {
            let expected = kind == Arity::Many;
            assert_eq!(
                c.has_input_arity(kind),
                expected,
                "gate_compute (point_type=Gate → input_arity=Many) must return {expected} for {kind:?}",
            );
        }
    }

    /// SIBLING-INDEPENDENCE — the peer scalar-carrier
    /// [`Classification::has_point_type`] and the peer derived-typed-
    /// projection [`Classification::has_input_arity`] read the SAME
    /// underlying slot (`self.point_type`) but through different
    /// closed sets ([`ConvergencePointType::ALL`] vs.
    /// [`Arity::ALL`]) — the arity probe is a many-to-one collapse of
    /// the point-type probe through
    /// [`ConvergencePointType::input_arity`]. A carrier with
    /// `point_type: Fork` MUST simultaneously answer
    /// `has_point_type(Fork) = true` AND
    /// `has_input_arity(One) = true` (Fork's input_arity projection),
    /// AND simultaneously answer
    /// `has_point_type(Broadcast) = false` (different variant, same
    /// bucket) AND `has_input_arity(Many) = false` (opposite bucket).
    /// Pins the projection-composition contract at ONE narrow site —
    /// a regression that (a) collapsed `has_input_arity` onto
    /// `has_point_type` (silently answering `true` only when the
    /// query names the raw point type, an out-of-vocabulary Arity
    /// query), (b) collapsed `has_point_type` onto `has_input_arity`
    /// (silently answering `true` for every point-type in the same
    /// arity bucket), or (c) swapped the projection direction fails
    /// HERE at the substrate before landing at any consumer. Peer of
    /// the [`crate::export::ExportSpecSliceExt`]'s
    /// SAME-CARRIER PROJECTION-COEXISTENCE pins on the
    /// `Option<TestReportSource>` nested-Option carrier
    /// (`has_report_format` vs. `has_report_payload_shape`) — the
    /// same "one carrier, two probes at different projection depths"
    /// contract pinned on the (required-parent × nested-struct-
    /// scalar-child) corner rather than on the (Option-parent ×
    /// Vec-child × nested-Option-carrier) corner.
    #[test]
    fn classification_has_input_arity_and_has_point_type_coexist_via_projection() {
        let c = Classification::gate_compute_with_axis(ConvergencePointType::Fork);
        assert!(c.has_point_type(ConvergencePointType::Fork));
        assert!(c.has_input_arity(Arity::One));
        assert!(!c.has_point_type(ConvergencePointType::Broadcast));
        assert!(!c.has_input_arity(Arity::Many));
    }

    // ── second derived-typed-projection presence probe on Classification × Arity ──
    //
    // Fail-before-pass-after granularity:
    // [`Classification::has_output_arity`] did not exist before this
    // commit — every consumer of the `(Classification, Arity) -> bool`
    // two-hop `self.point_type.output_arity() == kind` probe shape
    // would have to restate the derived-typed-projection walk at its
    // own callsite. Post-lift the shape lives at ONE substrate owner
    // and every downstream (the `output-arity-<kind>` require-tag
    // family in `tatara-check`, future DAG-composition validators
    // walking [`Arity::ALL`] on the fan-out side, any future consumer
    // keying on the output-edge cardinality of a Process's convergence
    // point) binds through the SAME `has(kind)` shape the peer input-
    // side probe [`Classification::has_input_arity`] publishes. SECOND
    // occupant of the DERIVED-TYPED-PROJECTION variant on the
    // (required-parent × nested-struct-scalar-child) corner — closing
    // the DAG-composition arity pair by mirroring `has_input_arity`
    // through the sibling [`ConvergencePointType::output_arity`]
    // projection.

    /// PROJECTION-TRUTH-TABLE — for every [`ConvergencePointType`]
    /// variant, `has_output_arity` on a [`Classification`] whose
    /// `point_type` field is set to that variant returns `true` on
    /// EXACTLY the [`Arity`] variant that
    /// [`ConvergencePointType::output_arity`] projects to (and `false`
    /// on every other variant). Sweep the
    /// [`ConvergencePointType::ALL`] × [`Arity::ALL`] cross so a
    /// regression that (a) probed [`ConvergencePointType`] directly
    /// (dropping the `.output_arity()` call, silently answering `true`
    /// on the populated slot only when the query happens to name the
    /// same variant), (b) inverted the projection (`One ↔ Many`), (c)
    /// crossed the wires with the sibling
    /// [`ConvergencePointType::input_arity`] projection (which
    /// disagrees on the diffusive `Fork | Broadcast → Many` output vs.
    /// `Fork | Broadcast → One` input, AND on the convergent `Join |
    /// Gate | Select | Reduce → One` output vs. `Many` input), (d)
    /// hard-coded the arm to a single [`Arity`] (silently returning
    /// `true` for every populated classification regardless of query
    /// kind), or (e) wired the equality to a fixed unrelated field
    /// fails HERE at the substrate primitive before landing at the
    /// operator-facing checks.lisp surface. The projection's many-to-
    /// one shape is pinned SYMMETRICALLY on both sides of the cross:
    /// `Fork`, `Broadcast` populated arms answer `true` only for
    /// `Arity::Many`; every other variant answers `true` only for
    /// `Arity::One`.
    #[test]
    fn classification_has_output_arity_returns_true_iff_projection_matches_per_kind() {
        for populated in ConvergencePointType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            let expected_arity = populated.output_arity();
            for query in Arity::ALL {
                assert_eq!(
                    c.has_output_arity(query),
                    query == expected_arity,
                    "point_type={populated:?} → output_arity={expected_arity:?}: query {query:?} classification drifted",
                );
            }
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `point_type: ConvergencePointType::Gate`, and
    /// [`ConvergencePointType::output_arity`] projects `Gate → One`,
    /// so `has_output_arity` returns `true` on [`Arity::One`] and
    /// `false` on [`Arity::Many`] — the MIRROR of the peer
    /// [`Self::has_input_arity`] baseline (`Gate → input_arity = Many`),
    /// which pins the convergent `(Many, One)` bucket at ONE
    /// projection pair site. Pins the composition of the substrate's
    /// baseline-constructor primitive with this presence-probe peer
    /// and the sibling-projection correspondence (which pins `Gate` to
    /// the fan-in `Many` input × fan-out `One` output cell) — a
    /// regression that flipped `Gate`'s `output_arity` bucket
    /// (silently mis-classifying every Gate as a `Many`-output point
    /// at every downstream DAG-composition validator + this
    /// require-tag family), or that wired `has_output_arity` to a
    /// fixed arity answer, or that crossed the wires with
    /// `input_arity` (which sends `Gate → Many`, the opposite bucket)
    /// fails here at ONE narrow site before drifting across every
    /// downstream fixture that keys assertions on the shape.
    #[test]
    fn classification_gate_compute_has_output_arity_one_only() {
        let c = Classification::gate_compute();
        for kind in Arity::ALL {
            let expected = kind == Arity::One;
            assert_eq!(
                c.has_output_arity(kind),
                expected,
                "gate_compute (point_type=Gate → output_arity=One) must return {expected} for {kind:?}",
            );
        }
    }

    /// CROSS-PROJECTION COEXISTENCE — the peer input-side probe
    /// [`Classification::has_input_arity`] and the output-side probe
    /// [`Classification::has_output_arity`] read the SAME underlying
    /// slot (`self.point_type`) through the SAME closed set
    /// ([`Arity::ALL`]) but through DIFFERENT typed projections
    /// ([`ConvergencePointType::input_arity`] vs.
    /// [`ConvergencePointType::output_arity`]). A carrier with
    /// `point_type: Fork` (the diffusive `(One, Many)` cell) MUST
    /// simultaneously answer `has_input_arity(One) = true` AND
    /// `has_output_arity(Many) = true` (Fork's arity pair), AND
    /// simultaneously answer `has_input_arity(Many) = false` AND
    /// `has_output_arity(One) = false` (opposite buckets). A carrier
    /// with `point_type: Transform` (the endomorphic `(One, One)`
    /// cell) MUST answer both probes with `Arity::One = true` — the
    /// two projections AGREE in the endomorphic bucket. A carrier with
    /// `point_type: Gate` (the convergent `(Many, One)` cell) MUST
    /// answer `has_input_arity(Many) = true` AND
    /// `has_output_arity(One) = true` — the mirror of the Fork case.
    /// Pins the projection-composition contract at ONE narrow site —
    /// a regression that (a) collapsed `has_output_arity` onto
    /// `has_input_arity` (silently answering the input arity for
    /// every output query on Fork/Broadcast/Join/Gate/Select/Reduce,
    /// the six variants where the two projections disagree), (b)
    /// swapped the projection direction (`Fork → (Many, One)` instead
    /// of `(One, Many)`), or (c) drifted the topology-bucket contract
    /// (silently mis-classifying Fork as endomorphic) fails HERE at
    /// the substrate before landing at any consumer. THIS is the DAG-
    /// composition arity pair pinned at ONE narrow site — the exact
    /// property `convergence_point_type_arity_pair_agrees_with_bucket`
    /// pins on the source projection functions themselves.
    #[test]
    fn classification_has_input_arity_and_has_output_arity_pin_dag_composition_pair() {
        let fork = Classification::gate_compute_with_axis(ConvergencePointType::Fork);
        assert!(fork.has_input_arity(Arity::One));
        assert!(fork.has_output_arity(Arity::Many));
        assert!(!fork.has_input_arity(Arity::Many));
        assert!(!fork.has_output_arity(Arity::One));

        let transform = Classification::gate_compute_with_axis(ConvergencePointType::Transform);
        assert!(transform.has_input_arity(Arity::One));
        assert!(transform.has_output_arity(Arity::One));
        assert!(!transform.has_input_arity(Arity::Many));
        assert!(!transform.has_output_arity(Arity::Many));

        let gate = Classification::gate_compute();
        assert!(gate.has_input_arity(Arity::Many));
        assert!(gate.has_output_arity(Arity::One));
        assert!(!gate.has_input_arity(Arity::One));
        assert!(!gate.has_output_arity(Arity::Many));
    }

    // ── Classification::horizon_terminates substrate pins ─────────────
    //
    // Fail-before-pass-after granularity: [`Classification::horizon_terminates`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the nested [`Horizon`] slot's
    // [`HorizonKind::terminates`] projection had no substrate owner.
    // Post-lift the shape lives at ONE substrate primitive and every
    // downstream (the `terminating-horizon` fixed tag in
    // `tatara-check`, the [`crate::ephemeral::EphemeralSpec::horizon_terminates`]
    // peer, future scheduler / termination-shape validators) composes
    // against the SAME `horizon_terminates()` shape rather than
    // restating the `classification.horizon.kind.terminates()` chain
    // at its own callsite.

    /// PER-VARIANT pin — for every [`HorizonKind`] variant, a
    /// [`Classification`] whose `horizon.kind` field carries that
    /// variant returns `horizon_terminates()` matching the closed
    /// set's own [`HorizonKind::terminates`] truth table. Sweep
    /// [`HorizonKind::ALL`] so a regression that (a) hard-coded the
    /// method body to a fixed answer (silently returning `true`
    /// regardless of the stored variant, silently rejecting every
    /// Asymptotic Process's scheduler-facing termination check), (b)
    /// inverted the projection (silently promoting Asymptotic to
    /// "terminates"), or (c) crossed the wires with the antisymmetric
    /// partner [`HorizonKind::requires_metric_axes`] fails HERE at
    /// the substrate primitive before drifting through the
    /// `terminating-horizon` fixed tag or the peer ephemeral surface.
    #[test]
    fn classification_horizon_terminates_matches_horizon_kind_projection() {
        for populated in HorizonKind::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.horizon_terminates(),
                populated.terminates(),
                "horizon.kind={populated:?}: horizon_terminates() drift from HorizonKind::terminates()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `horizon: Horizon::default()` whose `kind` field defaults to
    /// [`HorizonKind::Bounded`] via `#[default]`, and
    /// [`HorizonKind::Bounded::terminates`] projects `true`, so
    /// `horizon_terminates()` returns `true`. Pins the default-arm
    /// short-circuit through TWO layers of `Default` (`Horizon`'s +
    /// `HorizonKind`'s) at ONE narrow site — a regression that
    /// promoted [`HorizonKind::Asymptotic`] to `#[default]`, or that
    /// swapped `Horizon::default`'s stored `kind`, or that wired
    /// [`HorizonKind::Bounded`] to `terminates() = false` would fail
    /// HERE before drifting through every unadorned Process's
    /// scheduler-facing termination answer.
    #[test]
    fn classification_gate_compute_horizon_terminates_is_true() {
        let c = Classification::gate_compute();
        assert!(
            c.horizon_terminates(),
            "gate_compute (horizon.kind=Bounded → terminates=true) baseline",
        );
    }

    /// ANTISYMMETRY pin — [`HorizonKind::terminates`] XOR
    /// [`HorizonKind::requires_metric_axes`] holds on every variant
    /// (pinned by `horizon_kind_terminate_xor_requires_metric_axes`
    /// on the closed set itself); this composition-level test walks
    /// the same XOR contract through THIS derived-nullary predicate
    /// to prove the composition is faithful — a
    /// [`Classification`] answering `horizon_terminates() = true`
    /// implies its horizon does NOT require metric axes and vice
    /// versa. Pins the composition-level XOR at ONE narrow site so
    /// a regression that crossed the wires (`horizon_terminates`
    /// silently composed [`HorizonKind::requires_metric_axes`]
    /// instead of [`HorizonKind::terminates`]) surfaces here rather
    /// than at every downstream consumer that trusts the shape.
    #[test]
    fn classification_horizon_terminates_xor_horizon_kind_requires_metric_axes() {
        for kind in HorizonKind::ALL {
            let c = Classification::gate_compute_with_axis(kind);
            assert!(
                c.horizon_terminates() ^ kind.requires_metric_axes(),
                "{kind:?}: horizon_terminates() XOR requires_metric_axes() must hold",
            );
        }
    }

    // ── Classification::horizon_requires_metric_axes substrate pins ──
    //
    // Fail-before-pass-after granularity: [`Classification::horizon_requires_metric_axes`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the nested [`Horizon`] slot's
    // [`HorizonKind::requires_metric_axes`] projection had no substrate
    // owner. Post-lift the shape lives at ONE substrate primitive and
    // every downstream (the `metric-axes-required` fixed tag in
    // `tatara-check`, the [`crate::ephemeral::EphemeralSpec::horizon_requires_metric_axes`]
    // peer, future scheduler / metric-provisioning validators)
    // composes against the SAME `horizon_requires_metric_axes()` shape
    // rather than restating the
    // `classification.horizon.kind.requires_metric_axes()` chain at
    // its own callsite.

    /// PER-VARIANT pin — for every [`HorizonKind`] variant, a
    /// [`Classification`] whose `horizon.kind` field carries that
    /// variant returns `horizon_requires_metric_axes()` matching the
    /// closed set's own [`HorizonKind::requires_metric_axes`] truth
    /// table. Sweep [`HorizonKind::ALL`] so a regression that (a)
    /// hard-coded the method body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with the antisymmetric
    /// partner [`HorizonKind::terminates`] fails HERE at the substrate
    /// primitive before drifting through the `metric-axes-required`
    /// fixed tag or the peer ephemeral surface.
    #[test]
    fn classification_horizon_requires_metric_axes_matches_horizon_kind_projection() {
        for populated in HorizonKind::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.horizon_requires_metric_axes(),
                populated.requires_metric_axes(),
                "horizon.kind={populated:?}: horizon_requires_metric_axes() drift from HorizonKind::requires_metric_axes()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `horizon: Horizon::default()` whose `kind` field defaults to
    /// [`HorizonKind::Bounded`] via `#[default]`, and
    /// [`HorizonKind::Bounded::requires_metric_axes`] projects `false`,
    /// so `horizon_requires_metric_axes()` returns `false`. Pins the
    /// default-arm short-circuit through TWO layers of `Default`
    /// (`Horizon`'s + `HorizonKind`'s) at ONE narrow site — a
    /// regression that promoted [`HorizonKind::Asymptotic`] to
    /// `#[default]`, or that swapped `Horizon::default`'s stored
    /// `kind`, or that wired [`HorizonKind::Bounded`] to
    /// `requires_metric_axes() = true`, would fail HERE before
    /// drifting through every unadorned Process's metric-provisioning
    /// answer. Mirror image of
    /// `classification_gate_compute_horizon_terminates_is_true`.
    #[test]
    fn classification_gate_compute_horizon_requires_metric_axes_is_false() {
        let c = Classification::gate_compute();
        assert!(
            !c.horizon_requires_metric_axes(),
            "gate_compute (horizon.kind=Bounded → requires_metric_axes=false) baseline",
        );
    }

    /// BINARY XOR PARTITION pin — for every [`HorizonKind`] variant,
    /// EXACTLY ONE of [`Classification::horizon_terminates`] and
    /// [`Classification::horizon_requires_metric_axes`] returns `true`
    /// on a [`Classification`] whose `horizon.kind` field carries that
    /// variant. CLOSES the horizon axis into the FULL binary XOR
    /// partition contract sealed on the closed set by
    /// `horizon_kind_terminate_xor_requires_metric_axes` AND now
    /// composed through the parent-composed layer as a substrate-wide
    /// theorem. Binary counterpart of the ternary XOR partitions
    /// sealed on the sibling `point_type` and `substrate` axes by
    /// `classification_point_type_probes_form_three_way_xor_partition_over_all`
    /// and
    /// `classification_substrate_probes_form_three_way_xor_partition_over_all`
    /// — where those axes carve the closed set into THREE disjoint
    /// buckets, the horizon axis carves into TWO. Structural twin of
    /// the calm-axis and data-axis binary XOR partitions
    /// `classification_calm_probes_form_binary_xor_partition_over_all`
    /// and
    /// `classification_data_probes_form_binary_xor_partition_over_all`
    /// on the sibling axes — this pin is the FIFTH (and final)
    /// classification axis to reach the closed XOR partition landmark
    /// on the (parent × derived-nullary-bool) corner, promoting the
    /// axis-closure milestone from a proven-repeatable quadruple
    /// (`point_type` + `substrate` ternary; `calm` + `data` binary)
    /// to a proven-repeatable QUINTUPLE that spans every axis of
    /// [`Classification`]. Rewritten from the earlier binary-XOR-only
    /// form (walked as `a ^ b`) into the canonical bucket-array
    /// `hits == 1` shape shared with the calm/data partitions so
    /// downstream N-ary consumers (audit dispatchers, coverage
    /// checkers) walk every axis through the SAME contract. A
    /// regression that crossed the wires between the two parent-
    /// composed probes (one probe silently composing the wrong
    /// closed-set arm) fails HERE rather than at every downstream
    /// consumer that trusts the two probes partition the horizon
    /// slot into disjoint buckets whose union covers every variant.
    #[test]
    fn classification_horizon_probes_form_binary_xor_partition_over_all() {
        for populated in HorizonKind::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            let buckets = [c.horizon_terminates(), c.horizon_requires_metric_axes()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "horizon.kind={populated:?}: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
            );
        }
    }

    // ── Classification::calm_requires_coordination substrate pins ────
    //
    // Fail-before-pass-after granularity: [`Classification::calm_requires_coordination`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the scalar [`CalmClassification`]
    // slot's [`CalmClassification::requires_coordination`] projection
    // had no substrate owner. Post-lift the shape lives at ONE
    // substrate primitive and every downstream (the
    // `coordination-required` fixed tag in `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::calm_requires_coordination`]
    // peer, future scheduler / coordination-mode validators) composes
    // against the SAME `calm_requires_coordination()` shape rather than
    // restating the `classification.calm.requires_coordination()` chain
    // at its own callsite. THIRD occupant of the (parent × derived-
    // nullary-bool) corner across TWO closed-set axes (`HorizonKind`,
    // `CalmClassification`), pinning the corner as a proven-repeatable
    // primitive shape rather than a single-axis curiosity.

    /// PER-VARIANT pin — for every [`CalmClassification`] variant, a
    /// [`Classification`] whose `calm` field carries that variant
    /// returns `calm_requires_coordination()` matching the closed
    /// set's own [`CalmClassification::requires_coordination`] truth
    /// table. Sweep [`CalmClassification::ALL`] so a regression that
    /// (a) hard-coded the method body to a fixed answer (silently
    /// returning `true` regardless of the stored variant, silently
    /// forcing every Monotone Process onto the Raft coordination path
    /// and eliminating the CALM theorem's practical dividend), (b)
    /// inverted the projection (silently promoting Monotone to
    /// "requires coordination"), or (c) crossed the wires with a
    /// sibling classification-axis probe fails HERE at the substrate
    /// primitive before drifting through the `coordination-required`
    /// fixed tag or the peer ephemeral surface.
    #[test]
    fn classification_calm_requires_coordination_matches_calm_classification_projection() {
        for populated in CalmClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.calm_requires_coordination(),
                populated.requires_coordination(),
                "calm={populated:?}: calm_requires_coordination() drift from CalmClassification::requires_coordination()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `calm: CalmClassification::default()` which defaults to
    /// [`CalmClassification::Monotone`] via `#[default]`, and
    /// [`CalmClassification::Monotone::requires_coordination`] projects
    /// `false`, so `calm_requires_coordination()` returns `false`. Pins
    /// the default-arm short-circuit through ONE layer of `Default`
    /// (`CalmClassification`'s) at ONE narrow site — a regression that
    /// promoted [`CalmClassification::NonMonotone`] to `#[default]`, or
    /// that wired [`CalmClassification::Monotone`] to
    /// `requires_coordination() = true`, would fail HERE before
    /// drifting through every unadorned Process's scheduler-facing
    /// coordination-mode answer. Distinct from the two sibling
    /// `horizon_*` gate-compute-baseline pins by ONE structural
    /// degree: those short-circuit through TWO layers of `Default`
    /// (`Horizon`'s + `HorizonKind`'s); this pin walks ONE layer of
    /// `Default` because [`Classification::calm`] is a scalar rather
    /// than a nested-struct wrapper.
    #[test]
    fn classification_gate_compute_calm_requires_coordination_is_false() {
        let c = Classification::gate_compute();
        assert!(
            !c.calm_requires_coordination(),
            "gate_compute (calm=Monotone → requires_coordination=false) baseline",
        );
    }

    // ── Classification::data_is_regulated substrate pins ─────────────
    //
    // Fail-before-pass-after granularity: [`Classification::data_is_regulated`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the scalar [`DataClassification`]
    // slot's [`DataClassification::is_regulated`] projection had no
    // substrate owner. Post-lift the shape lives at ONE substrate
    // primitive and every downstream (the `data-regulated` fixed tag
    // in `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::data_is_regulated`] peer,
    // future compliance-baseline / regulatory-regime validators)
    // composes against the SAME `data_is_regulated()` shape rather
    // than restating the
    // `classification.data_classification.is_regulated()` chain at
    // its own callsite. FOURTH occupant of the (parent × derived-
    // nullary-bool) corner across THREE closed-set axes
    // (`HorizonKind`, `CalmClassification`, `DataClassification`),
    // pinning the corner as a proven-repeatable primitive shape
    // across the substrate's three defaulted-child classification-
    // axis closed sets rather than a two-axis curiosity. SECOND
    // direct-scalar peer on the corner after
    // [`Self::calm_requires_coordination`] opened the direct-scalar
    // sub-corner variant.

    /// PER-VARIANT pin — for every [`DataClassification`] variant, a
    /// [`Classification`] whose `data_classification` field carries
    /// that variant returns `data_is_regulated()` matching the closed
    /// set's own [`DataClassification::is_regulated`] truth table.
    /// Sweep [`DataClassification::ALL`] so a regression that (a)
    /// hard-coded the method body to a fixed answer (silently
    /// stamping every Process as regulated, silently forcing
    /// compliance-baseline overlays that only apply to PII/PHI/PCI
    /// onto every unadorned Process), (b) inverted the projection
    /// (silently downgrading regulated Pii/Phi/Pci to unregulated),
    /// or (c) crossed the wires with the sibling
    /// [`DataClassification::is_restricted`] projection (which
    /// disagrees on the two `Internal | Confidential` variants) fails
    /// HERE at the substrate primitive before drifting through the
    /// `data-regulated` fixed tag or the peer ephemeral surface.
    #[test]
    fn classification_data_is_regulated_matches_data_classification_projection() {
        for populated in DataClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.data_is_regulated(),
                populated.is_regulated(),
                "data_classification={populated:?}: data_is_regulated() drift from DataClassification::is_regulated()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `data_classification: DataClassification::default()` which
    /// defaults to [`DataClassification::Internal`] via `#[default]`,
    /// and [`DataClassification::Internal::is_regulated`] projects
    /// `false`, so `data_is_regulated()` returns `false`. Pins the
    /// default-arm short-circuit through ONE layer of `Default`
    /// (`DataClassification`'s) at ONE narrow site — a regression
    /// that promoted [`DataClassification::Pii`] (or any other
    /// regulated variant) to `#[default]`, or that wired
    /// [`DataClassification::Internal`] to `is_regulated() = true`,
    /// would fail HERE before drifting through every unadorned
    /// Process's compliance-baseline answer. Byte-for-byte
    /// structural peer of the sibling
    /// `classification_gate_compute_calm_requires_coordination_is_false`
    /// on the classification-data axis — same ONE-layer-of-Default
    /// short-circuit shape distinct from the two horizon-axis
    /// baselines which walk TWO layers of `Default`.
    #[test]
    fn classification_gate_compute_data_is_regulated_is_false() {
        let c = Classification::gate_compute();
        assert!(
            !c.data_is_regulated(),
            "gate_compute (data_classification=Internal → is_regulated=false) baseline",
        );
    }

    // ── Classification::data_is_restricted substrate pins ────────────
    //
    // Fail-before-pass-after granularity: [`Classification::data_is_restricted`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the scalar [`DataClassification`]
    // slot's [`DataClassification::is_restricted`] projection had no
    // substrate owner. Post-lift the shape lives at ONE substrate
    // primitive and every downstream (the `data-restricted` fixed tag
    // in `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::data_is_restricted`] peer,
    // future compliance-baseline / access-control-mandatory validators)
    // composes against the SAME `data_is_restricted()` shape rather
    // than restating the
    // `classification.data_classification.is_restricted()` chain at
    // its own callsite. FIFTH occupant of the (parent × derived-
    // nullary-bool) corner across THREE closed-set axes and the SECOND
    // occupant on the classification-data axis, pinning the axis as
    // a proven-repeatable structural sub-corner across TWO sibling
    // closed-set projections (`is_regulated` / `is_restricted`).
    // THIRD direct-scalar peer on the corner and the FIRST corner peer
    // whose gate-compute baseline projects to `true` rather than
    // `false` (mirror-image of the `Bounded`-default
    // `horizon_terminates` baseline on the nested-struct sub-corner).

    /// PER-VARIANT pin — for every [`DataClassification`] variant, a
    /// [`Classification`] whose `data_classification` field carries
    /// that variant returns `data_is_restricted()` matching the closed
    /// set's own [`DataClassification::is_restricted`] truth table.
    /// Sweep [`DataClassification::ALL`] so a regression that (a)
    /// hard-coded the method body to a fixed answer, (b) inverted the
    /// projection (silently promoting `Public` to restricted), or
    /// (c) crossed the wires with the sibling
    /// [`DataClassification::is_regulated`] projection (which
    /// disagrees on the two `Internal | Confidential` variants) fails
    /// HERE at the substrate primitive before drifting through the
    /// `data-restricted` fixed tag or the peer ephemeral surface.
    #[test]
    fn classification_data_is_restricted_matches_data_classification_projection() {
        for populated in DataClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.data_is_restricted(),
                populated.is_restricted(),
                "data_classification={populated:?}: data_is_restricted() drift from DataClassification::is_restricted()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `data_classification: DataClassification::default()` which
    /// defaults to [`DataClassification::Internal`] via `#[default]`,
    /// and [`DataClassification::Internal::is_restricted`] projects
    /// `true`, so `data_is_restricted()` returns `true`. Pins the
    /// default-arm short-circuit through ONE layer of `Default` at
    /// ONE narrow site — a regression that promoted
    /// [`DataClassification::Public`] to `#[default]`, or that wired
    /// [`DataClassification::Internal`] to `is_restricted() = false`,
    /// would fail HERE before drifting through every unadorned
    /// Process's access-control-mandatory answer. FIRST direct-scalar
    /// corner peer whose gate-compute baseline projects to `true`
    /// (`data_is_regulated` / `calm_requires_coordination` both
    /// project `false` on the same defaulted parent), mirror-image of
    /// the nested-struct sub-corner where
    /// `classification_gate_compute_horizon_terminates_is_true`
    /// pins the `Bounded`-default `true` baseline.
    #[test]
    fn classification_gate_compute_data_is_restricted_is_true() {
        let c = Classification::gate_compute();
        assert!(
            c.data_is_restricted(),
            "gate_compute (data_classification=Internal → is_restricted=true) baseline",
        );
    }

    /// COMPOSED IMPLICATION pin — the substrate-primitive-level
    /// counterpart of
    /// `data_classification_regulated_implies_restricted` at the
    /// [`Classification`] parent site: for every
    /// [`DataClassification`] variant, a [`Classification`] carrying
    /// that variant answers `data_is_regulated() ⇒
    /// data_is_restricted()` — regulated data is by construction
    /// restricted at the parent-composed derived-nullary-boolean
    /// projection, not just at the closed-set primitives. Pins the
    /// implication contract at the SAME substrate site that composes
    /// each side of the pair, so a regression that (a) reversed the
    /// [`Classification::data_is_regulated`] arm, (b) reversed the
    /// [`Classification::data_is_restricted`] arm, or (c) crossed
    /// their wires while the closed-set primitives stayed intact
    /// fails HERE. FIRST corner-peer pair on the workspace-wide
    /// (parent × derived-nullary-bool) corner whose two projections
    /// carry a non-trivial closed-set-internal implication
    /// relationship — a future compliance-baseline auto-selector
    /// binds through the parent-composed contract rather than
    /// restating the closed-set-primitive contract at the callsite.
    #[test]
    fn classification_data_is_regulated_implies_data_is_restricted_over_all() {
        for populated in DataClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert!(
                !c.data_is_regulated() || c.data_is_restricted(),
                "data_classification={populated:?}: data_is_regulated ⇒ data_is_restricted violated",
            );
        }
    }

    // ── Classification::point_is_endomorphic substrate pins ──────────
    //
    // Fail-before-pass-after granularity: [`Classification::point_is_endomorphic`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the scalar [`ConvergencePointType`]
    // slot's [`ConvergencePointType::is_endomorphic`] projection had no
    // substrate owner. Post-lift the shape lives at ONE substrate
    // primitive and every downstream (the `endomorphic-point` fixed
    // tag in `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::point_is_endomorphic`] peer,
    // future DAG composition / edge-cardinality validators) composes
    // against the SAME `point_is_endomorphic()` shape rather than
    // restating the `classification.point_type.is_endomorphic()` chain
    // at its own callsite. SIXTH occupant of the (parent × derived-
    // nullary-bool) corner across FOUR closed-set axes, and the FIRST
    // occupant threading the `point_type` axis, pinning the axis as a
    // proven-repeatable structural sub-corner rather than a horizon /
    // calm / data curiosity. FIRST direct-scalar corner peer whose
    // parent-composed baseline is NOT a substrate-`#[default]` short-
    // circuit — [`ConvergencePointType`] has no `impl Default`, so the
    // [`Classification::gate_compute`] baseline's `false` answer comes
    // from the chosen `point_type: Gate` field rather than a
    // defaulted-chain projection.

    /// PER-VARIANT pin — for every [`ConvergencePointType`] variant, a
    /// [`Classification`] whose `point_type` field carries that variant
    /// returns `point_is_endomorphic()` matching the closed set's own
    /// [`ConvergencePointType::is_endomorphic`] truth table. Sweep
    /// [`ConvergencePointType::ALL`] so a regression that (a)
    /// hard-coded the method body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with a sibling closed-set
    /// projection ([`ConvergencePointType::is_diffusive`] /
    /// [`ConvergencePointType::is_convergent`]) fails HERE at the
    /// substrate primitive before drifting through the
    /// `endomorphic-point` fixed tag or the peer ephemeral surface.
    #[test]
    fn classification_point_is_endomorphic_matches_point_type_projection() {
        for populated in ConvergencePointType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.point_is_endomorphic(),
                populated.is_endomorphic(),
                "point_type={populated:?}: point_is_endomorphic() drift from ConvergencePointType::is_endomorphic()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `point_type: ConvergencePointType::Gate` deliberately (NOT via
    /// `#[default]` — [`ConvergencePointType`] has no `impl Default`),
    /// and [`ConvergencePointType::Gate::is_endomorphic`] projects
    /// `false` (Gate is N→1 convergent, not 1→1 endomorphic), so
    /// `point_is_endomorphic()` returns `false`. Pins the baseline's
    /// chosen-field answer at ONE narrow site — a regression that
    /// promoted [`ConvergencePointType::Transform`] to the gate-compute
    /// baseline (silently retargeting every unadorned Process's
    /// topology bucket), or that wired [`ConvergencePointType::Gate`]
    /// to `is_endomorphic() = true`, would fail HERE before drifting
    /// through every unadorned Process's DAG-composition answer.
    /// FIRST direct-scalar corner peer whose parent-composed baseline
    /// is a chosen-field answer (not a substrate-`#[default]` short-
    /// circuit): distinct from the two `horizon_*` baselines (which
    /// short-circuit through TWO layers of `Default`), the sibling
    /// `calm_requires_coordination` baseline (ONE layer of `Default`),
    /// and the two `data_is_*` baselines (ONE layer of `Default`).
    #[test]
    fn classification_gate_compute_point_is_endomorphic_is_false() {
        let c = Classification::gate_compute();
        assert!(
            !c.point_is_endomorphic(),
            "gate_compute (point_type=Gate → is_endomorphic=false) baseline",
        );
    }

    // ── Classification::point_is_diffusive substrate pins ───────────
    //
    // Fail-before-pass-after granularity: [`Classification::point_is_diffusive`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the scalar [`ConvergencePointType`]
    // slot's [`ConvergencePointType::is_diffusive`] projection had no
    // substrate owner. Post-lift the shape lives at ONE substrate
    // primitive and every downstream (the `diffusive-point` fixed tag
    // in `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::point_is_diffusive`] peer,
    // future DAG composition / edge-cardinality validators) composes
    // against the SAME `point_is_diffusive()` shape. SEVENTH occupant
    // of the (parent × derived-nullary-bool) corner and SECOND
    // occupant threading the `point_type` axis, promoting that axis
    // from a proven-repeatable one-off (endomorphic alone) to a
    // proven-repeatable pair. FIRST corner-peer pair on the
    // `point_type` axis whose two projections carry a non-trivial
    // closed-set-internal MUTEX relationship (`point_is_endomorphic ⇒
    // ¬point_is_diffusive`), distinct from the sibling `data` axis
    // corner-peer pair whose two projections carry a non-trivial
    // implication (`data_is_regulated ⇒ data_is_restricted`).

    /// PER-VARIANT pin — for every [`ConvergencePointType`] variant, a
    /// [`Classification`] whose `point_type` field carries that variant
    /// returns `point_is_diffusive()` matching the closed set's own
    /// [`ConvergencePointType::is_diffusive`] truth table. Sweep
    /// [`ConvergencePointType::ALL`] so a regression that (a)
    /// hard-coded the method body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with a sibling closed-set
    /// projection ([`ConvergencePointType::is_endomorphic`] /
    /// [`ConvergencePointType::is_convergent`]) fails HERE at the
    /// substrate primitive before drifting through the
    /// `diffusive-point` fixed tag or the peer ephemeral surface.
    #[test]
    fn classification_point_is_diffusive_matches_point_type_projection() {
        for populated in ConvergencePointType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.point_is_diffusive(),
                populated.is_diffusive(),
                "point_type={populated:?}: point_is_diffusive() drift from ConvergencePointType::is_diffusive()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `point_type: ConvergencePointType::Gate` deliberately (NOT via
    /// `#[default]` — [`ConvergencePointType`] has no `impl Default`),
    /// and [`ConvergencePointType::Gate::is_diffusive`] projects
    /// `false` (Gate is N→1 convergent, not 1→N diffusive), so
    /// `point_is_diffusive()` returns `false`. Pins the baseline's
    /// chosen-field answer at ONE narrow site — a regression that
    /// promoted [`ConvergencePointType::Fork`] to the gate-compute
    /// baseline, or that wired [`ConvergencePointType::Gate`] to
    /// `is_diffusive() = true`, would fail HERE before drifting
    /// through every unadorned Process's DAG-composition answer.
    /// SECOND direct-scalar corner peer whose parent-composed baseline
    /// is a chosen-field answer (peer of
    /// `classification_gate_compute_point_is_endomorphic_is_false`).
    #[test]
    fn classification_gate_compute_point_is_diffusive_is_false() {
        let c = Classification::gate_compute();
        assert!(
            !c.point_is_diffusive(),
            "gate_compute (point_type=Gate → is_diffusive=false) baseline",
        );
    }

    /// MUTEX pin — [`Classification::point_is_endomorphic`] AND
    /// [`Classification::point_is_diffusive`] are NEVER simultaneously
    /// true for ANY [`ConvergencePointType`] variant, since the closed
    /// set's own `is_endomorphic` / `is_diffusive` / `is_convergent`
    /// triple carves it into THREE disjoint buckets (sealed on the
    /// closed set by
    /// `convergence_point_type_buckets_cover_every_variant`). Sweep
    /// [`ConvergencePointType::ALL`] so a regression that crossed the
    /// wires between the two corner peers at the parent-composed layer
    /// (one probe silently composing the wrong closed-set arm) fails
    /// HERE rather than at every downstream consumer that trusts the
    /// two probes partition the point-type slot into disjoint buckets.
    /// FIRST corner-peer pair on the workspace-wide (parent × derived-
    /// nullary-bool) corner whose two projections carry a non-trivial
    /// closed-set-internal MUTEX relationship (distinct from the
    /// sibling `data`-axis IMPLICATION pair sealed by
    /// `classification_data_is_regulated_implies_data_is_restricted_over_all`
    /// — that pair contains one bucket in another; this pair
    /// disjointly partitions two buckets of a three-way carving).
    /// When [`Classification::point_is_convergent`] lands the mutex
    /// closes into the full three-way XOR partition contract
    /// `point_is_endomorphic ⊕ point_is_diffusive ⊕
    /// point_is_convergent` composed through this corner as a
    /// substrate-wide theorem.
    #[test]
    fn classification_point_is_endomorphic_and_point_is_diffusive_are_mutex_over_all() {
        for populated in ConvergencePointType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert!(
                !(c.point_is_endomorphic() && c.point_is_diffusive()),
                "point_type={populated:?}: point_is_endomorphic AND point_is_diffusive both true (mutex violated)",
            );
        }
    }

    // ── Classification::point_is_convergent substrate pins ──────────
    //
    // Fail-before-pass-after granularity: [`Classification::point_is_convergent`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the scalar [`ConvergencePointType`]
    // slot's [`ConvergencePointType::is_convergent`] projection had no
    // substrate owner. Post-lift the shape lives at ONE substrate
    // primitive and every downstream (the `convergent-point` fixed
    // tag in `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::point_is_convergent`] peer,
    // future DAG composition / edge-cardinality validators) composes
    // against the SAME `point_is_convergent()` shape. EIGHTH occupant
    // of the (parent × derived-nullary-bool) corner and THIRD
    // occupant threading the `point_type` axis, closing the axis into
    // a proven-repeatable three-peer sub-corner. CLOSES the mutex
    // pair [`Self::point_is_endomorphic`] / [`Self::point_is_diffusive`]
    // into the FULL three-way XOR partition contract on the axis.

    /// PER-VARIANT pin — for every [`ConvergencePointType`] variant, a
    /// [`Classification`] whose `point_type` field carries that variant
    /// returns `point_is_convergent()` matching the closed set's own
    /// [`ConvergencePointType::is_convergent`] truth table. Sweep
    /// [`ConvergencePointType::ALL`] so a regression that (a)
    /// hard-coded the method body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with a sibling closed-set
    /// projection ([`ConvergencePointType::is_endomorphic`] /
    /// [`ConvergencePointType::is_diffusive`]) fails HERE at the
    /// substrate primitive before drifting through the
    /// `convergent-point` fixed tag or the peer ephemeral surface.
    #[test]
    fn classification_point_is_convergent_matches_point_type_projection() {
        for populated in ConvergencePointType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.point_is_convergent(),
                populated.is_convergent(),
                "point_type={populated:?}: point_is_convergent() drift from ConvergencePointType::is_convergent()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `point_type: ConvergencePointType::Gate` deliberately (NOT via
    /// `#[default]` — [`ConvergencePointType`] has no `impl Default`),
    /// and [`ConvergencePointType::Gate::is_convergent`] projects
    /// `true` (Gate is the canonical N→1 convergent barrier), so
    /// `point_is_convergent()` returns `true`. Pins the baseline's
    /// chosen-field answer at ONE narrow site — a regression that
    /// promoted [`ConvergencePointType::Transform`] to the gate-compute
    /// baseline (silently retargeting every unadorned Process's
    /// topology bucket), or that wired [`ConvergencePointType::Gate`]
    /// to `is_convergent() = false`, would fail HERE before drifting
    /// through every unadorned Process's DAG-composition answer.
    /// FIRST direct-scalar corner peer whose parent-composed
    /// gate-compute baseline projects `true` — mirror-inverted from
    /// the two sibling `point_is_endomorphic` /
    /// `point_is_diffusive` baselines which both project `false`.
    #[test]
    fn classification_gate_compute_point_is_convergent_is_true() {
        let c = Classification::gate_compute();
        assert!(
            c.point_is_convergent(),
            "gate_compute (point_type=Gate → is_convergent=true) baseline",
        );
    }

    /// THREE-WAY XOR PARTITION pin — for every
    /// [`ConvergencePointType`] variant, EXACTLY ONE of
    /// [`Classification::point_is_endomorphic`],
    /// [`Classification::point_is_diffusive`], and
    /// [`Classification::point_is_convergent`] returns `true` on a
    /// [`Classification`] carrying that variant. Closes the mutex pair
    /// `classification_point_is_endomorphic_and_point_is_diffusive_are_mutex_over_all`
    /// into the FULL ternary XOR partition contract sealed on the
    /// closed set by `convergence_point_type_buckets_cover_every_variant`
    /// AND now composed through the parent-composed layer as a
    /// substrate-wide theorem. Ternary lift of the closed-set XOR
    /// pair `terminates ^ requires_metric_axes` that already composes
    /// through this corner today — where the `horizon` axis carves
    /// its closed set into TWO non-empty buckets, the `point_type`
    /// axis carves into THREE non-empty buckets. A regression that
    /// crossed the wires between any two of the three parent-composed
    /// probes (one probe silently composing the wrong closed-set arm)
    /// fails HERE rather than at every downstream consumer that
    /// trusts the three probes partition the point-type slot into
    /// disjoint buckets whose union covers every variant.
    #[test]
    fn classification_point_type_probes_form_three_way_xor_partition_over_all() {
        for populated in ConvergencePointType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            let buckets = [
                c.point_is_endomorphic(),
                c.point_is_diffusive(),
                c.point_is_convergent(),
            ];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "point_type={populated:?}: probes {buckets:?} — exactly one must be true (three-way XOR partition violated)",
            );
        }
    }

    // ── Classification::substrate_is_resource substrate pins ────────
    //
    // Fail-before-pass-after granularity: [`Classification::substrate_is_resource`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the scalar [`SubstrateType`]
    // slot's [`SubstrateType::is_resource`] projection had no
    // substrate owner. Post-lift the shape lives at ONE substrate
    // primitive and every downstream (the `resource-substrate` fixed
    // tag in `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::substrate_is_resource`]
    // peer, future plane-baseline / compliance-baseline selectors)
    // composes against the SAME `substrate_is_resource()` shape.
    // NINTH occupant of the (parent × derived-nullary-bool) corner
    // and FIRST occupant threading the classification-`substrate`
    // axis — opens the fourth of six classification axes on the
    // corner after `horizon`, `calm`, `data_classification`, and
    // `point_type`.

    /// PER-VARIANT pin — for every [`SubstrateType`] variant, a
    /// [`Classification`] whose `substrate` field carries that
    /// variant returns `substrate_is_resource()` matching the closed
    /// set's own [`SubstrateType::is_resource`] truth table. Sweep
    /// [`SubstrateType::ALL`] so a regression that (a) hard-coded
    /// the method body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with a sibling closed-
    /// set projection ([`SubstrateType::is_policy`] /
    /// [`SubstrateType::is_telemetry`]) fails HERE at the substrate
    /// primitive before drifting through the `resource-substrate`
    /// fixed tag or the peer ephemeral surface.
    #[test]
    fn classification_substrate_is_resource_matches_substrate_projection() {
        for populated in SubstrateType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.substrate_is_resource(),
                populated.is_resource(),
                "substrate={populated:?}: substrate_is_resource() drift from SubstrateType::is_resource()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `substrate: SubstrateType::Compute` deliberately (NOT via
    /// `#[default]` — [`SubstrateType`] has no `impl Default`), and
    /// [`SubstrateType::Compute::is_resource`] projects `true`
    /// (Compute is a resource-plane substrate you allocate budgets
    /// from), so `substrate_is_resource()` returns `true`. Pins the
    /// baseline's chosen-field answer at ONE narrow site — a
    /// regression that promoted [`SubstrateType::Security`] (or any
    /// non-resource plane) to the gate-compute baseline (silently
    /// retargeting every unadorned Process's plane bucket), or that
    /// wired [`SubstrateType::Compute`] to `is_resource() = false`,
    /// would fail HERE before drifting through every unadorned
    /// Process's plane-baseline answer. Structural peer of
    /// `classification_gate_compute_point_is_convergent_is_true`:
    /// both walk direct-scalar chosen fields with `true` baseline
    /// answers (mirror-aligned with each other, mirror-inverted from
    /// the two other `point_type`-axis peers whose baselines answer
    /// `false`).
    #[test]
    fn classification_gate_compute_substrate_is_resource_is_true() {
        let c = Classification::gate_compute();
        assert!(
            c.substrate_is_resource(),
            "gate_compute (substrate=Compute → is_resource=true) baseline",
        );
    }

    // ── Classification::substrate_is_policy substrate pins ──────────
    //
    // Fail-before-pass-after granularity: [`Classification::substrate_is_policy`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the scalar [`SubstrateType`]
    // slot's [`SubstrateType::is_policy`] projection had no
    // substrate owner. Post-lift the shape lives at ONE substrate
    // primitive and every downstream (the `policy-substrate` fixed
    // tag in `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::substrate_is_policy`] peer,
    // future plane-baseline / compliance-baseline selectors)
    // composes against the SAME `substrate_is_policy()` shape.
    // TENTH occupant of the (parent × derived-nullary-bool) corner
    // and SECOND occupant threading the classification-`substrate`
    // axis, promoting that axis from a proven-repeatable one-off
    // (`substrate_is_resource` alone) to a proven-repeatable pair.
    // FIRST substrate-axis corner-peer pair carrying a non-trivial
    // closed-set-internal MUTEX relationship
    // (`substrate_is_resource ⇒ ¬substrate_is_policy`).

    /// PER-VARIANT pin — for every [`SubstrateType`] variant, a
    /// [`Classification`] whose `substrate` field carries that
    /// variant returns `substrate_is_policy()` matching the closed
    /// set's own [`SubstrateType::is_policy`] truth table. Sweep
    /// [`SubstrateType::ALL`] so a regression that (a) hard-coded
    /// the method body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with a sibling closed-
    /// set projection ([`SubstrateType::is_resource`] /
    /// [`SubstrateType::is_telemetry`]) fails HERE at the substrate
    /// primitive before drifting through the `policy-substrate`
    /// fixed tag or the peer ephemeral surface.
    #[test]
    fn classification_substrate_is_policy_matches_substrate_projection() {
        for populated in SubstrateType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.substrate_is_policy(),
                populated.is_policy(),
                "substrate={populated:?}: substrate_is_policy() drift from SubstrateType::is_policy()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `substrate: SubstrateType::Compute` deliberately (NOT via
    /// `#[default]` — [`SubstrateType`] has no `impl Default`), and
    /// [`SubstrateType::Compute::is_policy`] projects `false`
    /// (Compute is a resource-plane substrate, not a policy plane),
    /// so `substrate_is_policy()` returns `false`. Pins the
    /// baseline's chosen-field answer at ONE narrow site — a
    /// regression that promoted [`SubstrateType::Security`] (or any
    /// policy plane) to the gate-compute baseline, or that wired
    /// [`SubstrateType::Compute`] to `is_policy() = true`, would
    /// fail HERE before drifting through every unadorned Process's
    /// plane-baseline answer. Mirror-inverted from the sibling
    /// `classification_gate_compute_substrate_is_resource_is_true`
    /// baseline (both walk the SAME chosen `substrate: Compute`
    /// field, so `is_resource = true` ⇒ `is_policy = false` on the
    /// closed set's disjoint plane partition).
    #[test]
    fn classification_gate_compute_substrate_is_policy_is_false() {
        let c = Classification::gate_compute();
        assert!(
            !c.substrate_is_policy(),
            "gate_compute (substrate=Compute → is_policy=false) baseline",
        );
    }

    /// MUTEX pin — [`Classification::substrate_is_resource`] AND
    /// [`Classification::substrate_is_policy`] are NEVER simultaneously
    /// true for ANY [`SubstrateType`] variant, since the closed set's
    /// own `is_resource` / `is_policy` / `is_telemetry` triple carves
    /// it into THREE disjoint buckets (sealed on the closed set by
    /// `substrate_type_buckets_cover_every_variant`). Sweep
    /// [`SubstrateType::ALL`] so a regression that crossed the wires
    /// between the two corner peers at the parent-composed layer (one
    /// probe silently composing the wrong closed-set arm) fails HERE
    /// rather than at every downstream consumer that trusts the two
    /// probes partition the substrate slot into disjoint buckets.
    /// FIRST substrate-axis corner-peer pair whose two projections
    /// carry a non-trivial closed-set-internal MUTEX relationship —
    /// structural twin of the sibling `point_type`-axis MUTEX pair
    /// sealed by
    /// `classification_point_is_endomorphic_and_point_is_diffusive_are_mutex_over_all`.
    /// When [`Classification::substrate_is_telemetry`] lands the
    /// mutex closes into the full three-way XOR partition contract
    /// `substrate_is_resource ⊕ substrate_is_policy ⊕ substrate_is_telemetry`
    /// composed through this corner as a substrate-wide theorem.
    #[test]
    fn classification_substrate_is_resource_and_substrate_is_policy_are_mutex_over_all() {
        for populated in SubstrateType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert!(
                !(c.substrate_is_resource() && c.substrate_is_policy()),
                "substrate={populated:?}: substrate_is_resource AND substrate_is_policy both true (mutex violated)",
            );
        }
    }

    // ── Classification::substrate_is_telemetry substrate pins ───────
    //
    // Fail-before-pass-after granularity: [`Classification::substrate_is_telemetry`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the scalar [`SubstrateType`]
    // slot's [`SubstrateType::is_telemetry`] projection had no
    // substrate owner. Post-lift the shape lives at ONE substrate
    // primitive and every downstream (the `telemetry-substrate` fixed
    // tag in `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::substrate_is_telemetry`]
    // peer, future plane-baseline / compliance-baseline selectors)
    // composes against the SAME `substrate_is_telemetry()` shape.
    // ELEVENTH occupant of the (parent × derived-nullary-bool) corner
    // and THIRD occupant threading the classification-`substrate`
    // axis — CLOSES the substrate axis on the corner into the FULL
    // three-way XOR partition contract
    // `substrate_is_resource ⊕ substrate_is_policy ⊕ substrate_is_telemetry`
    // sealed on the closed set by
    // `substrate_type_buckets_cover_every_variant` and composed
    // through the parent-composed layer by
    // `classification_substrate_probes_form_three_way_xor_partition_over_all`.

    /// PER-VARIANT pin — for every [`SubstrateType`] variant, a
    /// [`Classification`] whose `substrate` field carries that
    /// variant returns `substrate_is_telemetry()` matching the closed
    /// set's own [`SubstrateType::is_telemetry`] truth table. Sweep
    /// [`SubstrateType::ALL`] so a regression that (a) hard-coded
    /// the method body to a fixed answer, (b) inverted the
    /// projection, or (c) crossed the wires with a sibling closed-
    /// set projection ([`SubstrateType::is_resource`] /
    /// [`SubstrateType::is_policy`]) fails HERE at the substrate
    /// primitive before drifting through the `telemetry-substrate`
    /// fixed tag or the peer ephemeral surface.
    #[test]
    fn classification_substrate_is_telemetry_matches_substrate_projection() {
        for populated in SubstrateType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.substrate_is_telemetry(),
                populated.is_telemetry(),
                "substrate={populated:?}: substrate_is_telemetry() drift from SubstrateType::is_telemetry()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `substrate: SubstrateType::Compute` deliberately (NOT via
    /// `#[default]` — [`SubstrateType`] has no `impl Default`), and
    /// [`SubstrateType::Compute::is_telemetry`] projects `false`
    /// (Compute is a resource-plane substrate, not a telemetry
    /// plane), so `substrate_is_telemetry()` returns `false`. Pins
    /// the baseline's chosen-field answer at ONE narrow site — a
    /// regression that promoted [`SubstrateType::Observability`] to
    /// the gate-compute baseline, or that wired
    /// [`SubstrateType::Compute`] to `is_telemetry() = true`, would
    /// fail HERE before drifting through every unadorned Process's
    /// plane-baseline answer. Mirror-inverted from the sibling
    /// `classification_gate_compute_substrate_is_resource_is_true`
    /// baseline (both walk the SAME chosen `substrate: Compute`
    /// field, so `is_resource = true` ⇒ `is_telemetry = false` on the
    /// closed set's disjoint plane partition), aligned with the
    /// sibling `substrate_is_policy` baseline's `false`.
    #[test]
    fn classification_gate_compute_substrate_is_telemetry_is_false() {
        let c = Classification::gate_compute();
        assert!(
            !c.substrate_is_telemetry(),
            "gate_compute (substrate=Compute → is_telemetry=false) baseline",
        );
    }

    /// MUTEX pin — [`Classification::substrate_is_resource`] AND
    /// [`Classification::substrate_is_telemetry`] are NEVER
    /// simultaneously true for ANY [`SubstrateType`] variant, since
    /// the closed set's own `is_resource` / `is_policy` /
    /// `is_telemetry` triple carves it into THREE disjoint buckets
    /// (sealed on the closed set by
    /// `substrate_type_buckets_cover_every_variant`). Second
    /// substrate-axis corner-peer MUTEX pin — peer of
    /// `classification_substrate_is_resource_and_substrate_is_policy_are_mutex_over_all`
    /// on a sibling closed-set projection.
    #[test]
    fn classification_substrate_is_resource_and_substrate_is_telemetry_are_mutex_over_all() {
        for populated in SubstrateType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert!(
                !(c.substrate_is_resource() && c.substrate_is_telemetry()),
                "substrate={populated:?}: substrate_is_resource AND substrate_is_telemetry both true (mutex violated)",
            );
        }
    }

    /// MUTEX pin — [`Classification::substrate_is_policy`] AND
    /// [`Classification::substrate_is_telemetry`] are NEVER
    /// simultaneously true for ANY [`SubstrateType`] variant. Third
    /// substrate-axis corner-peer MUTEX pin — completes the three
    /// pairwise MUTEX relations on the substrate axis alongside
    /// `classification_substrate_is_resource_and_substrate_is_policy_are_mutex_over_all`
    /// and
    /// `classification_substrate_is_resource_and_substrate_is_telemetry_are_mutex_over_all`.
    #[test]
    fn classification_substrate_is_policy_and_substrate_is_telemetry_are_mutex_over_all() {
        for populated in SubstrateType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert!(
                !(c.substrate_is_policy() && c.substrate_is_telemetry()),
                "substrate={populated:?}: substrate_is_policy AND substrate_is_telemetry both true (mutex violated)",
            );
        }
    }

    /// THREE-WAY XOR PARTITION pin — for every [`SubstrateType`]
    /// variant, EXACTLY ONE of
    /// [`Classification::substrate_is_resource`],
    /// [`Classification::substrate_is_policy`], and
    /// [`Classification::substrate_is_telemetry`] returns `true` on
    /// a [`Classification`] carrying that variant. CLOSES the three
    /// pairwise MUTEX pins on the substrate axis
    /// (`substrate_is_resource ⇒ ¬substrate_is_policy`,
    /// `substrate_is_resource ⇒ ¬substrate_is_telemetry`,
    /// `substrate_is_policy ⇒ ¬substrate_is_telemetry`) into the FULL
    /// ternary XOR partition contract sealed on the closed set by
    /// `substrate_type_buckets_cover_every_variant` AND now composed
    /// through the parent-composed layer as a substrate-wide theorem.
    /// Structural twin of the sibling `point_type`-axis ternary lift
    /// sealed on this surface by
    /// `classification_point_type_probes_form_three_way_xor_partition_over_all`.
    /// A regression that crossed the wires between any two of the
    /// three parent-composed probes (one probe silently composing the
    /// wrong closed-set arm) fails HERE rather than at every
    /// downstream consumer that trusts the three probes partition the
    /// substrate slot into disjoint buckets whose union covers every
    /// variant.
    #[test]
    fn classification_substrate_probes_form_three_way_xor_partition_over_all() {
        for populated in SubstrateType::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            let buckets = [
                c.substrate_is_resource(),
                c.substrate_is_policy(),
                c.substrate_is_telemetry(),
            ];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "substrate={populated:?}: probes {buckets:?} — exactly one must be true (three-way XOR partition violated)",
            );
        }
    }

    // ── Classification::calm_is_monotone substrate pins ─────────────
    //
    // Fail-before-pass-after granularity: [`Classification::calm_is_monotone`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the scalar [`CalmClassification`]
    // slot's [`CalmClassification::is_monotone`] projection had no
    // substrate owner. Post-lift the shape lives at ONE substrate
    // primitive and every downstream (the `monotone-calm` fixed tag
    // in `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::calm_is_monotone`] peer,
    // future scheduler / gossip-eligibility validators asking the
    // positive CALM framing) composes against the SAME
    // `calm_is_monotone()` shape rather than restating either
    // `!self.calm_requires_coordination()` or the
    // `self.calm.is_monotone()` chain at its own callsite. TWELFTH
    // occupant of the (parent × derived-nullary-bool) corner and
    // SECOND occupant threading the classification-`calm` axis —
    // CLOSES the calm axis on the corner into the FULL binary XOR
    // partition contract `calm_is_monotone ⊕ calm_requires_coordination`
    // sealed on the closed set by
    // `calm_classification_monotone_xor_requires_coordination` and
    // composed through the parent-composed layer by
    // `classification_calm_probes_form_binary_xor_partition_over_all`.

    /// PER-VARIANT pin — for every [`CalmClassification`] variant, a
    /// [`Classification`] whose `calm` field carries that variant
    /// returns `calm_is_monotone()` matching the closed set's own
    /// [`CalmClassification::is_monotone`] truth table. Sweep
    /// [`CalmClassification::ALL`] so a regression that (a) hard-
    /// coded the method body to a fixed answer (silently returning
    /// `true` regardless of the stored variant, silently marking
    /// every Process as gossip-eligible and shipping non-monotone
    /// operations onto the no-coordination path), (b) inverted the
    /// projection (silently promoting NonMonotone to "monotone"),
    /// or (c) crossed the wires with a sibling classification-axis
    /// probe fails HERE at the substrate primitive before drifting
    /// through the `monotone-calm` fixed tag or the peer ephemeral
    /// surface.
    #[test]
    fn classification_calm_is_monotone_matches_calm_classification_projection() {
        for populated in CalmClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.calm_is_monotone(),
                populated.is_monotone(),
                "calm={populated:?}: calm_is_monotone() drift from CalmClassification::is_monotone()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `calm: CalmClassification::default()` which defaults to
    /// [`CalmClassification::Monotone`] via `#[default]`, and
    /// [`CalmClassification::Monotone::is_monotone`] projects `true`,
    /// so `calm_is_monotone()` returns `true`. Pins the default-arm
    /// short-circuit through ONE layer of `Default`
    /// (`CalmClassification`'s) at ONE narrow site — a regression
    /// that promoted [`CalmClassification::NonMonotone`] to
    /// `#[default]`, or that wired [`CalmClassification::Monotone`]
    /// to `is_monotone() = false`, would fail HERE before drifting
    /// through every unadorned Process's positive-CALM-framing
    /// answer. Mirror-inverted from the sibling
    /// `classification_gate_compute_calm_requires_coordination_is_false`
    /// baseline (both walk the SAME defaulted `calm` field, so
    /// `requires_coordination = false` ⇒ `is_monotone = true` on the
    /// closed set's disjoint XOR partition).
    #[test]
    fn classification_gate_compute_calm_is_monotone_is_true() {
        let c = Classification::gate_compute();
        assert!(
            c.calm_is_monotone(),
            "gate_compute (calm=Monotone → is_monotone=true) baseline",
        );
    }

    /// MUTEX pin — [`Classification::calm_requires_coordination`] AND
    /// [`Classification::calm_is_monotone`] are NEVER simultaneously
    /// true for ANY [`CalmClassification`] variant, since the closed
    /// set's own `is_monotone` / `requires_coordination` pair carves
    /// it into TWO disjoint buckets sealed on the closed set by
    /// `calm_classification_monotone_xor_requires_coordination`.
    /// FIRST calm-axis corner-peer MUTEX pin — the calm axis's
    /// counterpart to the sibling substrate-axis
    /// `classification_substrate_is_resource_and_substrate_is_policy_are_mutex_over_all`
    /// on a binary (rather than ternary) closed set.
    #[test]
    fn classification_calm_requires_coordination_and_calm_is_monotone_are_mutex_over_all() {
        for populated in CalmClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert!(
                !(c.calm_requires_coordination() && c.calm_is_monotone()),
                "calm={populated:?}: calm_requires_coordination AND calm_is_monotone both true (mutex violated)",
            );
        }
    }

    /// BINARY XOR PARTITION pin — for every [`CalmClassification`]
    /// variant, EXACTLY ONE of
    /// [`Classification::calm_is_monotone`] and
    /// [`Classification::calm_requires_coordination`] returns `true`
    /// on a [`Classification`] carrying that variant. CLOSES the
    /// calm-axis MUTEX pin
    /// (`calm_requires_coordination ⇒ ¬calm_is_monotone`) into the
    /// FULL binary XOR partition contract sealed on the closed set
    /// by `calm_classification_monotone_xor_requires_coordination`
    /// AND now composed through the parent-composed layer as a
    /// substrate-wide theorem. Binary counterpart of the ternary XOR
    /// partitions sealed on the sibling `point_type` and `substrate`
    /// axes by
    /// `classification_point_type_probes_form_three_way_xor_partition_over_all`
    /// and
    /// `classification_substrate_probes_form_three_way_xor_partition_over_all`
    /// — where those axes carve the closed set into THREE disjoint
    /// buckets, the calm axis carves into TWO. Structural twin of
    /// the closed-set-layer binary XOR
    /// `horizon_kind_terminate_xor_requires_metric_axes` on the
    /// sibling horizon axis, lifted through the parent-composed
    /// layer to make the calm axis the THIRD classification axis to
    /// reach a closed XOR partition landmark on this corner. A
    /// regression that crossed the wires between the two parent-
    /// composed probes (one probe silently composing the wrong
    /// closed-set arm) fails HERE rather than at every downstream
    /// consumer that trusts the two probes partition the calm slot
    /// into disjoint buckets whose union covers every variant.
    #[test]
    fn classification_calm_probes_form_binary_xor_partition_over_all() {
        for populated in CalmClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            let buckets = [c.calm_is_monotone(), c.calm_requires_coordination()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "calm={populated:?}: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
            );
        }
    }

    // ── Classification::data_is_public substrate pins ───────────────
    //
    // Fail-before-pass-after granularity: [`Classification::data_is_public`]
    // did not exist before this commit — the `(Classification) -> bool`
    // derived-nullary-boolean walk over the scalar [`DataClassification`]
    // slot's [`DataClassification::is_public`] projection had no
    // substrate owner. Post-lift the shape lives at ONE substrate
    // primitive and every downstream (the `public-data` fixed tag
    // in `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::data_is_public`] peer,
    // future compliance-baseline / audit-log-optional validators
    // reading the positive distribution framing) composes against the
    // SAME `data_is_public()` shape rather than restating either
    // `!self.data_is_restricted()` or the `self.data_classification.is_public()`
    // chain at its own callsite. THIRTEENTH occupant of the (parent ×
    // derived-nullary-bool) corner and THIRD occupant threading the
    // classification-`data_classification` axis — CLOSES the data axis
    // on the corner into the FULL binary XOR partition contract
    // `data_is_public ⊕ data_is_restricted` sealed on the closed set
    // by `data_classification_public_xor_restricted` and composed
    // through the parent-composed layer by
    // `classification_data_probes_form_binary_xor_partition_over_all`.

    /// PER-VARIANT pin — for every [`DataClassification`] variant, a
    /// [`Classification`] whose `data_classification` field carries
    /// that variant returns `data_is_public()` matching the closed
    /// set's own [`DataClassification::is_public`] truth table. Sweep
    /// [`DataClassification::ALL`] so a regression that (a) hard-
    /// coded the method body to a fixed answer (silently returning
    /// `true` regardless of the stored variant, silently promoting
    /// every dataset onto the freely-distributable path and shipping
    /// PII/PHI/PCI content past every compliance gate), (b) inverted
    /// the projection (silently demoting `Public` to access-controlled),
    /// or (c) crossed the wires with a sibling classification-axis
    /// probe fails HERE at the substrate primitive before drifting
    /// through the `public-data` fixed tag or the peer ephemeral
    /// surface.
    #[test]
    fn classification_data_is_public_matches_data_classification_projection() {
        for populated in DataClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.data_is_public(),
                populated.is_public(),
                "data_classification={populated:?}: data_is_public() drift from DataClassification::is_public()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `data_classification: DataClassification::default()` which
    /// defaults to [`DataClassification::Internal`] via `#[default]`,
    /// and [`DataClassification::Internal::is_public`] projects
    /// `false`, so `data_is_public()` returns `false`. Pins the
    /// default-arm short-circuit through ONE layer of `Default`
    /// ([`DataClassification`]'s) at ONE narrow site — a regression
    /// that promoted [`DataClassification::Public`] to `#[default]`
    /// (silently ballooning the workspace's default compliance
    /// posture from access-controlled to publicly-distributable), or
    /// that wired [`DataClassification::Internal`] to
    /// `is_public() = true`, would fail HERE before drifting through
    /// every unadorned Process's positive-distribution-framing
    /// answer. Mirror-inverted from the sibling
    /// `classification_gate_compute_data_is_restricted_is_true`
    /// baseline (both walk the SAME defaulted `data_classification`
    /// field, so `is_restricted = true` ⇒ `is_public = false` on the
    /// closed set's disjoint XOR partition).
    #[test]
    fn classification_gate_compute_data_is_public_is_false() {
        let c = Classification::gate_compute();
        assert!(
            !c.data_is_public(),
            "gate_compute (data_classification=Internal → is_public=false) baseline",
        );
    }

    /// MUTEX pin — [`Classification::data_is_regulated`] AND
    /// [`Classification::data_is_public`] are NEVER simultaneously
    /// true for ANY [`DataClassification`] variant, since the closed
    /// set's own `is_public` / `is_regulated` pair carves it into
    /// disjoint buckets sealed by
    /// `data_classification_regulated_implies_not_public`. FIRST
    /// substrate-composed antisymmetric MUTEX pin against the
    /// positive-distribution framing: complementary to
    /// `data_is_regulated ⇒ data_is_restricted` on the sibling
    /// projection, this seals `data_is_regulated ⇒ ¬data_is_public`
    /// at the parent-composed layer, closing the closed-set-internal
    /// implication into the substrate primitive's own contract.
    #[test]
    fn classification_data_is_regulated_and_data_is_public_are_mutex_over_all() {
        for populated in DataClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert!(
                !(c.data_is_regulated() && c.data_is_public()),
                "data_classification={populated:?}: data_is_regulated AND data_is_public both true (mutex violated)",
            );
        }
    }

    /// BINARY XOR PARTITION pin — for every [`DataClassification`]
    /// variant, EXACTLY ONE of [`Classification::data_is_public`] and
    /// [`Classification::data_is_restricted`] returns `true` on a
    /// [`Classification`] carrying that variant. CLOSES the data-axis
    /// MUTEX pin (`data_is_regulated ⇒ ¬data_is_public`) into the
    /// FULL binary XOR partition contract sealed on the closed set
    /// by `data_classification_public_xor_restricted` AND now
    /// composed through the parent-composed layer as a substrate-wide
    /// theorem. Binary counterpart of the ternary XOR partitions
    /// sealed on the sibling `point_type` and `substrate` axes by
    /// `classification_point_type_probes_form_three_way_xor_partition_over_all`
    /// and
    /// `classification_substrate_probes_form_three_way_xor_partition_over_all`
    /// — where those axes carve the closed set into THREE disjoint
    /// buckets, the data axis carves into TWO. Structural twin of
    /// the calm-axis binary XOR partition
    /// `classification_calm_probes_form_binary_xor_partition_over_all`
    /// on the sibling calm axis — both bind a two-bucket (positive-
    /// framing/negative-framing) closed-set partition through the
    /// parent-composed layer. A regression that crossed the wires
    /// between the two parent-composed probes (one probe silently
    /// composing the wrong closed-set arm) fails HERE rather than at
    /// every downstream consumer that trusts the two probes partition
    /// the data slot into disjoint buckets whose union covers every
    /// variant.
    #[test]
    fn classification_data_probes_form_binary_xor_partition_over_all() {
        for populated in DataClassification::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            let buckets = [c.data_is_public(), c.data_is_restricted()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "data_classification={populated:?}: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
            );
        }
    }

    // ── Classification::direction_prefers_lower substrate pins ──────
    //
    // Fail-before-pass-after granularity:
    // [`Classification::direction_prefers_lower`] did not exist before
    // this commit — the `(Classification) -> bool` derived-nullary-
    // boolean walk over the [`Horizon::direction`] slot's
    // [`OptimizationDirection::prefers_lower`] projection had no
    // substrate owner. Post-lift the shape lives at ONE substrate
    // primitive and every downstream (the `prefers-lower-direction`
    // fixed tag in `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::direction_prefers_lower`]
    // peer, future asymptotic-health rate-window / regression-detector
    // evaluators keying on the optimization-polarity) composes against
    // the SAME `direction_prefers_lower()` shape rather than restating
    // the `self.horizon.direction.unwrap_or_default().prefers_lower()`
    // chain at its own callsite. FOURTEENTH occupant of the (parent ×
    // derived-nullary-bool) corner and FIRST occupant threading the
    // classification-`horizon.direction` axis — opens the SIXTH
    // classification axis into the fixed-tag algebra.

    /// PER-VARIANT pin — for every [`OptimizationDirection`] variant, a
    /// [`Classification`] whose `horizon.direction` slot carries
    /// `Some(variant)` returns `direction_prefers_lower()` matching
    /// the closed set's own [`OptimizationDirection::prefers_lower`]
    /// truth table. Sweep [`OptimizationDirection::ALL`] so a regression
    /// that (a) hard-coded the method body to a fixed answer (silently
    /// returning `true` regardless of the stored variant, silently
    /// keeping every Process on the lower-is-better path and inverting
    /// every rate-window evaluator that expected Maximize polarity),
    /// (b) inverted the projection (silently promoting `Maximize` to
    /// "prefers lower"), (c) dropped the `.unwrap_or_default()` hop
    /// (defaulting a `None` slot to a fixed `false` rather than the
    /// closed-set-level `Minimize.prefers_lower() = true`), or
    /// (d) crossed the wires with a sibling classification-axis probe
    /// fails HERE at the substrate primitive before drifting through
    /// the `prefers-lower-direction` fixed tag or the peer ephemeral
    /// surface.
    #[test]
    fn classification_direction_prefers_lower_matches_optimization_direction_projection() {
        for populated in OptimizationDirection::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.direction_prefers_lower(),
                populated.prefers_lower(),
                "horizon.direction={populated:?}: direction_prefers_lower() drift from OptimizationDirection::prefers_lower()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `horizon: Horizon::default()` whose `direction` field is `None`,
    /// so `self.horizon.direction.unwrap_or_default()` defaults to
    /// [`OptimizationDirection::Minimize`] via `#[default]`, and
    /// [`OptimizationDirection::Minimize::prefers_lower`] projects
    /// `true`, so `direction_prefers_lower()` returns `true`. Pins the
    /// default-arm short-circuit through TWO layers of `Default`
    /// ([`Horizon::default`] → `direction: None`; then
    /// [`OptimizationDirection::default = Minimize`]) at ONE narrow
    /// site — a regression that promoted [`OptimizationDirection::Maximize`]
    /// to `#[default]` (silently flipping every unadorned Process's
    /// rate-window evaluator polarity), that wired `Minimize` to
    /// `prefers_lower() = false`, or that dropped the
    /// `.unwrap_or_default()` hop (silently defaulting `None` to
    /// `false` rather than to the closed-set-level `Minimize` baseline),
    /// would fail HERE before drifting through every unadorned Process's
    /// optimization-polarity answer.
    #[test]
    fn classification_gate_compute_direction_prefers_lower_is_true() {
        let c = Classification::gate_compute();
        assert!(
            c.direction_prefers_lower(),
            "gate_compute (horizon.direction=None → unwrap_or_default=Minimize → prefers_lower=true) baseline",
        );
    }

    // ── Classification::direction_prefers_higher substrate pins ─────
    //
    // Fail-before-pass-after granularity:
    // [`Classification::direction_prefers_higher`] did not exist before
    // this commit — the positive higher-is-better framing peer of
    // [`Classification::direction_prefers_lower`] had no substrate
    // owner. Post-lift the shape lives at ONE substrate primitive and
    // every downstream (the `prefers-higher-direction` fixed tag in
    // `tatara-check`, the
    // [`crate::ephemeral::EphemeralSpec::direction_prefers_higher`]
    // peer, future asymptotic-health rate-window / regression-detector
    // evaluators keying on the higher-is-better polarity) composes
    // against the SAME `direction_prefers_higher()` shape rather than
    // restating either `!self.direction_prefers_lower()` or the
    // `self.horizon.direction.unwrap_or_default().prefers_higher()`
    // chain at its own callsite. FIFTEENTH occupant of the (parent ×
    // derived-nullary-bool) corner and SECOND occupant threading the
    // classification-`horizon.direction` axis — CLOSES the axis on the
    // corner into the FULL binary XOR partition contract
    // `direction_prefers_lower ⊕ direction_prefers_higher` sealed on
    // the closed set by
    // `optimization_direction_prefers_lower_xor_prefers_higher` and
    // composed through the parent-composed layer by
    // `classification_direction_probes_form_binary_xor_partition_over_all`.
    // ALL SIX classification axes (horizon, calm, data, point,
    // substrate, optimization-direction) now have their partitions
    // closed at the corner.

    /// PER-VARIANT pin — for every [`OptimizationDirection`] variant, a
    /// [`Classification`] whose `horizon.direction` slot carries
    /// `Some(variant)` returns `direction_prefers_higher()` matching
    /// the closed set's own [`OptimizationDirection::prefers_higher`]
    /// truth table. Sweep [`OptimizationDirection::ALL`] so a
    /// regression that (a) hard-coded the method body to a fixed
    /// answer (silently returning `false` regardless of the stored
    /// variant, silently keeping every Process on the lower-is-better
    /// path and inverting every rate-window evaluator that expected
    /// Maximize polarity), (b) inverted the projection (silently
    /// promoting `Minimize` to "prefers higher"), (c) dropped the
    /// `.unwrap_or_default()` hop (defaulting a `None` slot to a
    /// fixed `true` rather than the closed-set-level
    /// `Minimize.prefers_higher() = false`), or (d) crossed the wires
    /// with a sibling classification-axis probe fails HERE at the
    /// substrate primitive before drifting through the
    /// `prefers-higher-direction` fixed tag or the peer ephemeral
    /// surface.
    #[test]
    fn classification_direction_prefers_higher_matches_optimization_direction_projection() {
        for populated in OptimizationDirection::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert_eq!(
                c.direction_prefers_higher(),
                populated.prefers_higher(),
                "horizon.direction={populated:?}: direction_prefers_higher() drift from OptimizationDirection::prefers_higher()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] shape carries
    /// `horizon: Horizon::default()` whose `direction` field is `None`,
    /// so `self.horizon.direction.unwrap_or_default()` defaults to
    /// [`OptimizationDirection::Minimize`] via `#[default]`, and
    /// [`OptimizationDirection::Minimize::prefers_higher`] projects
    /// `false`, so `direction_prefers_higher()` returns `false`. Pins
    /// the default-arm short-circuit through TWO layers of `Default`
    /// ([`Horizon::default`] → `direction: None`; then
    /// [`OptimizationDirection::default = Minimize`]) at ONE narrow
    /// site — a regression that promoted [`OptimizationDirection::Maximize`]
    /// to `#[default]` (silently flipping every unadorned Process's
    /// rate-window evaluator polarity onto the higher-is-better path),
    /// that wired `Minimize` to `prefers_higher() = true`, or that
    /// dropped the `.unwrap_or_default()` hop (silently defaulting
    /// `None` to `true` rather than the closed-set-level `Minimize`
    /// baseline) would fail HERE before drifting through every
    /// unadorned Process's optimization-polarity answer. Mirror-
    /// inverted from the sibling
    /// `classification_gate_compute_direction_prefers_lower_is_true`
    /// baseline (both walk the SAME defaulted `horizon.direction`
    /// slot, so `prefers_lower = true` ⇒ `prefers_higher = false` on
    /// the closed set's disjoint XOR partition).
    #[test]
    fn classification_gate_compute_direction_prefers_higher_is_false() {
        let c = Classification::gate_compute();
        assert!(
            !c.direction_prefers_higher(),
            "gate_compute (horizon.direction=None → unwrap_or_default=Minimize → prefers_higher=false) baseline",
        );
    }

    /// MUTEX pin — [`Classification::direction_prefers_lower`] AND
    /// [`Classification::direction_prefers_higher`] are NEVER
    /// simultaneously true for ANY [`OptimizationDirection`] variant,
    /// since the closed set's own `prefers_lower` / `prefers_higher`
    /// pair carves it into disjoint buckets sealed by
    /// `optimization_direction_prefers_lower_xor_prefers_higher`.
    /// Substrate-composed antisymmetric MUTEX pin against the
    /// positive higher-is-better framing peer at the parent-composed
    /// layer.
    #[test]
    fn classification_direction_prefers_lower_and_prefers_higher_are_mutex_over_all() {
        for populated in OptimizationDirection::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            assert!(
                !(c.direction_prefers_lower() && c.direction_prefers_higher()),
                "horizon.direction={populated:?}: direction_prefers_lower AND direction_prefers_higher both true (mutex violated)",
            );
        }
    }

    /// BINARY XOR PARTITION pin — for every [`OptimizationDirection`]
    /// variant, EXACTLY ONE of [`Classification::direction_prefers_lower`]
    /// and [`Classification::direction_prefers_higher`] returns `true`
    /// on a [`Classification`] carrying that variant on its
    /// `horizon.direction` slot. CLOSES the optimization-direction
    /// axis into the FULL binary XOR partition contract sealed on the
    /// closed set by
    /// `optimization_direction_prefers_lower_xor_prefers_higher` and
    /// composed through the parent-composed layer as a substrate-wide
    /// theorem — the SIXTH (and final) classification axis to reach
    /// the parent-composed binary XOR partition landmark at this
    /// corner. Structural twin of the calm-axis binary XOR partition
    /// `classification_calm_probes_form_binary_xor_partition_over_all`
    /// and the data-axis binary XOR partition
    /// `classification_data_probes_form_binary_xor_partition_over_all`
    /// on the sibling calm + data axes — all three binary XOR
    /// partitions publish their two derived-nullary-bool projections
    /// as complementary XOR pairs at ONE site each so the axis carves
    /// into disjoint buckets by construction. A regression that
    /// crossed the wires between the two parent-composed probes (one
    /// probe silently composing the wrong closed-set arm) fails HERE
    /// rather than at every downstream consumer that trusts the two
    /// probes partition the direction slot into disjoint buckets
    /// whose union covers every variant.
    #[test]
    fn classification_direction_probes_form_binary_xor_partition_over_all() {
        for populated in OptimizationDirection::ALL {
            let c = Classification::gate_compute_with_axis(populated);
            let buckets = [c.direction_prefers_lower(), c.direction_prefers_higher()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "horizon.direction={populated:?}: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
            );
        }
    }

    // ── Classification::input_arity_is_one / input_arity_is_many
    //    substrate pins ─────────────────────────────────────────────────
    //
    // Fail-before-pass-after granularity:
    // [`Classification::input_arity_is_one`] and
    // [`Classification::input_arity_is_many`] did not exist before this
    // commit — the input-arity axis, previously reachable only through
    // the parameterized [`Classification::has_input_arity`] probe, had
    // no derived-nullary-bool corner occupant. Post-lift the two
    // shapes live at ONE substrate primitive each and every future
    // downstream (the `single-input-arity` / `multi-input-arity` fixed
    // tags in `tatara-check`, DAG composition validators, an ephemeral
    // surface peer through [`crate::ephemeral::EphemeralSpec::resolved_classification`])
    // composes against the SAME shape rather than restating either
    // `self.point_type.input_arity().is_one()` or
    // `self.has_input_arity(Arity::One)` at its own callsite.
    // SIXTEENTH + SEVENTEENTH occupants of the (parent × derived-
    // nullary-bool) corner and FIRST + SECOND occupants threading the
    // classification-`point_type`-derived input-arity axis — CLOSE the
    // SEVENTH classification axis into the FULL binary XOR partition
    // contract `input_arity_is_one ⊕ input_arity_is_many` sealed on
    // the closed set by `arity_is_one_xor_is_many_over_all` and
    // composed through the parent-composed layer by
    // `classification_input_arity_probes_form_binary_xor_partition_over_all`.

    /// PER-VARIANT pin — for every [`ConvergencePointType`] variant, a
    /// [`Classification`] whose `point_type` slot carries that variant
    /// returns `input_arity_is_one()` matching the closed set's own
    /// [`ConvergencePointType::input_arity`] projection composed with
    /// [`Arity::is_one`]. Sweep [`ConvergencePointType::ALL`] so a
    /// regression that (a) hard-coded the method body to a fixed
    /// answer, (b) inverted the projection (silently promoting the
    /// multi-input variants to "single-input"), (c) crossed the wires
    /// with [`ConvergencePointType::output_arity`] (which disagrees
    /// on the four `Fork | Broadcast | Join | Gate | Select | Reduce`
    /// arms), or (d) dropped the `.is_one()` hop (silently returning
    /// the raw [`Arity`] variant discriminant) fails HERE before
    /// drifting through every future downstream that trusts the
    /// derived-nullary shape.
    #[test]
    fn classification_input_arity_is_one_matches_input_arity_projection() {
        for kind in ConvergencePointType::ALL {
            let mut c = Classification::gate_compute();
            c.point_type = kind;
            assert_eq!(
                c.input_arity_is_one(),
                kind.input_arity().is_one(),
                "point_type={kind:?}: input_arity_is_one() drift from ConvergencePointType::input_arity().is_one()",
            );
        }
    }

    /// PER-VARIANT pin — antisymmetric peer of the sibling
    /// `classification_input_arity_is_one_matches_input_arity_projection`.
    /// For every [`ConvergencePointType`] variant, a
    /// [`Classification`] whose `point_type` slot carries that variant
    /// returns `input_arity_is_many()` matching
    /// [`ConvergencePointType::input_arity`] composed with
    /// [`Arity::is_many`]. Regressions matching the sibling
    /// per-variant pin's shape fail HERE for the multi-input side of
    /// the axis.
    #[test]
    fn classification_input_arity_is_many_matches_input_arity_projection() {
        for kind in ConvergencePointType::ALL {
            let mut c = Classification::gate_compute();
            c.point_type = kind;
            assert_eq!(
                c.input_arity_is_many(),
                kind.input_arity().is_many(),
                "point_type={kind:?}: input_arity_is_many() drift from ConvergencePointType::input_arity().is_many()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] carries `point_type: Gate`;
    /// [`ConvergencePointType::Gate::input_arity`] projects to
    /// [`Arity::Many`], so `input_arity_is_one()` returns `false` on
    /// the baseline. Pins the closed-set-driven default arm at ONE
    /// narrow site — a regression that promoted a different
    /// [`ConvergencePointType`] variant to the workspace-wide
    /// baseline, that flipped `Gate.input_arity()` from `Many` to
    /// `One`, or that inverted the `is_one()` projection would fail
    /// HERE before drifting through every unadorned Process's
    /// input-arity answer.
    #[test]
    fn classification_gate_compute_input_arity_is_one_is_false() {
        let c = Classification::gate_compute();
        assert!(
            !c.input_arity_is_one(),
            "gate_compute (point_type=Gate → input_arity=Many → is_one=false) baseline",
        );
    }

    /// GATE-COMPUTE BASELINE — the antisymmetric mirror of the
    /// sibling `_input_arity_is_one_is_false` pin: `Gate.input_arity()
    /// = Many`, so `input_arity_is_many()` returns `true` on the
    /// baseline. Together with the sibling pin the two seal the
    /// input-arity slot's default-arm answer on the workspace-wide
    /// baseline as a binary XOR partition — a regression breaking
    /// either bucket's default-arm answer fails HERE.
    #[test]
    fn classification_gate_compute_input_arity_is_many_is_true() {
        let c = Classification::gate_compute();
        assert!(
            c.input_arity_is_many(),
            "gate_compute (point_type=Gate → input_arity=Many → is_many=true) baseline",
        );
    }

    /// SUBSTRATE-COMPOSED MUTEX pin — for every
    /// [`ConvergencePointType`] variant, both predicates are NEVER
    /// simultaneously `true` on a [`Classification`] carrying that
    /// variant on its `point_type` slot. A regression that broke the
    /// disjointness (either predicate silently answering `true` for
    /// both single AND multi input variants) fails HERE.
    #[test]
    fn classification_input_arity_is_one_and_is_many_are_mutex_over_all() {
        for kind in ConvergencePointType::ALL {
            let mut c = Classification::gate_compute();
            c.point_type = kind;
            assert!(
                !(c.input_arity_is_one() && c.input_arity_is_many()),
                "point_type={kind:?}: input_arity_is_one AND input_arity_is_many both true (mutex violated)",
            );
        }
    }

    /// BINARY XOR PARTITION pin — for every [`ConvergencePointType`]
    /// variant, EXACTLY ONE of [`Classification::input_arity_is_one`]
    /// and [`Classification::input_arity_is_many`] returns `true` on
    /// a [`Classification`] carrying that variant on its `point_type`
    /// slot. CLOSES the input-arity axis (the SEVENTH classification
    /// axis) into the FULL binary XOR partition contract sealed on
    /// the closed set by `arity_is_one_xor_is_many_over_all` and
    /// composed through the parent-composed layer as a substrate-wide
    /// theorem. Structural twin of the calm-axis binary XOR partition
    /// (`classification_calm_probes_form_binary_xor_partition_over_all`),
    /// the data-axis binary XOR partition
    /// (`classification_data_probes_form_binary_xor_partition_over_all`),
    /// and the optimization-direction-axis binary XOR partition
    /// (`classification_direction_probes_form_binary_xor_partition_over_all`)
    /// on the sibling axes — all four binary XOR partitions publish
    /// their two derived-nullary-bool projections as complementary
    /// XOR pairs at ONE site each so the axis carves into disjoint
    /// buckets by construction. A regression that crossed the wires
    /// between the two parent-composed probes (one probe silently
    /// composing the wrong closed-set arm) fails HERE rather than at
    /// every future downstream consumer that trusts the two probes
    /// partition the input-arity projection into disjoint buckets
    /// whose union covers every [`ConvergencePointType`] variant.
    #[test]
    fn classification_input_arity_probes_form_binary_xor_partition_over_all() {
        for kind in ConvergencePointType::ALL {
            let mut c = Classification::gate_compute();
            c.point_type = kind;
            let buckets = [c.input_arity_is_one(), c.input_arity_is_many()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "point_type={kind:?}: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
            );
        }
    }

    // ── Classification::output_arity_is_one / output_arity_is_many
    //    substrate pins ─────────────────────────────────────────────────
    //
    // Fail-before-pass-after granularity:
    // [`Classification::output_arity_is_one`] and
    // [`Classification::output_arity_is_many`] did not exist before this
    // commit — the output-arity axis, previously reachable only through
    // the parameterized [`Classification::has_output_arity`] probe, had
    // no derived-nullary-bool corner occupant. Post-lift the two shapes
    // live at ONE substrate primitive each and every future downstream
    // (the `single-output-arity` / `multi-output-arity` fixed tags in
    // `tatara-check`, DAG composition validators, an ephemeral surface
    // peer through
    // [`crate::ephemeral::EphemeralSpec::resolved_classification`])
    // composes against the SAME shape rather than restating either
    // `self.point_type.output_arity().is_one()` or
    // `self.has_output_arity(Arity::One)` at its own callsite.
    // EIGHTEENTH + NINETEENTH occupants of the (parent × derived-
    // nullary-bool) corner and FIRST + SECOND occupants threading the
    // classification-`point_type`-derived output-arity axis — CLOSE the
    // EIGHTH classification axis into the FULL binary XOR partition
    // contract `output_arity_is_one ⊕ output_arity_is_many` sealed on
    // the closed set by `arity_is_one_xor_is_many_over_all` and
    // composed through the parent-composed layer by
    // `classification_output_arity_probes_form_binary_xor_partition_over_all`.

    /// PER-VARIANT pin — for every [`ConvergencePointType`] variant, a
    /// [`Classification`] whose `point_type` slot carries that variant
    /// returns `output_arity_is_one()` matching the closed set's own
    /// [`ConvergencePointType::output_arity`] projection composed with
    /// [`Arity::is_one`]. Sweep [`ConvergencePointType::ALL`] so a
    /// regression that (a) hard-coded the method body to a fixed
    /// answer, (b) inverted the projection (silently demoting the
    /// multi-output variants `Fork | Broadcast` into the single-output
    /// bucket), (c) crossed the wires with
    /// [`ConvergencePointType::input_arity`] (which disagrees on the
    /// six `Fork | Broadcast | Join | Gate | Select | Reduce` arms —
    /// six of eight variants), or (d) dropped the `.is_one()` hop
    /// (silently returning the raw [`Arity`] variant discriminant)
    /// fails HERE before drifting through every future downstream
    /// that trusts the derived-nullary shape.
    #[test]
    fn classification_output_arity_is_one_matches_output_arity_projection() {
        for kind in ConvergencePointType::ALL {
            let mut c = Classification::gate_compute();
            c.point_type = kind;
            assert_eq!(
                c.output_arity_is_one(),
                kind.output_arity().is_one(),
                "point_type={kind:?}: output_arity_is_one() drift from ConvergencePointType::output_arity().is_one()",
            );
        }
    }

    /// PER-VARIANT pin — antisymmetric peer of the sibling
    /// `classification_output_arity_is_one_matches_output_arity_projection`.
    /// For every [`ConvergencePointType`] variant, a
    /// [`Classification`] whose `point_type` slot carries that variant
    /// returns `output_arity_is_many()` matching
    /// [`ConvergencePointType::output_arity`] composed with
    /// [`Arity::is_many`]. Regressions matching the sibling per-variant
    /// pin's shape fail HERE for the multi-output side of the axis.
    #[test]
    fn classification_output_arity_is_many_matches_output_arity_projection() {
        for kind in ConvergencePointType::ALL {
            let mut c = Classification::gate_compute();
            c.point_type = kind;
            assert_eq!(
                c.output_arity_is_many(),
                kind.output_arity().is_many(),
                "point_type={kind:?}: output_arity_is_many() drift from ConvergencePointType::output_arity().is_many()",
            );
        }
    }

    /// GATE-COMPUTE BASELINE — the workspace-baseline
    /// [`Classification::gate_compute`] carries `point_type: Gate`;
    /// [`ConvergencePointType::Gate::output_arity`] projects to
    /// [`Arity::One`], so `output_arity_is_one()` returns `true` on
    /// the baseline. Pins the closed-set-driven default arm at ONE
    /// narrow site — a regression that promoted a different
    /// [`ConvergencePointType`] variant to the workspace-wide
    /// baseline, that flipped `Gate.output_arity()` from `One` to
    /// `Many`, or that inverted the `is_one()` projection would fail
    /// HERE before drifting through every unadorned Process's
    /// output-arity answer. NOTE the workspace-baseline answer FLIPS
    /// between the input-arity and output-arity axes on the exact
    /// same baseline: `input_arity_is_one` is `false` on
    /// `gate_compute` (`Gate.input_arity() = Many`), but
    /// `output_arity_is_one` is `true` — direct evidence the two axes
    /// carve the closed set into structurally different partitions.
    #[test]
    fn classification_gate_compute_output_arity_is_one_is_true() {
        let c = Classification::gate_compute();
        assert!(
            c.output_arity_is_one(),
            "gate_compute (point_type=Gate → output_arity=One → is_one=true) baseline",
        );
    }

    /// GATE-COMPUTE BASELINE — the antisymmetric mirror of the sibling
    /// `_output_arity_is_one_is_true` pin: `Gate.output_arity() = One`,
    /// so `output_arity_is_many()` returns `false` on the baseline.
    /// Together with the sibling pin the two seal the output-arity
    /// slot's default-arm answer on the workspace-wide baseline as a
    /// binary XOR partition — a regression breaking either bucket's
    /// default-arm answer fails HERE.
    #[test]
    fn classification_gate_compute_output_arity_is_many_is_false() {
        let c = Classification::gate_compute();
        assert!(
            !c.output_arity_is_many(),
            "gate_compute (point_type=Gate → output_arity=One → is_many=false) baseline",
        );
    }

    /// SUBSTRATE-COMPOSED MUTEX pin — for every
    /// [`ConvergencePointType`] variant, both predicates are NEVER
    /// simultaneously `true` on a [`Classification`] carrying that
    /// variant on its `point_type` slot. A regression that broke the
    /// disjointness (either predicate silently answering `true` for
    /// both single AND multi output variants) fails HERE.
    #[test]
    fn classification_output_arity_is_one_and_is_many_are_mutex_over_all() {
        for kind in ConvergencePointType::ALL {
            let mut c = Classification::gate_compute();
            c.point_type = kind;
            assert!(
                !(c.output_arity_is_one() && c.output_arity_is_many()),
                "point_type={kind:?}: output_arity_is_one AND output_arity_is_many both true (mutex violated)",
            );
        }
    }

    /// BINARY XOR PARTITION pin — for every [`ConvergencePointType`]
    /// variant, EXACTLY ONE of [`Classification::output_arity_is_one`]
    /// and [`Classification::output_arity_is_many`] returns `true` on
    /// a [`Classification`] carrying that variant on its `point_type`
    /// slot. CLOSES the output-arity axis (the EIGHTH classification
    /// axis) into the FULL binary XOR partition contract sealed on
    /// the closed set by `arity_is_one_xor_is_many_over_all` and
    /// composed through the parent-composed layer as a substrate-wide
    /// theorem. Structural twin of the input-arity binary XOR partition
    /// (`classification_input_arity_probes_form_binary_xor_partition_over_all`),
    /// the calm-axis binary XOR partition
    /// (`classification_calm_probes_form_binary_xor_partition_over_all`),
    /// the data-axis binary XOR partition
    /// (`classification_data_probes_form_binary_xor_partition_over_all`),
    /// and the optimization-direction-axis binary XOR partition
    /// (`classification_direction_probes_form_binary_xor_partition_over_all`)
    /// on the sibling axes — all five binary XOR partitions publish
    /// their two derived-nullary-bool projections as complementary
    /// XOR pairs at ONE site each so the axis carves into disjoint
    /// buckets by construction. A regression that crossed the wires
    /// between the two parent-composed probes (one probe silently
    /// composing the wrong closed-set arm) fails HERE rather than at
    /// every future downstream consumer that trusts the two probes
    /// partition the output-arity projection into disjoint buckets
    /// whose union covers every [`ConvergencePointType`] variant.
    #[test]
    fn classification_output_arity_probes_form_binary_xor_partition_over_all() {
        for kind in ConvergencePointType::ALL {
            let mut c = Classification::gate_compute();
            c.point_type = kind;
            let buckets = [c.output_arity_is_one(), c.output_arity_is_many()];
            let hits: u32 = buckets.iter().map(|b| u32::from(*b)).sum();
            assert_eq!(
                hits, 1,
                "point_type={kind:?}: probes {buckets:?} — exactly one must be true (binary XOR partition violated)",
            );
        }
    }

    /// DISTINCTNESS pin — the input-arity and output-arity axes carve
    /// the eight-variant [`ConvergencePointType`] closed set into
    /// DISTINCT partitions. Six of the eight variants disagree between
    /// [`Classification::input_arity_is_one`] and
    /// [`Classification::output_arity_is_one`] (the six non-endomorphic
    /// variants `Fork | Broadcast | Join | Gate | Select | Reduce`
    /// — the four multi-input-single-output arms and the two
    /// single-input-multi-output arms), and only the two endomorphic
    /// variants (`Transform | Observe` — both `(One, One)`) agree.
    /// Pins the axis-distinctness invariant at ONE narrow site — a
    /// regression that (a) collapsed the two axes onto the same
    /// projection (silently reading `output_arity` as an alias for
    /// `input_arity`), (b) copy-pasted `input_arity_is_one`'s body
    /// verbatim onto `output_arity_is_one`, or (c) crossed the
    /// projection wires between the sibling `is_one` / `is_many`
    /// closed-set predicates would fail HERE by shrinking the six-arm
    /// disagreement to zero (identical axes) rather than at every
    /// future downstream that trusts the two axes name distinct
    /// closed-set partitions. Structural anchor for the compounding
    /// insight: opening a SECOND derived-typed-projection axis
    /// (`ConvergencePointType::output_arity`) on the corner is
    /// substantive precisely because it disagrees on 75% of the
    /// closed set with the FIRST derived-typed-projection axis
    /// (`ConvergencePointType::input_arity`).
    #[test]
    fn classification_input_arity_and_output_arity_disagree_on_six_of_eight_variants() {
        let mut disagreements = 0u32;
        for kind in ConvergencePointType::ALL {
            let mut c = Classification::gate_compute();
            c.point_type = kind;
            if c.input_arity_is_one() != c.output_arity_is_one() {
                disagreements += 1;
            }
        }
        assert_eq!(
            disagreements, 6,
            "input_arity_is_one and output_arity_is_one must disagree on exactly six of {} ConvergencePointType variants (the six non-endomorphic arms Fork|Broadcast|Join|Gate|Select|Reduce)",
            ConvergencePointType::ALL.len(),
        );
    }
}
