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
            let c = Classification {
                point_type: populated,
                substrate: SubstrateType::Compute,
                horizon: Horizon::default(),
                calm: CalmClassification::default(),
                data_classification: DataClassification::default(),
            };
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
            let c = Classification {
                point_type: ConvergencePointType::Gate,
                substrate: populated,
                horizon: Horizon::default(),
                calm: CalmClassification::default(),
                data_classification: DataClassification::default(),
            };
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
        let c = Classification {
            point_type: ConvergencePointType::Fork,
            substrate: SubstrateType::Storage,
            horizon: Horizon::default(),
            calm: CalmClassification::default(),
            data_classification: DataClassification::default(),
        };
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
            let c = Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
                horizon: Horizon::default(),
                calm: populated,
                data_classification: DataClassification::default(),
            };
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
        let c = Classification {
            point_type: ConvergencePointType::Fork,
            substrate: SubstrateType::Storage,
            horizon: Horizon::default(),
            calm: CalmClassification::NonMonotone,
            data_classification: DataClassification::default(),
        };
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
            let c = Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
                horizon: Horizon::default(),
                calm: CalmClassification::default(),
                data_classification: populated,
            };
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
        let c = Classification {
            point_type: ConvergencePointType::Fork,
            substrate: SubstrateType::Storage,
            horizon: Horizon::default(),
            calm: CalmClassification::NonMonotone,
            data_classification: DataClassification::Pii,
        };
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
            let c = Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
                horizon: Horizon {
                    kind: populated,
                    ..Horizon::default()
                },
                calm: CalmClassification::default(),
                data_classification: DataClassification::default(),
            };
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
        let c = Classification {
            point_type: ConvergencePointType::Fork,
            substrate: SubstrateType::Storage,
            horizon: Horizon {
                kind: HorizonKind::Asymptotic,
                ..Horizon::default()
            },
            calm: CalmClassification::NonMonotone,
            data_classification: DataClassification::Pii,
        };
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
            let c = Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
                horizon: Horizon {
                    kind: HorizonKind::Asymptotic,
                    direction: Some(populated),
                    ..Horizon::default()
                },
                calm: CalmClassification::default(),
                data_classification: DataClassification::default(),
            };
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
        let c = Classification {
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
            let c = Classification {
                point_type: populated,
                substrate: SubstrateType::Compute,
                horizon: Horizon::default(),
                calm: CalmClassification::default(),
                data_classification: DataClassification::default(),
            };
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
        let c = Classification {
            point_type: ConvergencePointType::Fork,
            substrate: SubstrateType::Compute,
            horizon: Horizon::default(),
            calm: CalmClassification::default(),
            data_classification: DataClassification::default(),
        };
        assert!(c.has_point_type(ConvergencePointType::Fork));
        assert!(c.has_input_arity(Arity::One));
        assert!(!c.has_point_type(ConvergencePointType::Broadcast));
        assert!(!c.has_input_arity(Arity::Many));
    }
}
