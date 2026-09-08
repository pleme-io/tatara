//! Workspace-canonical closed set over the four CRDs `tatara-reconciler`
//! ships (`Process` + `ProcessTable` + `EphemeralPool` +
//! `EphemeralAllocation`) — the ONE substrate site every string-kind→
//! typed-CRD dispatch across the workspace's Rust surface routes through.
//!
//! Pre-lift the (kind-string × CRD-type) dispatch axis lived at TWO
//! peer sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger AND
//! carried a coverage drift between them:
//!
//! * `tatara-reconciler::bin::crd_gen` — the CRD-YAML emitter walked a
//!   4-entry inline array `[Process::crd(), ProcessTable::crd(),
//!   EphemeralPool::crd(), EphemeralAllocation::crd()]` restating each
//!   `<Kind>::crd()` call by hand. Adding a fifth CRD (a future
//!   `EphemeralAllocationBinding`, an `AttestationChain` slot) required
//!   editing THIS array AND remembering both peer dispatchers below.
//! * `tatara-reconciler::bin::tatara-check::check_crd_in_sync` — a
//!   4-arm `match kind { "Process" => …, "ProcessTable" => …,
//!   "EphemeralPool" => …, "EphemeralAllocation" => … }` dispatcher
//!   restating each `serde_yaml::to_string(&<Kind>::crd())` chain by
//!   hand. Same 4-CRD closed set, third peer surface with the SAME
//!   drift risk.
//! * `tatara-reconciler::bin::tatara-check::check_yaml_parses_as` — a
//!   TWO-arm `match kind { "Process" => …, "ProcessTable" => … }`
//!   dispatcher restating the `serde_yaml::from_str::<<Kind>>` chain
//!   by hand — but ONLY for two of the four CRDs. That's the
//!   pre-lift coverage drift: an operator authoring
//!   `(yaml-parses-as EphemeralPool "…")` in `checks.lisp` got
//!   `"unknown kind: EphemeralPool"` at runtime even though the CRD
//!   exists in this workspace.
//!
//! Post-lift the closed set lives at ONE substrate owner here. The
//! three peer dispatchers each route through it:
//! `crd_gen` iterates [`KnownCrd::ALL`] calling [`Self::emit_crd_yaml`];
//! `check_crd_in_sync` reads [`Self::from_kind`] then
//! [`Self::emit_crd_yaml`]; `check_yaml_parses_as` reads
//! [`Self::from_kind`] then [`Self::parse_yaml_as`] — and PICKS UP
//! `EphemeralPool` + `EphemeralAllocation` coverage FOR FREE, closing
//! the pre-lift coverage drift by construction. Adding a fifth CRD
//! lands at ONE variant + one [`Self::ALL`] entry + two match arms
//! ([`Self::as_str`], [`Self::emit_crd_yaml`], [`Self::parse_yaml_as`])
//! and every peer dispatcher picks up the extension mechanically —
//! exhaustively checked by rustc through the closed enum.
//!
//! Peer to `tatara_process::phase::ProcessPhase::ALL` +
//! `tatara_process::boundary::ConditionKind::ALL` +
//! `tatara_process::intent::IntentKind::ALL` +
//! `tatara_process::lifetime::LifetimeKind::ALL` +
//! `tatara_process::receipt::ReceiptKind::ALL` — the substrate-wide
//! pattern of "one closed-set typed enum per axis of dispatch, with
//! `ALL` binding the closed set + `as_str` binding the wire form +
//! `from_str`-shaped decoder + typed per-arm operator". This primitive
//! ports the pattern to the CRD-plane axis where the "arms" are entire
//! CRD types rather than scalar wire kinds.
//!
//! Theory anchor: THEORY.md §III (the typescape — the substrate's own
//! CRDs become a TYPE rather than four `&'static str` literals + four
//! hand-typed `<Kind>::crd()` calls repeated at every dispatcher);
//! THEORY.md §VI.1 (generation over composition — the CRD-kind→
//! CRD-type dispatch shape recurred at THREE hand-authored sites past
//! the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger and is lifted to
//! ONE owner here); THEORY.md §II.1 invariant 5 (composition preserves
//! proofs — a regression that drifted the (kind-string, CRD-type)
//! association at only one dispatcher surfaces at [`tests`] rather
//! than as silent operator-visible skew across the three peer
//! surfaces).

use kube::CustomResourceExt;

use tatara_process::allocation::EphemeralAllocation;
use tatara_process::k8s_wire_identity::K8sWireIdentity;
use tatara_process::pool::EphemeralPool;
use tatara_process::prelude::{Process, ProcessTable};
use tatara_process::API_VERSION;

/// Closed-set discriminator over the four CRDs the workspace ships.
///
/// Every downstream (kind-string × CRD-type) dispatch site routes
/// through [`Self::from_kind`] + one of the typed operators
/// ([`Self::emit_crd_yaml`], [`Self::parse_yaml_as`]) rather than
/// restating a `match kind { "Process" => …, "ProcessTable" => …, … }`
/// chain by hand.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum KnownCrd {
    /// `tatara.pleme.io/v1alpha1` Process — the Unix-process-model
    /// primary resource.
    Process,
    /// `tatara.pleme.io/v1alpha1` ProcessTable — cluster-scoped `/proc`
    /// singleton.
    ProcessTable,
    /// `tatara.pleme.io/v1alpha1` EphemeralPool — pooled ephemeral
    /// allocation source.
    EphemeralPool,
    /// `tatara.pleme.io/v1alpha1` EphemeralAllocation — one allocation
    /// out of an EphemeralPool.
    EphemeralAllocation,
}

impl KnownCrd {
    /// The closed set of CRDs the reconciler binaries dispatch over —
    /// single source of truth. Adding a fifth CRD lands at ONE variant,
    /// ONE `ALL` entry, and three match arms ([`Self::as_str`],
    /// [`Self::emit_crd_yaml`], [`Self::parse_yaml_as`]) — exhaustively
    /// checked by the compiler through the `[Self; N]` array literal
    /// on this const AND the non-fallthrough matches in the operators.
    pub const ALL: [Self; 4] = [
        Self::Process,
        Self::ProcessTable,
        Self::EphemeralPool,
        Self::EphemeralAllocation,
    ];

    /// Canonical Pascal-case wire-format kind — byte-identical to the
    /// `kube_derive`-generated `Kind` associated const on each CRD
    /// type. Pinned by [`tests::as_str_matches_kube_derived_kind_const`]
    /// so a rename lands at ONE arm here rather than at every consumer
    /// hand-composing the string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Process => "Process",
            Self::ProcessTable => "ProcessTable",
            Self::EphemeralPool => "EphemeralPool",
            Self::EphemeralAllocation => "EphemeralAllocation",
        }
    }

    /// K8s wire-form `apiVersion` string for this tatara CRD variant —
    /// invariant across the four variants, all four ride
    /// [`tatara_process::API_VERSION`] because every tatara CRD lives
    /// under the SAME `<GROUP>/<VERSION>` envelope (a workspace-wide
    /// version bump lands at ONE substrate const in `tatara-process`,
    /// not at four per-variant arms here).
    ///
    /// The method exists (rather than a bare associated const) so the
    /// projection surface matches the axis-family peers
    /// ([`tatara_process::k8s_builtin_resource::K8sBuiltinResource::api_version`],
    /// [`tatara_process::flux_resource::FluxResource::api_version`],
    /// [`tatara_process::routing_edge_resource::RoutingEdgeResource::api_version`])
    /// which DO differentiate the apiVersion per variant (`batch/v1` vs
    /// `v1` on the built-in axis, three distinct Flux group/versions on
    /// the FluxCD axis). A caller who imports either any of the three
    /// peer closed sets OR this one reads through the same
    /// `.api_version()` / `.kind()` / `.wire_identity()` method surface
    /// with no per-axis idiom drift.
    #[must_use]
    pub const fn api_version(self) -> &'static str {
        // The four tatara CRDs share ONE `apiVersion` — the workspace-
        // wide `tatara_process::API_VERSION` const. A future version
        // bump (`v1alpha1` → `v1beta1` → `v1`) lands at that ONE
        // substrate const in `tatara-process/src/lib.rs` and every
        // downstream consumer that reads through this projection
        // inherits the upgrade mechanically — no per-arm edit here
        // and no scattered `format!("{GROUP}/{VERSION}")` restatement
        // at the callsites (which the substrate composer
        // `tatara_process::api_version` already collapsed onto the
        // const at the runtime-owned surface).
        let _ = self; // acknowledge the receiver so the invariance is explicit
        API_VERSION
    }

    /// Project this variant onto the typed
    /// [`tatara_process::k8s_wire_identity::K8sWireIdentity`] pair —
    /// the substrate primitive that owns the workspace-wide
    /// `(apiVersion, kind)` pairing every emit or fetch site composes
    /// against. Peer to the sibling `wire_identity` projections on
    /// [`tatara_process::k8s_builtin_resource::K8sBuiltinResource::wire_identity`]
    /// (K8s built-in axis: `Job` / `ConfigMap`),
    /// [`tatara_process::flux_resource::FluxResource::wire_identity`]
    /// (FluxCD axis: `Kustomization` / `HelmRelease` / `OCIRepository`),
    /// and
    /// [`tatara_process::routing_edge_resource::RoutingEdgeResource::wire_identity`]
    /// (routing-edge axis: `Ingress` / `DNSEndpoint`).
    ///
    /// Together the FOUR closed sets partition every K8s wire-form
    /// identity `tatara-reconciler` reaches at run time — with this
    /// projection, the tatara-CRD arm carries the SAME `.wire_identity()`
    /// method surface the three peer axes already do, closing the
    /// axis-family invariant that the pre-existing lone
    /// [`tatara_process::PROCESS_WIRE_IDENTITY`] const opened for just
    /// the `Process` CRD.
    ///
    /// The `Process` arm ties into
    /// [`tatara_process::PROCESS_WIRE_IDENTITY`] by byte-shape parity
    /// (`KnownCrd::Process.wire_identity() == PROCESS_WIRE_IDENTITY`),
    /// pinned at
    /// [`tests::process_wire_identity_matches_workspace_const`]; the
    /// other three arms extend the same shape to the three tatara CRDs
    /// that pre-lift had NO typed wire-form identity primitive at all
    /// (`ProcessTable`, `EphemeralPool`, `EphemeralAllocation`) — every
    /// future SSA fetch, emit, or admission-webhook filter on those
    /// three CRDs routes through this ONE projection rather than
    /// re-composing the pair from
    /// [`tatara_process::API_VERSION`] + a bare `"ProcessTable"` /
    /// `"EphemeralPool"` / `"EphemeralAllocation"` literal.
    ///
    /// `const fn` so a caller can bind the pair into a `const` slot —
    /// a future coherence sweep or a compile-time cache reads the pair
    /// with zero runtime overhead, matching the three peer axes'
    /// `.wire_identity()` posture.
    ///
    /// Theory anchor: THEORY.md §III (the typescape — the tatara-CRD
    /// axis becomes a TYPED wire-form identity peer of the three
    /// pre-existing closed sets rather than a lone const on one variant
    /// plus a bare `.kind()` literal on the other three); THEORY.md
    /// §II.1 invariant 5 (composition preserves proofs — a regression
    /// that drifted the `(apiVersion, kind)` pair at ONE tatara-CRD
    /// emit site would surface at this module's byte-shape pins rather
    /// than as silent operator-visible wire-form skew).
    #[must_use]
    pub const fn wire_identity(self) -> K8sWireIdentity {
        K8sWireIdentity::new(self.api_version(), self.as_str())
    }

    /// Decode a `kind:` string into the typed variant iff it matches
    /// one of the four canonical CRD kinds. Returns `None` for any
    /// operator-supplied unknown kind so callers can surface a targeted
    /// diagnostic (`"unknown CRD kind"` at [`check_crd_in_sync`],
    /// `"unknown kind"` at [`check_yaml_parses_as`]).
    ///
    /// Composes [`Self::ALL`] + [`Self::as_str`] so a rename at
    /// [`Self::as_str`] automatically re-routes the decoder — no
    /// per-arm restatement of the (string, variant) association.
    #[must_use]
    pub fn from_kind(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// Emit this CRD's `CustomResourceDefinition` as YAML — the ONE
    /// substrate site every `serde_yaml::to_string(&<Kind>::crd())`
    /// dispatch across the two reconciler binaries routes through.
    ///
    /// Dispatches on the typed variant to the underlying
    /// [`kube::CustomResourceExt::crd`] projection on the concrete CRD
    /// type. A future normalization (an operator-facing header comment
    /// at every emit, an audit-trail tag stamping the compile
    /// generation, a per-CRD YAML style pass) lands at THIS ONE method
    /// and both `crd_gen` + `check_crd_in_sync` inherit the upgrade
    /// mechanically.
    pub fn emit_crd_yaml(self) -> Result<String, serde_yaml::Error> {
        match self {
            Self::Process => serde_yaml::to_string(&Process::crd()),
            Self::ProcessTable => serde_yaml::to_string(&ProcessTable::crd()),
            Self::EphemeralPool => serde_yaml::to_string(&EphemeralPool::crd()),
            Self::EphemeralAllocation => serde_yaml::to_string(&EphemeralAllocation::crd()),
        }
    }

    /// Parse `src` as a YAML document of this CRD's shape, returning
    /// `Ok(())` on success and the underlying `serde_yaml::Error` on
    /// parse failure — the ONE substrate site every
    /// `serde_yaml::from_str::<<Kind>>(&src).map(drop)` dispatch across
    /// the workspace's `yaml-parses-as` checker routes through.
    ///
    /// Extends coverage from the pre-lift two-arm dispatcher to all
    /// four CRDs by construction: an operator authoring
    /// `(yaml-parses-as EphemeralPool "…")` or
    /// `(yaml-parses-as EphemeralAllocation "…")` in `checks.lisp` now
    /// gets a typed parse rather than a runtime `"unknown kind"`
    /// diagnostic.
    pub fn parse_yaml_as(self, src: &str) -> Result<(), serde_yaml::Error> {
        match self {
            Self::Process => serde_yaml::from_str::<Process>(src).map(drop),
            Self::ProcessTable => serde_yaml::from_str::<ProcessTable>(src).map(drop),
            Self::EphemeralPool => serde_yaml::from_str::<EphemeralPool>(src).map(drop),
            Self::EphemeralAllocation => serde_yaml::from_str::<EphemeralAllocation>(src).map(drop),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_covers_the_four_workspace_crds_in_declaration_order() {
        // Byte-shape parity pin: the closed set MUST enumerate the
        // four CRDs in the SAME order the pre-lift `crd_gen.rs` inline
        // array `[Process::crd(), ProcessTable::crd(),
        // EphemeralPool::crd(), EphemeralAllocation::crd()]` walked.
        // A regression that reordered [`KnownCrd::ALL`] would silently
        // shift `tatara-crd-gen`'s emit order — which the fleet chart's
        // `chart/tatara-reconciler/crds/all.yaml` render depends on
        // for a stable diff at every regenerate. Pin the order at
        // fail-before-pass-after granularity.
        assert_eq!(KnownCrd::ALL.len(), 4);
        assert_eq!(KnownCrd::ALL[0], KnownCrd::Process);
        assert_eq!(KnownCrd::ALL[1], KnownCrd::ProcessTable);
        assert_eq!(KnownCrd::ALL[2], KnownCrd::EphemeralPool);
        assert_eq!(KnownCrd::ALL[3], KnownCrd::EphemeralAllocation);
    }

    #[test]
    fn as_str_matches_kube_resource_kind_projection() {
        // Cross-form coherence pin: the substrate's PascalCase wire
        // form MUST byte-match the `kube::Resource`-derived `kind`
        // projection on each CRD type — the SAME string the CRD's
        // apiVersion+kind envelope stamps AND the SAME string
        // `<Kind>::crd().spec.names.kind` returns for the emitted
        // CRD manifest. A regression that drifted [`KnownCrd::as_str`]
        // would silently break the pre-lift `match kind { "Process" =>
        // … }` dispatchers that routed operator-supplied kind strings
        // from `checks.lisp` into typed CRD dispatch.
        use kube::Resource;
        assert_eq!(KnownCrd::Process.as_str(), Process::kind(&()));
        assert_eq!(KnownCrd::ProcessTable.as_str(), ProcessTable::kind(&()));
        assert_eq!(KnownCrd::EphemeralPool.as_str(), EphemeralPool::kind(&()));
        assert_eq!(
            KnownCrd::EphemeralAllocation.as_str(),
            EphemeralAllocation::kind(&()),
        );
    }

    #[test]
    fn from_kind_round_trips_every_variant_through_as_str() {
        // Closed-set round-trip pin: every variant in [`KnownCrd::ALL`]
        // decodes back through [`KnownCrd::from_kind`] via its own
        // [`KnownCrd::as_str`] projection. A regression that decoupled
        // one direction of the (string, variant) association would
        // fail HERE rather than at the two `check_*` dispatchers.
        for k in KnownCrd::ALL {
            assert_eq!(
                KnownCrd::from_kind(k.as_str()),
                Some(k),
                "round-trip failed for {:?}",
                k
            );
        }
    }

    #[test]
    fn from_kind_returns_none_on_unknown() {
        // Negative pin: every non-canonical kind string produces
        // `None`. Callers use the `None` arm to surface a targeted
        // diagnostic (`"unknown CRD kind"` / `"unknown kind"`) matching
        // the pre-lift `other =>` fallthrough behavior byte-for-byte.
        assert_eq!(KnownCrd::from_kind(""), None);
        assert_eq!(KnownCrd::from_kind("process"), None); // case-sensitive
        assert_eq!(KnownCrd::from_kind("Unknown"), None);
        assert_eq!(KnownCrd::from_kind("EphemeralAllocationBinding"), None);
    }

    #[test]
    fn emit_crd_yaml_produces_a_custom_resource_definition_for_every_variant() {
        // End-to-end pin: [`KnownCrd::emit_crd_yaml`] produces a
        // parse-shaped YAML document whose `kind:` slot is
        // `CustomResourceDefinition` and whose embedded `names.kind`
        // slot names the variant. A regression that lost a match arm
        // (e.g. added a fifth variant + `_ =>` fallthrough without
        // dispatch) fails HERE rather than at the `crd_gen` bin's
        // stdout.
        for k in KnownCrd::ALL {
            let yaml = k.emit_crd_yaml().expect("emit_crd_yaml serializes");
            assert!(
                yaml.contains("kind: CustomResourceDefinition"),
                "{:?}: missing CustomResourceDefinition wire form",
                k
            );
            assert!(
                yaml.contains(k.as_str()),
                "{:?}: emitted CRD YAML must mention its own PascalCase kind ({:?})",
                k,
                k.as_str(),
            );
        }
    }

    #[test]
    fn parse_yaml_as_accepts_the_observability_stack_process_fixture() {
        // Positive pin on the [`KnownCrd::Process`] arm — the
        // `examples/process/observability-stack.yaml` fixture
        // (already parsed by `checks.lisp`'s `(yaml-parses-as Process
        // …)` primitive) parses through [`KnownCrd::parse_yaml_as`]
        // byte-identically to the pre-lift `serde_yaml::from_str::
        // <Process>(&src)` chain. A regression that mis-routed the
        // [`KnownCrd::Process`] arm through a peer variant's
        // deserializer would fail HERE.
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/process/observability-stack.yaml"
        ))
        .expect("observability-stack fixture reads");
        let res = KnownCrd::Process.parse_yaml_as(&src);
        assert!(
            res.is_ok(),
            "Process arm should parse the observability-stack fixture; got err: {:?}",
            res.err()
        );
    }

    #[test]
    fn parse_yaml_as_accepts_the_closed_loop_ephemeral_process_fixture() {
        // Peer to the observability-stack pin — the closed-loop-
        // ephemeral fixture is the SECOND already-parsed Process
        // document under `examples/process/`, exercising the
        // `Lifetime::Ephemeral` + `Intent::Aplicacao` +
        // `ClosedLoopAuth` postcondition slots on the SAME
        // [`KnownCrd::Process`] arm. Pinning both fixtures binds the
        // arm at the shape end-to-end tests bind the pre-lift
        // `check_yaml_parses_as` dispatcher against.
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/process/closed-loop-ephemeral.yaml"
        ))
        .expect("closed-loop-ephemeral fixture reads");
        let res = KnownCrd::Process.parse_yaml_as(&src);
        assert!(
            res.is_ok(),
            "Process arm should parse the closed-loop-ephemeral fixture; got err: {:?}",
            res.err()
        );
    }

    #[test]
    fn parse_yaml_as_rejects_syntactically_invalid_yaml_on_every_variant() {
        // Negative pin sweep: garbage YAML MUST fail parse via the
        // underlying serde_yaml reader for every variant. Sibling to
        // the "unknown kind" fall-through in [`Self::from_kind`]:
        // mistyping the caller's dispatch string yields `None`;
        // feeding a malformed document yields the underlying
        // `serde_yaml::Error` unchanged (which the caller's diagnostic
        // wraps as `"parse: {e}"` at the callsite).
        //
        // Note: the substrate CRDs deliberately DO NOT set
        // `deny_unknown_fields` on their spec structs, so a Process
        // document with extra fields parses tolerantly under peer
        // variants whose specs happen to accept-all-defaults — the
        // `check_yaml_parses_as` primitive's contract is "the
        // document reads as this CRD's shape", not "the document is
        // ONLY this CRD's shape". A future tightening (a
        // `#[serde(deny_unknown_fields)]` sweep, a stricter typed
        // validator) would land at each spec and reach every caller
        // through this ONE arm — a compounding upgrade path.
        let garbage = "not: [valid, yaml,\n: :\n";
        for k in KnownCrd::ALL {
            assert!(
                k.parse_yaml_as(garbage).is_err(),
                "{:?} arm should reject syntactically invalid YAML",
                k
            );
        }
    }

    #[test]
    fn parse_yaml_as_rejects_allocation_shaped_yaml_when_requestor_slot_is_absent() {
        // Semantic negative pin: [`KnownCrd::EphemeralAllocation`] MUST
        // reject a Process-shaped YAML because [`AllocationSpec`]
        // carries a non-defaulted `requestor:` slot the deserializer
        // requires. A regression that mis-routed the arm through a
        // peer variant's tolerant deserializer would silently succeed
        // HERE and produce a false-positive `(yaml-parses-as
        // EphemeralAllocation "…")` pass. This is the strongest
        // per-arm negative pin the substrate's tolerance-by-default
        // wire posture supports.
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/process/observability-stack.yaml"
        ))
        .expect("observability-stack fixture reads");
        assert!(
            KnownCrd::EphemeralAllocation.parse_yaml_as(&src).is_err(),
            "EphemeralAllocation arm rejects the Process fixture because \
             AllocationSpec::requestor has no serde default",
        );
    }

    #[test]
    fn parse_yaml_as_dispatches_by_variant_and_not_by_source_kind_slot() {
        // Coverage-widening pin: every variant of [`KnownCrd`] gets a
        // typed arm on [`Self::parse_yaml_as`] — including
        // [`KnownCrd::ProcessTable`], [`KnownCrd::EphemeralPool`],
        // and [`KnownCrd::EphemeralAllocation`] that the pre-lift
        // `check_yaml_parses_as` dispatcher rejected as
        // `"unknown kind"`. A regression that dropped one of those
        // three arms (e.g. adding a `_ => …` fallthrough that routed
        // them to the Process deserializer) fails HERE via the
        // negative pin above — routing a Process document through
        // an EphemeralPool arm MUST err, not silently succeed. Pins
        // the closed-set dispatch shape at compile-time exhaustiveness
        // (through the non-fallthrough matches on the operators) AND
        // at fail-before-pass-after runtime granularity.
        //
        // The positive-arm pins for the three non-Process variants
        // live at higher-level integration test coverage: a fifth
        // CRD added without an arm here fails compile (through the
        // `[Self; N]` arity on [`Self::ALL`] + the non-fallthrough
        // match); a fifth CRD added without a real fixture reveals
        // itself at first `(yaml-parses-as <NewKind> "…")` invocation
        // in `checks.lisp` rather than as silent skew across the two
        // reconciler binaries.
        for k in KnownCrd::ALL {
            let _ = k.as_str();
            let _ = k.emit_crd_yaml().expect("emit succeeds");
        }
    }

    // ─── KnownCrd::api_version + KnownCrd::wire_identity substrate pins ───
    //
    // Fail-before-pass-after granularity: the two projections did not
    // exist before this commit, so each test below fails to compile
    // pre-lift. Post-lift they collectively pin the tatara-CRD axis'
    // wire-form identity at ONE substrate owner — closing the axis-
    // family invariant the three peer closed sets
    // ([`tatara_process::k8s_builtin_resource::K8sBuiltinResource`],
    // [`tatara_process::flux_resource::FluxResource`],
    // [`tatara_process::routing_edge_resource::RoutingEdgeResource`])
    // opened. A regression that drifted the projection body (a
    // per-arm `api_version` override that broke the workspace-wide
    // invariance, a `wire_identity` slot swap that flipped the
    // `api_version` and `kind` positions, a `const fn` drop that
    // pushed the projection out of compile-time reachability, or a
    // silent re-inlining of the pair from
    // [`tatara_process::API_VERSION`] + a bare literal at a fifth
    // consumer) surfaces HERE rather than as silent operator-visible
    // wire-form skew at the reconciler's SSA emit / fetch surface.

    #[test]
    fn api_version_is_workspace_wide_tatara_v1alpha1_for_every_variant() {
        // Invariance pin — every tatara CRD variant shares the SAME
        // `apiVersion` string because every tatara CRD lives under the
        // workspace-wide `<GROUP>/<VERSION>` envelope owned by
        // [`tatara_process::API_VERSION`]. A regression that split the
        // apiVersion across variants (a per-CRD version override, a
        // hard-coded literal at one arm that missed a future
        // `v1alpha1` → `v1beta1` bump) would silently break the
        // "one substrate const per axis" invariant that
        // [`tatara_process::API_VERSION`] pins for the WHOLE tatara CRD
        // family.
        for k in KnownCrd::ALL {
            assert_eq!(
                k.api_version(),
                tatara_process::API_VERSION,
                "{k:?} apiVersion must route through tatara_process::API_VERSION",
            );
        }
    }

    #[test]
    fn api_version_matches_kube_derive_generated_api_version_for_every_variant() {
        // Cross-form coherence pin — the substrate's `apiVersion`
        // string MUST byte-match the `kube::Resource`-derived
        // `api_version` projection on each CRD type: the SAME string
        // the CRD's `apiVersion:` slot stamps at emit time, and the
        // SAME string the K8s API server dispatches on at fetch time.
        // A regression that drifted [`KnownCrd::api_version`] would
        // silently mis-route SSA-fetch against SSA-apply.
        use kube::Resource;
        assert_eq!(KnownCrd::Process.api_version(), Process::api_version(&()),);
        assert_eq!(
            KnownCrd::ProcessTable.api_version(),
            ProcessTable::api_version(&()),
        );
        assert_eq!(
            KnownCrd::EphemeralPool.api_version(),
            EphemeralPool::api_version(&()),
        );
        assert_eq!(
            KnownCrd::EphemeralAllocation.api_version(),
            EphemeralAllocation::api_version(&()),
        );
    }

    #[test]
    fn wire_identity_pairs_api_version_and_kind_per_variant() {
        // Projection pin: every variant's `wire_identity()` binds the
        // SAME variant's `api_version()` and `as_str()` into a typed
        // pair — the SAME shape the three peer closed sets
        // (K8sBuiltinResource / FluxResource / RoutingEdgeResource)
        // already pin at their sibling `wire_identity_pairs_*` tests.
        // A regression that projected the pair from two different
        // variants (a copy-paste that referenced
        // `KnownCrd::Process.api_version()` alongside
        // `KnownCrd::EphemeralPool.as_str()` at the projection body)
        // would surface here rather than as silent wire-form skew at
        // every reconciler emit or fetch site.
        for k in KnownCrd::ALL {
            let id = k.wire_identity();
            assert_eq!(
                id.api_version,
                k.api_version(),
                "{k:?} wire_identity.api_version must equal k.api_version()",
            );
            assert_eq!(
                id.kind,
                k.as_str(),
                "{k:?} wire_identity.kind must equal k.as_str()",
            );
        }
    }

    #[test]
    fn process_wire_identity_matches_workspace_const() {
        // Cross-substrate coherence pin — the `Process` arm's
        // wire-form identity MUST byte-match the pre-existing lone
        // [`tatara_process::PROCESS_WIRE_IDENTITY`] const, which
        // partitioned the `Process` corner of the axis-family before
        // this commit closed the three-CRD gap. A regression that
        // drifted this arm from the pre-existing const would silently
        // fork the `Process` wire form between callers that reach
        // through this projection and callers that reach through the
        // const (`owner_reference_json` at `tatara-process::lib`,
        // `PROCESS_WIRE_IDENTITY` at every consumer that imports it
        // directly).
        assert_eq!(
            KnownCrd::Process.wire_identity(),
            tatara_process::PROCESS_WIRE_IDENTITY,
            "KnownCrd::Process.wire_identity() must byte-match tatara_process::PROCESS_WIRE_IDENTITY",
        );
    }

    #[test]
    fn wire_identity_is_const_reachable() {
        // Compile-time reachability pin: `wire_identity()` is
        // `const fn` so a caller can bind the pair into a `const`
        // slot at compile time — matching the three peer axes'
        // (`K8sBuiltinResource`, `FluxResource`,
        // `RoutingEdgeResource`) own `const fn` pins. A regression
        // that dropped the `const` qualifier would fail-loudly here
        // rather than as a wrong-slot runtime dispatch at every future
        // callsite.
        const P: K8sWireIdentity = KnownCrd::Process.wire_identity();
        const PT: K8sWireIdentity = KnownCrd::ProcessTable.wire_identity();
        const EP: K8sWireIdentity = KnownCrd::EphemeralPool.wire_identity();
        const EA: K8sWireIdentity = KnownCrd::EphemeralAllocation.wire_identity();
        assert_eq!(P.api_version, tatara_process::API_VERSION);
        assert_eq!(P.kind, "Process");
        assert_eq!(PT.kind, "ProcessTable");
        assert_eq!(EP.kind, "EphemeralPool");
        assert_eq!(EA.kind, "EphemeralAllocation");
    }

    #[test]
    fn wire_identity_kind_slots_are_pairwise_distinct_across_the_closed_set() {
        // Cross-variant coherence pin: no two variants may share a
        // wire-form pair. Peer to
        // [`tatara_process::k8s_builtin_resource::tests::every_variants_api_version_and_kind_are_distinct_across_the_closed_set`]
        // on the K8s-builtin axis — the same invariant carries to the
        // tatara-CRD axis: a future variant addition that copy-pasted
        // an existing kind (a hypothetical `EphemeralPoolBinding` that
        // typo'd back to `"EphemeralPool"`) would surface here rather
        // than as silent operator-visible cross-axis ambiguity at
        // every dispatch.
        use std::collections::HashSet;
        let mut kinds = HashSet::new();
        for k in KnownCrd::ALL {
            let id = k.wire_identity();
            assert!(
                kinds.insert(id.kind),
                "duplicate wire-form kind at {k:?}: {}",
                id.kind
            );
        }
    }

    #[test]
    fn wire_identity_is_disjoint_from_every_peer_wire_form_axis() {
        // Cross-substrate coherence pin — the tatara-CRD axis must not
        // overlap with the K8s-builtin axis (`Job` / `ConfigMap`), the
        // FluxCD axis (`Kustomization` / `HelmRelease` /
        // `OCIRepository`), or the routing-edge axis
        // (`Ingress` / `DNSEndpoint`) at the wire. Peer to
        // [`tatara_process::k8s_builtin_resource::tests::peers_do_not_share_wire_form_pairs_with_flux_resource_or_routing_edge_resource`]
        // — this pin closes the axis-family cross-check symmetrically
        // from the tatara-CRD side so a hypothetical future overlap
        // (a `K8sBuiltinResource::Process` copy-paste, a Flux-side
        // `Process` variant naming collision) surfaces HERE rather
        // than as silent cross-axis ambiguity at every reconciler
        // dispatch.
        use tatara_process::flux_resource::FluxResource;
        use tatara_process::k8s_builtin_resource::K8sBuiltinResource;
        use tatara_process::routing_edge_resource::RoutingEdgeResource;
        for k in KnownCrd::ALL {
            let id = k.wire_identity();
            for b in K8sBuiltinResource::ALL {
                assert_ne!(
                    (id.api_version, id.kind),
                    (b.api_version(), b.kind()),
                    "KnownCrd {k:?} must not share a wire-form pair with K8sBuiltinResource {b:?}",
                );
            }
            for f in FluxResource::ALL {
                assert_ne!(
                    (id.api_version, id.kind),
                    (f.api_version(), f.kind()),
                    "KnownCrd {k:?} must not share a wire-form pair with FluxResource {f:?}",
                );
            }
            for r in RoutingEdgeResource::ALL {
                assert_ne!(
                    (id.api_version, id.kind),
                    (r.api_version(), r.kind()),
                    "KnownCrd {k:?} must not share a wire-form pair with RoutingEdgeResource {r:?}",
                );
            }
        }
    }

    #[test]
    fn wire_identity_composes_with_resource_json_bytewise_for_every_variant() {
        // End-to-end composition pin — every variant's
        // `.wire_identity().resource_json(<extras>)` composes into a
        // K8s resource JSON with the SAME `(apiVersion, kind)` slot
        // pair the pre-existing
        // [`tatara_process::PROCESS_WIRE_IDENTITY.resource_json(...)`]
        // call in `tatara-process::owner_reference_json` produces.
        // Sibling to
        // [`tatara_process::k8s_wire_identity::tests::resource_json_merges_identity_with_extras`]
        // on the composer axis; this pin binds the tatara-CRD-axis
        // variants against the SAME composer so a future emit consumer
        // (a `KnownCrd::EphemeralPool.wire_identity().resource_json(
        // json!({...}))` bind at a fleet-wide pool watcher) inherits
        // the composer mechanically.
        use serde_json::json;
        for k in KnownCrd::ALL {
            let body = k
                .wire_identity()
                .resource_json(json!({"metadata": {"name": "witness"}}));
            let obj = body.as_object().expect("resource_json emits an Object");
            assert_eq!(
                obj.get("apiVersion").and_then(|v| v.as_str()),
                Some(tatara_process::API_VERSION),
                "{k:?}: emitted apiVersion must route through tatara_process::API_VERSION",
            );
            assert_eq!(
                obj.get("kind").and_then(|v| v.as_str()),
                Some(k.as_str()),
                "{k:?}: emitted kind must route through KnownCrd::as_str",
            );
            assert_eq!(
                obj.get("metadata")
                    .and_then(|v| v.get("name"))
                    .and_then(|v| v.as_str()),
                Some("witness"),
                "{k:?}: caller-supplied extras must survive the identity merge",
            );
        }
    }

    #[test]
    fn emit_crd_yaml_round_trips_through_parse_yaml_as_for_every_variant() {
        // Substrate coherence pin: for every variant, the YAML the
        // emitter produces MUST parse as itself. The emit is a
        // `CustomResourceDefinition`, not a Process/PT/EP/EA
        // *instance*, so this pin binds the emit surface's shape at
        // fail-before-pass-after granularity — a regression that
        // corrupted the emitter (e.g. mixed up the crd-type arms)
        // would fail its OWN CRD-manifest parse via `serde_yaml`'s
        // structural check.
        for k in KnownCrd::ALL {
            let yaml = k.emit_crd_yaml().expect("emit succeeds");
            let v: serde_yaml::Value =
                serde_yaml::from_str(&yaml).expect("emitted YAML must be valid YAML");
            assert_eq!(
                v.get("kind").and_then(|v| v.as_str()),
                Some("CustomResourceDefinition"),
                "{:?}: emitted YAML's top-level kind must be CustomResourceDefinition",
                k
            );
        }
    }
}
